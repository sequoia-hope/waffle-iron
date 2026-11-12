# 07 — Modeling Ops: Architecture

`crates/modeling-ops` wraps single kernel operations. Each function calls the
kernel through the `KernelBundle` trait and returns an `OpResult`: the output
bodies, the provenance of their topology, and diagnostics. `feature-engine`
calls these functions during a rebuild and uses the provenance to resolve
references.

The crate depends only on `waffle-types`. It holds no state.

## KernelBundle

```rust
pub trait KernelBundle: Kernel + KernelIntrospect + KernelProjection + KernelMeasure {
    fn as_introspect(&self) -> &dyn KernelIntrospect;
    fn as_measure(&self) -> &dyn KernelMeasure;
}
```

It has a blanket implementation for any type that implements the four traits,
so one `&mut dyn KernelBundle` gives a caller mutable kernel access and
read-only introspection of the same kernel. `kernel_v2::KernelV2Adapter` and
`MockKernel` both satisfy it.

## Operation pattern

1. Execute the kernel call.
2. Snapshot the result's faces, edges and vertices with their signatures
   (`diff::snapshot`).
3. Diff against the caller's "before" snapshot (`diff::diff`): entities only
   in "after" are created, entities only in "before" are deleted, and pairs
   whose signature similarity exceeds 0.7 are treated as the same entity.
4. Assign roles to the result's faces.
5. Return the `OpResult`. Meshes are left empty; the bridge tessellates.

`signature_similarity` is a weighted score over surface type (weight 3), area,
centroid, normal, edge length, and the axis data of a surface of revolution.

## Operations

| Function | Kernel call | Roles assigned |
|---|---|---|
| `execute_extrude` | `extrude_face` | `EndCapPositive`, `EndCapNegative`, `SideFace { index }` |
| `execute_symmetric_extrude` | `extrude_face`, centered on the sketch plane | as extrude |
| `execute_revolve` | `revolve_face` (angle in degrees) | `RevStartFace`, `RevEndFace` on the planar profile faces of a partial revolve; `SideFace { index }` otherwise |
| `execute_boolean` | union / subtract / intersect (`BooleanKind`) | `BooleanBodyAFace`, `BooleanBodyBFace` |
| `execute_pipe` | `pipe`: a round section along a 2D path, optional inner radius | `EndCapNegative`, `EndCapPositive`, `SideFace { index }` |
| `execute_sweep` | `sweep`: a polygon section along a 3D chain | `EndCapNegative`, `EndCapPositive`, `SideFace { index }` |
| `execute_pattern_instances` | `transform_body`, `mirror_body` | `PatternInstance { index }` |
| `execute_import`, `execute_import_analytic`, `merge_import_results` | imported bodies | — |
| `execute_fillet`, `execute_chamfer`, `execute_shell` | deferred indefinitely (root `CLAUDE.md`) | |

Pattern output order is instance-major: instance 0 is the seed itself,
re-emitted rather than copied; output 0 is `Main` and the rest are
`Body { index }`.

Combining an extrude with existing bodies (add, cut, intersect) is decided in
`feature-engine`, which calls `execute_extrude` and then `execute_boolean`.
