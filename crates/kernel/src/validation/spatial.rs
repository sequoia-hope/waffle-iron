//! Level 3: Spatial coherence checks.
//!
//! Free edge detection, non-manifold edge detection, and self-intersection.

use std::collections::HashMap;

use crate::topology::brep::*;
use super::config::ValidationConfig;
use super::types::*;

/// Run all spatial coherence checks on a solid.
pub fn check_spatial(
    store: &EntityStore,
    solid_id: SolidId,
    config: &ValidationConfig,
    errors: &mut Vec<ValidationError>,
    _warnings: &mut Vec<ValidationError>,
) {
    check_free_edges(store, solid_id, errors);
    check_non_manifold_edges(store, solid_id, errors);

    if config.check_self_intersection {
        check_self_intersection(store, solid_id, errors);
    }
}

/// Check for free edges (shared by only one face) and non-manifold edges (shared by >2 faces).
fn build_edge_face_map(store: &EntityStore, solid_id: SolidId) -> HashMap<u64, Vec<FaceId>> {
    let solid = &store.solids[solid_id];
    let mut edge_faces: HashMap<u64, Vec<FaceId>> = HashMap::new();

    for &shell_id in &solid.shells {
        let shell = &store.shells[shell_id];
        for &face_id in &shell.faces {
            let face = &store.faces[face_id];
            collect_loop_edges(store, face.outer_loop, face_id, &mut edge_faces);
            for &inner_loop in &face.inner_loops {
                collect_loop_edges(store, inner_loop, face_id, &mut edge_faces);
            }
        }
    }

    edge_faces
}

fn collect_loop_edges(
    store: &EntityStore,
    loop_id: LoopId,
    face_id: FaceId,
    edge_faces: &mut HashMap<u64, Vec<FaceId>>,
) {
    use slotmap::Key;
    let loop_data = &store.loops[loop_id];
    for &he_id in &loop_data.half_edges {
        let he = &store.half_edges[he_id];
        let edge_key = he.edge.data().as_ffi();
        let faces = edge_faces.entry(edge_key).or_default();
        if !faces.contains(&face_id) {
            faces.push(face_id);
        }
    }
}

/// Detect free edges (boundary edges shared by only one face).
fn check_free_edges(
    store: &EntityStore,
    solid_id: SolidId,
    errors: &mut Vec<ValidationError>,
) {
    let edge_faces = build_edge_face_map(store, solid_id);

    for (&_edge_key, faces) in &edge_faces {
        if faces.len() == 1 {
            // Find the EdgeId from any half-edge referencing this edge.
            if let Some(edge_id) = find_edge_id_by_key(store, solid_id, _edge_key) {
                errors.push(ValidationError {
                    entity_type: EntityType::Edge,
                    entity_id: EntityId::Edge(edge_id),
                    parent_id: Some(EntityId::Face(faces[0])),
                    code: ErrorCode::FreeEdge,
                    message: "Edge is referenced by only one face (open boundary)".into(),
                    severity: Severity::Error,
                    numeric_value: None,
                    tolerance: None,
                });
            }
        }
    }
}

/// Detect non-manifold edges (shared by more than two faces).
fn check_non_manifold_edges(
    store: &EntityStore,
    solid_id: SolidId,
    errors: &mut Vec<ValidationError>,
) {
    let edge_faces = build_edge_face_map(store, solid_id);

    for (&_edge_key, faces) in &edge_faces {
        if faces.len() > 2 {
            if let Some(edge_id) = find_edge_id_by_key(store, solid_id, _edge_key) {
                errors.push(ValidationError {
                    entity_type: EntityType::Edge,
                    entity_id: EntityId::Edge(edge_id),
                    parent_id: None,
                    code: ErrorCode::InvalidMultiConnexity,
                    message: format!("Edge shared by {} faces (expected 2)", faces.len()),
                    severity: Severity::Error,
                    numeric_value: Some(faces.len() as f64),
                    tolerance: Some(2.0),
                });
            }
        }
    }
}

/// Find an EdgeId given its u64 key representation.
fn find_edge_id_by_key(store: &EntityStore, solid_id: SolidId, edge_key: u64) -> Option<EdgeId> {
    use slotmap::Key;
    let solid = &store.solids[solid_id];
    for &shell_id in &solid.shells {
        let shell = &store.shells[shell_id];
        for &face_id in &shell.faces {
            let face = &store.faces[face_id];
            let loop_data = &store.loops[face.outer_loop];
            for &he_id in &loop_data.half_edges {
                let he = &store.half_edges[he_id];
                if he.edge.data().as_ffi() == edge_key {
                    return Some(he.edge);
                }
            }
        }
    }
    None
}

/// Triangle-based self-intersection detection.
///
/// Tessellates the solid and checks for triangle-triangle intersections
/// between non-adjacent triangles using sweep-and-prune broad phase
/// + Moller-Trumbore narrow phase.
fn check_self_intersection(
    store: &EntityStore,
    solid_id: SolidId,
    errors: &mut Vec<ValidationError>,
) {
    // Collect all triangles from the solid's face polygons (outer + inner loops).
    let solid = &store.solids[solid_id];
    let mut triangles: Vec<([Point3d; 3], FaceId)> = Vec::new();

    for &shell_id in &solid.shells {
        let shell = &store.shells[shell_id];
        for &face_id in &shell.faces {
            let face = &store.faces[face_id];
            let all_loops = std::iter::once(face.outer_loop)
                .chain(face.inner_loops.iter().copied());
            for loop_id in all_loops {
                let loop_data = &store.loops[loop_id];
                let verts: Vec<Point3d> = loop_data.half_edges.iter()
                    .map(|&he_id| store.vertices[store.half_edges[he_id].start_vertex].point)
                    .collect();

                // Fan triangulation from vertex 0.
                if verts.len() >= 3 {
                    for i in 1..verts.len() - 1 {
                        triangles.push(([verts[0], verts[i], verts[i + 1]], face_id));
                    }
                }
            }
        }
    }

    if triangles.len() < 2 {
        return;
    }

    // Compute AABBs for each triangle.
    struct TriAABB {
        min_x: f64, max_x: f64,
        min_y: f64, max_y: f64,
        min_z: f64, max_z: f64,
    }

    let aabbs: Vec<TriAABB> = triangles.iter().map(|(tri, _)| {
        TriAABB {
            min_x: tri[0].x.min(tri[1].x).min(tri[2].x),
            max_x: tri[0].x.max(tri[1].x).max(tri[2].x),
            min_y: tri[0].y.min(tri[1].y).min(tri[2].y),
            max_y: tri[0].y.max(tri[1].y).max(tri[2].y),
            min_z: tri[0].z.min(tri[1].z).min(tri[2].z),
            max_z: tri[0].z.max(tri[1].z).max(tri[2].z),
        }
    }).collect();

    // Sort by min_x for sweep-and-prune.
    let mut indices: Vec<usize> = (0..triangles.len()).collect();
    indices.sort_by(|&a, &b| aabbs[a].min_x.partial_cmp(&aabbs[b].min_x).unwrap());

    // Sweep-and-prune + narrow phase.
    let eps = 1e-10;
    for ii in 0..indices.len() {
        let i = indices[ii];
        for jj in (ii + 1)..indices.len() {
            let j = indices[jj];
            // If min_x of j > max_x of i, no more overlaps with i.
            if aabbs[j].min_x > aabbs[i].max_x + eps {
                break;
            }
            // Check Y and Z overlap.
            if aabbs[i].max_y + eps < aabbs[j].min_y || aabbs[j].max_y + eps < aabbs[i].min_y {
                continue;
            }
            if aabbs[i].max_z + eps < aabbs[j].min_z || aabbs[j].max_z + eps < aabbs[i].min_z {
                continue;
            }

            // Skip adjacent triangles (share a vertex).
            let (tri_a, face_a) = &triangles[i];
            let (tri_b, face_b) = &triangles[j];
            if shares_vertex(tri_a, tri_b, eps) {
                continue;
            }

            // Moller-Trumbore triangle-triangle intersection test.
            if triangles_intersect(tri_a, tri_b) {
                errors.push(ValidationError {
                    entity_type: EntityType::Face,
                    entity_id: EntityId::Face(*face_a),
                    parent_id: Some(EntityId::Face(*face_b)),
                    code: ErrorCode::SelfIntersection,
                    message: format!("Triangle self-intersection detected between faces"),
                    severity: Severity::Error,
                    numeric_value: None,
                    tolerance: None,
                });
                return; // Report only the first intersection.
            }
        }
    }
}

fn shares_vertex(a: &[Point3d; 3], b: &[Point3d; 3], eps: f64) -> bool {
    for va in a {
        for vb in b {
            if va.distance_to(vb) < eps {
                return true;
            }
        }
    }
    false
}

/// Simplified triangle-triangle intersection test.
/// Tests if any edge of triangle A intersects triangle B, and vice versa.
fn triangles_intersect(a: &[Point3d; 3], b: &[Point3d; 3]) -> bool {
    edge_intersects_triangle(a[0], a[1], b)
        || edge_intersects_triangle(a[1], a[2], b)
        || edge_intersects_triangle(a[2], a[0], b)
        || edge_intersects_triangle(b[0], b[1], a)
        || edge_intersects_triangle(b[1], b[2], a)
        || edge_intersects_triangle(b[2], b[0], a)
}

/// Moller-Trumbore ray-triangle intersection, clamped to segment [0,1].
fn edge_intersects_triangle(p0: Point3d, p1: Point3d, tri: &[Point3d; 3]) -> bool {
    let dir = p1 - p0;
    let e1 = tri[1] - tri[0];
    let e2 = tri[2] - tri[0];
    let h = dir.cross(&e2);
    let det = e1.dot(&h);

    if det.abs() < 1e-14 {
        return false; // Parallel
    }

    let inv_det = 1.0 / det;
    let s = p0 - tri[0];
    let u = inv_det * s.dot(&h);
    if u < 0.0 || u > 1.0 {
        return false;
    }

    let q = s.cross(&e1);
    let v = inv_det * dir.dot(&q);
    if v < 0.0 || u + v > 1.0 {
        return false;
    }

    let t = inv_det * e2.dot(&q);
    // Intersection at t in [0, 1] means within the edge segment.
    // Use small epsilon to avoid false positives at exact shared edges.
    t > 1e-8 && t < 1.0 - 1e-8
}

use crate::geometry::point::Point3d;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::primitives::{make_box, make_cylinder};
    use crate::geometry::point::Point3d;

    #[test]
    fn test_box_no_free_edges() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let config = ValidationConfig::spatial();

        check_spatial(&store, solid_id, &config, &mut errors, &mut warnings);

        let free = errors.iter().filter(|e| e.code == ErrorCode::FreeEdge).count();
        assert_eq!(free, 0, "Box should have no free edges");
    }

    #[test]
    fn test_box_no_non_manifold_edges() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let config = ValidationConfig::spatial();

        check_spatial(&store, solid_id, &config, &mut errors, &mut warnings);

        let nm = errors.iter().filter(|e| e.code == ErrorCode::InvalidMultiConnexity).count();
        assert_eq!(nm, 0, "Box should have no non-manifold edges");
    }

    #[test]
    fn test_removed_face_creates_free_edges() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        // Remove a face from the shell.
        let shell_id = store.solids[solid_id].shells[0];
        store.shells[shell_id].faces.pop();

        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut config = ValidationConfig::spatial();
        config.check_self_intersection = false; // Skip expensive check

        check_spatial(&store, solid_id, &config, &mut errors, &mut warnings);

        let free = errors.iter().filter(|e| e.code == ErrorCode::FreeEdge).count();
        assert!(free > 0, "Removing a face should create free edges");
    }

    #[test]
    fn test_box_no_self_intersection() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let config = ValidationConfig::spatial();

        check_spatial(&store, solid_id, &config, &mut errors, &mut warnings);

        let si = errors.iter().filter(|e| e.code == ErrorCode::SelfIntersection).count();
        assert_eq!(si, 0, "Box should have no self-intersection");
    }

    #[test]
    fn test_cylinder_no_free_edges() {
        let mut store = EntityStore::new();
        let solid_id = make_cylinder(&mut store, Point3d::ORIGIN, 1.0, 2.0, 16);

        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut config = ValidationConfig::spatial();
        config.check_self_intersection = false;

        check_spatial(&store, solid_id, &config, &mut errors, &mut warnings);

        let free = errors.iter().filter(|e| e.code == ErrorCode::FreeEdge).count();
        assert_eq!(free, 0, "Cylinder should have no free edges");
    }
}
