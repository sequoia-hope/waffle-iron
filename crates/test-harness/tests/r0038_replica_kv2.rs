//! R0038 replicated in a COORDINATE frame (spec
//! `yang_433_tangent_point_mesh_update.md` §13, checkpoint 2 — the SECTOR
//! vocabulary): the corpus case's two partial revolves with their parallel
//! axes moved onto z, everything else verbatim.
//!
//! Sketch plane y = 0 (normal +y ⇒ sketch (u, v) → world (−u, 0, v)). A = the
//! rectangle u ∈ [9.0983, 13.4185] × v ∈ ±3.7528 revolved 30.427° about the
//! z-axis through the origin; B = the rectangle centred at the SAME u
//! (11.2584, the corpus profiles share their sketch origin) × ±2.0288 wide,
//! v ∈ ±4.3962, revolved 71.368° about the z-axis through (1.9303, 0) — so
//! its radii about its own axis are 11.1600 and 15.2175 — CUT. A's outer
//! cylinder (r 13.4185) and B's outer cylinder (r 15.2175) cross along ONE
//! ruling inside both sectors, 22.7° into A's sweep, at a 2.81° grazing
//! angle — R0038's `degenerate_no_longedge` STOP. Both revolves start on the
//! same sketch plane, so Stage 0 is ACTIVE on A#0 × B#0 exactly as in the
//! corpus; only the oblique frame is missing (checkpoint 3).
//!
//! A − B is TWO bodies: the inner band between A's inner arc (r 9.098 about
//! A's axis) and B's inner cylinder (r 11.160 about B's axis, which passes
//! through A's annulus without crossing A's inner circle), and the outer
//! crescent between A's outer arc and B's outer cylinder, from the sketch
//! plane to the crossing ruling. The expected volume is A's height times a
//! deterministic polar midpoint-grid integral of A's sector minus B's
//! radial band (B's angular sweep contains all of A's sector).

use test_harness::helpers::mesh_signed_volume;
use test_harness::oracle;
use test_harness::ModelBuilder;

const A_R0: f64 = 9.098326951522125;
const A_R1: f64 = 13.418501040494824;
const A_H: f64 = 7.505609330672316;
const A_ANGLE_DEG: f64 = 30.426946807208147;
const B_HALF_W: f64 = 2.028778032143659;
const B_H: f64 = 8.792493803862644;
const B_ANGLE_DEG: f64 = 71.36833374219326;
const AXIS_SEP: f64 = 1.9303267097854921;
/// Both profiles are centred at the same sketch u (the corpus documents'
/// shared sketch origin, 11.2584 from A's axis and 13.1887 from B's).
const CENTRE_U: f64 = (A_R0 + A_R1) / 2.0;
/// B's radii about ITS axis.
const B_R0: f64 = CENTRE_U + AXIS_SEP - B_HALF_W;
const B_R1: f64 = CENTRE_U + AXIS_SEP + B_HALF_W;

fn assert_clean(b: &ModelBuilder, label: &str) {
    let errors = b.engine_errors().to_vec();
    assert!(errors.is_empty(), "{label}: engine errors: {errors:?}");
    let warnings = b.engine_warnings().to_vec();
    assert!(
        warnings.is_empty(),
        "{label}: engine warnings: {warnings:?}"
    );
}

fn boss_only() -> ModelBuilder {
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch(
        "a_sk",
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        A_R0,
        -A_H / 2.0,
        A_R1 - A_R0,
        A_H,
    )
    .unwrap();
    b.revolve("a", "a_sk", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], A_ANGLE_DEG)
        .unwrap();
    b
}

fn replica() -> ModelBuilder {
    let mut b = boss_only();
    b.rect_sketch(
        "b_sk",
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        CENTRE_U - B_HALF_W,
        -B_H / 2.0,
        2.0 * B_HALF_W,
        B_H,
    )
    .unwrap();
    b.revolve_cut(
        "b",
        "b_sk",
        [AXIS_SEP, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        B_ANGLE_DEG,
    )
    .unwrap();
    b
}

/// Areas of A's cross-section sector outside B's radial band — `(inside
/// B's inner circle, outside B's outer circle)` — by a polar midpoint grid
/// over A's sector (deterministic; B's 71° sweep contains all of A's 30°
/// sector, so membership is radial only).
fn a_minus_b_section_areas() -> (f64, f64) {
    let (nr, nt) = (3000usize, 1500usize);
    let dr = (A_R1 - A_R0) / nr as f64;
    let dt = A_ANGLE_DEG.to_radians() / nt as f64;
    let (mut inner, mut outer) = (0.0, 0.0);
    for i in 0..nr {
        let r = A_R0 + (i as f64 + 0.5) * dr;
        for j in 0..nt {
            let t = (j as f64 + 0.5) * dt;
            // Point at A-azimuth t (from the sketch half-plane, either sense).
            let (x, y) = (-r * t.cos(), -r * t.sin());
            let rb = ((x - AXIS_SEP).powi(2) + y * y).sqrt();
            if rb < B_R0 {
                inner += r * dr * dt;
            } else if rb > B_R1 {
                outer += r * dr * dt;
            }
        }
    }
    (inner, outer)
}

/// Convention pin: the boss alone has the exact revolve volume
/// `(θ/2)(R² − r²)·h`, so the rectangle sits where the doc comment says.
#[test]
fn boss_alone_has_the_exact_sector_volume() {
    let mut b = boss_only();
    assert_clean(&b, "boss");
    let handle = b.solid_handle("a").expect("boss");
    let exact = b
        .kernel_ref()
        .as_introspect()
        .solid_volume(&handle)
        .expect("exact volume");
    let expect = 0.5 * A_ANGLE_DEG.to_radians() * (A_R1 * A_R1 - A_R0 * A_R0) * A_H;
    assert!(
        ((exact - expect) / expect).abs() < 1e-9,
        "boss exact volume {exact} vs {expect}"
    );
}

/// The crossing ruling really is inside both sectors and grazing.
#[test]
fn crossing_ruling_is_mid_sector_and_grazing() {
    let x = (A_R1 * A_R1 - B_R1 * B_R1 + AXIS_SEP * AXIS_SEP) / (2.0 * AXIS_SEP);
    let y = (A_R1 * A_R1 - x * x).sqrt();
    // A-azimuth from the sketch half-plane (−x direction).
    let az_a = (y).atan2(-x).to_degrees();
    assert!(az_a > 1.0 && az_a < A_ANGLE_DEG - 1.0, "A-azimuth {az_a}");
    let az_b = (y).atan2(-(x - AXIS_SEP)).to_degrees();
    assert!(az_b > 1.0 && az_b < B_ANGLE_DEG - 1.0, "B-azimuth {az_b}");
    let na = [x / A_R1, y / A_R1];
    let nb = [(x - AXIS_SEP) / B_R1, y / B_R1];
    let angle = (na[0] * nb[0] + na[1] * nb[1]).acos().to_degrees();
    assert!((angle - 2.81).abs() < 0.05, "crossing angle {angle}");
}

/// R0038's op 2 in a coordinate frame: the cut completes as TWO bodies (the
/// inner band and the outer crescent, in either order) whose exact volumes
/// are the two components of A minus B's annular sector.
#[test]
fn r0038_replica_cut_completes_with_the_exact_volume() {
    let mut b = replica();
    assert_clean(&b, "replica cut");
    let handles = b.solid_handles("b").expect("cut bodies");
    assert_eq!(handles.len(), 2, "A − B is two disjoint shells");
    let mut exact: Vec<f64> = handles
        .iter()
        .map(|h| {
            b.kernel_ref()
                .as_introspect()
                .solid_volume(h)
                .expect("exact volume")
        })
        .collect();
    exact.sort_by(f64::total_cmp);
    let (inner, outer) = a_minus_b_section_areas();
    let mut expect = [A_H * outer, A_H * inner];
    expect.sort_by(f64::total_cmp);
    for (got, want) in exact.iter().zip(expect) {
        assert!(
            ((got - want) / want).abs() < 2e-3,
            "cut exact volumes {exact:?} vs {expect:?}"
        );
    }
    // Each body a closed genus-0 shell on its own.
    for mesh in b.tessellate_all("b").unwrap() {
        let wt = oracle::check_watertight_mesh(&mesh);
        assert!(wt.passed, "{}", wt.detail);
        let chi = oracle::check_mesh_euler_characteristic(&mesh, 2);
        assert!(chi.passed, "{}", chi.detail);
    }
    let combined = b.tessellate_combined("b").unwrap();
    let v = mesh_signed_volume(&combined).abs();
    let total = A_H * (inner + outer);
    assert!(
        ((v - total) / total).abs() < 2e-2,
        "cut mesh volume {v} vs {total}"
    );
}
