//! The Q1 oracles of `specs/agent_mechanical_design.md` §4.4 (Distance):
//!
//! - closed-form distances the exact-membership vocabulary can state — two
//!   spheres, a sphere and a plane, two parallel cylinders, a point and each
//!   primitive — must match to `TAU_MODEL`;
//! - a body against a copy of itself translated by `d` along x is `d − extent`;
//! - symmetric in its operands;
//! - zero when the operands touch, and zero (not a positive edge distance)
//!   when they overlap.
//!
//! The tier matters as much as the number: a pair of planar faces is `exact`
//! from the seed, a curved pair only if the analytic refinement converged and
//! certified, and the test asserts WHICH, so a silent mesh answer cannot pass
//! as a measurement.

use std::f64::consts::PI;

use cad_primitives::{Point2, Point3, Vector3, TAU_MODEL};
use waffle_types::kernel::RigidPlacement;

use kernel_v2::{
    distance, distance_along, extrude, revolve, transform_solid, BrepArena, On, Profile, SolidId,
    Target,
};

/// A box with its near-origin corner at `origin`, of the given size.
fn boxx(arena: &mut BrepArena, origin: [f64; 3], sx: f64, sy: f64, sz: f64) -> SolidId {
    let p = Profile::new(
        Point3::new(origin[0], origin[1], origin[2]),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(sx, 0.0),
            Point2::new(sx, sy),
            Point2::new(0.0, sy),
        ],
        vec![],
    )
    .expect("rect profile");
    extrude(arena, &p, Vector3::new(0.0, 0.0, 1.0), sz)
        .expect("box")
        .solid
}

/// A cylinder of radius `r`, height `h`, axis +z through `(cx, cy, 0)`.
fn cylinder(arena: &mut BrepArena, cx: f64, cy: f64, r: f64, h: f64) -> SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(cx, cy),
        r,
    )
    .expect("circle profile");
    extrude(arena, &p, Vector3::new(0.0, 0.0, 1.0), h)
        .expect("cylinder")
        .solid
}

/// A ball of radius `r` centred at `(cx, 0, 0)`: the full-turn revolve of an
/// on-axis circle about +x̂.
fn ball(arena: &mut BrepArena, cx: f64, r: f64) -> SolidId {
    let c = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(cx, 0.0),
        r,
    )
    .expect("circle profile");
    revolve(
        arena,
        &c,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("ball")
    .solid
}

fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} vs {b} (tol {tol:e})");
}

#[test]
fn two_boxes_measure_their_analytic_gap_exactly_and_symmetrically() {
    let mut arena = BrepArena::new();
    let a = boxx(&mut arena, [0.0, 0.0, 0.0], 1.0, 1.0, 1.0);
    let b = boxx(&mut arena, [2.5, 0.0, 0.0], 1.0, 1.0, 1.0);

    let d = distance(&arena, Target::Solid(a), Target::Solid(b)).expect("distance");
    assert!(d.exact, "two planar bodies measure exactly: {d:?}");
    close(d.value, 1.5, 0.0, "box gap");
    // The closest points are on the facing faces, at x = 1 and x = 2.5.
    close(d.points[0].x(), 1.0, 0.0, "point on a");
    close(d.points[1].x(), 2.5, 0.0, "point on b");
    assert!(
        matches!(d.on[0], Some(On::Face(_))) && matches!(d.on[1], Some(On::Face(_))),
        "both points sit on faces: {:?}",
        d.on
    );

    // Symmetric: the same number, the points swapped.
    let r = distance(&arena, Target::Solid(b), Target::Solid(a)).expect("distance");
    assert_eq!(r.value, d.value, "distance is symmetric");
    assert_eq!(r.exact, d.exact);
    close(r.points[0].x(), 2.5, 0.0, "swapped point on b");
    close(r.points[1].x(), 1.0, 0.0, "swapped point on a");
}

#[test]
fn touching_boxes_are_zero_and_overlapping_boxes_are_zero() {
    let mut arena = BrepArena::new();
    let a = boxx(&mut arena, [0.0, 0.0, 0.0], 1.0, 1.0, 1.0);
    let touching = boxx(&mut arena, [1.0, 0.0, 0.0], 1.0, 1.0, 1.0);
    let overlapping = boxx(&mut arena, [0.5, 0.0, 0.0], 1.0, 1.0, 1.0);

    let d = distance(&arena, Target::Solid(a), Target::Solid(touching)).expect("distance");
    assert_eq!(d.value, 0.0, "coplanar touching faces are zero: {d:?}");
    assert!(d.exact);

    // Overlap must be ZERO, not the positive face-to-face gap an edge-only
    // triangle decomposition would report.
    let d = distance(&arena, Target::Solid(a), Target::Solid(overlapping)).expect("distance");
    assert_eq!(d.value, 0.0, "overlapping bodies are zero: {d:?}");
}

#[test]
fn two_balls_measure_the_closed_form_centre_distance_minus_radii() {
    let mut arena = BrepArena::new();
    // Two unit balls, centres 5 apart on x: the gap is 5 − 1 − 1 = 3.
    let a = ball(&mut arena, 0.0, 1.0);
    let b = ball(&mut arena, 5.0, 1.0);

    let d = distance(&arena, Target::Solid(a), Target::Solid(b)).expect("distance");
    assert!(
        d.exact,
        "a sphere pair refines onto its analytic surfaces: {d:?}"
    );
    close(d.value, 3.0, TAU_MODEL, "ball gap");
    // The feet are on the axis joining the centres, on each sphere.
    close(d.points[0].x(), 1.0, TAU_MODEL, "foot on a");
    close(d.points[1].x(), 4.0, TAU_MODEL, "foot on b");
    close(d.points[0].y(), 0.0, TAU_MODEL, "foot on a (y)");
    close(d.points[0].z(), 0.0, TAU_MODEL, "foot on a (z)");

    let r = distance(&arena, Target::Solid(b), Target::Solid(a)).expect("distance");
    close(r.value, d.value, 0.0, "symmetric");
}

#[test]
fn a_ball_and_a_plane_face_measure_the_centre_height_minus_the_radius() {
    let mut arena = BrepArena::new();
    // A ball of radius 1 at (0,0,0) and a plate whose top face is z = −4.
    let b = ball(&mut arena, 0.0, 1.0);
    let plate = boxx(&mut arena, [-5.0, -5.0, -5.0], 10.0, 10.0, 1.0);

    let d = distance(&arena, Target::Solid(b), Target::Solid(plate)).expect("distance");
    assert!(d.exact, "sphere against a plane is exact: {d:?}");
    close(d.value, 3.0, TAU_MODEL, "ball-to-plate gap");
    close(d.points[0].z(), -1.0, TAU_MODEL, "foot on the ball");
    close(d.points[1].z(), -4.0, TAU_MODEL, "foot on the plate");
}

/// The one face of `solid` whose surface is cylindrical.
fn lateral(arena: &BrepArena, solid: SolidId) -> kernel_v2::FaceId {
    let s = arena.solid(solid).expect("solid");
    s.shells
        .iter()
        .flat_map(|&sh| arena.shell(sh).expect("shell").faces.clone())
        .find(|&f| {
            kernel_v2::face_signature(arena, f).surface_type.as_deref() == Some("cylindrical")
        })
        .expect("a cylindrical face")
}

#[test]
fn two_parallel_cylinders_measure_the_axis_distance_minus_radii() {
    let mut arena = BrepArena::new();
    // Axes 10 apart on x, radii 2 and 3: the gap is 10 − 2 − 3 = 5.
    let a = cylinder(&mut arena, 0.0, 0.0, 2.0, 4.0);
    let b = cylinder(&mut arena, 10.0, 0.0, 3.0, 4.0);

    // The §4.4 closed form, as a surface pair: the two laterals. The seed is
    // chordal (5.00294 — the two sagittas), and the refinement certifies the
    // closed form.
    let d = distance(
        &arena,
        Target::Face(lateral(&arena, a)),
        Target::Face(lateral(&arena, b)),
    )
    .expect("lateral pair");
    assert!(d.exact, "a lateral pair refines and certifies: {d:?}");
    close(d.value, 5.0, TAU_MODEL, "cylinder gap");
    close(d.points[0].x(), 2.0, TAU_MODEL, "foot on a");
    close(d.points[1].x(), 7.0, TAU_MODEL, "foot on b");

    // Whole bodies: the VALUE is still the closed form, because the certified
    // lateral pair is the minimum over every candidate in the band. But the
    // tier is honestly the mesh one: the two bottom CAPS are coplanar disks
    // whose own closest points lie on their rims, and refining onto a
    // boundary CURVE is not Q1 (the alternating projection cannot move a
    // point that is already on its plane). A pair in the band that cannot be
    // certified means the minimum cannot be certified either.
    let d = distance(&arena, Target::Solid(a), Target::Solid(b)).expect("distance");
    close(d.value, 5.0, TAU_MODEL, "cylinder gap, whole bodies");
    assert!(
        !d.exact,
        "the coplanar cap pair in the band keeps this on the mesh tier: {d:?}"
    );
    assert!(d.chord_bound > 0.0 && d.chord_bound < 0.01, "{d:?}");
}

#[test]
fn a_point_measures_exactly_against_a_plane_a_sphere_and_a_cylinder() {
    let mut arena = BrepArena::new();
    let b = boxx(&mut arena, [0.0, 0.0, 0.0], 1.0, 1.0, 1.0);
    let s = ball(&mut arena, 20.0, 1.0);
    let c = cylinder(&mut arena, -20.0, 0.0, 2.0, 4.0);

    let p = Target::Point(Point3::new(0.5, 0.5, 4.0));
    let d = distance(&arena, p, Target::Solid(b)).expect("point-box");
    assert!(d.exact);
    close(d.value, 3.0, 0.0, "point above a box's top face");
    assert!(
        d.on[0].is_none(),
        "a free point sits on nothing: {:?}",
        d.on
    );

    let p = Target::Point(Point3::new(20.0, 0.0, 7.0));
    let d = distance(&arena, p, Target::Solid(s)).expect("point-ball");
    assert!(d.exact, "{d:?}");
    close(d.value, 6.0, TAU_MODEL, "point above a ball");

    let p = Target::Point(Point3::new(-20.0, 9.0, 2.0));
    let d = distance(&arena, p, Target::Solid(c)).expect("point-cylinder");
    assert!(d.exact, "{d:?}");
    close(d.value, 7.0, TAU_MODEL, "point beside a cylinder lateral");
}

#[test]
fn a_body_against_its_own_translated_copy_is_the_translation_minus_the_extent() {
    let mut arena = BrepArena::new();
    // The §4.4 corpus oracle, on one body: a cylinder of radius 2 (extent 4
    // along x) translated by 11 leaves a gap of 7. Exact, because the
    // closest pair is lateral-to-lateral and refines.
    let a = cylinder(&mut arena, 0.0, 0.0, 2.0, 3.0);
    let b = transform_solid(
        &mut arena,
        a,
        &RigidPlacement {
            translation: [11.0, 0.0, 0.0],
            ..RigidPlacement::IDENTITY
        },
    )
    .expect("translated copy");

    let d = distance(&arena, Target::Solid(a), Target::Solid(b)).expect("distance");
    close(d.value, 7.0, TAU_MODEL, "self-gap after an 11 m shift");
    // Mesh tier for the same reason as the cylinder pair above: the two
    // coplanar caps are a candidate in the band that cannot be certified.
    // The value is still the closed form — the certified lateral pair wins
    // the minimum — and the band is reported.
    assert!(!d.exact, "{d:?}");
    // The band is `rel × extent`: the operand's bounding-box diagonal is
    // √(4² + 4² + 3²) = √41, inscribed by the rim chords, so the band sits
    // just under rel·√41.
    let want = kernel_v2::RENDER_CHORD_TOLERANCE_REL * 41.0f64.sqrt();
    assert!(
        d.chord_bound <= want && d.chord_bound > want * 0.999,
        "band {} vs rel·√41 = {want}",
        d.chord_bound
    );
}

#[test]
fn distance_to_one_face_and_one_edge_names_what_it_landed_on() {
    let mut arena = BrepArena::new();
    let b = boxx(&mut arena, [0.0, 0.0, 0.0], 1.0, 1.0, 1.0);
    // The box's top face (z = 1) is the one whose outward normal is +z.
    let faces: Vec<_> = {
        let s = arena.solid(b).expect("solid");
        s.shells
            .iter()
            .flat_map(|&sh| arena.shell(sh).expect("shell").faces.clone())
            .collect()
    };
    let top = *faces
        .iter()
        .find(|&&f| {
            kernel_v2::face_signature(&arena, f)
                .normal
                .is_some_and(|n| n[2] > 0.9)
        })
        .expect("a +z face");

    let p = Target::Point(Point3::new(0.5, 0.5, 3.0));
    let d = distance(&arena, p, Target::Face(top)).expect("point-face");
    assert!(d.exact);
    close(d.value, 2.0, 0.0, "point above the top face");
    assert_eq!(d.on[1], Some(On::Face(top)));

    // An edge of that face: a point directly above one of its corners is
    // closest to the edge, and the answer says so.
    let edges = arena
        .loop_half_edges(arena.face(top).expect("face").outer_loop)
        .expect("loop");
    let d = distance(
        &arena,
        Target::Point(Point3::new(0.5, 0.0, 3.0)),
        Target::Edge(edges[0]),
    )
    .expect("point-edge");
    assert!(d.exact, "a straight edge is exact: {d:?}");
    assert!(matches!(d.on[1], Some(On::Edge(_))), "{:?}", d.on);
    assert!(d.value >= 2.0, "at least the height: {}", d.value);
}

#[test]
fn along_reports_the_directional_gap_and_a_negative_overlap() {
    let mut arena = BrepArena::new();
    let a = boxx(&mut arena, [0.0, 0.0, 0.0], 1.0, 1.0, 1.0);
    // Offset diagonally: the minimum distance is diagonal, but the gap ALONG
    // x is 2.5 − 1 = 1.5 and along y the two overlap by 1 (negative).
    let b = boxx(&mut arena, [2.5, 0.0, 0.0], 1.0, 1.0, 1.0);

    let d = distance_along(&arena, Target::Solid(a), Target::Solid(b), [1.0, 0.0, 0.0])
        .expect("along x");
    close(d.value, 1.5, 0.0, "gap along x");
    assert!(d.exact, "{d:?}");

    let d = distance_along(&arena, Target::Solid(a), Target::Solid(b), [0.0, 1.0, 0.0])
        .expect("along y");
    close(d.value, -1.0, 0.0, "overlap along y is negative");

    // The sign of the direction does not change the gap.
    let d = distance_along(&arena, Target::Solid(a), Target::Solid(b), [-1.0, 0.0, 0.0])
        .expect("along -x");
    close(d.value, 1.5, 0.0, "gap along −x");

    assert!(
        distance_along(&arena, Target::Solid(a), Target::Solid(b), [0.0, 0.0, 0.0]).is_err(),
        "a zero direction is refused, not defaulted"
    );
}

/// A straight edge against a curved face: the foot ON THE EDGE has a degree of
/// freedom, and an `exact` answer only means something if the refinement used
/// it. The seed foot comes from the cylinder's chord facets, so when the facet
/// nearest the edge STRADDLES the closest generator (the seam is rotated half
/// a facet here so it does) the seed foot sits at a facet corner — a measured
/// 3.26e-4 m away from the truth, which the first Q1 cut reported as `exact`
/// with a zero band.
#[test]
fn a_straight_edge_against_a_cylinder_slides_its_foot_before_claiming_exact() {
    let n = kernel_v2::circle_segment_count(kernel_v2::RENDER_CHORD_TOLERANCE_REL);
    let half = PI / n as f64;
    let mut arena = BrepArena::new();
    // Cylinder R = 2 about +z, seam rotated by half a facet so +x̂ lands in the
    // MIDDLE of a chord facet rather than on a mesh vertex.
    let c = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(half.cos(), half.sin(), 0.0),
        Vector3::new(-half.sin(), half.cos(), 0.0),
        Point2::new(0.0, 0.0),
        2.0,
    )
    .expect("circle profile");
    let cyl = extrude(&mut arena, &c, Vector3::new(0.0, 0.0, 1.0), 4.0)
        .expect("cylinder")
        .solid;
    let lateral = lateral(&arena, cyl);

    // A box out on +x: its x = 12 faces carry edges running along ŷ, whose
    // closest point to the cylinder is at y = 0, INSIDE the edge. The gap is
    // 12 − 2 = 10.
    let b = boxx(&mut arena, [12.0, -3.0, 0.0], 2.0, 6.0, 2.0);
    let mut checked = 0usize;
    for sh in arena.solid(b).expect("solid").shells.clone() {
        for f in arena.shell(sh).expect("shell").faces.clone() {
            let face = arena.face(f).expect("face");
            for h in arena.loop_half_edges(face.outer_loop).expect("loop") {
                let he = arena.half_edge(h).expect("half-edge");
                let p0 = arena.vertex(he.origin).expect("v0").point;
                let p1 = arena
                    .vertex(arena.half_edge(he.next).expect("next").origin)
                    .expect("v1")
                    .point;
                // Only the ŷ-running edges on the x = 12 face have an interior
                // minimum; the rest are endpoint-clamped and were always right.
                let runs_y = (p0.x() - 12.0).abs() < 1e-12
                    && (p1.x() - 12.0).abs() < 1e-12
                    && (p0.y() - p1.y()).abs() > 1.0;
                if !runs_y {
                    continue;
                }
                let d = distance(&arena, Target::Edge(h), Target::Face(lateral)).expect("edge");
                if d.exact {
                    close(
                        d.value,
                        10.0,
                        TAU_MODEL,
                        "an exact edge/cylinder gap is the truth",
                    );
                    assert!(
                        d.points[0].y().abs() <= TAU_MODEL,
                        "the foot slid to the closest generator: {:?}",
                        d.points[0]
                    );
                } else {
                    // Declining is also honest — but then the band must cover
                    // the error, and the value must not be better than it.
                    assert!(
                        (d.value - 10.0).abs() <= d.chord_bound,
                        "a mesh answer must lie inside its own band: {d:?}"
                    );
                }
                checked += 1;
            }
        }
    }
    assert!(
        checked >= 2,
        "only {checked} interior-minimum edges checked"
    );
}

/// The certificate must be reachable by the sweep that feeds it, at any
/// distance from the origin. The first Q1 cut stopped the alternating
/// projection at `TAU_EVAL × scale` but demanded a dimensionless normality
/// sine under `TAU_EVAL`, so the residual the sweep was allowed to leave grew
/// with the model's coordinates while the certificate did not: two unit balls
/// 48 m out measured an exact 5 one way and a mesh 5.00498 the other, a 5 mm
/// answer decided by which operand came first.
#[test]
fn a_sphere_pair_far_from_the_origin_certifies_in_both_directions() {
    let mut arena = BrepArena::new();
    // Centres 8 apart, radii 1 and 2: the gap is 8 − 1 − 2 = 5. Placed far
    // out on x so `scale` is ~48 rather than ~5.
    let a = ball(&mut arena, 40.0, 1.0);
    let b = ball(&mut arena, 48.0, 2.0);

    let fwd = distance(&arena, Target::Solid(a), Target::Solid(b)).expect("a→b");
    let rev = distance(&arena, Target::Solid(b), Target::Solid(a)).expect("b→a");
    close(fwd.value, 5.0, TAU_MODEL, "ball gap far from the origin");
    close(rev.value, fwd.value, 0.0, "symmetric");
    assert!(fwd.exact && rev.exact, "fwd {fwd:?}\nrev {rev:?}");
}
