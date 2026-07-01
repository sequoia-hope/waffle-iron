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

    // ---- 2. Build the working vertex pool + merge (Fig 11 b/c). ---------
    // Start from the patch pool; polyline points either MERGE a nearby patch
    // vertex (reuse its index, move it onto the curve) or APPEND as new points.
    let mut verts = patch.verts.clone();
    let mut claimed = vec![false; verts.len()];
    // Index in `verts` for each polyline point.
    let mut poly_vidx: Vec<u32> = Vec::with_capacity(polyline.points.len());
    for &q in &polyline.points {
        // Nearest UNCLAIMED existing patch vertex.
        let mut best: Option<(usize, f64)> = None;
        for (vi, &vp) in verts.iter().enumerate().take(patch.verts.len()) {
            if claimed[vi] {
                continue;
            }
            let d = dist2(q, vp);
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((vi, d));
            }
        }
        match best {
            // Merge: p is too close to q → fuse p into q (place it AT q, the
            // exact-curve point is authoritative), reuse its index.
            Some((vi, d)) if d <= opts.merge_tol * opts.merge_tol => {
                verts[vi] = q;
                claimed[vi] = true;
                poly_vidx.push(vi as u32);
            }
            // No merge: q is a fresh vertex.
            _ => {
                poly_vidx.push(verts.len() as u32);
                verts.push(q);
            }
        }
    }

    // ---- 3. Split (Fig 11 a): splice on-boundary non-merged points into ---
    //         the loop that hosts them; the rest stay interior.
    let mut outer = patch.boundary.clone();
    let mut holes = patch.holes.clone();
    let mut interior: Vec<u32> = Vec::new();
    // A polyline point that MERGED a boundary vertex is already on the boundary.
    let boundary_set: std::collections::HashSet<u32> = outer
        .iter()
        .chain(holes.iter().flatten())
        .copied()
        .collect();
    // Collect splices per loop-edge, then rebuild loops with points ordered
    // along each host edge.
    // host = None => outer; Some(h) => holes[h].
    let mut splices: Vec<(Option<usize>, usize, f64, u32)> = Vec::new(); // (host, edge_i, t, vidx)
    for &vidx in &poly_vidx {
        if boundary_set.contains(&vidx) {
            continue; // merged onto an existing boundary vertex — already split.
        }
        let q = verts[vidx as usize];
        if let Some((host, edge_i, t)) =
            locate_on_boundary(q, &outer, &holes, &verts, opts.merge_tol)
        {
            splices.push((host, edge_i, t, vidx));
        } else {
            // Not on any boundary → interior curve vertex.
            interior.push(vidx);
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

/// If `q` lies within `tol` of a boundary edge (and not at a vertex), return the
/// host loop (`None` = outer, `Some(h)` = `holes[h]`), the edge index, and the
/// along-edge parameter `t`.
fn locate_on_boundary(
    q: Point2,
    outer: &[u32],
    holes: &[Vec<u32>],
    verts: &[Point2],
    tol: f64,
) -> Option<(Option<usize>, usize, f64)> {
    let mut best: Option<(Option<usize>, usize, f64, f64)> = None; // (host, edge, t, dist)
    let mut scan = |host: Option<usize>, loop_idx: &[u32]| {
        let m = loop_idx.len();
        for i in 0..m {
            let a = verts[loop_idx[i] as usize];
            let b = verts[loop_idx[(i + 1) % m] as usize];
            let (d, t) = point_segment(q, a, b);
            // Strictly interior to the edge (not at either endpoint) and within tol.
            if d <= tol && t > 1e-12 && t < 1.0 - 1e-12 && best.is_none_or(|(_, _, _, bd)| d < bd) {
                best = Some((host, i, t, d));
            }
        }
    };
    scan(None, outer);
    for (h, hole) in holes.iter().enumerate() {
        scan(Some(h), hole);
    }
    best.map(|(host, edge, t, _)| (host, edge, t))
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
        // endpoint was appended). Vertex 4 moved onto the curve (x = 0.5+1e-6).
        assert_eq!(
            u.verts.len(),
            6,
            "5 patch verts + 1 top endpoint (merge reused v4)"
        );
        assert!(
            (u.verts[4].x() - (0.5 + 1e-6)).abs() < 1e-15,
            "v4 moved onto curve"
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
}
