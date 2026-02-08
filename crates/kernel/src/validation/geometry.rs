//! Level 2: Geometric consistency checks.
//!
//! SameParameter verification, degenerate edge/face detection,
//! vertex-on-surface checks, and tolerance hierarchy validation.

use crate::topology::brep::*;
use super::config::ValidationConfig;
use super::types::*;

/// Run all geometric consistency checks on a solid.
pub fn check_geometry(
    store: &EntityStore,
    solid_id: SolidId,
    config: &ValidationConfig,
    errors: &mut Vec<ValidationError>,
    _warnings: &mut Vec<ValidationError>,
    metrics: &mut ValidationMetrics,
) {
    check_same_parameter(store, solid_id, config, errors, metrics);
    check_degenerate_edges(store, solid_id, config, errors);
    check_degenerate_faces(store, solid_id, config, errors);
}

/// SameParameter check: verify edge curves lie on adjacent face surfaces.
///
/// For each edge, samples points along the curve and measures
/// the distance to each adjacent face's surface.
fn check_same_parameter(
    store: &EntityStore,
    solid_id: SolidId,
    config: &ValidationConfig,
    errors: &mut Vec<ValidationError>,
    metrics: &mut ValidationMetrics,
) {
    let solid = &store.solids[solid_id];
    let mut checked_edges = std::collections::HashSet::new();
    let mut total_gap = 0.0;
    let mut gap_count = 0usize;

    for &shell_id in &solid.shells {
        let shell = &store.shells[shell_id];
        for &face_id in &shell.faces {
            let face = &store.faces[face_id];
            check_loop_same_parameter(
                store, face.outer_loop, face_id, config,
                &mut checked_edges, errors, &mut total_gap, &mut gap_count, metrics,
            );
            for &inner_loop in &face.inner_loops {
                check_loop_same_parameter(
                    store, inner_loop, face_id, config,
                    &mut checked_edges, errors, &mut total_gap, &mut gap_count, metrics,
                );
            }
        }
    }

    if gap_count > 0 {
        metrics.tolerance_stats.mean_edge_gap = total_gap / gap_count as f64;
    }
}

fn check_loop_same_parameter(
    store: &EntityStore,
    loop_id: LoopId,
    face_id: FaceId,
    config: &ValidationConfig,
    checked_edges: &mut std::collections::HashSet<u64>,
    errors: &mut Vec<ValidationError>,
    total_gap: &mut f64,
    gap_count: &mut usize,
    metrics: &mut ValidationMetrics,
) {
    use slotmap::Key;
    let loop_data = &store.loops[loop_id];
    let face = &store.faces[face_id];

    for &he_id in &loop_data.half_edges {
        let he = &store.half_edges[he_id];
        let edge_key = he.edge.data().as_ffi();

        // Only check each edge once per face.
        let check_key = edge_key.wrapping_mul(31) ^ face_id.data().as_ffi();
        if !checked_edges.insert(check_key) {
            continue;
        }

        let edge = &store.edges[he.edge];
        let n = config.sampling_density as usize;

        for i in 0..=n {
            let frac = i as f64 / n as f64;
            let t = he.t_start + (he.t_end - he.t_start) * frac;
            let p = edge.curve.evaluate(t);
            let dist = face.surface.distance_to_point(&p);

            *total_gap += dist;
            *gap_count += 1;

            if dist > metrics.tolerance_stats.max_edge_gap {
                metrics.tolerance_stats.max_edge_gap = dist;
            }

            if dist > config.tolerance.resolution {
                errors.push(ValidationError {
                    entity_type: EntityType::Edge,
                    entity_id: EntityId::Edge(he.edge),
                    parent_id: Some(EntityId::Face(face_id)),
                    code: ErrorCode::SameParameterViolation,
                    message: format!(
                        "Edge curve deviates from face surface at t={t:.4}: gap={dist:.2e} > tol={:.2e}",
                        config.tolerance.resolution
                    ),
                    severity: Severity::Error,
                    numeric_value: Some(dist),
                    tolerance: Some(config.tolerance.resolution),
                });
                // Only report the first violation per edge-face pair.
                break;
            }
        }
    }
}

/// Check for zero-length edges.
fn check_degenerate_edges(
    store: &EntityStore,
    solid_id: SolidId,
    config: &ValidationConfig,
    errors: &mut Vec<ValidationError>,
) {
    let solid = &store.solids[solid_id];
    let mut checked = std::collections::HashSet::new();

    for &shell_id in &solid.shells {
        let shell = &store.shells[shell_id];
        for &face_id in &shell.faces {
            let face = &store.faces[face_id];
            let all_loops = std::iter::once(face.outer_loop)
                .chain(face.inner_loops.iter().copied());
            for loop_id in all_loops {
                for &he_id in &store.loops[loop_id].half_edges {
                    let he = &store.half_edges[he_id];
                    use slotmap::Key;
                    if !checked.insert(he.edge.data().as_ffi()) {
                        continue;
                    }
                    let edge = &store.edges[he.edge];
                    let start = &store.vertices[edge.start_vertex].point;
                    let end = &store.vertices[edge.end_vertex].point;
                    let length = start.distance_to(end);
                    if length < config.tolerance.resolution {
                        errors.push(ValidationError {
                            entity_type: EntityType::Edge,
                            entity_id: EntityId::Edge(he.edge),
                            parent_id: None,
                            code: ErrorCode::ZeroLengthEdge,
                            message: format!("Edge has near-zero length: {length:.2e}"),
                            severity: Severity::Error,
                            numeric_value: Some(length),
                            tolerance: Some(config.tolerance.resolution),
                        });
                    }
                }
            }
        }
    }
}

/// Check for zero-area faces (estimated from vertex polygon).
fn check_degenerate_faces(
    store: &EntityStore,
    solid_id: SolidId,
    config: &ValidationConfig,
    errors: &mut Vec<ValidationError>,
) {
    let solid = &store.solids[solid_id];
    let tol_sq = config.tolerance.resolution * config.tolerance.resolution;

    for &shell_id in &solid.shells {
        let shell = &store.shells[shell_id];
        for &face_id in &shell.faces {
            let face = &store.faces[face_id];
            let loop_data = &store.loops[face.outer_loop];

            if loop_data.half_edges.len() < 3 {
                continue;
            }

            // Estimate area using Newell's method on the face polygon.
            let verts: Vec<_> = loop_data.half_edges.iter()
                .map(|&he_id| store.vertices[store.half_edges[he_id].start_vertex].point)
                .collect();

            let mut nx = 0.0;
            let mut ny = 0.0;
            let mut nz = 0.0;
            let n = verts.len();
            for i in 0..n {
                let j = (i + 1) % n;
                nx += (verts[i].y - verts[j].y) * (verts[i].z + verts[j].z);
                ny += (verts[i].z - verts[j].z) * (verts[i].x + verts[j].x);
                nz += (verts[i].x - verts[j].x) * (verts[i].y + verts[j].y);
            }
            let area_sq = nx * nx + ny * ny + nz * nz;
            let area = area_sq.sqrt() * 0.5;

            if area < tol_sq {
                errors.push(ValidationError {
                    entity_type: EntityType::Face,
                    entity_id: EntityId::Face(face_id),
                    parent_id: None,
                    code: ErrorCode::ZeroAreaFace,
                    message: format!("Face has near-zero area: {area:.2e}"),
                    severity: Severity::Error,
                    numeric_value: Some(area),
                    tolerance: Some(tol_sq),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::primitives::make_box;
    use crate::geometry::point::Point3d;
    use crate::topology::primitives::make_cylinder;

    #[test]
    fn test_box_passes_same_parameter() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let config = ValidationConfig::geometry();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut metrics = ValidationMetrics::default();

        check_geometry(&store, solid_id, &config, &mut errors, &mut warnings, &mut metrics);
        let sp_errors: Vec<_> = errors.iter().filter(|e| e.code == ErrorCode::SameParameterViolation).collect();
        assert!(sp_errors.is_empty(), "Box edges should be on face surfaces: {sp_errors:?}");
    }

    #[test]
    fn test_cylinder_passes_same_parameter() {
        let mut store = EntityStore::new();
        let solid_id = make_cylinder(&mut store, Point3d::ORIGIN, 1.0, 2.0, 16);

        let config = ValidationConfig::geometry();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut metrics = ValidationMetrics::default();

        check_geometry(&store, solid_id, &config, &mut errors, &mut warnings, &mut metrics);
        let sp_errors: Vec<_> = errors.iter().filter(|e| e.code == ErrorCode::SameParameterViolation).collect();
        assert!(sp_errors.is_empty(), "Cylinder edges should be on face surfaces: {sp_errors:?}");
    }

    #[test]
    fn test_perturbed_edge_detected() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        // Perturb an edge curve's origin by 0.1 to create a SameParameter violation.
        let edge_id = store.edges.keys().next().unwrap();
        if let crate::geometry::curves::Curve::Line(ref mut line) = store.edges[edge_id].curve {
            line.origin.z += 0.1;
        }

        let config = ValidationConfig::geometry();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut metrics = ValidationMetrics::default();

        check_geometry(&store, solid_id, &config, &mut errors, &mut warnings, &mut metrics);
        let sp_errors: Vec<_> = errors.iter().filter(|e| e.code == ErrorCode::SameParameterViolation).collect();
        assert!(!sp_errors.is_empty(), "Perturbed edge should be detected as SameParameter violation");
        // Check that the numeric value is approximately 0.1.
        let val = sp_errors[0].numeric_value.unwrap();
        assert!(val > 0.05 && val < 0.2, "Expected gap ~0.1, got {val}");
    }

    #[test]
    fn test_no_degenerate_edges_in_box() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let config = ValidationConfig::geometry();
        let mut errors = Vec::new();
        check_degenerate_edges(&store, solid_id, &config, &mut errors);
        assert!(errors.is_empty(), "Box should have no degenerate edges");
    }

    #[test]
    fn test_no_degenerate_faces_in_box() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let config = ValidationConfig::geometry();
        let mut errors = Vec::new();
        check_degenerate_faces(&store, solid_id, &config, &mut errors);
        assert!(errors.is_empty(), "Box should have no degenerate faces");
    }
}
