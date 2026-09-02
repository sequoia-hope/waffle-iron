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

// ---------------------------------------------------------------------------
// inc-2c-3b-12b-2 — the CUT PATH across the own patch
// ---------------------------------------------------------------------------

/// The measured corner, built exactly: a site across the crease whose fan
/// straddles THREE input faces, laid out with the same shape the census found
/// at R0044's v47 — two `On` interior ring vertices, one `Home` interior one,
/// and one chain end of each kind.
///
/// Ring, in the fan's own cycle order and decreasing azimuth about the crease
/// circle (`z = 100`, `r = 100`):
///
/// | ring | position | side | role |
/// |---|---|---|---|
/// | `1` | 70°, `z = 95` | Home | chain end → `q1` (the `plane_x` chain) |
/// | `2` | 68° | On | interior |
/// | `3` | 66° | On | interior |
/// | `4` | 64°, `z = 95` | Home | interior → the one `Refined` crossing |
/// | `5` | `q2` itself | On | chain end → `q2` (the `plane_y` chain) |
/// | `6` | deep inside | Home | the CARRIER's ring vertex |
///
/// Vertex `1` is placed DELIBERATELY ADVERSARIALLY: its edge's crease-plane
/// crossing is much nearer `q2` than `q1` (3.9 against 26.2), so a
/// nearest-q-point rule would assign it wrongly. Its face across is the
/// `plane_x` one, so surface IDENTITY assigns it to `q1`. That is not a
/// contrived worry — the corpus census measured a NEGATIVE margin, i.e.
/// proximity picking the wrong q, at every site where both chains were
/// crossed edges (R0044 v47 −1.85, v38 −0.052; R0003 v7611 −0.36, v8809
/// −0.78).
fn cut_fixture() -> (crate::Mesh, crate::brep::TriangleAttributionMap) {
    let on = |deg: f64| {
        let r = deg.to_radians();
        Point3::new(100.0 * r.cos(), 100.0 * r.sin(), 100.0)
    };
    let home = |deg: f64| {
        let r = deg.to_radians();
        Point3::new(95.0 * r.cos(), 95.0 * r.sin(), 95.0)
    };
    let mesh = crate::Mesh::new(
        vec![
            Point3::new(66.0, 88.0, 110.0),              // 0: the site
            home(70.0),                                  // 1: chain end (Home)
            on(68.0),                                    // 2: interior, On
            on(66.0),                                    // 3: interior, On
            home(64.0),                                  // 4: interior, Home
            Point3::new(2256.0_f64.sqrt(), 88.0, 100.0), // 5: chain end = q2
            Point3::new(18.0, 24.0, 30.0),               // 6: the carrier's
        ],
        vec![
            [0, 1, 2],
            [0, 2, 3],
            [0, 3, 4],
            [0, 4, 5],
            [0, 5, 6],
            [0, 6, 1],
        ],
    );
    let f = |input, face| Some(crate::brep::TriangleAttribution { input, face });
    let attribution = crate::brep::TriangleAttributionMap {
        attributions: vec![
            f(InputId::B, 7), // the own patch (cone A) …
            f(InputId::B, 7),
            f(InputId::B, 7),
            f(InputId::B, 7),
            f(InputId::A, 3), // … then plane_y …
            f(InputId::A, 2), // … then plane_x, with the carrier edge between
        ],
    };
    (mesh, attribution)
}

/// Resolve the fixture's attributions to their surfaces, the way the census
/// resolves them against the input BReps.
fn cut_fixture_surfaces(i: InputId, face: u32) -> Option<Surface> {
    let (cone_a, _, plane_x, plane_y) = transit_fixture();
    match (i, face) {
        (InputId::B, 7) => Some(cone_a),
        (InputId::A, 2) => Some(plane_x),
        (InputId::A, 3) => Some(plane_y),
        _ => None,
    }
}

fn cut_fixture_transit() -> (
    crate::stage4_boundary_curve::CreaseTransit,
    Curve,
    Surface,
    Surface,
) {
    let (cone_a, cone_b, plane_x, plane_y) = transit_fixture();
    let c_b = crease_circle_from_pair(cone_a, cone_b).expect("coaxial cones share a circle");
    let t = crate::stage4_boundary_curve::solve_crease_transit(
        Point3::new(60.0, 80.0, 99.0),
        Point3::new(66.0, 88.0, 110.0),
        &[plane_x, plane_y, cone_a],
        &(c_b, cone_a, cone_b),
        &[],
    )
    .expect("the fixture step transits");
    (t, c_b, cone_a, cone_b)
}

/// The cut is an ARC across the own patch from one q-termination to the other,
/// and the three chains do not play the same role: two terminate at q-points,
/// the third is the CARRIER the site glides along.
#[test]
fn the_cut_runs_q_to_q_across_the_own_patch_and_names_the_carrier() {
    use crate::stage4_boundary_curve::{
        surface_distance_pub, transit_cut_path, transit_site_anatomy, CutCrossing,
    };
    let (t, c_b, cone_a, cone_b) = cut_fixture_transit();
    let plane = crease_plane(&c_b).expect("a circle has a plane");
    let (mesh, attribution) = cut_fixture();
    let an = transit_site_anatomy(
        &mesh,
        &attribution,
        0,
        &(c_b, cone_a, cone_b),
        surface_distance_pub(plane, mesh.verts[0]).expect("plane evaluates"),
        [t.q1, t.q2],
    )
    .expect("the site has a fan");

    let cut = transit_cut_path(
        &mesh,
        &an,
        mesh.verts[0],
        &(c_b, cone_a, cone_b),
        &t,
        &cut_fixture_surfaces,
    )
    .expect("the fixture corner is the supported anatomy");

    // The carrier is the edge between the two OTHER surfaces — never split,
    // never re-terminated; `X → J` is a step along it.
    assert_eq!(cut.carrier, 6);
    // The triangle whose two other corners are both ON the crease crosses
    // wholesale; the rest of the run is split.
    assert_eq!(cut.past_tris, vec![1]);
    assert_eq!(cut.split_tris, vec![0, 2, 3]);

    assert_eq!(cut.nodes.len(), 5);
    // Chain end 1: assigned to q1 by SURFACE, against a proximity answer that
    // would have said q2 — the margin is negative and the test pins that.
    match cut.nodes[0] {
        CutCrossing::QPoint { u, q, margin, .. } => {
            assert_eq!((u, q), (1, 0));
            assert!(
                margin < 0.0,
                "the fixture is adversarial: proximity prefers the OTHER q \
                 (margin {margin})"
            );
        }
        other => panic!("expected a crossed chain end, got {other:?}"),
    }
    assert_eq!(cut.nodes[1], CutCrossing::Vertex(2));
    assert_eq!(cut.nodes[2], CutCrossing::Vertex(3));
    match cut.nodes[3] {
        CutCrossing::Refined { u, point, lift } => {
            assert_eq!(u, 4);
            // A refinement crossing is ON the crease circle by construction.
            let a = point.as_array();
            assert!((a[2] - 100.0).abs() < 1e-9);
            assert!(((a[0] * a[0] + a[1] * a[1]).sqrt() - 100.0).abs() < 1e-9);
            assert!(lift > 0.0, "the chord crossing is off the circle");
        }
        other => panic!("expected a refinement, got {other:?}"),
    }
    // Chain end 5 IS q2 already — the mesh carries this q-point as a vertex,
    // the shape R0003's v1983 / v8658 / v11356 are measured in.
    match cut.nodes[4] {
        CutCrossing::QVertex { u, q, dist } => {
            assert_eq!((u, q), (5, 1));
            assert!(dist < 1e-9, "the ring vertex IS q2 (dist {dist})");
        }
        other => panic!("expected an existing q-vertex, got {other:?}"),
    }

    // The node angles are along the crease circle, one per node. The two
    // interior `On` vertices are laid out 2° apart by construction, and the
    // difference is independent of which orthonormal basis the circle picks —
    // which is what makes it a pin rather than a transcription.
    assert_eq!(cut.thetas.len(), cut.nodes.len());
    assert!(
        ((cut.thetas[1] - cut.thetas[2]).abs() - 2.0).abs() < 1e-9,
        "ring 2 and 3 are 68° and 66° on the crease: {:?}",
        cut.thetas
    );
    assert!(cut.span > 0.0 && cut.span.is_finite());
}

/// A one-ring neighbour already across the crease is a typed DECLINE: the cut
/// would leave the fan through an edge not incident to the site. Measured as
/// R0044's v38/v39/v59 cluster and R0003's v9336 — 4 of the 11 sites.
#[test]
fn a_neighbour_already_across_the_crease_is_declined() {
    use crate::stage4_boundary_curve::{
        surface_distance_pub, transit_cut_path, transit_site_anatomy, CutPathFailure,
    };
    let (t, c_b, cone_a, cone_b) = cut_fixture_transit();
    let plane = crease_plane(&c_b).expect("a circle has a plane");
    let (mut mesh, attribution) = cut_fixture();
    // Push ring vertex 4 across the crease, on cone A: r = z = 120.
    mesh.verts[4] = Point3::new(72.0, 96.0, 120.0);
    let an = transit_site_anatomy(
        &mesh,
        &attribution,
        0,
        &(c_b, cone_a, cone_b),
        surface_distance_pub(plane, mesh.verts[0]).expect("plane evaluates"),
        [t.q1, t.q2],
    )
    .expect("the site has a fan");
    assert_eq!(
        transit_cut_path(
            &mesh,
            &an,
            mesh.verts[0],
            &(c_b, cone_a, cone_b),
            &t,
            &cut_fixture_surfaces
        ),
        Err(CutPathFailure::PastNeighbour { u: 4 })
    );
}

/// If a chain's other face is not one of the site's two OTHER surfaces, WHICH
/// q-point it terminates at is not identified — and the answer is a decline,
/// not the nearest one.
#[test]
fn a_chain_whose_surface_is_unknown_is_declined_not_guessed() {
    use crate::stage4_boundary_curve::{
        surface_distance_pub, transit_cut_path, transit_site_anatomy, CutPathFailure,
    };
    let (t, c_b, cone_a, cone_b) = cut_fixture_transit();
    let plane = crease_plane(&c_b).expect("a circle has a plane");
    let (mesh, attribution) = cut_fixture();
    let an = transit_site_anatomy(
        &mesh,
        &attribution,
        0,
        &(c_b, cone_a, cone_b),
        surface_distance_pub(plane, mesh.verts[0]).expect("plane evaluates"),
        [t.q1, t.q2],
    )
    .expect("the site has a fan");
    // The `plane_x` face no longer resolves: its chain end has no identified q.
    let blind = |i: InputId, f: u32| match (i, f) {
        (InputId::A, 2) => None,
        _ => cut_fixture_surfaces(i, f),
    };
    assert_eq!(
        transit_cut_path(
            &mesh,
            &an,
            mesh.verts[0],
            &(c_b, cone_a, cone_b),
            &t,
            &blind
        ),
        Err(CutPathFailure::QSurfaceUnmatched { u: 1 })
    );
}

// ---------------------------------------------------------------------------
// inc-2c-3b-12b-3 — the EMISSION PLAN: what the mesh must ACQUIRE
// ---------------------------------------------------------------------------

fn d3(p: Point3, q: Point3) -> f64 {
    let (a, b) = (p.as_array(), q.as_array());
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Run one fixture mesh through anatomy → cut → emission plan.
fn emission_of(
    mesh: &crate::Mesh,
    attribution: &crate::brep::TriangleAttributionMap,
) -> crate::stage4_boundary_curve::TransitEmissionPlan {
    use crate::stage4_boundary_curve::{
        surface_distance_pub, transit_cut_path, transit_emission_plan, transit_site_anatomy,
    };
    let (t, c_b, cone_a, cone_b) = cut_fixture_transit();
    let plane = crease_plane(&c_b).expect("a circle has a plane");
    let an = transit_site_anatomy(
        mesh,
        attribution,
        0,
        &(c_b, cone_a, cone_b),
        surface_distance_pub(plane, mesh.verts[0]).expect("plane evaluates"),
        [t.q1, t.q2],
    )
    .expect("the site has a fan");
    let cut = transit_cut_path(
        mesh,
        &an,
        mesh.verts[0],
        &(c_b, cone_a, cone_b),
        &t,
        &cut_fixture_surfaces,
    )
    .expect("the fixture corner is the supported anatomy");
    transit_emission_plan(mesh, &an, &cut, &t, &(c_b, cone_a, cone_b))
        .expect("the fixture cut yields a plan")
}

/// The corner's own sweep in the fixture, in degrees, from the two q-points'
/// CLOSED FORMS — `x = 66` and `y = 88` meet the `r = 100` crease circle at
/// `(66, √5644)` and `(√2256, 88)`. Derived here rather than transcribed, so
/// the assertions below compare against geometry and not against a previous
/// run's output.
fn fixture_corner_deg() -> f64 {
    let dot = 66.0 * 2256.0_f64.sqrt() + 88.0 * 5644.0_f64.sqrt();
    (dot / 10_000.0).acos().to_degrees()
}

/// The two acquisitions are INDEPENDENT: the base fixture already carries `q2`
/// as a chain vertex, yet its crease has no local mesh chain reaching either
/// q-point, so the chain side is satisfied and the crease side is not.
///
/// This is the measured majority shape — R0044 v38 and R0003 v7611/v8809 are
/// `NoChain` on both sides.
#[test]
fn a_q_point_on_the_chain_can_still_be_absent_from_the_crease() {
    use crate::stage4_boundary_curve::{CreaseAcquire, QAcquire};
    let (mesh, attribution) = cut_fixture();
    let pl = emission_of(&mesh, &attribution);

    // Chain side: ring 1 is a crossed chain end, ring 5 IS q2 already.
    match pl.q_acquire[0] {
        QAcquire::SplitChain { u, lift } => {
            assert_eq!(u, 1);
            assert!(lift > 0.0, "the chord crossing is not the q-point");
        }
        other => panic!("expected a chain split for q1, got {other:?}"),
    }
    match pl.q_acquire[1] {
        QAcquire::AtVertex { u, dist } => {
            assert_eq!(u, 5);
            assert!(dist < 1e-9, "ring 5 IS q2 (dist {dist})");
        }
        other => panic!("expected an existing q-vertex for q2, got {other:?}"),
    }
    // Crease side: the fan's one crease edge spans 66°–68°, nowhere near the
    // corner, so NEITHER q-point can be acquired by splitting it.
    assert_eq!(
        pl.crease_acquire,
        [CreaseAcquire::NoChain, CreaseAcquire::NoChain]
    );
    assert_eq!(pl.fan_crease_edges.len(), 1);
    assert_eq!((pl.fan_crease_edges[0].0, pl.fan_crease_edges[0].1), (2, 3));
    assert!(
        (pl.fan_crease_edges[0].3 - pl.fan_crease_edges[0].2 - 2.0).abs() < 1e-9,
        "the crease edge spans the fixture's 68°–66°: {:?}",
        pl.fan_crease_edges
    );
    assert_eq!(pl.chain_overlap, None);
    assert!(pl.corner_clear);

    // The corner and the arc's sagitta are closed-form on this circle.
    let corner = fixture_corner_deg();
    assert!(
        (pl.corner_deg - corner).abs() < 1e-9,
        "corner {} vs closed form {corner}",
        pl.corner_deg
    );
    let sag = 100.0 * (1.0 - (corner.to_radians() / 2.0).cos());
    assert!(
        (pl.arc_sag - sag).abs() < 1e-9,
        "sag {} vs {sag}",
        pl.arc_sag
    );
    // The fan reaches well past the corner it has to carry — the over-reach
    // the census measures as 22× … 1508× on the corpus.
    assert!(pl.fan_span_deg > pl.corner_deg);
}

/// Move the fan's two on-crease ring vertices so their shared edge BRACKETS the
/// corner: both q-points then have to be inserted INTO that edge, which is
/// shared with the neighbouring face. R0044's v47 shape — both q's interior to
/// one 558.5-long rim chord, 10.39 and 10.36 off it.
#[test]
fn a_crease_edge_that_brackets_the_corner_takes_an_interior_insert() {
    use crate::stage4_boundary_curve::CreaseAcquire;
    let on = |deg: f64| {
        let r: f64 = f64::to_radians(deg);
        Point3::new(100.0 * r.cos(), 100.0 * r.sin(), 100.0)
    };
    let (mut mesh, attribution) = cut_fixture();
    // 70° and 40° bracket both q-points (48.7° and 61.7° in the xy frame).
    mesh.verts[2] = on(70.0);
    mesh.verts[3] = on(40.0);
    let pl = emission_of(&mesh, &attribution);

    assert_eq!(pl.fan_crease_edges.len(), 1);
    for (i, acq) in pl.crease_acquire.iter().enumerate() {
        match *acq {
            CreaseAcquire::Interior {
                a,
                b,
                t,
                off_chord,
                len,
            } => {
                assert_eq!((a, b), (2, 3));
                assert!((0.0..=1.0).contains(&t), "q{i} parameter {t} off the edge");
                // The q-point is EXACT on the circle and the edge is a chord,
                // so the displacement is the chord's own sag — bounded by the
                // sagitta of its 30° span and strictly positive.
                let chord_sag = 100.0 * (1.0 - f64::to_radians(15.0).cos());
                assert!(
                    off_chord > 0.0 && off_chord < chord_sag,
                    "q{i} off_chord {off_chord} outside (0, {chord_sag})"
                );
                let want = d3(mesh.verts[2], mesh.verts[3]);
                assert!((len - want).abs() < 1e-9, "edge length {len} vs {want}");
            }
            other => panic!("q{i} should need an interior insert, got {other:?}"),
        }
    }
    assert_eq!(pl.chain_overlap, None);
    assert!(pl.corner_clear);
}

/// The measured shape at R0003's v1983 / v8658 / v11356: the mesh already has
/// BOTH q-points as crease vertices, and two of the fan's crease edges each
/// run from one of them PAST the other — so the chain covers the corner arc
/// TWICE.
///
/// That is not a resolution shortfall. The overlap of the two edges is exactly
/// the corner (up to each q-vertex's own offset from the analytic q-point),
/// which is what makes the repair there a re-ordering rather than a
/// refinement. This test pins that identity on a fixture where both q-points
/// are placed at their closed-form positions.
#[test]
fn two_crease_edges_covering_the_corner_twice_are_the_corner() {
    use crate::stage4_boundary_curve::{CreaseAcquire, QAcquire};
    let on = |deg: f64| {
        let r: f64 = f64::to_radians(deg);
        Point3::new(100.0 * r.cos(), 100.0 * r.sin(), 100.0)
    };
    let home = |deg: f64| {
        let r: f64 = f64::to_radians(deg);
        Point3::new(95.0 * r.cos(), 95.0 * r.sin(), 95.0)
    };
    let (t, _, _, _) = cut_fixture_transit();
    let (mut mesh, attribution) = cut_fixture();
    // Ring 1 becomes q1 itself; ring 2 runs PAST q2, ring 4 PAST q1, so the
    // two crease edges (1,2) and (4,5) overlap over exactly [q1, q2].
    mesh.verts[1] = t.q1;
    mesh.verts[2] = on(70.0);
    mesh.verts[3] = home(64.0);
    mesh.verts[4] = on(40.0);
    let pl = emission_of(&mesh, &attribution);

    assert!(matches!(pl.q_acquire[0], QAcquire::AtVertex { u: 1, .. }));
    assert!(matches!(pl.q_acquire[1], QAcquire::AtVertex { u: 5, .. }));
    assert_eq!(
        pl.crease_acquire,
        [
            CreaseAcquire::AtEnd { u: 1, dist: 0.0 },
            CreaseAcquire::AtEnd {
                u: 5,
                dist: match pl.q_acquire[1] {
                    QAcquire::AtVertex { dist, .. } => dist,
                    _ => unreachable!(),
                }
            }
        ]
    );

    // THE identity: the doubled cover IS the corner.
    match pl.chain_overlap {
        Some(ov) => {
            assert_eq!((ov.a, ov.b), ((1, 2), (4, 5)));
            assert!(
                (ov.deg - pl.corner_deg).abs() < 1e-9,
                "overlap {} should be the corner {}",
                ov.deg,
                pl.corner_deg
            );
        }
        None => panic!("the two crease edges cover the corner twice"),
    }
    assert!(
        pl.corner_clear,
        "the doubled cover is not a swallowed vertex"
    );
}

/// A q-vertex whose distance to the analytic q-point EXCEEDS the contract band
/// is still the q-point: identity comes from the termination the cut already
/// resolved, never from re-measuring a distance.
///
/// Measured: R0003 v1983's `q1` vertex sits 1.7e-12 from the solved point
/// against a ~1.1e-12 band. A band test there reported the corner as unclear
/// and demanded an interior insert on a chain edge that already ended at the
/// point — for a vertex the mesh demonstrably carries. Here the offset is
/// pushed to 5e-12, five times the band, ALONG the crease and toward `q2`, so
/// the vertex also lands strictly inside its own corner interval.
#[test]
fn a_q_vertex_outside_the_band_is_still_the_q_point() {
    use crate::stage4_boundary_curve::{CreaseAcquire, QAcquire};
    let on = |deg: f64| {
        let r: f64 = f64::to_radians(deg);
        Point3::new(100.0 * r.cos(), 100.0 * r.sin(), 100.0)
    };
    let home = |deg: f64| {
        let r: f64 = f64::to_radians(deg);
        Point3::new(95.0 * r.cos(), 95.0 * r.sin(), 95.0)
    };
    let (t, c_b, cone_a, _) = cut_fixture_transit();
    let band = crate::stage4_relocate::junction_certificate_band(t.q1.as_array(), cone_a);
    let (mut mesh, attribution) = cut_fixture();
    // Slide q1 along the crease tangent, toward q2, by 5× the contract band.
    let a = t.q1.as_array();
    let step = 5.0 * band;
    let nudged = Point3::new(a[0] - step * a[1] / 100.0, a[1] + step * a[0] / 100.0, a[2]);
    let moved = d3(nudged, t.q1);
    assert!(
        moved > band,
        "the fixture must place the vertex OUTSIDE the band ({moved} vs {band})"
    );
    mesh.verts[1] = nudged;
    mesh.verts[2] = on(70.0);
    mesh.verts[3] = home(64.0);
    mesh.verts[4] = on(40.0);
    let pl = emission_of(&mesh, &attribution);

    // The chain side names it, so the crease side must agree — not re-derive.
    assert!(matches!(pl.q_acquire[0], QAcquire::AtVertex { u: 1, .. }));
    assert!(
        matches!(pl.crease_acquire[0], CreaseAcquire::AtEnd { u: 1, .. }),
        "a band test would demand an insert here: {:?}",
        pl.crease_acquire[0]
    );
    // …and the vertex does not count as a chain vertex swallowed by its own
    // corner, even though its angle now lies strictly inside it.
    assert!(pl.corner_clear, "q1 is not a vertex INSIDE the corner");
    let _ = c_b;
}

// ---------------------------------------------------------------------------
// inc-2c-3b-12b-4 — the EMISSION EDIT LIST
// ---------------------------------------------------------------------------

/// Run one fixture mesh through anatomy → cut → plan → EDITS.
///
/// Mirrors [`emission_of`] one stage further, and returns the `Result` so the
/// structural declines can be asserted as themselves rather than unwrapped.
fn edits_of(
    mesh: &crate::Mesh,
    attribution: &crate::brep::TriangleAttributionMap,
) -> Result<
    crate::stage4_boundary_curve::TransitEmissionEdits,
    crate::stage4_boundary_curve::EmissionEditFailure,
> {
    use crate::stage4_boundary_curve::{
        surface_distance_pub, transit_cut_path, transit_emission_edits, transit_emission_plan,
        transit_site_anatomy,
    };
    let (t, c_b, cone_a, cone_b) = cut_fixture_transit();
    let plane = crease_plane(&c_b).expect("a circle has a plane");
    let an = transit_site_anatomy(
        mesh,
        attribution,
        0,
        &(c_b, cone_a, cone_b),
        surface_distance_pub(plane, mesh.verts[0]).expect("plane evaluates"),
        [t.q1, t.q2],
    )
    .expect("the site has a fan");
    let cut = transit_cut_path(
        mesh,
        &an,
        mesh.verts[0],
        &(c_b, cone_a, cone_b),
        &t,
        &cut_fixture_surfaces,
    )
    .expect("the fixture corner is the supported anatomy");
    let pl = transit_emission_plan(mesh, &an, &cut, &t, &(c_b, cone_a, cone_b))
        .expect("the fixture cut yields a plan");
    transit_emission_edits(mesh, &an, &cut, &pl, &t)
}

/// R0044 v47's shape: both q-points interior to ONE crease chord, and both
/// chain ends crossed chords rather than existing vertices.
///
/// `across` adds the triangle on the FAR side of that chord — the neighbouring
/// patch's — so the fixture carries the reach the repair actually has. Without
/// it the chord is a mesh boundary and the reach is empty, which is a
/// different (and easier) mesh than the corpus one.
fn interior_insert_fixture(
    across: bool,
    reversed: bool,
) -> (crate::Mesh, crate::brep::TriangleAttributionMap) {
    let on = |deg: f64| {
        let r: f64 = f64::to_radians(deg);
        Point3::new(100.0 * r.cos(), 100.0 * r.sin(), 100.0)
    };
    let home = |deg: f64| {
        let r: f64 = f64::to_radians(deg);
        Point3::new(95.0 * r.cos(), 95.0 * r.sin(), 95.0)
    };
    let (mut mesh, mut attribution) = cut_fixture();
    // 70° and 40° bracket both q-points (48.7° and 61.7° in the xy frame);
    // `reversed` names the SAME chord from the other end.
    let (lo, hi) = if reversed { (40.0, 70.0) } else { (70.0, 40.0) };
    mesh.verts[2] = on(lo);
    mesh.verts[3] = on(hi);
    // Ring 5 sits exactly ON q2 in the base fixture. Pull it back to the home
    // side, along its own ray, so the plane_y chain is a CROSSED chord at both
    // ends — R0044 v47's `Split`/`Split` chain side.
    mesh.verts[5] = home(f64::atan2(88.0, 2256.0_f64.sqrt()).to_degrees());
    if across {
        // A vertex past the crease, and the neighbouring patch's triangle on
        // the shared chord. It touches no fan triangle, so anatomy, cut and
        // plan are all unchanged — only the reach is.
        mesh.verts.push(Point3::new(0.0, 0.0, 130.0));
        let w = (mesh.verts.len() - 1) as u32;
        mesh.tris.push([2, w, 3]);
        attribution
            .attributions
            .push(Some(crate::brep::TriangleAttribution {
                input: InputId::B,
                face: 8,
            }));
    }
    (mesh, attribution)
}

/// The determined shape yields a determined EDIT LIST: two mints, one shared
/// chord, and a reach that leaves the fan.
///
/// The measured corpus instance is R0044 v47 — host `(981, 6911)` carried by
/// tris `13112` (own patch, input face `(B, 168)`) and `13037` (the
/// neighbour's, `(B, 167)`), 7 triangles touched of which exactly one lies
/// outside the fan. That single outside triangle is the whole point: a crease
/// chord is shared, so refining it is not a fan-local act.
#[test]
fn the_edit_list_names_two_mints_one_chord_and_the_reach_outside_the_fan() {
    let (mesh, attribution) = interior_insert_fixture(true, false);
    let ed = edits_of(&mesh, &attribution).expect("the bracketed corner is an insertion");

    // The site is DERIVED from the fan, never passed.
    assert_eq!(ed.site, 0);
    // One chord, carried by the own patch's triangle and the neighbour's.
    assert_eq!(ed.crease_host, (2, 3));
    assert_eq!(
        ed.crease_tris,
        vec![1, 6],
        "tri 1 = [0,2,3], tri 6 = [2,w,3]"
    );
    // Both chain edges are interior to the fan: two triangles each, both
    // incident to the site.
    for (i, tris) in ed.chain_tris.iter().enumerate() {
        assert_eq!(tris.len(), 2, "chain {i} is interior: {tris:?}");
        for &x in tris {
            assert!(
                mesh.tris[x as usize].contains(&0),
                "chain triangle {x} should be in the fan"
            );
        }
    }
    // THE measurement: the reach outside the fan is exactly the neighbour's
    // triangle on the shared chord.
    assert_eq!(ed.outside_fan, vec![6]);
    assert!(
        ed.touched.contains(&6) && ed.touched.contains(&1),
        "both sides of the chord are re-triangulated: {:?}",
        ed.touched
    );
    // Every touched triangle is a real one, listed once, in order.
    assert!(ed.touched.windows(2).all(|w| w[0] < w[1]));
    assert!(ed.touched.iter().all(|&x| (x as usize) < mesh.tris.len()));

    // The mints are the EXACT q-points, and each carries both its roles.
    let (t, _, _, _) = cut_fixture_transit();
    let qs = [t.q1, t.q2];
    for ins in &ed.inserts {
        assert!(
            d3(ins.at, qs[ins.q]) == 0.0,
            "the mint IS the solved q-point"
        );
        assert_eq!(ins.crease, (2, 3));
        assert_eq!(ins.chain.0, 0, "the chain edge leaves the site");
        assert!(
            ins.chain_lift > 0.0,
            "the chord's own crossing is not the q-point"
        );
        assert!(ins.crease_off > 0.0, "the chord sags off the exact q");
    }
    // q1 is on plane_x (x = 66) and q2 on plane_y (y = 88), so their chains
    // are the fixture's two non-carrier chain edges.
    let by_q = |q: usize| ed.inserts.iter().find(|i| i.q == q).expect("both q's");
    assert_eq!(by_q(0).chain, (0, 1));
    assert_eq!(by_q(1).chain, (0, 5));
}

/// The insert order follows the CHORD, not the solver's q numbering.
///
/// Both mints go into one chord, so the refined chain has to connect them in
/// the order they occur along it; taking the q numbering instead would invert
/// the notch wherever the solver happened to number them the other way. It
/// does: at R0044 v47 the chord runs `981 → 6911` and `q2` (t = 0.42396)
/// precedes `q1` (t = 0.42805). Naming the SAME chord from its other end must
/// reverse the order and complement every parameter.
#[test]
fn the_insert_order_follows_the_chord_not_the_q_numbering() {
    let (mesh, attribution) = interior_insert_fixture(true, false);
    let fwd = edits_of(&mesh, &attribution).expect("insertion");
    let (rmesh, rattribution) = interior_insert_fixture(true, true);
    let rev = edits_of(&rmesh, &rattribution).expect("insertion");
    // Ordered along the chord in both, and the parameters increase.
    for ed in [&fwd, &rev] {
        assert!(
            ed.inserts[0].crease_t <= ed.inserts[1].crease_t,
            "inserts run along the chord: {:?}",
            ed.inserts.map(|i| i.crease_t)
        );
    }
    // The chord's two namings put the q's in OPPOSITE orders …
    assert_eq!(
        (fwd.inserts[0].q, fwd.inserts[1].q),
        (rev.inserts[1].q, rev.inserts[0].q),
        "reversing the chord reverses the insert order"
    );
    // … and this fixture is one where they differ from the q numbering, so the
    // distinction is actually exercised.
    assert_eq!((fwd.inserts[0].q, fwd.inserts[1].q), (1, 0));
    // Same points, complementary parameters.
    for ins in &fwd.inserts {
        let other = rev.inserts.iter().find(|i| i.q == ins.q).expect("same q's");
        assert!(
            (ins.crease_t + other.crease_t - 1.0).abs() < 1e-12,
            "q{} at {} and {} should complement",
            ins.q,
            ins.crease_t,
            other.crease_t
        );
        assert!(
            (ins.crease_off - other.crease_off).abs() < 1e-9,
            "the off-chord distance does not depend on the chord's naming"
        );
    }
    // The per-q chain edge travels with the q, not with the slot.
    let fq0 = fwd.inserts.iter().find(|i| i.q == 0).expect("q0");
    let rq0 = rev.inserts.iter().find(|i| i.q == 0).expect("q0");
    assert_eq!(fq0.chain, rq0.chain);
    // The chain TRIANGLES are ordered with the inserts, so slot i's triangles
    // carry slot i's edge — the pairing the mutation indexes by.
    for (ed, m) in [(&fwd, &mesh), (&rev, &rmesh)] {
        for (i, tris) in ed.chain_tris.iter().enumerate() {
            let (a, b) = ed.inserts[i].chain;
            for &x in tris {
                let tri = m.tris[x as usize];
                assert!(
                    tri.contains(&a) && tri.contains(&b),
                    "chain_tris[{i}] must carry inserts[{i}].chain"
                );
            }
        }
    }
}

/// Without the neighbour's triangle the chord is a mesh BOUNDARY, and the
/// reach is empty — measured, not assumed. The edit list still stands: a
/// one-sided chord is a legitimate mesh, and it is the two-sided one that
/// costs the extra split.
#[test]
fn a_chord_no_neighbour_carries_has_no_reach_outside_the_fan() {
    let (mesh, attribution) = interior_insert_fixture(false, false);
    let ed = edits_of(&mesh, &attribution).expect("insertion");
    assert_eq!(ed.crease_tris, vec![1], "only the own patch carries it");
    assert!(ed.outside_fan.is_empty());
    assert!(ed
        .touched
        .iter()
        .all(|&x| mesh.tris[x as usize].contains(&0)));
}

/// The three `AtEnd` sites are a RE-ORDERING, and the edit list says so rather
/// than manufacturing inserts for a corner the mesh already carries.
///
/// The decline carries the doubled cover's own sweep, which §3x measured to be
/// exactly the corner — so the caller gets the reason and its magnitude, not a
/// bare refusal.
#[test]
fn a_chain_that_already_carries_the_corner_declines_as_a_reordering() {
    use crate::stage4_boundary_curve::EmissionEditFailure;
    let on = |deg: f64| {
        let r: f64 = f64::to_radians(deg);
        Point3::new(100.0 * r.cos(), 100.0 * r.sin(), 100.0)
    };
    let home = |deg: f64| {
        let r: f64 = f64::to_radians(deg);
        Point3::new(95.0 * r.cos(), 95.0 * r.sin(), 95.0)
    };
    let (t, _, _, _) = cut_fixture_transit();
    let (mut mesh, attribution) = cut_fixture();
    mesh.verts[1] = t.q1;
    mesh.verts[2] = on(70.0);
    mesh.verts[3] = home(64.0);
    mesh.verts[4] = on(40.0);
    let pl = emission_of(&mesh, &attribution);
    match edits_of(&mesh, &attribution) {
        Err(EmissionEditFailure::AlreadyCarried { overlap_deg }) => {
            let ov = overlap_deg.expect("the doubled cover is measured");
            assert!(
                (ov - pl.corner_deg).abs() < 1e-9,
                "the decline carries the doubled cover {ov}, which IS the corner {}",
                pl.corner_deg
            );
        }
        other => panic!("expected a re-ordering decline, got {other:?}"),
    }
}

/// A fan with no crease chain at all has nothing to refine: the chain must be
/// CREATED, which needs the neighbour patch's mesh too. The majority shape —
/// R0044 v38 and R0003 v7611 / v8809 — and a decline, not a guess.
#[test]
fn a_fan_with_no_crease_chain_has_nothing_to_refine() {
    use crate::stage4_boundary_curve::EmissionEditFailure;
    let (mesh, attribution) = cut_fixture();
    assert_eq!(
        edits_of(&mesh, &attribution),
        Err(EmissionEditFailure::ChainAbsent)
    );
}

// ---------------------------------------------------------------------------
// inc-2c-3b-12b-5 — the EMISSION REGION
// ---------------------------------------------------------------------------

/// Run one fixture mesh through anatomy → cut → plan → edits → REGION.
fn region_of(
    mesh: &crate::Mesh,
    attribution: &crate::brep::TriangleAttributionMap,
) -> crate::stage4_boundary_curve::TransitEmissionRegion {
    let ed = edits_of(mesh, attribution).expect("the bracketed corner is an insertion");
    crate::stage4_boundary_curve::transit_emission_region(mesh, &ed)
        .expect("the fixture's host carriers bound a disk")
}

/// The region the mints land in is a topological DISK, and the site is on its
/// boundary.
///
/// That is the precondition the mutation needs: a single boundary cycle is a
/// polygon a re-triangulation is defined on, and a site left INTERIOR to it
/// would mean re-connecting the whole fan rather than filling a polygon. The
/// measured corpus instance is R0044 v47 — 6 triangles bounded by the octagon
/// `44 → 280 → 981 → 994 → 6911 → 47 → 6945 → 45`, with the site between the
/// chord's far end and the one fan triangle that carries no host edge.
#[test]
fn the_emission_region_is_a_disk_with_the_site_on_its_boundary() {
    for reversed in [false, true] {
        let (mesh, attribution) = interior_insert_fixture(true, reversed);
        let ed = edits_of(&mesh, &attribution).expect("insertion");
        let rg = region_of(&mesh, &attribution);

        // Every host carrier, and nothing else.
        let mut want: Vec<u32> = ed.crease_tris.clone();
        want.extend_from_slice(&ed.chain_tris[0]);
        want.extend_from_slice(&ed.chain_tris[1]);
        want.sort_unstable();
        want.dedup();
        assert_eq!(rg.tris, want, "reversed={reversed}");
        assert_eq!(rg.tris.len(), 6, "5 fan triangles + the neighbour's");

        // ONE cycle, each vertex visited once — a simple polygon.
        let mut seen = rg.boundary.clone();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), rg.boundary.len(), "the boundary self-touches");
        assert_eq!(rg.boundary.len(), 8, "an octagon: {:?}", rg.boundary);
        assert!(rg.site_on_boundary, "the site must stay on the boundary");

        // The mints land strictly INSIDE it: every host edge is interior to
        // the region, so neither mint is a boundary vertex.
        for (a, b) in [ed.crease_host, ed.inserts[0].chain, ed.inserts[1].chain] {
            let carriers = rg
                .tris
                .iter()
                .filter(|&&t| {
                    let tri = mesh.tris[t as usize];
                    tri.contains(&a) && tri.contains(&b)
                })
                .count();
            assert_eq!(carriers, 2, "host ({a},{b}) is interior to the region");
        }
    }
}

/// THE binding measurement: an edit list stated per-edge cannot be APPLIED
/// per-edge, and the reason is structural rather than incidental.
///
/// Each mint has two host edges — the chain it terminates and the crease chord
/// it refines — and the chord's own-patch carrier is the triangle apexed at
/// the SITE (§3y Reading 4: it is also the wholesale relabel). So splitting
/// that chord fans a new edge from the site to each mint, which is the very
/// edge the chain split already created. Every `(site, mint)` edge therefore
/// ends up carried by four triangles, in BOTH fan orders and with or without
/// the neighbour across the chord — the interference is forced by the
/// anatomy, not by a particular arrangement.
///
/// The corpus agrees: at R0044 v47 the per-edge split leaves `(47, 16355)` and
/// `(47, 16356)` at four incidences each.
#[test]
fn the_per_edge_split_doubles_every_site_to_mint_edge() {
    for across in [false, true] {
        for reversed in [false, true] {
            let (mesh, attribution) = interior_insert_fixture(across, reversed);
            let ed = edits_of(&mesh, &attribution).expect("insertion");
            let rg = region_of(&mesh, &attribution);

            // The chord's own-patch carrier IS apexed at the site — the
            // premise the doubling follows from.
            let (ha, hb) = ed.crease_host;
            assert!(
                ed.crease_tris.iter().any(|&t| {
                    let tri = mesh.tris[t as usize];
                    tri.contains(&ed.site) && tri.contains(&ha) && tri.contains(&hb)
                }),
                "the relabel triangle carries the chord and the site"
            );

            for m in rg.mints {
                let e = (ed.site.min(m), ed.site.max(m));
                let found = rg.overfull.iter().find(|o| o.edge == e).unwrap_or_else(|| {
                    panic!(
                        "({}, {m}) should be over-carried \
                             (across={across} reversed={reversed}): {:?}",
                        ed.site, rg.overfull
                    )
                });
                assert_eq!(found.incident, 4, "two hosts, two carriers each");
            }
        }
    }
}

/// The coincident FIN, by contrast, is order-dependent — so a fixture in one
/// orientation is not evidence about the other.
///
/// A mint's chain edge and the chord end it is nearest can be paired by the
/// fan's cyclic order or crossed by it. Paired, the chain split and the chord
/// split emit the same triangle twice in opposite windings: a zero-area fin.
/// R0044 v47 has the PAIRED arrangement (`[47, 981, 16355]`), and naming the
/// fixture's chord from its other end is what selects it — which is why the
/// over-carried edges above, not the fin, are the general statement.
#[test]
fn the_coincident_fin_depends_on_the_fan_order() {
    let (fmesh, fattr) = interior_insert_fixture(true, false);
    let (rmesh, rattr) = interior_insert_fixture(true, true);
    let (fwd, rev) = (region_of(&fmesh, &fattr), region_of(&rmesh, &rattr));

    assert!(
        fwd.coincident.is_empty(),
        "crossed by the fan order: {:?}",
        fwd.coincident
    );
    assert_eq!(rev.coincident.len(), 1, "paired: one fin");

    // The fin is the site, the chord end the mint is nearest, and the mint.
    let ed = edits_of(&rmesh, &rattr).expect("insertion");
    let fin = rev.coincident[0].verts;
    assert!(fin.contains(&ed.site), "the fin is apexed at the site");
    assert!(
        fin.contains(&rev.mints[0]),
        "the mint nearest the chord's first end: {fin:?}"
    );
    assert!(
        fin.contains(&ed.crease_host.0),
        "and that same first end: {fin:?}"
    );
    // Both parents are real children, and they wind oppositely — the pair
    // encloses no volume.
    let (i, j) = rev.coincident[0].parents;
    let (a, b) = (
        rev.naive_children[i as usize],
        rev.naive_children[j as usize],
    );
    let cyc = |t: [u32; 3], x: u32| (0..3).find(|&k| t[k] == x).expect("member");
    let step = |t: [u32; 3], x: u32| t[(cyc(t, x) + 1) % 3];
    assert_ne!(
        step(a, fin[0]),
        step(b, fin[0]),
        "the fin's two faces must wind oppositely: {a:?} {b:?}"
    );
}

/// Every over-carried edge touches a mint, and the region's own boundary is
/// left alone.
///
/// The interference is LOCAL: the split cannot damage the polygon it has to
/// stitch back into, or the mutation would reach further than §3y measured.
#[test]
fn the_interference_is_local_to_the_mints() {
    for reversed in [false, true] {
        let (mesh, attribution) = interior_insert_fixture(true, reversed);
        let rg = region_of(&mesh, &attribution);
        for o in &rg.overfull {
            assert!(
                rg.mints.contains(&o.edge.0) || rg.mints.contains(&o.edge.1),
                "over-carried edge {:?} touches no mint",
                o.edge
            );
        }
        // Every boundary edge of the region is still carried exactly twice —
        // once by a child, once by the triangle outside.
        for w in rg.boundary.windows(2) {
            let e = (w[0].min(w[1]), w[0].max(w[1]));
            assert!(
                !rg.overfull.iter().any(|o| o.edge == e),
                "the region's own boundary edge {e:?} must be untouched"
            );
        }
    }
}

/// A triangle carrying a host edge in two roles is REFUSED, not guessed at.
///
/// Its children are not defined by a single split, and the corpus does not
/// exhibit it — so the region declines structurally rather than picking a
/// host. Built by hand because no fixture produces the shape.
#[test]
fn a_triangle_in_two_host_roles_is_refused() {
    use crate::stage4_boundary_curve::{transit_emission_region, EmissionRegionFailure};
    let (mesh, attribution) = interior_insert_fixture(true, false);
    let mut ed = edits_of(&mesh, &attribution).expect("insertion");
    // Hand the chord's own-patch carrier to a chain slot as well.
    let dup = ed.crease_tris[0];
    ed.chain_tris[0].push(dup);
    assert_eq!(
        transit_emission_region(&mesh, &ed),
        Err(EmissionRegionFailure::TriangleInBothRoles { tri: dup })
    );
}

// ---------------------------------------------------------------------------
// inc-2c-3b-12b-6 — the FACE PARTITION and the boundary PINCH
// ---------------------------------------------------------------------------

/// The region is not the fill's unit: it spans both operands, and its own-patch
/// part is DISCONNECTED until the one triangle carrying no host edge rejoins it.
///
/// The corpus measurement is R0044 v47 — four parts, of which `(B, 168)` holds
/// `[13111, 13112, 13113]` in TWO edge-connected components, reconnected by
/// exactly `[13110]`. That is the same triangle §3z found leaving the site on
/// the region's boundary: excluding it both opened the region and cut the own
/// patch in half. Every part is a disk once closed.
#[test]
fn the_region_partitions_into_face_parts_and_the_own_patch_needs_closing() {
    use crate::stage4_boundary_curve::transit_emission_parts;
    for reversed in [false, true] {
        let (mesh, attribution) = interior_insert_fixture(true, reversed);
        let ed = edits_of(&mesh, &attribution).expect("insertion");
        let rg = region_of(&mesh, &attribution);
        let parts = transit_emission_parts(&mesh, &attribution, &rg, ed.site);

        // Every region triangle lands in exactly one part.
        let total: usize = parts.iter().map(|p| p.tris.len()).sum();
        assert_eq!(total, rg.tris.len(), "the partition is exact");
        assert!(parts.len() > 1, "the region spans more than one face");

        // The own patch — the part carrying the crease chord AND the site — is
        // the one that is cut in two, and its closure is a single triangle.
        let (ha, hb) = ed.crease_host;
        let own = parts
            .iter()
            .find(|p| {
                p.tris.iter().any(|&t| {
                    let tri = mesh.tris[t as usize];
                    tri.contains(&ha) && tri.contains(&hb) && tri.contains(&ed.site)
                })
            })
            .expect("some part carries the chord at the site");
        assert_eq!(own.components, 2, "the own patch is cut in two");
        assert_eq!(own.closure.len(), 1, "one triangle rejoins it");
        assert!(
            !rg.tris.contains(&own.closure[0]),
            "the closure is outside the region — that is why it was cut"
        );

        // Closed, every part is a disk: one component, one simple boundary.
        for p in &parts {
            assert_eq!(p.components_closed, 1, "part {:?} is connected", p.face);
            let b = p
                .boundary_closed
                .as_ref()
                .unwrap_or_else(|| panic!("part {:?} bounds one cycle", p.face));
            let mut seen = b.clone();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(seen.len(), b.len(), "part {:?} self-touches", p.face);
        }
    }
}

/// THE fill plan: inserting the mints pinches the own patch, and the loop
/// holding the site is the NOTCH.
///
/// The own patch carries all three host edges, so each mint lands on its
/// boundary TWICE. §3z measured that doubling as a per-edge split's
/// non-manifold residue; taken on the BOUNDARY instead of on the edges it is
/// not a pathology at all — a cycle that repeats a vertex pinches there, and
/// two pinches cut the own patch into a corner and its remainders. At R0044
/// v47 the loops are `[16355, 280, 981]`, `[16356, 47, 16355]` (the notch) and
/// `[45, 16356, 6911, 6945]`, and no other part pinches: each of those carries
/// a host edge in one role only, so each is an ordinary polygon fill that
/// receives the mints conformally.
///
/// But the clean cut has a PRECONDITION, and the fixture reaches both sides of
/// it: the two mints' repeat spans must not CROSS. Interleaved, the loops that
/// come out are not a corner and its remainders — the site's loop swells to
/// most of the patch — so there is no notch to hand over and the fill declines.
/// R0044 v47 is not interleaved; naming the fixture's chord from its other end
/// is what reaches the shape that is.
#[test]
fn inserting_the_mints_pinches_the_own_patch_into_the_notch() {
    use crate::stage4_boundary_curve::{transit_boundary_pinch, transit_emission_parts};
    // `reversed` names the same chord from its other end; only one of the two
    // arrangements gives a clean corner, and the corpus site is that one.
    for (reversed, want_interleaved) in [(true, false), (false, true)] {
        let (mesh, attribution) = interior_insert_fixture(true, reversed);
        let ed = edits_of(&mesh, &attribution).expect("insertion");
        let rg = region_of(&mesh, &attribution);
        let parts = transit_emission_parts(&mesh, &attribution, &rg, ed.site);

        let mut pinched = 0;
        for p in &parts {
            let b = p.boundary_closed.as_ref().expect("a disk");
            let pin = transit_boundary_pinch(&ed, rg.mints, b, ed.site).expect("a start vertex");

            // Every mint on this part's boundary is inserted once per host
            // edge the part carries — the conforming half of the fill.
            let norm = |a: u32, b: u32| (a.min(b), a.max(b));
            for (mi, m) in rg.mints.iter().enumerate() {
                let hosts = (0..b.len())
                    .filter(|&k| {
                        let e = norm(b[k], b[(k + 1) % b.len()]);
                        e == norm(ed.crease_host.0, ed.crease_host.1)
                            || e == norm(ed.inserts[mi].chain.0, ed.inserts[mi].chain.1)
                    })
                    .count();
                assert_eq!(
                    pin.inserted.iter().filter(|x| *x == m).count(),
                    hosts,
                    "mint {m} on part {:?}: one insert per host edge carried",
                    p.face
                );
            }

            if pin.loops.len() == 1 {
                assert!(pin.notch.is_none(), "an unpinched part has no notch");
                continue;
            }
            pinched += 1;
            assert_eq!(
                pin.interleaved, want_interleaved,
                "part {:?}, reversed={reversed}",
                p.face
            );
            if want_interleaved {
                // The crossed arrangement yields no corner, and says so rather
                // than handing over a loop that is not one.
                assert!(pin.notch.is_none(), "an interleaved pinch has no notch");
                continue;
            }
            // The clean cut: three loops, and the site's is the corner.
            assert_eq!(pin.loops.len(), 3, "two pinches, three loops");
            let n = pin.notch.expect("a clean pinch has a notch");
            let notch = &pin.loops[n];
            assert_eq!(notch.len(), 3, "the notch is the corner triangle");
            assert!(notch.contains(&ed.site));
            assert!(
                rg.mints.iter().all(|m| notch.contains(m)),
                "the notch is bounded by both mints: {notch:?}"
            );
            for (i, l) in pin.loops.iter().enumerate() {
                if i != n {
                    assert!(!l.contains(&ed.site), "only the notch holds the site");
                    assert!(l.len() >= 3, "a loop is a polygon");
                }
            }
            // Every loop's vertices come from the inserted boundary, and
            // together they use each of its entries once — the pinch
            // PARTITIONS the cycle rather than duplicating any of it.
            let used: usize = pin.loops.iter().map(|l| l.len()).sum();
            assert_eq!(used, pin.inserted.len(), "the pinch partitions the cycle");
            // And every loop EDGE is a directed consecutive pair of the
            // boundary — including the one closing back to the pinch point.
            // So the loops inherit the part's winding, which is what lets the
            // fill emit them without re-deriving an orientation.
            let n_ins = pin.inserted.len();
            let consecutive: std::collections::BTreeSet<(u32, u32)> = (0..n_ins)
                .map(|k| (pin.inserted[k], pin.inserted[(k + 1) % n_ins]))
                .collect();
            for l in &pin.loops {
                for k in 0..l.len() {
                    let e = (l[k], l[(k + 1) % l.len()]);
                    assert!(
                        consecutive.contains(&e),
                        "loop edge {e:?} is not a directed boundary step: {:?}",
                        pin.inserted
                    );
                }
            }
        }
        assert_eq!(pinched, 1, "only the own patch pinches");
    }
}

// ---------------------------------------------------------------------------
// §4.5.2 inc-2c-3b-12b-7 — the EMISSION FILL, planned but not written
// ---------------------------------------------------------------------------

/// The fixture's surfaces, with the neighbour across the chord — `(B, 8)`,
/// cone B — added, the way the census resolves every face against its BRep.
fn fill_fixture_surfaces(i: InputId, face: u32) -> Option<Surface> {
    let (_, cone_b, _, _) = transit_fixture();
    match (i, face) {
        (InputId::B, 8) => Some(cone_b),
        _ => cut_fixture_surfaces(i, face),
    }
}

/// The interior-insert fixture, made geometrically faithful where the fill
/// needs it, plus two SURVIVORS.
///
/// The fill is the first increment that projects the fan into its faces'
/// charts, and a chart sees what the topological increments did not: the
/// chain end `v1` was placed by crease ANGLE and does not lie on plane_x, so
/// the exact q-point lands 13 units outside its chord's projection and the
/// plane_x quad is a bow-tie (the CDT refuses it — measured first, then
/// fixed here). Both chain ends are therefore put ON their planes and on cone
/// A's home side, as the corpus chain edges are: `v1 = (66, 60, √(66²+60²))`
/// for plane_x, `v5 = (40, 88, √(40²+88²))` for plane_y. Their sides,
/// attributions and fan order are unchanged.
///
/// The survivors are triangles outside the region sharing an edge with it, so
/// the orientation certificate has testimony to check against: `[2, 1, s8]`
/// continues the own patch past the chain end and `[1, 6, s9]` continues
/// plane_x past the carrier; both are wound consistently with the fan
/// triangles they border (`[0, 1, 2]` traverses `1 → 2`, `[0, 6, 1]`
/// traverses `6 → 1`). Neither touches the site, a host edge or a q-point, so
/// anatomy, cut, plan, edits, region and parts are exactly the §3y–§3aa
/// fixture's.
fn fill_fixture(reversed: bool) -> (crate::Mesh, crate::brep::TriangleAttributionMap) {
    let (mut mesh, mut attribution) = interior_insert_fixture(true, reversed);
    mesh.verts[1] = Point3::new(66.0, 60.0, (66.0f64 * 66.0 + 60.0 * 60.0).sqrt());
    mesh.verts[5] = Point3::new(40.0, 88.0, (40.0f64 * 40.0 + 88.0 * 88.0).sqrt());
    let home = |deg: f64| {
        let r: f64 = f64::to_radians(deg);
        Point3::new(90.0 * r.cos(), 90.0 * r.sin(), 90.0)
    };
    let s8 = mesh.verts.len() as u32;
    mesh.verts.push(home(72.0));
    let s9 = mesh.verts.len() as u32;
    mesh.verts.push(Point3::new(66.0, 60.0, 70.0));
    mesh.tris.push([2, 1, s8]);
    mesh.tris.push([1, 6, s9]);
    let f = |input, face| Some(crate::brep::TriangleAttribution { input, face });
    attribution.attributions.push(f(InputId::B, 7));
    attribution.attributions.push(f(InputId::A, 2));
    (mesh, attribution)
}

fn fill_of(
    mesh: &crate::Mesh,
    attribution: &crate::brep::TriangleAttributionMap,
    surfaces: &dyn Fn(InputId, u32) -> Option<Surface>,
) -> Result<
    crate::stage4_boundary_curve::TransitEmissionFill,
    crate::stage4_boundary_curve::EmissionFillFailure,
> {
    use crate::stage4_boundary_curve::{transit_emission_fill, transit_emission_parts};
    let ed = edits_of(mesh, attribution).expect("insertion");
    let rg = region_of(mesh, attribution);
    let parts = transit_emission_parts(mesh, attribution, &rg, ed.site);
    let (t, ..) = cut_fixture_transit();
    transit_emission_fill(mesh, attribution, &ed, &rg, &parts, &t, surfaces)
}

/// THE FILL: the pinched loops become chart fills, the notch goes to the
/// neighbour, and the result is a manifold, conformal, consistently wound
/// mesh — certified against the WHOLE mesh, not the region in isolation.
///
/// The measured corpus instance is R0044 v47: 7 triangles out, 11 in — `(A, 2)`
/// and `(A, 3)` quads, the `(B, 167)` pentagon, and the own patch's triangle,
/// quad and notch — every mint edge carried exactly twice, the notch
/// `[16356, 47, 16355]` attributed to `(B, 167)`.
#[test]
fn the_fill_is_manifold_conformal_and_wound_by_its_loops() {
    use crate::stage4_boundary_curve::transit_emission_parts;
    let (mesh, attribution) = fill_fixture(true);
    let ed = edits_of(&mesh, &attribution).expect("insertion");
    let rg = region_of(&mesh, &attribution);
    let parts = transit_emission_parts(&mesh, &attribution, &rg, ed.site);
    let fill = fill_of(&mesh, &attribution, &fill_fixture_surfaces).expect("a determined fill");

    // What goes: the region and every closure, and nothing §3y did not
    // already name as touched.
    let mut want_removed: Vec<u32> = parts
        .iter()
        .flat_map(|p| p.tris.iter().chain(p.closure.iter()).copied())
        .collect();
    want_removed.sort_unstable();
    want_removed.dedup();
    assert_eq!(fill.removed, want_removed);
    assert!(
        fill.touched_delta.is_empty(),
        "removed and touched were derived by different routes and must agree: {:?}",
        fill.touched_delta
    );
    // The survivors are not touched.
    let n = mesh.tris.len() as u32;
    assert!(!fill.removed.contains(&(n - 1)) && !fill.removed.contains(&(n - 2)));

    // What comes: one polygon per part, plus the own patch's two extra loops.
    assert_eq!(fill.polygons.len(), parts.len() + 2);
    let notches: Vec<_> = fill.polygons.iter().filter(|p| p.notch).collect();
    assert_eq!(notches.len(), 1, "exactly one notch");
    let notch = notches[0];
    assert_eq!(notch.polygon.len(), 3, "the notch is the corner triangle");
    assert!(notch.polygon.contains(&ed.site));
    assert!(rg.mints.iter().all(|m| notch.polygon.contains(m)));
    assert_eq!(fill.own_face, (InputId::B, 7));
    assert_eq!(
        fill.notch_face,
        (InputId::B, 8),
        "the far side of the chord"
    );
    assert_eq!(
        notch.face, fill.notch_face,
        "the notch is attributed to the neighbour"
    );
    assert_eq!(fill.notch_surface_agrees, Some(true));
    assert_eq!(
        fill.polygons
            .iter()
            .filter(|p| p.face == fill.own_face)
            .count(),
        2,
        "the own patch keeps its triangle and its quad"
    );
    // Every polygon is triangulated without Steiner points: n − 2 triangles,
    // on exactly its own vertices.
    for p in &fill.polygons {
        assert_eq!(p.tris.len(), p.polygon.len() - 2, "{:?}", p.polygon);
        for tri in &p.tris {
            assert!(tri.iter().all(|v| p.polygon.contains(v)));
        }
    }

    // The certificates: manifold, conformal, consistently wound.
    assert!(
        fill.edge_defects.is_empty(),
        "every touched edge has the incidence a manifold requires: {:?}",
        fill.edge_defects
    );
    assert_eq!(fill.folded, 0, "no fill edge folds onto a survivor");
    assert_eq!(fill.added_folds, 0, "no fold inside the fill");
    // The chart→3D lift: every face's fill lies ONE way on its surface, it
    // lies the way the fossil did wherever the fossil was unanimous, and
    // every triangle was certifiable. (The fixture's own-patch fossil is NOT
    // unanimous — its fan is topological, not angularly monotone, and two of
    // its four triangles lift against the cone — which is exactly the folded
    // fossil §3q warned about, measured here rather than assumed away; the
    // fill's three replacements agree with each other.)
    assert_eq!(
        (fill.lift_flips, fill.lift_uncertified),
        (0, 0),
        "{:?}",
        fill.lift
    );
    for ls in &fill.lift {
        assert!(
            ls.old_along + ls.old_against > 0,
            "{:?} gave up nothing",
            ls.face
        );
        assert!(
            ls.new_along + ls.new_against > 0,
            "{:?} receives nothing",
            ls.face
        );
        assert!(
            ls.new_along == 0 || ls.new_against == 0,
            "fill folded: {ls:?}"
        );
        if ls.old_against == 0 {
            assert_eq!(ls.new_against, 0, "{ls:?}");
        }
        if ls.old_along == 0 {
            assert_eq!(ls.new_along, 0, "{ls:?}");
        }
    }
    let own_lift = fill
        .lift
        .iter()
        .find(|l| l.face == fill.own_face)
        .expect("the own patch is certified");
    assert_eq!(
        (own_lift.old_along, own_lift.old_against),
        (2, 2),
        "the fixture's own fossil is folded two against two"
    );
    assert_eq!(
        fill.opposed, 2,
        "each survivor shares exactly one edge with the fill, traversed the other way"
    );
    // Each mint carries the four edges its two roles demand — the stub to the
    // site and the stub to its chain end (the chain, split), the half-chord to
    // its chord end and the corner edge to the other mint (the chord, split)
    // — and every one of them is interior to the fill, carried by exactly two
    // fill triangles. (How many FURTHER edges a mint gets is the CDT's
    // diagonal choice, not an invariant.)
    let carried = |x: u32, y: u32| {
        fill.polygons
            .iter()
            .flat_map(|p| p.tris.iter())
            .filter(|tri| tri.contains(&x) && tri.contains(&y))
            .count()
    };
    let (ha, hb) = ed.crease_host;
    for (i, m) in rg.mints.iter().enumerate() {
        let chain_end = if ed.inserts[i].chain.0 == ed.site {
            ed.inserts[i].chain.1
        } else {
            ed.inserts[i].chain.0
        };
        let chord_end = if i == 0 { ha } else { hb };
        let other = rg.mints[1 - i];
        for (name, y) in [
            ("site stub", ed.site),
            ("chain end", chain_end),
            ("chord end", chord_end),
            ("corner edge", other),
        ] {
            assert_eq!(carried(*m, y), 2, "mint {m}: {name} ({m}, {y})");
        }
    }
}

/// The fill's positions are the exact analytic ones: the mints at the solved
/// q-points ON the crease circle, the site at the corrected junction — never a
/// chord crossing, never the out-of-domain solution.
#[test]
fn the_fill_places_the_mints_at_the_q_points_and_the_site_at_the_junction() {
    let (mesh, attribution) = fill_fixture(true);
    let ed = edits_of(&mesh, &attribution).expect("insertion");
    let fill = fill_of(&mesh, &attribution, &fill_fixture_surfaces).expect("fill");
    let (t, ..) = cut_fixture_transit();
    assert_eq!(fill.mints[0].0, mesh.verts.len() as u32);
    assert_eq!(fill.mints[1].0, mesh.verts.len() as u32 + 1);
    for (i, (_, at)) in fill.mints.iter().enumerate() {
        assert_eq!(*at, ed.inserts[i].at);
        let a = at.as_array();
        assert!((a[2] - 100.0).abs() < 1e-9, "mint {i} off the crease plane");
        assert!(
            ((a[0] * a[0] + a[1] * a[1]).sqrt() - 100.0).abs() < 1e-9,
            "mint {i} off the crease circle"
        );
    }
    assert_eq!(fill.site_at, t.j);
    let j = fill.site_at.as_array();
    assert!(
        (j[0] - 66.0).abs() < 1e-9 && (j[1] - 88.0).abs() < 1e-9 && (j[2] - 105.0).abs() < 1e-9
    );
}

/// The orientation certificate is not vacuous: wind one survivor the wrong
/// way and the fill reports the fold instead of hiding it in an area sum.
#[test]
fn a_folded_survivor_is_reported_not_absorbed() {
    let (mut mesh, attribution) = fill_fixture(true);
    let last = mesh.tris.len() - 1;
    mesh.tris[last].swap(1, 2);
    let fill = fill_of(&mesh, &attribution, &fill_fixture_surfaces).expect("fill");
    assert_eq!(fill.folded, 1);
    assert_eq!(fill.opposed, 1);
    assert!(
        fill.edge_defects.is_empty(),
        "a fold is not an incidence defect"
    );
}

/// The interleaved arrangement — the same chord named from its other end —
/// has no notch, and the fill declines rather than filling loops that are
/// not a corner and its remainders.
#[test]
fn the_interleaved_arrangement_has_no_fill() {
    use crate::stage4_boundary_curve::EmissionFillFailure;
    let (mesh, attribution) = fill_fixture(false);
    assert_eq!(
        fill_of(&mesh, &attribution, &fill_fixture_surfaces),
        Err(EmissionFillFailure::Interleaved)
    );
}

/// A chord no neighbour carries gives the notch nowhere to go: §3y's empty
/// reach becomes a typed decline here, not a guess at a face.
#[test]
fn a_chord_no_neighbour_carries_gives_the_notch_no_destination() {
    use crate::stage4_boundary_curve::EmissionFillFailure;
    let (mesh, attribution) = interior_insert_fixture(false, true);
    assert_eq!(
        fill_of(&mesh, &attribution, &fill_fixture_surfaces),
        Err(EmissionFillFailure::NotchDestinationUnknown)
    );
}

/// The mesh-derived destination is checked against the analytic one: a face
/// whose surface is not the transit's neighbour is reported as a
/// disagreement, and a face without a chart is a typed decline.
#[test]
fn the_notch_destination_is_checked_against_the_transit_neighbour() {
    use crate::stage4_boundary_curve::EmissionFillFailure;
    let (mesh, attribution) = fill_fixture(true);
    // The neighbour's face resolves to the OWN cone: same chart, wrong
    // surface identity.
    let wrong = |i: InputId, face: u32| -> Option<Surface> {
        let (cone_a, ..) = transit_fixture();
        match (i, face) {
            (InputId::B, 8) => Some(cone_a),
            _ => cut_fixture_surfaces(i, face),
        }
    };
    let fill = fill_of(&mesh, &attribution, &wrong).expect("fill");
    assert_eq!(fill.notch_surface_agrees, Some(false));
    // No surface for the neighbour: the pentagon has no chart.
    assert_eq!(
        fill_of(&mesh, &attribution, &cut_fixture_surfaces),
        Err(EmissionFillFailure::NoChart {
            face: Some((InputId::B, 8))
        })
    );
}

/// The like-for-like chord bound: planes certify at exactly zero both ways,
/// the cones certify finitely, and no face receives a fill coarser than what
/// it gave up.
#[test]
fn the_fill_is_no_coarser_than_what_it_replaces() {
    let (mesh, attribution) = fill_fixture(true);
    let fill = fill_of(&mesh, &attribution, &fill_fixture_surfaces).expect("fill");
    let by_face = |f: (InputId, u32)| {
        fill.chord
            .iter()
            .find(|c| c.face == f)
            .unwrap_or_else(|| panic!("a budget for {f:?}"))
    };
    for f in [(InputId::A, 2), (InputId::A, 3)] {
        let c = by_face(f);
        assert_eq!((c.old_max, c.new_max), (Some(0.0), Some(0.0)), "{f:?}");
    }
    for f in [(InputId::B, 7), (InputId::B, 8)] {
        let c = by_face(f);
        let (old, new) = (c.old_max.expect("certified"), c.new_max.expect("certified"));
        assert!(
            old.is_finite() && new.is_finite() && old > 0.0,
            "{f:?}: {old} / {new}"
        );
        assert!(
            new <= old,
            "{f:?}: the fill ({new}) is coarser than the fossil ({old})"
        );
    }
}

// ---------------------------------------------------------------------------
// §4.5.2 inc-2c-3b-12b-8 — WRITING the certified fill
// ---------------------------------------------------------------------------

/// Undirected edge → incidence count over a whole mesh.
fn edge_counts(mesh: &crate::Mesh) -> std::collections::BTreeMap<(u32, u32), usize> {
    let mut m = std::collections::BTreeMap::new();
    for tri in &mesh.tris {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            *m.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    m
}

/// THE WRITE: the certified fill lands in the mesh slot-stably — the removed
/// slots are overwritten, the surplus appended, the mints take exactly the ids
/// the plan named, the site moves to `J` — and the whole mesh comes out with
/// the incidence and winding the certificate promised.
#[test]
fn the_write_lands_the_fill_slot_stably_and_the_mesh_is_manifold() {
    use crate::stage4_boundary_curve::transit_emission_write;
    let (mut mesh, mut attribution) = fill_fixture(true);
    let fill = fill_of(&mesh, &attribution, &fill_fixture_surfaces).expect("fill");
    let before = mesh.clone();
    let before_counts = edge_counts(&before);
    let (nv, nt) = (mesh.verts.len(), mesh.tris.len());

    let report = transit_emission_write(&mut mesh, &mut attribution, &fill).expect("written");

    // Counts: two mints, |added| − |removed| more triangles, attribution
    // parallel, the removed slots overwritten and the rest appended.
    assert_eq!(mesh.verts.len(), nv + 2);
    assert_eq!(report.mints, [nv as u32, nv as u32 + 1]);
    assert_eq!(mesh.verts[nv], fill.mints[0].1);
    assert_eq!(mesh.verts[nv + 1], fill.mints[1].1);
    assert_eq!(mesh.verts[fill.site as usize], fill.site_at);
    let added: usize = fill.polygons.iter().map(|p| p.tris.len()).sum();
    assert_eq!(mesh.tris.len(), nt + added - fill.removed.len());
    assert_eq!(attribution.attributions.len(), mesh.tris.len());
    assert_eq!(report.overwritten, fill.removed.len());
    assert_eq!(report.appended, added - fill.removed.len());
    assert_eq!((report.removed, report.added), (fill.removed.len(), added));

    // Every survivor keeps its slot, its triangle and its attribution.
    for t in 0..nt as u32 {
        if fill.removed.contains(&t) {
            continue;
        }
        assert_eq!(mesh.tris[t as usize], before.tris[t as usize], "slot {t}");
        assert_eq!(
            attribution.attributions[t as usize],
            attribution.lookup(t),
            "slot {t}"
        );
    }
    // Every fill triangle is in the mesh with its face, and no removed
    // triangle's vertex set survives.
    for p in &fill.polygons {
        for tri in &p.tris {
            let slot = mesh
                .tris
                .iter()
                .position(|x| x == tri)
                .unwrap_or_else(|| panic!("fill triangle {tri:?} missing"));
            assert_eq!(
                attribution.lookup(slot as u32).map(|a| (a.input, a.face)),
                Some(p.face)
            );
        }
    }
    // (A removed triangle's vertex SET may legitimately reappear: with the
    // site at `J` the plane_x quad is convex and its Delaunay diagonal is the
    // old chain chord `(0, 1)`, so `{0, 1, 6}` comes back as a fill triangle
    // — the chord survives only as an interior diagonal of one face, no
    // longer as a face boundary. What matters is the incidence below.)

    // The whole mesh: every edge carried once (the fixture's open rim) or
    // twice, every pre-existing edge at its old count, every mint edge at
    // two, and no directed edge carried twice (consistent winding).
    let after = edge_counts(&mesh);
    for (&e, &c) in &after {
        let expected = match before_counts.get(&e) {
            Some(&b) => b,
            None => 2,
        };
        assert_eq!(c, expected, "edge {e:?}");
        assert!(c == 1 || c == 2, "edge {e:?} carried {c} times");
    }
    let mut directed = std::collections::BTreeSet::new();
    for tri in &mesh.tris {
        for k in 0..3 {
            assert!(
                directed.insert((tri[k], tri[(k + 1) % 3])),
                "directed edge {:?} carried twice",
                (tri[k], tri[(k + 1) % 3])
            );
        }
    }
    // The site's fan after the write is exactly the corrected junction's
    // three faces: the two planes and the NEIGHBOUR cone — never the own
    // patch it transited out of.
    let mut faces: Vec<_> = mesh
        .tris
        .iter()
        .enumerate()
        .filter(|(_, tri)| tri.contains(&fill.site))
        .filter_map(|(t, _)| attribution.lookup(t as u32).map(|a| (a.input, a.face)))
        .collect();
    faces.sort_unstable();
    faces.dedup();
    assert_eq!(
        faces,
        vec![(InputId::A, 2), (InputId::A, 3), (InputId::B, 8)],
        "the site's incidence after the transit"
    );
}

/// The write refuses — leaving the mesh untouched — on any unclean
/// certificate, on stale mint ids, and on a fill smaller than what it removes.
#[test]
fn the_write_refuses_an_uncertified_fill_and_leaves_the_mesh_untouched() {
    use crate::stage4_boundary_curve::{
        transit_emission_write, EdgeIncidence, EmissionWriteFailure,
    };
    let (mut mesh, mut attribution) = fill_fixture(true);
    let fill = fill_of(&mesh, &attribution, &fill_fixture_surfaces).expect("fill");
    let before = (mesh.clone(), attribution.clone());

    let mut bad = fill.clone();
    bad.edge_defects.push(EdgeIncidence {
        edge: (0, 1),
        before: 2,
        after: 3,
        expected: 2,
    });
    assert_eq!(
        transit_emission_write(&mut mesh, &mut attribution, &bad),
        Err(EmissionWriteFailure::CertificateFailed {
            what: "edge_defects"
        })
    );
    let mut bad = fill.clone();
    bad.folded = 1;
    assert_eq!(
        transit_emission_write(&mut mesh, &mut attribution, &bad),
        Err(EmissionWriteFailure::CertificateFailed { what: "folded" })
    );
    let mut bad = fill.clone();
    bad.notch_surface_agrees = Some(false);
    assert_eq!(
        transit_emission_write(&mut mesh, &mut attribution, &bad),
        Err(EmissionWriteFailure::CertificateFailed {
            what: "notch_surface_agrees"
        })
    );
    let mut bad = fill.clone();
    bad.chord[0].new_max = Some(f64::INFINITY);
    assert_eq!(
        transit_emission_write(&mut mesh, &mut attribution, &bad),
        Err(EmissionWriteFailure::CertificateFailed { what: "chord" })
    );
    let mut bad = fill.clone();
    bad.lift_flips = 1;
    assert_eq!(
        transit_emission_write(&mut mesh, &mut attribution, &bad),
        Err(EmissionWriteFailure::CertificateFailed { what: "lift_flips" })
    );
    let mut bad = fill.clone();
    bad.polygons.truncate(1);
    assert_eq!(
        transit_emission_write(&mut mesh, &mut attribution, &bad),
        Err(EmissionWriteFailure::FewerAddedThanRemoved {
            added: bad.polygons[0].tris.len(),
            removed: fill.removed.len(),
        })
    );
    assert_eq!((mesh.clone(), attribution.clone()), before, "untouched");

    // A vertex appended after planning makes the mint ids stale.
    mesh.verts.push(Point3::new(0.0, 0.0, 0.0));
    let next = mesh.verts.len() as u32;
    assert_eq!(
        transit_emission_write(&mut mesh, &mut attribution, &fill),
        Err(EmissionWriteFailure::MintIdsStale {
            planned: [fill.mints[0].0, fill.mints[1].0],
            next,
        })
    );
    assert_eq!(mesh.verts.len(), next as usize, "untouched");
    assert_eq!(mesh.tris, before.0.tris);
}
