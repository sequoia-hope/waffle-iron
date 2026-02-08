# 09 — File Format: Interfaces

## Types This Crate IMPLEMENTS

| Type | Role |
|------|------|
| Save logic | FeatureTree → JSON serialization |
| Load logic | JSON → FeatureTree deserialization + rebuild trigger |
| STEP export | FeatureTree → rebuild → STEP file via ruststep |
| Version migration | Old format → current format |

## Types This Crate CONSUMES

| Type | Source | Purpose |
|------|--------|---------|
| `FeatureTree` | INTERFACES.md (feature-engine) | The data to serialize/deserialize |
| `Feature` | INTERFACES.md | Individual features |
| `Operation` (all variants) | INTERFACES.md | Operation parameters |
| `GeomRef` | INTERFACES.md | Geometry references |
| `Sketch` | INTERFACES.md | Sketch data |
| `SketchEntity` | INTERFACES.md | Sketch entities |
| `SketchConstraint` | INTERFACES.md | Sketch constraints |

## Public API

```rust
/// Save a project to JSON string.
pub fn save_project(tree: &FeatureTree, metadata: &ProjectMetadata) -> String;

/// Load a project from JSON string.
pub fn load_project(json: &str) -> Result<(FeatureTree, ProjectMetadata), LoadError>;

/// Export the current model to STEP format.
pub fn export_step(
    tree: &FeatureTree,
    engine: &mut Engine,
) -> Result<String, ExportError>;
```

## File Format

- Extension: `.waffle`
- Format: JSON (pretty-printed)
- Encoding: UTF-8
- Version field for migration support

## Notes

- All persisted types must use `#[serde(tag = "type")]` for forward-compatible enums.
- Unknown fields are preserved via `#[serde(flatten)]` for forward compatibility.
