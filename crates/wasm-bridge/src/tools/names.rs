//! The entity-name tools (N1, `specs/agent_mechanical_design.md` §5.2):
//! `entity_name`, `entity_unname`, `names_list`.
//!
//! The refusals are typed HERE rather than left to the engine, because
//! `engine_call` collapses every engine error into `Internal` (ICR-2): a name
//! that is already taken is an ordinary thing for an agent to run into and
//! must come back as `NameTaken`, not as a broken invariant. The engine still
//! validates everything it is sent — a host that skips this layer cannot store
//! an unchecked name — so the two checks are deliberate, not duplicated logic
//! with one authority.

use modeling_ops::KernelBundle;
use serde_json::{json, Value};

use crate::engine_state::EngineState;
use crate::entity_names::{self, Target};
use crate::messages::{EngineToUi, EntityTarget, UiToEngine};
use crate::tools::author::{agent_provenance, apply_step, OnError};
use crate::tools::{require_body, unexpected, Answer, ToolFailure};

fn invalid_args(reason: impl Into<String>) -> ToolFailure {
    let reason = reason.into();
    ToolFailure::new(
        "InvalidArguments",
        reason.clone(),
        json!({ "reason": reason }),
    )
}

/// The `target` argument: `{"type":"entity","geom_ref":…}`,
/// `{"type":"body","body_id":…}` or `{"type":"name","name":…}`.
fn target_of(args: &Value) -> Result<EntityTarget, ToolFailure> {
    let value = args
        .get("target")
        .ok_or_else(|| invalid_args("target is required."))?;
    serde_json::from_value(value.clone()).map_err(|e| {
        invalid_args(format!(
            "target: {e}. A target is {{\"type\":\"entity\",\"geom_ref\":…}}, \
             {{\"type\":\"body\",\"body_id\":…}} or {{\"type\":\"name\",\"name\":…}}."
        ))
    })
}

fn name_of(args: &Value) -> Result<String, ToolFailure> {
    args.get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| invalid_args("name is required."))
}

/// Give one face, edge, vertex or body a name (one undo step).
pub(super) fn entity_name(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
    context: Option<&Value>,
) -> Answer {
    let name = name_of(args)?;
    let target = target_of(args)?;
    // Every check below reads the RENDERED body list (the namespace, the body
    // a reference belongs to, that body's display name), and a body with no
    // tessellated mesh is not in it. `apply_step` tessellates, but these run
    // before it — without this a dotted name would be refused for a body that
    // simply had not been meshed yet.
    crate::tessellation_runner::tessellate_missing_meshes(state, kb);

    // The grammar first, so a malformed name is refused before anything is
    // resolved and the message says which segment is wrong.
    let path = feature_engine::names::parse_name(&name).map_err(|reason| {
        ToolFailure::new(
            "InvalidName",
            format!("\"{name}\" is not a usable entity name: {reason}"),
            json!({ "name": name, "reason": reason }),
        )
    })?;
    let key = path.key();

    if let Some(holder) = entity_names::taken_by(state, &key) {
        return Err(ToolFailure::new(
            "NameTaken",
            format!("\"{key}\" is already {holder} in this document."),
            json!({ "name": key, "taken_by": holder }),
        ));
    }

    let resolved = entity_names::resolve_target(state, &target).map_err(|reason| {
        ToolFailure::new(
            "ReferenceNotResolved",
            reason.clone(),
            json!({ "reason": reason }),
        )
    })?;

    match resolved {
        // A body's name IS its display name — the mechanism `body_rename`
        // already owns, and the thing a dotted entity name's first segment
        // has to match. Naming one through here does not create a second
        // record of it (§5.2: bodies where not already present).
        Target::Body(body_id) => {
            if path.body.is_some() {
                return Err(ToolFailure::new(
                    "InvalidName",
                    format!(
                        "\"{key}\" is a dotted name, which names an entity inside a body; a body's \
                         own name is one segment."
                    ),
                    json!({ "name": key, "reason": "a body name is one segment" }),
                ));
            }
            let body_id = require_body(state, &body_id)?;
            let step = apply_step(
                state,
                kb,
                UiToEngine::RenameBody {
                    body_id: body_id.clone(),
                    new_name: key.clone(),
                },
                OnError::Report,
                "Internal",
            )?;
            let mut out = step.delta;
            out["name"] = json!(key);
            out["kind"] = json!({ "type": "Solid" });
            out["body_id"] = json!(body_id);
            Ok(out)
        }
        Target::Entity(geom_ref) => {
            // Minting needs the live kernel: the name stores the entity's
            // persistent id, which is a question about the geometry as it
            // stands (D0).
            let named = entity_names::mint(
                state,
                kb,
                &geom_ref,
                agent_provenance(context).expect("agent provenance is always recorded"),
            )
            .map_err(|reason| {
                ToolFailure::new(
                    "ReferenceNotResolved",
                    format!("the reference cannot be named: {reason}"),
                    json!({ "reason": reason }),
                )
            })?;
            let kind = named.kind;
            let body_id = entity_names::body_of_ref(&named.target);

            // The dotted body segment, checked here so a wrong one is an
            // ordinary `InvalidName` rather than the `Internal` an engine
            // refusal becomes. The engine checks it again for a host that
            // skips this layer.
            if let Some(segment) = &path.body {
                let actual = body_id
                    .as_ref()
                    .and_then(|id| entity_names::body_display_names(state).remove(id));
                match actual {
                    Some(actual) if actual == *segment => {}
                    other => {
                        return Err(ToolFailure::new(
                            "InvalidName",
                            match &other {
                                Some(actual) => format!(
                                    "\"{key}\" says this entity is in a body called \
                                     \"{segment}\", but it is in \"{actual}\". Name it \
                                     \"{actual}.{leaf}\" (rename the body first if that is not an \
                                     identifier), or drop the segment.",
                                    leaf = path.leaf
                                ),
                                None => format!(
                                    "\"{key}\" is dotted, but the body this entity belongs to has \
                                     no display name — it is not a rendered body of the open Part."
                                ),
                            },
                            json!({ "name": key, "segment": segment, "body": other }),
                        ));
                    }
                }
            }

            let step = apply_step(
                state,
                kb,
                UiToEngine::SetEntityName {
                    name: key.clone(),
                    named: Box::new(named),
                },
                OnError::Report,
                "Internal",
            )?;
            let stored = state
                .engine
                .tree
                .named_ref(&key)
                .map(|n| serde_json::to_value(&n.target).unwrap_or(Value::Null))
                .unwrap_or(Value::Null);
            let mut out = step.delta;
            out["name"] = json!(key);
            out["kind"] = serde_json::to_value(kind).unwrap_or(Value::Null);
            out["geom_ref"] = stored;
            if let Some(body_id) = body_id {
                out["body_id"] = json!(body_id);
            }
            Ok(out)
        }
    }
}

/// Remove one entity name (one undo step). A body's display name is not an
/// entity name; clear that with `body_rename` and an empty name.
pub(super) fn entity_unname(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let name = name_of(args)?;
    crate::tessellation_runner::tessellate_missing_meshes(state, kb);
    if !state.engine.tree.names.contains_key(&name) {
        let as_body = entity_names::body_display_names(state)
            .values()
            .any(|n| *n == name);
        return Err(ToolFailure::new(
            "NameNotFound",
            if as_body {
                format!(
                    "\"{name}\" is a body's display name, not an entity name; clear it with \
                     body_rename and an empty new_name."
                )
            } else {
                format!("This document has no entity named \"{name}\".")
            },
            json!({ "name": name, "is_body_name": as_body }),
        ));
    }
    let step = apply_step(
        state,
        kb,
        UiToEngine::ClearEntityName { name: name.clone() },
        OnError::Report,
        "Internal",
    )?;
    let mut out = step.delta;
    out["name"] = json!(name);
    Ok(out)
}

/// Every name in the document (or in one body), with whether it still resolves.
pub(super) fn names_list(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let body_id = args
        .get("body_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let body_id = match &body_id {
        Some(id) => Some(require_body(state, id)?),
        None => None,
    };
    let response = crate::tools::engine_call(
        state,
        kb,
        "QueryEntityNames",
        UiToEngine::QueryEntityNames { body_id },
    )?;
    let EngineToUi::EntityNamesListed { names } = &response else {
        return Err(unexpected(
            "QueryEntityNames",
            "EntityNamesListed",
            &response,
        ));
    };
    Ok(json!({ "names": names }))
}
