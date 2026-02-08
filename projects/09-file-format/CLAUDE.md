# 09 — File Format: Agent Instructions

You are working on **file-format**. Read ARCHITECTURE.md in this directory first.

## Your Job

Implement save, load, and STEP export for Waffle Iron projects. The native format is JSON containing the feature tree — NOT geometry. On load, the entire model rebuilds from the recipe.

## Critical Rules

1. **Store the recipe, not geometry.** The JSON file contains Operations, parameters, and GeomRefs. No BREP, no meshes, no solved positions.
2. **Test round-trips obsessively.** Save → load → rebuild → verify the resulting topology matches the original.
3. **Version the format.** Include a version number. Write migration functions for future format changes.
4. **Handle STEP export failures gracefully.** truck's STEP export has known limitations. Return clear error messages.
5. **All enums use `#[serde(tag = "type")]`.** Forward-compatible tagging.

## Build & Test

```bash
cargo test -p file-format
cargo clippy -p file-format
```

## Key Files

- `src/lib.rs` — Public API (save, load, export)
- `src/save.rs` — Serialization logic
- `src/load.rs` — Deserialization + validation
- `src/migrate.rs` — Version migration
- `src/step_export.rs` — STEP export via ruststep
- `src/metadata.rs` — ProjectMetadata type

## Dependencies

- feature-engine (FeatureTree, Engine for rebuild on load)
- kernel-fork (for STEP export via ruststep)
- serde, serde_json
