//! C0079-F1 regression: multi-target `Add` with DISJOINT explicit targets.
//!
//! An extrude with `combine = Add` and explicit targets `[A, B]` where A and
//! B are disjoint bodies bridged by the tool must union ALL of A, B, and the
//! tool into one dumbbell body (spec `optional_booleans_multibody_extrude.md`
//! §4.2: the result is the set union `(b0 ∪ b1 ∪ …) ∪ tool`, order-
//! independent, every resulting body emitted).
//!
//! The defect (found by the C-series baseline, 2026-07-05): the Add fold in
//! `dispatch_combine` unioned the targets together FIRST — a disjoint union,
//! which kernel-v2 legitimately returns as TWO lumps through
//! `boolean_union_multi` — and then took `.outputs.first()`, silently
//! discarding body B's material (volume 1.625 = A∪tool instead of 2.5).
//!
//! Uses the real kernel (MockKernel's `union_multi` can never return more
//! than one lump, so the defect is invisible to it). Geometry mirrors assay
//! case C0079:
//!   A: box x∈[−2,−1], y∈[−0.5,0.5], z∈[0,1]           (volume 1)
//!   B: box x∈[1,2],   y∈[−0.5,0.5], z∈[0,1] (NewBody)  (volume 1)
//!   C: bridge x∈[−1.5,1.5], y∈[−0.25,0.25], z∈[0.25,0.75], Add → [A, B]
//!      (volume 0.75, overlapping each of A and B by 0.125)
//!   Expected: ONE body, volume 2.5, χ = 2.

use test_harness::helpers::mesh_signed_volume;
use test_harness::oracle;
use test_harness::ModelBuilder;

const CASE_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../app/tests/cases/assay/C0079.waffle"
);

/// Load C0079, optionally transforming the raw JSON first.
fn build(transform: impl FnOnce(&mut serde_json::Value)) -> ModelBuilder {
    let raw = std::fs::read_to_string(CASE_PATH).expect("read C0079.waffle");
    let mut json: serde_json::Value = serde_json::from_str(&raw).expect("parse C0079.waffle");
    transform(&mut json);
    let mut b = ModelBuilder::kernel_v2();
    b.load(&serde_json::to_string(&json).expect("serialize"))
        .expect("LoadProject");
    b
}

/// The features array of the single Part tab.
fn features(json: &mut serde_json::Value) -> &mut Vec<serde_json::Value> {
    json["tabs"][0]["kind"]["features"]["features"]
        .as_array_mut()
        .expect("features array")
}

fn assert_dumbbell(b: &mut ModelBuilder, label: &str) {
    let errors = b.engine_errors().to_vec();
    assert!(errors.is_empty(), "{label}: engine errors: {errors:?}");
    let warnings = b.engine_warnings().to_vec();
    assert!(
        warnings.is_empty(),
        "{label}: unexpected warnings: {warnings:?}"
    );
    assert_eq!(
        b.distinct_solid_count(),
        1,
        "{label}: bridged Add must merge A, B, and the tool into ONE body"
    );
    let outputs = b
        .op_result("Extrude 3")
        .expect("Add feature result")
        .outputs
        .len();
    assert_eq!(
        outputs, 1,
        "{label}: bridged Add must emit exactly one output body"
    );
    let meshes = b.tessellate_live_with_tol(0.01).expect("tessellate");
    let vol: f64 = meshes.iter().map(mesh_signed_volume).sum();
    assert!(
        (vol - 2.5).abs() < 2.5e-3,
        "{label}: dumbbell volume must be 2.5 (1 + 1 + 0.75 − 2·0.125), got {vol}"
    );
    let last = b.tessellate_last_with_tol(0.01).expect("tessellate last");
    let chi = oracle::check_mesh_euler_characteristic(&last, 2);
    assert!(
        chi.passed,
        "{label}: dumbbell must have χ = 2: {}",
        chi.detail
    );
}

/// Canonical case: Add with explicit disjoint targets [A, B], tool bridging
/// both. This is assay case C0079 byte-for-byte.
#[test]
fn add_bridging_two_disjoint_targets_unions_all_material() {
    let mut b = build(|_| {});
    assert_dumbbell(&mut b, "targets [A,B]");
}

/// Adversarial (spec §4.2 "order-independent"): the same case with the
/// explicit target list REVERSED must produce the identical dumbbell.
#[test]
fn add_bridging_reversed_target_order_is_equivalent() {
    let mut b = build(|json| {
        let mut reversed = false;
        for f in features(json) {
            let op = &mut f["operation"];
            if op["type"] == "Extrude" && op["params"]["targets"].is_array() {
                let t = op["params"]["targets"].as_array_mut().unwrap();
                t.reverse();
                reversed = true;
            }
        }
        assert!(reversed, "fixture drift: no extrude with explicit targets");
    });
    assert_dumbbell(&mut b, "targets [B,A]");
}

/// Adversarial (no silent loss, spec §4.2 emits every resulting body): move
/// body B out of the bridge's reach. Add targets [A, B] where the tool only
/// touches A must yield TWO bodies — the A∪tool component and B untouched —
/// with a warning naming the disjoint remainder, and no material lost.
#[test]
fn add_with_unreachable_target_keeps_it_as_separate_body_with_warning() {
    let mut b = build(|json| {
        // B's sketch is the one anchored at x = 1.5 (the second boss). Move
        // it to x = 8 so the bridge (x ∈ [−1.5, 1.5]) cannot reach it.
        let mut moved = false;
        for f in features(json) {
            let op = &mut f["operation"];
            if op["type"] == "Sketch" && op["sketch"]["plane_origin"][0] == 1.5 {
                op["sketch"]["plane_origin"][0] = serde_json::json!(8.0);
                moved = true;
            }
        }
        assert!(moved, "fixture drift: no sketch anchored at x=1.5");
    });
    let errors = b.engine_errors().to_vec();
    assert!(
        errors.is_empty(),
        "unreachable target: engine errors: {errors:?}"
    );
    // The Add feature itself must emit TWO bodies: the A∪tool component as
    // Main and the untouched B as Body{1}. (`distinct_solid_count` counts
    // solid-bearing FEATURES, so it cannot see feature-internal extra bodies —
    // assert on the feature's own output list.)
    let outputs = b
        .op_result("Extrude 3")
        .expect("Add feature result")
        .outputs
        .len();
    assert_eq!(
        outputs, 2,
        "tool-disjoint target must survive as the Add feature's second output body"
    );
    let meshes = b.tessellate_live_with_tol(0.01).expect("tessellate");
    let vol: f64 = meshes.iter().map(mesh_signed_volume).sum();
    // A∪tool = 1 + 0.75 − 0.125 = 1.625; B = 1.0.
    assert!(
        (vol - 2.625).abs() < 2.7e-3,
        "total volume must be 2.625 (A∪tool 1.625 + B 1.0), got {vol}"
    );
    let warnings = b.engine_warnings().to_vec();
    assert!(
        warnings.iter().any(|w| w.contains("disjoint")),
        "a target left disjoint from the tool must be warned about, got {warnings:?}"
    );
}
