# Non-Convex Boolean via Point-in-Solid Ray Casting

## Problem

`classify_face` in `boolean.rs` uses Sutherland-Hodgman clipping (`clip_polygon_by_solid`)
to classify faces as Inside/Outside/Partial. This clips each face polygon against ALL
opposing faces' inward half-planes, treating the opposing solid as a convex intersection
of half-spaces. This is correct for boxes (6 faces, convex) but **wrong for gear extrudes**
(60+ faces, non-convex). The half-space intersection of a non-convex solid's faces is a
degenerate region, causing misclassification → incorrect face fragments → unpaired
half-edges → non-manifold error.

## Parameters

Same as `waffle_kernel_boolean.md` — no interface changes.

## Branch Table

| Solid A | Solid B | Method |
|---------|---------|--------|
| Convex (box) | Convex (box) | Point-in-solid ray casting (was: Sutherland-Hodgman) |
| Convex (box) | Non-convex (gear) | Point-in-solid ray casting (NEW) |
| Non-convex (gear) | Convex (box) | Point-in-solid ray casting (NEW) |
| Non-convex | Non-convex | Point-in-solid ray casting (NEW) |
| Cylinder | Any | SSI pipeline (unchanged) |

## Algorithm: Point-in-Solid Ray Casting

### Face Classification

For each face of solid A, classify against solid B's volume:

1. **Coplanar check** (unchanged from existing code)
2. **Sample points**: centroid + all vertices of the face
3. **Test each sample** with `point_in_solid(sample, B_faces)`
4. **Classify**: All inside → Inside, all outside → Outside, mixed → Partial

### `ray_crosses_polygon(origin, ray_dir, polygon) → Option<t>`

1. Compute ray-plane intersection: `t = dot(poly.origin - origin, poly.normal) / dot(ray_dir, poly.normal)`
2. If t ≤ 0 or ray parallel to plane → None
3. Compute hit point: `hit = origin + t * ray_dir`
4. Project hit + polygon vertices to 2D (drop axis with largest normal component)
5. 2D crossing-number point-in-polygon test
6. Return `Some(t)` if inside polygon

### `point_in_solid(point, faces) → bool`

1. Cast ray from `point` in +Z direction
2. Count crossings with all face polygons using `ray_crosses_polygon`
3. Odd count → inside, even count → outside
4. If grazing (ray hits edge/vertex), retry with perturbed direction

### Partial Face Handling

For faces classified as Partial (mixed in/out vertices):
- Vertex-walk approach: classify each vertex, find edge crossings via binary search
- Build inside polygon from [inside vertices + boundary crossing points]
- Build outside polygon(s) as complement

## Invariants

- V - E + F = 2 (Euler's formula for genus-0 closed polyhedra)
- Watertight: every half-edge has a twin
- Volume bounds: union volume ≤ vol(A) + vol(B)
- Area conservation: total face area is conserved modulo intersection boundaries

## Research References

- Ref #7 Jacobson: Generalized winding numbers (simplified to ray-crossing for polyhedra)
- Ref #4 Shewchuk: Robust geometric predicates for edge cases
- Moeller-Trumbore: Efficient ray-triangle intersection (adapted for ray-polygon)
