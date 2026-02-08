# 07 — Modeling Ops: Interfaces

## Types This Crate IMPLEMENTS

| Type | Role |
|------|------|
| `OpResult` | Complete operation result with outputs + provenance + diagnostics |
| `Provenance` | Created/deleted/modified entity tracking with roles |
| `EntityRecord` | Entity with KernelId + kind + signature |
| `Rewrite` | Modified entity record (before/after) |
| `Diagnostics` | Warnings and timing |
| Topology diff utility | Before/after → Provenance |

## Types This Crate CONSUMES

| Type | Source | Purpose |
|------|--------|---------|
| `ExtrudeParams` | INTERFACES.md | Extrude operation parameters |
| `RevolveParams` | INTERFACES.md | Revolve operation parameters |
| `FilletParams` | INTERFACES.md | Fillet operation parameters |
| `ChamferParams` | INTERFACES.md | Chamfer operation parameters |
| `ShellParams` | INTERFACES.md | Shell operation parameters |
| `BooleanParams` | INTERFACES.md | Boolean combine parameters |
| `Kernel` trait | kernel-fork | Geometry operations |
| `KernelIntrospect` trait | kernel-fork | Topology queries |
| `KernelSolidHandle` | kernel-fork | Solid handle |
| `KernelId` | kernel-fork | Entity identifier |
| `TopoKind` | INTERFACES.md | Entity kind |
| `TopoSignature` | INTERFACES.md | Geometric signature |
| `Role` | INTERFACES.md | Semantic role for entities |
| `GeomRef` | INTERFACES.md | Geometry references (in params) |

## Public API

```rust
/// Execute an extrude operation and return full OpResult.
pub fn execute_extrude(
    params: &ExtrudeParams,
    kernel: &mut dyn Kernel,
    introspect: &dyn KernelIntrospect,
    input_solid: Option<&KernelSolidHandle>,
) -> Result<OpResult, OpError>;

/// Execute a revolve operation.
pub fn execute_revolve(
    params: &RevolveParams,
    kernel: &mut dyn Kernel,
    introspect: &dyn KernelIntrospect,
    input_solid: Option<&KernelSolidHandle>,
) -> Result<OpResult, OpError>;

/// Execute a fillet operation.
pub fn execute_fillet(
    params: &FilletParams,
    kernel: &mut dyn Kernel,
    introspect: &dyn KernelIntrospect,
    solid: &KernelSolidHandle,
) -> Result<OpResult, OpError>;

/// ... similar for chamfer, shell, boolean_combine
```

## Notes

- Every operation function follows the same pattern: snapshot → execute → diff → assign roles → return OpResult.
- The diff utility is shared across all operations.
- Operations are stateless — they receive all needed context as parameters.
