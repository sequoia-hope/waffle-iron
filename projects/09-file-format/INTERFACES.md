# 09 — File Format: Interfaces

## Public API

```rust
pub fn load_document(json: &str) -> Result<LoadedDocument, LoadError>;
pub fn save_document(doc: &WaffleDocument) -> String;
pub fn save_document_verified(doc: &WaffleDocument) -> Result<String, LoadError>;

// Single-part convenience forms
pub fn load_project(json: &str) -> Result<(FeatureTree, ProjectMetadata), LoadError>;
pub fn save_project(tree: &FeatureTree, metadata: &ProjectMetadata) -> String;

pub fn export_step(tree: &FeatureTree, kb: &mut dyn KernelBundle) -> Result<String, ExportError>;

pub const FORMAT_VERSION: u32;
pub const MIN_READER_VERSION: u32;
```

Also exported: `WaffleDocument`, `LoadedDocument { document, warnings }`,
`DocumentMetadata`, `Tab`, `TabKind`, `PreviewMesh`, `SaveVerifier`,
`SourceEntry`, `SourceKind`, `Locator`, `GitRef`, `GitHost`, `Embed`,
`git_blob_sha1`, `check_content_hash`, `rebase_relative_sources`,
`join_repo_path`.

## Consumes

| Type | Source |
|---|---|
| `FeatureTree`, `Feature`, `Operation`, `AssemblyTree`, `Drawing` | `feature-engine` |
| `GeomRef`, `Sketch`, `SketchEntity`, `SketchConstraint` | `waffle-types` |
| `KernelBundle` | `modeling-ops` |

## File

Extension `.waffle`. UTF-8 JSON. Whitespace is not significant. The full
field-by-field definition is `docs/FILE_FORMAT.md`; the machine-readable
schema is `docs/schema/waffle-v5.schema.json`.
