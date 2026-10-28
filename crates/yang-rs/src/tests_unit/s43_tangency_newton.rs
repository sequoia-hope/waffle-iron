//! Unit pins for Yang et al. 2025 §4.3.1, §4.3.3 & Appendix C tangency
//! resolution by parametric Newton (`crates/yang-rs/src/stage4_tangency.rs`,
//! spec `specs/yang_43_tangency_newton.md`, roadmap F4 / N91).

use super::*;
use crate::boolean::cyl_cyl_tangent_points;
use crate::shell_contact::closest_on_triangle;
use crate::signed_distance_to_surface;
use crate::stage4_relocate::relocate_onto_implicit_pair;
use crate::stage4_small_loop::{
    discover_small_loop_candidate_areas, is_redundant_with_solved_curves,
};
use crate::stage4_tangency::{
    add3, cross3, discover_operand_pair_tangent_points_newton, dot3, norm3, scale3,
    solve_and_classify_seed, solve_parametric_pair_newton, solve_tangent_point_newton, sub3,
    LocalSurfaceChart, SeedOptimizationResult, TangencyKind, TANGENT_NORMAL_ANGLE_TOL,
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
                sol.normal_angle_rad > TANGENT_NORMAL_ANGLE_TOL,
                "expected transversal loop normal angle > 1e-6, got {:.3e}",
                sol.normal_angle_rad
            );
            assert!(signed_distance_to_surface(cyl_a, sol.point).unwrap().abs() < 1e-12);
            assert!(signed_distance_to_surface(cyl_b, sol.point).unwrap().abs() < 1e-12);
        }
        other => panic!("expected CurvePoint on C0116 small loop, got {other:?}"),
    }
}

/// Pin 5 (Fig. 10(c) `:471-473` R, `:491-499` L): candidate areas construct
/// `p_seed = barycenter(△t_A)` on the nearest triangle `T` of the left surface
/// and `q_seed` = exact closest point to `p_seed` on the right face's mesh,
/// and damped Newton with per-coordinate curvature/linear step caps converges
/// even when `△t_A` spans a `H = 40` tall cylinder ruling in either operand order.
#[test]
fn s43_fig10c_barycenter_seed_matches_paper_and_converges_on_tall_cylinders() {
    let backend = native_backend().expect("native backend");
    let tall_a = tube_brep([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.5, -19.0, 21.0);
    let short_b = tube_brep([0.0, 0.99, 1.0], [1.0, 0.0, 0.0], 0.5, -1.5, 1.5);

    for (lhs, rhs) in [(&tall_a, &short_b), (&short_b, &tall_a)] {
        let la = backend
            .labeled_arrangement(lhs.as_mesh(), rhs.as_mesh())
            .expect("arrangement");
        let areas = discover_small_loop_candidate_areas(
            lhs,
            lhs.as_mesh(),
            lhs.tri_face(),
            rhs,
            rhs.as_mesh(),
            rhs.tri_face(),
            &la,
        );
        let area = areas
            .iter()
            .find(|ar| ar.gauss_overlap)
            .expect("expected Gauss-map-admitted small-loop area");

        // `p_seed` must equal the exact barycenter of one of `area.tris_a`.
        let matched_bary = area.tris_a.iter().any(|&ta| {
            let tri = lhs.as_mesh().tris[ta];
            let v0 = lhs.as_mesh().verts[tri[0] as usize].as_array();
            let v1 = lhs.as_mesh().verts[tri[1] as usize].as_array();
            let v2 = lhs.as_mesh().verts[tri[2] as usize].as_array();
            let bary = scale3(add3(add3(v0, v1), v2), 1.0 / 3.0);
            norm3(sub3(area.p_seed.as_array(), bary)) < 1e-14
        });
        assert!(
            matched_bary,
            "p_seed {:?} is not the barycenter of any triangle in tris_a",
            area.p_seed
        );

        // `q_seed` must achieve the exact minimum distance from `p_seed` to
        // `face_b`'s mesh triangles.
        let pa = area.p_seed.as_array();
        let brute_min_d = rhs
            .tri_face()
            .iter()
            .enumerate()
            .filter(|&(_, &fb)| fb == area.face_b)
            .map(|(tb, _)| {
                let tri = rhs.as_mesh().tris[tb];
                let t0 = rhs.as_mesh().verts[tri[0] as usize].as_array();
                let t1 = rhs.as_mesh().verts[tri[1] as usize].as_array();
                let t2 = rhs.as_mesh().verts[tri[2] as usize].as_array();
                norm3(sub3(pa, closest_on_triangle(pa, t0, t1, t2)))
            })
            .fold(f64::INFINITY, f64::min);
        let q_dist = dist3(area.p_seed, area.q_seed);
        assert!(
            (q_dist - brute_min_d).abs() < 1e-13,
            "q_seed distance {q_dist} != brute-force closest distance {brute_min_d}"
        );

        let surf_a = lhs.faces()[area.face_a as usize].surface;
        let surf_b = rhs.faces()[area.face_b as usize].surface;
        match solve_and_classify_seed(area.p_seed, area.q_seed, surf_a, surf_b, Some(50.0)) {
            SeedOptimizationResult::CurvePoint(sol) => {
                assert!(sol.surface_gap <= 1e-12);
                assert!(signed_distance_to_surface(surf_a, sol.point).unwrap().abs() < 1e-12);
                assert!(signed_distance_to_surface(surf_b, sol.point).unwrap().abs() < 1e-12);
            }
            other => {
                panic!("expected CurvePoint from tall-cylinder Fig. 10(c) seed, got {other:?}")
            }
        }
    }

    // Tall-cylinder (`H = 40`) × sphere external kiss at `(0.5, 0, 1)`:
    // seeding `p_seed` at `z = 14.3` on the cylinder ruling (`~13.3` units
    // away from the kiss) converges to `< 1e-14`.
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 0.5,
    };
    let sph = Surface::Sphere {
        center: Point3::new(1.5, 0.0, 1.0),
        radius: 1.0,
    };
    let p_tall = Point3::new(0.495, 0.05, 14.3);
    let q_sph = Point3::new(0.51, 0.04, 1.12);
    let sol_kiss = solve_tangent_point_newton(p_tall, q_sph, cyl, sph, None)
        .expect("tall-cylinder × sphere kiss converges from Fig. 10(c) seed");
    assert_eq!(sol_kiss.kind, TangencyKind::IsolatedKiss);
    assert!(
        dist3(sol_kiss.point, Point3::new(0.5, 0.0, 1.0)) < 1e-14,
        "got {:?}",
        sol_kiss.point
    );
}

/// Pin 6 (§4.2.2 `:502-505` L curve-redundancy removal): removes a candidate
/// point `r0` that falls on an already-solved connected intersection curve,
/// while preserving genuinely distinct unwitnessed branches and disjoint loops.
#[test]
fn s43_curve_redundancy_removes_point_on_solved_curve_and_keeps_disjoint_loop() {
    let dp = cad_primitives::TAU_MODEL * 5.0;

    // (a) Sphere × Plane: single connected circle `x^2 + y^2 = 1` on `z = 0`.
    // A witness chord near `(1, 0, 0)` makes `(-1, 0, 0)` on the far side of
    // the same circle redundant.
    let sph = Surface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 1.0,
    };
    let pln = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let th = 0.1_f64;
    let solved_circle = vec![(
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(th.cos(), th.sin(), 0.0),
    )];
    assert!(is_redundant_with_solved_curves(
        Point3::new(-1.0, 0.0, 0.0),
        sph,
        pln,
        &solved_circle,
        &[],
        dp,
    ));

    // (b) Perpendicular Torus × Plane (`z = 0`): TWO disjoint concentric
    // circles (`r = 4` outer, `r = 2` inner). Witnessing only the outer circle
    // removes `(-4, 0, 0)` on the outer circle while keeping `(2, 0, 0)` on
    // the disjoint inner circle!
    let torus = Surface::Torus {
        center: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        major_radius: 3.0,
        minor_radius: 1.0,
    };
    let solved_outer = vec![(
        Point3::new(4.0, 0.0, 0.0),
        Point3::new(4.0 * th.cos(), 4.0 * th.sin(), 0.0),
    )];
    assert!(is_redundant_with_solved_curves(
        Point3::new(-4.0, 0.0, 0.0),
        torus,
        pln,
        &solved_outer,
        &[],
        dp,
    ));
    assert!(!is_redundant_with_solved_curves(
        Point3::new(2.0, 0.0, 0.0),
        torus,
        pln,
        &solved_outer,
        &[],
        dp,
    ));

    // (c) Equal-radius perpendicular cylinders (`R = 1`): TWO distinct
    // Steinmetz ellipses (`z = x` and `z = -x`). Witnessing only `z = x`
    // marks another point on `z = x` redundant while keeping `z = -x`!
    let cyl_z = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let cyl_x = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(1.0, 0.0, 0.0),
        radius: 1.0,
    };
    let a1 = 0.3_f64;
    let a2 = 0.5_f64;
    let solved_ellipse_pos = vec![(
        Point3::new(a1.sin(), a1.cos(), a1.sin()),
        Point3::new(a2.sin(), a2.cos(), a2.sin()),
    )];
    let r_same = Point3::new((-0.7_f64).sin(), (-0.7_f64).cos(), (-0.7_f64).sin());
    let r_other = Point3::new(0.7_f64.sin(), 0.7_f64.cos(), -0.7_f64.sin());
    assert!(is_redundant_with_solved_curves(
        r_same,
        cyl_z,
        cyl_x,
        &solved_ellipse_pos,
        &[],
        dp,
    ));
    assert!(!is_redundant_with_solved_curves(
        r_other,
        cyl_z,
        cyl_x,
        &solved_ellipse_pos,
        &[],
        dp,
    ));
}

/// Pin 7 (B1): `relocate_onto_implicit_pair` delegates ONLY when `det <= rank_eps`
/// within the scale-relative `0.25 · r_min` trust region around `p` (down to
/// microscopic radii with zero absolute `1e-3` floor), and returns `None` loudly
/// when two-plane Newton diverges/oscillates without hitting `det <= rank_eps`
/// (even if a tangent point sits within `0.25 · r_min`).
#[test]
fn s43_relocate_non_convergence_stays_loud_none_and_drift_is_scale_relative() {
    // (a) Separated sphere (`z_center = 1 + 1e-4`, `R = 1`) and plane `z = 0`:
    // off-axis seed `(0.15, 0.0, 0.0)` has non-collinear normals (`sin θ = 0.15`)
    // throughout 16 two-plane Newton steps while oscillating across the gap
    // without converging. Even if we compare against a tangent sphere–plane pair
    // where a seed starts outside `0.25 * R` or oscillates, non-convergence MUST
    // return `None`.
    let sph_sep = Surface::Sphere {
        center: Point3::new(0.0, 0.0, 1.0001),
        radius: 1.0,
    };
    let pln = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    assert!(relocate_onto_implicit_pair(Point3::new(0.15, 0.0, 0.0), sph_sep, pln).is_none());

    // Torus (`R = 0.60, r = 0.55`) and Cylinder (`axis = Y` through `(-0.55, 0, 0)`,
    // `r_c = 0.50`, so `r_min = 0.50` and `0.25 * r_min = 0.125`) kiss at the
    // single isolated 0D point `(-0.05, 0, 0)` (`solve_isolated_tangent_point_near`
    // converges there when seeded near `(-0.05, 0, 0)`). Starting on the opposite
    // inner wall `p = (0.05, 0.0, 0.03)` (`dist(p, (-0.05, 0, 0)) = 0.1044 < 0.125`),
    // two-plane Newton bounces across the inner hole without converging in 32
    // steps. With B1(a)'s post-loop retry removed, non-convergence stays loud `None`.
    let tor_kiss = Surface::Torus {
        center: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        major_radius: 0.60,
        minor_radius: 0.55,
    };
    let cyl_kiss = Surface::Cylinder {
        axis_point: Point3::new(-0.55, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 1.0, 0.0),
        radius: 0.50,
    };
    let near_kiss = relocate_onto_implicit_pair(Point3::new(-0.05, 0.0, 1e-8), tor_kiss, cyl_kiss)
        .expect("collinear-normal seed at (-0.05, 0, 1e-8) delegates to 0D tangency Newton");
    assert!(dist3(near_kiss, Point3::new(-0.05, 0.0, 0.0)) < 1e-14);
    assert!(
        relocate_onto_implicit_pair(Point3::new(0.05, 0.0, 0.03), tor_kiss, cyl_kiss).is_none(),
        "post-loop non-convergence must stay loud None even inside 0.25*r_min"
    );

    // (b) Scale-relative drift bound (`0.25 * r_min`, no `.max(1e-3)` floor):
    // on a `R = 1e-4` sphere kissing `z = 0` at `(0, 0, 0)`, a collinear-normal
    // seed at `(0, 0, -5e-4)` (`5 * R > 0.25 * R`, though `< 1e-3`) MUST be
    // rejected (`None`), whereas `(5e-11, 0, 0)` (`<< 0.25 * R` and `sin θ < 1e-6`)
    // resolves cleanly to `(0, 0, 0)`.
    let micro_r = 1e-4;
    let sph_micro = Surface::Sphere {
        center: Point3::new(0.0, 0.0, micro_r),
        radius: micro_r,
    };
    assert!(
        relocate_onto_implicit_pair(Point3::new(0.0, 0.0, -5.0 * micro_r), sph_micro, pln)
            .is_none(),
        "seed 5*R away on R=1e-4 sphere must not pass via an absolute 1e-3 floor"
    );
    let resolved_micro = relocate_onto_implicit_pair(Point3::new(5e-11, 0.0, 0.0), sph_micro, pln)
        .expect("near-kiss collinear seed within 0.25*R resolves");
    assert!(dist3(resolved_micro, Point3::new(0.0, 0.0, 0.0)) < 1e-15);
}

/// Pin 8 (S2 & S4): `solve_and_classify_seed` runs surface-pair optimization
/// FIRST and compares normals SECOND (§4.3 `:536-569`), returning
/// `TangencySolveFailed` (never `NoIntersection`) when normals are collinear on
/// a 1D tangent locus or near-coplanar crossing, `CoincidentSurfaces` on 2D
/// identical surfaces, and handling `(Plane, Plane)` without surface-type pre-skips.
#[test]
fn s43_classify_seed_pair_first_loud_on_collinear_failure_and_plane_pairs() {
    // 1D parallel tangent cylinders (`x^2 + y^2 = 1` and `(x - 2)^2 + y^2 = 1`
    // kissing along the generator `(1, 0, z)`): pair Newton converges to `(1, 0, z)`
    // with collinear normals, 2D coincidence is false, and 0D tangency Newton
    // sees `TangencyKind::TangentCurve` (`rank == 3`), yielding `TangencySolveFailed`.
    let cyl_a = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let cyl_b = Surface::Cylinder {
        axis_point: Point3::new(2.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let res_gen = solve_and_classify_seed(
        Point3::new(0.99, 0.14, 0.5),
        Point3::new(1.01, 0.14, 0.5),
        cyl_a,
        cyl_b,
        None,
    );
    assert!(
        matches!(res_gen, SeedOptimizationResult::TangencySolveFailed),
        "1D parallel cylinder generator must return TangencySolveFailed, got {res_gen:?}"
    );

    // Two planes crossing at `5e-7` rad (`<= 1e-6` rad): pair Newton converges
    // onto the intersection line with collinear-grade normals, 2D coincidence
    // is false, and 0D tangency Newton returns `None` -> `TangencySolveFailed`.
    let pln_z = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let tilt = 5e-7_f64;
    let pln_tilt = Surface::Plane {
        normal: Vector3::new(-tilt.sin(), 0.0, tilt.cos()),
        d: 0.0,
    };
    assert!(solve_tangent_point_newton(
        Point3::new(0.2, 0.3, 0.0),
        Point3::new(0.2, 0.3, 1e-7),
        pln_z,
        pln_tilt,
        None,
    )
    .is_none());
    let res_tilt = solve_and_classify_seed(
        Point3::new(0.2, 0.3, 0.0),
        Point3::new(0.2, 0.3, 1e-7),
        pln_z,
        pln_tilt,
        None,
    );
    assert!(
        matches!(res_tilt, SeedOptimizationResult::TangencySolveFailed),
        "sub-1e-6 rad plane wedge must return TangencySolveFailed, got {res_tilt:?}"
    );

    // Coplanar planes -> `CoincidentSurfaces` (and `solve_tangent_point_newton` -> `None` via `rank == 2`).
    let pln_anti = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, -1.0),
        d: 0.0,
    };
    assert!(solve_tangent_point_newton(
        Point3::new(0.1, -0.2, 0.0),
        Point3::new(0.1, -0.2, 0.0),
        pln_z,
        pln_anti,
        None,
    )
    .is_none());
    assert!(matches!(
        solve_and_classify_seed(
            Point3::new(0.1, -0.2, 0.0),
            Point3::new(0.1, -0.2, 0.0),
            pln_z,
            pln_anti,
            None,
        ),
        SeedOptimizationResult::CoincidentSurfaces
    ));

    // Separated parallel planes -> `NoIntersection`.
    let pln_sep = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -1e-4,
    };
    assert!(matches!(
        solve_and_classify_seed(
            Point3::new(0.1, -0.2, 0.0),
            Point3::new(0.1, -0.2, 1e-4),
            pln_z,
            pln_sep,
            None,
        ),
        SeedOptimizationResult::NoIntersection
    ));
}

/// Pin 9 (S6): deterministic sweep property test across cylinder pairs (equal-radius
/// Steinmetz crossings at 6 angles, unequal-radius internal kisses `±(R_A - R_B)`,
/// unequal-radius external kisses `±(R_A + R_B)`, rotated 3D frames, off-origin
/// translations, and separated/unequal crossing non-tangent pairs) verifying
/// that mesh-seeded `discover_operand_pair_tangent_points_newton` discovers
/// EVERY closed-form tangent point reported by the `cyl_cyl_tangent_points` oracle.
#[test]
fn s43_newton_discovers_every_closed_form_cyl_cyl_tangent_point_across_sweep() {
    let frames: [([f64; 3], [f64; 3]); 3] = [
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        (normalize3([0.3, -0.5, 1.0]), normalize3([1.0, 0.6, 0.0])),
        (normalize3([-0.7, 0.4, 0.5]), normalize3([0.4, 0.7, 0.0])),
    ];
    let origins: [[f64; 3]; 2] = [[0.0, 0.0, 0.0], [3.2, -1.7, 4.5]];
    let angles_deg: [f64; 5] = [20.0, 35.0, 55.0, 75.0, 90.0];

    let mut total_cases = 0usize;
    let mut total_tangent_pts = 0usize;

    for &(u_raw, perp_hint) in &frames {
        let u = normalize3(u_raw);
        let e1 = normalize3(sub3(perp_hint, scale3(u, dot3(perp_hint, u))));
        let e2 = cross3(u, e1);

        for &orig in &origins {
            for &deg in &angles_deg {
                let beta = deg.to_radians();
                // `v` lies in `span(u, e1)` at angle `beta` from `u`, so the
                // common perpendicular is `±e2`.
                let v = add3(scale3(u, beta.cos()), scale3(e1, beta.sin()));
                let span = 2.6;

                let configs: [(f64, f64, f64); 6] = [
                    // 1. Equal-radius Steinmetz crossing (2 saddle tangency points).
                    (0.35, 0.35, 0.0),
                    // 2. Internal kiss B inside A (`delta = R_A - R_B`, 1 saddle point).
                    (0.45, 0.25, 0.45 - 0.25),
                    // 3. Internal kiss opposite side (`delta = -(R_A - R_B)`, 1 saddle point).
                    (0.45, 0.25, -(0.45 - 0.25)),
                    // 4. External kiss (`delta = R_A + R_B`, 1 kiss point).
                    (0.30, 0.20, 0.30 + 0.20),
                    // 5. Unequal radii with intersecting axes (`delta = 0`, 0 tangent points).
                    (0.45, 0.25, 0.0),
                    // 6. Separated near-miss (`delta = R_A + R_B + 5e-5`, 0 tangent points).
                    (0.30, 0.20, 0.30 + 0.20 + 5e-5),
                ];

                for &(ra, rb, delta) in &configs {
                    total_cases += 1;
                    // Shift `ap_b` along `u`, `v`, and `delta * e2` so axial feet
                    // stay well inside `[-span, +span]` on both tubes.
                    let ap_a = orig;
                    let ap_b = add3(
                        add3(orig, scale3(u, 0.15)),
                        add3(scale3(v, -0.10), scale3(e2, delta)),
                    );

                    let oracle = cyl_cyl_tangent_points(
                        (
                            Point3::new(ap_a[0], ap_a[1], ap_a[2]),
                            Vector3::new(u[0], u[1], u[2]),
                            ra,
                        ),
                        (
                            Point3::new(ap_b[0], ap_b[1], ap_b[2]),
                            Vector3::new(v[0], v[1], v[2]),
                            rb,
                        ),
                    )
                    .expect("non-parallel axes in sweep");

                    let brep_a = tube_brep(ap_a, u, ra, -span, span);
                    let brep_b = tube_brep(ap_b, v, rb, -span, span);
                    let discovered_map =
                        discover_operand_pair_tangent_points_newton(&brep_a, &brep_b);
                    let discovered: Vec<Point3> = discovered_map
                        .get(&(0, 0))
                        .map(|v| v.iter().map(|s| s.point).collect())
                        .unwrap_or_default();

                    assert_eq!(
                        discovered.len(),
                        oracle.len(),
                        "count mismatch at deg={deg}, ra={ra}, rb={rb}, delta={delta}: \
                         discovered={discovered:?} vs oracle={oracle:?}"
                    );
                    for &exp in &oracle {
                        total_tangent_pts += 1;
                        let best_err = discovered
                            .iter()
                            .map(|&got| dist3(got, exp))
                            .fold(f64::INFINITY, f64::min);
                        assert!(
                            best_err < 1e-13,
                            "missed oracle tangent point {exp:?} (min err {best_err:e}) \
                             at deg={deg}, ra={ra}, rb={rb}, delta={delta}"
                        );
                    }
                }
            }
        }
    }
    assert_eq!(total_cases, 180);
    assert_eq!(total_tangent_pts, 150);
}
