use serde::{Deserialize, Serialize};
use std::ops::{Add, Sub};

use super::vector::Vec3;

/// A point in 3D Euclidean space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point3d {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Point3d {
    pub const ORIGIN: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn distance_to(&self, other: &Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    pub fn distance_squared_to(&self, other: &Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx * dx + dy * dy + dz * dz
    }

    pub fn midpoint(&self, other: &Self) -> Self {
        Self {
            x: (self.x + other.x) * 0.5,
            y: (self.y + other.y) * 0.5,
            z: (self.z + other.z) * 0.5,
        }
    }

    pub fn lerp(&self, other: &Self, t: f64) -> Self {
        Self {
            x: self.x + t * (other.x - self.x),
            y: self.y + t * (other.y - self.y),
            z: self.z + t * (other.z - self.z),
        }
    }

    pub fn to_vec3(&self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
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
}

impl Add<Vec3> for Point3d {
    type Output = Point3d;
    fn add(self, rhs: Vec3) -> Self::Output {
        Point3d::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Sub for Point3d {
    type Output = Vec3;
    fn sub(self, rhs: Self) -> Self::Output {
        Vec3::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl Sub<Vec3> for Point3d {
    type Output = Point3d;
    fn sub(self, rhs: Vec3) -> Self::Output {
        Point3d::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

/// A point in 2D space (for sketch/parametric operations).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point2d {
    pub x: f64,
    pub y: f64,
}

impl Point2d {
    pub const ORIGIN: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance_to(&self, other: &Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_distance() {
        let a = Point3d::new(1.0, 0.0, 0.0);
        let b = Point3d::new(4.0, 0.0, 0.0);
        assert!((a.distance_to(&b) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn test_point_midpoint() {
        let a = Point3d::new(0.0, 0.0, 0.0);
        let b = Point3d::new(2.0, 4.0, 6.0);
        let m = a.midpoint(&b);
        assert!((m.x - 1.0).abs() < 1e-12);
        assert!((m.y - 2.0).abs() < 1e-12);
        assert!((m.z - 3.0).abs() < 1e-12);
    }

    #[test]
    fn test_point_sub_gives_vector() {
        let a = Point3d::new(3.0, 4.0, 5.0);
        let b = Point3d::new(1.0, 1.0, 1.0);
        let v = a - b;
        assert!((v.x - 2.0).abs() < 1e-12);
        assert!((v.y - 3.0).abs() < 1e-12);
        assert!((v.z - 4.0).abs() < 1e-12);
    }

    #[test]
    fn test_point_add_vector() {
        let p = Point3d::new(1.0, 2.0, 3.0);
        let v = Vec3::new(10.0, 20.0, 30.0);
        let result = p + v;
        assert!((result.x - 11.0).abs() < 1e-12);
        assert!((result.y - 22.0).abs() < 1e-12);
        assert!((result.z - 33.0).abs() < 1e-12);
    }

    #[test]
    fn test_lerp() {
        let a = Point3d::ORIGIN;
        let b = Point3d::new(10.0, 0.0, 0.0);
        let mid = a.lerp(&b, 0.5);
        assert!((mid.x - 5.0).abs() < 1e-12);
    }
}
