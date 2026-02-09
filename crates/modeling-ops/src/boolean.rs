use kernel_fork::{KernelId, KernelSolidHandle};
use waffle_types::{OutputKey, Role, TopoKind};

use crate::diff::{self, TopoSnapshot};
use crate::kernel_ext::KernelBundle;
use crate::types::{BodyOutput, Diagnostics, OpError, OpResult, Provenance};

/// Boolean operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanKind {
    Union,
    Subtract,
    Intersect,
}

/// Execute a boolean operation between two solids.
pub fn execute_boolean(
    kb: &mut dyn KernelBundle,
    body_a: &KernelSolidHandle,
    body_b: &KernelSolidHandle,
    kind: BooleanKind,
) -> Result<OpResult, OpError> {
    // Snapshot both inputs for provenance
    let snap_a = diff::snapshot(kb.as_introspect(), body_a);
    let snap_b = diff::snapshot(kb.as_introspect(), body_b);

    // Merge before snapshots
    let mut before_faces = snap_a.faces.clone();
    before_faces.extend(snap_b.faces.clone());
    let mut before_edges = snap_a.edges.clone();
    before_edges.extend(snap_b.edges.clone());
    let mut before_vertices = snap_a.vertices.clone();
    before_vertices.extend(snap_b.vertices.clone());
    let before = TopoSnapshot {
        faces: before_faces,
        edges: before_edges,
        vertices: before_vertices,
    };

    // Execute boolean
    let handle = match kind {
        BooleanKind::Union => kb.boolean_union(body_a, body_b)?,
        BooleanKind::Subtract => kb.boolean_subtract(body_a, body_b)?,
        BooleanKind::Intersect => kb.boolean_intersect(body_a, body_b)?,
    };

    // Snapshot result
    let after = diff::snapshot(kb.as_introspect(), &handle);
    let diff_result = diff::diff(&before, &after);

    // Assign roles to result faces
    let role_assignments = assign_boolean_roles(kb.as_introspect(), &handle, &snap_a, &snap_b);

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

/// Assign roles to boolean result faces.
fn assign_boolean_roles(
    introspect: &dyn kernel_fork::KernelIntrospect,
    result: &KernelSolidHandle,
    snap_a: &TopoSnapshot,
    snap_b: &TopoSnapshot,
) -> Vec<(KernelId, Role)> {
    let result_faces = introspect.list_faces(result);
    let mut assignments = Vec::new();
    let mut a_index = 0;
    let mut b_index = 0;

    for &face_id in &result_faces {
        let sig = introspect.compute_signature(face_id, TopoKind::Face);

        let best_a = snap_a
            .faces
            .iter()
            .map(|(_, s)| crate::diff::signature_similarity(&sig, s))
            .fold(0.0_f64, |a, b| a.max(b));

        let best_b = snap_b
            .faces
            .iter()
            .map(|(_, s)| crate::diff::signature_similarity(&sig, s))
            .fold(0.0_f64, |a, b| a.max(b));

        if best_a >= best_b {
            assignments.push((face_id, Role::BooleanBodyAFace { index: a_index }));
            a_index += 1;
        } else {
            assignments.push((face_id, Role::BooleanBodyBFace { index: b_index }));
            b_index += 1;
        }
    }

    assignments
}
