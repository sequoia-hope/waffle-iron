# 09 — File Format: Architecture

`crates/file-format` reads and writes `.waffle` documents. **The format itself
is specified in [`docs/FILE_FORMAT.md`](../../docs/FILE_FORMAT.md)**; this
dossier describes the crate. Change the spec in the same commit as the code.

## What a file is

A `.waffle` file is a UTF-8 JSON document holding the parametric recipe of a
model, not its geometry. One file is one document with one or more tabs. A tab
is a Part (a feature tree), an Assembly or a Drawing. On load, every part is
rebuilt from its feature tree.

- **Stored:** features and their parameters, sketches and constraints,
  geometry references, design parameters and expressions, materials, entity
  names, the tab list, document metadata, and the `sources` table.
- **Stored as hints:** sketch solve status, solved positions and profiles,
  region boundaries inside extrude parameters, and an optional small preview
  mesh per tab. The engine recomputes them when they are absent.
- **Never stored:** B-Rep solids, kernel ids, render meshes, undo history.

Lengths are meters and angles are degrees. `display_unit` is a UI preference.

## Writing

There is one writer. The app assembles the document and sends
`SaveDocument`; the bridge calls `save_document_verified`, which serializes
and then loads its own output, so a document that would not load again (a
non-finite float, for example) is refused at save time. `SaveVerifier` caches
tab payloads it has already verified.

Every file carries `version` (`FORMAT_VERSION`) and `min_reader_version`
(`MIN_READER_VERSION`). A reader older than `min_reader_version` refuses the
file with `LoadError::FutureVersion`. The same two constants are mirrored in
`app/src/lib/engine/format.js`; `tests/js_format_mirror.rs` keeps them equal.

## Reading

`load_document` parses the current shape directly and migrates older versions
on the way in (`migrate.rs`). It returns the document and a list of warnings.

Forward compatibility:

- Unknown keys are preserved across load → save at the envelope, document,
  tab, source and feature-tree levels.
- Unknown tab kinds, source kinds, locator kinds and operation kinds are kept
  as opaque values and written back unchanged. An unknown operation is a
  per-feature rebuild error; later features still build.
- Floats parse exactly (`serde_json` `float_roundtrip`), so load → save never
  drifts a coordinate.

## Sources

`sources.rs` is the document's table of external content (STEP files, KiCad
boards, scripts). A `SourceEntry` has a kind, a `Locator` (`Git` pinned to a
commit or floating on a branch or tag, `Relative`, `Url`, `Local`,
`Embedded`), a `git-blob-sha1` content hash (`hash.rs`), and optionally a
packed copy of the content so the file is self-contained.
`rebase_relative_sources` turns `Relative` locators into absolute `Git`
locators when a linked document is forked.

## Schema

With the `json-schema` feature, `schema.rs` generates the JSON Schema of the
file from the Rust types. `tests/schema_golden.rs` pins it to
`docs/schema/waffle-v5.schema.json` and validates every `.waffle` file in the
repository against it.

## STEP export

`export_step(tree, kb)` rebuilds a feature tree and exports the last
feature's main body through `Kernel::export_step` (kernel-v2 writes analytic
AP214). The app's whole-model export, including placed assembly instances, is
the bridge's `ExportStep`.

## Tests

| Test | Pins |
|---|---|
| `tests/corpus_backcompat.rs` | Every `.waffle` in the repository (assay corpus, fixtures, examples) still loads. Adding a `.waffle` to the repo adds it to this corpus. |
| `tests/schema_golden.rs` | The generated schema, and every repo file against it |
| `tests/js_format_mirror.rs` | JS and Rust version constants agree |
| `tests/format_tests.rs`, `tests/v4_document_tests.rs` | Round trips, migrations, refusals |
