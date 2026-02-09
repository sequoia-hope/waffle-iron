use kernel_fork::{KernelId, KernelSolidHandle};
use waffle_types::{OutputKey, Role, TopoKind};

use crate::diff::{self, TopoSnapshot};
use crate::kernel_ext::KernelBundle;
use crate::types::{BodyOutput, Diagnostics, OpError, OpResult, Provenance};

/// Execute a fillet operation on specified edges of a solid.
pub fn execute_fillet(
    kb: &mut dyn KernelBundle,
    solid: &KernelSolidHandle,
    edges: &[KernelId],
    radius: f64,
) -> Result<OpResult, OpError> {
    if radius <= 0.0 {
        return Err(OpError::InvalidParameter {
            reason: "fillet radius must be positive".to_string(),
        });
    }

    // Snapshot before
    let before = diff::snapshot(kb.as_introspect(), solid);

    // Execute the kernel operation
    let handle = kb.fillet_edges(solid, edges, radius)?;

    // Snapshot after
    let after = diff::snapshot(kb.as_introspect(), &handle);
    let diff_result = diff::diff(&before, &after);

    // Assign roles: new faces created by the fillet get FilletFace role,
    // surviving faces keep no special role (they are trimmed originals).
    let role_assignments = assign_fillet_roles(kb.as_introspect(), &handle, &before);

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

/// Assign roles to faces of a filleted solid.
/// New faces that don't match any before face get FilletFace roles.
fn assign_fillet_roles(
    introspect: &dyn kernel_fork::KernelIntrospect,
    solid: &KernelSolidHandle,
    before: &TopoSnapshot,
) -> Vec<(KernelId, Role)> {
    let result_faces = introspect.list_faces(solid);
    let mut assignments = Vec::new();
    let mut fillet_index = 0;

    for &face_id in &result_faces {
        let sig = introspect.compute_signature(face_id, TopoKind::Face);

        // Check if this face matches any before face by signature similarity
        let best_match = before
            .faces
            .iter()
            .map(|(_, s)| crate::diff::signature_similarity(&sig, s))
            .fold(0.0_f64, |a, b| a.max(b));

        if best_match < 0.7 {
            // New face — likely a fillet face
            assignments.push((
                face_id,
                Role::FilletFace {
                    index: fillet_index,
                },
            ));
            fillet_index += 1;
        }
    }

    assignments
}
