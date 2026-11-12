# 13 — Development Infrastructure: Interfaces

Types exported by `crates/test-harness` for scripting and checking models.
`Result<T>` below is `Result<T, HarnessError>`.

## `ModelBuilder` (`src/workflow.rs`)

```rust
pub struct ModelBuilder { pub state: EngineState, /* kernel, names, history */ }

// Constructors
ModelBuilder::mock() -> Self
ModelBuilder::kernel_v2() -> Self
with_auto_check(self) -> Self          // assert no engine errors after every operation

// Sketch shortcuts
rect_sketch(name, origin, normal, x, y, w, h) -> Result<Uuid>
circle_sketch(name, origin, normal, cx, cy, r) -> Result<Uuid>
polygon_sketch(..) -> Result<Uuid>

// Manual sketch
begin_sketch(origin, normal) -> &mut Self
add_point(id, x, y) / add_line(id, start, end)
add_circle_entity(id, center, radius) / add_arc(id, center, start, end)
add_sketch_entity(entity)
finish_sketch_manual(name, positions, profiles, origin, normal) -> Result<Uuid>

// Features
extrude(name, sketch, depth) / extrude_no_merge / extrude_cut / extrude_on_face
extrude_through_all / extrude_up_to / extrude_directed / extrude_bidirectional
extrude_symmetric / extrude_advanced
revolve(name, sketch, axis_origin, axis_dir, angle_deg) / revolve_cut
pipe(..) / board_shape(..)
boolean_union(name, a, b) / boolean_subtract / boolean_intersect
add_operation(..)                      // any Operation
edit_extrude_depth(name, depth)

// History and feature management
undo() / redo()
suppress(name) / unsuppress(name) / delete_feature(name) / reorder(name, position)

// Queries
feature_id(name) -> Result<Uuid>
feature_count() -> usize
solid_handle(name) / solid_handles(name) / live_solid_handles()
tessellate(name) / tessellate_all(name) / tessellate_combined(name) / tessellate_last()
topology_counts(name) -> Result<(usize, usize, usize)>
face_signatures(..) / select_face_by_role(..) / select_face_by_normal(..)
op_result(name) -> Result<&OpResult>
engine_errors() / engine_warnings() / consumed_features()
kernel_ref() -> &dyn KernelBundle

// Raw access
dispatch(msg: UiToEngine) -> EngineToUi
agent_tool(..)                         // run an agent tool by name
rebuild_from_scratch()

// File I/O
save() -> Result<String>
load(json) -> Result<&mut Self>
export_stl(name) -> Result<Vec<u8>>

// Checks
check_mesh(name) / check_topology(name) -> Result<Vec<OracleVerdict>>
report() -> Result<ModelReport>
assert_feature_count(n) / assert_has_solid(name) / assert_no_errors() / assert_has_errors()
```

## `OracleVerdict` (`src/oracle.rs`)

```rust
pub struct OracleVerdict {
    pub oracle_name: String,
    pub passed: bool,
    pub detail: String,
    pub value: Option<f64>,
}
```

## `HarnessError` (`src/helpers.rs`)

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

## `ModelReport` (`src/report.rs`)

```rust
pub struct ModelReport {
    pub feature_entries: Vec<FeatureEntry>,
    pub mesh_summaries: Vec<MeshSummary>,
    pub bounding_box: Option<([f32; 3], [f32; 3])>,
    pub oracle_results: Vec<OracleVerdict>,
    pub errors: Vec<(String, String)>,
}
// to_text(&self) -> String
```

## Consumes

| Type | From |
|---|---|
| `EngineState`, `dispatch`, `UiToEngine`, `EngineToUi`, `execute_tool` | `wasm-bridge` |
| `KernelBundle`, `OpResult` | `modeling-ops` |
| `Feature`, `Operation` and parameter structs | `feature-engine` |
| `MockKernel`, `RenderMesh`, `KernelSolidHandle`, `GeomRef`, `SketchEntity`, `ClosedProfile` | `waffle-types` |
| `KernelV2Adapter` | `kernel-v2` |
