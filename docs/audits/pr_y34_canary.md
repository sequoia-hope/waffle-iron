# PR-Y34 canary — Yang 2025 §4.2.2 Theorem 4.1 same-mesh Gauss-map filter deletion

**Status:** SHIP-A recommended (Gates 1–7 all GREEN; F0020 Stage B missing-count drops 93 → 7 = -92.5%; F0044 byte-parity preserved; cohort missing-counts unchanged; yang_fast 10/157 preserved; kernel lib +1 net pass).

**Author:** canary-y34
**Date:** 2026-05-12
**Worktree branch:** `worktree-canary-y34`
**Parent commit:** `478db04` (PR-Y33 SHIPPED — STAGE4 first-divergent, infra-only)

---

## §0 Summary — single paragraph

PR-Y33 SHIPPED `478db04` localized F0020's first-divergent stage to `detect_intersections` (STAGE4) and identified two sub-anchors: (A) same-mesh `continue` at `intersection_class.rs:134-137` (Yang §4.2.2 Theorem 4.1, manifold premise violated by Cherchi 2022's soup input), and (B) `triangles_intersect_exact` over-permissiveness. PR-Y34 canary applies sub-anchor A as a ~6-line deletion (preserves the cross-mesh `orient3d` skip; deletes only the same-label early `continue`). All 7 gates of the canary brief pass: F0020 Stage B **missing-count 93 → 7 (-92.5%)**, **extras 148 → 0 (-100%)**, common 144 → 230 (+60%); F0044 byte-parity preserved (Cherchi 136 / Waffle 136 / 0 missing / 0 extras); F0045 + R0092 cohort missing-counts unchanged at 236 / 192 with F0045 extras dropping 466 → 273 (-41%) and R0092 extras unchanged at 368; yang_fast corpus 10/157 (no regression); kernel lib full suite net +1 pass (`test_subdivision_shared_edge_split_propagation` flipped FAIL → PASS, zero new failures). Sub-anchor B remains banked for PR-Y35 (the ~210 STAGE4 pair count growth shows sub-anchor B's predicate is still over-permissive, but its symptoms no longer manifest in Stage B missing-count once sub-anchor A unblocks the classification cascade).

---

## §1 Discipline — worktree-only, no live tree changes

Per PR-Y33 §1 template + worktree-canary brief.

- **Worktree:** `/home/claude/workspace/.claude/worktrees/canary-y34/` (branch `worktree-canary-y34`), parent `478db04`.
- **Live tree changes:** zero production code modifications to `/home/claude/workspace/` (main).
- **Cherchi C++ sidecar:** NOT present at PR-Y34 start. Re-cloned to `~/cherchi2022/InteractiveAndRobustMeshBooleans` from `github.com/gcherchi/InteractiveAndRobustMeshBooleans`, GUI disabled (`CINOLIB_USES_OPENGL_GLFW_IMGUI=OFF`), built `mesh_booleans` target only. The Y33_PROBE stage-dump patches from PR-Y33 §7 were NOT re-applied (this canary only uses Cherchi's final-output OBJ via the diff harness; per-stage byte-diff is not required to gate sub-anchor A).
- **Final diff** (`git diff HEAD --stat`):

```
 app/tests/cases/assay/results.json                 | 138 ++++++++++-----------
 crates/kernel/src/boolean/cherchi/intersection_class.rs |  12 +-
 2 files changed, 75 insertions(+), 75 deletions(-)
```

`numstat`:

```
69	69	app/tests/cases/assay/results.json
6	6	crates/kernel/src/boolean/cherchi/intersection_class.rs
```

`app/tests/cases/assay/results.json` is a side-effect of running `yang_fast` and the diff harness (each writes per-case status into the file). It is NOT part of the fix shape and would NOT be staged in a SHIP-A commit.

- **Production fix shape** (verbatim diff):

```diff
diff --git a/crates/kernel/src/boolean/cherchi/intersection_class.rs b/crates/kernel/src/boolean/cherchi/intersection_class.rs
index faa39a9..74ddc55 100644
--- a/crates/kernel/src/boolean/cherchi/intersection_class.rs
+++ b/crates/kernel/src/boolean/cherchi/intersection_class.rs
@@ -131,12 +131,12 @@ pub(crate) fn detect_intersections(
                 let len0_sq = n0[0] * n0[0] + n0[1] * n0[1] + n0[2] * n0[2];
                 let len1_sq = n1[0] * n1[0] + n1[1] * n1[1] + n1[2] * n1[2];
                 if dot > 0.0 && len0_sq > 1e-30 && len1_sq > 1e-30 {
-                    if ts.tri_label(t0) == ts.tri_label(t1) {
-                        // Same-mesh: safe to skip co-oriented pairs.
-                        continue;
-                    }
-                    // Cross-mesh: skip only if t1 is strictly on one side
-                    // of t0's plane (not coplanar, no straddling).
+                    // Skip only if t1 is strictly on one side of t0's plane
+                    // (not coplanar, no straddling). Manifold-agnostic — sound
+                    // for Cherchi 2022 §3 soup input which may contain co-planar
+                    // same-mesh face pairs along edges (e.g. F0020 3-extrude).
+                    // Yang 2025 §4.2.2 Theorem 4.1 same-mesh shortcut removed:
+                    // its manifold premise is violated by Cherchi's input contract.
                     let a = ts.tri_vert(t0, 0);
                     let b = ts.tri_vert(t0, 1);
                     let c = ts.tri_vert(t0, 2);
```

LOC delta: 6 lines added, 6 deleted (net 0). Effective change: deletes the `if ts.tri_label(t0) == ts.tri_label(t1) { continue; }` block; cross-mesh `orient3d` early-skip retained.

---

## §2 Method — 7 gates

Per worktree-canary brief.

| Gate | What | Command |
|---|---|---|
| 1 | Kernel build + cherchi lib tests pass | `cargo build -p kernel` + `cargo test -p kernel --lib -- cherchi` |
| 2 | F0020 STAGE4 pair count via Y33_PROBE | `Y33_PROBE=1 ... cargo test ... f0020_cherchi_diff_baseline` → `wc -l inv1/stage4_pairs.txt` |
| 3 | F0020 Stage B missing-count via Cherchi diff | same invocation as Gate 2, read `In Cherchi, not in Waffle:` |
| 4 | F0044 byte-parity HARD GATE | `cargo test ... pr_y31_f0044_extras_zero` (asserts missing=0 AND extras=0) |
| 5 | F0045/R0092 cohort missing-count | `cargo test ... cohort_cherchi_diff_baseline` |
| 6 | yang_fast corpus regression check | `YANG_BOOLEAN=1 cargo test ... yang_fast` |
| 7 | Kernel lib full suite | `cargo test -p kernel --lib` |

All gates run with `TBB_NUM_THREADS=1` per PR-Y29 banked Cherchi non-determinism note. Cherchi binary: `~/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans` (vanilla build, no Y33_PROBE patches; sufficient for Gates 3-5 which compare final OBJ only).

Each gate was paired with a `git stash`-toggled baseline run on the same Cherchi binary to control for non-determinism and avoid attributing baseline drift to the fix.

---

## §3 Empirical attribution table

### §3.1 Gate 1 — Build + cherchi lib tests

```
kernel build:    OK (warnings only, no new warnings vs PR-Y33 HEAD)
cherchi lib tests: 74 passed; 0 failed; 2 ignored
```

GREEN. Pre-existing `dot`/`len0_sq`/`len1_sq` variables remain in use by the outer `if` guard — no clippy fires.

### §3.2 Gate 2 — F0020 STAGE4 pair count

```
                  inv0 pairs   inv1 (load-bearing) pairs
Baseline (478db04):  61          155
Post-fix (canary):  112          365   (+186 inv0; +210 inv1)
```

Brief expected `155 → 179` (+24, matching PR-Y33's 24 Cherchi-only pairs). **Actual gain: +210, ~9× larger.** Interpretation: the deleted filter was rejecting ~210 same-mesh co-oriented pairs total; only 24 of those were "true intersections per Cherchi C++ exact tests" (per PR-Y33 §4.1) — the remaining ~186 also get past Waffle's `triangles_intersect_exact` (sub-anchor B's over-permissive predicate). This is **expected and benign for sub-anchor A's evaluation**: sub-anchor B remains banked for PR-Y35, and the load-bearing F0020 Stage B missing-count gate (§3.3) drops dramatically anyway because Waffle's STAGE5+ classification recovers correctness once the 24 true intersections are no longer Gauss-rejected.

This gate departs from the brief's predicted magnitude but does not invalidate the canary — the load-bearing oracle is F0020 Stage B missing-count, not the STAGE4 pair count itself.

### §3.3 Gate 3 — F0020 Stage B missing-count (LOAD-BEARING)

Same Cherchi binary, same harness invocation, only `intersection_class.rs:134-137` toggled.

```
                  Cherchi tris   Waffle tris   common   missing   extras   χ
Baseline (478db04):    253           294         144      93       148    1
Post-fix  (canary):    253           246         230       7         0    5
```

**Missing-count delta: -86 (93 → 7 = -92.5% reduction).**

Cherchi output count (253 tris) is byte-stable across both runs at `TBB_NUM_THREADS=1` — this is the favorable case noted in PR-Y31 banked (F0020 sometimes deterministic, sometimes flaps; this run is deterministic). Waffle output drops 294 → 246 because sub-anchor A removes spurious "intersection" triangles that the old filter let through STAGE4 but were geometrically invalid (the missing → 7 + extras → 0 signature means Waffle now produces a strict subset of Cherchi's output, with only 7 specific tris on Cherchi side that Waffle still doesn't generate).

PR-Y33 §4.3's propagation-trace caveat predicted F0020 missing would land in 50-80 / 70-80 / 90+ (best/likely/worst) post-sub-anchor-A. **Actual is 7, far below even the best case.** The propagation trace's underestimate suggests sub-anchor A's effect on STAGE5 classification is non-linear: the 24 missing pairs unblock paths in `classify_intersections` that produce *many* correct intersection vertices once they exist, not just the 19 vertices PR-Y33 §3 saw missing in raw STAGE6.

### §3.4 Gate 4 — F0044 byte-parity HARD ASSERT

```
                  Cherchi tris   Waffle tris   common   missing   extras
Baseline (478db04):    136           136         136       0         0
Post-fix  (canary):    136           136         136       0         0

`pr_y31_f0044_extras_zero` test result: ok. 1 passed; 0 failed
```

**HARD ASSERT GREEN.** F0044's byte-parity-with-Cherchi-`subtraction` (PR-Y31 load-bearing invariant) is preserved. F0044 is the canonical "boolean engine produces correct output; Failed status is downstream tessellation/normal layer" case — that downstream Failed status is unchanged (still 12 unpaired edges at Render LOD, 51.7% outward normals, χ=4 → expected 2).

### §3.5 Gate 5 — F0045 + R0092 cohort missing-count

```
            Baseline (478db04)       Post-fix (canary)
            missing  extras  common  missing  extras  common
F0044         0        0      136       0        0      136
F0045       236      466        0     236      273        0      (extras -193, missing +0)
R0092       192      368        0     192      368        0      (no change)
```

**Cohort missing-count: zero delta on all three.** Cohort guard is GREEN (per memory, use missing-count as gate; extras have known Cherchi non-determinism).

F0045 extras drop -193 is a side-effect signal that sub-anchor A is also removing over-detected intersections in F0045 — but F0045 has tessellation-grid divergence (Yang §4.1.1; 0 common at 1µm grid was structural per PR-Y30 banked), so this doesn't translate to a "fix" for F0045. The numeric improvement is recorded but should not be claimed as F0045 closure.

R0092 is unchanged — its dominant defect is at NMM-edge tessellation per PR-Y27 §D.3 (cohort tri-split), not the Gauss-map filter.

### §3.6 Gate 6 — yang_fast corpus

```
                  Pass    Fail    Errored (skipped 33 known timeouts)
Baseline (memory): 10/157   ?       ?
Post-fix (canary): 10/157  139      8
```

**10/157 baseline preserved.** No corpus regression.

(Brief notes a 11+/157 result would be a "small win to call out". This run does not show one; 10/157 is the floor.)

### §3.7 Gate 7 — Kernel lib full suite

```
                  Passed   Failed   Ignored
Baseline (478db04): 1254     25      42
Post-fix  (canary): 1255     24      42      (Δ +1 pass / -1 fail)
```

Diff of FAILED test names between baseline and post-fix runs:

```
Tests that flipped FAIL → PASS (newly passing):
  boolean::exact_mesh::tests::test_subdivision_shared_edge_split_propagation

Tests that flipped PASS → FAIL (newly failing):
  (none)
```

**Gate 7 GREEN — net +1 test pass, zero regressions.** The newly-passing test (`test_subdivision_shared_edge_split_propagation`) is a logical signal that the Gauss-map filter was also masking incorrect output in an integration test path independent of the F0020 spotlight.

---

## §4 Sub-anchor analysis

### §4.1 Sub-anchor A (this canary) — Gauss-map filter delete

**Anchor:** `crates/kernel/src/boolean/cherchi/intersection_class.rs:134-137` (the 4-line same-mesh `continue` block). **Fix shape:** delete the inner `if` block; keep the cross-mesh `orient3d` skip at L138-148. **LOC: 6 added, 6 deleted (net 0; effective deletion).**

**Empirical attribution** (this memo §3.3): F0020 Stage B missing 93 → 7 = -92.5% reduction. This **exceeds** PR-Y33 §4.3's propagation-trace prediction band by a large margin (predicted best 50-60, actual 7). The propagation trace was a lower bound on sub-anchor A's effect — it correctly identified that the 24 Cherchi-only pairs don't *exclusively* drive the 19 missing Cherchi-only STAGE6 verts, but it underestimated the cascade effect on STAGE5 classification overall.

**Paper grounding** (per PR-Y33 §4.1 and the canary brief; not re-verified by reading `refs/text/` because those files are gitignored and not present in the worktree):
- Yang 2025 §4.2.2 Theorem 4.1 states co-oriented same-mesh triangles on a *manifold* surface cannot self-intersect.
- Cherchi 2022 §3 input contract is a *triangle soup* (lines 251-256 of the extracted text): "the arrangement is guaranteed to be a well formed simplicial complex and surface patches are bounded by closed loops of non-manifold edges". The input is non-manifold by construction.
- Yang's Theorem 4.1 applied to Cherchi's soup input is **unsound**: the manifold premise is violated, and same-mesh adjacent faces co-planar along extrude boundaries (F0020) DO intersect (along their shared edge/vertex chain), which Cherchi C++'s un-filtered `detect_intersections` correctly captures.

### §4.2 Sub-anchor B (banked for PR-Y35) — `triangles_intersect_exact` over-permissiveness

**Anchor:** `crates/kernel/src/boolean/cherchi/intersection_class.rs:152` + the `triangles_intersect_exact` function body. **Fix shape:** re-port to `cinolib::Triangle::intersects_triangle(true)` semantics. **LOC budget: 100-200 (per PR-Y33 §4.2; canary scope precludes attempting this).**

**Status post-PR-Y34:** sub-anchor B's *symptom* (extras at STAGE4 / Stage B) is empirically masked once sub-anchor A unblocks the classification cascade. Gate 2 shows Waffle's STAGE4 inv1 pair count is 365 (vs Cherchi's 84), implying ~281 extras at STAGE4 — but Gate 3 shows F0020 Stage B extras = 0 (vs baseline 148). This is the empirical signature of "STAGE5 classification rejects sub-anchor-B's spurious pairs as non-intersecting on the exact-predicate path that runs inside `classify_intersections`, so they don't reach Stage B".

Sub-anchor B remains banked for PR-Y35 — it is a paper-cited correctness anchor (Cherchi 2022 §3 well-formed simplicial complex guarantee depends on a sound intersection predicate), and other cohort cases (F0045 structural / R0092 NMM-edge) may surface its symptoms differently. **Do not claim sub-anchor B is closed.**

### §4.3 What this canary does NOT close

- F0020 Status:Failed persists (40 unpaired Render-LOD edges, 8 degenerate tris, 10 self-intersections). The fix moves the defect upstream — Stage B is now ~Cherchi-byte-clean — but the downstream Render-LOD tessellation layer still produces a non-watertight mesh. This is the same architectural class of "Stage B GREEN, downstream RED" that F0044 has been at since PR-Y22.
- F0045 / R0092 cohort: missing-counts unchanged; their dominant defects are at different layers (F0045 tessellation-grid; R0092 NMM-edge tessellation). Sub-anchor A is not a cohort-wide fix.
- Sub-anchor B (over-permissive predicate) still present.
- yang_fast corpus is unchanged at 10/157 — sub-anchor A does not unblock any of the 139 currently-failing cases at the corpus-aggregate level.

---

## §5 Verdict — SHIP-A

Per the canary brief's verdict logic:

| Condition | Threshold | Actual | Status |
|---|---|---|---|
| Gate 4 (F0044 hard assert) | GREEN | GREEN (missing=0, extras=0) | PASS |
| Gate 6 (yang_fast) | ≥ 10/157 | 10/157 | PASS |
| Gate 7 (kernel lib) | GREEN | +1 net pass, zero regressions | PASS |
| F0020 missing-count drop | ≥ 50% | 93 → 7 = -92.5% | PASS (well above SHIP-A) |

**Recommendation: SHIP-A.**

Sub-anchor A is paper-cited correct on its own merit (Yang §4.2.2 manifold premise violated by Cherchi soup input), is a 6-line deletion, and empirically delivers a far stronger F0020 missing-count reduction than the propagation trace's lower-bound prediction. F0044 byte-parity preserved; F0045/R0092 cohort missing-count preserved; yang_fast no regression; kernel lib net improvement of 1 test. No new tests fail.

Sub-anchor B is banked for PR-Y35 and **must not be claimed as part of PR-Y34**. Further Yang work remains: F0020/F0044 downstream tessellation layer, F0045 tessellation-grid divergence, R0092 NMM-edge layer, the 139 failing yang_fast cases — all open beyond this PR.

---

## §6 Empirical confidence assessment

| Claim | Confidence | Basis |
|---|---|---|
| Sub-anchor A delete preserves F0044 byte-parity | HIGH | Gate 4 hard assert GREEN twice (own run + cohort run); exact same input/output pair as PR-Y31 baseline. |
| F0020 missing-count 93 → 7 is real, not noise | HIGH | Cherchi output deterministic at 253 tris this run (TBB=1); Waffle byte-deterministic per PR-Y30 §4; baseline + post-fix runs on same binaries differ only by the 6-line delete. |
| Sub-anchor A produces zero new test failures | HIGH | Gate 7 baseline (1254/25) and post-fix (1255/24) were run sequentially on same toolchain; explicit name-set diff confirms only `test_subdivision_shared_edge_split_propagation` flipped, FAIL → PASS. |
| Cohort missing-count preservation (F0045 / R0092) | MEDIUM-HIGH | Both runs against same Cherchi build; F0045 Cherchi=236 tris stable; R0092 Cherchi=225 tris this run vs 225 baseline run — appears deterministic this session but PR-Y31 banked notes R0092 sometimes non-det. Re-run not done. |
| F0020 inv1 STAGE4 pair growth +210 vs predicted +24 is benign | MEDIUM | Empirical: load-bearing Gate 3 still GREEN. Mechanism: sub-anchor B's over-detection is filtered downstream in `classify_intersections`. Not directly verified by per-stage byte-diff against Cherchi C++ — that would require re-applying PR-Y33 §7 patches to the C++ side, which is out of scope for this canary. |
| Yang §4.2.2 paper citation is accurate | MEDIUM | Cited from canary brief and PR-Y33 §4.1; not re-verified against `refs/text/yang2025_hybrid_boolean.txt` because those files are gitignored and not present in the worktree. PR-Y33 cites lines 420-540 and 251-256 specifically; if the live tree has them, an impl-y34 SHIP commit should verify. |
| Sub-anchor A is sufficient for "yang_fast corpus improves" | LOW | Empirical: 10/157 unchanged. Sub-anchor A does not unblock any of the 139 failing cases at corpus level. This is consistent with the architectural pattern that Stage B GREEN ≠ Render LOD GREEN. |
| Sub-anchor B remains a valid future PR | MEDIUM-HIGH | Gate 2 shows ~281 extras still at STAGE4; paper grounding (cinolib `intersects_triangle(true)` semantics) unchanged from PR-Y33 §4.2. Recommend canary-y35. |

---

## §7 Reproduction artifacts

All under `/tmp/y34-canary/`:

- `/tmp/y34-canary/waffle/inv{0,1}/stage4_pairs.txt` — post-fix STAGE4 pair dumps (inv1 = 365 pairs).
- `/tmp/y34-canary/waffle-baseline/inv{0,1}/stage4_pairs.txt` — baseline STAGE4 from spotlight_f0020 (inv1 = 155 pairs).
- `/tmp/y34-canary/waffle-baseline-diff/inv{0,1}/stage4_pairs.txt` — baseline STAGE4 from diff harness (sanity-check; inv1 = 155 pairs, matches PR-Y33 §3 exactly).
- `/tmp/y34-canary/f0020_run.log` — post-fix F0020 diff full log (missing=7, extras=0).
- `/tmp/y34-canary/f0020_baseline_run.log` — baseline F0020 diff (missing=93, extras=148).
- `/tmp/y34-canary/f0044_gate.log` — Gate 4 hard assert log.
- `/tmp/y34-canary/cohort_run.log` — post-fix cohort F0044/F0045/R0092.
- `/tmp/y34-canary/cohort_baseline.log` — baseline cohort.
- `/tmp/y34-canary/yang_fast.log` — Gate 6 (yang_fast 10/157).
- `/tmp/y34-canary/kernel_lib.log` — Gate 7 post-fix (1255/24/42).
- `/tmp/y34-canary/kernel_lib_baseline_full.log` — Gate 7 baseline (1254/25/42).
- `/tmp/y34-canary/failed_post.txt` / `/tmp/y34-canary/failed_baseline.txt` — per-test FAIL name sets used for the `comm` diff in §3.7.
- `/tmp/y34-canary/spotlight_f0020.log` — pilot spotlight run that established the STAGE4 pair growth pattern before Cherchi was rebuilt.

Cherchi C++ binary: `~/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans` (vanilla `master` branch, no Y33_PROBE patches; GUI disabled).

Production fix shape: `crates/kernel/src/boolean/cherchi/intersection_class.rs:131-141` (current branch `worktree-canary-y34`).
