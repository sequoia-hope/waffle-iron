# Waffle Iron — Guide for Antigravity / Gemini sessions

You are sharing this repo with Claude Code sessions, which carry long-lived
memory you cannot see. This file is the part of that memory you need. It is
short on purpose; the authority is elsewhere.

## 1. Read these first (they are NOT auto-loaded for you)

1. **`CLAUDE.md`** — the project's session guide. Despite the name, it applies
   to every agent. It is ~27 KB (over your 24 KB rule cap), so read it with
   your file reader at session start. Everything below assumes you have.
2. `AGENTS.md` (auto-loaded) — directory ownership and document precedence.
3. For any `yang-rs` / `kernel-v2` boolean change, additionally:
   - `docs/yang_faithfulness_roadmap.md` — the STANDING PRIORITY (from
     2026-10-27): milestones F0–F7 in order; take the first unchecked item
     of the earliest open milestone. Tail cases are customers of a
     milestone, never work items of their own.
   - `specs/yang_faithful_cleanup.md` — the substitution table and method
     the roadmap's milestones follow. Read it BEFORE touching yang-rs.
   - The rows of `docs/audits/yang_2025_audit.md` for the paper section you
     are changing.
   - The paper section itself: run `./scripts/extract-papers.sh`, then read
     `refs/text/yang2025_hybrid_boolean.txt` and cite line numbers.

Precedence when rules conflict: `governance/*` > `agents/*` > `AGENTS.md` >
`CLAUDE.md` > this file.

## 2. The one principle for kernel work

The user's ruling (governance A15.7): *"ad hoc patches will destroy a kernel
like this. the fixes must be principled. the principles are the paper."*

- **The paper is the spec.** A change to Yang pipeline code implements a
  mechanism Yang 2025 describes, at the section it describes it. Name the
  section (and `.txt` line range) in the code comment AND the commit message.
- **Do not add a new arm, band, ranking, clearance heuristic or tolerance** to
  make a case pass. The cleanup plan exists to RETIRE those. If a case fails
  because a paper mechanism is missing, build the mechanism (multi-commit is
  fine; see CLAUDE.md "Structural Fixes First") or record the gap and stop.
  A band is legitimate ONLY to turn a silent-wrong into a loud STOP.
- **A deviation found mid-investigation halts the cycle.** The deviation is the
  bug; record it in `docs/yang_deviations.md`, do not debug its downstream
  symptoms.
- **If you can't explain why it fails, don't change code to make it pass**
  (P9/P10). Write what you learned in `docs/yang_tail_triage.md` instead.
- **Never enumerate surface types / curve maps inside a general geometric rule**
  (e.g. "if plane-plane-torus then …"). The paper's rules are by incidence and
  by `surface_normal_at`, uniform over surface types.

### Anchor before you fix

- Work ONE tail case at a time, cheapest first (CLAUDE.md lists the open tail).
- Before designing a fix, anchor the case against its un-minimized lineage:
  `target/prospect/seed-{4,5}/candidates/` and `findings/`, or
  `WAFFLE_PATH=… cargo test --release -p test-harness --test user_case_probe replay_waffle_env`.
  The minimizer's rounding can MINT a contact that the real case never had.
  Neighbouring case IDs (P0061 / P0062) are NOT lineage of each other.
- Census the defect's OWN quantity on the case before calling it a mechanism.
  Several past "this is Fig. 12" readings were wrong; check before claiming.
- Never record a number you did not compute in this session.

## 3. Commits — the same-commit rule

Every kernel commit carries, in the SAME commit:
- `docs/yang_deviations.md` (new or amended ledger entry) — or a
  "faithful, no deviation" line in `docs/yang_tail_triage.md`,
- the touched rows of `docs/audits/yang_2025_audit.md`,
- the rebuilt WASM bundle (`./scripts/build-wasm.sh`, then `git add app/static/pkg/`).
  `git add` any NEW source files BEFORE building, or the fingerprint certifies
  a stale bundle.

The pre-commit hook enforces the ledger and WASM checks. **Do not use
`SKIP_LEDGER_CHECK=1` / `SKIP_WASM_CHECK=1` to get past it** — those are for a
mechanism-free refactor or a docs-only follow-up, nothing else.

Ledger language: if the user says "no opinion" / "I don't care" about a
deviation, record it as `DELEGATED` (open debt), never `PERMANENT`. Ask for
sign-off in prose with a paper cite, not as a multiple-choice menu. A fix you
proved SAFE is not thereby FAITHFUL.

Before committing: `cargo test -p <crate>`,
`cargo clippy --all-targets -p <crate> -- -D warnings` (`--all-targets` is
required; CI lints tests), `cargo fmt --check -p <crate>`.

Workflow: commit directly to `main`, then **`git push`** after each commit
(the user expects origin to be current). Don't commit the stray root files
(`*.waffle.json`, `*.step`, `pendant/`, a file named `1`) — those are the
user's scratch; leave them alone.

## 4. Running things — traps that have bitten before

- **Assays run in `--release`.** Debug builds are ~10–50× slower, so a debug
  `single_case` "TIMEOUT" means nothing.
  `ASSAY_CASE=P0045 cargo test --release -p test-harness --test assay_kv2 single_case -- --ignored --exact --nocapture`
- **Never pipe cargo into `grep | head`.** `head` closing the pipe truncates
  the suite and can hang cargo. Write to a log file, then grep the file:
  `cargo test … > target/agy-logs/run.log 2>&1; grep -E '^test result' target/agy-logs/run.log`.
  Wait for `^test result` before reading a verdict.
- **Full corpus** (~13 min wall, run in background, log to a file):
  `ASSAY_JOBS=8 ASSAY_CASE_TIMEOUT_SECS=900 cargo test -p test-harness --test assay_kv2 --release full_corpus_categorized -- --ignored --nocapture`.
  Report the five buckets (C / W / E / EE / T) over the case count, compared with the
  score in CLAUDE.md. It OVERWRITES `app/tests/cases/assay/results.json`.
  Re-run any lone TIMEOUT as `single_case` before believing it.
- **Do not run cargo in the main tree while a full corpus or `prospect_*` run
  is going** — it rebuilds the binary under the running job. Use a private
  target dir (`CARGO_TARGET_DIR=target/wt-agy`) for side work.
- **Timing:** `YANG_STAGE_TIME_PROBE=1` / `CHERCHI_TIME_PROBE=1` only make
  sense with `ASSAY_JOBS=1`. Time before designing for cost.
- **Probe env vars are documented** in `docs/kernel_probe_registry.md`. Don't
  invent tuning knobs (`KV2_…_EDGES=16`) to make a case pass; a knob that
  changes the answer is a deviation.
- Killing `cargo test` can leave the test binary burning a core — check
  `pgrep -a assay_kv2` and kill by PID. **Never `pkill -f`** (it matches your
  own shell).
- Test tiers: `./scripts/test.sh rewrite` after any low-stack (yang-rs,
  cherchi-rs, kernel-v2) change before you call it done.

## 5. Machine rules

- No ad-hoc servers (`python3 -m http.server 8000`) and no hardcoded ports.
  Projects are served via the host's `proj` registry (`proj up <name>` /
  `./serve.sh`); the app dev server is already managed — keep it running,
  don't restart it on a new port.
- Don't publish anything externally (pastebins, gists, hosted pages) to show
  results; write HTML/images into the project and point the user at them.

## 6. Hand-off

Claude sessions resume your work cold. At a stopping point (or when your
context is getting long), make sure the next agent can tell what you were doing:
- Commit finished, verified work (rules above).
- For unfinished work, leave a dated entry in `docs/yang_tail_triage.md` under
  the case: hypothesis, the paper section, what you measured (with the exact
  command), what is uncommitted and why. Don't leave a dirty tree with no
  explanation.
- Tell the user plainly what is verified, what is not, and what you skipped.
