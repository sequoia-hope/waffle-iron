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
        let row = ListedName {
            name: name.clone(),
            kind: named.kind,
            geom_ref: Some(named.target.clone()),
            body: owner.as_ref().and_then(|id| display.get(id).cloned()),
            body_id: owner,
            created: Some(named.created.clone()),
            // Filled in by whichever arm below answers.
            resolves: false,
            resolved_by: None,
            resolved_via: None,
            rebound: false,
            lost_identity: None,
            refusal: None,
            warnings: Vec::new(),
        };
        out.push(
            match names::resolve(named, &state.engine.feature_results, kb.as_introspect()) {
                Ok(r) => ListedName {
                    resolves: true,
                    resolved_by: Some(r.resolved_by),
                    resolved_via: Some(r.via),
                    rebound: r.rebound,
                    lost_identity: r.lost_identity,
                    warnings: r.warnings,
                    ..row
                },
                Err(e) => ListedName {
                    refusal: e.resolution_reason().cloned(),
                    warnings: vec![e.to_string()],
                    ..row
                },
            },
        );
    }

    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// One N1 name bound to one entity, with whatever the resolution had to say
/// about HOW it got there.
#[derive(Debug, Clone, Default)]
pub(crate) struct NameBinding {
    pub name: String,
    /// `names::resolve`'s own warnings, verbatim. Non-empty when the name did
    /// not reach this entity by its persistent id — N1's loud fallback ("the
    /// persistent id is gone; the name resolved through the reference it was
    /// authored with, which rebinds by geometry and may name a different
    /// entity"). A listing that showed the bare name would silence exactly
    /// the case N1 made loud, so every consumer of this map gets the warnings
    /// with it and decides whether to pass them on.
    pub warnings: Vec<String>,
}

/// The name pointing at each entity of one body, by kernel id and kind, with
/// its resolution warnings — what a result that carries a `GeomRef` needs to
/// also carry its name (§5.2).
///
/// Resolution is per name, and a name that no longer resolves at all simply is
/// not in the map; nothing is guessed. `unresolved` collects those names so a
/// caller can say "this body has a name that binds to nothing" rather than
/// leaving the fact invisible.
pub(crate) fn name_bindings(
    state: &EngineState,
    kb: &mut dyn KernelBundle,
    body_id: &str,
) -> (HashMap<(TopoKind, KernelId), NameBinding>, Vec<String>) {
    let mut out = HashMap::new();
    let mut unresolved = Vec::new();
    for (name, named) in &state.engine.tree.names {
        if body_of_ref(&named.target).as_deref() != Some(body_id) {
            continue;
        }
        match names::resolve(named, &state.engine.feature_results, kb.as_introspect()) {
            Ok(r) => {
                out.insert(
                    (named.kind, r.kernel_id),
                    NameBinding {
                        name: name.clone(),
                        warnings: r.warnings,
                    },
                );
            }
            // `tree.names` is a `BTreeMap`, so this list is in name order and
            // the same in every process.
            Err(_) => unresolved.push(name.clone()),
        }
    }
    (out, unresolved)
}

/// [`name_bindings`] with the warnings discarded — the name alone, for the
/// listings whose wire type carries no field for them.
///
/// One resolution path, so the two cannot drift.
pub(crate) fn names_by_entity(
    state: &EngineState,
    kb: &mut dyn KernelBundle,
    body_id: &str,
) -> HashMap<(TopoKind, KernelId), String> {
    name_bindings(state, kb, body_id)
        .0
        .into_iter()
        .map(|(k, b)| (k, b.name))
        .collect()
}
