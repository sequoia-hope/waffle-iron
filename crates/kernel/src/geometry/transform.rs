use serde::{Deserialize, Serialize};

use super::point::Point3d;
use super::vector::Vec3;

/// A 4x4 affine transformation matrix stored in column-major order.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    /// Column-major 4x4 matrix entries.
    pub m: [f64; 16],
}

impl Transform {
    pub fn identity() -> Self {
        #[rustfmt::skip]
        let m = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        Self { m }
    }

    pub fn translation(dx: f64, dy: f64, dz: f64) -> Self {
        #[rustfmt::skip]
        let m = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            dx,  dy,  dz,  1.0,
        ];
        Self { m }
    }

    pub fn from_translation_vec(v: Vec3) -> Self {
        Self::translation(v.x, v.y, v.z)
    }

    pub fn scaling(sx: f64, sy: f64, sz: f64) -> Self {
        #[rustfmt::skip]
        let m = [
            sx,  0.0, 0.0, 0.0,
            0.0, sy,  0.0, 0.0,
            0.0, 0.0, sz,  0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        Self { m }
    }

    pub fn uniform_scaling(s: f64) -> Self {
        Self::scaling(s, s, s)
    }

    /// Rotation around the X axis by `angle` radians.
    pub fn rotation_x(angle: f64) -> Self {
        let c = angle.cos();
        let s = angle.sin();
        #[rustfmt::skip]
        let m = [
            1.0, 0.0, 0.0, 0.0,
            0.0, c,   s,   0.0,
            0.0, -s,  c,   0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        Self { m }
    }

    /// Rotation around the Y axis by `angle` radians.
    pub fn rotation_y(angle: f64) -> Self {
        let c = angle.cos();
        let s = angle.sin();
        #[rustfmt::skip]
        let m = [
            c,   0.0, -s,  0.0,
            0.0, 1.0, 0.0, 0.0,
            s,   0.0, c,   0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        Self { m }
    }

    /// Rotation around the Z axis by `angle` radians.
    pub fn rotation_z(angle: f64) -> Self {
        let c = angle.cos();
        let s = angle.sin();
        #[rustfmt::skip]
        let m = [
            c,   s,   0.0, 0.0,
            -s,  c,   0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        Self { m }
    }

    /// Rotation around an arbitrary axis by `angle` radians (Rodrigues' formula).
    pub fn rotation_axis_angle(axis: Vec3, angle: f64) -> Self {
        let axis = axis.normalize();
        let c = angle.cos();
        let s = angle.sin();
        let t = 1.0 - c;
        let (x, y, z) = (axis.x, axis.y, axis.z);

        #[rustfmt::skip]
        let m = [
            t*x*x + c,     t*x*y + s*z,   t*x*z - s*y,   0.0,
            t*x*y - s*z,   t*y*y + c,     t*y*z + s*x,   0.0,
            t*x*z + s*y,   t*y*z - s*x,   t*z*z + c,     0.0,
            0.0,            0.0,            0.0,            1.0,
        ];
        Self { m }
    }

    /// Matrix element access (row, col), 0-indexed.
    fn at(&self, row: usize, col: usize) -> f64 {
        self.m[col * 4 + row]
    }

    /// Transform a point (applies translation).
    pub fn transform_point(&self, p: &Point3d) -> Point3d {
        let x = self.at(0, 0) * p.x + self.at(0, 1) * p.y + self.at(0, 2) * p.z + self.at(0, 3);
        let y = self.at(1, 0) * p.x + self.at(1, 1) * p.y + self.at(1, 2) * p.z + self.at(1, 3);
        let z = self.at(2, 0) * p.x + self.at(2, 1) * p.y + self.at(2, 2) * p.z + self.at(2, 3);
        Point3d::new(x, y, z)
    }

    /// Transform a vector (no translation).
    pub fn transform_vector(&self, v: &Vec3) -> Vec3 {
        let x = self.at(0, 0) * v.x + self.at(0, 1) * v.y + self.at(0, 2) * v.z;
        let y = self.at(1, 0) * v.x + self.at(1, 1) * v.y + self.at(1, 2) * v.z;
        let z = self.at(2, 0) * v.x + self.at(2, 1) * v.y + self.at(2, 2) * v.z;
        Vec3::new(x, y, z)
    }

    /// Compose two transforms: self * other.
    pub fn then(&self, other: &Transform) -> Transform {
        let mut result = [0.0f64; 16];
        for col in 0..4 {
            for row in 0..4 {
                let mut sum = 0.0;
                for k in 0..4 {
                    sum += self.at(row, k) * other.at(k, col);
                }
                result[col * 4 + row] = sum;
            }
        }
        Transform { m: result }
    }

    /// Compute the inverse transform. Returns None if the matrix is singular.
    pub fn inverse(&self) -> Option<Self> {
        let mut inv = [0.0f64; 16];
        let m = &self.m;

        inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
            + m[9] * m[7] * m[14]
            + m[13] * m[6] * m[11]
            - m[13] * m[7] * m[10];

        inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
            - m[8] * m[7] * m[14]
            - m[12] * m[6] * m[11]
            + m[12] * m[7] * m[10];

        inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
            + m[8] * m[7] * m[13]
            + m[12] * m[5] * m[11]
            - m[12] * m[7] * m[9];

        inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
            - m[8] * m[6] * m[13]
            - m[12] * m[5] * m[10]
            + m[12] * m[6] * m[9];

        let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
        if det.abs() < 1e-15 {
            return None;
        }

        inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
            - m[9] * m[3] * m[14]
            - m[13] * m[2] * m[11]
            + m[13] * m[3] * m[10];

        inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
            + m[8] * m[3] * m[14]
            + m[12] * m[2] * m[11]
            - m[12] * m[3] * m[10];

        inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
            - m[8] * m[3] * m[13]
            - m[12] * m[1] * m[11]
            + m[12] * m[3] * m[9];

        inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
            + m[8] * m[2] * m[13]
            + m[12] * m[1] * m[10]
            - m[12] * m[2] * m[9];

        inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
            + m[5] * m[3] * m[14]
            + m[13] * m[2] * m[7]
            - m[13] * m[3] * m[6];

        inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
            - m[4] * m[3] * m[14]
            - m[12] * m[2] * m[7]
            + m[12] * m[3] * m[6];

        inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
            + m[4] * m[3] * m[13]
            + m[12] * m[1] * m[7]
            - m[12] * m[3] * m[5];

        inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
            - m[4] * m[2] * m[13]
            - m[12] * m[1] * m[6]
            + m[12] * m[2] * m[5];

        inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
            - m[5] * m[3] * m[10]
            - m[9] * m[2] * m[7]
            + m[9] * m[3] * m[6];

        inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
            + m[4] * m[3] * m[10]
            + m[8] * m[2] * m[7]
            - m[8] * m[3] * m[6];

        inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
            - m[4] * m[3] * m[9]
            - m[8] * m[1] * m[7]
            + m[8] * m[3] * m[5];

        inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
            + m[4] * m[2] * m[9]
            + m[8] * m[1] * m[6]
            - m[8] * m[2] * m[5];

        let inv_det = 1.0 / det;
        for val in &mut inv {
            *val *= inv_det;
        }

        Some(Transform { m: inv })
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::identity()
    }
}

/// Axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    pub min: Point3d,
    pub max: Point3d,
}

impl BoundingBox {
    pub fn new(min: Point3d, max: Point3d) -> Self {
        Self { min, max }
    }

    pub fn empty() -> Self {
        Self {
            min: Point3d::new(f64::INFINITY, f64::INFINITY, f64::INFINITY),
            max: Point3d::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
        }
    }

    pub fn from_points(points: &[Point3d]) -> Self {
        let mut bb = Self::empty();
        for p in points {
            bb.expand_to_include(p);
        }
        bb
    }

    pub fn expand_to_include(&mut self, p: &Point3d) {
        self.min.x = self.min.x.min(p.x);
        self.min.y = self.min.y.min(p.y);
        self.min.z = self.min.z.min(p.z);
        self.max.x = self.max.x.max(p.x);
        self.max.y = self.max.y.max(p.y);
        self.max.z = self.max.z.max(p.z);
    }

    pub fn union(&self, other: &Self) -> Self {
        Self {
            min: Point3d::new(
                self.min.x.min(other.min.x),
                self.min.y.min(other.min.y),
                self.min.z.min(other.min.z),
            ),
            max: Point3d::new(
                self.max.x.max(other.max.x),
                self.max.y.max(other.max.y),
                self.max.z.max(other.max.z),
            ),
        }
    }

    pub fn intersects(&self, other: &Self) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }

    pub fn contains_point(&self, p: &Point3d) -> bool {
        p.x >= self.min.x
            && p.x <= self.max.x
            && p.y >= self.min.y
            && p.y <= self.max.y
            && p.z >= self.min.z
            && p.z <= self.max.z
    }

    pub fn center(&self) -> Point3d {
        self.min.midpoint(&self.max)
    }

    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    pub fn volume(&self) -> f64 {
        let s = self.size();
        s.x * s.y * s.z
    }

    pub fn is_valid(&self) -> bool {
        self.min.x <= self.max.x && self.min.y <= self.max.y && self.min.z <= self.max.z
    }

    pub fn expanded(&self, margin: f64) -> Self {
        Self {
            min: Point3d::new(
                self.min.x - margin,
                self.min.y - margin,
                self.min.z - margin,
            ),
            max: Point3d::new(
                self.max.x + margin,
                self.max.y + margin,
                self.max.z + margin,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    #[test]
    fn test_identity_transform() {
        let t = Transform::identity();
        let p = Point3d::new(1.0, 2.0, 3.0);
        let result = t.transform_point(&p);
        assert!((result.x - 1.0).abs() < 1e-12);
        assert!((result.y - 2.0).abs() < 1e-12);
        assert!((result.z - 3.0).abs() < 1e-12);
    }

    #[test]
    fn test_translation() {
        let t = Transform::translation(10.0, 20.0, 30.0);
        let p = Point3d::new(1.0, 2.0, 3.0);
        let result = t.transform_point(&p);
        assert!((result.x - 11.0).abs() < 1e-12);
        assert!((result.y - 22.0).abs() < 1e-12);
        assert!((result.z - 33.0).abs() < 1e-12);
    }

    #[test]
    fn test_rotation_z_90() {
        let t = Transform::rotation_z(FRAC_PI_2);
        let p = Point3d::new(1.0, 0.0, 0.0);
        let result = t.transform_point(&p);
        assert!((result.x).abs() < 1e-12);
        assert!((result.y - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_compose_transforms() {
        let t1 = Transform::translation(1.0, 0.0, 0.0);
        let t2 = Transform::translation(0.0, 2.0, 0.0);
        let combined = t1.then(&t2);
        let p = Point3d::ORIGIN;
        let result = combined.transform_point(&p);
        assert!((result.x - 1.0).abs() < 1e-12);
        assert!((result.y - 2.0).abs() < 1e-12);
    }

    #[test]
    fn test_inverse() {
        let t = Transform::translation(5.0, -3.0, 7.0);
        let inv = t.inverse().unwrap();
        let p = Point3d::new(1.0, 2.0, 3.0);
        let round_trip = inv.transform_point(&t.transform_point(&p));
        assert!((round_trip.x - p.x).abs() < 1e-12);
        assert!((round_trip.y - p.y).abs() < 1e-12);
        assert!((round_trip.z - p.z).abs() < 1e-12);
    }

    #[test]
    fn test_bounding_box() {
        let bb = BoundingBox::from_points(&[
            Point3d::new(0.0, 0.0, 0.0),
            Point3d::new(1.0, 2.0, 3.0),
            Point3d::new(-1.0, 0.5, 1.0),
        ]);
        assert!((bb.min.x - (-1.0)).abs() < 1e-12);
        assert!((bb.max.y - 2.0).abs() < 1e-12);
        assert!((bb.volume() - (2.0 * 2.0 * 3.0)).abs() < 1e-12);
    }

    #[test]
    fn test_bounding_box_intersects() {
        let a = BoundingBox::new(Point3d::new(0.0, 0.0, 0.0), Point3d::new(2.0, 2.0, 2.0));
        let b = BoundingBox::new(Point3d::new(1.0, 1.0, 1.0), Point3d::new(3.0, 3.0, 3.0));
        let c = BoundingBox::new(Point3d::new(5.0, 5.0, 5.0), Point3d::new(6.0, 6.0, 6.0));
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
    }
}
