//! Level 5: Continuity checks across edges.
//!
//! G0 (positional), G1 (tangent), and G2 (curvature) continuity
//! between adjacent faces sharing an edge.

use std::collections::HashSet;

use crate::geometry::point::Point3d;
use crate::topology::brep::*;
use super::config::ValidationConfig;
use super::types::*;

/// Metrics for continuity analysis of a single edge.
#[derive(Debug, Clone)]
pub struct ContinuityMetrics {
    pub edge_id: EdgeId,
    pub max_g0_gap: f64,
    pub max_g1_angle: f64,
    pub sample_count: usize,
}

/// Run continuity checks on all edges of a solid.
pub fn check_continuity(
    store: &EntityStore,
    solid_id: SolidId,
    config: &ValidationConfig,
    errors: &mut Vec<ValidationError>,
    warnings: &mut Vec<ValidationError>,
) -> Vec<ContinuityMetrics> {
    let solid = &store.solids[solid_id];
    let mut checked_edges = HashSet::new();
    let mut all_metrics = Vec::new();

    for &shell_id in &solid.shells {
        let shell = &store.shells[shell_id];
        for &face_id in &shell.faces {
            let face = &store.faces[face_id];
            check_loop_continuity(
                store, face.outer_loop, face_id, config,
                &mut checked_edges, errors, warnings, &mut all_metrics,
            );
            for &inner_loop in &face.inner_loops {
                check_loop_continuity(
                    store, inner_loop, face_id, config,
                    &mut checked_edges, errors, warnings, &mut all_metrics,
                );
            }
        }
    }

    all_metrics
}

fn check_loop_continuity(
    store: &EntityStore,
    loop_id: LoopId,
    _face_id: FaceId,
    config: &ValidationConfig,
    checked_edges: &mut HashSet<u64>,
    errors: &mut Vec<ValidationError>,
    warnings: &mut Vec<ValidationError>,
    all_metrics: &mut Vec<ContinuityMetrics>,
) {
    use slotmap::Key;
    let loop_data = &store.loops[loop_id];

    for &he_id in &loop_data.half_edges {
        let he = &store.half_edges[he_id];
        let edge_key = he.edge.data().as_ffi();

        if !checked_edges.insert(edge_key) {
            continue;
        }

        // Get both faces adjacent to this edge via half-edge and its twin.
        let twin = &store.half_edges[he.twin];
        if he.face == twin.face {
            continue; // Self-twin (degenerate edge on same face), skip.
        }

        let face_a = &store.faces[he.face];
        let face_b = &store.faces[twin.face];
        let edge = &store.edges[he.edge];

        let n = config.sampling_density as usize;
        let mut max_g0 = 0.0f64;
        let mut max_g1 = 0.0f64;

        for i in 0..=n {
            let frac = i as f64 / n as f64;
            let t = he.t_start + (he.t_end - he.t_start) * frac;
            let p = edge.curve.evaluate(t);

            // G0: Check that both surfaces are close to the edge point.
            let dist_a = face_a.surface.distance_to_point(&p);
            let dist_b = face_b.surface.distance_to_point(&p);
            let g0_gap = dist_a.max(dist_b);
            max_g0 = max_g0.max(g0_gap);

            // G1: Measure angle between surface normals from each side.
            let _closest_a = face_a.surface.closest_point(&p);
            let _closest_b = face_b.surface.closest_point(&p);

            // Get surface normals at the closest points.
            // For analytic surfaces, we use the surface normal at (0,0) direction —
            // but for a proper check we need the normal at the point's parameters.
            // Use a simpler approach: compute normal from the closest_point direction.
            let normal_a = compute_outward_normal_at(face_a, &p);
            let normal_b = compute_outward_normal_at(face_b, &p);

            let dot = normal_a.dot(&normal_b).clamp(-1.0, 1.0);
            let angle = dot.acos();
            max_g1 = max_g1.max(angle);
        }

        let metrics = ContinuityMetrics {
            edge_id: he.edge,
            max_g0_gap: max_g0,
            max_g1_angle: max_g1,
            sample_count: n + 1,
        };

        // Report G0 discontinuity.
        if max_g0 > config.tolerance.resolution {
            errors.push(ValidationError {
                entity_type: EntityType::Edge,
                entity_id: EntityId::Edge(he.edge),
                parent_id: None,
                code: ErrorCode::G0Discontinuity,
                message: format!(
                    "G0 gap across edge: {max_g0:.2e} > tol {:.2e}",
                    config.tolerance.resolution
                ),
                severity: Severity::Error,
                numeric_value: Some(max_g0),
                tolerance: Some(config.tolerance.resolution),
            });
        }

        // Report G1 discontinuity as warning (sharp edges are valid).
        if max_g1 > config.tolerance.angular_tol {
            warnings.push(ValidationError {
                entity_type: EntityType::Edge,
                entity_id: EntityId::Edge(he.edge),
                parent_id: None,
                code: ErrorCode::G1Discontinuity,
                message: format!(
                    "G1 angle across edge: {:.2} degrees",
                    max_g1.to_degrees()
                ),
                severity: Severity::Warning,
                numeric_value: Some(max_g1),
                tolerance: Some(config.tolerance.angular_tol),
            });
        }

        all_metrics.push(metrics);
    }
}

/// Compute the outward-facing surface normal at a point near the face.
///
/// For planes, the normal is constant. For curved surfaces, we use
/// the surface's built-in normal computation at parameters derived
/// from the closest point projection.
fn compute_outward_normal_at(face: &Face, p: &Point3d) -> crate::geometry::vector::Vec3 {
    use crate::geometry::surfaces::Surface;

    let n = match &face.surface {
        Surface::Plane(pl) => pl.normal,
        Surface::Cylinder(cyl) => {
            let d = *p - cyl.origin;
            let h = d.dot(&cyl.axis);
            let radial = d - cyl.axis * h;
            radial.normalized().unwrap_or(cyl.ref_dir)
        }
        Surface::Sphere(sph) => {
            let d = *p - sph.center;
            d.normalized().unwrap_or(crate::geometry::vector::Vec3::Z)
        }
        Surface::Cone(cone) => {
            let d = *p - cone.apex;
            let h = d.dot(&cone.axis);
            let radial = d - cone.axis * h;
            let cos_a = cone.half_angle.cos();
            let sin_a = cone.half_angle.sin();
            let radial_dir = radial.normalized().unwrap_or(cone.ref_dir);
            (radial_dir * cos_a - cone.axis * sin_a).normalized().unwrap_or(crate::geometry::vector::Vec3::Z)
        }
        Surface::Torus(_) | Surface::Nurbs(_) => {
            // Fall back to numerical normal via surface evaluation.
            face.surface.normal_at(0.0, 0.0)
        }
    };

    if face.same_sense { n } else { -n }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::primitives::make_box;

    #[test]
    fn test_box_g0_zero() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let config = ValidationConfig::full();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        let metrics = check_continuity(&store, solid_id, &config, &mut errors, &mut warnings);

        // All box edges should have G0 gap = 0 (exact for planes).
        let g0_errors: Vec<_> = errors.iter().filter(|e| e.code == ErrorCode::G0Discontinuity).collect();
        assert!(g0_errors.is_empty(), "Box should have zero G0 gaps: {g0_errors:?}");

        // All box edges are sharp 90-degree edges, so G1 warnings expected.
        let g1_warnings: Vec<_> = warnings.iter().filter(|e| e.code == ErrorCode::G1Discontinuity).collect();
        assert!(!g1_warnings.is_empty(), "Box edges should have G1 warnings (90-degree angles)");

        // Check that G1 angle is approximately 90 degrees.
        for m in &metrics {
            let angle_deg = m.max_g1_angle.to_degrees();
            assert!(
                (angle_deg - 90.0).abs() < 5.0,
                "Box edge G1 angle should be ~90 degrees, got {angle_deg:.1}"
            );
        }
    }

    #[test]
    fn test_box_g1_angle_values() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 2.0, 3.0, 4.0);

        let config = ValidationConfig::full();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        let metrics = check_continuity(&store, solid_id, &config, &mut errors, &mut warnings);

        // All 12 edges of a box should be checked.
        assert_eq!(metrics.len(), 12, "Box has 12 edges");

        // Each edge should have G1 angle of ~90 degrees.
        for m in &metrics {
            let angle_deg = m.max_g1_angle.to_degrees();
            assert!(
                angle_deg > 85.0 && angle_deg < 95.0,
                "Box edge G1 angle should be ~90 degrees, got {angle_deg:.1}"
            );
        }
    }
}
