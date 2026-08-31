//! §4.5.1 inc-2c-3b-12 (spec `specs/yang_451_corner_transit.md`) — the
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
