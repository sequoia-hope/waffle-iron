# Algorithm Decision Records

Each record documents a critical subsystem design choice: what we chose, what we
rejected, and why. Reference numbers `[#N]` refer to REFERENCES.md.

---

## ADR-1: Boolean Pipeline Architecture

**Decision**: Hybrid B-Rep/mesh boolean [#24]

**Alternatives considered**:

| Approach | Reference | Pros | Cons |
|----------|-----------|------|------|
| **Hybrid B-Rep/mesh** | [#24] Yang 2025 | Zero failures on test suite, 17x faster than OCCT, chained booleans via re-mapping, topology from exact mesh + geometry from NURBS | Requires bijective tessellation + re-mapping infrastructure |
| Pure B-Rep (truck-style) | [#3] OCCT, [#2] Hoffmann | Familiar, no mesh intermediate | Fragile at degeneracies, tolerance-dependent, 28 sprints of patches prove this doesn't converge |
| Pure exact mesh | [#8] Zhou, [#9] Cherchi | Provably correct topology | Loses NURBS geometry, discretization-dependent accuracy |
| Nef polyhedra | [#15] Hachenberger | Closed under all boolean/topological ops by construction | Flat faces only (no NURBS), heavy exact arithmetic, 4-6x slower than ACIS |
| BSP-tree | [#18] Bernstein | Fast (16-28x vs CGAL Nef), only 4 predicates | Flat faces only, plane-based representation limits applicability |

**Rationale**: Our 28-sprint experience with pure B-Rep booleans (truck fork)
demonstrates that tolerance-based approaches don't converge — each fix introduces
new edge cases. The hybrid approach [#24] separates the hard problem (topological
correctness, solved exactly on meshes) from the tractable problem (geometric
accuracy, preserved via original NURBS surfaces). Zero failures on Yang's test
suite vs. non-trivial failure rates for OCCT, ACIS, and SolidWorks.

---

## ADR-2: Surface-Surface Intersection (SSI)

**Decision**: Hybrid architecture — algebraic topology determination [#25] as
preprocessing for numerical tracing, with IATA [#29] fallback for
tangential/degenerate cases.

**Alternatives considered**:

| Approach | Reference | Pros | Cons |
|----------|-----------|------|------|
| **Topology-guaranteed tracing** | [#25] Yang 2023 | Correct topology on all 8 benchmark cases (OCCT: 4/8, ACIS: 6/8, SW: 5/8) | Dixon resultant high algebraic degree for high-order NURBS |
| Lattice/grid | [#1] Patrikalakis, [#3] OCCT IntPatch | Simple, well-understood | Resolution-dependent, misses features between grid points |
| Marching | [#1] Patrikalakis | Fast for easy cases | Misses disconnected branches, fails at singular points |
| Subdivision (Bezier clipping) | [#1] Patrikalakis | Robust | Slow for high-degree surfaces |
| **IATA** | [#29] Cheng 2023 | Handles all singularity types (tangent, cusps, tiny loops, self-intersections) | Slower on easy cases, limited to rational Bezier currently |

**Rationale**: The SSI survey [#27] concludes: "No single SSI method handles all
cases — hybrid architecture is necessary." Algebraic topology determination [#25]
succeeds where OCCT/ACIS/SolidWorks fail (8/8 vs 4-6/8 on benchmarks). IATA [#29]
handles the hardest degenerate cases (tangent points, tiny loops) that even [#25]
doesn't fully address. Using topology as a constraint for numerical tracing means
the tracer knows when it's gone wrong — a missing branch is a detectable error,
not a silent failure.

---

## ADR-3: Inside/Outside Classification

**Decision**: Generalized winding numbers on trimmed NURBS [#30], with mesh-based
GWN [#7] as fast path for non-degenerate cases.

**Alternatives considered**:

| Approach | Reference | Pros | Cons |
|----------|-----------|------|------|
| **GWN on trimmed NURBS** | [#30] Spainhour 2026 | No tessellation dependency, graceful degradation for non-watertight geometry, principled coplanar handling (GWN=0.5 at surface), BSD-licensed Axom implementation | 0.004-94ms per query, newer/less battle-tested |
| **GWN on mesh** | [#7] Jacobson 2013 | Fast, proven in Blender/libigl, works on non-manifold/self-intersecting | Tessellation-dependent accuracy — the discretization can flip classification at tight geometry |
| Ray-cast voting | [#17] Requicha | Simple to implement | Grazing rays, edge-coincident rays, majority voting unreliable for near-boundary points |
| Winding number vectors + BFS | [#8] Zhou 2016 | O(1) per cell after arrangement computation | Requires complete mesh arrangement first |
| 8-way classification tables | [#20] Astarlioglu | Fine-grained on+/on- states | Complex, vertex-neighborhood-based, doesn't handle curved surfaces natively |

**Rationale**: Our pipeline's most persistent bugs (B14-B27) stem from
tessellation-dependent classification — the mesh approximation flips inside/outside
at tight geometry. GWN on trimmed NURBS [#30] eliminates this entire class by
evaluating directly on the original surfaces. The Stokes' theorem dimension
reduction makes it efficient (>90% of queries are far-field, costing 0.004ms).
Mesh-based GWN [#7] serves as the fast path for the hybrid pipeline's mesh boolean
stage, where it's already proven reliable.

---

## ADR-4: Coplanar / Overlap Handling

**Decision**: Overlap region extraction as 2D phenomenon [#26], performed as
preprocessing before SSI.

**Alternatives considered**:

| Approach | Reference | Pros | Cons |
|----------|-----------|------|------|
| **Overlap extraction (2D bilevel)** | [#26] Yang 2025 | Correct framework — overlap is 2D with 1D boundary, shared topological entity from the start | Requires bilevel optimization |
| Same-domain connexity chains | [#3] OCCT | Production-proven in OCCT | Complex, OCCT-specific data structures, tolerance-dependent |
| 1D IC injection (our B14-B26) | Custom | Already implemented (~3000 lines) | Fundamentally wrong — forces 1D semantics on 2D phenomenon, 12 sprints of patches prove this doesn't converge |
| Imprinting | [#33] Stroud §6.1 | Classical approach | Requires matching sub-face creation on both surfaces |
| CDT clustering | [#8] Zhou, [#10] Levy | Groups coplanar mesh triangles for joint processing | Mesh-level only, no NURBS |

**Rationale**: Our D1 experience (B14-B26, ~3000 lines of patches) proves that
treating coplanar intersection as a 1D curve problem doesn't work. Yang [#26]
identifies the correct framework: overlap is a 2D region bounded by a 1D curve
where one surface's trim boundary crosses the other. The overlap boundary is
computed as a shared topological entity from the start (via bilevel optimization),
avoiding the `add_independent_loop` topology isolation that broke our approach.
Overlap detection becomes preprocessing before SSI, not a special case within it.

---

## ADR-5: Geometric Predicates

**Decision**: Shewchuk adaptive predicates [#4] with SoS tiebreaking [#5] for
all load-bearing geometric decisions.

**Alternatives considered**:

| Approach | Reference | Pros | Cons |
|----------|-----------|------|------|
| **Adaptive precision + SoS** | [#4] + [#5] | Fast for easy cases (>99.9999% [#19]), exact when needed, zero degenerate-case special handling | Requires exact arithmetic library |
| Indirect predicates | [#9] Cherchi 2020 | Float-only hardware, no exact arithmetic library needed | Limited to orient2d/3d, 10 variants needed |
| Fixed epsilon comparisons | truck current | Simple | Fragile — epsilon too large masks errors, too small triggers false negatives |
| Full exact arithmetic (CGAL) | [#15] Hachenberger | Provably correct always | 4-6x slower, heavy dependency |
| Topology-oriented (no precision req) | [#6] Sugihara | Works even with random numeric results | Problem-specific Q properties required, output is approximation |

**Rationale**: Devillers & Preparata [#19] prove that for orient3d (our most-used
predicate), floating-point filters fail ~10^-14 of the time — essentially never.
Shewchuk's adaptive approach [#4] adds exact arithmetic cost only for the
vanishingly rare degenerate cases. SoS [#5] eliminates ALL degenerate-case special
handling (no "what if the determinant is exactly zero?" branches). Combined, they
give us: fast normal-case performance, provably correct degenerate handling, and
drastically simpler code (no epsilon branches). The `robust` crate already
provides orient2d/orient3d; we need to extend usage to all geometric decisions.

---

## ADR-6: Topology Layer

**Decision**: Euler operator framework [#16, #33 Ch.4] — all topology mutations
via Euler operators that preserve the Euler-Poincare relation.

**Alternatives considered**:

| Approach | Reference | Pros | Cons |
|----------|-----------|------|------|
| **Euler operators** | [#16] Mantyla, [#33 Ch.4] Stroud | Manifoldness by construction, proven complete (any valid B-rep change expressible), incremental validity | 99 operators (spanning set of 10 suffices), requires disciplined usage |
| Direct half-edge manipulation | truck current | Flexible, simple API | No topological validity guarantee, easy to create invalid states (our `finalize_boolean_shell` struggles) |
| Nef sphere maps | [#15] Hachenberger | Handles non-manifold, exact | Heavy infrastructure, flat faces only |
| Weiler model | [#10] Levy, [#12] Barki | Handles non-manifold edges natively | More complex data structure |

**Rationale**: Our boolean pipeline's most persistent structural problem is
topology corruption — `finalize_boolean_shell` produces shells with singular
vertices, open edges, and invalid wire structures. Direct half-edge manipulation
(truck's approach) makes it easy to create topologically invalid intermediate
states that are difficult to recover from. Euler operators guarantee
V-E+F-2(S-G)=0 at every intermediate step, making corruption impossible by
construction. Mantyla [#16] proves the spanning set {MEV, MEF, MBFV, MGB, MEKL +
inverses} is complete — any valid topology change can be expressed as a sequence.
Stroud [#33 Ch.4] provides the matrix decomposition for verifying operator
sequences and the full catalog of 99 operators.

---

## ADR-7: Tolerance Architecture

**Decision**: Six-type tolerance policy [#33 Ch.16] with relative tolerances
after scale normalization [#24, #27, #31].

**Alternatives considered**:

| Approach | Reference | Pros | Cons |
|----------|-----------|------|------|
| **6-type policy + scale normalization** | [#33 Ch.16], [#24], [#31] | Each tolerance type has defined purpose and consistency rules, scale-independent via normalization | Requires discipline to maintain 6 distinct tolerances |
| Single absolute tolerance | truck current (`1e-6`) | Simple | Breaks at mm-scale (our B27 root cause), no distinction between modeling/intersection/visualization tolerances |
| Fuzzy values (per-entity) | [#3] OCCT `SetFuzzyValue` | Per-operation control, principled nearby-geometry merging | Complex tolerance propagation through operations |
| No tolerance (exact arithmetic) | [#15] Hachenberger, [#13] ESOLID | Eliminates tolerance issues entirely | Expensive, flat faces only (Nef) or low-degree only (ESOLID) |

**Rationale**: truck's single absolute tolerance (`1e-6`) is the root cause of
our mm-scale failures (B27/B28) — the tolerance is ~5% of geometry extent at
0.02mm scale. Scale normalization [#24, #27, #31] makes all tolerances relative to
geometry extent. Stroud's 6-type policy [#33 Ch.16] (machine, length, angle,
geometric, relative, polynomial) with consistency rules prevents the ad-hoc
epsilon proliferation that made our truck fork's tolerance landscape
unmaintainable.

---

## ADR-8: Model Validation

**Decision**: ACIS-style body checker [#33 §14.1] + algebraic self-intersection
detection [#31] as pre- and post-boolean validation.

**Alternatives considered**:

| Approach | Reference | Pros | Cons |
|----------|-----------|------|------|
| **Body checker + algebraic SI detection** | [#33 §14.1], [#31] Li 2025 | Systematic checks (edge convexity, containment, SI), algebraic signature is microsecond pre-check from control points | Requires implementation of full checker |
| Euler characteristic only | [#23] Edelsbrunner | Fast (V-E+F=2 check) | Necessary but not sufficient — topology can satisfy Euler formula while being geometrically invalid |
| Mesh-level watertightness | Current approach | Easy to compute on tessellation | Misses B-Rep level issues (self-intersecting NURBS, invalid trim curves) |
| No validation (trust the pipeline) | — | Zero overhead | Garbage-in-garbage-out for chained booleans |

**Rationale**: Post-boolean validation is essential for chained booleans — one
corrupt intermediate result corrupts all downstream operations. Li's algebraic
signature [#31] provides a microsecond pre-check (certifies self-intersection-free
from control points alone), catching problems before they enter the SSI pipeline.
Stroud's body checker [#33 §14.1] provides comprehensive post-boolean validation
(edge convexity, face containment, dangling elements). Together they form a
pre/post validation sandwich that catches problems early and prevents propagation.

---

## ADR-9: Tessellation

**Decision**: Curvature-adaptive tessellation with flatness bounds from control
point analysis [#31 Theorem C.3].

**Alternatives considered**:

| Approach | Reference | Pros | Cons |
|----------|-----------|------|------|
| **Curvature-adaptive with flatness bounds** | [#31], [#22] Sullivan | Tight error bounds from control points, adapts to surface complexity | Requires subdivision infrastructure |
| Single chordal tolerance | truck current | Simple, one knob | Over-tessellates flat regions, under-tessellates high-curvature regions |
| CGAL isotropic surface meshing | CGAL docs | Production-proven, quality guarantees | C++ dependency, heavy infrastructure |
| Uniform grid sampling | — | Trivially parallel | Wastes triangles on flat regions |

**Rationale**: Li [#31] provides a tight flatness bound (Theorem C.3) computed
directly from second-order control point differences — no surface evaluation
needed. This enables adaptive tessellation that concentrates triangles where
curvature demands them while using few triangles on flat regions. Sullivan [#22]
provides the theoretical framework for curvature across the smooth/discrete
boundary, informing error bounds. For the hybrid boolean pipeline [#24], the
tessellation also needs to be bijective (each triangle maps to exactly one B-Rep
face), which constrains the meshing algorithm but is well-understood.

---

## ADR-10: Analytical Primacy — Exact SSI for Quadric Surfaces

**Decision**: All boolean operations on quadric surfaces (plane, cylinder, cone,
sphere, torus) use exact surface-surface intersection (SSI). The mesh/polygon
boolean path is reserved for freeform NURBS/BSpline surfaces only.

**Alternatives considered**:

| Approach | Reference | Pros | Cons |
|----------|-----------|------|------|
| **Closed-form SSI per quadric pair** | [#1] Patrikalakis Ch.5, [#25] Yang, [#27] Li | Zero discretization error, exact geometry preserved through chains, minimal topology, surface types retained | 15 pair-specific solvers to implement |
| Unified mesh boolean for all surfaces | [#8] Zhou, [#9] Cherchi | Single code path, no surface detection needed | Geometric drift compounds through chains, surface type information lost, over-tessellated analytical surfaces |
| Mesh boolean + post-refinement | [#24] Barton | Better accuracy than raw mesh, bijective re-mapping | Still loses exact geometry, refinement adds complexity without eliminating drift |
| Algebraic implicitization | [#25] Yang (Dixon resultant) | Unified algebraic framework | High algebraic degree for non-quadric pairs, heavy symbolic computation |

**Rationale**: The chained boolean problem (box + cyl1 → result + cyl2) demonstrates
why mesh fallback fails for quadric surfaces: the first boolean's mesh approximation
assigns `SurfaceGeom::Planar` to all result faces, destroying analytical geometry.
The second boolean operates on degraded geometry, compounding error. Patrikalakis [#1]
documents closed-form SSI for all quadric pairs. Yang [#25] provides
topology-guaranteed tracing that preserves surface identity. Li [#27] recommends the
hybrid approach: analytical for quadric pairs, topology-guaranteed for freeform.
Stroud [#33] Ch.6 describes boolean pipeline integration with analytical SSI.
Barton [#24] demonstrates that even with bijective re-mapping, mesh intermediates
introduce unnecessary error for surfaces with known exact intersections.

See ARCHITECTURAL_INVARIANTS.md A15 for the full invariant and implementation
sequence (15 quadric pairs ordered by CAD frequency).

---

## Cross-Reference: ADR → Subsystem → Key Files (Current)

| ADR | Subsystem | Current Implementation | Target |
|-----|-----------|----------------------|--------|
| ADR-1 | Boolean pipeline | `vendor/truck/truck-shapeops/` | New clean-sheet kernel |
| ADR-2 | SSI | `vendor/truck/truck-shapeops/src/analytical_ssi/` | Topology-guaranteed [#25] + IATA [#29] |
| ADR-3 | Classification | `vendor/truck/truck-shapeops/src/robust_classify.rs` | GWN on NURBS [#30] |
| ADR-4 | Coplanar | `vendor/truck/truck-shapeops/src/divide_face/coplanar.rs` | Overlap extraction [#26] |
| ADR-5 | Predicates | `vendor/truck/truck-shapeops/src/robust_classify.rs` (partial) | Full adaptive + SoS |
| ADR-6 | Topology | `vendor/truck/truck-topology/` | Euler operator framework |
| ADR-7 | Tolerances | Scattered across truck crates | Centralized 6-type policy |
| ADR-8 | Validation | `crates/test-harness/` (test-time only) | Pre/post-boolean validation layer |
| ADR-9 | Tessellation | `vendor/truck/truck-meshalgo/` | Curvature-adaptive with flatness bounds |
| ADR-10 | SSI (quadric pairs) | `crates/kernel/src/boolean.rs` | Exact SSI per A15 |
