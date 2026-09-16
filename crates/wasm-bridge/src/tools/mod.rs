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

mod summary;

/// The tools [`execute_tool`] implements. The page shadows exactly these; the
/// rest are still JS. Keep it in sync with the `match` in [`execute_tool`].
pub const MIGRATED: &[&str] = &["model_summary"];

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

/// Run one agent tool against the open document.
///
/// `context` carries the per-call state a host holds (the agent's name, for
/// provenance); read from the authoring tools onward, unused by the read-only
/// tools of this checkpoint.
pub fn execute_tool(
    state: &mut EngineState,
    _kb: &mut dyn KernelBundle,
    name: &str,
    _arguments: &Value,
    _context: Option<&Value>,
) -> ToolResult {
    match name {
        "model_summary" => summary::model_summary(state),
        other => ToolResult::error(
            "ToolUnavailable",
            &format!("This engine has no tool named \"{other}\"."),
            json!({ "tool": other }),
        ),
    }
}
