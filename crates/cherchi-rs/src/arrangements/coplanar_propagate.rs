//! Ported from Cherchi et al. 2020 / 2022 (MIT).
//! © Gianmarco Cherchi et al.
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! https://github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! # PR-3 of the fully-coplanar arrangement port — `propagateCoplanarTrianglesIntersections`
//!
//! Faithful port of `propagateCoplanarTrianglesIntersections`
//! (`intersection_classification.cpp:788-829`) plus the per-coplanar-triangle
//! bucketing it consumes (the `edge2pts` / `tri2pts` / `tri2segs` population on
//! the coplanar path). Everything here is **standalone**: nothing in
//! [`super::soup::mesh_arrangement`] constructs or consumes it, so the live
//! arrangement output is byte-identical (corpus-neutral, exactly like PR-1).
//! PR-4 will wire {bucket → propagate → pocket emit} together and flip the
//! `Coplanar => continue` deferral.
//!
//! ## What the C++ propagate does (cpp:788)
//!
//! After all pairs are classified, for each triangle `t` that has coplanar
//! partners, and for each partner `copl_t`:
//!
//! - **points**: for each point `p` on each of `copl_t`'s three edges
//!   (`edgePointsList`), if `p` is NOT a corner of `t`
//!   (`!triContainsVert(t, p)`) AND `p` is `genericPointInsideTriangle(p, t,
//!   strict=true)` (STRICTLY inside `t`), copy `p` into `t`'s interior point
//!   list (`addVertexInTriangle(t, p)`).
//! - **segments**: for each segment `seg` in `copl_t`'s triangle-segment list
//!   (`triangleSegmentsList`), if BOTH endpoints are
//!   `genericPointInsideTriangle(.., t, strict=false)` (inside OR on boundary)
//!   AND at least one endpoint is NOT a corner of `t`
//!   (`!triContainsVert(t, seg.0) || !triContainsVert(t, seg.1)`), copy `seg`
//!   into `t`'s segment list (`addSegmentInTriangle(t, seg)`).
//!
//! Note the `true` / `false` strictness difference between the point loop and
//! the segment loop — mirrored exactly.
//!
//! ## The Rust data the propagate consumes
//!
//! [`bucket_coplanar_intersections`] turns the per-pair
//! [`PairClassification::Coplanar`] payloads (vertex set + symbolic segments)
//! into the same buckets the transversal path uses
//! ([`TriangleAuxPoints`] — `interior` + `edges[3]`) PLUS a per-triangle
//! interned-segment list (`Vec<Vec<(u32, u32)>>`). Point identity is GEOMETRIC
//! (exact rational coords), reusing [`super::aux_structure::exact_point_coords`]
//! exactly as the transversal interner does. This is kept SEPARATE from
//! [`super::group_intersection_points`] (whose `Coplanar => continue` arm is
//! untouched — neutrality depends on it).

use std::collections::{BTreeMap, BTreeSet};

use crate::arrangements::aux_structure::{exact_point_coords, ConstraintSegment};
use crate::arrangements::fast_trimesh::VertexCoords;
use crate::arrangements::{
    CoplanarAdjacency, FastTrimesh, IntersectionVertex, PairClassification, Plane,
    TriangleAuxPoints, TypedPoint,
};
use crate::predicates::indirect::{GenericPoint3D, Sign as IpSign};
use crate::predicates::{max_component_in_triangle_normal, Axis};
use cad_primitives::Point3;
use dashu::rational::RBig;

/// Build the coplanar adjacency ([`CoplanarAdjacency`], PR-1) from the
/// per-pair classification: for each [`PairClassification::Coplanar`] pair
/// `(ta, tb)` record `add_coplanar_triangles(ta, tb)`. Mirrors the C++
/// `addCoplanarTriangles` calls done while classifying the fully-coplanar
/// branch (`intersection_classification.cpp:141`).
///
/// Standalone — nothing in `mesh_arrangement` calls this (corpus-neutral).
pub fn build_coplanar_adjacency(
    soup: &FastTrimesh,
    classified: &[((u32, u32), PairClassification)],
) -> CoplanarAdjacency {
    let mut adj = CoplanarAdjacency::new(soup.num_tris());
    for ((ta, tb), classification) in classified {
        if matches!(classification, PairClassification::Coplanar { .. }) {
            adj.add_coplanar_triangles(*ta, *tb);
        }
    }
    adj
}

/// Per-triangle interned-segment lists (mirrors the C++ `tri2segs`): each entry
/// is a list of `(u32, u32)` interned-`TypedPoint`-id endpoint pairs for one
/// base triangle. Indexed by base-triangle id.
pub type TriangleSegments = Vec<Vec<(u32, u32)>>;

/// The bucketed coplanar data the propagate pass consumes (item 2 of PR-3).
pub struct CoplanarBuckets {
    /// Global geometrically-deduped typed-point set (mirrors the C++ global
    /// vertex list, keyed exactly like the transversal interner).
    pub points: Vec<TypedPoint>,
    /// Per-triangle point buckets (interior + 3 edges), indexed by base-tri id.
    pub buckets: Vec<TriangleAuxPoints>,
    /// Per-triangle symbolic-segment lists, indexed by base-tri id.
    pub tri_segments: TriangleSegments,
}

/// A geometric point interner local to the coplanar path. One id per EXACT
/// geometric point (keyed by exact rational coords); first-encountered
/// `VertexCoords` is the representative. Degenerate-generator points (no exact
/// coords) fall back to structural `VertexCoords` equality. This mirrors the
/// transversal `PointInterner` in `aux_structure.rs` (kept local here so the
/// coplanar path does not perturb the transversal one).
struct CoplanarInterner {
    points: Vec<TypedPoint>,
    by_exact: BTreeMap<[RBig; 3], u32>,
    structural_only: Vec<u32>,
}

impl CoplanarInterner {
    fn new() -> Self {
        CoplanarInterner {
            points: Vec::new(),
            by_exact: BTreeMap::new(),
            structural_only: Vec::new(),
        }
    }

    fn intern(&mut self, coords: VertexCoords) -> u32 {
        match exact_point_coords(&coords) {
            Some(xc) => {
                if let Some(&id) = self.by_exact.get(&xc) {
                    return id;
                }
                self.points.push(TypedPoint { coords });
                let id = (self.points.len() - 1) as u32;
                self.by_exact.insert(xc, id);
                id
            }
            None => {
                if let Some(&id) = self
                    .structural_only
                    .iter()
                    .find(|&&id| self.points[id as usize].coords == coords)
                {
                    return id;
                }
                self.points.push(TypedPoint { coords });
                let id = (self.points.len() - 1) as u32;
                self.structural_only.push(id);
                id
            }
        }
    }
}

/// The [`VertexCoords`] an [`IntersectionVertex`] interns as — identical to the
/// transversal mapping in `aux_structure.rs` (`Explicit → Explicit`,
/// `Lpi → Lpi`, `EdgeEdge → Lpi{ line = e, plane = [f0, f1, jolly] }`), so a
/// geometric point reached from both paths shares one exact-coordinate key.
fn coords_of(iv: &IntersectionVertex) -> VertexCoords {
    match iv {
        IntersectionVertex::Explicit { point, .. } => VertexCoords::Explicit(*point),
        IntersectionVertex::Lpi { line, plane, .. } => VertexCoords::Lpi {
            line: *line,
            plane: *plane,
        },
        IntersectionVertex::EdgeEdge { e, f, jolly, .. } => VertexCoords::Lpi {
            line: *e,
            plane: [f[0], f[1], *jolly],
        },
    }
}

/// A native generic point for an interned [`VertexCoords`], for the exact
/// inside tests. `Tpi` does not arise on the coplanar path (the coplanar
/// vertices are `Explicit` / `Lpi` / `EdgeEdge`→`Lpi`), but it is mapped for
/// totality.
fn generic_point_of(c: &VertexCoords) -> GenericPoint3D {
    match c {
        VertexCoords::Explicit(p) => GenericPoint3D::explicit(*p),
        VertexCoords::Lpi { line, plane } => {
            GenericPoint3D::lpi(line[0], line[1], plane[0], plane[1], plane[2])
        }
        VertexCoords::Tpi { v, w, u } => GenericPoint3D::tpi(*v, *w, *u),
    }
}

/// Bucket the fully-coplanar classifications into the propagate INPUT (item 2).
///
/// For every [`PairClassification::Coplanar`] pair, intern its `vertices`
/// (geometric dedup, exact coords) and:
/// - place each interned point into the per-triangle buckets of the triangle(s)
///   on which it lies (interior / edge), mirroring the C++ `addVertexInEdge`
///   (on-edge) / `addVertexInTriangle` (interior). A coplanar vertex lies on a
///   triangle's edge / interior iff it is geometrically there (boundary-
///   inclusive); corner-coincident points are dropped from that triangle (they
///   are already a vertex, introduce no split — same rule as the transversal
///   bucketer).
/// - collect each pair's symbolic `segments` (index pairs into the pair's
///   `vertices`, resolved to interned ids) into BOTH coplanar triangles'
///   segment lists, mirroring the C++ `addSymbolicSegment(.., tA, tB)` which
///   adds the segment to both `tA` and `tB` unless it is already a mesh edge of
///   that triangle (`!triContainsEdge`).
///
/// Returns the global interned set + per-triangle point buckets + per-triangle
/// segment lists. Standalone (corpus-neutral).
pub fn bucket_coplanar_intersections(
    soup: &FastTrimesh,
    classified: &[((u32, u32), PairClassification)],
) -> CoplanarBuckets {
    let mut interner = CoplanarInterner::new();
    let n = soup.num_tris() as usize;
    let mut buckets: Vec<TriangleAuxPoints> = vec![TriangleAuxPoints::default(); n];
    let mut tri_segments: TriangleSegments = vec![Vec::new(); n];

    for ((ta, tb), classification) in classified {
        let (vertices, segments) = match classification {
            PairClassification::Coplanar { vertices, segments } => (vertices, segments),
            _ => continue,
        };

        // Intern this pair's vertices; remember the pair-local → interned id
        // map so the segments (which index into `vertices`) can be resolved.
        let mut local_to_interned: Vec<u32> = Vec::with_capacity(vertices.len());
        for iv in vertices {
            let id = interner.intern(coords_of(iv));
            local_to_interned.push(id);
        }

        // A coplanar vertex lives on BOTH triangles (it is the overlap geometry
        // shared by `ta` and `tb`). Place each into both triangles' buckets,
        // classifying interior vs. on-edge vs. corner (dropped) per triangle.
        for (iv, &id) in vertices.iter().zip(local_to_interned.iter()) {
            let gp = generic_point_of(&coords_of(iv));
            place_in_triangle(soup, *ta, &gp, id, &mut buckets);
            place_in_triangle(soup, *tb, &gp, id, &mut buckets);
        }

        // Segments → both triangles' segment lists, skipping a triangle for
        // which the segment is already a mesh edge (both endpoints corners).
        for &(li, lj) in segments {
            let (ui, uj) = match (
                local_to_interned.get(li as usize),
                local_to_interned.get(lj as usize),
            ) {
                (Some(&a), Some(&b)) => (a, b),
                // An out-of-range segment index would be an upstream bug; skip
                // rather than panic (P9/P10) — neutrality is unaffected.
                _ => continue,
            };
            if ui == uj {
                continue; // geometric dedup collapsed the endpoints
            }
            for &t in &[*ta, *tb] {
                let a_corner = interned_is_corner_of(soup, t, ui, &interner);
                let b_corner = interned_is_corner_of(soup, t, uj, &interner);
                // C++ `!triContainsEdge(t, v0, v1)` = NOT (both corners).
                if a_corner && b_corner {
                    continue;
                }
                push_unique_seg(&mut tri_segments[t as usize], (ui, uj));
            }
        }
    }

    CoplanarBuckets {
        points: interner.points,
        buckets,
        tri_segments,
    }
}

/// Port of `propagateCoplanarTrianglesIntersections`
/// (`intersection_classification.cpp:788-829`).
///
/// For each triangle `t` with coplanar partners, and each partner `copl_t`:
/// - copy `copl_t`'s edge points that are STRICTLY inside `t` and not corners
///   of `t` into `t`'s interior bucket;
/// - copy `copl_t`'s segments whose BOTH endpoints are inside-or-on `t` and at
///   least one of which is not a corner of `t` into `t`'s segment list.
///
/// Mutates `buckets[t].interior` and `tri_segments[t]`. Standalone
/// (corpus-neutral).
pub fn propagate_coplanar_intersections(
    soup: &FastTrimesh,
    adjacency: &CoplanarAdjacency,
    points: &[TypedPoint],
    buckets: &mut [TriangleAuxPoints],
    tri_segments: &mut TriangleSegments,
) {
    let n = soup.num_tris();
    for t in 0..n {
        if !adjacency.triangle_has_coplanars(t) {
            continue;
        }
        // Collect the partner-sourced additions first, then apply — `buckets`
        // and `tri_segments` are read (the partner's data) and written (this
        // triangle's) in the same pass, so the additions cannot alias.
        let mut interior_adds: Vec<u32> = Vec::new();
        let mut seg_adds: Vec<(u32, u32)> = Vec::new();

        for &copl_t in adjacency.coplanar_triangles(t) {
            let cu = copl_t as usize;
            if cu >= buckets.len() || cu >= tri_segments.len() {
                continue;
            }
            // copl_t's edge points = the union of its three edge buckets
            // (mirrors edgePointsList over triEdgeID(copl_t, 0..3)).
            for edge_bucket in &buckets[cu].edges {
                for &p_id in edge_bucket {
                    if tri_contains_vert(soup, t, p_id, points) {
                        continue; // !triContainsVert(t, p)
                    }
                    if generic_point_inside_triangle(soup, t, p_id, points, true) {
                        interior_adds.push(p_id); // addVertexInTriangle(t, p)
                    }
                }
            }

            // copl_t's segments (triangleSegmentsList).
            for &(s0, s1) in &tri_segments[cu] {
                let in0 = generic_point_inside_triangle(soup, t, s0, points, false);
                let in1 = generic_point_inside_triangle(soup, t, s1, points, false);
                if !(in0 && in1) {
                    continue;
                }
                let c0 = tri_contains_vert(soup, t, s0, points);
                let c1 = tri_contains_vert(soup, t, s1, points);
                // (!triContainsVert(t, s0) || !triContainsVert(t, s1))
                if c0 && c1 {
                    continue;
                }
                seg_adds.push((s0, s1)); // addSegmentInTriangle(t, seg)
            }
        }

        let tu = t as usize;
        for p_id in interior_adds {
            push_unique(&mut buckets[tu].interior, p_id);
        }
        for seg in seg_adds {
            push_unique_seg(&mut tri_segments[tu], seg);
        }
    }
}

// =========================================================================
// Geometric helpers (exact, no tolerance)
// =========================================================================

/// Place interned point `id` (with native generic point `gp`) into triangle
/// `t`'s buckets: interior if strictly inside, on `edges[i]` if on edge `i`,
/// dropped if corner-coincident or strictly outside. Boundary-inclusive
/// classification (`addVertexInEdge` / `addVertexInTriangle`).
fn place_in_triangle(
    soup: &FastTrimesh,
    t: u32,
    gp: &GenericPoint3D,
    id: u32,
    buckets: &mut [TriangleAuxPoints],
) {
    let tu = t as usize;
    if tu >= buckets.len() {
        return;
    }
    let c = [
        soup.tri_vert(t, 0),
        soup.tri_vert(t, 1),
        soup.tri_vert(t, 2),
    ];
    let ce = [
        GenericPoint3D::explicit(c[0]),
        GenericPoint3D::explicit(c[1]),
        GenericPoint3D::explicit(c[2]),
    ];

    // Corner-coincident → already a vertex of `t`; introduces no split.
    for cv in &c {
        if gp_eq_explicit(gp, *cv) {
            return;
        }
    }

    let plane = triangle_ref_plane(soup, t);

    // Inside-or-on? (boundary-inclusive). If strictly outside, drop.
    if !generic_inside_or_on(plane, gp, &ce) {
        return;
    }

    // On a single edge? edge i connects corners i and (i+1)%3.
    for (i, _) in ce.iter().enumerate() {
        let a = &ce[i];
        let b = &ce[(i + 1) % 3];
        if dispatch(plane, a, b, gp) == IpSign::Zero {
            push_unique(&mut buckets[tu].edges[i], id);
            return;
        }
    }
    // Strictly inside.
    push_unique(&mut buckets[tu].interior, id);
}

/// `genericPointInsideTriangle(ts, p_id, t_id, strict)`
/// (`intersection_classification.cpp:929`): the three projected `orient2d`
/// signs must all be the SAME strict sign (`strict = true`) or all the same
/// sign allowing zeros (`strict = false`). Exact, projection chosen by the
/// soup's reference plane (the C++ `triPlane(t_id)`).
fn generic_point_inside_triangle(
    soup: &FastTrimesh,
    t: u32,
    p_id: u32,
    points: &[TypedPoint],
    strict: bool,
) -> bool {
    let p = match points.get(p_id as usize) {
        Some(tp) => generic_point_of(&tp.coords),
        None => return false,
    };
    let ce = [
        GenericPoint3D::explicit(soup.tri_vert(t, 0)),
        GenericPoint3D::explicit(soup.tri_vert(t, 1)),
        GenericPoint3D::explicit(soup.tri_vert(t, 2)),
    ];
    let plane = triangle_ref_plane(soup, t);
    if strict {
        generic_strictly_inside(plane, &p, &ce)
    } else {
        generic_inside_or_on(plane, &p, &ce)
    }
}

/// All three `orient2d(tv_i, tv_{i+1}, p)` strictly the same nonzero sign
/// (C++ `strict = true` branch). `Undefined` → false (never silent-wrong).
fn generic_strictly_inside(plane: Plane, p: &GenericPoint3D, ce: &[GenericPoint3D; 3]) -> bool {
    let s = [
        dispatch(plane, &ce[0], &ce[1], p),
        dispatch(plane, &ce[1], &ce[2], p),
        dispatch(plane, &ce[2], &ce[0], p),
    ];
    if s.contains(&IpSign::Undefined) {
        return false;
    }
    let all_pos = s.iter().all(|x| *x == IpSign::Positive);
    let all_neg = s.iter().all(|x| *x == IpSign::Negative);
    all_pos || all_neg
}

/// All three `orient2d` signs the same sign allowing zeros (C++
/// `strict = false` branch): all `>= 0` or all `<= 0`. `Undefined` → false.
fn generic_inside_or_on(plane: Plane, p: &GenericPoint3D, ce: &[GenericPoint3D; 3]) -> bool {
    let s = [
        dispatch(plane, &ce[0], &ce[1], p),
        dispatch(plane, &ce[1], &ce[2], p),
        dispatch(plane, &ce[2], &ce[0], p),
    ];
    if s.contains(&IpSign::Undefined) {
        return false;
    }
    let all_ge = s
        .iter()
        .all(|x| *x == IpSign::Positive || *x == IpSign::Zero);
    let all_le = s
        .iter()
        .all(|x| *x == IpSign::Negative || *x == IpSign::Zero);
    all_ge || all_le
}

/// `orient2d(a, b, p)` projected to `plane`, via the native indirect dispatch.
fn dispatch(plane: Plane, a: &GenericPoint3D, b: &GenericPoint3D, p: &GenericPoint3D) -> IpSign {
    crate::arrangements::gp_dispatch::dispatch_orient2d(plane, a, b, p)
}

/// The reference projection plane for base triangle `t` — drop its dominant-
/// normal axis. MUST match `soup::triangle_plane` (the plane the per-triangle
/// submesh + `split_single_triangle` use), so coplanar point classification
/// agrees with the split path. A vertical facet projects to XY as a zero-area
/// line, so `soup.ref_plane()` (always XY for the global container) is WRONG
/// for coplanar lateral faces — this picks the facet's own plane.
fn triangle_ref_plane(soup: &FastTrimesh, t: u32) -> Plane {
    let c0 = soup.tri_vert(t, 0);
    let c1 = soup.tri_vert(t, 1);
    let c2 = soup.tri_vert(t, 2);
    match max_component_in_triangle_normal(c0, c1, c2) {
        Axis::X => Plane::YZ,
        Axis::Y => Plane::ZX,
        Axis::Z => Plane::XY,
    }
}

/// True iff the native generic point `gp` equals the explicit point `q`
/// (exact). Uses the points' exact rational coordinates (the same geometric
/// identity the interner uses) so an `Lpi`/`EdgeEdge` that lands exactly on a
/// corner compares equal.
fn gp_eq_explicit(gp: &GenericPoint3D, q: Point3) -> bool {
    let qc = exact_point_coords(&VertexCoords::Explicit(q));
    let gc = match gp {
        GenericPoint3D::Explicit(p) => exact_point_coords(&VertexCoords::Explicit(*p)),
        GenericPoint3D::Lpi(l) => exact_point_coords(&VertexCoords::Lpi {
            line: [l.p, l.q],
            plane: [l.r, l.s, l.t],
        }),
        GenericPoint3D::Tpi(t) => exact_point_coords(&VertexCoords::Tpi {
            v: t.v,
            w: t.w,
            u: t.u,
        }),
    };
    match (gc, qc) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// `triContainsVert(t, p)`: is interned point `p_id` geometrically a corner of
/// triangle `t`? Mirrors the C++ predicate via exact coordinates.
fn tri_contains_vert(soup: &FastTrimesh, t: u32, p_id: u32, points: &[TypedPoint]) -> bool {
    let pc = match points
        .get(p_id as usize)
        .and_then(|tp| exact_point_coords(&tp.coords))
    {
        Some(c) => c,
        None => return false,
    };
    for off in 0..3 {
        if let Some(cc) = exact_point_coords(&VertexCoords::Explicit(soup.tri_vert(t, off))) {
            if cc == pc {
                return true;
            }
        }
    }
    false
}

/// Same as [`tri_contains_vert`] but reading the interned id's coords from the
/// in-progress [`CoplanarInterner`] (used by the bucketer before the final
/// `points` Vec exists).
fn interned_is_corner_of(soup: &FastTrimesh, t: u32, id: u32, interner: &CoplanarInterner) -> bool {
    let pc = match interner
        .points
        .get(id as usize)
        .and_then(|tp| exact_point_coords(&tp.coords))
    {
        Some(c) => c,
        None => return false,
    };
    for off in 0..3 {
        if let Some(cc) = exact_point_coords(&VertexCoords::Explicit(soup.tri_vert(t, off))) {
            if cc == pc {
                return true;
            }
        }
    }
    false
}

/// Place GLOBAL point id `id` (coords from `points`) into triangle `t`'s
/// `bucket`: interior if strictly inside, `edges[i]` if on edge `i`, dropped if
/// corner-coincident or strictly outside. Boundary-inclusive, exact — the
/// same classification as [`place_in_triangle`] but keyed on a global id /
/// `points` slice (used to guarantee coplanar segment endpoints become submesh
/// vertices). Idempotent (deduped by id).
fn place_global_in_triangle(
    soup: &FastTrimesh,
    t: u32,
    plane: Plane,
    id: u32,
    points: &[TypedPoint],
    bucket: &mut TriangleAuxPoints,
) {
    let gp = match points.get(id as usize) {
        Some(tp) => generic_point_of(&tp.coords),
        None => return,
    };
    let c = [
        soup.tri_vert(t, 0),
        soup.tri_vert(t, 1),
        soup.tri_vert(t, 2),
    ];
    let ce = [
        GenericPoint3D::explicit(c[0]),
        GenericPoint3D::explicit(c[1]),
        GenericPoint3D::explicit(c[2]),
    ];
    for cv in &c {
        if gp_eq_explicit(&gp, *cv) {
            return; // corner-coincident → already a vertex
        }
    }
    if !generic_inside_or_on(plane, &gp, &ce) {
        return; // strictly outside → not on this triangle
    }
    // Already present in interior or any edge bucket? (idempotent)
    if bucket.interior.contains(&id) || bucket.edges.iter().any(|e| e.contains(&id)) {
        return;
    }
    for (i, _) in ce.iter().enumerate() {
        let a = &ce[i];
        let b = &ce[(i + 1) % 3];
        if dispatch(plane, a, b, &gp) == IpSign::Zero {
            push_unique(&mut bucket.edges[i], id);
            return;
        }
    }
    push_unique(&mut bucket.interior, id);
}

fn push_unique(vec: &mut Vec<u32>, id: u32) {
    if !vec.contains(&id) {
        vec.push(id);
    }
}

/// Push a segment as the canonical (min, max) ordered pair, deduped (mirrors
/// the C++ `uniquePair` + `contains` in `addSegmentInTriangle`).
fn push_unique_seg(vec: &mut Vec<(u32, u32)>, seg: (u32, u32)) {
    let key = if seg.0 <= seg.1 {
        (seg.0, seg.1)
    } else {
        (seg.1, seg.0)
    };
    if !vec.contains(&key) {
        vec.push(key);
    }
}

// =========================================================================
// PR-4: pocket flood-fill (port of findPocketsInTriangle, triangulation.cpp:1269)
// =========================================================================

/// One pocket of a re-triangulated coplanar base triangle: the list of submesh
/// sub-triangle ids forming the pocket, plus the SUBMESH-LOCAL vertex ids on
/// its boundary polygon (the endpoints of every constraint/border edge the
/// flood-fill stopped at). `mesh_arrangement` welds the boundary ids to GLOBAL
/// ids to form the dedup key (so the two coplanar triangles' shared overlap
/// pocket hashes identically), and emits / OR-merges the sub-triangles.
pub struct TrianglePocket {
    /// Submesh sub-triangle ids in this pocket.
    pub sub_tris: Vec<u32>,
    /// Submesh-local boundary vertex ids (constraint/border edge endpoints).
    pub boundary_verts: BTreeSet<u32>,
}

/// Port of `findPocketsInTriangle` (`triangulation.cpp:1269-1314`).
///
/// Flood-fill the submesh sub-triangles into pockets: the flood STOPS at edges
/// where `edge_is_constr(e) || edge_is_boundary(e)` (those edges' endpoints
/// become the pocket's boundary polygon), and CROSSES every other interior
/// edge. The submesh here is the fully re-triangulated single base triangle
/// (post split + enforce), so the constraint edges are exactly the overlap-
/// boundary segments `enforce_constraints` flagged via `set_edge_constr`.
pub fn find_pockets_in_triangle(subm: &FastTrimesh) -> Vec<TrianglePocket> {
    let n = subm.num_tris();
    let mut visited = vec![false; n as usize];
    let mut pockets: Vec<TrianglePocket> = Vec::new();

    for seed in 0..n {
        if visited[seed as usize] {
            continue;
        }
        let mut sub_tris: Vec<u32> = Vec::new();
        let mut boundary: BTreeSet<u32> = BTreeSet::new();
        let mut stack: Vec<u32> = vec![seed];

        while let Some(curr) = stack.pop() {
            if visited[curr as usize] {
                continue;
            }
            visited[curr as usize] = true;
            sub_tris.push(curr);

            for e in subm.tri_edges(curr) {
                if subm.edge_is_constr(e) || subm.edge_is_boundary(e) {
                    boundary.insert(subm.edge_vert_id(e, 0));
                    boundary.insert(subm.edge_vert_id(e, 1));
                } else {
                    for &nbr in subm.adj_e2t(e) {
                        if nbr != curr && !visited[nbr as usize] {
                            stack.push(nbr);
                        }
                    }
                }
            }
        }

        pockets.push(TrianglePocket {
            sub_tris,
            boundary_verts: boundary,
        });
    }

    pockets
}

// =========================================================================
// PR-4: integrate the coplanar buckets/segments into the LIVE arrangement
// =========================================================================

/// The result of folding the coplanar path into the transversal arrangement
/// state, consumed by `soup::mesh_arrangement` step 9.
pub struct CoplanarIntegration {
    /// The coplanar adjacency — `triangle_has_coplanars(t)` selects the pocket
    /// emit path; `coplanar_triangles(t)` is unused downstream but kept whole.
    pub adjacency: CoplanarAdjacency,
    /// The set of base triangles that carry coplanar overlap geometry (those
    /// with a coplanar partner that contributed any point/segment). Step 9
    /// routes these through `solve_pockets_in_coplanar_triangle` instead of the
    /// plain per-sub-triangle emit.
    pub coplanar_tris: BTreeSet<u32>,
}

/// Wire the coplanar classification into the transversal `points` / `buckets` /
/// `segments_per_tri` so coplanar triangles enter the split path with their
/// overlap-boundary points + segments. This is the PR-4 step-(a) glue.
///
/// Concretely:
/// 1. build the coplanar adjacency + bucket the per-pair `Coplanar` payloads
///    ([`bucket_coplanar_intersections`]) + run the propagate pass
///    ([`propagate_coplanar_intersections`]) on the coplanar-local id space;
/// 2. MERGE the coplanar-local points into the GLOBAL `points` list by EXACT
///    geometric identity (reusing [`exact_point_coords`] — the same key the
///    transversal interner uses), building a coplanar→global id remap so a
///    point reached via both paths shares one global id;
/// 3. fold each coplanar triangle's remapped interior/edge point ids into the
///    global `buckets[t]`, and convert its symbolic segment list into
///    [`ConstraintSegment`]s appended to `segments_per_tri[t]` (with the
///    partner coplanar triangle's corners as the in-plane `source_tri`).
///
/// Returns the adjacency + the set of triangles that gained coplanar geometry.
/// `points`, `buckets`, `segments_per_tri` are mutated in place.
pub fn integrate_coplanar_into_arrangement(
    soup: &FastTrimesh,
    classified: &[((u32, u32), PairClassification)],
    points: &mut Vec<TypedPoint>,
    buckets: &mut [TriangleAuxPoints],
    segments_per_tri: &mut [Vec<ConstraintSegment>],
) -> CoplanarIntegration {
    let adjacency = build_coplanar_adjacency(soup, classified);

    // Steps 1: bucket + propagate on the coplanar-local id space.
    let mut cb = bucket_coplanar_intersections(soup, classified);
    propagate_coplanar_intersections(
        soup,
        &adjacency,
        &cb.points,
        &mut cb.buckets,
        &mut cb.tri_segments,
    );

    // Step 2: merge coplanar-local points into the GLOBAL `points` by exact
    // geometric identity. `remap[local_id] = global_id`.
    let mut by_exact: BTreeMap<[RBig; 3], u32> = BTreeMap::new();
    let mut structural: Vec<u32> = Vec::new(); // global ids without exact coords
    for (gid, tp) in points.iter().enumerate() {
        match exact_point_coords(&tp.coords) {
            Some(xc) => {
                by_exact.entry(xc).or_insert(gid as u32);
            }
            None => structural.push(gid as u32),
        }
    }
    let mut remap: Vec<u32> = Vec::with_capacity(cb.points.len());
    for tp in &cb.points {
        let gid = match exact_point_coords(&tp.coords) {
            Some(xc) => match by_exact.get(&xc) {
                Some(&g) => g,
                None => {
                    let g = points.len() as u32;
                    points.push(tp.clone());
                    by_exact.insert(xc, g);
                    g
                }
            },
            None => {
                // Degenerate-generator fallback: structural equality.
                match structural
                    .iter()
                    .find(|&&g| points[g as usize].coords == tp.coords)
                {
                    Some(&g) => g,
                    None => {
                        let g = points.len() as u32;
                        points.push(tp.clone());
                        structural.push(g);
                        g
                    }
                }
            }
        };
        remap.push(gid);
    }

    // Step 3: fold per-triangle buckets + segments into the global structures.
    let mut coplanar_tris: BTreeSet<u32> = BTreeSet::new();
    let n = soup.num_tris() as usize;
    for t in 0..n {
        if !adjacency.triangle_has_coplanars(t as u32) {
            continue;
        }
        let mut touched = false;

        // interior points.
        for &lid in &cb.buckets[t].interior {
            push_unique(&mut buckets[t].interior, remap[lid as usize]);
            touched = true;
        }
        // edge points.
        for (i, edge) in cb.buckets[t].edges.iter().enumerate() {
            for &lid in edge {
                push_unique(&mut buckets[t].edges[i], remap[lid as usize]);
                touched = true;
            }
        }

        // segments → ConstraintSegments. The segment lies in the coplanar
        // common plane; use a coplanar partner triangle's corners as the
        // in-plane `source_tri` (a plane through the overlap — the segment can
        // only meet other in-plane constraints at shared endpoints, a V-
        // junction, so no spurious TPI arises; the field is only consulted on
        // an interior crossing).
        let partner = adjacency
            .coplanar_triangles(t as u32)
            .first()
            .copied()
            .unwrap_or(t as u32);
        let src = [
            soup.tri_vert(partner, 0),
            soup.tri_vert(partner, 1),
            soup.tri_vert(partner, 2),
        ];
        for &(l0, l1) in &cb.tri_segments[t] {
            let (g0, g1) = (remap[l0 as usize], remap[l1 as usize]);
            if g0 == g1 {
                continue;
            }
            // Dedup against any existing (transversal or coplanar) segment with
            // the same geometric endpoint pair.
            let exists = segments_per_tri[t].iter().any(|s| {
                (s.endpoints.0 == g0 && s.endpoints.1 == g1)
                    || (s.endpoints.0 == g1 && s.endpoints.1 == g0)
            });
            if !exists {
                segments_per_tri[t].push(ConstraintSegment {
                    endpoints: (g0, g1),
                    source_tri: src,
                });
            }
            // Every segment endpoint MUST be a submesh vertex after split, or
            // `enforce_constraints` cannot resolve it (EndpointNotInSubmesh).
            // A propagated partner segment's endpoint may not have arrived via
            // the point buckets (the propagate pass copies only partner EDGE
            // points, but a segment endpoint can be a partner INTERIOR point or
            // an on-edge point), so place both endpoints into `buckets[t]` here
            // by their exact position on `t`.
            let plane_t = triangle_ref_plane(soup, t as u32);
            place_global_in_triangle(soup, t as u32, plane_t, g0, points, &mut buckets[t]);
            place_global_in_triangle(soup, t as u32, plane_t, g1, points, &mut buckets[t]);
            touched = true;
        }

        if touched {
            coplanar_tris.insert(t as u32);
        }
    }

    CoplanarIntegration {
        adjacency,
        coplanar_tris,
    }
}

#[cfg(test)]
mod tests;
