# Sphere Boolean SSI Dispatch

**FIP Feature Spec — Sprint 69**

## 1. Goal

Wire sphere primitives into the SSI-based boolean dispatch path in `do_boolean`,
extending `ssi_boolean_op` to handle box-sphere and sphere-sphere operand pairs.
This eliminates the polygon-approximation fallback for sphere boolean operations,
complying with Architectural Invariant A15 (Analytical Primacy).

Currently, sphere booleans go through the `a_all_quadric && b_all_quadric` polygon
clipping path, which discretizes curved sphere faces into flat polygon approximations.
This introduces geometric drift proportional to the tessellation density.

## 2. Parameters

### Box-Sphere Boolean
- `box_solid: &WaffleSolid` — axis-aligned box (all-planar, ≤6 faces)
- `sphere_params: &SphereParams` — center `[f64; 3]`, radius `f64`
- `op: BoolOp` — Union, Subtract, Intersect

### Sphere-Sphere Boolean
- `sphere_a: &SphereParams` — center, radius of first sphere
- `sphere_b: &SphereParams` — center, radius of second sphere
- `op: BoolOp` — Union, Subtract, Intersect

## 3. Branch Table

### Box-Sphere

| Configuration | Union | Subtract (box - sphere) | Intersect |
|---|---|---|---|
| Sphere fully inside box | box | box with spherical cavity (2 shells) | sphere |
| Disjoint | disjoint union (2 shells) | box | empty |
| Partial overlap | NotSupported → polygon fallback | NotSupported → polygon fallback | NotSupported → polygon fallback |
| Box fully inside sphere | sphere | empty | box |

### Sphere-Sphere

| Configuration | Union | Subtract (A - B) | Intersect |
|---|---|---|---|
| Concentric, r_a > r_b | sphere A | spherical shell (2 shells) | sphere B |
| Concentric, r_a = r_b | sphere A | empty | sphere A |
| Disjoint | disjoint union (2 shells) | sphere A | empty |
| Partial overlap | NotSupported → polygon fallback | NotSupported → polygon fallback | NotSupported → polygon fallback |
| B fully inside A | sphere A | spherical shell (2 shells) | sphere B |
| A fully inside B | sphere B | empty | sphere A |

**Note**: "Partial overlap" cases (where SSI produces a circle intersection curve
and the result has mixed planar+spherical faces) are deferred to a future sprint.
These cases return `NotSupported` and fall through to the existing polygon clipping
path with geometry preservation.

## 4. Invariants

1. **Euler formula**: V - E + F = 2S where S = number of shells
2. **Watertightness**: All output meshes must be watertight (0 unpaired edges)
3. **Volume conservation**: Output volume matches analytical prediction within 1%
4. **Surface type preservation**: Spherical faces in the result must carry
   `SurfaceGeom::Spherical`; planar faces carry `SurfaceGeom::Planar`
5. **Determinism**: Same inputs always produce same topology and geometry

## 5. Oracles

### Volume Oracles
- Box 10³ minus enclosed sphere r=3: `1000 - 4/3 π 27 ≈ 886.9`
- Two concentric spheres r=5 minus r=3 (shell): `4/3 π (125 - 27) ≈ 410.5`
- Two disjoint spheres union: `V_a + V_b`
- Sphere intersect with enclosing box: sphere volume `4/3 π r³`

### Topology Oracles
- Sphere primitive: V=6, E=12, F=8 (octahedral B-Rep)
- Box with enclosed spherical cavity: 2 shells, V-E+F = 4
- Disjoint union: 2 shells, V-E+F = 4

## 6. Failure Modes

1. **Tangent sphere-box**: Sphere touches box face but doesn't intersect.
   Classification: disjoint (sphere center outside box).
2. **Sphere at box corner/edge**: Partial overlap with complex intersection.
   Returns `NotSupported`, falls to polygon path.
3. **Nearly-concentric spheres**: Distance < TAU_MODEL treated as concentric.
4. **Zero-volume result**: Subtract sphere that exactly matches box → empty result.

## 7. Research Basis

- [#33] Stroud: Multi-shell Euler formula for cavity/void operations
- [#1] Patrikalakis Ch.5: Plane-sphere SSI produces circles
- [#24] Barton: Bijective surface remapping through boolean operations
- [#7] Jacobson: Generalized winding numbers for enclosure testing
