# 08 — UI Chrome: Interfaces

## Types This Crate CONSUMES

| Type | Source | Purpose |
|------|--------|---------|
| `EngineToUi::ModelUpdated` | wasm-bridge | Feature tree + meshes after rebuild |
| `EngineToUi::SketchSolved` | wasm-bridge | Solve status for status bar |
| `EngineToUi::Error` | wasm-bridge | Error display |
| `EngineToUi::SelectionChanged` | wasm-bridge | Selection info for status bar |
| `FeatureTree` | INTERFACES.md (via JSON) | Feature list for tree display |
| `Feature` | INTERFACES.md (via JSON) | Individual feature data |
| `Operation` (all variants) | INTERFACES.md (via JSON) | Parameter display/editing |

## Types/Events This Crate PRODUCES

| Type | Consumer | Purpose |
|-------|----------|---------|
| `UiToEngine::AddFeature` | wasm-bridge | Add new feature |
| `UiToEngine::EditFeature` | wasm-bridge | Edit feature parameters |
| `UiToEngine::DeleteFeature` | wasm-bridge | Remove feature |
| `UiToEngine::SuppressFeature` | wasm-bridge | Suppress/unsuppress |
| `UiToEngine::SetRollbackIndex` | wasm-bridge | Rollback slider |
| `UiToEngine::Undo` | wasm-bridge | Undo |
| `UiToEngine::Redo` | wasm-bridge | Redo |
| `UiToEngine::SelectEntity` | wasm-bridge | Entity selection from tree |

## Notes

- All data arrives as JSON-deserialized objects from wasm-bridge.
- This crate never imports Rust types directly.
- The FeatureTree in ModelUpdated drives the entire feature tree display.
