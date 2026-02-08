# 06 — Feature Engine: Agent Instructions

You are working on **feature-engine**. This is the hardest and most important sub-project. Read ARCHITECTURE.md in this directory thoroughly — especially the persistent naming strategy.

## Your Job

Build the parametric modeling engine: feature tree management, the GeomRef resolution algorithm (persistent naming), the rebuild algorithm, and undo/redo.

## Critical Rules

1. **GeomRef resolution is the core algorithm.** Test it obsessively. Create a feature tree, modify an early feature, verify all downstream references resolve correctly.
2. **The MockKernel from kernel-fork is your primary testing tool.** Do not wait for TruckKernel. Build and test everything against MockKernel's deterministic output.
3. **Rebuild must be correct before it's fast.** Get the algorithm right first. Optimize later.
4. **Role-based resolution first, signature fallback second.** Roles are fast and stable. Signatures are the safety net for when topology changes invalidate roles.
5. **Document persistent naming failures.** When you find cases that break, add them to PLAN.md under Notes. This helps future work.

## Build & Test

```bash
cargo test -p feature-engine
cargo clippy -p feature-engine
```

## Test Scenarios (Priority Order)

1. Add sketch → extrude → verify GeomRef to extrude face resolves.
2. Edit sketch dimension → rebuild → verify extrude updates correctly.
3. Add fillet referencing extrude edge → verify fillet's GeomRef resolves.
4. Edit sketch dimension → rebuild → verify fillet's GeomRef still resolves (role-based).
5. Add feature in middle of tree → verify downstream refs survive.
6. Remove feature → verify dependent features error appropriately.
7. Undo/redo cycle → verify state consistency.

## Key Files

- `src/lib.rs` — Engine struct, public API
- `src/tree.rs` — FeatureTree data structure and mutations
- `src/resolve.rs` — GeomRef resolution algorithm
- `src/rebuild.rs` — Rebuild algorithm (replay from change point)
- `src/undo.rs` — Command pattern undo/redo
- `src/commands.rs` — Command types (AddFeature, EditFeature, etc.)

## Dependencies

- kernel-fork (MockKernel for testing, Kernel/KernelIntrospect traits)
- modeling-ops (OpResult production)
- sketch-solver (SolvedSketch for Sketch features)
