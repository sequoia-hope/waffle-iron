pub mod point;
pub mod vector;
pub mod transform;
pub mod curves;
pub mod surfaces;
pub mod intersection;
pub mod surface_intersection;
pub mod nurbs;

use point::Point3d;
use vector::Vec3;

/// Trait for evaluating parametric curves.
///
/// Implement this to provide curve evaluation for any curve representation.
pub trait CurveEval {
    /// Evaluate the curve at parameter `t`, returning a 3D point.
    fn evaluate(&self, t: f64) -> Point3d;

    /// Evaluate the tangent vector at parameter `t`.
    fn tangent(&self, t: f64) -> Vec3;
}

/// Trait for evaluating parametric surfaces.
///
/// Implement this to provide surface evaluation for any surface representation.
pub trait SurfaceEval {
    /// Evaluate the surface at parameters `(u, v)`, returning a 3D point.
    fn evaluate(&self, u: f64, v: f64) -> Point3d;

    /// Compute the surface normal at parameters `(u, v)`.
    fn normal_at(&self, u: f64, v: f64) -> Vec3;
}

impl CurveEval for curves::Curve {
    fn evaluate(&self, t: f64) -> Point3d {
        curves::Curve::evaluate(self, t)
    }

    fn tangent(&self, t: f64) -> Vec3 {
        curves::Curve::derivative(self, t)
    }
}

impl SurfaceEval for surfaces::Surface {
    fn evaluate(&self, u: f64, v: f64) -> Point3d {
        surfaces::Surface::evaluate(self, u, v)
    }

    fn normal_at(&self, u: f64, v: f64) -> Vec3 {
        surfaces::Surface::normal_at(self, u, v)
    }
}

#[cfg(test)]
mod trait_tests {
    use super::*;

    #[test]
    fn test_curve_eval_line() {
        let curve = curves::Curve::Line(curves::Line3d::new(Point3d::ORIGIN, Vec3::X));
        let p = CurveEval::evaluate(&curve, 5.0);
        assert!((p.x - 5.0).abs() < 1e-12);
        let t = CurveEval::tangent(&curve, 0.0);
        assert!((t.x - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_curve_eval_circle() {
        let curve = curves::Curve::Circle(curves::Circle3d::new(Point3d::ORIGIN, Vec3::Z, 5.0));
        let p = CurveEval::evaluate(&curve, 0.0);
        assert!((p.distance_to(&Point3d::ORIGIN) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_surface_eval_plane() {
        let surface = surfaces::Surface::Plane(surfaces::Plane::xy());
        let p = SurfaceEval::evaluate(&surface, 3.0, 4.0);
        assert!((p.x - 3.0).abs() < 1e-12);
        assert!((p.y - 4.0).abs() < 1e-12);
        let n = SurfaceEval::normal_at(&surface, 0.0, 0.0);
        assert!((n.z - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_surface_eval_sphere() {
        let surface = surfaces::Surface::Sphere(surfaces::Sphere::new(Point3d::ORIGIN, 3.0));
        let p = SurfaceEval::evaluate(&surface, 0.0, 0.0);
        assert!((p.distance_to(&Point3d::ORIGIN) - 3.0).abs() < 1e-10);
    }
}
