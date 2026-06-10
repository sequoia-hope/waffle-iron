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

    // ----- profile validation (produced by `Profile::new`) ----------------
    //
    // Loop indexing convention for the variants below: `loop_index == 0` is
    // the outer loop; `loop_index == k + 1` is hole loop `k`.
    /// A profile coordinate, the plane origin, or a basis vector contains a
    /// non-finite component (NaN or ±∞).
    ProfileNotFinite,

    /// The profile plane's basis vectors do not span a plane: `u × v` is
    /// (numerically exactly) zero. See `profile::BASIS_MIN_SQ_CROSS_NORM`.
    ProfileDegenerateBasis,

    /// A profile loop has fewer than 3 vertices.
    ProfileTooFewVertices { loop_index: usize },

    /// A profile loop repeats a vertex consecutively (exact coordinate
    /// equality, including last == first — the closing edge is implicit).
    ProfileRepeatedVertex { loop_index: usize },

    /// A profile loop is not a simple polygon: two of its edges intersect,
    /// touch, or overlap (decided EXACTLY via dashu rational arithmetic —
    /// see the `profile` module docs for the simplicity-validation
    /// decision). Also covers degenerate zero-area loops: a loop that
    /// passes the simplicity check provably encloses nonzero area.
    ProfileNotSimple { loop_index: usize },

    /// Two distinct profile loops intersect or touch (exact check).
    ProfileLoopsIntersect { loop_a: usize, loop_b: usize },

    /// A hole loop does not lie strictly inside the outer loop
    /// (exact point-in-polygon on a witness vertex; valid because loop
    /// disjointness has already been established).
    ProfileHoleNotInsideOuter { hole_index: usize },

    /// One hole loop lies inside another (exact check); nested holes are
    /// not a meaningful profile.
    ProfileHolesNested {
        outer_hole: usize,
        inner_hole: usize,
    },

    // ----- constructor argument validation (extrude) -----------------------
    /// `extrude` distance must be finite and strictly positive.
    ExtrudeNonPositiveDistance,

    /// `extrude` direction is zero or non-finite, so it has no direction.
    ExtrudeDegenerateDirection,

    /// `extrude` direction is (numerically) parallel to the profile plane:
    /// `|d̂ · n̂| < construct::EXTRUDE_MIN_NORMAL_COSINE`. Sweeping within
    /// the plane produces no volume.
    ExtrudeDirectionInPlane,

    // ----- boolean delegation (PR-KV3, `boolean::boolean_op`) -------------
    /// The boolean inputs contain a coplanar face pair (touching or
    /// overlapping on a shared plane). The cherchi-rs arrangement defers
    /// coplanar pairs (`ArrangementError::CoplanarPairDeferred`); handling
    /// them is Yang 2025 §4.5.5 Stage-0 coplanar preprocessing — roadmap
    /// milestone M8, not yet implemented. Typed and loud so callers can
    /// distinguish this KNOWN Phase-3/M8 boundary from a pipeline bug.
    UnsupportedCoplanar,

    /// The yang-rs boolean pipeline failed for a non-coplanar reason. The
    /// payload is the yang error's full Display text — loud, no masking,
    /// no retry, no tolerance fallback (P9/P10).
    BooleanFailed(String),

    /// The boolean result is empty (e.g. intersection of disjoint solids).
    /// kernel-v2 has no empty solid; callers treat this as "no body".
    EmptyBooleanResult,

    /// A yang-rs output face carries a non-planar surface. Phase 4a
    /// reassembles planar output only; curved reassembly arrives with the
    /// curved-primitive constructors. Carries the yang output face index.
    UnsupportedBooleanOutputSurface { face: usize },

    /// The yang-rs output B-Rep could not be reassembled into a kernel-v2
    /// solid: a structural defect (open or discontinuous loop, an
    /// undirected edge not used by exactly two opposite directed edges,
    /// a degenerate or plane-disagreeing loop normal, non-integral genus).
    /// This is a REAL pipeline finding — surfaced loudly, never repaired
    /// silently (P9). The payload names the violated condition.
    InvalidBooleanOutput(&'static str),

    // ----- render tessellation (PR-KV3, `tessellate`) ----------------------
    /// Planar-face tessellation failed: the exact ear-clipping pass could
    /// not find a valid hole bridge or a clippable ear. Unreachable for the
    /// valid (weakly simple) loops a validated solid carries; surfaced
    /// loudly instead of looping or guessing (P9). The payload names the
    /// failing step.
    TessellationFailed { face: FaceId, reason: &'static str },

    /// RED-phase stub marker (PR-KV3). Removed at GREEN.
    NotImplemented(&'static str),

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
