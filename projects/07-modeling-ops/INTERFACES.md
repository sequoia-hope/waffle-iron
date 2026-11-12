# 07 — Modeling Ops: Interfaces

## Defines

```rust
pub struct OpResult {
    pub outputs: Vec<(OutputKey, BodyOutput)>,
    pub provenance: Provenance,
    pub diagnostics: Diagnostics,
}

pub struct BodyOutput {
    pub handle: KernelSolidHandle,      // runtime only
    pub mesh: Option<RenderMesh>,
    pub edges: Option<EdgeRenderData>,
}

pub struct Provenance {
    pub created: Vec<EntityRecord>,     // { kernel_id, kind, signature }
    pub deleted: Vec<EntityRecord>,
    pub modified: Vec<Rewrite>,         // { before, after, reason }
    pub role_assignments: Vec<(KernelId, Role)>,
}

pub struct Diagnostics {
    pub warnings: Vec<String>,
    pub kernel_time_ms: f64,
    pub tessellation_time_ms: f64,
    pub auto_union_fell_back: bool,
}

pub enum OpError { Kernel(KernelError), NoProfiles, InvalidParameter { reason } }
```

Also `KernelBundle`, `TopoSnapshot`, `DiffResult`, `BooleanKind`,
`PatternSeed`, `Instance`.

## Public functions

```rust
pub fn execute_extrude(kb: &mut dyn KernelBundle, face_id: KernelId,
    direction: [f64; 3], depth: f64,
    before_snapshot: Option<&TopoSnapshot>) -> Result<OpResult, OpError>;

pub fn execute_revolve(kb: &mut dyn KernelBundle, face_id: KernelId,
    axis_origin: [f64; 3], axis_direction: [f64; 3], angle: f64,
    before_snapshot: Option<&TopoSnapshot>) -> Result<OpResult, OpError>;

pub fn execute_boolean(kb: &mut dyn KernelBundle,
    body_a: &KernelSolidHandle, body_b: &KernelSolidHandle,
    kind: BooleanKind) -> Result<OpResult, OpError>;

pub fn execute_pattern_instances(kb: &mut dyn KernelBundle,
    seeds: &[PatternSeed], instances: &[Instance],
    skip: &[u32]) -> Result<OpResult, OpError>;
```

`execute_symmetric_extrude`, `execute_pipe`, `execute_sweep`,
`execute_import`, `execute_import_analytic`, `merge_import_results`,
`snapshot`, `signature_similarity` and `signature_match` are exported from
`src/lib.rs`.

## Consumes

`Kernel`, `KernelIntrospect`, `KernelProjection`, `KernelMeasure`,
`KernelId`, `KernelSolidHandle`, `RenderMesh`, `EdgeRenderData`, `OutputKey`,
`Role`, `TopoKind`, `TopoSignature` — all from `waffle-types`.
