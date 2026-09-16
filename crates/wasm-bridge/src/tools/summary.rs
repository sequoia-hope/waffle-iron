//! `model_summary` — the read-only overview of the open Part
//! (`specs/waffle_mcp_server.md` §2.5, I14).
//!
//! A pure function of [`EngineState`]: the same state always yields the same
//! structured content (I14 determinism), so a host and the page answer alike.
//!
//! Ported from `app/src/lib/agent/summary.js`, whose input was the store's
//! MIRROR of `ModelUpdated`. Two places where the mirror is not the engine's
//! own shape, and where reading the engine naively would diverge:
//!
//! - **Warnings are deduplicated.** The store keeps them in a `Set`
//!   (`store.svelte.js` `lastRebuildWarnings`), so a warning the engine emits
//!   twice reaches the agent once, in first-seen order.
//! - **A body with no tessellated mesh is not a body.** The worker skips any
//!   body whose vertex buffer is empty (`worker.js` `collectBodies`), so it
//!   never reaches `getBodies()` and must not reach this list either.
//!
//! Ghost bodies of an edit context belong to other parts and are excluded, as
//! they are in the store.

use serde_json::{json, Map, Value};

use crate::engine_state::EngineState;
use crate::messages::PartConnectorInfo;
use crate::tools::{rendered_bodies, Answer};

pub(super) fn model_summary(state: &EngineState) -> Answer {
    Ok(summarize(state))
}

/// The structured content of one `model_summary` call.
fn summarize(state: &EngineState) -> Value {
    let tree = &state.engine.tree;

    // Feature id → its rebuild error. A map, like the store's: if the engine
    // reported an id twice, the last message is the one the agent sees.
    let mut errors: Map<String, Value> = Map::new();
    for (id, message) in &state.engine.errors {
        errors.insert(id.to_string(), json!(message));
    }

    let features: Vec<Value> = tree
        .features
        .iter()
        .map(|f| {
            let mut out = json!({
                "id": f.id,
                "name": f.name,
                "kind": operation_kind(&f.operation),
                "suppressed": f.suppressed,
                // Only the origin: `Provenance.at` is a timestamp and would
                // break I14 determinism.
                "provenance": tree
                    .provenance
                    .get(&f.id)
                    .map(|p| json!(p.origin))
                    .unwrap_or_else(|| json!({ "type": "User" })),
            });
            if let Some(message) = errors.get(&f.id.to_string()) {
                out["error"] = message.clone();
            }
            out
        })
        .collect();

    json!({
        "document_name": state.project_name(),
        "features": features,
        "rollback_index": tree.active_index,
        "bodies": bodies(state),
        "errors": error_rows(state, &errors),
        "warnings": warnings(state),
        "parameters": parameters(state),
        "connectors": connectors(state),
    })
}

/// The `Operation`'s serde tag, as the store's `f.operation?.type` reads it.
fn operation_kind(operation: &feature_engine::types::Operation) -> String {
    serde_json::to_value(operation)
        .ok()
        .and_then(|v| v.get("type").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| "Unknown".to_string())
}

/// Rebuild errors in tree order; errors naming an id the tree does not hold
/// (should not happen) come last, sorted — reported, never dropped.
fn error_rows(state: &EngineState, errors: &Map<String, Value>) -> Vec<Value> {
    let mut rows = Vec::new();
    let mut in_tree = std::collections::HashSet::new();
    for feature in &state.engine.tree.features {
        let id = feature.id.to_string();
        in_tree.insert(id.clone());
        if let Some(message) = errors.get(&id) {
            rows.push(json!({ "feature_id": id, "message": message }));
        }
    }
    let mut orphans: Vec<&String> = errors.keys().filter(|id| !in_tree.contains(*id)).collect();
    orphans.sort();
    for id in orphans {
        rows.push(json!({ "feature_id": id, "message": errors[id] }));
    }
    rows
}

/// The solid bodies the viewport shows for the open Part.
fn bodies(state: &EngineState) -> Vec<Value> {
    rendered_bodies(state)
        .into_iter()
        .map(|meta| {
            json!({
                "body_id": meta.get("bodyId").cloned().unwrap_or(Value::Null),
                "name": meta.get("name").cloned().unwrap_or(Value::Null),
                "feature_id": meta.get("featureId").cloned().unwrap_or(Value::Null),
            })
        })
        .collect()
}

/// Rebuild warnings, verbatim, deduplicated in first-seen order (the store
/// holds them in a `Set`).
fn warnings(state: &EngineState) -> Vec<&str> {
    let mut seen = std::collections::HashSet::new();
    state
        .engine
        .warnings
        .iter()
        .filter(|w| seen.insert(w.as_str()))
        .map(String::as_str)
        .collect()
}

/// Design parameters as last evaluated (mm-space).
fn parameters(state: &EngineState) -> Vec<Value> {
    state
        .engine
        .tree
        .parameters
        .iter()
        .map(|p| {
            let mut row = json!({
                "id": p.id,
                "name": p.name,
                "expression": p.expression,
                "value_mm": p.value,
            });
            if let Some(error) = &p.error {
                row["error"] = json!(error);
            }
            row
        })
        .collect()
}

/// The part's named mate connectors, in part coordinates — the same rows
/// `ModelUpdated.connectors` carries, so the agent and the viewport agree. A
/// connector whose frame has no basis is not reported (it is in `errors`).
fn connectors(state: &EngineState) -> Vec<Value> {
    state
        .engine
        .connectors
        .iter()
        .filter_map(|pc| {
            let info = PartConnectorInfo::new(
                pc,
                Vec::new(),
                &feature_engine::assembly::Transform::identity(),
            )?;
            Some(json!({
                "feature_id": info.feature_id,
                "name": info.name,
                "kind": info.kind,
                "origin_m": info.origin,
                "z_axis": info.z_axis,
                "x_axis": info.x_axis,
            }))
        })
        .collect()
}
