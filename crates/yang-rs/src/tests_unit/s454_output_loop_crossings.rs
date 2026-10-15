//! T3c — the output-level §4.5.4 check WITHIN one face
//! (`failures::output_loop_crossings`, spec `yang_452_refinement_loop.md`
//! §3 "T3c"): a face whose own loops cross once every edge is read on its
//! exact curve is an illegal intersection (`:752-758`). Anchor: P0047 with
//! the §4.3.3 arms off — a prism side whose cut-cylinder ellipse arc bulges
//! past the face's own straight edge. And its orientation half: P0044 — a
//! cylinder patch whose one loop winds against its material side. Output
//! loops are CCW viewed from outside (`BRepFace::outer_loop`); the
//! Stage-1-style `rj_box` fixture walks CW and is not an output fixture.

use crate::boolean::failures::{output_loop_crossings, FailureTrigger};
use crate::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, Point3, Surface, Vector3};

fn plane_face(outer_loop: Vec<u32>) -> BRepFace {
    BRepFace {
        surface: Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        },
        outer_loop,
        inner_loops: Vec::new(),
        reversed: false,
    }
}

fn v(x: f64, y: f64) -> BRepVertex {
    BRepVertex {
        point: Point3::new(x, y, 0.0),
    }
}

fn line(a: u32, b: u32) -> BRepEdge {
    BRepEdge {
        start: a,
        end: b,
        curve: Curve::LineSegment,
    }
}

/// A thin rectangle 1 × `h`, walked CCW, whose TOP edge (1, h) → (0, h) is
/// the minor arc of a circle centred at (0.5, h + `c`).
fn thin_rectangle_with_arc_top(h: f64, c: f64) -> BRep {
    let radius = (0.25f64 + c * c).sqrt();
    let verts = vec![v(0.0, 0.0), v(1.0, 0.0), v(1.0, h), v(0.0, h)];
    let edges = vec![
        line(0, 1),
        line(1, 2),
        BRepEdge {
            start: 2,
            end: 3,
            curve: Curve::Circle {
                center: Point3::new(0.5, h + c, 0.0),
                normal: Vector3::new(0.0, 0.0, 1.0),
                radius,
            },
        },
        line(3, 0),
    ];
    BRep::new(verts, edges, vec![plane_face(vec![0, 1, 2, 3])]).expect("valid fixture")
}

/// A unit square, walked CCW about +z (`ccw`) or CW.
fn square(ccw: bool) -> BRep {
    let verts = vec![v(0.0, 0.0), v(1.0, 0.0), v(1.0, 1.0), v(0.0, 1.0)];
    let (edges, lp) = if ccw {
        (
            vec![line(0, 1), line(1, 2), line(2, 3), line(3, 0)],
            vec![0, 1, 2, 3],
        )
    } else {
        (
            vec![line(0, 3), line(3, 2), line(2, 1), line(1, 0)],
            vec![0, 1, 2, 3],
        )
    };
    BRep::new(verts, edges, vec![plane_face(lp)]).expect("valid fixture")
}

/// A patch of the unit cylinder about +z: θ ∈ [0, 0.5], h ∈ [0, 1], walked
/// CCW viewed from outside (`ccw`) or the other way; `reversed` sets the
/// face's flag (a cavity wall, whose outside is the axis side).
fn cylinder_patch(ccw: bool, reversed: bool) -> BRep {
    let p = |th: f64, h: f64| BRepVertex {
        point: Point3::new(th.cos(), th.sin(), h),
    };
    let verts = vec![p(0.0, 0.0), p(0.5, 0.0), p(0.5, 1.0), p(0.0, 1.0)];
    // `nz` orients the arc's normal so it sweeps CCW about it from start to
    // end (the constructor's reading) — always the 0.5-rad minor arc.
    let arc = |a: u32, b: u32, h: f64, nz: f64| BRepEdge {
        start: a,
        end: b,
        curve: Curve::Circle {
            center: Point3::new(0.0, 0.0, h),
            normal: Vector3::new(0.0, 0.0, nz),
            radius: 1.0,
        },
    };
    let edges = if ccw {
        vec![
            arc(0, 1, 0.0, 1.0),
            line(1, 2),
            arc(2, 3, 1.0, -1.0),
            line(3, 0),
        ]
    } else {
        vec![
            line(0, 3),
            arc(3, 2, 1.0, 1.0),
            line(2, 1),
            arc(1, 0, 0.0, -1.0),
        ]
    };
    let face = BRepFace {
        surface: Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        },
        outer_loop: vec![0, 1, 2, 3],
        inner_loops: Vec::new(),
        reversed,
    };
    BRep::new(verts, edges, vec![face]).expect("valid fixture")
}

#[test]
fn a_ccw_square_is_clean_and_a_cw_square_winds_against_its_side() {
    assert!(output_loop_crossings(&square(true)).is_empty());
    let sites = output_loop_crossings(&square(false));
    assert_eq!(sites.len(), 1, "{sites:?}");
    assert!(sites[0].detail.contains("winds against"), "{sites:?}");
}

#[test]
fn a_cylinder_patch_winds_with_its_material_side_or_is_a_site() {
    // CCW from outside on an outward face, and the same walk reversed on a
    // cavity wall (`reversed`), are both clean; the mismatched pairs are
    // P0044's signature.
    assert!(output_loop_crossings(&cylinder_patch(true, false)).is_empty());
    assert!(output_loop_crossings(&cylinder_patch(false, true)).is_empty());
    for (ccw, reversed) in [(false, false), (true, true)] {
        let sites = output_loop_crossings(&cylinder_patch(ccw, reversed));
        assert_eq!(sites.len(), 1, "ccw={ccw} reversed={reversed}: {sites:?}");
        assert!(sites[0].detail.contains("winds against"), "{sites:?}");
    }
}

#[test]
fn an_arc_that_stays_inside_the_face_is_not_a_crossing() {
    // Sagitta 1.118 − 1.0 ≈ 0.118 below the top on a 0.2-tall face.
    let b = thin_rectangle_with_arc_top(0.2, 1.0);
    assert!(output_loop_crossings(&b).is_empty());
}

#[test]
fn an_arc_that_bulges_through_the_opposite_edge_is_a_crossing() {
    // Same arc on a 0.1-tall face: its lowest point (0.5, −0.018) lies below
    // the bottom edge, so the exact loop crosses itself twice.
    let b = thin_rectangle_with_arc_top(0.1, 1.0);
    let sites = output_loop_crossings(&b);
    assert_eq!(sites.len(), 1, "{sites:?}");
    assert_eq!(sites[0].trigger, FailureTrigger::IllegalIntersection);
}

/// A narrower cylinder (axis x, r = 0.95) through a wider one (axis z,
/// r = 1): their intersection is TWO loops, one at x > 0 and one at x < 0.
fn two_loop_pair() -> (Surface, Surface) {
    (
        Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        },
        Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(1.0, 0.0, 0.0),
            radius: 0.95,
        },
    )
}

/// The pair-curve point at height-side `y` on the loop with sign `sx`.
fn on_loop(sx: f64, y: f64) -> Point3 {
    Point3::new(
        sx * (1.0 - y * y).sqrt(),
        y,
        (0.95f64 * 0.95 - y * y).sqrt(),
    )
}

#[test]
fn a_surface_pair_edge_on_one_branch_samples_onto_both_surfaces() {
    use crate::boolean::failures::sample_output_edge;
    let (a, b) = two_loop_pair();
    let curve = Curve::SurfacePair { a, b };
    let pts = sample_output_edge(&curve, on_loop(1.0, 0.5), on_loop(1.0, -0.5), false, 1e-7)
        .expect("one branch: a valid edge");
    assert!(
        pts.len() > 8,
        "a curved piece samples densely: {}",
        pts.len()
    );
    for p in &pts {
        let q = Point3::new(p[0], p[1], p[2]);
        for s in [a, b] {
            let d = crate::signed_distance_to_surface(s, q).expect("distance");
            assert!(d.abs() < 1e-9, "sample {p:?} is {d:e} off a surface");
        }
    }
}

#[test]
fn a_surface_pair_edge_bridging_two_branches_is_invalid() {
    use crate::boolean::failures::{sample_output_edge, EdgeRead};
    let (a, b) = two_loop_pair();
    let curve = Curve::SurfacePair { a, b };
    // One end on the x > 0 loop, the other on the x < 0 loop: the chord
    // crosses the gap between the branches, and no bisection converges onto
    // it. (Different heights: at a mirror-symmetric midpoint x = 0 both
    // normals lose their x-component and the Newton cannot leave that plane.)
    match sample_output_edge(&curve, on_loop(1.0, 0.5), on_loop(-1.0, 0.3), false, 1e-7) {
        Err(EdgeRead::Invalid(why)) => assert!(why.contains("branches"), "{why}"),
        other => panic!("expected an invalid edge, got {other:?}"),
    }
}
