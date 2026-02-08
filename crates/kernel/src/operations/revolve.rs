use std::collections::HashMap;

use crate::geometry::point::Point3d;
use crate::geometry::surfaces::{Plane, Surface};
use crate::geometry::vector::Vec3;
use crate::operations::OperationError;
use crate::topology::brep::*;
use crate::topology::primitives::create_face_edge_twinned;

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

/// Returns true if the angle is approximately a full revolution (2*PI).
fn is_full_revolution(angle: f64) -> bool {
    (angle.abs() - std::f64::consts::TAU).abs() < crate::default_tolerance().angular
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
/// A `Result<SolidId, OperationError>` referencing the newly created solid of revolution.
pub fn revolve_profile(
    store: &mut EntityStore,
    profile: &[Point3d],
    axis_origin: Point3d,
    axis_direction: Vec3,
    angle: f64,
    num_segments: usize,
) -> Result<SolidId, OperationError> {
    let n_profile = profile.len();
    if n_profile < 2 {
        return Err(OperationError::InsufficientProfile {
            required: 2,
            provided: n_profile,
        });
    }
    if num_segments < 3 {
        return Err(OperationError::InsufficientSegments {
            required: 3,
            provided: num_segments,
        });
    }
    if angle.abs() < 1e-15 {
        return Err(OperationError::InvalidDimension {
            parameter: "angle",
            value: angle,
        });
    }
    if axis_direction.length() < 1e-15 {
        return Err(OperationError::ZeroDirection);
    }

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
                    tolerance: crate::default_tolerance().coincidence,
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

    // Shared edge map for twin linking across all faces
    let mut edge_map: HashMap<(VertexId, VertexId), HalfEdgeId> = HashMap::new();

    // Create side faces (quads) connecting consecutive rings.
    for seg in 0..num_segments {
        let ring_a = seg;
        let ring_b = if full_rev {
            (seg + 1) % num_segments
        } else {
            seg + 1
        };

        for prof_edge in 0..(n_profile - 1) {
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

            create_face_edge_twinned(store, v0, v1, face_id, loop_id, &mut edge_map);
            create_face_edge_twinned(store, v1, v2, face_id, loop_id, &mut edge_map);
            create_face_edge_twinned(store, v2, v3, face_id, loop_id, &mut edge_map);
            create_face_edge_twinned(store, v3, v0, face_id, loop_id, &mut edge_map);
        }
    }

    // Cap faces (only for partial revolutions with 3+ profile points).
    // A 2-point cap is degenerate (only 2 vertices) and cannot form a valid polygon face.
    if !full_rev && n_profile >= 3 {
        // Start cap: the profile at ring 0, reversed winding so the outward
        // normal opposes the side face edges (which traverse 0->1 along profile).
        {
            let start_cap_verts: Vec<VertexId> = rings[0].iter().rev().copied().collect();
            let normal = compute_cap_normal(store, &start_cap_verts);
            let center = compute_centroid(store, &start_cap_verts);
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

            for i in 0..start_cap_verts.len() {
                let next = (i + 1) % start_cap_verts.len();
                create_face_edge_twinned(store, start_cap_verts[i], start_cap_verts[next], face_id, loop_id, &mut edge_map);
            }
        }

        // End cap: the profile at the last ring, forward winding (side faces
        // traverse n-1->...->0 on the end ring, so cap traverses 0->1->...->n-1).
        {
            let end_cap_verts: Vec<VertexId> = rings[num_rings - 1].clone();
            let normal = compute_cap_normal(store, &end_cap_verts);
            let center = compute_centroid(store, &end_cap_verts);
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

            for i in 0..end_cap_verts.len() {
                let next = (i + 1) % end_cap_verts.len();
                create_face_edge_twinned(store, end_cap_verts[i], end_cap_verts[next], face_id, loop_id, &mut edge_map);
            }
        }
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

/// Compute a normal for a cap face from its ordered vertices.
fn compute_cap_normal(store: &EntityStore, verts: &[VertexId]) -> Vec3 {
    if verts.len() < 3 {
        if verts.len() == 2 {
            let p0 = store.vertices[verts[0]].point;
            let p1 = store.vertices[verts[1]].point;
            let edge = p1 - p0;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::OperationError;
    use crate::validation::audit::verify_topology_l0;
    use std::f64::consts::{FRAC_PI_2, TAU};

    /// Verify twin linking: no self-twins, and if twinned, twin(twin(he)) == he
    /// and twins are on different faces. Boundary edges (twin == default) are
    /// allowed for open surfaces like capless revolves.
    fn assert_no_self_twins(store: &EntityStore, solid_id: SolidId) {
        let solid = &store.solids[solid_id];
        let mut twinned = 0usize;
        let mut boundary = 0usize;
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
                    } else {
                        boundary += 1;
                    }
                }
            }
        }
        assert!(twinned > 0, "No twinned edges found at all");
        // For debugging: boundary edges are ok for open surfaces
        let _ = boundary;
    }

    #[test]
    fn test_revolve_full_circle() {
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
            TAU,
            16,
        )
        .unwrap();

        let solid = &store.solids[solid_id];
        assert_eq!(solid.shells.len(), 1);

        let shell = &store.shells[solid.shells[0]];
        assert_eq!(shell.faces.len(), 16);

        let audit = verify_topology_l0(&store, solid_id);
        assert!(audit.all_faces_closed, "Full revolve has open loops");

        assert_no_self_twins(&store, solid_id);
    }

    #[test]
    fn test_revolve_partial() {
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
        )
        .unwrap();

        let solid = &store.solids[solid_id];
        let shell = &store.shells[solid.shells[0]];
        // 2-point profile: 8 side quads, no caps (degenerate 2-vertex caps skipped)
        assert_eq!(shell.faces.len(), 8);
        assert_eq!(store.vertices.len(), 18);

        let audit = verify_topology_l0(&store, solid_id);
        assert!(audit.all_faces_closed, "Partial revolve has open loops");

        assert_no_self_twins(&store, solid_id);
    }

    #[test]
    fn test_revolve_rectangle_profile() {
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
        )
        .unwrap();

        let solid = &store.solids[solid_id];
        let shell = &store.shells[solid.shells[0]];
        assert_eq!(shell.faces.len(), 24);
        assert_eq!(store.vertices.len(), 36);

        let audit = verify_topology_l0(&store, solid_id);
        assert!(audit.all_faces_closed, "Multi-edge revolve has open loops");

        assert_no_self_twins(&store, solid_id);
    }

    #[test]
    fn test_revolve_360_is_full_revolution() {
        assert!(is_full_revolution(TAU));
        assert!(is_full_revolution(-TAU));
        assert!(!is_full_revolution(std::f64::consts::PI));
        assert!(!is_full_revolution(0.0));
    }

    #[test]
    fn test_rotate_point_identity() {
        let p = Point3d::new(5.0, 0.0, 0.0);
        let result = rotate_point_around_axis(&p, &Point3d::ORIGIN, &Vec3::Y, 0.0);
        assert!((result.x - p.x).abs() < 1e-10);
        assert!((result.y - p.y).abs() < 1e-10);
        assert!((result.z - p.z).abs() < 1e-10);
    }

    #[test]
    fn test_rotate_point_90_degrees() {
        let p = Point3d::new(5.0, 0.0, 0.0);
        let result = rotate_point_around_axis(&p, &Point3d::ORIGIN, &Vec3::Y, FRAC_PI_2);
        assert!((result.x - 0.0).abs() < 1e-10);
        assert!((result.y - 0.0).abs() < 1e-10);
        assert!((result.z - (-5.0)).abs() < 1e-10);
    }

    #[test]
    fn test_revolve_zero_angle_returns_error() {
        let mut store = EntityStore::new();
        let profile = vec![
            Point3d::new(5.0, 0.0, 0.0),
            Point3d::new(5.0, 10.0, 0.0),
        ];
        let result = revolve_profile(&mut store, &profile, Point3d::ORIGIN, Vec3::Y, 0.0, 8);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            OperationError::InvalidDimension { parameter: "angle", .. }
        ));
    }

    #[test]
    fn test_revolve_insufficient_profile_returns_error() {
        let mut store = EntityStore::new();
        let profile = vec![Point3d::new(5.0, 0.0, 0.0)];
        let result = revolve_profile(&mut store, &profile, Point3d::ORIGIN, Vec3::Y, TAU, 8);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            OperationError::InsufficientProfile { required: 2, provided: 1 }
        ));
    }

    #[test]
    fn test_revolve_insufficient_segments_returns_error() {
        let mut store = EntityStore::new();
        let profile = vec![
            Point3d::new(5.0, 0.0, 0.0),
            Point3d::new(5.0, 10.0, 0.0),
        ];
        let result = revolve_profile(&mut store, &profile, Point3d::ORIGIN, Vec3::Y, TAU, 2);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            OperationError::InsufficientSegments { required: 3, provided: 2 }
        ));
    }
}
