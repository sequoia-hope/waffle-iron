# Cherchi 2020/2022 Port Audit (Rust vs C++ Upstream)

**Audit date**: 2026-04-28
**Branch**: `cherchi-port-audit`
**Method**: 4-auditor team, side-by-side Rust vs C++ comparison, file-by-file

## Header — sources audited

**Rust (this codebase)**:
- `crates/kernel/src/boolean/cherchi/{mod,processing,triangulation,intersection_class,fast_trimesh,aux_structure,common,tree,triangle_soup}.rs` (9 files, 7,378 lines)
- Cherchi-2022-relevant call graph in `boolean/exact_mesh.rs::label_sub_tri_raycast` and helpers (Algorithm 1 ray-cast in/out)
- `boolean/topology_extract.rs::flood_fill_patches` and `boolean/mesh_arrangement.rs::LocalMesh` (integration surface)

**C++ upstream** (cloned to `/tmp/cherchi*-cpp/`):
- `gcherchi/InteractiveAndRobustMeshBooleans` (Cherchi 2022) — SHA `7bd6c2697695f555f9a15d7e154e084d841316f0` — files: `booleans.cpp/.h`, `foctree.cpp/.h`
- `gcherchi/FastAndRobustMeshArrangements` (Cherchi 2020, maintained codebase including 2022 improvements) — SHA `bf7eb71da991a61ff5414946a4b2754bbd327e41` — files: `intersection_classification.{cpp,h}`, `aux_structure.{cpp,h}`, `fast_trimesh.{cpp,h}`, `processing.{cpp,h}`, `triangle_soup.{cpp,h}`, `triangulation.{cpp,h}`, `tree.h`, `common.h`, `solve_intersections.{cpp,h}`

**Papers anchored**:
- Cherchi/Livesu/Scateni/Attene **2020** — *"Fast and Robust Mesh Arrangements using Floating-point Arithmetic"* (SIGGRAPH Asia 2020). Indirect predicates + arrangement.
- Cherchi/Pellacini/Attene/Livesu **2022** — *"Interactive and Robust Mesh Booleans"* (SIGGRAPH Asia 2022). Boolean pipeline + ray-cast in/out (Algorithm 1, §5).
- Livesu/Cherchi/Pellacini/Attene **2021** — *"Deterministic Linear Time Constrained Triangulation Using Simplified Earcut"* (TVCG 2021). CDT replacement for earcut.
- Yang et al. **2025** — *"A Hybrid Boolean Algorithm for B-Reps and Mesh"* — cites Cherchi 2022 §4.2 (mesh intersection) and §4.4.2 (in/out classification).

## Executive Summary

This audit was prompted by the just-merged share-vertex-skip fix (commit `52e28b8`),
where a single deviation between our Rust port and Cherchi 2022's C++ upstream
broke F0004's L-corner intersections. That fix advanced the Yang assay from 7/157
→ 8/157 passing. The user authorized this systematic sweep to find similar
deviations before chasing more individual failure signatures.

**Result: 42 substantive findings across 4 audit slices.** Three dominant themes
account for the majority. The Yang assay gain potential from systematically
fixing CORRECTNESS-BUG findings is significant: the share-vertex skip alone
unblocked 3 cases (F0003, R0018, F0004 progressed). The 12 CORRECTNESS-BUG
findings catalogued here represent comparable-or-larger unblock potential.

### Severity counts

| Severity | Count |
|----------|-------|
| **CORRECTNESS-BUG** | 12 |
| **UNKNOWN-NEEDS-INVESTIGATION** | 12 |
| **DELIBERATE-DIVERGENCE** | 11 |
| **PERFORMANCE-DRIFT** | 7 |
| **Total** | **42** |

### Counts by file

| File | Findings | CORRECTNESS-BUG |
|------|----------|----------------|
| `processing.rs` | 4 (A-01, A-04, A-05, A-06) | 1 (A-01) |
| `triangle_soup.rs` | 2 (A-02, A-03) | 1 (A-02) |
| `intersection_class.rs` | 9 (B-01..B-07, B-11, B-13, B-14) | 1 (B-06) |
| `aux_structure.rs` | 4 (B-08..B-10, B-12) | 0 |
| `triangulation.rs` | 5 (C-01..C-03, C-05, C-06, C-07, C-10) | 4 (C-01, C-02, C-05, C-10) |
| `fast_trimesh.rs` | 4 (C-08, C-09, C-11, C-13) | 2 (C-08, C-09) |
| `cherchi/mod.rs` | 2 (D-02, D-04) | 0 |
| `boolean/exact_mesh.rs` | 6 (D-05..D-10) | 3 (D-05, D-07, D-10) |
| `boolean/topology_extract.rs` | 2 (D-11, D-12) | 0 |
| `boolean/mesh_arrangement.rs` | 1 (D-13) | 0 |
| `cherchi/{common,tree}.rs` | 0 | 0 |

### Verified-and-OK areas (no deviations found)

- `cherchi/tree.rs` — full file, faithful 1:1 port (calibration target; all 4 auditors verified independently).
- `cherchi/common.rs` — full file, faithful port.
- `cherchi/triangulation.rs::triangulation` outer driver (lines 112-226).
- `cherchi/triangulation.rs::triangulate_single_triangle` outer flow (238-358), `boundary_walker`/`boundary_walker_reverse` (944-1085).
- `cherchi/triangulation.rs::earcut_linear` (1100-1197) — faithful port of Livesu et al. 2021 linear-time CDT, uses `orient2d_indirect` (correctness improvement over the C++ `customOrient2D`).
- `cherchi/triangulation.rs::earcut` legacy (1199-1272) — never called from the active path; dead-code-flagged in both Rust and C++.
- `cherchi/triangulation.rs::compute_triangle_of_segment`, `solve_pockets_in_coplanar_triangle`, `find_pockets_in_triangle`.
- `cherchi/fast_trimesh.rs` accessors: `adj_v2t`, `tri_id`, `edge`/`edge_vert_id`/`edge_is_constr`/`edge_is_manifold`, `tri_vert_id`, `tri_node_id`/`set_tri_node_id`, `tri_vert_offset`, `tri_verts_are_ccw`, `flip_tri`.
- `cherchi/aux_structure.rs::add_vertex_in_sorted_list` — faithful port of `addVertexInSortedList`. (Note: properly used, but a missing-call site is C-10.)

## Cross-Slice Cluster Themes

Three dominant patterns emerged across multiple slices. These are higher-leverage
than individual findings: a single root-cause fix resolves multiple symptoms.

### Cluster I: Predicate-kernel symptom-paper-over (8 findings)

**Members**: A-01, A-02, B-06, C-01, C-02, C-05, C-07, C-08, C-09, C-11, C-13.

The Rust port has a recurring pattern of adding defensive guards
(skip-degenerate, skip-already-vertex, return-zero-on-fail, accept-non-manifold)
that mask a different upstream bug — namely, the predicate kernel sometimes
produces inexact comparisons that violate the algorithm's
freshness/manifoldness/non-degenerate invariants.

Each individual guard is a CORRECTNESS-BUG by the rubric's definition (silent
data loss / arbitrary recovery), but the *root cause* is in the predicate kernel
and in two non-exact-arithmetic helper functions:
- `processing.rs::points_are_collinear_3d` (A-01) uses inexact f64 cross-product where C++ uses Shewchuk's exact `orient2d`.
- `triangle_soup.rs::max_component_in_triangle_normal` (A-02) uses inexact f64 cross-product where C++ uses cascaded filtered/exact predicates.

The right remediation per `feedback_yang_only.md` is: (1) replace A-01/A-02 with exact-arithmetic predicates; (2) convert each defensive guard to `debug_assert!`; (3) run the assay and identify which inputs trigger each; (4) bug-hunt the remaining predicate path that produces the impossible state; (5) fix at root and remove the defenses.

**Worked example**: C-09 (`add_tri returns 0 on degenerate`) silently corrupts mesh state in `split_edge_with_tree` / `split_tri_with_tree` via `set_tri_node_id(0, …)`. This is the highest-impact instance and possibly the most concrete CORRECTNESS-BUG in the entire audit.

### Cluster II: SimplexIntersection state-space collapse (5 findings)

**Members**: B-03, B-04, B-05, B-12, B-14.

cinolib's `SimplexIntersection` enum has four states:
`DO_NOT_INTERSECT`, `SIMPLICIAL_COMPLEX`, `INTERSECT`, `OVERLAP`.

Our Rust `SegmentIntersection` enum collapses this to two: `DoNotIntersect`,
`Intersect`. The intermediate states (where two simplices share a vertex/edge
in a *valid* way that's NOT a real interior crossing) are conflated with
`Intersect`, leading to over-detection. Multiple workarounds (B-08
`triangle_has_actual_intersection_data`, B-06 soft-asserts in
`finalize_intersection`, B-13 `compute_lpi_coords` parallel-fallback) exist to
paper over the resulting over-detection downstream.

**Root fix**: port cinolib's 4-state `SimplexIntersection` enum + exact-arithmetic
predicate flow. Eliminates 5+ findings from this cluster simultaneously.

**Possible explanation for R0080 regression**: Post share-vertex-skip removal,
some pairs that cinolib would have classified `SIMPLICIAL_COMPLEX` (skip) are
now flagged `Intersect` by our 2-state enum, producing inconsistent intersection
sets. R0080's `no_self_intersection: face pairs (1,4)` regression (commit
`a125736`) is consistent with this hypothesis.

### Cluster III: Jolly-point eager-append cascade (3 findings)

**Members**: A-03, A-06, B-12.

C++ pipeline appends 5 utility "jolly points" to the vertex array ONLY at the
end (`solve_intersections.cpp:70 ts.appendJollyPoints()` after triangulation).
During detect/classify/triangulate, `ts.numVerts()` returns just the original +
intersection vertices.

Our Rust pipeline appends jolly points eagerly in `TriangleSoup::new`
(`triangle_soup.rs:124-127`). Cascading consequences:
- A-03: intersection-vertex IDs shift by +5 vs C++.
- A-06: `compute_approximate_coordinates` skips the wrong 5 (drops last
  intersections instead of jollies; downstream `mod.rs:182-221` papers over
  with a misleading `num_non_jolly` variable name).
- B-12: `aux_structure.rs::init_from_triangle_soup` populates `v_map` with
  jolly-point keys, contaminating `add_vertex_in_sorted_list` dedup. Geometric
  collision between a real intersection and a jolly-point coordinate is
  improbable but constructible.

**Root fix**: revert to C++ ordering — append jolly points only after
triangulation completes. Eliminates all three findings.

## Findings A — Preprocessing & Soup (auditor-a)

### A-01 — `points_are_collinear_3d` uses inexact f64 cross-product

**Severity**: CORRECTNESS-BUG. Cluster I (predicate-kernel).

**Rust**: `crates/kernel/src/boolean/cherchi/processing.rs:374-389`
**C++**: `cherchi2020-cpp/code/processing.cpp:144` (calls `cinolib::points_are_colinear_3d` → uses Shewchuk exact `orient2d` on three orthogonal projections)

```rust
fn points_are_collinear_3d(a: &[f64; 3], b: &[f64; 3], c: &[f64; 3]) -> bool {
    let ux = b[0] - a[0]; let uy = b[1] - a[1]; let uz = b[2] - a[2];
    let vx = c[0] - a[0]; let vy = c[1] - a[1]; let vz = c[2] - a[2];
    let cx = uy * vz - uz * vy;
    let cy = uz * vx - ux * vz;
    let cz = ux * vy - uy * vx;
    cx == 0.0 && cy == 0.0 && cz == 0.0
}
```

C++ uses Shewchuk's exact `orient2d` on three orthogonal projections
(`p_dropX`, `p_dropY`, `p_dropZ`). Cross-product f64 computation produces
tiny non-zero residuals for points that are mathematically collinear but
arose from inexact arithmetic; Rust will fail to detect those as
degenerate, while C++'s exact `orient2d` returns 0.

**Severity test**: Three points where exact orient2d gives 0 but f64
cross-product gives ~1e-16. C++ removes the degenerate triangle; Rust keeps
it and propagates to intersection detection.

**Suggested fix**: Replace with three calls to `geometry-predicates::orient2d`
on the three orthogonal projections, AND-ing the `== 0` results.

**Paper citation**: Cherchi 2020 §3 (robustness via cascaded filtered/exact predicates).

---

### A-02 — `max_component_in_triangle_normal` inexact

**Severity**: CORRECTNESS-BUG. Cluster I.

**Rust**: `triangle_soup.rs:363-383`
**C++**: `cherchi2022-cpp/arrangements/external/Indirect_Predicates/include/implicit_point.hpp:1024-1029` — `genericPoint::maxComponentInTriangleNormal`

The Plane returned drives 2D projection for ALL downstream orientation tests on this triangle — picking the wrong projection axis can flip orient2d signs. C++ uses cascaded filtered (epsilon `8.88395e-016 * max_var^2`) followed by exact-arithmetic fallback. Rust uses a single non-exact f64 cross product.

Additional independent contribution: C++ uses edges `(v1→v2)+(v2→v3)` (common endpoint v1); Rust uses `(v0→v1)+(v0→v2)` (common endpoint v0). Algebraically equal, but f64 round-off paths differ.

**Suggested fix**: Port `maxComponentInTriangleNormal_filtered` + `_exact` from `implicit_point.hpp:937-1022` using `geometry-predicates`'s expansion arithmetic, dispatch via the same filtered-then-exact cascade.

---

### A-03 — Jolly points appended eagerly at TriangleSoup construction

**Severity**: UNKNOWN-NEEDS-INVESTIGATION. Cluster III. Confirmed independently as D-03.

**Rust**: `triangle_soup.rs:121-127` (`TriangleSoup::new`)
**C++**: `cherchi2020-cpp/code/triangle_soup.cpp:369-376` (`appendJollyPoints` separate; called from `solve_intersections.cpp:70`)

In Rust, `ts.num_verts()` includes 5 jolly points throughout detect/classify/triangulate. In C++ jolly points only enter `vertices` after triangulation. Effect: intersection-vertex IDs shift by +5 vs C++. Downstream consumers that compare against `num_orig_vtxs` would be off by 5.

**Cascading findings**: A-06 (compute_approximate_coordinates layout), B-12 (v_map contamination).

**Suggested fix**: Revert to C++ ordering — call equivalent of `append_jolly_points()` from `solve_intersections.rs::solve_intersections` after `triangulation_with_parents`.

---

### A-04 — `Orientation` cosurface tracking added (PR10)

**Severity**: DELIBERATE-DIVERGENCE.

**Rust**: `processing.rs:230-329` (`remove_degenerate_and_duplicated_triangles` extended return tuple)
**C++**: `processing.cpp:125-173` (returns void; cosurface info derived elsewhere via `dupl_triangles` in `booleans.cpp:185-218`)

PR10 enhancement tracking Parallel/AntiParallel cosurface orientation when triangles merge during dedup. Cited inline at `processing.rs:42-46` and `processing.rs:236-242`. Cherchi 2020 §5.4 + Hoffmann 1989 §5.3.

**Suggested**: Optionally add explicit "PR10/A15.6" reference in the inline comment so future audits don't reflag.

---

### A-05 — `compute_multiplier` Rust silently fixes C++ UB

**Severity**: UNKNOWN-NEEDS-INVESTIGATION.

**Rust**: `processing.rs:77-109`
**C++**: `processing.cpp:47-64`

C++ uses `int` shift `1 << e`; for `e ≥ 31` this is signed-int undefined behavior. Typical CAD inputs have `e ≈ log2(R/1) ≈ 33`, so `1 << 33` triggers UB on every typical call. The C++ `if(multiplier < 0) multiplier = 1.0; // temporary fix` fallback explicitly admits this.

Rust uses `(1u64 << e.min(62))` which is well-defined and produces `2^e`. Rust is **strictly more correct** than upstream — but downstream predicates may be calibrated to C++'s actual UB-induced 1.0 multiplier rather than the well-defined 2^e.

**Suggested fix**: Write a unit test asserting `compute_multiplier_flat([1.0, ...])` returns `2^33`. Run upstream binary on the same input and capture C++ reference behavior. Resolve direction once both are measured.

---

### A-06 — `compute_approximate_coordinates` layout off by 5

**Severity**: UNKNOWN-NEEDS-INVESTIGATION. Cluster III with A-03.

**Rust**: `processing.rs:338-367`
**C++**: `processing.cpp:186-210`

Skipping the last 5 vertices (intended to drop jollies) drops intersection vertices in Rust due to A-03's eager append. Downstream `mod.rs:188` has a misleadingly-named `num_non_jolly` variable that actually equals `len-5`, not "non-jolly".

**Suggested fix**: Resolves naturally with A-03 fix.

## Findings B — Intersection Classification (auditor-b)

### B-01 — O(n²) broad-phase vs cinolib::Octree + TBB

**Severity**: PERFORMANCE-DRIFT.

**Rust**: `intersection_class.rs:103-105` (Rust author's own comment: "For production, replace with BVH/octree")
**C++ 2022**: `booleans.cpp:288-324` (`customDetectIntersections` with octree + TBB parallel_for)

Identical output set; Rust scales O(n²), C++ scales near O(n log n). Cherchi 2022 §4 explicitly cites octree + parallelism.

---

### B-02 — Yang §4.2.2 Gauss-map filter

**Severity**: DELIBERATE-DIVERGENCE.

**Rust**: `intersection_class.rs:111-144`
**C++**: not present (Yang-specific addition)

Cited inline with Yang §4.2.2 Theorem 4.1. The `ts.tri_label(t0) == ts.tri_label(t1)` same-mesh skip is the precedent that helped guide the share-vertex-skip fix.

---

### B-03 — Missing SIMPLICIAL_COMPLEX semantics in `triangles_intersect_exact`

**Severity**: UNKNOWN-NEEDS-INVESTIGATION (potential CORRECTNESS-BUG, over-detection direction). Cluster II.

**Rust**: `intersection_class.rs:1434-1449`
**C++ cinolib**: `predicates.cpp:1128-1252` (returns `DO_NOT_INTERSECT`/`SIMPLICIAL_COMPLEX`/`INTERSECT`); caller uses `ignore_if_valid_complex=true` to filter out simplicial-complex case.

cinolib distinguishes vertex-fan and shared-edge configurations as "valid simplicial complex" (no real intersection). Our 2-state Rust port conflates with `INTERSECT`, leading to over-detection. **Possible explanation for R0080 regression** post share-vertex fix.

**Suggested fix**: Port `t0_shared`/`t1_shared` bitset trichotomy from `cinolib::triangle_triangle_intersect_3d`.

---

### B-04 — `check_single_no_coplanar_edge_intersection` missing SIMPLICIAL_COMPLEX guard

**Severity**: UNKNOWN-NEEDS-INVESTIGATION. Cluster II.

**Rust**: `intersection_class.rs:1109`
**C++**: `intersection_classification.cpp:686`

C++ early-returns on both `DO_NOT_INTERSECT` AND `SIMPLICIAL_COMPLEX`. Rust returns only `DoNotIntersect` (collapse of B-05's enum). Construction of definite failing case beyond audit budget; downgraded from initial CORRECTNESS-BUG to UNKNOWN.

---

### B-05 — `SegmentIntersection` enum collapses 4 cinolib states into 2

**Severity**: UNKNOWN-NEEDS-INVESTIGATION. Cluster II root.

**Rust**: `intersection_class.rs:586-593`
**C++ cinolib**: `predicates.h::SimplexIntersection` (`DO_NOT_INTERSECT`, `SIMPLICIAL_COMPLEX`, `INTERSECT`, `OVERLAP`)

Cluster-II root cause. Resolving this resolves B-03, B-04, B-08, B-12 (mostly), B-14.

**Suggested fix**: Promote `SegmentIntersection` to `SimplexIntersection` with all four cinolib variants; port parallel-overlap detection from cinolib's `segment_segment_intersect_2d`/`3d`.

---

### B-06 — `finalize_intersection` soft-asserts mask materialize-fallback bug

**Severity**: CORRECTNESS-BUG. Cluster I.

**Rust**: `intersection_class.rs:381-401`
**C++**: `intersection_classification.cpp:267-279`

C++ asserts `v_tmp.size() <= 2` (non-coplanar) and `<= 3` (coplanar) — invariants the algorithm relies on. Rust silently skips when `len != 2`, with a comment claiming "Soft-check instead of hard assert to avoid debug-mode panics". The masked bug is in the materialize-fallback orient2d producing extra vertices — a predicate-kernel issue.

**Suggested fix**: Replace soft-skip with hard assert. Address upstream materialize-fallback bug separately (in `indirect_predicates.rs`).

---

### B-07 — `point_in_triangle_3d_classify` dominant-axis-only

**Severity**: UNKNOWN-NEEDS-INVESTIGATION.

**Rust**: `intersection_class.rs:599-676`
**C++ cinolib**: `predicates.cpp:447-481`

cinolib tests all three 2D projections (must be inside in ALL three to return STRICTLY_INSIDE). Rust tests only the dominant-axis projection. For non-degenerate triangles likely sufficient; for nearly-axis-aligned near-edge points the dominant-axis pick may differ.

---

### B-08 — `triangle_has_actual_intersection_data` workaround for B-03 over-detection

**Severity**: DELIBERATE-DIVERGENCE (uncited).

**Rust**: `aux_structure.rs:273-277`
**C++**: no equivalent

Workaround for B-03's over-detection. Becomes redundant when B-03 is fixed; mark for deletion at that time.

---

### B-09 — Drops 3 dead C++ fields

**Severity**: PERFORMANCE-DRIFT.

**Rust**: `aux_structure.rs:45-88`
**C++**: `aux_structure.h:181-198`

`num_intersections`, `num_tpi`, `visited_pockets` declared in C++ but never read (verified via grep). Rust correctly omits.

---

### B-10 — `v_map` populate timing flag (cross-slice)

**Severity**: PERFORMANCE-DRIFT.

Out-of-slice flag for auditor-d's mod.rs ordering check. Behavior identical as long as v_map populated before classification.

---

### B-11 — `propagate_coplanar_triangles_intersections` idiomatic loop+clone

**Severity**: PERFORMANCE-DRIFT.

**Rust**: `intersection_class.rs:1204-1238`
**C++**: `intersection_classification.cpp:788-830`

Rust uses a `for edge_off in 0..3` loop instead of three explicit copies of the edge-point block. Identical observable output; extra clones due to borrow-checker satisfaction.

---

### B-12 — `init_from_triangle_soup` populates v_map with jolly entries

**Severity**: UNKNOWN-NEEDS-INVESTIGATION. Cluster III.

**Rust**: `aux_structure.rs:113-130`
**C++**: `aux_structure.cpp:45-64` (via `solve_intersections.cpp:65` ordering — jolly NOT yet in vertices)

Geometrically-improbable but constructible CORRECTNESS-BUG: a real intersection point that geometrically equals a jolly-point coordinate would dedup to the wrong vertex.

**Suggested fix**: Resolves with A-03 fix (revert to C++ ordering).

---

### B-13 — Dead `compute_lpi_coords` with bad parallel-fallback

**Severity**: PERFORMANCE-DRIFT (dead code).

**Rust**: `intersection_class.rs:1581-1612`
**C++**: no equivalent (LPI represented implicitly via `implicitPoint3D_LPI` arena type, never materialized)

Dead code (verified via grep — never called). The "return midpoint as fallback when line is parallel to plane" branch is a classic symptom-paper-over: if revived under "fix the parallel case" pressure, it would corrupt downstream geometry silently.

**Suggested fix**: Delete entirely. If float-coord LPI is ever needed, use `ImplicitPoint::LPI{...}.materialize()` which returns `None` for the parallel case.

---

### B-14 — `segment_segment_intersect_3d` 1e-30 magnitude threshold

**Severity**: UNKNOWN-NEEDS-INVESTIGATION. Cluster II.

**Rust**: `intersection_class.rs:728-730`
**C++ cinolib**: `predicates.cpp:684` — no float-tolerance test; uses exact orient2d on three projections.

Soft predicate masquerading as parallelism check. For typical CAD geometry safe; for stretched/scaled geometries could fire incorrectly.

**Suggested fix**: Replace with exact orient2d on three projections. Resolves with B-05 (SimplexIntersection enum promotion).

## Findings C — Triangulation & Local Mesh (auditor-c)

### C-01 — `split_single_triangle_with_stack` skip-already-vertex defense

**Severity**: CORRECTNESS-BUG. Cluster I.

**Rust**: `triangulation.rs:506-529`
**C++**: `triangulation.cpp:228-363`

Cherchi 2020 §5.3 stack invariant: `curr_tri[3]` is a non-vertex point. Rust adds a pre-scan that skips already-vertex points and swaps the first valid candidate to position 3. Silent skip turns an invariant violation into hidden behavior.

---

### C-02 — `reposition_points_in_stack` is_vertex filter drops points

**Severity**: CORRECTNESS-BUG. Cluster I.

**Rust**: `triangulation.rs:612-659`
**C++**: `triangulation.cpp:378-403`

C++ uses non-strict `genericPoint::pointInTriangle`. Rust adds an `is_vertex` filter that prevents the vertex-coincident point from being pushed to a sub-triangle. Pairs with C-01 — together they shape a different algorithm than C++'s.

---

### C-03 — Manual descending-sort before `remove_tris`

**Severity**: PERFORMANCE-DRIFT.

**Rust**: `triangulation.rs:778-783` (manual sort)
**C++**: `triangulation.cpp:641` (calls `removeTris`, which sorts internally)

Identical effective semantics; duplicates internal logic.

---

### C-05 — `find_intersecting_elements` final-triangle append silent skip

**Severity**: CORRECTNESS-BUG. Cluster I.

**Rust**: `triangulation.rs:929-936`
**C++**: `triangulation.cpp:796-805` (asserts `t_id != -1` AND triangle contains v_start or v_stop)

If `tri_opp_to_edge` returns None at termination (cavity not closed), Rust silently skips and `boundary_walker` walks into stale state, producing wrong triangulations.

---

### C-06 — `sort_edge_points` re-added; C++ has it commented out

**Severity**: DELIBERATE-DIVERGENCE (no inline citation).

**Rust**: `triangulation.rs:53-97` (added; called at lines 304-306)
**C++**: `triangulation.cpp:65-72` (commented out; relies on AuxiliaryStructure to deliver pre-sorted)

Likely defensive against a Rust AuxiliaryStructure that doesn't preserve order. Either annotate with citation, or move sort to AuxiliaryStructure population to match C++ invariant location.

---

### C-07 — `CustomStack::push` degenerate filter

**Severity**: DELIBERATE-DIVERGENCE. Cluster I (boundary).

**Rust**: `triangulation.rs:382-394`
**C++**: `custom_stack.h:27-35` (no filter)

Comment cites "approximate coordinates causing imprecise point distribution". With exact predicates the C++ algorithm cannot push a degenerate triple.

---

### C-08 — `split_edge` defensive guards remove triangle without replacement

**Severity**: CORRECTNESS-BUG. Cluster I.

**Rust**: `fast_trimesh.rs:715-731`
**C++**: `fast_trimesh.cpp:708-726` (no guards)

Two defensive guards (`v_id == ev0_id || v_id == ev1_id`; `v_opp == v_id`) papering over inexact predicate output. The second guard removes a triangle without replacement → opens a hole in the cavity.

---

### C-09 — `add_tri` returns 0 on degenerate (HIGHEST PRIORITY)

**Severity**: CORRECTNESS-BUG. Cluster I.

**Rust**: `fast_trimesh.rs:608-622`
**C++**: `fast_trimesh.cpp:624-646` (asserts on degenerate)

Returning 0 silently corrupts mesh state in tree-tracking variants
(`split_edge_with_tree`, `split_tri_with_tree`) via `set_tri_node_id(0, …)`.
The original triangle 0's `info` is overwritten twice, then the original is
removed in `remove_tris(&adj_tris)`. Mesh corrupted.

**Promoted to "fix immediately" priority** by auditor-c, even before bug-hunting the upstream predicate path.

**Suggested fix**: Return `Option<usize>` from `add_tri` so callers short-circuit on degenerate. Or convert the silent-return to debug_assert.

---

### C-10 — `create_tpi` skips dedup against existing TPI vertices

**Severity**: CORRECTNESS-BUG (was UNKNOWN, promoted after investigation).

**Rust**: `triangulation.rs:1281-1318`
**C++**: `triangulation.cpp:1027-1041` (calls `g.addVertexInSortedList` for dedup)

The Rust port HAS the dedup primitive at `aux_structure.rs:323::add_vertex_in_sorted_list` (faithful port of C++'s `addVertexInSortedList`). The TPI creation site just doesn't call it. Pure missing-call bug — not in the predicate cluster.

**Severity test (concrete)**: Two adjacent triangles whose constraint segments cross at a TPI shared between them. C++: 1 vertex in vertices array, 1 entry in v_map; both triangles reference the same vertex ID. Rust: 2 coincident vertices (different IDs but identical implicit point), `rev_vtx_map` second insert overwrites the first → first vertex orphaned, edge-pairing breaks downstream.

**Suggested fix** (auditor-c provided implementation):
```rust
fn create_tpi(...) -> usize {
    // ... build tpi ...
    let candidate_pos = ts.num_verts();
    let (existing_or_new_pos, was_inserted) =
        aux.add_vertex_in_sorted_list(tpi.clone(), candidate_pos);
    if was_inserted {
        let v_id = ts.add_impl_point(tpi);
        debug_assert_eq!(v_id, existing_or_new_pos);
        v_id
    } else {
        existing_or_new_pos
    }
}
```

---

### C-11 — `tri_opp_to_edge` accepts non-manifold edges

**Severity**: DELIBERATE-DIVERGENCE / effective CORRECTNESS-BUG when triggered. Cluster I.

**Rust**: `fast_trimesh.rs:464-482`
**C++**: `fast_trimesh.cpp:470-485` (asserts `e2t[e_id].size() <= 2`)

Returns first non-self triangle. For non-manifold edges in the topological walk, this is undefined behavior — the walk goes off in an unpredictable direction.

---

### C-13 — `edge_id` softens `assert(ev0_id != ev1_id)` to silent None

**Severity**: DELIBERATE-DIVERGENCE (low impact). Cluster I.

**Rust**: `fast_trimesh.rs:291-301`
**C++**: `fast_trimesh.cpp:296-308`

Lower impact than C-08/C-09 because callers test the Option. But callers receive None when the algorithm should never construct an edge query with equal endpoints.

## Findings D — Entry Point & Cherchi-2022 Boolean Layer (auditor-d)

### D-02 — `solve_intersections` ports only Cherchi 2020 Phase 1

**Severity**: DELIBERATE-DIVERGENCE.

**Rust**: `cherchi/mod.rs:82-231`
**C++**: `booleans.cpp:42-77` (`customBooleanPipeline`) decomposed across `exact_mesh.rs::label_cells` + `topology_extract.rs::flood_fill_patches` + `select_boolean_result`.

Yang-authorized decomposition (§4.4.2). Documented in `specs/cherchi_2022_boolean_pipeline.md`.

**Suggested**: Add an inline comment in mod.rs explicitly citing booleans.cpp:42-77 to prevent future auditor confusion.

---

### D-04 — `cosurface_orientation` field (PR10)

**Severity**: DELIBERATE-DIVERGENCE.

**Rust**: `cherchi/mod.rs:55-72`
**C++**: no equivalent — uses `dupl_triangles` round-trip in `booleans.cpp:204-218`.

Cited inline at `mod.rs:43-46`. Cherchi 2020 §5.4 / Hoffmann 1989 §5.3.

**Suggested**: Regression test for identical-footprint coplanar union (Parallel duplicates) and difference (AntiParallel annihilation).

---

### D-05 — Parity counting instead of signed-volume orientation (HIGHEST-PRIORITY FINDING)

**Severity**: CORRECTNESS-BUG.

**Rust**: `exact_mesh.rs:1320-1440` (`ray_cast_inside`, `hit_count % 2 == 1`)
**C++**: `booleans.cpp:1290-1300` (`checkTriangleOrientation`, `orient3d(tv0, tv1, tv2, ray.v1) < 0`)

Cherchi 2022 §5.3 + Figure 5 explicitly state inside/outside is determined by the **orientation of the FIRST intersected triangle relative to the ray direction** — NOT parity counting. The paper text at line 89-90 explicitly contrasts this against parity counting ("up to 100× faster than existing approaches based on topological flooding"). **We are using the thing the paper contrasts against.**

Yang §4.4.2 cites Cherchi 2022 specifically for this classification step. This is the single most important finding in the audit. Per `feedback_yang_only.md`: "if Yang cites Cherchi 2022, we implement Cherchi 2022 — not patches." Parity counting is a patch.

**Severity test**: For a target mesh with two concentric closed shells (model with internal void), centroid in the gap: parity = 1 → Inside; signed-volume on first hit gives the correct interpretation depending on shell orientation. For Möbius-like non-orientable patches (legitimate post-arrangement output if input had self-intersection), parity and signed-volume disagree.

**Suggested fix**: Replace parity counting with: BVH-find the FIRST intersected triangle along the ray (smallest `t_hit > 0`), return `Inside iff orient3d(tv0, tv1, tv2, ray.v1) < 0`. Requires `ray_tri_intersect_axis` to return both hit-distance and triangle index, plus a `min_by_key(|h| h.t)` reduction.

**Test conflict**: `label_cells_raycast_matches_gwn_for_offset_boxes` (exact_mesh.rs:6352) PINS the parity-counting behavior as an invariant. The fix-PR must update or delete that test.

---

### D-06 — Centroid-only emanating point vs Cherchi cascaded `findRayEndpoints`

**Severity**: UNKNOWN-NEEDS-INVESTIGATION.

**Rust**: `exact_mesh.rs:1487` (always uses `sub_tri_centroid`)
**C++**: `booleans.cpp:475-546` (`findRayEndpoints`: prefer interior explicit vertex, fallback to centroid with snap-rounding round-trip)

Centroid-of-sub-triangle is *coplanar* with the sub-triangle's plane → +X ray from centroid risks grazing the sub-triangle's own plane when axis-aligned, yielding `Degenerate` results that fall into the Hoffmann fallback path more often than necessary. Cherchi 2022 §5.1 paragraph 4 explicitly: "we resort to guaranteed exact rational numbers only as backup strategy".

**Suggested fix**: Implement Cherchi-faithful cascade: try sub-tri vertex 0/1/2 first, fall back to centroid only when all three vertices project ambiguously.

---

### D-07 — Hoffmann perturbation vs Cherchi `nextafter` cascade

**Severity**: CORRECTNESS-BUG.

**Rust**: `exact_mesh.rs:1383-1387` (single Degenerate enum, advance to next axis, fall back to Hoffmann sample-both-sides along sub-tri normal)
**C++**: `booleans.cpp:780-915` (`perturbXRay`/`Y`/`Z` — cycle through 8 `std::nextafter` perturbation offsets); `booleans.cpp:626-714` (`pruneIntersectionsAndSortAlongRay` dispatches per-vertex / per-edge / per-face hits separately)

Cherchi 2022 §5.3 (paper text + Figure 6) handles vertex/edge ambiguity by classifying which kind occurred (`INT_IN_V0/V1/V2/EDGE01/EDGE12/EDGE20`) and applying a targeted perturbation cascade — only the small 1-ring or 2-tri edge ring is re-tested, with `std::nextafter` ray-endpoint perturbation cycling through 8 directional offsets.

The spec's claim "Hoffmann perturbation instead of nextafter perturbation, mathematically equivalent in the limit" underestimates this divergence. They agree asymptotically but **not** for finite ε (`d_epsilon.max(1e-6)`), where the two methods can disagree on boundary-coincident sub-triangles whose normals are not aligned with any global axis.

**Severity test**: Sub-triangle with centroid lying exactly on a vertex of the target mesh (degenerate on all 3 axis projections), sub-tri normal at 45° to all global axes. C++ perturbs ray-endpoint by `nextafter` along axis-aligned offsets and re-tests vertex 1-ring. Rust falls into Hoffmann sample-both-sides at `+/- eps * normal` along the sub-tri's own (45°) normal — samples DIFFERENT geometric points than C++'s perturbed ray would intersect.

**Suggested fix**: Track ambiguity kind (vertex / edge / coplanar) in `RayHit`; implement per-case 8-offset `std::nextafter` cascade.

---

### D-08 — Same-label skip implicit through API separation

**Severity**: DELIBERATE-DIVERGENCE.

For binary booleans (current scope), behavior is equivalent — Rust passes only the OTHER operand's mesh to `ray_cast_inside`. For N-ary booleans (Cherchi C++ supports >2 inputs), the per-pair Rust API breaks down vs C++'s single-pass labeling.

---

### D-09 — Slab-eps expansion vs Cherchi tight ray AABB

**Severity**: PERFORMANCE-DRIFT (correctness risk if mis-tuned).

**Rust**: `exact_mesh.rs:1330` (`slab_eps = TAU_EXACT_MESH_SLAB_EPS`)
**C++**: `booleans.cpp:550-589 + 596-621` (tight zero-extent ray AABB; Cherchi 2022 §5.2)

Rust expands the slab by `±slab_eps` as a bandaid for parity-counting + welded-mesh flow. Vanishes if D-05 + D-10 are fixed.

**Suggested fix**: Pair with D-05 fix; use tight zero-extent slab.

---

### D-10 — `weld_mesh_vertices` nanometer quantization (A15.6 violation)

**Severity**: CORRECTNESS-BUG (potentially).

**Rust**: `exact_mesh.rs:1684-1754` (`weld_mesh_vertices` quantizes at `QUANT_NANOMETER_SCALE`; called twice in `label_cells:1753-1754`)
**C++**: no equivalent — uses Cherchi 2020 indirect predicates throughout for vertex identity.

Tolerance-escalation deprecated by `governance/ARCHITECTURAL_INVARIANTS.md §A15.6`. The Rust comment justifies it as a workaround for non-watertight tessellation upstream — exactly the anti-pattern A15.6 forbids.

**Severity test**: Two cubes at distance `0.5 * 1/QUANT_NANOMETER_SCALE` (sub-nanometer separation): tessellation produces distinct vertices on each cube; `weld_mesh_vertices` collapses them into one → ray-cast classification silently treats the two cubes as joined.

**Suggested fix**: Per A15.6: fix upstream tessellation to produce shared vertex IDs at face boundaries (bijective tessellation per Yang §4.1.1); remove `weld_mesh_vertices` from `label_cells`.

---

### D-11 — `flood_fill_patches` intersection-edge vs manifold-edge barriers

**Severity**: UNKNOWN-NEEDS-INVESTIGATION.

**Rust**: `topology_extract.rs:472-521` (intersection-edge barriers, Yang §4.4.2 style)
**C++**: `booleans.cpp:412-431, 450-470` (manifold-edge barriers, Cherchi 2022 §5)

The two patch definitions interact with the ray-cast labeling in a non-obvious way. Yang's intersection-edge definition is self-consistent if labeling happens BEFORE flood-fill (per-sub-tri); Cherchi's manifold-edge definition is self-consistent if labeling happens per-patch AFTER flood-fill. Rust mixes the two: per-sub-tri labeling (Yang-style) followed by Yang-style flood-fill barriers — but this can produce a Rust patch containing both Inside-region and Outside-region sub-triangles under Cherchi's classification, receiving a single label that's wrong for half its triangles.

**Severity test**: Self-intersecting input mesh (Klein-bottle topology, or flat sheet folded over itself). Standard CAD inputs after Yang preprocessing are watertight per A15.6, but the self-intersection oracle reports 19/25 F-series have hidden self-intersections — these are the inputs where the divergence matters.

**Suggested fix**: Compute manifoldness per directed edge and stop flood-fill at non-manifold edges (Cherchi-faithful), then label PER-PATCH (one ray per patch). OR keep per-sub-tri labeling but assert label consistency within each Rust patch.

---

### D-12 — Yang "inner triangle" seed preference not implemented

**Severity**: UNKNOWN-NEEDS-INVESTIGATION.

Related to D-06. Yang §4.4.2 specifies seeding from "inner triangle, i.e. not on the boundaries of each mesh patch". Rust has no `vertInfo` / patch-border tracking, so cannot make Cherchi-faithful interior-vertex emanating-point choices.

---

### D-13 — Per-triangle `LocalMesh` vs C++ global `FastTrimesh`

**Severity**: DELIBERATE-DIVERGENCE (cascading).

**Rust**: `mesh_arrangement.rs:25-40` (per-triangle LocalMesh)
**C++**: `fast_trimesh.cpp` (global mesh aggregating all post-arrangement triangles)

Documented at `mesh_arrangement.rs:11-16` and in memory `[Yang Global Mesh Arrangement]` ("Per-triangle approach is a deviation. Must use global Cherchi mesh arrangement for watertight guarantee."). Cascades into D-11/D-12 because per-sub-tri output erases global manifold information.

**Note (resolved by team-lead grep)**: `mesh_arrangement::triangulate_single_triangle` is on the Yang-stage-3 SSI refinement path (out of audit scope), NOT the live Cherchi pipeline. The live path uses `cherchi/triangulation.rs::triangulation_with_parents` (auditor-c's slice) which IS the global structure. So D-13's "cascades into D-11/D-12" applies because `flood_fill_patches` consumes the per-sub-tri output of the live Cherchi pipeline, but the Cherchi pipeline itself is faithful.

**Suggested fix**: Long-term — make `flood_fill_patches` recover global manifold information (count `directed_edge_to_tris` per undirected edge, use as flood-fill barrier). This is the D-11 fix.

## Prioritized To-Fix Queue

Priority order per the plan: CORRECTNESS-BUG → UNKNOWN → DELIBERATE-DIVERGENCE
without citation → PERFORMANCE-DRIFT.

Each row → one future FIP-compliant PR (Spec → Test → Implementation →
Validation), one fix per PR per the user's directive.

### Tier 1: CORRECTNESS-BUG, ordered by Yang-assay-impact estimate

| Rank | ID | One-line fix | Estimated Yang-assay impact |
|------|-----|--------------|----------------------------|
| 1 | **D-05** | Replace parity counting with first-hit signed-volume orientation in `ray_cast_inside` | HIGH — Yang §4.4.2 cites this specifically. Likely affects most cases with internal voids or non-orientable post-arrangement patches. Test conflict: `label_cells_raycast_matches_gwn_for_offset_boxes` must be updated. |
| 2 | **C-09** | Return `Option<usize>` from `add_tri` (or panic on degenerate); fix predicate path | HIGH — silent mesh corruption in any input that produces a degenerate triangle in earcut/edge-split. |
| 3 | **C-10** | Wire `aux.add_vertex_in_sorted_list` into `create_tpi` (auditor-c provided implementation) | MEDIUM — affects any input with constraint segments crossing at TPI shared across multiple triangles. Pure missing-call bug. |
| 4 | **A-01 + A-02** (paired) | Replace inexact f64 collinearity / normal-axis tests with cascaded filtered/exact predicates from `geometry-predicates` | HIGH — these are root causes for Cluster I; fixing them likely allows removing several defensive guards. |
| 5 | **D-07** | Track ambiguity kind in `RayHit`; implement per-case 8-offset `nextafter` cascade | MEDIUM — affects sub-triangles with normals not aligned with global axes, particularly post-coplanar-preprocessing cases. |
| 6 | **D-10** | Per A15.6: fix upstream tessellation (bijective shared boundaries); remove `weld_mesh_vertices` | MEDIUM — A15.6 invariant violation. May surface previously-masked tessellation bugs. |
| 7 | **B-06** | Replace soft-skip with hard assert in `finalize_intersection`; address upstream materialize-fallback bug | LOW-MEDIUM — Cluster-I masking. |
| 8 | **C-01 + C-02 + C-05** (paired) | Convert defensive guards to debug_assert; bug-hunt the upstream predicate paths producing impossible state | MEDIUM — Cluster-I masking. |
| 9 | **C-08** | Same as C-01/02/05; remove `split_edge` defensive guards, fix upstream | MEDIUM — Cluster-I masking. |

### Tier 2: UNKNOWN-NEEDS-INVESTIGATION, ordered by code centrality

| Rank | ID | Investigation step |
|------|-----|---------------------|
| 1 | **B-03 / B-04 / B-05 / B-12 / B-14** (cluster) | Port cinolib's 4-state `SimplexIntersection` enum; verify each finding's failing-input hypothesis. **Single fix resolves 5 findings.** |
| 2 | **A-03 / A-06 / B-12** (cluster) | Move jolly-points append to end of pipeline (after triangulation), matching C++ `appendJollyPoints()`. **Single fix resolves 3 findings.** |
| 3 | **D-11 / D-12** (cluster) | Track patch-border vertices in flood-fill; use manifold-edge barriers; mark border vertices for `findRayEndpoints` cascade. **Single fix resolves 2 findings + supports D-06.** |
| 4 | **D-06** | Implement Cherchi-faithful `findRayEndpoints` cascade; pairs with D-12. |
| 5 | **A-05** | Write unit test asserting `compute_multiplier` exact behavior; capture C++ reference; resolve direction. |
| 6 | **B-07** | Verify `point_in_triangle_3d_classify` dominant-axis-vs-all-three-projections equivalence on near-degenerate triangles. |

### Tier 3: DELIBERATE-DIVERGENCE without inline citation

| ID | Add inline citation comment |
|------|---------------------------|
| C-06 | sort_edge_points — citation explaining why C++ has it commented out and Rust re-adds it |
| C-13 | edge_id Option-return — citation explaining when ev0_id == ev1_id can arise |
| B-08 | triangle_has_actual_intersection_data — link to B-03 root cause; mark for deletion when B-03 is fixed |

### Tier 4: PERFORMANCE-DRIFT

| ID | Optimization |
|------|--------------|
| B-01 | Build BVH/octree for broad-phase (Rust author's own TODO) |
| C-03 | Replace manual sort with `subm.remove_tris` |
| B-13 | Delete `compute_lpi_coords` dead code entirely |
| B-09 | Already-correct (Rust omits dead C++ fields); document in code comment |
| B-10 | Already-correct (out-of-slice flag for ordering verification) |
| B-11 | Loop+clone is acceptable; could be optimized with index-based iteration |
| D-09 | Resolves with D-05 fix (use tight zero-extent slab) |

## Out-of-Scope Notes / Future Audits

The following are **not findings** in this audit but are flagged for future work:

1. **`intersection_opt.rs::NormalCone::may_intersect()`** — half-angle=0 strict-inequality bug. Current Rust workaround in `intersection_class.rs:111-114` bypasses NormalCone for flat triangles via direct dot-product check. Severity estimate: PERFORMANCE-DRIFT or DELIBERATE-DIVERGENCE depending on equivalence proof.

2. **Yang stage 3 SSI refinement** (`ssi_refinement.rs`, uses `mesh_arrangement.rs::triangulate_single_triangle`) — separate audit candidate; not part of Cherchi pipeline proper.

3. **`coplanar_preprocess.rs`** — Yang §4.5.5 coplanar preprocessing layer; uses Cherchi predicates but is Yang-specific. Separate future audit.

4. **`indirect_predicates.rs`** — predicate-kernel layer; Cluster I findings strongly suggest this needs an audit pass for exact-arithmetic completeness.

5. **`compute_lpi_coords` materialize-fallback bug** — referenced in B-06; the predicate-kernel root cause for several CORRECTNESS-BUGs.

6. **Test calibrated to broken behavior**: `label_cells_raycast_matches_gwn_for_offset_boxes` (`exact_mesh.rs:6352`) PINS the parity-counting behavior. Must be updated when D-05 is fixed.

## Methodology

### Team structure

- **Lead**: scope review, calibration ground-truth, spot-check, integration.
- **Auditor A**: preprocessing/soup/common/tree (4 files, ~1,300 lines).
- **Auditor B**: intersection_class + aux_structure (2 files, ~2,400 lines).
- **Auditor C**: triangulation + fast_trimesh (2 files, ~3,300 lines).
- **Auditor D**: mod.rs + Cherchi-2022-relevant call graph in exact_mesh.rs/topology_extract.rs/mesh_arrangement.rs.

### Calibration

All 4 auditors independently audited `tree.rs` (114 lines) before their main slice. All 4 reached the same conclusion: zero substantive deviations (the only candidate, `addChildren` overload split into `add_children_2`/`_3`, is a Rust idiom translation per the IGNORE rubric — Rust lacks function overloading). This calibrated the rubric's IGNORE list and aligned all 4 auditors on rejecting language-level idiom translations as non-deviations.

### Verification (Lead)

- Spot-checked ~25% of findings by re-reading both Rust and C++ at cited line numbers.
- Verified 100% of paper citations on CORRECTNESS-BUG and DELIBERATE-DIVERGENCE findings.
- D-14 was downgraded to NOT A DEVIATION after team-lead grep verified
  `mesh_arrangement::triangulate_single_triangle` is only called from
  `ssi_refinement.rs` (out-of-audit-scope), not from the live Cherchi pipeline.

### What was NOT verified

- C++ paper source text for Cherchi 2020: only the 2022 paper extract was available locally. Citations to "Cherchi 2020 §X" come from Rust inline comments and the C++ implementation, not paper-source-verified.
- Performance equivalence claims for PERFORMANCE-DRIFT findings: the Rust author's TODO comments and the C++ algorithm's documented complexity were used as substitutes for actual benchmarks.
- C++ UB realization for A-05 (`compute_multiplier`): the hypothesis that GCC's signed-shift wraps to a negative value triggering the "temporary fix" fallback was not empirically tested.

---

*This audit is a snapshot. Future audits should reference this report as the
baseline and surface new deviations introduced after 2026-04-28 SHA `5ed9ee1`
(main).*
