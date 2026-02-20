//! Higher-level primitive builders on top of truck's sweep API.
//!
//! truck has no built-in box/cylinder/sphere — everything is successive sweeps.

use std::f64::consts::PI;
use truck_modeling::builder;
use truck_modeling::topology::Solid;
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
    let wire = builder::rsweep(&v, Point3::origin(), Vector3::unit_z(), Rad(2.0 * PI), 3);
    let face = builder::try_attach_plane(&[wire]).expect("Failed to create circular face");
    builder::tsweep(&face, Vector3::new(0.0, 0.0, height))
}

/// Create a sphere solid: semicircle arc → cone sweep 2π.
/// Centered at origin.
pub fn make_sphere(radius: f64) -> Solid {
    use truck_modeling::topology::{Shell, Solid as TruckSolid};

    // Create a semicircle arc from north pole (0,0,r) through equator (r,0,0)
    // to south pole (0,0,-r) by rotating (0,0,r) around Y axis by PI.
    // Both endpoints lie on the Z axis (the revolution axis).
    let v_top = builder::vertex(Point3::new(0.0, 0.0, radius));
    let arc_wire = builder::rsweep(&v_top, Point3::origin(), Vector3::unit_y(), Rad(PI), 3);

    // Use builder::cone which handles degenerate edges at vertices on the
    // revolution axis (the poles). This produces correct topology (V-E+F=2).
    let shell: Shell = builder::cone(&arc_wire, Vector3::unit_z(), Rad(2.0 * PI), 3);

    TruckSolid::new(vec![shell])
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
    fn test_euler_formula_cylinder() {
        let solid = make_cylinder(1.0, 2.0);
        let boundaries = solid.boundaries();
        let shell = &boundaries[0];

        let faces: Vec<_> = shell.face_iter().collect();
        let mut edge_ids = std::collections::HashSet::new();
        for edge in shell.edge_iter() {
            edge_ids.insert(edge.id());
        }
        let mut vert_ids = std::collections::HashSet::new();
        for v in shell.vertex_iter() {
            vert_ids.insert(v.id());
        }

        let v = vert_ids.len() as i64;
        let e = edge_ids.len() as i64;
        let f = faces.len() as i64;

        assert_eq!(
            v - e + f,
            2,
            "Euler formula V-E+F=2 must hold for cylinder (V={}, E={}, F={})",
            v,
            e,
            f
        );
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

    // ── Coverage: make_sphere ────────────────────────────────────────

    /// Sphere has valid topology: single shell.
    #[test]
    fn test_make_sphere_topology() {
        let solid = make_sphere(1.0);
        let boundaries = solid.boundaries();
        assert_eq!(boundaries.len(), 1, "Sphere should have 1 shell");

        let shell = &boundaries[0];
        let faces: Vec<_> = shell.face_iter().collect();
        // Sphere from revolving a semicircle should have multiple faces
        assert!(
            faces.len() >= 2,
            "Sphere should have at least 2 faces, got {}",
            faces.len()
        );
    }

    /// Euler's formula V-E+F=2 holds for a sphere.
    #[test]
    fn test_euler_formula_sphere() {
        let solid = make_sphere(1.0);
        let boundaries = solid.boundaries();
        let shell = &boundaries[0];

        let faces: Vec<_> = shell.face_iter().collect();
        let mut edge_ids = std::collections::HashSet::new();
        for edge in shell.edge_iter() {
            edge_ids.insert(edge.id());
        }
        let mut vert_ids = std::collections::HashSet::new();
        for v in shell.vertex_iter() {
            vert_ids.insert(v.id());
        }

        let v = vert_ids.len() as i64;
        let e = edge_ids.len() as i64;
        let f = faces.len() as i64;

        assert_eq!(
            v - e + f,
            2,
            "Euler formula V-E+F=2 must hold for sphere (V={}, E={}, F={})",
            v,
            e,
            f
        );
    }

    /// Sphere bounding box is approximately [-r, r] in all axes.
    /// Uses tessellated mesh since sphere vertices (control points) don't
    /// span the full extent of the curved surface.
    #[test]
    fn test_make_sphere_dimensions() {
        use crate::truck_kernel::TruckKernel;

        let radius = 2.5;
        let mut kernel = TruckKernel::new();
        let solid = make_sphere(radius);
        let handle = kernel.store_solid(solid);

        let mesh = crate::traits::Kernel::tessellate(&mut kernel, &handle, 0.05).unwrap();

        let mut min = [f64::MAX; 3];
        let mut max = [f64::MIN; 3];
        for chunk in mesh.vertices.chunks(3) {
            for i in 0..3 {
                min[i] = min[i].min(chunk[i] as f64);
                max[i] = max[i].max(chunk[i] as f64);
            }
        }

        let eps = 0.15; // Tolerance for tessellation approximation
        for i in 0..3 {
            assert!(
                (min[i] + radius).abs() < eps,
                "Sphere min[{}] should be ~{}, got {}",
                i,
                -radius,
                min[i]
            );
            assert!(
                (max[i] - radius).abs() < eps,
                "Sphere max[{}] should be ~{}, got {}",
                i,
                radius,
                max[i]
            );
        }
    }

    /// Sphere can be tessellated successfully.
    #[test]
    fn test_make_sphere_tessellation() {
        use crate::truck_kernel::TruckKernel;

        let mut kernel = TruckKernel::new();
        let solid = make_sphere(1.0);
        let handle = kernel.store_solid(solid);

        let mesh = crate::traits::Kernel::tessellate(&mut kernel, &handle, 0.1).unwrap();
        assert!(
            !mesh.vertices.is_empty(),
            "Sphere mesh should have vertices"
        );
        assert!(!mesh.indices.is_empty(), "Sphere mesh should have indices");
        assert!(!mesh.normals.is_empty(), "Sphere mesh should have normals");
        assert!(
            !mesh.face_ranges.is_empty(),
            "Sphere should have face ranges"
        );

        // All vertices should be finite
        for (i, v) in mesh.vertices.iter().enumerate() {
            assert!(v.is_finite(), "Sphere vertex[{}] = {} is not finite", i, v);
        }
    }

    /// Sphere edges can be extracted.
    #[test]
    fn test_make_sphere_edges() {
        use crate::truck_kernel::TruckKernel;

        let mut kernel = TruckKernel::new();
        let solid = make_sphere(1.0);
        let handle = kernel.store_solid(solid);

        let edges = crate::traits::Kernel::extract_edges(&mut kernel, &handle, 0.1).unwrap();
        assert!(
            !edges.edge_ranges.is_empty(),
            "Sphere should have edge ranges"
        );
    }

    /// make_cylinder with different radius/height values.
    /// Note: truck only stores wire vertices (one point on the circle),
    /// so we verify height via z-coordinates and verify the solid is valid.
    #[test]
    fn test_make_cylinder_dimensions() {
        let solid = make_cylinder(2.0, 5.0);
        let boundaries = solid.boundaries();
        let shell = &boundaries[0];

        let mut min_z = f64::MAX;
        let mut max_z = f64::MIN;
        for v in shell.vertex_iter() {
            let p = v.point();
            min_z = min_z.min(p[2]);
            max_z = max_z.max(p[2]);
        }

        let eps = 0.01;
        assert!(
            min_z.abs() < eps,
            "Cylinder bottom z should be ~0, got {}",
            min_z
        );
        assert!(
            (max_z - 5.0).abs() < eps,
            "Cylinder top z should be ~5, got {}",
            max_z
        );

        // Verify it's a valid single-shell solid with expected face count
        let faces: Vec<_> = shell.face_iter().collect();
        assert!(faces.len() >= 3, "Cylinder should have at least 3 faces");
    }
}
