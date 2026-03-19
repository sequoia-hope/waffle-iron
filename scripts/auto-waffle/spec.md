# auto-waffle — Autonomous Kernel Development Loop

## Purpose

auto-waffle is a headless loop that continuously develops the Waffle Iron
modeling kernel (`crates/kernel/`) by running Claude Code sessions that follow
the project's governance model (Engineering Constitution, FIP, DoD,
Architectural Invariants). It exists because the kernel is a well-specified
slog — the work is decomposable, the governance is strict, and a model that
follows the rules produces correct, reviewable commits.

## Design Principles

1. **Governance is the guardrail.** auto-waffle does not invent its own quality
   checks. It relies on the existing governance documents to define what "good
   work" looks like and asks Claude to follow them.
2. **Honest self-assessment over mechanical checks.** The cleanup/commit
   decision is made by Claude reading the governance model and judging its own
   work, not by a script checking exit codes. This works because the governance
   is strict enough to be unambiguous, and asking Claude to re-read it after
   losing context is effective at restoring lucidity.
3. **One loop, one kernel.** No parallelism. Each iteration completes or is
   cleaned up before the next begins. Continuous, not concurrent.
4. **Full FIP per iteration.** Each cycle is a complete Feature Implementation
   Protocol pass (spec → test → implement → validate). Safer and slower by
   design.

## Architecture

```
┌──────────────────────────────────────────────────┐
│                   run.sh (driver)                │
│                                                  │
│  ┌──────────┐    ┌──────────┐    ┌───────────┐  │
│  │  Work     │───▶│ Timeout  │───▶│ Cleanup   │  │
│  │  Prompt   │    │ Monitor  │    │ Prompt    │  │
│  └──────────┘    └──────────┘    └───────────┘  │
│       │               │               │          │
│       ▼               ▼               ▼          │
│  ┌──────────────────────────────────────────┐   │
│  │           logs/<timestamp>-*             │   │
│  │  plan.md | output.log | cleanup.md      │   │
│  └──────────────────────────────────────────┘   │
└──────────────────────────────────────────────────┘
```

### Components

- **`run.sh`** — The loop driver. Parses arguments, manages iterations,
  enforces timeouts, logs everything.
- **`prompts/work.md`** — The static work prompt given to each Claude session.
- **`prompts/cleanup.md`** — The cleanup prompt given when a session is
  interrupted by timeout.
- **`prompts/commit.md`** — The commit prompt given when a session completes
  normally within the time limit.
- **`prompts/review.md`** — The review-only prompt for review passes.
- **`logs/`** — Timestamped directories, one per iteration, containing plans,
  output, and cleanup/commit responses.

## Loop Lifecycle

### Normal Completion (< 1 hour)

```
1. run.sh invokes: claude -p <work-prompt> --dangerously-skip-permissions
2. Claude reads governance docs, picks highest-priority kernel task from PLAN.md
3. Claude writes plan to logs/<ts>-plan.md
4. Claude uses agent teams for FIP role separation:
   - Teammate: spec writer (writes /specs/<feature>.md)
   - Teammate: test author (writes failing tests)
   - Teammate: implementer (makes tests pass)
   - Teammate: adversary (edge cases, hardening)
   - Lead coordinates, enforces FIP phases
5. Claude completes within 1 hour
6. run.sh captures session ID from output
7. run.sh invokes: claude -p <commit-prompt> --resume <session-id>
8. Claude runs cargo test/clippy/fmt, reviews against governance, commits or
   reports what's incomplete
9. Commit prompt response logged to logs/<ts>-commit.md
10. Loop restarts
```

### Timeout (>= 1 hour)

```
1-4. Same as above
5. 1 hour elapses, run.sh sends SIGTERM to claude process
6. run.sh invokes: claude -p <cleanup-prompt> --resume <session-id>
7. Claude re-reads governance docs, reviews all uncommitted changes
8. For each changed file, Claude decides:
   - Commit: changes follow governance, tests pass, work is complete
   - Revert: changes are incomplete, violate governance, or break tests
9. Claude commits good work, reverts the rest, updates PLAN.md
10. Cleanup response logged to logs/<ts>-cleanup.md
11. Loop restarts
```

### Review Pass

A review pass is a non-implementation iteration. Instead of the work prompt,
Claude receives the review prompt, which asks it to:

- Read all governance documents
- Read the kernel source and recent git history
- Assess compliance with Constitution, FIP, DoD, Architectural Invariants
- Check that specs match implementations
- Check that tests are meaningful (not hack-to-green)
- Report findings to `logs/<ts>-review.md`
- Optionally fix issues it finds (with commits)

Review passes can be triggered explicitly (`--review`) or scheduled
(e.g., every N iterations).

## CLI Interface

```
Usage: ./scripts/auto-waffle/run.sh [OPTIONS]

Options:
  -n, --iterations N     Run N iterations then stop (default: unlimited)
  -t, --time-limit DURATION  Run for at most DURATION then stop (e.g., "4h", "30m")
  -w, --work-timeout MINS    Per-iteration timeout in minutes (default: 60)
  --review               Run a single review pass instead of work loop
  --review-every N       Run a review pass every N work iterations
  --dry-run              Print prompts that would be sent without executing
  --log-dir DIR          Override log directory (default: scripts/auto-waffle/logs)
  --continue             Resume from last iteration (skip completed work)
  -v, --verbose          Stream claude output to terminal in addition to logs
```

## Prompt Design

### Work Prompt (prompts/work.md)

The work prompt is static and generic. It does not name a specific task.
Instead, it instructs Claude to:

1. Read the governance documents (Constitution, FIP, DoD, Architectural
   Invariants)
2. Read the kernel's PLAN.md and ARCHITECTURE.md
3. Read INTERFACES.md for type contracts
4. Pick the highest-priority uncompleted task
5. Write a plan to a specified log path before starting
6. Use agent teams to maintain FIP role separation (P5):
   - Spawn teammates for distinct roles
   - Lead acts as Manager (never writes modeling code directly)
   - Test Author teammate ≠ Implementer teammate
7. Execute the full FIP cycle (spec → test → implement → validate)
8. Run `cargo test -p kernel && cargo clippy -p kernel` before declaring done
9. Update PLAN.md to reflect completed/discovered work

The prompt also includes key reminders:
- Analytical Primacy (A15) — exact SSI, no mesh fallbacks for quadrics
- P9–P10 — no hack-to-green, abort if diagnosis is wrong
- P8 — cite research references
- Surface type taxonomy — never downgrade tiers

### Cleanup Prompt (prompts/cleanup.md)

Given to a resumed session after timeout. Instructs Claude to:

1. Re-read governance documents (restores lucidity after context loss)
2. Run `git diff` and `git status` to see all uncommitted changes
3. Run `cargo test -p kernel` and `cargo clippy -p kernel`
4. For each logical unit of change, assess:
   - Does it follow the Engineering Constitution?
   - Does it have a spec? Does the spec match?
   - Are tests meaningful and passing?
   - Does it maintain architectural invariants?
5. Commit changes that pass governance review (with clear commit messages)
6. Revert changes that don't (with explanation of why)
7. Update PLAN.md: mark completed tasks, note partial progress, add blockers
8. Write a summary of decisions to the log path

### Commit Prompt (prompts/commit.md)

Given to a resumed session after normal completion. Lighter than cleanup:

1. Run `cargo test -p kernel && cargo clippy -p kernel && cargo fmt -p kernel -- --check`
2. If all pass: commit all changes with descriptive message, update PLAN.md
3. If any fail: assess what's broken, commit passing portions, revert broken
   portions, document in PLAN.md
4. Push to remote

### Review Prompt (prompts/review.md)

Detailed prompt TBD — will be workshopped after the basic system works. Core
idea: read governance + kernel source + recent history, assess compliance,
report findings, optionally fix issues.

## Logging

Each iteration creates a timestamped directory:

```
logs/
  2026-03-19T14-30-00/
    plan.md          — The plan Claude wrote before executing
    output.log       — Full claude session output
    commit.md        — Commit prompt response (normal completion)
    cleanup.md       — Cleanup prompt response (timeout)
    review.md        — Review pass response (review mode)
    meta.json        — Iteration metadata:
                       { iteration: 3,
                         started: "2026-03-19T14:30:00Z",
                         ended: "2026-03-19T15:12:34Z",
                         outcome: "completed|timeout|error",
                         commits: ["abc1234"],
                         task: "SSI solver: plane-torus" }
```

## Session Management

### Session Continuity

The work prompt runs as a fresh session (`claude -p`). When the session
completes or times out, the commit/cleanup prompt resumes the same session
(`claude -p --resume <session-id>`). This preserves context — the cleanup
agent can see what the work agent was doing.

### Session ID Capture

`claude -p --output-format json` returns a JSON object containing
`session_id`. The driver script captures this for `--resume`.

### Timeout Implementation

```bash
timeout --signal=TERM "${WORK_TIMEOUT}m" claude -p "..." --output-format json
exit_code=$?
if [ $exit_code -eq 124 ]; then
    # Timed out — run cleanup prompt with --resume
fi
```

The `timeout` command sends SIGTERM, which Claude Code handles gracefully
(completes current tool call, then exits). The session is persisted to disk
automatically, allowing `--resume`.

## Safety

- **No force pushes.** Prompts explicitly forbid `--force`.
- **No destructive git ops.** Prompts forbid `reset --hard`, `checkout .`,
  `clean -f`, `branch -D`.
- **Governance is re-read on cleanup.** Even if the work session lost context,
  the cleanup prompt starts by re-reading governance, which is effective at
  restoring correct judgment.
- **Atomic commits.** Each commit represents a complete, governance-compliant
  unit of work. Partial work is reverted, not committed.
- **PLAN.md is the record.** Every iteration updates PLAN.md, so the next
  iteration (and any human reviewer) knows what happened.
- **All output is logged.** Full session output is captured. Nothing is lost.

## Limitations

- **Agent teams are experimental.** The teammate spawning mechanism may have
  rough edges. If teams fail, the work prompt should degrade gracefully to
  single-agent FIP (with a note in the log that P5 role separation was
  compromised).
- **Context window pressure.** Full FIP cycles on complex tasks (e.g., torus-
  cylinder SSI) may exhaust the context window before completion. The 1-hour
  timeout is a backstop, but the cleanup prompt may also face context pressure
  on long sessions.
- **No human in the loop.** auto-waffle trusts Claude's governance self-
  assessment. Periodic human review of logs is expected.
- **Single-model limitation.** All roles are played by the same model family.
  The governance model was designed for this (P5 says "same agent role" not
  "same model"), but it's worth noting.

## Future Work

- **Review pass prompt** — to be workshopped after the basic loop works
- **Assay score tracking** — log assay score per iteration, plot progress
- **Notification hooks** — notify on completion, timeout, or error
- **Cost tracking** — log token usage per iteration via `--output-format json`
- **Adaptive timeout** — shorter timeout for simpler tasks, longer for complex
- **Multi-crate expansion** — extend beyond kernel to other crates once proven
