//! Half-edge B-Rep arena: entity types, ids, and slot storage.
//!
//! ## Structure
//!
//! The topology follows the half-edge ("winged-edge link") representation
//! described by Stroud 2006 §2.2/§3.3 (he attributes the term "half-edges"
//! to Mäntylä): every undirected edge is represented by exactly two directed
//! half-edges that are mutual twins; half-edges chain into closed loops
//! (`next`/`prev` cycles); loops bound faces (one outer loop, zero or more
//! inner "ring"/hole loops); faces group into shells; shells group into
//! solids.
//!
//! There is no separate `Edge` entity: half-edges pair directly via `twin`,
//! and the edge count is `live half-edges / 2` (an invariant checked by
//! `validate_solid`).
//!
//! ## Invariants (encoded in types where possible)
//!
//! - **Twin pairing**: `twin(twin(h)) == h`, `twin(h) != h`. `twin` is a
//!   plain `HalfEdgeId` (never `Option`) — half-edges are only ever created
//!   in pairs by the Euler operators, so an unpaired half-edge is
//!   unrepresentable through the sanctioned mutation API.
//! - **Loop closure**: `next`/`prev` are total and mutually inverse; the
//!   `next` orbit of any half-edge returns to it and stays in one loop.
//! - **Lone-vertex loops**: the loop created by `mvfs` contains a single
//!   vertex and no half-edges. This is encoded as [`LoopBoundary::Lone`] —
//!   a loop cannot be simultaneously empty and edged.
//! - **Newell invariant** (crate hard rule 2): for every face whose outer
//!   loop has a numerically nonzero Newell normal,
//!   `face.surface.normal ≡ normalize(Newell(outer_loop))`. Faces under
//!   construction whose loop is degenerate (lone vertex, or a path walked
//!   out and back so the Newell sum cancels exactly) carry `surface: None`;
//!   `validate_solid` requires `Some` on every face of a finished solid.
//! - **2-manifoldness** (crate hard rule 3): maintained by the Euler
//!   operators' preconditions and checked by `validate_solid` (single radial
//!   fan per vertex, exactly two half-edges per edge).
//!
//! ## Closed curved edges and Euler–Poincaré accounting (PR-KV5a)
//!
//! Circular edges are **vertex-anchored closed edges**: a full-circle edge
//! starts and ends at one seam ("fake-edge") vertex on the circle, so it is
//! an ordinary half-edge pair whose two ends meet at the same vertex. This
//! is Stroud 2006's single-fake-edge cylinder representation (§3.1.4: "If a
//! face can extend through 360 degrees, but must have a single fake edge,
//! then a ray extended through the face is guaranteed to cut at least one
//! edge"; fig. 6.58 top/middle shows the three-face cylinder whose "two
//! planar end faces are bounded by single circular edges"), and it is
//! exactly the topology yang-rs's M5 curved fixtures use (vertex-anchored
//! `Curve::Circle` edges with `start == end`; the lateral seam edge appears
//! twice in the lateral face's loop), which makes the KV5b boolean
//! conversion mechanical.
//!
//! Concretely, a cylinder solid is `V=2, E=3, F=3, R=0, S=1, G=0`:
//! two seam vertices (one per rim), three edges (two closed rim circles +
//! one straight seam ruling), three faces (two caps whose outer loops are a
//! SINGLE circle half-edge with `next(h) == h`, plus the lateral whose loop
//! walks rim → seam up → rim → seam down, traversing the seam edge once in
//! each direction). `V − E + F − R = 2 − 3 + 3 − 0 = 2 = 2(S − G)` — the
//! standard formula holds with NO special cases: closed edges consume no
//! extra vertices and a one-half-edge loop is still one loop. `euler_counts`
//! is unchanged; what IS curved-aware is the per-face orientation validation
//! (`validate_solid`): the Newell-normal invariant applies only to faces
//! whose loops are polygonal walks; circle-bounded faces validate via the
//! directional `Curve::Circle::normal` convention instead (see [`Curve`]).
//!
//! ## Storage and determinism
//!
//! Plain index arenas: `Vec<Option<T>>` slots. Killed entities become `None`
//! and slots are **never reused** — so two identical construction sequences
//! produce bit-identical arenas (the determinism oracle relies on this).
//! Ids are slot indices; access through the checked getters returns
//! `Err(KernelV2Error::InvalidId)` for dead or out-of-range ids.
//!
//! Fields are `pub` so that higher layers and the validation tests can
//! inspect (and, in tests, deliberately corrupt) the structure. **All
//! production mutation must go through the Euler operators** in
//! [`crate::euler`] — they are the only code that upholds the invariants.

use crate::error::KernelV2Error;
use cad_primitives::Point3;
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Ids
// ---------------------------------------------------------------------------

macro_rules! define_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);

        impl $name {
            /// Slot index into the owning arena vector.
            pub fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

define_id!(
    /// Index of a [`Vertex`] slot in the arena.
    VertexId
);
define_id!(
    /// Index of a [`HalfEdge`] slot in the arena.
    HalfEdgeId
);
define_id!(
    /// Index of a [`Loop`] slot in the arena.
    LoopId
);
define_id!(
    /// Index of a [`Face`] slot in the arena.
    FaceId
);
define_id!(
    /// Index of a [`Shell`] slot in the arena.
    ShellId
);
define_id!(
    /// Index of a [`Solid`] slot in the arena.
    SolidId
);

/// Persistent identity of a topological entity, stable across rebuilds —
/// distinct from the array-index handles (`FaceId` etc.), which churn on
/// every reconstruction. KV13 (provenance / topological naming): F1 assigns
/// `Pid`s to faces; edges/vertices are derived in [`crate::pid`] (D0 items
/// 2–3).
///
/// # Two disjoint halves of the number space
///
/// A face pid comes from one of two schemes, and which one it came from is
/// readable off the id:
///
/// - **Content-seeded** (D0 item 1, the F4a reseed) — `pid >= PID_CONTENT_BASE`,
///   i.e. the top bit is set. Derived by [`crate::pid::seeded_face_pid`] from
///   the creating step's [`FaceSeed`] and the face's role inside that step, so
///   re-executing an unchanged step reproduces the same ids no matter what
///   else the arena has built. This is what a document stores.
/// - **Monotonic** (the pre-D0 scheme, still the fallback) — `pid <
///   PID_CONTENT_BASE`, handed out by [`BrepArena::alloc_pid`]. Reproducible
///   only by a full in-order rebuild: an incremental one re-runs the edited
///   step in an arena whose counter has advanced, and its faces come out
///   renamed. Used where no seed is installed — boolean outputs (whose
///   identity is their journal LINEAGE, not their own number) and raw-arena
///   tests.
///
/// The split is the reason the two schemes can never alias: a content id has
/// the top bit set and the counter is refused before it could reach there
/// (`PidSpaceExhausted`), so no hash value and no counter value are ever the
/// same number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pid(pub u64);

/// First `Pid` of the content-seeded half of the number space (`1 << 63`).
/// Monotonic ids live strictly below it; content-seeded ids at or above it.
/// See [`Pid`].
pub const PID_CONTENT_BASE: u64 = 1 << 63;

/// The stable 128-bit name of the step that is creating geometry — the seed
/// a face's content pid is derived from (D0 item 1).
///
/// The kernel keeps this opaque on purpose: it is the engine that knows what
/// a feature is, and `kernel-v2` must not learn. The adapter converts the
/// contract's `waffle_types::kernel::ConstructSeed` into this at the
/// boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FaceSeed {
    /// The step's stable name, high word first.
    pub origin: [u64; 2],
}

/// An installed [`FaceSeed`] plus the number of stamping passes it has
/// already served.
///
/// One step can build more than one solid (a sketch with two profiles
/// extrudes twice under one feature id). Every such solid's faces would
/// otherwise take the same role indices under the same seed and collide, so
/// each stamping pass consumes the next `output` ordinal. The ordinal is the
/// pass's position in the step's own construct sequence, which is as
/// deterministic as the step is — nothing global feeds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaceSeedScope {
    /// The seed installed for this step.
    pub seed: FaceSeed,
    /// Ordinal of the NEXT stamping pass under this seed.
    pub next_output: u64,
}

impl FaceSeedScope {
    /// A freshly installed scope, before any pass has stamped.
    pub fn new(seed: FaceSeed) -> Self {
        Self {
            seed,
            next_output: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Geometry carried by topology
// ---------------------------------------------------------------------------

/// A direction in 3-space. Local to kernel-v2 until `cad-primitives` grows
/// vector arithmetic; stored unit-length on [`Plane::normal`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitVector3 {
    /// X component.
    pub x: f64,
    /// Y component.
    pub y: f64,
    /// Z component.
    pub z: f64,
}

/// An (unbounded) plane: a point on the plane plus its unit normal.
///
/// The normal of a face's plane is **derived from the polygon walk** of the
/// face's outer loop (normalized Newell normal) — the walk direction is the
/// source of truth (crate hard rule 5); the stored normal is a cache whose
/// agreement is asserted at every operator exit and by `validate_solid`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    /// A point on the plane (the origin of the loop's representative
    /// half-edge at the time the plane was computed).
    pub point: Point3,
    /// Unit normal, `≡ normalize(Newell(outer_loop))`.
    pub normal: UnitVector3,
}

/// Surface descriptor carried by a face.
///
/// KV1 implements `Plane`; PR-KV5a adds `Cylinder`. The enum is
/// `#[non_exhaustive]` so further curved surfaces (sphere, cone, torus)
/// extend it without breaking downstream matches.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Surface {
    /// Planar surface.
    Plane(Plane),
    /// Infinite right-circular cylinder: axis through `axis_point` along unit
    /// `axis_dir`, of `radius`. With `reversed == false` the outward side is
    /// radially **away from the axis** (a solid cylinder) — same convention
    /// and field shape as `yang_rs::Surface::Cylinder`, so the KV5b boolean
    /// conversion is a field-for-field copy. `reversed == true` (PR-KV5b)
    /// is the cavity sense: the face's outward normal is radially **toward
    /// the axis** (the wall of a drilled pocket / through-hole) — the
    /// kernel-v2 home for yang-rs's `BRepFace::reversed` flag, which yang
    /// sets exactly on curved Subtract-subtrahend walls. Constructors
    /// always build `reversed: false`; only `from_yang_brep` produces
    /// `true`. `validate_solid`'s orientation rules flip accordingly.
    Cylinder {
        /// A point on the axis (the base-rim center as constructed).
        axis_point: Point3,
        /// Unit axis direction.
        axis_dir: UnitVector3,
        /// Cylinder radius (meters, > 0).
        radius: f64,
        /// Cavity sense: `false` = outward away from the axis (solid),
        /// `true` = outward toward the axis (cavity wall).
        reversed: bool,
    },
    /// Right-circular cone (frustum) lateral surface: the half-line of
    /// half-angle `half_angle` to the axis through `apex` along unit
    /// `axis_dir`, swept around that axis. The field shape mirrors
    /// `yang_rs::Surface::Cone` (`apex` / `axis_dir` / `half_angle`) so the
    /// KV6c boolean conversion is a field-for-field copy, plus the `reversed`
    /// cavity flag every kernel-v2 curved surface carries (see
    /// [`Surface::Cylinder`]).
    ///
    /// A point `p` lies on the surface when its axial coordinate
    /// `τ = (p − apex) · axis_dir` is `> 0` (the single nappe on the
    /// `+axis_dir` side of the apex) and its radial distance from the axis
    /// equals `τ · tan(half_angle)` (see [`crate::geom::cone_radius_at`]).
    /// With `reversed == false` the outward side faces radially **away from
    /// the axis** (a solid cone / frustum); `reversed == true` is the cavity
    /// sense — outward **toward the axis** (a conical bore wall) — exactly as
    /// for [`Surface::Cylinder`]. `half_angle ∈ (0, π/2)`.
    Cone {
        /// The apex, where the surface degenerates to a point.
        apex: Point3,
        /// Unit axis direction, oriented so on-surface points have
        /// `(p − apex) · axis_dir > 0`.
        axis_dir: UnitVector3,
        /// Half-angle between the axis and the slant, radians, `∈ (0, π/2)`.
        half_angle: f64,
        /// Cavity sense: `false` = outward away from the axis (solid),
        /// `true` = outward toward the axis (conical bore wall).
        reversed: bool,
    },
    /// Torus surface (KV6d): revolving a circle of radius `minor_radius` (the
    /// profile) about the axis through `center` along unit `axis_dir`, the
    /// profile's center tracing a circle of radius `major_radius` (the tube
    /// center circle, in the plane through `center` ⊥ the axis). A ring torus
    /// requires `major_radius > minor_radius` (the revolve axis-clearance check
    /// guarantees it).
    ///
    /// A point `p` lies on the surface when, with axial `τ = (p − center) ·
    /// axis_dir` and radial `ρ = |(p − center) − τ·axis_dir|`, the tube residual
    /// `(ρ − major_radius)² + τ² − minor_radius²` is zero
    /// ([`crate::geom::torus_residual`]). With `reversed == false` the outward
    /// normal points AWAY from the tube center circle (a solid ring); `true` is
    /// the cavity sense (a toroidal groove / subtracted tube).
    Torus {
        /// A point on the axis, in the plane of the tube center circle.
        center: Point3,
        /// Unit axis direction.
        axis_dir: UnitVector3,
        /// Major radius `R`: axis → tube center circle (`> minor_radius`).
        major_radius: f64,
        /// Minor radius `r`: the tube (profile-circle) radius (`> 0`).
        minor_radius: f64,
        /// Cavity sense: `false` = outward away from the tube center circle
        /// (solid ring), `true` = outward toward it (a toroidal groove wall).
        reversed: bool,
    },
    /// Sphere surface (KV6d increment 2, spec `kv6d_sphere_revolve.md`):
    /// `|p − center| = radius`. The field shape mirrors
    /// `yang_rs::Surface::Sphere` (`center` / `radius`) so the boolean
    /// conversion is a field-for-field copy, plus the `reversed` cavity flag
    /// every kernel-v2 curved surface carries (see [`Surface::Cylinder`]).
    ///
    /// A point `p` lies on the surface when
    /// [`crate::geom::sphere_residual`]`(p, center, radius)` is zero. With
    /// `reversed == false` the outward side is radially **away from the
    /// center** (a solid ball); `reversed == true` is the cavity sense — the
    /// wall of a spherical dimple / pocket carved by a Subtract.
    ///
    /// The closed solid sphere's seam is CANONICAL world-z-up (poles at
    /// `center ± radius·ẑ`, meridian seam arc through `center + radius·x̂`)
    /// regardless of how it was constructed — the sphere is isotropic, and
    /// the fixed frame makes the yang Stage-1 conversion (PR-YR12, a
    /// fixed z-up lat/long parameterization) a direct emission.
    Sphere {
        /// Sphere center.
        center: Point3,
        /// Sphere radius (meters, > 0).
        radius: f64,
        /// Cavity sense: `false` = outward away from the center (solid
        /// ball), `true` = outward toward it (a spherical dimple wall).
        reversed: bool,
    },
}

/// Curve descriptor carried by a half-edge, AS TRAVERSED by that half-edge.
///
/// KV1–KV4 had straight edges only (the curve was implicit); PR-KV5a makes
/// it explicit so circular rim edges can exist. Conventions:
///
/// - **Twins describe the same undirected edge in opposite directions**:
///   both `LineSegment`, or both `Circle` with identical `center`/`radius`
///   and exactly negated `normal` (checked by `validate_solid` —
///   [`crate::error::KernelV2Error::CurveTwinMismatch`]).
/// - **`Circle.normal` is directional**: it is the axis around which THIS
///   half-edge runs counterclockwise. This is the curved analog of the
///   polygon walk being the orientation source of truth: a planar cap's
///   boundary circle half-edge must have `normal ≡ face plane normal`
///   exactly as a straight outer loop's Newell normal must.
/// - **A `Circle` half-edge is a closed curve**: it starts and ends at its
///   single anchor (seam) vertex, so `origin(next(h)) == origin(h)`. A loop
///   consisting of one circle half-edge alone (`next(h) == h`) is legal —
///   that is a cap boundary.
/// - **Arcs (PR-KV5b)**: [`Curve::Arc`] is a circular arc between two
///   DISTINCT vertices, traversed counterclockwise around its directional
///   `normal` from the half-edge's origin to its destination (both on the
///   circle). The sweep angle is implied by the endpoints and is unique in
///   `(0, 2π)` for a fixed normal; the twin carries the negated normal
///   (same point set, opposite traversal). Arcs enter the arena ONLY from
///   `from_yang_brep` (yang-rs boolean outputs tag exact intersection
///   circles on per-mesh-edge arcs), which constructs minor arcs
///   (sweep < π) exclusively — a near-half-circle arc is rejected there as
///   ambiguous rather than guessed.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Curve {
    /// Straight segment from the half-edge's origin to its destination.
    LineSegment,
    /// Procedural surface-pair curve (M5, `specs/m5_surface_pair_curve.md`):
    /// the general-position quadric-pair intersection piece between the
    /// half-edge's origin and destination, defined IMPLICITLY and exactly by
    /// its two analytic surfaces ([#24] Yang et al. 2025 §4.1.2/§4.3;
    /// Constitution P8 degree-4 clarification — a procedural curve whose
    /// defining surfaces are exact IS an analytical representation).
    ///
    /// Conventions:
    /// - Both endpoints lie ON BOTH surfaces (validated per-point residual);
    ///   interior render samples are minted by Newton projection onto both
    ///   surfaces (`tessellate::surface_pair_interior_samples`), never
    ///   carried as a stored point array (the certified point sequence of
    ///   the paper is the boolean output's vertex chain itself).
    /// - Twins carry BIT-IDENTICAL `a`/`b` (like `EllipseArc` twins share
    ///   `major_axis`); traversal direction is endpoint-determined — there
    ///   is no directional normal and no minor-arc derivation.
    /// - A closed (`origin == dest`) surface-pair half-edge has no producer
    ///   and is rejected by `validate_solid`.
    /// - Placement on a `Plane` face is invalid: a transversal quadric-pair
    ///   curve is never planar (degenerate configurations produce conics
    ///   upstream in ssi-rs).
    SurfacePair {
        /// First defining surface (ssi call order, preserved verbatim).
        a: PairSurface,
        /// Second defining surface.
        b: PairSurface,
    },
    /// Full circle of `radius` about `center`, traversed counterclockwise
    /// around unit `normal`, anchored at the half-edge's origin vertex
    /// (which lies on the circle).
    Circle {
        /// Circle center.
        center: Point3,
        /// Unit axis around which this half-edge traverses CCW.
        normal: UnitVector3,
        /// Circle radius (meters, > 0).
        radius: f64,
    },
    /// Circular arc of the circle (`center`, `radius`), traversed
    /// counterclockwise around unit `normal` from the half-edge's origin to
    /// its destination (distinct vertices, both on the circle). PR-KV5b.
    Arc {
        /// Circle center.
        center: Point3,
        /// Unit axis around which this half-edge traverses CCW.
        normal: UnitVector3,
        /// Circle radius (meters, > 0).
        radius: f64,
    },
    /// Elliptical arc (PR-KV9): the exact `oblique plane ∩ cylinder`
    /// section curve. Parameterized
    /// `P(t) = center + major_radius·cos t·m̂ + minor_radius·sin t·(n̂ × m̂)`
    /// and traversed counterclockwise around the directional `normal` from
    /// the half-edge's origin to its destination (both exactly on the
    /// ellipse). `start == end` (a closed loop of one such half-edge) is
    /// the full ellipse — the oblique cap analog of `Circle`. The twin
    /// carries the negated `normal` and the SAME `major_axis` (the frame's
    /// minor direction `n̂ × m̂` flips with `n̂`, so the point set is
    /// identical, traversed oppositely). Enters the arena ONLY from
    /// `from_yang_brep` (yang tags exact intersection ellipses on
    /// per-mesh-edge arcs), which constructs minor arcs (parametric sweep
    /// < π) and full ellipses exclusively — a near-half-ellipse arc is
    /// rejected there as ambiguous rather than guessed.
    EllipseArc {
        /// Ellipse center.
        center: Point3,
        /// Unit axis (the ellipse plane normal) around which this
        /// half-edge traverses CCW.
        normal: UnitVector3,
        /// Unit direction of the major axis (in the ellipse plane).
        major_axis: UnitVector3,
        /// Semi-major radius (meters, > 0).
        major_radius: f64,
        /// Semi-minor radius (meters, > 0, ≤ `major_radius`).
        minor_radius: f64,
    },
    /// Hyperbola arc (KV16, spec `kv16_hyperbola_arc_vocabulary`): the exact
    /// axis-steep `plane ∩ cone` section piece between the half-edge's origin
    /// and destination. Parameterized
    /// `P(t) = center + a·cosh t·m̂ + b·sinh t·(n̂ × m̂)` — the single
    /// `+major_axis` (`u > 0`) branch, matching `yang_rs::Curve::Hyperbola` /
    /// `ssi_rs::SsiCurve::Hyperbola` field-for-field.
    ///
    /// Conventions (the M5 `SurfacePair` OPEN-curve conventions, NOT the
    /// ellipse's CCW-directional ones):
    /// - One branch is open and injective in `t`, so the arc between two
    ///   distinct on-branch endpoints is UNIQUE: traversal is
    ///   endpoint-determined, there is no minor-arc ambiguity and no
    ///   directional normal. Twins carry BIT-IDENTICAL fields.
    /// - A closed (`origin == dest`) hyperbola half-edge is impossible (the
    ///   branch is unbounded) and rejected by the assembler.
    /// - Both endpoints lie ON the branch (`u > 0`, residual within the
    ///   import band — validated at `from_yang_brep` and in the debug tier).
    HyperbolaArc {
        /// Hyperbola center (in the section plane; NOT on the curve).
        center: Point3,
        /// Unit section-plane normal.
        normal: UnitVector3,
        /// Unit transverse-axis direction (in-plane, toward the branch).
        major_axis: UnitVector3,
        /// Transverse semi-axis `a` (meters, > 0).
        semi_transverse: f64,
        /// Conjugate semi-axis `b` (meters, > 0).
        semi_conjugate: f64,
    },
}

/// Unsigned analytic surface descriptor for [`Curve::SurfacePair`] — the
/// curve is a point set, so unlike [`Surface`] there is NO `reversed`
/// cavity flag (orientation lives on faces, traversal on half-edges).
///
/// `#[non_exhaustive]`: the producers are general-position
/// cylinder×cylinder (M5), the cone-pair arms (cyl×cone, cone×cone; the
/// R0008/R0003/R0019 `AmbiguousCurve` class), the F10 sphere pairs, and the
/// torus arm (torus × cylinder / cone / sphere / torus,
/// `specs/m5_surface_pair_curve.md` "Torus arm"). A `Plane` operand (the
/// torus × plane spiric section) is not in the vocabulary.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum PairSurface {
    /// Infinite right-circular cylinder: `dist(x, axis) = radius`.
    Cylinder {
        /// Any point on the axis.
        axis_point: Point3,
        /// Unit axis direction.
        axis_dir: UnitVector3,
        /// Cylinder radius (meters, > 0).
        radius: f64,
    },
    /// Infinite right-circular double cone with `apex`, axis `axis_dir`, and
    /// `half_angle` α ∈ (0, π/2): the zero set of `radial − |h|·tanα` (both
    /// nappes), where `h = (x − apex)·â` and `radial = |x − apex − h·â|`.
    Cone {
        /// Cone apex.
        apex: Point3,
        /// Unit axis direction.
        axis_dir: UnitVector3,
        /// Half-angle (radians, ∈ (0, π/2)).
        half_angle: f64,
    },
    /// Sphere: the zero set of `|x − center| − radius`. Operand of the F10
    /// general-position sphere×cylinder / sphere×cone degree-4 pairs (design
    /// review 2026-07-12).
    Sphere {
        /// Sphere center.
        center: Point3,
        /// Sphere radius (meters, > 0).
        radius: f64,
    },
    /// Ring torus (M5 torus arm): the zero set of the signed distance
    /// `√((ρ − R)² + h²) − r`, `h = (x − c)·â`, `ρ = |x − c − h·â|`. Same
    /// fields as [`Surface::Torus`] minus the cavity flag (`R > r > 0`).
    Torus {
        /// A point on the axis, in the plane of the tube centre circle.
        center: Point3,
        /// Unit axis direction.
        axis_dir: UnitVector3,
        /// Major radius `R` (meters, `> minor_radius`).
        major_radius: f64,
        /// Minor radius `r` (meters, > 0).
        minor_radius: f64,
    },
}

// ---------------------------------------------------------------------------
// Topological entities
// ---------------------------------------------------------------------------

/// A vertex: geometry only. Incident half-edges are derived by scanning
/// (validation is O(n) anyway), which removes a whole class of stale-pointer
/// invariants from the mutation paths.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    /// Position (meters, like everything in Waffle Iron).
    pub point: Point3,
}

/// A directed half-edge. See module docs for the invariants.
///
/// (`Eq` was dropped when [`Curve`] — which carries `f64` geometry — moved
/// onto the half-edge in PR-KV5a; `PartialEq` remains for the determinism
/// oracle's whole-arena comparisons.)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HalfEdge {
    /// The oppositely-directed partner sharing the same undirected edge.
    pub twin: HalfEdgeId,
    /// Next half-edge around the owning loop.
    pub next: HalfEdgeId,
    /// Previous half-edge around the owning loop (`prev(next(h)) == h`).
    pub prev: HalfEdgeId,
    /// Vertex this half-edge leaves from. Its destination is
    /// `origin(next)` (== `origin(twin)`).
    pub origin: VertexId,
    /// Loop this half-edge belongs to.
    pub loop_id: LoopId,
    /// The edge's curve, as traversed by this half-edge (see [`Curve`]).
    /// Every Euler operator creates `LineSegment` half-edges; `Circle`
    /// half-edges are created only by the curved direct assemblers
    /// (`construct::extrude` on a circle profile).
    pub curve: Curve,
}

/// What bounds a loop: either a single isolated vertex (the state created by
/// `mvfs`, before any edge exists) or a closed cycle of half-edges identified
/// by a representative member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopBoundary {
    /// Loop consists of one vertex and no edges (Stroud 2006 §F.8 creates
    /// this state; §F.10 calls the inner-loop analog a vertex hole-loop).
    Lone(VertexId),
    /// Loop is a closed half-edge cycle; the id is any member of the cycle.
    Edges(HalfEdgeId),
}

/// Whether a loop is a face's perimeter or a hole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopKind {
    /// The face's perimeter loop.
    Outer,
    /// An inner loop (ring / hole-loop). Counts toward `R` in the
    /// Euler–Poincaré formula and must wind opposite to the outer loop.
    Inner,
}

/// A closed boundary cycle belonging to a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Loop {
    /// Owning face.
    pub face: FaceId,
    /// The boundary content.
    pub boundary: LoopBoundary,
    /// Outer (perimeter) or inner (ring).
    pub kind: LoopKind,
}

/// A face: one outer loop, zero or more inner loops, a surface descriptor.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    /// Surface geometry. `None` only while the face is under construction
    /// and its outer loop is degenerate (see module docs); `validate_solid`
    /// rejects finished solids containing `None`.
    pub surface: Option<Surface>,
    /// The perimeter loop.
    pub outer_loop: LoopId,
    /// Rings (hole-loops), in creation order.
    pub inner_loops: Vec<LoopId>,
    /// Owning shell.
    pub shell: ShellId,
}

/// A connected set of faces forming one closed surface of a solid.
#[derive(Debug, Clone, PartialEq)]
pub struct Shell {
    /// Owning solid.
    pub solid: SolidId,
    /// Member faces, in creation order.
    pub faces: Vec<FaceId>,
    /// Genus bookkeeping: incremented by `kfmrh` (through-hole creation).
    /// Stroud 2006 §F.9 notes genus "is not specifically represented in the
    /// object datastructure" and must otherwise be derived; we track it
    /// explicitly so the Euler–Poincaré formula is checkable at every step.
    pub genus: u32,
}

/// Who placed a solid's coordinates — and therefore which band the
/// debug-tier geometric tripwires may hold them to (spec
/// `si5_geometry_provenance_tier.md`).
///
/// The construction tripwires ([`crate::validate::PLANARITY_DEBUG_TOLERANCE`],
/// [`crate::validate::CURVED_SURFACE_DEBUG_TOLERANCE`]) state their own
/// premise: geometry is exact *by construction*, because the assembler places
/// it from closed form. SI5's `ingest_analytic` is the first producer for
/// which that premise is false — its coordinates are an exchange file's own
/// roundings, and a file can disagree with itself (ABC `00000007` writes a
/// `CIRCLE` radius at ten significant digits against an anchor vertex at
/// fifteen, 4.6e-11 apart). Before this type the validator inferred the tier
/// from the curve FORM — a `Curve::Arc` endpoint got the import band, a
/// `Curve::Circle` anchor got the construction band — which was sound only
/// while the arena's constructors and `recover` were the only producers of a
/// full circle.
///
/// The ordering is a lattice: `Asserted > Constructed`, and the join is taken
/// wherever geometry meets (a boolean whose operand was ingested, a
/// transformed copy). A producer that FORGETS to carry `Asserted` tightens a
/// band and gets a loud [`crate::KernelV2Error::VertexOffSurface`]; it can
/// never silently admit worse geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum GeometryProvenance {
    /// The kernel itself: the arena's constructors from closed form
    /// (primitives, extrude, revolve, sweep, pipe) or the boolean reassembly.
    /// Exact by construction, which is exactly what the construction
    /// tripwires assert. Within such a solid the existing split by curve form
    /// still applies and is still correct — `recover`-minted canonical
    /// circles are construction-exact, Stage-4-relocated patch vertices are
    /// f64-computed and band at the import tier.
    #[default]
    Constructed,
    /// An exchange file somebody else wrote ([`crate::ingest_analytic`]).
    /// Every coordinate is the file's own rounding, so every debug-tier band
    /// floors at the import tier — the same band this validator already
    /// applies to the arc endpoints of the same files. The claim that a
    /// vertex lies on its own curve is not dropped: three PRODUCTION gates
    /// bracket it — `ingest`'s on-surface gate and seam-anchor reconciliation
    /// and the rim-radius agreement in the face validators (spec §4, measured
    /// as a sweep).
    Asserted,
}

/// A solid: one or more shells.
#[derive(Debug, Clone, PartialEq)]
pub struct Solid {
    /// Member shells (first is the peripheral shell), in creation order.
    pub shells: Vec<ShellId>,
    /// Who placed this solid's coordinates. See [`GeometryProvenance`] — it
    /// is a field rather than a side table so that every producer has to say
    /// which it is, at the point where it knows.
    pub provenance: GeometryProvenance,
}

impl Solid {
    /// A solid the kernel built itself ([`GeometryProvenance::Constructed`]).
    pub fn constructed(shells: Vec<ShellId>) -> Self {
        Self {
            shells,
            provenance: GeometryProvenance::Constructed,
        }
    }

    /// A solid assembled from an exchange file's own numbers
    /// ([`GeometryProvenance::Asserted`]).
    pub fn asserted(shells: Vec<ShellId>) -> Self {
        Self {
            shells,
            provenance: GeometryProvenance::Asserted,
        }
    }
}

// ---------------------------------------------------------------------------
// Arena
// ---------------------------------------------------------------------------

/// Slot-arena storage for the whole B-Rep. See module docs for the storage
/// and determinism contract.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BrepArena {
    /// Vertex slots (`None` == killed).
    pub vertices: Vec<Option<Vertex>>,
    /// Half-edge slots.
    pub half_edges: Vec<Option<HalfEdge>>,
    /// Loop slots.
    pub loops: Vec<Option<Loop>>,
    /// Face slots.
    pub faces: Vec<Option<Face>>,
    /// Shell slots.
    pub shells: Vec<Option<Shell>>,
    /// Solid slots.
    pub solids: Vec<Option<Solid>>,
    /// Persistent-id allocator (monotonic; KV13 F1). Part of the canonical
    /// arena state, so identical construction sequences produce identical
    /// `Pid`s (the determinism oracle compares whole arenas).
    pub next_pid: u64,
    /// Persistent id per live face (KV13 F1). Edges/vertices are deferred to
    /// F1b. Faces reused by a body split keep their existing `Pid`. A
    /// `BTreeMap` (not `HashMap`) so its `Debug` iteration order is
    /// deterministic — the determinism oracle compares arena debug strings.
    pub face_pids: BTreeMap<FaceId, Pid>,
    /// Operation journal (KV13 F2): per-operation evolution of face `Pid`s.
    /// Append-only; deterministic given operation order. `FaceOrigin` (F3)
    /// walks its `modified` edges back to a `generated` origin.
    pub journal: Vec<crate::journal::Evolution>,
    /// The content seed for faces stamped by the construct step now running
    /// (D0 item 1). `None` ⇒ [`assign_face_pids`](Self::assign_face_pids)
    /// falls back to [`alloc_pid`](Self::alloc_pid).
    ///
    /// Installed and restored by the caller through
    /// [`set_face_seed`](Self::set_face_seed) — it is part of the canonical
    /// arena state (the determinism oracle compares whole arenas), so a run
    /// that installs the same seeds in the same order compares equal and one
    /// that does not is meant to differ.
    pub face_seed: Option<FaceSeedScope>,
}

macro_rules! checked_getters {
    ($get:ident, $get_mut:ident, $vec:ident, $ty:ty, $id:ty, $kind:literal) => {
        /// Checked read access; `Err(InvalidId)` for dead or out-of-range ids.
        pub fn $get(&self, id: $id) -> Result<&$ty, KernelV2Error> {
            self.$vec
                .get(id.index())
                .and_then(|slot| slot.as_ref())
                .ok_or(KernelV2Error::InvalidId { kind: $kind })
        }

        /// Checked write access; `Err(InvalidId)` for dead or out-of-range ids.
        pub fn $get_mut(&mut self, id: $id) -> Result<&mut $ty, KernelV2Error> {
            self.$vec
                .get_mut(id.index())
                .and_then(|slot| slot.as_mut())
                .ok_or(KernelV2Error::InvalidId { kind: $kind })
        }
    };
}

impl BrepArena {
    /// Empty arena.
    pub fn new() -> Self {
        Self::default()
    }

    checked_getters!(vertex, vertex_mut, vertices, Vertex, VertexId, "vertex");
    checked_getters!(
        half_edge,
        half_edge_mut,
        half_edges,
        HalfEdge,
        HalfEdgeId,
        "half_edge"
    );
    checked_getters!(loop_, loop_mut, loops, Loop, LoopId, "loop");
    checked_getters!(face, face_mut, faces, Face, FaceId, "face");
    checked_getters!(shell, shell_mut, shells, Shell, ShellId, "shell");
    checked_getters!(solid, solid_mut, solids, Solid, SolidId, "solid");

    /// Number of live vertices in the whole arena.
    pub fn num_vertices(&self) -> usize {
        self.vertices.iter().flatten().count()
    }

    /// Number of live half-edges in the whole arena.
    pub fn num_half_edges(&self) -> usize {
        self.half_edges.iter().flatten().count()
    }

    /// Number of live undirected edges (`half-edges / 2`).
    pub fn num_edges(&self) -> usize {
        self.num_half_edges() / 2
    }

    /// Number of live faces in the whole arena.
    pub fn num_faces(&self) -> usize {
        self.faces.iter().flatten().count()
    }

    /// Number of live inner loops (rings / hole-loops) in the whole arena.
    pub fn num_rings(&self) -> usize {
        self.loops
            .iter()
            .flatten()
            .filter(|l| l.kind == LoopKind::Inner)
            .count()
    }

    /// Allocate a fresh persistent id from the monotonic half of the number
    /// space (KV13 F1).
    ///
    /// Refuses rather than crossing into the content-seeded half
    /// ([`PID_CONTENT_BASE`]), which is what makes "a counter id and a hash
    /// id are never the same number" true by construction rather than by
    /// luck. Reaching the wall needs 2^63 allocations in one arena, so the
    /// refusal is a guarantee, not a budget.
    pub fn alloc_pid(&mut self) -> Result<Pid, KernelV2Error> {
        if self.next_pid >= PID_CONTENT_BASE {
            return Err(KernelV2Error::PidSpaceExhausted);
        }
        let p = Pid(self.next_pid);
        self.next_pid += 1;
        Ok(p)
    }

    /// Install the content seed for the faces the current construct step
    /// stamps, returning the scope it replaced so the caller can restore it
    /// (D0 item 1).
    ///
    /// Pass `None` around any operation whose output identity is NOT the
    /// step's own content — a boolean, whose output faces are named by their
    /// journal lineage — otherwise those faces would take role indices under
    /// the step's seed and compete with the faces the step actually created.
    pub fn set_face_seed(&mut self, seed: Option<FaceSeed>) -> Option<FaceSeedScope> {
        std::mem::replace(&mut self.face_seed, seed.map(FaceSeedScope::new))
    }

    /// Restore a scope previously returned by [`Self::set_face_seed`],
    /// including how many passes it had already served.
    pub fn restore_face_seed(&mut self, scope: Option<FaceSeedScope>) {
        self.face_seed = scope;
    }

    /// The persistent id of a face, if one has been assigned (KV13 F1).
    pub fn face_pid(&self, face: FaceId) -> Option<Pid> {
        self.face_pids.get(&face).copied()
    }

    /// Assign a persistent id to every face of `solid` that lacks one.
    /// Constructors call this at their exit (`finalize_solid`) so a finished
    /// solid's faces are all tagged; faces reused by a body split already
    /// carry a `Pid` and keep it.
    ///
    /// # The role index
    ///
    /// With a seed installed ([`Self::set_face_seed`]) each face's id is
    /// derived from `(seed, output ordinal, role)` by
    /// [`crate::pid::seeded_face_pid`]. The **role** is the face's position
    /// in the solid's own face list in ascending `FaceId` order — a purely
    /// LOCAL index, which is the whole point: the `FaceId`s themselves are
    /// arena-global and march on as other steps build, but their relative
    /// order inside one constructor's output is a function of that
    /// constructor's creation sequence alone.
    ///
    /// That sequence is the face's semantic role, constructor by
    /// constructor. For a polygon [`crate::extrude`] it is top cap = 0, base
    /// cap = 1, then one lateral per profile edge in profile order, then the
    /// hole walls; a circle extrude assembles its three faces base, top,
    /// lateral. Each is a fixed function of its own constructor, and the two
    /// differ — so the orders are MEASURED and pinned by
    /// `tests/d0_face_seed.rs` rather than asserted as a general rule, and
    /// that is where to look before relying on a particular number. "Cap
    /// top/bottom, lateral by profile-edge index" (spec §4 item 1) is what
    /// the index expresses; it is not re-derived from geometry, so nothing
    /// about it can drift with a coordinate.
    ///
    /// Already-stamped faces consume their role index and are then skipped,
    /// so a second pass over the same solid (a constructor that finalizes an
    /// intermediate body first) leaves the earlier faces' ids alone and
    /// numbers only the new ones. `FaceId`s are append-only, so an earlier
    /// face's role never moves.
    ///
    /// # What a seeded pid is unique WITHIN
    ///
    /// One solid — never the whole arena, and deliberately so. A rebuild
    /// re-executes features into the SAME arena, leaving the previous
    /// incarnation's solid orphaned but still in `face_pids`; a seeded id is
    /// a function of the step and the role, so the new incarnation's faces
    /// take exactly the ids the orphan holds. That repetition is the
    /// property, not a fault — "the same step always names its faces the
    /// same way" is what a reopened document needs — and every consumer
    /// already looks a pid up inside one body (`pid::solid_pids`,
    /// `all_entity_pids`, and `resolve_by_pid`, which refuses rather than
    /// searching other bodies).
    ///
    /// # Refusals
    ///
    /// - [`KernelV2Error::PidCollision`] if a derived id is already on
    ///   ANOTHER face of this same solid. Two faces of one step with the
    ///   same role cannot happen (a role is a position in a deduped list),
    ///   so within one pass this can only be a 64-bit hash collision; it can
    ///   also catch a face stamped by an earlier pass whose id the current
    ///   seed would re-mint. Either way it is a refusal, never an alias: two
    ///   faces of one body sharing a name would let a stored reference
    ///   resolve to whichever came first.
    /// - [`KernelV2Error::PidSpaceExhausted`] from the monotonic fallback.
    pub fn assign_face_pids(&mut self, solid: SolidId) -> Result<(), KernelV2Error> {
        let shells = self.solid(solid)?.shells.clone();
        let mut faces: Vec<FaceId> = Vec::new();
        for sh in shells {
            faces.extend(self.shell(sh)?.faces.iter().copied());
        }
        faces.sort_unstable_by_key(|f| f.0);
        faces.dedup();

        let Some(scope) = self.face_seed else {
            for f in faces {
                if !self.face_pids.contains_key(&f) {
                    let p = self.alloc_pid()?;
                    self.face_pids.insert(f, p);
                }
            }
            return Ok(());
        };

        // A pass is only "used" if it actually stamps something, so a
        // constructor that finalizes a solid whose faces are all already
        // stamped does not burn an output ordinal.
        let mut stamped_any = false;
        let mut in_solid: std::collections::BTreeSet<Pid> = faces
            .iter()
            .filter_map(|f| self.face_pids.get(f))
            .copied()
            .collect();
        for (role, f) in faces.into_iter().enumerate() {
            if self.face_pids.contains_key(&f) {
                continue;
            }
            let p = crate::pid::seeded_face_pid(scope.seed, scope.next_output, role as u64);
            if !in_solid.insert(p) {
                return Err(KernelV2Error::PidCollision { kind: "face" });
            }
            self.face_pids.insert(f, p);
            stamped_any = true;
        }
        if stamped_any {
            if let Some(s) = self.face_seed.as_mut() {
                s.next_output += 1;
            }
        }
        Ok(())
    }

    /// Walk the half-edge cycle of a loop, starting at its representative.
    ///
    /// Returns the empty vector for a [`LoopBoundary::Lone`] loop. Returns
    /// `Err(LoopNotClosed)` if the `next` orbit fails to return to the
    /// representative within the number of live half-edges (a corrupted
    /// arena), or `Err(InvalidId)` if the walk hits a dead slot.
    pub fn loop_half_edges(&self, loop_id: LoopId) -> Result<Vec<HalfEdgeId>, KernelV2Error> {
        let lp = self.loop_(loop_id)?;
        let start = match lp.boundary {
            LoopBoundary::Lone(_) => return Ok(Vec::new()),
            LoopBoundary::Edges(h) => h,
        };
        let mut out = Vec::new();
        let mut cur = start;
        let budget = self.num_half_edges();
        loop {
            out.push(cur);
            cur = self.half_edge(cur)?.next;
            if cur == start {
                return Ok(out);
            }
            if out.len() > budget {
                return Err(KernelV2Error::LoopNotClosed { loop_id });
            }
        }
    }

    /// Origin points of a loop's half-edge cycle, in walk order.
    pub fn loop_points(&self, loop_id: LoopId) -> Result<Vec<Point3>, KernelV2Error> {
        self.loop_half_edges(loop_id)?
            .into_iter()
            .map(|h| Ok(self.vertex(self.half_edge(h)?.origin)?.point))
            .collect()
    }

    /// Euler–Poincaré element counts for one solid (entities reachable from
    /// its shells).
    pub fn euler_counts(&self, solid: SolidId) -> Result<EulerCounts, KernelV2Error> {
        let solid_ref = self.solid(solid)?;
        let mut vertices = std::collections::BTreeSet::new();
        let mut half_edges: usize = 0;
        let mut faces: usize = 0;
        let mut rings: usize = 0;
        let mut genus: u32 = 0;
        let shells = solid_ref.shells.len();
        for &sh in &solid_ref.shells {
            let shell = self.shell(sh)?;
            genus += shell.genus;
            for &f in &shell.faces {
                faces += 1;
                let face = self.face(f)?;
                let mut loops = vec![face.outer_loop];
                loops.extend(face.inner_loops.iter().copied());
                for lid in loops {
                    let lp = self.loop_(lid)?;
                    if lp.kind == LoopKind::Inner {
                        rings += 1;
                    }
                    match lp.boundary {
                        LoopBoundary::Lone(v) => {
                            vertices.insert(v);
                        }
                        LoopBoundary::Edges(_) => {
                            for h in self.loop_half_edges(lid)? {
                                half_edges += 1;
                                vertices.insert(self.half_edge(h)?.origin);
                            }
                        }
                    }
                }
            }
        }
        Ok(EulerCounts {
            v: vertices.len() as i64,
            e: (half_edges / 2) as i64,
            f: faces as i64,
            r: rings as i64,
            s: shells as i64,
            g: genus as i64,
        })
    }
}

/// Element counts entering the Euler–Poincaré formula
/// `V − E + F − R = 2(S − G)` (Stroud 2006 §4, rule 4, with
/// v/e/f/h/b/g spelled V/E/F/R/S/G here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EulerCounts {
    /// Vertices.
    pub v: i64,
    /// Edges (half-edge pairs).
    pub e: i64,
    /// Faces.
    pub f: i64,
    /// Rings (inner / hole-loops; Stroud's `h`).
    pub r: i64,
    /// Shells (Stroud's multiplicity `b`).
    pub s: i64,
    /// Genus.
    pub g: i64,
}

impl EulerCounts {
    /// Left-hand side `V − E + F − R`.
    pub fn lhs(&self) -> i64 {
        self.v - self.e + self.f - self.r
    }

    /// Right-hand side `2(S − G)`.
    pub fn rhs(&self) -> i64 {
        2 * (self.s - self.g)
    }

    /// Whether the Euler–Poincaré formula holds.
    pub fn holds(&self) -> bool {
        self.lhs() == self.rhs()
    }
}
