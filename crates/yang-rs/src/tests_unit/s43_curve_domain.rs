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
