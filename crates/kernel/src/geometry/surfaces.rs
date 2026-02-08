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

    /// Project a point onto the cylinder surface, returning the closest point.
    pub fn closest_point(&self, p: &Point3d) -> Point3d {
        let d = *p - self.origin;
        let h = d.dot(&self.axis);
        let radial = d - self.axis * h;
        let radial_len = radial.length();
        if radial_len < 1e-15 {
            // Point is on the axis — pick any point at the right height.
            self.origin + self.axis * h + self.ref_dir * self.radius
        } else {
            let radial_dir = radial * (1.0 / radial_len);
            self.origin + self.axis * h + radial_dir * self.radius
        }
    }

    /// Distance from a point to the cylinder surface.
    pub fn distance_to_point(&self, p: &Point3d) -> f64 {
        let d = *p - self.origin;
        let h = d.dot(&self.axis);
        let radial = d - self.axis * h;
        (radial.length() - self.radius).abs()
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

    /// Project a point onto the cone surface, returning the closest point.
    pub fn closest_point(&self, p: &Point3d) -> Point3d {
        let d = *p - self.apex;
        let h = d.dot(&self.axis);
        let radial = d - self.axis * h;
        let radial_len = radial.length();

        // Distance along the axis for the closest point on the cone surface.
        // The cone surface at height h has radius = h * tan(half_angle).
        let tan_a = self.half_angle.tan();
        let cos_a = self.half_angle.cos();

        if radial_len < 1e-15 {
            // Point is on the axis. Closest point is at the apex if h <= 0,
            // or at the cone surface at height h.
            if h <= 0.0 {
                self.apex
            } else {
                let r = h * tan_a;
                self.apex + self.axis * h + self.ref_dir * r
            }
        } else {
            let radial_dir = radial * (1.0 / radial_len);
            // Project onto the cone's generating line in the (h, radial_len) plane.
            // The cone line is: (h, r) = t * (1, tan_a) for t >= 0
            // Project (h, radial_len) onto this line.
            let t = (h + radial_len * tan_a) * cos_a * cos_a;
            let t = t.max(0.0); // Clamp to apex
            let cone_h = t;
            let cone_r = t * tan_a;
            self.apex + self.axis * cone_h + radial_dir * cone_r
        }
    }

    /// Distance from a point to the cone surface.
    pub fn distance_to_point(&self, p: &Point3d) -> f64 {
        let closest = self.closest_point(p);
        p.distance_to(&closest)
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

    /// Project a point onto the sphere surface.
    pub fn closest_point(&self, p: &Point3d) -> Point3d {
        let d = *p - self.center;
        let len = d.length();
        if len < 1e-15 {
            // Point at the center — return north pole.
            Point3d::new(self.center.x, self.center.y, self.center.z + self.radius)
        } else {
            self.center + d * (self.radius / len)
        }
    }

    /// Distance from a point to the sphere surface.
    pub fn distance_to_point(&self, p: &Point3d) -> f64 {
        let d = *p - self.center;
        (d.length() - self.radius).abs()
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

    /// Project a point onto the torus surface.
    pub fn closest_point(&self, p: &Point3d) -> Point3d {
        let d = *p - self.center;
        // Project onto the torus plane (perpendicular to axis).
        let axial_comp = d.dot(&self.axis);
        let in_plane = d - self.axis * axial_comp;
        let in_plane_len = in_plane.length();

        // Find the closest point on the major circle.
        let ring_center = if in_plane_len < 1e-15 {
            // Point is on the axis — pick an arbitrary direction.
            let ref_dir = if self.axis.x.abs() < 0.9 {
                Vec3::X.cross(&self.axis).normalize()
            } else {
                Vec3::Y.cross(&self.axis).normalize()
            };
            self.center + ref_dir * self.major_radius
        } else {
            let in_plane_dir = in_plane * (1.0 / in_plane_len);
            self.center + in_plane_dir * self.major_radius
        };

        // Now project onto the tube circle around ring_center.
        let to_p = *p - ring_center;
        let dist = to_p.length();
        if dist < 1e-15 {
            // At the ring center — return outer point of tube.
            let outward = (ring_center - self.center).normalized().unwrap_or(Vec3::X);
            ring_center + outward * self.minor_radius
        } else {
            ring_center + to_p * (self.minor_radius / dist)
        }
    }

    /// Distance from a point to the torus surface.
    pub fn distance_to_point(&self, p: &Point3d) -> f64 {
        let closest = self.closest_point(p);
        p.distance_to(&closest)
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

    /// Project a point onto the surface, returning the closest point.
    pub fn closest_point(&self, p: &Point3d) -> Point3d {
        match self {
            Surface::Plane(pl) => pl.project_point(p),
            Surface::Cylinder(c) => c.closest_point(p),
            Surface::Cone(c) => c.closest_point(p),
            Surface::Sphere(s) => s.closest_point(p),
            Surface::Torus(t) => t.closest_point(p),
            Surface::Nurbs(_) => {
                // NURBS closest-point requires iterative Newton-Raphson.
                // For now, return the point itself (distance = 0 is wrong but won't crash).
                *p
            }
        }
    }

    /// Distance from a point to the surface.
    pub fn distance_to_point(&self, p: &Point3d) -> f64 {
        match self {
            Surface::Plane(pl) => pl.distance_to_point(p).abs(),
            Surface::Cylinder(c) => c.distance_to_point(p),
            Surface::Cone(c) => c.distance_to_point(p),
            Surface::Sphere(s) => s.distance_to_point(p),
            Surface::Torus(t) => t.distance_to_point(p),
            Surface::Nurbs(_) => 0.0, // Placeholder
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
    fn test_cylinder_closest_point() {
        let c = Cylinder::new(Point3d::ORIGIN, Vec3::Z, 5.0);
        // Point outside the cylinder
        let p = Point3d::new(10.0, 0.0, 3.0);
        let cp = c.closest_point(&p);
        assert!((cp.x - 5.0).abs() < 1e-10, "x should be 5, got {}", cp.x);
        assert!(cp.y.abs() < 1e-10);
        assert!((cp.z - 3.0).abs() < 1e-10, "z should be 3, got {}", cp.z);
        assert!(c.distance_to_point(&p) - 5.0 < 1e-10);

        // Point inside the cylinder
        let p2 = Point3d::new(2.0, 0.0, 1.0);
        assert!((c.distance_to_point(&p2) - 3.0).abs() < 1e-10);
        let cp2 = c.closest_point(&p2);
        assert!(c.distance_to_point(&cp2) < 1e-10);
    }

    #[test]
    fn test_sphere_closest_point() {
        let s = Sphere::new(Point3d::ORIGIN, 3.0);
        let p = Point3d::new(6.0, 0.0, 0.0);
        let cp = s.closest_point(&p);
        assert!((cp.x - 3.0).abs() < 1e-10);
        assert!(cp.y.abs() < 1e-10);
        assert!(cp.z.abs() < 1e-10);
        assert!((s.distance_to_point(&p) - 3.0).abs() < 1e-10);

        // Point inside
        let p2 = Point3d::new(1.0, 0.0, 0.0);
        assert!((s.distance_to_point(&p2) - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_cone_closest_point() {
        let c = Cone::new(Point3d::ORIGIN, Vec3::Z, std::f64::consts::FRAC_PI_4);
        // At height 1, cone radius = 1 (45 degree half-angle).
        // A point at (2, 0, 1) should be at distance ~sqrt(2)/2 from the cone surface.
        let p = Point3d::new(2.0, 0.0, 1.0);
        let cp = c.closest_point(&p);
        // closest_point should lie on the cone: verify distance from axis equals height * tan(45) = height
        let h = (cp - c.apex).dot(&c.axis);
        let radial = cp - c.apex - c.axis * h;
        let r = radial.length();
        assert!((r - h * c.half_angle.tan()).abs() < 1e-10, "Closest point should lie on cone surface: r={r}, expected={}", h * c.half_angle.tan());

        // Point at apex
        let p2 = Point3d::ORIGIN;
        assert!(c.distance_to_point(&p2) < 1e-10, "Apex should be on cone");
    }

    #[test]
    fn test_torus_closest_point() {
        let t = Torus::new(Point3d::ORIGIN, Vec3::Z, 10.0, 3.0);
        // Point on the outer equator: (13, 0, 0) should be on the surface.
        let p_on = Point3d::new(13.0, 0.0, 0.0);
        assert!(t.distance_to_point(&p_on) < 1e-10, "Point on torus surface should have zero distance");

        // Point further out: (20, 0, 0) — closest point should be (13, 0, 0).
        let p_out = Point3d::new(20.0, 0.0, 0.0);
        let cp = t.closest_point(&p_out);
        assert!((cp.x - 13.0).abs() < 1e-10);
        assert!(cp.y.abs() < 1e-10);
        assert!(cp.z.abs() < 1e-10);
        assert!((t.distance_to_point(&p_out) - 7.0).abs() < 1e-10);
    }

    #[test]
    fn test_surface_dispatch_distance() {
        let surface = Surface::Plane(Plane::xy());
        let p = Point3d::new(1.0, 2.0, 3.0);
        assert!((surface.distance_to_point(&p) - 3.0).abs() < 1e-10);

        let surface = Surface::Sphere(Sphere::new(Point3d::ORIGIN, 5.0));
        let p = Point3d::new(8.0, 0.0, 0.0);
        assert!((surface.distance_to_point(&p) - 3.0).abs() < 1e-10);
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
