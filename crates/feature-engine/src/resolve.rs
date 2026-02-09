use kernel_fork::KernelId;
use modeling_ops::OpResult;
use uuid::Uuid;
use waffle_types::{GeomRef, ResolvePolicy, Role, Selector};

use crate::types::EngineError;

/// Result of resolving a GeomRef to a concrete KernelId.
#[derive(Debug, Clone)]
pub struct ResolvedRef {
    pub kernel_id: KernelId,
    pub warnings: Vec<String>,
}

/// Resolve a GeomRef to a KernelId using the feature results map.
pub fn resolve_geom_ref(
    geom_ref: &GeomRef,
    feature_results: &std::collections::HashMap<Uuid, OpResult>,
) -> Result<ResolvedRef, EngineError> {
    // Extract the feature ID from the anchor
    let feature_id = match &geom_ref.anchor {
        waffle_types::Anchor::FeatureOutput {
            feature_id,
            output_key: _,
        } => *feature_id,
        waffle_types::Anchor::Datum { datum_id } => {
            return Err(EngineError::ResolutionFailed {
                reason: format!("Datum references not yet supported (datum {})", datum_id),
            });
        }
    };

    // Find the feature's OpResult
    let op_result = feature_results
        .get(&feature_id)
        .ok_or(EngineError::ResolutionFailed {
            reason: format!("Feature {} has no result (not yet rebuilt?)", feature_id),
        })?;

    // Apply the selector
    match &geom_ref.selector {
        Selector::Role { ref role, index } => {
            resolve_by_role(op_result, role, *index, geom_ref.policy)
        }
        Selector::Signature { ref signature } => {
            resolve_by_signature(op_result, signature, geom_ref.policy)
        }
        Selector::Query { .. } => Err(EngineError::ResolutionFailed {
            reason: "Query-based resolution not yet implemented".to_string(),
        }),
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
                                    "Role resolution failed, fell back to kind-match (BestEffort): {}",
                                    primary_err
                                )],
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
        return Err(EngineError::ResolutionFailed {
            reason: format!("No entity with role {:?}", role),
        });
    }

    if index < matching.len() {
        Ok(ResolvedRef {
            kernel_id: matching[index],
            warnings: Vec::new(),
        })
    } else {
        match policy {
            ResolvePolicy::Strict => Err(EngineError::ResolutionFailed {
                reason: format!(
                    "Role {:?} index {} out of range (found {})",
                    role,
                    index,
                    matching.len()
                ),
            }),
            ResolvePolicy::BestEffort => {
                let kernel_id = *matching.last().unwrap();
                Ok(ResolvedRef {
                    kernel_id,
                    warnings: vec![format!(
                        "Role {:?} index {} clamped to {} (BestEffort)",
                        role,
                        index,
                        matching.len() - 1
                    )],
                })
            }
        }
    }
}

/// Resolve by geometric signature (fallback when role fails).
fn resolve_by_signature(
    op_result: &OpResult,
    target_sig: &waffle_types::TopoSignature,
    policy: ResolvePolicy,
) -> Result<ResolvedRef, EngineError> {
    let mut best_match: Option<(KernelId, f64)> = None;

    for entity in &op_result.provenance.created {
        let sim = modeling_ops::signature_similarity(&entity.signature, target_sig);
        if let Some((_, best_sim)) = best_match {
            if sim > best_sim {
                best_match = Some((entity.kernel_id, sim));
            }
        } else {
            best_match = Some((entity.kernel_id, sim));
        }
    }

    match best_match {
        Some((id, sim)) if sim > 0.5 => {
            let mut warnings = Vec::new();
            if sim < 0.9 {
                warnings.push(format!("Signature match confidence: {:.1}%", sim * 100.0));
            }
            Ok(ResolvedRef {
                kernel_id: id,
                warnings,
            })
        }
        Some((id, sim)) => match policy {
            ResolvePolicy::BestEffort => Ok(ResolvedRef {
                kernel_id: id,
                warnings: vec![format!(
                    "Low-confidence signature match: {:.1}%",
                    sim * 100.0
                )],
            }),
            ResolvePolicy::Strict => Err(EngineError::ResolutionFailed {
                reason: format!("Best signature match too low: {:.1}%", sim * 100.0),
            }),
        },
        None => Err(EngineError::ResolutionFailed {
            reason: "No entities to match signature against".to_string(),
        }),
    }
}
