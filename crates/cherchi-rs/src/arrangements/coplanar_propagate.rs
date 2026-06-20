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

use std::collections::BTreeMap;

use crate::arrangements::aux_structure::exact_point_coords;
use crate::arrangements::fast_trimesh::VertexCoords;
use crate::arrangements::{
    CoplanarAdjacency, FastTrimesh, IntersectionVertex, PairClassification, Plane,
    TriangleAuxPoints, TypedPoint,
};
use crate::predicates::indirect::{GenericPoint3D, Sign as IpSign};
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

    // Inside-or-on? (boundary-inclusive). If strictly outside, drop.
    if !generic_inside_or_on(soup.ref_plane(), gp, &ce) {
        return;
    }

    // On a single edge? edge i connects corners i and (i+1)%3.
    for (i, _) in ce.iter().enumerate() {
        let a = &ce[i];
        let b = &ce[(i + 1) % 3];
        if dispatch(soup.ref_plane(), a, b, gp) == IpSign::Zero {
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
    let plane = soup.ref_plane();
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

#[cfg(test)]
mod tests;
