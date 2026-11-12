# 07 — Modeling Ops: Agent Instructions

You are working on `crates/modeling-ops`. Read ARCHITECTURE.md in this
directory first.

## Rules

1. **Every operation returns provenance.** `feature-engine` resolves
   references from it.
2. **Assign roles to created faces.** Roles are how downstream features name
   geometry.
3. **Operations are stateless.** Everything arrives as an argument.
4. **Use only the kernel traits.** No kernel-v2 types here.
5. **Validate parameters before calling the kernel** and return
   `OpError::InvalidParameter`.
6. **Fillet, chamfer and shell are deferred indefinitely.** Do not work on
   them.

## Build & Test

```bash
cargo test -p modeling-ops
cargo clippy --all-targets -p modeling-ops -- -D warnings
cargo fmt --check -p modeling-ops
```

Tests here use `MockKernel`. Real-geometry tests are in `crates/test-harness`.

## Key Files

- `src/types.rs` — `OpResult`, `BodyOutput`, `Provenance`, `Diagnostics`, `OpError`
- `src/kernel_ext.rs` — `KernelBundle`
- `src/diff.rs` — snapshot, diff, signature similarity
- `src/extrude.rs`, `src/revolve.rs`, `src/boolean.rs`, `src/pipe.rs`,
  `src/sweep.rs`, `src/pattern.rs`, `src/import.rs`
