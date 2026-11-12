# 06 — Feature Engine: Interfaces

## Defines

`Engine`, `Feature`, `FeatureTree`, `Operation` and its parameter structs,
`EngineError`, `FeatureError`, `DesignParameter`, `Command`, `UndoStack`, the
assembly and drawing document models.

## Consumes

| Type | Source |
|---|---|
| `GeomRef`, `Anchor`, `OutputKey`, `Selector`, `Role`, `TopoSignature`, `TopoQuery`, `ResolvePolicy` | `waffle-types` |
| `Kernel`, `KernelIntrospect`, `KernelId`, `KernelSolidHandle` | `waffle-types` (`kernel`) |
| `KernelBundle`, `OpResult` | `modeling-ops` |
| `Sketch`, `SolvedSketch`, `ClosedProfile` | `waffle-types`, solved by `sketch-solver` |

## Engine API

Every mutating call takes the kernel and rebuilds from the affected feature.

```rust
impl Engine {
    pub fn new() -> Self;

    pub fn add_feature(&mut self, name: String, operation: Operation,
                       kb: &mut dyn KernelBundle) -> Result<Uuid, EngineError>;
    pub fn edit_feature(&mut self, id: Uuid, operation: Operation,
                        kb: &mut dyn KernelBundle) -> Result<(), EngineError>;
    pub fn remove_feature(&mut self, id: Uuid,
                          kb: &mut dyn KernelBundle) -> Result<(), EngineError>;
    pub fn set_suppressed(&mut self, id: Uuid, suppressed: bool,
                          kb: &mut dyn KernelBundle) -> Result<(), EngineError>;
    pub fn reorder_feature(&mut self, id: Uuid, new_position: usize,
                           kb: &mut dyn KernelBundle) -> Result<(), EngineError>;
    pub fn rename_feature(&mut self, id: Uuid, new_name: String) -> Result<(), EngineError>;
    pub fn set_rollback(&mut self, index: Option<usize>, kb: &mut dyn KernelBundle);

    pub fn undo(&mut self, kb: &mut dyn KernelBundle) -> Result<(), EngineError>;
    pub fn redo(&mut self, kb: &mut dyn KernelBundle) -> Result<(), EngineError>;
    pub fn can_undo(&self) -> bool;
    pub fn can_redo(&self) -> bool;

    pub fn rebuild_from_scratch(&mut self, kb: &mut dyn KernelBundle);
    pub fn get_result(&self, feature_id: Uuid) -> Option<&OpResult>;
}
```

`add_feature_with_provenance` and `edit_feature_with_provenance` record who
authored a change in the same undo step. `set_parameters`,
`set_document_parameters`, `edit_materials`, `set_entity_name`,
`clear_entity_name` and `rename_body` are in `src/lib.rs`.

## Rebuild outputs (public fields of `Engine`)

| Field | Contents |
|---|---|
| `tree` | The feature tree |
| `feature_results` | `OpResult` per feature from the last rebuild |
| `errors`, `feature_errors` | Per-feature errors, as text and typed |
| `warnings`, `feature_warnings` | Warnings, and the same attributed to features |
| `feature_references` | What the rebuild learned about each reference it resolved |
| `consumed_features`, `consumed_by` | Features whose bodies a later feature consumed |
| `pid_to_feature` | Persistent id → the feature that introduced it |
