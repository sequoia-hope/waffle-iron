//! Pins of the F5a increment-1 interface (spec
//! `specs/yang_f5a_surface_interface.md` §6 item 1).

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

#[test]
fn eval_is_stage4_dt_embedding_bit_for_bit() {
    let mut rng = Lcg(1);
    for s in surfaces() {
        for _ in 0..2000 {
            let p = random_uv(&s, &mut rng);
            let ours = s.eval(p).unwrap();
            let theirs = eval_uv(&s, p).unwrap().as_array();
            assert_eq!(
                ours.map(f64::to_bits),
                theirs.map(f64::to_bits),
                "{s:?} at {p:?}"
            );
        }
    }
}

#[test]
fn control_net_distance_is_d_of_t_bit_for_bit() {
    let mut rng = Lcg(2);
    for s in surfaces() {
        for _ in 0..2000 {
            let tri = random_tri(&s, &mut rng);
            let ours = control_net_distance(&s, tri).unwrap();
            let theirs = d_of_t(&s, tri).unwrap();
            assert_eq!(ours.to_bits(), theirs.to_bits(), "{s:?} at {tri:?}");
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
