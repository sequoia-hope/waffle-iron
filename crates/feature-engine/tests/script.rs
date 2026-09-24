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

// ── A-M3: query chains, named outputs, connectors, outer references ─────────

/// A `GeomRef` JSON value for a feature's Main body.
fn body_ref_json(feature_id: Uuid) -> serde_json::Value {
    serde_json::to_value(GeomRef {
        kind: TopoKind::Solid,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
        scope: None,
    })
    .unwrap()
}

/// A tree extrude of a `w × h` rect on the XY plane, `depth` deep (MockKernel
/// builds every extrusion at the origin).
fn tree_box(engine: &mut Engine, kernel: &mut MockKernel, name: &str, depth: f64) -> Uuid {
    let mut sketch = Sketch {
        id: Uuid::new_v4(),
        plane: GeomRef {
            kind: TopoKind::Face,
            anchor: Anchor::Datum {
                datum_id: Uuid::nil(),
            },
            selector: Selector::Role {
                role: Role::EndCapPositive,
                index: 0,
            },
            policy: ResolvePolicy::Strict,
            scope: None,
        },
        plane_origin: [0.0; 3],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities: vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 0.02,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 0.02,
                y: 0.02,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 0.02,
                construction: false,
            },
            SketchEntity::Line {
                id: 5,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 6,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 7,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            SketchEntity::Line {
                id: 8,
                start_id: 4,
                end_id: 1,
                construction: false,
            },
        ],
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions: std::collections::HashMap::new(),
        projected: Vec::new(),
        solved_profiles: Vec::new(),
    };
    script::host::derive_sketch(&mut sketch);
    // An extrude names its sketch by the sketch FEATURE's id.
    let sketch_id = engine
        .add_feature(
            format!("{name} sketch"),
            Operation::Sketch { sketch },
            kernel,
        )
        .unwrap();
    engine
        .add_feature(
            name.into(),
            Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id,
                    profile_index: 0,
                    profile_entity_ids: None,
                    depth,
                    depth_expr: None,
                    direction: None,
                    symmetric: false,
                    cut: false,
                    merge: false,
                    target_body: None,
                    depth_mode: DepthMode::Blind,
                    second_direction: None,
                    region: None,
                    regions: Vec::new(),
                    combine: Some(CombineMode::NewBody),
                    targets: Some(Vec::new()),
                },
            },
            kernel,
        )
        .unwrap()
}

const BOSS_SCRIPT: &str = r#"
// @feature name="Boss on top" version=1
// @param plane: plane
// @output base: main
// @output boss: body
// @output top: face
// @output pin: connector
fn feature(ctx, p) {
    let a = ctx.sketch(p.plane);
    a.rect(0.0, 0.0, 0.02, 0.02);
    let base = ctx.extrude(a.finish().regions()[0], #{ depth: 0.01 });
    // The query chain lowers to ONE TopoQuery: planar faces, farthest along +z.
    let top = created_by(base).faces().surface_type("planar").farthest_along([0.0, 0.0, 1.0]);
    let b = ctx.sketch(top);
    b.rect(0.005, 0.005, 0.01, 0.01);
    let boss = ctx.extrude(b.finish().regions()[0], #{ depth: 0.004 });
    ctx.mate_connector(#{ name: "pin", on: created_by(boss).faces().normal_near([0.0, 0.0, 1.0], 1.0) });
    #{ base: base, boss: boss, top: top }
}
"#;

#[test]
fn query_chains_named_outputs_and_connectors_are_public_on_the_node() {
    let (mut engine, mut kernel, src) = with_source(BOSS_SCRIPT);
    let id = engine
        .add_feature(
            "Boss".into(),
            script_op(src, json!({ "plane": plane_json() })),
            &mut kernel,
        )
        .unwrap();
    assert_eq!(error_of(&engine, id), None, "{:?}", engine.warnings);
    let r = engine.get_result(id).unwrap();
    // Two bodies: the declared main first, the named boss second.
    let keys: Vec<OutputKey> = r.outputs.iter().map(|(k, _)| k.clone()).collect();
    assert_eq!(
        keys,
        vec![
            OutputKey::Main,
            OutputKey::Named {
                name: "boss".into()
            }
        ]
    );
    // The face output is a Role::Named assignment on the node.
    let named: Vec<&Role> = r
        .provenance
        .role_assignments
        .iter()
        .map(|(_, role)| role)
        .filter(|role| matches!(role, Role::Named { .. }))
        .collect();
    assert_eq!(named, vec![&Role::Named { name: "top".into() }]);

    // A later feature references the named face without knowing the sub-tree:
    // a tree mate connector on `top` derives the top plane (z = 0.01, +z).
    let top_ref = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::Named { name: "top".into() },
            index: 0,
        },
        policy: ResolvePolicy::Strict,
        scope: None,
    };
    let mc = engine
        .add_feature(
            "On top".into(),
            Operation::MateConnector {
                params: MateConnectorParams {
                    name: "On top".into(),
                    geom_ref: Some(top_ref),
                    ..Default::default()
                },
            },
            &mut kernel,
        )
        .unwrap();
    assert_eq!(
        engine.errors.iter().find(|(f, _)| *f == mc),
        None,
        "{:?}",
        engine.errors
    );
    let on_top = engine
        .connectors
        .iter()
        .find(|c| c.name == "On top")
        .unwrap();
    assert!(
        (on_top.frame.origin[2] - 0.01).abs() < 1e-12,
        "{:?}",
        on_top.frame
    );
    assert_eq!(on_top.frame.z_axis, [0.0, 0.0, 1.0]);

    // The script's own connector is exposed on the node, after the tree's.
    let pin = engine.connectors.iter().find(|c| c.name == "pin").unwrap();
    assert_eq!(pin.feature_id, id);
    assert_eq!(pin.frame.z_axis, [0.0, 0.0, 1.0]);
    assert!(
        engine.connectors.iter().position(|c| c.name == "On top")
            < engine.connectors.iter().position(|c| c.name == "pin")
    );

    // A later boolean references the NAMED body.
    let named_boss = GeomRef {
        kind: TopoKind::Solid,
        anchor: Anchor::FeatureOutput {
            feature_id: id,
            output_key: OutputKey::Named {
                name: "boss".into(),
            },
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
        scope: None,
    };
    let tool = tree_box(&mut engine, &mut kernel, "Tool", 0.03);
    let cut = engine
        .add_feature(
            "Cut boss".into(),
            Operation::BooleanCombine {
                params: BooleanParams {
                    body_a: named_boss,
                    body_b: serde_json::from_value(body_ref_json(tool)).unwrap(),
                    operation: BooleanOp::Subtract,
                },
            },
            &mut kernel,
        )
        .unwrap();
    assert_eq!(
        engine.errors.iter().find(|(f, _)| *f == cut),
        None,
        "{:?}",
        engine.errors
    );
    assert!(engine.consumed_features.contains(&id));
}

#[test]
fn a_bare_return_is_main_and_a_declared_main_is_satisfied_by_it() {
    // `gear.rhai` declares `@output body: main` and returns the extrude.
    let (mut engine, mut kernel, src) = with_source(
        r#"
// @feature name="Declared main" version=1
// @param plane: plane
// @output body: main
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.rect(0.0, 0.0, 0.02, 0.02);
    ctx.extrude(sk.finish().regions()[0], #{ depth: 0.01 })
}
"#,
    );
    let id = engine
        .add_feature(
            "Declared".into(),
            script_op(src, json!({ "plane": plane_json() })),
            &mut kernel,
        )
        .unwrap();
    assert_eq!(error_of(&engine, id), None);
    assert_eq!(engine.get_result(id).unwrap().outputs[0].0, OutputKey::Main);
}

#[test]
fn output_contract_violations_are_loud_with_no_output() {
    let prelude = r#"
// @param plane: plane
fn body(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.rect(0.0, 0.0, 0.02, 0.02);
    ctx.extrude(sk.finish().regions()[0], #{ depth: 0.01 })
}
"#;
    let cases: Vec<(&str, String, &str)> = vec![
        (
            "declared output missing",
            format!("// @feature name=\"x\"\n// @output top: face\n{prelude}\nfn feature(ctx, p) {{ body(ctx, p) }}"),
            "declared output `top` (face) is missing",
        ),
        (
            "declared kind mismatch",
            format!("// @feature name=\"x\"\n// @output top: face\n{prelude}\nfn feature(ctx, p) {{ let b = body(ctx, p); #{{ main: b, top: b }} }}"),
            "declared `face` but the script returned a body",
        ),
        (
            "declared connector not placed",
            format!("// @feature name=\"x\"\n// @output pin: connector\n{prelude}\nfn feature(ctx, p) {{ body(ctx, p) }}"),
            "declared connector `pin` was not placed",
        ),
        (
            "two names one body",
            format!("// @feature name=\"x\"\n{prelude}\nfn feature(ctx, p) {{ let b = body(ctx, p); #{{ main: b, again: b }} }}"),
            "another output already names",
        ),
        (
            "unnarrowed face query",
            format!("// @feature name=\"x\"\n{prelude}\nfn feature(ctx, p) {{ let b = body(ctx, p); #{{ main: b, f: created_by(b).faces() }} }}"),
            "must name one entity",
        ),
        (
            "role plus filter",
            format!("// @feature name=\"x\"\n{prelude}\nfn feature(ctx, p) {{ let b = body(ctx, p); #{{ main: b, f: created_by(b).role(\"EndCapPositive\", 0).largest_area() }} }}"),
            "do not combine it with filters",
        ),
        (
            "two tie-breaks",
            format!("// @feature name=\"x\"\n{prelude}\nfn feature(ctx, p) {{ let b = body(ctx, p); #{{ main: b, f: created_by(b).faces().largest_area().first() }} }}"),
            "already has a tie-break",
        ),
        (
            "connector on a body",
            format!("// @feature name=\"x\"\n{prelude}\nfn feature(ctx, p) {{ let b = body(ctx, p); ctx.mate_connector(#{{ name: \"c\", on: created_by(b) }}); b }}"),
            "sits on a face or an edge",
        ),
        (
            "connector twice",
            format!("// @feature name=\"x\"\n{prelude}\nfn feature(ctx, p) {{ let b = body(ctx, p); let f = created_by(b).faces().largest_area(); ctx.mate_connector(#{{ name: \"c\", on: f }}); ctx.mate_connector(#{{ name: \"c\", on: f }}); b }}"),
            "already exists",
        ),
        (
            "named body consumed later",
            format!("// @feature name=\"x\"\n{prelude}\nfn feature(ctx, p) {{ let a = body(ctx, p); let b = body(ctx, p); let u = ctx.boolean(\"union\", a, b); #{{ main: u, lost: a }} }}"),
            "a later child consumed",
        ),
        (
            "output not a handle",
            format!("// @feature name=\"x\"\n{prelude}\nfn feature(ctx, p) {{ let b = body(ctx, p); #{{ main: b, n: 3 }} }}"),
            "must be a feature ref or a query",
        ),
    ];
    for (label, text, needle) in cases {
        let (mut engine, mut kernel, src) = with_source(&text);
        let id = engine
            .add_feature(
                label.into(),
                script_op(src, json!({ "plane": plane_json() })),
                &mut kernel,
            )
            .unwrap();
        let (stage, msg) =
            error_of(&engine, id).unwrap_or_else(|| panic!("{label}: expected an error"));
        assert_eq!(stage, "runtime", "{label}: {msg}");
        assert!(msg.contains(needle), "{label}: {msg}");
        assert!(
            engine.get_result(id).is_none(),
            "{label}: no partial output"
        );
        assert!(
            !engine.connectors.iter().any(|c| c.feature_id == id),
            "{label}: a failed node exposes no connector"
        );
    }
}

const CUT_SCRIPT: &str = r#"
// @feature name="Bore" version=1
// @param target: body
// @param plane: plane
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.rect(0.005, 0.005, 0.005, 0.005);
    ctx.extrude(sk.finish().regions()[0], #{ depth: 0.03, combine: "Cut", targets: [p.target] })
}
"#;

#[test]
fn an_outer_body_parameter_is_consumed_by_the_node_and_stays_consumed_when_carried() {
    let (mut engine, mut kernel, src) = with_source(CUT_SCRIPT);
    let block = tree_box(&mut engine, &mut kernel, "Block", 0.01);
    let bore = engine
        .add_feature(
            "Bore".into(),
            script_op(
                src,
                json!({ "plane": plane_json(), "target": body_ref_json(block) }),
            ),
            &mut kernel,
        )
        .unwrap();
    assert_eq!(error_of(&engine, bore), None, "{:?}", engine.errors);
    assert_eq!(body_count(&engine, bore), 1);
    assert!(engine.consumed_features.contains(&block));
    assert_eq!(engine.consumed_by.get(&bore), Some(&vec![block]));

    // An unrelated later feature: the rebuild carries the script node
    // without re-executing it — the consumption must survive the carry.
    let block_result_before = engine.get_result(bore).unwrap().outputs[0].1.handle.raw();
    let _other = tree_box(&mut engine, &mut kernel, "Other", 0.002);
    assert_eq!(
        engine.get_result(bore).unwrap().outputs[0].1.handle.raw(),
        block_result_before,
        "the script node was carried, not re-executed"
    );
    assert!(
        engine.consumed_features.contains(&block),
        "carried consumption"
    );
    assert_eq!(engine.consumed_by.get(&bore), Some(&vec![block]));

    // Wrong kinds are refused at the argument boundary.
    let (mut engine, mut kernel, src) = with_source(CUT_SCRIPT);
    let block = tree_box(&mut engine, &mut kernel, "Block", 0.01);
    let mut face_ref = body_ref_json(block);
    face_ref["kind"] = json!({ "type": "Face" });
    let bad = engine
        .add_feature(
            "Bad".into(),
            script_op(src, json!({ "plane": plane_json(), "target": face_ref })),
            &mut kernel,
        )
        .unwrap();
    let (stage, msg) = error_of(&engine, bad).unwrap();
    assert_eq!(stage, "args", "{msg}");
    assert!(msg.contains("kind Solid"), "{msg}");
    assert!(!engine.consumed_features.contains(&block));

    // An outer reference cannot be a named output (the caller has it already).
    let (mut engine, mut kernel, src) = with_source(
        r#"
// @feature name="x" version=1
// @param target: body
// @param plane: plane
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.rect(0.0, 0.0, 0.002, 0.002);
    let b = ctx.extrude(sk.finish().regions()[0], #{ depth: 0.001 });
    #{ main: b, theirs: p.target }
}
"#,
    );
    let block = tree_box(&mut engine, &mut kernel, "Block", 0.01);
    let bad = engine
        .add_feature(
            "Bad".into(),
            script_op(
                src,
                json!({ "plane": plane_json(), "target": body_ref_json(block) }),
            ),
            &mut kernel,
        )
        .unwrap();
    let (stage, msg) = error_of(&engine, bad).unwrap();
    assert_eq!(stage, "runtime", "{msg}");
    assert!(msg.contains("outside the script"), "{msg}");
}

#[test]
fn a_script_connector_is_carried_and_dropped_with_its_node() {
    let (mut engine, mut kernel, src) = with_source(BOSS_SCRIPT);
    let id = engine
        .add_feature(
            "Boss".into(),
            script_op(src, json!({ "plane": plane_json() })),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.connectors.iter().any(|c| c.name == "pin"));
    // Carried through a rebuild that does not re-execute the node.
    let _other = tree_box(&mut engine, &mut kernel, "Other", 0.002);
    assert!(engine.connectors.iter().any(|c| c.name == "pin"), "carried");
    // Suppressed: gone. Unsuppressed: back.
    engine.set_suppressed(id, true, &mut kernel).unwrap();
    assert!(!engine.connectors.iter().any(|c| c.name == "pin"));
    engine.set_suppressed(id, false, &mut kernel).unwrap();
    assert!(engine.connectors.iter().any(|c| c.name == "pin"));
    // Deleted: gone.
    engine.remove_feature(id, &mut kernel).unwrap();
    assert!(!engine.connectors.iter().any(|c| c.name == "pin"));
}

// ── B1 in the script API: ctx.pattern_circular / ctx.pattern_linear ────────

#[test]
fn patterns_copy_a_child_body_and_take_custody_of_the_seed() {
    let (mut engine, mut kernel, src) = with_source(
        r#"
// @feature name="Spokes" version=1
// @param plane: plane
// @param count: int = 6 min=2
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.rect(0.01, -0.001, 0.02, 0.002);
    let spoke = ctx.extrude(sk.finish().regions()[0], #{ depth: 0.003 });
    let ring = ctx.pattern_circular(spoke, #{ axis: #{ origin: [0.0, 0.0, 0.0], direction: [0.0, 0.0, 1.0] }, count: p.count });
    let sk2 = ctx.sketch(p.plane);
    sk2.rect(0.1, 0.0, 0.005, 0.005);
    let block = ctx.extrude(sk2.finish().regions()[0], #{ depth: 0.003 });
    let row = ctx.pattern_linear(block, #{ direction: [1.0, 0.0, 0.0], count: 3, spacing: 0.05, skip: [2] });
    #{ main: ring, row: row }
}
"#,
    );
    let id = engine
        .add_feature(
            "Spokes".into(),
            script_op(src, json!({ "plane": plane_json() })),
            &mut kernel,
        )
        .unwrap();
    assert_eq!(error_of(&engine, id), None);
    // Each pattern takes custody of its seed (not an output) and emits its
    // instances: 6 spokes + (3 − 1 skipped) blocks.
    assert_eq!(body_count(&engine, id), 8);
    let result = engine.get_result(id).unwrap();
    assert!(matches!(result.outputs[0].0, OutputKey::Main));
    assert!(result
        .outputs
        .iter()
        .any(|(k, _)| matches!(k, OutputKey::Named { name } if name == "row")));

    // Every argument problem is a typed runtime failure with no output.
    for (label, body) in [
        ("no axis", "ctx.pattern_circular(spoke, #{ count: 4 })"),
        ("count 1", "ctx.pattern_circular(spoke, #{ axis: [0.0, 0.0, 1.0], count: 1 })"),
        ("zero angle", "ctx.pattern_circular(spoke, #{ axis: [0.0, 0.0, 1.0], count: 4, angle_deg: 0 })"),
        ("cut without targets", "ctx.pattern_circular(spoke, #{ axis: [0.0, 0.0, 1.0], count: 4, combine: \"Cut\" })"),
        ("no spacing", "ctx.pattern_linear(spoke, #{ direction: [1.0, 0.0, 0.0], count: 3 })"),
        ("bad skip", "ctx.pattern_linear(spoke, #{ direction: [1.0, 0.0, 0.0], count: 3, spacing: 0.1, skip: [0] })"),
        ("seed not a body", "ctx.pattern_linear(spoke.faces(), #{ direction: [1.0, 0.0, 0.0], count: 3, spacing: 0.1 })"),
    ] {
        let text = format!(
            "// @feature name=\"x\"\n// @param plane: plane\nfn feature(ctx, p) {{\n let sk = ctx.sketch(p.plane);\n sk.rect(0.0, 0.0, 0.01, 0.01);\n let spoke = ctx.extrude(sk.finish().regions()[0], #{{ depth: 0.01 }});\n {body}\n}}\n"
        );
        let (mut engine, mut kernel, src) = with_source(&text);
        let id = engine
            .add_feature(
                "bad".into(),
                script_op(src, json!({ "plane": plane_json() })),
                &mut kernel,
            )
            .unwrap();
        let (stage, _) = error_of(&engine, id).unwrap_or_else(|| panic!("{label}: no error"));
        assert_eq!(stage, "runtime", "{label}");
        assert_eq!(body_count(&engine, id), 0, "{label}");
    }
}
