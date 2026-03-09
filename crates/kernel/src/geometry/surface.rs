//! Analytic surface types (stubs for future NURBS implementation).

use super::point::{Point3, Vector3};

/// Geometry attached to a B-Rep face.
#[derive(Debug, Clone)]
pub enum SurfaceGeom {
    Planar(Plane),
    Cylindrical(Cylinder),
}

/// An infinite plane.
#[derive(Debug, Clone)]
pub struct Plane {
    pub origin: Point3,
    pub normal: Vector3,
}

/// A cylinder (infinite extent along axis).
#[derive(Debug, Clone)]
pub struct Cylinder {
    pub origin: Point3,
    pub axis: Vector3,
    pub radius: f64,
}

/// A cone.
#[derive(Debug, Clone)]
pub struct Cone {
    pub apex: Point3,
    pub axis: Vector3,
    pub half_angle: f64,
}

/// A sphere.
#[derive(Debug, Clone)]
pub struct Sphere {
    pub center: Point3,
    pub radius: f64,
}

/// A torus.
#[derive(Debug, Clone)]
pub struct Torus {
    pub center: Point3,
    pub axis: Vector3,
    pub major_radius: f64,
    pub minor_radius: f64,
}
