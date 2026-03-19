# Wave 3: Integration

**Executor**: Opus (sequential, architectural)
**Depends on**: Wave 2 Fork A + Fork B merged
**Blocks**: Wave 4 (all forks)
**Estimated scope**: ~150 lines changed, ~400 lines deleted

## Goal

Wire the new solver core into the existing `solve_sketch()` API and pass all
59 oracle tests. Then delete the slvs-specific code.

## Steps

### 3.1 Rewrite `solver.rs`

Replace the slvs-based implementation with:

```rust
use crate::core::{ParamLayout, build_constraints, lm_solve, analyze_rank, classify_solve};
use crate::profiles::extract_profiles;

pub fn solve_sketch(sketch: &Sketch) -> SolvedSketch {
    // Expand gears before solving (existing pattern)
    let expanded = sketch.expand_gears();
    let entities = &expanded.entities;
    let constraints = &expanded.constraints;

    // Build parameter layout from entities
    let layout = ParamLayout::from_entities(entities);
    let x0 = layout.initial_params(entities);

    // Build constraint equations with scale type annotations
    let constraint_impls = build_constraints(constraints, entities, &layout);
    let scale_types = build_scale_types(&constraint_impls);
    let num_equations: usize = constraint_impls.iter().map(|c| c.num_equations()).sum();

    // Solve with LM (x0 serves as both starting guess and spring anchor
    // for initial solve — they diverge only during drag operations)
    let options = SolveOptions::default(); // TAU_MODEL, 50 iters, λ=1e-3, μ=1e-6
    let outcome = lm_solve(&x0, &x0, &constraint_impls, &scale_types, num_equations, &options);

    // Rank analysis on SCALED, UN-AUGMENTED Jacobian (from SolveOutcome)
    let eq_to_constraint = build_eq_to_constraint_map(&constraint_impls);
    let rank = analyze_rank(
        &outcome.jacobian_scaled,
        &outcome.residual_scaled,
        layout.num_params(),
        num_equations,
        &eq_to_constraint,
    );
    let status = classify_solve(&outcome, &rank, layout.num_params());

    // Extract positions
    let positions = layout.extract_positions(&outcome.params);

    // Extract profiles (existing algorithm, unchanged)
    let profiles = if matches!(status, SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }) {
        extract_profiles(entities, &positions)
    } else {
        Vec::new()
    };

    SolvedSketch { positions, profiles, status }
}
```

**Drag operation protocol (from R4):**

When the UI sends a `Dragged { point }` constraint, the solver must:
1. Use the point's current position (from the sketch entities) as the drag target
2. `x0` = previous solved positions (warm start for fast convergence)
3. `x_anchor` = positions captured at mouse-down (prevents null-space drift)
4. The `Dragged` constraint creates a hard anchor: `x_p - tx = 0, y_p - ty = 0`
5. Weak springs on all OTHER params pull toward `x_anchor`

For the initial Wave 3 implementation, `x0 == x_anchor == initial_params` (no
distinction needed until drag is wired through the UI with frame-by-frame state).
The `x_anchor` parameter exists in `lm_solve` to support drag in the future.

### 3.2 Delete slvs-specific modules

- Delete `src/constraint_mapping.rs`
- Delete `src/entity_mapping.rs`
- Remove `slvs = "0.6"` from Cargo.toml
- Update `src/lib.rs` to remove old module declarations, add `pub mod core;`

### 3.3 Run oracle test suite

```
cargo test -p sketch-solver
```

All 59 tests must pass. If any fail:

1. **Do not modify the test.** The test is the oracle.
2. Investigate: is the constraint implementation wrong? Is the solver not
   converging? Is the status classification off?
3. Common issues to watch for:
   - Sign conventions (SymmetricH/V — the spec's equations differ from
     the current slvs mapping's behavior)
   - Radius vs diameter (slvs uses diameter internally — our solver uses
     radius directly, but the existing tests may encode the old behavior)
   - DOF counts may differ slightly if our rank analysis is more/less
     accurate than slvs

### 3.4 Tolerance matching

If a test checks positions within a tolerance, our solver should match or
beat slvs. The spec requires TAU_MODEL = 1e-7. Existing tests use various
tolerances (check `assert_point_near` calls). Our solver should converge
tighter than these test tolerances.

### 3.5 Bridge test verification

```
cargo test -p wasm-bridge
```

The bridge tests that use `native-solver` feature must still pass since
sketch-solver's public API is unchanged.

## Deliverables

- Rewritten `src/solver.rs`
- Updated `src/lib.rs`
- Updated `Cargo.toml` (slvs removed)
- Deleted `src/constraint_mapping.rs`
- Deleted `src/entity_mapping.rs`

## Verification

- `cargo test -p sketch-solver` — all 59 tests pass
- `cargo test -p wasm-bridge` — bridge tests pass
- `cargo clippy -p sketch-solver` — no warnings
- No remaining `use slvs::` imports anywhere in sketch-solver
