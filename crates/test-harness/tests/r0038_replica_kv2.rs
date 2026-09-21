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
/// Op 3: the circle r 1.5497 at the shared origin, revolved 102.579° about
/// the parallel axis 2.3245 from the origin toward the other axes — a
/// partial ring torus (major 2.3245, minor 1.5497), CUT.
const T_MINOR: f64 = 1.5496706876700923;
const T_MAJOR: f64 = 2.3245060315051385;
const T_ANGLE_DEG: f64 = 102.57949081843903;
/// World x of the torus axis: the origin is at x = −CENTRE_U, the axis
/// 2.3245 toward +x.
const T_AXIS_X: f64 = -CENTRE_U + T_MAJOR;
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

/// The full three-op chain: the replica plus op 3's torus cut.
fn replica_full() -> ModelBuilder {
    let mut b = replica();
    b.true_circle_sketch(
        "t_sk",
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        CENTRE_U,
        0.0,
        T_MINOR,
    )
    .unwrap();
    b.revolve_cut(
        "t",
        "t_sk",
        [T_AXIS_X, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        T_ANGLE_DEG,
    )
    .unwrap();
    b
}

/// Is `(x, y, z)` inside the partial torus of op 3? Azimuth about the torus
/// axis measured from the sketch half-plane (the −x direction from the
/// axis) in the revolve's sense (either sense: the sector geometry is
/// mirror-symmetric in y, so the volume is the same), within the sweep; and
/// the tube condition `(ρ − R)² + z² ≤ r²`.
fn in_torus(x: f64, y: f64, z: f64) -> bool {
    let (dx, dy) = (x - T_AXIS_X, y);
    let rho = (dx * dx + dy * dy).sqrt();
    let az = (-dy).atan2(-dx).to_degrees();
    if !(0.0..=T_ANGLE_DEG).contains(&az) {
        return false;
    }
    (rho - T_MAJOR).powi(2) + z * z <= T_MINOR * T_MINOR
}

/// Volumes of the two components of A − B − T by a deterministic 3D polar
/// midpoint grid over A's sector: `(inner band, outer crescent)`.
fn a_minus_b_minus_t_volumes() -> (f64, f64) {
    let (nr, nt, nz) = (600usize, 300usize, 300usize);
    let dr = (A_R1 - A_R0) / nr as f64;
    let dt = A_ANGLE_DEG.to_radians() / nt as f64;
    let dz = A_H / nz as f64;
    let (mut inner, mut outer) = (0.0, 0.0);
    for i in 0..nr {
        let r = A_R0 + (i as f64 + 0.5) * dr;
        for j in 0..nt {
            let t = (j as f64 + 0.5) * dt;
            let (x, y) = (-r * t.cos(), -r * t.sin());
            let rb = ((x - AXIS_SEP).powi(2) + y * y).sqrt();
            let cell = r * dr * dt * dz;
            if rb < B_R0 {
                for k in 0..nz {
                    let z = -A_H / 2.0 + (k as f64 + 0.5) * dz;
                    if !in_torus(x, y, z) {
                        inner += cell;
                    }
                }
            } else if rb > B_R1 {
                for k in 0..nz {
                    let z = -A_H / 2.0 + (k as f64 + 0.5) * dz;
                    if !in_torus(x, y, z) {
                        outer += cell;
                    }
                }
            }
        }
    }
    (inner, outer)
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
    let b = boss_only();
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

/// R0038's whole chain in a coordinate frame: op 3's torus tube (minor
/// radius 1.55) crosses the 0.13–0.35-thick inner band at A-azimuth ≈ 15°,
/// mid-height — entering from B's inner wall and leaving into A's bore
/// with its end cap in the void — so the band gets a clean THROUGH-HOLE
/// (genus 1, χ = 0) while the crescent (r ≥ 13.3, beyond the torus's
/// reach of 12.8) is untouched (χ = 2). Two bodies, total χ = 2; the
/// per-body exact volumes match a deterministic 3D grid integral. This is
/// the adjudication behind R0038's authored `expected_shell_count: 2`
/// (the cubical exact-membership ladder cannot read this document: the
/// crescent tapers to a knife edge at the ruling).
#[test]
fn r0038_replica_full_chain_is_a_holed_band_and_a_crescent() {
    let mut b = replica_full();
    assert_clean(&b, "replica chain");
    let handles = b.solid_handles("t").expect("chain bodies");
    assert_eq!(handles.len(), 2, "two disjoint shells");
    // Per body: the exact volume where the closed form covers it (the
    // crescent), else the render mesh's (the holed band's cylinder patches
    // carry boolean chord facets, outside `signed_volume`'s scope — a
    // declared limit, not a defect), together with the body's χ.
    let meshes = b.tessellate_all("t").unwrap();
    assert_eq!(meshes.len(), 2);
    let mut bodies: Vec<(f64, bool, i64)> = Vec::new();
    for (h, mesh) in handles.iter().zip(&meshes) {
        let wt = oracle::check_watertight_mesh(mesh);
        assert!(wt.passed, "{}", wt.detail);
        let chi0 = oracle::check_mesh_euler_characteristic(mesh, 0);
        let chi2 = oracle::check_mesh_euler_characteristic(mesh, 2);
        assert!(
            chi0.passed || chi2.passed,
            "{} / {}",
            chi0.detail,
            chi2.detail
        );
        let chi = if chi0.passed { 0 } else { 2 };
        match b.kernel_ref().as_introspect().solid_volume(h) {
            Ok(v) => bodies.push((v, true, chi)),
            Err(_) => bodies.push((mesh_signed_volume(mesh).abs(), false, chi)),
        }
    }
    bodies.sort_by(|p, q| p.0.total_cmp(&q.0));
    let (inner, outer) = a_minus_b_minus_t_volumes();
    let mut expect = [outer, inner];
    expect.sort_by(f64::total_cmp);
    for ((got, exact, _), want) in bodies.iter().zip(expect) {
        let tol = if *exact { 5e-3 } else { 2e-2 };
        assert!(
            ((got - want) / want).abs() < tol,
            "chain volumes {bodies:?} vs {expect:?}"
        );
    }
    // The band lost volume to the tube; the crescent did not.
    let (band_area, crescent_area) = a_minus_b_section_areas();
    assert!(
        inner < A_H * band_area - 1.0,
        "the tube removed a macroscopic bite: {inner}"
    );
    // Exact geometry: the torus's farthest reach from A's axis is short of
    // the crescent's inner boundary (B's outer circle at the sketch plane,
    // 13.29), so the crescent cannot be touched; the two grids agree to
    // their own resolution.
    assert!(T_AXIS_X.abs() + T_MAJOR + T_MINOR < B_R1 - AXIS_SEP);
    assert!(
        ((outer - A_H * crescent_area) / outer).abs() < 2e-2,
        "crescent untouched: {outer} vs {}",
        A_H * crescent_area
    );
    // The smaller body is the crescent (χ = 2), the larger the holed band
    // (χ = 0).
    assert_eq!(bodies[0].2, 2, "the crescent is a sphere: {bodies:?}");
    assert_eq!(
        bodies[1].2, 0,
        "the band has the tube's through-hole: {bodies:?}"
    );
}
