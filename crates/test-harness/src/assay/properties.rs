//! Oracle-based property checkers for boolean operation results.
//!
//! Each function checks a single algebraic/geometric/topological invariant.
//! Results are returned as structured data, not panics.

use crate::helpers::{mesh_bounding_box, mesh_volume};
use crate::oracle;
use crate::workflow::ModelBuilder;
use waffle_types::kernel::RenderMesh;

use super::strategies::BoolOp;

/// Result of checking a single property.
#[derive(Debug, Clone)]
pub struct PropertyResult {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

impl PropertyResult {
    pub fn pass(name: &str, detail: String) -> Self {
        Self {
            name: name.to_string(),
            passed: true,
            detail,
        }
    }

    pub fn fail(name: &str, detail: String) -> Self {
        Self {
            name: name.to_string(),
            passed: false,
            detail,
        }
    }
}

/// Check that boolean result volume satisfies monotonicity constraints.
///
/// - Union: vol(A∪B) >= max(vol(A), vol(B))
/// - Subtract: vol(A-B) <= vol(A)
/// - Intersect: vol(A∩B) <= min(vol(A), vol(B))
pub fn check_volume_monotonicity(
    mesh_a: &RenderMesh,
    mesh_b: &RenderMesh,
    mesh_result: &RenderMesh,
    op: BoolOp,
) -> PropertyResult {
    let vol_a = mesh_volume(mesh_a);
    let vol_b = mesh_volume(mesh_b);
    let vol_r = mesh_volume(mesh_result);
    // Tolerance for volume comparison (tessellation introduces error)
    let tol = (vol_a + vol_b) * 0.05;

    match op {
        BoolOp::Union => {
            let min_expected = vol_a.max(vol_b);
            if vol_r >= min_expected - tol {
                PropertyResult::pass(
                    "volume_monotonicity_union",
                    format!(
                        "vol(A∪B)={:.3} >= max(vol(A)={:.3}, vol(B)={:.3})",
                        vol_r, vol_a, vol_b
                    ),
                )
            } else {
                PropertyResult::fail(
                    "volume_monotonicity_union",
                    format!(
                        "vol(A∪B)={:.3} < max(vol(A)={:.3}, vol(B)={:.3})",
                        vol_r, vol_a, vol_b
                    ),
                )
            }
        }
        BoolOp::Subtract => {
            if vol_r <= vol_a + tol {
                PropertyResult::pass(
                    "volume_monotonicity_subtract",
                    format!("vol(A-B)={:.3} <= vol(A)={:.3}", vol_r, vol_a),
                )
            } else {
                PropertyResult::fail(
                    "volume_monotonicity_subtract",
                    format!("vol(A-B)={:.3} > vol(A)={:.3}", vol_r, vol_a),
                )
            }
        }
        BoolOp::Intersect => {
            let max_expected = vol_a.min(vol_b);
            if vol_r <= max_expected + tol {
                PropertyResult::pass(
                    "volume_monotonicity_intersect",
                    format!(
                        "vol(A∩B)={:.3} <= min(vol(A)={:.3}, vol(B)={:.3})",
                        vol_r, vol_a, vol_b
                    ),
                )
            } else {
                PropertyResult::fail(
                    "volume_monotonicity_intersect",
                    format!(
                        "vol(A∩B)={:.3} > min(vol(A)={:.3}, vol(B)={:.3})",
                        vol_r, vol_a, vol_b
                    ),
                )
            }
        }
    }
}

/// Check Euler's formula on topology counts: V - E + F = 2 (genus-0).
pub fn check_euler_invariant(v: usize, e: usize, f: usize) -> PropertyResult {
    let chi = v as i64 - e as i64 + f as i64;
    if chi == 2 {
        PropertyResult::pass(
            "euler_invariant",
            format!("V({}) - E({}) + F({}) = 2", v, e, f),
        )
    } else {
        PropertyResult::fail(
            "euler_invariant",
            format!("V({}) - E({}) + F({}) = {} (expected 2)", v, e, f, chi),
        )
    }
}

/// Check bounding box containment for boolean results.
///
/// - Union: bbox(result) contains both bbox(A) and bbox(B)
/// - Subtract: bbox(result) contained within bbox(A)
/// - Intersect: bbox(result) contained within both bbox(A) and bbox(B)
pub fn check_bbox_containment(
    mesh_a: &RenderMesh,
    mesh_b: &RenderMesh,
    mesh_result: &RenderMesh,
    op: BoolOp,
) -> PropertyResult {
    let (min_a, max_a) = mesh_bounding_box(mesh_a);
    let (min_b, max_b) = mesh_bounding_box(mesh_b);
    let (min_r, max_r) = mesh_bounding_box(mesh_result);
    let tol = 0.5f32; // tessellation tolerance

    match op {
        BoolOp::Union => {
            // Result bbox must contain both input bboxes
            let mut ok = true;
            for i in 0..3 {
                if min_r[i] > min_a[i].min(min_b[i]) + tol {
                    ok = false;
                }
                if max_r[i] < max_a[i].max(max_b[i]) - tol {
                    ok = false;
                }
            }
            if ok {
                PropertyResult::pass(
                    "bbox_containment_union",
                    format!(
                        "bbox(A∪B) contains both inputs: ({:.1},{:.1},{:.1})-({:.1},{:.1},{:.1})",
                        min_r[0], min_r[1], min_r[2], max_r[0], max_r[1], max_r[2]
                    ),
                )
            } else {
                PropertyResult::fail(
                    "bbox_containment_union",
                    format!(
                        "bbox(A∪B) ({:.1},{:.1},{:.1})-({:.1},{:.1},{:.1}) doesn't contain both",
                        min_r[0], min_r[1], min_r[2], max_r[0], max_r[1], max_r[2]
                    ),
                )
            }
        }
        BoolOp::Subtract => {
            // Result bbox must be within bbox(A)
            let mut ok = true;
            for i in 0..3 {
                if min_r[i] < min_a[i] - tol {
                    ok = false;
                }
                if max_r[i] > max_a[i] + tol {
                    ok = false;
                }
            }
            if ok {
                PropertyResult::pass(
                    "bbox_containment_subtract",
                    "bbox(A-B) within bbox(A)".to_string(),
                )
            } else {
                PropertyResult::fail(
                    "bbox_containment_subtract",
                    format!(
                        "bbox(A-B) ({:.1},{:.1},{:.1})-({:.1},{:.1},{:.1}) outside bbox(A)",
                        min_r[0], min_r[1], min_r[2], max_r[0], max_r[1], max_r[2]
                    ),
                )
            }
        }
        BoolOp::Intersect => {
            // Result bbox must be within both bbox(A) and bbox(B)
            let mut ok = true;
            for i in 0..3 {
                if min_r[i] < min_a[i].max(min_b[i]) - tol {
                    ok = false;
                }
                if max_r[i] > max_a[i].min(max_b[i]) + tol {
                    ok = false;
                }
            }
            if ok {
                PropertyResult::pass(
                    "bbox_containment_intersect",
                    "bbox(A∩B) within intersection of input bboxes".to_string(),
                )
            } else {
                PropertyResult::fail(
                    "bbox_containment_intersect",
                    format!(
                        "bbox(A∩B) ({:.1},{:.1},{:.1})-({:.1},{:.1},{:.1}) outside bbox intersection",
                        min_r[0], min_r[1], min_r[2], max_r[0], max_r[1], max_r[2]
                    ),
                )
            }
        }
    }
}

/// Check that a mesh is watertight (wraps oracle).
pub fn check_watertight(mesh: &RenderMesh) -> PropertyResult {
    let verdict = oracle::check_watertight_mesh(mesh);
    PropertyResult {
        name: "watertight".to_string(),
        passed: verdict.passed,
        detail: verdict.detail,
    }
}

/// Check that a mesh has manifold edges (wraps oracle).
pub fn check_manifold_mesh(mesh: &RenderMesh) -> PropertyResult {
    // Use the watertight check as a proxy — every edge shared by exactly 2 tris
    let verdict = oracle::check_watertight_mesh(mesh);
    PropertyResult {
        name: "manifold".to_string(),
        passed: verdict.passed,
        detail: verdict.detail,
    }
}

/// Run all boolean property checks on a completed ModelBuilder.
///
/// Requires that body_a, body_b, and result features are already built.
pub fn run_all_boolean_properties(
    builder: &mut ModelBuilder,
    body_a_name: &str,
    body_b_name: &str,
    result_name: &str,
    op: BoolOp,
) -> Vec<PropertyResult> {
    let mut results = Vec::new();

    // Tessellate all three bodies
    let mesh_a = match builder.tessellate(body_a_name) {
        Ok(m) => m,
        Err(e) => {
            results.push(PropertyResult::fail(
                "tessellate_a",
                format!("Failed to tessellate {}: {}", body_a_name, e),
            ));
            return results;
        }
    };
    let mesh_b = match builder.tessellate(body_b_name) {
        Ok(m) => m,
        Err(e) => {
            results.push(PropertyResult::fail(
                "tessellate_b",
                format!("Failed to tessellate {}: {}", body_b_name, e),
            ));
            return results;
        }
    };
    let mesh_r = match builder.tessellate(result_name) {
        Ok(m) => m,
        Err(e) => {
            results.push(PropertyResult::fail(
                "tessellate_result",
                format!("Failed to tessellate {}: {}", result_name, e),
            ));
            return results;
        }
    };

    // Volume monotonicity
    results.push(check_volume_monotonicity(&mesh_a, &mesh_b, &mesh_r, op));

    // Bounding box containment
    results.push(check_bbox_containment(&mesh_a, &mesh_b, &mesh_r, op));

    // Watertight check on result
    results.push(check_watertight(&mesh_r));

    // Euler invariant on result topology
    if let Ok((v, e, f)) = builder.topology_counts(result_name) {
        results.push(check_euler_invariant(v, e, f));
    }

    results
}
