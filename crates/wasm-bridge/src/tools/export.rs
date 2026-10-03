//! The export tools, `export_step` and `export_stl`
//! (`specs/waffle_mcp_server.md` §2.5 Export, Q5–Q7), ported from
//! `app/src/lib/agent/export.js` (S3 C6), plus `export_dxf`
//! (`specs/drawings_and_mbd.md` §12, 2026-10-03), which is new here rather
//! than ported: the page never had a drawing export to port.
//!
//! All three are queries — they change nothing — and each wraps one engine
//! message (`ExportStep`; `ExportStl` or `ExportBodyStl`; `ExportDxf`). What
//! moved here is everything around that message: the `NothingToExport` /
//! `BodyNotFound` gates, the file name, the byte count, the Q6 payload cap and
//! the result's shape, including the embedded resource for `deliver:"agent"`.
//! `export_dxf` adds the view vocabulary — the six named orthographic views,
//! or an explicit direction — because naming a view is the agent's and the
//! document's concern, not the kernel's, which takes a direction.
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
use uuid::Uuid;
use waffle_types::kernel::ViewFrame;

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

/// The active tab's kind (`"Part"`, `"Assembly"`, `"Drawing"`, …), which is
/// what decides whether `export_dxf` draws the model or a sheet.
fn active_kind(state: &EngineState) -> String {
    let active = state.session.active_tab_id().to_string();
    state
        .session
        .tabs()
        .into_iter()
        .find(|t| t.id == active)
        .map(|t| t.kind)
        .unwrap_or_else(|| "Part".to_string())
}

/// An optional UUID argument, refused by name when it is not one.
fn uuid_arg(args: &Value, name: &str) -> Result<Option<Uuid>, ToolFailure> {
    let Some(value) = args.get(name).filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let text = value.as_str().unwrap_or_default();
    Uuid::parse_str(text).map(Some).map_err(|_| {
        ToolFailure::new(
            "InvalidArgument",
            format!("{name} must be a UUID, not `{text}`."),
            json!({ "path": format!("/{name}") }),
        )
    })
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

/// The six named orthographic views (`specs/drawings_and_mbd.md` §8
/// `Projection::Named`), as `(direction of sight, paper up)`.
///
/// Naming a view is a DOCUMENT concern, not a kernel one — the kernel takes a
/// direction — so the table lives here, where the agent's vocabulary is. Each
/// is third-angle conventional: `u = dir × up`, so the top view reads `+x`
/// right / `+y` up, the front `+x` right / `+z` up, the back mirrors `x`.
const NAMED_VIEWS: &[(&str, [f64; 3], [f64; 3])] = &[
    ("top", [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
    ("bottom", [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]),
    ("front", [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
    ("back", [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]),
    ("right", [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
    ("left", [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
];

/// `(direction of sight, paper up)` as the `ExportDxf` message carries them:
/// `None` leaves the choice to the engine.
type ViewArgs = (Option<[f64; 3]>, Option<[f64; 3]>);

/// The `view` / `direction` / `up` arguments as `(dir, up)`. `view` defaults
/// to `"top"` — the flat-pattern view §12's deliverable is for. `direction` is
/// the escape hatch for an axis nothing names; giving both is a refusal, not a
/// silent precedence rule.
fn view_arguments(args: &Value) -> Result<ViewArgs, ToolFailure> {
    let named = args.get("view").filter(|v| !v.is_null());
    let direction = args.get("direction").filter(|v| !v.is_null());
    if named.is_some() && direction.is_some() {
        return Err(ToolFailure::new(
            "InvalidArgument",
            "Give either view or direction, not both.",
            json!({ "path": "/direction" }),
        ));
    }
    let up = match args.get("up").filter(|v| !v.is_null()) {
        None => None,
        Some(v) => Some(vector3(v, "/up")?),
    };
    if let Some(v) = direction {
        return check_orientable((Some(vector3(v, "/direction")?), up));
    }
    let name = match named {
        None => "top",
        Some(Value::String(s)) => s.as_str(),
        Some(other) => {
            return Err(ToolFailure::new(
                "InvalidArgument",
                format!("view must be one of the named views, not {other}."),
                json!({ "path": "/view" }),
            ))
        }
    };
    let Some((_, dir, default_up)) = NAMED_VIEWS
        .iter()
        .find(|(n, _, _)| n.eq_ignore_ascii_case(name))
    else {
        return Err(ToolFailure::new(
            "InvalidArgument",
            format!(
                "`{name}` is not a named view; use one of {}.",
                NAMED_VIEWS
                    .iter()
                    .map(|(n, _, _)| *n)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            json!({ "path": "/view" }),
        ));
    };
    check_orientable((Some(*dir), Some(up.unwrap_or(*default_up))))
}

/// `up` must not be parallel to the line of sight, or the view plane has no
/// orientation.
///
/// The kernel refuses that pair too, but as a projection failure the bridge
/// can only report as a REBUILD error — a feature named "DXF export" that
/// does not exist, for a tool that rebuilds nothing. A bad argument is an
/// `InvalidArgument` naming the argument, so the check belongs here, asked of
/// the very frame the bridge will build ([`ViewFrame::from_parts`]) rather
/// than of a second opinion about what "parallel" means.
/// It names the argument actually at fault: a direction too short to
/// normalize at all is `/direction`, not `/up`. `vector3` refuses an exactly
/// zero vector, but `[1e-300, 0, 0]` is three finite non-zero numbers and
/// still has no unit direction in it.
fn check_orientable((dir, up): ViewArgs) -> Result<ViewArgs, ToolFailure> {
    if ViewFrame::from_parts(dir, None).basis().is_none() {
        return Err(ToolFailure::new(
            "InvalidArgument",
            "/direction is too short to be a direction of sight.",
            json!({ "path": "/direction" }),
        ));
    }
    if ViewFrame::from_parts(dir, up).basis().is_none() {
        return Err(ToolFailure::new(
            "InvalidArgument",
            "up must not be parallel to the direction of sight; the view plane \
             has no orientation without a perpendicular component.",
            json!({ "path": "/up" }),
        ));
    }
    Ok((dir, up))
}

/// A `[x, y, z]` argument of three finite numbers, not all zero.
fn vector3(v: &Value, path: &str) -> Result<[f64; 3], ToolFailure> {
    let bad = |why: &str| {
        ToolFailure::new(
            "InvalidArgument",
            format!("{path} must be {why}."),
            json!({ "path": path }),
        )
    };
    let arr = v
        .as_array()
        .ok_or_else(|| bad("an array of three numbers"))?;
    if arr.len() != 3 {
        return Err(bad("an array of three numbers"));
    }
    let mut out = [0.0; 3];
    for (slot, value) in out.iter_mut().zip(arr) {
        let n = value.as_f64().ok_or_else(|| bad("three numbers"))?;
        if !n.is_finite() {
            return Err(bad("three finite numbers"));
        }
        *slot = n;
    }
    if out == [0.0; 3] {
        return Err(bad("a non-zero direction"));
    }
    Ok(out)
}

/// An R12 DXF drawing: one orthographic view of the whole model
/// (`specs/drawings_and_mbd.md` §12's early deliverable), or — on a `Drawing`
/// tab — the sheet, or one view of it (D4a).
pub(super) fn export_dxf(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Result<ToolResult, ToolFailure> {
    let deliver = deliver(args)?;
    // On a Drawing tab the document decides the projections, so the
    // direction arguments are not read at all — and `view_arguments` would
    // default them to the top view, which the dispatch then refuses as
    // meaningless here. The sheet arguments are parsed instead.
    let drawing_tab = active_kind(state) == "Drawing";
    let (view_dir, up) = if drawing_tab {
        for named in ["view", "direction", "up"] {
            if args.get(named).is_some_and(|v| !v.is_null()) {
                return Err(ToolFailure::new(
                    "InvalidArgument",
                    format!(
                        "`{named}` describes a view of the model; a Drawing tab's views carry \
                         their own projections. Use sheet_id / view_id, or tab_switch to the \
                         Part tab for a flat pattern."
                    ),
                    json!({ "path": format!("/{named}") }),
                ));
            }
        }
        (None, None)
    } else {
        view_arguments(args)?
    };
    let (sheet_id, view_id) = if drawing_tab {
        (uuid_arg(args, "sheet_id")?, uuid_arg(args, "view_id")?)
    } else {
        for named in ["sheet_id", "view_id"] {
            if args.get(named).is_some_and(|v| !v.is_null()) {
                return Err(ToolFailure::new(
                    "TabKindNotSupported",
                    format!("`{named}` names a view of a Drawing tab; the open tab is not one."),
                    json!({ "kind": active_kind(state) }),
                ));
            }
        }
        // A Part or Assembly tab with no bodies has nothing to project. A
        // Drawing tab is checked per view instead, by the rebuild: an empty
        // view of a tab with nothing built is a blank sheet, not a refusal.
        require_bodies(state)?;
        (None, None)
    };
    let response = engine_call(
        state,
        kb,
        "ExportDxf",
        UiToEngine::ExportDxf {
            view_dir,
            up,
            sheet_id,
            view_id,
        },
    )?;
    let EngineToUi::DxfExportReady { dxf_data, warnings } = response else {
        return Err(unexpected("ExportDxf", "DxfExportReady", &response));
    };
    let file_name = format!("{}.dxf", safe_name(state.project_name()));
    let bytes = dxf_data.len();
    check_payload(deliver, &file_name, bytes)?;
    Ok(export_result(
        deliver,
        ExportFile {
            file_name,
            // The registered type for DXF (RFC 9287 / IANA `image/vnd.dxf`).
            mime_type: "image/vnd.dxf".to_string(),
            text: Some(dxf_data),
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
    let body_id = match &body_id {
        Some(id) => Some(require_body(state, id)?),
        None => None,
    };
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

    /// `ToolFailure` is not `Debug` (it is an MCP payload, not a Rust error),
    /// so the tests unwrap through its message.
    fn views(args: Value) -> ViewArgs {
        match view_arguments(&args) {
            Ok(v) => v,
            Err(e) => panic!("{args} refused: {}", e.message),
        }
    }

    #[test]
    fn the_view_defaults_to_top_and_names_the_rest() {
        assert_eq!(
            views(json!({})),
            (Some([0.0, 0.0, -1.0]), Some([0.0, 1.0, 0.0]))
        );
        assert_eq!(
            views(json!({ "view": "FRONT" })),
            (Some([0.0, 1.0, 0.0]), Some([0.0, 0.0, 1.0])),
            "a named view is case-insensitive"
        );
        // An explicit up overrides the named view's default.
        assert_eq!(
            views(json!({ "view": "top", "up": [1.0, 0.0, 0.0] })),
            (Some([0.0, 0.0, -1.0]), Some([1.0, 0.0, 0.0]))
        );
        assert_eq!(
            views(json!({ "direction": [1.0, 2.0, 3.0] })),
            (Some([1.0, 2.0, 3.0]), None),
            "a free direction lets the kernel choose an up"
        );
    }

    #[test]
    fn every_named_view_is_a_distinct_unit_axis_pair() {
        for (name, dir, up) in NAMED_VIEWS {
            let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
            assert_eq!(len, 1.0, "{name} dir is not a unit axis");
            let d = dir[0] * up[0] + dir[1] * up[1] + dir[2] * up[2];
            assert_eq!(d, 0.0, "{name} up is not perpendicular to its direction");
        }
        // Pairwise, not `dedup` — which only removes CONSECUTIVE repeats and
        // would miss `top` reappearing as `left`.
        for (i, (a, da, _)) in NAMED_VIEWS.iter().enumerate() {
            for (b, db, _) in &NAMED_VIEWS[i + 1..] {
                assert_ne!(da, db, "{a} and {b} look the same way");
            }
        }
        assert_eq!(
            NAMED_VIEWS.len(),
            6,
            "the six views look six different ways"
        );
    }

    #[test]
    fn a_bad_view_argument_is_refused_rather_than_guessed() {
        for args in [
            json!({ "view": "isometric" }),
            json!({ "view": 3 }),
            json!({ "view": "top", "direction": [0.0, 0.0, 1.0] }),
            json!({ "direction": [0.0, 0.0, 0.0] }),
            json!({ "direction": [1.0, 2.0] }),
            json!({ "direction": "x" }),
            json!({ "up": [0.0, 0.0, 0.0] }),
            // An `up` along the line of sight leaves the view plane without
            // an orientation. The kernel refuses it too, but only as a
            // projection failure the bridge reports as a rebuild error.
            json!({ "view": "front", "up": [0.0, -2.0, 0.0] }),
            json!({ "direction": [0.0, 0.0, 1.0], "up": [0.0, 0.0, 5.0] }),
            // Three finite non-zero numbers, and still no direction in it.
            json!({ "direction": [1e-300, 0.0, 0.0] }),
        ] {
            match view_arguments(&args) {
                Ok(v) => panic!("{args} should be refused, got {v:?}"),
                Err(e) => assert_eq!(e.code, "InvalidArgument", "{args}: {}", e.message),
            }
        }
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
