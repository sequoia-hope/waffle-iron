//! The measurement contract — `KernelMeasure`, §4.1 of
//! `specs/agent_mechanical_design.md`.
//!
//! Consumers (wasm-bridge's `measure_*` tools, and the rule check when K1
//! lands) ask geometric questions through this trait; `kernel_v2` answers
//! them, `MockKernel` refuses them typed. Every answer carries the
//! [`Method`] it was obtained by, so a mesh number is never presented as a
//! measurement.
//!
//! The trait grows one method per Q increment: Q1 `distance`, Q2
//! `interference`, Q3 `mass_properties`, Q5 `thickness` and Q6 `edge_length`
//! have landed. Methods default to `NotSupported`, so each addition is
//! additive for every implementor.
//!
//! Q4 (`section_with_plane`) is NOT here: a section cuts the solid, so it
//! lives on [`super::projection::KernelProjection`] with the rest of the
//! kernel's plane work, and the Q4 tool is a thin reader over it.

use super::types::{KernelError, KernelId, KernelSolidHandle};
use crate::TopoKind;

/// The default density for [`KernelMeasure::mass_properties`], kg/m³.
///
/// **1, not a real material.** The document model carries no material table
/// (checked 2026-10-03: nothing in `feature-engine` or `file-format` names a
/// density), so there is nothing to read one from. With the default,
/// [`MassProperties::mass`] is numerically the volume and
/// [`MassProperties::inertia_at_centroid`] is the volume-weighted tensor —
/// both scale linearly in `density`, so a caller who knows the material
/// multiplies, or passes it. When M1's material table lands, the tool reads
/// the body's own density and this stays the fallback.
pub const DEFAULT_DENSITY_KG_M3: f64 = 1.0;

/// What a measurement is taken from or to.
///
/// Named `MeasureEntity` rather than the spec's bare `Entity`: this module is
/// glob-re-exported from `waffle_types::kernel`, where a type called `Entity`
/// would collide with the sketch and provenance vocabularies.
///
/// Not `PartialEq`: a [`KernelSolidHandle`] is an opaque transient handle, and
/// comparing two of them says nothing about the bodies they name.
#[derive(Debug, Clone)]
pub enum MeasureEntity {
    /// A whole body.
    Solid(KernelSolidHandle),
    /// One face.
    Face(KernelId),
    /// One edge.
    Edge(KernelId),
    /// One vertex.
    Vertex(KernelId),
    /// A free point in space, in meters.
    Point([f64; 3]),
    /// An infinite axis: a point on it and a direction.
    Axis {
        origin: [f64; 3],
        direction: [f64; 3],
    },
}

/// Options for [`KernelMeasure::distance`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DistanceOpts {
    /// `None` (the default) is the MINIMUM distance. `Some(direction)`
    /// projects both operands onto the direction and reports the gap along
    /// it, negative when they overlap along it.
    pub along: Option<[f64; 3]>,
}

/// How a measured number was obtained.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Method {
    /// From the analytic geometry, and certified: the number cannot be
    /// improved.
    Exact,
    /// From a tessellation, within `chord_bound` meters of the analytic
    /// value. The consumer must carry this band rather than re-derive one.
    Mesh { chord_bound: f64 },
}

/// The entity a measured point lies on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntityRef {
    pub entity: KernelId,
    pub kind: TopoKind,
}

/// The answer to [`KernelMeasure::distance`].
#[derive(Debug, Clone, PartialEq)]
pub struct Distance {
    /// Meters. 0 when the operands touch or overlap; negative only for an
    /// `along` query whose operands overlap along the direction.
    pub value: f64,
    /// The closest point on the first operand, then on the second.
    pub points: [[f64; 3]; 2],
    /// The face / edge / vertex each of those points lies on. `None` for a
    /// free point or axis operand, which lies on no entity.
    pub on: [Option<EntityRef>; 2],
    /// Which tier `value` is.
    pub method: Method,
}

// ---------------------------------------------------------------------------
// Q2 — interference
// ---------------------------------------------------------------------------

/// One lump of the intersection region (Q2).
///
/// The region solid itself is NOT handed back: the query runs the Intersect in
/// a scratch arena of its own and drops it, so there is no id in the live
/// arena to name (the spec's `region: Option<SolidId>` and the tool's
/// `keep_region` wait for a kernel that can adopt a solid from one arena into
/// another). What survives is the measurable content of each lump.
#[derive(Debug, Clone, PartialEq)]
pub struct InterferenceBody {
    /// The lump's volume in m³, from the kernel's own integral over its B-Rep.
    pub volume: f64,
    /// Its centroid, in meters.
    pub centroid: [f64; 3],
    /// Its axis-aligned bounds, in meters — where in the assembly to look.
    pub aabb: [[f64; 3]; 2],
}

/// Why a pair was judged to be in [`Interference::Contact`] (Q2).
///
/// Both arms mean the same thing — the bodies share boundary but no interior —
/// and they are kept apart because they are reached by different evidence and
/// a caller auditing a close call needs to know which.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContactEvidence {
    /// The regularized Intersect came back EMPTY (no material in common) and
    /// the Q1 distance between the operands is zero: they meet on a set of
    /// measure zero — a shared face, edge or vertex.
    EmptyIntersectionAtZeroDistance,
    /// The Intersect produced a solid, but its volume is at or under the
    /// workspace's minimum-feature floor: a degenerate sliver, not shared
    /// interior. `volume` is what was measured, so the caller can see how
    /// close to the floor the call was.
    SliverIntersection { volume: f64 },
}

/// The answer to [`KernelMeasure::interference`] (Q2 of
/// `specs/agent_mechanical_design.md` §4.2).
///
/// Three outcomes, and nothing else: a boolean the kernel cannot run is a
/// typed `KernelError`, never a fourth arm and never silently `Disjoint`.
#[derive(Debug, Clone, PartialEq)]
pub enum Interference {
    /// The bodies share interior volume.
    Interferes {
        /// Total volume of the intersection in m³ — the sum over `bodies`.
        volume: f64,
        /// One entry per lump of the intersection region.
        bodies: Vec<InterferenceBody>,
        /// The tier `volume` and the per-lump numbers are (the intersection
        /// region of two curved operands is a partial-patch B-Rep, which the
        /// moment integrator reads at the mesh tier).
        method: Method,
    },
    /// The bodies touch but share no interior.
    Contact {
        evidence: ContactEvidence,
        /// The zero-distance witness: where they touch, and on what.
        closest: Distance,
    },
    /// The bodies do not touch. `distance` is Q1's answer, always filled in —
    /// never a silent nothing.
    Disjoint { distance: Distance },
}

// ---------------------------------------------------------------------------
// Q3 — mass properties
// ---------------------------------------------------------------------------

/// The answer to [`KernelMeasure::mass_properties`] (Q3 of
/// `specs/agent_mechanical_design.md` §4.2).
///
/// Every number is in the world frame and in SI (meters, kg/m³, kg, kg·m²).
/// `method` covers ALL of them at once: volume, area, centroid and inertia
/// come out of one integration over the same faces, so they cannot be at
/// different tiers.
#[derive(Debug, Clone, PartialEq)]
pub struct MassProperties {
    /// m³.
    pub volume: f64,
    /// m². From the same face integration as the moments — NOT from
    /// `solid_surface_area`, whose closed form refuses an arc-bounded patch
    /// (SI5 measured 411 of 709 exact shells with no closed-form area). A
    /// shell that has no closed form still gets an area here, at `method`.
    pub surface_area: f64,
    /// The centroid of the volume, meters.
    pub centroid: [f64; 3],
    /// The inertia tensor about the centroid, in the world axes, kg·m²:
    /// `I[i][i] = ρ∫(x_j² + x_k²)dV`, `I[i][j] = −ρ∫x_i x_j dV`.
    pub inertia_at_centroid: [[f64; 3]; 3],
    /// The eigenvalues of `inertia_at_centroid`, ascending.
    pub principal_moments: [f64; 3],
    /// The unit eigenvector of each principal moment, as rows, in the same
    /// order. Right-handed (the third row is the cross product of the first
    /// two) so the triple is a usable frame.
    pub principal_axes: [[f64; 3]; 3],
    /// The density the inertia and mass were scaled by, kg/m³.
    /// [`DEFAULT_DENSITY_KG_M3`] unless the caller passed one.
    pub density: f64,
    /// `density × volume`, kg.
    pub mass: f64,
    /// Which tier every number above is.
    pub method: Method,
}

// ---------------------------------------------------------------------------
// Q6 — edge arc length
// ---------------------------------------------------------------------------

/// How an arc length was obtained (Q6 of `specs/agent_mechanical_design.md`
/// §4.2).
///
/// Three tiers rather than [`Method`]'s two, because an edge has a third case
/// that neither of those describes honestly: a curve whose speed has a closed
/// form but whose INTEGRAL does not (an ellipse, a hyperbola — both elliptic
/// integrals). Calling such a number `Exact` would overclaim and calling it
/// `Mesh` would understate it, so it says what it is and carries a measured
/// witness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LengthMethod {
    /// A closed form evaluated in f64: a line's chord, a circle's `2πr`, a
    /// circular arc's `rΔθ`. The number cannot be improved.
    Exact,
    /// A convergent quadrature of a closed-form speed function.
    ///
    /// `residual` is a MEASURED Richardson estimate of this value's own
    /// error, derived from the same quadrature run at twice the step count
    /// (`(16/15)·|I₂ₙ − Iₙ|` for the `O(h⁴)` Simpson rule the implementation
    /// uses, floored at a few ulp of the value). Reported so a consumer can
    /// see when the integrand is hard rather than trusting a constant.
    ///
    /// **It is an estimate, not a proven bound.** Richardson assumes the step
    /// is already in the asymptotic regime; a near-kinked integrand is not.
    /// Measured against a 2 000 000-interval reference, it covers the error on
    /// every smooth arc and lands about 11 % BELOW it on a hyperbola arc whose
    /// semi-conjugate is 1e-4 of its semi-transverse. Read it as an order of
    /// magnitude on the accuracy, never as a tolerance to compute with. (The
    /// implementation's own accuracy census for the ellipse arm is on
    /// `Curve2::length`; `kernel_v2::measure`'s
    /// `the_hyperbola_arms_residual_is_measured_against_a_reference_integral`
    /// pins the hyperbola arm and the under-statement above.)
    Quadrature { residual: f64 },
    /// The sum of a sampled polyline's chords, which is a LOWER bound on the
    /// true arc length (a chord is never longer than the arc it subtends).
    ///
    /// `chord_bound` is the sampling band in meters when the sampler was ours
    /// — the render chord band, the same one [`Method::Mesh`] carries — and
    /// `None` when the polyline arrived from outside (a mesh-backed imported
    /// body), where we do not know what it was sampled at and will not invent
    /// a number for it.
    Chords { chord_bound: Option<f64> },
}

/// The arc length of one edge, and what kind of curve it is (Q6).
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeLength {
    /// Arc length in meters — the length of the whole curve, not its chord.
    pub value: f64,
    /// The analytic family, as one lowercase token: `line`, `circle`, `arc`,
    /// `ellipse_arc`, `hyperbola_arc`, `surface_pair` (an SSI curve defined
    /// by its two surfaces), or `polyline` (an imported body's sampled edge).
    pub curve_type: &'static str,
    /// Whether the edge closes on itself — a full circle or ellipse, whose
    /// two endpoints are one seam vertex.
    pub closed: bool,
    /// Which tier `value` is.
    pub method: LengthMethod,
}

// ---------------------------------------------------------------------------
// Q5 — sampled wall thickness
// ---------------------------------------------------------------------------

/// Options for [`KernelMeasure::thickness`] (Q5).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThicknessOpts {
    /// The largest gap allowed between neighbouring sample sites on one face,
    /// in meters. `None` asks for the kernel's own default, which the answer
    /// reports either way.
    ///
    /// A rule that needs to catch a thin web asks for a spacing under its
    /// width: a feature narrower than the spacing can sit between two sites
    /// and never be sampled, which is why the answer carries the number.
    pub spacing: Option<f64>,
}

/// One site a thickness was measured at (Q5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThicknessSite {
    /// The wall thickness there, in meters: the distance from `point` to
    /// `opposite` along the inward normal at `point`.
    pub thickness: f64,
    /// Where the ray started, on `from`.
    pub point: [f64; 3],
    /// Where it landed, on `to`.
    pub opposite: [f64; 3],
    /// The face the site sits on.
    pub from: EntityRef,
    /// The face the inward ray hit.
    pub to: EntityRef,
    /// Whether [`Self::from`] and [`Self::to`] are two DISTINCT faces that
    /// share an edge — so this reading crossed a CORNER rather than a wall.
    ///
    /// Two faces meeting at an edge enclose a wedge of material that goes to
    /// zero at the edge, so a cast between them measures how close the site is
    /// to that edge. A site that hits its OWN face is not such a reading: a
    /// solid cylinder measures its diameter across its own lateral face, which
    /// is a wall.
    pub faces_share_an_edge: bool,
}

/// One bar of a thickness histogram (Q5): equal-width bins over
/// `[min, max]`, counted in SITES rather than in area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThicknessBin {
    pub lo: f64,
    pub hi: f64,
    pub count: usize,
}

/// Sites that produced no thickness, by reason (Q5).
///
/// Counted rather than dropped: a body most of whose casts fail has an answer
/// covering much less of it than the sample count suggests, and nothing else
/// in the result would say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThicknessDeclines {
    /// The inward ray left the body without hitting a face — a shell that is
    /// not closed, or a cast that grazed out along a tangency.
    pub no_hit: usize,
    /// The only hit was on the site's OWN face, within a few times the local
    /// sagitta of the facet the site was taken from, so it cannot be told
    /// from the ray's own start. A wall that thin is below what a cast seeded
    /// on a render tessellation can resolve, and saying nothing about it
    /// would be worse than counting it.
    pub below_self_band: usize,
    /// The face carries no analytic surface to take an inward normal from.
    pub no_surface: usize,
}

/// How a thickness was obtained (Q5).
///
/// One arm, deliberately. A sampled minimum is an UPPER bound on the body's
/// true minimum wall — a thinner spot between two sites is simply not looked
/// at — so there must be no way for one to be reported as anything else. An
/// exact medial axis is not in scope and would add its own arm here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThicknessMethod {
    Sampled {
        /// Sites that produced a thickness.
        samples: usize,
        /// The spacing those sites were laid out at, meters.
        spacing: f64,
    },
}

/// The answer to [`KernelMeasure::thickness`] (Q5 of
/// `specs/agent_mechanical_design.md` §4.2).
///
/// Every number is in meters and SAMPLED — see [`ThicknessMethod`].
#[derive(Debug, Clone, PartialEq)]
pub struct Thickness {
    /// The smallest thickness found. An UPPER bound on the body's true
    /// minimum wall.
    ///
    /// This is §4.2's question answered exactly as posed — the shortest cast
    /// along an inward normal anywhere on the body — and it is dominated by
    /// any ACUTE edge, where the material is a sliver: a 4 mm slot through a
    /// 10/7 mm tube reports 0.043 mm here against a 3 mm wall. For the wall,
    /// read [`Self::min_wall`].
    pub min: f64,
    /// The smallest thickness among sites whose two faces do NOT share an
    /// edge ([`ThicknessSite::faces_share_an_edge`]) — the thinnest WALL,
    /// with every corner reading left out, and the number a wall-thickness
    /// rule wants.
    ///
    /// `None` when every site crossed a corner, so there is no wall the
    /// sample found — never a 0 or an infinity standing in for one.
    ///
    /// The exclusion is coarse on purpose: it drops every reading between two
    /// faces that meet ANYWHERE, so a tapered rib whose flanks meet at a tip
    /// edge does not contribute its own thickness here either. That is the
    /// conservative direction for a rule, and [`Self::min`] with
    /// [`Self::thinnest`]'s own faces is what a caller judges such a rib
    /// from.
    pub min_wall: Option<f64>,
    /// The unweighted mean over sites. The sites are approximately
    /// area-uniform — every facet is subdivided to the spacing — so this
    /// approximates the area-weighted mean wall.
    pub mean: f64,
    pub max: f64,
    /// Where [`Self::min`] is, and between which two faces.
    pub thinnest: ThicknessSite,
    /// Where [`Self::min_wall`] is. `None` on the same bodies that number is.
    pub thinnest_wall: Option<ThicknessSite>,
    pub histogram: Vec<ThicknessBin>,
    /// The tessellation band the sites were derived at, meters — the same
    /// `RENDER_CHORD_TOLERANCE_REL × extent` band [`Method::Mesh`] carries.
    pub chord_bound: f64,
    /// Sites whose hit was refined onto the analytic surface and certified
    /// there. The rest kept their facet hit, which is inside the true surface
    /// by at most [`Self::chord_bound`].
    pub refined: usize,
    pub declines: ThicknessDeclines,
    pub method: ThicknessMethod,
}

/// Geometric measurement over bodies the kernel holds (§4.1).
pub trait KernelMeasure {
    /// Minimum distance (or the gap along `opts.along`) between `a` and `b`,
    /// with the closest point on each and what each lies on.
    ///
    /// Typed `NotSupported` for an operand this kernel cannot measure — an
    /// infinite axis and a mesh-backed imported body in the Q1 increment —
    /// never a number obtained some other way.
    fn distance(
        &self,
        _a: &MeasureEntity,
        _b: &MeasureEntity,
        _opts: &DistanceOpts,
    ) -> Result<Distance, KernelError> {
        Err(KernelError::NotSupported {
            operation: "distance measurement".to_string(),
        })
    }

    /// Whether `a` and `b` share interior volume, touch, or are apart (Q2).
    ///
    /// `&self`, not the spec's `&mut self`: the implementation runs the
    /// Intersect on COPIES in a scratch arena, so the live arena — and, just
    /// as importantly, its provenance journal — is untouched by a query. A
    /// query that wrote the journal would renumber face pids and move every
    /// later reference.
    ///
    /// A boolean the kernel cannot run (a coplanar Stage-0 refusal, a curved
    /// partial-patch operand, a reassembly STOP) is the typed `KernelError`
    /// the boolean raised, named as such. It is NEVER reported as
    /// [`Interference::Disjoint`]: "the kernel could not tell" and "they do
    /// not touch" are different answers, and a clearance check that confused
    /// them would pass a collision.
    fn interference(
        &self,
        _a: &KernelSolidHandle,
        _b: &KernelSolidHandle,
    ) -> Result<Interference, KernelError> {
        Err(KernelError::NotSupported {
            operation: "interference query".to_string(),
        })
    }

    /// Volume, surface area, centroid and the inertia tensor about the
    /// centroid of `solid`, scaled by `density` (Q3).
    ///
    /// `density` of `None` means [`DEFAULT_DENSITY_KG_M3`] — the document
    /// model has no material table to read one from, so the answer says which
    /// density it used rather than pretending to know the material.
    fn mass_properties(
        &self,
        _solid: &KernelSolidHandle,
        _density: Option<f64>,
    ) -> Result<MassProperties, KernelError> {
        Err(KernelError::NotSupported {
            operation: "mass properties".to_string(),
        })
    }

    /// The arc length of one edge, its analytic family, and the tier the
    /// length is (Q6).
    ///
    /// This is the length of the CURVE. `TopoSignature::length` — what
    /// `compute_signature` reports for an edge — is the straight-line
    /// distance between its endpoints, which is 0 for a full circle; the two
    /// are different quantities and a consumer asking "how long is this rim"
    /// wants this one.
    ///
    /// `edge` must be an edge id. A kind mismatch is `EntityNotFound`, not a
    /// guess.
    fn edge_length(&self, _edge: KernelId) -> Result<EdgeLength, KernelError> {
        Err(KernelError::NotSupported {
            operation: "edge arc length".to_string(),
        })
    }

    /// The wall thickness of `solid`, by casting inward from points sampled on
    /// its own faces (Q5).
    ///
    /// This is the medial-axis question answered by SAMPLING, and the answer
    /// never claims to be anything else: [`Thickness::method`] has one arm,
    /// [`ThicknessMethod::Sampled`], carrying the site count and the spacing,
    /// so a consumer can see that a feature narrower than the spacing could
    /// have been missed. An exact medial axis is not in scope.
    ///
    /// Two minima come back, and a wall-thickness decision belongs to the
    /// second: [`Thickness::min`] is the shortest cast anywhere, which any
    /// acute edge drives towards zero, and [`Thickness::min_wall`] is the
    /// shortest cast that did not cross a corner.
    ///
    /// `Err` when no site produced a thickness at all — a body whose every
    /// cast failed is not a body with no walls, and reporting a `min` of 0 or
    /// of infinity for it would be a number nothing measured.
    fn thickness(
        &self,
        _solid: &KernelSolidHandle,
        _opts: &ThicknessOpts,
    ) -> Result<Thickness, KernelError> {
        Err(KernelError::NotSupported {
            operation: "wall thickness".to_string(),
        })
    }
}
