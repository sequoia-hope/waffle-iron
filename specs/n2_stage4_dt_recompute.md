# N2-2 — Stage-4 per-triangle `d(T)` recompute — Spec

Second increment of closing deviation **N2** (`docs/yang_deviations.md`; parent
spec `specs/n2_stage4_mesh_updating.md`). Yang 2025 §4.4.1 ends: "For the newly
generated boundary triangles around the intersection curve, we recalculate
`d(T)` to maintain controllable error"
(`refs/text/yang2025_hybrid_boolean.txt:568-571`). `d(T)` itself is defined in
§4.1.2 / Fig 6 (`refs/text/yang2025_hybrid_boolean.txt:340-378`): for a
boundary triangle `T`, find the minimal rectangular region
`(u0,u1)×(v0,v1)` in the parametric domain covering its three vertices, obtain
the surface sub-patch defined on that region, and compute the **maximal
distance between the sub-patch's control points and the triangle**. The
per-triangle discretization error bound is then

```
d_eps(T) = d_eps   if T is an inner triangle,
           d(T)    otherwise (boundary triangle).
```

The convex-hull property of the (positive-weight) rational Bézier control net
certifies the bound: the patch lies inside the hull of its control points, and
point-to-triangle distance is a convex function of the point, so its maximum
over the hull is attained at a control point. `d(T)` is therefore a **certified
upper bound** on the true max distance from the surface patch over `T`'s
parametric footprint to the 3D triangle — never an estimate.

## Decomposition context

- **N2-1 (DONE):** the §4.4.1 mesh-updating primitive (`stage4_mesh_update`).
- **N2-2 (this spec / this PR): the `d(T)` primitive.** A pure, deterministic
  function computing the certified Fig-6 bound for one triangle given the
  analytic surface and the triangle's parametric coordinates. **Not wired into
  the pipeline** — unit-tested in isolation.
- **N2-3:** wire `stage4_mesh_update` + `d_of_t` into
  `stage4_relocate_and_correct` (own spec).

## 1. Goal

A function `d_of_t(surface, uv) -> Result<f64, DtError>` in a new module
`yang_rs::stage4_dt` that computes the Fig-6 `d(T)` for a triangle with
parametric corners `uv: [Point2; 3]` on an analytic `Surface`, plus the pinned
parametric embedding `eval_uv(surface, p) -> Result<Point3, DtError>` that
defines what those coordinates MEAN (the same contract N2-3's patch extraction
will use).

The paper's surfaces are NURBS; ours are analytic quadrics. All four curved
`Surface` variants are **surfaces of revolution**, and a rational Bézier patch
of a surface of revolution is exactly representable [#32 Piegl & Tiller ch. 8]:
profile control points revolved through an azimuth arc, tensor-product weights.
So ONE constructor (roadmap §0.1 "general over piecemeal") covers all curved
surfaces; the per-surface part is only the profile generator (a line segment
for cylinder/cone, a circular arc for sphere/torus).

## 2. Parameters

`d_of_t(surface: &Surface, uv: [Point2; 3]) -> Result<f64, DtError>`

- `surface` — the yang-rs analytic `Surface` the patch lies on.
- `uv` — the triangle's three corners in that surface's parametric domain
  (below). The triangle's 3D vertices are `eval_uv(surface, uv[k])` — mesh
  vertices are sampled from the surface (Stage 1) or moved onto it
  (N2-1 merge), so corners are on-surface by construction.
- Returns `d(T) >= 0` in world units (parametric inputs are dimensionless
  angles / world-unit axial distances as defined below).

No tolerances are taken: the computation is closed-form and certified; there
is nothing to tune.

### Pinned parametric embedding (`eval_uv`)

All frames use the existing deterministic `ortho_basis` (lib.rs; PR-YR7) —
`(e1, e2) = ortho_basis(axis)`, so Stage-1 sampling, N2-1 patches and `d(T)`
share one convention.

| Surface | u | v | `eval_uv(u, v)` |
|---|---|---|---|
| `Plane { normal, d }` | in-plane e1 coord | in-plane e2 coord | `(-d)·n̂ + u·e1 + v·e2`, `(e1,e2) = ortho_basis(normal)` |
| `Cylinder { axis_point, axis_dir, radius }` | azimuth θ (rad) | axial offset h (world units) | `axis_point + h·â + r(cos u·e1 + sin u·e2)`, `â = normalize(axis_dir)` |
| `Cone { apex, axis_dir, half_angle }` | azimuth θ (rad) | axial distance t ≥ 0 from apex | `apex + v·â + v·tan(half_angle)(cos u·e1 + sin u·e2)` |
| `Sphere { center, radius }` | azimuth θ (rad) | latitude φ ∈ [−π/2, π/2] | `center + r(cos v cos u·e1 + cos v sin u·e2 + sin v·ẑ)`, frame `ortho_basis(ẑ)` with the pinned canonical axis `ẑ = (0,0,1)` (a sphere has no intrinsic axis) |
| `Torus { center, axis_dir, major_radius R, minor_radius r }` | azimuth θ (rad) | tube angle φ (rad) | `center + (R + r cos v)(cos u·e1 + sin u·e2) + r sin v·â` |

## 3. Branch table

| Surface | Profile generator (in the (radial ρ, axial z) half-plane) | Net degree (u×v) | Subdivision of the covering rectangle |
|---|---|---|---|
| Plane | — | — | none. `d(T) = 0.0` exactly (patch and triangle are coplanar; §4.1.2's bound is trivially zero). |
| Cylinder | line segment `ρ = r`, `z ∈ [v0, v1]`, weights 1 | 2 × 1 | u into `ceil(span_u / (π/2))` equal slices |
| Cone | line segment `ρ = v·tan(α)`, `z = v`, `v ∈ [v0, v1]`, weights 1 | 2 × 1 | u as cylinder |
| Sphere | circular arc radius `r` about center, `φ ∈ [v0, v1]` | 2 × 2 | u AND v into ≤ π/2 slices each |
| Torus | circular arc radius `r` about `(R, 0)`, `φ ∈ [v0, v1]` | 2 × 2 | u AND v into ≤ π/2 slices each |

Algorithm (curved surfaces, one shared path):

1. Covering rectangle `[u0,u1]×[v0,v1]` of the three `uv` corners (Fig 6c).
   Degenerate (zero-width/height) rectangles are legal — the net degenerates
   to a curve/point net and the bound still holds.
2. Validate ranges (§6). Subdivide the rectangle so every sub-rectangle's
   angular spans are ≤ π/2 (positive rational-arc weights; the exact-arc
   construction needs span < π, and ≤ π/2 keeps the middle weight
   `cos(span/2) ≥ √2/2`).
3. Per sub-rectangle, build the exact rational Bézier control net of the
   surface-of-revolution patch [#32 ch. 8]: circular-arc rows in u (endpoints
   on the surface, middle control point at the tangent intersection, scaled by
   `1/cos(span_u/2)` radially; weight `cos(span_u/2)`), profile control points
   in v (line = the two endpoint rings; arc = rational quadratic with middle
   point at the profile tangent intersection, weight `cos(span_v/2)`).
4. `d(T) = max` over ALL control points of all sub-rectangles of the
   **point-to-triangle distance** to the 3D triangle
   `[eval_uv(uv[0]), eval_uv(uv[1]), eval_uv(uv[2])]`. Point-to-triangle
   distance = distance to the closest point of the (possibly degenerate)
   triangle: interior-projection, else nearest edge/vertex. A degenerate 3D
   triangle degrades to segment/point distance — legal input.

No other modes. The subdivision count is derived, not a parameter.

## 4. Invariants (measurable)

- **I1 (certification — the load-bearing one):** for a dense barycentric grid
  of samples `(u,v)` INSIDE the uv triangle, every
  `dist(eval_uv(u,v), triangle3d) ≤ d_of_t(...) + 1e-12`. The bound must
  dominate the true patch-to-triangle distance.
- **I2 (plane zero):** any triangle on any `Plane` returns exactly `0.0`.
- **I3 (canonical exactness):** cylinder `r=1`, axis `+z` through the origin,
  uv triangle `{(0,0), (π/2,0), (0,1)}` → the covering rectangle is one 90°
  sub-rectangle; the six control points are `(1,0,h), (1,1,h), (0,1,h)` for
  `h ∈ {0,1}` and the hand-derived answer is `d(T) = √6/3` (attained at the
  scaled middle control points), asserted to `1e-12`.
- **I4 (shrink monotonicity):** halving the canonical uv triangle about its
  uv centroid strictly decreases `d(T)` (the bound is meaningful: it tracks
  the patch, it is not a constant of the surface).
- **I5 (determinism):** two calls on identical input return bit-identical
  `f64` (`to_bits` equality).
- **I6 (rigid-motion sanity):** translating the surface datum
  (`axis_point`/`center`/`apex`) by a fixed offset leaves `d(T)` unchanged
  within `1e-9` (the bound is geometric, not coordinate-dependent).
- **I7 (nonnegativity + finiteness):** result is finite and `>= 0` for all
  valid inputs.

## 5. Oracles

- **Canonical (I3):** the hand-derived `√6/3` cylinder case above.
- **Certification sweep (I1):** for each curved surface type, at least one
  non-axis-aligned uv triangle (spanning enough azimuth to force subdivision,
  i.e. `span_u > π/2`), dense-sampled (≥ 20×20 barycentric grid) against
  `d_of_t`.
- **Plane (I2):** two distinct planes (axis-aligned and oblique normal),
  arbitrary triangles → exactly `0.0`.
- **Sphere pole legality:** a sphere triangle with a corner at `v = π/2`
  (pole; the azimuth ring degenerates) computes a finite certified bound (I1
  holds there).
- **Cone near-apex:** a cone triangle with `v0 = 0` (corner at the apex) is
  legal (`t ≥ 0`), finite, certified.
- **Mutation sanity (FIP §6.3, run by the Adversary):** flipping the middle
  control-point scale from `1/cos` to `cos`, or skipping subdivision, must
  fail I1 or I3 — proves the net construction and subdivision branches are
  exercised, not dead.
- **Determinism (I5):** repeat invocation, `to_bits` equality.

## 6. Failure modes

Loud `Result::Err` (P9/P10 — no clamping, no silent legalization):

- `NonFiniteInput` — any NaN/∞ in `uv` or in surface fields.
- `InvalidSurface` — `radius ≤ 0`; cone `half_angle ∉ (0, π/2)`; torus
  `major_radius ≤ minor_radius` or `minor_radius ≤ 0`; zero
  `axis_dir`/`normal`.
- `AzimuthSpanTooLarge` — covering-rectangle u-span `> 2π` for a curved
  surface (the caller handed coordinates from more than one period — the
  covering rectangle is ambiguous; unwrapping is the caller's job).
- `PolarRangeOutOfBounds` — sphere v-range not within `[−π/2, π/2]`.
- `NegativeConeAxialRange` — cone with any `v < 0` (behind the apex; the
  single-nappe solid convention of `Surface::Cone`).

`eval_uv` shares `NonFiniteInput` / `InvalidSurface` / `PolarRangeOutOfBounds`
/ `NegativeConeAxialRange` for its point argument.

## 7. Research basis

- **#24 Yang et al. 2025 §4.1.2 + Fig 6** — the `d(T)` definition and its role
  as the boundary-triangle error bound
  (`refs/text/yang2025_hybrid_boolean.txt:340-378`); **§4.4.1** — the
  recompute after mesh updating this increment provides
  (`refs/text/yang2025_hybrid_boolean.txt:568-571`).
- **#32 Piegl & Tiller, The NURBS Book ch. 7–8** — exact rational Bézier
  representation of circular arcs (middle control point at the tangent
  intersection, weight `cos(span/2)`) and of surfaces of revolution
  (tensor-product net, product weights). Yang cites the same source for its
  patch machinery.
- **Convex-hull certificate** — rational Bézier patches with positive weights
  lie in the convex hull of their control points [#32 §4.2 properties]; the
  maximum of a convex function (point-to-triangle distance) over a convex hull
  is attained at an extreme point, hence at a control point. This replaces the
  paper's implicit "control points bound the patch" argument with the same
  mathematics, made explicit.

### 7a. Analytical vs approximate

Exact/analytic. The paper computes `d(T)` from NURBS control nets obtained by
knot-insertion subdivision; our surfaces are analytic quadrics whose
revolution patches have EXACT closed-form rational Bézier nets — no
approximation enters. This is the same class of substitution as the signed-off
**N7** (closed-form SSI instead of Newton iteration for analytic surfaces;
invariant A15 analytical primacy). No SSI is performed here; A15 surface-pair
coverage is N/A. Dense sampling appears ONLY in test oracles (as a lower bound
on the true max — it can never certify, only refute; the certified bound is
the control-net maximum).

## 8. Scope / non-goals (this PR)

- No pipeline wiring, no boundary-triangle identification, no `d_eps(T)`
  table plumbing (all N2-3).
- No NURBS/Bézier input surfaces (separate milestone; this constructor is the
  machinery that milestone will extend).
- No change to `stage4_mesh_update` (N2-1) or to the global
  `stage4_chord_band` budget.
