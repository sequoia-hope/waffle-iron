//! Ellipse & hyperbola exact parametrization for the from_yang output-curve
//! vocabulary (move-only F9 split from `geom.rs`; byte-identical): parametric
//! CCW sweep, param solve, point-at, and the hyperbola branch residual used by
//! endpoint certification. See `super`'s module docs.

use super::*;

/// Parametric CCW sweep of an ellipse arc from `p0` to `p1` around the
/// directional `normal`, in the frame `P(t) = c + a·cos t·m̂ + b·sin t·(n̂×m̂)`
/// — unique in `(0, 2π)`. `None` when an endpoint projects degenerately.
pub(crate) fn ellipse_ccw_sweep(
    center: Point3,
    normal: [f64; 3],
    major_axis: [f64; 3],
    major_radius: f64,
    minor_radius: f64,
    p0: Point3,
    p1: Point3,
) -> Option<f64> {
    let t0 = ellipse_param(center, normal, major_axis, major_radius, minor_radius, p0)?;
    let t1 = ellipse_param(center, normal, major_axis, major_radius, minor_radius, p1)?;
    let tau = 2.0 * std::f64::consts::PI;
    Some((t1 - t0).rem_euclid(tau))
}

/// Ellipse parameter of an (on-ellipse) point in the directional frame:
/// `t = atan2(v/b, u/a)` with `u = (p−c)·m̂`, `v = (p−c)·(n̂×m̂)`.
pub(crate) fn ellipse_param(
    center: Point3,
    normal: [f64; 3],
    major_axis: [f64; 3],
    major_radius: f64,
    minor_radius: f64,
    p: Point3,
) -> Option<f64> {
    if !(major_radius > 0.0 && minor_radius > 0.0) {
        return None;
    }
    let m = major_axis;
    let w = [
        normal[1] * m[2] - normal[2] * m[1],
        normal[2] * m[0] - normal[0] * m[2],
        normal[0] * m[1] - normal[1] * m[0],
    ];
    let d = [p.x() - center.x(), p.y() - center.y(), p.z() - center.z()];
    let u = (d[0] * m[0] + d[1] * m[1] + d[2] * m[2]) / major_radius;
    let v = (d[0] * w[0] + d[1] * w[1] + d[2] * w[2]) / minor_radius;
    if u.hypot(v) < 0.5 {
        return None; // not near the ellipse — degenerate projection
    }
    Some(v.atan2(u))
}

/// Point of the directional ellipse frame at parameter `t`.
pub(crate) fn ellipse_point_at(
    center: Point3,
    normal: [f64; 3],
    major_axis: [f64; 3],
    major_radius: f64,
    minor_radius: f64,
    t: f64,
) -> Point3 {
    let m = major_axis;
    let w = [
        normal[1] * m[2] - normal[2] * m[1],
        normal[2] * m[0] - normal[0] * m[2],
        normal[0] * m[1] - normal[1] * m[0],
    ];
    let (s, c) = t.sin_cos();
    Point3::new(
        center.x() + major_radius * c * m[0] + minor_radius * s * w[0],
        center.y() + major_radius * c * m[1] + minor_radius * s * w[1],
        center.z() + major_radius * c * m[2] + minor_radius * s * w[2],
    )
}

/// Point of the hyperbola branch at parameter `t` (KV16, spec
/// `kv16_hyperbola_arc_vocabulary`): `P(t) = c + a·cosh t·m̂ + b·sinh t·(n̂×m̂)`
/// — the single `+major_axis` (`u > 0`) branch, matching
/// `yang_rs::geom::hyperbola_point` / `ssi_rs::SsiCurve::Hyperbola`
/// field-for-field ([#1] Patrikalakis Ch.5 conic sections).
pub(crate) fn hyperbola_point_at(
    center: Point3,
    normal: [f64; 3],
    major_axis: [f64; 3],
    semi_transverse: f64,
    semi_conjugate: f64,
    t: f64,
) -> Point3 {
    let m = major_axis;
    let w = [
        normal[1] * m[2] - normal[2] * m[1],
        normal[2] * m[0] - normal[0] * m[2],
        normal[0] * m[1] - normal[1] * m[0],
    ];
    let (ch, sh) = (t.cosh(), t.sinh());
    Point3::new(
        center.x() + semi_transverse * ch * m[0] + semi_conjugate * sh * w[0],
        center.y() + semi_transverse * ch * m[1] + semi_conjugate * sh * w[1],
        center.z() + semi_transverse * ch * m[2] + semi_conjugate * sh * w[2],
    )
}

/// Parameter of an (on-branch) point of the hyperbola frame:
/// `t = asinh(v/b)` with `v = (p−c)·(n̂×m̂)`. `sinh` is injective along the
/// branch, so — unlike the ellipse's `atan2` — no quadrant or branch
/// reconciliation is needed. `None` for a non-positive conjugate semi-axis.
/// (Being ON the branch is validated separately via
/// [`hyperbola_branch_residual`]; this projection alone does not certify it.)
pub(crate) fn hyperbola_param(
    center: Point3,
    normal: [f64; 3],
    major_axis: [f64; 3],
    semi_conjugate: f64,
    p: Point3,
) -> Option<f64> {
    if !(semi_conjugate > 0.0 && semi_conjugate.is_finite()) {
        return None;
    }
    let m = major_axis;
    let w = [
        normal[1] * m[2] - normal[2] * m[1],
        normal[2] * m[0] - normal[0] * m[2],
        normal[0] * m[1] - normal[1] * m[0],
    ];
    let d = [p.x() - center.x(), p.y() - center.y(), p.z() - center.z()];
    let v = (d[0] * w[0] + d[1] * w[1] + d[2] * w[2]) / semi_conjugate;
    if !v.is_finite() {
        return None;
    }
    Some(v.asinh())
}

/// On-branch residual of `p` against the `u > 0` hyperbola branch (KV16):
/// `(in_plane_dist, out_of_plane, u)`, where `in_plane_dist` is the
/// first-order distance `|g| / |∇g|` of the in-plane implicit
/// `g = (u/a)² − (v/b)² − 1` (`∇g` in the scaled in-plane coordinates —
/// the honest length conversion of a signless quadric residual), and `u`
/// lets the caller reject the wrong nappe (`u ≤ 0`). `in_plane_dist` is
/// `+∞` at the center (gradient degenerate — certainly off-branch).
pub(crate) fn hyperbola_branch_residual(
    center: Point3,
    normal: [f64; 3],
    major_axis: [f64; 3],
    semi_transverse: f64,
    semi_conjugate: f64,
    p: Point3,
) -> (f64, f64, f64) {
    let m = major_axis;
    let w = [
        normal[1] * m[2] - normal[2] * m[1],
        normal[2] * m[0] - normal[0] * m[2],
        normal[0] * m[1] - normal[1] * m[0],
    ];
    let d = [p.x() - center.x(), p.y() - center.y(), p.z() - center.z()];
    let u = d[0] * m[0] + d[1] * m[1] + d[2] * m[2];
    let v = d[0] * w[0] + d[1] * w[1] + d[2] * w[2];
    let out_of_plane = d[0] * normal[0] + d[1] * normal[1] + d[2] * normal[2];
    let (a, b) = (semi_transverse, semi_conjugate);
    let g = (u / a).powi(2) - (v / b).powi(2) - 1.0;
    let grad = 2.0 * (u / (a * a)).hypot(v / (b * b));
    let in_plane = if grad > 0.0 && grad.is_finite() {
        (g / grad).abs()
    } else {
        f64::INFINITY
    };
    (in_plane, out_of_plane, u)
}
