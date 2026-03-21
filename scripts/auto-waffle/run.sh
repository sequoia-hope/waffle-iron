#!/usr/bin/env bash
#
# auto-waffle — Autonomous kernel development loop
#
# Runs Claude Code in a loop, each iteration executing a full FIP cycle
# against crates/kernel/. See spec.md for design details.
#
# Uses claude-runner.py for headless execution with SIGINT-based
# graceful timeout handling.
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PROMPTS_DIR="$SCRIPT_DIR/prompts"
RUNNER="$SCRIPT_DIR/claude-runner.py"
DEFAULT_LOG_DIR="$SCRIPT_DIR/logs"

# Defaults
MAX_ITERATIONS=0        # 0 = unlimited
TIME_LIMIT_SECS=0       # 0 = unlimited
WORK_TIMEOUT_MINS=60
LOG_DIR="$DEFAULT_LOG_DIR"
REVIEW_MODE=false
REVIEW_RATIO_DEV=3      # dev passes per cycle
REVIEW_RATIO_REV=2      # review passes per cycle
PLAN_ONLY=false
DRY_RUN=false
VERBOSE=false
USE_WORKTREE=false

usage() {
    cat <<'USAGE'
Usage: ./scripts/auto-waffle/run.sh [OPTIONS]

Options:
  -n, --iterations N         Run N iterations then stop (default: unlimited)
  -t, --time-limit DURATION  Run for at most DURATION then stop (e.g., "4h", "30m")
  -w, --work-timeout MINS    Per-iteration timeout in minutes (default: 60)
  --review                   Run a single review pass instead of work loop
  --review-ratio D:R         Dev-to-review ratio per cycle (default: "3:2" = 3 dev, 2 review)
  --no-review                Disable automatic review passes
  --plan-only                Generate a plan for the next task and exit (no execution)
  --worktree                 Run each iteration in an isolated git worktree
  --dry-run                  Print prompts that would be sent without executing
  --log-dir DIR              Override log directory
  -v, --verbose              Stream claude output to terminal in addition to logs
  -h, --help                 Show this help
USAGE
}

# Parse duration string (e.g., "4h", "30m", "2h30m") to seconds
parse_duration() {
    local input="$1"
    local total=0

    # Extract hours
    if [[ "$input" =~ ([0-9]+)h ]]; then
        total=$(( total + ${BASH_REMATCH[1]} * 3600 ))
    fi
    # Extract minutes
    if [[ "$input" =~ ([0-9]+)m ]]; then
        total=$(( total + ${BASH_REMATCH[1]} * 60 ))
    fi
    # Plain number = minutes
    if [[ "$input" =~ ^[0-9]+$ ]]; then
        total=$(( input * 60 ))
    fi

    echo "$total"
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        -n|--iterations)
            MAX_ITERATIONS="$2"; shift 2 ;;
        -t|--time-limit)
            TIME_LIMIT_SECS="$(parse_duration "$2")"; shift 2 ;;
        -w|--work-timeout)
            WORK_TIMEOUT_MINS="$2"; shift 2 ;;
        --review)
            REVIEW_MODE=true; shift ;;
        --plan-only)
            PLAN_ONLY=true; shift ;;
        --review-ratio)
            IFS=':' read -r REVIEW_RATIO_DEV REVIEW_RATIO_REV <<< "$2"; shift 2 ;;
        --no-review)
            REVIEW_RATIO_DEV=0; REVIEW_RATIO_REV=0; shift ;;
        --worktree)
            USE_WORKTREE=true; shift ;;
        --dry-run)
            DRY_RUN=true; shift ;;
        --log-dir)
            LOG_DIR="$2"; shift 2 ;;
        -v|--verbose)
            VERBOSE=true; shift ;;
        -h|--help)
            usage; exit 0 ;;
        *)
            echo "Unknown option: $1" >&2; usage; exit 1 ;;
    esac
done

mkdir -p "$LOG_DIR"

ITERATION=1
LOOP_START=$(date +%s)

log() {
    echo "[auto-waffle] $(date '+%H:%M:%S') $*"
}

# Build the prompt from a template file
build_prompt() {
    local template="$1"
    cat "$template"
}

# Create a worktree for an iteration, returns path via WORKTREE_PATH
# Chains off the previous iteration's branch if available, otherwise HEAD.
setup_worktree() {
    local ts="$1"
    local branch_name="auto-waffle/${ts}"
    WORKTREE_PATH="/tmp/auto-waffle-${ts}"

    log "Creating worktree at $WORKTREE_PATH (branch: $branch_name)"
    cd "$REPO_ROOT"
    git worktree add "$WORKTREE_PATH" -b "$branch_name" HEAD 2>/dev/null
}

# Clean up a worktree. If it has commits ahead of where it branched,
# keep the branch for manual merge. Otherwise remove everything.
cleanup_worktree() {
    local worktree_path="$1"
    local ts="$2"
    local branch_name="auto-waffle/${ts}"

    if [[ ! -d "$worktree_path" ]]; then
        return
    fi

    # Check if the branch has commits beyond its branch point
    local commits_ahead
    commits_ahead=$(cd "$worktree_path" && git rev-list HEAD --not "$(git merge-base HEAD main)" --count 2>/dev/null || echo "0")

    cd "$REPO_ROOT"

    # Remove the worktree first (keeps the branch)
    git worktree remove "$worktree_path" 2>/dev/null || git worktree remove --force "$worktree_path" 2>/dev/null || true

    if [[ "$commits_ahead" -gt 0 ]]; then
        log "Merging $branch_name ($commits_ahead commit(s)) into main..."
        if git merge "$branch_name" -m "auto-waffle: merge $branch_name" 2>/dev/null; then
            log "Merged successfully"
            git branch -d "$branch_name" 2>/dev/null || true
        else
            # Conflict — take theirs for generated files, ours for everything else
            log "Merge conflict — auto-resolving (theirs for assay results)..."
            git checkout --theirs app/tests/cases/assay/results.json 2>/dev/null || true
            git add -A 2>/dev/null
            git commit --no-edit 2>/dev/null || true
            git branch -d "$branch_name" 2>/dev/null || true
        fi
    else
        log "Worktree has no new commits — cleaning up"
        git branch -D "$branch_name" 2>/dev/null || true
    fi
}

# Single review pass
do_review() {
    local ts
    ts=$(date '+%Y-%m-%dT%H-%M-%S')
    local iter_dir="$LOG_DIR/${ITERATION}-${ts}"
    mkdir -p "$iter_dir"

    log "Starting review pass (iteration $ITERATION)"

    if $DRY_RUN; then
        echo "=== REVIEW PROMPT ==="
        if [[ -f "$PROMPTS_DIR/review.md" ]]; then
            cat "$PROMPTS_DIR/review.md"
        else
            echo "(review.md not yet created)"
        fi
        return 0
    fi

    local work_root="$REPO_ROOT"
    if $USE_WORKTREE; then
        setup_worktree "$ts"
        work_root="$WORKTREE_PATH"
    fi

    local runner_args=(
        python3 "$RUNNER"
        --work-prompt "$PROMPTS_DIR/review.md"
        --timeout 30
        --output-dir "$iter_dir"
        --repo-root "$work_root"
    )
    $VERBOSE && runner_args+=(--verbose)

    "${runner_args[@]}" || true

    if $USE_WORKTREE; then
        cleanup_worktree "$WORKTREE_PATH" "$ts"
    fi

    # Write metadata
    cat > "$iter_dir/meta.json" <<EOF
{
    "iteration": $ITERATION,
    "type": "review",
    "started": "$ts",
    "ended": "$(date '+%Y-%m-%dT%H-%M-%S')",
    "outcome": "completed"
}
EOF

    log "Review pass complete. Logs in $iter_dir/"
}

# Single work iteration
do_work() {
    local ts
    ts=$(date '+%Y-%m-%dT%H-%M-%S')
    local iter_dir="$LOG_DIR/${ITERATION}-${ts}"
    mkdir -p "$iter_dir"

    log "Starting work iteration $ITERATION (timeout: ${WORK_TIMEOUT_MINS}m)"

    if $DRY_RUN; then
        echo "=== WORK PROMPT ==="
        cat "$PROMPTS_DIR/work.md"
        return 0
    fi

    local work_root="$REPO_ROOT"
    if $USE_WORKTREE; then
        setup_worktree "$ts"
        work_root="$WORKTREE_PATH"
    fi

    # Run via runner — handles timeout + SIGINT + follow-up prompts
    local runner_args=(
        python3 "$RUNNER"
        --work-prompt "$PROMPTS_DIR/work.md"
        --commit-prompt "$PROMPTS_DIR/commit.md"
        --cleanup-prompt "$PROMPTS_DIR/cleanup.md"
        --timeout "$WORK_TIMEOUT_MINS"
        --output-dir "$iter_dir"
        --repo-root "$work_root"
    )
    $VERBOSE && runner_args+=(--verbose)

    local exit_code=0
    "${runner_args[@]}" || exit_code=$?

    local outcome="completed"
    if [[ $exit_code -eq 2 ]]; then
        outcome="timeout"
    elif [[ $exit_code -ne 0 ]]; then
        outcome="error"
    fi

    # Extract commit hashes made during this iteration
    local commits
    commits=$(cd "$work_root" && git log --after="$ts" --format='"%H"' 2>/dev/null \
        | paste -sd',' || echo "")

    local branch_name=""
    if $USE_WORKTREE; then
        branch_name="auto-waffle/${ts}"
        cleanup_worktree "$WORKTREE_PATH" "$ts"
    fi

    # Write metadata
    cat > "$iter_dir/meta.json" <<EOF
{
    "iteration": $ITERATION,
    "type": "work",
    "started": "$ts",
    "ended": "$(date '+%Y-%m-%dT%H-%M-%S')",
    "outcome": "$outcome",
    "timeout_mins": $WORK_TIMEOUT_MINS,
    "branch": "$branch_name",
    "commits": [${commits}]
}
EOF

    log "Iteration $ITERATION complete ($outcome). Logs in $iter_dir/"
    if [[ -n "$branch_name" ]] && git rev-parse --verify "$branch_name" &>/dev/null; then
        log "  Branch: $branch_name (merge when ready)"
    fi
}

# --- Main Loop ---

log "auto-waffle starting"
log "  iterations: $([ "$MAX_ITERATIONS" -gt 0 ] && echo "$MAX_ITERATIONS" || echo "unlimited")"
log "  time limit: $([ "$TIME_LIMIT_SECS" -gt 0 ] && echo "${TIME_LIMIT_SECS}s" || echo "unlimited")"
log "  work timeout: ${WORK_TIMEOUT_MINS}m"
log "  log dir: $LOG_DIR"
log "  worktree: $USE_WORKTREE"
if [[ "$REVIEW_RATIO_DEV" -gt 0 && "$REVIEW_RATIO_REV" -gt 0 ]]; then
    log "  review ratio: ${REVIEW_RATIO_DEV}:${REVIEW_RATIO_REV} (${REVIEW_RATIO_DEV} dev, ${REVIEW_RATIO_REV} review per cycle)"
else
    log "  review: disabled"
fi

if $PLAN_ONLY; then
    ts=$(date '+%Y-%m-%dT%H-%M-%S')
    iter_dir="$LOG_DIR/${ITERATION}-${ts}"
    mkdir -p "$iter_dir"

    log "Running plan-only mode"

    # Concatenate work prompt + plan-only suffix
    plan_prompt="$(cat "$PROMPTS_DIR/work.md")
$(cat "$PROMPTS_DIR/plan-only-suffix.md")"

    if $DRY_RUN; then
        echo "=== PLAN-ONLY PROMPT ==="
        echo "$plan_prompt"
        exit 0
    fi

    # Write combined prompt to a temp file for the runner
    tmp_prompt=$(mktemp)
    echo "$plan_prompt" > "$tmp_prompt"

    local_root="$REPO_ROOT"
    if $USE_WORKTREE; then
        setup_worktree "$ts"
        local_root="$WORKTREE_PATH"
    fi

    runner_args=(
        python3 "$RUNNER"
        --work-prompt "$tmp_prompt"
        --timeout 10
        --output-dir "$iter_dir"
        --repo-root "$local_root"
    )
    $VERBOSE && runner_args+=(--verbose)

    "${runner_args[@]}" || true
    rm -f "$tmp_prompt"

    if $USE_WORKTREE; then
        cleanup_worktree "$WORKTREE_PATH" "$ts"
    fi

    if [[ -f "$iter_dir/plan.md" ]]; then
        log "Plan written to $iter_dir/plan.md"
        echo ""
        cat "$iter_dir/plan.md"
    else
        log "No plan file was generated. Check $iter_dir/output.log"
    fi
    exit 0
fi

if $REVIEW_MODE; then
    do_review
    exit 0
fi

while true; do
    # Check iteration limit
    if [[ "$MAX_ITERATIONS" -gt 0 && "$ITERATION" -gt "$MAX_ITERATIONS" ]]; then
        log "Reached iteration limit ($MAX_ITERATIONS). Stopping."
        break
    fi

    # Check time limit
    if [[ "$TIME_LIMIT_SECS" -gt 0 ]]; then
        elapsed=$(( $(date +%s) - LOOP_START ))
        if [[ $elapsed -ge $TIME_LIMIT_SECS ]]; then
            log "Reached time limit. Stopping."
            break
        fi

        # Check if there's enough time for another iteration
        remaining=$(( TIME_LIMIT_SECS - elapsed ))
        if [[ $remaining -lt $(( WORK_TIMEOUT_MINS * 60 )) ]]; then
            log "Not enough time for another full iteration (${remaining}s remaining). Stopping."
            break
        fi
    fi

    # Check if this iteration should be a review pass
    # With ratio D:R, in a cycle of (D+R) iterations, the last R are reviews
    CYCLE_LEN=$(( REVIEW_RATIO_DEV + REVIEW_RATIO_REV ))
    if [[ "$CYCLE_LEN" -gt 0 && "$REVIEW_RATIO_REV" -gt 0 ]]; then
        POS_IN_CYCLE=$(( (ITERATION - 1) % CYCLE_LEN ))
        if [[ "$POS_IN_CYCLE" -ge "$REVIEW_RATIO_DEV" ]]; then
            do_review
        else
            do_work
        fi
    else
        do_work
    fi

    ITERATION=$(( ITERATION + 1 ))

    # Brief pause between iterations to let git settle
    sleep 5
done

log "auto-waffle finished after $((ITERATION - 1)) iterations"
