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
4. **An independent B-Rep kernel agrees.** Every boolean the corpus and the
   two gate seeds categorize CORRECT is also run through OCCT as a black box
   (operands exported as STEP, the same policy as the Cherchi sidecar: a
   dev-only oracle whose source we do not read). Volume, surface area and the
   per-type face census agree within stated bands, and every disagreement is
   adjudicated in writing. The harness's own oracles are authored by the
   project; fresh seeds and the B-spline pairs have no authored expectation,
   and this is the check that does not share our assumptions.

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
4. **Sign-offs are requested by class where one rationale covers several
   entries** (roadmap F7). A request is still prose, with the paper cite and
   one recommendation (A15.7 point 3); it names every ledger entry and matrix
   row it covers. The gate needs every PARTIAL and DEVIATES row fixed or
   PERMANENT-backed (122 rows and 38 DELEGATED entries at 2026-11-12), so
   requests are drafted as their milestone lands, not saved for the end.

### Gate 3 — Capable

The supported envelope, each item with a corpus or pinned customer:

1. **Every surface pair.** The 21 pairs of {plane, cylinder, cone, sphere,
   torus, B-spline} each have at least one general-position boolean customer
   (union, cut, intersect) that categorizes CORRECT. Built as a pair-matrix
   test in `crates/test-harness/` (new).
2. **The generic parametric-surface interface and a B-spline instance end to
   end** (roadmap F5a and F5b; resolves D14). A B-spline face enters Stage 1,
   meets an analytic face in a boolean, and leaves as an exact B-Rep face.
   The hybrid representation is the paper's: every mesh vertex carries
   parameter coordinates on each face it lies on (§4.2.3), and no stage
   reaches a surface except through the interface.
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
7. **Every degenerate contact has a stated answer.** The paper's test sets
   are randomly rotated pairs, so it covers general position, coplanar planes
   (§4.5.5) and isolated tangent points (§4.3.3). Mechanical models are mostly
   the other cases. A contact matrix in `crates/test-harness/` (new) crosses
   each surface pair with each contact class that pair admits: coincident
   surfaces, tangency along a curve, tangency at a point, an edge lying in a
   face, a vertex lying on a face, a shared edge. Every cell categorizes
   CORRECT or returns a typed refusal that names the contact class. No cell
   is WRONG, untyped, or untested. A refusal the paper does not cover is a
   SILENT row with a signed rationale (gate 2.3).

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
| 1 | fresh-seed WRONG | seed 6: 6 → 5 after the engine cut fix (00021 CORRECT); the 5 left are oracle defects (body membership by bbox, stale extent box, composition band); 0 kernel | 0 on two seeds |
| 1 | record-only oracles | 2 (O2, O5) → **1 (O2)** — O5 a verdict 2026-11-05: 1171 corpus steps checked, 121 not covered, 0 violations | 0 |
| 1 | exact-chain coverage (`assay_exact_coverage`) | corpus 355 / 374; seed 6 195 / 198 (main 592c813f) | no lower than baseline; every decline justified |
| 1 | N100 items open | see ledger N100 | 0 |
| 1 | independent-kernel (OCCT) disagreements not adjudicated | oracle not built (2026-11-12) | 0, over the corpus and two seeds |
| 2 | ledger debt | 78 | 0 |
| 2 | matrix not-faithful | 157 of 194 | 0 MISSING/ORDER; rest PERMANENT-backed |
| 2 | consecutive clean audits | 0 | 2 |
| 3 | surface pairs with a CORRECT customer | **15 of 21** (`assay_pair_matrix` on 75f2f1be, 3 seeds: 135 / 135 CORRECT, 0 WRONG, 0 ERROR, 132 with exact-volume agreement); the 6 B-spline pairs are not buildable (F5) | 21 of 21 |
| 3 | B-spline end to end | no | yes |
| 3 | parameter coordinates carried per vertex per face (F5a) | carried, unread: every vertex at every stage (2026-11-17, F5a inc 3); Stage 4 still projects 3D points (matrix 4.3-05) | yes |
| 3 | degenerate-contact matrix cells CORRECT or typed | matrix not built (2026-11-12) | every cell |
| 3 | fresh-seed ERROR rate | seed 6: 6 % (12/200; 2 are the empty-result harness verdict) | ≤ 5 %, all typed |
| 4 | corpus + seed TIMEOUTs | corpus 0; seed 6: 4 | 0 |
| 4 | undocumented panic sites in yang-rs (K7) | ~60 (review count; not yet gated) | 0 |

## 3. The way there

- **Step 0 — measure (2026-11-03).** Corpus on HEAD; prospector seed 6 with
  every oracle on; adjudicate the O2 and O5 outliers and promote both to
  verdicts. Output: the baseline column above, filled from computed numbers.
  - **O5 adjudicated (2026-11-03): all 41 corpus violations are AUTHORED-
    EXPECTATION defects, none kernel.** 39 are no-change steps (exact B-Rep
    volume identical, rel 0.0 — a tool that misses the body returns the
    operand; C0038/C0040 are 10 µm / 1 mm holes below the meta's resolution;
    C0043 is a union equal to A by design); 2 are wrong-sign (C0074, C0081:
    Intersect-combine bosses, which legitimately shrink). The generator writes
    "decrease" per cut and "increase" per boss without knowing reach, so O5
    cannot be promoted by reading `volume_monotonicity`. **Promotion path:**
    the expected direction per step (decrease / none / increase) comes from
    the exact chain's per-step volume where it covers the prefix, and a step
    is a violation only when the kernel's exact change disagrees with it —
    which still catches a silently dropped cut. Waits on the
    `agy/oracle-extent` review (same file).
- **Step 1 — the roadmap, in order.** F5a (the parameter-domain substrate:
  the surface interface and per-vertex parameter coordinates, result-neutral)
  → F2 (§4.4.1 on that substrate) → F1's flip → F3 → F4's open items → F5b
  (§4.1 Stage 1, one CDT per face, the B-spline instance) → F6 → F7. One
  owner holds F5a, F2 and F1's flip (D-2). Every milestone converts the
  production path in place, in this order (owner, 2026-11-16; reasons in
  `specs/yang_paper_path.md` §6).
- **Step 1, in parallel (non-kernel lanes).** Gate 1.4's OCCT oracle, gate
  3.1's pair matrix, gate 3.7's contact matrix, gate 4's K7 and K10, and the
  consumer contract (§3a). None of these waits for a milestone.
- **Step 2 — close the audit.** Two independent re-reads (gate 2.2).
- **Step 3 — declare 1.0** on the first commit where the script prints every
  target met, with the corpus and two fresh seeds run on that commit.

## 3a. The consumer contract — what the app, the MCP tools and Boardroom wait for

The app, the agent tools and the Boardroom integration do not wait for 1.0.
They need four things from the kernel, all in the remediation plan's lane A
and none of them a faithfulness milestone:

1. **Typed failures** (A1, the same work as gate 3.5): each Yang STOP class
   is a distinct `engine_error.kind` with face ids in `details`. No consumer
   parses error text.
2. **Typed no-ops** (A2): a boolean that changes nothing, and one that
   consumes its target, are typed outcomes, not successes with a warning.
3. **A validity answer** (A3, A4): `body_measure.closed` is right for boolean
   outputs, and `body_validate` reports closed, manifold, Euler and
   self-intersection.
4. **A stable trait**: `waffle_types::kernel::{Kernel, KernelIntrospect}` is
   the only kernel surface a consumer touches, and a change to it is an
   interface change (root `INTERFACES.md`).

When these four hold, consumer work proceeds against the kernel as it is. A
boolean the kernel cannot do yet is, to a consumer, a typed refusal it can
show or route around; kernel 1.0 raises how often the answer is a solid, it
does not change the contract.

**Outside the repo (user):** ask the paper's authors for their code and for
test sets A and B (the paper says both would be released; a search on
2026-11-12 found neither). A reference implementation would be a parity
oracle for the whole pipeline, as the Cherchi sidecar is for Stage 2.

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
(gate 3's pair matrix and contact matrix, gate 1.4's OCCT sidecar).

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
| 2026-11-03 | step 0: corpus on HEAD + seed 6 | corpus 0W; seed 6 6W (0 kernel) | 65 / 132 | ERROR 6 % | seed 6: 4 T | 38190e9b |
| 2026-11-03 | engine: cut direction + ThroughAll read the true extent | corpus 360C/0W (R0020 tie found and fixed); seed 6 5W, all oracle | 65 / 132 | — | — | 8dfcb8e8 |
| 2026-11-05 | step 0: O5 promoted to a verdict (direction AND bounds from a certified per-step interval of the exact chain, not the authored meta) | corpus 360C/0W/9E/5EE/0T held, O5 0 violations; record-only oracles 1 (O2) | 65 / 132 | — | corpus 0 T | (this commit) |
| 2026-11-06 | F2 / N106 increment 2: separate an update simplex the other operand holds (owner-signed); I2d retirement measured, blocked on two pipeline-mesh volume pins (needs F5 interior sampling) | corpus 360C/0W/9E/5EE/0T held (0 fires); refusal-off trial 360C/0W, 8 hidden cases convert | 65 / 132 | — | — | (this commit) |
| 2026-11-06 | F2: N41 doubled-membrane cancellation retired (0 fires on the corpus, seed 6 and the refusal-off trial, by the new `YANG_REPAIR_CENSUS`); a residual fin is a loud gate failure + T4 site, closing N41's even-χ silent hazard | corpus 360C/0W/9E/5EE/0T held; seed 6 187C/0W/10E/3T | 64 / 132 | seed 6 ERROR 5 % | seed 6: 3 T | (this commit) |
| 2026-11-12 | plan revised after review (user: *"update the 1.0 plan and roadmap with those recommendations"*): F5 split, F5a (parameter-domain substrate) ahead of F2; gate 1.4 independent OCCT oracle; gate 3.7 degenerate-contact matrix; class sign-offs (gate 2.4); consumer contract §3a not gated on 1.0. No geometry changed | corpus not re-run | 64 / 132 | contact matrix and F5a not built | — | (this commit) |
| 2026-11-13 | owner ruling: a retirement may cost a named dip (roadmap §1 item 4). First retirement under it: the Stage-1 cross-operand rim boost | corpus 360C/0W/9E/5EE/0T held; seed 6 188C/0W/10E/2T, verdict-identical | 64 / 132 | one kernel pin now loud: coaxial tube with a 1e-5 wall | — | (this commit) |
| 2026-11-13 | second retirement: the two a-priori Stage-1 folds (N88 amended) | corpus 360C/0W/9E/5EE/0T held; seed 6 verdict-identical | 64 / 132 | — | — | (this commit) |
| 2026-11-13 | third retirement: the crease and corner transits (N87 RESOLVED). Edge-in-plane arm 2 kept: a rigid-motion sweep found its customer | corpus 360C/0W/9E/5EE/0T held, every output bit-identical; seed 6 verdict-identical | 63 / 132 | rotation, at the tip after the `extent_along` fix: 69 of 1,440 rotated runs of CORRECT cases do not reproduce the unrotated result (62 loud, 7 a different body or vertex count; C0117 in 4 of 4, the rim boost's named dip) | — | (this commit) |
| 2026-11-14 | a silent wrong model under rotation found and closed: F0076 moved rigidly (extra material, no error). yang-rs trigger T9 holds an intersection curve's interior to its faces' domains (N107 OPEN: a certificate until F5a) | corpus 360C/0W/9E/5EE/0T held, every result line identical but F0064's error text | 64 / 132 | rotation: 68 of 1,440 rotated runs of CORRECT cases do not reproduce the unrotated result (62 loud, 6 a different body, face or vertex count); F0076 now reproduces in 4 of 4 | — | `fe13dfaf` |
| 2026-11-15 | T9 refused a correct build — a mounting hole beside a board's rounded corner (CI red on `fe13dfaf`, test-harness `board_shape_kv2`). The certificate now reads a face's side of a crease from its mesh triangles, not its vertices alone (N107 corrected, still OPEN) | corpus 360C/0W/9E/5EE/0T held, `results.json` byte-identical | 64 / 132 | rotation: all 374 sweep verdict files identical to 2026-11-14 (68 of 1,440 do not reproduce: 62 loud, 6 quiet) | — | (this commit) |
| 2026-11-15 | the quiet rotation mismatches read, no kernel change: R0057 moved is the same solid; F0094 moved is two bodies for want of a §4.5.5 overlay on a crossing disc pair (N108 ledgered, OPEN, roadmap F6; matrix 4.5.5-07 → PARTIAL) | corpus 360C/0W/9E/5EE/0T held | 65 / 133 | rotation: the sweep probe now reads where the moved body is (`geo`); the six quiet mismatches are all the unmoved solid in place, grouped differently | — | (this commit) |
| 2026-11-16 | owner decision: the core converts in place, in roadmap order (F5a → F2 → I3b → …). The combined core trial `specs/yang_paper_path.md` §6 proposed is not cheap (the I3b and N85 trials were never committed) and each recorded trial loss names a missing earlier milestone, not another core change | not re-run (no kernel change) | 65 / 133 | — | — | (this commit) |
| 2026-11-16 | step 1, F5a increments 1–2: the parameter-domain surface interface (`param_surface`), `stage4_dt` re-sited onto it | corpus 360C/0W/9E/5EE/0T, geometry hash identical on 374 | 65 / 133 | — | — | (this commit) |
| 2026-11-17 | step 1, F5a increment 3: parameter coordinates carried per vertex per face (N109 ledgered: from the closed-form inverse) | corpus 360C/0W/9E/5EE/0T, geometry hash identical on 374 | 66 / 133 | (u, v) carried, unread | — | (this commit) |
| 2026-11-17 | step 1, F5a increment 4: `Surface::` match inventory; 47 type tests converted to interface queries; 22 type-narrowed rules named against their milestones | corpus 360C/0W/9E/5EE/0T, geometry hash identical on 374 | 66 / 133 | — | — | (this commit) |
| 2026-11-17 | matrix re-audit at `244bb484`: 27 rows re-graded after review, none to FAITHFUL; the NURBS rows N/A → MISSING/PARTIAL/DEVIATES under the reopened D14 | not re-run (no kernel change) | 66 / 157 | — | — | (this commit) |
| 2026-11-17 | ledger catch-up: N110–N118, nine live mechanisms with no entry (found by the re-audit, each read in the code); N100 item 6 measured on the corpus, item 10 added | not re-run (print-only probes) | 75 / 157 | — | — | (this commit) |
| 2026-11-17 | ledger catch-up: N119–N121 (I6.6 pleat cancellation, KV10/KV15 post-Stage-2 near-welds, PR-KV9 junction-twin collapse); N113 (rehome enumerates surface types), N116 (circle arms), N117 (rim snap), N25 amended; the second Gemini pass's eight leads each read in the code | not re-run (diagnostic print gating only) | 78 / 157 | — | — | (this commit) |
| 2026-11-18 | step 2, F2 item 2 increments 0–2: the §4.4.1 patch chart on the face's parameter domain (`stage4_patch_chart`), taken by the update for sphere and torus patches and cone caps (N122 ledgered) | corpus 360C/0W/9E/5EE/0T held, no category move | 79 / 157 | — | — | 150fc2f2, 1906b5e8 |
| 2026-11-20 | step 2, F2 item 2 increment 3: one chart for the update — plane, cylinder and cone patches rebuild on the parameter domain too | corpus 360C/0W/9E/5EE/0T held, no category move, every oracle detail identical; geometry hash identical on 364 of 374 | 79 / 157 | — | — | (this commit) |
| 2026-11-20 | step 2, F2 item 2: the chain decimation's whole-patch fallback on the same chart; its sphere-only skip retired | corpus 360C/0W/9E/5EE/0T held, no category move, every oracle detail identical | 79 / 157 | — | — | (this commit) |
| 2026-11-21 | step 2, F2 item 3: run absorption, group absorption and corner re-homing retired (N113 RESOLVED); the Stage-5 corner merge measured and kept (P0027 under one rigid motion needs it, pinned) | corpus 360C/0W/9E/5EE/0T held, hash file identical on all 374; seed 6 189C/0W/9E/2T | 78 / 157 | seed 6 ERROR 4.5 % | seed 6: 2 T | (this commit) |
