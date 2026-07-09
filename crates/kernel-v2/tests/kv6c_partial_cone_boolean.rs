//! PR-KV6c increment 2 RED — booleans over partial-revolve CONE solids
//! (spec `specs/kv6c_partial_revolve_cone_patch.md`, §2 "boolean chain" row +
//! §4 "Boolean chain" oracle).
//!
//! Increment 1 (kernel-v2) shipped: a partial revolve of an oblique edge now
//! BUILDS an arc-bounded `Surface::Cone` wall. This suite pins increment 2 —
//! such solids entering yang booleans:
//!   - kernel-v2 `boolean.rs` currently gates a partial cone wall at
//!     conversion (`to_yang_brep`), so ANY boolean with such an operand
//!     returns `KernelV2Error::UnsupportedCurvedBoolean` (typed). That gate is
//!     what increment 2 removes.
//!   - yang-rs `tessellate_cone_face` currently rejects arc-bounded cone faces
//!     (the partial STRIP arm is the increment-2 addition).
//!
//! RED PHASE: the positive tests (1, 2, 3) all fail today at `boolean_op` with
//! `UnsupportedCurvedBoolean` (the conversion gate) — construction of the cone
//! operand itself succeeds (increment 1). The oblique-section boundary probe
//! (4) is `#[ignore]`d — it is not red-able (it passes today via the typed Err
//! arm) and its Ok arm needs an exact oracle the ellipse-section vocabulary
//! does not yet admit.
//!
//! Structure and helpers mirror `tests/kv6b_revolve_boolean.rs` (contained-box
//! volume oracles, the inscribed-chord band).

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, revolve, tessellate, validate_solid, BrepArena, KernelV2Error, Profile,
    RenderMesh, RevolveResult, SolidId, Surface,
};

const AXIS_O: Point3 = Point3::new(0.0, 0.0, 0.0);
const AXIS_D: Vector3 = Vector3::new(1.0, 0.0, 0.0);

/// The spec §4 canonical partial angle: 200° (a non-quadrant, reflex sweep).
fn canon_angle() -> f64 {
    200.0_f64.to_radians()
}

// =========================================================================
// Fixtures — the canonical trapezoid cone solid + box operands.
// =========================================================================

/// The spec §4 trapezoid `(s,t) = (1,0),(3,0),(2,1),(1,1)` as
/// `Point2(axial, radial)`: inner radius-1 cylinder, outer cone (radius 3→2
/// over axial 0→1), two annular-sector caps. Full-turn solid-of-revolution
/// volume = π·16/3; the partial `angle` fraction is `angle·8/3`.
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

fn revolve_cone(arena: &mut BrepArena, angle: f64) -> RevolveResult {
    revolve(arena, &trapezoid_profile(), AXIS_O, AXIS_D, angle)
        .unwrap_or_else(|e| panic!("partial cone revolve({angle}) builds (increment 1): {e:?}"))
}

/// Partial solid-of-revolution volume of the trapezoid = `(angle/2π)·π·16/3`.
fn trapezoid_partial_volume(angle: f64) -> f64 {
    angle * 8.0 / 3.0
}

/// Axis-aligned box via extrude: base rectangle [x0,x1]×[y0,y1] at z = z0,
/// extruded +z to z1 (x is the axial direction, y/z span the radial plane).
fn box_solid(arena: &mut BrepArena, x: (f64, f64), y: (f64, f64), z: (f64, f64)) -> SolidId {
    let profile = Profile::new(
        Point3::new(0.0, 0.0, z.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(x.0, y.0),
            Point2::new(x.1, y.0),
            Point2::new(x.1, y.1),
            Point2::new(x.0, y.1),
        ],
        vec![],
    )
    .expect("box profile");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), z.1 - z.0)
        .expect("box extrude")
        .solid
}

// =========================================================================
// Shared oracle helpers (copied from kv6b_revolve_boolean.rs — each test
// binary is its own crate; there is no shared test-support module).
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

/// Watertightness by POSITION-keyed directed-edge pairing (keys quantized at
/// 1e-9, far below any feature scale).
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

/// Mesh volume must land in `[0.95·expect, 1.001·expect]` — the inscribed-
/// chord band at the render d_ε (curved hulls under-estimate; the +0.1%
/// ceiling absorbs planar rounding). Mirrors kv6b's `assert_volume_band`.
fn assert_volume_band(actual: f64, expect: f64, what: &str) {
    assert!(
        actual <= expect * 1.001 && actual >= 0.95 * expect,
        "{what}: volume {actual} vs expected {expect}"
    );
}

/// True when the solid carries at least one `Surface::Cone` face — the cone
/// wall survived the boolean.
fn has_cone_wall(arena: &BrepArena, solid: SolidId) -> bool {
    arena.solid(solid).expect("solid").shells.iter().any(|&sh| {
        arena.shell(sh).expect("shell").faces.iter().any(|&fc| {
            matches!(
                arena.face(fc).expect("face").surface,
                Some(Surface::Cone { .. })
            )
        })
    })
}

// =========================================================================
// 1. Canonical wall-crossing chain: subtract an axial slab that truncates
//    the cone solid at x = 0.5. The slab's x = 0.5 face is ⊥ the cone axis,
//    so where it meets the cone lateral it is a CIRCLE arc (the KV6c 5c
//    supported plane×cone section). Exact remaining volume.
// =========================================================================

#[test]
fn subtract_axial_slab_truncates_cone_exact_volume() {
    let angle = canon_angle();
    let mut arena = BrepArena::new();
    let cone = revolve_cone(&mut arena, angle);

    // Box spans x∈[0.5, 1.5], y,z∈[−3, 3]: it fully contains the solid's
    // x ≥ 0.5 material (radial ≤ 3−x ≤ 2.5 < 3 at every azimuth of the 200°
    // sweep), so the subtract truncates the solid to x∈[0, 0.5].
    let slab = box_solid(&mut arena, (0.5, 1.5), (-3.0, 3.0), (-3.0, 3.0));
    let out = boolean_op(&mut arena, cone.solid, slab, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("cone − axial slab (5c circle section): {e:?}"));

    let report = validate_solid(&arena, out).expect("truncated cone validates");
    assert_eq!(report.shells, 1, "a through-cut leaves one shell");
    assert!(
        has_cone_wall(&arena, out),
        "the truncated cone lateral survives the cut"
    );

    // Exact remaining volume = (angle/2π)·π·∫₀^0.5((3−x)²−1)dx = angle·79/48.
    let expect = angle * 79.0 / 48.0;
    let mesh = tessellate(&arena, out).expect("truncated cone tessellates");
    assert_watertight(&mesh, "truncated cone mesh");
    assert_volume_band(mesh_signed_volume(&mesh), expect, "truncated cone volume");
}

// =========================================================================
// 2. Contained-box chains: union identity + subtract cavity. The box never
//    touches the cone wall (radius/angle band membership verified below), so
//    the volumes are exact up to the render chord band. These pin that a
//    partial-cone OPERAND converts and booleans at all (the gate that is red
//    today), independent of the SSI.
// =========================================================================

#[test]
fn union_with_contained_box_is_identity_volume() {
    let angle = canon_angle();
    let mut arena = BrepArena::new();
    let cone = revolve_cone(&mut arena, angle);
    // Box x∈[0.2,0.7], y∈[1.2,1.7], z∈[0.2,0.5]: radial √(y²+z²)∈[1.22,1.77]
    // ⊂ (1, 3−x) with 3−x ≥ 2.3, azimuth atan2(z,y)∈[6.7°,22.6°] ⊂ [0,200°],
    // x ⊂ [0,1] — fully interior, so A ∪ B = A.
    let b = box_solid(&mut arena, (0.2, 0.7), (1.2, 1.7), (0.2, 0.5));
    let out = boolean_op(&mut arena, cone.solid, b, BoolOp::Union)
        .unwrap_or_else(|e| panic!("cone ∪ contained box: {e:?}"));
    validate_solid(&arena, out).expect("union validates");
    let mesh = tessellate(&arena, out).expect("tessellate union");
    assert_volume_band(
        mesh_signed_volume(&mesh),
        trapezoid_partial_volume(angle),
        "cone ∪ contained box (identity)",
    );
}

#[test]
fn subtract_midsweep_contained_box_leaves_cavity() {
    // Item 2: a box wholly interior in θ (mid-sweep, ~θ=100°) subtracts to an
    // enclosed void — a second shell — with the cone hull and its walls
    // surviving. Box y∈[−0.4,−0.15], z∈[1.45,1.7]: radial∈[1.46,1.75],
    // azimuth atan2(z,y)∈[95°,105°] ⊂ (0,200°), x∈[0.3,0.6] ⊂ (0,1); every
    // corner has radial < 3−x, so the box floats free of both walls.
    let angle = canon_angle();
    let mut arena = BrepArena::new();
    let cone = revolve_cone(&mut arena, angle);
    let b = box_solid(&mut arena, (0.3, 0.6), (-0.4, -0.15), (1.45, 1.7));
    let box_vol = 0.3 * 0.25 * 0.25; // = 0.01875

    let out = boolean_op(&mut arena, cone.solid, b, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("cone − mid-sweep box: {e:?}"));
    let report = validate_solid(&arena, out).expect("cavity cut validates");
    assert_eq!(report.shells, 2, "interior box void is a second shell");
    assert!(
        has_cone_wall(&arena, out),
        "the cone hull wall survives the cavity cut"
    );

    let mesh = tessellate(&arena, out).expect("tessellate cavity cut");
    assert_watertight(&mesh, "cavity cut mesh");
    // The void subtracts exactly; the curved hull keeps its chord band.
    let solid_a = trapezoid_partial_volume(angle);
    let vol = mesh_signed_volume(&mesh);
    assert!(
        vol <= (solid_a - box_vol) * 1.001 && vol >= 0.95 * solid_a - box_vol,
        "cavity cut volume {vol} vs solid {solid_a} − box {box_vol}"
    );
}

// =========================================================================
// 3. Determinism: the cone-operand boolean is bit-reproducible.
// =========================================================================

#[test]
fn cone_boolean_deterministic() {
    let build = || {
        let mut arena = BrepArena::new();
        let cone = revolve_cone(&mut arena, canon_angle());
        let b = box_solid(&mut arena, (0.2, 0.7), (1.2, 1.7), (0.2, 0.5));
        let out = boolean_op(&mut arena, cone.solid, b, BoolOp::Subtract).expect("cut");
        let mesh = tessellate(&arena, out).expect("tessellate");
        (arena, mesh)
    };
    let (a1, m1) = build();
    let (a2, m2) = build();
    assert_eq!(a1, a2, "bit-identical arenas");
    assert_eq!(m1, m2, "bit-identical meshes");
}

// =========================================================================
// 4. Boundary probe (NOT a branch oracle): an OBLIQUE plane×cone section.
//
//    A slab whose cut face is tilted 45° to the cone axis (normal (1,1,0)/√2
//    — neither ⊥ nor ∥ the axis) crosses the outer cone lateral along an
//    ELLIPSE arc. Per spec §2 that oblique conic section is expected to stay a
//    typed `CurvedGeometryMismatch` (the ellipse-section vocabulary is a later
//    slice). This is a BOUNDARY PROBE, not a two-way branch oracle:
//      - Err  → the error MUST be typed (never a panic / opaque failure).
//      - Ok   → the kernel claims support, so the result MUST be correct:
//               validated, watertight, and volume in the physical subtract
//               bracket (0, vol(A)].
//    `#[ignore]`d: it is not red-able (the conversion gate makes it pass today
//    via the Err arm) and the Ok arm's bracket is a SANITY ceiling, not the
//    exact oracle a supported ellipse section would require. Un-ignore and
//    replace the Ok bracket with an exact volume when that wall is measured.
// =========================================================================

/// A thick slab whose bounding face passes through `P0` with unit normal
/// (1,1,0)/√2 (45° to the +x cone axis), extruded 8 units along that normal.
fn oblique_slab(arena: &mut BrepArena, p0: Point3) -> SolidId {
    let inv = 1.0 / 2.0_f64.sqrt();
    // Orthonormal in-plane basis: u = ẑ, v = (1,−1,0)/√2; u × v = (1,1,0)/√2.
    let u = Vector3::new(0.0, 0.0, 1.0);
    let v = Vector3::new(inv, -inv, 0.0);
    let n = Vector3::new(inv, inv, 0.0);
    let profile = Profile::new(
        p0,
        u,
        v,
        vec![
            Point2::new(-4.0, -4.0),
            Point2::new(4.0, -4.0),
            Point2::new(4.0, 4.0),
            Point2::new(-4.0, 4.0),
        ],
        vec![],
    )
    .expect("oblique slab profile");
    extrude(arena, &profile, n, 8.0)
        .expect("oblique slab extrude")
        .solid
}

#[test]
#[ignore = "KV6c5b boundary probe — oblique plane×cone (ellipse) section; \
            un-ignore with an exact volume oracle when the ellipse-section \
            wall is measured (spec §2 CurvedGeometryMismatch row)"]
fn oblique_section_stays_typed_or_exact_boundary_probe() {
    let angle = canon_angle();
    let mut arena = BrepArena::new();
    let cone = revolve_cone(&mut arena, angle);
    // P0 = (0.4, 0, 0): the tilted face slices the outer cone obliquely, so
    // the section is an ellipse arc, not a circle.
    let slab = oblique_slab(&mut arena, Point3::new(0.4, 0.0, 0.0));
    let solid_a = trapezoid_partial_volume(angle);

    match boolean_op(&mut arena, cone.solid, slab, BoolOp::Subtract) {
        Ok(out) => {
            // The kernel claims support: demand a correct solid. (This bracket
            // is a sanity ceiling — replace with the exact ellipse-section
            // volume before treating oblique sections as a supported class.)
            validate_solid(&arena, out).expect("oblique cut must validate if it succeeds");
            let mesh = tessellate(&arena, out).expect("oblique cut must tessellate if it succeeds");
            assert_watertight(&mesh, "oblique cut mesh");
            let v = mesh_signed_volume(&mesh).abs();
            assert!(
                v > 0.0 && v <= solid_a * 1.001,
                "oblique subtract volume {v} outside the physical bracket (0, {solid_a}]"
            );
        }
        Err(e) => assert!(
            matches!(
                e,
                KernelV2Error::CurvedGeometryMismatch { .. }
                    | KernelV2Error::UnsupportedCurvedBoolean { .. }
                    | KernelV2Error::BooleanFailed(_)
            ),
            "oblique cone section must stay a TYPED wall, got {e:?}"
        ),
    }
}

// =========================================================================
// KV7 curved∩curved coaxial rim recovery. A profile whose OBLIQUE (cone)
// edge is adjacent to a PARALLEL (cylinder) edge produces a coaxial
// cone∩cylinder shared rim. The mesh boolean leaves that rim as a chord
// polyline; `recover::recover_output_curves` must retag it to the exact
// shared circle (neither face is a plane, so the ⊥-plane retag cannot).
// Without the recovery a thin such band folds at render (the KV9-F2
// "patch triangulation folded" class — assay R0034/R0065). Here the band
// is not thin, so the oracle is structural: identity union round-trips
// watertight at the exact partial-revolution volume with BOTH analytic
// walls preserved.
// =========================================================================

/// Profile `(axial, radial)` = (0,1),(0,3),(1,2),(2,2),(2,1): an inner
/// radius-1 cylinder, a cone (radius 3→2 over axial 0→1) whose top rim at
/// (axial 1, radius 2) is SHARED with an outer radius-2 cylinder (axial
/// 1→2), plus two annular caps. Full-turn volume = π·25/3.
fn double_band_profile() -> Profile {
    Profile::new(
        AXIS_O,
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 1.0),
            Point2::new(0.0, 3.0),
            Point2::new(1.0, 2.0),
            Point2::new(2.0, 2.0),
            Point2::new(2.0, 1.0),
        ],
        vec![],
    )
    .expect("double-band profile")
}

fn has_cylinder_wall(arena: &BrepArena, solid: SolidId) -> bool {
    arena.solid(solid).expect("solid").shells.iter().any(|&sh| {
        arena.shell(sh).expect("shell").faces.iter().any(|&fc| {
            matches!(
                arena.face(fc).expect("face").surface,
                Some(Surface::Cylinder { .. })
            )
        })
    })
}

#[test]
fn coaxial_cone_cylinder_rim_recovers_through_boolean() {
    let angle = canon_angle();
    let mut arena = BrepArena::new();
    let band = revolve(&mut arena, &double_band_profile(), AXIS_O, AXIS_D, angle)
        .expect("double-band partial revolve builds");
    assert!(has_cone_wall(&arena, band.solid), "cone band built");
    assert!(has_cylinder_wall(&arena, band.solid), "cylinder band built");

    // Contained box (identical bracket to the single-cone identity test):
    // radial ∈ [1.22,1.77] ⊂ (1, 3−x), azimuth ∈ [6.7°,22.6°] ⊂ (0,200°),
    // x ⊂ [0.2,0.7] — fully interior, so A ∪ B = A. The union routes the
    // two-band solid through the boolean, turning the coaxial cone∩cylinder
    // rim into a chord polyline that recovery must restore.
    let b = box_solid(&mut arena, (0.2, 0.7), (1.2, 1.7), (0.2, 0.5));
    let out = boolean_op(&mut arena, band.solid, b, BoolOp::Union)
        .unwrap_or_else(|e| panic!("double-band ∪ contained box: {e:?}"));

    validate_solid(&arena, out).expect("identity union validates");
    assert!(
        has_cone_wall(&arena, out) && has_cylinder_wall(&arena, out),
        "both analytic walls survive the boolean (surface tier preserved, A15.5)"
    );

    // The recovered coaxial rim lets the cone patch tessellate on-surface
    // (no folded chord-midpoint sliver) — watertight at the exact volume.
    let mesh = tessellate(&arena, out).expect("tessellate identity union");
    assert_watertight(&mesh, "double-band identity union mesh");
    let expect = angle * 25.0 / 6.0; // (angle/2π)·π·25/3
    assert_volume_band(
        mesh_signed_volume(&mesh),
        expect,
        "double-band identity volume",
    );
}
