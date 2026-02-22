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
- [x] `fillet_edges()` — TruckKernel returns NotSupported (see M8 notes)
- [x] `chamfer_edges()` — TruckKernel returns NotSupported (see M9 notes)
- [x] `shell()` — TruckKernel returns NotSupported (see M10 notes)

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
- [x] MockKernel fillet: deterministic topology (V+2n, E+n, F+n per edge)
- [x] MockKernel chamfer: deterministic topology (same as fillet, planar face)
- [x] MockKernel shell: removes specified faces, adds offset inner faces

### M6: Tessellation Wrapper ✓
- [x] Wrap `MeshableShape::triangulation()` → `RenderMesh`
- [x] Face-range metadata computation (per-face triangle index ranges)
- [x] `EdgeRenderData` extraction via `extract_edges()` — samples edge curves into polylines
- [x] Test: box produces 12 edge ranges, cylinder produces 3+ edge ranges
- [x] Tolerance parameter passthrough
- [x] Test: tessellate box, verify face_ranges cover all triangles

### M7: Boolean Performance Investigation ✅
- [x] Benchmark box-cylinder boolean at various tolerances
- [x] Benchmark box-box booleans (coplanar and offset)
- [x] Document findings (see Performance Findings below)
- [x] Explore tolerance tuning strategies
- [ ] Profile hotspots (deferred: truck internals are opaque)
- [ ] Investigate alternative boolean algorithms (deferred: would require kernel replacement)

### M8: Fillet — Experimental, DEFERRED INDEFINITELY
- [x] MockKernel fillet_edges: deterministic topology modification
- [x] Test: fillet single edge on box → 7F, 13E, 10V
- [x] Test: fillet multiple edges (3) → 9F, 15E, 14V
- [x] Test: invalid radius → FilletFailed error
- [x] Test: invalid edge → FilletFailed error
- [ ] TruckKernel fillet: deferred (see Architectural Blockers)

### M9: Chamfer — Experimental, DEFERRED INDEFINITELY
- [x] MockKernel chamfer_edges: deterministic topology modification
- [x] Test: chamfer single edge → 7F, 13E (planar chamfer face)
- [x] Test: invalid distance → error
- [ ] TruckKernel chamfer: deferred (see Architectural Blockers)

### M10: Shell — Experimental, DEFERRED INDEFINITELY
- [x] MockKernel shell: removes faces, adds offset inner faces
- [x] Test: shell box removing 1 face → 10 faces (5 outer + 5 inner)
- [x] Test: invalid thickness → ShellFailed error
- [x] Test: invalid face → ShellFailed error
- [ ] TruckKernel shell: deferred (see Architectural Blockers)

### M11: STEP Export Investigation ✅
- [x] Added truck-stepio dependency
- [x] Investigate STEP export of simple primitives (box, cylinder): works
- [x] Investigate STEP export of boolean results: works for box-box union
- [x] Document ruststep limitations (see STEP Export Findings below)
- [x] Test: export box → valid STEP with MANIFOLD_SOLID_BREP
- [x] Test: export cylinder → valid STEP with revolved geometry

## High Priority Backlog

### HP-1: Auto-union fails for 3+ chained abutting extrudes
- **Test case**: `app/tests/cases/several-extrudes.waffle`
- **Symptom**: First two abutting box extrudes (x=0..10, x=10..20) merge into 1 body correctly. Third extrude (x=20..30) produces 2 bodies. Fourth (circle, x=30..40) produces 3 bodies. Each subsequent merge=true boss becomes a separate body instead of unioning.
- **Impact**: Cuts then only affect one body fragment, not the whole model. A cut from x=0 with depth=100 should slice through the entire unified solid but instead only hits one piece, leaving 4+ bodies.
- **Reproduced**: `crates/test-harness/tests/saved_test_cases.rs` — `several_extrudes_replay` and `several_extrudes_simplified_abutting`

### HP-2: Cut operation splits previously-unioned body into fragments
- **Test case**: `app/tests/cases/multi-cut.waffle`
- **Symptom**: Two merge=true bosses (rect box x=0..10 + large circle x=10..20) successfully auto-union into 1 body. But a subsequent cut (circle, depth=20) produces 2 bodies instead of 1. The cut appears to "un-merge" the unified body — it only subtracts from the second boss's geometry, leaving the first boss as a separate body.
- **Impact**: Through-cuts that should affect the entire model only affect part of it. The boolean subtract is fragmenting the shell rather than cleanly subtracting.
- **Reproduced**: `crates/test-harness/tests/saved_test_cases.rs` — `multi_cut_replay` and `multi_cut_simplified`

## Blockers

### Priority Note

**Fillet, chamfer, and shell are DEFERRED INDEFINITELY.** MockKernel implementations exist for testing, and experimental TruckKernel implementations were added in Sprint 18, but these are not production-ready. Do not prioritize work on these operations. Focus on boolean reliability, integration tests, and extrude/revolve pipeline polish instead.

### Architectural Blockers for TruckKernel Fillet/Chamfer/Shell

1. **No entity ID mapping**: TruckKernel doesn't maintain a mapping from KernelId to truck topology entities. KernelIds are allocated during tessellation and introspection (positional scheme: `handle_id * 10000 + offset`), but there's no reverse mapping. Fillet/chamfer/shell need to identify specific truck edges/faces from KernelIds.

2. **Truck lacks fillet/chamfer/shell APIs**: truck-stepio lib.rs explicitly states fillets are absent. These operations would require manual BREP topology reconstruction: computing intersection curves, trimming surfaces, stitching new faces — essentially implementing core CAD kernel functionality from scratch.

3. **No face trimming API**: truck has no built-in way to trim a face boundary. Implementing fillet requires computing new wire boundaries where the fillet surface intersects adjacent planar faces, then constructing new Face objects with those wires.

4. **Boolean unreliability blocks chamfer**: Chamfer via boolean subtraction of a wedge tool body depends on reliable booleans. Benchmarks show booleans fail for coplanar faces and box-cylinder operations.

**Resolution**: MockKernel implementations enable other teams (feature-engine, modeling-ops) to test against deterministic fillet/chamfer/shell behavior. TruckKernel implementations require a significant refactor to add entity ID mapping + manual topology construction. truck is our chosen kernel — future work should focus on improving our vendored fork rather than replacing it.

## Interface Change Requests

(None yet)

## Performance Findings (M7)

### Boolean Benchmark Results

**Box-Cylinder (Box 2x2x2, Cylinder r=0.5 h=3):**
| Tolerance | Union | Subtract | Intersect |
|-----------|-------|----------|-----------|
| 0.05 | PANIC ("wire not simple") ~150ms | PANIC ~150ms | PANIC ~150ms |
| 0.10 | FAILED (None) ~80ms | FAILED (None) ~85ms | FAILED (None) ~85ms |
| 0.20 | FAILED (None) ~75ms | FAILED (None) ~76ms | FAILED (None) ~74ms |
| 0.50 | PANIC ~87ms | PANIC ~87ms | PANIC ~87ms |

**All box-cylinder boolean operations fail regardless of tolerance.** This is a critical limitation. Panic mode: "This wire is not simple" from truck-topology.

**Box-Box (Box 2x2x2, Box 1x1x1):**
| Configuration | Union | Subtract | Intersect |
|---------------|-------|----------|-----------|
| Coplanar (origin-aligned) | FAILED (None) ~2ms | FAILED (None) ~1.7ms | FAILED (None) ~1.6ms |
| Offset (0.5,0.5,0.5) | OK ~1.1ms | OK ~1.2ms | OK ~1.1ms |

**Box-box with no coplanar faces: ~1ms, all operations succeed.**
**Box-box with coplanar faces: all operations fail (None).**

### Key Findings

1. **Planar-only booleans work** when faces aren't coplanar. Performance is fast (~1ms).
2. **Mixed geometry (planar+cylindrical) booleans always fail** in truck 0.4.
3. **Coplanar faces are a hard failure mode** — not just slow, but produce no result.
4. **Tolerance has no positive effect** on box-cylinder ops — lower tolerances panic, higher tolerances still fail.
5. **Performance is not the primary issue** — operations complete in <200ms, but produce invalid or no results.

### Implications

- The boolean "performance crisis" documented earlier is actually a **correctness crisis** in truck 0.4.
- Workaround for users: avoid operations that create coplanar faces (offset tool bodies from surfaces).
- Long-term: truck may fix these issues in future versions, or we may need to integrate a different boolean library.

## STEP Export Findings (M11)

### Working Cases
- **Simple box**: Exports valid STEP with MANIFOLD_SOLID_BREP, FACE_SURFACE, etc.
- **Cylinder (revolved geometry)**: Exports valid STEP.
- **Box-box boolean union (offset, no coplanar)**: Exports valid STEP (7071 chars).

### Limitations
- **AP203 only**: No colors, layers, annotations, or assemblies.
- **truck-stepio docs warn**: "Shapes created by set operations cannot be output yet." However, our testing shows box-box unions do export. The limitation may apply to operations producing NURBS intersection curves (e.g., planar-cylindrical intersections).
- **Cannot test box-cylinder export** because the boolean operation itself fails.
- **ruststep is experimental**: README warns "DO NOT USE FOR PRODUCT."

### Workaround Strategy
For production STEP export:
1. Rebuild model from feature tree to get final solid.
2. If boolean results export, use directly.
3. If export fails, document the failure for the user.
4. Future: AP214/AP242 support as ruststep improves.

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
- `Solid::compress()` always succeeds (topology only). STEP export failure is in the `DisplayByStep` formatting step.
- truck-stepio v0.3 can export planar+revolved geometry solids. Box-box boolean results export successfully.
