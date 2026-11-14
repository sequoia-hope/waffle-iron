//! §4.5.1 (spec `specs/yang_451_corner_transit.md` §3t) — the
//! relocation DOMAIN certificate: the crease circles that bound a face, and
//! the predicate that says a triple-junction relocation crossed one.
//!
//! Yang §4.5.1 (`refs/text/yang2025_hybrid_boolean.txt:672-690`) names the
//! defect in its own words — *"a full step length that takes the point to a
//! position `p1` outside the surface `S2` where the point is initially
//! located"* — and prescribes truncating to the boundary curve `C_b`. The
//! three-surface Newton solves the EXTENDED implicits, so its solution is an
//! identification, never a domain certificate; this module supplies the
//! missing half.
//!
//! Measured pin: R0044 v47. Its seed sits −0.194 from the cone×cone crease
//! that bounds its face and the exact triple solution +0.827 past it, while
//! the mesh chord scale there is ~18 — which is why the certificate must be
//! ANALYTIC. Four crease-riding relocations in the same case whose residuals
//! are pure noise (1.6e-11 … 1.4e-10) must NOT fire.

use crate::stage4_boundary_curve::{
    crease_circle_from_pair, crease_crossed_by_step, crease_plane, creases_by_surface,
    creases_for_surfaces, on_crease,
};
use crate::{Curve, InputId, Surface, Vector3};
use cad_primitives::Point3;

/// R0044's own geometry: the two coaxial cones of operand B whose shared rim
/// is the crease v47's solution overruns.
///
/// The second apex is constructed BY OFFSET along the shared axis rather than
/// transcribed, so coaxiality is exact. (Transcribing it from a 7-digit probe
/// dump leaves a 1.7e-3 perpendicular residual, and
/// [`crease_circle_from_pair`] then correctly declines the pair — two
/// near-coaxial cones meet in a quartic, not a circle. That decline is the
/// contract working, and it is what a rounded fixture would have mis-read as a
/// bug.)
fn r0044_cones() -> (Surface, Surface) {
    let raw = [0.8962863611165686_f64, 0.0, -0.44347577033747876];
    let l = (raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2]).sqrt();
    let a = [raw[0] / l, raw[1] / l, raw[2] / l];
    let axis = Vector3::new(a[0], a[1], a[2]);
    let apex0 = Point3::new(-2257.132844185381, -4037.05155950119, -2210.8288982994654);
    // δ = (apex0 − apex1)·â, R0044's measured apex separation along the axis.
    let delta = 183.357_417_f64;
    let apex1 = Point3::new(
        apex0.x() - delta * a[0],
        apex0.y() - delta * a[1],
        apex0.z() - delta * a[2],
    );
    (
        Surface::Cone {
            apex: apex0,
            axis_dir: axis,
            half_angle: 1.0475950601223083,
        },
        Surface::Cone {
            apex: apex1,
            axis_dir: axis,
            half_angle: 1.0100201410,
        },
    )
}

fn station(p: Point3, apex: Point3, axis: Vector3) -> f64 {
    let d = [p.x() - apex.x(), p.y() - apex.y(), p.z() - apex.z()];
    let a = axis.as_array();
    d[0] * a[0] + d[1] * a[1] + d[2] * a[2]
}

#[test]
pub(crate) fn coaxial_cone_pair_yields_their_shared_rim_circle() {
    let (c0, c1) = r0044_cones();
    let curve = crease_circle_from_pair(c0, c1).expect("coaxial cones share a rim circle");
    let Curve::Circle {
        center,
        normal,
        radius,
    } = curve
    else {
        panic!("crease must be a circle, got {curve:?}");
    };
    // The circle is ON both cones: at its own station each cone's radius is
    // the circle's radius.
    let (
        Surface::Cone {
            apex: a0,
            axis_dir,
            half_angle: g0,
        },
        Surface::Cone {
            apex: a1,
            half_angle: g1,
            ..
        },
    ) = (c0, c1)
    else {
        unreachable!("both are cones")
    };
    for (apex, g) in [(a0, g0), (a1, g1)] {
        let h = station(center, apex, axis_dir);
        let want = h * g.tan();
        assert!(
            (want - radius).abs() <= 1e-9 * radius,
            "crease radius {radius} disagrees with cone radius {want} at its own station"
        );
    }
    // The circle's plane is perpendicular to the shared axis.
    let n = normal.as_array();
    let a = axis_dir.as_array();
    let dot = n[0] * a[0] + n[1] * a[1] + n[2] * a[2];
    assert!(
        (dot.abs() - 1.0).abs() <= 1e-12,
        "crease plane must be perpendicular to the cone axis (|cos| = {})",
        dot.abs()
    );
}

#[test]
pub(crate) fn non_coaxial_and_equal_opening_cones_have_no_circle() {
    let (c0, _) = r0044_cones();
    let Surface::Cone {
        apex,
        axis_dir,
        half_angle,
    } = c0
    else {
        unreachable!()
    };
    // Same opening angle ⇒ nested/identical, no shared circle.
    let same = Surface::Cone {
        apex: Point3::new(apex.x() + 10.0 * axis_dir.as_array()[0], apex.y(), apex.z()),
        axis_dir,
        half_angle,
    };
    assert!(
        crease_circle_from_pair(c0, same).is_none(),
        "equal half-angles share no circle — must decline, never approximate"
    );
    // Tilted axis ⇒ the pair meets in a quartic, not a circle.
    let tilted = Surface::Cone {
        apex,
        axis_dir: Vector3::new(0.0, 1.0, 0.0),
        half_angle: 0.9,
    };
    assert!(
        crease_circle_from_pair(c0, tilted).is_none(),
        "non-coaxial cones must decline"
    );
}

#[test]
pub(crate) fn cone_plane_perpendicular_only() {
    let (c0, _) = r0044_cones();
    let Surface::Cone {
        apex,
        axis_dir,
        half_angle,
    } = c0
    else {
        unreachable!()
    };
    let a = axis_dir.as_array();
    // A plane ⊥ the axis at station 1000 cuts a circle of radius 1000·tanα.
    let h = 1000.0;
    let on_axis = [
        apex.x() + h * a[0],
        apex.y() + h * a[1],
        apex.z() + h * a[2],
    ];
    let d = -(a[0] * on_axis[0] + a[1] * on_axis[1] + a[2] * on_axis[2]);
    let plane = Surface::Plane {
        normal: axis_dir,
        d,
    };
    let curve = crease_circle_from_pair(c0, plane).expect("perpendicular plane cuts a circle");
    let Curve::Circle { radius, .. } = curve else {
        panic!("expected a circle")
    };
    assert!(
        (radius - h * half_angle.tan()).abs() <= 1e-9 * radius,
        "circle radius must be h·tanα"
    );
    // An oblique plane cuts a conic, not a circle — decline.
    let oblique = Surface::Plane {
        normal: Vector3::new(a[0] + 0.3, a[1] + 0.4, a[2]),
        d,
    };
    assert!(
        crease_circle_from_pair(c0, oblique).is_none(),
        "an oblique plane must decline rather than approximate its conic with a circle"
    );
}

/// The pin, with R0044's own measured numbers: a step from just INSIDE the
/// face to a solution PAST the crease is a domain violation.
#[test]
pub(crate) fn material_overrun_past_the_crease_fires() {
    let (c0, c1) = r0044_cones();
    let curve = crease_circle_from_pair(c0, c1).expect("crease");
    let plane = crease_plane(&curve).expect("crease plane");
    let axis = Vector3::new(0.8962863611165686, 0.0, -0.44347577033747876);
    let a = axis.as_array();
    let Curve::Circle { center, radius, .. } = curve else {
        unreachable!()
    };
    // Two points on the crease circle's own radial ray, straddling its plane
    // by R0044's measured offsets (−0.194 inside, +0.827 past).
    let e1 = {
        // Any unit vector perpendicular to the axis.
        let seed = [0.0, 1.0, 0.0];
        let h = seed[0] * a[0] + seed[1] * a[1] + seed[2] * a[2];
        let r = [seed[0] - h * a[0], seed[1] - h * a[1], seed[2] - h * a[2]];
        let l = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        [r[0] / l, r[1] / l, r[2] / l]
    };
    let at = |off: f64| -> Point3 {
        Point3::new(
            center.x() + radius * e1[0] + off * a[0],
            center.y() + radius * e1[1] + off * a[1],
            center.z() + radius * e1[2] + off * a[2],
        )
    };
    let (pre, post) = (at(-0.194_412_1), at(0.827_486_1));
    let creases = vec![(curve, c0, c1)];
    let hit = crease_crossed_by_step(pre, post, &creases);
    assert!(
        hit.is_some(),
        "a step from inside the face to 0.827 past its own crease must be a domain violation"
    );
    let (idx, fp, fq) = hit.expect("just asserted");
    assert_eq!(idx, 0);
    assert!(
        fp < 0.0 && fq > 0.0,
        "the residuals must straddle the crease plane (got {fp}, {fq})"
    );
    // Same step, but staying on the inside: no violation.
    assert!(
        crease_crossed_by_step(at(-0.9), at(-0.1), &creases).is_none(),
        "a step that stays inside the face is not a crossing"
    );
    // Sanity: the plane the certificate reads is the crease's own.
    assert!(matches!(plane, Surface::Plane { .. }));
}

/// The population the certificate must never confuse with a violation: a
/// relocation whose residuals against the crease plane are evaluation noise
/// at both ends. R0044 carries five; with the plane's own band alone they all
/// read as crossings, and the PROPAGATED band (plus both parent surfaces')
/// exempts every one while leaving a 0.3 overrun firing.
#[test]
pub(crate) fn noise_scale_residuals_are_not_a_crossing() {
    let (c0, c1) = r0044_cones();
    let curve = crease_circle_from_pair(c0, c1).expect("crease");
    let axis = Vector3::new(0.8962863611165686, 0.0, -0.44347577033747876);
    let a = axis.as_array();
    let Curve::Circle { center, radius, .. } = curve else {
        unreachable!()
    };
    let e1 = [0.0, 1.0, 0.0];
    let at = |off: f64| -> Point3 {
        Point3::new(
            center.x() + radius * e1[0] + off * a[0],
            center.y() + radius * e1[1] + off * a[1],
            center.z() + radius * e1[2] + off * a[2],
        )
    };
    let creases = vec![(curve, c0, c1)];
    // R0044's own measured noise magnitudes, straddling zero.
    for (lo, hi) in [
        (-2.955_858e-11, 1.446_097e-10),
        (1.614_353e-11, -1.591_616e-11),
        (-2.091_838e-11, 3.728_928e-11),
    ] {
        assert!(
            crease_crossed_by_step(at(lo), at(hi), &creases).is_none(),
            "residuals at evaluation-noise scale ({lo:e}, {hi:e}) are not a crossing"
        );
    }
    // …while the smallest MATERIAL overrun R0044 carries still fires.
    assert!(
        crease_crossed_by_step(at(4.989_373), at(-3.092_587e-1), &creases).is_some(),
        "a 0.31 overrun is material and must fire"
    );
}

#[test]
pub(crate) fn a_vertex_riding_the_crease_is_exempt() {
    let (c0, c1) = r0044_cones();
    let curve = crease_circle_from_pair(c0, c1).expect("crease");
    let Curve::Circle { center, radius, .. } = curve else {
        unreachable!()
    };
    let e1 = [0.0, 1.0, 0.0];
    let on = Point3::new(
        center.x() + radius * e1[0],
        center.y() + radius * e1[1],
        center.z() + radius * e1[2],
    );
    assert!(
        on_crease(on, c0, c1),
        "a point on the shared rim satisfies BOTH cones and belongs to both faces"
    );
    // A point materially off the crease is not on it.
    let off = Point3::new(on.x() + 10.0, on.y(), on.z());
    assert!(!on_crease(off, c0, c1), "10 away is not on the crease");
}

/// Creases are indexed by SURFACE, not by the moving vertex's own edges: the
/// domain a relocation must not leave belongs to the FACE. R0044 v47 sits 10.5
/// from the crease it overruns, so a vertex-incident sourcing finds nothing.
#[test]
pub(crate) fn creases_are_sourced_by_surface_not_by_incidence_at_the_vertex() {
    let (c0, c1) = r0044_cones();
    let mut inc: std::collections::BTreeMap<(u32, u32), Vec<(InputId, Surface)>> =
        std::collections::BTreeMap::new();
    // One operand-own rim edge, far from the vertex we will query.
    inc.insert((10, 11), vec![(InputId::B, c0), (InputId::B, c1)]);
    // A cross-input edge is NOT a crease.
    inc.insert((12, 13), vec![(InputId::A, c0), (InputId::B, c1)]);
    // Same-surface edge is patch-interior, not a crease.
    inc.insert((14, 15), vec![(InputId::B, c0), (InputId::B, c0)]);
    let by_surf = creases_by_surface(&inc);
    let found = creases_for_surfaces(&by_surf, &[c0]);
    assert_eq!(
        found.len(),
        1,
        "the crease bounding c0's face must be found from the SURFACE, not from edges at a vertex"
    );
    let unrelated = Surface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 1.0,
    };
    assert!(
        creases_for_surfaces(&by_surf, &[unrelated]).is_empty(),
        "a surface carrying no operand-own rim has no crease domain bound"
    );
}

// ---------------------------------------------------------------------------
// P0040 (2026-10-12): a circle crease seen from the PLANAR face that contains
// it — the radial divider.
// ---------------------------------------------------------------------------

/// A cylinder boss's top cap: the plane `z = 1` bounded by the rim circle of
/// radius 1 about the z axis, with the lateral cylinder across the rim.
fn cap_rim_fixture() -> (Curve, Surface, Surface) {
    let rim = Curve::Circle {
        center: Point3::new(0.0, 0.0, 1.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let cap = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -1.0,
    };
    let lateral = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    (rim, cap, lateral)
}

/// The circle's plane is the cap's OWN plane: a step within the cap from
/// inside the rim to outside it has a zero residual against that plane at
/// both ends and divides nothing. The divider the cap presents is the rim's
/// cylinder, and the step fires on it.
#[test]
fn a_step_within_a_cap_plane_that_leaves_its_rim_fires_on_the_radial_divider() {
    let (rim, cap, lateral) = cap_rim_fixture();
    let creases = vec![(rim, cap, lateral)];
    let p = Point3::new(0.5, 0.0, 1.0);
    let q = Point3::new(1.5, 0.0, 1.0);
    // The divider itself: the rim's cylinder for the cap …
    assert!(matches!(
        crate::stage4_boundary_curve::crease_divider_for(&rim, cap, Some(lateral)),
        Some(Surface::Cylinder { radius, .. }) if (radius - 1.0).abs() < 1e-15
    ));
    // … and still the circle's plane for the lateral, whose surface the plane
    // cuts transversally.
    assert_eq!(
        crate::stage4_boundary_curve::crease_divider_for(&rim, lateral, Some(cap)),
        crease_plane(&rim)
    );
    let fire = crease_crossed_by_step(p, q, &creases).expect("the cap step leaves its disc");
    assert_eq!(fire.0, 0);
    assert!(
        fire.1 < 0.0 && fire.2 > 0.0,
        "inside (−0.5) to outside (+0.5): {fire:?}"
    );
    assert!((fire.1 + 0.5).abs() < 1e-12 && (fire.2 - 0.5).abs() < 1e-12);
    // A step that stays inside the disc does not fire.
    assert!(crease_crossed_by_step(p, Point3::new(-0.5, 0.3, 1.0), &creases).is_none());
    // The lateral's own reading of the same rim is unchanged: a step along the
    // cylinder past the cap plane fires on the circle's plane.
    let lat = vec![(rim, lateral, cap)];
    let fire = crease_crossed_by_step(Point3::new(1.0, 0.0, 0.5), Point3::new(1.0, 0.0, 1.5), &lat)
        .expect("the lateral step leaves the band");
    assert!(fire.1 < 0.0 && fire.2 > 0.0);
}

/// The B-Rep crease index reads the same rim the same way: the cap face's
/// crease carries the cylinder divider, and the §4.5 postcondition fires on a
/// step within the cap that leaves the disc — the P0040 v53 step, which the
/// plane divider read as a zero residual at both ends.
#[test]
fn the_brep_crease_index_gives_a_cap_its_radial_divider() {
    use crate::brep::{BRepEdge, BRepFace, BRepVertex};
    use crate::stage4_boundary_curve::{boundary_crease_crossed, CreaseIndex};
    let (rim, cap, lateral) = cap_rim_fixture();
    let bottom = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, -1.0),
        d: 0.0,
    };
    let rim0 = Curve::Circle {
        center: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, -1.0),
        radius: 1.0,
    };
    // A closed unit cylinder: two rim vertices, two full-circle edges, a cap,
    // a bottom and the lateral (which lists both rims).
    let verts = vec![
        BRepVertex {
            point: Point3::new(1.0, 0.0, 1.0),
        },
        BRepVertex {
            point: Point3::new(1.0, 0.0, 0.0),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: rim,
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: rim0,
        },
    ];
    let faces = vec![
        BRepFace {
            surface: cap,
            outer_loop: vec![0],
            inner_loops: vec![],
            reversed: false,
        },
        BRepFace {
            surface: bottom,
            outer_loop: vec![1],
            inner_loops: vec![],
            reversed: false,
        },
        BRepFace {
            surface: lateral,
            outer_loop: vec![0, 1],
            inner_loops: vec![],
            reversed: false,
        },
    ];
    let brep = crate::BRep::new(verts, edges, faces).expect("a closed unit cylinder");
    let idx = CreaseIndex::build(&brep);
    let cap_creases = &idx.faces[0];
    assert_eq!(cap_creases.len(), 1, "{cap_creases:?}");
    assert!(
        matches!(cap_creases[0].divider, Surface::Cylinder { radius, .. } if (radius - 1.0).abs() < 1e-15),
        "{:?}",
        cap_creases[0].divider
    );
    let fire = boundary_crease_crossed(
        Point3::new(0.9, 0.0, 1.0),
        Point3::new(1.1, 0.0, 1.0),
        cap_creases,
    )
    .expect("a step within the cap past its rim is a crossing");
    assert!(fire.f_pre < 0.0 && fire.f_post > 0.0, "{fire:?}");
    // The lateral face keeps the plane divider for the same rim.
    let lat_creases = &idx.faces[2];
    assert!(lat_creases
        .iter()
        .all(|c| matches!(c.divider, Surface::Plane { .. })));
}

// ---------------------------------------------------------------------------
// 2026-10-13 — the P0040 rule for EVERY planar conic, not the circle alone:
// the divider of a crease seen from the planar face that CONTAINS it is the
// crease's OTHER surface. An oblique cap's rim is an ellipse; its divider on
// the cap is the cylinder it is a section of, and its divider on the lateral
// is still its plane.
// ---------------------------------------------------------------------------

/// A unit z-cylinder capped by the oblique plane `0.3·x + z = 1`: the cap, the
/// lateral, and their crease — the ellipse the ssi-rs plane section gives.
fn oblique_cap_fixture() -> (Curve, Surface, Surface) {
    let lateral = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let l = (0.3f64 * 0.3 + 1.0).sqrt();
    let cap = Surface::Plane {
        normal: Vector3::new(0.3 / l, 0.0, 1.0 / l),
        d: -1.0 / l,
    };
    let ell = crate::stage4_boundary_curve::crease_curve_from_pair(lateral, cap)
        .expect("an oblique plane section of a cylinder is one ellipse");
    assert!(
        matches!(ell, Curve::Ellipse { major_radius, minor_radius, .. }
            if (minor_radius - 1.0).abs() < 1e-12 && (major_radius - l).abs() < 1e-12),
        "{ell:?}"
    );
    (ell, cap, lateral)
}

/// A point of the oblique cap plane at `(x, y)`.
fn on_cap(x: f64, y: f64) -> Point3 {
    Point3::new(x, y, 1.0 - 0.3 * x)
}

#[test]
fn an_ellipse_crease_in_the_oblique_cap_that_contains_it_is_divided_by_its_cylinder() {
    use crate::stage4_boundary_curve::crease_divider_for;
    let (ell, cap, lateral) = oblique_cap_fixture();
    // The divider on the cap is the crease's other surface — the cylinder.
    assert_eq!(crease_divider_for(&ell, cap, Some(lateral)), Some(lateral));
    // On the lateral, whose surface the ellipse's plane cuts transversally,
    // the divider is the ellipse's plane.
    let plane = crease_divider_for(&ell, lateral, Some(cap)).expect("the ellipse has a plane");
    assert_eq!(Some(plane), crease_plane(&ell));
    assert!(
        matches!(plane, Surface::Plane { .. }) && on_crease(on_cap(1.0, 0.0), plane, lateral),
        "{plane:?}"
    );
    // A conic alone names two cylinders; with no other surface the divider is
    // DECLINED, never the face's own plane (which cannot fire).
    assert_eq!(crease_divider_for(&ell, cap, None), None);
    // The circle keeps its own cylinder without help — a cone or torus rim
    // shares that radial divider.
    let (rim, flat_cap, _) = cap_rim_fixture();
    assert!(matches!(
        crease_divider_for(&rim, flat_cap, None),
        Some(Surface::Cylinder { radius, .. }) if (radius - 1.0).abs() < 1e-15
    ));
    // The triple arm's certificate: a step WITHIN the cap from inside the
    // ellipse to outside it fires — P0040's v53 step on an oblique cap.
    let creases = vec![(ell, cap, lateral)];
    let fire = crease_crossed_by_step(on_cap(0.5, 0.0), on_cap(1.5, 0.0), &creases)
        .expect("the cap step leaves its ellipse");
    assert_eq!(fire.0, 0);
    assert!(
        (fire.1 + 0.5).abs() < 1e-12 && (fire.2 - 0.5).abs() < 1e-12,
        "{fire:?}"
    );
    // A step that stays inside does not.
    assert!(crease_crossed_by_step(on_cap(0.5, 0.0), on_cap(-0.5, 0.3), &creases).is_none());
    // The lateral's reading: a step along the cylinder past the cap plane
    // fires on the ellipse's plane.
    let lat = vec![(ell, lateral, cap)];
    let fire = crease_crossed_by_step(Point3::new(1.0, 0.0, 0.2), Point3::new(1.0, 0.0, 1.2), &lat)
        .expect("the lateral step leaves the band");
    assert!(fire.1 < 0.0 && fire.2 > 0.0, "{fire:?}");
}

#[test]
fn crease_curve_from_pair_knows_the_sections_the_circle_constructor_did_not() {
    use crate::stage4_boundary_curve::{crease_circle_from_pair, crease_curve_from_pair};
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    // A perpendicular plane: the circle, byte-identical to the rim constructor.
    let flat = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -3.0,
    };
    assert_eq!(
        crease_curve_from_pair(cyl, flat),
        crease_circle_from_pair(cyl, flat)
    );
    assert!(matches!(
        crease_curve_from_pair(cyl, flat),
        Some(Curve::Circle { .. })
    ));
    // An oblique plane: the ellipse the circle constructor declined.
    let l = (0.3f64 * 0.3 + 1.0).sqrt();
    let oblique = Surface::Plane {
        normal: Vector3::new(0.3 / l, 0.0, 1.0 / l),
        d: -1.0 / l,
    };
    assert_eq!(crease_circle_from_pair(cyl, oblique), None);
    assert!(matches!(
        crease_curve_from_pair(cyl, oblique),
        Some(Curve::Ellipse { .. })
    ));
    assert_eq!(
        crease_curve_from_pair(cyl, oblique),
        crease_curve_from_pair(oblique, cyl),
        "pair order is immaterial"
    );
    // A plane parallel to the axis meets the cylinder in two LINES: not a
    // planar conic crease — the B-Rep edge index carries line creases.
    let axial = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -1.0,
    };
    assert_eq!(crease_curve_from_pair(cyl, axial), None);
    // Two planes, cylinder × cylinder: no planar conic from this constructor.
    assert_eq!(crease_curve_from_pair(flat, oblique), None);
    let cyl2 = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(1.0, 0.0, 0.0),
        radius: 1.0,
    };
    assert_eq!(crease_curve_from_pair(cyl, cyl2), None);
    // A cone cut obliquely (below its half-angle): an ellipse.
    let cone = Surface::Cone {
        apex: Point3::new(0.0, 0.0, 5.0),
        axis_dir: Vector3::new(0.0, 0.0, -1.0),
        half_angle: 0.5,
    };
    assert!(
        matches!(
            crease_curve_from_pair(cone, oblique),
            Some(Curve::Ellipse { .. })
        ),
        "{:?}",
        crease_curve_from_pair(cone, oblique)
    );
}

/// The B-Rep crease index reads an oblique cap the same way on a REAL boolean
/// output: a tilted cylinder intersected with a box leaves the box's top plane
/// bounded by the ellipse the cylinder cuts out of it. That face's crease
/// carries the CYLINDER divider, and the §4.5 postcondition fires on a step
/// within the cap that leaves the ellipse.
#[test]
fn the_brep_crease_index_gives_an_oblique_cap_its_cylinder_divider() {
    use crate::stage4_boundary_curve::{boundary_crease_crossed, CreaseExtent, CreaseIndex};
    use crate::tests_unit::n2_junction::{rj_box, rj_cylinder};
    let nb = crate::native_backend().expect("native backend");
    // A unit cylinder whose axis is tilted by 0.3 toward +x, running from well
    // below to well above the box's top at z = 1.
    let l = (0.3f64 * 0.3 + 1.0).sqrt();
    let axis = [0.3 / l, 0.0, 1.0 / l];
    let cyl = rj_cylinder([-0.3 * 3.0 / l, 0.0, -3.0 / l], axis, 1.0, 8.0);
    let box_ = rj_box([-4.0, -4.0, -2.0], [4.0, 4.0, 1.0]);
    let out = crate::boolean(&cyl, &box_, cad_primitives::BoolOp::Intersect, &nb)
        .expect("tilted cylinder ∩ box");
    let lateral = *cyl
        .faces()
        .iter()
        .map(|f| &f.surface)
        .find(|s| matches!(s, Surface::Cylinder { .. }))
        .expect("the cylinder's lateral");
    // The output's top: the box plane z = 1 bounded by one ellipse.
    let (ti, top) = out
        .faces()
        .iter()
        .enumerate()
        .find(|(_, f)| {
            matches!(f.surface, Surface::Plane { normal, d }
                if normal.z().abs() > 0.999 && (d.abs() - 1.0).abs() < 1e-9)
        })
        .expect("the box top survives as the oblique cap");
    let edges = out.edges();
    assert!(
        top.outer_loop
            .iter()
            .all(|&e| matches!(edges[e as usize].curve, Curve::Ellipse { .. })),
        "the cap is bounded by the ellipse section: {:?}",
        top.outer_loop
            .iter()
            .map(|&e| edges[e as usize].curve)
            .collect::<Vec<_>>()
    );
    let idx = CreaseIndex::build(&out);
    let cap_creases = &idx.faces[ti];
    assert!(!cap_creases.is_empty(), "{cap_creases:?}");
    for c in cap_creases {
        assert_eq!(
            c.divider, lateral,
            "the cap's divider is the cylinder: {c:?}"
        );
        assert!(matches!(c.extent, CreaseExtent::Conic { .. }), "{c:?}");
    }
    // A step WITHIN the cap plane from inside the ellipse to outside it, along
    // +x where the ellipse's major axis reaches `l` from the centre.
    let ell = edges[top.outer_loop[0] as usize].curve;
    let Curve::Ellipse { center, .. } = ell else {
        unreachable!()
    };
    let at = |dx: f64| Point3::new(center.x() + dx, center.y(), 1.0);
    let fire = boundary_crease_crossed(at(0.9 * l), at(1.1 * l), cap_creases)
        .expect("a step within the cap past its ellipse is a crossing");
    assert!(fire.f_pre < 0.0 && fire.f_post > 0.0, "{fire:?}");
    assert!(boundary_crease_crossed(at(0.9 * l), at(0.5 * l), cap_creases).is_none());
    // The cylinder's own face keeps the plane divider for the same ellipse.
    let lat_creases = idx
        .faces
        .iter()
        .zip(out.faces())
        .filter(|(_, f)| f.surface == lateral)
        .flat_map(|(c, _)| c.iter())
        .filter(|c| matches!(c.extent, CreaseExtent::Conic { .. }))
        .collect::<Vec<_>>();
    assert!(!lat_creases.is_empty());
    assert!(lat_creases
        .iter()
        .all(|c| matches!(c.divider, Surface::Plane { .. })));
}
