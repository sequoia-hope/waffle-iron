# 06 — Feature Engine: Interfaces

## Types This Crate IMPLEMENTS

| Type | Role |
|------|------|
| `FeatureTree` | Ordered feature list with rollback support |
| `Feature` | Individual feature (operation + references + metadata) |
| GeomRef resolution | Anchor → OpResult → Selector → KernelId |
| Rebuild algorithm | Replay features from change point, re-resolve refs |
| Undo/redo | Command pattern on feature tree mutations |

## Types This Crate CONSUMES

| Type | Source | Purpose |
|------|--------|---------|
| `Operation` (all variants) | INTERFACES.md | Feature operation parameters |
| `GeomRef`, `Anchor`, `Selector`, `Role` | INTERFACES.md | Persistent naming |
| `TopoSignature` | INTERFACES.md | Signature-based fallback matching |
| `ResolvePolicy` | INTERFACES.md | Resolution failure handling |
| `OpResult` | modeling-ops | Operation results with provenance |
| `Provenance`, `EntityRecord`, `Rewrite` | modeling-ops | Entity tracking |
| `SolvedSketch` | sketch-solver | Solved sketch for Sketch features |
| `ClosedProfile` | sketch-solver | Profiles for extrude/revolve |
| `Kernel` trait | kernel-fork | Kernel operations (via modeling-ops) |
| `KernelIntrospect` trait | kernel-fork | Topology queries for resolution |
| `KernelSolidHandle`, `KernelId` | kernel-fork | Runtime geometry handles |

## Public API

```rust
/// The main engine interface.
pub struct Engine {
    tree: FeatureTree,
    // ... internal state
}

impl Engine {
    /// Add a feature at the end of the tree. Triggers rebuild.
    pub fn add_feature(&mut self, operation: Operation) -> Result<(), EngineError>;

    /// Edit an existing feature's operation. Triggers rebuild from that feature.
    pub fn edit_feature(&mut self, id: Uuid, operation: Operation) -> Result<(), EngineError>;

    /// Delete a feature. Triggers rebuild.
    pub fn delete_feature(&mut self, id: Uuid) -> Result<(), EngineError>;

    /// Suppress/unsuppress a feature. Triggers rebuild.
    pub fn suppress_feature(&mut self, id: Uuid, suppressed: bool) -> Result<(), EngineError>;

    /// Set rollback index. Triggers rebuild.
    pub fn set_rollback_index(&mut self, index: Option<usize>) -> Result<(), EngineError>;

    /// Undo the last command.
    pub fn undo(&mut self) -> Result<(), EngineError>;

    /// Redo the last undone command.
    pub fn redo(&mut self) -> Result<(), EngineError>;

    /// Get current feature tree state (for UI display).
    pub fn feature_tree(&self) -> &FeatureTree;

    /// Get tessellated meshes for all visible bodies.
    pub fn meshes(&self) -> Vec<RenderMesh>;
}
```

## Notes

- This crate orchestrates modeling-ops and sketch-solver. It calls the Kernel trait via modeling-ops.
- GeomRef resolution is the core algorithm. All other crates depend on it for stable geometry references.
- The rebuild algorithm is the performance-critical path. Optimize carefully.
