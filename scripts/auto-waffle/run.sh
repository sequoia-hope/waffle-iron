#!/usr/bin/env bash
#
# auto-waffle — lean reactive-driver worker runner
#
# Runs driver-authored custom prompts as plan-mode Claude Code worker sessions.
# There is NO generic task-discovery prompt and NO timeout-recovery prompt: the
# *driver* (an interactive Claude session working through the roadmap) authors a
# context-rich prompt per increment, this script runs it as one plan-mode worker
# (plan -> role-separated FIP -> commit -> push), and the driver inspects the
# result to author the next prompt. See spec.md.
#
# Usage:
#   ./run.sh --prompt-file PATH            # run ONE custom prompt as a plan-mode worker
#   ./run.sh --queue DIR                   # drain DIR/*.md in lexical order, one worker each
#   ./run.sh --prompt-file PATH --dry-run  # print the assembled worker prompt, don't execute
#
# Options:
#   -p, --prompt-file PATH   Driver-authored custom prompt for one increment
#   -q, --queue DIR          Directory of *.md prompts to run in lexical order
#   -w, --work-timeout MINS  Per-worker hang-backstop timeout (default: 90)
#       --log-dir DIR        Override log directory (default: scripts/auto-waffle/logs)
#       --dry-run            Print the assembled prompt(s) and exit (no execution)
#   -v, --verbose            Stream worker output to the terminal in addition to logs
#   -h, --help               Show this help

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RUNNER="$SCRIPT_DIR/claude-runner.py"
SUFFIX_FILE="$SCRIPT_DIR/prompts/worker-suffix.md"
DEFAULT_LOG_DIR="$SCRIPT_DIR/logs"

PROMPT_FILE=""
QUEUE_DIR=""
WORK_TIMEOUT_MINS=90
LOG_DIR="$DEFAULT_LOG_DIR"
DRY_RUN=false
VERBOSE=false

usage() {
    cat <<'USAGE'
Usage:
  ./run.sh --prompt-file PATH            Run ONE custom prompt as a plan-mode worker
  ./run.sh --queue DIR                   Drain DIR/*.md in lexical order, one worker each
  ./run.sh --prompt-file PATH --dry-run  Print the assembled worker prompt, don't execute

Options:
  -p, --prompt-file PATH   Driver-authored custom prompt for one increment
  -q, --queue DIR          Directory of *.md prompts to run in lexical order
  -w, --work-timeout MINS  Per-worker hang-backstop timeout (default: 90)
      --log-dir DIR        Override log directory
      --dry-run            Print the assembled prompt(s) and exit (no execution)
  -v, --verbose            Stream worker output to the terminal in addition to logs
  -h, --help               Show this help
USAGE
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -p|--prompt-file)  PROMPT_FILE="$2"; shift 2 ;;
        -q|--queue)        QUEUE_DIR="$2"; shift 2 ;;
        -w|--work-timeout) WORK_TIMEOUT_MINS="$2"; shift 2 ;;
        --log-dir)         LOG_DIR="$2"; shift 2 ;;
        --dry-run)         DRY_RUN=true; shift ;;
        -v|--verbose)      VERBOSE=true; shift ;;
        -h|--help)         usage; exit 0 ;;
        *) echo "Unknown option: $1" >&2; usage; exit 1 ;;
    esac
done

if [[ -z "$PROMPT_FILE" && -z "$QUEUE_DIR" ]]; then
    echo "error: one of --prompt-file or --queue is required" >&2; usage; exit 1
fi

mkdir -p "$LOG_DIR"
log() { echo "[auto-waffle] $(date '+%H:%M:%S') $*"; }

# Run ONE worker for the given custom prompt file.
run_one() {
    local prompt_file="$1"
    if [[ ! -f "$prompt_file" ]]; then
        log "prompt file not found: $prompt_file"; return 1
    fi
    local ts; ts=$(date '+%Y-%m-%dT%H-%M-%S')
    local iter_dir="$LOG_DIR/${ts}-$(basename "$prompt_file" .md)"
    mkdir -p "$iter_dir"

    if $DRY_RUN; then
        echo "=== ASSEMBLED WORKER PROMPT ($prompt_file) ==="
        cat "$prompt_file"
        [[ -f "$SUFFIX_FILE" ]] && cat "$SUFFIX_FILE"
        echo "=== (dry run -- not executed) ==="
        return 0
    fi

    log "Worker: $(basename "$prompt_file")  (timeout ${WORK_TIMEOUT_MINS}m, logs $iter_dir)"
    local runner_args=(
        python3 "$RUNNER"
        --prompt-file "$prompt_file"
        --suffix-file "$SUFFIX_FILE"
        --timeout "$WORK_TIMEOUT_MINS"
        --output-dir "$iter_dir"
        --repo-root "$REPO_ROOT"
    )
    $VERBOSE && runner_args+=(--verbose)

    local exit_code=0
    "${runner_args[@]}" || exit_code=$?

    local outcome="completed"
    [[ $exit_code -eq 2 ]] && outcome="timeout"
    [[ $exit_code -ne 0 && $exit_code -ne 2 ]] && outcome="error"

    local commits
    commits=$(cd "$REPO_ROOT" && git log --since="$ts" --format='"%H"' 2>/dev/null | paste -sd',' || echo "")
    cat > "$iter_dir/meta.json" <<EOF
{
    "prompt": "$(basename "$prompt_file")",
    "started": "$ts",
    "ended": "$(date '+%Y-%m-%dT%H-%M-%S')",
    "outcome": "$outcome",
    "timeout_mins": $WORK_TIMEOUT_MINS,
    "commits": [${commits}]
}
EOF
    log "Worker done ($outcome). Logs in $iter_dir/"
    return "$exit_code"
}

if [[ -n "$PROMPT_FILE" ]]; then
    run_one "$PROMPT_FILE"
    exit $?
fi

# --queue mode: drain *.md in lexical order, one worker each.
shopt -s nullglob
prompts=("$QUEUE_DIR"/*.md)
shopt -u nullglob
if [[ ${#prompts[@]} -eq 0 ]]; then
    log "No *.md prompts in $QUEUE_DIR"; exit 0
fi
log "Queue: ${#prompts[@]} prompt(s) from $QUEUE_DIR"
for p in "${prompts[@]}"; do
    run_one "$p" || log "Worker for $(basename "$p") exited non-zero -- continuing."
    $DRY_RUN || sleep 5
done
log "Queue drained."
