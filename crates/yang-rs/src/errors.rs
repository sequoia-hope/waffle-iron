//! Yang pipeline error types (extracted verbatim from lib.rs — spec
//! `specs/yang_rs_lib_decomposition.md`, increment 1).

use std::error::Error;
use std::fmt;

use crate::{BoolOp, InputId};

// =========================================================================
// Errors
// =========================================================================

/// Errors from the yang-rs pipeline.
#[derive(Debug)]
pub enum YangError {
    /// Input is not 2-manifold. **Not yet detected** in PR-YR2.
    NonManifoldInput,
    /// Reassembly would produce a non-2-manifold result. PR-YR3+.
    NonManifoldOutput,
    /// The mesh boolean backend (sidecar or native) failed.
    MeshBooleanFailed(Box<dyn Error + Send + Sync>),
    /// B-Rep topology is malformed: face with <3 edges, out-of-range
    /// vertex/edge index, etc. PR-YR2.
    MalformedTopology(String),
    /// A face is geometrically degenerate (zero-area / collinear loop):
    /// its Newell polygon normal has magnitude below `MIN_FEATURE_SIZE`,
    /// so its winding cannot be canonicalized. M1 (Stage-1 orientation).
    DegenerateFace { face: usize },
    /// Geometric face resolution failed for a kept arrangement triangle
    /// (M3, Stage 6). Either the triangle's centroid lies on no input face
    /// plane / ties between ≥2 planes within `TAU_WORK`, or (PR-YR26) a
    /// multi-solid-labeled triangle has no matching Stage-0 pair plane —
    /// in-scope coplanar overlaps now resolve via the §4.5.5 Stage-0
    /// overlay instead of erroring here. P9: fail loud, never a silent
    /// `None`.
    FaceResolutionFailed { tri: usize },
    /// The requested boolean op is not yet supported by the M3 pipeline.
    /// Currently only `Xor` (its symmetric-difference result is multi-shell /
    /// has a void that `reconstruct_topology` cannot reassemble yet — deferred
    /// from M3, spec §Scope). Fails loud rather than producing a generic
    /// `NonManifoldOutput` or a silently-wrong result (P9).
    UnsupportedOp(BoolOp),
    /// An input B-Rep face carries a curved surface (`Surface::Sphere`,
    /// `Cylinder`, or `Cone`). The face is well-formed, but the pipeline does
    /// not yet process curved geometry (PR-YR6 added the curved variants as
    /// types only). Carries the offending input B-Rep `face` index. This is a
    /// P9/P10 LOUD rejection — never a panic, silent skip, or planar
    /// approximation. Curved processing arrives in a later PR.
    CurvedSurfaceNotYetSupported { face: usize },
    /// PR-YR9 (P3): Stage-3 SSI refinement of an output intersection edge
    /// failed. The edge `(start, end)` (canonical mesh-vertex indices) lies on
    /// two input surfaces of DIFFERENT inputs; converting them to analytical
    /// quadrics and selecting the unique `ssi-rs` intersection curve passing
    /// through both endpoints did not yield exactly one curve. P9/P10 LOUD —
    /// never a silent fallback to `Curve::LineSegment`. Carries `reason`.
    SsiRefinementFailed {
        edge: (u32, u32),
        reason: SsiRefinementError,
    },
    /// PR-YR10 (Stage 4, §4.5.3): the reversed-intersection correction sweep
    /// could not resolve a reversal at `vertex` on intersection edge `edge`
    /// by collapsing successive next-points. A P9/P10 LOUD stop — genuine
    /// §4.5.2 local-refinement territory, never a silently-emitted inverted
    /// mesh.
    Stage4ReversalUnresolved { edge: (u32, u32), vertex: u32 },
    /// PR-YR10 (Stage 4, §4.4.1 / §4.5): a relocation region around `vertex`
    /// could not be made valid. `reason` names the specific failure. A P9/P10
    /// LOUD stop — never a tolerance widening, silent snap, or fallback path.
    Stage4RegionInvalid {
        vertex: u32,
        reason: Stage4InvalidReason,
    },
    /// PR-YR24/PR-YR26: input faces `face_a` (of solid `input_a`) and
    /// `face_b` (of solid `input_b`) are coplanar — bit-exactly or within a
    /// sub-model-resolution band — with overlapping AABBs, AND the case is
    /// in the UNSUPPORTED RESIDUE of Yang 2025 §4.5.5 Stage-0 coplanar
    /// preprocessing. Since PR-YR26 (M8 slice b) planar A×B pairs are
    /// HANDLED (`stage0::stage0_preprocess`: canonical-plane snap + exact
    /// 2D overlay + identical overlap meshes), so this error remains only
    /// for: intra-solid near pairs (`input_a == input_b`, the CHAINED form
    /// — a previous exact boolean's output re-imported with internal
    /// near-but-not-bit-identical face planes, see `scan_near_coplanar`),
    /// curved faces in a pair, a face in MORE than one pair, neighbor
    /// faces whose subdivided ring cannot be re-triangulated (holes /
    /// non-continuous loops / no valid fan apex), and overlay engine
    /// failures (e.g. `RoundingCollapse` on sub-ulp in-plane slivers). A
    /// P9/P10 LOUD boundary — never a silent wrong result.
    CoplanarFacesUnsupported {
        input_a: InputId,
        face_a: usize,
        input_b: InputId,
        face_b: usize,
    },

    /// #178 (spec `yang_178_subres_coplanar_gap_stop.md`): Stage-0's
    /// near-coplanar scan matched a cross A×B face pair whose planes are
    /// genuinely DISTINCT — separated by a NONZERO orientation-aligned
    /// offset gap above the coincidence-authoring noise class `band/100`
    /// yet inside the detection band `max(TAU_MODEL, scale·TAU_WORK)`. The
    /// volume between them is a sub-resolution feature (below the
    /// `MIN_FEATURE_SIZE` input contract) that the §4.5.5 overlay would
    /// silently dissolve — the measured C0111/C0113 wall dissolve: χ 0→2
    /// with green watertight/volume oracles. A P10 LOUD stop:
    /// out-of-contract input is rejected, never welded (the R0091 trap
    /// class).
    SubResolutionCoplanarGap {
        face_a: usize,
        face_b: usize,
        gap: f64,
        band: f64,
    },

    /// #172 half (b) (spec `yang_172_case_iii_graze_guard.md`): two
    /// cross A×B cylinder lateral surfaces genuinely INTERSECT at a
    /// penetration `depth` above the coincidence-authoring noise class
    /// yet below the observability floor of any practical rim
    /// tessellation (`floor` = the combined chord sagitta at the
    /// N-cap): Yang Fig. 8 Case III with no finite refinement answer.
    /// Emitting would be silent-wrong topology (the meshes never see
    /// the intersection, so the output stays unfused while the true
    /// surfaces interpenetrate — below the #173 render gate's sagitta
    /// too). A P10 LOUD stop, the graze mirror of
    /// [`Self::SubResolutionCoplanarGap`].
    SubSagittaGrazeIntersection {
        face_a: usize,
        face_b: usize,
        depth: f64,
        floor: f64,
    },
}

/// PR-YR10 (Stage 4): why a relocation region could not be made valid.
///
/// Each variant is a P9/P10 LOUD stop — the boolean returns
/// [`YangError::Stage4RegionInvalid`] rather than silently snapping a point,
/// widening a tolerance, or emitting an inverted / degenerate mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stage4InvalidReason {
    /// The mesh crossing point's residual to the exact curve exceeds the
    /// Stage-1 chord bound `d_ε` — beyond the relocation budget, so it is not
    /// this mesh-boolean output's own crossing point and snapping would lie.
    OffCurveBeyondChordBand,
    /// Radial projection onto the circle is degenerate: the point projects
    /// onto the circle axis (`ρ_radial < MIN_FEATURE_SIZE`).
    OnAxis,
    /// The intersection edge carries a `Curve::Ellipse`; closed-form ellipse
    /// relocation (a quartic) is not implemented in this PR. (Circle-only.)
    EllipseProjectionUnsupported,
    /// A relocated triangle's winding disagrees with its analytic surface
    /// normal (`dot ≤ 0`) — an inverted triangle the §4.5.3 sweep could not fix.
    InvertedTriangle,
    /// A relocated triangle's area dropped below `MIN_FEATURE_SIZE²`.
    DegenerateTriangle,
    /// A §4.5.3 loop shrank below 3 vertices during collapse.
    LoopTooSmall,
    /// Relocate + §4.5.3 correction left the region invalid; genuine §4.5.2
    /// local refinement (re-invoking the Stage-2 backend on a refined sub-mesh)
    /// is required and is out of scope for this PR (loud STOP).
    LocalRefinementRequired,
}

/// PR-YR9 (P3): why Stage-3 SSI refinement of an intersection edge failed.
///
/// Each variant is a P9/P10 LOUD stop — the boolean returns
/// [`YangError::SsiRefinementFailed`] rather than silently emitting a
/// mesh-approximate polyline on a genuine analytical failure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SsiRefinementError {
    /// `ssi_rs::intersect` returned an error for a surface pair we expected to
    /// intersect (e.g. degenerate input).
    IntersectFailed(ssi_rs::SsiError),
    /// Selecting the unique on-curve solution failed: `matched` of `candidates`
    /// returned curves pass through BOTH edge endpoints within tolerance, and
    /// `matched != 1` (zero or ≥2). Never pick the first / nearest (P10).
    AmbiguousCurve { candidates: usize, matched: usize },
    /// The selected curve is a `Parabola`/`Hyperbola` (defensive — cannot occur
    /// for the Cylinder∩Plane pair this PR handles).
    UnsupportedCurve,
    /// One of the two incident surfaces is a `Sphere`/`Cone`, which has no
    /// supported analytical SSI in this PR (defensive).
    UnsupportedSurfaceForSsi,
}

impl fmt::Display for YangError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonManifoldInput => write!(f, "yang-rs: input B-Rep is not 2-manifold"),
            Self::NonManifoldOutput => {
                write!(f, "yang-rs: reassembled output would be non-2-manifold")
            }
            Self::MeshBooleanFailed(source) => {
                write!(f, "yang-rs: mesh boolean backend failed: {source}")
            }
            Self::MalformedTopology(msg) => write!(f, "yang-rs: malformed B-Rep topology: {msg}"),
            Self::DegenerateFace { face } => {
                write!(
                    f,
                    "yang-rs: face {face} is degenerate (zero-area / collinear)"
                )
            }
            Self::FaceResolutionFailed { tri } => {
                // PR-YR27: precise text — the old "coplanar multi-solid
                // label" wording mislabeled the (much more common) membership
                // failures as coplanarity issues.
                write!(
                    f,
                    "yang-rs: geometric face resolution failed for kept triangle {tri} \
                     (centroid off all face surfaces, a membership tie unresolved by \
                     finite-extent containment, or a multi-solid label with no \
                     matching Stage-0 pair plane)"
                )
            }
            Self::UnsupportedOp(op) => {
                write!(
                    f,
                    "yang-rs: operation {op:?} not yet supported \
                     (XOR multi-shell reassembly deferred — M3)"
                )
            }
            Self::CurvedSurfaceNotYetSupported { face } => {
                write!(
                    f,
                    "yang-rs: face {face} has a curved surface (Sphere/Cylinder/Cone) \
                     which is not yet supported by the pipeline"
                )
            }
            Self::SsiRefinementFailed { edge, reason } => {
                write!(
                    f,
                    "yang-rs: Stage-3 SSI refinement failed for intersection edge \
                     {edge:?}: {reason:?}"
                )
            }
            Self::Stage4ReversalUnresolved { edge, vertex } => {
                write!(
                    f,
                    "yang-rs: Stage-4 §4.5.3 reversed-intersection correction could not \
                     resolve a reversal at vertex {vertex} on edge {edge:?}"
                )
            }
            Self::Stage4RegionInvalid { vertex, reason } => {
                write!(
                    f,
                    "yang-rs: Stage-4 relocation region around vertex {vertex} is invalid: \
                     {reason:?}"
                )
            }
            Self::CoplanarFacesUnsupported {
                input_a,
                face_a,
                input_b,
                face_b,
            } => {
                write!(
                    f,
                    "yang-rs: input faces {input_a:?}#{face_a} and {input_b:?}#{face_b} are \
                     coplanar (within the sub-model-resolution band) — coplanar boolean \
                     requires Yang 2025 §4.5.5 Stage-0 preprocessing (M8), not yet supported"
                )
            }
            Self::SubResolutionCoplanarGap {
                face_a,
                face_b,
                gap,
                band,
            } => {
                write!(
                    f,
                    "yang-rs: faces A#{face_a} and B#{face_b} are two DISTINCT parallel \
                     planes separated by {gap:.3e} — inside the coplanar detection band \
                     ({band:.3e}) but above rounding noise: a sub-resolution feature the \
                     Stage-0 overlay would silently dissolve; input is outside the \
                     MIN_FEATURE_SIZE contract (#178)"
                )
            }
            Self::SubSagittaGrazeIntersection {
                face_a,
                face_b,
                depth,
                floor,
            } => {
                write!(
                    f,
                    "yang-rs: cylinder faces A#{face_a} and B#{face_b} intersect at a \
                     penetration depth of {depth:.3e} — a genuine graze below the mesh \
                     observability floor ({floor:.3e}, the combined chord sagitta at the \
                     rim-N cap): no practical tessellation can sample the intersection \
                     (Yang Fig. 8 Case III), so the boolean would emit unfused topology \
                     that silently ignores it (#172)"
                )
            }
        }
    }
}

impl Error for YangError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::MeshBooleanFailed(source) => Some(&**source),
            _ => None,
        }
    }
}
