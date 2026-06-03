//! PR-NC1 Part A ADVERSARY — independent audit of `cdt_polygon_with_holes`.
//!
//! A THIRD agent (neither the RED author nor the GREEN implementer). The RED
//! corpus (`nc1_cdt.rs`) used an L-shape (one reflex), a square-in-square (one
//! hole), and a unit box. This file deliberately uses DISTINCT, harder fixtures
//! to verify the GREEN's load-bearing claims independently:
//!
//!   * a **5-point star** (FIVE reflex vertices, NOT star-shaped from vertex 0
//!     so any fan from the first vertex escapes the polygon),
//!   * an **E / comb** profile (THREE reflex vertices, deep notches),
//!   * a square with **TWO holes** (the RED corpus only ever had one hole).
//!
//! Independent oracles (all areas recomputed test-side via the shoelace
//! formula, no production helper shared):
//!   1. EXACT coverage: Σ tri area == outer area − Σ hole areas within TAU_MODEL.
//!   2. No boundary subdivision: every directed boundary edge (outer + each
//!      hole) appears as a directed mesh edge — UNSPLIT. (Load-bearing claim.)
//!   3. Vertex-set conservation: every emitted index is a referenced boundary
//!      vertex; NO Steiner point was introduced.
//!   4. No triangle outside the outer loop or inside any hole (centroid test).
//!   5. Determinism: identical input → byte-identical `Vec<[u32;3]>`.
//!   6. Order/transform invariance probes (where the spec claims them) +
//!      degenerate / adversarial inputs return a LOUD `Err`.
//!   7. A MUTATION WITNESS: an independently-built BROKEN triangulation
//!      (fan-from-vertex-0 across the star) is shown to FAIL oracle 1, proving
//!      the coverage oracle actually bites.

use std::collections::HashMap;

use cad_primitives::{Point2, TAU_MODEL};
use cherchi_rs::{cdt_polygon_with_holes, CdtError};

// ---- independent shoelace / area helpers (NOT shared with RED) ----
fn signed_area(poly: &[Point2]) -> f64 {
    let n = poly.len();
    let mut s = 0.0;
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        s += a.x() * b.y() - b.x() * a.y();
    }
    s * 0.5
}
fn abs_area(poly: &[Point2]) -> f64 {
    signed_area(poly).abs()
}
fn tri_area(a: Point2, b: Point2, c: Point2) -> f64 {
    0.5 * ((b.x() - a.x()) * (c.y() - a.y()) - (b.y() - a.y()) * (c.x() - a.x())).abs()
}
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
fn loop_points(verts: &[Point2], idx: &[u32]) -> Vec<Point2> {
    idx.iter().map(|&i| verts[i as usize]).collect()
}
fn directed_edges(tris: &[[u32; 3]]) -> HashMap<(u32, u32), u32> {
    let mut m = HashMap::new();
    for t in tris {
        for (i, j) in [(0, 1), (1, 2), (2, 0)] {
            *m.entry((t[i], t[j])).or_insert(0) += 1;
        }
    }
    m
}
/// A boundary edge (a,b) is present unsplit iff (a,b) OR (b,a) is a mesh edge.
fn boundary_edge_present(a: u32, b: u32, edges: &HashMap<(u32, u32), u32>) -> bool {
    edges.contains_key(&(a, b)) || edges.contains_key(&(b, a))
}
fn point_in_polygon(p: Point2, poly: &[Point2]) -> bool {
    let n = poly.len();
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

const AREA_TAU: f64 = TAU_MODEL;

// =====================================================================
// FIXTURE 1 — 5-point star (CCW). FIVE reflex vertices. NOT star-shaped
// from vertex 0, so a fan from vertex 0 escapes the polygon.
//
// Outer radius R=1, inner radius r=0.4, 10 alternating vertices starting at
// angle 90° (top point). Even indices = outer points, odd = inner (reflex).
// =====================================================================
fn star(outer_r: f64, inner_r: f64) -> (Vec<Point2>, Vec<u32>) {
    let mut verts = Vec::new();
    for k in 0..10u32 {
        let r = if k % 2 == 0 { outer_r } else { inner_r };
        // start at +y (90°), step −36° to wind CCW (k increasing → angle
        // increasing keeps CCW; use +36°).
        let theta = std::f64::consts::FRAC_PI_2 + (k as f64) * std::f64::consts::TAU / 10.0;
        verts.push(Point2::new(r * theta.cos(), r * theta.sin()));
    }
    let outer: Vec<u32> = (0..10).collect();
    (verts, outer)
}

// Analytic star area: 10 congruent triangles formed by center, an outer point
// and an adjacent inner point. Each has area 0.5 * R * r * sin(36°). Used as an
// INDEPENDENT cross-check of the shoelace area.
fn star_analytic_area(outer_r: f64, inner_r: f64) -> f64 {
    10.0 * 0.5 * outer_r * inner_r * (std::f64::consts::TAU / 10.0).sin()
}

#[test]
fn star_exact_coverage_independent() {
    let (verts, outer) = star(1.0, 0.4);
    let tris = cdt_polygon_with_holes(&verts, &outer, &[]).expect("star must triangulate");
    let outer_pts = loop_points(&verts, &outer);
    let shoelace = abs_area(&outer_pts);
    let analytic = star_analytic_area(1.0, 0.4);
    // The two independent area derivations must agree (sanity on the fixture).
    assert!(
        (shoelace - analytic).abs() < 1e-12,
        "star fixture: shoelace {shoelace} != analytic {analytic}"
    );
    let got = total_tri_area(&verts, &tris);
    assert!(
        (got - shoelace).abs() <= AREA_TAU,
        "star coverage {got} != outer area {shoelace} (TAU {AREA_TAU})"
    );
}

#[test]
fn star_no_triangle_escapes_polygon() {
    let (verts, outer) = star(1.0, 0.4);
    let tris = cdt_polygon_with_holes(&verts, &outer, &[]).expect("star must triangulate");
    let outer_pts = loop_points(&verts, &outer);
    for t in &tris {
        let a = verts[t[0] as usize];
        let b = verts[t[1] as usize];
        let c = verts[t[2] as usize];
        let centroid = Point2::new((a.x() + b.x() + c.x()) / 3.0, (a.y() + b.y() + c.y()) / 3.0);
        assert!(
            point_in_polygon(centroid, &outer_pts),
            "star: triangle {t:?} centroid {centroid:?} lies OUTSIDE the star \
             (a fan from vertex 0 would escape into a notch — this oracle catches it)"
        );
    }
}

#[test]
fn star_all_boundary_edges_unsplit_and_no_steiner() {
    let (verts, outer) = star(1.0, 0.4);
    let tris = cdt_polygon_with_holes(&verts, &outer, &[]).expect("star must triangulate");
    let edges = directed_edges(&tris);
    for i in 0..outer.len() {
        let a = outer[i];
        let b = outer[(i + 1) % outer.len()];
        assert!(
            boundary_edge_present(a, b, &edges),
            "star boundary edge ({a},{b}) is SUBDIVIDED / missing — CDT must not \
             split a constraint edge"
        );
    }
    // No Steiner point: every emitted index references a boundary vertex.
    let max_idx = *outer.iter().max().unwrap();
    for t in &tris {
        for &v in t {
            assert!(
                v <= max_idx,
                "star: triangle {t:?} references vertex {v} > max boundary index \
                 {max_idx} — a Steiner point was introduced"
            );
        }
    }
}

// =====================================================================
// FIXTURE 2 — E / comb profile. A vertical bar on the left with three teeth
// to the right (three notches → at least three reflex vertices). Definitely
// non-star-shaped. CCW.
//
// Footprint: x∈[0,4], y∈[0,7]. Spine x∈[0,1] full height. Teeth at
// y∈[0,1], y∈[3,4], y∈[6,7] extend to x=4. Notches at y∈[1,3] and y∈[4,6].
// =====================================================================
fn e_comb() -> (Vec<Point2>, Vec<u32>) {
    // CCW traversal of the E outline.
    let pts = [
        (0.0, 0.0),
        (4.0, 0.0),
        (4.0, 1.0),
        (1.0, 1.0),
        (1.0, 3.0),
        (4.0, 3.0),
        (4.0, 4.0),
        (1.0, 4.0),
        (1.0, 6.0),
        (4.0, 6.0),
        (4.0, 7.0),
        (0.0, 7.0),
    ];
    let verts: Vec<Point2> = pts.iter().map(|&(x, y)| Point2::new(x, y)).collect();
    let outer: Vec<u32> = (0..verts.len() as u32).collect();
    (verts, outer)
}

// Independent area of the E by rectangle decomposition: spine 1×7 plus three
// teeth each 3×1 = 7 + 9 = 16.
fn e_comb_analytic_area() -> f64 {
    1.0 * 7.0 + 3.0 * (3.0 * 1.0)
}

#[test]
fn e_comb_exact_coverage_and_membership() {
    let (verts, outer) = e_comb();
    let tris = cdt_polygon_with_holes(&verts, &outer, &[]).expect("E-comb must triangulate");
    let outer_pts = loop_points(&verts, &outer);
    let shoelace = abs_area(&outer_pts);
    assert!(
        (shoelace - e_comb_analytic_area()).abs() < 1e-12,
        "E-comb fixture: shoelace {shoelace} != decomposition {}",
        e_comb_analytic_area()
    );
    let got = total_tri_area(&verts, &tris);
    assert!(
        (got - shoelace).abs() <= AREA_TAU,
        "E-comb coverage {got} != area {shoelace}"
    );
    for t in &tris {
        let a = verts[t[0] as usize];
        let b = verts[t[1] as usize];
        let c = verts[t[2] as usize];
        let centroid = Point2::new((a.x() + b.x() + c.x()) / 3.0, (a.y() + b.y() + c.y()) / 3.0);
        assert!(
            point_in_polygon(centroid, &outer_pts),
            "E-comb: triangle {t:?} centroid escapes into a notch"
        );
    }
    // Boundary edges unsplit.
    let edges = directed_edges(&tris);
    for i in 0..outer.len() {
        let a = outer[i];
        let b = outer[(i + 1) % outer.len()];
        assert!(
            boundary_edge_present(a, b, &edges),
            "E-comb boundary edge ({a},{b}) subdivided"
        );
    }
}

// =====================================================================
// FIXTURE 3 — square with TWO holes (the RED corpus only ever had ONE hole).
// Outer 6×6, two 1×1 holes at distinct locations. Holes wound CW (opposite
// the outer CCW loop).
//
// Outer area 36, each hole area 1 → coverage 34.
// =====================================================================
fn square_two_holes() -> (Vec<Point2>, Vec<u32>, Vec<Vec<u32>>) {
    let verts = vec![
        // outer CCW 0..4
        Point2::new(0.0, 0.0),
        Point2::new(6.0, 0.0),
        Point2::new(6.0, 6.0),
        Point2::new(0.0, 6.0),
        // hole A (CW) 4..8 : [1,2]x[1,2]
        Point2::new(1.0, 1.0),
        Point2::new(1.0, 2.0),
        Point2::new(2.0, 2.0),
        Point2::new(2.0, 1.0),
        // hole B (CW) 8..12 : [4,5]x[4,5]
        Point2::new(4.0, 4.0),
        Point2::new(4.0, 5.0),
        Point2::new(5.0, 5.0),
        Point2::new(5.0, 4.0),
    ];
    let outer = vec![0, 1, 2, 3];
    let holes = vec![vec![4, 5, 6, 7], vec![8, 9, 10, 11]];
    (verts, outer, holes)
}

#[test]
fn two_holes_exact_coverage() {
    let (verts, outer, holes) = square_two_holes();
    let tris = cdt_polygon_with_holes(&verts, &outer, &holes).expect("two-holes must triangulate");
    let outer_pts = loop_points(&verts, &outer);
    let mut expected = abs_area(&outer_pts);
    for h in &holes {
        expected -= abs_area(&loop_points(&verts, h));
    }
    let got = total_tri_area(&verts, &tris);
    assert!(
        (got - expected).abs() <= AREA_TAU,
        "two-holes coverage {got} != (outer − 2 holes) {expected}"
    );
}

#[test]
fn two_holes_no_triangle_inside_a_hole() {
    let (verts, outer, holes) = square_two_holes();
    let tris = cdt_polygon_with_holes(&verts, &outer, &holes).expect("two-holes must triangulate");
    let outer_pts = loop_points(&verts, &outer);
    let hole_pts: Vec<Vec<Point2>> = holes.iter().map(|h| loop_points(&verts, h)).collect();
    for t in &tris {
        let a = verts[t[0] as usize];
        let b = verts[t[1] as usize];
        let c = verts[t[2] as usize];
        let centroid = Point2::new((a.x() + b.x() + c.x()) / 3.0, (a.y() + b.y() + c.y()) / 3.0);
        assert!(
            point_in_polygon(centroid, &outer_pts),
            "two-holes: triangle {t:?} centroid outside outer loop"
        );
        for (hi, hp) in hole_pts.iter().enumerate() {
            assert!(
                !point_in_polygon(centroid, hp),
                "two-holes: triangle {t:?} centroid lies INSIDE hole {hi}"
            );
        }
    }
}

#[test]
fn two_holes_all_boundary_edges_unsplit() {
    let (verts, outer, holes) = square_two_holes();
    let tris = cdt_polygon_with_holes(&verts, &outer, &holes).expect("two-holes must triangulate");
    let edges = directed_edges(&tris);
    let mut all_loops = vec![outer.clone()];
    all_loops.extend(holes.iter().cloned());
    for lp in &all_loops {
        for i in 0..lp.len() {
            let a = lp[i];
            let b = lp[(i + 1) % lp.len()];
            assert!(
                boundary_edge_present(a, b, &edges),
                "two-holes boundary edge ({a},{b}) subdivided / missing"
            );
        }
    }
}

// =====================================================================
// Determinism — byte-identical across repeated calls, on every fixture.
// =====================================================================
#[test]
fn determinism_byte_identical_all_fixtures() {
    let (v1, o1) = star(1.0, 0.4);
    let r1a = cdt_polygon_with_holes(&v1, &o1, &[]).unwrap();
    let r1b = cdt_polygon_with_holes(&v1, &o1, &[]).unwrap();
    assert_eq!(r1a, r1b, "star not deterministic");

    let (v2, o2) = e_comb();
    let r2a = cdt_polygon_with_holes(&v2, &o2, &[]).unwrap();
    let r2b = cdt_polygon_with_holes(&v2, &o2, &[]).unwrap();
    assert_eq!(r2a, r2b, "E-comb not deterministic");

    let (v3, o3, h3) = square_two_holes();
    let r3a = cdt_polygon_with_holes(&v3, &o3, &h3).unwrap();
    let r3b = cdt_polygon_with_holes(&v3, &o3, &h3).unwrap();
    assert_eq!(r3a, r3b, "two-holes not deterministic");
}

// Determinism must hold even when the input vectors are freshly rebuilt (no
// shared allocation / address-dependent ordering). The spec claims byte-
// identical output on the SAME input — this exercises it from independently
// constructed Vecs.
#[test]
fn determinism_fresh_construction_two_holes() {
    let a = {
        let (v, o, h) = square_two_holes();
        cdt_polygon_with_holes(&v, &o, &h).unwrap()
    };
    let b = {
        let (v, o, h) = square_two_holes();
        cdt_polygon_with_holes(&v, &o, &h).unwrap()
    };
    assert_eq!(
        a, b,
        "two-holes not deterministic across fresh construction"
    );
}

// NOTE (NOT a bug): a pure RIGID TRANSLATION of every vertex can change the
// chosen interior diagonals on the star (cocircular quads resolve differently
// under floating-point coordinate shift). The PR-NC1 spec only claims
// determinism on IDENTICAL input, NOT transform invariance, so this is
// expected and acceptable. Coverage / boundary / no-Steiner all still hold on
// the translated input — verified here.
#[test]
fn translation_preserves_coverage_even_if_topology_differs() {
    let (verts, outer) = star(1.0, 0.4);
    let shifted: Vec<Point2> = verts
        .iter()
        .map(|p| Point2::new(p.x() + 17.5, p.y() - 9.25))
        .collect();
    let moved = cdt_polygon_with_holes(&shifted, &outer, &[]).unwrap();
    let true_area = star_analytic_area(1.0, 0.4);
    let got = total_tri_area(&shifted, &moved);
    assert!(
        (got - true_area).abs() <= AREA_TAU,
        "translated star coverage {got} != true area {true_area} \
         (topology may differ, but COVERAGE must be invariant)"
    );
    // And boundary edges remain unsplit on the translated input.
    let edges = directed_edges(&moved);
    for i in 0..outer.len() {
        let a = outer[i];
        let b = outer[(i + 1) % outer.len()];
        assert!(
            boundary_edge_present(a, b, &edges),
            "translated star boundary edge ({a},{b}) subdivided"
        );
    }
}

// =====================================================================
// Degenerate / adversarial inputs must return a LOUD Err (never a silent
// wrong/empty triangulation).
// =====================================================================
#[test]
fn err_outer_too_few_vertices() {
    let verts = vec![Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)];
    let r = cdt_polygon_with_holes(&verts, &[0, 1], &[]);
    assert_eq!(r, Err(CdtError::DegenerateInput), "2-vertex outer must Err");
}

#[test]
fn err_loop_index_out_of_range() {
    let verts = vec![
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(0.0, 1.0),
    ];
    let r = cdt_polygon_with_holes(&verts, &[0, 1, 9], &[]);
    assert_eq!(
        r,
        Err(CdtError::LoopIndexOutOfRange),
        "out-of-range index must Err"
    );
}

#[test]
fn err_duplicate_coincident_vertex() {
    // Two distinct indices at the SAME position -> DuplicateVertex.
    let verts = vec![
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(0.0, 1.0),
        Point2::new(0.0, 0.0), // coincident with index 0
    ];
    let r = cdt_polygon_with_holes(&verts, &[0, 1, 2, 3], &[]);
    assert_eq!(
        r,
        Err(CdtError::DuplicateVertex),
        "coincident distinct loop vertices must Err (DuplicateVertex)"
    );
}

// ====================================================================
// ADVERSARY-FOUND BUG — NOW FIXED (PR-NC1 follow-up); live regression test.
//
// FINDING (was): a fully-COLLINEAR (zero-area) outer loop returned `Ok(vec![])` —
// a SILENT EMPTY triangulation — instead of a LOUD `Err`. FIXED by the
// implementer's empty-output guard (`tris.is_empty()` ⇒ `DegenerateInput`).
//
// This violates two explicit PR-NC1 contracts:
//   * `CdtError::DegenerateInput` is documented (triangulation/mod.rs:34-37) as
//     covering an outer loop that "is collinear, or encloses zero area";
//   * the STOP-and-report triggers + the module contract forbid a "silent
//     wrong/empty triangulation".
//
// MECHANISM: spade accepts the 4 distinct (non-coincident) collinear points and
// the non-crossing constraint edges, but `inner_faces()` yields nothing (no 2D
// face can form from collinear points), so the function falls through to
// `Ok(tris)` with `tris == []`. There is NO outer-loop area / collinearity
// guard before returning.
//
// REACHABILITY from yang-rs Stage-1: a HOLE-FREE degenerate face is routed to
// the FAN path (`planar_outer_loop_is_nonconvex` returns false on a near-zero-
// area projection — lib.rs:1138), whose `DegenerateFace` guard (lib.rs:650)
// catches it — so the common case is safe. BUT a face WITH an inner loop is
// routed to CDT unconditionally (`!inner_loops.is_empty()`, lib.rs:618); a
// holed face with a collinear/zero-area OUTER loop would reach this primitive
// and silently produce an empty face mesh (a hole punched through nothing),
// rather than erroring. Narrow but real.
//
// FIX (implementer, per P9/P10 — adversary does NOT fix): reject an outer loop
// whose signed area is ~0 with `CdtError::DegenerateInput` before/after spade
// (mirror yang-rs's Newell guard), OR validate `tris` is non-empty for a
// >=3-vertex outer loop. Removing `#[ignore]` should then pass.
// ====================================================================
#[test]
fn collinear_outer_loop_errs_degenerate() {
    let verts = vec![
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(2.0, 0.0),
        Point2::new(3.0, 0.0),
    ];
    let r = cdt_polygon_with_holes(&verts, &[0, 1, 2, 3], &[]);
    assert!(
        r.is_err(),
        "fully-collinear (zero-area) outer loop must Err (DegenerateInput), got \
         {r:?} — a silent empty triangulation is a contract violation"
    );
}

// A POSITIVE control proving the bug is specifically the missing zero-area
// guard (not a general spade rejection): the SAME 4 points perturbed into a
// genuine thin sliver (positive area) DO triangulate to a non-empty mesh whose
// coverage matches the sliver area. So the empty return above is purely the
// zero-area degeneracy, confirming the missing guard.
#[test]
fn sliver_positive_area_triangulates_nonempty() {
    let verts = vec![
        Point2::new(0.0, 0.0),
        Point2::new(3.0, 0.0),
        Point2::new(3.0, 1e-3),
        Point2::new(0.0, 1e-3),
    ];
    let outer = [0u32, 1, 2, 3];
    let tris = cdt_polygon_with_holes(&verts, &outer, &[]).expect("sliver must triangulate");
    assert!(
        !tris.is_empty(),
        "a positive-area (thin) quad must produce >=1 triangle"
    );
    let got = total_tri_area(&verts, &tris);
    let expected = abs_area(&loop_points(&verts, &outer));
    assert!(
        (got - expected).abs() <= AREA_TAU,
        "sliver coverage {got} != area {expected}"
    );
}

#[test]
fn err_self_intersecting_bowtie() {
    // A bow-tie (figure-eight) outer loop: edges cross. The constraint edges
    // intersect → must Err (TriangulationFailed), never silently split.
    let verts = vec![
        Point2::new(0.0, 0.0),
        Point2::new(2.0, 2.0),
        Point2::new(2.0, 0.0),
        Point2::new(0.0, 2.0),
    ];
    // loop 0->1->2->3->0 crosses itself (0-1 vs 2-3).
    let r = cdt_polygon_with_holes(&verts, &[0, 1, 2, 3], &[]);
    assert!(
        r.is_err(),
        "self-intersecting (bow-tie) outer loop must Err, got {r:?}"
    );
}

// =====================================================================
// MUTATION WITNESS — prove the coverage oracle actually bites.
//
// Build, INDEPENDENTLY of production, the BROKEN triangulation a naive fan from
// vertex 0 would produce on the star, and show it FAILS the same coverage
// oracle that production passes. This proves oracle 1 is not vacuous.
// =====================================================================
#[test]
fn mutation_fan_from_vertex0_breaks_coverage() {
    let (verts, outer) = star(1.0, 0.4);
    // Naive fan: [v0, v_i, v_{i+1}] for i in 1..n-1.
    let mut fan: Vec<[u32; 3]> = Vec::new();
    for i in 1..outer.len() - 1 {
        fan.push([outer[0], outer[i], outer[i + 1]]);
    }
    let outer_pts = loop_points(&verts, &outer);
    let true_area = abs_area(&outer_pts);
    let fan_area = total_tri_area(&verts, &fan);
    // The fan OVER-counts (it covers convex hull regions outside the star
    // notches) — its area must differ from the true star area by more than TAU.
    assert!(
        (fan_area - true_area).abs() > 1e-3,
        "expected the naive fan to MIS-cover the star (area {fan_area} vs true \
         {true_area}); if they matched, the coverage oracle would be vacuous"
    );

    // And the REAL production triangulation passes where the fan fails.
    let real = cdt_polygon_with_holes(&verts, &outer, &[]).unwrap();
    let real_area = total_tri_area(&verts, &real);
    assert!(
        (real_area - true_area).abs() <= AREA_TAU,
        "production star area {real_area} must match true {true_area}"
    );
}
