//! KV6 on-axis PARTIAL revolve, slice 3 — the WEDGE (spec
//! `specs/kv6_on_axis_revolve_partial_wedge.md`, task #85).
//!
//! A lathe 4-gon with exactly one ON-AXIS edge, revolved by a partial angle
//! (0 < α < 2π), builds a wedge: the two on-axis vertices are fixed by the
//! rotation, so the θ=0 and θ=α cap faces share the on-axis edge directly (the
//! swept face of the on-axis edge is degenerate and not emitted). Census
//! V=6, E=9, F=5, χ=2. Off-axis edge parallel → cylindrical (cheese) wedge;
//! oblique → conical-frustum wedge.
//!
//! Today the on-axis recovery arm in `revolve()` only fires for full turns, so
//! every partial on-axis profile returns `RevolveAxisIntersectsProfile`. This
//! slice widens that arm to partial angles.
//!
//! RED PHASE: the wedge-positive tests (canonical, frustum, near-full, tiny)
//! fail today with `RevolveAxisIntersectsProfile`; the typed-wall pins (apex
//! triangle partial, crossing partial) and the full-turn byte-identical replay
//! PASS today and stay green after the slice lands.
//!
//! Fixtures + assertion values mirror the slice-1/2 tests in
//! `kv6a_revolve.rs` (`on_axis_rect_profile`, `on_axis_frustum_profile`,
//! `on_axis_triangle_profile`; H=3, R1=1, R2=2).

use std::f64::consts::PI;

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::{
    geom, revolve, tessellate, validate_solid, BrepArena, KernelV2Error, Profile, RenderMesh,
    RevolveResult, SolidId, Surface,
};

const R1: f64 = 1.0;
const R2: f64 = 2.0;
const H: f64 = 3.0;

const AXIS_O: Point3 = Point3::new(0.0, 0.0, 0.0);
const AXIS_D: Vector3 = Vector3::new(1.0, 0.0, 0.0);

/// R0004's corpus angle (the driver case), in degrees.
const R0004_DEG: f64 = 39.20170800523344;

// =========================================================================
// Fixtures (identical to the slice-1/2 profiles in kv6a_revolve.rs)
// =========================================================================

/// Rectangle t∈[0,H], s∈[0,R2] — the s=0 edge (0,0)–(H,0) lies ON the axis.
/// Full turn → solid cylinder (slice 1); partial → cylindrical wedge.
fn on_axis_rect_profile() -> Profile {
    Profile::new(
        AXIS_O,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(H, 0.0),
            Point2::new(H, R2),
            Point2::new(0.0, R2),
        ],
        vec![],
    )
    .expect("on-axis rect profile")
}

/// On-axis edge (0,0)–(H,0), perpendicular caps, oblique off-axis edge from
/// radius R1 at t=H down to R2 at t=0. Full turn → solid frustum (slice 2A);
/// partial → conical-frustum wedge.
fn on_axis_frustum_profile() -> Profile {
    Profile::new(
        AXIS_O,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(H, 0.0),
            Point2::new(H, R1),
            Point2::new(0.0, R2),
        ],
        vec![],
    )
    .expect("on-axis frustum profile")
}

/// On-axis apex triangle (0,0)–(H,0)–(0,R2): full turn → solid cone, partial
/// → OUT of this slice (apex-on-boundary vocabulary), stays typed.
fn on_axis_triangle_profile() -> Profile {
    Profile::new(
        AXIS_O,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(H, 0.0),
            Point2::new(0.0, R2),
        ],
        vec![],
    )
    .expect("on-axis triangle profile")
}

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
}

fn assert_rel_eq(actual: f64, expected: f64, rel: f64, what: &str) {
    let err = (actual - expected).abs();
    assert!(
        err <= rel * expected.abs(),
        "{what}: |{actual} - {expected}| = {err} > {rel} * {}",
        expected.abs()
    );
}

/// (planes, cylinders, cones) over EVERY face of the solid's shells — robust
/// to how `RevolveResult` distributes faces between caps and walls.
fn surface_counts(arena: &BrepArena, solid: SolidId) -> (usize, usize, usize) {
    let (mut planes, mut cyls, mut cones) = (0, 0, 0);
    for &sh in &arena.solid(solid).expect("solid").shells {
        for &fc in &arena.shell(sh).expect("shell").faces {
            match arena.face(fc).expect("face").surface {
                Some(Surface::Plane(_)) => planes += 1,
                Some(Surface::Cylinder { .. }) => cyls += 1,
                Some(Surface::Cone { .. }) => cones += 1,
                other => panic!("unexpected surface {other:?}"),
            }
        }
    }
    (planes, cyls, cones)
}

/// The single `Surface::Cone` wall's params (apex[3], axis_dir[3], half_angle,
/// reversed). Panics unless exactly one cone face is present.
fn only_cone(arena: &BrepArena, solid: SolidId) -> ([f64; 3], [f64; 3], f64, bool) {
    let mut found = None;
    for &sh in &arena.solid(solid).expect("solid").shells {
        for &fc in &arena.shell(sh).expect("shell").faces {
            if let Some(Surface::Cone {
                apex,
                axis_dir,
                half_angle,
                reversed,
            }) = arena.face(fc).expect("face").surface
            {
                assert!(found.is_none(), "more than one cone wall");
                found = Some((
                    [apex.x(), apex.y(), apex.z()],
                    [axis_dir.x, axis_dir.y, axis_dir.z],
                    half_angle,
                    reversed,
                ));
            }
        }
    }
    found.expect("exactly one cone wall")
}

/// I1 census: a wedge is V=6, E=9, F=5, genus 0, χ=2, one shell, no rings.
fn assert_wedge_census(arena: &BrepArena, r: &RevolveResult, what: &str) {
    let report =
        validate_solid(arena, r.solid).unwrap_or_else(|e| panic!("{what} validates: {e:?}"));
    assert_eq!(report.vertices, 6, "{what} V");
    assert_eq!(report.edges, 9, "{what} E");
    assert_eq!(report.faces, 5, "{what} F");
    assert_eq!(report.rings, 0, "{what} rings");
    assert_eq!(report.shells, 1, "{what} shells");
    assert_eq!(report.genus, 0, "{what} genus");
    assert_eq!(report.euler_lhs, 2, "{what} χ");
    assert_eq!(report.euler_rhs, 2, "{what} χ rhs");
}

/// Cylinder wedge exact volume: (α/2)·r²·h (r = R2).
fn cyl_wedge_volume(angle: f64) -> f64 {
    0.5 * angle * R2 * R2 * H
}

/// Frustum wedge exact volume: (α/2π)·(πH/3)(r₀²+r₀r₁+r₁²).
fn frustum_wedge_volume(angle: f64) -> f64 {
    (angle / (2.0 * PI)) * (PI * H / 3.0) * (R2 * R2 + R2 * R1 + R1 * R1)
}

// =========================================================================
// 1. Canonical cylindrical wedge — 90°, 200°, R0004's 39.2°
// =========================================================================

#[test]
fn canonical_cylindrical_wedge() {
    for angle in [PI / 2.0, 200.0_f64.to_radians(), R0004_DEG.to_radians()] {
        let mut arena = BrepArena::new();
        let r = revolve(&mut arena, &on_axis_rect_profile(), AXIS_O, AXIS_D, angle)
            .unwrap_or_else(|e| panic!("cylindrical wedge at {angle}: {e:?}"));

        assert_wedge_census(&arena, &r, "cylindrical wedge");

        // I1 surfaces: one cylinder wall + two planar pie sectors + two planar
        // caps = 4 planes, 1 cylinder, 0 cones.
        assert_eq!(
            surface_counts(&arena, r.solid),
            (4, 1, 0),
            "cylindrical wedge surface census at {angle}"
        );

        // I2 volume: exact Pappus fraction (α/2)·r²·h.
        let vol = geom::signed_volume(&arena, r.solid)
            .unwrap_or_else(|e| panic!("wedge volume at {angle}: {e:?}"));
        assert!(vol > 0.0, "outward orientation at {angle}");
        assert_rel_eq(
            vol,
            cyl_wedge_volume(angle),
            1e-9,
            "cylindrical wedge volume",
        );

        // I3 render: watertight mesh, volume in the 2% chord band.
        let mesh =
            tessellate(&arena, r.solid).unwrap_or_else(|e| panic!("tessellate at {angle}: {e:?}"));
        assert_mesh_sane(&mesh, "cylindrical wedge mesh");
        assert_watertight(&mesh, "cylindrical wedge mesh");
        let mv = mesh_signed_volume(&mesh);
        assert!(mv > 0.0, "positive mesh volume at {angle}");
        assert_rel_eq(
            mv,
            cyl_wedge_volume(angle),
            2e-2,
            "cylindrical wedge mesh band",
        );
    }
}

// =========================================================================
// 2. Frustum wedge — the slice-2A oblique quad at 200°
// =========================================================================

#[test]
fn frustum_wedge_at_200_degrees() {
    let angle = 200.0_f64.to_radians();
    let mut arena = BrepArena::new();
    let r = revolve(
        &mut arena,
        &on_axis_frustum_profile(),
        AXIS_O,
        AXIS_D,
        angle,
    )
    .unwrap_or_else(|e| panic!("frustum wedge: {e:?}"));

    assert_wedge_census(&arena, &r, "frustum wedge");

    // One cone wall + two planar pie sectors + two planar caps.
    assert_eq!(
        surface_counts(&arena, r.solid),
        (4, 0, 1),
        "frustum wedge surface census"
    );

    // I1: the cone params equal the slice-2A full-turn values. Slant from
    // (t=0, s=R2) to (t=H, s=R1): apex at t = −R2·H/(R1−R2) = 6, axis_dir −x̂
    // (apex → rims, τ>0), half_angle = atan((R2−R1)/H) = atan(1/3), outer.
    let (apex, axis_dir, half_angle, reversed) = only_cone(&arena, r.solid);
    let apex_t = -R2 * H / (R1 - R2);
    assert!(
        (apex[0] - apex_t).abs() <= 1e-12 * apex_t
            && apex[1].abs() <= 1e-15
            && apex[2].abs() <= 1e-15,
        "frustum wedge apex {apex:?} != ({apex_t}, 0, 0)"
    );
    assert!(
        (axis_dir[0] + 1.0).abs() <= 1e-15
            && axis_dir[1].abs() <= 1e-15
            && axis_dir[2].abs() <= 1e-15,
        "frustum wedge axis_dir {axis_dir:?} != -x̂"
    );
    assert!(
        (half_angle - ((R2 - R1) / H).atan()).abs() <= 1e-15,
        "frustum wedge half_angle {half_angle} != atan(1/3)"
    );
    assert!(!reversed, "outer frustum wedge wall is not reversed");

    // I2 volume: exact Pappus fraction of the full-turn frustum (7π).
    let vol = geom::signed_volume(&arena, r.solid).expect("frustum wedge volume");
    assert_rel_eq(
        vol,
        frustum_wedge_volume(angle),
        1e-9,
        "frustum wedge volume",
    );

    // I3 render.
    let mesh = tessellate(&arena, r.solid).expect("frustum wedge tessellates");
    assert_mesh_sane(&mesh, "frustum wedge mesh");
    assert_watertight(&mesh, "frustum wedge mesh");
    assert_rel_eq(
        mesh_signed_volume(&mesh),
        frustum_wedge_volume(angle),
        2e-2,
        "frustum wedge mesh band",
    );
}

// =========================================================================
// 3. Edge angles — near-full stays a WEDGE (not the full-turn solid); tiny
// =========================================================================

#[test]
fn near_full_angle_stays_a_wedge() {
    // 2π − 1e-3 is well outside the full-turn band (1e-9), so it must build a
    // 6-vertex WEDGE, NOT collapse into the 2-vertex full-turn solid cylinder.
    let angle = 2.0 * PI - 1e-3;
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, &on_axis_rect_profile(), AXIS_O, AXIS_D, angle)
        .unwrap_or_else(|e| panic!("near-full wedge: {e:?}"));
    assert_wedge_census(&arena, &r, "near-full wedge");
    assert_eq!(
        surface_counts(&arena, r.solid),
        (4, 1, 0),
        "near-full census"
    );
    let vol = geom::signed_volume(&arena, r.solid).expect("near-full wedge volume");
    assert_rel_eq(vol, cyl_wedge_volume(angle), 1e-9, "near-full wedge volume");
    let mesh = tessellate(&arena, r.solid).expect("near-full wedge tessellates");
    assert_watertight(&mesh, "near-full wedge mesh");
}

#[test]
fn tiny_angle_wedge_builds_and_measures() {
    let angle = 1e-3;
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, &on_axis_rect_profile(), AXIS_O, AXIS_D, angle)
        .unwrap_or_else(|e| panic!("tiny wedge: {e:?}"));
    assert_wedge_census(&arena, &r, "tiny wedge");
    let vol = geom::signed_volume(&arena, r.solid).expect("tiny wedge volume");
    assert_rel_eq(vol, cyl_wedge_volume(angle), 1e-9, "tiny wedge volume");
}

// =========================================================================
// 4a. Determinism (I6)
// =========================================================================

#[test]
fn wedge_construction_deterministic() {
    let build = || {
        let mut arena = BrepArena::new();
        revolve(
            &mut arena,
            &on_axis_rect_profile(),
            AXIS_O,
            AXIS_D,
            200.0_f64.to_radians(),
        )
        .expect("wedge");
        arena
    };
    assert_eq!(build(), build(), "wedge revolve must be deterministic");
}

// =========================================================================
// 4b. Typed walls that STAY (spec §2/§5): pre-mutation, unchanged
// =========================================================================

#[test]
fn partial_apex_triangle_stays_axis_intersects_typed() {
    // 3-gon reaching the axis: the apex-on-boundary vocabulary is OUT of this
    // slice, so a partial angle stays typed and pre-mutation.
    let mut arena = BrepArena::new();
    let err = revolve(
        &mut arena,
        &on_axis_triangle_profile(),
        AXIS_O,
        AXIS_D,
        200.0_f64.to_radians(),
    )
    .expect_err("partial apex triangle stays typed");
    assert_eq!(err, KernelV2Error::RevolveAxisIntersectsProfile);
    assert_eq!(arena, BrepArena::new(), "arena untouched after Err");
}

#[test]
fn partial_crossing_profile_stays_rejected() {
    // Rectangle straddling the axis (y ∈ [−1, 1]) — material on both radial
    // sides. A partial angle stays invalid input (crossing, not a wedge).
    let crossing = Profile::new(
        AXIS_O,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, -1.0),
            Point2::new(H, -1.0),
            Point2::new(H, 1.0),
            Point2::new(0.0, 1.0),
        ],
        vec![],
    )
    .expect("crossing profile");
    let mut arena = BrepArena::new();
    let err = revolve(
        &mut arena,
        &crossing,
        AXIS_O,
        AXIS_D,
        200.0_f64.to_radians(),
    )
    .expect_err("partial crossing profile stays rejected");
    assert_eq!(err, KernelV2Error::RevolveAxisIntersectsProfile);
    assert_eq!(arena, BrepArena::new(), "arena untouched after Err");
}

// =========================================================================
// 4c. Full-turn byte-identical (I4): the slice-1 solid cylinder is unchanged
//     — values copied verbatim from
//     `on_axis_rectangle_full_turn_builds_solid_cylinder` (kv6a_revolve.rs).
// =========================================================================

#[test]
fn full_turn_on_axis_rectangle_unchanged() {
    let mut arena = BrepArena::new();
    let r = revolve(
        &mut arena,
        &on_axis_rect_profile(),
        AXIS_O,
        AXIS_D,
        2.0 * PI,
    )
    .expect("on-axis full-turn revolve (the lathe shaft) unchanged");

    let report = validate_solid(&arena, r.solid).expect("shaft validates");
    assert_eq!(report.vertices, 2, "one seam vertex per rim circle");
    assert_eq!(report.edges, 3, "2 rim circles + 1 seam ruling");
    assert_eq!(report.faces, 3, "2 disc caps + 1 lateral");
    assert_eq!(report.shells, 1);
    assert_eq!(report.genus, 0);

    // 2 disc caps + outer lateral cylinder, no arcs (full-circle vocabulary).
    assert_eq!(
        surface_counts(&arena, r.solid),
        (2, 1, 0),
        "full-turn census"
    );

    // Exact volume π·R2²·H = 12π (bitwise-adjacent).
    let vol = geom::signed_volume(&arena, r.solid).expect("analytic shaft volume");
    let want = PI * R2 * R2 * H;
    assert!(
        (vol - want).abs() <= 1e-12 * want,
        "shaft volume {vol} != π·r²·h {want}"
    );
}
