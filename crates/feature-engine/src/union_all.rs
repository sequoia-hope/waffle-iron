//! `Operation::UnionAll` — many-body union as one feature
//! (`specs/b4_balanced_union.md`).
//!
//! Every live body of the part (or an explicit list) is folded into
//! connected lumps by a BALANCED tree of ordinary pairwise unions
//! (`modeling_ops::execute_boolean`, the full pipeline every time), with an
//! exact bounding-box gate that never runs the kernel on a pair whose
//! conservative boxes are disjoint. The fold primitive
//! ([`fold_into`]) is shared with the pattern's `Add` / `Intersect`
//! (`crate::pattern`), which keeps its chain order and gains the gate.

use std::collections::{HashMap, HashSet};

use modeling_ops::{execute_boolean, BodyOutput, BooleanKind, KernelBundle, OpResult};
use uuid::Uuid;
use waffle_types::kernel::KernelSolidHandle;
use waffle_types::{Anchor, OutputKey, ResolvePolicy};

use crate::progress::{self, ProgressEvent};
use crate::rebuild::find_solid_handle;
use crate::types::{EngineError, Feature, FeatureTree, Operation, UnionAllParams, UnionTargets};

/// A conservative axis-aligned box, `(lo, hi)`.
pub(crate) type Aabb = ([f64; 3], [f64; 3]);

/// A body in the fold: its handle and its conservative box (`None` ⇒ the
/// kernel could not bound it; treated as overlapping everything).
#[derive(Debug, Clone)]
pub(crate) struct Lump {
    pub handle: KernelSolidHandle,
    pub aabb: Option<Aabb>,
}

impl Lump {
    pub(crate) fn of(kb: &dyn KernelBundle, handle: KernelSolidHandle) -> Self {
        let aabb = kb.as_introspect().solid_aabb(&handle);
        Lump { handle, aabb }
    }
}

/// Closed-interval overlap on every axis; an unknown box overlaps.
pub(crate) fn may_touch(a: Option<&Aabb>, b: Option<&Aabb>) -> bool {
    match (a, b) {
        (Some((lo_a, hi_a)), Some((lo_b, hi_b))) => {
            (0..3).all(|k| lo_a[k] <= hi_b[k] && lo_b[k] <= hi_a[k])
        }
        _ => true,
    }
}

/// Extend `acc`'s provenance and warnings with a boolean's.
pub(crate) fn absorb(acc: &mut OpResult, res: &OpResult) {
    acc.provenance
        .created
        .extend(res.provenance.created.iter().cloned());
    acc.provenance
        .deleted
        .extend(res.provenance.deleted.iter().cloned());
    acc.provenance
        .role_assignments
        .extend(res.provenance.role_assignments.iter().cloned());
    acc.diagnostics
        .warnings
        .extend(res.diagnostics.warnings.iter().cloned());
}

/// `Main` for the first body, `Body{index}` for the rest, in order.
pub(crate) fn rekey(bodies: Vec<BodyOutput>) -> Vec<(OutputKey, BodyOutput)> {
    bodies
        .into_iter()
        .enumerate()
        .map(|(i, b)| {
            let key = if i == 0 {
                OutputKey::Main
            } else {
                OutputKey::Body { index: i }
            };
            (key, b)
        })
        .collect()
}

/// Fold `body` into `lumps` (pairwise-disjoint connected lumps, insertion
/// order). It merges with every existing lump it touches — the scan
/// continues with the merged body, so one body can bridge two lumps — and a
/// body disjoint from every lump becomes its own lump. A pair whose boxes
/// are disjoint never reaches the kernel; a pair the kernel splits into two
/// lumps keeps the originals (disjoint despite overlapping boxes).
/// `on_union` runs once per union the kernel RAN. `label` flavors the empty-
/// result error.
pub(crate) fn fold_into(
    kb: &mut dyn KernelBundle,
    lumps: &mut Vec<Lump>,
    body: Lump,
    prov: &mut OpResult,
    label: &str,
    on_union: &mut dyn FnMut(),
) -> Result<(), EngineError> {
    let mut cur = body;
    let mut i = 0;
    while i < lumps.len() {
        if !may_touch(lumps[i].aabb.as_ref(), cur.aabb.as_ref()) {
            i += 1;
            continue;
        }
        let res = execute_boolean(kb, &lumps[i].handle, &cur.handle, BooleanKind::Union)?;
        on_union();
        match res.outputs.len() {
            0 => {
                return Err(EngineError::ResolutionFailed {
                    reason: format!("{label}: a union produced no solid"),
                });
            }
            1 => {
                absorb(prov, &res);
                let handle = res.outputs.into_iter().next().unwrap().1.handle;
                cur = Lump::of(kb, handle);
                lumps.remove(i);
                // continue at the same index (next un-scanned lump)
            }
            // Disjoint: the split result duplicates the operands — keep
            // the originals.
            _ => i += 1,
        }
    }
    lumps.push(cur);
    Ok(())
}

/// Balanced many-body union: split, fold each half, merge the halves
/// (`specs/b4_balanced_union.md` §2.2). Lump 0 is the first body's lump.
pub(crate) fn union_balanced(
    kb: &mut dyn KernelBundle,
    bodies: Vec<Lump>,
    prov: &mut OpResult,
    label: &str,
    on_union: &mut dyn FnMut(),
) -> Result<Vec<Lump>, EngineError> {
    if bodies.len() <= 1 {
        return Ok(bodies);
    }
    let mut left = bodies;
    let right = left.split_off(left.len() / 2);
    let mut lumps = union_balanced(kb, left, prov, label, on_union)?;
    for r in union_balanced(kb, right, prov, label, on_union)? {
        fold_into(kb, &mut lumps, r, prov, label, on_union)?;
    }
    Ok(lumps)
}

/// One resolved source body.
#[derive(Debug, Clone)]
pub(crate) struct Source {
    pub feature_id: Uuid,
    pub handle: KernelSolidHandle,
}

/// The features whose bodies a feature's `All` target set would fold: every
/// active, unsuppressed, not-yet-consumed solid-bearing feature before it.
pub(crate) fn live_features_before<'a>(
    feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &'a FeatureTree,
    already_consumed: &HashSet<Uuid>,
) -> Vec<&'a Feature> {
    let active = tree.active_features();
    let current_idx = active
        .iter()
        .position(|f| f.id == feature.id)
        .unwrap_or(active.len());
    active[..current_idx]
        .iter()
        .filter(|f| !f.suppressed && !already_consumed.contains(&f.id))
        .filter(|f| {
            !matches!(
                &f.operation,
                Operation::Sketch { .. }
                    | Operation::Sketch3d { .. }
                    | Operation::DatumPlane { .. }
                    | Operation::MateConnector { .. }
            )
        })
        .filter(|f| {
            feature_results.get(&f.id).is_some_and(|r| {
                r.outputs
                    .iter()
                    .any(|(k, _)| matches!(k, OutputKey::Main | OutputKey::Body { .. }))
            })
        })
        .collect()
}

/// Resolve the feature's body set (spec §2.1). Warnings collect dropped
/// `BestEffort` bodies.
pub(crate) fn resolve_sources(
    feature: &Feature,
    params: &UnionAllParams,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    already_consumed: &HashSet<Uuid>,
    warnings: &mut Vec<String>,
) -> Result<Vec<Source>, EngineError> {
    let mut out = Vec::new();
    match &params.targets {
        UnionTargets::All => {
            for f in live_features_before(feature, feature_results, tree, already_consumed) {
                let Some(r) = feature_results.get(&f.id) else {
                    continue;
                };
                for (key, body) in &r.outputs {
                    if matches!(key, OutputKey::Main | OutputKey::Body { .. }) {
                        out.push(Source {
                            feature_id: f.id,
                            handle: body.handle.clone(),
                        });
                    }
                }
            }
        }
        UnionTargets::Selected { bodies } => {
            let mut seen: HashSet<(Uuid, OutputKey)> = HashSet::new();
            for gr in bodies {
                let Anchor::FeatureOutput {
                    feature_id,
                    output_key,
                } = &gr.anchor
                else {
                    return Err(EngineError::ResolutionFailed {
                        reason: "union all: a body must anchor to a feature output".into(),
                    });
                };
                if !seen.insert((*feature_id, output_key.clone())) {
                    return Err(EngineError::ResolutionFailed {
                        reason: format!(
                            "union all: body {output_key:?} of feature {feature_id} is listed twice"
                        ),
                    });
                }
                if *feature_id == feature.id {
                    return Err(EngineError::ResolutionFailed {
                        reason: "union all: a feature cannot union its own output".into(),
                    });
                }
                if already_consumed.contains(feature_id) {
                    let msg = format!(
                        "union all: body {output_key:?} of feature {feature_id} was already \
                         consumed by an earlier feature"
                    );
                    match gr.policy {
                        ResolvePolicy::Strict => {
                            return Err(EngineError::ResolutionFailed { reason: msg });
                        }
                        ResolvePolicy::BestEffort => {
                            warnings.push(format!("{msg}; dropped"));
                            continue;
                        }
                    }
                }
                match find_solid_handle(gr, feature_results) {
                    Ok(handle) => out.push(Source {
                        feature_id: *feature_id,
                        handle,
                    }),
                    Err(e) => match gr.policy {
                        ResolvePolicy::Strict => return Err(e),
                        ResolvePolicy::BestEffort => {
                            warnings.push(format!(
                                "union all: body {output_key:?} of feature {feature_id} could \
                                 not be resolved and was dropped: {e}"
                            ));
                        }
                    },
                }
            }
        }
    }
    Ok(out)
}

/// The feature ids a `UnionAll` consumes (in body order, deduplicated).
pub(crate) fn consumed_feature_ids(
    feature: &Feature,
    params: &UnionAllParams,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    already_consumed: &HashSet<Uuid>,
) -> Vec<Uuid> {
    let mut warnings = Vec::new();
    let mut out: Vec<Uuid> = Vec::new();
    if let Ok(sources) = resolve_sources(
        feature,
        params,
        feature_results,
        tree,
        already_consumed,
        &mut warnings,
    ) {
        for s in sources {
            if !out.contains(&s.feature_id) {
                out.push(s.feature_id);
            }
        }
    }
    out
}

/// The body whose custom name the union's `Main` inherits: the first listed
/// body (`Selected`), or `(first_consumed_feature, Main)` for `All` — the
/// caller passes that feature id from its `consumed_by` record.
pub(crate) fn name_source(
    params: &UnionAllParams,
    first_consumed: Option<Uuid>,
) -> Option<(Uuid, String)> {
    match &params.targets {
        UnionTargets::Selected { bodies } => bodies.first().and_then(|gr| {
            if let Anchor::FeatureOutput {
                feature_id,
                output_key,
            } = &gr.anchor
            {
                Some((*feature_id, FeatureTree::body_id(*feature_id, output_key)))
            } else {
                None
            }
        }),
        UnionTargets::All => {
            first_consumed.map(|fid| (fid, FeatureTree::body_id(fid, &OutputKey::Main)))
        }
    }
}

/// Execute a `UnionAll` feature.
pub(crate) fn execute(
    feature: &Feature,
    params: &UnionAllParams,
    kb: &mut dyn KernelBundle,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    already_consumed: &HashSet<Uuid>,
) -> Result<OpResult, EngineError> {
    let mut warnings = Vec::new();
    let sources = resolve_sources(
        feature,
        params,
        feature_results,
        tree,
        already_consumed,
        &mut warnings,
    )?;
    if sources.is_empty() {
        return Err(EngineError::ResolutionFailed {
            reason: "union all: no live bodies to union".into(),
        });
    }
    let n = sources.len();
    let mut prov = OpResult {
        outputs: Vec::new(),
        provenance: modeling_ops::Provenance {
            created: Vec::new(),
            deleted: Vec::new(),
            modified: Vec::new(),
            role_assignments: Vec::new(),
        },
        diagnostics: modeling_ops::Diagnostics::default(),
    };
    if n == 1 {
        warnings.push("union all: only one live body; passed through unchanged".into());
    }

    // Progress: one frame per union RUN, capped by the n − 1 a fully
    // connected set needs (disjoint pairs skip).
    let max_unions = n - 1;
    let mut done = 0usize;
    let report = |done: usize| {
        progress::report(&ProgressEvent {
            feature_id: feature.id,
            feature_name: feature.name.clone(),
            done,
            remaining: max_unions.saturating_sub(done),
            label: format!("union {done} of ≤ {max_unions} ({n} bodies)"),
        });
    };
    report(0);
    let bodies: Vec<Lump> = sources
        .iter()
        .map(|s| Lump::of(&*kb, s.handle.clone()))
        .collect();
    let lumps = union_balanced(kb, bodies, &mut prov, "union all", &mut || {
        done += 1;
        report(done);
    })?;
    progress::report(&ProgressEvent {
        feature_id: feature.id,
        feature_name: feature.name.clone(),
        done,
        remaining: 0,
        label: format!("union all: {done} union(s), {} body/bodies", lumps.len()),
    });

    prov.outputs = rekey(
        lumps
            .into_iter()
            .map(|l| BodyOutput {
                handle: l.handle,
                mesh: None,
                edges: None,
            })
            .collect(),
    );
    prov.diagnostics.warnings.extend(warnings);
    Ok(prov)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(lo: [f64; 3], hi: [f64; 3]) -> Aabb {
        (lo, hi)
    }

    #[test]
    fn may_touch_is_closed_interval_overlap_and_unknown_boxes_touch() {
        let a = bx([0.0; 3], [1.0; 3]);
        let b = bx([1.0, 0.0, 0.0], [2.0, 1.0, 1.0]); // touching face
        let c = bx([1.5, 0.0, 0.0], [2.0, 1.0, 1.0]);
        assert!(may_touch(Some(&a), Some(&b)));
        assert!(!may_touch(Some(&a), Some(&c)));
        assert!(may_touch(None, Some(&c)));
        assert!(may_touch(Some(&a), None));
    }

    #[test]
    fn name_source_is_first_listed_body_or_first_consumed_main() {
        let fid = Uuid::new_v4();
        let all = UnionAllParams {
            targets: UnionTargets::All,
        };
        assert_eq!(
            name_source(&all, Some(fid)),
            Some((fid, FeatureTree::body_id(fid, &OutputKey::Main)))
        );
        assert_eq!(name_source(&all, None), None);
        let gr = waffle_types::GeomRef {
            kind: waffle_types::TopoKind::Solid,
            anchor: Anchor::FeatureOutput {
                feature_id: fid,
                output_key: OutputKey::Body { index: 2 },
            },
            selector: waffle_types::Selector::Role {
                role: waffle_types::Role::EndCapPositive,
                index: 0,
            },
            policy: ResolvePolicy::Strict,
            scope: None,
        };
        let sel = UnionAllParams {
            targets: UnionTargets::Selected { bodies: vec![gr] },
        };
        assert_eq!(
            name_source(&sel, None),
            Some((
                fid,
                FeatureTree::body_id(fid, &OutputKey::Body { index: 2 })
            ))
        );
    }
}
