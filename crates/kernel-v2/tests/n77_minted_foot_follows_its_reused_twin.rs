//! N77 amendment (2026-10-05) — a MINTED seam foot takes the exact azimuth of
//! the REUSED foot it shares a band with, in the component pass too.
//!
//! Deviation `docs/yang_deviations.md` N77, "Amendment 2026-10-05". The F11
//! rule (commit 0c1cb420, 2026-09-15; PASS 2 of `recover.rs`): the two feet of
//! one canonical seam are the ends of ONE ruling, so when one rim reuses a
//! retained vertex and the other mints its foot, the minted foot must sit at
//! the reused foot's azimuth EXACTLY — "within `band` of the component
//! direction" (≈1e-9 relative) is not a ruling to the validator's 1e-12.
//! PASS 1C (component-wise anchoring, always-on since 2026-10-03) re-stated
//! the per-rim rule "existing vertex within `band`, else mint at `dir`" and so
//! dropped F11: a bore band whose component direction came from the coaxial
//! outer wall reused one rim's vertex 2.6e-10 rad off `dir` and minted the
//! other AT `dir`, PASS 1C's own ruling check refused the band
//! (`off=3.66e-12 > 1e-12`), the face stayed ANNULAR, and the assembler
//! refused its output with `cylinder face with inner loops is outside the
//! KV5a vocabulary`.
//!
//! ## The fixtures
//!
//! * **F11's tube, verbatim, through the adapter.** An annulus sketch region
//!   (OD r = 0.0159, bore r = 0.014) computed by `compute_regions` and staged
//!   through `make_face_from_region` — the region path's arc subdivision is
//!   what puts the Stage-1 lattice vertices where they are, and the defect
//!   lives in WHICH retained vertices each yang rim keeps, so a hand-built
//!   quarter-arc profile does not reach it (measured: it seams either way).
//!   Then the box Cut that misses the tube. A subtract has no AABB passthrough
//!   (task #134 is union-only), so yang re-emits the tube with its walls as
//!   two closed 4-arc rims each, seams gone — the form recover canonicalizes.
//!   WHICH bore radius trips the rule depends on which lattice vertices the
//!   yang rims retain, and that differs between this adapter-level entry and
//!   the engine's (`test-harness/tests/f11_disjoint_cut_thin_tube.rs`, where
//!   r = 0.014 is the red one at `off = 3.66e-12`). Measured here with
//!   `KV2_RECOVER_PROBE=1` on the pre-amendment code: r = 0.015 →
//!   `pass1c component 3 face 3: off=1.26699317704037630e-11 -> REFUSED`
//!   (the KV5a wall), r = 0.014 → `off = 0`, seamed. With the amendment both
//!   read `off = 0`. So r = 0.015 is this file's RED→GREEN pin and r = 0.014
//!   its always-green twin of the same shape.
//! * **A coaxial cylinder subtract** (disc minus coaxial disc over the same
//!   height, coplanar caps). It does NOT reach the amended rule (nothing is
//!   minted) and pins the coaxial subtract's canonical form next to the F11
//!   fixture so a change to either half of the machinery shows.
//!
//! Run: `cargo test -p kernel-v2 --release --test n77_minted_foot_follows_its_reused_twin`

use std::collections::HashMap;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::construct::extrude;
use kernel_v2::{boolean_op, tessellate, validate_solid, BrepArena, KernelV2Adapter, Profile};
use waffle_types::kernel::{Kernel, KernelSolidHandle};
use waffle_types::{compute_regions, regions, CircleProfile, ClosedProfile, SketchEntity};

const R_OUTER: f64 = 0.0159;
const HEIGHT: f64 = 0.3;

/// Enclosed volume of a flat `(positions, indices)` triangle mesh.
fn enclosed_volume(pos: &[f64], indices: &[u32]) -> f64 {
    let p = |i: u32| {
        let k = (i as usize) * 3;
        [pos[k], pos[k + 1], pos[k + 2]]
    };
    let mut s = 0.0;
    for t in indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        s += a[0] * (b[1] * c[2] - b[2] * c[1])
            + a[1] * (b[2] * c[0] - b[0] * c[2])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
    }
    (s / 6.0).abs()
}

/// F11's tube: concentric circles r = 0.0159 / `r_bore` about a point,
/// staged as the annulus REGION the sketch solver computes (arc edges on both
/// loops), extruded 0.3 along +z. Then F11's box Cut `x ∈ [0.10, 0.30]`,
/// `y ∈ [−0.05, 0.05]`, `z ∈ [0.10, 0.20]`, disjoint from the tube. Returns
/// the output's render volume.
fn f11_tube_minus_disjoint_box(r_bore: f64) -> Result<f64, String> {
    let mut k = KernelV2Adapter::new();

    // The tube.
    let point = SketchEntity::Point {
        id: 0,
        x: 0.0,
        y: 0.0,
        construction: false,
    };
    let circle = |id, radius| SketchEntity::Circle {
        id,
        center_id: 0,
        radius,
        construction: false,
    };
    let entities = vec![point, circle(1, R_OUTER), circle(2, r_bore)];
    let positions = HashMap::from([(0u32, (0.0, 0.0))]);
    let region = compute_regions(&entities, &positions, regions::DEFAULT_CHORD_TOLERANCE)
        .into_iter()
        .find(|r| !r.holes.is_empty())
        .expect("concentric circles yield an annulus region");
    let annulus = k
        .make_face_from_region(&region, [0.0; 3], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0])
        .map_err(|e| format!("annulus region: {e:?}"))?;
    let tube: KernelSolidHandle = k
        .extrude_face(annulus, [0.0, 0.0, 1.0], HEIGHT)
        .map_err(|e| format!("tube: {e:?}"))?;

    // The tool, at z = 0.10 in a sketch whose plane is z = 0.10.
    let (x0, y0, w, h) = (0.10, -0.05, 0.20, 0.10);
    let rect_positions = HashMap::from([
        (1u32, (x0, y0)),
        (2, (x0 + w, y0)),
        (3, (x0 + w, y0 + h)),
        (4, (x0, y0 + h)),
    ]);
    let rect = ClosedProfile {
        entity_ids: vec![1, 2, 3, 4],
        is_outer: true,
        vertex_ids: vec![1, 2, 3, 4],
        circle: None::<CircleProfile>,
        spline_segments: vec![],
        arc_segments: vec![],
    };
    let faces = k
        .make_faces_from_profiles(
            &[rect],
            [0.0, 0.0, 0.10],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &rect_positions,
        )
        .map_err(|e| format!("tool profile: {e:?}"))?;
    let tool = k
        .extrude_face(faces[0], [0.0, 0.0, 1.0], 0.10)
        .map_err(|e| format!("tool: {e:?}"))?;

    let out = k
        .boolean_subtract(&tube, &tool)
        .map_err(|e| format!("subtract: {e:?}"))?;
    let mesh = k
        .tessellate(&out, 0.001)
        .map_err(|e| format!("tessellate: {e:?}"))?;
    let pos: Vec<f64> = mesh.vertices.iter().map(|&v| v as f64).collect();
    Ok(enclosed_volume(&pos, &mesh.indices))
}

/// The render mesh inscribes both walls, so its volume sits BELOW the exact
/// tube by the chord sagitta: measured rel 1.305e-3 at both radii on the
/// kernel-v2 render tessellation. The band is a 2× margin over that; a
/// dropped wall or a swallowed bore would be a 100 % miss, and a mis-seamed
/// wall never validates at all (the subtract itself errors).
fn check_volume(r_bore: f64, volume: f64) {
    let exact = std::f64::consts::PI * (R_OUTER * R_OUTER - r_bore * r_bore) * HEIGHT;
    let rel = (volume - exact).abs() / exact;
    assert!(
        rel < 3e-3,
        "r_bore={r_bore}: render volume {volume:.9e} vs exact {exact:.9e} (rel {rel:.3e})"
    );
}

/// RED before the amendment: `subtract: BooleanFailed { reason: "kernel-v2
/// boolean_subtract failed: CurvedGeometryMismatch { face: FaceId(19), reason:
/// \"cylinder face with inner loops is outside the KV5a vocabulary\" }" }`
/// from the bore wall, refused by PASS 1C at `off = 1.267e-11`.
#[test]
fn f11_bore_r15_one_rim_reused_one_minted_is_seamed_on_one_ruling() {
    let volume = f11_tube_minus_disjoint_box(0.015)
        .expect("a Cut that misses the tube must still assemble the tube");
    check_volume(0.015, volume);
}

/// Always green here (`off = 0` before and after); pins the same shape so a
/// change to either half shows. (The engine-path twin of THIS radius is the
/// red one over there.)
#[test]
fn f11_bore_r14_stays_seamed() {
    let volume = f11_tube_minus_disjoint_box(0.014)
        .expect("a Cut that misses the tube must still assemble the tube");
    check_volume(0.014, volume);
}

/// The same tube as a boolean of two canonical cylinders: the r = 0.0159 disc
/// extrude minus the coaxial r = `r_bore` disc extrude over the SAME height
/// (coplanar caps, Stage-0 overlay). Returns (faces, rings, render volume).
fn cylinder_minus_coaxial_cylinder(r_bore: f64) -> (usize, usize, f64) {
    let mut arena = BrepArena::new();
    let disc = |r: f64| {
        Profile::circle(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Point2::new(0.0, 0.0),
            r,
        )
        .expect("disc profile")
    };
    let outer = extrude(
        &mut arena,
        &disc(R_OUTER),
        Vector3::new(0.0, 0.0, 1.0),
        HEIGHT,
    )
    .expect("outer cylinder")
    .solid;
    let bore = extrude(
        &mut arena,
        &disc(r_bore),
        Vector3::new(0.0, 0.0, 1.0),
        HEIGHT,
    )
    .expect("bore cylinder")
    .solid;
    let out = boolean_op(&mut arena, outer, bore, BoolOp::Subtract)
        .expect("the coaxial subtract assembles a tube");
    let report = validate_solid(&arena, out).expect("the tube validates");
    let mesh = tessellate(&arena, out).expect("the tube tessellates");
    (
        report.faces,
        report.rings,
        enclosed_volume(&mesh.positions, &mesh.indices),
    )
}

fn check_coaxial(r_bore: f64) {
    let (faces, rings, volume) = cylinder_minus_coaxial_cylinder(r_bore);
    // Two caps + two CANONICAL seamed laterals: a wall left in the annular
    // form would never have validated. Each annular cap carries exactly one
    // inner rim.
    assert_eq!(
        (faces, rings),
        (4, 2),
        "r_bore={r_bore}: expected two caps and two seamed walls (4 faces, 2 rings)"
    );
    check_volume(r_bore, volume);
}

#[test]
fn a_coaxial_cylinder_subtract_seams_the_bore_r14() {
    check_coaxial(0.014);
}

#[test]
fn a_coaxial_cylinder_subtract_seams_the_bore_r15() {
    check_coaxial(0.015);
}
