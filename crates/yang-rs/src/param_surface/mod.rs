//! The parameter-domain surface interface (roadmap F5a; spec
//! `specs/yang_f5a_surface_interface.md`).
//!
//! Yang 2025's hybrid representation lives in the parameter domain: Stage 1
//! samples each patch in (u, v) (`refs/text/yang2025_hybrid_boolean.txt:371-374`),
//! a boundary triangle's error bound is the control-net distance of the
//! rational Bézier sub-patch over its covering parameter rectangle (§4.1.2,
//! Fig. 6, `:358-378`; Theorem A.1), and the Gauss-map test bounds a
//! sub-patch's normals by a cone built from its control net (§4.2.2,
//! Theorem 4.1, `:446-466`; Appendix B, `:1206-1236`).
//!
//! [`ParamSurface`] is the one interface every stage reaches a surface
//! through. The two bounds are generic functions over
//! [`ParamSurface::bezier_patches`], not trait methods, so an instance cannot
//! substitute its own answer: [`control_net_distance`] (d(T)) and
//! [`normal_cone`] (Theorem 4.1). The five analytic surfaces are its
//! instances (`analytic.rs`), on the embedding `stage4_dt::eval_uv` pins.
//!
//! Increment 1: unwired. Nothing in the production path calls this module
//! yet; its pins hold it to `stage4_dt` bit for bit.
#![cfg_attr(not(test), allow(dead_code))]

mod analytic;
pub(crate) mod carried;
#[cfg(test)]
mod tests;

use cad_primitives::Point2;
use std::f64::consts::PI;

/// A point of a surface's parameter domain.
pub(crate) type Uv = Point2;

/// A closed parameter rectangle `[u0, u1] × [v0, v1]` (`u0 <= u1`,
/// `v0 <= v1`; a zero span is legal).
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct ParamRect {
    pub u0: f64,
    pub u1: f64,
    pub v0: f64,
    pub v1: f64,
}

impl ParamRect {
    /// The smallest rectangle covering `pts` (Fig. 6(c)).
    pub(crate) fn covering(pts: &[Uv]) -> Self {
        let mut r = ParamRect {
            u0: f64::INFINITY,
            u1: f64::NEG_INFINITY,
            v0: f64::INFINITY,
            v1: f64::NEG_INFINITY,
        };
        for p in pts {
            r.u0 = r.u0.min(p.x());
            r.u1 = r.u1.max(p.x());
            r.v0 = r.v0.min(p.y());
            r.v1 = r.v1.max(p.y());
        }
        r
    }

    fn is_finite(&self) -> bool {
        self.u0.is_finite() && self.u1.is_finite() && self.v0.is_finite() && self.v1.is_finite()
    }

    /// The four quarters, for subdivision.
    fn quarters(&self) -> [ParamRect; 4] {
        let um = 0.5 * (self.u0 + self.u1);
        let vm = 0.5 * (self.v0 + self.v1);
        [
            ParamRect {
                u0: self.u0,
                u1: um,
                v0: self.v0,
                v1: vm,
            },
            ParamRect {
                u0: um,
                u1: self.u1,
                v0: self.v0,
                v1: vm,
            },
            ParamRect {
                u0: self.u0,
                u1: um,
                v0: vm,
                v1: self.v1,
            },
            ParamRect {
                u0: um,
                u1: self.u1,
                v0: vm,
                v1: self.v1,
            },
        ]
    }
}

/// One axis of the parameter domain.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct ParamAxis {
    /// `−∞` on an unbounded axis.
    pub lo: f64,
    /// `+∞` on an unbounded axis.
    pub hi: f64,
    /// `Some(2π)` on an angular axis; its seam is `lo ≡ hi`.
    pub period: Option<f64>,
}

/// An iso-line the parametrization collapses to one point (a sphere pole, a
/// cone apex). The normal is undefined there.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum Degeneracy {
    // No analytic instance degenerates along a u-iso-line; a B-spline may.
    #[allow(dead_code)]
    UIso(f64),
    VIso(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParamDomain {
    pub u: ParamAxis,
    pub v: ParamAxis,
    pub degenerate: Vec<Degeneracy>,
}

/// `s` and its first and second partial derivatives at one parameter point.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct SurfaceDerivs {
    pub s: [f64; 3],
    pub su: [f64; 3],
    pub sv: [f64; 3],
    pub suu: [f64; 3],
    pub suv: [f64; 3],
    pub svv: [f64; 3],
}

/// A tensor-product rational Bézier patch: `(deg_u + 1) × (deg_v + 1)`
/// control points, index `i * (deg_v + 1) + j` for u-index `i`, v-index `j`,
/// every weight positive.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RationalBezierPatch {
    pub deg_u: usize,
    pub deg_v: usize,
    pub points: Vec<[f64; 3]>,
    pub weights: Vec<f64>,
    /// The weights factor as `w_ij = a_i · b_j` (true of every
    /// surface-of-revolution net and of every polynomial patch). Then the
    /// partial derivatives lie in the cones of the same-row control-point
    /// differences; otherwise [`normal_cone`] takes the cross-row ones too.
    pub separable_weights: bool,
}

impl RationalBezierPatch {
    fn point(&self, i: usize, j: usize) -> [f64; 3] {
        self.points[i * (self.deg_v + 1) + j]
    }
}

/// Why a surface query failed. Every variant is loud; nothing clamps.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum SurfaceError {
    /// A NaN or infinite coordinate in the query or in the surface's fields.
    NonFiniteInput,
    /// The surface's own fields are invalid (zero axis, radius ≤ 0, cone
    /// half-angle outside (0, π/2), a non-ring torus).
    InvalidSurface,
    /// The query lies outside the parameter domain (a sphere latitude past a
    /// pole, a cone parameter behind the apex).
    OutsideDomain,
    /// A rectangle spans more than one period of a periodic axis; which
    /// copy is meant is ambiguous. Unwrapping is the caller's job.
    SpanExceedsPeriod,
    /// The parametrization is degenerate at the query (a pole, an apex):
    /// `s_u × s_v = 0`.
    Degenerate,
    /// [`normal_cone`]: Theorem 4.1's hypothesis (the two derivative cones
    /// are half-cones whose complete cones do not overlap) still fails after
    /// the rectangle was subdivided [`NORMAL_CONE_MAX_DEPTH`] times.
    ConeHypothesis,
}

/// A surface reached through its parameter domain.
pub(crate) trait ParamSurface {
    /// The surface's own fields are valid.
    fn validate(&self) -> Result<(), SurfaceError>;
    fn domain(&self) -> ParamDomain;
    /// `true` when the parametrization is affine (a plane): the image of a
    /// parameter triangle is exactly the 3D triangle through its corners,
    /// so d(T) is exactly 0.
    fn is_affine(&self) -> bool {
        false
    }
    /// The plane `n·x + d = 0` an affine surface spans, as the instance
    /// stores it (`n` need not be unit); `None` exactly when `!is_affine()`.
    fn affine_plane(&self) -> Option<(cad_primitives::Vector3, f64)> {
        None
    }
    fn eval(&self, uv: Uv) -> Result<[f64; 3], SurfaceError>;
    fn derivs(&self, uv: Uv) -> Result<SurfaceDerivs, SurfaceError>;
    /// Unit `s_u × s_v`. For every analytic instance this is the OUTWARD
    /// normal of the solid the surface bounds; a face's `reversed` flag is
    /// the caller's. `Err(Degenerate)` where `s_u × s_v = 0`.
    fn normal(&self, uv: Uv) -> Result<[f64; 3], SurfaceError> {
        let d = self.derivs(uv)?;
        let n = cross3(d.su, d.sv);
        let len = dot3(n, n).sqrt();
        if len == 0.0 || !len.is_finite() {
            return Err(SurfaceError::Degenerate);
        }
        Ok([n[0] / len, n[1] / len, n[2] / len])
    }
    /// The surface over `rect` as rational Bézier patches with positive
    /// weights whose union is the surface over `rect`.
    fn bezier_patches(&self, rect: ParamRect) -> Result<Vec<RationalBezierPatch>, SurfaceError>;
}

/// The closed-form inverse `p ↦ (u, v)` of an analytic instance: a recorded
/// fast path, NOT part of [`ParamSurface`] (spec
/// `specs/yang_f5a_surface_interface.md` §5; ledger N109). The paper never
/// projects: Stage 1 samples in (u, v) and knows every vertex's parameters
/// (`refs/text/yang2025_hybrid_boolean.txt:371-374`). The inverse only
/// assigns coordinates to points the pipeline built in 3D.
pub(crate) trait AnalyticInverse {
    /// The parameters of the surface point closest to `p`.
    ///
    /// A periodic coordinate is returned on the branch nearest `seed`'s, or
    /// in `[lo, lo + period)` without a seed. Where the closest point does
    /// not fix a coordinate (`p` on a revolution axis, a sphere's centre, a
    /// torus's tube circle) that coordinate is `seed`'s, or the axis's `lo`
    /// (0 on an unbounded axis) without one. `Err(OutsideDomain)` when the
    /// closest point of the implicit surface lies outside the domain (a cone
    /// point nearer the nappe behind the apex); nothing clamps.
    fn inverse(&self, p: [f64; 3], seed: Option<Uv>) -> Result<Uv, SurfaceError>;
}

/// `a` moved by whole periods onto the branch nearest `seed`, or into
/// `[lo, lo + period)` without one.
pub(crate) fn wrap_to_branch(a: f64, axis: &ParamAxis, seed: Option<f64>) -> f64 {
    let Some(period) = axis.period else {
        return a;
    };
    match seed {
        Some(s) if s.is_finite() => a + period * ((s - a) / period).round(),
        _ => {
            let r = (a - axis.lo).rem_euclid(period);
            // `rem_euclid` can round up to `period` itself for a tiny
            // negative input; that is the seam, `lo`.
            axis.lo + if r >= period { 0.0 } else { r }
        }
    }
}

/// d(T) of Yang 2025 §4.1.2 / Fig. 6 (`:358-378`): the largest distance from
/// any control point of the sub-patch over the covering rectangle of the
/// three parameter corners to the 3D triangle through the evaluated corners.
/// By the convex-hull property of a positive-weight rational net (Theorem
/// A.1) it is a certified upper bound on the distance from the surface over
/// the triangle's footprint to the triangle. An affine parametrization (a
/// plane) returns exactly 0.
pub(crate) fn control_net_distance<S: ParamSurface + ?Sized>(
    surface: &S,
    uv: [Uv; 3],
) -> Result<f64, SurfaceError> {
    if uv.iter().any(|c| !c.x().is_finite() || !c.y().is_finite()) {
        return Err(SurfaceError::NonFiniteInput);
    }
    surface.validate()?;
    if surface.is_affine() {
        return Ok(0.0);
    }
    let patches = surface.bezier_patches(ParamRect::covering(&uv))?;
    let mut tri = [[0.0_f64; 3]; 3];
    for (k, &c) in uv.iter().enumerate() {
        tri[k] = surface.eval(c)?;
    }
    let mut d_max = 0.0_f64;
    for p in &patches {
        for &q in &p.points {
            d_max = d_max.max(dist_point_tri(q, &tri));
        }
    }
    Ok(d_max)
}

/// A circular cone of directions: every vector within `half_angle` of
/// `axis` (unit). `half_angle` may reach π (every direction).
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct NormalCone {
    pub axis: [f64; 3],
    pub half_angle: f64,
}

impl NormalCone {
    /// `v` (any length > 0) lies in the cone.
    pub(crate) fn contains(&self, v: [f64; 3]) -> bool {
        angle_between(self.axis, v) <= self.half_angle
    }

    /// The smallest cone along the great circle through both axes that
    /// contains both cones.
    pub(crate) fn union(&self, other: &NormalCone) -> NormalCone {
        let phi = angle_between(self.axis, other.axis);
        if phi + other.half_angle <= self.half_angle {
            return *self;
        }
        if phi + self.half_angle <= other.half_angle {
            return *other;
        }
        let half = 0.5 * (phi + self.half_angle + other.half_angle);
        if half >= PI {
            return NormalCone {
                axis: self.axis,
                half_angle: PI,
            };
        }
        // Rotate `self.axis` toward `other.axis` by `half − self.half_angle`.
        let t = half - self.half_angle;
        let w = sub3(other.axis, scale3(self.axis, dot3(self.axis, other.axis)));
        let w_len = dot3(w, w).sqrt();
        let axis = if w_len == 0.0 {
            // Antipodal axes: any great circle serves; the union is all
            // directions within `half` of `self.axis` rotated by `t`, which
            // a conservative full cone covers.
            return NormalCone {
                axis: self.axis,
                half_angle: PI,
            };
        } else {
            let w = scale3(w, 1.0 / w_len);
            let a = add3(scale3(self.axis, t.cos()), scale3(w, t.sin()));
            unit(a).unwrap_or(self.axis)
        };
        NormalCone {
            axis,
            half_angle: half + FP_ANGLE_GUARD,
        }
    }
}

/// How many times [`normal_cone`] quarters a rectangle whose patches fail
/// Theorem 4.1's hypothesis before it refuses. Each level halves both
/// spans; the hypothesis is met once a patch's derivative directions vary
/// by less than the angle between them, which halving reaches on any
/// regular patch.
pub(crate) const NORMAL_CONE_MAX_DEPTH: usize = 8;

/// IEEE-754 angular rounding guard (`64 · ε_mach` rad) added to every cone
/// angle this module computes, so rounding in `atan2` and the frame cannot
/// make a cone exclude a normal it contains. A rounding bound, not a
/// geometric tolerance (the same guard as `gauss_map.rs`).
const FP_ANGLE_GUARD: f64 = 64.0 * f64::EPSILON;

/// Theorem 4.1 (`:446-466`; proof Appendix B, `:1206-1236`): a cone holding
/// every normal of `surface` over `rect`.
///
/// Per patch, `C_u` bounds the directions of `s_u` and `C_v` those of
/// `s_v`. A rational Bézier curve's derivative is a non-negative combination
/// of its control-point differences `P_k − P_i`, `i < k` (Sederberg & Wang
/// 1987); with separable weights the tensor-product partial derivative is a
/// non-negative combination of the same-row differences, otherwise of every
/// cross-row pair. Theorem 4.1 needs the complete cones `±C_u`, `±C_v` not
/// to overlap; then the normals `s_u × s_v` lie in the cone about
/// `d = a_u × a_v / |a_u × a_v|` whose half-angle is the largest angle
/// between `d` and a plane through the origin tangent to both cones with
/// `(a_u · n)(a_v · n) < 0`. A patch that fails the hypothesis is quartered,
/// as the paper refines the discretization when the cones overlap
/// (`:459-462`); past [`NORMAL_CONE_MAX_DEPTH`] levels the call refuses.
/// The patch cones are unioned.
pub(crate) fn normal_cone<S: ParamSurface + ?Sized>(
    surface: &S,
    rect: ParamRect,
) -> Result<NormalCone, SurfaceError> {
    if !rect.is_finite() {
        return Err(SurfaceError::NonFiniteInput);
    }
    surface.validate()?;
    normal_cone_rec(surface, rect, 0)
}

fn normal_cone_rec<S: ParamSurface + ?Sized>(
    surface: &S,
    rect: ParamRect,
    depth: usize,
) -> Result<NormalCone, SurfaceError> {
    let patches = surface.bezier_patches(rect)?;
    let mut acc: Option<NormalCone> = None;
    for p in &patches {
        let cone = match patch_normal_cone(p) {
            Some(c) => c,
            None if depth < NORMAL_CONE_MAX_DEPTH => {
                // Quarter the whole rectangle: the analytic instances split
                // `rect` into patches along fixed angular steps, so the
                // failing patch's own rectangle is not returned to us.
                let mut sub: Option<NormalCone> = None;
                for q in rect.quarters() {
                    let c = normal_cone_rec(surface, q, depth + 1)?;
                    sub = Some(match sub {
                        None => c,
                        Some(s) => s.union(&c),
                    });
                }
                return sub.ok_or(SurfaceError::ConeHypothesis);
            }
            None => return Err(SurfaceError::ConeHypothesis),
        };
        acc = Some(match acc {
            None => cone,
            Some(a) => a.union(&cone),
        });
    }
    acc.ok_or(SurfaceError::Degenerate)
}

/// Theorem 4.1 for one patch; `None` when its hypothesis fails.
fn patch_normal_cone(p: &RationalBezierPatch) -> Option<NormalCone> {
    let (nu, nv) = (p.deg_u + 1, p.deg_v + 1);
    let mut du: Vec<Difference> = Vec::new();
    let mut dv: Vec<Difference> = Vec::new();
    if p.separable_weights {
        for j in 0..nv {
            for i in 0..nu {
                for k in i + 1..nu {
                    du.push(Difference::of(p.point(k, j), p.point(i, j)));
                }
            }
        }
        for i in 0..nu {
            for j in 0..nv {
                for l in j + 1..nv {
                    dv.push(Difference::of(p.point(i, l), p.point(i, j)));
                }
            }
        }
    } else {
        for i in 0..nu {
            for k in i + 1..nu {
                for j in 0..nv {
                    for l in 0..nv {
                        du.push(Difference::of(p.point(k, l), p.point(i, j)));
                    }
                }
            }
        }
        for j in 0..nv {
            for l in j + 1..nv {
                for i in 0..nu {
                    for k in 0..nu {
                        dv.push(Difference::of(p.point(k, l), p.point(i, j)));
                    }
                }
            }
        }
    }
    let cu = direction_cone(&du)?;
    let cv = direction_cone(&dv)?;
    let (au, alpha_u) = (cu.axis, cu.half_angle);
    let (av, alpha_v) = (cv.axis, cv.half_angle);
    // Complete cones ±C_u, ±C_v must not overlap: the angle between the
    // axis LINES exceeds the sum of the half-angles.
    let gamma = angle_between(au, av);
    if !(alpha_u + alpha_v < gamma && gamma < PI - (alpha_u + alpha_v)) {
        return None;
    }
    let d = unit(cross3(au, av))?;
    // Appendix B assumes the angle between the axes is at most π/2, taking
    // the opposite half-cone otherwise; the bound is the same with |g|.
    let g = dot3(au, av).abs();
    let (su, sv) = (alpha_u.sin(), alpha_v.sin());
    let det = 1.0 - g * g;
    if det <= 0.0 {
        return None;
    }
    // A unit n with n·a_u = c_u, n·a_v = c_v, c_u c_v = −sin α_u sin α_v,
    // written n = p a_u + q a_v + r d, has r² = 1 − (p c_u + q c_v) and the
    // normal's angle to d is arccos r.
    // (r² ≤ 0 means a tangent plane contains d: no half-cone of normals.)
    // θ = arccos √(1 − pc) = arcsin √pc; the arcsin form keeps a small θ
    // (where 1 − pc rounds to 1).
    let pc = (su * su + sv * sv + 2.0 * g * su * sv) / det;
    if pc.is_nan() || pc >= 1.0 {
        return None;
    }
    let theta = pc.max(0.0).sqrt().asin();
    Some(NormalCone {
        axis: d,
        half_angle: theta + FP_ANGLE_GUARD,
    })
}

/// A control-point difference `P − Q` and the angle by which rounding can
/// have turned it: each control point carries a few ulps of its own
/// magnitude from its evaluation, so the difference's direction is good to
/// `CONTROL_POINT_ULPS · ε · (|P| + |Q|) / |P − Q|` rad. A rounding bound,
/// not a geometric tolerance; it is large only where two control points
/// nearly coincide far from the origin.
struct Difference {
    v: [f64; 3],
    guard: f64,
}

/// Ulps of its own magnitude a control point's evaluation may carry (the
/// frame's normalization and cross product, the ring's three products and
/// two sums, the arc's `1/cos` scaling).
const CONTROL_POINT_ULPS: f64 = 16.0;

impl Difference {
    fn of(p: [f64; 3], q: [f64; 3]) -> Self {
        let v = sub3(p, q);
        let len = dot3(v, v).sqrt();
        let mag = dot3(p, p).sqrt() + dot3(q, q).sqrt();
        let guard = if len > 0.0 {
            CONTROL_POINT_ULPS * f64::EPSILON * mag / len
        } else {
            f64::INFINITY
        };
        Difference { v, guard }
    }
}

/// A half-cone holding every non-zero difference of `vs`, widened by each
/// one's rounding guard: axis the normalized sum of their unit directions,
/// half-angle the largest angle to it. `None` when there is no direction
/// or the cone is not a half-cone (half-angle ≥ π/2).
fn direction_cone(vs: &[Difference]) -> Option<NormalCone> {
    // A difference whose rounding guard reaches a radian cannot be told
    // from zero at evaluation precision (two control points that coincide
    // in exact arithmetic — a sphere pole row, a cone apex row); a zero
    // difference contributes nothing to the derivative, so it is dropped.
    let units: Vec<([f64; 3], f64)> = vs
        .iter()
        .filter(|d| d.guard < 1.0)
        .filter_map(|d| unit(d.v).map(|u| (u, d.guard)))
        .collect();
    if units.is_empty() {
        return None;
    }
    let mut sum = [0.0; 3];
    for (u, _) in &units {
        sum = add3(sum, *u);
    }
    let axis = unit(sum)?;
    let mut alpha = 0.0_f64;
    for (u, guard) in &units {
        alpha = alpha.max(angle_between(axis, *u) + guard);
    }
    let alpha = alpha + FP_ANGLE_GUARD;
    (alpha < 0.5 * PI).then_some(NormalCone {
        axis,
        half_angle: alpha,
    })
}

// ---- small vector helpers -------------------------------------------------

pub(super) fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(super) fn add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(super) fn scale3(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub(super) fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(super) fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let l = dot3(v, v).sqrt();
    (l > 0.0 && l.is_finite()).then(|| [v[0] / l, v[1] / l, v[2] / l])
}

/// The angle between two non-zero vectors, by `atan2(|a × b|, a · b)`
/// (accurate near 0 and π, unlike `acos`).
fn angle_between(a: [f64; 3], b: [f64; 3]) -> f64 {
    let c = cross3(a, b);
    dot3(c, c).sqrt().atan2(dot3(a, b))
}

fn dist_point_seg(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> f64 {
    let ab = sub3(b, a);
    let ap = sub3(p, a);
    let len2 = dot3(ab, ab);
    let t = if len2 == 0.0 {
        0.0
    } else {
        (dot3(ap, ab) / len2).clamp(0.0, 1.0)
    };
    let d = [ap[0] - t * ab[0], ap[1] - t * ab[1], ap[2] - t * ab[2]];
    dot3(d, d).sqrt()
}

/// Point-to-triangle distance (Ericson, Real-Time Collision Detection
/// §5.1.5); a degenerate triangle degrades to segment / point distance.
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
        let d00 = dot3(v0, v0);
        let d01 = dot3(v0, v1);
        let d11 = dot3(v1, v1);
        let d20 = dot3(v2, v0);
        let d21 = dot3(v2, v1);
        let denom = d00 * d11 - d01 * d01;
        let bv = (d11 * d20 - d01 * d21) / denom;
        let bw = (d00 * d21 - d01 * d20) / denom;
        if bv >= 0.0 && bw >= 0.0 && bv + bw <= 1.0 {
            return dot3(v2, n).abs() / n2.sqrt();
        }
    }
    edge_min
}
