// ====================================================================
// Yang §4.3 curve-interior domain certificate (trigger T9,
// `stage4_correct::curve_domain_postcondition`): the side of each crease's
// divider an input face occupies, read from the face's own Stage-1 mesh
// (`stage4_correct::face_sides`). The end-to-end pin is the integration test
// `tests/s43_curve_domain.rs` (the F0076 site at every rim phase).
// ====================================================================

use super::plane_weld::box_brep;
use super::*;
use crate::stage4_boundary_curve::CreaseIndex;
use crate::stage4_correct::face_sides;

/// A closed cylinder about +z: lateral face 0, base cap 1, top cap 2.
fn closed_cylinder(radius: f64, height: f64) -> BRep {
    let verts = vec![
        BRepVertex {
            point: Point3::new(radius, 0.0, 0.0),
        },
        BRepVertex {
            point: Point3::new(radius, 0.0, height),
        },
    ];
    let rim = |z: f64, v: u32, nz: f64| BRepEdge {
        start: v,
        end: v,
        curve: Curve::Circle {
            center: Point3::new(0.0, 0.0, z),
            normal: Vector3::new(0.0, 0.0, nz),
            radius,
        },
    };
    let edges = vec![
        rim(0.0, 0, -1.0),
        rim(height, 1, 1.0),
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, -1.0),
                d: 0.0,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -height,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("closed cylinder")
}

#[test]
fn a_box_face_lies_on_one_side_of_each_of_its_edges_with_no_margin() {
    let b = box_brep([0.0, 0.0, 0.0], [1.0, 2.0, 3.0]);
    let index = CreaseIndex::build(&b);
    for (fi, creases) in index.faces.iter().enumerate() {
        assert_eq!(creases.len(), 4, "face {fi}");
        let sides = face_sides(&b, fi as u32, creases);
        assert!(
            sides.sign.iter().all(Option::is_some),
            "face {fi}: {:?}",
            sides.sign
        );
        assert_eq!(
            sides.margin, 0.0,
            "face {fi}: a planar face with straight edges"
        );
    }
}

#[test]
fn a_cylinder_lateral_lies_between_its_rim_planes_within_its_chord_bound() {
    let c = closed_cylinder(1.0, 2.0);
    let index = CreaseIndex::build(&c);
    let creases = &index.faces[0];
    let sides = face_sides(&c, 0, creases);
    // The two rims (B-Rep edges 0 and 1) each get a side. The seam's radial
    // plane has the lateral on both sides of it: no side, no certificate.
    let rims: Vec<usize> = (0..creases.len())
        .filter(|&k| creases[k].edge < 2)
        .collect();
    assert_eq!(rims.len(), 2);
    for (k, c) in creases.iter().enumerate() {
        assert_eq!(
            sides.sign[k].is_some(),
            c.edge < 2,
            "crease {k} (edge {})",
            c.edge
        );
    }
    // A point of the lateral at mid-height is on the face's side of both
    // rim planes; a point below the base is not.
    let on = |z: f64, k: usize| -> f64 {
        let g =
            crate::geom::signed_distance_to_surface(creases[k].divider, Point3::new(1.0, 0.0, z))
                .expect("plane distance");
        sides.sign[k].expect("signed") * g
    };
    assert!(on(1.0, rims[0]) > 0.0 && on(1.0, rims[1]) > 0.0);
    assert!(on(-0.5, rims[0]).min(on(-0.5, rims[1])) < 0.0);
    // The margin is the face's own distance from its mesh: positive, and no
    // larger than the radius.
    assert!(sides.margin > 0.0 && sides.margin < 1.0, "{}", sides.margin);
}

#[test]
fn a_face_with_witnesses_on_both_sides_of_a_divider_gets_no_side() {
    // The lateral face read against a plane through its own middle: Stage-1
    // vertices on both rims straddle it, so there is no certificate.
    let c = closed_cylinder(1.0, 2.0);
    let index = CreaseIndex::build(&c);
    let mut crease = index.faces[0][0];
    crease.divider = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -1.0,
    };
    let sides = face_sides(&c, 0, &[crease]);
    assert_eq!(sides.sign, vec![None]);
}

/// The top cap of [`closed_cylinder`] read against one crease whose divider
/// is the cylinder of radius `r` about the +z line through `(x, 0)`.
fn cap_against_cylinder(x: f64, r: f64) -> crate::stage4_correct::FaceSides {
    let c = closed_cylinder(1.0, 2.0);
    let index = CreaseIndex::build(&c);
    let mut crease = index.faces[2][0];
    crease.divider = Surface::Cylinder {
        axis_point: Point3::new(x, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: r,
    };
    face_sides(&c, 2, &[crease])
}

#[test]
fn a_face_that_covers_a_cylinder_its_corners_are_all_outside_gets_no_side() {
    // A plate with a rounded corner, as a hole drilled beside that corner
    // meets it (test-harness `board_with_arc_corners_…`, 2026-11-15): the
    // corner arc's divider is its whole cylinder, every mesh vertex of the
    // plate's cap is on or outside it, and the cap covers its disc. Here the
    // unit disc against a cylinder of radius 0.2 well inside its rim — no
    // rim vertex is inside the small cylinder, and the disc's triangles are.
    let sides = cap_against_cylinder(0.3, 0.2);
    assert_eq!(sides.sign, vec![None]);
    assert_eq!(sides.reach, vec![0.0]);
}

#[test]
fn a_face_wholly_outside_a_cylinder_keeps_that_side() {
    let sides = cap_against_cylinder(3.0, 0.5);
    assert_eq!(sides.sign, vec![Some(1.0)]);
    assert_eq!(sides.reach, vec![0.0]);
}

#[test]
fn a_dividers_range_on_a_triangle_is_read_inside_it_not_only_at_its_corners() {
    use crate::stage4_correct::divider_range_on_triangle;
    let cylinder = |x: f64, y: f64, r: f64| Surface::Cylinder {
        axis_point: Point3::new(x, y, -7.0),
        axis_dir: Vector3::new(0.0, 0.0, 2.0),
        radius: r,
    };
    let tri = [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [0.0, 4.0, 0.0]];
    // The axis passes through the triangle: the least distance is −r, though
    // every corner is outside.
    let (lo, hi) = divider_range_on_triangle(cylinder(1.0, 1.0, 0.25), tri).expect("range");
    assert_eq!(lo, -0.25);
    assert!((hi - (10.0f64.sqrt() - 0.25)).abs() < 1e-15, "{hi}");
    // The axis passes outside, nearest the hypotenuse: the least distance is
    // on that side, not at a corner.
    let (lo, _) = divider_range_on_triangle(cylinder(3.0, 3.0, 0.5), tri).expect("range");
    assert!((lo - (2.0f64.sqrt() - 0.5)).abs() < 1e-15, "{lo}");
    // A chord of the cylinder's own circle dips inside it by its sagitta.
    let (s, c) = (0.3f64.sin(), 0.3f64.cos());
    let chord = [[c, -s, 1.0], [c, s, 1.0], [5.0, 0.0, 1.0]];
    let (lo, _) = divider_range_on_triangle(cylinder(0.0, 0.0, 1.0), chord).expect("range");
    assert!((lo - (c - 1.0)).abs() < 1e-15, "{lo}");
    // A plane is linear: its range is its corners'.
    let plane = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -1.0,
    };
    assert_eq!(divider_range_on_triangle(plane, tri), Some((-1.0, 3.0)));
    // A cone's residual is neither linear nor convex: declined.
    let cone = Surface::Cone {
        apex: Point3::new(0.0, 0.0, 5.0),
        axis_dir: Vector3::new(0.0, 0.0, -1.0),
        half_angle: 0.4,
    };
    assert_eq!(divider_range_on_triangle(cone, tri), None);
}
