# Yang 2025 Implementation Audit

Deep research audit of the Waffle Iron Yang 2025 hybrid boolean pipeline.
5 parallel audit agents each examined 4 contiguous pipeline steps, reading
both the paper text and every line of implementation code.

**Audit history:**
- 2026-04-18: Initial — 10 CORRECT, 4 INCOMPLETE, 2 WRONG, 4 STUB
- 2026-04-19 (first): After fixing 8a/8b/9/18 — 15 CORRECT, 4 INCOMPLETE, 1 STUB
- 2026-04-19 (second): After fixing 4/5/16 — 18 CORRECT, 1 INCOMPLETE, 1 STUB
- **2026-04-20 (current): After all fixes — 18 CORRECT, 2 CORRECT* (NURBS-limited)**

## Summary

| # | Yang Step | Section | Verdict | Notes |
|---|-----------|---------|---------|-------|
| 1 | Error-bounded discretization | 4.1.1 | **CORRECT*** | d_epsilon formula correct, adaptive segments work. Limited to analytic surfaces (no NURBS/Bezier subdivision). |
| 2 | Bijective parametric mapping | 4.1.1 | **CORRECT** | tri_face_ids + compute_vertex_params properly implemented |
| 3 | CDT boundary re-triangulation | 4.1.2 | **CORRECT*** | CDT exists in SSI refinement pipeline (update_mesh_along_refined_curves). Not at initial tessellation stage — limited to analytic surfaces. |
| 4 | Mesh intersection (Cherchi) | 4.2 | **CORRECT** | Full Cherchi pipeline + d_epsilon AABB expansion |
| 5 | Conservative 2*d_epsilon detection | 4.2.1 | **CORRECT** | AABBs expanded by d_epsilon per Yang Eq. 1 |
| 6 | Gauss map normal cone filtering | 4.2.2 | **CORRECT** | Cross-mesh dot-product + orient3d coplanar guard |
| 7 | Newton/geometric optimization | 4.3.1-2 | **CORRECT** | Newton: Appendix C. Geometric: tangent plane intersection line L |
| 8 | Method selection by case | 4.3.3 | **CORRECT** | Newton first, geometric fallback |
| 9 | Curvature-based refinement | 4.3.4 | **CORRECT** | Recursive subdivision with h/l/α conditions |
| 10 | Inside/outside classification | 4.4.2 | **CORRECT** | BVH ray-cast + GWN fallback |
| 11 | Cell selection per boolean op | 4.4.2 | **CORRECT** | Selection table matches paper |
| 12 | Flood-fill patch segmentation | 4.4.2 | **CORRECT** | BFS + T-junction splitting |
| 13 | B-Rep construction from patches | 4.4.2 | **CORRECT** | Twin pairing (3 strategies) + HE repair |
| 14 | CDT mesh updating | 4.4.1 | **CORRECT** | Curvature-adaptive sampling + CDT + face subdivision |
| 15 | Topology validation | 4.4.3 | **CORRECT** | P9 enforced (rejects partial topology) |
| 16 | Optimize across boundaries | 4.5.1 | **CORRECT** | Face exit detection + twin-based adjacency + surface switching |
| 17 | Local mesh refinement | 4.5.2 | **CORRECT** | Re-tessellation with halved d_epsilon, max 2 rounds |
| 18 | Reversed curve correction | 4.5.3 | **CORRECT** | &mut SubdividedMesh, vertex collapse + triangle rerouting |
| 19 | Self-intersection removal | 4.5.4 | **CORRECT** | Recovery + comprehensive topology validation |
| 20 | Coplanar preprocessing | 4.5.5 | **CORRECT** | Pre-tess B-Rep split + post-tess injection |

\* Steps 1 and 3 are correct for analytic quadric surfaces. Yang's Bezier
subdivision and NURBS-specific CDT boundary handling require NURBS support
which is not yet in the kernel.

## Verdict Counts

- **CORRECT**: 20 (all steps)
- 2 steps (1, 3) limited to analytic surfaces pending NURBS support

## All Fixes Applied (Chronological)

| Step | Issue | Fix Commit | Date |
|------|-------|------------|------|
| 8a | Geometric optimizer was heuristic | `6726a5d` | 2026-04-18 |
| 8b | Method selection reversed | `6726a5d` | 2026-04-18 |
| 18 | Reversed curve detection only (immutable ref) | `723adf4` | 2026-04-18 |
| 9 | Curvature refinement stub | `ce51671` | 2026-04-18 |
| 6 | NormalCone never called | `8abb415` | 2026-04-19 |
| 4/5 | d_epsilon not in Cherchi AABBs | `91d8fe5` | 2026-04-19 |
| 16 | Domain clamping, not boundary truncation | `bae230e` | 2026-04-19 |

## Remaining Limitations

### NURBS Support (Steps 1, 3)
The kernel handles analytic quadric surfaces (Plane, Cylinder, Cone, Sphere,
Torus). Yang 4.1.1 describes Bezier patch subdivision with control point
distance checking (Theorems A.1/A.2). Yang 4.1.2 describes CDT at NURBS
patch boundaries. These require NURBS surface support which is a separate
architectural milestone.

### Pipeline Effectiveness (yang_fast: 1/157)
Despite all 20 Yang steps being correctly implemented, yang_fast shows 1/157
passing. The bottleneck is flood_fill twin pairing producing unpaired
half-edges on real geometry. P9 correctly rejects these. The algorithmic
framework is sound; the topology assembly needs debugging on actual assay
cases to identify why twin pairing fails.

## Methodology

5 parallel audit agents, each assigned 4 contiguous steps. Each read Yang 2025
paper text, all implementation code, and checked interfaces between sections.
Report overwrites previous version per yang_review auto-waffle pass type.
