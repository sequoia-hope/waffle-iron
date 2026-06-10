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

pub use cad_primitives::{BoolOp, Point3, Vector3};
pub use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
pub use cherchi_rs::{Mesh, MeshBoolean};
#[cfg(feature = "native-boolean")]
pub use cherchi_rs::{NativeBoolean, NativeBooleanError};

/// Construct the PRODUCTION boolean backend: the native, in-process
/// cherchi-rs pipeline ([`NativeBoolean`]) — `mesh_arrangement` → labeling →
/// `keep_set(op)`. Reference parity vs the upstream C++ `mesh_booleans`
/// binary is the M6 gate (cherchi-rs `tests/parity_native_vs_sidecar.rs`);
/// the C++ subprocess sidecar (`cherchi-sidecar-rs`) is demoted to a
/// test-only parity oracle (PR-CR-BL3c).
///
/// Returns `None` when the build linked the no-op indirect-predicates FFI
/// stub (the Indirect_Predicates C++ source was missing at build time —
/// `scripts/build_sidecars.sh`, roadmap M0). In that build every predicate
/// returns garbage, so handing out a backend would produce silently-wrong
/// geometry (P9: fail loud / skip loud, never wrong-quietly). Tests use
/// `let Some(nb) = yang_rs::native_backend() else { /* skip */ }` exactly
/// like the old `SidecarBoolean::from_env()` self-skip.
///
/// Non-WASM until roadmap M7 (clean-room predicates); see the
/// `native-boolean` feature docs in Cargo.toml.
#[cfg(feature = "native-boolean")]
pub fn native_backend() -> Option<NativeBoolean> {
    if cherchi_rs::ffi_shim_available() {
        Some(NativeBoolean)
    } else {
        None
    }
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

        // Stage 1 tessellation (PR-YR7: planar Newell-fan + curved cylinder;
        // PR-YR12: sphere lat/long grid). Cone still rejects loudly.
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
        let circle_edges: Vec<(usize, Point3, Vector3, f64)> = edges
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match e.curve {
                Curve::Circle {
                    center,
                    normal,
                    radius,
                } if !sphere_seam_edges.contains(&(i as u32)) => Some((i, center, normal, radius)),
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
            let mut d_eps = curved_chord_bound(&edges).unwrap_or(0.0);
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
                    // Route non-convex (reflex-vertex) outer loops and any face
                    // with inner loops (holes) to the CDT path; convex,
                    // hole-free faces keep the existing byte-for-byte fan path.
                    // (PR-NC1: a fan is valid only for convex, hole-free
                    // polygons; CDT handles the rest with exact coverage and no
                    // Steiner points.)
                    let needs_cdt = !f.inner_loops.is_empty()
                        || planar_outer_loop_is_nonconvex(f, &edges, &out_verts, normal);

                    if needs_cdt {
                        tessellate_planar_cdt_face(
                            f_idx,
                            f,
                            &edges,
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
                Surface::Sphere { center, radius } => {
                    tessellate_sphere_face(
                        f_idx,
                        f,
                        &edges,
                        &verts,
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
                        &edges,
                        &rim_rings,
                        &verts,
                        apex,
                        axis_dir,
                        half_angle,
                        &mut out_verts,
                        &mut sources,
                        &mut out_tris,
                    )?;
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
/// non-convex. A near-zero cross (collinear vertices) is not reflex.
fn planar_outer_loop_is_nonconvex(
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
    if circle_edges.len() != 1 {
        return Err(YangError::MalformedTopology(format!(
            "face {f_idx}: cone lateral must be bounded by exactly one base-rim Circle edge, \
             found {} (a cone surface on a non-circular boundary is malformed topology)",
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
            "face {f_idx}: cone rim ring has {nseg} samples (< 3)"
        )));
    }

    // ---- Locate the pre-seeded apex mesh vertex by exact position match to the
    // cone's `apex` (within `TAU_MODEL`). The B-Rep verts are seeded 1:1 into
    // `out_verts` at the top of `BRep::new`, so a vertex's B-Rep index IS its
    // mesh index. REUSE it (no duplicate apex push → watertight). No match →
    // loud MalformedTopology.
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

    // ---- Apex fan: triangle (apex, ring[k], ring[(k+1) % N]); orient each
    // outward via the tilted cone normal.
    for k in 0..nseg {
        let mut tri = [apex_vi, ring[k], ring[(k + 1) % nseg]];
        let n = cone_outward_normal(out_verts, &tri, apex, axis_dir, half_angle);
        orient_tri(out_verts, &mut tri, n);
        out_tris.push(tri);
    }
    Ok(())
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
            return Err(YangError::NonManifoldOutput);
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
            return Err(YangError::NonManifoldOutput);
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
        {
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

        let mut matched_idx: Option<usize> = None;
        let mut matched = 0usize;
        for (i, curve) in returned.iter().enumerate() {
            if curve_contains_point(curve, p_s, tol, source_radius)
                && curve_contains_point(curve, p_e, tol, source_radius)
            {
                matched += 1;
                matched_idx = Some(i);
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
                Surface::Plane { .. } => Ok(cad_primitives::TAU_WORK),
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
            // inputs every hit is EXACT (planar tol == TAU_WORK), so this is
            // byte-for-byte the old "exactly one face within TAU_WORK" rule.
            let mut exact_hit: Option<u32> = None;
            let mut n_exact = 0usize;
            let mut band_hit: Option<u32> = None;
            let mut n_band = 0usize;
            for (fi, f) in input_brep.faces().iter().enumerate() {
                let d = plane_dist(fi, f)?;
                if d < tol_for(fi, f.surface)? {
                    if d < cad_primitives::TAU_WORK {
                        n_exact += 1;
                        if n_exact == 1 {
                            exact_hit = Some(fi as u32);
                        }
                    } else {
                        n_band += 1;
                        if n_band == 1 {
                            band_hit = Some(fi as u32);
                        }
                    }
                }
            }
            match (n_exact, exact_hit, n_band, band_hit) {
                (1, Some(fi), _, _) => fi, // unique exact-tier hit dominates
                (0, _, 1, Some(fi)) => fi, // no exact hit; unique band-tier hit
                _ => return Err(YangError::FaceResolutionFailed { tri: compact_t }),
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
    let (vertices, edges, faces, sources) =
        reconstruct_topology_stage4(&mut kept_submesh, &mut triangle_attribution, a, b, op)?;

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
///
/// PR-YR10: extended with a fourth component — the per-output-mesh-vertex
/// `Vec<TessellationSource>` (default `BRepVertex(i)`, overridden to
/// `BRepEdge { edge, t }` for Stage-4-relocated intersection vertices).
type ReconstructedTopology = (
    Vec<BRepVertex>,
    Vec<BRepEdge>,
    Vec<BRepFace>,
    Vec<TessellationSource>,
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
    input_curved_chord_bound(a).or_else(|| input_curved_chord_bound(b))
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
    let mut endpoints: Vec<u32> = Vec::new();
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
                    vert_circle.insert(v, (center, normal, radius, source_radius));
                    endpoints.push(v);
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
                            } => cyl = Some((axis_point, axis_dir, radius)),
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
                        };
                        for v in [s, e] {
                            vert_ellipse.insert(v, er);
                            endpoints.push(v);
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
                    // Neither cylinder+plane nor cone+plane: out of scope (e.g.
                    // sphere, or coplanar multi-solid). Loud STOP (P9/P10).
                    _ => {
                        return Err(YangError::Stage4RegionInvalid {
                            vertex: s,
                            reason: Stage4InvalidReason::LocalRefinementRequired,
                        });
                    }
                }
            }
            Curve::LineSegment => {}
        }
    }

    // A vertex shared by BOTH a circle and an ellipse edge (two distinct curves
    // through one vertex) is a genuine ambiguity — relocating it twice would be
    // wrong, so loud STOP rather than silently picking one (spec §4 no-skip
    // audit / P10).
    for v in vert_ellipse.keys() {
        if vert_circle.contains_key(v) {
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
        if vert_circle.contains_key(v) || vert_ellipse.contains_key(v) {
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

    // PR-YR11: ellipse relocation loop, mirroring the circle loop above. Closed
    // form via the cylinder parameterization (spec §2). Same `d_eps` chord band.
    for (&v, er) in &vert_ellipse {
        let p = mesh.verts[v as usize];
        let rho = ellipse_residual(p, er);
        if rho > d_eps {
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

    // No-skip audit (anti-disproven-attempt): every conic endpoint was handled.
    let relocation_keys: HashSet<u32> = relocations.iter().map(|&(v, _)| v).collect();
    let endpoint_set: HashSet<u32> = endpoints.iter().copied().collect();
    if processed != endpoint_set || processed != relocation_keys {
        return Err(YangError::Stage4RegionInvalid {
            vertex: u32::MAX,
            reason: Stage4InvalidReason::LocalRefinementRequired,
        });
    }

    // (3) §4.5.3 reversed-intersection correction sweep.
    let mut collapsed_any = false;
    let mut attr_vec = std::mem::take(&mut attribution.attributions);
    let sweep_result = sweep_reversed_intersections(mesh, &mut attr_vec, a, b, d_eps);
    attribution.attributions = attr_vec;
    let any_collapse = sweep_result?;
    collapsed_any |= any_collapse;

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
                    // Collapse the next point p_n onto p_r (remove + reconnect).
                    let dropped = collapse_vertex(mesh, attribution, p_n, p_r);
                    if dropped == 0 {
                        // Nothing collapsed ⇒ cannot make progress on this
                        // reversal by removing the next point. LOUD STOP.
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
    let (vertices, edges, faces, _sources) =
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
    let (mut infos, _incidence, mut intersection_curves) =
        compute_phase_a(mesh, attribution, a, b)?;

    // (4a) Stage 4 (seam A1): relocate onto the exact analytical curves
    // (Yang §4.4.1) + §4.5.3 reversal correction. Entered on ANY analytic conic
    // (Circle OR Ellipse) so an ellipse-only fixture reaches the loud
    // `EllipseProjectionUnsupported` STOP rather than silently passing an
    // un-relocated mesh. No conic edges ⇒ Stage 4 is a strict no-op (planar
    // byte-identity).
    // PR-YR22: include `Parabola` so a parabola-only fixture enters Stage 4 and
    // its cone-parabola seam is relocated onto the exact section.
    // PR-YR23: include `Hyperbola` likewise so a hyperbola edge enters Stage 4.
    let has_conic = intersection_curves.values().any(|c| {
        matches!(
            c,
            Curve::Circle { .. }
                | Curve::Ellipse { .. }
                | Curve::Parabola { .. }
                | Curve::Hyperbola { .. }
        )
    });
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
    for info in infos {
        let cycles = &info.cycles;
        let inherited = info.inherited;
        let face_idx = info.face_idx;

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
            Surface::Cylinder { .. } | Surface::Sphere { .. } | Surface::Cone { .. }
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
                    return Err(YangError::NonManifoldOutput);
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
                return Err(YangError::NonManifoldOutput);
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
                reversed: op == BoolOp::Subtract && info.input == InputId::B,
            });
            continue;
        }

        let (normal, d) = match inherited {
            Surface::Plane { normal, d } => (normal, d),
            // Cylinder, Sphere, and Cone are all handled by the curved branch
            // above (PR-YR17 added Cone), so these arms are unreachable-
            // defensive. Kept LOUD (P9) for any genuinely unexpected surface.
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

        // Empty-cycles guard (PR-CF1 defensive mirror of the curved branch):
        // a kept planar patch with no boundary cycle cannot form a bounded
        // face. Latent here (the all-planar fuzz never produces empty cycles)
        // but structurally identical to the curved-branch panic — the
        // `signed_areas[outer_idx]` / `cycles[outer_idx]` index below would
        // panic on the empty set. Mirrors the E2/E3 degenerate guards.
        if cycles.is_empty() {
            return Err(YangError::NonManifoldOutput);
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

    Ok((vertices, edges, faces, sources))
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

    // PR-YR10 N3 regression (Yang §4.5.3): a U-turn at p_r — consecutive points
    // double back so v1 ≈ −v2 ⇒ |t̃| ≈ 0 — IS a reversal. The paper places the
    // collinear/degenerate-t̃ case WITHIN the reversal subset ("directly detect
    // the reversal, avoiding the angle comparisons"). p_b=(0,0,0) → p_r=(1,0,0)
    // → p_n=(0.5,0,0) reverses direction (v1=+x, v2=−x, t̃=0). The degenerate
    // branch must report a reversal. (Was the N3 logic inversion: returned
    // `false` = "healthy", silently failing to correct the very reversal §4.5.3
    // exists for; reachable whenever relocation produces an out-of-order point.)
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
                reversed: false,
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
