# Phase 1: Pipeline Stage Mapping — Intersection + Face Division + Coplanar Handling

## Overview

This document compares the current intersection, face division, and coplanar handling stages of the Waffle Iron boolean pipeline (in `vendor/truck/truck-shapeops/`) against the literature and the production spec (`specs/SHAPEOPS-BOOLEAN-SPEC.md`). No code changes are made — this is a pure analysis deliverable.

**Files under review:**
- `vendor/truck/truck-shapeops/src/transversal/loops_store/mod.rs` (~1315 lines)
- `vendor/truck/truck-shapeops/src/transversal/divide_face/mod.rs` (~786 lines)
- `vendor/truck/truck-shapeops/src/transversal/coplanar.rs` (~488 lines)
- `vendor/truck/truck-shapeops/src/transversal/coplanar_overlay.rs` (~932 lines)
- `vendor/truck/truck-shapeops/src/transversal/polyline_construction/mod.rs`
- `vendor/truck/truck-shapeops/src/transversal/intersection_curve/`

**References consulted:**
- OCCT General Fuse Algorithm (staged interference VV→VE→EE→VF→EF→FF)
- Patrikalakis et al. (SSI algorithms — marching, lattice, subdivision)
- Levy 2025 (exact predicates & constructions)
- Barki 2015 (co-refinement booleans)
- Sugihara-Iri (topology-oriented implementation)
- Zhou 2016 (mesh arrangements, winding numbers)
- Cherchi 2020 (indirect predicates)

---

## Section 1: Intersection Stage (`create_loops_stores`)

### 1.1 Algorithm Identification

The function `create_loops_stores` (`loops_store/mod.rs:721`) implements a **tessellation-first mesh interference** approach to surface-surface intersection. The algorithm proceeds:

1. **Initialization** (lines 738-751): Collect face boundary wires from both geometric and polyline shells into `LoopsStore` structures. Pre-compute per-face AABBs from polygon mesh positions.

2. **Coplanar pre-scan** (lines 753-778): O(n×m) scan of all face pairs with AABB culling. For each overlapping pair, call `check_coplanar_faces` to identify faces on the same plane within `coplanar_tol`. Record coplanar face indices and pairs.

3. **Coplanar adjacency skip computation** (lines 786-875): For coplanar pairs where one face is strictly contained within the other (2D point-in-polygon), compute a set of face pairs to skip in the main intersection loop.

4. **Cross-shell coincident vertex detection (Phase 1B)** (lines 883-925): O(V0×V1) brute-force scan of all boundary vertex pairs across shells. Finds positions appearing in both shells within tolerance. Deduplicates into a canonical set.

5. **Coincident-edge face pair skip (Phase 1B)** (lines 936-996): For each face pair (with AABB culling), check if they share a coincident edge (both endpoints match within tolerance). Such pairs are skipped because their SSI would produce degenerate ICs.

6. **Main intersection loop** (lines 998-1265): Iterate over all face pairs (i, j) from shell0 × shell1:
   - Skip coplanar, coplanar-adjacent, coincident-edge pairs
   - Skip non-overlapping AABBs
   - Tessellate and call `intersection_curves()` which:
     - Calls `polygon0.extract_interference(polygon1)` for triangle-triangle intersection
     - Assembles segments into polylines via `construct_polylines()` (graph walk with quantized endpoints)
     - Projects each polyline sample back onto parametric surfaces via `search_triple` (Newton iteration)
   - For each resulting IC: determine And/Or status from normal cross products, handle closed/open ICs, snap endpoints to coincident vertices, filter boundary-touching ICs, insert IC endpoints into face boundary wires

7. **Biangle wire cleanup** (lines 1267-1281): Remove degenerate 2-edge wires.

8. **Coplanar boundary injection** (lines 1283-1302): For coplanar pairs with full containment, inject boundary wire as independent loop.

**Algorithm classification:** This is an **ad-hoc, tessellation-first intersection algorithm** that does not correspond to any named algorithm from the literature. It is not OCCT's General Fuse (analytical SSI with staged interference), not Patrikalakis-style marching (analytical curve tracing on parametric surfaces), not Levy/Barki co-refinement (exact triangle-triangle intersections). The approach is: "tessellate both surfaces, find mesh collision segments, stitch into polylines, project back onto parametric surfaces." The Sprint 37 additions (Phase 1B coincident vertex detection, Phase 1C short IC filtering, coplanar adjacency skip sets) are ad-hoc patches for specific degeneracy cases.

### 1.2 OCCT Interference Type Coverage

The OCCT General Fuse Algorithm processes 6 B-Rep interference types in strict sequence:

| Interference Type | OCCT Description | Our Coverage | Implementation |
|---|---|---|---|
| **VV** (Vertex/Vertex) | Merge coincident vertices across shapes | **Partial** | Phase 1B coincident vertex detection (`mod.rs:883-925`) finds shared positions, snaps IC endpoints (`mod.rs:1112-1127`). But does NOT merge actual B-Rep vertex topology. |
| **VE** (Vertex/Edge) | Project vertices onto edges, create paves | **Missing** | No code projects vertices from one shell onto edges of the other. |
| **EE** (Edge/Edge) | Detect edge-edge common parts and intersections | **Partial** | Phase 1B coincident-edge skip (`mod.rs:936-996`) detects coincident edges by endpoint matching. But only uses this to SKIP face pairs — does not create common blocks or intersection vertices at edge crossings. |
| **VF** (Vertex/Face) | Project vertices onto faces | **Minimal** | Phase 1C short IC filtering (`mod.rs:1097`) indirectly handles some VF cases. No explicit vertex-on-face projection. |
| **EF** (Edge/Face) | Detect edge-face intersection points | **Missing** | No code tests edges of one shell against faces of the other. Relies on FF to produce all intersection curves. |
| **FF** (Face/Face) | Compute intersection curves between faces | **Yes** | The main loop implements this via mesh collision. |

**Summary: 1 of 6 interference types fully implemented. 3 partial. 2 missing.**

**How missing types manifest as bugs:**
- **Missing VE**: A vertex of shell A on an edge of shell B produces degenerate near-zero ICs, handled by the Phase 1C band-aid filter.
- **Missing EE**: Edge crossings only detected at the triangle level; coarse tessellation near crossings causes inaccurate ICs.
- **Missing VF**: Corner-of-box-touching-face produces no IC but needs topology adjustment, causing unclassified `Unknown` faces.
- **Missing EF**: An edge piercing a face without face-face overlap is missed entirely.

### 1.3 Spec Gap Analysis

| Question | Spec Requirement | Current State | Gap |
|---|---|---|---|
| Separate intersection and corefinement stages? | Two distinct stages with well-defined intermediate representations | **Interleaved** — IC discovery and endpoint insertion into face boundaries happen in the same loop pass | **YES** |
| Per-IC `τ_local` tolerance? | "Every generated intersection curve segment MUST carry `τ_local`" | No `tau_local` field on `IntersectionCurveWithParameters`. All decisions use global `tol`. | **YES** |
| Adaptive refinement on projection failure? | "Reduce `τ_mesh` locally and recompute only the affected face pairs" | `search_triple` with fixed 100 iterations returns `None` on failure. No residual checking, no local recomputation. | **YES** |
| Avoid global quantization floors? | "Polyline construction must use a context tolerance, not a hardwired global `TOLERANCE`" | `construct_polylines` uses `2.0 * TOLERANCE` (1e-6) as spacing floor (`polyline_construction/mod.rs:23`). At 1 µm scale, distinct intersection points collapse into the same grid cell. | **YES** |
| `BooleanTolerance` usage? | Layered per-stage tolerances | Mixed — `BooleanTolerance` fields unpacked at the caller level, but core intersection (`intersection_curves`, `construct_polylines`, `search_triple`) uses hardcoded `TOLERANCE`. | **PARTIAL** |

### 1.4 Tolerance Usage Map

| Location | Tolerance Used | Source | Spec Requirement |
|---|---|---|---|
| `collision.rs:164` (coplanar triangle test) | `TOLERANCE2` (1e-12) | Hardcoded | Should use `τ_work` |
| `polyline_construction/mod.rs:19` (segment filter) | `TOLERANCE` (1e-6) | Hardcoded | Should use `τ_mesh` |
| `polyline_construction/mod.rs:23` (spacing floor) | `2 * TOLERANCE` | Hardcoded | Should use `τ_mesh` |
| `polyline_construction/mod.rs:54` (closure test) | `TOLERANCE` via `.near()` | Hardcoded | Should use `τ_mesh` |
| `intersection_curve/mod.rs:40` (search_triple) | `TOLERANCE` via `.near()` | Hardcoded | Should use `τ_work` |
| `mod.rs:751` (AABB margin) | `tol * 2.0` | Caller-provided | Reasonable |
| `mod.rs:767` (coplanar check) | `coplanar_tol` | Caller-provided | Matches spec |
| `mod.rs:909` (coincident vertex) | `tol` | Caller-provided | Should be `τ_weld` |
| `mod.rs:1097` (IC length filter) | `tol` | Caller-provided | Should be `τ_mesh` |
| `mod.rs:1116,1122` (IC endpoint snap) | `tol` | Caller-provided | Should be `τ_weld` |

**Assessment:** The tolerance model is two-tiered but not layered per spec. A single `tol` parameter serves 8+ semantically distinct purposes. Core intersection computation (polyline construction, search_triple) bypasses `BooleanTolerance` entirely.

### 1.5 Robustness Assessment

**Critical failure modes:**

1. **Tessellation-dependent accuracy**: IC quality depends entirely on mesh density. No adaptive refinement.
2. **Coplanar triangle miss**: `collide_seg_triangle` (`collision.rs:164`) returns `None` for coplanar segment-triangle pairs. If two triangles from different faces are coplanar, their intersection is silently dropped.
3. **Floating-point orientation test**: `ShapesOpStatus::from_is_curve` (line 61-77) determines And/Or from `normal0.cross(der).dot(normal1) > 0.0` — bare floating-point comparison with no robust predicate. Near tangential ICs produce unreliable classification.
4. **Newton convergence failure**: Both `search_triple` and `curve_surface_projection` use Newton iteration with fixed trial counts. Near singularities, Newton may not converge, silently dropping the IC.
5. **Quantization collapse at 1 µm**: The spacing floor of `2 * TOLERANCE = 2e-6` means intersection points within 2 µm merge into the same graph node.

**Tessellation-first vs. analytical SSI:**

| Criterion | Tessellation-first (ours) | Analytical SSI (OCCT) |
|---|---|---|
| Generality | Works for any surface type | Requires per-surface-pair formulas |
| Accuracy | Limited by mesh density | Exact up to curve approximation |
| Coplanar handling | Fundamentally broken (coplanar triangles → None) | Dedicated 2D overlap analysis |
| Tangential intersections | Very poor (tiny intersection region) | Dedicated tangency detection |
| Adaptivity | None (fixed tessellation) | Can adaptively refine marching step |

---

## Section 2: Face Division Stage (`divide_one_face`)

### 2.1 Algorithm Identification

The function `divide_one_face` (`divide_face/mod.rs:335-651`) implements a **parametric-space face splitting algorithm**:

1. **Parameter-space projection** (lines 347-363): Each wire converted from 3D to 2D parametric space via `surface.search_parameter()`.
2. **Area classification** (lines 349-362): Signed parametric area determines outer boundaries (positive) vs. holes (negative). Wires below `tau_area` discarded.
3. **Hole nesting** (lines 364-379): Negative-area wires assigned to containing positive-area wire via point-in-polygon.
4. **Biangle filtering** (lines 395-396): Degenerate 2-edge wires filtered.
5. **Proactive wire splitting** (lines 400-422): Non-simple wires split via `split_wire_recursive()` before face construction.
6. **Face construction** (lines 426-431): `Face::try_new(wires, surface)` validates closure, simplicity, disjointness.
7. **Recovery pipeline** (lines 433-646): 6-level cascade on failure.

### 2.2 Recovery Pipeline Assessment

| Level | Strategy | Lines | Principled? |
|---|---|---|---|
| 0 | Biangle filter | 395-396 | Semi-principled (matches regularization) |
| 1 | Proactive wire splitting | 400-422 | Ad-hoc (K8 test case driven) |
| 2 | `Face::try_new` | 426-431 | Primary construction, not recovery |
| 3 | Embedded-wire removal | 503-569 | Ad-hoc (proper-subset heuristic) |
| 4 | Reactive wire splitting | 571-598 | Ad-hoc (same algorithm, reactive trigger) |
| 5 | Merge+splice wires | 599-644 | Ad-hoc (figure-8 splice-then-split, no published basis) |
| 6 | Return `None` | 645 | Fallback — original face preserved as Unknown |

**Of 6 recovery levels: 1 semi-principled, 5 ad-hoc.** Each was developed iteratively to address specific test failures. No clear stopping condition or correctness guarantee.

### 2.3 Comparison to Literature

**OCCT Pave Block splitting:**

| Aspect | OCCT | Ours |
|---|---|---|
| Edge tracking | Pave Blocks with global IDs, parameter-ordered | No equivalent; edges tracked only as wire elements |
| Edge subdivision | Explicit via pave blocks | Via `add_polygon_vertex` cutting edges at IC endpoints |
| Face splitting | `BOPAlgo_BuilderFace` — dedicated builder | `divide_one_face` — parametric-space area classification |
| FaceInfo states | In/On/Sc with pave blocks per category | Only And/Or/Unknown per BoundaryWire |
| Common Blocks | Explicit tracking of coincident pave blocks | No equivalent; coincidence via vertex ID sharing |

**Key gap: No Pave Block abstraction.** Edge subdivisions tracked implicitly through wire membership. OCCT's model provides clean tracking of how each edge was split and which pieces are shared.

**Sugihara-Iri topology-oriented approach:**

Our code **does NOT follow** the topology-oriented principle. Evidence:
- Numerics drive topology, not vice versa (parametric-space projection and area computation directly determine topological structure)
- No topological invariant checking at decision points (checked only at `Face::try_new`, after all decisions made)
- Recovery is reactive, not preventive (fix violations after they occur rather than maintaining invariants)
- Numerical failures can cause complete face loss

A topology-oriented `divide_one_face` would define a topological skeleton ("partition wires into sub-face sets maintaining closure/simplicity/disjointness"), with numerics only choosing which partition, and backtracking if any choice violates invariants.

**Barki co-refinement:**

| Aspect | Barki | Ours |
|---|---|---|
| Working representation | 2D arrangements on triangular faces | Parametric-space polyline wires on NURBS faces |
| Subdivision | CDT triangulation of 2D arrangement | Wire-based face splitting via area classification |
| Projection | Bijective drop-coordinate | `surface.search_parameter()` (Newton, can fail) |
| Exactness | Exact rational arithmetic | Floating-point with tolerance-based checks |

### 2.4 Wire Splitting Analysis

`split_wire_recursive` (`divide_face/mod.rs:25-167`):
- Scans for repeated vertices, splits wire at each repeated vertex
- **Depth guard of 10** — empirical constant, not derived from theory
- Biangle wire detection: 2 edges sharing same `EdgeID` in opposite orientations
- **Failure modes**: Position-based coincidence without ID sharing; non-contiguous repeated vertex patterns; depth exceeded

### 2.5 Topological Invariant Preservation

| Invariant | Maintained? | Violation Paths |
|---|---|---|
| Wire closure | Partially | `create_parameter_boundary` failure → `None` → original face with partially modified boundaries |
| Manifoldness | Not guaranteed | Recovery can break edge identity with adjacent faces |
| Euler's formula (V-E+F=2) | Not checked | No `validate_euler_characteristic` call during face division |

**Specific violation-prone paths:**
- Embedded-wire removal (`mod.rs:530-565`): Can silently remove legitimate holes
- Merge+splice (`mod.rs:206-291`): May produce different face boundaries than original wires
- Area-based cancellation (`mod.rs:372-373`): Uses `tol` in parametric space where non-uniform scaling makes comparison unreliable

### 2.6 Spec Gap Analysis (Face Division)

| Gap | Severity | Spec Reference |
|---|---|---|
| No Pave Block abstraction for edge subdivision tracking | High | OCCT Sections 3.2-3.3 |
| Recovery pipeline is ad-hoc heuristics, not topology-oriented | High | Sugihara-Iri Section 3 |
| No `τ_local` per intersection curve segment | High | Spec "Local per-edge tolerances" |
| No FaceInfo (In/On/Sc); only And/Or/Unknown per wire | Medium | OCCT Section 3.6 |
| Edge identity may break during recovery levels 3-5 | Medium | Spec "consistent edge/vertex identity" |
| No Euler formula checking during face division | Medium | Spec "Postprocess & validate" |
| `search_parameter` failure silently drops entire face | Medium | Spec "structured diagnostic, not silent None" |

---

## Section 3: Coplanar Handling

### 3.1 Detection Algorithm

**`check_coplanar()` (`coplanar.rs:218-257`)**: Two-tier test on `FaceSampleInfo` (single boundary point + normal):
1. Angular test: normals (anti-)parallel via `(1.0 - dot.abs()) > tol * tol`
2. Distance test: `signed_plane_distance()` using Shewchuk `robust_orient3d` for the numerator

**Limitation:** Tests only **one sample point** per face. Can produce false positives for non-planar surfaces.

**`check_coplanar_faces()` (`coplanar_splitting.rs:58-197`)**: More thorough — tests **all vertices** of face1 against face0's plane:
1. Tier 1 — Exact: `exact_points_coplanar()` using `robust_orient3d` for every vertex
2. Tier 2 — Tolerance: `max_coplanar_deviation()` checking max perpendicular distance
3. 2D overlap verification: AABB overlap, point-in-polygon (winding number), edge intersection

**Pre-scan in `loops_store/mod.rs`**: O(n×m) scan with AABB culling (`loops_store/mod.rs:753-778`). Results used for: skipping coplanar pairs from main SSI loop, coplanar-adjacent skip, boundary loop injection.

### 3.2 Overlay Algorithm (`coplanar_overlay.rs`)

The overlay uses the **iOverlay** library (acceptable per spec). Core function `compute_coplanar_overlay()` (`coplanar_overlay.rs:156-321`):

1. **Projection to 2D**: Both faces projected into shared coordinate system from face0's plane
2. **Three overlay operations**: A∩B (overlap), A\B (face0-only), B\A (face1-only)
3. **Fragment classification**:
   - Same-sense overlap → And
   - Anti-sense overlap → And (both removed as internal)
   - Non-overlapping regions → Or

Uses `FillRule::EvenOdd` and supports shapes with holes.

### 3.3 Comparison to Literature

**OCCT Same-Domain analysis:**
- OCCT tracks Same-Domain relationships via `myShapesSD` and `AreFacesSameDomain()`. Groups into connexity chains. Our code has **no equivalent** of connexity chains.
- OCCT uses FaceInfo with In/On/Sc states. Our code uses simpler And/Or/Unknown.
- OCCT's staged interference feeds lower-dimensional results into coplanar handling. Our pre-scan is independent.

**Zhou winding numbers:**
- Zhou's classification is purely combinatorial — winding numbers propagate via BFS. Coplanar faces need no special classification because winding number differences across any patch boundary are exactly ±1.
- **Our code does not use winding numbers at all** for coplanar classification. Uses point-in-polygon + area intersection + ray-cast fallback instead.

**Cherchi indirect predicates:**
- Cherchi uses auxiliary tetrahedron trick for coplanar intersection points and global pocket deduplication. Our code doesn't need the tetrahedron trick (projects to 2D + iOverlay), but lacks pocket deduplication for multiple coplanar face pairs.

### 3.4 Classification of Coplanar Fragments

**3-tier cascade** (`integrate/mod.rs:734-756`):
1. `classify_coplanar_via_overlay()` — full 2D polygon overlay
2. `classify_coplanar_fragment()` — single-point test fallback
3. `ray_cast_classify()` — ray casting if neither coplanar method works

**Same-sense shortcut** (`coplanar.rs:80-91`): Return Or for same-sense non-overlapping. **Correct** — normal direction guarantees same side of solid.

**Anti-sense path** (`coplanar.rs:70-77`): Falls through to ray-cast. **Correct** — anti-parallel normals mean non-overlapping region could be inside.

**Correctness issues:**
- **Partial anti-sense overlap**: `classify_coplanar_via_overlay` classifies **entire** face as `Remove` if **any** overlap exists (`coplanar_overlay.rs:473-474`). Non-overlapping portion should potentially be classified differently.
- **Multiple coplanar faces with mixed sense**: Anti-sense check takes priority; partial anti-sense overlap can incorrectly classify the whole face as `Remove`.

### 3.5 Spec Gap Analysis (Coplanar)

| Gap | Severity | Spec Reference |
|---|---|---|
| `compute_coplanar_overlay()` and `inject_overlay_fragments()` are dead code — overlay only used for classification, not corefinement | High | Spec: "2D overlay logic on trimming loops" |
| Partial anti-sense overlap classifies entire face as Remove | High | Correctness |
| iOverlay does not use exact predicates internally | High | Spec: "Robust geometric predicates for load-bearing decisions" |
| No OCCT-style Same-Domain connexity chains | Medium | OCCT reference |
| No winding number classification | Medium | Zhou reference |
| Uses `EvenOdd` fill rule; can mishandle self-touching boundaries | Medium | Robustness |
| Progressive union for multi-face merge is numerically unstable | Medium | Robustness |
| Hard-coded ratios (`boundary_tol = tol * 0.01`, `min_area = tol²`) | Low | Spec: independently configurable layers |

---

## Consolidated Gap Summary

### Critical Gaps (must fix for production correctness)

| ID | Stage | Gap | Severity |
|---|---|---|---|
| P1-G1 | Intersection | 5 of 6 OCCT interference types missing or partial (VE, EE, VF, EF fully or partially missing) | Critical |
| P1-G2 | Intersection | Core intersection uses hardcoded `TOLERANCE` (1e-6), collapsing 1 µm features | Critical |
| P1-G3 | Intersection | `ShapesOpStatus` determination from normal cross product uses bare f64, not robust predicate | Critical |
| P1-G4 | Intersection | No `τ_local` per intersection curve segment | High |
| P1-G5 | Intersection | No adaptive refinement on projection failure | High |
| P1-G6 | Intersection | Coplanar triangle pairs silently dropped in `collide_seg_triangle` | High |
| P1-G7 | Division | Recovery pipeline is 5 levels of ad-hoc heuristics, not topology-oriented | High |
| P1-G8 | Division | No Pave Block abstraction for edge subdivision tracking | High |
| P1-G9 | Coplanar | Overlay only for classification, not corefinement (dead code for face splitting) | High |
| P1-G10 | Coplanar | Partial anti-sense overlap misclassifies entire face | High |
| P1-G11 | Coplanar | iOverlay lacks exact predicates for near-degenerate 2D geometry | High |

### Medium Gaps (functional but non-compliant)

| ID | Stage | Gap | Severity |
|---|---|---|---|
| P1-G12 | Intersection | Intersection and corefinement interleaved, not separate stages | Medium |
| P1-G13 | Intersection | Single `tol` parameter used for 8+ semantically distinct purposes | Medium |
| P1-G14 | Intersection | Newton convergence failure silently drops ICs | Medium |
| P1-G15 | Intersection | No structured error reporting (returns `Option<...>`) | Medium |
| P1-G16 | Division | Edge identity may break during recovery levels 3-5 | Medium |
| P1-G17 | Division | No Euler formula checking during face division | Medium |
| P1-G18 | Division | No FaceInfo (In/On/Sc); only And/Or/Unknown per wire | Medium |
| P1-G19 | Coplanar | No Same-Domain connexity chains | Medium |
| P1-G20 | Coplanar | Progressive union for multi-face merge is numerically unstable | Medium |

### Low Gaps (aspirational or edge-case)

| ID | Stage | Gap | Severity |
|---|---|---|---|
| P1-G21 | Intersection | O(V0×V1) coincident vertex scan | Low |
| P1-G22 | Intersection | Polyline-based ICs are C0, no curvature information | Low |
| P1-G23 | Division | Depth guard of 10 for wire splitting is arbitrary | Low |
| P1-G24 | Division | Area-based cancellation uses global `tol` in parametric space | Low |
| P1-G25 | Coplanar | No winding number classification | Low |
