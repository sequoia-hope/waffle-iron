//! The five analytic surfaces as [`ParamSurface`] instances.
//!
//! The embedding is the one `stage4_dt::eval_uv` pins (its module docs hold
//! the table), so Stage 1, d(T) and every later reader share one convention:
//!
//! | Surface | u | v | degenerate |
//! |---|---|---|---|
//! | Plane | in-plane `e1` | in-plane `e2` | — |
//! | Cylinder | azimuth, period 2π | axial offset | — |
//! | Cone | azimuth, period 2π | axial distance from the apex, ≥ 0 | v = 0 |
//! | Sphere | azimuth, period 2π | latitude, [−π/2, π/2] | v = ±π/2 |
//! | Torus | azimuth, period 2π | tube angle, period 2π | — |
//!
//! Frames are `ortho_basis`, which is right-handed (`e1 × e2 = n̂`), so
//! `s_u × s_v` is outward on all five. The sphere's axis is the pinned world
//! ẑ: its poles sit at world ±z, not in a face's own frame — a property of
//! the embedding, not of the paper.
//!
//! [`ParamSurface::bezier_patches`] builds the exact nets of Piegl & Tiller
//! ch. 7–8 that `stage4_dt::d_of_t` builds: each angular span split into
//! `ceil(span / (π/2))` equal pieces, so every middle weight
//! `cos(Δ/2) ≥ √2/2`; a degree-1 profile line for the cylinder and cone, a
//! rational-quadratic arc for the sphere and torus.

use super::{
    ParamAxis, ParamDomain, ParamRect, ParamSurface, RationalBezierPatch, SurfaceDerivs,
    SurfaceError, Uv,
};
use crate::{normalize3, ortho_basis, Surface};
use cad_primitives::{Point3, Vector3};
use std::f64::consts::{FRAC_PI_2, PI};

const TWO_PI: f64 = 2.0 * PI;

fn finite3(v: [f64; 3]) -> bool {
    v[0].is_finite() && v[1].is_finite() && v[2].is_finite()
}

fn norm2(v: [f64; 3]) -> f64 {
    v[0] * v[0] + v[1] * v[1] + v[2] * v[2]
}

/// Revolution frame: datum, unit axis, and `(e1, e2) = ortho_basis(axis)`.
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
    /// `datum + z·â + ρ·(cos θ·e1 + sin θ·e2)` — the expression, operation
    /// for operation, of `stage4_dt`'s, so the two agree bit for bit.
    fn ring(&self, rho: f64, cos_t: f64, sin_t: f64, z: f64) -> [f64; 3] {
        [
            self.datum[0] + z * self.axis[0] + rho * (cos_t * self.e1[0] + sin_t * self.e2[0]),
            self.datum[1] + z * self.axis[1] + rho * (cos_t * self.e1[1] + sin_t * self.e2[1]),
            self.datum[2] + z * self.axis[2] + rho * (cos_t * self.e1[2] + sin_t * self.e2[2]),
        ]
    }

    /// `a·â + b·r(θ) + c·t(θ)` with `r = cos θ·e1 + sin θ·e2` (radial) and
    /// `t = −sin θ·e1 + cos θ·e2` (tangential): a derivative vector.
    fn vec(&self, a: f64, b: f64, c: f64, cos_t: f64, sin_t: f64) -> [f64; 3] {
        let mut out = [0.0; 3];
        for (k, o) in out.iter_mut().enumerate() {
            let r = cos_t * self.e1[k] + sin_t * self.e2[k];
            let t = -sin_t * self.e1[k] + cos_t * self.e2[k];
            *o = a * self.axis[k] + b * r + c * t;
        }
        out
    }
}

/// The profile in the (ρ radial, z axial) half-plane of a surface of
/// revolution: `datum + z·â + ρ·r(u)`.
#[derive(Copy, Clone)]
enum Profile {
    /// Cylinder (`ρ = r`) and cone (`ρ = v·tan α`): `ρ = offset + slope·v`,
    /// `z = v`.
    Line { offset: f64, slope: f64 },
    /// Sphere (`center_rho = 0`) and torus (`center_rho = R`): an arc of
    /// radius `r` about `(center_rho, 0)`, `ρ = center_rho + r cos v`,
    /// `z = r sin v`.
    Arc { center_rho: f64, r: f64 },
}

impl Profile {
    /// `(ρ, z, ρ', z', ρ'', z'')` at `v`.
    fn at(&self, v: f64) -> [f64; 6] {
        match *self {
            Profile::Line { offset, slope } => [offset + slope * v, v, slope, 1.0, 0.0, 0.0],
            Profile::Arc { center_rho, r } => {
                let (s, c) = v.sin_cos();
                [center_rho + r * c, r * s, -r * s, r * c, -r * c, -r * s]
            }
        }
    }

    /// Control rows `(ρ, z, weight)` over `[va, vb]`.
    fn rows(&self, va: f64, vb: f64) -> Vec<[f64; 3]> {
        match *self {
            Profile::Line { offset, slope } => vec![
                [offset + slope * va, va, 1.0],
                [offset + slope * vb, vb, 1.0],
            ],
            Profile::Arc { center_rho, r } => {
                let dv = vb - va;
                let m = 0.5 * (va + vb);
                let half = 0.5 * dv;
                let s = r / half.cos();
                vec![
                    [center_rho + r * va.cos(), r * va.sin(), 1.0],
                    [center_rho + s * m.cos(), s * m.sin(), half.cos()],
                    [center_rho + r * vb.cos(), r * vb.sin(), 1.0],
                ]
            }
        }
    }
}

/// A curved analytic surface as a revolved profile.
fn revolution(surface: &Surface) -> Option<(RevFrame, Profile)> {
    match *surface {
        Surface::Plane { .. } => None,
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => Some((
            rev_frame(axis_point, axis_dir),
            Profile::Line {
                offset: radius,
                slope: 0.0,
            },
        )),
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => Some((
            rev_frame(apex, axis_dir),
            Profile::Line {
                offset: 0.0,
                slope: half_angle.tan(),
            },
        )),
        Surface::Sphere { center, radius } => Some((
            rev_frame(center, Vector3::new(0.0, 0.0, 1.0)),
            Profile::Arc {
                center_rho: 0.0,
                r: radius,
            },
        )),
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } => Some((
            rev_frame(center, axis_dir),
            Profile::Arc {
                center_rho: major_radius,
                r: minor_radius,
            },
        )),
    }
}

/// Plane frame: the point `−d·n̂` and `(e1, e2) = ortho_basis(normal)`.
fn plane_frame(normal: Vector3, d: f64) -> ([f64; 3], [f64; 3], [f64; 3]) {
    let n = normalize3(normal.as_array());
    let (e1, e2) = ortho_basis(normal);
    (
        [-d * n[0], -d * n[1], -d * n[2]],
        e1.as_array(),
        e2.as_array(),
    )
}

impl Surface {
    /// The domain checks of one parameter point.
    fn check_point(&self, uv: Uv) -> Result<(), SurfaceError> {
        if !uv.x().is_finite() || !uv.y().is_finite() {
            return Err(SurfaceError::NonFiniteInput);
        }
        self.validate()?;
        self.check_v_range(uv.y(), uv.y())
    }

    fn check_v_range(&self, v0: f64, v1: f64) -> Result<(), SurfaceError> {
        match self {
            Surface::Sphere { .. } if v0 < -FRAC_PI_2 || v1 > FRAC_PI_2 => {
                Err(SurfaceError::OutsideDomain)
            }
            Surface::Cone { .. } if v0 < 0.0 => Err(SurfaceError::OutsideDomain),
            _ => Ok(()),
        }
    }
}

impl ParamSurface for Surface {
    fn validate(&self) -> Result<(), SurfaceError> {
        let (finite, valid) = match *self {
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
                finite3(axis_point.as_array())
                    && finite3(axis_dir.as_array())
                    && radius.is_finite(),
                radius > 0.0 && norm2(axis_dir.as_array()) > 0.0,
            ),
            Surface::Cone {
                apex,
                axis_dir,
                half_angle,
            } => (
                finite3(apex.as_array()) && finite3(axis_dir.as_array()) && half_angle.is_finite(),
                half_angle > 0.0 && half_angle < FRAC_PI_2 && norm2(axis_dir.as_array()) > 0.0,
            ),
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
                minor_radius > 0.0
                    && major_radius > minor_radius
                    && norm2(axis_dir.as_array()) > 0.0,
            ),
        };
        if !finite {
            return Err(SurfaceError::NonFiniteInput);
        }
        if !valid {
            return Err(SurfaceError::InvalidSurface);
        }
        Ok(())
    }

    fn domain(&self) -> ParamDomain {
        let free = ParamAxis {
            lo: f64::NEG_INFINITY,
            hi: f64::INFINITY,
            period: None,
        };
        let angle = ParamAxis {
            lo: 0.0,
            hi: TWO_PI,
            period: Some(TWO_PI),
        };
        match self {
            Surface::Plane { .. } => ParamDomain {
                u: free,
                v: free,
                degenerate: Vec::new(),
            },
            Surface::Cylinder { .. } => ParamDomain {
                u: angle,
                v: free,
                degenerate: Vec::new(),
            },
            Surface::Cone { .. } => ParamDomain {
                u: angle,
                v: ParamAxis {
                    lo: 0.0,
                    hi: f64::INFINITY,
                    period: None,
                },
                degenerate: vec![super::Degeneracy::VIso(0.0)],
            },
            Surface::Sphere { .. } => ParamDomain {
                u: angle,
                v: ParamAxis {
                    lo: -FRAC_PI_2,
                    hi: FRAC_PI_2,
                    period: None,
                },
                degenerate: vec![
                    super::Degeneracy::VIso(-FRAC_PI_2),
                    super::Degeneracy::VIso(FRAC_PI_2),
                ],
            },
            Surface::Torus { .. } => ParamDomain {
                u: angle,
                v: ParamAxis {
                    lo: -PI,
                    hi: PI,
                    period: Some(TWO_PI),
                },
                degenerate: Vec::new(),
            },
        }
    }

    fn is_affine(&self) -> bool {
        matches!(self, Surface::Plane { .. })
    }

    fn eval(&self, uv: Uv) -> Result<[f64; 3], SurfaceError> {
        self.check_point(uv)?;
        let (u, v) = (uv.x(), uv.y());
        Ok(match *self {
            Surface::Plane { normal, d } => {
                let (o, e1, e2) = plane_frame(normal, d);
                [
                    o[0] + u * e1[0] + v * e2[0],
                    o[1] + u * e1[1] + v * e2[1],
                    o[2] + u * e1[2] + v * e2[2],
                ]
            }
            _ => {
                let (frame, _) = revolution(self).ok_or(SurfaceError::InvalidSurface)?;
                // ρ and z written as `stage4_dt::eval_core` writes them, so
                // the two embeddings agree bit for bit.
                let (rho, z) = match *self {
                    Surface::Cylinder { radius, .. } => (radius, v),
                    Surface::Cone { half_angle, .. } => (v * half_angle.tan(), v),
                    Surface::Sphere { radius, .. } => (radius * v.cos(), radius * v.sin()),
                    Surface::Torus {
                        major_radius,
                        minor_radius,
                        ..
                    } => (
                        major_radius + minor_radius * v.cos(),
                        minor_radius * v.sin(),
                    ),
                    Surface::Plane { .. } => return Err(SurfaceError::InvalidSurface),
                };
                frame.ring(rho, u.cos(), u.sin(), z)
            }
        })
    }

    fn derivs(&self, uv: Uv) -> Result<SurfaceDerivs, SurfaceError> {
        let s = self.eval(uv)?;
        let (u, v) = (uv.x(), uv.y());
        if let Surface::Plane { normal, d } = *self {
            let (_, e1, e2) = plane_frame(normal, d);
            return Ok(SurfaceDerivs {
                s,
                su: e1,
                sv: e2,
                suu: [0.0; 3],
                suv: [0.0; 3],
                svv: [0.0; 3],
            });
        }
        let (frame, profile) = revolution(self).ok_or(SurfaceError::InvalidSurface)?;
        let [rho, _z, drho, dz, ddrho, ddz] = profile.at(v);
        let (sin_u, cos_u) = u.sin_cos();
        // s = datum + z(v)·â + ρ(v)·r(u); r' = t, t' = −r.
        Ok(SurfaceDerivs {
            s,
            su: frame.vec(0.0, 0.0, rho, cos_u, sin_u),
            sv: frame.vec(dz, drho, 0.0, cos_u, sin_u),
            suu: frame.vec(0.0, -rho, 0.0, cos_u, sin_u),
            suv: frame.vec(0.0, 0.0, drho, cos_u, sin_u),
            svv: frame.vec(ddz, ddrho, 0.0, cos_u, sin_u),
        })
    }

    fn bezier_patches(&self, rect: ParamRect) -> Result<Vec<RationalBezierPatch>, SurfaceError> {
        if !rect.is_finite() {
            return Err(SurfaceError::NonFiniteInput);
        }
        self.validate()?;
        self.check_v_range(rect.v0, rect.v1)?;
        if let Surface::Plane { normal, d } = *self {
            let (o, e1, e2) = plane_frame(normal, d);
            let at = |u: f64, v: f64| {
                [
                    o[0] + u * e1[0] + v * e2[0],
                    o[1] + u * e1[1] + v * e2[1],
                    o[2] + u * e1[2] + v * e2[2],
                ]
            };
            return Ok(vec![RationalBezierPatch {
                deg_u: 1,
                deg_v: 1,
                points: vec![
                    at(rect.u0, rect.v0),
                    at(rect.u0, rect.v1),
                    at(rect.u1, rect.v0),
                    at(rect.u1, rect.v1),
                ],
                weights: vec![1.0; 4],
                separable_weights: true,
            }]);
        }
        let span_u = rect.u1 - rect.u0;
        let span_v = rect.v1 - rect.v0;
        if span_u > TWO_PI {
            return Err(SurfaceError::SpanExceedsPeriod);
        }
        if matches!(self, Surface::Torus { .. }) && span_v > TWO_PI {
            return Err(SurfaceError::SpanExceedsPeriod);
        }
        let (frame, profile) = revolution(self).ok_or(SurfaceError::InvalidSurface)?;
        let n_u = ((span_u / FRAC_PI_2).ceil() as usize).max(1);
        let n_v = match profile {
            Profile::Arc { .. } => ((span_v / FRAC_PI_2).ceil() as usize).max(1),
            Profile::Line { .. } => 1,
        };
        let mut out = Vec::with_capacity(n_u * n_v);
        for iu in 0..n_u {
            let ua = rect.u0 + span_u * (iu as f64) / (n_u as f64);
            let ub = rect.u0 + span_u * ((iu + 1) as f64) / (n_u as f64);
            let du = ub - ua;
            let mu = 0.5 * (ua + ub);
            let u_scale = 1.0 / (0.5 * du).cos();
            let w_mid = (0.5 * du).cos();
            let az = [
                (1.0, ua.cos(), ua.sin(), 1.0),
                (u_scale, mu.cos(), mu.sin(), w_mid),
                (1.0, ub.cos(), ub.sin(), 1.0),
            ];
            for iv in 0..n_v {
                let va = rect.v0 + span_v * (iv as f64) / (n_v as f64);
                let vb = rect.v0 + span_v * ((iv + 1) as f64) / (n_v as f64);
                let rows = profile.rows(va, vb);
                let mut points = Vec::with_capacity(3 * rows.len());
                let mut weights = Vec::with_capacity(3 * rows.len());
                for &(scale, c, s, wu) in &az {
                    for &[rho, z, wv] in &rows {
                        points.push(frame.ring(rho * scale, c, s, z));
                        weights.push(wu * wv);
                    }
                }
                out.push(RationalBezierPatch {
                    deg_u: 2,
                    deg_v: rows.len() - 1,
                    points,
                    weights,
                    separable_weights: true,
                });
            }
        }
        Ok(out)
    }
}
