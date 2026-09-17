//! The export pair in the engine (`specs/waffle_server_mode.md` §2.3 S3 C6):
//! `export_step` and `export_stl`.
//!
//! Agreement with the page's former JS implementation is held end to end by
//! `app/tests/gui/agent-export-import.spec.js`, which drives the REAL relay
//! and predates the port. These tests pin what that cannot reach: the wire
//! shape of a `deliver:"download"` answer — the file rides OUT OF BAND in
//! `download`, for the host, and is absent from every other answer — and the
//! refusals on a document with nothing to export.
//!
//! Real kernel throughout: an export needs tessellated meshes and a B-Rep,
//! which `MockKernel` never produces.

use feature_engine::types::*;
use serde_json::{json, Value};
use uuid::Uuid;
use waffle_types::*;
use wasm_bridge::messages::{EngineToUi, UiToEngine};
use wasm_bridge::*;

fn tool(
    state: &mut EngineState,
    kernel: &mut kernel_v2::KernelV2Adapter,
    name: &str,
    args: Value,
) -> ToolResult {
    execute_tool(state, kernel, name, &args, None)
}

fn point(id: u32, x: f64, y: f64) -> SketchEntity {
    SketchEntity::Point {
        id,
        x,
        y,
        construction: false,
    }
}

/// A 20 × 10 mm rectangle with real edges, extruded 5 mm: O1's box.
fn box_document() -> (EngineState, kernel_v2::KernelV2Adapter, String) {
    let mut state = EngineState::new();
    let mut kernel = kernel_v2::KernelV2Adapter::new();
    let corners = [
        (1, 0.0, 0.0),
        (2, 0.02, 0.0),
        (3, 0.02, 0.01),
        (4, 0.0, 0.01),
    ];
    let mut entities: Vec<SketchEntity> =
        corners.iter().map(|&(id, x, y)| point(id, x, y)).collect();
    for (id, (a, b)) in [(10, (1, 2)), (11, (2, 3)), (12, (3, 4)), (13, (4, 1))] {
        entities.push(SketchEntity::Line {
            id,
            start_id: a,
            end_id: b,
            construction: false,
        });
    }
    let sketch = Operation::Sketch {
        sketch: Sketch {
            id: Uuid::new_v4(),
            plane: GeomRef {
                kind: TopoKind::Face,
                anchor: Anchor::Datum {
                    datum_id: Uuid::new_v4(),
                },
                selector: Selector::Role {
                    role: Role::EndCapPositive,
                    index: 0,
                },
                policy: ResolvePolicy::BestEffort,
                scope: None,
            },
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities,
            constraints: Vec::new(),
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: corners.iter().map(|&(id, x, y)| (id, (x, y))).collect(),
            projected: Vec::new(),
            solved_profiles: vec![ClosedProfile {
                entity_ids: vec![10, 11, 12, 13],
                is_outer: true,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            }],
        },
    };
    let added = tool(
        &mut state,
        &mut kernel,
        "feature_add",
        json!({ "operation": serde_json::to_value(sketch).expect("a sketch operation") }),
    );
    assert!(!added.is_error, "{added:?}");
    let sketch_id = added.structured_content["feature_id"].clone();
    let solid = tool(
        &mut state,
        &mut kernel,
        "feature_add",
        json!({ "operation": {
            "type": "Extrude",
            "params": {
                "sketch_id": sketch_id,
                "profile_index": 0,
                "profile_entity_ids": [10, 11, 12, 13],
                "depth": 0.005,
                "symmetric": false,
                "cut": false,
            }
        } }),
    );
    assert!(!solid.is_error, "{solid:?}");
    let summary = tool(&mut state, &mut kernel, "model_summary", json!({}));
    let body_id = summary.structured_content["bodies"][0]["body_id"]
        .as_str()
        .expect("the box's body id")
        .to_string();
    (state, kernel, body_id)
}

fn resource(result: &ToolResult) -> Option<&Value> {
    result
        .content
        .iter()
        .find(|c| c["type"] == "resource")
        .map(|c| &c["resource"])
}

// ── Refusals ────────────────────────────────────────────────────────────

#[test]
fn an_empty_part_has_nothing_to_export() {
    let mut state = EngineState::new();
    let mut kernel = kernel_v2::KernelV2Adapter::new();
    for name in ["export_step", "export_stl"] {
        let result = tool(&mut state, &mut kernel, name, json!({}));
        assert!(result.is_error, "{name}: {result:?}");
        assert_eq!(
            result.structured_content["error"]["code"],
            "NothingToExport"
        );
        assert!(result.download.is_none());
    }
}

#[test]
fn export_stl_refuses_a_body_the_part_does_not_render() {
    let (mut state, mut kernel, _) = box_document();
    let result = tool(
        &mut state,
        &mut kernel,
        "export_stl",
        json!({ "body_id": "no-such-body" }),
    );
    assert!(result.is_error);
    assert_eq!(result.structured_content["error"]["code"], "BodyNotFound");
    assert_eq!(
        result.structured_content["error"]["details"]["body_id"],
        "no-such-body"
    );
}

#[test]
fn a_deliver_that_is_neither_agent_nor_download_is_refused() {
    let (mut state, mut kernel, _) = box_document();
    let result = tool(
        &mut state,
        &mut kernel,
        "export_step",
        json!({ "deliver": "email" }),
    );
    assert!(result.is_error);
    assert_eq!(
        result.structured_content["error"]["code"],
        "InvalidArgument"
    );
    assert_eq!(
        result.structured_content["error"]["details"]["path"],
        "/deliver"
    );
}

// ── deliver:"agent" ────────────────────────────────────────────────────

#[test]
fn export_step_to_the_agent_embeds_the_step_text() {
    let (mut state, mut kernel, _) = box_document();
    let result = tool(&mut state, &mut kernel, "export_step", json!({}));
    assert!(!result.is_error, "{result:?}");
    let meta = &result.structured_content;
    assert_eq!(meta["deliver"], "agent");
    assert_eq!(meta["mime_type"], "model/step");
    assert_eq!(meta["file_name"], "Untitled.step");
    assert_eq!(meta["warnings"], json!([]));

    let res = resource(&result).expect("an embedded resource");
    assert_eq!(res["mimeType"], "model/step");
    assert_eq!(res["uri"], "waffle://export/Untitled.step");
    let text = res["text"].as_str().expect("STEP text");
    assert!(text.starts_with("ISO-10303-21;"));
    assert!(res.get("blob").is_none());
    assert_eq!(meta["bytes"], text.len());
    // Nothing for the host to deliver: the agent has the file.
    assert!(result.download.is_none());
}

#[test]
fn export_stl_to_the_agent_embeds_the_binary_stl_for_one_body_or_all() {
    let (mut state, mut kernel, body_id) = box_document();
    for (args, file_name) in [
        (json!({ "body_id": body_id }), "Extrude.stl"),
        (json!({}), "Untitled.stl"),
    ] {
        let result = tool(&mut state, &mut kernel, "export_stl", args);
        assert!(!result.is_error, "{result:?}");
        let meta = &result.structured_content;
        assert_eq!(meta["deliver"], "agent");
        assert_eq!(meta["mime_type"], "model/stl");
        assert_eq!(meta["file_name"], file_name);
        let res = resource(&result).expect("an embedded resource");
        assert_eq!(res["mimeType"], "model/stl");
        let blob = res["blob"].as_str().expect("base64 STL");
        assert!(res.get("text").is_none());
        let bytes = base64_decode(blob);
        // Binary STL: 80-byte header, u32 triangle count, 50 bytes each.
        let triangles = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        assert_eq!(triangles, 12, "a box is twelve triangles");
        assert_eq!(bytes.len(), 84 + 50 * triangles);
        assert_eq!(meta["bytes"], bytes.len());
        assert!(result.download.is_none());
    }
}

// ── deliver:"download" ─────────────────────────────────────────────────

#[test]
fn a_download_hands_the_file_to_the_host_and_embeds_nothing() {
    let (mut state, mut kernel, _) = box_document();

    let step = tool(
        &mut state,
        &mut kernel,
        "export_step",
        json!({ "deliver": "download" }),
    );
    assert!(!step.is_error, "{step:?}");
    assert_eq!(step.structured_content["deliver"], "download");
    assert!(
        resource(&step).is_none(),
        "the agent's answer only describes the file"
    );
    let file = step.download.as_ref().expect("the file, for the host");
    assert_eq!(file.file_name, "Untitled.step");
    assert_eq!(file.mime_type, "model/step");
    assert!(file.text.as_deref().unwrap().starts_with("ISO-10303-21;"));
    assert!(file.blob.is_none());
    assert_eq!(
        step.structured_content["bytes"],
        file.text.as_ref().unwrap().len()
    );

    let stl = tool(
        &mut state,
        &mut kernel,
        "export_stl",
        json!({ "deliver": "download" }),
    );
    assert!(!stl.is_error, "{stl:?}");
    assert!(resource(&stl).is_none());
    let file = stl.download.as_ref().expect("the file, for the host");
    assert_eq!(file.file_name, "Untitled.stl");
    assert_eq!(file.mime_type, "model/stl");
    assert!(file.text.is_none());
    assert_eq!(
        stl.structured_content["bytes"],
        base64_decode(file.blob.as_deref().unwrap()).len()
    );
}

#[test]
fn on_the_wire_download_rides_beside_the_mcp_fields_and_only_for_a_download() {
    // The page's `toolAnswer` picks `content`/`structuredContent`/`isError`
    // for the relay and `download` for itself; a host parses the same frame.
    let (mut state, mut kernel, _) = box_document();
    let answer = |state: &mut EngineState, kernel: &mut kernel_v2::KernelV2Adapter, args| {
        let msg = UiToEngine::Tool {
            name: "export_step".to_string(),
            arguments: args,
            context: None,
        };
        let response = dispatch(state, msg, kernel);
        assert!(matches!(response, EngineToUi::ToolResult { .. }));
        serde_json::to_value(&response).expect("serializable")
    };

    let download = answer(&mut state, &mut kernel, json!({ "deliver": "download" }));
    assert_eq!(download["type"], "ToolResult");
    assert_eq!(download["download"]["file_name"], "Untitled.step");
    assert_eq!(download["download"]["mime_type"], "model/step");
    assert!(download["download"]["text"].is_string());
    assert!(
        download.get("model").is_none(),
        "a read-only tool carries no model"
    );
    let parsed: EngineToUi = serde_json::from_value(download).expect("round-trips");
    let EngineToUi::ToolResult { result, .. } = parsed else {
        unreachable!()
    };
    assert!(result.download.is_some());

    let inline = answer(&mut state, &mut kernel, json!({}));
    assert!(
        inline.get("download").is_none(),
        "an inline answer has nothing out of band: {inline}"
    );
}

/// Standard padded base64, decoded by hand so the test needs no crate.
fn base64_decode(s: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let value = |c: u8| ALPHABET.iter().position(|&a| a == c).expect("base64 char") as u32;
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    for chunk in s.as_bytes().chunks(4) {
        let pad = chunk.iter().filter(|&&c| c == b'=').count();
        let mut acc = 0u32;
        for &c in chunk {
            acc = (acc << 6) | if c == b'=' { 0 } else { value(c) };
        }
        let bytes = acc.to_be_bytes();
        out.extend_from_slice(&bytes[1..4 - pad]);
    }
    out
}
