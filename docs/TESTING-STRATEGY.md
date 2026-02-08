# Waffle Iron — Testing Strategy

## Test Pyramid

```
         ╱╲
        ╱  ╲        Integration Tests
       ╱    ╲       (full pipeline, real kernel)
      ╱──────╲
     ╱        ╲     Property-Based Tests
    ╱          ╲    (invariant checking)
   ╱────────────╲
  ╱              ╲  Unit Tests
 ╱                ╲ (per-crate, MockKernel, fast)
╱──────────────────╲
```

## 1. Unit Tests

**Scope:** Per-crate, fast (<10s per crate), mock all dependencies.

**Tools:** `#[test]`, MockKernel, hand-written mocks.

**Examples:**
- kernel-fork: MockKernel returns deterministic topology → verify face/edge/vertex counts.
- sketch-solver: solve a rectangle sketch → verify positions match analytically.
- modeling-ops: extrude via MockKernel → verify OpResult has correct roles and provenance.
- feature-engine: build feature tree → resolve GeomRefs → verify correct KernelId.
- file-format: serialize FeatureTree → deserialize → compare.

**Rules:**
- Every public function has at least one test.
- Tests are deterministic: no random values, no system time, no filesystem side effects.
- Tests use MockKernel, not TruckKernel (for speed and determinism).
- Tests run in parallel safely (no shared mutable state).

## 2. Property-Based Tests

**Scope:** Verify geometric and topological invariants that must always hold.

**Tools:** `proptest` or `quickcheck` crate.

**Properties to verify:**

### Euler's Formula (V - E + F = 2 for genus-0 solids)
After any operation producing a genus-0 solid (box, cylinder, extrude, fillet):
```rust
let v = introspect.list_vertices(&solid).len();
let e = introspect.list_edges(&solid).len();
let f = introspect.list_faces(&solid).len();
assert_eq!(v - e + f, 2);
```

### Watertightness
Every edge is shared by exactly 2 faces (no dangling edges, no gaps):
```rust
for edge in introspect.list_edges(&solid) {
    let faces = introspect.edge_faces(edge);
    assert_eq!(faces.len(), 2);
}
```

### Manifoldness
Every vertex has a consistent fan of edges/faces (no non-manifold topology).

### Normal Consistency
All face normals point outward (consistent orientation).

### Tessellation Validity
- No degenerate triangles (zero area).
- All normals are unit length.
- face_ranges cover all triangles (no gaps, no overlaps).
- Indices are within bounds.

### Provenance Completeness
After any operation:
- Every entity in the result appears in provenance (created or modified).
- No entity appears in both created and deleted.
- Role assignments reference only entities that exist in the result.

## 3. Integration Tests

**Scope:** Full pipeline tests using real truck kernel.

**Flow:** sketch → solve → extrude → tessellate → verify mesh.

**Examples:**
- Create a rectangle sketch, solve, extrude, tessellate → verify mesh has 12 triangles (2 per face × 6 faces for a box).
- Create sketch → extrude → fillet → tessellate → verify mesh is valid.
- Create feature tree → save to JSON → load → rebuild → compare topology.
- Send UiToEngine messages → verify EngineToUi responses.

**Performance:**
- Integration tests are slower (real kernel operations).
- Run them separately: `cargo test --test integration`.
- Mark slow tests with `#[ignore]` for CI, run explicitly.

## 4. Regression Tests

**Scope:** Deterministic snapshots of known-good outputs.

**Approach:**
- For key scenarios, store JSON snapshots of:
  - Topology (face/edge/vertex counts and signatures)
  - Tessellation (vertex/index counts, bounding box)
  - Provenance (role assignments, created/deleted/modified counts)
- On each test run, compare current output to snapshot.
- If different → test fails. Developer reviews and either fixes the code or updates the snapshot.

**Storage:** Snapshots stored as `.json` files in `tests/snapshots/`.

**Tool:** `insta` crate for snapshot testing.

## 5. Performance Benchmarks

**Scope:** Track performance over time. Not blocking CI.

**Metrics:**
- Tessellation time for standard shapes (box, cylinder, complex solid).
- Sketch solver time for 10, 50, 100, 300 constraints.
- Rebuild time for 10, 20, 50-feature trees.
- Boolean operation time (the known crisis).
- Mesh transfer time across WASM bridge.

**Tools:** `criterion` crate for Rust benchmarks.

**Policy:**
- Benchmarks are tracked but NOT gates for CI.
- Performance regressions are investigated but don't block merges.
- Boolean performance is monitored especially closely.

## 6. Test Efficiency Guidelines

### MockKernel for Unit Tests
Use MockKernel (from kernel-fork) for all unit tests in modeling-ops, feature-engine, and file-format. MockKernel is:
- **Fast:** No real geometry computation.
- **Deterministic:** Same inputs always produce same IDs and signatures.
- **Predictable:** Documented behavior (extrude rectangle → 8V, 12E, 6F).

### Real Kernel for Integration Only
TruckKernel is used only in integration tests. These tests verify that the real kernel behaves as MockKernel predicts.

### No Random Fuzzing in CI
Property-based tests use fixed seeds in CI for reproducibility. Random exploration happens locally.

### Test Data
Standard test sketches and feature trees stored in `tests/fixtures/`:
- `rectangle_100x50.json` — basic rectangle sketch
- `circle_r25.json` — basic circle sketch
- `box_100x50x25.json` — feature tree: sketch + extrude
- `filleted_box.json` — feature tree: sketch + extrude + fillet
