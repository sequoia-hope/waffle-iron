//! Yang 2025 hybrid B-Rep / mesh boolean pipeline.
//!
//! ## Scope (aspirational)
//!
//! Implements the pipeline described in Yang et al. 2025, "A robust hybrid
//! Boolean operations method for mesh-and-surface hybrid models":
//!
//! - **Stage 0** (§4.5.5): Coplanar preprocessing
//! - **Stage 1** (§4.1): Bijective tessellation — PR-YR2: planar B-Reps
//! - **Stage 2** (§4.2): Mesh boolean — delegate to `cherchi-rs`
//! - **Stage 3** (§4.3): SSI refinement — delegate to `ssi-rs`
//! - **Stage 4** (§4.4.1): Mesh updating via CDT
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

pub use cad_primitives::{BoolOp, Point3, Vector3};
pub use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
pub use cherchi_rs::{Mesh, MeshBoolean};

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
/// **Banked deferrals:** subtracted/cavity curved-face *sense* (the curved
/// analog of the plane's outward-normal flip at reconstruction) is deferred to
/// the curved Stage-6 reassembly PR — this PR stores no curved output, so no
/// `sense` field is carried (mirroring ssi-rs, which has none). The
/// `Curve::Parabola`/`Hyperbola` variants are likewise deferred.
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
}

/// Analytical curve for a B-Rep edge.
///
/// PR-YR2 supports `LineSegment` (endpoints implicit from the edge's
/// start/end vertices). PR-YR6 adds `Circle` and `Ellipse` as TYPES (field
/// shapes mirror `ssi-rs`'s `SsiCurve` field-for-field). No production code
/// consumes the curved variants yet — they exist so a future Stage-3 SSI
/// wiring can store analytical intersection curves on output edges.
///
/// **Banked deferral:** `Parabola`/`Hyperbola` are not needed for the first
/// curved demo pairs (plane∩sphere → circle, plane∩cylinder → circle/ellipse,
/// sphere∩cylinder → circle/ellipse) and are deferred to a later PR. Future
/// PRs also add `NurbsCurve`.
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
}

impl BRep {
    /// Construct from B-Rep topology. **Eagerly tessellates** via Stage 1.
    ///
    /// PR-YR2 limitations:
    /// - `Surface::Plane` only (caller-provided)
    /// - Convex faces only (fan-triangulation; non-convex produces
    ///   self-intersecting triangles)
    /// - No inner loops (no holes)
    /// - No Steiner points (output `Mesh` has exactly `verts.len()` vertices)
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
            for &e_idx in &f.outer_loop {
                if (e_idx as usize) >= n_edges {
                    return Err(YangError::MalformedTopology(format!(
                        "face {f_idx}: edge index {e_idx} out of range (edges.len() = {n_edges})"
                    )));
                }
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

        // Stage 1 tessellation (PR-YR7: planar Newell-fan + curved cylinder).
        //
        // Mesh vertices start 1:1 with the B-Rep vertices (the planar box path
        // emits no Steiner points). The curved path appends rim-ring + cap-
        // center Steiner vertices and indexes the SHARED cached rings so the
        // cylinder mesh is watertight.
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
        let circle_edges: Vec<(usize, Point3, Vector3, f64)> = edges
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match e.curve {
                Curve::Circle {
                    center,
                    normal,
                    radius,
                } => Some((i, center, normal, radius)),
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
            let d_eps = curved_chord_bound(&edges).unwrap_or(0.0);
            // Smallest N >= 3 with max_radius·(1 − cos(π/N)) ≤ d_eps.
            let max_r = circle_edges
                .iter()
                .map(|&(_, _, _, r)| r)
                .fold(0.0f64, f64::max);
            let mut n_seg = 3usize;
            // d_eps > 0 for any non-degenerate cylinder; if it is somehow zero
            // (a degenerate AABB), keep the floor N=3 rather than loop forever.
            if d_eps > 0.0 {
                while max_r * (1.0 - (std::f64::consts::PI / n_seg as f64).cos()) > d_eps {
                    n_seg += 1;
                }
            }

            // Build the shared ring for each circle edge.
            for &(e_idx, center, normal, radius) in &circle_edges {
                let (e1, e2) = ortho_basis(normal);
                let c = center.as_array();
                let e1a = e1.as_array();
                let e2a = e2.as_array();
                let seam_vertex = edges[e_idx].start;
                // The seam B-Rep vertex is NOT required to lie at angle 0 of
                // this circle's `ortho_basis` frame — the fixture chooses its
                // own angle-0 convention. Recover the seam's ACTUAL angle `phi0`
                // in this frame so the Steiner verts are placed at evenly-spaced
                // angles STARTING FROM the seam (`phi0 + 2πk/N`). Then `ring[0]`
                // (the seam) is consistent with `ring[1..N]` (chord spacing is
                // uniform) and — crucially for the lateral — the two rims, whose
                // seams sit at the same geometric azimuth, stay azimuth-aligned
                // under the `(N−k)` opposite-rim mapping (spec §6).
                let phi0 = {
                    let sp = match verts.get(seam_vertex as usize) {
                        Some(v) => v.point.as_array(),
                        None => {
                            return Err(YangError::MalformedTopology(format!(
                                "circle edge {e_idx}: seam vertex {seam_vertex} out of range"
                            )))
                        }
                    };
                    let w = [sp[0] - c[0], sp[1] - c[1], sp[2] - c[2]];
                    let wx = w[0] * e1a[0] + w[1] * e1a[1] + w[2] * e1a[2];
                    let wy = w[0] * e2a[0] + w[1] * e2a[1] + w[2] * e2a[2];
                    wy.atan2(wx)
                };
                let mut ring: Vec<u32> = Vec::with_capacity(n_seg);
                // ring[0] = the seam B-Rep vertex (keep its BRepVertex source) —
                // no duplicate at the seam.
                ring.push(seam_vertex);
                for k in 1..n_seg {
                    let theta = phi0 + 2.0 * std::f64::consts::PI * (k as f64) / (n_seg as f64);
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
                rim_rings.insert(e_idx as u32, ring);
            }
        }

        // ---- Per-face dispatch.
        for (f_idx, f) in faces.iter().enumerate() {
            let all_line = f
                .outer_loop
                .iter()
                .all(|&e_idx| matches!(edges[e_idx as usize].curve, Curve::LineSegment));

            match f.surface {
                Surface::Plane { normal, d } if all_line => {
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
                    if mag < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE {
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
                Surface::Plane { normal, .. } => {
                    // ===== Curved-bounded planar cap (disk fan). =====
                    // A planar face whose loop contains a non-LineSegment edge
                    // is a cap bounded by a `Curve::Circle`. Fan from a new
                    // center Steiner vertex over the cached rim ring.
                    tessellate_cap_face(
                        f_idx,
                        f,
                        &edges,
                        &rim_rings,
                        normal,
                        &mut out_verts,
                        &mut sources,
                        &mut out_tris,
                    )?;
                }
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                } => {
                    tessellate_lateral_face(
                        f_idx,
                        f,
                        &edges,
                        &rim_rings,
                        &out_verts,
                        axis_point,
                        axis_dir,
                        radius,
                        &mut out_tris,
                    )?;
                }
                Surface::Sphere { .. } | Surface::Cone { .. } => {
                    return Err(YangError::CurvedSurfaceNotYetSupported { face: f_idx });
                }
            }
        }

        let mesh = Mesh::new(out_verts, out_tris);
        let tessellation = TessellationMap { sources };

        Ok(Self {
            vertices: verts,
            edges,
            faces,
            mesh,
            tessellation,
            triangle_attribution: TriangleAttributionMap::empty(),
        })
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
        }
    }

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

    pub fn num_verts(&self) -> usize {
        self.mesh.num_verts()
    }

    pub fn num_tris(&self) -> usize {
        self.mesh.num_tris()
    }

    /// Evaluate a [`TessellationSource`] back to its 3D point — the inverse of
    /// the Stage-1 bijection (PR-YR7, spec §4).
    ///
    /// INFALLIBLE and panic-free (P9). For the variants the cylinder pipeline
    /// emits (`BRepVertex`, `BRepEdge` over `LineSegment`/`Circle`, `BRepFace`
    /// over `Plane`/`Cylinder`) it reproduces the sampled point exactly via the
    /// SAME `ortho_basis` used during sampling. The remaining cases never occur
    /// for the cylinder pipeline; they use a documented defensive fallback (a
    /// representative point on the surface) rather than panicking.
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
                    Curve::Ellipse { center, .. } => {
                        // Not emitted by the cylinder pipeline; defensive
                        // fallback to the ellipse center.
                        center
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
                    // Not emitted by the cylinder pipeline; defensive fallback
                    // to a representative point (center / apex).
                    Surface::Sphere { center, .. } => center,
                    Surface::Cone { apex, .. } => apex,
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
fn normalize3(v: [f64; 3]) -> [f64; 3] {
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
fn ortho_basis(n: Vector3) -> (Vector3, Vector3) {
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
    out_verts: &[Point3],
    axis_point: Point3,
    axis_dir: Vector3,
    _radius: f64,
    out_tris: &mut Vec<[u32; 3]>,
) -> Result<(), YangError> {
    // The lateral must have exactly 2 Circle boundary edges (its two rims). A
    // cylinder face on a triangle (no Circle rims) is MalformedTopology (loud).
    let circle_edges: Vec<u32> = f
        .outer_loop
        .iter()
        .copied()
        .filter(|&e| matches!(edges[e as usize].curve, Curve::Circle { .. }))
        .collect();
    if circle_edges.len() != 2 {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: cylinder lateral must have exactly 2 Circle rim edges, found {} \
             (a cylinder surface on a non-circular boundary is malformed topology)",
            circle_edges.len()
        )));
    }

    // Identify which rim is the +axis ("top") and which is −axis ("bottom") by
    // the sign of (rim_center − axis_point) · axis_dir.
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
    let (mut bottom_e, mut top_e) = (circle_edges[0], circle_edges[1]);
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
            "face {f_idx}: cylinder rims have mismatched / too-few samples"
        )));
    }

    // Connect by geometric azimuth: top_ring[k] ↔ bottom_ring[(N−k) mod N].
    for k in 0..nseg {
        let kn = (k + 1) % nseg;
        let t0 = top_ring[k];
        let t1 = top_ring[kn];
        let b0 = bottom_ring[(nseg - k) % nseg];
        let b1 = bottom_ring[(nseg - kn) % nseg];
        // Quad (b0, b1, t1, t0) split into 2 tris; orient each radially outward.
        for mut tri in [[b0, b1, t1], [b0, t1, t0]] {
            let n = radial_outward_normal(out_verts, &tri, ap, au);
            orient_tri(out_verts, &mut tri, n);
            out_tris.push(tri);
        }
    }
    Ok(())
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

/// PR-YR7: signed distance from `point` to an analytic `surface` (spec §5).
///
/// - `Plane { normal, d }` → `normal·point + d` (the stored normal, as the
///   planar fixtures use unit normals — same convention as the existing
///   `plane_dist`).
/// - `Cylinder { axis_point, axis_dir, radius }` → `dist(point, axis) − radius`.
/// - `Sphere` / `Cone` → `Err(CurvedSurfaceNotYetSupported { face: usize::MAX })`
///   (the free function has no face index; the boolean closure substitutes the
///   real `fi`). LOUD rejection (P9) — never a panic or planar approximation.
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
        Surface::Sphere { .. } | Surface::Cone { .. } => {
            Err(YangError::CurvedSurfaceNotYetSupported { face: usize::MAX })
        }
    }
}

// =========================================================================
// PR-YR9 (P3) — Stage 3: analytical SSI refinement of intersection edges
// =========================================================================

/// PR-YR9: convert a yang `Surface` into the analytical `ssi_rs::QuadricSurface`
/// for Stage-3 SSI (spec §5.2).
///
/// `Surface::Plane { normal, d }` uses the convention `n·x + d = 0`, while
/// `QuadricSurface::Plane` is `n·(x − point) = 0`, so a point on the plane is
/// `point = -d · n` (with `n` the stored unit normal). `Cylinder` maps
/// field-for-field. `Sphere`/`Cone` have no supported SSI pair in this PR and
/// reject loudly (P9).
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
        Surface::Sphere { .. } | Surface::Cone { .. } => {
            Err(SsiRefinementError::UnsupportedSurfaceForSsi)
        }
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
        ssi_rs::SsiCurve::Parabola { .. } | ssi_rs::SsiCurve::Hyperbola { .. } => {
            Err(SsiRefinementError::UnsupportedCurve)
        }
    }
}

/// PR-YR9: implicit on-curve test (spec §5.4) — does point `p` lie within `tol`
/// of curve `c`? No parameter solving; uses the curve's implicit residual.
/// `tol` is supplied by the caller (the Stage-1 chord bound `d_ε`); no ad-hoc
/// epsilon is introduced. `Parabola`/`Hyperbola` always return `false`.
fn curve_contains_point(c: &ssi_rs::SsiCurve, p: Point3, tol: f64) -> bool {
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
            axial.abs() <= tol && (radial - radius).abs() <= tol
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
        ssi_rs::SsiCurve::Parabola { .. } | ssi_rs::SsiCurve::Hyperbola { .. } => false,
    }
}

/// PR-YR9: build the EXACT analytical `Curve` for each output intersection edge
/// (spec §5.5). An intersection edge is an undirected mesh boundary edge whose
/// incidence list has EXACTLY TWO entries with DIFFERENT `InputId` — it lies on
/// one surface of input A and one of input B.
///
/// For each such edge: convert both surfaces to `QuadricSurface`, call
/// `ssi_rs::intersect`, derive the selection tolerance `tol` from the
/// cylinder-owning input's Stage-1 chord bound, and select the UNIQUE returned
/// curve passing through BOTH mesh endpoints within `tol`. `matched != 1` is a
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

        // Selection tolerance: the Stage-1 chord bound of the cylinder-owning
        // input (A14.3 single source). Plane∩Plane → no cylinder → TAU_WORK.
        let cyl_input = if matches!(surf0, Surface::Cylinder { .. }) {
            Some(input0)
        } else if matches!(surf1, Surface::Cylinder { .. }) {
            Some(input1)
        } else {
            None
        };
        let tol = match cyl_input {
            Some(input) => {
                let owner = match input {
                    InputId::A => a,
                    InputId::B => b,
                };
                match curved_chord_bound(owner.edges()) {
                    Some(t) => t,
                    // A cylinder-bearing input with no circle rims is a producer
                    // fault: never default to TAU_WORK for a curved selection.
                    None => {
                        return Err(YangError::SsiRefinementFailed {
                            edge: (s, e),
                            reason: SsiRefinementError::AmbiguousCurve {
                                candidates: returned.len(),
                                matched: 0,
                            },
                        });
                    }
                }
            }
            None => cad_primitives::TAU_WORK,
        };

        let p_s = mesh.verts[s as usize];
        let p_e = mesh.verts[e as usize];
        let mut matched_idx: Option<usize> = None;
        let mut matched = 0usize;
        for (i, curve) in returned.iter().enumerate() {
            if curve_contains_point(curve, p_s, tol) && curve_contains_point(curve, p_e, tol) {
                matched += 1;
                matched_idx = Some(i);
            }
        }

        if matched != 1 {
            return Err(YangError::SsiRefinementFailed {
                edge: (s, e),
                reason: SsiRefinementError::AmbiguousCurve {
                    candidates: returned.len(),
                    matched,
                },
            });
        }
        let curve = ssi_curve_to_curve(returned[matched_idx.unwrap()]).map_err(|reason| {
            YangError::SsiRefinementFailed {
                edge: (s, e),
                reason,
            }
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
    /// (M3, Stage 6). Either the triangle's surface label names ≥2 solids
    /// (coplanar multi-solid overlap, out of scope → M8), or its centroid
    /// lies on no input face plane / ties between ≥2 planes within
    /// `TAU_WORK`. P9: fail loud, never a silent `None`.
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
                write!(
                    f,
                    "yang-rs: geometric face resolution failed for kept triangle {tri} \
                     (coplanar multi-solid label, or centroid off all face planes / tie)"
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
///    `TriangleAttributionMap` (every entry `Some`). `surface[t]` of
///    length ≠ 1 → `FaceResolutionFailed` (F2 coplanar / multi-solid). For a
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
pub fn boolean(
    a: &BRep,
    b: &BRep,
    op: BoolOp,
    backend: &dyn MeshBoolean,
) -> Result<BRep, YangError> {
    // (1) Stage 2: full labeled arrangement.
    let la = backend
        .labeled_arrangement(a.as_mesh(), b.as_mesh())
        .map_err(YangError::MeshBooleanFailed)?;

    // (2) I6 weld: the C++ producer does NOT always weld coincident vertices
    // (it can emit two distinct indices at bit-identical coordinates — a
    // non-manifold touching point — used by shared triangles). yang's
    // index-based adjacency requires coincident points to share one index, so
    // weld each vertex to the ORIGINAL index of its first bit-identical
    // occurrence. (Mapping to the original index — not a renumbered counter —
    // keeps `la.mesh.verts[welded]` valid: coordinates are unchanged.)
    let weld: Vec<u32> = {
        use std::collections::HashMap;
        let mut first: HashMap<[u64; 3], u32> = HashMap::with_capacity(la.mesh.verts.len());
        la.mesh
            .verts
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let key = [v.x().to_bits(), v.y().to_bits(), v.z().to_bits()];
                *first.entry(key).or_insert(i as u32)
            })
            .collect()
    };
    // A bit-exact weld is all the exact arrangement needs: the producer never
    // emits TAU_WORK-near-but-bit-distinct coincident verts (they would survive
    // as distinct indices and fragment adjacency). A defensive O(n²) near-check
    // is therefore redundant for this producer and is omitted.

    // (3) Stage 4: which arrangement tris survive `op`.
    let kept = la.keep_set(op);

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

    // (5) Stage 6: geometric face resolution → FULL attribution.
    let mut attributions: Vec<Option<TriangleAttribution>> = Vec::with_capacity(orig_tri.len());
    for (compact_t, &orig_t) in orig_tri.iter().enumerate() {
        let surf = &la.surface[orig_t];
        // F2: coplanar / multi-solid surface label (out of scope, M8).
        if surf.len() != 1 {
            return Err(YangError::FaceResolutionFailed { tri: compact_t });
        }
        let LaInputId(k) = surf[0];
        // cherchi InputId(u32): 0 → A, 1 → B.
        let (input_brep, input) = match k {
            0 => (a, InputId::A),
            _ => (b, InputId::B),
        };

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
        let plane_dist = |fi: usize, face: &BRepFace| -> Result<f64, YangError> {
            // PR-YR7: delegate to the shared `signed_distance_to_surface`
            // (Plane + Cylinder); take `.abs()` (distance to the surface).
            // Sphere/Cone still reject loudly — the free function returns a
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
                Surface::Plane { .. } => Ok(cad_primitives::TAU_WORK),
                Surface::Cylinder { .. } => match band {
                    Some(de) => Ok(de),
                    None => Err(YangError::FaceResolutionFailed { tri: compact_t }),
                },
                // Sphere/Cone never reach here (`plane_dist` propagates the loud
                // CurvedSurfaceNotYetSupported first), but stay LOUD if they do.
                Surface::Sphere { .. } | Surface::Cone { .. } => {
                    Err(YangError::CurvedSurfaceNotYetSupported { face: fi })
                }
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
            // Non-degenerate triangle: the F1/F3 rule generalized to the
            // per-face tolerance — count faces whose surface contains the
            // centroid within ITS tolerance: exactly one → attribute; zero →
            // F3 (no match); ≥2 → F3 (tie).
            //
            // For all-planar inputs (every tol = TAU_WORK), "exactly one face
            // with dist < TAU_WORK" is byte-for-byte the OLD rule ("min <
            // TAU_WORK ∧ second ≥ TAU_WORK", argmin face): the single sub-tol
            // face IS the overall argmin, so the attributed face is identical.
            let mut hit: Option<u32> = None;
            let mut n_hits = 0usize;
            for (fi, f) in input_brep.faces().iter().enumerate() {
                if plane_dist(fi, f)? < tol_for(fi, f.surface)? {
                    n_hits += 1;
                    if n_hits == 1 {
                        hit = Some(fi as u32);
                    }
                }
            }
            match (n_hits, hit) {
                (1, Some(fi)) => fi,
                _ => return Err(YangError::FaceResolutionFailed { tri: compact_t }),
            }
        };
        attributions.push(Some(TriangleAttribution { input, face }));
    }
    let triangle_attribution = TriangleAttributionMap { attributions };

    // (6) Topology reconstruction (unchanged).
    let (vertices, edges, faces) =
        reconstruct_topology(&kept_submesh, &triangle_attribution, a, b)?;

    // Output mesh = the compact kept sub-mesh; tessellation 1:1 with its
    // verts (BRepVertex(i)).
    let sources: Vec<TessellationSource> = (0..kept_submesh.num_verts() as u32)
        .map(TessellationSource::BRepVertex)
        .collect();
    let tessellation = TessellationMap { sources };

    Ok(BRep {
        vertices,
        edges,
        faces,
        mesh: kept_submesh,
        tessellation,
        triangle_attribution,
    })
}

// =========================================================================
// PR-YR5 — topology reconstruction
// =========================================================================

/// PR-YR5 internal: the triple `(vertices, edges, faces)` produced
/// by `reconstruct_topology` to populate the output `BRep`.
type ReconstructedTopology = (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>);

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
fn reconstruct_topology(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
    a: &BRep,
    b: &BRep,
) -> Result<ReconstructedTopology, YangError> {
    // (1) Vertices: 1:1 with mesh.verts
    let vertices: Vec<BRepVertex> = mesh
        .verts
        .iter()
        .map(|&p| BRepVertex { point: p })
        .collect();

    // (2) Triangle adjacency
    let adjacency = triangle_adjacency(mesh);

    // (3) Flood-fill same-attribution patches
    let patches = flood_fill_patches(mesh, attribution, &adjacency);

    // (4) First pass (PR-YR9): resolve each patch's boundary cycles, owning
    // input, inherited surface, and face index ONCE. The face-range check and
    // inherited-surface lookup live HERE only (spec §5.6 — "in exactly one
    // place"), so the emission loop never re-derives them.
    struct PatchInfo {
        cycles: Vec<Vec<(u32, u32)>>,
        input: InputId,
        inherited: Surface,
        face_idx: usize,
    }
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
        infos.push(PatchInfo {
            cycles,
            input,
            inherited,
            face_idx,
        });
    }

    // (4a) PR-YR9 Stage 3: build EXACT intersection curves. An output
    // intersection edge is incident to two patches of DIFFERENT InputId; its
    // analytical `Curve` comes from `ssi_rs::intersect` of the two inherited
    // surfaces. The incidence map keys edges canonically; `info.inherited` is
    // UNFLIPPED (the conic is sign-invariant and Union has no flip).
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
    let intersection_curves = build_intersection_curves(&incidence, mesh, a, b)?;

    // (4b) Emission pass: walk `infos`, emit faces/edges. The Newell / flip /
    // E2 / E3 machinery is UNCHANGED (it reads `cycles` / `signed_areas`, never
    // the per-edge curve). The ONLY change vs PR-YR8 is the per-edge `curve`:
    // an intersection edge gets its exact conic, all others stay LineSegment.
    let mut edges: Vec<BRepEdge> = Vec::new();
    let mut faces: Vec<BRepFace> = Vec::new();
    for info in &infos {
        let cycles = &info.cycles;
        let inherited = info.inherited;
        let face_idx = info.face_idx;

        // PR-YR8 (P2c) Blocker 2, spec §4: curved-surface branch BEFORE the
        // planar normal/Newell/flip machinery. A `Cylinder` patch is a barrel
        // — a single plane normal + signed-area classification is meaningless,
        // so we DROP the E3/`positive_count` check and the inherited-normal
        // flip. We INHERIT the surface UNCHANGED (Union has no cavity → no
        // sense flip; the curved cavity-sense flip for Subtract is explicitly
        // DEFERRED). `patch_boundary_cycle` (called above) is surface-agnostic,
        // so we reuse `cycles`. We KEEP the E2 degenerate-loop guard.
        if let Surface::Cylinder { .. } = inherited {
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
                    return Err(YangError::NonManifoldOutput);
                }
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

            faces.push(BRepFace {
                surface: inherited,
                outer_loop,
                inner_loops,
            });
            continue;
        }

        let (normal, d) = match inherited {
            Surface::Plane { normal, d } => (normal, d),
            // Sphere/Cone Stage-6 reassembly stays a loud reject (P9): they can
            // never reach here through `BRep::new` (rejected at construction).
            // Cylinder is handled by the curved branch above (unreachable here).
            Surface::Sphere { .. } | Surface::Cylinder { .. } | Surface::Cone { .. } => {
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
                return Err(YangError::NonManifoldOutput);
            }
            signed_areas.push(nx * n[0] + ny * n[1] + nz * n[2]);
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
            return Err(YangError::NonManifoldOutput);
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

        faces.push(BRepFace {
            surface,
            outer_loop,
            inner_loops,
        });
    }

    Ok((vertices, edges, faces))
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

    // Precompute edge → tris-in-patch count for O(T) total cost
    let mut patch_edge_count: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for &t in &patch.tri_indices {
        let tri = &mesh.tris[t as usize];
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (va, vb) = (tri[i], tri[j]);
            let key = if va < vb { (va, vb) } else { (vb, va) };
            *patch_edge_count.entry(key).or_insert(0) += 1;
        }
    }

    // Collect directed boundary edges in triangle CCW order
    let mut directed_boundary: Vec<(u32, u32)> = Vec::new();
    for &t in &patch.tri_indices {
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
                let next_vec = by_start
                    .get_mut(&current)
                    .ok_or(YangError::NonManifoldOutput)?;
                if next_vec.is_empty() {
                    // Dead-end / T-junction: a genuine non-manifold patch.
                    return Err(YangError::NonManifoldOutput);
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
                return Err(YangError::NonManifoldOutput);
            }
        }
        cycles.push(cycle);
    }

    Ok(cycles)
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
        }];
        (verts, edges, faces)
    }

    #[test]
    fn brep_new_rejects_sphere_face() {
        let (verts, edges, faces) = single_triangle_topology(Surface::Sphere {
            center: p(0.0, 0.0, 0.0),
            radius: 1.0,
        });
        let result = BRep::new(verts, edges, faces);
        assert!(
            matches!(
                result,
                Err(YangError::CurvedSurfaceNotYetSupported { face: 0 })
            ),
            "expected CurvedSurfaceNotYetSupported {{ face: 0 }}, got {result:?}"
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
        let (verts, edges, faces) = single_triangle_topology(Surface::Cone {
            apex: p(0.0, 0.0, 1.0),
            axis_dir: Vector3::new(0.0, 0.0, -1.0),
            half_angle: 0.5,
        });
        let result = BRep::new(verts, edges, faces);
        assert!(
            matches!(
                result,
                Err(YangError::CurvedSurfaceNotYetSupported { face: 0 })
            ),
            "expected CurvedSurfaceNotYetSupported {{ face: 0 }}, got {result:?}"
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
            }, // bottom (verts 0,1,2)
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![9, 3, 7],
                inner_loops: Vec::new(),
            }, // back (verts 1,0,3) - using 1→0,0→3,3→1
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![10, 4, 8],
                inner_loops: Vec::new(),
            }, // right (verts 2,1,3)
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![11, 5, 6],
                inner_loops: Vec::new(),
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
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![4, 5, 6, 7],
                inner_loops: Vec::new(),
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![8, 9, 10, 11],
                inner_loops: Vec::new(),
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![12, 13, 14, 15],
                inner_loops: Vec::new(),
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![16, 17, 18, 19],
                inner_loops: Vec::new(),
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![20, 21, 22, 23],
                inner_loops: Vec::new(),
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
            }, // F0
            BRepFace {
                surface: f1_plane,
                outer_loop: vec![3, 4, 5],
                inner_loops: Vec::new(),
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
        // offset is d = -c.
        let offs = [-z, z + 1.0, -y, x + 1.0, y + 1.0, -x];
        let faces: Vec<BRepFace> = (0..6)
            .map(|i| BRepFace {
                surface: Surface::Plane {
                    normal: normals[i],
                    d: offs[i],
                },
                outer_loop: loops[i].clone(),
                inner_loops: Vec::new(),
            })
            .collect();
        BRep::new(verts, edges, faces).unwrap()
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

    /// Hand-built arrangement: a CLOSEABLE 2-triangle quad on cube A's
    /// bottom face (z=0). The quad's 4 verts are A's exact bottom-face
    /// corners (0,0,0)(1,0,0)(1,1,0)(0,1,0), so:
    /// - real-label path: each tri's centroid lies on exactly one A face
    ///   plane (z=0) → I7 unique-face → full Some(A, face0) attribution;
    /// - the patch boundary 0→1→2→3→0 closes (single manifold cycle) so
    ///   `reconstruct_topology` succeeds (no `NonManifoldOutput`);
    /// - the verts coincide with A's `BRepVertex`es, so the M4 substitute's
    ///   spatial matching also resolves to A's bottom face (vertex-face
    ///   incidence majority → F0), letting the differential oracle agree.
    ///
    /// All `inside` all-false ⇒ both tris kept by Union.
    fn arrangement_a_bottom_quad() -> LabeledArrangement {
        let verts = vec![
            p(0.0, 0.0, 0.0), // A vert 0
            p(1.0, 0.0, 0.0), // A vert 1
            p(1.0, 1.0, 0.0), // A vert 2
            p(0.0, 1.0, 0.0), // A vert 3
        ];
        // Two tris forming the quad; boundary 0→1→2→3→0 closes cleanly.
        let tris = vec![[0u32, 1, 2], [0, 2, 3]];
        let mesh = Mesh::new(verts, tris);
        // Both on A's surface (solid 0), none on B; inside all-false ⇒ Union keeps.
        let surface = vec![vec![LaInputId(0)]; 2];
        let inside = vec![vec![false, false]; 2];
        let patch = vec![0u32, 0];
        LabeledArrangement {
            mesh,
            surface,
            inside,
            patch,
            num_inputs: 2,
        }
    }

    #[test]
    fn m3_union_full_attribution_coverage() {
        // I7 + full-coverage: every kept output triangle resolves to Some.
        let a = cube_brep([0.0, 0.0, 0.0]);
        let b = cube_brep([0.5, 0.0, 0.0]);
        let la = arrangement_a_bottom_quad();
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
        // centroid lies on (here A's bottom face, z=0).
        let a = cube_brep([0.0, 0.0, 0.0]);
        let b = cube_brep([0.5, 0.0, 0.0]);
        let la = arrangement_a_bottom_quad();
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
        let b = cube_brep([0.5, 0.0, 0.0]);
        let la = arrangement_a_bottom_quad();
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
        let b = cube_brep([0.0, 0.0, 0.0]); // coincident so a z=0 tri is on both
        let verts = vec![p(0.0, 0.0, 0.0), p(0.5, 0.0, 0.0), p(0.0, 0.5, 0.0)];
        let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
        let la = LabeledArrangement {
            mesh,
            // surface names BOTH A and B (coplanar multi-solid) — F2.
            surface: vec![vec![LaInputId(0), LaInputId(1)]],
            inside: vec![vec![false, false]], // kept by Union
            patch: vec![0],
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
        let b = cube_brep([0.5, 0.0, 0.0]);
        // Triangle floating at z=0.5 (interior; off every cube face plane).
        let verts = vec![p(0.25, 0.25, 0.5), p(0.5, 0.25, 0.5), p(0.25, 0.5, 0.5)];
        let mesh = Mesh::new(verts, vec![[0u32, 1, 2]]);
        let la = LabeledArrangement {
            mesh,
            surface: vec![vec![LaInputId(0)]], // claims solid A's surface
            inside: vec![vec![false, false]],  // kept by Union
            patch: vec![0],
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
        let b = cube_brep([0.5, 0.0, 0.0]);
        let la = arrangement_a_bottom_quad();
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
}
