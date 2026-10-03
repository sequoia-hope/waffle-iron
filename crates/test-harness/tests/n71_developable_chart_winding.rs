//! N71 — a developable patch's material-CCW winding is a property of its
//! boundary CURVES, not of their chart chords.
//!
//! Deviation `docs/yang_deviations.md` N71; ledger
//! `docs/yang_tail_triage.md` 2026-10-03 (late). Both arms of kernel-v2's
//! bounded-patch postcondition (`validate_cylinder_patch` and
//! `validate_cone_patch`) measured the winding of the unrolled `(θ, h)` /
//! developed `(θ, τ)` chart polygon from ONE point per half-edge, i.e. with
//! every boundary edge replaced by its chart CHORD. Four of the six `Curve`
//! variants a boolean-output patch can carry have chart images that are not
//! straight, and the bulge is unbounded relative to the patch's own width —
//! so a sliver patch read the OPPOSITE sign and the kernel rejected its own
//! correct output face.
//!
//! Measured on P0018's `FaceId(27)`: an oblique plane∩cylinder `EllipseArc`
//! whose chart image is exactly `h(θ) = 349.0216 − 221.2497·cos(θ − 0.42957)`
//! dips to 127.77 at θ = 0.4296, twenty chart units below BOTH its endpoints
//! and sixteen below the lowest point of the seven-chord return polyline.
//! Chord shoelace −4.575297817481767 (a hole); canonical chart polygon
//! +18.30465761273355 (material).
//!
//! These two tests are the end-to-end pins for the two arms. Reverting the
//! cylinder arm makes `p0018_cylinder_patch_sliver_is_material` fail with the
//! verbatim `CurvedGeometryMismatch { … "bounded cylinder patch must have
//! exactly one material-CCW loop" }`; reverting the cone arm makes
//! `p0017_cone_patch_sliver_is_material` fail the same way with the cone
//! wording, because P0017 would stop at the postcondition instead of
//! rebuilding clean.
//!
//! P0017's SECOND wall (`ring rejected by CDT`) was its own finding and is
//! converted — deviation N76, `n76_output_curve_backtrack.rs`.
//!
//! Run: `cargo test -p test-harness --release --test n71_developable_chart_winding`

use std::fs;
use std::path::PathBuf;

use test_harness::ModelBuilder;

/// The postcondition both arms used to trip, verb-stripped to the shared tail.
const MATERIAL_CCW_WALL: &str = "must have exactly one material-CCW loop";

/// P0017's OWN wall, one stage later and a different family (N68, P0013):
/// the render CDT declining the same 7.175e-8-area chart sliver.
const CDT_REJECT_WALL: &str = "ring rejected by CDT";

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

fn load(case: &str) -> ModelBuilder {
    let waffle_json = fs::read_to_string(assay_dir().join(format!("{case}.waffle")))
        .unwrap_or_else(|e| panic!("{case}.waffle must be readable: {e}"));
    let mut builder = ModelBuilder::kernel_v2();
    builder
        .load(&waffle_json)
        .expect("LoadProject must succeed");
    builder
}

fn engine_errors(builder: &ModelBuilder) -> Vec<String> {
    builder
        .engine_errors()
        .iter()
        .map(|(id, msg)| format!("error {id}: {msg}"))
        .collect()
}

/// The CYLINDER arm: P0018 rebuilds clean, and its severed result is the one
/// body with TWO shells the exact-membership lattice adjudicated.
#[test]
fn p0018_cylinder_patch_sliver_is_material() {
    let mut builder = load("P0018");
    let failures = engine_errors(&builder);
    assert!(
        !failures.iter().any(|f| f.contains(MATERIAL_CCW_WALL)),
        "the sliver between the oblique-section ellipse arc and its chord \
         polyline is MATERIAL (chart area +18.30465761273355), not a hole \
         (chord area −4.575297817481767); got:\n  {}",
        failures.join("\n  ")
    );
    assert!(
        failures.is_empty(),
        "P0018 must rebuild clean; got:\n  {}",
        failures.join("\n  ")
    );

    // The 7-vertex non-convex cut SEVERS the body: the exact-membership
    // lattice reads two components at every rung (1024-cell total 6.738844e6,
    // the small piece ≈ 4.4e3) and the kernel keeps BOTH — its render mesh
    // welds to components of 6.730121e6 and 4.297951e3. The tessellated total
    // therefore sits one chord deficit below the exact volume.
    let mesh = builder
        .tessellate_last_with_tol(1e-3)
        .expect("the converted body must tessellate");
    let vol = signed_volume(&mesh.vertices, &mesh.indices);
    let expected = 6.734419e6;
    assert!(
        (vol - expected).abs() <= 1e-4 * expected,
        "tessellated volume {vol:.7e} must match the adjudicated \
         6.734419240e6 (big component 6.730121e6 + severed speck 4.297951e3) \
         within 1e-4 relative"
    );
}

/// The CONE arm: P0017's postcondition now PASSES. This is the mutation
/// detector for the cone half — with the chord measure back, the error text
/// reverts to the postcondition.
///
/// P0017 was HALF converted when this file was written: the same sliver
/// reached the render tessellator, which declined it
/// (`ring rejected by CDT (degenerate/self-intersecting)`). That wall was its
/// own finding — deviation **N76**, the output loop double-covering one
/// `SurfacePair` curve — and it landed 2026-10-03 (night), so the
/// `CDT_REJECT_WALL` assertion this test used to carry is UN-QUARANTINED here
/// in the same spirit: the case must now emit NO engine error at all.
/// `n76_output_curve_backtrack.rs` is that fix's own pin.
#[test]
fn p0017_cone_patch_sliver_is_material() {
    let builder = load("P0017");
    let failures = engine_errors(&builder);
    assert!(
        !failures.iter().any(|f| f.contains(MATERIAL_CCW_WALL)),
        "the cut's sliver remnant of cone band FaceId(16) is MATERIAL (chart \
         area +7.175427296555491e-8), not a hole (chord area \
         −6.722980363191853e-7); got:\n  {}",
        failures.join("\n  ")
    );
    assert!(
        !failures.iter().any(|f| f.contains(CDT_REJECT_WALL)),
        "P0017's render-CDT sliver wall ({CDT_REJECT_WALL}) was converted by \
         N76 and must not return; got:\n  {}",
        failures.join("\n  ")
    );
    assert!(
        failures.is_empty(),
        "P0017 rebuilds clean since N76; got:\n  {}",
        failures.join("\n  ")
    );
}

/// Divergence-theorem volume of a closed triangle soup (flat positions,
/// triangle indices).
fn signed_volume(positions: &[f32], indices: &[u32]) -> f64 {
    let p = |i: u32| -> [f64; 3] {
        let k = i as usize * 3;
        [
            f64::from(positions[k]),
            f64::from(positions[k + 1]),
            f64::from(positions[k + 2]),
        ]
    };
    let mut v = 0.0f64;
    for t in indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        v += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
    }
    v
}
