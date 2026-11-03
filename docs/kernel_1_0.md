# Kernel 1.0 — the definition and the way there

**Adopted 2026-11-03** (user: *"keep B-spline in 1.0, write the doc and start
step 0"*). Precedence: governance A15.7 → **this file (what 1.0 means)** →
`docs/yang_faithfulness_roadmap.md` (the F0–F7 sequence, which is gate 2) →
`docs/review/remediation_2026-10-28.md` (the oracle and robustness items gates
1 and 4 cite) → the per-mechanism specs.

## 0. Why a 1.0

The faithfulness roadmap says when the code matches the paper. It does not
say when the kernel is good enough to stop adding capability and start on
cost. 1.0 is that point. **No optimization work starts before 1.0**, except
result-neutral complexity fixes that a gate needs (an O(n²) pass that makes
a case TIMEOUT). After 1.0, each speedup is a recorded deviation measured
against the 1.0 loop (N67 amendment).

1.0 is reached when all four gates hold on the same commit. Every gate is a
number a script prints; none is a judgement made at the end.

## 1. The four gates

### Gate 1 — Correct

1. **WRONG = 0** on the canonical corpus and on **two consecutive fresh
   prospector seeds**, with every harness oracle in force as a verdict.
2. **No record-only oracles.** O2 (exact-versus-mesh volume) and O5 (volume
   monotonicity per op) are promoted from record-only to verdicts, with their
   current outliers adjudicated first (O2: F0054, R0042, and the 148
   unmeasurable cases — an integrator coverage gap; O5: the 35 R-series
   no-change steps).
3. **The silent-wrong census is closed:** every N100 item is either fixed,
   made loud with a typed error and a pin, or shown unreachable with a test.

### Gate 2 — Faithful

1. The roadmap's finish line (`docs/yang_faithfulness_roadmap.md` §1): ledger
   debt 0, no MISSING or ORDER matrix row, every cleanup §2 substitute
   retired or re-graded as the paper's own operation, corpus held.
2. **The audit is closed:** two consecutive independent re-reads of paper
   against code (§3 below), each by an agent that did not write the code
   under review, add no new not-faithful row. The 2026-10-27 re-read moved
   the count 100 → 139; a finish line whose denominator still moves is not
   a finish line.
3. Where the paper is silent, the choice is a SILENT row with a recorded
   rationale, signed in prose by the user (A15.7) — never DELEGATED.

### Gate 3 — Capable

The supported envelope, each item with a corpus or pinned customer:

1. **Every surface pair.** The 21 pairs of {plane, cylinder, cone, sphere,
   torus, B-spline} each have at least one general-position boolean customer
   (union, cut, intersect) that categorizes CORRECT. Built as a pair-matrix
   test in `crates/test-harness/` (new).
2. **The generic parametric-surface interface and a B-spline instance end to
   end** (roadmap F5; resolves D14). A B-spline face enters Stage 1, meets an
   analytic face in a boolean, and leaves as an exact B-Rep face.
3. **Profiles:** holed revolve profiles and arc/spline profiles without a
   chord polygon are supported, not `NotSupported`.
4. **Coplanar inputs:** R0023's Stage-0 n-ary case (N25) converts; the
   `coplanar input face pair` wall survives only for a refusal the paper
   itself does not cover, recorded as PERMANENT.
5. **Fresh-geometry ERROR rate ≤ 5 %** on each of the two gate-1 seeds (the
   seed-5 figure), and **every ERROR is typed and loud** — a `YangError` /
   `KernelError` variant the app and the MCP tools can tell apart without
   reading text (remediation A1).
6. **Out of scope for 1.0:** fillet, chamfer, shell (deferred indefinitely,
   CLAUDE.md). An attempt returns a typed `NotSupported`.

### Gate 4 — Bounded, not fast

1. **No TIMEOUT** on the corpus or the gate-1 seeds at the canonical 900 s
   CPU budget. Gear revolves (prospector seed-5 indices 37 and 44, 900 s to
   2627 s) are the known customers. A result-neutral complexity fix (K10:
   BVH for `detect_improper_contacts`, no per-removal `compute_phase_a`) is
   allowed for this gate; anything that changes a result is not.
2. **No panics in kernel code:** remediation K7 (`expect`/`unwrap` in yang-rs
   become typed returns or carry an invariant comment; clippy deny), and the
   WASM build clean.
3. **Deterministic:** the O7 canonical geometry hash agrees across three runs,
   serial and `cherchi-rs/parallel` (already green; kept green).

## 2. Scoreboard

Printed by `./scripts/kernel_1_0_score.sh` (gates 1–4 from the latest corpus
report, prospector reports and the faithfulness script). Report it at every
roadmap checkpoint, next to the roadmap's own scoreboard.

| gate | metric | 2026-11-03 baseline | 1.0 target |
|---|---|---|---|
| 1 | corpus WRONG | 0 (HEAD a1932058: 360C / 0W / 9E / 5EE / 0T, 2026-11-03) | 0 |
| 1 | fresh-seed WRONG | seed 6: 6 — 4 oracle defects (body membership by bbox; composition band), 2 engine cut auto-reversal; 0 kernel | 0 on two seeds |
| 1 | record-only oracles | 2 (O2, O5) | 0 |
| 1 | N100 items open | see ledger N100 | 0 |
| 2 | ledger debt | 65 | 0 |
| 2 | matrix not-faithful | 132 of 194 | 0 MISSING/ORDER; rest PERMANENT-backed |
| 2 | consecutive clean audits | 0 | 2 |
| 3 | surface pairs with a CORRECT customer | not built | 21 of 21 |
| 3 | B-spline end to end | no | yes |
| 3 | fresh-seed ERROR rate | seed 6: 6 % (12/200; 2 are the empty-result harness verdict) | ≤ 5 %, all typed |
| 4 | corpus + seed TIMEOUTs | corpus 0; seed 6: 4 | 0 |
| 4 | undocumented panic sites in yang-rs (K7) | ~60 (review count; not yet gated) | 0 |

## 3. The way there

- **Step 0 — measure (2026-11-03).** Corpus on HEAD; prospector seed 6 with
  every oracle on; adjudicate the O2 and O5 outliers and promote both to
  verdicts. Output: the baseline column above, filled from computed numbers.
- **Step 1 — the roadmap, in order.** F2 (one owner, D-2) → F3 → F4's open
  items → F5 (the generic interface, one CDT per face, the B-spline instance)
  → F6 → F7. Gate 3's pair matrix and gate 4's K7 / K10 are non-kernel-lane
  work and run in parallel with any milestone.
- **Step 2 — close the audit.** Two independent re-reads (gate 2.2).
- **Step 3 — declare 1.0** on the first commit where the script prints every
  target met, with the corpus and two fresh seeds run on that commit.

## 4. Delegation to Antigravity (Gemini)

Gemini (`agy`, model `gemini-4-argon-high`) takes bounded tasks; the session
that owns the milestone reviews and lands. It reads `GEMINI.md`, not
CLAUDE.md, so each task carries a written brief.

**Good fits:** algorithmic primitives built gated-off with red/green pins
(isometric-chart CDT, closest-point projection, the parametric-surface
interface, the B-spline instance, BVHs); anchoring an ERROR to its
un-minimized lineage with the timing probes and naming its milestone
(report only); independent audit re-reads of one paper section at a time
(gate 2.2 — a different model family is the point); test-harness oracles
(gate 3's pair matrix).

**Not delegated:** owning a kernel milestone, grading a matrix row or ledger
entry FAITHFUL / RESOLVED, landing a kernel commit, running the canonical
corpus. Past runs over-graded their own work and moved substitutes instead
of building the paper's mechanism (memory, 2026-10-31).

**Protocol:**
1. A worktree `~/wt/agy-<task>` on branch `agy/<task>` from main, with
   `refs/` symlinked in (it is gitignored), and a private
   `CARGO_TARGET_DIR=target/wt-agy-<task>`.
2. A brief `AGY_BRIEF_<TASK>.md` in the worktree: paper lines, code sites,
   deliverables, pins, and the rules (no wiring, no knobs, reserve every
   heavy job, no ledger edits, a summary file with the exact test result
   lines and what is unfinished).
3. Run headless: `agy --model gemini-4-argon-high
   --dangerously-skip-permissions --print-timeout 0 -p "<prompt>"` from the
   worktree (`-p` takes the NEXT argument as the prompt — put it last).
4. Review the branch independently: re-run its tests, read every diff against
   the paper, check its claims against its own logs. Land through the normal
   gates, or send it back with the review.

## 5. Checkpoint log

| date | step | gate 1 | gate 2 (debt / not-faithful) | gate 3 | gate 4 | commit |
|---|---|---|---|---|---|---|
| 2026-11-03 | adopted; step 0 started | — | 65 / 132 | pair matrix not built | — | dc422b05 |
| 2026-11-03 | step 0: corpus on HEAD + seed 6 | corpus 0W; seed 6 6W (0 kernel) | 65 / 132 | ERROR 6 % | seed 6: 4 T | (this commit) |
