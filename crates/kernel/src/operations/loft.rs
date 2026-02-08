use std::collections::HashMap;

use crate::geometry::point::Point3d;
use crate::geometry::surfaces::{Plane, Surface};
use crate::geometry::vector::Vec3;
use crate::operations::OperationError;
use crate::topology::brep::*;
use crate::topology::primitives::create_face_edge_twinned;

/// Loft between two profiles to create a solid by connecting corresponding
/// vertices with side faces.
///
/// Both profiles must have the same number of vertices (>= 3). The bottom
/// profile is used as-is (reversed winding for the inward-facing cap), and
/// the top profile forms the outward-facing cap. Side quads connect
/// corresponding edges of the two profiles.
pub fn loft_profiles(
    store: &mut EntityStore,
    bottom_profile: &[Point3d],
    top_profile: &[Point3d],
) -> Result<SolidId, OperationError> {
    let n = bottom_profile.len();

    if n < 3 {
        return Err(OperationError::InsufficientProfile {
            required: 3,
            provided: n,
        });
    }

    if top_profile.len() < 3 {
        return Err(OperationError::InsufficientProfile {
            required: 3,
            provided: top_profile.len(),
        });
    }

    if n != top_profile.len() {
        return Err(OperationError::ProfileMismatch {
            bottom_count: n,
            top_count: top_profile.len(),
        });
    }

    // Create vertices
    let bottom_verts: Vec<VertexId> = bottom_profile
        .iter()
        .map(|p| {
            store.vertices.insert(Vertex {
                point: *p,
                tolerance: crate::default_tolerance().coincidence,
            })
        })
        .collect();

    let top_verts: Vec<VertexId> = top_profile
        .iter()
        .map(|p| {
            store.vertices.insert(Vertex {
                point: *p,
                tolerance: crate::default_tolerance().coincidence,
            })
        })
        .collect();

    let solid_id = store.solids.insert(Solid { shells: vec![] });
    let shell_id = store.shells.insert(Shell {
        faces: vec![],
        orientation: ShellOrientation::Outward,
        solid: solid_id,
    });
    store.solids[solid_id].shells.push(shell_id);

    let mut edge_map: HashMap<(VertexId, VertexId), HalfEdgeId> = HashMap::new();

    // Bottom face (normal pointing away from top profile — reversed winding)
    {
        let center = compute_centroid(store, &bottom_verts);
        // Estimate outward normal for bottom face: from bottom center toward top center, negated
        let top_center = compute_centroid(store, &top_verts);
        let up = top_center - center;
        let bottom_normal = if up.length() > 1e-15 {
            -up.normalize()
        } else {
            // Fallback: use cross product of first two edges
            let p0 = store.vertices[bottom_verts[0]].point;
            let p1 = store.vertices[bottom_verts[1]].point;
            let p2 = store.vertices[bottom_verts[2]].point;
            let e1 = p1 - p0;
            let e2 = p2 - p0;
            -e1.cross(&e2).normalized().unwrap_or(Vec3::Z)
        };

        let surface = Surface::Plane(Plane::new(center, bottom_normal));
        let loop_id = store.loops.insert(Loop {
            half_edges: vec![],
            face: FaceId::default(),
        });
        let face_id = store.faces.insert(Face {
            surface,
            outer_loop: loop_id,
            inner_loops: vec![],
            same_sense: true,
            shell: shell_id,
        });
        store.loops[loop_id].face = face_id;
        store.shells[shell_id].faces.push(face_id);

        // Reversed winding for bottom face
        for i in 0..n {
            let from = (n - i) % n;
            let to = if from == 0 { n - 1 } else { from - 1 };
            create_face_edge_twinned(
                store,
                bottom_verts[from],
                bottom_verts[to],
                face_id,
                loop_id,
                &mut edge_map,
            );
        }
    }

    // Top face (normal pointing away from bottom profile — forward winding)
    {
        let center = compute_centroid(store, &top_verts);
        let bottom_center = compute_centroid(store, &bottom_verts);
        let up = center - bottom_center;
        let top_normal = if up.length() > 1e-15 {
            up.normalize()
        } else {
            let p0 = store.vertices[top_verts[0]].point;
            let p1 = store.vertices[top_verts[1]].point;
            let p2 = store.vertices[top_verts[2]].point;
            let e1 = p1 - p0;
            let e2 = p2 - p0;
            e1.cross(&e2).normalized().unwrap_or(Vec3::Z)
        };

        let surface = Surface::Plane(Plane::new(center, top_normal));
        let loop_id = store.loops.insert(Loop {
            half_edges: vec![],
            face: FaceId::default(),
        });
        let face_id = store.faces.insert(Face {
            surface,
            outer_loop: loop_id,
            inner_loops: vec![],
            same_sense: true,
            shell: shell_id,
        });
        store.loops[loop_id].face = face_id;
        store.shells[shell_id].faces.push(face_id);

        // Forward winding for top face
        for i in 0..n {
            let next = (i + 1) % n;
            create_face_edge_twinned(
                store,
                top_verts[i],
                top_verts[next],
                face_id,
                loop_id,
                &mut edge_map,
            );
        }
    }

    // Side faces: quads connecting bottom[i]->bottom[i+1]->top[i+1]->top[i]
    for i in 0..n {
        let next = (i + 1) % n;

        let v0 = bottom_verts[i];
        let v1 = bottom_verts[next];
        let v2 = top_verts[next];
        let v3 = top_verts[i];

        let p0 = store.vertices[v0].point;
        let p1 = store.vertices[v1].point;
        let p3 = store.vertices[v3].point;
        let edge1 = p1 - p0;
        let edge2 = p3 - p0;
        let normal = edge1.cross(&edge2).normalized().unwrap_or(Vec3::Z);

        let center = Point3d::new(
            (p0.x + p1.x + store.vertices[v2].point.x + p3.x) / 4.0,
            (p0.y + p1.y + store.vertices[v2].point.y + p3.y) / 4.0,
            (p0.z + p1.z + store.vertices[v2].point.z + p3.z) / 4.0,
        );
        let surface = Surface::Plane(Plane::new(center, normal));
        let loop_id = store.loops.insert(Loop {
            half_edges: vec![],
            face: FaceId::default(),
        });
        let face_id = store.faces.insert(Face {
            surface,
            outer_loop: loop_id,
            inner_loops: vec![],
            same_sense: true,
            shell: shell_id,
        });
        store.loops[loop_id].face = face_id;
        store.shells[shell_id].faces.push(face_id);

        // Quad winding: v0 -> v1 -> v2 -> v3
        create_face_edge_twinned(store, v0, v1, face_id, loop_id, &mut edge_map);
        create_face_edge_twinned(store, v1, v2, face_id, loop_id, &mut edge_map);
        create_face_edge_twinned(store, v2, v3, face_id, loop_id, &mut edge_map);
        create_face_edge_twinned(store, v3, v0, face_id, loop_id, &mut edge_map);
    }

    Ok(solid_id)
}

fn compute_centroid(store: &EntityStore, verts: &[VertexId]) -> Point3d {
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut cz = 0.0;
    for &v in verts {
        let p = store.vertices[v].point;
        cx += p.x;
        cy += p.y;
        cz += p.z;
    }
    let n = verts.len() as f64;
    Point3d::new(cx / n, cy / n, cz / n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::audit::{verify_topology_l0, verify_geometry_l1};

    fn assert_twin_invariants(store: &EntityStore, solid_id: SolidId) {
        let solid = &store.solids[solid_id];
        for &shell_id in &solid.shells {
            let shell = &store.shells[shell_id];
            for &face_id in &shell.faces {
                let face = &store.faces[face_id];
                let loop_data = &store.loops[face.outer_loop];
                for &he_id in &loop_data.half_edges {
                    let he = &store.half_edges[he_id];
                    assert_ne!(
                        he_id, he.twin,
                        "Half-edge has self-twin (twin points to itself)"
                    );
                    let twin = &store.half_edges[he.twin];
                    assert_eq!(
                        twin.twin, he_id,
                        "twin(twin(he)) != he -- twin symmetry violated"
                    );
                    assert_ne!(
                        he.face, twin.face,
                        "Twin half-edges belong to the same face"
                    );
                }
            }
        }
    }

    #[test]
    fn test_loft_identical_rectangles_produces_box() {
        let mut store = EntityStore::new();
        let bottom = vec![
            Point3d::new(-2.0, -2.0, 0.0),
            Point3d::new(2.0, -2.0, 0.0),
            Point3d::new(2.0, 2.0, 0.0),
            Point3d::new(-2.0, 2.0, 0.0),
        ];
        let top = vec![
            Point3d::new(-2.0, -2.0, 5.0),
            Point3d::new(2.0, -2.0, 5.0),
            Point3d::new(2.0, 2.0, 5.0),
            Point3d::new(-2.0, 2.0, 5.0),
        ];
        let solid_id = loft_profiles(&mut store, &bottom, &top).unwrap();

        // Should produce a box: 2 caps + 4 sides = 6 faces
        let shell = &store.shells[store.solids[solid_id].shells[0]];
        assert_eq!(shell.faces.len(), 6);

        let bb = store.solid_bounding_box(solid_id);
        assert!((bb.min.x - (-2.0)).abs() < 1e-10);
        assert!((bb.max.x - 2.0).abs() < 1e-10);
        assert!((bb.min.z - 0.0).abs() < 1e-10);
        assert!((bb.max.z - 5.0).abs() < 1e-10);

        let audit = verify_topology_l0(&store, solid_id);
        assert!(audit.all_faces_closed, "Loft box has open loops");
        assert!(audit.euler_valid, "Euler formula violated");

        assert_twin_invariants(&store, solid_id);
    }

    #[test]
    fn test_loft_frustum_topology() {
        let mut store = EntityStore::new();
        let bottom = vec![
            Point3d::new(-2.0, -2.0, 0.0),
            Point3d::new(2.0, -2.0, 0.0),
            Point3d::new(2.0, 2.0, 0.0),
            Point3d::new(-2.0, 2.0, 0.0),
        ];
        let top = vec![
            Point3d::new(-1.0, -1.0, 5.0),
            Point3d::new(1.0, -1.0, 5.0),
            Point3d::new(1.0, 1.0, 5.0),
            Point3d::new(-1.0, 1.0, 5.0),
        ];
        let solid_id = loft_profiles(&mut store, &bottom, &top).unwrap();

        let shell = &store.shells[store.solids[solid_id].shells[0]];
        assert_eq!(shell.faces.len(), 6);

        let audit = verify_topology_l0(&store, solid_id);
        assert!(audit.all_faces_closed, "Frustum has open loops");
        assert!(audit.euler_valid, "Frustum Euler formula violated");

        assert_twin_invariants(&store, solid_id);

        let geom_errors = verify_geometry_l1(&store, solid_id);
        assert!(geom_errors.is_empty(), "Geometry errors: {:?}", geom_errors);
    }

    #[test]
    fn test_loft_mismatched_profiles_returns_error() {
        let mut store = EntityStore::new();
        let bottom = vec![
            Point3d::new(0.0, 0.0, 0.0),
            Point3d::new(1.0, 0.0, 0.0),
            Point3d::new(1.0, 1.0, 0.0),
        ];
        let top = vec![
            Point3d::new(0.0, 0.0, 5.0),
            Point3d::new(1.0, 0.0, 5.0),
            Point3d::new(1.0, 1.0, 5.0),
            Point3d::new(0.0, 1.0, 5.0),
        ];
        let result = loft_profiles(&mut store, &bottom, &top);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            OperationError::ProfileMismatch { .. }
        ));
    }

    #[test]
    fn test_loft_insufficient_points_returns_error() {
        let mut store = EntityStore::new();
        let bottom = vec![
            Point3d::new(0.0, 0.0, 0.0),
            Point3d::new(1.0, 0.0, 0.0),
        ];
        let top = vec![
            Point3d::new(0.0, 0.0, 5.0),
            Point3d::new(1.0, 0.0, 5.0),
        ];
        let result = loft_profiles(&mut store, &bottom, &top);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            OperationError::InsufficientProfile { .. }
        ));
    }

    #[test]
    fn test_loft_triangles() {
        let mut store = EntityStore::new();
        let bottom = vec![
            Point3d::new(0.0, 0.0, 0.0),
            Point3d::new(4.0, 0.0, 0.0),
            Point3d::new(2.0, 3.0, 0.0),
        ];
        let top = vec![
            Point3d::new(0.0, 0.0, 6.0),
            Point3d::new(4.0, 0.0, 6.0),
            Point3d::new(2.0, 3.0, 6.0),
        ];
        let solid_id = loft_profiles(&mut store, &bottom, &top).unwrap();

        // 2 caps + 3 sides = 5 faces
        let shell = &store.shells[store.solids[solid_id].shells[0]];
        assert_eq!(shell.faces.len(), 5);

        let audit = verify_topology_l0(&store, solid_id);
        assert!(audit.all_faces_closed);
        assert!(audit.euler_valid);

        assert_twin_invariants(&store, solid_id);
    }
}
