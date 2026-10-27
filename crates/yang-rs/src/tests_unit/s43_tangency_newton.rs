//! Unit pins for Yang et al. 2025 §4.3.1, §4.3.3 & Appendix C tangency
//! resolution by parametric Newton (`crates/yang-rs/src/stage4_tangency.rs`,
//! spec `specs/yang_43_tangency_newton.md`, roadmap F4 / N91).

use super::*;
use crate::signed_distance_to_surface;
use crate::stage4_relocate::relocate_onto_implicit_pair;
use crate::stage4_tangency::{
    discover_operand_pair_tangent_points_newton, solve_and_classify_seed,
    solve_parametric_pair_newton, solve_tangent_point_newton, LocalSurfaceChart,
    SeedOptimizationResult, TangencyKind,
};

fn dist3(a: Point3, b: Point3) -> f64 {
    ((a.x() - b.x()).powi(2) + (a.y() - b.y()).powi(2) + (a.z() - b.z()).powi(2)).sqrt()
}

fn tube_brep(axis_point: [f64; 3], axis_dir: [f64; 3], radius: f64, lo: f64, hi: f64) -> BRep {
    let u = normalize3(axis_dir);
    let p_lo = [
        axis_point[0] + lo * u[0],
        axis_point[1] + lo * u[1],
        axis_point[2] + lo * u[2],
    ];
    let p_hi = [
        axis_point[0] + hi * u[0],
        axis_point[1] + hi * u[1],
        axis_point[2] + hi * u[2],
    ];
    let (e1v, _) = ortho_basis(Vector3::new(u[0], u[1], u[2]));
    let e1 = e1v.as_array();
    let v0 = Point3::new(
        p_lo[0] + radius * e1[0],
        p_lo[1] + radius * e1[1],
        p_lo[2] + radius * e1[2],
    );
    let v1 = Point3::new(
        p_hi[0] + radius * e1[0],
        p_hi[1] + radius * e1[1],
        p_hi[2] + radius * e1[2],
    );
    let neg_u = [-u[0], -u[1], -u[2]];
    let d_lo = -(neg_u[0] * p_lo[0] + neg_u[1] * p_lo[1] + neg_u[2] * p_lo[2]);
    let d_hi = -(u[0] * p_hi[0] + u[1] * p_hi[1] + u[2] * p_hi[2]);

    let verts = vec![BRepVertex { point: v0 }, BRepVertex { point: v1 }];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: Point3::new(p_lo[0], p_lo[1], p_lo[2]),
                normal: Vector3::new(neg_u[0], neg_u[1], neg_u[2]),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: Point3::new(p_hi[0], p_hi[1], p_hi[2]),
                normal: Vector3::new(u[0], u[1], u[2]),
                radius,
            },
        },
        BRepEdge {
            start: 0,
            end: 1,
            curve: Curve::LineSegment,
        },
    ];
    let faces = vec![
        BRepFace {
            surface: Surface::Cylinder {
                axis_point: Point3::new(axis_point[0], axis_point[1], axis_point[2]),
                axis_dir: Vector3::new(axis_dir[0], axis_dir[1], axis_dir[2]),
                radius,
            },
            outer_loop: vec![0, 2, 1, 2],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(neg_u[0], neg_u[1], neg_u[2]),
                d: d_lo,
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(u[0], u[1], u[2]),
                d: d_hi,
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("valid canonical cylinder B-Rep")
}

/// Central-difference check of `(su, sv, nu, nv)` and `project(eval(u, v))`
/// across all five analytic `Surface` variants, including exact north/south
/// sphere poles where global latitude/longitude charts are singular.
#[test]
fn s43_local_surface_charts_match_central_differences_on_all_five_surfaces() {
    let surfaces = [
        (
            "plane",
            Surface::Plane {
                normal: Vector3::new(1.0, -2.0, 2.0),
                d: -3.5,
            },
            Point3::new(0.5, -1.2, 0.4),
        ),
        (
            "sphere-south-pole",
            Surface::Sphere {
                center: Point3::new(0.0, 0.0, 1.0),
                radius: 1.0,
            },
            Point3::new(0.0, 0.0, 0.0),
        ),
        (
            "sphere-north-pole",
            Surface::Sphere {
                center: Point3::new(1.0, -2.0, 3.0),
                radius: 2.5,
            },
            Point3::new(1.0, -2.0, 5.5),
        ),
        (
            "cylinder",
            Surface::Cylinder {
                axis_point: Point3::new(0.5, -0.3, 1.2),
                axis_dir: Vector3::new(0.3, 0.4, 0.866_025_403_784_438_6),
                radius: 0.75,
            },
            Point3::new(1.2, 0.1, 1.5),
        ),
        (
            "cone",
            Surface::Cone {
                apex: Point3::new(-1.0, 0.5, -2.0),
                axis_dir: Vector3::new(0.0, 0.6, 0.8),
                half_angle: 0.45,
            },
            Point3::new(-0.3, 1.4, -0.6),
        ),
        (
            "torus-inner",
            Surface::Torus {
                center: Point3::new(0.0, 0.0, 0.0),
                axis_dir: Vector3::new(0.0, 0.0, 1.0),
                major_radius: 3.0,
                minor_radius: 1.0,
            },
            Point3::new(2.0, 0.0, 0.0),
        ),
    ];

    let h = 1e-7;
    for (name, surf, anchor) in surfaces {
        let chart = LocalSurfaceChart::new(surf, anchor).expect(name);
        for (u, v) in [(0.0, 0.0), (0.07, -0.05)] {
            let e = chart.eval(u, v).expect(name);
            let p = Point3::new(e.point[0], e.point[1], e.point[2]);
            let sd = signed_distance_to_surface(surf, p).unwrap().abs();
            assert!(sd < 1e-13, "{name}: chart point off surface by {sd:e}");

            let (pu, pv) = chart.project(e.point).expect(name);
            assert!(
                (pu - u).abs() < 1e-12 && (pv - v).abs() < 1e-12,
                "{name}: project(eval({u},{v})) = ({pu},{pv})"
            );

            let eu_p = chart.eval(u + h, v).unwrap();
            let eu_m = chart.eval(u - h, v).unwrap();
            let ev_p = chart.eval(u, v + h).unwrap();
            let ev_m = chart.eval(u, v - h).unwrap();
            for k in 0..3 {
                let fd_su = (eu_p.point[k] - eu_m.point[k]) / (2.0 * h);
                let fd_sv = (ev_p.point[k] - ev_m.point[k]) / (2.0 * h);
                let fd_nu = (eu_p.normal[k] - eu_m.normal[k]) / (2.0 * h);
                let fd_nv = (ev_p.normal[k] - ev_m.normal[k]) / (2.0 * h);
                assert!(
                    (fd_su - e.su[k]).abs() < 1e-7,
                    "{name}: su[{k}]={:.6e} vs fd={fd_su:.6e}",
                    e.su[k]
                );
                assert!(
                    (fd_sv - e.sv[k]).abs() < 1e-7,
                    "{name}: sv[{k}]={:.6e} vs fd={fd_sv:.6e}",
                    e.sv[k]
                );
                assert!(
                    (fd_nu - e.nu[k]).abs() < 1e-7,
                    "{name}: nu[{k}]={:.6e} vs fd={fd_nu:.6e}",
                    e.nu[k]
                );
                assert!(
                    (fd_nv - e.nv[k]).abs() < 1e-7,
                    "{name}: nv[{k}]={:.6e} vs fd={fd_nv:.6e}",
                    e.nv[k]
                );
            }
        }
    }
}

/// Pin 1 (tangent cylinder pair): C0058's equal-radius crossing cylinders
/// (`R = 0.4`, axes crossing at `(0, 0, 1)` at `30°`) and unequal-radius
/// offset crossing cylinders (`R_A = 0.4, R_B = 0.25`, offset `R_A - R_B` and
/// `R_A + R_B`), plus mesh-seeded Stage-1 discovery and near-miss rejection.
#[test]
fn s43_newton_resolves_tangent_cylinder_pairs_and_discovers_from_meshes() {
    let beta = std::f64::consts::FRAC_PI_6;
    let cyl_a = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 0.4,
    };
    let cyl_b = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 1.0),
        axis_dir: Vector3::new(beta.sin(), 0.0, beta.cos()),
        radius: 0.4,
    };

    // Seed from C0058's measured off-tangency mesh vertex v33 (~0.1334 away!).
    let v33 = Point3::new(
        0.121_914_593_969_821_09,
        -0.360_387_547_160_967_66,
        1.037_067_479_103_390_5,
    );
    let sol_neg = solve_tangent_point_newton(v33, v33, cyl_a, cyl_b, None)
        .expect("converges to -y Steinmetz tangency");
    assert_eq!(sol_neg.kind, TangencyKind::IsolatedSaddle);
    assert!(
        dist3(sol_neg.point, Point3::new(0.0, -0.4, 1.0)) < 1e-14,
        "got {:?}",
        sol_neg.point
    );
    assert!(sol_neg.normal_angle_rad < 1e-13);

    // Stage-1 mesh-seeded discovery finds BOTH `(0, -0.4, 1)` and `(0, +0.4, 1)`
    // with zero cylinder×cylinder closed form.
    let brep_a = tube_brep([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.4, -0.5, 2.5);
    let brep_b = tube_brep(
        [0.0, 0.0, 1.0],
        [beta.sin(), 0.0, beta.cos()],
        0.4,
        -1.5,
        1.5,
    );
    let discovered = discover_operand_pair_tangent_points_newton(&brep_a, &brep_b);
    let pts = discovered
        .get(&(0, 0))
        .expect("lateral×lateral pair discovers tangency points");
    assert_eq!(pts.len(), 2, "expected 2 Steinmetz points, got {pts:?}");
    for expected in [Point3::new(0.0, -0.4, 1.0), Point3::new(0.0, 0.4, 1.0)] {
        assert!(
            pts.iter().any(|p| dist3(p.point, expected) < 1e-14),
            "missing expected Steinmetz tangency {expected:?} in {pts:?}"
        );
    }

    // Unequal-radius perpendicular cylinders with axis offset `R_A - R_B`
    // (Viviani figure-eight saddle at `(0, 0.4, 1)`): both direct Newton and
    // `relocate_onto_implicit_pair` resolve the pinch!
    let cyl_inner = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.4 - 0.25, 1.0),
        axis_dir: Vector3::new(1.0, 0.0, 0.0),
        radius: 0.25,
    };
    let seed_viv = Point3::new(0.03, 0.398, 1.02);
    let sol_viv = solve_tangent_point_newton(seed_viv, seed_viv, cyl_a, cyl_inner, None)
        .expect("converges to Viviani pinch");
    assert_eq!(sol_viv.kind, TangencyKind::IsolatedSaddle);
    assert!(dist3(sol_viv.point, Point3::new(0.0, 0.4, 1.0)) < 1e-14);

    let reloc_viv =
        relocate_onto_implicit_pair(Point3::new(1e-7, 0.4, 1.0 - 1e-7), cyl_a, cyl_inner)
            .expect("relocate_onto_implicit_pair resolves isolated saddle tangency");
    assert!(dist3(reloc_viv, Point3::new(0.0, 0.4, 1.0)) < 1e-14);

    // External kiss (`offset = R_A + R_B`) is classified as `IsolatedKiss`.
    let cyl_ext = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.4 + 0.25, 1.0),
        axis_dir: Vector3::new(1.0, 0.0, 0.0),
        radius: 0.25,
    };
    let sol_ext = solve_tangent_point_newton(seed_viv, seed_viv, cyl_a, cyl_ext, None)
        .expect("converges to external cylinder kiss");
    assert_eq!(sol_ext.kind, TangencyKind::IsolatedKiss);
    assert!(dist3(sol_ext.point, Point3::new(0.0, 0.4, 1.0)) < 1e-14);

    // Near-tangency separated by `1e-6` (and `1e-8`) beyond `TAU_WORK` is
    // refused — never fused (R0053 invariant).
    for gap in [1e-6, 1e-8] {
        let cyl_near = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.4 + 0.25 + gap, 1.0),
            axis_dir: Vector3::new(1.0, 0.0, 0.0),
            radius: 0.25,
        };
        assert!(
            solve_tangent_point_newton(seed_viv, seed_viv, cyl_a, cyl_near, None).is_none(),
            "gap={gap:e} must not fuse into a tangency"
        );
    }
}

/// Pin 2 (sphere–plane kiss): resolves both south-pole axis-aligned kisses
/// (where global spherical coordinates have `∂s/∂u = 0`) and oblique-frame
/// kisses to `< 1e-14`, while rejecting separated near-misses.
#[test]
fn s43_newton_resolves_sphere_plane_kiss_including_poles() {
    let sphere = Surface::Sphere {
        center: Point3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let plane = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let p_seed = Point3::new(0.11, -0.08, 0.01);
    let q_seed = Point3::new(0.09, -0.07, 0.0);

    let sol = solve_tangent_point_newton(p_seed, q_seed, sphere, plane, None)
        .expect("sphere-plane south-pole kiss converges");
    assert_eq!(sol.kind, TangencyKind::IsolatedKiss);
    assert!(
        dist3(sol.point, Point3::new(0.0, 0.0, 0.0)) < 1e-14,
        "got {:?}",
        sol.point
    );
    assert!(sol.normal_angle_rad < 1e-13);

    // §4.3.3 seed classifier agrees (`Tangent`, not `CurvePoint`) and Appendix C
    // parametric pair Newton converges on the multiplicity-2 kiss too.
    match solve_and_classify_seed(p_seed, q_seed, sphere, plane, None) {
        SeedOptimizationResult::Tangent(t) => {
            assert!(dist3(t.point, Point3::new(0.0, 0.0, 0.0)) < 1e-14);
        }
        other => panic!("expected Tangent, got {other:?}"),
    }
    let app_c = solve_parametric_pair_newton(p_seed, q_seed, sphere, plane)
        .expect("Appendix C parametric pair Newton converges at kiss");
    assert!(app_c.surface_gap <= 1e-12);
    assert!(dist3(app_c.point, Point3::new(0.0, 0.0, 0.0)) < 1e-5);

    // `relocate_onto_implicit_pair` resolves near-kiss seeds instead of refusing.
    let reloc = relocate_onto_implicit_pair(Point3::new(0.02, -0.015, 2e-4), sphere, plane)
        .expect("relocate_onto_implicit_pair resolves sphere-plane kiss");
    assert!(dist3(reloc, Point3::new(0.0, 0.0, 0.0)) < 1e-14);

    // Oblique sphere–plane kiss at non-unit scale.
    let n_raw = normalize3([2.0, -1.0, 2.0]);
    let center = [13.0, -27.0, 41.0];
    let radius = 6.5;
    let exact_kiss = Point3::new(
        center[0] - radius * n_raw[0],
        center[1] - radius * n_raw[1],
        center[2] - radius * n_raw[2],
    );
    let d_oblique =
        -(n_raw[0] * exact_kiss.x() + n_raw[1] * exact_kiss.y() + n_raw[2] * exact_kiss.z());
    let sph_ob = Surface::Sphere {
        center: Point3::new(center[0], center[1], center[2]),
        radius,
    };
    let pln_ob = Surface::Plane {
        normal: Vector3::new(n_raw[0], n_raw[1], n_raw[2]),
        d: d_oblique,
    };
    let ob_seed = Point3::new(
        exact_kiss.x() + 0.35,
        exact_kiss.y() - 0.25,
        exact_kiss.z() + 0.2,
    );
    let sol_ob = solve_tangent_point_newton(ob_seed, ob_seed, sph_ob, pln_ob, None)
        .expect("oblique sphere-plane kiss converges");
    assert!(
        dist3(sol_ob.point, exact_kiss) < 1e-12,
        "got {:?}",
        sol_ob.point
    );

    // Separated sphere–plane (`z_center = 1 + 1e-8`) has no intersection.
    let separated_sphere = Surface::Sphere {
        center: Point3::new(0.0, 0.0, 1.0 + 1e-8),
        radius: 1.0,
    };
    assert!(matches!(
        solve_and_classify_seed(p_seed, q_seed, separated_sphere, plane, None),
        SeedOptimizationResult::NoIntersection
    ));
}

/// Pin 3 (torus–plane graze): resolves all three torus–plane grazing classes —
/// outer-equator elliptic kiss (`x = R + r`), inner-throat Booth-lemniscate
/// saddle graze (`x = R - r`), and horizontal bottom-circle graze (`z = -r`).
#[test]
fn s43_newton_resolves_torus_plane_outer_inner_and_bottom_grazes() {
    let torus = Surface::Torus {
        center: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        major_radius: 3.0,
        minor_radius: 1.0,
    };

    // (a) Outer-equator elliptic kiss at `(4, 0, 0)`.
    let outer_plane = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -4.0,
    };
    let seed_outer = Point3::new(3.96, 0.18, -0.14);
    let sol_outer = solve_tangent_point_newton(seed_outer, seed_outer, torus, outer_plane, None)
        .expect("torus outer-equator plane graze converges");
    assert_eq!(sol_outer.kind, TangencyKind::IsolatedKiss);
    assert!(dist3(sol_outer.point, Point3::new(4.0, 0.0, 0.0)) < 1e-14);
    let reloc_outer =
        relocate_onto_implicit_pair(Point3::new(4.0, 1e-7, -1e-7), torus, outer_plane)
            .expect("relocate_onto_implicit_pair resolves torus outer kiss");
    assert!(dist3(reloc_outer, Point3::new(4.0, 0.0, 0.0)) < 1e-14);

    // (b) Inner-throat hyperbolic saddle graze at `(2, 0, 0)` (figure-eight
    // Booth lemniscate self-crossing!).
    let inner_plane = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -2.0,
    };
    let seed_inner = Point3::new(2.04, -0.15, 0.12);
    let sol_inner = solve_tangent_point_newton(seed_inner, seed_inner, torus, inner_plane, None)
        .expect("torus inner-throat saddle graze converges");
    assert_eq!(sol_inner.kind, TangencyKind::IsolatedSaddle);
    assert!(dist3(sol_inner.point, Point3::new(2.0, 0.0, 0.0)) < 1e-14);
    let reloc_inner =
        relocate_onto_implicit_pair(Point3::new(2.0, 1e-7, -1e-7), torus, inner_plane)
            .expect("relocate_onto_implicit_pair resolves torus inner lemniscate pinch");
    assert!(dist3(reloc_inner, Point3::new(2.0, 0.0, 0.0)) < 1e-14);

    // (c) Horizontal bottom-circle graze on `z = -1`: 1D tangent locus
    // (`TangencyKind::TangentCurve`).
    let bottom_plane = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 1.0,
    };
    let phi: f64 = 0.7;
    let seed_bot = Point3::new(3.05 * phi.cos(), 3.05 * phi.sin(), -0.96);
    let sol_bot = solve_tangent_point_newton(seed_bot, seed_bot, torus, bottom_plane, None)
        .expect("torus bottom-circle plane graze converges");
    assert_eq!(sol_bot.kind, TangencyKind::TangentCurve);
    let r_xy = sol_bot.point.x().hypot(sol_bot.point.y());
    assert!((r_xy - 3.0).abs() < 1e-13, "r_xy={r_xy}");
    assert!(
        (sol_bot.point.z() + 1.0).abs() < 1e-14,
        "z={}",
        sol_bot.point.z()
    );
    // 1D tangent circle is NOT an isolated 0D point, so 1D curve relocation
    // (`relocate_onto_implicit_pair`) continues to return `None`.
    assert!(relocate_onto_implicit_pair(sol_bot.point, torus, bottom_plane).is_none());
}

/// Pin 4 (§4.3.3 `10^{-6}` rad tangency vs small-loop classifier): on C0116's
/// `0.01`-deep perpendicular cylinder graze, `solve_and_classify_seed` lands on
/// the closed loop and classifies it as `CurvePoint` (`∠(n_A, ±n_B) > 10^{-6}`),
/// NOT `Tangent`.
#[test]
fn s43_seed_classifier_distinguishes_small_loop_from_tangency_on_c0116() {
    let cyl_a = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 0.5,
    };
    let cyl_b = Surface::Cylinder {
        axis_point: Point3::new(-1.0, 0.99, 1.0),
        axis_dir: Vector3::new(1.0, 0.0, 0.0),
        radius: 0.5,
    };
    let p_seed = Point3::new(0.04, 0.495, 1.02);
    let q_seed = Point3::new(0.04, 0.495, 1.02);
    match solve_and_classify_seed(p_seed, q_seed, cyl_a, cyl_b, Some(0.25)) {
        SeedOptimizationResult::CurvePoint(sol) => {
            assert!(sol.surface_gap <= 1e-12);
            assert!(
                sol.normal_angle_rad > cad_primitives::MIN_FEATURE_SIZE,
                "expected transversal loop normal angle > 1e-6, got {:.3e}",
                sol.normal_angle_rad
            );
            assert!(signed_distance_to_surface(cyl_a, sol.point).unwrap().abs() < 1e-12);
            assert!(signed_distance_to_surface(cyl_b, sol.point).unwrap().abs() < 1e-12);
        }
        other => panic!("expected CurvePoint on C0116 small loop, got {other:?}"),
    }
}
