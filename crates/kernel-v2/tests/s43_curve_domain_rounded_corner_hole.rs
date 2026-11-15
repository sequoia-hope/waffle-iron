//! Yang §4.3 curve-interior domain certificate (yang-rs trigger T9, ledger
//! N107): a hole drilled beside a plate's rounded corner.
//!
//! The plate is 60 × 40 × 1.6 mm with 5 mm rounded corners; the hole is
//! r 1.5 mm, centred 3 mm in from the corner arc's centre on both axes — a
//! mounting hole, the shape of test-harness
//! `board_with_arc_corners_cutouts_keepouts_cw_winding_and_thickness_expr`.
//! The hole's circle passes within 2.74 mm of the corner arc's centre, so
//! part of it lies inside the CYLINDER that carries the corner — and still on
//! the plate's cap, which covers that cylinder's whole disc.
//!
//! The certificate reads which side of each crease's divider a face lies on.
//! Every Stage-1 mesh vertex of the cap is on or outside the corner
//! cylinder; read from the vertices alone (2026-11-14), the cap lay
//! "outside" it and the hole's circle "left" the cap: 21 `CurveLeftDomain`
//! sites that no refinement clears, and the subtraction failed with
//! `§4.5.2 local refinement ended with … unresolved failure site(s)`. Read
//! from the cap's triangles the cap is on both sides, and there is no
//! certificate there.

use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{boolean_op, extrude, geom, validate_solid, BrepArena, Profile, ProfileEdge};

const A: f64 = 0.030;
const B: f64 = 0.020;
const RC: f64 = 0.005;
const T: f64 = 0.0016;
const R_HOLE: f64 = 0.0015;

/// The plate's outline: half-extents `(A, B)`, quarter-circle corners of
/// radius `RC`, CCW about +z.
fn rounded_rect() -> Profile {
    let p = |x, y| Point2::new(x, y);
    let arc = |a, b, center| ProfileEdge::Arc {
        a,
        b,
        center,
        radius: RC,
        ccw: true,
    };
    let line = |a, b| ProfileEdge::Line { a, b };
    let (x, y) = (A - RC, B - RC);
    let edges = vec![
        line(p(-x, -B), p(x, -B)),
        arc(p(x, -B), p(A, -y), p(x, -y)),
        line(p(A, -y), p(A, y)),
        arc(p(A, y), p(x, B), p(x, y)),
        line(p(x, B), p(-x, B)),
        arc(p(-x, B), p(-A, y), p(-x, y)),
        line(p(-A, y), p(-A, -y)),
        arc(p(-A, -y), p(-x, -B), p(-x, -y)),
    ];
    Profile::arc_polygon(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        edges,
        vec![],
    )
    .expect("rounded rectangle")
}

#[test]
fn a_hole_beside_a_rounded_corner_is_cut() {
    let mut arena = BrepArena::new();
    let plate = extrude(&mut arena, &rounded_rect(), Vector3::new(0.0, 0.0, 1.0), T)
        .expect("plate")
        .solid;
    // 3 mm in from the south-west corner arc's centre on both axes.
    let centre = Point2::new(-(A - RC) + 0.003, -(B - RC) + 0.003);
    let drill = Profile::circle(
        Point3::new(0.0, 0.0, -0.001),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        centre,
        R_HOLE,
    )
    .expect("hole profile");
    let drill = extrude(&mut arena, &drill, Vector3::new(0.0, 0.0, 1.0), T + 0.002)
        .expect("drill")
        .solid;

    let out = boolean_op(&mut arena, plate, drill, BoolOp::Subtract)
        .expect("a hole beside a rounded corner is cut");

    let report = validate_solid(&arena, out).expect("the drilled plate validates");
    // 8 walls + 2 caps + the hole's wall; one through hole.
    assert_eq!(report.faces, 11);
    assert_eq!(report.genus, 1);

    let area = 4.0 * A * B - RC * RC * (4.0 - PI) - PI * R_HOLE * R_HOLE;
    let vol = geom::signed_volume(&arena, out).expect("exact volume");
    assert!(
        (vol - area * T).abs() <= 1e-9 * area * T,
        "volume {vol} vs {}",
        area * T
    );
}
