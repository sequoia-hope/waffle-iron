use crate::geometry::curves::{Curve, Line3d};
use crate::geometry::point::Point3d;
use crate::geometry::surfaces::{Plane, Surface};
use crate::geometry::vector::Vec3;
use crate::topology::brep::*;

/// Chamfer a box solid by cutting one edge at a given distance.
///
/// The chamfer replaces the edge between two adjacent faces with a new flat face
/// (the bevel), trimming both original faces. This implementation works on
/// axis-aligned box edges specified by two vertex positions.
///
/// Returns the new solid (the original is not modified).
pub fn chamfer_edge(
    store: &mut EntityStore,
    solid_id: SolidId,
    edge_v0: Point3d,
    edge_v1: Point3d,
    distance: f64,
) -> SolidId {
    // Collect all face vertex-lists from the original solid
    let solid = &store.solids[solid_id];
    let shell_id = solid.shells[0];
    let shell = &store.shells[shell_id];

    let mut face_polygons: Vec<(Vec<Point3d>, Vec3)> = Vec::new();

    for &face_id in &shell.faces {
        let face = &store.faces[face_id];
        let normal = face.surface.normal_at(0.0, 0.0);
        let loop_data = &store.loops[face.outer_loop];
        let verts: Vec<Point3d> = loop_data
            .half_edges
            .iter()
            .map(|&he_id| store.vertices[store.half_edges[he_id].start_vertex].point)
            .collect();
        face_polygons.push((verts, normal));
    }

    // Find the two faces adjacent to the specified edge
    let tol = 1e-6;
    let mut adjacent_face_indices: Vec<usize> = Vec::new();

    for (fi, (verts, _)) in face_polygons.iter().enumerate() {
        let n = verts.len();
        for i in 0..n {
            let a = verts[i];
            let b = verts[(i + 1) % n];
            // Check if this edge matches (in either direction)
            let match_fwd = a.distance_to(&edge_v0) < tol && b.distance_to(&edge_v1) < tol;
            let match_rev = a.distance_to(&edge_v1) < tol && b.distance_to(&edge_v0) < tol;
            if match_fwd || match_rev {
                adjacent_face_indices.push(fi);
                break;
            }
        }
    }

    if adjacent_face_indices.len() != 2 {
        // Can't chamfer — return a clone of the original
        let shells = store.solids[solid_id].shells.clone();
        return store.solids.insert(Solid { shells });
    }

    let fi_a = adjacent_face_indices[0];
    let fi_b = adjacent_face_indices[1];

    let normal_a = face_polygons[fi_a].1;
    let normal_b = face_polygons[fi_b].1;

    // Compute chamfer cut points: move each edge endpoint along each face normal
    // to create the bevel vertices
    let chamfer_a0 = edge_v0 + normal_a * (-distance);
    let chamfer_a1 = edge_v1 + normal_a * (-distance);
    let chamfer_b0 = edge_v0 + normal_b * (-distance);
    let chamfer_b1 = edge_v1 + normal_b * (-distance);

    // Project chamfer points back onto the respective face planes
    // (for axis-aligned boxes, the chamfer points are already in-plane for the adjacent face)

    // Build the new solid with modified face polygons
    let new_solid_id = store.solids.insert(Solid { shells: vec![] });
    let new_shell_id = store.shells.insert(Shell {
        faces: vec![],
        orientation: ShellOrientation::Outward,
        solid: new_solid_id,
    });
    store.solids[new_solid_id].shells.push(new_shell_id);

    for (fi, (verts, normal)) in face_polygons.iter().enumerate() {
        if fi == fi_a {
            // Replace the edge vertices with chamfer vertices on face A
            let modified = replace_edge_verts(verts, &edge_v0, &edge_v1, &chamfer_b0, &chamfer_b1, tol);
            create_chamfer_face(store, new_shell_id, &modified, *normal);
        } else if fi == fi_b {
            // Replace the edge vertices with chamfer vertices on face B
            let modified = replace_edge_verts(verts, &edge_v0, &edge_v1, &chamfer_a0, &chamfer_a1, tol);
            create_chamfer_face(store, new_shell_id, &modified, *normal);
        } else {
            // Copy unchanged face, but replace edge_v0/v1 if they appear
            let modified: Vec<Point3d> = verts
                .iter()
                .flat_map(|v| {
                    if v.distance_to(&edge_v0) < tol {
                        vec![chamfer_a0, chamfer_b0]
                    } else if v.distance_to(&edge_v1) < tol {
                        // Need consistent ordering — check which comes first in the polygon
                        vec![chamfer_a1, chamfer_b1]
                    } else {
                        vec![*v]
                    }
                })
                .collect();
            create_chamfer_face(store, new_shell_id, &modified, *normal);
        }
    }

    // Add the chamfer bevel face
    let bevel_normal = (normal_a + normal_b).normalized().unwrap_or(Vec3::Z);
    let bevel_verts = vec![chamfer_a0, chamfer_a1, chamfer_b1, chamfer_b0];
    create_chamfer_face(store, new_shell_id, &bevel_verts, bevel_normal);

    new_solid_id
}

/// Replace occurrences of edge_v0 and edge_v1 in a polygon with new vertices.
fn replace_edge_verts(
    verts: &[Point3d],
    edge_v0: &Point3d,
    edge_v1: &Point3d,
    new_v0: &Point3d,
    new_v1: &Point3d,
    tol: f64,
) -> Vec<Point3d> {
    verts
        .iter()
        .map(|v| {
            if v.distance_to(edge_v0) < tol {
                *new_v0
            } else if v.distance_to(edge_v1) < tol {
                *new_v1
            } else {
                *v
            }
        })
        .collect()
}

fn create_chamfer_face(
    store: &mut EntityStore,
    shell_id: ShellId,
    verts: &[Point3d],
    normal: Vec3,
) {
    if verts.len() < 3 {
        return;
    }

    let vertex_ids: Vec<VertexId> = verts
        .iter()
        .map(|p| {
            store.vertices.insert(Vertex {
                point: *p,
                tolerance: 1e-7,
            })
        })
        .collect();

    let center = {
        let n = verts.len() as f64;
        let (sx, sy, sz) = verts.iter().fold((0.0, 0.0, 0.0), |(x, y, z), p| {
            (x + p.x, y + p.y, z + p.z)
        });
        Point3d::new(sx / n, sy / n, sz / n)
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

    for i in 0..vertex_ids.len() {
        let next = (i + 1) % vertex_ids.len();
        let v_start = vertex_ids[i];
        let v_end = vertex_ids[next];
        let p_start = verts[i];
        let p_end = verts[next];
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
    use crate::topology::primitives::make_box;

    #[test]
    fn test_chamfer_box_edge() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        // Chamfer the front-bottom edge (z=0, y=0, x: 0→10)
        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let result = chamfer_edge(&mut store, box_id, v0, v1, 2.0);

        let shell = &store.shells[store.solids[result].shells[0]];
        // Original 6 faces + 1 bevel face = 7
        assert_eq!(shell.faces.len(), 7, "Chamfered box should have 7 faces");
    }

    #[test]
    fn test_chamfer_creates_bevel() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let result = chamfer_edge(&mut store, box_id, v0, v1, 2.0);

        // The bounding box should still be 10x10x10
        let bb = store.solid_bounding_box(result);
        assert!((bb.max.x - 10.0).abs() < 1e-9);
        assert!((bb.max.y - 10.0).abs() < 1e-9);
        assert!((bb.max.z - 10.0).abs() < 1e-9);
    }

    #[test]
    fn test_chamfer_nonexistent_edge() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        // Edge that doesn't exist
        let v0 = Point3d::new(99.0, 99.0, 99.0);
        let v1 = Point3d::new(100.0, 99.0, 99.0);
        let result = chamfer_edge(&mut store, box_id, v0, v1, 2.0);

        // Should return a copy with same number of faces
        let shell = &store.shells[store.solids[result].shells[0]];
        assert_eq!(shell.faces.len(), 6, "Non-existent edge chamfer should return unchanged solid");
    }
}
