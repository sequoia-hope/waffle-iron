use kernel_fork::{KernelId, KernelSolidHandle};
use waffle_types::{OutputKey, Role, TopoKind};

use crate::diff::{self, TopoSnapshot};
use crate::kernel_ext::KernelBundle;
use crate::types::{BodyOutput, Diagnostics, OpError, OpResult, Provenance};

/// Execute a revolve operation.
pub fn execute_revolve(
    kb: &mut dyn KernelBundle,
    face_id: KernelId,
    axis_origin: [f64; 3],
    axis_direction: [f64; 3],
    angle: f64,
    before_snapshot: Option<&TopoSnapshot>,
) -> Result<OpResult, OpError> {
    let handle = kb.revolve_face(face_id, axis_origin, axis_direction, angle)?;

    let after = diff::snapshot(kb.as_introspect(), &handle);

    let empty_snap = TopoSnapshot {
        faces: Vec::new(),
        edges: Vec::new(),
        vertices: Vec::new(),
    };
    let before = before_snapshot.unwrap_or(&empty_snap);
    let diff_result = diff::diff(before, &after);

    let role_assignments =
        assign_revolve_roles(kb.as_introspect(), &handle, &axis_direction, angle);

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

/// Assign semantic roles to faces of a revolved solid.
fn assign_revolve_roles(
    introspect: &dyn kernel_fork::KernelIntrospect,
    solid: &KernelSolidHandle,
    axis_direction: &[f64; 3],
    angle: f64,
) -> Vec<(KernelId, Role)> {
    let faces = introspect.list_faces(solid);
    if faces.is_empty() {
        return Vec::new();
    }

    let is_full_revolution = angle.abs() >= std::f64::consts::TAU - 1e-6;

    // Normalize axis direction
    let dir_len =
        (axis_direction[0].powi(2) + axis_direction[1].powi(2) + axis_direction[2].powi(2)).sqrt();
    let norm_axis = if dir_len > 1e-12 {
        [
            axis_direction[0] / dir_len,
            axis_direction[1] / dir_len,
            axis_direction[2] / dir_len,
        ]
    } else {
        [0.0, 0.0, 1.0]
    };

    let mut assignments = Vec::new();

    if is_full_revolution {
        for (i, &face_id) in faces.iter().enumerate() {
            assignments.push((face_id, Role::SideFace { index: i }));
        }
    } else {
        let mut face_dots: Vec<(KernelId, f64)> = faces
            .iter()
            .map(|&face_id| {
                let sig = introspect.compute_signature(face_id, TopoKind::Face);
                let dot = sig
                    .normal
                    .map(|n| {
                        (n[0] * norm_axis[0] + n[1] * norm_axis[1] + n[2] * norm_axis[2]).abs()
                    })
                    .unwrap_or(0.0);
                (face_id, dot)
            })
            .collect();

        face_dots.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut start_assigned = false;
        let mut end_assigned = false;
        let mut side_index = 0;

        for (face_id, dot) in face_dots {
            if dot > 0.5 && !start_assigned {
                assignments.push((face_id, Role::RevStartFace));
                start_assigned = true;
            } else if dot > 0.5 && !end_assigned {
                assignments.push((face_id, Role::RevEndFace));
                end_assigned = true;
            } else {
                assignments.push((face_id, Role::SideFace { index: side_index }));
                side_index += 1;
            }
        }
    }

    assignments
}
