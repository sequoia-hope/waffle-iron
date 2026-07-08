use waffle_types::kernel::{KernelId, KernelSolidHandle};
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

    // Execute boolean (multi-body aware)
    let raw = match kind {
        BooleanKind::Union => kb.boolean_union_multi(body_a, body_b),
        BooleanKind::Subtract => kb.boolean_subtract_multi(body_a, body_b),
        BooleanKind::Intersect => kb.boolean_intersect_multi(body_a, body_b),
    };
    let handles = match raw {
        Ok(h) => h,
        // Spec `cut_consumes_body` §3 branches 2–3: a Subtract or Intersect
        // whose result is legitimately EMPTY consumed the entire target —
        // return zero output bodies + a warning (body-lifetime policy), with
        // the target's whole topology recorded as deleted. A Union of
        // non-empty operands cannot be empty, so branch 4 keeps the typed
        // error loud (a kernel defect must not masquerade as consumption).
        Err(waffle_types::kernel::KernelError::BooleanEmptyResult)
            if matches!(kind, BooleanKind::Subtract | BooleanKind::Intersect) =>
        {
            let empty = TopoSnapshot {
                faces: Vec::new(),
                edges: Vec::new(),
                vertices: Vec::new(),
            };
            let diff_result = diff::diff(&before, &empty);
            let warning = match kind {
                BooleanKind::Subtract => {
                    "cut consumed the entire target body (no material remains)"
                }
                _ => "intersect produced no material (target body consumed)",
            };
            return Ok(OpResult {
                outputs: Vec::new(),
                provenance: Provenance {
                    created: Vec::new(),
                    deleted: diff_result.deleted,
                    modified: Vec::new(),
                    role_assignments: Vec::new(),
                },
                diagnostics: Diagnostics {
                    warnings: vec![warning.to_string()],
                    ..Diagnostics::default()
                },
            });
        }
        Err(e) => return Err(e.into()),
    };

    // Build outputs: first handle is Main, rest are Body { index }
    let mut outputs = Vec::with_capacity(handles.len());
    let mut all_after_faces = Vec::new();
    let mut all_after_edges = Vec::new();
    let mut all_after_vertices = Vec::new();
    let mut all_role_assignments = Vec::new();

    for (i, handle) in handles.into_iter().enumerate() {
        let after = diff::snapshot(kb.as_introspect(), &handle);
        all_after_faces.extend(after.faces.clone());
        all_after_edges.extend(after.edges.clone());
        all_after_vertices.extend(after.vertices.clone());

        let roles = assign_boolean_roles(kb.as_introspect(), &handle, &snap_a, &snap_b);
        all_role_assignments.extend(roles);

        let key = if i == 0 {
            OutputKey::Main
        } else {
            OutputKey::Body { index: i }
        };
        outputs.push((
            key,
            BodyOutput {
                handle,
                mesh: None,
                edges: None,
            },
        ));
    }

    let after_merged = TopoSnapshot {
        faces: all_after_faces,
        edges: all_after_edges,
        vertices: all_after_vertices,
    };
    let diff_result = diff::diff(&before, &after_merged);

    let provenance = Provenance {
        created: diff_result.created,
        deleted: diff_result.deleted,
        modified: Vec::new(),
        role_assignments: all_role_assignments,
    };

    Ok(OpResult {
        outputs,
        provenance,
        diagnostics: Diagnostics::default(),
    })
}

/// Assign roles to boolean result faces.
fn assign_boolean_roles(
    introspect: &dyn waffle_types::kernel::KernelIntrospect,
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
