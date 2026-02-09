use kernel_fork::{KernelId, KernelSolidHandle};
use waffle_types::{OutputKey, Role, TopoKind};

use crate::diff::{self, TopoSnapshot};
use crate::kernel_ext::KernelBundle;
use crate::types::{BodyOutput, Diagnostics, OpError, OpResult, Provenance};

/// Execute an extrude operation.
///
/// Takes a face ID (from make_faces_from_profiles), extrudes it along
/// a direction vector by a given depth, and returns an OpResult with
/// full provenance tracking.
pub fn execute_extrude(
    kb: &mut dyn KernelBundle,
    face_id: KernelId,
    direction: [f64; 3],
    depth: f64,
    before_snapshot: Option<&TopoSnapshot>,
) -> Result<OpResult, OpError> {
    // Execute the kernel operation
    let handle = kb.extrude_face(face_id, direction, depth)?;

    // Take snapshot after
    let after = diff::snapshot(kb.as_introspect(), &handle);

    // Diff
    let empty_snap = TopoSnapshot {
        faces: Vec::new(),
        edges: Vec::new(),
        vertices: Vec::new(),
    };
    let before = before_snapshot.unwrap_or(&empty_snap);
    let diff_result = diff::diff(before, &after);

    // Assign roles to created faces
    let role_assignments = assign_extrude_roles(kb.as_introspect(), &handle, &direction);

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

/// Assign semantic roles to faces of an extruded solid.
fn assign_extrude_roles(
    introspect: &dyn kernel_fork::KernelIntrospect,
    solid: &KernelSolidHandle,
    direction: &[f64; 3],
) -> Vec<(KernelId, Role)> {
    let faces = introspect.list_faces(solid);
    if faces.is_empty() {
        return Vec::new();
    }

    // Normalize direction
    let dir_len = (direction[0].powi(2) + direction[1].powi(2) + direction[2].powi(2)).sqrt();
    let norm_dir = if dir_len > 1e-12 {
        [
            direction[0] / dir_len,
            direction[1] / dir_len,
            direction[2] / dir_len,
        ]
    } else {
        [0.0, 0.0, 1.0]
    };

    // Compute dot products of face normals with extrude direction
    let mut face_dots: Vec<(KernelId, f64)> = faces
        .iter()
        .map(|&face_id| {
            let sig = introspect.compute_signature(face_id, TopoKind::Face);
            let dot = sig
                .normal
                .map(|n| n[0] * norm_dir[0] + n[1] * norm_dir[1] + n[2] * norm_dir[2])
                .unwrap_or(0.0);
            (face_id, dot)
        })
        .collect();

    // Sort by dot product descending
    face_dots.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut assignments = Vec::new();
    let mut side_index = 0;

    for (i, (face_id, dot)) in face_dots.iter().enumerate() {
        if i == 0 && *dot > 0.5 {
            assignments.push((*face_id, Role::EndCapPositive));
        } else if i == face_dots.len() - 1 && *dot < -0.5 {
            assignments.push((*face_id, Role::EndCapNegative));
        } else {
            assignments.push((*face_id, Role::SideFace { index: side_index }));
            side_index += 1;
        }
    }

    assignments
}
