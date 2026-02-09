# 01 — Kernel Fork: Plan

## Milestones

### M1: Fork and Build Setup ✓
- [x] Fork truck repository
- [x] Set up as workspace dependency (crates.io path dependency)
- [x] Verify `cargo build` for truck-topology, truck-geometry, truck-modeling, truck-shapeops, truck-meshalgo
- [x] Create kernel-fork crate skeleton with Cargo.toml

### M2: Higher-Level Primitive API ✓
- [x] `make_box(width, height, depth) -> Solid`
- [x] `make_cylinder(radius, height) -> Solid`
- [x] `make_sphere(radius) -> Solid`
- [x] Unit tests for each primitive (vertex/edge/face counts, bounding box)

### M3: Kernel Trait Adapter ✓
- [x] Implement `TruckKernel` struct
- [x] `extrude_face()` via `builder::tsweep`
- [x] `revolve_face()` via `builder::rsweep`
- [x] `boolean_union()` via truck-shapeops `or`
- [x] `boolean_subtract()` via `solid.not()` + `and`
- [x] `boolean_intersect()` via truck-shapeops `and`
- [x] `make_faces_from_profiles()` — planar face construction from sketch profiles
- [ ] `fillet_edges()` — returns NotSupported (deferred to M8)
- [ ] `chamfer_edges()` — returns NotSupported (deferred to M9)
- [ ] `shell()` — returns NotSupported (deferred to M10)

### M4: KernelIntrospect Adapter ✓
- [x] `list_faces/edges/vertices()` via truck-topology iteration
- [x] `face_edges()`, `edge_faces()`, `edge_vertices()`
- [x] `face_neighbors()` — faces sharing an edge
- [x] `compute_signature()` — surface type, centroid, normal (area/bbox deferred)
- [x] `compute_all_signatures()` — batch computation

### M5: MockKernel ✓
- [x] Implement `MockKernel` with deterministic synthetic topology
- [x] Extrude rectangle → 8V, 12E, 6F with predictable signatures
- [ ] Extrude circle → predictable V/E/F counts (not yet implemented)
- [x] Boolean operations with predictable topology changes
- [x] Sequential ID assignment
- [x] Deterministic tessellation output
- [x] Comprehensive tests verifying MockKernel matches expected behavior (14 tests)

### M6: Tessellation Wrapper ✓
- [x] Wrap `MeshableShape::triangulation()` → `RenderMesh`
- [x] Face-range metadata computation (per-face triangle index ranges)
- [ ] `EdgeRenderData` extraction (sharp edge line segments) — type defined, extraction deferred
- [x] Tolerance parameter passthrough
- [x] Test: tessellate box, verify face_ranges cover all triangles

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

### truck API Learnings (discovered during M1–M6)

- `truck_modeling::*` exports a `Result<T>` type alias that shadows `std::result::Result`. Use selective imports.
- `Solid::not()` mutates in place (returns `()`), does NOT return a new solid.
- `Solid::boundaries()` returns `&Vec<Shell>`, not an iterator.
- `Shell::edge_iter()` and `Shell::vertex_iter()` yield duplicates — deduplicate with `HashSet` on `.id()`.
- `Face::boundaries()` returns `Vec<Wire>` (cloned).
- `builder::tsweep` of a `Vertex` returns an `Edge`, not a `Wire`.
- `Point3::origin()` requires `EuclideanSpace` trait in scope.
- `Vector3::magnitude()` / `normalize()` require `InnerSpace` trait in scope.
- Wire construction for closed profiles: must use `Edge::new(&v1, &v2, curve)` with shared vertex references. Creating edges via `builder::tsweep` produces disconnected vertices and `try_attach_plane` fails with "wire is not closed".
- `truck_shapeops::or()` / `and()` take `(&Solid, &Solid, tolerance)` and return `Option<Solid>`.
- Cylinder full 2π rsweep produces 5 faces (not 3) due to internal subdivisions.
- Tessellation: `MeshableShape::triangulation(tol)` on Solid returns meshed solid with `Option<PolygonMesh>` per face surface. `PolygonMesh::tri_faces()` yields `[StandardVertex; 3]` where `StandardVertex.pos: usize`.
