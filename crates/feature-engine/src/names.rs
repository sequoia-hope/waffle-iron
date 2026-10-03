//! Entity names — user/agent-assigned labels over persistent references
//! (N1 of `specs/agent_mechanical_design.md` §5.2).
//!
//! A name is an **alias, not a second identity system**: it points at a
//! [`GeomRef`] whose selector is a `Selector::Pid` (drawings spec D0) whenever
//! the kernel has an id for the entity, so the name survives a rebuild exactly
//! as well as the pid does and no better. The table itself is a plain
//! `BTreeMap` on the `FeatureTree`, so it saves with the document, orders
//! deterministically, and never participates in geometry.
//!
//! Two properties are worth stating because they are deliberate:
//!
//! - **A name is never GC'd.** Deleting the feature that introduced the
//!   entity leaves the name in place, resolving to nothing — `names_list`
//!   reports `resolves: false` so an agent sees the hole instead of silently
//!   losing the record (§5.2).
//! - **The dotted path is a label, not the identity.** `plate.top_face` is one
//!   map key; the body segment is checked against the body's display name when
//!   the name is assigned, and nothing re-checks it afterwards. A body rename
//!   therefore leaves a dotted name that still resolves (its pid did not move)
//!   but whose first segment has drifted — which `names_list` shows by
//!   reporting the owning body's CURRENT display name next to the name. The
//!   alternative, rewriting labels from inside `rename_body`, would make a
//!   user's body rename fail on an agent's name collision; a label is not
//!   worth that.

use std::collections::{BTreeMap, HashMap};

use modeling_ops::OpResult;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use waffle_types::kernel::{KernelId, KernelIntrospect};
use waffle_types::{GeomRef, Selector, TopoKind};

use crate::types::{EngineError, Provenance};

/// Longest a single name segment may be. A name is a handle an agent types,
/// not a payload; the cap keeps a pathological key out of the document.
pub const MAX_SEGMENT_LEN: usize = 64;

/// One named entity: what it points at, and what it was when it was named.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct NamedRef {
    /// The preferred reference: `Selector::Pid` when the kernel had an id for
    /// the entity at naming time, else whatever selector identified it.
    /// Always stored with `ResolvePolicy::Strict` (§5.3): an agent sees no
    /// warning, so a near miss would be a silent wrong answer.
    pub target: GeomRef,
    /// What kind of entity this names. Redundant with `target.kind`, and kept
    /// because the tools answer with it and a caller should not have to reach
    /// into the reference to branch on it.
    pub kind: TopoKind,
    /// The reference as the caller authored it, kept ONLY when `target` is a
    /// pid — the fallback §5.2 asks for, used when the pid is gone and
    /// reported as `resolved_by: "query"` so the agent knows the primary
    /// identity was lost. Absent when `target` already IS the authored
    /// reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<GeomRef>,
    /// Who named it (ICR-4), the same record a feature's provenance carries.
    pub created: Provenance,
}

/// Name → the entity it labels. A `BTreeMap` so the document's key order is
/// the name order, in every process.
pub type NameTable = BTreeMap<String, NamedRef>;

/// A parsed name: an optional body segment and the leaf identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamePath {
    /// The body segment of a dotted name (`plate` in `plate.top_face`).
    pub body: Option<String>,
    /// The last segment, which is the entity's own name.
    pub leaf: String,
}

impl NamePath {
    /// The name as written — the map key.
    pub fn key(&self) -> String {
        match &self.body {
            Some(body) => format!("{body}.{}", self.leaf),
            None => self.leaf.clone(),
        }
    }
}

/// Whether one segment is a legal identifier: `[A-Za-z_][A-Za-z0-9_]*`, at
/// most [`MAX_SEGMENT_LEN`] bytes. The same grammar design parameters use, so
/// a name can be typed anywhere an identifier can.
pub fn is_identifier(segment: &str) -> bool {
    if segment.is_empty() || segment.len() > MAX_SEGMENT_LEN {
        return false;
    }
    let mut chars = segment.chars();
    let first = chars.next().unwrap_or('0');
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Parse a name into its segments, or say why it is not a name.
///
/// One or two segments: a bare `top_face`, or a body-qualified
/// `plate.top_face`. A third segment has nothing to mean today, so it is
/// refused rather than silently treated as part of the leaf.
pub fn parse_name(name: &str) -> Result<NamePath, String> {
    if name.is_empty() {
        return Err("a name cannot be empty".to_string());
    }
    let segments: Vec<&str> = name.split('.').collect();
    for segment in &segments {
        if !is_identifier(segment) {
            return Err(format!(
                "segment \"{segment}\" is not an identifier: a name segment is \
                 [A-Za-z_][A-Za-z0-9_]* and at most {MAX_SEGMENT_LEN} characters"
            ));
        }
    }
    match segments.as_slice() {
        [leaf] => Ok(NamePath {
            body: None,
            leaf: (*leaf).to_string(),
        }),
        [body, leaf] => Ok(NamePath {
            body: Some((*body).to_string()),
            leaf: (*leaf).to_string(),
        }),
        _ => Err(format!(
            "\"{name}\" has {} segments; a name is `leaf` or `body.leaf`",
            segments.len()
        )),
    }
}

/// Which reference answered a name (§5.2: the fallback's use is reported so
/// the agent knows the primary identity was lost).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResolvedBy {
    /// The stored `Selector::Pid` target resolved — possibly through its
    /// lineage root, which `warnings` then says.
    Pid,
    /// The stored target resolved and is not a pid (the kernel had no identity
    /// map for this entity when it was named).
    Selector,
    /// The pid is gone and the authored reference answered instead. The name
    /// still points somewhere, but not by persistent identity any more.
    Query,
}

/// What a name resolves to right now.
#[derive(Debug, Clone)]
pub struct NameResolution {
    pub kernel_id: KernelId,
    pub resolved_by: ResolvedBy,
    /// Which rung of the ladder actually answered (N2). This is what closes
    /// N1's last open item: `resolved_by: "pid"` could not say whether the pid
    /// answered directly or through its lineage root, and the honest signal was
    /// the presence of a warning. The resolver reports its own rung now, so
    /// `via` says `pid` or `pid_root` as a fact rather than an inference.
    pub via: crate::resolve::ResolvedVia,
    /// The resolver's own warnings, verbatim (a pid answered through its
    /// lineage root says so here), plus the primary failure when the fallback
    /// was used.
    pub warnings: Vec<String>,
}

/// Build the [`NamedRef`] for a reference the caller just authored: resolve it
/// against the live geometry, and store the entity's **persistent id** when the
/// kernel has one (N1 + drawings D0).
///
/// The authored reference is kept as `fallback` only when a pid replaced it —
/// otherwise `target` already is it, and a second copy would just be a second
/// thing to keep in step. A kernel with no identity map for the body (a
/// mesh-backed import) therefore yields a name over the authored selector,
/// which is what §5.2's "else the `Query` that identified it at naming time"
/// asks for; `names_list` reports that as `resolved_by: "selector"`.
pub fn mint(
    target: &GeomRef,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn KernelIntrospect,
    created: Provenance,
) -> Result<NamedRef, EngineError> {
    // Strict whatever the caller sent: an agent sees no warning, so a
    // near-miss rebind would be a silently wrong name (§5.3). That rule, and
    // the pid/fallback pair itself, live in `resolve::pin_identity` since N2 —
    // a sketch's plane face is pinned the same way, and two copies of the
    // rule would be two places for it to drift.
    let pinned = crate::resolve::pin_identity(target, feature_results, introspect)?;
    Ok(NamedRef {
        kind: pinned.target.kind,
        target: pinned.target,
        fallback: pinned.fallback,
        created,
    })
}

/// Resolve a name's reference against the live geometry.
///
/// The pid first; the authored fallback only when the pid is gone, and then
/// the pid's own failure is carried as a warning so the agent is told the
/// primary identity was lost rather than just handed an answer.
pub fn resolve(
    named: &NamedRef,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn KernelIntrospect,
) -> Result<NameResolution, EngineError> {
    let by_pid = matches!(named.target.selector, Selector::Pid { .. });
    let (resolved, used_fallback) = crate::resolve::resolve_pinned(
        &named.target,
        named.fallback.as_ref(),
        feature_results,
        introspect,
    )?;
    Ok(NameResolution {
        kernel_id: resolved.kernel_id,
        resolved_by: match (used_fallback, by_pid) {
            (true, _) => ResolvedBy::Query,
            (false, true) => ResolvedBy::Pid,
            (false, false) => ResolvedBy::Selector,
        },
        via: resolved.via,
        warnings: resolved.warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_the_parameter_grammar() {
        for good in ["a", "_x", "top_face", "F1", "_", "a1_B2"] {
            assert!(is_identifier(good), "{good} should be an identifier");
        }
        for bad in ["", "1a", "a-b", "a b", "a.b", "ä", "a!"] {
            assert!(!is_identifier(bad), "{bad} should not be an identifier");
        }
        assert!(is_identifier(&"a".repeat(MAX_SEGMENT_LEN)));
        assert!(!is_identifier(&"a".repeat(MAX_SEGMENT_LEN + 1)));
    }

    #[test]
    fn a_bare_name_has_no_body_segment() {
        let path = parse_name("top_face").expect("a legal name");
        assert_eq!(path.body, None);
        assert_eq!(path.leaf, "top_face");
        assert_eq!(path.key(), "top_face");
    }

    #[test]
    fn a_dotted_name_splits_into_body_and_leaf() {
        let path = parse_name("motor_mount.top_face").expect("a legal name");
        assert_eq!(path.body.as_deref(), Some("motor_mount"));
        assert_eq!(path.leaf, "top_face");
        assert_eq!(path.key(), "motor_mount.top_face");
    }

    #[test]
    fn three_segments_are_refused_rather_than_folded_into_the_leaf() {
        let err = parse_name("a.b.c").expect_err("three segments are not a name");
        assert!(err.contains("3 segments"), "{err}");
    }

    #[test]
    fn an_illegal_segment_names_itself_in_the_refusal() {
        let err = parse_name("plate.1face").expect_err("a leading digit is not an identifier");
        assert!(err.contains("1face"), "{err}");
        let err = parse_name("").expect_err("empty is not a name");
        assert!(err.contains("empty"), "{err}");
        // An empty segment inside a dotted name is a segment, not an empty name.
        let err = parse_name("plate.").expect_err("a trailing dot leaves an empty segment");
        assert!(err.contains("not an identifier"), "{err}");
    }
}
