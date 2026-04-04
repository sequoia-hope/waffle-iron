# Algorithm Cross-Reference Index

Maps specific algorithms to their academic source AND the file/function in
our codebase that implements them. Reference numbers `[#N]` from REFERENCES.md.

> **Note (2026-04-04):** Sections below referencing `vendor/truck/` and
> `crates/kernel-fork/` paths point to the **archived truck-based kernel**
> (`archive/truck/`). The active clean-sheet kernel is at `crates/kernel/`.
> See the "Clean-Sheet Kernel" section for current implementations.

## Predicates & Robustness

| Algorithm | Reference | File | Function/Type |
|-----------|-----------|------|---------------|
| Adaptive orient3d | [#4] Shewchuk 1997 | `vendor/truck/truck-shapeops/src/transversal/robust_classify.rs` | `robust_orient3d()` |
| Adaptive orient2d | [#4] Shewchuk 1997 | `robust_classify.rs` | `robust_orient2d()` |
| SoS orient2d tiebreak | [#5] Edelsbrunner-Mucke 1990 | `robust_classify.rs` | `sos_orient2d_tiebreak()` |
| SoS orient3d tiebreak (D=3 cofactor chain) | [#5] Edelsbrunner-Mucke 1990 | `robust_classify.rs` | `sos_orient3d_tiebreak()` |
| Lazy exact escalation (triple product) | [#4] Shewchuk + [#19] Devillers-Preparata | `robust_classify.rs` | `lazy_exact_triple_sign()` |
| Ray-triangle crossing with SoS | [#4]+[#5] | `robust_classify.rs` | `robust_ray_triangle_cross()` |

## Classification

| Algorithm | Reference | File | Function/Type |
|-----------|-----------|------|---------------|
| Generalized winding number | [#7] Jacobson 2013 | `vendor/truck/truck-shapeops/src/transversal/winding.rs` | `winding_number()` |
| Solid angle (Van Oosterom-Strackee) | [#7] | `winding.rs` | `solid_angle()` |
| Ray-cast 8-ray majority vote | [#2] Hoffmann Ch.3, [#17] Requicha | `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` | `ray_cast_classify()` |
| Coplanar face containment | [#2] Hoffmann neighborhood analysis | `vendor/truck/truck-shapeops/src/transversal/coplanar.rs` | `classify_coplanar_fragment()` |
| Coplanar 2D polygon overlay | [#26] Yang 2025, [#8] Zhou 2016 | `vendor/truck/truck-shapeops/src/transversal/coplanar_overlay.rs` | `compute_coplanar_overlay()` |

## Boolean Pipeline

| Algorithm | Reference | File | Function/Type |
|-----------|-----------|------|---------------|
| Pave blocks (edge segments between IC crossings) | [#3] OpenCASCADE GFA | `vendor/truck/truck-shapeops/src/transversal/pave_block.rs` | `PaveBlock<C>`, `IcVertex`, `IcSegment` |
| Shrunk parametric ranges | [#3] OpenCASCADE | `pave_block.rs` | `PaveBlock.shrunk_range` |
| IC injection into face loops | [#2] Hoffmann Ch.3, [#33] Stroud §6.1 | `vendor/truck/truck-shapeops/src/transversal/loops_store/mod.rs` | `inject_ic_edges_direct()` |
| Face boundary graph division | [#2] Hoffmann, [#3] OCCT GFA | `vendor/truck/truck-shapeops/src/transversal/divide_face/mod.rs` | `divide_one_face_v2()` |
| Multi-IC chain detection | [#33] Stroud §6.1 wire sewing | `loops_store/mod.rs` | B22: multi-IC closed chain assembly |
| Boundary-coincident IC skip | [#3] OCCT same-domain | `loops_store/mod.rs` | D1.6: `has_boundary_edge_between()` |
| Per-stage tolerance config | [#33] Stroud Ch.16 | `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` | `BooleanTolerance` |
| Euler characteristic validation | [#16] Mantyla, [#23] Edelsbrunner | `integrate/mod.rs` | `validate_euler_characteristic()` |

## Surface-Surface Intersection

| Algorithm | Reference | File | Function/Type |
|-----------|-----------|------|---------------|
| IC construction (mesh-based + Newton refinement) | [#1] Patrikalakis Ch.5-6 | `vendor/truck/truck-shapeops/src/transversal/intersection_curve/mod.rs` | `IntersectionCurveWithParameters::try_new()` |
| Analytical plane-cylinder SSI | [#1] Patrikalakis Ch.5 | `intersection_curve/analytical.rs` | `refine_polyline()` |
| Analytical plane-cone SSI | [#1] Patrikalakis Ch.5 | `intersection_curve/analytical.rs` | conic section projection |
| Analytical plane-sphere SSI | [#1] Patrikalakis Ch.5 | `intersection_curve/analytical.rs` | circle projection |

## Geometry

| Algorithm | Reference | File | Function/Type |
|-----------|-----------|------|---------------|
| NURBS arc healing (circle arc for plane-curved IC) | [#1] Patrikalakis Ch.6, [#32] Piegl Ch.7 | `crates/kernel-fork/src/healing.rs` | `try_heal_nurbs_arc_for_ic()` |
| NURBS arc construction (4-point quadratic) | [#32] Piegl Ch.7 | `healing.rs` | `build_nurbs_arc_in_frame()` |
| Scale normalization (mm-scale workaround) | [#24] Yang, [#27] Li, [#31] Li | `crates/kernel-fork/src/truck_kernel.rs` | `compute_scale_normalization()` |
| Bounding box extent computation | [#31] Li (unit-cube normalization) | `truck_kernel.rs` | `solid_max_extent()` |

## Tessellation

| Algorithm | Reference | File | Function/Type |
|-----------|-----------|------|---------------|
| CDT-based tessellation | [#22] Sullivan (curvature theory) | `vendor/truck/truck-meshalgo/src/tessellation/mod.rs` | `MeshableShape::triangulation()` |
| Shell tessellation (parallel sampling + CDT) | [#32] Piegl Ch.6 (point inversion) | `tessellation/triangulation.rs` | `shell_tessellation()` |

## Clean-Sheet Kernel (crates/kernel/)

| Algorithm | Reference | File | Status |
|-----------|-----------|------|--------|
| Euler operators (mvfs, mev, mef, kemr, kfmrh) | [#16] Mantyla, [#33] Stroud Ch.4 | `src/topology/euler_ops.rs` | Done |
| Bijective mesh���B-Rep mapping | [#24] Yang 2025 | `src/tessellation/bijective.rs` | Done (Phase 1) |
| Exact mesh boolean (indirect predicates) | [#9] Cherchi 2020 | `src/boolean/exact_mesh.rs` | Done (Phase 2) |
| Topology extraction (face survival) | [#24] Yang 2025 | `src/boolean/topology_extract.rs` | In progress (Phase 3) |
| SSI geometry refinement | [#1] Patrikalakis, [#24] Yang | `src/boolean/ssi_refinement.rs` | In progress (Phase 4) |
| Quadric SSI solvers (12/15 pairs) | [#1] Patrikalakis Ch.5 | `src/ssi/mod.rs` | 12 done, 3 partial |
| BVH-accelerated ray-cast classification | [#7] Jacobson, [#4] Shewchuk | `src/boolean/exact_mesh.rs` | Done |
| Co-surface elimination + coplanar merge | [#24] Yang, [#8] Zhou | `src/boolean/topology_extract.rs` | Done |

## Not Yet Implemented (Target Architecture)

| Algorithm | Reference | Status |
|-----------|-----------|--------|
| Topology-guaranteed SSI (Dixon resultant) | [#25] Yang 2023 | Target |
| GWN on trimmed NURBS (no tessellation) | [#30] Spainhour 2026 | Target |
| IATA for tangent/degenerate SSI | [#29] Cheng 2023 | Target |
| Overlap extraction (bilevel optimization) | [#26] Yang 2025 | Target |
| Algebraic self-intersection detection | [#31] Li 2025 | Target |
| Curvature-adaptive tessellation (R^6 embedding) | [#34] Dassi 2014 | Target |
| Lazy exact evaluation (curved solids) | [#13] ESOLID, Keyser 2004 | Architectural reference |
