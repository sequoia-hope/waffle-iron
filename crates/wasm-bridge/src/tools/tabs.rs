//! The tab tools in the engine (`specs/waffle_server_mode.md` §2.3, the
//! P-C remainder): `tab_switch`, `tab_add`, `tab_move`, `tab_rename`.
//!
//! They were the page's JS until 2026-09-23 (`app/src/lib/agent/documents.js`),
//! which kept a native host from holding an assembly document — the tab bar
//! is session state, and the session lives here. Each answers the document
//! as the session knows it: identity, name, tabs, active tab, sources. What
//! only a HOST knows — the storage record and provider, read-only status,
//! whether a user edit is waiting for autosave — the host overlays on the
//! answer (the page from its store, the native host from its file provider),
//! so an agent sees `document_info`'s shape from either.

use serde_json::{json, Value};

use modeling_ops::KernelBundle;

use super::{engine_call, Answer, ToolFailure};
use crate::engine_state::EngineState;
use crate::messages::UiToEngine;

/// The tab tools, for hosts that overlay their storage fields on the answer.
pub const TAB_TOOLS: &[&str] = &["tab_switch", "tab_add", "tab_move", "tab_rename"];

/// The document as the session knows it — the engine's share of
/// `document_info` (the page's `documentInfo()` minus its host fields).
pub fn document_core(state: &EngineState) -> Value {
    let doc = crate::dispatch::document_info(state);
    let sources: Vec<Value> = crate::dispatch::source_statuses(state)
        .into_iter()
        .map(|s| json!({ "id": s.id, "name": s.name, "kind": s.kind, "available": s.available }))
        .collect();
    json!({
        "document_id": doc.id,
        "name": doc.name,
        "tabs": doc.tabs.iter().map(|t| json!({ "id": t.id, "name": t.name, "kind": t.kind })).collect::<Vec<_>>(),
        "active_tab": doc.active_tab,
        "sources": sources,
    })
}

fn tab_id_arg(args: &Value) -> String {
    args.get("tab_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// The tab with this id, or `TabNotFound` (JS `requireTab`).
fn require_tab(state: &EngineState, tab_id: &str) -> Result<crate::session::TabInfo, ToolFailure> {
    state
        .session
        .tabs()
        .into_iter()
        .find(|t| t.id == tab_id)
        .ok_or_else(|| {
            ToolFailure::new(
                "TabNotFound",
                format!("The document has no tab with id {tab_id}."),
                json!({ "tab_id": tab_id }),
            )
        })
}

/// Make `tab_id` the active tab: an Assembly tab is opened and evaluated, a
/// Part tab is rebuilt. A tab that is already active is left alone, as the
/// page's `switchTab` leaves it.
fn activate(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    tab: &crate::session::TabInfo,
) -> Result<(), ToolFailure> {
    if state.session.active_tab_id() == tab.id {
        return Ok(());
    }
    if tab.kind == "Assembly" {
        engine_call(
            state,
            kb,
            "OpenAssembly",
            UiToEngine::OpenAssembly {
                tab_id: tab.id.clone(),
            },
        )?;
    } else {
        engine_call(
            state,
            kb,
            "SwitchTab",
            UiToEngine::SwitchTab {
                tab_id: tab.id.clone(),
            },
        )?;
    }
    Ok(())
}

/// `tab_switch`: Part tabs take the feature tools, Assembly tabs the
/// assembly tools; any other kind has no tool to work it.
pub(crate) fn tab_switch(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab_id = tab_id_arg(args);
    let tab = require_tab(state, &tab_id)?;
    if tab.kind != "Part" && tab.kind != "Assembly" {
        return Err(ToolFailure::new(
            "TabKindNotSupported",
            format!(
                "Tab {} is a {} tab; agents work on Part and Assembly tabs.",
                tab.name, tab.kind
            ),
            json!({ "kind": tab.kind }),
        ));
    }
    activate(state, kb, &tab)?;
    Ok(document_core(state))
}

/// `tab_add`: a new Part or Assembly tab after the last one, named
/// `"{kind} N"` unless `name` is given, and activated unless `activate` is
/// false. Answers `{tab_id, ...document}`.
pub(crate) fn tab_add(state: &mut EngineState, kb: &mut dyn KernelBundle, args: &Value) -> Answer {
    let kind = args
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("Part")
        .to_string();
    if kind != "Part" && kind != "Assembly" {
        return Err(ToolFailure::new(
            "TabKindNotSupported",
            format!("A tab of kind {kind} cannot be added; agents add Part and Assembly tabs."),
            json!({ "kind": kind }),
        ));
    }
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .filter(|n| !n.is_empty())
        .map(str::to_string);
    let activate_it = args
        .get("activate")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let before: Vec<String> = state.session.tabs().into_iter().map(|t| t.id).collect();
    engine_call(state, kb, "AddTab", UiToEngine::AddTab { kind, name })?;
    let added = state
        .session
        .tabs()
        .into_iter()
        .find(|t| !before.contains(&t.id))
        .ok_or_else(|| {
            ToolFailure::new("Internal", "The engine did not add the tab.", json!({}))
        })?;
    if activate_it {
        activate(state, kb, &added)?;
    }
    let mut answer = json!({ "tab_id": added.id });
    merge(&mut answer, document_core(state));
    Ok(answer)
}

/// `tab_move`: to `index` in the bar (0 = first); an index past the end
/// moves it last.
pub(crate) fn tab_move(state: &mut EngineState, kb: &mut dyn KernelBundle, args: &Value) -> Answer {
    let tab_id = tab_id_arg(args);
    require_tab(state, &tab_id)?;
    let index = args
        .get("index")
        .and_then(Value::as_f64)
        .map(|i| i.max(0.0).trunc() as usize)
        .unwrap_or(0);
    engine_call(state, kb, "MoveTab", UiToEngine::MoveTab { tab_id, index })?;
    Ok(document_core(state))
}

/// `tab_rename`.
pub(crate) fn tab_rename(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab_id = tab_id_arg(args);
    require_tab(state, &tab_id)?;
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    engine_call(
        state,
        kb,
        "RenameTab",
        UiToEngine::RenameTab { tab_id, name },
    )?;
    Ok(document_core(state))
}

/// Add every field of `extra` to the object `into` (existing keys kept).
pub(crate) fn merge(into: &mut Value, extra: Value) {
    if let (Some(target), Some(source)) = (into.as_object_mut(), extra.as_object()) {
        for (k, v) in source {
            target.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }
}
