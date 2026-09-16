//! Agent tool semantics in Rust (`specs/waffle_server_mode.md` §2.3 S3).
//!
//! One implementation of every non-render agent tool, reachable from any host:
//! the browser page sends `UiToEngine::Tool`, and a native host will call
//! [`execute_tool`] directly. The page keeps only what §3.3 assigns to it —
//! the engine lock, the `UserBusy` / `AgentPaused` gates, the viewport and the
//! storage providers — and the *semantics* (what a tool reads, what it
//! refuses, how its result is shaped) live here.
//!
//! Tools migrate one at a time. Until a tool's JS body is deleted, the page
//! runs both and compares `structuredContent` (the shadow in
//! `app/src/lib/agent/executor.js`); [`MIGRATED`] is the list the shadow reads.
//! A name that has not migrated yet answers `ToolUnavailable`, exactly as an
//! unknown one does — a host must never silently do nothing.

use modeling_ops::KernelBundle;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::engine_state::EngineState;
use crate::messages::{EngineToUi, UiToEngine};

mod inspect;
mod summary;

/// The tools [`execute_tool`] implements. The page shadows exactly these; the
/// rest are still JS. Keep it in sync with the `match` in [`execute_tool`].
pub const MIGRATED: &[&str] = &[
    "model_summary",
    "feature_get",
    "body_measure",
    "face_list",
    "sketch_regions",
    "expression_evaluate",
];

/// An MCP tool result (`specs/waffle_mcp_server.md` §2.3 `result`, I10).
///
/// Field names are the MCP wire names, so the page can hand a result to the
/// relay unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: Vec<Value>,
    #[serde(rename = "structuredContent")]
    pub structured_content: Value,
    #[serde(rename = "isError")]
    pub is_error: bool,
}

impl ToolResult {
    /// A successful result carrying `structured` (JS `toolOk`).
    pub fn ok(structured: Value) -> Self {
        Self {
            content: vec![json!({ "type": "text", "text": structured.to_string() })],
            structured_content: structured,
            is_error: false,
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
/// provenance); read from the authoring tools onward, unused by the read-only
/// tools of these checkpoints.
pub fn execute_tool(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    name: &str,
    arguments: &Value,
    _context: Option<&Value>,
) -> ToolResult {
    match run(state, kb, name, arguments) {
        Ok(structured) => ToolResult::ok(structured),
        Err(failure) => ToolResult::error(failure.code, &failure.message, failure.details),
    }
}

fn run(state: &mut EngineState, kb: &mut dyn KernelBundle, name: &str, args: &Value) -> Answer {
    match name {
        "model_summary" => summary::model_summary(state),
        "feature_get" => inspect::feature_get(state, args),
        "body_measure" => inspect::body_measure(state, kb, args),
        "face_list" => inspect::face_list(state, kb, args),
        "sketch_regions" => inspect::sketch_regions(state, kb, args),
        "expression_evaluate" => inspect::expression_evaluate(state, kb, args),
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
    crate::render_view::body_metadata(state)
        .into_iter()
        .enumerate()
        .filter(|(index, meta)| {
            meta.get("context") != Some(&json!(true))
                && crate::render_view::body_vertices(state, *index).is_some_and(|v| !v.is_empty())
        })
        .map(|(_, meta)| meta)
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
