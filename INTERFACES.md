# Waffle Iron — Cross-Crate Interfaces

The contracts between crates, and between the engine and its hosts. The Rust
source is normative; each section names the file it describes. Code blocks are
taken from the source with doc comments shortened. Where a type is too large to
reproduce, the section lists its variants or fields by name and points at the
file.

| Section | Contract | Defined in |
|---|---|---|
| A | Geometry references | `crates/waffle-types/src/{geom_ref,topo,roles}.rs` |
| B | Operation results | `crates/modeling-ops/src/types.rs` |
| C | Sketch types | `crates/waffle-types/src/sketch.rs` |
| D | Feature tree | `crates/feature-engine/src/types.rs` |
| E | Bridge protocol | `crates/wasm-bridge/src/messages.rs` |
| F | Render data | `crates/waffle-types/src/kernel/types.rs`, `crates/wasm-bridge/src/render_view.rs` |
| G | Kernel contract | `crates/waffle-types/src/kernel/` |

The persisted form of a document is specified separately in
`docs/FILE_FORMAT.md`.

**Conventions**

- Lengths are meters.
- Persisted enums are internally tagged with `#[serde(tag = "type")]`. The
  exceptions are noted where they occur.
- `KernelSolidHandle` and `KernelId` are valid for one kernel session only.
  They are never written to a `.waffle` file. Anything persisted refers to
  geometry through a `GeomRef`.
- Persistent ids (`pid`, `root_pid`) are `u64` in Rust and decimal strings on
  the wire, because they exceed the range JavaScript numbers hold exactly.

---

## A) Geometry references

**Defined in:** `waffle-types` (`geom_ref.rs`, `topo.rs`, `roles.rs`)
**Used by:** feature-engine, modeling-ops, wasm-bridge, file-format, the app

A `GeomRef` names one face, edge or vertex in a way that survives a parametric
rebuild. It says which feature output holds the entity (`anchor`) and how to
find the entity there (`selector`).

```rust
pub enum TopoKind { Vertex, Edge, Face, Shell, Solid }

pub struct GeomRef {
    pub kind: TopoKind,
    pub anchor: Anchor,
    pub selector: Selector,
    #[serde(default)]
    pub policy: ResolvePolicy,
    /// Present when the anchor lives in another tab's assembly instance.
    /// Absent means the current tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<RefScope>,
}

pub struct RefScope {
    pub source_id: Option<Uuid>,     // absent: this document
    pub tab_id: Option<String>,      // absent: the current tab
    pub instance_path: Vec<Uuid>,    // assembly-instance ids down to the part
}

pub enum Anchor {
    FeatureOutput { feature_id: Uuid, output_key: OutputKey },
    Datum { datum_id: Uuid },
}

pub enum OutputKey {
    Main,
    Body { index: usize },
    Profile { index: usize },
    Datum { name: String },
    /// A body a custom feature script named in its return value.
    Named { name: String },
}

pub enum Selector {
    /// The semantic role a modeling operation assigned.
    Role { role: Role, index: usize },
    /// Geometric fingerprint matching.
    Signature { signature: TopoSignature },
    /// Filter-and-tie-break query.
    Query { query: TopoQuery },
    /// Nearest entity to a 3D position.
    Position { x: f64, y: f64, z: f64 },
    /// Persistent id. Never rebinds: if both ids are gone the reference
    /// fails, under either policy.
    Pid {
        #[serde(with = "crate::pid_str")] pid: u64,
        #[serde(with = "crate::pid_str")] root_pid: u64,
    },
}

pub enum ResolvePolicy {
    /// Fail if the reference cannot be uniquely resolved.
    Strict,
    /// Use the closest match and emit a warning. The default.
    BestEffort,
}
```

`OutputKey::tag()` gives the stable string (`"Main"`, `"Body:1"`, `"Named:hub"`)
used to build a body id of the form `"{feature_id}/{tag}"`.

```rust
pub enum Role {
    EndCapPositive,
    EndCapNegative,
    SideFace { index: usize },
    RevStartFace,
    RevEndFace,
    FilletFace { index: usize },
    ChamferFace { index: usize },
    ShellInnerFace { index: usize },
    ProfileFace,
    PatternInstance { index: usize },
    BooleanBodyAFace { index: usize },
    BooleanBodyBFace { index: usize },
    /// A face or edge a custom feature script named in its return value.
    Named { name: String },
}

pub struct TopoSignature {
    pub surface_type: Option<String>,  // planar, cylindrical, conical, spherical, toroidal, nurbs
    pub area: Option<f64>,
    pub centroid: Option<[f64; 3]>,
    pub normal: Option<[f64; 3]>,
    pub bbox: Option<[f64; 6]>,        // [min_x, min_y, min_z, max_x, max_y, max_z]
    pub adjacency_hash: Option<u64>,
    pub length: Option<f64>,           // edges
    #[serde(default)]
    pub axis: Option<AxisDescriptor>,  // surfaces of revolution
}

/// The rotation-invariant content of a face on a cylinder, cone, sphere or torus.
pub struct AxisDescriptor {
    pub direction: Option<[f64; 3]>,   // None for a sphere
    pub radius: Option<f64>,           // cylinder radius, torus major radius, sphere radius
    pub half_angle: Option<f64>,       // cone, radians
    pub minor_radius: Option<f64>,     // torus tube radius
    pub extent: Option<f64>,           // how far the face reaches along the axis
}

pub struct TopoQuery {
    pub filters: Vec<Filter>,
    pub tie_break: Option<TieBreak>,
}

pub enum Filter {
    SurfaceType { surface_type: String },
    NormalDirection { direction: [f64; 3], tolerance: f64 },  // tolerance in radians
    NearPoint { point: [f64; 3], distance: f64 },
    AreaRange { min: f64, max: f64 },
}

pub enum TieBreak {
    LargestArea,
    NearestTo { point: [f64; 3] },
    FarthestAlong { direction: [f64; 3] },
    SmallestIndex,
}
```

A face that goes all the way round its axis has its centroid on the axis and
no `normal`; such a face is identified by its `axis` descriptor instead.

Resolution is implemented in `feature_engine::resolve`
(`resolve_geom_ref`, and `resolve_geom_ref_live` for selectors that need the
live kernel, such as `Pid`). A reference that cannot be resolved is a typed
`EngineError::ReferenceUnresolved`.

---

## B) Operation results

**Defined in:** `modeling-ops` (`types.rs`, `kernel_ext.rs`)
**Used by:** feature-engine, wasm-bridge

Every modeling operation returns an `OpResult`: the bodies it produced, the
provenance that persistent naming is built on, and diagnostics.

```rust
pub struct OpResult {
    pub outputs: Vec<(OutputKey, BodyOutput)>,
    pub provenance: Provenance,
    pub diagnostics: Diagnostics,
}

pub struct BodyOutput {
    pub handle: KernelSolidHandle,        // runtime only
    pub mesh: Option<RenderMesh>,
    pub edges: Option<EdgeRenderData>,
}

pub struct Provenance {
    pub created: Vec<EntityRecord>,
    pub deleted: Vec<EntityRecord>,
    pub modified: Vec<Rewrite>,
    pub role_assignments: Vec<(KernelId, Role)>,
}

pub struct EntityRecord {
    pub kernel_id: KernelId,
    pub kind: TopoKind,
    pub signature: TopoSignature,
}

pub struct Rewrite {
    pub before: KernelId,
    pub after: KernelId,
    pub reason: RewriteReason,
}

pub enum RewriteReason { Trimmed, Split, Merged, Moved }

pub struct Diagnostics {
    pub warnings: Vec<String>,
    pub kernel_time_ms: f64,
    pub tessellation_time_ms: f64,
    /// An auto-union failed and the tool was kept as a standalone body.
    pub auto_union_fell_back: bool,
}

pub enum OpError {
    Kernel(#[from] waffle_types::kernel::KernelError),
    NoProfiles,
    InvalidParameter { reason: String },
}
```

Operations take the kernel as one object that is both mutable `Kernel` and
readable `KernelIntrospect`:

```rust
pub trait KernelBundle: Kernel + KernelIntrospect + KernelProjection + KernelMeasure {
    fn as_introspect(&self) -> &dyn KernelIntrospect;
    fn as_measure(&self) -> &dyn KernelMeasure;
}
// Blanket-implemented for every type that implements the four traits.
```

The operation entry points are `execute_extrude`, `execute_symmetric_extrude`,
`execute_revolve`, `execute_boolean`, `execute_pattern_instances`,
`execute_pipe`, `execute_sweep`, `execute_import`, `execute_import_analytic`,
`execute_fillet`, `execute_chamfer` and `execute_shell`. For example:

```rust
pub fn execute_extrude(
    kb: &mut dyn KernelBundle,
    face_id: KernelId,
    direction: [f64; 3],
    depth: f64,
    before_snapshot: Option<&TopoSnapshot>,
) -> Result<OpResult, OpError>;

pub fn execute_boolean(
    kb: &mut dyn KernelBundle,
    body_a: &KernelSolidHandle,
    body_b: &KernelSolidHandle,
    kind: BooleanKind,          // Union | Subtract | Intersect
) -> Result<OpResult, OpError>;
```

---

## C) Sketch types

**Defined in:** `waffle-types` (`sketch.rs`)
**Used by:** sketch-solver, feature-engine, wasm-bridge, the app

```rust
pub struct Sketch {
    pub id: Uuid,
    pub plane: GeomRef,
    pub plane_origin: [f64; 3],
    pub plane_normal: [f64; 3],
    /// World direction of the sketch's +u axis. Absent: derived from the normal.
    pub plane_x_axis: Option<[f64; 3]>,
    pub entities: Vec<SketchEntity>,
    pub constraints: Vec<SketchConstraint>,
    pub solve_status: SolveStatus,
    /// Derived: solved point positions, keyed by point entity id.
    pub solved_positions: HashMap<u32, (f64, f64)>,
    /// Derived: closed profiles of the solved geometry.
    pub solved_profiles: Vec<ClosedProfile>,
    /// Points driven by external model geometry.
    pub projected: Vec<ProjectedEntity>,
    /// The local model face the sketch was drawn on, when it was drawn on one.
    pub plane_face: Option<SketchFaceRef>,
}

pub enum SketchEntity {
    Point  { id: u32, x: f64, y: f64, construction: bool },
    Line   { id: u32, start_id: u32, end_id: u32, construction: bool },
    Circle { id: u32, center_id: u32, radius: f64, construction: bool },
    Arc    { id: u32, center_id: u32, start_id: u32, end_id: u32, construction: bool },
    Spline { id: u32, point_ids: Vec<u32>, construction: bool },
    /// Parametric generators, stored compactly and expanded to primitives on demand.
    Gear     { id: u32, params: GearParams, construction: bool },
    Sprocket { id: u32, params: SprocketParams, construction: bool },
}
```

`SketchConstraint` variants, by group (see `sketch.rs` for their fields):

| Group | Variants |
|---|---|
| Geometric | `Coincident`, `Horizontal`, `Vertical`, `HorizontalPoints`, `VerticalPoints`, `Parallel`, `Perpendicular`, `Tangent`, `Equal`, `Symmetric`, `SymmetricH`, `SymmetricV`, `Midpoint`, `OnEntity`, `EqualAngle`, `Ratio`, `EqualPointToLine`, `SameOrientation` |
| Dimensions | `Distance`, `PointLineDistance`, `HDistance`, `VDistance`, `Angle`, `Radius`, `Diameter`, `Offset` |
| Position | `Dragged`, `Pinned` |

Every dimension carries its `value` (`value_degrees` for `Angle`), an optional
driving `expression: Option<String>`, and `reference: bool` (a reference
dimension is displayed but does not drive the solve).

```rust
pub enum SolveStatus {
    /// Never solved; the engine solves it on the next rebuild. The default.
    Unsolved,
    FullyConstrained,
    UnderConstrained { dof: u32 },
    /// `conflicts` are indices into the sketch's constraint list.
    OverConstrained { conflicts: Vec<u32> },
    SolveFailed { reason: String },
}

pub struct SolvedSketch {
    pub positions: HashMap<u32, (f64, f64)>,
    pub radii: HashMap<u32, f64>,          // circles, keyed by entity id
    pub profiles: Vec<ClosedProfile>,
    pub status: SolveStatus,
    pub report: SketchSolveReport,         // residuals, conflicts, redundancy, freedoms
}

pub struct ClosedProfile {
    pub entity_ids: Vec<u32>,
    pub is_outer: bool,                    // counter-clockwise outer loop, or a hole
    pub vertex_ids: Vec<u32>,              // point ids in winding order
    pub circle: Option<CircleProfile>,     // a standalone circle
    pub spline_segments: Vec<SplineSegment>,
    pub arc_segments: Vec<ArcSegment>,
}

pub struct CircleProfile { pub center_u: f64, pub center_v: f64, pub radius: f64 }
```

The solver's entry points (`sketch-solver`):

```rust
pub fn solve_sketch(sketch: &Sketch) -> SolvedSketch;

pub fn extract_profiles(
    entities: &[SketchEntity],
    positions: &HashMap<u32, (f64, f64)>,
) -> Vec<ClosedProfile>;
```

---

## D) Feature tree

**Defined in:** `feature-engine` (`types.rs`, `lib.rs`)
**Used by:** wasm-bridge, file-format, the app

```rust
pub struct FeatureTree {
    pub features: Vec<Feature>,
    /// Features after this index are skipped during rebuild. None: all active.
    pub active_index: Option<usize>,
    pub body_names: BodyNames,              // body id → user-assigned name
    pub parameters: Vec<DesignParameter>,   // named design variables
    pub provenance: ProvenanceTable,        // feature id → who created it
    pub names: NameTable,                   // entity name → pinned reference
    pub materials: Vec<Material>,
    pub body_materials: BodyMaterials,      // body id → material name
    /// Unknown keys, preserved across load and save.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

pub struct Feature {
    pub id: Uuid,
    pub name: String,
    pub operation: Operation,
    pub suppressed: bool,
    pub references: Vec<GeomRef>,
}

#[serde(tag = "type")]
pub enum Operation {
    Sketch          { sketch: Sketch },
    Sketch3d        { sketch: waffle_types::sketch3d::Sketch3d },
    Extrude         { params: ExtrudeParams },
    Revolve         { params: RevolveParams },
    Fillet          { params: FilletParams },
    Chamfer         { params: ChamferParams },
    Shell           { params: ShellParams },
    BooleanCombine  { params: BooleanParams },
    DatumPlane      { params: DatumPlaneParams },
    ImportedBody    { params: ImportedBodyParams },
    MateConnector   { params: MateConnectorParams },
    PatternCircular { params: PatternCircularParams },
    PatternLinear   { params: PatternLinearParams },
    PatternMirror   { params: PatternMirrorParams },
    Pipe            { params: PipeParams },
    Sweep           { params: SweepParams },
    Script          { params: ScriptParams },
    UnionAll        { params: UnionAllParams },
    BoardShape      { params: BoardShapeParams },
    /// An operation tag this build does not know. Kept verbatim and re-emitted
    /// on save; rebuilding it is an `EngineError::UnsupportedOperation`.
    #[serde(untagged)]
    Unknown(serde_json::Value),
}
```

`OPERATION_TAGS` lists the tags this build can rebuild. `Fillet`, `Chamfer` and
`Shell` are in the tree vocabulary, but the kernel answers them with
`NotSupported`.

The most-used parameter types:

```rust
pub struct ExtrudeParams {
    pub sketch_id: Uuid,
    pub profile_index: usize,
    pub profile_entity_ids: Option<Vec<u32>>,
    pub depth: f64,
    pub depth_expr: Option<String>,
    pub direction: Option<[f64; 3]>,
    pub symmetric: bool,
    pub cut: bool,
    pub merge: bool,                        // serde default: true
    pub target_body: Option<GeomRef>,
    pub depth_mode: DepthMode,              // serde default: Blind
    pub second_direction: Option<SecondDirection>,
    pub region: Option<waffle_types::Region>,
    pub regions: Vec<waffle_types::Region>,
    pub combine: Option<CombineMode>,
    pub targets: Option<Vec<GeomRef>>,
}

pub enum DepthMode { Blind, ThroughAll, UpTo { reference: GeomRef } }

pub enum SecondDirection {
    Symmetric,
    Blind { depth: f64, depth_expr: Option<String> },
    ThroughAll,
    UpTo { reference: GeomRef },
}

pub enum CombineMode { NewBody, Add, Cut, Intersect }

pub struct RevolveParams {
    pub sketch_id: Uuid,
    pub profile_index: usize,
    pub profile_entity_ids: Option<Vec<u32>>,
    pub axis_origin: [f64; 3],
    pub axis_origin_expr: Option<[Option<String>; 3]>,
    pub axis_direction: [f64; 3],
    pub angle: f64,                         // degrees
    pub angle_expr: Option<String>,
    pub cut: bool,
    pub merge: bool,                        // serde default: true
    pub combine: Option<CombineMode>,
    pub targets: Option<Vec<GeomRef>>,
}

pub struct BooleanParams {
    pub body_a: GeomRef,
    pub body_b: GeomRef,
    pub operation: BooleanOp,
}

pub enum BooleanOp { Union, Subtract, Intersect }

pub struct DesignParameter {
    pub id: Uuid,
    pub name: String,
    pub expression: String,
    pub value: f64,                         // evaluated
    pub error: Option<String>,
    pub unit: Option<Dimension>,
    pub comment: Option<String>,
}
```

`AxisRef` and `PlaneDefinition` are tagged with `#[serde(tag = "method")]`
rather than `"type"`. The remaining parameter types are in `types.rs` and, for
`BoardShapeParams`, `board_shape.rs`.

### The engine

`feature_engine::Engine` holds one part's tree, the cached `OpResult` of each
feature, and the outcome of the last rebuild. A call that changes geometry
takes the kernel and rebuilds from the point of change:

```rust
impl Engine {
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
    pub fn rebuild_from_scratch(&mut self, kb: &mut dyn KernelBundle);
    pub fn get_result(&self, feature_id: Uuid) -> Option<&OpResult>;
}
```

After a rebuild, the public fields carry the result: `feature_results`,
`errors` and the typed `feature_errors`, `warnings` and `feature_warnings`,
`consumed_features` (features whose body a later boolean consumed and which
are therefore not rendered), and `connectors`.

```rust
pub struct FeatureError {
    pub feature_id: Uuid,
    pub kind: ErrorKind,       // machine-readable class; hosts branch on this
    pub message: String,       // human text; nothing should parse it
}
```

Assemblies (`assembly.rs`: `AssemblyTree`), drawings (`drawing.rs`: `Drawing`),
entity names (`names.rs`) and scripts (`script/`) have their own modules in
`feature-engine`; their persisted shapes are specified in `docs/FILE_FORMAT.md`.

---

## E) Bridge protocol

**Defined in:** `wasm-bridge` (`messages.rs`, `dispatch.rs`, `process.rs`, `wasm_api.rs`, `tools/`)
**Used by:** the app's worker, waffle-host

A host sends one `UiToEngine` and receives one `EngineToUi`. Both are
internally tagged enums (`#[serde(tag = "type")]`) carried as JSON.

```rust
// Any host:
pub fn dispatch(state: &mut EngineState, msg: UiToEngine, kb: &mut dyn KernelBundle) -> EngineToUi;

// The browser worker (wasm_api.rs):
#[wasm_bindgen] pub fn init();
#[wasm_bindgen] pub fn process_message(json_input: &str) -> String;
#[wasm_bindgen] pub fn set_progress_sink(sink: js_sys::Function);
```

`EngineState` holds the engine of the active tab, the active sketch, selection
and hover, and the `DocumentSession` (document metadata, the tab list, every
inactive tab's tree, per-tab undo).

### `UiToEngine` variants

| Group | Variants |
|---|---|
| Sketching | `BeginSketch`, `AddSketchEntity`, `AddConstraint`, `SolveSketch`, `FinishSketch`, `ComputeRegions`, `ApplySketchOps`, `QuerySketch` |
| Features | `AddFeature`, `EditFeature`, `DeleteFeature`, `SuppressFeature`, `ReorderFeature`, `RenameFeature`, `RenameBody`, `SetRollbackIndex` |
| History | `Undo`, `Redo` |
| Selection | `SelectEntity`, `HoverEntity` |
| Entity names | `SetEntityName`, `ClearEntityName`, `QueryEntityNames` |
| Parameters | `SetParameters`, `SetDocumentParameters`, `EvaluateExpression` |
| Document and tabs | `NewDocument`, `SaveProject`, `LoadProject`, `SaveDocument`, `SetDocumentMeta`, `SetDisplaySettings`, `EditMaterials`, `SwitchTab`, `AddTab`, `CloseTab`, `RenameTab`, `MoveTab` |
| Sources and scripts | `ProvideSource`, `UpdateSourceEntry`, `ListSources`, `ReadSource`, `RebaseSources`, `ListSourceTabs`, `AddScriptSource`, `SetScriptSource`, `CheckScript` |
| Import | `ImportStep`, `ImportStepFromLocator`, `ImportKicad`, `LinkKicadFromLocator` |
| Assemblies | `OpenAssembly`, `EditAssembly`, `OpenPartInContext`, `ProbeConnectorRef` |
| Drawings | `OpenDrawing`, `EditDrawing`, `DrawingEdit`, `ProbeDrawingView` |
| Inspection | `QueryEntityMeta`, `ListFaces`, `ListEntities` |
| Measurement | `MeasureBody`, `MeasureDistance`, `MeasureInterference`, `MeasureMass`, `MeasureSection`, `MeasureThickness` |
| Export | `ExportStep`, `ExportDxf`, `ExportStl`, `ExportBodyStl` |
| Generators | `GenerateGearPreview`, `GenerateGearProfile`, `GenerateSprocketPreview`, `GenerateSprocketProfile`, `GeneratePlanetary`, `GeneratePlanetaryPreview` |
| Agent tools | `Tool` |

The core feature messages:

```rust
// `Provenance` here is `feature_engine::types::Provenance`: who or what
// created the feature (user, agent, import, derived). Not section B's type.
AddFeature  { operation: Operation, provenance: Option<Provenance> },
EditFeature { feature_id: Uuid, operation: Operation, provenance: Option<Provenance> },
DeleteFeature { feature_id: Uuid },

FinishSketch {
    solved_positions: HashMap<u32, (f64, f64)>,
    solved_profiles: Vec<ClosedProfile>,
    plane_origin: [f64; 3],
    plane_normal: [f64; 3],
    plane_x_axis: Option<[f64; 3]>,
    entities: Vec<SketchEntity>,
    constraints: Vec<SketchConstraint>,
    projected: Vec<ProjectedEntity>,
    provenance: Option<Provenance>,
},
```

### `EngineToUi` variants

| Group | Variants |
|---|---|
| Model | `ModelUpdated`, `ProjectLoaded`, `Error` |
| Sketching | `SketchSolved`, `RegionsComputed`, `SketchOpsApplied`, `SketchQueried` |
| Selection | `HoverChanged`, `SelectionChanged` |
| Inspection | `EntityMeta`, `FacesListed`, `EntitiesListed`, `EntityNamesListed` |
| Measurement | `BodyMeasured`, `DistanceMeasured`, `InterferenceMeasured`, `MassMeasured`, `SectionMeasured`, `ThicknessMeasured` |
| Document and sources | `SaveReady`, `SourcesListed`, `SourceContent`, `SourceTabsListed`, `ScriptSourceAdded`, `ScriptChecked` |
| Assemblies and drawings | `ConnectorRefProbed`, `DrawingViewProbed` |
| Export | `ExportReady`, `DxfExportReady`, `StlExportReady` |
| Generators | `GearPreviewGenerated`, `GearProfileGenerated`, `SprocketPreviewGenerated`, `SprocketProfileGenerated`, `PlanetaryGenerated`, `PlanetaryPreviewGenerated` |
| Parameters | `ExpressionEvaluated` |
| Agent tools | `ToolResult` |

A command that changes the model answers `ModelUpdated`:

```rust
ModelUpdated {
    feature_id: Option<Uuid>,                 // the feature the command created or edited
    feature_tree: FeatureTree,
    meshes: Vec<RenderMesh>,                  // always empty; see section F
    edges: Vec<EdgeRenderData>,               // always empty; see section F
    errors: Vec<(Uuid, String)>,
    feature_errors: Vec<FeatureError>,
    warnings: Vec<String>,
    feature_warnings: Vec<(Uuid, String)>,
    consumed_features: Vec<Uuid>,
    preview_mesh: Option<PreviewMesh>,        // decimated thumbnail
    sources: Vec<SourceStatus>,
    assembly: Option<AssemblyStatus>,         // while an assembly tab is open
    drawing: Option<DrawingStatus>,           // while a drawing tab is open
    context: Option<ContextStatus>,           // while a part is open in an assembly's context
    connectors: Vec<PartConnectorInfo>,       // the open part's mate connectors
    document: Option<DocumentInfo>,
},

Error {
    message: String,
    feature_id: Option<Uuid>,
    kind: Option<ErrorKind>,
},
```

### Agent tools

Agent tools are implemented once, in `wasm_bridge::tools`, and reached from any
host:

```rust
// As a message:
UiToEngine::Tool { name: String, arguments: serde_json::Value, context: Option<serde_json::Value> }
EngineToUi::ToolResult { result: ToolResult, model: Option<Box<EngineToUi>> }

// Directly:
pub fn execute_tool(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    name: &str,
    arguments: &Value,
    context: Option<&Value>,
) -> ToolResult;

/// Field names are the MCP wire names.
pub struct ToolResult {
    pub content: Vec<Value>,
    #[serde(rename = "structuredContent")] pub structured_content: Value,
    #[serde(rename = "isError")] pub is_error: bool,
    /// A file the host must deliver to the user. Not part of the MCP result.
    pub download: Option<ExportFile>,
}
```

`tools::MIGRATED` lists the tool names `execute_tool` implements. A name it
does not implement answers a `ToolUnavailable` error. Tools that belong to the
host (the viewport, selection, document storage) are implemented by the host,
not here.

---

## F) Render data

**Produced by:** the kernel (`Kernel::tessellate`, `Kernel::extract_edges`)
**Used by:** wasm-bridge, the app's viewport

At the kernel boundary a mesh is keyed by `KernelId`:

```rust
pub struct RenderMesh {
    pub vertices: Vec<f32>,       // [x0, y0, z0, x1, y1, z1, ...]
    pub normals: Vec<f32>,        // parallel to vertices
    pub indices: Vec<u32>,        // triangles
    pub face_ranges: Vec<FaceRange>,
}

/// A contiguous range of `indices` belonging to one face.
pub struct FaceRange {
    pub face_id: KernelId,
    pub start_index: u32,
    pub end_index: u32,
}

pub struct EdgeRenderData {
    pub vertices: Vec<f32>,       // polyline points, [x0, y0, z0, ...]
    pub edge_ranges: Vec<EdgeRange>,
}

pub struct EdgeRange {
    pub edge_id: KernelId,
    pub start_vertex: u32,
    pub end_vertex: u32,
    /// Present for a circular edge, so a consumer can build a true arc.
    pub curve: Option<EdgeCurve>,
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EdgeCurve {
    Circle { center: [f64; 3], normal: [f64; 3], radius: f64 },
    Arc    { center: [f64; 3], normal: [f64; 3], radius: f64 },
}
```

Meshes do not cross to JavaScript inside `ModelUpdated`. After a model update
the worker reads each renderable body through accessors indexed by a flat body
index (`wasm_api.rs`):

| Accessor | Returns |
|---|---|
| `get_body_count()` | number of renderable bodies |
| `get_body_metadata()` | JSON array, one entry per body |
| `get_body_vertices(i)`, `get_body_normals(i)` | `Float32Array` |
| `get_body_indices(i)` | `Uint32Array` |
| `get_body_face_data(i)` | JSON array of face entries |
| `get_body_edge_vertices(i)` | `Float32Array` |
| `get_body_edge_data(i)` | JSON array of edge entries |

A typed-array view points into WASM memory and is invalidated by any memory
growth, so the worker copies or transfers it immediately.

The bridge replaces kernel ids with persistent references before the data
leaves it. A face entry is

```json
{ "geom_ref": GeomRef, "start_index": u32, "end_index": u32,
  "created_by_feature": "uuid" | null,
  "plane": { "origin": [x, y, z], "normal": [x, y, z] } }
```

where `plane` is present for planar faces and gives the exact (f64) plane a
sketch on that face must use. An edge entry is

```json
{ "geom_ref": GeomRef, "start_index": u32, "end_index": u32, "curve": EdgeCurve | null }
```

---

## G) Kernel contract

**Defined in:** `waffle_types::kernel` (`traits.rs`, `types.rs`, `measure.rs`, `projection.rs`, `import.rs`, `analytic.rs`, `units.rs`)
**Implemented by:** `kernel_v2::KernelV2Adapter`; `waffle_types::kernel::MockKernel` (feature `mock-kernel`), a deterministic test double with synthetic topology
**Used by:** modeling-ops, feature-engine, wasm-bridge, file-format, test-harness

Four traits. `Kernel` constructs and modifies solids; `KernelIntrospect` reads
topology and geometry; `KernelMeasure` answers distance, interference, mass
and thickness questions; `KernelProjection` produces drawing views and
sections. A method with a default body returns `KernelError::NotSupported`
(or `None` / empty) unless the implementor overrides it.

### `Kernel`

```rust
pub trait Kernel {
    // --- construction ---

    /// Create planar faces from closed sketch profiles. The returned ids are
    /// staged profiles, consumed by `extrude_face` / `revolve_face`.
    fn make_faces_from_profiles(
        &mut self,
        profiles: &[ClosedProfile],
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        plane_x_axis: [f64; 3],
        positions: &HashMap<u32, (f64, f64)>,
    ) -> Result<Vec<KernelId>, KernelError>;

    /// Create one planar face from an explicit region: an outer loop plus
    /// hole loops in sketch (u, v) coordinates.
    fn make_face_from_region(
        &mut self,
        region: &crate::Region,
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        plane_x_axis: [f64; 3],
    ) -> Result<KernelId, KernelError> { /* default: NotSupported */ }

    /// Extrude a planar face along a direction vector.
    fn extrude_face(
        &mut self,
        face: KernelId,
        direction: [f64; 3],
        depth: f64,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Revolve a planar face around an axis. `angle` is in degrees.
    fn revolve_face(
        &mut self,
        face: KernelId,
        axis_origin: [f64; 3],
        axis_direction: [f64; 3],
        angle: f64,
    ) -> Result<KernelSolidHandle, KernelError>;

    /// Sweep a circle (hollow when `inner_radius` is `Some`) along an open,
    /// tangent-continuous chain of lines and arcs in the sketch plane.
    fn pipe(
        &mut self,
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        plane_x_axis: [f64; 3],
        path: &[PipePathSegment],
        radius: f64,
        inner_radius: Option<f64>,
    ) -> Result<KernelSolidHandle, KernelError> { /* default: NotSupported */ }

    /// Sweep a planar polygon section along a chain of lines and arcs in
    /// world coordinates.
    fn sweep(
        &mut self,
        section: &SweepSection,
        path: &crate::sketch3d::Chain3d,
    ) -> Result<KernelSolidHandle, KernelError> { /* default: NotSupported */ }

    // --- booleans ---

    fn boolean_union(&mut self, a: &KernelSolidHandle, b: &KernelSolidHandle)
        -> Result<KernelSolidHandle, KernelError>;
    fn boolean_subtract(&mut self, a: &KernelSolidHandle, b: &KernelSolidHandle)
        -> Result<KernelSolidHandle, KernelError>;
    fn boolean_intersect(&mut self, a: &KernelSolidHandle, b: &KernelSolidHandle)
        -> Result<KernelSolidHandle, KernelError>;

    /// The same operations, returning every disjoint body of the result.
    /// Default: the single-body result wrapped in a one-element vec.
    fn boolean_union_multi(&mut self, a: &KernelSolidHandle, b: &KernelSolidHandle)
        -> Result<Vec<KernelSolidHandle>, KernelError> { /* default */ }
    fn boolean_subtract_multi(&mut self, a: &KernelSolidHandle, b: &KernelSolidHandle)
        -> Result<Vec<KernelSolidHandle>, KernelError> { /* default */ }
    fn boolean_intersect_multi(&mut self, a: &KernelSolidHandle, b: &KernelSolidHandle)
        -> Result<Vec<KernelSolidHandle>, KernelError> { /* default */ }

    // --- copies ---

    /// Rigid copy: a new solid moved by `placement`. Exact on analytic geometry.
    fn transform_body(&mut self, solid: &KernelSolidHandle, placement: &RigidPlacement)
        -> Result<KernelSolidHandle, KernelError> { /* default: NotSupported */ }

    /// Mirrored copy: a new solid reflected through `plane`.
    fn mirror_body(&mut self, solid: &KernelSolidHandle, plane: &MirrorPlane)
        -> Result<KernelSolidHandle, KernelError> { /* default: NotSupported */ }

    // --- edge and face modification ---

    fn fillet_edges(&mut self, solid: &KernelSolidHandle, edges: &[KernelId], radius: f64)
        -> Result<KernelSolidHandle, KernelError>;
    fn chamfer_edges(&mut self, solid: &KernelSolidHandle, edges: &[KernelId], distance: f64)
        -> Result<KernelSolidHandle, KernelError>;
    fn shell(&mut self, solid: &KernelSolidHandle, faces_to_remove: &[KernelId], thickness: f64)
        -> Result<KernelSolidHandle, KernelError>;

    // --- import and export ---

    /// Ingest a mesh-backed imported body, already placed in world coordinates.
    fn import_body(&mut self, data: &ImportedBodyData)
        -> Result<KernelSolidHandle, KernelError> { /* default: NotSupported */ }

    /// Ingest an exact analytic shell as a first-class solid. A shell the
    /// kernel cannot carry exactly is refused with a typed error.
    fn import_analytic_shell(&mut self, shell: &AnalyticShellData)
        -> Result<KernelSolidHandle, KernelError> { /* default: NotSupported */ }

    /// Export one solid as ISO 10303-21 (STEP AP214) text.
    fn export_step(&mut self, solid: &KernelSolidHandle, file_name: &str)
        -> Result<String, KernelError> { /* default: NotSupported */ }

    /// Export several named, optionally placed solids into one STEP file.
    fn export_step_bodies(&mut self, bodies: &[StepExportBody], file_name: &str)
        -> Result<String, KernelError> { /* default: NotSupported */ }

    // --- rendering ---

    fn tessellate(&mut self, solid: &KernelSolidHandle, tolerance: f64)
        -> Result<RenderMesh, KernelError>;
    fn extract_edges(&mut self, solid: &KernelSolidHandle, tolerance: f64)
        -> Result<EdgeRenderData, KernelError>;

    // --- persistent identity ---

    /// Install the seed that subsequent construction calls stamp their new
    /// faces' persistent ids from; returns the seed it replaced. The caller
    /// sets it before one geometry-creating step and restores it afterwards.
    fn set_construct_seed(&mut self, seed: Option<ConstructSeed>)
        -> Option<ConstructSeed> { /* default: None */ }
}
```

### `KernelIntrospect`

```rust
pub trait KernelIntrospect {
    fn list_faces(&self, solid: &KernelSolidHandle) -> Vec<KernelId>;
    fn list_edges(&self, solid: &KernelSolidHandle) -> Vec<KernelId>;
    fn list_vertices(&self, solid: &KernelSolidHandle) -> Vec<KernelId>;

    fn face_edges(&self, face: KernelId) -> Vec<KernelId>;
    fn edge_faces(&self, edge: KernelId) -> Vec<KernelId>;
    fn edge_vertices(&self, edge: KernelId) -> (KernelId, KernelId);
    fn face_neighbors(&self, face: KernelId) -> Vec<KernelId>;

    /// The edge as a 3D polyline from start vertex to end vertex. Default:
    /// the two endpoints.
    fn edge_polyline(&self, edge: KernelId) -> Vec<[f64; 3]> { /* default */ }

    fn compute_signature(&self, entity: KernelId, kind: TopoKind) -> TopoSignature;
    fn compute_all_signatures(&self, solid: &KernelSolidHandle, kind: TopoKind)
        -> Vec<(KernelId, TopoSignature)>;

    /// Whether the solid is exact (analytic surfaces) rather than a
    /// mesh-backed imported body. Default: true.
    fn solid_is_exact(&self, solid: &KernelSolidHandle) -> bool { /* default */ }

    /// Volume in m³ and surface area in m², exactly from the B-Rep. A kernel
    /// that cannot integrate a solid answers an error, never an approximation.
    fn solid_volume(&self, solid: &KernelSolidHandle)
        -> Result<f64, KernelError> { /* default: NotSupported */ }
    fn solid_surface_area(&self, solid: &KernelSolidHandle)
        -> Result<f64, KernelError> { /* default: NotSupported */ }

    /// A conservative axis-aligned bounding box `(lo, hi)`. None when the
    /// kernel cannot bound the solid; never a box that under-covers.
    fn solid_aabb(&self, solid: &KernelSolidHandle)
        -> Option<([f64; 3], [f64; 3])> { /* default: None */ }

    /// `(min, max)` of the solid along `direction`. Default: from the vertices.
    fn solid_extent_along(&self, solid: &KernelSolidHandle, direction: [f64; 3])
        -> Option<(f64, f64)> { /* default */ }

    /// Per face, the largest distance from its boundary to its own surface.
    fn surface_conformance(&self, solid: &KernelSolidHandle)
        -> Result<SurfaceConformance, KernelError> { /* default: NotSupported */ }

    /// Per face, the largest distance from its render mesh to its surface.
    fn mesh_conformance(&self, solid: &KernelSolidHandle)
        -> Result<MeshConformance, KernelError> { /* default: NotSupported */ }

    /// The analytic axis of a cylindrical, conical, toroidal or spherical
    /// face, or of a circular or elliptical edge. None for anything else.
    fn entity_axis(&self, entity: KernelId, kind: TopoKind)
        -> Option<EntityAxis> { /* default: None */ }

    /// A face's persistent id and its lineage root.
    fn face_provenance(&self, face: KernelId)
        -> Option<FaceProvenance> { /* default: None */ }

    /// Persistent identity of a face, edge or vertex. None when the kernel
    /// cannot derive one; it never invents an id.
    fn entity_pid(&self, entity: KernelId, kind: TopoKind)
        -> Option<EntityPid> { /* default: None */ }

    /// Every entity of `kind` in `solid` with its persistent identity. An
    /// empty answer means no identity is available, not that entities are gone.
    fn all_entity_pids(&self, solid: &KernelSolidHandle, kind: TopoKind)
        -> Vec<(KernelId, EntityPid)> { /* default: empty */ }
}
```

### `KernelMeasure` and `KernelProjection`

Every method defaults to `NotSupported`.

```rust
pub trait KernelMeasure {
    fn distance(&self, a: &MeasureEntity, b: &MeasureEntity, opts: &DistanceOpts)
        -> Result<Distance, KernelError>;
    fn interference(&self, a: &KernelSolidHandle, b: &KernelSolidHandle)
        -> Result<Interference, KernelError>;
    fn mass_properties(&self, solid: &KernelSolidHandle, density: Option<f64>)
        -> Result<MassProperties, KernelError>;
    fn edge_length(&self, edge: KernelId) -> Result<EdgeLength, KernelError>;
    fn thickness(&self, solid: &KernelSolidHandle, opts: &ThicknessOpts)
        -> Result<Thickness, KernelError>;
}

pub trait KernelProjection {
    fn project(&self, solid: &KernelSolidHandle, view: &ViewFrame, opts: &ProjectOpts)
        -> Result<ViewGeometry, KernelError>;
    fn project_bodies(&self, bodies: &[ProjectionBody], view: &ViewFrame, opts: &ProjectOpts)
        -> Result<ViewGeometry, KernelError>;
    fn section_with_plane(&mut self, solid: &KernelSolidHandle,
                          plane_origin: [f64; 3], plane_normal: [f64; 3])
        -> Result<SectionResult, KernelError>;
    fn release_section_cut(&mut self, cut: &KernelSolidHandle) -> Result<(), KernelError>;
    fn export_dxf_with_declines(&self, bodies: &[ProjectionBody], view: &ViewFrame,
                                opts: &ProjectOpts)
        -> Result<(String, ProjectionDeclines), KernelError>;
    /// Default: `export_dxf_with_declines`, dropping the declines.
    fn export_dxf(&self, bodies: &[ProjectionBody], view: &ViewFrame, opts: &ProjectOpts)
        -> Result<String, KernelError>;
}
```

### What `KernelV2Adapter` supports

`KernelV2Adapter` implements all four traits in full; no method is left at a
`NotSupported` default. The boundaries:

| Method | Behaviour |
|---|---|
| `make_faces_from_profiles` | Polygons, circles, exact arcs, splines through their chord polygon, and holed regions. An arc or spline profile with no `vertex_ids` polygon is `NotSupported`. |
| `extrude_face` | Supported. A circle profile becomes a cylinder. |
| `revolve_face` | Full and partial turns, producing cylinders, cones, tori and spheres. A holed profile is `NotSupported`; an axis that touches or crosses the profile, or an out-of-range angle, is an error. |
| `pipe` | Open tangent-continuous chains. A closed loop is `NotSupported`. |
| `sweep` | Polygon sections. A holed or arc-bearing section, a section edge oblique to a bend's axis, and a twisted 3D ring are `NotSupported`. |
| `boolean_*` | Solids bounded by planes, cylinders, cones, spheres and tori. Coplanar inputs that Stage 0 cannot resolve are `NotSupported`; an imported mesh-backed operand is `NotSupported`; a pipeline stage that cannot certify its result is `BooleanFailed` carrying the stage's message. |
| `boolean_*_multi` | Split the result into its spatially disjoint bodies. |
| `fillet_edges`, `chamfer_edges`, `shell` | Always `NotSupported`. |
| `transform_body`, `mirror_body` | Supported; a placement that is not a proper rotation is an error. |
| `import_body` | Mesh-backed body: renders, introspects and has signatures. |
| `import_analytic_shell` | Exact solid, or a typed refusal. |
| `export_step`, `export_step_bodies` | AP214 with exact surfaces and curves. A mesh-backed body is `NotSupported`. |
| `tessellate`, `extract_edges` | Supported. The `tolerance` argument is ignored; the kernel uses its own chord bound. |

Arguments are checked before any work: a NaN or infinite coordinate, distance
or angle is `KernelError::InvalidInput`.

### Shared types (`types.rs`)

```rust
/// Opaque handle to a solid. `from_raw` / `raw` are for `Kernel` implementors.
pub struct KernelSolidHandle(/* u64 */);

/// Session-scoped entity id. In `KernelV2Adapter` it is tag-encoded
/// (`tag << 40 | index`): 1 vertex, 2 edge, 3 face, 4 staged profile,
/// 5–7 the face, edge and vertex of an imported mesh-backed body.
pub struct KernelId(pub u64);

/// Persistent identity of a face: stable through chained booleans.
pub struct FaceProvenance {
    pub pid: u64,        // the face's own persistent id
    pub root_pid: u64,   // where its geometry was introduced
}

/// Persistent identity of a face, edge or vertex. For edges and vertices
/// `root_pid == pid`.
pub struct EntityPid {
    pub pid: u64,
    pub root_pid: u64,
}

/// The stable 128-bit name of the step about to create geometry (in practice
/// a feature's `Uuid`). Opaque to the kernel.
pub struct ConstructSeed {
    pub origin: [u64; 2],
}

pub enum KernelError {
    BooleanFailed { reason: String },
    /// A correct boolean that leaves no material (for example a subtract
    /// whose tool engulfs the target). A typed outcome, not a failure.
    BooleanEmptyResult,
    FilletFailed { reason: String },
    ShellFailed { reason: String },
    TessellationFailed { reason: String },
    EntityNotFound { id: KernelId },
    /// The capability-boundary marker.
    NotSupported { operation: String },
    /// A caller-supplied argument the kernel refuses before doing any work.
    InvalidInput { argument: String, reason: String },
    Other { message: String },
}

/// A rigid motion `p' = R·p + t`. `rotation` is row-major, orthonormal,
/// determinant +1.
pub struct RigidPlacement {
    pub translation: [f64; 3],
    pub rotation: [[f64; 3]; 3],
}

/// Reflection through the plane through `point` with normal `normal`.
pub struct MirrorPlane {
    pub point: [f64; 3],
    pub normal: [f64; 3],
}

pub struct StepExportBody {
    pub handle: KernelSolidHandle,
    pub name: String,
    pub placement: Option<RigidPlacement>,   // None: identity
}

/// One segment of a pipe path in sketch (u, v) coordinates.
pub enum PipePathSegment {
    Line { a: (f64, f64), b: (f64, f64) },
    Arc  { a: (f64, f64), b: (f64, f64), center: (f64, f64), radius: f64, ccw: bool },
}

/// The planar polygon a sweep carries along its path.
pub struct SweepSection {
    pub plane_origin: [f64; 3],
    pub plane_normal: [f64; 3],
    pub plane_x_axis: [f64; 3],
    pub outer: Vec<(f64, f64)>,           // counter-clockwise, not closed
    pub holes: Vec<Vec<(f64, f64)>>,
}

pub struct EntityAxis {
    pub kind: AxisKind,          // Cylindrical | Conical | Toroidal | Spherical | Circular | Elliptical
    pub origin: [f64; 3],        // a point on the axis
    pub direction: [f64; 3],     // unit
    pub radius: Option<f64>,
}
```

`RenderMesh`, `EdgeRenderData` and their ranges are in section F.

### Imported bodies (`import.rs`, `analytic.rs`)

Two tiers. Neither is serialized; the persisted artifact is the STEP text,
re-parsed on rebuild.

```rust
// Mesh tier: Kernel::import_body
pub struct ImportedBodyData  { pub source_name: String, pub shells: Vec<ImportedShellData> }
pub struct ImportedShellData { pub faces: Vec<ImportedFaceData>, pub edges: Vec<ImportedEdgeData> }
pub struct ImportedFaceData {
    pub surface: ImportedSurface,   // Plane { origin, normal } | Cylindrical | Conical | Spherical | Toroidal | Freeform
    pub positions: Vec<f64>,
    pub normals: Vec<f64>,
    pub indices: Vec<u32>,
    pub edge_indices: Vec<u32>,
}
pub struct ImportedEdgeData  { pub polyline: Vec<[f64; 3]> }

// Exact tier: Kernel::import_analytic_shell
pub struct AnalyticShellData {
    pub vertices: Vec<Point3>,
    pub edges: Vec<AnalyticEdge>,
    pub faces: Vec<AnalyticFace>,
}
```

### Tolerances (`units.rs`)

The canonical tolerances are defined once, in `cad-primitives`, and re-exported
here:

| Constant | Value | Meaning |
|---|---|---|
| `MIN_FEATURE_SIZE` | 1e-6 m | Degeneracy floor |
| `TAU_MODEL` | 1e-7 m | Model coincidence |
| `TAU_WORK` | 1e-12 m | Working precision floor |
