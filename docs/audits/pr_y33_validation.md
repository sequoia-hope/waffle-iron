# PR-Y33 Validation (Adversary `adv-y33`)

**Verdict: ACCEPT WITH BANKED FINDINGS** — CASE B infra-only ship.

PR-Y33 ships the Cherchi-Rust per-stage byte-diff canary (Y33_PROBE instrumentation +
canary memo). All 12 applicable no-regression gates hold strictly; gates 1+2 are
**N/A** because the PR is explicitly infra-only and ships no production-logic fix.

---

## Gate table (14 gates per plan §"Phase 0e — Adversary")

| # | Gate | Status | Observed |
|---|------|--------|----------|
| 1 | F0020 STAGE-N byte-parity (production-fix gate) | **N/A** | CASE B ships zero production-logic fix |
| 2 | F0020 missing improvement | **N/A** | Same: no fix to measure |
| 3 | F0020 [topo-extract] unpaired=0 (PR-Y22 contract) | **PASS** | max=Some(0); 2 invocations on b#1+b#2 |
| 4 | F0020 [twin-oracle] unpaired_count=0 (PR-Y24 contract) | **PASS** | max=Some(0); 2 invocations |
| 5 | F0044 batch [topo-extract] unpaired=0 (cohort guard) | **PASS** | max=Some(0); 7 invocations (F0044 b#1 + F0045 b#2-4 + R0092 b#5-7) |
| 6 | F0044 batch [twin-oracle] unpaired_count=0 | **PASS** | max=Some(0); 7 invocations |
| 7 | F0044 byte-match (PR-Y31 contract) | **PASS** | missing=0, extras=0, common=136 — `pr_y31_f0044_extras_zero` GREEN |
| 8 | Cohort differential diff unchanged | **PASS** | F0020 missing=93 / extras=148 / common=144 (within Cherchi non-det vs PR-Y29 baseline of 97/146/140); F0044 0/0/136 (deterministic, unchanged); F0045 236/466/0 (deterministic, unchanged); R0092 192/368/0 (within Cherchi non-det vs historical 153/405 swing) |
| 9 | Spotlight F0020 Failed (unchanged) | **PASS** | 36 unpaired/130, χ=-4 — matches prior baselines |
| 10 | Spotlight F0030 Failed (unchanged) | **PASS** | 12 unpaired/66, χ=3 — matches brief exactly |
| 11 | Spotlight F0044 Failed (unchanged) | **PASS** | 12 unpaired/180, χ=4 |
| 12 | Spotlight F0050 Failed (unchanged) | **PASS** | 39 unpaired/417, χ=106 |
| 13 | Yang fast corpus ≥10/157 (PR-Y31 baseline) | **PASS** | 10/157 passed, 140 failed, 7 errored (33 skipped timeouts) |
| 14 | Kernel baseline parity (1254/25/42) | **PASS** | HEAD: 1254 pass / 25 fail / 42 ignored; baseline `b061542`: identical 1254 / 25 / 42 |

**No-regression gates 3-14: ALL PASS.**
**Production-fix gates 1-2: N/A by CASE B definition (no production-logic delta).**

---

## CASE B justification

PR-Y33's stated goal (per plan §"Phase 0a — Step 5"): ship the per-stage canary
infrastructure. The empirical decision rule was "ship a production fix if a single
sub-anchor accounts for ≥80% of F0020 pair-diff and costs ≤200 LOC; otherwise ship
infra-only."

**Canary empirical finding (memo §3.4 + §4.2):**
- Sub-anchor A (Yang Gauss-map filter at `intersection_class.rs:134-137`): accounts
  for 24/119 Waffle-missed STAGE4 pairs (20% — below 80% threshold). Fix cost ~5-20 LOC.
- Sub-anchor B (`triangles_intersect_exact` over-permissiveness vs `cinolib::Triangle::intersects_triangle(true)`):
  accounts for 95/119 Waffle-extra STAGE4 pairs (80%). Fix cost 100-200 LOC re-port.

Combined budget ~120-220 LOC straddles the 200-LOC ceiling, but the **§4.3 propagation
trace** added in `4a2f37c` is the load-bearing observation: 0/19 (0%) of Cherchi-only
STAGE6 verts trace EXCLUSIVELY to Cherchi-only STAGE4 pairs. The 24-pair (sub-anchor A)
contribution is *aliased* by shared-triangle classification overlap with 60 common pairs.
Sub-anchor A alone has no clean 1:1 chain to the F0020 missing-count gate.

**Conclusion: CASE B (infra-only) is the structurally correct verdict.** Sub-anchor A is
cheap-to-attempt but not provably load-bearing; sub-anchor B is paper-cited but costly.
Splitting into PR-Y33 (infra) + PR-Y34+ (fix attempts) is the lower-risk path.

---

## Probe code review (CASE B critical check)

The probe at `crates/kernel/src/boolean/cherchi/mod.rs` is **genuinely env-gated and
default-off**:

1. **Entry guard** (`y33_probe::dir_for()`):
   ```rust
   if std::env::var("Y33_PROBE").as_deref() != Ok("1") {
       return None;
   }
   ```
   Returns `None` unless `Y33_PROBE=1`. All call sites use
   `if let Some(d) = y33_dir.as_ref()` to guard dumps.

2. **Only behavioral side effect on default path**: a `fetch_add(1)` on the
   `Y33_INVOCATION_COUNTER: AtomicU32`. The result is consumed by `y33_probe::dir_for()`
   and discarded if `Y33_PROBE` is unset. The atomic itself is not observable
   externally and has no synchronization impact on the rest of the pipeline.

3. **Default-off byte-parity empirically verified**:
   - HEAD `4a2f37c` deterministic test stats vs baseline `b061542`: byte-identical
     (Waffle: 294 tris / 117 verts / χ=1; Cherchi: 246 tris / 120 verts / χ=5;
     pair diff: 93 missing / 155 extras / 137 common — all match).
   - Kernel test suite: HEAD 1254/25/42 = baseline 1254/25/42 (identical counts).
   - Inter-run variance is pre-existing HashMap-ordering non-determinism present
     in baseline too — NOT introduced by PR-Y33.

4. **No leaked dumps on default path**: `/tmp/y33-canary/` contained pre-existing
   canary investigation artifacts from the PR-Y33 canary phase, but no new dumps
   were emitted by default-path runs in this validation.

5. **Probe-on verification (Y33_PROBE=1)**: confirmed working. F0020 produces
   2 invocation directories (`inv0/`, `inv1/`) each containing 11 dump files
   (`stage3_verts.txt`, `stage3_jolly.txt`, `stage3_tris.txt`, `stage3_edges.txt`,
   `stage4_pairs.txt`, `stage5_cop_tris.txt`, `stage5_int_tris.txt`, `stage5_segs.txt`,
   `stage5_tri2pts.txt`, `stage6_tris.txt`, `stage6_verts.txt`). Matches the
   canary memo's documentation (F0020 = 3 extrudes → 2 boolean invocations).

---

## Lint check (banked finding)

The canary memo claimed "lint checks all clean." Re-verified at HEAD `4a2f37c`:

- **`cargo fmt --check -p kernel`**: clean (no output).
- **`cargo check -p kernel`**: clean (only the pre-existing
  `OptimError::NotConverged` dead-code warning, present at baseline too).
- **`cargo clippy -p kernel`**: kernel warning count goes 95 → 99 (delta +4).
  All 4 new warnings are `clippy::ptr_arg` style lints flagging `&PathBuf` vs
  `&Path` parameter types on the 4 probe dump functions
  (`dump_stage3`/`dump_stage4`/`dump_stage5`/`dump_stage6` at
  `cherchi/mod.rs:120/178/192/247`).

**Banked finding (non-blocking)**: 4 new `clippy::ptr_arg` warnings on probe code,
cosmetic only, do not affect compilation or behavior. Trivially fixable in a
follow-up by changing parameters from `&PathBuf` to `&Path`. Does not block ACCEPT.

---

## Anti-fabrication artifacts

```
$ git log --oneline b061542..HEAD
4a2f37c audit(yang-pr-y33-canary): add §4.3 propagation trace — confirms CASE B
5e378db audit(yang-pr-y33-canary): STAGE4 detect_intersections is first-divergent | CASE B INFRA-ONLY recommended

$ git diff b061542..HEAD --numstat
223	1	crates/kernel/src/boolean/cherchi/mod.rs
443	0	docs/audits/pr_y33_per_stage_canary.md
```

Net: +666 / -1 LOC across 2 commits and 2 files. The brief described 1 commit
(`c5657bf` ≡ `5e378db`) with "~+629 LOC, 1 deletion." The worktree has the
follow-up `4a2f37c` adding +38 LOC of §4.3 propagation-trace appendix to the
canary memo (no production code, no probe-code changes). This refinement
strengthens the CASE B verdict (canary memo §4.3) and should be cherry-picked
together with `5e378db`.

The 223-LOC `mod.rs` diff (vs brief's stated 180) is the literal `git diff --numstat`
output; brief's "180 LOC" appears to have been a hand-count estimate. All 223 inserted
lines are either (a) inside the `mod y33_probe` module (default-off), (b) inside
`if let Some(d) = y33_dir.as_ref()` guards, or (c) the atomic counter declaration
and increment. None modify production logic.

---

## PR-Y34 anchor recommendation (per canary §5 + §4.3 propagation trace)

**Recommended sequence** (cheapest-first):

1. **Sub-anchor A canary first** (~5 LOC): delete the Yang Gauss-map filter at
   `crates/kernel/src/boolean/cherchi/intersection_class.rs:134-137`. This is a
   paper-cited correction — Yang §4.2.2 Theorem 4.1 assumes manifold inputs,
   which Waffle's surface-boundary triangles routinely violate per canary §3.3
   evidence (24/24 Waffle-missed pairs are at boundary configurations). Test:
   re-run cohort diff; check F0020 missing count (baseline 93–97 with Cherchi
   non-det).

2. **§4.3 propagation trace caveat**: Sub-anchor A is NOT guaranteed to clear
   the F0020 missing gate because the 24 Cherchi-only STAGE4 pairs share input
   triangles with 60 common pairs, and Cherchi's `classifyIntersections`
   aggregates per-triangle. The fix is **architecturally correct** (removes a
   wrong manifold-assumption pruning step) but its missing-count delta is
   bounded above by 24 pairs and below by 0. Treat the canary outcome as the
   load-bearing oracle.

3. **If sub-anchor A insufficient** (canary §4.2 prediction): re-port
   `triangles_intersect_exact` to match `cinolib::Triangle::intersects_triangle(true)`
   semantics (sub-anchor B, ~100-200 LOC). The canary memo has the LOC-level
   anchor at `intersection_class.rs` and the cinolib reference patch.

4. **Per "Verify Fix Anchor Before Coding"** (`feedback_anchor_before_fix.md`):
   even though canary §3 + §4.3 are *position-co-located* on F0020, run
   Y33_PROBE=1 with the proposed fix applied and confirm the dump matches
   Cherchi's STAGE4 dump on F0020 BEFORE declaring PR-Y34 complete. The probe
   is now the ground-truth oracle.

---

## Verdict: ACCEPT WITH BANKED FINDINGS

PR-Y33 cleanly meets its stated CASE B goal: ship empirical-evidence-grade
infrastructure for future Cherchi-Rust parity work, with **zero production-logic
risk** and a probe that is **byte-identical to baseline on the default path**.
All no-regression gates (3-14) hold strictly. The 4 new `clippy::ptr_arg`
warnings on probe-code parameters are cosmetic and do not block acceptance.

Banked findings for follow-up:
1. 4 `clippy::ptr_arg` warnings on probe dump functions — trivially fixable.
2. PR-Y34 sub-anchor A (Yang Gauss-map deletion) is paper-cited and cheap but
   **not provably load-bearing** for the F0020 missing gate (§4.3 propagation
   trace). Canary outcome is the load-bearing oracle, not anchor topology.
3. Cherchi-side TBB non-determinism persists (F0020 / R0092 missing-counts
   vary across runs); future PR-Y3x canaries should `TBB_NUM_THREADS=1` AND
   accept that Cherchi-side missing-count is a *range* not a *value*.

---

*Validation by `adv-y33` on worktree branch `worktree-y33-canary`.
Memo committed at: TBD (this commit). HEAD at validation start: `4a2f37c`.*
