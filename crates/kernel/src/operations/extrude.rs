use crate::geometry::curves::{Curve, Line3d};
use crate::geometry::point::Point3d;
use crate::geometry::surfaces::{Plane, Surface};
use crate::geometry::vector::Vec3;
use crate::topology::brep::*;

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
pub fn extrude_profile(
    store: &mut EntityStore,
    profile: &Profile,
    direction: Vec3,
    distance: f64,
) -> SolidId {
    let n = profile.points.len();
    assert!(n >= 3, "Profile must have at least 3 points");

    let extrusion = direction.normalize() * distance;

    // Create bottom and top vertices
    let bottom_verts: Vec<VertexId> = profile
        .points
        .iter()
        .map(|p| {
            store.vertices.insert(Vertex {
                point: *p,
                tolerance: 1e-7,
            })
        })
        .collect();

    let top_verts: Vec<VertexId> = profile
        .points
        .iter()
        .map(|p| {
            store.vertices.insert(Vertex {
                point: *p + extrusion,
                tolerance: 1e-7,
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

    // Bottom face (normal opposite to extrusion direction)
    let bottom_normal = -direction.normalize();
    create_polygon_face(store, &bottom_verts, shell_id, bottom_normal, true);

    // Top face (normal in extrusion direction)
    let top_normal = direction.normalize();
    // Top face vertices need reversed winding for outward normal
    let top_verts_reversed: Vec<VertexId> = top_verts.iter().rev().copied().collect();
    create_polygon_face(store, &top_verts_reversed, shell_id, top_normal, false);

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

        let side_verts = vec![v0, v1, v2, v3];
        create_polygon_face(store, &side_verts, shell_id, normal, true);
    }

    solid_id
}

fn create_polygon_face(
    store: &mut EntityStore,
    verts: &[VertexId],
    shell_id: ShellId,
    normal: Vec3,
    _forward_winding: bool,
) {
    let center = {
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
    };

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

    for i in 0..verts.len() {
        let next = (i + 1) % verts.len();
        let v_start = verts[i];
        let v_end = verts[next];

        let p_start = store.vertices[v_start].point;
        let p_end = store.vertices[v_end].point;
        let line = Line3d::from_points(p_start, p_end);

        let he_id = store.half_edges.insert_with_key(|_| HalfEdge {
            edge: EdgeId::default(),
            twin: HalfEdgeId::default(),
            face: face_id,
            loop_id,
            start_vertex: v_start,
            end_vertex: v_end,
            t_start: 0.0,
            t_end: p_start.distance_to(&p_end),
            forward: true,
        });

        let edge_id = store.edges.insert(Edge {
            curve: Curve::Line(line),
            half_edges: (he_id, he_id),
            start_vertex: v_start,
            end_vertex: v_end,
        });

        store.half_edges[he_id].edge = edge_id;
        store.loops[loop_id].half_edges.push(he_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extrude_rectangle() {
        let mut store = EntityStore::new();
        let profile = Profile::rectangle(10.0, 5.0);
        let solid_id = extrude_profile(&mut store, &profile, Vec3::Z, 20.0);

        let solid = &store.solids[solid_id];
        assert_eq!(solid.shells.len(), 1);

        let shell = &store.shells[solid.shells[0]];
        // Rectangle extruded: 2 caps + 4 sides = 6 faces
        assert_eq!(shell.faces.len(), 6);
    }

    #[test]
    fn test_extrude_triangle() {
        let mut store = EntityStore::new();
        let profile = Profile::from_points(vec![
            Point3d::new(0.0, 0.0, 0.0),
            Point3d::new(10.0, 0.0, 0.0),
            Point3d::new(5.0, 8.66, 0.0),
        ]);
        let solid_id = extrude_profile(&mut store, &profile, Vec3::Z, 5.0);

        let shell = &store.shells[store.solids[solid_id].shells[0]];
        // Triangle extruded: 2 caps + 3 sides = 5 faces
        assert_eq!(shell.faces.len(), 5);
    }

    #[test]
    fn test_extrude_vertices_correct() {
        let mut store = EntityStore::new();
        let profile = Profile::rectangle(2.0, 2.0);
        let solid_id = extrude_profile(&mut store, &profile, Vec3::Z, 3.0);

        let bb = store.solid_bounding_box(solid_id);
        assert!((bb.min.x - (-1.0)).abs() < 1e-10);
        assert!((bb.max.x - 1.0).abs() < 1e-10);
        assert!((bb.min.z - 0.0).abs() < 1e-10);
        assert!((bb.max.z - 3.0).abs() < 1e-10);
    }
}
