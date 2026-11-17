//! Pins of the F5a increment-1 interface (spec
//! `specs/yang_f5a_surface_interface.md` §6 item 1).

use super::dist_point_tri;
use super::*;
use crate::stage4_dt::{d_of_t, eval_uv};
use crate::Surface;
use cad_primitives::{Point3, Vector3};
use std::f64::consts::{FRAC_PI_2, PI};

/// Deterministic LCG in [0, 1) (Knuth MMIX constants); no system entropy.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next()
    }
}

fn uv(u: f64, v: f64) -> Uv {
    Uv::new(u, v)
}

/// One of each variant, off-origin and on tilted axes so no frame is the
/// identity.
fn surfaces() -> Vec<Surface> {
    let tilt = Vector3::new(0.3, -0.4, 0.86);
    vec![
        Surface::Plane {
            normal: Vector3::new(0.2, 0.5, -1.3),
            d: 0.7,
        },
        Surface::Cylinder {
            axis_point: Point3::new(0.1, -0.2, 0.3),
            axis_dir: tilt,
            radius: 1.7,
        },
        Surface::Cone {
            apex: Point3::new(-0.5, 0.25, 1.0),
            axis_dir: tilt,
            half_angle: 0.4,
        },
        Surface::Sphere {
            center: Point3::new(0.1, 0.2, 0.3),
            radius: 2.0,
        },
        Surface::Torus {
            center: Point3::new(0.4, -0.1, 0.2),
            axis_dir: tilt,
            major_radius: 3.0,
            minor_radius: 0.8,
        },
    ]
}

/// A parameter point inside the variant's domain.
fn random_uv(s: &Surface, rng: &mut Lcg) -> Uv {
    let u = rng.range(-2.0 * PI, 2.0 * PI);
    let v = match s {
        Surface::Plane { .. } | Surface::Cylinder { .. } => rng.range(-3.0, 3.0),
        Surface::Cone { .. } => rng.range(0.0, 3.0),
        Surface::Sphere { .. } => rng.range(-FRAC_PI_2, FRAC_PI_2),
        Surface::Torus { .. } => rng.range(-PI, PI),
    };
    uv(u, v)
}

/// A parameter triangle near `c` whose spans stay within one period.
fn random_tri(s: &Surface, rng: &mut Lcg) -> [Uv; 3] {
    let c = random_uv(s, rng);
    let size = rng.range(0.0, 1.2);
    let mut out = [c; 3];
    for p in out.iter_mut().skip(1) {
        let mut v = c.y() + rng.range(-size, size);
        match s {
            Surface::Cone { .. } => v = v.max(0.0),
            Surface::Sphere { .. } => v = v.clamp(-FRAC_PI_2, FRAC_PI_2),
            _ => {}
        }
        *p = uv(c.x() + rng.range(-size, size), v);
    }
    out
}

/// The embedding table written out directly — an oracle independent of
/// `analytic.rs` (frames from `ortho_basis`, the sphere on world ẑ).
fn table_eval(s: &Surface, p: Uv) -> [f64; 3] {
    let (u, v) = (p.x(), p.y());
    let ring = |datum: [f64; 3], axis: Vector3, rho: f64, z: f64| {
        let a = crate::normalize3(axis.as_array());
        let (e1, e2) = crate::ortho_basis(axis);
        let r = add3(
            scale3(e1.as_array(), u.cos()),
            scale3(e2.as_array(), u.sin()),
        );
        add3(add3(datum, scale3(a, z)), scale3(r, rho))
    };
    match *s {
        Surface::Plane { normal, d } => {
            let n = crate::normalize3(normal.as_array());
            let (e1, e2) = crate::ortho_basis(normal);
            add3(
                scale3(n, -d),
                add3(scale3(e1.as_array(), u), scale3(e2.as_array(), v)),
            )
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => ring(axis_point.as_array(), axis_dir, radius, v),
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => ring(apex.as_array(), axis_dir, v * half_angle.tan(), v),
        Surface::Sphere { center, radius } => ring(
            center.as_array(),
            Vector3::new(0.0, 0.0, 1.0),
            radius * v.cos(),
            radius * v.sin(),
        ),
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } => ring(
            center.as_array(),
            axis_dir,
            major_radius + minor_radius * v.cos(),
            minor_radius * v.sin(),
        ),
    }
}

#[test]
fn eval_is_the_embedding_table() {
    let mut rng = Lcg(1);
    for s in surfaces() {
        for _ in 0..2000 {
            let p = random_uv(&s, &mut rng);
            let ours = s.eval(p).unwrap();
            let want = table_eval(&s, p);
            let err = dot3(sub3(ours, want), sub3(ours, want)).sqrt();
            assert!(err < 1e-12, "{s:?} at {p:?}: {ours:?} vs {want:?}");
            // `stage4_dt::eval_uv` is this evaluation.
            assert_eq!(eval_uv(&s, p).unwrap().as_array(), ours);
        }
    }
}

#[test]
fn control_net_distance_bounds_the_surface_over_the_triangle() {
    // Theorem A.1: d(T) is an upper bound on the distance from the surface
    // over the triangle's parameter footprint to the 3D triangle. Sample the
    // footprint densely and check none exceeds it.
    let mut rng = Lcg(2);
    for s in surfaces() {
        for _ in 0..400 {
            let tri = random_tri(&s, &mut rng);
            let bound = control_net_distance(&s, tri).unwrap();
            assert_eq!(d_of_t(&s, tri).unwrap().to_bits(), bound.to_bits());
            let corners = tri.map(|c| s.eval(c).unwrap());
            let n = 12;
            for i in 0..=n {
                for j in 0..=(n - i) {
                    let (a, b) = (i as f64 / n as f64, j as f64 / n as f64);
                    let c = 1.0 - a - b;
                    let p = uv(
                        c * tri[0].x() + a * tri[1].x() + b * tri[2].x(),
                        c * tri[0].y() + a * tri[1].y() + b * tri[2].y(),
                    );
                    // Rounding can carry a convex combination a hair past
                    // a pole or the apex.
                    let p = match s {
                        Surface::Sphere { .. } => uv(p.x(), p.y().clamp(-FRAC_PI_2, FRAC_PI_2)),
                        Surface::Cone { .. } => uv(p.x(), p.y().max(0.0)),
                        _ => p,
                    };
                    let q = s.eval(p).unwrap();
                    let d = dist_point_tri(q, &corners);
                    assert!(
                        d <= bound * (1.0 + 1e-12) + 1e-14,
                        "{s:?} {tri:?}: surface point at {p:?} is {d:e} from the triangle, \
                         bound {bound:e}"
                    );
                }
            }
        }
    }
}

#[test]
fn control_net_distance_refuses_where_d_of_t_refuses() {
    let sphere = surfaces()[3];
    let cone = surfaces()[2];
    let cyl = surfaces()[1];
    let cases: Vec<(Surface, [Uv; 3])> = vec![
        (sphere, [uv(0.0, 0.0), uv(0.1, 1.6), uv(0.2, 0.0)]),
        (cone, [uv(0.0, -0.1), uv(0.1, 1.0), uv(0.2, 1.0)]),
        (cyl, [uv(0.0, 0.0), uv(7.0, 0.0), uv(0.1, 0.1)]),
        (cyl, [uv(f64::NAN, 0.0), uv(0.0, 0.0), uv(0.1, 0.1)]),
        (
            Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                radius: -1.0,
            },
            [uv(0.0, 0.0), uv(0.1, 0.0), uv(0.1, 0.1)],
        ),
    ];
    for (s, tri) in cases {
        assert!(control_net_distance(&s, tri).is_err(), "{s:?} {tri:?}");
        assert!(d_of_t(&s, tri).is_err(), "{s:?} {tri:?}");
    }
    // `stage4_dt` names each refusal in its own vocabulary.
    use crate::stage4_dt::DtError;
    assert_eq!(
        d_of_t(&sphere, [uv(0.0, 0.0), uv(0.1, 1.6), uv(0.2, 0.0)]),
        Err(DtError::PolarRangeOutOfBounds)
    );
    assert_eq!(
        d_of_t(&cone, [uv(0.0, -0.1), uv(0.1, 1.0), uv(0.2, 1.0)]),
        Err(DtError::NegativeConeAxialRange)
    );
    assert_eq!(
        d_of_t(&cyl, [uv(0.0, 0.0), uv(7.0, 0.0), uv(0.1, 0.1)]),
        Err(DtError::AzimuthSpanTooLarge)
    );
}

#[test]
fn su_cross_sv_is_the_outward_normal() {
    let mut rng = Lcg(3);
    for s in surfaces() {
        for _ in 0..500 {
            let p = random_uv(&s, &mut rng);
            let Ok(n) = s.normal(p) else { continue };
            let x = s.eval(p).unwrap();
            // The outward direction, independent of the parametrization.
            let out = match s {
                Surface::Plane { normal, .. } => normal.as_array(),
                Surface::Sphere { center, .. } => sub3(x, center.as_array()),
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    ..
                } => radial(x, axis_point.as_array(), axis_dir.as_array()),
                Surface::Cone {
                    apex,
                    axis_dir,
                    half_angle,
                } => {
                    // Radial away from the axis, tilted back toward the apex.
                    let a = crate::normalize3(axis_dir.as_array());
                    let r = radial(x, apex.as_array(), a);
                    let rl = dot3(r, r).sqrt();
                    let r = scale3(r, 1.0 / rl);
                    sub3(scale3(r, half_angle.cos()), scale3(a, half_angle.sin()))
                }
                Surface::Torus {
                    center,
                    axis_dir,
                    major_radius,
                    ..
                } => {
                    let a = crate::normalize3(axis_dir.as_array());
                    let r = radial(x, center.as_array(), a);
                    let rl = dot3(r, r).sqrt();
                    let tube = add3(center.as_array(), scale3(r, major_radius / rl));
                    sub3(x, tube)
                }
            };
            let ol = dot3(out, out).sqrt();
            let cos = dot3(n, out) / ol;
            assert!(cos > 1.0 - 1e-9, "{s:?} at {p:?}: n·out = {cos}");
        }
    }
}

fn radial(x: [f64; 3], datum: [f64; 3], axis: [f64; 3]) -> [f64; 3] {
    let a = crate::normalize3(axis);
    let w = sub3(x, datum);
    sub3(w, scale3(a, dot3(w, a)))
}

#[test]
fn derivatives_match_central_differences() {
    let mut rng = Lcg(4);
    let h = 1e-5;
    for s in surfaces() {
        for _ in 0..300 {
            let mut p = random_uv(&s, &mut rng);
            // Keep the stencil inside the domain.
            if let Surface::Cone { .. } = s {
                p = uv(p.x(), p.y() + 2.0 * h);
            }
            if let Surface::Sphere { .. } = s {
                p = uv(
                    p.x(),
                    p.y().clamp(-FRAC_PI_2 + 2.0 * h, FRAC_PI_2 - 2.0 * h),
                );
            }
            let d = s.derivs(p).unwrap();
            let e = |du: f64, dv: f64| s.eval(uv(p.x() + du, p.y() + dv)).unwrap();
            let fd_u = scale3(sub3(e(h, 0.0), e(-h, 0.0)), 0.5 / h);
            let fd_v = scale3(sub3(e(0.0, h), e(0.0, -h)), 0.5 / h);
            let fd_uu = scale3(
                add3(sub3(e(h, 0.0), scale3(d.s, 2.0)), e(-h, 0.0)),
                1.0 / (h * h),
            );
            let fd_vv = scale3(
                add3(sub3(e(0.0, h), scale3(d.s, 2.0)), e(0.0, -h)),
                1.0 / (h * h),
            );
            let fd_uv = scale3(
                sub3(sub3(e(h, h), e(h, -h)), sub3(e(-h, h), e(-h, -h))),
                0.25 / (h * h),
            );
            let close = |a: [f64; 3], b: [f64; 3], tol: f64, what: &str| {
                let err = dot3(sub3(a, b), sub3(a, b)).sqrt();
                assert!(err < tol, "{s:?} {what} at {p:?}: {a:?} vs {b:?} ({err:e})");
            };
            close(d.su, fd_u, 1e-7, "s_u");
            close(d.sv, fd_v, 1e-7, "s_v");
            close(d.suv, fd_uv, 1e-4, "s_uv");
            close(d.suu, fd_uu, 1e-3, "s_uu");
            close(d.svv, fd_vv, 1e-3, "s_vv");
            assert_eq!(d.s, s.eval(p).unwrap());
        }
    }
}

#[test]
fn degenerate_iso_lines_refuse_a_normal() {
    let cone = surfaces()[2];
    assert_eq!(cone.normal(uv(0.3, 0.0)), Err(SurfaceError::Degenerate));
    let dom = cone.domain();
    assert_eq!(dom.degenerate, vec![Degeneracy::VIso(0.0)]);
    assert!(surfaces()[3].domain().degenerate.len() == 2);
}

#[test]
fn bezier_patch_weights_are_positive_and_endpoints_lie_on_the_surface() {
    let mut rng = Lcg(5);
    for s in surfaces() {
        for _ in 0..200 {
            let tri = random_tri(&s, &mut rng);
            let r = ParamRect::covering(&tri);
            let patches = s.bezier_patches(r).unwrap();
            for p in &patches {
                assert!(p.weights.iter().all(|&w| w > 0.0));
                assert_eq!(p.points.len(), (p.deg_u + 1) * (p.deg_v + 1));
            }
            // The rectangle's corner (u0, v0) is the first patch's first
            // control point.
            let c = s.eval(uv(r.u0, r.v0)).unwrap();
            let q = patches[0].points[0];
            let err = dot3(sub3(c, q), sub3(c, q)).sqrt();
            assert!(err < 1e-12, "{s:?}: corner {c:?} vs control point {q:?}");
        }
    }
}

/// Every normal sampled on a grid over `rect` lies in `normal_cone`.
fn assert_cone_holds_normals(s: &Surface, r: ParamRect) -> NormalCone {
    let cone = normal_cone(s, r).unwrap_or_else(|e| panic!("{s:?} {r:?}: {e:?}"));
    let n = 24;
    for i in 0..=n {
        for j in 0..=n {
            let p = uv(
                r.u0 + (r.u1 - r.u0) * i as f64 / n as f64,
                r.v0 + (r.v1 - r.v0) * j as f64 / n as f64,
            );
            let Ok(nrm) = s.normal(p) else { continue };
            assert!(
                cone.contains(nrm),
                "{s:?} {r:?}: normal {nrm:?} at {p:?} outside cone {cone:?} \
                 (angle {})",
                angle_between(cone.axis, nrm)
            );
        }
    }
    cone
}

#[test]
fn normal_cone_contains_every_sampled_normal() {
    let mut rng = Lcg(6);
    for s in surfaces() {
        for _ in 0..150 {
            let tri = random_tri(&s, &mut rng);
            assert_cone_holds_normals(&s, ParamRect::covering(&tri));
        }
    }
}

#[test]
fn normal_cone_of_a_plane_is_its_normal() {
    let s = surfaces()[0];
    let cone = assert_cone_holds_normals(
        &s,
        ParamRect {
            u0: -1.0,
            u1: 2.0,
            v0: -0.5,
            v1: 0.5,
        },
    );
    assert!(cone.half_angle < 1e-12, "{cone:?}");
}

#[test]
fn normal_cone_of_a_quarter_cylinder_is_tight() {
    // A π/2 arc of a cylinder: the true normals span ±π/4 about the middle
    // radial; Theorem 4.1's cone is exactly that wide for one patch.
    let s = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let cone = assert_cone_holds_normals(
        &s,
        ParamRect {
            u0: 0.0,
            u1: FRAC_PI_2,
            v0: 0.0,
            v1: 5.0,
        },
    );
    assert!((cone.half_angle - PI / 4.0).abs() < 1e-9, "{cone:?}");
}

#[test]
fn normal_cone_over_a_full_turn_holds_every_normal() {
    for s in surfaces().into_iter().skip(1) {
        let r = match s {
            Surface::Sphere { .. } => ParamRect {
                u0: 0.0,
                u1: 2.0 * PI,
                v0: -FRAC_PI_2,
                v1: FRAC_PI_2,
            },
            Surface::Torus { .. } => ParamRect {
                u0: 0.0,
                u1: 2.0 * PI,
                v0: -PI,
                v1: PI,
            },
            _ => ParamRect {
                u0: 0.0,
                u1: 2.0 * PI,
                v0: 0.5,
                v1: 2.0,
            },
        };
        assert_cone_holds_normals(&s, r);
    }
}

/// A surface whose only patch has parallel derivative cones: Theorem 4.1's
/// hypothesis never holds, so the cone is refused, never guessed.
struct Collinear;
impl ParamSurface for Collinear {
    fn validate(&self) -> Result<(), SurfaceError> {
        Ok(())
    }
    fn domain(&self) -> ParamDomain {
        let free = ParamAxis {
            lo: f64::NEG_INFINITY,
            hi: f64::INFINITY,
            period: None,
        };
        ParamDomain {
            u: free,
            v: free,
            degenerate: Vec::new(),
        }
    }
    fn eval(&self, p: Uv) -> Result<[f64; 3], SurfaceError> {
        Ok([p.x() + p.y(), 0.0, 0.0])
    }
    fn derivs(&self, p: Uv) -> Result<SurfaceDerivs, SurfaceError> {
        Ok(SurfaceDerivs {
            s: self.eval(p)?,
            su: [1.0, 0.0, 0.0],
            sv: [1.0, 0.0, 0.0],
            suu: [0.0; 3],
            suv: [0.0; 3],
            svv: [0.0; 3],
        })
    }
    fn bezier_patches(&self, r: ParamRect) -> Result<Vec<RationalBezierPatch>, SurfaceError> {
        let at = |u: f64, v: f64| self.eval(uv(u, v)).unwrap();
        Ok(vec![RationalBezierPatch {
            deg_u: 1,
            deg_v: 1,
            points: vec![
                at(r.u0, r.v0),
                at(r.u0, r.v1),
                at(r.u1, r.v0),
                at(r.u1, r.v1),
            ],
            weights: vec![1.0; 4],
            separable_weights: true,
        }])
    }
}

#[test]
fn normal_cone_refuses_when_the_hypothesis_never_holds() {
    let r = ParamRect {
        u0: 0.0,
        u1: 1.0,
        v0: 0.0,
        v1: 1.0,
    };
    assert_eq!(
        normal_cone(&Collinear, r),
        Err(SurfaceError::ConeHypothesis)
    );
    assert_eq!(
        Collinear.normal(uv(0.0, 0.0)),
        Err(SurfaceError::Degenerate)
    );
}

#[test]
fn cone_union_contains_both_cones() {
    let mut rng = Lcg(7);
    for _ in 0..500 {
        let rand_cone = |rng: &mut Lcg| {
            let a = [
                rng.range(-1.0, 1.0),
                rng.range(-1.0, 1.0),
                rng.range(-1.0, 1.0),
            ];
            NormalCone {
                axis: unit(a).unwrap_or([0.0, 0.0, 1.0]),
                half_angle: rng.range(0.0, 1.2),
            }
        };
        let (a, b) = (rand_cone(&mut rng), rand_cone(&mut rng));
        let u = a.union(&b);
        for c in [a, b] {
            // The cone's boundary along 32 directions.
            let (e1, e2) = crate::ortho_basis(Vector3::new(c.axis[0], c.axis[1], c.axis[2]));
            let (e1, e2) = (e1.as_array(), e2.as_array());
            for k in 0..32 {
                let t = 2.0 * PI * k as f64 / 32.0;
                let side = add3(scale3(e1, t.cos()), scale3(e2, t.sin()));
                let v = add3(
                    scale3(c.axis, c.half_angle.cos()),
                    scale3(side, c.half_angle.sin()),
                );
                assert!(
                    angle_between(u.axis, v) <= u.half_angle + 1e-12,
                    "{a:?} ∪ {b:?} = {u:?} misses {v:?}"
                );
            }
        }
    }
}

#[test]
fn closed_form_gauss_maps_and_the_net_cone_hold_the_same_normals() {
    // The closed-form Gauss maps (`gauss_map.rs`, N91/N92) are a recorded
    // fast path for Theorem 4.1's net cone (spec §5). Both must hold every
    // normal sampled over the same rectangle; a sample one of them misses is
    // a defect in that one.
    use crate::gauss_map::GaussMap;
    let mut rng = Lcg(8);
    for s in surfaces() {
        for _ in 0..150 {
            let tri = random_tri(&s, &mut rng);
            let r = ParamRect::covering(&tri);
            let closed = GaussMap::from_uv_rect(&s, [r.u0, r.u1], [r.v0, r.v1]);
            let net = normal_cone(&s, r).unwrap();
            let n = 12;
            for i in 0..=n {
                for j in 0..=n {
                    let p = uv(
                        r.u0 + (r.u1 - r.u0) * i as f64 / n as f64,
                        r.v0 + (r.v1 - r.v0) * j as f64 / n as f64,
                    );
                    let Ok(nrm) = s.normal(p) else { continue };
                    assert!(net.contains(nrm), "{s:?} {r:?}: net cone misses {nrm:?}");
                    assert!(
                        closed.overlaps(&GaussMap::from_normal(nrm)),
                        "{s:?} {r:?}: closed-form map misses {nrm:?} at {p:?}"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Increment 3: the analytic inverse (spec §5, ledger N109).
// ---------------------------------------------------------------------------

fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// `true` when `a` and `b` name the same parameter point modulo the
/// periods of `s`'s domain.
fn same_param(s: &Surface, a: Uv, b: Uv, tol: f64) -> bool {
    let dom = s.domain();
    let close = |x: f64, y: f64, period: Option<f64>| match period {
        Some(p) => {
            let d = (x - y).rem_euclid(p);
            d.min(p - d) <= tol
        }
        None => (x - y).abs() <= tol,
    };
    close(a.x(), b.x(), dom.u.period) && close(a.y(), b.y(), dom.v.period)
}

/// Away from a degenerate iso-line (where u is free), the inverse of an
/// evaluated point is the parameter point it came from, modulo the period
/// without a seed and on the seed's branch with one.
#[test]
fn inverse_undoes_eval() {
    let mut rng = Lcg(0x1_f5a3);
    for s in surfaces() {
        for _ in 0..2000 {
            let q = random_uv(&s, &mut rng);
            if matches!(s, Surface::Cone { .. }) && q.y() < 1e-3 {
                continue;
            }
            if matches!(s, Surface::Sphere { .. }) && FRAC_PI_2 - q.y().abs() < 1e-3 {
                continue;
            }
            let p = s.eval(q).unwrap();
            let bare = s.inverse(p, None).unwrap();
            assert!(same_param(&s, bare, q, 1e-9), "{s:?}: {q:?} -> {bare:?}");
            let dom = s.domain();
            if let Some(period) = dom.u.period {
                assert!(bare.x() >= dom.u.lo && bare.x() < dom.u.lo + period);
            }
            if let Some(period) = dom.v.period {
                assert!(bare.y() >= dom.v.lo && bare.y() < dom.v.lo + period);
            }
            // A seed near q, possibly whole periods away, picks its branch.
            let k = (rng.range(-2.0, 2.0)).round() * 2.0 * PI;
            let ku = if dom.u.period.is_some() { k } else { 0.0 };
            let kv = if dom.v.period.is_some() { -k } else { 0.0 };
            let seed = uv(q.x() + ku + rng.range(-1.0, 1.0), q.y() + kv);
            let seeded = s.inverse(p, Some(seed)).unwrap();
            assert!(
                (seeded.x() - (q.x() + ku)).abs() <= 1e-9,
                "{s:?}: seeded u {seeded:?} vs {q:?} + {ku}"
            );
            if dom.v.period.is_some() {
                assert!((seeded.y() - (q.y() + kv)).abs() <= 1e-9);
            }
            assert!(dist3(s.eval(seeded).unwrap(), p) <= 1e-12 * (1.0 + dist3(p, [0.0; 3])));
        }
    }
}

/// Off the surface, the inverse names the CLOSEST point: `p − s(uv)` is
/// normal to the surface there (orthogonal to `s_u` and `s_v`), and a point
/// moved along the normal returns to where it left.
#[test]
fn inverse_is_the_closest_point() {
    let mut rng = Lcg(0x2_f5a3);
    for s in surfaces() {
        for _ in 0..1000 {
            let q = random_uv(&s, &mut rng);
            let Ok(n) = s.normal(q) else { continue };
            if matches!(s, Surface::Cone { .. }) && q.y() < 0.2 {
                continue;
            }
            if matches!(s, Surface::Sphere { .. }) && FRAC_PI_2 - q.y().abs() < 1e-2 {
                continue;
            }
            // Within half the smallest curvature radius there (0.8 on every
            // fixture but the cone, whose azimuthal radius is ρ / cos α).
            let reach = match s {
                Surface::Cone { half_angle, .. } => (0.5 * q.y() * half_angle.tan()).min(0.3),
                _ => 0.3,
            };
            let h = rng.range(-reach, reach);
            let s0 = s.eval(q).unwrap();
            let p = [s0[0] + h * n[0], s0[1] + h * n[1], s0[2] + h * n[2]];
            let back = s.inverse(p, Some(q)).unwrap();
            assert!(
                (back.x() - q.x()).abs() <= 1e-9 && (back.y() - q.y()).abs() <= 1e-9,
                "{s:?}: {q:?} moved {h} -> {back:?}"
            );
            let d = s.derivs(back).unwrap();
            let w = [p[0] - d.s[0], p[1] - d.s[1], p[2] - d.s[2]];
            let dot = |a: [f64; 3]| (w[0] * a[0] + w[1] * a[1] + w[2] * a[2]).abs();
            let scale = 1e-9 * (1.0 + dist3(w, [0.0; 3]));
            assert!(dot(d.su) <= scale * dist3(d.su, [0.0; 3]).max(1.0));
            assert!(dot(d.sv) <= scale * dist3(d.sv, [0.0; 3]).max(1.0));
        }
    }
}

/// Where the closest point fixes no coordinate, the seed's is kept; a cone
/// point nearer the nappe behind the apex is outside the domain; a
/// non-finite point is refused.
#[test]
fn inverse_degenerate_points_and_refusals() {
    let ss = surfaces();
    let cone = ss[2];
    let sphere = ss[3];
    // The apex itself: v = 0, u from the seed.
    let apex = cone.eval(uv(0.0, 0.0)).unwrap();
    let at_apex = cone.inverse(apex, Some(uv(1.25, 0.5))).unwrap();
    assert!(at_apex.y().abs() <= 1e-12);
    // A point behind the apex, on the mirrored nappe.
    let Surface::Cone { half_angle, .. } = cone else {
        unreachable!()
    };
    let front = cone.eval(uv(0.7, 2.0)).unwrap();
    let behind = [
        2.0 * apex[0] - front[0],
        2.0 * apex[1] - front[1],
        2.0 * apex[2] - front[2],
    ];
    assert!(half_angle < std::f64::consts::FRAC_PI_4);
    assert_eq!(cone.inverse(behind, None), Err(SurfaceError::OutsideDomain));
    // The sphere's pole: u from the seed, latitude exact.
    let pole = sphere.eval(uv(0.0, FRAC_PI_2)).unwrap();
    let at_pole = sphere.inverse(pole, Some(uv(2.5, 1.0))).unwrap();
    assert!((at_pole.y() - FRAC_PI_2).abs() <= 1e-12);
    assert!(dist3(sphere.eval(at_pole).unwrap(), pole) <= 1e-12);
    for s in &ss {
        assert_eq!(
            s.inverse([f64::NAN, 0.0, 0.0], None),
            Err(SurfaceError::NonFiniteInput)
        );
    }
}
