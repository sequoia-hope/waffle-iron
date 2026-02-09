//! Higher-level primitive builders on top of truck's sweep API.
//!
//! truck has no built-in box/cylinder/sphere — everything is successive sweeps.

use std::f64::consts::PI;
use truck_modeling::builder;
use truck_modeling::topology::{Edge, Solid, Wire};
use truck_modeling::{EuclideanSpace, Point3, Rad, Vector3};

/// Create a box solid via successive translational sweeps.
/// Origin at (0,0,0), extends to (w,h,d).
pub fn make_box(w: f64, h: f64, d: f64) -> Solid {
    let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
    let edge = builder::tsweep(&v, Vector3::new(w, 0.0, 0.0));
    let face = builder::tsweep(&edge, Vector3::new(0.0, h, 0.0));
    builder::tsweep(&face, Vector3::new(0.0, 0.0, d))
}

/// Create a cylinder solid: circle wire → face → translational sweep.
/// Base centered at origin in XY plane, extending along +Z.
pub fn make_cylinder(radius: f64, height: f64) -> Solid {
    let v = builder::vertex(Point3::new(radius, 0.0, 0.0));
    let wire = builder::rsweep(&v, Point3::origin(), Vector3::unit_z(), Rad(2.0 * PI));
    let face = builder::try_attach_plane(&[wire]).expect("Failed to create circular face");
    builder::tsweep(&face, Vector3::new(0.0, 0.0, height))
}

/// Create a sphere solid: semicircle face → rotational sweep 2π.
/// Centered at origin.
pub fn make_sphere(radius: f64) -> Solid {
    // Create semicircle arc in XZ plane: rotate (r,0,0) around Y axis by PI
    // This produces a wire from (r,0,0) through (0,0,r) to (-r,0,0) in XZ plane
    let v_right = builder::vertex(Point3::new(radius, 0.0, 0.0));
    let arc_wire = builder::rsweep(&v_right, Point3::origin(), Vector3::unit_y(), Rad(PI));

    // Close with line from (-r,0,0) to (r,0,0)
    let v_left = builder::vertex(Point3::new(-radius, 0.0, 0.0));
    let line_edge: Edge = builder::tsweep(&v_left, Vector3::new(2.0 * radius, 0.0, 0.0));

    // Combine arc edges + line edge into closed wire
    let mut edges: Vec<Edge> = Vec::new();
    for edge in arc_wire.edge_iter() {
        edges.push(edge.clone());
    }
    edges.push(line_edge);
    let closed_wire = Wire::from_iter(edges);

    // Attach plane to make a half-disc face
    let face = builder::try_attach_plane(&[closed_wire]).expect("Failed to create semicircle face");

    // Revolve around Z axis by 2π to create sphere
    builder::rsweep(&face, Point3::origin(), Vector3::unit_z(), Rad(2.0 * PI))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_box_topology() {
        let solid = make_box(1.0, 2.0, 3.0);

        let boundaries = solid.boundaries();
        assert_eq!(boundaries.len(), 1, "Box should have 1 shell");

        let shell = &boundaries[0];
        let faces: Vec<_> = shell.face_iter().collect();

        // Deduplicate edges and vertices
        let mut edge_ids = std::collections::HashSet::new();
        for edge in shell.edge_iter() {
            edge_ids.insert(edge.id());
        }
        let mut vert_ids = std::collections::HashSet::new();
        for v in shell.vertex_iter() {
            vert_ids.insert(v.id());
        }

        assert_eq!(faces.len(), 6, "Box should have 6 faces");
        assert_eq!(edge_ids.len(), 12, "Box should have 12 edges");
        assert_eq!(vert_ids.len(), 8, "Box should have 8 vertices");

        // Euler's formula: V - E + F = 2
        let v = vert_ids.len() as i64;
        let e = edge_ids.len() as i64;
        let f = faces.len() as i64;
        assert_eq!(v - e + f, 2, "Euler formula must hold");
    }

    #[test]
    fn test_make_cylinder_topology() {
        let solid = make_cylinder(1.0, 2.0);

        let boundaries = solid.boundaries();
        assert_eq!(boundaries.len(), 1, "Cylinder should have 1 shell");

        let shell = &boundaries[0];
        let faces: Vec<_> = shell.face_iter().collect();

        // Cylinder: truck may produce more faces depending on internal sweep
        // division. At minimum: top + bottom + side(s).
        assert!(faces.len() >= 3, "Cylinder should have at least 3 faces");
    }

    #[test]
    fn test_make_box_dimensions() {
        let solid = make_box(2.0, 3.0, 4.0);
        let boundaries = solid.boundaries();
        let shell = &boundaries[0];

        let mut min = [f64::MAX; 3];
        let mut max = [f64::MIN; 3];
        for v in shell.vertex_iter() {
            let p = v.point();
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }

        let eps = 1e-10;
        assert!((max[0] - min[0] - 2.0).abs() < eps, "Width should be 2");
        assert!((max[1] - min[1] - 3.0).abs() < eps, "Height should be 3");
        assert!((max[2] - min[2] - 4.0).abs() < eps, "Depth should be 4");
    }
}
