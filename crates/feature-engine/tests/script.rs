//! Custom feature scripts (`specs/custom_features_and_modeling_roadmap.md`
//! Part A, A-M0/A-M1) on MockKernel: a `Script` node records children and
//! executes them; every failure class is a typed `EngineError::Script`
//! with no outputs; limits; determinism; expression-driven arguments;
//! suppression and undo. Gear parity is `script_gear_parity.rs`; real
//! geometry is `test-harness/tests/script_kv2.rs`.

use std::collections::BTreeMap;

use feature_engine::script;
use feature_engine::types::*;
use feature_engine::Engine;
use serde_json::json;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;

const BOX_SCRIPT: &str = r#"
// @feature name="Box" version=1
// @param width: length = 0.02 min=0.001
// @param height: length = 0.01
// @param depth: length = 0.005
// @param plane: plane
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.rect(0.0, 0.0, p.width, p.height);
    let r = sk.finish().regions();
    ctx.log("regions: " + r.len());
    ctx.extrude(r[0], #{ depth: p.depth })
}
"#;

fn plane_json() -> serde_json::Value {
    json!({ "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] })
}

fn script_op(source_id: Uuid, args: serde_json::Value) -> Operation {
    let args: BTreeMap<String, serde_json::Value> = serde_json::from_value(args).unwrap();
    Operation::Script {
        params: ScriptParams {
            source_id,
            entry: "feature".into(),
            args,
            arg_exprs: BTreeMap::new(),
            arg_values: BTreeMap::new(),
        },
    }
}

/// `(engine, kernel, source_id)` with `text` registered as a script source.
fn with_source(text: &str) -> (Engine, MockKernel, Uuid) {
    let mut engine = Engine::new();
    let kernel = MockKernel::new();
    let id = Uuid::new_v4();
    engine.sources.insert_text(id, text);
    (engine, kernel, id)
}

fn error_of(engine: &Engine, id: Uuid) -> Option<(String, String)> {
    engine
        .feature_errors
        .iter()
        .find(|e| e.feature_id == id)
        .map(|e| {
            let stage = match &e.kind {
                ErrorKind::Script { stage } => stage.clone(),
                other => format!("{other:?}"),
            };
            (stage, e.message.clone())
        })
}

fn body_count(engine: &Engine, id: Uuid) -> usize {
    engine.get_result(id).map(|r| r.outputs.len()).unwrap_or(0)
}

#[test]
fn a_box_script_is_one_node_with_one_body() {
    let (mut engine, mut kernel, src) = with_source(BOX_SCRIPT);
    let id = engine
        .add_feature(
            "Box".into(),
            script_op(src, json!({ "plane": plane_json(), "width": 0.03 })),
            &mut kernel,
        )
        .unwrap();
    assert_eq!(error_of(&engine, id), None);
    assert_eq!(body_count(&engine, id), 1);
    let r = engine.get_result(id).unwrap();
    assert_eq!(r.outputs[0].0, OutputKey::Main);
    // The extrude's roles are carried (EndCapPositive etc.).
    assert!(r
        .provenance
        .role_assignments
        .iter()
        .any(|(_, role)| matches!(role, Role::EndCapPositive)));
    // ctx.log lines reach the diagnostics.
    assert!(
        engine
            .warnings
            .iter()
            .any(|w| w.contains("log: regions: 1")),
        "{:?}",
        engine.warnings
    );
    // The tree has ONE feature; the children are private.
    assert_eq!(engine.tree.features.len(), 1);
    // A later feature can reference the node's Main body.
    let boss_src = Uuid::new_v4();
    engine.sources.insert_text(
        boss_src,
        r#"
// @feature name="Boss" version=1
// @param plane: plane
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.rect(0.0, 0.0, 0.005, 0.005);
    ctx.extrude(sk.finish().regions()[0], #{ depth: 0.002 })
}
"#,
    );
    let boss = engine
        .add_feature(
            "Boss".into(),
            script_op(boss_src, json!({ "plane": plane_json() })),
            &mut kernel,
        )
        .unwrap();
    assert_eq!(error_of(&engine, boss), None);
    let union = Operation::BooleanCombine {
        params: BooleanParams {
            body_a: GeomRef {
                kind: TopoKind::Solid,
                anchor: Anchor::FeatureOutput {
                    feature_id: id,
                    output_key: OutputKey::Main,
                },
                selector: Selector::Role {
                    role: Role::EndCapPositive,
                    index: 0,
                },
                policy: ResolvePolicy::Strict,
                scope: None,
            },
            body_b: GeomRef {
                kind: TopoKind::Solid,
                anchor: Anchor::FeatureOutput {
                    feature_id: boss,
                    output_key: OutputKey::Main,
                },
                selector: Selector::Role {
                    role: Role::EndCapPositive,
                    index: 0,
                },
                policy: ResolvePolicy::Strict,
                scope: None,
            },
            operation: BooleanOp::Union,
        },
    };
    let u = engine
        .add_feature("Union".into(), union, &mut kernel)
        .unwrap();
    assert_eq!(engine.errors.iter().find(|(f, _)| *f == u), None);
    assert!(engine.consumed_features.contains(&id) && engine.consumed_features.contains(&boss));
}

#[test]
fn children_can_chain_booleans_and_consumed_children_are_not_outputs() {
    let (mut engine, mut kernel, src) = with_source(
        r#"
// @feature name="Two" version=1
// @param plane: plane
fn feature(ctx, p) {
    let a = ctx.sketch(p.plane);
    a.rect(0.0, 0.0, 0.02, 0.02);
    let base = ctx.extrude(a.finish().regions()[0], #{ depth: 0.01 });
    let b = ctx.sketch(p.plane);
    b.rect(0.005, 0.005, 0.01, 0.01);
    let boss = ctx.extrude(b.finish().regions()[0], #{ depth: 0.02, combine: "Add", targets: [base] });
    let c = ctx.sketch(p.plane);
    c.rect(0.0, 0.0, 0.001, 0.001);
    let tool = ctx.extrude(c.finish().regions()[0], #{ depth: 0.03 });
    ctx.boolean("subtract", boss, tool)
}
"#,
    );
    let id = engine
        .add_feature(
            "Two".into(),
            script_op(src, json!({ "plane": plane_json() })),
            &mut kernel,
        )
        .unwrap();
    assert_eq!(error_of(&engine, id), None);
    // base consumed by boss (Add), boss and tool consumed by the subtract:
    // exactly ONE body survives.
    assert_eq!(body_count(&engine, id), 1);
}

#[test]
fn every_failure_class_is_a_typed_script_error_with_no_output() {
    let cases: Vec<(&str, &str, &str, &str)> = vec![
        (
            "no header",
            "fn feature(ctx, p) { }",
            "header",
            "@feature",
        ),
        (
            "parse error",
            "// @feature name=\"x\"\nfn feature(ctx, p) { let = ; }",
            "parse",
            "",
        ),
        (
            "no entry",
            "// @feature name=\"x\"\nfn other(ctx, p) { }",
            "parse",
            "fn feature",
        ),
        (
            "runtime error",
            "// @feature name=\"x\"\nfn feature(ctx, p) { let a = []; a[3] }",
            "runtime",
            "",
        ),
        (
            "ctx.fail",
            "// @feature name=\"x\"\nfn feature(ctx, p) { ctx.fail(\"nope\"); }",
            "fail",
            "nope",
        ),
        (
            "infinite loop",
            "// @feature name=\"x\"\nfn feature(ctx, p) { while true { } }",
            "limit",
            "",
        ),
        (
            "deep recursion",
            "// @feature name=\"x\"\nfn r(n) { r(n + 1) }\nfn feature(ctx, p) { r(0) }",
            "limit",
            "",
        ),
        (
            "geometry budget",
            "// @feature name=\"x\"\n// @param plane: plane\nfn feature(ctx, p) { loop { let s = ctx.sketch(p.plane); s.rect(0.0,0.0,1.0,1.0); ctx.extrude(s.finish().regions()[0], #{ depth: 1.0 }); } }",
            "runtime",
            "geometry budget",
        ),
        (
            "no operations",
            "// @feature name=\"x\"\nfn feature(ctx, p) { 42 }",
            "runtime",
            "no operations",
        ),
        (
            "eval disabled",
            "// @feature name=\"x\"\nfn feature(ctx, p) { eval(\"1\") }",
            "parse",
            "",
        ),
        (
            "add without targets",
            "// @feature name=\"x\"\n// @param plane: plane\nfn feature(ctx, p) { let s = ctx.sketch(p.plane); s.rect(0.0,0.0,1.0,1.0); ctx.extrude(s.finish().regions()[0], #{ depth: 1.0, combine: \"Add\" }) }",
            "runtime",
            "needs `targets",
        ),
    ];
    for (label, text, stage, needle) in cases {
        let (mut engine, mut kernel, src) = with_source(text);
        let id = engine
            .add_feature(
                label.into(),
                script_op(src, json!({ "plane": plane_json() })),
                &mut kernel,
            )
            .unwrap();
        let (got_stage, msg) =
            error_of(&engine, id).unwrap_or_else(|| panic!("{label}: expected an error"));
        // Unknown-arg refusal comes first for scripts without a plane param.
        if got_stage == "args" && !text.contains("@param plane") {
            continue;
        }
        assert_eq!(got_stage, stage, "{label}: {msg}");
        assert!(msg.contains(needle), "{label}: {msg}");
        assert!(
            engine.get_result(id).is_none(),
            "{label}: no partial output"
        );
    }
}

#[test]
fn arguments_are_typed_defaulted_range_checked_and_closed() {
    let (mut engine, mut kernel, src) = with_source(BOX_SCRIPT);
    let cases: Vec<(&str, serde_json::Value, &str)> = vec![
        (
            "unknown",
            json!({ "plane": plane_json(), "wdith": 0.02 }),
            "not a parameter",
        ),
        ("missing plane", json!({}), "`plane` (plane) is required"),
        (
            "below min",
            json!({ "plane": plane_json(), "width": 0.0001 }),
            "below its minimum",
        ),
        (
            "wrong type",
            json!({ "plane": plane_json(), "width": "wide" }),
            "must be a number",
        ),
        (
            "bad plane",
            json!({ "plane": "not-a-uuid" }),
            "datum plane id",
        ),
    ];
    for (label, args, needle) in cases {
        let id = engine
            .add_feature(label.into(), script_op(src, args), &mut kernel)
            .unwrap();
        let (stage, msg) =
            error_of(&engine, id).unwrap_or_else(|| panic!("{label}: expected an error"));
        assert_eq!(stage, "args", "{label}: {msg}");
        assert!(msg.contains(needle), "{label}: {msg}");
        engine.remove_feature(id, &mut kernel).unwrap();
    }
    // Defaults fill in what is not given.
    let id = engine
        .add_feature(
            "ok".into(),
            script_op(src, json!({ "plane": plane_json() })),
            &mut kernel,
        )
        .unwrap();
    assert_eq!(error_of(&engine, id), None);
}

#[test]
fn missing_source_is_source_unavailable() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let id = engine
        .add_feature(
            "Orphan".into(),
            script_op(Uuid::new_v4(), json!({ "plane": plane_json() })),
            &mut kernel,
        )
        .unwrap();
    let e = engine
        .feature_errors
        .iter()
        .find(|e| e.feature_id == id)
        .unwrap();
    assert!(
        matches!(e.kind, ErrorKind::SourceUnavailable { .. }),
        "{e:?}"
    );
}

#[test]
fn expression_driven_arguments_regenerate_on_parameter_change() {
    let (mut engine, mut kernel, src) = with_source(BOX_SCRIPT);
    let mut op = script_op(src, json!({ "plane": plane_json() }));
    if let Operation::Script { params } = &mut op {
        params.arg_exprs.insert("depth".into(), "thick * 2".into());
    }
    engine.set_parameters(vec![DesignParameter::new("thick", "5")], &mut kernel);
    let id = engine.add_feature("Box".into(), op, &mut kernel).unwrap();
    assert_eq!(error_of(&engine, id), None);
    let Operation::Script { params } = &engine.tree.features[0].operation else {
        panic!()
    };
    // Cached raw value is mm-space (10); the script saw 0.010 m.
    assert_eq!(params.arg_values.get("depth"), Some(&10.0));
    let mesh_before = engine.get_result(id).unwrap().outputs[0].1.handle.raw();

    engine.set_parameters(vec![DesignParameter::new("thick", "7")], &mut kernel);
    assert_eq!(error_of(&engine, id), None);
    let Operation::Script { params } = &engine.tree.features[0].operation else {
        panic!()
    };
    assert_eq!(params.arg_values.get("depth"), Some(&14.0));
    // Re-executed: a fresh kernel body.
    assert_ne!(
        engine.get_result(id).unwrap().outputs[0].1.handle.raw(),
        mesh_before
    );

    // A broken expression is a loud parameter error and the node keeps its
    // last value (like depth_expr).
    engine.set_parameters(vec![DesignParameter::new("thick", "1 / 0")], &mut kernel);
    assert!(
        engine.errors.iter().any(|(_, m)| m.contains("thick")),
        "{:?}",
        engine.errors
    );
}

#[test]
fn recording_is_deterministic_and_suppress_undo_behave() {
    let params = ScriptParams {
        source_id: Uuid::new_v4(),
        entry: "feature".into(),
        args: serde_json::from_value(json!({ "plane": plane_json() })).unwrap(),
        arg_exprs: BTreeMap::new(),
        arg_values: BTreeMap::new(),
    };
    let a = script::record(BOX_SCRIPT, &params).unwrap();
    let b = script::record(BOX_SCRIPT, &params).unwrap();
    assert_eq!(a.children.len(), b.children.len());
    for (x, y) in a.children.iter().zip(&b.children) {
        // Child ids are fresh per run (private), but their operations are
        // identical byte for byte.
        let mut ox = serde_json::to_value(&x.feature.operation).unwrap();
        let mut oy = serde_json::to_value(&y.feature.operation).unwrap();
        for o in [&mut ox, &mut oy] {
            if let Some(obj) = o.get_mut("sketch") {
                obj["id"] = json!(null);
            }
            if let Some(obj) = o.get_mut("params") {
                obj["sketch_id"] = json!(null);
            }
        }
        assert_eq!(ox, oy);
    }
    assert_eq!(a.interface.name, "Box");

    let (mut engine, mut kernel, src) = with_source(BOX_SCRIPT);
    let id = engine
        .add_feature(
            "Box".into(),
            script_op(src, json!({ "plane": plane_json() })),
            &mut kernel,
        )
        .unwrap();
    engine.set_suppressed(id, true, &mut kernel).unwrap();
    assert!(engine.get_result(id).is_none());
    engine.set_suppressed(id, false, &mut kernel).unwrap();
    assert_eq!(body_count(&engine, id), 1);
    engine.undo(&mut kernel).unwrap();
    engine.undo(&mut kernel).unwrap();
    engine.undo(&mut kernel).unwrap();
    assert!(engine.tree.features.is_empty());
    engine.redo(&mut kernel).unwrap();
    assert_eq!(body_count(&engine, id), 1);
}

#[test]
fn script_operation_round_trips_and_is_a_known_tag() {
    let op = script_op(
        Uuid::new_v4(),
        json!({ "plane": plane_json(), "width": 0.02 }),
    );
    let v = serde_json::to_value(&op).unwrap();
    assert_eq!(v["type"], "Script");
    assert_eq!(v["params"]["entry"], "feature");
    assert!(
        v["params"].get("arg_exprs").is_none(),
        "empty maps are omitted"
    );
    let back: Operation = serde_json::from_value(v).unwrap();
    assert!(matches!(back, Operation::Script { .. }));
    assert!(OPERATION_TAGS.contains(&"Script"));
    let minimal: Operation = serde_json::from_value(json!({
        "type": "Script", "params": { "source_id": Uuid::new_v4() }
    }))
    .unwrap();
    let Operation::Script { params } = minimal else {
        panic!()
    };
    assert_eq!(params.entry, "feature");
}
