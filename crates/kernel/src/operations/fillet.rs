use std::collections::HashMap;

use crate::geometry::point::Point3d;
use crate::geometry::surfaces::{Plane, Surface};
use crate::geometry::vector::Vec3;
use crate::operations::OperationError;
use crate::topology::brep::*;
use crate::topology::primitives::create_face_edge_twinned;

/// Fillet (round) an edge of a solid with a circular arc.
///
/// For box-like solids, this replaces a sharp edge with a smooth curved bevel
/// approximated by a series of planar segments. The fillet is constructed by:
/// 1. Identifying the two faces adjacent to the specified edge
/// 2. Computing arc points along the fillet radius
/// 3. Building new faces: trimmed originals + fillet strip
///
/// Returns a new solid (the original is not modified), or an error if the
/// radius is not positive or the edge is not found.
pub fn fillet_edge(
    store: &mut EntityStore,
    solid_id: SolidId,
    edge_v0: Point3d,
    edge_v1: Point3d,
    radius: f64,
    segments: usize,
) -> Result<SolidId, OperationError> {
    if radius <= 0.0 {
        return Err(OperationError::InvalidDimension {
            parameter: "radius",
            value: radius,
        });
    }

    let solid = &store.solids[solid_id];
    let shell_id = solid.shells[0];
    let shell = &store.shells[shell_id];

    // Collect all face polygons from original solid
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

    // Compute the fillet arc points along the edge
    let segments = segments.max(2);
    let arc_points = compute_fillet_arc_points(
        &edge_v0, &edge_v1, &normal_a, &normal_b, radius, segments,
    );

    // Build the new solid
    let new_solid_id = store.solids.insert(Solid { shells: vec![] });
    let new_shell_id = store.shells.insert(Shell {
        faces: vec![],
        orientation: ShellOrientation::Outward,
        solid: new_solid_id,
    });
    store.solids[new_solid_id].shells.push(new_shell_id);

    // Vertex dedup map and edge twin linking map
    let mut vertex_map: HashMap<(i64, i64, i64), VertexId> = HashMap::new();
    let mut edge_map: HashMap<(VertexId, VertexId), HalfEdgeId> = HashMap::new();

    // Collect all new face point-lists
    let mut new_face_polys: Vec<(Vec<Point3d>, Vec3)> = Vec::new();

    // arc_points[0] has offset = -normal_a * r, which is the face B tangent point
    // arc_points[last] has offset = -normal_b * r, which is the face A tangent point
    let last_arc = arc_points.len() - 1;

    for (fi, (verts, normal)) in face_polygons.iter().enumerate() {
        if fi == fi_a {
            // Face A: use arc_points[LAST] (the face A tangent — on face A's plane)
            let new_v0 = arc_points[last_arc].0;
            let new_v1 = arc_points[last_arc].1;
            let modified = replace_edge_verts(verts, &edge_v0, &edge_v1, &new_v0, &new_v1, tol);
            new_face_polys.push((modified, *normal));
        } else if fi == fi_b {
            // Face B: use arc_points[0] (the face B tangent — on face B's plane)
            let new_v0 = arc_points[0].0;
            let new_v1 = arc_points[0].1;
            let modified = replace_edge_verts(verts, &edge_v0, &edge_v1, &new_v0, &new_v1, tol);
            new_face_polys.push((modified, *normal));
        } else {
            // Non-adjacent face: insert ALL arc points at each shared vertex,
            // with ordering determined by polygon context (distance heuristic).
            let modified = replace_vertex_with_arc_chain(
                verts, &edge_v0, &edge_v1, &arc_points, tol,
            );
            new_face_polys.push((modified, *normal));
        }
    }

    // Add fillet strip faces (one quad per arc segment)
    let edge_mid = Point3d::new(
        (edge_v0.x + edge_v1.x) / 2.0,
        (edge_v0.y + edge_v1.y) / 2.0,
        (edge_v0.z + edge_v1.z) / 2.0,
    );
    for i in 0..last_arc {
        let (a0, a1) = arc_points[i];
        let (b0, b1) = arc_points[i + 1];

        let mid = Point3d::new(
            (a0.x + a1.x + b0.x + b1.x) / 4.0,
            (a0.y + a1.y + b0.y + b1.y) / 4.0,
            (a0.z + a1.z + b0.z + b1.z) / 4.0,
        );
        let outward = (mid - edge_mid).normalized().unwrap_or(
            (normal_a + normal_b).normalized().unwrap_or(Vec3::Z),
        );

        // Determine correct quad winding by checking geometric normal
        let v1 = Vec3::new(a1.x - a0.x, a1.y - a0.y, a1.z - a0.z);
        let v2 = Vec3::new(b0.x - a0.x, b0.y - a0.y, b0.z - a0.z);
        let geo_normal = v1.cross(&v2);

        let quad = if geo_normal.dot(&outward) >= 0.0 {
            vec![a0, a1, b1, b0]
        } else {
            vec![b0, b1, a1, a0]
        };

        new_face_polys.push((quad, outward));
    }

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

/// Compute fillet arc points for both endpoints of the edge.
fn compute_fillet_arc_points(
    edge_v0: &Point3d,
    edge_v1: &Point3d,
    normal_a: &Vec3,
    normal_b: &Vec3,
    radius: f64,
    segments: usize,
) -> Vec<(Point3d, Point3d)> {
    let mut result = Vec::with_capacity(segments + 1);

    let offset_a = *normal_a * (-radius);
    let offset_b = *normal_b * (-radius);

    for i in 0..=segments {
        let t = i as f64 / segments as f64;
        let offset = slerp_vec3(&offset_a, &offset_b, t);

        let p0 = *edge_v0 + offset;
        let p1 = *edge_v1 + offset;
        result.push((p0, p1));
    }

    result
}

/// Spherical linear interpolation between two vectors.
fn slerp_vec3(a: &Vec3, b: &Vec3, t: f64) -> Vec3 {
    let dot = a.dot(b) / (a.length() * b.length()).max(1e-15);
    let dot = dot.clamp(-1.0, 1.0);
    let theta = dot.acos();

    if theta.abs() < crate::default_tolerance().angular {
        *a * (1.0 - t) + *b * t
    } else {
        let sin_theta = theta.sin();
        *a * ((1.0 - t) * theta).sin() / sin_theta + *b * (t * theta).sin() / sin_theta
    }
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

/// Replace occurrences of edge_v0 / edge_v1 in a polygon with the FULL arc
/// point chain, using a distance-based heuristic to determine ordering.
///
/// For each vertex matching edge_v0 or edge_v1, the predecessor vertex in the
/// polygon determines whether the arc points should be inserted in forward
/// (index 0..last) or reverse (last..0) order.  The predecessor is closer to
/// one end of the arc; we start from that end so that shared edges between
/// the non-adjacent face and the adjacent/strip faces have opposite traversal
/// directions, enabling proper twin linking.
fn replace_vertex_with_arc_chain(
    verts: &[Point3d],
    edge_v0: &Point3d,
    edge_v1: &Point3d,
    arc_points: &[(Point3d, Point3d)],
    tol: f64,
) -> Vec<Point3d> {
    let n = verts.len();
    let mut result = Vec::new();

    for i in 0..n {
        let v = &verts[i];

        if v.distance_to(edge_v0) < tol {
            let pred = &verts[(i + n - 1) % n];
            let arc_first = arc_points[0].0;
            let arc_last = arc_points[arc_points.len() - 1].0;

            if pred.distance_to(&arc_first) <= pred.distance_to(&arc_last) {
                for ap in arc_points {
                    result.push(ap.0);
                }
            } else {
                for ap in arc_points.iter().rev() {
                    result.push(ap.0);
                }
            }
        } else if v.distance_to(edge_v1) < tol {
            let pred = &verts[(i + n - 1) % n];
            let arc_first = arc_points[0].1;
            let arc_last = arc_points[arc_points.len() - 1].1;

            if pred.distance_to(&arc_first) <= pred.distance_to(&arc_last) {
                for ap in arc_points {
                    result.push(ap.1);
                }
            } else {
                for ap in arc_points.iter().rev() {
                    result.push(ap.1);
                }
            }
        } else {
            result.push(*v);
        }
    }

    result
}

/// Quantize a point to integer coordinates for vertex dedup.
fn quantize_point(p: &Point3d) -> (i64, i64, i64) {
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
    fn test_fillet_box_edge() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let result = fillet_edge(&mut store, box_id, v0, v1, 2.0, 4).unwrap();

        let shell = &store.shells[store.solids[result].shells[0]];
        assert!(
            shell.faces.len() >= 9,
            "Filleted box should have at least 9 faces, got {}",
            shell.faces.len()
        );

        let audit = verify_topology_l0(&store, result);
        assert!(audit.all_faces_closed, "Filleted box has open loops");

        assert_no_self_twins(&store, result);
    }

    #[test]
    fn test_fillet_preserves_bounding_box() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let result = fillet_edge(&mut store, box_id, v0, v1, 2.0, 4).unwrap();

        let bb = store.solid_bounding_box(result);
        assert!(bb.max.x <= 10.0 + 1e-6);
        assert!(bb.max.y <= 10.0 + 1e-6);
        assert!(bb.max.z <= 10.0 + 1e-6);
    }

    #[test]
    fn test_fillet_nonexistent_edge() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let v0 = Point3d::new(99.0, 99.0, 99.0);
        let v1 = Point3d::new(100.0, 99.0, 99.0);
        let result = fillet_edge(&mut store, box_id, v0, v1, 2.0, 4);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OperationError::EdgeNotFound));
    }

    #[test]
    fn test_fillet_multi_segment() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 8.0, 6.0);

        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let result = fillet_edge(&mut store, box_id, v0, v1, 1.5, 8).unwrap();

        let shell = &store.shells[store.solids[result].shells[0]];
        assert!(
            shell.faces.len() >= 13,
            "8-segment fillet should produce many faces, got {}",
            shell.faces.len()
        );

        let audit = verify_topology_l0(&store, result);
        assert!(audit.all_faces_closed, "Multi-segment fillet has open loops");

        assert_no_self_twins(&store, result);
    }

    #[test]
    fn test_fillet_radius_larger_than_edge() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 5.0, 5.0, 5.0);

        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(5.0, 0.0, 0.0);
        let result = fillet_edge(&mut store, box_id, v0, v1, 10.0, 4).unwrap();

        let shell = &store.shells[store.solids[result].shells[0]];
        assert!(shell.faces.len() >= 6, "Oversized fillet should still produce faces");
    }

    #[test]
    fn test_fillet_minimum_segments() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let result = fillet_edge(&mut store, box_id, v0, v1, 2.0, 1).unwrap();

        let shell = &store.shells[store.solids[result].shells[0]];
        assert!(shell.faces.len() >= 7, "Minimum-segment fillet should produce faces");
    }

    #[test]
    fn test_fillet_zero_radius_returns_error() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);
        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let result = fillet_edge(&mut store, box_id, v0, v1, 0.0, 4);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            OperationError::InvalidDimension { parameter: "radius", .. }
        ));
    }
}
