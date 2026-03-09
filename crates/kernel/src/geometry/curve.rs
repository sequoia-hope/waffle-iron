//! 3D curve types (stubs for future NURBS implementation).

use super::point::{Point3, Vector3};

/// Geometry attached to a B-Rep edge.
#[derive(Debug, Clone)]
pub enum CurveGeom {
    Linear(Line3D),
    Circular(Circle3D),
    Arc(Arc3D),
}

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

/// A partial circular arc in 3D space.
#[derive(Debug, Clone)]
pub struct Arc3D {
    pub center: Point3,
    pub normal: Vector3,
    pub radius: f64,
    pub start_point: Point3,
    pub sweep_angle: f64,
}
