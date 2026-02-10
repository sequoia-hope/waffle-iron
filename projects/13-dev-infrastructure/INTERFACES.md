# Sub-project 13: Type Contracts

## Types Exported by test-harness

### `ModelBuilder` (workflow.rs)
```rust
pub struct ModelBuilder {
    pub state: EngineState,
    // kernel: Box<dyn KernelBundle> (pub(crate))
}

// Constructors
ModelBuilder::mock() -> Self
ModelBuilder::truck() -> Self
ModelBuilder::with_auto_check(self) -> Self

// Sketch shortcuts
rect_sketch(&mut self, name, origin, normal, x, y, w, h) -> Result<Uuid>
circle_sketch(&mut self, name, origin, normal, cx, cy, r) -> Result<Uuid>

// Manual sketch
begin_sketch(&mut self, origin, normal) -> &mut Self
add_point(&mut self, id, x, y) -> &mut Self
add_line(&mut self, id, start, end) -> &mut Self
add_circle_entity(&mut self, id, center, radius) -> &mut Self
add_arc(&mut self, id, center, start, end) -> &mut Self
finish_sketch_manual(&mut self, name, positions, profiles, origin, normal) -> Result<Uuid>

// Feature operations
extrude(&mut self, name, sketch_name, depth) -> Result<Uuid>
extrude_cut(&mut self, name, sketch_name, depth) -> Result<Uuid>
extrude_on_face(&mut self, name, sketch_name, depth, direction) -> Result<Uuid>
revolve(&mut self, name, sketch_name, axis_origin, axis_dir, angle_deg) -> Result<Uuid>
fillet(&mut self, name, target, radius) -> Result<Uuid>
chamfer(&mut self, name, target, distance) -> Result<Uuid>
shell(&mut self, name, target, thickness) -> Result<Uuid>
boolean_union(&mut self, name, a, b) -> Result<Uuid>
boolean_subtract(&mut self, name, a, b) -> Result<Uuid>
boolean_intersect(&mut self, name, a, b) -> Result<Uuid>

// History
undo(&mut self) -> Result<&mut Self>
redo(&mut self) -> Result<&mut Self>

// Feature management
suppress(&mut self, name) -> Result<&mut Self>
unsuppress(&mut self, name) -> Result<&mut Self>
delete_feature(&mut self, name) -> Result<&mut Self>
reorder(&mut self, name, position) -> Result<&mut Self>

// Queries
feature_id(&self, name) -> Result<Uuid>
feature_count(&self) -> usize
solid_handle(&self, name) -> Result<KernelSolidHandle>
tessellate(&mut self, name) -> Result<RenderMesh>
topology_counts(&self, name) -> Result<(usize, usize, usize)>
kernel(&self) -> &dyn KernelBundle
engine_errors(&self) -> &[(Uuid, String)]

// File I/O
save(&mut self) -> Result<String>
load(&mut self, json) -> Result<&mut Self>
export_stl(&mut self, name) -> Result<Vec<u8>>

// Report
report(&mut self) -> Result<ModelReport>

// Assertions
assert_feature_count(&self, expected) -> Result<&Self>
assert_has_solid(&self, name) -> Result<&Self>
assert_no_errors(&self) -> Result<&Self>
assert_has_errors(&self) -> Result<&Self>
```

### `OracleVerdict` (oracle.rs)
```rust
pub struct OracleVerdict {
    pub oracle_name: String,
    pub passed: bool,
    pub detail: String,
    pub value: Option<f64>,
}
```

### `HarnessError` (helpers.rs)
```rust
pub enum HarnessError {
    FeatureNotFound { name },
    DispatchError { message },
    NoSolid { name },
    AssertionFailed { detail },
    OracleFailure { oracle, detail },
    StlError { reason },
    Engine(String),
    DuplicateName { name },
}
```

### `ModelReport` (report.rs)
```rust
pub struct ModelReport {
    pub feature_entries: Vec<FeatureEntry>,
    pub mesh_summaries: Vec<MeshSummary>,
    pub bounding_box: Option<([f32; 3], [f32; 3])>,
    pub oracle_results: Vec<OracleVerdict>,
    pub errors: Vec<(String, String)>,
}

ModelReport::to_text(&self) -> String
```

## Types Consumed (from other crates)

| Type | From | Usage |
|------|------|-------|
| `EngineState` | wasm-bridge | Owned by ModelBuilder |
| `dispatch()` | wasm-bridge | Core message handler |
| `UiToEngine/EngineToUi` | wasm-bridge | Message protocol |
| `KernelBundle` | modeling-ops | Trait object for kernel |
| `MockKernel/TruckKernel` | kernel-fork | Kernel implementations |
| `RenderMesh/FaceRange` | kernel-fork | Tessellated mesh data |
| `KernelSolidHandle/KernelId` | kernel-fork | Entity handles |
| `OpResult/Role/Provenance` | modeling-ops | Operation results |
| `GeomRef/Anchor/Selector` | waffle-types | Persistent naming |
| `SketchEntity/ClosedProfile` | waffle-types | Sketch data |
| `Feature/Operation/*Params` | feature-engine | Feature tree types |
