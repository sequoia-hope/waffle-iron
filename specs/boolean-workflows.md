# Boolean Workflow Specification

## Overview

End-to-end tests for boolean operations through the feature engine pipeline,
exercising `ModelBuilder::truck()` with real TruckKernel geometry.

## Coordinate System

- `rect_sketch([0,0,0], [0,0,1], 0,0,10,10)` + `extrude(10)` produces a cube
  at approximately x∈[−10,0], y∈[0,10], z∈[0,10].
- `tangent_x_from_normal([0,0,1])` → X along [0,−1,0], Y along [1,0,0].
- `tangent_x_from_normal([0,1,0])` → X along [0,0,−1], Y along [1,0,0].
- Circle sketches use 16-segment polygons: area = r² × 16 × sin(2π/16) / 2.
- `eps=0.01` in rebuild.rs offsets cut tools; `tol=0.05` in TruckKernel.

## Branch Table

### A. Boss-on-Boss Union

| # | Scenario | Coplanar? | Expected |
|---|----------|-----------|----------|
| A1 | Circle boss on z=10 top face | Yes | Union succeeds, f>6, vol>1000, bbox z_max≈15 |
| A2 | Circle boss on z=0 bottom, dir [0,0,−1] | Yes | Union succeeds, bbox z_min≈−5 |
| A3 | Circle boss on y=10 side face | Yes | Union succeeds, bbox y_max≈15 |
| A4 | Two bosses same face sequential | Yes×2 | IGNORED: chained boolean risk |
| A5 | Rect boss 4×4×5 on top | Yes | Union succeeds, vol≈1080 |

### B. Cut Through Boss

| # | Scenario | Chained? | Expected |
|---|----------|----------|----------|
| B1 | Circle cut through boss (extrude_cut) | Yes | IGNORED: chained boolean |
| B2 | Circle cut through boss (explicit subtract) | Yes | IGNORED: chained boolean |
| B3 | Shallow cut into tall boss | Yes | IGNORED: chained boolean |
| B4 | Wide cut removes boss footprint | Yes | IGNORED: chained boolean |

### C. Cut Wrong Direction / Free Space

| # | Scenario | Expected |
|---|----------|----------|
| C1 | Cut directed away from solid | Volume unchanged or error |
| C2 | Cut with no prior solid | Engine error |
| C3 | Cut misses solid laterally | Volume unchanged or error |

### D. Partial Overlap / Symmetric

| # | Scenario | Coplanar? | Expected |
|---|----------|-----------|----------|
| D1 | Two offset boxes, partial overlap | No | Vol between max(v1,v2) and v1+v2 |
| D2 | Boss on boss (double coplanar) | Yes×2 | IGNORED: chained boolean |
| D3 | Partially overlapping coplanar rects | Partial | IGNORED: face splitting needed |
| D4 | Two offset boxes, partial overlap variant | No | Vol between max and sum |
| D5 | Circle boss height=50 on 10³ cube | Yes | bbox z_max≈60, vol>1000 |

### E. Adversarial Cases

| # | Scenario | Expected |
|---|----------|----------|
| E1 | Very thin boss depth=0.1 | vol≈1000+polygon_area×0.1 |
| E2 | Boss r=8 exceeds 10×10 face | IGNORED: crosses boundary |
| E3 | Boss at cube edge | IGNORED: boundary degenerate |
| E4 | Cut depth exactly solid height | Solid has through-hole, vol reduced |
| E5 | Multiple non-overlapping cuts | IGNORED: chained boolean |
| E6 | Two explicit subtracts | IGNORED: chained boolean |
| E7 | Circle boss volume conservation | merged_vol = cube_vol + boss_vol ± tol |

## Invariants

1. **Topology**: Boolean results have f>6 (more faces than a simple box).
2. **Volume conservation**: Union vol ≤ vol_A + vol_B; Subtract vol < vol_target.
3. **Bounding box**: Union bbox ⊇ max(bbox_A, bbox_B); Cut bbox ⊆ original bbox.
4. **Manifold**: All active tests produce manifold solids (Euler V−E+F=2).
5. **No engine errors**: Active tests call `assert_no_errors()`.

## Oracle Descriptions

- **mesh_volume**: Divergence theorem on triangle mesh. Tolerance ±5% for polygonal approximation.
- **mesh_bounding_box**: AABB from vertex positions. Tolerance ±0.5 for tessellation.
- **topology_counts**: (V, E, F) from kernel introspection.
- **check_topology**: Euler formula, manifold edge check.
- **check_mesh**: Watertight, consistent normals, valid indices.

## Polygon Area Formula

For a 16-segment circle approximation:
```
area = r² × 16 × sin(2π/16) / 2 ≈ r² × 3.0615
```
