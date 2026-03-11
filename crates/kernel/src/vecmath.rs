//! Shared vector math helpers for [f64; 3] arrays.
//!
//! Canonical implementations used by boolean, tessellation, and waffle_kernel modules.

use crate::units::TAU_NORMALIZE;

pub(crate) fn v3_sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn v3_add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn v3_scale(v: [f64; 3], s: f64) -> [f64; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

pub(crate) fn v3_dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn v3_cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn v3_length(v: [f64; 3]) -> f64 {
    v3_dot(v, v).sqrt()
}

pub(crate) fn v3_negate(v: [f64; 3]) -> [f64; 3] {
    [-v[0], -v[1], -v[2]]
}

pub(crate) fn v3_normalize(v: [f64; 3]) -> [f64; 3] {
    let len = v3_length(v);
    if len < TAU_NORMALIZE {
        v
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

/// 3x3 rotation matrix stored row-major.
pub(crate) type Mat3 = [[f64; 3]; 3];

pub(crate) const MAT3_IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Apply a 3x3 rotation matrix to a vector.
pub(crate) fn mat3_mul_vec(m: &Mat3, v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// Transpose of a 3x3 matrix (inverse for orthonormal rotation matrices).
pub(crate) fn mat3_transpose(m: &Mat3) -> Mat3 {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}
