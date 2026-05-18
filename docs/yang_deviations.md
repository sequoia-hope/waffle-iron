# Yang/Cherchi Implementation Deviations Log

**Purpose:** authoritative record of known divergences between the Yang 2025 hybrid B-Rep/mesh boolean pipeline as specified in the paper, and the implementation in `crates/kernel/`. Any deviation listed here MUST have either (a) a user sign-off with stated rationale, or (b) an active remediation tracked.

**Discipline:** per `CLAUDE.md` "Paper-Spec Compliance is MANDATORY," deviations are errors. Investigation on a component with an open deviation is blocked until either the deviation is fixed or signed off in writing.

---

## Open deviations (NOT signed off; investigation blocked)

### D1 — Simplified earcut used where Yang §4.4.1 specifies CDT

**Status:** OPEN. NOT signed off.

**Discovered:** 2026-05-16 during PR-Y46 architectural review.

**Code location:**
- `crates/kernel/src/tessellation/mod.rs:3471` — `earcutr::earcut(...)` call (convex path)
- `crates/kernel/src/tessellation/mod.rs:3509` — `earcutr::earcut(...)` call (with-holes path)
- `tessellate_planar_face_bounded` in `tessellation/mod.rs` (per-face independent triangulation entry point)

**Paper prescription:** Yang 2025 §4.4.1 ("Mesh updating") at `refs/text/yang2025_hybrid_boolean.txt:548-590`. Quote: "through CDT we obtain valid discretizations of the trimmed meshes" (line ~557). Yang specifies **Constrained Delaunay Triangulation** — an algorithm that enforces intersection-curve polyline boundaries as hard constraint edges across all faces that meet at a shared boundary.

**Current implementation:** uses the `earcutr` crate (Livesu et al. 2021 "Deterministic Linear Time Constrained Triangulation Using Simplified Earcut"). The simplified earcut is a deterministic O(n) ear-removal algorithm. It does NOT enforce constrained edges across face boundaries; each B-Rep face's earcut runs independently with no cross-face coordination beyond shared `disc.positions` vertex pool.

**Architectural consequence:** when two adjacent B-Rep faces meet at a shared boundary that includes intersection-curve vertices, both faces' earcut calls receive the same boundary vertex positions (via shared `disc.positions`), but each face's earcut chooses diagonals independently. This produces the "Case D" defect signature measured across PR-Y43-Y46: all 3 vertex positions are present in the final Render LOD vertex set (sub-class (a) `m1x=3, m5x=3`), but no triangle connects them because each face's independent earcut emitted incompatible diagonals.

**Investigation cost prior to discovery:** 15 PR cycles (PR-Y32 through PR-Y46), ~2821 LOC of probe scaffolding, 0 production code, F0020 unpaired count unchanged at 40 across the entire arc.

**Remediation options:**
1. Constrained earcut wrapper (~80-150 LOC) — detect B-Rep shared edges, pass as hard constraints to earcut OR post-process diagonal flips. Closest to current architecture.
2. Replace `earcutr` with a true CDT (full Bowyer-Watson + constraint enforcement, or port from CGAL / use a Rust CDT crate). ~800-1500 LOC. Highest paper-alignment.
3. Cross-face coordination phase before per-face earcut. ~50-100 LOC for boundary alignment but doesn't fix earcut divergence root.

**Investigation status:** **REMEDIATION IN PROGRESS.** Earcut sweep complete; Tier 2/3 remain.

**Cycle 1 (2026-05-16):** Replaced `earcutr` with `spade::ConstrainedDelaunayTriangulation::try_bulk_load_cdt` at `tessellate_planar_face_bounded` (two call sites: planar-no-holes, planar-with-holes). Constraint edges constructed from boundary loops. Results: F0020 unpaired 40→35; kernel lib 1262→1266; yang_fast 10/157→13/157.

**Cycle 1.1 (2026-05-16):** Removed the earcut-fallback arm at both call sites — `try_bulk_load_cdt`'s silent-conflict callback handles upstream defects internally, so the fallback was unreachable on F0020. Replaced fallback with eprintln + empty output.

**Cycle 1.2 (2026-05-16):** Swept the 9 remaining earcut call sites across the kernel — each replaced with `cdt::cdt_triangulate_flat` (earcut-shaped flat-API wrapper). Sites converted: `boolean/coplanar_preprocess.rs::triangulate_polygon_with_holes` (Yang §4.5.5 path), plus 8 sites in `tessellation/mod.rs` (convex/non-convex polygon, revolve caps, cylinder strip, sphere/torus cap). Removed `earcutr` from `Cargo.toml`. Results: kernel lib 1266→1268; yang_fast 13/157 (unchanged); F0020 35 unpaired (unchanged); cdt unit tests 4→6 passing.

**Earcut now removed from the kernel.** Confirmed via `grep -rn earcutr crates/kernel/src/` returning zero hits.

**Cycle 2a (2026-05-16):** Plumbed `edge_is_intersection` from `ResultTopology` through `WaffleSolid` to a thread-local at tessellation entry. Added `Y47T2_INTERSECTION_PROBE` env-gated probe at `tessellate_solid_bounded` that walks each face's boundary loops and counts which segments map to intersection-flagged arena edges. Default-off byte-identical (F0020 35 unpaired unchanged); kernel lib 1266→1268.

**MEASUREMENT (load-bearing):** On F0020 with probe enabled, the thread-local marker carries 48 arena edges of which 20 are flagged `is_intersection=true`. The first version of the probe used vertex-index inversion via `disc.edge_verts` and reported `intersection_segs=0` across all faces — **that probe had a bug**: vertex inversion is ambiguous because a single vertex belongs to multiple edges. The corrected probe walks half-edges directly (`arena.half_edges[he].edge`) and gives the exact `EdgeIdx` per boundary segment.

**Corrected measurement:** all 20 flagged edges DO appear on boundary loops, totaling 40 face-loop incidences across 7 faces:
- Face4, Face7, Face13: walked_edges=10, walked_intersection=10 — **entirely intersection-bordered** (these are trim faces born from the boolean)
- Face9, Face10, Face11, Face12: walked_edges=5-6, walked_intersection=2-3 — partial intersection borders

**The boundary loops already include the intersection edges, and the CDT calls already pass every boundary segment as a constraint edge** (via the `loops` parameter to `cdt_triangulate_2d_with_loops`). So "Tier 2 = add intersection-curve constraints" produces no new constraints — they're already there.

**Tier 2 EMPIRICALLY REFUTED, with corrected reasoning.** The intersection edges aren't a missing-constraint problem; the boundary IS the intersection in these cases. The Case D 24/24 defect must come from CDT divergence on the SAME boundary input across adjacent faces. Cross-face shared-edge analysis from the deep probe:
- Face4 ↔ Face13: share 10 intersection edges (one large intersection patch boundary).
- Face7 ↔ Face9, Face10, Face11, Face12: share intersection edges across 4 neighboring faces.

The 24 Case D missing triangles are presumably the ones spade chose differently on each side of these shared boundaries.

**Remaining work to close D1 (revised after cycle 2a corrected measurement):**
- **Tier 3:** canonical cross-face Newell basis — eliminate ±ε 2D-projection drift between adjacent faces sharing intersection-edge boundaries (Face4 ↔ Face13 and Face7 ↔ {Face9,10,11,12} on F0020). With identical 2D inputs, deterministic CDT would produce identical outputs on shared regions. Estimated load-bearing.
- **Cross-face vertex-set divergence:** adjacent faces' CDT calls operate on different non-shared vertex sets (each face has its own interior vertices). Even with identical boundary constraints and identical Newell bases, Delaunay diagonal choice on shared-boundary triangles depends on the full vertex set. Forces investigation of global-coplanar-CDT (architectural rewrite Candidate C from the earlier review). The Face4/Face13 case is concrete: both faces' CDT calls see different non-shared vertex sets but identical 10-edge intersection boundary, and presumably emit different diagonals on the shared region.
- **Spade-CDT determinism caveat:** verify that spade actually emits identical triangulations given identical input (vertices + constraints) across separate calls. If spade's `bulk_load_cdt` has any input-order dependence we don't control, that's a contributing factor.

**Implementation choice note:** Yang §4.4.1 says "CDT in CGAL [2024]." We use `spade` (Rust-native CDT, ~5-10k LOC) rather than porting CGAL's CDT (~12k lines of C++ templates plus kernel/predicate infrastructure). `spade` implements the same algorithm class (Constrained Delaunay Triangulation with adaptive predicates via the `robust` crate); the deviation is "CGAL specifically" not "CDT semantics." This is a documented choice, not a behavioral deviation.

**Sign-off:** *not eligible.* Deviation remains until Tier 2 (and possibly Tier 3) lands.

---

## Audit findings (2026-05-18)

Three parallel Explore agents audited Yang §4.1 through §4.5 against the code. Findings consolidated below. Entries grouped by category. Each is independent of D1.

### Fundamental — replace wholesale

#### D2 — Extra post-tessellation repair pipeline (legacy S-H residue)

**Status:** OPEN. NOT signed off.

**Code location:** `crates/kernel/src/tessellation/repair.rs` (entire module, ~925 LOC); called from `crates/kernel/src/tessellation/mod.rs` lines ~650-770 in a multi-pass loop.

**Paper section:** Yang §4.4.3 (`yang2025:599-605`).

**Paper requirement:** "The watertightness of our result is **inherited from the mesh Boolean output**, ensuring the mesh has no geometric gaps." Yang asserts watertight-by-construction with no post-processing.

**Current implementation:** Hundreds of LOC of post-tessellation repair passes: `remove_winding_insensitive_duplicates`, `remove_nonmanifold_topology_aware`, `remove_nonmanifold_duplicates_aggressive`, progressive `weld_boundary_vertices_with_scale`, `fill_boundary_holes`, `close_near_boundary_chains`, `resolve_mesh_t_junctions`, multi-pass convergence loops. The module's own comment (line ~12) labels these "deprecated S-H clipping repair pipeline" that "mask classification errors."

**Deviation magnitude:** Fundamental. This entire layer doesn't exist in Yang.

**Notes:** These passes mask defects from upstream stages. With Yang done correctly, they should be unnecessary. Removing them will likely surface real upstream defects that the repair currently hides.

#### D3 — §4.5.4 illegal-intersection detection/removal absent

**Status:** OPEN. NOT signed off.

**Code location:** Not present.

**Paper section:** Yang §4.5.4 (`yang2025:752-758`).

**Paper requirement:** "We detect these illegal intersections [self-intersections in the trimmed mesh arising from discretization or mesh updating] and perform local refinement. Since the input B-Rep model has no self-intersections, these illegal intersections are eliminated."

**Current implementation:** No detection or removal logic for post-trim self-intersections. The `no_self_intersection` oracle counts them as a test gate but doesn't fix them.

**Deviation magnitude:** Fundamental. An entire §4.5.4 step is missing.

### Structural — algorithm differs

#### D4 — §4.5.2 global re-tessellation instead of localized refinement

**Status:** OPEN. NOT signed off.

**Code location:** `crates/kernel/src/boolean/yang_integration.rs:843-904`.

**Paper section:** Yang §4.5.2 (`yang2025:659-670`).

**Paper requirement:** When optimization fails in a region, refine only the surfaces traversed by the failed intersection curve segment plus a one-ring of neighbors. Re-compute intersections only in the refined regions.

**Current implementation:** When the SSI optimization can't recover, the entire pipeline halves `d_ε` and re-tessellates BOTH solids globally, up to 2 rounds. Not localized.

**Deviation magnitude:** Structural.

#### D5 — §4.4.1 r_A = r_B = r identification not explicit at CDT construction

**Status:** OPEN. UNCERTAIN — needs deeper trace.

**Code location:** `crates/kernel/src/tessellation/mod.rs:3377-3407`, `cdt.rs:39-80`.

**Paper section:** Yang §4.4.1 (`yang2025:548-556`).

**Paper requirement:** Before CDT, explicitly identify every intersection point's representation on mesh A and mesh B by setting `r_A = r_B = r` so both meshes share byte-identical vertex values along the intersection curve.

**Current implementation:** Shared vertices come implicitly from the shared `disc.positions` pool. No explicit "set r_A = r_B" step. May be functionally equivalent if the pool guarantees identity, but the paper's explicit step is absent.

**Deviation magnitude:** Structural. Possibly cosmetic if functionally equivalent.

#### D6 — §4.4.1 Fig 11 split/merge/insert procedures unclear

**Status:** OPEN. UNCERTAIN.

**Code location:** `crates/kernel/src/boolean/topology_extract.rs:249`, `mesh_arrangement.rs`, `cherchi/fast_trimesh.rs`.

**Paper section:** Yang §4.4.1 Fig 11 (`yang2025:555-565`).

**Paper requirement:** Three preprocessing steps before CDT:
- (a) Locate the constraint edge containing intersection point q; split it at q.
- (b) If a split-edge endpoint p is too close to q, merge p with q.
- (c) If an intersection loop has no interior mesh vertices, insert one.

**Current implementation:** Generic `split_edge` and `insert_vertex_into_triangle` routines exist. Whether they're applied in this exact sequence at CDT-prep time is unclear.

**Deviation magnitude:** Structural pending clarification.

#### D7 — §4.4.2 patch segmentation: flood-fill vs Cherchi 2022 per-patch ray-cast

**Status:** OPEN. UNCERTAIN.

**Code location:** `crates/kernel/src/boolean/topology_extract.rs:404-637, 1868+` (`face_survival_detect`).

**Paper section:** Yang §4.4.2 (`yang2025:574-598`).

**Paper requirement:** "We directly apply a standard inside/outside classification step [Cherchi et al. 2022]." Cherchi 2022 uses per-patch ray-casting through manifold-edge barriers.

**Current implementation:** Flood-fill patch segmentation with manifold-edge-barrier walking. Comment at `topology_extract.rs:508-514` notes a "refactor from Yang's intersection-edge-barrier flood to Cherchi 2022 §5 manifold-edge-barrier flood" — but whether the refactor matches Cherchi exactly or is a variant is unclear.

**Deviation magnitude:** Structural pending clarification.

#### D8 — §4.5.1 boundary step rescaling via surface-switching

**Status:** OPEN. UNCERTAIN.

**Code location:** `crates/kernel/src/boolean/intersection_opt.rs:901-1006`.

**Paper section:** Yang §4.5.1 (`yang2025:626-638`, Fig 12).

**Paper requirement:** When a Newton step exits the current surface's domain, **rescale the step magnitude** to land on the boundary curve `C_b`; continue optimization on the adjacent surface from there.

**Current implementation:** Clamps the parameter update to domain bounds, then if the point exits, finds the adjacent face and switches surface parameterization entirely. Both prevent escaping the domain but the mechanics differ from Yang's step-rescaling.

**Deviation magnitude:** Structural; semantic equivalence uncertain.

#### D9 — §4.1.2 per-surface u-v CDT for adjacency handling

**Status:** OPEN. NOT signed off.

**Code location:** `crates/kernel/src/tessellation/mod.rs` — no §4.1.2 path.

**Paper section:** Yang §4.1.2 (`yang2025:397-410`).

**Paper requirement:** Discretize each surface patch independently in its own u-v domain; **re-sample boundary curves**; **apply CDT in each surface's parametric domain** to reconstruct triangulation around the boundaries. This is a per-surface CDT during the discretization step (separate from §4.4.1 CDT during mesh updating).

**Current implementation:** Per-surface discretization done via edge-first `discretize_edges` then per-face dispatch (analytic surfaces use surface-specific tessellation; planar uses Newell-derived 2D basis + CDT). No explicit per-surface u-v CDT for boundary re-sampling at discretization time. The §4.4.1 CDT is the only CDT in the pipeline.

**Deviation magnitude:** Structural. Yang's §4.1.2 step is folded into our §4.4.1-style path.

#### D10 — Tessellation density not fully `d_ε`-driven

**Status:** OPEN. NOT signed off.

**Code location:** `crates/kernel/src/tessellation/mod.rs:34-87`.

**Paper section:** Yang §4.1 (`yang2025:330-395`).

**Paper requirement:** Discretize each surface with iterative refinement until distance-to-surface < `d_ε`. Adaptive per surface, driven by `d_ε`.

**Current implementation:** Three LOD modes: `Boolean` (fixed 16 segments/circle), `Render` (fixed 64), `Adaptive { d_epsilon }` (sagitta-formula segment count for circular edges only). Production path uses fixed `Boolean` LOD for boolean stages and fixed `Render` LOD for output. Adaptive is invoked but constrained to circular-edge sagitta — not the full surface-iterative refinement Yang prescribes.

**Deviation magnitude:** Structural.

#### D11 — `d_ε` computed from combined-AABB across both solids

**Status:** OPEN. NOT signed off.

**Code location:** `crates/kernel/src/boolean/yang_integration.rs:642-661`.

**Paper section:** Yang §4.1 (`yang2025:378-382`).

**Paper requirement:** "We select `d_ε` as a value 10⁻² · d relative to the diagonal length d of the AABB of **the B-Rep model**" (singular).

**Current implementation:** `d_ε = 0.01 · diag` where diag is the AABB diagonal over BOTH solids' vertices combined. Single `d_ε` value used for both operand discretizations.

**Deviation magnitude:** Structural. May over-smooth small solids when paired with large ones.

### Performance — algorithm class differs

#### D12 — O(n²) broad-phase intersection detection vs octree

**Status:** OPEN. NOT signed off.

**Code location:** `crates/kernel/src/boolean/cherchi/intersection_class.rs:106-160`.

**Paper section:** Yang §4.2.1 (`yang2025:450-451`).

**Paper requirement:** "We use an **octree** to detect triangles that are closer than `2·d_ε`."

**Current implementation:** O(n²) pairwise loop over all triangle pairs with AABB culling per pair. Comment at line 107 acknowledges this: "Simple O(n²) broad phase with AABB culling + Gauss map filtering. For production, replace with BVH/octree."

**Deviation magnitude:** Structural for scale; functional behavior is correct for small inputs.

### Scope — we don't support what Yang supports

#### D13 — Gauss-map check uses triangle-normal dot product, not Theorem 4.1 cones

**Status:** OPEN. NOT signed off.

**Code location:** `crates/kernel/src/boolean/cherchi/intersection_class.rs:117-149`.

**Paper section:** Yang §4.2.2 Theorem 4.1 (`yang2025:457-480`).

**Paper requirement:** Construct circular cone C₃ around a₃ = (a₁ × a₂) / |a₁ × a₂| where a₁, a₂ are from the Bézier control net. The cone bounds the normal-vector field of the entire patch.

**Current implementation:** Per-triangle normal cross product + dot-product check. Valid for flat triangles; not the cone-construction Theorem 4.1 prescribes.

**Deviation magnitude:** Scope (Bézier-specific algorithm). Becomes a structural deviation when D14 (NURBS support) is closed.

#### D14 — No NURBS / Bézier surface support; only analytic primitives

**Status:** OPEN. NOT signed off.

**Code location:** `crates/kernel/src/geometry/surface.rs:56-62`, `crates/kernel/src/tessellation/analytic.rs` (entire file).

**Paper section:** Yang §4.1 + §4.1.1 NURBS-to-Bézier conversion (`yang2025:330-395`).

**Paper requirement:** Yang's whole pipeline takes NURBS B-Rep input, converts to rational Bézier sub-patches, and discretizes via recursive Bézier subdivision. Theorems A.1 and A.2 give the control-net distance bounds that make this work.

**Current implementation:** Five analytic surface types only (Planar, Cylindrical, Conical, Spherical, Toroidal). No NURBS, no Bézier subdivision. The tessellation strategy is geometry-specific dispatch per primitive type, not the unified Bézier algorithm Yang specifies.

**Deviation magnitude:** Scope. We are building Yang for analytic-primitive inputs only. The paper's algorithm scales to NURBS; our subset doesn't need to (yet) but should be acknowledged.

**Notes:** This is the largest scope gap, but the most defensible — analytic surfaces are a strict subset of NURBS, so an analytic-only kernel can be a faithful Yang implementation for that subset. The decision is whether to broaden scope to NURBS later (likely yes for real CAD use) or stay analytic-only.

---

## Priority order for remediation

1. **D2** (post-tessellation repair pipeline) — fundamental, blocks investigation of anything downstream. Removing this will surface what the upstream stages are actually doing.
2. **D1 Tier 3** + **D5** + **D9** — cross-face triangulation consistency (Newell drift, r_A=r_B, per-surface u-v CDT). All related; closing them together makes sense.
3. **D6** (Fig 11 procedures) — closes the §4.4.1 mesh-updating story.
4. **D7** (Cherchi vs flood-fill) — confirm or correct the patch-segmentation refactor.
5. **D3** (§4.5.4 illegal-intersection removal) — independent; can be added at any time.
6. **D4** (localized refinement) — depends on having a working failure-detection path first.
7. **D8** (boundary step rescaling) — minor semantic difference; defer unless it shows up empirically.
8. **D10, D11** (adaptive d_ε) — improvements to discretization fidelity.
9. **D12** (octree) — performance, not correctness.
10. **D13, D14** (NURBS / Bézier) — scope decision; defer until needed.

---

## Closed deviations (signed off; investigation may proceed)

*(none yet)*

---

## Resolved deviations (implementation brought into line with paper)

*(none yet)*

---

## How to add a new deviation entry

When you discover a divergence during work:

1. Add a new entry to "Open deviations" with: code location, paper section, current behavior, paper prescription, architectural consequence, remediation options.
2. Cite the relevant paper line numbers from `refs/text/*.txt`.
3. Flag in the current cycle's audit memo and mention at the TOP of your next message to the user.
4. Halt the affected investigation; do not continue accumulating probe data on the divergent implementation.

## How to sign off on a deviation (user)

Append to the deviation's entry:

```
**Sign-off:** approved by <name>, <date>, rationale: <text>. Tracking issue / future remediation: <link or note>.
```

This is a deliberate choice to accept the deviation — usually because the paper-aligned implementation is impractical or impossible in this codebase context. The sign-off documents the trade-off so future investigations don't waste cycles re-discovering the gap.

## How to mark a deviation resolved

When implementation is brought into line with the paper:

1. Move the entry from "Open deviations" to "Resolved deviations".
2. Append `**Resolved:** <date>, commit <sha>, description: <text>`.
3. Update CLAUDE.md "Known deviations" section if you summarized there.
