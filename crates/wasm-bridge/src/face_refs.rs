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
/// without one gets a `Signature` selector carrying its geometric
/// fingerprint.
///
/// **N0 of `specs/agent_mechanical_design.md` §5.1.** A roleless face used to
/// get a choice: the fingerprint, or — on the viewport's own path and the
/// agent's `face_list` — a signature whose only field was the face INDEX in
/// `adjacency_hash`. `signature_similarity` does not read `adjacency_hash`,
/// so that selector carried no geometry at all: every candidate scored 0.0
/// and the reference resolved to whichever face was created first, with a
/// "0.0%" warning. That was the ICR-3 limit, and it bit every imported STEP
/// body (which has no roles at all). There is no index fallback any more —
/// the fingerprint is the only roleless selector, and a fingerprint that
/// cannot identify one face is refused at resolution rather than bound (see
/// `feature_engine::resolve::resolve_by_signature`).
pub fn face_geom_refs(
    feature_id: Uuid,
    output_key: &OutputKey,
    mesh: &RenderMesh,
    role_assignments: &[(KernelId, Role)],
    introspect: &dyn KernelIntrospect,
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
        .map(|range| {
            let selector = if let Some(role) = role_map.get(&range.face_id) {
                Selector::Role {
                    role: role.clone(),
                    index: 0,
                }
            } else {
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
                        // The identifying content of a face that goes all the
                        // way round its axis, which has no point normal to
                        // carry (N0; `waffle_types::AxisDescriptor`). Without
                        // this a cylinder lateral's ref would hold only its
                        // type and area.
                        axis: sig.axis,
                    },
                }
            };
            (range.face_id, anchored(selector))
        })
        .collect()
}
