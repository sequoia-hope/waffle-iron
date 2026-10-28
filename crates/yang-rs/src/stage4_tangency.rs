//! Yang et al. 2025 §4.3.1, §4.3.3 & Appendix C — parametric surface-pair
//! Newton optimization and tangency resolution
//! (`refs/text/yang2025_hybrid_boolean.txt:479-492`, `:518-540`, `:567-573`,
//! `:744-745`, `:1232-1250`).
//!
//! ## Paper authority
//! - **§4.3.1 / Appendix C (`:479-492` R, `:1232-1250` R):** Treats the
//!   parametric coordinates `(u, v)` on `S_A = s_1(u, v)` and `(s, t)` on
//!   `S_B = s_2(s, t)` as variables and solves `D(u, v, s, t) = s_1(u, v) -
//!   s_2(s, t) = 0` via the minimum-norm step `∇D ∇D^T a_k = s_2 - s_1`,
//!   `x_{k+1} = x_k + ∇D^T a_k`.
//! - **§4.3.2 / §4.3.3 (`:493-540` R, `:567-573` L):** Uses the geometric
//!   two-plane projection (`relocate_onto_implicit_pair`) where the two tangent
//!   planes are non-parallel, and uses Newton's method on the parametric
//!   surfaces for tangential points and `< 2d_ε` non-crossing candidate regions.
//!   At an isolated converged point `r`, compares `n_A(r)` and `n_B(r)` against
//!   the paper's angular tolerance `10^{-6}` rad (`:745` R): collinear normals
//!   classify a **tangent point** (`:567-569` L); non-collinear normals classify
//!   a **point on a small loop** (`:569-573` L).
//!
//! All five analytic [`Surface`] variants (`Plane`, `Sphere`, `Cylinder`,
//! `Cone`, `Torus`) are handled uniformly through pole-free, arc-length-scaled
//! [`LocalSurfaceChart`]s with zero surface-type-pair branching.

use crate::gauss_map::GaussMap;
use crate::shell_contact::{closest_on_triangle, triangle_distance, Bvh, TriSet};
use crate::stage4_relocate::{
    s43_tangency_census_log, surface_distance_and_normal, surface_kind_tag,
};
use crate::{normalize3, ortho_basis, BRep, Point3, Surface, Vector3};
use std::collections::BTreeMap;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Yang et al. 2025 §4.3.3 / §5 (`yang2025_hybrid_boolean.txt:567-573` L,
/// `:745` R): angular threshold (`10^{-6}` rad) comparing `n_A(r)` and `n_B(r)`
/// at a converged surface-pair intersection point to distinguish a tangent
/// point (`≤ 10^{-6}` rad) from a point on a loop (`> 10^{-6}` rad).
pub(crate) const TANGENT_NORMAL_ANGLE_TOL: f64 = 1e-6;

/// Yang et al. 2025 §4.2.1 Eq. (1) (`:456-461` L): factor `2` in the `< 2 d_ε`
/// proximity band.
const PROXIMITY_BAND_FACTOR: f64 = 2.0;

/// Dimensionless relative distance threshold (`10^{-12}`) from a coordinate
/// singularity (`Sphere` center, `Cylinder`/`Torus` axis, `Cone` apex/axis,
/// `Torus` tube spine) when constructing or evaluating [`LocalSurfaceChart`].
const CHART_SINGULAR_REL_TOL: f64 = 1e-12;

/// Dimensionless relative radial threshold (`10^{-15}`) below which `atan2`
/// falls back to the chart reference azimuth `0` in [`LocalSurfaceChart::project`].
const CHART_PROJECT_AXIS_REL_TOL: f64 = 1e-15;

/// Dimensionless relative axial threshold (`10^{-9}`) rejecting opposite-nappe
/// points in [`LocalSurfaceChart::project`] on a [`Surface::Cone`].
const CONE_PROJECT_NAPPE_REL_TOL: f64 = 1e-9;

/// Maximum unwrapped angular span (`2.2 < π` rad) allowed for a single triangle
/// edge in a local periodic chart before it straddles the `±π` branch cut.
const CHART_MAX_NON_WRAPPING_ARC_RAD: f64 = 2.2;

/// Fraction (`0.75`) of the remaining slant distance `s_0 + v` to the cone apex
/// permitted in a single negative-`v` Newton step so `σ > 0` always holds.
const CONE_APEX_STEP_FRAC: f64 = 0.75;

/// Latitude threshold (`0.8` rad) triggering equatorial re-centering of a
/// [`LocalSurfaceChart::Sphere`] during Newton iteration.
const SPHERE_RECENTER_LAT_RAD: f64 = 0.8;

/// Hard latitude domain bound (`1.48 < π/2` rad) keeping [`LocalSurfaceChart::Sphere`]
/// strictly away from its `±90°` local chart poles.
const SPHERE_MAX_CHART_LAT_RAD: f64 = 1.48;

/// Maximum one-sided Jacobi sweeps in [`thin_svd`].
const SVD_MAX_SWEEPS: usize = 20;

/// Dimensionless relative off-diagonal convergence threshold in [`thin_svd`].
const SVD_OFFDIAG_REL_TOL: f64 = 1e-15;

/// Dimensionless relative determinant threshold (`|det| <= 10^{-24} · g00 · g11`)
/// guarding degenerate `2 × 2` tangent-plane Gram solves.
const GRAM_DET_REL_TOL: f64 = 1e-24;

/// Maximum normal-collinearity angle (`10^{-4}` rad) at which a stationary
/// positive-gap iterate qualifies for the Sederberg quadratic loop-escape step.
const ESCAPE_MAX_COLLINEAR_ANGLE_RAD: f64 = 1e-4;

/// Minimum normal-component fraction (`|δ_n| > 10^{-12} · gap`) for the
/// Sederberg quadratic loop-escape step.
const ESCAPE_MIN_NORMAL_GAP_FRAC: f64 = 1e-12;

/// Minimum dimensionless curvature-gap product (`λ · δ_n > 10^{-18}`) certifying
/// local interpenetration in [`quadratic_collinear_loop_escape`].
const ESCAPE_MIN_CURVATURE_GAP_PROD: f64 = 1e-18;

/// Dimensionless relative off-diagonal threshold in the `2 × 2` curvature
/// eigensolver of [`quadratic_collinear_loop_escape`].
const ESCAPE_OFFDIAG_REL_TOL: f64 = 1e-15;

/// Sufficient-decrease factor (`0.9`) on `‖s_1 - s_2‖` for accepting a
/// Sederberg quadratic loop-escape step.
const ESCAPE_ACCEPT_GAP_FRAC: f64 = 0.9;

/// Maximum damped Newton iterations in [`solve_parametric_pair_newton`].
const PAIR_NEWTON_MAX_ITERS: usize = 56;

/// Machine-epsilon multiplier (`16 · ε_mach · scale`) in [`coincidence_floor`].
const PAIR_GAP_MACHINE_EPS_MULT: f64 = 16.0;

/// Maximum angular step (`0.75` rad of arc) per iteration along curved chart
/// coordinates in [`solve_parametric_pair_newton`].
const PAIR_CURV_STEP_CAP_RAD: f64 = 0.75;

/// Multiplier (`2.0`) on `seed_sep + scale` bounding single-iteration steps
/// along zero-curvature linear coordinates (`Plane`, `Cylinder` ruling, `Cone` `+v`).
const LINEAR_STEP_CAP_FACTOR: f64 = 2.0;

/// Dimensionless relative gradient stationarity threshold (`10^{-13}`) in
/// [`solve_parametric_pair_newton`] and [`solve_tangent_with_sign`].
const STATIONARY_GRAD_REL_TOL: f64 = 1e-13;

/// Singular-value floor (`10^{-14}`) detecting a completely zero Jacobian in
/// [`solve_parametric_pair_newton`] and [`solve_tangent_with_sign`].
const SVD_ZERO_SIGMA_TOL: f64 = 1e-14;

/// Relative singular-value cutoff (`10^{-14} · σ_0`) for the `3 × 4`
/// minimum-norm pseudoinverse step in [`solve_parametric_pair_newton`].
const PAIR_SVD_REL_CUTOFF: f64 = 1e-14;

/// Relative parameter-step convergence threshold (`10^{-15}`) in Newton loops.
const STEP_NORM_REL_TOL: f64 = 1e-15;

/// Maximum backtracking line-search halvings (`10`) in [`solve_parametric_pair_newton`]
/// and [`solve_tangent_with_sign`].
const NEWTON_LINE_SEARCH_HALVINGS: usize = 10;

/// Normal-alignment cosine threshold (`|n_A · n_B| > 0.75`) selecting a single
/// orientation sign `σ ∈ {+1, -1}` in [`solve_tangent_point_newton`].
const TANGENT_SIGN_LOCK_COS: f64 = 0.75;

/// Maximum damped Gauss–Newton iterations in [`solve_tangent_with_sign`].
const TANGENT_NEWTON_MAX_ITERS: usize = 36;

/// Machine-epsilon multiplier (`4 · ε_mach · scale`) for early polished exit on
/// position gap in [`solve_tangent_with_sign`].
const TANGENT_GAP_MACHINE_EPS_MULT: f64 = 4.0;

/// Machine-epsilon multiplier (`8 · ε_mach` rad) for early polished exit on
/// normal collinearity in [`solve_tangent_with_sign`].
const TANGENT_ANG_MACHINE_EPS_MULT: f64 = 8.0;

/// Maximum converged normal angle (`10^{-9}` rad) accepted as an exact root of
/// `n_1(u,v) - σ n_2(s,t) = 0` in [`solve_tangent_with_sign`].
const TANGENT_SOLVER_MAX_ANGLE_RAD: f64 = 1e-9;

/// Relative gap threshold (`10^{-6}`) at which [`solve_tangent_with_sign`]
/// re-anchors both local charts once onto the iterate so final polish evaluates
/// trigonometric functions at `(u, v) ≈ (0, 0)`.
const TANGENT_REANCHOR_GAP_REL_TOL: f64 = 1e-6;

/// Normal-angle threshold (`10^{-4}` rad) accompanying [`TANGENT_REANCHOR_GAP_REL_TOL`].
const TANGENT_REANCHOR_ANGLE_RAD: f64 = 1e-4;

/// Relative singular-value rank threshold (`10^{-9} · σ_0`) classifying the
/// `5 × 4` tangency Jacobian into `rank == 4` (isolated 0D point), `rank == 3`
/// (1D tangent curve), or `rank <= 2` (2D coincident surface).
const TANGENT_RANK_REL_TOL: f64 = 1e-9;

/// Relative singular-value pseudoinverse cutoff (`10^{-12} · σ_0`) in
/// [`solve_tangent_with_sign`].
const TANGENT_SVD_REL_CUTOFF: f64 = 1e-12;

/// Maximum angular step (`0.85` rad of arc) per iteration along curved chart
/// coordinates in [`solve_tangent_with_sign`].
const TANGENT_CURV_STEP_CAP_RAD: f64 = 0.85;

/// Slack multiplier (`2.0`) on `max_drift` permitted for intermediate
/// backtracking line-search iterates in [`solve_tangent_with_sign`].
const LINE_SEARCH_DRIFT_SLACK: f64 = 2.0;

/// Dimensionless local-chart probe fraction (`0.05`) testing whether two
/// collinear-normal surfaces coincide identically in 2D around a converged point.
const COINCIDENCE_PROBE_STEP_FRAC: f64 = 0.05;

/// Scale-relative trust-region radius fraction (`0.25 · r_min`) around the
/// input vertex in [`solve_isolated_tangent_point_near`].
const ISOLATED_TANGENT_DRIFT_RADIUS_FRAC: f64 = 0.25;

pub(crate) type V3 = [f64; 3];

#[inline]
pub(crate) fn dot3(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub(crate) fn sub3(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
pub(crate) fn add3(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[inline]
pub(crate) fn scale3(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[inline]
pub(crate) fn cross3(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
pub(crate) fn norm3(a: V3) -> f64 {
    dot3(a, a).sqrt()
}

#[inline]
pub(crate) fn mag3(a: V3) -> f64 {
    a[0].abs().max(a[1].abs()).max(a[2].abs())
}

/// First-order surface position and normal jet at a chart parameter `(u, v)`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ChartEval {
    pub(crate) point: V3,
    pub(crate) su: V3,
    pub(crate) sv: V3,
    pub(crate) normal: V3,
    pub(crate) nu: V3,
    pub(crate) nv: V3,
}

/// Pole-free, seam-free, arc-length-scaled local parametric chart centered at
/// an anchor point on any analytic [`Surface`].
///
/// Scaling `(u, v)` to world length units at `(0, 0)` (`‖∂s/∂u(0,0)‖ = 1`,
/// `‖∂s/∂v(0,0)‖ = 1`) eliminates the parameterization-scaling mismatch noted
/// in Yang et al. 2025 §4.3.1 (`yang2025_hybrid_boolean.txt:485-488` R).
#[derive(Clone, Copy, Debug)]
pub(crate) enum LocalSurfaceChart {
    Plane {
        origin: V3,
        e1: V3,
        e2: V3,
        normal: V3,
    },
    Sphere {
        center: V3,
        radius: f64,
        e1: V3,
        e2: V3,
        e3: V3,
    },
    Cylinder {
        origin: V3,
        axis: V3,
        e1: V3,
        e2: V3,
        radius: f64,
    },
    Cone {
        apex: V3,
        axis: V3,
        e1: V3,
        e2: V3,
        sin_a: f64,
        cos_a: f64,
        sgn: f64,
        s0: f64,
        u_scale: f64,
    },
    Torus {
        center: V3,
        axis: V3,
        e1: V3,
        e2: V3,
        major_radius: f64,
        minor_radius: f64,
        phi0: f64,
        u_scale: f64,
    },
}

impl LocalSurfaceChart {
    /// Construct a pole-free local chart on `surface` centered at `anchor`'s
    /// foot on `surface`. Returns `None` for degenerate surface parameters or
    /// singular anchor placement (e.g. cone apex).
    pub(crate) fn new(surface: Surface, anchor: Point3) -> Option<Self> {
        let p = anchor.as_array();
        if !p.iter().all(|c| c.is_finite()) {
            return None;
        }
        match surface {
            Surface::Plane { normal, d } => {
                let raw = normal.as_array();
                let nl = norm3(raw);
                if nl <= cad_primitives::TAU_WORK || !nl.is_finite() || !d.is_finite() {
                    return None;
                }
                let n = scale3(raw, 1.0 / nl);
                let dn = d / nl;
                let f = dot3(n, p) + dn;
                let origin = sub3(p, scale3(n, f));
                let (e1v, e2v) = ortho_basis(Vector3::new(n[0], n[1], n[2]));
                Some(Self::Plane {
                    origin,
                    e1: e1v.as_array(),
                    e2: e2v.as_array(),
                    normal: n,
                })
            }
            Surface::Sphere { center, radius } => {
                let c = center.as_array();
                if !c.iter().all(|v| v.is_finite()) || !radius.is_finite() || radius <= 0.0 {
                    return None;
                }
                let w = sub3(p, c);
                let wl = norm3(w);
                let e1 = if wl > CHART_SINGULAR_REL_TOL * radius {
                    scale3(w, 1.0 / wl)
                } else {
                    [1.0, 0.0, 0.0]
                };
                // Place `e1` on the local equator `(u, v) = (0, 0)`: `cos φ = 1`
                // at the seed regardless of global north/south pole alignment.
                let (e2v, e3v) = ortho_basis(Vector3::new(e1[0], e1[1], e1[2]));
                Some(Self::Sphere {
                    center: c,
                    radius,
                    e1,
                    e2: e2v.as_array(),
                    e3: e3v.as_array(),
                })
            }
            Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                let ap = axis_point.as_array();
                let ad = axis_dir.as_array();
                let al = norm3(ad);
                if !ap.iter().all(|v| v.is_finite())
                    || !ad.iter().all(|v| v.is_finite())
                    || al <= cad_primitives::TAU_WORK
                    || !radius.is_finite()
                    || radius <= 0.0
                {
                    return None;
                }
                let axis = scale3(ad, 1.0 / al);
                let w = sub3(p, ap);
                let h0 = dot3(w, axis);
                let origin = add3(ap, scale3(axis, h0));
                let rad = sub3(w, scale3(axis, h0));
                let rl = norm3(rad);
                let (e1, e2) = if rl > CHART_SINGULAR_REL_TOL * radius {
                    let e1 = scale3(rad, 1.0 / rl);
                    let e2 = cross3(axis, e1);
                    (e1, e2)
                } else {
                    let (e1v, e2v) = ortho_basis(Vector3::new(axis[0], axis[1], axis[2]));
                    (e1v.as_array(), e2v.as_array())
                };
                Some(Self::Cylinder {
                    origin,
                    axis,
                    e1,
                    e2,
                    radius,
                })
            }
            Surface::Cone {
                apex,
                axis_dir,
                half_angle,
            } => {
                let ap = apex.as_array();
                let ad = axis_dir.as_array();
                let al = norm3(ad);
                if !ap.iter().all(|v| v.is_finite())
                    || !ad.iter().all(|v| v.is_finite())
                    || al <= cad_primitives::TAU_WORK
                    || !half_angle.is_finite()
                    || half_angle <= 0.0
                    || half_angle >= FRAC_PI_2
                {
                    return None;
                }
                let axis = scale3(ad, 1.0 / al);
                let (sin_a, cos_a) = half_angle.sin_cos();
                let w = sub3(p, ap);
                let h = dot3(w, axis);
                let rad = sub3(w, scale3(axis, h));
                let rl = norm3(rad);
                let sgn = if h >= 0.0 { 1.0 } else { -1.0 };
                let s0 = h.abs() * cos_a + rl * sin_a;
                let eps = CHART_SINGULAR_REL_TOL * mag3(ap).max(mag3(p));
                if s0 <= eps {
                    return None;
                }
                let axis_eps = CHART_SINGULAR_REL_TOL * s0;
                let u_scale = (s0 * sin_a).max(axis_eps);
                let (e1, e2) = if rl > axis_eps {
                    let e1 = scale3(rad, 1.0 / rl);
                    let e2 = cross3(axis, e1);
                    (e1, e2)
                } else {
                    let (e1v, e2v) = ortho_basis(Vector3::new(axis[0], axis[1], axis[2]));
                    (e1v.as_array(), e2v.as_array())
                };
                Some(Self::Cone {
                    apex: ap,
                    axis,
                    e1,
                    e2,
                    sin_a,
                    cos_a,
                    sgn,
                    s0,
                    u_scale,
                })
            }
            Surface::Torus {
                center,
                axis_dir,
                major_radius,
                minor_radius,
            } => {
                let c = center.as_array();
                let ad = axis_dir.as_array();
                let al = norm3(ad);
                if !c.iter().all(|v| v.is_finite())
                    || !ad.iter().all(|v| v.is_finite())
                    || al <= cad_primitives::TAU_WORK
                    || !major_radius.is_finite()
                    || !minor_radius.is_finite()
                    || minor_radius <= 0.0
                    || major_radius <= minor_radius
                {
                    return None;
                }
                let axis = scale3(ad, 1.0 / al);
                let w = sub3(p, c);
                let z = dot3(w, axis);
                let rad = sub3(w, scale3(axis, z));
                let rho = norm3(rad);
                let (e1, e2) = if rho > CHART_SINGULAR_REL_TOL * major_radius {
                    let e1 = scale3(rad, 1.0 / rho);
                    let e2 = cross3(axis, e1);
                    (e1, e2)
                } else {
                    let (e1v, e2v) = ortho_basis(Vector3::new(axis[0], axis[1], axis[2]));
                    (e1v.as_array(), e2v.as_array())
                };
                let dr = rho - major_radius;
                let phi0 = if dr.hypot(z) > CHART_SINGULAR_REL_TOL * minor_radius {
                    z.atan2(dr)
                } else {
                    0.0
                };
                let u_scale = (major_radius + minor_radius * phi0.cos()).max(minor_radius);
                Some(Self::Torus {
                    center: c,
                    axis,
                    e1,
                    e2,
                    major_radius,
                    minor_radius,
                    phi0,
                    u_scale,
                })
            }
        }
    }

    /// Characteristic curvature radius of this surface near the chart origin,
    /// or `None` for a flat plane.
    pub(crate) fn characteristic_radius(&self) -> Option<f64> {
        match *self {
            Self::Plane { .. } => None,
            Self::Sphere { radius, .. } | Self::Cylinder { radius, .. } => Some(radius),
            Self::Cone { u_scale, cos_a, .. } => Some(u_scale / cos_a),
            Self::Torus {
                major_radius,
                minor_radius,
                ..
            } => Some(minor_radius.min(major_radius - minor_radius)),
        }
    }

    /// Maximum single-triangle-edge span `(max_du, max_dv)` before an edge
    /// straddles a periodic branch cut in `(u, v)`. Zero-curvature linear
    /// coordinates (`Plane` `(u, v)`, `Cylinder`/`Cone` ruling `v`) are
    /// non-periodic (`f64::INFINITY`).
    pub(crate) fn max_non_wrapping_edge_delta(&self) -> (f64, f64) {
        match *self {
            Self::Plane { .. } => (f64::INFINITY, f64::INFINITY),
            Self::Sphere { radius, .. } => (
                CHART_MAX_NON_WRAPPING_ARC_RAD * radius,
                CHART_MAX_NON_WRAPPING_ARC_RAD * radius,
            ),
            Self::Cylinder { radius, .. } => {
                (CHART_MAX_NON_WRAPPING_ARC_RAD * radius, f64::INFINITY)
            }
            Self::Cone { u_scale, .. } => (CHART_MAX_NON_WRAPPING_ARC_RAD * u_scale, f64::INFINITY),
            Self::Torus {
                u_scale,
                minor_radius,
                ..
            } => (
                CHART_MAX_NON_WRAPPING_ARC_RAD * u_scale,
                CHART_MAX_NON_WRAPPING_ARC_RAD * minor_radius,
            ),
        }
    }

    /// Per-coordinate positive step caps `[cap_u, cap_v]` for damped parametric
    /// Newton steps from Fig. 10(c) triangle-barycenter seeds
    /// (`yang2025_hybrid_boolean.txt:471-473` R, `:491-495` L).
    ///
    /// Curved angular coordinates (`Sphere` `(u, v)`, `Cylinder`/`Cone` `u`,
    /// `Torus` `(u, v)`) stay bounded by `curv_factor` radians of arc so an
    /// iterate never wraps past a periodic branch, while zero-curvature linear
    /// coordinates (`Plane` `(u, v)`, `Cylinder` axial `v`, `Cone` outward
    /// slant `+v`) allow full-face steps `lin_cap` in a single Newton step.
    pub(crate) fn step_caps(
        &self,
        x_cur: [f64; 2],
        delta_uv: [f64; 2],
        curv_factor: f64,
        lin_cap: f64,
    ) -> [f64; 2] {
        match *self {
            Self::Plane { .. } => [lin_cap, lin_cap],
            Self::Sphere { radius, .. } => {
                let c = curv_factor * radius;
                [c, c]
            }
            Self::Cylinder { radius, .. } => [curv_factor * radius, lin_cap],
            Self::Cone { u_scale, s0, .. } => {
                let cu = curv_factor * u_scale;
                let cv = if delta_uv[1] < 0.0 {
                    // Keep `sigma = s0 + v > 0` strictly away from the cone apex.
                    CONE_APEX_STEP_FRAC * (s0 + x_cur[1]).max(CHART_SINGULAR_REL_TOL * s0)
                } else {
                    lin_cap
                };
                [cu, cv]
            }
            Self::Torus {
                u_scale,
                minor_radius,
                ..
            } => [curv_factor * u_scale, curv_factor * minor_radius],
        }
    }

    /// Re-center a `Sphere` chart onto the local equator `(u, v) = (0, 0)` at
    /// `point` whenever latitude `|phi| = |v / radius| > SPHERE_RECENTER_LAT_RAD`,
    /// keeping iterates far from the `±90°` chart poles even from coarse seeds.
    pub(crate) fn recenter_sphere_if_needed(
        &mut self,
        surface: Surface,
        uv: &mut (f64, f64),
        point: V3,
    ) -> Option<bool> {
        if let Self::Sphere { radius, .. } = *self {
            if uv.1.abs() > SPHERE_RECENTER_LAT_RAD * radius {
                *self = Self::new(surface, Point3::new(point[0], point[1], point[2]))?;
                *uv = (0.0, 0.0);
                return Some(true);
            }
        }
        Some(false)
    }

    /// Evaluate position, tangent vectors, unit outward normal, and normal
    /// derivatives at `(u, v)`.
    pub(crate) fn eval(&self, u: f64, v: f64) -> Option<ChartEval> {
        if !u.is_finite() || !v.is_finite() {
            return None;
        }
        match *self {
            Self::Plane {
                origin,
                e1,
                e2,
                normal,
            } => {
                let point = add3(origin, add3(scale3(e1, u), scale3(e2, v)));
                Some(ChartEval {
                    point,
                    su: e1,
                    sv: e2,
                    normal,
                    nu: [0.0; 3],
                    nv: [0.0; 3],
                })
            }
            Self::Sphere {
                center,
                radius,
                e1,
                e2,
                e3,
            } => {
                let theta = u / radius;
                let phi = v / radius;
                // Keep iterates strictly away from the local chart's ±90° poles
                // (90° away from the seed!).
                if phi.abs() >= SPHERE_MAX_CHART_LAT_RAD {
                    return None;
                }
                let (st, ct) = theta.sin_cos();
                let (sp, cp) = phi.sin_cos();
                let eq = add3(scale3(e1, ct), scale3(e2, st));
                let t_theta = add3(scale3(e1, -st), scale3(e2, ct));
                let r_hat = add3(scale3(eq, cp), scale3(e3, sp));
                let t_phi = add3(scale3(eq, -sp), scale3(e3, cp));
                let point = add3(center, scale3(r_hat, radius));
                let su = scale3(t_theta, cp);
                let sv = t_phi;
                let inv_r = 1.0 / radius;
                Some(ChartEval {
                    point,
                    su,
                    sv,
                    normal: r_hat,
                    nu: scale3(su, inv_r),
                    nv: scale3(sv, inv_r),
                })
            }
            Self::Cylinder {
                origin,
                axis,
                e1,
                e2,
                radius,
            } => {
                let theta = u / radius;
                let (st, ct) = theta.sin_cos();
                let r_hat = add3(scale3(e1, ct), scale3(e2, st));
                let t_theta = add3(scale3(e1, -st), scale3(e2, ct));
                let point = add3(origin, add3(scale3(axis, v), scale3(r_hat, radius)));
                Some(ChartEval {
                    point,
                    su: t_theta,
                    sv: axis,
                    normal: r_hat,
                    nu: scale3(t_theta, 1.0 / radius),
                    nv: [0.0; 3],
                })
            }
            Self::Cone {
                apex,
                axis,
                e1,
                e2,
                sin_a,
                cos_a,
                sgn,
                s0,
                u_scale,
            } => {
                let sigma = s0 + v;
                if sigma <= CHART_SINGULAR_REL_TOL * s0 {
                    return None;
                }
                let theta = u / u_scale;
                let (st, ct) = theta.sin_cos();
                let r_hat = add3(scale3(e1, ct), scale3(e2, st));
                let t_theta = add3(scale3(e1, -st), scale3(e2, ct));
                let e_slant = add3(scale3(axis, sgn * cos_a), scale3(r_hat, sin_a));
                let point = add3(apex, scale3(e_slant, sigma));
                let su = scale3(t_theta, (sigma * sin_a) / u_scale);
                let sv = e_slant;
                let normal = sub3(scale3(r_hat, cos_a), scale3(axis, sgn * sin_a));
                let nu = scale3(t_theta, cos_a / u_scale);
                Some(ChartEval {
                    point,
                    su,
                    sv,
                    normal,
                    nu,
                    nv: [0.0; 3],
                })
            }
            Self::Torus {
                center,
                axis,
                e1,
                e2,
                major_radius,
                minor_radius,
                phi0,
                u_scale,
            } => {
                let theta = u / u_scale;
                let phi = phi0 + v / minor_radius;
                let (st, ct) = theta.sin_cos();
                let (sp, cp) = phi.sin_cos();
                let r_hat = add3(scale3(e1, ct), scale3(e2, st));
                let t_theta = add3(scale3(e1, -st), scale3(e2, ct));
                let r_eff = major_radius + minor_radius * cp;
                let point = add3(
                    center,
                    add3(scale3(r_hat, r_eff), scale3(axis, minor_radius * sp)),
                );
                let su = scale3(t_theta, r_eff / u_scale);
                let sv = add3(scale3(r_hat, -sp), scale3(axis, cp));
                let normal = add3(scale3(r_hat, cp), scale3(axis, sp));
                let nu = scale3(t_theta, cp / u_scale);
                let nv = scale3(sv, 1.0 / minor_radius);
                Some(ChartEval {
                    point,
                    su,
                    sv,
                    normal,
                    nu,
                    nv,
                })
            }
        }
    }

    /// Project a 3D point `p` onto its closest local `(u, v)` chart coordinates.
    pub(crate) fn project(&self, p: V3) -> Option<(f64, f64)> {
        if !p.iter().all(|c| c.is_finite()) {
            return None;
        }
        match *self {
            Self::Plane { origin, e1, e2, .. } => {
                let w = sub3(p, origin);
                Some((dot3(w, e1), dot3(w, e2)))
            }
            Self::Sphere {
                center,
                radius,
                e1,
                e2,
                e3,
            } => {
                let w = sub3(p, center);
                let wl = norm3(w);
                if wl <= CHART_PROJECT_AXIS_REL_TOL * radius {
                    return Some((0.0, 0.0));
                }
                let d = scale3(w, 1.0 / wl);
                let x = dot3(d, e1);
                let y = dot3(d, e2);
                let z = dot3(d, e3).clamp(-1.0, 1.0);
                let theta = y.atan2(x);
                let phi = z.asin();
                if phi.abs() >= SPHERE_MAX_CHART_LAT_RAD {
                    return None;
                }
                Some((radius * theta, radius * phi))
            }
            Self::Cylinder {
                origin,
                axis,
                e1,
                e2,
                radius,
            } => {
                let w = sub3(p, origin);
                let v = dot3(w, axis);
                let rad = sub3(w, scale3(axis, v));
                let rl = norm3(rad);
                let theta = if rl > CHART_PROJECT_AXIS_REL_TOL * radius {
                    dot3(rad, e2).atan2(dot3(rad, e1))
                } else {
                    0.0
                };
                Some((radius * theta, v))
            }
            Self::Cone {
                apex,
                axis,
                e1,
                e2,
                sin_a,
                cos_a,
                sgn,
                s0,
                u_scale,
            } => {
                let w = sub3(p, apex);
                let h = dot3(w, axis);
                if h * sgn <= 0.0 && h.abs() > CONE_PROJECT_NAPPE_REL_TOL * s0 {
                    return None;
                }
                let rad = sub3(w, scale3(axis, h));
                let rl = norm3(rad);
                let sigma = h.abs() * cos_a + rl * sin_a;
                if sigma <= CHART_SINGULAR_REL_TOL * s0 {
                    return None;
                }
                let theta = if rl > CHART_PROJECT_AXIS_REL_TOL * u_scale {
                    dot3(rad, e2).atan2(dot3(rad, e1))
                } else {
                    0.0
                };
                Some((u_scale * theta, sigma - s0))
            }
            Self::Torus {
                center,
                axis,
                e1,
                e2,
                major_radius,
                minor_radius,
                phi0,
                u_scale,
            } => {
                let w = sub3(p, center);
                let z = dot3(w, axis);
                let rad = sub3(w, scale3(axis, z));
                let rho = norm3(rad);
                let theta = if rho > CHART_PROJECT_AXIS_REL_TOL * major_radius {
                    dot3(rad, e2).atan2(dot3(rad, e1))
                } else {
                    0.0
                };
                let dr = rho - major_radius;
                let phi = if dr.hypot(z) > CHART_PROJECT_AXIS_REL_TOL * minor_radius {
                    z.atan2(dr)
                } else {
                    phi0
                };
                let dphi = (phi - phi0 + PI).rem_euclid(TAU) - PI;
                Some((u_scale * theta, minor_radius * dphi))
            }
        }
    }
}

/// Deterministic one-sided Jacobi thin SVD of an `M × N` matrix (`M >= N`),
/// returning `(U, sigma, V)` with `A = U * diag(sigma) * V^T` and
/// `sigma[0] >= sigma[1] >= ... >= sigma[N - 1] >= 0`.
///
/// Orthogonalizes columns of `W` directly without forming `A^T A`, preserving
/// full `f64` relative accuracy on small singular values near tangency.
pub(crate) fn thin_svd<const M: usize, const N: usize>(
    a: [[f64; N]; M],
) -> ([[f64; N]; M], [f64; N], [[f64; N]; N]) {
    let mut w = a;
    let mut v = [[0.0; N]; N];
    for (i, row) in v.iter_mut().enumerate() {
        row[i] = 1.0;
    }

    for _sweep in 0..SVD_MAX_SWEEPS {
        let mut converged = true;
        for p in 0..N {
            for q in (p + 1)..N {
                let mut alpha = 0.0;
                let mut beta = 0.0;
                let mut gamma = 0.0;
                for row in &w {
                    alpha += row[p] * row[p];
                    beta += row[q] * row[q];
                    gamma += row[p] * row[q];
                }
                if gamma.abs() <= SVD_OFFDIAG_REL_TOL * (alpha * beta).sqrt() {
                    continue;
                }
                converged = false;
                let zeta = (beta - alpha) / (2.0 * gamma);
                let t = if zeta >= 0.0 {
                    1.0 / (zeta + (1.0 + zeta * zeta).sqrt())
                } else {
                    -1.0 / (-zeta + (1.0 + zeta * zeta).sqrt())
                };
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = c * t;
                for row in &mut w {
                    let wp = row[p];
                    let wq = row[q];
                    row[p] = c * wp - s * wq;
                    row[q] = s * wp + c * wq;
                }
                for row in &mut v {
                    let vp = row[p];
                    let vq = row[q];
                    row[p] = c * vp - s * vq;
                    row[q] = s * vp + c * vq;
                }
            }
        }
        if converged {
            break;
        }
    }

    let mut sigma = [0.0; N];
    let mut u = [[0.0; N]; M];
    for j in 0..N {
        let mut norm_sq = 0.0;
        for row in &w {
            norm_sq += row[j] * row[j];
        }
        let s = norm_sq.sqrt();
        sigma[j] = s;
        if s > 0.0 {
            for i in 0..M {
                u[i][j] = w[i][j] / s;
            }
        } else if j < M {
            u[j][j] = 1.0;
        }
    }

    // Sort singular values descending (deterministic insertion sort).
    for i in 0..N {
        let mut best = i;
        for j in (i + 1)..N {
            if sigma[j] > sigma[best] {
                best = j;
            }
        }
        if best != i {
            sigma.swap(i, best);
            for row in &mut u {
                row.swap(i, best);
            }
            for row in &mut v {
                row.swap(i, best);
            }
        }
    }

    (u, sigma, v)
}

/// Converged solution of Appendix C's two-point parametric Newton
/// (`D(u, v, s, t) = s_1(u, v) - s_2(s, t) = 0`).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct ParametricPairSolution {
    pub(crate) point: Point3,
    pub(crate) uv_a: (f64, f64),
    pub(crate) uv_b: (f64, f64),
    pub(crate) normal_a: V3,
    pub(crate) normal_b: V3,
    pub(crate) surface_gap: f64,
    pub(crate) normal_angle_rad: f64,
}

/// Differential-geometric classification of a converged surface tangency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TangencyKind {
    /// Isolated 0D elliptic kiss (`rank(J_tan) == 4`, `det(ΔII) >= 0`): locally
    /// `{s_1 = s_2} = {r}` with no 1D curve branches (e.g. sphere–plane kiss,
    /// torus outer-equator plane graze, external sphere/cylinder kiss).
    IsolatedKiss,
    /// Isolated 0D hyperbolic saddle tangency (`rank(J_tan) == 4`,
    /// `det(ΔII) < 0`): two 1D intersection curve branches cross in an `X` at
    /// `r` (e.g. Steinmetz / Viviani cylinder tangency, torus inner-throat
    /// plane graze).
    IsolatedSaddle,
    /// 1D tangent curve locus (`rank(J_tan) == 3`, e.g. torus bottom circle
    /// grazing a horizontal plane or parallel tangent cylinder generator).
    TangentCurve,
}

/// Converged solution of §4.3's tangency-point Newton solver.
#[allow(dead_code)] // Fields uv_*/normal_* consumed by loop/tangent seeding in Steps 2b & 3.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TangentSolution {
    pub(crate) point: Point3,
    pub(crate) uv_a: (f64, f64),
    pub(crate) uv_b: (f64, f64),
    pub(crate) normal_a: V3,
    pub(crate) normal_b: V3,
    pub(crate) surface_gap: f64,
    pub(crate) normal_angle_rad: f64,
    pub(crate) kind: TangencyKind,
}

impl TangentSolution {
    #[inline]
    pub(crate) fn is_isolated_point(&self) -> bool {
        matches!(
            self.kind,
            TangencyKind::IsolatedKiss | TangencyKind::IsolatedSaddle
        )
    }
}

/// §4.3.3 classification of an isolated candidate seed (`:535-540` R,
/// `:567-573` L, `:745` R).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum SeedOptimizationResult {
    /// Converged to an isolated 0D tangent point (`∠(n_A, ±n_B) <= 10^{-6}` rad,
    /// `rank(J_tan) == 4`; `:567-569` L, `:745` R).
    Tangent(TangentSolution),
    /// Converged to an exact point on a non-tangential intersection curve /
    /// small loop (`∠(n_A, ±n_B) > 10^{-6}` rad), suitable as a Bajaj et al.
    /// 1988 marching seed (`:569-573` L).
    CurvePoint(ParametricPairSolution),
    /// Converged to a common point where the two surfaces coincide identically
    /// in 2D (`∠(n_A, ±n_B) <= 10^{-6}` rad, owned by §4.5.5 Stage 0).
    CoincidentSurfaces,
    /// Pair optimization converged with collinear normals (`<= 10^{-6}` rad),
    /// but isolated 0D tangency Newton failed or hit a 1D tangent locus
    /// (`TangencyKind::TangentCurve`). Must be handled loudly by the caller.
    TangencySolveFailed,
    /// Surfaces do not intersect in the seed's neighborhood (Case IV ruled out,
    /// `:529-534` R).
    NoIntersection,
}

#[inline]
pub(crate) fn collinear_angle_rad(na: V3, nb: V3) -> f64 {
    let cx = norm3(cross3(na, nb));
    let dt = dot3(na, nb).abs();
    cx.atan2(dt)
}

#[inline]
pub(crate) fn surface_intrinsic_scale(s: Surface) -> f64 {
    match s {
        Surface::Plane { normal, d } => {
            let nl = norm3(normal.as_array());
            if nl > 0.0 {
                d.abs() / nl
            } else {
                d.abs()
            }
        }
        Surface::Sphere { center, radius } => mag3(center.as_array()).max(radius.abs()),
        Surface::Cylinder {
            axis_point, radius, ..
        } => mag3(axis_point.as_array()).max(radius.abs()),
        Surface::Cone { apex, .. } => mag3(apex.as_array()),
        Surface::Torus {
            center,
            major_radius,
            minor_radius,
            ..
        } => mag3(center.as_array())
            .max(major_radius.abs())
            .max(minor_radius.abs()),
    }
}

#[inline]
pub(crate) fn coincidence_floor(
    p_a: Point3,
    p_b: Point3,
    s_a: Surface,
    s_b: Surface,
) -> (f64, f64) {
    let scale = mag3(p_a.as_array())
        .max(mag3(p_b.as_array()))
        .max(surface_intrinsic_scale(s_a))
        .max(surface_intrinsic_scale(s_b));
    let tau = (cad_primitives::TAU_WORK * (1.0 + scale))
        .max(PAIR_GAP_MACHINE_EPS_MULT * f64::EPSILON * scale);
    (tau, scale)
}

/// Express a 3D tangent-plane vector `w` in `(su, sv)` coordinates via a `2 × 2`
/// Gram solve.
#[inline]
fn solve_in_tangent_frame(su: V3, sv: V3, w: V3) -> [f64; 2] {
    let g00 = dot3(su, su);
    let g01 = dot3(su, sv);
    let g11 = dot3(sv, sv);
    let det = g00 * g11 - g01 * g01;
    if det.abs() <= GRAM_DET_REL_TOL * g00 * g11 {
        return [0.0, 0.0];
    }
    let r0 = dot3(su, w);
    let r1 = dot3(sv, w);
    [(g11 * r0 - g01 * r1) / det, (g00 * r1 - g01 * r0) / det]
}

/// Second-order quadratic step escaping a Sederberg collinear-normal interpenetration
/// extremum (`∇(‖D‖²/2) = 0`, `D = δ n_A ≠ 0`, `n_A = ±n_B`; Sederberg et al. 1989,
/// `yang2025_hybrid_boolean.txt:442-446` R).
///
/// Along a unit common-tangent direction `ŵ`, `n_A · (s_A(t ŵ) - s_B(t ŵ)) =
/// δ - ½ (ŵ^T M_rel ŵ) t² + O(t³)`. When `M_rel` has an eigenvalue `λ` with
/// `λ · δ > 0`, the surfaces interpenetrate in a closed loop around the extremum
/// and `t = √(2 δ / λ)` steps directly onto the loop; when `λ · δ <= 0` for both
/// eigenvalues, the surfaces are locally separated (Case IV) and `None` is returned.
#[allow(clippy::too_many_arguments)]
fn quadratic_collinear_loop_escape(
    chart_a: &LocalSurfaceChart,
    chart_b: &LocalSurfaceChart,
    x_cur: [f64; 4],
    ea: &ChartEval,
    eb: &ChartEval,
    d_vec: V3,
    gap: f64,
    step_cap: f64,
) -> Option<[f64; 4]> {
    if collinear_angle_rad(ea.normal, eb.normal) > ESCAPE_MAX_COLLINEAR_ANGLE_RAD {
        return None;
    }
    let sigma_sign = if dot3(ea.normal, eb.normal) >= 0.0 {
        1.0
    } else {
        -1.0
    };
    let delta_n = dot3(d_vec, ea.normal);
    if delta_n.abs() <= ESCAPE_MIN_NORMAL_GAP_FRAC * gap {
        return None;
    }
    let b0 = normalize3(ea.su);
    let b1 = normalize3(sub3(ea.sv, scale3(b0, dot3(ea.sv, b0))));
    let mut m = [[0.0; 2]; 2];
    for (col, basis_vec) in [b0, b1].into_iter().enumerate() {
        let ca = solve_in_tangent_frame(ea.su, ea.sv, basis_vec);
        let cb = solve_in_tangent_frame(eb.su, eb.sv, basis_vec);
        let dna = add3(scale3(ea.nu, ca[0]), scale3(ea.nv, ca[1]));
        let dnb = add3(scale3(eb.nu, cb[0]), scale3(eb.nv, cb[1]));
        let d_rel = sub3(dna, scale3(dnb, sigma_sign));
        m[0][col] = dot3(b0, d_rel);
        m[1][col] = dot3(b1, d_rel);
    }
    let m00 = m[0][0];
    let m11 = m[1][1];
    let m01 = 0.5 * (m[0][1] + m[1][0]);
    let tr = m00 + m11;
    let diff = m00 - m11;
    let disc = (diff * diff + 4.0 * m01 * m01).sqrt();
    let eigs = [0.5 * (tr + disc), 0.5 * (tr - disc)];

    // Pick the interpenetrating principal direction (`lam * delta_n > 0`) with
    // largest `|lam|` so the step `√(2 δ / λ)` to the loop's minor-axis vertex
    // is minimal.
    let mut best_lam: Option<f64> = None;
    for lam in eigs {
        if lam * delta_n > ESCAPE_MIN_CURVATURE_GAP_PROD
            && best_lam.is_none_or(|prev| lam.abs() > prev.abs())
        {
            best_lam = Some(lam);
        }
    }
    let lam = best_lam?;
    let v2d = if m01.abs() > ESCAPE_OFFDIAG_REL_TOL * (m00.abs() + m11.abs() + disc) {
        let vx = m01;
        let vy = lam - m00;
        let vn = vx.hypot(vy);
        [vx / vn, vy / vn]
    } else if (lam - m00).abs() <= (lam - m11).abs() {
        [1.0, 0.0]
    } else {
        [0.0, 1.0]
    };

    let t_step = (2.0 * delta_n / lam).sqrt().min(step_cap);
    let w_vec = scale3(add3(scale3(b0, v2d[0]), scale3(b1, v2d[1])), t_step);
    let ca = solve_in_tangent_frame(ea.su, ea.sv, w_vec);
    let cb = solve_in_tangent_frame(eb.su, eb.sv, w_vec);
    for sign in [1.0, -1.0] {
        let x_try = [
            x_cur[0] + sign * ca[0],
            x_cur[1] + sign * ca[1],
            x_cur[2] + sign * cb[0],
            x_cur[3] + sign * cb[1],
        ];
        if let (Some(ta), Some(tb)) = (
            chart_a.eval(x_try[0], x_try[1]),
            chart_b.eval(x_try[2], x_try[3]),
        ) {
            if norm3(sub3(ta.point, tb.point)) < ESCAPE_ACCEPT_GAP_FRAC * gap {
                return Some(x_try);
            }
        }
    }
    None
}

/// Yang et al. 2025 §4.3.1 & Appendix C (`yang2025_hybrid_boolean.txt:479-492` R,
/// `:1232-1250` R): minimum-norm parametric Newton solving
/// `D(u, v, s, t) = s_1(u, v) - s_2(s, t) = 0` via
/// `∇D ∇D^T a_k = s_2 - s_1`, `x_{k+1} = x_k + ∇D^T a_k`.
pub(crate) fn solve_parametric_pair_newton(
    p_seed: Point3,
    q_seed: Point3,
    s_a: Surface,
    s_b: Surface,
) -> Option<ParametricPairSolution> {
    let mut chart_a = LocalSurfaceChart::new(s_a, p_seed)?;
    let mut chart_b = LocalSurfaceChart::new(s_b, q_seed)?;
    let (u0, v0) = chart_a.project(p_seed.as_array())?;
    let (s0, t0) = chart_b.project(q_seed.as_array())?;
    let (tau, scale) = coincidence_floor(p_seed, q_seed, s_a, s_b);
    let seed_sep = norm3(sub3(p_seed.as_array(), q_seed.as_array()));
    let step_ref_scale = seed_sep + scale;
    let lin_cap = LINEAR_STEP_CAP_FACTOR * step_ref_scale;
    let curv_cap = match (
        chart_a.characteristic_radius(),
        chart_b.characteristic_radius(),
    ) {
        (Some(ra), Some(rb)) => PAIR_CURV_STEP_CAP_RAD * ra.min(rb),
        (Some(r), None) | (None, Some(r)) => PAIR_CURV_STEP_CAP_RAD * r,
        (None, None) => step_ref_scale,
    };

    let try_accept = |x_cur: [f64; 4], ea: &ChartEval, eb: &ChartEval, gap: f64| {
        if gap > tau {
            return None;
        }
        let mid = scale3(add3(ea.point, eb.point), 0.5);
        let (fa, _) = surface_distance_and_normal(s_a, mid)?;
        let (fb, _) = surface_distance_and_normal(s_b, mid)?;
        if fa.abs() <= tau && fb.abs() <= tau {
            Some(ParametricPairSolution {
                point: Point3::new(mid[0], mid[1], mid[2]),
                uv_a: (x_cur[0], x_cur[1]),
                uv_b: (x_cur[2], x_cur[3]),
                normal_a: ea.normal,
                normal_b: eb.normal,
                surface_gap: gap,
                normal_angle_rad: collinear_angle_rad(ea.normal, eb.normal),
            })
        } else {
            None
        }
    };

    let mut x = [u0, v0, s0, t0];
    for it in 0..=PAIR_NEWTON_MAX_ITERS {
        let mut ea = chart_a.eval(x[0], x[1])?;
        let mut uv_a = (x[0], x[1]);
        if chart_a.recenter_sphere_if_needed(s_a, &mut uv_a, ea.point)? {
            x[0] = uv_a.0;
            x[1] = uv_a.1;
            ea = chart_a.eval(x[0], x[1])?;
        }
        let mut eb = chart_b.eval(x[2], x[3])?;
        let mut uv_b = (x[2], x[3]);
        if chart_b.recenter_sphere_if_needed(s_b, &mut uv_b, eb.point)? {
            x[2] = uv_b.0;
            x[3] = uv_b.1;
            eb = chart_b.eval(x[2], x[3])?;
        }
        let d_vec = sub3(ea.point, eb.point);
        let gap = norm3(d_vec);
        if gap == 0.0 {
            if let Some(sol) = try_accept(x, &ea, &eb, gap) {
                return Some(sol);
            }
        }

        // `A = ∇D^T` (4 × 3): columns are rows of `∇D = [su_a, sv_a, -su_b, -sv_b]`.
        let a_mat: [[f64; 3]; 4] = [
            [ea.su[0], ea.su[1], ea.su[2]],
            [ea.sv[0], ea.sv[1], ea.sv[2]],
            [-eb.su[0], -eb.su[1], -eb.su[2]],
            [-eb.sv[0], -eb.sv[1], -eb.sv[2]],
        ];

        // Stationarity check: `∇(‖D‖²/2) = A · D = 0` at a strictly positive
        // separation is either a Sederberg collinear-normal interpenetration
        // extremum inside a closed small loop (escaped in one second-order
        // step) or non-intersecting surfaces (Case IV).
        let mut grad_sq = 0.0;
        for row in &a_mat {
            let g = row[0] * d_vec[0] + row[1] * d_vec[1] + row[2] * d_vec[2];
            grad_sq += g * g;
        }
        if grad_sq.sqrt() <= STATIONARY_GRAD_REL_TOL * gap.max(tau) {
            if let Some(x_esc) = quadratic_collinear_loop_escape(
                &chart_a, &chart_b, x, &ea, &eb, d_vec, gap, curv_cap,
            ) {
                x = x_esc;
                continue;
            }
            return try_accept(x, &ea, &eb, gap);
        }

        let (u_svd, sigma, v_svd) = thin_svd::<4, 3>(a_mat);
        if sigma[0] <= SVD_ZERO_SIGMA_TOL {
            return try_accept(x, &ea, &eb, gap);
        }
        // `∇D = V Σ U^T` ⇒ minimum-norm step `Δ = -U Σ^+ V^T D`.
        let svd_cut = PAIR_SVD_REL_CUTOFF * sigma[0];
        let mut delta = [0.0; 4];
        for j in 0..3 {
            if sigma[j] <= svd_cut {
                continue;
            }
            let vd = v_svd[0][j] * d_vec[0] + v_svd[1][j] * d_vec[1] + v_svd[2][j] * d_vec[2];
            let coeff = -vd / sigma[j];
            for i in 0..4 {
                delta[i] += coeff * u_svd[i][j];
            }
        }

        let d_norm =
            (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2] + delta[3] * delta[3])
                .sqrt();
        if d_norm <= STEP_NORM_REL_TOL * step_ref_scale || it == PAIR_NEWTON_MAX_ITERS {
            return try_accept(x, &ea, &eb, gap);
        }
        let caps_a = chart_a.step_caps(
            [x[0], x[1]],
            [delta[0], delta[1]],
            PAIR_CURV_STEP_CAP_RAD,
            lin_cap,
        );
        let caps_b = chart_b.step_caps(
            [x[2], x[3]],
            [delta[2], delta[3]],
            PAIR_CURV_STEP_CAP_RAD,
            lin_cap,
        );
        let caps = [caps_a[0], caps_a[1], caps_b[0], caps_b[1]];
        let ratio = (0..4).fold(1.0_f64, |m, i| {
            if caps[i] > 0.0 {
                m.max(delta[i].abs() / caps[i])
            } else {
                m
            }
        });
        if ratio > 1.0 {
            let sc = 1.0 / ratio;
            for d in &mut delta {
                *d *= sc;
            }
        }

        let mut accepted = false;
        let mut alpha = 1.0;
        for _ls in 0..NEWTON_LINE_SEARCH_HALVINGS {
            let x_try = [
                x[0] + alpha * delta[0],
                x[1] + alpha * delta[1],
                x[2] + alpha * delta[2],
                x[3] + alpha * delta[3],
            ];
            if let (Some(ta), Some(tb)) = (
                chart_a.eval(x_try[0], x_try[1]),
                chart_b.eval(x_try[2], x_try[3]),
            ) {
                let try_gap = norm3(sub3(ta.point, tb.point));
                if try_gap < gap {
                    x = x_try;
                    accepted = true;
                    break;
                }
            }
            alpha *= 0.5;
        }
        if !accepted {
            return try_accept(x, &ea, &eb, gap);
        }
    }
    None
}

/// Yang et al. 2025 §4.3.1 / §4.3.3 (`yang2025_hybrid_boolean.txt:489-492` R,
/// `:518-540` R, `:567-569` L): coupled parametric tangency Newton solving
/// `{s_1(u, v) - s_2(s, t) = 0, n_1(u, v) - σ n_2(s, t) = 0}` to machine
/// precision.
///
/// Resolves 0D elliptic kisses, 0D hyperbolic (`X`-pinch) saddle tangencies,
/// and 1D tangent loci uniformly across all five analytic surfaces, while
/// rejecting separated near-misses (`gap > TAU_WORK·(1+scale)`), transversal
/// crossings, and 2D coincident surfaces (`rank <= 2`).
pub(crate) fn solve_tangent_point_newton(
    p_seed: Point3,
    q_seed: Point3,
    s_a: Surface,
    s_b: Surface,
    max_drift: Option<f64>,
) -> Option<TangentSolution> {
    let chart_a = LocalSurfaceChart::new(s_a, p_seed)?;
    let chart_b = LocalSurfaceChart::new(s_b, q_seed)?;
    let (u0, v0) = chart_a.project(p_seed.as_array())?;
    let (s0, t0) = chart_b.project(q_seed.as_array())?;
    let ea0 = chart_a.eval(u0, v0)?;
    let eb0 = chart_b.eval(s0, t0)?;
    let (tau, scale) = coincidence_floor(p_seed, q_seed, s_a, s_b);
    let seed_sep = norm3(sub3(p_seed.as_array(), q_seed.as_array()));
    let step_ref_scale = seed_sep + scale;

    let w_ang = match (
        chart_a.characteristic_radius(),
        chart_b.characteristic_radius(),
    ) {
        (Some(ra), Some(rb)) => ra.min(rb),
        (Some(r), None) | (None, Some(r)) => r,
        (None, None) => {
            if step_ref_scale > 0.0 {
                step_ref_scale
            } else {
                1.0
            }
        }
    };
    let lin_cap = LINEAR_STEP_CAP_FACTOR * step_ref_scale;

    let dot0 = dot3(ea0.normal, eb0.normal);
    let signs: &[f64] = if dot0 > TANGENT_SIGN_LOCK_COS {
        &[1.0]
    } else if dot0 < -TANGENT_SIGN_LOCK_COS {
        &[-1.0]
    } else if dot0 >= 0.0 {
        &[1.0, -1.0]
    } else {
        &[-1.0, 1.0]
    };

    let mut best_sol: Option<(f64, TangentSolution)> = None;
    for &sigma_sign in signs {
        if let Some(sol) = solve_tangent_with_sign(
            chart_a,
            chart_b,
            [u0, v0, s0, t0],
            p_seed,
            s_a,
            s_b,
            sigma_sign,
            w_ang,
            lin_cap,
            step_ref_scale,
            tau,
            scale,
            max_drift,
        ) {
            let drift = norm3(sub3(sol.point.as_array(), p_seed.as_array()));
            if best_sol.as_ref().is_none_or(|(d, _)| drift < *d) {
                best_sol = Some((drift, sol));
            }
        }
    }
    best_sol.map(|(_, s)| s)
}

#[allow(clippy::too_many_arguments)]
fn solve_tangent_with_sign(
    mut chart_a: LocalSurfaceChart,
    mut chart_b: LocalSurfaceChart,
    x_init: [f64; 4],
    p_seed: Point3,
    s_a: Surface,
    s_b: Surface,
    sigma_sign: f64,
    w_ang: f64,
    lin_cap: f64,
    step_ref_scale: f64,
    tau: f64,
    scale: f64,
    max_drift: Option<f64>,
) -> Option<TangentSolution> {
    let gap_machine_tol = TANGENT_GAP_MACHINE_EPS_MULT * f64::EPSILON * scale;
    let ang_machine_tol = TANGENT_ANG_MACHINE_EPS_MULT * f64::EPSILON;
    let try_accept = |x_cur: [f64; 4],
                      ea: &ChartEval,
                      eb: &ChartEval,
                      rank: usize,
                      aligned: bool,
                      gap: f64,
                      ang: f64|
     -> Option<TangentSolution> {
        if !aligned || gap > tau || ang > TANGENT_SOLVER_MAX_ANGLE_RAD {
            return None;
        }
        let mid = scale3(add3(ea.point, eb.point), 0.5);
        if let Some(limit) = max_drift {
            if norm3(sub3(mid, p_seed.as_array())) > limit {
                return None;
            }
        }
        let (fa, _) = surface_distance_and_normal(s_a, mid)?;
        let (fb, _) = surface_distance_and_normal(s_b, mid)?;
        if fa.abs() > tau || fb.abs() > tau {
            return None;
        }
        let kind = if rank == 4 {
            classify_isolated_curvature(ea, eb, sigma_sign)
        } else {
            TangencyKind::TangentCurve
        };
        Some(TangentSolution {
            point: Point3::new(mid[0], mid[1], mid[2]),
            uv_a: (x_cur[0], x_cur[1]),
            uv_b: (x_cur[2], x_cur[3]),
            normal_a: ea.normal,
            normal_b: eb.normal,
            surface_gap: gap,
            normal_angle_rad: ang,
            kind,
        })
    };

    let mut x = x_init;
    let mut reanchored = false;
    for it in 0..=TANGENT_NEWTON_MAX_ITERS {
        let mut ea = chart_a.eval(x[0], x[1])?;
        let mut uv_a = (x[0], x[1]);
        if chart_a.recenter_sphere_if_needed(s_a, &mut uv_a, ea.point)? {
            x[0] = uv_a.0;
            x[1] = uv_a.1;
            ea = chart_a.eval(x[0], x[1])?;
        }
        let mut eb = chart_b.eval(x[2], x[3])?;
        let mut uv_b = (x[2], x[3]);
        if chart_b.recenter_sphere_if_needed(s_b, &mut uv_b, eb.point)? {
            x[2] = uv_b.0;
            x[3] = uv_b.1;
            eb = chart_b.eval(x[2], x[3])?;
        }

        let pre_gap = norm3(sub3(ea.point, eb.point));
        let pre_ang = collinear_angle_rad(ea.normal, eb.normal);
        let pre_aligned = dot3(ea.normal, scale3(eb.normal, sigma_sign)) > 0.0;
        if !reanchored
            && pre_aligned
            && pre_gap <= TANGENT_REANCHOR_GAP_REL_TOL * scale.max(w_ang)
            && pre_ang <= TANGENT_REANCHOR_ANGLE_RAD
        {
            let mid = scale3(add3(ea.point, eb.point), 0.5);
            let mid_pt = Point3::new(mid[0], mid[1], mid[2]);
            if let (Some(ca_new), Some(cb_new)) = (
                LocalSurfaceChart::new(s_a, mid_pt),
                LocalSurfaceChart::new(s_b, mid_pt),
            ) {
                if let (Some((ua, va)), Some((ub, vb))) =
                    (ca_new.project(ea.point), cb_new.project(eb.point))
                {
                    if let (Some(ea_new), Some(eb_new)) = (ca_new.eval(ua, va), cb_new.eval(ub, vb))
                    {
                        chart_a = ca_new;
                        chart_b = cb_new;
                        x = [ua, va, ub, vb];
                        ea = ea_new;
                        eb = eb_new;
                        reanchored = true;
                    }
                }
            }
        }

        let target_n = scale3(eb.normal, sigma_sign);
        let (l0v, l1v) = ortho_basis(Vector3::new(target_n[0], target_n[1], target_n[2]));
        let (l0, l1) = (l0v.as_array(), l1v.as_array());

        let d_pos = sub3(ea.point, eb.point);
        let d_n = sub3(ea.normal, target_n);
        let f_vec = [
            d_pos[0],
            d_pos[1],
            d_pos[2],
            w_ang * dot3(l0, d_n),
            w_ang * dot3(l1, d_n),
        ];
        let j_mat: [[f64; 4]; 5] = [
            [ea.su[0], ea.sv[0], -eb.su[0], -eb.sv[0]],
            [ea.su[1], ea.sv[1], -eb.su[1], -eb.sv[1]],
            [ea.su[2], ea.sv[2], -eb.su[2], -eb.sv[2]],
            [
                w_ang * dot3(l0, ea.nu),
                w_ang * dot3(l0, ea.nv),
                -w_ang * sigma_sign * dot3(l0, eb.nu),
                -w_ang * sigma_sign * dot3(l0, eb.nv),
            ],
            [
                w_ang * dot3(l1, ea.nu),
                w_ang * dot3(l1, ea.nv),
                -w_ang * sigma_sign * dot3(l1, eb.nu),
                -w_ang * sigma_sign * dot3(l1, eb.nv),
            ],
        ];

        let (u_svd, sv, v_svd) = thin_svd::<5, 4>(j_mat);
        if sv[0] <= SVD_ZERO_SIGMA_TOL {
            return None;
        }
        let rank = sv
            .iter()
            .filter(|&&s| s > TANGENT_RANK_REL_TOL * sv[0])
            .count();
        // `rank <= 2` means a 2D coincident surface (e.g. coplanar planes,
        // concentric spheres, or coaxial cylinders/cones/tori), never an
        // isolated tangent point or 1D tangent curve.
        if rank <= 2 {
            return None;
        }

        let gap = norm3(d_pos);
        let aligned = dot3(ea.normal, target_n) > 0.0;
        let ang = collinear_angle_rad(ea.normal, eb.normal);
        // Only exit early before polishing after re-anchoring and once BOTH
        // position and normal angle have reached IEEE-754 machine floor.
        if reanchored && aligned && gap <= gap_machine_tol && ang <= ang_machine_tol {
            if let Some(sol) = try_accept(x, &ea, &eb, rank, aligned, gap, ang) {
                return Some(sol);
            }
        }

        // Gauss–Newton normal-equation gradient check: if `J^T F ≈ 0`, `F_tan`
        // is stationary (either at an exact machine-precision tangency root or
        // at a strictly positive least-squares minimum on separated/transversal
        // surfaces).
        let mut grad = [0.0_f64; 4];
        for (j_row, &fv) in j_mat.iter().zip(f_vec.iter()) {
            for (g, &jc) in grad.iter_mut().zip(j_row.iter()) {
                *g += jc * fv;
            }
        }
        let grad_sq: f64 = grad.iter().map(|g| g * g).sum();
        let f_norm = (f_vec[0] * f_vec[0]
            + f_vec[1] * f_vec[1]
            + f_vec[2] * f_vec[2]
            + f_vec[3] * f_vec[3]
            + f_vec[4] * f_vec[4])
            .sqrt();
        if grad_sq.sqrt() <= STATIONARY_GRAD_REL_TOL * f_norm.max(tau) {
            return try_accept(x, &ea, &eb, rank, aligned, gap, ang);
        }

        let svd_cut = TANGENT_SVD_REL_CUTOFF * sv[0];
        let mut delta = [0.0; 4];
        for j in 0..4 {
            if sv[j] <= svd_cut {
                continue;
            }
            let mut uf = 0.0;
            for i in 0..5 {
                uf += u_svd[i][j] * f_vec[i];
            }
            let coeff = -uf / sv[j];
            for i in 0..4 {
                delta[i] += coeff * v_svd[i][j];
            }
        }

        let d_norm =
            (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2] + delta[3] * delta[3])
                .sqrt();
        if d_norm <= STEP_NORM_REL_TOL * step_ref_scale || it == TANGENT_NEWTON_MAX_ITERS {
            return try_accept(x, &ea, &eb, rank, aligned, gap, ang);
        }
        let caps_a = chart_a.step_caps(
            [x[0], x[1]],
            [delta[0], delta[1]],
            TANGENT_CURV_STEP_CAP_RAD,
            lin_cap,
        );
        let caps_b = chart_b.step_caps(
            [x[2], x[3]],
            [delta[2], delta[3]],
            TANGENT_CURV_STEP_CAP_RAD,
            lin_cap,
        );
        let caps = [caps_a[0], caps_a[1], caps_b[0], caps_b[1]];
        let ratio = (0..4).fold(1.0_f64, |m, i| {
            if caps[i] > 0.0 {
                m.max(delta[i].abs() / caps[i])
            } else {
                m
            }
        });
        if ratio > 1.0 {
            let sc = 1.0 / ratio;
            for d in &mut delta {
                *d *= sc;
            }
        }

        let mut accepted = false;
        let mut alpha = 1.0;
        for _ls in 0..NEWTON_LINE_SEARCH_HALVINGS {
            let x_try = [
                x[0] + alpha * delta[0],
                x[1] + alpha * delta[1],
                x[2] + alpha * delta[2],
                x[3] + alpha * delta[3],
            ];
            if let (Some(ta), Some(tb)) = (
                chart_a.eval(x_try[0], x_try[1]),
                chart_b.eval(x_try[2], x_try[3]),
            ) {
                if let Some(limit) = max_drift {
                    let mid_try = scale3(add3(ta.point, tb.point), 0.5);
                    if norm3(sub3(mid_try, p_seed.as_array())) > LINE_SEARCH_DRIFT_SLACK * limit {
                        alpha *= 0.5;
                        continue;
                    }
                }
                let t_n = scale3(tb.normal, sigma_sign);
                let dp = sub3(ta.point, tb.point);
                let dn = sub3(ta.normal, t_n);
                let f_try = [
                    dp[0],
                    dp[1],
                    dp[2],
                    w_ang * dot3(l0, dn),
                    w_ang * dot3(l1, dn),
                ];
                let try_norm = (f_try[0] * f_try[0]
                    + f_try[1] * f_try[1]
                    + f_try[2] * f_try[2]
                    + f_try[3] * f_try[3]
                    + f_try[4] * f_try[4])
                    .sqrt();
                let try_ang = collinear_angle_rad(ta.normal, tb.normal);
                if try_norm < f_norm
                    || (norm3(dp) <= gap_machine_tol
                        && try_ang <= ang_machine_tol
                        && dot3(ta.normal, t_n) > 0.0)
                {
                    x = x_try;
                    accepted = true;
                    break;
                }
            }
            alpha *= 0.5;
        }
        if !accepted {
            return try_accept(x, &ea, &eb, rank, aligned, gap, ang);
        }
    }
    None
}

/// Distinguish an elliptic 0D kiss (`det(ΔII) >= 0`) from a hyperbolic saddle
/// `X`-junction (`det(ΔII) < 0`) via the `2 × 2` relative shape operator
/// `M_rel = B^T N_A - σ B^T N_B (C^T B)` in an orthonormal basis `B` of the
/// common tangent plane.
fn classify_isolated_curvature(ea: &ChartEval, eb: &ChartEval, sigma_sign: f64) -> TangencyKind {
    let b0 = normalize3(ea.su);
    let b1 = normalize3(sub3(ea.sv, scale3(b0, dot3(ea.sv, b0))));
    let mut m = [[0.0; 2]; 2];
    for (col, basis_vec) in [b0, b1].into_iter().enumerate() {
        let ca = solve_in_tangent_frame(ea.su, ea.sv, basis_vec);
        let cb = solve_in_tangent_frame(eb.su, eb.sv, basis_vec);
        let dna = add3(scale3(ea.nu, ca[0]), scale3(ea.nv, ca[1]));
        let dnb = add3(scale3(eb.nu, cb[0]), scale3(eb.nv, cb[1]));
        let d_rel = sub3(dna, scale3(dnb, sigma_sign));
        m[0][col] = dot3(b0, d_rel);
        m[1][col] = dot3(b1, d_rel);
    }
    let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    if det < 0.0 {
        TangencyKind::IsolatedSaddle
    } else {
        TangencyKind::IsolatedKiss
    }
}

/// Uniform chart-evaluation test for whether `s_a` and `s_b` coincide as 2D
/// surfaces in a local neighborhood of `pt` (zero `Surface` variant branching).
fn surfaces_coincident_2d(s_a: Surface, s_b: Surface, pt: Point3, tau: f64, scale: f64) -> bool {
    let Some(ca) = LocalSurfaceChart::new(s_a, pt) else {
        return false;
    };
    let Some(cb) = LocalSurfaceChart::new(s_b, pt) else {
        return false;
    };
    let h = match (ca.characteristic_radius(), cb.characteristic_radius()) {
        (Some(ra), Some(rb)) => COINCIDENCE_PROBE_STEP_FRAC * ra.min(rb),
        (Some(r), None) | (None, Some(r)) => COINCIDENCE_PROBE_STEP_FRAC * r,
        (None, None) => {
            if scale > 0.0 {
                COINCIDENCE_PROBE_STEP_FRAC * scale
            } else {
                COINCIDENCE_PROBE_STEP_FRAC
            }
        }
    };
    for (du, dv) in [(h, 0.0), (-h, 0.0), (0.0, h), (0.0, -h)] {
        let Some(ea) = ca.eval(du, dv) else {
            return false;
        };
        let Some((fb, nb)) = surface_distance_and_normal(s_b, ea.point) else {
            return false;
        };
        if fb.abs() > tau || collinear_angle_rad(ea.normal, nb) > TANGENT_SOLVER_MAX_ANGLE_RAD {
            return false;
        }
    }
    true
}

/// Yang et al. 2025 §4.3.3 (`yang2025_hybrid_boolean.txt:523-540` R,
/// `:567-573` L, `:745` R): optimize an isolated seed pair `(p_seed, q_seed)`
/// on `(s_a, s_b)` by surface-pair Newton first, then compare normal vectors
/// at the converged intersection point.
pub(crate) fn solve_and_classify_seed(
    p_seed: Point3,
    q_seed: Point3,
    s_a: Surface,
    s_b: Surface,
    max_drift: Option<f64>,
) -> SeedOptimizationResult {
    let Some(pair_sol) = solve_parametric_pair_newton(p_seed, q_seed, s_a, s_b) else {
        return SeedOptimizationResult::NoIntersection;
    };
    if let Some(limit) = max_drift {
        if norm3(sub3(pair_sol.point.as_array(), p_seed.as_array())) > limit {
            return SeedOptimizationResult::NoIntersection;
        }
    }
    if pair_sol.normal_angle_rad > TANGENT_NORMAL_ANGLE_TOL {
        return SeedOptimizationResult::CurvePoint(pair_sol);
    }

    let (tau, scale) = coincidence_floor(pair_sol.point, pair_sol.point, s_a, s_b);
    if surfaces_coincident_2d(s_a, s_b, pair_sol.point, tau, scale) {
        return SeedOptimizationResult::CoincidentSurfaces;
    }

    match solve_tangent_point_newton(pair_sol.point, pair_sol.point, s_a, s_b, max_drift) {
        Some(tan_sol) if tan_sol.is_isolated_point() => SeedOptimizationResult::Tangent(tan_sol),
        _ => SeedOptimizationResult::TangencySolveFailed,
    }
}

/// §4.3.3 collinear-normal tangency polish called by
/// [`crate::stage4_relocate::relocate_onto_implicit_pair`] exclusively when
/// §4.3.2's normal-vector comparison detects parallel or near-parallel surface
/// normals (`sin θ <= MIN_FEATURE_SIZE`, `yang2025_hybrid_boolean.txt:489-492`,
/// `:501-505` R, `:536-569`).
///
/// Accepts **only** isolated 0D tangent points (`rank(J_tan) == 4`, i.e.
/// [`TangencyKind::IsolatedKiss`] or [`TangencyKind::IsolatedSaddle`]) within a
/// strictly scale-relative curvature neighborhood (`0.25 · r_min`) of `seed`,
/// so 2D coincident surfaces (`rank <= 2`) and 1D tangent rims/generators
/// (`rank == 3`) continue to return `None`.
pub(crate) fn solve_isolated_tangent_point_near(
    iterate: Point3,
    seed: Point3,
    s0: Surface,
    s1: Surface,
    residual_floor: f64,
) -> Option<Point3> {
    let c0 = LocalSurfaceChart::new(s0, iterate)?;
    let c1 = LocalSurfaceChart::new(s1, iterate)?;
    let max_drift = match (c0.characteristic_radius(), c1.characteristic_radius()) {
        (Some(r0), Some(r1)) => ISOLATED_TANGENT_DRIFT_RADIUS_FRAC * r0.min(r1),
        (Some(r), None) | (None, Some(r)) => ISOLATED_TANGENT_DRIFT_RADIUS_FRAC * r,
        (None, None) => return None,
    };
    let sol = solve_tangent_point_newton(iterate, iterate, s0, s1, Some(max_drift))?;
    if !sol.is_isolated_point() {
        return None;
    }
    let qa = sol.point.as_array();
    if norm3(sub3(qa, seed.as_array())) > max_drift {
        return None;
    }
    let (f0, _) = surface_distance_and_normal(s0, qa)?;
    let (f1, _) = surface_distance_and_normal(s1, qa)?;
    if f0.abs() > residual_floor || f1.abs() > residual_floor {
        return None;
    }
    s43_tangency_census_log(
        "pair_newton_tangency_resolved",
        &format!(
            "kind={:?} gap={:.3e} ang={:.3e} surfs={}×{}",
            sol.kind,
            sol.surface_gap,
            sol.normal_angle_rad,
            surface_kind_tag(s0),
            surface_kind_tag(s1)
        ),
    );
    Some(sol.point)
}

/// Discover all isolated 0D surface-tangency points (`rank(J_tan) == 4`)
/// between cross-operand face pairs of `a` and `b` by seeding
/// [`solve_tangent_point_newton`] from `< 2d_ε` Stage-1 triangle proximity
/// pairs (Yang §4.2.1 Eq. (1) `:456-461` L + §4.3.1/§4.3.3 `:489-492` R,
/// `:518-540` R).
///
/// Keyed by `(face_idx_a, face_idx_b)` with deterministic deduplicated
/// `Point3` vectors. Uses zero surface-type-pair closed forms or pre-skips.
pub(crate) fn discover_operand_pair_tangent_points_newton(
    a: &BRep,
    b: &BRep,
) -> BTreeMap<(u32, u32), Vec<TangentSolution>> {
    let mut out: BTreeMap<(u32, u32), Vec<TangentSolution>> = BTreeMap::new();
    let (Ok(set_a), Ok(set_b)) = (TriSet::new(a), TriSet::new(b)) else {
        return out;
    };
    if set_a.pts.is_empty() || set_b.pts.is_empty() {
        return out;
    }
    let tri_face_a = a.tri_face();
    let tri_face_b = b.tri_face();
    let scale = set_a
        .lo
        .iter()
        .chain(set_a.hi.iter())
        .chain(set_b.lo.iter())
        .chain(set_b.hi.iter())
        .flat_map(|v| v.iter())
        .fold(0.0_f64, |m, &c| m.max(c.abs()));
    let band =
        PROXIMITY_BAND_FACTOR * cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK);
    let max_dev_b = set_b.dev.iter().copied().fold(0.0_f64, f64::max);
    let bvh_b = Bvh::new(&set_b);

    // Collect Fig. 10(b)–(c) seeds per `(fa, fb)` (`yang2025_hybrid_boolean.txt:471-473` R,
    // `:489-499` L): filter `< 2d_ε` pairs whose closed-form Gauss maps overlap
    // into red sectors on `M_A`, locate the nearest triangle `T` per sector
    // (local minimum of `d(△t_A, fb)` over vertex-adjacent qualifying triangles
    // of `M_A`), and seed Newton from `p = barycenter(T)` and its exact closest
    // point `q` on `fb` in `M_B`.
    let mut sectors_by_pair: BTreeMap<(u32, u32), BTreeMap<usize, (usize, f64)>> = BTreeMap::new();
    for (ta_idx, &fa) in tri_face_a.iter().enumerate().take(set_a.pts.len()) {
        let surf_a = set_a.surf[ta_idx];
        let pad = set_a.dev[ta_idx] + max_dev_b + band;
        bvh_b.for_each_near(set_a.lo[ta_idx], set_a.hi[ta_idx], pad, |tb_idx_u32| {
            let tb_idx = tb_idx_u32 as usize;
            let fb = tri_face_b[tb_idx];
            let surf_b = set_b.surf[tb_idx];
            let reach = set_a.dev[ta_idx] + set_b.dev[tb_idx] + band;
            if (0..3).any(|k| {
                set_b.lo[tb_idx][k] > set_a.hi[ta_idx][k] + reach
                    || set_a.lo[ta_idx][k] > set_b.hi[tb_idx][k] + reach
            }) {
                return;
            }
            let d = triangle_distance(set_a.pts[ta_idx], set_b.pts[tb_idx]);
            if d > reach {
                return;
            }
            // §4.2.2 (`yang2025_hybrid_boolean.txt:440-446` R, Fig. 10(b)
            // `:489-491` L): a surface tangency requires `n_A = ±n_B`; prune
            // non-overlapping closed-form Gauss maps before Newton.
            let gm_a = GaussMap::from_triangle(&surf_a, set_a.pts[ta_idx].map(|p| p.as_array()));
            let gm_b = GaussMap::from_triangle(&surf_b, set_b.pts[tb_idx].map(|p| p.as_array()));
            if !gm_a.overlaps(&gm_b) {
                return;
            }
            sectors_by_pair
                .entry((fa, fb))
                .or_default()
                .entry(ta_idx)
                .and_modify(|cur| {
                    if d.total_cmp(&cur.1).then_with(|| tb_idx.cmp(&cur.0)).is_lt() {
                        *cur = (tb_idx, d);
                    }
                })
                .or_insert((tb_idx, d));
        });
    }

    let a_tris = &a.as_mesh().tris;
    let mut pair_seeds: BTreeMap<(u32, u32), Vec<(Point3, Point3)>> = BTreeMap::new();
    for ((fa, fb), sector_tas) in sectors_by_pair {
        let mut v_owners: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for &ta_idx in sector_tas.keys() {
            for &v in &a_tris[ta_idx] {
                v_owners.entry(v).or_default().push(ta_idx);
            }
        }
        let seeds = pair_seeds.entry((fa, fb)).or_default();
        for (&ta_idx, &(tb_init, min_d)) in &sector_tas {
            // For non-crossing sectors (`min_d > 0`, Fig. 10(b)–(c)), pick the
            // strict local-minimum triangle `(min_d, ta_idx)`. For crossing
            // triangles (`min_d == 0`, e.g. antipodal Steinmetz saddles on
            // `C0058` / `F0056`–`F0060` when run pre-Stage-2 under N82), every
            // Gauss-map-overlapping crossing triangle already attains `d = 0`
            // to `M_B` along the crossing curve, so breaking `0 == 0` ties by
            // `ta_idx` across the connected circumference would drop the second
            // saddle.
            let is_sector_min = min_d == 0.0
                || a_tris[ta_idx].iter().all(|v| {
                    v_owners[v].iter().all(|&other_ta| {
                        if other_ta == ta_idx {
                            return true;
                        }
                        let other_d = sector_tas[&other_ta].1;
                        min_d
                            .total_cmp(&other_d)
                            .then_with(|| ta_idx.cmp(&other_ta))
                            .is_lt()
                    })
                });
            if !is_sector_min {
                continue;
            }
            let ta_pts = set_a.pts[ta_idx].map(|p| p.as_array());
            let pa = scale3(add3(add3(ta_pts[0], ta_pts[1]), ta_pts[2]), 1.0 / 3.0);
            let tb_init_pts = set_b.pts[tb_init].map(|p| p.as_array());
            let mut best_pb =
                closest_on_triangle(pa, tb_init_pts[0], tb_init_pts[1], tb_init_pts[2]);
            let mut best_d2 = dot3(sub3(pa, best_pb), sub3(pa, best_pb));
            let mut best_tb = tb_init;
            let query_pad = best_d2.sqrt();
            bvh_b.for_each_near(pa, pa, query_pad, |tb_u32| {
                let tb = tb_u32 as usize;
                if tri_face_b.get(tb).copied() != Some(fb) {
                    return;
                }
                let t = set_b.pts[tb].map(|p| p.as_array());
                let cand = closest_on_triangle(pa, t[0], t[1], t[2]);
                let d2 = dot3(sub3(pa, cand), sub3(pa, cand));
                if d2
                    .total_cmp(&best_d2)
                    .then_with(|| tb.cmp(&best_tb))
                    .is_lt()
                {
                    best_d2 = d2;
                    best_pb = cand;
                    best_tb = tb;
                }
            });
            seeds.push((
                Point3::new(pa[0], pa[1], pa[2]),
                Point3::new(best_pb[0], best_pb[1], best_pb[2]),
            ));
        }
    }

    let merge_tol = cad_primitives::TAU_MODEL * (1.0 + scale);
    let merge_tol2 = merge_tol * merge_tol;

    for ((fa, fb), seeds) in pair_seeds {
        let surf_a = a.faces()[fa as usize].surface;
        let surf_b = b.faces()[fb as usize].surface;
        let mut found: Vec<TangentSolution> = Vec::new();
        for (pa, pb) in seeds {
            let Some(sol) = solve_tangent_point_newton(pa, pb, surf_a, surf_b, None) else {
                continue;
            };
            if !sol.is_isolated_point() {
                continue;
            }
            let qa = sol.point.as_array();
            if let Some(existing) = found.iter_mut().find(|existing| {
                dot3(
                    sub3(existing.point.as_array(), qa),
                    sub3(existing.point.as_array(), qa),
                ) <= merge_tol2
            }) {
                let err_new = sol.surface_gap + scale * sol.normal_angle_rad;
                let err_old = existing.surface_gap + scale * existing.normal_angle_rad;
                if err_new < err_old {
                    *existing = sol;
                }
                continue;
            }
            found.push(sol);
        }
        if !found.is_empty() {
            // Canonical coordinate ordering for determinism across triangle
            // traversal orders.
            found.sort_by(|x, y| {
                x.point
                    .x()
                    .total_cmp(&y.point.x())
                    .then_with(|| x.point.y().total_cmp(&y.point.y()))
                    .then_with(|| x.point.z().total_cmp(&y.point.z()))
            });
            out.insert((fa, fb), found);
        }
    }

    out
}
