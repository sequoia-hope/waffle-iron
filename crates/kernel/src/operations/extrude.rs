use std::collections::HashMap;

use crate::geometry::point::Point3d;
use crate::geometry::surfaces::{Plane, Surface};
use crate::geometry::vector::Vec3;
use crate::operations::OperationError;
use crate::topology::brep::*;
use crate::topology::primitives::create_face_edge_twinned;

/// A 2D profile for extrusion (list of points forming a closed polygon).
#[derive(Debug, Clone)]
pub struct Profile {
    pub points: Vec<Point3d>,
}

impl Profile {
    /// Create a rectangular profile on the XY plane.
    pub fn rectangle(width: f64, height: f64) -> Self {
        let hw = width / 2.0;
        let hh = height / 2.0;
        Self {
            points: vec![
                Point3d::new(-hw, -hh, 0.0),
                Point3d::new(hw, -hh, 0.0),
                Point3d::new(hw, hh, 0.0),
                Point3d::new(-hw, hh, 0.0),
            ],
        }
    }

    /// Create a profile from arbitrary points.
    pub fn from_points(points: Vec<Point3d>) -> Self {
        Self { points }
    }
}

/// Extrude a profile along a direction to create a solid.
///
/// Returns `Err` if the profile has fewer than 3 points, the distance is not
/// positive, or the direction vector has zero length.
pub fn extrude_profile(
    store: &mut EntityStore,
    profile: &Profile,
    direction: Vec3,
    distance: f64,
) -> Result<SolidId, OperationError> {
    let n = profile.points.len();
    if n < 3 {
        return Err(OperationError::InsufficientProfile {
            required: 3,
            provided: n,
        });
    }
    if distance <= 0.0 {
        return Err(OperationError::InvalidDimension {
            parameter: "distance",
            value: distance,
        });
    }
    if direction.length() < 1e-15 {
        return Err(OperationError::ZeroDirection);
    }

    let extrusion = direction.normalize() * distance;

    // Create bottom and top vertices
    let bottom_verts: Vec<VertexId> = profile
        .points
        .iter()
        .map(|p| {
            store.vertices.insert(Vertex {
                point: *p,
                tolerance: crate::default_tolerance().coincidence,
            })
        })
        .collect();

    let top_verts: Vec<VertexId> = profile
        .points
        .iter()
        .map(|p| {
            store.vertices.insert(Vertex {
                point: *p + extrusion,
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

    // Shared edge map for twin linking across all faces
    let mut edge_map: HashMap<(VertexId, VertexId), HalfEdgeId> = HashMap::new();

    // Bottom face (normal opposite to extrusion direction)
    {
        let bottom_normal = -direction.normalize();
        let center = compute_centroid(store, &bottom_verts);
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

        // Bottom face winding: reversed for outward normal pointing opposite to extrusion
        for i in 0..n {
            let from = (n - i) % n;
            let to = if from == 0 { n - 1 } else { from - 1 };
            create_face_edge_twinned(store, bottom_verts[from], bottom_verts[to], face_id, loop_id, &mut edge_map);
        }
    }

    // Top face (normal in extrusion direction)
    {
        let top_normal = direction.normalize();
        let center = compute_centroid(store, &top_verts);
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

        // Top face winding: forward
        for i in 0..n {
            let next = (i + 1) % n;
            create_face_edge_twinned(store, top_verts[i], top_verts[next], face_id, loop_id, &mut edge_map);
        }
    }

    // Side faces
    for i in 0..n {
        let next = (i + 1) % n;

        let v0 = bottom_verts[i];
        let v1 = bottom_verts[next];
        let v2 = top_verts[next];
        let v3 = top_verts[i];

        // Compute outward normal for this side face
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
    use crate::operations::OperationError;
    use crate::validation::audit::{verify_topology_l0, verify_geometry_l1};

    /// Verify twin linking invariants for all half-edges in a solid.
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
                        "twin(twin(he)) != he — twin symmetry violated"
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
    fn test_extrude_rectangle() {
        let mut store = EntityStore::new();
        let profile = Profile::rectangle(10.0, 5.0);
        let solid_id = extrude_profile(&mut store, &profile, Vec3::Z, 20.0).unwrap();

        let solid = &store.solids[solid_id];
        assert_eq!(solid.shells.len(), 1);

        let shell = &store.shells[solid.shells[0]];
        // Rectangle extruded: 2 caps + 4 sides = 6 faces
        assert_eq!(shell.faces.len(), 6);

        // Topology audit
        let audit = verify_topology_l0(&store, solid_id);
        assert!(audit.all_faces_closed, "Extruded rectangle has open loops");

        // Twin linking
        assert_twin_invariants(&store, solid_id);
    }

    #[test]
    fn test_extrude_triangle() {
        let mut store = EntityStore::new();
        let profile = Profile::from_points(vec![
            Point3d::new(0.0, 0.0, 0.0),
            Point3d::new(10.0, 0.0, 0.0),
            Point3d::new(5.0, 8.66, 0.0),
        ]);
        let solid_id = extrude_profile(&mut store, &profile, Vec3::Z, 5.0).unwrap();

        let shell = &store.shells[store.solids[solid_id].shells[0]];
        // Triangle extruded: 2 caps + 3 sides = 5 faces
        assert_eq!(shell.faces.len(), 5);

        let audit = verify_topology_l0(&store, solid_id);
        assert!(audit.all_faces_closed, "Extruded triangle has open loops");

        assert_twin_invariants(&store, solid_id);
    }

    #[test]
    fn test_extrude_vertices_correct() {
        let mut store = EntityStore::new();
        let profile = Profile::rectangle(2.0, 2.0);
        let solid_id = extrude_profile(&mut store, &profile, Vec3::Z, 3.0).unwrap();

        let bb = store.solid_bounding_box(solid_id);
        assert!((bb.min.x - (-1.0)).abs() < 1e-10);
        assert!((bb.max.x - 1.0).abs() < 1e-10);
        assert!((bb.min.z - 0.0).abs() < 1e-10);
        assert!((bb.max.z - 3.0).abs() < 1e-10);

        let geom_errors = verify_geometry_l1(&store, solid_id);
        assert!(geom_errors.is_empty(), "Geometry errors: {:?}", geom_errors);

        assert_twin_invariants(&store, solid_id);
    }

    #[test]
    fn test_extrude_non_z_direction() {
        let mut store = EntityStore::new();
        let profile = Profile::rectangle(4.0, 4.0);
        let solid_id = extrude_profile(&mut store, &profile, Vec3::X, 10.0).unwrap();

        let bb = store.solid_bounding_box(solid_id);
        // Rectangle centered at origin: X range [-2, 2], extruded 10 in +X -> max X = 12
        assert!((bb.max.x - 12.0).abs() < 1e-10, "Should extend to 12 in X (2 + 10)");

        let audit = verify_topology_l0(&store, solid_id);
        assert!(audit.all_faces_closed, "X-direction extrusion has open loops");

        assert_twin_invariants(&store, solid_id);
    }

    #[test]
    fn test_extrude_pentagon() {
        let mut store = EntityStore::new();
        let mut pts = Vec::new();
        for i in 0..5 {
            let angle = 2.0 * std::f64::consts::PI * (i as f64) / 5.0;
            pts.push(Point3d::new(5.0 * angle.cos(), 5.0 * angle.sin(), 0.0));
        }
        let profile = Profile::from_points(pts);
        let solid_id = extrude_profile(&mut store, &profile, Vec3::Z, 8.0).unwrap();

        let shell = &store.shells[store.solids[solid_id].shells[0]];
        // Pentagon: 2 caps + 5 sides = 7 faces
        assert_eq!(shell.faces.len(), 7);

        let audit = verify_topology_l0(&store, solid_id);
        assert!(audit.all_faces_closed, "Extruded pentagon has open loops");

        assert_twin_invariants(&store, solid_id);
    }

    #[test]
    fn test_extrude_zero_height_returns_error() {
        let mut store = EntityStore::new();
        let profile = Profile::rectangle(10.0, 5.0);
        let result = extrude_profile(&mut store, &profile, Vec3::Z, 0.0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            OperationError::InvalidDimension { parameter: "distance", .. }
        ));
    }

    #[test]
    fn test_extrude_negative_height_returns_error() {
        let mut store = EntityStore::new();
        let profile = Profile::rectangle(10.0, 5.0);
        let result = extrude_profile(&mut store, &profile, Vec3::Z, -5.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_extrude_insufficient_profile_returns_error() {
        let mut store = EntityStore::new();
        let profile = Profile::from_points(vec![
            Point3d::new(0.0, 0.0, 0.0),
            Point3d::new(1.0, 0.0, 0.0),
        ]);
        let result = extrude_profile(&mut store, &profile, Vec3::Z, 5.0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            OperationError::InsufficientProfile { required: 3, provided: 2 }
        ));
    }

    #[test]
    fn test_extrude_zero_direction_returns_error() {
        let mut store = EntityStore::new();
        let profile = Profile::rectangle(10.0, 5.0);
        let result = extrude_profile(
            &mut store,
            &profile,
            Vec3::new(0.0, 0.0, 0.0),
            5.0,
        );
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OperationError::ZeroDirection));
    }
}
