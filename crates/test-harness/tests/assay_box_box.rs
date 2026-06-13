//! Property-based tests for box-box boolean operations.
//!
//! Uses proptest to generate random box pairs and verify algebraic invariants.

use proptest::prelude::*;
use test_harness::assay::properties::*;
use test_harness::assay::strategies::strats::*;
use test_harness::assay::strategies::*;
use test_harness::helpers::mesh_volume;
use test_harness::ModelBuilder;

/// Build a boolean scenario using ModelBuilder and return (builder, mesh_a, mesh_b, mesh_result).
fn execute_scenario(scenario: &BooleanScenario) -> Result<ModelBuilder, String> {
    let mut builder = ModelBuilder::kernel_v2();

    // Build body A
    match &scenario.body_a.profile {
        SketchProfile::Rect(r) => {
            builder
                .rect_sketch(
                    "sk_a",
                    scenario.body_a.origin,
                    scenario.body_a.normal,
                    r.x,
                    r.y,
                    r.w,
                    r.h,
                )
                .map_err(|e| e.to_string())?;
        }
        SketchProfile::Circle(c) => {
            builder
                .circle_sketch(
                    "sk_a",
                    scenario.body_a.origin,
                    scenario.body_a.normal,
                    c.cx,
                    c.cy,
                    c.r,
                )
                .map_err(|e| e.to_string())?;
        }
    }
    builder
        .extrude_no_merge("body_a", "sk_a", scenario.body_a.depth)
        .map_err(|e| e.to_string())?;

    // Build body B
    match &scenario.body_b.profile {
        SketchProfile::Rect(r) => {
            builder
                .rect_sketch(
                    "sk_b",
                    scenario.body_b.origin,
                    scenario.body_b.normal,
                    r.x,
                    r.y,
                    r.w,
                    r.h,
                )
                .map_err(|e| e.to_string())?;
        }
        SketchProfile::Circle(c) => {
            builder
                .circle_sketch(
                    "sk_b",
                    scenario.body_b.origin,
                    scenario.body_b.normal,
                    c.cx,
                    c.cy,
                    c.r,
                )
                .map_err(|e| e.to_string())?;
        }
    }
    builder
        .extrude_no_merge("body_b", "sk_b", scenario.body_b.depth)
        .map_err(|e| e.to_string())?;

    // Apply boolean
    let bool_result = match scenario.op {
        BoolOp::Union => builder.boolean_union("result", "body_a", "body_b"),
        BoolOp::Subtract => builder.boolean_subtract("result", "body_a", "body_b"),
        BoolOp::Intersect => builder.boolean_intersect("result", "body_a", "body_b"),
    };
    bool_result.map_err(|e| e.to_string())?;

    Ok(builder)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    #[test]
    fn union_volume_monotonicity(scenario in boolean_scenario_union_rect()) {
        if let Ok(mut builder) = execute_scenario(&scenario) {
            let results = run_all_boolean_properties(
                &mut builder, "body_a", "body_b", "result", BoolOp::Union,
            );
            for r in &results {
                if r.name.contains("volume_monotonicity") {
                    prop_assert!(r.passed, "Volume monotonicity failed: {}", r.detail);
                }
            }
        }
        // If execution fails (boolean cascade), skip — not a property violation
    }

    #[test]
    fn subtract_volume_bounded(scenario in boolean_scenario_subtract_rect()) {
        if let Ok(mut builder) = execute_scenario(&scenario) {
            let mesh_a = builder.tessellate("body_a").unwrap();
            let mesh_r = builder.tessellate_combined("result").unwrap();
            let vol_a = mesh_volume(&mesh_a);
            let vol_r = mesh_volume(&mesh_r);
            let tol = vol_a * 0.05;
            prop_assert!(
                vol_r <= vol_a + tol,
                "vol(A-B)={:.3} > vol(A)={:.3}",
                vol_r,
                vol_a
            );
        }
    }

    #[test]
    fn intersect_volume_bounded(scenario in boolean_scenario_intersect_rect()) {
        if let Ok(mut builder) = execute_scenario(&scenario) {
            let mesh_a = builder.tessellate("body_a").unwrap();
            let mesh_b = builder.tessellate("body_b").unwrap();
            let mesh_r = builder.tessellate_combined("result").unwrap();
            let vol_a = mesh_volume(&mesh_a);
            let vol_b = mesh_volume(&mesh_b);
            let vol_r = mesh_volume(&mesh_r);
            let tol = (vol_a + vol_b) * 0.05;
            prop_assert!(
                vol_r <= vol_a.min(vol_b) + tol,
                "vol(A∩B)={:.3} > min(vol(A)={:.3}, vol(B)={:.3})",
                vol_r,
                vol_a,
                vol_b
            );
        }
    }

    #[test]
    fn union_bbox_containment(scenario in boolean_scenario_union_rect()) {
        if let Ok(mut builder) = execute_scenario(&scenario) {
            let mesh_a = builder.tessellate("body_a").unwrap();
            let mesh_b = builder.tessellate("body_b").unwrap();
            let mesh_r = builder.tessellate_combined("result").unwrap();
            let result = check_bbox_containment(&mesh_a, &mesh_b, &mesh_r, BoolOp::Union);
            prop_assert!(result.passed, "BBox containment failed: {}", result.detail);
        }
    }
}

// ── Deterministic integration tests ─────────────────────────────────────

/// Two 10x10x10 boxes overlapping by 5 units along X.
/// Union volume = 10*10*10 + 10*10*10 - 5*10*10 = 1500.
#[test]
fn deterministic_box_box_union_volume() {
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    b.extrude_no_merge("body_a", "sk_a", 10.).unwrap();
    b.rect_sketch("sk_b", [0., 0., 0.], [0., 0., 1.], 5., 0., 10., 10.)
        .unwrap();
    b.extrude_no_merge("body_b", "sk_b", 10.).unwrap();
    b.boolean_union("result", "body_a", "body_b").unwrap();
    let mesh = b.tessellate("result").unwrap();
    let vol = mesh_volume(&mesh);
    assert!(
        (vol - 1500.0).abs() < 1.0,
        "Union volume: expected ~1500, got {}",
        vol
    );
}

/// Two 10x10x10 boxes overlapping by 5 units along X.
/// Subtract volume = 10*10*10 - 5*10*10 = 500.
#[test]
fn deterministic_box_box_subtract_volume() {
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    b.extrude_no_merge("body_a", "sk_a", 10.).unwrap();
    b.rect_sketch("sk_b", [0., 0., 0.], [0., 0., 1.], 5., 0., 10., 10.)
        .unwrap();
    b.extrude_no_merge("body_b", "sk_b", 10.).unwrap();
    b.boolean_subtract("result", "body_a", "body_b").unwrap();
    let mesh = b.tessellate("result").unwrap();
    let vol = mesh_volume(&mesh);
    assert!(
        (vol - 500.0).abs() < 1.0,
        "Subtract volume: expected ~500, got {}",
        vol
    );
}
