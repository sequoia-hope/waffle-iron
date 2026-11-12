# 09 — File Format: Agent Instructions

You are working on `crates/file-format`. Read `docs/FILE_FORMAT.md` (the
format spec) and ARCHITECTURE.md in this directory first.

## Rules

1. **Store the recipe, not geometry.**
2. **Update `docs/FILE_FORMAT.md` in the same commit** as any format change.
3. **Additive changes are serde-defaulted.** A new optional field must parse
   when absent. Bump `FORMAT_VERSION`, and `MIN_READER_VERSION` only when an
   older reader would misread the file; mirror both in
   `app/src/lib/engine/format.js`.
4. **Preserve what you do not understand.** Unknown keys and unknown tagged
   kinds are written back unchanged.
5. **Persisted enums are internally tagged** (`#[serde(tag = "type")]`).
6. **Old files keep loading.** `tests/corpus_backcompat.rs` must stay green; a
   breaking shape change needs a migration in `migrate.rs`.
7. **Regenerate the schema golden** when a persisted type changes.

## Build & Test

```bash
cargo test -p file-format
cargo test -p file-format --features json-schema --test schema_golden
cargo clippy --all-targets -p file-format -- -D warnings
```

## Key Files

- `src/document.rs` — `WaffleDocument`, `LoadedDocument`, validation
- `src/metadata.rs` — `DocumentMetadata`, `Tab`, `TabKind`, `PreviewMesh`
- `src/save.rs` — writers, `SaveVerifier`, version constants
- `src/load.rs`, `src/migrate.rs` — readers and migrations
- `src/sources.rs`, `src/hash.rs` — sources table, locators, content hashes
- `src/schema.rs` — JSON Schema generation
- `src/step_export.rs` — single-tree STEP export
- `src/errors.rs` — `LoadError`, `ExportError`

## Dependencies

`feature-engine`, `modeling-ops`, `waffle-types`, `step-import`, `serde`,
`serde_json`, `chrono`, `uuid`, `sha1`.
