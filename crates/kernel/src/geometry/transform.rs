//! Affine transforms in 3D space.

use super::point::{Point3, Vector3};

/// A 4x4 affine transformation matrix (column-major).
#[derive(Debug, Clone)]
pub struct Transform {
    /// Column-major 4x4 matrix.
    pub m: [f64; 16],
}

impl Transform {
    pub fn identity() -> Self {
        Self {
            m: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    pub fn translation(dx: f64, dy: f64, dz: f64) -> Self {
        Self {
            m: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, dx, dy, dz, 1.0,
            ],
        }
    }

    pub fn apply_point(&self, p: Point3) -> Point3 {
        Point3::new(
            self.m[0] * p.x + self.m[4] * p.y + self.m[8] * p.z + self.m[12],
            self.m[1] * p.x + self.m[5] * p.y + self.m[9] * p.z + self.m[13],
            self.m[2] * p.x + self.m[6] * p.y + self.m[10] * p.z + self.m[14],
        )
    }

    pub fn apply_vector(&self, v: Vector3) -> Vector3 {
        Vector3::new(
            self.m[0] * v.x + self.m[4] * v.y + self.m[8] * v.z,
            self.m[1] * v.x + self.m[5] * v.y + self.m[9] * v.z,
            self.m[2] * v.x + self.m[6] * v.y + self.m[10] * v.z,
        )
    }
}
