//! 3D curve types (stubs for future NURBS implementation).

use super::point::{Point3, Vector3};

/// A line in 3D space.
#[derive(Debug, Clone)]
pub struct Line3D {
    pub origin: Point3,
    pub direction: Vector3,
}

/// A circle in 3D space.
#[derive(Debug, Clone)]
pub struct Circle3D {
    pub center: Point3,
    pub normal: Vector3,
    pub radius: f64,
}
