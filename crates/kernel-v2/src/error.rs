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

    // ----- circle profile validation (produced by `Profile::circle`) ------
    /// A circle profile's radius must be finite and strictly positive.
    ProfileCircleNonPositiveRadius,

    /// A circle profile's plane frame `(u, v)` must be orthonormal
    /// (`|u| = |v| = 1`, `u · v = 0` within
    /// `profile::CIRCLE_FRAME_ORTHONORMALITY_TOLERANCE`): the embedding
    /// `origin + x·u + y·v` maps the plane-coordinate circle to a true 3D
    /// circle of the same radius **only** for an isometric frame — a skewed
    /// or scaled frame would silently turn the circle into an ellipse
    /// (elliptic cylinders are out of the KV5a vocabulary).
    ProfileCircleFrameNotOrthonormal,

    // ----- constructor argument validation (extrude) -----------------------
    /// `extrude` distance must be finite and strictly positive.
    ExtrudeNonPositiveDistance,

    /// `extrude` direction is zero or non-finite, so it has no direction.
    ExtrudeDegenerateDirection,

    /// `extrude` direction is (numerically) parallel to the profile plane:
    /// `|d̂ · n̂| < construct::EXTRUDE_MIN_NORMAL_COSINE`. Sweeping within
    /// the plane produces no volume.
    ExtrudeDirectionInPlane,

    /// `extrude` of a circle profile along a direction oblique to the
    /// profile-plane normal (`|d̂ × n̂| > construct::CIRCLE_EXTRUDE_MAX_AXIS_SINE`)
    /// would produce an **elliptic** cylinder, which is out of the KV5a
    /// surface vocabulary — typed and loud, never approximated. (Corpus
    /// check 2026-06-11: all 339 extrudes across the 53 circle-bearing assay
    /// cases carry `direction: null` — i.e. along the sketch-plane normal —
    /// so right cylinders are corpus-complete.)
    ExtrudeObliqueCircleUnsupported,

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

    /// A boolean input solid contains curved geometry (a non-planar face
    /// surface or a non-segment edge curve). The kernel-v2 ↔ yang-rs curved
    /// conversion is PR-KV5b; until it lands, curved boolean inputs are
    /// rejected loudly here rather than mistranslated as planar.
    UnsupportedCurvedBoolean { face: FaceId },

    // ----- render tessellation (PR-KV3, `tessellate`) ----------------------
    /// Planar-face tessellation failed: the exact ear-clipping pass could
    /// not find a valid hole bridge or a clippable ear. Unreachable for the
    /// valid (weakly simple) loops a validated solid carries; surfaced
    /// loudly instead of looping or guessing (P9). The payload names the
    /// failing step.
    TessellationFailed { face: FaceId, reason: &'static str },

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

    /// A half-edge's curve disagrees with its twin's: twins must describe
    /// the same undirected edge in opposite directions (both `LineSegment`,
    /// or both `Circle` with identical center/radius and exactly negated
    /// normals), and a `Circle` half-edge must close on its own origin
    /// vertex (`origin(next(h)) == origin(h)`).
    CurveTwinMismatch { half_edge: HalfEdgeId },

    /// A face's curved orientation/consistency invariants failed — the
    /// curved analog of [`KernelV2Error::NewellMismatch`]. Production-tier:
    /// e.g. a planar cap's circle half-edge normal does not equal the face
    /// normal (outer) / its negation (ring); a cylinder face is not bounded
    /// by exactly two full-circle rims; a rim circle's normal is not along
    /// the cylinder axis, its radius disagrees with the surface, or its
    /// traversal axis does not point toward the opposite rim (outward
    /// orientation). The payload names the violated condition.
    CurvedGeometryMismatch { face: FaceId, reason: &'static str },

    /// Debug-tier finding only: a loop vertex of a curved face is further
    /// from the analytic surface than the documented debug tolerance —
    /// the curved analog of [`KernelV2Error::NonPlanarFace`]. See
    /// `validate::CURVED_SURFACE_DEBUG_TOLERANCE`.
    VertexOffSurface { face: FaceId },

    /// `introspect::face_plane` was asked for the plane of a face whose
    /// surface is not planar.
    FaceNotPlanar { face: FaceId },

    /// Temporary RED-phase stub marker (PR-KV5a): the typed surface exists
    /// but the implementation lands in the GREEN commit. Removed at GREEN.
    NotImplemented(&'static str),
}

impl core::fmt::Display for KernelV2Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for KernelV2Error {}
