use std::collections::HashMap;

use crate::geometry::point::Point3d;
use crate::geometry::surfaces::{Plane, Surface};
use crate::geometry::vector::Vec3;
use crate::operations::OperationError;
use crate::topology::brep::*;
use crate::topology::primitives::create_face_edge_twinned;

/// Sweep a profile along a polyline path to create a solid.
///
/// The profile is placed at each path waypoint by computing a local coordinate
/// frame (tangent + two perpendicular axes). Side faces connect consecutive
/// profile instances, and cap faces close the start and end.
///
/// # Arguments
///
/// * `store` - The entity store to insert topology into.
/// * `profile` - Points forming a closed polygon cross-section (>= 3 points).
///   These are specified relative to a local frame: the profile is centered
///   at the origin, lying in a plane perpendicular to the first path segment.
/// * `path` - Polyline waypoints (>= 2 points). The profile is swept from
///   path[0] to path[last].
pub fn sweep_profile(
    store: &mut EntityStore,
    profile: &[Point3d],
    path: &[Point3d],
) -> Result<SolidId, OperationError> {
    let n_prof = profile.len();
    if n_prof < 3 {
        return Err(OperationError::InsufficientProfile {
            required: 3,
            provided: n_prof,
        });
    }

    let n_path = path.len();
    if n_path < 2 {
        return Err(OperationError::InsufficientPath {
            required: 2,
            provided: n_path,
        });
    }

    // Compute local frames at each path point.
    // Each frame is (tangent, normal, binormal) where the profile lies in the
    // normal-binormal plane.
    let frames = compute_frames(path);

    // Transform the profile to each path point using the local frame.
    let mut rings: Vec<Vec<VertexId>> = Vec::with_capacity(n_path);
    for (seg_idx, frame) in frames.iter().enumerate() {
        let origin = path[seg_idx];
        let mut ring = Vec::with_capacity(n_prof);
        for p in profile {
            // Profile point in local coordinates: p.x along normal, p.y along binormal
            let world = origin
                + frame.normal * p.x
                + frame.binormal * p.y
                + frame.tangent * p.z;
            ring.push(store.vertices.insert(Vertex {
                point: world,
                tolerance: crate::default_tolerance().coincidence,
            }));
        }
        rings.push(ring);
    }

    let solid_id = store.solids.insert(Solid { shells: vec![] });
    let shell_id = store.shells.insert(Shell {
        faces: vec![],
        orientation: ShellOrientation::Outward,
        solid: solid_id,
    });
    store.solids[solid_id].shells.push(shell_id);

    let mut edge_map: HashMap<(VertexId, VertexId), HalfEdgeId> = HashMap::new();

    // Start cap (first ring, reversed winding)
    {
        let center = compute_centroid(store, &rings[0]);
        let cap_normal = -frames[0].tangent;
        let surface = Surface::Plane(Plane::new(center, cap_normal));
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

        for i in 0..n_prof {
            let from = (n_prof - i) % n_prof;
            let to = if from == 0 { n_prof - 1 } else { from - 1 };
            create_face_edge_twinned(
                store,
                rings[0][from],
                rings[0][to],
                face_id,
                loop_id,
                &mut edge_map,
            );
        }
    }

    // End cap (last ring, forward winding)
    {
        let last = n_path - 1;
        let center = compute_centroid(store, &rings[last]);
        let cap_normal = frames[last].tangent;
        let surface = Surface::Plane(Plane::new(center, cap_normal));
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

        for i in 0..n_prof {
            let next = (i + 1) % n_prof;
            create_face_edge_twinned(
                store,
                rings[last][i],
                rings[last][next],
                face_id,
                loop_id,
                &mut edge_map,
            );
        }
    }

    // Side faces connecting consecutive rings
    for seg in 0..(n_path - 1) {
        let ring_a = &rings[seg];
        let ring_b = &rings[seg + 1];

        for i in 0..n_prof {
            let next = (i + 1) % n_prof;

            let v0 = ring_a[i];
            let v1 = ring_a[next];
            let v2 = ring_b[next];
            let v3 = ring_b[i];

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

    Ok(solid_id)
}

/// A local coordinate frame at a path point.
struct Frame {
    tangent: Vec3,
    normal: Vec3,
    binormal: Vec3,
}

/// Compute local frames at each path waypoint using a rotation-minimizing
/// approach. The first frame's tangent is the first segment direction, with
/// normal and binormal chosen to form a right-handed coordinate system.
/// Subsequent frames rotate the previous frame to align with the new tangent.
fn compute_frames(path: &[Point3d]) -> Vec<Frame> {
    let n = path.len();
    let mut frames = Vec::with_capacity(n);

    // First tangent
    let t0 = (path[1] - path[0]).normalize();

    // Choose initial normal perpendicular to tangent
    let initial_normal = if t0.x.abs() < 0.9 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let n0 = (initial_normal - t0 * initial_normal.dot(&t0)).normalize();
    let b0 = t0.cross(&n0);

    frames.push(Frame {
        tangent: t0,
        normal: n0,
        binormal: b0,
    });

    // Propagate frame along path using rotation minimizing frames (double reflection)
    for i in 1..n {
        let prev = &frames[i - 1];

        // Tangent at this point
        let ti = if i < n - 1 {
            // Average of incoming and outgoing segment directions
            let seg_in = (path[i] - path[i - 1]).normalize();
            let seg_out = (path[i + 1] - path[i]).normalize();
            (seg_in + seg_out).normalized().unwrap_or(seg_in)
        } else {
            // Last point: use incoming segment direction
            (path[i] - path[i - 1]).normalize()
        };

        // Rotate previous frame to align with new tangent
        // Using reflection method for rotation-minimizing frames
        let v1 = path[i] - path[i - 1];
        let c1 = v1.dot(&v1);
        if c1 < 1e-30 {
            // Degenerate segment, reuse previous frame
            frames.push(Frame {
                tangent: ti,
                normal: prev.normal,
                binormal: prev.binormal,
            });
            continue;
        }

        // Reflect previous normal and tangent
        let r_l = prev.normal - v1 * (2.0 * v1.dot(&prev.normal) / c1);
        let t_l = prev.tangent - v1 * (2.0 * v1.dot(&prev.tangent) / c1);

        // Second reflection to align with new tangent
        let v2 = ti - t_l;
        let c2 = v2.dot(&v2);
        let ni = if c2 < 1e-30 {
            r_l
        } else {
            r_l - v2 * (2.0 * v2.dot(&r_l) / c2)
        };

        let bi = ti.cross(&ni);

        frames.push(Frame {
            tangent: ti,
            normal: ni,
            binormal: bi,
        });
    }

    frames
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
    fn test_sweep_straight_path_approximates_extrude() {
        let mut store = EntityStore::new();
        // Rectangle profile in XY plane
        let profile = vec![
            Point3d::new(-2.0, -1.5, 0.0),
            Point3d::new(2.0, -1.5, 0.0),
            Point3d::new(2.0, 1.5, 0.0),
            Point3d::new(-2.0, 1.5, 0.0),
        ];
        // Straight path along Z
        let path = vec![
            Point3d::new(0.0, 0.0, 0.0),
            Point3d::new(0.0, 0.0, 10.0),
        ];

        let solid_id = sweep_profile(&mut store, &profile, &path).unwrap();

        // Should produce a box-like shape: 2 caps + 4 sides = 6 faces
        let shell = &store.shells[store.solids[solid_id].shells[0]];
        assert_eq!(shell.faces.len(), 6);

        let bb = store.solid_bounding_box(solid_id);
        assert!((bb.min.x - (-2.0)).abs() < 1e-10);
        assert!((bb.max.x - 2.0).abs() < 1e-10);
        assert!((bb.min.z - 0.0).abs() < 1e-10);
        assert!((bb.max.z - 10.0).abs() < 1e-10);

        let audit = verify_topology_l0(&store, solid_id);
        assert!(audit.all_faces_closed, "Sweep has open loops");
        assert!(audit.euler_valid, "Euler formula violated");

        assert_twin_invariants(&store, solid_id);

        let geom_errors = verify_geometry_l1(&store, solid_id);
        assert!(geom_errors.is_empty(), "Geometry errors: {:?}", geom_errors);
    }

    #[test]
    fn test_sweep_l_shaped_path() {
        let mut store = EntityStore::new();
        // Triangle profile
        let profile = vec![
            Point3d::new(-1.0, -1.0, 0.0),
            Point3d::new(1.0, -1.0, 0.0),
            Point3d::new(0.0, 1.0, 0.0),
        ];
        // L-shaped path: up then right
        let path = vec![
            Point3d::new(0.0, 0.0, 0.0),
            Point3d::new(0.0, 0.0, 5.0),
            Point3d::new(5.0, 0.0, 5.0),
        ];

        let solid_id = sweep_profile(&mut store, &profile, &path).unwrap();

        // 2 caps + 2 segments * 3 sides = 8 faces
        let shell = &store.shells[store.solids[solid_id].shells[0]];
        assert_eq!(shell.faces.len(), 8);

        let audit = verify_topology_l0(&store, solid_id);
        assert!(audit.all_faces_closed, "L-sweep has open loops");
        assert!(audit.euler_valid, "Euler formula violated for L-sweep");

        assert_twin_invariants(&store, solid_id);
    }

    #[test]
    fn test_sweep_insufficient_profile_returns_error() {
        let mut store = EntityStore::new();
        let profile = vec![
            Point3d::new(0.0, 0.0, 0.0),
            Point3d::new(1.0, 0.0, 0.0),
        ];
        let path = vec![
            Point3d::new(0.0, 0.0, 0.0),
            Point3d::new(0.0, 0.0, 5.0),
        ];
        let result = sweep_profile(&mut store, &profile, &path);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            OperationError::InsufficientProfile { .. }
        ));
    }

    #[test]
    fn test_sweep_insufficient_path_returns_error() {
        let mut store = EntityStore::new();
        let profile = vec![
            Point3d::new(-1.0, -1.0, 0.0),
            Point3d::new(1.0, -1.0, 0.0),
            Point3d::new(0.0, 1.0, 0.0),
        ];
        let path = vec![Point3d::new(0.0, 0.0, 0.0)];
        let result = sweep_profile(&mut store, &profile, &path);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            OperationError::InsufficientPath { .. }
        ));
    }

    #[test]
    fn test_sweep_pentagon_along_straight_path() {
        let mut store = EntityStore::new();
        let mut profile = Vec::new();
        for i in 0..5 {
            let angle = 2.0 * std::f64::consts::PI * (i as f64) / 5.0;
            profile.push(Point3d::new(2.0 * angle.cos(), 2.0 * angle.sin(), 0.0));
        }
        let path = vec![
            Point3d::new(0.0, 0.0, 0.0),
            Point3d::new(0.0, 0.0, 8.0),
        ];

        let solid_id = sweep_profile(&mut store, &profile, &path).unwrap();

        // 2 caps + 5 sides = 7
        let shell = &store.shells[store.solids[solid_id].shells[0]];
        assert_eq!(shell.faces.len(), 7);

        let audit = verify_topology_l0(&store, solid_id);
        assert!(audit.all_faces_closed);
        assert!(audit.euler_valid);

        assert_twin_invariants(&store, solid_id);
    }
}
