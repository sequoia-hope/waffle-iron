//! Stage-4 per-triangle `d(T)` recompute (Yang 2025 §4.1.2 / Fig 6) — the
//! certified discretization-error bound for boundary triangles.
//!
//! Closes deviation **N2** (`docs/yang_deviations.md`) increment 2. Yang 2025
//! §4.4.1 ends: "For the newly generated boundary triangles around the
//! intersection curve, we recalculate `d(T)` to maintain controllable error"
//! (`refs/text/yang2025_hybrid_boolean.txt:568-571`). `d(T)` itself is defined
//! in §4.1.2 / Fig 6 (`refs/text/yang2025_hybrid_boolean.txt:340-378`): take
//! the minimal parametric rectangle covering the triangle's three `uv`
//! corners, build the surface sub-patch on that rectangle, and compute the
//! maximal distance between the sub-patch's **control points** and the 3D
//! triangle. The convex-hull property of the positive-weight rational Bézier
//! net certifies the result as an upper bound on the true patch-to-triangle
//! max distance — never an estimate (spec §7).
//!
//! Our surfaces are analytic quadrics; all four curved [`Surface`] variants
//! are surfaces of revolution with EXACT closed-form rational Bézier nets
//! [#32 Piegl & Tiller ch. 8], so one constructor covers them all (spec §3).
//!
//! Spec: `specs/n2_stage4_dt_recompute.md`. Scope of increment 2: the pure
//! `d(T)` primitive plus its pinned parametric embedding [`eval_uv`],
//! unit-tested in isolation — **not** wired into
//! `stage4_relocate_and_correct` (that is N2-3).

use crate::{normalize3, ortho_basis, Surface};
use cad_primitives::{Point2, Point3, Vector3};
use std::f64::consts::{FRAC_PI_2, PI};

/// Why `d(T)` / [`eval_uv`] failed (spec §6). Every variant is a P9/P10 LOUD
/// stop — no clamping, no silent legalization.
#[derive(Debug, Clone, PartialEq)]
pub enum DtError {
    /// Any NaN/∞ in `uv` (or in `eval_uv`'s point argument) or in the
    /// surface's fields.
    NonFiniteInput,
    /// `radius <= 0`; cone `half_angle ∉ (0, π/2)`; torus
    /// `major_radius <= minor_radius` or `minor_radius <= 0`; zero
    /// `axis_dir`/`normal`.
    InvalidSurface,
    /// Covering-rectangle u-span `> 2π` for a curved surface — the caller
    /// handed coordinates from more than one period, so the covering
    /// rectangle is ambiguous. Unwrapping is the caller's job.
    AzimuthSpanTooLarge,
    /// Sphere v-range not within `[−π/2, π/2]`.
    PolarRangeOutOfBounds,
    /// Cone with any `v < 0` (behind the apex; the single-nappe solid
    /// convention of [`Surface::Cone`]).
    NegativeConeAxialRange,
}

/// Evaluate the pinned parametric embedding of `surface` at `p = (u, v)`
/// (spec §2). This defines what the `uv` coordinates handed to [`d_of_t`]
/// MEAN — Stage-1 sampling, the N2-1 patches and the Fig-6 bound must share
/// this one convention. All frames use the deterministic
/// [`crate::ortho_basis`] `(e1, e2)` (PR-YR7).
///
/// | Surface | `u` | `v` | `eval_uv(u, v)` |
/// |---|---|---|---|
/// | `Plane { normal, d }` | in-plane `e1` coord | in-plane `e2` coord | `(−d)·n̂ + u·e1 + v·e2`, `(e1, e2) = ortho_basis(normal)` |
/// | `Cylinder` | azimuth θ (rad) | axial offset h (world units) | `axis_point + h·â + r(cos u·e1 + sin u·e2)`, `â = normalize(axis_dir)` |
/// | `Cone` | azimuth θ (rad) | axial distance t ≥ 0 from apex | `apex + v·â + v·tan(half_angle)(cos u·e1 + sin u·e2)` |
/// | `Sphere` | azimuth θ (rad) | latitude φ ∈ [−π/2, π/2] | `center + r(cos v cos u·e1 + cos v sin u·e2 + sin v·ẑ)`, frame `ortho_basis(ẑ)` with the pinned canonical axis `ẑ = (0, 0, 1)` |
/// | `Torus` | azimuth θ (rad) | tube angle φ (rad) | `center + (R + r cos v)(cos u·e1 + sin u·e2) + r sin v·â` |
///
/// # Errors
///
/// [`DtError::NonFiniteInput`], [`DtError::InvalidSurface`],
/// [`DtError::PolarRangeOutOfBounds`], [`DtError::NegativeConeAxialRange`].
pub fn eval_uv(surface: &Surface, p: Point2) -> Result<Point3, DtError> {
    // Spec §6: finiteness first — a NaN/∞ coordinate is NonFiniteInput, never
    // a structural surface defect.
    if !p.x().is_finite() || !p.y().is_finite() {
        return Err(DtError::NonFiniteInput);
    }
    validate_surface(surface)?;
    validate_v_range(surface, p.y(), p.y())?;
    let q = eval_core(surface, p.x(), p.y());
    Ok(Point3::new(q[0], q[1], q[2]))
}

/// The Yang 2025 §4.1.2 / Fig 6 per-triangle discretization-error bound
/// `d(T)` for the triangle with parametric corners `uv` on `surface`
/// (spec §1/§3):
///
/// 1. Covering rectangle `[u0,u1]×[v0,v1]` of the three corners (Fig 6c);
///    degenerate rectangles are legal.
/// 2. Validate ranges (spec §6); subdivide so every sub-rectangle's angular
///    spans are ≤ π/2.
/// 3. Per sub-rectangle, build the exact rational Bézier control net of the
///    surface-of-revolution patch [#32 ch. 8].
/// 4. `d(T)` = max over ALL control points of all sub-rectangles of the
///    point-to-triangle distance to the 3D triangle
///    `[eval_uv(uv[0]), eval_uv(uv[1]), eval_uv(uv[2])]` (degenerate 3D
///    triangles degrade to segment/point distance — legal input).
///
/// The convex-hull property of the positive-weight rational net makes the
/// result a CERTIFIED upper bound on the true max distance from the surface
/// patch over the triangle's parametric footprint to the 3D triangle. A
/// `Plane` returns exactly `0.0`. Result is finite and `>= 0`, in world
/// units. Pure and deterministic; no tolerances — there is nothing to tune.
pub fn d_of_t(surface: &Surface, uv: [Point2; 3]) -> Result<f64, DtError> {
    // Spec §6: finiteness first (NonFiniteInput outranks every other error).
    for c in &uv {
        if !c.x().is_finite() || !c.y().is_finite() {
            return Err(DtError::NonFiniteInput);
        }
    }
    validate_surface(surface)?;

    // Covering rectangle `[u0,u1]×[v0,v1]` of the three corners (Fig 6c).
    // Zero-span (degenerate) rectangles are legal — the control net
    // degenerates to a curve/point net and the hull bound still holds
    // (spec §3 step 1).
    let u0 = uv[0].x().min(uv[1].x()).min(uv[2].x());
    let u1 = uv[0].x().max(uv[1].x()).max(uv[2].x());
    let v0 = uv[0].y().min(uv[1].y()).min(uv[2].y());
    let v1 = uv[0].y().max(uv[1].y()).max(uv[2].y());
    validate_v_range(surface, v0, v1)?;

    // Spec I2: a plane's sub-patch and triangle are coplanar — the Fig-6
    // bound is trivially and EXACTLY zero; no net is built. (This branch also
    // exempts planes from the azimuth-span check below: plane u/v are
    // unbounded in-plane coordinates, not angles.)
    let (frame, profile) = match *surface {
        Surface::Plane { .. } => return Ok(0.0),
        // Spec §3 branch table: profile generator in the (ρ radial, z axial)
        // half-plane. Cylinder: line ρ = r. Cone: line ρ = v·tan α.
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => (
            rev_frame(axis_point, axis_dir),
            Profile::Line {
                offset: radius,
                slope: 0.0,
            },
        ),
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => (
            rev_frame(apex, axis_dir),
            Profile::Line {
                offset: 0.0,
                slope: half_angle.tan(),
            },
        ),
        // Spec §2: a sphere has no intrinsic axis — the embedding pins the
        // canonical ẑ = (0, 0, 1). Profile: arc of radius r about the origin.
        Surface::Sphere { center, radius } => (
            rev_frame(center, Vector3::new(0.0, 0.0, 1.0)),
            Profile::Arc {
                center_rho: 0.0,
                r: radius,
            },
        ),
        // Torus profile: arc of radius r about (R, 0).
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } => (
            rev_frame(center, axis_dir),
            Profile::Arc {
                center_rho: major_radius,
                r: minor_radius,
            },
        ),
    };

    // Spec §6: u-span > 2π means the caller handed coordinates from more than
    // one period — the covering rectangle is ambiguous; unwrapping is the
    // caller's job. Span EXACTLY 2π is legal (a full turn is one period).
    let span_u = u1 - u0;
    if span_u > 2.0 * PI {
        return Err(DtError::AzimuthSpanTooLarge);
    }
    let span_v = v1 - v0;

    // The 3D triangle (Fig 6b): eval_uv of the three corners — mesh vertices
    // are on-surface by construction (spec §2), so corners are shared with
    // the patch.
    let mut tri = [[0.0_f64; 3]; 3];
    for (k, &c) in uv.iter().enumerate() {
        tri[k] = eval_uv(surface, c)?.as_array();
    }

    // Subdivision rationale (spec §3 step 2): the exact rational-arc
    // construction needs span < π, and capping every angular span at π/2
    // keeps the middle weight cos(span/2) ≥ √2/2 — comfortably positive, so
    // the convex-hull certificate below applies to every sub-rectangle. The
    // count is DERIVED (ceil(span / (π/2))), not a parameter. v is an ANGLE
    // only for sphere/torus; for cylinder/cone v is axial length and the
    // degree-1 profile line is exact over any span — no v subdivision.
    let n_u = ((span_u / FRAC_PI_2).ceil() as usize).max(1);
    let n_v = match surface {
        Surface::Sphere { .. } | Surface::Torus { .. } => {
            ((span_v / FRAC_PI_2).ceil() as usize).max(1)
        }
        _ => 1,
    };

    // Spec I1 (the certificate): every weight in the tensor-product rational
    // Bézier net is positive (endpoint weights 1; middle weights cos(Δ/2) ≥
    // √2/2 for spans ≤ π/2), so each sub-patch lies in the CONVEX HULL of its
    // control POINTS [#32 Piegl & Tiller §4.2 properties; ch. 8 surfaces of
    // revolution]. Point-to-triangle distance is convex in the point, so its
    // maximum over the hull — hence over the sub-patch — is attained at a
    // control point. The max over ALL control points of ALL sub-rectangles is
    // therefore a certified upper bound on the true patch-to-triangle max
    // distance over the triangle's parametric footprint (⊆ covering rect).
    //
    // Spec I5 (determinism): pure f64, fixed iteration order (u-major then
    // v), no hashing, no randomness.
    let mut d_max = 0.0_f64;
    let mut rows: Vec<[f64; 2]> = Vec::new();
    for iu in 0..n_u {
        let ua = u0 + span_u * (iu as f64) / (n_u as f64);
        let ub = u0 + span_u * ((iu + 1) as f64) / (n_u as f64);
        // Azimuth arc rows [#32 ch. 8]: endpoints on the surface; middle
        // control point at the TANGENT INTERSECTION — radially scaled by
        // 1/cos(Δu/2), weight cos(Δu/2). Δu = 0 degenerates cleanly
        // (cos 0 = 1: middle == endpoints).
        let du = ub - ua;
        let mu = 0.5 * (ua + ub);
        let u_scale = 1.0 / (0.5 * du).cos();
        let (cos_a, sin_a) = (ua.cos(), ua.sin());
        let (cos_m, sin_m) = (mu.cos(), mu.sin());
        let (cos_b, sin_b) = (ub.cos(), ub.sin());
        for iv in 0..n_v {
            let va = v0 + span_v * (iv as f64) / (n_v as f64);
            let vb = v0 + span_v * ((iv + 1) as f64) / (n_v as f64);
            profile_rows(&profile, va, vb, &mut rows);
            for &[rho, z] in rows.iter() {
                // Revolve the profile control point (ρ, z) through the
                // sub-rect's azimuth arc: endpoint / tangent-intersection
                // middle / endpoint (spec §3 step 3).
                for p in [
                    frame.ring(rho, cos_a, sin_a, z),
                    frame.ring(rho * u_scale, cos_m, sin_m, z),
                    frame.ring(rho, cos_b, sin_b, z),
                ] {
                    d_max = d_max.max(dist_point_tri(p, &tri));
                }
            }
        }
    }
    Ok(d_max)
}

// =========================================================================
// Internal helpers (validation, embedding core, control nets, distance).
// =========================================================================

fn finite3(v: [f64; 3]) -> bool {
    v[0].is_finite() && v[1].is_finite() && v[2].is_finite()
}

fn norm2(v: [f64; 3]) -> f64 {
    v[0] * v[0] + v[1] * v[1] + v[2] * v[2]
}

/// Spec §6 surface validation. NonFiniteInput checks strictly precede
/// InvalidSurface: a NaN radius is a non-finite input, not a structural
/// defect.
fn validate_surface(surface: &Surface) -> Result<(), DtError> {
    let (finite, valid) = match *surface {
        Surface::Plane { normal, d } => (
            finite3(normal.as_array()) && d.is_finite(),
            norm2(normal.as_array()) > 0.0,
        ),
        Surface::Sphere { center, radius } => (
            finite3(center.as_array()) && radius.is_finite(),
            radius > 0.0,
        ),
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => (
            finite3(axis_point.as_array()) && finite3(axis_dir.as_array()) && radius.is_finite(),
            radius > 0.0 && norm2(axis_dir.as_array()) > 0.0,
        ),
        // Cone half_angle must lie in the OPEN interval (0, π/2): 0 is a
        // degenerate line, π/2 a degenerate plane (spec §6).
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => (
            finite3(apex.as_array()) && finite3(axis_dir.as_array()) && half_angle.is_finite(),
            half_angle > 0.0 && half_angle < FRAC_PI_2 && norm2(axis_dir.as_array()) > 0.0,
        ),
        // Ring torus only: major > minor > 0 (spec §6; horn/spindle rejected).
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } => (
            finite3(center.as_array())
                && finite3(axis_dir.as_array())
                && major_radius.is_finite()
                && minor_radius.is_finite(),
            minor_radius > 0.0 && major_radius > minor_radius && norm2(axis_dir.as_array()) > 0.0,
        ),
    };
    if !finite {
        return Err(DtError::NonFiniteInput);
    }
    if !valid {
        return Err(DtError::InvalidSurface);
    }
    Ok(())
}

/// Spec §6 v-range checks, shared by [`eval_uv`] (a single point) and
/// [`d_of_t`] (the covering rectangle's v-extent).
fn validate_v_range(surface: &Surface, v_min: f64, v_max: f64) -> Result<(), DtError> {
    match surface {
        // Sphere latitude must stay within [−π/2, π/2] — INCLUSIVE: the poles
        // are legal, the azimuth ring degenerates gracefully (spec §5).
        Surface::Sphere { .. } if v_min < -FRAC_PI_2 || v_max > FRAC_PI_2 => {
            Err(DtError::PolarRangeOutOfBounds)
        }
        // Single-nappe cone: v is the axial distance from the apex, never
        // negative; v = 0 (the apex itself) is legal (spec §5).
        Surface::Cone { .. } if v_min < 0.0 => Err(DtError::NegativeConeAxialRange),
        _ => Ok(()),
    }
}

/// Revolution frame: datum point, unit axis `â`, and the deterministic
/// in-plane pair `(e1, e2) = ortho_basis(axis)` — the ONE frame convention
/// shared with Stage-1 sampling (PR-YR7; spec §2).
struct RevFrame {
    datum: [f64; 3],
    axis: [f64; 3],
    e1: [f64; 3],
    e2: [f64; 3],
}

fn rev_frame(datum: Point3, axis_dir: Vector3) -> RevFrame {
    let (e1, e2) = ortho_basis(axis_dir);
    RevFrame {
        datum: datum.as_array(),
        axis: normalize3(axis_dir.as_array()),
        e1: e1.as_array(),
        e2: e2.as_array(),
    }
}

impl RevFrame {
    /// `datum + z·â + ρ·(cos θ·e1 + sin θ·e2)` with the azimuth passed as
    /// precomputed `(cos θ, sin θ)` so control rows reuse one evaluation.
    fn ring(&self, rho: f64, cos_t: f64, sin_t: f64, z: f64) -> [f64; 3] {
        [
            self.datum[0] + z * self.axis[0] + rho * (cos_t * self.e1[0] + sin_t * self.e2[0]),
            self.datum[1] + z * self.axis[1] + rho * (cos_t * self.e1[1] + sin_t * self.e2[1]),
            self.datum[2] + z * self.axis[2] + rho * (cos_t * self.e1[2] + sin_t * self.e2[2]),
        ]
    }
}

/// Pinned parametric embedding core (spec §2 table) — callers have already
/// validated the surface and ranges. [`d_of_t`]'s endpoint control rows land
/// exactly on these values, which is what makes the triangle corners shared
/// patch/triangle points (I3's corner pin).
fn eval_core(surface: &Surface, u: f64, v: f64) -> [f64; 3] {
    match *surface {
        // Plane: (−d)·n̂ + u·e1 + v·e2 for the plane n̂·x + d = 0.
        Surface::Plane { normal, d } => {
            let n = normalize3(normal.as_array());
            let (e1, e2) = ortho_basis(normal);
            let (e1, e2) = (e1.as_array(), e2.as_array());
            [
                -d * n[0] + u * e1[0] + v * e2[0],
                -d * n[1] + u * e1[1] + v * e2[1],
                -d * n[2] + u * e1[2] + v * e2[2],
            ]
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => rev_frame(axis_point, axis_dir).ring(radius, u.cos(), u.sin(), v),
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => rev_frame(apex, axis_dir).ring(v * half_angle.tan(), u.cos(), u.sin(), v),
        // Sphere: pinned canonical axis ẑ = (0, 0, 1) (spec §2 — a sphere has
        // no intrinsic axis).
        Surface::Sphere { center, radius } => rev_frame(center, Vector3::new(0.0, 0.0, 1.0)).ring(
            radius * v.cos(),
            u.cos(),
            u.sin(),
            radius * v.sin(),
        ),
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } => rev_frame(center, axis_dir).ring(
            major_radius + minor_radius * v.cos(),
            u.cos(),
            u.sin(),
            minor_radius * v.sin(),
        ),
    }
}

/// Profile generator in the (ρ radial, z axial) half-plane (spec §3 branch
/// table). The revolved surface point is
/// `datum + z·â + ρ·(cos u·e1 + sin u·e2)`.
enum Profile {
    /// Cylinder (`ρ = r`) / cone (`ρ = v·tan α`): `ρ(v) = offset + slope·v`,
    /// `z = v`. The exact degree-1 net is the two endpoints, weights 1.
    Line { offset: f64, slope: f64 },
    /// Sphere (`center_rho = 0`) / torus (`center_rho = R`): circular arc of
    /// radius `r` about `(center_rho, 0)` in the (ρ, z) plane.
    Arc { center_rho: f64, r: f64 },
}

/// Profile control points `(ρ, z)` over the sub-rect's v-range `[va, vb]`
/// (spec §3 step 3). Weights are 1 (line, arc endpoints) and cos(Δv/2) (arc
/// middle) — all positive for Δv ≤ π/2, which the caller's subdivision
/// guarantees; the positions alone feed the convex-hull bound.
fn profile_rows(profile: &Profile, va: f64, vb: f64, rows: &mut Vec<[f64; 2]>) {
    rows.clear();
    match *profile {
        Profile::Line { offset, slope } => {
            rows.push([offset + slope * va, va]);
            rows.push([offset + slope * vb, vb]);
        }
        // Exact rational-quadratic arc [#32 Piegl & Tiller ch. 7]: endpoints
        // on the profile circle; middle control point at the TANGENT
        // INTERSECTION, radially scaled by 1/cos(Δv/2), weight cos(Δv/2).
        // Δv = 0 degenerates cleanly (cos 0 = 1: middle == endpoints).
        Profile::Arc { center_rho, r } => {
            let dv = vb - va;
            let m = 0.5 * (va + vb);
            let s = r / (0.5 * dv).cos();
            rows.push([center_rho + r * va.cos(), r * va.sin()]);
            rows.push([center_rho + s * m.cos(), s * m.sin()]);
            rows.push([center_rho + r * vb.cos(), r * vb.sin()]);
        }
    }
}

// ---- Robust point-to-triangle distance (spec §3 step 4). -----------------
// Standard region/clamp-based closest-point algorithm (Ericson, Real-Time
// Collision Detection §5.1.5): if the perpendicular foot lands inside a
// non-degenerate triangle, the plane distance is the answer; otherwise the
// closest point lies on an edge, and clamped point-segment distances cover
// edges AND vertices. Degenerate 3D triangles (collinear / coincident
// corners) skip the interior branch entirely and degrade gracefully to
// segment/point distance — legal input (spec §3).

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dist_point_seg(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> f64 {
    let ab = sub3(b, a);
    let ap = sub3(p, a);
    let len2 = dot3(ab, ab);
    // Zero-length segment (coincident endpoints) → point distance.
    let t = if len2 == 0.0 {
        0.0
    } else {
        (dot3(ap, ab) / len2).clamp(0.0, 1.0)
    };
    let d = [ap[0] - t * ab[0], ap[1] - t * ab[1], ap[2] - t * ab[2]];
    dot3(d, d).sqrt()
}

fn dist_point_tri(p: [f64; 3], tri: &[[f64; 3]; 3]) -> f64 {
    let [a, b, c] = *tri;
    let edge_min = dist_point_seg(p, a, b)
        .min(dist_point_seg(p, b, c))
        .min(dist_point_seg(p, c, a));
    let v0 = sub3(b, a);
    let v1 = sub3(c, a);
    let v2 = sub3(p, a);
    let n = cross3(v0, v1);
    let n2 = dot3(n, n);
    if n2 > 0.0 {
        // Barycentric coordinates of the perpendicular foot.
        let d00 = dot3(v0, v0);
        let d01 = dot3(v0, v1);
        let d11 = dot3(v1, v1);
        let d20 = dot3(v2, v0);
        let d21 = dot3(v2, v1);
        let denom = d00 * d11 - d01 * d01; // == n2 > 0
        let bv = (d11 * d20 - d01 * d21) / denom;
        let bw = (d00 * d21 - d01 * d20) / denom;
        if bv >= 0.0 && bw >= 0.0 && bv + bw <= 1.0 {
            // Interior foot: plane distance (≤ every edge distance).
            return dot3(v2, n).abs() / n2.sqrt();
        }
    }
    edge_min
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_primitives::Vector3;
    use std::f64::consts::PI;

    // ---- Surfaces under test (spec §5 oracles). --------------------------

    fn cyl_z() -> Surface {
        Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        }
    }

    fn cone_z() -> Surface {
        Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: 0.5,
        }
    }

    fn sphere_off() -> Surface {
        Surface::Sphere {
            center: Point3::new(0.1, 0.2, 0.3),
            radius: 2.0,
        }
    }

    fn torus_z() -> Surface {
        Surface::Torus {
            center: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            major_radius: 3.0,
            minor_radius: 1.0,
        }
    }

    fn uv(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> [Point2; 3] {
        [
            Point2::new(a.0, a.1),
            Point2::new(b.0, b.1),
            Point2::new(c.0, c.1),
        ]
    }

    /// The I3 canonical cylinder triangle: one 90° sub-rectangle, hand-derived
    /// `d(T) = √6/3`.
    fn canonical_tri() -> [Point2; 3] {
        uv((0.0, 0.0), (PI / 2.0, 0.0), (0.0, 1.0))
    }

    // ---- Test-local robust point-to-triangle distance oracle. ------------
    // Interior projection when the perpendicular foot lands inside the
    // triangle, else nearest edge/vertex (segment distances are clamped, so
    // vertices are covered). Degenerate 3D triangles fall through to the
    // edge minimum.

    fn sub(a: Point3, b: Point3) -> [f64; 3] {
        [a.x() - b.x(), a.y() - b.y(), a.z() - b.z()]
    }

    fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn point_segment_distance(p: Point3, a: Point3, b: Point3) -> f64 {
        let ab = sub(b, a);
        let ap = sub(p, a);
        let len2 = dot(ab, ab);
        let t = if len2 == 0.0 {
            0.0
        } else {
            (dot(ap, ab) / len2).clamp(0.0, 1.0)
        };
        let d = [ap[0] - t * ab[0], ap[1] - t * ab[1], ap[2] - t * ab[2]];
        dot(d, d).sqrt()
    }

    fn point_triangle_distance(p: Point3, a: Point3, b: Point3, c: Point3) -> f64 {
        let edge_min = point_segment_distance(p, a, b)
            .min(point_segment_distance(p, b, c))
            .min(point_segment_distance(p, c, a));
        let v0 = sub(b, a);
        let v1 = sub(c, a);
        let v2 = sub(p, a);
        let n = cross(v0, v1);
        let n2 = dot(n, n);
        if n2 > 0.0 {
            let d00 = dot(v0, v0);
            let d01 = dot(v0, v1);
            let d11 = dot(v1, v1);
            let d20 = dot(v2, v0);
            let d21 = dot(v2, v1);
            let denom = d00 * d11 - d01 * d01; // == n2 > 0
            let bv = (d11 * d20 - d01 * d21) / denom;
            let bw = (d00 * d21 - d01 * d20) / denom;
            let bu = 1.0 - bv - bw;
            if bu >= 0.0 && bv >= 0.0 && bw >= 0.0 {
                // Perpendicular foot is interior: plane distance.
                return dot(v2, n).abs() / n2.sqrt();
            }
        }
        edge_min
    }

    /// Spec I7: every Ok result is finite and `>= 0`.
    fn assert_i7(d: f64) {
        assert!(d.is_finite() && d >= 0.0, "I7 violated: d(T) = {d}");
    }

    /// Spec I1 (certification, the load-bearing invariant): dense barycentric
    /// grid (all `(i, j)` with `i + j <= 20`) over the uv triangle; every
    /// sample's distance to the 3D triangle must be `<= d_of_t(...) + 1e-12`.
    /// Returns the bound for further assertions.
    fn assert_i1_certified(surface: &Surface, tri: [Point2; 3]) -> f64 {
        const N: usize = 20;
        let d = d_of_t(surface, tri).expect("d_of_t must succeed on a legal triangle");
        assert_i7(d);
        let c: Vec<Point3> = tri
            .iter()
            .map(|&p| eval_uv(surface, p).expect("corner eval_uv must succeed"))
            .collect();
        for i in 0..=N {
            for j in 0..=(N - i) {
                let b0 = i as f64 / N as f64;
                let b1 = j as f64 / N as f64;
                let b2 = (N - i - j) as f64 / N as f64;
                let s = Point2::new(
                    b0 * tri[0].x() + b1 * tri[1].x() + b2 * tri[2].x(),
                    b0 * tri[0].y() + b1 * tri[1].y() + b2 * tri[2].y(),
                );
                let q = eval_uv(surface, s).expect("sample eval_uv must succeed");
                let dist = point_triangle_distance(q, c[0], c[1], c[2]);
                assert!(
                    dist <= d + 1e-12,
                    "I1 violated at grid ({i},{j}) uv=({}, {}): sample dist {dist} > d(T) {d}",
                    s.x(),
                    s.y()
                );
            }
        }
        d
    }

    // ---- Spec I2: any triangle on any Plane returns exactly 0.0. ---------
    #[test]
    fn i2_plane_returns_exact_zero() {
        let axis_aligned = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -2.0,
        };
        let oblique = Surface::Plane {
            normal: Vector3::new(1.0, 2.0, 3.0),
            d: 0.7,
        };
        for s in [axis_aligned, oblique] {
            assert_eq!(d_of_t(&s, uv((0.0, 0.0), (3.0, -1.0), (0.5, 4.0))), Ok(0.0));
            assert_eq!(
                d_of_t(&s, uv((-5.0, 2.0), (7.0, 2.5), (1.0, -3.0))),
                Ok(0.0)
            );
        }
    }

    // ---- Spec I3: canonical cylinder exactness, d(T) = √6/3. -------------
    // Also pins the eval_uv parameterization + ortho_basis frame: for axis
    // (0,0,1), ortho_basis gives e1 = (1,0,0), e2 = (0,1,0), so the three
    // corners land at (1,0,0), (0,1,0), (1,0,1).
    #[test]
    fn i3_canonical_cylinder_sqrt6_over_3() {
        let s = cyl_z();
        let tri = canonical_tri();
        let expect = [
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(1.0, 0.0, 1.0),
        ];
        for (k, e) in expect.iter().enumerate() {
            let got = eval_uv(&s, tri[k]).expect("corner eval_uv must succeed");
            assert!(
                (got.x() - e.x()).abs() <= 1e-15
                    && (got.y() - e.y()).abs() <= 1e-15
                    && (got.z() - e.z()).abs() <= 1e-15,
                "eval_uv corner {k}: got {got:?}, want {e:?} (frame/parameterization pin)"
            );
        }
        let d = d_of_t(&s, tri).expect("canonical triangle must succeed");
        assert_i7(d);
        let want = 6.0_f64.sqrt() / 3.0;
        assert!(
            (d - want).abs() <= 1e-12,
            "I3 violated: d(T) = {d}, want √6/3 = {want}"
        );
    }

    // ---- Spec I1 certification sweeps, one per curved surface type. ------
    // Each u-span exceeds π/2 to force azimuth subdivision (spec §5).
    #[test]
    fn i1_certified_cylinder_subdivided() {
        // Spec I1: u-span 2.0 > π/2.
        assert_i1_certified(&cyl_z(), uv((0.0, 0.0), (2.0, 0.0), (1.0, 1.5)));
    }

    #[test]
    fn i1_certified_cone_subdivided() {
        // Spec I1: u-span 1.8 > π/2; v > 0 throughout.
        assert_i1_certified(&cone_z(), uv((0.0, 0.5), (1.8, 0.5), (0.9, 2.0)));
    }

    #[test]
    fn i1_certified_sphere_subdivided() {
        // Spec I1: u-span 1.7 > π/2; v within [−π/2, π/2].
        assert_i1_certified(&sphere_off(), uv((0.0, -0.4), (1.7, -0.4), (0.8, 0.9)));
    }

    #[test]
    fn i1_certified_torus_subdivided() {
        // Spec I1: u-span 1.9 > π/2 AND v-span 1.8 > π/2 (both axes subdivide).
        assert_i1_certified(&torus_z(), uv((0.0, 0.0), (1.9, 0.3), (0.9, 1.8)));
    }

    // ---- Spec §5 "Sphere pole legality": corner at v = π/2 exactly. ------
    #[test]
    fn sphere_pole_corner_is_legal_and_certified() {
        // The azimuth ring degenerates at the pole; still finite + certified.
        assert_i1_certified(&sphere_off(), uv((0.2, PI / 2.0), (0.9, 0.3), (1.6, 0.5)));
    }

    // ---- Spec §5 "Cone near-apex": corner at v = 0 exactly (t >= 0). -----
    #[test]
    fn cone_apex_corner_is_legal_and_certified() {
        assert_i1_certified(&cone_z(), uv((0.0, 0.0), (1.0, 1.0), (0.3, 2.0)));
    }

    // ---- Spec I4: shrink monotonicity about the uv centroid. -------------
    #[test]
    fn i4_shrink_monotonicity() {
        let s = cyl_z();
        let tri = canonical_tri();
        let cx = (tri[0].x() + tri[1].x() + tri[2].x()) / 3.0;
        let cy = (tri[0].y() + tri[1].y() + tri[2].y()) / 3.0;
        let half = [
            Point2::new(cx + 0.5 * (tri[0].x() - cx), cy + 0.5 * (tri[0].y() - cy)),
            Point2::new(cx + 0.5 * (tri[1].x() - cx), cy + 0.5 * (tri[1].y() - cy)),
            Point2::new(cx + 0.5 * (tri[2].x() - cx), cy + 0.5 * (tri[2].y() - cy)),
        ];
        let d_full = d_of_t(&s, tri).expect("full triangle must succeed");
        let d_half = d_of_t(&s, half).expect("halved triangle must succeed");
        assert_i7(d_full);
        assert_i7(d_half);
        assert!(
            d_half < d_full,
            "I4 violated: d(half) = {d_half} not strictly < d(full) = {d_full}"
        );
    }

    // ---- Spec I5: determinism (bit-identical repeat invocation). ----------
    #[test]
    fn i5_deterministic_bitwise() {
        let s = torus_z();
        let tri = uv((0.0, 0.0), (1.9, 0.3), (0.9, 1.8));
        let a = d_of_t(&s, tri).expect("first call must succeed");
        let b = d_of_t(&s, tri).expect("second call must succeed");
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "I5 violated: {a} vs {b} differ bitwise"
        );
    }

    // ---- Spec I6: rigid-motion (translation) sanity. ----------------------
    #[test]
    fn i6_translation_invariance() {
        // Cylinder: translate axis_point by (1, 2, 3); uv unchanged.
        let tri = canonical_tri();
        let d0 = d_of_t(&cyl_z(), tri).expect("origin cylinder must succeed");
        let moved = Surface::Cylinder {
            axis_point: Point3::new(1.0, 2.0, 3.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let d1 = d_of_t(&moved, tri).expect("moved cylinder must succeed");
        assert!(
            (d0 - d1).abs() < 1e-9,
            "I6 violated (cylinder): {d0} vs {d1}"
        );
        // Sphere: translate center by (1, 2, 3); uv unchanged.
        let stri = uv((0.0, -0.4), (1.7, -0.4), (0.8, 0.9));
        let d2 = d_of_t(&sphere_off(), stri).expect("sphere must succeed");
        let smoved = Surface::Sphere {
            center: Point3::new(1.1, 2.2, 3.3),
            radius: 2.0,
        };
        let d3 = d_of_t(&smoved, stri).expect("moved sphere must succeed");
        assert!((d2 - d3).abs() < 1e-9, "I6 violated (sphere): {d2} vs {d3}");
    }

    // ---- Failure modes (spec §6), exact variants. --------------------------
    #[test]
    fn rejects_non_finite_input() {
        // NaN in a uv coordinate.
        assert_eq!(
            d_of_t(&cyl_z(), uv((f64::NAN, 0.0), (1.0, 0.0), (0.5, 1.0))),
            Err(DtError::NonFiniteInput)
        );
        // NaN in a surface field.
        let bad = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: f64::NAN,
        };
        assert_eq!(
            d_of_t(&bad, uv((0.0, 0.0), (1.0, 0.0), (0.5, 1.0))),
            Err(DtError::NonFiniteInput)
        );
    }

    #[test]
    fn rejects_invalid_surface() {
        let tri = uv((0.0, 0.0), (1.0, 0.0), (0.5, 1.0));
        // Cylinder radius 0.
        let zero_r = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 0.0,
        };
        assert_eq!(d_of_t(&zero_r, tri), Err(DtError::InvalidSurface));
        // Cone half_angle at both ends of the open interval (0, π/2).
        let cone0 = Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: 0.0,
        };
        assert_eq!(d_of_t(&cone0, tri), Err(DtError::InvalidSurface));
        let cone90 = Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle: PI / 2.0,
        };
        assert_eq!(d_of_t(&cone90, tri), Err(DtError::InvalidSurface));
        // Torus major_radius == minor_radius (horn torus).
        let horn = Surface::Torus {
            center: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            major_radius: 1.0,
            minor_radius: 1.0,
        };
        assert_eq!(d_of_t(&horn, tri), Err(DtError::InvalidSurface));
        // Zero axis_dir.
        let zero_axis = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 0.0),
            radius: 1.0,
        };
        assert_eq!(d_of_t(&zero_axis, tri), Err(DtError::InvalidSurface));
    }

    #[test]
    fn rejects_azimuth_span_too_large() {
        // u-span 2π + 0.1 > 2π: more than one period, ambiguous covering rect.
        assert_eq!(
            d_of_t(&cyl_z(), uv((0.0, 0.0), (2.0 * PI + 0.1, 0.0), (1.0, 1.0))),
            Err(DtError::AzimuthSpanTooLarge)
        );
    }

    #[test]
    fn rejects_polar_range_out_of_bounds() {
        // Sphere corner at v = π/2 + 0.01 (beyond the pole).
        assert_eq!(
            d_of_t(
                &sphere_off(),
                uv((0.0, PI / 2.0 + 0.01), (0.5, 0.0), (1.0, 0.2))
            ),
            Err(DtError::PolarRangeOutOfBounds)
        );
    }

    #[test]
    fn rejects_negative_cone_axial_range() {
        // Cone corner at v = −0.01 (behind the apex).
        assert_eq!(
            d_of_t(&cone_z(), uv((0.0, -0.01), (0.5, 1.0), (1.0, 0.5))),
            Err(DtError::NegativeConeAxialRange)
        );
    }

    #[test]
    fn eval_uv_rejects_non_finite_point() {
        assert_eq!(
            eval_uv(&cyl_z(), Point2::new(f64::NAN, 0.0)),
            Err(DtError::NonFiniteInput)
        );
    }

    #[test]
    fn eval_uv_rejects_polar_out_of_bounds() {
        assert_eq!(
            eval_uv(&sphere_off(), Point2::new(0.0, PI / 2.0 + 0.01)),
            Err(DtError::PolarRangeOutOfBounds)
        );
    }

    // ---- Boundary-legal: u-span EXACTLY 2π is Ok (only > 2π errors). ------
    #[test]
    fn full_turn_azimuth_span_is_legal() {
        let d = d_of_t(&cyl_z(), uv((0.0, 0.0), (2.0 * PI, 0.0), (PI, 1.0)))
            .expect("span == 2π must be legal");
        assert_i7(d);
    }
}
