//! Additional trait abstractions for validation and verification.
//!
//! The primary cross-module traits are defined closer to their domain:
//! - `BooleanEngine` in `boolean/mod.rs`
//! - `CurveEval` and `SurfaceEval` in `geometry/mod.rs`
//! - `SketchSolver` in the `cad-solver` crate
//!
//! This module provides validation-oriented traits and the solid verification trait.

use crate::geometry::point::Point3d;
use crate::geometry::vector::Vec3;
use crate::topology::brep::{EntityStore, SolidId};
use crate::validation::audit::VerificationReport;

/// Validation for curve geometry (extends CurveEval with degeneracy checks).
pub trait CurveValidation {
    /// Evaluate the curve at parameter `t`, returning a 3D point.
    fn evaluate(&self, t: f64) -> Point3d;

    /// Evaluate the derivative (tangent) at parameter `t`.
    fn derivative(&self, t: f64) -> Vec3;

    /// Check whether the curve is degenerate (e.g., zero-length direction).
    fn is_degenerate(&self, tolerance: f64) -> bool;

    /// Validate that a point lies on the curve within tolerance.
    fn point_on_curve(&self, point: &Point3d, t: f64, tolerance: f64) -> bool {
        point.distance_to(&self.evaluate(t)) < tolerance
    }
}

/// Validation for surface geometry (extends SurfaceEval with degeneracy checks).
pub trait SurfaceValidation {
    /// Evaluate the surface at parameters `(u, v)`.
    fn evaluate(&self, u: f64, v: f64) -> Point3d;

    /// Compute the outward normal at `(u, v)`.
    fn normal_at(&self, u: f64, v: f64) -> Vec3;

    /// Check whether the surface is degenerate at `(u, v)`.
    fn is_degenerate_at(&self, u: f64, v: f64, tolerance: f64) -> bool;

    /// Validate that a point lies on the surface within tolerance.
    fn point_on_surface(&self, point: &Point3d, u: f64, v: f64, tolerance: f64) -> bool {
        point.distance_to(&self.evaluate(u, v)) < tolerance
    }
}

/// Full solid verification combining topology and geometry checks.
pub trait SolidVerification {
    /// Run all verification levels and produce a report.
    fn verify(&self, store: &EntityStore, solid_id: SolidId) -> VerificationReport;
}

// ── Implementations ────────────────────────────────────────────────────────

impl CurveValidation for crate::geometry::curves::Curve {
    fn evaluate(&self, t: f64) -> Point3d {
        crate::geometry::curves::Curve::evaluate(self, t)
    }

    fn derivative(&self, t: f64) -> Vec3 {
        crate::geometry::curves::Curve::derivative(self, t)
    }

    fn is_degenerate(&self, tolerance: f64) -> bool {
        match self {
            crate::geometry::curves::Curve::Line(l) => l.direction.length() < tolerance,
            crate::geometry::curves::Curve::Circle(c) => c.radius < tolerance,
            crate::geometry::curves::Curve::Ellipse(e) => {
                e.major_radius < tolerance || e.minor_radius < tolerance
            }
            crate::geometry::curves::Curve::Nurbs(n) => n.control_points.len() < 2,
        }
    }
}

impl SurfaceValidation for crate::geometry::surfaces::Surface {
    fn evaluate(&self, u: f64, v: f64) -> Point3d {
        crate::geometry::surfaces::Surface::evaluate(self, u, v)
    }

    fn normal_at(&self, u: f64, v: f64) -> Vec3 {
        crate::geometry::surfaces::Surface::normal_at(self, u, v)
    }

    fn is_degenerate_at(&self, u: f64, v: f64, tolerance: f64) -> bool {
        let n = crate::geometry::surfaces::Surface::normal_at(self, u, v);
        n.length() < tolerance
    }
}

/// Default solid verification using the existing audit infrastructure.
pub struct DefaultSolidVerifier;

impl SolidVerification for DefaultSolidVerifier {
    fn verify(&self, store: &EntityStore, solid_id: SolidId) -> VerificationReport {
        crate::validation::audit::full_verify(store, solid_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::primitives::make_box;

    #[test]
    fn test_curve_validation_line() {
        use crate::geometry::curves::{Curve, Line3d};
        let curve = Curve::Line(Line3d::new(Point3d::ORIGIN, Vec3::X));
        assert!(!CurveValidation::is_degenerate(&curve, 1e-10));
        let p = CurveValidation::evaluate(&curve, 5.0);
        assert!((p.x - 5.0).abs() < 1e-12);
    }

    #[test]
    fn test_curve_validation_degenerate_circle() {
        use crate::geometry::curves::{Circle3d, Curve};
        let curve = Curve::Circle(Circle3d::new(Point3d::ORIGIN, Vec3::Z, 0.0));
        assert!(CurveValidation::is_degenerate(&curve, 1e-10));
    }

    #[test]
    fn test_curve_validation_point_on_curve() {
        use crate::geometry::curves::{Curve, Line3d};
        let curve = Curve::Line(Line3d::new(Point3d::ORIGIN, Vec3::X));
        let p = Point3d::new(3.0, 0.0, 0.0);
        assert!(curve.point_on_curve(&p, 3.0, 1e-10));
        let off = Point3d::new(3.0, 1.0, 0.0);
        assert!(!curve.point_on_curve(&off, 3.0, 1e-10));
    }

    #[test]
    fn test_surface_validation_plane() {
        use crate::geometry::surfaces::{Plane, Surface};
        let surface = Surface::Plane(Plane::xy());
        assert!(!SurfaceValidation::is_degenerate_at(&surface, 0.0, 0.0, 1e-10));
        let p = SurfaceValidation::evaluate(&surface, 3.0, 4.0);
        assert!((p.x - 3.0).abs() < 1e-12);
        assert!((p.y - 4.0).abs() < 1e-12);
    }

    #[test]
    fn test_surface_validation_point_on_surface() {
        use crate::geometry::surfaces::{Plane, Surface};
        let surface = Surface::Plane(Plane::xy());
        let p = Point3d::new(1.0, 2.0, 0.0);
        assert!(surface.point_on_surface(&p, 1.0, 2.0, 1e-10));
        let off = Point3d::new(1.0, 2.0, 1.0);
        assert!(!surface.point_on_surface(&off, 1.0, 2.0, 1e-10));
    }

    #[test]
    fn test_solid_verifier_trait() {
        let verifier = DefaultSolidVerifier;
        let mut store = EntityStore::new();
        let solid = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let report = verifier.verify(&store, solid);
        assert!(report.topology_valid);
    }
}
