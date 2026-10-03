use modeling_ops::OpResult;
use uuid::Uuid;
use waffle_types::kernel::units::MIN_FEATURE_SIZE;
use waffle_types::kernel::{KernelId, KernelIntrospect};
use waffle_types::{
    Anchor, Filter, GeomRef, OutputKey, ResolvePolicy, Role, Selector, TieBreak, TopoKind,
    TopoQuery, TopoSignature,
};

use crate::types::{EngineError, ReferenceRefusal, ResolutionReason};

/// A reference scoped to another tab's instance (v4 §2.8, in-context editing)
/// must be resolved through the edit context (`crate::context`), never against
/// this part's own results: the anchor feature id belongs to ANOTHER part, and
/// an accidental id match would silently pick the wrong geometry.
fn refuse_scoped(geom_ref: &GeomRef) -> Result<(), EngineError> {
    match &geom_ref.scope {
        None => Ok(()),
        Some(scope) => {
            let scope_text = crate::context::describe_scope(scope, None);
            Err(refuse(
                geom_ref,
                ResolutionReason::ScopeMissing {
                    scope: scope_text.clone(),
                },
                format!(
                    "reference is scoped to {scope_text}; it resolves only through the open \
                     assembly context"
                ),
            ))
        }
    }
}

/// A classified refusal of `geom_ref` (N2 §5.3 item 2). Every refusal the
/// ladder itself reaches goes through here, so none of them reaches a host as
/// an unclassified string.
fn refuse(geom_ref: &GeomRef, reason: ResolutionReason, text: String) -> EngineError {
    EngineError::ReferenceUnresolved(Box::new(ReferenceRefusal::of(geom_ref, reason, text)))
}

/// A refusal with no reference to attribute it to — the two shared scorers
/// ([`resolve_signature_over`], [`resolve_query_over`]) see candidates and a
/// fingerprint, not the `GeomRef` they came from. The caller that owns the
/// reference attaches the digest on the way out
/// ([`attribute`]).
fn refuse_bare(reason: ResolutionReason, text: String) -> EngineError {
    EngineError::ReferenceUnresolved(Box::new(ReferenceRefusal {
        reason_text: text,
        reason,
        reference: None,
        name: None,
    }))
}

/// Fill in the reference digest of a refusal raised by a scorer that did not
/// have the `GeomRef` (see [`refuse_bare`]). A refusal that already names one,
/// and every other error, passes through untouched.
fn attribute(err: EngineError, geom_ref: &GeomRef) -> EngineError {
    match err {
        EngineError::ReferenceUnresolved(mut r) if r.reference.is_none() => {
            r.reference = Some(crate::types::RefDigest::of(geom_ref));
            EngineError::ReferenceUnresolved(r)
        }
        other => other,
    }
}

/// Which rung of the ladder answered a reference (N2 §5.3).
///
/// A `BestEffort` reference that cannot be answered on its own terms rebinds by
/// geometry instead of refusing. Before N2 the only trace of that was a warning
/// string in [`ResolvedRef::warnings`] — which the app toasts and an agent never
/// sees. The rung is now reported beside the answer, and [`ResolvedVia::rebound`]
/// is the one question a caller has to ask: did this bind to the identity I
/// recorded, or to whatever was nearest?
///
/// This also closes N1's last open item: `resolved_by: "pid"` no longer hides
/// whether the pid answered directly or through its lineage root. The resolver
/// knows which rung it used, so nothing has to be inferred from the presence of
/// a warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolvedVia {
    /// The recorded persistent id AND its recorded lineage root matched.
    Pid,
    /// The pid is gone; the entity still descending from its recorded lineage
    /// root answered ([`resolve_by_pid`] rung 2). Not a rebind — it is the
    /// recorded lineage — but not the recorded id either, which is why it is
    /// its own rung.
    PidRoot,
    /// The recorded role and index answered.
    Role,
    /// The recorded fingerprint answered, above the confidence floor.
    Signature,
    /// The recorded query's filters answered.
    Query,
    /// The picked position matched an entity within the pick grid.
    Position,
    /// `BestEffort` rebind: the role index was out of range and was clamped.
    RoleClamped,
    /// `BestEffort` rebind: the fingerprint's best score was below the floor.
    SignatureLowConfidence,
    /// `BestEffort` rebind: the query matched nothing, so the first entity of
    /// the right kind was taken.
    QueryFirstOfKind,
    /// `BestEffort` rebind: the role did not resolve, so the first created
    /// entity of the right kind was taken ([`resolve_with_fallback`]).
    KindFallback,
    /// `BestEffort` rebind: nothing was within the pick grid of the position,
    /// so the nearest entity was taken.
    PositionNearest,
}

impl ResolvedVia {
    /// True when the answer is NOT the identity the reference recorded: the
    /// resolver rebound by geometry because `BestEffort` allowed it. Each of
    /// these also carries a warning naming what it bound and why; this is the
    /// same fact as a flag, so a tool result can report it without a host
    /// having to read prose.
    pub fn rebound(self) -> bool {
        matches!(
            self,
            Self::RoleClamped
                | Self::SignatureLowConfidence
                | Self::QueryFirstOfKind
                | Self::KindFallback
                | Self::PositionNearest
        )
    }
}

/// Result of resolving a GeomRef to a concrete KernelId.
#[derive(Debug, Clone)]
pub struct ResolvedRef {
    pub kernel_id: KernelId,
    pub warnings: Vec<String>,
    /// Which rung answered (N2). [`ResolvedVia::rebound`] says whether the
    /// answer is the recorded identity or a `BestEffort` rebind.
    pub via: ResolvedVia,
}

/// Resolve a GeomRef to a KernelId using the feature results map.
pub fn resolve_geom_ref(
    geom_ref: &GeomRef,
    feature_results: &std::collections::HashMap<Uuid, OpResult>,
) -> Result<ResolvedRef, EngineError> {
    refuse_scoped(geom_ref)?;
    // Extract the feature ID from the anchor
    let feature_id = match &geom_ref.anchor {
        waffle_types::Anchor::FeatureOutput {
            feature_id,
            output_key: _,
        } => *feature_id,
        waffle_types::Anchor::Datum { datum_id } => {
            return Err(refuse(
                geom_ref,
                ResolutionReason::NoMatch,
                format!("Datum references not yet supported (datum {})", datum_id),
            ));
        }
    };

    // Find the feature's OpResult
    let op_result = feature_results.get(&feature_id).ok_or_else(|| {
        refuse(
            geom_ref,
            ResolutionReason::NoMatch,
            format!("Feature {} has no result (not yet rebuilt?)", feature_id),
        )
    })?;

    // Apply the selector
    match &geom_ref.selector {
        Selector::Role { ref role, index } => {
            resolve_by_role(op_result, role, *index, geom_ref.policy)
                .map_err(|e| attribute(e, geom_ref))
        }
        Selector::Signature { ref signature } => {
            resolve_by_signature(op_result, signature, geom_ref.kind, geom_ref.policy)
                .map_err(|e| attribute(e, geom_ref))
        }
        Selector::Query { ref query } => {
            resolve_by_query(op_result, query, geom_ref.kind, geom_ref.policy)
                .map_err(|e| attribute(e, geom_ref))
        }
        Selector::Position { .. } => {
            // Position selectors are used for viewport picking (vertex overlay).
            // They don't resolve to kernel entities — return an error.
            Err(refuse(
                geom_ref,
                ResolutionReason::NoMatch,
                "Position selectors are not resolvable to kernel entities".to_string(),
            ))
        }
        Selector::Pid { pid, root_pid } => Err(refuse(
            geom_ref,
            ResolutionReason::NoMatch,
            format!(
                "persistent id {pid} (root {root_pid}) names an entity of the body's current \
                 geometry, which only the live kernel can read, and this resolution path has \
                 none (feature-engine: call `resolve_geom_ref_live`)"
            ),
        )),
    }
}

/// Exact-match tolerance for resolving a `Selector::Position`. The UI quantizes
/// picked viewport positions to 1e-6 (`Math.round(p*1e6)/1e6`), so the match
/// tolerance must be that grid — `MIN_FEATURE_SIZE` (1e-6) — NOT the finer
/// `TAU_MODEL` (1e-7), which a quantized position can legitimately exceed.
/// Distinct topological vertices are far enough apart for this to still
/// disambiguate; this is quantization accounting, not tolerance widening.
const POSITION_MATCH_TOL: f64 = MIN_FEATURE_SIZE;

/// Resolve a `Selector::Position` GeomRef to a concrete `KernelId` by finding
/// the nearest vertex / edge / face (per `geom_ref.kind`) of the anchor
/// feature's output body, using kernel introspection. This is what lets a
/// projected sketch entity re-bind to moved geometry across rebuilds.
///
/// `pos` is the picked 3D position (normally `geom_ref.selector`'s Position).
pub fn resolve_by_position(
    geom_ref: &GeomRef,
    feature_results: &std::collections::HashMap<Uuid, OpResult>,
    introspect: &dyn KernelIntrospect,
    pos: [f64; 3],
) -> Result<ResolvedRef, EngineError> {
    refuse_scoped(geom_ref)?;
    let handle = anchor_body_handle(geom_ref, feature_results, true)?;

    let candidates = match geom_ref.kind {
        TopoKind::Vertex => introspect.list_vertices(handle),
        TopoKind::Edge => introspect.list_edges(handle),
        TopoKind::Face => introspect.list_faces(handle),
        other => {
            return Err(refuse(
                geom_ref,
                ResolutionReason::NoMatch,
                format!("Position selector unsupported for {:?}", other),
            ));
        }
    };

    let mut best: Option<(KernelId, f64)> = None;
    for id in candidates {
        let sig = introspect.compute_signature(id, geom_ref.kind);
        if let Some(c) = sig.centroid {
            let d = ((c[0] - pos[0]).powi(2) + (c[1] - pos[1]).powi(2) + (c[2] - pos[2]).powi(2))
                .sqrt();
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((id, d));
            }
        }
    }

    match best {
        Some((id, d)) if d <= POSITION_MATCH_TOL => Ok(ResolvedRef {
            kernel_id: id,
            warnings: Vec::new(),
            via: ResolvedVia::Position,
        }),
        Some((id, d)) => match geom_ref.policy {
            ResolvePolicy::BestEffort => Ok(ResolvedRef {
                kernel_id: id,
                warnings: vec![format!(
                    "the picked position matches no {:?} within {:.0e}; bound the NEAREST one \
                     instead, {:.2e} away (BestEffort) — the geometry moved, and this is a \
                     rebind, not the entity that was picked",
                    geom_ref.kind, POSITION_MATCH_TOL, d
                )],
                via: ResolvedVia::PositionNearest,
            }),
            ResolvePolicy::Strict => Err(refuse(
                geom_ref,
                ResolutionReason::NoMatch,
                format!(
                    "No {:?} within {:.0e} of picked position (nearest {:.2e})",
                    geom_ref.kind, POSITION_MATCH_TOL, d
                ),
            )),
        },
        None => Err(refuse(
            geom_ref,
            ResolutionReason::NoMatch,
            format!(
                "Feature output has no {:?} entities to match position",
                geom_ref.kind
            ),
        )),
    }
}

/// Whether two output keys denote the same body output.
fn key_matches(a: &OutputKey, b: &OutputKey) -> bool {
    a.tag() == b.tag()
}

/// The kernel handle of the body output a `GeomRef`'s anchor names.
///
/// The anchor's `output_key` wins. `allow_first_body` then decides what a
/// feature whose outputs do not carry that key does: fall back to its first
/// body (the long-standing behaviour of [`resolve_by_position`]) or refuse.
///
/// [`resolve_by_pid`] refuses, and that is not fussiness. A persistent id is
/// only unique WITHIN one solid — two bodies split out of one operation can
/// carry edges with the same adjacent-face roots and therefore the same id —
/// so looking a pid up in the wrong body can find a different edge under the
/// stored number and report a wrong dimension while looking healthy. The
/// first-body fallback is a rebinding step, and this selector does not rebind.
fn anchor_body_handle<'a>(
    geom_ref: &GeomRef,
    feature_results: &'a std::collections::HashMap<Uuid, OpResult>,
    allow_first_body: bool,
) -> Result<&'a waffle_types::kernel::KernelSolidHandle, EngineError> {
    let (feature_id, output_key) = match &geom_ref.anchor {
        Anchor::FeatureOutput {
            feature_id,
            output_key,
        } => (*feature_id, output_key.clone()),
        Anchor::Datum { datum_id } => {
            return Err(refuse(
                geom_ref,
                ResolutionReason::NoMatch,
                format!("selector on datum {} not supported", datum_id),
            ));
        }
    };
    let op_result = feature_results.get(&feature_id).ok_or_else(|| {
        refuse(
            geom_ref,
            ResolutionReason::NoMatch,
            format!("Feature {} has no result (not yet rebuilt?)", feature_id),
        )
    })?;
    if let Some((_, body)) = op_result
        .outputs
        .iter()
        .find(|(k, _)| key_matches(k, &output_key))
    {
        return Ok(&body.handle);
    }
    if !allow_first_body {
        return Err(refuse(
            geom_ref,
            ResolutionReason::NoMatch,
            format!(
                "feature {} has no output {} any more, and a persistent id is only unique within \
                 one body, so another of its bodies must not be substituted",
                feature_id,
                output_key.tag()
            ),
        ));
    }
    op_result
        .outputs
        .first()
        .map(|(_, b)| &b.handle)
        .ok_or_else(|| {
            refuse(
                geom_ref,
                ResolutionReason::NoMatch,
                "feature produced no body output to resolve against".to_string(),
            )
        })
}

/// Resolve a `Selector::Pid` against the live kernel (drawings spec D0 §4
/// item 4).
///
/// 1. An entity of the anchor body whose `pid` matches **and whose
///    `root_pid` still matches the recorded one** wins. The root is a
///    mandatory cross-check, not a second choice: see below.
/// 2. Otherwise, an entity whose `root_pid` matches the stored `root_pid`
///    wins — this is the face that a later boolean rebuilt, carrying a fresh
///    pid but the same lineage root. The warning says so.
/// 3. Anything else is a loud failure, under BOTH policies: an ambiguous
///    match (two entities share the root, i.e. the face was split and the
///    reference cannot say which half it meant) and an absent one (the
///    entity is gone) alike.
///
/// Step 3 is the whole contract. Every other selector may fall back to a
/// nearest or first match under `BestEffort`; this one must not, because
/// silently rebinding a dimension to a different edge produces a drawing
/// that is wrong rather than one that is visibly broken (P9/P10).
///
/// # Why the root is checked and not just preferred
///
/// A pid is not globally unique forever — it is unique within one body at
/// one moment. Faces built by a constructor are content-seeded (D0 item 1),
/// but a BOOLEAN's own output faces are still counter-allocated, and a
/// counter restarts in a fresh arena. So a reopened document whose earlier
/// boolean changed its output face count re-mints the same numbers onto
/// different faces. Measured 2026-10-03 (one plate, two pockets): a name
/// recorded against the second pocket's floor came back, after a reopen, on
/// that pocket's SIDE WALL — matched exactly by `pid`, with the floor still
/// present and unnamed.
///
/// The root is what catches it: the floor descends from the cutter's end
/// cap and the wall from its lateral, so their roots differ even when a
/// recycled number does not. Requiring both means a reused number is no
/// longer a match at all; resolution falls through to the recorded root,
/// which still names the right face, and the caller is TOLD the number was
/// re-minted. If nothing answers, it refuses.
///
/// This cannot catch every reuse: edges and vertices carry `root_pid ==
/// pid`, so for them the check is vacuous. It does not need to be more —
/// their ids are content-seeded from their faces' lineage roots already
/// (`kernel_v2::pid`), so they are not counter-allocated and have no
/// recycled numbers to confuse.
pub fn resolve_by_pid(
    geom_ref: &GeomRef,
    feature_results: &std::collections::HashMap<Uuid, OpResult>,
    introspect: &dyn KernelIntrospect,
    pid: u64,
    root_pid: u64,
) -> Result<ResolvedRef, EngineError> {
    refuse_scoped(geom_ref)?;
    let handle = anchor_body_handle(geom_ref, feature_results, false)?;
    let known = introspect.all_entity_pids(handle, geom_ref.kind);
    let gone = |geom_ref: &GeomRef, text: String| {
        refuse(
            geom_ref,
            ResolutionReason::PidGone {
                pid,
                root_pid,
                last_seen_feature: match &geom_ref.anchor {
                    Anchor::FeatureOutput { feature_id, .. } => Some(*feature_id),
                    Anchor::Datum { .. } => None,
                },
            },
            text,
        )
    };
    if known.is_empty() {
        return Err(gone(
            geom_ref,
            format!(
                "kernel reports no persistent ids for {:?} on this body, so pid {} cannot be \
                 resolved (a mesh-backed imported body, or a kernel without persistent identity)",
                geom_ref.kind, pid
            ),
        ));
    }

    // The number AND the lineage it was recorded with. A face that answers
    // to the number alone is not this reference's face.
    let exact: Vec<KernelId> = known
        .iter()
        .filter(|(_, p)| p.pid == pid && p.root_pid == root_pid)
        .map(|(id, _)| *id)
        .collect();
    if exact.len() == 1 {
        return Ok(ResolvedRef {
            kernel_id: exact[0],
            warnings: Vec::new(),
            via: ResolvedVia::Pid,
        });
    }
    if exact.len() > 1 {
        return Err(refuse(
            geom_ref,
            ResolutionReason::Ambiguous {
                candidates: exact.iter().map(|id| id.0).collect(),
            },
            format!(
                "persistent id {} names {} different {:?} entities on this body — the kernel's \
                 identity map is not injective, which is a kernel defect, not a stale reference",
                pid,
                exact.len(),
                geom_ref.kind
            ),
        ));
    }

    // The number is on the body but on OTHER geometry: a recycled
    // counter-allocated id (a boolean output's — D0 item 1b). Whatever
    // answers below, the caller is told this happened, because "the id I
    // recorded now belongs to something else" is the fact that decides
    // whether a stored reference is still trustworthy.
    let reused: Vec<u64> = known
        .iter()
        .filter(|(_, p)| p.pid == pid && p.root_pid != root_pid)
        .map(|(_, p)| p.root_pid)
        .collect();
    let reuse_note = (!reused.is_empty()).then(|| {
        format!(
            "{:?} pid {} is no longer this entity's: it now belongs to geometry rooted at {} \
             (recorded root {}), so the id was re-minted onto something else — it was not \
             matched by number",
            geom_ref.kind,
            pid,
            reused
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            root_pid
        )
    });

    let by_root: Vec<KernelId> = known
        .iter()
        .filter(|(_, p)| p.root_pid == root_pid)
        .map(|(id, _)| *id)
        .collect();
    match by_root.len() {
        1 => {
            let mut warnings = Vec::new();
            warnings.extend(reuse_note);
            warnings.push(format!(
                "{:?} pid {} is gone; resolved through its lineage root {} (geometry was rebuilt \
                 by a later operation)",
                geom_ref.kind, pid, root_pid
            ));
            Ok(ResolvedRef {
                kernel_id: by_root[0],
                warnings,
                via: ResolvedVia::PidRoot,
            })
        }
        0 => Err(gone(
            geom_ref,
            match reuse_note {
                Some(note) => format!(
                    "no {:?} on this body carries persistent id {} with root {} — {}, and nothing \
                     descends from that root any more",
                    geom_ref.kind, pid, root_pid, note
                ),
                None => format!(
                    "no {:?} with persistent id {} (root {}) on this body — the referenced entity \
                     no longer exists",
                    geom_ref.kind, pid, root_pid
                ),
            },
        )),
        n => Err(refuse(
            geom_ref,
            ResolutionReason::Ambiguous {
                candidates: by_root.iter().map(|id| id.0).collect(),
            },
            format!(
                "{:?} pid {} is gone and its lineage root {} now names {} entities — the geometry \
                 was split and the reference cannot say which part it meant",
                geom_ref.kind, pid, root_pid, n
            ),
        )),
    }
}

/// Resolve a GeomRef with automatic fallback from role to signature.
///
/// 1. Try the primary selector (role or signature).
/// 2. If role fails and the feature has created entities, fall back to
///    signature matching among entities of the same `TopoKind`.
pub fn resolve_with_fallback(
    geom_ref: &GeomRef,
    feature_results: &std::collections::HashMap<Uuid, OpResult>,
) -> Result<ResolvedRef, EngineError> {
    match resolve_geom_ref(geom_ref, feature_results) {
        Ok(resolved) => Ok(resolved),
        Err(primary_err) => {
            // Only fall back when the selector is Role-based
            if let Selector::Role { .. } = &geom_ref.selector {
                let feature_id = match &geom_ref.anchor {
                    waffle_types::Anchor::FeatureOutput { feature_id, .. } => *feature_id,
                    _ => return Err(primary_err),
                };

                let op_result = match feature_results.get(&feature_id) {
                    Some(r) => r,
                    None => return Err(primary_err),
                };

                // Try to find an entity matching the requested TopoKind
                let matching: Vec<KernelId> = op_result
                    .provenance
                    .created
                    .iter()
                    .filter(|e| e.kind == geom_ref.kind)
                    .map(|e| e.kernel_id)
                    .collect();

                match geom_ref.policy {
                    ResolvePolicy::BestEffort => {
                        if let Some(&kernel_id) = matching.first() {
                            Ok(ResolvedRef {
                                kernel_id,
                                warnings: vec![format!(
                                    "the recorded role did not resolve ({primary_err}); bound the \
                                     FIRST {:?} this feature created instead, of {} (BestEffort) \
                                     — a rebind by position in the provenance list, not the \
                                     entity the reference recorded",
                                    geom_ref.kind,
                                    matching.len()
                                )],
                                via: ResolvedVia::KindFallback,
                            })
                        } else {
                            Err(primary_err)
                        }
                    }
                    ResolvePolicy::Strict => Err(primary_err),
                }
            } else {
                Err(primary_err)
            }
        }
    }
}

/// Resolve a GeomRef with the kernel at hand: a `Selector::Query` or a
/// `Selector::Signature` anchored at a body output is answered over that
/// body's CURRENT entities (every face / edge of the body, with live
/// signatures), not over the feature's provenance diff. The diff records what
/// an operation CREATED — right for a standalone extrude, but a merged or cut
/// body's surviving faces are absent from it and its deleted faces present,
/// so a query over the diff can miss the face the caller means or name one
/// that no longer exists. Every other selector, and the two above when the
/// body cannot be listed, take [`resolve_with_fallback`].
///
/// N0 of `specs/agent_mechanical_design.md` §5.1 brought `Signature` onto
/// this path for the reason the doc above already gave for `Query`: a UNION's
/// `created` list holds no faces at all (the diff matches every result face
/// to an operand face and calls it survived), so a fingerprint for a boolean
/// result's face had nothing of its own kind to match against and fell back
/// to whatever the first created entity was — measured as a cap resolving to
/// a vertex.
pub fn resolve_geom_ref_live(
    geom_ref: &GeomRef,
    feature_results: &std::collections::HashMap<Uuid, OpResult>,
    introspect: &dyn KernelIntrospect,
) -> Result<ResolvedRef, EngineError> {
    // A persistent id is answered HERE and nowhere else: it is a question
    // about the body's current entities, which only the kernel can answer,
    // and it must never silently fall through to a rebinding selector.
    if let Selector::Pid { pid, root_pid } = &geom_ref.selector {
        return resolve_by_pid(geom_ref, feature_results, introspect, *pid, *root_pid);
    }
    if let Anchor::FeatureOutput {
        feature_id,
        output_key,
    } = &geom_ref.anchor
    {
        if matches!(
            geom_ref.selector,
            Selector::Query { .. } | Selector::Signature { .. }
        ) {
            refuse_scoped(geom_ref)?;
            if let Some(body) = feature_results.get(feature_id).and_then(|r| {
                r.outputs
                    .iter()
                    .find(|(k, _)| key_matches(k, output_key))
                    .map(|(_, b)| b)
            }) {
                let live = introspect.compute_all_signatures(&body.handle, geom_ref.kind);
                if !live.is_empty() {
                    let candidates: Vec<(KernelId, &TopoSignature)> =
                        live.iter().map(|(id, sig)| (*id, sig)).collect();
                    match &geom_ref.selector {
                        Selector::Query { query } => {
                            return resolve_query_over(
                                &candidates,
                                query,
                                geom_ref.kind,
                                geom_ref.policy,
                            )
                            .map_err(|e| attribute(e, geom_ref));
                        }
                        Selector::Signature { signature } => {
                            return resolve_signature_over(
                                &candidates,
                                signature,
                                geom_ref.kind,
                                geom_ref.policy,
                            )
                            .map_err(|e| attribute(e, geom_ref));
                        }
                        _ => unreachable!("guarded by the matches! above"),
                    }
                }
            }
        }
    }
    resolve_with_fallback(geom_ref, feature_results)
}

/// A stored identity: the entity's **persistent id** when the kernel has one,
/// with the reference as the caller authored it kept as the fallback.
///
/// This is the shape N1's `names::mint` invented for a name and N2 gives to
/// every stored reference that has to survive a rebuild — a sketch's own plane
/// face first (§5.3 item 3). The pair is what makes the ladder possible: the
/// pid answers by identity, its lineage root answers when a later operation
/// rebuilt the geometry, and only then does the authored reference rebind by
/// geometry — loudly.
#[derive(Debug, Clone)]
pub struct PinnedRef {
    /// What to store and try first: a `Selector::Pid` when the kernel had an
    /// identity for the entity, else the authored reference itself.
    pub target: GeomRef,
    /// The authored reference, stored only when a pid replaced it — otherwise
    /// `target` already IS it, and a second copy would be a second thing to
    /// keep in step.
    pub fallback: Option<GeomRef>,
    /// The entity it resolved to at pinning time.
    pub kernel_id: KernelId,
}

/// Pin `authored` to the identity the kernel has for it right now.
///
/// `Strict` whatever the caller sent: this runs at AUTHORING time, with the
/// entity in front of the author, so a reference that does not identify one
/// entity is a thing to fix now rather than to rebind silently on every
/// rebuild forever (§5.3 item 1).
pub fn pin_identity(
    authored: &GeomRef,
    feature_results: &std::collections::HashMap<Uuid, OpResult>,
    introspect: &dyn KernelIntrospect,
) -> Result<PinnedRef, EngineError> {
    let mut authored = authored.clone();
    authored.policy = ResolvePolicy::Strict;
    let resolved = resolve_geom_ref_live(&authored, feature_results, introspect)?;
    match introspect.entity_pid(resolved.kernel_id, authored.kind) {
        Some(pid) if !matches!(authored.selector, Selector::Pid { .. }) => {
            let mut by_pid = authored.clone();
            by_pid.selector = Selector::Pid {
                pid: pid.pid,
                root_pid: pid.root_pid,
            };
            Ok(PinnedRef {
                target: by_pid,
                fallback: Some(authored),
                kernel_id: resolved.kernel_id,
            })
        }
        _ => Ok(PinnedRef {
            target: authored,
            fallback: None,
            kernel_id: resolved.kernel_id,
        }),
    }
}

/// Resolve a stored identity through the full ladder: `target` (a pid, then its
/// lineage root, inside [`resolve_by_pid`]), then `fallback` — and only then.
///
/// Returns the answer and, when the FALLBACK supplied it, the primary failure
/// that made it necessary. That failure is the fact which decides whether the
/// reference is still trustworthy — the identity the caller recorded is gone
/// and this answer came from geometry instead — so it is handed back rather
/// than only written into a warning string.
///
/// **This path rebinds, deliberately** (N1 §5.2: a name whose entity is gone
/// still measures through its fallback). §5.3's oracle asks for a refusal
/// instead; what N2 does is make the rebind machine-visible — see the spec's
/// "Implementation notes (N2)" for why the softer reading won.
pub fn resolve_pinned(
    target: &GeomRef,
    fallback: Option<&GeomRef>,
    feature_results: &std::collections::HashMap<Uuid, OpResult>,
    introspect: &dyn KernelIntrospect,
) -> Result<(ResolvedRef, Option<EngineError>), EngineError> {
    match resolve_geom_ref_live(target, feature_results, introspect) {
        Ok(resolved) => Ok((resolved, None)),
        Err(primary) => {
            let Some(fallback) = fallback else {
                return Err(primary);
            };
            let mut resolved = resolve_geom_ref_live(fallback, feature_results, introspect)
                .map_err(|_| primary.clone())?;
            let mut warnings = vec![format!(
                "the persistent id is gone ({primary}); the reference resolved through the \
                 selector it was authored with, which rebinds by geometry and may name a \
                 different entity"
            )];
            warnings.append(&mut resolved.warnings);
            resolved.warnings = warnings;
            Ok((resolved, Some(primary)))
        }
    }
}

/// Resolve by user-specified geometric query over the feature's provenance
/// (what the operation created). See [`resolve_geom_ref_live`] for the
/// body-wide form.
fn resolve_by_query(
    op_result: &OpResult,
    query: &TopoQuery,
    kind: TopoKind,
    policy: ResolvePolicy,
) -> Result<ResolvedRef, EngineError> {
    let candidates: Vec<(KernelId, &TopoSignature)> = op_result
        .provenance
        .created
        .iter()
        .filter(|e| e.kind == kind)
        .map(|e| (e.kernel_id, &e.signature))
        .collect();
    resolve_query_over(&candidates, query, kind, policy)
}

/// Apply a query's filters and tie-break to `candidates` (all of `kind`).
fn resolve_query_over(
    candidates: &[(KernelId, &TopoSignature)],
    query: &TopoQuery,
    kind: TopoKind,
    policy: ResolvePolicy,
) -> Result<ResolvedRef, EngineError> {
    let matches: Vec<(KernelId, &TopoSignature)> = candidates
        .iter()
        .filter(|(_, sig)| passes_all_filters(sig, &query.filters))
        .copied()
        .collect();

    if matches.is_empty() {
        return match policy {
            ResolvePolicy::Strict => Err(refuse_bare(
                ResolutionReason::NoMatch,
                format!(
                    "Query matched no {:?} entities ({} candidates, {} filters)",
                    kind,
                    candidates.len(),
                    query.filters.len()
                ),
            )),
            ResolvePolicy::BestEffort => {
                // Fall back to first entity of matching kind
                let fallback = candidates.first().map(|(id, _)| *id);
                match fallback {
                    Some(id) => Ok(ResolvedRef {
                        kernel_id: id,
                        warnings: vec![format!(
                            "the query's {} filter(s) matched none of the {} {kind:?} entities; \
                             bound the FIRST of them instead (BestEffort) — a rebind by listing \
                             order, not an entity the reference described",
                            query.filters.len(),
                            candidates.len()
                        )],
                        via: ResolvedVia::QueryFirstOfKind,
                    }),
                    None => Err(refuse_bare(
                        ResolutionReason::NoMatch,
                        format!("No {:?} entities available for query fallback", kind),
                    )),
                }
            }
        };
    }

    // Apply tie-breaking
    let winner = apply_tie_break(&matches, &query.tie_break);

    Ok(ResolvedRef {
        kernel_id: winner,
        warnings: if matches.len() > 1 {
            vec![format!(
                "Query matched {} entities, tie-break selected one",
                matches.len()
            )]
        } else {
            Vec::new()
        },
        via: ResolvedVia::Query,
    })
}

/// Check if a signature passes all query filters. Public for the bridge's
/// `ListFaces` query (agent-link ICR-3), which filters with the same rules.
pub fn passes_all_filters(sig: &TopoSignature, filters: &[Filter]) -> bool {
    for filter in filters {
        match filter {
            Filter::SurfaceType { surface_type } => match &sig.surface_type {
                Some(st) => {
                    if st != surface_type {
                        return false;
                    }
                }
                None => return false,
            },
            Filter::NormalDirection {
                direction,
                tolerance,
            } => {
                match &sig.normal {
                    Some(normal) => {
                        let dot = normal[0] * direction[0]
                            + normal[1] * direction[1]
                            + normal[2] * direction[2];
                        // Clamp to [-1, 1] for acos safety
                        let dot_clamped = dot.clamp(-1.0, 1.0);
                        let angle = dot_clamped.acos();
                        if angle > *tolerance {
                            return false;
                        }
                    }
                    None => return false,
                }
            }
            Filter::NearPoint { point, distance } => match &sig.centroid {
                Some(centroid) => {
                    let dx = centroid[0] - point[0];
                    let dy = centroid[1] - point[1];
                    let dz = centroid[2] - point[2];
                    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                    if dist > *distance {
                        return false;
                    }
                }
                None => return false,
            },
            Filter::AreaRange { min, max } => match sig.area {
                Some(area) => {
                    if area < *min || area > *max {
                        return false;
                    }
                }
                None => return false,
            },
        }
    }
    true
}

/// Apply tie-breaking to select a single entity from matches.
fn apply_tie_break(
    matches: &[(KernelId, &TopoSignature)],
    tie_break: &Option<TieBreak>,
) -> KernelId {
    debug_assert!(!matches.is_empty());

    match tie_break {
        Some(TieBreak::LargestArea) => {
            matches
                .iter()
                .max_by(|a, b| {
                    let area_a = a.1.area.unwrap_or(0.0);
                    let area_b = b.1.area.unwrap_or(0.0);
                    area_a
                        .partial_cmp(&area_b)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap()
                .0
        }
        Some(TieBreak::NearestTo { point }) => {
            matches
                .iter()
                .min_by(|a, b| {
                    let dist_a = centroid_distance(a.1, point);
                    let dist_b = centroid_distance(b.1, point);
                    dist_a
                        .partial_cmp(&dist_b)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap()
                .0
        }
        Some(TieBreak::FarthestAlong { direction }) => {
            // Strictly-greater comparison keeps the FIRST of equals.
            let along = |sig: &TopoSignature| -> f64 {
                sig.centroid.map_or(f64::MIN, |c| {
                    c[0] * direction[0] + c[1] * direction[1] + c[2] * direction[2]
                })
            };
            let mut best = matches[0];
            for m in &matches[1..] {
                if along(m.1) > along(best.1) {
                    best = *m;
                }
            }
            best.0
        }
        Some(TieBreak::SmallestIndex) | None => {
            // First in iteration order
            matches[0].0
        }
    }
}

/// Compute distance from a signature's centroid to a point.
fn centroid_distance(sig: &TopoSignature, point: &[f64; 3]) -> f64 {
    match &sig.centroid {
        Some(c) => {
            let dx = c[0] - point[0];
            let dy = c[1] - point[1];
            let dz = c[2] - point[2];
            (dx * dx + dy * dy + dz * dz).sqrt()
        }
        None => f64::MAX,
    }
}

/// Resolve by semantic role.
fn resolve_by_role(
    op_result: &OpResult,
    role: &Role,
    index: usize,
    policy: ResolvePolicy,
) -> Result<ResolvedRef, EngineError> {
    let matching: Vec<KernelId> = op_result
        .provenance
        .role_assignments
        .iter()
        .filter(|(_, r)| r == role)
        .map(|(id, _)| *id)
        .collect();

    if matching.is_empty() {
        return Err(refuse_bare(
            ResolutionReason::NoMatch,
            format!("No entity with role {:?}", role),
        ));
    }

    if index < matching.len() {
        Ok(ResolvedRef {
            kernel_id: matching[index],
            warnings: Vec::new(),
            via: ResolvedVia::Role,
        })
    } else {
        match policy {
            ResolvePolicy::Strict => Err(refuse_bare(
                ResolutionReason::NoMatch,
                format!(
                    "Role {:?} index {} out of range (found {})",
                    role,
                    index,
                    matching.len()
                ),
            )),
            ResolvePolicy::BestEffort => {
                let kernel_id = *matching.last().unwrap();
                Ok(ResolvedRef {
                    kernel_id,
                    warnings: vec![format!(
                        "role {:?} index {} is out of range; CLAMPED to index {} (BestEffort) — \
                         the feature now has {} entities with that role, so this is the last of \
                         them, not the one the reference recorded",
                        role,
                        index,
                        matching.len() - 1,
                        matching.len()
                    )],
                    via: ResolvedVia::RoleClamped,
                })
            }
        }
    }
}

/// Resolve by geometric signature (fallback when role fails).
/// Two scores closer than this are the SAME score: the difference is f64
/// summation rounding over a handful of weighted terms, not information about
/// which candidate the reference meant. `TAU_WORK` is the workspace's working
/// floor; a genuine geometric difference exceeds it by orders of magnitude.
const SIGNATURE_TIE: f64 = waffle_types::kernel::units::TAU_WORK;

/// Resolve a `Selector::Signature` by scoring every created entity against the
/// stored fingerprint.
///
/// Two refusals are policy-independent (N0 of
/// `specs/agent_mechanical_design.md` §5.1) because neither is a question of
/// confidence — in both, the reference does not identify an entity at all:
///
/// - the fingerprint shares no scorable field with any candidate
///   ([`modeling_ops::signature_match`] returns `None` for all of them), the
///   case of the index-only signature the viewport used to mint; and
/// - the best score is shared by several candidates, so nothing in the
///   reference distinguishes them.
///
/// Before N0 both bound to the first created entity, with a "0.0%" warning in
/// the first case and silently in the second.
fn resolve_by_signature(
    op_result: &OpResult,
    target_sig: &waffle_types::TopoSignature,
    kind: TopoKind,
    policy: ResolvePolicy,
) -> Result<ResolvedRef, EngineError> {
    // Only entities of the reference's OWN kind are candidates. Scoring a
    // face's fingerprint against an edge or a vertex record compares the one
    // field they happen to share — a vertex carries a centroid and nothing
    // else — so a cap and the seam vertex sitting on it both scored 100%, and
    // which one won was the order `created` happened to be in (measured on a
    // cylinder extrude: face, edge and vertex all at `[0, −5, 0]`).
    let candidates: Vec<(KernelId, &TopoSignature)> = op_result
        .provenance
        .created
        .iter()
        .filter(|e| e.kind == kind)
        .map(|e| (e.kernel_id, &e.signature))
        .collect();
    resolve_signature_over(&candidates, target_sig, kind, policy)
}

/// Score `target_sig` against `candidates` and bind the single best — the
/// shared core of the provenance form ([`resolve_by_signature`]) and the live
/// body-wide form ([`resolve_geom_ref_live`]).
fn resolve_signature_over(
    candidates: &[(KernelId, &TopoSignature)],
    target_sig: &waffle_types::TopoSignature,
    kind: TopoKind,
    policy: ResolvePolicy,
) -> Result<ResolvedRef, EngineError> {
    let scored: Vec<(KernelId, f64)> = candidates
        .iter()
        .filter_map(|(id, sig)| modeling_ops::signature_match(sig, target_sig).map(|s| (*id, s)))
        .collect();

    if scored.is_empty() {
        return Err(refuse_bare(
            ResolutionReason::NoMatch,
            if candidates.is_empty() {
                format!("No entities of kind {kind:?} to match signature against")
            } else {
                format!(
                    "the reference's signature carries no geometry to match against \
                     {} candidate(s) of kind {kind:?} — nothing to compare, so \
                     nothing may be bound",
                    candidates.len()
                )
            },
        ));
    }

    let best_sim = scored
        .iter()
        .map(|(_, s)| *s)
        .fold(f64::NEG_INFINITY, f64::max);
    let tied: Vec<u64> = scored
        .iter()
        .filter(|(_, s)| (best_sim - *s).abs() <= SIGNATURE_TIE)
        .map(|(id, _)| id.0)
        .collect();
    if tied.len() > 1 {
        return Err(EngineError::ReferenceAmbiguous {
            candidates: tied,
            score: best_sim,
        });
    }
    let best_match = scored
        .iter()
        .find(|(_, s)| *s == best_sim)
        .map(|(id, s)| (*id, *s));

    match best_match {
        Some((id, sim)) if sim > 0.5 => {
            let mut warnings = Vec::new();
            if sim < 0.9 {
                warnings.push(format!("Signature match confidence: {:.1}%", sim * 100.0));
            }
            Ok(ResolvedRef {
                kernel_id: id,
                warnings,
                via: ResolvedVia::Signature,
            })
        }
        Some((id, sim)) => match policy {
            ResolvePolicy::BestEffort => Ok(ResolvedRef {
                kernel_id: id,
                warnings: vec![format!(
                    "no {kind:?} fits the reference's fingerprint: the best of {} scored only \
                     {:.1}%, below the 50% floor, and was bound anyway (BestEffort) — a rebind \
                     to the closest geometry, not the entity the reference recorded",
                    scored.len(),
                    sim * 100.0
                )],
                via: ResolvedVia::SignatureLowConfidence,
            }),
            ResolvePolicy::Strict => Err(refuse_bare(
                ResolutionReason::NoMatch,
                format!("Best signature match too low: {:.1}%", sim * 100.0),
            )),
        },
        None => Err(refuse_bare(
            ResolutionReason::NoMatch,
            "No entities to match signature against".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use modeling_ops::{Diagnostics, EntityRecord, OpResult, Provenance};
    use waffle_types::kernel::KernelId;
    use waffle_types::{Filter, TieBreak, TopoKind, TopoQuery, TopoSignature};

    fn make_face(
        id: u64,
        surface_type: &str,
        area: f64,
        centroid: [f64; 3],
        normal: [f64; 3],
    ) -> EntityRecord {
        EntityRecord {
            kernel_id: KernelId(id),
            kind: TopoKind::Face,
            signature: TopoSignature {
                surface_type: Some(surface_type.to_string()),
                area: Some(area),
                centroid: Some(centroid),
                normal: Some(normal),
                bbox: None,
                adjacency_hash: None,
                length: None,
                axis: None,
            },
        }
    }

    fn make_op_result(entities: Vec<EntityRecord>) -> OpResult {
        OpResult {
            outputs: vec![],
            provenance: Provenance {
                created: entities,
                deleted: vec![],
                modified: vec![],
                role_assignments: vec![],
            },
            diagnostics: Diagnostics::default(),
        }
    }

    // --- Position-selector resolution (projection incr 2) ---

    use modeling_ops::BodyOutput;
    use std::collections::HashMap;
    use waffle_types::kernel::{KernelIntrospect, KernelSolidHandle};
    use waffle_types::{Anchor, OutputKey, Selector};

    /// Minimal KernelIntrospect over a fixed set of vertex positions.
    struct FakeIntrospect {
        verts: Vec<(KernelId, [f64; 3])>,
    }
    impl KernelIntrospect for FakeIntrospect {
        fn list_vertices(&self, _: &KernelSolidHandle) -> Vec<KernelId> {
            self.verts.iter().map(|(id, _)| *id).collect()
        }
        fn compute_signature(&self, entity: KernelId, _kind: TopoKind) -> TopoSignature {
            let centroid = self
                .verts
                .iter()
                .find(|(id, _)| *id == entity)
                .map(|(_, p)| *p);
            TopoSignature {
                surface_type: None,
                area: None,
                centroid,
                normal: None,
                bbox: None,
                adjacency_hash: None,
                length: None,
                axis: None,
            }
        }
        fn list_faces(&self, _: &KernelSolidHandle) -> Vec<KernelId> {
            vec![]
        }
        fn list_edges(&self, _: &KernelSolidHandle) -> Vec<KernelId> {
            vec![]
        }
        fn face_edges(&self, _: KernelId) -> Vec<KernelId> {
            vec![]
        }
        fn edge_faces(&self, _: KernelId) -> Vec<KernelId> {
            vec![]
        }
        fn edge_vertices(&self, _: KernelId) -> (KernelId, KernelId) {
            (KernelId(0), KernelId(0))
        }
        fn face_neighbors(&self, _: KernelId) -> Vec<KernelId> {
            vec![]
        }
        fn compute_all_signatures(
            &self,
            _: &KernelSolidHandle,
            _: TopoKind,
        ) -> Vec<(KernelId, TopoSignature)> {
            vec![]
        }
    }

    fn op_with_body() -> OpResult {
        OpResult {
            outputs: vec![(
                OutputKey::Main,
                BodyOutput {
                    handle: KernelSolidHandle::from_raw(1),
                    mesh: None,
                    edges: None,
                },
            )],
            provenance: Provenance {
                created: vec![],
                deleted: vec![],
                modified: vec![],
                role_assignments: vec![],
            },
            diagnostics: Diagnostics::default(),
        }
    }

    fn pos_ref(feature_id: Uuid, x: f64, y: f64, z: f64, policy: ResolvePolicy) -> GeomRef {
        GeomRef {
            kind: TopoKind::Vertex,
            anchor: Anchor::FeatureOutput {
                feature_id,
                output_key: OutputKey::Main,
            },
            selector: Selector::Position { x, y, z },
            policy,
            scope: None,
        }
    }

    #[test]
    fn position_resolves_to_exact_vertex() {
        let fid = Uuid::new_v4();
        let mut results = HashMap::new();
        results.insert(fid, op_with_body());
        let intro = FakeIntrospect {
            verts: vec![
                (KernelId(10), [0.0, 0.0, 0.0]),
                (KernelId(11), [2.0, 0.0, 0.0]),
                (KernelId(12), [2.0, 3.0, 5.0]),
            ],
        };
        // Pick exactly vertex 12.
        let gref = pos_ref(fid, 2.0, 3.0, 5.0, ResolvePolicy::BestEffort);
        let r = resolve_by_position(&gref, &results, &intro, [2.0, 3.0, 5.0]).unwrap();
        assert_eq!(r.kernel_id, KernelId(12));
        assert!(r.warnings.is_empty(), "exact match warns nothing");
    }

    #[test]
    fn position_tolerates_ui_quantization() {
        // The UI rounds positions to 1e-6; a vertex 5e-7 away must still match
        // exactly (TAU_MODEL=1e-7 would wrongly reject this).
        let fid = Uuid::new_v4();
        let mut results = HashMap::new();
        results.insert(fid, op_with_body());
        let intro = FakeIntrospect {
            verts: vec![
                (KernelId(10), [0.0, 0.0, 0.0]),
                (KernelId(11), [5.0, 0.0, 0.0]),
            ],
        };
        let gref = pos_ref(fid, 5.0, 0.0, 0.0, ResolvePolicy::BestEffort);
        let r = resolve_by_position(&gref, &results, &intro, [5.0 + 5e-7, 0.0, 0.0]).unwrap();
        assert_eq!(r.kernel_id, KernelId(11));
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn position_reresolves_to_moved_vertex() {
        // Simulate an upstream edit moving the box: the SAME picked position now
        // lands nearest a vertex that has shifted; BestEffort still resolves it
        // (and warns), which is how a projected point follows moved geometry.
        let fid = Uuid::new_v4();
        let mut results = HashMap::new();
        results.insert(fid, op_with_body());
        let intro = FakeIntrospect {
            verts: vec![
                (KernelId(10), [0.0, 0.0, 0.0]),
                (KernelId(11), [2.0, 0.0, 10.0]),
            ],
        };
        // Originally picked the vertex at z=5 (box height 5); the box grew to 10.
        let gref = pos_ref(fid, 2.0, 0.0, 5.0, ResolvePolicy::BestEffort);
        let r = resolve_by_position(&gref, &results, &intro, [2.0, 0.0, 5.0]).unwrap();
        assert_eq!(r.kernel_id, KernelId(11));
        assert!(!r.warnings.is_empty(), "approximate match should warn");
    }

    #[test]
    fn position_strict_errors_when_far() {
        let fid = Uuid::new_v4();
        let mut results = HashMap::new();
        results.insert(fid, op_with_body());
        let intro = FakeIntrospect {
            verts: vec![(KernelId(10), [0.0, 0.0, 0.0])],
        };
        let gref = pos_ref(fid, 9.0, 9.0, 9.0, ResolvePolicy::Strict);
        assert!(resolve_by_position(&gref, &results, &intro, [9.0, 9.0, 9.0]).is_err());
    }

    #[test]
    fn query_farthest_along_tie_break_picks_the_top_face_and_keeps_the_first_of_equals() {
        // A box's six planar faces: farthest along +z is the top (centroid
        // z = 5); the four side faces tie at z = 2.5 and the FIRST one wins
        // when the direction is +x-and-nothing-else-distinguishes.
        let op = make_op_result(vec![
            make_face(1, "planar", 10.0, [2.0, 2.0, 0.0], [0.0, 0.0, -1.0]),
            make_face(2, "planar", 10.0, [2.0, 2.0, 5.0], [0.0, 0.0, 1.0]),
            make_face(3, "planar", 20.0, [2.0, 0.0, 2.5], [0.0, -1.0, 0.0]),
            make_face(4, "planar", 20.0, [4.0, 2.0, 2.5], [1.0, 0.0, 0.0]),
            make_face(5, "planar", 20.0, [2.0, 4.0, 2.5], [0.0, 1.0, 0.0]),
            make_face(6, "planar", 20.0, [0.0, 2.0, 2.5], [-1.0, 0.0, 0.0]),
        ]);
        let top = TopoQuery {
            filters: vec![Filter::SurfaceType {
                surface_type: "planar".to_string(),
            }],
            tie_break: Some(TieBreak::FarthestAlong {
                direction: [0.0, 0.0, 1.0],
            }),
        };
        let r = resolve_by_query(&op, &top, TopoKind::Face, ResolvePolicy::Strict).unwrap();
        assert_eq!(r.kernel_id, KernelId(2));
        let plus_x = TopoQuery {
            filters: vec![],
            tie_break: Some(TieBreak::FarthestAlong {
                direction: [1.0, 0.0, 0.0],
            }),
        };
        let r = resolve_by_query(&op, &plus_x, TopoKind::Face, ResolvePolicy::Strict).unwrap();
        assert_eq!(r.kernel_id, KernelId(4));
        // Equal projections keep the first: -z direction, faces 3..6 tie at
        // -2.5 but the bottom (face 1, z = 0) is farthest along -z.
        let minus_z = TopoQuery {
            filters: vec![],
            tie_break: Some(TieBreak::FarthestAlong {
                direction: [0.0, 0.0, -1.0],
            }),
        };
        let r = resolve_by_query(&op, &minus_z, TopoKind::Face, ResolvePolicy::Strict).unwrap();
        assert_eq!(r.kernel_id, KernelId(1));
    }

    #[test]
    fn query_surface_type_filter() {
        let op = make_op_result(vec![
            make_face(1, "planar", 10.0, [0.0, 0.0, 5.0], [0.0, 0.0, 1.0]),
            make_face(2, "cylindrical", 20.0, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
        ]);
        let query = TopoQuery {
            filters: vec![Filter::SurfaceType {
                surface_type: "cylindrical".to_string(),
            }],
            tie_break: None,
        };
        let result = resolve_by_query(&op, &query, TopoKind::Face, ResolvePolicy::Strict).unwrap();
        assert_eq!(result.kernel_id, KernelId(2));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn query_area_range_filter() {
        let op = make_op_result(vec![
            make_face(1, "planar", 5.0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            make_face(2, "planar", 15.0, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            make_face(3, "planar", 25.0, [2.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ]);
        let query = TopoQuery {
            filters: vec![Filter::AreaRange {
                min: 10.0,
                max: 20.0,
            }],
            tie_break: None,
        };
        let result = resolve_by_query(&op, &query, TopoKind::Face, ResolvePolicy::Strict).unwrap();
        assert_eq!(result.kernel_id, KernelId(2));
    }

    #[test]
    fn query_normal_direction_filter() {
        let op = make_op_result(vec![
            make_face(1, "planar", 10.0, [0.0, 0.0, 5.0], [0.0, 0.0, 1.0]), // +Z
            make_face(2, "planar", 10.0, [0.0, 0.0, -5.0], [0.0, 0.0, -1.0]), // -Z
        ]);
        let query = TopoQuery {
            filters: vec![Filter::NormalDirection {
                direction: [0.0, 0.0, 1.0],
                tolerance: 0.1, // ~5.7 degrees
            }],
            tie_break: None,
        };
        let result = resolve_by_query(&op, &query, TopoKind::Face, ResolvePolicy::Strict).unwrap();
        assert_eq!(result.kernel_id, KernelId(1));
    }

    #[test]
    fn query_near_point_filter() {
        let op = make_op_result(vec![
            make_face(1, "planar", 10.0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            make_face(2, "planar", 10.0, [10.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ]);
        let query = TopoQuery {
            filters: vec![Filter::NearPoint {
                point: [9.0, 0.0, 0.0],
                distance: 2.0,
            }],
            tie_break: None,
        };
        let result = resolve_by_query(&op, &query, TopoKind::Face, ResolvePolicy::Strict).unwrap();
        assert_eq!(result.kernel_id, KernelId(2));
    }

    #[test]
    fn query_multiple_filters_combined() {
        let op = make_op_result(vec![
            make_face(1, "planar", 10.0, [0.0, 0.0, 5.0], [0.0, 0.0, 1.0]),
            make_face(2, "cylindrical", 20.0, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            make_face(3, "planar", 30.0, [0.0, 0.0, -5.0], [0.0, 0.0, -1.0]),
        ]);
        let query = TopoQuery {
            filters: vec![
                Filter::SurfaceType {
                    surface_type: "planar".to_string(),
                },
                Filter::AreaRange {
                    min: 20.0,
                    max: 50.0,
                },
            ],
            tie_break: None,
        };
        let result = resolve_by_query(&op, &query, TopoKind::Face, ResolvePolicy::Strict).unwrap();
        assert_eq!(result.kernel_id, KernelId(3));
    }

    #[test]
    fn query_tie_break_largest_area() {
        let op = make_op_result(vec![
            make_face(1, "planar", 10.0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            make_face(2, "planar", 30.0, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            make_face(3, "planar", 20.0, [2.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ]);
        let query = TopoQuery {
            filters: vec![Filter::SurfaceType {
                surface_type: "planar".to_string(),
            }],
            tie_break: Some(TieBreak::LargestArea),
        };
        let result = resolve_by_query(&op, &query, TopoKind::Face, ResolvePolicy::Strict).unwrap();
        assert_eq!(result.kernel_id, KernelId(2));
        assert!(!result.warnings.is_empty()); // multiple matches warning
    }

    #[test]
    fn query_tie_break_nearest_to() {
        let op = make_op_result(vec![
            make_face(1, "planar", 10.0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            make_face(2, "planar", 10.0, [5.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            make_face(3, "planar", 10.0, [10.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ]);
        let query = TopoQuery {
            filters: vec![],
            tie_break: Some(TieBreak::NearestTo {
                point: [9.0, 0.0, 0.0],
            }),
        };
        let result = resolve_by_query(&op, &query, TopoKind::Face, ResolvePolicy::Strict).unwrap();
        assert_eq!(result.kernel_id, KernelId(3));
    }

    #[test]
    fn query_tie_break_smallest_index() {
        let op = make_op_result(vec![
            make_face(1, "planar", 10.0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            make_face(2, "planar", 10.0, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ]);
        let query = TopoQuery {
            filters: vec![],
            tie_break: Some(TieBreak::SmallestIndex),
        };
        let result = resolve_by_query(&op, &query, TopoKind::Face, ResolvePolicy::Strict).unwrap();
        assert_eq!(result.kernel_id, KernelId(1));
    }

    #[test]
    fn query_no_match_strict_errors() {
        let op = make_op_result(vec![make_face(
            1,
            "planar",
            10.0,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
        )]);
        let query = TopoQuery {
            filters: vec![Filter::SurfaceType {
                surface_type: "cylindrical".to_string(),
            }],
            tie_break: None,
        };
        let result = resolve_by_query(&op, &query, TopoKind::Face, ResolvePolicy::Strict);
        assert!(result.is_err());
    }

    #[test]
    fn query_no_match_best_effort_falls_back() {
        let op = make_op_result(vec![make_face(
            1,
            "planar",
            10.0,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
        )]);
        let query = TopoQuery {
            filters: vec![Filter::SurfaceType {
                surface_type: "cylindrical".to_string(),
            }],
            tie_break: None,
        };
        let result =
            resolve_by_query(&op, &query, TopoKind::Face, ResolvePolicy::BestEffort).unwrap();
        assert_eq!(result.kernel_id, KernelId(1));
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn query_kind_mismatch_skipped() {
        let op = make_op_result(vec![make_face(
            1,
            "planar",
            10.0,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
        )]);
        let query = TopoQuery {
            filters: vec![],
            tie_break: None,
        };
        // Looking for Edge, but only Face exists
        let result = resolve_by_query(&op, &query, TopoKind::Edge, ResolvePolicy::Strict);
        assert!(result.is_err());
    }

    #[test]
    fn query_empty_filters_matches_all_of_kind() {
        let op = make_op_result(vec![
            make_face(1, "planar", 10.0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            make_face(2, "cylindrical", 20.0, [1.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
        ]);
        let query = TopoQuery {
            filters: vec![],
            tie_break: Some(TieBreak::LargestArea),
        };
        let result = resolve_by_query(&op, &query, TopoKind::Face, ResolvePolicy::Strict).unwrap();
        assert_eq!(result.kernel_id, KernelId(2)); // largest area
    }

    // --- N0 defect 2: a fingerprint that cannot distinguish must REFUSE ---
    // `specs/agent_mechanical_design.md` §5.1. Both of these used to bind to
    // the FIRST created face and carry a "0.0%" warning, under BestEffort AND
    // (for the tie) under Strict — the ICR-3 limit.

    /// An index-only signature (`adjacency_hash` and nothing else) carries no
    /// geometry: `signature_similarity` never reads `adjacency_hash`, so every
    /// candidate scores 0.0 and the "best" match is whichever face came first.
    /// Refused now, under either policy.
    #[test]
    fn signature_with_no_geometry_refuses_under_both_policies() {
        let op = make_op_result(vec![
            make_face(1, "planar", 10.0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            make_face(2, "planar", 20.0, [0.0, 0.0, 5.0], [0.0, 0.0, -1.0]),
        ]);
        let index_only = TopoSignature {
            adjacency_hash: Some(1),
            ..TopoSignature::empty()
        };
        for policy in [ResolvePolicy::BestEffort, ResolvePolicy::Strict] {
            let err = resolve_by_signature(&op, &index_only, TopoKind::Face, policy)
                .expect_err("a geometry-free signature must not bind");
            assert!(
                err.resolution_text()
                    .is_some_and(|r| r.contains("carries no geometry")),
                "{policy:?}: want a typed geometry-free refusal, got {err:?}"
            );
        }
    }

    /// Two candidates the fingerprint fits equally well: nothing in the
    /// reference distinguishes them, so it names both and refuses rather than
    /// picking the one that happens to be first.
    #[test]
    fn equally_matching_candidates_refuse_and_name_themselves() {
        let twin = |id| make_face(id, "planar", 10.0, [1.0, 2.0, 3.0], [0.0, 0.0, 1.0]);
        let op = make_op_result(vec![
            twin(7),
            twin(9),
            make_face(11, "cylindrical", 99.0, [9.0, 9.0, 9.0], [1.0, 0.0, 0.0]),
        ]);
        let target = twin(7).signature;
        for policy in [ResolvePolicy::BestEffort, ResolvePolicy::Strict] {
            let err = resolve_by_signature(&op, &target, TopoKind::Face, policy)
                .expect_err("an ambiguous fingerprint must not bind");
            match &err {
                EngineError::ReferenceAmbiguous { candidates, .. } => {
                    assert_eq!(candidates, &vec![7u64, 9u64], "{policy:?}: {err:?}");
                }
                other => panic!("{policy:?}: want ReferenceAmbiguous, got {other:?}"),
            }
        }
    }

    // --- Pid-selector resolution (drawings spec D0 §4 item 4) ------------

    /// A kernel that reports exactly the identity map the test hands it.
    struct PidIntrospect {
        /// `(kernel id, pid, root pid)` for entities of kind `kind`.
        entities: Vec<(KernelId, u64, u64)>,
        kind: TopoKind,
    }
    impl KernelIntrospect for PidIntrospect {
        fn all_entity_pids(
            &self,
            _: &KernelSolidHandle,
            kind: TopoKind,
        ) -> Vec<(KernelId, waffle_types::kernel::EntityPid)> {
            if kind != self.kind {
                return Vec::new();
            }
            self.entities
                .iter()
                .map(|&(id, pid, root_pid)| (id, waffle_types::kernel::EntityPid { pid, root_pid }))
                .collect()
        }
        fn list_faces(&self, _: &KernelSolidHandle) -> Vec<KernelId> {
            vec![]
        }
        fn list_edges(&self, _: &KernelSolidHandle) -> Vec<KernelId> {
            vec![]
        }
        fn list_vertices(&self, _: &KernelSolidHandle) -> Vec<KernelId> {
            vec![]
        }
        fn face_edges(&self, _: KernelId) -> Vec<KernelId> {
            vec![]
        }
        fn edge_faces(&self, _: KernelId) -> Vec<KernelId> {
            vec![]
        }
        fn edge_vertices(&self, _: KernelId) -> (KernelId, KernelId) {
            (KernelId(0), KernelId(0))
        }
        fn face_neighbors(&self, _: KernelId) -> Vec<KernelId> {
            vec![]
        }
        fn compute_signature(&self, _: KernelId, _: TopoKind) -> TopoSignature {
            TopoSignature::empty()
        }
        fn compute_all_signatures(
            &self,
            _: &KernelSolidHandle,
            _: TopoKind,
        ) -> Vec<(KernelId, TopoSignature)> {
            vec![]
        }
    }

    fn pid_ref(feature_id: Uuid, pid: u64, root_pid: u64, policy: ResolvePolicy) -> GeomRef {
        GeomRef {
            kind: TopoKind::Edge,
            anchor: Anchor::FeatureOutput {
                feature_id,
                output_key: OutputKey::Main,
            },
            selector: Selector::Pid { pid, root_pid },
            policy,
            scope: None,
        }
    }

    fn pid_fixture(
        entities: Vec<(KernelId, u64, u64)>,
    ) -> (Uuid, HashMap<Uuid, OpResult>, PidIntrospect) {
        let fid = Uuid::new_v4();
        let mut results = HashMap::new();
        results.insert(fid, op_with_body());
        (
            fid,
            results,
            PidIntrospect {
                entities,
                kind: TopoKind::Edge,
            },
        )
    }

    #[test]
    fn pid_resolves_to_its_own_entity() {
        let (fid, results, k) =
            pid_fixture(vec![(KernelId(10), 7001, 7001), (KernelId(11), 7002, 7002)]);
        let got = resolve_geom_ref_live(
            &pid_ref(fid, 7002, 7002, ResolvePolicy::Strict),
            &results,
            &k,
        )
        .expect("pid resolves");
        assert_eq!(got.kernel_id, KernelId(11));
        assert!(
            got.warnings.is_empty(),
            "an exact match warns about nothing"
        );
    }

    #[test]
    fn a_rebuilt_entity_resolves_through_its_lineage_root_and_says_so() {
        // The stored pid is gone; one entity still carries its root (the
        // face a later boolean rebuilt).
        let (fid, results, k) =
            pid_fixture(vec![(KernelId(10), 9100, 7001), (KernelId(11), 9200, 7002)]);
        let got = resolve_geom_ref_live(
            &pid_ref(fid, 7001, 7001, ResolvePolicy::Strict),
            &results,
            &k,
        )
        .expect("resolves through the root");
        assert_eq!(got.kernel_id, KernelId(10));
        assert!(
            got.warnings.iter().any(|w| w.contains("lineage root")),
            "the root fallback must be reported, got {:?}",
            got.warnings
        );
    }

    /// The root is a CROSS-CHECK, not a second choice: an entity carrying
    /// the recorded number but a different lineage root is not this
    /// reference's entity, and the recorded root answers instead.
    ///
    /// This is the counter-reuse case (D0 item 1b): a boolean output's pid is
    /// counter-allocated, and a reopened document re-mints the number onto
    /// other geometry. Measured end to end in
    /// `crates/wasm-bridge/tests/tool_names.rs`
    /// (`a_name_on_a_boolean_output_face_does_not_move_after_a_reload`).
    #[test]
    fn a_recycled_pid_is_not_a_match_and_the_recorded_root_answers() {
        // 22 is now held by geometry rooted at 8800; the entity that was
        // recorded as (22, 7001) has been re-minted as 9100 but kept its root.
        let (fid, results, k) =
            pid_fixture(vec![(KernelId(10), 22, 8800), (KernelId(11), 9100, 7001)]);
        let got =
            resolve_geom_ref_live(&pid_ref(fid, 22, 7001, ResolvePolicy::Strict), &results, &k)
                .expect("the recorded root still names it");
        assert_eq!(
            got.kernel_id,
            KernelId(11),
            "the number alone must not win: {:?}",
            got.warnings
        );
        assert!(
            got.warnings
                .iter()
                .any(|w| w.contains("re-minted onto something else")),
            "the caller is told the id now belongs elsewhere, got {:?}",
            got.warnings
        );
        assert!(
            got.warnings.iter().any(|w| w.contains("lineage root")),
            "and which door answered, got {:?}",
            got.warnings
        );
    }

    /// Same reuse, but nothing descends from the recorded root any more: a
    /// typed refusal that names the reuse, never the entity holding the
    /// recycled number.
    #[test]
    fn a_recycled_pid_with_no_surviving_root_is_refused() {
        for policy in [ResolvePolicy::Strict, ResolvePolicy::BestEffort] {
            let (fid, results, k) = pid_fixture(vec![(KernelId(10), 22, 8800)]);
            let err = resolve_geom_ref_live(&pid_ref(fid, 22, 7001, policy), &results, &k)
                .expect_err("a recycled number is not a match");
            let reason = err
                .resolution_text()
                .unwrap_or_else(|| panic!("{policy:?}: want a resolution refusal, got {err:?}"));
            assert!(
                reason.contains("re-minted onto something else"),
                "{policy:?}: the refusal says why, got {reason}"
            );
            // N2: and it is CLASSIFIED — a host branches on `PidGone` with the
            // numbers, rather than reading the sentence above.
            assert!(
                matches!(
                    err.resolution_reason(),
                    Some(ResolutionReason::PidGone {
                        pid: 22,
                        root_pid: 7001,
                        last_seen_feature: Some(f),
                    }) if *f == fid
                ),
                "{policy:?}: want PidGone{{22, 7001, {fid}}}, got {:?}",
                err.resolution_reason()
            );
        }
    }

    /// The property the whole selector exists for: a reference whose entity
    /// is gone FAILS, under BestEffort as well as Strict. Every other
    /// selector rebinds here; this one must not.
    #[test]
    fn a_vanished_pid_is_refused_under_both_policies() {
        for policy in [ResolvePolicy::Strict, ResolvePolicy::BestEffort] {
            let (fid, results, k) =
                pid_fixture(vec![(KernelId(10), 7001, 7001), (KernelId(11), 7002, 7002)]);
            let err = resolve_geom_ref_live(&pid_ref(fid, 4242, 4242, policy), &results, &k)
                .expect_err("a vanished pid must not rebind");
            let reason = err
                .resolution_text()
                .unwrap_or_else(|| panic!("{policy:?}: wrong error {err:?}"));
            assert!(
                reason.contains("no longer exists"),
                "{policy:?}: unexpected reason {reason}"
            );
            assert!(
                matches!(
                    err.resolution_reason(),
                    Some(ResolutionReason::PidGone {
                        pid: 4242,
                        root_pid: 4242,
                        ..
                    })
                ),
                "{policy:?}: want PidGone, got {:?}",
                err.resolution_reason()
            );
        }
    }

    /// The far side of the same rule: a fingerprint that fits ONE candidate
    /// best still resolves, and a near-miss still resolves with a warning.
    #[test]
    fn a_distinguishing_signature_still_resolves() {
        let op = make_op_result(vec![
            make_face(1, "planar", 10.0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            make_face(2, "planar", 10.0, [0.0, 0.0, 5.0], [0.0, 0.0, -1.0]),
        ]);
        let exact = make_face(2, "planar", 10.0, [0.0, 0.0, 5.0], [0.0, 0.0, -1.0]).signature;
        let hit = resolve_by_signature(&op, &exact, TopoKind::Face, ResolvePolicy::Strict)
            .expect("exact match");
        assert_eq!(hit.kernel_id, KernelId(2));
        assert!(hit.warnings.is_empty(), "{:?}", hit.warnings);

        let perturbed = make_face(2, "planar", 10.2, [0.0, 0.0, 5.01], [0.0, 0.0, -1.0]).signature;
        let hit = resolve_by_signature(&op, &perturbed, TopoKind::Face, ResolvePolicy::Strict)
            .expect("near match");
        assert_eq!(hit.kernel_id, KernelId(2));
    }

    #[test]
    fn a_split_root_is_refused_rather_than_guessed() {
        // The pid is gone and TWO entities now share its root: the geometry
        // was split, and nothing in the reference says which half it meant.
        let (fid, results, k) =
            pid_fixture(vec![(KernelId(10), 9100, 7001), (KernelId(11), 9101, 7001)]);
        let err = resolve_geom_ref_live(
            &pid_ref(fid, 7001, 7001, ResolvePolicy::BestEffort),
            &results,
            &k,
        )
        .expect_err("an ambiguous root must not be guessed");
        let reason = err
            .resolution_text()
            .unwrap_or_else(|| panic!("wrong error {err:?}"));
        assert!(reason.contains("split"), "unexpected reason {reason}");
        // N2: a split root is an AMBIGUITY, not an absence — and the refusal
        // names both halves, which is what an agent needs to pick one.
        match err.resolution_reason() {
            Some(ResolutionReason::Ambiguous { candidates }) => {
                assert_eq!(candidates, &vec![10, 11], "both halves are named");
            }
            other => panic!("want Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn a_kernel_without_identity_says_so_instead_of_reporting_absence() {
        // An empty identity map means "no identity available here", which is
        // a different finding from "your entity is gone" — a caller that
        // confused them would blame the document for a kernel limitation.
        let (fid, results, _) = pid_fixture(vec![]);
        let k = PidIntrospect {
            entities: vec![],
            kind: TopoKind::Edge,
        };
        let err = resolve_geom_ref_live(
            &pid_ref(fid, 7001, 7001, ResolvePolicy::Strict),
            &results,
            &k,
        )
        .expect_err("no identity map");
        let reason = err
            .resolution_text()
            .unwrap_or_else(|| panic!("wrong error {err:?}"));
        assert!(
            reason.contains("no persistent ids"),
            "unexpected reason {reason}"
        );
    }

    #[test]
    fn a_pid_selector_without_the_kernel_refuses_loudly() {
        // `resolve_geom_ref` has no introspection, so it cannot answer a
        // question about the body's current entities. It must say that, not
        // fall through to a different selector.
        let (fid, results, _) = pid_fixture(vec![]);
        let err = resolve_geom_ref(&pid_ref(fid, 7001, 7001, ResolvePolicy::Strict), &results)
            .expect_err("needs the live kernel");
        let reason = err
            .resolution_text()
            .unwrap_or_else(|| panic!("wrong error {err:?}"));
        assert!(reason.contains("live kernel"), "unexpected reason {reason}");
    }

    /// A pid is unique only WITHIN one body, so the first-body fallback that
    /// every other live selector uses would be a silent body substitution
    /// here — and the same number can name a different edge in a sibling body
    /// split out of the same operation. Refuse instead.
    #[test]
    fn a_pid_whose_output_key_is_gone_refuses_rather_than_taking_another_body() {
        let (fid, results, k) = pid_fixture(vec![(KernelId(10), 7001, 7001)]);
        let mut r = pid_ref(fid, 7001, 7001, ResolvePolicy::BestEffort);
        // The fixture's only output is `Main`; ask for a body that is gone.
        r.anchor = Anchor::FeatureOutput {
            feature_id: fid,
            output_key: OutputKey::Body { index: 2 },
        };
        let err = resolve_geom_ref_live(&r, &results, &k)
            .expect_err("a missing output must not resolve against a sibling body");
        let reason = err
            .resolution_text()
            .unwrap_or_else(|| panic!("wrong error {err:?}"));
        assert!(
            reason.contains("unique within one body"),
            "unexpected reason {reason}"
        );
    }

    /// The same shape through a Position selector KEEPS the fallback: that is
    /// long-standing viewport-picking behaviour and is not what this change
    /// is about.
    #[test]
    fn a_position_reference_still_falls_back_to_the_first_body() {
        let fid = Uuid::new_v4();
        let mut results = HashMap::new();
        results.insert(fid, op_with_body());
        let mut r = pos_ref(fid, 0.0, 0.0, 0.0, ResolvePolicy::Strict);
        r.anchor = Anchor::FeatureOutput {
            feature_id: fid,
            output_key: OutputKey::Body { index: 2 },
        };
        let kernel = FakeIntrospect { verts: vec![] };
        let err = resolve_by_position(&r, &results, &kernel, [0.0, 0.0, 0.0])
            .expect_err("the stub kernel lists no vertices");
        let reason = err
            .resolution_text()
            .unwrap_or_else(|| panic!("wrong error {err:?}"));
        assert!(
            reason.contains("no Vertex entities"),
            "it reached the body and failed on its contents, not on the key: {reason}"
        );
    }

    #[test]
    fn a_pid_reference_scoped_into_another_tab_is_refused() {
        let (fid, results, k) = pid_fixture(vec![(KernelId(10), 7001, 7001)]);
        let mut r = pid_ref(fid, 7001, 7001, ResolvePolicy::Strict);
        r.scope = Some(waffle_types::RefScope::in_assembly("tab-1", vec![]));
        assert!(
            resolve_geom_ref_live(&r, &results, &k).is_err(),
            "a scoped reference resolves only through the open assembly context"
        );
    }
}
