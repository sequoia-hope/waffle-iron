//! PR-KV6c increment 5 ADVERSARY — pathological inputs and structural
//! sanity checks against the just-landed partial-revolve cone patch
//! (spec `specs/kv6c_partial_revolve_cone_patch.md`, FIP §6).
//!
//! This file only ADDS tests; it never touches implementation code. Each test
//! carries numeric/structural asserts that would catch a regression in the
//! construct → validate → signed_volume → tessellate chain or the boolean
//! gate. Helpers are copied in (each test binary is its own crate; there is no
//! shared test-support module — the same convention as
//! `tests/kv6c_partial_cone_patch.rs`).
//!
//! Coverage map (task brief §1):
//!  1. Near-tolerance oblique: Δs just ABOVE the 1e-9·len band → Cone wall;
//!     just BELOW → Parallel/Cylinder wall (not an error).
//!  2. Tiny sweep (1e-3 rad) and near-full sweep (2π − 1e-3): build, validate,
//!     Pappus volume, watertight tessellation.
//!  3. Steep cone (Δs = ~100·Δt) and shallow cone (Δs = Δt·1e-3): build + volume.
//!  4. All three edge classes (parallel + perpendicular + oblique) in one
//!     revolve: census + volume + watertight tessellation.
//!  5. Boolean gate: a partial-cone solid as an operand returns a typed
//!     UnsupportedCurvedBoolean (never a panic, never silent-wrong output),
//!     in both operand positions and for union + subtract.
//!  6. Geometry health: no NaN coordinates, no truly-degenerate (zero-area)
//!     triangles in the tessellation.

use std::f64::consts::{FRAC_PI_2, PI};

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, geom, revolve, tessellate, validate_solid, BrepArena, KernelV2Error,
    Profile, RenderMesh, RevolveResult, SolidId, Surface,
};

const AXIS_O: Point3 = Point3::new(0.0, 0.0, 0.0);
const AXIS_D: Vector3 = Vector3::new(1.0, 0.0, 0.0);

// =========================================================================
// Shared oracle helpers (copied from kv6c_partial_cone_patch.rs /
// kv6a_revolve.rs — test binaries do not share a support module).
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
        assert!(v.is_finite(), "{what}: non-finite position {v}");
    }
    for n in &mesh.normals {
        assert!(n.is_finite(), "{what}: non-finite normal {n}");
    }
    for chunk in mesh.normals.chunks_exact(3) {
        let len = (chunk[0] * chunk[0] + chunk[1] * chunk[1] + chunk[2] * chunk[2]).sqrt();
        assert!(
            (len - 1.0).abs() < 1e-9,
            "{what}: non-unit normal {chunk:?}"
        );
    }
}

/// No triangle in the mesh may be exactly degenerate (zero doubled-area or a
/// repeated vertex position). Slivers within the chord band are fine; a
/// genuinely zero-area facet is not — it signals a collapsed unroll node.
fn assert_no_degenerate_triangles(mesh: &RenderMesh, what: &str) {
    let p = |i: u32| {
        let k = (i as usize) * 3;
        [
            mesh.positions[k],
            mesh.positions[k + 1],
            mesh.positions[k + 2],
        ]
    };
    for (ti, t) in mesh.indices.chunks_exact(3).enumerate() {
        assert!(
            t[0] != t[1] && t[1] != t[2] && t[0] != t[2],
            "{what}: triangle {ti} references a repeated index {t:?}"
        );
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let area2 = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!(
            area2 > 1e-15,
            "{what}: triangle {ti} has zero area ({area2:e}), verts {a:?} {b:?} {c:?}"
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

/// The analytic cone parameters read off a `Surface::Cone` wall (apex and
/// reversed are captured for diagnostics even where a given test only asserts
/// the half-angle).
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
struct ConeParams {
    apex: [f64; 3],
    half_angle: f64,
    reversed: bool,
}

fn cone_walls(arena: &BrepArena, r: &RevolveResult) -> Vec<ConeParams> {
    r.walls
        .iter()
        .filter_map(|&w| match arena.face(w).expect("wall").surface {
            Some(Surface::Cone {
                apex,
                half_angle,
                reversed,
                ..
            }) => Some(ConeParams {
                apex: [apex.x(), apex.y(), apex.z()],
                half_angle,
                reversed,
            }),
            _ => None,
        })
        .collect()
}

/// Census of wall surface types (cones, cylinders, planes) in `walls` order.
fn wall_surface_census(arena: &BrepArena, r: &RevolveResult) -> (usize, usize, usize) {
    let (mut cones, mut cyls, mut planes) = (0, 0, 0);
    for &w in &r.walls {
        match arena.face(w).expect("wall").surface {
            Some(Surface::Cone { .. }) => cones += 1,
            Some(Surface::Cylinder { .. }) => cyls += 1,
            Some(Surface::Plane(_)) => planes += 1,
            _ => {}
        }
    }
    (cones, cyls, planes)
}

/// Reference full-turn volume of a profile (via the already-supported 360°
/// oblique path).
fn full_turn_volume(profile: &Profile) -> f64 {
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, profile, AXIS_O, AXIS_D, 2.0 * PI)
        .expect("full-turn reference revolve builds");
    geom::signed_volume(&arena, r.solid).expect("full-turn reference volume")
}

/// The canonical trapezoid `(s,t) = (1,0),(3,0),(2,1),(1,1)`: one oblique
/// outer edge, an inner radius-1 cylinder, two annular caps — all three edge
/// classes at once. `Point2` is `(axial, radial)`.
fn trapezoid_profile() -> Profile {
    Profile::new(
        AXIS_O,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 1.0),
            Point2::new(0.0, 3.0),
            Point2::new(1.0, 2.0),
            Point2::new(1.0, 1.0),
        ],
        vec![],
    )
    .expect("trapezoid profile")
}

/// A four-vertex profile whose OUTER edge slants by `ds_outer` over unit axial
/// span (from radius 2 at axial 0 to radius `2 + ds_outer` at axial 1); the
/// inner wall is a radius-1 cylinder. Used to probe the classification band.
fn outer_slope_profile(ds_outer: f64) -> Profile {
    Profile::new(
        AXIS_O,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 1.0),
            Point2::new(0.0, 2.0),
            Point2::new(1.0, 2.0 + ds_outer),
            Point2::new(1.0, 1.0),
        ],
        vec![],
    )
    .expect("outer-slope profile")
}

// =========================================================================
// 1. Near-tolerance oblique: ABOVE the band → Cone, BELOW → Cylinder
// =========================================================================

#[test]
fn near_tolerance_oblique_just_above_band_builds_cone() {
    // Δs = 2e-9 over unit length → |Δs| = 2e-9 > 1e-9·len(≈1): classified
    // Oblique, so the wall is a Surface::Cone (a very distant apex, half-angle
    // atan(2e-9)). Build, validate, and confirm the surface TYPE.
    let profile = outer_slope_profile(2e-9);
    let angle = FRAC_PI_2;
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, &profile, AXIS_O, AXIS_D, angle)
        .unwrap_or_else(|e| panic!("near-tolerance-above builds: {e:?}"));
    validate_solid(&arena, r.solid).expect("near-tolerance cone validates");

    let cones = cone_walls(&arena, &r);
    assert_eq!(
        cones.len(),
        1,
        "Δs=2e-9 (> 1e-9·len) classifies Oblique → exactly one Cone wall"
    );
    // The half-angle is atan of the ACTUAL representable slope: `2.0 + 2e-9`
    // rounds (2.0's ULP is ~4.4e-16), so recompute Δs the way the classifier
    // does rather than assuming an exact 2e-9. It must remain a tiny, positive
    // angle just above the 1e-9 alignment band.
    let ds_repr: f64 = (2.0 + 2e-9) - 2.0;
    assert!(
        ds_repr > 1e-9,
        "representable Δs {ds_repr:e} clears the band"
    );
    assert!(
        (cones[0].half_angle - ds_repr.atan()).abs() <= 1e-18,
        "near-tolerance half_angle {} != atan(Δs_repr={ds_repr:e})",
        cones[0].half_angle
    );
    // Volume stays the Pappus fraction of the full-turn reference. NOTE the
    // relaxed tolerance: a half-angle of 2e-9 places the cone apex ~1e9 units
    // away, so the flux formula's τ²≈1e18 intermediate terms lose ~8 digits to
    // catastrophic cancellation. The result is still correct to ~7 significant
    // figures and the error scales monotonically with apex distance (measured:
    // 2.4e-14 rel at Δs=1e-3, 5.1e-8 rel at Δs=2e-9) — expected conditioning of
    // a near-degenerate (all-but-cylindrical) cone, not a formula defect. The
    // well-conditioned cones (shallow/steep/canonical below) hold 1e-9.
    let full = full_turn_volume(&profile);
    let vol = geom::signed_volume(&arena, r.solid).expect("near-tolerance cone volume");
    assert_rel_eq(
        vol,
        angle / (2.0 * PI) * full,
        1e-6,
        "near-tolerance Pappus",
    );
}

#[test]
fn near_tolerance_oblique_just_below_band_builds_cylinder_not_error() {
    // Δs = 5e-10 over unit length → |Δs| = 5e-10 < 1e-9·len: classified
    // Parallel, so the wall is a Surface::Cylinder — NOT an error, NOT a cone.
    let profile = outer_slope_profile(5e-10);
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, &profile, AXIS_O, AXIS_D, FRAC_PI_2)
        .unwrap_or_else(|e| panic!("near-tolerance-below must build (not error): {e:?}"));
    validate_solid(&arena, r.solid).expect("near-tolerance cylinder validates");

    assert!(
        cone_walls(&arena, &r).is_empty(),
        "Δs=5e-10 (< 1e-9·len) is Parallel: no Cone wall"
    );
    let (cones, cyls, _planes) = wall_surface_census(&arena, &r);
    assert_eq!(cones, 0, "sub-band edge is not a cone");
    assert_eq!(
        cyls, 2,
        "both radial walls are cylinders (inner + near-parallel outer)"
    );
}

// =========================================================================
// 2. Tiny sweep and near-full sweep angles
// =========================================================================

#[test]
fn tiny_sweep_angle_builds_validates_and_measures() {
    // A 1e-3 rad wedge — a sliver of the trapezoid solid.
    let angle = 1e-3;
    let full = full_turn_volume(&trapezoid_profile());
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, &trapezoid_profile(), AXIS_O, AXIS_D, angle)
        .unwrap_or_else(|e| panic!("tiny sweep builds: {e:?}"));
    validate_solid(&arena, r.solid).expect("tiny-sweep cone validates");
    assert_eq!(cone_walls(&arena, &r).len(), 1, "one cone wall at 1e-3 rad");

    let vol = geom::signed_volume(&arena, r.solid).expect("tiny-sweep volume");
    assert!(vol > 0.0, "tiny sweep keeps outward orientation");
    assert_rel_eq(vol, angle / (2.0 * PI) * full, 1e-9, "tiny-sweep Pappus");

    let mesh = tessellate(&arena, r.solid).expect("tiny-sweep tessellates");
    assert_mesh_sane(&mesh, "tiny-sweep mesh");
    assert_no_degenerate_triangles(&mesh, "tiny-sweep mesh");
    assert_watertight(&mesh, "tiny-sweep mesh");
}

#[test]
fn near_full_sweep_angle_builds_validates_and_measures() {
    // 2π − 1e-3: an almost-closed revolve that must remain the PARTIAL branch
    // (patch walls, seam caps), never collapse to the full-turn washer.
    let angle = 2.0 * PI - 1e-3;
    let full = full_turn_volume(&trapezoid_profile());
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, &trapezoid_profile(), AXIS_O, AXIS_D, angle)
        .unwrap_or_else(|e| panic!("near-full sweep builds: {e:?}"));
    validate_solid(&arena, r.solid).expect("near-full cone validates");
    assert_eq!(cone_walls(&arena, &r).len(), 1, "one cone wall near 2π");

    let vol = geom::signed_volume(&arena, r.solid).expect("near-full volume");
    assert_rel_eq(vol, angle / (2.0 * PI) * full, 1e-9, "near-full Pappus");

    let mesh = tessellate(&arena, r.solid).expect("near-full tessellates");
    assert_mesh_sane(&mesh, "near-full mesh");
    assert_no_degenerate_triangles(&mesh, "near-full mesh");
    assert_watertight(&mesh, "near-full mesh");
    assert_rel_eq(
        mesh_signed_volume(&mesh),
        angle / (2.0 * PI) * full,
        2e-2,
        "near-full mesh band",
    );
}

// =========================================================================
// 3. Steep and shallow cones
// =========================================================================

/// A near-flat (steep half-angle) cone: outer edge radius 101 → 1.5 over unit
/// axial span (Δs ≈ −99.5, half-angle atan(99.5) ≈ 1.5607 ≈ π/2⁻), inner
/// cylinder r = 1.
fn steep_cone_profile() -> Profile {
    Profile::new(
        AXIS_O,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 1.0),
            Point2::new(0.0, 101.0),
            Point2::new(1.0, 1.5),
            Point2::new(1.0, 1.0),
        ],
        vec![],
    )
    .expect("steep-cone profile")
}

#[test]
fn steep_cone_builds_validates_and_measures() {
    let profile = steep_cone_profile();
    let full = full_turn_volume(&profile);
    let angle = PI;
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, &profile, AXIS_O, AXIS_D, angle)
        .unwrap_or_else(|e| panic!("steep cone builds: {e:?}"));
    validate_solid(&arena, r.solid).expect("steep cone validates");

    let cones = cone_walls(&arena, &r);
    assert_eq!(cones.len(), 1, "one steep cone wall");
    assert!(
        cones[0].half_angle > 1.5 && cones[0].half_angle < FRAC_PI_2,
        "steep half_angle {} is near π/2⁻",
        cones[0].half_angle
    );
    let vol = geom::signed_volume(&arena, r.solid).expect("steep cone volume");
    assert_rel_eq(vol, angle / (2.0 * PI) * full, 1e-9, "steep Pappus");

    let mesh = tessellate(&arena, r.solid).expect("steep cone tessellates");
    assert_mesh_sane(&mesh, "steep cone mesh");
    assert_no_degenerate_triangles(&mesh, "steep cone mesh");
    assert_watertight(&mesh, "steep cone mesh");
}

#[test]
fn shallow_cone_builds_validates_and_measures() {
    // Δs = Δt·1e-3 (radius 2 → 2.001 over unit axial span): a barely-conical
    // wall well above the alignment band. Half-angle atan(1e-3).
    let profile = outer_slope_profile(1e-3);
    let full = full_turn_volume(&profile);
    let angle = PI;
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, &profile, AXIS_O, AXIS_D, angle)
        .unwrap_or_else(|e| panic!("shallow cone builds: {e:?}"));
    validate_solid(&arena, r.solid).expect("shallow cone validates");

    let cones = cone_walls(&arena, &r);
    assert_eq!(cones.len(), 1, "one shallow cone wall");
    assert!(
        (cones[0].half_angle - 1e-3_f64.atan()).abs() <= 1e-12,
        "shallow half_angle {} != atan(1e-3)",
        cones[0].half_angle
    );
    let vol = geom::signed_volume(&arena, r.solid).expect("shallow cone volume");
    assert_rel_eq(vol, angle / (2.0 * PI) * full, 1e-9, "shallow Pappus");

    let mesh = tessellate(&arena, r.solid).expect("shallow cone tessellates");
    assert_mesh_sane(&mesh, "shallow cone mesh");
    assert_no_degenerate_triangles(&mesh, "shallow cone mesh");
    assert_watertight(&mesh, "shallow cone mesh");
}

// =========================================================================
// 4. All three edge classes in one revolve
// =========================================================================

#[test]
fn mixed_edge_classes_census_volume_and_watertight() {
    // The canonical trapezoid carries every class at once: two axis-⊥ caps
    // (Perpendicular → Plane), one radius-1 wall (Parallel → Cylinder), one
    // slant (Oblique → Cone). Census all four walls, then volume + mesh.
    let profile = trapezoid_profile();
    let full = full_turn_volume(&profile);
    let angle = 200.0_f64.to_radians();
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, &profile, AXIS_O, AXIS_D, angle)
        .unwrap_or_else(|e| panic!("mixed-class revolve builds: {e:?}"));
    validate_solid(&arena, r.solid).expect("mixed-class solid validates");

    assert_eq!(r.walls.len(), 4, "one wall per profile edge");
    let (cones, cyls, planes) = wall_surface_census(&arena, &r);
    assert_eq!(cones, 1, "exactly one Cone (the oblique edge)");
    assert_eq!(cyls, 1, "exactly one Cylinder (the parallel edge)");
    assert_eq!(planes, 2, "two Plane caps (the perpendicular edges)");

    let vol = geom::signed_volume(&arena, r.solid).expect("mixed-class volume");
    assert_rel_eq(vol, angle / (2.0 * PI) * full, 1e-9, "mixed-class Pappus");

    let mesh = tessellate(&arena, r.solid).expect("mixed-class tessellates");
    assert_mesh_sane(&mesh, "mixed-class mesh");
    assert_no_degenerate_triangles(&mesh, "mixed-class mesh");
    assert_watertight(&mesh, "mixed-class mesh");
    assert_rel_eq(
        mesh_signed_volume(&mesh),
        angle / (2.0 * PI) * full,
        2e-2,
        "mixed-class mesh band",
    );
}

// =========================================================================
// 5. Boolean gate: partial-cone operand → typed NotSupported, never a panic
// =========================================================================

/// A box solid via extrude, positioned anywhere (the partial-cone conversion
/// trips before any geometry work, so placement is irrelevant).
fn box_solid(arena: &mut BrepArena) -> SolidId {
    let profile = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.4, 1.2),
            Point2::new(0.8, 1.2),
            Point2::new(0.8, 1.7),
            Point2::new(0.4, 1.7),
        ],
        vec![],
    )
    .expect("box profile");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), 0.5)
        .expect("box extrude")
        .solid
}

#[test]
fn partial_cone_operand_is_typed_boolean_wall_both_positions_both_ops() {
    for op in [BoolOp::Union, BoolOp::Subtract, BoolOp::Intersect] {
        // Cone as operand A.
        {
            let mut arena = BrepArena::new();
            let r = revolve(&mut arena, &trapezoid_profile(), AXIS_O, AXIS_D, PI)
                .expect("partial cone builds");
            let b = box_solid(&mut arena);
            let err = boolean_op(&mut arena, r.solid, b, op)
                .expect_err("partial-cone operand A must stay a typed wall");
            assert!(
                matches!(err, KernelV2Error::UnsupportedCurvedBoolean { .. }),
                "operand A {op:?}: expected UnsupportedCurvedBoolean, got {err:?}"
            );
        }
        // Cone as operand B.
        {
            let mut arena = BrepArena::new();
            let b = box_solid(&mut arena);
            let r = revolve(&mut arena, &trapezoid_profile(), AXIS_O, AXIS_D, PI)
                .expect("partial cone builds");
            let err = boolean_op(&mut arena, b, r.solid, op)
                .expect_err("partial-cone operand B must stay a typed wall");
            assert!(
                matches!(err, KernelV2Error::UnsupportedCurvedBoolean { .. }),
                "operand B {op:?}: expected UnsupportedCurvedBoolean, got {err:?}"
            );
        }
    }
}

// =========================================================================
// 6. Geometry health: no NaN, no degenerate triangles across angles/senses
// =========================================================================

#[test]
fn tessellation_geometry_health_across_angles_and_senses() {
    // Outer solid cone, inner conical bore, and a shallow slant — over a
    // spread of angles including near-quadrant. Every mesh must be finite,
    // non-degenerate, and watertight.
    let conical_bore = Profile::new(
        AXIS_O,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 1.0),
            Point2::new(0.0, 4.0),
            Point2::new(1.0, 4.0),
            Point2::new(1.0, 2.0),
        ],
        vec![],
    )
    .expect("conical-bore profile");

    for profile in [trapezoid_profile(), conical_bore, steep_cone_profile()] {
        for angle in [
            1e-2,
            FRAC_PI_2,
            3.0 * FRAC_PI_2,
            200.0_f64.to_radians(),
            2.0 * PI - 1e-2,
        ] {
            let mut arena = BrepArena::new();
            let r = revolve(&mut arena, &profile, AXIS_O, AXIS_D, angle)
                .unwrap_or_else(|e| panic!("health build at {angle}: {e:?}"));
            validate_solid(&arena, r.solid)
                .unwrap_or_else(|e| panic!("health validate at {angle}: {e:?}"));
            let mesh = tessellate(&arena, r.solid)
                .unwrap_or_else(|e| panic!("health tessellate at {angle}: {e:?}"));
            assert_mesh_sane(&mesh, "health mesh");
            assert_no_degenerate_triangles(&mesh, "health mesh");
            assert_watertight(&mesh, "health mesh");
        }
    }
}

// =========================================================================
// 7. Negative guard: an arc that does NOT lie on the declared cone surface
//    must be rejected by validate (closes the arc-radius-agreement gap —
//    the production-tier check `r_arc == τ_c·tan α` in `validate_cone_patch`
//    that no positive/construction test exercises, since construction always
//    emits on-surface arcs).
// =========================================================================

#[test]
fn cone_patch_arc_off_surface_is_rejected_by_validate() {
    // Build a valid partial cone patch, then corrupt ONLY the wall face's
    // declared `half_angle` (a public surface parameter — topology, twins,
    // and every rim arc's stored radius stay intact). The arcs now disagree
    // with the surface `τ_c·tan α`, which `validate_cone_patch` must catch.
    let mut arena = BrepArena::new();
    let r = revolve(&mut arena, &trapezoid_profile(), AXIS_O, AXIS_D, PI)
        .expect("baseline partial cone builds");
    validate_solid(&arena, r.solid).expect("baseline validates before corruption");

    // Locate the single cone wall and read its analytic params.
    let cone_face = *r
        .walls
        .iter()
        .find(|&&w| matches!(arena.face(w).unwrap().surface, Some(Surface::Cone { .. })))
        .expect("one cone wall");
    let Some(Surface::Cone {
        apex,
        axis_dir,
        half_angle,
        reversed,
    }) = arena.face(cone_face).unwrap().surface
    else {
        unreachable!("cone wall carries Surface::Cone");
    };

    // Corrupt the half-angle by 1.5× — the rim arcs (radius 2 and 3, unchanged)
    // no longer satisfy `r = τ_c·tan α`. The disagreement is O(1), far above
    // the 1e-9 band and far below any weakened 1e9 tolerance.
    arena.face_mut(cone_face).unwrap().surface = Some(Surface::Cone {
        apex,
        axis_dir,
        half_angle: half_angle * 1.5,
        reversed,
    });

    let err = validate_solid(&arena, r.solid)
        .expect_err("an off-surface cone-patch arc must fail validation");
    assert!(
        matches!(err, KernelV2Error::CurvedGeometryMismatch { face, .. } if face == cone_face),
        "expected CurvedGeometryMismatch on the cone wall, got {err:?}"
    );
}
