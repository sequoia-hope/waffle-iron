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

/// P0007 (2026-09-29): a re-entered OBLIQUE plane × cylinder boundary is a
/// `Curve::Ellipse` input edge. The pass must re-type its chords the way it
/// re-types circle rims — endpoints in band, one candidate, sweep ≤ π/2,
/// minor-arc midpoint on both owner surfaces — with twin-opposed normals.
/// The owner faces are the cylinder (axis z, r = 10) and the plane through
/// the origin with normal (sin φ, 0, cos φ); the section ellipse has
/// major axis (cos φ, 0, −sin φ), a = r / cos φ, b = r.
fn oblique_section() -> (Curve, Surface, Surface) {
    let r = 10.0;
    let phi = 0.5f64;
    let (sp, cp) = (phi.sin(), phi.cos());
    let ellipse = Curve::Ellipse {
        center: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(sp, 0.0, cp),
        major_axis: Vector3::new(cp, 0.0, -sp),
        major_radius: r / cp,
        minor_radius: r,
    };
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: r,
    };
    let plane = Surface::Plane {
        normal: Vector3::new(sp, 0.0, cp),
        d: 0.0,
    };
    (ellipse, cyl, plane)
}

fn ellipse_fixture(t0: f64, t1: f64) -> Fixture {
    let (ellipse, cyl, plane) = oblique_section();
    let mut fx = fixture(0.1, [(InputId::A, 0), (InputId::A, 1)]);
    let at = |t: f64| crate::geom::conic_eval(&ellipse, t).expect("ellipse eval");
    fx.verts[0] = at(t0);
    fx.verts[1] = at(t1);
    fx.faces[0].surface = cyl;
    fx.faces[1].surface = plane;
    fx
}

#[test]
fn carried_ellipse_chord_typed_on_both_copies() {
    let (ellipse, _, _) = oblique_section();
    let mut fx = ellipse_fixture(0.3, 0.5);
    let (in_faces, in_edges) = input_with_curves(&[ellipse, Curve::LineSegment]);
    // RED before the ellipse arm: `no_candidate` — the pass read circles
    // only and the chord stayed a LineSegment.
    let stats = restore_carried_edge_curves(
        &fx.verts,
        &mut fx.edges,
        &fx.faces,
        &fx.attr,
        (&in_faces, &in_edges),
        (&[], &[]),
    );
    assert_eq!(stats.typed_chords, 1, "{stats:?}");
    assert_eq!(stats.typed_ellipses, 1);
    let n_of = |e: &BRepEdge| match e.curve {
        Curve::Ellipse { normal, .. } => normal,
        ref c => panic!("expected ellipse, got {c:?}"),
    };
    let (na, nb) = (n_of(&fx.edges[0]), n_of(&fx.edges[2]));
    assert!(
        (na.x() + nb.x()).abs() < 1e-12 && (na.z() + nb.z()).abs() < 1e-12,
        "twin-opposed normals: {na:?} {nb:?}"
    );
    assert!(matches!(fx.edges[1].curve, Curve::LineSegment));
    assert!(matches!(fx.edges[3].curve, Curve::LineSegment));
    assert_eq!((fx.edges[0].start, fx.edges[0].end), (0, 1));
    assert_eq!((fx.edges[2].start, fx.edges[2].end), (1, 0));
}

#[test]
fn off_ellipse_chord_declined() {
    // Wrong minor radius: both endpoints sit ~0.5 off the candidate.
    let mut fx = ellipse_fixture(0.3, 0.5);
    let (ellipse, _, _) = oblique_section();
    let Curve::Ellipse {
        center,
        normal,
        major_axis,
        major_radius,
        ..
    } = ellipse
    else {
        unreachable!()
    };
    let wrong = Curve::Ellipse {
        center,
        normal,
        major_axis,
        major_radius,
        minor_radius: 10.5,
    };
    let (in_faces, in_edges) = input_with_curves(&[wrong]);
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
    assert!(stats.declined_offcurve >= 1, "{stats:?}");
    assert_eq!(fx.edges, before, "declined pass must be the identity");
}

#[test]
fn wide_ellipse_sweep_declined() {
    let (ellipse, _, _) = oblique_section();
    let mut fx = ellipse_fixture(0.0, 2.0);
    let (in_faces, in_edges) = input_with_curves(&[ellipse]);
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
    assert_eq!(stats.declined_sweep, 1, "{stats:?}");
    assert_eq!(fx.edges, before);
}

// ---- P0033 (2026-10-11): the hyperbola and surface-pair arms ----------------
//
// A re-entering body's axis-steep plane × cone sections (`Hyperbola`) and
// cylinder × cone curves (`SurfacePair`) left the second boolean as
// mesh-density `LineSegment` chords: the pair chord's 9.2e-7 sag sat inside
// Stage 1's 1e-2·r chain bound and 25× above kernel-v2's render facet band,
// the KV9-F2a deep-chord fold on P0033's face 45. The restoration vocabulary
// is every typed input curve; these pins are RED before the two arms
// (`no_candidate` — the pass read conics only).

/// Cylinder (axis z through (0.5, 0, 0), r = 1) × cone (apex at the origin,
/// axis +z, half-angle π/4): the pair curve satisfies x = z² − 3/4 with
/// y² = z² − x². Points are seeded from that closed form at height `z` and
/// Newton-polished onto both surfaces, exactly as a Stage-4 vertex is.
fn pair_fixture_surfaces() -> (Surface, Surface) {
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(0.5, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let cone = Surface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    (cyl, cone)
}

fn pair_point_at(z: f64) -> Point3 {
    let (cyl, cone) = pair_fixture_surfaces();
    let x = z * z - 0.75;
    let y = (z * z - x * x).sqrt();
    crate::stage4_relocate::relocate_onto_implicit_pair(Point3::new(x, y, z), cyl, cone)
        .expect("seed converges onto the pair curve")
}

fn pair_fixture(z0: f64, z1: f64) -> Fixture {
    let (cyl, cone) = pair_fixture_surfaces();
    let mut fx = fixture(0.1, [(InputId::A, 0), (InputId::A, 1)]);
    fx.verts[0] = pair_point_at(z0);
    fx.verts[1] = pair_point_at(z1);
    fx.faces[0].surface = cyl;
    fx.faces[1].surface = cone;
    fx
}

#[test]
fn carried_surface_pair_chord_typed_on_both_copies() {
    let (cyl, cone) = pair_fixture_surfaces();
    let pair = Curve::SurfacePair { a: cyl, b: cone };
    let mut fx = pair_fixture(1.40, 1.42);
    let (in_faces, in_edges) = input_with_curves(&[pair, Curve::LineSegment]);
    let stats = restore_carried_edge_curves(
        &fx.verts,
        &mut fx.edges,
        &fx.faces,
        &fx.attr,
        (&in_faces, &in_edges),
        (&[], &[]),
    );
    assert_eq!(stats.typed_chords, 1, "{stats:?}");
    assert_eq!(stats.typed_pairs, 1);
    // Both copies carry the input descriptor bit-for-bit (a pair has no
    // directional normal; `from_yang` demands identical twins).
    assert_eq!(fx.edges[0].curve, pair);
    assert_eq!(fx.edges[2].curve, pair);
    assert!(matches!(fx.edges[1].curve, Curve::LineSegment));
    assert!(matches!(fx.edges[3].curve, Curve::LineSegment));
    assert_eq!((fx.edges[0].start, fx.edges[0].end), (0, 1));
    assert_eq!((fx.edges[2].start, fx.edges[2].end), (1, 0));
}

#[test]
fn off_pair_chord_declined() {
    let (cyl, cone) = pair_fixture_surfaces();
    let pair = Curve::SurfacePair { a: cyl, b: cone };
    let mut fx = pair_fixture(1.40, 1.42);
    // Lift one endpoint along the cylinder's generator: still on the
    // cylinder, 7e-4 off the cone — far outside the 1e-9 import band.
    let v = fx.verts[0];
    fx.verts[0] = Point3::new(v.x(), v.y(), v.z() + 1e-3);
    let (in_faces, in_edges) = input_with_curves(&[pair]);
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
    assert!(stats.declined_offcurve >= 1, "{stats:?}");
    assert_eq!(fx.edges, before, "declined pass must be the identity");
}

#[test]
fn wide_pair_chord_past_the_chain_bound_declined() {
    // Both endpoints ON the curve, but the chord spans a quarter of the
    // loop: its midpoint projects 0.1 off the chord, past the Stage-1 chain
    // bound (1e-2 × the smallest local radius) — not a chord the chain
    // pre-pass would hold, so not one the restoration may mint.
    let (cyl, cone) = pair_fixture_surfaces();
    let pair = Curve::SurfacePair { a: cyl, b: cone };
    let mut fx = pair_fixture(1.00, 1.45);
    let (in_faces, in_edges) = input_with_curves(&[pair]);
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
    assert_eq!(stats.declined_sweep, 1, "{stats:?}");
    assert_eq!(fx.edges, before);
}

#[test]
fn pair_chord_whose_owner_is_not_a_pair_surface_declined() {
    // Endpoints on the pair curve, but one OWNER face is a plane the curve
    // never touches: the projected midpoint fails the owner-domain check.
    let (cyl, cone) = pair_fixture_surfaces();
    let pair = Curve::SurfacePair { a: cyl, b: cone };
    let mut fx = pair_fixture(1.40, 1.42);
    fx.faces[1].surface = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let (in_faces, in_edges) = input_with_curves(&[pair]);
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
    assert_eq!(stats.declined_midpoint, 1, "{stats:?}");
    assert_eq!(fx.edges, before);
}

/// Cone (apex at the origin, axis +z, half-angle π/4) × the plane x = 2: the
/// section is the hyperbola z²/4 − y²/4 = 1 in that plane — centre (2, 0, 0),
/// normal x, transverse axis z, a = b = 2. `hyperbola_point` walks it as
/// (2, −2 sinh t, 2 cosh t).
fn steep_section() -> (Curve, Surface, Surface) {
    let hyp = Curve::Hyperbola {
        center: Point3::new(2.0, 0.0, 0.0),
        normal: Vector3::new(1.0, 0.0, 0.0),
        major_axis: Vector3::new(0.0, 0.0, 1.0),
        semi_transverse: 2.0,
        semi_conjugate: 2.0,
    };
    let cone = Surface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: std::f64::consts::FRAC_PI_4,
    };
    let plane = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -2.0,
    };
    (hyp, cone, plane)
}

fn hyperbola_fixture(t0: f64, t1: f64) -> Fixture {
    let (hyp, cone, plane) = steep_section();
    let Curve::Hyperbola {
        center,
        normal,
        major_axis,
        semi_transverse,
        semi_conjugate,
    } = hyp
    else {
        unreachable!()
    };
    let at = |t: f64| {
        crate::geom::hyperbola_point(
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
            t,
        )
    };
    let mut fx = fixture(0.1, [(InputId::A, 0), (InputId::A, 1)]);
    fx.verts[0] = at(t0);
    fx.verts[1] = at(t1);
    fx.faces[0].surface = cone;
    fx.faces[1].surface = plane;
    fx
}

#[test]
fn carried_hyperbola_chord_typed_on_both_copies() {
    let (hyp, _, _) = steep_section();
    let mut fx = hyperbola_fixture(0.3, 0.5);
    let (in_faces, in_edges) = input_with_curves(&[hyp, Curve::LineSegment]);
    let stats = restore_carried_edge_curves(
        &fx.verts,
        &mut fx.edges,
        &fx.faces,
        &fx.attr,
        (&in_faces, &in_edges),
        (&[], &[]),
    );
    assert_eq!(stats.typed_chords, 1, "{stats:?}");
    assert_eq!(stats.typed_hyperbolas, 1);
    // Hyperbola twins are endpoint-oriented: identical descriptors.
    assert_eq!(fx.edges[0].curve, hyp);
    assert_eq!(fx.edges[2].curve, hyp);
    assert!(matches!(fx.edges[1].curve, Curve::LineSegment));
    assert!(matches!(fx.edges[3].curve, Curve::LineSegment));
    assert_eq!((fx.edges[0].start, fx.edges[0].end), (0, 1));
    assert_eq!((fx.edges[2].start, fx.edges[2].end), (1, 0));
}

#[test]
fn off_hyperbola_chord_declined() {
    // Wrong conjugate semi-axis: both endpoints sit ~3e-2 off the candidate.
    let mut fx = hyperbola_fixture(0.3, 0.5);
    let (hyp, _, _) = steep_section();
    let Curve::Hyperbola {
        center,
        normal,
        major_axis,
        semi_transverse,
        ..
    } = hyp
    else {
        unreachable!()
    };
    let wrong = Curve::Hyperbola {
        center,
        normal,
        major_axis,
        semi_transverse,
        semi_conjugate: 2.1,
    };
    let (in_faces, in_edges) = input_with_curves(&[wrong]);
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
    assert!(stats.declined_offcurve >= 1, "{stats:?}");
    assert_eq!(fx.edges, before, "declined pass must be the identity");
}

#[test]
fn hyperbola_chord_whose_owner_misses_the_branch_declined() {
    let (hyp, _, _) = steep_section();
    let mut fx = hyperbola_fixture(0.3, 0.5);
    fx.faces[1].surface = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let (in_faces, in_edges) = input_with_curves(&[hyp]);
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
    assert_eq!(stats.declined_midpoint, 1, "{stats:?}");
    assert_eq!(fx.edges, before);
}
