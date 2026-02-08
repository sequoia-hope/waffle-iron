use std::collections::HashMap;

use crate::geometry::point::Point3d;
use crate::geometry::surfaces::{Plane, Surface};
use crate::geometry::vector::Vec3;
use crate::operations::OperationError;
use crate::topology::brep::*;
use crate::topology::primitives::create_face_edge_twinned;

/// Chamfer a box solid by cutting one edge at a given distance.
///
/// The chamfer replaces the edge between two adjacent faces with a new flat face
/// (the bevel), trimming both original faces. This implementation works on
/// axis-aligned box edges specified by two vertex positions.
///
/// Returns a new solid (the original is not modified), or an error if the
/// distance is not positive or the edge is not found.
pub fn chamfer_edge(
    store: &mut EntityStore,
    solid_id: SolidId,
    edge_v0: Point3d,
    edge_v1: Point3d,
    distance: f64,
) -> Result<SolidId, OperationError> {
    if distance <= 0.0 {
        return Err(OperationError::InvalidDimension {
            parameter: "distance",
            value: distance,
        });
    }

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
    let tol = crate::default_tolerance().coincidence;
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
        return Err(OperationError::EdgeNotFound);
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

    // Build the new solid with modified face polygons
    let new_solid_id = store.solids.insert(Solid { shells: vec![] });
    let new_shell_id = store.shells.insert(Shell {
        faces: vec![],
        orientation: ShellOrientation::Outward,
        solid: new_solid_id,
    });
    store.solids[new_solid_id].shells.push(new_shell_id);

    // Vertex dedup map: quantized position -> VertexId
    let mut vertex_map: HashMap<(i64, i64, i64), VertexId> = HashMap::new();
    // Edge twin linking map
    let mut edge_map: HashMap<(VertexId, VertexId), HalfEdgeId> = HashMap::new();

    // Collect all face point-lists for the new solid
    let mut new_face_polys: Vec<(Vec<Point3d>, Vec3)> = Vec::new();

    for (fi, (verts, normal)) in face_polygons.iter().enumerate() {
        if fi == fi_a {
            let modified = replace_edge_verts(verts, &edge_v0, &edge_v1, &chamfer_b0, &chamfer_b1, tol);
            new_face_polys.push((modified, *normal));
        } else if fi == fi_b {
            let modified = replace_edge_verts(verts, &edge_v0, &edge_v1, &chamfer_a0, &chamfer_a1, tol);
            new_face_polys.push((modified, *normal));
        } else {
            // Non-adjacent face: insert both chamfer vertices at each shared
            // endpoint, with ordering determined by the predecessor vertex to
            // ensure correct twin linking with adjacent and bevel faces.
            let n = verts.len();
            let modified: Vec<Point3d> = (0..n)
                .flat_map(|i| {
                    let v = &verts[i];
                    if v.distance_to(&edge_v0) < tol {
                        let pred = &verts[(i + n - 1) % n];
                        if pred.distance_to(&chamfer_a0) <= pred.distance_to(&chamfer_b0) {
                            vec![chamfer_a0, chamfer_b0]
                        } else {
                            vec![chamfer_b0, chamfer_a0]
                        }
                    } else if v.distance_to(&edge_v1) < tol {
                        let pred = &verts[(i + n - 1) % n];
                        if pred.distance_to(&chamfer_a1) <= pred.distance_to(&chamfer_b1) {
                            vec![chamfer_a1, chamfer_b1]
                        } else {
                            vec![chamfer_b1, chamfer_a1]
                        }
                    } else {
                        vec![*v]
                    }
                })
                .collect();
            new_face_polys.push((modified, *normal));
        }
    }

    // Add the chamfer bevel face
    let bevel_normal = (normal_a + normal_b).normalized().unwrap_or(Vec3::Z);
    let bevel_verts = vec![chamfer_a0, chamfer_a1, chamfer_b1, chamfer_b0];
    new_face_polys.push((bevel_verts, bevel_normal));

    // Create all faces with vertex dedup and twin linking
    for (points, normal) in &new_face_polys {
        if points.len() < 3 {
            continue;
        }

        let vertex_ids: Vec<VertexId> = points
            .iter()
            .map(|p| get_or_create_vertex(store, &mut vertex_map, *p))
            .collect();

        let center = compute_centroid_from_points(points);
        let surface = Surface::Plane(Plane::new(center, *normal));

        let loop_id = store.loops.insert(Loop {
            half_edges: vec![],
            face: FaceId::default(),
        });
        let face_id = store.faces.insert(Face {
            surface,
            outer_loop: loop_id,
            inner_loops: vec![],
            same_sense: true,
            shell: new_shell_id,
        });
        store.loops[loop_id].face = face_id;
        store.shells[new_shell_id].faces.push(face_id);

        for i in 0..vertex_ids.len() {
            let next = (i + 1) % vertex_ids.len();
            create_face_edge_twinned(store, vertex_ids[i], vertex_ids[next], face_id, loop_id, &mut edge_map);
        }
    }

    Ok(new_solid_id)
}

/// Quantize a point to integer coordinates for vertex dedup.
fn quantize_point(p: &Point3d) -> (i64, i64, i64) {
    // Use a resolution of 1e-8 to merge coincident vertices
    let scale = 1e8;
    (
        (p.x * scale).round() as i64,
        (p.y * scale).round() as i64,
        (p.z * scale).round() as i64,
    )
}

/// Get an existing vertex at this position or create a new one.
fn get_or_create_vertex(
    store: &mut EntityStore,
    vertex_map: &mut HashMap<(i64, i64, i64), VertexId>,
    point: Point3d,
) -> VertexId {
    let key = quantize_point(&point);
    *vertex_map.entry(key).or_insert_with(|| {
        store.vertices.insert(Vertex {
            point,
            tolerance: crate::default_tolerance().coincidence,
        })
    })
}

fn compute_centroid_from_points(points: &[Point3d]) -> Point3d {
    let n = points.len() as f64;
    let (sx, sy, sz) = points.iter().fold((0.0, 0.0, 0.0), |(x, y, z), p| {
        (x + p.x, y + p.y, z + p.z)
    });
    Point3d::new(sx / n, sy / n, sz / n)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::OperationError;
    use crate::topology::primitives::make_box;
    use crate::validation::audit::verify_topology_l0;

    /// Verify twin linking: no self-twins, and if twinned, twin(twin(he)) == he.
    fn assert_no_self_twins(store: &EntityStore, solid_id: SolidId) {
        let solid = &store.solids[solid_id];
        let mut twinned = 0usize;
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
                    if store.half_edges.contains_key(he.twin) {
                        twinned += 1;
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
        assert!(twinned > 0, "No twinned edges found at all");
    }

    #[test]
    fn test_chamfer_box_edge() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let result = chamfer_edge(&mut store, box_id, v0, v1, 2.0).unwrap();

        let shell = &store.shells[store.solids[result].shells[0]];
        assert_eq!(shell.faces.len(), 7, "Chamfered box should have 7 faces");

        let audit = verify_topology_l0(&store, result);
        assert!(audit.all_faces_closed, "Chamfered box has open loops");

        assert_no_self_twins(&store, result);
    }

    #[test]
    fn test_chamfer_creates_bevel() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let result = chamfer_edge(&mut store, box_id, v0, v1, 2.0).unwrap();

        let bb = store.solid_bounding_box(result);
        assert!((bb.max.x - 10.0).abs() < 1e-9);
        assert!((bb.max.y - 10.0).abs() < 1e-9);
        assert!((bb.max.z - 10.0).abs() < 1e-9);

        let audit = verify_topology_l0(&store, result);
        assert!(audit.all_faces_closed, "Chamfered bevel box has open loops");

        assert_no_self_twins(&store, result);
    }

    #[test]
    fn test_chamfer_nonexistent_edge() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let v0 = Point3d::new(99.0, 99.0, 99.0);
        let v1 = Point3d::new(100.0, 99.0, 99.0);
        let result = chamfer_edge(&mut store, box_id, v0, v1, 2.0);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OperationError::EdgeNotFound));
    }

    #[test]
    fn test_chamfer_on_already_chamfered_edge() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let first_chamfer = chamfer_edge(&mut store, box_id, v0, v1, 2.0).unwrap();

        let shell = &store.shells[store.solids[first_chamfer].shells[0]];
        assert_eq!(shell.faces.len(), 7, "First chamfer should produce 7 faces");

        let v2 = Point3d::new(0.0, 0.0, 10.0);
        let v3 = Point3d::new(10.0, 0.0, 10.0);
        let second_chamfer = chamfer_edge(&mut store, first_chamfer, v2, v3, 2.0).unwrap();

        let shell2 = &store.shells[store.solids[second_chamfer].shells[0]];
        assert_eq!(shell2.faces.len(), 8, "Double chamfer should produce 8 faces");

        let audit = verify_topology_l0(&store, second_chamfer);
        assert!(audit.all_faces_closed, "Double-chamfered box has open loops");

        assert_no_self_twins(&store, second_chamfer);
    }

    #[test]
    fn test_chamfer_zero_distance_returns_error() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);
        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let result = chamfer_edge(&mut store, box_id, v0, v1, 0.0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            OperationError::InvalidDimension { parameter: "distance", .. }
        ));
    }

    #[test]
    fn test_chamfer_very_short_edge() {
        // Chamfer on a very short edge of a tiny box
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 0.001, 0.001, 0.001);

        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(0.001, 0.0, 0.0);
        // Small chamfer distance relative to edge length
        let result = chamfer_edge(&mut store, box_id, v0, v1, 0.0001);
        assert!(result.is_ok(), "Chamfer on short edge should succeed");
    }
}
