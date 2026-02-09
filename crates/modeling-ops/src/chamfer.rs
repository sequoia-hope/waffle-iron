use kernel_fork::{KernelId, KernelSolidHandle};
use waffle_types::{OutputKey, Role, TopoKind};

use crate::diff::{self, TopoSnapshot};
use crate::kernel_ext::KernelBundle;
use crate::types::{BodyOutput, Diagnostics, OpError, OpResult, Provenance};

/// Execute a chamfer operation on specified edges of a solid.
pub fn execute_chamfer(
    kb: &mut dyn KernelBundle,
    solid: &KernelSolidHandle,
    edges: &[KernelId],
    distance: f64,
) -> Result<OpResult, OpError> {
    if distance <= 0.0 {
        return Err(OpError::InvalidParameter {
            reason: "chamfer distance must be positive".to_string(),
        });
    }

    // Snapshot before
    let before = diff::snapshot(kb.as_introspect(), solid);

    // Execute the kernel operation
    let handle = kb.chamfer_edges(solid, edges, distance)?;

    // Snapshot after
    let after = diff::snapshot(kb.as_introspect(), &handle);
    let diff_result = diff::diff(&before, &after);

    // Assign roles
    let role_assignments = assign_chamfer_roles(kb.as_introspect(), &handle, &before);

    let provenance = Provenance {
        created: diff_result.created,
        deleted: diff_result.deleted,
        modified: Vec::new(),
        role_assignments,
    };

    Ok(OpResult {
        outputs: vec![(OutputKey::Main, BodyOutput { handle, mesh: None })],
        provenance,
        diagnostics: Diagnostics::default(),
    })
}

/// Assign roles to faces of a chamfered solid.
/// New faces created by chamfer get ChamferFace roles.
fn assign_chamfer_roles(
    introspect: &dyn kernel_fork::KernelIntrospect,
    solid: &KernelSolidHandle,
    before: &TopoSnapshot,
) -> Vec<(KernelId, Role)> {
    let result_faces = introspect.list_faces(solid);
    let mut assignments = Vec::new();
    let mut chamfer_index = 0;

    // Find faces that are new (not in before by signature match)
    for &face_id in &result_faces {
        let sig = introspect.compute_signature(face_id, TopoKind::Face);

        // Check if this face matches any before face with high similarity
        let best_match = before
            .faces
            .iter()
            .map(|(_, s)| crate::diff::signature_similarity(&sig, s))
            .fold(0.0_f64, |a, b| a.max(b));

        if best_match < 0.7 {
            // This is a new face — likely a chamfer face
            assignments.push((
                face_id,
                Role::ChamferFace {
                    index: chamfer_index,
                },
            ));
            chamfer_index += 1;
        }
    }

    assignments
}
