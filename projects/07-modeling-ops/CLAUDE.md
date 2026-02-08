# 07 — Modeling Ops: Agent Instructions

You are working on **modeling-ops**. Read ARCHITECTURE.md in this directory first.

## Your Job

Implement modeling operations (extrude, revolve, fillet, chamfer, shell, boolean combine). Every operation MUST return a complete OpResult with provenance — created/deleted/modified entities and role assignments. This provenance is what makes persistent naming work in feature-engine.

## Critical Rules

1. **Every operation returns complete provenance.** This is non-negotiable. Feature-engine depends on it.
2. **Assign semantic roles to created entities.** EndCapPositive, SideFace, FilletFace, etc. These are how downstream features reference geometry stably.
3. **The topology diff utility is foundational.** Build it first. All operations use it.
4. **Test against MockKernel first, then TruckKernel.** MockKernel gives deterministic, predictable results.
5. **Operations are stateless.** Each operation receives all context as parameters. No hidden state.

## Build & Test

```bash
cargo test -p modeling-ops
cargo clippy -p modeling-ops
```

## Key Files

- `src/lib.rs` — Public API (execute_extrude, execute_revolve, etc.)
- `src/diff.rs` — Topology diff utility
- `src/extrude.rs` — Extrude operation
- `src/revolve.rs` — Revolve operation
- `src/fillet.rs` — Fillet operation
- `src/chamfer.rs` — Chamfer operation
- `src/shell.rs` — Shell operation
- `src/boolean.rs` — Boolean combine operation
- `src/roles.rs` — Role assignment logic

## Dependencies

- kernel-fork (Kernel + KernelIntrospect traits)
- No dependencies on feature-engine or sketch-solver (this crate is called BY feature-engine)
