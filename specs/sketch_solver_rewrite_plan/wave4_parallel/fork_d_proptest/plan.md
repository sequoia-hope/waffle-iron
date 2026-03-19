# Wave 4 / Fork D: Property-Based Testing

**Executor**: Claude fork (worktree), with Gemini workers
**Depends on**: Wave 3 (working solver)
**Parallel with**: Fork E (LM fallback), Fork F (JS elimination)
**Estimated scope**: ~600 lines of test code

## Goal

Build a comprehensive proptest suite that exercises the solver with randomly
generated sketches, verifying structural invariants hold for all inputs.

## Worker Breakdown

### Worker D1: Proptest strategies (`tests/proptest_strategies.rs`)

Reusable strategies for generating sketch components:

```rust
/// Random point in [-100, 100]²
fn arb_point() -> impl Strategy<Value = (f64, f64)>

/// Random sketch with N points connected by lines (polygon)
fn arb_polygon(n: usize) -> impl Strategy<Value = (Vec<SketchEntity>, Vec<(f64, f64)>)>

/// Random fully-constrained rectangle (4 points, 4 lines, H/V + distance constraints)
fn arb_constrained_rectangle() -> impl Strategy<Value = Sketch>

/// Random triangle with all distances constrained
fn arb_constrained_triangle() -> impl Strategy<Value = Sketch>
```

### Worker D2: "Seed and measure" property tests

The core strategy from the spec's testing section:

**Test: `proptest_seed_and_measure_polygon`**
1. Generate N random points (3..=8)
2. Connect with lines to form a closed polygon
3. Measure all edge lengths from seed positions
4. Add Distance constraints with measured values
5. Pin first point (Coincident to origin) + Horizontal on first edge
6. Perturb all point positions by random offsets (±10% of sketch size)
7. Solve
8. Assert: solved positions match seed positions within 1e-5
9. Assert: status is FullyConstrained or UnderConstrained with expected DOF

**Test: `proptest_seed_and_measure_rectangle`**
1. Generate random width/height in [0.01, 10.0]
2. Create 4 points + 4 lines
3. Add H/V constraints + Distance constraints from measured dimensions
4. Pin origin point
5. Perturb, solve, verify

### Worker D3: Structural invariant tests

**Test: `proptest_residuals_near_zero`**
- For any solvable sketch, after solve, all constraint residuals < TAU_MODEL

**Test: `proptest_dof_conservation`**
- structural DOF = 2 * num_points + num_circles - total_constraint_equations
- Verify this holds before and after solve

**Test: `proptest_idempotent_solve`**
- Solve a sketch, take the solved positions, re-solve → same result

**Test: `proptest_adding_constraint_reduces_dof`**
- Solve with N constraints → DOF = d
- Add one non-redundant constraint → DOF = d - (equations added)

**Test: `proptest_fully_constrained_unique_solution`**
- For a fully constrained sketch, solve from 10 different random perturbations
- All should converge to the same solution (within tolerance)

### Worker D4: Edge case and adversarial tests

Per spec §Numerical Stability Tests:

**Test: `proptest_near_parallel_angle`**
- Two nearly-parallel lines (angle < 0.01 rad) with angle constraint
- Solver should converge (possibly slowly) or report meaningful failure

**Test: `proptest_scale_invariance`**
- Same sketch at 1e-3, 1e0, 1e3 scale → same DOF, same convergence

**Test: `proptest_zero_length_line`**
- Line with coincident endpoints + length constraint → should handle gracefully

### Worker D5: Visual proptest integration

If `render` feature is enabled:
- For each proptest case, render SVG
- Assert SVG structural correctness (element counts match entity counts)
- Optionally write failing cases to `test-output/` for human review

## Deliverables

- `tests/proptest_strategies.rs`: reusable strategies
- `tests/proptest_solve.rs`: seed-and-measure tests
- `tests/proptest_invariants.rs`: structural property tests
- `tests/proptest_adversarial.rs`: edge case tests

## Verification

- `cargo test -p sketch-solver -- proptest` — all pass with 256+ cases each
- `PROPTEST_CASES=1000 cargo test -p sketch-solver -- proptest` — stress test
