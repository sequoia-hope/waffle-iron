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

// ---------------------------------------------------------------------------
// inc-2c-3b-12b — the REPAIR: truncate → transit → q-points
// ---------------------------------------------------------------------------

/// R0044's anatomy, built EXACTLY rather than transcribed (the fixture note on
/// [`r0044_cones`] records why a rounded transcription is worse than useless
/// here: it destroys the coaxiality the crease construction depends on).
///
/// Two coaxial cones about `+z` sharing a crease circle, plus the two other
/// surfaces of the relocated vertex's triple, chosen as axis-parallel planes so
/// every quantity in the test has a closed form:
///
/// * cone A (`s_own`), apex at the origin, `tan α₀ = 1` — its face is the band
///   `z < 100`, bounded by the crease;
/// * cone B (`s_nbr`), apex at `z = 50`, `tan α₁ = 2`;
/// * their crease: `z·tanα₀ = (z−δ)·tanα₁` ⇒ `z_c = 2δ = 100`, `r = 100`;
/// * the other two surfaces: `x = 66` and `y = 88`, so `ρ = 110` EXACTLY
///   (66² + 88² = 110²).
///
/// The triple therefore has a closed-form solution on each cone:
/// `X = (66, 88, 110)` on cone A — 10 PAST its own crease, the defect — and
/// `J = (66, 88, 105)` on cone B, in the neighbour's band. That 3-4-5 scaling
/// is what makes every expected value below exact instead of a transcription.
fn transit_fixture() -> (Surface, Surface, Surface, Surface) {
    let z = Vector3::new(0.0, 0.0, 1.0);
    let cone_a = Surface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: z,
        half_angle: 1.0_f64.atan(),
    };
    let cone_b = Surface::Cone {
        apex: Point3::new(0.0, 0.0, 50.0),
        axis_dir: z,
        half_angle: 2.0_f64.atan(),
    };
    let plane_x = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -66.0,
    };
    let plane_y = Surface::Plane {
        normal: Vector3::new(0.0, 1.0, 0.0),
        d: -88.0,
    };
    (cone_a, cone_b, plane_x, plane_y)
}

/// THE REPAIR, end to end: a step that overruns its own crease is truncated to
/// `C_b`, re-solved on the neighbouring surface, and the two `q`-points are
/// solved on `C_b` — Yang §4.5.1's four steps, in its order.
#[test]
fn an_out_of_domain_step_transits_onto_the_neighbour() {
    let (cone_a, cone_b, plane_x, plane_y) = transit_fixture();
    let c_b = crease_circle_from_pair(cone_a, cone_b).expect("coaxial cones share a circle");
    // The defect: the exact triple solution on cone A's EXTENDED surface.
    let x_bad = Point3::new(66.0, 88.0, 110.0);
    // The seed, inside cone A's own band (z < 100).
    let seed = Point3::new(60.0, 80.0, 99.0);

    let t = crate::stage4_boundary_curve::solve_crease_transit(
        seed,
        x_bad,
        &[plane_x, plane_y, cone_a],
        &(c_b, cone_a, cone_b),
        &[],
    )
    .expect("a determined transit");

    // The corrected junction is the closed-form root on the NEIGHBOUR.
    let j = t.j.as_array();
    for (got, want, name) in [(j[0], 66.0, "x"), (j[1], 88.0, "y"), (j[2], 105.0, "z")] {
        assert!(
            (got - want).abs() < 1e-9,
            "corrected junction {name}: got {got}, want {want}"
        );
    }
    // …and it is a real correction, not a no-op: |X − J| = 5 exactly.
    assert!(
        (t.correction - 5.0).abs() < 1e-9,
        "correction should be 5.0, got {}",
        t.correction
    );
    // The truncation lands ON the crease circle.
    let pt = t.p_trunc.as_array();
    assert!((pt[2] - 100.0).abs() < 1e-9, "p_trunc off the crease plane");
    assert!(
        ((pt[0] * pt[0] + pt[1] * pt[1]).sqrt() - 100.0).abs() < 1e-9,
        "p_trunc off the crease circle"
    );
}

/// The `q`-points are the paper's `q1`/`q2`: they lie ON `C_b` AND on their own
/// surface. In this fixture both are closed-form — `x = 66` meets the crease at
/// `y = ±√5644`, `y = 88` meets it at `x = ±√2256` — so the test pins the
/// values, not merely the membership.
#[test]
fn the_q_points_lie_on_the_crease_and_on_their_own_surface() {
    let (cone_a, cone_b, plane_x, plane_y) = transit_fixture();
    let c_b = crease_circle_from_pair(cone_a, cone_b).expect("circle");
    let t = crate::stage4_boundary_curve::solve_crease_transit(
        Point3::new(60.0, 80.0, 99.0),
        Point3::new(66.0, 88.0, 110.0),
        &[plane_x, plane_y, cone_a],
        &(c_b, cone_a, cone_b),
        &[],
    )
    .expect("a determined transit");

    let (q1, q2) = (t.q1.as_array(), t.q2.as_array());
    // Both on the crease circle.
    for (q, name) in [(q1, "q1"), (q2, "q2")] {
        assert!((q[2] - 100.0).abs() < 1e-9, "{name} off the crease plane");
        assert!(
            ((q[0] * q[0] + q[1] * q[1]).sqrt() - 100.0).abs() < 1e-9,
            "{name} off the crease circle"
        );
    }
    // Each on its OWN surface, and the root NEAREST the junction: the junction
    // sits at y = 88 > 0 and x = 66 > 0, so both positive branches win.
    assert!(
        (q1[0] - 66.0).abs() < 1e-9,
        "q1 must lie on the plane x = 66"
    );
    assert!(
        (q1[1] - 5644.0_f64.sqrt()).abs() < 1e-9,
        "q1 y: got {}, want +√5644",
        q1[1]
    );
    assert!(
        (q2[1] - 88.0).abs() < 1e-9,
        "q2 must lie on the plane y = 88"
    );
    assert!(
        (q2[0] - 2256.0_f64.sqrt()).abs() < 1e-9,
        "q2 x: got {}, want +√2256",
        q2[0]
    );
    // The margin is how much CLOSER the winner is — the measure of whether the
    // choice was a coin flip — not the chord between the roots: two equidistant
    // roots are ambiguous however far apart they sit. Here the rejected root is
    // the mirrored branch `y = −√5644`, so with `J = (66, 88, 105)` the margin
    // has the closed form below (≈ 149.4, against a 5-unit correction).
    let sq = 5644.0_f64.sqrt();
    let near = ((88.0 - sq).powi(2) + 25.0_f64).sqrt();
    let far = ((88.0 + sq).powi(2) + 25.0_f64).sqrt();
    assert!(
        (t.q_margin[0] - (far - near)).abs() < 1e-6,
        "q1 margin: got {}, want {}",
        t.q_margin[0],
        far - near
    );
}

/// The postcondition that keeps the repair honest: if the corrected junction
/// leaves the NEIGHBOUR's domain in turn, the site is declined — a transit that
/// merely carries the overrun one face further is not a repair. Cone C is
/// constructed so its crease with cone B falls at `z = 102`, between the
/// truncation (`z = 100`) and the junction (`z = 105`).
#[test]
fn a_transit_that_leaves_the_neighbour_in_turn_is_declined() {
    let (cone_a, cone_b, plane_x, plane_y) = transit_fixture();
    let c_b = crease_circle_from_pair(cone_a, cone_b).expect("circle");
    // (102 − 50)·tanα₁ = (102 − z_C)·tanα_C with tanα_C = 4 ⇒ z_C = 76.
    let cone_c = Surface::Cone {
        apex: Point3::new(0.0, 0.0, 76.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 4.0_f64.atan(),
    };
    let c_second = crease_circle_from_pair(cone_b, cone_c).expect("circle");
    if let Curve::Circle { center, .. } = c_second {
        assert!(
            (center.as_array()[2] - 102.0).abs() < 1e-9,
            "fixture: the second crease must sit between 100 and 105"
        );
    }

    let got = crate::stage4_boundary_curve::solve_crease_transit(
        Point3::new(60.0, 80.0, 99.0),
        Point3::new(66.0, 88.0, 110.0),
        &[plane_x, plane_y, cone_a],
        &(c_b, cone_a, cone_b),
        &[(c_second, cone_b, cone_c)],
    );
    match got {
        Err(crate::stage4_boundary_curve::CreaseTransitFailure::TransitLeavesNeighbour {
            d_pre,
            d_post,
        }) => {
            // The decline REPORTS its overrun rather than merely naming itself.
            assert!(
                (d_pre < 0.0) != (d_post < 0.0),
                "the declined crossing must be a sign change, got {d_pre} / {d_post}"
            );
            assert!(
                d_pre.abs() > 1.0 && d_post.abs() > 1.0,
                "both residuals should be material, got {d_pre} / {d_post}"
            );
        }
        other => panic!("expected TransitLeavesNeighbour, got {other:?}"),
    }
}

/// A vertex already incident to the neighbour is the `on_crease` population,
/// which is exempted upstream; the solver must not silently treat it as a
/// transit.
#[test]
fn a_vertex_already_on_the_neighbour_is_an_anatomy_mismatch() {
    let (cone_a, cone_b, plane_x, _) = transit_fixture();
    let c_b = crease_circle_from_pair(cone_a, cone_b).expect("circle");
    let got = crate::stage4_boundary_curve::solve_crease_transit(
        Point3::new(60.0, 80.0, 99.0),
        Point3::new(66.0, 88.0, 110.0),
        &[plane_x, cone_b, cone_a],
        &(c_b, cone_a, cone_b),
        &[],
    );
    assert!(
        matches!(
            got,
            Err(crate::stage4_boundary_curve::CreaseTransitFailure::AnatomyMismatch)
        ),
        "expected AnatomyMismatch, got {got:?}"
    );
}

/// A step that never reaches the crease has nothing to truncate — the solver is
/// only ever called behind [`crease_crossed_by_step`], and says so rather than
/// inventing a landing.
#[test]
fn a_step_that_does_not_cross_has_no_truncation() {
    let (cone_a, cone_b, plane_x, plane_y) = transit_fixture();
    let c_b = crease_circle_from_pair(cone_a, cone_b).expect("circle");
    let got = crate::stage4_boundary_curve::solve_crease_transit(
        Point3::new(60.0, 80.0, 90.0),
        Point3::new(66.0, 88.0, 99.0), // still inside z < 100
        &[plane_x, plane_y, cone_a],
        &(c_b, cone_a, cone_b),
        &[],
    );
    assert!(
        matches!(
            got,
            Err(crate::stage4_boundary_curve::CreaseTransitFailure::NoTruncation)
        ),
        "expected NoTruncation, got {got:?}"
    );
}

// ---------------------------------------------------------------------------
// inc-2c-3b-12b-1 — the EMISSION-half site anatomy
// ---------------------------------------------------------------------------

/// The mesh the emission half would have to edit, built on the SAME exact
/// fixture as the repair solver so the two halves are pinned against one
/// geometry rather than two.
///
/// The site vertex is the defect `X = (66, 88, 110)` — 10 past cone A's own
/// crease (the circle `z = 100`, `r = 100`). Its one ring is deliberately
/// mixed, one vertex of each class the classifier has to separate:
///
/// * `v1 = (100, 0, 100)` and `v2 = (0, 100, 100)` — ON the crease (on both
///   cones exactly), and their shared mesh edge IS the crease chain locally;
/// * `v3 = (30, 40, 50)` and `v4 = (45, 60, 75)` — on cone A inside its own
///   band, i.e. HOME;
/// * `v5 = (72, 96, 120)` — on cone A but PAST the crease, the shape a
///   neighbouring site that has already been relocated leaves behind
///   (measured on R0044: v39's ring carries v38 at its recorded `d_post`).
///
/// Every vertex is exactly on its cone by construction (`3-4-5` triples
/// scaled), so no coordinate here is a transcription.
fn anatomy_fixture() -> (crate::Mesh, crate::brep::TriangleAttributionMap) {
    let mesh = crate::Mesh::new(
        vec![
            Point3::new(66.0, 88.0, 110.0), // 0: the site, past the crease
            Point3::new(100.0, 0.0, 100.0), // 1: on the crease
            Point3::new(0.0, 100.0, 100.0), // 2: on the crease
            Point3::new(30.0, 40.0, 50.0),  // 3: home
            Point3::new(45.0, 60.0, 75.0),  // 4: home
            Point3::new(72.0, 96.0, 120.0), // 5: past
        ],
        vec![[0, 1, 2], [0, 2, 3], [0, 3, 4], [0, 4, 5], [0, 5, 1]],
    );
    let attribution = crate::brep::TriangleAttributionMap {
        attributions: vec![
            Some(crate::brep::TriangleAttribution {
                input: InputId::B,
                face: 7,
            }),
            Some(crate::brep::TriangleAttribution {
                input: InputId::B,
                face: 7,
            }),
            Some(crate::brep::TriangleAttribution {
                input: InputId::A,
                face: 2,
            }),
            None,
            Some(crate::brep::TriangleAttribution {
                input: InputId::B,
                face: 7,
            }),
        ],
    };
    (mesh, attribution)
}

/// The anatomy separates the one ring into the three classes the repair has to
/// treat differently — HOME stays, ON is the crease chain to split, PAST is
/// already across — and names the face that owns the fan today.
#[test]
fn the_site_anatomy_classifies_its_one_ring_and_names_the_owning_face() {
    use crate::stage4_boundary_curve::{surface_distance_pub, transit_site_anatomy, CreaseSide};
    let (cone_a, cone_b, _, _) = transit_fixture();
    let c_b = crease_circle_from_pair(cone_a, cone_b).expect("coaxial cones share a circle");
    let plane = crease_plane(&c_b).expect("a circle has a plane");
    let (mesh, attribution) = anatomy_fixture();
    // Taken from the fixture rather than assumed, so the test does not depend
    // on which way the derived crease circle's normal happens to point.
    let d_post = surface_distance_pub(plane, mesh.verts[0]).expect("plane evaluates");

    let an = transit_site_anatomy(
        &mesh,
        &attribution,
        0,
        &(c_b, cone_a, cone_b),
        d_post,
        [mesh.verts[0], mesh.verts[0]],
    )
    .expect("the site has a fan");

    assert_eq!(an.fan.len(), 5, "every incident triangle is in the fan");
    assert_eq!(an.ring.len(), 5, "the one ring is v1..v5");
    // 2 home (v3, v4), 2 on the crease (v1, v2), 1 already past (v5).
    assert_eq!(an.sides, [2, 2, 1]);
    let class = |u: u32| an.ring.iter().find(|(x, _, _)| *x == u).expect("in ring").2;
    assert_eq!(class(1), CreaseSide::On);
    assert_eq!(class(2), CreaseSide::On);
    assert_eq!(class(3), CreaseSide::Home);
    assert_eq!(class(4), CreaseSide::Home);
    assert_eq!(class(5), CreaseSide::Past);

    // Descending count, ties by key — and `None` sorts before `Some` (a
    // triangle no attribution claimed is a distinct answer, not a missing one).
    assert_eq!(
        an.fan_faces,
        vec![
            (Some((InputId::B, 7)), 3),
            (None, 1),
            (Some((InputId::A, 2)), 1),
        ]
    );
}

/// The q-points' host is the mesh edge lying ON the crease, and the anatomy
/// carries that edge's own length and sag — the measurement that says whether
/// the repair can split what the mesh already has (R0003) or must refine it
/// first (R0044: a 558-long rim chord with the q-points 10.4 off it).
#[test]
fn the_q_host_is_the_crease_edge_and_carries_its_own_sag() {
    use crate::stage4_boundary_curve::{
        solve_crease_transit, surface_distance_pub, transit_site_anatomy,
    };
    let (cone_a, cone_b, plane_x, plane_y) = transit_fixture();
    let c_b = crease_circle_from_pair(cone_a, cone_b).expect("coaxial cones share a circle");
    let plane = crease_plane(&c_b).expect("a circle has a plane");
    let (mesh, attribution) = anatomy_fixture();
    let x_bad = mesh.verts[0];

    // The repair's own q-points, not transcribed ones: the two halves are
    // pinned against one geometry.
    let t = solve_crease_transit(
        Point3::new(60.0, 80.0, 99.0),
        x_bad,
        &[plane_x, plane_y, cone_a],
        &(c_b, cone_a, cone_b),
        &[],
    )
    .expect("the fixture step transits");

    let an = transit_site_anatomy(
        &mesh,
        &attribution,
        0,
        &(c_b, cone_a, cone_b),
        surface_distance_pub(plane, x_bad).expect("plane evaluates"),
        [t.q1, t.q2],
    )
    .expect("the site has a fan");

    let sub = |p: Point3, q: Point3| {
        let (a, b) = (p.as_array(), q.as_array());
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    };
    let ab = sub(mesh.verts[2], mesh.verts[1]);
    let ab_len = (ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2]).sqrt();

    for (i, q) in [t.q1, t.q2].iter().enumerate() {
        let h = an.q_hosts[i].expect("the crease edge hosts both q-points");
        // v1-v2 is the ONLY edge with both ends on the crease; both q-points
        // must find it, and it is inside the fan being edited.
        assert_eq!((h.a, h.b), (1, 2));
        assert!(h.in_fan);
        assert!(
            (h.len - ab_len).abs() < 1e-9,
            "host length is the edge's own"
        );
        // `dist` checked by a DIFFERENT route than the code's projection: the
        // cross-product distance from the point to the segment's line, which
        // agrees exactly when (and only when) the foot is interior.
        let aq = sub(*q, mesh.verts[1]);
        let cross = [
            aq[1] * ab[2] - aq[2] * ab[1],
            aq[2] * ab[0] - aq[0] * ab[2],
            aq[0] * ab[1] - aq[1] * ab[0],
        ];
        let d_line =
            (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt() / ab_len;
        assert!(
            (h.dist - d_line).abs() < 1e-9,
            "q{} sag {} vs cross-product {d_line}",
            i + 1,
            h.dist
        );
        // And `t` is the foot's parameter: the residual is perpendicular to
        // the edge. (Independent of the projection formula that produced it.)
        let foot = [
            mesh.verts[1].as_array()[0] + h.t * ab[0],
            mesh.verts[1].as_array()[1] + h.t * ab[1],
            mesh.verts[1].as_array()[2] + h.t * ab[2],
        ];
        let qa = q.as_array();
        let resid = [qa[0] - foot[0], qa[1] - foot[1], qa[2] - foot[2]];
        let dot = resid[0] * ab[0] + resid[1] * ab[1] + resid[2] * ab[2];
        assert!(
            dot.abs() < 1e-9,
            "foot is the perpendicular one (dot {dot})"
        );
        assert!(h.t > 0.0 && h.t < 1.0, "the foot is interior to the edge");
    }
}

/// A vertex no triangle uses has no anatomy — reported as absent, never as an
/// empty fan the caller could mistake for "measured, nothing there".
#[test]
fn a_vertex_with_no_incident_triangles_has_no_anatomy() {
    use crate::stage4_boundary_curve::transit_site_anatomy;
    let (cone_a, cone_b, _, _) = transit_fixture();
    let c_b = crease_circle_from_pair(cone_a, cone_b).expect("coaxial cones share a circle");
    let (mut mesh, attribution) = anatomy_fixture();
    mesh.verts.push(Point3::new(1.0, 2.0, 3.0));
    let orphan = (mesh.verts.len() - 1) as u32;
    assert!(transit_site_anatomy(
        &mesh,
        &attribution,
        orphan,
        &(c_b, cone_a, cone_b),
        10.0,
        [Point3::new(0.0, 0.0, 0.0); 2],
    )
    .is_none());
}
