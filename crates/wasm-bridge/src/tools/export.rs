//! The export pair, `export_step` and `export_stl`
//! (`specs/waffle_mcp_server.md` §2.5 Export, Q5–Q7), ported from
//! `app/src/lib/agent/export.js` (S3 C6).
//!
//! Both are queries — they change nothing — and each wraps one engine
//! message (`ExportStep`; `ExportStl` or `ExportBodyStl`). What moved here is
//! everything around that message: the `NothingToExport` / `BodyNotFound`
//! gates, the file name, the byte count, the Q6 payload cap and the result's
//! shape, including the embedded resource for `deliver:"agent"`.
//!
//! What did NOT move is handing the file to the user for `deliver:"download"`:
//! that is a host concern by `specs/waffle_server_mode.md` §3.3 (the browser
//! downloads it; a native host writes it into its exports directory). The
//! engine cannot do it and must not pretend to, so the answer for a download
//! carries the file OUT OF BAND — [`ToolResult::download`], never in the MCP
//! `content` — and the host delivers it. A host that ignores that field has
//! silently dropped the user's file; the MCP result still says
//! `deliver:"download"`, so nothing downstream would notice. Every host must
//! honour it.

use modeling_ops::KernelBundle;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::engine_state::EngineState;
use crate::messages::{EngineToUi, UiToEngine};
use crate::tools::{
    engine_call, rendered_bodies, require_body, unexpected, ToolFailure, ToolResult,
};

/// Q6: the largest file returned inline to the agent (16 MiB).
pub const MAX_AGENT_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;

/// An exported file the host must deliver to the user
/// (`deliver:"download"`). Exactly one of `text` and `blob` is set, as in an
/// MCP embedded resource: STEP is text, STL is base64 of the binary file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportFile {
    /// The name to save it under, extension included.
    pub file_name: String,
    pub mime_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Standard padded base64 of the file's bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

/// JS `safeName`: anything that is not `[A-Za-z0-9_.-]` collapses to one `_`;
/// an empty name is `model`.
pub(crate) fn safe_name(name: &str) -> String {
    if name.is_empty() {
        return "model".to_string();
    }
    let mut out = String::with_capacity(name.len());
    let mut in_run = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-' {
            out.push(c);
            in_run = false;
        } else if !in_run {
            out.push('_');
            in_run = true;
        }
    }
    out
}

/// Decoded size of standard padded base64 (JS `base64Bytes`).
fn base64_bytes(b64: &str) -> usize {
    let pad = if b64.ends_with("==") {
        2
    } else if b64.ends_with('=') {
        1
    } else {
        0
    };
    (b64.len() * 3) / 4 - pad
}

/// The `deliver` argument, defaulting to `"agent"`.
fn deliver(args: &Value) -> Result<&str, ToolFailure> {
    match args.get("deliver") {
        None | Some(Value::Null) => Ok("agent"),
        Some(Value::String(s)) if s == "agent" || s == "download" => Ok(s),
        Some(other) => Err(ToolFailure::new(
            "InvalidArgument",
            format!("deliver must be \"agent\" or \"download\", not {other}."),
            json!({ "path": "/deliver" }),
        )),
    }
}

/// Q5: the open Part has bodies to export.
fn require_bodies(state: &EngineState) -> Result<(), ToolFailure> {
    if rendered_bodies(state).is_empty() {
        return Err(ToolFailure::new(
            "NothingToExport",
            "The open Part has no bodies to export.",
            json!({}),
        ));
    }
    Ok(())
}

/// Q6: an inline result may not exceed the cap; a download may be any size.
fn check_payload(deliver: &str, file_name: &str, bytes: usize) -> Result<(), ToolFailure> {
    if deliver != "agent" || bytes <= MAX_AGENT_PAYLOAD_BYTES {
        return Ok(());
    }
    Err(ToolFailure::new(
        "PayloadTooLarge",
        format!(
            "{file_name} is {bytes} bytes, over the {MAX_AGENT_PAYLOAD_BYTES}-byte limit for a result. \
             Use deliver: \"download\"."
        ),
        json!({ "bytes": bytes }),
    ))
}

/// The result both tools share (JS `exportResult`): the description in
/// `structuredContent`; for `deliver:"agent"` the file embedded as an MCP
/// resource, for `deliver:"download"` the file handed to the host instead.
fn export_result(
    deliver: &str,
    file: ExportFile,
    bytes: usize,
    warnings: Vec<String>,
) -> ToolResult {
    let structured = json!({
        "deliver": deliver,
        "file_name": file.file_name,
        "mime_type": file.mime_type,
        "bytes": bytes,
        "warnings": warnings,
    });
    let mut result = ToolResult::ok(structured);
    if deliver == "agent" {
        let mut resource = json!({
            "uri": format!("waffle://export/{}", encode_uri_component(&file.file_name)),
            "mimeType": file.mime_type,
        });
        if let Some(text) = file.text {
            resource["text"] = json!(text);
        }
        if let Some(blob) = file.blob {
            resource["blob"] = json!(blob);
        }
        result
            .content
            .push(json!({ "type": "resource", "resource": resource }));
    } else {
        result.download = Some(file);
    }
    result
}

/// JS `encodeURIComponent`: percent-encode every byte outside its unreserved
/// set `A–Z a–z 0–9 - _ . ! ~ * ' ( )`.
fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The whole model as analytic STEP AP214 (Q7).
pub(super) fn export_step(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Result<ToolResult, ToolFailure> {
    let deliver = deliver(args)?;
    require_bodies(state)?;
    let response = engine_call(state, kb, "ExportStep", UiToEngine::ExportStep)?;
    let EngineToUi::ExportReady {
        step_data,
        warnings,
    } = response
    else {
        return Err(unexpected("ExportStep", "ExportReady", &response));
    };
    let file_name = format!("{}.step", safe_name(state.project_name()));
    // UTF-8 length, as the page's `TextEncoder` measured it.
    let bytes = step_data.len();
    check_payload(deliver, &file_name, bytes)?;
    Ok(export_result(
        deliver,
        ExportFile {
            file_name,
            mime_type: "model/step".to_string(),
            text: Some(step_data),
            blob: None,
        },
        bytes,
        warnings,
    ))
}

/// One body, or every body merged, as binary STL.
pub(super) fn export_stl(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Result<ToolResult, ToolFailure> {
    let deliver = deliver(args)?;
    require_bodies(state)?;
    let body_id = args
        .get("body_id")
        .filter(|v| !v.is_null())
        .map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        });
    if let Some(id) = &body_id {
        require_body(state, id)?;
    }
    let (tag, message) = match &body_id {
        Some(id) => (
            "ExportBodyStl",
            UiToEngine::ExportBodyStl {
                body_id: id.clone(),
            },
        ),
        None => ("ExportStl", UiToEngine::ExportStl),
    };
    let response = engine_call(state, kb, tag, message)?;
    let EngineToUi::StlExportReady { stl_data } = response else {
        return Err(unexpected(tag, "StlExportReady", &response));
    };
    let base = match &body_id {
        Some(id) => rendered_bodies(state)
            .iter()
            .find(|b| b.get("bodyId") == Some(&json!(id)))
            .and_then(|b| b.get("name").and_then(Value::as_str))
            .map(safe_name)
            .unwrap_or_else(|| safe_name("")),
        None => safe_name(state.project_name()),
    };
    let file_name = format!("{base}.stl");
    let bytes = base64_bytes(&stl_data);
    check_payload(deliver, &file_name, bytes)?;
    Ok(export_result(
        deliver,
        ExportFile {
            file_name,
            mime_type: "model/stl".to_string(),
            text: None,
            blob: Some(stl_data),
        },
        bytes,
        Vec::new(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_name_matches_the_page() {
        assert_eq!(safe_name(""), "model");
        assert_eq!(safe_name("Bike frame"), "Bike_frame");
        assert_eq!(safe_name("a  b/c:d"), "a_b_c_d");
        assert_eq!(safe_name("ok-name.v2_x"), "ok-name.v2_x");
        assert_eq!(safe_name("ünïcode"), "_n_code");
    }

    #[test]
    fn base64_bytes_counts_the_decoded_size() {
        assert_eq!(base64_bytes(""), 0);
        assert_eq!(base64_bytes("YQ=="), 1);
        assert_eq!(base64_bytes("YWI="), 2);
        assert_eq!(base64_bytes("YWJj"), 3);
        assert_eq!(base64_bytes("YWJjZA=="), 4);
    }

    #[test]
    fn encode_uri_component_matches_the_page() {
        assert_eq!(encode_uri_component("Bike_frame.step"), "Bike_frame.step");
        assert_eq!(encode_uri_component("a b/c"), "a%20b%2Fc");
        assert_eq!(encode_uri_component("é"), "%C3%A9");
    }

    #[test]
    fn the_payload_cap_applies_only_to_inline_results() {
        assert!(check_payload("agent", "x.step", MAX_AGENT_PAYLOAD_BYTES).is_ok());
        let err = check_payload("agent", "x.step", MAX_AGENT_PAYLOAD_BYTES + 1).unwrap_err();
        assert_eq!(err.code, "PayloadTooLarge");
        assert_eq!(err.details["bytes"], MAX_AGENT_PAYLOAD_BYTES + 1);
        assert!(check_payload("download", "x.step", MAX_AGENT_PAYLOAD_BYTES + 1).is_ok());
    }
}
