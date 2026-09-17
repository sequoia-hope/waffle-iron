//! The authoring agent tools (`specs/waffle_mcp_server.md` §2.5 Authoring),
//! ported from `app/src/lib/agent/commands.js` and `delta.js`.
//!
//! Every tool here changes the document, and they all share one core:
//! [`apply_step`] — snapshot, send one model-changing message, snapshot again,
//! and account for the difference as a `ModelDelta`. A step that makes any
//! feature *newly* fail is undone and the undo verified exact (A2, A4), unless
//! the caller asked to keep it (A3) or the failure is the expected outcome
//! (delete, suppress, … — A15).
//!
//! Three things about this port are not obvious from the JS:
//!
//! - **The step must tessellate before its after-snapshot.** `bodies_added` and
//!   `bodies_removed` come from the rendered body list, and a body with no
//!   tessellated mesh is not in it ([`crate::tools::rendered_bodies`]). In the
//!   page the worker tessellates before the store sees the answer; here the
//!   tool runs *inside* one dispatch, and `process::process_message` only
//!   tessellates a `ModelUpdated` — never a `ToolResult`. Without the explicit
//!   pass below, every `feature_add` would answer `bodies_added: []`.
//! - **The toast and the pause are the host's.** §3.3 leaves rendering with the
//!   host, so a rolled-back step says so in `details.rolled_back`, and a
//!   rollback that did not restore the document exactly says `pause_agent`.
//!   Nothing here draws anything.
//! - **`EngineCrashed` cannot arise in this layer.** In the page it came from
//!   the worker failing to restart (`needsRestart`), which is a transport
//!   failure, not an engine answer. A host detects its own crashed engine.

use feature_engine::types::{DesignParameter, Operation, Provenance, ProvenanceOrigin};
use modeling_ops::KernelBundle;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

use crate::engine_state::EngineState;
use crate::messages::{EngineToUi, UiToEngine};
use crate::tools::{require_body, require_feature, Answer, ToolFailure};

/// Fillet, chamfer and shell are deferred project-wide (A5, I11).
const DEFERRED: &[&str] = &["Fillet", "Chamfer", "Shell"];

/// Operation kinds an agent may author through `feature_add` / `feature_edit`.
const AUTHORABLE: &[&str] = &[
    "Sketch",
    "Extrude",
    "Revolve",
    "BooleanCombine",
    "DatumPlane",
    "MateConnector",
];

/// What a failing step does (JS `applyStep`'s `onError`).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum OnError {
    /// Undo the step and refuse the call (A2, A4). The default.
    Rollback,
    /// Leave it in place and report `kept_with_error` (A3).
    Keep,
    /// Leave it without the flag: a delete or suppress whose dependents fail
    /// is doing what it was asked (A15).
    Report,
}

impl OnError {
    /// The caller's `on_error`, which the schema limits to rollback | keep.
    pub(super) fn from_args(args: &Value) -> Self {
        match args.get("on_error").and_then(Value::as_str) {
            Some("keep") => OnError::Keep,
            _ => OnError::Rollback,
        }
    }
}

/// The model state one step can change (JS `takeSnapshot`).
struct Snapshot {
    /// The feature tree as JSON: features, rollback index, provenance,
    /// parameters, body names. Compared as a `Value`, whose maps are ordered,
    /// so this is the JS canonical-JSON comparison without the string.
    tree: Value,
    /// Feature id → its rebuild error, last message winning, like the store's
    /// map.
    errors: HashMap<String, String>,
    /// The rendered bodies, in render order.
    body_ids: Vec<String>,
}

impl Snapshot {
    /// The features of this snapshot, in tree order.
    fn feature_ids(&self) -> Vec<String> {
        self.tree
            .get("features")
            .and_then(Value::as_array)
            .map(|features| {
                features
                    .iter()
                    .filter_map(|f| f.get("id").and_then(Value::as_str).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// One feature's definition together with its provenance origin — what
    /// `features_changed` compares (JS `featureRecord`).
    fn feature_record(&self, id: &str) -> Value {
        let feature = self
            .tree
            .get("features")
            .and_then(Value::as_array)
            .and_then(|features| {
                features
                    .iter()
                    .find(|f| f.get("id").and_then(Value::as_str) == Some(id))
            })
            .cloned()
            .unwrap_or(Value::Null);
        let origin = self
            .tree
            .get("provenance")
            .and_then(|table| table.get(id))
            .and_then(|record| record.get("origin"))
            .cloned()
            .unwrap_or_else(|| json!({ "type": "User" }));
        json!({ "feature": feature, "origin": origin })
    }
}

/// The document as it stands (JS `snapshotNow`).
fn snapshot(state: &EngineState) -> Snapshot {
    Snapshot {
        tree: serde_json::to_value(&state.engine.tree).unwrap_or(Value::Null),
        errors: state
            .engine
            .errors
            .iter()
            .map(|(id, message)| (id.to_string(), message.clone()))
            .collect(),
        body_ids: crate::tools::rendered_bodies(state)
            .iter()
            .filter_map(|b| b.get("bodyId").and_then(Value::as_str).map(str::to_string))
            .collect(),
    }
}

/// Whether two snapshots hold the same document model (JS `sameModel`).
fn same_model(a: &Snapshot, b: &Snapshot) -> bool {
    a.tree == b.tree
}

/// Features whose rebuild error is new or changed, in `after`'s tree order
/// (errors for ids outside the tree last, sorted). An error that merely
/// persists is not "new" (JS `newlyErroring`).
fn newly_erroring(before: &Snapshot, after: &Snapshot) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for id in error_ids_in_order(after) {
        let Some(message) = after.errors.get(&id) else {
            continue;
        };
        if before.errors.get(&id) != Some(message) {
            out.push((id, message.clone()));
        }
    }
    out
}

/// Every id carrying an error, in tree order, with ids the tree does not hold
/// last and sorted — reported, never dropped.
fn error_ids_in_order(snapshot: &Snapshot) -> Vec<String> {
    let in_tree = snapshot.feature_ids();
    let mut ids: Vec<String> = in_tree
        .iter()
        .filter(|id| snapshot.errors.contains_key(*id))
        .cloned()
        .collect();
    let mut orphans: Vec<String> = snapshot
        .errors
        .keys()
        .filter(|id| !in_tree.contains(*id))
        .cloned()
        .collect();
    orphans.sort();
    ids.extend(orphans);
    ids
}

/// The spec's `ModelDelta` between two snapshots (JS `modelDelta`).
///
/// `features_changed` lists features present in both whose definition or
/// provenance origin changed; a pure reorder changes no record, so it is
/// reported by `order_changed` instead.
fn model_delta(
    before: &Snapshot,
    after: &Snapshot,
    typed_errors: &[feature_engine::types::FeatureError],
    warnings: &[String],
) -> Value {
    let before_ids = before.feature_ids();
    let after_ids = after.feature_ids();

    let common: Vec<String> = after_ids
        .iter()
        .filter(|id| before_ids.contains(*id))
        .cloned()
        .collect();
    let common_before: Vec<String> = before_ids
        .iter()
        .filter(|id| after_ids.contains(*id))
        .cloned()
        .collect();

    let kinds: HashMap<String, &feature_engine::types::ErrorKind> = typed_errors
        .iter()
        .map(|e| (e.feature_id.to_string(), &e.kind))
        .collect();

    let errors: Vec<Value> = error_ids_in_order(after)
        .into_iter()
        .map(|id| {
            let mut row = json!({
                "feature_id": id,
                "message": after.errors.get(&id),
            });
            if let Some(kind) = kinds.get(&id) {
                row["kind"] = json!(kind);
            }
            row
        })
        .collect();

    json!({
        "features_added": after_ids
            .iter()
            .filter(|id| !before_ids.contains(*id))
            .collect::<Vec<_>>(),
        "features_changed": common
            .iter()
            .filter(|id| before.feature_record(id) != after.feature_record(id))
            .collect::<Vec<_>>(),
        "features_removed": before_ids
            .iter()
            .filter(|id| !after_ids.contains(*id))
            .collect::<Vec<_>>(),
        "order_changed": common
            .iter()
            .enumerate()
            .any(|(i, id)| common_before.get(i) != Some(id)),
        "bodies_added": after
            .body_ids
            .iter()
            .filter(|id| !before.body_ids.contains(*id))
            .collect::<Vec<_>>(),
        "bodies_removed": before
            .body_ids
            .iter()
            .filter(|id| !after.body_ids.contains(*id))
            .collect::<Vec<_>>(),
        "errors": errors,
        "warnings": warnings,
    })
}

/// One model-changing message, sent and accounted for.
pub(super) struct Step {
    pub(super) delta: Value,
    pub(super) feature_id: Option<Uuid>,
}

/// Send one message, mapping an engine rejection to a typed tool failure.
///
/// Never parses message text: the class comes from the answer's `kind`
/// (ICR-2). `fallback` is the code for a bridge-level failure, which carries
/// no engine kind.
pub(super) fn send(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    msg: UiToEngine,
    fallback: &'static str,
) -> Result<EngineToUi, ToolFailure> {
    let response = crate::dispatch::dispatch(state, msg, kb);
    let EngineToUi::Error { kind, message, .. } = &response else {
        return Ok(response);
    };
    let engine_error = json!({ "engine_error": { "kind": kind, "message": message } });

    use feature_engine::types::ErrorKind;
    let failure = match kind {
        Some(ErrorKind::FeatureNotFound { .. }) => {
            ToolFailure::new("FeatureNotFound", message.clone(), engine_error)
        }
        Some(ErrorKind::NothingToUndo) => {
            ToolFailure::new("NothingToUndo", "There is nothing to undo.", engine_error)
        }
        Some(ErrorKind::NothingToRedo) => {
            ToolFailure::new("NothingToRedo", "There is nothing to redo.", engine_error)
        }
        Some(ErrorKind::NotSupported { .. }) => {
            ToolFailure::new("NotSupported", message.clone(), engine_error)
        }
        Some(_) => ToolFailure::new("FeatureRebuildFailed", message.clone(), engine_error),
        // No kind: the message never formed or was refused by the bridge.
        None => match fallback {
            "InvalidOperation" => ToolFailure::new(
                "InvalidOperation",
                message.clone(),
                json!({ "schema_path": "/operation", "reason": message }),
            ),
            "InvalidSketch" => ToolFailure::new(
                "InvalidSketch",
                message.clone(),
                json!({ "reason": message }),
            ),
            other => ToolFailure::new(other, message.clone(), engine_error),
        },
    };
    Err(failure)
}

/// Send one model-changing message and account for it (JS `applyStep`).
pub(super) fn apply_step(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    msg: UiToEngine,
    on_error: OnError,
    fallback: &'static str,
) -> Result<Step, ToolFailure> {
    // The live state is tessellated by the dispatch that produced it, but a
    // tool may run after one that was not (a `Tool` response is not a
    // `ModelUpdated`, so `process_message` skips its tessellation pass).
    crate::tessellation_runner::tessellate_missing_meshes(state, kb);
    let before = snapshot(state);

    let sent_feature_id = message_feature_id(&msg);
    let response = send(state, kb, msg, fallback)?;
    // The bodies this step created have no mesh until this runs, and a body
    // with no mesh is not in the rendered list: without it `bodies_added` is
    // always empty.
    crate::tessellation_runner::tessellate_missing_meshes(state, kb);
    let after = snapshot(state);

    let (typed_errors, warnings, answered_feature_id) = match &response {
        EngineToUi::ModelUpdated {
            feature_errors,
            warnings,
            feature_id,
            ..
        } => (feature_errors.clone(), warnings.clone(), *feature_id),
        _ => (Vec::new(), Vec::new(), None),
    };
    let feature_id = answered_feature_id.or(sent_feature_id);
    let fresh = newly_erroring(&before, &after);

    if !fresh.is_empty() && on_error == OnError::Rollback {
        return Err(roll_back(
            state,
            kb,
            &before,
            &fresh,
            &typed_errors,
            feature_id,
        ));
    }

    let mut delta = model_delta(&before, &after, &typed_errors, &warnings);
    if !fresh.is_empty() && on_error == OnError::Keep {
        delta["kept_with_error"] = json!(true);
    }
    Ok(Step { delta, feature_id })
}

/// Undo a step that made a feature newly fail, and verify the undo restored
/// the document exactly (A2, A4).
///
/// The refusal names the step's own feature when that is one of the failures,
/// so the agent is told what it did rather than which dependent noticed.
fn roll_back(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    before: &Snapshot,
    fresh: &[(String, String)],
    typed_errors: &[feature_engine::types::FeatureError],
    feature_id: Option<Uuid>,
) -> ToolFailure {
    let kind_of = |id: &str| {
        typed_errors
            .iter()
            .find(|e| e.feature_id.to_string() == id)
            .map(|e| &e.kind)
    };
    let target = feature_id
        .and_then(|id| fresh.iter().find(|(fid, _)| *fid == id.to_string()))
        .unwrap_or(&fresh[0]);
    let (target_id, target_message) = (target.0.clone(), target.1.clone());
    let target_kind = kind_of(&target_id).cloned();

    if let Err(failure) = send(state, kb, UiToEngine::Undo, "Internal") {
        return failure;
    }
    crate::tessellation_runner::tessellate_missing_meshes(state, kb);
    if !same_model(before, &snapshot(state)) {
        // The document is not what it was and this layer cannot repair it: the
        // host must stop the agent (it renders the pause; §3.3).
        return ToolFailure::new(
            "Internal",
            "The rollback did not restore the document exactly; the agent session is paused.",
            json!({ "feature_id": target_id, "pause_agent": true }),
        );
    }

    let code = if matches!(
        target_kind,
        Some(feature_engine::types::ErrorKind::NotSupported { .. })
    ) {
        "NotSupported"
    } else {
        "FeatureRebuildFailed"
    };
    ToolFailure::new(
        code,
        target_message.clone(),
        json!({
            "feature_id": target_id,
            "engine_error": { "kind": target_kind, "message": target_message },
            "errors": fresh
                .iter()
                .map(|(id, message)| json!({
                    "feature_id": id,
                    "message": message,
                    "kind": kind_of(id),
                }))
                .collect::<Vec<_>>(),
            "rolled_back": true,
        }),
    )
}

/// The feature a message names, for steps whose answer carries no id of its
/// own (JS `message.feature_id`).
fn message_feature_id(msg: &UiToEngine) -> Option<Uuid> {
    match msg {
        UiToEngine::EditFeature { feature_id, .. }
        | UiToEngine::DeleteFeature { feature_id }
        | UiToEngine::SuppressFeature { feature_id, .. }
        | UiToEngine::ReorderFeature { feature_id, .. }
        | UiToEngine::RenameFeature { feature_id, .. } => Some(*feature_id),
        _ => None,
    }
}

/// The provenance an agent's step records (ICR-4). `at` is a timestamp the
/// engine has no clock for, and the page sends none either.
pub(super) fn agent_provenance(context: Option<&Value>) -> Option<Provenance> {
    Some(Provenance {
        origin: ProvenanceOrigin::Agent {
            name: agent_name(context),
        },
        at: None,
    })
}

/// The calling agent's name, which the host supplies per call.
fn agent_name(context: Option<&Value>) -> String {
    context
        .and_then(|c| c.get("agent_name"))
        .and_then(Value::as_str)
        .unwrap_or("agent")
        .to_string()
}

/// Refuse an operation an agent may not author (JS `checkOperation`).
///
/// Checked on the type TAG, before the operation is parsed, so an unknown or
/// deferred kind is refused by name rather than by a parse failure.
fn check_operation(operation: &Value) -> Result<(), ToolFailure> {
    let type_tag = operation.get("type").and_then(Value::as_str);
    let Some(tag) = type_tag else {
        return Err(invalid_operation(type_tag));
    };
    if DEFERRED.contains(&tag) {
        return Err(ToolFailure::new(
            "Deferred",
            format!("{tag} is deferred in Waffle Iron and cannot be authored."),
            json!({ "operation": tag }),
        ));
    }
    if tag == "ImportedBody" {
        return Err(ToolFailure::new(
            "UseImportTool",
            "Imported bodies come from a STEP import, not from feature_add or feature_edit.",
            json!({ "operation": tag }),
        ));
    }
    if !AUTHORABLE.contains(&tag) {
        return Err(invalid_operation(type_tag));
    }
    Ok(())
}

fn invalid_operation(type_tag: Option<&str>) -> ToolFailure {
    let quoted = match type_tag {
        Some(tag) => format!("\"{tag}\""),
        None => "undefined".to_string(),
    };
    ToolFailure::new(
        "InvalidOperation",
        format!("Operation type {quoted} cannot be authored."),
        json!({
            "schema_path": "/operation/type",
            "reason": format!("expected one of {}", AUTHORABLE.join(", ")),
        }),
    )
}

/// The `operation` argument as an `Operation`, once its tag is allowed.
fn parse_operation(operation: &Value) -> Result<Operation, ToolFailure> {
    serde_json::from_value(operation.clone()).map_err(|e| {
        ToolFailure::new(
            "InvalidOperation",
            e.to_string(),
            json!({ "schema_path": "/operation", "reason": e.to_string() }),
        )
    })
}

/// A feature's provenance origin tag, defaulting to `User` (JS
/// `provenanceOrigin`).
fn provenance_origin(state: &EngineState, id: Uuid) -> String {
    state
        .engine
        .tree
        .provenance
        .get(&id)
        .map(|p| match &p.origin {
            ProvenanceOrigin::User => "User",
            ProvenanceOrigin::Agent { .. } => "Agent",
            ProvenanceOrigin::Import { .. } => "Import",
            ProvenanceOrigin::Derived { .. } => "Derived",
        })
        .unwrap_or("User")
        .to_string()
}

/// The `Operation`'s serde tag.
fn operation_tag(operation: &Operation) -> Option<String> {
    serde_json::to_value(operation)
        .ok()
        .and_then(|v| v.get("type").and_then(Value::as_str).map(str::to_string))
}

// ── The tools ────────────────────────────────────────────────────────────

/// Add a feature at the end of the tree (one undo step).
pub(super) fn feature_add(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
    context: Option<&Value>,
) -> Answer {
    let operation = args.get("operation").unwrap_or(&Value::Null);
    check_operation(operation)?;
    let operation = parse_operation(operation)?;
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

/// Replace a feature's operation, same kind, and rebuild (one undo step).
pub(super) fn feature_edit(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
    context: Option<&Value>,
) -> Answer {
    let feature = require_feature(state, args)?;
    let feature_id = feature.id;
    let current_tag = operation_tag(&feature.operation);

    let origin = provenance_origin(state, feature_id);
    if origin == "Derived" {
        return Err(ToolFailure::new(
            "DerivedFeatureReadOnly",
            "This feature is regenerated from a source and cannot be edited.",
            json!({ "feature_id": feature_id }),
        ));
    }
    if origin == "Import" || current_tag.as_deref() == Some("ImportedBody") {
        return Err(ToolFailure::new(
            "UseImportTool",
            "Imported features are placed through the import dialog, not feature_edit.",
            json!({ "feature_id": feature_id }),
        ));
    }

    let operation = args.get("operation").unwrap_or(&Value::Null);
    check_operation(operation)?;
    let new_tag = operation.get("type").and_then(Value::as_str);
    if current_tag.as_deref() != new_tag {
        return Err(ToolFailure::new(
            "OperationKindMismatch",
            format!(
                "Feature {feature_id} is a {}, not a {}.",
                current_tag.clone().unwrap_or_else(|| "undefined".into()),
                new_tag.unwrap_or("undefined"),
            ),
            json!({ "expected": current_tag, "got": new_tag }),
        ));
    }
    let operation = parse_operation(operation)?;

    let step = apply_step(
        state,
        kb,
        UiToEngine::EditFeature {
            feature_id,
            operation,
            provenance: agent_provenance(context),
        },
        OnError::from_args(args),
        "InvalidOperation",
    )?;
    Ok(step.delta)
}

/// Delete a feature (one undo step). Dependents that start failing are
/// reported, not rolled back (A15).
pub(super) fn feature_delete(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let feature_id = require_feature(state, args)?.id;
    let step = apply_step(
        state,
        kb,
        UiToEngine::DeleteFeature { feature_id },
        OnError::Report,
        "Internal",
    )?;
    Ok(step.delta)
}

/// Suppress or unsuppress a feature (one undo step).
pub(super) fn feature_suppress(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let feature_id = require_feature(state, args)?.id;
    let suppressed = args
        .get("suppressed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let step = apply_step(
        state,
        kb,
        UiToEngine::SuppressFeature {
            feature_id,
            suppressed,
        },
        OnError::Report,
        "Internal",
    )?;
    Ok(step.delta)
}

/// Move a feature to a new zero-based position and rebuild (one undo step).
pub(super) fn feature_reorder(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let feature_id = require_feature(state, args)?.id;
    let new_position = args
        .get("new_position")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let step = apply_step(
        state,
        kb,
        UiToEngine::ReorderFeature {
            feature_id,
            new_position,
        },
        OnError::Report,
        "Internal",
    )?;
    Ok(step.delta)
}

/// Rename a feature (one undo step).
pub(super) fn feature_rename(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let feature_id = require_feature(state, args)?.id;
    let new_name = args
        .get("new_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let step = apply_step(
        state,
        kb,
        UiToEngine::RenameFeature {
            feature_id,
            new_name,
        },
        OnError::Report,
        "Internal",
    )?;
    Ok(step.delta)
}

/// Set a body's display name; an empty name reverts to the derived one.
pub(super) fn body_rename(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let body_id = args
        .get("body_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    require_body(state, &body_id)?;
    let new_name = args
        .get("new_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let step = apply_step(
        state,
        kb,
        UiToEngine::RenameBody { body_id, new_name },
        OnError::Report,
        "Internal",
    )?;
    Ok(step.delta)
}

/// Move the rollback bar. Not an undo step: undo does not move it.
pub(super) fn rollback_set(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let count = state.engine.tree.features.len();
    let index = match args.get("index") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let index = value.as_u64().unwrap_or(0) as usize;
            if index >= count {
                return Err(ToolFailure::new(
                    "FeatureNotFound",
                    format!(
                        "Rollback index {index} is past the last feature (the tree has {count})."
                    ),
                    json!({ "index": index, "feature_count": count }),
                ));
            }
            Some(index)
        }
    };
    let step = apply_step(
        state,
        kb,
        UiToEngine::SetRollbackIndex { index },
        OnError::Report,
        "Internal",
    )?;
    Ok(step.delta)
}

/// Replace the design-parameter table with the complete list and rebuild.
///
/// A failing expression is reported per parameter, not rolled back, so the
/// answer carries the table as the rebuild evaluated it.
pub(super) fn parameters_set(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    // Keyed by the parsed id, not its text: the id below is parsed leniently
    // (`Uuid::parse_str` accepts braces and upper case), so a non-canonical
    // spelling that still names this parameter must find its last value.
    let current: HashMap<Uuid, f64> = state
        .engine
        .tree
        .parameters
        .iter()
        .map(|p| (p.id, p.value))
        .collect();

    let rows = args
        .get("parameters")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut parameters = Vec::with_capacity(rows.len());
    for row in &rows {
        // A parameter with no id is a new one; an id that is not a UUID names
        // no existing parameter, so it becomes one too.
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .and_then(|text| Uuid::parse_str(text).ok())
            .unwrap_or_else(Uuid::new_v4);
        parameters.push(DesignParameter {
            id,
            name: row
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            expression: row
                .get("expression")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            // The last good value is kept so dependents hold their geometry
            // while an expression is being fixed.
            value: current.get(&id).copied().unwrap_or(0.0),
            error: None,
        });
    }

    let step = apply_step(
        state,
        kb,
        UiToEngine::SetParameters { parameters },
        OnError::Report,
        "Internal",
    )?;

    let evaluated: Vec<Value> = state
        .engine
        .tree
        .parameters
        .iter()
        .map(|p| {
            let mut row = json!({
                "id": p.id,
                "name": p.name,
                "value_mm": if p.error.is_some() { Value::Null } else { json!(p.value) },
            });
            if let Some(error) = &p.error {
                row["error"] = json!(error);
            }
            row
        })
        .collect();

    let mut out = step.delta;
    merge_first(&mut out, json!({ "parameters": evaluated }));
    Ok(out)
}

/// Import a STEP body as a feature (one undo step).
///
/// The engine records `Import` provenance itself, and unlike the app's import
/// this opens no placement dialog — the identity placement stands.
pub(super) fn import_step(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let file_name = args
        .get("file_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let data = args
        .get("step_text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let step = apply_step(
        state,
        kb,
        UiToEngine::ImportStep { file_name, data },
        OnError::from_args(args),
        "FeatureRebuildFailed",
    )?;
    Ok(with_feature_id(step))
}

/// Undo the last feature-level step in the document (the agent's or the
/// user's).
pub(super) fn undo(state: &mut EngineState, kb: &mut dyn KernelBundle) -> Answer {
    let step = apply_step(state, kb, UiToEngine::Undo, OnError::Report, "Internal")?;
    Ok(step.delta)
}

/// Redo the last undone feature-level step.
pub(super) fn redo(state: &mut EngineState, kb: &mut dyn KernelBundle) -> Answer {
    let step = apply_step(state, kb, UiToEngine::Redo, OnError::Report, "Internal")?;
    Ok(step.delta)
}

/// A delta with the step's feature id in front of it, as the tools that
/// create a feature answer (`{feature_id, ...delta}`).
fn with_feature_id(step: Step) -> Value {
    let mut out = step.delta;
    merge_first(&mut out, json!({ "feature_id": step.feature_id }));
    out
}

/// Insert `extra`'s entries into `target`. The result is one flat object, as
/// the JS spread `{...out, ...delta}` produced.
fn merge_first(target: &mut Value, extra: Value) {
    let (Some(target), Value::Object(extra)) = (target.as_object_mut(), extra) else {
        return;
    };
    for (key, value) in extra {
        target.insert(key, value);
    }
}
