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

use feature_engine::expr::Dimension;
use feature_engine::types::{DesignParameter, Operation, Provenance, ProvenanceOrigin};
use modeling_ops::KernelBundle;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::engine_state::EngineState;
use crate::messages::{EngineToUi, UiToEngine};
use crate::tools::{require_body, require_feature, Answer, ToolFailure};

/// Fillet, chamfer and shell are deferred project-wide (A5, I11).
const DEFERRED: &[&str] = &["Fillet", "Chamfer", "Shell"];

/// Operation kinds an agent may author through `feature_add` / `feature_edit`.
const AUTHORABLE: &[&str] = &[
    "Sketch",
    "Sketch3d",
    "Extrude",
    "Revolve",
    "BooleanCombine",
    "DatumPlane",
    "MateConnector",
    "PatternCircular",
    "PatternLinear",
    "PatternMirror",
    "Pipe",
    "Sweep",
    "Script",
    "UnionAll",
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
pub(super) struct Snapshot {
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

    /// Feature definitions by id, BORROWED — with [`Snapshot::origin`], the
    /// pair `features_changed` compares (JS `featureRecord`).
    ///
    /// Comparing that record per id used to mean scanning the feature array
    /// and cloning twice, inside a loop over every common id — quadratic in
    /// the size of the tab (docs/notes/eiffel/FEATURE_NOTES.md §0). Built
    /// once, it is a lookup, and nothing is cloned.
    fn feature_index(&self) -> HashMap<&str, &Value> {
        self.tree
            .get("features")
            .and_then(Value::as_array)
            .map(|features| {
                features
                    .iter()
                    .filter_map(|f| f.get("id").and_then(Value::as_str).map(|id| (id, f)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A feature's provenance origin, defaulting to `User` where the table has
    /// no record — the default the JS `featureRecord` reported.
    fn origin(&self, id: &str) -> Value {
        self.tree
            .get("provenance")
            .and_then(|table| table.get(id))
            .and_then(|record| record.get("origin"))
            .cloned()
            .unwrap_or_else(|| json!({ "type": "User" }))
    }
}

/// The document as it stands (JS `snapshotNow`).
pub(super) fn snapshot(state: &EngineState) -> Snapshot {
    Snapshot {
        tree: serde_json::to_value(&state.engine.tree).unwrap_or(Value::Null),
        errors: state
            .engine
            .errors
            .iter()
            .map(|(id, message)| (id.to_string(), message.clone()))
            .collect(),
        body_ids: crate::tools::rendered_body_ids(state),
    }
}

/// Whether two snapshots hold the same document model (JS `sameModel`).
pub(super) fn same_model(a: &Snapshot, b: &Snapshot) -> bool {
    a.tree == b.tree
}

/// Features whose rebuild error is new or changed, in `after`'s tree order
/// (errors for ids outside the tree last, sorted). An error that merely
/// persists is not "new" (JS `newlyErroring`).
pub(super) fn newly_erroring(before: &Snapshot, after: &Snapshot) -> Vec<(String, String)> {
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
pub(super) fn model_delta(
    before: &Snapshot,
    after: &Snapshot,
    typed_errors: &[feature_engine::types::FeatureError],
    warnings: &[String],
) -> Value {
    let before_ids = before.feature_ids();
    let after_ids = after.feature_ids();

    // Membership through sets, and the feature definitions through an index:
    // every `contains` below was a scan of the id list, and every
    // the feature record a scan of the feature array plus two clones, each
    // inside a loop over the ids — O(N^2) on every authoring call
    // (docs/notes/eiffel/FEATURE_NOTES.md §0).
    let before_set: HashSet<&str> = before_ids.iter().map(String::as_str).collect();
    let after_set: HashSet<&str> = after_ids.iter().map(String::as_str).collect();
    let before_features = before.feature_index();
    let after_features = after.feature_index();
    let before_bodies: HashSet<&str> = before.body_ids.iter().map(String::as_str).collect();
    let after_bodies: HashSet<&str> = after.body_ids.iter().map(String::as_str).collect();

    let common: Vec<String> = after_ids
        .iter()
        .filter(|id| before_set.contains(id.as_str()))
        .cloned()
        .collect();
    let common_before: Vec<String> = before_ids
        .iter()
        .filter(|id| after_set.contains(id.as_str()))
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
            .filter(|id| !before_set.contains(id.as_str()))
            .collect::<Vec<_>>(),
        "features_changed": common
            .iter()
            .filter(|id| {
                before_features.get(id.as_str()) != after_features.get(id.as_str())
                    || before.origin(id) != after.origin(id)
            })
            .collect::<Vec<_>>(),
        "features_removed": before_ids
            .iter()
            .filter(|id| !after_set.contains(id.as_str()))
            .collect::<Vec<_>>(),
        "order_changed": common
            .iter()
            .enumerate()
            .any(|(i, id)| common_before.get(i) != Some(id)),
        "bodies_added": after
            .body_ids
            .iter()
            .filter(|id| !before_bodies.contains(id.as_str()))
            .collect::<Vec<_>>(),
        "bodies_removed": before
            .body_ids
            .iter()
            .filter(|id| !after_bodies.contains(id.as_str()))
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
    let body_id = require_body(
        state,
        args.get("body_id").and_then(Value::as_str).unwrap_or(""),
    )?;
    let new_name = args
        .get("new_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    // A body's display name and an entity name share ONE namespace (N1,
    // `specs/agent_mechanical_design.md` §5.2) — `entity_name` refuses a name
    // a body already holds, so this is the other direction of the same rule.
    // Without it one string would resolve to the entity through a
    // `{"type":"name"}` operand and to the body through `require_body`.
    if state.engine.tree.names.contains_key(&new_name) {
        return Err(ToolFailure::new(
            "NameTaken",
            format!("\"{new_name}\" is already an entity name in this document."),
            json!({ "name": new_name, "taken_by": "an entity name" }),
        ));
    }
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

/// Resolve a `{id?, name?}` row against the current table, by PARSED id
/// first and then by name. The id is parsed leniently (`Uuid::parse_str`
/// accepts braces and upper case), so a non-canonical spelling that still
/// names a parameter finds it.
fn find_parameter(table: &[DesignParameter], row: &Value) -> Option<usize> {
    if let Some(id) = row
        .get("id")
        .and_then(Value::as_str)
        .and_then(|text| Uuid::parse_str(text).ok())
    {
        if let Some(i) = table.iter().position(|p| p.id == id) {
            return Some(i);
        }
    }
    let name = row.get("name").and_then(Value::as_str)?;
    table.iter().position(|p| p.name == name)
}

/// A `delete` entry is a name or an id.
fn find_parameter_by_key(table: &[DesignParameter], key: &str) -> Option<usize> {
    if let Ok(id) = Uuid::parse_str(key) {
        if let Some(i) = table.iter().position(|p| p.id == id) {
            return Some(i);
        }
    }
    table.iter().position(|p| p.name == key)
}

/// Read a row's declared unit, distinguishing "absent" from "explicitly
/// null". An absent `unit` KEEPS what the parameter has — in BOTH modes: an
/// agent that sets one expression, or re-sends a table it did not read the
/// sidecars of, must not silently strip a declared dimension it never
/// mentioned, because `unit` changes which fields accept the value and so
/// changes what the document refuses. An explicit `null` clears it. The same
/// rule governs `comment`.
fn declared_unit(
    row: &Value,
    current: Option<Dimension>,
) -> Result<Option<Dimension>, ToolFailure> {
    match row.get("unit") {
        None => Ok(current),
        Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value::<Dimension>(v.clone())
            .map(Some)
            .map_err(|_| {
                ToolFailure::new(
                    "InvalidArguments",
                    format!("`unit` must be one of Length, Angle, Count, Ratio (got {v})"),
                    json!({ "schema_path": "/parameters/unit" }),
                )
            }),
    }
}

/// Write the design-parameter table and rebuild (one undo step).
///
/// Two modes. By default `parameters` REPLACES the table, which is what the
/// panel sends and what every pre-P5 caller sent. With `merge: true` the
/// rows are applied over the current table and `delete` removes parameters,
/// so an agent can set one value without re-sending twenty
/// (`specs/agent_mechanical_design.md` §6 P5).
///
/// In both modes an incoming row whose ID matches a parameter with a
/// DIFFERENT name is a rename, and its dependents follow: every other
/// parameter's expression and every expression field on the tree is
/// rewritten through the AST (`params::rename_parameter`). The id is the
/// identity — that is what "keep a parameter's id to preserve it" has always
/// meant — so a changed name on a kept id cannot be anything else.
///
/// A failing expression is reported per parameter, not rolled back, so the
/// answer carries the table as the rebuild evaluated it. A request that
/// cannot be APPLIED at all (a delete of a parameter something still reads,
/// a rename onto a name already taken, an invalid name) is refused whole,
/// with nothing written: half a rename is worse than none.
pub(super) fn parameters_set(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let scope = args
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("tab")
        .to_string();
    // An instance's overrides are MAGNITUDES, not expression rows (P2), so
    // they take a different payload and a different path. Routed from here
    // rather than from a separate tool name so that "set a parameter" is one
    // tool whichever scope the caller means.
    if scope == "instance" {
        return parameters_set_instance(state, kb, args, merge_flag(args));
    }
    if scope != "tab" && scope != "document" {
        return Err(ToolFailure::new(
            "InvalidArguments",
            format!("scope: unknown scope `{scope}`; expected `tab`, `document` or `instance`."),
            json!({ "schema_path": "/scope",
                    "expected": ["tab", "document", "instance"] }),
        ));
    }
    let document_scope = scope == "document";
    let merge = merge_flag(args);
    let table = if document_scope {
        state.engine.document_parameters.clone()
    } else {
        state.engine.tree.parameters.clone()
    };
    let deletes = args
        .get("delete")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !merge && !deletes.is_empty() {
        return Err(ToolFailure::new(
            "InvalidArguments",
            "`delete` needs `merge: true`. Without merge, `parameters` is the \
             complete table and omitting a parameter already removes it."
                .to_string(),
            json!({ "schema_path": "/delete" }),
        ));
    }
    if !merge && args.get("parameters").is_none() {
        return Err(ToolFailure::new(
            "InvalidArguments",
            "`parameters` is required: without `merge: true` it is the complete \
             table that replaces the current one."
                .to_string(),
            json!({ "schema_path": "/parameters" }),
        ));
    }

    let rows = args
        .get("parameters")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    // Build the new table.
    let mut parameters: Vec<DesignParameter> = if merge { table.clone() } else { Vec::new() };
    let mut renames: Vec<(String, String)> = Vec::new();
    for row in &rows {
        let existing = find_parameter(&table, row);
        let base = existing.map(|i| &table[i]);
        let name = match row.get("name").and_then(Value::as_str) {
            Some(name) => name.to_string(),
            // In merge mode a row may name only the id and the expression.
            None => match base {
                Some(p) => p.name.clone(),
                None => String::new(),
            },
        };
        let expression = match row.get("expression").and_then(Value::as_str) {
            Some(e) => e.to_string(),
            None => match base {
                Some(p) => p.expression.clone(),
                None => String::new(),
            },
        };
        // A rename: the SAME parameter under a different name. Validate the
        // new name here rather than letting the table report a duplicate —
        // a rename that lands on a taken name would rewrite every dependent
        // onto a parameter that is not the one they meant.
        if let Some(p) = base {
            if p.name != name {
                if let Err(why) = feature_engine::expr::validate_name(&name) {
                    return Err(ToolFailure::new(
                        "InvalidParameterName",
                        format!("Cannot rename '{}' to '{name}': {why}.", p.name),
                        json!({ "parameter": p.name, "new_name": name }),
                    ));
                }
                if table
                    .iter()
                    .any(|other| other.id != p.id && other.name == name)
                {
                    return Err(ToolFailure::new(
                        "ParameterNameTaken",
                        format!(
                            "Cannot rename '{}' to '{name}': another parameter is \
                             already called '{name}'.",
                            p.name
                        ),
                        json!({ "parameter": p.name, "new_name": name }),
                    ));
                }
                if document_scope {
                    // The rewrite a rename needs reaches every TAB's
                    // expressions, and this call holds only the open tab's
                    // tree — the others live in the session. Rewriting one
                    // tab and not the rest is the half-rename
                    // `ParameterNameTaken` exists to prevent, so the rename
                    // is refused rather than half-applied. Repointing by
                    // hand is three loud steps: add the new name, change the
                    // readers, delete the old.
                    return Err(ToolFailure::new(
                        "ParameterRenameNotSupported",
                        format!(
                            "Cannot rename the document parameter '{}' to '{name}': a \
                             document parameter is read by every tab, and this call \
                             can only rewrite the open one. Nothing was changed. Add \
                             '{name}' as a new document parameter, repoint the \
                             expressions that read '{}', then delete '{}'.",
                            p.name, p.name, p.name
                        ),
                        json!({ "parameter": p.name, "new_name": name,
                                "scope": "document" }),
                    ));
                }
                renames.push((p.name.clone(), name.clone()));
            }
        }
        let built = DesignParameter {
            // A row with no id (or an id naming nothing) is a NEW parameter.
            id: base.map(|p| p.id).unwrap_or_else(Uuid::new_v4),
            name,
            expression,
            // The last good value is kept so dependents hold their geometry
            // while an expression is being fixed.
            value: base.map(|p| p.value).unwrap_or(0.0),
            error: None,
            // A declared dimension (P1): the expression must produce it, and
            // every field that reads this parameter is checked against it.
            // Absent leaves the parameter a plain number, as before.
            unit: declared_unit(row, base.and_then(|p| p.unit))?,
            // Same rule as `unit`: absent keeps, `null` clears. Before this
            // the two disagreed — a full-table send kept a declared `unit`
            // it did not mention and dropped the `comment` beside it, which
            // no caller can predict from one schema.
            comment: match row.get("comment") {
                None => base.and_then(|p| p.comment.clone()),
                Some(Value::String(c)) => Some(c.clone()),
                _ => None,
            },
            // Derived state: the rebuild this step triggers refills every
            // parameter's evaluated dimension.
            tag: None,
        };
        match existing.filter(|_| merge) {
            // Merge keeps the table's ORDER: an edit must not reshuffle the
            // panel under the person reading it.
            Some(_) => {
                let at = parameters
                    .iter()
                    .position(|p| p.id == built.id)
                    .expect("the merge table starts as the current one");
                parameters[at] = built;
            }
            None => parameters.push(built),
        }
    }

    // Deletes, after the sets: a parameter this very call replaced may be
    // named by id in both lists, and the delete is then the later word.
    let mut removed: Vec<String> = Vec::new();
    for key in &deletes {
        let Some(key) = key.as_str() else {
            return Err(ToolFailure::new(
                "InvalidArguments",
                "`delete` entries must be parameter names or ids.".to_string(),
                json!({ "schema_path": "/delete" }),
            ));
        };
        let Some(at) = find_parameter_by_key(&parameters, key) else {
            return Err(ToolFailure::new(
                "ParameterNotFound",
                format!("No parameter named '{key}' to delete."),
                json!({ "parameter": key }),
            ));
        };
        removed.push(parameters[at].name.clone());
        parameters.remove(at);
    }

    // Every rename's new name must name exactly ONE parameter in the table
    // this call produces. The per-row check above compares each new name
    // against the table as it WAS, so it cannot see two rows renamed onto the
    // same name in one call, or a rename onto a name a NEW row in the same
    // call also takes. Letting either through is the harm `ParameterNameTaken`
    // exists to prevent: the dependents would be rewritten onto whichever
    // duplicate the table resolves first, which is not the parameter they
    // meant, and the geometry would move with no error on any feature.
    // (Measured before this check: two rows renamed to `x` left a dependent
    // of the second silently reading the first.)
    for (from, to) in &renames {
        let targets = parameters.iter().filter(|p| p.name == *to).count();
        if targets != 1 {
            let why = if targets == 0 {
                format!("this call also removes '{to}'")
            } else {
                format!("this call would leave {targets} parameters called '{to}'")
            };
            return Err(ToolFailure::new(
                "ParameterNameTaken",
                format!("Cannot rename '{from}' to '{to}': {why}. Nothing was changed."),
                json!({ "parameter": from, "new_name": to, "targets": targets }),
            ));
        }
    }

    // A delete that leaves a reader behind is refused by NAME, not left to
    // break at the next rebuild: the dependents are the answer the caller
    // needs, and a loud per-feature "unknown variable" an hour later is not.
    if !removed.is_empty() {
        let mut blocked: Vec<Value> = Vec::new();
        // A DOCUMENT parameter is read by every tab, so the check walks every
        // part tab's tree, not just the open one. Checking only the open tab
        // would let a delete through that breaks a Part the caller is not
        // looking at — the exact harm this refusal exists to prevent, moved
        // one tab away.
        let field_uses = if document_scope {
            let mut uses = Vec::new();
            for (tab_id, mut tree) in state.session.part_trees(&state.engine) {
                let tab_name = state
                    .session
                    .tab(&tab_id)
                    .map(|t| t.name.clone())
                    .unwrap_or(tab_id);
                uses.extend(
                    feature_engine::params::field_uses(&mut tree)
                        .into_iter()
                        .map(|mut u| {
                            u.feature_name = format!("{tab_name} › {}", u.feature_name);
                            u
                        }),
                );
            }
            uses
        } else {
            feature_engine::params::field_uses(&mut state.engine.tree)
        };
        for name in &removed {
            let mut readers: Vec<String> = parameters
                .iter()
                .filter(|p| {
                    feature_engine::expr::dependencies(&p.expression)
                        .is_some_and(|ids| ids.contains(name))
                })
                .map(|p| format!("parameter '{}'", p.name))
                .collect();
            readers.extend(
                field_uses
                    .iter()
                    .filter(|u| u.reads.contains(name))
                    .map(|u| format!("{} {}", u.feature_name, u.field)),
            );
            if !readers.is_empty() {
                blocked.push(json!({ "parameter": name, "dependents": readers }));
            }
        }
        if !blocked.is_empty() {
            let detail: Vec<String> = blocked
                .iter()
                .map(|b| {
                    format!(
                        "'{}' is read by {}",
                        b["parameter"].as_str().unwrap_or(""),
                        b["dependents"]
                            .as_array()
                            .map(|d| d
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(", "))
                            .unwrap_or_default()
                    )
                })
                .collect();
            return Err(ToolFailure::new(
                "ParameterInUse",
                format!(
                    "Nothing was changed. {}. Change or remove the dependents first, \
                     or rename instead of deleting.",
                    detail.join("; ")
                ),
                json!({ "blocked": blocked }),
            ));
        }
    }

    let message = if document_scope {
        UiToEngine::SetDocumentParameters { parameters }
    } else {
        UiToEngine::SetParameters {
            parameters,
            renames,
        }
    };
    let step = apply_step(state, kb, message, OnError::Report, "Internal")?;

    let answered = if document_scope {
        state.engine.document_parameters.clone()
    } else {
        state.engine.tree.parameters.clone()
    };
    let evaluated: Vec<Value> = answered
        .iter()
        .map(|p| {
            let mut row = json!({
                "id": p.id,
                "name": p.name,
                // Echoed (P5): after a rename or a merge, the expression the
                // table now holds is not the one the caller sent.
                "expression": p.expression,
                "value_mm": if p.error.is_some() { Value::Null } else { json!(p.value) },
            });
            if let Some(unit) = p.unit {
                row["unit"] = json!(unit);
            }
            if let Some(comment) = &p.comment {
                row["comment"] = json!(comment);
            }
            if let Some(error) = &p.error {
                row["error"] = json!(error);
            }
            row
        })
        .collect();

    let mut out = step.delta;
    merge_first(&mut out, json!({ "scope": scope, "parameters": evaluated }));
    Ok(out)
}

/// `merge`, read the same way wherever `parameters_set` needs it.
fn merge_flag(args: &Value) -> bool {
    args.get("merge").and_then(Value::as_bool).unwrap_or(false)
}

/// `parameters_set {scope: "instance", instance_id, overrides}` (P2): the
/// magnitudes one placed instance pins on its part.
///
/// Delegates to `instance_edit`, which is the one place an instance is
/// written: an override is a field of the instance, and a second writer for
/// it would be a second set of preconditions (the derived-instance refusal,
/// the assembly commit, the re-evaluation) to keep in step. The argument is
/// spelled `overrides` here and `parameter_overrides` there because this
/// call's scope already said what is being set.
fn parameters_set_instance(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
    merge: bool,
) -> Answer {
    let instance_id = args
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ToolFailure::new(
                "InvalidArguments",
                "instance_id is required for scope `instance`.",
                json!({ "schema_path": "/instance_id" }),
            )
        })?;
    let overrides = args
        .get("overrides")
        .or_else(|| args.get("parameter_overrides"))
        .ok_or_else(|| {
            ToolFailure::new(
                "InvalidArguments",
                "`overrides` is required for scope `instance`: an object of \
                 {name: number} (working-space magnitudes), or null to clear \
                 every override."
                    .to_string(),
                json!({ "schema_path": "/overrides" }),
            )
        })?;
    crate::tools::assembly::instance_edit(
        state,
        kb,
        &json!({
            "instance_id": instance_id,
            "parameter_overrides": overrides,
            "merge": merge,
        }),
    )
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

/// Link a KiCad board (`specs/kicad_board_link.md` §2.4): the `.kicad_pcb`
/// text becomes a `KicadPcb` source (linked when `locator` is given, else
/// embedded), a Board Part tab, placeholder Parts and a Board assembly tab;
/// the Board tab is opened. Answers the tab ids and the delta.
pub(super) fn kicad_link(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let file_name = args
        .get("file_name")
        .and_then(Value::as_str)
        .unwrap_or("board.kicad_pcb")
        .to_string();
    let data = args
        .get("pcb_text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if data.is_empty() {
        return Err(ToolFailure::new(
            "InvalidArguments",
            "pcb_text is required.",
            json!({ "reason": "pcb_text is required." }),
        ));
    }
    let parse_locator = |key: &str| -> Result<Option<file_format::Locator>, ToolFailure> {
        match args.get(key) {
            Some(l) if !l.is_null() => serde_json::from_value(l.clone()).map(Some).map_err(|e| {
                ToolFailure::new(
                    "InvalidArguments",
                    format!("{key}: {e}"),
                    json!({ "reason": e.to_string() }),
                )
            }),
            _ => Ok(None),
        }
    };
    // The board STEP beside it (C3): its products are the component models.
    let board_step = match args.get("step_text").and_then(Value::as_str) {
        Some(text) if !text.is_empty() => Some(crate::messages::BoardStepData {
            file_name: args
                .get("step_file_name")
                .and_then(Value::as_str)
                .unwrap_or("board.step")
                .to_string(),
            data: text.to_string(),
            locator: parse_locator("step_locator")?,
            resolved_commit: args
                .get("step_resolved_commit")
                .and_then(Value::as_str)
                .map(str::to_string),
        }),
        _ => None,
    };
    let msg = match parse_locator("locator")? {
        Some(locator) => UiToEngine::LinkKicadFromLocator {
            file_name,
            locator,
            data,
            resolved_commit: args
                .get("resolved_commit")
                .and_then(Value::as_str)
                .map(str::to_string),
            board_step,
        },
        None => UiToEngine::ImportKicad {
            file_name,
            data,
            board_step,
        },
    };
    let step = apply_step(
        state,
        kb,
        msg,
        OnError::from_args(args),
        "FeatureRebuildFailed",
    )?;
    let mut out = step.delta;
    if let Some(rec) = state.kicad_boards.last() {
        out["source_id"] = json!(rec.source_id);
        out["board_tab"] = json!(rec.board_tab);
        out["assembly_tab"] = json!(rec.assembly_tab);
        out["placeholder_tabs"] = json!(rec.placeholder_tabs);
        out["board"] = json!(rec.board);
        out["component_count"] = json!(rec.components.len());
        out["board_step_source_id"] = json!(rec.board_step);
    }
    Ok(out)
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
pub(super) fn with_feature_id(step: Step) -> Value {
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
