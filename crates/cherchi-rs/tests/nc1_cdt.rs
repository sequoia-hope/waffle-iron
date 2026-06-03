//! PR-NC1 Part A — `cdt_polygon_with_holes` oracle suite (RED).
//!
//! Covers the five Part A oracle points from
//! `specs/yang_pr_nc1_nonconvex_cdt.md`:
//!   1. exact coverage: Σ interior-tri area == outer area − Σ hole areas (TAU).
//!   2. every constraint (boundary) edge appears as a triangle edge.
//!   3. no triangle centroid is outside the outer loop or inside a hole.
//!   4. determinism: two calls produce identical `Vec<[u32; 3]>`.
//!   5. an L/U-shaped non-convex case AND a square-in-square hole case.
//!
//! These tests run against the pure-Rust `cdt_polygon_with_holes` (no sidecar,
//! no filesystem, no rand, no time — fixed coordinates only). They MUST FAIL
//! in the RED phase: the stub returns `Err(CdtError::TriangulationFailed)`, so
//! every `.expect(...)` on the result panics. The GREEN sub-agent makes them
//! pass.

use cad_primitives::Point2;
use cherchi_rs::{cdt_polygon_with_holes, CdtError};
use std::collections::HashMap;

/// Area-coverage tolerance. The fixtures use unit-ish coordinates, so an
/// absolute `1e-9` is well below any genuine area and well above f64 summation
/// noise on a handful of triangles. (cad_primitives `TAU_MODEL` is `1e-7`; we
/// pick a tighter `1e-9` because the fixtures are exactly representable and a
/// correct CDT reproduces the polygon area to machine precision.)
const AREA_TAU: f64 = 1e-9;

// =========================================================================
// Local 2D polygon helpers (self-contained — no production helpers).
// =========================================================================

/// Signed area of a closed polygon given its ordered vertices (shoelace).
/// Positive for CCW, negative for CW.
fn signed_area(poly: &[Point2]) -> f64 {
    let n = poly.len();
    let mut acc = 0.0;
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        acc += a.x() * b.y() - b.x() * a.y();
    }
    acc * 0.5
}

/// Absolute area of a polygon (orientation-independent).
fn abs_area(poly: &[Point2]) -> f64 {
    signed_area(poly).abs()
}

/// Area of a single triangle given three points.
fn tri_area(a: Point2, b: Point2, c: Point2) -> f64 {
    0.5 * ((b.x() - a.x()) * (c.y() - a.y()) - (c.x() - a.x()) * (b.y() - a.y())).abs()
}

/// Sum of all interior-triangle areas in the CDT output.
fn total_tri_area(verts: &[Point2], tris: &[[u32; 3]]) -> f64 {
    tris.iter()
        .map(|t| {
            tri_area(
                verts[t[0] as usize],
                verts[t[1] as usize],
                verts[t[2] as usize],
            )
        })
        .sum()
}

/// Even-odd point-in-polygon test (loop given as ordered vertices). Points are
/// triangle centroids chosen to avoid landing on boundaries.
fn point_in_polygon(p: Point2, poly: &[Point2]) -> bool {
    let n = poly.len();
    let mut inside = false;
    let (px, py) = (p.x(), p.y());
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (poly[i].x(), poly[i].y());
        let (xj, yj) = (poly[j].x(), poly[j].y());
        let intersects = ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi);
        if intersects {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Resolve an index loop into ordered `Point2`s.
fn loop_points(verts: &[Point2], idx: &[u32]) -> Vec<Point2> {
    idx.iter().map(|&i| verts[i as usize]).collect()
}

/// Collect undirected triangle edges as a set of sorted `(u32, u32)` keys.
fn tri_edge_set(tris: &[[u32; 3]]) -> HashMap<(u32, u32), u32> {
    let mut set: HashMap<(u32, u32), u32> = HashMap::new();
    for t in tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (t[i], t[j]);
            let key = if a < b { (a, b) } else { (b, a) };
            *set.entry(key).or_insert(0) += 1;
        }
    }
    set
}

/// Assert every directed boundary edge of `loop_idx` appears as an undirected
/// triangle edge in `edges`.
fn assert_loop_edges_present(loop_idx: &[u32], edges: &HashMap<(u32, u32), u32>, label: &str) {
    let n = loop_idx.len();
    for i in 0..n {
        let a = loop_idx[i];
        let b = loop_idx[(i + 1) % n];
        let key = if a < b { (a, b) } else { (b, a) };
        assert!(
            edges.contains_key(&key),
            "{label}: boundary edge ({a},{b}) is not present as a triangle edge in the CDT output"
        );
    }
}

// =========================================================================
// Fixtures
// =========================================================================

/// L-shaped non-convex polygon. The single reflex vertex is index 3 (1,1).
/// Outer area = 2x2 square minus a 1x1 corner = 3.0. (CCW.)
fn l_shape() -> (Vec<Point2>, Vec<u32>) {
    // CCW outer loop of an L (area = 2x2 square minus a 1x1 corner = 3.0).
    // Vertices (CCW):
    //   0:(0,0) 1:(2,0) 2:(2,1) 3:(1,1) 4:(1,2) 5:(0,2)
    // The reflex vertex is index 3 (1,1).
    let verts = vec![
        Point2::new(0.0, 0.0),
        Point2::new(2.0, 0.0),
        Point2::new(2.0, 1.0),
        Point2::new(1.0, 1.0),
        Point2::new(1.0, 2.0),
        Point2::new(0.0, 2.0),
    ];
    let outer = vec![0, 1, 2, 3, 4, 5];
    (verts, outer)
}

/// Square-in-square: outer 4x4 square with a centered 2x2 square hole.
/// Outer area 16, hole area 4 → interior coverage 12.
fn square_in_square() -> (Vec<Point2>, Vec<u32>, Vec<Vec<u32>>) {
    let verts = vec![
        // outer (CCW): 0..4
        Point2::new(0.0, 0.0),
        Point2::new(4.0, 0.0),
        Point2::new(4.0, 4.0),
        Point2::new(0.0, 4.0),
        // hole (CW from outside → opposite orientation): 4..8
        Point2::new(1.0, 1.0),
        Point2::new(1.0, 3.0),
        Point2::new(3.0, 3.0),
        Point2::new(3.0, 1.0),
    ];
    let outer = vec![0, 1, 2, 3];
    let holes = vec![vec![4, 5, 6, 7]];
    (verts, outer, holes)
}

// =========================================================================
// Oracle 5 (cases) + Oracle 1 (coverage) + Oracle 3 (membership)
// =========================================================================

#[test]
fn l_shape_exact_coverage_and_membership() {
    let (verts, outer) = l_shape();
    let tris = cdt_polygon_with_holes(&verts, &outer, &[])
        .expect("L-shape CDT should triangulate (RED: stub returns Err)");

    // Oracle 1: exact coverage. L-shape area = 3.0.
    let outer_pts = loop_points(&verts, &outer);
    let expected = abs_area(&outer_pts);
    let got = total_tri_area(&verts, &tris);
    assert!(
        (got - expected).abs() < AREA_TAU,
        "L-shape coverage {got} != outer area {expected} (TAU {AREA_TAU})"
    );

    // Oracle 3: no triangle centroid lies outside the outer polygon. (A fan
    // from vertex 0 across the reflex vertex would put a centroid outside.)
    for t in &tris {
        let a = verts[t[0] as usize];
        let b = verts[t[1] as usize];
        let c = verts[t[2] as usize];
        let cx = (a.x() + b.x() + c.x()) / 3.0;
        let cy = (a.y() + b.y() + c.y()) / 3.0;
        let centroid = Point2::new(cx, cy);
        assert!(
            point_in_polygon(centroid, &outer_pts),
            "L-shape: triangle {t:?} centroid ({cx},{cy}) is outside the polygon"
        );
    }
}

#[test]
fn square_in_square_exact_coverage_and_membership() {
    let (verts, outer, holes) = square_in_square();
    let tris = cdt_polygon_with_holes(&verts, &outer, &holes)
        .expect("square-in-square CDT should triangulate (RED: stub returns Err)");

    // Oracle 1: coverage == outer area − hole area = 16 − 4 = 12.
    let outer_pts = loop_points(&verts, &outer);
    let hole_pts = loop_points(&verts, &holes[0]);
    let expected = abs_area(&outer_pts) - abs_area(&hole_pts);
    let got = total_tri_area(&verts, &tris);
    assert!(
        (got - expected).abs() < AREA_TAU,
        "square-in-square coverage {got} != (outer {} − hole {}) (TAU {AREA_TAU})",
        abs_area(&outer_pts),
        abs_area(&hole_pts),
    );

    // Oracle 3: no centroid outside outer, none inside the hole.
    for t in &tris {
        let a = verts[t[0] as usize];
        let b = verts[t[1] as usize];
        let c = verts[t[2] as usize];
        let cx = (a.x() + b.x() + c.x()) / 3.0;
        let cy = (a.y() + b.y() + c.y()) / 3.0;
        let centroid = Point2::new(cx, cy);
        assert!(
            point_in_polygon(centroid, &outer_pts),
            "square-in-square: triangle {t:?} centroid is outside the outer loop"
        );
        assert!(
            !point_in_polygon(centroid, &hole_pts),
            "square-in-square: triangle {t:?} centroid lies INSIDE the hole"
        );
    }
}

// =========================================================================
// Oracle 2 — every constraint (boundary) edge appears as a triangle edge.
// =========================================================================

#[test]
fn boundary_edges_are_constrained_l_shape() {
    let (verts, outer) = l_shape();
    let tris = cdt_polygon_with_holes(&verts, &outer, &[])
        .expect("L-shape CDT should triangulate (RED: stub returns Err)");
    let edges = tri_edge_set(&tris);
    assert_loop_edges_present(&outer, &edges, "L-shape outer");
}

#[test]
fn boundary_edges_are_constrained_square_in_square() {
    let (verts, outer, holes) = square_in_square();
    let tris = cdt_polygon_with_holes(&verts, &outer, &holes)
        .expect("square-in-square CDT should triangulate (RED: stub returns Err)");
    let edges = tri_edge_set(&tris);
    assert_loop_edges_present(&outer, &edges, "square outer");
    assert_loop_edges_present(&holes[0], &edges, "square hole");
}

// =========================================================================
// Oracle 4 — determinism (two calls → identical Vec<[u32;3]>).
// =========================================================================

#[test]
fn determinism_l_shape() {
    let (verts, outer) = l_shape();
    let a = cdt_polygon_with_holes(&verts, &outer, &[])
        .expect("L-shape CDT call A (RED: stub returns Err)");
    let b = cdt_polygon_with_holes(&verts, &outer, &[])
        .expect("L-shape CDT call B (RED: stub returns Err)");
    assert_eq!(a, b, "CDT output is not deterministic across two calls");
}

#[test]
fn determinism_square_in_square() {
    let (verts, outer, holes) = square_in_square();
    let a = cdt_polygon_with_holes(&verts, &outer, &holes)
        .expect("square CDT call A (RED: stub returns Err)");
    let b = cdt_polygon_with_holes(&verts, &outer, &holes)
        .expect("square CDT call B (RED: stub returns Err)");
    assert_eq!(a, b, "CDT output is not deterministic across two calls");
}

// =========================================================================
// Error-path coverage — these EXERCISE the Result API shape. They are
// expected to PASS even in RED for the out-of-range case (stub also errs),
// but they pin the API: errors are values, not panics.
// =========================================================================

#[test]
fn out_of_range_index_is_an_error_not_a_panic() {
    let (verts, _outer) = l_shape();
    // Index 99 is out of range of the 6-vertex pool.
    let bad_outer = vec![0, 1, 99];
    let r = cdt_polygon_with_holes(&verts, &bad_outer, &[]);
    assert!(
        r.is_err(),
        "out-of-range loop index must return Err, got {r:?}"
    );
    // In GREEN this should specifically be LoopIndexOutOfRange; the RED stub
    // returns TriangulationFailed. Accept either error value here so this case
    // does not block RED on the precise variant — coverage tests are the gate.
    let _ = CdtError::LoopIndexOutOfRange;
}
