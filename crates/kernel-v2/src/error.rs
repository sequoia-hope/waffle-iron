//! Typed errors for kernel-v2 topology construction and validation.
//!
//! Per the crate hard rules (crates/kernel-v2/CLAUDE.md):
//! - No `panic!` in production paths — every failure is a `Result` with a
//!   variant from this enum.
//! - Operations that would produce non-manifold topology return
//!   [`KernelV2Error::NonManifoldTopology`]. No silent repair.

use crate::arena::{FaceId, HalfEdgeId, LoopId, VertexId};

/// Error type for all kernel-v2 topology operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelV2Error {
    /// RED-phase stub marker. Removed when the operator is implemented.
    NotImplemented(&'static str),

    /// An entity id does not refer to a live arena slot. `kind` names the
    /// entity class ("vertex", "half_edge", "loop", "face", "shell", "solid").
    InvalidId { kind: &'static str },

    /// The requested operation would produce (or constitutes) non-2-manifold
    /// topology. The payload describes the violated condition.
    ///
    /// Per hard rule 3 (crates/kernel-v2/CLAUDE.md): no silent repair —
    /// the arena is left unmodified.
    NonManifoldTopology(&'static str),

    /// `mef` was given two half-edges that do not lie in the same loop.
    /// Connecting vertices in different loops is the province of other Euler
    /// operators (Stroud 2006 §F.4: same loop → MEF; different loops of the
    /// same face → MEKH; different objects → MEKFB; anything else is an
    /// incorrectly specified operation).
    MefDifferentLoops,

    /// The operation would create an edge from a vertex to itself.
    DegenerateEdge,

    /// The operation would create a face whose outer-loop Newell normal is
    /// (numerically) zero, so the `face.normal ≡ Newell(outer_loop)`
    /// invariant could not be established (hard rule 2).
    DegenerateFaceNormal,

    /// `mev_lone` was applied to a loop that already has edges. The lone-vertex
    /// entry point is only valid on the empty loop produced by `mvfs`.
    LoopNotLone,

    /// `kfmrh` was given the same face as both kill and receive argument.
    KfmrhSameFace,

    /// `kfmrh` requires the face being killed to have no inner loops
    /// (Stroud 2006 §F.9: "the presence of hole-loops in the face to be
    /// killed is treated as an error condition").
    KfmrhFaceHasRings,

    /// `kfmrh` (KV1 slice) requires both faces to lie in the same shell —
    /// the genus-increasing case of Stroud 2006 §F.9. Shell-merging and
    /// object-merging interpretations are out of scope.
    KfmrhDifferentShells,

    // ----- validation findings (produced by `validate_solid`) ------------
    /// A face reachable from the validated solid has no surface descriptor.
    /// Finished solids must have `Some(Surface)` on every face.
    FaceWithoutSurface { face: FaceId },

    /// A face's stored plane normal does not match the Newell normal of its
    /// outer loop (hard rule 2 violation).
    NewellMismatch { face: FaceId },

    /// An inner loop (ring) of a face does not wind opposite to the face's
    /// outer loop (its Newell normal is not antiparallel to the face normal).
    RingWindingMismatch { face: FaceId, ring: LoopId },

    /// `twin(twin(he)) != he`, or `he.twin == he`, or the twin is dead.
    TwinPairingBroken { half_edge: HalfEdgeId },

    /// A loop's `next` cycle does not close back on its representative
    /// half-edge, or `prev`/`next` are not mutually consistent, or a member
    /// half-edge does not point back at the loop.
    LoopNotClosed { loop_id: LoopId },

    /// The half-edges leaving a vertex do not form a single radial fan
    /// (2-manifold vertex condition).
    NonManifoldVertex { vertex: VertexId },

    /// Euler–Poincaré bookkeeping failed: `V − E + F − R != 2(S − G)`
    /// (Stroud 2006 §4, rule 4; written there as v − e + f − h = 2(b − g)).
    EulerFormulaViolation { lhs: i64, rhs: i64 },

    /// Debug-tier finding only: a vertex of a face's loop is further from the
    /// face plane than the documented debug tolerance. See
    /// `validate::PLANARITY_DEBUG_TOLERANCE` for why this is not a production
    /// correctness gate.
    NonPlanarFace { face: FaceId },
}

impl core::fmt::Display for KernelV2Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for KernelV2Error {}
