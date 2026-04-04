//! 3D curve types with evaluation methods.
//!
//! Parametrization conventions:
//! - Line3D: evaluate(t) = origin + t·direction
//! - Circle3D: evaluate(t) = center + r·cos(t)·x + r·sin(t)·y, t ∈ [0, 2π)
//! - Arc3D: evaluate(t) = center + r·cos(t)·x + r·sin(t)·y, t ∈ [0, sweep_angle]

use super::point::{Point3, Vector3};
use crate::units::TAU_NORMALIZE;

/// Geometry attached to a B-Rep edge.
#[derive(Debug, Clone)]
pub enum CurveGeom {
    Linear(Line3D),
    Circular(Circle3D),
    Arc(Arc3D),
    Elliptical(Ellipse3D),
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

/// An ellipse in 3D space.
///
/// Parametrization: evaluate(t) = center + semi_major·cos(t)·major_axis + semi_minor·sin(t)·minor_axis
/// where minor_axis = normal × major_axis, t ∈ [0, 2π).
#[derive(Debug, Clone)]
pub struct Ellipse3D {
    pub center: Point3,
    pub normal: Vector3,
    pub major_axis: Vector3, // unit direction of semi-major axis
    pub semi_major: f64,     // >= semi_minor
    pub semi_minor: f64,
}

/// Given a unit normal, return (x_axis, y_axis) forming a right-handed frame.
fn make_frame(normal: Vector3) -> (Vector3, Vector3) {
    let hint = if normal.x.abs() < crate::units::BASIS_AXIS_ALIGNMENT {
        Vector3::new(1.0, 0.0, 0.0)
    } else {
        Vector3::new(0.0, 1.0, 0.0)
    };
    let x = normal.cross(hint).normalized();
    let y = normal.cross(x);
    (x, y)
}

// ── CurveGeom dispatch ──────────────────────────────────────────────────

impl CurveGeom {
    /// Evaluate the curve at parameter t, returning a 3D point.
    pub fn evaluate(&self, t: f64) -> Point3 {
        match self {
            CurveGeom::Linear(l) => l.evaluate(t),
            CurveGeom::Circular(c) => c.evaluate(t),
            CurveGeom::Arc(a) => a.evaluate(t),
            CurveGeom::Elliptical(e) => e.evaluate(t),
        }
    }

    /// Compute the unit tangent vector at parameter t.
    pub fn tangent(&self, t: f64) -> Vector3 {
        match self {
            CurveGeom::Linear(l) => l.tangent(t),
            CurveGeom::Circular(c) => c.tangent(t),
            CurveGeom::Arc(a) => a.tangent(t),
            CurveGeom::Elliptical(e) => e.tangent(t),
        }
    }
}

// ── Line3D evaluation ───────────────────────────────────────────────────

impl Line3D {
    pub fn evaluate(&self, t: f64) -> Point3 {
        Point3::new(
            self.origin.x + t * self.direction.x,
            self.origin.y + t * self.direction.y,
            self.origin.z + t * self.direction.z,
        )
    }

    pub fn tangent(&self, _t: f64) -> Vector3 {
        self.direction.normalized()
    }
}

// ── Circle3D evaluation ─────────────────────────────────────────────────

impl Circle3D {
    pub fn evaluate(&self, t: f64) -> Point3 {
        let (x, y) = make_frame(self.normal);
        Point3::new(
            self.center.x + self.radius * (t.cos() * x.x + t.sin() * y.x),
            self.center.y + self.radius * (t.cos() * x.y + t.sin() * y.y),
            self.center.z + self.radius * (t.cos() * x.z + t.sin() * y.z),
        )
    }

    pub fn tangent(&self, t: f64) -> Vector3 {
        let (x, y) = make_frame(self.normal);
        Vector3::new(
            -t.sin() * x.x + t.cos() * y.x,
            -t.sin() * x.y + t.cos() * y.y,
            -t.sin() * x.z + t.cos() * y.z,
        )
    }
}

// ── Arc3D evaluation ────────────────────────────────────────────────────

impl Arc3D {
    pub fn evaluate(&self, t: f64) -> Point3 {
        let (x_axis, _) = self.local_frame();
        let y_axis = self.normal.cross(x_axis);
        Point3::new(
            self.center.x + self.radius * (t.cos() * x_axis.x + t.sin() * y_axis.x),
            self.center.y + self.radius * (t.cos() * x_axis.y + t.sin() * y_axis.y),
            self.center.z + self.radius * (t.cos() * x_axis.z + t.sin() * y_axis.z),
        )
    }

    pub fn tangent(&self, t: f64) -> Vector3 {
        let (x_axis, _) = self.local_frame();
        let y_axis = self.normal.cross(x_axis);
        Vector3::new(
            -t.sin() * x_axis.x + t.cos() * y_axis.x,
            -t.sin() * x_axis.y + t.cos() * y_axis.y,
            -t.sin() * x_axis.z + t.cos() * y_axis.z,
        )
    }

    /// Derive the local frame from start_point: x_axis = (start_point - center) / radius.
    fn local_frame(&self) -> (Vector3, Vector3) {
        let dx = self.start_point.x - self.center.x;
        let dy = self.start_point.y - self.center.y;
        let dz = self.start_point.z - self.center.z;
        let len = (dx * dx + dy * dy + dz * dz).sqrt();
        if len < TAU_NORMALIZE {
            return make_frame(self.normal);
        }
        let x_axis = Vector3::new(dx / len, dy / len, dz / len);
        let y_axis = self.normal.cross(x_axis);
        (x_axis, y_axis)
    }
}

// ── Ellipse3D evaluation ───────────────────────────────────────────────

impl Ellipse3D {
    pub fn evaluate(&self, t: f64) -> Point3 {
        let minor_axis = self.normal.cross(self.major_axis);
        Point3::new(
            self.center.x
                + self.semi_major * t.cos() * self.major_axis.x
                + self.semi_minor * t.sin() * minor_axis.x,
            self.center.y
                + self.semi_major * t.cos() * self.major_axis.y
                + self.semi_minor * t.sin() * minor_axis.y,
            self.center.z
                + self.semi_major * t.cos() * self.major_axis.z
                + self.semi_minor * t.sin() * minor_axis.z,
        )
    }

    pub fn tangent(&self, t: f64) -> Vector3 {
        let minor_axis = self.normal.cross(self.major_axis);
        let dx = -self.semi_major * t.sin() * self.major_axis.x
            + self.semi_minor * t.cos() * minor_axis.x;
        let dy = -self.semi_major * t.sin() * self.major_axis.y
            + self.semi_minor * t.cos() * minor_axis.y;
        let dz = -self.semi_major * t.sin() * self.major_axis.z
            + self.semi_minor * t.cos() * minor_axis.z;
        Vector3::new(dx, dy, dz).normalized()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::MIN_FEATURE_SIZE;
    use std::f64::consts::FRAC_PI_2;

    const EPS: f64 = MIN_FEATURE_SIZE;

    #[test]
    fn line_evaluate() {
        let l = Line3D {
            origin: Point3::new(1.0, 0.0, 0.0),
            direction: Vector3::new(0.0, 1.0, 0.0),
        };
        let pt = l.evaluate(3.0);
        assert!((pt.x - 1.0).abs() < EPS);
        assert!((pt.y - 3.0).abs() < EPS);
        assert!(pt.z.abs() < EPS, "z must be 0 for Y-axis line");
        // Oracle: evaluate at multiple parameters, verify collinearity
        let pt0 = l.evaluate(0.0);
        let pt5 = l.evaluate(5.0);
        // Cross product of (pt5-pt0) × (pt-pt0) must be zero (collinear)
        let d1 = Vector3::new(pt5.x - pt0.x, pt5.y - pt0.y, pt5.z - pt0.z);
        let d2 = Vector3::new(pt.x - pt0.x, pt.y - pt0.y, pt.z - pt0.z);
        let cross_mag = (d1.y * d2.z - d1.z * d2.y).powi(2)
            + (d1.z * d2.x - d1.x * d2.z).powi(2)
            + (d1.x * d2.y - d1.y * d2.x).powi(2);
        assert!(cross_mag < EPS * EPS, "evaluated points must be collinear");
    }

    #[test]
    fn line_tangent() {
        let l = Line3D {
            origin: Point3::origin(),
            direction: Vector3::new(0.0, 2.0, 0.0),
        };
        let t = l.tangent(0.0);
        assert!((t.y - 1.0).abs() < EPS);
    }

    #[test]
    fn circle_evaluate() {
        let c = Circle3D {
            center: Point3::origin(),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 5.0,
        };
        let pt = c.evaluate(0.0);
        // Should be at distance 5 from center in XY plane
        let dist = ((pt.x * pt.x) + (pt.y * pt.y)).sqrt();
        assert!((dist - 5.0).abs() < EPS);
        assert!(pt.z.abs() < EPS);
    }

    #[test]
    fn circle_tangent_perpendicular_to_radius() {
        let c = Circle3D {
            center: Point3::origin(),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 3.0,
        };
        // Verify tangent perpendicularity at multiple parameters
        for &t in &[0.0, FRAC_PI_2, std::f64::consts::PI, 4.5] {
            let pt = c.evaluate(t);
            let tang = c.tangent(t);
            // All points must lie on circle (distance from center = radius)
            let dist = (pt.x * pt.x + pt.y * pt.y).sqrt();
            assert!(
                (dist - 3.0).abs() < EPS,
                "evaluate({t}) not on circle: dist={dist}"
            );
            // All points must be in the z=0 plane
            assert!(pt.z.abs() < EPS, "circle point must be in z=0 plane");
            // Tangent must be perpendicular to radius vector
            let radius_vec = Vector3::new(pt.x, pt.y, pt.z);
            let dot = radius_vec.dot(tang);
            assert!(
                dot.abs() < EPS,
                "tangent not perp to radius at t={t}: dot={dot}"
            );
            // Tangent must be unit length
            let tang_len = (tang.x * tang.x + tang.y * tang.y + tang.z * tang.z).sqrt();
            assert!(
                (tang_len - 1.0).abs() < EPS,
                "tangent not unit length at t={t}: len={tang_len}"
            );
        }
    }

    #[test]
    fn arc_evaluate_start() {
        let a = Arc3D {
            center: Point3::origin(),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 4.0,
            start_point: Point3::new(4.0, 0.0, 0.0),
            sweep_angle: FRAC_PI_2,
        };
        let pt = a.evaluate(0.0);
        assert!((pt.x - 4.0).abs() < EPS);
        assert!(pt.y.abs() < EPS);
    }

    #[test]
    fn curve_geom_dispatch() {
        let cg = CurveGeom::Linear(Line3D {
            origin: Point3::origin(),
            direction: Vector3::new(1.0, 0.0, 0.0),
        });
        let pt = cg.evaluate(5.0);
        assert!((pt.x - 5.0).abs() < EPS);
        let t = cg.tangent(0.0);
        assert!((t.x - 1.0).abs() < EPS);
    }

    #[test]
    fn ellipse3d_evaluate() {
        // Ellipse in XY plane: center=(0,0,0), normal=Z, major_axis=X
        // semi_major=5, semi_minor=3
        let e = Ellipse3D {
            center: Point3::origin(),
            normal: Vector3::new(0.0, 0.0, 1.0),
            major_axis: Vector3::new(1.0, 0.0, 0.0),
            semi_major: 5.0,
            semi_minor: 3.0,
        };
        // At t=0: point = (5, 0, 0)
        let pt0 = e.evaluate(0.0);
        assert!((pt0.x - 5.0).abs() < EPS);
        assert!(pt0.y.abs() < EPS);
        assert!(pt0.z.abs() < EPS);

        // At t=PI/2: point = (0, 3, 0)
        let pt90 = e.evaluate(FRAC_PI_2);
        assert!(pt90.x.abs() < EPS, "x={}", pt90.x);
        assert!((pt90.y - 3.0).abs() < EPS, "y={}", pt90.y);
        assert!(pt90.z.abs() < EPS);

        // At t=PI: point = (-5, 0, 0)
        let pt180 = e.evaluate(std::f64::consts::PI);
        assert!((pt180.x + 5.0).abs() < EPS, "x={}", pt180.x);
        assert!(pt180.y.abs() < EPS, "y={}", pt180.y);
    }

    #[test]
    fn ellipse3d_tangent() {
        let e = Ellipse3D {
            center: Point3::origin(),
            normal: Vector3::new(0.0, 0.0, 1.0),
            major_axis: Vector3::new(1.0, 0.0, 0.0),
            semi_major: 5.0,
            semi_minor: 3.0,
        };
        // At t=0: tangent direction is (0, semi_minor, 0), normalized = (0, 1, 0)
        let t0 = e.tangent(0.0);
        assert!(t0.x.abs() < EPS, "tx={}", t0.x);
        assert!((t0.y - 1.0).abs() < EPS, "ty={}", t0.y);
        assert!(t0.z.abs() < EPS);

        // Oracle: verify tangent is perpendicular to ellipse gradient at multiple points.
        // For ellipse (x/a)² + (y/b)² = 1, gradient ∇f = (2x/a², 2y/b²).
        // Tangent must be perpendicular to ∇f (not the radius vector — that's circles only).
        for &t in &[0.0, FRAC_PI_2, std::f64::consts::PI, 4.0] {
            let pt = e.evaluate(t);
            let tang = e.tangent(t);
            // Tangent must be perpendicular to the gradient of the implicit equation
            let grad = Vector3::new(2.0 * pt.x / (5.0 * 5.0), 2.0 * pt.y / (3.0 * 3.0), 0.0);
            let dot = grad.dot(tang);
            assert!(
                dot.abs() < EPS,
                "tangent not perp to gradient at t={t}: dot={dot}"
            );
            // Tangent must be unit length
            let tang_len = (tang.x * tang.x + tang.y * tang.y + tang.z * tang.z).sqrt();
            assert!(
                (tang_len - 1.0).abs() < EPS,
                "tangent not unit length at t={t}: len={tang_len}"
            );
            // All tangents must lie in z=0 plane
            assert!(
                tang.z.abs() < EPS,
                "ellipse tangent must be in z=0 plane at t={t}"
            );
            // Evaluated point must satisfy ellipse equation: (x/a)² + (y/b)² = 1
            let ellipse_eq = (pt.x / 5.0).powi(2) + (pt.y / 3.0).powi(2);
            assert!(
                (ellipse_eq - 1.0).abs() < EPS,
                "point not on ellipse at t={t}: eq={ellipse_eq}"
            );
        }
    }
}
