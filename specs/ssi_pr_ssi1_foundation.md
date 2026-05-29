# Spec: PR-SSI1 — ssi-rs exact-SSI foundation

**Status:** active (M5 step 1 — Yang Stage 3's analytical-curve engine)
**Feature cycle:** ssi-1
**Roles (P5):** Spec Writer = Manager; Test Author and Implementer are distinct agents.

## Goal

Stand up the empty `ssi-rs` crate with its public types and the first three
analytical surface-surface intersection solvers, producing **true analytical
curves** (not polylines). This is the foundation Yang Stage 3 will use to refine
mesh-approximate intersection edges to surface-exact curves. No `yang-rs` wiring.

**Precision (decided):** analytical curve *representation*, **f64 parameters**.
A `Circle` is the exact circle (zero shape error); ~15-digit params. Topology
robustness lives in the exact mesh predicates already built — NOT here. f64
closed-form algebra; no `dashu`.

## Types (ssi-rs owns them — it's below yang-rs; cad-primitives is types-only)

```
pub enum QuadricSurface {
    Plane  { point: Point3, normal: Vector3 },   // normal assumed unit; n·(x−point)=0
    Sphere { center: Point3, radius: f64 },
}   // Cylinder/Cone/Torus arrive with their solvers (avoid unused-variant lint)

pub enum SsiCurve {
    Line   { point: Point3, dir: Vector3 },               // infinite line; dir unit
    Circle { center: Point3, normal: Vector3, radius: f64 }, // normal unit
}

pub enum SsiError {
    AnalyticalSolutionNotAvailable,   // pair not implemented (A15.2: no fallback)
    DegenerateInput,                  // coincident planes, zero/neg radius, zero/non-finite normal, …
}
```
Add a private/`pub` parametric **evaluator** (`SsiCurve::eval(t)` — circle: point at
angle `t` in its plane frame; line: `point + t·dir`) so the on-surface oracle and
future Stage-3 consumers can sample. The circle needs a deterministic in-plane
basis from `normal` (document the construction; e.g. pick the least-aligned axis).

## Solvers (closed-form, f64; each doc-comment cites Patrikalakis §5.8)

Signature: `fn <pair>(a: &QuadricSurface, b: &QuadricSurface) -> Result<Vec<SsiCurve>, SsiError>`
(or typed args). Empty `Vec` = no intersection curve. A top-level
`pub fn intersect(a, b) -> Result<Vec<SsiCurve>, SsiError>` dispatches by pair and
returns `AnalyticalSolutionNotAvailable` for any unimplemented combination.

### `plane_plane`
| case | result |
|---|---|
| transverse (normals not parallel) | one `Line` (dir = n_a × n_b, normalized; a point on both planes) |
| parallel, distinct | `[]` (no intersection) |
| coincident | `Err(DegenerateInput)` (overlap is 2D, not a curve) |

### `plane_sphere`  (circle of radius √(r²−d²), d = signed dist center→plane)
| case | result |
|---|---|
| `\|d\| < r` (transverse) | one `Circle` { center = sphere.center − d·n, normal = n, radius = √(r²−d²) } |
| `\|d\| == r` (tangent, within TAU_MODEL) | `[]` (point contact — not a curve; deferred) |
| `\|d\| > r` (disjoint) | `[]` |
| radius ≤ 0 | `Err(DegenerateInput)` |

### `sphere_sphere`  (circle in the plane perpendicular to the center line)
Let `D = |c_b − c_a|`. 
| case | result |
|---|---|
| `\|r_a − r_b\| < D < r_a + r_b` (transverse) | one `Circle` (center on the line c_a→c_b at `a = (D² + r_a² − r_b²)/(2D)` from c_a; normal = (c_b−c_a)/D; radius = √(r_a² − a²)) |
| tangent (`D == r_a+r_b` or `D == \|r_a−r_b\|`, within TAU_MODEL) | `[]` (point contact — deferred) |
| disjoint (`D > r_a+r_b`) or contained (`D < \|r_a−r_b\|`) | `[]` |
| concentric (`D < TAU_MODEL`) or radius ≤ 0 | `Err(DegenerateInput)` |

## Invariants / oracles (P1, DoD §1)

- **I1 (on-surface — the core exactness proof):** sample each result curve at N
  parameter values via `eval`; every sample satisfies **both** input surfaces'
  implicit equations within `TAU_MODEL` — plane: `|n·(x−p)| < TAU_MODEL`; sphere:
  `| |x−c| − r | < TAU_MODEL`.
- **I2 (analytical geometry):** assert the closed-form facts — plane∩sphere circle
  center is the foot of perpendicular from sphere center; radius² = r² − d²; circle
  normal ∥ plane normal. sphere∩sphere center/radius per the formula above.
- **I3 (branch coverage, P4):** every case in each table has ≥1 test (transverse,
  tangent, disjoint/contained, degenerate).
- **I4 (symmetry):** `intersect(a,b)` and `intersect(b,a)` yield the same curve set
  (same circle/line geometry, up to representation; e.g. line dir may flip sign —
  compare as unoriented).
- **I5 (determinism):** identical inputs → identical output bytes (no nondeterministic
  ordering; the in-plane basis construction is deterministic).

## Failure modes
- Unimplemented pair (e.g. plane∩cylinder) → `Err(AnalyticalSolutionNotAvailable)`
  (A15.2 — never a mesh/grid fallback).
- Degenerate input (coincident planes, concentric/zero-radius spheres, zero normal)
  → `Err(DegenerateInput)`. No `panic!`.

## Research basis
- **Patrikalakis & Maekawa**, *Shape Interrogation for CAD/M*, **§5.8
  Surface/Surface Intersections** (natural quadrics: plane, sphere, cylinder, cone)
  — local extract `docs/references/patrikalakis-shape-interrogation.txt`; §5.5.1
  (point/implicit-surface) grounds the on-surface oracle. Cite per solver.
- **Governance:** A15.1 (exact SSI for quadrics), A15.2 (no fallback), P8 (cite
  research), A14.3 (`cad-primitives` tolerances).

## Definition of Done (DoD §1)
Spec (this file); RED→GREEN separate commits; every branch (the tables) tested;
numeric/structural oracles (on-surface + analytical geometry, not "no panic");
canonical (transverse) + edge (tangent/disjoint/degenerate) cases; symmetry +
determinism; an unimplemented-pair test asserts `AnalyticalSolutionNotAvailable`;
no `unsafe`/`panic!`; CI gate (fmt + clippy -D warnings) clean for `ssi-rs`.
