# 01 — Kernel Fork: Plan

## Milestones

### M1: Fork and Build Setup
- [ ] Fork truck repository
- [ ] Set up as workspace dependency (git subtree or path dependency)
- [ ] Verify `cargo build` for truck-topology, truck-geometry, truck-modeling, truck-shapeops, truck-meshalgo
- [ ] Create kernel-fork crate skeleton with Cargo.toml

### M2: Higher-Level Primitive API
- [ ] `make_box(width, height, depth) -> KernelSolidHandle`
- [ ] `make_cylinder(radius, height) -> KernelSolidHandle`
- [ ] `make_sphere(radius) -> KernelSolidHandle`
- [ ] Unit tests for each primitive (vertex/edge/face counts, bounding box)

### M3: Kernel Trait Adapter
- [ ] Implement `TruckKernel` struct
- [ ] `extrude_face()` via `builder::tsweep`
- [ ] `revolve_face()` via `builder::rsweep`
- [ ] `boolean_union()` via truck-shapeops `or`
- [ ] `boolean_subtract()` via `solid.not()` + `and`
- [ ] `boolean_intersect()` via truck-shapeops `and`
- [ ] `make_faces_from_profiles()` — planar face construction from sketch profiles

### M4: KernelIntrospect Adapter
- [ ] `list_faces/edges/vertices()` via truck-topology iteration
- [ ] `face_edges()`, `edge_faces()`, `edge_vertices()`
- [ ] `face_neighbors()` — faces sharing an edge
- [ ] `compute_signature()` — surface type, area, centroid, normal, bbox, adjacency hash
- [ ] `compute_all_signatures()` — batch computation

### M5: MockKernel
- [ ] Implement `MockKernel` with deterministic synthetic topology
- [ ] Extrude rectangle → 8V, 12E, 6F with predictable signatures
- [ ] Extrude circle → predictable V/E/F counts
- [ ] Boolean operations with predictable topology changes
- [ ] Sequential ID assignment
- [ ] Deterministic tessellation output
- [ ] Comprehensive tests verifying MockKernel matches expected behavior

### M6: Tessellation Wrapper
- [ ] Wrap `MeshableShape::triangulation()` → `RenderMesh`
- [ ] Face-range metadata computation (per-face triangle index ranges)
- [ ] `EdgeRenderData` extraction (sharp edge line segments)
- [ ] Tolerance parameter passthrough
- [ ] Test: tessellate box, verify face_ranges cover all triangles

### M7: Boolean Performance Investigation
- [ ] Benchmark cube-cylinder boolean at various tolerances
- [ ] Profile hotspots (surface-surface intersection)
- [ ] Document findings
- [ ] Explore tolerance tuning strategies
- [ ] Investigate alternative boolean algorithms

### M8: Fillet (Constant Radius, Planar Faces)
- [ ] Identify edge between two planar faces
- [ ] Compute fillet surface (cylindrical for 90-degree edges)
- [ ] Trim adjacent faces
- [ ] Reconstruct topology
- [ ] Test on box edges

### M9: Chamfer
- [ ] Compute chamfer cut plane along edge
- [ ] Construct tool body (wedge)
- [ ] Boolean subtract from solid
- [ ] Test on box edges

### M10: Shell
- [ ] Offset faces inward by thickness
- [ ] Subtract offset solid from original
- [ ] Test on box with one face removed

### M11: STEP Export Fix
- [ ] Investigate why boolean-result solids fail to export
- [ ] Document ruststep limitations
- [ ] Attempt workarounds (reconstruct solid before export)

## Blockers

(None yet)

## Interface Change Requests

(None yet)

## Notes

- Boolean performance is the biggest risk. Track benchmarks over time.
- MockKernel is a critical deliverable — other teams are blocked without it.
- Always document truck bugs/limitations when encountered.
