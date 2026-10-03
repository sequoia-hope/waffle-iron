use std::collections::{HashMap, HashSet};

use waffle_types::kernel::units::TAU_WORK;
use waffle_types::kernel::{KernelId, KernelIntrospect, KernelSolidHandle};
use waffle_types::{TopoKind, TopoSignature};

use crate::types::EntityRecord;

/// A snapshot of the topology of a solid at a point in time.
#[derive(Debug, Clone)]
pub struct TopoSnapshot {
    pub faces: Vec<(KernelId, TopoSignature)>,
    pub edges: Vec<(KernelId, TopoSignature)>,
    pub vertices: Vec<(KernelId, TopoSignature)>,
}

/// Take a topology snapshot of a solid for diffing.
pub fn snapshot(introspect: &dyn KernelIntrospect, solid: &KernelSolidHandle) -> TopoSnapshot {
    TopoSnapshot {
        faces: introspect.compute_all_signatures(solid, TopoKind::Face),
        edges: introspect.compute_all_signatures(solid, TopoKind::Edge),
        vertices: introspect.compute_all_signatures(solid, TopoKind::Vertex),
    }
}

/// Result of diffing two topology snapshots.
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// Entities present in `after` but not in `before`.
    pub created: Vec<EntityRecord>,
    /// Entities present in `before` but not in `after`.
    pub deleted: Vec<EntityRecord>,
    /// Entities that likely correspond between before and after (by signature similarity).
    pub survived: Vec<(KernelId, KernelId)>,
}

/// Diff two topology snapshots to find created, deleted, and surviving entities.
/// For the initial extrude (no "before" solid), pass an empty snapshot as `before`.
pub fn diff(before: &TopoSnapshot, after: &TopoSnapshot) -> DiffResult {
    let mut created = Vec::new();
    let mut deleted = Vec::new();
    let mut survived = Vec::new();

    diff_kind(
        &before.faces,
        &after.faces,
        TopoKind::Face,
        &mut created,
        &mut deleted,
        &mut survived,
    );
    diff_kind(
        &before.edges,
        &after.edges,
        TopoKind::Edge,
        &mut created,
        &mut deleted,
        &mut survived,
    );
    diff_kind(
        &before.vertices,
        &after.vertices,
        TopoKind::Vertex,
        &mut created,
        &mut deleted,
        &mut survived,
    );

    DiffResult {
        created,
        deleted,
        survived,
    }
}

/// Diff a single kind of topology entity.
fn diff_kind(
    before: &[(KernelId, TopoSignature)],
    after: &[(KernelId, TopoSignature)],
    kind: TopoKind,
    created: &mut Vec<EntityRecord>,
    deleted: &mut Vec<EntityRecord>,
    survived: &mut Vec<(KernelId, KernelId)>,
) {
    let before_ids: HashSet<KernelId> = before.iter().map(|(id, _)| *id).collect();
    let after_ids: HashSet<KernelId> = after.iter().map(|(id, _)| *id).collect();

    // Entities with the same ID survived
    for &id in before_ids.intersection(&after_ids) {
        survived.push((id, id));
    }

    // New entities in after
    let new_ids: Vec<KernelId> = after_ids.difference(&before_ids).copied().collect();
    let gone_ids: Vec<KernelId> = before_ids.difference(&after_ids).copied().collect();

    // Try to match gone entities to new entities by signature similarity
    let before_map: HashMap<KernelId, &TopoSignature> =
        before.iter().map(|(id, sig)| (*id, sig)).collect();
    let after_map: HashMap<KernelId, &TopoSignature> =
        after.iter().map(|(id, sig)| (*id, sig)).collect();

    let mut matched_before: HashSet<KernelId> = HashSet::new();
    let mut matched_after: HashSet<KernelId> = HashSet::new();

    // Greedy matching: for each gone entity, find the best-matching new entity
    for &gone_id in &gone_ids {
        let gone_sig = before_map[&gone_id];
        let mut best_match: Option<(KernelId, f64)> = None;

        for &new_id in &new_ids {
            if matched_after.contains(&new_id) {
                continue;
            }
            let new_sig = after_map[&new_id];
            let similarity = signature_similarity(gone_sig, new_sig);
            if similarity > 0.7 {
                if let Some((_, best_sim)) = best_match {
                    if similarity > best_sim {
                        best_match = Some((new_id, similarity));
                    }
                } else {
                    best_match = Some((new_id, similarity));
                }
            }
        }

        if let Some((matched_id, _)) = best_match {
            survived.push((gone_id, matched_id));
            matched_before.insert(gone_id);
            matched_after.insert(matched_id);
        }
    }

    // Remaining unmatched gone entities are deleted
    for &gone_id in &gone_ids {
        if !matched_before.contains(&gone_id) {
            deleted.push(EntityRecord {
                kernel_id: gone_id,
                kind,
                signature: before_map[&gone_id].clone(),
            });
        }
    }

    // Remaining unmatched new entities are created
    for &new_id in &new_ids {
        if !matched_after.contains(&new_id) {
            created.push(EntityRecord {
                kernel_id: new_id,
                kind,
                signature: after_map[&new_id].clone(),
            });
        }
    }
}

/// Compute similarity between two topology signatures (0.0 to 1.0).
/// Higher means more similar. Used for signature-based matching.
///
/// **Zero weight scores 0.0 here**, which is indistinguishable from "weighed
/// every field and agreed on none". A caller that BINDS a reference must use
/// [`signature_match`] instead, which separates the two (N0 of
/// `specs/agent_mechanical_design.md` §5.1): a pair of signatures sharing no
/// field at all is not a bad match, it is no comparison.
pub fn signature_similarity(a: &TopoSignature, b: &TopoSignature) -> f64 {
    signature_match(a, b).unwrap_or(0.0)
}

/// [`signature_similarity`] with the no-comparison case told apart from a
/// score of zero: `None` when `a` and `b` share no field this scorer weighs
/// (surface type, area, centroid, normal, length), so there was nothing to
/// compare.
///
/// The index-only fingerprint the viewport used to mint — `adjacency_hash`
/// and nothing else — lands here: `adjacency_hash` carries no geometry and
/// is deliberately not weighed, so every candidate "scored" 0.0 and the
/// best match was whichever entity came first.
pub fn signature_match(a: &TopoSignature, b: &TopoSignature) -> Option<f64> {
    let mut score = 0.0;
    let mut weight = 0.0;

    // Surface type match (high weight)
    if let (Some(ref st_a), Some(ref st_b)) = (&a.surface_type, &b.surface_type) {
        weight += 3.0;
        if st_a == st_b {
            score += 3.0;
        }
    }

    // Area similarity (medium weight)
    if let (Some(area_a), Some(area_b)) = (a.area, b.area) {
        weight += 2.0;
        let max_area = area_a.max(area_b);
        if max_area > TAU_WORK {
            let diff = (area_a - area_b).abs() / max_area;
            score += 2.0 * (1.0 - diff.min(1.0));
        } else {
            score += 2.0; // Both effectively zero
        }
    }

    // Centroid proximity (medium weight)
    if let (Some(c_a), Some(c_b)) = (a.centroid, b.centroid) {
        weight += 2.0;
        let dist =
            ((c_a[0] - c_b[0]).powi(2) + (c_a[1] - c_b[1]).powi(2) + (c_a[2] - c_b[2]).powi(2))
                .sqrt();
        // Within 0.1 unit = full match, falls off linearly
        score += 2.0 * (1.0 - (dist / 10.0).min(1.0));
    }

    // Normal alignment (medium weight)
    if let (Some(n_a), Some(n_b)) = (a.normal, b.normal) {
        weight += 2.0;
        let dot = n_a[0] * n_b[0] + n_a[1] * n_b[1] + n_a[2] * n_b[2];
        // dot = 1.0 means parallel, -1.0 means anti-parallel
        score += 2.0 * ((dot + 1.0) / 2.0).max(0.0);
    }

    // Length similarity (for edges)
    if let (Some(len_a), Some(len_b)) = (a.length, b.length) {
        weight += 2.0;
        let max_len = len_a.max(len_b);
        if max_len > TAU_WORK {
            let diff = (len_a - len_b).abs() / max_len;
            score += 2.0 * (1.0 - diff.min(1.0));
        } else {
            score += 2.0;
        }
    }

    // The rotation-invariant content of a surface of revolution (N0 of
    // `specs/agent_mechanical_design.md` §5.1). This is what identifies a
    // full-turn face, which has no point normal to weigh: the axis direction
    // (compared sign-free — an axis and its negative are the same axis), the
    // radii the surface holds, and how far the face reaches along the axis.
    if let (Some(ax_a), Some(ax_b)) = (a.axis, b.axis) {
        if let (Some(da), Some(db)) = (ax_a.direction, ax_b.direction) {
            weight += 2.0;
            let dot = (da[0] * db[0] + da[1] * db[1] + da[2] * db[2]).abs();
            score += 2.0 * dot.clamp(0.0, 1.0);
        }
        for (wt, va, vb) in [
            (2.0, ax_a.radius, ax_b.radius),
            (1.0, ax_a.half_angle, ax_b.half_angle),
            (1.0, ax_a.minor_radius, ax_b.minor_radius),
            (1.0, ax_a.extent, ax_b.extent),
        ] {
            if let (Some(va), Some(vb)) = (va, vb) {
                weight += wt;
                let max = va.abs().max(vb.abs());
                if max > TAU_WORK {
                    let diff = (va - vb).abs() / max;
                    score += wt * (1.0 - diff.min(1.0));
                } else {
                    score += wt;
                }
            }
        }
    }

    if weight > 0.0 {
        Some(score / weight)
    } else {
        None
    }
}
