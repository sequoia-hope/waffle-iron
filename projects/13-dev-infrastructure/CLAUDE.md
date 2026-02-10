# Sub-project 13: Agent Instructions

## Quick Start

```rust
use test_harness::ModelBuilder;

let mut m = ModelBuilder::mock();
m.rect_sketch("sk", [0.,0.,0.], [0.,0.,1.], 0., 0., 10., 10.).unwrap();
m.extrude("box", "sk", 10.0).unwrap();
m.assert_has_solid("box").unwrap();

let report = m.report().unwrap();
println!("{}", report.to_text());
```

## Adding New Test Scenarios

1. Create a test in `tests/scenarios_mock.rs` (or `scenarios_truck.rs`)
2. Use `ModelBuilder::mock()` for deterministic tests, `ModelBuilder::truck()` for real geometry
3. Name every feature — this makes failures readable
4. Call `m.assert_has_solid("name")` after operations that should produce solids
5. Use `m.report()` to generate a full diagnostic report
6. Run: `cargo test -p test-harness`

## Oracle Usage

```rust
// Individual checks
let verdicts = m.check_mesh("box").unwrap();  // Mesh quality oracles
let verdicts = m.check_topology("box").unwrap();  // V/E/F oracles

// Or get everything in one report
let report = m.report().unwrap();
for v in &report.oracle_results {
    println!("[{}] {}: {}", if v.passed {"PASS"} else {"FAIL"}, v.oracle_name, v.detail);
}
```

## Report Interpretation

The report has these sections:
- **Feature Tree**: name, type, parameters, topology (V/E/F), Euler check, roles
- **Mesh Summary**: triangle/vertex/face-range counts per feature
- **Bounding Box**: overall min/max coordinates
- **Oracle Results**: [PASS]/[FAIL] for each check with detail
- **Errors**: any engine rebuild errors

## Important Patterns

- Use `m.begin_sketch()` + `m.add_point()` + `m.add_line()` + `m.finish_sketch_manual()` for custom shapes
- Use `edge_ref_best_effort(feature_id)` for fillet/chamfer targets (MockKernel re-IDs entities)
- Use `body_ref(feature_id)` for boolean body references
- MockKernel box: V=8 E=12 F=6, mesh: 12 triangles, 24 vertices (per-face), 6 face ranges
- TruckKernel: fillet/chamfer/shell return NotSupported; coplanar booleans fail

## Running Tests

```bash
cargo test -p test-harness                    # All tests
cargo test -p test-harness test_full_workflow -- --nocapture  # See report output
cargo test -p test-harness --test scenarios_truck -- --ignored  # Run ignored truck tests
```
