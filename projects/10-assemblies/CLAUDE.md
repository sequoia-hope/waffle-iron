# 10 — Assemblies: Agent Instructions

Read ARCHITECTURE.md in this directory first. Assembly code lives in:

- `crates/feature-engine/src/assembly.rs` — model, rigid-transform math,
  `solve_fastened`
- `crates/feature-engine/src/assembly_solver.rs` — `solve_mates`
- `crates/feature-engine/src/connector.rs` — connector frames
- `crates/feature-engine/src/context.rs` — in-context editing
- `crates/wasm-bridge/src/assembly_view.rs` — evaluation and flattening
- `crates/wasm-bridge/src/tools/assembly.rs` — agent tools
- `app/src/lib/ui/AssemblyPanel.svelte` — UI

## Rules

1. **Placements are derived.** Recompute them on every evaluation; never
   treat the saved values as authoritative.
2. **Nothing is guessed.** An unknown mate kind, an unsatisfiable mate, a pick
   with no frame, a missing part connector and a scoped reference without a
   context are all reported, not defaulted.
3. **A scoped `GeomRef` resolves only through `EditContext`.**
4. **New fields on persisted types are additive and serde-defaulted.** Update
   `docs/FILE_FORMAT.md` and regenerate the schema golden in the same commit.
5. **Key part builds on `PartBuild`, not `PartRef`.** Two instances with
   different parameter overrides are different solids.

## Test

```bash
cargo test -p feature-engine --test connector_tests --test context_tests
cargo test -p wasm-bridge --test assembly_tests --test tool_assembly
```

GUI: `app/tests/gui/assembly.spec.js`, `in-context-editing.spec.js`,
`part-connectors.spec.js`.
