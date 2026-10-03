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

/// A `side`-cube with its near corner at `at`.
fn corner_cube(arena: &mut BrepArena, at: [f64; 3], side: f64) -> SolidId {
    let p = Profile::new(
        Point3::new(0.0, 0.0, at[2]),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(at[0], at[1]),
            Point2::new(at[0] + side, at[1]),
            Point2::new(at[0] + side, at[1] + side),
            Point2::new(at[0], at[1] + side),
        ],
        vec![],
    )
    .expect("cube profile");
    extrude(arena, &p, Vector3::new(0.0, 0.0, 1.0), side)
        .expect("extrude")
        .solid
}

/// A 1 m cube with its near corner at `(x0, 0, 0)` — the scale at which a
/// thin overlap is still far above the `MIN_FEATURE_SIZE³` volume floor.
fn unit_cube(arena: &mut BrepArena, x0: f64) -> SolidId {
    corner_cube(arena, [x0, 0.0, 0.0], 1.0)
}

/// A `side` × `side` × 10 mm plate with its near corner at `(x0, 0, 0)`.
fn plate(arena: &mut BrepArena, x0: f64, side: f64) -> SolidId {
    let p = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(x0, 0.0),
            Point2::new(x0 + side, 0.0),
            Point2::new(x0 + side, side),
            Point2::new(x0, side),
        ],
        vec![],
    )
    .expect("plate profile");
    extrude(arena, &p, Vector3::new(0.0, 0.0, 1.0), 0.01)
        .expect("extrude")
        .solid
}

/// A drilling tool through a 10 mm plate, protruding both ways so neither cap
/// is coplanar with the plate's (the Stage-0 wall is not what these cases are
/// about).
fn bore(arena: &mut BrepArena, cx: f64, cy: f64, r: f64) -> SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, -0.005),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(cx, cy),
        r,
    )
    .expect("circle profile");
    extrude(arena, &p, Vector3::new(0.0, 0.0, 1.0), 0.02)
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
        chord_bound,
    } = got
    else {
        panic!("want Interferes, got {got:?}");
    };
    assert!(exact, "two planar boxes intersect to a planar region");
    assert_eq!(chord_bound, 0.0, "an exact answer carries no band");
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
    let Interference::Interferes {
        volume,
        exact,
        chord_bound,
        ..
    } = got
    else {
        panic!("want Interferes, got {got:?}");
    };
    assert!(exact, "the region is a full cylinder: an exact integration");
    assert_eq!(chord_bound, 0.0, "an exact answer carries no band");
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

// -------------------------------------------------------------------------
// Review additions (2026-10-03)
// -------------------------------------------------------------------------

/// The query must be the SAME boolean the user's own Intersect would run, on
/// operands that are themselves boolean output — so the scratch-arena copy is
/// shown to carry everything `to_yang_brep` reads: the bore's `reversed`
/// cylinder surface, its circle rim curves, face orientation and loop order.
///
/// The region here CONTAINS the bore, so the copied cylinder is not merely
/// along for the ride: it has to survive the pipeline and come back out as an
/// exact band.
#[test]
fn a_query_on_prior_boolean_operands_equals_the_live_intersect() {
    let mut arena = BrepArena::new();
    // A: a 20 mm plate with a 6 mm through bore at x = 15 mm.
    let plate_a = plate(&mut arena, 0.0, 0.02);
    let tool_a = bore(&mut arena, 0.015, 0.010, 0.003);
    let a = boolean_op(&mut arena, plate_a, tool_a, BoolOp::Subtract).expect("A's bore");
    // B: the same plate shifted 10 mm in x, its own bore outside the overlap.
    let plate_b = plate(&mut arena, 0.010, 0.02);
    let tool_b = bore(&mut arena, 0.025, 0.010, 0.003);
    let b = boolean_op(&mut arena, plate_b, tool_b, BoolOp::Subtract).expect("B's bore");

    let query = interference(&arena, a, b).expect("the query runs");
    let Interference::Interferes {
        volume,
        bodies,
        exact,
        ..
    } = &query
    else {
        panic!("want Interferes, got {query:?}");
    };
    assert!(*exact, "the region's bore wall is a full cylinder band");

    // The overlap is the 10 × 20 × 10 mm slab, less A's bore, which falls
    // inside it. An independent closed form, not just agreement.
    let want = 0.01 * 0.02 * 0.01 - PI * 0.003 * 0.003 * 0.01;
    close(*volume, want, 1e-13, "the closed-form overlap volume");

    // …and the same Intersect run as a real feature on the LIVE arena gives
    // the identical numbers. This is the parity the scratch copy has to earn.
    let live = boolean_op(&mut arena, a, b, BoolOp::Intersect).expect("the live Intersect");
    let lumps = kernel_v2::split_solid_into_bodies(&mut arena, live).expect("split");
    assert_eq!(lumps.len(), bodies.len(), "the same lump count");
    let mut live_total = 0.0;
    for &lump in &lumps {
        let m = kernel_v2::mass::mass_properties(&arena, lump, 1.0).expect("the lump integrates");
        assert!(m.exact, "the live region is exact too");
        live_total += m.volume;
    }
    assert_eq!(
        *volume, live_total,
        "the query's volume is the live Intersect's, to the bit"
    );
    for (i, lump) in lumps.iter().enumerate() {
        let m = kernel_v2::mass::mass_properties(&arena, *lump, 1.0).expect("mass");
        assert_eq!(bodies[i].centroid, m.centroid, "lump {i} centroid");
    }
}

/// A thin but real overlap is an overlap, reported with its volume — and
/// below the kernel's own resolution floor the pipeline STOPs by name rather
/// than calling it contact.
///
/// This is where the `SliverIntersection` arm was expected to earn its keep;
/// it does not. The guard that fires first is yang's MIN_FEATURE_SIZE input
/// contract (#178), one level up and far above the `MIN_FEATURE_SIZE³` volume
/// floor, so a sub-resolution overlap never reaches the floor at all.
#[test]
fn a_thin_overlap_is_an_overlap_until_the_kernel_refuses_the_input() {
    // 1 m cubes overlapping by `t` along x: the overlap is t × 1 × 1 m³,
    // vastly above the MIN_FEATURE_SIZE³ = 1e-18 m³ sliver floor at every t
    // here, so the answer must be its volume and not `Contact`.
    for t in [1e-5, 1e-6] {
        let mut arena = BrepArena::new();
        let a = unit_cube(&mut arena, 0.0);
        let b = unit_cube(&mut arena, 1.0 - t);
        let got = interference(&arena, a, b).expect("the Intersect runs");
        let Interference::Interferes { volume, .. } = got else {
            panic!("overlap {t:e} is an overlap, got {got:?}");
        };
        close(volume, t, 1e-9, &format!("overlap volume at t = {t:e}"));
    }
    // One decade below MIN_FEATURE_SIZE the input itself is outside the
    // kernel's contract, and it says so instead of answering.
    let mut arena = BrepArena::new();
    let a = unit_cube(&mut arena, 0.0);
    let b = unit_cube(&mut arena, 1.0 - 1e-7);
    match interference(&arena, a, b) {
        Err(KernelV2Error::BooleanFailed(reason)) => {
            assert!(
                reason.contains("MIN_FEATURE_SIZE"),
                "the refusal names the contract: {reason}"
            );
        }
        other => panic!("want the typed sub-resolution refusal, got {other:?}"),
    }
}

/// A curved region is the mesh tier, and it must carry the band its numbers
/// sit in. A mesh volume labelled with a ZERO band is indistinguishable from
/// an exact one to a consumer doing error arithmetic — and the band is what
/// explains the only asymmetry the query has (see below).
#[test]
fn a_curved_region_reports_the_band_its_volume_sits_in() {
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
    // A knife that truncates the sphere, so the region is a sphere PATCH and
    // not the whole ball: that is what puts the answer at the mesh tier.
    let knife = corner_cube(&mut arena, [-0.01, -0.01, -0.002], 0.02);

    let got = interference(&arena, sphere, knife).expect("the Intersect runs");
    let Interference::Interferes {
        volume,
        exact,
        chord_bound,
        ..
    } = got
    else {
        panic!("want Interferes, got {got:?}");
    };
    assert!(!exact, "a sphere patch is the mesh tier");
    assert!(
        chord_bound > 0.0,
        "a mesh-tier answer must carry its band, not a zero: {chord_bound:e}"
    );
    assert!(volume > 0.0, "the sphere and the knife plainly overlap");

    // The other half of the band's job. The mesh tier is the ONLY place the
    // answer depends on the argument order — measured 2026-10-03, this pair
    // differs in the fifth significant digit — and the difference is a chord
    // effect, bounded by the band over the region's own surface. A consumer
    // handed the band can account for that; one handed a zero cannot.
    let swapped = interference(&arena, knife, sphere).expect("the Intersect runs");
    let Interference::Interferes { volume: other, .. } = swapped else {
        panic!("want Interferes, got {swapped:?}");
    };
    let surface = kernel_v2::mass::mass_properties(&arena, sphere, 1.0)
        .expect("the sphere integrates")
        .surface_area;
    assert!(
        (volume - other).abs() <= chord_bound * surface,
        "the order difference {:e} must be inside band × area = {:e}",
        (volume - other).abs(),
        chord_bound * surface
    );
}

/// The P10 net: an `Intersect` result that is not inside both operands is not
/// their intersection, and Q2 STOPs instead of reporting it.
///
/// The customer is real. Two 10 mm cubes meeting along ONE edge — offset a
/// full side in two axes, flush in the third — come back from the boolean as
/// a copy of operand A: volume 1e-6 m³, centroid at A's centre, A's bounding
/// box. The live `Union` of the same pair drops an operand the same way, so
/// the defect is in the boolean and not in this query. What Q2 owes is that
/// it never reports "they overlap by 1000 mm³" about two bodies that touch.
///
/// Two of the three orientations fail and one does not (an x-aligned shared
/// edge answers `Contact` correctly), which is why this test walks all three.
#[test]
fn an_intersect_region_outside_the_operands_stops_instead_of_answering() {
    let s = 0.01;
    // (dx, dy, dz) each a full side or zero: the three edge-contact
    // orientations of the same configuration.
    for (d, label) in [
        ([s, s, 0.0], "shared edge along z"),
        ([s, 0.0, s], "shared edge along y"),
        ([0.0, s, s], "shared edge along x"),
    ] {
        let mut arena = BrepArena::new();
        let a = corner_cube(&mut arena, [0.0, 0.0, 0.0], s);
        let b = corner_cube(&mut arena, d, s);
        match interference(&arena, a, b) {
            // The honest answer: they touch on an edge.
            Ok(Interference::Contact { .. }) => {}
            // Or the net fires, because the boolean returned an operand.
            Err(KernelV2Error::InterferenceRegionOutsideOperands { bounds }) => {
                assert!(
                    bounds.contains("not inside the operands' shared box"),
                    "{label}: the STOP names the violation: {bounds}"
                );
            }
            other => panic!("{label}: two cubes sharing one edge share no volume — got {other:?}"),
        }
    }
}
