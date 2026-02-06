use super::brep::*;
use crate::geometry::curves::{Curve, Line3d};
use crate::geometry::point::Point3d;
use crate::geometry::surfaces::{Plane, Surface};
use crate::geometry::vector::Vec3;

/// Build a box solid directly from corner coordinates.
/// The box is axis-aligned with one corner at (x0,y0,z0) and opposite at (x1,y1,z1).
pub fn make_box(store: &mut EntityStore, x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) -> SolidId {
    // 8 vertices of the box
    let v = [
        Point3d::new(x0, y0, z0), // 0: front-bottom-left
        Point3d::new(x1, y0, z0), // 1: front-bottom-right
        Point3d::new(x1, y1, z0), // 2: front-top-right
        Point3d::new(x0, y1, z0), // 3: front-top-left
        Point3d::new(x0, y0, z1), // 4: back-bottom-left
        Point3d::new(x1, y0, z1), // 5: back-bottom-right
        Point3d::new(x1, y1, z1), // 6: back-top-right
        Point3d::new(x0, y1, z1), // 7: back-top-left
    ];

    let vertex_ids: Vec<VertexId> = v
        .iter()
        .map(|p| store.vertices.insert(Vertex { point: *p, tolerance: 1e-7 }))
        .collect();

    let solid_id = store.solids.insert(Solid { shells: vec![] });
    let shell_id = store.shells.insert(Shell {
        faces: vec![],
        orientation: ShellOrientation::Outward,
        solid: solid_id,
    });
    store.solids[solid_id].shells.push(shell_id);

    // Define 6 faces with their vertex indices (outward normals via CCW winding)
    // Each face: [v0, v1, v2, v3], surface normal
    let face_defs: [(usize, usize, usize, usize, Vec3); 6] = [
        (0, 1, 2, 3, -Vec3::Z), // front (z = z0)
        (5, 4, 7, 6, Vec3::Z),  // back  (z = z1)
        (0, 3, 7, 4, -Vec3::X), // left  (x = x0)
        (1, 5, 6, 2, Vec3::X),  // right (x = x1)
        (0, 4, 5, 1, -Vec3::Y), // bottom (y = y0)
        (3, 2, 6, 7, Vec3::Y),  // top    (y = y1)
    ];

    let mut all_edge_he_ids: std::collections::HashMap<(usize, usize), HalfEdgeId> =
        std::collections::HashMap::new();

    for &(vi0, vi1, vi2, vi3, normal) in &face_defs {
        let face_verts = [vi0, vi1, vi2, vi3];
        let center = Point3d::new(
            (v[vi0].x + v[vi2].x) / 2.0,
            (v[vi0].y + v[vi2].y) / 2.0,
            (v[vi0].z + v[vi2].z) / 2.0,
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

        // Create edges and half-edges for this face
        for edge_idx in 0..4 {
            let v_start_idx = face_verts[edge_idx];
            let v_end_idx = face_verts[(edge_idx + 1) % 4];
            let v_start = vertex_ids[v_start_idx];
            let v_end = vertex_ids[v_end_idx];

            let edge_key = if v_start_idx < v_end_idx {
                (v_start_idx, v_end_idx)
            } else {
                (v_end_idx, v_start_idx)
            };

            let forward = v_start_idx < v_end_idx;

            let he_id = store.half_edges.insert_with_key(|_| HalfEdge {
                edge: EdgeId::default(),
                twin: HalfEdgeId::default(),
                face: face_id,
                loop_id,
                start_vertex: v_start,
                end_vertex: v_end,
                t_start: 0.0,
                t_end: v[v_start_idx].distance_to(&v[v_end_idx]),
                forward,
            });

            store.loops[loop_id].half_edges.push(he_id);

            if let Some(&twin_he_id) = all_edge_he_ids.get(&edge_key) {
                // This edge already exists — link twins
                let edge_id = store.half_edges[twin_he_id].edge;
                store.half_edges[he_id].twin = twin_he_id;
                store.half_edges[he_id].edge = edge_id;
                store.half_edges[twin_he_id].twin = he_id;
                store.edges[edge_id].half_edges.1 = he_id;
            } else {
                // New edge
                let p_start = v[v_start_idx];
                let p_end = v[v_end_idx];
                let (e_start, e_end) = if forward {
                    (v_start, v_end)
                } else {
                    (v_end, v_start)
                };
                let line = if forward {
                    Line3d::from_points(p_start, p_end)
                } else {
                    Line3d::from_points(p_end, p_start)
                };

                let edge_id = store.edges.insert(Edge {
                    curve: Curve::Line(line),
                    half_edges: (he_id, HalfEdgeId::default()),
                    start_vertex: e_start,
                    end_vertex: e_end,
                });

                store.half_edges[he_id].edge = edge_id;
                all_edge_he_ids.insert(edge_key, he_id);
            }
        }
    }

    solid_id
}

/// Build a cylinder solid along the Z axis.
pub fn make_cylinder(store: &mut EntityStore, center: Point3d, radius: f64, height: f64, num_segments: usize) -> SolidId {
    let solid_id = store.solids.insert(Solid { shells: vec![] });
    let shell_id = store.shells.insert(Shell {
        faces: vec![],
        orientation: ShellOrientation::Outward,
        solid: solid_id,
    });
    store.solids[solid_id].shells.push(shell_id);

    // Create vertices around bottom and top circles
    let mut bottom_verts = Vec::new();
    let mut top_verts = Vec::new();

    for i in 0..num_segments {
        let angle = 2.0 * std::f64::consts::PI * (i as f64) / (num_segments as f64);
        let x = center.x + radius * angle.cos();
        let y = center.y + radius * angle.sin();

        bottom_verts.push(store.vertices.insert(Vertex {
            point: Point3d::new(x, y, center.z),
            tolerance: 1e-7,
        }));
        top_verts.push(store.vertices.insert(Vertex {
            point: Point3d::new(x, y, center.z + height),
            tolerance: 1e-7,
        }));
    }

    // Bottom face (normal pointing -Z)
    {
        let surface = Surface::Plane(Plane::new(center, -Vec3::Z));
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

        // Create edges around bottom circle (reversed winding for -Z normal)
        for i in 0..num_segments {
            let next = (i + 1) % num_segments;
            create_face_edge(store, bottom_verts[next], bottom_verts[i], face_id, loop_id);
        }
    }

    // Top face (normal pointing +Z)
    {
        let top_center = Point3d::new(center.x, center.y, center.z + height);
        let surface = Surface::Plane(Plane::new(top_center, Vec3::Z));
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

        for i in 0..num_segments {
            let next = (i + 1) % num_segments;
            create_face_edge(store, top_verts[i], top_verts[next], face_id, loop_id);
        }
    }

    // Side faces (quads)
    for i in 0..num_segments {
        let next = (i + 1) % num_segments;
        let angle_mid = 2.0 * std::f64::consts::PI * ((i as f64 + 0.5) / num_segments as f64);
        let normal_dir = Vec3::new(angle_mid.cos(), angle_mid.sin(), 0.0);

        let surface = Surface::Plane(Plane::new(
            store.vertices[bottom_verts[i]].point.midpoint(&store.vertices[top_verts[next]].point),
            normal_dir,
        ));

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

        // Bottom edge, right edge, top edge (reversed), left edge (reversed)
        create_face_edge(store, bottom_verts[i], bottom_verts[next], face_id, loop_id);
        create_face_edge(store, bottom_verts[next], top_verts[next], face_id, loop_id);
        create_face_edge(store, top_verts[next], top_verts[i], face_id, loop_id);
        create_face_edge(store, top_verts[i], bottom_verts[i], face_id, loop_id);
    }

    solid_id
}

fn create_face_edge(
    store: &mut EntityStore,
    v_start: VertexId,
    v_end: VertexId,
    face_id: FaceId,
    loop_id: LoopId,
) -> HalfEdgeId {
    let p_start = store.vertices[v_start].point;
    let p_end = store.vertices[v_end].point;

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

    let line = Line3d::from_points(p_start, p_end);
    let edge_id = store.edges.insert(Edge {
        curve: Curve::Line(line),
        half_edges: (he_id, he_id), // twin will be linked later in a full implementation
        start_vertex: v_start,
        end_vertex: v_end,
    });

    store.half_edges[he_id].edge = edge_id;
    store.loops[loop_id].half_edges.push(he_id);

    he_id
}

/// Build a sphere solid (tessellated).
pub fn make_sphere(store: &mut EntityStore, center: Point3d, radius: f64, num_meridians: usize, num_parallels: usize) -> SolidId {
    let solid_id = store.solids.insert(Solid { shells: vec![] });
    let shell_id = store.shells.insert(Shell {
        faces: vec![],
        orientation: ShellOrientation::Outward,
        solid: solid_id,
    });
    store.solids[solid_id].shells.push(shell_id);

    // Create vertices
    let north = store.vertices.insert(Vertex {
        point: Point3d::new(center.x, center.y, center.z + radius),
        tolerance: 1e-7,
    });
    let south = store.vertices.insert(Vertex {
        point: Point3d::new(center.x, center.y, center.z - radius),
        tolerance: 1e-7,
    });

    let mut ring_verts: Vec<Vec<VertexId>> = Vec::new();

    for j in 1..num_parallels {
        let phi = std::f64::consts::PI * (j as f64 / num_parallels as f64);
        let mut ring = Vec::new();
        for i in 0..num_meridians {
            let theta = 2.0 * std::f64::consts::PI * (i as f64 / num_meridians as f64);
            let x = center.x + radius * phi.sin() * theta.cos();
            let y = center.y + radius * phi.sin() * theta.sin();
            let z = center.z + radius * phi.cos();
            ring.push(store.vertices.insert(Vertex {
                point: Point3d::new(x, y, z),
                tolerance: 1e-7,
            }));
        }
        ring_verts.push(ring);
    }

    // North cap triangles
    for i in 0..num_meridians {
        let next = (i + 1) % num_meridians;
        let normal = {
            let p0 = store.vertices[north].point;
            let p1 = store.vertices[ring_verts[0][i]].point;
            let p2 = store.vertices[ring_verts[0][next]].point;
            let mid = Point3d::new(
                (p0.x + p1.x + p2.x) / 3.0,
                (p0.y + p1.y + p2.y) / 3.0,
                (p0.z + p1.z + p2.z) / 3.0,
            );
            (mid - center).normalize()
        };

        let surface = Surface::Plane(Plane::new(store.vertices[north].point, normal));
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

        create_face_edge(store, north, ring_verts[0][i], face_id, loop_id);
        create_face_edge(store, ring_verts[0][i], ring_verts[0][next], face_id, loop_id);
        create_face_edge(store, ring_verts[0][next], north, face_id, loop_id);
    }

    // Middle quad strips
    for j in 0..(num_parallels - 2) {
        for i in 0..num_meridians {
            let next = (i + 1) % num_meridians;
            let normal = {
                let p0 = store.vertices[ring_verts[j][i]].point;
                let p2 = store.vertices[ring_verts[j + 1][next]].point;
                let mid = p0.midpoint(&p2);
                (mid - center).normalize()
            };

            let surface = Surface::Plane(Plane::new(
                store.vertices[ring_verts[j][i]].point,
                normal,
            ));
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

            create_face_edge(store, ring_verts[j][i], ring_verts[j][next], face_id, loop_id);
            create_face_edge(store, ring_verts[j][next], ring_verts[j + 1][next], face_id, loop_id);
            create_face_edge(store, ring_verts[j + 1][next], ring_verts[j + 1][i], face_id, loop_id);
            create_face_edge(store, ring_verts[j + 1][i], ring_verts[j][i], face_id, loop_id);
        }
    }

    // South cap triangles
    let last_ring = ring_verts.len() - 1;
    for i in 0..num_meridians {
        let next = (i + 1) % num_meridians;
        let normal = {
            let p0 = store.vertices[ring_verts[last_ring][i]].point;
            let p1 = store.vertices[south].point;
            let p2 = store.vertices[ring_verts[last_ring][next]].point;
            let mid = Point3d::new(
                (p0.x + p1.x + p2.x) / 3.0,
                (p0.y + p1.y + p2.y) / 3.0,
                (p0.z + p1.z + p2.z) / 3.0,
            );
            (mid - center).normalize()
        };

        let surface = Surface::Plane(Plane::new(store.vertices[south].point, normal));
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

        create_face_edge(store, ring_verts[last_ring][i], south, face_id, loop_id);
        create_face_edge(store, south, ring_verts[last_ring][next], face_id, loop_id);
        create_face_edge(store, ring_verts[last_ring][next], ring_verts[last_ring][i], face_id, loop_id);
    }

    solid_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_box_creates_correct_topology() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);

        let solid = &store.solids[solid_id];
        assert_eq!(solid.shells.len(), 1);

        let shell = &store.shells[solid.shells[0]];
        assert_eq!(shell.faces.len(), 6, "Box should have 6 faces");

        let (v, e, f) = store.count_topology(solid.shells[0]);
        assert_eq!(v, 8, "Box should have 8 vertices");
        assert_eq!(f, 6, "Box should have 6 faces");
        assert_eq!(e, 12, "Box should have 12 edges");

        // Euler formula: V - E + F = 2
        assert_eq!(v as i64 - e as i64 + f as i64, 2, "Euler formula violated");
    }

    #[test]
    fn test_make_box_vertices_at_correct_positions() {
        let mut store = EntityStore::new();
        let _solid_id = make_box(&mut store, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0);

        // All vertices should be at corners of the box
        for (_id, v) in &store.vertices {
            assert!(
                (v.point.x.abs() - 1.0).abs() < 1e-12
                    && (v.point.y.abs() - 1.0).abs() < 1e-12
                    && (v.point.z.abs() - 1.0).abs() < 1e-12,
                "Vertex at unexpected position: {:?}",
                v.point
            );
        }
    }

    #[test]
    fn test_make_box_bounding_box() {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 20.0, 30.0);

        let bb = store.solid_bounding_box(solid_id);
        assert!((bb.min.x - 0.0).abs() < 1e-10);
        assert!((bb.min.y - 0.0).abs() < 1e-10);
        assert!((bb.min.z - 0.0).abs() < 1e-10);
        assert!((bb.max.x - 10.0).abs() < 1e-10);
        assert!((bb.max.y - 20.0).abs() < 1e-10);
        assert!((bb.max.z - 30.0).abs() < 1e-10);
    }

    #[test]
    fn test_make_cylinder() {
        let mut store = EntityStore::new();
        let solid_id = make_cylinder(&mut store, Point3d::ORIGIN, 5.0, 10.0, 16);

        let solid = &store.solids[solid_id];
        assert_eq!(solid.shells.len(), 1);

        let shell = &store.shells[solid.shells[0]];
        // 2 caps + 16 sides = 18 faces
        assert_eq!(shell.faces.len(), 18);
    }

    #[test]
    fn test_make_sphere() {
        let mut store = EntityStore::new();
        let solid_id = make_sphere(&mut store, Point3d::ORIGIN, 5.0, 8, 6);

        let solid = &store.solids[solid_id];
        assert_eq!(solid.shells.len(), 1);

        let shell = &store.shells[solid.shells[0]];
        // 8 north cap triangles + 4*8 middle quads + 8 south cap triangles = 48 faces
        let expected_faces = 8 + (6 - 2) * 8 + 8;
        assert_eq!(shell.faces.len(), expected_faces);
    }
}
