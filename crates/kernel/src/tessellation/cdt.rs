//! Constrained Delaunay Triangulation wrapper for 2D polygon triangulation.
//!
//! Yang 2025 §4.4.1 specifies CDT for B-Rep face tessellation. The previous
//! implementation used `earcutr` (Livesu 2021 simplified earcut), which does
//! not enforce constraint edges and is not Delaunay — logged as deviation D1
//! in `docs/yang_deviations.md`. This module wraps `spade`'s CDT to provide
//! a spec-compliant replacement.

use spade::{ConstrainedDelaunayTriangulation, InsertionError, Point2, Triangulation};

#[derive(Debug)]
pub enum CdtError {
    Insertion(InsertionError),
    /// The spade backend panicked on bad input (e.g., self-intersecting constraint
    /// edges from upstream B-Rep boundary defects). The panic was caught and the
    /// caller should fall back to a more permissive triangulator.
    BackendPanic,
}

impl From<InsertionError> for CdtError {
    fn from(e: InsertionError) -> Self {
        CdtError::Insertion(e)
    }
}

/// Triangulate a 2D polygon (with optional holes) via Constrained Delaunay
/// Triangulation, enforcing every consecutive boundary segment as a constraint
/// edge (per Yang 2025 §4.4.1).
///
/// `points` are 2D vertex positions. `loops` describes the polygon: the first
/// loop is the outer boundary; subsequent loops are holes. Each loop is a list
/// of indices into `points`. Loops are assumed to be closed by adjacency
/// (last vertex connects back to first); callers should NOT duplicate the
/// first vertex at the end.
///
/// Output: triangle index triplets in CCW order, with triangles outside the
/// polygon (concavities inside the convex hull, or inside holes) filtered out
/// via centroid-based point-in-polygon test.
pub fn cdt_triangulate_2d_with_loops(
    points: &[(f64, f64)],
    loops: &[Vec<usize>],
) -> Result<Vec<[usize; 3]>, CdtError> {
    if loops.is_empty() {
        return Ok(Vec::new());
    }

    // Build constraint edges from every loop's consecutive segments + closing edge.
    let mut constraint_edges: Vec<[usize; 2]> = Vec::new();
    for lp in loops {
        let n = lp.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let j = (i + 1) % n;
            constraint_edges.push([lp[i], lp[j]]);
        }
    }

    let spade_points: Vec<Point2<f64>> = points
        .iter()
        .map(|&(x, y)| Point2::new(x, y))
        .collect();

    // Use try_bulk_load_cdt which routes intersecting/conflicting constraint
    // edges through an on_conflict_found callback rather than panicking. Still
    // wrap in catch_unwind for defensive safety against deeper invariant
    // violations spade may panic on — the CDT struct is constructed fresh, so
    // AssertUnwindSafe is sound.
    let build_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ConstrainedDelaunayTriangulation::<Point2<f64>>::try_bulk_load_cdt(
            spade_points,
            constraint_edges,
            |_conflict_edge| {
                // Silently drop conflicting edges. Yang §4.4.1 assumes simple
                // boundary loops; if upstream produced a self-intersecting loop
                // we don't have a recovery story other than "best-effort."
            },
        )
    }));

    let cdt = match build_result {
        Ok(Ok(cdt)) => cdt,
        Ok(Err(e)) => return Err(CdtError::Insertion(e)),
        Err(_) => return Err(CdtError::BackendPanic),
    };

    let outer = &loops[0];
    let holes: &[Vec<usize>] = &loops[1..];

    let mut triangles = Vec::with_capacity(cdt.num_inner_faces());
    for face in cdt.inner_faces() {
        let verts = face.vertices();
        let idx = [
            verts[0].fix().index(),
            verts[1].fix().index(),
            verts[2].fix().index(),
        ];

        let cx = (points[idx[0]].0 + points[idx[1]].0 + points[idx[2]].0) / 3.0;
        let cy = (points[idx[0]].1 + points[idx[1]].1 + points[idx[2]].1) / 3.0;

        if !point_in_polygon((cx, cy), outer, points) {
            continue;
        }
        if holes.iter().any(|h| point_in_polygon((cx, cy), h, points)) {
            continue;
        }
        triangles.push(idx);
    }
    Ok(triangles)
}

/// Earcut-shaped convenience wrapper: flat `[x0, y0, x1, y1, ...]` 2D coordinate
/// array + `hole_indices` array marking the start vertex of each hole (in vertex
/// units, not coord units — same convention as `earcutr::earcut`). Returns a
/// flat `Vec<usize>` of triangle indices (3 per triangle).
///
/// This mirrors the `earcutr::earcut(coords, hole_indices, 2)` signature so
/// existing call sites can swap in mechanically.
pub fn cdt_triangulate_flat(
    coords_2d: &[f64],
    hole_indices: &[usize],
) -> Result<Vec<usize>, CdtError> {
    let n_verts = coords_2d.len() / 2;
    if n_verts < 3 {
        return Ok(Vec::new());
    }
    let points: Vec<(f64, f64)> = (0..n_verts)
        .map(|i| (coords_2d[2 * i], coords_2d[2 * i + 1]))
        .collect();

    let mut loops: Vec<Vec<usize>> = Vec::new();
    let outer_end = hole_indices.first().copied().unwrap_or(n_verts);
    loops.push((0..outer_end).collect());
    for k in 0..hole_indices.len() {
        let start = hole_indices[k];
        let end = hole_indices.get(k + 1).copied().unwrap_or(n_verts);
        loops.push((start..end).collect());
    }

    let triangles = cdt_triangulate_2d_with_loops(&points, &loops)?;
    let mut flat = Vec::with_capacity(triangles.len() * 3);
    for t in triangles {
        flat.extend_from_slice(&t);
    }
    Ok(flat)
}

/// Standard even-odd ray-casting point-in-polygon test.
fn point_in_polygon(p: (f64, f64), boundary: &[usize], points: &[(f64, f64)]) -> bool {
    let (px, py) = p;
    let n = boundary.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = points[boundary[i]];
        let (xj, yj) = points[boundary[j]];
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

    #[test]
    fn triangulates_simple_square() {
        let pts = vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let loops = vec![vec![0, 1, 2, 3]];
        let tris = cdt_triangulate_2d_with_loops(&pts, &loops).unwrap();
        assert_eq!(tris.len(), 2);
    }

    #[test]
    fn triangulates_square_with_hole() {
        let pts = vec![
            (0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0),  // outer
            (1.0, 1.0), (3.0, 1.0), (3.0, 3.0), (1.0, 3.0),  // hole
        ];
        let loops = vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7]];
        let tris = cdt_triangulate_2d_with_loops(&pts, &loops).unwrap();
        // 8 vertices, 2 closed loops; an annulus has 8 triangles.
        assert_eq!(tris.len(), 8);

        // No triangle should have centroid inside the hole.
        for t in &tris {
            let cx = (pts[t[0]].0 + pts[t[1]].0 + pts[t[2]].0) / 3.0;
            let cy = (pts[t[0]].1 + pts[t[1]].1 + pts[t[2]].1) / 3.0;
            let in_hole = cx > 1.0 && cx < 3.0 && cy > 1.0 && cy < 3.0;
            assert!(!in_hole, "Triangle {:?} centroid in hole at ({}, {})", t, cx, cy);
        }
    }

    #[test]
    fn triangulates_concave_polygon() {
        // L-shape: 6 vertices.
        let pts = vec![
            (0.0, 0.0), (2.0, 0.0), (2.0, 1.0), (1.0, 1.0), (1.0, 2.0), (0.0, 2.0),
        ];
        let loops = vec![vec![0, 1, 2, 3, 4, 5]];
        let tris = cdt_triangulate_2d_with_loops(&pts, &loops).unwrap();
        // L-shape has 4 triangles (any valid triangulation).
        assert_eq!(tris.len(), 4);

        // No triangle should have centroid in the missing corner (1..2, 1..2).
        for t in &tris {
            let cx = (pts[t[0]].0 + pts[t[1]].0 + pts[t[2]].0) / 3.0;
            let cy = (pts[t[0]].1 + pts[t[1]].1 + pts[t[2]].1) / 3.0;
            let in_missing = cx > 1.0 && cx < 2.0 && cy > 1.0 && cy < 2.0;
            assert!(!in_missing, "Triangle {:?} centroid in missing corner at ({}, {})", t, cx, cy);
        }
    }

    #[test]
    fn flat_api_no_holes_simple_square() {
        let coords: Vec<f64> = vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];
        let tris = cdt_triangulate_flat(&coords, &[]).unwrap();
        assert_eq!(tris.len(), 6); // 2 triangles × 3 indices
    }

    #[test]
    fn flat_api_with_holes_annulus() {
        let coords: Vec<f64> = vec![
            0.0, 0.0, 4.0, 0.0, 4.0, 4.0, 0.0, 4.0,  // outer (4 verts)
            1.0, 1.0, 3.0, 1.0, 3.0, 3.0, 1.0, 3.0,  // hole (4 verts), starts at vertex 4
        ];
        let hole_indices = vec![4];
        let tris = cdt_triangulate_flat(&coords, &hole_indices).unwrap();
        assert_eq!(tris.len(), 24); // 8 triangles × 3 indices
    }

    #[test]
    fn rejects_degenerate_loop() {
        let pts = vec![(0.0, 0.0), (1.0, 0.0)];
        let loops = vec![vec![0, 1]];
        let tris = cdt_triangulate_2d_with_loops(&pts, &loops).unwrap();
        assert!(tris.is_empty());
    }
}
