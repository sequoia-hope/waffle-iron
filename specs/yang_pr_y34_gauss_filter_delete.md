# PR-Y34 — Delete Yang §4.2.2 Gauss-map filter same-mesh shortcut from Cherchi-Rust port

**Authors:** spec-y34 (Spec Writer), canary-y34 (Empirical Canary)
**Parent:** `478db04` (PR-Y33 SHIPPED, infrastructure-only)
**Date:** 2026-05-12
**Status:** Spec (FIP §3); Phase 4 (test) / Phase 5 (impl) pending

---

## §1 Context

Waffle's Cherchi-Rust mesh-arrangement port (`crates/kernel/src/boolean/cherchi/`) carries a Yang 2025 §4.2.2 Theorem 4.1 Gauss-map filter at `intersection_class.rs:117-150` that the upstream Cherchi 2022 C++ reference does **not** carry. PR-Y33 (`docs/audits/pr_y33_per_stage_canary.md`) localized F0020's 93-tri missing-from-Cherchi defect to STAGE4 (`detect_intersections`) via per-stage byte-diff and split the cause into two sub-anchors:

- **Sub-anchor A** — same-mesh `continue` at `intersection_class.rs:134-137`. 24/24 of Waffle's STAGE4 missed pairs are attributable to this skip.
- **Sub-anchor B** — over-permissive `triangles_intersect_exact` predicate. 95 Waffle-extra pairs at STAGE4.

PR-Y34 ships **sub-anchor A only** as a ~6-line deletion. Sub-anchor B is banked for PR-Y35. The cross-mesh `orient3d`-based skip at L138-148 — manifoldness-agnostic, paper-independent — is retained.

---

## §2 Why — Cherchi 2022 §3 soup-input contract violates Yang's manifold premise

The same-mesh skip is unsound for Cherchi's input contract:

**Yang 2025 §4.2.2 Theorem 4.1** (`refs/text/yang2025_hybrid_boolean.txt:440-466`) frames Gauss-map filtering as a redundancy reducer for cross-surface tests, with co-oriented normals being a sufficient skip condition when the input is a **manifold surface**. Yang's §4.2 pipeline operates on tessellated NURBS patches, each a manifold (lines 449-461). The same-mesh case Yang's filter targets is essentially "two co-oriented triangles of the same manifold surface do not intersect each other beyond their shared edge" — true for the closed manifold premise.

**Cherchi 2022 §3** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:249-256`) is explicit that the arrangement input is a **triangle soup**, not a manifold:

> "the result of a Boolean operation between two watertight manifold meshes that do not touch tangentially is guaranteed to be manifold watertight. Properties of this kind may be relevant also for downstream applications, regardless of the geometric degeneracies that snap rounding may introduce in the output."

And again at line 295-298:

> "From the perspective of the arrangement algorithm, the input meshes M1, M2, ..., Mn can be seen as a soup of possibly intersecting triangles."

Cherchi C++ runs **no Gauss-map filter** in `detectIntersections` (`InteractiveAndRobustMeshBooleans/intersection_classification.cpp:85-95` per PR-Y33 §4.1 verification): the C++ reference performs plain AABB-overlap + exact tri-tri intersection. There is no co-oriented-same-mesh shortcut.

**The canonical failure case is F0020** — a 3-extrude solid. Its tessellation contains co-planar same-mesh face pairs along extrude boundaries: adjacent quad sub-triangles share an edge, are co-oriented, and DO intersect in soup-space (along that shared edge/vertex chain). Cherchi C++'s un-filtered detection captures these pairs as required by §3's well-formed-simplicial-complex guarantee; Waffle's Yang-derived skip silently drops them, producing a downstream classification cascade that costs F0020 93 missing triangles by Stage B.

The same-mesh skip is a **port deviation from the load-bearing reference**, not a Yang-paper-faithful improvement. Reverting Waffle to reference-parity at this anchor is what PR-Y34 ships.

---

## §3 Fix shape

Delete the 4-line same-mesh `continue` block; retain the cross-mesh `orient3d` skip. Verbatim diff from canary §1 (canary memo "Production fix shape"):

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

LOC delta: 6 added, 6 deleted (net 0). Only logical change: deletion of `if ts.tri_label(t0) == ts.tri_label(t1) { continue; }`.

---

## §4 Empirical evidence (from canary, not re-run)

Source: `docs/audits/pr_y34_canary.md` §3.

### §4.1 F0020 Stage B missing-count (load-bearing, Gate 3)

|                     | Cherchi tris | Waffle tris | common | missing | extras |
|---------------------|--------------|-------------|--------|---------|--------|
| Baseline `478db04`  | 253          | 294         | 144    | **93**  | 148    |
| Post-fix (canary)   | 253          | 246         | 230    | **7**   | 0      |

**Missing-count: 93 → 7 (−92.5%)**. Extras: 148 → 0. This exceeds PR-Y33 §4.3's propagation-trace lower-bound prediction (best ~50-60); the cascade effect on STAGE5 classification is non-linear.

### §4.2 F0044 byte-parity preserved (Gate 4, HARD ASSERT)

|                     | Cherchi tris | Waffle tris | common | missing | extras |
|---------------------|--------------|-------------|--------|---------|--------|
| Baseline `478db04`  | 136          | 136         | 136    | 0       | 0      |
| Post-fix (canary)   | 136          | 136         | 136    | 0       | 0      |

`cherchi_differential_diff::pr_y31_f0044_extras_zero` PASS (asserts `missing == 0 && extras == 0`). This is the PR-Y31 load-bearing invariant for "boolean engine produces correct output."

### §4.3 yang_fast corpus unchanged (Gate 6)

`YANG_BOOLEAN=1 cargo test ... yang_fast`: **10/157 pass** (baseline = 10/157). No corpus regression. Sub-anchor A does not unblock any of the 139 failing cases at the corpus-aggregate level — the F0020 fix moves the defect from Stage B upstream to the Render-LOD tessellation layer (same architectural class as F0044 since PR-Y22).

### §4.4 Kernel lib full suite (Gate 7, net +1 pass)

|                     | Passed | Failed | Ignored |
|---------------------|--------|--------|---------|
| Baseline `478db04`  | 1254   | 25     | 42      |
| Post-fix (canary)   | 1255   | 24     | 42      |

The single test that flipped FAIL → PASS is `boolean::exact_mesh::tests::test_subdivision_shared_edge_split_propagation` (see §5). Zero new failures.

### §4.5 Cohort missing-count (Gate 5)

| Case  | Baseline missing | Post-fix missing | Delta |
|-------|------------------|------------------|-------|
| F0044 | 0                | 0                | 0     |
| F0045 | 236              | 236              | 0     |
| R0092 | 192              | 192              | 0     |

Cohort missing-counts preserved (per banked guidance, missing-count is the deterministic gate; extras flap on Cherchi non-determinism). F0045 extras drop -193 is incidental and **not** claimed as F0045 closure.

---

## §5 Regression coverage — existing test pins the path

PR-Y34 introduces **no new test file**. Regression coverage is provided by an existing kernel lib unit test that was failing on parent `478db04` and passes post-fix:

**Test:** `boolean::exact_mesh::tests::test_subdivision_shared_edge_split_propagation`
**Location:** `crates/kernel/src/boolean/exact_mesh.rs:5403-5469`

The test constructs a minimal scenario that exercises the exact path the Gauss-map same-mesh skip blocked:

- Mesh A has two triangles `T0 = [0,1,2]` and `T1 = [1,3,2]` sharing edge `(1,2)`. Both triangles are co-oriented (both lie in the z=0 plane with the same winding), so they have **identical normals** — the precondition for the deleted same-mesh `continue` to fire.
- Mesh B is a single cutting triangle that crosses through the shared edge `(1,2)` near y=0.
- The test asserts that **both** `T0` and `T1` (parents that share the intersected edge) produce more than one sub-triangle — i.e. that the subdivision is conformal across the shared edge.

Mechanism: on parent `478db04`, `detect_intersections` is asked to consider the pair `(T0, T1)` in mesh A. They have the same mesh label and co-oriented normals → the deleted skip at L134-137 fires → the pair is dropped from `intersection_list`. Downstream, when mesh B's cut splits edge `(1,2)`, only one of the two same-mesh parents picks up the split point, leaving the other unsubdivided. The assertion `t0_count > 1 && t1_count > 1` fails on baseline.

Canary observation (§3.7): on parent `478db04` this test was in the FAILED set (1254 pass / 25 fail); post-fix it is in the PASSED set (1255 pass / 24 fail). The single name that moved across the diff is precisely `test_subdivision_shared_edge_split_propagation`.

**FIP §4 ("failing tests before impl") is satisfied** by this existing test — it was RED on parent, will be GREEN post-impl. No new `tests/pr_y34_gauss_skip_deleted.rs` test file is required for regression coverage.

(If a finer-grained STAGE4 pair-count assert under `Y33_PROBE` is desired as defense-in-depth, that is a Phase-4 implementer decision; this spec does not mandate it.)

---

## §6 Out of scope

- **Sub-anchor B** (`triangles_intersect_exact` over-permissiveness). Banked for PR-Y35. Gate 2 of the canary shows ~281 extra STAGE4 pairs persist post-PR-Y34, but their symptoms are masked at Stage B by `classify_intersections`. PR-Y35 must canary independently.
- **F0020 Render-LOD downstream Failed status.** F0020's Spotlight `Status: Failed` persists (40 unpaired Render-LOD edges, 8 degenerate tris, 10 self-intersections). The fix moves the F0020 defect from Stage B to Render LOD — same architectural class as F0044's post-PR-Y22 state. Out of scope.
- **F0045 / R0092 cohort defects.** Both unchanged. F0045's dominant defect is tessellation-grid divergence (per PR-Y30 banked); R0092's is NMM-edge tessellation (per PR-Y27 §D.3). Sub-anchor A is not a cohort-wide fix.
- **yang_fast corpus aggregate (10/157).** Unchanged. PR-Y34 does not unblock any of the 139 currently-failing cases.

PR-Y34 closes one paper-cited port deviation. Other Yang work remains open. This is not "the fix"; it is **one** of many remaining gaps.

---

## §7 Risk and mitigation

**Risk:** removing the same-mesh skip means same-mesh co-oriented triangle pairs in **other** corpus cases will now flow through `triangles_intersect_exact` and downstream classification. Pairs that were silently dropped pre-PR-Y34 may now exercise paths previously dead in tested corpus state.

**Mitigation (from canary, no re-run required):**

1. **Gate 7 kernel lib full suite:** 1254/25 → 1255/24 with explicit name-set diff. Only `test_subdivision_shared_edge_split_propagation` moved (FAIL → PASS). Zero PASS → FAIL transitions in 953-baseline kernel coverage including all yang_integration tests.
2. **Gate 6 yang_fast corpus:** 10/157 preserved. The 156 randomized .waffle cases (mix of boolean / sketch / extrude scenarios from seed 42) exercise the full Yang pipeline including `detect_intersections`. No regression.
3. **Gate 5 cohort missing-count:** F0044 / F0045 / R0092 missing-counts preserved (Δ = 0 on all three).

**Residual risk:** cases not exercised by the kernel lib suite, yang_fast corpus, or cohort diff harness. The diff harness only runs on F0020 / F0044 / F0045 / R0092; broader corpus diff vs Cherchi C++ is not part of CI. Per FIP §5, adversarial validation (Phase 6) should sample 5 additional R-series / F-series cases for Stage B missing-count regression check before SHIP.

---

## §8 Citations

- **Yang 2025 §4.2.2 Theorem 4.1:** `refs/text/yang2025_hybrid_boolean.txt:440-466` (manifold-premise Gauss-map filter).
- **Cherchi 2022 §3 soup-input contract:** `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:249-256` (well-formed simplicial complex; non-manifold edges).
- **Cherchi 2022 §3 arrangement input definition:** `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:295-298` ("soup of possibly intersecting triangles").
- **Cherchi C++ reference (no Gauss-map filter):** `InteractiveAndRobustMeshBooleans/intersection_classification.cpp:85-95::detectIntersections` (per PR-Y33 §4.1; AABB + exact tri-tri only, no normal-based skip).
- **PR-Y33 per-stage byte-diff:** `docs/audits/pr_y33_per_stage_canary.md` (24/24 missed pairs attributed to same-mesh skip via `check_gauss_filter.py`).
- **PR-Y34 canary memo:** `docs/audits/pr_y34_canary.md` (7 gates GREEN, F0020 93 → 7, F0044 byte-parity preserved, kernel lib +1 net).
- **Regression test:** `crates/kernel/src/boolean/exact_mesh.rs:5403-5469` (`test_subdivision_shared_edge_split_propagation`).
- **Target code:** `crates/kernel/src/boolean/cherchi/intersection_class.rs:117-150` (Gauss-map filter block, same-mesh shortcut at L134-137 of pre-fix tree).
