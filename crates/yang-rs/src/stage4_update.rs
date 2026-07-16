//! Stage-4 mesh updating (Yang 2025 §4.4.1, Fig 11) — the parametric-domain
//! primitive.
//!
//! Closes deviation **N2** (`docs/yang_deviations.md`) increment 1. Stage 4
//! today *relocates* intersection vertices onto the exact curve; the paper
//! additionally **updates the mesh**: it inserts each intersection polyline into
//! the affected patch as a constrained boundary and re-triangulates via CDT so
//! the trimmed patch stays bijective with the trimmed surface and contains no
//! flipped / sliver triangles.
//!
//! This module implements the paper's three quality-control operations —
//! `split`, `merge`, `insert` (Fig 11 a–c) — over a patch expressed in a 2D
//! **parametric domain** ("The triangulation can be totally operated in the
//! parametric domain", §4.4.1). The CDT itself is
//! [`cherchi_rs::cdt_with_interior_constraints`].
//!
//! Spec: `specs/n2_stage4_mesh_updating.md`. Scope of increment 1: the pure
//! primitive, unit-tested in isolation — **not** wired into
//! `stage4_relocate_and_correct` (that is N2-3), and no `d(T)` recompute (N2-2).

use cad_primitives::Point2;
use cherchi_rs::{cdt_with_interior_constraints, CdtError};

/// A mesh patch in a 2D parametric domain, already triangulated upstream (Stage
/// 1). Only its boundary topology is needed to re-triangulate with an inserted
/// intersection curve.
#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
    /// Parametric-domain vertex pool.
    pub verts: Vec<Point2>,
    /// Outer boundary loop, CCW, indices into `verts`.
    pub boundary: Vec<u32>,
    /// Inner boundary loops (existing holes), indices into `verts`.
    pub holes: Vec<Vec<u32>>,
}

/// An intersection polyline in the SAME parametric domain as its [`Patch`].
#[derive(Debug, Clone, PartialEq)]
pub struct Polyline {
    /// Ordered intersection points.
    pub points: Vec<Point2>,
    /// `true` iff the polyline is a closed loop (last connects to first).
    pub closed: bool,
}

/// Mesh-update tolerances (spec §2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshUpdateOpts {
    /// Fig 11(b/c): a patch vertex nearer than this to a polyline point is
    /// merged into it. Must be in `(0, d_eps)`.
    pub merge_tol: f64,
    /// The Stage-1 chord budget (`stage4_chord_band`); a polyline point farther
    /// than this from the patch region is not this patch's crossing.
    pub d_eps: f64,
}

/// The re-triangulated patch (spec §1).
#[derive(Debug, Clone, PartialEq)]
pub struct PatchUpdate {
    /// The updated vertex pool (patch verts, some moved onto the curve by a
    /// merge, plus appended non-merged polyline points and any Fig-11 insert
    /// point).
    pub verts: Vec<Point2>,
    /// Triangles indexing `verts`.
    pub tris: Vec<[u32; 3]>,
}

/// Why a mesh update failed (spec §6). Every variant is a P9/P10 LOUD stop.
#[derive(Debug, Clone, PartialEq)]
pub enum MeshUpdateError {
    /// Fewer than 2 points (open) / 3 (closed), or consecutive coincident points.
    DegeneratePolyline,
    /// `merge_tol >= d_eps` or `merge_tol <= 0` (a merge could move a vertex off
    /// the curve budget).
    MergeTolTooLarge,
    /// Polyline point `point` lies farther than `d_eps` from the patch region.
    PolylineOffPatch { point: usize },
    /// The polyline (or a merged endpoint) conflicts with a boundary / another
    /// constraint (CDT crossing) — we never Steiner-split to resolve it.
    SelfIntersectingPolyline,
    /// The CDT backend rejected the constraints.
    CdtFailed(CdtError),
}

/// The nearest unclaimed boundary vertex to `q`, as `(index, dist²)`.
///
/// `boundary_set` is a `HashSet`, whose iteration order is seeded per instance,
/// so a `min` on distance ALONE would resolve an exact distance tie to whichever
/// vertex the seed happens to yield first — making [`stage4_mesh_update`]'s
/// output depend on the hash seed (a violation of its "pure and deterministic"
/// contract). The tie is broken deterministically on the **lowest vertex index**,
/// so the choice is seed-independent.
fn nearest_unclaimed_boundary_vertex(
    q: Point2,
    boundary_set: &std::collections::HashSet<u32>,
    claimed: &[bool],
    verts: &[Point2],
) -> Option<(usize, f64)> {
    boundary_set
        .iter()
        .map(|&i| i as usize)
        .filter(|&i| !claimed[i])
        .map(|i| (i, dist2(q, verts[i])))
        // Order by (dist², index): the index tie-break makes ties independent of
        // the HashSet's per-instance iteration order.
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap().then(a.0.cmp(&b.0)))
}

/// Update `patch`'s triangulation so `polyline` becomes a chain of constrained
/// edges, faithfully to Yang 2025 §4.4.1 (Fig 11 split / merge / insert).
///
/// Pure and deterministic. See the module docs and `specs/n2_stage4_mesh_updating.md`.
pub fn stage4_mesh_update(
    patch: &Patch,
    polyline: &Polyline,
    opts: MeshUpdateOpts,
) -> Result<PatchUpdate, MeshUpdateError> {
    // ---- 1. Validate tolerances + polyline (spec §6). -------------------
    if opts.merge_tol <= 0.0 || opts.merge_tol >= opts.d_eps {
        return Err(MeshUpdateError::MergeTolTooLarge);
    }
    let min_pts = if polyline.closed { 3 } else { 2 };
    if polyline.points.len() < min_pts {
        return Err(MeshUpdateError::DegeneratePolyline);
    }
    for w in polyline.points.windows(2) {
        if dist2(w[0], w[1]) == 0.0 {
            return Err(MeshUpdateError::DegeneratePolyline);
        }
    }
    if polyline.closed && dist2(polyline.points[0], *polyline.points.last().unwrap()) == 0.0 {
        return Err(MeshUpdateError::DegeneratePolyline);
    }

    // Outer + hole positions, for point-in-region and on-boundary tests.
    let outer_pts: Vec<Point2> = patch
        .boundary
        .iter()
        .map(|&i| patch.verts[i as usize])
        .collect();
    let hole_pts: Vec<Vec<Point2>> = patch
        .holes
        .iter()
        .map(|h| h.iter().map(|&i| patch.verts[i as usize]).collect())
        .collect();

    // Every polyline point must lie within `d_eps` of the patch region.
    for (i, &p) in polyline.points.iter().enumerate() {
        if region_distance(p, &outer_pts, &hole_pts) > opts.d_eps {
            return Err(MeshUpdateError::PolylineOffPatch { point: i });
        }
    }

    // ---- 2-3. Classify each polyline point + build the working pool. ----
    // Faithful §4.4.1 (Fig 11). Each intersection point falls into ONE case,
    // and EVERY case leaves the boundary polygon geometrically unchanged, so
    // total area is conserved exactly (spec I4) — no silent boundary reshaping
    // (P9/P10):
    //   * boundary-VERTEX merge (Fig 11 b/c) — within `merge_tol` of an existing
    //     boundary vertex → reuse it and KEEP it fixed (the curve point snaps
    //     onto the boundary vertex; the boundary does not move). Removes the
    //     near-coincident split-edge-endpoint sliver.
    //   * boundary-EDGE split (Fig 11 a) — within `merge_tol` of a boundary edge
    //     interior → PROJECT onto the edge and splice the foot into the loop. The
    //     foot lies on the original edge line, so the boundary is unchanged.
    //   * interior merge — within `merge_tol` of an interior patch vertex → move
    //     that vertex onto the curve point (interior re-partition, area-safe).
    //   * interior append — otherwise, a free interior curve vertex.
    let mut verts = patch.verts.clone();
    let boundary_set: std::collections::HashSet<u32> = patch
        .boundary
        .iter()
        .chain(patch.holes.iter().flatten())
        .copied()
        .collect();
    let mut claimed = vec![false; verts.len()];
    let mut poly_vidx: Vec<u32> = Vec::with_capacity(polyline.points.len());
    let mut outer = patch.boundary.clone();
    let mut holes = patch.holes.clone();
    let mut interior: Vec<u32> = Vec::new();
    // (host, edge_i, t, vidx) per boundary-edge splice.
    let mut splices: Vec<(Option<usize>, usize, f64, u32)> = Vec::new();
    let tol2 = opts.merge_tol * opts.merge_tol;
    for &q in &polyline.points {
        // Nearest unclaimed boundary vertex (positions never move → patch.verts).
        let bv = nearest_unclaimed_boundary_vertex(q, &boundary_set, &claimed, &patch.verts);
        // A boundary vertex within tol wins (Fig 11 sliver removal), fixed pos.
        if let Some((bi, bd)) = bv {
            if bd <= tol2 {
                claimed[bi] = true;
                poly_vidx.push(bi as u32);
                continue;
            }
        }
        // Nearest boundary edge (perpendicular foot strictly interior to the edge).
        let edge = nearest_boundary_edge(q, &outer, &holes, &verts).filter(|e| e.3 <= tol2);
        // Nearest unclaimed interior patch vertex.
        let iv = (0..patch.verts.len())
            .filter(|i| !claimed[*i] && !boundary_set.contains(&(*i as u32)))
            .map(|i| (i, dist2(q, patch.verts[i])))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .filter(|v| v.1 <= tol2);
        match (edge, iv) {
            // Edge split: splice the PROJECTED foot (on the edge line) — boundary
            // unchanged. Prefer the edge when it is at least as close as `iv`.
            (Some((host, ei, t, ed, proj)), iv2) if iv2.is_none_or(|(_, ivd)| ed <= ivd) => {
                let vidx = verts.len() as u32;
                verts.push(proj);
                splices.push((host, ei, t, vidx));
                poly_vidx.push(vidx);
            }
            // Interior merge: move the interior vertex onto the curve point.
            (_, Some((ivi, _))) => {
                verts[ivi] = q;
                claimed[ivi] = true;
                poly_vidx.push(ivi as u32);
            }
            // Free interior curve vertex.
            _ => {
                let vidx = verts.len() as u32;
                verts.push(q);
                interior.push(vidx);
                poly_vidx.push(vidx);
            }
        }
    }
    splice_into_loops(&mut outer, &mut holes, &splices);

    // ---- 4. Insert (Fig 11): a closed loop enclosing no patch vertex gets --
    //         one interior point at its centroid, so its interior sub-patch has
    //         a vertex (topology aligns with the trimmed surface).
    if polyline.closed {
        let loop_pts: Vec<Point2> = poly_vidx.iter().map(|&i| verts[i as usize]).collect();
        let encloses_patch_vert = (0..patch.verts.len()).any(|vi| {
            // Skip verts that are themselves on the loop.
            if poly_vidx.contains(&(vi as u32)) {
                return false;
            }
            point_in_polygon(patch.verts[vi], &loop_pts)
        });
        if !encloses_patch_vert {
            let c = centroid(&loop_pts);
            let ci = verts.len() as u32;
            verts.push(c);
            interior.push(ci);
        }
    }

    // ---- 5. Constraint edges = consecutive polyline segments. -----------
    let n = poly_vidx.len();
    let mut constraints: Vec<[u32; 2]> = Vec::new();
    let seg_count = if polyline.closed { n } else { n - 1 };
    for i in 0..seg_count {
        let a = poly_vidx[i];
        let b = poly_vidx[(i + 1) % n];
        if a != b {
            constraints.push([a, b]);
        }
    }

    // ---- 6. CDT. --------------------------------------------------------
    let tris = cdt_with_interior_constraints(&verts, &outer, &holes, &interior, &constraints)
        .map_err(|e| match e {
            // A constraint crossing the boundary / another constraint is a
            // self-intersecting polyline (we never Steiner-split, P9/P10).
            CdtError::TriangulationFailed => MeshUpdateError::SelfIntersectingPolyline,
            other => MeshUpdateError::CdtFailed(other),
        })?;

    Ok(PatchUpdate { verts, tris })
}

// ---------------------------------------------------------------------------
// 2D geometry helpers (parametric domain).
// ---------------------------------------------------------------------------

fn dist2(a: Point2, b: Point2) -> f64 {
    let dx = a.x() - b.x();
    let dy = a.y() - b.y();
    dx * dx + dy * dy
}

fn centroid(pts: &[Point2]) -> Point2 {
    let n = pts.len() as f64;
    let (sx, sy) = pts
        .iter()
        .fold((0.0, 0.0), |(sx, sy), p| (sx + p.x(), sy + p.y()));
    Point2::new(sx / n, sy / n)
}

/// Distance from `p` to segment `ab`, and the clamped projection parameter `t`.
fn point_segment(p: Point2, a: Point2, b: Point2) -> (f64, f64) {
    let (abx, aby) = (b.x() - a.x(), b.y() - a.y());
    let len2 = abx * abx + aby * aby;
    if len2 == 0.0 {
        return (dist2(p, a).sqrt(), 0.0);
    }
    let t = (((p.x() - a.x()) * abx + (p.y() - a.y()) * aby) / len2).clamp(0.0, 1.0);
    let proj = Point2::new(a.x() + t * abx, a.y() + t * aby);
    (dist2(p, proj).sqrt(), t)
}

/// Distance from `p` to the patch region: 0 if inside the outer loop and outside
/// every hole; otherwise the distance to the nearest boundary edge.
fn region_distance(p: Point2, outer: &[Point2], holes: &[Vec<Point2>]) -> f64 {
    let inside = point_in_polygon(p, outer) && !holes.iter().any(|h| point_in_polygon(p, h));
    if inside {
        return 0.0;
    }
    let mut best = f64::INFINITY;
    for loop_pts in std::iter::once(outer).chain(holes.iter().map(|h| h.as_slice())) {
        let m = loop_pts.len();
        for i in 0..m {
            let (d, _) = point_segment(p, loop_pts[i], loop_pts[(i + 1) % m]);
            best = best.min(d);
        }
    }
    best
}

/// Nearest boundary edge whose perpendicular foot of `q` is STRICTLY interior to
/// the edge (not at an endpoint — those are handled as boundary-vertex merges).
/// Returns `(host, edge_i, t, dist², foot)` where `host` is `None` for `outer`
/// or `Some(h)` for `holes[h]`. `dist²` is the squared perpendicular distance.
#[allow(clippy::type_complexity)]
fn nearest_boundary_edge(
    q: Point2,
    outer: &[u32],
    holes: &[Vec<u32>],
    verts: &[Point2],
) -> Option<(Option<usize>, usize, f64, f64, Point2)> {
    let mut best: Option<(Option<usize>, usize, f64, f64, Point2)> = None;
    let mut scan = |host: Option<usize>, loop_idx: &[u32]| {
        let m = loop_idx.len();
        for i in 0..m {
            let a = verts[loop_idx[i] as usize];
            let b = verts[loop_idx[(i + 1) % m] as usize];
            let (foot, t) = project_segment(q, a, b);
            // Endpoints are boundary-vertex merges, not edge splits.
            if t <= 1e-12 || t >= 1.0 - 1e-12 {
                continue;
            }
            let d2 = dist2(q, foot);
            if best.is_none_or(|(_, _, _, bd, _)| d2 < bd) {
                best = Some((host, i, t, d2, foot));
            }
        }
    };
    scan(None, outer);
    for (h, hole) in holes.iter().enumerate() {
        scan(Some(h), hole);
    }
    best
}

/// Perpendicular foot of `p` on segment `ab` (clamped to `[0,1]`) and its
/// along-edge parameter `t`.
fn project_segment(p: Point2, a: Point2, b: Point2) -> (Point2, f64) {
    let (abx, aby) = (b.x() - a.x(), b.y() - a.y());
    let len2 = abx * abx + aby * aby;
    if len2 == 0.0 {
        return (a, 0.0);
    }
    let t = (((p.x() - a.x()) * abx + (p.y() - a.y()) * aby) / len2).clamp(0.0, 1.0);
    (Point2::new(a.x() + t * abx, a.y() + t * aby), t)
}

/// Rebuild each loop, inserting spliced vertices in order along their host edge.
fn splice_into_loops(
    outer: &mut Vec<u32>,
    holes: &mut [Vec<u32>],
    splices: &[(Option<usize>, usize, f64, u32)],
) {
    let rebuild = |loop_idx: &mut Vec<u32>, host: Option<usize>| {
        let m = loop_idx.len();
        let mut out: Vec<u32> = Vec::with_capacity(m + splices.len());
        for (i, &v) in loop_idx.iter().enumerate() {
            out.push(v);
            // Collect splices hosted on edge i, ordered by t.
            let mut on_edge: Vec<(f64, u32)> = splices
                .iter()
                .filter(|(hh, ei, _, _)| *hh == host && *ei == i)
                .map(|(_, _, t, v)| (*t, *v))
                .collect();
            on_edge.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            for (_, v) in on_edge {
                out.push(v);
            }
        }
        *loop_idx = out;
    };
    rebuild(outer, None);
    for (h, hole) in holes.iter_mut().enumerate() {
        rebuild(hole, Some(h));
    }
}

/// Even-odd point-in-polygon (ray casting). Boundary case is unspecified but not
/// relied upon (we test containment of vertices strictly inside/outside).
fn point_in_polygon(p: Point2, poly: &[Point2]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let (px, py) = (p.x(), p.y());
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (poly[i].x(), poly[i].y());
        let (xj, yj) = (poly[j].x(), poly[j].y());
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_area(a: Point2, b: Point2, c: Point2) -> f64 {
        0.5 * ((b.x() - a.x()) * (c.y() - a.y()) - (c.x() - a.x()) * (b.y() - a.y()))
    }

    fn total_area(u: &PatchUpdate) -> f64 {
        u.tris
            .iter()
            .map(|t| {
                signed_area(
                    u.verts[t[0] as usize],
                    u.verts[t[1] as usize],
                    u.verts[t[2] as usize],
                )
            })
            .sum()
    }

    fn edge_present(u: &PatchUpdate, a: u32, b: u32) -> bool {
        u.tris.iter().any(|t| {
            let e = [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])];
            e.iter().any(|&(x, y)| (x, y) == (a, b) || (x, y) == (b, a))
        })
    }

    fn no_flips(u: &PatchUpdate) -> bool {
        let s: Vec<f64> = u
            .tris
            .iter()
            .map(|t| {
                signed_area(
                    u.verts[t[0] as usize],
                    u.verts[t[1] as usize],
                    u.verts[t[2] as usize],
                )
            })
            .collect();
        s.iter().all(|&x| x > 0.0) || s.iter().all(|&x| x < 0.0)
    }

    fn unit_square() -> Patch {
        Patch {
            verts: vec![
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(1.0, 1.0),
                Point2::new(0.0, 1.0),
            ],
            boundary: vec![0, 1, 2, 3],
            holes: vec![],
        }
    }

    // ---- Canonical: open chord splitting the square (spec §5). ----------
    #[test]
    fn open_chord_becomes_edge_and_conserves_area() {
        let patch = unit_square();
        // Horizontal chord across the middle, endpoints ON the left/right edges.
        let poly = Polyline {
            points: vec![Point2::new(0.0, 0.5), Point2::new(1.0, 0.5)],
            closed: false,
        };
        let opts = MeshUpdateOpts {
            merge_tol: 1e-3,
            d_eps: 1e-2,
        };
        let u = stage4_mesh_update(&patch, &poly, opts).unwrap();
        // The two endpoints were appended at indices 4 and 5.
        assert!(edge_present(&u, 4, 5), "chord must be an edge (I1)");
        assert!(no_flips(&u), "no flipped triangles (I2)");
        assert!(
            (total_area(&u).abs() - 1.0).abs() < 1e-9,
            "area conserved (I4)"
        );
        // I3: the four original boundary verts are still on the boundary — none
        // became interior. The chord endpoints (4,5) split the L/R edges.
        assert_eq!(u.verts.len(), 6, "4 original + 2 chord endpoints");
    }

    // ---- Merge branch (Fig 11 b/c; spec §5). ----------------------------
    #[test]
    fn merge_fuses_near_boundary_vertex() {
        // Square with an extra boundary vertex at (0.5, 0) on the bottom edge.
        let mut patch = unit_square();
        patch.verts.push(Point2::new(0.5, 0.0)); // idx 4
        patch.boundary = vec![0, 4, 1, 2, 3];
        // Open chord whose lower endpoint sits ε away from vertex 4.
        let poly = Polyline {
            points: vec![Point2::new(0.5 + 1e-6, 0.0), Point2::new(0.5, 1.0)],
            closed: false,
        };
        let opts = MeshUpdateOpts {
            merge_tol: 1e-3,
            d_eps: 1e-2,
        };
        let u = stage4_mesh_update(&patch, &poly, opts).unwrap();
        // I5: the near endpoint MERGED vertex 4 (net new verts = 1: only the top
        // endpoint was appended). The curve point snaps ONTO the boundary vertex,
        // which STAYS fixed at (0.5, 0) — the boundary does not move, so area is
        // exactly conserved (the adversary-caught silent-reshape fix).
        assert_eq!(
            u.verts.len(),
            6,
            "5 patch verts + 1 top endpoint (merge reused v4)"
        );
        assert_eq!(
            u.verts[4],
            Point2::new(0.5, 0.0),
            "boundary vertex 4 stays put; curve point snaps to it"
        );
        assert!(edge_present(&u, 4, 5), "chord edge v4-v5 present");
        assert!(no_flips(&u));
        assert!((total_area(&u).abs() - 1.0).abs() < 1e-9);
    }

    // ---- Mutation sanity: without merge_tol the endpoint would NOT fuse. --
    #[test]
    fn merge_requires_tolerance() {
        let mut patch = unit_square();
        patch.verts.push(Point2::new(0.5, 0.0));
        patch.boundary = vec![0, 4, 1, 2, 3];
        let poly = Polyline {
            points: vec![Point2::new(0.5 + 1e-6, 0.0), Point2::new(0.5, 1.0)],
            closed: false,
        };
        // merge_tol smaller than the 1e-6 gap → NO merge; the endpoint is a new
        // vertex spliced onto the bottom edge (7 verts total).
        let opts = MeshUpdateOpts {
            merge_tol: 1e-9,
            d_eps: 1e-2,
        };
        let u = stage4_mesh_update(&patch, &poly, opts).unwrap();
        assert_eq!(u.verts.len(), 7, "no merge: both endpoints appended");
        assert!(no_flips(&u));
    }

    // ---- Determinism: an exact distance tie between two boundary vertices
    //      must resolve identically on every call (the module docstring's
    //      "Pure and deterministic" contract). The boundary-vertex nearest
    //      search iterates a `HashSet`, whose iteration order is seeded per
    //      instance; a `min_by` on distance ALONE returns whichever tied vertex
    //      the seed happens to yield first. The fix breaks ties on the vertex
    //      index, so the choice is seed-independent (spec: lowest index wins).
    // ---- Determinism: an EXACT distance tie between two boundary vertices must
    //      resolve identically on every call. The nearest-vertex search iterates
    //      a `HashSet` whose order is seeded per instance; a distance-only `min`
    //      would return whichever tied vertex the seed yields first, breaking the
    //      "pure and deterministic" contract on `stage4_mesh_update`. The fix
    //      tie-breaks on the lowest vertex index. Each loop iteration builds a
    //      FRESH HashSet (fresh RandomState seed), so this samples many iteration
    //      orders in one run — pre-fix (distance-only min) the returned index
    //      flips ~50/50 and the assert fails; post-fix it is always the minimum.
    #[test]
    fn tie_resolves_to_lowest_index_across_seeds() {
        // v3 and v5 are at genuinely different positions but bit-for-bit
        // equidistant from q: their dist² terms are the same two values (0.01,
        // 0.0004) summed in either order, so IEEE addition makes them exactly
        // equal. v3 has the lower index and must always win.
        let verts = [
            Point2::new(9.0, 9.0),  // 0  (far)
            Point2::new(9.0, -9.0), // 1  (far)
            Point2::new(-9.0, 9.0), // 2  (far)
            Point2::new(0.1, 0.02), // 3  tie candidate, LOWER index
            Point2::new(-9.0, 0.0), // 4  (far)
            Point2::new(0.02, 0.1), // 5  tie candidate, higher index
        ];
        let q = Point2::new(0.0, 0.0);
        // dist²(q, v3) = 0.1² + 0.02² ; dist²(q, v5) = 0.02² + 0.1² — equal.
        assert_eq!(
            dist2(q, verts[3]),
            dist2(q, verts[5]),
            "must be an exact tie"
        );
        let claimed = vec![false; verts.len()];

        let mut winner = None;
        for _ in 0..128 {
            let mut set = std::collections::HashSet::new();
            for i in 0..verts.len() as u32 {
                set.insert(i);
            }
            let (idx, _) = nearest_unclaimed_boundary_vertex(q, &set, &claimed, &verts).unwrap();
            match winner {
                None => winner = Some(idx),
                Some(w) => assert_eq!(idx, w, "tie must resolve identically every call"),
            }
        }
        assert_eq!(winner, Some(3), "the lowest-index tied vertex wins");
    }

    // ---- Insert branch (Fig 11 insert; spec §5). ------------------------
    #[test]
    fn closed_empty_loop_gets_interior_point() {
        let patch = unit_square(); // 4 boundary verts, no interior
        let poly = Polyline {
            points: vec![
                Point2::new(0.3, 0.3),
                Point2::new(0.7, 0.3),
                Point2::new(0.5, 0.7),
            ],
            closed: true,
        };
        let opts = MeshUpdateOpts {
            merge_tol: 1e-3,
            d_eps: 1e-2,
        };
        let u = stage4_mesh_update(&patch, &poly, opts).unwrap();
        // I5: 4 boundary + 3 loop + 1 inserted centroid = 8.
        assert_eq!(u.verts.len(), 8, "one interior insert point added");
        assert!(edge_present(&u, 4, 5) && edge_present(&u, 5, 6) && edge_present(&u, 6, 4));
        assert!(no_flips(&u));
        assert!((total_area(&u).abs() - 1.0).abs() < 1e-9);
    }

    // ---- Determinism (I6). ----------------------------------------------
    #[test]
    fn deterministic() {
        let patch = unit_square();
        let poly = Polyline {
            points: vec![Point2::new(0.0, 0.5), Point2::new(1.0, 0.5)],
            closed: false,
        };
        let opts = MeshUpdateOpts {
            merge_tol: 1e-3,
            d_eps: 1e-2,
        };
        let a = stage4_mesh_update(&patch, &poly, opts).unwrap();
        let b = stage4_mesh_update(&patch, &poly, opts).unwrap();
        assert_eq!(a, b);
    }

    // =====================================================================
    // ADVERSARY probes (FIP §6). Each asserts spec §4 invariants explicitly.
    // =====================================================================

    /// Input-patch (outer-loop) area, the I4 reference.
    fn input_patch_area(patch: &Patch) -> f64 {
        let pts: Vec<Point2> = patch
            .boundary
            .iter()
            .map(|&i| patch.verts[i as usize])
            .collect();
        let mut a = 0.0;
        let m = pts.len();
        for i in 0..m {
            let p = pts[i];
            let q = pts[(i + 1) % m];
            a += p.x() * q.y() - q.x() * p.y();
        }
        // subtract holes
        for h in &patch.holes {
            let hp: Vec<Point2> = h.iter().map(|&i| patch.verts[i as usize]).collect();
            let mut ha = 0.0;
            let hm = hp.len();
            for i in 0..hm {
                let p = hp[i];
                let q = hp[(i + 1) % hm];
                ha += p.x() * q.y() - q.x() * p.y();
            }
            a -= ha.abs();
        }
        0.5 * a
    }

    fn assert_i2_no_flips(u: &PatchUpdate) {
        assert!(
            no_flips(u),
            "I2 violated: mixed winding signs in {:?}",
            u.tris
        );
    }

    fn assert_i4_area(u: &PatchUpdate, expect: f64) {
        let got = total_area(u).abs();
        assert!(
            (got - expect.abs()).abs() < 1e-9,
            "I4 violated: output area {got} != input area {} (delta {})",
            expect.abs(),
            (got - expect.abs()).abs()
        );
    }

    /// PROBE A (regression) — a curve point sitting `1e-4` PERPENDICULARLY off a
    /// boundary vertex used to drag that vertex into the interior, shrinking the
    /// outer-loop region (I4 violated, silent Ok — the FIP-adversary finding).
    /// The fix: a boundary-vertex merge KEEPS the vertex fixed (the curve point
    /// snaps onto it), so the boundary never moves and area is conserved exactly.
    #[test]
    fn probe_merge_moves_boundary_vertex_off_edge_breaks_area() {
        // Square with a bottom-edge vertex at (0.5, 0).
        let mut patch = unit_square();
        patch.verts.push(Point2::new(0.5, 0.0)); // idx 4
        patch.boundary = vec![0, 4, 1, 2, 3];
        let input_area = input_patch_area(&patch); // 1.0
                                                   // Chord endpoint sits 1e-4 ABOVE the bottom edge (perpendicular), well
                                                   // within merge_tol=1e-3 → merges v4 and drags it up into the interior.
        let poly = Polyline {
            points: vec![Point2::new(0.5, 1e-4), Point2::new(0.5, 1.0)],
            closed: false,
        };
        let opts = MeshUpdateOpts {
            merge_tol: 1e-3,
            d_eps: 1e-2,
        };
        match stage4_mesh_update(&patch, &poly, opts) {
            Ok(u) => {
                // If it returns Ok, area MUST be conserved (I4). It is not: the
                // boundary vertex moved perpendicular, so the region shrank.
                assert_i2_no_flips(&u);
                assert_i4_area(&u, input_area);
            }
            Err(e) => panic!("unexpected Err {e:?} (a loud reject would also be acceptable)"),
        }
    }

    /// PROBE B — two polyline points that both want to merge the SAME patch
    /// vertex. The 2nd finds it claimed and APPENDS a fresh point coincident (or
    /// near-coincident) with the 1st. Assert no coincident/degenerate output.
    #[test]
    fn probe_two_points_target_same_vertex() {
        // Interior patch vertex at (0.5,0.5). Two consecutive-ish polyline points
        // both within merge_tol of it, but NON-consecutive so the dup-check that
        // only compares consecutive pairs cannot see them.
        let mut patch = unit_square();
        patch.verts.push(Point2::new(0.5, 0.5)); // idx 4 interior
                                                 // boundary unchanged (0,1,2,3); vertex 4 is a free interior vertex.
        let opts = MeshUpdateOpts {
            merge_tol: 1e-2,
            d_eps: 1e-1,
        };
        // Open chord left->right passing NEAR the interior vertex twice.
        let poly = Polyline {
            points: vec![
                Point2::new(0.0, 0.5),        // on left edge
                Point2::new(0.5 - 1e-4, 0.5), // near interior v4
                Point2::new(0.5 + 1e-4, 0.5), // also near interior v4 (v4 already claimed)
                Point2::new(1.0, 0.5),        // on right edge
            ],
            closed: false,
        };
        match stage4_mesh_update(&patch, &poly, opts) {
            Ok(u) => {
                assert_i2_no_flips(&u);
                assert_i4_area(&u, 1.0);
                // No two DISTINCT output vertices may be coincident (would mean a
                // degenerate/zero-length edge lurks).
                for i in 0..u.verts.len() {
                    for j in (i + 1)..u.verts.len() {
                        let d = dist2(u.verts[i], u.verts[j]);
                        assert!(
                            d > 1e-24,
                            "coincident output verts {i}&{j}: {:?}",
                            u.verts[i]
                        );
                    }
                }
            }
            Err(_) => { /* a loud reject is acceptable */ }
        }
    }

    /// PROBE C — polyline endpoint landing EXACTLY on an existing boundary vertex
    /// (a corner, not mid-edge). Merge should fuse it; result stays valid.
    #[test]
    fn probe_endpoint_on_boundary_corner() {
        let patch = unit_square();
        let opts = MeshUpdateOpts {
            merge_tol: 1e-3,
            d_eps: 1e-2,
        };
        // chord from corner (0,0)=v0 exactly, to (1,1)=v2 exactly (the diagonal).
        let poly = Polyline {
            points: vec![Point2::new(0.0, 0.0), Point2::new(1.0, 1.0)],
            closed: false,
        };
        match stage4_mesh_update(&patch, &poly, opts) {
            Ok(u) => {
                assert_i2_no_flips(&u);
                assert_i4_area(&u, 1.0);
                // Both endpoints merged existing corners → no new verts.
                assert_eq!(u.verts.len(), 4, "corners reused, no appended verts");
                assert!(edge_present(&u, 0, 2), "diagonal 0-2 realized (I1)");
            }
            Err(e) => panic!("endpoint-on-corner should succeed, got {e:?}"),
        }
    }

    /// PROBE D — open chord with a genuine INTERIOR middle point (3 pts), ends on
    /// boundary. Every consecutive segment must be an edge (I1).
    #[test]
    fn probe_open_chord_interior_middle_point() {
        let patch = unit_square();
        let opts = MeshUpdateOpts {
            merge_tol: 1e-4,
            d_eps: 1e-2,
        };
        let poly = Polyline {
            points: vec![
                Point2::new(0.0, 0.5), // left edge
                Point2::new(0.5, 0.6), // interior kink
                Point2::new(1.0, 0.5), // right edge
            ],
            closed: false,
        };
        match stage4_mesh_update(&patch, &poly, opts) {
            Ok(u) => {
                assert_i2_no_flips(&u);
                assert_i4_area(&u, 1.0);
                // poly_vidx: 4,5,6 (all appended). Both segments must be edges.
                assert!(edge_present(&u, 4, 5), "I1 seg0 4-5 missing");
                assert!(edge_present(&u, 5, 6), "I1 seg1 5-6 missing");
            }
            Err(e) => panic!("interior-kink chord should succeed, got {e:?}"),
        }
    }

    /// PROBE E — concave (non-convex) patch boundary; chord across the notch.
    #[test]
    fn probe_concave_patch() {
        // Arrow/chevron concave hexagon.
        let patch = Patch {
            verts: vec![
                Point2::new(0.0, 0.0), // 0
                Point2::new(2.0, 0.0), // 1
                Point2::new(2.0, 2.0), // 2
                Point2::new(1.0, 1.0), // 3 re-entrant
                Point2::new(0.0, 2.0), // 4
            ],
            boundary: vec![0, 1, 2, 3, 4],
            holes: vec![],
        };
        let input_area = input_patch_area(&patch);
        let opts = MeshUpdateOpts {
            merge_tol: 1e-4,
            d_eps: 1e-2,
        };
        // horizontal chord low across the solid part, endpoints on left/right edge
        let poly = Polyline {
            points: vec![Point2::new(0.0, 0.5), Point2::new(2.0, 0.5)],
            closed: false,
        };
        match stage4_mesh_update(&patch, &poly, opts) {
            Ok(u) => {
                assert_i2_no_flips(&u);
                assert_i4_area(&u, input_area);
                assert!(edge_present(&u, 5, 6), "chord edge missing (I1)");
            }
            Err(e) => panic!("concave chord should succeed, got {e:?}"),
        }
    }

    /// PROBE F — closed loop that DOES enclose a patch interior vertex → insert
    /// must NOT fire (I5: no interior point added).
    #[test]
    fn probe_closed_loop_encloses_interior_vertex_no_insert() {
        let mut patch = unit_square();
        patch.verts.push(Point2::new(0.5, 0.5)); // idx 4 interior, no loop
        let opts = MeshUpdateOpts {
            merge_tol: 1e-4,
            d_eps: 1e-2,
        };
        // Big triangular loop around the interior vertex.
        let poly = Polyline {
            points: vec![
                Point2::new(0.2, 0.2),
                Point2::new(0.8, 0.2),
                Point2::new(0.5, 0.9),
            ],
            closed: true,
        };
        match stage4_mesh_update(&patch, &poly, opts) {
            Ok(u) => {
                assert_i2_no_flips(&u);
                assert_i4_area(&u, 1.0);
                // 5 patch verts + 3 loop pts = 8; NO insert point (encloses v4).
                assert_eq!(u.verts.len(), 8, "insert must NOT fire (loop encloses v4)");
                assert!(edge_present(&u, 5, 6) && edge_present(&u, 6, 7) && edge_present(&u, 7, 5));
            }
            Err(e) => panic!("enclosing loop should succeed, got {e:?}"),
        }
    }

    /// PROBE G — self-crossing (figure-8) open polyline → expect a loud Err, NEVER
    /// a bad mesh.
    #[test]
    fn probe_self_crossing_polyline() {
        let patch = unit_square();
        let opts = MeshUpdateOpts {
            merge_tol: 1e-4,
            d_eps: 1e-2,
        };
        // A bowtie: the two segments cross. Endpoints on boundary, middle interior.
        let poly = Polyline {
            points: vec![
                Point2::new(0.0, 0.2),
                Point2::new(1.0, 0.8),
                Point2::new(0.0, 0.8),
                Point2::new(1.0, 0.2),
            ],
            closed: false,
        };
        match stage4_mesh_update(&patch, &poly, opts) {
            Ok(u) => {
                // If it insists on Ok, at minimum it must be a VALID mesh.
                assert_i2_no_flips(&u);
                assert_i4_area(&u, 1.0);
            }
            Err(MeshUpdateError::SelfIntersectingPolyline) => { /* correct */ }
            Err(e) => panic!("unexpected error variant {e:?}"),
        }
    }

    /// PROBE H — a closed loop whose edges cross an existing hole boundary.
    #[test]
    fn probe_loop_crosses_hole() {
        // 4x4 square with a central 1x1 hole.
        let patch = Patch {
            verts: vec![
                Point2::new(0.0, 0.0), // 0
                Point2::new(4.0, 0.0), // 1
                Point2::new(4.0, 4.0), // 2
                Point2::new(0.0, 4.0), // 3
                Point2::new(1.5, 1.5), // 4 hole
                Point2::new(2.5, 1.5), // 5 hole
                Point2::new(2.5, 2.5), // 6 hole
                Point2::new(1.5, 2.5), // 7 hole
            ],
            boundary: vec![0, 1, 2, 3],
            holes: vec![vec![4, 5, 6, 7]],
        };
        let opts = MeshUpdateOpts {
            merge_tol: 1e-4,
            d_eps: 1e-2,
        };
        // A loop that straddles the hole boundary (partly inside the hole).
        let poly = Polyline {
            points: vec![
                Point2::new(2.0, 0.5),
                Point2::new(3.5, 2.0),
                Point2::new(2.0, 2.0), // inside the hole region
            ],
            closed: true,
        };
        match stage4_mesh_update(&patch, &poly, opts) {
            Ok(u) => {
                assert_i2_no_flips(&u);
                // area must equal outer(16) - hole(1) = 15
                assert_i4_area(&u, 15.0);
            }
            Err(_) => { /* loud reject acceptable */ }
        }
    }

    /// PROBE I — extreme magnitudes (large coords) + tiny tolerances.
    #[test]
    fn probe_extreme_magnitude() {
        let s = 1e6_f64;
        let patch = Patch {
            verts: vec![
                Point2::new(0.0, 0.0),
                Point2::new(s, 0.0),
                Point2::new(s, s),
                Point2::new(0.0, s),
            ],
            boundary: vec![0, 1, 2, 3],
            holes: vec![],
        };
        let opts = MeshUpdateOpts {
            merge_tol: 1.0,
            d_eps: 10.0,
        };
        let poly = Polyline {
            points: vec![Point2::new(0.0, s / 2.0), Point2::new(s, s / 2.0)],
            closed: false,
        };
        match stage4_mesh_update(&patch, &poly, opts) {
            Ok(u) => {
                assert_i2_no_flips(&u);
                assert_i4_area(&u, s * s);
                assert!(edge_present(&u, 4, 5), "chord realized at scale (I1)");
            }
            Err(e) => panic!("large-scale chord should succeed, got {e:?}"),
        }
    }

    /// PROBE J — determinism under REORDERED input: reversing an open polyline
    /// should give the same triangulated region (same vertex SET & area). The raw
    /// PatchUpdate may differ in index order, so compare area + winding only.
    #[test]
    fn probe_determinism_reordered() {
        let patch = unit_square();
        let opts = MeshUpdateOpts {
            merge_tol: 1e-4,
            d_eps: 1e-2,
        };
        let fwd = Polyline {
            points: vec![Point2::new(0.0, 0.5), Point2::new(1.0, 0.5)],
            closed: false,
        };
        let rev = Polyline {
            points: vec![Point2::new(1.0, 0.5), Point2::new(0.0, 0.5)],
            closed: false,
        };
        let a = stage4_mesh_update(&patch, &fwd, opts).unwrap();
        let b = stage4_mesh_update(&patch, &rev, opts).unwrap();
        assert_i4_area(&a, 1.0);
        assert_i4_area(&b, 1.0);
        assert_i2_no_flips(&a);
        assert_i2_no_flips(&b);
    }

    /// PROBE K — a polyline that CROSSES the outer boundary (exits the patch and
    /// re-enters). A constraint edge crossing the boundary constraint must be a
    /// loud Err, not a silently clipped mesh.
    #[test]
    fn probe_polyline_crosses_boundary() {
        let patch = unit_square();
        let opts = MeshUpdateOpts {
            merge_tol: 1e-4,
            d_eps: 2.0,
        };
        // Middle point sits OUTSIDE the square (but within d_eps), so the chain
        // pierces the boundary twice.
        let poly = Polyline {
            points: vec![
                Point2::new(0.5, 0.5),
                Point2::new(1.5, 0.5), // outside, within d_eps
                Point2::new(0.5, 0.9),
            ],
            closed: false,
        };
        match stage4_mesh_update(&patch, &poly, opts) {
            Ok(u) => {
                // If Ok, the mesh must still be valid & cover exactly the square.
                assert_i2_no_flips(&u);
                assert_i4_area(&u, 1.0);
            }
            Err(MeshUpdateError::SelfIntersectingPolyline) => {}
            Err(e) => panic!("unexpected error {e:?}"),
        }
    }

    // ---- Failure modes (spec §6). ---------------------------------------
    #[test]
    fn rejects_bad_merge_tol() {
        let patch = unit_square();
        let poly = Polyline {
            points: vec![Point2::new(0.0, 0.5), Point2::new(1.0, 0.5)],
            closed: false,
        };
        assert_eq!(
            stage4_mesh_update(
                &patch,
                &poly,
                MeshUpdateOpts {
                    merge_tol: 1e-2,
                    d_eps: 1e-2
                }
            ),
            Err(MeshUpdateError::MergeTolTooLarge)
        );
        assert_eq!(
            stage4_mesh_update(
                &patch,
                &poly,
                MeshUpdateOpts {
                    merge_tol: 0.0,
                    d_eps: 1e-2
                }
            ),
            Err(MeshUpdateError::MergeTolTooLarge)
        );
    }

    #[test]
    fn rejects_degenerate_polyline() {
        let patch = unit_square();
        let opts = MeshUpdateOpts {
            merge_tol: 1e-3,
            d_eps: 1e-2,
        };
        let one = Polyline {
            points: vec![Point2::new(0.0, 0.5)],
            closed: false,
        };
        assert_eq!(
            stage4_mesh_update(&patch, &one, opts),
            Err(MeshUpdateError::DegeneratePolyline)
        );
        let dup = Polyline {
            points: vec![Point2::new(0.0, 0.5), Point2::new(0.0, 0.5)],
            closed: false,
        };
        assert_eq!(
            stage4_mesh_update(&patch, &dup, opts),
            Err(MeshUpdateError::DegeneratePolyline)
        );
    }

    #[test]
    fn rejects_off_patch_point() {
        let patch = unit_square();
        let poly = Polyline {
            points: vec![Point2::new(0.5, 0.5), Point2::new(5.0, 5.0)],
            closed: false,
        };
        let opts = MeshUpdateOpts {
            merge_tol: 1e-3,
            d_eps: 1e-2,
        };
        assert_eq!(
            stage4_mesh_update(&patch, &poly, opts),
            Err(MeshUpdateError::PolylineOffPatch { point: 1 })
        );
    }

    // ==== #169 Phase A — two-sided conformality principle ====================
    //
    // The mesh-update epic's linchpin (spec `yang_mesh_updating_epic.md` §3): when
    // an intersection curve is re-inserted into the two ADJACENT patches (one per
    // operand), the two sides must realize the SAME seam-vertex chain along the
    // curve, or the reassembled mesh is non-manifold (the wall that stalled #168
    // §5c.8 and #137 part-b). These tests pin the design principle at the primitive
    // level: driving both patches from ONE shared curve keeps them conformal;
    // reconstructing the curve independently per side diverges.

    /// The seam positions a patch realizes for `poly` = the update vertices that
    /// coincide (within `tol`) with a polyline point, as a sorted, rounded set.
    fn seam_positions(u: &PatchUpdate, poly: &Polyline, tol: f64) -> Vec<(i64, i64)> {
        let q = 1.0 / tol;
        let key = |p: Point2| ((p.x() * q).round() as i64, (p.y() * q).round() as i64);
        let mut s: Vec<(i64, i64)> = u
            .verts
            .iter()
            .filter(|&&v| poly.points.iter().any(|&p| dist2(v, p) <= tol * tol))
            .map(|&v| key(v))
            .collect();
        s.sort_unstable();
        s.dedup();
        s
    }

    /// GREEN: two GENUINELY DIFFERENT patches (a plain square vs. one with extra
    /// boundary density + an interior vertex) that share the SAME intersection
    /// chord both realize that chord as a connected edge at IDENTICAL positions —
    /// the differing interiors do not perturb the seam. This is the property that,
    /// applied to the two operands' patches, keeps the reassembled seam manifold.
    #[test]
    fn two_patches_sharing_one_curve_get_conformal_seam() {
        let opts = MeshUpdateOpts {
            merge_tol: 1e-3,
            d_eps: 1e-2,
        };
        // The shared intersection curve (in the common parametric frame both
        // patches are expressed in for this fixture): a horizontal chord y=0.5.
        let shared = Polyline {
            points: vec![Point2::new(0.0, 0.5), Point2::new(1.0, 0.5)],
            closed: false,
        };

        // Patch A: plain unit square.
        let a = stage4_mesh_update(&unit_square(), &shared, opts).unwrap();

        // Patch B: same outline, but a DIFFERENT triangulation seed — extra
        // boundary vertex on the top edge and a free interior vertex — so its
        // interior mesh differs from A's.
        let mut pb = unit_square();
        pb.verts.push(Point2::new(0.5, 1.0)); // 4: extra top-edge boundary vertex
        pb.verts.push(Point2::new(0.5, 0.75)); // 5: interior vertex
        pb.boundary = vec![0, 1, 2, 4, 3];
        let b = stage4_mesh_update(&pb, &shared, opts).unwrap();

        // Both realize the chord as an edge, and the seam-vertex position SET is
        // identical — the conformal-seam invariant.
        assert_eq!(
            seam_positions(&a, &shared, 1e-9),
            seam_positions(&b, &shared, 1e-9),
            "two patches sharing one curve must realize IDENTICAL seam positions"
        );
        assert!(no_flips(&a) && no_flips(&b));
    }

    /// The #168 failure mode, pinned: if the two sides reconstruct the curve
    /// INDEPENDENTLY and disagree on its vertices (here B inserts an extra
    /// collinear midpoint), the seam-vertex sets DIVERGE — the reassembled seam
    /// would be non-manifold (A has edge p0–p1; B has p0–m, m–p1). This is why
    /// Phase A must drive both patches from ONE shared curve-vertex identity set,
    /// never per-side reconstruction.
    #[test]
    fn independent_seam_reconstruction_diverges() {
        let opts = MeshUpdateOpts {
            merge_tol: 1e-3,
            d_eps: 1e-2,
        };
        let curve_a = Polyline {
            points: vec![Point2::new(0.0, 0.5), Point2::new(1.0, 0.5)],
            closed: false,
        };
        // Side B independently reconstructs the SAME geometric curve but with an
        // extra collinear sample — the divergence #168 hit after per-patch work.
        let curve_b = Polyline {
            points: vec![
                Point2::new(0.0, 0.5),
                Point2::new(0.5, 0.5),
                Point2::new(1.0, 0.5),
            ],
            closed: false,
        };
        let a = stage4_mesh_update(&unit_square(), &curve_a, opts).unwrap();
        let b = stage4_mesh_update(&unit_square(), &curve_b, opts).unwrap();
        assert_ne!(
            seam_positions(&a, &curve_a, 1e-9),
            seam_positions(&b, &curve_b, 1e-9),
            "independent per-side curve reconstruction must be detected as divergent"
        );
    }
}
