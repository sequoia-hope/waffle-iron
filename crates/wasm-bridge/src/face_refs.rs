//! The persistent `GeomRef` of each rendered face.
//!
//! Shared by the viewport's face-range accessors (`render_view`) and the
//! `ListFaces` query (`specs/waffle_mcp_server.md` ICR-3), so a ref a user
//! picks and a ref an agent lists are the same ref by construction.

use std::collections::HashMap;

use uuid::Uuid;
use waffle_types::kernel::{KernelId, KernelIntrospect, RenderMesh};
use waffle_types::{
    Anchor, GeomRef, OutputKey, ResolvePolicy, Role, Selector, TopoKind, TopoSignature,
};

/// `(face, its GeomRef)` for each face range of `mesh`, in range order.
///
/// A face with a role gets a `Role` selector, stable across rebuilds. A face
/// without one gets a `Signature` selector: its geometric fingerprint when
/// `fingerprint_roleless` (a ghost body must be resolvable against the owning
/// part's signatures — `signature_similarity` ignores `adjacency_hash`, so an
/// index-only fallback would match an arbitrary face), otherwise the
/// face-index fallback the viewport has always used.
pub fn face_geom_refs(
    feature_id: Uuid,
    output_key: &OutputKey,
    mesh: &RenderMesh,
    role_assignments: &[(KernelId, Role)],
    introspect: &dyn KernelIntrospect,
    fingerprint_roleless: bool,
) -> Vec<(KernelId, GeomRef)> {
    let role_map: HashMap<_, _> = role_assignments.iter().cloned().collect();
    let anchored = |selector: Selector| GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: output_key.clone(),
        },
        selector,
        policy: ResolvePolicy::BestEffort,
        scope: None,
    };

    mesh.face_ranges
        .iter()
        .enumerate()
        .map(|(face_idx, range)| {
            let selector = if let Some(role) = role_map.get(&range.face_id) {
                Selector::Role {
                    role: role.clone(),
                    index: 0,
                }
            } else if fingerprint_roleless {
                let sig = introspect.compute_signature(range.face_id, TopoKind::Face);
                Selector::Signature {
                    signature: TopoSignature {
                        surface_type: sig.surface_type.clone(),
                        area: sig.area,
                        centroid: sig.centroid,
                        normal: sig.normal,
                        bbox: None,
                        adjacency_hash: None,
                        length: None,
                    },
                }
            } else {
                Selector::Signature {
                    signature: TopoSignature {
                        surface_type: None,
                        area: None,
                        centroid: None,
                        normal: None,
                        bbox: None,
                        adjacency_hash: Some(face_idx as u64),
                        length: None,
                    },
                }
            };
            (range.face_id, anchored(selector))
        })
        .collect()
}
