# 06 — Feature Engine: Agent Instructions

You are working on `crates/feature-engine`. Read ARCHITECTURE.md in this
directory first, especially "Persistent references".

## Rules

1. **Reference resolution is the core algorithm.** Test it by building a tree,
   changing an early feature, and checking that every downstream reference
   still resolves to the entity it named.
2. **Nothing rebinds silently.** A `Strict` reference that no longer
   identifies one entity is a typed refusal. A `BestEffort` rebind carries a
   warning and a `ResolvedVia` that says it rebound.
3. **Prefer `Selector::Pid`** for references that must stay on one entity.
   Pin at authoring time with `pin_identity`.
4. **Keep rebuilds incremental.** A feature that did not change and whose
   inputs did not re-execute keeps its result. State a re-executed feature
   needs from the last rebuild goes in `Carried`.
5. **A consumed body is not live.** Never pick it as a combine target or
   render it.
6. **Correct before fast.**
7. **Fillet, chamfer and shell are deferred indefinitely.** Do not work on
   them.

## Build & Test

```bash
cargo test -p feature-engine
cargo clippy --all-targets -p feature-engine -- -D warnings
cargo fmt --check -p feature-engine
```

Unit tests use `waffle_types::kernel::MockKernel` (feature `mock-kernel`).
Real-geometry tests run on kernel-v2 and live in `crates/test-harness`.

## Key Files

- `src/lib.rs` — `Engine` and its public API
- `src/types.rs` — `Feature`, `FeatureTree`, `Operation`, errors, parameters
- `src/tree.rs` — tree mutations
- `src/rebuild.rs`, `src/rebuild/` — rebuild, combine targets, depth and
  plane resolution
- `src/resolve.rs` — reference resolution
- `src/undo.rs` — commands and the undo stack
- `src/params.rs`, `src/expr/` — parameters and expressions

## Dependencies

`modeling-ops`, `sketch-solver`, `step-import`, `kicad-pcb`, `waffle-types`,
`rhai`, `nalgebra`.
