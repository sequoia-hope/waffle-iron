//! The custom-feature-script agent tools
//! (`specs/custom_features_and_modeling_roadmap.md` §A8, A-M4): the agent's
//! authoring loop is *write the script → `script_run_check` → add the source
//! → `script_feature_add` → read the node's error back → fix the source*.
//!
//! - `script_run_check` — header + compile + entry function, on unsaved text
//!   or a stored source; with `args` a DRY RUN (no kernel) that exercises the
//!   runtime, `ctx.fail`, the limits and the `@output` contract. Read-only.
//! - `script_source_add` — an embedded `Script` source from text or the
//!   built-in library. Refused when the check fails: a source an agent adds
//!   is one it means to use, so a broken one is loud here rather than on a
//!   node later. Not an undo step (sources are assets, v4 §2.3).
//! - `script_source_get` — one source's text and check (and the features
//!   naming it), or, without an id, the document's script sources.
//! - `script_source_update` — replace a source's text and rebuild every node
//!   naming it. A node that newly fails rolls the text back (A2 semantics,
//!   done by re-setting the previous text since sources are outside undo).
//! - `script_feature_add` — one `Script` node; `feature_add` with the same
//!   `Operation` is equivalent, this one takes the arguments directly and
//!   the node takes the script's declared name.

use feature_engine::types::{Operation, ScriptParams};
use modeling_ops::KernelBundle;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::dispatch::{check_script, DEFAULT_SCRIPT_ENTRY, LIBRARY_SCRIPTS};
use crate::engine_state::EngineState;
use crate::messages::{EngineToUi, ScriptCheck, UiToEngine};
use crate::tools::author::{
    agent_provenance, apply_step, model_delta, newly_erroring, same_model, send, snapshot,
    with_feature_id, OnError,
};
use crate::tools::{Answer, ToolFailure};

/// `source_id` as a UUID, or `SourceNotFound` (a malformed id names nothing).
fn parse_source_id(args: &Value) -> Result<Uuid, ToolFailure> {
    let raw = args.get("source_id").and_then(Value::as_str).unwrap_or("");
    Uuid::parse_str(raw).map_err(|_| source_not_found(raw))
}

fn source_not_found(id: &str) -> ToolFailure {
    ToolFailure::new(
        "SourceNotFound",
        format!("No script source with id {id} in the open document."),
        json!({ "source_id": id }),
    )
}

/// The `Script` source entry with this id, or `SourceNotFound`. A source of
/// another kind is also "not a script source".
fn require_script_source(
    state: &EngineState,
    id: Uuid,
) -> Result<&file_format::SourceEntry, ToolFailure> {
    state
        .sources
        .iter()
        .find(|s| s.id == id && matches!(s.kind, file_format::SourceKind::Script))
        .ok_or_else(|| source_not_found(&id.to_string()))
}

/// The features whose `Script` operation names `source_id`, in tree order.
fn features_naming(state: &EngineState, source_id: Uuid) -> Vec<Value> {
    state
        .engine
        .tree
        .features
        .iter()
        .filter(|f| matches!(&f.operation, Operation::Script { params } if params.source_id == source_id))
        .map(|f| json!({ "feature_id": f.id, "name": f.name }))
        .collect()
}

/// The `entry` argument, defaulted.
fn entry_of(args: &Value) -> String {
    args.get("entry")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .unwrap_or(DEFAULT_SCRIPT_ENTRY)
        .to_string()
}

/// The `args` argument as the script's argument map (absent ⇒ empty).
fn arg_map(args: &Value, key: &str) -> Result<BTreeMap<String, Value>, ToolFailure> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(BTreeMap::new()),
        Some(Value::Object(map)) => Ok(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
        Some(other) => Err(ToolFailure::new(
            "InvalidScript",
            format!("`{key}` must be an object of parameter name → value, got {other}"),
            json!({ "schema_path": format!("/{key}") }),
        )),
    }
}

/// A check that did not pass, as the `InvalidScript` refusal (the stage and
/// the interpreter's own message, verbatim).
fn invalid_script(check: &ScriptCheck) -> ToolFailure {
    let (stage, reason) = check
        .error
        .as_ref()
        .map(|e| (e.stage.clone(), e.reason.clone()))
        .unwrap_or_else(|| ("check".to_string(), "the script did not check".to_string()));
    ToolFailure::new(
        "InvalidScript",
        format!("script {stage}: {reason}"),
        json!({ "stage": stage, "reason": reason }),
    )
}

/// `{ ok, interface?, error?, dry_run? }` for an answer.
fn check_json(check: &ScriptCheck) -> Value {
    serde_json::to_value(check).unwrap_or(Value::Null)
}

// ── The tools ────────────────────────────────────────────────────────────

/// Check a script without a node; with `args`, dry-run it. Read-only.
pub(super) fn script_run_check(state: &mut EngineState, args: &Value) -> Answer {
    let text = args.get("text").and_then(Value::as_str).map(str::to_string);
    let source_id = match args.get("source_id") {
        Some(v) if !v.is_null() => Some(parse_source_id(args)?),
        _ => None,
    };
    let text = match (text, source_id) {
        (Some(t), _) => t,
        (None, Some(id)) => {
            require_script_source(state, id)?;
            state
                .engine
                .sources
                .text(id)
                .ok_or_else(|| source_not_found(&id.to_string()))?
        }
        (None, None) => {
            return Err(ToolFailure::new(
                "InvalidScript",
                "Give `text` (unsaved script text) or `source_id` (a stored script source).",
                json!({ "schema_path": "/text" }),
            ))
        }
    };
    let entry = entry_of(args);
    let dry_run_args = match args.get("args") {
        None | Some(Value::Null) => None,
        Some(_) => Some((source_id, arg_map(args, "args")?)),
    };
    let check = check_script(&text, &entry, dry_run_args);
    let mut out = check_json(&check);
    if let Some(id) = source_id {
        out["source_id"] = json!(id);
    }
    out["entry"] = json!(entry);
    Ok(out)
}

/// Add an embedded `Script` source (text or a library script). Refused
/// when the script does not check.
pub(super) fn script_source_add(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let text = args.get("text").and_then(Value::as_str).map(str::to_string);
    let library = args
        .get("library")
        .and_then(Value::as_str)
        .map(str::to_string);
    let name = args.get("name").and_then(Value::as_str).map(str::to_string);
    if text.is_none() && library.is_none() {
        return Err(ToolFailure::new(
            "InvalidScript",
            format!(
                "Give `text` (the script) or `library` (one of {}).",
                LIBRARY_SCRIPTS.join(", ")
            ),
            json!({ "schema_path": "/text" }),
        ));
    }
    if let Some(lib) = &library {
        if text.is_some() {
            return Err(ToolFailure::new(
                "InvalidScript",
                "Give `text` or `library`, not both.",
                json!({ "schema_path": "/library" }),
            ));
        }
        if !LIBRARY_SCRIPTS.contains(&lib.as_str()) {
            return Err(ToolFailure::new(
                "InvalidScript",
                format!(
                    "No built-in script library `{lib}` (one of {}).",
                    LIBRARY_SCRIPTS.join(", ")
                ),
                json!({ "schema_path": "/library", "library": lib }),
            ));
        }
    }
    // Check BEFORE the source exists: a source that does not parse is
    // refused, never added (the editor may save work in progress; an agent
    // checks first).
    if let Some(t) = &text {
        let check = check_script(t, DEFAULT_SCRIPT_ENTRY, None);
        if !check.ok {
            return Err(invalid_script(&check));
        }
    }
    let response = send(
        state,
        kb,
        UiToEngine::AddScriptSource {
            name,
            text,
            library,
        },
        "InvalidScript",
    )?;
    let EngineToUi::ScriptSourceAdded {
        source_id,
        name,
        check,
        ..
    } = &response
    else {
        return Err(crate::tools::unexpected(
            "AddScriptSource",
            "ScriptSourceAdded",
            &response,
        ));
    };
    Ok(json!({
        "source_id": source_id,
        "name": name,
        "interface": check.interface,
    }))
}

/// One script source's text and check, or the list of script sources.
pub(super) fn script_source_get(state: &mut EngineState, args: &Value) -> Answer {
    let wants_one = matches!(args.get("source_id"), Some(v) if !v.is_null());
    if !wants_one {
        let scripts: Vec<Value> = state
            .sources
            .iter()
            .filter(|s| matches!(s.kind, file_format::SourceKind::Script))
            .map(|s| {
                let text = state.engine.sources.text(s.id);
                let check = text
                    .as_deref()
                    .map(|t| check_script(t, DEFAULT_SCRIPT_ENTRY, None));
                json!({
                    "source_id": s.id,
                    "name": s.name,
                    "available": text.is_some(),
                    "ok": check.as_ref().map(|c| c.ok),
                    "feature_name": check.as_ref().and_then(|c| c.interface.as_ref()).and_then(|i| i.get("name").cloned()),
                    "features": features_naming(state, s.id),
                })
            })
            .collect();
        return Ok(json!({ "scripts": scripts, "library": LIBRARY_SCRIPTS }));
    }
    let id = parse_source_id(args)?;
    let entry = require_script_source(state, id)?;
    let name = entry.name.clone();
    let text = state
        .engine
        .sources
        .text(id)
        .ok_or_else(|| source_not_found(&id.to_string()))?;
    let check = check_script(&text, DEFAULT_SCRIPT_ENTRY, None);
    Ok(json!({
        "source_id": id,
        "name": name,
        "text": text,
        "check": check_json(&check),
        "features": features_naming(state, id),
    }))
}

/// Replace a script source's text and rebuild (every node naming it
/// regenerates). Refused when the new text does not check; a node the new
/// text newly breaks rolls the text back unless `on_error` is `keep`.
pub(super) fn script_source_update(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let id = parse_source_id(args)?;
    require_script_source(state, id)?;
    let Some(text) = args.get("text").and_then(Value::as_str) else {
        return Err(ToolFailure::new(
            "InvalidScript",
            "`text` (the new script text) is required.",
            json!({ "schema_path": "/text" }),
        ));
    };
    let check = check_script(text, DEFAULT_SCRIPT_ENTRY, None);
    if !check.ok {
        return Err(invalid_script(&check));
    }
    let previous = state
        .engine
        .sources
        .text(id)
        .ok_or_else(|| source_not_found(&id.to_string()))?;
    let on_error = OnError::from_args(args);

    crate::tessellation_runner::tessellate_missing_meshes(state, kb);
    let before = snapshot(state);
    let response = send(
        state,
        kb,
        UiToEngine::SetScriptSource {
            source_id: id,
            text: text.to_string(),
        },
        "InvalidScript",
    )?;
    crate::tessellation_runner::tessellate_missing_meshes(state, kb);
    let after = snapshot(state);
    let (typed_errors, warnings) = match &response {
        EngineToUi::ModelUpdated {
            feature_errors,
            warnings,
            ..
        } => (feature_errors.clone(), warnings.clone()),
        _ => (Vec::new(), Vec::new()),
    };
    let fresh = newly_erroring(&before, &after);
    if !fresh.is_empty() && on_error == OnError::Rollback {
        // Sources are outside undo: the rollback is the previous text again.
        let kind_of = |fid: &str| {
            typed_errors
                .iter()
                .find(|e| e.feature_id.to_string() == fid)
                .map(|e| e.kind.clone())
        };
        let errors: Vec<Value> = fresh
            .iter()
            .map(|(fid, message)| {
                json!({ "feature_id": fid, "message": message, "kind": kind_of(fid) })
            })
            .collect();
        send(
            state,
            kb,
            UiToEngine::SetScriptSource {
                source_id: id,
                text: previous,
            },
            "Internal",
        )?;
        crate::tessellation_runner::tessellate_missing_meshes(state, kb);
        if !same_model(&before, &snapshot(state)) {
            return Err(ToolFailure::new(
                "Internal",
                "The rollback did not restore the document exactly; the agent session is paused.",
                json!({ "source_id": id, "pause_agent": true }),
            ));
        }
        let (target_id, target_message) = fresh[0].clone();
        return Err(ToolFailure::new(
            "FeatureRebuildFailed",
            target_message.clone(),
            json!({
                "source_id": id,
                "feature_id": target_id,
                "engine_error": { "kind": kind_of(&target_id), "message": target_message },
                "errors": errors,
                "rolled_back": true,
            }),
        ));
    }
    let mut delta = model_delta(&before, &after, &typed_errors, &warnings);
    if !fresh.is_empty() && on_error == OnError::Keep {
        delta["kept_with_error"] = json!(true);
    }
    delta["source_id"] = json!(id);
    delta["interface"] = check.interface.unwrap_or(Value::Null);
    Ok(delta)
}

/// Add one `Script` node (one undo step).
pub(super) fn script_feature_add(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
    context: Option<&Value>,
) -> Answer {
    let id = parse_source_id(args)?;
    require_script_source(state, id)?;
    let entry = entry_of(args);
    let script_args = arg_map(args, "args")?;
    let mut arg_exprs = BTreeMap::new();
    for (k, v) in arg_map(args, "arg_exprs")? {
        let Some(expr) = v.as_str() else {
            return Err(ToolFailure::new(
                "InvalidScript",
                format!("`arg_exprs.{k}` must be an expression string, got {v}"),
                json!({ "schema_path": "/arg_exprs" }),
            ));
        };
        arg_exprs.insert(k, expr.to_string());
    }
    let operation = Operation::Script {
        params: ScriptParams {
            source_id: id,
            entry,
            args: script_args,
            arg_exprs,
            arg_values: BTreeMap::new(),
        },
    };
    let step = apply_step(
        state,
        kb,
        UiToEngine::AddFeature {
            operation,
            provenance: agent_provenance(context),
        },
        OnError::from_args(args),
        "InvalidOperation",
    )?;
    Ok(with_feature_id(step))
}
