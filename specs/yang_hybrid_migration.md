# Yang 2025 Hybrid Boolean Pipeline — Migration Plan

**Reference**: [#24] Yang, Jia & Yan, "Boolean Operations on NURBS B-Rep Models
via a Hybrid Approach" (SIGGRAPH 2025). See REFERENCES.md for full citation.

**Governance**: ARCHITECTURAL_INVARIANTS.md A15.6.

**Goal**: Replace the S-H clipping + tolerance escalation boolean pipeline with
the Yang hybrid approach. Meshes as exact computational tool for B-Rep topology.

---

## Architecture Overview

```
Current (deprecated):
  B-Rep faces → S-H polygon clipping → classify face → collect fragments
    → stitch with 5000× tolerance → tessellate with synthetic repair
    → watertight but geometrically wrong mesh

Target (Yang 2025):
  B-Rep faces → tessellate with bijective mapping → exact mesh boolean
    → extract topology → refine to SSI curves → assemble B-Rep
    → tessellate clean B-Rep → correct mesh
```

---

## Phases

Each phase is independently valuable and testable. Phases produce working code
that can be committed and verified before proceeding. An autonomous agent should
be able to complete one phase (or one task within a phase) per session.

### Phase 1: Bijective Tessellation Mapping

**What**: Extend the tessellation layer so each output triangle carries a mapping
back to its source B-Rep face and parametric coordinates on that face.

**Why first**: This is pure infrastructure with no behavioral change. All existing
tests continue to pass. The mapping is the key enabler for topology extraction
(Phase 3).

**Tasks**:

- [x] **1a**: Add `BijectiveMap` struct: maps mesh triangle index → (face_id, [u,v] parametric coords for each vertex). File: `crates/kernel/src/tessellation/bijective.rs`
- [x] **1b**: Extend `tessellate_solid_bounded` to populate `BijectiveMap` alongside the existing `RenderMesh`. Each face's triangulation already knows its face origin — thread that through.
- [x] **1c**: Extend fan tessellation path similarly. Each `FacePoly` carries `surface_geom` and `origin` — map these to the `BijectiveMap`.
- [x] **1d**: Tests: verify bijective property (every triangle maps to exactly one face, every face's triangles tile it completely). Use existing box/cylinder/gear test cases.

**Acceptance**: `cargo test -p kernel` passes. `BijectiveMap` is populated for all tessellation paths. No behavioral change to existing boolean output.

---

### Phase 2: Exact Mesh Boolean (Core)

**What**: Implement triangle-triangle intersection and face classification using
exact predicates, producing a topologically correct result mesh.

**Why**: This is the core algorithm that replaces S-H clipping + tolerance
escalation. Uses Cherchi 2020 indirect predicates for exactness.

**Research basis**: [#9] Cherchi et al. 2020, [#4] Shewchuk 1997,
[#10] Levy 2025 (radial sort).

**Dependencies**: `robust` crate (already integrated), `geometry-predicates`
crate (add to Cargo.toml for expansion arithmetic primitives).

**Tasks**:

- [x] **2a**: Add `geometry-predicates` crate dependency. Verify WASM compilation.
- [x] **2b**: Implement triangle-triangle intersection using exact `orient3d`. Produce intersection segments as indirect points (symbolic references to input triangles, not materialized coordinates). File: `crates/kernel/src/boolean/exact_mesh.rs`
- [x] **2c**: Implement constrained triangulation of intersected faces — subdivide each input triangle along intersection segments. Each sub-triangle inherits the bijective mapping of its parent.
- [ ] **2d**: Implement cell labeling via generalized winding numbers. Each cell (region bounded by sub-triangles) gets a winding number vector. Boolean ops extract cells by winding number condition.
- [ ] **2e**: Implement radial sort ([#10] Levy) for non-manifold edge resolution — when multiple triangles meet at an intersection edge, sort them by angular position using exact predicates. This replaces tolerance-based edge pairing entirely.
- [ ] **2f**: Tests: exact mesh boolean on axis-aligned box pairs (union, subtract, intersect). Verify zero unpaired edges, correct Euler characteristic, correct volume sign. Compare against known-correct results.

**Acceptance**: Box-box boolean via exact mesh path produces provably correct topology. No tolerance parameters anywhere in the pipeline.

---

### Phase 3: Topology Extraction

**What**: From the exact mesh boolean result + bijective mappings, extract the
B-Rep topology: which original faces survive, how they are trimmed, which new
intersection edges are created.

**Tasks**:

- [ ] **3a**: Implement face survival detection: for each cell in the result, use bijective map to determine which original B-Rep face it came from. Group cells by source face.
- [ ] **3b**: Implement trim boundary extraction: for each surviving face, find the boundary edges that are intersection curves (not original face edges). These become trim curves on the original surface.
- [ ] **3c**: Implement connectivity extraction: build half-edge topology from the result mesh. Each edge in the result maps to either an original edge (surviving) or an intersection edge (new). Use Euler operators to construct the result B-Rep.
- [ ] **3d**: Tests: box-box subtract via full pipeline (tessellate → exact boolean → extract topology → build B-Rep). Verify result has correct face/edge/vertex counts. Verify `SurfaceGeom` types preserved on surviving faces.

**Acceptance**: Box-box subtract produces a valid half-edge B-Rep with correct topology and preserved surface types.

---

### Phase 4: SSI Refinement Integration

**What**: For intersection edges on curved surfaces, refine the mesh-derived
approximation to true SSI curves using existing quadric solvers.

**Tasks**:

- [ ] **4a**: Identify intersection edges that lie on curved surfaces (cylindrical, conical, spherical, toroidal) using the bijective map.
- [ ] **4b**: For each such edge, call the appropriate A15.4 SSI solver to compute the exact intersection curve. Replace mesh-derived edge geometry with the analytical curve.
- [ ] **4c**: For planar-planar intersections, the mesh result is already exact (intersection is a line). No refinement needed.
- [ ] **4d**: Tests: box-cylinder subtract via full pipeline. Verify the circular intersection edge is refined from mesh approximation to exact circle.

**Acceptance**: Mixed planar/curved booleans produce analytical intersection curves where solvers exist. `KernelError::NotSupported` for missing solvers (consistent with A15.2).

---

### Phase 5: Switchover and Deprecation Removal

**What**: Route boolean operations through the new pipeline. Remove the deprecated
S-H clipping + tolerance escalation code.

**Precondition**: Phases 1-4 pass all tests. Assay score with new pipeline ≥
current assay score (which is low — 0/10 R-series — so this bar is easy to clear).

**Tasks**:

- [ ] **5a**: Add feature flag `yang_boolean` (default on) to `WaffleKernel::do_boolean`. When enabled, route through the new pipeline. When disabled, use legacy S-H path.
- [ ] **5b**: Run full assay suite with new pipeline. Document pass/fail comparison.
- [ ] **5c**: Once new pipeline matches or exceeds legacy results: remove feature flag, make new pipeline the only path.
- [ ] **5d**: Delete deprecated code: `classify_face`, `classify_face_nonconvex`, `collect_fragments`, `build_brep_from_polygons` (S-H path), `stitch.rs` tolerance escalation (steps 3c/3d), tessellation repair convergence loops, `fill_boundary_holes`, `close_near_boundary_chains`, `remove_isolated_triangles`, `weld_boundary_vertices_with_scale`.
- [ ] **5e**: Remove tolerance constants that are only used by deprecated code: `TAU_CLASSIFY_FACTOR`, `TAU_SH_DIVERGENCE_FACTOR`, `STITCH_UNPAIRED_TOLERANT`, etc.
- [ ] **5f**: Update ARCHITECTURAL_INVARIANTS.md A15.6 to mark migration complete.

**Acceptance**: No S-H clipping or tolerance escalation code remains. Boolean operations use exact mesh boolean for topology. All assay tests that passed before still pass. Self-intersection oracle shows improvement.

---

## Task Sizing for Autonomous Agents

Each numbered task (1a, 1b, 2a, etc.) is designed to be completable in a single
agent session (~30-60 minutes). Tasks within a phase are sequential (each depends
on the previous). Phases are sequential (each depends on the previous).

**An agent picking up this work should**:
1. Read this spec
2. Find the first unchecked task
3. Implement it
4. Run `cargo test -p kernel && cargo clippy -p kernel`
5. Commit with message `feat(kernel): yang pipeline phase N task Na — description`
6. Check the box in this file
7. Stop (next agent picks up the next task)

**Do NOT attempt multiple tasks in one session** unless the task is trivially small.
Each task involves non-trivial design decisions that should be committed and
reviewed independently.

---

## Key Implementation Decisions

### Indirect Predicates vs. Exact Arithmetic

Use Cherchi's indirect predicate approach: intersection points stored as symbolic
references to input geometry, predicates evaluated without materializing coordinates.
The `geometry-predicates` crate provides expansion arithmetic primitives.

Do NOT use full exact rational arithmetic (e.g., `dashu`) for the core pipeline —
it's too slow for interactive CAD. Reserve exact rationals for validation/testing.

### Mesh Representation

Use existing `FacePoly` (verts + normal + origin + surface_geom) as the mesh
triangle representation. Extend with bijective mapping metadata. Do not introduce
a separate mesh data structure — reuse what exists.

### Integration Point

The new pipeline replaces `planar_planar_boolean` and `polygon_approx_boolean`
in `crates/kernel/src/boolean/`. The `ssi_boolean_op` (analytical SSI for
primitive pairs) continues to exist and feeds Phase 4 refinement.

---

## References

- [#24] Yang, Jia & Yan (2025) — Hybrid B-Rep/mesh boolean pipeline
- [#9] Cherchi et al. (2020) — Fast exact mesh arrangements, indirect predicates
- [#10] Levy (2025) — Exact constructions + radial sort
- [#4] Shewchuk (1997) — Adaptive precision predicates
- [#25] Yang, Jia & Yan (2023) — Topology-guaranteed SSI
- [#26] Yang & Jia (2025) — Overlap region extraction (coplanar faces)
