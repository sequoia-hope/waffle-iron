//! Analytic surface types with evaluation methods.
//!
//! Each surface type supports parametric evaluation, normal computation,
//! point containment, and closest-point projection. These methods provide
//! a shared API that replaces ad-hoc surface math throughout the kernel.
//!
//! Parametrization conventions:
//! - Plane: (u, v) → origin + u·x_axis + v·y_axis (x_axis, y_axis derived from normal)
//! - Cylinder: (u, v) → origin + r·cos(u)·x + r·sin(u)·y + v·axis, u ∈ [0, 2π), v ∈ ℝ
//! - Cone: (u, v) → apex + v·(cos(u)·x + sin(u)·y)·tan(half_angle) + v·axis, u ∈ [0, 2π), v > 0
//! - Sphere: (u, v) → center + r·cos(v)·cos(u)·x + r·cos(v)·sin(u)·y + r·sin(v)·z, u ∈ [0,2π), v ∈ [-π/2, π/2]
//! - Torus: (u, v) → center + (R + r·cos(v))·cos(u)·x + (R + r·cos(v))·sin(u)·y + r·sin(v)·axis

use super::point::{Point3, Vector3};
use crate::units::{TAU_COINCIDENT, TAU_NORMALIZE};

/// Geometry attached to a B-Rep face.
#[derive(Debug, Clone)]
pub enum SurfaceGeom {
    Planar(Plane),
    Cylindrical(Cylinder),
    Conical(Cone),
    Spherical(Sphere),
    Toroidal(Torus),
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

// ── Helper: build a local frame from a single normal vector ─────────────

/// Given a unit normal, return (x_axis, y_axis) forming a right-handed frame.
fn make_frame(normal: Vector3) -> (Vector3, Vector3) {
    // Pick the coordinate axis least aligned with normal to avoid degeneracy
    let hint = if normal.x.abs() < crate::units::BASIS_AXIS_ALIGNMENT {
        Vector3::new(1.0, 0.0, 0.0)
    } else {
        Vector3::new(0.0, 1.0, 0.0)
    };
    let x = normal.cross(hint).normalized();
    let y = normal.cross(x);
    (x, y)
}

// ── SurfaceGeom dispatch methods ────────────────────────────────────────

impl SurfaceGeom {
    /// Evaluate the surface at parameters (u, v), returning a 3D point.
    pub fn evaluate(&self, u: f64, v: f64) -> Point3 {
        match self {
            SurfaceGeom::Planar(p) => p.evaluate(u, v),
            SurfaceGeom::Cylindrical(c) => c.evaluate(u, v),
            SurfaceGeom::Conical(c) => c.evaluate(u, v),
            SurfaceGeom::Spherical(s) => s.evaluate(u, v),
            SurfaceGeom::Toroidal(t) => t.evaluate(u, v),
        }
    }

    /// Compute the outward unit normal at parameters (u, v).
    pub fn normal_at(&self, u: f64, v: f64) -> Vector3 {
        match self {
            SurfaceGeom::Planar(p) => p.normal_at(u, v),
            SurfaceGeom::Cylindrical(c) => c.normal_at(u, v),
            SurfaceGeom::Conical(c) => c.normal_at(u, v),
            SurfaceGeom::Spherical(s) => s.normal_at(u, v),
            SurfaceGeom::Toroidal(t) => t.normal_at(u, v),
        }
    }

    /// Test whether a point lies on (within TAU_COINCIDENT of) this surface.
    pub fn contains_point(&self, pt: Point3) -> bool {
        match self {
            SurfaceGeom::Planar(p) => p.contains_point(pt),
            SurfaceGeom::Cylindrical(c) => c.contains_point(pt),
            SurfaceGeom::Conical(c) => c.contains_point(pt),
            SurfaceGeom::Spherical(s) => s.contains_point(pt),
            SurfaceGeom::Toroidal(t) => t.contains_point(pt),
        }
    }

    /// Project a point onto this surface, returning the closest point.
    pub fn project_point(&self, pt: Point3) -> Point3 {
        match self {
            SurfaceGeom::Planar(p) => p.project_point(pt),
            SurfaceGeom::Cylindrical(c) => c.project_point(pt),
            SurfaceGeom::Conical(c) => c.project_point(pt),
            SurfaceGeom::Spherical(s) => s.project_point(pt),
            SurfaceGeom::Toroidal(t) => t.project_point(pt),
        }
    }

    /// Returns true if this is a quadric surface (plane, cylinder, cone, sphere, torus).
    pub fn is_quadric(&self) -> bool {
        // All current variants are quadric; future BSpline would return false
        true
    }
}

// ── Plane evaluation ────────────────────────────────────────────────────

impl Plane {
    pub fn evaluate(&self, u: f64, v: f64) -> Point3 {
        let (x, y) = make_frame(self.normal);
        Point3::new(
            self.origin.x + u * x.x + v * y.x,
            self.origin.y + u * x.y + v * y.y,
            self.origin.z + u * x.z + v * y.z,
        )
    }

    pub fn normal_at(&self, _u: f64, _v: f64) -> Vector3 {
        self.normal
    }

    pub fn contains_point(&self, pt: Point3) -> bool {
        let d = (pt.x - self.origin.x) * self.normal.x
            + (pt.y - self.origin.y) * self.normal.y
            + (pt.z - self.origin.z) * self.normal.z;
        d.abs() < TAU_COINCIDENT
    }

    pub fn project_point(&self, pt: Point3) -> Point3 {
        let d = (pt.x - self.origin.x) * self.normal.x
            + (pt.y - self.origin.y) * self.normal.y
            + (pt.z - self.origin.z) * self.normal.z;
        Point3::new(
            pt.x - d * self.normal.x,
            pt.y - d * self.normal.y,
            pt.z - d * self.normal.z,
        )
    }
}

// ── Cylinder evaluation ─────────────────────────────────────────────────

impl Cylinder {
    pub fn evaluate(&self, u: f64, v: f64) -> Point3 {
        let (x, y) = make_frame(self.axis);
        Point3::new(
            self.origin.x
                + self.radius * u.cos() * x.x
                + self.radius * u.sin() * y.x
                + v * self.axis.x,
            self.origin.y
                + self.radius * u.cos() * x.y
                + self.radius * u.sin() * y.y
                + v * self.axis.y,
            self.origin.z
                + self.radius * u.cos() * x.z
                + self.radius * u.sin() * y.z
                + v * self.axis.z,
        )
    }

    pub fn normal_at(&self, u: f64, _v: f64) -> Vector3 {
        let (x, y) = make_frame(self.axis);
        Vector3::new(
            u.cos() * x.x + u.sin() * y.x,
            u.cos() * x.y + u.sin() * y.y,
            u.cos() * x.z + u.sin() * y.z,
        )
    }

    pub fn contains_point(&self, pt: Point3) -> bool {
        let dp = Vector3::new(
            pt.x - self.origin.x,
            pt.y - self.origin.y,
            pt.z - self.origin.z,
        );
        let axial = dp.dot(self.axis);
        let radial_sq = dp.dot(dp) - axial * axial;
        (radial_sq.sqrt() - self.radius).abs() < TAU_COINCIDENT
    }

    pub fn project_point(&self, pt: Point3) -> Point3 {
        let dp = Vector3::new(
            pt.x - self.origin.x,
            pt.y - self.origin.y,
            pt.z - self.origin.z,
        );
        let axial = dp.dot(self.axis);
        let radial = Vector3::new(
            dp.x - axial * self.axis.x,
            dp.y - axial * self.axis.y,
            dp.z - axial * self.axis.z,
        );
        let r_len = radial.length();
        if r_len < TAU_NORMALIZE {
            // Point is on the axis — pick an arbitrary direction
            let (x, _) = make_frame(self.axis);
            return Point3::new(
                self.origin.x + axial * self.axis.x + self.radius * x.x,
                self.origin.y + axial * self.axis.y + self.radius * x.y,
                self.origin.z + axial * self.axis.z + self.radius * x.z,
            );
        }
        let scale = self.radius / r_len;
        Point3::new(
            self.origin.x + axial * self.axis.x + scale * radial.x,
            self.origin.y + axial * self.axis.y + scale * radial.y,
            self.origin.z + axial * self.axis.z + scale * radial.z,
        )
    }
}

// ── Cone evaluation ─────────────────────────────────────────────────────

impl Cone {
    pub fn evaluate(&self, u: f64, v: f64) -> Point3 {
        let (x, y) = make_frame(self.axis);
        let r = v * self.half_angle.tan();
        Point3::new(
            self.apex.x + v * self.axis.x + r * (u.cos() * x.x + u.sin() * y.x),
            self.apex.y + v * self.axis.y + r * (u.cos() * x.y + u.sin() * y.y),
            self.apex.z + v * self.axis.z + r * (u.cos() * x.z + u.sin() * y.z),
        )
    }

    pub fn normal_at(&self, u: f64, _v: f64) -> Vector3 {
        let (x, y) = make_frame(self.axis);
        let cos_ha = self.half_angle.cos();
        let sin_ha = self.half_angle.sin();
        // Outward normal: cos(half_angle) * radial - sin(half_angle) * axis
        let radial_x = u.cos() * x.x + u.sin() * y.x;
        let radial_y = u.cos() * x.y + u.sin() * y.y;
        let radial_z = u.cos() * x.z + u.sin() * y.z;
        Vector3::new(
            cos_ha * radial_x - sin_ha * self.axis.x,
            cos_ha * radial_y - sin_ha * self.axis.y,
            cos_ha * radial_z - sin_ha * self.axis.z,
        )
        .normalized()
    }

    pub fn contains_point(&self, pt: Point3) -> bool {
        let dp = Vector3::new(pt.x - self.apex.x, pt.y - self.apex.y, pt.z - self.apex.z);
        let h = dp.dot(self.axis);
        if h < TAU_NORMALIZE {
            return false; // Behind or at apex
        }
        let radial_sq = dp.dot(dp) - h * h;
        let expected_r = h * self.half_angle.tan();
        (radial_sq.sqrt() - expected_r).abs() < TAU_COINCIDENT
    }

    pub fn project_point(&self, pt: Point3) -> Point3 {
        let dp = Vector3::new(pt.x - self.apex.x, pt.y - self.apex.y, pt.z - self.apex.z);
        let h = dp.dot(self.axis).max(TAU_NORMALIZE);
        let radial = Vector3::new(
            dp.x - h * self.axis.x,
            dp.y - h * self.axis.y,
            dp.z - h * self.axis.z,
        );
        let r_len = radial.length();
        let target_r = h * self.half_angle.tan();

        if r_len < TAU_NORMALIZE {
            let (x, _) = make_frame(self.axis);
            return Point3::new(
                self.apex.x + h * self.axis.x + target_r * x.x,
                self.apex.y + h * self.axis.y + target_r * x.y,
                self.apex.z + h * self.axis.z + target_r * x.z,
            );
        }
        let scale = target_r / r_len;
        Point3::new(
            self.apex.x + h * self.axis.x + scale * radial.x,
            self.apex.y + h * self.axis.y + scale * radial.y,
            self.apex.z + h * self.axis.z + scale * radial.z,
        )
    }
}

// ── Sphere evaluation ───────────────────────────────────────────────────

impl Sphere {
    pub fn evaluate(&self, u: f64, v: f64) -> Point3 {
        Point3::new(
            self.center.x + self.radius * v.cos() * u.cos(),
            self.center.y + self.radius * v.cos() * u.sin(),
            self.center.z + self.radius * v.sin(),
        )
    }

    pub fn normal_at(&self, u: f64, v: f64) -> Vector3 {
        Vector3::new(v.cos() * u.cos(), v.cos() * u.sin(), v.sin())
    }

    pub fn contains_point(&self, pt: Point3) -> bool {
        (pt.distance_to(self.center) - self.radius).abs() < TAU_COINCIDENT
    }

    pub fn project_point(&self, pt: Point3) -> Point3 {
        let d = Vector3::new(
            pt.x - self.center.x,
            pt.y - self.center.y,
            pt.z - self.center.z,
        );
        let len = d.length();
        if len < TAU_NORMALIZE {
            return Point3::new(self.center.x + self.radius, self.center.y, self.center.z);
        }
        let scale = self.radius / len;
        Point3::new(
            self.center.x + scale * d.x,
            self.center.y + scale * d.y,
            self.center.z + scale * d.z,
        )
    }
}

// ── Torus evaluation ────────────────────────────────────────────────────

impl Torus {
    pub fn evaluate(&self, u: f64, v: f64) -> Point3 {
        let (x, y) = make_frame(self.axis);
        let r = self.major_radius + self.minor_radius * v.cos();
        Point3::new(
            self.center.x
                + r * (u.cos() * x.x + u.sin() * y.x)
                + self.minor_radius * v.sin() * self.axis.x,
            self.center.y
                + r * (u.cos() * x.y + u.sin() * y.y)
                + self.minor_radius * v.sin() * self.axis.y,
            self.center.z
                + r * (u.cos() * x.z + u.sin() * y.z)
                + self.minor_radius * v.sin() * self.axis.z,
        )
    }

    pub fn normal_at(&self, u: f64, v: f64) -> Vector3 {
        let (x, y) = make_frame(self.axis);
        // Radial direction in the major circle plane
        let radial_x = u.cos() * x.x + u.sin() * y.x;
        let radial_y = u.cos() * x.y + u.sin() * y.y;
        let radial_z = u.cos() * x.z + u.sin() * y.z;
        // Outward normal from the tube surface
        Vector3::new(
            v.cos() * radial_x + v.sin() * self.axis.x,
            v.cos() * radial_y + v.sin() * self.axis.y,
            v.cos() * radial_z + v.sin() * self.axis.z,
        )
        .normalized()
    }

    pub fn contains_point(&self, pt: Point3) -> bool {
        let dp = Vector3::new(
            pt.x - self.center.x,
            pt.y - self.center.y,
            pt.z - self.center.z,
        );
        let axial = dp.dot(self.axis);
        let radial_sq = dp.dot(dp) - axial * axial;
        let radial = radial_sq.max(0.0).sqrt();
        let tube_dist_sq = (radial - self.major_radius).powi(2) + axial * axial;
        (tube_dist_sq.sqrt() - self.minor_radius).abs() < TAU_COINCIDENT
    }

    pub fn project_point(&self, pt: Point3) -> Point3 {
        let dp = Vector3::new(
            pt.x - self.center.x,
            pt.y - self.center.y,
            pt.z - self.center.z,
        );
        let axial = dp.dot(self.axis);
        let radial_vec = Vector3::new(
            dp.x - axial * self.axis.x,
            dp.y - axial * self.axis.y,
            dp.z - axial * self.axis.z,
        );
        let r_len = radial_vec.length();

        // Center of the tube circle closest to pt
        let (tube_cx, tube_cy, tube_cz) = if r_len < TAU_NORMALIZE {
            let (x, _) = make_frame(self.axis);
            (
                self.center.x + self.major_radius * x.x,
                self.center.y + self.major_radius * x.y,
                self.center.z + self.major_radius * x.z,
            )
        } else {
            let s = self.major_radius / r_len;
            (
                self.center.x + s * radial_vec.x,
                self.center.y + s * radial_vec.y,
                self.center.z + s * radial_vec.z,
            )
        };

        // Project from tube center onto the tube circle
        let to_pt = Vector3::new(pt.x - tube_cx, pt.y - tube_cy, pt.z - tube_cz);
        let dist = to_pt.length();
        if dist < TAU_NORMALIZE {
            // On the tube center — pick radial direction
            let s = if r_len < TAU_NORMALIZE {
                let (x, _) = make_frame(self.axis);
                x
            } else {
                Vector3::new(
                    radial_vec.x / r_len,
                    radial_vec.y / r_len,
                    radial_vec.z / r_len,
                )
            };
            return Point3::new(
                tube_cx + self.minor_radius * s.x,
                tube_cy + self.minor_radius * s.y,
                tube_cz + self.minor_radius * s.z,
            );
        }
        let scale = self.minor_radius / dist;
        Point3::new(
            tube_cx + scale * to_pt.x,
            tube_cy + scale * to_pt.y,
            tube_cz + scale * to_pt.z,
        )
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::MIN_FEATURE_SIZE;
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};

    const EPS: f64 = MIN_FEATURE_SIZE;

    #[test]
    fn plane_evaluate_origin() {
        let p = Plane {
            origin: Point3::new(1.0, 2.0, 3.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
        };
        let pt = p.evaluate(0.0, 0.0);
        assert!((pt.x - 1.0).abs() < EPS);
        assert!((pt.y - 2.0).abs() < EPS);
        assert!((pt.z - 3.0).abs() < EPS);

        // Verify non-origin (u,v) stays on plane (z = 3.0)
        let pt2 = p.evaluate(7.0, -3.0);
        assert!(
            (pt2.z - 3.0).abs() < EPS,
            "off-origin evaluate must stay on plane"
        );
    }

    #[test]
    fn plane_contains_and_project() {
        let p = Plane {
            origin: Point3::origin(),
            normal: Vector3::new(0.0, 0.0, 1.0),
        };
        assert!(p.contains_point(Point3::new(5.0, 3.0, 0.0)));
        assert!(!p.contains_point(Point3::new(5.0, 3.0, 1.0)));
        let proj = p.project_point(Point3::new(5.0, 3.0, 7.0));
        assert!((proj.z).abs() < EPS);
        assert!((proj.x - 5.0).abs() < EPS);
    }

    #[test]
    fn cylinder_contains_point() {
        let c = Cylinder {
            origin: Point3::origin(),
            axis: Vector3::new(0.0, 0.0, 1.0),
            radius: 3.0,
        };
        // Point on surface at (3, 0, 5)
        assert!(c.contains_point(Point3::new(3.0, 0.0, 5.0)));
        // Point inside
        assert!(!c.contains_point(Point3::new(1.0, 0.0, 5.0)));
        // Point on surface at 90° angle: (0, 3, 0)
        assert!(c.contains_point(Point3::new(0.0, 3.0, 0.0)));
        // Point on surface at 45° angle: (3/√2, 3/√2, 0)
        let s = 3.0 / std::f64::consts::SQRT_2;
        assert!(c.contains_point(Point3::new(s, s, 0.0)));
    }

    #[test]
    fn cylinder_project() {
        let c = Cylinder {
            origin: Point3::origin(),
            axis: Vector3::new(0.0, 0.0, 1.0),
            radius: 3.0,
        };
        let proj = c.project_point(Point3::new(6.0, 0.0, 5.0));
        assert!((proj.x - 3.0).abs() < EPS);
        assert!(proj.y.abs() < EPS);
        assert!((proj.z - 5.0).abs() < EPS);
    }

    #[test]
    fn sphere_evaluate() {
        let s = Sphere {
            center: Point3::origin(),
            radius: 5.0,
        };
        // u=0, v=0 → (5, 0, 0)
        let pt = s.evaluate(0.0, 0.0);
        assert!((pt.x - 5.0).abs() < EPS);
        assert!(pt.y.abs() < EPS);
        assert!(pt.z.abs() < EPS);
        // u=0, v=π/2 → (0, 0, 5)
        let pt2 = s.evaluate(0.0, FRAC_PI_2);
        assert!(pt2.x.abs() < EPS);
        assert!(pt2.y.abs() < EPS);
        assert!((pt2.z - 5.0).abs() < EPS);
        // All evaluated points must lie exactly on sphere surface (distance oracle)
        for &(u, v) in &[(0.5, 0.3), (1.0, -0.7), (3.14, 1.2)] {
            let p = s.evaluate(u, v);
            let dist = ((p.x * p.x) + (p.y * p.y) + (p.z * p.z)).sqrt();
            assert!(
                (dist - 5.0).abs() < EPS,
                "evaluate({u},{v}) not on sphere: dist={dist}"
            );
        }
    }

    #[test]
    fn sphere_contains_and_project() {
        let s = Sphere {
            center: Point3::new(1.0, 2.0, 3.0),
            radius: 5.0,
        };
        assert!(s.contains_point(Point3::new(6.0, 2.0, 3.0)));
        assert!(!s.contains_point(Point3::new(10.0, 2.0, 3.0)));
        let proj = s.project_point(Point3::new(11.0, 2.0, 3.0));
        assert!((proj.x - 6.0).abs() < EPS);
        assert!((proj.y - 2.0).abs() < EPS);
    }

    #[test]
    fn cone_contains_point() {
        let c = Cone {
            apex: Point3::origin(),
            axis: Vector3::new(0.0, 0.0, 1.0),
            half_angle: FRAC_PI_4, // 45°
        };
        // At h=5, r should be 5 (tan(45°) = 1). Point at (5, 0, 5) is on surface.
        assert!(c.contains_point(Point3::new(5.0, 0.0, 5.0)));
        // Inside
        assert!(!c.contains_point(Point3::new(1.0, 0.0, 5.0)));
        // Verify tan(half_angle) relationship: at height h, radius = h*tan(45°) = h
        for h in &[1.0, 3.0, 10.0] {
            assert!(c.contains_point(Point3::new(*h, 0.0, *h)));
            assert!(c.contains_point(Point3::new(0.0, *h, *h)));
        }
    }

    #[test]
    fn torus_contains_point() {
        let t = Torus {
            center: Point3::origin(),
            axis: Vector3::new(0.0, 0.0, 1.0),
            major_radius: 5.0,
            minor_radius: 1.0,
        };
        // Point on outer equator: (6, 0, 0) — distance from axis = 6, on tube at R+r
        assert!(t.contains_point(Point3::new(6.0, 0.0, 0.0)));
        // Point on inner equator: (4, 0, 0)
        assert!(t.contains_point(Point3::new(4.0, 0.0, 0.0)));
        // Point at top of tube: (5, 0, 1)
        assert!(t.contains_point(Point3::new(5.0, 0.0, 1.0)));
        // Point not on torus
        assert!(!t.contains_point(Point3::new(0.0, 0.0, 0.0)));
    }

    #[test]
    fn torus_project_point() {
        let t = Torus {
            center: Point3::origin(),
            axis: Vector3::new(0.0, 0.0, 1.0),
            major_radius: 5.0,
            minor_radius: 1.0,
        };
        // Project a point far from outer equator onto torus
        let proj = t.project_point(Point3::new(10.0, 0.0, 0.0));
        assert!((proj.x - 6.0).abs() < EPS);
        assert!(proj.y.abs() < EPS);
        assert!(proj.z.abs() < EPS);
    }

    #[test]
    fn surface_geom_dispatch() {
        let sg = SurfaceGeom::Spherical(Sphere {
            center: Point3::origin(),
            radius: 3.0,
        });
        assert!(sg.is_quadric());
        assert!(sg.contains_point(Point3::new(3.0, 0.0, 0.0)));
        let proj = sg.project_point(Point3::new(6.0, 0.0, 0.0));
        assert!((proj.x - 3.0).abs() < EPS);
    }

    #[test]
    fn tilted_plane_contains() {
        let n = Vector3::new(1.0, 1.0, 1.0).normalized();
        let p = Plane {
            origin: Point3::new(1.0, 1.0, 1.0),
            normal: n,
        };
        // Point on plane: (2, 1, 0) — (2-1, 1-1, 0-1)·(1,1,1)/√3 = (1+0-1)/√3 = 0
        assert!(p.contains_point(Point3::new(2.0, 1.0, 0.0)));
    }

    #[test]
    fn tilted_cylinder_contains() {
        let axis = Vector3::new(1.0, 1.0, 0.0).normalized();
        let c = Cylinder {
            origin: Point3::origin(),
            axis,
            radius: 2.0,
        };
        // Point perpendicular to axis at distance 2
        let pt = Point3::new(0.0, 0.0, 2.0);
        assert!(c.contains_point(pt));
    }
}
