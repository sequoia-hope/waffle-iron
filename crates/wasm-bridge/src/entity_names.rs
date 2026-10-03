//! Entity names at the bridge (N1, `specs/agent_mechanical_design.md` §5.2).
//!
//! The table and its rules live in `feature_engine::names`; what is here is
//! the part that needs the live kernel and the rendered-body view:
//!
//! - turning an `EntityTarget` (a reference, a body id, or an existing name)
//!   into the thing a name points at,
//! - minting a `NamedRef` over the entity's persistent id,
//! - answering "what is this body called right now", which only the render
//!   layer knows (a body with no override is named after its producing feature
//!   plus an ordinal among that feature's RENDERED bodies),
//! - and listing every name with whether it still resolves.

use std::collections::HashMap;

use feature_engine::names::{self, NamedRef};
use feature_engine::types::{FeatureTree, Provenance};
use modeling_ops::KernelBundle;
use serde_json::Value;
use waffle_types::kernel::KernelId;
use waffle_types::{Anchor, GeomRef, TopoKind};

use crate::engine_state::EngineState;
use crate::messages::{EntityTarget, ListedName};

/// What an [`EntityTarget`] named.
// The same call `MeasureOperand` makes: one of these exists per call, lives
// on the stack, and boxing the reference arm would buy a pointer chase.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Target {
    /// A face, edge or vertex, by the reference that identifies it.
    Entity(GeomRef),
    /// A whole body, by its persistent id.
    Body(String),
}

/// The display name of every rendered body, by persistent body id — the names
/// `model_summary` reports, which are the names a dotted entity name's first
/// segment must match.
pub(crate) fn body_display_names(state: &EngineState) -> HashMap<String, String> {
    crate::tools::rendered_bodies(state)
        .iter()
        .filter_map(|b| {
            let id = b.get("bodyId").and_then(Value::as_str)?.to_string();
            let name = b.get("name").and_then(Value::as_str)?.to_string();
            Some((id, name))
        })
        .collect()
}

/// The persistent body id a reference's anchor names, if it anchors a feature
/// output at all (a `Datum` anchor names no body).
pub(crate) fn body_of_ref(geom_ref: &GeomRef) -> Option<String> {
    match &geom_ref.anchor {
        Anchor::FeatureOutput {
            feature_id,
            output_key,
        } => Some(FeatureTree::body_id(*feature_id, output_key)),
        Anchor::Datum { .. } => None,
    }
}

/// Resolve an [`EntityTarget`] to what a name would point at.
///
/// A `Name` target is looked up in the name table first and the body display
/// names second, so an agent can re-label either through the one argument.
pub(crate) fn resolve_target(state: &EngineState, target: &EntityTarget) -> Result<Target, String> {
    match target {
        EntityTarget::Entity { geom_ref } => Ok(Target::Entity(geom_ref.clone())),
        EntityTarget::Body { body_id } => Ok(Target::Body(body_id.clone())),
        EntityTarget::Name { name } => {
            if let Some(named) = state.engine.tree.named_ref(name) {
                return Ok(Target::Entity(named.target.clone()));
            }
            body_display_names(state)
                .into_iter()
                .find(|(_, display)| display == name)
                .map(|(id, _)| Target::Body(id))
                .ok_or_else(|| format!("this document has no entity or body named \"{name}\""))
        }
    }
}

/// Build the [`NamedRef`] for a reference, storing the entity's persistent id.
pub(crate) fn mint(
    state: &EngineState,
    kb: &mut dyn KernelBundle,
    geom_ref: &GeomRef,
    created: Provenance,
) -> Result<NamedRef, String> {
    names::mint(
        geom_ref,
        &state.engine.feature_results,
        kb.as_introspect(),
        created,
    )
    .map_err(|e| e.to_string())
}

/// Whether a name is already in use — by an entity name or by a body's display
/// name. Both share one namespace, because a body name is the first segment of
/// every dotted entity name in it.
pub(crate) fn taken_by(state: &EngineState, name: &str) -> Option<&'static str> {
    if state.engine.tree.names.contains_key(name) {
        return Some("an entity name");
    }
    if body_display_names(state).values().any(|n| n == name) {
        return Some("a body's display name");
    }
    None
}

/// Every name the document holds, in name order, each with whether it resolves.
///
/// `body_id` limits the answer to one body: its own display name, plus every
/// entity name whose reference anchors that body.
pub(crate) fn list(
    state: &EngineState,
    kb: &mut dyn KernelBundle,
    body_id: Option<&str>,
) -> Vec<ListedName> {
    let display = body_display_names(state);
    let mut out: Vec<ListedName> = Vec::new();

    // Body display names first — they are the namespace the dotted entity
    // names sit in, so a list that omitted them would not show why a segment
    // was refused.
    for (id, name) in &display {
        if body_id.is_some_and(|want| want != id) {
            continue;
        }
        out.push(ListedName {
            name: name.clone(),
            kind: TopoKind::Solid,
            geom_ref: None,
            body_id: Some(id.clone()),
            body: Some(name.clone()),
            resolves: true,
            resolved_by: None,
            resolved_via: None,
            rebound: false,
            lost_identity: None,
            refusal: None,
            warnings: Vec::new(),
            created: None,
        });
    }

    for (name, named) in &state.engine.tree.names {
        let owner = body_of_ref(&named.target);
        if let Some(want) = body_id {
            if owner.as_deref() != Some(want) {
                continue;
            }
        }
        // N2 §5.3 items 2 and 4: the rung that answered and, on a refusal, its
        // classification — so an agent reads the state of each reference off
        // fields instead of out of the warning prose.
        let resolution = names::resolve(named, &state.engine.feature_results, kb.as_introspect());
        #[allow(clippy::type_complexity)]
        let (resolves, resolved_by, resolved_via, rebound, lost_identity, refusal, warnings): (
            bool,
            Option<names::ResolvedBy>,
            Option<feature_engine::resolve::ResolvedVia>,
            bool,
            Option<feature_engine::types::ResolutionReason>,
            Option<feature_engine::types::ResolutionReason>,
            Vec<String>,
        ) = match resolution {
            Ok(r) => (
                true,
                Some(r.resolved_by),
                Some(r.via),
                r.rebound,
                r.lost_identity,
                None,
                r.warnings,
            ),
            Err(e) => (
                false,
                None,
                None,
                false,
                None,
                e.resolution_reason().cloned(),
                vec![e.to_string()],
            ),
        };
        out.push(ListedName {
            name: name.clone(),
            kind: named.kind,
            geom_ref: Some(named.target.clone()),
            body: owner.as_ref().and_then(|id| display.get(id).cloned()),
            body_id: owner,
            resolves,
            resolved_by,
            resolved_via,
            rebound,
            lost_identity,
            refusal,
            warnings,
            created: Some(named.created.clone()),
        });
    }

    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The name pointing at each entity of one body, by kernel id and kind — what
/// a result that carries a `GeomRef` needs to also carry its name (§5.2).
///
/// Resolution is per name, and a name that no longer resolves simply is not in
/// the map; nothing is guessed.
pub(crate) fn names_by_entity(
    state: &EngineState,
    kb: &mut dyn KernelBundle,
    body_id: &str,
) -> HashMap<(TopoKind, KernelId), String> {
    let mut out = HashMap::new();
    for (name, named) in &state.engine.tree.names {
        if body_of_ref(&named.target).as_deref() != Some(body_id) {
            continue;
        }
        if let Ok(r) = names::resolve(named, &state.engine.feature_results, kb.as_introspect()) {
            out.insert((named.kind, r.kernel_id), name.clone());
        }
    }
    out
}
