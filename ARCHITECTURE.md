# Architecture

## Overview

A parametric CAD system with a Rust computational kernel compiled to WebAssembly and a TypeScript GUI frontend. The kernel handles B-Rep modeling, constraint solving, and Boolean operations. The GUI renders tessellated meshes via a WASM bridge that serializes geometry data as JSON.

## Crate Structure

```
crates/
  kernel/         Core B-Rep engine, geometry, Boolean ops, validation
  tessellation/   Converts B-Rep solids to triangle meshes for rendering
  solver/         2D constraint solver for parametric sketches
  wasm-bridge/    Exposes kernel + tessellation + solver to JS/TS via WASM
gui/              TypeScript frontend (Three.js or similar)
```

### kernel (`cad-kernel`)

The central crate. Contains:

- **geometry/** -- Point3d, Vec3, transforms, bounding boxes, curves (Line, Circle, Ellipse, NURBS), surfaces (Plane, Sphere, Cylinder, Cone, Torus), ray-surface intersections.
- **topology/** -- B-Rep data model (`brep`), Euler operators (`euler`), primitive solid builders (`primitives`).
- **boolean/** -- Boolean engine with face classification and point-in-solid ray casting. Monte Carlo volume estimation.
- **operations/** -- Profile extrusion, parametric feature tree (Sketch, Extrude, Revolve, Fillet, Chamfer, BooleanOp).
- **validation/** -- Multi-level verification: topology audit (L0), geometry consistency (L1), full verification report.

Global tolerance constants live in `lib.rs` (`Tolerance` struct): coincidence 1e-7 m, angular 1e-10 rad, parametric 1e-9.

### tessellation (`cad-tessellation`)

Depends on `cad-kernel`. Provides:

- `tessellate_planar_face` -- Fan triangulation of planar polygonal faces.
- `tessellate_solid` -- Tessellates all faces of a solid into a merged `TriangleMesh`.
- `tessellate_surface_grid` -- UV-grid sampling for parametric surfaces (spheres, cylinders).

Output format: interleaved `Vec<f32>` positions/normals + `Vec<u32>` indices.

### solver (`cad-solver`)

Independent 2D constraint solver. No dependency on the kernel.

- **Sketch** -- Collection of entities (Point, Line, Circle, Arc) with a flat parameter vector `[x0, y0, x1, y1, ...]`.
- **Constraints** -- Coincident, PointOnEntity, Parallel, Perpendicular, Horizontal, Vertical, Equal, Tangent, Symmetric, Distance, Angle, Radius, Fixed.
- **Solver** -- Gradient descent with backtracking line search (simplified Levenberg-Marquardt). Residual-based convergence with configurable tolerance and iteration limit.

### wasm-bridge (`cad-wasm-bridge`)

Thin orchestration layer. `CadEngine` struct holds an `EntityStore` and a list of solid IDs. Exposes high-level operations:

- `create_box`, `create_cylinder`, `create_sphere`, `extrude_rectangle`
- `boolean(a, b, op)` -- union / intersection / difference
- `tessellate(solid_idx)` -> `MeshData` (JSON-serializable)
- `model_info(solid_idx)` -> face/edge/vertex counts

## Key Traits and Interfaces

The codebase currently uses concrete types rather than trait abstractions. The main interface boundaries are:

| Boundary | From | To | Data |
|---|---|---|---|
| Modeling -> Tessellation | `EntityStore + SolidId` | `TriangleMesh` | `tessellate_solid()` |
| Modeling -> Validation | `EntityStore + SolidId` | `VerificationReport` | `full_verify()` |
| Sketch -> Kernel | `Sketch` params | `Profile` points | Manual conversion |
| WASM bridge -> JS | `MeshData` | JSON string | `serde_json` |

Curves implement `evaluate(t) -> Point3d`. Surfaces implement `evaluate(u,v) -> Point3d` and `normal_at(u,v) -> Vec3`. Both are enum-dispatched (`Curve`, `Surface`).

## B-Rep Topology Model

Arena-based storage using `slotmap::SlotMap` with typed keys:

```
Solid
  └── Shell[]              (Outward or Inward orientation)
        └── Face[]         (Surface + outer Loop + inner Loop[])
              └── Loop
                    └── HalfEdge[]  (linked to Edge via twin pairs)
                          └── Edge  (Curve + two HalfEdges + start/end Vertex)
                                └── Vertex (Point3d + tolerance)
```

Each entity is stored in its own `SlotMap` inside `EntityStore`. References between entities use typed IDs (`VertexId`, `EdgeId`, `HalfEdgeId`, `LoopId`, `FaceId`, `ShellId`, `SolidId`).

Half-edges carry: owning edge, twin half-edge, face, loop, start/end vertices, parameter range on curve, and forward/reverse flag.

Euler operators (`mvfs`, `mev`, `mef`) maintain the invariant V - E + F = 2 per genus-0 shell.

Primitive builders construct complete solids directly: `make_box` (proper twin linking), `make_cylinder` and `make_sphere` (per-face edge creation, twins deferred).

## Verification System

Three verification levels:

### L0: Topology Audit (`audit_solid`)

- Euler-Poincare check: V - E + F = 2 per shell.
- Loop closure: last half-edge's end vertex equals first half-edge's start vertex.
- Half-edge twin consistency: `twin.twin == self`.
- Reports structured `TopologyError` variants (EulerViolation, OpenLoop, DanglingVertex, HalfEdgeTwinMismatch, VertexPositionMismatch).

### L1: Geometry Consistency (`verify_geometry_l1`)

- Vertex positions match curve endpoint evaluations within tolerance.
- Reports `GeometryError` variants (VertexCurveMismatch, EdgeNotOnSurface, NormalInconsistency).

### Volume Verification (`verify_boolean_volume_identity`)

- Checks vol(A union B) = vol(A) + vol(B) - vol(A intersect B).
- Uses Monte Carlo volume estimation (deterministic LCG PRNG, ray-casting point classification).
- Returns relative error for threshold-based pass/fail.

All three levels are combined in `full_verify()` which returns a `VerificationReport`.

## Current Status

### Implemented

- B-Rep data model with arena storage and typed keys.
- Euler operators: mvfs, mev, mef.
- Primitive solids: box (with proper edge twins), cylinder, sphere.
- Geometry: points, vectors, transforms, bounding boxes, lines, circles, ellipses, NURBS curves, planes, spheres, cylinders, cones, tori.
- Ray-surface intersection: plane, sphere, cylinder.
- Profile extrusion along arbitrary direction.
- Parametric feature tree data model (Sketch, Extrude, Revolve, Fillet, Chamfer, Boolean).
- Boolean operations: bounding-box pre-check, face classification via ray-cast point-in-solid, face selection by operation type.
- 2D constraint solver with gradient descent.
- Planar face tessellation (fan triangulation) and UV-grid surface tessellation.
- WASM bridge with JSON serialization.
- Multi-level topology and geometry verification.
- Monte Carlo volume estimation.

### Planned / In Progress

- Proper half-edge twin linking for cylinder and sphere primitives.
- Dangling vertex detection and normal consistency checks in topology audit.
- Surface-surface intersection for Boolean operations on curved faces.
- Revolve, fillet, and chamfer operation implementations (data model exists, execution pending).
- NURBS surface tessellation with adaptive refinement.
- Full Levenberg-Marquardt solver with Jacobian (current solver uses finite-difference gradient descent).
- TypeScript GUI with Three.js rendering.
- `wasm-bindgen` / `wasm-pack` integration for the WASM bridge.
- Parametric expression evaluation for feature parameters.
