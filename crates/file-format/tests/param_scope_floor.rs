//! Why P2 moved the format floor to v12.
//!
//! `docs/FILE_FORMAT.md` §13.3 bumps `MIN_READER_VERSION` for "a defaulted
//! field whose ABSENCE changes what gets built". `DocumentMetadata.parameters`
//! and `Instance.parameter_overrides` are both additive, both defaulted, and
//! neither makes an old reader FAIL — serde drops an unknown key and an
//! absent `Option`/`Vec` takes its default. The harm is the same one v8
//! (`DesignParameter.unit`) and v9 (`Sketch.plane_face`) moved the floor for:
//! one file, two solids.
//!
//! `parameter_overrides` is the sharper of the two, and it is the reason the
//! bump is not arguable. The field has been WRITTEN since Phase 3 ("reserved;
//! not applied") and P2 makes it load-bearing, so the very same bytes now
//! mean "this instance is 25 mm tall" to a v12 reader and nothing at all to
//! a v11 one — which builds the part's own 10 mm instead. A reader cannot be
//! allowed to silently disagree about an instance's size.
//!
//! Both halves are measured below against the kernel, not against the field
//! the apply pass wrote.

use std::collections::BTreeMap;

use feature_engine::types::{DesignParameter, FeatureTree, Operation};
use feature_engine::Engine;
use serde_json::{json, Value};
use uuid::Uuid;
use waffle_types::kernel::{Kernel, MockKernel};

// ── the part: a 20 × 10 mm rectangle extruded by `height` ──────────────────

fn tree_json(parameter: Value, depth_expr: &str) -> Value {
    let sketch_fid = "11111111-1111-4111-8111-111111111111";
    json!({
        "features": [
            {
                "id": sketch_fid,
                "name": "Sketch1",
                "operation": { "type": "Sketch", "sketch": {
                    "id": "22222222-2222-4222-8222-222222222222",
                    "plane": {
                        "kind": { "type": "Face" },
                        "anchor": { "type": "Datum",
                                    "datum_id": "33333333-3333-4333-8333-333333333333" },
                        "selector": { "type": "Position", "x": 0.0, "y": 0.0, "z": 0.0 },
                        "policy": { "type": "BestEffort" }
                    },
                    "plane_origin": [0.0, 0.0, 0.0],
                    "plane_normal": [0.0, 0.0, 1.0],
                    "entities": [
                        { "type": "Point", "id": 1, "x": 0.0,  "y": 0.0 },
                        { "type": "Point", "id": 2, "x": 0.02, "y": 0.0 },
                        { "type": "Point", "id": 3, "x": 0.02, "y": 0.01 },
                        { "type": "Point", "id": 4, "x": 0.0,  "y": 0.01 },
                        { "type": "Line", "id": 5, "start_id": 1, "end_id": 2 },
                        { "type": "Line", "id": 6, "start_id": 2, "end_id": 3 },
                        { "type": "Line", "id": 7, "start_id": 3, "end_id": 4 },
                        { "type": "Line", "id": 8, "start_id": 4, "end_id": 1 }
                    ],
                    "constraints": [],
                    "solve_status": { "type": "FullyConstrained" }
                }},
                "suppressed": false,
                "references": []
            },
            {
                "id": "44444444-4444-4444-8444-444444444444",
                "name": "Extrude1",
                "operation": { "type": "Extrude", "params": {
                    "sketch_id": sketch_fid,
                    "profile_index": 0,
                    "depth": 0.010,
                    "depth_expr": depth_expr,
                    "symmetric": false,
                    "cut": false,
                    "merge": false,
                    "combine": { "type": "NewBody" }
                }},
                "suppressed": false,
                "references": []
            }
        ],
        "parameters": parameter
    })
}

/// The thickness of the solid the extrude produced, from the kernel.
fn built_thickness_m(
    tree: FeatureTree,
    document: Vec<DesignParameter>,
    overrides: Option<BTreeMap<String, f64>>,
) -> f64 {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    engine.tree = tree;
    engine.document_parameters = document;
    engine.parameter_overrides = overrides;
    engine.rebuild_from_scratch(&mut kernel);
    let extrude = engine
        .tree
        .features
        .iter()
        .find(|f| matches!(f.operation, Operation::Extrude { .. }))
        .expect("an extrude")
        .id;
    let result = engine
        .get_result(extrude)
        .expect("the extrude must produce a solid");
    let handle = &result.outputs[0].1.handle;
    let mesh = kernel.tessellate(handle, 1e-5).expect("tessellate");
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in mesh.vertices.chunks(3) {
        lo = lo.min(p[2] as f64);
        hi = hi.max(p[2] as f64);
    }
    hi - lo
}

fn tree_of(v: &Value) -> FeatureTree {
    serde_json::from_value(v.clone()).expect("the fixture must deserialize")
}

// ── half one: the document table ───────────────────────────────────────────

#[test]
fn a_reader_that_drops_the_document_table_builds_a_different_solid() {
    // One document: a part whose extrude depth reads a DOCUMENT variable.
    let tree = tree_json(json!([]), "stock");
    let document_json = json!([{
        "id": "55555555-5555-4555-8555-555555555555",
        "name": "stock",
        "expression": "25",
        "value": 25.0
    }]);

    // A v12 reader knows `document.parameters` and builds 25 mm.
    let document: Vec<DesignParameter> =
        serde_json::from_value(document_json.clone()).expect("document table");
    let with = built_thickness_m(tree_of(&tree), document, None);
    assert!((with - 0.025).abs() < 1e-7, "with the table: {with} m");

    // A v11 reader has no field for it: serde drops the key, the table is
    // empty, `stock` is an unknown variable, and the depth keeps the 10 mm
    // the file happened to have cached.
    let without = built_thickness_m(tree_of(&tree), Vec::new(), None);
    assert!(
        (without - 0.010).abs() < 1e-7,
        "without the table: {without} m"
    );

    assert!(
        (with - without).abs() > 1e-6,
        "one file, two solids — {with} m against {without} m"
    );
}

#[test]
fn a_shadowed_document_row_is_inert_in_both_readers() {
    // The LIMIT of the first test's claim, measured so the note does not
    // overstate it: when the tab shadows the name, dropping the document
    // table changes nothing, because the local row was winning anyway. So
    // the document table alone is not harmful in every document — it is
    // harmful in the documents that read it, which is why the first test
    // exists and this one bounds it.
    let tree = tree_json(
        json!([{
            "id": "66666666-6666-4666-8666-666666666666",
            "name": "stock",
            "expression": "6",
            "value": 6.0
        }]),
        "stock",
    );
    // The local row shadows the document row in BOTH readers, so both build
    // 6 mm and the document row is inert here — the shadowing rule, pinned
    // from the format's side.
    let with = built_thickness_m(
        tree_of(&tree),
        vec![DesignParameter::new("stock", "25")],
        None,
    );
    let without = built_thickness_m(tree_of(&tree), Vec::new(), None);
    assert!((with - 0.006).abs() < 1e-7, "{with} m");
    assert!((without - 0.006).abs() < 1e-7, "{without} m");
}

// ── half two: the instance overrides ───────────────────────────────────────

#[test]
fn a_reader_that_drops_the_instance_overrides_builds_a_different_solid() {
    // One document: a part with `height = 10`, instanced with
    // `parameter_overrides: { height: 25 }`.
    let tree = tree_json(
        json!([{
            "id": "77777777-7777-4777-8777-777777777777",
            "name": "height",
            "expression": "10",
            "value": 10.0
        }]),
        "height",
    );
    let overrides_json = json!({ "height": 25.0 });
    let overrides: BTreeMap<String, f64> =
        serde_json::from_value(overrides_json).expect("overrides");

    let with = built_thickness_m(tree_of(&tree), Vec::new(), Some(overrides));
    let without = built_thickness_m(tree_of(&tree), Vec::new(), None);
    assert!((with - 0.025).abs() < 1e-7, "with the overrides: {with} m");
    assert!(
        (without - 0.010).abs() < 1e-7,
        "without them the part's own 10 mm: {without} m"
    );
    assert!(
        (with - without).abs() > 1e-6,
        "one file, two solids — {with} m against {without} m"
    );
}

#[test]
fn an_instance_override_was_already_being_written_before_it_was_applied() {
    // Why this is not merely "a new field an old reader never saw": the key
    // has been persisted since Phase 3 with the comment "reserved; not
    // applied". A pre-P2 document can already contain one, and after P2 the
    // same bytes mean a different solid. That is a reinterpretation of an
    // existing field, which §13.3 bumps for even without a new field
    // (compare v10's pid strings).
    let inst: feature_engine::assembly::Instance = serde_json::from_value(json!({
        "id": "88888888-8888-4888-8888-888888888888",
        "name": "Instance 1",
        "source": { "tab_id": "tab-a" },
        "parameter_overrides": { "height": 25.0 }
    }))
    .expect("a Phase-3 instance with the reserved field");
    assert_eq!(
        inst.parameter_overrides
            .as_ref()
            .and_then(|o| o.get("height"))
            .copied(),
        Some(25.0)
    );
}

// ── and the compatibility claim, measured ──────────────────────────────────

#[test]
fn a_document_without_either_field_writes_the_same_bytes_as_before() {
    // Both fields are omitted when empty, so no existing document's bytes
    // change — the half of the argument that DOES hold, pinned so a future
    // edit cannot quietly start emitting `"parameters": []`.
    let meta = file_format::DocumentMetadata::new("Doc");
    let json = serde_json::to_value(&meta).unwrap();
    assert!(
        !json.as_object().unwrap().contains_key("parameters"),
        "an empty document table must not be written: {json}"
    );

    let inst: feature_engine::assembly::Instance = serde_json::from_value(json!({
        "id": "99999999-9999-4999-8999-999999999999",
        "name": "Instance 1",
        "source": { "tab_id": "tab-a" }
    }))
    .unwrap();
    let json = serde_json::to_value(&inst).unwrap();
    assert!(
        !json
            .as_object()
            .unwrap()
            .contains_key("parameter_overrides"),
        "an absent override map must not be written: {json}"
    );
}

#[test]
fn the_document_table_round_trips_with_every_sidecar() {
    let mut meta = file_format::DocumentMetadata::new("Doc");
    meta.parameters = vec![
        DesignParameter::new("tube_id", "30"),
        DesignParameter::new("turn", "90").with_unit(feature_engine::expr::Dimension::Angle),
    ];
    meta.parameters[0].comment = Some("inner diameter of the stock tube".to_string());
    let text = serde_json::to_string(&meta).unwrap();
    let back: file_format::DocumentMetadata = serde_json::from_str(&text).unwrap();
    assert_eq!(back.parameters.len(), 2);
    assert_eq!(back.parameters[0].name, "tube_id");
    assert_eq!(back.parameters[0].expression, "30");
    assert_eq!(
        back.parameters[0].comment.as_deref(),
        Some("inner diameter of the stock tube")
    );
    assert_eq!(
        back.parameters[1].unit,
        Some(feature_engine::expr::Dimension::Angle)
    );
    // And the second trip is byte-identical.
    assert_eq!(serde_json::to_string(&back).unwrap(), text);
}

#[test]
fn a_pre_v12_document_still_loads_with_an_empty_document_table() {
    let meta: file_format::DocumentMetadata = serde_json::from_value(json!({
        "id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "name": "Legacy",
        "created": "2026-01-01T00:00:00.000Z",
        "modified": "2026-01-01T00:00:00.000Z"
    }))
    .expect("a v11 document must still load");
    assert!(meta.parameters.is_empty());
    assert!(
        meta.extra.is_empty(),
        "`parameters` must be a real field, not swept into `extra`: {:?}",
        meta.extra
    );
}

#[test]
fn an_unknown_document_key_still_lands_in_extra_beside_the_new_field() {
    // The flattened `extra` map and a named `parameters` field coexist:
    // serde matches the named field first. Pinned because a regression here
    // would either lose the table or lose every preserved unknown key.
    let meta: file_format::DocumentMetadata = serde_json::from_value(json!({
        "id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
        "name": "Doc",
        "created": "2026-01-01T00:00:00.000Z",
        "modified": "2026-01-01T00:00:00.000Z",
        "parameters": [
            { "id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
              "name": "w", "expression": "20", "value": 20.0 }
        ],
        "someFutureKey": { "kept": true }
    }))
    .unwrap();
    assert_eq!(meta.parameters.len(), 1);
    assert_eq!(meta.parameters[0].name, "w");
    assert!(meta.extra.contains_key("someFutureKey"));
    let _ = Uuid::nil();
}
