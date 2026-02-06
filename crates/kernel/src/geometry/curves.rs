use serde::{Deserialize, Serialize};

use super::nurbs::NurbsCurve;
use super::point::Point3d;
use super::vector::Vec3;

/// Analytic and parametric curve representations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Curve {
    Line(Line3d),
    Circle(Circle3d),
    Ellipse(Ellipse3d),
    Nurbs(NurbsCurve),
}

/// An infinite line defined by a point and direction (used in intersection math).
/// For bounded segments, use parameter range.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Line3d {
    pub origin: Point3d,
    pub direction: Vec3,
}

impl Line3d {
    pub fn new(origin: Point3d, direction: Vec3) -> Self {
        Self {
            origin,
            direction: direction.normalize(),
        }
    }

    pub fn from_points(a: Point3d, b: Point3d) -> Self {
        let dir = b - a;
        Self {
            origin: a,
            direction: dir.normalize(),
        }
    }

    pub fn evaluate(&self, t: f64) -> Point3d {
        self.origin + self.direction * t
    }

    pub fn closest_point(&self, p: &Point3d) -> (Point3d, f64) {
        let v = *p - self.origin;
        let t = v.dot(&self.direction);
        (self.evaluate(t), t)
    }

    pub fn distance_to_point(&self, p: &Point3d) -> f64 {
        let (closest, _) = self.closest_point(p);
        p.distance_to(&closest)
    }
}

/// A circle in 3D space.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Circle3d {
    pub center: Point3d,
    pub normal: Vec3,
    pub radius: f64,
    /// Reference direction in the plane (x-axis of the local frame).
    pub x_axis: Vec3,
}

impl Circle3d {
    pub fn new(center: Point3d, normal: Vec3, radius: f64) -> Self {
        let normal = normal.normalize();
        // Compute a perpendicular x_axis
        let x_axis = if normal.x.abs() < 0.9 {
            Vec3::X.cross(&normal).normalize()
        } else {
            Vec3::Y.cross(&normal).normalize()
        };
        Self {
            center,
            normal,
            radius,
            x_axis,
        }
    }

    pub fn with_axes(center: Point3d, normal: Vec3, x_axis: Vec3, radius: f64) -> Self {
        Self {
            center,
            normal: normal.normalize(),
            x_axis: x_axis.normalize(),
            radius,
        }
    }

    fn y_axis(&self) -> Vec3 {
        self.normal.cross(&self.x_axis)
    }

    /// Evaluate at angle t (radians, 0..2*PI).
    pub fn evaluate(&self, t: f64) -> Point3d {
        let y_axis = self.y_axis();
        self.center + self.x_axis * (self.radius * t.cos()) + y_axis * (self.radius * t.sin())
    }

    pub fn derivative(&self, t: f64) -> Vec3 {
        let y_axis = self.y_axis();
        self.x_axis * (-self.radius * t.sin()) + y_axis * (self.radius * t.cos())
    }

    pub fn circumference(&self) -> f64 {
        2.0 * std::f64::consts::PI * self.radius
    }
}

/// An ellipse in 3D space.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Ellipse3d {
    pub center: Point3d,
    pub normal: Vec3,
    pub major_axis: Vec3,
    pub major_radius: f64,
    pub minor_radius: f64,
}

impl Ellipse3d {
    pub fn new(
        center: Point3d,
        normal: Vec3,
        major_axis: Vec3,
        major_radius: f64,
        minor_radius: f64,
    ) -> Self {
        Self {
            center,
            normal: normal.normalize(),
            major_axis: major_axis.normalize(),
            major_radius,
            minor_radius,
        }
    }

    fn minor_axis(&self) -> Vec3 {
        self.normal.cross(&self.major_axis)
    }

    pub fn evaluate(&self, t: f64) -> Point3d {
        let minor_axis = self.minor_axis();
        self.center
            + self.major_axis * (self.major_radius * t.cos())
            + minor_axis * (self.minor_radius * t.sin())
    }

    pub fn derivative(&self, t: f64) -> Vec3 {
        let minor_axis = self.minor_axis();
        self.major_axis * (-self.major_radius * t.sin())
            + minor_axis * (self.minor_radius * t.cos())
    }
}

impl Curve {
    /// Evaluate the curve at parameter t.
    pub fn evaluate(&self, t: f64) -> Point3d {
        match self {
            Curve::Line(l) => l.evaluate(t),
            Curve::Circle(c) => c.evaluate(t),
            Curve::Ellipse(e) => e.evaluate(t),
            Curve::Nurbs(n) => n.evaluate(t),
        }
    }

    /// Evaluate the derivative at parameter t.
    pub fn derivative(&self, t: f64) -> Vec3 {
        match self {
            Curve::Line(l) => l.direction,
            Curve::Circle(c) => c.derivative(t),
            Curve::Ellipse(e) => e.derivative(t),
            Curve::Nurbs(n) => n.derivative(t),
        }
    }

    /// Approximate arc length over the given parameter range.
    pub fn approximate_length(&self, t0: f64, t1: f64, num_samples: usize) -> f64 {
        let mut length = 0.0;
        let mut prev = self.evaluate(t0);
        for i in 1..=num_samples {
            let t = t0 + (t1 - t0) * (i as f64 / num_samples as f64);
            let curr = self.evaluate(t);
            length += prev.distance_to(&curr);
            prev = curr;
        }
        length
    }

    /// Classify the curve type for logging/debugging.
    pub fn curve_type_name(&self) -> &'static str {
        match self {
            Curve::Line(_) => "Line",
            Curve::Circle(_) => "Circle",
            Curve::Ellipse(_) => "Ellipse",
            Curve::Nurbs(_) => "Nurbs",
        }
    }
}

/// A ray for intersection testing.
#[derive(Debug, Clone, Copy)]
pub struct Ray {
    pub origin: Point3d,
    pub direction: Vec3,
}

impl Ray {
    pub fn new(origin: Point3d, direction: Vec3) -> Self {
        Self {
            origin,
            direction: direction.normalize(),
        }
    }

    pub fn at(&self, t: f64) -> Point3d {
        self.origin + self.direction * t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn test_line_evaluate() {
        let l = Line3d::new(Point3d::ORIGIN, Vec3::X);
        assert!(l.evaluate(5.0).distance_to(&Point3d::new(5.0, 0.0, 0.0)) < 1e-12);
    }

    #[test]
    fn test_line_closest_point() {
        let l = Line3d::new(Point3d::ORIGIN, Vec3::X);
        let p = Point3d::new(5.0, 3.0, 0.0);
        let (closest, t) = l.closest_point(&p);
        assert!((t - 5.0).abs() < 1e-12);
        assert!((closest.x - 5.0).abs() < 1e-12);
        assert!(closest.y.abs() < 1e-12);
    }

    #[test]
    fn test_circle_evaluate() {
        let c = Circle3d::new(Point3d::ORIGIN, Vec3::Z, 5.0);
        let p0 = c.evaluate(0.0);
        assert!((p0.distance_to(&Point3d::ORIGIN) - 5.0).abs() < 1e-10);

        // Midway point should also be at radius 5
        let p_mid = c.evaluate(PI / 4.0);
        assert!((p_mid.distance_to(&Point3d::ORIGIN) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_circle_full_loop() {
        let c = Circle3d::new(Point3d::ORIGIN, Vec3::Z, 1.0);
        let p0 = c.evaluate(0.0);
        let p2pi = c.evaluate(2.0 * PI);
        assert!(p0.distance_to(&p2pi) < 1e-10);
    }

    #[test]
    fn test_ellipse_evaluate() {
        let e = Ellipse3d::new(Point3d::ORIGIN, Vec3::Z, Vec3::X, 10.0, 5.0);
        let p0 = e.evaluate(0.0);
        assert!((p0.x - 10.0).abs() < 1e-10);
        assert!(p0.y.abs() < 1e-10);

        let p_pi2 = e.evaluate(PI / 2.0);
        assert!(p_pi2.x.abs() < 1e-10);
        assert!((p_pi2.y - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_curve_enum_dispatch() {
        let c = Curve::Line(Line3d::new(Point3d::ORIGIN, Vec3::X));
        let p = c.evaluate(3.0);
        assert!((p.x - 3.0).abs() < 1e-12);
    }
}
