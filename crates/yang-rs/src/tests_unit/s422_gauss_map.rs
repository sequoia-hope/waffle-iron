//! Unit and end-to-end pins for Yang et al. 2025 §4.2.2 closed-form Gauss-map
//! estimation + overlap admission (`crates/yang-rs/src/gauss_map.rs`, ledger
//! **N92**, compliance matrix rows **4.2-12**, **4.2-13**, **5.6-02**) and
//! §4.3.3 / §4.3.4 / §4.4.1 small-loop tracing, parameter-midpoint refinement,
//! and Fig. 11(c) local chart CDT mesh updating
//! (`crates/yang-rs/src/stage4_small_loop.rs`, ledger **N91**, spec
//! `specs/yang_43_tangency_newton.md`).

use std::f64::consts::{FRAC_PI_2, FRAC_PI_3, FRAC_PI_4, PI, TAU};

use cad_primitives::{BoolOp, Point2};

use crate::boolean::boolean_once;
use crate::boolean::failures::{self, FailureTrigger};
use crate::gauss_map::{gauss_maps_overlap, GaussMap};
use crate::geom::surface_normal_at;
use crate::signed_distance_to_surface;
use crate::stage4_boundary_curve::CreaseIndex;
use crate::stage4_correct::paper_chain_sample_redundant;
use crate::stage4_dt::eval_uv;
use crate::stage4_small_loop::{
    discover_small_loop_candidate_areas, register_spliced_loop_edges_in_arrangement,
    resolve_and_splice_small_loops, subdivide_segment_s434, trace_and_refine_small_loop,
    SplicedLoopChord,
};
use crate::stage4_tangency::solve_parametric_pair_newton;
use crate::tests_unit::{rj_box, rj_cylinder};
use crate::{
    boolean, native_backend, normalize3, ortho_basis, BRep, BRepEdge, BRepFace, BRepVertex, Curve,
    InputId, MeshBoolean, Point3, Stage4InvalidReason, Surface, Vector3, YangError,
};

#[inline]
fn v3(a: [f64; 3]) -> Vector3 {
    Vector3::new(a[0], a[1], a[2])
}

/// Dense parameter-grid conservativeness oracle: every exact surface normal
/// sampled over `[u0, u1] × [v0, v1]` (via `stage4_dt::eval_uv` +
/// `geom::surface_normal_at`) must overlap `gm` under both `+n` and `-n`, and
/// overlap `gm` with `overlaps_oriented` under `+n`.
fn assert_uv_rect_conservative(
    surface: &Surface,
    u_range: [f64; 2],
    v_range: [f64; 2],
    nu: usize,
    nv: usize,
) -> GaussMap {
    let gm = GaussMap::from_uv_rect(surface, u_range, v_range);
    for iu in 0..=nu {
        let tu = iu as f64 / nu as f64;
        let u = u_range[0] + tu * (u_range[1] - u_range[0]);
        for iv in 0..=nv {
            let tv = iv as f64 / nv as f64;
            let v = v_range[0] + tv * (v_range[1] - v_range[0]);
            let Ok(pt) = eval_uv(surface, Point2::new(u, v)) else {
                continue;
            };
            let Some(n) = surface_normal_at(*surface, pt) else {
                continue;
            };
            let pos = GaussMap::from_normal(n);
            let neg = GaussMap::from_normal([-n[0], -n[1], -n[2]]);
            assert!(
                gm.overlaps_oriented(&pos),
                "oriented false negative at (u={u}, v={v}), n={n:?}, gm={gm:?}"
            );
            assert!(
                gauss_maps_overlap(&gm, &pos),
                "+n false negative at (u={u}, v={v}), n={n:?}, gm={gm:?}"
            );
            assert!(
                gauss_maps_overlap(&gm, &neg),
                "-n false negative at (u={u}, v={v}), n={n:?}, gm={gm:?}"
            );
        }
    }
    gm
}

/// Dense 3D-barycentric conservativeness oracle: every point `x` sampled across
/// the filled 3D triangle `tri` (vertices, edges, interior) whose
/// `surface_normal_at(surface, x)` is defined must overlap `gm` (both oriented
/// `+n` and collinear `±n`).
fn assert_triangle_conservative(surface: &Surface, tri: [[f64; 3]; 3], steps: usize) -> GaussMap {
    let gm = GaussMap::from_triangle(surface, tri);
    for i in 0..=steps {
        for j in 0..=(steps - i) {
            let k = steps - i - j;
            let (li, lj, lk) = (
                i as f64 / steps as f64,
                j as f64 / steps as f64,
                k as f64 / steps as f64,
            );
            let p = Point3::new(
                li * tri[0][0] + lj * tri[1][0] + lk * tri[2][0],
                li * tri[0][1] + lj * tri[1][1] + lk * tri[2][1],
                li * tri[0][2] + lj * tri[1][2] + lk * tri[2][2],
            );
            let Some(n) = surface_normal_at(*surface, p) else {
                continue;
            };
            let pos = GaussMap::from_normal(n);
            let neg = GaussMap::from_normal([-n[0], -n[1], -n[2]]);
            assert!(
                gm.overlaps_oriented(&pos),
                "triangle oriented false negative at bary=({li},{lj},{lk}), n={n:?}, gm={gm:?}"
            );
            assert!(
                gm.overlaps(&pos) && gm.overlaps(&neg),
                "triangle collinear false negative at bary=({li},{lj},{lk}), n={n:?}, gm={gm:?}"
            );
        }
    }
    gm
}

// =========================================================================
// 1. Per-surface conservativeness & tightness pins
// =========================================================================

#[test]
fn plane_gauss_map_is_conservative_and_separates_non_collinear_planes() {
    let n_raw = normalize3([0.3, -0.7, 0.648074069840786]);
    let plane = Surface::Plane {
        normal: v3(n_raw),
        d: -12.5,
    };
    let gm_uv = assert_uv_rect_conservative(&plane, [-5.0, 5.0], [-3.0, 8.0], 8, 8);
    let tri = [[10.0, 2.0, 3.0], [-4.0, 9.0, 1.0], [0.5, -6.0, 7.0]];
    let gm_tri = assert_triangle_conservative(&plane, tri, 10);

    // Opposite-sense parallel plane overlaps under §4.2.2 collinearity, not oriented.
    let opp = GaussMap::from_normal([-n_raw[0], -n_raw[1], -n_raw[2]]);
    assert!(gm_uv.overlaps(&opp));
    assert!(!gm_uv.overlaps_oriented(&opp));

    // Plane tilted by 1e-5 rad is strictly rejected.
    let (e1, _) = ortho_basis(v3(n_raw));
    let e1a = e1.as_array();
    let eps = 1e-5_f64;
    let tilted = normalize3([
        eps.cos() * n_raw[0] + eps.sin() * e1a[0],
        eps.cos() * n_raw[1] + eps.sin() * e1a[1],
        eps.cos() * n_raw[2] + eps.sin() * e1a[2],
    ]);
    assert!(!gm_uv.overlaps(&GaussMap::from_normal(tilted)));
    assert!(!gm_tri.overlaps(&GaussMap::from_normal(tilted)));
}

#[test]
fn cylinder_gauss_map_uv_and_triangle_conservative_and_tight() {
    let axis = normalize3([1.0, 2.0, -0.5]);
    let cyl = Surface::Cylinder {
        axis_point: Point3::new(150.0, -80.0, 40.0),
        axis_dir: v3(axis),
        radius: 2.5,
    };

    // Narrow azimuth sector `[0.2, 0.7]` and wide sector `[-0.4, 3.2]` (`Δu > π`).
    let gm_narrow = assert_uv_rect_conservative(&cyl, [0.2, 0.7], [-4.0, 6.0], 24, 6);
    let _gm_wide = assert_uv_rect_conservative(&cyl, [-0.4, 3.2], [0.0, 2.0], 36, 4);

    // Inscribed 3D triangle on `[0.2, 0.7]`.
    let p0 = eval_uv(&cyl, Point2::new(0.2, -1.0)).unwrap().as_array();
    let p1 = eval_uv(&cyl, Point2::new(0.7, -1.0)).unwrap().as_array();
    let p2 = eval_uv(&cyl, Point2::new(0.45, 3.0)).unwrap().as_array();
    let gm_tri = assert_triangle_conservative(&cyl, [p0, p1, p2], 16);

    // Tightness: cylinder axis normal and orthogonal azimuth `u = 0.2 + π/2` do NOT overlap.
    assert!(!gm_narrow.overlaps(&GaussMap::from_normal(axis)));
    assert!(!gm_tri.overlaps(&GaussMap::from_normal(axis)));

    let orth_pt = eval_uv(&cyl, Point2::new(0.45 + FRAC_PI_2, 0.0)).unwrap();
    let orth_n = surface_normal_at(cyl, orth_pt).unwrap();
    assert!(!gm_narrow.overlaps(&GaussMap::from_normal(orth_n)));
    assert!(!gm_tri.overlaps(&GaussMap::from_normal(orth_n)));
}

#[test]
fn cone_gauss_map_uv_and_triangle_conservative_and_zone_tight() {
    let axis = normalize3([-0.4, 0.8, 0.4472135954999579]);
    for &half_angle in &[15.0_f64.to_radians(), FRAC_PI_4, 70.0_f64.to_radians()] {
        let cone = Surface::Cone {
            apex: Point3::new(-25.0, 10.0, 60.0),
            axis_dir: v3(axis),
            half_angle,
        };
        let gm_arc = assert_uv_rect_conservative(&cone, [0.4, 1.3], [0.5, 4.0], 24, 8);
        let gm_ring = assert_uv_rect_conservative(&cone, [0.0, TAU], [1.0, 2.0], 36, 4);

        let p0 = eval_uv(&cone, Point2::new(0.4, 0.8)).unwrap().as_array();
        let p1 = eval_uv(&cone, Point2::new(1.3, 0.8)).unwrap().as_array();
        let p2 = eval_uv(&cone, Point2::new(0.85, 3.2)).unwrap().as_array();
        let gm_tri = assert_triangle_conservative(&cone, [p0, p1, p2], 16);

        // Even a full 2π frustum band strictly away from the apex lives on the
        // thin small circle `∠(n, axis) = π/2 + α` and rejects an axial plane `±axis`.
        assert!(!gm_arc.overlaps(&GaussMap::from_normal(axis)));
        assert!(!gm_ring.overlaps(&GaussMap::from_normal(axis)));
        assert!(!gm_tri.overlaps(&GaussMap::from_normal(axis)));
    }
}

#[test]
fn sphere_gauss_map_uv_and_triangle_conservative_and_tight() {
    let sphere = Surface::Sphere {
        center: Point3::new(300.0, -120.0, 45.0),
        radius: 3.0,
    };

    let gm_patch = assert_uv_rect_conservative(&sphere, [0.3, 1.1], [0.2, 0.9], 24, 24);
    let _gm_wide = assert_uv_rect_conservative(&sphere, [-2.5, 1.4], [-0.8, 0.4], 32, 20);

    let p0 = eval_uv(&sphere, Point2::new(0.3, 0.2)).unwrap().as_array();
    let p1 = eval_uv(&sphere, Point2::new(1.1, 0.25)).unwrap().as_array();
    let p2 = eval_uv(&sphere, Point2::new(0.6, 0.9)).unwrap().as_array();
    let gm_tri = assert_triangle_conservative(&sphere, [p0, p1, p2], 16);

    // Quadrature normal `(u = 0.7 + π/2, v = 0)` is outside both bounds.
    let far_pt = eval_uv(&sphere, Point2::new(0.7 + FRAC_PI_2, 0.0)).unwrap();
    let far_n = surface_normal_at(sphere, far_pt).unwrap();
    assert!(!gm_patch.overlaps(&GaussMap::from_normal(far_n)));
    assert!(!gm_tri.overlaps(&GaussMap::from_normal(far_n)));

    // Two externally kissing spheres have antipodal normals (`+z` vs `-z`):
    // collinearity overlap MUST report `true` while oriented overlap reports `false`.
    let north = assert_uv_rect_conservative(&sphere, [-0.2, 0.2], [1.3, FRAC_PI_2], 12, 12);
    let south_b = Surface::Sphere {
        center: Point3::new(300.0, -120.0, 51.0),
        radius: 3.0,
    };
    let south = assert_uv_rect_conservative(&south_b, [-0.2, 0.2], [-FRAC_PI_2, -1.3], 12, 12);
    assert!(north.overlaps(&south));
    assert!(!north.overlaps_oriented(&south));
}

#[test]
fn torus_gauss_map_uv_and_triangle_conservative_and_tight() {
    let axis = normalize3([0.0, 0.6, 0.8]);
    let torus = Surface::Torus {
        center: Point3::new(-40.0, 75.0, 12.0),
        axis_dir: v3(axis),
        major_radius: 10.0,
        minor_radius: 2.0,
    };

    let gm_outer = assert_uv_rect_conservative(&torus, [0.1, 0.6], [-0.4, 0.5], 20, 20);
    let p0 = eval_uv(&torus, Point2::new(0.1, -0.3)).unwrap().as_array();
    let p1 = eval_uv(&torus, Point2::new(0.6, -0.2)).unwrap().as_array();
    let p2 = eval_uv(&torus, Point2::new(0.35, 0.45)).unwrap().as_array();
    let gm_tri = assert_triangle_conservative(&torus, [p0, p1, p2], 16);

    // Normal along `axis` (`v = π/2`) is strictly outside `v ∈ [-0.4, 0.5]`.
    assert!(!gm_outer.overlaps(&GaussMap::from_normal(axis)));
    assert!(!gm_tri.overlaps(&GaussMap::from_normal(axis)));
}

// =========================================================================
// 2. Required adversarial geometry pins
// =========================================================================

#[test]
fn adversarial_cone_apex_straddle_covers_polar_normal_cone_and_double_nappe() {
    let axis = [0.0, 0.0, 1.0];
    let half_angle = FRAC_PI_3; // 60°: smooth normals have ∠(n, -z) = 30°
    let cone = Surface::Cone {
        apex: Point3::new(1.0, -2.0, 3.0),
        axis_dir: v3(axis),
        half_angle,
    };

    // (a) UV rectangle touching the apex `v = 0` on a narrow azimuth slice.
    let gm_uv_apex = assert_uv_rect_conservative(&cone, [0.0, 0.4], [0.0, 1.5], 16, 16);

    // (b) 3D triangle with all three vertices at `h = +1.0 > 0` around the axis
    // (`0°, 120°, 240°`), so `h_min > 0` yet the cone patch spanned by the
    // triangle bridges across the cone axis and straddles the apex!
    let t0 = eval_uv(&cone, Point2::new(0.0, 1.0)).unwrap().as_array();
    let t1 = eval_uv(&cone, Point2::new(2.0 * FRAC_PI_3, 1.0))
        .unwrap()
        .as_array();
    let t2 = eval_uv(&cone, Point2::new(4.0 * FRAC_PI_3, 1.0))
        .unwrap()
        .as_array();
    let gm_tri_apex = assert_triangle_conservative(&cone, [t0, t1, t2], 18);

    // Every supporting plane of the +z solid cone at the apex has normal inside
    // the polar cone `∠(±n, -z) ≤ π/2 − α = 30°`, including `-z` itself and the
    // exact boundary `30°`.
    assert!(gm_uv_apex.overlaps_oriented(&GaussMap::from_normal([0.0, 0.0, -1.0])));
    assert!(gm_tri_apex.overlaps_oriented(&GaussMap::from_normal([0.0, 0.0, -1.0])));
    let boundary_theta = FRAC_PI_2 - half_angle; // 30° from -z
    let boundary_n = [boundary_theta.sin(), 0.0, -boundary_theta.cos()];
    assert!(gm_uv_apex.overlaps(&GaussMap::from_normal(boundary_n)));
    assert!(gm_tri_apex.overlaps(&GaussMap::from_normal(boundary_n)));

    // A vertical plane `normal = (1, 0, 0)` (`90°` from `±z`, steeper than the
    // 30° polar cone) is STILL strictly rejected on the single-nappe apex cap!
    assert!(!gm_uv_apex.overlaps(&GaussMap::from_normal([1.0, 0.0, 0.0])));
    assert!(!gm_tri_apex.overlaps(&GaussMap::from_normal([1.0, 0.0, 0.0])));

    // (c) Double-nappe straddle (`h_min < 0 < h_max`): emits `GaussMap::FULL`.
    let gm_two_nappe_uv = GaussMap::from_uv_rect(&cone, [0.1, 0.5], [-0.5, 0.5]);
    assert!(gm_two_nappe_uv.is_full());
    let neg_nappe_v = [1.0 + half_angle.tan(), -2.0, 3.0 - 1.0];
    let gm_two_nappe_tri = GaussMap::from_triangle(&cone, [t0, t1, neg_nappe_v]);
    assert!(gm_two_nappe_tri.is_full());
    assert!(gm_two_nappe_tri.overlaps(&GaussMap::from_normal([1.0, 0.0, 0.0])));
}

#[test]
fn adversarial_torus_inner_equator_and_crown_straddle() {
    let torus = Surface::Torus {
        center: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        major_radius: 8.0,
        minor_radius: 2.0,
    };

    // (a) Inner-equator UV patch straddling `v = π` (`v ∈ [3π/4, 5π/4]`) near `u = 0`.
    let gm_inner_uv = assert_uv_rect_conservative(
        &torus,
        [-0.15, 0.15],
        [3.0 * FRAC_PI_4, 5.0 * FRAC_PI_4],
        20,
        24,
    );

    // (b) 3D triangle straddling the inner equator `v = ±π` across `z = 0` (`d = ρ − R < 0`).
    let q0 = eval_uv(&torus, Point2::new(-0.12, PI - 0.25))
        .unwrap()
        .as_array();
    let q1 = eval_uv(&torus, Point2::new(0.12, PI - 0.20))
        .unwrap()
        .as_array();
    let q2 = eval_uv(&torus, Point2::new(0.0, -PI + 0.25))
        .unwrap()
        .as_array();
    let gm_inner_tri = assert_triangle_conservative(&torus, [q0, q1, q2], 18);

    // At `(u = 0, v = π)`, the outward torus normal points INWARD (`[-1, 0, 0]`),
    // NOT `[+1, 0, 0]`! Oriented overlap distinguishes them, and `[0, 1, 0]` is
    // rejected under collinearity as well.
    assert!(gm_inner_uv.overlaps_oriented(&GaussMap::from_normal([-1.0, 0.0, 0.0])));
    assert!(!gm_inner_uv.overlaps_oriented(&GaussMap::from_normal([1.0, 0.0, 0.0])));
    assert!(!gm_inner_uv.overlaps(&GaussMap::from_normal([0.0, 1.0, 0.0])));

    assert!(gm_inner_tri.overlaps_oriented(&GaussMap::from_normal([-1.0, 0.0, 0.0])));
    assert!(!gm_inner_tri.overlaps_oriented(&GaussMap::from_normal([1.0, 0.0, 0.0])));
    assert!(!gm_inner_tri.overlaps(&GaussMap::from_normal([0.0, 1.0, 0.0])));

    // (c) Crown straddle across `v = +π/2` (`cos v` flips sign from outer `+r̂` to inner `-r̂`).
    let gm_crown = assert_uv_rect_conservative(&torus, [-0.2, 0.2], [1.1, 2.0], 20, 24);
    let c0 = eval_uv(&torus, Point2::new(-0.15, 1.2)).unwrap().as_array();
    let c1 = eval_uv(&torus, Point2::new(0.15, 1.2)).unwrap().as_array();
    let c2 = eval_uv(&torus, Point2::new(0.0, 1.95)).unwrap().as_array();
    let gm_crown_tri = assert_triangle_conservative(&torus, [c0, c1, c2], 18);
    assert!(gm_crown.overlaps_oriented(&GaussMap::from_normal([0.0, 0.0, 1.0])));
    assert!(gm_crown_tri.overlaps_oriented(&GaussMap::from_normal([0.0, 0.0, 1.0])));
    assert!(!gm_crown.overlaps(&GaussMap::from_normal([0.0, 1.0, 0.0])));
    assert!(!gm_crown_tri.overlaps(&GaussMap::from_normal([0.0, 1.0, 0.0])));
}

#[test]
fn adversarial_sphere_pole_straddle_north_south_and_tilted_frame() {
    let sphere = Surface::Sphere {
        center: Point3::new(-5.0, 8.0, -3.0),
        radius: 4.0,
    };

    // (a) North-pole UV cap `[−π, π] × [75°, 90°]`.
    let gm_np_uv = assert_uv_rect_conservative(
        &sphere,
        [-PI, PI],
        [75.0_f64.to_radians(), FRAC_PI_2],
        36,
        12,
    );
    assert!(gm_np_uv.overlaps_oriented(&GaussMap::from_normal([0.0, 0.0, 1.0])));
    assert!(!gm_np_uv.overlaps(&GaussMap::from_normal([1.0, 0.0, 0.0])));

    // (b) 3D triangle straddling the NORTH pole (`v = +70°` at all 3 vertices,
    // longitudes `0°, 120°, 240°` — no vertex at `+z`, yet barycenter projects to `+z`).
    let lat = 70.0_f64.to_radians();
    let n0 = eval_uv(&sphere, Point2::new(0.0, lat)).unwrap().as_array();
    let n1 = eval_uv(&sphere, Point2::new(2.0 * FRAC_PI_3, lat))
        .unwrap()
        .as_array();
    let n2 = eval_uv(&sphere, Point2::new(4.0 * FRAC_PI_3, lat))
        .unwrap()
        .as_array();
    let gm_np_tri = assert_triangle_conservative(&sphere, [n0, n1, n2], 18);
    assert!(gm_np_tri.overlaps_oriented(&GaussMap::from_normal([0.0, 0.0, 1.0])));
    assert!(!gm_np_tri.overlaps(&GaussMap::from_normal([1.0, 0.0, 0.0])));

    // (c) South-pole straddling 3D triangle (`v = -70°`).
    let s0 = eval_uv(&sphere, Point2::new(0.3, -lat)).unwrap().as_array();
    let s1 = eval_uv(&sphere, Point2::new(0.3 + 2.0 * FRAC_PI_3, -lat))
        .unwrap()
        .as_array();
    let s2 = eval_uv(&sphere, Point2::new(0.3 + 4.0 * FRAC_PI_3, -lat))
        .unwrap()
        .as_array();
    let gm_sp_tri = assert_triangle_conservative(&sphere, [s0, s1, s2], 18);
    assert!(gm_sp_tri.overlaps_oriented(&GaussMap::from_normal([0.0, 0.0, -1.0])));
    assert!(!gm_sp_tri.overlaps(&GaussMap::from_normal([0.0, 1.0, 0.0])));

    // (d) Tilted revolve-frame pole `ŵ = normalize([1, 1, 1])` (`tessellate_sphere_patch_in_frame`).
    let w = normalize3([1.0, 1.0, 1.0]);
    let (e1, e2) = ortho_basis(v3(w));
    let (e1a, e2a) = (e1.as_array(), e2.as_array());
    let c = [-5.0, 8.0, -3.0];
    let r = 4.0;
    let ring_pt = |ang: f64| -> [f64; 3] {
        let (cl, sl) = (lat.cos(), lat.sin());
        let (ca, sa) = (ang.cos(), ang.sin());
        let dir = [
            cl * (ca * e1a[0] + sa * e2a[0]) + sl * w[0],
            cl * (ca * e1a[1] + sa * e2a[1]) + sl * w[1],
            cl * (ca * e1a[2] + sa * e2a[2]) + sl * w[2],
        ];
        [c[0] + r * dir[0], c[1] + r * dir[1], c[2] + r * dir[2]]
    };
    let gm_tilted = assert_triangle_conservative(
        &sphere,
        [
            ring_pt(0.0),
            ring_pt(2.0 * FRAC_PI_3),
            ring_pt(4.0 * FRAC_PI_3),
        ],
        18,
    );
    assert!(gm_tilted.overlaps_oriented(&GaussMap::from_normal(w)));
    assert!(!gm_tilted.overlaps(&GaussMap::from_normal(e1a)));
}

#[test]
fn adversarial_periodic_seam_straddle_across_all_revolved_surfaces() {
    let u_lo = 170.0_f64.to_radians();
    let u_hi = 190.0_f64.to_radians(); // unwrapped across π

    let cyl = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 2.0,
    };
    let cone = Surface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 35.0_f64.to_radians(),
    };
    let sphere = Surface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 2.0,
    };
    let torus = Surface::Torus {
        center: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        major_radius: 5.0,
        minor_radius: 1.0,
    };

    for surf in [cyl, cone, sphere, torus] {
        let v_pair = match surf {
            Surface::Cylinder { .. } => [-1.0, 1.0],
            Surface::Cone { .. } => [1.0, 2.0],
            Surface::Sphere { .. } => [-0.2, 0.25],
            Surface::Torus { .. } => [-0.2, 0.25],
            Surface::Plane { .. } => unreachable!(),
        };
        let gm_uv = assert_uv_rect_conservative(&surf, [u_lo, u_hi], v_pair, 16, 12);

        let a0 = eval_uv(&surf, Point2::new(u_lo, v_pair[0]))
            .unwrap()
            .as_array();
        let a1 = eval_uv(&surf, Point2::new(u_hi - TAU, v_pair[0]))
            .unwrap()
            .as_array();
        let a2 = eval_uv(&surf, Point2::new(PI, v_pair[1]))
            .unwrap()
            .as_array();
        let gm_tri = assert_triangle_conservative(&surf, [a0, a1, a2], 16);

        let quad_pt =
            eval_uv(&surf, Point2::new(FRAC_PI_2, 0.5 * (v_pair[0] + v_pair[1]))).unwrap();
        let quad_n = surface_normal_at(surf, quad_pt).unwrap();
        assert!(
            !gm_uv.overlaps(&GaussMap::from_normal(quad_n)),
            "UV seam bloat on {surf:?}"
        );
        assert!(
            !gm_tri.overlaps(&GaussMap::from_normal(quad_n)),
            "triangle seam bloat on {surf:?}"
        );
    }
}

#[test]
fn boundary_tangent_gauss_maps_in_rotated_frames_never_false_negative() {
    let axis_a = normalize3([0.37, -0.81, 0.454972526643093]);
    let alpha_a = 28.0_f64.to_radians();
    let cone_a = Surface::Cone {
        apex: Point3::new(12.3, -45.6, 78.9),
        axis_dir: v3(axis_a),
        half_angle: alpha_a,
    };
    let u_touch = 1.23456789;
    let pt_touch = eval_uv(&cone_a, Point2::new(u_touch, 2.5)).unwrap();
    let n_touch = surface_normal_at(cone_a, pt_touch).unwrap();

    let gm_a = GaussMap::from_uv_rect(&cone_a, [u_touch - 0.4, u_touch], [1.0, 3.0]);
    let plane_b = Surface::Plane {
        normal: v3([-n_touch[0], -n_touch[1], -n_touch[2]]),
        d: 0.0,
    };
    let gm_b = GaussMap::from_uv_rect(&plane_b, [0.0, 1.0], [0.0, 1.0]);
    assert!(gauss_maps_overlap(&gm_a, &gm_b));
}

// =========================================================================
// 3. §4.2.2 Gauss-map admission + §4.3.3/§4.3.4/§4.4.1 small-loop pins
// =========================================================================

/// Build a canonical 3-face cylinder with explicit seam radial direction
/// `seam_dir` (matching `kernel-v2`'s sketch `u_axis`: `+X` on `Z`-extrudes,
/// `+Y` on `X`-extrudes in `C0116.waffle` / `C0118.waffle`).
fn extruded_cylinder(
    axis_point: [f64; 3],
    axis_dir: [f64; 3],
    seam_dir: [f64; 3],
    radius: f64,
    height: f64,
) -> BRep {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let d = normalize3(axis_dir);
    let along = dot(seam_dir, d);
    let e1 = normalize3([
        seam_dir[0] - along * d[0],
        seam_dir[1] - along * d[1],
        seam_dir[2] - along * d[2],
    ]);
    let bot = axis_point;
    let top = [
        bot[0] + d[0] * height,
        bot[1] + d[1] * height,
        bot[2] + d[2] * height,
    ];
    let verts = vec![
        BRepVertex {
            point: Point3::new(
                bot[0] + e1[0] * radius,
                bot[1] + e1[1] * radius,
                bot[2] + e1[2] * radius,
            ),
        },
        BRepVertex {
            point: Point3::new(
                top[0] + e1[0] * radius,
                top[1] + e1[1] * radius,
                top[2] + e1[2] * radius,
            ),
        },
    ];
    let edges = vec![
        BRepEdge {
            start: 0,
            end: 0,
            curve: Curve::Circle {
                center: Point3::new(bot[0], bot[1], bot[2]),
                normal: Vector3::new(-d[0], -d[1], -d[2]),
                radius,
            },
        },
        BRepEdge {
            start: 1,
            end: 1,
            curve: Curve::Circle {
                center: Point3::new(top[0], top[1], top[2]),
                normal: Vector3::new(d[0], d[1], d[2]),
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
                normal: Vector3::new(-d[0], -d[1], -d[2]),
                d: dot(d, bot),
            },
            outer_loop: vec![0],
            inner_loops: Vec::new(),
            reversed: false,
        },
        BRepFace {
            surface: Surface::Plane {
                normal: Vector3::new(d[0], d[1], d[2]),
                d: -dot(d, top),
            },
            outer_loop: vec![1],
            inner_loops: Vec::new(),
            reversed: false,
        },
    ];
    BRep::new(verts, edges, faces).expect("extruded cylinder fixture builds")
}

#[test]
fn gauss_map_prunes_non_overlapping_proximity_and_admits_c0116_without_trim_exemption() {
    let backend = native_backend().expect("native backend");

    // (a) Non-overlapping curved × plane `< 2d_ε` proximity:
    // Cylinder along Z (`R = 0.5`, `z ∈ [0, 2]`) has lateral outward normals
    // strictly horizontal (`n_z = 0`). A broad disk `z ∈ [2.000_05, 2.5]` sits
    // `5e-5 < d_ε` above the Z-cylinder: the cylinder's lateral triangles
    // (`n_z = 0`) are within `5e-5 < d_ε` of the disk's bottom plane
    // (`n = (0, 0, -1)`), and their Gauss maps are orthogonal (`0°` overlap),
    // so `discover_small_loop_candidate_areas` MUST find the non-crossing
    // `< 2d_ε` lateral×plane area and prune it (`gauss_overlap == false`)
    // with ZERO trim-boundary exemption!
    let cyl_a = extruded_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], 0.5, 2.0);
    let plate_b = rj_cylinder([0.0, 0.0, 2.000_05], [0.0, 0.0, 1.0], 1.0, 0.5);
    let la_pruned = backend
        .labeled_arrangement(cyl_a.as_mesh(), plate_b.as_mesh())
        .expect("arrangement");
    let pruned_areas = discover_small_loop_candidate_areas(
        &cyl_a,
        cyl_a.as_mesh(),
        cyl_a.tri_face(),
        &plate_b,
        plate_b.as_mesh(),
        plate_b.tri_face(),
        &la_pruned,
    );
    let lateral_vs_bottom: Vec<_> = pruned_areas
        .iter()
        .filter(|ar| {
            matches!(
                cyl_a.faces()[ar.face_a as usize].surface,
                Surface::Cylinder { .. }
            ) && matches!(
                plate_b.faces()[ar.face_b as usize].surface,
                Surface::Plane { .. }
            )
        })
        .collect();
    assert!(
        !lateral_vs_bottom.is_empty(),
        "expected < 2d_ε non-crossing lateral×plane candidate area"
    );
    assert!(
        lateral_vs_bottom.iter().all(|ar| !ar.gauss_overlap),
        "closed-form GaussMap must reject orthogonal cylinder-lateral vs z-plane without trim exemption: {lateral_vs_bottom:?}"
    );

    // (b) `C0116` (`R = 0.5` Z-cylinder × `R = 0.3` X-cylinder at `y = 0.79, z = 1.0`,
    // `0.01`-deep interior small loop): coarse cylinder strip triangles touch
    // the `z = 0, 2` and `x = -1.5, 2.0` cap rims, yet `GaussMap::overlaps`
    // admits the `(lateral, lateral)` candidate area (`gauss_overlap == true`)
    // unconditionally with NO trim-boundary exemption!
    let tool_c0116 = extruded_cylinder(
        [-1.5, 0.79, 1.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        0.3,
        3.5,
    );
    let la_c0116 = backend
        .labeled_arrangement(cyl_a.as_mesh(), tool_c0116.as_mesh())
        .expect("C0116 initial arrangement");
    assert!(
        la_c0116.intersection_edges.is_empty(),
        "coarse Stage-1 meshes miss C0116's 0.01-deep small loop on pass 0"
    );
    let c0116_areas = discover_small_loop_candidate_areas(
        &cyl_a,
        cyl_a.as_mesh(),
        cyl_a.tri_face(),
        &tool_c0116,
        tool_c0116.as_mesh(),
        tool_c0116.tri_face(),
        &la_c0116,
    );
    assert!(
        c0116_areas.iter().any(|ar| ar.gauss_overlap),
        "GaussMap must admit C0116's lateral×lateral small-loop area: {c0116_areas:?}"
    );
}

#[test]
fn bajaj_traces_and_s434_refines_closed_small_loops_across_surface_pairs() {
    let assert_valid_refined_loop = |name: &str, pts: &[Point3], sa: Surface, sb: Surface| {
        assert!(
            pts.len() >= 4,
            "{name}: expected >= 4 loop points, got {}",
            pts.len()
        );
        let n = pts.len();
        for i in 0..n {
            let p = pts[i];
            let q = pts[(i + 1) % n];
            let da = signed_distance_to_surface(sa, p).unwrap().abs();
            let db = signed_distance_to_surface(sb, p).unwrap().abs();
            assert!(
                da < 1e-12 && db < 1e-12,
                "{name}[{i}]: off surface da={da:e}, db={db:e}"
            );
            // Yang §4.3.4 (`:579-592` L): every final chord `(p, q)` satisfies
            // `h < 10^2 d_p, l < 10^3 d_p, α < π/18` against its curve midpoint `m`.
            let chord_mid = Point3::new(
                0.5 * (p.x() + q.x()),
                0.5 * (p.y() + q.y()),
                0.5 * (p.z() + q.z()),
            );
            let m = solve_parametric_pair_newton(chord_mid, chord_mid, sa, sb)
                .expect("curve midpoint converges")
                .point;
            assert!(
                paper_chain_sample_redundant(p.as_array(), m.as_array(), q.as_array()),
                "{name}[{i}]: chord (p, m, q) violates §4.3.4 paper_chain_sample_redundant"
            );
        }
    };

    // 1. C0116 Cylinder × Cylinder (`0.01` deep), including exact symmetry-plane
    // Sederberg collinear-normal seed `(0, 0.5, 1)` vs `(0, 0.49, 1)`.
    let cyl_z = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: 0.5,
    };
    let cyl_x_01 = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.79, 1.0),
        axis_dir: Vector3::new(1.0, 0.0, 0.0),
        radius: 0.3,
    };
    let seed_01 = solve_parametric_pair_newton(
        Point3::new(0.0, 0.5, 1.0),
        Point3::new(0.0, 0.49, 1.0),
        cyl_z,
        cyl_x_01,
    )
    .expect("C0116 symmetry-plane seed escapes Sederberg extremum onto loop");
    let loop_01 = trace_and_refine_small_loop(seed_01.point, cyl_z, cyl_x_01, &[], &[], 2.0)
        .expect("C0116 closed loop");
    assert_valid_refined_loop("C0116", &loop_01, cyl_z, cyl_x_01);

    // 2. C0118 sub-band Cylinder × Cylinder (`1e-8` deep, loop diameter `~2.0e-4`),
    // seeded directly on the symmetry plane `x = 0`.
    let cyl_x_sub = Surface::Cylinder {
        axis_point: Point3::new(0.0, 0.8 - 1e-8, 1.0),
        axis_dir: Vector3::new(1.0, 0.0, 0.0),
        radius: 0.3,
    };
    let seed_sub = solve_parametric_pair_newton(
        Point3::new(0.0, 0.5, 1.0),
        Point3::new(0.0, 0.5 - 1e-8, 1.0),
        cyl_z,
        cyl_x_sub,
    )
    .expect("C0118 symmetry-plane seed escapes Sederberg extremum onto loop");
    let loop_sub = trace_and_refine_small_loop(seed_sub.point, cyl_z, cyl_x_sub, &[], &[], 2.0)
        .expect("C0118 sub-band closed loop");
    assert_valid_refined_loop("C0118-subband", &loop_sub, cyl_z, cyl_x_sub);

    // 3. Sphere × Plane small loop (`1e-4` deep).
    let sph = Surface::Sphere {
        center: Point3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    };
    let pl_z = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: -1e-4,
    };
    let seed_sp = solve_parametric_pair_newton(
        Point3::new(0.015, 0.0, 1e-4),
        Point3::new(0.015, 0.0, 1e-4),
        sph,
        pl_z,
    )
    .expect("sphere-plane seed");
    let loop_sp = trace_and_refine_small_loop(seed_sp.point, sph, pl_z, &[], &[], 2.0)
        .expect("sphere-plane closed loop");
    assert_valid_refined_loop("sphere-plane", &loop_sp, sph, pl_z);

    // 4. Torus × Plane small outer-equator graze loop (`1e-4` deep).
    let tor = Surface::Torus {
        center: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        major_radius: 3.0,
        minor_radius: 1.0,
    };
    let pl_x = Surface::Plane {
        normal: Vector3::new(1.0, 0.0, 0.0),
        d: -(4.0 - 1e-4),
    };
    let seed_tp = solve_parametric_pair_newton(
        Point3::new(4.0 - 1e-4, 0.02, 0.0),
        Point3::new(4.0 - 1e-4, 0.02, 0.0),
        tor,
        pl_x,
    )
    .expect("torus-plane seed");
    let loop_tp = trace_and_refine_small_loop(seed_tp.point, tor, pl_x, &[], &[], 4.0)
        .expect("torus-plane closed loop");
    assert_valid_refined_loop("torus-plane", &loop_tp, tor, pl_x);

    // 5. Open boundary-clipped arc MUST be declined (`None`) when the loop
    // crosses a face boundary crease (`z = 1.0` rim cutting right through C0116).
    let clipped_boss = rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.5, 1.0);
    let clip_creases = CreaseIndex::build(&clipped_boss);
    assert!(
        trace_and_refine_small_loop(
            seed_01.point,
            cyl_z,
            cyl_x_01,
            &clip_creases.faces[0],
            &[],
            2.0,
        )
        .is_none(),
        "boundary-clipped arc must NOT be closed into an interior loop"
    );
}

#[test]
fn small_loop_boolean_resolves_c0116_and_subband_c0118_on_pass_zero() {
    let backend = native_backend().expect("native backend");
    let boss = extruded_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], 0.5, 2.0);

    // (a) C0116 (`0.01`-deep perpendicular graze): resolves on pass 0 with zero
    // failure triggers!
    let tool_c0116 = extruded_cylinder(
        [-1.5, 0.79, 1.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        0.3,
        3.5,
    );
    let union_c0116 = boolean(&boss, &tool_c0116, BoolOp::Union, &backend).expect("C0116 union");
    assert!(
        failures::recorded().is_empty(),
        "C0116 small loop should resolve on pass 0 without failure sites: {:?}",
        failures::recorded()
    );
    // Two 3-face cylinders fused along 1 closed loop have 6 faces (2 trimmed
    // laterals + 4 planar end caps) as a single genus-0 shell, not 2 disjoint shells!
    assert_eq!(union_c0116.faces().len(), 6);
    assert!(
        union_c0116
            .edges()
            .iter()
            .any(|e| matches!(e.curve, Curve::SurfacePair { .. })),
        "C0116 union must carry the exact SurfacePair small-loop intersection edge"
    );

    let inter_c0116 =
        boolean(&boss, &tool_c0116, BoolOp::Intersect, &backend).expect("C0116 intersect");
    assert_eq!(
        inter_c0116.faces().len(),
        2,
        "C0116 intersection lens has 2 cylindrical cap faces"
    );

    // (b) Shallow small loop above `MIN_FEATURE_SIZE` (`2e-5` deep, `|t_A - t_B| = 2e-5
    // >= MIN_FEATURE_SIZE * scale`): resolves on pass 0 into a single fused solid
    // with an exact `SurfacePair` loop edge and 2-face intersection lens.
    let tool_shallow = extruded_cylinder(
        [-1.5, 0.8 - 2e-5, 1.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        0.3,
        3.5,
    );
    let union_shallow =
        boolean_once(&boss, &tool_shallow, BoolOp::Union, &backend).expect("2e-5 pass-0 union");
    assert!(
        failures::recorded().is_empty(),
        "2e-5 small loop should resolve on pass 0 with zero failure sites: {:?}",
        failures::recorded()
    );
    assert_eq!(union_shallow.faces().len(), 6);
    assert!(
        union_shallow
            .edges()
            .iter()
            .any(|e| matches!(e.curve, Curve::SurfacePair { .. })),
        "2e-5 small-loop union must carry the exact SurfacePair loop edge"
    );
    let inter_shallow = boolean_once(&boss, &tool_shallow, BoolOp::Intersect, &backend)
        .expect("2e-5 pass-0 intersect");
    assert_eq!(inter_shallow.faces().len(), 2);

    // (c) C0118 (`1e-8`-deep sub-`MIN_FEATURE_SIZE` graze, `|t_A - t_B| = 1e-8 <
    // MIN_FEATURE_SIZE * scale`, S1): suppressed by `SubMinFeatureTwinSheet`
    // without emitting sub-`MIN_FEATURE_SIZE` twin sheets or failure sites.
    let tool_c0118 = extruded_cylinder(
        [-1.5, 0.8 - 1e-8, 1.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        0.3,
        3.5,
    );
    let union_c0118 =
        boolean_once(&boss, &tool_c0118, BoolOp::Union, &backend).expect("C0118 pass-0 union");
    assert!(
        failures::recorded().is_empty(),
        "C0118 sub-MIN_FEATURE_SIZE graze should not record failure sites: {:?}",
        failures::recorded()
    );
    assert_eq!(union_c0118.faces().len(), 6);
    assert_eq!(
        union_c0118
            .edges()
            .iter()
            .filter(|e| matches!(e.curve, Curve::SurfacePair { .. }))
            .count(),
        0,
        "C0118 must not emit sub-MIN_FEATURE_SIZE twin sheets"
    );
}

/// S10 red-first end-to-end pin (failed RED on base `main` `a236e510` with
/// `MissedIntersection` on pass 0; passes GREEN here while preserving `C0118`).
#[test]
fn small_loop_pass_zero_resolves_c0116_and_preserves_sub_feature_c0118() {
    let backend = native_backend().expect("native backend");
    let boss = extruded_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], 0.5, 2.0);

    let tool_c0118 = extruded_cylinder(
        [-1.5, 0.8 - 1e-8, 1.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        0.3,
        3.5,
    );
    let c0118_p0 = boolean_once(&boss, &tool_c0118, BoolOp::Union, &backend);
    let c0118_p0_fails = failures::recorded();
    let c0118_full = boolean(&boss, &tool_c0118, BoolOp::Union, &backend).expect("C0118 union");
    let c0118_pair_edges = c0118_full
        .edges()
        .iter()
        .filter(|e| matches!(e.curve, Curve::SurfacePair { .. }))
        .count();
    assert!(c0118_p0.is_ok(), "C0118 pass 0 must succeed");
    assert!(
        c0118_p0_fails.is_empty(),
        "C0118 pass 0 must record zero failures, got {c0118_p0_fails:?}"
    );
    assert_eq!(c0118_full.faces().len(), 6);
    assert_eq!(
        c0118_pair_edges, 0,
        "C0118 must not emit sub-MIN_FEATURE_SIZE SurfacePair twin sheets"
    );

    let tool_c0116 = extruded_cylinder(
        [-1.5, 0.79, 1.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        0.3,
        3.5,
    );
    let union_c0116 =
        boolean_once(&boss, &tool_c0116, BoolOp::Union, &backend).expect("C0116 pass-0 union");
    assert!(
        failures::recorded().is_empty(),
        "C0116 small loop should resolve on pass 0 with zero failure sites, got {:?}",
        failures::recorded()
    );
    assert!(
        union_c0116
            .edges()
            .iter()
            .any(|e| matches!(e.curve, Curve::SurfacePair { .. })),
        "C0116 pass-0 union must carry the exact SurfacePair small-loop intersection edge"
    );
    let inter_c0116 = boolean_once(&boss, &tool_c0116, BoolOp::Intersect, &backend)
        .expect("C0116 pass-0 intersect");
    assert_eq!(inter_c0116.faces().len(), 2);
}

/// S3 & S4 pins:
/// - `(Plane, Plane)` proximity is inspected without surface-type pre-skips and
///   pruned by `GaussMap::overlaps` when non-parallel;
/// - an unresolved Gauss-admitted candidate area records a §4.5 `FailureSite`
///   instead of silently declining;
/// - `subdivide_segment_s434` fails loudly (`false`) at `depth >= 12` on an
///   unrefined chord;
/// - `register_spliced_loop_edges_in_arrangement` fails loudly with
///   `Err(Stage4RegionInvalid)` and records a `FailureSite` on a missed chord.
#[test]
fn small_loop_loud_failure_sites_plane_pruning_and_depth_overflow() {
    let backend = native_backend().expect("native backend");

    // (a) S4: `(Plane, Plane)` non-parallel proximity (`< 2 d_p`) is discovered
    // without a `(Plane, Plane)` pre-skip and pruned by `GaussMap::overlaps`.
    let box_a = rj_box([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    let z_drop = 0.1 * (0.2 / 1.04_f64.sqrt());
    let tilt_cyl = extruded_cylinder(
        [0.5, 0.5, 1.0 + z_drop + 5e-8],
        [0.2, 0.0, 1.0],
        [1.0, 0.0, -0.2],
        0.1,
        0.5,
    );
    let la_pp = backend
        .labeled_arrangement(box_a.as_mesh(), tilt_cyl.as_mesh())
        .expect("plane-plane arrangement");
    let areas_pp = discover_small_loop_candidate_areas(
        &box_a,
        box_a.as_mesh(),
        box_a.tri_face(),
        &tilt_cyl,
        tilt_cyl.as_mesh(),
        tilt_cyl.tri_face(),
        &la_pp,
    );
    let plane_plane_areas: Vec<_> = areas_pp
        .iter()
        .filter(|ar| {
            matches!(
                box_a.faces()[ar.face_a as usize].surface,
                Surface::Plane { .. }
            ) && matches!(
                tilt_cyl.faces()[ar.face_b as usize].surface,
                Surface::Plane { .. }
            )
        })
        .collect();
    assert!(
        !plane_plane_areas.is_empty(),
        "expected non-parallel (Plane, Plane) proximity area without surface-type pre-skip"
    );
    assert!(
        plane_plane_areas.iter().all(|ar| !ar.gauss_overlap),
        "non-parallel (Plane, Plane) area must be pruned by GaussMap::overlaps: {plane_plane_areas:?}"
    );

    // (b) S3: Gauss-admitted boundary-clipped candidate area (`z = 1.04` cap rim
    // slicing through `C0116`'s `0.01`-deep loop while coarse meshes remain
    // non-crossing) MUST record a `FailureSite` on `(face_a=0, face_b=0)`.
    let clipped_boss =
        extruded_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], 0.5, 1.04);
    let tool_c0116 = extruded_cylinder(
        [-1.5, 0.79, 1.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        0.3,
        3.5,
    );
    let la_clip = backend
        .labeled_arrangement(clipped_boss.as_mesh(), tool_c0116.as_mesh())
        .expect("clipped arrangement");
    assert!(
        la_clip.intersection_edges.is_empty(),
        "coarse clipped cylinders remain non-crossing on pass 0"
    );
    failures::reset();
    let upd = resolve_and_splice_small_loops(
        &clipped_boss,
        clipped_boss.as_mesh(),
        clipped_boss.tri_face(),
        &tool_c0116,
        tool_c0116.as_mesh(),
        tool_c0116.tri_face(),
        &la_clip,
    );
    assert!(upd.is_none(), "boundary-clipped loop must not splice");
    let rec = failures::recorded();
    assert!(
        rec.iter().any(|s| {
            s.trigger == FailureTrigger::RelocationDiverged
                && s.faces == vec![(InputId::A, 0), (InputId::B, 0)]
        }),
        "unresolved Gauss-admitted area must record a §4.5 FailureSite, got {rec:?}"
    );

    // (c) S3: `subdivide_segment_s434` at `depth == 12` with `chord > 2 * dp`
    // returns loud `false` instead of silently accepting an unrefined chord.
    let sph = Surface::Sphere {
        center: Point3::new(0.0, 0.0, 0.0),
        radius: 1.0,
    };
    let pln = Surface::Plane {
        normal: Vector3::new(0.0, 0.0, 1.0),
        d: 0.0,
    };
    let mut out_pts = Vec::new();
    assert!(
        !subdivide_segment_s434(
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            sph,
            pln,
            &[],
            &[],
            1e-7,
            12,
            &mut out_pts,
        ),
        "depth >= 12 with chord > 2*dp must fail loudly (false)"
    );

    // (d) S3: `register_spliced_loop_edges_in_arrangement` with a missing chord
    // fails loudly (`Err(Stage4RegionInvalid)`) and records a `FailureSite`.
    let mut la_miss = la_clip;
    failures::reset();
    let bogus_chord = SplicedLoopChord {
        p: Point3::new(9.0, 9.0, 9.0),
        q: Point3::new(9.1, 9.0, 9.0),
        face_a: 0,
        face_b: 0,
    };
    let reg_res = register_spliced_loop_edges_in_arrangement(&mut la_miss, &[bogus_chord], 1.0);
    assert!(
        matches!(
            reg_res,
            Err(YangError::Stage4RegionInvalid {
                reason: Stage4InvalidReason::LocalRefinementRequired,
                ..
            })
        ),
        "missing spliced chord must return Err(Stage4RegionInvalid), got {reg_res:?}"
    );
    assert!(
        !failures::recorded().is_empty(),
        "missing spliced chord must record a §4.5 FailureSite"
    );
}
