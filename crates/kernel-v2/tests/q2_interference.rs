//! Q2 oracles — `specs/agent_mechanical_design.md` §4.4 "Interference".
//!
//! The spec's three box configurations, the `interference(A, A) = volume(A)`
//! identity, argument symmetry, and the one thing that matters most: an
//! operand pair whose Intersect the kernel REFUSES comes back as the typed
//! refusal, not as `Disjoint`. A clearance check that reported "they do not
//! touch" when the kernel could not tell would pass a collision.

use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::interference::{interference, ContactEvidence, Interference};
use kernel_v2::{boolean_op, extrude, revolve, BrepArena, KernelV2Error, Profile, SolidId};

/// A `s`-sided cube whose near corner is at `(x0, 0, 0)`.
fn cube(arena: &mut BrepArena, x0: f64, s: f64) -> SolidId {
    let p = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(x0, 0.0),
            Point2::new(x0 + s, 0.0),
            Point2::new(x0 + s, s),
            Point2::new(x0, s),
        ],
        vec![],
    )
    .expect("rectangle profile");
    extrude(arena, &p, Vector3::new(0.0, 0.0, 1.0), s)
        .expect("extrude")
        .solid
}

fn cylinder(arena: &mut BrepArena, z0: f64, r: f64, h: f64) -> SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, z0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        r,
    )
    .expect("circle profile");
    extrude(arena, &p, Vector3::new(0.0, 0.0, 1.0), h)
        .expect("extrude")
        .solid
}

/// The spec's configuration: two 10 mm cubes, the second offset along x.
fn two_cubes(offset: f64) -> (BrepArena, SolidId, SolidId) {
    let mut arena = BrepArena::new();
    let a = cube(&mut arena, 0.0, 0.01);
    let b = cube(&mut arena, offset, 0.01);
    (arena, a, b)
}

#[track_caller]
fn close(got: f64, want: f64, rel: f64, what: &str) {
    let scale = want.abs().max(got.abs()).max(f64::MIN_POSITIVE);
    assert!(
        (got - want).abs() <= rel * scale,
        "{what}: got {got:e}, want {want:e}"
    );
}

// -------------------------------------------------------------------------
// The three outcomes
// -------------------------------------------------------------------------

#[test]
fn overlapping_boxes_interfere_with_the_analytic_overlap_volume() {
    // 10 mm cubes offset 5 mm: the overlap is a 5 × 10 × 10 mm slab.
    let (arena, a, b) = two_cubes(0.005);
    let got = interference(&arena, a, b).expect("the Intersect runs");
    let Interference::Interferes {
        volume,
        bodies,
        exact,
    } = got
    else {
        panic!("want Interferes, got {got:?}");
    };
    assert!(exact, "two planar boxes intersect to a planar region");
    close(volume, 0.005 * 0.01 * 0.01, 1e-13, "overlap volume");
    assert_eq!(bodies.len(), 1, "one lump: {bodies:?}");
    close(
        bodies[0].volume,
        volume,
        1e-15,
        "the lump is the whole region",
    );
    // The lump's centroid and bounds say WHERE to look: the slab spans
    // x ∈ [5, 10] mm, so its centroid is at x = 7.5 mm.
    close(bodies[0].centroid[0], 0.0075, 1e-12, "lump centroid x");
    close(bodies[0].aabb[0][0], 0.005, 1e-12, "lump aabb min x");
    close(bodies[0].aabb[1][0], 0.01, 1e-12, "lump aabb max x");
}

#[test]
fn boxes_sharing_a_face_are_contact_not_interference() {
    let (arena, a, b) = two_cubes(0.01);
    let got = interference(&arena, a, b).expect("the Intersect runs");
    let Interference::Contact { evidence, closest } = got else {
        panic!("want Contact, got {got:?}");
    };
    assert_eq!(
        evidence,
        ContactEvidence::EmptyIntersectionAtZeroDistance,
        "the regularized Intersect is empty and Q1 measures zero"
    );
    assert_eq!(closest.value, 0.0, "the witness gap is exactly zero");
    assert!(closest.exact, "two planar faces touching is an exact zero");
    assert!(
        closest.on[0].is_some() && closest.on[1].is_some(),
        "the witness names what it touches on: {closest:?}"
    );
}

#[test]
fn separated_boxes_are_disjoint_with_q1s_gap() {
    let (arena, a, b) = two_cubes(0.015);
    let got = interference(&arena, a, b).expect("the AABBs reject");
    let Interference::Disjoint { distance } = got else {
        panic!("want Disjoint, got {got:?}");
    };
    close(distance.value, 0.005, 1e-14, "gap");
    assert!(distance.exact, "a planar pair measures exactly");
    // The same number Q1 answers on its own — `Disjoint` reuses Q1, it does
    // not re-derive a gap.
    let q1 = kernel_v2::measure::distance(
        &arena,
        kernel_v2::measure::Target::Solid(a),
        kernel_v2::measure::Target::Solid(b),
    )
    .expect("Q1");
    assert_eq!(distance, q1, "Disjoint carries Q1's own answer");
}

// -------------------------------------------------------------------------
// Identities
// -------------------------------------------------------------------------

#[test]
fn a_body_against_itself_interferes_by_its_whole_volume() {
    // The spec's corpus identity. Two coincident cubes — every face of one is
    // coplanar with a face of the other, so this also exercises Stage 0.
    let (arena, a, b) = two_cubes(0.0);
    let got = interference(&arena, a, b).expect("the Intersect runs");
    let Interference::Interferes { volume, .. } = got else {
        panic!("want Interferes, got {got:?}");
    };
    let own = kernel_v2::mass::mass_properties(&arena, a, 1.0)
        .expect("a measures")
        .volume;
    close(volume, own, 1e-13, "A ∩ A is A");
}

#[test]
fn the_same_identity_holds_for_a_curved_body() {
    let mut arena = BrepArena::new();
    let a = cylinder(&mut arena, 0.0, 0.005, 0.01);
    let b = cylinder(&mut arena, 0.0, 0.005, 0.01);
    let got = interference(&arena, a, b).expect("the Intersect runs");
    let Interference::Interferes { volume, exact, .. } = got else {
        panic!("want Interferes, got {got:?}");
    };
    assert!(exact, "the region is a full cylinder: an exact integration");
    close(volume, PI * 0.005 * 0.005 * 0.01, 1e-13, "A ∩ A is A");
}

#[test]
fn the_answer_does_not_depend_on_the_argument_order() {
    for offset in [0.005, 0.01, 0.015] {
        let (arena, a, b) = two_cubes(offset);
        let ab = interference(&arena, a, b).expect("a, b");
        let ba = interference(&arena, b, a).expect("b, a");
        match (&ab, &ba) {
            (
                Interference::Interferes { volume: va, .. },
                Interference::Interferes { volume: vb, .. },
            ) => close(*va, *vb, 1e-12, &format!("volume at offset {offset}")),
            (
                Interference::Contact { evidence: ea, .. },
                Interference::Contact { evidence: eb, .. },
            ) => {
                assert_eq!(ea, eb, "evidence at offset {offset}")
            }
            (Interference::Disjoint { distance: da }, Interference::Disjoint { distance: db }) => {
                close(
                    da.value,
                    db.value,
                    1e-14,
                    &format!("gap at offset {offset}"),
                )
            }
            _ => panic!("offset {offset}: {ab:?} one way, {ba:?} the other"),
        }
        // The witness points swap with the operands, so only the kind and the
        // magnitude are order-free — which is what a caller reads.
    }
}

// -------------------------------------------------------------------------
// A refused boolean is a refusal
// -------------------------------------------------------------------------

#[test]
fn a_boolean_the_kernel_cannot_run_declines_by_name_and_is_not_disjoint() {
    // A half-cut sphere: its surviving face is a boolean-output sphere patch,
    // which cannot re-enter yang Stage 1 (a declared capability boundary, not
    // a bug). The two bodies plainly overlap, so the one answer this must NOT
    // give is `Disjoint`.
    let mut arena = BrepArena::new();
    let p = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        0.005,
    )
    .expect("on-axis circle");
    let sphere = revolve(
        &mut arena,
        &p,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("sphere revolve")
    .solid;
    let knife = cube(&mut arena, -0.01, 0.02);
    let half = boolean_op(&mut arena, sphere, knife, BoolOp::Subtract).expect("half cut");
    // A small cube well inside the surviving half.
    let probe = {
        let q = Profile::new(
            Point3::new(0.0, 0.0, -0.006),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            vec![
                Point2::new(-0.002, -0.002),
                Point2::new(0.002, -0.002),
                Point2::new(0.002, 0.002),
                Point2::new(-0.002, 0.002),
            ],
            vec![],
        )
        .expect("probe profile");
        extrude(&mut arena, &q, Vector3::new(0.0, 0.0, 1.0), 0.004)
            .expect("probe extrude")
            .solid
    };

    match interference(&arena, half, probe) {
        Err(KernelV2Error::UnsupportedCurvedBoolean { reason, .. }) => {
            assert!(
                reason.contains("sphere patch"),
                "the refusal names the wall: {reason}"
            );
        }
        other => panic!("want the typed curved-patch refusal, got {other:?}"),
    }
}
