//! §4.4.2 carried-edge curve restoration unit tests (spec
//! `yang_434_output_chord_refinement.md` inc-1, revised): the
//! `restore_carried_edge_curves` pass re-types same-input boundary chords
//! onto their carried input circles — certification-driven, declining
//! anything off-curve, ambiguous, or wide-sweeping, and never touching
//! vertices, loop structure, or edge indices.

use crate::brep::{BRepEdge, BRepFace, InputId, TriangleAttribution};
use crate::geom::{Curve, Surface};
use crate::stage5_output_refine::restore_carried_edge_curves;
use crate::{Point3, Vector3};

/// Minimal input topology: `faces[i]` has a one-entry outer loop naming
/// `edges[i]`; edge endpoints are irrelevant to candidate collection (only
/// the curve is read).
fn input_with_curves(curves: &[Curve]) -> (Vec<BRepFace>, Vec<BRepEdge>) {
    let plane = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let mut faces = Vec::new();
    let mut edges = Vec::new();
    for (i, &c) in curves.iter().enumerate() {
        edges.push(BRepEdge {
            start: 0,
            end: 0,
            curve: c,
        });
        faces.push(BRepFace {
            surface: plane,
            outer_loop: vec![i as u32],
            inner_loops: Vec::new(),
            reversed: false,
        });
    }
    (faces, edges)
}

fn circle_z(radius: f64) -> Curve {
    Curve::Circle {
        center: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius,
    }
}

/// Output fixture: one chord (v0→v1 on face 0, v1→v0 on face 1) plus a
/// second, unrelated straight chord (v2→v3 / v3→v2) as a control that must
/// never be re-typed.
struct Fixture {
    verts: Vec<Point3>,
    edges: Vec<BRepEdge>,
    faces: Vec<BRepFace>,
    attr: Vec<TriangleAttribution>,
}

fn fixture(theta: f64, attr: [(InputId, u32); 2]) -> Fixture {
    let r = 10.0;
    let verts = vec![
        Point3::new(r, 0.0, 0.0),
        Point3::new(r * theta.cos(), r * theta.sin(), 0.0),
        Point3::new(100.0, 100.0, 0.0),
        Point3::new(101.0, 100.0, 0.0),
    ];
    let seg = |s: u32, e: u32| BRepEdge {
        start: s,
        end: e,
        curve: Curve::LineSegment,
    };
    let edges = vec![seg(0, 1), seg(2, 3), seg(1, 0), seg(3, 2)];
    // Two DISTINCT cones that both contain the r=10 circle at z=0 (apexes
    // ±10 on the axis, half-angle π/4) — the restored arc's midpoint lies
    // on both, as a genuine shared rim does.
    let cone = |apex_z: f64| Surface::Cone {
        apex: Point3::new(0.0, 0.0, apex_z),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let faces = vec![
        BRepFace {
            surface: cone(-10.0),
            outer_loop: vec![0, 1],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: cone(10.0),
            outer_loop: vec![2, 3],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    let attr = vec![
        TriangleAttribution {
            input: attr[0].0,
            face: attr[0].1,
        },
        TriangleAttribution {
            input: attr[1].0,
            face: attr[1].1,
        },
    ];
    Fixture {
        verts,
        edges,
        faces,
        attr,
    }
}

#[test]
fn carried_rim_chord_typed_on_both_copies() {
    let mut fx = fixture(0.1, [(InputId::A, 0), (InputId::A, 1)]);
    let (in_faces, in_edges) = input_with_curves(&[circle_z(10.0), Curve::LineSegment]);
    let stats = restore_carried_edge_curves(
        &fx.verts,
        &mut fx.edges,
        &fx.faces,
        &fx.attr,
        (&in_faces, &in_edges),
        (&[], &[]),
    );
    assert_eq!(stats.typed_chords, 1, "{}", stats.eligible);
    // Both copies carry the circle, normals twin-opposed (traversal-oriented).
    let n_of = |e: &BRepEdge| match e.curve {
        Curve::Circle { normal, .. } => normal,
        ref c => panic!("expected circle, got {c:?}"),
    };
    let (na, nb) = (n_of(&fx.edges[0]), n_of(&fx.edges[2]));
    assert!((na.z() - 1.0).abs() < 1e-12, "forward copy CCW +z: {na:?}");
    assert!((nb.z() + 1.0).abs() < 1e-12, "reverse copy negated: {nb:?}");
    // The control chord is untouched.
    assert!(matches!(fx.edges[1].curve, Curve::LineSegment));
    assert!(matches!(fx.edges[3].curve, Curve::LineSegment));
    // No structural mutation: endpoints and counts unchanged.
    assert_eq!(fx.edges.len(), 4);
    assert_eq!((fx.edges[0].start, fx.edges[0].end), (0, 1));
    assert_eq!((fx.edges[2].start, fx.edges[2].end), (1, 0));
}

#[test]
fn off_curve_chord_declined() {
    // Wrong radius: endpoints sit 0.5 off the candidate circle.
    let mut fx = fixture(0.1, [(InputId::A, 0), (InputId::A, 1)]);
    let (in_faces, in_edges) = input_with_curves(&[circle_z(10.5)]);
    let before = fx.edges.clone();
    let stats = restore_carried_edge_curves(
        &fx.verts,
        &mut fx.edges,
        &fx.faces,
        &fx.attr,
        (&in_faces, &in_edges),
        (&[], &[]),
    );
    assert_eq!(stats.typed_chords, 0);
    // Both the rim chord AND the control chord are eligible and off the
    // r=10.5 candidate — two off-curve declines.
    assert_eq!(stats.declined_offcurve, 2);
    assert_eq!(fx.edges, before, "declined pass must be the identity");
}

#[test]
fn ambiguous_two_circles_declined() {
    // Diametral endpoints lie on BOTH the z-normal and the y-normal circle.
    let mut fx = fixture(std::f64::consts::PI, [(InputId::A, 0), (InputId::A, 1)]);
    let circle_y = Curve::Circle {
        center: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 1.0, 0.0),
        radius: 10.0,
    };
    let (in_faces, in_edges) = input_with_curves(&[circle_z(10.0), circle_y]);
    let before = fx.edges.clone();
    let stats = restore_carried_edge_curves(
        &fx.verts,
        &mut fx.edges,
        &fx.faces,
        &fx.attr,
        (&in_faces, &in_edges),
        (&[], &[]),
    );
    assert_eq!(stats.typed_chords, 0);
    assert_eq!(stats.declined_ambiguous, 1);
    assert_eq!(fx.edges, before);
}

#[test]
fn wide_sweep_declined() {
    // 2.0 rad > π/2: not a mesh-density chord — declined by the sweep guard.
    let mut fx = fixture(2.0, [(InputId::A, 0), (InputId::A, 1)]);
    let (in_faces, in_edges) = input_with_curves(&[circle_z(10.0)]);
    let before = fx.edges.clone();
    let stats = restore_carried_edge_curves(
        &fx.verts,
        &mut fx.edges,
        &fx.faces,
        &fx.attr,
        (&in_faces, &in_edges),
        (&[], &[]),
    );
    assert_eq!(stats.typed_chords, 0);
    assert_eq!(stats.declined_sweep, 1);
    assert_eq!(fx.edges, before);
}

#[test]
fn cross_input_chord_not_eligible() {
    // The chord's owners descend from different INPUTS: intersection-seam
    // territory (owned by `intersection_curves`), never this pass.
    let mut fx = fixture(0.1, [(InputId::A, 0), (InputId::B, 0)]);
    let (in_faces, in_edges) = input_with_curves(&[circle_z(10.0)]);
    let before = fx.edges.clone();
    let stats = restore_carried_edge_curves(
        &fx.verts,
        &mut fx.edges,
        &fx.faces,
        &fx.attr,
        (&in_faces, &in_edges),
        (&in_faces, &in_edges),
    );
    assert_eq!(stats.typed_chords, 0);
    assert_eq!(stats.eligible, 0);
    assert_eq!(fx.edges, before);
}

#[test]
fn same_input_face_pair_not_eligible() {
    // Both owners descend from the SAME input face: a mesh-seam inside one
    // face, not a carried boundary edge.
    let mut fx = fixture(0.1, [(InputId::A, 0), (InputId::A, 0)]);
    let (in_faces, in_edges) = input_with_curves(&[circle_z(10.0)]);
    let before = fx.edges.clone();
    let stats = restore_carried_edge_curves(
        &fx.verts,
        &mut fx.edges,
        &fx.faces,
        &fx.attr,
        (&in_faces, &in_edges),
        (&[], &[]),
    );
    assert_eq!(stats.typed_chords, 0);
    assert_eq!(stats.eligible, 0);
    assert_eq!(fx.edges, before);
}

#[test]
fn chord_of_rim_between_edge_on_planes_declined() {
    // Both endpoints lie EXACTLY on the r=10 rim circle, but the owner
    // faces are planes CONTAINING the chord line and edge-on to the circle
    // (the R0063 anchor): the arc's midpoint bulges off both planes, so
    // the domain certification must decline — the carried edge here is the
    // straight chord itself, not the rim.
    let mut fx = fixture(0.1, [(InputId::A, 0), (InputId::A, 1)]);
    let (p, q) = (fx.verts[0], fx.verts[1]);
    let d = [q.x() - p.x(), q.y() - p.y(), q.z() - p.z()];
    let n1 = {
        // chord × z: a normal perpendicular to the chord, in the z=0 plane.
        let n = [d[1], -d[0], 0.0];
        let l = (n[0] * n[0] + n[1] * n[1]).sqrt();
        [n[0] / l, n[1] / l, 0.0]
    };
    let n2 = {
        // chord × n1: the other independent normal containing the chord.
        let n = [
            d[1] * n1[2] - d[2] * n1[1],
            d[2] * n1[0] - d[0] * n1[2],
            d[0] * n1[1] - d[1] * n1[0],
        ];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        [n[0] / l, n[1] / l, n[2] / l]
    };
    let plane = |n: [f64; 3]| Surface::Plane {
        normal: Vector3::new(n[0], n[1], n[2]),
        d: -(n[0] * p.x() + n[1] * p.y() + n[2] * p.z()),
    };
    fx.faces[0].surface = plane(n1);
    fx.faces[1].surface = plane(n2);
    let (in_faces, in_edges) = input_with_curves(&[circle_z(10.0)]);
    let before = fx.edges.clone();
    let stats = restore_carried_edge_curves(
        &fx.verts,
        &mut fx.edges,
        &fx.faces,
        &fx.attr,
        (&in_faces, &in_edges),
        (&[], &[]),
    );
    assert_eq!(stats.typed_chords, 0);
    assert_eq!(stats.declined_midpoint, 1, "{}", stats.eligible);
    assert_eq!(fx.edges, before, "declined pass must be the identity");
}
