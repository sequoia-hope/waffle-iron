# Spec: WaffleKernel Circle Extrude + Cylinder Tessellation

**Milestone 2** — Extends the extrude pipeline to accept `CircleProfile`, producing
a true cylinder B-Rep (3 faces, 3 edges, 2 vertices) with geometry-driven tessellation.

## Parameters

| Parameter | Type | Units | Range |
|-----------|------|-------|-------|
| CircleProfile.center_u | f64 | meters | any |
| CircleProfile.center_v | f64 | meters | any |
| CircleProfile.radius | f64 | meters | > 0 |
| plane_origin | [f64;3] | meters | any |
| plane_normal | [f64;3] | unit | |n|=1 |
| direction | [f64;3] | unit | |d|=1 |
| depth | f64 | meters | > 0 |

## Branch Table

| Case | Radius | Depth | Expected |
|------|--------|-------|----------|
| Unit cylinder | 1.0 | 1.0 | 2V, 3E, 3F, vol=π |
| Simple cylinder | 5.0 | 10.0 | 2V, 3E, 3F, vol=250π |
| Tall rod | 1.0 | 100.0 | 2V, 3E, 3F, vol=100π |
| Wide short | 10.0 | 1.0 | 2V, 3E, 3F, vol=100π |
| Micro (1e-4) | 1e-4 | 1e-4 | vol=π×1e-12 |
| Macro (1e3) | 1e3 | 1e3 | vol=π×1e9 |
| Off-center | r=5 at (10,20) | 1.0 | shifted bbox |

## Cylinder B-Rep Topology

```
Vertices: 2  (v_bottom at seam, v_top at seam)
Edges:    3  (e_bottom = circle v0→v0, e_top = circle v1→v1, e_seam = line v0→v1)
Faces:    3  (bottom cap, top cap, side)
V-E+F = 2-3+3 = 2 ✓

Bottom cap loop:  he_bot_a (self-loop: next=prev=self)
Top cap loop:     he_top_a (self-loop: next=prev=self)
Side face loop:   he_bot_b → he_seam_a → he_top_b → he_seam_b → (cycle)
```

## Invariants

- V-E+F = 2 (genus-0 solid)
- Volume ≈ π×r²×depth (within tolerance from N=64 tessellation)
- Watertight mesh (every edge shared by exactly 2 triangles)
- 3 faces, 3 edges, 2 vertices
- Face normals: caps point along ±plane_normal, side points radially outward
- Bounding box: center ± r in plane, 0 to depth along direction

## Failure Modes

- radius ≤ 0 → `KernelError::Other`
- depth ≤ 0 → `KernelError::Other` (already enforced)
- Invalid face ID → `KernelError::EntityNotFound`

## Tessellation (N=64)

| Test Case | Volume | Tol | N=64 Error | Pass? |
|-----------|--------|-----|------------|-------|
| Simple cyl (r=5,h=10) | 785.4 | 5.0 | 1.27 | ✓ |
| Tall rod (r=1,h=100) | 314.2 | 1.0 | 0.50 | ✓ |
| Wide short (r=10,h=1) | 314.2 | 1.0 | 0.50 | ✓ |
| Micro (r=1e-4,h=1e-4) | π×1e-12 | 1e-14 | ~1e-15 | ✓ |

## Research Basis

- [#33] Stroud Ch.4 — half-edge B-Rep for cylindrical topology
- [#16] Mantyla — self-loop edge representation
- Fan triangulation for circular caps — standard CG technique
