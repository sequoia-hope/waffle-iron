# F5a — the surface interface and carried parameter coordinates

**Roadmap:** `docs/yang_faithfulness_roadmap.md` F5a. **Method:** in place
(`specs/yang_paper_path.md` §6). **Ledger:** D14 (reopened 2026-10-27),
N7, matrix rows 4.2-24, 4.3-05, 4.4-09. **Gate for every increment:** the
canonical geometry hash (`WAFFLE_GEOMETRY_HASH_OUT`) identical on all 374
corpus cases, and the rewrite tier green. F5a changes no output.

## 1. What the paper needs and the code does not have

The paper's hybrid representation lives in the parameter domain:

- Stage 1 samples each patch in (u, v) and so knows every vertex's
  parameters (`refs/text/yang2025_hybrid_boolean.txt:371-374`).
- The discretization bound of a boundary triangle is the control-net
  distance of the rational Bézier sub-patch over the triangle's covering
  parameter rectangle, d(T) (§4.1.2, Fig. 6, `:358-378`; Theorem A.1).
- The Gauss-map test bounds the normals of a sub-patch by a cone built from
  its control net (§4.2.2, Theorem 4.1, `:446-466`; Appendix B,
  `:1206-1236`).
- Each intersection point gets (u, v) on both surfaces from barycentric
  coordinates in the triangles that meet there (§4.2.3, `:510-519`).
- §4.3 optimizes (u, v, s, t) from those coordinates (`:539-540`,
  Appendix C), and §4.4.1 triangulates the trimmed patch in the parameter
  domain (`:560-562`).

yang-rs holds 3D points and tests them against implicit surfaces. It forms
no parameter coordinates (matrix 4.2-24 DEVIATES), seeds Newton from a 3D
position (4.3-05), and charts only planes, cylinders and cones (4.4-09).
Surfaces are reached by matching on the five `Surface` variants: 668 uses
in 56 non-test files of yang-rs (2026-11-16). A B-spline surface has no
implicit form, so none of that extends to it.

F5a builds the substrate and changes no result: one interface every stage
can reach a surface through, the five analytic surfaces as its instances,
and (u, v) carried on every mesh vertex. F2 (§4.4.1 on parameter charts),
F4 (§4.3 seeded in parameters) and F5b (Stage 1 as §4.1, the first
B-spline instance) are its consumers.

## 2. The interface

A new module `crates/yang-rs/src/param_surface/`. All of it is `pub(crate)`.

```rust
/// A point of a surface's parameter domain.
pub(crate) type Uv = cad_primitives::Point2;

/// A closed parameter rectangle [u0, u1] × [v0, v1].
pub(crate) struct ParamRect { pub u0: f64, pub u1: f64, pub v0: f64, pub v1: f64 }

/// One axis of the parameter domain.
pub(crate) struct ParamAxis {
    pub lo: f64,            // −∞ for an unbounded axis
    pub hi: f64,            // +∞ for an unbounded axis
    pub period: Option<f64>,// Some(2π) on an angular axis; the seam is lo ≡ hi
}

/// Where the parametrization degenerates: an iso-line that maps to one point
/// (a sphere pole, a cone apex). The normal is undefined there.
pub(crate) enum Degeneracy { UIso(f64), VIso(f64) }

pub(crate) struct ParamDomain {
    pub u: ParamAxis,
    pub v: ParamAxis,
    pub degenerate: Vec<Degeneracy>,
}

/// s, s_u, s_v, s_uu, s_uv, s_vv at one parameter point.
pub(crate) struct SurfaceDerivs { pub s: [f64; 3], pub su: [f64; 3], pub sv: [f64; 3],
                                  pub suu: [f64; 3], pub suv: [f64; 3], pub svv: [f64; 3] }

/// A tensor-product rational Bézier patch: (deg_u + 1) × (deg_v + 1)
/// control points, row-major in u, all weights > 0.
pub(crate) struct RationalBezierPatch {
    pub deg_u: usize,
    pub deg_v: usize,
    pub points: Vec<[f64; 3]>,
    pub weights: Vec<f64>,
}

pub(crate) trait ParamSurface {
    fn domain(&self) -> ParamDomain;
    fn eval(&self, uv: Uv) -> Result<[f64; 3], SurfaceError>;
    fn derivs(&self, uv: Uv) -> Result<SurfaceDerivs, SurfaceError>;
    /// Unit s_u × s_v. For every instance this is the OUTWARD normal of the
    /// solid the `Surface` bounds (a face's `reversed` flag is applied by
    /// the caller, as today). `Err(Degenerate)` on a degenerate iso-line.
    fn normal(&self, uv: Uv) -> Result<[f64; 3], SurfaceError>;
    /// The surface over `rect` as rational Bézier patches whose union is the
    /// surface over `rect` and whose weights are all positive. An instance
    /// may split `rect` (the analytic arcs need angular spans ≤ π/2).
    fn bezier_patches(&self, rect: ParamRect) -> Result<Vec<RationalBezierPatch>, SurfaceError>;
}
```

`SurfaceError` names every refusal loudly (non-finite input, invalid
surface, a rectangle outside the domain or spanning more than one period, a
degenerate point). There is no clamping.

**The two bounds are generic functions over `bezier_patches`, not trait
methods.** An instance cannot substitute its own answer; a closed-form
shortcut is a separate, recorded fast path (§5).

- `control_net_distance(surface, tri_uv: [Uv; 3]) -> Result<f64, _>` is
  d(T): the covering rectangle of the three corners, its patches, and the
  largest distance from any control point to the 3D triangle through the
  three evaluated corners. The convex-hull property of a positive-weight
  rational net makes it a certified upper bound (Theorem A.1). This is
  today's `stage4_dt::d_of_t`, made generic.
- `normal_cone(surface, rect) -> Result<NormalCone, _>` is Theorem 4.1 per
  patch, unioned. For each patch, C2 bounds the directions of s_u and C1
  those of s_v. For a rational patch the derivative direction lies in the
  cone of the control-point differences P_j − P_i (i < j) along that
  parameter within each row (Sederberg & Wang 1987); a polynomial patch
  needs only consecutive differences, and the rational rule is used for
  both. If C1 and C2 are not separable by a plane through the origin (the
  theorem's hypothesis), the call returns `Err(ConeHypothesis)` and the
  caller subdivides, as the paper refines the discretization (`:459-462`).
  Otherwise C3 has axis d = a1 × a2 / |a1 × a2| and half-angle the largest
  angle between d and the normal of a plane through the origin tangent to
  both cones with (a1 · n)(a2 · n) < 0. `NormalCone { axis, half_angle }`
  unions conservatively.

A third primitive, the Theorem A.2 two-triangle check of §4.1.1, belongs
to F5b's Stage 1 and is not built here.

## 3. The five analytic instances

`impl ParamSurface for Surface` dispatches to one implementation per
variant. The embedding is the one `stage4_dt::eval_uv` pins today, so
Stage 1, d(T) and every later reader share a single convention:

| Surface | u | v | domain | degenerate |
|---|---|---|---|---|
| Plane | in-plane e1 | in-plane e2 | unbounded × unbounded | — |
| Cylinder | azimuth, period 2π | axial offset, unbounded | | — |
| Cone | azimuth, period 2π | axial distance from apex, [0, ∞) | | v = 0 (apex) |
| Sphere | azimuth, period 2π | latitude, [−π/2, π/2] | | v = ±π/2 (poles) |
| Torus | azimuth, period 2π | tube angle, period 2π | | — |

Frames come from `ortho_basis`, which is right-handed (e1 × e2 = n̂). With
it s_u × s_v points outward on all five, and a pin checks it per variant.
The sphere's axis stays the pinned world ẑ, so its poles sit at world ±z,
not in the face's own frame. That is a property of the embedding, not of
the paper; it is recorded in the module docs and becomes an instance
parameter if a sphere face ever needs a pole elsewhere.

`bezier_patches` builds the exact nets `stage4_dt` already builds: the
surface-of-revolution construction of Piegl & Tiller ch. 8, each angular
span split to ≤ π/2 so the middle weight cos(Δ/2) ≥ √2/2. A plane over a
rectangle is one bilinear patch.

## 4. Parameter coordinates carried

A per-vertex map in lockstep with `mesh.verts`, the vertex counterpart of
the per-triangle `TriangleErrorBoundMap` (`param_surface/carried.rs`):

```rust
/// For each mesh vertex, its (u, v) on every face it lies on.
pub(crate) struct VertexParamMap {
    pub params: Vec<Vec<VertexParam>>,          // 1:1 with mesh.verts
    pub refused: usize,                          // entries the inverse refused
    pub settled: usize,                          // entries added by incidence
    pub surfaces: BTreeMap<FaceRef, Surface>,    // every face's surface
}
pub(crate) struct VertexParam { pub face: FaceRef, pub surface: Surface, pub uv: Uv }
pub(crate) struct FaceRef { pub input: InputId, pub face: u32 }
```

Each entry carries its surface, so a mutator re-inverts a moved vertex
without the input solids; the face table lets `settle` work from a
triangle's attribution alone.

- **Stage 1 / Stage 0.** At the Stage-2 hand-off (after Stage 0 and the
  small-loop splice), `for_operand` gives each operand vertex its (u, v) on
  every face it is a triangle corner of (`tri_face`), by the closed-form
  inverse (§5). The tessellators build their points from angles in frames
  not yet shown to be the pinned embedding's, so they do not write those
  angles; F5b's Stage 1 does (N109).
- **Stage 2** (`for_arrangement`) is §4.2.3: for each arrangement triangle
  and each of its source triangles (`la.source`), every corner gets its
  (u, v) on the source's face by barycentric interpolation of the source
  corners, periodic corners unwrapped to the first live corner's period, a
  corner on a degenerate iso-line (pole, apex) left out of the azimuth. A
  vertex bit-equal to a source corner takes the corner's (u, v). The
  barycentrics are signed sub-areas against the triangle's normal: the Gram
  normal equations square a sliver's condition number (R0003's gear caps
  put a point on a long edge 4e-3 off). The kept submesh's vertex holds the
  union of the arrangement vertices that welded onto it. The paper calls
  these coordinates approximate (`:518-519`).
- **Stages 4 and 5.** Every vertex mutator keeps the map in lockstep:
  `move_vertex` / `moved` re-invert a moved vertex seeded with its old
  (u, v) for the branch; a pinch split copies (`push_copy`); a collapse that
  hands the survivor the victim's position merges both vertices' faces
  (`merged_into`); a compaction remaps (`remap`); CDT seeds and on-curve
  inserts are appended bare (`push_bare`) and truncated with the vertices.
  `settle` then gives every vertex its (u, v) on any face it is now a
  triangle corner of and has none for, by the inverse seeded with another
  corner's (u, v) on that face: that is how a seed gets its patch's face,
  and how a collapse survivor gets the victim's faces. It runs at the end of
  every rebuild batch, after the Stage-4 relocation pass, and at the Stage-2
  and Stage-5 hand-offs. The map asserts its length against `mesh.verts`
  at the hand-offs `tri_bounds` asserts at, and at every compaction.
- **Nothing reads the map** in F5a. `YANG_UV_CENSUS=<file>` (`census`)
  records, per stage, the vertices with no (u, v), the refusals, the
  entries settled by incidence, the largest |eval(uv) − p| / d_ε over curved
  faces and the largest |eval(uv) − p| over planar ones.

## 5. Fast paths and the matches on surface type

The interface is the paper's route; the analytic surfaces also have
closed forms the paper does not use. Each closed form that stays is a
recorded fast path with the generic computation as its differential pin:

- **The analytic inverse** p ↦ (u, v) (a separate trait,
  `AnalyticInverse`, not part of `ParamSurface`; ledger N109): the paper
  never projects, because it samples in (u, v). It assigns coordinates to
  points the pipeline holds in 3D (§4). It returns the parameters of the
  closest surface point, a periodic coordinate on the branch nearest a seed
  (or in `[lo, lo + period)`), the seed's coordinate where the closest point
  fixes none (an axis point, a sphere's centre, a torus's tube circle), and
  `OutsideDomain` for a cone point nearer the nappe behind the apex. Pins:
  `inverse_undoes_eval`, `inverse_is_the_closest_point`,
  `inverse_degenerate_points_and_refusals`.
- **The closed-form Gauss maps** of `gauss_map.rs` (N91/N92) stay as the
  §4.2.2 admission test and gain a pin that each closed-form region
  contains the net cone's sampled normals over the same rectangle.
- **Closed-form SSI** (N7, PERMANENT) is untouched.

The 668 matches on `Surface::` are inventoried by the rule each sits in
(`docs/audits/f5a_surface_matches.md`, generated, one row per site) and
each is classed as:
(a) instance data: constructing, validating or printing a surface; stays.
(b) a general geometric rule that enumerates types: becomes an interface
call. The standing rule "never enumerate surface types inside a general
geometric rule" (CLAUDE.md) is this class's target.
(c) a closed-form fast path: stays, recorded as above, with its pin.
Class (b) converts in batches by rule family, each batch hash-identical.

## 6. Increments

1. **The module, unwired.** *(Landed 2026-11-16, bf81c65e.)* Types, trait, the five instances, the two
   generic bounds. Pins: `eval` equals `stage4_dt::eval_uv` bit for bit;
   `control_net_distance` equals `stage4_dt::d_of_t` bit for bit on its
   existing fixtures plus a randomized-but-seeded sweep; s_u × s_v is
   outward on all five; analytic derivatives agree with central differences;
   the normal cone contains a dense grid of sampled normals on every
   instance over rectangles up to the hypothesis limit, and refuses
   (`ConeHypothesis`) rather than answer past it. No production change.
2. **`stage4_dt` re-sited.** `eval_uv` and `d_of_t` become thin calls into
   the module and their duplicated nets are deleted; `gauss_map` gains its
   differential pin. Gate: hash identical. *(Landed 2026-11-16, hash
   identical on all 374 cases. A torus tube-angle span past 2π stays legal —
   the net covers the tube more than once and the bound holds — as
   `tests/n2_dt_adversary.rs` pins; only the azimuth is held to one
   period.)*
3. **Parameter coordinates carried** (§4), with the inverse ledgered
   (N109). Gate: hash identical; the census recorded in the roadmap's
   checkpoint log. Matrix 4.2-24 DEVIATES → PARTIAL (the corners come from
   the inverse, N109).
4. **The inventory and class (b) conversions**, by rule family. Gate per
   batch: hash identical.

After 4 the roadmap's F5a items are checked and F2 builds §4.4.1's patch
chart on `domain()` and the carried (u, v) (4.4-09), and F4 seeds §4.3
from them (4.3-05).
