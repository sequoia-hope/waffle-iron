//! Rich assertion helpers with diagnostic output.
//!
//! Every failure includes: expected vs actual, current feature tree summary,
//! and any engine errors for maximum debuggability.

use kernel_fork::types::RenderMesh;
use kernel_fork::KernelSolidHandle;
use modeling_ops::types::OpResult;
use modeling_ops::KernelBundle;
use waffle_types::Role;
use wasm_bridge::EngineState;

use crate::helpers::HarnessError;

/// Assert exact topology counts (V, E, F) for a solid.
pub fn assert_topology_eq(
    kb: &dyn KernelBundle,
    solid: &KernelSolidHandle,
    expected_v: usize,
    expected_e: usize,
    expected_f: usize,
    ctx: &str,
) -> Result<(), HarnessError> {
    let introspect = kb.as_introspect();
    let v = introspect.list_vertices(solid).len();
    let e = introspect.list_edges(solid).len();
    let f = introspect.list_faces(solid).len();

    if v == expected_v && e == expected_e && f == expected_f {
        Ok(())
    } else {
        Err(HarnessError::AssertionFailed {
            detail: format!(
                "[{}] expected V={} E={} F={}, got V={} E={} F={}",
                ctx, expected_v, expected_e, expected_f, v, e, f,
            ),
        })
    }
}

/// Assert the mesh bounding box matches expected values within tolerance.
pub fn assert_bounding_box(
    mesh: &RenderMesh,
    expected_min: [f32; 3],
    expected_max: [f32; 3],
    tol: f32,
    ctx: &str,
) -> Result<(), HarnessError> {
    let (actual_min, actual_max) = crate::helpers::mesh_bounding_box(mesh);

    for i in 0..3 {
        if (actual_min[i] - expected_min[i]).abs() > tol {
            return Err(HarnessError::AssertionFailed {
                detail: format!(
                    "[{}] bounding box min[{}]: expected {:.3}, got {:.3} (tol={})",
                    ctx, i, expected_min[i], actual_min[i], tol,
                ),
            });
        }
        if (actual_max[i] - expected_max[i]).abs() > tol {
            return Err(HarnessError::AssertionFailed {
                detail: format!(
                    "[{}] bounding box max[{}]: expected {:.3}, got {:.3} (tol={})",
                    ctx, i, expected_max[i], actual_max[i], tol,
                ),
            });
        }
    }
    Ok(())
}

/// Assert that a specific role is assigned in an OpResult.
pub fn assert_role_assigned(op: &OpResult, role: &Role, ctx: &str) -> Result<(), HarnessError> {
    let found = op
        .provenance
        .role_assignments
        .iter()
        .any(|(_, r)| r == role);

    if found {
        Ok(())
    } else {
        let available: Vec<String> = op
            .provenance
            .role_assignments
            .iter()
            .map(|(_, r)| format!("{:?}", r))
            .collect();
        Err(HarnessError::AssertionFailed {
            detail: format!(
                "[{}] expected role {:?} not found. Available: [{}]",
                ctx,
                role,
                available.join(", "),
            ),
        })
    }
}

/// Assert the feature tree structure matches expected (name, op_type) pairs.
pub fn assert_tree_structure(
    state: &EngineState,
    expected: &[(&str, &str)],
) -> Result<(), HarnessError> {
    let actual: Vec<(String, String)> = state
        .engine
        .tree
        .features
        .iter()
        .map(|f| {
            let op_type = match &f.operation {
                feature_engine::types::Operation::Sketch { .. } => "Sketch",
                feature_engine::types::Operation::Extrude { .. } => "Extrude",
                feature_engine::types::Operation::Revolve { .. } => "Revolve",
                feature_engine::types::Operation::Fillet { .. } => "Fillet",
                feature_engine::types::Operation::Chamfer { .. } => "Chamfer",
                feature_engine::types::Operation::Shell { .. } => "Shell",
                feature_engine::types::Operation::BooleanCombine { .. } => "Boolean",
            };
            (f.name.clone(), op_type.to_string())
        })
        .collect();

    if actual.len() != expected.len() {
        let errors: Vec<String> = state
            .engine
            .errors
            .iter()
            .map(|(id, msg)| format!("  {}: {}", id, msg))
            .collect();
        return Err(HarnessError::AssertionFailed {
            detail: format!(
                "tree length mismatch: expected {}, got {}.\nActual: {:?}\nErrors:\n{}",
                expected.len(),
                actual.len(),
                actual,
                if errors.is_empty() {
                    "  none".to_string()
                } else {
                    errors.join("\n")
                },
            ),
        });
    }

    for (i, ((act_name, act_type), (exp_name, exp_type))) in
        actual.iter().zip(expected.iter()).enumerate()
    {
        if act_name != exp_name || act_type != exp_type {
            return Err(HarnessError::AssertionFailed {
                detail: format!(
                    "tree mismatch at index {}: expected (\"{}\", \"{}\"), got (\"{}\", \"{}\")",
                    i, exp_name, exp_type, act_name, act_type,
                ),
            });
        }
    }

    Ok(())
}
