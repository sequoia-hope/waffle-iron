//! Circular and linear patterns (`specs/custom_features_and_modeling_roadmap.md`
//! §B1): rigid copies of seed BODIES at computed placements, optionally
//! combined into target bodies.
//!
//! ## Semantics
//!
//! - A pattern instances bodies, not features: the seed's operations are not
//!   re-executed; its body is copied through `Kernel::transform_body`, which
//!   is exact on analytic geometry.
//! - The pattern feature takes CUSTODY of its seeds: their producing features
//!   are consumed, and instance 0 (the seed body itself) is re-emitted as the
//!   pattern's `Main` output, so one node owns every instance. Suppressing or
//!   rolling back past the pattern restores the seed to its own feature.
//! - Every reference is explicit (seeds, targets, an `AxisRef`); a pattern
//!   never picks a body by tree position, so it is never position-dependent
//!   for incremental rebuild.
//! - Combine (`Add`/`Cut`/`Intersect`) is against explicit targets only.
//!   `Add` folds targets and instances into connected lumps (an instance
//!   disjoint from everything stays its own body — never dropped); `Cut`
//!   subtracts every instance from every target piece; `Intersect` keeps
//!   each target ∩ (the union of the instances).
//!
//! ## P10 posture
//!
//! A seed or target that names an output an EARLIER feature already consumed
//! is refused (Strict) or dropped with a warning (BestEffort) — the
//! consumed-target duplication defect recorded in
//! `projects/06-feature-engine/PLAN.md` is not repeated here. A seed that is
//! also a target is refused. An axis pick with no derivable axis is a typed
//! error, never a default axis.

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

use modeling_ops::{
    execute_boolean, execute_pattern_instances, BooleanKind, KernelBundle, OpResult, PatternSeed,
};
use uuid::Uuid;
use waffle_types::kernel::units::TAU_WORK;
use waffle_types::kernel::{KernelIntrospect, KernelSolidHandle, RigidPlacement};
use waffle_types::{Anchor, GeomRef, OutputKey};

use crate::assembly::AxialAnchor;
use crate::connector::resolve_connector_frame;
use crate::rebuild::{
    carry_untargeted_named, find_solid_handle, resolve_combine_targets, unit_normal,
};
use crate::types::{
    normalize_pattern_combine, AxisRef, CombineMode, EffectiveCombine, EngineError, Feature,
    FeatureTree, LinearSecondDirection, PatternCircularParams, PatternLinearParams,
};
use crate::union_all::{absorb, fold_into, rekey, Lump};

/// Geometry budget: more instances than this is a runaway parameter, not a
/// design (the spec's "fails loud in milliseconds" rule).
pub const MAX_PATTERN_INSTANCES: usize = 10_000;

/// Either pattern's parameters, borrowed.
#[derive(Debug, Clone, Copy)]
pub(crate) enum PatternSpec<'a> {
    Circular(&'a PatternCircularParams),
    Linear(&'a PatternLinearParams),
}

impl<'a> PatternSpec<'a> {
    fn seeds(&self) -> &'a [GeomRef] {
        match self {
            PatternSpec::Circular(p) => &p.seeds,
            PatternSpec::Linear(p) => &p.seeds,
        }
    }

    fn skip(&self) -> &'a [u32] {
        match self {
            PatternSpec::Circular(p) => &p.skip,
            PatternSpec::Linear(p) => &p.skip,
        }
    }

    pub(crate) fn combine(&self) -> EffectiveCombine {
        match self {
            PatternSpec::Circular(p) => normalize_pattern_combine(p.combine, &p.targets),
            PatternSpec::Linear(p) => normalize_pattern_combine(p.combine, &p.targets),
        }
    }

    fn label(&self) -> &'static str {
        match self {
            PatternSpec::Circular(_) => "circular pattern",
            PatternSpec::Linear(_) => "linear pattern",
        }
    }
}

fn anchor_of(gr: &GeomRef) -> Option<(Uuid, OutputKey)> {
    match &gr.anchor {
        Anchor::FeatureOutput {
            feature_id,
            output_key,
        } => Some((*feature_id, output_key.clone())),
        _ => None,
    }
}

/// Resolve an [`AxisRef`] to `(origin, unit direction)`.
pub(crate) fn resolve_axis(
    axis: &AxisRef,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn KernelIntrospect,
    what: &str,
) -> Result<([f64; 3], [f64; 3]), EngineError> {
    let (origin, direction) = match axis {
        AxisRef::Explicit { origin, direction } => (*origin, *direction),
        AxisRef::Entity { geom_ref } => {
            let (frame, _geometry) =
                resolve_connector_frame(geom_ref, feature_results, introspect, AxialAnchor::Middle)
                    .map_err(|e| EngineError::ResolutionFailed {
                        reason: format!("{what}: {e}"),
                    })?;
            (frame.origin, frame.z_axis)
        }
    };
    if origin
        .iter()
        .chain(direction.iter())
        .any(|v| !v.is_finite())
    {
        return Err(EngineError::ResolutionFailed {
            reason: format!("{what}: axis has a non-finite component"),
        });
    }
    let len =
        (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2])
            .sqrt();
    if len < TAU_WORK {
        return Err(EngineError::ResolutionFailed {
            reason: format!("{what}: axis direction is zero-length"),
        });
    }
    Ok((origin, unit_normal(direction)))
}

fn check_count(count: u32, what: &str) -> Result<usize, EngineError> {
    if count < 2 {
        return Err(EngineError::ResolutionFailed {
            reason: format!(
                "{what}: count must be at least 2 (the seed plus one instance); got {count}"
            ),
        });
    }
    Ok(count as usize)
}

/// The placements of every instance (index 0 = identity, the seed).
pub(crate) fn placements(
    spec: PatternSpec<'_>,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn KernelIntrospect,
) -> Result<Vec<RigidPlacement>, EngineError> {
    let out = match spec {
        PatternSpec::Circular(p) => {
            let count = check_count(p.count, "circular pattern")?;
            if !p.angle_deg.is_finite() || p.angle_deg.abs() < TAU_WORK {
                return Err(EngineError::ResolutionFailed {
                    reason: format!(
                        "circular pattern: angle must be a non-zero finite sweep in degrees; got {}",
                        p.angle_deg
                    ),
                });
            }
            let (origin, axis) = resolve_axis(
                &p.axis,
                feature_results,
                introspect,
                "circular pattern axis",
            )?;
            // Equal spacing: a full turn divides by `count` so the last
            // instance does not land on the seed; a partial sweep puts the
            // last instance exactly at `angle_deg`.
            let full_turn = (p.angle_deg.abs() - 360.0).abs() < 1e-9;
            let step = if full_turn {
                p.angle_deg.signum() * 2.0 * PI / count as f64
            } else {
                p.angle_deg.to_radians() / (count - 1) as f64
            };
            let mut out = Vec::with_capacity(count);
            out.push(RigidPlacement::IDENTITY);
            for i in 1..count {
                out.push(RigidPlacement::rotation_about(
                    origin,
                    axis,
                    step * i as f64,
                ));
            }
            out
        }
        PatternSpec::Linear(p) => {
            let count = check_count(p.count, "linear pattern")?;
            let (_, dir) = resolve_axis(
                &p.direction,
                feature_results,
                introspect,
                "linear pattern direction",
            )?;
            check_spacing(p.spacing, "linear pattern")?;
            let second: Option<(usize, [f64; 3], f64)> = match &p.second {
                None => None,
                Some(LinearSecondDirection {
                    direction,
                    count,
                    spacing,
                    ..
                }) => {
                    let n = check_count(*count, "linear pattern second direction")?;
                    let (_, d2) = resolve_axis(
                        direction,
                        feature_results,
                        introspect,
                        "linear pattern second direction",
                    )?;
                    check_spacing(*spacing, "linear pattern second direction")?;
                    // Parallel second direction collapses the grid onto a line
                    // (coincident instances) — refuse rather than overlap.
                    let cross = [
                        dir[1] * d2[2] - dir[2] * d2[1],
                        dir[2] * d2[0] - dir[0] * d2[2],
                        dir[0] * d2[1] - dir[1] * d2[0],
                    ];
                    let sin =
                        (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
                    if sin < 1e-9 {
                        return Err(EngineError::ResolutionFailed {
                            reason: "linear pattern: the second direction is parallel to the \
                                     first; the grid would collapse onto a line"
                                .into(),
                        });
                    }
                    Some((n, d2, *spacing))
                }
            };
            let cols = second.map_or(1, |(n, _, _)| n);
            let mut out = Vec::with_capacity(count * cols);
            for j in 0..cols {
                for i in 0..count {
                    if i == 0 && j == 0 {
                        out.push(RigidPlacement::IDENTITY);
                        continue;
                    }
                    let mut t = [
                        dir[0] * p.spacing * i as f64,
                        dir[1] * p.spacing * i as f64,
                        dir[2] * p.spacing * i as f64,
                    ];
                    if let Some((_, d2, s2)) = second {
                        t[0] += d2[0] * s2 * j as f64;
                        t[1] += d2[1] * s2 * j as f64;
                        t[2] += d2[2] * s2 * j as f64;
                    }
                    out.push(RigidPlacement::translation(t));
                }
            }
            out
        }
    };
    if out.len() > MAX_PATTERN_INSTANCES {
        return Err(EngineError::ResolutionFailed {
            reason: format!(
                "{}: {} instances exceeds the pattern budget of {MAX_PATTERN_INSTANCES}",
                spec.label(),
                out.len()
            ),
        });
    }
    Ok(out)
}

fn check_spacing(spacing: f64, what: &str) -> Result<(), EngineError> {
    if !spacing.is_finite() || spacing.abs() < TAU_WORK {
        return Err(EngineError::ResolutionFailed {
            reason: format!("{what}: spacing must be a non-zero finite length; got {spacing}"),
        });
    }
    Ok(())
}

/// Resolve the seed bodies: every seed must resolve (a pattern without its
/// seed is meaningless, so `ResolvePolicy` does not soften this), must
/// anchor to a feature output, and must not already be consumed.
fn resolve_seeds(
    spec: PatternSpec<'_>,
    feature_results: &HashMap<Uuid, OpResult>,
    already_consumed: &HashSet<Uuid>,
) -> Result<Vec<(Uuid, OutputKey, KernelSolidHandle)>, EngineError> {
    if spec.seeds().is_empty() {
        return Err(EngineError::ResolutionFailed {
            reason: format!("{}: no seed bodies", spec.label()),
        });
    }
    let mut out = Vec::with_capacity(spec.seeds().len());
    let mut seen: HashSet<(Uuid, OutputKey)> = HashSet::new();
    for gr in spec.seeds() {
        let (fid, key) = anchor_of(gr).ok_or_else(|| EngineError::ResolutionFailed {
            reason: format!("{}: a seed must anchor to a feature output", spec.label()),
        })?;
        if already_consumed.contains(&fid) {
            return Err(EngineError::ResolutionFailed {
                reason: format!(
                    "{}: seed body {key:?} of feature {fid} was already consumed by an \
                     earlier feature",
                    spec.label()
                ),
            });
        }
        if !seen.insert((fid, key.clone())) {
            return Err(EngineError::ResolutionFailed {
                reason: format!(
                    "{}: seed body {key:?} of feature {fid} is listed twice",
                    spec.label()
                ),
            });
        }
        let handle =
            find_solid_handle(gr, feature_results).map_err(|e| EngineError::ResolutionFailed {
                reason: format!("{}: seed {e}", spec.label()),
            })?;
        out.push((fid, key, handle));
    }
    Ok(out)
}

/// The features a pattern consumes: every seed's feature plus every target
/// that resolves. (Only what actually resolves is consumed, so the consumed
/// set stays consistent with what dispatch merged.)
pub(crate) fn consumed_feature_ids(
    spec: PatternSpec<'_>,
    feature_results: &HashMap<Uuid, OpResult>,
) -> Vec<Uuid> {
    let mut out: Vec<Uuid> = Vec::new();
    for gr in spec.seeds() {
        if let Some((fid, _)) = anchor_of(gr) {
            if find_solid_handle(gr, feature_results).is_ok() && !out.contains(&fid) {
                out.push(fid);
            }
        }
    }
    let eff = spec.combine();
    if let crate::types::TargetStrategy::Explicit(list) = &eff.targets {
        for gr in list {
            if let Some((fid, _)) = anchor_of(gr) {
                if find_solid_handle(gr, feature_results).is_ok() && !out.contains(&fid) {
                    out.push(fid);
                }
            }
        }
    }
    out
}

/// Execute a pattern feature: resolve, place, instance, combine, carry.
pub(crate) fn execute(
    feature: &Feature,
    kb: &mut dyn KernelBundle,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    already_consumed: &HashSet<Uuid>,
    spec: PatternSpec<'_>,
) -> Result<OpResult, EngineError> {
    let seeds = resolve_seeds(spec, feature_results, already_consumed)?;
    let eff = spec.combine();
    let mut warnings: Vec<String> = Vec::new();

    // Targets: explicit only. `resolve_combine_targets` honors each target's
    // ResolvePolicy; the already-consumed check is added here (P10).
    let mut targets: Vec<(Uuid, KernelSolidHandle)> = Vec::new();
    let mut named: Vec<(Uuid, OutputKey)> = seeds.iter().map(|(f, k, _)| (*f, k.clone())).collect();
    if !matches!(eff.mode, CombineMode::NewBody) {
        if let crate::types::TargetStrategy::Explicit(list) = &eff.targets {
            for gr in list {
                let Some((fid, key)) = anchor_of(gr) else {
                    return Err(EngineError::ResolutionFailed {
                        reason: format!(
                            "{}: a combine target must anchor to a feature output",
                            spec.label()
                        ),
                    });
                };
                if seeds.iter().any(|(f, k, _)| *f == fid && *k == key) {
                    return Err(EngineError::ResolutionFailed {
                        reason: format!(
                            "{}: body {key:?} of feature {fid} is both a seed and a target",
                            spec.label()
                        ),
                    });
                }
                if already_consumed.contains(&fid) {
                    let msg = format!(
                        "{}: target body {key:?} of feature {fid} was already consumed by an \
                         earlier feature",
                        spec.label()
                    );
                    match gr.policy {
                        waffle_types::ResolvePolicy::Strict => {
                            return Err(EngineError::ResolutionFailed { reason: msg });
                        }
                        waffle_types::ResolvePolicy::BestEffort => {
                            warnings.push(format!("{msg}; dropped"));
                            continue;
                        }
                    }
                }
                let resolved = resolve_combine_targets(
                    &crate::types::TargetStrategy::Explicit(vec![gr.clone()]),
                    feature,
                    feature_results,
                    tree,
                    already_consumed,
                    &mut warnings,
                )?;
                for (f, h) in resolved {
                    named.push((f, key.clone()));
                    targets.push((f, h));
                }
            }
        }
        if targets.is_empty() && !matches!(eff.mode, CombineMode::Add) {
            return Err(EngineError::ResolutionFailed {
                reason: format!(
                    "{}: {:?} requires at least one target body",
                    spec.label(),
                    eff.mode
                ),
            });
        }
    }

    let placements = placements(spec, feature_results, kb.as_introspect())?;
    let pattern_seeds: Vec<PatternSeed> = seeds
        .iter()
        .map(|(_, _, h)| PatternSeed { handle: h.clone() })
        .collect();
    let instances = execute_pattern_instances(kb, &pattern_seeds, &placements, spec.skip())?;

    let mut result = match eff.mode {
        CombineMode::NewBody => instances,
        CombineMode::Add => combine_add(kb, &targets, instances)?,
        CombineMode::Cut => combine_cut(kb, &targets, instances)?,
        CombineMode::Intersect => combine_intersect(kb, &targets, instances)?,
    };
    carry_untargeted_named(&mut result, &named, feature_results);
    result.diagnostics.warnings.extend(warnings);
    Ok(result)
}

/// Fold `bodies` into pairwise-disjoint connected lumps by union, in chain
/// order (the first body — a target, when there is one — seeds lump 0, the
/// `Main` output). The fold primitive is `crate::union_all::fold_into`: the
/// same bridging scan, plus the conservative-box gate that skips pairs
/// that cannot touch (`specs/b4_balanced_union.md` §2.2 — byte-identical
/// output, fewer kernel runs).
fn fold_union(
    kb: &mut dyn KernelBundle,
    bodies: Vec<KernelSolidHandle>,
    prov: &mut OpResult,
) -> Result<Vec<KernelSolidHandle>, EngineError> {
    let mut lumps: Vec<Lump> = Vec::new();
    for body in bodies {
        let lump = Lump::of(&*kb, body);
        fold_into(kb, &mut lumps, lump, prov, "pattern Add", &mut || {})?;
    }
    Ok(lumps.into_iter().map(|l| l.handle).collect())
}

fn combine_add(
    kb: &mut dyn KernelBundle,
    targets: &[(Uuid, KernelSolidHandle)],
    instances: OpResult,
) -> Result<OpResult, EngineError> {
    let mut prov = OpResult {
        outputs: Vec::new(),
        provenance: instances.provenance,
        diagnostics: instances.diagnostics,
    };
    let mut bodies: Vec<KernelSolidHandle> = targets.iter().map(|(_, h)| h.clone()).collect();
    bodies.extend(instances.outputs.into_iter().map(|(_, b)| b.handle));
    let lumps = fold_union(kb, bodies, &mut prov)?;
    prov.outputs = rekey(
        lumps
            .into_iter()
            .map(|handle| modeling_ops::BodyOutput {
                handle,
                mesh: None,
                edges: None,
            })
            .collect(),
    );
    Ok(prov)
}

fn combine_cut(
    kb: &mut dyn KernelBundle,
    targets: &[(Uuid, KernelSolidHandle)],
    instances: OpResult,
) -> Result<OpResult, EngineError> {
    let mut prov = OpResult {
        outputs: Vec::new(),
        provenance: instances.provenance,
        diagnostics: instances.diagnostics,
    };
    let tools: Vec<KernelSolidHandle> = instances
        .outputs
        .into_iter()
        .map(|(_, b)| b.handle)
        .collect();
    let mut bodies = Vec::new();
    for (fid, target) in targets {
        // Subtract every instance from every piece of the target; a subtract
        // may split a piece (multi outputs) or consume it (zero outputs).
        let mut pieces = vec![target.clone()];
        for tool in &tools {
            let mut next = Vec::new();
            for piece in &pieces {
                let res = execute_boolean(kb, piece, tool, BooleanKind::Subtract)?;
                absorb(&mut prov, &res);
                next.extend(res.outputs.into_iter().map(|(_, b)| b.handle));
            }
            pieces = next;
            if pieces.is_empty() {
                prov.diagnostics.warnings.push(format!(
                    "pattern Cut consumed the entire target body of feature {fid}"
                ));
                break;
            }
        }
        bodies.extend(pieces.into_iter().map(|handle| modeling_ops::BodyOutput {
            handle,
            mesh: None,
            edges: None,
        }));
    }
    prov.outputs = rekey(bodies);
    Ok(prov)
}

fn combine_intersect(
    kb: &mut dyn KernelBundle,
    targets: &[(Uuid, KernelSolidHandle)],
    instances: OpResult,
) -> Result<OpResult, EngineError> {
    let mut prov = OpResult {
        outputs: Vec::new(),
        provenance: instances.provenance,
        diagnostics: instances.diagnostics,
    };
    let tools: Vec<KernelSolidHandle> = instances
        .outputs
        .into_iter()
        .map(|(_, b)| b.handle)
        .collect();
    // target ∩ (∪ instances): union the instances into lumps first, then
    // intersect each target with each lump; every non-empty piece survives.
    let lumps = fold_union(kb, tools, &mut prov)?;
    let mut bodies = Vec::new();
    for (fid, target) in targets {
        let mut any = false;
        for lump in &lumps {
            let res = execute_boolean(kb, target, lump, BooleanKind::Intersect)?;
            absorb(&mut prov, &res);
            for (_, b) in res.outputs {
                any = true;
                bodies.push(b);
            }
        }
        if !any {
            prov.diagnostics.warnings.push(format!(
                "pattern Intersect produced no material from the target body of feature {fid}"
            ));
        }
    }
    prov.outputs = rekey(bodies);
    Ok(prov)
}
