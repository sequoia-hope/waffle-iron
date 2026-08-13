use modeling_ops::OpResult;
use uuid::Uuid;
use waffle_types::kernel::units::MIN_FEATURE_SIZE;
use waffle_types::kernel::{KernelId, KernelIntrospect};
use waffle_types::{
    Anchor, Filter, GeomRef, OutputKey, ResolvePolicy, Role, Selector, TieBreak, TopoKind,
    TopoQuery, TopoSignature,
};

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
        Selector::Query { ref query } => {
            resolve_by_query(op_result, query, geom_ref.kind, geom_ref.policy)
        }
        Selector::Position { .. } => {
            // Position selectors are used for viewport picking (vertex overlay).
            // They don't resolve to kernel entities — return an error.
            Err(EngineError::ResolutionFailed {
                reason: "Position selectors are not resolvable to kernel entities".to_string(),
            })
        }
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
    let (feature_id, output_key) = match &geom_ref.anchor {
        Anchor::FeatureOutput {
            feature_id,
            output_key,
        } => (*feature_id, output_key.clone()),
        Anchor::Datum { datum_id } => {
            return Err(EngineError::ResolutionFailed {
                reason: format!("Position selector on datum {} not supported", datum_id),
            });
        }
    };

    let op_result = feature_results
        .get(&feature_id)
        .ok_or(EngineError::ResolutionFailed {
            reason: format!("Feature {} has no result (not yet rebuilt?)", feature_id),
        })?;

    // The body output whose key matches the anchor (fallback: the first body).
    let handle = op_result
        .outputs
        .iter()
        .find(|(k, _)| key_matches(k, &output_key))
        .or_else(|| op_result.outputs.first())
        .map(|(_, b)| &b.handle)
        .ok_or(EngineError::ResolutionFailed {
            reason: "feature produced no body output for position resolution".to_string(),
        })?;

    let candidates = match geom_ref.kind {
        TopoKind::Vertex => introspect.list_vertices(handle),
        TopoKind::Edge => introspect.list_edges(handle),
        TopoKind::Face => introspect.list_faces(handle),
        other => {
            return Err(EngineError::ResolutionFailed {
                reason: format!("Position selector unsupported for {:?}", other),
            });
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
        }),
        Some((id, d)) => match geom_ref.policy {
            ResolvePolicy::BestEffort => Ok(ResolvedRef {
                kernel_id: id,
                warnings: vec![format!(
                    "Position matched nearest {:?} at distance {:.2e} (> {:.0e}) — geometry may have moved",
                    geom_ref.kind, d, POSITION_MATCH_TOL
                )],
            }),
            ResolvePolicy::Strict => Err(EngineError::ResolutionFailed {
                reason: format!(
                    "No {:?} within {:.0e} of picked position (nearest {:.2e})",
                    geom_ref.kind, POSITION_MATCH_TOL, d
                ),
            }),
        },
        None => Err(EngineError::ResolutionFailed {
            reason: format!(
                "Feature output has no {:?} entities to match position",
                geom_ref.kind
            ),
        }),
    }
}

/// Whether two output keys denote the same body output.
fn key_matches(a: &OutputKey, b: &OutputKey) -> bool {
    a.tag() == b.tag()
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

/// Resolve by user-specified geometric query.
fn resolve_by_query(
    op_result: &OpResult,
    query: &TopoQuery,
    kind: TopoKind,
    policy: ResolvePolicy,
) -> Result<ResolvedRef, EngineError> {
    // Collect candidates: entities matching the requested kind that pass all filters.
    let mut matches: Vec<(KernelId, &TopoSignature)> = Vec::new();

    for entity in &op_result.provenance.created {
        if entity.kind != kind {
            continue;
        }
        if passes_all_filters(&entity.signature, &query.filters) {
            matches.push((entity.kernel_id, &entity.signature));
        }
    }

    if matches.is_empty() {
        return match policy {
            ResolvePolicy::Strict => Err(EngineError::ResolutionFailed {
                reason: format!(
                    "Query matched no {:?} entities ({} candidates, {} filters)",
                    kind,
                    op_result
                        .provenance
                        .created
                        .iter()
                        .filter(|e| e.kind == kind)
                        .count(),
                    query.filters.len()
                ),
            }),
            ResolvePolicy::BestEffort => {
                // Fall back to first entity of matching kind
                let fallback = op_result
                    .provenance
                    .created
                    .iter()
                    .find(|e| e.kind == kind)
                    .map(|e| e.kernel_id);
                match fallback {
                    Some(id) => Ok(ResolvedRef {
                        kernel_id: id,
                        warnings: vec![
                            "Query matched no entities, fell back to first of matching kind (BestEffort)".to_string(),
                        ],
                    }),
                    None => Err(EngineError::ResolutionFailed {
                        reason: format!("No {:?} entities available for query fallback", kind),
                    }),
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
    })
}

/// Check if a signature passes all query filters.
fn passes_all_filters(sig: &TopoSignature, filters: &[Filter]) -> bool {
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
}
