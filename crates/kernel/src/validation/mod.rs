pub mod audit;
pub mod volume;
pub mod types;
pub mod config;
pub mod geometry;
pub mod spatial;
pub mod continuity;
pub mod harness;

pub use types::*;
pub use config::*;

use std::collections::HashSet;
use tracing::{info, instrument};

use crate::topology::brep::*;

/// Unified B-Rep validation engine.
///
/// Runs hierarchical checks at increasing levels of sophistication:
/// - **Topology** (L0-1): Euler formula, twin consistency, loop closure, normals, vertex-on-curve.
/// - **Geometry** (L2): SameParameter, degenerate edges/faces, vertex-on-surface.
/// - **Spatial** (L3): Free edges, non-manifold edges, self-intersection.
/// - **Full** (L5): G0/G1 continuity across edges.
pub struct BRepValidator {
    config: ValidationConfig,
}

impl BRepValidator {
    pub fn new(config: ValidationConfig) -> Self {
        Self { config }
    }

    /// Validate a solid, returning a unified report.
    #[instrument(skip(self, store))]
    pub fn validate(&self, store: &EntityStore, solid_id: SolidId) -> ValidationReport {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut metrics = ValidationMetrics::default();

        // Always compute entity counts.
        metrics.entity_counts = compute_entity_counts(store, solid_id);

        // Level 0-1: Topology checks (adapted from existing audit infrastructure).
        self.check_topology(store, solid_id, &mut errors, &mut warnings);

        let mut level_completed = ValidationLevel::Topology;

        // Short-circuit: if topology has errors and we're only doing topology, stop.
        if self.config.level >= ValidationLevel::Geometry && errors.is_empty() {
            // Level 2: Geometric consistency.
            geometry::check_geometry(store, solid_id, &self.config, &mut errors, &mut warnings, &mut metrics);
            level_completed = ValidationLevel::Geometry;
        } else if self.config.level >= ValidationLevel::Geometry {
            // Topology errors exist — still run geometry but note we had topo errors.
            geometry::check_geometry(store, solid_id, &self.config, &mut errors, &mut warnings, &mut metrics);
            level_completed = ValidationLevel::Geometry;
        }

        if self.config.level >= ValidationLevel::Spatial {
            spatial::check_spatial(store, solid_id, &self.config, &mut errors, &mut warnings);
            level_completed = ValidationLevel::Spatial;
        }

        if self.config.level >= ValidationLevel::Full && self.config.check_continuity {
            continuity::check_continuity(store, solid_id, &self.config, &mut errors, &mut warnings);
            level_completed = ValidationLevel::Full;
        }

        let valid = errors.is_empty();

        info!(
            valid,
            level = ?level_completed,
            error_count = errors.len(),
            warning_count = warnings.len(),
            "validation complete"
        );

        ValidationReport {
            valid,
            level_completed,
            errors,
            warnings,
            metrics,
        }
    }

    /// Adapt existing topology audit into ValidationError format.
    fn check_topology(
        &self,
        store: &EntityStore,
        solid_id: SolidId,
        errors: &mut Vec<ValidationError>,
        warnings: &mut Vec<ValidationError>,
    ) {
        let audit = audit::verify_topology_l0(store, solid_id);

        // Convert TopologyError variants to ValidationError.
        for topo_err in &audit.errors {
            match topo_err {
                TopologyError::EulerViolation { shell, v, e, f, expected_chi, actual_chi } => {
                    errors.push(ValidationError {
                        entity_type: EntityType::Shell,
                        entity_id: EntityId::Shell(*shell),
                        parent_id: Some(EntityId::Solid(solid_id)),
                        code: ErrorCode::EulerPoincareViolation,
                        message: format!(
                            "Euler formula violated: V={v} E={e} F={f}, chi={actual_chi} (expected {expected_chi})"
                        ),
                        severity: Severity::Error,
                        numeric_value: Some(*actual_chi as f64),
                        tolerance: Some(*expected_chi as f64),
                    });
                }
                TopologyError::OpenLoop { loop_id } => {
                    errors.push(ValidationError {
                        entity_type: EntityType::Loop,
                        entity_id: EntityId::Loop(*loop_id),
                        parent_id: None,
                        code: ErrorCode::WireNotClosed,
                        message: "Loop does not close: last vertex != first vertex".into(),
                        severity: Severity::Error,
                        numeric_value: None,
                        tolerance: None,
                    });
                }
                TopologyError::DanglingVertex { vertex } => {
                    errors.push(ValidationError {
                        entity_type: EntityType::Vertex,
                        entity_id: EntityId::Vertex(*vertex),
                        parent_id: None,
                        code: ErrorCode::DanglingReference,
                        message: "Vertex referenced by fewer than 2 half-edges".into(),
                        severity: Severity::Error,
                        numeric_value: None,
                        tolerance: None,
                    });
                }
                TopologyError::HalfEdgeTwinMismatch { half_edge } => {
                    errors.push(ValidationError {
                        entity_type: EntityType::HalfEdge,
                        entity_id: EntityId::HalfEdge(*half_edge),
                        parent_id: None,
                        code: ErrorCode::HalfEdgeTwinMismatch,
                        message: "Half-edge twin does not point back to this half-edge".into(),
                        severity: Severity::Error,
                        numeric_value: None,
                        tolerance: None,
                    });
                }
                TopologyError::VertexPositionMismatch { vertex, edge, distance, .. } => {
                    errors.push(ValidationError {
                        entity_type: EntityType::Vertex,
                        entity_id: EntityId::Vertex(*vertex),
                        parent_id: Some(EntityId::Edge(*edge)),
                        code: ErrorCode::InvalidPointOnCurve,
                        message: format!(
                            "Vertex position does not match curve endpoint (gap={distance:.2e})"
                        ),
                        severity: Severity::Error,
                        numeric_value: Some(*distance),
                        tolerance: Some(self.config.tolerance.resolution),
                    });
                }
            }
        }

        // Also adapt the L1 geometry checks (vertex-on-curve).
        let geom_errors = audit::verify_geometry_l1(store, solid_id);
        for ge in &geom_errors {
            match ge {
                audit::GeometryError::VertexCurveMismatch { vertex, edge, distance } => {
                    errors.push(ValidationError {
                        entity_type: EntityType::Vertex,
                        entity_id: EntityId::Vertex(*vertex),
                        parent_id: Some(EntityId::Edge(*edge)),
                        code: ErrorCode::InvalidPointOnCurve,
                        message: format!(
                            "Vertex not on curve endpoint (gap={distance:.2e})"
                        ),
                        severity: Severity::Error,
                        numeric_value: Some(*distance),
                        tolerance: Some(self.config.tolerance.resolution),
                    });
                }
                audit::GeometryError::EdgeNotOnSurface { edge, face, max_distance } => {
                    errors.push(ValidationError {
                        entity_type: EntityType::Edge,
                        entity_id: EntityId::Edge(*edge),
                        parent_id: Some(EntityId::Face(*face)),
                        code: ErrorCode::SameParameterViolation,
                        message: format!(
                            "Edge curve deviates from face surface (max_gap={max_distance:.2e})"
                        ),
                        severity: Severity::Error,
                        numeric_value: Some(*max_distance),
                        tolerance: Some(self.config.tolerance.resolution),
                    });
                }
                audit::GeometryError::NormalInconsistency { face } => {
                    warnings.push(ValidationError {
                        entity_type: EntityType::Face,
                        entity_id: EntityId::Face(*face),
                        parent_id: None,
                        code: ErrorCode::BadOrientationOfFaces,
                        message: "Face normal inconsistent with surface normal direction".into(),
                        severity: Severity::Warning,
                        numeric_value: None,
                        tolerance: None,
                    });
                }
            }
        }

        // Check normals consistency from the topology audit.
        if !audit.normals_consistent {
            warnings.push(ValidationError {
                entity_type: EntityType::Solid,
                entity_id: EntityId::Solid(solid_id),
                parent_id: None,
                code: ErrorCode::BadOrientationOfFaces,
                message: "Some face normals are inconsistent with winding order".into(),
                severity: Severity::Warning,
                numeric_value: None,
                tolerance: None,
            });
        }
    }
}

/// Count topological entities for a solid.
fn compute_entity_counts(store: &EntityStore, solid_id: SolidId) -> EntityCounts {
    let solid = &store.solids[solid_id];
    let mut vertex_set = HashSet::new();
    let mut edge_set = HashSet::new();
    let mut he_count = 0usize;
    let mut face_count = 0usize;
    let mut loop_count = 0usize;

    for &shell_id in &solid.shells {
        let shell = &store.shells[shell_id];
        for &face_id in &shell.faces {
            face_count += 1;
            let face = &store.faces[face_id];
            loop_count += 1; // outer loop
            loop_count += face.inner_loops.len();

            let loop_data = &store.loops[face.outer_loop];
            for &he_id in &loop_data.half_edges {
                he_count += 1;
                let he = &store.half_edges[he_id];
                use slotmap::Key;
                edge_set.insert(he.edge.data().as_ffi());
                vertex_set.insert(he.start_vertex.data().as_ffi());
                vertex_set.insert(he.end_vertex.data().as_ffi());
            }
            for &inner_loop in &face.inner_loops {
                let ld = &store.loops[inner_loop];
                for &he_id in &ld.half_edges {
                    he_count += 1;
                    let he = &store.half_edges[he_id];
                    use slotmap::Key;
                    edge_set.insert(he.edge.data().as_ffi());
                    vertex_set.insert(he.start_vertex.data().as_ffi());
                    vertex_set.insert(he.end_vertex.data().as_ffi());
                }
            }
        }
    }

    EntityCounts {
        vertices: vertex_set.len(),
        edges: edge_set.len(),
        half_edges: he_count,
        faces: face_count,
        shells: solid.shells.len(),
        loops: loop_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::primitives::{make_box, make_cylinder, make_sphere};
    use crate::geometry::point::Point3d;

    #[test]
    fn test_box_passes_topology_validation() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let validator = BRepValidator::new(ValidationConfig::topology());
        let report = validator.validate(&store, solid_id);

        assert!(report.valid, "Box should pass topology validation: {report}");
        assert_eq!(report.error_count(), 0);
        assert_eq!(report.metrics.entity_counts.vertices, 8);
        assert_eq!(report.metrics.entity_counts.edges, 12);
        assert_eq!(report.metrics.entity_counts.faces, 6);
        assert_eq!(report.metrics.entity_counts.shells, 1);
    }

    #[test]
    fn test_cylinder_passes_topology_validation() {
        let mut store = EntityStore::new();
        let solid_id = make_cylinder(&mut store, Point3d::ORIGIN, 1.0, 2.0, 16);

        let validator = BRepValidator::new(ValidationConfig::topology());
        let report = validator.validate(&store, solid_id);

        assert!(report.valid, "Cylinder should pass topology validation: {report}");
        assert_eq!(report.error_count(), 0);
    }

    #[test]
    fn test_sphere_passes_topology_validation() {
        let mut store = EntityStore::new();
        let solid_id = make_sphere(&mut store, Point3d::ORIGIN, 1.0, 8, 6);

        let validator = BRepValidator::new(ValidationConfig::topology());
        let report = validator.validate(&store, solid_id);

        assert!(report.valid, "Sphere should pass topology validation: {report}");
        assert_eq!(report.error_count(), 0);
    }

    #[test]
    fn test_corrupted_twin_detected() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        // Corrupt a twin link: find any half-edge and point its twin to itself.
        let he_id = store.half_edges.keys().next().unwrap();
        store.half_edges[he_id].twin = he_id; // self-twin = broken

        let validator = BRepValidator::new(ValidationConfig::topology());
        let report = validator.validate(&store, solid_id);

        assert!(!report.valid, "Corrupted twin should fail validation");
        assert!(!report.no_errors_of(ErrorCode::HalfEdgeTwinMismatch));
    }

    #[test]
    fn test_removed_face_euler_violation() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        // Remove a face from the shell's face list.
        let shell_id = store.solids[solid_id].shells[0];
        store.shells[shell_id].faces.pop();

        let validator = BRepValidator::new(ValidationConfig::topology());
        let report = validator.validate(&store, solid_id);

        assert!(!report.valid, "Missing face should cause Euler violation");
        assert!(!report.no_errors_of(ErrorCode::EulerPoincareViolation));
    }

    #[test]
    fn test_open_loop_detected() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        // Remove the last half-edge from the first face's outer loop.
        let shell_id = store.solids[solid_id].shells[0];
        let face_id = store.shells[shell_id].faces[0];
        let loop_id = store.faces[face_id].outer_loop;
        store.loops[loop_id].half_edges.pop();

        let validator = BRepValidator::new(ValidationConfig::topology());
        let report = validator.validate(&store, solid_id);

        assert!(!report.valid, "Open loop should fail validation");
        assert!(!report.no_errors_of(ErrorCode::WireNotClosed));
    }

    #[test]
    fn test_entity_counts_box() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let counts = compute_entity_counts(&store, solid_id);
        assert_eq!(counts.vertices, 8, "Box has 8 vertices");
        assert_eq!(counts.edges, 12, "Box has 12 edges");
        assert_eq!(counts.faces, 6, "Box has 6 faces");
        assert_eq!(counts.shells, 1, "Box has 1 shell");
        assert_eq!(counts.loops, 6, "Box has 6 loops (one per face)");
        // Each face has 4 half-edges, 6 faces = 24 half-edges
        assert_eq!(counts.half_edges, 24, "Box has 24 half-edges");
    }

    #[test]
    fn test_validation_report_filtering() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        // Corrupt to get a specific error type.
        let he_id = store.half_edges.keys().next().unwrap();
        store.half_edges[he_id].twin = he_id;

        let validator = BRepValidator::new(ValidationConfig::topology());
        let report = validator.validate(&store, solid_id);

        let twin_errors = report.errors_of(ErrorCode::HalfEdgeTwinMismatch);
        assert!(!twin_errors.is_empty());
        assert!(report.no_errors_of(ErrorCode::WireNotClosed));
    }
}
