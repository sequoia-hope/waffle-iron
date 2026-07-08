//! Yang 2025 hybrid B-Rep / mesh boolean pipeline.
//!
//! ## Scope (aspirational)
//!
//! Implements the pipeline described in Yang et al. 2025, "A robust hybrid
//! Boolean operations method for mesh-and-surface hybrid models":
//!
//! - **Stage 0** (§4.5.5): Coplanar preprocessing
//! - **Stage 1** (§4.1): Bijective tessellation — PR-YR2: planar B-Reps;
//!   PR-YR7: cylinder; PR-YR12: sphere (Cone still rejects loudly)
//! - **Stage 2** (§4.2): Mesh boolean — delegate to `cherchi-rs`
//! - **Stage 3** (§4.3): SSI refinement — delegate to `ssi-rs`
//! - **Stage 4** (§4.4.1): Mesh updating — RELOCATION of intersection crossings
//!   onto the exact curve (+ §4.5.3 reversed-point sweep), watertightness
//!   inherited from the mesh boolean. The paper's CDT remesh / split-merge-insert
//!   is **NOT implemented** (deviation N2 in `docs/yang_deviations.md`); the
//!   sidecar's trimmed mesh is trusted and `check_watertight_2manifold` gates the
//!   output. Likewise §4.5.4 illegal-self-intersection removal is **NOT
//!   implemented** (deviation N6, roadmap-tracked).
//! - **Stage 5** (§4.4.2): Patch segmentation (flood-fill)
//! - **Stage 6** (§4.4.2): B-Rep reassembly
//!
//! ## Current implementation status (PR-YR5)
//!
//! - **Stage 1 PLANAR** (PR-YR2): `BRep::new(verts, edges, faces)`
//!   fan-triangulates each planar face from its first vertex; produces
//!   a 1:1 bijection (no Steiner points). Convex faces only; no inner
//!   loops; `Surface::Plane` only.
//! - **`boolean()` vertex provenance** (PR-YR3): every output mesh
//!   vertex is spatially matched against input A then B (within
//!   [`MATCH_TOLERANCE`]). On match, the corresponding input's
//!   `TessellationSource` is copied; unmatched verts get
//!   `TessellationSource::Intersection`.
//! - **`boolean()` triangle attribution** (PR-YR4): every output
//!   triangle is attributed to an input `(InputId, face_idx)` via
//!   majority-vote (≥2 of 3) over the vertices' provenance.
//!   Accessible via [`BRep::triangle_attribution`].
//! - **`boolean()` topology reconstruction** (PR-YR5): output `BRep`
//!   gets non-empty `vertices` (1:1 with mesh), `edges`, and `faces`
//!   via patch flood-fill on triangle attribution + boundary cycle
//!   recovery + surface inheritance from input faces.
//!   None-attributed (cut surface) triangles are intentionally
//!   skipped — output is a "kept-portions skeleton."
//! - **`BRep::from_mesh()` degenerate path** (PR-YR1 compat): empty
//!   topology; all-`Unknown` TessellationMap; empty
//!   TriangleAttributionMap.
//!
//! **Honest framing**: PR-YR3 + PR-YR4 + PR-YR5 are NOT real Yang
//! Stage 5/6. Real Stage 5/6 needs per-triangle labels from Stage 2's
//! arrangement which the C++ sidecar doesn't expose. The current
//! pipeline is a sidecar-feasible substitute.
//!
//! **PR-YR5 output is intentionally NOT 2-manifold** (rule-4
//! deviation): faces cover input-derived ("kept") portions only.
//! Cut-surface faces (`None`-attributed triangles → new BRepFaces with
//! reconstructed surfaces) are PR-YR6, which also re-enables the
//! 2-manifold contract.
//!
//! Banked for future PRs:
//! - PR-YR2b: ear-cutting for non-convex faces
//! - PR-YR2c: inner loops (holes) — currently → `NonManifoldOutput`
//! - PR-YR2d: curved surfaces (`Surface::Cylinder`, `Sphere`, NURBS)
//! - PR-YR2e: Steiner points + dε tolerance
//! - PR-YR2f: CDT at shared edges
//! - PR-YR4b: precomputed vertex→edge / edge→face incidence indices
//! - PR-YR5b: edge deduplication across faces (each face owns its edges in v1)
//! - PR-YR5c: inner-loop / hole support in patch boundary recovery
//! - PR-YR6: cut-surface face generation + 2-manifold validation
//! - PR-YR7+: edge curve recovery beyond `Curve::LineSegment`
//! - Real Stage 5/6: gated on labeled arrangement output
//!
//! ## Input / output
//!
//! - Input: two B-Rep solids (`BRep`)
//! - Output: one B-Rep solid
//! - Non-manifold detection is **not yet implemented** in PR-YR2.
//!
//! ## References
//!
//! - Yang et al. 2025 — `refs/text/yang2025_hybrid_boolean.txt`

use std::error::Error;
use std::fmt;

// Stage 0 (Yang §4.5.5) coplanar-overlay geometric engine — M8 slice a
// (PR-YR25). NOT yet wired into `boolean()`; that's M8 slice b.
pub mod coplanar_overlay;
mod stage0;
// N2 increment 2: the §4.1.2 / Fig 6 per-triangle `d(T)` bound + its pinned
// parametric embedding. NOT yet wired into `stage4_relocate_and_correct`;
// that is N2-3. Spec: `specs/n2_stage4_dt_recompute.md`.
pub mod stage4_dt;
// N2 increment 1: the §4.4.1 mesh-updating primitive (Fig 11 split/merge/insert
// + interior-constraint CDT). NOT yet wired into `stage4_relocate_and_correct`;
// that is N2-3. Spec: `specs/n2_stage4_mesh_updating.md`.
pub mod stage4_update;

pub use cad_primitives::{BoolOp, Point3, Vector3};
pub use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
pub use cherchi_rs::{Mesh, MeshBoolean};
pub use cherchi_rs::{NativeBoolean, NativeBooleanError};
// The constrained-Delaunay primitive, re-exported for the kernel-v2 render
// tessellation cores (its `tessellate.rs` patch/planar triangulation). kernel-v2
// may depend on yang-rs but NOT on cherchi-rs directly, so it consumes the CDT
// through this seam — the same pattern as `NativeBoolean` above and the torus
// UV-patch consumer's existing use of this primitive.
pub use cherchi_rs::triangulation::{
    cdt_polygon_with_holes, cdt_polygon_with_holes_floodfill, CdtError,
};
// `ArrangementError` is re-exported so that kernel-v2 (whose dep rules allow
// `yang-rs` but NOT `cherchi-rs`) can pattern-match the M8 boundary inside
// `NativeBooleanError::Arrangement` — specifically
// `ArrangementError::CoplanarPairDeferred`, which kernel-v2 maps to its
// typed `UnsupportedCoplanar` error. Public-surface addition only.
pub use cherchi_rs::ArrangementError;

/// Construct the PRODUCTION boolean backend: the native, in-process
/// cherchi-rs pipeline ([`NativeBoolean`]) — `mesh_arrangement` → labeling →
/// `keep_set(op)`. Reference parity vs the upstream C++ `mesh_booleans`
/// binary is the M6 gate (cherchi-rs `tests/parity_native_vs_sidecar.rs`);
/// the C++ subprocess sidecar (`cherchi-sidecar-rs`) is demoted to a
/// test-only parity oracle (PR-CR-BL3c).
///
/// Always `Some` since PR-CR-M7c: the predicates are clean-room pure Rust
/// (`cherchi-rs::predicates::indirect`) — there is no FFI stub build left to
/// guard against, and the backend is WASM-clean. The `Option` signature is
/// retained for the many existing
/// `let Some(nb) = yang_rs::native_backend() else { /* skip */ }` call
/// sites (their skip arms are now dead but harmless).
pub fn native_backend() -> Option<NativeBoolean> {
    Some(NativeBoolean)
}

// =========================================================================
// Surface / Curve enums
// =========================================================================

/// Analytical surface for a B-Rep face.
///
/// PR-YR2 supports `Plane` end to end. PR-YR6 adds the curved variants
/// `Sphere`, `Cylinder`, and `Cone` as TYPES so a B-Rep can carry curved
/// faces, but the pipeline does **not** yet process curved geometry: every
/// stage that consumes a `Surface` rejects the curved variants LOUDLY with
/// `YangError::CurvedSurfaceNotYetSupported` (governance A15.2, P9/P10 — never
/// a panic, silent skip, or planar approximation). Field shapes mirror
/// `ssi-rs`'s `QuadricSurface` field-for-field so a future Stage-3 yang→ssi
/// mapping is a trivial copy.
///
/// Future PRs add `Torus`, `NurbsSurface`.
///
/// **Cavity-sense (implemented PR-YR13):** the curved cavity-sense for the
/// `box − cylinder` blind pocket is now implemented via the [`BRepFace`]`.reversed`
/// flag (the curved analog of the plane's outward-normal flip at
/// reconstruction). The surface enum still carries **no** `sense` field — sense
/// lives on `BRepFace`, mirroring `ssi-rs` (which has none). PR-YR15 extends the
/// curved-cavity path to a spherical (hemispherical-dimple) cavity; PR-YR17 extends
/// it to a CONICAL POCKET (`box − cone`, apex inside / base above the top,
/// perpendicular top-plane exit → exact `Circle` rim). Still-deferred curved
/// cavities: through-cone / cone-base-subtracted, OBLIQUE cone cuts
/// (ellipse/parabola/hyperbola rims), and fully-internal cone/sphere voids
/// (multi-shell). The `Curve::Parabola`/`Hyperbola` variants are now wired
/// end-to-end (PR-YR22 parabola, PR-YR23 hyperbola).
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Surface {
    /// Plane: `n·x + d = 0`. Normal `n` points OUTWARD from the solid.
    Plane { normal: Vector3, d: f64 },
    /// Sphere `|x − center| = radius`. Outward side = radially **away from
    /// `center`** (a positive-radius solid ball). No `sense` field (mirrors
    /// `ssi-rs`).
    Sphere { center: Point3, radius: f64 },
    /// Infinite right-circular cylinder, axis through `axis_point` along
    /// `axis_dir`, of `radius`. Outward side = radially **away from the axis**
    /// (a solid cylinder). No `sense` field (mirrors `ssi-rs`).
    Cylinder {
        axis_point: Point3,
        axis_dir: Vector3,
        radius: f64,
    },
    /// Infinite right-circular cone with `apex`, axis `axis_dir`, and
    /// `half_angle`. Outward side = radially **away from the axis** (a solid
    /// cone). No `sense` field (mirrors `ssi-rs`).
    Cone {
        apex: Point3,
        axis_dir: Vector3,
        half_angle: f64,
    },
    /// Ring torus (KV6d): revolving a circle of radius `minor_radius` (the
    /// profile, the tube) about the axis through `center` along `axis_dir`, the
    /// profile center tracing a circle of radius `major_radius` (the tube
    /// center circle) in the plane through `center` ⊥ the axis. `major_radius >
    /// minor_radius`. Outward side = radially **away from the tube center
    /// circle** (a solid ring). No `sense` field (mirrors the other surfaces).
    Torus {
        center: Point3,
        axis_dir: Vector3,
        major_radius: f64,
        minor_radius: f64,
    },
}

/// Analytical curve for a B-Rep edge.
///
/// PR-YR2 supports `LineSegment` (endpoints implicit from the edge's
/// start/end vertices). PR-YR6 adds `Circle` and `Ellipse` as TYPES (field
/// shapes mirror `ssi-rs`'s `SsiCurve` field-for-field). No production code
/// consumes the curved variants yet — they exist so a future Stage-3 SSI
/// wiring can store analytical intersection curves on output edges.
///
/// `Parabola` (PR-YR22) and `Hyperbola` (PR-YR23) are now wired end-to-end for
/// the cone∩plane sections. Future PRs also add `NurbsCurve`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Curve {
    /// Straight segment; endpoints implicit from the edge's start/end vertices.
    LineSegment,
    /// Circle of `radius` centered at `center`, in the plane with unit
    /// `normal`.
    Circle {
        center: Point3,
        normal: Vector3,
        radius: f64,
    },
    /// Ellipse centered at `center` in the plane with unit `normal`. The
    /// semi-major axis lies along unit `major_axis` with length `major_radius`;
    /// the semi-minor axis (`normal × major_axis`) has length `minor_radius`.
    Ellipse {
        center: Point3,
        normal: Vector3,
        major_axis: Vector3,
        major_radius: f64,
        minor_radius: f64,
    },
    /// Parabola with `vertex` on the curve, in the plane with unit `normal`.
    /// The axis of symmetry lies along unit `axis_dir`; the conjugate in-plane
    /// direction is `normal × axis_dir` (unit, since both are unit and
    /// orthogonal). `focal_length` is the focal distance `f > 0`. In the
    /// in-plane frame `(x along axis_dir, y along normal × axis_dir)` the curve
    /// satisfies `y² = 4f·x`, parameterized (matching `ssi_rs::SsiCurve`) as
    /// `vertex + (t²/(4f))·axis_dir + t·(normal × axis_dir)`.
    Parabola {
        vertex: Point3,
        normal: Vector3,
        axis_dir: Vector3,
        focal_length: f64,
    },
    /// Hyperbola centered at `center` in the plane with unit `normal`. The
    /// transverse axis lies along unit `major_axis`; the conjugate in-plane
    /// direction is `normal × major_axis` (unit, since both are unit and
    /// orthogonal). `semi_transverse` is the transverse semi-axis `a > 0`;
    /// `semi_conjugate` is the conjugate semi-axis `b > 0`. In the in-plane
    /// frame `(u along major_axis, v along normal × major_axis)` the
    /// `+major_axis` branch satisfies `(u/a)² − (v/b)² = 1` with `u > 0`,
    /// parameterized (matching `ssi_rs::SsiCurve`) as
    /// `center + (a·cosh t)·major_axis + (b·sinh t)·(normal × major_axis)`.
    Hyperbola {
        center: Point3,
        normal: Vector3,
        major_axis: Vector3,
        semi_transverse: f64,
        semi_conjugate: f64,
    },
}

// =========================================================================
// B-Rep topology
// =========================================================================

#[derive(Clone, Debug, PartialEq)]
pub struct BRepVertex {
    pub point: Point3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BRepEdge {
    pub start: u32,
    pub end: u32,
    pub curve: Curve,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BRepFace {
    pub surface: Surface,
    /// Edge indices in CCW order viewed from outside the solid along
    /// the face normal. Successive edges connect:
    /// `edges[outer_loop[i]].end == edges[outer_loop[i+1]].start`
    /// (modulo wrap). PR-YR2 does NOT validate this cycle continuity.
    pub outer_loop: Vec<u32>,
    /// Inner loops (holes), each an edge-index list; CW viewed from
    /// outside (opposite the outer loop). Empty for simple faces.
    pub inner_loops: Vec<Vec<u32>>,
    /// When `true`, the face's effective outward normal (outward from the
    /// result solid) is the **negation** of the surface's canonical analytic
    /// outward normal. Planar faces encode sense in `Plane.normal` and keep
    /// `reversed == false`; only curved cavity walls from a `Subtract`
    /// subtrahend (input B) set `true`. Any future consumer that computes a
    /// curved outward normal (Stage-1 winding, face resolution) MUST negate
    /// when `reversed`; Stage-1 runs on canonical inputs (`reversed == false`)
    /// so no current path needs the negation.
    pub reversed: bool,
}

// =========================================================================
// TessellationMap — the bijection
// =========================================================================

/// Where a mesh vertex came from in the B-Rep input.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TessellationSource {
    /// Mesh vertex coincides with B-Rep vertex (index into `BRep::vertices`).
    BRepVertex(u32),
    /// Mesh vertex is on edge `edge` at parameter `t`. The meaning of `t`
    /// depends on the edge's curve: for `Curve::LineSegment`, `t ∈ [0, 1]`
    /// lerps start→end; for `Curve::Circle`, `t` is an **angle in radians** in
    /// the circle's own `ortho_basis(normal)` frame (PR-YR7).
    BRepEdge { edge: u32, t: f64 },
    /// Mesh vertex is interior to face `face` at surface params `(u, v)`.
    BRepFace { face: u32, u: f64, v: f64 },
    /// Output vertex created by the boolean operation; no spatial
    /// match against either input. New in PR-YR3.
    Intersection,
    /// Source genuinely unknown — `BRep::from_mesh` degenerate path.
    Unknown,
}

/// Spatial tolerance for matching output mesh vertices to input
/// mesh vertices in `boolean()`. Tight enough to avoid false
/// positives on genuine intersection points; loose enough to absorb
/// the sidecar's internal coordinate-normalization rounding.
pub const MATCH_TOLERANCE: f64 = 1e-9;

/// Per-mesh-vertex bijection to B-Rep features. Established by Stage 1.
#[derive(Clone, Debug, PartialEq)]
pub struct TessellationMap {
    sources: Vec<TessellationSource>,
}

impl TessellationMap {
    pub fn empty() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.sources.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// Look up the source feature for a mesh vertex.
    ///
    /// Panics in debug if `mesh_vertex` is out of range.
    pub fn lookup(&self, mesh_vertex: u32) -> TessellationSource {
        debug_assert!(
            (mesh_vertex as usize) < self.sources.len(),
            "TessellationMap::lookup: vertex {mesh_vertex} out of range (len {})",
            self.sources.len()
        );
        self.sources[mesh_vertex as usize]
    }
}

// =========================================================================
// PR-YR4 — per-triangle face attribution
// =========================================================================

/// Identifies which input of `boolean(a, b, ...)` a vertex / triangle
/// descends from. `A < B` by enum discriminant (drives tie-break in
/// majority-vote attribution).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InputId {
    A,
    B,
}

/// "This output triangle descends from face `face` of input `input`."
/// Produced by `boolean()` via majority-vote of the triangle's 3
/// vertices' provenance (PR-YR3).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TriangleAttribution {
    pub input: InputId,
    pub face: u32,
}

/// Per-output-triangle attribution to an input B-Rep face.
///
/// `None` means no `(InputId, face)` pair won a 2-of-3 majority — the
/// triangle is either entirely from new intersection vertices or
/// straddles both inputs.
///
/// Established by `boolean()` only. `BRep::new` and `BRep::from_mesh`
/// produce `TriangleAttributionMap::empty()`.
#[derive(Clone, Debug, PartialEq)]
pub struct TriangleAttributionMap {
    attributions: Vec<Option<TriangleAttribution>>,
}

impl TriangleAttributionMap {
    pub fn empty() -> Self {
        Self {
            attributions: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.attributions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.attributions.is_empty()
    }

    /// Look up the attribution for a mesh triangle.
    ///
    /// Panics in debug if `mesh_tri` is out of range.
    pub fn lookup(&self, mesh_tri: u32) -> Option<TriangleAttribution> {
        debug_assert!(
            (mesh_tri as usize) < self.attributions.len(),
            "TriangleAttributionMap::lookup: tri {mesh_tri} out of range (len {})",
            self.attributions.len()
        );
        self.attributions[mesh_tri as usize]
    }
}

// =========================================================================
// BRep
// =========================================================================

/// Boundary-Representation solid for yang-rs's boolean pipeline.
///
/// Two construction paths:
/// - [`BRep::new`]: pass real topology (`Vec<BRepVertex>`, etc.); eager
///   Stage 1 tessellation produces the internal `mesh` + `TessellationMap`.
/// - [`BRep::from_mesh`]: PR-YR1 backward-compat. Empty topology;
///   `TessellationMap` entries are all `Unknown`.
///
/// Always populated: `mesh` and `tessellation_map`. PR-YR3 will consume
/// the `TessellationMap` for Stage 5/6 reassembly.
///
/// PR-YR4 adds `triangle_attribution`: a per-output-triangle label
/// `(InputId, face)` populated by `boolean()` via majority-vote of the
/// triangle's 3 vertices' provenance. `BRep::new` and `BRep::from_mesh`
/// produce an empty `TriangleAttributionMap`.
#[derive(Clone, Debug, PartialEq)]
pub struct BRep {
    vertices: Vec<BRepVertex>,
    edges: Vec<BRepEdge>,
    faces: Vec<BRepFace>,
    mesh: Mesh,
    tessellation: TessellationMap,
    triangle_attribution: TriangleAttributionMap,
    /// PR-KV13 F2: per-output-FACE attribution, parallel to `faces` — the
    /// `(input, face)` each output face descends from. Populated by
    /// `boolean()`; empty for `new`/`from_mesh` (no boolean lineage).
    face_attribution: Vec<TriangleAttribution>,
    /// N4 (1b): per Stage-1 mesh triangle (parallel to `mesh.tris`), the index
    /// of the B-Rep `face` that produced it. This is the inverse of the Stage-1
    /// `face_tri_ranges`; it lets `boolean()` attribute a kept arrangement
    /// triangle to its B-Rep face DIRECTLY from cherchi's per-triangle
    /// provenance (`LabeledArrangement.source`), replacing geometric
    /// centroid-proximity (deviation N4). Populated by `BRep::new`; EMPTY for
    /// `from_mesh` and boolean-output BReps (no Stage-1 face lineage) — those
    /// fall back to the geometric attribution path.
    tri_face: Vec<u32>,
    /// Case-IV phantom guard (spec `yang_case_iv_phantom_guard`): the forced
    /// minimum rim segment count this B-Rep was (re)tessellated at. Stage-0's
    /// internal re-tessellations (`disc_rim_ring`, `build_stage0_mesh`, split
    /// re-triangulation) MUST honor it so their rims stay conformal with
    /// `as_mesh()`. `None` = the solid's own Stage-1 chord bound (the default).
    forced_rim_n: Option<usize>,
}

impl BRep {
    /// Construct from B-Rep topology. **Eagerly tessellates** via Stage 1.
    ///
    /// Planar-face tessellation (PR-YR2 / PR-NC1):
    /// - Convex, hole-free planar faces use the original fan triangulation
    ///   (unchanged, byte-for-byte).
    /// - Non-convex planar faces (a reflex vertex on the outer loop) **and**
    ///   planar faces with inner loops (holes) tessellate via a constrained
    ///   Delaunay triangulation (`cherchi_rs::cdt_polygon_with_holes`, PR-NC1).
    ///   The CDT path adds **no** interior Steiner points and never subdivides
    ///   a boundary edge — the output vertex set equals the input boundary
    ///   vertex set, so the planar `TessellationMap` stays 1:1 on boundary.
    /// - Curved surfaces (cylinder / sphere / cone) follow their own Stage-1
    ///   paths and DO introduce Steiner rim / center vertices.
    ///
    /// Returns `Err(YangError::MalformedTopology)` for:
    /// - Any face with `outer_loop.len() < 3`
    /// - Out-of-range edge index in any face's `outer_loop`
    /// - Out-of-range vertex index in any edge
    pub fn new(
        verts: Vec<BRepVertex>,
        edges: Vec<BRepEdge>,
        faces: Vec<BRepFace>,
    ) -> Result<Self, YangError> {
        let n_verts = verts.len();
        let n_edges = edges.len();

        // Validate: every edge's endpoints are in range.
        for (e_idx, e) in edges.iter().enumerate() {
            if (e.start as usize) >= n_verts {
                return Err(YangError::MalformedTopology(format!(
                    "edge {e_idx}.start = {} out of range (verts.len() = {n_verts})",
                    e.start
                )));
            }
            if (e.end as usize) >= n_verts {
                return Err(YangError::MalformedTopology(format!(
                    "edge {e_idx}.end = {} out of range (verts.len() = {n_verts})",
                    e.end
                )));
            }
        }

        // Validate: every face's outer_loop is well-formed. Out-of-range edge
        // indices are always rejected. The `len >= 3` rule applies ONLY when
        // EVERY loop edge is a `Curve::LineSegment` (PR-YR7 loop-length
        // relaxation): a face bounded by a closed curve (a disk cap bounded by
        // one `Curve::Circle`) has a 1-edge loop and is legal.
        for (f_idx, f) in faces.iter().enumerate() {
            for &e_idx in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
                if (e_idx as usize) >= n_edges {
                    return Err(YangError::MalformedTopology(format!(
                        "face {f_idx}: edge index {e_idx} out of range (edges.len() = {n_edges})"
                    )));
                }
            }
            if f.reversed && matches!(f.surface, Surface::Plane { .. }) {
                return Err(YangError::MalformedTopology(format!(
                    "face {f_idx}: a planar face must carry its sense in the plane \
                     normal, not `reversed` (PR-KV6b-1)"
                )));
            }
            let all_line = f
                .outer_loop
                .iter()
                .all(|&e_idx| matches!(edges[e_idx as usize].curve, Curve::LineSegment));
            if all_line && f.outer_loop.len() < 3 {
                return Err(YangError::MalformedTopology(format!(
                    "face {f_idx}.outer_loop.len() = {} < 3 (all-LineSegment loop)",
                    f.outer_loop.len()
                )));
            }
        }

        // Stage 1 tessellation — extracted to `stage1_tessellate` (PR-YR26)
        // so the Stage-0 coplanar overlay can re-tessellate with snapped
        // vertices + per-face overrides. Byte-for-byte the pre-YR26 output.
        Self::from_topology(verts, edges, faces, None)
    }

    /// Shared constructor body: Stage-1 tessellation with an optional forced
    /// minimum rim segment count (`None` = byte-identical to [`BRep::new`]).
    /// The Case-IV phantom guard (spec `yang_case_iv_phantom_guard`) rebuilds
    /// both boolean operands through this path when their analytic gap
    /// demands a finer sampling than each solid's own chord bound chose.
    fn from_topology(
        verts: Vec<BRepVertex>,
        edges: Vec<BRepEdge>,
        faces: Vec<BRepFace>,
        min_n_seg: Option<usize>,
    ) -> Result<Self, YangError> {
        let tess = stage1_tessellate_min_segments(&verts, &edges, &faces, min_n_seg)?;
        // N4: invert face_tri_ranges into a per-triangle owning-face map (1:1
        // with the mesh triangles), so kept arrangement triangles can be
        // attributed via cherchi provenance instead of geometric proximity.
        let mut tri_face = vec![0u32; tess.tris.len()];
        for (fi, range) in tess.face_tri_ranges.iter().enumerate() {
            for ti in range.clone() {
                tri_face[ti] = fi as u32;
            }
        }
        let mesh = Mesh::new(tess.verts, tess.tris);
        let tessellation = TessellationMap {
            sources: tess.sources,
        };

        Ok(Self {
            vertices: verts,
            edges,
            faces,
            mesh,
            tessellation,
            triangle_attribution: TriangleAttributionMap::empty(),
            face_attribution: Vec::new(),
            tri_face,
            forced_rim_n: min_n_seg,
        })
    }

    /// Case-IV phantom guard (spec `yang_case_iv_phantom_guard`): rebuild
    /// this B-Rep's Stage-1 mesh forcing the rim segment count to at least
    /// `n`. Topology (vertices/edges/faces) is unchanged — only the
    /// tessellation density rises, which is always chord-valid (a finer N
    /// only shrinks the sagitta; governance A14.3).
    fn rebuilt_with_min_rim_segments(&self, n: usize) -> Result<Self, YangError> {
        Self::from_topology(
            self.vertices.clone(),
            self.edges.clone(),
            self.faces.clone(),
            Some(n),
        )
    }

    /// Construct from a pre-tessellated mesh (no topology).
    /// Degenerate B-Rep: `TessellationMap` entries are all `Unknown`.
    pub fn from_mesh(mesh: Mesh) -> Self {
        let n = mesh.num_verts();
        Self {
            vertices: Vec::new(),
            edges: Vec::new(),
            faces: Vec::new(),
            tessellation: TessellationMap {
                sources: vec![TessellationSource::Unknown; n],
            },
            mesh,
            triangle_attribution: TriangleAttributionMap::empty(),
            face_attribution: Vec::new(),
            tri_face: Vec::new(),
            forced_rim_n: None,
        }
    }

    /// N4: per Stage-1 mesh triangle, its owning B-Rep face index (parallel to
    /// `as_mesh().tris`). Empty when there is no Stage-1 face lineage
    /// (`from_mesh`, boolean output) — callers then fall back to geometric
    /// attribution.
    pub(crate) fn tri_face(&self) -> &[u32] {
        &self.tri_face
    }

    /// Case-IV phantom guard: the forced minimum rim segment count this
    /// B-Rep was (re)tessellated at (`None` = the solid's own chord bound).
    /// Stage-0's internal re-tessellations must pass this through so their
    /// rims stay conformal with `as_mesh()`.
    pub(crate) fn forced_rim_n(&self) -> Option<usize> {
        self.forced_rim_n
    }
}

/// Stage-1 tessellation output (PR-YR26 extraction of the `BRep::new` body):
/// the mesh vertex pool (B-Rep vertices first, Steiner appended), the 1:1
/// `TessellationSource` per vertex, the triangles, and per input face the
/// range of `tris` it produced (consumed by the Stage-0 overlay
/// re-tessellation to splice per-face replacements).
pub(crate) struct Stage1Tess {
    pub(crate) verts: Vec<Point3>,
    pub(crate) sources: Vec<TessellationSource>,
    pub(crate) tris: Vec<[u32; 3]>,
    pub(crate) face_tri_ranges: Vec<std::ops::Range<usize>>,
}

/// Stage 1 tessellation (PR-YR7: planar Newell-fan + curved cylinder;
/// PR-YR12: sphere lat/long grid; PR-NC1: CDT for non-convex/holed planar
/// faces). Extracted verbatim from `BRep::new` in PR-YR26 (plus per-face
/// triangle-range recording) so Stage-0 coplanar preprocessing can
/// re-tessellate with snapped vertex coordinates.
///
/// Mesh vertices start 1:1 with the B-Rep vertices (the planar box path
/// emits no Steiner points). The curved path appends rim-ring + cap-
/// center Steiner vertices and indexes the SHARED cached rings so the
/// cylinder mesh is watertight.
pub(crate) fn stage1_tessellate(
    verts: &[BRepVertex],
    edges: &[BRepEdge],
    faces: &[BRepFace],
) -> Result<Stage1Tess, YangError> {
    stage1_tessellate_with_rim_overrides(
        verts,
        edges,
        faces,
        &std::collections::BTreeMap::new(),
        None,
    )
}

/// Stage 1 tessellation with optional per-circle-edge RIM CROSSING points
/// (PR-M8 disc-rim crossing). `rim_overrides[e]` lists extra 3D points to
/// insert into edge `e`'s full-circle rim ring as additional Steiner
/// vertices — the §4.5.5 shared-boundary sampling for a coplanar disc whose
/// rim the overlap boundary CROSSES. Each inserted point is placed at its
/// angle-from-seam sorted position; the edge is recorded in the returned
/// `inserted_rims` set so [`tessellate_lateral_face`] routes that rim's
/// lateral through the AZIMUTH-MERGE strip (uniform index-pairing no longer
/// holds once a rim carries non-uniform samples).
///
/// An EMPTY `rim_overrides` map yields byte-identical `verts` and `tris` to
/// [`stage1_tessellate`] — the uniform-rim path is left 100% untouched (see
/// the `rim_override_empty_is_byte_identical` unit test).
///
/// Loud errors (`MalformedTopology`):
/// - an override point that coincides ANGULARLY with an existing UNIFORM
///   sample (we never silently merge a crossing into a uniform vertex), or
/// - an override point not on the rim circle (off-radius / off-plane).
///
/// An override point coinciding with an already-inserted override is
/// deduplicated (skipped).
pub(crate) fn stage1_tessellate_with_rim_overrides(
    verts: &[BRepVertex],
    edges: &[BRepEdge],
    faces: &[BRepFace],
    rim_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
    min_n_seg: Option<usize>,
) -> Result<Stage1Tess, YangError> {
    stage1_tessellate_inner(verts, edges, faces, rim_overrides, min_n_seg).map(|(t, _)| t)
}

/// Stage 1 tessellation forcing the circle-rim segment count to AT LEAST
/// `min_n_seg` (M8-cyl Increment 1). The cylinder rim N is normally derived
/// from this solid's own chord-error AABB; for two COINCIDENT cylinders to get
/// identical overlap meshes (§4.5.5) BOTH must sample at the SAME N. The caller
/// passes the max of both solids' N so each gets a rim sampling that satisfies
/// BOTH chord bounds (a FINER tessellation is always chord-valid — it can only
/// REDUCE the sagitta, never widen it; this is not a tolerance relaxation).
/// `min_n_seg = None` is byte-identical to [`stage1_tessellate`].
pub(crate) fn stage1_tessellate_min_segments(
    verts: &[BRepVertex],
    edges: &[BRepEdge],
    faces: &[BRepFace],
    min_n_seg: Option<usize>,
) -> Result<Stage1Tess, YangError> {
    stage1_tessellate_inner(
        verts,
        edges,
        faces,
        &std::collections::BTreeMap::new(),
        min_n_seg,
    )
    .map(|(t, _)| t)
}

/// Inner implementation returning the tessellation AND the set of rim edges
/// that received inserted crossing points (consumed by the lateral dispatch).
#[allow(clippy::type_complexity)]
fn stage1_tessellate_inner(
    verts: &[BRepVertex],
    edges: &[BRepEdge],
    faces: &[BRepFace],
    rim_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
    min_n_seg: Option<usize>,
) -> Result<(Stage1Tess, std::collections::BTreeSet<u32>), YangError> {
    {
        let mut inserted_rims: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        let mut out_verts: Vec<Point3> = verts.iter().map(|v| v.point).collect();
        let mut sources: Vec<TessellationSource> = (0..verts.len() as u32)
            .map(TessellationSource::BRepVertex)
            .collect();
        let mut out_tris: Vec<[u32; 3]> = Vec::new();

        // ---- Curved pre-pass: choose N (chord error) + build shared rim rings.
        //
        // N is chosen once from the analytic AABB of ALL `Curve::Circle` rim
        // edges combined (spec §3), and shared by every circle. The minimal
        // cylinder has exactly two rims of equal radius, so a single N applies.
        //
        // PR-YR12: a `Surface::Sphere` face is self-contained — it builds its
        // own latitude/longitude grid in `tessellate_sphere_face` and does NOT
        // participate in the cylinder rim-ring pre-pass. Exclude any Circle edge
        // that belongs to a sphere face's loops so the cylinder path stays
        // byte-for-byte unchanged (with a pure-sphere B-Rep `circle_edges` ends
        // up empty and the whole rim pre-pass is skipped).
        let sphere_seam_edges: std::collections::BTreeSet<u32> = faces
            .iter()
            .filter(|f| matches!(f.surface, Surface::Sphere { .. }))
            .flat_map(|f| {
                f.outer_loop
                    .iter()
                    .chain(f.inner_loops.iter().flatten())
                    .copied()
            })
            .collect();
        let circle_edges: Vec<(usize, Point3, Vector3, f64, u32, u32)> = edges
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match e.curve {
                Curve::Circle {
                    center,
                    normal,
                    radius,
                } if !sphere_seam_edges.contains(&(i as u32)) => {
                    Some((i, center, normal, radius, e.start, e.end))
                }
                _ => None,
            })
            .collect();

        // edge_idx -> the cached ring of mesh-vertex indices (ring[0] reuses the
        // circle's seam B-Rep vertex; ring[1..N] are new Steiner verts).
        let mut rim_rings: std::collections::BTreeMap<u32, Vec<u32>> =
            std::collections::BTreeMap::new();

        if !circle_edges.is_empty() {
            // Stage-1 chord bound `d_ε = 1e-2 × analytic-AABB-diag` over all rim
            // circles, from the SINGLE shared source (governance A14.3). Since
            // `circle_edges` is non-empty, `curved_chord_bound` returns `Some`;
            // the `unwrap_or(0.0)` is an unreachable no-panic guard (P9 — a 0.0
            // band is already handled by the `d_eps > 0.0` floor below, keeping
            // the N=3 floor rather than panicking).
            let mut d_eps = curved_chord_bound(edges).unwrap_or(0.0);
            // PR-YR16 (spec §3): the rim-AABB `curved_chord_bound` ignores the
            // cone height and can EXCEED the cone's honest bound for wide-short
            // cones (`h < 2R`), which would permit a residual larger than
            // `cone_chord_bound`. When ANY `Surface::Cone` face is present,
            // tighten `d_eps` by folding in each cone's own bound via min().
            // Cylinder / sphere / all-planar inputs have no cone face, so this
            // branch is never entered and those paths stay byte-for-byte.
            if faces
                .iter()
                .any(|f| matches!(f.surface, Surface::Cone { .. }))
            {
                for f in faces.iter() {
                    if let Surface::Cone {
                        apex,
                        axis_dir,
                        half_angle,
                    } = f.surface
                    {
                        let au = normalize3(axis_dir.as_array());
                        let ap = apex.as_array();
                        // Derive height_f from this cone's rim Circle (the
                        // single Circle edge in its outer loop).
                        for &e_idx in &f.outer_loop {
                            if let Curve::Circle { center, .. } = edges[e_idx as usize].curve {
                                let c = center.as_array();
                                let height_f = ((c[0] - ap[0]) * au[0]
                                    + (c[1] - ap[1]) * au[1]
                                    + (c[2] - ap[2]) * au[2])
                                    .abs();
                                d_eps = d_eps.min(cone_chord_bound(height_f, half_angle));
                            }
                        }
                    }
                }
            }
            // Smallest N >= 3 with max_radius·(1 − cos(π/N)) ≤ d_eps.
            let max_r = circle_edges
                .iter()
                .map(|&(_, _, _, r, _, _)| r)
                .fold(0.0f64, f64::max);
            let mut n_seg = 3usize;
            // d_eps > 0 for any non-degenerate cylinder; if it is somehow zero
            // (a degenerate AABB), keep the floor N=3 rather than loop forever.
            if d_eps > 0.0 {
                while max_r * (1.0 - (std::f64::consts::PI / n_seg as f64).cos()) > d_eps {
                    n_seg += 1;
                }
            }
            // M8-cyl Increment 1: a coincident-cylinder pair forces BOTH solids
            // to the same (max) N so their overlap rings are identically sampled
            // (a finer N only shrinks the sagitta — chord-valid for this solid).
            if let Some(force) = min_n_seg {
                n_seg = n_seg.max(force);
            }
            // Case-IV INTRA-solid phantom fold (spec
            // `yang_case_iv_phantom_guard`, M8 increment 16): two of THIS
            // solid's own analytically-disjoint cylinders closer than the
            // chord bands (a hole lateral 0.0115 from the plate wall — the
            // chained F0088 output) would make the cap's outer-rim chords
            // dip across the hole rim, so the planar CDT gets CROSSING
            // constraints and fails loud at CONSTRUCTION time. Fold each
            // disjoint pair's derived N in here, where every tessellation
            // (conversion, Stage-0 rebuilds, the guard's cross-pair rebuild)
            // picks it up natively. Far pairs derive a tiny N the natural
            // bound absorbs — self-limiting, no mode branch.
            let cyls: Vec<(Point3, Vector3, f64)> = faces
                .iter()
                .filter_map(|f| match f.surface {
                    Surface::Cylinder {
                        axis_point,
                        axis_dir,
                        radius,
                    } => Some((axis_point, axis_dir, radius)),
                    _ => None,
                })
                .collect();
            for i in 0..cyls.len() {
                for j in (i + 1)..cyls.len() {
                    if let Some(n) = cyl_pair_phantom_n(cyls[i], cyls[j]) {
                        n_seg = n_seg.max(n);
                    }
                }
            }
            // Diagnostic experiment knob (M8 increment-8 spec phase, task
            // #62): force a global rim-N floor to measure whether/where the
            // mint-displacement fold class is a pure sampling artifact and
            // what finer N costs. Dev-only, like TIEBREAK_NEUTER /
            // YANG_SHIFT_NEUTER — never set in production or tests.
            if let Some(floor) = std::env::var("YANG_NSEG_FLOOR")
                .ok()
                .and_then(|s| s.parse::<usize>().ok())
            {
                n_seg = n_seg.max(floor);
            }
            if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
                eprintln!(
                    "[stage1-nseg] n_seg={n_seg} d_eps={d_eps:e} max_r={max_r} \
                     min_n_seg={min_n_seg:?} circles={}",
                    circle_edges.len()
                );
            }

            // Build the shared sample CHAIN for each circle edge (PR-KV6b-1):
            // a full circle (`start == end`) gets the closed seam-anchored
            // ring exactly as before; an ARC (`start != end` — the new input
            // convention: the CCW sweep around `curve.normal` from `start`
            // to `end`, unique in (0, 2π)) gets an OPEN chain
            // `[start, Steiner…, end]` sampled at the same per-chord angle
            // bound, so the global `d_eps` holds for arcs too. Chains are
            // shared between the two faces incident to the edge — the
            // watertightness mechanism, unchanged.
            for &(e_idx, center, normal, radius, e_start, e_end) in &circle_edges {
                let (e1, e2) = ortho_basis(normal);
                let c = center.as_array();
                let e1a = e1.as_array();
                let e2a = e2.as_array();
                let seam_vertex = e_start;

                // Angle of a vertex in this circle's frame + on-circle
                // validation (the arc convention makes endpoints
                // load-bearing, so off-circle endpoints are loud).
                let angle_of = |v: u32| -> Result<f64, YangError> {
                    let sp = match verts.get(v as usize) {
                        Some(vv) => vv.point.as_array(),
                        None => {
                            return Err(YangError::MalformedTopology(format!(
                                "circle edge {e_idx}: vertex {v} out of range"
                            )))
                        }
                    };
                    let w = [sp[0] - c[0], sp[1] - c[1], sp[2] - c[2]];
                    let nu = normalize3(normal.as_array());
                    let along = w[0] * nu[0] + w[1] * nu[1] + w[2] * nu[2];
                    let wx = w[0] * e1a[0] + w[1] * e1a[1] + w[2] * e1a[2];
                    let wy = w[0] * e2a[0] + w[1] * e2a[1] + w[2] * e2a[2];
                    let r = (wx * wx + wy * wy).sqrt();
                    let band = 1e-9 * (1.0 + radius);
                    if (r - radius).abs() > band || along.abs() > band {
                        return Err(YangError::MalformedTopology(format!(
                            "circle edge {e_idx}: endpoint vertex {v} is not on the circle                              (radial {r} vs radius {radius}, axial offset {along})"
                        )));
                    }
                    Ok(wy.atan2(wx))
                };

                if e_start != e_end {
                    // ---- ARC chain (PR-KV6b-1) ----
                    let phi0 = angle_of(e_start)?;
                    let phi1 = angle_of(e_end)?;
                    let sweep = (phi1 - phi0).rem_euclid(2.0 * std::f64::consts::PI);
                    if sweep <= 0.0 || !sweep.is_finite() {
                        return Err(YangError::MalformedTopology(format!(
                            "circle edge {e_idx}: degenerate arc sweep {sweep}"
                        )));
                    }
                    // Same per-chord angle as the full-circle ring; floor 2
                    // segments so a π arc never degenerates to one diameter
                    // chord.
                    let m = ((sweep * n_seg as f64) / (2.0 * std::f64::consts::PI)).ceil() as usize;
                    let m = m.max(2);
                    let mut chain: Vec<u32> = Vec::with_capacity(m + 1);
                    chain.push(e_start);
                    for k in 1..m {
                        let theta = phi0 + sweep * (k as f64) / (m as f64);
                        let (st, ct) = theta.sin_cos();
                        let pt = [
                            c[0] + radius * (ct * e1a[0] + st * e2a[0]),
                            c[1] + radius * (ct * e1a[1] + st * e2a[1]),
                            c[2] + radius * (ct * e1a[2] + st * e2a[2]),
                        ];
                        let vi = out_verts.len() as u32;
                        out_verts.push(Point3::new(pt[0], pt[1], pt[2]));
                        sources.push(TessellationSource::BRepEdge {
                            edge: e_idx as u32,
                            t: theta,
                        });
                        chain.push(vi);
                    }
                    chain.push(e_end);
                    rim_rings.insert(e_idx as u32, chain);
                    continue;
                }
                // The seam B-Rep vertex is NOT required to lie at angle 0 of
                // this circle's `ortho_basis` frame — the fixture chooses its
                // own angle-0 convention. Recover the seam's ACTUAL angle `phi0`
                // in this frame so the Steiner verts are placed at evenly-spaced
                // angles STARTING FROM the seam (`phi0 + 2πk/N`). Then `ring[0]`
                // (the seam) is consistent with `ring[1..N]` (chord spacing is
                // uniform) and — crucially for the lateral — the two rims, whose
                // seams sit at the same geometric azimuth, stay azimuth-aligned
                // under the `(N−k)` opposite-rim mapping (spec §6).
                let phi0 = angle_of(seam_vertex)?;
                // Uniform sample angles RELATIVE to the seam, in [0, 2π): the
                // seam at offset 0, the k-th Steiner at 2πk/N. A rim-crossing
                // override is inserted by its OWN seam-relative angle, sorted in.
                let two_pi = 2.0 * std::f64::consts::PI;
                // Build the (angle_offset, kind) list. kind: Uniform(k) places a
                // uniform Steiner (or reuses the seam for k==0); Override(point)
                // inserts a crossing point.
                enum RimSlot {
                    Uniform(usize),
                    Override(Point3),
                }
                let mut slots: Vec<(f64, RimSlot)> = Vec::with_capacity(n_seg);
                for k in 0..n_seg {
                    slots.push((two_pi * (k as f64) / (n_seg as f64), RimSlot::Uniform(k)));
                }
                if let Some(extra) = rim_overrides.get(&(e_idx as u32)) {
                    // Angular margin around a uniform sample: a crossing landing
                    // within this of an existing uniform vertex is a malformed
                    // overlap (we never silently merge into a uniform sample).
                    let uni_step = two_pi / (n_seg as f64);
                    let merge_tol = uni_step * 1.0e-6;
                    let mut inserted_keys: Vec<[u64; 3]> = Vec::new();
                    for &pt in extra {
                        // Resolve the point's seam-relative angle + validate it
                        // lies on the rim circle.
                        let sp = pt.as_array();
                        let w = [sp[0] - c[0], sp[1] - c[1], sp[2] - c[2]];
                        let nu = normalize3(normal.as_array());
                        let along = w[0] * nu[0] + w[1] * nu[1] + w[2] * nu[2];
                        let wx = w[0] * e1a[0] + w[1] * e1a[1] + w[2] * e1a[2];
                        let wy = w[0] * e2a[0] + w[1] * e2a[1] + w[2] * e2a[2];
                        let r = (wx * wx + wy * wy).sqrt();
                        let band = 1e-9 * (1.0 + radius);
                        // A rim-crossing override lies on the tessellated rim
                        // POLYGON — a CHORD between two on-circle samples — so it
                        // sits up to the Stage-1 chord sagitta
                        // `radius·(1 − cos(π/N))` INSIDE the analytic circle.
                        // That chord is the rim's own Stage-1 representation
                        // (A14.3 chord bound), the SAME points the cap overlay
                        // triangles use, so the radial deficit is expected, not a
                        // fault — validating against it keeps the override
                        // bit-identical with the overlap mesh (no T-junction) and
                        // is NOT a tolerance widening. A point OUTSIDE the circle,
                        // or inside by MORE than the sagitta (a bad projection),
                        // or off the cap plane (axial) is still a loud fault.
                        let sagitta = radius * (1.0 - (std::f64::consts::PI / n_seg as f64).cos());
                        if r - radius > band || radius - r > sagitta + band || along.abs() > band {
                            return Err(YangError::MalformedTopology(format!(
                                "circle edge {e_idx}: rim-crossing override ({},{},{}) is off the \
                                 rim (radial {r} vs radius {radius} sagitta {sagitta}, axial {along})",
                                sp[0], sp[1], sp[2]
                            )));
                        }
                        let off = (wy.atan2(wx) - phi0).rem_euclid(two_pi);
                        // Coincides angularly with a uniform sample? loud.
                        let k_near = (off / uni_step).round();
                        if (off - k_near * uni_step).abs() <= merge_tol {
                            return Err(YangError::MalformedTopology(format!(
                                "circle edge {e_idx}: rim-crossing override at angle-offset {off} \
                                 coincides with uniform sample k={k_near} (silent merge refused)"
                            )));
                        }
                        // Bit-identical to an already-inserted override? dedup
                        // (the SAME point re-arriving from adjacent sub-chords).
                        // M-C fix (spec `m8_stage0_band_scale_crossing_verts`
                        // E-C1/E-C1b): identity is EXACT coordinate bits — never
                        // an angular tolerance. Genuinely distinct band-close
                        // crossings (the R0088/R0070 twin population) must BOTH
                        // enter the ring, or it desynchronizes from the cap
                        // override that carries both points (T-junction holes).
                        let key = [sp[0].to_bits(), sp[1].to_bits(), sp[2].to_bits()];
                        if inserted_keys.contains(&key) {
                            continue;
                        }
                        inserted_keys.push(key);
                        slots.push((off, RimSlot::Override(pt)));
                    }
                    if !inserted_keys.is_empty() {
                        inserted_rims.insert(e_idx as u32);
                    }
                }
                // Sort by seam-relative angle (the seam, offset 0, leads).
                // ULP-twin overrides collide on the f64 angle key (spec
                // `m8_holed_disc_coplanar_overlay` §8 F2): break the tie by
                // the EXACT angular order — never by insertion order, which
                // is frame-direction-dependent and desynchronises the ring
                // from the cap overlay boundary / the opposite rim.
                // Override-vs-uniform ties are impossible (merge_tol guard).
                slots.sort_by(|a, b| match a.0.total_cmp(&b.0) {
                    std::cmp::Ordering::Equal => match (&a.1, &b.1) {
                        (RimSlot::Override(pa), RimSlot::Override(pb)) => {
                            exact_rim_ccw_tiebreak(c, e1a, e2a, *pa, *pb)
                        }
                        _ => std::cmp::Ordering::Equal,
                    },
                    ord => ord,
                });
                let mut ring: Vec<u32> = Vec::with_capacity(slots.len());
                for (off, slot) in slots {
                    match slot {
                        RimSlot::Uniform(0) => {
                            // ring[0] = the seam B-Rep vertex (keep its source).
                            ring.push(seam_vertex);
                        }
                        RimSlot::Uniform(_) => {
                            let theta = phi0 + off;
                            let (ct, st) = (theta.cos(), theta.sin());
                            let pt = [
                                c[0] + radius * (ct * e1a[0] + st * e2a[0]),
                                c[1] + radius * (ct * e1a[1] + st * e2a[1]),
                                c[2] + radius * (ct * e1a[2] + st * e2a[2]),
                            ];
                            let vi = out_verts.len() as u32;
                            out_verts.push(Point3::new(pt[0], pt[1], pt[2]));
                            sources.push(TessellationSource::BRepEdge {
                                edge: e_idx as u32,
                                t: theta,
                            });
                            ring.push(vi);
                        }
                        RimSlot::Override(pt) => {
                            let theta = phi0 + off;
                            let vi = out_verts.len() as u32;
                            out_verts.push(pt);
                            sources.push(TessellationSource::BRepEdge {
                                edge: e_idx as u32,
                                t: theta,
                            });
                            ring.push(vi);
                        }
                    }
                }
                rim_rings.insert(e_idx as u32, ring);
            }
        }

        // ---- Per-face dispatch.
        let mut face_tri_ranges: Vec<std::ops::Range<usize>> = Vec::with_capacity(faces.len());
        for (f_idx, f) in faces.iter().enumerate() {
            let range_start = out_tris.len();
            // PR-KV7: ALL loops (outer + rings) must be segments for the
            // pure-planar paths — a seg-bounded face with a CIRCLE ring (a
            // box top with a recovered round hole re-entering the pipeline)
            // must route to the generalized curved CDT below, or the
            // seg-only CDT silently covers the hole.
            let all_line = f
                .outer_loop
                .iter()
                .chain(f.inner_loops.iter().flatten())
                .all(|&e_idx| matches!(edges[e_idx as usize].curve, Curve::LineSegment));

            match f.surface {
                Surface::Plane { normal, d } if all_line => {
                    // Route non-convex (reflex-vertex) outer loops, loops with
                    // COLLINEAR boundary runs (PR-YR27 — the fan would emit
                    // zero-area glue triangles the next arrangement drops),
                    // and any face with inner loops (holes) to the CDT path;
                    // strictly-convex hole-free faces keep the existing
                    // byte-for-byte fan path. (PR-NC1: a fan is valid only for
                    // strictly-convex, hole-free polygons; CDT handles the
                    // rest with exact coverage and no Steiner points.)
                    let needs_cdt = !f.inner_loops.is_empty()
                        || planar_outer_loop_fan_unsafe(f, edges, &out_verts, normal);

                    if needs_cdt {
                        tessellate_planar_cdt_face(
                            f_idx,
                            f,
                            edges,
                            normal,
                            &out_verts,
                            &mut out_tris,
                        )?;
                    } else {
                        // ===== Planar box path (UNCHANGED — Newell fan). =====
                        let mut face_verts: Vec<u32> = f
                            .outer_loop
                            .iter()
                            .map(|&e_idx| edges[e_idx as usize].start)
                            .collect();

                        let mut newell = [0.0f64; 3];
                        let m = face_verts.len();
                        for i in 0..m {
                            let vi = out_verts[face_verts[i] as usize].as_array();
                            let vj = out_verts[face_verts[(i + 1) % m] as usize].as_array();
                            newell[0] += (vi[1] - vj[1]) * (vi[2] + vj[2]);
                            newell[1] += (vi[2] - vj[2]) * (vi[0] + vj[0]);
                            newell[2] += (vi[0] - vj[0]) * (vi[1] + vj[1]);
                        }
                        let mag =
                            (newell[0] * newell[0] + newell[1] * newell[1] + newell[2] * newell[2])
                                .sqrt();
                        if mag < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE
                        {
                            return Err(YangError::DegenerateFace { face: f_idx });
                        }
                        let n = normal.as_array();
                        let dot = newell[0] * n[0] + newell[1] * n[1] + newell[2] * n[2];
                        if dot < 0.0 {
                            face_verts.reverse();
                        }
                        for i in 1..face_verts.len() - 1 {
                            out_tris.push([face_verts[0], face_verts[i], face_verts[i + 1]]);
                        }
                        let _ = d;
                    }
                }
                Surface::Plane { normal, .. } => {
                    // ===== Curved-bounded planar faces. =====
                    // The full-circle DISK (single closed Circle outer loop,
                    // no rings) keeps the byte-for-byte cap fan. Everything
                    // else — annular-sector walls ([seg, arc, seg, arc]),
                    // holed circle caps (full-circle outer + full-circle
                    // ring), arbitrary mixed loops — goes through the
                    // generalized CDT over the spliced sample chains
                    // (PR-KV6b-1).
                    let is_disk = f.inner_loops.is_empty()
                        && f.outer_loop.len() == 1
                        && matches!(
                            &edges[f.outer_loop[0] as usize],
                            BRepEdge {
                                start,
                                end,
                                curve: Curve::Circle { .. },
                            } if start == end
                        );
                    if is_disk {
                        tessellate_cap_face(
                            f_idx,
                            f,
                            edges,
                            &rim_rings,
                            normal,
                            &mut out_verts,
                            &mut sources,
                            &mut out_tris,
                        )?;
                    } else {
                        tessellate_planar_curved_cdt_face(
                            f_idx,
                            f,
                            edges,
                            &rim_rings,
                            normal,
                            &out_verts,
                            &mut out_tris,
                        )?;
                    }
                }
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                } => {
                    tessellate_lateral_face(
                        f_idx,
                        f,
                        edges,
                        &rim_rings,
                        &inserted_rims,
                        &out_verts,
                        axis_point,
                        axis_dir,
                        radius,
                        &mut out_tris,
                    )?;
                }
                Surface::Sphere { center, radius } => {
                    tessellate_sphere_face(
                        f_idx,
                        f,
                        edges,
                        verts,
                        center,
                        radius,
                        &mut out_verts,
                        &mut sources,
                        &mut out_tris,
                    )?;
                }
                Surface::Cone {
                    apex,
                    axis_dir,
                    half_angle,
                } => {
                    tessellate_cone_face(
                        f_idx,
                        f,
                        edges,
                        &rim_rings,
                        verts,
                        apex,
                        axis_dir,
                        half_angle,
                        &mut out_verts,
                        &mut sources,
                        &mut out_tris,
                    )?;
                }
                Surface::Torus {
                    center,
                    axis_dir,
                    major_radius,
                    minor_radius,
                } => {
                    tessellate_torus_face(
                        f_idx,
                        f,
                        edges,
                        &rim_rings,
                        center,
                        axis_dir,
                        major_radius,
                        minor_radius,
                        &mut out_verts,
                        &mut sources,
                        &mut out_tris,
                    )?;
                }
            }
            face_tri_ranges.push(range_start..out_tris.len());
        }

        Ok((
            Stage1Tess {
                verts: out_verts,
                sources,
                tris: out_tris,
                face_tri_ranges,
            },
            inserted_rims,
        ))
    }
}

impl BRep {
    pub fn vertices(&self) -> &[BRepVertex] {
        &self.vertices
    }

    pub fn edges(&self) -> &[BRepEdge] {
        &self.edges
    }

    pub fn faces(&self) -> &[BRepFace] {
        &self.faces
    }

    pub fn as_mesh(&self) -> &Mesh {
        &self.mesh
    }

    pub fn into_mesh(self) -> Mesh {
        self.mesh
    }

    pub fn tessellation_map(&self) -> &TessellationMap {
        &self.tessellation
    }

    /// Per-output-triangle attribution to an input B-Rep face.
    ///
    /// Populated only by `boolean()`. `BRep::new` / `BRep::from_mesh`
    /// return an empty map.
    pub fn triangle_attribution(&self) -> &TriangleAttributionMap {
        &self.triangle_attribution
    }

    /// Per-output-FACE attribution (PR-KV13 F2), parallel to [`Self::faces`]:
    /// `face_attribution()[i]` is the `(input, face)` that output face `i`
    /// descends from (the majority over its patch's triangles, recorded during
    /// reassembly). Empty for `new`/`from_mesh`; for a `boolean()` output it has
    /// one entry per face. The kernel maps each `(input, face)` to the operand's
    /// persistent face id to chain provenance.
    pub fn face_attribution(&self) -> &[TriangleAttribution] {
        &self.face_attribution
    }

    pub fn num_verts(&self) -> usize {
        self.mesh.num_verts()
    }

    pub fn num_tris(&self) -> usize {
        self.mesh.num_tris()
    }

    /// Evaluate a [`TessellationSource`] back to its 3D point — the inverse of
    /// the Stage-1 bijection (PR-YR7, spec §4).
    ///
    /// INFALLIBLE and panic-free (P9). For the variants the cylinder and sphere
    /// pipelines emit (`BRepVertex`, `BRepEdge` over `LineSegment`/`Circle`,
    /// `BRepFace` over `Plane`/`Cylinder`/`Sphere`) it reproduces the sampled
    /// point exactly via the SAME `ortho_basis` / z-up parameterization used
    /// during sampling. The remaining cases (Cone face) never occur for those
    /// pipelines; they use a documented defensive fallback (a representative
    /// point on the surface) rather than panicking.
    pub fn eval_source(&self, src: TessellationSource) -> Point3 {
        match src {
            TessellationSource::BRepVertex(i) => {
                // The source guarantees this index is valid (it was emitted for
                // an existing B-Rep vertex). Defensive bounds-check keeps it
                // panic-free if a caller hands in a stale source.
                match self.vertices.get(i as usize) {
                    Some(v) => v.point,
                    None => Point3::new(0.0, 0.0, 0.0),
                }
            }
            TessellationSource::BRepEdge { edge, t } => {
                let Some(e) = self.edges.get(edge as usize) else {
                    return Point3::new(0.0, 0.0, 0.0);
                };
                match e.curve {
                    Curve::Parabola {
                        vertex,
                        normal,
                        axis_dir,
                        focal_length,
                    } => parabola_point(vertex, normal, axis_dir, focal_length, t),
                    Curve::Hyperbola {
                        center,
                        normal,
                        major_axis,
                        semi_transverse,
                        semi_conjugate,
                    } => hyperbola_point(
                        center,
                        normal,
                        major_axis,
                        semi_transverse,
                        semi_conjugate,
                        t,
                    ),
                    Curve::LineSegment => {
                        let s = match self.vertices.get(e.start as usize) {
                            Some(v) => v.point.as_array(),
                            None => return Point3::new(0.0, 0.0, 0.0),
                        };
                        let en = match self.vertices.get(e.end as usize) {
                            Some(v) => v.point.as_array(),
                            None => return Point3::new(0.0, 0.0, 0.0),
                        };
                        Point3::new(
                            s[0] + t * (en[0] - s[0]),
                            s[1] + t * (en[1] - s[1]),
                            s[2] + t * (en[2] - s[2]),
                        )
                    }
                    Curve::Circle {
                        center,
                        normal,
                        radius,
                    } => {
                        let (e1, e2) = ortho_basis(normal);
                        let c = center.as_array();
                        let e1a = e1.as_array();
                        let e2a = e2.as_array();
                        let (ct, st) = (t.cos(), t.sin());
                        Point3::new(
                            c[0] + radius * (ct * e1a[0] + st * e2a[0]),
                            c[1] + radius * (ct * e1a[1] + st * e2a[1]),
                            c[2] + radius * (ct * e1a[2] + st * e2a[2]),
                        )
                    }
                    Curve::Ellipse {
                        center,
                        normal,
                        major_axis,
                        major_radius,
                        minor_radius,
                    } => {
                        // PR-YR11: evaluate via the shared ellipse frame (spec §3)
                        // so a relocated vertex tagged `BRepEdge { edge, t }`
                        // round-trips exactly to its mesh position.
                        ellipse_point(center, normal, major_axis, major_radius, minor_radius, t)
                    }
                }
            }
            TessellationSource::BRepFace { face, u, v } => {
                let Some(f) = self.faces.get(face as usize) else {
                    return Point3::new(0.0, 0.0, 0.0);
                };
                match f.surface {
                    Surface::Plane { normal, d } => {
                        // Origin O = -d · normal_unit (the plane point closest
                        // to the world origin).
                        let nu = normalize3(normal.as_array());
                        let o = [-d * nu[0], -d * nu[1], -d * nu[2]];
                        let (e1, e2) = ortho_basis(normal);
                        let e1a = e1.as_array();
                        let e2a = e2.as_array();
                        Point3::new(
                            o[0] + u * e1a[0] + v * e2a[0],
                            o[1] + u * e1a[1] + v * e2a[1],
                            o[2] + u * e1a[2] + v * e2a[2],
                        )
                    }
                    Surface::Cylinder {
                        axis_point,
                        axis_dir,
                        radius,
                    } => {
                        let au = normalize3(axis_dir.as_array());
                        let (e1, e2) = ortho_basis(axis_dir);
                        let ap = axis_point.as_array();
                        let e1a = e1.as_array();
                        let e2a = e2.as_array();
                        let (cu, su) = (u.cos(), u.sin());
                        Point3::new(
                            ap[0] + v * au[0] + radius * (cu * e1a[0] + su * e2a[0]),
                            ap[1] + v * au[1] + radius * (cu * e1a[1] + su * e2a[1]),
                            ap[2] + v * au[2] + radius * (cu * e1a[2] + su * e2a[2]),
                        )
                    }
                    // PR-YR12: z-up sphere parameterization — byte-identical to
                    // `face_eval` in `tessellate_sphere_face` so an interior
                    // vertex tagged `BRepFace { u, v }` round-trips exactly.
                    Surface::Sphere { center, radius } => {
                        let c = center.as_array();
                        let (cu, su) = (u.cos(), u.sin());
                        let (cv, sv) = (v.cos(), v.sin());
                        Point3::new(
                            c[0] + radius * cv * cu,
                            c[1] + radius * cv * su,
                            c[2] + radius * sv,
                        )
                    }
                    // PR-YR16: cone FACE arm (spec §5.2). `v` is the axial
                    // height from the apex, `u` the angular param:
                    //   point(u, v) = apex + v·â + v·tanα·(cos u·ê1 + sin u·ê2)
                    // The pure apex-fan emits no `BRepFace`-cone vertices, so
                    // this arm is exercised only by the focused unit test.
                    Surface::Cone {
                        apex,
                        axis_dir,
                        half_angle,
                    } => {
                        let ax = normalize3(axis_dir.as_array());
                        let (e1, e2) = ortho_basis(axis_dir);
                        let e1a = e1.as_array();
                        let e2a = e2.as_array();
                        let ap = apex.as_array();
                        let (cu, su) = (u.cos(), u.sin());
                        let rr = v * half_angle.tan();
                        Point3::new(
                            ap[0] + v * ax[0] + rr * (cu * e1a[0] + su * e2a[0]),
                            ap[1] + v * ax[1] + rr * (cu * e1a[1] + su * e2a[1]),
                            ap[2] + v * ax[2] + rr * (cu * e1a[2] + su * e2a[2]),
                        )
                    }
                    // KV6d: torus FACE arm. `u` = φ (profile angle), `v` = θ
                    // (sweep), in the `ortho_basis(axis)` frame:
                    //   p(u,v) = center + (R + r cos u)(cos v·ê1 + sin v·ê2)
                    //            + r sin u · â
                    Surface::Torus {
                        center,
                        axis_dir,
                        major_radius,
                        minor_radius,
                    } => {
                        let ax = normalize3(axis_dir.as_array());
                        let (e1, e2) = ortho_basis(axis_dir);
                        let e1a = e1.as_array();
                        let e2a = e2.as_array();
                        let cc = center.as_array();
                        let (cu, su) = (u.cos(), u.sin());
                        let (cv, sv) = (v.cos(), v.sin());
                        let rad = major_radius + minor_radius * cu;
                        Point3::new(
                            cc[0] + rad * (cv * e1a[0] + sv * e2a[0]) + minor_radius * su * ax[0],
                            cc[1] + rad * (cv * e1a[1] + sv * e2a[1]) + minor_radius * su * ax[1],
                            cc[2] + rad * (cv * e1a[2] + sv * e2a[2]) + minor_radius * su * ax[2],
                        )
                    }
                }
            }
            // Boolean-output / degenerate sources have no B-Rep geometry to
            // invert; defensive fallback to the origin (never emitted by the
            // Stage-1 cylinder bijection the round-trip oracle exercises).
            TessellationSource::Intersection | TessellationSource::Unknown => {
                Point3::new(0.0, 0.0, 0.0)
            }
        }
    }
}

// =========================================================================
// PR-YR7 — curved Stage-1 geometry helpers
// =========================================================================

/// Normalize a `[f64; 3]`; returns the input unchanged if its length is below
/// `TAU_WORK` (defensive — callers pass real surface normals / axes).
pub(crate) fn normalize3(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < cad_primitives::TAU_WORK {
        return v;
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

/// Deterministic orthonormal in-plane basis `(e1, e2)` for the plane with
/// (not-necessarily-unit) normal `n` (PR-YR7, spec §2 "critical coupling").
///
/// USED BY BOTH Stage-1 sampling AND [`BRep::eval_source`] — if these two
/// disagree, the bijection round-trip fails. Construction:
/// 1. `nu = normalize(n)`.
/// 2. Seed = the world axis with the SMALLEST `|nu_i|` (ties broken x<y<z) —
///    the axis least aligned with `nu`, for numerical stability.
/// 3. `e1 = normalize(seed − (seed·nu)·nu)` (Gram–Schmidt).
/// 4. `e2 = nu × e1`.
///
/// `e1` and `e2` are unit and orthogonal to `nu` (and to each other). Note
/// `ortho_basis(-n)` and `ortho_basis(n)` share the SAME `e1` (the projection
/// is invariant to flipping `nu`) but have OPPOSITE `e2` (since `e2 = nu × e1`)
/// — the opposite-rim twist the lateral tessellation must compensate for.
pub(crate) fn ortho_basis(n: Vector3) -> (Vector3, Vector3) {
    let nu = normalize3(n.as_array());
    let abs = [nu[0].abs(), nu[1].abs(), nu[2].abs()];
    // Seed = world axis with smallest |component| (tie-break x < y < z).
    let seed = if abs[0] <= abs[1] && abs[0] <= abs[2] {
        [1.0, 0.0, 0.0]
    } else if abs[1] <= abs[2] {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let sdotn = seed[0] * nu[0] + seed[1] * nu[1] + seed[2] * nu[2];
    let e1_raw = [
        seed[0] - sdotn * nu[0],
        seed[1] - sdotn * nu[1],
        seed[2] - sdotn * nu[2],
    ];
    let e1 = normalize3(e1_raw);
    // e2 = nu × e1.
    let e2 = [
        nu[1] * e1[2] - nu[2] * e1[1],
        nu[2] * e1[0] - nu[0] * e1[2],
        nu[0] * e1[1] - nu[1] * e1[0],
    ];
    (
        Vector3::new(e1[0], e1[1], e1[2]),
        Vector3::new(e2[0], e2[1], e2[2]),
    )
}

/// EXACT CCW tie-break for two rim points whose f64 frame angles COLLIDE
/// (ULP twins — spec `m8_holed_disc_coplanar_overlay` §8 increment 3): the
/// sign of the exact 2D cross product of their in-frame coordinates, computed
/// in rational arithmetic over the raw f64 inputs (products and sums of f64
/// values are exact in `RBig` — no rounding anywhere). `cross(a,b) > 0` means
/// `b` lies counterclockwise of `a` in the `(e1, e2)` frame, i.e. `a` orders
/// FIRST along increasing frame angle → `Less`. A zero cross (identical exact
/// direction; distinct rim points cannot subtend it) compares `Equal`.
///
/// Only valid for points whose angular separation is far below π (the callers
/// invoke it exclusively on bit-equal f64 angle keys, where the separation is
/// sub-ULP), since a bare cross sign cannot totally order antipodal points.
fn exact_rim_ccw_tiebreak(
    center: [f64; 3],
    e1: [f64; 3],
    e2: [f64; 3],
    pa: Point3,
    pb: Point3,
) -> std::cmp::Ordering {
    use crate::coplanar_overlay::rat;
    use dashu::rational::RBig;
    if std::env::var_os("TIEBREAK_NEUTER").is_some() {
        return std::cmp::Ordering::Equal;
    }
    let frame_coords = |p: Point3| -> Option<(RBig, RBig)> {
        let a = p.as_array();
        let w = [
            rat(a[0]).ok()? - rat(center[0]).ok()?,
            rat(a[1]).ok()? - rat(center[1]).ok()?,
            rat(a[2]).ok()? - rat(center[2]).ok()?,
        ];
        let dot = |v: &[f64; 3]| -> Option<RBig> {
            Some(&w[0] * rat(v[0]).ok()? + &w[1] * rat(v[1]).ok()? + &w[2] * rat(v[2]).ok()?)
        };
        Some((dot(&e1)?, dot(&e2)?))
    };
    match (frame_coords(pa), frame_coords(pb)) {
        (Some((xa, ya)), Some((xb, yb))) => {
            let cross = &xa * &yb - &ya * &xb;
            if cross > RBig::ZERO {
                std::cmp::Ordering::Less
            } else if cross < RBig::ZERO {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        }
        // Non-finite input (never produced by the tessellation) — keep stable.
        _ => std::cmp::Ordering::Equal,
    }
}

// =========================================================================
// PR-YR11 — ONE shared ellipse frame (analogous to `ortho_basis` for circles).
//
// The ellipse parameterization
//   point(t) = C + major_radius·cos t·major + minor_radius·sin t·minor_dir
// with  minor_dir = normalize(normal) × normalize(major_axis)
// MUST be byte-identical in all THREE consumers (spec §3): Stage-4 relocation's
// `t`, `eval_source`'s `Curve::Ellipse` arm, and `is_reversed`'s ellipse tangent.
// These three helpers are the single source of truth; matching the
// `curve_contains_point` Ellipse convention (lib.rs §PR-YR9) exactly.
// =========================================================================

/// PR-YR11 (spec §3): the ellipse's in-plane minor direction
/// `minor_dir = normalize(normal) × normalize(major_axis)`. Returned as a unit
/// `[f64; 3]`; the inputs are the stored `Curve::Ellipse` `normal` / `major_axis`.
fn ellipse_frame(normal: Vector3, major_axis: Vector3) -> [f64; 3] {
    let n = normalize3(normal.as_array());
    let maj = normalize3(major_axis.as_array());
    [
        n[1] * maj[2] - n[2] * maj[1],
        n[2] * maj[0] - n[0] * maj[2],
        n[0] * maj[1] - n[1] * maj[0],
    ]
}

/// PR-YR22: evaluate the exact parabola point at parameter `t`, matching the
/// `ssi_rs::SsiCurve::Parabola` convention field-for-field:
/// `vertex + (t²/(4·focal_length))·axis_dir + t·(normal × axis_dir)`. The
/// conjugate in-plane direction `normal × axis_dir` is unit when `normal` and
/// `axis_dir` are unit and orthogonal (as ssi-rs guarantees). Used by
/// `eval_source` and the relocation round-trip oracle.
pub fn parabola_point(
    vertex: Point3,
    normal: Vector3,
    axis_dir: Vector3,
    focal_length: f64,
    t: f64,
) -> Point3 {
    let ax = axis_dir.as_array();
    let conj = [
        normal.as_array()[1] * ax[2] - normal.as_array()[2] * ax[1],
        normal.as_array()[2] * ax[0] - normal.as_array()[0] * ax[2],
        normal.as_array()[0] * ax[1] - normal.as_array()[1] * ax[0],
    ];
    let v = vertex.as_array();
    Point3::new(
        v[0] + ax[0] * t * t / (4.0 * focal_length) + conj[0] * t,
        v[1] + ax[1] * t * t / (4.0 * focal_length) + conj[1] * t,
        v[2] + ax[2] * t * t / (4.0 * focal_length) + conj[2] * t,
    )
}

/// PR-YR23: evaluate the exact hyperbola point at parameter `t`, matching the
/// `ssi_rs::SsiCurve::Hyperbola` convention field-for-field:
/// `center + (a·cosh t)·major_axis + (b·sinh t)·(normal × major_axis)` with
/// `a = semi_transverse`, `b = semi_conjugate`. The conjugate in-plane direction
/// `normal × major_axis` is unit when `normal` and `major_axis` are unit and
/// orthogonal (as ssi-rs guarantees). This traces the single `+major_axis`
/// branch (`u > 0`). Used by `eval_source` and the relocation round-trip oracle.
pub fn hyperbola_point(
    center: Point3,
    normal: Vector3,
    major_axis: Vector3,
    semi_transverse: f64,
    semi_conjugate: f64,
    t: f64,
) -> Point3 {
    let maj = major_axis.as_array();
    let conj = [
        normal.as_array()[1] * maj[2] - normal.as_array()[2] * maj[1],
        normal.as_array()[2] * maj[0] - normal.as_array()[0] * maj[2],
        normal.as_array()[0] * maj[1] - normal.as_array()[1] * maj[0],
    ];
    let c = center.as_array();
    let ch = semi_transverse * t.cosh();
    let sh = semi_conjugate * t.sinh();
    Point3::new(
        c[0] + maj[0] * ch + conj[0] * sh,
        c[1] + maj[1] * ch + conj[1] * sh,
        c[2] + maj[2] * ch + conj[2] * sh,
    )
}

fn ellipse_point(
    center: Point3,
    normal: Vector3,
    major_axis: Vector3,
    major_radius: f64,
    minor_radius: f64,
    t: f64,
) -> Point3 {
    let c = center.as_array();
    let maj = normalize3(major_axis.as_array());
    let mindir = ellipse_frame(normal, major_axis);
    let (ct, st) = (t.cos(), t.sin());
    Point3::new(
        c[0] + major_radius * ct * maj[0] + minor_radius * st * mindir[0],
        c[1] + major_radius * ct * maj[1] + minor_radius * st * mindir[1],
        c[2] + major_radius * ct * maj[2] + minor_radius * st * mindir[2],
    )
}

/// PR-YR11 (spec §3): the ellipse parameter `t` of a point `x` (assumed on / near
/// the ellipse), in the SAME frame as [`ellipse_point`]:
/// `u = (x−C)·major`, `v = (x−C)·minor_dir`,
/// `t = atan2(v / minor_radius, u / major_radius)`.
fn ellipse_param(
    x: Point3,
    center: Point3,
    normal: Vector3,
    major_axis: Vector3,
    major_radius: f64,
    minor_radius: f64,
) -> f64 {
    let c = center.as_array();
    let xa = x.as_array();
    let maj = normalize3(major_axis.as_array());
    let mindir = ellipse_frame(normal, major_axis);
    let w = [xa[0] - c[0], xa[1] - c[1], xa[2] - c[2]];
    let u = w[0] * maj[0] + w[1] * maj[1] + w[2] * maj[2];
    let v = w[0] * mindir[0] + w[1] * mindir[1] + w[2] * mindir[2];
    (v / minor_radius).atan2(u / major_radius)
}

/// PR-YR11 (spec §3): the (unnormalized) ellipse tangent at parameter `t`:
/// `−major_radius·sin t·major + minor_radius·cos t·minor_dir`. Used by
/// `is_reversed` for the exact ellipse tangent at a relocated point.
fn ellipse_tangent(
    normal: Vector3,
    major_axis: Vector3,
    major_radius: f64,
    minor_radius: f64,
    t: f64,
) -> [f64; 3] {
    let maj = normalize3(major_axis.as_array());
    let mindir = ellipse_frame(normal, major_axis);
    let (ct, st) = (t.cos(), t.sin());
    [
        -major_radius * st * maj[0] + minor_radius * ct * mindir[0],
        -major_radius * st * maj[1] + minor_radius * ct * mindir[1],
        -major_radius * st * maj[2] + minor_radius * ct * mindir[2],
    ]
}

/// PR-NC1: is the outer loop of a planar, all-LineSegment face **non-convex**
/// (does it have a reflex vertex)?
///
/// Builds `face_verts` from each outer-loop edge's `.start` (the same vertex
/// order the fan path uses), projects them into the plane's intrinsic 2D frame
/// (`ortho_basis(normal)` — the SAME projection the CDT path uses, so the
/// reflex test and the triangulation agree), then walks consecutive 2D cross
/// products. The loop's overall orientation is the sign of its signed area; any
/// turn whose cross product has the OPPOSITE sign is a reflex vertex ⇒
/// non-convex. A near-zero cross (collinear vertices) is not reflex — but it
/// IS fan-unsafe, see below.
///
/// PR-YR27 (unmasked latent, found by the yr5c chained-subtract adversary
/// once Finding 3 let the chain proceed): a CONVEX loop with a COLLINEAR
/// boundary run is also routed to the CDT. A previous boolean's output face
/// legitimately carries collinear boundary subdivisions (arrangement
/// vertices on a straight face edge, e.g. a tunnel wall's rim subdivided by
/// the neighbor cap's mesh); re-fed as input, the fan from vertex 0 emits a
/// ZERO-AREA triangle whenever a collinear chain includes vertex 0's own
/// boundary edge (`fan(v0, c, b)` over collinear `v0—c—b`). That degenerate
/// glue triangle pairs the mesh locally, but the NEXT exact arrangement
/// drops it (zero-area tris cannot be embedded), leaving a T-junction and a
/// NON-watertight kept set. The CDT triangulates the same ring with every
/// boundary sub-segment as a constraint and emits positive-area triangles
/// only. Strictly-convex hole-free loops (every fixture box) keep the
/// byte-for-byte fan path.
fn planar_outer_loop_fan_unsafe(
    f: &BRepFace,
    edges: &[BRepEdge],
    out_verts: &[Point3],
    normal: Vector3,
) -> bool {
    let pts2d = project_loop_2d(&f.outer_loop, edges, out_verts, normal);
    let m = pts2d.len();
    if m < 4 {
        // A triangle is always convex.
        return false;
    }

    // Loop orientation = sign of the 2D signed (shoelace) area.
    let mut area2 = 0.0;
    for i in 0..m {
        let a = pts2d[i];
        let b = pts2d[(i + 1) % m];
        area2 += a[0] * b[1] - b[0] * a[1];
    }
    // Degenerate (zero-area) projection: treat as convex (the fan path's
    // own degeneracy guard will reject it downstream).
    if area2.abs() < cad_primitives::TAU_WORK {
        return false;
    }
    let orient = area2.signum();

    // Tolerance scaled to the loop's area so it is invariant to model scale.
    let eps = area2.abs() * 1e-9;
    for i in 0..m {
        let prev = pts2d[(i + m - 1) % m];
        let cur = pts2d[i];
        let next = pts2d[(i + 1) % m];
        let d1 = [cur[0] - prev[0], cur[1] - prev[1]];
        let d2 = [next[0] - cur[0], next[1] - cur[1]];
        let cross = d1[0] * d2[1] - d1[1] * d2[0];
        // A turn opposite the loop orientation is a reflex vertex.
        if cross * orient < -eps {
            return true;
        }
        // PR-YR27: a (near-)zero turn is a collinear boundary run — convex,
        // but fan-UNSAFE (see the function docs): route to the CDT.
        if cross.abs() <= eps {
            return true;
        }
    }
    false
}

/// PR-NC1: project an edge-index loop's vertices (each loop edge's `.start`)
/// into the plane's intrinsic 2D frame `ortho_basis(normal)`. Returns the 2D
/// coordinates in loop order. The 3D point of vertex `v` projects to
/// `(p·e1, p·e2)` (the origin offset cancels for in-plane analysis).
fn project_loop_2d(
    loop_edges: &[u32],
    edges: &[BRepEdge],
    out_verts: &[Point3],
    normal: Vector3,
) -> Vec<[f64; 2]> {
    let (e1, e2) = ortho_basis(normal);
    let e1a = e1.as_array();
    let e2a = e2.as_array();
    loop_edges
        .iter()
        .map(|&e_idx| {
            let p = out_verts[edges[e_idx as usize].start as usize].as_array();
            [
                p[0] * e1a[0] + p[1] * e1a[1] + p[2] * e1a[2],
                p[0] * e2a[0] + p[1] * e2a[1] + p[2] * e2a[2],
            ]
        })
        .collect()
}

/// PR-NC1: tessellate a planar, all-LineSegment face that is **non-convex** or
/// has **inner loops** via a constrained Delaunay triangulation
/// (`cherchi_rs::cdt_polygon_with_holes`).
///
/// Projects the outer loop + every inner loop into the plane's intrinsic 2D
/// frame (`ortho_basis(normal)`, matching the reflex test), builds a *local*
/// `Point2` pool with a `local → global out_verts index` map, triangulates, and
/// maps the local tri indices back to global indices. Each output triangle is
/// wound to agree with the plane normal (reusing `orient_tri`, the same sign
/// rule the fan path uses).
///
/// Pushes **no** new vertices — the output indexes only into existing
/// `out_verts`, so the `TessellationMap` 1:1-on-boundary bijection is preserved
/// (no Steiner points, no boundary subdivision).
/// PR-KV6b-1: expand a B-Rep edge-index loop into its mesh-vertex polyline,
/// splicing each `Curve::Circle` edge's cached sample chain (arc chains are
/// open `[start … end]`, full circles closed seam rings). Edge traversal
/// direction is derived from loop continuity; the returned polyline lists
/// each boundary vertex ONCE (no closing duplicate).
fn loop_polyline(
    f_idx: usize,
    loop_edges: &[u32],
    edges: &[BRepEdge],
    chains: &std::collections::BTreeMap<u32, Vec<u32>>,
) -> Result<Vec<u32>, YangError> {
    let malformed = |msg: String| YangError::MalformedTopology(format!("face {f_idx}: {msg}"));

    // Single full-circle loop: the chain IS the (closed) polyline.
    if loop_edges.len() == 1 {
        let e = &edges[loop_edges[0] as usize];
        if matches!(e.curve, Curve::Circle { .. }) && e.start == e.end {
            return chains
                .get(&loop_edges[0])
                .cloned()
                .ok_or_else(|| malformed(format!("chain for edge {} not built", loop_edges[0])));
        }
    }

    // Expansion of one directed edge: the vertex sequence from its
    // traversal origin up to (EXCLUDING) its destination.
    let expand = |e_idx: u32, forward: bool| -> Result<Vec<u32>, YangError> {
        let e = &edges[e_idx as usize];
        match e.curve {
            Curve::LineSegment => Ok(vec![if forward { e.start } else { e.end }]),
            Curve::Circle { .. } => {
                let chain = chains
                    .get(&e_idx)
                    .ok_or_else(|| malformed(format!("chain for edge {e_idx} not built")))?;
                if e.start == e.end {
                    return Err(malformed(format!(
                        "full-circle edge {e_idx} inside a multi-edge loop"
                    )));
                }
                let mut seq: Vec<u32> = if forward {
                    chain[..chain.len() - 1].to_vec()
                } else {
                    chain[1..].iter().rev().copied().collect()
                };
                if seq.is_empty() {
                    seq.push(if forward { e.start } else { e.end });
                }
                Ok(seq)
            }
            _ => Err(malformed(format!(
                "loop edge {e_idx} carries an unsupported curve for Stage-1 ingestion"
            ))),
        }
    };

    // Walk with continuity, trying the first edge forward then backward.
    'attempt: for first_forward in [true, false] {
        let e0 = &edges[loop_edges[0] as usize];
        let mut cur = if first_forward { e0.start } else { e0.end };
        let mut poly: Vec<u32> = Vec::new();
        for &e_idx in loop_edges {
            let e = &edges[e_idx as usize];
            let forward = if e.start == cur {
                true
            } else if e.end == cur {
                false
            } else {
                continue 'attempt;
            };
            poly.extend(expand(e_idx, forward)?);
            cur = if forward { e.end } else { e.start };
        }
        // Closure: the walk must return to its origin.
        if cur == poly[0] {
            return Ok(poly);
        }
    }
    Err(malformed("loop is not edge-continuous".to_string()))
}

/// PR-KV6b-1: CDT tessellation of a planar face whose loops mix straight and
/// `Curve::Circle` edges (annular sectors, holed circle caps, …). The
/// boundary polylines splice the SHARED per-edge sample chains
/// ([`loop_polyline`]), so faces meeting along an arc emit identical sample
/// vertices — the watertightness mechanism. Triangulation + orientation are
/// exactly the all-segment CDT path's (no Steiner points, no boundary
/// subdivision).
#[allow(clippy::too_many_arguments)]
fn tessellate_planar_curved_cdt_face(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    chains: &std::collections::BTreeMap<u32, Vec<u32>>,
    normal: Vector3,
    out_verts: &[Point3],
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    if f.reversed {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: a planar face must carry its sense in the plane normal,              not `reversed`"
        )));
    }
    let (e1, e2) = ortho_basis(normal);
    let e1a = e1.as_array();
    let e2a = e2.as_array();
    let project = |g: u32| -> cad_primitives::Point2 {
        let p = out_verts[g as usize].as_array();
        cad_primitives::Point2::new(
            p[0] * e1a[0] + p[1] * e1a[1] + p[2] * e1a[2],
            p[0] * e2a[0] + p[1] * e2a[1] + p[2] * e2a[2],
        )
    };

    let mut local_verts: Vec<cad_primitives::Point2> = Vec::new();
    let mut global_of_local: Vec<u32> = Vec::new();
    let mut local_of_global: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut intern = |g: u32,
                      local_verts: &mut Vec<cad_primitives::Point2>,
                      global_of_local: &mut Vec<u32>|
     -> u32 {
        if let Some(&l) = local_of_global.get(&g) {
            return l;
        }
        let l = local_verts.len() as u32;
        local_verts.push(project(g));
        global_of_local.push(g);
        local_of_global.insert(g, l);
        l
    };

    let outer_poly = loop_polyline(f_idx, &f.outer_loop, edges, chains)?;
    let outer_local: Vec<u32> = outer_poly
        .iter()
        .map(|&g| intern(g, &mut local_verts, &mut global_of_local))
        .collect();
    let mut holes_local: Vec<Vec<u32>> = Vec::new();
    for inner in &f.inner_loops {
        let poly = loop_polyline(f_idx, inner, edges, chains)?;
        holes_local.push(
            poly.iter()
                .map(|&g| intern(g, &mut local_verts, &mut global_of_local))
                .collect(),
        );
    }

    // Diagnostic probe (env-gated, zero-cost off): dump the exact CDT inputs
    // (bit-precise) + outputs for one face, to extract minimal repros of
    // boundary-conformality failures.
    let cdt_probe = std::env::var("YANG_CDT_PROBE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|want| want == f_idx)
        .unwrap_or(false);
    if cdt_probe {
        eprintln!("[cdt-probe] face {f_idx}: verts {}", local_verts.len());
        for (i, p) in local_verts.iter().enumerate() {
            eprintln!(
                "[cdt-probe] v {i} = ({:?}, {:?}) bits=({:#x},{:#x})",
                p.x(),
                p.y(),
                p.x().to_bits(),
                p.y().to_bits()
            );
        }
        eprintln!("[cdt-probe] outer = {outer_local:?}");
        eprintln!("[cdt-probe] holes = {holes_local:?}");
    }
    // FLOOD-FILL variant (M8 holed-disc increment 3, spec
    // `m8_holed_disc_coplanar_overlay` §8): rim-override ULP twins put femto
    // slivers along the boundary chords, and the plain variant's f64 centroid
    // parity misclassifies them (the F0047 "parity slitting" class) — the cap
    // then disagrees with the shared rim ring and the Stage-0 mesh goes
    // non-manifold. The flood-fill variant classifies the outer region
    // topologically and (since increment 3) hole parity exactly; kernel-v2's
    // render cores made the same migration in `kv2_cdt_triangulation_core`.
    let local_tris = cherchi_rs::triangulation::cdt_polygon_with_holes_floodfill(
        &local_verts,
        &outer_local,
        &holes_local,
    )
    .map_err(|e| {
        YangError::MalformedTopology(format!("face {f_idx}: CDT triangulation failed: {e}"))
    })?;
    if cdt_probe {
        eprintln!("[cdt-probe] tris = {local_tris:?}");
    }

    let nu = normalize3(normal.as_array());
    for t in &local_tris {
        let mut tri = [
            global_of_local[t[0] as usize],
            global_of_local[t[1] as usize],
            global_of_local[t[2] as usize],
        ];
        orient_tri(out_verts, &mut tri, nu);
        out_tris.push(tri);
    }
    Ok(())
}

fn tessellate_planar_cdt_face(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    normal: Vector3,
    out_verts: &[Point3],
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    // Build local 2D pool + local→global map. Each loop vertex is keyed by its
    // global `out_verts` index so shared vertices map to one local index.
    let (e1, e2) = ortho_basis(normal);
    let e1a = e1.as_array();
    let e2a = e2.as_array();
    let project = |g: u32| -> cad_primitives::Point2 {
        let p = out_verts[g as usize].as_array();
        cad_primitives::Point2::new(
            p[0] * e1a[0] + p[1] * e1a[1] + p[2] * e1a[2],
            p[0] * e2a[0] + p[1] * e2a[1] + p[2] * e2a[2],
        )
    };

    let mut local_verts: Vec<cad_primitives::Point2> = Vec::new();
    let mut global_of_local: Vec<u32> = Vec::new();
    let mut local_of_global: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();

    let intern = |g: u32,
                  local_verts: &mut Vec<cad_primitives::Point2>,
                  global_of_local: &mut Vec<u32>,
                  local_of_global: &mut std::collections::HashMap<u32, u32>|
     -> u32 {
        if let Some(&l) = local_of_global.get(&g) {
            return l;
        }
        let l = local_verts.len() as u32;
        local_verts.push(project(g));
        global_of_local.push(g);
        local_of_global.insert(g, l);
        l
    };

    let loop_to_local = |loop_edges: &[u32],
                         local_verts: &mut Vec<cad_primitives::Point2>,
                         global_of_local: &mut Vec<u32>,
                         local_of_global: &mut std::collections::HashMap<u32, u32>|
     -> Vec<u32> {
        loop_edges
            .iter()
            .map(|&e_idx| {
                let g = edges[e_idx as usize].start;
                intern(g, local_verts, global_of_local, local_of_global)
            })
            .collect()
    };

    let outer_local = loop_to_local(
        &f.outer_loop,
        &mut local_verts,
        &mut global_of_local,
        &mut local_of_global,
    );
    let holes_local: Vec<Vec<u32>> = f
        .inner_loops
        .iter()
        .map(|inner| {
            loop_to_local(
                inner,
                &mut local_verts,
                &mut global_of_local,
                &mut local_of_global,
            )
        })
        .collect();

    let local_tris = cherchi_rs::cdt_polygon_with_holes(&local_verts, &outer_local, &holes_local)
        .map_err(|e| {
        YangError::MalformedTopology(format!("face {f_idx}: CDT triangulation failed: {e}"))
    })?;

    let nu = normalize3(normal.as_array());
    for t in &local_tris {
        let mut tri = [
            global_of_local[t[0] as usize],
            global_of_local[t[1] as usize],
            global_of_local[t[2] as usize],
        ];
        orient_tri(out_verts, &mut tri, nu);
        out_tris.push(tri);
    }
    Ok(())
}

/// PR-YR7: tessellate a planar disk cap bounded by a single `Curve::Circle`
/// edge. A new center Steiner vertex (source `BRepFace { face, u: 0, v: 0 }`,
/// which `eval_source` maps to the plane origin = the rim center) fans over the
/// cached rim ring → `N` triangles, wound to agree with the cap plane normal.
#[allow(clippy::too_many_arguments)]
fn tessellate_cap_face(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    rim_rings: &std::collections::BTreeMap<u32, Vec<u32>>,
    normal: Vector3,
    out_verts: &mut Vec<Point3>,
    sources: &mut Vec<TessellationSource>,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    // Find the (single) Circle boundary edge.
    let circle_edges: Vec<u32> = f
        .outer_loop
        .iter()
        .copied()
        .filter(|&e| matches!(edges[e as usize].curve, Curve::Circle { .. }))
        .collect();
    if circle_edges.len() != 1 {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: planar cap must be bounded by exactly one Circle edge, found {}",
            circle_edges.len()
        )));
    }
    let ring = rim_rings.get(&circle_edges[0]).ok_or_else(|| {
        YangError::MalformedTopology(format!(
            "face {f_idx}: rim ring for edge {} not built",
            circle_edges[0]
        ))
    })?;
    let nseg = ring.len();
    if nseg < 3 {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: cap rim ring has {nseg} samples (< 3)"
        )));
    }

    // Center Steiner vertex = the rim center. For a `Curve::Circle` boundary the
    // center equals the cap plane origin; we read it from the circle to keep it
    // exact, and tag its source so `eval_source` reproduces it.
    let Curve::Circle { center, .. } = edges[circle_edges[0] as usize].curve else {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: cap boundary edge is not a Circle"
        )));
    };
    // The center Steiner vertex sits at the rim center. Its source is the cap
    // face's surface params `(u, v)` such that `eval_source` reproduces it:
    // `center = O + u·e1 + v·e2`, `O = −d·n_unit`. Solve `u = (center−O)·e1`,
    // `v = (center−O)·e2` (e1,e2 orthonormal). For a rim center that already
    // lies on the world-origin normal line (the unit cylinder) `O == center`
    // and `u = v = 0`, but the general off-origin cap needs the offset.
    let (e1c, e2c) = ortho_basis(normal);
    let nuc = normalize3(normal.as_array());
    let dval = match f.surface {
        Surface::Plane { d, .. } => d,
        _ => 0.0,
    };
    let o = [-dval * nuc[0], -dval * nuc[1], -dval * nuc[2]];
    let cc = center.as_array();
    let rel = [cc[0] - o[0], cc[1] - o[1], cc[2] - o[2]];
    let e1ca = e1c.as_array();
    let e2ca = e2c.as_array();
    let u_param = rel[0] * e1ca[0] + rel[1] * e1ca[1] + rel[2] * e1ca[2];
    let v_param = rel[0] * e2ca[0] + rel[1] * e2ca[1] + rel[2] * e2ca[2];
    let center_vi = out_verts.len() as u32;
    out_verts.push(center);
    sources.push(TessellationSource::BRepFace {
        face: f_idx as u32,
        u: u_param,
        v: v_param,
    });

    let nu = normalize3(normal.as_array());
    // Fan: triangle (center, ring[k], ring[k+1]); orient to the plane normal.
    for k in 0..nseg {
        let a = ring[k];
        let bnext = ring[(k + 1) % nseg];
        let mut tri = [center_vi, a, bnext];
        orient_tri(out_verts, &mut tri, nu);
        out_tris.push(tri);
    }
    Ok(())
}

/// PR-YR7: tessellate the lateral tube of a cylinder (2 axial rings → `2N`
/// triangles, watertight via the shared cached rim rings).
///
/// HAZARD (spec §6): the bottom rim circle has `normal = −axis_dir`, the top
/// `+axis_dir`. `ortho_basis(−d)` and `ortho_basis(+d)` share `e1` but have
/// OPPOSITE `e2`, so the two rings — built at the same parameter angle `θ_k` in
/// their OWN frames — counter-rotate. To align quads by GEOMETRIC azimuth, the
/// bottom ring index for top azimuth `θ_k` is `(N − k) mod N` (its stored angle
/// is `2π − θ_k`). `ring[0]` of each rim is its seam vertex at azimuth 0, so
/// quad 0 aligns.
#[allow(clippy::too_many_arguments)]
fn tessellate_lateral_face(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    rim_rings: &std::collections::BTreeMap<u32, Vec<u32>>,
    inserted_rims: &std::collections::BTreeSet<u32>,
    out_verts: &[Point3],
    axis_point: Point3,
    axis_dir: Vector3,
    _radius: f64,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    // Dispatch on the boundary vocabulary:
    // - 2 FULL-circle rims (+ seam rulings)         → the canonical tube
    // - 2 ARCS + ruling segments (PR-KV6b-1)        → the partial patch strip
    // Anything else is MalformedTopology (loud).
    let full_rims: Vec<u32> = f
        .outer_loop
        .iter()
        .copied()
        .filter(|&e| {
            let ed = &edges[e as usize];
            matches!(ed.curve, Curve::Circle { .. }) && ed.start == ed.end
        })
        .collect();
    let arcs: Vec<u32> = f
        .outer_loop
        .iter()
        .copied()
        .filter(|&e| {
            let ed = &edges[e as usize];
            matches!(ed.curve, Curve::Circle { .. }) && ed.start != ed.end
        })
        .collect();

    let au = normalize3(axis_dir.as_array());
    let ap = axis_point.as_array();
    let rim_param = |e: u32| -> f64 {
        if let Curve::Circle { center, .. } = edges[e as usize].curve {
            let c = center.as_array();
            (c[0] - ap[0]) * au[0] + (c[1] - ap[1]) * au[1] + (c[2] - ap[2]) * au[2]
        } else {
            0.0
        }
    };
    // The stored normal's sense along the axis determines each chain's
    // angular direction (its frame's e2 = normal × e1 mirrors with the
    // normal). Two chains co-rotate when their normal signs agree.
    let rim_sense = |e: u32| -> f64 {
        if let Curve::Circle { normal, .. } = edges[e as usize].curve {
            let n = normalize3(normal.as_array());
            (n[0] * au[0] + n[1] * au[1] + n[2] * au[2]).signum()
        } else {
            1.0
        }
    };
    // Orientation target: outward radial for a solid lateral, inward for a
    // cavity wall (`reversed` — PR-KV6b-1, the washer's inner tube and the
    // partial revolve's inner-bore wall).
    let orient_target = |verts: &[Point3], tri: &[u32; 3]| -> [f64; 3] {
        let n = radial_outward_normal(verts, tri, ap, au);
        if f.reversed {
            [-n[0], -n[1], -n[2]]
        } else {
            n
        }
    };

    if full_rims.len() == 2 && arcs.is_empty() {
        // ===== Canonical tube (KV5a/M5 shape) =====
        let (mut bottom_e, mut top_e) = (full_rims[0], full_rims[1]);
        if rim_param(bottom_e) > rim_param(top_e) {
            std::mem::swap(&mut bottom_e, &mut top_e);
        }

        // PR-M8 disc-rim crossing: when EITHER rim carries inserted crossing
        // points the uniform index-pairing (`b_index`) no longer holds — pair
        // the two rings by GEOMETRIC azimuth instead. The uniform path is left
        // 100% untouched for all other cylinders.
        if inserted_rims.contains(&bottom_e) || inserted_rims.contains(&top_e) {
            return tessellate_lateral_azimuth_merge(
                f_idx, f, rim_rings, bottom_e, top_e, out_verts, axis_point, axis_dir, out_tris,
            );
        }

        let bottom_ring = rim_rings.get(&bottom_e).ok_or_else(|| {
            YangError::MalformedTopology(format!(
                "face {f_idx}: bottom rim ring {bottom_e} not built"
            ))
        })?;
        let top_ring = rim_rings.get(&top_e).ok_or_else(|| {
            YangError::MalformedTopology(format!("face {f_idx}: top rim ring {top_e} not built"))
        })?;
        let nseg = top_ring.len();
        if nseg < 3 || bottom_ring.len() != nseg {
            return Err(YangError::MalformedTopology(format!(
                "face {f_idx}: cylinder rims have mismatched / too-few samples"
            )));
        }

        // Connect by geometric azimuth. The classic shape stores bottom
        // normal = −axis, top = +axis (counter-rotating frames) ⇒ bottom
        // index (N−k). A washer INNER tube stores the mirrored senses ⇒
        // the rings co-rotate and align index-for-index. Generalize via the
        // product of the stored-normal senses (PR-KV6b-1).
        let co_rotating = rim_sense(bottom_e) * rim_sense(top_e) > 0.0;
        let b_index = |k: usize| -> usize {
            if co_rotating {
                k
            } else {
                (nseg - k) % nseg
            }
        };
        for k in 0..nseg {
            let kn = (k + 1) % nseg;
            let t0 = top_ring[k];
            let t1 = top_ring[kn];
            let b0 = bottom_ring[b_index(k)];
            let b1 = bottom_ring[b_index(kn)];
            for mut tri in [[b0, b1, t1], [b0, t1, t0]] {
                let n = orient_target(out_verts, &tri);
                orient_tri(out_verts, &mut tri, n);
                out_tris.push(tri);
            }
        }
        return Ok(());
    }

    if arcs.len() == 2
        && full_rims.is_empty()
        && f.outer_loop.iter().all(|&e| {
            matches!(
                edges[e as usize].curve,
                Curve::Circle { .. } | Curve::LineSegment
            )
        })
    {
        // ===== Partial patch (PR-KV6b-1): 2 sweep arcs + ruling segments =====
        let (mut bottom_e, mut top_e) = (arcs[0], arcs[1]);
        if rim_param(bottom_e) > rim_param(top_e) {
            std::mem::swap(&mut bottom_e, &mut top_e);
        }
        let bottom = rim_rings.get(&bottom_e).ok_or_else(|| {
            YangError::MalformedTopology(format!("face {f_idx}: arc chain {bottom_e} not built"))
        })?;
        let top = rim_rings.get(&top_e).ok_or_else(|| {
            YangError::MalformedTopology(format!("face {f_idx}: arc chain {top_e} not built"))
        })?;
        if bottom.len() != top.len() || bottom.len() < 2 {
            return Err(YangError::MalformedTopology(format!(
                "face {f_idx}: partial-cylinder arc chains have mismatched sample counts                  ({} vs {})",
                bottom.len(),
                top.len()
            )));
        }
        // Chains are open polylines [start … end]; with agreeing stored
        // senses they are azimuth-aligned index-for-index, with mirrored
        // senses index k pairs with (M−k).
        let m = bottom.len() - 1;
        let co_rotating = rim_sense(bottom_e) * rim_sense(top_e) > 0.0;
        let b_index = |k: usize| -> usize {
            if co_rotating {
                k
            } else {
                m - k
            }
        };
        for k in 0..m {
            let t0 = top[k];
            let t1 = top[k + 1];
            let b0 = bottom[b_index(k)];
            let b1 = bottom[b_index(k + 1)];
            for mut tri in [[b0, b1, t1], [b0, t1, t0]] {
                let n = orient_target(out_verts, &tri);
                orient_tri(out_verts, &mut tri, n);
                out_tris.push(tri);
            }
        }
        return Ok(());
    }

    Err(YangError::MalformedTopology(format!(
        "face {f_idx}: cylinder lateral must be bounded by exactly 2 full-circle rims          (canonical tube) or 2 arcs + ruling segments (partial patch); found {} full          rims and {} arcs",
        full_rims.len(),
        arcs.len()
    )))
}

/// PR-M8 disc-rim crossing: tessellate the canonical tube when one or both
/// rims carry inserted crossing points, so the uniform `(N−k)` index-pairing
/// no longer aligns the two rings.
///
/// Both rings are projected into ONE SHARED `ortho_basis(axis)` frame (so
/// their azimuths are GLOBAL, not per-rim-frame), sorted by azimuth, and
/// verified to present the SAME azimuth multiset within `tol = (2π/n)·0.25`
/// (the crossing point is shared between both rims by construction — Stage 0
/// projects each cap crossing onto the opposite rim — so a missing match is a
/// malformed input, NOT something to fudge). The quad strip then pairs
/// consecutive-by-azimuth vertices, reusing `orient_target`/`orient_tri`.
#[allow(clippy::too_many_arguments)]
fn tessellate_lateral_azimuth_merge(
    f_idx: usize,
    f: &BRepFace,
    rim_rings: &std::collections::BTreeMap<u32, Vec<u32>>,
    bottom_e: u32,
    top_e: u32,
    out_verts: &[Point3],
    axis_point: Point3,
    axis_dir: Vector3,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    let bottom_ring = rim_rings.get(&bottom_e).ok_or_else(|| {
        YangError::MalformedTopology(format!(
            "face {f_idx}: bottom rim ring {bottom_e} not built (azimuth merge)"
        ))
    })?;
    let top_ring = rim_rings.get(&top_e).ok_or_else(|| {
        YangError::MalformedTopology(format!(
            "face {f_idx}: top rim ring {top_e} not built (azimuth merge)"
        ))
    })?;
    let n = top_ring.len();
    if n < 3 || bottom_ring.len() != n {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: azimuth-merge rims have mismatched / too-few samples ({} vs {})",
            bottom_ring.len(),
            top_ring.len()
        )));
    }

    // ONE shared frame for both rims → global azimuth.
    let au = normalize3(axis_dir.as_array());
    let (e1, e2) = ortho_basis(Vector3::new(au[0], au[1], au[2]));
    let (e1, e2) = (e1.as_array(), e2.as_array());
    let ap = axis_point.as_array();
    let azimuth = |vi: u32| -> f64 {
        let p = out_verts[vi as usize].as_array();
        let w = [p[0] - ap[0], p[1] - ap[1], p[2] - ap[2]];
        let x = w[0] * e1[0] + w[1] * e1[1] + w[2] * e1[2];
        let y = w[0] * e2[0] + w[1] * e2[1] + w[2] * e2[2];
        y.atan2(x).rem_euclid(2.0 * std::f64::consts::PI)
    };

    // Sort each ring's vertex indices by global azimuth. ULP-twin crossing
    // points collide on the f64 azimuth key (spec
    // `m8_holed_disc_coplanar_overlay` §8 F3): break ties by the EXACT
    // angular order in the SHARED frame, so both rings sort identically and
    // the positional pairing below cannot twist the strip between twins.
    let sort_by_az = |ring: &[u32]| -> Vec<(f64, u32)> {
        let mut v: Vec<(f64, u32)> = ring.iter().map(|&vi| (azimuth(vi), vi)).collect();
        v.sort_by(|a, b| match a.0.total_cmp(&b.0) {
            std::cmp::Ordering::Equal => {
                exact_rim_ccw_tiebreak(ap, e1, e2, out_verts[a.1 as usize], out_verts[b.1 as usize])
            }
            ord => ord,
        });
        v
    };
    let bot = sort_by_az(bottom_ring);
    let top = sort_by_az(top_ring);

    // The two sorted rings are CIRCULAR sequences: the 0/2π cut can split
    // geometrically-identical azimuths across the wrap (a RECOVERED rim's
    // seam vertex at y = −ε maps to 2π−ε under `rem_euclid` while the other
    // rim's sits at exactly 0), shifting one sorted array by a slot — the
    // F0086 chained swiss-cheese wall (task #62). Align cyclically: pair
    // bot[k] ↔ top[(k+shift) % n], with the shift chosen so top[shift] is
    // circularly nearest bot[0]. The multiset check below is unchanged in
    // strength (pairwise within tol, no silent fudge) — it just compares the
    // cyclically aligned pairs.
    let two_pi = 2.0 * std::f64::consts::PI;
    let circ = |x: f64, y: f64| {
        let d = (x - y).abs();
        d.min(two_pi - d)
    };
    let mut shift = 0usize;
    let mut best = f64::INFINITY;
    for (j, t) in top.iter().enumerate() {
        let d = circ(bot[0].0, t.0);
        if d < best {
            best = d;
            shift = j;
        }
    }
    if std::env::var_os("YANG_SHIFT_NEUTER").is_some() {
        shift = 0;
    }

    // Verify the SAME azimuth multiset (no silent fudge): pairwise within tol.
    let tol = (two_pi / n as f64) * 0.25;
    for k in 0..n {
        let d = circ(bot[k].0, top[(k + shift) % n].0);
        if d > tol {
            // Diagnostic probe (env-gated): dump both sorted rings so a
            // multiset mismatch self-localizes (phase shift vs count skew vs
            // stray point).
            if std::env::var_os("YANG_AZMERGE_PROBE").is_some() {
                eprintln!(
                    "[azmerge-probe] face {f_idx}: n={n} tol={tol} shift={shift}\n  \
                     bottom: {:?}\n  top:    {:?}",
                    bot.iter().map(|(az, _)| *az).collect::<Vec<_>>(),
                    top.iter().map(|(az, _)| *az).collect::<Vec<_>>(),
                );
            }
            return Err(YangError::MalformedTopology(format!(
                "face {f_idx}: azimuth-merge rims disagree at index {k} (bottom {} vs top {}, \
                 shift {shift}, tol {tol})",
                bot[k].0,
                top[(k + shift) % n].0
            )));
        }
    }

    // Orientation target (outward radial; inward for a cavity wall).
    let orient_target = |verts: &[Point3], tri: &[u32; 3]| -> [f64; 3] {
        let nrm = radial_outward_normal(verts, tri, ap, au);
        if f.reversed {
            [-nrm[0], -nrm[1], -nrm[2]]
        } else {
            nrm
        }
    };

    for k in 0..n {
        let kn = (k + 1) % n;
        let t0 = top[(k + shift) % n].1;
        let t1 = top[(kn + shift) % n].1;
        let b0 = bot[k].1;
        let b1 = bot[kn].1;
        for mut tri in [[b0, b1, t1], [b0, t1, t0]] {
            let nrm = orient_target(out_verts, &tri);
            orient_tri(out_verts, &mut tri, nrm);
            out_tris.push(tri);
        }
    }
    Ok(())
}

/// PR-YR12 (P2b): tessellate a closed solid-sphere face (one `Surface::Sphere`
/// bounded by a single `Curve::Circle` meridian seam) into a watertight
/// latitude/longitude grid mesh with a bijective `TessellationMap`.
///
/// Mirrors `tessellate_lateral_face` / `tessellate_cap_face` in style:
/// - Fixed z-up parameterization (spec §2):
///   `face_eval(u, v) = center + r·(cos v·cos u, cos v·sin u, sin v)`,
///   `u = 2π·i/n_lon`, `v = −π/2 + π·j/n_lat`, seam at `u = 0`.
/// - Chord bound `d_ε = 1e-2 × 2r√3` (the AABB space diagonal of the sphere,
///   spec §3) — `n_lon` / `n_lat` are refined honestly; the bound is fixed.
/// - The two pole vertices are the B-Rep verts `seam.start` (south) /
///   `seam.end` (north), already seeded 1:1 into `out_verts`/`sources`, so they
///   are SHARED (single vertex each → watertight pole closure). The seam column
///   (`i = 0`) is REUSED via the modular wrap `(i+1)%n_lon` (no welding).
/// - Sources: poles → `BRepVertex`; seam column → `BRepEdge { seam, t }` (the
///   recovered seam-frame angle); interior columns → `BRepFace { f_idx, u, v }`.
#[allow(clippy::too_many_arguments)]
fn tessellate_sphere_face(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    verts: &[BRepVertex],
    center: Point3,
    radius: f64,
    out_verts: &mut Vec<Point3>,
    sources: &mut Vec<TessellationSource>,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    use std::f64::consts::PI;

    // ---- Find the single Circle meridian seam edge in the outer loop.
    let circle_edges: Vec<u32> = f
        .outer_loop
        .iter()
        .copied()
        .filter(|&e| matches!(edges[e as usize].curve, Curve::Circle { .. }))
        .collect();
    if circle_edges.len() != 1 {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: sphere must be bounded by exactly one Circle seam edge, found {}",
            circle_edges.len()
        )));
    }
    let seam_edge_index = circle_edges[0];
    let seam = &edges[seam_edge_index as usize];
    let Curve::Circle { normal, .. } = seam.curve else {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: sphere seam edge {seam_edge_index} is not a Circle"
        )));
    };

    // ---- Pole B-Rep vertices (south = seam.start, north = seam.end). These are
    // already mesh verts 0..verts.len() (seeded 1:1), so REUSE the indices — no
    // duplicate pushes. Bounds-check the indices (P9: no panic on B-Rep data).
    let south_vi = seam.start;
    let north_vi = seam.end;
    if verts.get(south_vi as usize).is_none() {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: sphere seam south pole vertex {south_vi} out of range"
        )));
    }
    if verts.get(north_vi as usize).is_none() {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: sphere seam north pole vertex {north_vi} out of range"
        )));
    }

    // ---- Chord bound + honest grid refinement (spec §3). The bound `d_ε` is
    // FIXED at `1e-2·2r√3`; we only raise N (never widen the tolerance, P9/P10).
    //
    // The per-segment **arc** sagitta `r·(1−cos θ)` bounds deviation at edge
    // midpoints, but oracle 1 also samples each triangle's CENTROID, and a flat
    // triangle inscribed in the sphere dips inward more than its edge midpoints
    // (worst at the long, thin pole-fan triangles). To keep the centroid within
    // `d_ε` we size each segment to half the budget (`d_ε/2`) — this is honest
    // refinement (more triangles), NOT tolerance widening. The factor 2 leaves a
    // comfortable margin across the corpus (verified: worst centroid deviation
    // ≈ 0.82·d_ε), and the ratio is scale-invariant so one N pair fits all radii.
    let d_eps = sphere_chord_bound(radius);
    let seg_budget = d_eps / 2.0;
    // n_lon: smallest N ≥ 3 with r·(1 − cos(π/N)) ≤ d_ε/2 (equator chord).
    let mut n_lon = 3usize;
    if seg_budget > 0.0 {
        while radius * (1.0 - (PI / n_lon as f64).cos()) > seg_budget {
            n_lon += 1;
        }
    }
    // n_lat: smallest N ≥ 2 with r·(1 − cos(π/(2N))) ≤ d_ε/2 (meridian
    // half-circle of total turn π split into N segments → half-angle π/(2N)).
    let mut n_lat = 2usize;
    if seg_budget > 0.0 {
        while radius * (1.0 - (PI / (2.0 * n_lat as f64)).cos()) > seg_budget {
            n_lat += 1;
        }
    }

    // ---- Seam frame (for per-sample seam-angle recovery, mirroring the
    // cylinder `phi0`) and the z-up surface evaluator.
    let (e1, e2) = ortho_basis(normal);
    let e1a = e1.as_array();
    let e2a = e2.as_array();
    let cen = center.as_array();
    let face_eval = |u: f64, v: f64| -> [f64; 3] {
        let (cu, su) = (u.cos(), u.sin());
        let (cv, sv) = (v.cos(), v.sin());
        [
            cen[0] + radius * cv * cu,
            cen[1] + radius * cv * su,
            cen[2] + radius * sv,
        ]
    };

    // ---- Interior latitude rings j = 1..n_lat (n_lat-1 rings strictly between
    // the poles). rings[j-1] is the ring at latitude index j.
    let mut rings: Vec<Vec<u32>> = Vec::with_capacity(n_lat - 1);
    for j in 1..n_lat {
        let v_j = -PI / 2.0 + PI * (j as f64) / (n_lat as f64);
        let mut ring: Vec<u32> = Vec::with_capacity(n_lon);
        for i in 0..n_lon {
            let u_i = 2.0 * PI * (i as f64) / (n_lon as f64);
            let pos = face_eval(u_i, v_j);
            let vi = out_verts.len() as u32;
            out_verts.push(Point3::new(pos[0], pos[1], pos[2]));
            let src = if i == 0 {
                // Seam column → recover its angle in the seam circle's frame so
                // `eval_source(BRepEdge{seam, t})` reproduces this point exactly.
                let w = [pos[0] - cen[0], pos[1] - cen[1], pos[2] - cen[2]];
                let wx = w[0] * e1a[0] + w[1] * e1a[1] + w[2] * e1a[2];
                let wy = w[0] * e2a[0] + w[1] * e2a[1] + w[2] * e2a[2];
                TessellationSource::BRepEdge {
                    edge: seam_edge_index,
                    t: wy.atan2(wx),
                }
            } else {
                TessellationSource::BRepFace {
                    face: f_idx as u32,
                    u: u_i,
                    v: v_j,
                }
            };
            sources.push(src);
            ring.push(vi);
        }
        rings.push(ring);
    }

    // ---- Triangles, each oriented by the full outward radial normal.
    let mut push_oriented = |mut tri: [u32; 3], out_verts: &[Point3]| {
        let n = sphere_outward_normal(out_verts, &tri, center);
        orient_tri(out_verts, &mut tri, n);
        out_tris.push(tri);
    };

    // South fan (poles share a single vertex; seam column reused via wrap).
    let first = &rings[0];
    for i in 0..n_lon {
        push_oriented([south_vi, first[i], first[(i + 1) % n_lon]], out_verts);
    }
    // North fan.
    let last_idx = rings.len() - 1;
    let last = &rings[last_idx];
    for i in 0..n_lon {
        push_oriented([north_vi, last[(i + 1) % n_lon], last[i]], out_verts);
    }
    // Middle bands between consecutive interior rings (empty when n_lat == 2).
    for j in 0..rings.len() - 1 {
        let lo = rings[j].clone();
        let up = rings[j + 1].clone();
        for i in 0..n_lon {
            let inext = (i + 1) % n_lon;
            let (a, b, c, d) = (lo[i], lo[inext], up[inext], up[i]);
            push_oriented([a, b, c], out_verts);
            push_oriented([a, c, d], out_verts);
        }
    }

    Ok(())
}

/// PR-YR16 (P2c): tessellate a closed solid-cone lateral face (one
/// `Surface::Cone` bounded by a single base-rim `Curve::Circle`) into a
/// watertight apex fan with a bijective `TessellationMap`.
///
/// Spec §1/§2: the cone lateral is topologically a DISK — its only boundary is
/// the base circle, the apex a single interior singular point (no seam edge).
/// Because the cone is ruled (straight generators apex→rim, exactly on the
/// surface), the lateral is a PURE fan with NO interior rings: `N` triangles
/// (apex, `ring[k]`, `ring[(k+1) % N]`) over the cached base-rim ring. The apex
/// is the pre-seeded B-Rep vertex (`verts` are seeded 1:1 into `out_verts` at
/// the top of `BRep::new`), located by exact position match to
/// `Surface::Cone.apex` within `TAU_MODEL` and REUSED (no duplicate keeps
/// watertight + Euler valid). The base cap is tessellated by the existing
/// `tessellate_cap_face` over the SAME ring (the watertightness mechanism), and
/// each triangle is oriented outward via `cone_outward_normal` + `orient_tri`.
#[allow(clippy::too_many_arguments)]
fn tessellate_cone_face(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    rim_rings: &std::collections::BTreeMap<u32, Vec<u32>>,
    verts: &[BRepVertex],
    apex: Point3,
    axis_dir: Vector3,
    half_angle: f64,
    out_verts: &mut [Point3],
    _sources: &mut [TessellationSource],
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    // ---- Find the single base-rim Circle edge. A cone face on a triangle (no
    // base rim Circle in its loop) is MalformedTopology (loud), mirroring the
    // cylinder/sphere "wrong boundary" rejection.
    let circle_edges: Vec<u32> = f
        .outer_loop
        .iter()
        .copied()
        .filter(|&e| matches!(edges[e as usize].curve, Curve::Circle { .. }))
        .collect();
    // A cone face is bounded by EITHER one base rim (an apex-pointed cone — a
    // topological disk fanned from the apex, PR-YR16) OR two rims at different
    // radii (a FRUSTUM band — the ruled analog of the cylinder tube, KV6c 5b;
    // kernel-v2 revolve produces these, since the profile cannot reach the
    // axis). Anything else is MalformedTopology (loud).
    match circle_edges.as_slice() {
        [rim_e] => {
            let ring = rim_rings.get(rim_e).ok_or_else(|| {
                YangError::MalformedTopology(format!(
                    "face {f_idx}: rim ring for edge {rim_e} not built"
                ))
            })?;
            let nseg = ring.len();
            if nseg < 3 {
                return Err(YangError::MalformedTopology(format!(
                    "face {f_idx}: cone rim ring has {nseg} samples (< 3)"
                )));
            }
            // Locate the pre-seeded apex mesh vertex by exact position match to
            // the cone's `apex` (within `TAU_MODEL`). The B-Rep verts are seeded
            // 1:1 into `out_verts` at the top of `BRep::new`, so a vertex's
            // B-Rep index IS its mesh index. REUSE it (no duplicate apex push →
            // watertight). No match → loud MalformedTopology.
            let ap = apex.as_array();
            let apex_vi = verts
                .iter()
                .position(|bv| {
                    let p = bv.point.as_array();
                    let dx = p[0] - ap[0];
                    let dy = p[1] - ap[1];
                    let dz = p[2] - ap[2];
                    (dx * dx + dy * dy + dz * dz).sqrt() <= cad_primitives::TAU_MODEL
                })
                .map(|i| i as u32)
                .ok_or_else(|| {
                    YangError::MalformedTopology(format!(
                        "face {f_idx}: cone apex {ap:?} matches no pre-seeded B-Rep vertex"
                    ))
                })?;
            // Apex fan: triangle (apex, ring[k], ring[(k+1) % N]); orient each
            // outward via the tilted cone normal.
            for k in 0..nseg {
                let mut tri = [apex_vi, ring[k], ring[(k + 1) % nseg]];
                let n = cone_outward_normal(out_verts, &tri, apex, axis_dir, half_angle);
                orient_tri(out_verts, &mut tri, n);
                out_tris.push(tri);
            }
            Ok(())
        }
        [rim_a, rim_b] => tessellate_cone_frustum_band(
            f_idx, f, edges, *rim_a, *rim_b, rim_rings, apex, axis_dir, half_angle, out_verts,
            out_tris,
        ),
        other => Err(YangError::MalformedTopology(format!(
            "face {f_idx}: cone lateral must be bounded by ONE base rim (apex cone) or TWO \
             rims (frustum band); found {} circle edges (a cone on a non-circular boundary is \
             malformed topology)",
            other.len()
        ))),
    }
}

/// KV6c increment 5b: tessellate a FRUSTUM-band cone face — two full-circle
/// rims at different radii — as a ruled quad strip, the tilted-normal analog of
/// the cylinder canonical tube ([`tessellate_lateral_face`]). The rings pair by
/// azimuth exactly as the tube does (counter-rotating stored senses ⇒ `N − k`);
/// each triangle is oriented by [`cone_outward_normal`], negated for a cavity
/// bore (`f.reversed`).
#[allow(clippy::too_many_arguments)]
fn tessellate_cone_frustum_band(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    rim_a: u32,
    rim_b: u32,
    rim_rings: &std::collections::BTreeMap<u32, Vec<u32>>,
    apex: Point3,
    axis_dir: Vector3,
    half_angle: f64,
    out_verts: &[Point3],
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    let au = normalize3(axis_dir.as_array());
    let ap = apex.as_array();
    // Axial coordinate (from the apex) and stored-normal sense of a rim.
    let rim_param = |e: u32| -> f64 {
        if let Curve::Circle { center, .. } = edges[e as usize].curve {
            let c = center.as_array();
            (c[0] - ap[0]) * au[0] + (c[1] - ap[1]) * au[1] + (c[2] - ap[2]) * au[2]
        } else {
            0.0
        }
    };
    let rim_sense = |e: u32| -> f64 {
        if let Curve::Circle { normal, .. } = edges[e as usize].curve {
            let n = normalize3(normal.as_array());
            (n[0] * au[0] + n[1] * au[1] + n[2] * au[2]).signum()
        } else {
            1.0
        }
    };
    let (mut bottom_e, mut top_e) = (rim_a, rim_b);
    if rim_param(bottom_e) > rim_param(top_e) {
        std::mem::swap(&mut bottom_e, &mut top_e);
    }
    let bottom_ring = rim_rings.get(&bottom_e).ok_or_else(|| {
        YangError::MalformedTopology(format!(
            "face {f_idx}: bottom rim ring {bottom_e} not built"
        ))
    })?;
    let top_ring = rim_rings.get(&top_e).ok_or_else(|| {
        YangError::MalformedTopology(format!("face {f_idx}: top rim ring {top_e} not built"))
    })?;
    let nseg = top_ring.len();
    if nseg < 3 || bottom_ring.len() != nseg {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: cone frustum rims have mismatched / too-few samples ({} vs {})",
            bottom_ring.len(),
            top_ring.len()
        )));
    }
    let co_rotating = rim_sense(bottom_e) * rim_sense(top_e) > 0.0;
    let b_index = |k: usize| -> usize {
        if co_rotating {
            k
        } else {
            (nseg - k) % nseg
        }
    };
    for k in 0..nseg {
        let kn = (k + 1) % nseg;
        let t0 = top_ring[k];
        let t1 = top_ring[kn];
        let b0 = bottom_ring[b_index(k)];
        let b1 = bottom_ring[b_index(kn)];
        for mut tri in [[b0, b1, t1], [b0, t1, t0]] {
            let mut n = cone_outward_normal(out_verts, &tri, apex, axis_dir, half_angle);
            if f.reversed {
                n = [-n[0], -n[1], -n[2]];
            }
            orient_tri(out_verts, &mut tri, n);
            out_tris.push(tri);
        }
    }
    Ok(())
}

/// KV6d 4b: tessellate a partial-torus `Surface::Torus` face — a bent tube —
/// as a watertight (θ × φ) bijective grid. Rows are meridians (constant sweep
/// angle θ), columns are longitudes (constant profile angle φ). The two θ-end
/// meridians REUSE the profile-circle rim rings (so they match the disk caps
/// bit-for-bit → watertight); the φ=0 column REUSES the seam-arc chain; the
/// interior points are fresh `BRepFace { u=φ, v=θ }` (so `eval_source`
/// round-trips via the torus `face_eval` arm). Rings are aligned by intrinsic
/// φ (`atan2(τ, ρ−R)`), so a counter-rotating end meridian still lines up
/// column-for-column.
#[allow(clippy::too_many_arguments)]
fn tessellate_torus_face(
    f_idx: usize,
    f: &BRepFace,
    edges: &[BRepEdge],
    rim_rings: &std::collections::BTreeMap<u32, Vec<u32>>,
    center: Point3,
    axis_dir: Vector3,
    major: f64,
    minor: f64,
    out_verts: &mut Vec<Point3>,
    sources: &mut Vec<TessellationSource>,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    use std::collections::BTreeSet;
    use std::f64::consts::PI;
    let malformed = |m: String| YangError::MalformedTopology(m);
    let cen = center.as_array();
    let ax = normalize3(axis_dir.as_array());
    let (e1v, e2v) = ortho_basis(axis_dir);
    let (e1, e2) = (e1v.as_array(), e2v.as_array());
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let band = 1e-9 * (1.0 + major + minor);

    // Classify the boundary edges: 2 profile circles (closed, radius ≈ minor)
    // + 1 seam arc (open, radius ≈ major+minor).
    let mut profiles: Vec<u32> = Vec::new();
    let mut seam: Option<u32> = None;
    let mut seen = BTreeSet::new();
    for &e in f.outer_loop.iter() {
        if !seen.insert(e) {
            continue;
        }
        if let Curve::Circle { radius, .. } = edges[e as usize].curve {
            let ed = &edges[e as usize];
            if ed.start == ed.end && (radius - minor).abs() <= band {
                profiles.push(e);
            } else if ed.start != ed.end && (radius - (major + minor)).abs() <= band {
                seam = Some(e);
            }
        }
    }
    let (Some(seam_e), 2) = (seam, profiles.len()) else {
        return Err(malformed(format!(
            "face {f_idx}: torus face needs 2 profile circles + 1 seam arc (got {} circles)",
            profiles.len()
        )));
    };
    let seam_chain = rim_rings
        .get(&seam_e)
        .ok_or_else(|| malformed(format!("face {f_idx}: seam chain {seam_e} not built")))?;
    let n_theta = seam_chain.len() - 1;
    if n_theta < 1 {
        return Err(malformed(format!(
            "face {f_idx}: torus seam chain too short"
        )));
    }
    let (seam_start, seam_end) = (edges[seam_e as usize].start, edges[seam_e as usize].end);
    let prof0_e = *profiles
        .iter()
        .find(|&&p| edges[p as usize].start == seam_start)
        .ok_or_else(|| malformed(format!("face {f_idx}: no θ=0 profile at the seam start")))?;
    let profa_e = *profiles
        .iter()
        .find(|&&p| edges[p as usize].start == seam_end)
        .ok_or_else(|| malformed(format!("face {f_idx}: no θ=α profile at the seam end")))?;
    let ring0 = rim_rings
        .get(&prof0_e)
        .ok_or_else(|| malformed(format!("face {f_idx}: θ=0 profile ring not built")))?;
    let ringa = rim_rings
        .get(&profa_e)
        .ok_or_else(|| malformed(format!("face {f_idx}: θ=α profile ring not built")))?;
    let n_phi = ring0.len();
    if ringa.len() != n_phi || n_phi < 3 {
        return Err(malformed(format!(
            "face {f_idx}: torus profile rings mismatched / too few ({n_phi} vs {})",
            ringa.len()
        )));
    }

    // Intrinsic profile angle φ of a mesh vertex → its φ grid slot.
    let phi_slot = |out_verts: &[Point3], vi: u32| -> usize {
        let p = out_verts[vi as usize].as_array();
        let d = [p[0] - cen[0], p[1] - cen[1], p[2] - cen[2]];
        let tau = dot(d, ax);
        let radial = [d[0] - tau * ax[0], d[1] - tau * ax[1], d[2] - tau * ax[2]];
        let rho = dot(radial, radial).sqrt();
        let phi = tau.atan2(rho - major).rem_euclid(2.0 * PI);
        ((phi / (2.0 * PI / n_phi as f64)).round() as usize) % n_phi
    };
    let mut row0 = vec![u32::MAX; n_phi];
    for &v in ring0 {
        row0[phi_slot(out_verts, v)] = v;
    }
    let mut rowa = vec![u32::MAX; n_phi];
    for &v in ringa {
        rowa[phi_slot(out_verts, v)] = v;
    }
    if row0.contains(&u32::MAX) || rowa.contains(&u32::MAX) {
        return Err(malformed(format!(
            "face {f_idx}: torus profile ring φ sampling is not slot-aligned"
        )));
    }

    // Build the full (n_theta+1) × n_phi grid of vertex indices.
    let mut grid: Vec<Vec<u32>> = Vec::with_capacity(n_theta + 1);
    #[allow(clippy::needless_range_loop)]
    for i in 0..=n_theta {
        if i == 0 {
            grid.push(row0.clone());
        } else if i == n_theta {
            grid.push(rowa.clone());
        } else {
            let s = seam_chain[i];
            let p = out_verts[s as usize].as_array();
            let d = [p[0] - cen[0], p[1] - cen[1], p[2] - cen[2]];
            let theta = dot(d, e2).atan2(dot(d, e1));
            let (st, ct) = theta.sin_cos();
            let mut row = vec![0u32; n_phi];
            row[0] = s; // φ=0 column reuses the seam chain
            for (j, slot) in row.iter_mut().enumerate().skip(1) {
                let phi = 2.0 * PI * (j as f64) / (n_phi as f64);
                let rad = major + minor * phi.cos();
                let sp = minor * phi.sin();
                let pt = [
                    cen[0] + rad * (ct * e1[0] + st * e2[0]) + sp * ax[0],
                    cen[1] + rad * (ct * e1[1] + st * e2[1]) + sp * ax[1],
                    cen[2] + rad * (ct * e1[2] + st * e2[2]) + sp * ax[2],
                ];
                let vi = out_verts.len() as u32;
                out_verts.push(Point3::new(pt[0], pt[1], pt[2]));
                sources.push(TessellationSource::BRepFace {
                    face: f_idx as u32,
                    u: phi,
                    v: theta,
                });
                *slot = vi;
            }
            grid.push(row);
        }
    }

    // Emit quads, each triangle wound to agree with the torus outward normal
    // (direction from the nearest tube-center-circle point).
    let emit = |a: u32, b: u32, c: u32, out_verts: &[Point3], out_tris: &mut Vec<[u32; 3]>| {
        let pa = out_verts[a as usize].as_array();
        let pb = out_verts[b as usize].as_array();
        let pc = out_verts[c as usize].as_array();
        let e_a = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let e_b = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let gn = [
            e_a[1] * e_b[2] - e_a[2] * e_b[1],
            e_a[2] * e_b[0] - e_a[0] * e_b[2],
            e_a[0] * e_b[1] - e_a[1] * e_b[0],
        ];
        let ctr = [
            (pa[0] + pb[0] + pc[0]) / 3.0,
            (pa[1] + pb[1] + pc[1]) / 3.0,
            (pa[2] + pb[2] + pc[2]) / 3.0,
        ];
        let d = [ctr[0] - cen[0], ctr[1] - cen[1], ctr[2] - cen[2]];
        let tau = dot(d, ax);
        let rv = [d[0] - tau * ax[0], d[1] - tau * ax[1], d[2] - tau * ax[2]];
        let rl = dot(rv, rv).sqrt().max(1e-300);
        let rhat = [rv[0] / rl, rv[1] / rl, rv[2] / rl];
        let on = [
            ctr[0] - (cen[0] + major * rhat[0]),
            ctr[1] - (cen[1] + major * rhat[1]),
            ctr[2] - (cen[2] + major * rhat[2]),
        ];
        if dot(gn, on) >= 0.0 {
            out_tris.push([a, b, c]);
        } else {
            out_tris.push([a, c, b]);
        }
    };
    for i in 0..n_theta {
        for j in 0..n_phi {
            let jn = (j + 1) % n_phi;
            let (a, b, c, d) = (grid[i][j], grid[i][jn], grid[i + 1][jn], grid[i + 1][j]);
            emit(a, b, c, out_verts, out_tris);
            emit(a, c, d, out_verts, out_tris);
        }
    }
    Ok(())
}

/// Unwrap a periodic angle sequence in place: each value is shifted by a
/// multiple of 2π so successive entries never jump by more than π (standard
/// phase unwrapping). Turns a torus patch's `atan2` parameters into a simple
/// (non-self-crossing) polygon as long as the patch does not wrap the whole way
/// around a seam.
fn unwrap_seq(a: &mut [f64]) {
    use std::f64::consts::{PI, TAU};
    for k in 1..a.len() {
        while a[k] - a[k - 1] > PI {
            a[k] -= TAU;
        }
        while a[k - 1] - a[k] > PI {
            a[k] += TAU;
        }
    }
}

/// KV6d UV-CDT consumer: tessellate an arbitrary torus PATCH from its ordered
/// closed 3D boundary loop `boundary` (the mesh-boolean intersection curve +
/// seam, finely sampled) via the interior-Steiner CDT
/// ([`cherchi_rs::cdt_polygon_with_holes_refined`]).
///
/// Pipeline: invert each boundary point into the `(u = meridian, v = longitude)`
/// parameter plane — `face_eval(u,v) = center + (R + r·cos u)·(cos v·ê1 +
/// sin v·ê2) + r·sin u·â`, so `v = atan2(w·ê2, w·ê1)` and `u = atan2(τ, ρ − R)`
/// (τ axial, ρ radial-from-axis) — angle-UNWRAP `u` and `v` to a simple polygon,
/// SCALE to ~arc-length isotropy (`u·r`, `v·R`) so a Delaunay-quality triangle
/// maps to a well-shaped 3D one, REFINE to `max_3d_area`, then MAP BACK: BOUNDARY
/// verts keep their EXACT input 3D coordinates (conformal with the neighbouring
/// patch — the boolean's shared intersection-curve samples), interior STEINER
/// verts are `face_eval` of their unscaled `(u,v)` (exactly on the torus).
///
/// Returns `(verts, tris)` with `verts[0..boundary.len()] == boundary` bit-for-
/// bit. `None` if the boundary degenerates or the CDT rejects a self-intersecting
/// projection (a seam-CROSSING / self-overlapping patch — out of this v1 scope;
/// the deferral is loud at the caller).
///
/// Consumed by kernel-v2's render-time torus-patch tessellation (the owner of
/// output recovery re-tessellation); exposed `pub` for that cross-crate call.
/// One boundary loop of a torus patch projected into the continuous, scaled
/// meridian/longitude plane (`su = u·minor`, `sv = v·major`), with its net
/// MERIDIAN winding `wu` (in 2π units). `wu != 0` ⇒ the loop wraps the meridian
/// seam (a band edge); the longitude is required non-wrapping (partial torus).
struct PLoop {
    su: Vec<f64>,
    sv: Vec<f64>,
    pts: Vec<Point3>,
    wu: i64,
}

/// KV6d seam-wrapping render — the cylinder patch's pass-2 case-2 ported to the
/// torus meridian. Unrolls a BAND bounded by two oppositely-meridian-wrapping
/// loops `pc` (wrap +1) and `mc` (wrap −1) into a single simple ring in the
/// universal cover: walk `pc` over one full period (`su` increasing by `span`),
/// bridge to `mc`, walk `mc` reversed (`su` decreasing), bridge back. The seam
/// bridges connect an anchor pair chosen so neither bridge crosses a boundary
/// edge; the duplicated anchor vertices coincide in 3D (the meridian is
/// periodic) so the result is watertight. Returns the ring as `(su, sv, 3d)` per
/// vertex, or `None` if no unblocked bridge exists.
fn band_seam_bridge<F: Fn(f64, f64) -> Point3>(
    pc: &PLoop,
    mc: &PLoop,
    span: f64,
    minor: f64,
    major: f64,
    eval: F,
) -> Option<Vec<(f64, f64, Point3)>> {
    let (m, mm) = (pc.su.len(), mc.su.len());
    if m < 3 || mm < 3 {
        return None;
    }
    let principal = |d: f64| d - (d / span).round() * span;
    // Proper (open) segment crossing in the (su, sv) plane.
    let seg_cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]| -> bool {
        let o = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
            (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
        };
        o(a, c, d) * o(b, c, d) < 0.0 && o(a, b, c) * o(a, b, d) < 0.0
    };
    // Each seam bridge spans the full longitude gap (v0 → v1) as a SINGLE
    // segment; left unsplit, the CDT cannot place Steiner near it and the edge
    // region stays coarse → large chord error on the curved tube. Subdivide it
    // into `k` segments matching the meridian sampling density, with each
    // intermediate point ON the torus at the seam meridian (so the left- and
    // right-seam copies, one period apart in u, coincide in 3D → watertight).
    let mean_step = span / m as f64;
    // Interpolate a bridge from `(su_a, sv_a)` to `(su_b, sv_b)` at the seam
    // meridian `u_seam`, returning the K−1 interior points (2D + on-torus 3D).
    let bridge_pts =
        |su_a: f64, sv_a: f64, su_b: f64, sv_b: f64, u_seam: f64| -> Vec<(f64, f64, Point3)> {
            let dsv = (sv_b - sv_a).abs();
            let k = (dsv / mean_step).ceil().max(1.0) as usize;
            let mut out = Vec::with_capacity(k.saturating_sub(1));
            for i in 1..k {
                let t = i as f64 / k as f64;
                let su = su_a + (su_b - su_a) * t;
                let sv = sv_a + (sv_b - sv_a) * t;
                out.push((su, sv, eval(u_seam, sv / major)));
            }
            out
        };
    let build_ring = |xi: usize, yi: usize, dpr: f64| -> Vec<(f64, f64, Point3)> {
        let mut ring: Vec<(f64, f64, Point3)> = Vec::with_capacity(m + mm + 2);
        let base_x = pc.su[xi];
        for j in 0..=m {
            let idx = (xi + j) % m;
            let mut u = pc.su[idx];
            if j > 0 && (xi + j) >= m {
                u += span;
            }
            if j == m {
                u = base_x + span;
            }
            ring.push((u, pc.sv[idx], pc.pts[idx]));
        }
        let y_target = base_x + span + dpr;
        let y_base = mc.su[yi];
        let wsign = mc.wu as f64; // −1
                                  // Bridge 1: pc closing point (base_x+span, v0) → mc start (y_target, v1),
                                  // at the right-seam meridian (base_x+span)/minor.
        ring.extend(bridge_pts(
            base_x + span,
            pc.sv[xi],
            y_target,
            mc.sv[yi],
            (base_x + span) / minor,
        ));
        for j in 0..=mm {
            let idx = (yi + j) % mm;
            let mut u = mc.su[idx];
            if j > 0 && (yi + j) >= mm {
                u += wsign * span;
            }
            if j == mm {
                u = y_base + wsign * span;
            }
            ring.push((u - y_base + y_target, mc.sv[idx], mc.pts[idx]));
        }
        // Bridge 2: mc closing point (≈ base_x+dpr, v1) → pc start (base_x, v0),
        // at the left-seam meridian base_x/minor (= right meridian − 2π in u, so
        // `eval` returns the SAME 3D points as bridge 1 → watertight seam).
        ring.extend(bridge_pts(
            base_x + dpr,
            mc.sv[yi],
            base_x,
            pc.sv[xi],
            base_x / minor,
        ));
        ring
    };
    // Pick the anchor pair (xi on pc, yi on mc) whose two seam-bridge SPANS
    // (validated as single segments, before subdivision) cross no boundary edge.
    for xi in 0..m {
        let xu = pc.su[xi];
        let mut best: Option<(usize, f64)> = None;
        for (yi, &syu) in mc.su.iter().enumerate() {
            let dpr = principal(syu - xu);
            if best.is_none_or(|(_, b)| dpr.abs() < b.abs()) {
                best = Some((yi, dpr));
            }
        }
        let (yi, dpr) = best?;
        let base_x = pc.su[xi];
        // The two seam spans (right: pc→mc; left: mc→pc), in the unrolled frame.
        let spans = [
            ([base_x + span, pc.sv[xi]], [base_x + span + dpr, mc.sv[yi]]),
            ([base_x + dpr, mc.sv[yi]], [base_x, pc.sv[xi]]),
        ];
        // Boundary edges to test against: the pc bottom chain (at sv=pc.sv) and
        // the mc top chain (at sv=mc.sv), in the bridge's local frame.
        let bottom: Vec<[f64; 2]> = (0..=m)
            .map(|j| {
                let idx = (xi + j) % m;
                let mut u = pc.su[idx];
                if (j > 0 && (xi + j) >= m) || j == m {
                    u = if j == m { base_x + span } else { u + span };
                }
                [u, pc.sv[idx]]
            })
            .collect();
        let y_target = base_x + span + dpr;
        let y_base = mc.su[yi];
        let wsign = mc.wu as f64;
        let top: Vec<[f64; 2]> = (0..=mm)
            .map(|j| {
                let idx = (yi + j) % mm;
                let mut u = mc.su[idx];
                if (j > 0 && (yi + j) >= mm) || j == mm {
                    u = if j == mm {
                        y_base + wsign * span
                    } else {
                        u + wsign * span
                    };
                }
                [u - y_base + y_target, mc.sv[idx]]
            })
            .collect();
        let crosses = |p: [f64; 2], q: [f64; 2]| -> bool {
            if p == q {
                return true;
            }
            let chain_hit =
                |chain: &[[f64; 2]]| chain.windows(2).any(|w| seg_cross(p, q, w[0], w[1]));
            chain_hit(&bottom) || chain_hit(&top)
        };
        if !spans.iter().any(|&(p, q)| crosses(p, q)) {
            return Some(build_ring(xi, yi, dpr));
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
pub fn tessellate_torus_patch(
    center: Point3,
    axis_dir: Vector3,
    major: f64,
    minor: f64,
    boundary: &[Point3],
    holes: &[Vec<Point3>],
    max_3d_area: f64,
) -> Option<(Vec<Point3>, Vec<[u32; 3]>)> {
    if boundary.len() < 3 || major <= 0.0 || minor <= 0.0 {
        return None;
    }
    let ax = normalize3(axis_dir.as_array());
    let (e1, e2) = ortho_basis(axis_dir);
    let (e1a, e2a) = (e1.as_array(), e2.as_array());
    let c = center.as_array();
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let eval = |u: f64, v: f64| -> Point3 {
        let (cu, su) = (u.cos(), u.sin());
        let (cv, sv) = (v.cos(), v.sin());
        let rad = major + minor * cu;
        Point3::new(
            c[0] + rad * (cv * e1a[0] + sv * e2a[0]) + minor * su * ax[0],
            c[1] + rad * (cv * e1a[1] + sv * e2a[1]) + minor * su * ax[1],
            c[2] + rad * (cv * e1a[2] + sv * e2a[2]) + minor * su * ax[2],
        )
    };
    let snap = |a: f64| (a / 1e-12).round() * 1e-12;
    // Invert ONE loop into (u, v): project, unwrap each angle to a simple
    // polygon, and condition to the TAU_WORK (1e-12 rad) grid. The atan2/cos/sin
    // inversion introduces ~1 ULP noise; on a straight seam run (constant u or
    // v) that faint kink makes spade over-refine into a sliver storm — snapping
    // removes it without perturbing output geometry (boundary verts are emitted
    // from the EXACT input 3D below; Steiner verts are `face_eval`, on-torus).
    let invert = |loop_pts: &[Point3]| -> (Vec<f64>, Vec<f64>) {
        let mut us = Vec::with_capacity(loop_pts.len());
        let mut vs = Vec::with_capacity(loop_pts.len());
        for &p in loop_pts {
            let pa = p.as_array();
            let w = [pa[0] - c[0], pa[1] - c[1], pa[2] - c[2]];
            let tau = dot(w, ax);
            let radial = [w[0] - tau * ax[0], w[1] - tau * ax[1], w[2] - tau * ax[2]];
            let wx = dot(radial, e1a);
            let wy = dot(radial, e2a);
            let rho = (wx * wx + wy * wy).sqrt();
            vs.push(wy.atan2(wx));
            us.push(tau.atan2(rho - major));
        }
        unwrap_seq(&mut us);
        unwrap_seq(&mut vs);
        for a in us.iter_mut().chain(vs.iter_mut()) {
            *a = snap(*a);
        }
        (us, vs)
    };

    // Net winding of a closed loop's already-unwrapped angle sequence: the span
    // plus the closure step, in units of 2π. Nonzero ⇒ the loop wraps that
    // periodic coordinate (a meridian- or longitude-wrapping patch).
    let net_wrap = |a: &[f64]| -> i64 {
        if a.len() < 2 {
            return 0;
        }
        let raw_closure = a[0] - a[a.len() - 1];
        // wrap_to_pi(raw_closure)
        let mut c = raw_closure % std::f64::consts::TAU;
        if c > std::f64::consts::PI {
            c -= std::f64::consts::TAU;
        } else if c <= -std::f64::consts::PI {
            c += std::f64::consts::TAU;
        }
        let net = (a[a.len() - 1] - a[0]) + c;
        (net / std::f64::consts::TAU).round() as i64
    };

    use std::f64::consts::TAU;
    // ---- Pass 1: project every loop into the continuous, scaled (su, sv)
    // meridian/longitude plane with its net meridian winding `wu`. ----------
    let project = |pts: &[Point3]| -> Option<PLoop> {
        if pts.len() < 3 {
            return None;
        }
        let (us, vs) = invert(pts);
        if net_wrap(&vs) != 0 {
            return None; // a LONGITUDE wrap is a full-torus seam — out of scope
        }
        let wu = net_wrap(&us);
        Some(PLoop {
            su: us.iter().map(|u| u * minor).collect(),
            sv: vs.iter().map(|v| v * major).collect(),
            pts: pts.to_vec(),
            wu,
        })
    };
    let mut ploops: Vec<PLoop> = Vec::with_capacity(1 + holes.len());
    ploops.push(project(boundary)?);
    for h in holes {
        ploops.push(project(h)?);
    }
    let span = TAU * minor; // one full meridian wrap in scaled u
    let span_v = TAU * major;

    // ---- Pass 2: assemble one simple (su, sv) outer polygon + holes. -------
    let wrapping: Vec<usize> = (0..ploops.len()).filter(|&i| ploops[i].wu != 0).collect();
    let mut verts2d: Vec<cad_primitives::Point2> = Vec::new();
    let mut vert3d: Vec<Point3> = Vec::new();
    let mut hole_idx: Vec<Vec<u32>> = Vec::new();
    let umean = |l: &PLoop| l.su.iter().sum::<f64>() / l.su.len() as f64;
    let vmean = |l: &PLoop| l.sv.iter().sum::<f64>() / l.sv.len() as f64;
    let outer: Vec<u32> = match wrapping.len() {
        0 => {
            // DISK: boundary is the outer loop; holes are the rest, each shifted
            // by whole periods so it sits in the outer's period (a hole atan2
            // placed on the opposite branch would project outside the outer).
            let (u_ref, v_ref) = (umean(&ploops[0]), vmean(&ploops[0]));
            let mut o = Vec::with_capacity(ploops[0].su.len());
            for k in 0..ploops[0].su.len() {
                o.push(verts2d.len() as u32);
                verts2d.push(cad_primitives::Point2::new(
                    ploops[0].su[k],
                    ploops[0].sv[k],
                ));
                vert3d.push(ploops[0].pts[k]);
            }
            for l in &ploops[1..] {
                let du = ((umean(l) - u_ref) / span).round() * span;
                let dv = ((vmean(l) - v_ref) / span_v).round() * span_v;
                let mut hi = Vec::with_capacity(l.su.len());
                for k in 0..l.su.len() {
                    hi.push(verts2d.len() as u32);
                    verts2d.push(cad_primitives::Point2::new(l.su[k] - du, l.sv[k] - dv));
                    vert3d.push(l.pts[k]);
                }
                hole_idx.push(hi);
            }
            o
        }
        2 if ploops.len() == 2 => {
            // BAND: two oppositely-meridian-wrapping loops, no holes (a holed
            // band is out of this v1 scope). Universal-cover seam bridge.
            let (a, b) = (wrapping[0], wrapping[1]);
            let (pc, mc) = if ploops[a].wu > 0 {
                (&ploops[a], &ploops[b])
            } else {
                (&ploops[b], &ploops[a])
            };
            if pc.wu + mc.wu != 0 {
                return None;
            }
            let ring = band_seam_bridge(pc, mc, span, minor, major, eval)?;
            let mut o = Vec::with_capacity(ring.len());
            for (su, sv, p) in &ring {
                o.push(verts2d.len() as u32);
                verts2d.push(cad_primitives::Point2::new(*su, *sv));
                vert3d.push(*p);
            }
            o
        }
        // 1 wrapping loop (degenerate), a holed band, or > 2 wraps: out of scope.
        _ => return None,
    };

    let (ref_verts, tris) =
        cherchi_rs::cdt_polygon_with_holes_refined(&verts2d, &outer, &hole_idx, max_3d_area)
            .ok()?;

    // Map back: boundary verts → EXACT input 3D; refined Steiner verts (appended
    // after) → `face_eval`. A band's duplicated seam vertices carry the same 3D
    // point on both sides (the meridian is periodic) ⇒ watertight.
    let n = vert3d.len();
    let mut verts3d: Vec<Point3> = Vec::with_capacity(ref_verts.len());
    for (i, sp) in ref_verts.iter().enumerate() {
        if i < n {
            verts3d.push(vert3d[i]);
        } else {
            verts3d.push(eval(sp.x() / minor, sp.y() / major));
        }
    }
    Some((verts3d, tris))
}

#[cfg(test)]
mod torus_patch_tests {
    use super::*;
    use std::collections::BTreeMap;

    #[allow(clippy::too_many_arguments)]
    fn eval(
        center: Point3,
        ax: [f64; 3],
        e1a: [f64; 3],
        e2a: [f64; 3],
        major: f64,
        minor: f64,
        u: f64,
        v: f64,
    ) -> Point3 {
        let c = center.as_array();
        let (cu, su) = (u.cos(), u.sin());
        let (cv, sv) = (v.cos(), v.sin());
        let rad = major + minor * cu;
        Point3::new(
            c[0] + rad * (cv * e1a[0] + sv * e2a[0]) + minor * su * ax[0],
            c[1] + rad * (cv * e1a[1] + sv * e2a[1]) + minor * su * ax[1],
            c[2] + rad * (cv * e1a[2] + sv * e2a[2]) + minor * su * ax[2],
        )
    }

    #[test]
    fn torus_patch_roundtrip_on_surface_watertight() {
        // Torus: center origin, axis +Z, R=3, r=1.
        let center = Point3::new(0.0, 0.0, 0.0);
        let axis = Vector3::new(0.0, 0.0, 1.0);
        let (major, minor) = (3.0_f64, 1.0_f64);
        let ax = normalize3(axis.as_array());
        let (e1, e2) = ortho_basis(axis);
        let (e1a, e2a) = (e1.as_array(), e2.as_array());

        // A sub-(u,v)-rectangle patch boundary, finely sampled along its 4 edges.
        let (u0, u1, v0, v1) = (0.2_f64, 1.2_f64, 0.5_f64, 1.8_f64);
        let ns = 8;
        let mut boundary: Vec<Point3> = Vec::new();
        let mut push =
            |u: f64, v: f64| boundary.push(eval(center, ax, e1a, e2a, major, minor, u, v));
        for k in 0..ns {
            let t = k as f64 / ns as f64;
            push(u0 + (u1 - u0) * t, v0);
        }
        for k in 0..ns {
            let t = k as f64 / ns as f64;
            push(u1, v0 + (v1 - v0) * t);
        }
        for k in 0..ns {
            let t = k as f64 / ns as f64;
            push(u1 - (u1 - u0) * t, v1);
        }
        for k in 0..ns {
            let t = k as f64 / ns as f64;
            push(u0, v1 - (v1 - v0) * t);
        }

        let n = boundary.len();
        let (verts, tris) =
            tessellate_torus_patch(center, axis, major, minor, &boundary, &[], 0.05)
                .expect("patch tessellation");

        // Interior Steiner points were added (refinement actually fired).
        assert!(verts.len() > n, "no Steiner points: {} verts", verts.len());

        // Boundary verts preserved bit-for-bit (conformal).
        for i in 0..n {
            assert_eq!(verts[i], boundary[i], "boundary vert {i} moved");
        }

        // Every vert lies on the torus surface.
        let surf = Surface::Torus {
            center,
            axis_dir: axis,
            major_radius: major,
            minor_radius: minor,
        };
        for (i, &p) in verts.iter().enumerate() {
            let d = signed_distance_to_surface(surf, p).expect("torus distance");
            assert!(d.abs() < 1e-9, "vert {i} off torus: d={d}");
        }

        // Manifold/watertight: every edge in 1 (boundary) or 2 (interior) tris.
        let mut edges: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for t in &tris {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        assert!(
            edges.values().all(|&c| c == 1 || c == 2),
            "non-manifold edge present"
        );

        // The closed boundary loop is exactly the count-1 edges (no slits).
        let boundary_edges = edges.values().filter(|&&c| c == 1).count();
        assert_eq!(
            boundary_edges, n,
            "boundary edge count {boundary_edges} != {n}"
        );

        // The chorded 3D area matches the analytic patch area (a faithful, hole-
        // free, non-folded tessellation). Analytic area of the (u,v) rectangle:
        //   ∫∫ (R + r·cos u)·r du dv = r·(v1−v0)·[R·(u1−u0) + r·(sin u1 − sin u0)].
        let analytic = minor * (v1 - v0) * (major * (u1 - u0) + minor * (u1.sin() - u0.sin()));
        let mut area3d = 0.0;
        for t in &tris {
            let a = verts[t[0] as usize].as_array();
            let b = verts[t[1] as usize].as_array();
            let c = verts[t[2] as usize].as_array();
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cr = [
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            ];
            area3d += 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
        }
        // Inscribed chords slightly under-shoot the smooth area; refinement keeps
        // it within ~1%. It must never exceed the smooth area (that would signal
        // folded/overlapping triangles).
        assert!(
            area3d <= analytic * (1.0 + 1e-6) && area3d >= analytic * 0.985,
            "area3d {area3d} vs analytic {analytic} (folds or holes?)"
        );
    }

    #[test]
    fn torus_patch_rejects_degenerate() {
        let center = Point3::new(0.0, 0.0, 0.0);
        let axis = Vector3::new(0.0, 0.0, 1.0);
        let too_few = [Point3::new(4.0, 0.0, 0.0), Point3::new(3.0, 0.0, 1.0)];
        assert!(tessellate_torus_patch(center, axis, 3.0, 1.0, &too_few, &[], 0.05).is_none());
    }

    /// Seam-wrapping (cylindrical) BAND render: a longitude slice v ∈ [v0, v1] of
    /// the tube wraps the full meridian (u ∈ [0, 2π)). Bounded by two meridian
    /// circles (opposite winding), it is triangulated via the universal-cover
    /// seam bridge into a watertight, on-tube mesh.
    #[test]
    fn torus_band_seam_wrapping_render() {
        let center = Point3::new(0.0, 0.0, 0.0);
        let axis = Vector3::new(0.0, 0.0, 1.0);
        let (major, minor) = (3.0_f64, 1.0_f64);
        let ax = normalize3(axis.as_array());
        let (e1, e2) = ortho_basis(axis);
        let (e1a, e2a) = (e1.as_array(), e2.as_array());
        // A FULL-quarter longitude slice (Δv = π/2): a large band whose seam
        // bridges must be subdivided, or the edge regions stay coarse and the
        // chorded area undershoots (the KV6d band-render regression).
        let (v0, v1) = (0.0_f64, std::f64::consts::FRAC_PI_2);
        let nu = 24;
        // Two meridian circles: v0 wound +u (wrap +1), v1 wound −u (wrap −1).
        let mut c0: Vec<Point3> = Vec::new();
        let mut c1: Vec<Point3> = Vec::new();
        for k in 0..nu {
            let u = std::f64::consts::TAU * (k as f64) / (nu as f64);
            c0.push(eval(center, ax, e1a, e2a, major, minor, u, v0));
            c1.push(eval(center, ax, e1a, e2a, major, minor, -u, v1));
        }
        let (verts, tris) = tessellate_torus_patch(center, axis, major, minor, &c0, &[c1], 0.05)
            .expect("band tessellation");
        assert!(!tris.is_empty(), "non-empty band mesh");

        // Every vertex on the tube.
        let surf = torus(major, minor);
        for (i, &p) in verts.iter().enumerate() {
            let d = signed_distance_to_surface(surf, p).unwrap();
            assert!(d.abs() < 1e-9, "band vert {i} off tube: {d:e}");
        }
        // Manifold + watertight across the SEAM: group edges by 3D POSITION (the
        // periodic seam's duplicated vertices coincide in 3D). Every edge is
        // shared by 2 tris (interior + the seam, where the universal-cover bridge
        // duplicates coincide) EXCEPT the band's two real meridian-circle
        // boundaries at v0 / v1 (shared by 1). No edge is shared by >2.
        let key = |p: Point3| {
            let a = p.as_array();
            [
                (a[0] * 1e7).round() as i64,
                (a[1] * 1e7).round() as i64,
                (a[2] * 1e7).round() as i64,
            ]
        };
        let mut edges: BTreeMap<([i64; 3], [i64; 3]), u32> = BTreeMap::new();
        for t in &tris {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let (ka, kb) = (key(verts[a as usize]), key(verts[b as usize]));
                let e = if ka < kb { (ka, kb) } else { (kb, ka) };
                *edges.entry(e).or_insert(0) += 1;
            }
        }
        assert!(
            edges.values().all(|&c| c == 1 || c == 2),
            "non-manifold edge (some positional edge in >2 tris) — seam not watertight"
        );
        // Exactly the two meridian-circle boundaries (nu edges each) are count-1;
        // the seam bridges coincide in 3D and are interior (count-2).
        let boundary_edges = edges.values().filter(|&&c| c == 1).count();
        assert_eq!(
            boundary_edges,
            2 * nu,
            "expected the two v-circle boundaries ({} edges), got {boundary_edges}",
            2 * nu
        );

        // The chorded area approaches the analytic band area
        // ∫∫ (R + r cos u)·r du dv = r·(v1−v0)·2π·R  (∫cos u over a full turn = 0).
        let analytic = minor * (v1 - v0) * std::f64::consts::TAU * major;
        let mut area = 0.0;
        for t in &tris {
            let a = verts[t[0] as usize].as_array();
            let b = verts[t[1] as usize].as_array();
            let c = verts[t[2] as usize].as_array();
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cr = [
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            ];
            area += 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
        }
        assert!(
            area <= analytic * (1.0 + 1e-6) && area >= analytic * 0.97,
            "band area {area} vs analytic {analytic}"
        );
    }

    fn torus(major: f64, minor: f64) -> Surface {
        Surface::Torus {
            center: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            major_radius: major,
            minor_radius: minor,
        }
    }

    #[test]
    fn newton_relocates_onto_torus_plane_intersection() {
        // Torus R=3 r=1 axis +z; oblique-ish plane x = 3.4 (a spiric section,
        // NOT a conic). A chord point near the curve must land on BOTH surfaces.
        let t = torus(3.0, 1.0);
        let plane = Surface::Plane {
            normal: Vector3::new(1.0, 0.0, 0.0),
            d: -3.4,
        };
        // Seed: a torus surface point near x≈3.4, nudged off both surfaces.
        let (u, v) = (0.7_f64, 0.15_f64);
        let rad = 3.0 + 1.0 * u.cos();
        let seed = Point3::new(
            rad * v.cos() + 0.03,
            rad * v.sin() - 0.02,
            1.0 * u.sin() + 0.04,
        );
        let relocated = relocate_onto_implicit_pair(seed, t, plane).expect("converges");
        let ft = signed_distance_to_surface(t, relocated).unwrap();
        let fp = signed_distance_to_surface(plane, relocated).unwrap();
        assert!(ft.abs() <= cad_primitives::TAU_MODEL, "off torus: {ft:e}");
        assert!(fp.abs() <= cad_primitives::TAU_MODEL, "off plane: {fp:e}");
    }

    #[test]
    fn newton_relocates_onto_torus_cylinder_intersection() {
        let t = torus(3.0, 1.0);
        // Cylinder coaxial-offset: axis ∥ +y through (3,0,0), radius 0.6 — cuts
        // the tube near θ=0 in a degree-4 curve.
        let cyl = Surface::Cylinder {
            axis_point: Point3::new(3.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 1.0, 0.0),
            radius: 0.6,
        };
        let seed = Point3::new(3.5, 0.1, 0.45);
        let r = relocate_onto_implicit_pair(seed, t, cyl).expect("converges");
        assert!(signed_distance_to_surface(t, r).unwrap().abs() <= cad_primitives::TAU_MODEL);
        assert!(signed_distance_to_surface(cyl, r).unwrap().abs() <= cad_primitives::TAU_MODEL);
    }

    #[test]
    fn newton_stops_when_there_is_no_intersection() {
        // Plane x = 10 lies entirely outside the torus (max x = R+r = 4): no
        // common zero, so the relocation must REFUSE (no curve to land on)
        // rather than wander to a wrong point.
        let t = torus(3.0, 1.0);
        let far = Surface::Plane {
            normal: Vector3::new(1.0, 0.0, 0.0),
            d: -10.0,
        };
        let seed = Point3::new(3.5, 0.0, 0.2);
        assert!(
            relocate_onto_implicit_pair(seed, t, far).is_none(),
            "no intersection ⇒ STOP, not a guessed relocation"
        );
    }
}

/// PR-YR12 (P2b): full outward radial normal of a sphere face at the centroid of
/// `tri` — `normalize(centroid − center)`. The analog of `radial_outward_normal`
/// but with no axis projection (a sphere is isotropic). Used to orient sphere
/// triangle winding via `orient_tri`.
fn sphere_outward_normal(verts: &[Point3], tri: &[u32; 3], center: Point3) -> [f64; 3] {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    let cen = [
        (a[0] + b[0] + c[0]) / 3.0,
        (a[1] + b[1] + c[1]) / 3.0,
        (a[2] + b[2] + c[2]) / 3.0,
    ];
    let ctr = center.as_array();
    normalize3([cen[0] - ctr[0], cen[1] - ctr[1], cen[2] - ctr[2]])
}

/// PR-YR7: outward radial normal of the cylinder surface at the centroid of
/// `tri` — the component of `(centroid − axis_point)` perpendicular to the
/// axis, normalized. Used to orient lateral triangle winding (governance
/// A15.5). Falls back to the raw radial vector if it is (near-)axial.
fn radial_outward_normal(
    verts: &[Point3],
    tri: &[u32; 3],
    axis_point: [f64; 3],
    axis_unit: [f64; 3],
) -> [f64; 3] {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    let cen = [
        (a[0] + b[0] + c[0]) / 3.0,
        (a[1] + b[1] + c[1]) / 3.0,
        (a[2] + b[2] + c[2]) / 3.0,
    ];
    let w = [
        cen[0] - axis_point[0],
        cen[1] - axis_point[1],
        cen[2] - axis_point[2],
    ];
    let along = w[0] * axis_unit[0] + w[1] * axis_unit[1] + w[2] * axis_unit[2];
    let radial = [
        w[0] - along * axis_unit[0],
        w[1] - along * axis_unit[1],
        w[2] - along * axis_unit[2],
    ];
    normalize3(radial)
}

/// PR-YR16 (spec §4): outward normal of a cone lateral at the centroid of `tri`.
///
/// The cone normal is TILTED ⟂ the generator (NOT purely radial like the
/// cylinder). A cone point is `P = apex + s·â + s·tanα·r̂` with generator
/// `g = â + tanα·r̂`; the surface normal lies in `span{â, r̂}` ⟂ `g`. Imposing
/// `n·g = 0` on `n = a·r̂ + b·â` gives `b = −a·tanα`, so the outward
/// (positive-radial) normal is `n̂ = unit(r̂ − tanα·â)`. The analog of
/// `radial_outward_normal` / `sphere_outward_normal`, feeding `orient_tri`. The
/// fan-triangle centroid sits at ≈ 2/3 of the way to the rim, so its radial
/// component is never degenerate near the apex.
fn cone_outward_normal(
    verts: &[Point3],
    tri: &[u32; 3],
    apex: Point3,
    axis_dir: Vector3,
    half_angle: f64,
) -> [f64; 3] {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    let cen = [
        (a[0] + b[0] + c[0]) / 3.0,
        (a[1] + b[1] + c[1]) / 3.0,
        (a[2] + b[2] + c[2]) / 3.0,
    ];
    let ax = normalize3(axis_dir.as_array());
    let ap = apex.as_array();
    let w = [cen[0] - ap[0], cen[1] - ap[1], cen[2] - ap[2]];
    let along = w[0] * ax[0] + w[1] * ax[1] + w[2] * ax[2];
    let radial_vec = [
        w[0] - along * ax[0],
        w[1] - along * ax[1],
        w[2] - along * ax[2],
    ];
    let rhat = normalize3(radial_vec);
    let t = half_angle.tan();
    normalize3([
        rhat[0] - t * ax[0],
        rhat[1] - t * ax[1],
        rhat[2] - t * ax[2],
    ])
}

/// PR-YR7: flip `tri`'s winding (swap last two verts) if its geometric normal
/// `(v1−v0)×(v2−v0)` opposes the analytic outward normal `target`.
fn orient_tri(verts: &[Point3], tri: &mut [u32; 3], target: [f64; 3]) {
    let a = verts[tri[0] as usize].as_array();
    let b = verts[tri[1] as usize].as_array();
    let c = verts[tri[2] as usize].as_array();
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    let dot = cross[0] * target[0] + cross[1] * target[1] + cross[2] * target[2];
    if dot < 0.0 {
        tri.swap(1, 2);
    }
}

/// PR-YR8 (P2c): the Stage-1 chord-error bound `d_ε = 1e-2 × analytic-AABB-diag`
/// for a solid, derived from its `Curve::Circle` rim edges (spec §4 Blocker 1).
///
/// This is the **single source** (governance A14.3) of the `1e-2` chord-bound
/// constant: both `BRep::new` (which derives the cylinder tessellation `n_seg`
/// from it) and Stage-6 face resolution (which uses it as the per-curved-face
/// membership tolerance, degenerate and non-degenerate alike) call this — there
/// is no second copy of the math or the literal anywhere in the crate.
///
/// Per axis a circle of center `c`, unit normal `n`, radius `r` spans
/// `c_i ± r·√(max(0, 1 − n_i²))`; the AABB is the union of those spans over all
/// rim circles. Returns:
/// - `Some(1e-2 × diag)` when the solid has ≥1 `Curve::Circle` rim (it has a
///   tessellated curved face, so it exposes a chord band), or
/// - `None` when there are no circle rims (an all-planar solid has zero chord
///   error; its faces resolve at `TAU_WORK`, not at a curved band).
fn curved_chord_bound(edges: &[BRepEdge]) -> Option<f64> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut any = false;
    for e in edges {
        if let Curve::Circle {
            center,
            normal,
            radius,
        } = e.curve
        {
            any = true;
            let nu = normalize3(normal.as_array());
            let c = center.as_array();
            for i in 0..3 {
                let span = radius * (1.0 - nu[i] * nu[i]).max(0.0).sqrt();
                lo[i] = lo[i].min(c[i] - span);
                hi[i] = hi[i].max(c[i] + span);
            }
        }
    }
    if !any {
        return None;
    }
    let dx = hi[0] - lo[0];
    let dy = hi[1] - lo[1];
    let dz = hi[2] - lo[2];
    let diag = (dx * dx + dy * dy + dz * dz).sqrt();
    Some(1e-2 * diag)
}

/// PR-YR15: the Stage-1 chord bound for a `Surface::Sphere` tessellation,
/// `d_ε = 1e-2 · 2r√3` (the sphere's bounding-cube diagonal × 1e-2). SINGLE
/// SOURCE OF TRUTH (A14.3): both `tessellate_sphere_face` (which derives the
/// tessellation `n_lon`/`n_lat` from it) and Stage-6 face resolution (`tol_for`,
/// which uses it as the per-sphere-face membership tolerance) call this — there
/// is no second copy of the literal anywhere in the crate.
///
/// NOTE: this is NOT `curved_chord_bound` (the Circle-rim AABB × 1e-2). The
/// rim circle's AABB diagonal is `2r√2`, which UNDERESTIMATES the sphere's own
/// `2r√3` chord error, so a sphere face must use its own bound here — not the
/// rim band. This is A14.3/A15, not tolerance widening.
fn sphere_chord_bound(radius: f64) -> f64 {
    1e-2 * 2.0 * radius * 3f64.sqrt()
}

/// PR-YR16 (spec §3): the Stage-1 chord bound for a `Surface::Cone`
/// tessellation, `d_ε = 1e-2 · √((2R)² + h²)` with `R = height·tan(half_angle)`.
/// SINGLE SOURCE OF TRUTH (A14.3) of the cone's `1e-2` literal: both the
/// pre-pass N-sizing (folded in via `min()` whenever a cone face is present)
/// and the test-side oracle compute this exact value, so they agree by
/// construction.
///
/// NOTE: this is NOT `curved_chord_bound` (the Circle-rim AABB × 1e-2). The
/// rim's AABB diagonal `2R√2` IGNORES the cone height and can EXCEED the cone's
/// honest bound for wide-short cones (`h < 2R`), so a cone face must fold in its
/// own bound — not rely on the rim band alone. This is A14.3/A15, not tolerance
/// widening.
fn cone_chord_bound(height: f64, half_angle: f64) -> f64 {
    let r = height * half_angle.tan();
    1e-2 * ((2.0 * r).powi(2) + height.powi(2)).sqrt()
}

/// PR-YR7: signed distance from `point` to an analytic `surface` (spec §5).
///
/// - `Plane { normal, d }` → `normal·point + d` (the stored normal, as the
///   planar fixtures use unit normals — same convention as the existing
///   `plane_dist`).
/// - `Cylinder { axis_point, axis_dir, radius }` → `dist(point, axis) − radius`.
/// - `Sphere { center, radius }` → `|point − center| − radius` (PR-YR12).
/// - `Cone { apex, axis_dir, half_angle }` → signed radial residual
///   `radial − |h_axial|·tanα` (PR-YR16, spec §5.3): positive outside the
///   lateral, negative inside, ≈ 0 on the surface. LOUD `Ok` — never a panic
///   or planar approximation.
pub fn signed_distance_to_surface(surface: Surface, point: Point3) -> Result<f64, YangError> {
    let x = point.as_array();
    match surface {
        Surface::Plane { normal, d } => {
            let n = normal.as_array();
            Ok(n[0] * x[0] + n[1] * x[1] + n[2] * x[2] + d)
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            let au = normalize3(axis_dir.as_array());
            let ap = axis_point.as_array();
            let w = [x[0] - ap[0], x[1] - ap[1], x[2] - ap[2]];
            let along = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
            let radial = [
                w[0] - along * au[0],
                w[1] - along * au[1],
                w[2] - along * au[2],
            ];
            let dist =
                (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
            Ok(dist - radius)
        }
        Surface::Sphere { center, radius } => {
            let c = center.as_array();
            let w = [x[0] - c[0], x[1] - c[1], x[2] - c[2]];
            Ok((w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt() - radius)
        }
        // PR-YR16 (spec §5.3): SIGNED radial residual of the cone lateral.
        // Positive outside the lateral, negative inside, ≈ 0 on the surface —
        // the honest analog of the Cylinder/Sphere signed arms. LOUD `Ok`
        // (never a panic or planar approximation).
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let au = normalize3(axis_dir.as_array());
            let a = apex.as_array();
            let w = [x[0] - a[0], x[1] - a[1], x[2] - a[2]];
            let h_axial = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
            let radial_vec = [
                w[0] - h_axial * au[0],
                w[1] - h_axial * au[1],
                w[2] - h_axial * au[2],
            ];
            let radial = (radial_vec[0] * radial_vec[0]
                + radial_vec[1] * radial_vec[1]
                + radial_vec[2] * radial_vec[2])
                .sqrt();
            Ok(radial - h_axial.abs() * half_angle.tan())
        }
        // KV6d: signed distance to the torus tube surface,
        // `√((ρ − R)² + τ²) − r` (ρ = radial dist from axis, τ = axial). <0
        // inside the tube, >0 outside, ≈0 on the surface.
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } => {
            let au = normalize3(axis_dir.as_array());
            let c = center.as_array();
            let w = [x[0] - c[0], x[1] - c[1], x[2] - c[2]];
            let tau = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
            let radial_vec = [w[0] - tau * au[0], w[1] - tau * au[1], w[2] - tau * au[2]];
            let rho = (radial_vec[0] * radial_vec[0]
                + radial_vec[1] * radial_vec[1]
                + radial_vec[2] * radial_vec[2])
                .sqrt();
            let d = rho - major_radius;
            Ok((d * d + tau * tau).sqrt() - minor_radius)
        }
    }
}

// =========================================================================
// PR-YR10 — Stage 4: relocate mesh intersection points onto exact curves
// (Yang §4.4.1 mesh updating) + §4.5.3 reversed-intersection correction.
// =========================================================================

/// PR-YR10 (spec §4.3): closed-form radial projection of `p` onto the exact
/// `Circle { center, normal, radius }`. Returns `(proj, t)` where `t` is the
/// angle in the circle's `ortho_basis(normal)` frame — the SAME frame Stage-1
/// sampling and [`BRep::eval_source`] use, so a relocated vertex tagged
/// `BRepEdge { edge, t }` round-trips exactly.
///
/// `Err(OnAxis)` when the point's radial component is below `MIN_FEATURE_SIZE`
/// (the projection direction is undefined on the axis). No Newton, no tolerance
/// widening (P9).
fn project_onto_circle(
    p: Point3,
    center: Point3,
    normal: Vector3,
    radius: f64,
) -> Result<(Point3, f64), Stage4InvalidReason> {
    let (e1, e2) = ortho_basis(normal);
    let e1a = e1.as_array();
    let e2a = e2.as_array();
    let c = center.as_array();
    let x = p.as_array();
    let w = [x[0] - c[0], x[1] - c[1], x[2] - c[2]];
    let u = w[0] * e1a[0] + w[1] * e1a[1] + w[2] * e1a[2];
    let v = w[0] * e2a[0] + w[1] * e2a[1] + w[2] * e2a[2];
    let rho = u.hypot(v);
    if rho < cad_primitives::MIN_FEATURE_SIZE {
        return Err(Stage4InvalidReason::OnAxis);
    }
    let t = v.atan2(u);
    let (ct, st) = (t.cos(), t.sin());
    let proj = Point3::new(
        c[0] + radius * (ct * e1a[0] + st * e2a[0]),
        c[1] + radius * (ct * e1a[1] + st * e2a[1]),
        c[2] + radius * (ct * e1a[2] + st * e2a[2]),
    );
    Ok((proj, t))
}

/// KV6d Tier B: the implicit value `F(x)` and UNIT gradient (the surface's
/// analytic unit normal) of a `Surface` at `x`. `F` matches
/// [`signed_distance_to_surface`] byte-for-byte (so the residual gate is
/// shared), and the gradient is `∇F / |∇F|`. Returns `None` where the normal is
/// undefined — a point on a cylinder/torus axis, a cone apex, a sphere centre,
/// or (torus) a point on the tube centre circle — which the caller treats as a
/// loud STOP, never a guess (P9).
fn surface_value_and_normal(s: Surface, x: [f64; 3]) -> Option<(f64, [f64; 3])> {
    let eps = cad_primitives::MIN_FEATURE_SIZE;
    match s {
        Surface::Plane { normal, d } => {
            let n = normal.as_array();
            let f = n[0] * x[0] + n[1] * x[1] + n[2] * x[2] + d;
            Some((f, normalize3(n)))
        }
        Surface::Sphere { center, radius } => {
            let c = center.as_array();
            let w = [x[0] - c[0], x[1] - c[1], x[2] - c[2]];
            let l = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
            if l < eps {
                return None;
            }
            Some((l - radius, [w[0] / l, w[1] / l, w[2] / l]))
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            let au = normalize3(axis_dir.as_array());
            let ap = axis_point.as_array();
            let w = [x[0] - ap[0], x[1] - ap[1], x[2] - ap[2]];
            let along = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
            let rad = [
                w[0] - along * au[0],
                w[1] - along * au[1],
                w[2] - along * au[2],
            ];
            let l = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
            if l < eps {
                return None;
            }
            Some((l - radius, [rad[0] / l, rad[1] / l, rad[2] / l]))
        }
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let au = normalize3(axis_dir.as_array());
            let a = apex.as_array();
            let w = [x[0] - a[0], x[1] - a[1], x[2] - a[2]];
            let h = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
            let rad = [w[0] - h * au[0], w[1] - h * au[1], w[2] - h * au[2]];
            let l = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
            if l < eps {
                return None;
            }
            let f = l - h.abs() * half_angle.tan();
            // ∇F = r̂ − sign(h)·tanα·â ; its unit form is the cone normal
            // cosα·r̂ − sign(h)·sinα·â.
            let sgn = if h >= 0.0 { 1.0 } else { -1.0 };
            let (sa, ca) = half_angle.sin_cos();
            let g = [
                ca * rad[0] / l - sgn * sa * au[0],
                ca * rad[1] / l - sgn * sa * au[1],
                ca * rad[2] / l - sgn * sa * au[2],
            ];
            Some((f, normalize3(g)))
        }
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } => {
            let au = normalize3(axis_dir.as_array());
            let c = center.as_array();
            let w = [x[0] - c[0], x[1] - c[1], x[2] - c[2]];
            let tau = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
            let rad = [w[0] - tau * au[0], w[1] - tau * au[1], w[2] - tau * au[2]];
            let rho = (rad[0] * rad[0] + rad[1] * rad[1] + rad[2] * rad[2]).sqrt();
            if rho < eps {
                return None; // on the torus axis: radial direction undefined
            }
            let rhat = [rad[0] / rho, rad[1] / rho, rad[2] / rho];
            // Nearest tube-centre-circle point q = c + R·r̂; normal = (x − q)/|x − q|.
            let q = [
                c[0] + major_radius * rhat[0],
                c[1] + major_radius * rhat[1],
                c[2] + major_radius * rhat[2],
            ];
            let xq = [x[0] - q[0], x[1] - q[1], x[2] - q[2]];
            let l = (xq[0] * xq[0] + xq[1] * xq[1] + xq[2] * xq[2]).sqrt();
            if l < eps {
                return None; // on the tube centre circle: normal undefined
            }
            Some((l - minor_radius, [xq[0] / l, xq[1] / l, xq[2] / l]))
        }
    }
}

/// KV6d Tier B: relocate `p` onto the exact intersection curve of two surfaces
/// by Gauss–Newton on the implicit system `{F0(x)=0, F1(x)=0}` — the degree-4
/// analog of the closed-form conic projectors, used when a torus is one of the
/// pair (a torus's intersections are not conics, so there is no closed form).
/// Each step is the least-norm solution of `J·dx = −[F0; F1]`, with
/// `J = [n̂0; n̂1]` the 2×3 unit-normal Jacobian; for unit rows
/// `J Jᵀ = [[1, b], [b, 1]]`, `b = n̂0·n̂1`.
///
/// Returns the relocated point with both residuals ≤ `TAU_MODEL`, or `None` for
/// a loud STOP (P9 — never a partial move or a guessed root):
/// - a TANGENTIAL pair (`sin²θ = 1 − b² ≤ MIN_FEATURE_SIZE²`): `J` is rank-
///   deficient and the intersection root is ill-posed;
/// - an UNDEFINED normal at the iterate (axis / apex / centre circle);
/// - NON-CONVERGENCE within `MAX_ITERS`.
fn relocate_onto_implicit_pair(p: Point3, s0: Surface, s1: Surface) -> Option<Point3> {
    const MAX_ITERS: usize = 32;
    // Converge tightly (well below the 1e-12 on-surface validation band; the
    // torus residual is ~2·minor·|F|): Newton is quadratic so this is a few
    // extra cheap steps. Absolute tol suits the unit-scale model corpus.
    let tau = 1e-13_f64;
    let rank_eps = cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE;
    let mut x = p.as_array();
    for _ in 0..=MAX_ITERS {
        let (f0, n0) = surface_value_and_normal(s0, x)?;
        let (f1, n1) = surface_value_and_normal(s1, x)?;
        let b = n0[0] * n1[0] + n0[1] * n1[1] + n0[2] * n1[2];
        let det = 1.0 - b * b; // sin²θ between the two unit normals
                               // Tangential / parallel normals → no transversal 1D intersection curve
                               // to relocate onto (the contact is a point or a higher-order tangency).
                               // STOP whether or not the residual is already small — a tangent point IS
                               // on both surfaces but is not a curve a mesh edge can lie along.
        if det <= rank_eps {
            return None;
        }
        if f0.abs() <= tau && f1.abs() <= tau {
            return Some(Point3::new(x[0], x[1], x[2]));
        }
        // (J Jᵀ)⁻¹ [f0; f1] = 1/det · [[1, −b], [−b, 1]] [f0; f1]
        let m0 = (f0 - b * f1) / det;
        let m1 = (f1 - b * f0) / det;
        // dx = −Jᵀ m = −(n̂0·m0 + n̂1·m1)
        x = [
            x[0] - (n0[0] * m0 + n1[0] * m1),
            x[1] - (n0[1] * m0 + n1[1] * m1),
            x[2] - (n0[2] * m0 + n1[2] * m1),
        ];
    }
    None
}

/// KV6d Tier B junction: relocate `p` onto the common point of THREE surfaces
/// `{F0=0, F1=0, F2=0}` by Newton on the square system (3×3 Jacobian of unit
/// normals). The torus analog of the conic line+circle / ellipse×ellipse
/// junctions — e.g. a box EDGE (two planes) piercing the torus: the shared
/// vertex must land on the torus AND both planes, else relocating it onto only
/// one pair slides it off the third. `None` is a loud STOP: a degenerate
/// (near-coplanar normals → `|det J|` below the rank floor) junction, an
/// undefined normal, or non-convergence (P9 — no partial move).
fn relocate_onto_implicit_triple(
    p: Point3,
    s0: Surface,
    s1: Surface,
    s2: Surface,
) -> Option<Point3> {
    const MAX_ITERS: usize = 32;
    // Converge tightly (well below the 1e-12 on-surface validation band; the
    // torus residual is ~2·minor·|F|): Newton is quadratic so this is a few
    // extra cheap steps. Absolute tol suits the unit-scale model corpus.
    let tau = 1e-13_f64;
    let rank_eps = cad_primitives::MIN_FEATURE_SIZE;
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let mut x = p.as_array();
    for _ in 0..=MAX_ITERS {
        let (f0, n0) = surface_value_and_normal(s0, x)?;
        let (f1, n1) = surface_value_and_normal(s1, x)?;
        let (f2, n2) = surface_value_and_normal(s2, x)?;
        let c12 = cross(n1, n2);
        let det = n0[0] * c12[0] + n0[1] * c12[1] + n0[2] * c12[2];
        if det.abs() <= rank_eps {
            return None; // coplanar normals → ill-posed junction
        }
        if f0.abs() <= tau && f1.abs() <= tau && f2.abs() <= tau {
            return Some(Point3::new(x[0], x[1], x[2]));
        }
        // dx = −J⁻¹ f = −(1/det)[ f0·(n1×n2) + f1·(n2×n0) + f2·(n0×n1) ]
        let c20 = cross(n2, n0);
        let c01 = cross(n0, n1);
        for i in 0..3 {
            x[i] -= (f0 * c12[i] + f1 * c20[i] + f2 * c01[i]) / det;
        }
    }
    None
}

/// PR-YR10 (spec §4.4): per-component residual `(|axial|, |radial − r|)` of `pt`
/// to an exact circle. This is the spec §4.5 classification residual the Stage-4
/// relocation drives ≤ `TAU_MODEL`. The legacy combined form
/// `ρ = max(|axial|, |radial − r|)` (PR-YR10) is recovered as `axial.max(radial_dev)`.
///
/// PR-YR19: the Stage-4 circle relocation guard splits the residual so the
/// in-plane RADIAL band can be the propagated `(R/r_c)·d_ε` for a sphere section
/// circle while the AXIAL band stays `d_ε` (spec §2/§4 Site 2, N11). Non-sphere
/// callers fold it back to the combined max, so behavior there is byte-identical.
fn circle_residual_split(pt: Point3, center: Point3, normal: Vector3, radius: f64) -> (f64, f64) {
    let n = normalize3(normal.as_array());
    let c = center.as_array();
    let x = pt.as_array();
    let w = [x[0] - c[0], x[1] - c[1], x[2] - c[2]];
    let axial = (w[0] * n[0] + w[1] * n[1] + w[2] * n[2]).abs();
    let radial_vec = [
        w[0] - (w[0] * n[0] + w[1] * n[1] + w[2] * n[2]) * n[0],
        w[1] - (w[0] * n[0] + w[1] * n[1] + w[2] * n[2]) * n[1],
        w[2] - (w[0] * n[0] + w[1] * n[1] + w[2] * n[2]) * n[2],
    ];
    let radial = (radial_vec[0] * radial_vec[0]
        + radial_vec[1] * radial_vec[1]
        + radial_vec[2] * radial_vec[2])
        .sqrt();
    (axial, (radial - radius).abs())
}

/// M8 disc∩disc CROSSING: the exact intersection of two COPLANAR circles
/// `(c_a, n_a, r_a)` and `(c_b, n_b, r_b)`, picking the root nearest `near`.
/// Two coplanar circles meet in ≤ 2 points (the lens corners); closed-form 2D
/// in their shared plane. Returns `None` (→ a loud Stage-4 STOP) when the
/// circles are NOT coplanar (parallel normals + co-planar centers), are
/// concentric, or do not actually cross — none of which is a disc∩disc lens
/// corner, so we never guess.
fn coplanar_circle_circle_intersection(
    c_a: Point3,
    n_a: Vector3,
    r_a: f64,
    c_b: Point3,
    n_b: Vector3,
    r_b: f64,
    near: Point3,
) -> Option<Point3> {
    let n = normalize3(n_a.as_array());
    let nb = normalize3(n_b.as_array());
    let ca = c_a.as_array();
    let cb = c_b.as_array();
    // Coplanarity: normals parallel AND c_b in c_a's plane.
    let cross_n = [
        n[1] * nb[2] - n[2] * nb[1],
        n[2] * nb[0] - n[0] * nb[2],
        n[0] * nb[1] - n[1] * nb[0],
    ];
    let cross_mag =
        (cross_n[0] * cross_n[0] + cross_n[1] * cross_n[1] + cross_n[2] * cross_n[2]).sqrt();
    let u = [cb[0] - ca[0], cb[1] - ca[1], cb[2] - ca[2]];
    let off_plane = (u[0] * n[0] + u[1] * n[1] + u[2] * n[2]).abs();
    if cross_mag > cad_primitives::MIN_FEATURE_SIZE || off_plane > cad_primitives::MIN_FEATURE_SIZE
    {
        return None; // not coplanar → not a disc∩disc lens corner
    }
    let d = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
    if d < cad_primitives::MIN_FEATURE_SIZE {
        return None; // concentric
    }
    let uh = [u[0] / d, u[1] / d, u[2] / d];
    // Distance from c_a to the radical line along û.
    let a = (d * d + r_a * r_a - r_b * r_b) / (2.0 * d);
    let h2 = r_a * r_a - a * a;
    if h2 <= 0.0 {
        return None; // circles do not cross (tangent/disjoint)
    }
    let h = h2.sqrt();
    let m = [ca[0] + a * uh[0], ca[1] + a * uh[1], ca[2] + a * uh[2]];
    // In-plane perpendicular: v̂ = n × û (unit by construction).
    let vh = [
        n[1] * uh[2] - n[2] * uh[1],
        n[2] * uh[0] - n[0] * uh[2],
        n[0] * uh[1] - n[1] * uh[0],
    ];
    let q = near.as_array();
    let mut best: Option<(Point3, f64)> = None;
    for s in [h, -h] {
        let x = [m[0] + s * vh[0], m[1] + s * vh[1], m[2] + s * vh[2]];
        let dd = (x[0] - q[0]).powi(2) + (x[1] - q[1]).powi(2) + (x[2] - q[2]).powi(2);
        if best.map(|(_, b)| dd < b).unwrap_or(true) {
            best = Some((Point3::new(x[0], x[1], x[2]), dd));
        }
    }
    best.map(|(p, _)| p)
}

/// PR-F3 (KV6b-F3): the exact ruling LINE for one plane∥axis × cylinder
/// intersection edge (`ssi` plane_cylinder C3a/C3b), carried per-vertex like
/// `vert_circle`. The stored `Curve::LineSegment` carries no analytic data, so
/// Stage 4 recomputes the line from the edge's incidence (cylinder + plane)
/// and re-selects the unique candidate through both endpoints — the SAME
/// matching rule Stage 3 used, so selection here cannot disagree with it.
#[derive(Clone, Copy)]
struct LineReloc {
    point: Point3,
    dir: Vector3,
    /// PR-F3b/PR-KV9: the ABSOLUTE residual budget for the line-distance
    /// metric — the owner chord band(s) propagated through the
    /// [`line_band_amplification`] metric conversion. Cylinder×plane:
    /// `amp · d_ε(cylinder owner)`; cylinder×cylinder: `amp · (d_ε(A) +
    /// d_ε(B))` (both meshes' chords contribute to the crossing).
    band_budget: f64,
}

/// Per-vertex circle assignment `(center, normal, radius, source_sphere_radius)`
/// — the `vert_circle` value tuple, shared by the PR-F3 line+circle junction map.
type CircleAssign = (Point3, Vector3, f64, Option<f64>);

/// PR-F3b: band amplification for a ruling-LINE membership / residual test on
/// a `cylinder × plane` pair. A mesh point on a Stage-1 facet chord is off the
/// cylinder RADIALLY by `ρ ≤ d_ε` (the Stage-1 contract), but its
/// perpendicular distance to the C3a intersection line — measured IN the
/// cutting plane — is `ρ·r/√(r² − d²)` where `d` is the axis-to-plane
/// distance: the radial deficit divides by the cosine between the radial
/// direction at the line and the in-plane direction. This is the line analog
/// of the PR-YR19 sphere section-circle `(R/r_c)` propagation (see
/// `chord_band_propagates_into_section_metric`): a DERIVED metric conversion,
/// not tolerance widening. Surface-normal backstops (the on-both-surfaces
/// gate) stay UNSCALED. Returns `None` for non-cylinder/plane pairs and for
/// near-tangent planes (`d → r`, where the factor diverges — such a pair
/// fails loud upstream rather than matching everything).
fn line_band_amplification(surf0: Surface, surf1: Surface) -> Option<f64> {
    // PR-KV9: PARALLEL cylinder × cylinder ruling lines (ssi cyl∥cyl secant/
    // tangent). The same gradient-geometry derivation applies with BOTH
    // constraint gradients radial: at a crossing point X of the two
    // cross-section circles (radii r1, r2, inter-axis distance d), the angle
    // α between the radial directions r̂ᵢ = (X − cᵢ)/rᵢ follows the law of
    // cosines in the triangle (c1, c2, X):
    //   cos α = (r1² + r2² − d²) / (2·r1·r2)
    // (symmetric for both crossing lines), and the membership band is
    // 1/sin α — the general 1/‖ĝ1 × ĝ2‖ form the cylinder×plane case is a
    // special case of (there ĝ2 = the plane normal and 1/sin α reduces to
    // r/√(r² − d²)). Near-tangent (sin α → 0) returns None: no finite
    // propagated band, the pair fails loud upstream.
    if let (
        Surface::Cylinder {
            axis_point: p1,
            axis_dir: d1,
            radius: r1,
        },
        Surface::Cylinder {
            axis_point: p2,
            axis_dir: d2,
            radius: r2,
        },
    ) = (surf0, surf1)
    {
        let u1 = normalize3(d1.as_array());
        let u2 = normalize3(d2.as_array());
        let cx = [
            u1[1] * u2[2] - u1[2] * u2[1],
            u1[2] * u2[0] - u1[0] * u2[2],
            u1[0] * u2[1] - u1[1] * u2[0],
        ];
        let cross_norm = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
        if cross_norm > cad_primitives::TAU_MODEL || r1 <= 0.0 || r2 <= 0.0 {
            return None; // non-parallel axes: no ruling lines from this pair
        }
        let q1a = p1.as_array();
        let q2a = p2.as_array();
        let rel = [q2a[0] - q1a[0], q2a[1] - q1a[1], q2a[2] - q1a[2]];
        let along = rel[0] * u1[0] + rel[1] * u1[1] + rel[2] * u1[2];
        let perp = [
            rel[0] - along * u1[0],
            rel[1] - along * u1[1],
            rel[2] - along * u1[2],
        ];
        let d = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
        let cos_a = ((r1 * r1 + r2 * r2 - d * d) / (2.0 * r1 * r2)).clamp(-1.0, 1.0);
        let sin_a = (1.0 - cos_a * cos_a).max(0.0).sqrt();
        if sin_a <= cad_primitives::MIN_FEATURE_SIZE {
            return None; // tangent-grade crossing: band diverges
        }
        return Some(1.0 / sin_a);
    }
    let (cyl, pl) = match (surf0, surf1) {
        (c @ Surface::Cylinder { .. }, p @ Surface::Plane { .. }) => (c, p),
        (p @ Surface::Plane { .. }, c @ Surface::Cylinder { .. }) => (c, p),
        _ => return None,
    };
    let (
        Surface::Cylinder {
            axis_point, radius, ..
        },
        Surface::Plane { normal, d },
    ) = (cyl, pl)
    else {
        return None;
    };
    let n = normal.as_array();
    let nn = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    // NaN-safe: a non-finite `nn` fails the `>=` and returns None.
    if nn < cad_primitives::MIN_FEATURE_SIZE || !nn.is_finite() || radius <= 0.0 {
        return None;
    }
    let p = axis_point.as_array();
    let dist = ((n[0] * p[0] + n[1] * p[1] + n[2] * p[2] + d) / nn).abs();
    let half_sep_sq = radius * radius - dist * dist;
    if half_sep_sq <= (cad_primitives::MIN_FEATURE_SIZE * radius).powi(2) {
        return None; // near-tangent: no finite propagated band.
    }
    Some(radius / half_sep_sq.sqrt())
}

/// PR-KV9: per-point membership amplification for a curve on TWO cylinders
/// — `1/‖ĝ₁×ĝ₂‖` with `ĝᵢ` the unit radial gradients of the two cylinders
/// at `x`. The constraint-band intersection at angle α has diameter
/// `(ρ₁+ρ₂)/sin α`; near surface tangency (the Steinmetz ellipse crossing
/// points, where both radials align) the band legitimately diverges —
/// `None` there, and the caller falls back to the tangent-direction
/// discriminator.
fn cyl_cyl_point_amplification(
    x: Point3,
    c1: (Point3, Vector3),
    c2: (Point3, Vector3),
) -> Option<f64> {
    let grad = |(ap, ad): (Point3, Vector3)| -> Option<[f64; 3]> {
        let a = normalize3(ad.as_array());
        let p = ap.as_array();
        let w = [x.x() - p[0], x.y() - p[1], x.z() - p[2]];
        let h = w[0] * a[0] + w[1] * a[1] + w[2] * a[2];
        let r = [w[0] - h * a[0], w[1] - h * a[1], w[2] - h * a[2]];
        let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        if rl < cad_primitives::MIN_FEATURE_SIZE {
            return None;
        }
        Some([r[0] / rl, r[1] / rl, r[2] / rl])
    };
    let g1 = grad(c1)?;
    let g2 = grad(c2)?;
    let cx = [
        g1[1] * g2[2] - g1[2] * g2[1],
        g1[2] * g2[0] - g1[0] * g2[2],
        g1[0] * g2[1] - g1[1] * g2[0],
    ];
    let sin_a = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    if sin_a < 1e-3 {
        return None; // tangency-grade: no finite band
    }
    Some(1.0 / sin_a)
}

/// PR-KV9: unit tangent of an ssi candidate curve at (the projection of)
/// `x` — the tangent-direction discriminator for multi-matched candidates.
/// `None` for curve types without a closed-form tangent here.
fn curve_tangent_at(curve: &ssi_rs::SsiCurve, x: Point3) -> Option<[f64; 3]> {
    match curve {
        ssi_rs::SsiCurve::Line { dir, .. } => Some(normalize3(dir.as_array())),
        ssi_rs::SsiCurve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            let n = normalize3(normal.as_array());
            let m = normalize3(major_axis.as_array());
            let w = [
                n[1] * m[2] - n[2] * m[1],
                n[2] * m[0] - n[0] * m[2],
                n[0] * m[1] - n[1] * m[0],
            ];
            let c = center.as_array();
            let dxv = [x.x() - c[0], x.y() - c[1], x.z() - c[2]];
            let u = (dxv[0] * m[0] + dxv[1] * m[1] + dxv[2] * m[2]) / major_radius;
            let v = (dxv[0] * w[0] + dxv[1] * w[1] + dxv[2] * w[2]) / minor_radius;
            let t = v.atan2(u);
            // dP/dt = −a·sin t·m̂ + b·cos t·ŵ
            let (st, ct) = t.sin_cos();
            let tan = [
                -major_radius * st * m[0] + minor_radius * ct * w[0],
                -major_radius * st * m[1] + minor_radius * ct * w[1],
                -major_radius * st * m[2] + minor_radius * ct * w[2],
            ];
            Some(normalize3(tan))
        }
        _ => None,
    }
}

/// Perpendicular distance of `p` to the line `(point, dir)` (`dir` need not be
/// unit). PR-F3 line membership / residual metric.
fn line_perp_distance(p: Point3, point: Point3, dir: Vector3) -> f64 {
    let d = normalize3(dir.as_array());
    let pt = point.as_array();
    let x = p.as_array();
    let w = [x[0] - pt[0], x[1] - pt[1], x[2] - pt[2]];
    let along = w[0] * d[0] + w[1] * d[1] + w[2] * d[2];
    let perp = [
        w[0] - along * d[0],
        w[1] - along * d[1],
        w[2] - along * d[2],
    ];
    (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt()
}

/// R0072: position tie-break among near-coincident PARALLEL line candidates.
///
/// A near-tangent `plane ∩ cylinder` secant yields two near-coincident parallel
/// generators; both pass the chord-band containment test, so a `matched == 1`
/// selector deadlocks (`AmbiguousCurve`). The tangent-direction discriminator
/// cannot help — parallel lines share a direction. But the mesh edge lies on
/// exactly ONE generator, which is nearer to BOTH endpoints.
///
/// Given matched line candidates `(point, unit dir)` and the edge endpoints,
/// return the index whose endpoint-distance interval `[min, max]` lies strictly
/// below every other candidate's (`hi_w < lo_j ∀ j≠w`): the winner's worst
/// endpoint still beats every rival's best, so the endpoints unambiguously lie
/// on it. Margin-free and scale-free. Returns `None` (caller keeps its loud
/// stop) when there are < 2 candidates, the candidates are NOT mutually parallel
/// (a transversal multi-match the tangent pass owns), or the intervals overlap
/// (generators merged below mesh resolution — true-tangency territory). P9: a
/// proximity tie-break on geometry already gate-verified on both surfaces, never
/// a band widening; genuine ambiguity is preserved. Spec
/// `specs/yr_r0072_parallel_line_position_tiebreak.md`.
fn select_disjoint_parallel_line(
    cands: &[(Point3, Vector3)],
    p_s: Point3,
    p_e: Point3,
) -> Option<usize> {
    if cands.len() < 2 {
        return None;
    }
    // Every candidate must be mutually parallel — the case the tangent
    // discriminator structurally cannot resolve.
    let dirs: Vec<[f64; 3]> = cands.iter().map(|c| normalize3(c.1.as_array())).collect();
    for w in dirs.windows(2) {
        let (a, b) = (w[0], w[1]);
        let c = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        if (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() >= cad_primitives::TAU_MODEL {
            return None;
        }
    }
    let ivs: Vec<(f64, f64)> = cands
        .iter()
        .map(|c| {
            let ds = line_perp_distance(p_s, c.0, c.1);
            let de = line_perp_distance(p_e, c.0, c.1);
            (ds.min(de), ds.max(de))
        })
        .collect();
    let wk = (0..ivs.len()).min_by(|&i, &j| {
        ivs[i]
            .1
            .partial_cmp(&ivs[j].1)
            .unwrap_or(std::cmp::Ordering::Equal)
    })?;
    let hi_w = ivs[wk].1;
    if (0..ivs.len()).all(|k| k == wk || hi_w < ivs[k].0) {
        Some(wk)
    } else {
        None
    }
}

/// PR-YR11 (spec §1): the true cylinder + cutting plane for one oblique ellipse
/// edge, carried per-vertex (analogous to `vert_circle`'s `(center, normal,
/// radius)`). The cylinder fields are `Surface::Cylinder`; the plane fields are
/// the cutting `Surface::Plane` (`n·x + d = 0`); the ellipse fields are the
/// stored `Curve::Ellipse` (for the relocation parameter `t` + the round-trip).
#[derive(Clone, Copy)]
struct EllipseReloc {
    axis_point: Point3,
    axis_dir: Vector3,
    radius: f64,
    plane_n: Vector3,
    plane_d: f64,
    center: Point3,
    normal: Vector3,
    major_axis: Vector3,
    major_radius: f64,
    minor_radius: f64,
    /// PR-KV9: `Some((other_axis_point, other_axis_dir, combined_band))`
    /// for a cylinder×CYLINDER section — the residual gate then uses the
    /// per-point gradient amplification against the combined band instead
    /// of the global `d_ε` (the metric conversion diverges at surface
    /// tangency, where the Stage-3 surface-membership gate remains the
    /// backstop). `None` keeps the cylinder×plane path byte-identical.
    second_cyl: Option<(Point3, Vector3, f64)>,
}

/// PR-YR11 (spec §2): relocate `p` onto the exact ellipse via the CYLINDER
/// parameterization (Yang §4.3.2) — closed-form, NO quartic. The relocated point
/// lies on BOTH the cylinder (radius `r` about its axis) AND the cutting plane
/// (`n·x + d = 0`), hence exactly on `plane ∩ cylinder` = the ellipse. Returns
/// `(proj, t)` where `t` is the ellipse parameter in the shared
/// [`ellipse_point`] frame, so a relocated vertex tagged `BRepEdge { edge, t }`
/// round-trips exactly.
///
/// LOUD STOPs (P9/P10), never a silent snap / divide-by-~0:
/// - `Err(OnAxis)` when the radial component `ρ < MIN_FEATURE_SIZE`.
/// - `Err(LocalRefinementRequired)` for the out-of-scope axis-parallel section
///   `|n·â| < MIN_FEATURE_SIZE` (the linear axial solve is degenerate there).
fn project_onto_ellipse_via_cylinder(
    p: Point3,
    er: &EllipseReloc,
) -> Result<(Point3, f64), Stage4InvalidReason> {
    let q = er.axis_point.as_array();
    let a_hat = normalize3(er.axis_dir.as_array());
    let n = normalize3(er.plane_n.as_array());
    // The plane offset `d` must be expressed for the UNIT normal `n`. The stored
    // `Surface::Plane` normals in the corpus are already unit, but normalize the
    // offset defensively against the same scale used for `n`.
    let n_raw = er.plane_n.as_array();
    let n_len = (n_raw[0] * n_raw[0] + n_raw[1] * n_raw[1] + n_raw[2] * n_raw[2]).sqrt();
    let d = if n_len < cad_primitives::TAU_WORK {
        er.plane_d
    } else {
        er.plane_d / n_len
    };
    let r = er.radius;
    let x = p.as_array();

    let w = [x[0] - q[0], x[1] - q[1], x[2] - q[2]];
    let along = w[0] * a_hat[0] + w[1] * a_hat[1] + w[2] * a_hat[2];
    let radial = [
        w[0] - along * a_hat[0],
        w[1] - along * a_hat[1],
        w[2] - along * a_hat[2],
    ];
    let rho = (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
    if rho < cad_primitives::MIN_FEATURE_SIZE {
        return Err(Stage4InvalidReason::OnAxis);
    }
    let rdir = [radial[0] / rho, radial[1] / rho, radial[2] / rho];

    let n_dot_a = n[0] * a_hat[0] + n[1] * a_hat[1] + n[2] * a_hat[2];
    if n_dot_a.abs() < cad_primitives::MIN_FEATURE_SIZE {
        // Axis-parallel / degenerate-line section: out of scope. Loud STOP rather
        // than dividing by ~0 (spec §6).
        return Err(Stage4InvalidReason::LocalRefinementRequired);
    }
    let n_dot_q = n[0] * q[0] + n[1] * q[1] + n[2] * q[2];
    let n_dot_rdir = n[0] * rdir[0] + n[1] * rdir[1] + n[2] * rdir[2];
    let s = -(n_dot_q + r * n_dot_rdir + d) / n_dot_a;

    let proj = Point3::new(
        q[0] + s * a_hat[0] + r * rdir[0],
        q[1] + s * a_hat[1] + r * rdir[1],
        q[2] + s * a_hat[2] + r * rdir[2],
    );
    let t = ellipse_param(
        proj,
        er.center,
        er.normal,
        er.major_axis,
        er.major_radius,
        er.minor_radius,
    );
    Ok((proj, t))
}

/// PR-YR21 (spec §3.1/§3.2): per-vertex Ellipse relocation data for a
/// `cone ∩ plane` oblique section — the cone analog of [`EllipseReloc`]. Carries
/// the true cone (apex / axis / half-angle), the cutting plane (`plane_n` /
/// `plane_d`), the stored ellipse params (for the `ellipse_param` round-trip),
/// and the cone's OWN Stage-1 chord budget `cone_d_eps`
/// (`cone_chord_bound(height, half_angle)`) — NOT the rim-AABB `d_eps`, so a
/// tall-thin cone's residual is gated against the honest cone bound.
#[derive(Clone, Copy)]
struct ConeEllipseReloc {
    apex: Point3,
    axis_dir: Vector3,
    half_angle: f64,
    plane_n: Vector3,
    plane_d: f64,
    center: Point3,
    normal: Vector3,
    major_axis: Vector3,
    major_radius: f64,
    minor_radius: f64,
    cone_d_eps: f64,
}

/// PR-YR22: per-vertex Parabola relocation data for a `cone ∩ plane` θ=α
/// (generator-parallel) section — the parabola sibling of [`ConeEllipseReloc`].
/// Carries the true cone (`apex` / `cone_axis_dir` / `half_angle`), the cutting
/// plane (`plane_n` / `plane_d`), and the stored parabola params (`vertex` /
/// parabola `normal` / `para_axis_dir` — these differ from the cone's
/// `cone_axis_dir`/normal, hence the unambiguous names), plus the cone's OWN
/// Stage-1 chord budget `cone_d_eps`. `focal_length` is not stored: the
/// relocation tag `t` is the conjugate-axis coordinate (needs only `vertex` /
/// `normal` / `para_axis_dir`), and `eval_source` / `is_reversed` recover the
/// full parabola from the output edge's own `Curve::Parabola` fields.
#[derive(Clone, Copy)]
struct ConeParabolaReloc {
    apex: Point3,
    cone_axis_dir: Vector3,
    half_angle: f64,
    plane_n: Vector3,
    plane_d: f64,
    vertex: Point3,
    normal: Vector3,
    para_axis_dir: Vector3,
    cone_d_eps: f64,
}

/// PR-YR23: per-vertex Hyperbola relocation data for a `cone ∩ plane`
/// axis-parallel (HYPE) section — the hyperbola sibling of [`ConeParabolaReloc`].
/// Carries the true cone (`apex` / `cone_axis_dir` / `half_angle`), the cutting
/// plane (`plane_n` / `plane_d`), and the stored hyperbola params (`center` /
/// hyperbola `normal` / `major_axis` / `semi_transverse` / `semi_conjugate`) plus
/// the cone's OWN Stage-1 chord budget `cone_d_eps`. The relocation tag `t` is
/// `asinh(v / b)` where `v` is the conjugate-axis coordinate (`(proj − center)·
/// (normal × major_axis)`) and `b = semi_conjugate` (the `sinh` coordinate is the
/// bijective one). `eval_source` / `is_reversed` recover the full hyperbola from
/// the output edge's own `Curve::Hyperbola` fields. (`semi_transverse` is NOT
/// stored: the relocation tag `t = asinh(v / b)` needs only `semi_conjugate`,
/// mirroring how [`ConeParabolaReloc`] omits `focal_length`.)
#[derive(Clone, Copy)]
struct ConeHyperbolaReloc {
    apex: Point3,
    cone_axis_dir: Vector3,
    half_angle: f64,
    plane_n: Vector3,
    plane_d: f64,
    center: Point3,
    normal: Vector3,
    major_axis: Vector3,
    semi_conjugate: f64,
    cone_d_eps: f64,
}

/// PR-YR21 (spec §3.1): relocate `p` onto the exact `cone ∩ plane` ellipse via
/// the CONE GENERATOR parameterization (Yang §4.3.2) — closed-form, NO quartic.
/// The cone analog of [`project_onto_ellipse_via_cylinder`]. The relocated point
/// is built on the cone generator at `p`'s azimuth (so it lies on the cone) and
/// solved to satisfy `n·x + d = 0` (so it lies on the plane), hence exactly on
/// `plane ∩ cone` = the ellipse. Returns only the relocated 3D point
/// (type-agnostic; the caller does its own conic param inversion — YR22/YR23
/// reuse this unchanged for parabola/hyperbola).
///
/// LOUD STOPs (P9/P10), never a silent snap / divide-by-~0:
/// - `Err(OnAxis)` when the radial component `ρ < MIN_FEATURE_SIZE`.
/// - `Err(LocalRefinementRequired)` when the generator is parallel to the plane
///   (`|n·g| < MIN_FEATURE_SIZE` — the asymptotic / parabola-tail direction,
///   out of scope) or the solved generator parameter `s ≤ 0` (apex-coincident /
///   wrong-nappe).
fn project_onto_cone_section(
    p: Point3,
    apex: Point3,
    axis_dir: Vector3,
    half_angle: f64,
    plane_n: Vector3,
    plane_d: f64,
) -> Result<Point3, Stage4InvalidReason> {
    let ap = apex.as_array();
    let a_hat = normalize3(axis_dir.as_array());
    let n = normalize3(plane_n.as_array());
    // The plane offset `d` must be expressed for the UNIT normal `n`. Stored
    // `Surface::Plane` normals in the corpus are already unit, but normalize the
    // offset defensively against the same scale used for `n` (same pattern as
    // `project_onto_ellipse_via_cylinder`).
    let n_raw = plane_n.as_array();
    let n_len = (n_raw[0] * n_raw[0] + n_raw[1] * n_raw[1] + n_raw[2] * n_raw[2]).sqrt();
    let d = if n_len < cad_primitives::TAU_WORK {
        plane_d
    } else {
        plane_d / n_len
    };
    let x = p.as_array();

    let w = [x[0] - ap[0], x[1] - ap[1], x[2] - ap[2]];
    let axial = w[0] * a_hat[0] + w[1] * a_hat[1] + w[2] * a_hat[2];
    let radial = [
        w[0] - axial * a_hat[0],
        w[1] - axial * a_hat[1],
        w[2] - axial * a_hat[2],
    ];
    let rho = (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
    if rho < cad_primitives::MIN_FEATURE_SIZE {
        return Err(Stage4InvalidReason::OnAxis);
    }
    let rdir = [radial[0] / rho, radial[1] / rho, radial[2] / rho];

    // Nappe sign from the axial component; the upper nappe (axial ≥ 0) uses
    // `+cosα·â`, the lower (`axial < 0`) uses `−cosα·â`. ρ ≥ MIN_FEATURE_SIZE so
    // the point is genuinely off-axis; the `|n·g|` / `s ≤ 0` guards below catch
    // any apex-plane degeneracy.
    let nappe = if axial < 0.0 { -1.0 } else { 1.0 };
    let (ca, sa) = (half_angle.cos(), half_angle.sin());
    // Unit generator at `p`'s azimuth (|g| = 1 by construction).
    let g = [
        nappe * ca * a_hat[0] + sa * rdir[0],
        nappe * ca * a_hat[1] + sa * rdir[1],
        nappe * ca * a_hat[2] + sa * rdir[2],
    ];

    let n_dot_g = n[0] * g[0] + n[1] * g[1] + n[2] * g[2];
    if n_dot_g.abs() < cad_primitives::MIN_FEATURE_SIZE {
        // Generator parallel to the plane: the asymptotic / parabola-tail
        // direction — out of scope (spec §6). Loud STOP rather than dividing by
        // ~0.
        return Err(Stage4InvalidReason::LocalRefinementRequired);
    }
    let n_dot_apex = n[0] * ap[0] + n[1] * ap[1] + n[2] * ap[2];
    let s = -(n_dot_apex + d) / n_dot_g;
    if s <= 0.0 {
        // Apex-coincident / wrong-nappe: the generator pierces the plane at or
        // behind the apex — out of scope. Loud STOP.
        return Err(Stage4InvalidReason::LocalRefinementRequired);
    }
    Ok(Point3::new(
        ap[0] + s * g[0],
        ap[1] + s * g[1],
        ap[2] + s * g[2],
    ))
}

/// PR-YR21 (spec §3.3): derive a cone's Stage-1 chord budget
/// `cone_chord_bound(height, half_angle)` from the cone OWNER's rim
/// `Curve::Circle`, using the SAME height derivation as `cone_chord_tol_for_owner`
/// / `tol_for`: `height = |(rim_center − apex)·â|`. A cone owner with no rim
/// Circle is a producer fault → `None` (the caller raises a loud STOP; NEVER a
/// `TAU_WORK` default for a curved relocation — P10).
fn cone_chord_budget_from_owner(
    apex: Point3,
    axis_dir: Vector3,
    half_angle: f64,
    owner: &BRep,
) -> Option<f64> {
    let au = normalize3(axis_dir.as_array());
    let ap = apex.as_array();
    for f in owner.faces() {
        if let Surface::Cone { .. } = f.surface {
            for &e_idx in &f.outer_loop {
                if let Curve::Circle { center, .. } = owner.edges()[e_idx as usize].curve {
                    let c = center.as_array();
                    let height =
                        ((c[0] - ap[0]) * au[0] + (c[1] - ap[1]) * au[1] + (c[2] - ap[2]) * au[2])
                            .abs();
                    return Some(cone_chord_bound(height, half_angle));
                }
            }
        }
    }
    None
}

/// PR-YR21 (spec §3.1/§4): the on-both-surfaces residual `max(cone radial,
/// plane)` of `pt` to an exact `cone ∩ plane` ellipse, recomputed from the
/// stored cone/plane. The cone analog of [`ellipse_residual`]. Cone radial
/// residual `|ρ − |axial|·tanα|` + plane residual `|n·x + d|` (plane offset
/// normalized to the unit normal).
fn cone_ellipse_residual(pt: Point3, cer: &ConeEllipseReloc) -> f64 {
    cone_plane_residual(
        pt,
        cer.apex,
        cer.axis_dir,
        cer.half_angle,
        cer.plane_n,
        cer.plane_d,
    )
}

/// PR-YR22: the on-both-surfaces residual `max(cone radial, plane)` of `pt` to an
/// exact `cone ∩ plane` section, recomputed from the stored cone/plane. Cone
/// radial residual `|ρ − |axial|·tanα|` + plane residual `|n·x + d|` (plane
/// offset normalized to the unit normal). Shared by [`cone_ellipse_residual`]
/// (ellipse) and the Stage-4 parabola loop — the conic TYPE does not change this
/// cone+plane residual (it only depends on the two surfaces, not the section
/// shape), so both call it (spec §3.1/§4).
fn cone_plane_residual(
    pt: Point3,
    apex: Point3,
    axis_dir: Vector3,
    half_angle: f64,
    plane_n: Vector3,
    plane_d: f64,
) -> f64 {
    let ap = apex.as_array();
    let a_hat = normalize3(axis_dir.as_array());
    let x = pt.as_array();
    let w = [x[0] - ap[0], x[1] - ap[1], x[2] - ap[2]];
    let axial = w[0] * a_hat[0] + w[1] * a_hat[1] + w[2] * a_hat[2];
    let radial = [
        w[0] - axial * a_hat[0],
        w[1] - axial * a_hat[1],
        w[2] - axial * a_hat[2],
    ];
    let rho = (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
    let cone_res = (rho - axial.abs() * half_angle.tan()).abs();

    let n_raw = plane_n.as_array();
    let n_len = (n_raw[0] * n_raw[0] + n_raw[1] * n_raw[1] + n_raw[2] * n_raw[2]).sqrt();
    let (n, d) = if n_len < cad_primitives::TAU_WORK {
        (n_raw, plane_d)
    } else {
        (
            [n_raw[0] / n_len, n_raw[1] / n_len, n_raw[2] / n_len],
            plane_d / n_len,
        )
    };
    let plane_res = (x[0] * n[0] + x[1] * n[1] + x[2] * n[2] + d).abs();
    cone_res.max(plane_res)
}

/// PR-YR11 (spec §4): the on-both-surfaces residual `max(|dist(x,axis)−r|,
/// |n·x+d|)` of `pt` to an exact oblique ellipse (cylinder ∩ plane). Matches the
/// RED Oracle-1 contract. The plane offset is normalized to the unit normal.
fn ellipse_residual(pt: Point3, er: &EllipseReloc) -> f64 {
    let q = er.axis_point.as_array();
    let a_hat = normalize3(er.axis_dir.as_array());
    let x = pt.as_array();
    let w = [x[0] - q[0], x[1] - q[1], x[2] - q[2]];
    let along = w[0] * a_hat[0] + w[1] * a_hat[1] + w[2] * a_hat[2];
    let radial = [
        w[0] - along * a_hat[0],
        w[1] - along * a_hat[1],
        w[2] - along * a_hat[2],
    ];
    let dist = (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
    let radial_res = (dist - er.radius).abs();

    let n_raw = er.plane_n.as_array();
    let n_len = (n_raw[0] * n_raw[0] + n_raw[1] * n_raw[1] + n_raw[2] * n_raw[2]).sqrt();
    let (n, d) = if n_len < cad_primitives::TAU_WORK {
        (n_raw, er.plane_d)
    } else {
        (
            [n_raw[0] / n_len, n_raw[1] / n_len, n_raw[2] / n_len],
            er.plane_d / n_len,
        )
    };
    let plane_res = (x[0] * n[0] + x[1] * n[1] + x[2] * n[2] + d).abs();
    radial_res.max(plane_res)
}

/// PR-YR10 (spec §4.4): the explicit Stage-4 watertightness gate (§4.4.3).
/// Every directed half-edge `(a, b)` must have exactly one opposite `(b, a)`
/// (a watertight 2-manifold), and each connected shell must be a closed
/// orientable 2-manifold with Euler characteristic
/// `χ = V − E + F = 2 − 2g` for genus `g ≥ 0` (χ even and ≤ 2); odd χ or
/// χ > 2 is impossible for such a shell and is rejected. Returns
/// `Err(NonManifoldOutput)` on failure.
/// KV9-F1 Increment 0c probe (kept env-gated, like the Stage-4 probes): name
/// the specific non-manifold gate that fired via `NONMANIFOLD_SITE_PROBE` so a
/// `NonManifoldOutput` wall self-localizes, then construct the error.
fn non_manifold_at(site: &str, detail: std::fmt::Arguments<'_>) -> YangError {
    if std::env::var("NONMANIFOLD_SITE_PROBE").is_ok() {
        eprintln!("NONMANIFOLD_SITE_PROBE {site}: {detail}");
    }
    YangError::NonManifoldOutput
}

fn check_watertight_2manifold(mesh: &Mesh) -> Result<(), YangError> {
    use std::collections::{BTreeMap, BTreeSet};
    // Directed half-edge multiset: every (a,b) must be paired by one (b,a).
    let mut dir: BTreeMap<(u32, u32), i32> = BTreeMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            *dir.entry((tri[i], tri[j])).or_insert(0) += 1;
        }
    }
    for (&(s, e), &fwd) in &dir {
        let rev = dir.get(&(e, s)).copied().unwrap_or(0);
        if fwd != rev {
            if std::env::var("NONMANIFOLD_SITE_PROBE").is_ok() {
                eprintln!(
                    "NONMANIFOLD_SITE_PROBE s4-halfedge-pairing coords: \
                     v{s}={:?} v{e}={:?}",
                    mesh.verts[s as usize], mesh.verts[e as usize]
                );
            }
            return Err(non_manifold_at(
                "s4-halfedge-pairing",
                format_args!("edge ({s},{e}) fwd={fwd} rev={rev}"),
            ));
        }
    }

    // Euler χ = 2 − 2g per connected shell (g ≥ 0). Connectivity via undirected
    // edges; the whole mesh is a union of disjoint closed orientable shells,
    // each of which has χ = 2 − 2g (sphere g=0 / through-hole g=1 / …).
    let n_verts = mesh.num_verts();
    if n_verts == 0 {
        return Ok(());
    }
    // Union-find over vertices through triangle edges.
    let mut parent: Vec<u32> = (0..n_verts as u32).collect();
    fn find(parent: &mut [u32], x: u32) -> u32 {
        let mut r = x;
        while parent[r as usize] != r {
            r = parent[r as usize];
        }
        // Path compression.
        let mut cur = x;
        while parent[cur as usize] != r {
            let next = parent[cur as usize];
            parent[cur as usize] = r;
            cur = next;
        }
        r
    }
    let union = |parent: &mut Vec<u32>, a: u32, b: u32| {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[ra as usize] = rb;
        }
    };
    for tri in &mesh.tris {
        union(&mut parent, tri[0], tri[1]);
        union(&mut parent, tri[1], tri[2]);
    }
    // Per-shell V, E (undirected), F.
    let mut shell_v: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    let mut shell_e: BTreeMap<u32, BTreeSet<(u32, u32)>> = BTreeMap::new();
    let mut shell_f: BTreeMap<u32, i64> = BTreeMap::new();
    for tri in &mesh.tris {
        let root = find(&mut parent, tri[0]);
        let v_set = shell_v.entry(root).or_default();
        for &vi in tri {
            v_set.insert(vi);
        }
        let e_set = shell_e.entry(root).or_default();
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (tri[i], tri[j]);
            e_set.insert(if a < b { (a, b) } else { (b, a) });
        }
        *shell_f.entry(root).or_insert(0) += 1;
    }
    for (root, v_set) in &shell_v {
        let v = v_set.len() as i64;
        let e = shell_e.get(root).map(|s| s.len()).unwrap_or(0) as i64;
        let f = shell_f.get(root).copied().unwrap_or(0);
        let chi = v - e + f;
        // A closed orientable 2-manifold shell has χ = 2 − 2g for integer genus
        // g ≥ 0, so χ is EVEN and ≤ 2. Accept any such χ (sphere χ=2 / g=0;
        // through-hole χ=0 / g=1; …). Reject odd χ or χ > 2 — impossible for a
        // closed orientable manifold → a real defect (NOT a tolerance/fallback
        // relaxation; P9/P10).
        if chi > 2 || chi.rem_euclid(2) != 0 {
            return Err(non_manifold_at(
                "s4-shell-euler",
                format_args!("shell root {root} chi={chi} v={v} e={e} f={f}"),
            ));
        }
    }
    Ok(())
}

// =========================================================================
// PR-YR9 (P3) — Stage 3: analytical SSI refinement of intersection edges
// =========================================================================

/// PR-YR9: convert a yang `Surface` into the analytical `ssi_rs::QuadricSurface`
/// for Stage-3 SSI (spec §5.2).
///
/// `Surface::Plane { normal, d }` uses the convention `n·x + d = 0`, while
/// `QuadricSurface::Plane` is `n·(x − point) = 0`, so a point on the plane is
/// `point = -d · n` (with `n` the stored unit normal). `Cylinder`, `Sphere`,
/// and `Cone` map field-for-field (PR-YR15 wires `Sphere`, enabling the exact
/// `plane ∩ sphere` great-circle rim; PR-YR17 wires `Cone`, enabling the exact
/// `plane ∩ cone` perpendicular-cut `Circle` rim via the `ssi_rs` `plane_cone`
/// C1 branch).
fn surface_to_quadric(s: Surface) -> Result<ssi_rs::QuadricSurface, SsiRefinementError> {
    match s {
        Surface::Plane { normal, d } => {
            let n = normal.as_array();
            Ok(ssi_rs::QuadricSurface::Plane {
                point: Point3::new(-d * n[0], -d * n[1], -d * n[2]),
                normal,
            })
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => Ok(ssi_rs::QuadricSurface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        }),
        Surface::Sphere { center, radius } => Ok(ssi_rs::QuadricSurface::Sphere { center, radius }),
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => Ok(ssi_rs::QuadricSurface::Cone {
            apex,
            axis_dir,
            half_angle,
        }),
        // A torus is a DEGREE-4 surface, not a quadric — its SSI refinement is
        // out of the quadric-solver vocabulary (KV6d boolean increment).
        Surface::Torus { .. } => Err(SsiRefinementError::UnsupportedSurfaceForSsi),
    }
}

/// PR-YR9: convert an `ssi_rs::SsiCurve` into a yang `Curve` (spec §5.3).
/// `Circle`/`Ellipse` map field-for-field; `Line` becomes `LineSegment`
/// (the edge's endpoints trim it). `Parabola`/`Hyperbola` cannot arise for the
/// Cylinder∩Plane pair and reject loudly (P9, defensive).
fn ssi_curve_to_curve(c: ssi_rs::SsiCurve) -> Result<Curve, SsiRefinementError> {
    match c {
        ssi_rs::SsiCurve::Circle {
            center,
            normal,
            radius,
        } => Ok(Curve::Circle {
            center,
            normal,
            radius,
        }),
        ssi_rs::SsiCurve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => Ok(Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        }),
        ssi_rs::SsiCurve::Line { .. } => Ok(Curve::LineSegment),
        // PR-YR22: the θ=α cone∩plane section is a Parabola (the single-candidate
        // conic). Map field-for-field.
        ssi_rs::SsiCurve::Parabola {
            vertex,
            normal,
            axis_dir,
            focal_length,
        } => Ok(Curve::Parabola {
            vertex,
            normal,
            axis_dir,
            focal_length,
        }),
        // PR-YR23: the axis-parallel (HYPE) cone∩plane section returns TWO
        // Hyperbola candidates (one per nappe). Map field-for-field; the
        // two-branch selection falls out of `curve_contains_point`'s `u > 0`
        // discriminator in `build_intersection_curves`.
        ssi_rs::SsiCurve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } => Ok(Curve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        }),
    }
}

/// PR-YR9: implicit on-curve test (spec §5.4) — does point `p` lie within `tol`
/// of curve `c`? No parameter solving; uses the curve's implicit residual.
/// `tol` is supplied by the caller (the Stage-1 chord bound `d_ε`); no ad-hoc
/// epsilon is introduced. `Parabola`/`Hyperbola` always return `false`.
///
/// PR-YR19 (spec §2/§4): `source_radius` carries the originating sphere radius
/// `R` for a sphere section `Circle`, so the in-plane RADIAL band is scaled by
/// the propagated factor `(R / r_circle)` (the projection of the surface-normal
/// chord error `d_ε` onto the section plane — see spec §2's
/// `dr ≈ (R/r_c)·d_sphere`). The AXIAL (out-of-plane) band stays the unscaled
/// `tol` (the cut plane is exact). `source_radius = None` (every non-sphere
/// path: cylinder / cone / plane) is BYTE-IDENTICAL to the old flat-`tol`
/// behavior. A near-tangent section (`r_circle ≤ MIN_FEATURE_SIZE`) fails closed
/// (keeps the unscaled band) so the factor cannot blow up.
fn curve_contains_point(
    c: &ssi_rs::SsiCurve,
    p: Point3,
    tol: f64,
    source_radius: Option<f64>,
) -> bool {
    let x = p.as_array();
    match c {
        ssi_rs::SsiCurve::Circle {
            center,
            normal,
            radius,
        } => {
            let n = normalize3(normal.as_array());
            let cc = center.as_array();
            let w = [x[0] - cc[0], x[1] - cc[1], x[2] - cc[2]];
            let axial = w[0] * n[0] + w[1] * n[1] + w[2] * n[2];
            let radial_vec = [
                w[0] - axial * n[0],
                w[1] - axial * n[1],
                w[2] - axial * n[2],
            ];
            let radial = (radial_vec[0] * radial_vec[0]
                + radial_vec[1] * radial_vec[1]
                + radial_vec[2] * radial_vec[2])
                .sqrt();
            let radial_tol = match source_radius {
                Some(big_r) if *radius > cad_primitives::MIN_FEATURE_SIZE => {
                    (big_r / *radius) * tol
                }
                _ => tol,
            };
            axial.abs() <= tol && (radial - radius).abs() <= radial_tol
        }
        ssi_rs::SsiCurve::Line { point, dir } => {
            let d = normalize3(dir.as_array());
            let pt = point.as_array();
            let w = [x[0] - pt[0], x[1] - pt[1], x[2] - pt[2]];
            let along = w[0] * d[0] + w[1] * d[1] + w[2] * d[2];
            let perp = [
                w[0] - along * d[0],
                w[1] - along * d[1],
                w[2] - along * d[2],
            ];
            (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt() <= tol
        }
        ssi_rs::SsiCurve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            let n = normalize3(normal.as_array());
            let maj = normalize3(major_axis.as_array());
            let min_axis = [
                n[1] * maj[2] - n[2] * maj[1],
                n[2] * maj[0] - n[0] * maj[2],
                n[0] * maj[1] - n[1] * maj[0],
            ];
            let cc = center.as_array();
            let w = [x[0] - cc[0], x[1] - cc[1], x[2] - cc[2]];
            let out_of_plane = w[0] * n[0] + w[1] * n[1] + w[2] * n[2];
            if out_of_plane.abs() > tol {
                return false;
            }
            let u = w[0] * maj[0] + w[1] * maj[1] + w[2] * maj[2];
            let v = w[0] * min_axis[0] + w[1] * min_axis[1] + w[2] * min_axis[2];
            let residual = ((u / major_radius).powi(2) + (v / minor_radius).powi(2)).sqrt() - 1.0;
            residual.abs() * major_radius.min(*minor_radius) <= tol
        }
        ssi_rs::SsiCurve::Parabola {
            vertex,
            normal,
            axis_dir,
            focal_length,
        } => {
            // PR-YR22: in-plane implicit membership `y² = 4f·x` for the θ=α
            // cone∩plane parabola. Out-of-plane reject first (the cut plane is
            // exact), then the in-plane relation.
            let n = normalize3(normal.as_array());
            let ax = normalize3(axis_dir.as_array());
            let conj = [
                n[1] * ax[2] - n[2] * ax[1],
                n[2] * ax[0] - n[0] * ax[2],
                n[0] * ax[1] - n[1] * ax[0],
            ];
            let vtx = vertex.as_array();
            let w = [x[0] - vtx[0], x[1] - vtx[1], x[2] - vtx[2]];
            let out_of_plane = w[0] * n[0] + w[1] * n[1] + w[2] * n[2];
            if out_of_plane.abs() > tol {
                return false;
            }
            let px = w[0] * ax[0] + w[1] * ax[1] + w[2] * ax[2];
            let py = w[0] * conj[0] + w[1] * conj[1] + w[2] * conj[2];
            // The implicit residual `y² − 4f·x` has units length². Convert it to
            // a perpendicular distance (length) by dividing by the in-plane
            // gradient magnitude `|∇(y²−4f·x)| = |(−4f, 2y)| = 2√(4f²+y²)` —
            // the parabola analog of the Ellipse arm's residual→length scaling.
            // Compare that geometric residual against the cone chord band `tol`.
            let implicit = (py * py - 4.0 * focal_length * px).abs();
            let grad = 2.0 * (4.0 * focal_length * focal_length + py * py).sqrt();
            let geo_res = if grad > cad_primitives::MIN_FEATURE_SIZE {
                implicit / grad
            } else {
                implicit
            };
            geo_res <= tol
        }
        ssi_rs::SsiCurve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } => {
            // PR-YR23: in-plane implicit membership `(u/a)² − (v/b)² = 1` for the
            // axis-parallel (HYPE) cone∩plane hyperbola, AND the branch
            // discriminator `u > 0` (the OTHER nappe's branch — opposite
            // major_axis — gives u < 0 here and is rejected, so matched == 1).
            // Out-of-plane reject first (the cut plane is exact), then the
            // in-plane relation + branch test.
            let n = normalize3(normal.as_array());
            let maj = normalize3(major_axis.as_array());
            let conj = [
                n[1] * maj[2] - n[2] * maj[1],
                n[2] * maj[0] - n[0] * maj[2],
                n[0] * maj[1] - n[1] * maj[0],
            ];
            let cc = center.as_array();
            let w = [x[0] - cc[0], x[1] - cc[1], x[2] - cc[2]];
            let out_of_plane = w[0] * n[0] + w[1] * n[1] + w[2] * n[2];
            if out_of_plane.abs() > tol {
                return false;
            }
            let a = *semi_transverse;
            let b = *semi_conjugate;
            let u = w[0] * maj[0] + w[1] * maj[1] + w[2] * maj[2];
            let v = w[0] * conj[0] + w[1] * conj[1] + w[2] * conj[2];
            // The implicit residual `F = (u/a)² − (v/b)² − 1` is dimensionless.
            // Convert it to a perpendicular distance (length) by dividing by the
            // in-plane gradient magnitude `|∇F| = |(2u/a², −2v/b²)|` — the
            // hyperbola analog of the Ellipse/Parabola arms' residual→length
            // scaling (NOT a flat widening). Compare against the cone chord band
            // `tol`.
            let implicit = ((u / a).powi(2) - (v / b).powi(2) - 1.0).abs();
            let gu = 2.0 * u / (a * a);
            let gv = 2.0 * v / (b * b);
            let grad = (gu * gu + gv * gv).sqrt();
            let geo_res = if grad > cad_primitives::MIN_FEATURE_SIZE {
                implicit / grad
            } else {
                implicit
            };
            geo_res <= tol && u > 0.0
        }
    }
}

/// Selection tolerance for a CYLINDER-owning intersection edge: the cylinder
/// input's Stage-1 chord bound via `curved_chord_bound` (the SINGLE source for
/// the cylinder band). A cylinder-bearing input with NO circle rims is a
/// producer fault → LOUD `AmbiguousCurve { matched: 0 }` (never silently
/// default to `TAU_WORK` for a curved selection). Factored out of
/// `build_intersection_curves` (PR-YR15) so the sphere arm can sit beside it
/// without duplicating the producer-fault path; sphere uses its OWN
/// `sphere_chord_bound` (2r√3), not this cylinder/rim-AABB band.
fn chord_tol_for_curved_owner(
    input: InputId,
    a: &BRep,
    b: &BRep,
    candidates: usize,
    edge: (u32, u32),
) -> Result<f64, YangError> {
    let owner = match input {
        InputId::A => a,
        InputId::B => b,
    };
    match curved_chord_bound(owner.edges()) {
        Some(t) => Ok(t),
        None => Err(YangError::SsiRefinementFailed {
            edge,
            reason: SsiRefinementError::AmbiguousCurve {
                candidates,
                matched: 0,
            },
        }),
    }
}

/// PR-YR17: selection tolerance for a CONE-owning intersection edge. A cone
/// edge is the perpendicular `plane ∩ cone` cut whose returned `ssi_rs` curve is
/// the exact rim `Circle`; the mesh endpoints sit on the cone's Stage-1 chord
/// approximation, off that exact circle by up to the cone's OWN chord bound
/// `cone_chord_bound(height, half_angle)` (A14.3 single source — the SAME bound
/// Stage 1 guarantees, NOT tolerance widening). `Surface::Cone` carries no
/// height, so it is derived from the cone owner's rim `Curve::Circle` edge in
/// the cone face's outer loop exactly as the Stage-1 pre-pass / `tol_for` do:
/// `height = |(rim_center − apex)·â|`. A cone-bearing input with NO rim Circle
/// is a producer fault → LOUD `AmbiguousCurve { matched: 0 }` (never silently
/// default to `TAU_WORK` for a curved selection), mirroring
/// `chord_tol_for_curved_owner`.
fn cone_chord_tol_for_owner(
    cone_surface: Surface,
    input: InputId,
    a: &BRep,
    b: &BRep,
    candidates: usize,
    edge: (u32, u32),
) -> Result<f64, YangError> {
    let Surface::Cone {
        apex,
        axis_dir,
        half_angle,
    } = cone_surface
    else {
        return Err(YangError::SsiRefinementFailed {
            edge,
            reason: SsiRefinementError::AmbiguousCurve {
                candidates,
                matched: 0,
            },
        });
    };
    let owner = match input {
        InputId::A => a,
        InputId::B => b,
    };
    let au = normalize3(axis_dir.as_array());
    let ap = apex.as_array();
    for f in owner.faces() {
        if let Surface::Cone { .. } = f.surface {
            for &e_idx in &f.outer_loop {
                if let Curve::Circle { center, .. } = owner.edges()[e_idx as usize].curve {
                    let c = center.as_array();
                    let height =
                        ((c[0] - ap[0]) * au[0] + (c[1] - ap[1]) * au[1] + (c[2] - ap[2]) * au[2])
                            .abs();
                    return Ok(cone_chord_bound(height, half_angle));
                }
            }
        }
    }
    Err(YangError::SsiRefinementFailed {
        edge,
        reason: SsiRefinementError::AmbiguousCurve {
            candidates,
            matched: 0,
        },
    })
}

/// PR-YR9: build the EXACT analytical `Curve` for each output intersection edge
/// (spec §5.5). An intersection edge is an undirected mesh boundary edge whose
/// incidence list has EXACTLY TWO entries with DIFFERENT `InputId` — it lies on
/// one surface of input A and one of input B.
///
/// For each such edge: convert both surfaces to `QuadricSurface`, call
/// `ssi_rs::intersect`, derive the selection tolerance `tol` from the
/// CURVED-owning input's Stage-1 chord bound (cylinder via `curved_chord_bound`,
/// sphere via `sphere_chord_bound` — PR-YR15, cone via `cone_chord_tol_for_owner`
/// — PR-YR17), and select the UNIQUE returned curve passing through BOTH mesh
/// endpoints within `tol`. `matched != 1` is a
/// P9/P10 LOUD stop (`AmbiguousCurve`) — never a silent polyline fallback.
///
/// Plane∩Plane edges yield a `Line` → `LineSegment` (equal to the caller's
/// fallback, so the planar corpus is unchanged); their `tol` is `TAU_WORK`
/// (a plane∩plane line has zero chord error).
fn build_intersection_curves(
    incidence: &std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    mesh: &Mesh,
    a: &BRep,
    b: &BRep,
) -> Result<std::collections::BTreeMap<(u32, u32), Curve>, YangError> {
    let mut out: std::collections::BTreeMap<(u32, u32), Curve> = std::collections::BTreeMap::new();
    for (&(s, e), entries) in incidence {
        if entries.len() != 2 {
            continue;
        }
        let (input0, surf0) = entries[0];
        let (input1, surf1) = entries[1];
        if input0 == input1 {
            continue;
        }

        // Selection tolerance: the Stage-1 chord bound of the CURVED-owning
        // input (A14.3 single source). The mesh edge endpoints sit on the
        // curved surface's Stage-1 chord approximation, off the EXACT analytic
        // curve by up to that surface's own chord bound — so the on-curve test
        // must admit them at that bound (the SAME bound Stage 1 guarantees, NOT
        // tolerance widening). Plane∩Plane → no curved surface → TAU_WORK
        // (zero chord error). PR-YR15 extends the cylinder-only logic to a
        // SPHERE edge: a sphere uses its OWN bound `sphere_chord_bound(radius)`
        // (2r√3), NOT the rim-AABB `curved_chord_bound` (2r√2, which would
        // underestimate — I-sphere-band).
        //
        // PR-YR18: `tol` is computed FIRST (before `surface_to_quadric` /
        // `ssi_rs::intersect`) so it can drive the on-both-surfaces gate below.
        // The producer-fault helpers' `candidates` argument is diagnostic-only
        // (untested); in this pre-intersect position we have no `returned.len()`
        // yet, so we pass `0`.
        // PR-YR19: alongside `tol`, derive `source_radius` — `Some(R)` ONLY for
        // a sphere-owning edge, so `curve_contains_point` scales the section
        // `Circle`'s in-plane radial band by the propagated factor `(R/r_c)`
        // (spec §2). Cylinder / cone / plane arms keep `None` (byte-identical to
        // the pre-YR19 flat-band membership test).
        let (tol, source_radius): (f64, Option<f64>) = if matches!(surf0, Surface::Cylinder { .. })
            && matches!(surf1, Surface::Cylinder { .. })
        {
            // PR-KV9: cylinder × cylinder edge — the arrangement vertex sits
            // on the crossing of BOTH meshes' facet chords, off the exact
            // curve by up to the SUM of the two inputs' own Stage-1 chord
            // bounds (each endpoint is within its owner's band of its
            // surface; the crossing inherits both). The sum is the derived
            // combined bound, not widening.
            (
                chord_tol_for_curved_owner(input0, a, b, 0, (s, e))?
                    + chord_tol_for_curved_owner(input1, a, b, 0, (s, e))?,
                None,
            )
        } else if matches!(surf0, Surface::Cylinder { .. }) {
            (chord_tol_for_curved_owner(input0, a, b, 0, (s, e))?, None)
        } else if matches!(surf1, Surface::Cylinder { .. }) {
            (chord_tol_for_curved_owner(input1, a, b, 0, (s, e))?, None)
        } else if let Surface::Sphere { radius, .. } = surf0 {
            (sphere_chord_bound(radius), Some(radius))
        } else if let Surface::Sphere { radius, .. } = surf1 {
            (sphere_chord_bound(radius), Some(radius))
        } else if matches!(surf0, Surface::Cone { .. }) {
            (
                cone_chord_tol_for_owner(surf0, input0, a, b, 0, (s, e))?,
                None,
            )
        } else if matches!(surf1, Surface::Cone { .. }) {
            (
                cone_chord_tol_for_owner(surf1, input1, a, b, 0, (s, e))?,
                None,
            )
        } else {
            (cad_primitives::TAU_WORK, None)
        };

        let p_s = mesh.verts[s as usize];
        let p_e = mesh.verts[e as usize];

        // PR-YR18 (spec §2/§3): on-both-surfaces gate. An edge handed to
        // `ssi_rs::intersect` as a `(surf0, surf1)` intersection edge must have
        // BOTH endpoints on BOTH attributed surfaces within the edge's Stage-1
        // chord band `tol`. `compute_phase_a` pushes a patch's single inherited
        // surface onto every boundary edge of the patch cycle, so a seam edge
        // can be tagged `(surfA, surfB)` while one endpoint is genuinely off one
        // surface — that is a single-surface internal edge, NOT a true
        // intersection edge. Skip it (→ `Curve::LineSegment` fallback in
        // `emit_topology`) before it reaches the SSI. Reuses the SAME `tol` the
        // selection uses (no widening): the intersection curve lies ON both
        // surfaces, so every edge that currently selects `matched == 1`
        // necessarily passes this gate — it can only reclassify edges that today
        // raise `AmbiguousCurve` with an endpoint off a surface beyond `tol`.
        let on_both = |pt: Point3| -> Result<bool, YangError> {
            Ok(signed_distance_to_surface(surf0, pt)?.abs() <= tol
                && signed_distance_to_surface(surf1, pt)?.abs() <= tol)
        };
        if !(on_both(p_s)? && on_both(p_e)?) {
            if let Ok(list) = std::env::var("YANG_V_PROBE") {
                if list
                    .split(',')
                    .any(|t| t.trim().parse::<u32>() == Ok(s) || t.trim().parse::<u32>() == Ok(e))
                {
                    eprintln!(
                        "YANG_V_PROBE on-both gate SKIP edge ({s},{e}) tol={tol:.3e} \
                         surf0={surf0:?} surf1={surf1:?} \
                         d_s=({:.3e},{:.3e}) d_e=({:.3e},{:.3e})",
                        signed_distance_to_surface(surf0, p_s)?.abs(),
                        signed_distance_to_surface(surf1, p_s)?.abs(),
                        signed_distance_to_surface(surf0, p_e)?.abs(),
                        signed_distance_to_surface(surf1, p_e)?.abs(),
                    );
                }
            }
            continue;
        }

        // Plane∩Plane: the curve is the unique line through the two (gate-
        // verified on-both-planes) endpoints — `Curve::LineSegment`, exact,
        // zero chord error. This short-circuit is byte-equivalent to the
        // SSI route for TRANSVERSAL planes (ssi returns the Line, which
        // maps to `LineSegment`) and is REQUIRED for the §4.5.5 coplanar
        // seams (PR-YR26): the boundary of a trimmed common planar surface
        // is an intersection curve between two COINCIDENT planes ("The
        // boundaries of the common surface are regarded as intersection
        // curves between the two models"), where `ssi_rs::intersect`
        // correctly refuses the parallel-plane pair (`DegenerateInput`) —
        // the curve comes from the 2D overlay, not from SSI.
        if matches!(surf0, Surface::Plane { .. }) && matches!(surf1, Surface::Plane { .. }) {
            out.insert((s, e), Curve::LineSegment);
            continue;
        }

        if cylinders_are_coincident(surf0, surf1, tol) {
            // PR-5: COINCIDENT cylinders (the §4.5.5 membrane analog for a coaxial
            // flange wall == gear bore, `err.waffle`). `ssi_rs::intersect`
            // correctly refuses an identical-quadric pair (`DegenerateInput`):
            // the two cylinders do not intersect transversally, so the edge
            // curve does NOT come from SSI but from the overlap boundary on the
            // shared cylinder — exactly the coincident-PLANE case above.
            //
            // Every such edge (rim, generator, or interior tessellation chord)
            // is left to the `Curve::LineSegment` fallback in `emit_topology`
            // (the mesh chord — the analog of the plane case handing every seam
            // edge a straight segment, refinement-free). Emitting a rim
            // `Curve::Circle` here would instead mark its endpoints for Stage-4
            // relocation onto the analytic circle, collapsing adjacent
            // membrane-region triangles (`DegenerateTriangle`): those vertices
            // already sit on the shared cylinder's exact chord rim (the lateral
            // tessellation put them there) and need no relocation. It is NEVER
            // sent to the degenerate SSI (which would be a loud `DegenerateInput`).
            continue;
        }

        // KV6d Tier B: a TORUS intersection edge is degree-4 — there is no
        // analytic SSI curve (`surface_to_quadric` refuses a torus). Leave it as
        // the `Curve::LineSegment` fallback (the `emit_topology` default); Stage
        // 4 relocates its endpoints onto the exact torus∩surface curve via the
        // implicit-pair Newton (`relocate_onto_implicit_pair`/`_triple`).
        if matches!(surf0, Surface::Torus { .. }) || matches!(surf1, Surface::Torus { .. }) {
            continue;
        }

        let q0 = surface_to_quadric(surf0).map_err(|reason| YangError::SsiRefinementFailed {
            edge: (s, e),
            reason,
        })?;
        let q1 = surface_to_quadric(surf1).map_err(|reason| YangError::SsiRefinementFailed {
            edge: (s, e),
            reason,
        })?;

        let returned =
            ssi_rs::intersect(&q0, &q1).map_err(|err| YangError::SsiRefinementFailed {
                edge: (s, e),
                reason: SsiRefinementError::IntersectFailed(err),
            })?;

        // PR-F3b: a `Line` candidate's membership band carries the propagated
        // factor `r/√(r² − d²)` (the radial chord deficit measured in the
        // cutting plane's in-plane metric) — same derivation as the PR-YR19
        // sphere section circle's `(R/r_c)` scaling. Conic candidates keep
        // the unscaled `tol` byte-for-byte.
        let line_amp = line_band_amplification(surf0, surf1);
        // PR-KV9: cylinder×cylinder pairs carry the per-point gradient
        // amplification (membership measured against the band intersection
        // of BOTH surfaces; diverges at surface tangency — the Steinmetz
        // crossing points — where the tangent-direction discriminator below
        // takes over).
        let cyl_pair: Option<((Point3, Vector3), (Point3, Vector3))> = match (surf0, surf1) {
            (
                Surface::Cylinder {
                    axis_point: p1,
                    axis_dir: d1,
                    ..
                },
                Surface::Cylinder {
                    axis_point: p2,
                    axis_dir: d2,
                    ..
                },
            ) => Some(((p1, d1), (p2, d2))),
            _ => None,
        };
        let point_tol = |x: Point3, curve: &ssi_rs::SsiCurve| -> f64 {
            match curve {
                ssi_rs::SsiCurve::Line { .. } => line_amp.map_or(tol, |a| a * tol),
                ssi_rs::SsiCurve::Ellipse { .. } => match cyl_pair {
                    Some((c1, c2)) => {
                        cyl_cyl_point_amplification(x, c1, c2).map_or(f64::INFINITY, |a| a * tol)
                    }
                    None => tol,
                },
                _ => tol,
            }
        };
        let mut matched_idx: Option<usize> = None;
        let mut matched = 0usize;
        for (i, curve) in returned.iter().enumerate() {
            if curve_contains_point(curve, p_s, point_tol(p_s, curve), source_radius)
                && curve_contains_point(curve, p_e, point_tol(p_e, curve), source_radius)
            {
                matched += 1;
                matched_idx = Some(i);
            }
        }

        // PR-KV9: tangent-direction discrimination for multi-matches. Two
        // curves through one region (the Steinmetz ellipses near their
        // crossing) CROSS transversally, so the mesh edge's direction
        // aligns with exactly one curve's tangent. Selected only with a
        // clear margin; otherwise the loud ambiguity stands (P9 — a
        // tie-break, never a band widening).
        if matched > 1 {
            let edge_dir = {
                let d = [p_e.x() - p_s.x(), p_e.y() - p_s.y(), p_e.z() - p_s.z()];
                normalize3(d)
            };
            let mid = Point3::new(
                (p_s.x() + p_e.x()) / 2.0,
                (p_s.y() + p_e.y()) / 2.0,
                (p_s.z() + p_e.z()) / 2.0,
            );
            let mut scored: Vec<(f64, usize)> = Vec::new();
            for (i, curve) in returned.iter().enumerate() {
                if !(curve_contains_point(curve, p_s, point_tol(p_s, curve), source_radius)
                    && curve_contains_point(curve, p_e, point_tol(p_e, curve), source_radius))
                {
                    continue;
                }
                if let Some(t) = curve_tangent_at(curve, mid) {
                    let c = (t[0] * edge_dir[0] + t[1] * edge_dir[1] + t[2] * edge_dir[2]).abs();
                    scored.push((c, i));
                }
            }
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            if scored.len() >= 2 && scored[0].0 > scored[1].0 + 0.1 {
                matched = 1;
                matched_idx = Some(scored[0].1);
            }
        }

        // R0072: POSITION tie-break for near-coincident PARALLEL-line matches.
        // A near-tangent `plane ∩ cylinder` secant returns two near-coincident
        // parallel generators; both pass `curve_contains_point` (the `line_amp`
        // near-tangency band inflation admits both), and the tangent pass above
        // cannot separate them — parallel lines share a direction, so the
        // cosine margin never fires. But the mesh edge lies on exactly ONE
        // generator, which is nearer to BOTH endpoints. Select the candidate
        // whose endpoint-distance interval lies strictly below every other
        // matched candidate's (`hi_w < lo_j ∀ j≠w`): the winner's WORST endpoint
        // still beats every rival's BEST, so the endpoints unambiguously lie on
        // it. Margin-free and scale-free. If the intervals overlap (generators
        // merged below mesh resolution — true-tangency territory), no candidate
        // qualifies and the loud `AmbiguousCurve` stands (P9 — a proximity
        // tie-break on geometry the on-both gate already verified, never a band
        // widening). Spec: `specs/yr_r0072_parallel_line_position_tiebreak.md`.
        if matched > 1 {
            // Collect the matched candidates that are lines, paired with their
            // `returned` index; bail if any matched candidate is NOT a line (a
            // mixed line/conic multi-match is not the parallel-generator case).
            let mut cands: Vec<(Point3, Vector3)> = Vec::new();
            let mut cand_idx: Vec<usize> = Vec::new();
            let mut all_matched_are_lines = true;
            for (i, curve) in returned.iter().enumerate() {
                if !(curve_contains_point(curve, p_s, point_tol(p_s, curve), source_radius)
                    && curve_contains_point(curve, p_e, point_tol(p_e, curve), source_radius))
                {
                    continue;
                }
                if let ssi_rs::SsiCurve::Line { point, dir } = curve {
                    cands.push((*point, *dir));
                    cand_idx.push(i);
                } else {
                    all_matched_are_lines = false;
                    break;
                }
            }
            if all_matched_are_lines && cands.len() == matched {
                if let Some(wk) = select_disjoint_parallel_line(&cands, p_s, p_e) {
                    matched = 1;
                    matched_idx = Some(cand_idx[wk]);
                }
            }
        }

        let idx = match (matched, matched_idx) {
            (1, Some(idx)) => idx,
            _ => {
                return Err(YangError::SsiRefinementFailed {
                    edge: (s, e),
                    reason: SsiRefinementError::AmbiguousCurve {
                        candidates: returned.len(),
                        matched,
                    },
                });
            }
        };
        let curve =
            ssi_curve_to_curve(returned[idx]).map_err(|reason| YangError::SsiRefinementFailed {
                edge: (s, e),
                reason,
            })?;
        out.insert((s, e), curve);
    }
    Ok(out)
}

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

// =========================================================================
// boolean() — PR-YR3 vertex provenance + PR-YR4 triangle attribution
// =========================================================================

/// Per-op orientation fix for a kept arrangement triangle, mirroring
/// Cherchi's `booleans.cpp` post-keep flip loops:
/// - Union (`boolUnion`) / Intersection (`boolIntersection`): no flip.
/// - Subtraction (`boolSubtraction`:1480-1483): flip kept tris NOT on
///   solid A's surface (`surface[t][0] != 1`) — the B-surface tris that
///   bound the carved cavity, whose outward normal must point into A.
/// - Xor (`boolXOR`:1506-1509): flip kept tris with any inside bit set
///   (`inside.count() > 0`).
fn flip_for_op(op: BoolOp, la: &LabeledArrangement, t: usize) -> bool {
    match op {
        BoolOp::Union | BoolOp::Intersect => false,
        BoolOp::Subtract => {
            // surface[t][0] set ⟺ solid 0 (A) is in the surface label list.
            let on_a = la.surface[t].iter().any(|&LaInputId(id)| id == 0);
            !on_a
        }
        BoolOp::Xor => la.inside[t].iter().any(|&b| b),
    }
}

/// PR-YR24: Stage-1 NEAR-coplanar input scan (PR-YR26: now the Stage-0
/// DETECTOR, no longer a hard gate).
///
/// Scans A-face × B-face pairs of the two input B-Reps (planar faces only,
/// while their surfaces are still symbolic `Surface::Plane`s — i.e. BEFORE
/// any mesh-level processing) and returns ALL cross pairs that are coplanar
/// within the sub-model-resolution band AND could actually interact
/// (overlapping AABBs), plus the first INTRA-solid pair (which remains the
/// loud unsupported residue).
///
/// **Why this scan exists.** Yang 2025 §4.5.5 requires coplanar face pairs
/// to be detected and resolved by a 2D Boolean at the B-Rep level BEFORE
/// mesh discretization ("it is necessary to check coplanar planes and
/// perform 2D Boolean operations before mesh discretizations",
/// `refs/text/yang2025_hybrid_boolean.txt:717-731`) — Stage 0, roadmap
/// milestone M8. Bit-EXACT coplanar overlaps that reach the arrangement
/// unhandled hit cherchi-rs's loud deferral (`CoplanarPairDeferred`,
/// deviation N17, `arrangements/soup.rs`). But f64 vertex construction
/// leaves femto-scale residuals on faces built on the SAME oblique sketch
/// plane (the KV4-F1 corpus family: R0029, F0016/18/19/21/25), so the EXACT
/// deferral does not catch them; the exact arrangement then faithfully
/// builds sub-f64-ulp sliver patches (all-LPI, all-border, width < 1 ulp)
/// whose in/out classification has no seedable ray origin
/// (`NoExplicitRayOrigin` — the C++ reference `booleans.cpp:504-575` would
/// exit there too). PR-YR24 converted both classes into the loud typed
/// `CoplanarFacesUnsupported` wall; PR-YR26 (M8 slice b) HANDLES the
/// cross-pair planar class via the §4.5.5 overlay (`stage0_preprocess`) and
/// keeps the wall only for the residue (intra pairs, unsupported face
/// shapes, multi-pair faces).
///
/// **The band.** For a candidate pair, with unit normals `n̂a`, `n̂b`
/// (orientation-aligned: `s = sign(n̂a·n̂b)`) and unit-normal plane offsets
/// `d̂ = d/‖n‖` (`n̂·x + d̂ = 0`):
///
/// ```text
/// scale = max |coordinate| over both faces' AABB corners
/// band  = max(TAU_MODEL, scale · TAU_WORK)
/// ```
///
/// and the pair is flagged iff ALL of:
/// 1. offset agreement:  |d̂a − s·d̂b| ≤ band
/// 2. parallel normals:  ‖n̂a × n̂b‖ · extent ≤ band, where `extent` is the
///    diagonal of the union of the two faces' AABBs (an angular tilt θ
///    displaces the planes by at most sin θ · extent over the region where
///    the faces could meet, so this bounds the true plane-to-plane gap by
///    2·band over that region)
/// 3. AABB overlap (each axis, inflated by band) — far-apart faces on the
///    same plane do not interact in the boolean and are NOT flagged
///    (over-deferral avoided).
///
/// Justification: `TAU_MODEL` (1e-7, absolute, governance A14) is the model
/// resolution — two parallel planes closer than `TAU_MODEL` are
/// sub-model-resolution and semantically the same plane (the R0029 family's
/// residuals are ~1e-13..1e-15 absolute at |coord| ~ 6e2, far inside the
/// band, while `MIN_FEATURE_SIZE` = 1e-6 guarantees genuinely distinct
/// model features sit OUTSIDE it). The `scale·TAU_WORK` term (relative
/// 1e-12 ≫ machine ε ≈ 2.2e-16) keeps the band above the f64
/// construction-noise floor for very large models where 1e-7 absolute
/// approaches the coordinate ulp; for |coord| < 1e5 it is inactive.
///
/// Conservative choices: face AABBs are taken over the loop edges'
/// START/END vertices (a curved rim's bulge is not included), which can
/// only UNDER-approximate the AABB — i.e. err toward NOT flagging; a missed
/// pair falls through to the existing loud downstream errors, never to a
/// silent wrong result. Non-planar surfaces are skipped (curved-curved
/// coplanarity is out of this gate's scope; the curved pipeline has its own
/// guards).
///
/// **Intra-solid pairs (the CHAINED KV4-F1 mechanism).** A solid that is
/// itself the output of an exact boolean re-creates near-incidences via
/// exact→f64 output rounding: the surviving A-side and B-side fragments of
/// one near-coplanar plane come back as faces of the SAME solid on planes a
/// few ulps apart (e.g. F0016's second union: operand A carries face pairs
/// with offset residual ~1.6e-16). The next boolean then builds the same
/// sub-ulp sliver patches. So the gate also scans A×A and B×B pairs — with
/// one crucial distinction: BIT-IDENTICAL intra-solid planes are benign (one
/// plane legitimately split into several faces, e.g. an annulus; cherchi's
/// N17 passes exact same-plane adjacency through) and are skipped; only
/// near-but-NOT-bit-identical intra pairs carry the femto signature. Cross
/// (A×B) pairs flag in BOTH cases — bit-exact A×B coplanarity is the
/// original M8 case.
///
/// Intra pairs use a DIFFERENT condition 3: the two fragments of a rounded
/// plane are usually disjoint in-plane regions that never overlap each
/// other, so the cross rule's mutual-overlap test can never fire. The
/// danger is contact by the OTHER solid: crossing both fragments creates
/// two cut lines a few ulps apart (verified on F0018), and even crossing
/// ONE fragment can cut through the rounded seam geometry the split left
/// behind (observed on F0025, where the other solid overlaps only one
/// fragment yet in/out still fails). AABB granularity cannot localize the
/// seam, so the conservative rule is: flag the intra pair iff the other
/// solid's whole-solid AABB overlaps EITHER fragment's AABB
/// (band-inflated). This over-defers a boolean that touches a femto-split
/// plane's region without actually reaching its seam — weighed and
/// accepted: a loud typed M8 deferral is strictly better than
/// `NoExplicitRayOrigin` (P9), and a boolean that stays clear of the
/// region entirely is still NOT flagged.
/// One near-coplanar CROSS (A-face × B-face) pair found by
/// [`scan_near_coplanar`], with the pair's detection `band`.
pub(crate) struct CrossCoplanarPair {
    pub(crate) face_a: usize,
    pub(crate) face_b: usize,
    pub(crate) band: f64,
}

/// Output of [`scan_near_coplanar`]: ALL cross pairs (PR-YR26 Stage-0
/// handles each via the §4.5.5 overlay) plus the FIRST intra-solid pair
/// (still the loud unsupported-residue error — the chained-output class).
pub(crate) struct CoplanarScan {
    pub(crate) cross: Vec<CrossCoplanarPair>,
    pub(crate) intra: Option<(InputId, usize, usize)>,
}

pub(crate) fn scan_near_coplanar(a: &BRep, b: &BRep) -> CoplanarScan {
    /// Per-face plane data: unit normal, unit-normal offset, loop-vertex
    /// AABB, plus the RAW (un-normalized) plane bits for the intra-solid
    /// bit-identical exclusion.
    struct FacePlane {
        n: [f64; 3],
        d: f64,
        lo: [f64; 3],
        hi: [f64; 3],
        raw_bits: [u64; 4],
    }

    fn collect(brep: &BRep) -> Vec<Option<FacePlane>> {
        brep.faces()
            .iter()
            .map(|f| {
                let Surface::Plane { normal, d } = f.surface else {
                    return None;
                };
                let na = normal.as_array();
                let len = (na[0] * na[0] + na[1] * na[1] + na[2] * na[2]).sqrt();
                if len < cad_primitives::MIN_FEATURE_SIZE {
                    // Degenerate normal — rejected loudly elsewhere
                    // (`DegenerateFace`); not this gate's job.
                    return None;
                }
                let n = [na[0] / len, na[1] / len, na[2] / len];
                let mut lo = [f64::INFINITY; 3];
                let mut hi = [f64::NEG_INFINITY; 3];
                for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
                    for &e in lp {
                        let Some(edge) = brep.edges().get(e as usize) else {
                            continue;
                        };
                        for vi in [edge.start, edge.end] {
                            let Some(v) = brep.vertices().get(vi as usize) else {
                                continue;
                            };
                            let p = v.point.as_array();
                            for k in 0..3 {
                                lo[k] = lo[k].min(p[k]);
                                hi[k] = hi[k].max(p[k]);
                            }
                        }
                        // A `Circle`/`Ellipse` loop edge's endpoints are only
                        // its seam — the swept curve reaches much further. A
                        // disc cap bounded by a single closed circle would
                        // otherwise get a single-POINT AABB (the seam), so a
                        // coplanar disc∩polygon pair is detected only when the
                        // seam happens to overlap the other face. Expand by the
                        // analytic circle box: `center ± r·√(1−n_k²)` per axis.
                        if let Curve::Circle {
                            center,
                            normal,
                            radius,
                        } = edge.curve
                        {
                            let c = center.as_array();
                            let nu = normalize3(normal.as_array());
                            for k in 0..3 {
                                let ext = radius * (1.0 - nu[k] * nu[k]).max(0.0).sqrt();
                                lo[k] = lo[k].min(c[k] - ext);
                                hi[k] = hi[k].max(c[k] + ext);
                            }
                        }
                    }
                }
                if !lo[0].is_finite() {
                    return None;
                }
                Some(FacePlane {
                    n,
                    d: d / len,
                    lo,
                    hi,
                    raw_bits: [
                        na[0].to_bits(),
                        na[1].to_bits(),
                        na[2].to_bits(),
                        d.to_bits(),
                    ],
                })
            })
            .collect()
    }

    /// Conditions 1 (offset agreement) + 2 (parallel normals) for one face
    /// pair; returns the pair's `band` when both hold. Condition 3 (which
    /// AABBs must overlap) differs between cross and intra pairs — see the
    /// scan loops below.
    fn near_coplanar_band(pa: &FacePlane, pb: &FacePlane) -> Option<f64> {
        // scale = max |coordinate| over both faces' AABB corners.
        let mut scale: f64 = 0.0;
        for p in [&pa.lo, &pa.hi, &pb.lo, &pb.hi] {
            for &c in p.iter() {
                scale = scale.max(c.abs());
            }
        }
        let band = cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK);

        // 1. Orientation-aligned offset agreement.
        let dot = pa.n[0] * pb.n[0] + pa.n[1] * pb.n[1] + pa.n[2] * pb.n[2];
        let s = if dot >= 0.0 { 1.0 } else { -1.0 };
        if (pa.d - s * pb.d).abs() > band {
            return None;
        }

        // 2. Parallel normals over the pair's geometric extent.
        let cross = [
            pa.n[1] * pb.n[2] - pa.n[2] * pb.n[1],
            pa.n[2] * pb.n[0] - pa.n[0] * pb.n[2],
            pa.n[0] * pb.n[1] - pa.n[1] * pb.n[0],
        ];
        let sin = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let mut ext2 = 0.0;
        for k in 0..3 {
            let e = pa.hi[k].max(pb.hi[k]) - pa.lo[k].min(pb.lo[k]);
            ext2 += e * e;
        }
        if sin * ext2.sqrt() > band {
            return None;
        }
        Some(band)
    }

    /// Band-inflated AABB overlap on every axis.
    fn aabbs_overlap(
        lo_a: &[f64; 3],
        hi_a: &[f64; 3],
        lo_b: &[f64; 3],
        hi_b: &[f64; 3],
        band: f64,
    ) -> bool {
        (0..3).all(|k| lo_a[k] <= hi_b[k] + band && lo_b[k] <= hi_a[k] + band)
    }

    /// Whole-solid AABB over all B-Rep vertices (None for an empty solid).
    fn solid_aabb(brep: &BRep) -> Option<([f64; 3], [f64; 3])> {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for v in brep.vertices() {
            let p = v.point.as_array();
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        lo[0].is_finite().then_some((lo, hi))
    }

    let fa = collect(a);
    let fb = collect(b);

    // Cross pairs (A×B): bit-exact AND near-coplanar both flag; condition 3
    // is mutual AABB overlap (the two faces must be able to interact).
    // PR-YR26: collect ALL such pairs (Stage 0 overlays each), not just the
    // first.
    let mut cross: Vec<CrossCoplanarPair> = Vec::new();
    for (ia, pa) in fa.iter().enumerate() {
        let Some(pa) = pa else { continue };
        for (ib, pb) in fb.iter().enumerate() {
            let Some(pb) = pb else { continue };
            if let Some(band) = near_coplanar_band(pa, pb) {
                if aabbs_overlap(&pa.lo, &pa.hi, &pb.lo, &pb.hi, band) {
                    cross.push(CrossCoplanarPair {
                        face_a: ia,
                        face_b: ib,
                        band,
                    });
                }
            }
        }
    }

    // Intra-solid pairs (A×A, B×B): only near-but-NOT-bit-identical planes
    // flag (bit-identical = one plane split into several faces, benign).
    // Condition 3 is different: the fragments are typically DISJOINT
    // in-plane regions, so the danger is contact by the OTHER solid —
    // flagged iff the other solid's whole-solid AABB overlaps EITHER
    // fragment (see the function docs for the F0018/F0025 evidence and the
    // weighed over-deferral).
    let mut intra: Option<(InputId, usize, usize)> = None;
    'intra: for (input, fp, other) in [
        (InputId::A, &fa, solid_aabb(b)),
        (InputId::B, &fb, solid_aabb(a)),
    ] {
        let Some((olo, ohi)) = other else { continue };
        for (i, pi) in fp.iter().enumerate() {
            let Some(pi) = pi else { continue };
            for (j, pj) in fp.iter().enumerate().skip(i + 1) {
                let Some(pj) = pj else { continue };
                if pi.raw_bits == pj.raw_bits {
                    continue;
                }
                // Spec `m8_intra_opposite_plane_canonicalization` B6: raw
                // plane values that are EXACTLY negated (f64 VALUE compare,
                // so `0.0 == -0.0` matches — bit compare would not) are two
                // orientations of ONE geometric plane. A valid 2-manifold
                // solid's faces on one plane are disjoint in-plane (a
                // stepped solid: lower-step top + overhang bottom), so the
                // arrangement needs no Stage-0 resolution — benign, like
                // the bit-identical case above. `to_yang_brep`'s sign-aware
                // sibling canonicalization produces exactly this form for
                // chained outputs; near-but-NOT-exact negation still walls
                // loud below (B7).
                if (0..4).all(|k| f64::from_bits(pi.raw_bits[k]) == -f64::from_bits(pj.raw_bits[k]))
                {
                    continue;
                }
                if let Some(band) = near_coplanar_band(pi, pj) {
                    // (A PR-KV6b attempt narrowed this to ADJACENT fragments;
                    // it regressed F0017–F0025 from the typed M8 deferral
                    // into NoExplicitRayOrigin failures — the conservative
                    // rule stands. The benign exactly-coplanar class — a
                    // 180° revolve's two caps — is excluded by the
                    // bit-identical rule above instead: producers SNAP their
                    // trig so exact-π caps carry bitwise-equal planes.)
                    if aabbs_overlap(&pi.lo, &pi.hi, &olo, &ohi, band)
                        || aabbs_overlap(&pj.lo, &pj.hi, &olo, &ohi, band)
                    {
                        intra = Some((input, i, j));
                        break 'intra;
                    }
                }
            }
        }
    }
    CoplanarScan { cross, intra }
}

/// PR-YR27 (Finding 3): finite-extent STRICT containment — is `p` strictly
/// inside planar face `fi`'s trimmed region (outer loop minus holes) of
/// `brep`, tested EXACTLY in the face's 2D plane frame?
///
/// Verdicts:
/// - `Some(true)`  — strictly interior: inside the loop arrangement
///   (even-odd over outer + holes) and ON no loop edge,
/// - `Some(false)` — ON a loop edge, or outside,
/// - `None`        — undecidable by this test (curved surface, a curved
///   loop edge — whose chord segment would misrepresent the boundary —
///   or non-finite coordinates). The caller must NOT exclude the face.
///
/// Exactness: the 2D projection `(u, v) = (q·e1, q·e2)` is one LINEAR map
/// applied in f64 and lifted exactly to rationals, so points that are
/// 3D-collinear along a straight loop edge project to EXACTLY 2D-collinear
/// points — the on-boundary rejection cannot be defeated by femto rounding.
/// Loop-vertex off-plane residuals (e.g. a Stage-0 snapped pair face) lie
/// along the face normal, which both frame axes annihilate, so they do not
/// perturb the in-plane region shape.
/// PR-KV7: finite-extent strict containment for a CYLINDER face, along the
/// AXIS only. A chainable boolean output can carry several faces of the SAME
/// infinite cylinder (the two stubs of a drill-through), so the YR27
/// infinite-surface membership ties between them; the axial span breaks the
/// tie exactly like the planar 2D test: the TRUE owning face's loop vertices
/// (rims / arc endpoints / ruling ends — all exactly on the surface) bound an
/// axial interval that strictly contains the centroid of every positive-area
/// triangle attributed to it, while a different same-cylinder face at best
/// touches the boundary. Azimuthal extent is NOT tested: a false candidate
/// that ties axially merely keeps the tie loud (P9-safe), never mis-excludes
/// the owner. `None` for non-cylinder faces / degenerate axes.
fn point_strictly_in_cylinder_face_axially(brep: &BRep, fi: usize, p: [f64; 3]) -> Option<bool> {
    let f = brep.faces().get(fi)?;
    let Surface::Cylinder {
        axis_point,
        axis_dir,
        ..
    } = f.surface
    else {
        return None;
    };
    let a = normalize3(axis_dir.as_array());
    let ap = axis_point.as_array();
    let t_of = |q: [f64; 3]| (q[0] - ap[0]) * a[0] + (q[1] - ap[1]) * a[1] + (q[2] - ap[2]) * a[2];
    let mut t_min = f64::INFINITY;
    let mut t_max = f64::NEG_INFINITY;
    for e_idx in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
        let e = brep.edges().get(*e_idx as usize)?;
        for v in [e.start, e.end] {
            let t = t_of(brep.vertices().get(v as usize)?.point.as_array());
            t_min = t_min.min(t);
            t_max = t_max.max(t);
        }
    }
    if !(t_min.is_finite() && t_max.is_finite() && t_min < t_max) {
        return None;
    }
    let t = t_of(p);
    Some(t_min < t && t < t_max)
}

fn point_strictly_in_planar_face(brep: &BRep, fi: usize, p: [f64; 3]) -> Option<bool> {
    use crate::coplanar_overlay::{cross_r, point_in_even_odd, ExactPoint2};
    use dashu::rational::RBig;

    let f = brep.faces().get(fi)?;
    let Surface::Plane { normal, .. } = f.surface else {
        return None;
    };
    let n = normal.as_array();
    if (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() < cad_primitives::MIN_FEATURE_SIZE {
        return None;
    }
    let (e1, e2) = ortho_basis(normal);
    let (e1, e2) = (e1.as_array(), e2.as_array());
    let proj = |q: [f64; 3]| -> Option<ExactPoint2> {
        ExactPoint2::from_f64(
            q[0] * e1[0] + q[1] * e1[1] + q[2] * e1[2],
            q[0] * e2[0] + q[1] * e2[1] + q[2] * e2[2],
        )
    };
    let q = proj(p)?;

    let mut edges2: Vec<(ExactPoint2, ExactPoint2)> = Vec::new();
    for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
        for &ei in lp {
            let edge = brep.edges().get(ei as usize)?;
            // A curved loop edge's chord would misrepresent the trimmed
            // boundary — undecidable, never a silent approximation.
            if !matches!(edge.curve, Curve::LineSegment) {
                return None;
            }
            let s = brep.vertices().get(edge.start as usize)?.point.as_array();
            let e = brep.vertices().get(edge.end as usize)?.point.as_array();
            edges2.push((proj(s)?, proj(e)?));
        }
    }

    // Exact ON-closed-segment rejection against every loop edge (strictness:
    // a boundary point is NOT contained).
    for (a, b) in &edges2 {
        if cross_r(a, b, &q) != RBig::ZERO {
            continue;
        }
        let dx = &b.x - &a.x;
        let dy = &b.y - &a.y;
        let t_num = (&q.x - &a.x) * &dx + (&q.y - &a.y) * &dy;
        let len2 = &dx * &dx + &dy * &dy;
        if t_num >= RBig::ZERO && t_num <= len2 {
            return Some(false);
        }
    }

    // Strictly off the boundary: exact even-odd over outer + hole loops
    // (the no-boundary precondition of `point_in_even_odd` now holds).
    Some(point_in_even_odd(&q, &edges2))
}

/// Surface distance of a point `c` to a coincident-cylinder pair, namely the
/// value `abs(dist_to_axis_line minus radius)`, which is zero on the shared
/// cylindrical surface. Used by the membrane resolution to match an
/// overlap-sheet triangle to a [`stage0::PairCylinder`] (the cylinder analog of
/// the planar plane-distance match).
fn centroid_on_cylinder(c: [f64; 3], p: &stage0::PairCylinder) -> f64 {
    let w = [
        c[0] - p.axis_point[0],
        c[1] - p.axis_point[1],
        c[2] - p.axis_point[2],
    ];
    let t = w[0] * p.axis_dir[0] + w[1] * p.axis_dir[1] + w[2] * p.axis_dir[2];
    let perp = [
        w[0] - t * p.axis_dir[0],
        w[1] - t * p.axis_dir[1],
        w[2] - t * p.axis_dir[2],
    ];
    let dist = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
    (dist - p.radius).abs()
}

/// PR-5: are `surf0` and `surf1` COINCIDENT cylinders — same axis line
/// (parallel axes, collinear) and equal radius, all within `tol`? Two such
/// cylinders share their entire lateral surface and `ssi_rs::intersect` refuses
/// them (`DegenerateInput`), so the caller must NOT route their edges to SSI.
fn cylinders_are_coincident(surf0: Surface, surf1: Surface, tol: f64) -> bool {
    let (
        Surface::Cylinder {
            axis_point: ap0,
            axis_dir: ad0,
            radius: r0,
        },
        Surface::Cylinder {
            axis_point: ap1,
            axis_dir: ad1,
            radius: r1,
        },
    ) = (surf0, surf1)
    else {
        return false;
    };
    let ad0 = normalize3(ad0.as_array());
    let ad1 = normalize3(ad1.as_array());
    // Parallel axes (|cross| ≈ 0).
    let cross = [
        ad0[1] * ad1[2] - ad0[2] * ad1[1],
        ad0[2] * ad1[0] - ad0[0] * ad1[2],
        ad0[0] * ad1[1] - ad0[1] * ad1[0],
    ];
    let sin = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
    if sin > tol.max(cad_primitives::TAU_MODEL) {
        return false;
    }
    // Equal radius.
    if (r0 - r1).abs() > tol {
        return false;
    }
    // Collinear axes: ap1 lies on ap0's axis line (perpendicular distance ≈ 0).
    let ap0a = ap0.as_array();
    let ap1a = ap1.as_array();
    let w = [ap1a[0] - ap0a[0], ap1a[1] - ap0a[1], ap1a[2] - ap0a[2]];
    let tw = w[0] * ad0[0] + w[1] * ad0[1] + w[2] * ad0[2];
    let perp = [w[0] - tw * ad0[0], w[1] - tw * ad0[1], w[2] - tw * ad0[2]];
    (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt() <= tol
}

/// Boolean operation on two B-Rep solids via a `MeshBoolean` backend.
///
/// **M3 functional pipeline** (replaces the PR-YR3/YR4 spatial-match +
/// majority-vote substitute, now a `#[cfg(test)]` differential oracle):
///
/// 0. **XOR is deferred (spec §Scope)** — its symmetric-difference result
///    is multi-shell / has a void that `reconstruct_topology` cannot
///    reassemble yet. `boolean()` errors loudly with `UnsupportedOp` once it
///    sees a non-empty XOR kept-set (a degenerate XOR with nothing to
///    reassemble still trivially yields an empty result).
/// 1. Obtain the real Stage-2 [`LabeledArrangement`] from
///    `backend.labeled_arrangement(..)` (full arrangement mesh +
///    per-triangle `surface`/`inside`/`patch` labels).
/// 2. **I6 weld** — the C++ producer does NOT always weld coincident
///    vertices (e.g. A@[0,0,0]/B@[0.7,0.3,0.4] emits a bit-exact duplicate
///    vertex used by shared triangles), so yang welds: map each vertex to
///    the *original index* of its first bit-identical occurrence. yang's
///    index-based adjacency then sees coincident points as one index. A
///    kept triangle that welds to a repeated index is a zero-area sliver at
///    that coincident point — dropped (no surface/volume; its edges pair up
///    so the output stays watertight). Two *distinct* surviving triangles
///    that weld to the same 3 indices are genuinely coincident faces →
///    `NonManifoldInput` (the a4 bit-exact-coincident-vertex case).
/// 3. `keep = la.keep_set(op)` — Stage 4 face survival.
/// 4. Compact the welded kept tris into a fresh sub-mesh (the output mesh).
/// 5. **Geometric face resolution** (Stage 6) per kept tri → a FULL
///    `TriangleAttributionMap` (every entry `Some`). A SURVIVING
///    multi-solid `surface[t]` (a §4.5.5 overlap-sheet triangle the (3b)
///    side rule kept) attributes to input A — the dedup survivor's side,
///    whose winding it carries (PR-YR26; B's coincident face has the same
///    plane, so the inherited output surface is identical). For a
///    *non-degenerate* (positive-area) triangle: pick the unique labeled-solid
///    face plane within `TAU_WORK` of the centroid; no match / a genuine tie →
///    `FaceResolutionFailed` (F3). For a *degenerate* (zero-area sliver, kept
///    because its edges pair into the watertight result) triangle: attribute
///    to the LOWEST labeled-solid face index within `TAU_WORK` (its centroid
///    sits on a solid edge, so the two adjacent planes tie — harmless for a
///    zero-area tri; never F3). Never a silent `None` (P9).
/// 6. `reconstruct_topology(..)` — flood-fill patches, walk boundary
///    cycles, inherit input-face `Surface`; full attribution ⇒ closed
///    boundary cycles ⇒ watertight 2-manifold output.
///
/// **N4 (provenance):** before the geometric resolution in step 5, a kept
/// triangle is attributed DIRECTLY from cherchi's per-triangle provenance
/// (`LabeledArrangement.source` → the parent input triangle → its B-Rep face via
/// the Stage-1 `tri_face` map) whenever that is unambiguous. The geometric path
/// remains the fallback. See [`provenance_face_reason`].
///
/// N4 helper: resolve a kept arrangement triangle's B-Rep face from cherchi's
/// per-triangle provenance (`§4.2.3`), not geometric centroid-proximity.
///
/// The triangle is attributed to `surface_input` (A or B — the side the keep-rule
/// kept it on; for a coplanar overlap sheet the §4.5.5 survivor convention picks
/// A). We select that side's parent from `source` and resolve it through that
/// input mesh's per-triangle face map (`tri_face_a` for A, `tri_face_b` for B).
/// This handles BOTH a non-coplanar triangle (its only parent) AND a coplanar
/// overlap sheet (the parent on the kept side). Returns `None` (→ geometric
/// fallback) when that side has no parent in `source`, the parent is beyond
/// the face map (a Stage-0 path that did not emit provenance, or a lineage-less
/// `from_mesh` / boolean-output input), or the parent maps to the `u32::MAX`
/// sentinel (a producer that emitted a map but could not attribute THAT
/// triangle — e.g. a coincident-cylinder band-strip column with no covering
/// arc-patch face). Never a wrong face.
/// Why N4 provenance attribution could not name a face for a kept triangle —
/// the exact reason the Stage-6 geometric fallback is still reached. Used by the
/// `YANG_N4_FALLBACK_PROBE` measurement (N4 retirement: prove the geometric path
/// is dead in production, or name the producers that still leave a triangle
/// un-provenanced).
#[derive(Debug, Clone, Copy)]
enum ProvMiss {
    /// The kept triangle's `source` has no parent triangle from this input
    /// (e.g. a cut/arrangement triangle with only the OTHER input's lineage).
    /// On a lineage-carrying input this is a producer FAULT (loud).
    NoSourceEntry,
    /// This input emitted NO provenance map at all (empty `tri_face`) — a
    /// LINEAGE-LESS input: a yang boolean OUTPUT chained directly back in,
    /// or a `from_mesh` B-Rep. This is the documented geometric-resolution
    /// path (task #53), NOT a fault.
    NoLineage,
    /// The map is present but the parent-triangle index lies beyond it —
    /// the producer emitted a TOO-SHORT provenance map (fault, loud).
    NoMap,
    /// The producer minted this triangle but could not attribute it to a face
    /// (`u32::MAX` sentinel — e.g. the coincident-cylinder band strip with no
    /// covering arc column). Fault, loud.
    Sentinel,
}

/// N4 (§4.2.3): map a kept triangle to its owning B-Rep face via the
/// arrangement's per-triangle provenance. `Ok(face)` on a hit; `Err(reason)`
/// records WHY it missed — `NoLineage` is the one non-fault reason (the
/// input never had a provenance map), everything else is loud at the caller
/// (task #53, spec `specs/n4_retire_stage6_fallback.md`).
fn provenance_face_reason(
    source: &[(LaInputId, u32)],
    surface_input: InputId,
    tri_face_a: &[u32],
    tri_face_b: &[u32],
) -> Result<u32, ProvMiss> {
    let (want_k, tf): (u32, &[u32]) = match surface_input {
        InputId::A => (0, tri_face_a),
        InputId::B => (1, tri_face_b),
    };
    if tf.is_empty() {
        return Err(ProvMiss::NoLineage);
    }
    let &(_, local) = source
        .iter()
        .find(|&&(LaInputId(k), _)| k == want_k)
        .ok_or(ProvMiss::NoSourceEntry)?;
    match tf.get(local as usize).copied() {
        None => Err(ProvMiss::NoMap),
        Some(f) if f == u32::MAX => Err(ProvMiss::Sentinel),
        Some(f) => Ok(f),
    }
}

/// M8 Stage-0 operand dump — diagnostic-only observer (spec
/// `specs/m8_stage0_inputcheck_clean_emission.md` §6). Env-gated on
/// `YANG_STAGE0_DUMP_DIR`; zero-cost when unset (never set in production or
/// WASM). Writes, per boolean call, the EXACT operand meshes handed to the
/// backend — plus, when Stage 0 rewrote them, each solid's pre-Stage-0
/// Stage-1 mesh (`_pre`) and the `tri_face` provenance maps — so the
/// five-axiom census can split defects introduced-vs-inherited and join
/// offenders back to B-Rep faces. Vertex coordinates use f64 `Display`
/// (shortest round-trip), so the dump is bit-faithful. Write failures are
/// reported on stderr and never affect the boolean (read-only, spec I6).
fn stage0_dump(
    op: BoolOp,
    stage0: Option<&stage0::Stage0>,
    cyl_pair_count: usize,
    mesh_a: &Mesh,
    mesh_b: &Mesh,
    pre_a: &Mesh,
    pre_b: &Mesh,
) {
    let Some(dir) = std::env::var_os("YANG_STAGE0_DUMP_DIR") else {
        return;
    };
    use std::sync::atomic::{AtomicU64, Ordering};
    // Process-global op counter: yang-rs has no case identity; harnesses
    // namespace by pointing the env var at a per-case directory.
    static OP_COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = OP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::path::PathBuf::from(dir);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!(
            "[stage0-dump] create_dir_all({}) failed: {e}",
            dir.display()
        );
        return;
    }
    let op_name = match op {
        BoolOp::Union => "union",
        BoolOp::Intersect => "intersect",
        BoolOp::Subtract => "subtract",
        BoolOp::Xor => "xor",
    };
    let stem = format!("{n:03}_{op_name}");
    let write_obj = |suffix: &str, m: &Mesh| {
        let path = dir.join(format!("{stem}_{suffix}.obj"));
        let mut out = String::new();
        for v in &m.verts {
            out.push_str(&format!("v {} {} {}\n", v.x(), v.y(), v.z()));
        }
        for t in &m.tris {
            out.push_str(&format!("f {} {} {}\n", t[0] + 1, t[1] + 1, t[2] + 1));
        }
        if let Err(e) = std::fs::write(&path, out) {
            eprintln!("[stage0-dump] write {} failed: {e}", path.display());
        }
    };
    write_obj("a", mesh_a);
    write_obj("b", mesh_b);
    let mut meta = format!(
        "op: {op_name}\nstage0: {}\ncyl_pairs: {cyl_pair_count}\n\
         mesh_a: {} verts / {} tris\nmesh_b: {} verts / {} tris\n",
        stage0.is_some(),
        mesh_a.verts.len(),
        mesh_a.tris.len(),
        mesh_b.verts.len(),
        mesh_b.tris.len(),
    );
    if let Some(s0) = stage0 {
        write_obj("a_pre", pre_a);
        write_obj("b_pre", pre_b);
        let write_csv = |suffix: &str, tf: &[u32]| {
            let path = dir.join(format!("{stem}_{suffix}.tri_face.csv"));
            let mut out = String::new();
            for f in tf {
                out.push_str(&format!("{f}\n"));
            }
            if let Err(e) = std::fs::write(&path, out) {
                eprintln!("[stage0-dump] write {} failed: {e}", path.display());
            }
        };
        write_csv("a", &s0.tri_face_a);
        write_csv("b", &s0.tri_face_b);
        for p in &s0.pairs {
            meta.push_str(&format!(
                "pair_plane: face_a={} face_b={} opposite={} n=({},{},{}) d={} band={}\n",
                p.face_a, p.face_b, p.opposite, p.n[0], p.n[1], p.n[2], p.d, p.band,
            ));
        }
    }
    let meta_path = dir.join(format!("{stem}_meta.txt"));
    if let Err(e) = std::fs::write(&meta_path, meta.as_bytes()) {
        eprintln!("[stage0-dump] write {} failed: {e}", meta_path.display());
    }
}

/// Case-IV phantom guard analysis (spec `yang_case_iv_phantom_guard`,
/// M8 increment 15): the forced minimum rim segment count over all
/// ANALYTICALLY DISJOINT cylinder-face pairs (A×B) whose Stage-1 chord
/// bands could otherwise overlap the gap between the surfaces (Yang Fig. 8
/// Case IV — the meshes would intersect where the surfaces do not,
/// manufacturing a phantom intersection curve; measured F0088 op 4).
///
/// For each pair: the axis-line distance gives the analytic gap (external
/// `d − r_a − r_b` for any axis pose; nested `r_large − d − r_small` for
/// parallel axes). A positive gap demands the smallest `N` with
/// `sag(r_a, N) + sag(r_b, N) ≤ gap/2` (`sag(r, N) = r(1 − cos(π/N))` —
/// the Stage-1 sagitta, A14.3 single source; the factor-2 margin keeps the
/// combined band strictly clear, and a finer N is always chord-valid). Far
/// pairs derive a tiny N that the natural Stage-1 `max()` absorbs — the
/// guard is self-limiting, no mode branch. True near-tangency (N would
/// exceed 4096) yields no requirement: the loud Stage-3 `AmbiguousCurve`
/// remains the tripwire (P9 — never silently proceed with phantom
/// topology).
/// The Case-IV pairwise requirement of two cylinder surfaces (spec
/// `yang_case_iv_phantom_guard`): `None` unless the pair is analytically
/// disjoint with a practical derived N — the smallest `N` with
/// `sag(r_a, N) + sag(r_b, N) ≤ gap/2` (`sag(r, N) = r(1 − cos(π/N))`, the
/// Stage-1 sagitta; the factor-2 margin keeps the combined chord band
/// strictly clear of the gap, and a finer N is always chord-valid). Shared
/// by the `boolean()` cross-pair guard AND Stage 1's intra-solid fold.
fn cyl_pair_phantom_n(
    (pa, da, ra): (Point3, Vector3, f64),
    (pb, db, rb): (Point3, Vector3, f64),
) -> Option<usize> {
    let ua = normalize3(da.as_array());
    let ub = normalize3(db.as_array());
    let w = [pb.x() - pa.x(), pb.y() - pa.y(), pb.z() - pa.z()];
    let cx = [
        ua[1] * ub[2] - ua[2] * ub[1],
        ua[2] * ub[0] - ua[0] * ub[2],
        ua[0] * ub[1] - ua[1] * ub[0],
    ];
    let cross_norm = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    // Axis-line distance: skew/crossing axes project the offset onto the
    // common normal; parallel axes take the perpendicular point-line
    // distance.
    let (parallel, d_axes) = if cross_norm > 1e-12 {
        let d = (w[0] * cx[0] + w[1] * cx[1] + w[2] * cx[2]).abs() / cross_norm;
        (false, d)
    } else {
        let t = w[0] * ua[0] + w[1] * ua[1] + w[2] * ua[2];
        let perp = [w[0] - t * ua[0], w[1] - t * ua[1], w[2] - t * ua[2]];
        let d = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
        (true, d)
    };
    let external = d_axes - (ra + rb);
    let nested = if parallel {
        ra.max(rb) - d_axes - ra.min(rb)
    } else {
        f64::NEG_INFINITY
    };
    let gap = external.max(nested);
    if gap.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return None; // surfaces intersect / NaN (degenerate input): real curve or no-op
    }
    let sag = |r: f64, n: usize| r * (1.0 - (std::f64::consts::PI / n as f64).cos());
    let mut n = 3usize;
    while sag(ra, n) + sag(rb, n) > gap / 2.0 {
        n += 1;
        if n > 4096 {
            // True near-tangency: no finite practical N — leave the loud
            // Stage-3 stop as the tripwire.
            return None;
        }
    }
    Some(n)
}

fn phantom_min_rim_segments(a: &BRep, b: &BRep) -> Option<usize> {
    let cyls = |brep: &BRep| -> Vec<(Point3, Vector3, f64)> {
        brep.faces()
            .iter()
            .filter_map(|f| match f.surface {
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                } => Some((axis_point, axis_dir, radius)),
                _ => None,
            })
            .collect()
    };
    let (ca, cb) = (cyls(a), cyls(b));
    let mut req: Option<usize> = None;
    // CROSS pairs only (A×B): the two operands' meshes must not intersect
    // where their surfaces do not (the measured F0088 cut-4 class).
    // INTRA-solid pairs are folded into Stage 1's own N selection
    // (`stage1_tessellate_inner` — M8 increment 16), where EVERY
    // tessellation of the solid picks them up (conversion, Stage-0 rebuilds,
    // this guard's rebuilds), so they need no handling here.
    for &sa in &ca {
        for &sb in &cb {
            if let Some(n) = cyl_pair_phantom_n(sa, sb) {
                req = Some(req.map_or(n, |r: usize| r.max(n)));
            }
        }
    }
    // Self-limiting gate: a requirement BOTH solids' natural Stage-1 N
    // already satisfies is dropped, keeping the common path byte-identical
    // (and rebuild-free). `natural_rim_n` mirrors the Stage-1 N derivation
    // (chord bound over all rim circles, N from the max radius).
    let natural_rim_n = |brep: &BRep| -> usize {
        let Some(d_eps) = curved_chord_bound(brep.edges()) else {
            return usize::MAX; // no circles: nothing to boost
        };
        let max_r = brep
            .edges()
            .iter()
            .filter_map(|e| match e.curve {
                Curve::Circle { radius, .. } => Some(radius),
                _ => None,
            })
            .fold(0.0f64, f64::max);
        let mut n = 3usize;
        if d_eps > 0.0 {
            while max_r * (1.0 - (std::f64::consts::PI / n as f64).cos()) > d_eps {
                n += 1;
            }
        }
        n
    };
    let gated = match req {
        Some(n) if n > natural_rim_n(a) || n > natural_rim_n(b) => Some(n),
        _ => None,
    };
    if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
        eprintln!(
            "[phantom-guard] req={req:?} natural=({},{}) gated={gated:?} \
             cyl_faces=({},{})",
            natural_rim_n(a),
            natural_rim_n(b),
            a.faces()
                .iter()
                .filter(|f| matches!(f.surface, Surface::Cylinder { .. }))
                .count(),
            b.faces()
                .iter()
                .filter(|f| matches!(f.surface, Surface::Cylinder { .. }))
                .count(),
        );
    }
    gated
}

pub fn boolean(
    a: &BRep,
    b: &BRep,
    op: BoolOp,
    backend: &dyn MeshBoolean,
) -> Result<BRep, YangError> {
    // Case-IV phantom guard (spec `yang_case_iv_phantom_guard`): rebuild
    // both operands at the pair-derived rim density BEFORE any Stage-0/1
    // machinery samples their meshes, so analytically-disjoint cylinder
    // pairs cannot mesh-intersect. `None` (no cylinder faces, e.g. the
    // `from_mesh` chained-output operand, or no disjoint pair demanding
    // more than each solid's own N) leaves both operands byte-identical.
    let boosted: Option<(BRep, BRep)> = match phantom_min_rim_segments(a, b) {
        Some(n) => Some((
            a.rebuilt_with_min_rim_segments(n)?,
            b.rebuilt_with_min_rim_segments(n)?,
        )),
        None => None,
    };
    let (a, b): (&BRep, &BRep) = match &boosted {
        Some((ba, bb)) => (ba, bb),
        None => (a, b),
    };

    // (0) Stage 0 — §4.5.5 coplanar preprocessing (PR-YR26, M8 slice b).
    // Near-coplanar planar A×B face pairs are HANDLED: both faces snapped
    // onto one canonical shared plane, segmented by the exact 2D overlay,
    // and re-tessellated so the overlap region carries IDENTICAL meshes on
    // both solids (see `stage0::stage0_preprocess`). Unsupported residue
    // (intra-solid near pairs — the chained-output class — plus curved /
    // multi-pair faces and overlay failures) keeps the loud typed PR-YR24
    // wall (`CoplanarFacesUnsupported`).
    let stage0 = stage0::stage0_preprocess(a, b)?;
    // M8-cyl Increment 1 (§4.5.5 curved analog): when the planar scan found NO
    // cross pairs, a COINCIDENT-CYLINDER pair (the gear's bore wall ∩ a coaxial
    // flange/plug wall, opposite normal, full θ, one z-extent contained in the
    // other) gets a conformal re-tessellation so its overlap band is
    // bit-identical on BOTH solids' meshes. cherchi then pocket-dedups the band
    // into one multi-label sheet and the membrane resolution below drops it.
    // `task28_plug_in_bore` proved both native cherchi AND the C++ sidecar leave
    // this non-watertight WITHOUT this upstream conformal step. Only consulted
    // when the planar Stage-0 produced nothing (the two paths never overlap on a
    // single pair in Increment 1's scope).
    let stage0 = match stage0 {
        Some(s0) => Some(s0),
        None => stage0::coincident_cylinder_stage0(a, b)?,
    };
    // PR-5: coincident-CYLINDER A×B pairs (the membrane analog of the planar
    // `PairPlane`s in `stage0`). cherchi (coplanar PRs 1-4) constructs the
    // coincident-cylinder overlap with a MULTI-SOLID label exactly as it does a
    // coplanar planar overlap, but the Stage-0 planar scan records only
    // `Surface::Plane` pairs — so a coaxial-cylinder sheet (a flange outer wall
    // coincident with a gear bore, `err.waffle`) had no matching pair and was
    // dropped with `FaceResolutionFailed`. This parallel detector supplies the
    // keep/drop decision for those sheets. It does NOT touch the planar overlay
    // / mesh re-tessellation path (the coincident-cylinder meshes are already
    // bit-identical: both faces are the identical analytic cylinder).
    let cyl_pairs = stage0::detect_coincident_cylinder_pairs(a, b);
    let (mesh_a, mesh_b): (&Mesh, &Mesh) = match &stage0 {
        Some(s0) => (&s0.mesh_a, &s0.mesh_b),
        // No coplanar pairs: the B-Reps' own Stage-1 meshes — byte-for-byte
        // the pre-YR26 path.
        None => (a.as_mesh(), b.as_mesh()),
    };
    // M8 diagnostic operand dump (env-gated, read-only; spec
    // `m8_stage0_inputcheck_clean_emission` §6).
    stage0_dump(
        op,
        stage0.as_ref(),
        cyl_pairs.len(),
        mesh_a,
        mesh_b,
        a.as_mesh(),
        b.as_mesh(),
    );

    // (1) Stage 2: full labeled arrangement.
    let la = backend
        .labeled_arrangement(mesh_a, mesh_b)
        .map_err(YangError::MeshBooleanFailed)?;

    // (2) I6 weld: the C++ producer does NOT always weld coincident vertices
    // (it can emit two distinct indices at bit-identical coordinates — a
    // non-manifold touching point — used by shared triangles). yang's
    // index-based adjacency requires coincident points to share one index, so
    // weld each vertex to the ORIGINAL index of its first coincident
    // occurrence. (Mapping to the original index — not a renumbered counter —
    // keeps `la.mesh.verts[welded]` valid: coordinates are unchanged.)
    //
    // PR-KV10 (M8 residue): for ALL-PLANAR input pairs the weld is
    // NEAR-aware, not just bit-exact. The old "the producer never emits
    // TAU_WORK-near-but-bit-distinct coincident verts" assumption is FALSE
    // for chained planar inputs: an oblique solid's f64 vertices make
    // adjacent same-face tessellation triangles span femto-different EXACT
    // planes, so the exact arrangement legitimately mints distinct
    // intersection points ~1e-16·scale apart where several intersection
    // segments junction (one geometric point, several generating tri
    // pairs). Left distinct, the copies chain into sliver fans in the
    // output B-Rep and poison the NEXT boolean's attribution (the
    // F0016-class corpus residue's second layer — found behind the
    // intra-coplanar wall). Welding them within the scale-relative rounding
    // band `TAU_WORK·(1+|coord|)` is the same reconciliation principle as
    // the §4.5.5 Stage-0 snap; genuinely distinct model features are
    // ≥ MIN_FEATURE_SIZE apart — six orders beyond the band. Clusters weld
    // to their LOWEST member index (deterministic; survivor keeps its own
    // coordinates). Bucketed by a quantized grid with 27-neighborhood
    // probing + an EXACT per-pair band check — quantization alone aliases
    // (the KV8c lesson), so it only ever NOMINATES candidates, never
    // decides.
    //
    // CURVED inputs keep the bit-exact weld: the cyl×cyl pipeline expects
    // near-coincident-but-structurally-distinct vertices at ruling-line /
    // tangency junctions (one copy per incident surface's chord ring) and
    // reconciles them ITSELF in Stage-4 relocation with curve knowledge
    // (the KV9 junction duplicate collapse); welding them at step (2)
    // collapses lens-tip seam edges into degenerate (<3-edge) output loops
    // — found by kv9_cyl_cyl_special RED on the first attempt.
    let all_planar = a
        .faces()
        .iter()
        .chain(b.faces().iter())
        .all(|f| matches!(f.surface, Surface::Plane { .. }));
    let weld: Vec<u32> = if all_planar {
        use std::collections::HashMap;
        let verts = &la.mesh.verts;
        // Union-find over vertex indices (path-halving; union by min index
        // happens at the final resolution pass).
        let mut parent: Vec<u32> = (0..verts.len() as u32).collect();
        fn find(parent: &mut [u32], mut x: u32) -> u32 {
            while parent[x as usize] != x {
                parent[x as usize] = parent[parent[x as usize] as usize];
                x = parent[x as usize];
            }
            x
        }
        // Grid cell size: one band at the mesh's coordinate scale.
        let scale = verts
            .iter()
            .flat_map(|v| v.as_array())
            .fold(0.0f64, |m, c| m.max(c.abs()));
        let band = cad_primitives::TAU_WORK * (1.0 + scale);
        let cell = |c: f64| -> i64 { (c / band).floor() as i64 };
        let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::with_capacity(verts.len());
        for (i, v) in verts.iter().enumerate() {
            let p = v.as_array();
            let key = [cell(p[0]), cell(p[1]), cell(p[2])];
            // Probe the 27-neighborhood for near-coincident occupants; the
            // EXACT pairwise band test decides. Union with EVERY in-band
            // occupant (a vertex can bridge two so-far-separate clusters).
            for dx in -1..=1i64 {
                for dy in -1..=1i64 {
                    for dz in -1..=1i64 {
                        let Some(occ) = grid.get(&[key[0] + dx, key[1] + dy, key[2] + dz]) else {
                            continue;
                        };
                        for &j in occ {
                            let q = verts[j as usize].as_array();
                            let pair_band = cad_primitives::TAU_WORK
                                * (1.0
                                    + p.iter().chain(q.iter()).fold(0.0f64, |m, c| m.max(c.abs())));
                            if (0..3).all(|k| (p[k] - q[k]).abs() <= pair_band) {
                                let (ri, rj) = (find(&mut parent, i as u32), find(&mut parent, j));
                                if ri != rj {
                                    // Root at the smaller index so the final
                                    // representative is the cluster minimum.
                                    parent[ri.max(rj) as usize] = ri.min(rj);
                                }
                            }
                        }
                    }
                }
            }
            grid.entry(key).or_default().push(i as u32);
        }
        (0..verts.len() as u32)
            .map(|i| find(&mut parent, i))
            .collect()
    } else {
        // Bit-exact weld (the pre-KV10 path, byte-identical for curved
        // pipelines): weld each vertex to the ORIGINAL index of its first
        // bit-identical occurrence.
        use std::collections::HashMap;
        let mut first: HashMap<[u64; 3], u32> = HashMap::with_capacity(la.mesh.verts.len());
        let mut weld: Vec<u32> = la
            .mesh
            .verts
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let key = [v.x().to_bits(), v.y().to_bits(), v.z().to_bits()];
                *first.entry(key).or_insert(i as u32)
            })
            .collect();

        // PR-6 (coincident-cylinder rim conformal weld). The §4.5.5 planar
        // Stage-0 overlay makes two coincident PLANAR faces' shared loop
        // vertices bit-identical (the cross-weld at `stage0.rs:261`). Its
        // curved analog: where a coincident-CYLINDER pair's lateral meets a
        // CAP PLANE, cherchi's exact arrangement mints the SAME rim-circle
        // point redundantly (once per generating tri-pair / incident surface),
        // landing a cluster of copies a FEW ULPs apart (verified on
        // `err.waffle`: 31 such near-twins, all at machine-zero distance from
        // a `cyl_pairs` lateral AND on the cap plane, max separation ~9e-19 at
        // a coordinate scale of 5e-3 — i.e. ~1 ULP). The bit-exact weld leaves
        // them distinct, so a kept triangle can carry two copies of one
        // geometric rim point: a zero-area sliver that fails Stage-4
        // (`DegenerateTriangle` at v4497/v4495) and pinches the post-membrane
        // seam.
        //
        // The conformal reconciliation: union ONLY vertices that lie EXACTLY
        // (within the pair's analytic band) on a coincident-cylinder pair's
        // shared lateral AND are within the scale-relative `TAU_WORK·(1+scale)`
        // band of each other. This is an EXACT-IDENTITY weld of redundant
        // reconstructions of one analytic point — NOT a tolerance bucket:
        //   • Membership is gated on the analytic coincident-cylinder surface
        //     (machine-zero radial distance), not a proximity guess.
        //   • The union band (~1e-12) is six orders below MIN_FEATURE_SIZE
        //     (1e-6); genuinely distinct rim points (≥ chord-spacing ~1e-4)
        //     never fuse — only sub-ULP duplicates do.
        //   • It touches NO planar case (gated on `cyl_pairs`), so it cannot
        //     reintroduce the reverted F0057 planar-weld masking (that weld
        //     fused planar vertices and hid 74 unpaired edges).
        // Survivor = the cluster's minimum welded index (deterministic).
        if !cyl_pairs.is_empty() {
            let verts = &la.mesh.verts;
            // On-cylinder predicate: radial distance within the pair band. The
            // observed rim duplicates sit at ~1e-19 (machine zero); the band
            // (1e-7) is a safe analytic membership gate that admits no
            // off-surface vertex of this model (off-rim arrangement points are
            // ≥ chord-scale ~1e-4 off any OTHER cylinder, and on-lateral
            // tessellation chords sit up to the sagitta INSIDE the radius —
            // far beyond 1e-7 — so only true on-surface rim points qualify).
            let on_rim = |i: u32| -> bool {
                let c = verts[i as usize].as_array();
                cyl_pairs
                    .iter()
                    .any(|p| centroid_on_cylinder(c, p) <= p.band)
            };
            let scale = verts
                .iter()
                .flat_map(|v| v.as_array())
                .fold(0.0f64, |m, c| m.max(c.abs()));
            let cluster_band = cad_primitives::TAU_WORK * (1.0 + scale);
            // Candidate rim vertices (post bit-exact weld representatives only).
            let rim: Vec<u32> = (0..verts.len() as u32)
                .filter(|&i| weld[i as usize] == i && on_rim(i))
                .collect();
            // Bucketed union-find (27-neighborhood probe + exact pairwise band).
            let mut parent: HashMap<u32, u32> = rim.iter().map(|&i| (i, i)).collect();
            fn find(parent: &mut HashMap<u32, u32>, mut x: u32) -> u32 {
                while parent[&x] != x {
                    let g = parent[&parent[&x]];
                    parent.insert(x, g);
                    x = g;
                }
                x
            }
            let cell = |c: f64| -> i64 { (c / cluster_band).floor() as i64 };
            let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
            for &i in &rim {
                let p = verts[i as usize].as_array();
                let key = [cell(p[0]), cell(p[1]), cell(p[2])];
                for dx in -1..=1i64 {
                    for dy in -1..=1i64 {
                        for dz in -1..=1i64 {
                            let Some(occ) = grid.get(&[key[0] + dx, key[1] + dy, key[2] + dz])
                            else {
                                continue;
                            };
                            for &j in occ {
                                let q = verts[j as usize].as_array();
                                let pair_band = cad_primitives::TAU_WORK
                                    * (1.0
                                        + p.iter()
                                            .chain(q.iter())
                                            .fold(0.0f64, |m, c| m.max(c.abs())));
                                if (0..3).all(|k| (p[k] - q[k]).abs() <= pair_band) {
                                    let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                                    if ri != rj {
                                        parent.insert(ri.max(rj), ri.min(rj));
                                    }
                                }
                            }
                        }
                    }
                }
                grid.entry(key).or_default().push(i);
            }
            // Re-point every vertex whose bit-exact representative is a rim
            // candidate to its cluster minimum.
            for w in weld.iter_mut() {
                if parent.contains_key(w) {
                    *w = find(&mut parent, *w);
                }
            }
        }

        weld
    };

    // (3) Stage 4: which arrangement tris survive `op`.
    let kept = la.keep_set(op);

    // KV9-F1 diagnosis probe (read-only, env-gated): per-input label + keep
    // census over the labeled arrangement.
    if std::env::var_os("YANG_KEEP_PROBE").is_some() {
        let kept_set: std::collections::BTreeSet<usize> = kept.iter().copied().collect();
        let mut rows: std::collections::BTreeMap<(String, Vec<bool>, bool), usize> =
            std::collections::BTreeMap::new();
        for t in 0..la.mesh.tris.len() {
            let surf = format!("{:?}", la.surface[t]);
            *rows
                .entry((surf, la.inside[t].clone(), kept_set.contains(&t)))
                .or_insert(0) += 1;
        }
        eprintln!(
            "[keep-probe] la tris {} kept {} (op {op:?})",
            la.mesh.tris.len(),
            kept.len()
        );
        for ((surf, inside, k), n) in rows {
            eprintln!("[keep-probe]   surface {surf} inside {inside:?} kept={k}: {n}");
        }
        let mut patches: std::collections::BTreeMap<u32, (String, usize)> =
            std::collections::BTreeMap::new();
        for t in 0..la.mesh.tris.len() {
            let e = patches
                .entry(la.patch[t])
                .or_insert_with(|| (format!("{:?}", la.surface[t]), 0));
            e.1 += 1;
        }
        for (pid, (surf, n)) in patches {
            eprintln!("[keep-probe]   patch {pid}: surface {surf} tris {n}");
        }
    }

    // (3a) XOR deferred (spec §Scope): its symmetric-difference result is
    // multi-shell / has a void that `reconstruct_topology` cannot reassemble
    // yet. Error LOUDLY (`UnsupportedOp`) rather than emitting a generic
    // `NonManifoldOutput` or a silently-wrong result (P9). Gated on a
    // non-empty XOR kept-set: a degenerate XOR with nothing to reassemble
    // (empty arrangement) still trivially succeeds with an empty result, so
    // op-dispatch over an empty arrangement is well-defined for all four ops.
    if op == BoolOp::Xor && !kept.is_empty() {
        return Err(YangError::UnsupportedOp(op));
    }

    // (4) Compact kept sub-mesh: weld + per-op winding fix, then remap the
    // referenced (welded) verts to dense indices.
    let mut remap: Vec<Option<u32>> = vec![None; la.mesh.verts.len()];
    let mut compact_verts: Vec<Point3> = Vec::new();
    let mut compact_tris: Vec<[u32; 3]> = Vec::with_capacity(kept.len());
    // compact-tri index -> original `la` tri index (for surface lookup).
    let mut orig_tri: Vec<usize> = Vec::with_capacity(kept.len());
    for &orig_t in &kept {
        let raw = la.mesh.tris[orig_t];

        // (3b) §4.5.5 overlap-sheet ("membrane") resolution. A triangle with
        // a multi-solid surface label lies on the trimmed common planar
        // surface of a Stage-0 pair. Cherchi's keep-rules alone keep it for
        // EVERY op (surface = {A,B}, inside = ∅ satisfies the union /
        // intersection / subtraction-branch-1 rules, booleans.cpp:1397/
        // 1422/1467 — the C++ emits the zero-volume sheet); solid semantics
        // instead keep it iff exactly ONE side of its plane is inside the
        // result. With the pair's normal-agreement flag (`opposite`: solids
        // on opposite sides, stacked; else both interiors on the same
        // side, flush/pocket) that side rule reduces to:
        //
        //   Union:     keep iff !opposite (boundary of both ⇒ of the union)
        //   Intersect: keep iff !opposite (boundary of A∩B; opposite ⇒ the
        //              intersection is the zero-volume sheet itself: drop)
        //   Subtract:  keep iff opposite (B is beyond the plane: the sheet
        //              stays A's boundary; equal ⇒ B consumes it: the
        //              pocket OPENING is removed)
        //
        // The kept copy is the dedup survivor — input A's, with A's winding
        // — which is the correct result orientation in every kept case
        // (subtract-opposite / union-equal / intersect-equal all bound the
        // result with A's outward direction).
        if la.surface[orig_t].len() > 1 {
            let p0 = la.mesh.verts[raw[0] as usize].as_array();
            let p1 = la.mesh.verts[raw[1] as usize].as_array();
            let p2 = la.mesh.verts[raw[2] as usize].as_array();
            let c = [
                (p0[0] + p1[0] + p2[0]) / 3.0,
                (p0[1] + p1[1] + p2[1]) / 3.0,
                (p0[2] + p1[2] + p2[2]) / 3.0,
            ];
            // The sheet's `opposite` flag — found by matching its centroid to a
            // Stage-0 PLANAR pair plane (the §4.5.5 membrane) OR, failing that,
            // to a coincident-CYLINDER pair (PR-5: a sheet triangle lies on a
            // cylinder pair iff `|dist(c, axis_line) − radius| <= band`). Only
            // if NEITHER matches is it an unhandled config — still loud (P9).
            let planar = stage0.as_ref().and_then(|s0| {
                s0.pairs
                    .iter()
                    .find(|p| (p.n[0] * c[0] + p.n[1] * c[1] + p.n[2] * c[2] + p.d).abs() <= p.band)
                    .map(|p| p.opposite)
            });
            let opposite = match planar {
                Some(o) => o,
                // A sheet triangle on the TESSELLATED cylinder sits up to the
                // Stage-1 chord sagitta inside the analytic radius — far beyond
                // the detection `band`. Match against the curved chord bound
                // `d_ε` (the SAME bound Stage 1 sizes the tessellation to and
                // Stage-6 attribution uses for cylinder faces — A14.3, not a
                // widening). Both solids' overlap meshes are bit-identical, so
                // either chord bound applies; use the larger to be safe.
                None => match cyl_pairs.iter().find(|p| {
                    let de = curved_chord_bound(a.edges())
                        .unwrap_or(0.0)
                        .max(curved_chord_bound(b.edges()).unwrap_or(0.0))
                        .max(p.band);
                    centroid_on_cylinder(c, p) <= de
                }) {
                    Some(p) => p.opposite,
                    // On no known pair (planar or cylinder) — loud, never a
                    // guessed config.
                    None => return Err(YangError::FaceResolutionFailed { tri: orig_t }),
                },
            };
            let keep_sheet = match op {
                BoolOp::Union | BoolOp::Intersect => !opposite,
                BoolOp::Subtract => opposite,
                // XOR never reaches here (rejected at (3a) on a non-empty
                // kept set), but the side rule drops the sheet in both
                // configs anyway.
                BoolOp::Xor => false,
            };
            if !keep_sheet {
                continue;
            }
        }

        // Apply the weld (coincident points → shared original index).
        let mut tri = [
            weld[raw[0] as usize],
            weld[raw[1] as usize],
            weld[raw[2] as usize],
        ];
        // A welded triangle with a repeated index is a zero-area sliver at a
        // coincident (welded) point — it carries no surface and no volume, and
        // its two non-degenerate directed edges are mutual opposites that
        // cancel, so dropping it preserves the watertight half-edge pairing.
        // (Real, in-scope arrangement artifact — NOT non-manifold input.)
        if tri[0] == tri[1] || tri[1] == tri[2] || tri[2] == tri[0] {
            continue;
        }
        // Per-op winding fix (Cherchi booleans.cpp boolSubtraction:1480-1483):
        // the keep-rule selects triangles but some kept triangles bound the
        // result with reversed orientation and must be flipped so the output
        // is consistently outward-oriented (I9 signed volume). Union /
        // Intersection keep winding as-is.
        if flip_for_op(op, &la, orig_t) {
            tri.swap(1, 2);
        }
        let mut new_tri = [0u32; 3];
        for (k, &wi) in tri.iter().enumerate() {
            let slot = &mut remap[wi as usize];
            let new_vi = match slot {
                Some(idx) => *idx,
                None => {
                    let idx = compact_verts.len() as u32;
                    compact_verts.push(la.mesh.verts[wi as usize]);
                    *slot = Some(idx);
                    idx
                }
            };
            new_tri[k] = new_vi;
        }
        compact_tris.push(new_tri);
        orig_tri.push(orig_t);
    }
    // (I6 guard) Two distinct surviving triangles that welded to the same 3
    // vertices are genuinely coincident faces (non-manifold input) — e.g. the
    // a4 fixture's two tris over bit-exact-coincident vertices. A valid
    // arrangement has no such pair; reject it. (Compact indices are 1:1 with
    // welded indices, so a sorted-index key suffices.)
    {
        use std::collections::HashSet;
        let mut seen: HashSet<[u32; 3]> = HashSet::with_capacity(compact_tris.len());
        for t in &compact_tris {
            let mut sorted = *t;
            sorted.sort_unstable();
            if !seen.insert(sorted) {
                return Err(YangError::NonManifoldInput);
            }
        }
    }
    let kept_submesh = Mesh::new(compact_verts, compact_tris);

    // (5) Stage 6: face resolution → FULL attribution. PRIMARY path is N4
    // provenance (cherchi `source` → B-Rep face via the per-triangle face map);
    // the geometric resolution below is the fallback. The face map is the
    // inputs' OWN Stage-1 `tri_face` when Stage 0 did not re-tessellate, else the
    // Stage-0 re-tessellated meshes' `tri_face` (`stage0::Stage0`). Either may be
    // empty (a Stage-0 path that does not emit provenance yet, or a lineage-less
    // input) → that triangle falls back to geometric.
    let (tri_face_a, tri_face_b): (&[u32], &[u32]) = match &stage0 {
        Some(s0) => (&s0.tri_face_a, &s0.tri_face_b),
        None => (a.tri_face(), b.tri_face()),
    };
    let mut attributions: Vec<Option<TriangleAttribution>> = Vec::with_capacity(orig_tri.len());
    for (compact_t, &orig_t) in orig_tri.iter().enumerate() {
        let surf = &la.surface[orig_t];
        let (input_brep, input) = if surf.len() > 1 {
            // §4.5.5 trimmed common surface (PR-YR26): a SURVIVING
            // multi-label triangle is a kept overlap-sheet triangle (the
            // (3b) side rule already decided it bounds the result). It
            // descends from coincident faces of BOTH inputs; the kept copy
            // is the dedup survivor — input A's, with A's winding — so it
            // attributes to input A (its plane equals B's, so the
            // inherited output surface is identical either way; A is the
            // deterministic choice consistent with the kept orientation).
            (a, InputId::A)
        } else {
            let LaInputId(k) = surf[0];
            // cherchi InputId(u32): 0 → A, 1 → B.
            match k {
                0 => (a, InputId::A),
                _ => (b, InputId::B),
            }
        };

        // N4 (provenance, §4.2.3): attribute this kept triangle to its B-Rep face
        // DIRECTLY from its parent input triangle (cherchi `source` → `tri_face`)
        // — exact, no geometry, no tolerance. Works for non-coplanar AND coplanar
        // overlaps (the latter via the Stage-0 re-tessellated meshes' face maps).
        //
        // N4 RETIREMENT (task #53, spec `specs/n4_retire_stage6_fallback.md`):
        // on a lineage-CARRYING input, a provenance MISS is a producer fault
        // and fails LOUDLY — the `YANG_N4_FALLBACK_PROBE` measurement proved
        // zero misses across the full corpus, and a silent geometric guess can
        // misattribute (the failure class N4 eliminated) while masking
        // provenance regressions. The geometric resolution below remains ONLY
        // for LINEAGE-LESS attribution: an arrangement without `source` (the
        // dev-only C++ sidecar oracle and the in-crate mock-label fixtures;
        // reference parity depends on it) or an input without a provenance
        // map (`ProvMiss::NoLineage` — a yang boolean OUTPUT chained directly
        // back in, or a `from_mesh` B-Rep).
        if !la.source.is_empty() {
            match provenance_face_reason(&la.source[orig_t], input, tri_face_a, tri_face_b) {
                Ok(face) => {
                    attributions.push(Some(TriangleAttribution { input, face }));
                    continue;
                }
                // Lineage-less input: the documented geometric path below.
                Err(ProvMiss::NoLineage) => {}
                Err(reason) => {
                    // Env-gated diagnostic naming the miss reason; the error
                    // itself is unconditional.
                    if std::env::var_os("YANG_N4_FALLBACK_PROBE").is_some() {
                        eprintln!(
                            "[n4-fallback] input={input:?} orig_t={orig_t} reason={reason:?} \
                             stage0={} tf_a_len={} tf_b_len={}",
                            stage0.is_some(),
                            tri_face_a.len(),
                            tri_face_b.len(),
                        );
                    }
                    return Err(YangError::FaceResolutionFailed { tri: compact_t });
                }
            }
        }

        // Centroid of the (compact) triangle — same coords as `la.mesh`.
        let tri = kept_submesh.tris[compact_t];
        let p0 = kept_submesh.verts[tri[0] as usize].as_array();
        let p1 = kept_submesh.verts[tri[1] as usize].as_array();
        let p2 = kept_submesh.verts[tri[2] as usize].as_array();
        let c = [
            (p0[0] + p1[0] + p2[0]) / 3.0,
            (p0[1] + p1[1] + p2[1]) / 3.0,
            (p0[2] + p1[2] + p2[2]) / 3.0,
        ];

        // Is this kept triangle DEGENERATE (zero-area / collinear)? The exact
        // arrangement emits sliver triangles along shared solid edges (3
        // distinct welded verts, all collinear). They carry no surface and no
        // volume but pair their edges into the watertight result, so they are
        // kept (not dropped — dropping breaks edge-pairing). Their centroid
        // lands on a solid edge, equidistant from the two adjacent face planes,
        // so the unique-face rule would (wrongly) F3-tie them. Threshold is the
        // M1 area threshold (2·area = ‖cross(e1,e2)‖; compare to MIN_FEATURE_SIZE²;
        // governance A14.3 — shared constant, no ad-hoc epsilon).
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let cross = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let twice_area = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let degenerate =
            twice_area < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE;

        // Distance of the centroid to each labeled-solid face plane. Curved
        // faces are already rejected at `BRep::new`, so this is defensive — but
        // it must compile and be LOUD (P9): a curved arm returns the carrying
        // `Err`, never `unreachable!`/panic. `fi` is the input B-Rep face index.
        // PR-YR27 (Finding 2): a face that went through a Stage-0 pair had
        // its loop vertices SNAPPED onto the pair's CANONICAL plane, so its
        // kept triangles lie on the canonical plane — up to the pair's
        // detection `band` (≫ TAU_WORK) away from the face's STORED plane.
        // Membership for exactly those faces is therefore measured against
        // the canonical pair plane (KEYED to the pair: every non-pair face
        // keeps its stored surface + TAU_WORK byte-for-byte — this is the
        // Stage-1 geometry the snap actually produced, NOT a tolerance
        // widening).
        let stage0_pair_plane = |fi: usize| -> Option<&stage0::PairPlane> {
            stage0.as_ref().and_then(|s0| {
                s0.pairs.iter().find(|p| match input {
                    InputId::A => p.face_a == fi,
                    InputId::B => p.face_b == fi,
                })
            })
        };
        let plane_dist = |fi: usize, face: &BRepFace| -> Result<f64, YangError> {
            if let Some(pp) = stage0_pair_plane(fi) {
                return Ok((pp.n[0] * c[0] + pp.n[1] * c[1] + pp.n[2] * c[2] + pp.d).abs());
            }
            // PR-YR7: delegate to the shared `signed_distance_to_surface`
            // (Plane + Cylinder + Sphere); take `.abs()` (distance to the
            // surface). Cone still rejects loudly — the free function returns a
            // sentinel face index, which we replace with the real input `fi`.
            match signed_distance_to_surface(face.surface, Point3::new(c[0], c[1], c[2])) {
                Ok(d) => Ok(d.abs()),
                Err(YangError::CurvedSurfaceNotYetSupported { .. }) => {
                    Err(YangError::CurvedSurfaceNotYetSupported { face: fi })
                }
                Err(other) => Err(other),
            }
        };

        // PER-FACE membership tolerance (PR-YR8 Blocker 1, spec §4). The
        // membership tolerance is the surface's OWN Stage-1 tessellation chord
        // bound (governance A15 / A14.3 — not tolerance widening): a `Plane`
        // face has zero chord error → `TAU_WORK`; a `Cylinder` face is a
        // `d_ε`-chord approximation BY CONSTRUCTION → its labeled solid's curved
        // chord band `d_ε`, the SAME bound Stage 1 guarantees. Computed once per
        // labeled solid from the SINGLE shared source.
        //
        // A `Cylinder` face implies the solid HAS circle rims, so `band` is
        // `Some`; if it is somehow `None` for a cylinder face that is a genuine
        // producer fault → `FaceResolutionFailed` (do NOT silently default a
        // cylinder face to `TAU_WORK`).
        //
        // For ALL-PLANAR inputs every face uses `TAU_WORK` (planar faces always
        // do; an all-planar solid has `band == None` so no face consults it),
        // making BOTH branches below byte-for-byte the OLD rules — the 900-case
        // box fuzz and the m3/yr5c planar-sliver tests are unaffected.
        let band = curved_chord_bound(input_brep.edges());
        let tol_for = |fi: usize, surface: Surface| -> Result<f64, YangError> {
            match surface {
                // PR-YR27 Finding 2 (completion): a planar face welded onto a
                // Stage-0 canonical pair plane legitimately lies up to the
                // pair's detection `band` from it — the SAME band `plane_dist`
                // above already measures the centroid against. The membership
                // THRESHOLD must match that distance basis, so a pair-plane face
                // uses its pair band; every NON-pair planar face keeps TAU_WORK
                // byte-for-byte (the exact/band tier split below still keys on
                // TAU_WORK, so on-plane triangles stay EXACT hits and the
                // all-planar fuzz corpus is unaffected — this only admits the
                // band-level offset the Stage-0 weld itself introduced, NOT a
                // widening). Without it a coplanar boolean at non-unit model
                // scale (e.g. a 10 mm bearing recess, coords ~1e-2, weld
                // residual ~1e-10 ≫ TAU_WORK) loses its annulus-cap triangles to
                // a spurious FaceResolutionFailed.
                Surface::Plane { .. } => Ok(match stage0_pair_plane(fi) {
                    Some(pp) => pp.band.max(cad_primitives::TAU_WORK),
                    None => cad_primitives::TAU_WORK,
                }),
                Surface::Cylinder { .. } => match band {
                    Some(de) => Ok(de),
                    None => Err(YangError::FaceResolutionFailed { tri: compact_t }),
                },
                // PR-YR15: a Sphere face uses its OWN Stage-1 chord bound
                // `sphere_chord_bound(radius) = 1e-2·2r√3` — the SAME bound
                // Stage 1 guarantees (A15/A14.3, NOT tolerance widening). It is
                // deliberately NOT the Circle-rim `band` (2r√2), which would
                // underestimate the sphere's chord error.
                Surface::Sphere { radius, .. } => Ok(sphere_chord_bound(radius)),
                // PR-YR17: a Cone face uses its OWN Stage-1 chord bound
                // `cone_chord_bound(height, half_angle)` — the SAME bound Stage 1
                // guarantees (A15/A14.3, NOT tolerance widening). The cone height
                // is not in `Surface::Cone` (only apex/axis_dir/half_angle), so it
                // is derived from the cone face's rim `Curve::Circle` edge in its
                // outer loop exactly as the Stage-1 pre-pass does (src/lib.rs
                // ~503-525): `height = |(rim_center − apex)·â|`. This is the live
                // reject site for a Cone (PR-YR16 made
                // `signed_distance_to_surface(Cone)` return `Ok`, so `plane_dist`
                // no longer rejects the cone upstream). If the cone face's outer
                // loop has NO rim Circle, no sound height can be derived → loud
                // `FaceResolutionFailed` (a genuine producer fault; P9 — NEVER a
                // defaulted or widened tolerance).
                Surface::Cone {
                    apex,
                    axis_dir,
                    half_angle,
                } => {
                    let au = normalize3(axis_dir.as_array());
                    let ap = apex.as_array();
                    let mut height: Option<f64> = None;
                    for &e_idx in &input_brep.faces()[fi].outer_loop {
                        if let Curve::Circle { center, .. } =
                            input_brep.edges()[e_idx as usize].curve
                        {
                            let c = center.as_array();
                            height = Some(
                                ((c[0] - ap[0]) * au[0]
                                    + (c[1] - ap[1]) * au[1]
                                    + (c[2] - ap[2]) * au[2])
                                    .abs(),
                            );
                            break;
                        }
                    }
                    match height {
                        Some(h) => Ok(cone_chord_bound(h, half_angle)),
                        None => Err(YangError::FaceResolutionFailed { tri: compact_t }),
                    }
                }
                // KV6d: a torus face uses the rim chord `band` (the rim AABB
                // bound covers the outermost latitude radius major+minor).
                Surface::Torus { .. } => match band {
                    Some(de) => Ok(de),
                    None => Err(YangError::FaceResolutionFailed { tri: compact_t }),
                },
            }
        };

        let face = if degenerate {
            // Degenerate sliver: attribute to the LOWEST face index within ITS
            // per-face tolerance (a zero-area triangle has no area, so which
            // adjacent face it joins is geometrically harmless). Never an F3
            // tie — the tie contract is for *real* (positive-area) triangles.
            //
            // PR-YR8: this branch uses the PER-FACE tolerance, not absolute
            // TAU_WORK. The spec §4 "degenerate branch keeps TAU_WORK" line was
            // written for the planar-only world (slivers only on shared
            // planar-planar solid edges, centroid on both planes within
            // TAU_WORK). It did not foresee a sliver lying ON a tessellated
            // CYLINDER face: the sidecar arrangement emits a near-zero-area
            // sliver on the cylinder lateral surface whose centroid is ~d_ε
            // inside the analytic cylinder (within the Stage-1 bound, but ≫
            // TAU_WORK). The governing PRINCIPLE (§4 Blocker 1: test membership
            // at the surface's own Stage-1 chord bound) applies to ANY triangle
            // on the cylinder face, degenerate or not. For all-planar inputs
            // this stays byte-identical (every tol = TAU_WORK). If no face is
            // within tolerance, that is a genuine producer fault → loud (P9).
            let mut hit: Option<u32> = None;
            for (fi, f) in input_brep.faces().iter().enumerate() {
                if plane_dist(fi, f)? < tol_for(fi, f.surface)? {
                    hit = Some(fi as u32);
                    break;
                }
            }
            match hit {
                Some(fi) => fi,
                None => return Err(YangError::FaceResolutionFailed { tri: compact_t }),
            }
        } else {
            // PR-YR20 tiered tie-break: an EXACT membership (centroid within
            // TAU_WORK of the surface — it lies ON it) dominates a
            // within-chord-band membership. Each face still uses its own A14.3
            // band via tol_for; we only rank the tie by tier. For all-planar
            // inputs every hit is EXACT (planar tol == TAU_WORK), so a unique
            // hit is byte-for-byte the old "exactly one face within TAU_WORK"
            // rule.
            let mut exact_hits: Vec<u32> = Vec::new();
            let mut band_hits: Vec<u32> = Vec::new();
            for (fi, f) in input_brep.faces().iter().enumerate() {
                let d = plane_dist(fi, f)?;
                if d < tol_for(fi, f.surface)? {
                    if d < cad_primitives::TAU_WORK {
                        exact_hits.push(fi as u32);
                    } else {
                        band_hits.push(fi as u32);
                    }
                }
            }
            // PR-YR27 (Finding 3): a multi-hit tier is narrowed by FINITE-
            // EXTENT strict containment before it is declared a tie. The
            // infinite-plane rule alone false-positives whenever a kept
            // triangle's centroid happens to lie bit-exactly ON another
            // face's plane (the L-profile CDT class: cap triangle
            // (0,0),(2,0),(1,1) → centroid x = 1 = the x=1 side plane;
            // likewise a chained input carrying two same-plane faces). The
            // TRUE owning face strictly contains the centroid of every
            // positive-area kept triangle attributed to it; the false
            // positive at best touches its trimmed region's boundary —
            // strictness is therefore sound and load-bearing. Faces the
            // exact 2D test cannot decide (curved surfaces / curved loop
            // edges → `None`) are NEVER excluded, so an undecidable tie
            // stays the loud error (P9 — containment breaks ties, it never
            // widens membership; a unique hit is accepted without it,
            // byte-identical to the old rule).
            let narrow = |hits: Vec<u32>| -> Result<Option<u32>, YangError> {
                match hits.len() {
                    0 => Ok(None),
                    1 => Ok(Some(hits[0])),
                    _ => {
                        let kept: Vec<u32> = hits
                            .into_iter()
                            .filter(|&fi| {
                                point_strictly_in_planar_face(input_brep, fi as usize, c)
                                    != Some(false)
                                    && point_strictly_in_cylinder_face_axially(
                                        input_brep,
                                        fi as usize,
                                        c,
                                    ) != Some(false)
                            })
                            .collect();
                        match kept.len() {
                            1 => Ok(Some(kept[0])),
                            // 0 (centroid on every tied face's boundary) — loud.
                            0 => Err(YangError::FaceResolutionFailed { tri: compact_t }),
                            // ≥2 survivors. SAME-SURFACE TIE: faces sharing
                            // IDENTICAL surface geometry are INTERCHANGEABLE for
                            // attribution — a triangle on that surface belongs to
                            // it no matter which fragment owns it, and topology
                            // reconstruction regroups them by adjacency into one
                            // output face. This arises when one analytic surface
                            // is SPLIT into several faces — e.g. a cylindrical
                            // bore fragmented into arc-faces by the
                            // tessellated-polygon profile fallback (gear bores).
                            // Pick the lowest index: NOT silent-wrong (same
                            // surface), unlike a tolerance widening. A tie among
                            // GEOMETRICALLY DISTINCT surfaces stays the loud error
                            // (P9 — genuinely ambiguous).
                            _ => {
                                let s0 = input_brep.faces()[kept[0] as usize].surface;
                                if kept
                                    .iter()
                                    .all(|&fi| input_brep.faces()[fi as usize].surface == s0)
                                {
                                    Ok(kept.iter().copied().min())
                                } else {
                                    Err(YangError::FaceResolutionFailed { tri: compact_t })
                                }
                            }
                        }
                    }
                }
            };
            match narrow(exact_hits)? {
                Some(fi) => fi, // exact tier dominates
                None => match narrow(band_hits)? {
                    Some(fi) => fi,
                    None => return Err(YangError::FaceResolutionFailed { tri: compact_t }),
                },
            }
        };
        attributions.push(Some(TriangleAttribution { input, face }));
    }
    let mut triangle_attribution = TriangleAttributionMap { attributions };

    // (6) Topology reconstruction + Stage-4 relocation (PR-YR10). Stage 4 may
    // relocate intersection vertices in-place (onto the exact curves) and, on a
    // §4.5.3 reversal, edge-collapse a mesh vertex — mutating BOTH the mesh and
    // the attribution in lockstep — so both are passed by `&mut` and the
    // tessellation sources come back from `reconstruct_topology`.
    let mut kept_submesh = kept_submesh;
    let (vertices, edges, faces, sources, face_attribution) =
        reconstruct_topology_stage4(&mut kept_submesh, &mut triangle_attribution, a, b, op)?;

    let tessellation = TessellationMap { sources };

    Ok(BRep {
        vertices,
        edges,
        faces,
        mesh: kept_submesh,
        tessellation,
        triangle_attribution,
        face_attribution,
        // A boolean-output BRep has no Stage-1 face_tri_ranges lineage; leave the
        // provenance map empty so a CHAINED boolean falls back to geometric
        // attribution (until the output reconstruction also emits a tri→face map).
        tri_face: Vec::new(),
        forced_rim_n: None,
    })
}

// =========================================================================
// PR-YR5 — topology reconstruction
// =========================================================================

/// PR-YR5 internal: the triple `(vertices, edges, faces)` produced
/// by `reconstruct_topology` to populate the output `BRep`.
///
/// PR-YR10: extended with a fourth component — the per-output-mesh-vertex
/// `Vec<TessellationSource>` (default `BRepVertex(i)`, overridden to
/// `BRepEdge { edge, t }` for Stage-4-relocated intersection vertices).
type ReconstructedTopology = (
    Vec<BRepVertex>,
    Vec<BRepEdge>,
    Vec<BRepFace>,
    Vec<TessellationSource>,
    // PR-KV13 F2: per-output-face attribution, parallel to `faces` — the
    // `(input, face)` the patch descends from (the kernel maps it to the
    // operand's persistent face id for boolean provenance).
    Vec<TriangleAttribution>,
);

/// PR-YR5/9 `(vertices, edges, faces)` triple — the pre-PR-YR10 reconstruction
/// shape retained for the `#[cfg(test)]` unit-test callers.
#[cfg(test)]
type LegacyTopology = (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>);

/// PR-YR9 (lifted to module scope in PR-YR10 so `stage4_relocate_and_correct`
/// can consume the same ordered, oriented patch loops + inherited surface that
/// the Phase-B emission uses — no re-derivation, no classification drift).
struct PatchInfo {
    cycles: Vec<Vec<(u32, u32)>>,
    input: InputId,
    inherited: Surface,
    face_idx: usize,
    /// The INPUT face's cavity sense (PR-KV6b-1): a kept patch of an
    /// already-reversed input wall (e.g. a washer's inner tube) must keep
    /// its sense in the output — composed by XOR with the Subtract-B flip.
    input_reversed: bool,
    /// Spec yang_stage6_sliver_topology §2/§4B: this patch contained ≥1 FOLD
    /// sliver that §4A excluded from boundary derivation (`patch_fold_slivers`).
    /// Such a patch may carry a whole shared solid edge as ONE un-subdivided
    /// chord (the collapsed subdivision the slivers used to represent), so it
    /// — and ONLY it — is eligible for the §4B loop T-subdivision. Patches
    /// with no excluded fold sliver keep byte-identical loops (the measured
    /// chord lives on the fold-bearing side; the other side already
    /// subdivides), which keeps curved / benign-T-junction output at exact
    /// reference parity.
    had_fold_sliver: bool,
}

/// PR-YR10: the Phase-A structures `reconstruct_topology` derives before the
/// Phase-B emission: per-patch ordered loops + inherited surface (`infos`), the
/// edge→incident-(input,surface) map (`incidence`), and the exact per-edge
/// analytical `Curve` map (`curves`). Recomputed after a §4.5.3 collapse.
type PhaseA = (
    Vec<PatchInfo>,
    std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>>,
    std::collections::BTreeMap<(u32, u32), Curve>,
);

/// PR-YR10: compute the Phase-A structures (adjacency → patches → cycles →
/// incidence → exact intersection curves) from the current mesh + attribution.
/// Factored out of `reconstruct_topology` so it can be re-run after a §4.5.3
/// collapse mutates the mesh.
fn compute_phase_a(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
) -> Result<PhaseA, YangError> {
    let adjacency = triangle_adjacency(mesh);
    let patches = flood_fill_patches(mesh, attribution, &adjacency);
    // PR-YR27 (Finding 1a): merge edge-adjacent patches lying on the SAME
    // plane with the SAME orientation into one output face — a coplanar
    // boolean otherwise emits e.g. A's and B's side fragments as two faces
    // on one bit-identical plane, and the NEXT boolean in a chain
    // exact-ties between them. Non-adjacent same-plane patches stay
    // separate faces (their union is not a single connected face).
    let patches = merge_same_plane_patches(patches, &adjacency, a, b);

    let mut infos: Vec<PatchInfo> = Vec::with_capacity(patches.len());
    for patch in &patches {
        let cycles = patch_boundary_cycle(patch, mesh)?;
        let input = patch.attribution.input;
        let input_brep = match input {
            InputId::A => a,
            InputId::B => b,
        };
        let face_idx = patch.attribution.face as usize;
        if face_idx >= input_brep.faces().len() {
            return Err(YangError::MalformedTopology(format!(
                "attribution.face = {face_idx} out of range (input has {} faces)",
                input_brep.faces().len()
            )));
        }
        let inherited = input_brep.faces()[face_idx].surface;
        let input_reversed = input_brep.faces()[face_idx].reversed;
        let had_fold_sliver = !patch_fold_slivers(patch, mesh).is_empty();
        infos.push(PatchInfo {
            cycles,
            input,
            inherited,
            face_idx,
            input_reversed,
            had_fold_sliver,
        });
    }

    let mut incidence: std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>> =
        std::collections::BTreeMap::new();
    for info in &infos {
        for cycle in &info.cycles {
            for &(s, e) in cycle {
                let key = if s < e { (s, e) } else { (e, s) };
                incidence
                    .entry(key)
                    .or_default()
                    .push((info.input, info.inherited));
            }
        }
    }
    let curves = build_intersection_curves(&incidence, mesh, a, b)?;
    Ok((infos, incidence, curves))
}

/// PR-YR27 (Finding 1a): merge edge-adjacent output patches whose inherited
/// planes are the same plane with the same orientation (bit-identical or
/// within `TAU_WORK` on the UNIT-normalized `(n̂, d̂)`) into ONE patch, so
/// Stage 6 emits one face per connected same-plane region of the output
/// solid.
///
/// Why: a coplanar boolean's output legitimately carries triangles from
/// BOTH inputs' faces on one geometric plane (e.g. exactly stacked boxes:
/// each side plane has an A fragment and a B fragment, edge-adjacent along
/// the seam). `flood_fill_patches` groups by attribution, so those
/// fragments emit as TWO faces on a bit-identical plane — a fragmented
/// B-Rep whose NEXT boolean exact-ties Stage-6 membership between them
/// (assay F0066). Merging is keyed to edge adjacency: non-adjacent
/// same-plane patches (genuinely separate faces) are NOT merged.
///
/// Safety / blast radius:
/// - Only `Surface::Plane` patches participate; the orientation test
///   (component-wise `|n̂ᵢ−n̂ⱼ| ≤ TAU_WORK`) means an opposite-normal pair
///   (e.g. a subtract cavity wall against an outer wall) NEVER merges.
/// - Distinct input faces on one plane only exist when an input itself
///   carries same-plane faces or the two inputs share a plane — exactly
///   the coplanar classes; every other fixture has zero mergeable pairs
///   and is byte-identical.
/// - The merged patch's attribution is the lexicographically smallest
///   member `(input, face)` (deterministic); the members' inherited
///   surfaces agree within `TAU_WORK`, so the choice is geometric noise.
/// - The seam edges become patch-INTERIOR (they vanish from the boundary
///   cycles and therefore from the output edge set) — the merged region's
///   single outer cycle is exactly the §4.5.5 result-face boundary.
fn merge_same_plane_patches(
    mut patches: Vec<Patch>,
    adjacency: &[Vec<u32>],
    a: &BRep,
    b: &BRep,
) -> Vec<Patch> {
    if patches.len() < 2 {
        return patches;
    }

    // Inherited surface key per patch (`None` = unmergeable surface kind or
    // degenerate — never merged). A `Plane` keys on its unit `(n̂, d̂)`; a
    // `Cylinder` keys on its unit axis, an axis-line anchor (the axis point
    // projected to remove the free axial slide), the radius, AND the effective
    // outward sense (`reversed`) — two coincident cylinders of OPPOSITE sense
    // (a bore wall vs an outer wall) must NEVER merge (PR-5; mirrors the planar
    // opposite-normal guard). Spheres/cones keep `None` (not yet needed).
    enum SurfKey {
        Plane {
            n: [f64; 3],
            d: f64,
        },
        Cyl {
            axis: [f64; 3],
            anchor: [f64; 3],
            radius: f64,
            reversed: bool,
        },
    }
    let keys: Vec<Option<SurfKey>> = patches
        .iter()
        .map(|p| {
            let brep = match p.attribution.input {
                InputId::A => a,
                InputId::B => b,
            };
            let f = brep.faces().get(p.attribution.face as usize)?;
            match f.surface {
                Surface::Plane { normal, d } => {
                    let n = normal.as_array();
                    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                    if len < cad_primitives::MIN_FEATURE_SIZE {
                        return None;
                    }
                    Some(SurfKey::Plane {
                        n: [n[0] / len, n[1] / len, n[2] / len],
                        d: d / len,
                    })
                }
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                } => {
                    let ad = axis_dir.as_array();
                    let len = (ad[0] * ad[0] + ad[1] * ad[1] + ad[2] * ad[2]).sqrt();
                    if len < cad_primitives::MIN_FEATURE_SIZE {
                        return None;
                    }
                    let axis = [ad[0] / len, ad[1] / len, ad[2] / len];
                    // Anchor = axis_point with its axial component removed, so
                    // two cylinders sharing one axis LINE but with axis points at
                    // different axial offsets get an identical anchor.
                    let ap = axis_point.as_array();
                    let t = ap[0] * axis[0] + ap[1] * axis[1] + ap[2] * axis[2];
                    let anchor = [
                        ap[0] - t * axis[0],
                        ap[1] - t * axis[1],
                        ap[2] - t * axis[2],
                    ];
                    Some(SurfKey::Cyl {
                        axis,
                        anchor,
                        radius,
                        reversed: f.reversed,
                    })
                }
                _ => None,
            }
        })
        .collect();
    let mergeable = |i: usize, j: usize| -> bool {
        match (&keys[i], &keys[j]) {
            (Some(SurfKey::Plane { n: ni, d: di }), Some(SurfKey::Plane { n: nj, d: dj })) => {
                (di - dj).abs() <= cad_primitives::TAU_WORK
                    && (0..3).all(|k| (ni[k] - nj[k]).abs() <= cad_primitives::TAU_WORK)
            }
            (
                Some(SurfKey::Cyl {
                    axis: ai,
                    anchor: anchi,
                    radius: ri,
                    reversed: revi,
                }),
                Some(SurfKey::Cyl {
                    axis: aj,
                    anchor: anchj,
                    radius: rj,
                    reversed: revj,
                }),
            ) => {
                // Same effective sense, equal radius, parallel axes, same axis
                // line (anchors agree up to TAU_WORK; axes may be antiparallel —
                // a cylinder's axis_dir sign is free — so compare |aᵢ·aⱼ|≈1).
                revi == revj
                    && (ri - rj).abs() <= cad_primitives::TAU_WORK
                    && (ai[0] * aj[0] + ai[1] * aj[1] + ai[2] * aj[2]).abs()
                        >= 1.0 - cad_primitives::TAU_WORK
                    && (0..3).all(|k| (anchi[k] - anchj[k]).abs() <= cad_primitives::TAU_WORK)
            }
            _ => false,
        }
    };

    // patch index per mesh triangle.
    let mut patch_of: Vec<usize> = vec![usize::MAX; adjacency.len()];
    for (pi, p) in patches.iter().enumerate() {
        for &t in &p.tri_indices {
            patch_of[t as usize] = pi;
        }
    }

    // Union-find over patches, united on (edge-adjacent AND same-plane).
    let mut parent: Vec<usize> = (0..patches.len()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]]; // path halving
            x = parent[x];
        }
        x
    }
    for (pi, p) in patches.iter().enumerate() {
        for &t in &p.tri_indices {
            for &u in &adjacency[t as usize] {
                let pj = patch_of[u as usize];
                if pj == usize::MAX || pj == pi {
                    continue;
                }
                if mergeable(pi, pj) {
                    let (ri, rj) = (find(&mut parent, pi), find(&mut parent, pj));
                    if ri != rj {
                        parent[ri.max(rj)] = ri.min(rj);
                    }
                }
            }
        }
    }

    // Rebuild merged patches in first-member order (deterministic; a strict
    // no-op — same patches, same order — when nothing merged).
    let roots: Vec<usize> = (0..patches.len()).map(|i| find(&mut parent, i)).collect();
    let mut out: Vec<Patch> = Vec::with_capacity(patches.len());
    let mut taken = vec![false; patches.len()];
    for i in 0..patches.len() {
        if taken[i] {
            continue;
        }
        let members: Vec<usize> = (i..patches.len())
            .filter(|&j| roots[j] == roots[i])
            .collect();
        for &m in &members {
            taken[m] = true;
        }
        let attribution = members
            .iter()
            .map(|&m| patches[m].attribution)
            .min()
            .expect("members is non-empty");
        let mut tri_indices: Vec<u32> = Vec::new();
        for &m in &members {
            tri_indices.append(&mut patches[m].tri_indices);
        }
        out.push(Patch {
            attribution,
            tri_indices,
        });
    }
    out
}

/// PR-YR15 helper: the Stage-1 curved chord bound of ONE input, choosing the
/// surface's OWN bound (A14.3 / I-sphere-band). A `Surface::Sphere` face's
/// tessellation vertices sit off the exact great circle by up to the sphere's
/// own `sphere_chord_bound(radius) = 1e-2·2r√3`, which is LARGER than the
/// rim-AABB `curved_chord_bound` (2r√2) — so a sphere-bearing input must report
/// its sphere bound, NOT the rim band (which would underestimate and reject
/// valid sphere-rim vertices). Cylinder/all-planar inputs keep the rim-AABB
/// `curved_chord_bound` byte-for-byte. When both are present we take the MAX
/// (the budget must admit every curved-surface vertex). `None` only for an
/// all-planar input (zero chord error). This is the SINGLE source consulted by
/// both `build_intersection_curves` (selection tol) and `stage4_chord_band`
/// (relocation budget); it is NOT tolerance widening.
fn input_curved_chord_bound(brep: &BRep) -> Option<f64> {
    let rim = curved_chord_bound(brep.edges());
    let sphere = brep
        .faces()
        .iter()
        .filter_map(|f| match f.surface {
            Surface::Sphere { radius, .. } => Some(sphere_chord_bound(radius)),
            _ => None,
        })
        .fold(None, |acc: Option<f64>, b| {
            Some(acc.map_or(b, |a| a.max(b)))
        });
    match (rim, sphere) {
        (Some(r), Some(s)) => Some(r.max(s)),
        (Some(r), None) => Some(r),
        (None, s) => s,
    }
}

/// PR-YR10 helper: the Stage-4 chord-band relocation budget `d_ε` — the
/// Stage-1 chord bound of whichever input bears a curved surface (the curved
/// solid). Uses [`input_curved_chord_bound`] so a sphere input reports its OWN
/// (larger) 2r√3 bound, not the rim-AABB 2r√2 (I-sphere-band). `None` only if
/// NEITHER input has a curved surface, which cannot happen when a conic
/// intersection edge exists (a conic edge implies a curved input).
fn stage4_chord_band(a: &BRep, b: &BRep) -> Option<f64> {
    // PR-KV7: the MAX of the two inputs' Stage-1 bounds, not A-with-B-
    // fallback. An arrangement vertex on an A×B intersection curve sits on
    // the curved OWNER's facet chord, off the exact curve by up to that
    // owner's OWN sagitta — and with chainable boolean outputs the owner
    // can be EITHER input (a recovered body re-entering as A can have a
    // much tighter rim AABB than the fresh operand B whose curves are
    // being relocated). `max` admits exactly up to the looser owner's
    // honest Stage-1 bound for this model pair — a derived bound, not
    // tolerance widening. (Per-curve owner resolution, as Stage-3's
    // `chord_tol_for_curved_owner` does for selection, is the M5-era
    // refinement; `max` is its conservative envelope.)
    match (input_curved_chord_bound(a), input_curved_chord_bound(b)) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    }
}

/// PR-YR10 helper: edge-collapse `victim` onto `survivor` in `mesh` + the
/// parallel `attribution`. Replaces every `victim` index with `survivor`, then
/// drops the now-degenerate triangles (two equal indices) from BOTH the mesh
/// and the attribution in lockstep. A proper edge-collapse preserves the
/// watertight half-edge pairing (the two collapsed slivers' surviving directed
/// edges are mutual opposites that cancel — spec §4.5.3 / boolean() sliver rule
/// at the compaction step). Returns the number of triangles dropped.
fn collapse_vertex(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    victim: u32,
    survivor: u32,
) -> usize {
    let mut new_tris: Vec<[u32; 3]> = Vec::with_capacity(mesh.tris.len());
    let mut new_attr: Vec<Option<TriangleAttribution>> = Vec::with_capacity(attribution.len());
    let mut dropped = 0usize;
    for (t, tri) in mesh.tris.iter().enumerate() {
        let mapped = [
            if tri[0] == victim { survivor } else { tri[0] },
            if tri[1] == victim { survivor } else { tri[1] },
            if tri[2] == victim { survivor } else { tri[2] },
        ];
        if mapped[0] == mapped[1] || mapped[1] == mapped[2] || mapped[2] == mapped[0] {
            dropped += 1;
            continue;
        }
        new_tris.push(mapped);
        new_attr.push(attribution.get(t).copied().flatten());
    }
    *mesh = Mesh::new(std::mem::take(&mut mesh.verts), new_tris);
    *attribution = new_attr;
    dropped
}

/// PR-YR11 helper: drop mesh vertices no surviving triangle references and remap
/// triangle indices + the Stage-4 `relocations` keys to the dense vertex set.
///
/// A §4.5.3 [`collapse_vertex`] keeps the full vertex array (it only drops the
/// now-degenerate triangles), leaving the collapsed-away vertices DANGLING. The
/// internal per-shell `check_watertight_2manifold` gate ignores them (it sums V
/// over triangle-referenced verts only), but they inflate a caller's GLOBAL
/// `V − E + F`. An output mesh must carry no unreferenced vertices, so this
/// compaction runs after Stage 4. It is a strict NO-OP (returns early, mesh and
/// `relocations` untouched) when every vertex is already referenced — so the
/// no-collapse paths (planar / perpendicular-circle / on-curve mock) stay
/// byte-identical.
fn compact_unreferenced_verts(mesh: &mut Mesh, relocations: &mut Vec<(u32, f64)>) {
    let n = mesh.verts.len();
    let mut referenced = vec![false; n];
    for tri in &mesh.tris {
        for &v in tri {
            referenced[v as usize] = true;
        }
    }
    if referenced.iter().all(|&r| r) {
        return; // no danglers — byte-identical no-op.
    }
    // Dense remap preserving the relative order of surviving vertices.
    let mut remap: Vec<Option<u32>> = vec![None; n];
    let mut new_verts: Vec<Point3> = Vec::with_capacity(n);
    for (i, &r) in referenced.iter().enumerate() {
        if r {
            remap[i] = Some(new_verts.len() as u32);
            new_verts.push(mesh.verts[i]);
        }
    }
    let new_tris: Vec<[u32; 3]> = mesh
        .tris
        .iter()
        .map(|tri| {
            // Invariant: `referenced` was built from this same triangle list
            // above, so every triangle vertex has a `Some` remap entry.
            tri.map(|v| {
                remap[v as usize]
                    .expect("compact_unreferenced_verts: triangle vertex not marked referenced")
            })
        })
        .collect();
    *mesh = Mesh::new(new_verts, new_tris);
    // Remap (and drop) relocation keys: a relocation referencing a collapsed-away
    // (now-unreferenced) vertex is no longer in the mesh, so it is dropped.
    let remapped: Vec<(u32, f64)> = relocations
        .iter()
        .filter_map(|&(v, t)| remap[v as usize].map(|nv| (nv, t)))
        .collect();
    *relocations = remapped;
}

/// PR-YR10 (Yang §4.4.1 + §4.5.3): Stage 4 — relocate the mesh intersection
/// points onto the exact analytical `Circle` curves, then correct any reversed
/// intersection points by the §4.5.3 polyline-tangent sweep.
///
/// Returns `(relocations, collapsed)` where `relocations` is the list of
/// `(vertex, t)` pairs (the circle-frame angle `t` for every relocated OR
/// already-on-curve intersection vertex — the caller maps these to
/// `BRepEdge { edge, t }` tessellation sources once the output edges exist), and
/// `collapsed` is `true` iff the §4.5.3 sweep edge-collapsed at least one
/// vertex (so the caller must recompute Phase A).
///
/// LOUD STOPs (P9/P10), never a silent snap / tolerance widening / no-op:
/// - `Stage4RegionInvalid { OnAxis }` — a point projects onto the circle/cylinder
///   axis.
/// - `Stage4RegionInvalid { OffCurveBeyondChordBand }` — residual `ρ > d_ε`.
/// - `Stage4RegionInvalid { LoopTooSmall }` — a loop shrank below 3 verts.
/// - `Stage4RegionInvalid { InvertedTriangle / DegenerateTriangle }` — a
///   relocated triangle is inverted / degenerate after correction.
/// - `Stage4ReversalUnresolved` — the §4.5.3 sweep could not resolve a reversal.
/// - `Stage4RegionInvalid { LocalRefinementRequired }` — relocate + §4.5.3 left
///   a region invalid (genuine §4.5.2 territory, out of scope).
///
/// No-skip audit (anti-disproven-attempt): a `processed` set tracks EVERY conic
/// edge endpoint; it must equal the relocation-key set at the end. The function
/// NEVER `continue`s past a `Circle` edge endpoint.
fn stage4_relocate_and_correct(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
) -> Result<(Vec<(u32, f64)>, bool), YangError> {
    use std::collections::{BTreeMap, HashSet};

    // d_ε relocation budget (a conic edge implies a curved input ⇒ Some).
    let d_eps = match stage4_chord_band(a, b) {
        Some(de) => de,
        None => {
            // A conic edge with no circle-bearing input is a producer fault;
            // never default to TAU_WORK for a curved relocation (P10).
            return Err(YangError::Stage4RegionInvalid {
                vertex: u32::MAX,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
    };

    // (1) Collect + classify every conic-edge endpoint from the CURRENT Phase A.
    // PR-YR11: the incidence map (no longer discarded) supplies the TRUE cylinder
    // + cutting plane per Ellipse edge for the closed-form cylinder relocation.
    let (_infos0, inc0, curves0) = compute_phase_a(mesh, attribution, a, b)?;

    // Per-vertex Circle assignment (deterministic via BTreeMap). PR-YR19: the
    // 4th tuple element carries the originating sphere radius `Some(R)` for a
    // sphere section circle (else `None`) so the relocation guard can scale the
    // in-plane radial band by `(R/r_c)` (spec §2/§4 Site 2).
    let mut vert_circle: BTreeMap<u32, (Point3, Vector3, f64, Option<f64>)> = BTreeMap::new();
    // PR-YR11: per-vertex Ellipse relocation data (the true cylinder + plane +
    // stored ellipse), analogous to `vert_circle`.
    let mut vert_ellipse: BTreeMap<u32, EllipseReloc> = BTreeMap::new();
    // PR-YR21: per-vertex cone-ellipse relocation data (the true cone + plane +
    // stored ellipse + the cone's OWN chord budget), for a `cone ∩ plane`
    // oblique section. Kept separate from `vert_ellipse` (cylinder) so the
    // cylinder path stays byte-identical.
    let mut vert_cone_ellipse: BTreeMap<u32, ConeEllipseReloc> = BTreeMap::new();
    // PR-YR22: per-vertex cone-parabola relocation data for a `cone ∩ plane` θ=α
    // (generator-parallel) section. Kept separate from the ellipse maps so the
    // ellipse/cylinder paths stay byte-identical.
    let mut vert_parabola: BTreeMap<u32, ConeParabolaReloc> = BTreeMap::new();
    // PR-YR23: per-vertex cone-hyperbola relocation data for a `cone ∩ plane`
    // axis-parallel (HYPE) section. Kept separate from the other conic maps so
    // the ellipse/cylinder/parabola paths stay byte-identical.
    let mut vert_cone_hyperbola: BTreeMap<u32, ConeHyperbolaReloc> = BTreeMap::new();
    // PR-F3: per-vertex ruling-LINE relocation data for a plane∥axis ×
    // cylinder intersection edge (ssi C3a/C3b). A `Curve::LineSegment`
    // intersection edge whose incidence carries a CYLINDER is such a line; its
    // arrangement points sit on Stage-1 facet chords, off the exact line (and
    // off the cylinder) by up to the sagitta — they need relocation exactly
    // like the conic arms. Plane∩plane segments are exact and stay skipped.
    let mut vert_line: BTreeMap<u32, LineReloc> = BTreeMap::new();
    // PR-KV9: a vertex shared by TWO DIFFERENT ellipse edges (the crossing
    // points of the Steinmetz cyl×cyl pair) must land on BOTH curves — the
    // exact junction is `(plane₁ ∩ plane₂) line ∩ cylinder`. Detected at
    // insert time (a silent overwrite would relocate one ellipse's endpoint
    // onto the other, collapsing the seam).
    let mut vert_ell_junction: BTreeMap<u32, (EllipseReloc, EllipseReloc)> = BTreeMap::new();
    // M8 disc∩disc CROSSING: a vertex shared by TWO DIFFERENT coplanar CIRCLE
    // edges (the lens corners of two overlapping coplanar cap rims) must land on
    // BOTH circles — the exact junction is the closed-form circle∩circle
    // intersection in their shared plane. Detected at insert time (a silent
    // overwrite would relocate it onto only the last-scanned circle, leaving the
    // other arc's endpoint off-circle by the lens displacement — the kernel-v2
    // "output arc endpoint does not lie on its circle" reject). The circle analog
    // of `vert_ell_junction`.
    let mut vert_circle_junction: BTreeMap<u32, (CircleAssign, CircleAssign)> = BTreeMap::new();
    // PR-KV11: per-vertex plane∩plane intersection-LINE incidences. The pp
    // segments themselves are exact (skipped), but their ENDPOINT on a
    // chordized curved lateral is a TRIPLE point (e.g. capA∩faceB line ×
    // lateral ellipse): the arrangement vertex lies exactly ON the line but
    // only chord-close to the cylinder, so relocating it onto the conic
    // alone slides it OFF the line (off the cap plane — the F0046 Newell
    // disagreement). Collected here; resolved into `vert_ell_junction`
    // after the scan (the junction is `(plane ∩ plane) ∩ cylinder`, the
    // same closed form as the ellipse×ellipse box-edge junction).
    let mut vert_pp_planes: BTreeMap<u32, Vec<(Vector3, f64, Vector3, f64)>> = BTreeMap::new();
    // PR-KV11: junction-aware insertion, shared by BOTH ellipse arms
    // (cylinder+plane AND cylinder×cylinder). A vertex already assigned a
    // DIFFERENT ellipse (the box-edge crossing of two cylinder∩plane
    // sections, or the Steinmetz cyl×cyl crossing) is demoted to the
    // junction map; a silent overwrite would relocate it onto only the
    // last-scanned ellipse, leaving it off the first by the Stage-1 chord
    // error (the F0046-class "endpoint does not lie on its ellipse").
    fn insert_ellipse_or_junction(
        v: u32,
        er: EllipseReloc,
        vert_ellipse: &mut BTreeMap<u32, EllipseReloc>,
        vert_ell_junction: &mut BTreeMap<u32, (EllipseReloc, EllipseReloc)>,
        endpoints: &mut Vec<u32>,
    ) {
        if let Ok(list) = std::env::var("YANG_V_PROBE") {
            if list.split(',').any(|t| t.trim().parse::<u32>() == Ok(v)) {
                eprintln!(
                    "YANG_V_PROBE insert_ellipse v={v} plane_n={:?} plane_d={:.17e} center={:?}",
                    er.plane_n, er.plane_d, er.center,
                );
            }
        }
        if let Some(prev) = vert_ellipse.get(&v).copied() {
            let same = prev.plane_d == er.plane_d
                && prev.plane_n.as_array() == er.plane_n.as_array()
                && prev.center.as_array() == er.center.as_array();
            if !same {
                vert_ellipse.remove(&v);
                vert_ell_junction.insert(v, (prev, er));
                endpoints.push(v);
                return;
            }
        } else if vert_ell_junction.contains_key(&v) {
            // Already a junction of two ellipses; a third co-incident
            // section adds no relocation freedom (the junction point is
            // fully determined by line ∩ cylinder).
            endpoints.push(v);
            return;
        }
        vert_ellipse.insert(v, er);
        endpoints.push(v);
    }
    // M8 disc∩disc: insert a CIRCLE assignment, demoting to `vert_circle_junction`
    // when the vertex already carries a DIFFERENT circle (the lens corner of two
    // coplanar cap rims). Mirrors `insert_ellipse_or_junction`.
    fn insert_circle_or_junction(
        v: u32,
        ca: CircleAssign,
        vert_circle: &mut BTreeMap<u32, CircleAssign>,
        vert_circle_junction: &mut BTreeMap<u32, (CircleAssign, CircleAssign)>,
        endpoints: &mut Vec<u32>,
    ) {
        if let Some(prev) = vert_circle.get(&v).copied() {
            // Same circle (two arcs of ONE split circle meet here) → keep single.
            let same = prev.0.as_array() == ca.0.as_array()
                && prev.1.as_array() == ca.1.as_array()
                && prev.2 == ca.2;
            if !same {
                vert_circle.remove(&v);
                vert_circle_junction.insert(v, (prev, ca));
                endpoints.push(v);
                return;
            }
        } else if vert_circle_junction.contains_key(&v) {
            // Already a circle∩circle junction; a third co-incident circle adds
            // no relocation freedom (the junction is fully determined by the
            // first two), so don't overwrite — just keep it an endpoint.
            endpoints.push(v);
            return;
        }
        vert_circle.insert(v, ca);
        endpoints.push(v);
    }
    let mut endpoints: Vec<u32> = Vec::new();
    if let Ok(list) = std::env::var("YANG_V_PROBE") {
        let probed: Vec<u32> = list
            .split(',')
            .filter_map(|t| t.trim().parse::<u32>().ok())
            .collect();
        for (&(s, e), curve) in &curves0 {
            if probed.contains(&s) || probed.contains(&e) {
                eprintln!("YANG_V_PROBE curves0 edge ({s},{e}) curve={curve:?}");
            }
        }
    }
    for (&(s, e), curve) in &curves0 {
        match *curve {
            Curve::Parabola {
                vertex,
                normal,
                axis_dir,
                focal_length: _, // recovered from the output edge in eval_source.
            } => {
                // PR-YR22: identify the TRUE cone + cutting plane from this edge's
                // incidence (the θ=α generator-parallel section), mirroring the
                // cone-ellipse arm. Carry the cone's owning `InputId` so its chord
                // budget can be derived from its rim Circle.
                let key = if s < e { (s, e) } else { (e, s) };
                let entries = inc0.get(&key);
                let mut cone: Option<(InputId, Point3, Vector3, f64)> = None;
                let mut plane: Option<(Vector3, f64)> = None;
                if let Some(entries) = entries {
                    for &(input, surf) in entries {
                        match surf {
                            Surface::Cone {
                                apex,
                                axis_dir: cone_axis,
                                half_angle,
                            } => cone = Some((input, apex, cone_axis, half_angle)),
                            Surface::Plane { normal: pn, d: pd } => plane = Some((pn, pd)),
                            _ => {}
                        }
                    }
                }
                let (Some((cone_input, apex, cone_axis_dir, half_angle)), Some((plane_n, plane_d))) =
                    (cone, plane)
                else {
                    // A parabola section that is not a cone+plane pair is out of
                    // scope (producer fault). Loud STOP (P9/P10), mirroring the
                    // cone-ellipse `_ =>` arm.
                    return Err(YangError::Stage4RegionInvalid {
                        vertex: s,
                        reason: Stage4InvalidReason::LocalRefinementRequired,
                    });
                };
                let owner = match cone_input {
                    InputId::A => a,
                    InputId::B => b,
                };
                let Some(cone_d_eps) =
                    cone_chord_budget_from_owner(apex, cone_axis_dir, half_angle, owner)
                else {
                    return Err(YangError::Stage4RegionInvalid {
                        vertex: s,
                        reason: Stage4InvalidReason::LocalRefinementRequired,
                    });
                };
                let cpr = ConeParabolaReloc {
                    apex,
                    cone_axis_dir,
                    half_angle,
                    plane_n,
                    plane_d,
                    vertex,
                    normal,
                    para_axis_dir: axis_dir,
                    cone_d_eps,
                };
                for v in [s, e] {
                    vert_parabola.insert(v, cpr);
                    endpoints.push(v);
                }
            }
            Curve::Hyperbola {
                center,
                normal,
                major_axis,
                semi_transverse: _, // recovered from the output edge in eval_source.
                semi_conjugate,
            } => {
                // PR-YR23: identify the TRUE cone + cutting plane from this edge's
                // incidence (the axis-parallel HYPE section), mirroring the
                // cone-parabola arm. Carry the cone's owning `InputId` so its
                // chord budget can be derived from its rim Circle.
                let key = if s < e { (s, e) } else { (e, s) };
                let entries = inc0.get(&key);
                let mut cone: Option<(InputId, Point3, Vector3, f64)> = None;
                let mut plane: Option<(Vector3, f64)> = None;
                if let Some(entries) = entries {
                    for &(input, surf) in entries {
                        match surf {
                            Surface::Cone {
                                apex,
                                axis_dir: cone_axis,
                                half_angle,
                            } => cone = Some((input, apex, cone_axis, half_angle)),
                            Surface::Plane { normal: pn, d: pd } => plane = Some((pn, pd)),
                            _ => {}
                        }
                    }
                }
                let (Some((cone_input, apex, cone_axis_dir, half_angle)), Some((plane_n, plane_d))) =
                    (cone, plane)
                else {
                    // A hyperbola section that is not a cone+plane pair is out of
                    // scope (producer fault). Loud STOP (P9/P10), mirroring the
                    // cone-parabola arm.
                    return Err(YangError::Stage4RegionInvalid {
                        vertex: s,
                        reason: Stage4InvalidReason::LocalRefinementRequired,
                    });
                };
                let owner = match cone_input {
                    InputId::A => a,
                    InputId::B => b,
                };
                let Some(cone_d_eps) =
                    cone_chord_budget_from_owner(apex, cone_axis_dir, half_angle, owner)
                else {
                    return Err(YangError::Stage4RegionInvalid {
                        vertex: s,
                        reason: Stage4InvalidReason::LocalRefinementRequired,
                    });
                };
                let chr = ConeHyperbolaReloc {
                    apex,
                    cone_axis_dir,
                    half_angle,
                    plane_n,
                    plane_d,
                    center,
                    normal,
                    major_axis,
                    semi_conjugate,
                    cone_d_eps,
                };
                for v in [s, e] {
                    vert_cone_hyperbola.insert(v, chr);
                    endpoints.push(v);
                }
            }
            Curve::Circle {
                center,
                normal,
                radius,
            } => {
                // PR-YR19: scan this edge's incidence for a `Surface::Sphere`
                // owner → `Some(R)`; else `None`. Uses the SAME canonical key as
                // the Ellipse arm below.
                let key = if s < e { (s, e) } else { (e, s) };
                let mut source_radius: Option<f64> = None;
                if let Some(entries) = inc0.get(&key) {
                    for &(_input, surf) in entries {
                        if let Surface::Sphere { radius: sr, .. } = surf {
                            source_radius = Some(sr);
                        }
                    }
                }
                for v in [s, e] {
                    insert_circle_or_junction(
                        v,
                        (center, normal, radius, source_radius),
                        &mut vert_circle,
                        &mut vert_circle_junction,
                        &mut endpoints,
                    );
                }
            }
            Curve::Ellipse {
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
            } => {
                // PR-YR11: identify the TRUE cylinder + cutting plane from this
                // edge's incidence (the two incident surfaces of DIFFERENT
                // inputs). A conic Ellipse edge is, by construction, one cylinder
                // lateral + one cutting plane.
                let key = if s < e { (s, e) } else { (e, s) };
                let entries = inc0.get(&key);
                let mut cyl: Option<(Point3, Vector3, f64)> = None;
                // PR-KV9: ALL cylinder entries with their owning inputs —
                // a cylinder×cylinder ellipse needs both for the per-point
                // gradient band + the combined chord budget.
                let mut cyls: Vec<(InputId, Point3, Vector3, f64)> = Vec::new();
                let mut plane: Option<(Vector3, f64)> = None;
                // PR-YR21: additionally scan for a `Surface::Cone` owner (the
                // cone+plane oblique section). Carry the owning `InputId` so the
                // cone's chord budget can be derived from its rim Circle.
                let mut cone: Option<(InputId, Point3, Vector3, f64)> = None;
                if let Some(entries) = entries {
                    for &(input, surf) in entries {
                        match surf {
                            Surface::Cylinder {
                                axis_point,
                                axis_dir,
                                radius,
                            } => {
                                cyl = Some((axis_point, axis_dir, radius));
                                cyls.push((input, axis_point, axis_dir, radius));
                            }
                            Surface::Plane { normal: pn, d: pd } => plane = Some((pn, pd)),
                            Surface::Cone {
                                apex,
                                axis_dir,
                                half_angle,
                            } => cone = Some((input, apex, axis_dir, half_angle)),
                            _ => {}
                        }
                    }
                }
                match (cyl, cone, plane) {
                    // YR11 cylinder + plane: the EXISTING path, byte-for-byte.
                    (Some((axis_point, axis_dir, radius)), _, Some((plane_n, plane_d))) => {
                        let er = EllipseReloc {
                            axis_point,
                            axis_dir,
                            radius,
                            plane_n,
                            plane_d,
                            center,
                            normal,
                            major_axis,
                            major_radius,
                            minor_radius,
                            second_cyl: None,
                        };
                        for v in [s, e] {
                            insert_ellipse_or_junction(
                                v,
                                er,
                                &mut vert_ellipse,
                                &mut vert_ell_junction,
                                &mut endpoints,
                            );
                        }
                    }
                    // PR-YR21 cone + plane (no cylinder): the new cone-ellipse
                    // path. Derive the cone's OWN chord budget from the cone
                    // owner's rim Circle (spec §3.3); a cone owner with no rim
                    // Circle is a producer fault → loud STOP (never TAU_WORK).
                    (
                        None,
                        Some((cone_input, apex, axis_dir, half_angle)),
                        Some((plane_n, plane_d)),
                    ) => {
                        let owner = match cone_input {
                            InputId::A => a,
                            InputId::B => b,
                        };
                        let Some(cone_d_eps) =
                            cone_chord_budget_from_owner(apex, axis_dir, half_angle, owner)
                        else {
                            return Err(YangError::Stage4RegionInvalid {
                                vertex: s,
                                reason: Stage4InvalidReason::LocalRefinementRequired,
                            });
                        };
                        let cer = ConeEllipseReloc {
                            apex,
                            axis_dir,
                            half_angle,
                            plane_n,
                            plane_d,
                            center,
                            normal,
                            major_axis,
                            major_radius,
                            minor_radius,
                            cone_d_eps,
                        };
                        for v in [s, e] {
                            vert_cone_ellipse.insert(v, cer);
                            endpoints.push(v);
                        }
                    }
                    // PR-KV9: cylinder × CYLINDER ellipse (the equal-radius
                    // intersecting-axes Steinmetz section, ssi cyl∩cyl). The
                    // ellipse lies in a KNOWN plane — its own stored frame —
                    // and it equals `cylinder ∩ that-plane` for EITHER owner
                    // (the curve is on both), so the existing cylinder+plane
                    // relocation closed form applies verbatim with the plane
                    // derived from the stored curve: n̂ from the ellipse
                    // normal, d = −n̂·center. `cyl` here holds the LAST
                    // cylinder scanned; with two cylinder entries either is
                    // exact, and the incidence order is deterministic.
                    (Some(_), None, None) if cyls.len() == 2 => {
                        // Deterministic owner order: sort by InputId (A first).
                        let mut cs = cyls.clone();
                        cs.sort_by_key(|&(i, ..)| matches!(i, InputId::B));
                        let (i1, axis_point, axis_dir, radius) = cs[0];
                        let (i2, ap2, ad2, _) = cs[1];
                        let budget = chord_tol_for_curved_owner(i1, a, b, 0, (s, e))?
                            + chord_tol_for_curved_owner(i2, a, b, 0, (s, e))?;
                        let nn = normalize3(normal.as_array());
                        let plane_n = Vector3::new(nn[0], nn[1], nn[2]);
                        let c = center.as_array();
                        let plane_d = -(nn[0] * c[0] + nn[1] * c[1] + nn[2] * c[2]);
                        let er = EllipseReloc {
                            axis_point,
                            axis_dir,
                            radius,
                            plane_n,
                            plane_d,
                            center,
                            normal,
                            major_axis,
                            major_radius,
                            minor_radius,
                            second_cyl: Some((ap2, ad2, budget)),
                        };
                        for v in [s, e] {
                            insert_ellipse_or_junction(
                                v,
                                er,
                                &mut vert_ellipse,
                                &mut vert_ell_junction,
                                &mut endpoints,
                            );
                        }
                    }
                    // Anything else (sphere, coplanar multi-solid): out of
                    // scope. Loud STOP (P9/P10).
                    _ => {
                        return Err(YangError::Stage4RegionInvalid {
                            vertex: s,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        });
                    }
                }
            }
            Curve::LineSegment => {
                // PR-F3: a LineSegment intersection edge between a PLANE and a
                // CYLINDER is a ruling LINE of the cylinder (ssi plane_cylinder
                // C3a/C3b). Recompute the exact line from the incidence and
                // re-select the unique candidate through both endpoints (the
                // SAME rule Stage 3's `build_intersection_curves` used).
                // Plane∩plane segments are exact → skip. Any OTHER curved
                // surface on a LineSegment edge is out of scope → loud STOP
                // (P9; cone generator lines arrive with their own closed form
                // when a fixture demands them).
                let key = if s < e { (s, e) } else { (e, s) };
                let Some(entries) = inc0.get(&key) else {
                    continue;
                };
                // KV6d Tier B: a TORUS-bearing LineSegment edge is a degree-4
                // intersection handled by the implicit-pair Newton relocation
                // block after this scan — defer it here (the conic LineSegment
                // arm has no closed form for it). Skip rather than STOP.
                if entries
                    .iter()
                    .any(|&(_, s)| matches!(s, Surface::Torus { .. }))
                {
                    continue;
                }
                let mut cyls: Vec<(InputId, Surface)> = Vec::new();
                let mut plane_surf: Option<Surface> = None;
                let mut pp: Vec<(Vector3, f64)> = Vec::new();
                let mut other_curved = false;
                for &(input, surf) in entries {
                    match surf {
                        Surface::Cylinder { .. } => cyls.push((input, surf)),
                        Surface::Plane { normal, d } => {
                            plane_surf = Some(surf);
                            pp.push((normal, d));
                        }
                        _ => other_curved = true,
                    }
                }
                // Two convertible pairs: cylinder × ⊥plane (F3) and PARALLEL
                // cylinder × cylinder (PR-KV9, ssi cyl∥cyl ruling lines).
                // Other curved-bearing line edges stay a loud STOP.
                let (surf_a, surf_b, tol) = match (cyls.as_slice(), plane_surf) {
                    ([(ci, cs)], Some(pl)) if !other_curved => {
                        (*cs, pl, chord_tol_for_curved_owner(*ci, a, b, 0, (s, e))?)
                    }
                    ([(i1, c1), (i2, c2)], None) if !other_curved => {
                        // Both meshes' facet chords contribute to the crossing
                        // vertex — the combined band is the SUM of the two
                        // owners' Stage-1 bounds (derived, not widening).
                        let t = chord_tol_for_curved_owner(*i1, a, b, 0, (s, e))?
                            + chord_tol_for_curved_owner(*i2, a, b, 0, (s, e))?;
                        (*c1, *c2, t)
                    }
                    ([], _) if !other_curved => {
                        // plane∩plane — the segment is exact, but record the
                        // line's planes per endpoint for the PR-KV11 triple-
                        // point pass below.
                        if pp.len() == 2 {
                            let entry = (pp[0].0, pp[0].1, pp[1].0, pp[1].1);
                            for v in [s, e] {
                                vert_pp_planes.entry(v).or_default().push(entry);
                            }
                        }
                        continue;
                    }
                    _ => {
                        return Err(YangError::Stage4RegionInvalid {
                            vertex: s,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        });
                    }
                };
                let to_ssi_err = |reason| YangError::SsiRefinementFailed {
                    edge: (s, e),
                    reason,
                };
                let q0 = surface_to_quadric(surf_a).map_err(to_ssi_err)?;
                let q1 = surface_to_quadric(surf_b).map_err(to_ssi_err)?;
                let returned =
                    ssi_rs::intersect(&q0, &q1).map_err(|err| YangError::SsiRefinementFailed {
                        edge: (s, e),
                        reason: SsiRefinementError::IntersectFailed(err),
                    })?;
                let p_s = mesh.verts[s as usize];
                let p_e = mesh.verts[e as usize];
                // PR-F3b: the SAME propagated band as Stage-3 matching (the
                // metric is shared, so every gate carries the factor).
                let band_amp = line_band_amplification(surf_a, surf_b).unwrap_or(1.0);
                let line_tol = band_amp * tol;
                let mut matched: Option<LineReloc> = None;
                let mut matched_n = 0usize;
                let mut matched_lines: Vec<(Point3, Vector3)> = Vec::new();
                for c in &returned {
                    if let ssi_rs::SsiCurve::Line { point, dir } = *c {
                        if line_perp_distance(p_s, point, dir) <= line_tol
                            && line_perp_distance(p_e, point, dir) <= line_tol
                        {
                            matched_n += 1;
                            matched_lines.push((point, dir));
                            matched = Some(LineReloc {
                                point,
                                dir,
                                band_budget: line_tol,
                            });
                        }
                    }
                }
                // R0072: near-tangent plane∩cylinder yields two near-coincident
                // parallel generators that both pass the band; the edge lies on
                // exactly one. Break the tie by position (the disjoint-lowest
                // endpoint-distance interval) — the SAME rule Stage 3 uses. If no
                // unambiguous winner (overlapping intervals / non-parallel), the
                // loud `AmbiguousCurve` below stands.
                if matched_n > 1 {
                    if let Some(wk) = select_disjoint_parallel_line(&matched_lines, p_s, p_e) {
                        let (point, dir) = matched_lines[wk];
                        matched_n = 1;
                        matched = Some(LineReloc {
                            point,
                            dir,
                            band_budget: line_tol,
                        });
                    }
                }
                let Some(lr) = (if matched_n == 1 { matched } else { None }) else {
                    return Err(YangError::SsiRefinementFailed {
                        edge: (s, e),
                        reason: SsiRefinementError::AmbiguousCurve {
                            candidates: returned.len(),
                            matched: matched_n,
                        },
                    });
                };
                for v in [s, e] {
                    // A vertex on TWO DIFFERENT lines (e.g. a box corner ruling
                    // piercing the cylinder) would need a line∩line junction —
                    // out of scope, loud STOP rather than silently overwriting
                    // (the same defect class F3 fixes for line+circle).
                    if let Some(prev) = vert_line.get(&v) {
                        let same = line_perp_distance(prev.point, lr.point, lr.dir)
                            <= cad_primitives::TAU_MODEL
                            && {
                                let d1 = normalize3(prev.dir.as_array());
                                let d2 = normalize3(lr.dir.as_array());
                                let cx = [
                                    d1[1] * d2[2] - d1[2] * d2[1],
                                    d1[2] * d2[0] - d1[0] * d2[2],
                                    d1[0] * d2[1] - d1[1] * d2[0],
                                ];
                                (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt()
                                    <= cad_primitives::TAU_MODEL
                            };
                        if !same {
                            return Err(YangError::Stage4RegionInvalid {
                                vertex: v,
                                reason: Stage4InvalidReason::LocalRefinementRequired,
                            });
                        }
                    }
                    vert_line.insert(v, lr);
                    endpoints.push(v);
                }
            }
        }
    }

    // PR-KV11: resolve ellipse × (plane∩plane line) TRIPLE points. An ellipse
    // endpoint that also terminates an exact pp-segment (the cap∩face trace
    // crossing the lateral) must land on `(plane ∩ plane) ∩ cylinder`, not on
    // the ellipse alone — reuse the ellipse-junction closed form with a
    // synthetic second member carrying the line's OTHER plane (the one that
    // is not the ellipse's own cutting plane; bit identity — both come from
    // the same incidence `Surface::Plane` values).
    {
        let shared: Vec<u32> = vert_ellipse
            .keys()
            .filter(|v| vert_pp_planes.contains_key(v))
            .copied()
            .collect();
        for v in shared {
            let e_a = vert_ellipse[&v];
            let mut others: Vec<(Vector3, f64)> = Vec::new();
            for &(n1, d1, n2, d2) in &vert_pp_planes[&v] {
                let m1 = n1.as_array() == e_a.plane_n.as_array() && d1 == e_a.plane_d;
                let m2 = n2.as_array() == e_a.plane_n.as_array() && d2 == e_a.plane_d;
                let other = if m1 {
                    Some((n2, d2))
                } else if m2 {
                    Some((n1, d1))
                } else {
                    None
                };
                if let Some(o) = other {
                    if !others
                        .iter()
                        .any(|&(n, d)| n.as_array() == o.0.as_array() && d == o.1)
                    {
                        others.push(o);
                    }
                }
            }
            match others.len() {
                // A pp-line through an ellipse endpoint whose pair does not
                // include the ellipse's own plane, or more than one distinct
                // crossing line: relocating onto any single curve leaves the
                // vertex off the others — loud STOP, never a silent pick
                // (P9/P10).
                0 | 2.. => {
                    return Err(YangError::Stage4RegionInvalid {
                        vertex: v,
                        reason: Stage4InvalidReason::LocalRefinementRequired,
                    });
                }
                1 => {
                    let (on, od) = others[0];
                    let e_b = EllipseReloc {
                        plane_n: on,
                        plane_d: od,
                        ..e_a
                    };
                    vert_ellipse.remove(&v);
                    vert_ell_junction.insert(v, (e_a, e_b));
                }
            }
        }
    }

    // PR-F3: a vertex shared by a LINE edge and a CIRCLE edge is a TRIPLE
    // point — it must end up on BOTH curves. Relocating onto either alone
    // leaves it off the other (the KV6b-F3 probe defect: radius exactly r,
    // axial coordinate off by the sagitta → output-face plane vs Newell
    // disagreement). The exact junction is `line ∩ plane-of-circle`: the line
    // lies ON the cylinder and the circle IS `cylinder ∩ circle-plane`, so the
    // line's piercing of the circle plane lies exactly on the circle. Pull
    // such vertices OUT of both single-curve maps into a junction map.
    let mut vert_junction: BTreeMap<u32, (LineReloc, CircleAssign)> = BTreeMap::new();
    {
        let shared: Vec<u32> = vert_line
            .keys()
            .filter(|v| vert_circle.contains_key(v))
            .copied()
            .collect();
        for v in shared {
            let lr = vert_line.remove(&v).expect("key from vert_line");
            let circ = vert_circle.remove(&v).expect("checked contains_key");
            vert_junction.insert(v, (lr, circ));
        }
    }

    // M8 disc∩disc no-skip audit (P10): a circle∩circle lens corner that is ALSO
    // on any OTHER curve type (a line, ellipse, cone conic, or line+circle
    // junction) is an over-determined junction this arm does not resolve — loud
    // STOP rather than relocate it onto only the two circles. (Cannot arise for a
    // pure disc∩disc lens, but never silently pick.)
    for v in vert_circle_junction.keys() {
        if vert_line.contains_key(v)
            || vert_ellipse.contains_key(v)
            || vert_cone_ellipse.contains_key(v)
            || vert_parabola.contains_key(v)
            || vert_cone_hyperbola.contains_key(v)
            || vert_junction.contains_key(v)
        {
            return Err(YangError::Stage4RegionInvalid {
                vertex: *v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
    }

    // A vertex shared by BOTH a circle and an ellipse edge (two distinct curves
    // through one vertex) is a genuine ambiguity — relocating it twice would be
    // wrong, so loud STOP rather than silently picking one (spec §4 no-skip
    // audit / P10).
    // PR-F3: the line+circle junction is HANDLED (vert_junction above); a line
    // meeting any OTHER conic is still a loud STOP, folded into each audit.
    for v in vert_ellipse.keys() {
        if vert_circle.contains_key(v) || vert_line.contains_key(v) || vert_junction.contains_key(v)
        {
            return Err(YangError::Stage4RegionInvalid {
                vertex: *v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
    }
    // PR-YR21: a vertex shared by a cone-ellipse edge AND any other conic edge
    // (cylinder-ellipse or circle) is a genuine ambiguity — loud STOP (spec
    // §3.2 / P10), the same no-skip audit extended to the cone map.
    for v in vert_cone_ellipse.keys() {
        if vert_circle.contains_key(v)
            || vert_ellipse.contains_key(v)
            || vert_line.contains_key(v)
            || vert_junction.contains_key(v)
        {
            return Err(YangError::Stage4RegionInvalid {
                vertex: *v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
    }
    // PR-YR22: a vertex shared by a cone-parabola edge AND any other conic edge
    // (circle / cylinder-ellipse / cone-ellipse) is a genuine ambiguity — loud
    // STOP (P10), the same no-skip audit extended to the parabola map.
    for v in vert_parabola.keys() {
        if vert_circle.contains_key(v)
            || vert_ellipse.contains_key(v)
            || vert_cone_ellipse.contains_key(v)
            || vert_cone_hyperbola.contains_key(v)
            || vert_line.contains_key(v)
            || vert_junction.contains_key(v)
        {
            return Err(YangError::Stage4RegionInvalid {
                vertex: *v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
    }
    // PR-YR23: a vertex shared by a cone-hyperbola edge AND any other conic edge
    // (circle / cylinder-ellipse / cone-ellipse / cone-parabola) is a genuine
    // ambiguity — loud STOP (P10), the same no-skip audit extended to the
    // hyperbola map.
    for v in vert_cone_hyperbola.keys() {
        if vert_circle.contains_key(v)
            || vert_ellipse.contains_key(v)
            || vert_cone_ellipse.contains_key(v)
            || vert_parabola.contains_key(v)
            || vert_line.contains_key(v)
            || vert_junction.contains_key(v)
        {
            return Err(YangError::Stage4RegionInvalid {
                vertex: *v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
    }

    // (2) Relocate / retag every endpoint. `processed` is the no-skip audit set;
    // `moved` is the subset whose position actually changed (ρ > TAU_WORK) — the
    // triangles touching THOSE verts are the ones Stage-4 validation gates
    // (spec §4.5 step 4: validate per RELOCATED triangle, not pre-existing
    // arrangement slivers that `boolean()` legitimately kept for watertightness).
    if let Ok(list) = std::env::var("YANG_V_PROBE") {
        for tok in list.split(',') {
            let Ok(v) = tok.trim().parse::<u32>() else {
                continue;
            };
            if let Some(er) = vert_ellipse.get(&v) {
                eprintln!(
                    "YANG_V_PROBE v={v} er plane_n={:?} plane_d={:.17e} center={:?} \
                     normal={:?} major_axis={:?} a={:.17e} b={:.17e} second_cyl={:?}",
                    er.plane_n,
                    er.plane_d,
                    er.center,
                    er.normal,
                    er.major_axis,
                    er.major_radius,
                    er.minor_radius,
                    er.second_cyl,
                );
            }
            eprintln!(
                "YANG_V_PROBE v={v} p={:?} circle={} ellipse={} cone_ell={} parab={} hyp={} \
                 line={} ell_junction={} circle_junction={} line_circle_junction={} \
                 pp_planes={} endpoint={}",
                mesh.verts.get(v as usize),
                vert_circle.contains_key(&v),
                vert_ellipse.contains_key(&v),
                vert_cone_ellipse.contains_key(&v),
                vert_parabola.contains_key(&v),
                vert_cone_hyperbola.contains_key(&v),
                vert_line.contains_key(&v),
                vert_ell_junction.contains_key(&v),
                vert_circle_junction.contains_key(&v),
                vert_junction.contains_key(&v),
                vert_pp_planes.contains_key(&v),
                endpoints.contains(&v),
            );
        }
    }
    let mut processed: HashSet<u32> = HashSet::new();
    let mut moved: HashSet<u32> = HashSet::new();
    let mut relocations: Vec<(u32, f64)> = Vec::new();
    // Deterministic order: BTreeMap iteration.
    for (&v, &(center, normal, radius, src_r)) in &vert_circle {
        let p = mesh.verts[v as usize];
        // PR-YR19 (spec §4 Site 2): split the residual so the in-plane RADIAL
        // band is the propagated `(R/r_c)·d_ε` for a sphere section circle while
        // the AXIAL band stays `d_ε`. For `None`/non-sphere this is identical to
        // `max(axial, radial_dev) > d_eps`, i.e. byte-identical to the prior
        // `circle_residual > d_eps`. Near-tangent (`radius ≤ MIN_FEATURE_SIZE`)
        // fails closed (keeps the unscaled band).
        let (axial, radial_dev) = circle_residual_split(p, center, normal, radius);
        let radial_band = match src_r {
            Some(big_r) if radius > cad_primitives::MIN_FEATURE_SIZE => (big_r / radius) * d_eps,
            _ => d_eps,
        };
        if axial > d_eps || radial_dev > radial_band {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        // Preserve the original combined-max `rho` for the `> TAU_WORK`
        // move-gate so its semantics are unchanged.
        let rho = axial.max(radial_dev);
        // Always project to obtain the circle-frame angle `t` (and the exact
        // on-curve position). For ρ ≤ TAU_WORK the projection is a no-op move
        // but still yields the retag `t`; for the relocate band it moves the
        // vertex onto the curve.
        let (proj, t) = project_onto_circle(p, center, normal, radius)
            .map_err(|reason| YangError::Stage4RegionInvalid { vertex: v, reason })?;
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // M8 disc∩disc CROSSING: relocate each lens-corner vertex onto the EXACT
    // circle∩circle intersection (on BOTH coplanar circles). The vertex sits on
    // a Stage-1 chord, off each circle radially by ≤ d_eps; the displacement to
    // the exact corner is amplified by `1/sin θ`, θ = angle between the two
    // circles' radial directions at the corner (the same derived gradient metric
    // as the cyl×cyl ellipse junction — NOT tolerance widening). A grazing/
    // tangent crossing (θ → 0) has no well-defined corner and `coplanar_circle_
    // circle_intersection` returns `None` → loud STOP.
    for (&v, &(ca, cb)) in &vert_circle_junction {
        let p = mesh.verts[v as usize];
        let (c_a, n_a, r_a, _) = ca;
        let (c_b, n_b, r_b, _) = cb;
        let Some(j) = coplanar_circle_circle_intersection(c_a, n_a, r_a, c_b, n_b, r_b, p) else {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        };
        let pa = p.as_array();
        let ja = j.as_array();
        let rho =
            ((ja[0] - pa[0]).powi(2) + (ja[1] - pa[1]).powi(2) + (ja[2] - pa[2]).powi(2)).sqrt();
        // sin θ = |r̂_a × r̂_b| at the corner (both radial vectors are in-plane).
        let ra_v = [ja[0] - c_a.x(), ja[1] - c_a.y(), ja[2] - c_a.z()];
        let rb_v = [ja[0] - c_b.x(), ja[1] - c_b.y(), ja[2] - c_b.z()];
        let ra_h = normalize3(ra_v);
        let rb_h = normalize3(rb_v);
        let cr = [
            ra_h[1] * rb_h[2] - ra_h[2] * rb_h[1],
            ra_h[2] * rb_h[0] - ra_h[0] * rb_h[2],
            ra_h[0] * rb_h[1] - ra_h[1] * rb_h[0],
        ];
        let sin_theta = (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
        let gate = if sin_theta > 0.0 {
            2.0 * d_eps / sin_theta
        } else {
            f64::INFINITY
        };
        if rho > gate {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        // `j` is on circle_a by construction; project to get its frame angle `t`
        // for the source retag (positionally exact on both circles either way).
        let (proj, t) = project_onto_circle(j, c_a, n_a, r_a)
            .map_err(|reason| YangError::Stage4RegionInvalid { vertex: v, reason })?;
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-YR11: ellipse relocation loop, mirroring the circle loop above. Closed
    // form via the cylinder parameterization (spec §2). Same `d_eps` chord band.
    for (&v, er) in &vert_ellipse {
        let p = mesh.verts[v as usize];
        let rho = ellipse_residual(p, er);
        // PR-KV9: cylinder×cylinder sections gate against the per-point
        // gradient band (combined budget × 1/sin α); at tangency grade the
        // metric is unbounded and the Stage-3 surface-membership gate is
        // the backstop. The cylinder×plane path keeps the global d_ε
        // byte-for-byte.
        let gate = match er.second_cyl {
            Some((ap2, ad2, budget)) => {
                cyl_cyl_point_amplification(p, (er.axis_point, er.axis_dir), (ap2, ad2))
                    .map_or(f64::INFINITY, |amp| amp * budget)
            }
            None => d_eps,
        };
        if rho > gate {
            if std::env::var("KV11_PROBE").is_ok() {
                eprintln!(
                    "KV11_PROBE ellipse band reject: v={v} rho={rho:.3e} gate={gate:.3e} p={p:?}"
                );
            }
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        let (proj, t) = project_onto_ellipse_via_cylinder(p, er)
            .map_err(|reason| YangError::Stage4RegionInvalid { vertex: v, reason })?;
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-KV9: ellipse×ellipse JUNCTION relocation. The exact junction lies
    // on `(plane₁ ∩ plane₂) ∩ cylinder` (the crossing point of the two
    // Steinmetz sections — on the cylinder and in BOTH cutting planes,
    // hence on both ellipses). The plane–plane line is exact; intersecting
    // it with the relocation cylinder is a quadratic with ≤ 2 roots; the
    // root nearest the current vertex is the junction (the two crossing
    // points are 2r apart — far outside any chord band, so nearest-pick is
    // deterministic and unambiguous). Gate at 2·d_ε (each constituent
    // membership is within its own propagated band; the junction inherits
    // both, mirroring the line+circle junction's derivation).
    for (&v, &(e_a, e_b)) in &vert_ell_junction {
        let p = mesh.verts[v as usize];
        let n1 = normalize3(e_a.plane_n.as_array());
        let n2 = normalize3(e_b.plane_n.as_array());
        let dir = [
            n1[1] * n2[2] - n1[2] * n2[1],
            n1[2] * n2[0] - n1[0] * n2[2],
            n1[0] * n2[1] - n1[1] * n2[0],
        ];
        let dl = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        if dl < cad_primitives::MIN_FEATURE_SIZE {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
        let d = [dir[0] / dl, dir[1] / dl, dir[2] / dl];
        // A point on both planes: solve n1·x = −d1, n2·x = −d2 in the span
        // of {n1, n2} (x = α·n1 + β·n2; Gram system with g = n1·n2).
        let g = n1[0] * n2[0] + n1[1] * n2[1] + n1[2] * n2[2];
        let det = 1.0 - g * g;
        if det.abs() < cad_primitives::MIN_FEATURE_SIZE {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
        let (r1, r2) = (-e_a.plane_d, -e_b.plane_d);
        let alpha = (r1 - g * r2) / det;
        let beta = (r2 - g * r1) / det;
        let p0 = [
            alpha * n1[0] + beta * n2[0],
            alpha * n1[1] + beta * n2[1],
            alpha * n1[2] + beta * n2[2],
        ];
        // Intersect the line p0 + t·d with the relocation cylinder of e_a.
        let ax = normalize3(e_a.axis_dir.as_array());
        let ap = e_a.axis_point.as_array();
        let rel = [p0[0] - ap[0], p0[1] - ap[1], p0[2] - ap[2]];
        let perp = |w: [f64; 3]| -> [f64; 3] {
            let h = w[0] * ax[0] + w[1] * ax[1] + w[2] * ax[2];
            [w[0] - h * ax[0], w[1] - h * ax[1], w[2] - h * ax[2]]
        };
        let rp = perp(rel);
        let dp = perp(d);
        let aa = dp[0] * dp[0] + dp[1] * dp[1] + dp[2] * dp[2];
        let bb = 2.0 * (rp[0] * dp[0] + rp[1] * dp[1] + rp[2] * dp[2]);
        let cc = rp[0] * rp[0] + rp[1] * rp[1] + rp[2] * rp[2] - e_a.radius * e_a.radius;
        let disc = bb * bb - 4.0 * aa * cc;
        if !(aa > cad_primitives::MIN_FEATURE_SIZE && disc >= 0.0) {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
        let sq = disc.sqrt();
        let pa = p.as_array();
        let mut best: Option<([f64; 3], f64)> = None;
        for t in [(-bb - sq) / (2.0 * aa), (-bb + sq) / (2.0 * aa)] {
            let x = [p0[0] + t * d[0], p0[1] + t * d[1], p0[2] + t * d[2]];
            let dd =
                ((x[0] - pa[0]).powi(2) + (x[1] - pa[1]).powi(2) + (x[2] - pa[2]).powi(2)).sqrt();
            if best.map(|(_, b)| dd < b).unwrap_or(true) {
                best = Some((x, dd));
            }
        }
        let (j, rho) = best.expect("two real roots checked");
        // PR-KV11: the vertex moves ALONG the junction line to reach the
        // cylinder, so its radial chord residual (≤ the combined band) is
        // amplified by `1/|d̂·r̂|` — the directional derivative of the
        // radial distance along the line at the junction (the same derived
        // metric propagation as the KV9 cyl×cyl `1/sin α` gradient band; a
        // grazing line ⇒ unbounded metric, backstopped by the Stage-3
        // surface-membership gates, mirroring the cyl×cyl arm).
        let rel_j = [j[0] - ap[0], j[1] - ap[1], j[2] - ap[2]];
        let rp_j = perp(rel_j);
        let rp_j_len = (rp_j[0] * rp_j[0] + rp_j[1] * rp_j[1] + rp_j[2] * rp_j[2]).sqrt();
        let grad = if rp_j_len > 0.0 {
            ((d[0] * rp_j[0] + d[1] * rp_j[1] + d[2] * rp_j[2]) / rp_j_len).abs()
        } else {
            0.0
        };
        // KV9-F1 E-L2 (spec §2c, branch row J1): a junction of two sections of
        // the SAME unordered cylinder pair is ALWAYS the pair's surface-tangency
        // point (the two decomposition planes intersect in the line through both
        // tangency points; that line meets the cylinder exactly where the two
        // radial gradients align). There the vertex is the PINCH of the two
        // faceted-surface intersection polylines, whose standoff from the exact
        // crossing is SECOND-order-controlled: in tangent-plane coordinates the
        // cylinders are the graphs y = r − x²/2r and y = r − z²/2r; facet
        // displacements a ∈ [0, ε_A], b ∈ [0, ε_B] perturb the intersection to
        // the hyperbola x² − z² = 2r(b−a), standoff √(2r·|b−a|) ≤ √(2r·B) with
        // B the combined chord budget carried by `second_cyl`, plus ≤ B
        // normal-direction offset. A derived metric conversion (the
        // single-ellipse arm's 1/sin α analog at tangency grade), NOT tolerance
        // widening — the relocation target stays the EXACT junction. Every
        // other junction (row J2 — the KV11 box-edge class) keeps the
        // first-order 2·d_ε/|d̂·r̂| line metric byte-identical.
        let same_pair_budget = match (e_a.second_cyl, e_b.second_cyl) {
            (Some((sa_p, sa_d, ba)), Some((sb_p, sb_d, bb))) => {
                let same = e_a.axis_point.as_array() == e_b.axis_point.as_array()
                    && e_a.axis_dir.as_array() == e_b.axis_dir.as_array()
                    && sa_p.as_array() == sb_p.as_array()
                    && sa_d.as_array() == sb_d.as_array();
                if same {
                    Some(ba.max(bb))
                } else {
                    None
                }
            }
            _ => None,
        };
        let gate = if let Some(budget) = same_pair_budget {
            (2.0 * e_a.radius * budget).sqrt() + budget
        } else if grad > 0.0 {
            2.0 * d_eps / grad
        } else {
            f64::INFINITY
        };
        // KV9-F1 Increment 0c census: per-junction second_cyl provenance +
        // first-order gate state (kept env-gated, like the other Stage-4 probes).
        if std::env::var("KV9_JUNCTION_PROBE").is_ok() {
            eprintln!(
                "KV9_JUNCTION_PROBE v={v} p={p:?} j={j:?} rho={rho:.4e} grad={grad:.4e} \
                 gate={gate:.4e} d_eps={d_eps:.4e} \
                 a_axis=({:?},{:?}) a_second={:?} b_axis=({:?},{:?}) b_second={:?}",
                e_a.axis_point.as_array(),
                e_a.axis_dir.as_array(),
                e_a.second_cyl
                    .map(|(sp, sd, bud)| (sp.as_array(), sd.as_array(), bud)),
                e_b.axis_point.as_array(),
                e_b.axis_dir.as_array(),
                e_b.second_cyl
                    .map(|(sp, sd, bud)| (sp.as_array(), sd.as_array(), bud)),
            );
        }
        if rho > gate {
            if std::env::var("KV11_PROBE").is_ok() {
                eprintln!(
                    "KV11_PROBE junction band reject: v={v} rho={rho:.3e} gate={gate:.3e} p={p:?} j={j:?}"
                );
            }
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        let proj = Point3::new(j[0], j[1], j[2]);
        // Param on e_a's ellipse for the source retag (output edges of BOTH
        // ellipses touch this vertex; the position is exact on both, so the
        // retag curve choice is positional-exact either way).
        let t = ellipse_param(
            proj,
            e_a.center,
            e_a.normal,
            e_a.major_axis,
            e_a.major_radius,
            e_a.minor_radius,
        );
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-YR21: cone-ellipse relocation loop, mirroring the cylinder-ellipse loop.
    // Closed form via the cone GENERATOR parameterization (spec §3.1). Gated
    // against the cone's OWN chord budget `cone_d_eps` (NOT the rim-AABB `d_eps`)
    // so a tall-thin cone's residual is checked against the honest cone bound.
    for (&v, cer) in &vert_cone_ellipse {
        let p = mesh.verts[v as usize];
        let rho = cone_ellipse_residual(p, cer);
        if rho > cer.cone_d_eps {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        let proj = project_onto_cone_section(
            p,
            cer.apex,
            cer.axis_dir,
            cer.half_angle,
            cer.plane_n,
            cer.plane_d,
        )
        .map_err(|reason| YangError::Stage4RegionInvalid { vertex: v, reason })?;
        // Round-trip param `t` in the stored ellipse frame so the unchanged
        // `eval_source` Ellipse arm reproduces the relocated position.
        let t = ellipse_param(
            proj,
            cer.center,
            cer.normal,
            cer.major_axis,
            cer.major_radius,
            cer.minor_radius,
        );
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-YR22: cone-parabola relocation loop, mirroring the cone-ellipse loop.
    // Closed form via the cone GENERATOR parameterization (the section TYPE does
    // not change the relocation — `project_onto_cone_section` is type-agnostic;
    // its `s ≤ 0` / generator-parallel guards correctly reject the out-of-scope
    // parabola tail, which the fixture's finite arc avoids). Gated against the
    // cone's OWN chord budget `cone_d_eps`.
    for (&v, cpr) in &vert_parabola {
        let p = mesh.verts[v as usize];
        let rho = cone_plane_residual(
            p,
            cpr.apex,
            cpr.cone_axis_dir,
            cpr.half_angle,
            cpr.plane_n,
            cpr.plane_d,
        );
        if rho > cpr.cone_d_eps {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        let proj = project_onto_cone_section(
            p,
            cpr.apex,
            cpr.cone_axis_dir,
            cpr.half_angle,
            cpr.plane_n,
            cpr.plane_d,
        )
        .map_err(|reason| YangError::Stage4RegionInvalid { vertex: v, reason })?;
        // Round-trip param `t` = the conjugate-axis coordinate of the parabola
        // parameterization `(proj − vertex)·(normal × axis_dir)`, so the unchanged
        // `eval_source` Parabola arm reproduces the relocated position (oracle3).
        let n = normalize3(cpr.normal.as_array());
        let ax = normalize3(cpr.para_axis_dir.as_array());
        let conj = [
            n[1] * ax[2] - n[2] * ax[1],
            n[2] * ax[0] - n[0] * ax[2],
            n[0] * ax[1] - n[1] * ax[0],
        ];
        let vtx = cpr.vertex.as_array();
        let pr = proj.as_array();
        let t =
            (pr[0] - vtx[0]) * conj[0] + (pr[1] - vtx[1]) * conj[1] + (pr[2] - vtx[2]) * conj[2];
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-YR23: cone-hyperbola relocation loop, mirroring the cone-parabola loop.
    // Closed form via the same type-agnostic cone GENERATOR parameterization
    // (`project_onto_cone_section`); its `s ≤ 0` / generator-parallel guards
    // correctly reject the out-of-scope asymptote, which the fixture's finite arc
    // avoids. Gated against the cone's OWN chord budget `cone_d_eps`.
    for (&v, chr) in &vert_cone_hyperbola {
        let p = mesh.verts[v as usize];
        let rho = cone_plane_residual(
            p,
            chr.apex,
            chr.cone_axis_dir,
            chr.half_angle,
            chr.plane_n,
            chr.plane_d,
        );
        if rho > chr.cone_d_eps {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        let proj = project_onto_cone_section(
            p,
            chr.apex,
            chr.cone_axis_dir,
            chr.half_angle,
            chr.plane_n,
            chr.plane_d,
        )
        .map_err(|reason| YangError::Stage4RegionInvalid { vertex: v, reason })?;
        // Round-trip param `t = asinh(v_coord / b)` where `v_coord` is the
        // conjugate-axis coordinate `(proj − center)·(normal × major_axis)` and
        // `b = semi_conjugate`. The eval is
        // `center + a·cosh(t)·major + b·sinh(t)·(normal×major)`, so
        // `v_coord = b·sinh(t) ⇒ t = asinh(v_coord/b)` (sinh is the bijective
        // coordinate; well-defined ∀ v_coord). The unchanged `eval_source`
        // Hyperbola arm reproduces the relocated position (oracle3).
        let n = normalize3(chr.normal.as_array());
        let maj = normalize3(chr.major_axis.as_array());
        let conj = [
            n[1] * maj[2] - n[2] * maj[1],
            n[2] * maj[0] - n[0] * maj[2],
            n[0] * maj[1] - n[1] * maj[0],
        ];
        let ctr = chr.center.as_array();
        let pr = proj.as_array();
        let v_coord =
            (pr[0] - ctr[0]) * conj[0] + (pr[1] - ctr[1]) * conj[1] + (pr[2] - ctr[2]) * conj[2];
        let t = (v_coord / chr.semi_conjugate).asinh();
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // PR-F3: ruling-line relocation loop. The residual is the perpendicular
    // distance to the exact line (the sagitta of the Stage-1 facet chord the
    // arrangement point sits on), gated at the same global `d_eps` band as the
    // circle loop. The relocated position is the foot of the perpendicular —
    // exactly on the line, hence exactly on BOTH the cutting plane and the
    // cylinder. `t` is the along-line parameter; no conic OUTPUT edge claims a
    // line vertex in `emit_topology`, so its source stays `BRepVertex` and
    // `eval_source` returns the relocated mesh position directly.
    for (&v, lr) in &vert_line {
        let p = mesh.verts[v as usize];
        let rho = line_perp_distance(p, lr.point, lr.dir);
        // PR-F3b/PR-KV9: the residual is the line-distance metric, so the
        // gate is the ABSOLUTE propagated budget computed at collection (the
        // owner chord band(s) converted into this metric) — not the raw
        // radial band, and not the global d_ε (whose owner mix is wrong for
        // cylinder×cylinder lines).
        if rho > lr.band_budget {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        let d = normalize3(lr.dir.as_array());
        let pt = lr.point.as_array();
        let x = p.as_array();
        let w = [x[0] - pt[0], x[1] - pt[1], x[2] - pt[2]];
        let along = w[0] * d[0] + w[1] * d[1] + w[2] * d[2];
        let proj = Point3::new(
            pt[0] + along * d[0],
            pt[1] + along * d[1],
            pt[2] + along * d[2],
        );
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, along));
        processed.insert(v);
    }

    // PR-F3: line+circle JUNCTION relocation loop. The exact junction is
    // `line ∩ plane-of-circle` (which lies exactly on the circle, since the
    // line is on the cylinder and the circle is cylinder ∩ circle-plane). The
    // residual gate is `2·d_eps`: the vertex is off the line radially by ≤ one
    // sagitta AND off the circle plane along the line by ≤ another
    // sagitta-order term (it sits on the crossing of the cutting plane with a
    // rim-chord facet edge), so the combined displacement to the junction is
    // bounded by 2·d_eps — a derived bound, not tolerance widening. The final
    // position is `project_onto_circle(j)` so the vertex's `BRepEdge { edge, t }`
    // source round-trips bitwise through the unchanged `eval_source` Circle arm.
    for (&v, &(lr, (center, normal, radius, _src_r))) in &vert_junction {
        let p = mesh.verts[v as usize];
        let n = normalize3(normal.as_array());
        let d = normalize3(lr.dir.as_array());
        let denom = n[0] * d[0] + n[1] * d[1] + n[2] * d[2];
        if denom.abs() < cad_primitives::TAU_MODEL {
            // Line parallel to the circle plane: no transversal junction.
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }
        let pt = lr.point.as_array();
        let c = center.as_array();
        let s_par = (n[0] * (c[0] - pt[0]) + n[1] * (c[1] - pt[1]) + n[2] * (c[2] - pt[2])) / denom;
        let j = Point3::new(
            pt[0] + s_par * d[0],
            pt[1] + s_par * d[1],
            pt[2] + s_par * d[2],
        );
        let pj = [
            p.as_array()[0] - j.as_array()[0],
            p.as_array()[1] - j.as_array()[1],
            p.as_array()[2] - j.as_array()[2],
        ];
        let rho = (pj[0] * pj[0] + pj[1] * pj[1] + pj[2] * pj[2]).sqrt();
        // PR-F3b: line-band component carries the propagated budget; the
        // along-line crossing component stays at the raw d_ε.
        if rho > lr.band_budget + d_eps {
            return Err(YangError::Stage4RegionInvalid {
                vertex: v,
                reason: Stage4InvalidReason::OffCurveBeyondChordBand,
            });
        }
        let (proj, t) = project_onto_circle(j, center, normal, radius)
            .map_err(|reason| YangError::Stage4RegionInvalid { vertex: v, reason })?;
        if rho > cad_primitives::TAU_WORK {
            mesh.verts[v as usize] = proj;
            moved.insert(v);
        }
        relocations.push((v, t));
        processed.insert(v);
    }

    // No-skip audit (anti-disproven-attempt): every conic endpoint was handled.
    let relocation_keys: HashSet<u32> = relocations.iter().map(|&(v, _)| v).collect();
    let endpoint_set: HashSet<u32> = endpoints.iter().copied().collect();
    if processed != endpoint_set || processed != relocation_keys {
        return Err(YangError::Stage4RegionInvalid {
            vertex: u32::MAX,
            reason: Stage4InvalidReason::LocalRefinementRequired,
        });
    }

    // (2t) KV6d Tier B — degree-4 (TORUS) relocation via Newton on the implicit
    // surface pair. A torus's intersections are not conics, so these edges never
    // reach the `curves0` conic scan above; they arrive as untyped chord
    // segments and would otherwise stay off the analytic torus (the proven KV6d
    // blocker). For each intersection edge bearing exactly one torus and one
    // transversal partner, relocate both endpoints onto {F_torus=0, F_other=0}.
    // Kept SEPARATE from the conic bookkeeping (processed / endpoints /
    // relocations) — the output torus-intersection edges stay LineSegment
    // polylines (no analytic curve, no `t` retag), which validation and
    // `tessellate_torus_patch` already accept — so the conic no-skip audit above
    // is unaffected. Moved vertices join `moved` for the relocated-triangle
    // validation. v1 scope: one torus + one partner per edge; torus∩torus,
    // multi-surface junctions, and torus×conic junctions are loud STOPs (P9).
    {
        // Aggregate, per torus-edge endpoint, the single incident torus and the
        // DISTINCT partner surfaces across all its torus edges. One partner is a
        // plain torus∩surface edge (2-equation Newton); two partners is a
        // 3-surface JUNCTION — a box edge (two planes) piercing the torus, or a
        // torus∩plane meeting a torus∩plane′ — relocated onto all three. More
        // than two partners, or a torus∩torus edge, is out of v1 scope (STOP).
        let mut vert_torus: BTreeMap<u32, Surface> = BTreeMap::new();
        let mut vert_partners: BTreeMap<u32, Vec<Surface>> = BTreeMap::new();
        for (&(s, e), entries) in &inc0 {
            let mut tori: Vec<Surface> = Vec::new();
            let mut others: Vec<Surface> = Vec::new();
            for &(_input, surf) in entries {
                if matches!(surf, Surface::Torus { .. }) {
                    tori.push(surf);
                } else {
                    others.push(surf);
                }
            }
            if tori.is_empty() {
                continue; // not a torus edge — conic scan / exact handles it
            }
            if tori.len() != 1 {
                // torus∩torus (degree-4 with no single base surface) — out of
                // v1 scope. Loud STOP.
                return Err(YangError::Stage4RegionInvalid {
                    vertex: s,
                    reason: Stage4InvalidReason::LocalRefinementRequired,
                });
            }
            for v in [s, e] {
                vert_torus.insert(v, tori[0]);
                let entry = vert_partners.entry(v).or_default();
                for o in &others {
                    if !entry.contains(o) {
                        entry.push(*o);
                    }
                }
            }
        }
        for (&v, &t_surf) in &vert_torus {
            // A torus-edge endpoint that is also a CONIC endpoint mixes the
            // implicit-pair and closed-form relocations — out of v1 scope, STOP.
            if endpoint_set.contains(&v) {
                return Err(YangError::Stage4RegionInvalid {
                    vertex: v,
                    reason: Stage4InvalidReason::LocalRefinementRequired,
                });
            }
            let partners = &vert_partners[&v];
            let p = mesh.verts[v as usize];
            let (proj, n0, n1) = match partners.as_slice() {
                [s1] => {
                    let proj = relocate_onto_implicit_pair(p, t_surf, *s1).ok_or(
                        YangError::Stage4RegionInvalid {
                            vertex: v,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        },
                    )?;
                    let qa = proj.as_array();
                    let (_, n0) = surface_value_and_normal(t_surf, qa).ok_or(
                        YangError::Stage4RegionInvalid {
                            vertex: v,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        },
                    )?;
                    let (_, n1) = surface_value_and_normal(*s1, qa).ok_or(
                        YangError::Stage4RegionInvalid {
                            vertex: v,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        },
                    )?;
                    (proj, n0, n1)
                }
                [s1, s2] => {
                    // 3-surface junction: relocate onto {torus, s1, s2}. The
                    // displacement gate uses the torus∩s1 angle (the junction is
                    // a point; any incident curve's metric bounds the move).
                    let proj = relocate_onto_implicit_triple(p, t_surf, *s1, *s2).ok_or(
                        YangError::Stage4RegionInvalid {
                            vertex: v,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        },
                    )?;
                    let qa = proj.as_array();
                    let (_, n0) = surface_value_and_normal(t_surf, qa).ok_or(
                        YangError::Stage4RegionInvalid {
                            vertex: v,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        },
                    )?;
                    let (_, n1) = surface_value_and_normal(*s1, qa).ok_or(
                        YangError::Stage4RegionInvalid {
                            vertex: v,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        },
                    )?;
                    (proj, n0, n1)
                }
                _ => {
                    return Err(YangError::Stage4RegionInvalid {
                        vertex: v,
                        reason: Stage4InvalidReason::LocalRefinementRequired,
                    });
                }
            };
            // Derived displacement gate: a chord point moves to the exact curve
            // by ≤ 2·d_ε / sin θ, θ the angle between two incident surface
            // normals at the relocated point (the same metric as the disc∩disc /
            // cyl×cyl junction bands — NOT tolerance widening). Beyond it is a
            // real off-curve error, not a Stage-1 chord artifact → STOP.
            let pa = p.as_array();
            let qa = proj.as_array();
            let rho = ((qa[0] - pa[0]).powi(2) + (qa[1] - pa[1]).powi(2) + (qa[2] - pa[2]).powi(2))
                .sqrt();
            let cx = [
                n0[1] * n1[2] - n0[2] * n1[1],
                n0[2] * n1[0] - n0[0] * n1[2],
                n0[0] * n1[1] - n0[1] * n1[0],
            ];
            let sin_theta = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
            let gate = if sin_theta > 0.0 {
                2.0 * d_eps / sin_theta
            } else {
                f64::INFINITY
            };
            if rho > gate {
                return Err(YangError::Stage4RegionInvalid {
                    vertex: v,
                    reason: Stage4InvalidReason::OffCurveBeyondChordBand,
                });
            }
            if rho > cad_primitives::TAU_WORK {
                mesh.verts[v as usize] = proj;
                moved.insert(v);
            }
        }
    }

    // (3) §4.5.3 reversed-intersection correction sweep.
    let mut collapsed_any = false;
    let mut attr_vec = std::mem::take(&mut attribution.attributions);
    // PR-KV9: junction vertices that landed on the SAME exact point are
    // duplicates of one geometric junction (near a tangency-grade curve
    // crossing the two chord polylines can intersect several times, giving
    // several arrangement vertices for ONE junction). Collapse the extras
    // onto the lowest index — the standard edge-collapse, which drops the
    // degenerate slivers between them and keeps the half-edge pairing
    // watertight.
    {
        let mut by_pos: std::collections::BTreeMap<[u64; 3], Vec<u32>> =
            std::collections::BTreeMap::new();
        for &v in vert_ell_junction.keys() {
            let p = mesh.verts[v as usize];
            by_pos
                .entry([p.x().to_bits(), p.y().to_bits(), p.z().to_bits()])
                .or_default()
                .push(v);
        }
        for (_, group) in by_pos {
            if group.len() < 2 {
                continue;
            }
            let survivor = *group.iter().min().expect("non-empty");
            for &victim in group.iter().filter(|&&v| v != survivor) {
                collapse_vertex(mesh, &mut attr_vec, victim, survivor);
                collapsed_any = true;
            }
        }
    }
    let sweep_result = sweep_reversed_intersections(mesh, &mut attr_vec, a, b, d_eps);
    attribution.attributions = attr_vec;
    let any_collapse = sweep_result?;
    collapsed_any |= any_collapse;

    // (3c) §4.4.1(b) sub-feature-size vertex merge (Yang Fig. 11(b): "if an
    // endpoint p of the split edge is too close to q, we merge p with q"). After
    // relocation a degenerate triangle can have two vertices nearer than
    // MIN_FEATURE_SIZE — the governance feature floor (A14.2): two points closer
    // than the smallest representable feature ARE the same point. This is the
    // curved-input analog of the I6 near-weld (which is bit-exact-only for curved
    // inputs — "Stage-4 owns junction-duplicate collapse"). Merge such a pair via
    // the watertight-preserving `collapse_vertex` (higher index → lower, dropping
    // the now-degenerate slivers), iterating to a fixed point. P9/P10: the gate is
    // the GOVERNANCE feature floor, not a tuned tolerance, and a genuinely-spread
    // degenerate (vertices ≥ the floor apart — e.g. a monotonic-collinear sliver
    // on a curved patch) is left UNTOUCHED for `validate_relocated_triangles` to
    // STOP loudly / the curved-patch re-CDT (N2-2) to handle. Spec
    // `specs/yang_n2_stage4_cdt_mesh_updating.md` §5 increment N2-1.
    //
    // SCOPE NOTE (M8 holed-disc increment 3, 2026-07-06): a GLOBAL widening of
    // this scan (all triangles + a Stage-4 ENTRY pass) was tried and REVERTED —
    // at micro model scale (R0091, 1.6e-4) the ABSOLUTE floor collapses
    // legitimately-distinct arrangement geometry (Euler flipped to −4,
    // SUPPORTED_WRONG). The relocation/conic-adjacent eligibility below is
    // LOAD-BEARING: it keeps the merge away from pre-existing arrangement
    // slivers that `boolean()` legitimately kept for watertightness.
    {
        let floor = cad_primitives::MIN_FEATURE_SIZE;
        let mut attr_vec = std::mem::take(&mut attribution.attributions);
        // KV9-F3 (spec `kv9_f3_output_vertex_identity` E-V2): junction
        // duplicates that are ALREADY on their exact curve (rho ≤ TAU_WORK)
        // are never `moved`, yet they are precisely the population the I6
        // weld delegates to Stage-4 ("Stage-4 owns junction-duplicate
        // collapse" — curved inputs weld bit-exact only). Scan eligibility
        // therefore includes triangles touching any CONIC-ENDPOINT vertex;
        // the merge criterion below is unchanged (the governance
        // MIN_FEATURE_SIZE floor, A14.2 — never a tuned tolerance).
        let conic_endpoint: std::collections::BTreeSet<u32> = vert_circle
            .keys()
            .chain(vert_line.keys())
            .chain(vert_ellipse.keys())
            .chain(vert_cone_ellipse.keys())
            .chain(vert_parabola.keys())
            .chain(vert_cone_hyperbola.keys())
            .chain(vert_ell_junction.keys())
            .chain(vert_circle_junction.keys())
            .copied()
            .collect();
        // Spec `yang_453_junction_protected_collapse` §3b: closed-form junction
        // vertices (exact on TWO curves) outrank single-curve conic endpoints,
        // which outrank plain mesh vertices, in merge-survivor selection.
        let junction_verts: std::collections::BTreeSet<u32> = vert_ell_junction
            .keys()
            .chain(vert_circle_junction.keys())
            .chain(vert_junction.keys())
            .copied()
            .collect();
        // Each pass collapses ≤1 sub-feature edge; bounded by the triangle count.
        let max_merge_passes = mesh.tris.len() + 1;
        let mut merge_passes = 0usize;
        loop {
            merge_passes += 1;
            if merge_passes > max_merge_passes {
                attribution.attributions = attr_vec;
                return Err(YangError::Stage4RegionInvalid {
                    vertex: u32::MAX,
                    reason: Stage4InvalidReason::LocalRefinementRequired,
                });
            }
            let mut to_merge: Option<(u32, u32)> = None;
            for tri in &mesh.tris {
                if !tri
                    .iter()
                    .any(|v| moved.contains(v) || conic_endpoint.contains(v))
                {
                    continue;
                }
                let p0 = mesh.verts[tri[0] as usize].as_array();
                let p1 = mesh.verts[tri[1] as usize].as_array();
                let p2 = mesh.verts[tri[2] as usize].as_array();
                let nrm = tri_area_vector(p0, p1, p2);
                let twice_area = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
                if twice_area * 0.5 >= floor * floor {
                    continue; // not degenerate — leave it
                }
                // Degenerate relocated triangle: if its SHORTEST edge is below the
                // feature floor, those two endpoints are the same point → merge.
                let dist = |a: [f64; 3], b: [f64; 3]| {
                    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                };
                let edges = [
                    (tri[0], tri[1], dist(p0, p1)),
                    (tri[1], tri[2], dist(p1, p2)),
                    (tri[2], tri[0], dist(p2, p0)),
                ];
                let (u, v, len) = edges
                    .iter()
                    .copied()
                    .min_by(|x, y| x.2.partial_cmp(&y.2).unwrap_or(std::cmp::Ordering::Equal))
                    .expect("3 edges");
                if len < floor {
                    // Spec `yang_453_junction_protected_collapse` §3b: the
                    // exactness-ranked survivor (`sub_feature_merge_direction`,
                    // Yang Fig. 11(b) — the exact vertex survives) is BANKED,
                    // DELIBERATELY UNWIRED: wiring it flips R0091 from its
                    // loud ellipse-endpoint ERROR to SUPPORTED_WRONG
                    // (χ = −4 vs meta 2; unverifiable in-session — see spec
                    // §3b status). The index rule stays until the R0091
                    // output's true χ is verified (sidecar reference parity)
                    // or the meta χ is refuted from the authored numbers.
                    let _ = &junction_verts;
                    let survivor = u.min(v);
                    let victim = u.max(v);
                    to_merge = Some((victim, survivor));
                    break;
                }
            }
            match to_merge {
                Some((victim, survivor)) => {
                    collapse_vertex(mesh, &mut attr_vec, victim, survivor);
                    collapsed_any = true;
                }
                None => break,
            }
        }
        attribution.attributions = attr_vec;
    }

    // KV9-F1 Increment 0c census: post-merge junction-twin state — coincident
    // junction vertices that SURVIVED the §4.4.1(b) merge, and whether the
    // survivors are edge-adjacent in the current mesh (kept env-gated).
    if std::env::var("KV9_JUNCTION_PROBE").is_ok() {
        let keys: Vec<u32> = vert_ell_junction.keys().copied().collect();
        for (i, &u) in keys.iter().enumerate() {
            for &w in &keys[i + 1..] {
                let (pu, pw) = (mesh.verts[u as usize], mesh.verts[w as usize]);
                if pu.as_array() != pw.as_array() {
                    continue;
                }
                let adjacent = mesh.tris.iter().any(|t| t.contains(&u) && t.contains(&w));
                let (du, dw) = (
                    mesh.tris.iter().filter(|t| t.contains(&u)).count(),
                    mesh.tris.iter().filter(|t| t.contains(&w)).count(),
                );
                eprintln!(
                    "KV9_JUNCTION_PROBE post-merge coincident twins: v{u} v{w} at {:?} \
                     edge_adjacent={adjacent} deg({u})={du} deg({w})={dw}",
                    pu.as_array()
                );
            }
        }
    }

    // (3d) §4.4.1(a) edge-split (Yang Fig. 11(a): "locate the constrained edge
    // containing q, split it at q"). A degenerate relocated triangle D=[a,b,c] is
    // collinear: the vertex OFF its longest edge (`b`) lies on that long edge
    // `a-c` (a redundant intersection point on the constraint curve). The faithful
    // fix inserts `b` into the triangle ON THE OTHER SIDE of `a-c` — split that
    // neighbour N=[a,c,d] into [a,b,d]+[b,c,d] — and drops D. This is a LOCAL,
    // watertight-preserving operation (D's edges a-b/b-c re-pair with the split
    // halves; the long edge a-c, shared only by D and N, vanishes): no re-CDT, no
    // parametric domain, no cylinder θ-seam. Iterate, each step acting on a
    // degenerate triangle whose long-edge neighbour is NON-degenerate (so the
    // strip unzips from its non-degenerate margin inward); a remaining degenerate
    // triangle with no non-degenerate neighbour is a genuine §4.5.2 STOP. Spec
    // `specs/yang_n2_stage4_cdt_mesh_updating.md`.
    {
        let degen_area = cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE;
        let is_degen = |ti: usize, mesh: &Mesh| -> bool {
            let t = mesh.tris[ti];
            if !t.iter().any(|v| moved.contains(v)) {
                return false;
            }
            let av = tri_area_vector(
                mesh.verts[t[0] as usize].as_array(),
                mesh.verts[t[1] as usize].as_array(),
                mesh.verts[t[2] as usize].as_array(),
            );
            (av[0] * av[0] + av[1] * av[1] + av[2] * av[2]).sqrt() * 0.5 < degen_area
        };
        // The off-longest-edge vertex `b` (the collinear middle) + extremes a,c.
        let long_edge_off = |t: &[u32; 3], mesh: &Mesh| -> (u32, u32, u32) {
            let d = |i: usize, j: usize| {
                let p = mesh.verts[t[i] as usize].as_array();
                let q = mesh.verts[t[j] as usize].as_array();
                let e = [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
                e[0] * e[0] + e[1] * e[1] + e[2] * e[2]
            };
            let (e01, e12, e20) = (d(0, 1), d(1, 2), d(2, 0));
            if e01 >= e12 && e01 >= e20 {
                (t[0], t[1], t[2]) // long a-c = v0-v1, off b = v2
            } else if e12 >= e20 {
                (t[1], t[2], t[0])
            } else {
                (t[2], t[0], t[1])
            }
        };
        let mut attr_vec = std::mem::take(&mut attribution.attributions);
        let max_passes = mesh.tris.len() + 1;
        let mut passes = 0usize;
        loop {
            passes += 1;
            if passes > max_passes {
                attribution.attributions = attr_vec;
                return Err(YangError::Stage4RegionInvalid {
                    vertex: u32::MAX,
                    reason: Stage4InvalidReason::LocalRefinementRequired,
                });
            }
            // Edge → incident triangle indices (for the across-edge neighbour).
            let mut edge_tris: std::collections::HashMap<(u32, u32), Vec<u32>> =
                std::collections::HashMap::new();
            for (ti, tri) in mesh.tris.iter().enumerate() {
                for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                    let (u, v) = (tri[i], tri[j]);
                    let key = if u < v { (u, v) } else { (v, u) };
                    edge_tris.entry(key).or_default().push(ti as u32);
                }
            }
            // Pick a degenerate triangle whose long-edge neighbour is non-degenerate.
            let mut action: Option<(usize, usize, u32, u32, u32)> = None;
            let mut any_degen = false;
            for ti in 0..mesh.tris.len() {
                if !is_degen(ti, mesh) {
                    continue;
                }
                any_degen = true;
                let (a, c, b) = long_edge_off(&mesh.tris[ti], mesh);
                let key = if a < c { (a, c) } else { (c, a) };
                let inc = match edge_tris.get(&key) {
                    Some(v) if v.len() == 2 => v,
                    _ => continue, // boundary / non-manifold long edge — skip
                };
                let n = if inc[0] as usize == ti {
                    inc[1]
                } else {
                    inc[0]
                } as usize;
                if is_degen(n, mesh) {
                    continue; // defer until the neighbour is resolved
                }
                action = Some((ti, n, a, c, b));
                break;
            }
            let (d_idx, n_idx, a, c, b) = match action {
                Some(x) => x,
                None => {
                    if any_degen {
                        // Degenerate triangles remain but none has a non-degenerate
                        // long-edge neighbour — genuine local-refinement territory.
                        attribution.attributions = attr_vec;
                        return Err(YangError::Stage4RegionInvalid {
                            vertex: u32::MAX,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        });
                    }
                    break; // no degenerate relocated triangles remain
                }
            };
            // Split N=[a,c,d] at b → [a,b,d] + [b,c,d], wound like N; drop D.
            let nt = mesh.tris[n_idx];
            let dd = nt
                .iter()
                .copied()
                .find(|&v| v != a && v != c)
                .expect("neighbour shares edge a-c, has a third vertex");
            let n_norm = tri_area_vector(
                mesh.verts[nt[0] as usize].as_array(),
                mesh.verts[nt[1] as usize].as_array(),
                mesh.verts[nt[2] as usize].as_array(),
            );
            let mut t1 = [a, b, dd];
            let mut t2 = [b, c, dd];
            orient_tri(&mesh.verts, &mut t1, n_norm);
            orient_tri(&mesh.verts, &mut t2, n_norm);
            let n_attr = attr_vec.get(n_idx).copied().flatten();
            // Rebuild tris + attribution, dropping D and N, appending the split.
            let mut new_tris: Vec<[u32; 3]> = Vec::with_capacity(mesh.tris.len() + 1);
            let mut new_attr: Vec<Option<TriangleAttribution>> =
                Vec::with_capacity(attr_vec.len() + 1);
            for (i, t) in mesh.tris.iter().enumerate() {
                if i == d_idx || i == n_idx {
                    continue;
                }
                new_tris.push(*t);
                new_attr.push(attr_vec.get(i).copied().flatten());
            }
            new_tris.push(t1);
            new_attr.push(n_attr);
            new_tris.push(t2);
            new_attr.push(n_attr);
            *mesh = Mesh::new(std::mem::take(&mut mesh.verts), new_tris);
            attr_vec = new_attr;
            collapsed_any = true;
        }
        attribution.attributions = attr_vec;
    }

    // KV9-F3 diagnosis probe (read-only, env-gated): census near-twin mesh
    // vertex pairs at Stage-4 exit with their merge-eligibility context —
    // `moved` membership, shared-triangle adjacency, curve assignments.
    if std::env::var_os("YANG_S4_TWIN_PROBE").is_some() {
        let n = mesh.verts.len();
        let scale = mesh
            .verts
            .iter()
            .flat_map(|p| p.as_array())
            .fold(1.0_f64, |m, c| m.max(c.abs()));
        let band = 1.0e-9 * scale;
        for i in 0..n {
            for j in (i + 1)..n {
                let (p, q) = (mesh.verts[i].as_array(), mesh.verts[j].as_array());
                let d2 = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
                if d2 > band * band || d2 == 0.0 {
                    continue;
                }
                let (iu, ju) = (i as u32, j as u32);
                let shared_tri = mesh
                    .tris
                    .iter()
                    .position(|t| t.contains(&iu) && t.contains(&ju));
                eprintln!(
                    "[s4-twin-probe] verts {i}/{j} dist={:e} moved=({},{}) shared_tri={:?}\n  \
                     circle=({},{}) line=({},{}) ell=({},{}) junction=({},{})\n  \
                     {i}: ({},{},{})\n  {j}: ({},{},{})",
                    d2.sqrt(),
                    moved.contains(&iu),
                    moved.contains(&ju),
                    shared_tri,
                    vert_circle.contains_key(&iu),
                    vert_circle.contains_key(&ju),
                    vert_line.contains_key(&iu),
                    vert_line.contains_key(&ju),
                    vert_ellipse.contains_key(&iu),
                    vert_ellipse.contains_key(&ju),
                    vert_ell_junction.contains_key(&iu),
                    vert_ell_junction.contains_key(&ju),
                    p[0],
                    p[1],
                    p[2],
                    q[0],
                    q[1],
                    q[2]
                );
            }
        }
    }

    // (4) Validate every RELOCATED triangle (one touching a moved vertex) for
    // non-degeneracy (Yang §4.5 step 4). Reversed intersections are handled by
    // the §4.5.3 sweep above; watertightness by the global gate below (§4.4.3).
    validate_relocated_triangles(mesh, attribution, &moved)?;
    // (4b) Explicit Stage-4 watertightness gate (§4.4.3).
    check_watertight_2manifold(mesh)?;

    // After a collapse the vertex set may have lost some relocated verts; keep
    // only relocations whose vertex still carries a conic output edge. The
    // caller resolves the output-edge index; relocations referencing a
    // now-absent vertex are simply not emitted (the caller guards the index).
    Ok((relocations, collapsed_any))
}

/// PR-YR10 (§4.5.3): walk every ordered intersection loop and correct reversed
/// points by edge-collapsing the offending next-point. Returns `true` iff any
/// collapse occurred. LOUD STOP on an unresolvable reversal.
fn sweep_reversed_intersections(
    mesh: &mut Mesh,
    attribution: &mut Vec<Option<TriangleAttribution>>,
    a: &BRep,
    b: &BRep,
    _d_eps: f64,
) -> Result<bool, YangError> {
    use std::collections::HashSet;
    const ANG_TOL: f64 = 1e-6; // radians (Yang §5).
    let lo = std::f64::consts::FRAC_PI_4 - ANG_TOL; // 45° − tol
    let hi = 3.0 * std::f64::consts::FRAC_PI_4 + ANG_TOL; // 135° + tol

    let mut collapsed_any = false;
    // Bound the outer restart loop by the initial triangle count (each pass
    // either makes progress by collapsing ≥1 triangle or terminates).
    let max_passes = mesh.tris.len() + 1;
    let mut passes = 0usize;
    loop {
        passes += 1;
        if passes > max_passes {
            // Could not reach a fixed point — genuine §4.5.2 territory.
            return Err(YangError::Stage4RegionInvalid {
                vertex: u32::MAX,
                reason: Stage4InvalidReason::LocalRefinementRequired,
            });
        }

        // Recompute Phase A so the loops reflect any prior collapse (spec §4.5.3
        // step 3 — re-sweep on fresh loops, never stale ones).
        let map = TriangleAttributionMap {
            attributions: std::mem::take(attribution),
        };
        let phase_a = compute_phase_a(mesh, &map, a, b);
        *attribution = map.attributions;
        let (infos, _inc, curves) = phase_a?;

        // Collect the ordered intersection loops: a patch boundary cycle whose
        // every edge carries a `Circle` curve. Dedup by sorted vertex set so the
        // cylinder-side and cap-side copies of the same ring are swept once.
        let mut seen: HashSet<Vec<u32>> = HashSet::new();
        let mut loops: Vec<Vec<(u32, u32)>> = Vec::new();
        for info in &infos {
            for cycle in &info.cycles {
                if cycle.len() < 3 {
                    continue;
                }
                // PR-YR11: widen to `all_conic` — every edge is a Circle OR an
                // Ellipse (the oblique cap sections), still EXCLUDING LineSegment.
                let all_conic = cycle.iter().all(|&(s, e)| {
                    let key = if s < e { (s, e) } else { (e, s) };
                    matches!(
                        curves.get(&key),
                        Some(Curve::Circle { .. }) | Some(Curve::Ellipse { .. })
                    )
                });
                if !all_conic {
                    continue;
                }
                let mut sorted: Vec<u32> = cycle.iter().map(|&(s, _)| s).collect();
                sorted.sort_unstable();
                if seen.insert(sorted) {
                    loops.push(cycle.clone());
                }
            }
        }

        // Find the FIRST reversal across all loops; collapse, then restart the
        // whole sweep (re-deriving loops). Deterministic: loops are in the
        // deterministic patch/cycle order; within a loop we scan in order.
        let mut acted = false;
        'outer: for cycle in &loops {
            let m = cycle.len();
            if m < 3 {
                return Err(YangError::Stage4RegionInvalid {
                    vertex: cycle.first().map(|&(s, _)| s).unwrap_or(u32::MAX),
                    reason: Stage4InvalidReason::LoopTooSmall,
                });
            }
            // Ordered vertex sequence of the loop (start vertices).
            let verts: Vec<u32> = cycle.iter().map(|&(s, _)| s).collect();
            for i in 0..m {
                let p_b = verts[(i + m - 1) % m];
                let p_r = verts[i];
                let p_n = verts[(i + 1) % m];
                if is_reversed(mesh, &curves, p_b, p_r, p_n, lo, hi) {
                    // Spec `yang_453_junction_protected_collapse` §3: pick the
                    // collapse victim so a curve-junction vertex (the exact
                    // endpoint shared by two different conic sections) always
                    // survives — Yang §4.5.3 removes points progressing along
                    // ONE curve C, never C's endpoints.
                    let p_after = verts[(i + 2) % m];
                    let (victim, survivor) =
                        reversal_collapse_direction(&curves, p_r, p_n, p_after);
                    if std::env::var_os("YANG_V_PROBE").is_some() {
                        eprintln!(
                            "YANG_V_PROBE reversal collapse: p_b={p_b} p_r={p_r} p_n={p_n} \
                             victim={victim} survivor={survivor} at {:?} <- {:?}",
                            mesh.verts.get(survivor as usize),
                            mesh.verts.get(victim as usize),
                        );
                    }
                    let dropped = collapse_vertex(mesh, attribution, victim, survivor);
                    if dropped == 0 {
                        // Nothing collapsed ⇒ cannot make progress on this
                        // reversal. LOUD STOP.
                        return Err(YangError::Stage4ReversalUnresolved {
                            edge: if p_r < p_n { (p_r, p_n) } else { (p_n, p_r) },
                            vertex: p_r,
                        });
                    }
                    collapsed_any = true;
                    acted = true;
                    break 'outer;
                }
            }
        }

        if !acted {
            // Fixed point: no reversal remains.
            return Ok(collapsed_any);
        }
    }
}

/// §4.5.3 collapse direction (spec `yang_453_junction_protected_collapse` §3):
/// which loop vertex is REMOVED for a reversal detected at `p_r` with next
/// point `p_n` (whose own next point is `p_after`)? Returns
/// `(victim, survivor)` for [`collapse_vertex`].
///
/// Yang §4.5.3 (Fig. 15, `refs/text/yang2025_hybrid_boolean.txt:709-745`)
/// removes `p_n` — but its setting is consecutive points progressing along ONE
/// intersection curve C. When `p_n` is a curve JUNCTION (the loop's curve
/// changes there: `curve(p_r,p_n) ≠ curve(p_n,p_after)`), `p_n` is C's exact
/// closed-form endpoint and must survive; the out-of-order point is `p_r`
/// itself, whose §4.4.1 relocation overshot C's end — so `p_r` collapses onto
/// the junction. `is_reversed` returning true implies both edges at `p_r`
/// carry the SAME curve (PR-KV11 guard), so `p_r` is never itself a junction
/// here, and the victim always lies on the survivor's curve (spec I3).
fn reversal_collapse_direction(
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    p_r: u32,
    p_n: u32,
    p_after: u32,
) -> (u32, u32) {
    let key_n = if p_r < p_n { (p_r, p_n) } else { (p_n, p_r) };
    let key_after = if p_n < p_after {
        (p_n, p_after)
    } else {
        (p_after, p_n)
    };
    match (curves.get(&key_n), curves.get(&key_after)) {
        (Some(cn), Some(ca)) if cn != ca => (p_r, p_n),
        _ => (p_n, p_r),
    }
}

/// §4.4.1(b) merge direction (spec `yang_453_junction_protected_collapse`
/// §3b): which vertex of a sub-feature-floor edge `(u, v)` is REMOVED?
/// Returns `(victim, survivor)` for [`collapse_vertex`].
///
/// Yang Fig. 11(b) merges the split-edge endpoint INTO the existing exact
/// intersection point ("if an endpoint p of the split edge is too close to q,
/// we merge p with q") — the exact vertex survives. Rank: closed-form
/// junction (exact on TWO curves) > single-curve conic endpoint > plain mesh
/// vertex; equal ranks keep the lower-index-survives rule byte-identical to
/// the pre-fix behavior.
///
/// BANKED, DELIBERATELY UNWIRED (spec §3b status): wiring this at the (3c)
/// merge call site flips R0091 ERROR → SUPPORTED_WRONG (χ = −4 vs meta 2,
/// unverifiable in-session). Unit-tested + mutation-killed; wire it when the
/// R0091 output's true χ is verified via sidecar reference parity or the
/// meta χ is refuted.
#[allow(dead_code)]
fn sub_feature_merge_direction(
    junction_verts: &std::collections::BTreeSet<u32>,
    conic_endpoint: &std::collections::BTreeSet<u32>,
    u: u32,
    v: u32,
) -> (u32, u32) {
    let rank = |x: u32| -> u8 {
        if junction_verts.contains(&x) {
            2
        } else if conic_endpoint.contains(&x) {
            1
        } else {
            0
        }
    };
    match rank(u).cmp(&rank(v)) {
        std::cmp::Ordering::Greater => (v, u),
        std::cmp::Ordering::Less => (u, v),
        std::cmp::Ordering::Equal => (u.max(v), u.min(v)),
    }
}

/// PR-YR10 (§4.5.3): is `p_r` a reversed intersection point? Compares the
/// discrete polyline tangent `t̃ = unit(p_r − p_b) + unit(p_n − p_r)` against the
/// exact circle tangent at `p_r`. Collinear `t̃` (`|t̃| < TAU_WORK`) is the
/// HEALTHY case — skip the angle test (Yang §4.5.3). Reversal ⟺ the unsigned
/// angle ∈ (45°, 135°) (with the supplied 1e-6 rad slack baked into `lo`/`hi`).
#[allow(clippy::too_many_arguments)]
fn is_reversed(
    mesh: &Mesh,
    curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    p_b: u32,
    p_r: u32,
    p_n: u32,
    lo: f64,
    hi: f64,
) -> bool {
    // PR-KV11: the §4.5.3 test is defined for points progressing along ONE
    // intersection curve C ("p_r is a point on the intersection curve C
    // between the two surfaces S_A and S_B", refs/text/yang2025_hybrid_
    // boolean.txt:709-745). A vertex where the loop TRANSITIONS between two
    // different conics (the ellipse×ellipse box-edge junction) is a genuine
    // corner — the discrete tangent legitimately kinks there and the angle
    // test against either single curve's tangent false-positives, collapsing
    // the junction loop vertex by vertex (the kv11 vanishing-bulge failure).
    {
        let key_n = if p_r < p_n { (p_r, p_n) } else { (p_n, p_r) };
        let key_b = if p_b < p_r { (p_b, p_r) } else { (p_r, p_b) };
        if let (Some(cn), Some(cb)) = (curves.get(&key_n), curves.get(&key_b)) {
            if cn != cb {
                return false;
            }
        }
    }
    let pb = mesh.verts[p_b as usize].as_array();
    let pr = mesh.verts[p_r as usize].as_array();
    let pn = mesh.verts[p_n as usize].as_array();
    let v1 = normalize3([pr[0] - pb[0], pr[1] - pb[1], pr[2] - pb[2]]);
    let v2 = normalize3([pn[0] - pr[0], pn[1] - pr[1], pn[2] - pr[2]]);
    let t_tilde = [v1[0] + v2[0], v1[1] + v2[1], v1[2] + v2[2]];
    let t_tilde_len =
        (t_tilde[0] * t_tilde[0] + t_tilde[1] * t_tilde[1] + t_tilde[2] * t_tilde[2]).sqrt();
    if t_tilde_len < cad_primitives::TAU_WORK {
        // Degenerate/collinear t̃ (|t̃| ≈ 0 ⟺ v1 ≈ −v2 ⟺ the polyline doubles
        // back at p_r). Yang §4.5.3 (lines 743-745) places this collinear case
        // WITHIN the reversal subset — the angle test is undefined here, so
        // "directly detect the reversal, avoiding the angle comparisons." A
        // U-turn IS a reversal. (Prior code returned `false`/"healthy" — the N3
        // logic inversion; see docs/yang_deviations.md.)
        return true;
    }

    // Exact conic tangent at p_r. Find the Circle OR Ellipse this edge carries
    // (PR-YR11: ellipse edges compute the ellipse tangent). Prefer the current
    // edge `(p_r, p_n)`; fall back to the previous edge `(p_b, p_r)`.
    let key = if p_r < p_n { (p_r, p_n) } else { (p_n, p_r) };
    let key2 = if p_b < p_r { (p_b, p_r) } else { (p_r, p_b) };
    let conic = match curves.get(&key) {
        Some(c @ (Curve::Circle { .. } | Curve::Ellipse { .. })) => Some(*c),
        _ => match curves.get(&key2) {
            Some(c @ (Curve::Circle { .. } | Curve::Ellipse { .. })) => Some(*c),
            _ => None,
        },
    };
    let Some(conic) = conic else {
        // No exact tangent available — cannot diagnose; treat as healthy
        // (the validation pass still guards inverted/degenerate triangles).
        return false;
    };
    let p_r_pt = mesh.verts[p_r as usize];
    let tan_c = match conic {
        Curve::Parabola {
            vertex,
            normal,
            axis_dir,
            focal_length,
        } => {
            // PR-YR22: parabola tangent `d/dt point(t) = (t/(2f))·axis_dir +
            // (normal × axis_dir)`, evaluated at the conjugate-axis coordinate
            // `t = (p_r − vertex)·(normal × axis_dir)` (the same tag the Stage-4
            // parabola loop stores). Defensively correct even though the open-arc
            // parabola section is excluded from the closed-loop `all_conic` sweep.
            let n = normalize3(normal.as_array());
            let ax = normalize3(axis_dir.as_array());
            let conj = [
                n[1] * ax[2] - n[2] * ax[1],
                n[2] * ax[0] - n[0] * ax[2],
                n[0] * ax[1] - n[1] * ax[0],
            ];
            let vtx = vertex.as_array();
            let pr = p_r_pt.as_array();
            let t = (pr[0] - vtx[0]) * conj[0]
                + (pr[1] - vtx[1]) * conj[1]
                + (pr[2] - vtx[2]) * conj[2];
            normalize3([
                (t / (2.0 * focal_length)) * ax[0] + conj[0],
                (t / (2.0 * focal_length)) * ax[1] + conj[1],
                (t / (2.0 * focal_length)) * ax[2] + conj[2],
            ])
        }
        Curve::Circle {
            center,
            normal,
            radius,
        } => {
            // Circle tangent: derivative of `center + r(cos t·e1 + sin t·e2)`
            // ⇒ `-sin t·e1 + cos t·e2`.
            let Ok((_proj, t)) = project_onto_circle(p_r_pt, center, normal, radius) else {
                return false;
            };
            let (e1, e2) = ortho_basis(normal);
            let e1a = e1.as_array();
            let e2a = e2.as_array();
            let (st, ct) = (t.sin(), t.cos());
            normalize3([
                -st * e1a[0] + ct * e2a[0],
                -st * e1a[1] + ct * e2a[1],
                -st * e1a[2] + ct * e2a[2],
            ])
        }
        Curve::Ellipse {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => {
            // PR-YR11: ellipse tangent `−a·sin t·major + b·cos t·minor_dir` at the
            // p_r parameter, in the shared ellipse frame (spec §3).
            let t = ellipse_param(
                p_r_pt,
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
            );
            normalize3(ellipse_tangent(
                normal,
                major_axis,
                major_radius,
                minor_radius,
                t,
            ))
        }
        Curve::Hyperbola {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } => {
            // PR-YR23: hyperbola tangent `d/dt point(t) = a·sinh(t)·major +
            // b·cosh(t)·(normal × major_axis)`, evaluated at the tag
            // `t = asinh(v_coord / b)` with `v_coord = (p_r − center)·
            // (normal × major_axis)` (the same tag the Stage-4 hyperbola loop
            // stores). Defensively correct even though the open-arc hyperbola
            // section is excluded from the closed-loop `all_conic` sweep
            // (which selects only Circle/Ellipse), so this arm is never reached.
            let n = normalize3(normal.as_array());
            let maj = normalize3(major_axis.as_array());
            let conj = [
                n[1] * maj[2] - n[2] * maj[1],
                n[2] * maj[0] - n[0] * maj[2],
                n[0] * maj[1] - n[1] * maj[0],
            ];
            let ctr = center.as_array();
            let pr = p_r_pt.as_array();
            let v_coord = (pr[0] - ctr[0]) * conj[0]
                + (pr[1] - ctr[1]) * conj[1]
                + (pr[2] - ctr[2]) * conj[2];
            let t = (v_coord / semi_conjugate).asinh();
            let (sh, ch) = (t.sinh(), t.cosh());
            normalize3([
                semi_transverse * sh * maj[0] + semi_conjugate * ch * conj[0],
                semi_transverse * sh * maj[1] + semi_conjugate * ch * conj[1],
                semi_transverse * sh * maj[2] + semi_conjugate * ch * conj[2],
            ])
        }
        Curve::LineSegment => return false,
    };
    let t_tilde_u = normalize3(t_tilde);
    let dotv = (t_tilde_u[0] * tan_c[0] + t_tilde_u[1] * tan_c[1] + t_tilde_u[2] * tan_c[2])
        .clamp(-1.0, 1.0);
    // Unsigned angle between t̃ and the exact tangent (sign of the tangent is
    // arbitrary, so fold to [0, π/2] via |dot|).
    let angle = dotv.abs().acos();
    angle > lo && angle < hi
}

/// Unnormalized triangle area-vector `(p1−p0) × (p2−p0)` (= 2·area·n̂).
fn tri_area_vector(p0: [f64; 3], p1: [f64; 3], p2: [f64; 3]) -> [f64; 3] {
    let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ]
}

/// PR-YR10 (Yang §4.4.1 / §4.4.3 / §4.5 step 4): validate every RELOCATED
/// triangle (one touching a `moved` vertex) for **non-degeneracy** — its
/// post-relocation area must stay ≥ `MIN_FEATURE_SIZE²`, else
/// `DegenerateTriangle`. Triangles untouched by relocation are skipped:
/// `boolean()` legitimately keeps near-zero-area arrangement slivers for
/// watertightness, which Stage 4 must not re-litigate.
///
/// **Why there is no per-facet absolute "winding vs analytic normal" gate.**
/// Yang §4.4.1 states plainly that relocating the discrete crossing points onto
/// the exact curve "essentially breaks bijectivity, causing gaps or
/// self-intersections," and that **watertightness is inherited from the
/// mesh-boolean output and repaired locally** (§4.4.3) — it is NOT re-derived
/// per facet. The genuine *reversed-intersection* defect (§4.5.3) is a
/// non-monotonic ordering of points ALONG an intersection curve; that is
/// detected and corrected by the polyline-tangent sweep
/// (`sweep_reversed_intersections`) on the ordered conic loops, which either
/// fixes it (edge-collapse) or STOPs loudly (`Stage4ReversalUnresolved` /
/// `LocalRefinementRequired`). What remains after a monotonic-loop sweep is the
/// benign in-surface self-intersection Yang accepts: e.g. a planar cap-fan
/// triangle bridging the relocated ring to a fixed box corner can locally fold
/// WITHIN its (unchanged) supporting plane when a ring vertex moves outward onto
/// the true circle. That fold does NOT move the cap off its exact `Plane`, does
/// NOT reverse the intersection curve, and does NOT break watertightness (pure
/// relocation leaves mesh connectivity — hence half-edge pairing and χ —
/// untouched). An absolute pointwise `dot(winding, surface_normal) > 0` test
/// false-positives on exactly these facets (verified: the cap facet's kept
/// winding is opposite the box's stored cap normal before
/// `reconstruct_topology`'s Newell orientation pass reconciles it; and a
/// faceted cylinder's facet normal legitimately deviates from the pointwise
/// centroid radial by up to the facet half-angle). The faithful output
/// invariant is therefore: non-degenerate relocated facets + the §4.5.3 sweep +
/// the global `check_watertight_2manifold` gate (§4.4.3) — not a per-facet
/// winding sign.
fn validate_relocated_triangles(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    moved: &std::collections::HashSet<u32>,
) -> Result<(), YangError> {
    let _ = attribution; // attribution no longer consulted (no per-facet normal gate)
    for tri in &mesh.tris {
        // Only triangles incident to a relocated (moved) vertex are validated.
        if !tri.iter().any(|v| moved.contains(v)) {
            continue;
        }
        let p0 = mesh.verts[tri[0] as usize].as_array();
        let p1 = mesh.verts[tri[1] as usize].as_array();
        let p2 = mesh.verts[tri[2] as usize].as_array();
        let nrm = tri_area_vector(p0, p1, p2);
        let twice_area = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
        if twice_area * 0.5 < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE {
            if std::env::var_os("YANG_RELOC_PROBE").is_some() {
                eprintln!(
                    "[reloc-degen] tri={tri:?} moved={:?} p0={p0:?} p1={p1:?} p2={p2:?} 2A={twice_area}",
                    tri.iter().map(|v| moved.contains(v)).collect::<Vec<_>>()
                );
            }
            return Err(YangError::Stage4RegionInvalid {
                vertex: tri[0],
                reason: Stage4InvalidReason::DegenerateTriangle,
            });
        }
    }
    Ok(())
}

/// PR-YR5: rebuild output `BRep` topology (`vertices`, `edges`,
/// `faces`) from the per-triangle attribution map.
///
/// Algorithm:
/// 1. Build per-triangle adjacency via canonical-edge BTreeMap.
/// 2. Flood-fill same-attribution patches. Skip None-attributed
///    triangles (cut surfaces → PR-YR6).
/// 3. For each patch, walk ALL directed boundary cycles (edges in
///    exactly one patch triangle, ordered).
/// 4. Classify cycles outer (signed area > 0) vs inner (< 0) along the
///    face normal; build `BRepFace { outer_loop, inner_loops }` (PR-YR5c).
/// 5. Inherit `surface` from `input.faces()[attribution.face]`.
/// 6. Output `vertices` is 1:1 with `mesh.verts`.
///
/// Errors:
/// - `NonManifoldOutput`: cycle walking dead-ends / T-junctions (E1),
///   a degenerate loop (E2), or not exactly one positive-area cycle
///   (E3 — disconnected / nested patch, out of scope).
/// - `MalformedTopology`: defensive; `attribution.face` out of range
///   in the input BRep.
///
/// PR-YR10: the production boolean path now goes through
/// [`reconstruct_topology_stage4`] (which runs Stage 4 then shares the same
/// [`emit_topology`]). This `&Mesh` / 3-tuple form is retained for the PR-YR5/9
/// unit-test callers (no-conic fixtures where Stage 4 would be a strict no-op),
/// hence `#[cfg(test)]`.
#[cfg(test)]
fn reconstruct_topology(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
) -> Result<LegacyTopology, YangError> {
    // PR-YR9 path (unchanged signature, used by the unit tests): build Phase A
    // and emit with NO Stage-4 relocation (these fixtures carry no conic edges,
    // so Stage 4 would be a strict no-op anyway). The Stage-4-aware entry point
    // is `reconstruct_topology_stage4`, called by `boolean()`.
    let (infos, _incidence, intersection_curves) = compute_phase_a(mesh, attribution, a, b)?;
    let (vertices, edges, faces, _sources, _face_attr) =
        emit_topology(mesh, &infos, &intersection_curves, &[], BoolOp::Union)?;
    Ok((vertices, edges, faces))
}

/// PR-YR10: the Stage-4-aware reconstruction `boolean()` calls. Builds Phase A,
/// runs Stage 4 (relocate intersection points onto the exact curves + §4.5.3
/// reversed-point correction), recomputes Phase A after any §4.5.3 collapse,
/// then runs the SAME Phase-B emission as `reconstruct_topology` (via the shared
/// [`emit_topology`]). Returns the 4-tuple including the per-output-vertex
/// `TessellationSource` vector (relocated verts → `BRepEdge { edge, t }`).
fn reconstruct_topology_stage4(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
    op: BoolOp,
) -> Result<ReconstructedTopology, YangError> {
    // (4) Phase A: per-patch ordered loops + inherited surface (`infos`), and the
    // exact per-edge intersection `Curve` map.
    let (mut infos, incidence, mut intersection_curves) = compute_phase_a(mesh, attribution, a, b)?;

    // KV9-F1 diagnosis probe (read-only, env-gated): kept-set attribution
    // census + per-patch summary at Stage-6 entry.
    if std::env::var_os("YANG_S6_PATCH_PROBE").is_some() {
        let (mut na, mut nb, mut none) = (0usize, 0usize, 0usize);
        for att in &attribution.attributions {
            match att {
                Some(TriangleAttribution {
                    input: InputId::A, ..
                }) => na += 1,
                Some(TriangleAttribution {
                    input: InputId::B, ..
                }) => nb += 1,
                None => none += 1,
            }
        }
        eprintln!(
            "[s6-patch-probe] kept tris: A={na} B={nb} none={none} (mesh tris {})",
            mesh.tris.len()
        );
        for (i, info) in infos.iter().enumerate() {
            eprintln!(
                "[s6-patch-probe] patch {i}: input {:?} face {} cycles {:?} fold_sliver {}",
                info.input,
                info.face_idx,
                info.cycles.iter().map(|c| c.len()).collect::<Vec<_>>(),
                info.had_fold_sliver
            );
        }
    }

    // (4a) Stage 4 (seam A1): relocate onto the exact analytical curves
    // (Yang §4.4.1) + §4.5.3 reversal correction. Entered on ANY analytic conic
    // (Circle OR Ellipse) so an ellipse-only fixture reaches the loud
    // `EllipseProjectionUnsupported` STOP rather than silently passing an
    // un-relocated mesh. No conic edges ⇒ Stage 4 is a strict no-op (planar
    // byte-identity).
    // PR-YR22: include `Parabola` so a parabola-only fixture enters Stage 4 and
    // its cone-parabola seam is relocated onto the exact section.
    // PR-YR23: include `Hyperbola` likewise so a hyperbola edge enters Stage 4.
    // PR-F3: ALSO enter Stage 4 when a `LineSegment` intersection edge has a
    // CURVED surface in its incidence — that is a plane∥axis × cylinder ruling
    // line (ssi C3a/C3b) whose arrangement points sit on Stage-1 facet chords
    // and need relocation onto the exact line. A plane∩plane segment is exact
    // and does NOT trigger Stage 4 (planar byte-identity preserved).
    let has_conic = intersection_curves.iter().any(|(key, c)| {
        matches!(
            c,
            Curve::Circle { .. }
                | Curve::Ellipse { .. }
                | Curve::Parabola { .. }
                | Curve::Hyperbola { .. }
        ) || (matches!(c, Curve::LineSegment)
            && incidence.get(key).is_some_and(|entries| {
                entries
                    .iter()
                    .any(|&(_, s)| !matches!(s, Surface::Plane { .. }))
            }))
    });
    // KV6d Tier B: a TORUS intersection edge is degree-4 (never conic), so it
    // does not register above — but its endpoints sit on Stage-1 chords off the
    // analytic torus and need the implicit-pair Newton relocation in Stage 4.
    let has_torus = incidence
        .values()
        .any(|es| es.iter().any(|(_, s)| matches!(s, Surface::Torus { .. })));
    let has_conic = has_conic || has_torus;
    // (vertex, circle-frame angle t) for every relocated / retagged intersection
    // vertex. Mapped to `BRepEdge { edge, t }` sources in `emit_topology` once
    // the output edges exist.
    let mut relocations: Vec<(u32, f64)> = Vec::new();
    if has_conic {
        let (relocs, collapsed) = stage4_relocate_and_correct(mesh, attribution, a, b)?;
        relocations = relocs;
        // A §4.5.3 collapse mutated the mesh topology + attribution, so the
        // pre-collapse Phase-A loops are stale (spec §4.1 note). Recompute them
        // before the Phase-B emission re-validates the corrected mesh.
        if collapsed {
            // PR-YR11: drop the vertices the collapse left unreferenced (and
            // remap triangle indices + `relocations`) BEFORE recomputing Phase A,
            // so the emitted output mesh carries no dangling vertices (a global
            // V−E+F = 2 for a single closed shell). Strict no-op when there were
            // no danglers.
            compact_unreferenced_verts(mesh, &mut relocations);
            let (i2, _inc2, cv2) = compute_phase_a(mesh, attribution, a, b)?;
            infos = i2;
            intersection_curves = cv2;
        }
    }

    emit_topology(mesh, &infos, &intersection_curves, &relocations, op)
}

/// PR-YR5/YR9 Phase-B emission (factored out in PR-YR10 so both
/// [`reconstruct_topology`] and [`reconstruct_topology_stage4`] share ONE copy):
/// walk `infos`, emit `edges`/`faces`, and build the per-vertex
/// `TessellationSource` vector (relocated verts → `BRepEdge { edge, t }`).
///
/// The Newell / flip / E2 / E3 machinery is UNCHANGED from PR-YR8/YR9 (it reads
/// `cycles` / `signed_areas`, never the per-edge curve). The per-edge `curve`
/// comes from `intersection_curves` (an intersection edge gets its exact conic;
/// all others stay `LineSegment`).
fn emit_topology(
    mesh: &Mesh,
    infos: &[PatchInfo],
    intersection_curves: &std::collections::BTreeMap<(u32, u32), Curve>,
    relocations: &[(u32, f64)],
    op: BoolOp,
) -> Result<ReconstructedTopology, YangError> {
    // (1) Vertices: 1:1 with the (possibly relocated) mesh.verts.
    let vertices: Vec<BRepVertex> = mesh
        .verts
        .iter()
        .map(|&p| BRepVertex { point: p })
        .collect();

    let mut edges: Vec<BRepEdge> = Vec::new();
    let mut faces: Vec<BRepFace> = Vec::new();
    // PR-KV13 F2: per-output-face attribution, pushed in lockstep with `faces`.
    let mut face_attribution: Vec<TriangleAttribution> = Vec::new();
    // Spec yang_stage6_sliver_topology §4B: T-subdivide each loop edge at
    // foreign on-segment output vertices BEFORE emission, so a shared solid
    // edge subdivided differently on its two sides pairs segment-by-segment.
    // A strict no-op (byte-identical cycles) for output with no such vertices.
    let subdivided_cycles = subdivide_loops_at_shared_vertices(infos, mesh);
    for (info_index, info) in infos.iter().enumerate() {
        let cycles = &subdivided_cycles[info_index];
        let inherited = info.inherited;
        let face_idx = info.face_idx;
        let info_attr = TriangleAttribution {
            input: info.input,
            face: info.face_idx as u32,
        };

        // PR-YR8 (P2c) Blocker 2, spec §4: curved-surface branch BEFORE the
        // planar normal/Newell/flip machinery. A `Cylinder` patch is a barrel
        // and a `Sphere` patch is a cap (PR-YR15) — for either, a single plane
        // normal + signed-area classification is meaningless, so we DROP the
        // E3/`positive_count` check and the inherited-normal
        // flip. We INHERIT the surface UNCHANGED (the canonical params must stay
        // exact for downstream SSI / kernel-v2 — we never perturb them to signal
        // sense). Instead, cavity-sense is recorded out-of-band in
        // `BRepFace.reversed`, set from `op == Subtract && info.input == B` — the
        // same `flip_for_op` signal the mesh winding used, so face sense and mesh
        // winding are provably consistent (Union → no cavity → `reversed`
        // false). `patch_boundary_cycle` (called above) is surface-agnostic, so
        // we reuse `cycles`. We KEEP the E2 degenerate-loop guard.
        if matches!(
            inherited,
            Surface::Cylinder { .. }
                | Surface::Sphere { .. }
                | Surface::Cone { .. }
                | Surface::Torus { .. }
        ) {
            let push_loop = |edges: &mut Vec<BRepEdge>, cycle: &[(u32, u32)]| -> Vec<u32> {
                let start_idx = edges.len() as u32;
                for &(s, e) in cycle {
                    edges.push(BRepEdge {
                        start: s,
                        end: e,
                        curve: intersection_curves
                            .get(&if s < e { (s, e) } else { (e, s) })
                            .copied()
                            .unwrap_or(Curve::LineSegment),
                    });
                }
                (start_idx..edges.len() as u32).collect()
            };

            // E2 degenerate-loop guard: each cycle's Newell area-vector
            // magnitude must exceed MIN_FEATURE_SIZE² (A14.3 shared constant).
            for cycle in cycles {
                let mut nx = 0.0f64;
                let mut ny = 0.0f64;
                let mut nz = 0.0f64;
                let m = cycle.len();
                for i in 0..m {
                    let a_pt = mesh.verts[cycle[i].0 as usize].as_array();
                    let b_pt = mesh.verts[cycle[(i + 1) % m].0 as usize].as_array();
                    nx += a_pt[1] * b_pt[2] - a_pt[2] * b_pt[1];
                    ny += a_pt[2] * b_pt[0] - a_pt[0] * b_pt[2];
                    nz += a_pt[0] * b_pt[1] - a_pt[1] * b_pt[0];
                }
                let nrm_mag = (nx * nx + ny * ny + nz * nz).sqrt();
                if nrm_mag < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE {
                    return Err(non_manifold_at(
                        "s6-curved-degenerate-loop",
                        format_args!(
                            "face {face_idx} cycle len {} |N|={nrm_mag:.3e}",
                            cycle.len()
                        ),
                    ));
                }
            }

            // Empty-cycles guard (PR-CF1 case#23): a kept curved patch can come out with
            // no boundary cycle for the box-as-subtrahend direction (prim − box), which
            // is a DEFERRED, out-of-scope op direction (spec §2) — the reassembly leaves
            // the curved patch with no intersection-boundary loop even though the solid
            // result is non-empty. Such a patch cannot form a bounded face; refuse loudly,
            // mirroring the E2/E3 degenerate-reassembly guards. Without this, the
            // `cycles[outer_idx]` index below panics on the empty set.
            if cycles.is_empty() {
                return Err(non_manifold_at(
                    "s6-curved-empty-cycles",
                    format_args!("face {face_idx}"),
                ));
            }

            // Deterministic loop assignment: outer = the cycle with the MOST
            // edges; tie-break = lowest min start-vertex index within the
            // cycle. All other cycles = inner_loops.
            let cycle_min_vert = |c: &[(u32, u32)]| c.iter().map(|&(s, _)| s).min().unwrap_or(0);
            let mut outer_idx = 0usize;
            for i in 1..cycles.len() {
                let cur_len = cycles[i].len();
                let best_len = cycles[outer_idx].len();
                if cur_len > best_len
                    || (cur_len == best_len
                        && cycle_min_vert(&cycles[i]) < cycle_min_vert(&cycles[outer_idx]))
                {
                    outer_idx = i;
                }
            }

            let outer_loop = push_loop(&mut edges, &cycles[outer_idx]);
            let mut inner_loops: Vec<Vec<u32>> = Vec::new();
            for (i, cycle) in cycles.iter().enumerate() {
                if i != outer_idx {
                    inner_loops.push(push_loop(&mut edges, cycle));
                }
            }

            face_attribution.push(info_attr);
            faces.push(BRepFace {
                surface: inherited,
                outer_loop,
                inner_loops,
                // PR-KV6b-1: compose the input face's own cavity sense with
                // the Subtract-B flip (XOR). A no-op for every pre-KV6b
                // fixture (inputs always carried `reversed == false`).
                reversed: info.input_reversed
                    ^ (op == BoolOp::Subtract && info.input == InputId::B),
            });
            continue;
        }

        let (normal, d) = match inherited {
            Surface::Plane { normal, d } => (normal, d),
            // Cylinder, Sphere, Cone, and Torus are all handled by the curved
            // branch above (PR-YR17 added Cone; KV6d-5b2 added Torus), so these
            // arms are unreachable-defensive. Kept LOUD (P9) for any genuinely
            // unexpected surface.
            Surface::Sphere { .. }
            | Surface::Cylinder { .. }
            | Surface::Cone { .. }
            | Surface::Torus { .. } => {
                return Err(YangError::CurvedSurfaceNotYetSupported { face: face_idx });
            }
        };
        let n = normal.as_array();

        // Per-cycle Newell area-vector `N = Σ v_i × v_{i+1}` and its signed
        // area along the inherited face normal. The kept tris are outward-
        // oriented w.r.t. the RESULT solid, but for Subtract the B-surface
        // tris are flipped (`flip_for_op`) so a B-face patch winds OPPOSITE
        // its inherited normal. So we cannot assume the inherited normal
        // already agrees with the winding: instead, take the largest-area
        // cycle as the patch's outer boundary, let ITS winding define the
        // face's true outward normal (flip the inherited normal if the
        // winding opposes it — a subtracted B-face becomes a cavity wall
        // whose outward normal points into the cavity), then classify the
        // remaining cycles relative to that corrected orientation.
        let mut signed_areas: Vec<f64> = Vec::with_capacity(cycles.len());
        for cycle in cycles {
            let mut nx = 0.0f64;
            let mut ny = 0.0f64;
            let mut nz = 0.0f64;
            let m = cycle.len();
            for i in 0..m {
                let a_pt = mesh.verts[cycle[i].0 as usize].as_array();
                let b_pt = mesh.verts[cycle[(i + 1) % m].0 as usize].as_array();
                nx += a_pt[1] * b_pt[2] - a_pt[2] * b_pt[1];
                ny += a_pt[2] * b_pt[0] - a_pt[0] * b_pt[2];
                nz += a_pt[0] * b_pt[1] - a_pt[1] * b_pt[0];
            }
            // E2: degenerate loop — Newell area-vector magnitude below the
            // minimum feature area (MIN_FEATURE_SIZE²; A14.3 shared constant).
            let nrm_mag = (nx * nx + ny * ny + nz * nz).sqrt();
            if nrm_mag < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE {
                return Err(non_manifold_at(
                    "s6-planar-degenerate-loop",
                    format_args!(
                        "face {face_idx} cycle len {} |N|={nrm_mag:.3e}",
                        cycle.len()
                    ),
                ));
            }
            signed_areas.push(nx * n[0] + ny * n[1] + nz * n[2]);
        }

        // Empty-cycles guard (PR-CF1 defensive mirror of the curved branch):
        // a kept planar patch with no boundary cycle cannot form a bounded
        // face. Latent here (the all-planar fuzz never produces empty cycles)
        // but structurally identical to the curved-branch panic — the
        // `signed_areas[outer_idx]` / `cycles[outer_idx]` index below would
        // panic on the empty set. Mirrors the E2/E3 degenerate guards.
        if cycles.is_empty() {
            return Err(non_manifold_at(
                "s6-planar-empty-cycles",
                format_args!("face {face_idx}"),
            ));
        }

        // Outer boundary = the largest-|area| cycle. Its sign (relative to
        // the inherited normal) tells us whether the winding agrees with the
        // inherited normal; if not, flip the stored normal so the output
        // face's normal matches its outward winding.
        let mut outer_idx = 0usize;
        for (i, &s) in signed_areas.iter().enumerate() {
            if s.abs() > signed_areas[outer_idx].abs() {
                outer_idx = i;
            }
        }
        let flip = signed_areas[outer_idx] < 0.0;
        let surface = if flip {
            Surface::Plane {
                normal: Vector3::new(-n[0], -n[1], -n[2]),
                d: -d,
            }
        } else {
            inherited
        };
        // After any flip, the outer cycle's signed area is positive and the
        // holes are negative. E3: a connected outward-oriented patch has
        // EXACTLY one cycle whose corrected sign is positive (its outer
        // boundary). 0 or ≥2 ⇒ disconnected / nested, out of scope.
        let orient = if flip { -1.0 } else { 1.0 };
        let positive_count = signed_areas.iter().filter(|&&s| s * orient > 0.0).count();
        if positive_count != 1 {
            return Err(non_manifold_at(
                "s6-planar-positive-count",
                format_args!("face {face_idx} positive_count={positive_count}"),
            ));
        }
        let outer_cycle = &cycles[outer_idx];
        let inner_cycles: Vec<&Vec<(u32, u32)>> = cycles
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != outer_idx)
            .map(|(_, c)| c)
            .collect();

        // Emit the outer loop's edges first, then each inner loop's edges.
        let push_loop = |edges: &mut Vec<BRepEdge>, cycle: &[(u32, u32)]| -> Vec<u32> {
            let start_idx = edges.len() as u32;
            for &(s, e) in cycle {
                edges.push(BRepEdge {
                    start: s,
                    end: e,
                    curve: intersection_curves
                        .get(&if s < e { (s, e) } else { (e, s) })
                        .copied()
                        .unwrap_or(Curve::LineSegment),
                });
            }
            (start_idx..edges.len() as u32).collect()
        };

        let outer_loop = push_loop(&mut edges, outer_cycle);
        let mut inner_loops: Vec<Vec<u32>> = Vec::with_capacity(inner_cycles.len());
        for inner in &inner_cycles {
            inner_loops.push(push_loop(&mut edges, inner));
        }

        face_attribution.push(info_attr);
        faces.push(BRepFace {
            surface,
            outer_loop,
            inner_loops,
            reversed: false,
        });
    }

    // Tessellation sources (PR-YR10): default `BRepVertex(i)`; each relocated /
    // retagged intersection vertex overrides to `BRepEdge { edge, t }` where
    // `edge` is the FIRST output Circle edge incident to the vertex (the output
    // edges exist only after the emission pass above). The angle `t` is the
    // circle-frame parameter Stage 4 computed, so `eval_source` reproduces the
    // relocated position exactly.
    let mut sources: Vec<TessellationSource> = (0..mesh.num_verts() as u32)
        .map(TessellationSource::BRepVertex)
        .collect();
    for &(vid, t) in relocations {
        if (vid as usize) >= sources.len() {
            continue;
        }
        let edge_idx = edges.iter().position(|e| {
            matches!(
                e.curve,
                Curve::Circle { .. }
                    | Curve::Ellipse { .. }
                    | Curve::Parabola { .. }
                    | Curve::Hyperbola { .. }
            ) && (e.start == vid || e.end == vid)
        });
        if let Some(ei) = edge_idx {
            sources[vid as usize] = TessellationSource::BRepEdge { edge: ei as u32, t };
        }
    }

    Ok((vertices, edges, faces, sources, face_attribution))
}

/// PR-YR5 internal: grouped patch of same-attribution triangles.
struct Patch {
    attribution: TriangleAttribution,
    tri_indices: Vec<u32>,
}

/// PR-YR5 helper: per-triangle neighbor list via canonical-edge
/// BTreeMap (deterministic insertion + iteration order).
fn triangle_adjacency(mesh: &Mesh) -> Vec<Vec<u32>> {
    use std::collections::BTreeMap;
    let mut edge_to_tris: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for (t, tri) in mesh.tris.iter().enumerate() {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (va, vb) = (tri[i], tri[j]);
            let key = if va < vb { (va, vb) } else { (vb, va) };
            edge_to_tris.entry(key).or_default().push(t as u32);
        }
    }
    let mut neighbors: Vec<Vec<u32>> = vec![Vec::new(); mesh.tris.len()];
    for sharing in edge_to_tris.values() {
        for &t1 in sharing {
            for &t2 in sharing {
                if t1 != t2 && !neighbors[t1 as usize].contains(&t2) {
                    neighbors[t1 as usize].push(t2);
                }
            }
        }
    }
    neighbors
}

/// PR-YR5 helper: BFS flood-fill same-attribution triangles into
/// patches. Skip None-attributed triangles. Deterministic seed order:
/// lowest unvisited tri index first.
fn flood_fill_patches(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    adjacency: &[Vec<u32>],
) -> Vec<Patch> {
    use std::collections::VecDeque;
    let mut visited = vec![false; mesh.tris.len()];
    let mut patches: Vec<Patch> = Vec::new();
    for seed in 0..mesh.tris.len() as u32 {
        if visited[seed as usize] {
            continue;
        }
        let Some(seed_attr) = attribution.lookup(seed) else {
            visited[seed as usize] = true;
            continue;
        };
        let mut queue: VecDeque<u32> = VecDeque::from([seed]);
        let mut tri_indices: Vec<u32> = Vec::new();
        while let Some(t) = queue.pop_front() {
            if visited[t as usize] {
                continue;
            }
            let Some(t_attr) = attribution.lookup(t) else {
                continue;
            };
            if t_attr != seed_attr {
                continue;
            }
            visited[t as usize] = true;
            tri_indices.push(t);
            for &n in &adjacency[t as usize] {
                if !visited[n as usize] {
                    queue.push_back(n);
                }
            }
        }
        patches.push(Patch {
            attribution: seed_attr,
            tri_indices,
        });
    }
    patches
}

/// PR-YR5c helper: recover ALL directed boundary cycles of a patch.
/// Boundary edges = edges in exactly one patch triangle (canonical
/// (min, max) test). Walk each cycle from the lowest remaining
/// start-vertex; follow start→end chain via `BTreeMap` (deterministic).
///
/// A simple face yields 1 cycle; an annulus (holed face) yields 2 (the
/// outer boundary + one hole); etc. Classification of which cycle is
/// outer vs inner happens in `reconstruct_topology`.
///
/// Returns `Err(NonManifoldOutput)` on dead-end or T-junction (a genuine
/// non-manifold patch).
fn patch_boundary_cycle(patch: &Patch, mesh: &Mesh) -> Result<Vec<Vec<(u32, u32)>>, YangError> {
    use std::collections::{BTreeMap, HashSet};

    let patch_set: HashSet<u32> = patch.tri_indices.iter().copied().collect();

    // Precompute edge → tris-in-patch count for O(T) total cost.
    //
    // Spec yang_stage6_sliver_topology §4A (walk robustness): EXCLUDE the
    // patch's FOLD slivers (`patch_fold_slivers`) from BOTH the edge-count
    // preamble and the directed-boundary collection. A fold sliver is a
    // zero-area triangle whose sign-of-zero winding duplicates a real
    // triangle's directed edge (the measured F0016 chord fold); its spurious
    // directed edges would unbalance the walk into a false `NonManifoldOutput`.
    // The fold slivers keep their attribution and stay in the output mesh; they
    // simply carry no boundary of their own. NON-fold degenerate slivers (e.g.
    // a femto-twin membrane welding two coincident vertices, whose edges all
    // pair anti-parallel with real neighbours) are KEPT — excluding them would
    // promote a legitimately-interior real edge to a false boundary and diverge
    // from the reference arrangement. A patch that is ALL fold slivers derives
    // an empty boundary here → the caller's empty-cycles guard raises the loud
    // `NonManifoldOutput` (S5), never a silent degenerate face.
    let excluded_slivers = patch_fold_slivers(patch, mesh);
    let mut patch_edge_count: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for &t in &patch.tri_indices {
        if excluded_slivers.contains(&t) {
            continue;
        }
        let tri = &mesh.tris[t as usize];
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (va, vb) = (tri[i], tri[j]);
            let key = if va < vb { (va, vb) } else { (vb, va) };
            *patch_edge_count.entry(key).or_insert(0) += 1;
        }
    }

    // Collect directed boundary edges in triangle CCW order (fold slivers
    // excluded — see §4A note above).
    let mut directed_boundary: Vec<(u32, u32)> = Vec::new();
    for &t in &patch.tri_indices {
        if excluded_slivers.contains(&t) {
            continue;
        }
        let tri = &mesh.tris[t as usize];
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (va, vb) = (tri[i], tri[j]);
            let key = if va < vb { (va, vb) } else { (vb, va) };
            if patch_edge_count.get(&key).copied().unwrap_or(0) == 1 {
                directed_boundary.push((va, vb));
            }
        }
    }
    let _ = patch_set; // patch_set was kept for readability; not needed after precompute

    if directed_boundary.is_empty() {
        return Ok(Vec::new());
    }

    // Build start → ends adjacency (sorted ascending for determinism)
    let mut by_start: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for &(s, e) in &directed_boundary {
        by_start.entry(s).or_default().push(e);
    }
    for ends in by_start.values_mut() {
        ends.sort_unstable();
    }

    // Track how many boundary edges remain unconsumed across all cycles, to
    // bound a single cycle walk (per-cycle "loop escaped" safety guard).
    let mut remaining = directed_boundary.len();
    let mut cycles: Vec<Vec<(u32, u32)>> = Vec::new();

    // Extract every cycle: while any start vertex still has an outgoing edge,
    // begin a new cycle at the LOWEST such start vertex and walk it with the
    // per-cycle start→end chain logic (consuming edges as we go).
    while let Some((&start, _)) = by_start.iter().find(|(_, ends)| !ends.is_empty()) {
        // `start` is the lowest start vertex whose end-list is still non-empty.
        // Edges available when this cycle starts: it cannot exceed this.
        let budget = remaining;
        let mut current = start;
        let mut cycle: Vec<(u32, u32)> = Vec::new();
        loop {
            let next = {
                let next_vec = by_start.get_mut(&current).ok_or_else(|| {
                    non_manifold_at(
                        "s6-boundary-walk-no-start",
                        format_args!("vertex {current}"),
                    )
                })?;
                if next_vec.is_empty() {
                    // Dead-end / T-junction: a genuine non-manifold patch.
                    return Err(non_manifold_at(
                        "s6-boundary-walk-deadend",
                        format_args!("vertex {current} cycle so far {}", cycle.len()),
                    ));
                }
                next_vec.remove(0)
            };
            cycle.push((current, next));
            remaining -= 1;
            current = next;
            if current == start {
                break;
            }
            // Per-cycle safety: a single cycle cannot be longer than the
            // edges that remained when it started (else the walk escaped).
            if cycle.len() > budget {
                return Err(non_manifold_at(
                    "s6-boundary-walk-escaped",
                    format_args!("start {start} budget {budget}"),
                ));
            }
        }
        cycles.push(cycle);
    }

    Ok(cycles)
}

/// A mesh triangle is a degenerate (zero-area) sliver when twice its area
/// `‖(p1−p0)×(p2−p0)‖` falls below `MIN_FEATURE_SIZE²` — the SAME shared
/// threshold the Stage-6 attribution degenerate branch uses (governance A14.3,
/// no ad-hoc epsilon). The exact arrangement keeps such slivers along shared
/// collinear solid edges for watertightness; spec `yang_stage6_sliver_topology`
/// §4A excludes them from boundary derivation.
fn triangle_is_degenerate(mesh: &Mesh, t: u32) -> bool {
    let tri = &mesh.tris[t as usize];
    let p0 = mesh.verts[tri[0] as usize].as_array();
    let p1 = mesh.verts[tri[1] as usize].as_array();
    let p2 = mesh.verts[tri[2] as usize].as_array();
    let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    let cross = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    let twice_area = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
    twice_area < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE
}

/// The patch's FOLD slivers — the degenerate zero-area triangles that spec
/// `yang_stage6_sliver_topology` §4A excludes from boundary derivation.
///
/// A fold sliver has at least one DIRECTED edge `(a→b)` that coincides,
/// SAME-direction, with a directed edge of another triangle in the patch
/// (directed multiplicity ≥ 2). That is the measured F0016 signature: a
/// zero-area shim whose sign-of-zero winding duplicates the real triangle's
/// chord edge, unbalancing the boundary walk into a false `NonManifoldOutput`.
///
/// A degenerate sliver whose edges instead pair ANTI-parallel with their
/// neighbours — e.g. a femto-twin membrane welding two coincident vertices,
/// where every directed edge is unique — is NOT a fold. Such a sliver carries a
/// legitimate (if zero-length) boundary; excluding it would drop a real
/// neighbour edge from interior to boundary and diverge from the reference
/// arrangement (curved / twin parity). So it is KEPT.
fn patch_fold_slivers(patch: &Patch, mesh: &Mesh) -> std::collections::HashSet<u32> {
    use std::collections::{HashMap, HashSet};
    // Directed edge multiplicity over ALL patch triangles (real + degenerate).
    let mut dir_count: HashMap<(u32, u32), u32> = HashMap::new();
    for &t in &patch.tri_indices {
        let tri = &mesh.tris[t as usize];
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            *dir_count.entry((tri[i], tri[j])).or_insert(0) += 1;
        }
    }
    let mut folds: HashSet<u32> = HashSet::new();
    for &t in &patch.tri_indices {
        if !triangle_is_degenerate(mesh, t) {
            continue;
        }
        let tri = &mesh.tris[t as usize];
        let is_fold = [(0usize, 1usize), (1, 2), (2, 0)]
            .iter()
            .any(|&(i, j)| dir_count.get(&(tri[i], tri[j])).copied().unwrap_or(0) >= 2);
        if is_fold {
            folds.insert(t);
        }
    }
    folds
}

/// Exact 3D on-open-segment test for the Stage-6 loop T-subdivision (spec
/// `yang_stage6_sliver_topology` §4B). Returns `Some(t_num)` — the exact
/// numerator of the segment parameter `t = ((v−a)·(b−a)) / |b−a|²` — when `v`
/// lies STRICTLY between `a` and `b` (exactly collinear AND `0 < t < 1`);
/// `None` otherwise. All candidate vertices on one edge share the `|b−a|²`
/// denominator, so `t_num` alone orders them. Pure rational (`f64 → RBig` is
/// lossless); no tolerance — a vertex only ULP-near the segment does NOT split
/// it (that residue class stays loud downstream, spec §5 S6).
fn on_open_segment_param(a: [f64; 3], b: [f64; 3], v: [f64; 3]) -> Option<dashu::rational::RBig> {
    use crate::coplanar_overlay::rat;
    use dashu::rational::RBig;
    let r = |x: f64| rat(x).ok();
    let (ax, ay, az) = (r(a[0])?, r(a[1])?, r(a[2])?);
    let (bx, by, bz) = (r(b[0])?, r(b[1])?, r(b[2])?);
    let (vx, vy, vz) = (r(v[0])?, r(v[1])?, r(v[2])?);
    let (abx, aby, abz) = (&bx - &ax, &by - &ay, &bz - &az);
    let (dax, day, daz) = (&vx - &ax, &vy - &ay, &vz - &az);
    // Exactly collinear: cross(ab, da) == 0 in all three components.
    let cx = &aby * &daz - &abz * &day;
    let cy = &abz * &dax - &abx * &daz;
    let cz = &abx * &day - &aby * &dax;
    if cx != RBig::ZERO || cy != RBig::ZERO || cz != RBig::ZERO {
        return None;
    }
    let t_num = &dax * &abx + &day * &aby + &daz * &abz;
    let len2 = &abx * &abx + &aby * &aby + &abz * &abz;
    // Strict betweenness: 0 < t_num < len2 (len2 > 0 for a real edge; a
    // degenerate zero-length edge yields len2 == 0 and no split).
    if t_num > RBig::ZERO && t_num < len2 {
        Some(t_num)
    } else {
        None
    }
}

/// Stage-6 loop T-subdivision (spec `yang_stage6_sliver_topology` §4B). After
/// §4A excludes degenerate slivers from boundary derivation, a face whose patch
/// carried a whole shared solid edge as ONE chord `(a,b)` must split that chord
/// at every output vertex that (i) lies STRICTLY on segment `a–b` (exact
/// rational collinearity + betweenness) AND (ii) is used by SOME OTHER output
/// loop. The other side of the shared edge subdivides it at those same
/// vertices, so after the split every segment of the solid edge is used by
/// exactly two directed loop edges — the 2-manifold seam kernel-v2's
/// edge-pairing check demands. Self-pairs within one weakly-simple loop are
/// legitimate (matching the existing kernel-v2 self-pair handling).
///
/// Returns per-`info` subdivided cycles (same outer shape as `info.cycles`).
/// Determinism: split vertices ordered by exact segment parameter, ties by
/// vertex index. A no-op (byte-identical cycles) when no loop edge has an
/// on-segment foreign vertex (spec §5 S3), so non-sliver output is unaffected.
fn subdivide_loops_at_shared_vertices(
    infos: &[PatchInfo],
    mesh: &Mesh,
) -> Vec<Vec<Vec<(u32, u32)>>> {
    use dashu::rational::RBig;
    use std::collections::BTreeMap;

    // (1) Assign a global loop id to every (info, cycle); record which loop ids
    // use each vertex. A vertex used repeatedly within ONE loop counts that
    // loop once (dedup) so the "used by some OTHER loop" test is exact.
    let mut next_loop = 0usize;
    let mut cycle_loop_ids: Vec<Vec<usize>> = Vec::with_capacity(infos.len());
    let mut vertex_loops: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for info in infos {
        let mut ids = Vec::with_capacity(info.cycles.len());
        for cycle in &info.cycles {
            for &(s, _e) in cycle {
                vertex_loops.entry(s).or_default().push(next_loop);
            }
            ids.push(next_loop);
            next_loop += 1;
        }
        cycle_loop_ids.push(ids);
    }
    for ids in vertex_loops.values_mut() {
        ids.sort_unstable();
        ids.dedup();
    }

    // (2) Split each loop edge at its foreign on-segment vertices — but ONLY
    // for patches that had a degenerate sliver excluded (spec §2/§4B). A patch
    // with no excluded sliver keeps byte-identical loops: the measured
    // un-subdivided chord lives on the sliver-bearing side, and subdividing a
    // benign T-junction on a clean curved/planar patch would diverge from the
    // C++ reference arrangement (which does not carry that vertex on the edge),
    // breaking reference parity. The vertex-loop map above still spans ALL
    // loops, so the sliver side splits at the foreign (fine-side) vertices.
    let mut out: Vec<Vec<Vec<(u32, u32)>>> = Vec::with_capacity(infos.len());
    for (ii, info) in infos.iter().enumerate() {
        if !info.had_fold_sliver {
            out.push(info.cycles.clone());
            continue;
        }
        let mut info_cycles: Vec<Vec<(u32, u32)>> = Vec::with_capacity(info.cycles.len());
        for (ci, cycle) in info.cycles.iter().enumerate() {
            let lid = cycle_loop_ids[ii][ci];
            let mut new_cycle: Vec<(u32, u32)> = Vec::with_capacity(cycle.len());
            for &(s, e) in cycle {
                let pa = mesh.verts[s as usize].as_array();
                let pb = mesh.verts[e as usize].as_array();
                let mut splits: Vec<(RBig, u32)> = Vec::new();
                for (&v, luse) in &vertex_loops {
                    if v == s || v == e {
                        continue;
                    }
                    // Must be used by a loop OTHER than this one.
                    if !luse.iter().any(|&l| l != lid) {
                        continue;
                    }
                    let pv = mesh.verts[v as usize].as_array();
                    if let Some(t_num) = on_open_segment_param(pa, pb, pv) {
                        splits.push((t_num, v));
                    }
                }
                splits.sort_by(|x, y| x.0.cmp(&y.0).then(x.1.cmp(&y.1)));
                let mut prev = s;
                for (_, v) in splits {
                    new_cycle.push((prev, v));
                    prev = v;
                }
                new_cycle.push((prev, e));
            }
            info_cycles.push(new_cycle);
        }
        out.push(info_cycles);
    }
    out
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── Case-IV phantom guard (spec `yang_case_iv_phantom_guard`) ────────

    /// Minimal solid cylinder B-Rep (two rims + seam) for the guard tests.
    fn guard_cyl(cx: f64, cy: f64, r: f64, h: f64) -> BRep {
        let verts = vec![
            BRepVertex {
                point: Point3::new(cx + r, cy, 0.0),
            },
            BRepVertex {
                point: Point3::new(cx + r, cy, h),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(cx, cy, 0.0),
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(cx, cy, h),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![
            BRepFace {
                surface: Surface::Cylinder {
                    axis_point: Point3::new(cx, cy, 0.0),
                    axis_dir: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
                outer_loop: vec![0, 2, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    d: 0.0,
                },
                outer_loop: vec![0],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    d: -h,
                },
                outer_loop: vec![1],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        BRep::new(verts, edges, faces).expect("guard cylinder")
    }

    /// The measured F0088 pair: a nested-disjoint tool inside the plate
    /// cylinder with gap 0.0115 < the natural N=14 sagitta — the guard must
    /// demand a finer N (34 at these radii: the smallest N with
    /// sag(R,N)+sag(r,N) ≤ gap/2).
    #[test]
    fn phantom_guard_nested_disjoint_demands_finer_n() {
        let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
        let tool = guard_cyl(1.2243, 0.0, 0.042871795720997065, 0.23);
        let n = phantom_min_rim_segments(&plate, &tool).expect("guard must fire");
        let gap = 1.2787008340600021 - 1.2243 - 0.042871795720997065;
        let sag = |r: f64, n: usize| r * (1.0 - (std::f64::consts::PI / n as f64).cos());
        assert!(
            sag(1.2787008340600021, n) + sag(0.042871795720997065, n) <= gap / 2.0,
            "derived N={n} must clear the analytic gap with the factor-2 margin"
        );
        assert!(
            sag(1.2787008340600021, n - 1) + sag(0.042871795720997065, n - 1) > gap / 2.0,
            "derived N={n} must be MINIMAL (no over-refinement)"
        );
    }

    /// A crossing pair (the tool overlaps the plate wall) has no analytic
    /// gap — a real intersection curve exists and SSI refines it. No boost.
    #[test]
    fn phantom_guard_crossing_pair_is_silent() {
        let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
        let tool = guard_cyl(1.26, 0.0, 0.042871795720997065, 0.23);
        assert_eq!(phantom_min_rim_segments(&plate, &tool), None);
    }

    /// A far-disjoint pair derives a tiny N that both solids' natural
    /// Stage-1 N already satisfies — the self-limiting gate drops it.
    #[test]
    fn phantom_guard_far_pair_is_silent() {
        let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
        let tool = guard_cyl(0.3, 0.1, 0.042871795720997065, 0.23);
        assert_eq!(phantom_min_rim_segments(&plate, &tool), None);
    }

    /// Build one B-Rep carrying TWO cylinders (a plate wall + a hole at
    /// `(hx, hy)` with radius `hr`).
    fn two_cyl_brep(hx: f64, hy: f64, hr: f64) -> BRep {
        let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
        let tool = guard_cyl(hx, hy, hr, 0.23);
        let mut verts = plate.vertices.clone();
        let mut edges = plate.edges.clone();
        let mut faces = plate.faces.clone();
        let (vo, eo) = (verts.len() as u32, edges.len() as u32);
        verts.extend(tool.vertices.iter().cloned());
        for e in &tool.edges {
            edges.push(BRepEdge {
                start: e.start + vo,
                end: e.end + vo,
                curve: e.curve,
            });
        }
        for f in &tool.faces {
            faces.push(BRepFace {
                surface: f.surface,
                outer_loop: f.outer_loop.iter().map(|&e| e + eo).collect(),
                inner_loops: Vec::new(),
                reversed: f.reversed,
            });
        }
        BRep::new(verts, edges, faces).expect("combined solid")
    }

    /// INTRA-solid pair (the chained F0088 output: hole 4's lateral 0.0115
    /// from the plate wall inside ONE solid): STAGE 1's own N selection must
    /// fold the pair's derived N in — otherwise ANY tessellation of the
    /// solid (input conversion included) puts the cap's outer-rim chords
    /// across the hole rim and the planar CDT gets crossing constraints
    /// (measured corpus F0088 ops 7/15, `CDT triangulation failed`). The
    /// near-rim solid must tessellate strictly denser than the same solid
    /// with its hole far from the wall.
    #[test]
    fn stage1_intra_solid_phantom_fold_densifies_rims() {
        let near = two_cyl_brep(1.2243, 0.0, 0.042871795720997065);
        let far = two_cyl_brep(0.3, 0.1, 0.042871795720997065);
        assert!(
            near.as_mesh().num_verts() > far.as_mesh().num_verts(),
            "near-rim solid must tessellate denser (near {} verts vs far {})",
            near.as_mesh().num_verts(),
            far.as_mesh().num_verts()
        );
        // And the cross-pair guard is silent for it — the intra fold lives
        // in Stage 1, not in the pair analysis.
        let partner = guard_cyl(10.0, 10.0, 0.1, 0.23);
        assert_eq!(phantom_min_rim_segments(&near, &partner), None);
    }

    /// An operand without B-Rep faces (the `from_mesh` chained-output
    /// degenerate) has no cylinder faces to scan — byte-identical path.
    #[test]
    fn phantom_guard_faceless_operand_is_silent() {
        let plate = guard_cyl(0.0, 0.0, 1.2787008340600021, 0.23);
        let raw = BRep::from_mesh(plate.as_mesh().clone());
        assert_eq!(phantom_min_rim_segments(&plate, &raw), None);
        assert_eq!(phantom_min_rim_segments(&raw, &plate), None);
    }

    // R0072: position tie-break for near-coincident PARALLEL line candidates
    // (`select_disjoint_parallel_line`). Mirrors the instrumented R0072 edge
    // (2,143): two parallel generators whose endpoint-distance intervals are
    // disjoint → the lower (nearer) one is selected. The numbers are the live
    // probe values (cand0 ≈ 2.0e-5, cand1 ≈ 3.3e-5).
    #[test]
    fn r0072_parallel_line_position_tiebreak() {
        let dir = Vector3::new(
            0.539_214_627_766_961_7,
            -0.348_918_218_865_836_5,
            -0.766_487_874_493_543,
        );
        // Two parallel lines offset along a perpendicular `n̂` (⟂ dir), 2e-5 and
        // 3.3e-5 from the edge endpoints which sit on the origin segment.
        let n = {
            // any unit vector ⟂ dir
            let d = normalize3(dir.as_array());
            let t = [1.0, 0.0, 0.0];
            let dot = t[0] * d[0] + t[1] * d[1] + t[2] * d[2];
            let p = [t[0] - dot * d[0], t[1] - dot * d[1], t[2] - dot * d[2]];
            normalize3(p)
        };
        let line_at = |off: f64| (Point3::new(off * n[0], off * n[1], off * n[2]), dir);
        let cand0 = line_at(2.0e-5);
        let cand1 = line_at(3.3e-5);
        let p_s = Point3::new(0.0, 0.0, 0.0);
        let p_e = Point3::new(
            d_scale(dir, 1e-4)[0],
            d_scale(dir, 1e-4)[1],
            d_scale(dir, 1e-4)[2],
        );

        // Disjoint intervals → the nearer line (index 0) wins regardless of order.
        assert_eq!(
            select_disjoint_parallel_line(&[cand0, cand1], p_s, p_e),
            Some(0)
        );
        assert_eq!(
            select_disjoint_parallel_line(&[cand1, cand0], p_s, p_e),
            Some(1)
        );

        // OVERLAPPING intervals (generators merged below resolution) → no clear
        // winner → None (the caller keeps its loud `AmbiguousCurve`). Put the two
        // lines symmetrically about the segment so each endpoint is equidistant.
        let near_a = line_at(2.0e-5);
        let near_b = line_at(-2.0e-5);
        assert_eq!(
            select_disjoint_parallel_line(&[near_a, near_b], p_s, p_e),
            None
        );

        // NON-parallel candidates → None (the tangent discriminator's job).
        let crossing = (Point3::new(0.0, 0.0, 0.0), Vector3::new(n[0], n[1], n[2]));
        assert_eq!(
            select_disjoint_parallel_line(&[cand0, crossing], p_s, p_e),
            None
        );

        // Fewer than two candidates → None.
        assert_eq!(select_disjoint_parallel_line(&[cand0], p_s, p_e), None);
    }

    fn d_scale(v: Vector3, s: f64) -> [f64; 3] {
        let d = normalize3(v.as_array());
        [d[0] * s, d[1] * s, d[2] * s]
    }

    // PR-YR10 N3 regression (Yang §4.5.3): a U-turn at p_r — consecutive points
    // double back so v1 ≈ −v2 ⇒ |t̃| ≈ 0 — IS a reversal. The paper places the
    // collinear/degenerate-t̃ case WITHIN the reversal subset ("directly detect
    // the reversal, avoiding the angle comparisons"). p_b=(0,0,0) → p_r=(1,0,0)
    // → p_n=(0.5,0,0) reverses direction (v1=+x, v2=−x, t̃=0). The degenerate
    // branch must report a reversal. (Was the N3 logic inversion: returned
    // `false` = "healthy", silently failing to correct the very reversal §4.5.3
    // exists for; reachable whenever relocation produces an out-of-order point.)

    // PR-6 (coincident-cylinder rim conformal weld). Locks the two invariants
    // that make the curved-input rim weld a conformal exact-identity merge of
    // redundant reconstructions — NOT a tolerance bucket that could mask
    // unpaired edges (the reverted F0057 hazard):
    //   (1) two sub-ULP rim duplicates of one analytic point are BOTH on the
    //       cylinder (within the analytic band) AND within the cluster band,
    //       so they fuse;
    //   (2) two GENUINELY distinct rim points (≥ MIN_FEATURE_SIZE apart, here
    //       the ~1e-4 chord spacing) are on the cylinder but FAR outside the
    //       cluster band, so they never fuse.
    #[test]
    fn pr6_rim_weld_fuses_only_sub_ulp_duplicates() {
        let cyl = stage0::PairCylinder {
            axis_point: [0.0, 0.0, 0.0],
            axis_dir: [0.0, 0.0, 1.0],
            radius: 1.0,
            band: 1e-7,
            opposite: true,
        };
        let base = [1.0, 0.0, 0.3];
        // (1) A sub-ULP duplicate: perturb the in-plane coord by ~2 ULPs.
        let twin = [1.0 + 2.0 * f64::EPSILON, 0.0, 0.3];
        let scale = base
            .iter()
            .chain(twin.iter())
            .fold(0.0f64, |m, &c| m.max(c.abs()));
        let cluster_band = cad_primitives::TAU_WORK * (1.0 + scale);
        assert!(
            centroid_on_cylinder(base, &cyl) <= cyl.band,
            "base rim point must be on the cylinder"
        );
        assert!(
            centroid_on_cylinder(twin, &cyl) <= cyl.band,
            "sub-ULP twin must still be on the cylinder"
        );
        assert!(
            (0..3).all(|k| (base[k] - twin[k]).abs() <= cluster_band),
            "sub-ULP twin must be within the cluster band ⇒ fuses"
        );
        // (2) A genuinely distinct rim point ~1e-4 away along the rim: on the
        // cylinder, but FAR outside the cluster band ⇒ never fused.
        let theta = 1e-4_f64;
        let distinct = [theta.cos(), theta.sin(), 0.3];
        assert!(
            centroid_on_cylinder(distinct, &cyl) <= cyl.band,
            "the distinct rim point is also exactly on the cylinder"
        );
        assert!(
            (0..3).any(|k| (base[k] - distinct[k]).abs() > cluster_band),
            "a genuinely distinct rim point (≥ chord spacing) must lie OUTSIDE \
             the cluster band so the conformal weld never fuses it (no \
             tolerance-bucket masking)"
        );
    }

    // Spec `yang_453_junction_protected_collapse` §3: the §4.5.3 collapse
    // victim is `p_n` on a same-curve run, but `p_r` when `p_n` is a curve
    // junction (the loop's curve changes at `p_n`).
    #[test]
    fn s453_collapse_removes_p_n_on_same_curve_run() {
        let circle = Curve::Circle {
            center: p(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let mut curves: std::collections::BTreeMap<(u32, u32), Curve> =
            std::collections::BTreeMap::new();
        curves.insert((1, 2), circle);
        curves.insert((2, 3), circle);
        assert_eq!(
            reversal_collapse_direction(&curves, 1, 2, 3),
            (2, 1),
            "same curve beyond p_n ⇒ paper default: p_n is the victim"
        );
    }

    #[test]
    fn s453_collapse_protects_junction_p_n() {
        let circle = Curve::Circle {
            center: p(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let other = Curve::Circle {
            center: p(5.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 2.0,
        };
        let mut curves: std::collections::BTreeMap<(u32, u32), Curve> =
            std::collections::BTreeMap::new();
        curves.insert((1, 2), circle);
        curves.insert((2, 3), other);
        assert_eq!(
            reversal_collapse_direction(&curves, 1, 2, 3),
            (1, 2),
            "curve changes at p_n ⇒ p_n is an exact curve-junction endpoint \
             and must survive; the overshooting p_r is the victim"
        );
        // Canonical-key robustness: descending vertex ids on both edges.
        let mut curves_rev: std::collections::BTreeMap<(u32, u32), Curve> =
            std::collections::BTreeMap::new();
        curves_rev.insert((7, 9), circle);
        curves_rev.insert((3, 7), other);
        assert_eq!(
            reversal_collapse_direction(&curves_rev, 9, 7, 3),
            (9, 7),
            "junction protection must hold under canonical (min,max) edge keys"
        );
    }

    // Spec §3b: §4.4.1(b) merge survivor ranking — junction > conic endpoint
    // > plain vertex; equal ranks keep the lower-index rule.
    #[test]
    fn s453_merge_survivor_prefers_exact_vertex() {
        use std::collections::BTreeSet;
        let junction: BTreeSet<u32> = [15u32].into_iter().collect();
        let conic: BTreeSet<u32> = [15u32, 20u32].into_iter().collect();

        // Conic endpoint (higher index) survives over a plain vertex — the
        // R0091 configuration, in BOTH argument orders.
        assert_eq!(
            sub_feature_merge_direction(&junction, &conic, 8, 20),
            (8, 20)
        );
        assert_eq!(
            sub_feature_merge_direction(&junction, &conic, 20, 8),
            (8, 20)
        );

        // Junction survives over a plain single-curve conic endpoint.
        assert_eq!(
            sub_feature_merge_direction(&junction, &conic, 20, 15),
            (20, 15)
        );
        assert_eq!(
            sub_feature_merge_direction(&junction, &conic, 15, 20),
            (20, 15)
        );

        // Equal rank (both plain): lower index survives — byte-identical to
        // the pre-fix behavior.
        assert_eq!(sub_feature_merge_direction(&junction, &conic, 4, 9), (9, 4));
        assert_eq!(sub_feature_merge_direction(&junction, &conic, 9, 4), (9, 4));
    }

    #[test]
    fn n3_degenerate_tangent_is_reversal() {
        let mesh = Mesh::new(
            vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.5, 0.0, 0.0)],
            vec![],
        );
        let curves: std::collections::BTreeMap<(u32, u32), Curve> =
            std::collections::BTreeMap::new();
        let lo = std::f64::consts::FRAC_PI_4;
        let hi = 3.0 * std::f64::consts::FRAC_PI_4;
        assert!(
            is_reversed(&mesh, &curves, 0, 1, 2, lo, hi),
            "a 180° U-turn (degenerate t̃, Yang §4.5.3 collinear case) must be \
             detected as a reversal, not treated as healthy"
        );
    }

    // =====================================================================
    // M4 — demoted substitutes (test-only differential oracle).
    //
    // These were the production PR-YR3/YR4 spatial-match + majority-vote
    // attribution path. M3 replaced production attribution with real
    // LabeledArrangement labels; per roadmap rule #9 the substitutes are
    // RETAINED here as a second independent attribution method that
    // cross-checks the true-label path (the `m4_*` differential test).
    // Disagreement on a fixture localizes a label-path bug. Do NOT delete.
    // =====================================================================

    /// M4 oracle: try to match `target` against a vertex in `brep`'s mesh
    /// within `MATCH_TOLERANCE`. Returns the matched vertex's
    /// `TessellationSource` or `None`.
    fn match_against(brep: &BRep, target: Point3) -> Option<TessellationSource> {
        let tol2 = MATCH_TOLERANCE * MATCH_TOLERANCE;
        for (i, v) in brep.as_mesh().verts.iter().enumerate() {
            let dx = v.x() - target.x();
            let dy = v.y() - target.y();
            let dz = v.z() - target.z();
            if dx * dx + dy * dy + dz * dz <= tol2 {
                return Some(brep.tessellation_map().lookup(i as u32));
            }
        }
        None
    }

    /// M4 oracle: match `target` against A first, then B; track which
    /// input matched.
    fn match_with_input(
        a: &BRep,
        b: &BRep,
        target: Point3,
    ) -> (Option<InputId>, TessellationSource) {
        if let Some(src) = match_against(a, target) {
            return (Some(InputId::A), src);
        }
        if let Some(src) = match_against(b, target) {
            return (Some(InputId::B), src);
        }
        (None, TessellationSource::Intersection)
    }

    /// M4 oracle: the set of `(InputId, face_idx)` pairs that a single
    /// output vertex's provenance is compatible with.
    fn face_candidates(
        input: Option<InputId>,
        source: TessellationSource,
        a: &BRep,
        b: &BRep,
    ) -> Vec<(InputId, u32)> {
        let Some(input) = input else {
            return Vec::new();
        };
        let brep = match input {
            InputId::A => a,
            InputId::B => b,
        };
        match source {
            TessellationSource::BRepFace { face, .. } => vec![(input, face)],
            TessellationSource::BRepEdge { edge, .. } => brep
                .faces()
                .iter()
                .enumerate()
                .filter(|(_, f)| f.outer_loop.contains(&edge))
                .map(|(i, _)| (input, i as u32))
                .collect(),
            TessellationSource::BRepVertex(v) => brep
                .faces()
                .iter()
                .enumerate()
                .filter(|(_, f)| {
                    f.outer_loop.iter().any(|&e| {
                        let edge = &brep.edges()[e as usize];
                        edge.start == v || edge.end == v
                    })
                })
                .map(|(i, _)| (input, i as u32))
                .collect(),
            TessellationSource::Intersection | TessellationSource::Unknown => Vec::new(),
        }
    }

    /// M4 oracle: count votes per `(InputId, face)` across 3 candidate
    /// sets; return the highest-count pair reaching ≥2 votes (ties → lowest
    /// `(InputId, face)` lexicographic).
    fn majority_vote(sets: &[Vec<(InputId, u32)>; 3]) -> Option<TriangleAttribution> {
        use std::collections::BTreeMap;
        let mut counts: BTreeMap<(InputId, u32), u8> = BTreeMap::new();
        for set in sets {
            let mut uniq: Vec<(InputId, u32)> = set.clone();
            uniq.sort();
            uniq.dedup();
            for c in uniq {
                *counts.entry(c).or_insert(0) += 1;
            }
        }
        let mut best: Option<((InputId, u32), u8)> = None;
        for (key, &count) in &counts {
            if count < 2 {
                continue;
            }
            match best {
                None => best = Some((*key, count)),
                Some((_, bc)) if count > bc => best = Some((*key, count)),
                _ => {}
            }
        }
        best.map(|((input, face), _)| TriangleAttribution { input, face })
    }

    /// M4 oracle composite: run the full demoted substitute attribution
    /// (vertex provenance → per-vertex face candidates → majority vote)
    /// over `mesh`, producing a `TriangleAttributionMap`. This is exactly
    /// what the pre-M3 production `boolean()` computed internally; the
    /// reworked PR-YR4 substitute tests and the yr5_* reconstruction tests
    /// call it directly instead of routing through production `boolean()`
    /// (whose attribution is now the real-label path).
    fn substitute_attribution(mesh: &Mesh, a: &BRep, b: &BRep) -> TriangleAttributionMap {
        let mut inputs: Vec<Option<InputId>> = Vec::with_capacity(mesh.num_verts());
        let mut sources: Vec<TessellationSource> = Vec::with_capacity(mesh.num_verts());
        for &target in &mesh.verts {
            let (inp, src) = match_with_input(a, b, target);
            inputs.push(inp);
            sources.push(src);
        }
        let mut attributions = Vec::with_capacity(mesh.num_tris());
        for tri in &mesh.tris {
            let sets = [
                face_candidates(inputs[tri[0] as usize], sources[tri[0] as usize], a, b),
                face_candidates(inputs[tri[1] as usize], sources[tri[1] as usize], a, b),
                face_candidates(inputs[tri[2] as usize], sources[tri[2] as usize], a, b),
            ];
            attributions.push(majority_vote(&sets));
        }
        TriangleAttributionMap { attributions }
    }

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    /// An empty (0-triangle) `LabeledArrangement` for backend-dispatch
    /// tests that only care about the Ok/err control flow, not labels.
    fn empty_arrangement() -> LabeledArrangement {
        LabeledArrangement {
            mesh: Mesh::empty(),
            surface: Vec::new(),
            inside: Vec::new(),
            patch: Vec::new(),
            source: Vec::new(),
            num_inputs: 2,
        }
    }

    fn sample_mesh() -> Mesh {
        Mesh::new(
            vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
            vec![[0, 1, 2]],
        )
    }

    /// Backend whose `boolean()` always errors and which does NOT override
    /// the M3 `labeled_arrangement` trait method, so it surfaces through
    /// the default ("not supported") error. Used by
    /// `boolean_with_err_backend` to confirm `boolean()` maps a backend
    /// failure to `YangError::MeshBooleanFailed`.
    struct MockBackend;
    impl MeshBoolean for MockBackend {
        fn boolean(
            &self,
            _a: &Mesh,
            _b: &Mesh,
            _op: BoolOp,
        ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
            Err(Box::from("mock failure"))
        }
    }

    // ----- Group 2: yang-rs type construction -----

    #[test]
    fn surface_plane_construction() {
        let s = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -1.0,
        };
        match s {
            Surface::Plane { normal, d } => {
                assert_eq!(normal, Vector3::new(0.0, 0.0, 1.0));
                assert_eq!(d, -1.0);
            }
            // `s` is constructed as `Plane`, so this arm is never hit; it
            // only satisfies exhaustiveness once curved variants are added.
            _ => panic!("expected Plane"),
        }
    }

    // ----- PR-YR6: curved Surface / Curve construction round-trips -----

    #[test]
    fn surface_sphere_construction() {
        let s = Surface::Sphere {
            center: p(1.0, 2.0, 3.0),
            radius: 5.0,
        };
        match s {
            Surface::Sphere { center, radius } => {
                assert_eq!(center, p(1.0, 2.0, 3.0));
                assert_eq!(radius, 5.0);
            }
            _ => panic!("expected Sphere"),
        }
    }

    #[test]
    fn surface_cylinder_construction() {
        let s = Surface::Cylinder {
            axis_point: p(1.0, 2.0, 3.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 4.0,
        };
        match s {
            Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                assert_eq!(axis_point, p(1.0, 2.0, 3.0));
                assert_eq!(axis_dir, Vector3::new(0.0, 0.0, 1.0));
                assert_eq!(radius, 4.0);
            }
            _ => panic!("expected Cylinder"),
        }
    }

    #[test]
    fn surface_cone_construction() {
        let s = Surface::Cone {
            apex: p(0.0, 0.0, 10.0),
            axis_dir: Vector3::new(0.0, 0.0, -1.0),
            half_angle: 0.5,
        };
        match s {
            Surface::Cone {
                apex,
                axis_dir,
                half_angle,
            } => {
                assert_eq!(apex, p(0.0, 0.0, 10.0));
                assert_eq!(axis_dir, Vector3::new(0.0, 0.0, -1.0));
                assert_eq!(half_angle, 0.5);
            }
            _ => panic!("expected Cone"),
        }
    }

    #[test]
    fn curve_circle_construction() {
        let c = Curve::Circle {
            center: p(1.0, 2.0, 3.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 2.5,
        };
        match c {
            Curve::Circle {
                center,
                normal,
                radius,
            } => {
                assert_eq!(center, p(1.0, 2.0, 3.0));
                assert_eq!(normal, Vector3::new(0.0, 0.0, 1.0));
                assert_eq!(radius, 2.5);
            }
            _ => panic!("expected Circle"),
        }
    }

    #[test]
    fn curve_ellipse_construction() {
        let c = Curve::Ellipse {
            center: p(1.0, 2.0, 3.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            major_axis: Vector3::new(1.0, 0.0, 0.0),
            major_radius: 6.0,
            minor_radius: 3.0,
        };
        match c {
            Curve::Ellipse {
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
            } => {
                assert_eq!(center, p(1.0, 2.0, 3.0));
                assert_eq!(normal, Vector3::new(0.0, 0.0, 1.0));
                assert_eq!(major_axis, Vector3::new(1.0, 0.0, 0.0));
                assert_eq!(major_radius, 6.0);
                assert_eq!(minor_radius, 3.0);
            }
            _ => panic!("expected Ellipse"),
        }
    }

    // ----- PR-YR6: BRep::new loud-rejects curved surfaces -----

    /// Minimal well-formed single-triangle topology (3 verts, 3 edges, one
    /// face with a 3-edge outer loop). Mirrors the `brep_new_single_triangle`
    /// fixture exactly except the single face's surface is caller-supplied,
    /// so the ONLY variable across the loud-rejection tests is the surface.
    fn single_triangle_topology(
        surface: Surface,
    ) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface,
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        (verts, edges, faces)
    }

    #[test]
    fn brep_new_rejects_sphere_face() {
        // PR-YR12 migration: the sphere path is now implemented, but a sphere
        // face on a single *triangle* (no Circle meridian seam edge) lacks the
        // seam the sphere tessellation requires, so it is rejected as
        // MalformedTopology rather than CurvedSurfaceNotYetSupported. It must
        // STILL error loudly; only the error kind changed (mirrors the cylinder
        // migration above).
        let (verts, edges, faces) = single_triangle_topology(Surface::Sphere {
            center: p(0.0, 0.0, 0.0),
            radius: 1.0,
        });
        let result = BRep::new(verts, edges, faces);
        assert!(
            matches!(result, Err(YangError::MalformedTopology(_))),
            "expected MalformedTopology (sphere on a triangle lacks its meridian \
             seam Circle edge), got {result:?}"
        );
    }

    #[test]
    fn brep_new_rejects_cylinder_face() {
        // PR-YR7 migration: the cylinder lateral path is now implemented, but a
        // cylinder face on a single *triangle* (no Circle rim edges) lacks the
        // lateral's 2 required Circle rims, so it is rejected as
        // MalformedTopology rather than CurvedSurfaceNotYetSupported. It must
        // STILL error loudly; only the error kind changed.
        let (verts, edges, faces) = single_triangle_topology(Surface::Cylinder {
            axis_point: p(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        });
        let result = BRep::new(verts, edges, faces);
        assert!(
            matches!(result, Err(YangError::MalformedTopology(_))),
            "expected MalformedTopology (cylinder lateral on a triangle lacks its \
             2 Circle rim edges), got {result:?}"
        );
    }

    #[test]
    fn brep_new_rejects_cone_face() {
        // PR-YR16 migration: a Cone face on a *triangle* (no base-rim Circle the
        // cone tessellation path requires) is now MalformedTopology, mirroring the
        // cylinder/sphere-on-a-triangle rejection. It must STILL error loudly
        // (never silently succeed); only the error *kind* changed.
        let (verts, edges, faces) = single_triangle_topology(Surface::Cone {
            apex: p(0.0, 0.0, 1.0),
            axis_dir: Vector3::new(0.0, 0.0, -1.0),
            half_angle: 0.5,
        });
        let result = BRep::new(verts, edges, faces);
        assert!(
            matches!(result, Err(YangError::MalformedTopology(_))),
            "expected MalformedTopology (cone lateral on a triangle lacks its \
             base-rim Circle edge), got {result:?}"
        );
    }

    #[test]
    fn curve_line_segment_construction() {
        let c = Curve::LineSegment;
        assert_eq!(c, Curve::LineSegment);
    }

    #[test]
    fn brep_topology_construction() {
        let v = BRepVertex {
            point: p(0.0, 0.0, 0.0),
        };
        let e = BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        };
        let f = BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        };
        assert_eq!(v.point, p(0.0, 0.0, 0.0));
        assert_eq!(e.start, 0);
        assert_eq!(f.outer_loop.len(), 3);
    }

    #[test]
    fn tessellation_source_round_trip() {
        let src = TessellationSource::BRepVertex(7);
        match src {
            TessellationSource::BRepVertex(i) => assert_eq!(i, 7),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn tessellation_map_empty() {
        let m = TessellationMap::empty();
        assert_eq!(m.len(), 0);
        assert!(m.is_empty());
    }

    // ----- Group 3: from_mesh degenerate path -----

    #[test]
    fn from_mesh_preserves_mesh() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.as_mesh(), &m);
    }

    #[test]
    fn from_mesh_map_length_matches_verts() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.tessellation_map().len(), m.num_verts());
    }

    #[test]
    fn from_mesh_map_entries_all_unknown() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        for i in 0..b.tessellation_map().len() as u32 {
            assert_eq!(b.tessellation_map().lookup(i), TessellationSource::Unknown);
        }
    }

    // ----- Group 4: BRep::new Stage 1 happy paths -----

    fn plane_z_up() -> Surface {
        Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        }
    }

    #[test]
    fn brep_new_single_triangle() {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let b = BRep::new(verts, edges, faces).unwrap();
        assert_eq!(b.num_verts(), 3);
        assert_eq!(b.num_tris(), 1);
        for i in 0..3u32 {
            assert_eq!(
                b.tessellation_map().lookup(i),
                TessellationSource::BRepVertex(i)
            );
        }
    }

    #[test]
    fn brep_new_quad_face() {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 3,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 3,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let b = BRep::new(verts, edges, faces).unwrap();
        assert_eq!(b.num_verts(), 4);
        assert_eq!(b.num_tris(), 2); // 4-vert fan: 2 tris
    }

    #[test]
    fn brep_new_tetrahedron() {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 0.0, 1.0),
            },
        ];
        // Edges of a tetrahedron: 6 edges between 4 vertices.
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            }, // 0
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            }, // 1
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            }, // 2
            BRepEdge {
                start: 0,
                end: 3,
                curve: Curve::LineSegment,
            }, // 3
            BRepEdge {
                start: 1,
                end: 3,
                curve: Curve::LineSegment,
            }, // 4
            BRepEdge {
                start: 2,
                end: 3,
                curve: Curve::LineSegment,
            }, // 5
            // Reverse-direction edges for the loops (each tet face has 3 edges)
            BRepEdge {
                start: 3,
                end: 0,
                curve: Curve::LineSegment,
            }, // 6
            BRepEdge {
                start: 3,
                end: 1,
                curve: Curve::LineSegment,
            }, // 7
            BRepEdge {
                start: 3,
                end: 2,
                curve: Curve::LineSegment,
            }, // 8
            BRepEdge {
                start: 1,
                end: 0,
                curve: Curve::LineSegment,
            }, // 9
            BRepEdge {
                start: 2,
                end: 1,
                curve: Curve::LineSegment,
            }, // 10
            BRepEdge {
                start: 0,
                end: 2,
                curve: Curve::LineSegment,
            }, // 11
        ];
        // 4 triangular faces. Each loop is 3 edges. Note: outer_loop's
        // start vertices must form a coherent cycle for fan-triangulation
        // to produce correct tris; we use edges 0,1,2 for the "bottom"
        // (verts 0→1→2), etc.
        let faces = vec![
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![0, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            }, // bottom (verts 0,1,2)
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![9, 3, 7],
                inner_loops: Vec::new(),
                reversed: false,
            }, // back (verts 1,0,3) - using 1→0,0→3,3→1
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![10, 4, 8],
                inner_loops: Vec::new(),
                reversed: false,
            }, // right (verts 2,1,3)
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![11, 5, 6],
                inner_loops: Vec::new(),
                reversed: false,
            }, // left (verts 0,2,3)
        ];
        let b = BRep::new(verts, edges, faces).unwrap();
        assert_eq!(b.num_verts(), 4);
        assert_eq!(b.num_tris(), 4);
    }

    #[test]
    fn brep_new_unit_cube() {
        // 8 verts of a unit cube at origin.
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 0.0, 1.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 1.0),
            },
            BRepVertex {
                point: p(1.0, 1.0, 1.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 1.0),
            },
        ];
        // For PR-YR2 we don't need real edge dedup; just enumerate the
        // 24 directed edges we'll need (one per face boundary).
        // bottom face vertices: 0→3→2→1, edges 0:0→3, 1:3→2, 2:2→1, 3:1→0
        // (we just need fan_verts[0] to be the starting vertex of each
        // outer_loop)
        let edges: Vec<BRepEdge> = vec![
            // bottom face: 0, 3, 2, 1
            (0, 3),
            (3, 2),
            (2, 1),
            (1, 0),
            // top face: 4, 5, 6, 7
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            // south face: 0, 1, 5, 4
            (0, 1),
            (1, 5),
            (5, 4),
            (4, 0),
            // north face: 3, 7, 6, 2
            (3, 7),
            (7, 6),
            (6, 2),
            (2, 3),
            // east face: 1, 2, 6, 5
            (1, 2),
            (2, 6),
            (6, 5),
            (5, 1),
            // west face: 0, 4, 7, 3
            (0, 4),
            (4, 7),
            (7, 3),
            (3, 0),
        ]
        .into_iter()
        .map(|(s, e)| BRepEdge {
            start: s,
            end: e,
            curve: Curve::LineSegment,
        })
        .collect();
        let plane = plane_z_up();
        let faces = vec![
            BRepFace {
                surface: plane,
                outer_loop: vec![0, 1, 2, 3],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![4, 5, 6, 7],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![8, 9, 10, 11],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![12, 13, 14, 15],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![16, 17, 18, 19],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![20, 21, 22, 23],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        let b = BRep::new(verts, edges, faces).unwrap();
        assert_eq!(b.num_verts(), 8);
        assert_eq!(b.num_tris(), 12); // 6 quads × 2 tris each
    }

    #[test]
    fn brep_new_bijection_is_one_to_one() {
        // Build a tetrahedron and confirm every mesh vertex i maps to
        // TessellationSource::BRepVertex(i).
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 0.0, 1.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let b = BRep::new(verts, edges, faces).unwrap();
        for i in 0..b.num_verts() as u32 {
            assert_eq!(
                b.tessellation_map().lookup(i),
                TessellationSource::BRepVertex(i),
                "vertex {i} should map to BRepVertex({i})"
            );
        }
    }

    // ----- Group 5: Error paths -----

    #[test]
    fn brep_new_face_with_too_few_edges_errors() {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
        ];
        let edges = vec![BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        }];
        // 1-edge face — degenerate
        let faces = vec![BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let err = BRep::new(verts, edges, faces).unwrap_err();
        match err {
            YangError::MalformedTopology(_) => {}
            other => panic!("expected MalformedTopology, got {:?}", other),
        }
    }

    #[test]
    fn brep_new_out_of_range_edge_index_errors() {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        // Face references edge 99 — out of range
        let faces = vec![BRepFace {
            surface: plane_z_up(),
            outer_loop: vec![0, 1, 99],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        let err = BRep::new(verts, edges, faces).unwrap_err();
        match err {
            YangError::MalformedTopology(_) => {}
            other => panic!("expected MalformedTopology, got {:?}", other),
        }
    }

    // ----- PR-YR1 backward-compat: existing boolean dispatch tests -----

    #[test]
    fn brep_from_mesh_as_mesh_round_trip() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.as_mesh(), &m);
    }

    #[test]
    fn brep_into_mesh_returns_wrapped() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.into_mesh(), m);
    }

    #[test]
    fn brep_counts_delegate_to_mesh() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.num_verts(), m.num_verts());
        assert_eq!(b.num_tris(), m.num_tris());
    }

    #[test]
    fn yang_error_display_non_empty() {
        for e in [
            YangError::NonManifoldInput,
            YangError::NonManifoldOutput,
            YangError::MeshBooleanFailed(Box::from("test")),
            YangError::MalformedTopology("test".to_string()),
        ] {
            let msg = format!("{}", e);
            assert!(!msg.is_empty(), "empty Display for {e:?}");
        }
    }

    #[test]
    fn yang_error_source_propagates() {
        let inner: Box<dyn Error + Send + Sync> = Box::from("inner");
        let e = YangError::MeshBooleanFailed(inner);
        let src = e.source().expect("source should be Some");
        assert_eq!(src.to_string(), "inner");
    }

    #[test]
    fn boolean_with_ok_backend() {
        // M3: boolean() consumes a LabeledArrangement. An empty arrangement
        // (0 tris) keeps nothing → empty output BRep, Ok.
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let backend = LabelMockBackend::new(empty_arrangement());
        let r = boolean(&a, &b, BoolOp::Union, &backend).unwrap();
        assert_eq!(r.num_verts(), 0);
    }

    #[test]
    fn boolean_with_err_backend() {
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let mock = MockBackend;
        match boolean(&a, &b, BoolOp::Union, &mock) {
            Err(YangError::MeshBooleanFailed(_)) => {}
            other => panic!("expected MeshBooleanFailed, got {:?}", other),
        }
    }

    #[test]
    fn boolean_dispatches_all_four_ops() {
        // M3: an empty arrangement is keep-set-empty for every op → Ok.
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        for op in [
            BoolOp::Union,
            BoolOp::Intersect,
            BoolOp::Subtract,
            BoolOp::Xor,
        ] {
            let backend = LabelMockBackend::new(empty_arrangement());
            assert!(boolean(&a, &b, op, &backend).is_ok(), "op {op:?}");
        }
    }

    // ----- PR-YR3: Group 1 — TessellationSource::Intersection variant -----

    #[test]
    fn intersection_variant_constructs_and_matches() {
        let s = TessellationSource::Intersection;
        match s {
            TessellationSource::Intersection => {}
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn intersection_distinct_from_unknown() {
        assert_ne!(
            TessellationSource::Intersection,
            TessellationSource::Unknown
        );
    }

    // ----- PR-YR3: Group 2 — MATCH_TOLERANCE constant -----

    #[test]
    fn match_tolerance_is_1e_minus_9() {
        assert_eq!(MATCH_TOLERANCE, 1e-9);
    }

    // ----- PR-YR3: Group 3 — Spatial matching via mock backend -----

    /// Build a BRep with explicit topology (triangle) so its mesh has
    /// non-trivial TessellationMap entries (`BRepVertex(i)` for each i).
    fn triangle_brep() -> BRep {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            },
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            },
        ];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        BRep::new(verts, edges, faces).unwrap()
    }

    // PR-YR3 spatial-vertex-provenance was REMOVED from production by M3
    // (production tessellation_map is now BRepVertex(i) 1:1 with the kept
    // sub-mesh). Per Manager policy (a), these tests are reworked to call
    // the now-#[cfg(test)] substitute helper `match_with_input` DIRECTLY,
    // preserving the substitute's coverage as the M4 oracle rather than
    // routing through production `boolean()`.

    #[test]
    fn boolean_input_a_verbatim_copies_a_map() {
        let a = triangle_brep();
        let b = triangle_brep();
        // Each of A's mesh verts matches input A's BRepVertex(i).
        for (i, &target) in a.as_mesh().verts.iter().enumerate() {
            let (input, src) = match_with_input(&a, &b, target);
            assert_eq!(input, Some(InputId::A), "vert {i} should match A");
            assert_eq!(
                src,
                TessellationSource::BRepVertex(i as u32),
                "output vertex {i}"
            );
        }
    }

    #[test]
    fn boolean_input_b_verbatim_copies_b_map() {
        let a = triangle_brep();
        // B has different vertices so A's spatial match fails first.
        let mut b_verts = a.vertices().to_vec();
        for v in &mut b_verts {
            v.point = Point3::new(v.point.x() + 10.0, v.point.y(), v.point.z());
        }
        let b = BRep::new(b_verts, a.edges().to_vec(), a.faces().to_vec()).unwrap();
        for (i, &target) in b.as_mesh().verts.iter().enumerate() {
            let (input, src) = match_with_input(&a, &b, target);
            assert_eq!(input, Some(InputId::B), "vert {i} should match B");
            assert_eq!(
                src,
                TessellationSource::BRepVertex(i as u32),
                "output vertex {i} — should match input B's BRepVertex({i})"
            );
        }
    }

    #[test]
    fn boolean_all_new_coords_are_intersection() {
        let a = triangle_brep();
        let b = triangle_brep();
        // Coords far from both inputs → no match → Intersection.
        for target in [
            p(100.0, 100.0, 100.0),
            p(101.0, 100.0, 100.0),
            p(100.0, 101.0, 100.0),
        ] {
            let (input, src) = match_with_input(&a, &b, target);
            assert_eq!(input, None);
            assert_eq!(
                src,
                TessellationSource::Intersection,
                "novel coord should be Intersection"
            );
        }
    }

    #[test]
    fn boolean_mixed_match_and_intersection() {
        let a = triangle_brep();
        let b = triangle_brep();
        // 2 verts from A + 2 new coords.
        let expectations = [
            (p(0.0, 0.0, 0.0), TessellationSource::BRepVertex(0)),
            (p(1.0, 0.0, 0.0), TessellationSource::BRepVertex(1)),
            (p(99.0, 99.0, 0.0), TessellationSource::Intersection),
            (p(98.0, 98.0, 0.0), TessellationSource::Intersection),
        ];
        for (i, (target, expect)) in expectations.into_iter().enumerate() {
            let (_input, src) = match_with_input(&a, &b, target);
            assert_eq!(src, expect, "vertex {i}");
        }
    }

    // ----- PR-YR4: Group 1 — types -----

    #[test]
    fn input_id_ordering_and_derives() {
        assert!(InputId::A < InputId::B);
        assert_eq!(InputId::A, InputId::A);
        assert_ne!(InputId::A, InputId::B);
        assert_eq!(format!("{:?}", InputId::A), "A");
        assert_eq!(format!("{:?}", InputId::B), "B");
        // Copy
        let x = InputId::A;
        let y = x;
        assert_eq!(x, y);
    }

    #[test]
    fn triangle_attribution_construct_and_equality() {
        let t1 = TriangleAttribution {
            input: InputId::A,
            face: 7,
        };
        let t2 = TriangleAttribution {
            input: InputId::A,
            face: 7,
        };
        let t3 = TriangleAttribution {
            input: InputId::B,
            face: 7,
        };
        assert_eq!(t1, t2);
        assert_ne!(t1, t3);
        // Copy + accessors
        let t4 = t1;
        assert_eq!(t4.input, InputId::A);
        assert_eq!(t4.face, 7);
    }

    #[test]
    fn triangle_attribution_map_empty_and_len() {
        let m = TriangleAttributionMap::empty();
        assert_eq!(m.len(), 0);
        assert!(m.is_empty());
    }

    // ----- PR-YR4: Group 2 — algorithm via mock backend -----

    /// Two-face B-Rep where V0 is shared by F0 and F1; V1, V2 only in F0;
    /// V3, V4 only in F1. Used by tie-break + pure-input tests.
    fn two_face_shared_vertex_brep() -> BRep {
        let verts = vec![
            BRepVertex {
                point: p(0.0, 0.0, 0.0),
            }, // 0 — shared (F0 & F1)
            BRepVertex {
                point: p(1.0, 0.0, 0.0),
            }, // 1 — F0 only
            BRepVertex {
                point: p(1.0, 1.0, 0.0),
            }, // 2 — F0 only (moved off x-axis: was (2,0,0)) so F0 is a real triangle in z=0
            BRepVertex {
                point: p(0.0, 1.0, 0.0),
            }, // 3 — F1 only
            BRepVertex {
                point: p(0.0, 1.0, 1.0),
            }, // 4 — F1 only (moved off y-axis: was (0,2,0)) so F1 is a real triangle in x=0
        ];
        // F0 edges (triangle V0-V1-V2):
        // E0 V0→V1, E1 V1→V2, E2 V2→V0
        // F1 edges (triangle V0-V3-V4):
        // E3 V0→V3, E4 V3→V4, E5 V4→V0
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 1,
                end: 2,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 0,
                end: 3,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 3,
                end: 4,
                curve: Curve::LineSegment,
            },
            BRepEdge {
                start: 4,
                end: 0,
                curve: Curve::LineSegment,
            },
        ];
        // F0 lies in z=0 (normal +z); F1 now lies in x=0 (normal +x).
        let f0_plane = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        let f1_plane = Surface::Plane {
            normal: Vector3::new(1.0, 0.0, 0.0),
            d: 0.0,
        };
        let faces = vec![
            BRepFace {
                surface: f0_plane,
                outer_loop: vec![0, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            }, // F0
            BRepFace {
                surface: f1_plane,
                outer_loop: vec![3, 4, 5],
                inner_loops: Vec::new(),
                reversed: false,
            }, // F1
        ];
        BRep::new(verts, edges, faces).unwrap()
    }

    // PR-YR4 majority-vote ATTRIBUTION was REMOVED from production by M3
    // (production attributes via real LabeledArrangement labels + geometric
    // face resolution). Per Manager policy (a), these tests are reworked to
    // exercise the now-#[cfg(test)] substitute via `substitute_attribution`
    // DIRECTLY (not via production `boolean()`), preserving the substitute's
    // coverage as the M4 differential oracle.

    #[test]
    fn boolean_pure_a_attributes_to_a_faces() {
        // Pure-A: substitute over A's mesh. Each tri's verts are
        // BRepVertex(i) of A → per-vertex face incidence → majority vote
        // attributes each tri to its source face.
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let attr = substitute_attribution(a.as_mesh(), &a, &b);
        assert_eq!(attr.len(), 2);
        assert_eq!(
            attr.lookup(0),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0
            }),
            "output tri 0 (F0 fan tri) should attribute to A's F0"
        );
        assert_eq!(
            attr.lookup(1),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 1
            }),
            "output tri 1 (F1 fan tri) should attribute to A's F1"
        );
    }

    #[test]
    fn boolean_pure_b_attributes_to_b_faces() {
        let a = two_face_shared_vertex_brep();
        // B is the same B-Rep, shifted so A's spatial match fails first.
        let mut b_verts = a.vertices().to_vec();
        for v in &mut b_verts {
            v.point = Point3::new(v.point.x() + 100.0, v.point.y(), v.point.z());
        }
        let b = BRep::new(b_verts, a.edges().to_vec(), a.faces().to_vec()).unwrap();
        let attr = substitute_attribution(b.as_mesh(), &a, &b);
        assert_eq!(
            attr.lookup(0),
            Some(TriangleAttribution {
                input: InputId::B,
                face: 0
            })
        );
        assert_eq!(
            attr.lookup(1),
            Some(TriangleAttribution {
                input: InputId::B,
                face: 1
            })
        );
    }

    #[test]
    fn boolean_all_new_coords_attribute_to_none() {
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        // A mesh with coords far from both inputs.
        let novel = Mesh::new(
            vec![
                p(1000.0, 1000.0, 1000.0),
                p(1001.0, 1000.0, 1000.0),
                p(1000.0, 1001.0, 1000.0),
            ],
            vec![[0, 1, 2]],
        );
        let attr = substitute_attribution(&novel, &a, &b);
        assert_eq!(attr.len(), 1);
        assert_eq!(
            attr.lookup(0),
            None,
            "all-new triangle should have None attribution"
        );
    }

    #[test]
    fn boolean_mixed_majority_wins() {
        // 2 verts match A's F0 + 1 novel → F0 attribution.
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let mixed = Mesh::new(
            vec![
                p(1.0, 0.0, 0.0),       // matches a.verts[1] (F0 only)
                p(1.0, 1.0, 0.0),       // matches a.verts[2] (F0 only)
                p(1000.0, 0.0, 1000.0), // novel
            ],
            vec![[0, 1, 2]],
        );
        let attr = substitute_attribution(&mixed, &a, &b);
        assert_eq!(
            attr.lookup(0),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0
            }),
            "2 A-F0-verts + 1 novel → majority F0"
        );
    }

    #[test]
    fn boolean_no_majority_returns_none() {
        // 1 A-vert + 1 B-vert + 1 novel → no majority, None.
        let a = two_face_shared_vertex_brep();
        let mut b_verts = a.vertices().to_vec();
        for v in &mut b_verts {
            v.point = Point3::new(v.point.x() + 100.0, v.point.y(), v.point.z());
        }
        let b = BRep::new(b_verts, a.edges().to_vec(), a.faces().to_vec()).unwrap();
        let mixed = Mesh::new(
            vec![
                p(1.0, 0.0, 0.0),     // matches a.verts[1] (A, F0)
                p(101.0, 0.0, 0.0),   // matches b.verts[1] (B, F0)
                p(500.0, 500.0, 0.0), // novel
            ],
            vec![[0, 1, 2]],
        );
        let attr = substitute_attribution(&mixed, &a, &b);
        assert_eq!(
            attr.lookup(0),
            None,
            "1 A + 1 B + 1 novel → no 2-of-3 majority"
        );
    }

    #[test]
    fn boolean_tie_break_picks_lowest_face() {
        // Triangle (V0 shared, V1 F0-only, V3 F1-only) → candidates
        // {F0,F1}, {F0}, {F1}. Counts: F0=2, F1=2. Tie. Lowest face → F0.
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let tie_mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0), // V0 — shared
                p(1.0, 0.0, 0.0), // V1 — F0 only
                p(0.0, 1.0, 0.0), // V3 — F1 only
            ],
            vec![[0, 1, 2]],
        );
        let attr = substitute_attribution(&tie_mesh, &a, &b);
        assert_eq!(
            attr.lookup(0),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0
            }),
            "tie at count 2 between F0 and F1 → lowest face (F0)"
        );
    }

    // ----- PR-YR4: Group 3 — empty-topology degradation (substitute) -----

    #[test]
    fn boolean_both_inputs_from_mesh_all_none() {
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let attr = substitute_attribution(&sample_mesh(), &a, &b);
        assert_eq!(attr.len(), sample_mesh().num_tris());
        assert_eq!(
            attr.lookup(0),
            None,
            "from_mesh inputs have all-Unknown sources → all-None attribution"
        );
    }

    #[test]
    fn boolean_mixed_from_mesh_and_topologized() {
        // a has topology, b is from_mesh. Substitute over a's mesh.
        // Attribution should reflect a's per-tri face ownership.
        let a = two_face_shared_vertex_brep();
        let b = BRep::from_mesh(sample_mesh());
        let attr = substitute_attribution(a.as_mesh(), &a, &b);
        assert_eq!(
            attr.lookup(0),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0
            })
        );
        assert_eq!(
            attr.lookup(1),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 1
            })
        );
    }

    // ----- PR-YR5: topology reconstruction -----
    //
    // `reconstruct_topology` is UNCHANGED production. Per Manager policy
    // (b), these tests previously routed through `boolean()` via the
    // boolean-only MockBackend (which M3 no longer drives); they are
    // reworked to build a `TriangleAttributionMap` via the #[cfg(test)]
    // substitute and call `reconstruct_topology` DIRECTLY — exercising the
    // same durable reconstruction logic without the removed substitute
    // production path.

    #[test]
    fn yr5_single_triangle_round_trip_produces_one_face() {
        // Pure-A on triangle_brep (1 face, 1 fan tri) → 1 face with 3
        // boundary edges + 3 vertices forming a closed cycle.
        let a = triangle_brep();
        let b = triangle_brep();
        let mesh = a.as_mesh().clone();
        let attr = substitute_attribution(&mesh, &a, &b);
        let (verts, edges, faces) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
        assert_eq!(faces.len(), 1, "expected 1 BRepFace");
        assert_eq!(faces[0].outer_loop.len(), 3, "expected 3-edge loop");
        assert_eq!(edges.len(), 3, "expected 3 BRepEdges");
        assert_eq!(verts.len(), 3, "expected 3 BRepVertices");
        // Cycle closure
        let f = &faces[0];
        for i in 0..3 {
            let e_curr = &edges[f.outer_loop[i] as usize];
            let e_next = &edges[f.outer_loop[(i + 1) % 3] as usize];
            assert_eq!(
                e_curr.end, e_next.start,
                "cycle break at edge {i}: {} != {}",
                e_curr.end, e_next.start
            );
        }
    }

    #[test]
    fn yr5_two_face_round_trip_produces_two_faces() {
        // two_face_shared_vertex_brep has 2 triangular faces sharing only
        // V0; 2 output tris with different attributions (F0 vs F1) → 2
        // BRepFaces.
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let mesh = a.as_mesh().clone();
        let attr = substitute_attribution(&mesh, &a, &b);
        let (_v, _e, faces) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
        assert_eq!(faces.len(), 2, "expected 2 BRepFaces");
        for f in &faces {
            assert_eq!(f.outer_loop.len(), 3);
        }
    }

    #[test]
    fn yr5_disconnected_components_become_separate_faces() {
        // Two tris with the SAME attribution but NO shared vertex →
        // flood-fill leaves them as 2 patches → 2 faces. Regression guard
        // vs. naive attribution-bucketing.
        let a = triangle_brep();
        let b = triangle_brep();
        // 6 vertices = TWO copies of A's 3 verts at distinct indices.
        let dup = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0), // matches A.V0
                p(1.0, 0.0, 0.0), // matches A.V1
                p(0.0, 1.0, 0.0), // matches A.V2
                p(0.0, 0.0, 0.0), // duplicate matching A.V0 (different idx)
                p(1.0, 0.0, 0.0), // duplicate matching A.V1
                p(0.0, 1.0, 0.0), // duplicate matching A.V2
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        );
        let attr = substitute_attribution(&dup, &a, &b);
        let (_v, _e, faces) = reconstruct_topology(&dup, &attr, &a, &b).unwrap();
        assert_eq!(
            faces.len(),
            2,
            "disconnected same-attribution tris should be separate faces"
        );
    }

    #[test]
    fn yr5_none_attributed_tris_omitted_from_faces() {
        // tri 0 matches A's verts (Some(A, F0)); tri 1 is all novel coords
        // (None). reconstruct_topology should yield 1 face.
        let a = triangle_brep();
        let b = triangle_brep();
        let mixed = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0), // matches A.V0
                p(1.0, 0.0, 0.0), // matches A.V1
                p(0.0, 1.0, 0.0), // matches A.V2
                p(1000.0, 0.0, 0.0),
                p(1001.0, 0.0, 0.0),
                p(1000.0, 1.0, 0.0),
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        );
        let attr = substitute_attribution(&mixed, &a, &b);
        let (_v, _e, faces) = reconstruct_topology(&mixed, &attr, &a, &b).unwrap();
        assert_eq!(
            faces.len(),
            1,
            "None-attributed tris should not contribute faces"
        );
    }

    #[test]
    fn yr5_vertex_count_matches_mesh() {
        let a = triangle_brep();
        let b = triangle_brep();
        let mesh = a.as_mesh().clone();
        let attr = substitute_attribution(&mesh, &a, &b);
        let (verts, _e, _f) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
        assert_eq!(verts.len(), mesh.num_verts());
        for (i, v) in verts.iter().enumerate() {
            assert_eq!(v.point, mesh.verts[i]);
        }
    }

    #[test]
    fn yr5_surface_inherited_from_input() {
        let a = triangle_brep();
        let b = triangle_brep();
        let mesh = a.as_mesh().clone();
        let attr = substitute_attribution(&mesh, &a, &b);
        let (_v, _e, faces) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
        assert_eq!(faces.len(), 1);
        assert_eq!(
            faces[0].surface,
            a.faces()[0].surface,
            "output face should inherit input A's surface"
        );
    }

    #[test]
    fn yr5_empty_input_produces_empty_face_set() {
        // Both inputs from_mesh → all-None attribution → no faces/edges.
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let mesh = sample_mesh();
        let attr = substitute_attribution(&mesh, &a, &b);
        let (verts, edges, faces) = reconstruct_topology(&mesh, &attr, &a, &b).unwrap();
        assert!(
            faces.is_empty(),
            "all-None attribution should yield empty faces"
        );
        assert!(
            edges.is_empty(),
            "all-None attribution should yield empty edges"
        );
        // Vertices still populated 1:1 with mesh.
        assert_eq!(verts.len(), mesh.num_verts());
    }

    // ----- Stage-6 degenerate-sliver topology (spec yang_stage6_sliver_topology) -----
    //
    // Reproduces §2's measured structure at the unit level: a shared collinear
    // solid-edge chain a–c–d–b where two abutting faces subdivide it
    // DIFFERENTLY, and the arrangement keeps ZERO-AREA shim slivers along the
    // chord to stay watertight. One sliver is wound so its directed chord edge
    // DUPLICATES the real triangle's chord edge (sign-of-zero winding is
    // arbitrary) — the measured fold. Today `reconstruct_topology` dead-ends in
    // `patch_boundary_cycle` at `NonManifoldOutput`; the Stage-6 design (spec §4:
    // exclude degenerate tris from boundary derivation + loop T-subdivision) must
    // reassemble a 2-manifold output whose shared segments are each 2-covered.

    /// The shared solid edge is the y-axis (x=0, z=0): the intersection of the
    /// two abutting faces' planes z=0 (face 0, apex off +y in z=0) and x=0
    /// (face 1, apex off +y in x=0). Chain vertices a<c<d<b sit on the y-axis,
    /// exactly collinear, so every sliver along it is exactly zero-area.
    ///
    /// Vertex indices: 0=a 1=b 2=c 3=d 4=x1(face-0 apex) 5=x2(face-1 apex).
    fn sliver_fixture_mesh() -> Mesh {
        Mesh::new(
            vec![
                p(0.0, 0.0, 0.0), // 0 = a  (chain end)
                p(0.0, 3.0, 0.0), // 1 = b  (chain end)
                p(0.0, 1.0, 0.0), // 2 = c  (between a,b)
                p(0.0, 2.0, 0.0), // 3 = d  (between a,b)
                p(1.0, 1.5, 0.0), // 4 = x1 (face 0 apex, z=0 plane)
                p(0.0, 1.5, 1.0), // 5 = x2 (face 1 apex, x=0 plane)
            ],
            vec![
                // face 0 (z=0 plane, normal +z): ONE real triangle carrying the
                // whole chord b→a, plus two zero-area shim slivers wound so each
                // DUPLICATES the real directed chord edge b→a (1→0).
                [0, 4, 1], // T1 real: edges a→x1, x1→b, b→a
                [1, 0, 2], // S1 sliver: edges b→a (dup!), a→c, c→b
                [1, 0, 3], // S2 sliver: edges b→a (dup!), a→d, d→b
                // face 1 (x=0 plane, normal +x): the OTHER side subdivides the
                // chain a→c→d→b (opposite direction) via a fan from x2.
                [0, 2, 5], // edges a→c, c→x2, x2→a
                [2, 3, 5], // edges c→d, d→x2, x2→c
                [3, 1, 5], // edges d→b, b→x2, x2→d
            ],
        )
    }

    /// Attribution for `sliver_fixture_mesh`: face-0 patch = {T1,S1,S2},
    /// face-1 patch = {the three fan tris}. Built directly (in-module access to
    /// the private field) so the slivers land in face 0's patch deterministically
    /// — this is the measured N4-provenance placement (§2.3), not a geometric
    /// guess.
    fn sliver_fixture_attr() -> TriangleAttributionMap {
        let f0 = Some(TriangleAttribution {
            input: InputId::A,
            face: 0,
        });
        let f1 = Some(TriangleAttribution {
            input: InputId::A,
            face: 1,
        });
        TriangleAttributionMap {
            attributions: vec![f0, f0, f0, f1, f1, f1],
        }
    }

    /// Canonical undirected key.
    fn und(x: u32, y: u32) -> (u32, u32) {
        if x < y {
            (x, y)
        } else {
            (y, x)
        }
    }

    /// Multiset of undirected loop edges across ALL output faces, derived from
    /// each face's `outer_loop` (edge indices) via the returned edge table.
    fn loop_edge_counts(
        edges: &[BRepEdge],
        faces: &[BRepFace],
    ) -> std::collections::BTreeMap<(u32, u32), u32> {
        let mut counts: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for f in faces {
            for &ei in &f.outer_loop {
                let e = &edges[ei as usize];
                *counts.entry(und(e.start, e.end)).or_insert(0) += 1;
            }
            for hole in &f.inner_loops {
                for &ei in hole {
                    let e = &edges[ei as usize];
                    *counts.entry(und(e.start, e.end)).or_insert(0) += 1;
                }
            }
        }
        counts
    }

    /// TARGET (spec §5 S2/S4). RED today: `reconstruct_topology` dead-ends at
    /// `NonManifoldOutput` because sliver S1's directed edge b→a duplicates
    /// real T1's b→a, unbalancing face 0's boundary walk. GREEN: slivers are
    /// excluded from boundary derivation (A) and face 0's chord is T-subdivided
    /// at c,d (B) so every shared segment is 2-covered.
    #[test]
    fn stage6_sliver_fold_reassembles_with_subdivided_chord() {
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let mesh = sliver_fixture_mesh();
        let attr = sliver_fixture_attr();

        let (_verts, edges, faces) = reconstruct_topology(&mesh, &attr, &a, &b).expect(
            "Stage-6 sliver RED: reconstruction must succeed once zero-area slivers are \
             excluded from boundary derivation (spec §4A) — today it dead-ends at \
             NonManifoldOutput on the duplicated chord edge b→a",
        );

        // S2: both real faces survive (slivers carry no boundary of their own).
        assert_eq!(
            faces.len(),
            2,
            "expected 2 output faces (chord side + chain side)"
        );

        let counts = loop_edge_counts(&edges, &faces);

        // S4: the full chord (a,b) must NOT remain a raw loop edge — it is
        // T-subdivided at c,d.
        assert_eq!(
            counts.get(&und(0, 1)).copied().unwrap_or(0),
            0,
            "chord (a,b) must be subdivided at c,d, not carried as a single loop edge; \
             loop edges: {counts:?}"
        );
        // S4: every shared segment of the solid edge is used by exactly two
        // directed loop edges (2-manifold seam).
        for (name, key) in [("a–c", und(0, 2)), ("c–d", und(2, 3)), ("d–b", und(3, 1))] {
            assert_eq!(
                counts.get(&key).copied().unwrap_or(0),
                2,
                "shared segment {name} must be 2-covered across output loops; \
                 loop edges: {counts:?}"
            );
        }
    }

    /// S5 (spec §5): a patch made ENTIRELY of zero-area slivers cannot bound a
    /// face — it must stay loudly `NonManifoldOutput`, never silently emit a
    /// degenerate face. Passes today (the fold errors) and must remain Err
    /// through the fix (excluding all its triangles leaves no boundary).
    #[test]
    fn stage6_all_degenerate_patch_stays_loud() {
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        // A single patch of ONLY collinear slivers on the y-axis (no real tri).
        let mesh = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0), // 0 = a
                p(0.0, 3.0, 0.0), // 1 = b
                p(0.0, 1.0, 0.0), // 2 = c
                p(0.0, 2.0, 0.0), // 3 = d
            ],
            vec![[1, 0, 2], [1, 0, 3]], // two zero-area slivers sharing (a,b)
        );
        let f0 = Some(TriangleAttribution {
            input: InputId::A,
            face: 0,
        });
        let attr = TriangleAttributionMap {
            attributions: vec![f0, f0],
        };
        assert!(
            reconstruct_topology(&mesh, &attr, &a, &b).is_err(),
            "an all-degenerate patch must stay loud (NonManifoldOutput) — it cannot bound a face"
        );
    }

    // ====================================================================
    // M3 — functional boolean via LabeledArrangement (Group A unit tests)
    //
    // These tests target the M3 rewire: boolean() must consume a real
    // `LabeledArrangement` from `backend.labeled_arrangement(..)`, select
    // result triangles via `keep_set(op)`, geometrically resolve each kept
    // triangle's source face (centroid-in-plane), and produce a FULL
    // attribution (every output triangle → Some). Spec:
    // specs/yang_m3_functional_boolean.md (I7 unique-face, F1/F2/F3).
    //
    // RED expectations until the Implementer lands M3:
    //   - `MeshBoolean::labeled_arrangement` trait method does not exist.
    //   - `YangError::FaceResolutionFailed { tri }` variant does not exist.
    //   - `LabeledArrangement` is not imported here yet.
    //   - current boolean() ignores labels → no full coverage.
    // ====================================================================

    use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};

    /// Mock backend that returns a hand-built `LabeledArrangement` from
    /// the (M3) `labeled_arrangement` trait method. `boolean()` is still
    /// required (object-safe trait) but is unused on the M3 path.
    struct LabelMockBackend {
        arrangement: LabeledArrangement,
    }
    impl LabelMockBackend {
        fn new(arrangement: LabeledArrangement) -> Self {
            Self { arrangement }
        }
    }
    impl MeshBoolean for LabelMockBackend {
        fn boolean(
            &self,
            _a: &Mesh,
            _b: &Mesh,
            _op: BoolOp,
        ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
            // Not exercised on the M3 path; return the arrangement mesh so
            // a stray call is at least well-formed.
            Ok(self.arrangement.mesh.clone())
        }
        // M3: the trait gains this method (default impl errors NotSupported);
        // this mock overrides it with a hand-built arrangement.
        fn labeled_arrangement(
            &self,
            _a: &Mesh,
            _b: &Mesh,
        ) -> Result<LabeledArrangement, Box<dyn Error + Send + Sync>> {
            Ok(self.arrangement.clone())
        }
    }

    /// Axis-aligned unit cube BRep at `origin` with correct OUTWARD face
    /// normals — minimal topology sufficient for geometric face
    /// resolution (centroid-in-plane). 8 verts, 24 edges, 6 quad faces.
    fn cube_brep(origin: [f64; 3]) -> BRep {
        let [x, y, z] = origin;
        let verts = vec![
            BRepVertex { point: p(x, y, z) },
            BRepVertex {
                point: p(x + 1.0, y, z),
            },
            BRepVertex {
                point: p(x + 1.0, y + 1.0, z),
            },
            BRepVertex {
                point: p(x, y + 1.0, z),
            },
            BRepVertex {
                point: p(x, y, z + 1.0),
            },
            BRepVertex {
                point: p(x + 1.0, y, z + 1.0),
            },
            BRepVertex {
                point: p(x + 1.0, y + 1.0, z + 1.0),
            },
            BRepVertex {
                point: p(x, y + 1.0, z + 1.0),
            },
        ];
        let face_verts: [[u32; 4]; 6] = [
            [0, 1, 2, 3], // bottom (z)
            [4, 7, 6, 5], // top (z+1)
            [0, 4, 5, 1], // front (y)
            [1, 5, 6, 2], // right (x+1)
            [2, 6, 7, 3], // back (y+1)
            [3, 7, 4, 0], // left (x)
        ];
        let mut edges = Vec::new();
        let mut loops = Vec::new();
        for vs in &face_verts {
            let base = edges.len() as u32;
            for i in 0..4 {
                edges.push(BRepEdge {
                    start: vs[i],
                    end: vs[(i + 1) % 4],
                    curve: Curve::LineSegment,
                });
            }
            loops.push(vec![base, base + 1, base + 2, base + 3]);
        }
        let normals = [
            Vector3::new(0.0, 0.0, -1.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(0.0, -1.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(-1.0, 0.0, 0.0),
        ];
        // Plane convention n·x + d = 0. For a face on plane n·x = c the
        // offset is d = -c — WITH n the face's OUTWARD normal, so the three
        // negative-axis faces have c = -coord (e.g. bottom: n=(0,0,-1),
        // n·p = -z ⇒ d = z). The pre-2026-07-03 array had the sign flipped
        // on every face with a non-zero plane coordinate; it went unnoticed
        // because the historical bottom-quad arrangement only ever resolved
        // attribution against the origin cube's BOTTOM face (d = 0 either
        // way). The closed-shell fixture (rule-4 gate cycle) exercises all
        // six planes and unmasked it.
        let offs = [z, -(z + 1.0), y, -(x + 1.0), -(y + 1.0), x];
        let faces: Vec<BRepFace> = (0..6)
            .map(|i| BRepFace {
                surface: Surface::Plane {
                    normal: normals[i],
                    d: offs[i],
                },
                outer_loop: loops[i].clone(),
                inner_loops: Vec::new(),
                reversed: false,
            })
            .collect();
        BRep::new(verts, edges, faces).unwrap()
    }

    // N4 (1b): `BRep::new` must populate the per-triangle → owning-face map
    // (`tri_face`) 1:1 with the Stage-1 mesh triangles, with valid face indices
    // and every face owning ≥1 triangle. This is the provenance substrate that
    // lets `boolean()` attribute kept triangles to faces directly from cherchi's
    // `source` instead of geometric proximity. (The end-to-end correctness of
    // provenance attribution is covered by the full boolean suite / box fuzz,
    // which now runs provenance as the PRIMARY path.)
    #[test]
    fn brep_new_populates_tri_face_provenance() {
        let cube = cube_brep([0.0, 0.0, 0.0]);
        let tf = cube.tri_face();
        assert_eq!(
            tf.len(),
            cube.as_mesh().tris.len(),
            "tri_face must be 1:1 with the Stage-1 mesh triangles"
        );
        let nf = cube.faces().len() as u32;
        assert_eq!(nf, 6, "cube has 6 faces");
        let mut owned = vec![false; nf as usize];
        for (t, &f) in tf.iter().enumerate() {
            assert!(f < nf, "tri {t} → face {f} out of range (faces = {nf})");
            owned[f as usize] = true;
        }
        assert!(
            owned.iter().all(|&o| o),
            "every cube face must own ≥1 Stage-1 triangle"
        );

        // `from_mesh` has no Stage-1 face lineage → empty tri_face (→ geometric
        // fallback in attribution).
        let degenerate = BRep::from_mesh(cube.as_mesh().clone());
        assert!(
            degenerate.tri_face().is_empty(),
            "from_mesh BRep carries no provenance map"
        );
    }

    /// Centroid of a triangle.
    fn centroid(mesh: &Mesh, tri: [u32; 3]) -> Point3 {
        let a = mesh.verts[tri[0] as usize].as_array();
        let b = mesh.verts[tri[1] as usize].as_array();
        let c = mesh.verts[tri[2] as usize].as_array();
        Point3::new(
            (a[0] + b[0] + c[0]) / 3.0,
            (a[1] + b[1] + c[1]) / 3.0,
            (a[2] + b[2] + c[2]) / 3.0,
        )
    }

    /// Find the single face of `brep` whose plane contains `c` within
    /// TAU_WORK; panics if zero or >1 (the expected-attribution helper
    /// must be unambiguous for a well-posed fixture).
    fn resolve_face(brep: &BRep, c: Point3) -> u32 {
        let mut hit: Option<u32> = None;
        for (i, f) in brep.faces().iter().enumerate() {
            let Surface::Plane { normal, d } = f.surface else {
                continue;
            };
            let n = normal.as_array();
            let cc = c.as_array();
            let dist = (n[0] * cc[0] + n[1] * cc[1] + n[2] * cc[2] + d).abs();
            if dist < cad_primitives::TAU_WORK {
                assert!(hit.is_none(), "ambiguous: centroid on >1 face plane");
                hit = Some(i as u32);
            }
        }
        hit.expect("centroid lies on no face plane")
    }

    // ----- Group A.1: full attribution coverage + correctness -----

    /// Hand-built arrangement: cube A's full closed surface shell. The verts
    /// are A's exact 8 `BRepVertex` corners, so:
    /// - real-label path: each tri's centroid lies strictly inside exactly
    ///   one A face plane → I7 unique-face → full Some(A, face) attribution;
    /// - every patch boundary closes (per-face manifold cycles) and the
    ///   whole shell is watertight, matching the closed kept mesh a real
    ///   boolean produces;
    /// - the verts coincide with A's `BRepVertex`es, so the M4 substitute's
    ///   spatial matching also resolves each tri to its cube face
    ///   (vertex-face incidence majority), letting the differential oracle
    ///   agree.
    ///
    /// All `inside` all-false ⇒ all 12 tris kept by Union.
    fn arrangement_a_cube_shell() -> LabeledArrangement {
        // The full unit-cube SURFACE of `cube_brep([0,0,0])`: 12 outward-wound
        // tris, 2 per face. Historically this fixture was A's bottom quad only
        // (an open 2-tri sheet) — a mock shape no real boolean produces. The
        // 2026-07-03 gate cycle (spec `yang_kept_mesh_manifold_gate`, aborted
        // per P10 — see its §2b) closed it to model a real kept mesh; the
        // closed form is kept: it is strictly more faithful and it unmasked
        // the `cube_brep` plane-offset sign bug below. All consuming
        // assertions are computed FROM the fixture (keep-set count, geometric
        // face resolve, majority vote), so their intent is unchanged.
        let verts = vec![
            p(0.0, 0.0, 0.0), // 0
            p(1.0, 0.0, 0.0), // 1
            p(1.0, 1.0, 0.0), // 2
            p(0.0, 1.0, 0.0), // 3
            p(0.0, 0.0, 1.0), // 4
            p(1.0, 0.0, 1.0), // 5
            p(1.0, 1.0, 1.0), // 6
            p(0.0, 1.0, 1.0), // 7
        ];
        // Outward winding per face (−z, +z, −y, +y, −x, +x); every directed
        // edge pairs with its reverse ⇒ watertight 2-manifold (χ = 2).
        let tris = vec![
            [0u32, 3, 2],
            [0, 2, 1], // bottom z=0
            [4, 5, 6],
            [4, 6, 7], // top z=1
            [0, 1, 5],
            [0, 5, 4], // front y=0
            [2, 3, 7],
            [2, 7, 6], // back y=1
            [0, 4, 7],
            [0, 7, 3], // left x=0
            [1, 2, 6],
            [1, 6, 5], // right x=1
        ];
        let mesh = Mesh::new(verts, tris);
        // All on A's surface (solid 0), none on B; inside all-false ⇒ Union keeps.
        let surface = vec![vec![LaInputId(0)]; 12];
        let inside = vec![vec![false, false]; 12];
        let patch = vec![0u32, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5];
        LabeledArrangement {
            mesh,
            surface,
            inside,
            patch,
            source: Vec::new(),
            num_inputs: 2,
        }
    }

    #[test]
    fn m3_union_full_attribution_coverage() {
        // I7 + full-coverage: every kept output triangle resolves to Some.
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B offset on ALL axes — a [0.5,0,0] offset shares the
        // y/z face planes with A (bit-exact coplanar input), which the
        // near-coplanar input gate now rejects BEFORE the (mock) backend.
        let b = cube_brep([0.5, 0.3, 0.4]);
        let la = arrangement_a_cube_shell();
        let backend = LabelMockBackend::new(la);
        let r = boolean(&a, &b, BoolOp::Union, &backend).unwrap();

        let attr = r.triangle_attribution();
        assert_eq!(
            attr.len(),
            r.num_tris(),
            "attribution length must equal output triangle count"
        );
        assert!(r.num_tris() > 0, "expected non-empty kept sub-mesh");
        for t in 0..attr.len() as u32 {
            assert!(
                attr.lookup(t).is_some(),
                "M3 requires FULL attribution: tri {t} is None (skeleton, not closed)"
            );
        }
    }

    #[test]
    fn m3_union_attribution_matches_geometric_face() {
        // F1: each kept tri attributes to the unique A-face plane its
        // centroid lies on (one of the cube shell's six faces).
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B offset on ALL axes — a [0.5,0,0] offset shares the
        // y/z face planes with A (bit-exact coplanar input), which the
        // near-coplanar input gate now rejects BEFORE the (mock) backend.
        let b = cube_brep([0.5, 0.3, 0.4]);
        let la = arrangement_a_cube_shell();
        let mesh = la.mesh.clone();
        let backend = LabelMockBackend::new(la);
        let r = boolean(&a, &b, BoolOp::Union, &backend).unwrap();
        let attr = r.triangle_attribution();

        // The kept sub-mesh re-indexes verts but preserves triangle geometry.
        // For each output triangle, its centroid must lie on A's face that
        // the attribution names.
        for t in 0..r.num_tris() as u32 {
            let got = attr.lookup(t).expect("full coverage");
            assert_eq!(got.input, InputId::A, "tris are all on solid A's surface");
            let c = centroid(r.as_mesh(), r.as_mesh().tris[t as usize]);
            let expected_face = resolve_face(&a, c);
            assert_eq!(
                got.face, expected_face,
                "tri {t}: attributed face {} != geometric face {}",
                got.face, expected_face
            );
        }
        let _ = mesh; // keep capture explicit
    }

    #[test]
    fn m3_kept_submesh_is_keep_set_count() {
        // Stage 4: the kept sub-mesh must contain exactly keep_set(op) tris.
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B offset on ALL axes — a [0.5,0,0] offset shares the
        // y/z face planes with A (bit-exact coplanar input), which the
        // near-coplanar input gate now rejects BEFORE the (mock) backend.
        let b = cube_brep([0.5, 0.3, 0.4]);
        let la = arrangement_a_cube_shell();
        let expected_kept = la.keep_set(BoolOp::Union).len();
        let backend = LabelMockBackend::new(la);
        let r = boolean(&a, &b, BoolOp::Union, &backend).unwrap();
        assert_eq!(
            r.num_tris(),
            expected_kept,
            "output mesh tri count must equal keep_set(Union) count"
        );
    }

    // ----- Group A.2: F2 / F3 error cases (P9: loud, never None) -----

    #[test]
    fn m3_coplanar_surface_len_two_errors_f2() {
        // F2: a kept tri whose surface label names BOTH solids (coplanar
        // overlap, len==2) → FaceResolutionFailed (out of scope, M8).
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B must NOT be input-coplanar with A (the gate fires
        // first, before the backend); the F2 condition under test is the
        // ARRANGEMENT-level multi-solid surface label, which the mock
        // fabricates below regardless of the input geometry.
        let b = cube_brep([0.5, 0.3, 0.4]);
        let verts = vec![p(0.0, 0.0, 0.0), p(0.5, 0.0, 0.0), p(0.0, 0.5, 0.0)];
        let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
        let la = LabeledArrangement {
            mesh,
            // surface names BOTH A and B (coplanar multi-solid) — F2.
            surface: vec![vec![LaInputId(0), LaInputId(1)]],
            inside: vec![vec![false, false]], // kept by Union
            patch: vec![0],
            source: Vec::new(),
            num_inputs: 2,
        };
        let backend = LabelMockBackend::new(la);
        match boolean(&a, &b, BoolOp::Union, &backend) {
            Err(YangError::FaceResolutionFailed { tri }) => {
                assert_eq!(tri, 0, "F2 should name the offending tri index");
            }
            other => panic!("expected FaceResolutionFailed (F2), got {other:?}"),
        }
    }

    #[test]
    fn m3_centroid_off_all_planes_errors_f3() {
        // F3: a kept tri on solid A's surface whose centroid lies on NO
        // A-face plane → FaceResolutionFailed (loud, never None).
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B offset on ALL axes — a [0.5,0,0] offset shares the
        // y/z face planes with A (bit-exact coplanar input), which the
        // near-coplanar input gate now rejects BEFORE the (mock) backend.
        let b = cube_brep([0.5, 0.3, 0.4]);
        // Triangle floating at z=0.5 (interior; off every cube face plane).
        let verts = vec![p(0.25, 0.25, 0.5), p(0.5, 0.25, 0.5), p(0.25, 0.5, 0.5)];
        let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
        let la = LabeledArrangement {
            mesh,
            surface: vec![vec![LaInputId(0)]], // claims solid A's surface
            inside: vec![vec![false, false]],  // kept by Union
            patch: vec![0],
            source: Vec::new(),
            num_inputs: 2,
        };
        let backend = LabelMockBackend::new(la);
        match boolean(&a, &b, BoolOp::Union, &backend) {
            Err(YangError::FaceResolutionFailed { tri }) => {
                assert_eq!(tri, 0, "F3 should name the offending tri index");
            }
            other => panic!("expected FaceResolutionFailed (F3), got {other:?}"),
        }
    }

    /// N4 retirement (task #53, spec `specs/n4_retire_stage6_fallback.md`):
    /// on a provenance-CARRYING arrangement, a triangle whose provenance
    /// MISSES must fail loudly — never a silent geometric guess. The
    /// triangle lies ON A's bottom face plane, so the old geometric
    /// fallback would happily (mis)attribute it; the miss is a
    /// `NoSourceEntry` (its source names only input B while the surface
    /// label says A).
    #[test]
    fn n4_provenance_miss_errors_loudly() {
        let a = cube_brep([0.0, 0.0, 0.0]);
        let b = cube_brep([0.5, 0.3, 0.4]);
        let verts = vec![p(0.1, 0.1, 0.0), p(0.4, 0.1, 0.0), p(0.1, 0.4, 0.0)];
        let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
        let la = LabeledArrangement {
            mesh,
            surface: vec![vec![LaInputId(0)]], // claims solid A's surface…
            inside: vec![vec![false, false]],  // kept by Union
            patch: vec![0],
            // …but provenance names only input B: a NoSourceEntry miss.
            source: vec![vec![(LaInputId(1), 0)]],
            num_inputs: 2,
        };
        let backend = LabelMockBackend::new(la);
        match boolean(&a, &b, BoolOp::Union, &backend) {
            Err(YangError::FaceResolutionFailed { tri }) => {
                assert_eq!(tri, 0, "the miss should name the offending tri");
            }
            other => panic!("provenance miss must be loud (FaceResolutionFailed), got {other:?}"),
        }
    }

    /// N4 retirement: the `NoMap` miss reason (parent index beyond the
    /// input's `tri_face` map) is equally loud.
    #[test]
    fn n4_provenance_out_of_range_parent_errors_loudly() {
        let a = cube_brep([0.0, 0.0, 0.0]);
        let b = cube_brep([0.5, 0.3, 0.4]);
        let verts = vec![p(0.1, 0.1, 0.0), p(0.4, 0.1, 0.0), p(0.1, 0.4, 0.0)];
        let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
        let la = LabeledArrangement {
            mesh,
            surface: vec![vec![LaInputId(0)]],
            inside: vec![vec![false, false]],
            patch: vec![0],
            // Parent index far beyond A's 12-triangle Stage-1 map: NoMap.
            source: vec![vec![(LaInputId(0), 9999)]],
            num_inputs: 2,
        };
        let backend = LabelMockBackend::new(la);
        match boolean(&a, &b, BoolOp::Union, &backend) {
            Err(YangError::FaceResolutionFailed { tri }) => {
                assert_eq!(tri, 0, "the miss should name the offending tri");
            }
            other => panic!("provenance miss must be loud (FaceResolutionFailed), got {other:?}"),
        }
    }

    // ----- Group C: M4 differential oracle (real label vs substitute) -----

    #[test]
    fn m4_real_label_and_substitute_agree_on_pure_a() {
        // The (now test-only) substitute attribution and the real-label
        // path must agree on a pure-A fixture. Disagreement localizes a
        // label-path bug. The substitute is exercised here via the M4
        // test-only helpers (`match_with_input`/`face_candidates`/
        // `majority_vote`), which the Implementer relocates into the test
        // module. If those are not yet callable, this is a compile RED.
        let a = cube_brep([0.0, 0.0, 0.0]);
        // PR-YR24: B offset on ALL axes — a [0.5,0,0] offset shares the
        // y/z face planes with A (bit-exact coplanar input), which the
        // near-coplanar input gate now rejects BEFORE the (mock) backend.
        let b = cube_brep([0.5, 0.3, 0.4]);
        let la = arrangement_a_cube_shell();
        let mesh = la.mesh.clone();
        let backend = LabelMockBackend::new(la);

        // Real-label path:
        let r = boolean(&a, &b, BoolOp::Union, &backend).unwrap();
        let attr = r.triangle_attribution();

        // Substitute path (vertex provenance + majority vote) over the
        // SAME kept sub-mesh:
        for t in 0..r.num_tris() {
            let tri = r.as_mesh().tris[t];
            let mut inputs = [None; 3];
            let mut sources = [TessellationSource::Unknown; 3];
            for (k, &vi) in tri.iter().enumerate() {
                let target = r.as_mesh().verts[vi as usize];
                let (inp, src) = match_with_input(&a, &b, target);
                inputs[k] = inp;
                sources[k] = src;
            }
            let sets = [
                face_candidates(inputs[0], sources[0], &a, &b),
                face_candidates(inputs[1], sources[1], &a, &b),
                face_candidates(inputs[2], sources[2], &a, &b),
            ];
            let substitute = majority_vote(&sets);
            let real = attr.lookup(t as u32);
            assert_eq!(
                real, substitute,
                "M4 differential: real-label tri {t} attribution {real:?} \
                 disagrees with substitute {substitute:?}"
            );
        }
        let _ = mesh;
    }

    // ───────────────────────────────────────────────────────────────────
    // PR-M8 disc-rim crossing — rim-override Stage-1 unit tests
    // ───────────────────────────────────────────────────────────────────

    /// A z-axis cylinder B-Rep: bottom cap (−z) at `z=base`, top cap (+z) at
    /// `z=base+h`, seam at +x, radius `r`. Two full-circle rims + one seam
    /// segment (mirrors the m8 test fixture).
    fn rt_cylinder(base: f64, h: f64, r: f64) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
        let v0 = Point3::new(r, 0.0, base);
        let v1 = Point3::new(r, 0.0, base + h);
        let verts = vec![BRepVertex { point: v0 }, BRepVertex { point: v1 }];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, base),
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, base + h),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![
            BRepFace {
                surface: Surface::Cylinder {
                    axis_point: Point3::new(0.0, 0.0, base),
                    axis_dir: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
                outer_loop: vec![0, 2, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    d: base,
                },
                outer_loop: vec![0],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    d: -(base + h),
                },
                outer_loop: vec![1],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        (verts, edges, faces)
    }

    /// An EMPTY rim-override map yields byte-identical verts AND tris to the
    /// plain `stage1_tessellate` for a plain cylinder — the uniform-rim path is
    /// 100% untouched.
    #[test]
    fn rim_override_empty_is_byte_identical() {
        let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
        let plain = stage1_tessellate(&verts, &edges, &faces).expect("plain");
        let empty: std::collections::BTreeMap<u32, Vec<Point3>> = std::collections::BTreeMap::new();
        let overridden = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &empty, None)
            .expect("empty");
        assert_eq!(
            plain.verts.len(),
            overridden.verts.len(),
            "empty override must not add verts"
        );
        for (a, b) in plain.verts.iter().zip(&overridden.verts) {
            assert_eq!(a.as_array(), b.as_array(), "verts must be byte-identical");
        }
        assert_eq!(plain.tris, overridden.tris, "tris must be byte-identical");
    }

    /// Inserting a crossing point on BOTH rims (at the same geometric azimuth):
    /// both points appear bit-exactly on the top AND bottom rim rings, and the
    /// resulting cylinder mesh (caps + lateral) stays a closed 2-manifold.
    #[test]
    fn rim_override_inserts_into_both_rims_no_t_junction() {
        let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
        // A point on each rim at azimuth 0.3 rad (NOT a uniform sample): radius
        // 0.5 in the rim's plane.
        let az = 0.3_f64;
        let (s, c) = az.sin_cos();
        let bottom_pt = Point3::new(0.5 * c, 0.5 * s, 0.0);
        let top_pt = Point3::new(0.5 * c, 0.5 * s, 1.0);
        let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        ov.insert(0, vec![bottom_pt]); // bottom rim = circle edge 0
        ov.insert(1, vec![top_pt]); // top rim = circle edge 1
        let t = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &ov, None)
            .expect("dual-rim override");

        // Both inserted points present bit-exactly in the vertex pool.
        let has = |p: Point3| t.verts.iter().any(|q| q.as_array() == p.as_array());
        assert!(has(bottom_pt), "bottom crossing point missing from mesh");
        assert!(has(top_pt), "top crossing point missing from mesh");

        // The mesh stays a closed 2-manifold (every undirected edge shared by
        // exactly two triangles).
        let mut counts: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for tri in &t.tris {
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        assert!(!counts.is_empty());
        assert!(
            counts.values().all(|&c| c == 2),
            "dual-rim override must keep the cylinder a closed 2-manifold"
        );
    }

    /// M-C RED (spec `m8_stage0_band_scale_crossing_verts` §4 E-C1): two
    /// DISTINCT override points whose angular separation is far below the
    /// legacy merge_tol (band-close genuine crossings — the R0088/R0070
    /// twin population) must BOTH be inserted into the rim ring. Silently
    /// keeping only one desynchronizes the ring from the cap override that
    /// carries both points (T-junction holes, the measured M-C class). A
    /// bit-identical duplicate must still be deduplicated (E-C1b).
    #[test]
    fn rim_override_band_close_distinct_points_both_inserted() {
        let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
        let r = 0.5_f64;
        let mk = |az: f64, z: f64| {
            let (s, c) = az.sin_cos();
            Point3::new(r * c, r * s, z)
        };
        // Two on-circle points ~2e-13 rad apart (distinct f64 coordinates,
        // far below uni_step·1e-6), on both rims for lateral balance.
        let (az1, az2) = (0.3_f64, 0.3_f64 + 2.0e-13);
        let (b1, b2) = (mk(az1, 0.0), mk(az2, 0.0));
        let (t1, t2) = (mk(az1, 1.0), mk(az2, 1.0));
        assert_ne!(b1.as_array(), b2.as_array(), "twin construction degenerate");
        let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        ov.insert(0, vec![b1, b2]);
        ov.insert(1, vec![t1, t2]);
        let t = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &ov, None)
            .expect("band-close distinct overrides must be accepted");
        for (name, p) in [("b1", b1), ("b2", b2), ("t1", t1), ("t2", t2)] {
            assert!(
                t.verts.iter().any(|q| q.as_array() == p.as_array()),
                "M-C RED — distinct band-close override {name} missing from the \
                 rim ring (silent merge_tol drop, spec §2)"
            );
        }
        // Ring stays a closed 2-manifold with the band-thin segments present.
        let mut counts: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for tri in &t.tris {
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        assert!(
            counts.values().all(|&c| c == 2),
            "band-close override insertion must keep the cylinder closed"
        );

        // E-C1b: a bit-identical duplicate is still dropped (no double vertex).
        // Balanced across both rims (the lateral azimuth-merge expectation).
        let mut dup: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        dup.insert(0, vec![b1, b1]);
        dup.insert(1, vec![t1, t1]);
        let td = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &dup, None)
            .expect("bit-identical duplicate override must be accepted");
        assert_eq!(
            td.verts
                .iter()
                .filter(|q| q.as_array() == t1.as_array())
                .count(),
            1,
            "bit-identical duplicate override must be deduplicated exactly once"
        );
    }

    /// Chained swiss-cheese wall 1 RED (task #62, spec
    /// `m8_holed_disc_coplanar_overlay` §8 increment 5): the azimuth-merge
    /// lateral pairing must be WRAP-AWARE. A RECOVERED B-Rep (boolean output
    /// re-entering a boolean) can carry one rim's seam vertex at azimuth
    /// exactly 0 while the other rim's sits a femto BELOW the +x axis
    /// (y = −ε): `atan2(…).rem_euclid(2π)` maps the latter to 2π−ε, sorting
    /// it LAST instead of FIRST, and the positional `bot[k] ↔ top[k]` pairing
    /// shifts by one slot — the F0086 step-2 wall
    /// (`azimuth-merge rims disagree at index 0 (bottom 0 vs top 0.4488)`).
    /// The two sorted rings are CIRCULAR sequences: pairing must align them
    /// by cyclic shift, not by absolute sort position.
    ///
    /// Fixture: rt-style cylinder whose TOP seam vertex is rotated a femto
    /// below the +x axis (y = −r·5e−16, on-circle within band), with one
    /// same-azimuth override pair on both rims to force the azimuth-merge
    /// path. Oracle: tessellation SUCCEEDS and stays a closed 2-manifold.
    /// RED today: MalformedTopology "rims disagree at index 0".
    #[test]
    fn rim_override_wrap_seam_cyclic_alignment() {
        let r = 0.5_f64;
        let eps_y = -r * 5.0e-16; // top seam vertex a femto BELOW the +x axis
        let v0 = Point3::new(r, 0.0, 0.0);
        let v1 = Point3::new(r, eps_y, 1.0);
        let verts = vec![BRepVertex { point: v0 }, BRepVertex { point: v1 }];
        let edges = vec![
            BRepEdge {
                start: 0,
                end: 0,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 0.0),
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 1,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.0, 0.0, 1.0),
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
            },
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::LineSegment,
            },
        ];
        let faces = vec![
            BRepFace {
                surface: Surface::Cylinder {
                    axis_point: Point3::new(0.0, 0.0, 0.0),
                    axis_dir: Vector3::new(0.0, 0.0, 1.0),
                    radius: r,
                },
                outer_loop: vec![0, 2, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, -1.0),
                    d: 0.0,
                },
                outer_loop: vec![0],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    d: -1.0,
                },
                outer_loop: vec![1],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        // One override pair at the same geometric azimuth on both rims (not
        // near a uniform sample) — forces the azimuth-merge lateral path.
        let az = 0.3_f64;
        let (s, c) = az.sin_cos();
        let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        ov.insert(0, vec![Point3::new(r * c, r * s, 0.0)]);
        ov.insert(1, vec![Point3::new(r * c, r * s, 1.0)]);
        let t = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &ov, None).expect(
            "wrap-seam cylinder must tessellate — the azimuth-merge pairing \
             must align the rings cyclically, not by absolute sort position",
        );
        let mut counts: std::collections::BTreeMap<(u32, u32), u32> =
            std::collections::BTreeMap::new();
        for tri in &t.tris {
            for k in 0..3 {
                let (a, b) = (tri[k], tri[(k + 1) % 3]);
                *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        assert!(
            !counts.is_empty() && counts.values().all(|&c| c == 2),
            "wrap-seam cylinder must stay a closed 2-manifold"
        );
    }

    /// M8 holed-disc increment 3 RED (spec `m8_holed_disc_coplanar_overlay`
    /// §8): ULP-TWIN override points — two distinct points 1 ULP apart in x
    /// whose f64 seam-relative rim angles COLLIDE — must be ring-ordered by
    /// their EXACT angular order on BOTH rims, regardless of the caller's
    /// insertion order, and the lateral strip must pair each bottom twin with
    /// its same-azimuth top partner (no twisted quad). Today the slot sort
    /// falls back to insertion order on the f64 tie, and the two rims' frames
    /// have OPPOSITE orientations, so one rim always comes out mis-ordered →
    /// the cap fan walks U_lo–twinB–twinA–U_hi on one cap (wrong adjacency)
    /// and the wall strip twists (a self-intersecting Stage-0 mesh — the
    /// `annular_cap_under_disc` cherchi `SegmentNotLocatable` wall).
    ///
    /// Oracles (frame-independent, structural):
    /// - on each cap, the uniform sample at the LOWER global azimuth is
    ///   ring-adjacent to the LOWER-azimuth twin (and not to the other);
    /// - the lateral contains BOTH vertical edges (A_bot,A_top), (B_bot,B_top);
    /// - the full mesh stays a closed 2-manifold;
    /// - both insertion orders ([A,B] and [B,A]) yield the same triangle SET.
    #[test]
    fn rim_override_ulp_twins_exact_order_both_rims() {
        let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);

        // Pick the bottom-rim chord whose midpoint has the smallest |x| (near
        // the ±y axis, far from the seam at +x): there the azimuth derivative
        // dθ/dx = |y|/r² is maximal while ULP(θ-offset) is fixed, so a 1-ULP
        // x perturbation moves the angle by far LESS than one ULP of the
        // seam-relative offset → the f64 angles of the twins collide.
        let plain = stage1_tessellate(&verts, &edges, &faces).expect("plain");
        let mut rim0: Vec<(f64, Point3)> = plain
            .sources
            .iter()
            .enumerate()
            .filter_map(|(i, src)| match src {
                TessellationSource::BRepEdge { edge: 0, t } => Some((*t, plain.verts[i])),
                _ => None,
            })
            .collect();
        rim0.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(rim0.len() >= 4, "bottom rim must have >=4 Steiner samples");
        let mut best: Option<([f64; 2], [f64; 2])> = None;
        for w in rim0.windows(2) {
            let (p0, p1) = (w[0].1.as_array(), w[1].1.as_array());
            let mid_x = 0.5 * (p0[0] + p1[0]);
            if best.is_none_or(|(a, b)| mid_x.abs() < 0.5 * (a[0] + b[0]).abs()) {
                best = Some(([p0[0], p0[1]], [p1[0], p1[1]]));
            }
        }
        let (e0, e1) = best.unwrap();
        let mx = 0.5 * (e0[0] + e1[0]);
        let my = 0.5 * (e0[1] + e1[1]);
        // The ULP twins: same y, x one ULP apart (the real Stage-0 twin shape:
        // two sweep-event columns from 1-ULP-different rim-sample x's).
        let xa = mx;
        let xb = f64::from_bits(mx.to_bits() + 1);
        assert_ne!(xa, xb, "twin construction degenerate");
        // Exact global-azimuth order: cross(A,B) = xa·my − my·xb = my·(xa−xb),
        // exact in f64 (adjacent-float subtraction is exact). Positive cross
        // means B is CCW of A, i.e. A has the LOWER azimuth.
        let a_first = my * (xa - xb) > 0.0;
        let (x_lo, x_hi) = if a_first { (xa, xb) } else { (xb, xa) };
        let tw_lo_b = Point3::new(x_lo, my, 0.0); // lower-azimuth twin, bottom
        let tw_hi_b = Point3::new(x_hi, my, 0.0);
        let tw_lo_t = Point3::new(x_lo, my, 1.0); // same azimuths on top rim
        let tw_hi_t = Point3::new(x_hi, my, 1.0);
        // Twin global azimuth (for locating each cap's bracketing uniform
        // samples — the top rim's samples are NOT bit-identical in (x,y) to
        // the bottom's, its frame flips, so each cap is searched on its own).
        let az_of = |x: f64, y: f64| y.atan2(x).rem_euclid(2.0 * std::f64::consts::PI);
        let az_tw = az_of(mx, my);

        let run = |first: Point3, second: Point3, tfirst: Point3, tsecond: Point3| {
            let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> =
                std::collections::BTreeMap::new();
            ov.insert(0, vec![first, second]);
            ov.insert(1, vec![tfirst, tsecond]);
            stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &ov, None)
                .expect("ULP-twin overrides must be accepted")
        };

        let check = |t: &Stage1Tess, tag: &str| {
            let vid = |p: Point3| -> u32 {
                t.verts
                    .iter()
                    .position(|q| q.as_array() == p.as_array())
                    .unwrap_or_else(|| panic!("{tag}: point {p:?} missing from mesh"))
                    as u32
            };
            // The rim-E uniform samples bracketing the twin azimuth (the
            // twins' ring neighbours on that rim).
            let brackets = |edge: u32| -> (u32, u32) {
                let mut lo: Option<(f64, u32)> = None;
                let mut hi: Option<(f64, u32)> = None;
                for (i, src) in t.sources.iter().enumerate() {
                    if !matches!(src, TessellationSource::BRepEdge { edge: e, .. } if *e == edge) {
                        continue;
                    }
                    let a = t.verts[i].as_array();
                    // Skip the inserted twins themselves (also BRepEdge-tagged).
                    if a[1] == my && (a[0] == xa || a[0] == xb) {
                        continue;
                    }
                    let az = az_of(a[0], a[1]);
                    if az < az_tw {
                        if lo.is_none_or(|(b, _)| az > b) {
                            lo = Some((az, i as u32));
                        }
                    } else if hi.is_none_or(|(b, _)| az < b) {
                        hi = Some((az, i as u32));
                    }
                }
                (
                    lo.unwrap_or_else(|| panic!("{tag}: no uniform below twin on rim {edge}"))
                        .1,
                    hi.unwrap_or_else(|| panic!("{tag}: no uniform above twin on rim {edge}"))
                        .1,
                )
            };
            // Undirected edge sets: bottom cap (all z==0), top cap (all z==1),
            // lateral (z-spanning).
            let mut cap_b = std::collections::BTreeSet::new();
            let mut cap_t = std::collections::BTreeSet::new();
            let mut lat = std::collections::BTreeSet::new();
            let mut counts: std::collections::BTreeMap<(u32, u32), u32> =
                std::collections::BTreeMap::new();
            for tri in &t.tris {
                let zs: Vec<f64> = tri
                    .iter()
                    .map(|&v| t.verts[v as usize].as_array()[2])
                    .collect();
                let bucket: &mut std::collections::BTreeSet<(u32, u32)> =
                    if zs.iter().all(|&z| z == 0.0) {
                        &mut cap_b
                    } else if zs.iter().all(|&z| z == 1.0) {
                        &mut cap_t
                    } else {
                        &mut lat
                    };
                for k in 0..3 {
                    let (a, b) = (tri[k], tri[(k + 1) % 3]);
                    let e = (a.min(b), a.max(b));
                    bucket.insert(e);
                    *counts.entry(e).or_insert(0) += 1;
                }
            }
            let e = |a: u32, b: u32| (a.min(b), a.max(b));
            for (cap, lo, hi, edge, z) in [
                (&cap_b, tw_lo_b, tw_hi_b, 0u32, 0.0),
                (&cap_t, tw_lo_t, tw_hi_t, 1u32, 1.0),
            ] {
                let (vlo, vhi) = (vid(lo), vid(hi));
                let (ulo, uhi) = brackets(edge);
                assert!(
                    cap.contains(&e(ulo, vlo)),
                    "{tag}: cap z={z} — lower uniform must be ring-adjacent to \
                     the LOWER-azimuth twin (exact order), edge missing"
                );
                assert!(
                    !cap.contains(&e(ulo, vhi)),
                    "{tag}: cap z={z} — lower uniform adjacent to the HIGHER \
                     twin: ring is in WRONG (insertion/tie) order"
                );
                assert!(
                    cap.contains(&e(uhi, vhi)),
                    "{tag}: cap z={z} — upper uniform must be ring-adjacent to \
                     the HIGHER-azimuth twin, edge missing"
                );
                assert!(
                    !cap.contains(&e(uhi, vlo)),
                    "{tag}: cap z={z} — upper uniform adjacent to the LOWER \
                     twin: ring is in WRONG (insertion/tie) order"
                );
            }
            // Untwisted wall: both same-azimuth vertical edges exist.
            let (blo, bhi) = (vid(tw_lo_b), vid(tw_hi_b));
            let (tlo, thi) = (vid(tw_lo_t), vid(tw_hi_t));
            assert!(
                lat.contains(&e(blo, tlo)),
                "{tag}: lateral misses vertical edge at the lower twin column \
                 (twisted quad — bottom twin paired with the WRONG top twin)"
            );
            assert!(
                lat.contains(&e(bhi, thi)),
                "{tag}: lateral misses vertical edge at the higher twin column \
                 (twisted quad — bottom twin paired with the WRONG top twin)"
            );
            assert!(
                counts.values().all(|&c| c == 2),
                "{tag}: mesh must stay a closed 2-manifold"
            );
            let mut tris: Vec<[[u64; 3]; 3]> = t
                .tris
                .iter()
                .map(|tri| {
                    let mut ps: [[u64; 3]; 3] = [[0; 3]; 3];
                    for (k, &v) in tri.iter().enumerate() {
                        let a = t.verts[v as usize].as_array();
                        ps[k] = [a[0].to_bits(), a[1].to_bits(), a[2].to_bits()];
                    }
                    ps.sort();
                    ps
                })
                .collect();
            tris.sort();
            tris
        };

        // Insertion order 1: exact order (lo, hi). Insertion order 2: reversed.
        // BOTH must produce the exact ring order (the sort may not fall back
        // to insertion order on the f64 angle tie) and the same geometry.
        let t1 = run(tw_lo_b, tw_hi_b, tw_lo_t, tw_hi_t);
        let g1 = check(&t1, "insertion (lo,hi)");
        let t2 = run(tw_hi_b, tw_lo_b, tw_hi_t, tw_lo_t);
        let g2 = check(&t2, "insertion (hi,lo)");
        assert_eq!(
            g1, g2,
            "ring order must be insertion-order independent (exact, not stable-tie)"
        );
    }

    /// A rim-crossing override lies on the tessellated rim POLYGON (a CHORD
    /// between two on-circle samples), so it sits radially INSIDE the analytic
    /// circle by up to the Stage-1 chord sagitta. The override validation must
    /// ACCEPT such a point (it is the same point the cap overlay uses — snapping
    /// it to the circle would mint a T-junction), while still rejecting a point
    /// that is OUTSIDE the circle or inside by MORE than the sagitta (a genuine
    /// off-rim fault). Regression for task #21 (the `is not on the circle`
    /// rejection that masked the same-normal crossing path).
    #[test]
    fn rim_override_accepts_chord_point_rejects_off_rim() {
        let (verts, edges, faces) = rt_cylinder(0.0, 1.0, 0.5);
        let r = 0.5_f64;
        let az = 0.3_f64; // not a uniform sample
        let (s, c) = az.sin_cos();
        // Derive a point GUARANTEED on a chord of the actual tessellated top
        // rim (circle edge 1): the midpoint of two consecutive rim samples — its
        // radial deficit equals the exact Stage-1 chord sagitta for this N.
        let plain = stage1_tessellate(&verts, &edges, &faces).expect("plain");
        let mut rim1: Vec<(f64, Point3)> = plain
            .sources
            .iter()
            .enumerate()
            .filter_map(|(i, src)| match src {
                TessellationSource::BRepEdge { edge: 1, t } => Some((*t, plain.verts[i])),
                _ => None,
            })
            .collect();
        rim1.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(rim1.len() >= 2, "top rim must have >=2 samples");
        let (p0, p1) = (rim1[0].1.as_array(), rim1[1].1.as_array());
        let mx = 0.5 * (p0[0] + p1[0]);
        let my = 0.5 * (p0[1] + p1[1]);
        let top_chord = Point3::new(mx, my, 1.0);
        // Same (x,y) on the BOTTOM rim plane (z=0): same global azimuth + same
        // radial deficit (the cylinder is axis-aligned), so inserting on BOTH
        // rims keeps the lateral azimuth-merge balanced.
        let bot_chord = Point3::new(mx, my, 0.0);
        let single = |e: u32, p: Point3| {
            let mut ov: std::collections::BTreeMap<u32, Vec<Point3>> =
                std::collections::BTreeMap::new();
            ov.insert(e, vec![p]);
            ov
        };

        // (1) chord point (radial deficit = chord sagitta) → ACCEPTED + present.
        let mut both: std::collections::BTreeMap<u32, Vec<Point3>> =
            std::collections::BTreeMap::new();
        both.insert(0, vec![bot_chord]);
        both.insert(1, vec![top_chord]);
        let t = stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, &both, None)
            .expect("a rim point on the tessellated chord must be accepted");
        assert!(
            t.verts.iter().any(|q| q.as_array() == top_chord.as_array()),
            "accepted chord point must appear in the mesh"
        );

        // (2) far INSIDE the circle (deficit 0.1 ≫ sagitta) → loud reject
        // (the off-rim validation fires before the lateral merge).
        let too_deep = Point3::new((r - 0.1) * c, (r - 0.1) * s, 1.0);
        assert!(
            matches!(
                stage1_tessellate_with_rim_overrides(
                    &verts,
                    &edges,
                    &faces,
                    &single(1, too_deep),
                    None
                ),
                Err(YangError::MalformedTopology(_))
            ),
            "a point far inside the rim circle must be rejected (off-rim fault)"
        );

        // (3) OUTSIDE the circle → loud reject.
        let outside = Point3::new((r + 0.01) * c, (r + 0.01) * s, 1.0);
        assert!(
            matches!(
                stage1_tessellate_with_rim_overrides(
                    &verts,
                    &edges,
                    &faces,
                    &single(1, outside),
                    None
                ),
                Err(YangError::MalformedTopology(_))
            ),
            "a point outside the rim circle must be rejected"
        );
    }

    // ── M8-intra: exactly-negated intra-solid coplanar exclusion ────────────
    // Spec `specs/m8_intra_opposite_plane_canonicalization.md` (FIP Phase 2,
    // RED). `scan_near_coplanar` is `pub(crate)`, so these unit tests reach it
    // directly.

    /// A minimal planar `BRepFace` with a valid CCW square loop in one plane,
    /// so `BRep::new`'s Stage-1 tessellation accepts it while `scan` reads the
    /// DECLARED `(normal, d)`.
    fn m8_intra_square_a() -> BRep {
        // Two coplanar squares (z = 3) with EXACTLY-negated plane values — a
        // stepped solid's shared plane carrying opposite outward normals. The
        // negation is value-exact AND exercises 0.0 == -0.0 in the normal's x/y
        // components (spec B6 / §6): F0 = ((0.0, 0.0, 1.0), -3.0),
        // F1 = ((-0.0, -0.0, -1.0), 3.0).
        let verts = vec![
            // F0 corners (CCW viewed from +z).
            BRepVertex {
                point: Point3::new(0.0, 0.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(2.0, 0.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(2.0, 2.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(0.0, 2.0, 3.0),
            },
            // F1 corners (same coords; wound CCW viewed from −z).
            BRepVertex {
                point: Point3::new(0.0, 0.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(2.0, 0.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(2.0, 2.0, 3.0),
            },
            BRepVertex {
                point: Point3::new(0.0, 2.0, 3.0),
            },
        ];
        let seg = |s: u32, e: u32| BRepEdge {
            start: s,
            end: e,
            curve: Curve::LineSegment,
        };
        let edges = vec![
            seg(0, 1),
            seg(1, 2),
            seg(2, 3),
            seg(3, 0), // F0 (+z winding)
            seg(4, 7),
            seg(7, 6),
            seg(6, 5),
            seg(5, 4), // F1 (−z winding)
        ];
        let faces = vec![
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, 0.0, 1.0),
                    d: -3.0,
                },
                outer_loop: vec![0, 1, 2, 3],
                inner_loops: Vec::new(),
                reversed: false,
            },
            BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(-0.0, -0.0, -1.0),
                    d: 3.0,
                },
                outer_loop: vec![4, 5, 6, 7],
                inner_loops: Vec::new(),
                reversed: false,
            },
        ];
        BRep::new(verts, edges, faces).expect("intra-A BRep::new")
    }

    /// Solid B: a single tilted triangle whose AABB overlaps solid A's face
    /// region (x,y ∈ [0.5,1.5], z ∈ [2.5,3.5]) but shares NO plane with A — the
    /// "other operand reaches the shared-plane region" contact condition the
    /// intra gate keys on.
    fn m8_intra_overlapping_b() -> BRep {
        let verts = vec![
            BRepVertex {
                point: Point3::new(0.5, 0.5, 2.5),
            },
            BRepVertex {
                point: Point3::new(1.5, 0.5, 2.5),
            },
            BRepVertex {
                point: Point3::new(1.0, 1.5, 3.5),
            },
        ];
        let seg = |s: u32, e: u32| BRepEdge {
            start: s,
            end: e,
            curve: Curve::LineSegment,
        };
        let edges = vec![seg(0, 1), seg(1, 2), seg(2, 0)];
        // Tilted plane normal = (v1−v0)×(v2−v0), un-normalized is fine (scan
        // normalizes); it is not parallel to z, so no coplanar cross pair.
        let faces = vec![BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, -1.0, 1.0),
                d: -2.0,
            },
            outer_loop: vec![0, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        }];
        BRep::new(verts, edges, faces).expect("intra-B BRep::new")
    }

    /// Spec B6 (RED): an intra-solid pair on EXACTLY-negated planes (two
    /// orientations of ONE plane) is benign and must NOT flag the intra gate,
    /// even though the other solid overlaps the region.
    ///
    /// RED today: the two faces' raw bits differ (n vs −n, d vs −d, and
    /// 0.0 vs −0.0), so the bit-identity exclusion does not fire and the
    /// near-coplanar band flags them → `scan.intra == Some(..)`.
    #[test]
    fn intra_exactly_negated_pair_is_excluded() {
        let a = m8_intra_square_a();
        let b = m8_intra_overlapping_b();
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_none(),
            "exactly-negated intra pair must be benign (B6), got {:?}",
            scan.intra
        );
    }

    /// Spec B7 (guard): a near-but-NOT-exactly-negated intra pair (one normal
    /// component drifted 1 ULP from exact negation) is the loud residue and
    /// MUST still flag. Passes today; pins that the B6 exclusion is exact-only.
    #[test]
    fn intra_one_ulp_off_negation_still_walls_guard() {
        let mut a = m8_intra_square_a();
        // Drift F1's z-normal component 1 ULP off exact negation.
        {
            let faces = a.faces();
            let Surface::Plane { normal, d } = faces[1].surface else {
                panic!("F1 not planar");
            };
            let n = normal.as_array();
            let drifted = f64::from_bits(n[2].to_bits().wrapping_add(1));
            // Rebuild A with the drifted F1 normal (BRep faces are not mutable
            // in place through the accessor).
            let verts = a.vertices().to_vec();
            let edges = a.edges().to_vec();
            let mut new_faces = a.faces().to_vec();
            new_faces[1].surface = Surface::Plane {
                normal: Vector3::new(n[0], n[1], drifted),
                d,
            };
            a = BRep::new(verts, edges, new_faces).expect("drifted intra-A BRep::new");
        }
        let b = m8_intra_overlapping_b();
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_some(),
            "a 1-ULP-off (not exactly negated) intra pair must still wall loud (B7)"
        );
    }

    // ── ADVERSARY (FIP Phase 4, governance/FEATURE_IMPLEMENTATION_PROTOCOL §6) ──
    // Attacks on the exactly-negated intra exclusion in `scan_near_coplanar`.
    // Appended here (not in a new `tests/` file) because `scan_near_coplanar`
    // is `pub(crate)`. Purely additive; touches no existing test. Reuses the
    // `m8_intra_square_a` / `m8_intra_overlapping_b` helpers above.

    /// Rebuild solid A with a chosen F1 (upper-plane) normal/offset so an attack
    /// can inject exact bit patterns the accessor cannot mutate in place.
    fn m8_intra_a_with_f1(normal: Vector3, d: f64) -> BRep {
        let a = m8_intra_square_a();
        let verts = a.vertices().to_vec();
        let edges = a.edges().to_vec();
        let mut faces = a.faces().to_vec();
        faces[1].surface = Surface::Plane { normal, d };
        BRep::new(verts, edges, faces).expect("rebuilt intra-A")
    }

    /// FINDING (test strength). Spec §6 / B6 claim the exclusion uses f64 VALUE
    /// equality "so `0.0 == -0.0` matches — bit compare would not". The existing
    /// `intra_exactly_negated_pair_is_excluded` fixture puts −0.0 on F1's x/y,
    /// but for a −0.0 vs 0.0 pairing a *sign-flip-bit* compare
    /// (`a.to_bits() == b.to_bits() ^ SIGN`) gives the SAME answer as the value
    /// compare — so that test does NOT actually distinguish value from bit and
    /// SURVIVES the sign-flip-bit mutation. This fixture uses +0.0 on BOTH
    /// faces' x/y (0.0 vs 0.0), where value-negation still holds (0.0 == −0.0)
    /// but sign-flip-bit does NOT — a producer that emits +0.0 on both
    /// orientations (a hand-built / file-loaded solid that never ran
    /// `canonicalize_sibling_planes`) is a real input. This is the case that
    /// genuinely KILLS a bit-compare mutation.
    #[test]
    fn adversary_both_positive_zero_negation_excluded() {
        // F0 = ((0,0,1), −3); F1 = ((+0,+0,−1), +3): value-exact negation with
        // +0.0 (NOT −0.0) in x/y on BOTH faces.
        let a = m8_intra_a_with_f1(Vector3::new(0.0, 0.0, -1.0), 3.0);
        let b = m8_intra_overlapping_b();
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_none(),
            "value-exact negation with +0.0/+0.0 must be benign (B6), got {:?}",
            scan.intra
        );
    }

    /// Attack 5 (non-unit normals). Two faces on ONE geometric plane whose raw
    /// stored normals differ in magnitude (n vs −2n) are NOT exact value
    /// negations, so the B6 exclusion must NOT fire; the pair then normalizes to
    /// parallel-opposite-coplanar and — since B reaches the region — walls LOUD.
    /// The documented conservative residue; nothing crashes.
    #[test]
    fn adversary_nonunit_opposite_normals_still_wall() {
        // F1 = ((0,0,−2), 6): plane −2z + 6 = 0 ⇒ z = 3, opposite orientation of
        // F0's z = 3 plane, but stored non-unit.
        let a = m8_intra_a_with_f1(Vector3::new(0.0, 0.0, -2.0), 6.0);
        let b = m8_intra_overlapping_b();
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_some(),
            "non-unit opposite normals must not be excluded (conservative residue)"
        );
    }

    /// Attack 4 (plane through the origin). Both faces carry d = 0.0 and a zero
    /// x/y normal component; F1's normal is the value-negation of F0's. The
    /// value compare (0.0 == −0.0, and 0.0 == −0.0 on d) excludes it.
    #[test]
    fn adversary_plane_through_origin_negation_excluded() {
        // Move both squares to z = 0 so d = 0 on both faces, then negate F1.
        let mut a = m8_intra_square_a();
        {
            let mut verts = a.vertices().to_vec();
            for v in verts.iter_mut() {
                v.point = Point3::new(v.point.x(), v.point.y(), 0.0);
            }
            let edges = a.edges().to_vec();
            let mut faces = a.faces().to_vec();
            faces[0].surface = Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            };
            faces[1].surface = Surface::Plane {
                normal: Vector3::new(-0.0, -0.0, -1.0),
                d: -0.0,
            };
            a = BRep::new(verts, edges, faces).expect("origin-plane intra-A");
        }
        // B straddles z = 0 so its AABB overlaps the shared plane region.
        let b = {
            let verts = vec![
                BRepVertex {
                    point: Point3::new(0.5, 0.5, -0.5),
                },
                BRepVertex {
                    point: Point3::new(1.5, 0.5, -0.5),
                },
                BRepVertex {
                    point: Point3::new(1.0, 1.5, 0.5),
                },
            ];
            let seg = |s: u32, e: u32| BRepEdge {
                start: s,
                end: e,
                curve: Curve::LineSegment,
            };
            let edges = vec![seg(0, 1), seg(1, 2), seg(2, 0)];
            let faces = vec![BRepFace {
                surface: Surface::Plane {
                    normal: Vector3::new(0.0, -1.0, 1.0),
                    d: 0.0,
                },
                outer_loop: vec![0, 1, 2],
                inner_loops: Vec::new(),
                reversed: false,
            }];
            BRep::new(verts, edges, faces).expect("origin-plane B")
        };
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_none(),
            "through-origin value-negation (d = 0.0/−0.0) must be benign (B6), got {:?}",
            scan.intra
        );
    }

    /// Attack (asymmetry). The B6 exclusion is orientation-blind to which face
    /// is listed first: swapping F0/F1 (rep negated first) is still excluded.
    #[test]
    fn adversary_negation_exclusion_is_symmetric() {
        // A with F0 negated instead of F1: F0 = ((−0,−0,−1), 3), F1 = ((0,0,1), −3).
        let a = {
            let base = m8_intra_square_a();
            let verts = base.vertices().to_vec();
            let edges = base.edges().to_vec();
            let mut faces = base.faces().to_vec();
            faces[0].surface = Surface::Plane {
                normal: Vector3::new(-0.0, -0.0, -1.0),
                d: 3.0,
            };
            faces[1].surface = Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -3.0,
            };
            BRep::new(verts, edges, faces).expect("swapped intra-A")
        };
        let b = m8_intra_overlapping_b();
        let scan = scan_near_coplanar(&a, &b);
        assert!(
            scan.intra.is_none(),
            "negation exclusion must be symmetric in face order, got {:?}",
            scan.intra
        );
    }
}
