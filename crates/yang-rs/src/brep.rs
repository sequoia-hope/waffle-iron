//! B-Rep topology types, tessellation bijection maps, triangle
//! attribution, and the `BRep` container (extracted verbatim from
//! lib.rs — spec `specs/yang_rs_lib_decomposition.md`, increment 3).

use crate::stage1_tessellate_with_rim_overrides;
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
    ) -> Result<Self, YangError> {
        let tess =
            stage1_tessellate_with_rim_overrides(&verts, &edges, &faces, rim_overrides, min_n_seg)?;
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
    pub(crate) fn rebuilt_with_min_rim_segments(&self, n: usize) -> Result<Self, YangError> {
        Self::from_topology(
            self.vertices.clone(),
            self.edges.clone(),
            self.faces.clone(),
            Some(n),
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
        if rim_overrides.is_empty() {
            return Self::from_topology(
                self.vertices.clone(),
                self.edges.clone(),
                self.faces.clone(),
                self.forced_rim_n,
            );
        }
        Self::from_topology_with_rim_overrides(
            self.vertices.clone(),
            self.edges.clone(),
            self.faces.clone(),
            self.forced_rim_n,
            rim_overrides,
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
                    // parameterization, so its endpoints are `BRepVertex`
                    // sources, never `BRepEdge { edge, t }` — this arm should
                    // not be reached. Fall back to the endpoint lerp (identical
                    // to `LineSegment`) rather than panic (P9 defensive; no
                    // plausible-wrong analytic point).
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
