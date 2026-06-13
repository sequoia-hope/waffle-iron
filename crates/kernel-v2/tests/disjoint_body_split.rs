//! Body decomposition: `split_solid_into_bodies` splits a disjoint union into
//! one body per lump (AABB-overlap clustering); overlapping unions stay one.
use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::*;

fn make_box(arena: &mut BrepArena, lo: [f64; 3], hi: [f64; 3]) -> SolidId {
    let profile = Profile::new(
        Point3::new(0.0, 0.0, lo[2]),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(lo[0], lo[1]),
            Point2::new(hi[0], lo[1]),
            Point2::new(hi[0], hi[1]),
            Point2::new(lo[0], hi[1]),
        ],
        vec![],
    )
    .unwrap();
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), hi[2] - lo[2])
        .unwrap()
        .solid
}

#[test]
fn split_disjoint_union_into_two_bodies() {
    let mut arena = BrepArena::new();
    let a = make_box(&mut arena, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let b = make_box(&mut arena, [5.0, 5.0, 0.0], [6.0, 6.0, 1.0]);
    let s = boolean_op(&mut arena, a, b, BoolOp::Union).unwrap();
    let bodies = split_solid_into_bodies(&mut arena, s).unwrap();
    eprintln!("PROBE bodies={}", bodies.len());
    assert_eq!(bodies.len(), 2, "disjoint union should split into 2 bodies");
    // Each body is a single-shell solid with volume 1.0.
    for bid in &bodies {
        assert_eq!(arena.solid(*bid).unwrap().shells.len(), 1);
        let v = geom::signed_volume(&arena, *bid).unwrap();
        assert!((v - 1.0).abs() < 1e-9, "each body volume ~1.0, got {v}");
    }
}

#[test]
fn overlapping_union_stays_one_body() {
    let mut arena = BrepArena::new();
    let a = make_box(&mut arena, [0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
    let b = make_box(&mut arena, [1.0, 1.0, 1.0], [3.0, 3.0, 3.0]); // overlaps
    let s = boolean_op(&mut arena, a, b, BoolOp::Union).unwrap();
    let bodies = split_solid_into_bodies(&mut arena, s).unwrap();
    assert_eq!(bodies.len(), 1, "overlapping union is a single body");
}
