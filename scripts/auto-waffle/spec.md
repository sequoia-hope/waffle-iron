# auto-waffle — Reactive-Driver Worker Loop

## Purpose

auto-waffle runs **driver-authored** Claude Code worker sessions that advance the
Waffle Iron kernel roadmap under the project's governance model (Engineering
Constitution, FIP, DoD, Architectural Invariants).

It is **not** an autonomous task-discovery loop. The intelligence lives in the
**driver** — an interactive Claude session working through
`docs/yang_functional_roadmap.md`. The driver decides the next increment, authors
a context-rich prompt for it (the specific task + verified math + scope decision),
and hands that prompt to a thin **worker** runner. The worker plans and executes
it; the driver inspects the result and authors the next prompt.

This replaces the older design (a generic "read PLAN.md, pick a task" discovery
prompt + a timeout-recovery/cleanup prompt). Those existed to keep a weaker model
on the rails; they are no longer needed.

## Roles

```
   ┌─────────────────────────────────────────────────────────────┐
   │ DRIVER  (interactive Claude, working the roadmap)            │
   │   reads roadmap + git log + last worker's result            │
   │   authors the next increment's custom prompt  ──────────┐   │
   │   inspects each worker result, reconciles, decides next │   │
   └─────────────────────────────────────────────────────────┼───┘
                                                              │
                            custom prompt (one increment)     ▼
   ┌─────────────────────────────────────────────────────────────┐
   │ WORKER  (run.sh -> claude-runner.py, headless plan mode)    │
   │   prompt = <custom prompt> + prompts/worker-suffix.md       │
   │   plan mode: auto-create + auto-approve FIP plan,           │
   │   role-separated cycle (spec -> RED -> GREEN -> adversary), │
   │   commit each phase, push at end. No discovery. No recovery.│
   └─────────────────────────────────────────────────────────────┘
```

The **worker suffix** (`prompts/worker-suffix.md`) carries the standing phrases
("Go into plan mode. Follow governance docs.") plus the cross-cutting operating
rules learned from real cycles: role-separated TDD with distinct sub-agents, stay
on `main` (reconcile stray branches), commit-each-phase + push, faithful
contract-migration when a change obsoletes prior test expectations, no
hack-to-green / STOP-and-report on genuine conflicts, and a clean CI gate before
done. This lets each per-increment custom prompt stay focused on task specifics.

## Components

- **`run.sh`** — thin executor. Runs one custom prompt (`--prompt-file`) or drains
  a queue (`--queue DIR`) as plan-mode workers. Logs each run; writes `meta.json`
  (prompt, outcome, commit hashes). No loop intelligence — that's the driver.
- **`claude-runner.py`** — runs the worker as **two turns of one session**:
  Phase A in plan mode (`--permission-mode plan --dangerously-skip-permissions`),
  where it writes + presents its plan and the `-p` turn ends; then Phase B resumes
  the *same* session (`--resume`, plan mode off) with a fixed "your plan is
  approved — execute it" prompt, releasing it to run the full FIP cycle, commit,
  and push. (Empirically, headless plan mode on a real task presents-and-ends; the
  execute-resume is load-bearing, not optional — see Limitations.) Assembles
  `prompt-file + suffix-file`, persists the exact prompt to `prompt.md`. No
  commit/cleanup *recovery* passes.
- **`prompts/worker-suffix.md`** — the standing suffix appended to every worker
  prompt.
- **`prompts/{review,yang-review}.md`** — legacy standalone audit prompts; not
  wired into the loop, but can be driven manually via `--prompt-file`.
- **`logs/<ts>-<prompt>/`** — per-worker: `prompt.md` (exact assembled prompt),
  `output.log` (full session stream), `plan.md` (the FIP plan the worker wrote),
  `meta.json`.

## Usage

```
# One increment (the driver writes /tmp/pr-ssi7.md, then):
./scripts/auto-waffle/run.sh --prompt-file /tmp/pr-ssi7.md

# Preview the assembled worker prompt without executing:
./scripts/auto-waffle/run.sh --prompt-file /tmp/pr-ssi7.md --dry-run

# Drain a pre-authored queue of increments (Option A, less reactive):
./scripts/auto-waffle/run.sh --queue scripts/auto-waffle/queue

# Options: -w/--work-timeout MINS (default 90, hang backstop only),
#          --log-dir DIR, -v/--verbose
```

### Driving via `/loop`

The intended mode is the driver (an interactive Claude session) running `/loop`:
read the roadmap, author the next increment's prompt, invoke `run.sh
--prompt-file`, read the worker's result + adversary report, author the next, and
self-pace. The driver is where between-phase judgment lives — reconciling stray
branches, migrating obsoleted tests, making scope-fork calls — the work a bare
worker cannot do and correctly stops on.

## Worker lifecycle (one increment)

```
1. run.sh assembles: <custom prompt> + worker-suffix.md  -> prompt.md
2. Phase A: claude -p --permission-mode plan ... <prompt>
   -> worker enters plan mode, writes its FIP plan, presents it, turn ends
3. Phase B: claude -p --resume <session> ... "<plan approved — execute>"
   -> same session, plan mode off, runs the role-separated cycle with distinct
      sub-agents: spec (Manager) -> RED tests -> GREEN impl -> adversary
4. Worker commits each phase and pushes origin/main
5. run.sh records meta.json; the driver inspects logs + git and authors the next
```

## Safety

- Plan mode + skip-permissions: the worker auto-approves its own plan and runs
  unattended within one session. The governance docs (re-read each session) are
  the guardrail.
- No force pushes / destructive git ops (forbidden by governance + the suffix).
- Commit-per-phase + push: each phase is an atomic, governance-compliant unit; the
  driver can revert/redirect between increments if a worker goes wrong.
- The driver is the human-adjacent reviewer in the loop: every worker result is
  inspected before the next prompt is authored.

## Removed vs the old design

- **Removed:** generic discovery prompt (`work.md`/`plan.md`), timeout-recovery
  prompt (`cleanup.md`), the two-phase plan/execute split, the commit follow-up
  (`commit.md`), and the review-ratio scheduler. Recovery existed to catch an older
  model going off the rails; the reactive driver replaces it.
- **Kept:** plan mode, headless streaming + session capture, per-iteration logging,
  the worktree-free `main`-linear workflow.

## Limitations

- **Headless plan mode presents-and-ends.** A real run (PR-SSI7) showed that
  `claude -p --permission-mode plan` on a substantive task writes + presents its
  plan via ExitPlanMode and then *ends the turn awaiting approval* — it does NOT
  auto-continue into execution. (A trivial smoke barreled through and gave a false
  positive.) Hence the Phase-B execute-resume is required. Re-verify this behavior
  if the CLI version changes.
- **Single model family** plays all roles (governance P5 is "same role," not "same
  model" — designed for this).
- **The driver must stay in the loop** for between-phase reconciliation; a bare
  queue (`--queue`) trades that reactivity for unattended throughput.
