# 01 — Kernel Fork: Architecture

## Purpose

Wrap the [truck](https://github.com/ricosjp/truck) BREP geometry kernel behind Waffle Iron's `Kernel` and `KernelIntrospect` traits. No truck types may leak to other crates. This crate is the only crate that depends on truck directly.

## truck Crates Used

| Crate | Purpose |
|-------|---------|
| truck-topology | Topological data structures (Vertex, Edge, Wire, Shell, Solid), entity IDs |
| truck-geometry | NURBS curves/surfaces, geometric primitives |
| truck-modeling | Construction API (tsweep, rsweep), builder functions |
| truck-shapeops | Boolean operations (and, or, solid.not()) |
| truck-meshalgo | Tessellation (triangulation with chordal tolerance) |

## Fork Strategy

Fork truck as a git subtree or workspace dependency. We extend truck — we do not rewrite it. Extensions:

1. **Higher-level primitive builders** — `make_box(width, height, depth)`, `make_cylinder(radius, height)`, `make_sphere(radius)` built on top of successive `tsweep`/`rsweep` calls.
2. **Kernel trait adapter** — `TruckKernel` struct that implements the `Kernel` trait by delegating to truck-modeling and truck-shapeops.
3. **KernelIntrospect adapter** — `TruckIntrospect` struct that implements `KernelIntrospect` by querying truck-topology IDs and computing geometric signatures.
4. **Tessellation wrapper** — Wraps `MeshableShape::triangulation()` to produce `RenderMesh` with `FaceRange` metadata for GPU picking.

## TruckKernel Implementation Notes

### Extrude
`builder::tsweep(face, vector)` — translational sweep. Polymorphic via the `Sweep` trait. Extruding a closed planar face produces a closed solid with caps.

### Revolve
`builder::rsweep(face, origin, axis, angle)` — rotational sweep. If angle >= 2*PI, shape is closed automatically.

### Boolean Subtraction
No dedicated difference function. Implemented as: `solid_b.not()` (flip normals) followed by `solid_a.and(flipped_b)`. This is fragile and performance-limited.

### Boolean Performance Crisis
GitHub Issue #68 documents cube-cylinder boolean taking **13–15 seconds on M1 MacBook**. Tolerance parameter strongly affects speed/stability:
- tol=0.9 → ~4 seconds
- tol=0.2 → ~15 seconds
- tol=1.0 → panic

Commercial kernels do equivalent operations in single-digit milliseconds. This is the single largest technical risk and must be monitored and investigated continuously.

### Boolean Robustness Limitations
Known failures: cone apex (degenerate normal), coplanar faces, near-tangent surfaces, small slivers. Operations return `Option<Solid>` — `None` on failure with no diagnostic information.

## MockKernel

`MockKernel` implements both `Kernel` and `KernelIntrospect` with deterministic synthetic topology. This is critical — other teams (feature-engine, modeling-ops) depend on it for testing.

MockKernel design:
- Extruding a rectangle produces 8 vertices, 12 edges, 6 faces with predictable signatures.
- Extruding a circle produces predictable vertex/edge/face counts.
- Boolean operations produce deterministic topology changes.
- Entity IDs are assigned sequentially (KernelId(1), KernelId(2), ...).
- Signatures are computed from the synthetic geometry (known positions, normals, areas).
- Tessellation produces simple but valid meshes (e.g., 12 triangles for a box).

## Tessellation with Face-Range Tracking

truck-meshalgo's `triangulation(tol)` tessellates per-face. We collect triangles per-face and record the index ranges, producing `FaceRange` entries that map triangle ranges to logical faces. This enables GPU picking in three.js: given a triangle hit from raycasting, binary-search `face_ranges` to identify the owning face.

## Fillet Strategy (Phased)

Fillets are effectively absent in truck ("Now, one cannot make a fillet... by truck"). Our phased approach:

1. **Phase 1:** Constant-radius edge fillet between two planar faces. Build the fillet surface analytically (cylindrical for 90-degree edges), trim the adjacent planar faces, reconstruct topology.
2. **Phase 2:** Constant-radius fillet for planar-cylindrical intersections.
3. **Phase 3:** Variable-radius fillets, vertex blending. (Deferred)

## Chamfer Strategy

Chamfer = cut with a plane at 45 degrees (equal distance) along the edge. Implemented via boolean subtraction of a wedge-shaped tool body. Fragile due to boolean limitations but simpler than fillet.

## Shell Strategy

Shell = offset all faces inward by thickness, then subtract the offset solid from the original. Depends on boolean subtraction quality.

## STEP Export Limitations

truck's ruststep supports AP203 only. Boolean-result solids cannot be exported. For production: rebuild the final solid from the feature tree, attempt export. Document failures clearly.
