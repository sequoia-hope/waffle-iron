use kernel_fork::{KernelId, KernelSolidHandle};
use waffle_types::{OutputKey, Role, TopoKind};

use crate::diff::{self, TopoSnapshot};
use crate::kernel_ext::KernelBundle;
use crate::types::{BodyOutput, Diagnostics, OpError, OpResult, Provenance};

/// Execute a shell operation: hollow out a solid by removing faces
/// and offsetting remaining faces inward.
pub fn execute_shell(
    kb: &mut dyn KernelBundle,
    solid: &KernelSolidHandle,
    faces_to_remove: &[KernelId],
    thickness: f64,
) -> Result<OpResult, OpError> {
    if thickness <= 0.0 {
        return Err(OpError::InvalidParameter {
            reason: "shell thickness must be positive".to_string(),
        });
    }

    // Snapshot before
    let before = diff::snapshot(kb.as_introspect(), solid);

    // Execute the kernel operation
    let handle = kb.shell(solid, faces_to_remove, thickness)?;

    // Snapshot after
    let after = diff::snapshot(kb.as_introspect(), &handle);
    let diff_result = diff::diff(&before, &after);

    // Assign roles: inner faces get ShellInnerFace role
    let role_assignments = assign_shell_roles(kb.as_introspect(), &handle, &before);

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

/// Assign roles to faces of a shelled solid.
/// Inner (offset) faces get ShellInnerFace role.
fn assign_shell_roles(
    introspect: &dyn kernel_fork::KernelIntrospect,
    solid: &KernelSolidHandle,
    before: &TopoSnapshot,
) -> Vec<(KernelId, Role)> {
    let result_faces = introspect.list_faces(solid);
    let mut assignments = Vec::new();
    let mut inner_index = 0;

    for &face_id in &result_faces {
        let sig = introspect.compute_signature(face_id, TopoKind::Face);

        // Inner faces have inverted normals compared to outer faces.
        // Check if normal is opposite to any before face.
        let best_match = before
            .faces
            .iter()
            .map(|(_, s)| crate::diff::signature_similarity(&sig, s))
            .fold(0.0_f64, |a, b| a.max(b));

        if best_match < 0.7 {
            // New face — likely an inner face
            assignments.push((face_id, Role::ShellInnerFace { index: inner_index }));
            inner_index += 1;
        }
    }

    assignments
}
