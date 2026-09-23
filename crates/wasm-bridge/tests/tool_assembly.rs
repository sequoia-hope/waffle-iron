//! The assembly tools in the engine (`crates/wasm-bridge/src/tools/assembly.rs`):
//! what the page's JS answered until 2026-09-23. Run with the real kernel-v2
//! adapter, as `assembly_tests.rs` does, so the solved placements and the
//! connector frames are real. `app/tests/gui/agent-assembly.spec.js` drives
//! the same tools through the relay and the page; this pins the engine's
//! refusals and shapes without a browser.

use serde_json::{json, Value};
use uuid::Uuid;
use wasm_bridge::messages::UiToEngine;
use wasm_bridge::tools::{mutates, ASSEMBLY_TOOLS, MIGRATED};
use wasm_bridge::*;

const CUBE_STEP: &str = include_str!("../../step-import/tests/fixtures/cube.step");

struct Fixture {
    state: EngineState,
    kernel: kernel_v2::KernelV2Adapter,
    /// The Part tab holding the 10 mm cube.
    part: String,
    /// The Assembly tab, active.
    asm: String,
}

/// A document with the cube in its first Part tab and an empty, ACTIVE
/// Assembly tab.
fn fixture() -> Fixture {
    let mut state = EngineState::new();
    let mut kernel = kernel_v2::KernelV2Adapter::new();
    dispatch(
        &mut state,
        UiToEngine::ImportStep {
            file_name: "cube.step".into(),
            data: CUBE_STEP.to_string(),
        },
        &mut kernel,
    );
    let part = state.session.tabs()[0].id.clone();
    let added = execute_tool(
        &mut state,
        &mut kernel,
        "tab_add",
        &json!({ "kind": "Assembly", "name": "Asm" }),
        None,
    );
    assert!(!added.is_error, "{added:?}");
    let asm = added.structured_content["tab_id"]
        .as_str()
        .unwrap()
        .to_string();
    Fixture {
        state,
        kernel,
        part,
        asm,
    }
}

impl Fixture {
    fn ok(&mut self, name: &str, args: Value) -> Value {
        let result = execute_tool(&mut self.state, &mut self.kernel, name, &args, None);
        assert!(!result.is_error, "{name} failed: {result:?}");
        result.structured_content
    }

    fn refused(&mut self, name: &str, args: Value) -> Value {
        let result = execute_tool(&mut self.state, &mut self.kernel, name, &args, None);
        assert!(result.is_error, "{name} was expected to refuse: {result:?}");
        result.structured_content["error"].clone()
    }
}

fn id(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().to_string()
}

fn find<'a>(list: &'a Value, id: &str) -> &'a Value {
    list.as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {list}"))
}

fn vec3(v: &Value) -> [f64; 3] {
    serde_json::from_value(v.clone()).unwrap()
}

fn close(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
}

#[test]
fn assembly_tools_refuse_a_part_tab() {
    let mut f = fixture();
    f.ok("tab_switch", json!({ "tab_id": f.part }));
    for name in ["assembly_get", "instance_add", "mate_delete"] {
        let err = f.refused(name, json!({ "tab_id": f.part, "mate_id": "x" }));
        assert_eq!(err["code"], "TabKindNotSupported", "{name}");
        assert_eq!(err["details"]["kind"], "Part");
        assert!(err["message"]
            .as_str()
            .unwrap()
            .contains("assembly tools need an Assembly tab"));
    }
}

#[test]
fn instances_connectors_and_mates_round_trip_with_the_page_shapes() {
    let mut f = fixture();
    let part_name = f.state.session.tabs()[0].name.clone();
    let (part, asm) = (f.part.clone(), f.asm.clone());

    // An assembly cannot contain itself.
    let err = f.refused("instance_add", json!({ "tab_id": asm }));
    assert_eq!(err["code"], "TabNotFound");
    assert_eq!(err["details"]["tab_id"], asm);
    let err = f.refused(
        "instance_add",
        json!({ "tab_id": part, "source_id": "not-a-source" }),
    );
    assert_eq!(err["code"], "TabNotFound");

    let a = f.ok("instance_add", json!({ "tab_id": part, "fixed": true }));
    let a_id = id(&a, "instance_id");
    assert_eq!(a["tab_id"], asm);
    assert_eq!(a["name"], "Asm");
    let inst = find(&a["instances"], &a_id);
    assert_eq!(
        inst["name"],
        format!("{part_name} 1"),
        "the store's default name"
    );
    assert_eq!(inst["part_name"], part_name);
    assert_eq!(inst["fixed"], true);
    assert_eq!(inst["suppressed"], false);
    assert_eq!(inst["source"]["tab_id"], part);
    assert!(inst["source"]["source_id"].is_null());
    assert!(a["available_parts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["tab_id"] == part));
    assert!(!a["available_parts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["tab_id"] == asm));
    assert_eq!(a["errors"], json!([]));

    let b = f.ok(
        "instance_add",
        json!({ "tab_id": part, "name": "Lid", "transform": { "translation_m": [0.05, 0.0, 0.0] } }),
    );
    let b_id = id(&b, "instance_id");
    assert_eq!(b["instances"].as_array().unwrap().len(), 2);
    let lid = find(&b["instances"], &b_id);
    assert_eq!(lid["name"], "Lid");
    assert!(
        close(vec3(&lid["placement"]["translation_m"]), [0.05, 0.0, 0.0]),
        "unmated: its own transform"
    );

    // Connectors: an explicit frame on each; one refusal per rule.
    let ca = f.ok(
        "connector_add",
        json!({ "instance_path": [a_id], "frame": { "origin": [0.0, 0.0, 0.01] }, "name": "Top" }),
    );
    let ca_id = id(&ca, "connector_id");
    let top = find(&ca["connectors"], &ca_id);
    assert_eq!(top["name"], "Top");
    assert_eq!(top["anchor"], "middle");
    assert_eq!(top["flip_z"], false);
    assert_eq!(
        top["frame"],
        json!({ "origin": [0.0, 0.0, 0.01], "z_axis": [0.0, 0.0, 1.0], "x_axis": [0.0, 0.0, 0.0] })
    );
    assert!(
        top["world_frame"]["kind"].is_null(),
        "an explicit frame is derived from nothing"
    );
    assert!(close(vec3(&top["world_frame"]["origin"]), [0.0, 0.0, 0.01]));

    let cb = f.ok("connector_add", json!({ "instance_path": [b_id] }));
    let cb_id = id(&cb, "connector_id");
    assert_eq!(find(&cb["connectors"], &cb_id)["name"], "Lid connector 2");

    let err = f.refused(
        "connector_add",
        json!({ "instance_path": [a_id], "frame": {}, "part_connector": Uuid::new_v4() }),
    );
    assert_eq!(err["code"], "InvalidArguments");
    let err = f.refused("connector_add", json!({ "instance_path": ["nope"] }));
    assert_eq!(err["code"], "InstanceNotFound");
    let err = f.refused(
        "connector_add",
        json!({ "instance_path": [a_id], "part_connector": Uuid::new_v4() }),
    );
    assert_eq!(
        err["code"], "ConnectorNotFound",
        "no evaluated MateConnector feature by that id"
    );

    // Mates.
    let err = f.refused("mate_add", json!({ "a": ca_id, "b": ca_id }));
    assert_eq!(err["code"], "InvalidArguments");
    let err = f.refused("mate_add", json!({ "a": ca_id, "b": "nope" }));
    assert_eq!(err["code"], "ConnectorNotFound");

    let m = f.ok("mate_add", json!({ "a": ca_id, "b": cb_id, "flip": false }));
    let m_id = id(&m, "mate_id");
    let mate = find(&m["mates"], &m_id);
    assert_eq!(mate["name"], "Fastened 1");
    assert_eq!(mate["kind"], json!({ "type": "Fastened" }));
    assert_eq!(mate["connectors"], json!([ca_id, cb_id]));
    assert_eq!(m["errors"], json!([]));
    let lid = find(&m["instances"], &b_id);
    assert!(
        close(vec3(&lid["placement"]["translation_m"]), [0.0, 0.0, 0.01]),
        "fastened: the lid's frame coincides with the top's: {lid}"
    );

    let s = f.ok("mate_edit", json!({ "mate_id": m_id, "suppressed": true }));
    assert!(close(
        vec3(&find(&s["instances"], &b_id)["placement"]["translation_m"]),
        [0.05, 0.0, 0.0]
    ));
    let s = f.ok(
        "mate_edit",
        json!({ "mate_id": m_id, "suppressed": false, "kind": "Revolute", "name": "Hinge" }),
    );
    let mate = find(&s["mates"], &m_id);
    assert_eq!(mate["name"], "Hinge");
    assert_eq!(mate["kind"]["type"], "Revolute");
    assert_eq!(mate["suppressed"], false);

    // Connector adjustments, each default removed when set back.
    let e = f.ok(
        "connector_edit",
        json!({ "connector_id": ca_id, "offset_m": [0.0, 0.0, 0.005], "flip_z": true, "rotation_deg": 90 }),
    );
    let top = find(&e["connectors"], &ca_id);
    assert_eq!(top["offset_m"], json!([0.0, 0.0, 0.005]));
    assert_eq!(top["flip_z"], true);
    assert_eq!(top["rotation_deg"], 90.0);
    let e = f.ok(
        "connector_edit",
        json!({ "connector_id": ca_id, "offset_m": [0, 0, 0], "flip_z": false, "rotation_deg": 0, "anchor": "positive_end" }),
    );
    let top = find(&e["connectors"], &ca_id);
    assert_eq!(top["offset_m"], json!([0.0, 0.0, 0.0]));
    assert_eq!(top["flip_z"], false);
    assert_eq!(top["anchor"], "positive_end");

    // Deletes cascade: the connector takes its mate, the instance its connectors.
    let d = f.ok("connector_delete", json!({ "connector_id": cb_id }));
    assert_eq!(d["connectors"].as_array().unwrap().len(), 1);
    assert_eq!(d["mates"], json!([]));
    let d = f.ok("instance_delete", json!({ "instance_id": a_id }));
    assert_eq!(d["instances"].as_array().unwrap().len(), 1);
    assert_eq!(d["connectors"], json!([]));

    assert_eq!(
        f.refused("instance_delete", json!({ "instance_id": a_id }))["code"],
        "InstanceNotFound"
    );
    assert_eq!(
        f.refused("connector_delete", json!({ "connector_id": ca_id }))["code"],
        "ConnectorNotFound"
    );
    assert_eq!(
        f.refused("mate_delete", json!({ "mate_id": m_id }))["code"],
        "MateNotFound"
    );
    assert_eq!(
        f.refused(
            "instance_edit",
            json!({ "instance_id": "nope", "name": "x" })
        )["code"],
        "InstanceNotFound"
    );
    assert_eq!(
        f.refused("connector_edit", json!({ "connector_id": "nope" }))["code"],
        "ConnectorNotFound"
    );
    assert_eq!(
        f.refused("mate_edit", json!({ "mate_id": "nope" }))["code"],
        "MateNotFound"
    );

    // The state survives a round trip through a Part tab.
    f.ok("tab_switch", json!({ "tab_id": part }));
    f.ok("tab_switch", json!({ "tab_id": asm }));
    let g = f.ok("assembly_get", json!({}));
    assert_eq!(g["instances"].as_array().unwrap().len(), 1);
    assert_eq!(find(&g["instances"], &b_id)["name"], "Lid");
}

#[test]
fn instance_edit_takes_euler_or_quaternion_but_not_both() {
    let mut f = fixture();
    let part = f.part.clone();
    let a_id = id(
        &f.ok("instance_add", json!({ "tab_id": part })),
        "instance_id",
    );

    let err = f.refused(
        "instance_edit",
        json!({ "instance_id": a_id, "transform": { "rotation_quat": [0, 0, 0, 1], "rotation_euler_deg": [0, 0, 90] } }),
    );
    assert_eq!(err["code"], "InvalidArguments");
    assert_eq!(err["details"]["field"], "transform");
    let err = f.refused(
        "instance_edit",
        json!({ "instance_id": a_id, "transform": { "rotation_quat": [0, 0, 0, 0] } }),
    );
    assert_eq!(err["code"], "InvalidArguments");
    assert_eq!(err["details"]["field"], "transform.rotation_quat");

    let e = f.ok(
        "instance_edit",
        json!({ "instance_id": a_id, "name": "Turned", "suppressed": true, "transform": { "rotation_euler_deg": [0, 0, 90] } }),
    );
    let inst = find(&e["instances"], &a_id);
    assert_eq!(inst["name"], "Turned");
    assert_eq!(inst["suppressed"], true);
    let q: [f64; 4] = serde_json::from_value(inst["transform"]["rotation_quat"].clone()).unwrap();
    let h = std::f64::consts::FRAC_1_SQRT_2;
    assert!(
        (q[2] - h).abs() < 1e-12 && (q[3] - h).abs() < 1e-12,
        "{q:?}"
    );
    let e = f.ok(
        "instance_edit",
        json!({ "instance_id": a_id, "transform": { "rotation_quat": [0, 0, 0, 2], "translation_m": [1, 2, 3] } }),
    );
    let inst = find(&e["instances"], &a_id);
    assert_eq!(
        inst["transform"]["rotation_quat"],
        json!([0.0, 0.0, 0.0, 1.0]),
        "normalised"
    );
    assert_eq!(inst["transform"]["translation_m"], json!([1.0, 2.0, 3.0]));
}

#[test]
fn a_geom_ref_the_engine_cannot_resolve_is_refused_before_it_is_minted() {
    let mut f = fixture();
    let part = f.part.clone();
    let a_id = id(
        &f.ok("instance_add", json!({ "tab_id": part })),
        "instance_id",
    );
    let geom_ref = json!({
        "kind": { "type": "Face" },
        "anchor": { "type": "FeatureOutput", "feature_id": Uuid::new_v4(), "output_key": { "type": "Main" } },
        "selector": { "type": "Role", "role": { "type": "EndCapPositive" }, "index": 0 },
        "policy": { "type": "Strict" }
    });
    let err = f.refused(
        "connector_add",
        json!({ "instance_path": [a_id], "geom_ref": geom_ref }),
    );
    assert_eq!(err["code"], "ConnectorRefused", "{err}");
    assert!(err["message"]
        .as_str()
        .unwrap()
        .starts_with("Cannot put a connector here: "));
    assert!(err["details"]["reason"].is_string());
    let g = f.ok("assembly_get", json!({}));
    assert_eq!(g["connectors"], json!([]), "nothing was minted");
}

#[test]
fn every_assembly_tool_is_migrated_and_the_edits_carry_the_model() {
    for name in ASSEMBLY_TOOLS {
        assert!(MIGRATED.contains(name), "{name} is not listed as migrated");
        assert_eq!(mutates(name), *name != "assembly_get", "{name}");
    }
}
