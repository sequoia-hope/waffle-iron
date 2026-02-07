use crate::geometry::curves::{Curve, Line3d};
use crate::geometry::point::Point3d;
use crate::geometry::surfaces::{Plane, Surface};
use crate::geometry::vector::Vec3;
use crate::topology::brep::*;

/// Rotate a point around an axis using Rodrigues' rotation formula.
fn rotate_point_around_axis(
    point: &Point3d,
    axis_origin: &Point3d,
    axis_dir: &Vec3,
    angle: f64,
) -> Point3d {
    let v = *point - *axis_origin;
    let k = axis_dir.normalize();
    let cos_a = angle.cos();
    let sin_a = angle.sin();
    let rotated = v * cos_a + k.cross(&v) * sin_a + k * k.dot(&v) * (1.0 - cos_a);
    *axis_origin + rotated
}

/// Tolerance for comparing angles to a full revolution (2*PI).
const FULL_REVOLUTION_TOL: f64 = 1e-6;

/// Returns true if the angle is approximately a full revolution (2*PI).
fn is_full_revolution(angle: f64) -> bool {
    (angle.abs() - std::f64::consts::TAU).abs() < FULL_REVOLUTION_TOL
}

/// Revolve a profile (open polyline) around an axis to create a solid of revolution.
///
/// # Arguments
///
/// * `store` - The entity store to insert topology into.
/// * `profile` - Ordered points forming an open polyline in a plane that contains the axis.
///   These points define the cross-section to be revolved. Must have at least 2 points.
/// * `axis_origin` - A point on the rotation axis.
/// * `axis_direction` - The direction of the rotation axis (will be normalized internally).
/// * `angle` - The angle of revolution in radians. Use `2*PI` for a full revolution.
/// * `num_segments` - Number of angular subdivisions around the axis. Must be at least 3.
///
/// # Returns
///
/// A `SolidId` referencing the newly created solid of revolution.
///
/// # Panics
///
/// Panics if `profile` has fewer than 2 points or `num_segments` is less than 3.
pub fn revolve_profile(
    store: &mut EntityStore,
    profile: &[Point3d],
    axis_origin: Point3d,
    axis_direction: Vec3,
    angle: f64,
    num_segments: usize,
) -> SolidId {
    let n_profile = profile.len();
    assert!(n_profile >= 2, "Profile must have at least 2 points");
    assert!(num_segments >= 3, "Must have at least 3 segments");

    let full_rev = is_full_revolution(angle);

    // Number of distinct rings of vertices we need to create.
    // For a full revolution, the last ring wraps back to the first, so we only
    // create `num_segments` rings. For a partial revolution we need
    // `num_segments + 1` rings (start and end are distinct).
    let num_rings = if full_rev {
        num_segments
    } else {
        num_segments + 1
    };

    // Create vertex rings: rings[ring_index][profile_index]
    let mut rings: Vec<Vec<VertexId>> = Vec::with_capacity(num_rings);
    for ring_idx in 0..num_rings {
        let theta = angle * (ring_idx as f64) / (num_segments as f64);
        let ring: Vec<VertexId> = profile
            .iter()
            .map(|p| {
                let rotated = rotate_point_around_axis(p, &axis_origin, &axis_direction, theta);
                store.vertices.insert(Vertex {
                    point: rotated,
                    tolerance: 1e-7,
                })
            })
            .collect();
        rings.push(ring);
    }

    // Create solid and shell
    let solid_id = store.solids.insert(Solid { shells: vec![] });
    let shell_id = store.shells.insert(Shell {
        faces: vec![],
        orientation: ShellOrientation::Outward,
        solid: solid_id,
    });
    store.solids[solid_id].shells.push(shell_id);

    // Create side faces (quads) connecting consecutive rings.
    // For each segment along the revolution and each edge of the profile,
    // we create a quad face.
    for seg in 0..num_segments {
        let ring_a = seg;
        let ring_b = if full_rev {
            (seg + 1) % num_segments
        } else {
            seg + 1
        };

        for prof_edge in 0..(n_profile - 1) {
            // Quad vertices:
            //   v0 = rings[ring_a][prof_edge]
            //   v1 = rings[ring_a][prof_edge + 1]
            //   v2 = rings[ring_b][prof_edge + 1]
            //   v3 = rings[ring_b][prof_edge]
            //
            // Winding order chosen so the outward normal points away from the axis.
            let v0 = rings[ring_a][prof_edge];
            let v1 = rings[ring_a][prof_edge + 1];
            let v2 = rings[ring_b][prof_edge + 1];
            let v3 = rings[ring_b][prof_edge];

            let p0 = store.vertices[v0].point;
            let p1 = store.vertices[v1].point;
            let p3 = store.vertices[v3].point;

            let edge1 = p1 - p0;
            let edge2 = p3 - p0;
            let normal = edge1.cross(&edge2).normalized().unwrap_or(Vec3::Z);

            let quad_verts = vec![v0, v1, v2, v3];
            create_polygon_face(store, &quad_verts, shell_id, normal);
        }
    }

    // Cap faces (only for partial revolutions).
    if !full_rev {
        // Start cap: the profile at ring 0 (the original profile positions).
        let start_cap_verts: Vec<VertexId> = rings[0].clone();
        let start_normal = compute_cap_normal(store, &start_cap_verts);
        create_polygon_face(store, &start_cap_verts, shell_id, start_normal);

        // End cap: the profile at the last ring, reversed winding for outward normal.
        let end_cap_verts: Vec<VertexId> = rings[num_rings - 1].iter().rev().copied().collect();
        let end_normal = compute_cap_normal(store, &end_cap_verts);
        create_polygon_face(store, &end_cap_verts, shell_id, end_normal);
    }

    solid_id
}

/// Compute a normal for a cap face from its ordered vertices.
fn compute_cap_normal(store: &EntityStore, verts: &[VertexId]) -> Vec3 {
    if verts.len() < 3 {
        // For a 2-point cap (degenerate), pick an arbitrary normal.
        // We still try to compute something meaningful from the edge direction.
        if verts.len() == 2 {
            let p0 = store.vertices[verts[0]].point;
            let p1 = store.vertices[verts[1]].point;
            let edge = p1 - p0;
            // Pick a perpendicular vector
            if edge.x.abs() < 0.9 * edge.length() {
                return edge.cross(&Vec3::X).normalized().unwrap_or(Vec3::Z);
            } else {
                return edge.cross(&Vec3::Y).normalized().unwrap_or(Vec3::Z);
            }
        }
        return Vec3::Z;
    }

    let p0 = store.vertices[verts[0]].point;
    let p1 = store.vertices[verts[1]].point;
    let p2 = store.vertices[verts[2]].point;
    let e1 = p1 - p0;
    let e2 = p2 - p0;
    e1.cross(&e2).normalized().unwrap_or(Vec3::Z)
}

/// Create a polygon face from an ordered list of vertex ids, adding it to the shell.
/// This follows the same pattern as the extrude operation's create_polygon_face.
fn create_polygon_face(
    store: &mut EntityStore,
    verts: &[VertexId],
    shell_id: ShellId,
    normal: Vec3,
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
    use std::f64::consts::{FRAC_PI_2, TAU};

    #[test]
    fn test_revolve_full_circle() {
        // Revolve a single edge (2 points) 360 degrees around Y axis with 16 segments.
        // This creates a tube-like surface.
        let mut store = EntityStore::new();

        // Profile: two points along the X axis, offset from the Y axis.
        let profile = vec![
            Point3d::new(5.0, 0.0, 0.0),
            Point3d::new(5.0, 10.0, 0.0),
        ];

        let solid_id = revolve_profile(
            &mut store,
            &profile,
            Point3d::ORIGIN,
            Vec3::Y,
            TAU,
            16,
        );

        let solid = &store.solids[solid_id];
        assert_eq!(solid.shells.len(), 1);

        let shell = &store.shells[solid.shells[0]];

        // With 2 profile points (1 profile edge) and 16 segments around:
        // side faces = 16 * (2 - 1) = 16
        // Full revolution: no cap faces.
        assert_eq!(shell.faces.len(), 16);
    }

    #[test]
    fn test_revolve_partial() {
        // Revolve 90 degrees (PI/2) and verify cap faces exist.
        let mut store = EntityStore::new();

        let profile = vec![
            Point3d::new(5.0, 0.0, 0.0),
            Point3d::new(5.0, 10.0, 0.0),
        ];

        let solid_id = revolve_profile(
            &mut store,
            &profile,
            Point3d::ORIGIN,
            Vec3::Y,
            FRAC_PI_2,
            8,
        );

        let solid = &store.solids[solid_id];
        let shell = &store.shells[solid.shells[0]];

        // With 2 profile points (1 profile edge) and 8 segments at 90 degrees:
        // side faces = 8 * 1 = 8
        // Partial revolution: 2 cap faces (start and end).
        // Total = 8 + 2 = 10
        assert_eq!(shell.faces.len(), 10);

        // Verify we have more vertices than a full revolution would
        // (partial has separate start and end ring vertices).
        // Rings = 8 + 1 = 9, each with 2 points = 18 vertices.
        assert_eq!(store.vertices.len(), 18);
    }

    #[test]
    fn test_revolve_rectangle_profile() {
        // Revolve a 3-point L-shaped profile around Y axis to create a more complex solid.
        let mut store = EntityStore::new();

        let profile = vec![
            Point3d::new(3.0, 0.0, 0.0),
            Point3d::new(5.0, 0.0, 0.0),
            Point3d::new(5.0, 10.0, 0.0),
        ];

        let solid_id = revolve_profile(
            &mut store,
            &profile,
            Point3d::ORIGIN,
            Vec3::Y,
            TAU,
            12,
        );

        let solid = &store.solids[solid_id];
        let shell = &store.shells[solid.shells[0]];

        // With 3 profile points (2 profile edges) and 12 segments:
        // side faces = 12 * 2 = 24
        // Full revolution: no cap faces.
        assert_eq!(shell.faces.len(), 24);

        // Vertices: 12 rings * 3 profile points = 36.
        assert_eq!(store.vertices.len(), 36);
    }

    #[test]
    fn test_rotate_point_identity() {
        // Rotating by 0 should return the same point.
        let p = Point3d::new(5.0, 0.0, 0.0);
        let result = rotate_point_around_axis(&p, &Point3d::ORIGIN, &Vec3::Y, 0.0);
        assert!((result.x - p.x).abs() < 1e-10);
        assert!((result.y - p.y).abs() < 1e-10);
        assert!((result.z - p.z).abs() < 1e-10);
    }

    #[test]
    fn test_rotate_point_90_degrees() {
        // Rotating (5, 0, 0) by 90 degrees around Y should give (0, 0, -5).
        let p = Point3d::new(5.0, 0.0, 0.0);
        let result = rotate_point_around_axis(&p, &Point3d::ORIGIN, &Vec3::Y, FRAC_PI_2);
        assert!((result.x - 0.0).abs() < 1e-10);
        assert!((result.y - 0.0).abs() < 1e-10);
        assert!((result.z - (-5.0)).abs() < 1e-10);
    }
}
