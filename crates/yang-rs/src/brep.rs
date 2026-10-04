//! B-Rep topology types, tessellation bijection maps, triangle
//! attribution, and the `BRep` container (extracted verbatim from
//! lib.rs — spec `specs/yang_rs_lib_decomposition.md`, increment 3).

use crate::stage1_tessellate_inner_overrides;
use crate::{ellipse_point, hyperbola_point, normalize3, ortho_basis, parabola_point};
use crate::{Curve, Point3, Surface, YangError};
use cherchi_rs::Mesh;

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

/// Is the consecutive loop edge pair `(ei: a→v, ej: v→b)` a backtrack-spike
/// needle: both `LineSegment`, sharing `v` (`ei.end == ej.start`), with `a,v,b`
/// collinear (relative `|d1×d2| ≤ 1e-9·|d1||d2|` ⇔ sinθ ≤ 1e-9, never a real
/// corner) AND reversing direction (`dot(v−a, b−v) < 0`)? See
/// [`BRep::normalized_without_backtrack_spikes`].
fn is_backtrack_spike_pair(
    verts: &[BRepVertex],
    edges: &[BRepEdge],
    protected: &std::collections::HashSet<u32>,
    ei: u32,
    ej: u32,
) -> bool {
    let (e1, e2) = (&edges[ei as usize], &edges[ej as usize]);
    if !matches!(e1.curve, Curve::LineSegment) || !matches!(e2.curve, Curve::LineSegment) {
        return false;
    }
    if e1.end != e2.start {
        return false;
    }
    // NEVER remove a curve junction: the needle vertex `v` must not be an
    // endpoint of any non-`LineSegment` edge (an arc/ellipse start/end). When a
    // shared straight edge carries a ZIGZAG of two near-coincident collinear
    // points — the spurious spike AND a real arc junction — both are backtracks;
    // protecting the arc junction makes every face converge on removing the
    // SAME (spurious) vertex, preserving conformance.
    if protected.contains(&e1.end) {
        return false;
    }
    let a = verts[e1.start as usize].point.as_array();
    let v = verts[e1.end as usize].point.as_array();
    let b = verts[e2.end as usize].point.as_array();
    let d1 = [v[0] - a[0], v[1] - a[1], v[2] - a[2]];
    let d2 = [b[0] - v[0], b[1] - v[1], b[2] - v[2]];
    let cr = [
        d1[1] * d2[2] - d1[2] * d2[1],
        d1[2] * d2[0] - d1[0] * d2[2],
        d1[0] * d2[1] - d1[1] * d2[0],
    ];
    let crm = (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
    let l1 = (d1[0] * d1[0] + d1[1] * d1[1] + d1[2] * d1[2]).sqrt();
    let l2 = (d2[0] * d2[0] + d2[1] * d2[1] + d2[2] * d2[2]).sqrt();
    let dot = d1[0] * d2[0] + d1[1] * d2[1] + d1[2] * d2[2];
    l1 > 0.0 && l2 > 0.0 && crm <= 1e-9 * l1 * l2 && dot < 0.0
}

/// Merge every backtrack-spike edge pair in one face loop into a single
/// `LineSegment` (appending the merged edge to `edges`), iterating to a
/// fixpoint. Sets `*changed` when any merge happens. See
/// [`BRep::normalized_without_backtrack_spikes`].
fn clean_spike_loop(
    verts: &[BRepVertex],
    edges: &mut Vec<BRepEdge>,
    protected: &std::collections::HashSet<u32>,
    lp: &mut Vec<u32>,
    changed: &mut bool,
) {
    'restart: loop {
        let n = lp.len();
        if n < 2 {
            return;
        }
        for k in 0..n {
            let (ei, ej) = (lp[k], lp[(k + 1) % n]);
            if is_backtrack_spike_pair(verts, edges, protected, ei, ej) {
                let a = edges[ei as usize].start;
                let b = edges[ej as usize].end;
                let new_idx = edges.len() as u32;
                edges.push(BRepEdge {
                    start: a,
                    end: b,
                    curve: Curve::LineSegment,
                });
                if k + 1 < n {
                    lp[k] = new_idx;
                    lp.remove(k + 1);
                } else {
                    // The spike pair wraps (last edge, first edge): the merged
                    // edge takes slot 0, the wrapping last slot is dropped.
                    lp[0] = new_idx;
                    lp.remove(n - 1);
                }
                *changed = true;
                continue 'restart;
            }
        }
        return;
    }
}

/// The exact tangent DIRECTION of a `Curve::SurfacePair` at a point that lies
/// on it: `n̂_a × n̂_b`, the cross product of the two defining surfaces' unit
/// normals. The curve is the transversal intersection of two level sets, so
/// its tangent is orthogonal to both gradients; `|n̂_a × n̂_b|² = sin²θ` between
/// the normals is exactly the `det` that
/// [`crate::stage4_relocate::relocate_onto_implicit_pair`] already uses as its
/// transversality measure, with the same `MIN_FEATURE_SIZE²` rank floor — no
/// new constant is introduced here.
///
/// `None` (the caller then FAILS CLOSED, changing nothing) for a non-pair
/// curve, for a point on a defining surface's axis (no normal), and at a
/// TANGENCY of the two surfaces (parallel normals), where the intersection is
/// not a transversal curve and has no single tangent.
fn surface_pair_tangent(curve: &Curve, p: [f64; 3]) -> Option<[f64; 3]> {
    let Curve::SurfacePair { a, b } = *curve else {
        return None;
    };
    let (_, na) = crate::stage4_relocate::surface_distance_and_normal(a, p)?;
    let (_, nb) = crate::stage4_relocate::surface_distance_and_normal(b, p)?;
    let t = [
        na[1] * nb[2] - na[2] * nb[1],
        na[2] * nb[0] - na[0] * nb[2],
        na[0] * nb[1] - na[1] * nb[0],
    ];
    let l2 = t[0] * t[0] + t[1] * t[1] + t[2] * t[2];
    let rank_eps = cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE;
    if !(l2.is_finite() && l2 > rank_eps) {
        return None;
    }
    let l = l2.sqrt();
    Some([t[0] / l, t[1] / l, t[2] / l])
}

/// A chord may order its endpoint along the curve only when its TANGENTIAL
/// component dominates: `|d·T| ≥ CURVE_BACKTRACK_MIN_COS · |d|`. This is a
/// fail-closed decisiveness floor, never an acceptance band — an arc that
/// sweeps far enough for its chord to turn away from the shared vertex's
/// tangent derives NOTHING and the loud downstream wall stands. Measured on
/// P0017's spur: 0.998741 and 0.995439.
const CURVE_BACKTRACK_MIN_COS: f64 = 0.5;

/// Is the consecutive loop edge pair `(ei: a→v, ej: v→b)` a CURVED
/// backtrack-spike — the [`is_backtrack_spike_pair`] twin on an exact
/// intersection curve (deviation **N76**, P0017)?
///
/// The straight rule has to MEASURE collinearity, because
/// `Curve::LineSegment`'s `PartialEq` is kind-only. A `Curve::SurfacePair`
/// compares both defining surfaces, so `e1.curve == e2.curve` already means
/// the two edges lie on ONE curve, and the only remaining question is which
/// way each leaves the shared vertex `v`. On a smooth curve through `v` there
/// are exactly two tangent directions: two arcs leaving `v` either take
/// OPPOSITE ones — a plain split of one boundary, kept untouched — or the SAME
/// one, in which case one arc COVERS the other and the excursion
/// `a → v → (back over a) → b` is a zero-width spur. The discriminant is the
/// sign of `((v−a)·T) · ((b−v)·T)` for the exact tangent `T` at `v`.
///
/// Fails closed (returns `false`, nothing is rewritten) whenever the tangent
/// is undefined, whenever either chord is not decisively tangential, and when
/// the pair is the WHOLE loop (`e1.start == e2.end`): a loop that is nothing
/// but a spur encloses no area and stays the loud reject it is.
fn is_curve_backtrack_pair(verts: &[BRepVertex], edges: &[BRepEdge], ei: u32, ej: u32) -> bool {
    if ei == ej {
        return false;
    }
    let (e1, e2) = (&edges[ei as usize], &edges[ej as usize]);
    if !matches!(e1.curve, Curve::SurfacePair { .. }) || e1.curve != e2.curve {
        return false;
    }
    if e1.end != e2.start || e1.start == e2.end {
        return false;
    }
    let a = verts[e1.start as usize].point.as_array();
    let v = verts[e1.end as usize].point.as_array();
    let b = verts[e2.end as usize].point.as_array();
    let Some(t) = surface_pair_tangent(&e1.curve, v) else {
        return false;
    };
    let d1 = [v[0] - a[0], v[1] - a[1], v[2] - a[2]];
    let d2 = [b[0] - v[0], b[1] - v[1], b[2] - v[2]];
    let l1 = (d1[0] * d1[0] + d1[1] * d1[1] + d1[2] * d1[2]).sqrt();
    let l2 = (d2[0] * d2[0] + d2[1] * d2[1] + d2[2] * d2[2]).sqrt();
    if !(l1 > 0.0 && l2 > 0.0) {
        return false;
    }
    let p1 = d1[0] * t[0] + d1[1] * t[1] + d1[2] * t[2];
    let p2 = d2[0] * t[0] + d2[1] * t[1] + d2[2] * t[2];
    if p1.abs() < CURVE_BACKTRACK_MIN_COS * l1 || p2.abs() < CURVE_BACKTRACK_MIN_COS * l2 {
        return false;
    }
    p1 * p2 < 0.0
}

/// Merge every curved backtrack-spike pair in one face loop into a single edge
/// on the SAME curve (appending it to `edges`), iterating to a fixpoint.
/// Mirrors [`clean_spike_loop`]; `*fires` counts the merges. See
/// [`BRep::normalize_output_curve_backtracks`].
fn clean_curve_backtrack_loop(
    verts: &[BRepVertex],
    edges: &mut Vec<BRepEdge>,
    lp: &mut Vec<u32>,
    fires: &mut usize,
) {
    'restart: loop {
        let n = lp.len();
        if n < 2 {
            return;
        }
        for k in 0..n {
            let (ei, ej) = (lp[k], lp[(k + 1) % n]);
            if is_curve_backtrack_pair(verts, edges, ei, ej) {
                let start = edges[ei as usize].start;
                let end = edges[ej as usize].end;
                let curve = edges[ei as usize].curve;
                let new_idx = edges.len() as u32;
                edges.push(BRepEdge { start, end, curve });
                if k + 1 < n {
                    lp[k] = new_idx;
                    lp.remove(k + 1);
                } else {
                    // The spike pair wraps (last edge, first edge): the merged
                    // edge takes slot 0, the wrapping last slot is dropped.
                    lp[0] = new_idx;
                    lp.remove(n - 1);
                }
                *fires += 1;
                continue 'restart;
            }
        }
        return;
    }
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
pub const MATCH_TOLERANCE: f64 = cad_primitives::TAU_EVAL;

/// Per-mesh-vertex bijection to B-Rep features. Established by Stage 1.
#[derive(Clone, Debug, PartialEq)]
pub struct TessellationMap {
    pub(crate) sources: Vec<TessellationSource>,
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
    pub(crate) attributions: Vec<Option<TriangleAttribution>>,
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
    pub(crate) vertices: Vec<BRepVertex>,
    pub(crate) edges: Vec<BRepEdge>,
    pub(crate) faces: Vec<BRepFace>,
    pub(crate) mesh: Mesh,
    pub(crate) tessellation: TessellationMap,
    pub(crate) triangle_attribution: TriangleAttributionMap,
    /// PR-KV13 F2: per-output-FACE attribution, parallel to `faces` — the
    /// `(input, face)` each output face descends from. Populated by
    /// `boolean()`; empty for `new`/`from_mesh` (no boolean lineage).
    pub(crate) face_attribution: Vec<TriangleAttribution>,
    /// N4 (1b): per Stage-1 mesh triangle (parallel to `mesh.tris`), the index
    /// of the B-Rep `face` that produced it. This is the inverse of the Stage-1
    /// `face_tri_ranges`; it lets `boolean()` attribute a kept arrangement
    /// triangle to its B-Rep face DIRECTLY from cherchi's per-triangle
    /// provenance (`LabeledArrangement.source`), replacing geometric
    /// centroid-proximity (deviation N4). Populated by `BRep::new`; EMPTY for
    /// `from_mesh` and boolean-output BReps (no Stage-1 face lineage) — those
    /// fall back to the geometric attribution path.
    pub(crate) tri_face: Vec<u32>,
    /// Case-IV phantom guard (spec `yang_case_iv_phantom_guard`): the forced
    /// minimum rim segment count this B-Rep was (re)tessellated at. Stage-0's
    /// internal re-tessellations (`disc_rim_ring`, `build_stage0_mesh`, split
    /// re-triangulation) MUST honor it so their rims stay conformal with
    /// `as_mesh()`. `None` = the solid's own Stage-1 chord bound (the default).
    pub(crate) forced_rim_n: Option<usize>,
    /// STANDING Stage-1 rim samples (spec `yang_433_tangent_point_mesh_update`
    /// §12): per full-circle rim edge, exact extra ring samples every
    /// re-tessellation of this B-Rep from topology must carry — the
    /// `forced_rim_n` precedent for POINTS instead of a density. Populated by
    /// [`Self::rebuilt_with_rim_overrides`] / [`Self::rebuilt_with_all_overrides`]
    /// (the map they inserted, composed with any standing samples already in
    /// force) and honored by Stage 0's internal re-tessellations
    /// (`disc_rim_ring`, the annular / mixed ring readers, the coincident-
    /// cylinder build, `build_stage0_mesh`) and by the §4.5.2 / phantom-guard
    /// rebuilds — so a §4.3.3 mint made BEFORE Stage 0 (the generator ruling
    /// of an internally tangent cylinder pair whose caps are coplanar, C0043)
    /// is part of the rings Stage 0 classifies and emits, not lost at the
    /// next `from_topology`. Empty for `new` / `from_mesh`; an empty map is
    /// the Stage-1 byte-identical identity.
    pub(crate) standing_rim: std::collections::BTreeMap<u32, Vec<Point3>>,
    /// STANDING Stage-1 face-interior points (spec
    /// `yang_433_tangent_point_mesh_update` §13.2): per face index, exact
    /// interior Steiner points every re-tessellation of this B-Rep from
    /// topology must carry — the `standing_rim` analog for the face
    /// channel. Populated by [`Self::rebuilt_with_all_overrides`] /
    /// [`Self::rebuilt_with_overrides_at_least`] (composed bit-deduped with
    /// what already stands) and honored by every `from_topology*` rebuild
    /// and by Stage 0's emitted-mesh builds (`build_stage0_mesh`, the
    /// coincident-cylinder build). Its customer is the oblique-frame ruling
    /// splice: the overlap span's endpoints minted into the OTHER operand's
    /// lateral ON its ruling, so the shared segment is one bit-identical
    /// edge in both meshes. Empty for `new` / `from_mesh`; an empty map is
    /// the Stage-1 byte-identical identity.
    pub(crate) standing_face: std::collections::BTreeMap<u32, Vec<Point3>>,
    /// STANDING Stage-1 face-interior CONSTRAINT segments (spec
    /// `yang_455_edge_in_plane_conformity.md` arm 2): per face index, the
    /// segments every re-tessellation of this B-Rep from topology must carry
    /// as EDGES of that face's CDT — the `standing_face` analog for the
    /// partner sub-segment of an edge lying in the face. Populated by
    /// [`Self::rebuilt_with_all_overrides_and_constraints`]; honored by every
    /// `from_topology*` rebuild. Empty for `new` / `from_mesh`; an empty map is
    /// the Stage-1 byte-identical identity.
    pub(crate) standing_face_constraints: crate::stage1_tessellate::FaceConstraints,
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
        Self::from_topology_with_rim_overrides(
            verts,
            edges,
            faces,
            min_n_seg,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            &crate::stage1_tessellate::FaceConstraints::new(),
        )
    }

    /// Increment-2 constructor body (spec `yang_rim_junction_insertion`):
    /// [`from_topology`] plus per-rim-edge exact junction points inserted
    /// as extra Stage-1 rim samples. An empty map is byte-identical to
    /// [`from_topology`] (the Stage-1 empty-override identity).
    fn from_topology_with_rim_overrides(
        verts: Vec<BRepVertex>,
        edges: Vec<BRepEdge>,
        faces: Vec<BRepFace>,
        min_n_seg: Option<usize>,
        rim_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
        face_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
        face_constraints: &crate::stage1_tessellate::FaceConstraints,
    ) -> Result<Self, YangError> {
        let tess = crate::stage1_tessellate_with_standing_overrides(
            &verts,
            &edges,
            &faces,
            rim_overrides,
            face_overrides,
            face_constraints,
            min_n_seg,
        )?;
        let mut out = Self::from_topology_and_tess(verts, edges, faces, min_n_seg, tess)?;
        out.standing_rim = rim_overrides.clone();
        out.standing_face = face_overrides.clone();
        out.standing_face_constraints = face_constraints.clone();
        Ok(out)
    }

    /// The standing Stage-1 face constraint segments (see the field docs).
    pub(crate) fn standing_face_constraints(&self) -> &crate::stage1_tessellate::FaceConstraints {
        &self.standing_face_constraints
    }

    /// Edge-in-plane IDENTIFICATION (spec `yang_455_edge_in_plane_conformity`
    /// arm 1): the same topology with `verts` replacing this B-Rep's vertex
    /// positions (same count, same indices), re-tessellated from topology with
    /// every standing override carried on. A vertex moved by a femto amount
    /// onto a partner plane changes only the mesh vertices that read it.
    pub(crate) fn rebuilt_with_vertices(&self, verts: Vec<BRepVertex>) -> Result<Self, YangError> {
        if verts.len() != self.vertices.len() {
            return Err(YangError::MalformedTopology(format!(
                "rebuilt_with_vertices: {} vertices replace {}",
                verts.len(),
                self.vertices.len()
            )));
        }
        Self::from_topology_with_rim_overrides(
            verts,
            self.edges.clone(),
            self.faces.clone(),
            self.forced_rim_n,
            &self.standing_rim,
            &self.standing_face,
            &self.standing_face_constraints,
        )
    }

    /// Stage-0 plane weld (`stage0::plane_weld`): the same topology with
    /// `verts`, `edges` and `faces` replacing this B-Rep's (same counts, same
    /// indices — only positions, curve anchors and face planes change),
    /// re-tessellated from topology with every standing override carried on.
    pub(crate) fn rebuilt_with_geometry(
        &self,
        verts: Vec<BRepVertex>,
        edges: Vec<BRepEdge>,
        faces: Vec<BRepFace>,
    ) -> Result<Self, YangError> {
        if verts.len() != self.vertices.len()
            || edges.len() != self.edges.len()
            || faces.len() != self.faces.len()
        {
            return Err(YangError::MalformedTopology(format!(
                "rebuilt_with_geometry: {}/{}/{} vertices/edges/faces replace {}/{}/{}",
                verts.len(),
                edges.len(),
                faces.len(),
                self.vertices.len(),
                self.edges.len(),
                self.faces.len()
            )));
        }
        Self::from_topology_with_rim_overrides(
            verts,
            edges,
            faces,
            self.forced_rim_n,
            &self.standing_rim,
            &self.standing_face,
            &self.standing_face_constraints,
        )
    }

    /// `extra` composed over this B-Rep's standing rim samples: per rim edge
    /// the standing points first, then every `extra` point not already present
    /// bit-for-bit. Insertion order is immaterial to the rim build (it sorts
    /// by azimuth); the dedup is what makes a re-mint of a standing sample
    /// (the §4.3.3 generator arm minted again by the junction sampler on an
    /// already boosted operand) a no-op instead of a refused duplicate slot.
    fn compose_rim_overrides(
        &self,
        extra: &std::collections::BTreeMap<u32, Vec<Point3>>,
    ) -> std::collections::BTreeMap<u32, Vec<Point3>> {
        merge_rim_points(&self.standing_rim, extra)
    }

    /// The standing Stage-1 rim samples (see the field docs).
    pub(crate) fn standing_rim(&self) -> &std::collections::BTreeMap<u32, Vec<Point3>> {
        &self.standing_rim
    }

    /// The standing Stage-1 face-interior points (see the field docs).
    pub(crate) fn standing_face(&self) -> &std::collections::BTreeMap<u32, Vec<Point3>> {
        &self.standing_face
    }

    /// Shared tail of the `from_topology*` constructors: fold a Stage-1
    /// tessellation into the B-Rep container (mesh, 1:1 tessellation map,
    /// per-triangle owning-face attribution).
    fn from_topology_and_tess(
        verts: Vec<BRepVertex>,
        edges: Vec<BRepEdge>,
        faces: Vec<BRepFace>,
        min_n_seg: Option<usize>,
        tess: crate::stage1_tessellate::Stage1Tess,
    ) -> Result<Self, YangError> {
        // N4: invert face_tri_ranges into a per-triangle owning-face map (1:1
        // with the mesh triangles), so kept arrangement triangles can be
        // attributed via cherchi provenance instead of geometric proximity.
        let mut tri_face = vec![0u32; tess.tris.len()];
        for (fi, range) in tess.face_tri_ranges.iter().enumerate() {
            for ti in range.clone() {
                tri_face[ti] = fi as u32;
            }
        }
        if std::env::var_os("YANG_FACE_CENSUS").is_some() {
            use std::collections::BTreeMap;
            let mut by_kind: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
            for (fi, range) in tess.face_tri_ranges.iter().enumerate() {
                let kind = match faces[fi].surface {
                    Surface::Plane { .. } => "plane",
                    Surface::Cylinder { .. } => "cylinder",
                    Surface::Cone { .. } => "cone",
                    Surface::Sphere { .. } => "sphere",
                    Surface::Torus { .. } => "torus",
                };
                let e = by_kind.entry(kind).or_default();
                e.0 += 1;
                e.1 += range.len();
            }
            let total: usize = by_kind.values().map(|v| v.1).sum();
            eprintln!("[face-census] total_tris={total} faces={}", faces.len());
            for (k, (nf, nt)) in &by_kind {
                eprintln!(
                    "   {k:<9} faces={nf:>5} tris={nt:>8}  ({:.1}% of mesh, {:.0} tris/face)",
                    100.0 * *nt as f64 / total.max(1) as f64,
                    *nt as f64 / (*nf).max(1) as f64
                );
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
            standing_rim: std::collections::BTreeMap::new(),
            standing_face: std::collections::BTreeMap::new(),
            standing_face_constraints: crate::stage1_tessellate::FaceConstraints::new(),
        })
    }

    /// Case-IV phantom guard (spec `yang_case_iv_phantom_guard`): rebuild
    /// this B-Rep's Stage-1 mesh forcing the rim segment count to at least
    /// `n`. Topology (vertices/edges/faces) is unchanged — only the
    /// tessellation density rises, which is always chord-valid (a finer N
    /// only shrinks the sagitta; governance A14.3).
    /// §4.5.2 local refinement (spec `specs/yang_452_local_refinement.md`):
    /// re-derive this B-Rep's Stage-1 discretization at the `d_ε` currently in
    /// force — i.e. inside [`crate::stage1_tessellate::with_refined_chord`].
    ///
    /// TOPOLOGY IS UNTOUCHED: the same vertices, edges and faces, re-meshed at
    /// the finer tolerance, which is exactly what Yang §4.5.2 asks for
    /// ("we increase the mesh resolution of the parametric surfaces associated
    /// with the erroneous regions"). Any phantom-guard rim boost already in
    /// force (`forced_rim_n`) is preserved, so the two mechanisms compose. At
    /// the natural rung this returns a byte-identical rebuild.
    pub(crate) fn retessellated_at_current_d_eps(&self) -> Result<Self, YangError> {
        Self::from_topology_with_rim_overrides(
            self.vertices.clone(),
            self.edges.clone(),
            self.faces.clone(),
            self.forced_rim_n,
            &self.standing_rim,
            &self.standing_face,
            &self.standing_face_constraints,
        )
    }

    pub(crate) fn rebuilt_with_min_rim_segments(&self, n: usize) -> Result<Self, YangError> {
        Self::from_topology_with_rim_overrides(
            self.vertices.clone(),
            self.edges.clone(),
            self.faces.clone(),
            Some(n),
            &self.standing_rim,
            &self.standing_face,
            &self.standing_face_constraints,
        )
    }

    /// Increment-2 (spec `yang_rim_junction_insertion`): rebuild this
    /// B-Rep's Stage-1 mesh with exact rim junction points inserted as
    /// extra rim samples. Preserves an existing phantom-guard boost
    /// (`forced_rim_n`) so the two mechanisms COMPOSE. Topology is
    /// unchanged; inserting a rim sample only shrinks sagittas (A14.3).
    pub(crate) fn rebuilt_with_rim_overrides(
        &self,
        rim_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
    ) -> Result<Self, YangError> {
        self.rebuilt_with_rim_overrides_at_least(rim_overrides, None)
    }

    /// [`Self::rebuilt_with_rim_overrides`] that also raises the forced
    /// minimum rim segment count to `min_n` (§4.3.3 §13: the crossing arm's
    /// density demand, `N ≥ π/α`, so the two cross-section polygons cross
    /// once at the minted ruling). Composes with an existing boost (the
    /// larger wins) and is stored, so every later from-topology rebuild
    /// keeps it.
    pub(crate) fn rebuilt_with_rim_overrides_at_least(
        &self,
        rim_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
        min_n: Option<usize>,
    ) -> Result<Self, YangError> {
        self.rebuilt_with_overrides_at_least(
            rim_overrides,
            &std::collections::BTreeMap::new(),
            min_n,
        )
    }

    /// [`Self::rebuilt_with_rim_overrides_at_least`] plus STANDING
    /// face-interior points (§13.2): both maps compose bit-deduped with
    /// what already stands and are stored, so every later from-topology
    /// rebuild — Stage 0's emitted-mesh builds included — keeps them.
    pub(crate) fn rebuilt_with_overrides_at_least(
        &self,
        rim_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
        face_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
        min_n: Option<usize>,
    ) -> Result<Self, YangError> {
        let forced = match (self.forced_rim_n, min_n) {
            (Some(x), Some(y)) => Some(x.max(y)),
            (x, y) => x.or(y),
        };
        Self::from_topology_with_rim_overrides(
            self.vertices.clone(),
            self.edges.clone(),
            self.faces.clone(),
            forced,
            &self.compose_rim_overrides(rim_overrides),
            &merge_rim_points(&self.standing_face, face_overrides),
            &self.standing_face_constraints,
        )
    }

    /// P3a #146 increment 2 (spec `yang_146_conformal_junction_sampling.md`
    /// §4): rebuild with `LineSegment` edge-polyline + face-interior
    /// junction overrides only — the pre-4d entry, now a thin delegate of
    /// [`Self::rebuilt_with_all_overrides`] kept as the P3a fixture seam
    /// (production calls the composed form directly).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn rebuilt_with_junction_overrides(
        &self,
        edge_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
        face_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
    ) -> Result<Self, YangError> {
        self.rebuilt_with_all_overrides(
            &std::collections::BTreeMap::new(),
            edge_overrides,
            face_overrides,
        )
    }

    /// P3b inc-4d-2 (spec `yang_169_p3b_curved_partner_pierce.md` §7.3
    /// "Composition"): [`Self::rebuilt_with_junction_overrides`] plus exact
    /// junction points inserted into full-circle RIM rings
    /// (`rim_overrides[e]` — the curved-owner half of a rim×planar-face
    /// pierce; the cap CDT and the lateral strip both consume the shared
    /// ring, so the owner's two incident faces are conformal by
    /// construction). All three override kinds compose inside the ONE
    /// Stage-1 tessellation (`stage1_tessellate_inner_overrides` accepts
    /// them together). Empty maps are byte-identical to
    /// [`Self::rebuilt_with_junction_overrides`] (the empty-override
    /// identity). Preserves an existing phantom-guard boost.
    pub(crate) fn rebuilt_with_all_overrides(
        &self,
        rim_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
        edge_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
        face_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
    ) -> Result<Self, YangError> {
        self.rebuilt_with_all_overrides_and_constraints(
            rim_overrides,
            edge_overrides,
            face_overrides,
            &crate::stage1_tessellate::FaceConstraints::new(),
        )
    }

    /// [`Self::rebuilt_with_all_overrides`] plus STANDING face-interior
    /// CONSTRAINT segments (spec `yang_455_edge_in_plane_conformity` arm 2):
    /// each `face_constraints[f]` segment becomes an edge of planar face `f`'s
    /// CDT. Composes bit-deduped with what already stands and is stored, so
    /// every later from-topology rebuild keeps it. An empty map is
    /// byte-identical to [`Self::rebuilt_with_all_overrides`].
    pub(crate) fn rebuilt_with_all_overrides_and_constraints(
        &self,
        rim_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
        edge_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
        face_overrides: &std::collections::BTreeMap<u32, Vec<Point3>>,
        face_constraints: &crate::stage1_tessellate::FaceConstraints,
    ) -> Result<Self, YangError> {
        let rim_overrides = self.compose_rim_overrides(rim_overrides);
        // Face-interior points compose with the STANDING ones the same way
        // (§13.2); the standing set is carried on, the caller's extras
        // bit-deduped in.
        let face_overrides = merge_rim_points(&self.standing_face, face_overrides);
        let face_constraints =
            merge_face_constraints(&self.standing_face_constraints, face_constraints);
        if rim_overrides.is_empty()
            && edge_overrides.is_empty()
            && face_overrides.is_empty()
            && face_constraints.is_empty()
        {
            return Self::from_topology(
                self.vertices.clone(),
                self.edges.clone(),
                self.faces.clone(),
                self.forced_rim_n,
            );
        }
        let tess = stage1_tessellate_inner_overrides(
            &self.vertices,
            &self.edges,
            &self.faces,
            &rim_overrides,
            edge_overrides,
            &face_overrides,
            &face_constraints,
            self.forced_rim_n,
        )
        .map(|(t, _)| t)?;
        let mut out = Self::from_topology_and_tess(
            self.vertices.clone(),
            self.edges.clone(),
            self.faces.clone(),
            self.forced_rim_n,
            tess,
        )?;
        out.standing_rim = rim_overrides;
        out.standing_face = face_overrides;
        out.standing_face_constraints = face_constraints;
        Ok(out)
    }

    /// Normalize away BACKTRACK-SPIKE needle vertices in every face loop.
    ///
    /// A chained boolean output can carry an invalid, self-overlapping boundary
    /// loop: a straight edge `a→v` overshoots a near-tangent arc/line junction
    /// `b` by a tiny real-scale amount, then a second straight edge `v→b`
    /// backtracks to `b` (F0064: face 590 `v1189 → v1190`(x=-0.15811, overshoot)
    /// `→ v1191`(x=-0.15936, the Circle-arc start)). The needle vertex `v`
    /// (`1190`) is degree-2 and purely collinear — it connects only to `a` and
    /// `b`, both by `LineSegment`. Re-tessellating this loop emits a zero-area
    /// triangle `[a, v, b]` that survives the Cherchi arrangement and trips the
    /// Stage-4 watertight gate (`s4-halfedge-pairing`).
    ///
    /// Fix: per face loop, replace any consecutive `LineSegment` pair
    /// `(a→v, v→b)` whose `a,v,b` are collinear AND reverse direction
    /// (`dot(v−a, b−v) < 0`) with a single `LineSegment` `a→b`. Both incident
    /// faces of the shared edge run the identical per-loop rule, so the result
    /// stays boundary-CONFORMAL. **Arc-safe:** the merge requires BOTH edges to
    /// be `LineSegment`, so a genuine arc/line junction (one edge is
    /// `Curve::Circle`) is never touched. **P9/S7-safe:** a collinear backtrack
    /// is a self-overlap that NEVER occurs in a valid simple polygon, so this
    /// can only fire on already-invalid input and cannot alter any
    /// currently-passing tessellation. Normal collinear Steiner points
    /// (`dot ≥ 0`, `v` strictly between `a` and `b`) are preserved.
    ///
    /// Returns `Ok(None)` when no spike was found (the fast path — the caller
    /// keeps the original B-Rep); `Ok(Some(rebuilt))` when at least one loop was
    /// cleaned (the topology is re-tessellated from the merged edges).
    pub(crate) fn normalized_without_backtrack_spikes(&self) -> Result<Option<Self>, YangError> {
        let mut edges = self.edges.clone();
        let mut faces = self.faces.clone();
        let mut changed = false;
        // Curve-junction vertices: endpoints of any non-`LineSegment` edge.
        // These are real topological junctions (arc/ellipse start/end) and are
        // NEVER removed, so a zigzag of two collinear backtrack points resolves
        // to the same survivor on every face that shares the edge.
        let protected: std::collections::HashSet<u32> = self
            .edges
            .iter()
            .filter(|e| !matches!(e.curve, Curve::LineSegment))
            .flat_map(|e| [e.start, e.end])
            .collect();
        for f in faces.iter_mut() {
            let mut outer = std::mem::take(&mut f.outer_loop);
            clean_spike_loop(
                &self.vertices,
                &mut edges,
                &protected,
                &mut outer,
                &mut changed,
            );
            f.outer_loop = outer;
            let n_inner = f.inner_loops.len();
            for j in 0..n_inner {
                let mut inner = std::mem::take(&mut f.inner_loops[j]);
                clean_spike_loop(
                    &self.vertices,
                    &mut edges,
                    &protected,
                    &mut inner,
                    &mut changed,
                );
                f.inner_loops[j] = inner;
            }
        }
        if !changed {
            return Ok(None);
        }
        // The spike merge only rewrites `LineSegment` runs; circle rim edges
        // keep their indices, so the standing rim samples stay addressable.
        Ok(Some(Self::from_topology_with_rim_overrides(
            self.vertices.clone(),
            edges,
            faces,
            self.forced_rim_n,
            &self.standing_rim,
            &self.standing_face,
            &self.standing_face_constraints,
        )?))
    }

    /// Normalize away CURVED backtrack-spike pairs in every face loop, in
    /// place — deviation **N76** (P0017), the
    /// [`Self::normalized_without_backtrack_spikes`] twin for an exact
    /// intersection curve.
    ///
    /// A boolean OUTPUT loop can traverse one `SurfacePair` curve TWICE: two
    /// consecutive edges on the same curve leaving their shared vertex in the
    /// SAME tangent direction, so one covers the other and the excursion is a
    /// zero-width spur pointing OUT of the material. P0017's `FaceId(28)` is
    /// the corpus case — a cone sliver whose loop
    /// `Arc + SurfacePair(node3→node5) + SurfacePair(node5→node0)` has
    /// `h₊(θ₃) = h_arc` exactly, so the second edge retraces the first before
    /// continuing; its nine-point chart ring carries four proper
    /// self-crossings and the render CDT rightly refuses it
    /// (`docs/yang_tail_triage.md`, the 2026-10-03 night section). Both
    /// covering uses — here `FaceId(28)`'s loop and the cut cylinder's — merge
    /// to the same undirected edge, so the output stays boundary-CONFORMAL;
    /// twin pairing in `from_yang_brep` is keyed on
    /// `(vertex pair, CurveKey)`, never on edge index.
    ///
    /// Rewrites LOOPS and the EDGE table only. The mesh, the tessellation map
    /// and both attribution maps are the boolean's own result, not a
    /// tessellation of this topology, and are left untouched — which is why
    /// this is a separate method from the input-side normalizer (that one
    /// re-tessellates from topology). Merged-away edge indices are simply left
    /// unreferenced: every downstream consumer enumerates edges through the
    /// faces' loops.
    ///
    /// Returns the number of merges; **0** (the overwhelming majority — a
    /// valid loop never double-covers a curve) leaves `self` byte-identical.
    pub(crate) fn normalize_output_curve_backtracks(&mut self) -> usize {
        let mut fires = 0usize;
        let verts = std::mem::take(&mut self.vertices);
        let mut edges = std::mem::take(&mut self.edges);
        for f in self.faces.iter_mut() {
            let mut outer = std::mem::take(&mut f.outer_loop);
            clean_curve_backtrack_loop(&verts, &mut edges, &mut outer, &mut fires);
            f.outer_loop = outer;
            let n_inner = f.inner_loops.len();
            for j in 0..n_inner {
                let mut inner = std::mem::take(&mut f.inner_loops[j]);
                clean_curve_backtrack_loop(&verts, &mut edges, &mut inner, &mut fires);
                f.inner_loops[j] = inner;
            }
        }
        self.vertices = verts;
        self.edges = edges;
        fires
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
            standing_rim: std::collections::BTreeMap::new(),
            standing_face: std::collections::BTreeMap::new(),
            standing_face_constraints: crate::stage1_tessellate::FaceConstraints::new(),
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

/// Per rim edge, `base` followed by every `extra` point not already present
/// bit-for-bit (the standing-sample composition; see `BRep::standing_rim`).
pub(crate) fn merge_rim_points(
    base: &std::collections::BTreeMap<u32, Vec<Point3>>,
    extra: &std::collections::BTreeMap<u32, Vec<Point3>>,
) -> std::collections::BTreeMap<u32, Vec<Point3>> {
    let mut out = base.clone();
    for (&e, pts) in extra {
        let slot = out.entry(e).or_default();
        for &p in pts {
            if !slot.contains(&p) {
                slot.push(p);
            }
        }
    }
    out.retain(|_, v| !v.is_empty());
    out
}

/// [`merge_rim_points`] for face constraint segments: the standing segments
/// first, then every `extra` segment not already present bit-for-bit (in
/// either orientation).
pub(crate) fn merge_face_constraints(
    base: &crate::stage1_tessellate::FaceConstraints,
    extra: &crate::stage1_tessellate::FaceConstraints,
) -> crate::stage1_tessellate::FaceConstraints {
    let mut out = base.clone();
    for (&f, segs) in extra {
        let slot = out.entry(f).or_default();
        for &[p, q] in segs {
            let dup = slot
                .iter()
                .any(|&[a, b]| (a == p && b == q) || (a == q && b == p));
            if !dup {
                slot.push([p, q]);
            }
        }
    }
    out.retain(|_, v| !v.is_empty());
    out
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
                    // M5: a procedural surface-pair curve has no closed-form
                    // parameterization. Its Stage-4 endpoints are `BRepVertex`
                    // sources; its Stage-1 INPUT chain Steiner samples (M5 K11
                    // re-entry) carry `BRepEdge { edge, t }` with an ORDINAL
                    // bisection `t` that no evaluator can turn back into the
                    // certified position (the vertex position IS the sample).
                    // Fall back to the endpoint lerp (identical to
                    // `LineSegment`) rather than panic (P9 defensive; no
                    // plausible-wrong analytic point; no production consumer
                    // evaluates this arm — the sphere seam column is the only
                    // `eval_source` caller).
                    Curve::SurfacePair { .. } => {
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

#[cfg(test)]
mod spike_normalization_tests {
    use super::*;
    use std::collections::HashSet;

    fn vtx(x: f64, y: f64) -> BRepVertex {
        BRepVertex {
            point: Point3::new(x, y, 0.47715355249616415),
        }
    }
    fn seg(start: u32, end: u32) -> BRepEdge {
        BRepEdge {
            start,
            end,
            curve: Curve::LineSegment,
        }
    }

    /// F0064 geometry along one boundary line: corner `v0(-0.2757)`, spurious
    /// spike `v1(-0.15811, overshoot)`, arc-junction `v2(-0.15936)`. The pair
    /// `(v0→v1, v1→v2)` is a backtrack spike; `(v1→v2, v2→v0)` is NOT the target
    /// (its needle `v2` is a protected arc junction).
    fn f0064_verts() -> Vec<BRepVertex> {
        vec![
            vtx(-0.2757114308522339, -0.05656023626695868), // 0 corner
            vtx(-0.1581114617736767, -0.05656023626695868), // 1 spurious spike
            vtx(-0.15936068363645936, -0.05656023626695865), // 2 arc junction
        ]
    }

    #[test]
    fn detects_backtrack_spike_pair() {
        let verts = f0064_verts();
        let edges = vec![seg(0, 1), seg(1, 2)];
        let protected = HashSet::new();
        assert!(is_backtrack_spike_pair(&verts, &edges, &protected, 0, 1));
    }

    #[test]
    fn protected_arc_junction_is_not_removed() {
        // Same geometry, but the needle vertex 1 is a protected arc junction:
        // the pair must NOT be flagged (arc/ellipse endpoints are real).
        let verts = f0064_verts();
        let edges = vec![seg(0, 1), seg(1, 2)];
        let protected: HashSet<u32> = [1u32].into_iter().collect();
        assert!(!is_backtrack_spike_pair(&verts, &edges, &protected, 0, 1));
    }

    #[test]
    fn collinear_steiner_point_is_not_a_spike() {
        // v1 strictly BETWEEN v0 and v2 (dot ≥ 0) — a legitimate split point.
        let verts = vec![vtx(0.0, 0.0), vtx(1.0, 0.0), vtx(2.0, 0.0)];
        let edges = vec![seg(0, 1), seg(1, 2)];
        assert!(!is_backtrack_spike_pair(
            &verts,
            &edges,
            &HashSet::new(),
            0,
            1
        ));
    }

    #[test]
    fn reflex_corner_is_not_a_spike() {
        // v1 off the v0→v2 line — a real (non-collinear) corner.
        let verts = vec![vtx(0.0, 0.0), vtx(1.0, 0.5), vtx(2.0, 0.0)];
        let edges = vec![seg(0, 1), seg(1, 2)];
        assert!(!is_backtrack_spike_pair(
            &verts,
            &edges,
            &HashSet::new(),
            0,
            1
        ));
    }

    #[test]
    fn clean_loop_merges_spike_and_preserves_arc_junction() {
        // The F0064 wall zigzag: v3(-0.0566) → v2(arc jn) → v1(spike) → v0(corner).
        // Both v2 and v1 are collinear backtracks, but v2 is protected, so the
        // survivor is unambiguously v1's removal (keep the arc junction v2).
        let mut verts = f0064_verts();
        verts.push(vtx(-0.05656023626695868, -0.05656023626695868)); // 3
        let mut edges = vec![seg(3, 2), seg(2, 1), seg(1, 0)];
        let protected: HashSet<u32> = [2u32].into_iter().collect();
        let mut lp = vec![0u32, 1, 2]; // edge indices
        let mut changed = false;
        clean_spike_loop(&verts, &mut edges, &protected, &mut lp, &mut changed);
        assert!(changed, "the spurious spike v1 must be merged out");
        // After merging (v2→v1, v1→v0) into (v2→v0), the loop walks
        // [seg(3,2), merged(2,0)] and vertex 1 (the spike) is gone; vertex 2
        // (the arc junction) survives on both endpoints of remaining edges.
        let survivors: HashSet<u32> = lp
            .iter()
            .flat_map(|&e| [edges[e as usize].start, edges[e as usize].end])
            .collect();
        assert!(!survivors.contains(&1), "spurious spike v1 removed");
        assert!(survivors.contains(&2), "arc junction v2 preserved");
    }

    #[test]
    fn clean_brep_returns_none() {
        // A plain unit square (no spikes) → the fast path returns None.
        let verts = vec![vtx(0.0, 0.0), vtx(1.0, 0.0), vtx(1.0, 1.0), vtx(0.0, 1.0)];
        let edges = vec![seg(0, 1), seg(1, 2), seg(2, 3), seg(3, 0)];
        let faces = vec![BRepFace {
            surface: Surface::Plane {
                normal: cad_primitives::Vector3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            outer_loop: vec![0, 1, 2, 3],
            inner_loops: vec![],
            reversed: false,
        }];
        let brep = BRep::new(verts, edges, faces).expect("valid square");
        assert!(
            brep.normalized_without_backtrack_spikes()
                .expect("normalize")
                .is_none(),
            "a clean B-Rep must take the no-op fast path"
        );
    }
}

#[cfg(test)]
mod n76_curve_backtrack_tests {
    //! N76 (P0017): a boolean OUTPUT loop that traverses one `SurfacePair`
    //! curve twice carries a zero-width spur. Every figure below is P0017's
    //! own measured geometry (`docs/yang_tail_triage.md`, 2026-10-03 night).

    use super::*;

    /// P0017's cone, `FaceId(28)`'s surface.
    fn p0017_cone() -> Surface {
        Surface::Cone {
            apex: Point3::new(-0.00024969834927697054, 0.000517, -2.7e-5),
            axis_dir: cad_primitives::Vector3::new(1.0, 0.0, 0.0),
            half_angle: 0.8757228702119423,
        }
    }

    /// P0017's cut cylinder.
    fn p0017_cylinder() -> Surface {
        Surface::Cylinder {
            axis_point: Point3::new(
                -0.0004709225775969472,
                -0.0004215112535003616,
                0.0001366432516812921,
            ),
            axis_dir: cad_primitives::Vector3::new(0.0, 1.0, 0.0),
            radius: 0.0008322345964738464,
        }
    }

    fn p0017_pair() -> Curve {
        Curve::SurfacePair {
            a: p0017_cylinder(),
            b: p0017_cone(),
        }
    }

    /// The three loop vertices, in θ order along the shared curve:
    /// `node0` (θ = 0), `node3` (θ = 0.1420979), `node5` (θ = 0.2999498).
    fn p0017_verts() -> Vec<BRepVertex> {
        vec![
            BRepVertex {
                point: Point3::new(
                    0.00011093906025019774,
                    0.00048629930727492094,
                    -0.00045837898149140525,
                ),
            },
            BRepVertex {
                point: Point3::new(
                    0.00011093906025019774,
                    0.000547700692725079,
                    -0.00045837898149140525,
                ),
            },
            BRepVertex {
                point: Point3::new(
                    0.00011567029264257224,
                    0.0006164179483835168,
                    -0.0004537153073050707,
                ),
            },
        ]
    }

    fn pair_edge(start: u32, end: u32) -> BRepEdge {
        BRepEdge {
            start,
            end,
            curve: p0017_pair(),
        }
    }

    #[test]
    fn p0017_double_cover_is_a_curved_backtrack() {
        let verts = p0017_verts();
        // `he 137` (node3 → node5) then `he 138` (node5 → node0): both leave
        // node5 along +T, so 138 retraces 137 before continuing. Measured
        // cosines at node5: 0.998741 and 0.995439.
        let edges = vec![pair_edge(1, 2), pair_edge(2, 0)];
        assert!(
            is_curve_backtrack_pair(&verts, &edges, 0, 1),
            "node3 and node0 both lie on the +T side of node5, so half-edge \
             138's span contains 137's: a zero-width spur"
        );
        let t = surface_pair_tangent(&p0017_pair(), verts[2].point.as_array())
            .expect("the cyl×cone pair is transversal at node5");
        let proj = |i: usize| {
            let p = verts[i].point.as_array();
            let v = verts[2].point.as_array();
            let d = [p[0] - v[0], p[1] - v[1], p[2] - v[2]];
            let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            (d[0] * t[0] + d[1] * t[1] + d[2] * t[2], l)
        };
        let (p3, l3) = proj(1);
        let (p0, l0) = proj(0);
        // The tangent's SIGN is the `n̂_a × n̂_b` order's; only agreement
        // matters. Both endpoints sit on ONE side of node5 along the curve.
        assert!(
            p3 * p0 > 0.0,
            "node3 ({p3:.6e}) and node0 ({p0:.6e}) are on the SAME side of \
             node5 — that is the double cover"
        );
        assert!(
            p3.abs() / l3 > 0.99 && p0.abs() / l0 > 0.99,
            "both chords are decisively tangential ({}, {}) — far above the \
             fail-closed floor {CURVE_BACKTRACK_MIN_COS}",
            p3.abs() / l3,
            p0.abs() / l0
        );
        assert!(
            p3.abs() < p0.abs(),
            "node3 ({:.6e}) is strictly inside the span node5 → node0 \
             ({:.6e})",
            p3.abs(),
            p0.abs()
        );
    }

    #[test]
    fn a_plain_split_of_one_curve_is_not_a_backtrack() {
        let verts = p0017_verts();
        // node0 → node3 → node5 walks the SAME curve monotonically: the two
        // arcs leave the shared node3 in OPPOSITE tangent directions, which is
        // an ordinary split of one boundary and must never be merged.
        let edges = vec![pair_edge(0, 1), pair_edge(1, 2)];
        assert!(
            !is_curve_backtrack_pair(&verts, &edges, 0, 1),
            "a monotone split of one intersection curve is not a spur"
        );
    }

    #[test]
    fn clean_curve_backtrack_loop_merges_the_spur_into_the_lens() {
        let verts = p0017_verts();
        // FaceId(28)'s loop: rim Arc(node0 → node3), then the doubled pair.
        let mut edges = vec![
            BRepEdge {
                start: 0,
                end: 1,
                curve: Curve::Circle {
                    center: Point3::new(0.00011093906025019774, 0.000517, -2.7e-5),
                    normal: cad_primitives::Vector3::new(1.0, 0.0, 0.0),
                    radius: 0.0004324700662547,
                },
            },
            pair_edge(1, 2),
            pair_edge(2, 0),
        ];
        let mut lp = vec![0u32, 1, 2];
        let mut fires = 0usize;
        clean_curve_backtrack_loop(&verts, &mut edges, &mut lp, &mut fires);
        assert_eq!(fires, 1, "exactly one spur merge");
        assert_eq!(lp.len(), 2, "the loop becomes the two-edge LENS");
        assert_eq!(lp[0], 0, "the rim Arc keeps its index");
        let merged = &edges[lp[1] as usize];
        assert_eq!(
            (merged.start, merged.end),
            (1, 0),
            "the merged edge runs node3 → node0 — the lens's lower boundary"
        );
        assert_eq!(
            merged.curve,
            p0017_pair(),
            "the merged edge stays on the SAME pair curve"
        );
        // Idempotent: a clean loop derives nothing.
        let mut again = 0usize;
        clean_curve_backtrack_loop(&verts, &mut edges, &mut lp, &mut again);
        assert_eq!(again, 0, "the normalization is a fixpoint");
    }

    #[test]
    fn a_tangency_and_an_off_tangent_chord_both_fail_closed() {
        let verts = p0017_verts();
        // (a) COAXIAL cylinders: the two normals are parallel everywhere, so
        //     the "intersection" is not a transversal curve and has no single
        //     tangent. No tangent ⇒ no verdict ⇒ no merge.
        let coaxial = Curve::SurfacePair {
            a: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: cad_primitives::Vector3::new(1.0, 0.0, 0.0),
                radius: 1.0,
            },
            b: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: cad_primitives::Vector3::new(1.0, 0.0, 0.0),
                radius: 2.0,
            },
        };
        assert!(
            surface_pair_tangent(&coaxial, [0.0, 1.0, 0.0]).is_none(),
            "parallel normals have no curve tangent"
        );
        let edges = vec![
            BRepEdge {
                start: 1,
                end: 2,
                curve: coaxial,
            },
            BRepEdge {
                start: 2,
                end: 0,
                curve: coaxial,
            },
        ];
        assert!(
            !is_curve_backtrack_pair(&verts, &edges, 0, 1),
            "a tangency derives nothing and the loud wall stands"
        );

        // (b) A chord perpendicular to the tangent at the shared vertex cannot
        //     order its endpoint along the curve — the decisiveness floor.
        let t = surface_pair_tangent(&p0017_pair(), verts[2].point.as_array())
            .expect("transversal at node5");
        let v = verts[2].point.as_array();
        let perp = {
            // Any unit vector ⊥ T.
            let seed = if t[0].abs() < 0.9 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let d = seed[0] * t[0] + seed[1] * t[1] + seed[2] * t[2];
            let w = [seed[0] - d * t[0], seed[1] - d * t[1], seed[2] - d * t[2]];
            let l = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
            [w[0] / l, w[1] / l, w[2] / l]
        };
        let mut skewed = p0017_verts();
        skewed[1] = BRepVertex {
            point: Point3::new(
                v[0] + 1e-4 * perp[0],
                v[1] + 1e-4 * perp[1],
                v[2] + 1e-4 * perp[2],
            ),
        };
        let edges = vec![pair_edge(1, 2), pair_edge(2, 0)];
        assert!(
            !is_curve_backtrack_pair(&skewed, &edges, 0, 1),
            "an off-tangent chord fails closed"
        );
    }

    #[test]
    fn the_whole_loop_being_the_spur_stays_loud() {
        let verts = p0017_verts();
        // `a → v` then `v → a`: a two-edge loop that is nothing but the spur
        // encloses no area. There is no merged edge to make, so it is left to
        // the loud downstream reject rather than normalized to a self-loop.
        let edges = vec![pair_edge(1, 2), pair_edge(2, 1)];
        assert!(
            !is_curve_backtrack_pair(&verts, &edges, 0, 1),
            "a loop that is only a spur is not normalizable"
        );
    }
}
