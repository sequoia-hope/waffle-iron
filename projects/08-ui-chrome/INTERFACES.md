# 08 — UI Chrome: Interfaces

The chrome works with JSON the bridge has already deserialized. Message types
are defined in `crates/wasm-bridge/src/messages.rs`.

## Reads (from the store)

| Data | Source message |
|---|---|
| Feature tree, bodies, errors, warnings, document and tab list | `ModelUpdated` |
| Sketch solve status | `SketchSolved` |
| Selection and hover | Store state |
| Errors | `Error` |

## Sends (through store functions)

| Message | Trigger |
|---|---|
| `AddFeature`, `EditFeature`, `DeleteFeature` | Dialogs, property editor, tree |
| `SuppressFeature`, `ReorderFeature`, `RenameFeature`, `RenameBody` | Feature tree |
| `SetRollbackIndex` | Rollback slider |
| `Undo`, `Redo` | Toolbar, shortcuts |
| `SwitchTab`, `AddTab`, `CloseTab`, `RenameTab`, `MoveTab` | Tab bar |
| `SetParameters`, `SetDocumentParameters` | Variables |
| `SaveDocument`, `LoadProject`, `NewDocument`, `ImportStep`, `ExportStep`, `ExportStl` | File actions |
