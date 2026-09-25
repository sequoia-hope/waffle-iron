//! Agent tool semantics in Rust (`specs/waffle_server_mode.md` §2.3 S3).
//!
//! One implementation of every non-render agent tool, reachable from any host:
//! the browser page sends `UiToEngine::Tool`, and a native host will call
//! [`execute_tool`] directly. The page keeps only what §3.3 assigns to it —
//! the engine lock, the `UserBusy` / `AgentPaused` gates, the viewport and the
//! storage providers — and the *semantics* (what a tool reads, what it
//! refuses, how its result is shaped) live here.
//!
//! Tools migrated one at a time (C1–C6). A read-only tool was shadowed until
//! its differential was green — the page ran both implementations and
//! compared `structuredContent` — and then its JS body was deleted; an
//! authoring tool was cut over against recorded goldens instead, since a step
//! that changes the document cannot run twice; the export pair (C6) against
//! the end-to-end relay spec that predates the port. Nothing shadows any
//! more: the page routes every name in [`MIGRATED`] here and has no JS body
//! for it (`app/src/lib/agent/executor.js` `ENGINE_QUERIES` /
//! `ENGINE_COMMANDS`). A name not listed answers `ToolUnavailable`, exactly
//! as an unknown one does — a host must never silently do nothing.

use modeling_ops::KernelBundle;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::engine_state::EngineState;
use crate::messages::{EngineToUi, UiToEngine};

mod assembly;
mod author;
mod export;
mod inspect;
mod script;
mod sketch;
mod sketch3d;
mod summary;
mod tabs;

pub use assembly::ASSEMBLY_TOOLS;
pub use export::{ExportFile, MAX_AGENT_PAYLOAD_BYTES};
pub use tabs::{document_core, TAB_TOOLS};

/// The tools [`execute_tool`] implements — every non-render agent tool. The
/// page routes exactly these to `Tool` and implements none of them; what it
/// keeps (`selection_get`, the viewport, the storage tools, and DELIVERING an
/// export the answer hands it in [`ToolResult::download`]) is host state by
/// §3.3. Keep it in sync with the `match` in [`execute_tool`].
pub const MIGRATED: &[&str] = &[
    "model_summary",
    "feature_get",
    "body_measure",
    "face_list",
    "sketch_regions",
    "expression_evaluate",
    "export_step",
    "export_stl",
    "feature_add",
    "feature_edit",
    "feature_delete",
    "feature_suppress",
    "feature_reorder",
    "feature_rename",
    "body_rename",
    "rollback_set",
    "parameters_set",
    "import_step",
    "undo",
    "redo",
    "sketch_create",
    "sketch3d_get",
    "script_run_check",
    "script_source_add",
    "script_source_get",
    "script_source_update",
    "script_feature_add",
    "tab_switch",
    "tab_add",
    "tab_move",
    "tab_rename",
    "assembly_get",
    "instance_add",
    "instance_edit",
    "instance_delete",
    "connector_add",
    "connector_edit",
    "connector_delete",
    "mate_add",
    "mate_edit",
    "mate_delete",
];

/// Whether this tool can change the document.
///
/// The mutating tools are C4's twelve plus C5's `sketch_create`, the A-M4
/// script tools that change the document (a source add changes the `sources`
/// table the host mirrors, so it carries the update too), and since
/// 2026-09-23 the tab tools and the assembly edits (a tab switch changes what
/// is on screen; an assembly edit re-solves the tab): their answers carry a
/// model update (`EngineToUi::ToolResult::model`), because a `ToolResult` is
/// not a `ModelUpdated` and nothing else would refresh the host's view. A
/// tool that is not listed here is read-only and answers with no model.
pub fn mutates(name: &str) -> bool {
    matches!(
        name,
        "feature_add"
            | "feature_edit"
            | "feature_delete"
            | "feature_suppress"
            | "feature_reorder"
            | "feature_rename"
            | "body_rename"
            | "rollback_set"
            | "parameters_set"
            | "import_step"
            | "undo"
            | "redo"
            | "sketch_create"
            | "script_source_add"
            | "script_source_update"
            | "script_feature_add"
            | "tab_switch"
            | "tab_add"
            | "tab_move"
            | "tab_rename"
            | "instance_add"
            | "instance_edit"
            | "instance_delete"
            | "connector_add"
            | "connector_edit"
            | "connector_delete"
            | "mate_add"
            | "mate_edit"
            | "mate_delete"
    )
}

/// An MCP tool result (`specs/waffle_mcp_server.md` §2.3 `result`, I10).
///
/// Field names are the MCP wire names, so the page can hand a result to the
/// relay unchanged — all but `download`, which is for the HOST, not the
/// agent: it must be stripped before the result goes onto the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: Vec<Value>,
    #[serde(rename = "structuredContent")]
    pub structured_content: Value,
    #[serde(rename = "isError")]
    pub is_error: bool,
    /// A file the host must deliver to the user — an export the agent asked
    /// for with `deliver:"download"` (S3 C6, §3.3). Not part of the MCP
    /// result: the agent's answer only describes the file, and this is the
    /// file. A host that does not act on it has dropped the user's download
    /// while the answer says it happened. Absent from every other answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download: Option<ExportFile>,
}

impl ToolResult {
    /// A successful result carrying `structured` (JS `toolOk`).
    pub fn ok(structured: Value) -> Self {
        Self {
            content: vec![json!({ "type": "text", "text": structured.to_string() })],
            structured_content: structured,
            is_error: false,
            download: None,
        }
    }

    /// A refusal or failure (JS `toolError`): `isError` with the closed-set
    /// code of spec §6.1. Never a bare string — nothing silent (I10).
    pub fn error(code: &str, message: &str, details: Value) -> Self {
        Self {
            content: vec![json!({ "type": "text", "text": format!("{code}: {message}") })],
            structured_content: json!({
                "error": { "code": code, "message": message, "details": details }
            }),
            is_error: true,
            download: None,
        }
    }
}

/// A refusal on the way to a [`ToolResult`] (JS `ToolFailure`).
pub(crate) struct ToolFailure {
    code: &'static str,
    message: String,
    details: Value,
}

impl ToolFailure {
    pub(crate) fn new(code: &'static str, message: impl Into<String>, details: Value) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }
}

/// What a tool body returns: its structured content, or the refusal.
pub(crate) type Answer = Result<Value, ToolFailure>;

/// Run one agent tool against the open document.
///
/// `context` carries the per-call state a host holds (the agent's name, for
/// provenance); the authoring tools read it, the read-only ones do not.
pub fn execute_tool(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    name: &str,
    arguments: &Value,
    context: Option<&Value>,
) -> ToolResult {
    // The export pair shapes its own result: an embedded resource in
    // `content`, or the file for the host in `download`, neither of which
    // `structuredContent` alone can carry.
    let outcome = match name {
        "export_step" => export::export_step(state, kb, arguments),
        "export_stl" => export::export_stl(state, kb, arguments),
        _ => run(state, kb, name, arguments, context).map(ToolResult::ok),
    };
    match outcome {
        Ok(result) => result,
        Err(failure) => ToolResult::error(failure.code, &failure.message, failure.details),
    }
}

fn run(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    name: &str,
    args: &Value,
    context: Option<&Value>,
) -> Answer {
    match name {
        "model_summary" => summary::model_summary(state),
        "feature_get" => inspect::feature_get(state, args),
        "body_measure" => inspect::body_measure(state, kb, args),
        "face_list" => inspect::face_list(state, kb, args),
        "sketch_regions" => inspect::sketch_regions(state, kb, args),
        "expression_evaluate" => inspect::expression_evaluate(state, kb, args),
        "feature_add" => author::feature_add(state, kb, args, context),
        "feature_edit" => author::feature_edit(state, kb, args, context),
        "feature_delete" => author::feature_delete(state, kb, args),
        "feature_suppress" => author::feature_suppress(state, kb, args),
        "feature_reorder" => author::feature_reorder(state, kb, args),
        "feature_rename" => author::feature_rename(state, kb, args),
        "body_rename" => author::body_rename(state, kb, args),
        "rollback_set" => author::rollback_set(state, kb, args),
        "parameters_set" => author::parameters_set(state, kb, args),
        "import_step" => author::import_step(state, kb, args),
        "undo" => author::undo(state, kb),
        "redo" => author::redo(state, kb),
        "sketch_create" => sketch::sketch_create(state, kb, args, context),
        "sketch3d_get" => sketch3d::sketch3d_get(state, args),
        "script_run_check" => script::script_run_check(state, args),
        "script_source_add" => script::script_source_add(state, kb, args),
        "script_source_get" => script::script_source_get(state, args),
        "script_source_update" => script::script_source_update(state, kb, args),
        "script_feature_add" => script::script_feature_add(state, kb, args, context),
        "tab_switch" => tabs::tab_switch(state, kb, args),
        "tab_add" => tabs::tab_add(state, kb, args),
        "tab_move" => tabs::tab_move(state, kb, args),
        "tab_rename" => tabs::tab_rename(state, kb, args),
        "assembly_get" => assembly::assembly_get(state, kb),
        "instance_add" => assembly::instance_add(state, kb, args),
        "instance_edit" => assembly::instance_edit(state, kb, args),
        "instance_delete" => assembly::instance_delete(state, kb, args),
        "connector_add" => assembly::connector_add(state, kb, args),
        "connector_edit" => assembly::connector_edit(state, kb, args),
        "connector_delete" => assembly::connector_delete(state, kb, args),
        "mate_add" => assembly::mate_add(state, kb, args),
        "mate_edit" => assembly::mate_edit(state, kb, args),
        "mate_delete" => assembly::mate_delete(state, kb, args),
        other => Err(ToolFailure::new(
            "ToolUnavailable",
            format!("This engine has no tool named \"{other}\"."),
            json!({ "tool": other }),
        )),
    }
}

/// The bodies the viewport shows for the open Part — the list the page's
/// `getBodies()` derives, so the tools refuse the same ids it refuses.
///
/// Two filters that are NOT obvious from the engine's own state:
///
/// - **A ghost belongs to another part.** In-context editing renders other
///   instances' bodies in the edited part's frame; they are not this part's.
/// - **A body with no tessellated mesh is not a body.** `worker.js`
///   `collectBodies` skips an empty vertex buffer, so such a body never
///   reaches the store — and must not reach an agent either.
pub(crate) fn rendered_bodies(state: &EngineState) -> Vec<Value> {
    // One collection shared by the metadata and the vertex check: resolving
    // each body by index would re-walk the list per body
    // (docs/notes/eiffel/FEATURE_NOTES.md §0).
    let addrs = crate::render_view::collect_renderable_bodies(state);
    crate::render_view::body_metadata_for(state, &addrs)
        .into_iter()
        .enumerate()
        .filter(|(index, meta)| {
            meta.get("context") != Some(&json!(true))
                && addrs.get(*index).is_some_and(|addr| {
                    crate::render_view::body_vertices_at(state, addr).is_some_and(|v| !v.is_empty())
                })
        })
        .map(|(_, meta)| meta)
        .collect()
}

/// Just the `bodyId`s [`rendered_bodies`] would report, in the same order.
///
/// The authoring snapshot needs nothing but the ids, twice per call, and
/// building the full metadata for that meant resolving a display name, an
/// ordinal and an instance path for every body in the document — work thrown
/// away a line later (docs/notes/eiffel/FEATURE_NOTES.md §0). The filters are
/// the ones `rendered_bodies` documents: not another part's context body, and
/// not a body without a tessellated mesh.
pub(crate) fn rendered_body_ids(state: &EngineState) -> Vec<String> {
    crate::render_view::collect_renderable_bodies(state)
        .into_iter()
        .filter(|addr| !crate::render_view::is_context_body(state, addr))
        .filter(|addr| {
            crate::render_view::body_vertices_at(state, addr).is_some_and(|v| !v.is_empty())
        })
        .filter_map(|addr| crate::render_view::body_id_of(state, &addr))
        .collect()
}

/// The feature with this id, or `FeatureNotFound` (JS `requireFeature`).
///
/// Matched as text, like the page: an id that is not a UUID at all simply
/// names no feature, which is the same refusal.
pub(crate) fn require_feature<'a>(
    state: &'a EngineState,
    args: &Value,
) -> Result<&'a feature_engine::types::Feature, ToolFailure> {
    let id = args.get("feature_id").and_then(Value::as_str).unwrap_or("");
    state
        .engine
        .tree
        .features
        .iter()
        .find(|f| f.id.to_string() == id)
        .ok_or_else(|| {
            ToolFailure::new(
                "FeatureNotFound",
                format!("No feature with id {id} in the open Part."),
                json!({ "feature_id": id }),
            )
        })
}

/// Check that `body_id` names a rendered body, or `BodyNotFound`
/// (JS `requireBody`).
pub(crate) fn require_body(state: &EngineState, body_id: &str) -> Result<(), ToolFailure> {
    if rendered_bodies(state)
        .iter()
        .any(|b| b.get("bodyId") == Some(&json!(body_id)))
    {
        return Ok(());
    }
    Err(ToolFailure::new(
        "BodyNotFound",
        format!("No body with id {body_id} in the open Part."),
        json!({ "body_id": body_id }),
    ))
}

/// Send one message to the engine and expect an answer, the way the page's
/// `ask()` does: an `Error` response is `Internal` carrying the engine's own
/// `kind` and message — never text this layer parsed (ICR-2).
///
/// `tag` is the message's type name, which is what the page reports.
pub(crate) fn engine_call(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    tag: &str,
    msg: UiToEngine,
) -> Result<EngineToUi, ToolFailure> {
    let response = crate::dispatch::dispatch(state, msg, kb);
    if let EngineToUi::Error { kind, message, .. } = &response {
        return Err(ToolFailure::new(
            "Internal",
            format!("{tag} failed: {message}"),
            json!({ "engine_error": { "kind": kind, "message": message } }),
        ));
    }
    Ok(response)
}

/// The refusal for an answer of the wrong kind — a broken invariant, not a
/// user error, so it is `Internal` with no details (JS `ask`).
pub(crate) fn unexpected(tag: &str, expected: &str, response: &EngineToUi) -> ToolFailure {
    let actual = serde_json::to_value(response)
        .ok()
        .and_then(|v| v.get("type").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| "nothing".to_string());
    ToolFailure::new(
        "Internal",
        format!("{tag} answered {actual}, expected {expected}."),
        json!({}),
    )
}
