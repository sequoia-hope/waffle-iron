# 02 — Sketch Solver: Agent Instructions

You are working on `crates/sketch-solver`. Read ARCHITECTURE.md in this
directory first.

## Build & Test

```bash
cargo test -p sketch-solver
cargo clippy --all-targets -p sketch-solver -- -D warnings
cargo fmt --check -p sketch-solver
```

The crate is in CI's lint and test jobs and ships in the WASM bundle, so it
must not call `Instant::now()` or anything else unavailable on
`wasm32-unknown-unknown`.

## Rules

1. **Every `CompiledConstraint` variant has an analytic-vs-numeric Jacobian
   test.** A wrong derivative is invisible to solve-level tests because LM's
   damping converges through it. Add the check with the variant.
2. **Keep the solve path deterministic.** Assemble in declaration order; use
   `HashMap` for id lookup only, never iterate one.
3. **Report indices in the caller's constraint space.** Conflicts, residuals
   and redundancy index `sketch.constraints`, not the filtered driving list
   and not residual rows.
4. **A failed solve is inert.** `SolveFailed` returns the input positions.
5. **Sketch operations never fail silently.** Return a typed `SketchOpError`.
6. **Test against sketches with analytic answers.** Units are meters.

## Dependencies

`waffle-types` only. Consumers are `feature-engine`, `wasm-bridge` and
`test-harness`.
