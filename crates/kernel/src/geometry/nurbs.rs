use serde::{Deserialize, Serialize};

use super::point::Point3d;
use super::vector::Vec3;

/// A NURBS (Non-Uniform Rational B-Spline) curve in 3D.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NurbsCurve {
    /// Degree of the curve.
    pub degree: usize,
    /// Control points in 3D.
    pub control_points: Vec<Point3d>,
    /// Weights for rational curves. If empty, treated as all 1.0 (non-rational).
    pub weights: Vec<f64>,
    /// Knot vector (must have len = control_points.len() + degree + 1).
    pub knots: Vec<f64>,
}

impl NurbsCurve {
    pub fn new(degree: usize, control_points: Vec<Point3d>, weights: Vec<f64>, knots: Vec<f64>) -> Self {
        assert!(
            knots.len() == control_points.len() + degree + 1,
            "Knot vector length must be n + p + 1"
        );
        assert!(
            weights.is_empty() || weights.len() == control_points.len(),
            "Weights must be empty or same length as control points"
        );
        Self {
            degree,
            control_points,
            weights,
            knots,
        }
    }

    /// Create a non-rational B-spline curve.
    pub fn bspline(degree: usize, control_points: Vec<Point3d>, knots: Vec<f64>) -> Self {
        Self::new(degree, control_points, vec![], knots)
    }

    fn is_rational(&self) -> bool {
        !self.weights.is_empty()
    }

    fn weight(&self, i: usize) -> f64 {
        if self.is_rational() {
            self.weights[i]
        } else {
            1.0
        }
    }

    /// Number of control points.
    pub fn num_control_points(&self) -> usize {
        self.control_points.len()
    }

    /// Parameter domain [t_min, t_max].
    pub fn domain(&self) -> (f64, f64) {
        (self.knots[self.degree], self.knots[self.knots.len() - self.degree - 1])
    }

    /// Find the knot span index for parameter t using binary search.
    fn find_span(&self, t: f64) -> usize {
        let n = self.num_control_points() - 1;
        let p = self.degree;

        if t >= self.knots[n + 1] {
            return n;
        }
        if t <= self.knots[p] {
            return p;
        }

        let mut low = p;
        let mut high = n + 1;
        let mut mid = (low + high) / 2;
        while t < self.knots[mid] || t >= self.knots[mid + 1] {
            if t < self.knots[mid] {
                high = mid;
            } else {
                low = mid;
            }
            mid = (low + high) / 2;
        }
        mid
    }

    /// Compute B-spline basis functions at parameter t.
    fn basis_functions(&self, span: usize, t: f64) -> Vec<f64> {
        let p = self.degree;
        let mut n_vals = vec![0.0; p + 1];
        let mut left = vec![0.0; p + 1];
        let mut right = vec![0.0; p + 1];

        n_vals[0] = 1.0;
        for j in 1..=p {
            left[j] = t - self.knots[span + 1 - j];
            right[j] = self.knots[span + j] - t;
            let mut saved = 0.0;
            for r in 0..j {
                let temp = n_vals[r] / (right[r + 1] + left[j - r]);
                n_vals[r] = saved + right[r + 1] * temp;
                saved = left[j - r] * temp;
            }
            n_vals[j] = saved;
        }
        n_vals
    }

    /// Evaluate the curve at parameter t using de Boor's algorithm.
    pub fn evaluate(&self, t: f64) -> Point3d {
        let span = self.find_span(t);
        let basis = self.basis_functions(span, t);
        let p = self.degree;

        if !self.is_rational() {
            let mut point = Vec3::ZERO;
            for i in 0..=p {
                let cp = self.control_points[span - p + i];
                point = point + Vec3::new(cp.x, cp.y, cp.z) * basis[i];
            }
            Point3d::new(point.x, point.y, point.z)
        } else {
            let mut wx = 0.0;
            let mut wy = 0.0;
            let mut wz = 0.0;
            let mut w_sum = 0.0;
            for i in 0..=p {
                let idx = span - p + i;
                let cp = self.control_points[idx];
                let w = self.weight(idx);
                let bw = basis[i] * w;
                wx += cp.x * bw;
                wy += cp.y * bw;
                wz += cp.z * bw;
                w_sum += bw;
            }
            Point3d::new(wx / w_sum, wy / w_sum, wz / w_sum)
        }
    }

    /// Evaluate the first derivative at parameter t.
    pub fn derivative(&self, t: f64) -> Vec3 {
        // Use finite difference for robustness; analytic derivative can be added later
        let dt = 1e-8;
        let (tmin, tmax) = self.domain();
        let t0 = (t - dt).max(tmin);
        let t1 = (t + dt).min(tmax);
        let p0 = self.evaluate(t0);
        let p1 = self.evaluate(t1);
        let actual_dt = t1 - t0;
        if actual_dt.abs() < 1e-15 {
            return Vec3::ZERO;
        }
        (p1 - p0) / actual_dt
    }

    /// Compute an approximate arc length by sampling.
    pub fn approximate_length(&self, num_samples: usize) -> f64 {
        let (t0, t1) = self.domain();
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
}

/// A NURBS surface (tensor-product).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NurbsSurface {
    pub degree_u: usize,
    pub degree_v: usize,
    /// Control points grid: [u_index * num_v + v_index]
    pub control_points: Vec<Point3d>,
    pub weights: Vec<f64>,
    pub knots_u: Vec<f64>,
    pub knots_v: Vec<f64>,
    pub num_u: usize,
    pub num_v: usize,
}

impl NurbsSurface {
    pub fn new(
        degree_u: usize,
        degree_v: usize,
        control_points: Vec<Point3d>,
        weights: Vec<f64>,
        knots_u: Vec<f64>,
        knots_v: Vec<f64>,
        num_u: usize,
        num_v: usize,
    ) -> Self {
        assert_eq!(control_points.len(), num_u * num_v);
        assert_eq!(knots_u.len(), num_u + degree_u + 1);
        assert_eq!(knots_v.len(), num_v + degree_v + 1);
        Self {
            degree_u,
            degree_v,
            control_points,
            weights,
            knots_u,
            knots_v,
            num_u,
            num_v,
        }
    }

    fn is_rational(&self) -> bool {
        !self.weights.is_empty()
    }

    fn weight(&self, u_idx: usize, v_idx: usize) -> f64 {
        if self.is_rational() {
            self.weights[u_idx * self.num_v + v_idx]
        } else {
            1.0
        }
    }

    pub fn domain_u(&self) -> (f64, f64) {
        (
            self.knots_u[self.degree_u],
            self.knots_u[self.knots_u.len() - self.degree_u - 1],
        )
    }

    pub fn domain_v(&self) -> (f64, f64) {
        (
            self.knots_v[self.degree_v],
            self.knots_v[self.knots_v.len() - self.degree_v - 1],
        )
    }

    fn find_span_u(&self, u: f64) -> usize {
        let n = self.num_u - 1;
        let p = self.degree_u;
        if u >= self.knots_u[n + 1] {
            return n;
        }
        if u <= self.knots_u[p] {
            return p;
        }
        let mut low = p;
        let mut high = n + 1;
        let mut mid = (low + high) / 2;
        while u < self.knots_u[mid] || u >= self.knots_u[mid + 1] {
            if u < self.knots_u[mid] {
                high = mid;
            } else {
                low = mid;
            }
            mid = (low + high) / 2;
        }
        mid
    }

    fn find_span_v(&self, v: f64) -> usize {
        let n = self.num_v - 1;
        let p = self.degree_v;
        if v >= self.knots_v[n + 1] {
            return n;
        }
        if v <= self.knots_v[p] {
            return p;
        }
        let mut low = p;
        let mut high = n + 1;
        let mut mid = (low + high) / 2;
        while v < self.knots_v[mid] || v >= self.knots_v[mid + 1] {
            if v < self.knots_v[mid] {
                high = mid;
            } else {
                low = mid;
            }
            mid = (low + high) / 2;
        }
        mid
    }

    fn basis_functions(knots: &[f64], span: usize, t: f64, degree: usize) -> Vec<f64> {
        let p = degree;
        let mut n_vals = vec![0.0; p + 1];
        let mut left = vec![0.0; p + 1];
        let mut right = vec![0.0; p + 1];

        n_vals[0] = 1.0;
        for j in 1..=p {
            left[j] = t - knots[span + 1 - j];
            right[j] = knots[span + j] - t;
            let mut saved = 0.0;
            for r in 0..j {
                let temp = n_vals[r] / (right[r + 1] + left[j - r]);
                n_vals[r] = saved + right[r + 1] * temp;
                saved = left[j - r] * temp;
            }
            n_vals[j] = saved;
        }
        n_vals
    }

    /// Evaluate the surface at (u, v).
    pub fn evaluate(&self, u: f64, v: f64) -> Point3d {
        let span_u = self.find_span_u(u);
        let span_v = self.find_span_v(v);
        let basis_u = Self::basis_functions(&self.knots_u, span_u, u, self.degree_u);
        let basis_v = Self::basis_functions(&self.knots_v, span_v, v, self.degree_v);

        let mut wx = 0.0;
        let mut wy = 0.0;
        let mut wz = 0.0;
        let mut w_sum = 0.0;

        for i in 0..=self.degree_u {
            let u_idx = span_u - self.degree_u + i;
            for j in 0..=self.degree_v {
                let v_idx = span_v - self.degree_v + j;
                let cp = self.control_points[u_idx * self.num_v + v_idx];
                let w = self.weight(u_idx, v_idx);
                let bw = basis_u[i] * basis_v[j] * w;
                wx += cp.x * bw;
                wy += cp.y * bw;
                wz += cp.z * bw;
                w_sum += bw;
            }
        }

        if self.is_rational() {
            Point3d::new(wx / w_sum, wy / w_sum, wz / w_sum)
        } else {
            Point3d::new(wx, wy, wz)
        }
    }

    /// Compute the surface normal at (u, v) via finite differences.
    pub fn normal(&self, u: f64, v: f64) -> Vec3 {
        let du = 1e-7;
        let dv = 1e-7;
        let (u_min, u_max) = self.domain_u();
        let (v_min, v_max) = self.domain_v();

        let u0 = (u - du).max(u_min);
        let u1 = (u + du).min(u_max);
        let v0 = (v - dv).max(v_min);
        let v1 = (v + dv).min(v_max);

        let pu0 = self.evaluate(u0, v);
        let pu1 = self.evaluate(u1, v);
        let pv0 = self.evaluate(u, v0);
        let pv1 = self.evaluate(u, v1);

        let du_vec = pu1 - pu0;
        let dv_vec = pv1 - pv0;
        let n = du_vec.cross(&dv_vec);
        n.normalized().unwrap_or(Vec3::Z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_line_as_nurbs() -> NurbsCurve {
        // A degree-1 NURBS from (0,0,0) to (10,0,0)
        NurbsCurve::bspline(
            1,
            vec![Point3d::new(0.0, 0.0, 0.0), Point3d::new(10.0, 0.0, 0.0)],
            vec![0.0, 0.0, 1.0, 1.0],
        )
    }

    #[test]
    fn test_nurbs_line_evaluate() {
        let c = make_line_as_nurbs();
        let p = c.evaluate(0.5);
        assert!((p.x - 5.0).abs() < 1e-10);
        assert!(p.y.abs() < 1e-10);
    }

    #[test]
    fn test_nurbs_line_endpoints() {
        let c = make_line_as_nurbs();
        let (t0, t1) = c.domain();
        let p0 = c.evaluate(t0);
        let p1 = c.evaluate(t1);
        assert!(p0.distance_to(&Point3d::ORIGIN) < 1e-10);
        assert!((p1.x - 10.0).abs() < 1e-10);
    }

    #[test]
    fn test_nurbs_quadratic_curve() {
        // Degree-2 curve: a parabolic arc
        let c = NurbsCurve::bspline(
            2,
            vec![
                Point3d::new(0.0, 0.0, 0.0),
                Point3d::new(5.0, 10.0, 0.0),
                Point3d::new(10.0, 0.0, 0.0),
            ],
            vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        );
        let mid = c.evaluate(0.5);
        // At t=0.5 for this symmetric quadratic, y should be 5.0
        assert!((mid.x - 5.0).abs() < 1e-10);
        assert!((mid.y - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_nurbs_circle_via_rational() {
        // Quarter circle using rational NURBS (degree 2)
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let c = NurbsCurve::new(
            2,
            vec![
                Point3d::new(1.0, 0.0, 0.0),
                Point3d::new(1.0, 1.0, 0.0),
                Point3d::new(0.0, 1.0, 0.0),
            ],
            vec![1.0, w, 1.0],
            vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        );

        // All points should lie on the unit circle
        for i in 0..=20 {
            let t = i as f64 / 20.0;
            let p = c.evaluate(t);
            let r = (p.x * p.x + p.y * p.y).sqrt();
            assert!(
                (r - 1.0).abs() < 1e-7,
                "Point at t={} has radius {}, expected 1.0",
                t,
                r
            );
        }
    }

    #[test]
    fn test_nurbs_derivative_direction() {
        let c = make_line_as_nurbs();
        let d = c.derivative(0.5);
        // For a line along X, derivative should point in +X
        assert!(d.x > 0.0);
        assert!(d.y.abs() < 1e-6);
        assert!(d.z.abs() < 1e-6);
    }
}
