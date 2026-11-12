# 10 — Assemblies: Interfaces

Types are in `feature_engine::assembly` unless noted. All persisted types
keep unknown keys in an `extra` map.

```rust
pub struct AssemblyTree {
    pub instances: Vec<Instance>,
    pub connectors: Vec<MateConnector>,
    pub mates: Vec<Mate>,
    pub placements: BTreeMap<Uuid, Transform>,   // derived
}

pub struct Transform { pub translation_m: [f64; 3], pub rotation_quat: [f64; 4] } // [x, y, z, w]

pub struct PartRef { pub source_id: Option<Uuid>, pub tab_id: String }

pub struct Instance {
    pub id: Uuid,
    pub name: String,
    pub source: PartRef,
    pub transform: Transform,
    pub fixed: bool,
    pub suppressed: bool,
    pub external_key: Option<String>,
    pub parameter_overrides: Option<BTreeMap<String, f64>>,
}

pub struct MateConnector {
    pub id: Uuid,
    pub name: String,
    pub instance_path: Vec<Uuid>,
    pub geom_ref: Option<GeomRef>,
    pub part_connector: Option<Uuid>,
    pub frame: Frame,
    pub anchor: AxialAnchor,
    pub flip_z: bool,
    pub rotation_deg: f64,
    pub offset_m: [f64; 3],
}

pub struct Mate {
    pub id: Uuid,
    pub name: String,
    pub kind: MateKind,          // Fastened | Revolute | Slider | Cylindrical | Planar | Ball | Unknown
    pub connectors: [Uuid; 2],
    pub suppressed: bool,
}

pub struct SolveResult {
    pub placements: BTreeMap<Uuid, Transform>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}
```

## Functions

```rust
// feature_engine::assembly_solver
pub fn solve_mates(tree: &AssemblyTree, frames: &HashMap<Uuid, Frame>, tol_m: f64) -> SolveResult;
```

`feature_engine::connector`: `resolve_connector_frame`,
`part_connector_frame`, `part_connectors`.
`feature_engine::context`: `EditContext`, `apply_context`.

## Bridge messages

`OpenAssembly`, `EditAssembly`, `ProbeConnectorRef`, `OpenPartInContext`,
`ListSourceTabs`. `ModelUpdated.assembly` carries the evaluation;
`ModelUpdated.context` carries the edit context.

## File format

`TabKind::Assembly { assembly }` in `file-format`; `GeomRef.scope`
(`waffle_types::RefScope`) for in-context references.
