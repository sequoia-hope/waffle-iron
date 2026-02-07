use crate::geometry::curves::{Curve, Line3d};
use crate::geometry::point::Point3d;
use crate::geometry::surfaces::{Plane, Surface};
use crate::geometry::vector::Vec3;
use crate::topology::brep::*;

/// Fillet (round) an edge of a solid with a circular arc.
///
/// For box-like solids, this replaces a sharp edge with a smooth curved bevel
/// approximated by a series of planar segments. The fillet is constructed by:
/// 1. Identifying the two faces adjacent to the specified edge
/// 2. Computing arc points along the fillet radius
/// 3. Building new faces: trimmed originals + fillet strip
///
/// Returns a new solid (the original is not modified).
pub fn fillet_edge(
    store: &mut EntityStore,
    solid_id: SolidId,
    edge_v0: Point3d,
    edge_v1: Point3d,
    radius: f64,
    segments: usize,
) -> SolidId {
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
    let tol = 1e-6;
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
        // Can't fillet — return a clone of the original
        let shells = store.solids[solid_id].shells.clone();
        return store.solids.insert(Solid { shells });
    }

    let fi_a = adjacent_face_indices[0];
    let fi_b = adjacent_face_indices[1];
    let normal_a = face_polygons[fi_a].1;
    let normal_b = face_polygons[fi_b].1;

    // Compute the fillet arc points along the edge
    // The arc goes from face_a's surface to face_b's surface
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

    // For each original face, emit modified or unchanged version
    for (fi, (verts, normal)) in face_polygons.iter().enumerate() {
        if fi == fi_a {
            // Replace the edge vertices with the first arc point on face A side
            let new_v0 = arc_points[0].0;
            let new_v1 = arc_points[0].1;
            let modified = replace_edge_verts(verts, &edge_v0, &edge_v1, &new_v0, &new_v1, tol);
            create_fillet_face(store, new_shell_id, &modified, *normal);
        } else if fi == fi_b {
            // Replace the edge vertices with the last arc point on face B side
            let last = arc_points.len() - 1;
            let new_v0 = arc_points[last].0;
            let new_v1 = arc_points[last].1;
            let modified = replace_edge_verts(verts, &edge_v0, &edge_v1, &new_v0, &new_v1, tol);
            create_fillet_face(store, new_shell_id, &modified, *normal);
        } else {
            // Unchanged faces that touch the edge vertices get updated
            let first_v0 = arc_points[0].0;
            let first_v1 = arc_points[0].1;
            let last_v0 = arc_points[arc_points.len() - 1].0;
            let last_v1 = arc_points[arc_points.len() - 1].1;

            let modified: Vec<Point3d> = verts
                .iter()
                .flat_map(|v| {
                    if v.distance_to(&edge_v0) < tol {
                        vec![first_v0, last_v0]
                    } else if v.distance_to(&edge_v1) < tol {
                        vec![first_v1, last_v1]
                    } else {
                        vec![*v]
                    }
                })
                .collect();
            create_fillet_face(store, new_shell_id, &modified, *normal);
        }
    }

    // Add fillet strip faces (one quad per arc segment)
    for i in 0..(arc_points.len() - 1) {
        let (a0, a1) = arc_points[i];
        let (b0, b1) = arc_points[i + 1];
        let quad = vec![a0, a1, b1, b0];

        // Compute outward normal for this fillet segment
        let mid = Point3d::new(
            (a0.x + a1.x + b0.x + b1.x) / 4.0,
            (a0.y + a1.y + b0.y + b1.y) / 4.0,
            (a0.z + a1.z + b0.z + b1.z) / 4.0,
        );
        let edge_mid = Point3d::new(
            (edge_v0.x + edge_v1.x) / 2.0,
            (edge_v0.y + edge_v1.y) / 2.0,
            (edge_v0.z + edge_v1.z) / 2.0,
        );
        let outward = (mid - edge_mid).normalized().unwrap_or(
            (normal_a + normal_b).normalized().unwrap_or(Vec3::Z),
        );

        create_fillet_face(store, new_shell_id, &quad, outward);
    }

    new_solid_id
}

/// Compute fillet arc points for both endpoints of the edge.
///
/// Returns a Vec of (point_at_v0, point_at_v1) pairs tracing the arc.
fn compute_fillet_arc_points(
    edge_v0: &Point3d,
    edge_v1: &Point3d,
    normal_a: &Vec3,
    normal_b: &Vec3,
    radius: f64,
    segments: usize,
) -> Vec<(Point3d, Point3d)> {
    let mut result = Vec::with_capacity(segments + 1);

    // The fillet arc transitions from face A's normal direction to face B's normal direction
    // We move edge_v0/v1 inward along each normal by radius, then trace an arc
    let offset_a = *normal_a * (-radius);
    let offset_b = *normal_b * (-radius);

    for i in 0..=segments {
        let t = i as f64 / segments as f64;
        // Spherical-linear interpolation (slerp) of the offset direction
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

    if theta.abs() < 1e-10 {
        // Vectors are nearly parallel, use linear interpolation
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

fn create_fillet_face(
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
    fn test_fillet_box_edge() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let result = fillet_edge(&mut store, box_id, v0, v1, 2.0, 4);

        let shell = &store.shells[store.solids[result].shells[0]];
        // Original 6 faces + 4 fillet strip segments = 10
        // (two adjacent faces trimmed, four non-adjacent faces get split vertices)
        assert!(
            shell.faces.len() >= 9,
            "Filleted box should have at least 9 faces, got {}",
            shell.faces.len()
        );
    }

    #[test]
    fn test_fillet_preserves_bounding_box() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0);

        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let result = fillet_edge(&mut store, box_id, v0, v1, 2.0, 4);

        let bb = store.solid_bounding_box(result);
        // Bounding box should be <= original (fillet removes material)
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

        let shell = &store.shells[store.solids[result].shells[0]];
        assert_eq!(shell.faces.len(), 6, "Non-existent edge fillet should return unchanged");
    }

    #[test]
    fn test_fillet_multi_segment() {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, 10.0, 8.0, 6.0);

        let v0 = Point3d::new(0.0, 0.0, 0.0);
        let v1 = Point3d::new(10.0, 0.0, 0.0);
        let result = fillet_edge(&mut store, box_id, v0, v1, 1.5, 8);

        let shell = &store.shells[store.solids[result].shells[0]];
        // With 8 segments we get 8 strip faces + 6 modified original faces
        assert!(
            shell.faces.len() >= 13,
            "8-segment fillet should produce many faces, got {}",
            shell.faces.len()
        );
    }
}
