//! B6 increment S1 (spec `specs/b6_general_sweep.md`): the `SweepPath`
//! contract — mitre geometry, parallel transport, and every typed refusal.
//!
//! Everything here is a PURE value test: S1 builds no geometry, so the
//! oracles are the identities the assembler will rely on at S2.

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::construct::{section_support, SweepPath, SweepSegmentKind};
use kernel_v2::{KernelV2Error, Profile};
use kernel_v2::{ProfileEdge, ProfileRegion};
use std::f64::consts::FRAC_PI_2;
use waffle_types::sketch3d::{Chain3d, Edge3d, Edge3dKind};

// ---------------------------------------------------------------------------
// builders
// ---------------------------------------------------------------------------

fn line(a: [f64; 3], b: [f64; 3]) -> Edge3d {
    Edge3d {
        entity_id: 0,
        kind: Edge3dKind::Line,
        a,
        b,
    }
}

fn arc(a: [f64; 3], b: [f64; 3], center: [f64; 3], normal: [f64; 3], radius: f64) -> Edge3d {
    Edge3d {
        entity_id: 0,
        kind: Edge3dKind::Arc {
            center,
            normal,
            radius,
        },
        a,
        b,
    }
}

/// A chain with DELIBERATELY unset `g1` flags: the sweep classifies joints
/// itself, and `chain_g1_flags_are_advisory` pins that.
fn chain(edges: Vec<Edge3d>) -> Chain3d {
    let g1 = vec![false; edges.len().saturating_sub(1)];
    Chain3d {
        edges,
        closed: false,
        g1,
    }
}

/// A `half`-wide square section in the plane through `origin` spanned by
/// `u`/`v`, centred on the section origin.
fn square(origin: [f64; 3], u: [f64; 3], v: [f64; 3], half: f64) -> Profile {
    Profile::new(
        Point3::new(origin[0], origin[1], origin[2]),
        Vector3::new(u[0], u[1], u[2]),
        Vector3::new(v[0], v[1], v[2]),
        vec![
            Point2::new(-half, -half),
            Point2::new(half, -half),
            Point2::new(half, half),
            Point2::new(-half, half),
        ],
        vec![],
    )
    .expect("square section")
}

/// The canonical starting section: the path leaves the origin along +x, so
/// the section plane is spanned by ŷ and ẑ.
fn yz_square(half: f64) -> Profile {
    square([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], half)
}

/// A square section pierced by a chain at its own start, whichever way the
/// chain walker happened to orient it.
fn section_at_start(c: &Chain3d, half: f64) -> Profile {
    let a = c.edges[0].a;
    let t = c.edges[0].start_tangent();
    let seed = if t[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let u = unit(cross(t, seed));
    let v = cross(t, u); // u × v = t
    square(a, u, v, half)
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn near_pt(a: Point3, b: [f64; 3], tol: f64) -> bool {
    near(a.x(), b[0], tol) && near(a.y(), b[1], tol) && near(a.z(), b[2], tol)
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

// ---------------------------------------------------------------------------
// the straight case
// ---------------------------------------------------------------------------

#[test]
fn a_single_line_is_two_capped_stations_with_no_shear() {
    let path = SweepPath::new(
        &chain(vec![line([0.0; 3], [10.0, 0.0, 0.0])]),
        &yz_square(0.5),
    )
    .expect("straight sweep");

    assert_eq!(path.segments().len(), 1);
    assert_eq!(path.stations().len(), 2);
    assert!(near(path.length(), 10.0, 0.0));

    // Both ends are caps: the plane perpendicular to the tangent, which IS
    // the degenerate mitre (spec §4).
    for s in path.stations() {
        assert!(s.g1);
        assert!(near_pt(
            Point3::new(s.normal.x, s.normal.y, s.normal.z),
            [1.0, 0.0, 0.0],
            0.0
        ));
    }

    // The section is used exactly as drawn — bit-exact, not within a band.
    let p = Point2::new(0.5, 0.5);
    assert_eq!(path.stations()[0].rim(p), Point3::new(0.0, 0.5, 0.5));
    assert_eq!(path.stations()[1].rim(p), Point3::new(10.0, 0.5, 0.5));
}

#[test]
fn the_sections_own_offset_travels_with_it() {
    // A member whose centreline is NOT the section's centre: the section is
    // drawn 2 up in v. Spec §8 — nothing re-centres it.
    let section = square([0.0, 0.0, 2.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    let path = SweepPath::new(&chain(vec![line([0.0; 3], [10.0, 0.0, 0.0])]), &section)
        .expect("offset sweep");
    assert_eq!(path.stations()[0].offset, [0.0, 2.0]);
    assert_eq!(
        path.stations()[1].rim(Point2::new(0.0, 0.0)),
        Point3::new(10.0, 0.0, 2.0)
    );
}

// ---------------------------------------------------------------------------
// mitre geometry (spec §4)
// ---------------------------------------------------------------------------

/// The path of a right-angle elbow: in along +x, out along +y.
fn right_angle() -> Chain3d {
    chain(vec![
        line([-10.0, 0.0, 0.0], [0.0; 3]),
        line([0.0; 3], [0.0, 10.0, 0.0]),
    ])
}

#[test]
fn the_mitre_plane_is_perpendicular_to_the_average_tangent() {
    let section = square([-10.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    let path = SweepPath::new(&right_angle(), &section).expect("elbow");
    let joint = path.stations()[1];

    assert!(!joint.g1);
    let r = 0.5f64.sqrt();
    assert!(near(joint.normal.x, r, 1e-15));
    assert!(near(joint.normal.y, r, 1e-15));
    assert!(near(joint.normal.z, 0.0, 1e-15));

    // A point on the INSIDE of the turn gets the short rail, one on the
    // outside the long one — the picture-frame mitre. (The difference
    // formula the spec's §4 parenthesis gives would put the plane on the
    // other diagonal and swap these.)
    let inside = joint.rim(Point2::new(0.5, 0.0));
    let outside = joint.rim(Point2::new(-0.5, 0.0));
    assert!(near_pt(inside, [-0.5, 0.5, 0.0], 1e-15));
    assert!(near_pt(outside, [0.5, -0.5, 0.0], 1e-15));
}

#[test]
fn the_mitre_rim_is_the_same_from_both_sides() {
    // The load-bearing identity of spec §3: the shear onto the mitre plane
    // from the incoming frame and from the outgoing (parallel-transported)
    // frame land on the SAME point, so the rim is shared rather than
    // duplicated. Checked over in-plane and out-of-plane corners at many
    // turn angles.
    for turn_deg in [5.0f64, 30.0, 60.0, 89.0, 120.0, 170.0] {
        for tilt_deg in [0.0f64, 17.0, 90.0] {
            let turn = turn_deg.to_radians();
            let tilt = tilt_deg.to_radians();
            // Outgoing direction: turn by `turn` in a plane tilted by
            // `tilt` about the incoming tangent.
            let out = unit([turn.cos(), turn.sin() * tilt.cos(), turn.sin() * tilt.sin()]);
            let c = chain(vec![
                line([-10.0, 0.0, 0.0], [0.0; 3]),
                line([0.0; 3], [out[0] * 10.0, out[1] * 10.0, out[2] * 10.0]),
            ]);
            let section = square([-10.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 0.5);
            let path = SweepPath::new(&c, &section).expect("mitre");
            let joint = path.stations()[1];
            let (fi, fo) = (joint.frame_in.unwrap(), joint.frame_out.unwrap());
            for p in [(0.5, 0.5), (-0.5, 0.5), (0.25, -0.5), (0.0, 0.0)] {
                let p = Point2::new(p.0, p.1);
                let a = joint.rim_via(&fi, p);
                let b = joint.rim_via(&fo, p);
                assert!(
                    near_pt(a, [b.x(), b.y(), b.z()], 1e-13),
                    "turn {turn_deg} tilt {tilt_deg}: {a:?} vs {b:?}"
                );
            }
        }
    }
}

#[test]
fn a_g1_joint_takes_the_degenerate_mitre() {
    // Line into a tangent quarter-turn: the joint's cut plane is the plane
    // perpendicular to the common tangent, and the section is used exactly
    // as drawn — the continuity that makes a pipe a special case of a sweep
    // (spec §9).
    let c = chain(vec![
        line([-10.0, 0.0, 0.0], [0.0; 3]),
        arc(
            [0.0; 3],
            [4.0, 4.0, 0.0],
            [0.0, 4.0, 0.0],
            [0.0, 0.0, 1.0],
            4.0,
        ),
    ]);
    let section = square([-10.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    let path = SweepPath::new(&c, &section).expect("tangent bend");
    let joint = path.stations()[1];
    assert!(joint.g1);
    assert!(near(joint.normal.x, 1.0, 1e-15));
    // No shear at all: exact equality, not a band.
    assert_eq!(
        joint.rim(Point2::new(0.5, -0.25)),
        Point3::new(0.0, 0.5, -0.25)
    );
}

#[test]
fn a_mitre_at_a_curved_joint_is_refused() {
    // The correction to spec §4 (module docs, correction 2): a plane cuts a
    // bent member in a different curve than it cuts a straight one, so
    // there is no shared rim to build.
    //
    // A bend whose centre is NOT on the incoming normal: radius 4 about
    // (2, 2√3), so the arc leaves the origin 30° below +x — a genuine
    // non-tangent, non-reversing joint at an arc.
    let s3 = 3.0f64.sqrt();
    let c = chain(vec![
        line([-10.0, 0.0, 0.0], [0.0; 3]),
        arc(
            [0.0; 3],
            [4.0, 0.0, 0.0],
            [2.0, 2.0 * s3, 0.0],
            [0.0, 0.0, 1.0],
            4.0,
        ),
    ]);
    let section = square([-10.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    assert_eq!(
        SweepPath::new(&c, &section).unwrap_err(),
        KernelV2Error::SweepMitreAtCurvedJoint { joint: 1 }
    );
}

#[test]
fn a_doubling_back_joint_is_refused() {
    let c = chain(vec![
        line([0.0; 3], [10.0, 0.0, 0.0]),
        line([10.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
    ]);
    assert_eq!(
        SweepPath::new(&c, &yz_square(0.5)).unwrap_err(),
        KernelV2Error::SweepCornerReversal { joint: 1 }
    );
}

// ---------------------------------------------------------------------------
// the corner gates
// ---------------------------------------------------------------------------

/// A zig-zag whose middle segment of length `l` is mitred at both ends,
/// with both mitres cutting into the SAME side — the shape that runs out of
/// material first.
fn zigzag(l: f64) -> Chain3d {
    chain(vec![
        line([-10.0, 0.0, 0.0], [0.0; 3]),
        line([0.0; 3], [0.0, l, 0.0]),
        line([0.0, l, 0.0], [-10.0, l, 0.0]),
    ])
}

#[test]
fn a_corner_tighter_than_the_section_is_refused() {
    let section = square([-10.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    // Each 90° mitre eats `half` into the middle segment on the same side,
    // so a 1.0-wide section needs more than 1.0 of run.
    assert_eq!(
        SweepPath::new(&zigzag(0.9), &section).unwrap_err(),
        KernelV2Error::SweepCornerTooTight { segment: 1 }
    );
    assert_eq!(
        SweepPath::new(&zigzag(1.0), &section).unwrap_err(),
        KernelV2Error::SweepCornerTooTight { segment: 1 }
    );
    // Just past the exact limit the corner is legal, and the gate does not
    // pad it away.
    assert!(SweepPath::new(&zigzag(1.0 + 1e-6), &section).is_ok());
    assert!(SweepPath::new(&zigzag(4.0), &section).is_ok());
}

#[test]
fn a_thinner_section_survives_the_same_corner() {
    let thin = square([-10.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 0.2);
    assert!(SweepPath::new(&zigzag(0.9), &thin).is_ok());
}

#[test]
fn a_section_that_reaches_the_bend_axis_is_refused() {
    // Bend of radius 1 about the axis at y = 1: a section reaching 1.5 out
    // in +u crosses it, one reaching 0.5 does not.
    let bend = chain(vec![arc(
        [0.0; 3],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        1.0,
    )]);
    let fat = square([0.0; 3], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 1.5);
    assert_eq!(
        SweepPath::new(&bend, &fat).unwrap_err(),
        KernelV2Error::SweepSectionCrossesBendAxis { segment: 0 }
    );
    let slim = square([0.0; 3], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    assert!(SweepPath::new(&bend, &slim).is_ok());
    // Exactly touching the axis pinches a seam, and is refused too.
    let touching = square([0.0; 3], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 1.0);
    assert_eq!(
        SweepPath::new(&bend, &touching).unwrap_err(),
        KernelV2Error::SweepSectionCrossesBendAxis { segment: 0 }
    );
}

// ---------------------------------------------------------------------------
// the pierce rule (spec §8)
// ---------------------------------------------------------------------------

#[test]
fn a_section_not_perpendicular_to_the_start_tangent_is_refused() {
    let tilted = square([0.0; 3], unit([0.2, 1.0, 0.0]), [0.0, 0.0, 1.0], 0.5);
    assert_eq!(
        SweepPath::new(&chain(vec![line([0.0; 3], [10.0, 0.0, 0.0])]), &tilted).unwrap_err(),
        KernelV2Error::SweepProfileNotPerpendicular
    );
}

#[test]
fn a_path_that_misses_the_section_plane_is_refused() {
    // Right plane orientation, wrong plane: the start is 1 m off it.
    let offset_plane = square([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    assert_eq!(
        SweepPath::new(
            &chain(vec![line([0.0; 3], [10.0, 0.0, 0.0])]),
            &offset_plane
        )
        .unwrap_err(),
        KernelV2Error::SweepPathDoesNotPierceProfile
    );
}

#[test]
fn a_skewed_section_basis_is_refused() {
    let skew = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(0.0, 0.3, 0.95),
        vec![
            Point2::new(-0.5, -0.5),
            Point2::new(0.5, -0.5),
            Point2::new(0.5, 0.5),
            Point2::new(-0.5, 0.5),
        ],
        vec![],
    )
    .expect("skewed profile is still a profile");
    assert_eq!(
        SweepPath::new(&chain(vec![line([0.0; 3], [10.0, 0.0, 0.0])]), &skew).unwrap_err(),
        KernelV2Error::SweepSectionBasisNotOrthonormal
    );
}

// ---------------------------------------------------------------------------
// chain shape refusals
// ---------------------------------------------------------------------------

#[test]
fn malformed_chains_are_refused_typed() {
    let s = yz_square(0.5);
    assert_eq!(
        SweepPath::new(&chain(vec![]), &s).unwrap_err(),
        KernelV2Error::SweepPathEmpty
    );
    let mut closed = chain(vec![
        line([0.0; 3], [10.0, 0.0, 0.0]),
        line([10.0, 0.0, 0.0], [10.0, 10.0, 0.0]),
        line([10.0, 10.0, 0.0], [0.0; 3]),
    ]);
    closed.closed = true;
    assert_eq!(
        SweepPath::new(&closed, &s).unwrap_err(),
        KernelV2Error::SweepClosedPathUnsupported
    );
    // ...and a ring that forgot to say so is still a ring.
    closed.closed = false;
    assert_eq!(
        SweepPath::new(&closed, &s).unwrap_err(),
        KernelV2Error::SweepClosedPathUnsupported
    );
    let gapped = chain(vec![
        line([0.0; 3], [10.0, 0.0, 0.0]),
        line([10.0, 0.1, 0.0], [20.0, 0.1, 0.0]),
    ]);
    assert_eq!(
        SweepPath::new(&gapped, &s).unwrap_err(),
        KernelV2Error::SweepPathNotChained { segment: 0 }
    );
    let degenerate = chain(vec![line([0.0; 3], [0.0; 3])]);
    assert_eq!(
        SweepPath::new(&degenerate, &s).unwrap_err(),
        KernelV2Error::SweepPathEdgeInvalid { segment: 0 }
    );
    let off_circle = chain(vec![arc(
        [0.0; 3],
        [1.0, 1.5, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        1.0,
    )]);
    assert_eq!(
        SweepPath::new(&off_circle, &s).unwrap_err(),
        KernelV2Error::SweepPathEdgeInvalid { segment: 0 }
    );
}

// ---------------------------------------------------------------------------
// the station's plane and its rim are the same plane
// ---------------------------------------------------------------------------

#[test]
fn every_rim_point_lies_on_its_own_stations_cut_plane() {
    // What the assembler will build the cap and the rim loop from: the plane
    // the station reports and the points `rim` returns must not be two
    // slightly different planes. A mixed path — mitre, straight run, tangent
    // bend — exercises both the sheared and unsheared branches.
    let c = chain(vec![
        line([-10.0, 0.0, 0.0], [0.0; 3]),
        line([0.0; 3], [0.0, 6.0, 0.0]),
        // A right-hand bend: traversed clockwise about +z, i.e. CCW about −z.
        arc(
            [0.0, 6.0, 0.0],
            [2.0, 8.0, 0.0],
            [2.0, 6.0, 0.0],
            [0.0, 0.0, -1.0],
            2.0,
        ),
        line([2.0, 8.0, 0.0], [9.0, 8.0, 0.0]),
    ]);
    let section = square([-10.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    let path = SweepPath::new(&c, &section).expect("mixed path");
    assert_eq!(path.stations().len(), 5);
    for (i, s) in path.stations().iter().enumerate() {
        for p in [(0.5, 0.5), (-0.5, 0.5), (0.5, -0.5), (-0.5, -0.5)] {
            let r = s.rim(Point2::new(p.0, p.1));
            let d = dot(
                [
                    r.x() - s.point.x(),
                    r.y() - s.point.y(),
                    r.z() - s.point.z(),
                ],
                [s.normal.x, s.normal.y, s.normal.z],
            );
            assert!(near(d, 0.0, 1e-15), "station {i}: rim off plane by {d}");
        }
    }
}

// ---------------------------------------------------------------------------
// parallel transport (spec §6)
// ---------------------------------------------------------------------------

#[test]
fn a_joint_turns_the_frame_by_the_minimal_rotation() {
    // The defining property of a rotation-minimizing frame across a joint:
    // the component along the turn's own axis is unchanged, and the
    // in-plane part turns by exactly the turn angle — no roll about the
    // tangent is introduced.
    let out = unit([1.0, 1.0, 0.7]);
    let c = chain(vec![
        line([-10.0, 0.0, 0.0], [0.0; 3]),
        line([0.0; 3], [out[0] * 5.0, out[1] * 5.0, out[2] * 5.0]),
    ]);
    let section = square([-10.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    let path = SweepPath::new(&c, &section).expect("3D corner");
    let j = path.stations()[1];
    let (fi, fo) = (j.frame_in.unwrap(), j.frame_out.unwrap());
    let (ti, to) = (
        [fi.tangent.x, fi.tangent.y, fi.tangent.z],
        [fo.tangent.x, fo.tangent.y, fo.tangent.z],
    );
    let m = unit(cross(ti, to));
    let xi = [fi.x.x, fi.x.y, fi.x.z];
    let xo = [fo.x.x, fo.x.y, fo.x.z];
    assert!(near(dot(xi, m), dot(xo, m), 1e-14));

    // The in-plane parts subtend the turn angle.
    let turn = dot(ti, to).clamp(-1.0, 1.0).acos();
    let pi = [
        xi[0] - m[0] * dot(xi, m),
        xi[1] - m[1] * dot(xi, m),
        xi[2] - m[2] * dot(xi, m),
    ];
    let po = [
        xo[0] - m[0] * dot(xo, m),
        xo[1] - m[1] * dot(xo, m),
        xo[2] - m[2] * dot(xo, m),
    ];
    let angle = (dot(pi, po) / (dot(pi, pi).sqrt() * dot(po, po).sqrt()))
        .clamp(-1.0, 1.0)
        .acos();
    assert!(near(angle, turn, 1e-9), "{angle} vs {turn}");
}

#[test]
fn an_arc_rotates_the_frame_rigidly_about_its_own_axis() {
    // For a planar curve the rotation-minimizing frame IS the rotation
    // about the curve's plane normal — which is what keeps an arc segment
    // a partial revolve (spec §1).
    let c = chain(vec![arc(
        [0.0; 3],
        [4.0, 4.0, 0.0],
        [0.0, 4.0, 0.0],
        [0.0, 0.0, 1.0],
        4.0,
    )]);
    let path = SweepPath::new(&c, &yz_square(0.5)).expect("quarter bend");
    let start = path.stations()[0].frame_out.unwrap();
    let end = path.stations()[1].frame_in.unwrap();
    // Start frame: x = +y, y = +z. After a +90° turn about +z: x = −x.
    assert!(near(start.x.y, 1.0, 1e-15));
    assert!(near(end.x.x, -1.0, 1e-9));
    assert!(near(end.y.z, 1.0, 1e-15));
    assert!(near(end.tangent.y, 1.0, 1e-9));
    if let SweepSegmentKind::Arc { sweep, radius, .. } = path.segments()[0].kind {
        assert!(near(sweep, FRAC_PI_2, 1e-12));
        assert!(near(radius, 4.0, 0.0));
        assert!(near(path.length(), 4.0 * FRAC_PI_2, 1e-12));
    } else {
        panic!("expected an arc segment");
    }
}

#[test]
fn a_planar_path_has_no_twist() {
    // Spec §6: on a planar path the frame is constant — the section is used
    // exactly as drawn at every station, with nothing to decide.
    let c = chain(vec![
        line([-10.0, 0.0, 0.0], [-4.0, 0.0, 0.0]),
        arc(
            [-4.0, 0.0, 0.0],
            [0.0, 4.0, 0.0],
            [-4.0, 4.0, 0.0],
            [0.0, 0.0, 1.0],
            4.0,
        ),
        line([0.0, 4.0, 0.0], [0.0, 12.0, 0.0]),
    ]);
    let section = square([-10.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    let path = SweepPath::new(&c, &section).expect("planar path");
    let n = [0.0, 0.0, 1.0]; // the path plane's normal
    for s in path.stations() {
        for f in [s.frame_in, s.frame_out].into_iter().flatten() {
            // Out-of-plane component of the section basis is invariant: x
            // stays in the path plane, y stays along its normal.
            assert!(near(dot([f.x.x, f.x.y, f.x.z], n), 0.0, 1e-15));
            assert!(near(dot([f.y.x, f.y.y, f.y.z], n).abs(), 1.0, 1e-15));
        }
        assert!(s.g1, "every joint of a tangent path is smooth");
    }
    assert!(near(path.length(), 6.0 + 4.0 * FRAC_PI_2 + 8.0, 1e-12));
}

// ---------------------------------------------------------------------------
// contracts
// ---------------------------------------------------------------------------

#[test]
fn chain_g1_flags_are_advisory() {
    // The sweep classifies joints from the tangents it derives, so a chain
    // that mislabels a square corner as smooth cannot smuggle one through.
    let mut lying = right_angle();
    lying.g1 = vec![true];
    let section = square([-10.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    let honest = SweepPath::new(&right_angle(), &section).expect("elbow");
    let from_lie = SweepPath::new(&lying, &section).expect("elbow");
    assert_eq!(honest, from_lie);
    assert!(!from_lie.stations()[1].g1);
}

#[test]
fn construction_is_deterministic() {
    let section = square([-10.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    let a = SweepPath::new(&right_angle(), &section).expect("elbow");
    let b = SweepPath::new(&right_angle(), &section).expect("elbow");
    assert_eq!(a, b);
}

#[test]
fn the_length_is_the_chains_own() {
    let c = chain(vec![
        line([-10.0, 0.0, 0.0], [-4.0, 0.0, 0.0]),
        arc(
            [-4.0, 0.0, 0.0],
            [0.0, 4.0, 0.0],
            [-4.0, 4.0, 0.0],
            [0.0, 0.0, 1.0],
            4.0,
        ),
    ]);
    let section = square([-10.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    let path = SweepPath::new(&c, &section).expect("path");
    assert!(near(path.length(), c.length(), 1e-12));
}

// ---------------------------------------------------------------------------
// the support function the gates are decided by
// ---------------------------------------------------------------------------

#[test]
fn section_support_is_exact_for_every_region_kind() {
    let poly = ProfileRegion::Polygon {
        outer: vec![
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(2.0, 1.0),
            Point2::new(0.0, 1.0),
        ],
        holes: vec![],
    };
    assert!(near(section_support(&poly, [1.0, 0.0]), 2.0, 0.0));
    assert!(near(section_support(&poly, [-1.0, -1.0]), 0.0, 0.0));

    let circle = ProfileRegion::Circle {
        center: Point2::new(1.0, 0.0),
        radius: 2.0,
    };
    assert!(near(section_support(&circle, [1.0, 0.0]), 3.0, 1e-15));
    assert!(near(section_support(&circle, [0.0, 1.0]), 2.0, 1e-15));
    // A non-unit direction scales the whole functional, radius included.
    assert!(near(section_support(&circle, [2.0, 0.0]), 6.0, 1e-15));

    // A quarter-round corner: the support along the corner's diagonal is
    // the arc's own extreme point, which is NOT one of the edge vertices —
    // the case a vertex-only gate would get wrong.
    let arc_poly = ProfileRegion::ArcPolygon {
        outer: vec![
            ProfileEdge::Arc {
                a: Point2::new(1.0, 0.0),
                b: Point2::new(0.0, 1.0),
                center: Point2::new(0.0, 0.0),
                radius: 1.0,
                ccw: true,
            },
            ProfileEdge::Line {
                a: Point2::new(0.0, 1.0),
                b: Point2::new(0.0, 0.0),
            },
            ProfileEdge::Line {
                a: Point2::new(0.0, 0.0),
                b: Point2::new(1.0, 0.0),
            },
        ],
        holes: vec![],
    };
    let diag = 0.5f64.sqrt();
    assert!(near(section_support(&arc_poly, [diag, diag]), 1.0, 1e-15));
    // ...and in −u the arc contributes nothing beyond its endpoints.
    assert!(near(section_support(&arc_poly, [-1.0, 0.0]), 0.0, 1e-15));

    // An antipodal arc edge is outside `Profile`'s minor-arc contract: it
    // cannot say which way it bulges, so the support fails CLOSED.
    let ambiguous = ProfileRegion::ArcPolygon {
        outer: vec![
            ProfileEdge::Arc {
                a: Point2::new(-1.0, 0.0),
                b: Point2::new(1.0, 0.0),
                center: Point2::new(0.0, 0.0),
                radius: 1.0,
                ccw: false,
            },
            ProfileEdge::Line {
                a: Point2::new(1.0, 0.0),
                b: Point2::new(-1.0, 0.0),
            },
        ],
        holes: vec![],
    };
    assert_eq!(section_support(&ambiguous, [0.0, 1.0]), f64::INFINITY);
}

// ---------------------------------------------------------------------------
// the path a user actually draws
// ---------------------------------------------------------------------------

#[test]
fn a_filleted_sketch_corner_sweeps_without_a_mitre() {
    // The reason correction 2 costs nothing: a bend is authored as a sketch
    // fillet, and a fillet is tangent by construction, so the arc's joints
    // are G1 and never reach the curved-mitre refusal.
    use waffle_types::sketch3d::{NoAnchors, Sketch3d, Sketch3dEntity};

    let sketch = Sketch3d {
        id: uuid::Uuid::nil(),
        entities: vec![
            Sketch3dEntity::Point {
                id: 1,
                xyz: [-10.0, 0.0, 0.0],
                attach: None,
                xyz_expr: None,
                construction: false,
            },
            Sketch3dEntity::Point {
                id: 2,
                xyz: [0.0, 0.0, 0.0],
                attach: None,
                xyz_expr: None,
                construction: false,
            },
            Sketch3dEntity::Point {
                id: 3,
                xyz: [0.0, 10.0, 0.0],
                attach: None,
                xyz_expr: None,
                construction: false,
            },
            Sketch3dEntity::Line {
                id: 4,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            Sketch3dEntity::Line {
                id: 5,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            Sketch3dEntity::Fillet {
                id: 6,
                at_point_id: 2,
                radius: 2.0,
                radius_expr: None,
            },
        ],
    };
    let evaluation = sketch.evaluate(&NoAnchors).expect("evaluates");
    assert_eq!(evaluation.chains.len(), 1);
    let c = &evaluation.chains[0];
    assert_eq!(c.edges.len(), 3, "line, fillet arc, line");

    // The section is built off the chain's own start, so the test does not
    // depend on which end the walker started from.
    let section = section_at_start(c, 0.5);
    let path = SweepPath::new(c, &section).expect("filleted corner sweeps");
    assert!(path.stations().iter().all(|s| s.g1));
    assert!(near(path.length(), c.length(), 1e-12));
    // The turn is a quarter circle of radius 2, so the whole run is
    // 8 + 8 + 2·(π/2): the two 2 m setbacks come off the legs.
    assert!(near(path.length(), 8.0 + 8.0 + 2.0 * FRAC_PI_2, 1e-12));
    // Every station's rim is the section as drawn: a tangent path never
    // shears (the pipe-continuity property of spec §9).
    for s in path.stations() {
        let p = Point2::new(0.5, -0.25);
        let expected = [
            s.point.x() + 0.5 * s.frame.x.x - 0.25 * s.frame.y.x,
            s.point.y() + 0.5 * s.frame.x.y - 0.25 * s.frame.y.y,
            s.point.z() + 0.5 * s.frame.x.z - 0.25 * s.frame.y.z,
        ];
        assert_eq!(s.rim(p), Point3::new(expected[0], expected[1], expected[2]));
    }
}
