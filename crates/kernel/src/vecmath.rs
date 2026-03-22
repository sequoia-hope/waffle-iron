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

/// Compute an orthonormal basis (u, v) for a plane given its normal.
pub(crate) fn compute_plane_basis(normal: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let up = if normal[0].abs() < crate::units::BASIS_AXIS_ALIGNMENT {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let u = v3_normalize(v3_cross(normal, up));
    let v = v3_cross(normal, u);
    (u, v)
}

/// Newell method for computing polygon normal from vertex loop.
pub(crate) fn compute_newell_normal(verts: &[[f64; 3]]) -> [f64; 3] {
    let n = verts.len();
    let mut newell = [0.0f64; 3];
    for i in 0..n {
        let curr = verts[i];
        let next = verts[(i + 1) % n];
        newell[0] += (curr[1] - next[1]) * (curr[2] + next[2]);
        newell[1] += (curr[2] - next[2]) * (curr[0] + next[0]);
        newell[2] += (curr[0] - next[0]) * (curr[1] + next[1]);
    }
    v3_normalize(newell)
}

/// Compute centroid (average) of a set of vertices.
pub(crate) fn compute_centroid(verts: &[[f64; 3]]) -> [f64; 3] {
    let n = verts.len() as f64;
    if n < 1.0 {
        return [0.0; 3];
    }
    let mut sum = [0.0; 3];
    for v in verts {
        sum[0] += v[0];
        sum[1] += v[1];
        sum[2] += v[2];
    }
    [sum[0] / n, sum[1] / n, sum[2] / n]
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

/// Multiply two 3x3 matrices: result = a * b (row-major).
pub(crate) fn mat3_mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    r
}

/// Transpose of a 3x3 matrix (inverse for orthonormal rotation matrices).
pub(crate) fn mat3_transpose(m: &Mat3) -> Mat3 {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}
