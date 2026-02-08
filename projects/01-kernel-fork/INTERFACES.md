# 01 — Kernel Fork: Interfaces

## Types This Crate IMPLEMENTS

| Type | Role |
|------|------|
| `Kernel` trait | `TruckKernel` adapter wrapping truck's API |
| `KernelIntrospect` trait | `TruckIntrospect` adapter querying truck topology |
| `MockKernel` | Deterministic test double implementing both traits |
| `KernelSolidHandle` | Opaque handle wrapping truck's `Solid` type |
| `KernelId` | Newtype wrapping truck's VertexID/EdgeID/FaceID |
| `RenderMesh` | Produced by tessellation wrapper |
| `EdgeRenderData` | Produced by edge extraction |
| `FaceRange` | Produced during tessellation (maps triangles to faces) |
| `EdgeRange` | Produced during edge extraction |
| `KernelError` | Error type for all kernel operations |

## Types This Crate CONSUMES

| Type | Source | Purpose |
|------|--------|---------|
| `TopoKind` | INTERFACES.md | Entity kind classification |
| `TopoSignature` | INTERFACES.md | Geometric signature (this crate computes them) |
| `ClosedProfile` | sketch-solver | Input to `make_faces_from_profiles` |

## Trait Contracts

### `Kernel` trait

This crate provides two implementations:

1. **`TruckKernel`** — Wraps real truck operations. Used in integration tests and production.
2. **`MockKernel`** — Deterministic synthetic geometry. Used in unit tests by modeling-ops and feature-engine.

Both implementations must satisfy the same behavioral contract:
- `extrude_face` on a rectangular face produces a solid with 6 faces, 12 edges, 8 vertices.
- `boolean_subtract` removes material from body A where body B overlaps.
- `tessellate` produces a valid `RenderMesh` with correct face_ranges.
- Entity IDs from `KernelIntrospect` are stable within a session (no ID changes without a mutating operation).

### `KernelIntrospect` trait

Provides read-only topology queries. Critical for:
- **modeling-ops**: topology diffing (before/after operation to compute provenance)
- **feature-engine**: GeomRef resolution (finding entities by signature)

`compute_signature` must produce stable signatures for the same geometry. Signatures include surface type, area, centroid, normal, bounding box, and adjacency hash.
