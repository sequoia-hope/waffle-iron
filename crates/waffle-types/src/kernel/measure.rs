//! The measurement contract — `KernelMeasure`, §4.1 of
//! `specs/agent_mechanical_design.md`.
//!
//! Consumers (wasm-bridge's `measure_*` tools, and the rule check when K1
//! lands) ask geometric questions through this trait; `kernel_v2` answers
//! them, `MockKernel` refuses them typed. Every answer carries the
//! [`Method`] it was obtained by, so a mesh number is never presented as a
//! measurement.
//!
//! The trait grows one method per Q increment (Q1 distance, Q2 interference
//! and Q3 mass properties have landed; Q5 thickness and Q6 `edge_length` add
//! theirs in their own increments). Methods default to `NotSupported`, so each
//! addition is additive for every implementor.

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
}
