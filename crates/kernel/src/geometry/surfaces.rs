use serde::{Deserialize, Serialize};

use super::nurbs::NurbsSurface;
use super::point::Point3d;
use super::vector::Vec3;

/// All surface types supported by the kernel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Surface {
    Plane(Plane),
    Cylinder(Cylinder),
    Cone(Cone),
    Sphere(Sphere),
    Torus(Torus),
    Nurbs(NurbsSurface),
}

/// An infinite plane.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Plane {
    pub origin: Point3d,
    pub normal: Vec3,
    pub u_axis: Vec3,
    pub v_axis: Vec3,
}

impl Plane {
    pub fn new(origin: Point3d, normal: Vec3) -> Self {
        let normal = normal.normalize();
        let u_axis = if normal.x.abs() < 0.9 {
            Vec3::X.cross(&normal).normalize()
        } else {
            Vec3::Y.cross(&normal).normalize()
        };
        let v_axis = normal.cross(&u_axis);
        Self {
            origin,
            normal,
            u_axis,
            v_axis,
        }
    }

    pub fn xy() -> Self {
        Self {
            origin: Point3d::ORIGIN,
            normal: Vec3::Z,
            u_axis: Vec3::X,
            v_axis: Vec3::Y,
        }
    }

    pub fn xz() -> Self {
        Self {
            origin: Point3d::ORIGIN,
            normal: Vec3::Y,
            u_axis: Vec3::X,
            v_axis: Vec3::Z,
        }
    }

    pub fn yz() -> Self {
        Self {
            origin: Point3d::ORIGIN,
            normal: Vec3::X,
            u_axis: Vec3::Y,
            v_axis: Vec3::Z,
        }
    }

    pub fn evaluate(&self, u: f64, v: f64) -> Point3d {
        self.origin + self.u_axis * u + self.v_axis * v
    }

    pub fn normal_at(&self, _u: f64, _v: f64) -> Vec3 {
        self.normal
    }

    pub fn distance_to_point(&self, p: &Point3d) -> f64 {
        let v = *p - self.origin;
        v.dot(&self.normal)
    }

    pub fn project_point(&self, p: &Point3d) -> Point3d {
        let dist = self.distance_to_point(p);
        *p - self.normal * dist
    }

    /// Get (u, v) parameters for a point projected onto the plane.
    pub fn parameters_of(&self, p: &Point3d) -> (f64, f64) {
        let v = *p - self.origin;
        (v.dot(&self.u_axis), v.dot(&self.v_axis))
    }
}

/// A cylinder surface (infinite along axis).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Cylinder {
    pub origin: Point3d,
    pub axis: Vec3,
    pub radius: f64,
    pub ref_dir: Vec3,
}

impl Cylinder {
    pub fn new(origin: Point3d, axis: Vec3, radius: f64) -> Self {
        let axis = axis.normalize();
        let ref_dir = if axis.x.abs() < 0.9 {
            Vec3::X.cross(&axis).normalize()
        } else {
            Vec3::Y.cross(&axis).normalize()
        };
        Self {
            origin,
            axis,
            radius,
            ref_dir,
        }
    }

    /// Evaluate at (u=angle, v=height along axis).
    pub fn evaluate(&self, u: f64, v: f64) -> Point3d {
        let y_dir = self.axis.cross(&self.ref_dir);
        self.origin
            + self.ref_dir * (self.radius * u.cos())
            + y_dir * (self.radius * u.sin())
            + self.axis * v
    }

    pub fn normal_at(&self, u: f64, _v: f64) -> Vec3 {
        let y_dir = self.axis.cross(&self.ref_dir);
        (self.ref_dir * u.cos() + y_dir * u.sin()).normalize()
    }
}

/// A cone surface.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Cone {
    pub apex: Point3d,
    pub axis: Vec3,
    pub half_angle: f64,
    pub ref_dir: Vec3,
}

impl Cone {
    pub fn new(apex: Point3d, axis: Vec3, half_angle: f64) -> Self {
        let axis = axis.normalize();
        let ref_dir = if axis.x.abs() < 0.9 {
            Vec3::X.cross(&axis).normalize()
        } else {
            Vec3::Y.cross(&axis).normalize()
        };
        Self {
            apex,
            axis,
            half_angle,
            ref_dir,
        }
    }

    /// Evaluate at (u=angle, v=distance from apex along axis).
    pub fn evaluate(&self, u: f64, v: f64) -> Point3d {
        let y_dir = self.axis.cross(&self.ref_dir);
        let r = v * self.half_angle.tan();
        self.apex + self.axis * v + self.ref_dir * (r * u.cos()) + y_dir * (r * u.sin())
    }

    pub fn normal_at(&self, u: f64, _v: f64) -> Vec3 {
        let y_dir = self.axis.cross(&self.ref_dir);
        let cos_a = self.half_angle.cos();
        let sin_a = self.half_angle.sin();
        let radial = self.ref_dir * u.cos() + y_dir * u.sin();
        (radial * cos_a - self.axis * sin_a).normalize()
    }
}

/// A sphere surface.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Sphere {
    pub center: Point3d,
    pub radius: f64,
}

impl Sphere {
    pub fn new(center: Point3d, radius: f64) -> Self {
        Self { center, radius }
    }

    /// Evaluate at (u=longitude 0..2PI, v=latitude -PI/2..PI/2).
    pub fn evaluate(&self, u: f64, v: f64) -> Point3d {
        let cos_v = v.cos();
        Point3d::new(
            self.center.x + self.radius * cos_v * u.cos(),
            self.center.y + self.radius * cos_v * u.sin(),
            self.center.z + self.radius * v.sin(),
        )
    }

    pub fn normal_at(&self, u: f64, v: f64) -> Vec3 {
        let p = self.evaluate(u, v);
        (p - self.center).normalize()
    }
}

/// A torus surface.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Torus {
    pub center: Point3d,
    pub axis: Vec3,
    pub major_radius: f64,
    pub minor_radius: f64,
}

impl Torus {
    pub fn new(center: Point3d, axis: Vec3, major_radius: f64, minor_radius: f64) -> Self {
        Self {
            center,
            axis: axis.normalize(),
            major_radius,
            minor_radius,
        }
    }

    /// Evaluate at (u=major angle, v=minor angle).
    pub fn evaluate(&self, u: f64, v: f64) -> Point3d {
        let ref_dir = if self.axis.x.abs() < 0.9 {
            Vec3::X.cross(&self.axis).normalize()
        } else {
            Vec3::Y.cross(&self.axis).normalize()
        };
        let y_dir = self.axis.cross(&ref_dir);

        let ring_center = self.center + ref_dir * (self.major_radius * u.cos()) + y_dir * (self.major_radius * u.sin());
        let radial = (ring_center - self.center).normalized().unwrap_or(ref_dir);

        ring_center + radial * (self.minor_radius * v.cos()) + self.axis * (self.minor_radius * v.sin())
    }

    pub fn normal_at(&self, u: f64, v: f64) -> Vec3 {
        let ref_dir = if self.axis.x.abs() < 0.9 {
            Vec3::X.cross(&self.axis).normalize()
        } else {
            Vec3::Y.cross(&self.axis).normalize()
        };
        let y_dir = self.axis.cross(&ref_dir);
        let ring_center = self.center + ref_dir * (self.major_radius * u.cos()) + y_dir * (self.major_radius * u.sin());

        let p = self.evaluate(u, v);
        (p - ring_center).normalized().unwrap_or(Vec3::Z)
    }
}

impl Surface {
    pub fn evaluate(&self, u: f64, v: f64) -> Point3d {
        match self {
            Surface::Plane(p) => p.evaluate(u, v),
            Surface::Cylinder(c) => c.evaluate(u, v),
            Surface::Cone(c) => c.evaluate(u, v),
            Surface::Sphere(s) => s.evaluate(u, v),
            Surface::Torus(t) => t.evaluate(u, v),
            Surface::Nurbs(n) => n.evaluate(u, v),
        }
    }

    pub fn normal_at(&self, u: f64, v: f64) -> Vec3 {
        match self {
            Surface::Plane(p) => p.normal_at(u, v),
            Surface::Cylinder(c) => c.normal_at(u, v),
            Surface::Cone(c) => c.normal_at(u, v),
            Surface::Sphere(s) => s.normal_at(u, v),
            Surface::Torus(t) => t.normal_at(u, v),
            Surface::Nurbs(n) => n.normal(u, v),
        }
    }

    pub fn surface_type_name(&self) -> &'static str {
        match self {
            Surface::Plane(_) => "Plane",
            Surface::Cylinder(_) => "Cylinder",
            Surface::Cone(_) => "Cone",
            Surface::Sphere(_) => "Sphere",
            Surface::Torus(_) => "Torus",
            Surface::Nurbs(_) => "Nurbs",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    #[test]
    fn test_plane_evaluate() {
        let p = Plane::xy();
        let pt = p.evaluate(3.0, 4.0);
        assert!((pt.x - 3.0).abs() < 1e-12);
        assert!((pt.y - 4.0).abs() < 1e-12);
        assert!(pt.z.abs() < 1e-12);
    }

    #[test]
    fn test_plane_distance() {
        let p = Plane::xy();
        let pt = Point3d::new(0.0, 0.0, 5.0);
        assert!((p.distance_to_point(&pt) - 5.0).abs() < 1e-12);
    }

    #[test]
    fn test_plane_project() {
        let p = Plane::xy();
        let pt = Point3d::new(1.0, 2.0, 3.0);
        let proj = p.project_point(&pt);
        assert!((proj.x - 1.0).abs() < 1e-12);
        assert!((proj.y - 2.0).abs() < 1e-12);
        assert!(proj.z.abs() < 1e-12);
    }

    #[test]
    fn test_cylinder_on_surface() {
        let c = Cylinder::new(Point3d::ORIGIN, Vec3::Z, 5.0);
        for i in 0..20 {
            let u = 2.0 * PI * (i as f64 / 20.0);
            let p = c.evaluate(u, 0.0);
            let r = (p.x * p.x + p.y * p.y).sqrt();
            assert!((r - 5.0).abs() < 1e-10);
        }
    }

    #[test]
    fn test_sphere_on_surface() {
        let s = Sphere::new(Point3d::ORIGIN, 3.0);
        for i in 0..10 {
            for j in 0..10 {
                let u = 2.0 * PI * (i as f64 / 10.0);
                let v = -FRAC_PI_2 + PI * (j as f64 / 10.0);
                let p = s.evaluate(u, v);
                let r = p.distance_to(&Point3d::ORIGIN);
                assert!((r - 3.0).abs() < 1e-10, "r={} at u={}, v={}", r, u, v);
            }
        }
    }

    #[test]
    fn test_sphere_normal_is_outward() {
        let s = Sphere::new(Point3d::ORIGIN, 2.0);
        let u = 0.5;
        let v = 0.3;
        let p = s.evaluate(u, v);
        let n = s.normal_at(u, v);
        let expected_dir = (p - s.center).normalize();
        assert!((n.x - expected_dir.x).abs() < 1e-10);
        assert!((n.y - expected_dir.y).abs() < 1e-10);
        assert!((n.z - expected_dir.z).abs() < 1e-10);
    }

    #[test]
    fn test_torus_on_surface() {
        let t = Torus::new(Point3d::ORIGIN, Vec3::Z, 10.0, 3.0);
        // A point on the torus should be between (major-minor) and (major+minor) from center
        for i in 0..10 {
            let u = 2.0 * PI * (i as f64 / 10.0);
            let v = 0.0;
            let p = t.evaluate(u, v);
            let dist_xy = (p.x * p.x + p.y * p.y).sqrt();
            assert!(
                (dist_xy - 13.0).abs() < 1e-8 || (dist_xy - 7.0).abs() < 1e-8,
                "dist_xy={} at u={}, v={}",
                dist_xy,
                u,
                v
            );
        }
    }
}
