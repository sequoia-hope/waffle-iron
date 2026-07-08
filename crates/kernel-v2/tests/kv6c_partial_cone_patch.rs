//! PR-KV6c increment 5 RED oracles — partial revolve of an oblique edge:
//! the arc-bounded CONE patch (spec `specs/kv6c_partial_revolve_cone_patch.md`).
//!
//! ## Scope (spec §4, increment 1 — kernel-v2 only)
//!
//! A partial-angle revolve (0 < α < 2π) of a hole-free off-axis polygon whose
//! edge classification contains `EdgeClass::Oblique` — today rejected typed
//! (`RevolveObliqueEdgeUnsupported`) before any mutation. After increment 1 the
//! oblique edge sweeps a `Surface::Cone` wall carried on the KV6a partial-wall
//! topology (V=2k, E=3k, F=k+2), and validate / signed_volume / render
//! tessellation accept it. Topologically IDENTICAL to the partial cylinder
//! wall; only the surface arm, volume flux, and tessellation are new.
//!
//! ## Cone parameters (spec §1, matching `validate_revolve_geometry`)
//!
//! For an oblique edge running from radius `s0` at axial `t0` to `s1` at `t1`:
//! `apex` is where the slant extended meets the axis (`s = 0`), `axis_dir` is
//! oriented so both rims have τ > 0 (apex behind), `half_angle = atan|Δs/Δt|`,
//! `reversed = dt > 0` in the working-CCW loop (material on the larger-radius
//! side — a conical bore).
//!
//! ## Oracle groups (spec §4)
//!
//! 1. Canonical: trapezoid `(s,t) = (1,0),(3,0),(2,1),(1,1)` at 90°/180°/200°
//!    — census, one cone wall with analytic params, Pappus-fraction volume,
//!    watertight mesh in the chord band.
//! 2. Cavity sense: an inner-bore oblique edge builds `reversed = true`.
//! 3. Two oblique edges → two cone walls, distinct apex/half_angle.
//! 4. Edge cases: 270° (near-quadrant) and a slender oblique edge.
//! 5. Typed walls that STAY: on-axis partial revolve and holed profile.
//!
//! RED PHASE: every positive build here fails today with
//! `RevolveObliqueEdgeUnsupported`; the two rejection oracles already pass and
//! pin the walls that must remain typed after increment 1.

use std::f64::consts::{FRAC_PI_2, PI};

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::{
    geom, revolve, tessellate, validate_solid, BrepArena, Profile, RenderMesh, RevolveResult,
    Surface,
};

const AXIS_O: Point3 = Point3::new(0.0, 0.0, 0.0);
const AXIS_D: Vector3 = Vector3::new(1.0, 0.0, 0.0);

// =========================================================================
// Fixtures — (s, t) = (radial, axial). Point2 is (axial, radial) with the
// plane basis u = x̂ (the axis), v = ŷ (the in-plane radial direction).
// =========================================================================

/// The canonical trapezoid `(s,t) = (1,0),(3,0),(2,1),(1,1)`: one oblique
/// outer edge from radius 3 (t=0) to radius 2 (t=1), an inner radius-1
/// cylinder wall, and two axis-perpendicular annular-sector caps.
fn trapezoid_profile() -> Profile {
    Profile::new(
        AXIS_O,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 1.0), // (s,t)=(1,0)
            Point2::new(0.0, 3.0), // (s,t)=(3,0)
            Point2::new(1.0, 2.0), // (s,t)=(2,1)
            Point2::new(1.0, 1.0), // (s,t)=(1,1)
        ],
        vec![],
    )
    .expect("trapezoid profile")
}

/// The full-turn solid-of-revolution volume of `trapezoid_profile`:
/// ∫₀¹ π((3−t)² − 1) dt = π·16/3. The exact anchor for the reference used by
/// the Pappus-fraction oracle.
const TRAPEZOID_FULL_VOLUME: f64 = 16.0 * PI / 3.0;

// =========================================================================
// Shared oracle helpers (copied from kv6a_revolve.rs — each test binary is
// its own crate; there is no shared test-support module).
// =========================================================================

fn mesh_signed_volume(mesh: &RenderMesh) -> f64 {
    let p = |i: u32| {
        let k = (i as usize) * 3;
        [
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        ]
    };
    let mut six_v = 0.0;
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        six_v += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    six_v / 6.0
}

/// Watertightness by POSITION-keyed directed-edge pairing (faces emit
/// duplicated vertices; keys quantized at 1e-9, far below any feature scale).
fn assert_watertight(mesh: &RenderMesh, what: &str) {
    use std::collections::HashMap;
    let q = |x: f64| (x / 1e-9).round() as i64;
    let key = |i: u32| {
        let k = (i as usize) * 3;
        (
            q(mesh.positions[k]),
            q(mesh.positions[k + 1]),
            q(mesh.positions[k + 2]),
        )
    };
    let mut count: HashMap<_, i64> = HashMap::new();
    for t in mesh.indices.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (ka, kb) = (key(a), key(b));
            if ka == kb {
                continue;
            }
            *count.entry((ka, kb)).or_insert(0) += 1;
            *count.entry((kb, ka)).or_insert(0) -= 1;
        }
    }
    let unpaired = count.values().filter(|&&c| c != 0).count();
    assert_eq!(unpaired, 0, "{what}: {unpaired} unpaired directed edges");
}

fn assert_mesh_sane(mesh: &RenderMesh, what: &str) {
    assert!(!mesh.indices.is_empty(), "{what}: empty mesh");
    for v in &mesh.positions {
        assert!(v.is_finite(), "{what}: non-finite position");
    }
    for chunk in mesh.normals.chunks_exact(3) {
        let len = (chunk[0] * chunk[0] + chunk[1] * chunk[1] + chunk[2] * chunk[2]).sqrt();
        assert!(
            (len - 1.0).abs() < 1e-9,
            "{what}: non-unit normal {chunk:?}"
        );
    }
}

fn assert_rel_eq(actual: f64, expected: f64, rel: f64, what: &str) {
    let err = (actual - expected).abs();
    assert!(
        err <= rel * expected.abs(),
        "{what}: |{actual} - {expected}| = {err} > {rel} * {}",
        expected.abs()
    );
}

/// The analytic cone parameters read off a `Surface::Cone` wall.
#[derive(Debug, Clone, Copy)]
struct ConeParams {
    apex: [f64; 3],
    axis_dir: [f64; 3],
    half_angle: f64,
    reversed: bool,
}

/// Every `Surface::Cone` wall of a revolve result, in `walls` order.
fn cone_walls(arena: &BrepArena, r: &RevolveResult) -> Vec<ConeParams> {
    r.walls
        .iter()
        .filter_map(|&w| match arena.face(w).expect("wall").surface {
            Some(Surface::Cone {
                apex,
                axis_dir,
                half_angle,
                reversed,
            }) => Some(ConeParams {
                apex: [apex.x(), apex.y(), apex.z()],
                axis_dir: [axis_dir.x, axis_dir.y, axis_dir.z],
                half_angle,
                reversed,
            }),
            _ => None,
        })
        .collect()
}

/// Reference full-turn volume of a profile (built on a fresh arena via the
/// already-supported 360° oblique path).
fn full_turn_volume(profile: &Profile) -> f64 {
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, profile, AXIS_O, AXIS_D, 2.0 * PI)
        .expect("full-turn reference revolve builds");
    geom::signed_volume(&arena, r.solid).expect("full-turn reference volume")
}

// =========================================================================
// 1. Canonical: trapezoid at 90° / 180° / 200°
// =========================================================================

#[test]
fn canonical_partial_cone_patch_topology_params_and_volume() {
    // Anchor the reference: the full-turn trapezoid solid is π·16/3 exactly.
    let full_vol = full_turn_volume(&trapezoid_profile());
    assert_rel_eq(
        full_vol,
        TRAPEZOID_FULL_VOLUME,
        1e-9,
        "full-turn trapezoid volume anchor",
    );

    for angle in [FRAC_PI_2, PI, 200.0_f64.to_radians()] {
        let mut arena = BrepArena::new();
        let r = revolve(&mut arena, &trapezoid_profile(), AXIS_O, AXIS_D, angle)
            .unwrap_or_else(|e| panic!("partial cone revolve at {angle}: {e:?}"));

        // I1 census: k=4 → V=2k, E=3k, F=k+2, genus 0, χ=2.
        let report = validate_solid(&arena, r.solid)
            .unwrap_or_else(|e| panic!("partial cone patch validates at {angle}: {e:?}"));
        assert_eq!(report.vertices, 8, "V at {angle}");
        assert_eq!(report.edges, 12, "E at {angle}");
        assert_eq!(report.faces, 6, "F at {angle}");
        assert_eq!(report.genus, 0, "partial cone patch is genus 0 at {angle}");
        assert_eq!(report.euler_lhs, 2);
        assert_eq!(report.euler_rhs, 2);
        assert_eq!(r.walls.len(), 4, "one wall per profile edge at {angle}");

        // I1 surface: exactly one cone wall, analytic params. The oblique
        // edge runs radius 3→2 over axial 0→1; extended it meets the axis at
        // t=3 (apex (3,0,0)), axis_dir points apex→rims = −x̂, half_angle
        // = atan(1) = π/4, reversed = false (an outer wall).
        let cones = cone_walls(&arena, &r);
        assert_eq!(cones.len(), 1, "exactly one Surface::Cone wall at {angle}");
        let c = cones[0];
        assert!(
            (c.apex[0] - 3.0).abs() <= 1e-12
                && c.apex[1].abs() <= 1e-15
                && c.apex[2].abs() <= 1e-15,
            "apex {:?} != (3,0,0) at {angle}",
            c.apex
        );
        assert!(
            (c.axis_dir[0] + 1.0).abs() <= 1e-15
                && c.axis_dir[1].abs() <= 1e-15
                && c.axis_dir[2].abs() <= 1e-15,
            "axis_dir {:?} != -x̂ at {angle}",
            c.axis_dir
        );
        assert!(
            (c.half_angle - FRAC_PI_2 / 2.0).abs() <= 1e-15,
            "half_angle {} != π/4 at {angle}",
            c.half_angle
        );
        assert!(!c.reversed, "outer oblique wall is not reversed at {angle}");

        // I2 volume: exact Pappus fraction of the full-turn reference.
        let frac = angle / (2.0 * PI);
        let vol = geom::signed_volume(&arena, r.solid)
            .unwrap_or_else(|e| panic!("cone patch volume at {angle}: {e:?}"));
        assert!(vol > 0.0, "outward orientation at {angle}");
        assert_rel_eq(vol, frac * full_vol, 1e-9, "Pappus-fraction cone volume");

        // I4 render: watertight mesh, volume in the chord band.
        let mesh =
            tessellate(&arena, r.solid).unwrap_or_else(|e| panic!("tessellate at {angle}: {e:?}"));
        assert_mesh_sane(&mesh, "partial cone mesh");
        assert_watertight(&mesh, "partial cone mesh");
        let mv = mesh_signed_volume(&mesh);
        assert!(mv > 0.0, "positive mesh volume at {angle}");
        assert_rel_eq(mv, frac * full_vol, 2e-2, "cone patch mesh volume band");
    }
}

// =========================================================================
// 2. Cavity sense: an inner conical bore builds reversed = true
// =========================================================================

/// A conical-bore annulus: outer cylinder at radius 4, inner cone from radius
/// 1 (t=0) to radius 2 (t=1). The oblique INNER edge has material on its
/// larger-radius side, so its wall is `reversed = true`.
fn conical_bore_profile() -> Profile {
    Profile::new(
        AXIS_O,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 1.0), // inner-bottom, radius 1
            Point2::new(0.0, 4.0), // outer-bottom, radius 4
            Point2::new(1.0, 4.0), // outer-top, radius 4
            Point2::new(1.0, 2.0), // inner-top, radius 2
        ],
        vec![],
    )
    .expect("conical-bore profile")
}

#[test]
fn cavity_sense_oblique_builds_reversed_cone() {
    let full_vol = full_turn_volume(&conical_bore_profile());
    // ∫₀¹ π(16 − (1+t)²) dt = π·41/3 — the independent anchor.
    assert_rel_eq(full_vol, 41.0 * PI / 3.0, 1e-9, "conical-bore full volume");

    let angle = 200.0_f64.to_radians();
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, &conical_bore_profile(), AXIS_O, AXIS_D, angle)
        .unwrap_or_else(|e| panic!("conical-bore partial revolve: {e:?}"));
    validate_solid(&arena, r.solid).expect("conical-bore patch validates");

    let cones = cone_walls(&arena, &r);
    assert_eq!(cones.len(), 1, "one conical-bore wall");
    assert!(cones[0].reversed, "inner conical bore is the cavity sense");
    // Inner cone: radius 1→2 over axial 0→1 (slope +1), apex where s=0 at
    // t=−1, half_angle = atan(1) = π/4.
    assert!(
        (cones[0].apex[0] + 1.0).abs() <= 1e-12,
        "bore apex t {:?} != −1",
        cones[0].apex
    );
    assert!(
        (cones[0].half_angle - FRAC_PI_2 / 2.0).abs() <= 1e-15,
        "bore half_angle {} != π/4",
        cones[0].half_angle
    );

    let frac = angle / (2.0 * PI);
    let vol = geom::signed_volume(&arena, r.solid).expect("conical-bore volume");
    assert_rel_eq(vol, frac * full_vol, 1e-9, "conical-bore Pappus fraction");
}

// =========================================================================
// 3. Two oblique edges → two distinct cone walls
// =========================================================================

/// A conical shell whose BOTH radial walls slant: outer `(s,t)` 3→2 over
/// t 0→1 (apex t=3, half π/4), inner 1→1.5 over t 0→1 (apex t=−2,
/// half atan(0.5)). The t=0 and t=1 edges are axis-perpendicular caps.
fn double_cone_profile() -> Profile {
    Profile::new(
        AXIS_O,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 1.0), // (s,t)=(1,0)   inner-bottom
            Point2::new(0.0, 3.0), // (s,t)=(3,0)   outer-bottom
            Point2::new(1.0, 2.0), // (s,t)=(2,1)   outer-top
            Point2::new(1.0, 1.5), // (s,t)=(1.5,1) inner-top
        ],
        vec![],
    )
    .expect("double-cone profile")
}

#[test]
fn two_oblique_edges_build_two_distinct_cones() {
    let full_vol = full_turn_volume(&double_cone_profile());
    // ∫₀¹ π((3−t)² − (1+0.5t)²) dt = π·4.75 — the anchor.
    assert_rel_eq(full_vol, 4.75 * PI, 1e-9, "double-cone full volume");

    let angle = PI;
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, &double_cone_profile(), AXIS_O, AXIS_D, angle)
        .unwrap_or_else(|e| panic!("double-cone partial revolve: {e:?}"));
    validate_solid(&arena, r.solid).expect("double-cone patch validates");

    let cones = cone_walls(&arena, &r);
    assert_eq!(cones.len(), 2, "two Surface::Cone walls");
    // Distinct apexes (t=3 outer, t=−2 inner) and half-angles (π/4, atan 0.5).
    let mut apex_t: Vec<f64> = cones.iter().map(|c| c.apex[0]).collect();
    apex_t.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!(
        (apex_t[0] + 2.0).abs() <= 1e-12 && (apex_t[1] - 3.0).abs() <= 1e-12,
        "cone apexes {apex_t:?} != (−2, 3)"
    );
    let mut halves: Vec<f64> = cones.iter().map(|c| c.half_angle).collect();
    halves.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!(
        (halves[0] - 0.5_f64.atan()).abs() <= 1e-15 && (halves[1] - FRAC_PI_2 / 2.0).abs() <= 1e-15,
        "cone half-angles {halves:?} != (atan 0.5, π/4)"
    );
    // One wall per sense: the outer is solid, the inner is a bore.
    assert_eq!(
        cones.iter().filter(|c| c.reversed).count(),
        1,
        "exactly one reversed (inner-bore) cone wall"
    );

    let frac = angle / (2.0 * PI);
    let vol = geom::signed_volume(&arena, r.solid).expect("double-cone volume");
    assert_rel_eq(vol, frac * full_vol, 1e-9, "double-cone Pappus fraction");
}

// =========================================================================
// 4. Edge cases: 270° near-quadrant and a slender oblique edge
// =========================================================================

#[test]
fn near_quadrant_270_partial_cone_builds() {
    let full_vol = full_turn_volume(&trapezoid_profile());
    let angle = 3.0 * FRAC_PI_2; // 270°
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, &trapezoid_profile(), AXIS_O, AXIS_D, angle)
        .unwrap_or_else(|e| panic!("270° partial cone revolve: {e:?}"));
    validate_solid(&arena, r.solid).expect("270° cone patch validates");
    assert_eq!(cone_walls(&arena, &r).len(), 1, "one cone wall at 270°");

    let frac = angle / (2.0 * PI);
    let vol = geom::signed_volume(&arena, r.solid).expect("270° cone volume");
    assert_rel_eq(vol, frac * full_vol, 1e-9, "270° Pappus fraction");

    let mesh = tessellate(&arena, r.solid).expect("270° cone tessellates");
    assert_watertight(&mesh, "270° cone mesh");
    assert_rel_eq(
        mesh_signed_volume(&mesh),
        frac * full_vol,
        2e-2,
        "270° mesh band",
    );
}

/// A near-cylindrical oblique wall: outer radius 2 → 2.001 over axial 0→1
/// (Δs = 1e-3, well above the 1e-9·len alignment band), inner cylinder r=1.
fn slender_oblique_profile() -> Profile {
    Profile::new(
        AXIS_O,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 1.0),
            Point2::new(0.0, 2.0),
            Point2::new(1.0, 2.001),
            Point2::new(1.0, 1.0),
        ],
        vec![],
    )
    .expect("slender-oblique profile")
}

#[test]
fn slender_oblique_edge_classifies_and_builds_cone() {
    let full_vol = full_turn_volume(&slender_oblique_profile());
    let angle = FRAC_PI_2;
    let mut arena = BrepArena::new();
    let r = revolve(
        &mut arena,
        &slender_oblique_profile(),
        AXIS_O,
        AXIS_D,
        angle,
    )
    .unwrap_or_else(|e| panic!("slender oblique revolve: {e:?}"));
    validate_solid(&arena, r.solid).expect("slender cone patch validates");

    let cones = cone_walls(&arena, &r);
    assert_eq!(
        cones.len(),
        1,
        "the slender edge classifies Oblique → one cone"
    );
    // Δs = 1e-3, Δt = 1 → a very small, but nonzero, half-angle.
    assert!(
        (cones[0].half_angle - 1e-3_f64.atan()).abs() <= 1e-12,
        "slender half_angle {} != atan(1e-3)",
        cones[0].half_angle
    );

    let frac = angle / (2.0 * PI);
    let vol = geom::signed_volume(&arena, r.solid).expect("slender cone volume");
    assert_rel_eq(vol, frac * full_vol, 1e-9, "slender Pappus fraction");
}

// =========================================================================
// 5. Typed walls that STAY (spec §2/§5): on-axis partial + holed profile
// =========================================================================

#[test]
fn on_axis_partial_oblique_stays_axis_intersects_typed() {
    // Bottom edge on the axis (radius 0) with an oblique outer edge: the
    // clearance rejection fires before edge classification, and the on-axis
    // recovery is full-turn only, so a partial angle stays typed.
    let on_axis = Profile::new(
        AXIS_O,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(3.0, 0.0),
            Point2::new(2.0, 1.0),
            Point2::new(0.0, 1.0),
        ],
        vec![],
    )
    .expect("on-axis oblique profile");
    let mut arena = BrepArena::new();
    let err = revolve(&mut arena, &on_axis, AXIS_O, AXIS_D, PI)
        .expect_err("on-axis partial oblique stays typed");
    assert_eq!(err, kernel_v2::KernelV2Error::RevolveAxisIntersectsProfile);
    assert_eq!(arena, BrepArena::new(), "arena untouched after Err");
}

#[test]
fn holed_oblique_profile_stays_holes_unsupported_typed() {
    // A holed outer polygon with an oblique edge: the hole rejection precedes
    // edge classification, so it stays typed even after the oblique wall lands.
    let holed = Profile::new(
        AXIS_O,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 1.0),
            Point2::new(0.0, 3.0),
            Point2::new(1.0, 2.0),
            Point2::new(1.0, 1.0),
        ],
        vec![vec![
            Point2::new(0.25, 1.4),
            Point2::new(0.25, 1.7),
            Point2::new(0.6, 1.7),
            Point2::new(0.6, 1.4),
        ]],
    )
    .expect("holed oblique profile");
    let mut arena = BrepArena::new();
    let err =
        revolve(&mut arena, &holed, AXIS_O, AXIS_D, PI).expect_err("holed profile stays typed");
    assert_eq!(
        err,
        kernel_v2::KernelV2Error::RevolveProfileHolesUnsupported
    );
    assert_eq!(arena, BrepArena::new(), "arena untouched after Err");
}
