use serde::{Deserialize, Serialize};
use std::ops::{Add, Div, Mul, Neg, Sub};

/// A vector in 3D Euclidean space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const X: Self = Self {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    };
    pub const Y: Self = Self {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };
    pub const Z: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };

    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn dot(&self, other: &Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(&self, other: &Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    pub fn length(&self) -> f64 {
        self.length_squared().sqrt()
    }

    pub fn length_squared(&self) -> f64 {
        self.dot(self)
    }

    pub fn normalized(&self) -> Option<Self> {
        let len = self.length();
        if len < 1e-15 {
            None
        } else {
            Some(*self / len)
        }
    }

    /// Normalize, panicking if the vector is near-zero.
    pub fn normalize(&self) -> Self {
        self.normalized().expect("Cannot normalize zero-length vector")
    }

    pub fn angle_to(&self, other: &Self) -> f64 {
        let d = self.dot(other);
        let len_product = self.length() * other.length();
        if len_product < 1e-15 {
            return 0.0;
        }
        (d / len_product).clamp(-1.0, 1.0).acos()
    }

    pub fn is_parallel_to(&self, other: &Self, angular_tol: f64) -> bool {
        let angle = self.angle_to(other);
        angle < angular_tol || (std::f64::consts::PI - angle) < angular_tol
    }

    pub fn is_perpendicular_to(&self, other: &Self, angular_tol: f64) -> bool {
        let angle = self.angle_to(other);
        (angle - std::f64::consts::FRAC_PI_2).abs() < angular_tol
    }

    pub fn project_onto(&self, other: &Self) -> Self {
        let denom = other.length_squared();
        if denom < 1e-30 {
            return Self::ZERO;
        }
        *other * (self.dot(other) / denom)
    }

    pub fn reflect(&self, normal: &Self) -> Self {
        *self - *normal * (2.0 * self.dot(normal))
    }

    pub fn to_array(&self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }

    pub fn from_array(arr: [f64; 3]) -> Self {
        Self {
            x: arr[0],
            y: arr[1],
            z: arr[2],
        }
    }

    /// Triple scalar product: self . (b x c)
    pub fn triple(&self, b: &Self, c: &Self) -> f64 {
        self.dot(&b.cross(c))
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Mul<f64> for Vec3 {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl Mul<Vec3> for f64 {
    type Output = Vec3;
    fn mul(self, rhs: Vec3) -> Self::Output {
        Vec3::new(self * rhs.x, self * rhs.y, self * rhs.z)
    }
}

impl Div<f64> for Vec3 {
    type Output = Self;
    fn div(self, rhs: f64) -> Self::Output {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}

impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self::new(-self.x, -self.y, -self.z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    #[test]
    fn test_dot_product() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        assert!((a.dot(&b) - 32.0).abs() < 1e-12);
    }

    #[test]
    fn test_cross_product() {
        let result = Vec3::X.cross(&Vec3::Y);
        assert!((result.x - Vec3::Z.x).abs() < 1e-12);
        assert!((result.y - Vec3::Z.y).abs() < 1e-12);
        assert!((result.z - Vec3::Z.z).abs() < 1e-12);
    }

    #[test]
    fn test_normalize() {
        let v = Vec3::new(3.0, 0.0, 4.0);
        let n = v.normalize();
        assert!((n.length() - 1.0).abs() < 1e-12);
        assert!((n.x - 0.6).abs() < 1e-12);
        assert!((n.z - 0.8).abs() < 1e-12);
    }

    #[test]
    fn test_angle_to() {
        let angle = Vec3::X.angle_to(&Vec3::Y);
        assert!((angle - FRAC_PI_2).abs() < 1e-12);

        let angle2 = Vec3::X.angle_to(&(-Vec3::X));
        assert!((angle2 - PI).abs() < 1e-12);
    }

    #[test]
    fn test_parallel() {
        assert!(Vec3::X.is_parallel_to(&(Vec3::X * 5.0), 1e-10));
        assert!(Vec3::X.is_parallel_to(&(-Vec3::X), 1e-10));
        assert!(!Vec3::X.is_parallel_to(&Vec3::Y, 1e-10));
    }

    #[test]
    fn test_reflect() {
        let incoming = Vec3::new(1.0, -1.0, 0.0);
        let normal = Vec3::Y;
        let reflected = incoming.reflect(&normal);
        assert!((reflected.x - 1.0).abs() < 1e-12);
        assert!((reflected.y - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_triple_product() {
        let a = Vec3::X;
        let b = Vec3::Y;
        let c = Vec3::Z;
        assert!((a.triple(&b, &c) - 1.0).abs() < 1e-12);
    }
}
