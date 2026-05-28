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
pub use cherchi_rs::{Mesh, MeshBoolean};

// =========================================================================
// Surface / Curve enums
// =========================================================================

/// Analytical surface for a B-Rep face.
///
/// PR-YR2 supports `Plane` only. Future PRs add `Cylinder`, `Sphere`,
/// `Cone`, `Torus`, `NurbsSurface`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Surface {
    /// Plane: `n·x + d = 0`. Normal `n` points OUTWARD from the solid.
    Plane { normal: Vector3, d: f64 },
}

/// Analytical curve for a B-Rep edge.
///
/// PR-YR2 supports `LineSegment` only (endpoints implicit from the
/// edge's start/end vertices). Future PRs add `Circle`, `Ellipse`,
/// `NurbsCurve`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Curve {
    LineSegment,
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
}

// =========================================================================
// TessellationMap — the bijection
// =========================================================================

/// Where a mesh vertex came from in the B-Rep input.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TessellationSource {
    /// Mesh vertex coincides with B-Rep vertex (index into `BRep::vertices`).
    BRepVertex(u32),
    /// Mesh vertex is on edge `edge` at parameter `t ∈ [0, 1]`.
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

        // Validate: every face's outer_loop is well-formed.
        for (f_idx, f) in faces.iter().enumerate() {
            if f.outer_loop.len() < 3 {
                return Err(YangError::MalformedTopology(format!(
                    "face {f_idx}.outer_loop.len() = {} < 3",
                    f.outer_loop.len()
                )));
            }
            for &e_idx in &f.outer_loop {
                if (e_idx as usize) >= n_edges {
                    return Err(YangError::MalformedTopology(format!(
                        "face {f_idx}: edge index {e_idx} out of range (edges.len() = {n_edges})"
                    )));
                }
            }
        }

        // Stage 1 fan-triangulation:
        // - Mesh vertices = B-Rep vertices (1:1, no Steiner points)
        // - Each face fan-triangulated from its first vertex (face_verts[0])
        let out_verts: Vec<Point3> = verts.iter().map(|v| v.point).collect();
        let mut sources: Vec<TessellationSource> = (0..verts.len() as u32)
            .map(TessellationSource::BRepVertex)
            .collect();
        // Pad sources to mesh-vertex count (it equals B-Rep vertex count
        // in PR-YR2, so this is a no-op, but explicit for future PRs).
        sources.truncate(out_verts.len());

        let mut out_tris: Vec<[u32; 3]> = Vec::new();
        for (f_idx, f) in faces.iter().enumerate() {
            // Walk the outer loop: collect each edge's start vertex
            // (which is the face's vertex at that loop position).
            let mut face_verts: Vec<u32> = f
                .outer_loop
                .iter()
                .map(|&e_idx| edges[e_idx as usize].start)
                .collect();

            // Stage-1 winding canonicalization (Yang 2025 §4.1: the
            // tessellation must preserve the B-Rep surface orientation;
            // Cherchi 2022 §3 requires globally-oriented input or the
            // boolean is undefined). Per governance A15.5 the analytic
            // surface normal is authoritative, so we orient each face's
            // triangle winding to agree with `Surface::Plane.normal`
            // rather than trusting the (possibly inside-out) loop order.
            //
            // Compute the polygon normal via Newell's method (Sutherland,
            // Sproull & Schumacker 1974) — robust for (near-)planar loops:
            //   nx += (y_i - y_j)*(z_i + z_j), etc., over consecutive
            // loop vertices (j = next, wrapping).
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
                (newell[0] * newell[0] + newell[1] * newell[1] + newell[2] * newell[2]).sqrt();
            // B3: zero-area / collinear / degenerate face. `mag` is the
            // Newell magnitude = 2×(polygon area) (units length²), so the
            // threshold is an AREA: compare against MIN_FEATURE_SIZE² (the
            // minimum feature area, 1e-12 m²), computed inline from the
            // shared length constant (governance A14.3: no ad-hoc epsilon).
            if mag < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE {
                return Err(YangError::DegenerateFace { face: f_idx });
            }

            let Surface::Plane { normal, .. } = f.surface;
            let n = normal.as_array();
            let dot = newell[0] * n[0] + newell[1] * n[1] + newell[2] * n[2];
            // B2: Newell normal opposes the analytic outward normal →
            // reverse the loop so the fan winds outward.
            if dot < 0.0 {
                face_verts.reverse();
            }

            // Fan-triangulate from the (possibly reversed) loop's first vertex.
            for i in 1..face_verts.len() - 1 {
                out_tris.push([face_verts[0], face_verts[i], face_verts[i + 1]]);
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

/// Boolean operation on two B-Rep solids via a `MeshBoolean` backend.
///
/// Pipeline (after backend produces the output mesh):
/// 1. **PR-YR3 vertex pass**: for each output vertex, spatially match
///    against input A first, then B (within [`MATCH_TOLERANCE`]). On
///    match, copy that input's `TessellationSource`; else mark
///    `TessellationSource::Intersection`.
/// 2. **PR-YR4 triangle pass**: for each output triangle, derive
///    candidate `(InputId, face_idx)` sets from each vertex's
///    provenance (a `BRepVertex(v)` contributes every face touching
///    `v`; a `BRepEdge { edge }` contributes every face whose
///    `outer_loop` contains it; a `BRepFace { face }` contributes the
///    singleton; `Intersection`/`Unknown` contribute nothing).
///    Attribution = `Some((input, face))` whose vote count is the
///    maximum among pairs with count ≥ 2; ties broken by lowest
///    `(input, face)` lexicographic. No 2-of-3 majority → `None`.
///
/// **NOT real Yang Stage 5/6.** Real Stage 5/6 needs per-triangle
/// labels from Stage 2's arrangement which the C++ sidecar doesn't
/// expose. PR-YR3 + PR-YR4 are sidecar-feasible substitutes that
/// recover vertex- and triangle-level provenance via spatial matching
/// and majority-vote. Output `BRep` topology (faces, edges) remains
/// empty — face reconstruction is PR-YR5+.
pub fn boolean(
    a: &BRep,
    b: &BRep,
    op: BoolOp,
    backend: &dyn MeshBoolean,
) -> Result<BRep, YangError> {
    let output_mesh = backend
        .boolean(a.as_mesh(), b.as_mesh(), op)
        .map_err(YangError::MeshBooleanFailed)?;

    // PR-YR3 vertex pass + internal InputId tracking for PR-YR4.
    let mut sources = Vec::with_capacity(output_mesh.num_verts());
    let mut inputs: Vec<Option<InputId>> = Vec::with_capacity(output_mesh.num_verts());
    for &target in &output_mesh.verts {
        let (input, src) = match_with_input(a, b, target);
        sources.push(src);
        inputs.push(input);
    }
    let tessellation = TessellationMap { sources };

    // PR-YR4 triangle pass.
    let mut attributions = Vec::with_capacity(output_mesh.num_tris());
    for tri in &output_mesh.tris {
        let sets = [
            face_candidates(inputs[tri[0] as usize], tessellation.lookup(tri[0]), a, b),
            face_candidates(inputs[tri[1] as usize], tessellation.lookup(tri[1]), a, b),
            face_candidates(inputs[tri[2] as usize], tessellation.lookup(tri[2]), a, b),
        ];
        attributions.push(majority_vote(&sets));
    }
    let triangle_attribution = TriangleAttributionMap { attributions };

    // PR-YR5 topology reconstruction: group same-attribution triangles
    // into patches, walk each patch's boundary cycle, inherit input
    // face surface, build BRepVertex / BRepEdge / BRepFace.
    let (vertices, edges, faces) = reconstruct_topology(&output_mesh, &triangle_attribution, a, b)?;

    Ok(BRep {
        vertices,
        edges,
        faces,
        mesh: output_mesh,
        tessellation,
        triangle_attribution,
    })
}

/// Try to match `target` against a vertex in `brep`'s mesh within
/// `MATCH_TOLERANCE`. Returns the matched vertex's `TessellationSource`
/// or `None`. Private to PR-YR3's `boolean()` impl.
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

/// PR-YR4 helper: match `target` against A first, then B; track
/// which input matched (for triangle-level face attribution).
fn match_with_input(a: &BRep, b: &BRep, target: Point3) -> (Option<InputId>, TessellationSource) {
    if let Some(src) = match_against(a, target) {
        return (Some(InputId::A), src);
    }
    if let Some(src) = match_against(b, target) {
        return (Some(InputId::B), src);
    }
    (None, TessellationSource::Intersection)
}

/// PR-YR4 helper: compute the set of `(InputId, face_idx)` pairs
/// that a single output vertex's provenance is compatible with.
///
/// - `BRepFace { face, .. }` → `[(input, face)]`
/// - `BRepEdge { edge, .. }` → every face whose `outer_loop` contains `edge`
/// - `BRepVertex(v)` → every face whose `outer_loop` has an edge with `start==v` or `end==v`
/// - `Intersection` / `Unknown` / `input == None` → `[]`
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

/// PR-YR4 helper: count votes per `(InputId, face)` across the 3
/// vertices' candidate sets and return the highest-count pair that
/// reaches ≥2 votes. Ties broken by lowest `(InputId, face)`
/// lexicographic (achieved via `BTreeMap` ascending iteration +
/// strictly-greater replacement rule).
fn majority_vote(sets: &[Vec<(InputId, u32)>; 3]) -> Option<TriangleAttribution> {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<(InputId, u32), u8> = BTreeMap::new();
    for set in sets {
        // Dedup within a single vertex's set (a face should appear
        // once per vertex; defensive).
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
/// 3. For each patch, walk the directed boundary cycle (edges in
///    exactly one patch triangle, ordered).
/// 4. Inherit `surface` from `input.faces()[attribution.face]`.
/// 5. Output `vertices` is 1:1 with `mesh.verts`.
///
/// Errors:
/// - `NonManifoldOutput`: cycle walking dead-ends, T-junctions, or
///   patch has multiple boundary cycles (inner loops unsupported in v1).
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

    // (4) Per-patch boundary cycle + face construction
    let mut edges: Vec<BRepEdge> = Vec::new();
    let mut faces: Vec<BRepFace> = Vec::new();
    for patch in &patches {
        let cycle = patch_boundary_cycle(patch, mesh)?;
        let edge_start_idx = edges.len() as u32;
        for (s, e) in &cycle {
            edges.push(BRepEdge {
                start: *s,
                end: *e,
                curve: Curve::LineSegment,
            });
        }
        let outer_loop: Vec<u32> = (edge_start_idx..edges.len() as u32).collect();

        let input_brep = match patch.attribution.input {
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
        let surface = input_brep.faces()[face_idx].surface;
        faces.push(BRepFace {
            surface,
            outer_loop,
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

/// PR-YR5 helper: recover the directed boundary cycle of a patch.
/// Boundary edges = edges in exactly one patch triangle (canonical
/// (min, max) test). Walk from the lowest start-vertex; follow
/// start→end chain via `BTreeMap` (deterministic).
///
/// Returns `Err(NonManifoldOutput)` on dead-end, T-junction, or
/// multi-cycle patches (inner loops unsupported in v1).
fn patch_boundary_cycle(patch: &Patch, mesh: &Mesh) -> Result<Vec<(u32, u32)>, YangError> {
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

    // Walk: start at lowest start vertex.
    let start = *by_start.keys().next().expect("non-empty boundary");
    let mut current = start;
    let mut cycle: Vec<(u32, u32)> = Vec::new();
    loop {
        let next = {
            let next_vec = by_start
                .get_mut(&current)
                .ok_or(YangError::NonManifoldOutput)?;
            if next_vec.is_empty() {
                return Err(YangError::NonManifoldOutput);
            }
            next_vec.remove(0)
        };
        cycle.push((current, next));
        current = next;
        if current == start {
            break;
        }
        if cycle.len() > directed_boundary.len() {
            return Err(YangError::NonManifoldOutput);
        }
    }

    if cycle.len() != directed_boundary.len() {
        // Multi-cycle patch (inner loops / disjoint boundaries).
        return Err(YangError::NonManifoldOutput);
    }

    Ok(cycle)
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    fn sample_mesh() -> Mesh {
        Mesh::new(
            vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
            vec![[0, 1, 2]],
        )
    }

    struct MockBackend {
        result: Result<Mesh, &'static str>,
    }
    impl MockBackend {
        fn ok(mesh: Mesh) -> Self {
            Self { result: Ok(mesh) }
        }
        fn err() -> Self {
            Self {
                result: Err("mock failure"),
            }
        }
    }
    impl MeshBoolean for MockBackend {
        fn boolean(
            &self,
            _a: &Mesh,
            _b: &Mesh,
            _op: BoolOp,
        ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
            self.result
                .clone()
                .map_err(|s| -> Box<dyn Error + Send + Sync> { Box::from(s) })
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
        }
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
            }, // bottom (verts 0,1,2)
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![9, 3, 7],
            }, // back (verts 1,0,3) - using 1→0,0→3,3→1
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![10, 4, 8],
            }, // right (verts 2,1,3)
            BRepFace {
                surface: plane_z_up(),
                outer_loop: vec![11, 5, 6],
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
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![4, 5, 6, 7],
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![8, 9, 10, 11],
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![12, 13, 14, 15],
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![16, 17, 18, 19],
            },
            BRepFace {
                surface: plane,
                outer_loop: vec![20, 21, 22, 23],
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
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let mock = MockBackend::ok(Mesh::empty());
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(r.num_verts(), 0);
    }

    #[test]
    fn boolean_with_err_backend() {
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let mock = MockBackend::err();
        match boolean(&a, &b, BoolOp::Union, &mock) {
            Err(YangError::MeshBooleanFailed(_)) => {}
            other => panic!("expected MeshBooleanFailed, got {:?}", other),
        }
    }

    #[test]
    fn boolean_dispatches_all_four_ops() {
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let mock = MockBackend::ok(Mesh::empty());
        for op in [
            BoolOp::Union,
            BoolOp::Intersect,
            BoolOp::Subtract,
            BoolOp::Xor,
        ] {
            assert!(boolean(&a, &b, op, &mock).is_ok(), "op {op:?}");
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
        }];
        BRep::new(verts, edges, faces).unwrap()
    }

    #[test]
    fn boolean_input_a_verbatim_copies_a_map() {
        let a = triangle_brep();
        let b = triangle_brep();
        // Mock returns input A's mesh verbatim.
        let mock = MockBackend::ok(a.as_mesh().clone());
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(r.num_verts(), 3);
        // Every output vertex should map to a BRepVertex (matched
        // against input A, which has BRepVertex entries).
        for i in 0..3u32 {
            assert_eq!(
                r.tessellation_map().lookup(i),
                TessellationSource::BRepVertex(i),
                "output vertex {i}"
            );
        }
    }

    #[test]
    fn boolean_input_b_verbatim_copies_b_map() {
        let a = triangle_brep();
        // B has different vertices so A's spatial match fails.
        let mut b_verts = a.vertices().to_vec();
        for v in &mut b_verts {
            v.point = Point3::new(v.point.x() + 10.0, v.point.y(), v.point.z());
        }
        let b_edges = a.edges().to_vec();
        let b_faces = a.faces().to_vec();
        let b = BRep::new(b_verts, b_edges, b_faces).unwrap();
        // Mock returns B's mesh verbatim.
        let mock = MockBackend::ok(b.as_mesh().clone());
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        for i in 0..3u32 {
            assert_eq!(
                r.tessellation_map().lookup(i),
                TessellationSource::BRepVertex(i),
                "output vertex {i} — should match input B's BRepVertex({i})"
            );
        }
    }

    #[test]
    fn boolean_all_new_coords_are_intersection() {
        let a = triangle_brep();
        let b = triangle_brep();
        // Mock returns a mesh with totally new coords (offset by 100).
        let novel = Mesh::new(
            vec![
                p(100.0, 100.0, 100.0),
                p(101.0, 100.0, 100.0),
                p(100.0, 101.0, 100.0),
            ],
            vec![[0, 1, 2]],
        );
        let mock = MockBackend::ok(novel);
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        for i in 0..3u32 {
            assert_eq!(
                r.tessellation_map().lookup(i),
                TessellationSource::Intersection,
                "vertex {i} should be Intersection"
            );
        }
    }

    #[test]
    fn boolean_mixed_match_and_intersection() {
        let a = triangle_brep();
        let b = triangle_brep();
        // Mock returns 2 vertices from A + 2 new coords.
        let mixed = Mesh::new(
            vec![
                p(0.0, 0.0, 0.0),   // matches a.vertex(0)
                p(1.0, 0.0, 0.0),   // matches a.vertex(1)
                p(99.0, 99.0, 0.0), // new
                p(98.0, 98.0, 0.0), // new
            ],
            vec![[0, 1, 2], [0, 2, 3]],
        );
        let mock = MockBackend::ok(mixed);
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(
            r.tessellation_map().lookup(0),
            TessellationSource::BRepVertex(0)
        );
        assert_eq!(
            r.tessellation_map().lookup(1),
            TessellationSource::BRepVertex(1)
        );
        assert_eq!(
            r.tessellation_map().lookup(2),
            TessellationSource::Intersection
        );
        assert_eq!(
            r.tessellation_map().lookup(3),
            TessellationSource::Intersection
        );
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
            }, // F0
            BRepFace {
                surface: f1_plane,
                outer_loop: vec![3, 4, 5],
            }, // F1
        ];
        BRep::new(verts, edges, faces).unwrap()
    }

    #[test]
    fn boolean_pure_a_attributes_to_a_faces() {
        // Pure-A: mock returns A's mesh verbatim. Each output tri's verts
        // are BRepVertex(i) of A → candidates derive A's per-vertex face
        // incidence → majority-vote attributes each tri to its source face.
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let mock = MockBackend::ok(a.as_mesh().clone());
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(r.num_tris(), 2);
        assert_eq!(
            r.triangle_attribution().lookup(0),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0
            }),
            "output tri 0 (F0 fan tri) should attribute to A's F0"
        );
        assert_eq!(
            r.triangle_attribution().lookup(1),
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
        // B is the same B-Rep, but shifted so A's spatial match fails first.
        let mut b_verts = a.vertices().to_vec();
        for v in &mut b_verts {
            v.point = Point3::new(v.point.x() + 100.0, v.point.y(), v.point.z());
        }
        let b = BRep::new(b_verts, a.edges().to_vec(), a.faces().to_vec()).unwrap();
        let mock = MockBackend::ok(b.as_mesh().clone());
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(
            r.triangle_attribution().lookup(0),
            Some(TriangleAttribution {
                input: InputId::B,
                face: 0
            })
        );
        assert_eq!(
            r.triangle_attribution().lookup(1),
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
        // Mock returns a mesh with coords far from both inputs.
        let novel = Mesh::new(
            vec![
                p(1000.0, 1000.0, 1000.0),
                p(1001.0, 1000.0, 1000.0),
                p(1000.0, 1001.0, 1000.0),
            ],
            vec![[0, 1, 2]],
        );
        let mock = MockBackend::ok(novel);
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(r.num_tris(), 1);
        assert_eq!(
            r.triangle_attribution().lookup(0),
            None,
            "all-new triangle should have None attribution"
        );
    }

    #[test]
    fn boolean_mixed_majority_wins() {
        // 2 verts match A's F0 + 1 novel → F0 attribution.
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        // Mock returns mesh with V1, V2 from A + 1 new coord.
        let mixed = Mesh::new(
            vec![
                p(1.0, 0.0, 0.0),       // matches a.verts[1] (F0 only)
                p(1.0, 1.0, 0.0),       // matches a.verts[2] (F0 only) — tracks moved V2
                p(1000.0, 0.0, 1000.0), // novel
            ],
            vec![[0, 1, 2]],
        );
        let mock = MockBackend::ok(mixed);
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(
            r.triangle_attribution().lookup(0),
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
        // B has distinct coords from A (offset).
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
        let mock = MockBackend::ok(mixed);
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(
            r.triangle_attribution().lookup(0),
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
        let mock = MockBackend::ok(tie_mesh);
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(
            r.triangle_attribution().lookup(0),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0
            }),
            "tie at count 2 between F0 and F1 → lowest face (F0)"
        );
    }

    // ----- PR-YR4: Group 3 — empty-topology degradation -----

    #[test]
    fn boolean_both_inputs_from_mesh_all_none() {
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let mock = MockBackend::ok(sample_mesh());
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(r.triangle_attribution().len(), r.num_tris());
        assert_eq!(
            r.triangle_attribution().lookup(0),
            None,
            "from_mesh inputs have all-Unknown sources → all-None attribution"
        );
    }

    #[test]
    fn boolean_mixed_from_mesh_and_topologized() {
        // a has topology, b is from_mesh. Mock returns a's mesh verbatim.
        // Attribution should reflect a's per-tri face ownership.
        let a = two_face_shared_vertex_brep();
        let b = BRep::from_mesh(sample_mesh());
        let mock = MockBackend::ok(a.as_mesh().clone());
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(
            r.triangle_attribution().lookup(0),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 0
            })
        );
        assert_eq!(
            r.triangle_attribution().lookup(1),
            Some(TriangleAttribution {
                input: InputId::A,
                face: 1
            })
        );
    }

    // ----- PR-YR5: topology reconstruction -----

    #[test]
    fn yr5_single_triangle_round_trip_produces_one_face() {
        // Pure-A on triangle_brep (1 face, 1 fan tri) → output has 1
        // face with 3 boundary edges + 3 vertices forming a closed
        // cycle.
        let a = triangle_brep();
        let b = triangle_brep();
        let mock = MockBackend::ok(a.as_mesh().clone());
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(r.faces().len(), 1, "expected 1 BRepFace");
        assert_eq!(r.faces()[0].outer_loop.len(), 3, "expected 3-edge loop");
        assert_eq!(r.edges().len(), 3, "expected 3 BRepEdges");
        assert_eq!(r.vertices().len(), 3, "expected 3 BRepVertices");
        // Cycle closure
        let f = &r.faces()[0];
        for i in 0..3 {
            let e_curr = &r.edges()[f.outer_loop[i] as usize];
            let e_next = &r.edges()[f.outer_loop[(i + 1) % 3] as usize];
            assert_eq!(
                e_curr.end, e_next.start,
                "cycle break at edge {i}: {} != {}",
                e_curr.end, e_next.start
            );
        }
    }

    #[test]
    fn yr5_two_face_round_trip_produces_two_faces() {
        // two_face_shared_vertex_brep has 2 triangular faces sharing
        // only V0. Fan-tri: 1 tri per face = 2 output tris with
        // different attributions (F0 vs F1). PR-YR5 should produce 2
        // BRepFaces.
        let a = two_face_shared_vertex_brep();
        let b = two_face_shared_vertex_brep();
        let mock = MockBackend::ok(a.as_mesh().clone());
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(r.faces().len(), 2, "expected 2 BRepFaces");
        // Each face is a triangle with 3 edges.
        for f in r.faces() {
            assert_eq!(f.outer_loop.len(), 3);
        }
    }

    #[test]
    fn yr5_disconnected_components_become_separate_faces() {
        // Two output triangles with the SAME attribution but NO shared
        // vertex → flood-fill leaves them as 2 patches → 2 faces.
        // Regression guard vs. naive attribution-bucketing.
        let a = triangle_brep();
        let b = triangle_brep();
        // Mock returns 6 vertices = TWO copies of A's 3 verts at distinct
        // indices, and 2 disjoint triangles.
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
        let mock = MockBackend::ok(dup);
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        // Both tris attribute to (A, F0), but they share no vertex
        // index → connectivity flood-fill keeps them separate.
        assert_eq!(
            r.faces().len(),
            2,
            "disconnected same-attribution tris should be separate faces"
        );
    }

    #[test]
    fn yr5_none_attributed_tris_omitted_from_faces() {
        // Mock returns 2 tris: tri 0 matches A's verts (Some(A, F0)),
        // tri 1 is all novel coords (None). Output has 1 face.
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
        let mock = MockBackend::ok(mixed);
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(
            r.faces().len(),
            1,
            "None-attributed tris should not contribute faces"
        );
    }

    #[test]
    fn yr5_vertex_count_matches_mesh() {
        let a = triangle_brep();
        let b = triangle_brep();
        let mock = MockBackend::ok(a.as_mesh().clone());
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(r.vertices().len(), r.as_mesh().num_verts());
        for i in 0..r.vertices().len() {
            assert_eq!(r.vertices()[i].point, r.as_mesh().verts[i]);
        }
    }

    #[test]
    fn yr5_surface_inherited_from_input() {
        let a = triangle_brep();
        let b = triangle_brep();
        let mock = MockBackend::ok(a.as_mesh().clone());
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(r.faces().len(), 1);
        assert_eq!(
            r.faces()[0].surface,
            a.faces()[0].surface,
            "output face should inherit input A's surface"
        );
    }

    #[test]
    fn yr5_empty_input_produces_empty_face_set() {
        // Both inputs from_mesh → all-None attribution → no faces.
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let mock = MockBackend::ok(sample_mesh());
        let r = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert!(
            r.faces().is_empty(),
            "all-None attribution should yield empty faces"
        );
        assert!(
            r.edges().is_empty(),
            "all-None attribution should yield empty edges"
        );
        // Vertices still populated 1:1 with mesh.
        assert_eq!(r.vertices().len(), r.as_mesh().num_verts());
    }
}
