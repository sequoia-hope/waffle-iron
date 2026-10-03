//! The measurement contract — `KernelMeasure`, §4.1 of
//! `specs/agent_mechanical_design.md`.
//!
//! Consumers (wasm-bridge's `measure_*` tools, and the rule check when K1
//! lands) ask geometric questions through this trait; `kernel_v2` answers
//! them, `MockKernel` refuses them typed. Every answer carries the
//! [`Method`] it was obtained by, so a mesh number is never presented as a
//! measurement.
//!
//! The trait grows one method per Q increment (Q1 distance; Q2 interference,
//! Q3 mass properties, Q5 thickness and Q6 `edge_length` add theirs in their
//! own increments). Methods default to `NotSupported`, so each addition is
//! additive for every implementor.

use super::types::{KernelError, KernelId, KernelSolidHandle};
use crate::TopoKind;

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
}
