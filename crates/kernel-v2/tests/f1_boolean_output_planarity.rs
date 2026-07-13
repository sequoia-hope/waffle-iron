//! F1 (design review 2026-07-12): PRODUCTION planarity gate for
//! boolean-path solids.
//!
//! The defect class this pins (F0064/R0051/F0067, task #146): yang boolean
//! output can carry a "planar" face whose loop vertices sit measurably off
//! the face plane while the AVERAGED Newell normal stays inside
//! `NORMAL_AGREEMENT_TOLERANCE` — production `validate_solid` (orientation
//! checks only; the coplanarity residual is a `debug_assertions` tripwire)
//! passed such solids, and the defect surfaced downstream as tessellation
//! self-intersections.
//!
//! The saddle fixture below reproduces the evasion exactly: alternating
//! ±d out-of-plane perturbation of a square face keeps the Newell normal
//! EXACTLY on +z by symmetry (in-plane cross-product components cancel),
//! so no orientation check can see it — only a vertex↔plane residual can.
//! The perturbation is along z, which is IN-plane for the four lateral
//! faces, so every other production invariant stays satisfied.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::error::KernelV2Error;
use kernel_v2::{
    boolean_op, extrude, validate_boolean_output_planarity, BrepArena, Profile, SolidId,
};

fn boxx(a: &mut BrepArena, x: (f64, f64), y: (f64, f64), z: (f64, f64)) -> SolidId {
    let p = Profile::new(
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
    .unwrap();
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), z.1 - z.0)
        .unwrap()
        .solid
}

/// Perturb the top face (+z) of a box: alternating `±d` along z around the
/// loop. Newell normal of the perturbed loop is exactly +z (symmetric
/// saddle); every loop vertex is |d| off the stored face plane.
fn saddle_top_face(arena: &mut BrepArena, solid: SolidId, d: f64) {
    let shells = arena.solid(solid).unwrap().shells.clone();
    let mut top_loop = None;
    for &sh in &shells {
        let faces = arena.shell(sh).unwrap().faces.clone();
        for &f in &faces {
            let face = arena.face(f).unwrap();
            if let Some(kernel_v2::arena::Surface::Plane(pl)) = &face.surface {
                if pl.normal.z > 0.5 {
                    top_loop = Some(face.outer_loop);
                }
            }
        }
    }
    let lid = top_loop.expect("box must have a +z top face");
    let hes = arena.loop_half_edges(lid).unwrap();
    assert_eq!(hes.len(), 4, "box top face is a quad");
    for (i, &h) in hes.iter().enumerate() {
        let v = arena.half_edge(h).unwrap().origin;
        let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
        let p = arena.vertex(v).unwrap().point;
        arena.vertex_mut(v).unwrap().point = Point3::new(p.x(), p.y(), p.z() + sign * d);
    }
}

/// RED-PHASE PIN: a real-scale (1e-4 m, 100× MIN_FEATURE_SIZE) saddle on a
/// unit box must be REJECTED by the production boolean-output gate with the
/// typed `NonPlanarFace` wall.
#[test]
fn saddle_face_is_rejected_by_the_production_gate() {
    let mut a = BrepArena::new();
    let s = boxx(&mut a, (0.0, 1.0), (0.0, 1.0), (0.0, 1.0));
    saddle_top_face(&mut a, s, 1e-4);
    let err = validate_boolean_output_planarity(&a, s)
        .expect_err("real-scale off-plane loop vertices must be rejected");
    assert!(
        matches!(err, KernelV2Error::NonPlanarFace { .. }),
        "expected NonPlanarFace, got {err:?}"
    );
}

/// The gate accepts exactly-constructed geometry.
#[test]
fn constructed_box_passes_the_gate() {
    let mut a = BrepArena::new();
    let s = boxx(&mut a, (0.0, 1.0), (0.0, 1.0), (0.0, 1.0));
    validate_boolean_output_planarity(&a, s).expect("exact box must pass");
}

/// Sub-band noise (1e-12 relative at unit scale = the healthy yang-output
/// tier after Stage-4 relocation + rational canonicalization) must PASS —
/// the gate is a defect wall, not a precision fetish (P9: reject threshold
/// sits ≥1000× above legitimate noise).
#[test]
fn sub_band_noise_passes_the_gate() {
    let mut a = BrepArena::new();
    let s = boxx(&mut a, (0.0, 1.0), (0.0, 1.0), (0.0, 1.0));
    saddle_top_face(&mut a, s, 1e-12);
    validate_boolean_output_planarity(&a, s).expect("sub-band noise must pass");
}

/// End-to-end: real boolean outputs pass through `boolean_op` with the gate
/// wired in — the band does not false-positive on healthy pipeline output.
/// (Union with a genuine intersection AND a subtract, both producing
/// re-assembled planar faces.)
#[test]
fn real_boolean_outputs_pass_the_wired_gate() {
    let mut a = BrepArena::new();
    let s1 = boxx(&mut a, (0.0, 2.0), (0.0, 2.0), (0.0, 2.0));
    let s2 = boxx(&mut a, (1.0, 3.0), (1.0, 3.0), (1.0, 3.0));
    let u = boolean_op(&mut a, s1, s2, BoolOp::Union).expect("overlapping union succeeds");
    validate_boolean_output_planarity(&a, u).expect("union output is planar within band");

    let mut b = BrepArena::new();
    let s3 = boxx(&mut b, (0.0, 4.0), (0.0, 4.0), (0.0, 4.0));
    let s4 = boxx(&mut b, (1.0, 3.0), (1.0, 3.0), (1.0, 5.0));
    let cut = boolean_op(&mut b, s3, s4, BoolOp::Subtract).expect("pocket subtract succeeds");
    validate_boolean_output_planarity(&b, cut).expect("subtract output is planar within band");
}
