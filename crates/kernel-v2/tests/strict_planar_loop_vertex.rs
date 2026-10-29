//! O4 (remediation 2026-10-28): the strict validation tier checks every
//! loop vertex of a PLANAR face against its face plane — in release too.
//!
//! `validate_planar_face`'s loop-vertex residual used to sit behind
//! `cfg!(debug_assertions)`, so a `--release` corpus run with
//! `strict-validation` on (test-harness) never tested it. It now runs in
//! every build that compiles the strict tier (debug, or the
//! `strict-validation` feature) and stays compiled OUT of a release build
//! without the feature — the shipped WASM bundle.
//!
//! Fixture: the saddle of `f1_boolean_output_planarity.rs` — alternating
//! `±d` along z on a unit box's top face keeps the Newell normal exactly on
//! +z, and z is in-plane for the four lateral faces, so only a
//! vertex↔plane residual can see it. `d = 1e-6` is 5×10⁵ the strict band at
//! unit scale (`1e-12·(1+1)`); what these tests pin is which build runs
//! `validate_solid`'s own check, not the band.

use cad_primitives::{Point2, Point3, Vector3};
use kernel_v2::{extrude, validate_solid, BrepArena, Profile, SolidId};

fn unit_box(a: &mut BrepArena) -> SolidId {
    let p = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ],
        vec![],
    )
    .unwrap();
    extrude(a, &p, Vector3::new(0.0, 0.0, 1.0), 1.0)
        .unwrap()
        .solid
}

/// Alternating `±d` along z around the top (+z) face's loop.
fn saddle_top_face(arena: &mut BrepArena, solid: SolidId, d: f64) {
    let shells = arena.solid(solid).unwrap().shells.clone();
    let mut top_loop = None;
    for &sh in &shells {
        for &f in &arena.shell(sh).unwrap().faces.clone() {
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

/// The unperturbed box validates in every build (the fixture is sound).
#[test]
fn exact_box_validates() {
    let mut a = BrepArena::new();
    let s = unit_box(&mut a);
    validate_solid(&a, s).expect("exact box must validate");
}

/// Strict tier (debug, or `strict-validation` in release): a planar face
/// with loop vertices off its plane by more than the band is REJECTED by
/// `validate_solid` with the typed `NonPlanarFace` wall.
#[cfg(any(debug_assertions, feature = "strict-validation"))]
#[test]
fn off_plane_loop_vertex_fails_validate_solid_in_the_strict_tier() {
    let mut a = BrepArena::new();
    let s = unit_box(&mut a);
    saddle_top_face(&mut a, s, 1e-6);
    let err = validate_solid(&a, s).expect_err("off-plane loop vertices must fail the strict tier");
    assert!(
        matches!(err, kernel_v2::error::KernelV2Error::NonPlanarFace { .. }),
        "expected NonPlanarFace, got {err:?}"
    );
}

/// Production build (release, no `strict-validation` — the WASM bundle):
/// the check is compiled out, so the same saddle passes `validate_solid`.
/// Pins that production does not pay for, or change behaviour on, the
/// strict tier. (Production planarity walls are the boolean-output gate and
/// STEP ingestion, not `validate_solid`.)
#[cfg(not(any(debug_assertions, feature = "strict-validation")))]
#[test]
fn off_plane_loop_vertex_is_not_checked_in_a_production_build() {
    let mut a = BrepArena::new();
    let s = unit_box(&mut a);
    saddle_top_face(&mut a, s, 1e-6);
    validate_solid(&a, s).expect("production validate_solid has no planar loop-vertex tier");
}
