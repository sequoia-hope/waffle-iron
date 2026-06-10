//! PR-YR25 RED — Yang §4.5.5 Stage-0 coplanar overlay oracles (M8 slice a).
//!
//! The engine under test (`yang_rs::coplanar_overlay`) must segment two
//! polygons-with-holes on a SHARED plane into ONE conforming classified
//! triangulation (AOnly / BOnly / Overlap), exactly — Yang 2025 §4.5.5,
//! `refs/text/yang2025_hybrid_boolean.txt:717-731` + Fig. 16 (`:752-760`).
//!
//! Oracle stack (every fixture runs `check_properties`):
//!  1. EXACT area identities: area(AOnly)+area(Overlap) == area(A) (rational,
//!     bit-exact equality; same for B).
//!  2. Output f64 verts all finite.
//!  3. No degenerate output triangle: every tri has strictly positive EXACT
//!     area (CCW in exact coords).
//!  4. Every input edge appears as a union of triangle edges (exact interval
//!     tiling of the segment, no gaps).
//!  5. Determinism: a second run produces bit-identical verts/tris/class.
//!  6. Conformity: every undirected edge has ≤ 2 adjacent triangles (no
//!     T-junctions).
//!
//! Plus fixture-specific region/area/boundary assertions, and the
//! rounding-stress fixture asserting the LOUD sliver behavior
//! (`CoplanarOverlayError::RoundingCollapse`), never silence.

use cad_primitives::Point2;
use dashu::float::FBig;
use dashu::rational::RBig;
use std::collections::BTreeMap;
use yang_rs::coplanar_overlay::{
    coplanar_overlay, ClassifiedOverlay, CoplanarOverlayError, ExactPoint2, PolygonWithHoles,
    RegionClass,
};

// ───────────────────────────── helpers ──────────────────────────────────

fn pts(coords: &[(f64, f64)]) -> Vec<Point2> {
    coords.iter().map(|&(x, y)| Point2::new(x, y)).collect()
}

fn pwh(outer: &[(f64, f64)], holes: &[&[(f64, f64)]]) -> PolygonWithHoles {
    PolygonWithHoles {
        outer: pts(outer),
        holes: holes.iter().map(|h| pts(h)).collect(),
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> PolygonWithHoles {
    pwh(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], &[])
}

/// Exact f64 → RBig (total for finite input).
fn rb(x: f64) -> RBig {
    let fb: FBig = FBig::try_from(x).expect("finite f64 → FBig is total");
    RBig::try_from(fb).expect("FBig → RBig is total")
}

/// Exact rational n/d.
fn rq(n: i64, d: i64) -> RBig {
    RBig::from(n) / RBig::from(d)
}

fn rabs(x: RBig) -> RBig {
    if x < RBig::ZERO {
        -x
    } else {
        x
    }
}

fn ep(x: f64, y: f64) -> ExactPoint2 {
    ExactPoint2 { x: rb(x), y: rb(y) }
}

/// `cross(b−a, c−a)` exact (twice the signed area of (a,b,c)).
fn cross_e(a: &ExactPoint2, b: &ExactPoint2, c: &ExactPoint2) -> RBig {
    (&b.x - &a.x) * (&c.y - &a.y) - (&b.y - &a.y) * (&c.x - &a.x)
}

/// Exact signed shoelace area of one loop.
fn shoelace(loop_pts: &[Point2]) -> RBig {
    let n = loop_pts.len();
    let mut sum = RBig::ZERO;
    for i in 0..n {
        let p = loop_pts[i];
        let q = loop_pts[(i + 1) % n];
        sum += rb(p.x()) * rb(q.y()) - rb(q.x()) * rb(p.y());
    }
    sum / RBig::from(2)
}

/// Exact area of a simple polygon-with-holes: |outer| − Σ|holes|.
fn input_area_exact(p: &PolygonWithHoles) -> RBig {
    let mut area = rabs(shoelace(&p.outer));
    for h in &p.holes {
        area -= rabs(shoelace(h));
    }
    area
}

fn count_class(ov: &ClassifiedOverlay, c: RegionClass) -> usize {
    ov.class.iter().filter(|x| **x == c).count()
}

/// Oracle 4: the input edge (a→b) must be tiled, gap-free, by output
/// triangle edges lying exactly ON it (exact rational interval coverage).
fn assert_edge_covered(ov: &ClassifiedOverlay, a: &ExactPoint2, b: &ExactPoint2, label: &str) {
    let dx = &b.x - &a.x;
    let dy = &b.y - &a.y;
    let dd = &dx * &dx + &dy * &dy;
    assert!(
        dd > RBig::ZERO,
        "{label}: zero-length input edge in fixture"
    );

    // Param of q along a→b (valid only when q is on the supporting line).
    let param = |q: &ExactPoint2| -> RBig { ((&q.x - &a.x) * &dx + (&q.y - &a.y) * &dy) / &dd };

    let mut intervals: Vec<(RBig, RBig)> = Vec::new();
    for tri in &ov.tris {
        for k in 0..3 {
            let qi = &ov.exact_verts[tri[k] as usize];
            let qj = &ov.exact_verts[tri[(k + 1) % 3] as usize];
            if cross_e(a, b, qi) != RBig::ZERO || cross_e(a, b, qj) != RBig::ZERO {
                continue; // not on the supporting line
            }
            let (ti, tj) = (param(qi), param(qj));
            let (lo, hi) = if ti <= tj { (ti, tj) } else { (tj, ti) };
            if lo >= RBig::ZERO && hi <= RBig::from(1) && lo != hi {
                intervals.push((lo, hi));
            }
        }
    }
    intervals.sort();
    let mut covered = RBig::ZERO;
    for (lo, hi) in intervals {
        assert!(
            lo <= covered,
            "{label}: gap in triangle-edge coverage of input edge at param {covered}"
        );
        if hi > covered {
            covered = hi;
        }
    }
    assert_eq!(
        covered,
        RBig::from(1),
        "{label}: input edge not fully tiled by triangle edges"
    );
}

fn all_loops(p: &PolygonWithHoles) -> Vec<&[Point2]> {
    std::iter::once(p.outer.as_slice())
        .chain(p.holes.iter().map(|h| h.as_slice()))
        .collect()
}

/// The full property-oracle bundle (oracles 1–6 of the module docs).
fn check_properties(a: &PolygonWithHoles, b: &PolygonWithHoles, ov: &ClassifiedOverlay) {
    // 1. Exact area identities.
    assert_eq!(
        ov.area_exact(RegionClass::AOnly) + ov.area_exact(RegionClass::Overlap),
        input_area_exact(a),
        "area(AOnly) + area(Overlap) != area(A) (exact)"
    );
    assert_eq!(
        ov.area_exact(RegionClass::BOnly) + ov.area_exact(RegionClass::Overlap),
        input_area_exact(b),
        "area(BOnly) + area(Overlap) != area(B) (exact)"
    );

    // Structural 1:1.
    assert_eq!(ov.tris.len(), ov.class.len(), "class not 1:1 with tris");
    assert_eq!(
        ov.verts.len(),
        ov.exact_verts.len(),
        "exact_verts not 1:1 with verts"
    );

    // 2. All rounded verts finite.
    for (i, v) in ov.verts.iter().enumerate() {
        assert!(
            v.x().is_finite() && v.y().is_finite(),
            "vert {i} is not finite"
        );
    }

    // 3. No degenerate triangle: strictly positive EXACT area, CCW.
    for (i, tri) in ov.tris.iter().enumerate() {
        let area2 = cross_e(
            &ov.exact_verts[tri[0] as usize],
            &ov.exact_verts[tri[1] as usize],
            &ov.exact_verts[tri[2] as usize],
        );
        assert!(
            area2 > RBig::ZERO,
            "tri {i} has non-positive exact area {area2}"
        );
    }

    // 4. Every input edge of A and B appears as a union of triangle edges.
    for (side, poly) in [("A", a), ("B", b)] {
        for (li, lp) in all_loops(poly).into_iter().enumerate() {
            for i in 0..lp.len() {
                let p = lp[i];
                let q = lp[(i + 1) % lp.len()];
                assert_edge_covered(
                    ov,
                    &ep(p.x(), p.y()),
                    &ep(q.x(), q.y()),
                    &format!("{side} loop {li} edge {i}"),
                );
            }
        }
    }

    // 5. Determinism: bit-identical second run.
    let ov2 = coplanar_overlay(a, b).expect("second run must succeed");
    assert_eq!(ov.tris, ov2.tris, "tris differ across runs");
    assert_eq!(ov.class, ov2.class, "classes differ across runs");
    assert_eq!(ov.exact_verts, ov2.exact_verts, "exact verts differ");
    let bits = |vs: &[Point2]| -> Vec<(u64, u64)> {
        vs.iter()
            .map(|v| (v.x().to_bits(), v.y().to_bits()))
            .collect()
    };
    assert_eq!(bits(&ov.verts), bits(&ov2.verts), "f64 verts differ");

    // 6. Conformity: ≤ 2 triangles per undirected edge (no T-junctions).
    let mut edge_count: BTreeMap<[u32; 2], usize> = BTreeMap::new();
    for tri in &ov.tris {
        for k in 0..3 {
            let (i, j) = (tri[k], tri[(k + 1) % 3]);
            let key = if i < j { [i, j] } else { [j, i] };
            *edge_count.entry(key).or_default() += 1;
        }
    }
    for (e, n) in edge_count {
        assert!(
            n <= 2,
            "edge {e:?} has {n} adjacent triangles (T-junction?)"
        );
    }
}

/// Assert a polyline's edges all lie exactly on the given axis-aligned "L"
/// segments, and return the exact summed length (axis-aligned: |dx| + |dy|).
fn polyline_length_on_segments(
    ov: &ClassifiedOverlay,
    edges: &[[u32; 2]],
    segments: &[(ExactPoint2, ExactPoint2)],
    label: &str,
) -> RBig {
    let mut total = RBig::ZERO;
    for [i, j] in edges {
        let p = &ov.exact_verts[*i as usize];
        let q = &ov.exact_verts[*j as usize];
        let on_some = segments.iter().any(|(a, b)| {
            cross_e(a, b, p) == RBig::ZERO
                && cross_e(a, b, q) == RBig::ZERO
                && within(a, b, p)
                && within(a, b, q)
        });
        assert!(
            on_some,
            "{label}: interface edge ({i},{j}) not on an expected segment"
        );
        total += rabs(&q.x - &p.x) + rabs(&q.y - &p.y);
    }
    total
}

/// q within the axis-aligned bounding box of [a, b] (used with collinear q).
fn within(a: &ExactPoint2, b: &ExactPoint2, q: &ExactPoint2) -> bool {
    let (xlo, xhi) = if a.x <= b.x {
        (&a.x, &b.x)
    } else {
        (&b.x, &a.x)
    };
    let (ylo, yhi) = if a.y <= b.y {
        (&a.y, &b.y)
    } else {
        (&b.y, &a.y)
    };
    &q.x >= xlo && &q.x <= xhi && &q.y >= ylo && &q.y <= yhi
}

/// Exact triangle centroid (mean of the three exact verts).
fn centroid(ov: &ClassifiedOverlay, tri: &[u32; 3]) -> ExactPoint2 {
    let mut x = RBig::ZERO;
    let mut y = RBig::ZERO;
    for &i in tri {
        x += &ov.exact_verts[i as usize].x;
        y += &ov.exact_verts[i as usize].y;
    }
    ExactPoint2 {
        x: x / RBig::from(3),
        y: y / RBig::from(3),
    }
}

// ───────────────────────────── fixtures ─────────────────────────────────

/// Identical squares → ONE region: everything Overlap, area exact.
#[test]
fn identical_squares_all_overlap() {
    let a = rect(0.0, 0.0, 2.0, 2.0);
    let b = rect(0.0, 0.0, 2.0, 2.0);
    let ov = coplanar_overlay(&a, &b).expect("identical squares must overlay");

    assert!(!ov.tris.is_empty(), "no triangles emitted");
    assert!(
        ov.class.iter().all(|c| *c == RegionClass::Overlap),
        "identical squares must be ALL Overlap, got {:?}",
        ov.class
    );
    assert_eq!(ov.area_exact(RegionClass::Overlap), rq(4, 1));
    assert_eq!(ov.area_exact(RegionClass::AOnly), RBig::ZERO);
    assert_eq!(ov.area_exact(RegionClass::BOnly), RBig::ZERO);
    assert!(ov
        .interface_edges(RegionClass::Overlap, RegionClass::AOnly)
        .is_empty());
    check_properties(&a, &b, &ov);
}

/// Half-overlapping squares — the F0002 stacked-box pattern. 3 regions,
/// exact areas 3/3/1, and the Overlap/AOnly interface is the expected
/// L-polyline from (1,2) through (1,1) to (2,1).
#[test]
fn half_overlapping_squares_f0002_pattern() {
    let a = rect(0.0, 0.0, 2.0, 2.0);
    let b = rect(1.0, 1.0, 3.0, 3.0);
    let ov = coplanar_overlay(&a, &b).expect("F0002 pattern must overlay");

    assert_eq!(ov.area_exact(RegionClass::AOnly), rq(3, 1));
    assert_eq!(ov.area_exact(RegionClass::BOnly), rq(3, 1));
    assert_eq!(ov.area_exact(RegionClass::Overlap), rq(1, 1));
    for c in [RegionClass::AOnly, RegionClass::BOnly, RegionClass::Overlap] {
        assert!(count_class(&ov, c) > 0, "no {c:?} triangles");
    }

    // Overlap/AOnly interface: the L from (1,2) down to (1,1) right to (2,1).
    let edges = ov.interface_edges(RegionClass::Overlap, RegionClass::AOnly);
    assert!(!edges.is_empty(), "no Overlap/AOnly interface edges");
    let l_segments = [
        (ep(1.0, 1.0), ep(1.0, 2.0)), // vertical leg x = 1
        (ep(1.0, 1.0), ep(2.0, 1.0)), // horizontal leg y = 1
    ];
    let len = polyline_length_on_segments(&ov, &edges, &l_segments, "Overlap/AOnly");
    assert_eq!(len, rq(2, 1), "interface length must be exactly 2");

    let chains = ov.interface_polylines(RegionClass::Overlap, RegionClass::AOnly);
    assert_eq!(chains.len(), 1, "interface must chain into ONE polyline");
    let chain = &chains[0];
    let first = &ov.exact_verts[chain[0] as usize];
    let last = &ov.exact_verts[*chain.last().expect("nonempty") as usize];
    let mut ends = [first.clone(), last.clone()];
    ends.sort();
    assert_eq!(
        ends,
        [ep(1.0, 2.0), ep(2.0, 1.0)],
        "interface polyline endpoints must be (1,2) and (2,1)"
    );
    check_properties(&a, &b, &ov);
}

/// B exactly beside A, sharing edge x=2: AOnly + BOnly only, no Overlap;
/// the shared edge is the AOnly/BOnly interface, present as constraint in
/// both (its tiling is also enforced by oracle 4 since it is an input edge
/// of BOTH polygons).
#[test]
fn shared_edge_adjacency_no_overlap() {
    let a = rect(0.0, 0.0, 2.0, 2.0);
    let b = rect(2.0, 0.0, 4.0, 2.0);
    let ov = coplanar_overlay(&a, &b).expect("shared-edge pair must overlay");

    assert_eq!(
        count_class(&ov, RegionClass::Overlap),
        0,
        "no Overlap expected"
    );
    assert_eq!(ov.area_exact(RegionClass::AOnly), rq(4, 1));
    assert_eq!(ov.area_exact(RegionClass::BOnly), rq(4, 1));

    let edges = ov.interface_edges(RegionClass::AOnly, RegionClass::BOnly);
    assert!(
        !edges.is_empty(),
        "shared edge must appear as A/B interface"
    );
    let seg = [(ep(2.0, 0.0), ep(2.0, 2.0))];
    let len = polyline_length_on_segments(&ov, &edges, &seg, "AOnly/BOnly");
    assert_eq!(len, rq(2, 1), "shared-edge interface length must be 2");
    check_properties(&a, &b, &ov);
}

/// B entirely inside A: Overlap island surrounded by an AOnly ring; the
/// Overlap/AOnly interface is ONE closed loop tracing B's boundary.
#[test]
fn island_b_entirely_inside_a() {
    let a = rect(0.0, 0.0, 3.0, 3.0);
    let b = rect(1.0, 1.0, 2.0, 2.0);
    let ov = coplanar_overlay(&a, &b).expect("island must overlay");

    assert_eq!(ov.area_exact(RegionClass::Overlap), rq(1, 1));
    assert_eq!(ov.area_exact(RegionClass::AOnly), rq(8, 1));
    assert_eq!(count_class(&ov, RegionClass::BOnly), 0, "no BOnly expected");

    let edges = ov.interface_edges(RegionClass::Overlap, RegionClass::AOnly);
    let b_boundary = [
        (ep(1.0, 1.0), ep(2.0, 1.0)),
        (ep(2.0, 1.0), ep(2.0, 2.0)),
        (ep(2.0, 2.0), ep(1.0, 2.0)),
        (ep(1.0, 2.0), ep(1.0, 1.0)),
    ];
    let len = polyline_length_on_segments(&ov, &edges, &b_boundary, "island interface");
    assert_eq!(
        len,
        rq(4, 1),
        "island interface must trace B's full boundary"
    );

    let chains = ov.interface_polylines(RegionClass::Overlap, RegionClass::AOnly);
    assert_eq!(chains.len(), 1, "island interface must be ONE loop");
    assert_eq!(
        chains[0].first(),
        chains[0].last(),
        "island interface loop must be closed (first == last)"
    );
    check_properties(&a, &b, &ov);
}

/// A with a hole, B covering part of the hole: the hole boundary is
/// respected and a BOnly region exists INSIDE A's hole.
#[test]
fn hole_partially_covered_by_b() {
    let a = pwh(
        &[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)],
        &[&[(1.0, 1.0), (3.0, 1.0), (3.0, 3.0), (1.0, 3.0)]],
    );
    let b = rect(2.0, 1.5, 3.5, 2.5);
    let ov = coplanar_overlay(&a, &b).expect("hole fixture must overlay");

    // area(A) = 16 − 4 = 12; B = 1.5; Overlap = B ∩ A-material = [3,3.5]×[1.5,2.5].
    assert_eq!(ov.area_exact(RegionClass::Overlap), rq(1, 2));
    assert_eq!(ov.area_exact(RegionClass::AOnly), rq(23, 2));
    assert_eq!(ov.area_exact(RegionClass::BOnly), rq(1, 1));

    // A BOnly triangle must sit strictly inside A's hole box (1,3)×(1,3).
    let one = RBig::from(1);
    let three = RBig::from(3);
    let in_hole = ov
        .tris
        .iter()
        .zip(&ov.class)
        .filter(|(_, c)| **c == RegionClass::BOnly)
        .any(|(tri, _)| {
            let c = centroid(&ov, tri);
            c.x > one && c.x < three && c.y > one && c.y < three
        });
    assert!(in_hole, "no BOnly triangle inside A's hole");
    check_properties(&a, &b, &ov);
}

/// Concave (L-shaped) overlap: A is an L, B a square across the concave
/// corner. Exact areas: AOnly 13/4, Overlap 7/4, BOnly 9/4.
#[test]
fn l_shaped_concave_overlap() {
    let a = pwh(
        &[
            (0.0, 0.0),
            (3.0, 0.0),
            (3.0, 1.0),
            (1.0, 1.0),
            (1.0, 3.0),
            (0.0, 3.0),
        ],
        &[],
    );
    let b = rect(0.5, 0.5, 2.5, 2.5);
    let ov = coplanar_overlay(&a, &b).expect("L-shape must overlay");

    assert_eq!(ov.area_exact(RegionClass::AOnly), rq(13, 4));
    assert_eq!(ov.area_exact(RegionClass::Overlap), rq(7, 4));
    assert_eq!(ov.area_exact(RegionClass::BOnly), rq(9, 4));
    for c in [RegionClass::AOnly, RegionClass::BOnly, RegionClass::Overlap] {
        assert!(count_class(&ov, c) > 0, "no {c:?} triangles");
    }
    check_properties(&a, &b, &ov);
}

/// T-junction: B has a vertex ON the interior of A's left edge; B is
/// otherwise inside A → all of B is Overlap, no BOnly.
#[test]
fn t_junction_vertex_on_edge() {
    let a = rect(0.0, 0.0, 4.0, 4.0);
    let b = pwh(&[(0.0, 2.0), (2.0, 1.0), (2.0, 3.0)], &[]);
    let ov = coplanar_overlay(&a, &b).expect("T-junction must overlay");

    assert_eq!(ov.area_exact(RegionClass::Overlap), rq(2, 1));
    assert_eq!(ov.area_exact(RegionClass::AOnly), rq(14, 1));
    assert_eq!(count_class(&ov, RegionClass::BOnly), 0, "no BOnly expected");

    // The T-junction vertex (0,2) must exist in the shared vertex pool.
    assert!(
        ov.exact_verts.contains(&ep(0.0, 2.0)),
        "T-junction vertex (0,2) missing from shared vertex pool"
    );
    check_properties(&a, &b, &ov);
}

/// Collinear PARTIAL edge overlap: A's bottom edge spans x∈[0,2], B's spans
/// x∈[1,3] on the same line y=0 (and similarly at y=2). Splits must produce
/// the shared sub-segment [1,2] with split vertices at (1,0) and (2,0).
#[test]
fn collinear_partial_edge_overlap() {
    let a = rect(0.0, 0.0, 2.0, 2.0);
    let b = rect(1.0, 0.0, 3.0, 2.0);
    let ov = coplanar_overlay(&a, &b).expect("collinear partial must overlay");

    assert_eq!(ov.area_exact(RegionClass::AOnly), rq(2, 1));
    assert_eq!(ov.area_exact(RegionClass::Overlap), rq(2, 1));
    assert_eq!(ov.area_exact(RegionClass::BOnly), rq(2, 1));

    // Collinear split vertices must exist exactly.
    for (x, y) in [(1.0, 0.0), (2.0, 0.0), (1.0, 2.0), (2.0, 2.0)] {
        assert!(
            ov.exact_verts.contains(&ep(x, y)),
            "collinear split vertex ({x},{y}) missing"
        );
    }
    check_properties(&a, &b, &ov);
}

/// Rounding-stress: B is a wedge whose two slanted edges cross A's top edge
/// at two DISTINCT rational points that both round to the SAME f64 (0.25, 1)
/// — the offsets (≈1.39e-17) are below half an ulp. The sliver cells between
/// the crossings collapse under rounding; the engine must fail LOUDLY with
/// `RoundingCollapse`, never silently drop or flip.
#[test]
fn rounding_stress_sliver_collapse_is_loud() {
    let a = rect(0.0, 0.0, 1.0, 1.0);
    let apex_y = 1.0 - (2.0f64).powi(-53); // representable; 2^-53 below 1.0
    let b = pwh(&[(0.25, apex_y), (0.375, 2.0), (0.125, 2.0)], &[]);

    let err = coplanar_overlay(&a, &b).expect_err("sliver collapse must be LOUD");
    assert!(
        matches!(err, CoplanarOverlayError::RoundingCollapse { .. }),
        "expected RoundingCollapse, got {err:?}"
    );
}

/// Degenerate input is rejected loudly, not processed.
#[test]
fn degenerate_loop_rejected() {
    let a = pwh(&[(0.0, 0.0), (1.0, 0.0)], &[]); // 2 vertices
    let b = rect(0.0, 0.0, 1.0, 1.0);
    let err = coplanar_overlay(&a, &b).expect_err("2-vertex loop must be rejected");
    assert!(matches!(err, CoplanarOverlayError::DegenerateLoop(_)));

    let nan = pwh(&[(0.0, 0.0), (1.0, 0.0), (f64::NAN, 1.0)], &[]);
    let err = coplanar_overlay(&nan, &b).expect_err("NaN input must be rejected");
    assert!(matches!(err, CoplanarOverlayError::NonFiniteInput));
}
