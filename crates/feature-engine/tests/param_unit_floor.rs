//! Why `DesignParameter.unit` moved the format floor to v8.
//!
//! `docs/FILE_FORMAT.md` §13.3 bumps `MIN_READER_VERSION` for "a new field
//! that a reader must not silently ignore", and v5 (`GeomRef.scope`) and v6
//! (`Sketch.plane_x_axis`) both moved it for a purely additive optional
//! field, because an old reader would DROP it and build something
//! different. P1's `unit` was left at 7 with the argument that dropping it
//! can only turn a refusal back into the pre-P1 coercion, so a document
//! that builds cleanly builds identically.
//!
//! This test measures the other half of that argument: a document that does
//! NOT build cleanly. `unit` is the author's written statement that a
//! parameter is an angle; a reader that drops the statement hands the
//! number to a length field as millimetres and produces a solid the
//! declaring build refuses to produce. Two different geometries from one
//! file is exactly the v5/v6 harm, so the floor moves.

use feature_engine::params::apply_parameters;
use feature_engine::types::FeatureTree;
use serde_json::Value;

/// A tree with an `Angle` parameter and an extrude depth that reads it —
/// the shape of a document mid-edit, before the author notices the slip.
fn tree_json() -> Value {
    serde_json::json!({
        "features": [{
            "id": "11111111-1111-4111-8111-111111111111",
            "name": "Extrude",
            "operation": {
                "type": "Extrude",
                "params": {
                    "sketch_id": "22222222-2222-4222-8222-222222222222",
                    "profile_index": 0,
                    "depth": 0.004,
                    "depth_expr": "turn",
                    "symmetric": false,
                    "cut": false,
                    "merge": true
                }
            },
            "suppressed": false,
            "references": []
        }],
        "parameters": [{
            "id": "33333333-3333-4333-8333-333333333333",
            "name": "turn",
            "expression": "90",
            "value": 90.0,
            "unit": "Angle"
        }]
    })
}

fn depth_of(tree: &FeatureTree) -> f64 {
    match &tree.features[0].operation {
        feature_engine::types::Operation::Extrude { params } => params.depth,
        other => panic!("expected an extrude, got {other:?}"),
    }
}

#[test]
fn a_reader_that_drops_unit_builds_a_different_solid() {
    // A reader that KNOWS `unit` refuses the depth and leaves it alone.
    let json = tree_json();
    let mut declared: FeatureTree = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(
        declared.parameters[0].unit,
        Some(feature_engine::expr::Dimension::Angle)
    );
    let outcome = apply_parameters(&mut declared);
    assert_eq!(
        depth_of(&declared),
        0.004,
        "the declaring reader must not move the depth"
    );
    assert_eq!(outcome.errors.len(), 1, "{:?}", outcome.errors);
    assert!(
        outcome.errors[0]
            .1
            .contains("expected a length, got an angle"),
        "{}",
        outcome.errors[0].1
    );

    // A reader that does not know the field drops it — serde ignores
    // unknown keys, and `DesignParameter` keeps none — which is the same
    // document with `unit` removed.
    let mut stripped_json = json;
    stripped_json["parameters"][0]
        .as_object_mut()
        .unwrap()
        .remove("unit");
    let mut dropped: FeatureTree = serde_json::from_value(stripped_json).unwrap();
    assert_eq!(dropped.parameters[0].unit, None);
    let outcome = apply_parameters(&mut dropped);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);

    // THE MEASUREMENT: 90 mm of extrude where the declaring build refused
    // to extrude at all. One file, two solids.
    assert_eq!(depth_of(&dropped), 0.090);
    assert_ne!(depth_of(&declared), depth_of(&dropped));
}

#[test]
fn a_document_that_builds_cleanly_is_unaffected_either_way() {
    // The half of the argument that IS right, pinned so the bump is not
    // mistaken for `unit` changing a magnitude. It never does.
    let mut json = tree_json();
    json["parameters"][0]["unit"] = serde_json::json!("Length");
    json["parameters"][0]["expression"] = serde_json::json!("25");

    let mut declared: FeatureTree = serde_json::from_value(json.clone()).unwrap();
    assert!(apply_parameters(&mut declared).errors.is_empty());

    json["parameters"][0]
        .as_object_mut()
        .unwrap()
        .remove("unit");
    let mut dropped: FeatureTree = serde_json::from_value(json).unwrap();
    assert!(apply_parameters(&mut dropped).errors.is_empty());

    assert_eq!(depth_of(&declared), 0.025);
    assert_eq!(
        depth_of(&declared).to_bits(),
        depth_of(&dropped).to_bits(),
        "a clean document must build bit-identically without the field"
    );
}
