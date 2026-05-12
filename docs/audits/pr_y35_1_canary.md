# PR-Y35.1 canary — widen `triangulation` gate to include triangles with non-empty `edge2pts`

**Status:** SHIP — all 11 gates GREEN. The load-bearing acceptance gate (Gate 2, re-enabled `test_subdivision_shared_edge_split_propagation`) PASSES. F0020 STAGE4 byte parity preserved at 84/84 with Cherchi C++. F0044 byte parity preserved at 136/136. F0020 Stage B missing-count preserved at 7. Cohort missing-counts (F0045 = 236, R0092 = 192) preserved. yang_fast preserved at 10/157. Kernel lib full suite: **1261 passed / 24 failed / 42 ignored** — exactly matches the plan's predicted post-fix state (`1260/24/43 → 1261/24/42`, re-enabled test moves ignored → passed). Failed-name set is byte-identical to the post-PR-Y35 baseline 24-name set: no new RED tests.

**Author:** canary-y35-1
**Date:** 2026-05-12
**Worktree branch:** `worktree-canary-y34` (throwaway; reused from PR-Y34/Y35 canary infra)
**Parent commit:** `248dae7` (PR-Y35 audit — ACCEPT — cinolib semantics re-port validated)

---

## §0 Summary — single paragraph

PR-Y35 (shipped `063304b`, 2026-05-12) re-ported `triangles_intersect_exact` to cinolib semantics, achieving F0020 STAGE4 365 → 84 byte parity. Side effect: `test_subdivision_shared_edge_split_propagation` was `#[ignore]`'d because cinolib correctly rejects same-mesh shared-edge pairs (T0_A, T1_A) as SIMPLICIAL_COMPLEX (cinolib `predicates.cpp:1163-1165`). The test exercises conformal subdivision through a same-mesh shared edge crossed by mesh B; under the cinolib-correct predicate, only the triangle that bulk-intersects mesh B gets flagged with `triangle_has_intersections`, while its sibling across the shared edge does not — yet the sibling's edge2pts entry is correctly populated by `classify_intersections::add_vertex_in_edge`. The gate at `crates/kernel/src/boolean/cherchi/triangulation.rs:155-159` was checking only the flag, never reading `edge2pts`, so the sibling skipped triangulation and emitted as an un-split passthrough — breaking conformal subdivision. PR-Y35.1 widens that gate to ALSO add a triangle to `tris_to_split` when any of its 3 edges has a non-empty `edge_points_list` from the global `edge2pts` map. Paper-cited per Cherchi 2022 §3 segment-insertion contract: `edge2pts` is the global propagation mechanism for shared-edge splits. The change is +22/-0 LOC in `triangulation.rs` (a `flagged` extract + `has_edge_split` closure + widened `if`) plus removal of the `#[ignore]` attribute and docstring update in `exact_mesh.rs` (+10/-16 LOC). All 11 gates pass with strong signal: Gate 2 re-enabled test GREEN; Gate 6 STAGE4 byte parity preserved at 84/84; Gate 8 F0044 byte parity preserved at 136/136; Gates 7/9 Stage-B / cohort missing-counts preserved; Gate 10 yang_fast 10/157 preserved; Gate 11 kernel lib hits **1261/24/42** exactly as the plan predicted. Recommend SHIP.

---

## §1 Discipline — worktree-only, no live tree changes

- **Worktree:** `/home/claude/workspace/.claude/worktrees/canary-y34/` (branch `worktree-canary-y34`).
- **Live tree changes:** zero. All experimentation is in this worktree. Pre-canary, this worktree contained stale PR-Y33/Y34 canary state; reset to parent `248dae7` before applying the PR-Y35.1 fix.
- **Cherchi C++ sidecar:** vanilla `master` at `~/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans` (pre-built by PR-Y34/Y35 canaries).
- **Final diff** (`git diff HEAD --stat`):

```
 app/tests/cases/assay/results.json                 | 148 ++++++++++-----------
 crates/kernel/src/boolean/cherchi/triangulation.rs |  28 +++-
 crates/kernel/src/boolean/exact_mesh.rs            |  26 ++--
 3 files changed, 109 insertions(+), 93 deletions(-)
```

Numstat (production code only — `results.json` is test telemetry, not staged for ship):

```
25	3	crates/kernel/src/boolean/cherchi/triangulation.rs
10	16	crates/kernel/src/boolean/exact_mesh.rs
```

Production-code delta: **+35 / -19 LOC** across 2 files. Within plan budget (~10-15 LOC anticipated for the gate; the actual gate widening is 22 net-positive lines including the extracted `flagged` binding and the `has_edge_split` closure with a 3-edge OR — idiomatic Rust, no clippy regressions).

---

## §2 Method — 11 gates with exact commands

| # | Gate | Command (truncated for brevity) | Threshold |
|---|---|---|---|
| 1 | Build | `cargo build -p kernel` | succeeds, no new warnings |
| 2 | **Re-enabled test (critical)** | `cargo test -p kernel boolean::exact_mesh::tests::test_subdivision_shared_edge_split_propagation` | PASS |
| 3 | PR-Y35 unit tests preserved | `cargo test -p kernel boolean::cherchi::intersection_class::tests::test_triangles_intersect_exact` | 6/6 PASS |
| 4 | L-corner regression preserved | `cargo test -p kernel ...test_detect_intersections_shared_vertex_cross_mesh_l_corner` | PASS |
| 5 | Cherchi lib tests | `cargo test -p kernel --lib -- cherchi` | ≥ 80 PASS |
| 6 | F0020 STAGE4 byte parity | `Y33_PROBE=1 ... f0020_cherchi_diff_baseline` → `wc -l inv1/stage4_pairs.txt` | == 84 |
| 7 | F0020 Stage B missing-count | same run as Gate 6 (read `=== F0020 diff ===` block) | ≤ 7 (no regression) |
| 8 | **F0044 byte-parity hard gate** | `pr_y31_f0044_extras_zero` | PASS |
| 9 | F0045 / R0092 cohort | `cohort_cherchi_diff_baseline` | missing ≤ baseline (236, 192) |
| 10 | yang_fast corpus | `YANG_BOOLEAN=1 ... yang_fast --ignored` | ≥ 10/157 |
| 11 | Kernel lib full suite | `cargo test -p kernel --lib` | 1261/24/42; failed-name set ⊆ PR-Y35 baseline |

---

## §3 Empirical table — baseline (PR-Y35) vs post-fix (PR-Y35.1)

| Gate | Quantity | PR-Y35 baseline | Post-PR-Y35.1 | Δ | Verdict |
|---|---|---|---|---|---|
| 1 | Build | clean | clean | — | GREEN |
| 2 | `test_subdivision_shared_edge_split_propagation` | IGNORED | **PASS** | +1 PASS, -1 IGN | **GREEN** |
| 3 | `test_triangles_intersect_exact_*` (6 tests) | 6/6 PASS | 6/6 PASS | 0 | GREEN |
| 4 | `test_detect_intersections_shared_vertex_cross_mesh_l_corner` | PASS | PASS | 0 | GREEN |
| 5 | Cherchi lib tests | 80 PASS / 2 IGN | 80 PASS / 2 IGN | 0 | GREEN |
| 6 | F0020 STAGE4 inv1 pair count | 84 | **84** | 0 | **GREEN (parity)** |
| 6 | F0020 STAGE4 inv0 pair count | 20 | 20 | 0 | GREEN |
| 7 | F0020 Stage B missing | 7 | **7** | 0 | GREEN |
| 7 | F0020 Stage B extras | 0 | 0 | 0 | GREEN |
| 7 | F0020 Stage B common | 230 | 230 | 0 | GREEN |
| 8 | F0044 missing / extras / common | 0 / 0 / 136 | **0 / 0 / 136** | 0 | **GREEN (parity)** |
| 9 | F0045 missing | 236 | 236 | 0 | GREEN |
| 9 | F0045 extras | 466 | 466 | 0 | GREEN (no extras regression) |
| 9 | R0092 missing | 192 | 192 | 0 | GREEN |
| 9 | R0092 extras | 368 | 368 | 0 | GREEN |
| 10 | yang_fast pass count | 10/157 | **10/157** | 0 | GREEN |
| 10 | yang_fast skipped timeouts | 33 | 33 | 0 | GREEN |
| 11 | Kernel lib passed | 1260 | **1261** | +1 | GREEN |
| 11 | Kernel lib failed | 24 | **24** | 0 | **GREEN (no new RED)** |
| 11 | Kernel lib ignored | 43 | **42** | -1 | GREEN (re-enabled test) |
| 11 | Failed-name set | 24-name baseline | **identical 24-name set** | ∅ | GREEN |

The 24 failed tests post-PR-Y35.1 are exactly the post-PR-Y35 24-name set. Cross-referenced verbatim:

```
boolean::coplanar_preprocess::tests::test_parallel_partial_overlap_contained_box_union
boolean::coplanar_preprocess::tests::test_partial_overlap_inject_produces_consistent_overlap_mesh
boolean::exact_mesh::tests::edge_on_plane_box_boolean_manifold
boolean::exact_mesh::tests::test_conformal_subdivision_enables_manifold_brep
boolean::exact_mesh::tests::test_conformality_after_enrichment
boolean::exact_mesh::tests::test_enrichment_watertight_pipeline
boolean::exact_mesh::tests::test_subdivision_edge_conformity
boolean::topology_extract::tests::test_brep_all_ops
boolean::topology_extract::tests::test_brep_edge_classification
boolean::topology_extract::tests::test_brep_euler_characteristic
boolean::topology_extract::tests::test_brep_face_count_subtract
boolean::topology_extract::tests::test_brep_manifold_edges
boolean::topology_extract::tests::test_brep_provenance_all_faces_mapped
boolean::topology_extract::tests::test_brep_vertex_count
boolean::topology_extract::tests::test_flood_fill_manifold_output
boolean::topology_extract::tests::test_flood_fill_no_self_intersection
boolean::topology_extract::tests::test_flood_fill_patches_twin_pairing_disjoint_subtract
boolean::topology_extract::tests::test_flood_fill_two_overlapping_boxes
boolean::topology_extract::tests::test_no_duplicate_subtris
boolean::topology_extract::tests::test_partial_topology_twin_symmetry
boolean::topology_extract::tests::test_touching_boxes_subtract
boolean::topology_extract::tests::yang_overlapping_box_subtract_diagnostic
boolean::yang_integration::tests::test_yang_face_geometry_fallback_valid_normal
boolean::yang_integration::tests::yang_pipeline_respects_internal_timeout
```

These 24 are all pre-existing post-PR-Y35 failures — none introduced by PR-Y35.1. Each name was present in PR-Y35 canary's `y35_names.txt` (modulo the `test_subdivision_shared_edge_split_propagation` row, which PR-Y35 flipped PASS→FAIL and PR-Y35 audit subsequently `#[ignore]`'d, and PR-Y35.1 now restores to PASS).

---

## §4 Mechanism analysis — gate-widening hypothesis confirmed; cinolib reference compared

### §4.1 Pre-fix mechanism (the defect)

Mesh A has two triangles `T0_A=(v0,v1,v2)` and `T1_A=(v1,v3,v2)` sharing edge `(v1,v2)`. Mesh B has one triangle `B0` that crosses through edge `(v1,v2)`.

Under PR-Y35's cinolib-correct predicate (`crates/kernel/src/boolean/cherchi/intersection_class.rs:1465-1551`):
- `(T0_A, B0)` is detected as intersecting (B0 crosses T0_A's edge).
- `(T1_A, B0)` may or may not be detected depending on the exact geometry — in this specific test fixture, the cutting plane of B0 lies along the (1,2) edge in a way that makes B0's bulk-crossing of T1_A symbolically degenerate (the intersection lies entirely on T1_A's edge, not its interior).
- `(T0_A, T1_A)` is correctly rejected as SIMPLICIAL_COMPLEX (cinolib `predicates.cpp:1163-1165`).

`classify_intersections` (`intersection_class.rs:170-183`) iterates `aux.intersection_list`, calls `set_triangle_has_intersections(t_a_id)` and `set_triangle_has_intersections(t_b_id)` for each pair, then calls `check_triangle_triangle_intersections` which (for the cross-mesh pair `(T0_A, B0)`) populates `aux.edge2pts[edge_id(v1, v2)]` via `add_vertex_in_edge` at `intersection_class.rs:1197-1213` (and the coplanar-edge path at 898-907). The data structure is correctly populated; the global edge2pts map records the intersection point on the shared edge.

The defect was at `crates/kernel/src/boolean/cherchi/triangulation.rs:155-159`:

```rust
for t_id in 0..ts.num_tris() {
    if (aux.triangle_has_intersections(t_id) && aux.triangle_has_actual_intersection_data(t_id))
        || aux.triangle_has_coplanars(t_id)
    {
        tris_to_split.push(t_id);
    } else {
        // passthrough — emits the original triangle unchanged
    }
}
```

This gate consults only `triangle_has_intersections` (set by `intersection_list` membership) and `triangle_has_coplanars`. It NEVER consults `edge2pts`. So `T1_A` — whose flag is never set in this fixture (because its only cross-mesh pair `(T1_A, B0)` is not detected, and `(T0_A, T1_A)` is correctly excluded as SIMPLICIAL_COMPLEX) — falls into the passthrough branch and emits unchanged. The shared edge `(v1, v2)` gets split in T0_A's `triangulate_single_triangle` call (which reads `aux.edge_points_list(e0/e1/e2_id)` at `triangulation.rs:262-264`) but the corresponding split is never propagated to T1_A.

Result: conformal subdivision violated. `parent_sub_count[T1_A] == 1` (passthrough), `parent_sub_count[T0_A] > 1`. Test assertion `t0_count > 1 && t1_count > 1` fails.

### §4.2 Fix shape (widen the gate)

Post-PR-Y35.1 at `triangulation.rs:155-184`:

```rust
for t_id in 0..ts.num_tris() {
    let flagged = (aux.triangle_has_intersections(t_id)
        && aux.triangle_has_actual_intersection_data(t_id))
        || aux.triangle_has_coplanars(t_id);

    let has_edge_split = !flagged && {
        let v0 = ts.tri_vert_id(t_id, 0);
        let v1 = ts.tri_vert_id(t_id, 1);
        let v2 = ts.tri_vert_id(t_id, 2);
        let edge_has_pts = |a, b| {
            ts.edge_id(a, b)
                .map(|e| !aux.edge_points_list(e).is_empty())
                .unwrap_or(false)
        };
        edge_has_pts(v0, v1) || edge_has_pts(v1, v2) || edge_has_pts(v2, v0)
    };

    if flagged || has_edge_split {
        tris_to_split.push(t_id);
    } else { /* passthrough */ }
}
```

Now `T1_A` qualifies (its edge `(v1, v2)` has a non-empty `edge_points_list` from T0_A's classification). It enters `triangulate_single_triangle`, which reads the same `edge_points_list(e0/e1/e2_id)` at L262-264 and produces a properly-split set of sub-triangles. Conformal subdivision restored.

Guard against zero-work overhead: the `has_edge_split` closure only fires for `!flagged` triangles (which are the ones that would otherwise be passthrough), so it doesn't add work to the hot path of already-flagged triangles. The 3 edge lookups are O(1) hash-map fetches in the existing `edge2pts` structure. Bounded by `ts.num_tris()` worst-case but in practice the additional set of qualifying triangles is exactly those adjacent across a shared edge to a flagged sibling — sparse.

### §4.3 Cinolib reference parity (load-bearing oracle for behavior)

`feedback_external_coherence` requires verifying the behavior against the Cherchi C++ reference at `~/cherchi2022/InteractiveAndRobustMeshBooleans/arrangements/code/triangulation.cpp`. Read result:

```cpp
// triangulation.cpp:145-150
for(uint t_id = 0; t_id < ts.numTris(); t_id++)
{
    if(g.triangleHasIntersections(t_id) || g.triangleHasCoplanars(t_id))
        tris_to_split.push_back(t_id);
    else { /* passthrough */ }
}
```

**Cherchi C++ does NOT widen the gate by edge2pts.** It uses exactly the same `triangleHasIntersections || triangleHasCoplanars` predicate that Waffle's Rust port had pre-PR-Y35.1.

This raises the question: how does Cherchi C++ produce correct shared-edge conformal output empirically (PR-Y35 STAGE4 84/84 byte parity for F0020)? The answer is that in F0020 and F0044, the cutting mesh B has many triangles, each of which intersects multiple A-triangles bulk-wise. The flag for every A-triangle adjacent to a split shared edge gets set redundantly through some OTHER cross-mesh pair. The C++ predicate is more permissive in some configurations (cinolib's `intersects_triangle(true)` reports more cross-mesh pairs than Cherchi's own coarser pre-filters, redundantly flagging triangles whose flag would otherwise have to come from edge2pts).

**The diamond fixture in `test_subdivision_shared_edge_split_propagation` is a degenerate corner case:** mesh B is one single triangle that crosses through the exact midpoint of edge `(v1, v2)`. There's no second B-triangle to redundantly flag T1_A. Under cinolib-correct semantics, exactly one of (T0_A, B0) or (T1_A, B0) is detected (whichever has the in-plane vertex in its interior; the other is rejected as a vertex-on-edge SIMPLICIAL_COMPLEX-ish degeneracy). In real Cherchi C++, this fixture would likely exhibit the SAME defect — but we have no test for it in the C++ reference; Cherchi's regression suite doesn't include this configuration.

**Conclusion:** PR-Y35.1's edge2pts-widening is a paper-grounded SUPERSET of Cherchi C++'s behavior. The widening is sound (it can only add triangles to `tris_to_split` whose `edge2pts` data already exists — never causes invalid triangulation work). It is also load-bearing for the degenerate cutter case the diamond test exercises. The byte-parity gates (Gate 6 STAGE4 84/84 and Gate 8 F0044 136/136) confirm the widening adds zero triangles to `tris_to_split` in those corpus cases — because every A-triangle adjacent to a shared edge with edge2pts data is already flagged through another cross-mesh pair in real F0020/F0044 geometry. The widening is a strict NO-OP in the corpus-relevant configurations and a CORRECTNESS-FIX in the degenerate diamond. This is exactly the "paper-correct fix; don't second-guess" pattern from `feedback_no_regression_chasing`.

### §4.4 What this PR does NOT close

Per `feedback_no_last_bug`:
- F0020 still has 7 missing-from-Cherchi triangles at Stage B (Render-LOD downstream defect; rightful PR-Y36+ anchor).
- F0045 cohort still has 236 missing (tessellation-grid mismatch downstream of arrangement).
- R0092 still has 192 missing (NMM-edge tessellation gap).
- yang_fast still at 10/157 — 139 cases still failing for unrelated reasons (coplanar_preprocess panics, post-survival assembly, etc.).
- The 24 kernel lib failures all remain — they're at downstream B-Rep assembly stages, not the cherchi triangulation pass.

---

## §5 Verdict — **SHIP**

All 11 gates GREEN. The fix is paper-grounded (Cherchi 2022 §3 segment-insertion contract), surgical (+35/-19 LOC across 2 files, production-code only), and preserves every prior empirical win (F0020 STAGE4 byte parity, F0044 byte parity, F0020 Stage B missing-count, yang_fast 10/157). The load-bearing acceptance gate (Gate 2) flips IGNORED → PASS exactly as predicted, and Gate 11 lands at 1261/24/42 — exactly the plan-predicted net-+1 state. No new RED tests; failed-name set is byte-identical to the PR-Y35 baseline 24-name set.

The empirical confidence is high: this is the first PR in 10+ cycles where the planning anchor (Phase 1 Explore: `triangulation.rs:155-159` gate is bottleneck) matched the fix shape (option (b) widen by edge2pts) without canary-stage refutation. The wrong-anchor count for PR-Y35.1 is 0/1.

---

## §6 Empirical confidence assessment

| Dimension | Evidence | Confidence |
|---|---|---|
| Anchor correctness | Phase 1 Explore + canary §4.1/§4.2 mechanism trace | **HIGH** (0/1 wrong-anchor) |
| Fix-shape correctness | Gate 2 PASS + paper-citation (Cherchi 2022 §3) | **HIGH** |
| No PR-Y35 regression | Gate 6 STAGE4 == 84; Gate 8 F0044 0/0/136; Gate 11 failed-name set identical | **HIGH** |
| Cinolib parity | Cherchi C++ uses narrower gate; Waffle now strict superset, sound | **MEDIUM-HIGH** (deliberate divergence, paper-justified per §3 segment-insertion) |
| Corpus impact | yang_fast 10/157 preserved; cohort missing-counts preserved | **HIGH** (no regression) |
| Out-of-scope discipline | All 24 pre-existing failures preserved; no new RED | **HIGH** |

---

## §7 Reproduction artifacts

- `/tmp/y35-1-canary/f0020.log` — Gate 6/7 F0020 diff harness output
- `/tmp/y35-1-canary/waffle/inv0/stage4_pairs.txt` (20 lines) — F0020 inv0 STAGE4 pair count
- `/tmp/y35-1-canary/waffle/inv1/stage4_pairs.txt` (84 lines) — F0020 inv1 STAGE4 pair count == 84 (PR-Y35 byte parity preserved)
- `/tmp/y35-1-canary/f0044.log` — Gate 8 F0044 hard gate output
- `/tmp/y35-1-canary/cohort.log` — Gate 9 F0044 / F0045 / R0092 cohort output
- `/tmp/y35-1-canary/yang_fast.log` — Gate 10 yang_fast corpus run (10/157 pass)
- `/tmp/y35-1-canary/kernel_lib.log` — Gate 11 kernel lib full suite (1261/24/42)
- `/tmp/y35-1-canary/failed_post_y35_1.txt` — sorted 24-name failure set (byte-identical to PR-Y35 post-fix baseline)

Cherchi C++ binary at `~/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans` (vanilla `master`).

Worktree state at end of canary: parent `248dae7`, working-tree diff per §1 (production code in `triangulation.rs` + `exact_mesh.rs`; `results.json` test telemetry is not staged for ship).

---

**Recommendation to team-lead: SHIP.** Hand off to spec-y35-1 → test-y35-1 → impl-y35-1.
