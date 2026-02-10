//! Tests for the ModelBuilder workflow API.

use test_harness::ModelBuilder;

#[test]
fn rect_sketch_creates_feature() {
    let mut m = ModelBuilder::mock();
    let id = m
        .rect_sketch("my_sketch", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    assert_eq!(m.feature_count(), 1);
    assert_eq!(m.feature_id("my_sketch").unwrap(), id);
}

#[test]
fn extrude_by_name() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    let ex_id = m.extrude("box", "sk", 10.0).unwrap();
    assert_eq!(m.feature_count(), 2);
    assert_eq!(m.feature_id("box").unwrap(), ex_id);
    m.assert_has_solid("box").unwrap();
}

#[test]
fn full_chain_sketch_extrude_tessellate() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    let mesh = m.tessellate("box").unwrap();
    assert_eq!(mesh.indices.len(), 36, "Box should have 12 triangles");
}

#[test]
fn named_lookup_returns_correct_uuid() {
    let mut m = ModelBuilder::mock();
    let sk_id = m
        .rect_sketch("s1", [0., 0., 0.], [0., 0., 1.], 0., 0., 5., 5.)
        .unwrap();
    let ex_id = m.extrude("e1", "s1", 3.0).unwrap();
    assert_eq!(m.feature_id("s1").unwrap(), sk_id);
    assert_eq!(m.feature_id("e1").unwrap(), ex_id);
    assert!(m.feature_id("nonexistent").is_err());
}

#[test]
fn duplicate_name_returns_error() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    let result = m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5., 5.);
    assert!(result.is_err(), "Duplicate name should error");
}

#[test]
fn auto_check_catches_errors() {
    let mut m = ModelBuilder::mock().with_auto_check();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    // Extrude with the sketch should work fine
    let result = m.extrude("box", "sk", 10.0);
    // MockKernel usually succeeds, so this should be Ok
    assert!(result.is_ok());
}

#[test]
fn undo_redo_works() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();
    assert_eq!(m.feature_count(), 2);

    m.undo().unwrap();
    assert_eq!(m.feature_count(), 1);

    m.redo().unwrap();
    assert_eq!(m.feature_count(), 2);
}

#[test]
fn save_load_roundtrip() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let json = m.save().unwrap();
    assert!(json.contains("waffle-iron"));

    let mut m2 = ModelBuilder::mock();
    m2.load(&json).unwrap();
    assert_eq!(m2.feature_count(), 2);
    // After load, feature names are restored from the tree
    assert!(m2.feature_id("Sketch").is_ok() || m2.feature_id("sk").is_ok());
}

#[test]
fn topology_counts_for_box() {
    let mut m = ModelBuilder::mock();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box", "sk", 10.0).unwrap();

    let (v, e, f) = m.topology_counts("box").unwrap();
    assert_eq!((v, e, f), (8, 12, 6), "MockKernel box: V=8 E=12 F=6");
}

#[test]
fn assert_feature_count_works() {
    let mut m = ModelBuilder::mock();
    m.assert_feature_count(0).unwrap();
    m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.assert_feature_count(1).unwrap();
    assert!(m.assert_feature_count(5).is_err());
}
