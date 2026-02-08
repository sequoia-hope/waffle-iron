use tracing::{info, instrument};

use crate::topology::brep::*;

/// Multi-level verification for B-Rep solids.
/// These checks are meant to be run automatically in tests and debug builds.

/// L0: Topological invariant checks.
#[instrument(skip(store))]
pub fn verify_topology_l0(store: &EntityStore, solid_id: SolidId) -> TopologyAudit {
    let audit = audit_solid(store, solid_id);
    info!(
        euler_valid = audit.euler_valid,
        all_faces_closed = audit.all_faces_closed,
        all_edges_two_faced = audit.all_edges_two_faced,
        no_dangling_vertices = audit.no_dangling_vertices,
        normals_consistent = audit.normals_consistent,
        error_count = audit.errors.len(),
        "topology L0 audit complete"
    );
    audit
}

/// L1: Geometric consistency checks.
#[instrument(skip(store))]
pub fn verify_geometry_l1(store: &EntityStore, solid_id: SolidId) -> Vec<GeometryError> {
    let mut errors = Vec::new();
    let solid = &store.solids[solid_id];

    for &shell_id in &solid.shells {
        let shell = &store.shells[shell_id];
        for &face_id in &shell.faces {
            let face = &store.faces[face_id];
            let loop_data = &store.loops[face.outer_loop];

            for &he_id in &loop_data.half_edges {
                let he = &store.half_edges[he_id];
                let edge = &store.edges[he.edge];

                // Check that vertex positions match curve endpoints.
                // The edge's curve goes from start_vertex to end_vertex of the *edge*,
                // but the half-edge may traverse it in reverse.
                let edge_start = &store.vertices[edge.start_vertex];
                let edge_end = &store.vertices[edge.end_vertex];

                let curve_start = edge.curve.evaluate(0.0);
                let curve_end = edge.curve.evaluate(
                    edge_start.point.distance_to(&edge_end.point),
                );

                let start_dist = edge_start.point.distance_to(&curve_start);
                let end_dist = edge_end.point.distance_to(&curve_end);

                let geom_tol = crate::default_tolerance().coincidence;
                if start_dist > geom_tol {
                    errors.push(GeometryError::VertexCurveMismatch {
                        vertex: edge.start_vertex,
                        edge: he.edge,
                        distance: start_dist,
                    });
                }

                if end_dist > geom_tol {
                    errors.push(GeometryError::VertexCurveMismatch {
                        vertex: edge.end_vertex,
                        edge: he.edge,
                        distance: end_dist,
                    });
                }
            }
        }
    }

    info!(
        geometry_error_count = errors.len(),
        "geometry L1 check complete"
    );
    errors
}

#[derive(Debug, Clone)]
pub enum GeometryError {
    VertexCurveMismatch {
        vertex: VertexId,
        edge: EdgeId,
        distance: f64,
    },
    EdgeNotOnSurface {
        edge: EdgeId,
        face: FaceId,
        max_distance: f64,
    },
    NormalInconsistency {
        face: FaceId,
    },
}

/// Full verification combining all levels.
#[instrument(skip(store))]
pub fn full_verify(store: &EntityStore, solid_id: SolidId) -> VerificationReport {
    let topo = verify_topology_l0(store, solid_id);
    let geom_errors = verify_geometry_l1(store, solid_id);

    let report = VerificationReport {
        topology_valid: topo.all_valid(),
        topology_audit: topo,
        geometry_errors: geom_errors,
    };
    info!(
        valid = report.is_valid(),
        topology_valid = report.topology_valid,
        geometry_errors = report.geometry_errors.len(),
        "full verification complete"
    );
    report
}

#[derive(Debug)]
pub struct VerificationReport {
    pub topology_valid: bool,
    pub topology_audit: TopologyAudit,
    pub geometry_errors: Vec<GeometryError>,
}

impl VerificationReport {
    pub fn is_valid(&self) -> bool {
        self.topology_valid && self.geometry_errors.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    use crate::topology::primitives::make_box;

    #[test]
    fn test_box_passes_topology_audit() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let audit = verify_topology_l0(&store, solid_id);
        assert!(audit.euler_valid, "Euler formula violated: {:?}", audit.errors);
        assert!(audit.all_faces_closed, "Not all faces closed");
    }

    #[test]
    fn test_box_passes_geometry_check() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let errors = verify_geometry_l1(&store, solid_id);
        assert!(errors.is_empty(), "Geometry errors: {:?}", errors);
    }

    #[test]
    fn test_full_verify_box() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, -5.0, -5.0, -5.0, 5.0, 5.0, 5.0);

        let report = full_verify(&store, solid_id);
        assert!(report.topology_valid);
    }

    #[test]
    fn test_tracing_does_not_affect_audit_results() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0);

        let audit = verify_topology_l0(&store, solid_id);
        assert!(audit.euler_valid);
        assert!(audit.all_faces_closed);
        assert!(audit.all_edges_two_faced);
        assert!(audit.all_valid());

        let geom_errors = verify_geometry_l1(&store, solid_id);
        assert!(geom_errors.is_empty());

        let report = full_verify(&store, solid_id);
        assert!(report.is_valid());
    }
}
