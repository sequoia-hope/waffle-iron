//! Tests for the report module.

use test_harness::ModelBuilder;

#[test]
fn empty_model_report() {
    let mut m = ModelBuilder::mock();
    let report = m.report().unwrap();
    let text = report.to_text();
    assert!(text.contains("Feature Tree (0 features"));
    assert!(text.contains("Errors: none"));
}

#[test]
fn report_contains_feature_tree_section() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let report = m.report().unwrap();
    let text = report.to_text();
    assert!(
        text.contains("Feature Tree (2 features"),
        "Should list 2 features: {}",
        text
    );
    assert!(text.contains("Sketch"), "Should mention Sketch");
    assert!(text.contains("Extrude"), "Should mention Extrude");
}

#[test]
fn report_contains_mesh_summary() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let report = m.report().unwrap();
    let text = report.to_text();
    assert!(
        text.contains("Mesh Summary"),
        "Should have mesh summary section"
    );
    assert!(text.contains("triangles"), "Should mention triangle count");
}

#[test]
fn report_contains_topology_data() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let report = m.report().unwrap();
    let text = report.to_text();
    assert!(text.contains("V=8"), "Should show vertex count");
    assert!(text.contains("E=12"), "Should show edge count");
    assert!(text.contains("F=6"), "Should show face count");
}

#[test]
fn report_contains_bounding_box() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let report = m.report().unwrap();
    let text = report.to_text();
    assert!(text.contains("Bounding Box"), "Should have bounding box");
}

#[test]
fn report_contains_oracle_results() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let report = m.report().unwrap();
    let text = report.to_text();
    assert!(
        text.contains("Oracle Results"),
        "Should have oracle section"
    );
    assert!(text.contains("[PASS]"), "Should have passing oracles");
}

#[test]
fn report_oracle_results_vec_nonempty() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let report = m.report().unwrap();
    assert!(
        !report.oracle_results.is_empty(),
        "Should have oracle results"
    );
}

#[test]
fn report_shows_sketch_details() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();

    let report = m.report().unwrap();
    let text = report.to_text();
    assert!(text.contains("points"), "Should describe sketch entities");
    assert!(text.contains("lines"), "Should describe sketch entities");
}
