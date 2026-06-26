//! PR-KV6b-2 RED — booleans over kernel-v2 REVOLVE solids.
//!
//! KV6b-1 taught yang-rs Stage 1 the revolve vocabulary (directional input
//! arcs, partial cylinder patches, annular sectors, holed circle caps,
//! reversed input walls). This suite pins the kernel-v2 side: `to_yang_brep`
//! converts revolve solids (today: `UnsupportedCurvedBoolean` — every test
//! here fails RED) and the end-to-end booleans hold analytic volume.
//!
//! Volume design: the box operands are placed FULLY INSIDE the revolve
//! solids (radius/angle band membership verified in comments), so the
//! expected volumes are exact (`A ∪ B = A`, `A − B = vol(A) − vol(B)`)
//! up to the inscribed-chord band of the render mesh.
//!
//! What REMAINS walled after GREEN (pinned here):
//! - boolean-OUTPUT re-entry: result solids carry partial patches whose
//!   curved boundaries are untagged chord polylines — same wall as kv5b's
//!   `curved_result_reentry_is_typed_wall`.
//! - cylinder×cylinder SSI (M5), oblique ellipse sections: unchanged.

use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extrude, geom, revolve, tessellate, to_yang_brep, validate_solid, BrepArena,
    KernelV2Error, Profile, RenderMesh, RevolveResult, Surface,
};

const R1: f64 = 1.0;
const R2: f64 = 2.0;
const H: f64 = 3.0;

fn rect_profile() -> Profile {
    Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, R1),
            Point2::new(H, R1),
            Point2::new(H, R2),
            Point2::new(0.0, R2),
        ],
        vec![],
    )
    .expect("rect profile")
}

fn revolve_rect(arena: &mut BrepArena, angle: f64) -> RevolveResult {
    revolve(
        arena,
        &rect_profile(),
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        angle,
    )
    .unwrap_or_else(|e| panic!("revolve({angle}): {e:?}"))
}

/// Box solid via extrude: base rectangle [x0,x1]×[y0,y1] at z = z0, height.
fn box_solid(
    arena: &mut BrepArena,
    x: (f64, f64),
    y: (f64, f64),
    z: (f64, f64),
) -> kernel_v2::SolidId {
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

fn pappus(angle: f64) -> f64 {
    angle * (R2 * R2 - R1 * R1) * H / 2.0
}

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

/// Mesh volume must land in `[0.95·expect, 1.001·expect]` — the inscribed-
/// chord band at the Stage-1/render d_ε (see the KV6b-1 calibration note).
fn assert_volume_band(actual: f64, expect: f64, what: &str) {
    assert!(
        actual <= expect * 1.001 && actual >= 0.95 * expect,
        "{what}: volume {actual} vs expected {expect}"
    );
}

// =========================================================================
// 1. Conversion: revolve operands enter yang (flips the KV6a wall)
// =========================================================================

#[test]
fn revolve_solids_convert_to_yang_brep() {
    for angle in [PI / 2.0, PI, 350.0_f64.to_radians(), 2.0 * PI] {
        let mut arena = BrepArena::new();
        let r = revolve_rect(&mut arena, angle);
        let ybrep = to_yang_brep(&arena, r.solid)
            .unwrap_or_else(|e| panic!("to_yang_brep at {angle}: {e:?}"));
        assert!(!ybrep.faces().is_empty());
    }
}

// =========================================================================
// 2. Union with a fully-contained box: A ∪ B = A exactly
// =========================================================================

#[test]
fn union_with_contained_box_is_identity_volume() {
    // Box (y,z) corners span radii 1.217..1.838 and angles 0°..30° — fully
    // inside the 90° sweep annulus band [R1, R2] × [0°, 90°], x ⊂ [0, H].
    let mut arena = BrepArena::new();
    let r = revolve_rect(&mut arena, PI / 2.0);
    let b = box_solid(&mut arena, (1.0, 1.8), (1.2, 1.7), (0.2, 0.7));
    let out = boolean_op(&mut arena, r.solid, b, BoolOp::Union)
        .unwrap_or_else(|e| panic!("revolve ∪ contained box: {e:?}"));
    validate_solid(&arena, out).expect("union validates");
    let mesh = tessellate(&arena, out).expect("tessellate union");
    assert_volume_band(mesh_signed_volume(&mesh), pappus(PI / 2.0), "union volume");
}

// =========================================================================
// 3. Subtract a fully-contained box: vol = Pappus − box (cavity census)
// =========================================================================

#[test]
fn subtract_contained_box_volume_and_cavity() {
    let mut arena = BrepArena::new();
    let r = revolve_rect(&mut arena, PI / 2.0);
    let b = box_solid(&mut arena, (1.0, 1.8), (1.2, 1.7), (0.2, 0.7));
    let box_vol = 0.8 * 0.5 * 0.5;
    let out = boolean_op(&mut arena, r.solid, b, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("revolve − contained box: {e:?}"));
    let report = validate_solid(&arena, out).expect("cut validates");
    // Enclosed box cavity: a second closed shell.
    assert_eq!(report.shells, 2, "interior cavity is a second shell");
    let mesh = tessellate(&arena, out).expect("tessellate cut");
    // The cavity subtracts exactly; the revolve hull keeps its chord band.
    let vol = mesh_signed_volume(&mesh);
    let expect_hi = pappus(PI / 2.0) - box_vol;
    assert!(
        vol <= expect_hi * 1.001 && vol >= 0.95 * pappus(PI / 2.0) - box_vol,
        "cut volume {vol} vs {expect_hi}"
    );
}

// =========================================================================
// 3t. KV6d-5a: partial-torus operand booleans through the full adapter,
//     guarded by volume conservation (the band-render fix made these correct).
// =========================================================================

/// A circle profile revolved by `angle` → a partial torus (major 3, minor 1,
/// axis +x), built through the same `revolve` path the corpus uses.
fn revolve_torus(arena: &mut BrepArena, angle: f64) -> RevolveResult {
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 3.0),
        1.0,
    )
    .expect("circle profile");
    revolve(
        arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        angle,
    )
    .expect("partial torus revolve")
}

/// The torus tube volume (Pappus): α·R·π·r², here R=3, r=1.
fn torus_pappus(angle: f64) -> f64 {
    angle * 3.0 * std::f64::consts::PI
}

#[test]
fn torus_operand_converts_to_yang() {
    // KV6d-5a: the to_yang torus wall is gone — a Surface::Torus operand now
    // converts (two profile circles + a seam-arc twin pair → a yang Torus face).
    let mut arena = BrepArena::new();
    let r = revolve_torus(&mut arena, PI / 2.0);
    let yb = to_yang_brep(&arena, r.solid).expect("torus operand converts to yang (5a)");
    assert!(
        yb.faces()
            .iter()
            .any(|f| matches!(f.surface, yang_rs::Surface::Torus { .. })),
        "the converted yang B-Rep carries a Surface::Torus face"
    );
}

#[test]
fn torus_union_contained_box_conserves_volume() {
    // A box fully inside the 90° tube (centre arc (0, 3cosθ, 3sinθ), minor 1;
    // at θ=45° the centre ≈ (0, 2.12, 2.12)). A ∪ B = A, so the result keeps the
    // full tube volume. PRE band-render fix this rendered a thin sliver (~4.3);
    // the volume guard (⊇ A) now passes only because the band renders fully.
    let mut arena = BrepArena::new();
    let r = revolve_torus(&mut arena, PI / 2.0);
    let b = box_solid(&mut arena, (-0.3, 0.3), (1.85, 2.4), (1.85, 2.4));
    let out = boolean_op(&mut arena, r.solid, b, BoolOp::Union)
        .unwrap_or_else(|e| panic!("torus ∪ contained box: {e:?}"));
    validate_solid(&arena, out).expect("union validates");
    let vol = mesh_signed_volume(&tessellate(&arena, out).expect("tessellate")).abs();
    assert_volume_band(vol, torus_pappus(PI / 2.0), "torus ∪ contained box");
}

#[test]
fn torus_subtract_contained_box_conserves_volume() {
    // A − B leaves the tube with an interior box void: vol = tube − box, two
    // shells. The subtract bound (⊆ A) plus the box-void lower bound catch the
    // pre-fix thin sliver (~4.5).
    let mut arena = BrepArena::new();
    let r = revolve_torus(&mut arena, PI / 2.0);
    let b = box_solid(&mut arena, (-0.3, 0.3), (1.85, 2.4), (1.85, 2.4));
    let box_vol = 0.6 * 0.55 * 0.55;
    let out = boolean_op(&mut arena, r.solid, b, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("torus − contained box: {e:?}"));
    let report = validate_solid(&arena, out).expect("cut validates");
    assert_eq!(report.shells, 2, "interior box void is a second shell");
    let vol = mesh_signed_volume(&tessellate(&arena, out).expect("tessellate")).abs();
    let tube = torus_pappus(PI / 2.0);
    // ⊆ A (chord-banded) and ≈ tube − box (not a thin sliver).
    assert!(
        vol <= tube * 1.001 && vol >= 0.95 * tube - box_vol,
        "torus − box volume {vol} vs tube {tube} − box {box_vol}"
    );
}

// =========================================================================
// 4. The exactly-π and major-arc operands
// =========================================================================

#[test]
fn pi_and_major_arc_operands_boolean_cleanly() {
    for angle in [PI, 350.0_f64.to_radians()] {
        let mut arena = BrepArena::new();
        let r = revolve_rect(&mut arena, angle);
        // Same contained box (its angular span 0°..30° is inside any sweep
        // ≥ 90°).
        let b = box_solid(&mut arena, (1.0, 1.8), (1.2, 1.7), (0.2, 0.7));
        let out = boolean_op(&mut arena, r.solid, b, BoolOp::Union)
            .unwrap_or_else(|e| panic!("union at {angle}: {e:?}"));
        validate_solid(&arena, out).expect("validates");
        let mesh = tessellate(&arena, out).expect("tessellate");
        assert_volume_band(mesh_signed_volume(&mesh), pappus(angle), "π/major union");
    }
}

// =========================================================================
// 5. The washer: union + subtract (reversed inner tube rides through)
// =========================================================================

#[test]
fn washer_boolean_with_contained_box() {
    // Box (y,z) ∈ [1.2, 1.7] × [−0.25, 0.25]: radius band 1.22..1.72 ⊂
    // (R1, R2), all angles covered by the full turn; x ∈ [1, 2] ⊂ [0, H].
    let mut arena = BrepArena::new();
    let r = revolve_rect(&mut arena, 2.0 * PI);
    let b = box_solid(&mut arena, (1.0, 2.0), (1.2, 1.7), (-0.25, 0.25));
    let box_vol = 1.0 * 0.5 * 0.5;

    let cut = boolean_op(&mut arena, r.solid, b, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("washer − box: {e:?}"));
    let report = validate_solid(&arena, cut).expect("washer cut validates");
    assert_eq!(report.shells, 2, "enclosed cavity shell");
    // The washer's own inner tube must KEEP its cavity sense.
    let mut kept_reversed_r1 = 0;
    for &sh in &arena.solid(cut).unwrap().shells {
        for &fid in &arena.shell(sh).unwrap().faces {
            if let Some(Surface::Cylinder {
                radius, reversed, ..
            }) = arena.face(fid).unwrap().surface
            {
                if (radius - R1).abs() < 1e-9 && reversed {
                    kept_reversed_r1 += 1;
                }
            }
        }
    }
    assert!(
        kept_reversed_r1 > 0,
        "washer inner tube must keep reversed: true through the boolean"
    );
    let mesh = tessellate(&arena, cut).expect("tessellate washer cut");
    let vol = mesh_signed_volume(&mesh);
    let expect_hi = pappus(2.0 * PI) - box_vol;
    assert!(
        vol <= expect_hi * 1.001 && vol >= 0.95 * pappus(2.0 * PI) - box_vol,
        "washer cut volume {vol} vs {expect_hi}"
    );
}

// =========================================================================
// 6. Analytic volume of boolean outputs stays loud-or-exact (no regression)
// =========================================================================

#[test]
fn analytic_volume_on_revolve_inputs_still_exact() {
    // signed_volume must keep working on the INPUT solids after any
    // conversion-related refactor (guards accidental coupling).
    let mut arena = BrepArena::new();
    let r = revolve_rect(&mut arena, PI / 2.0);
    let v = geom::signed_volume(&arena, r.solid).expect("analytic volume");
    assert!((v - pappus(PI / 2.0)).abs() <= 1e-12 * pappus(PI / 2.0));
}

// =========================================================================
// 7. Determinism
// =========================================================================

#[test]
fn revolve_boolean_deterministic() {
    let build = || {
        let mut arena = BrepArena::new();
        let r = revolve_rect(&mut arena, PI / 2.0);
        let b = box_solid(&mut arena, (1.0, 1.8), (1.2, 1.7), (0.2, 0.7));
        let out = boolean_op(&mut arena, r.solid, b, BoolOp::Subtract).expect("cut");
        let mesh = tessellate(&arena, out).expect("tessellate");
        (arena, mesh)
    };
    let (a1, m1) = build();
    let (a2, m2) = build();
    assert_eq!(a1, a2, "bit-identical arenas");
    assert_eq!(m1, m2, "bit-identical meshes");
}

// =========================================================================
// 8. What remains walled: OUTPUT re-entry (chord-polyline boundaries)
// =========================================================================

#[test]
fn revolve_boolean_output_reentry_stays_typed_wall() {
    let mut arena = BrepArena::new();
    let r = revolve_rect(&mut arena, PI / 2.0);
    let b = box_solid(&mut arena, (1.0, 1.8), (1.2, 1.7), (0.2, 0.7));
    let out = boolean_op(&mut arena, r.solid, b, BoolOp::Subtract)
        .unwrap_or_else(|e| panic!("first boolean: {e:?}"));
    // PR-KV7: the pocket box is fully interior, so the first output is a
    // TWO-SHELL solid (wedge + internal void). Output curve recovery
    // removed the chord-polyline wall that used to shadow this, exposing
    // the real boundary: yang's flat-face-list input has no shell
    // structure and its reassembly cannot rebuild voids — the typed
    // multi-shell wall.
    let b2 = box_solid(&mut arena, (10.0, 11.0), (10.0, 11.0), (10.0, 11.0));
    let err =
        boolean_op(&mut arena, out, b2, BoolOp::Union).expect_err("output re-entry stays walled");
    assert!(
        matches!(
            err,
            KernelV2Error::UnsupportedMultiShellBoolean { shells: 2 }
        ),
        "typed multi-shell wall, got {err:?}"
    );
}

// =========================================================================
// 9. Legacy-trait adapter end-to-end (flips the kv6a adapter wall pin)
// =========================================================================

mod adapter {
    use super::*;
    use kernel_v2::KernelV2Adapter;
    use std::collections::HashMap;
    use waffle_types::kernel::{ClosedProfile, Kernel, KernelError};

    fn stage_rect(
        k: &mut KernelV2Adapter,
        pts: [(f64, f64); 4],
        plane_origin: [f64; 3],
    ) -> waffle_types::kernel::KernelId {
        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let mut positions = HashMap::new();
        for (i, &(x, y)) in pts.iter().enumerate() {
            positions.insert((i + 1) as u32, (x, y));
        }
        k.make_faces_from_profiles(
            &[profile],
            plane_origin,
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        )
        .expect("stage profile")[0]
    }

    #[test]
    fn adapter_revolve_union_box() {
        let mut k = KernelV2Adapter::new();
        let face = stage_rect(&mut k, [(0.0, R1), (H, R1), (H, R2), (0.0, R2)], [0.0; 3]);
        let rev = k
            .revolve_face(face, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 90.0)
            .expect("revolve");
        let face2 = stage_rect(
            &mut k,
            [(1.0, 1.2), (1.8, 1.2), (1.8, 1.7), (1.0, 1.7)],
            [0.0, 0.0, 0.2],
        );
        let bx = k.extrude_face(face2, [0.0, 0.0, 1.0], 0.5).expect("box");
        let out = k
            .boolean_union(&rev, &bx)
            .unwrap_or_else(|e| panic!("adapter revolve ∪ box: {e:?}"));
        let mesh = k.tessellate(&out, 0.001).expect("tessellate");
        assert!(!mesh.indices.is_empty());
        // KernelError mapping sanity: NotSupported must NOT have fired.
        let _ = KernelError::EntityNotFound {
            id: waffle_types::kernel::KernelId(0),
        };
    }
}
