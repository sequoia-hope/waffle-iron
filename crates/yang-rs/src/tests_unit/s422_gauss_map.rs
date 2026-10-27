//! Unit and wiring pins for Yang 2025 §4.2.2 closed-form Gauss-map estimation
//! and overlap filtering (`crates/yang-rs/src/gauss_map.rs`,
//! `crates/yang-rs/src/shell_contact.rs::contact_pairs_with_stats`,
//! ledger **N92**, compliance matrix rows **4.2-12**, **4.2-13**, **5.6-02**,
//! spec `specs/yang_422_gauss_map_filter.md`).

use std::f64::consts::{FRAC_PI_2, FRAC_PI_3, FRAC_PI_4, PI, TAU};

use cad_primitives::Point2;

use crate::gauss_map::{gauss_maps_overlap, GaussMap};
use crate::geom::surface_normal_at;
use crate::shell_contact::contact_pairs_with_stats;
use crate::stage4_dt::eval_uv;
use crate::tests_unit::rj_cylinder;
use crate::{normalize3, ortho_basis, BRepFace, Mesh, Point3, Surface, Vector3};

#[inline]
fn v3(a: [f64; 3]) -> Vector3 {
    Vector3::new(a[0], a[1], a[2])
}

#[inline]
fn p3(a: [f64; 3]) -> Point3 {
    Point3::new(a[0], a[1], a[2])
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
    // Vertices at `u = +170°` (`2.967 rad`) and `u = -170°` (`-2.967 rad`)
    // straddle the `u = ±π` `atan2` branch cut over a true `20°` span around
    // `-e1`. A buggy naive `[min u, max u]` unwrap would span `340°` around `+e1`
    // and either bloat to the whole circle or miss `-e1`!
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

        // Quadrature point at `u = +π/2` (`90°` away from seam `u = π`) MUST be rejected,
        // proving the 3D-triangle unwrap stayed tight (`20°`) across the `±π` seam!
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
    // Construct two cones sharing an exact common normal at a single generator
    // direction in a non-axis-aligned frame: exact boundary tangency must
    // report `overlaps == true` despite IEEE-754 trig rounding.
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

    // Region A ends right at `u_touch` as a corner.
    let gm_a = GaussMap::from_uv_rect(&cone_a, [u_touch - 0.4, u_touch], [1.0, 3.0]);
    // Region B is a plane with normal `-n_touch` (anti-parallel boundary touch).
    let plane_b = Surface::Plane {
        normal: v3([-n_touch[0], -n_touch[1], -n_touch[2]]),
        d: 0.0,
    };
    let gm_b = GaussMap::from_uv_rect(&plane_b, [0.0, 1.0], [0.0, 1.0]);
    assert!(gauss_maps_overlap(&gm_a, &gm_b));
}

// =========================================================================
// 3. End-to-end `shell_contact::contact_pairs` wiring pins
// =========================================================================

fn dummy_face(surface: Surface) -> BRepFace {
    BRepFace {
        surface,
        outer_loop: Vec::new(),
        inner_loops: Vec::new(),
        reversed: false,
    }
}

/// Build a closed cone-frustum mesh (`z_bands` axial strips × `n_seg` azimuth
/// segments) with face 0 = cone lateral (`z_bands ≥ 3` gives an interior middle
/// strip with zero trim-boundary edges!), face 1 = bottom cap, face 2 = top cap.
fn closed_subdivided_cone_frustum(
    apex_z: f64,
    half_angle: f64,
    z_heights: &[f64],
    n_seg: usize,
) -> (Mesh, Vec<u32>, Vec<BRepFace>) {
    assert!(z_heights.len() >= 2 && n_seg >= 6);
    let nb = z_heights.len();
    let mut verts = Vec::with_capacity(nb * n_seg + 2);
    for &z in z_heights {
        let r = (z - apex_z) * half_angle.tan();
        for i in 0..n_seg {
            let u = TAU * (i as f64) / (n_seg as f64);
            verts.push(Point3::new(r * u.cos(), r * u.sin(), z));
        }
    }
    let bot_center = verts.len() as u32;
    verts.push(Point3::new(0.0, 0.0, z_heights[0]));
    let top_center = verts.len() as u32;
    verts.push(Point3::new(0.0, 0.0, *z_heights.last().unwrap()));

    let mut tris = Vec::new();
    let mut tri_face = Vec::new();
    for b in 0..(nb - 1) {
        let row0 = (b * n_seg) as u32;
        let row1 = ((b + 1) * n_seg) as u32;
        for i in 0..n_seg {
            let i_next = (i + 1) % n_seg;
            let (v00, v10) = (row0 + i as u32, row0 + i_next as u32);
            let (v01, v11) = (row1 + i as u32, row1 + i_next as u32);
            tris.push([v00, v10, v11]);
            tri_face.push(0);
            tris.push([v00, v11, v01]);
            tri_face.push(0);
        }
    }
    // Bottom cap (face 1) & top cap (face 2).
    for i in 0..n_seg {
        let i_next = (i + 1) % n_seg;
        tris.push([bot_center, i_next as u32, i as u32]);
        tri_face.push(1);
        let top_row = ((nb - 1) * n_seg) as u32;
        tris.push([top_center, top_row + i as u32, top_row + i_next as u32]);
        tri_face.push(2);
    }

    let faces = vec![
        dummy_face(Surface::Cone {
            apex: Point3::new(0.0, 0.0, apex_z),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            half_angle,
        }),
        dummy_face(Surface::Plane {
            normal: Vector3::new(0.0, 0.0, -1.0),
            d: z_heights[0],
        }),
        dummy_face(Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: -*z_heights.last().unwrap(),
        }),
    ];
    (Mesh { verts, tris }, tri_face, faces)
}

#[test]
fn contact_pairs_prunes_interior_disjoint_gauss_area_without_leaf_refinement() {
    // 1. Coaxial non-intersecting cone frustums (`z ≥ 1.20`, above the `z = 1.143`
    //    root of `z tan 45° = (z + 2) tan 20°`, so `r_A(z) > r_B(z)` everywhere and
    //    `intersecting == 0`). Cone A (`α = 45°`, normals at `135°/45°` to `ẑ`) and
    //    Cone B (`α = 20°`, normals at `110°/70°` to `ẑ`) have strictly disjoint
    //    Gauss maps (`25°` zone gap), while on the interior middle band (`[1.32, 1.40]`,
    //    zero trim-boundary edges) their `n_seg = 8` chords sit within `reach ≈ 0.16`.
    //    §4.2.2 must prune every interior candidate with ZERO calls to `pieces_cross_point`.
    let (mesh_a, tf_a, faces_a) =
        closed_subdivided_cone_frustum(0.0, 45.0_f64.to_radians(), &[1.20, 1.30, 1.42, 2.40], 8);
    let (mesh_b, tf_b, faces_b) =
        closed_subdivided_cone_frustum(-2.0, 20.0_f64.to_radians(), &[1.25, 1.32, 1.40, 2.15], 8);

    let in_mid_band = |t: u32| (16..32).contains(&t);
    let (contacts, stats) = contact_pairs_with_stats(
        &mesh_a,
        &tf_a,
        &faces_a,
        &mesh_b,
        &tf_b,
        &faces_b,
        &|ta, tb| !(in_mid_band(ta) && in_mid_band(tb)),
    )
    .expect("valid Stage-1 lineage");

    assert_eq!(
        stats.intersecting, 0,
        "frustums are strictly disjoint: {stats:?}"
    );
    assert!(
        contacts.is_empty(),
        "disjoint Gauss maps cannot intersect: {contacts:?}"
    );
    assert!(
        stats.gauss_rejected > 0,
        "expected interior disjoint-Gauss pairs to be pruned by §4.2.2, got {stats:?}"
    );
    assert_eq!(
        stats.refined, 0,
        "expected zero pieces_cross_point refinements after §4.2.2 pruning, got {stats:?}"
    );

    // 2. Ring-2 pruning beside a real mesh intersection (`z = 0.80` crosses in band 0,
    //    band 1 `[1.24, 1.32]` is Ring 1 skipped by `RingCapture`, band 2 `[1.32, 1.40]`
    //    is Ring 2 strictly interior and separated from the crossing):
    let (mesh_a2, tf_a2, faces_a2) = closed_subdivided_cone_frustum(
        0.0,
        45.0_f64.to_radians(),
        &[0.80, 1.24, 1.32, 1.40, 2.20],
        8,
    );
    let (mesh_b2, tf_b2, faces_b2) = closed_subdivided_cone_frustum(
        -2.0,
        20.0_f64.to_radians(),
        &[0.50, 1.22, 1.30, 1.42, 2.40],
        8,
    );
    let in_ring2_band = |t: u32| (32..48).contains(&t);
    let (contacts2, stats2) = contact_pairs_with_stats(
        &mesh_a2,
        &tf_a2,
        &faces_a2,
        &mesh_b2,
        &tf_b2,
        &faces_b2,
        &|ta, tb| !(in_ring2_band(ta) && in_ring2_band(tb)),
    )
    .expect("valid Stage-1 lineage");
    assert!(stats2.intersecting > 0 && contacts2.is_empty());
    assert!(
        stats2.gauss_rejected > 0 && stats2.refined == 0,
        "expected Ring-2 beside crossing to be pruned by §4.2.2, got {stats2:?}"
    );
}

/// Build a closed UV sphere mesh (`n_lat` bands × `n_lon` segments) on a single
/// `Surface::Sphere` face (face 0, zero trim-boundary edges anywhere!).
fn closed_uv_sphere(
    center: [f64; 3],
    radius: f64,
    n_lat: usize,
    n_lon: usize,
) -> (Mesh, Vec<u32>, Vec<BRepFace>) {
    assert!(n_lat >= 4 && n_lon >= 4);
    let mut verts = Vec::new();
    // South pole (0), intermediate rings, north pole (last).
    verts.push(Point3::new(center[0], center[1], center[2] - radius));
    for ilat in 1..n_lat {
        let v = -FRAC_PI_2 + PI * (ilat as f64) / (n_lat as f64);
        let (cv, sv) = (v.cos(), v.sin());
        for ilon in 0..n_lon {
            let u = TAU * (ilon as f64) / (n_lon as f64);
            verts.push(Point3::new(
                center[0] + radius * cv * u.cos(),
                center[1] + radius * cv * u.sin(),
                center[2] + radius * sv,
            ));
        }
    }
    let north_idx = verts.len() as u32;
    verts.push(Point3::new(center[0], center[1], center[2] + radius));

    let mut tris = Vec::new();
    // South cap fan.
    for i in 0..n_lon {
        let i_next = (i + 1) % n_lon;
        tris.push([0, 1 + i_next as u32, 1 + i as u32]);
    }
    // Quad bands.
    for ilat in 0..(n_lat - 2) {
        let r0 = 1 + (ilat * n_lon) as u32;
        let r1 = 1 + ((ilat + 1) * n_lon) as u32;
        for i in 0..n_lon {
            let i_next = (i + 1) % n_lon;
            let (v00, v10) = (r0 + i as u32, r0 + i_next as u32);
            let (v01, v11) = (r1 + i as u32, r1 + i_next as u32);
            tris.push([v00, v10, v11]);
            tris.push([v00, v11, v01]);
        }
    }
    // North cap fan.
    let top_ring = 1 + ((n_lat - 2) * n_lon) as u32;
    for i in 0..n_lon {
        let i_next = (i + 1) % n_lon;
        tris.push([top_ring + i as u32, top_ring + i_next as u32, north_idx]);
    }

    let tri_face = vec![0u32; tris.len()];
    let faces = vec![dummy_face(Surface::Sphere {
        center: p3(center),
        radius,
    })];
    (Mesh { verts, tris }, tri_face, faces)
}

#[test]
fn contact_pairs_preserves_multi_triangle_interior_small_loop() {
    // Two unit spheres kissing along the `+x` equator (`u_A = 0` vs `u_B = π`,
    // rotated by half a segment `π / n_lon` so chord midpoints face each other
    // and the coarse meshes stay strictly disjoint while the exact spheres
    // overlap by `0.004`, spanning a 4-triangle interior area with zero trim
    // boundaries). The antipodal normals (`+x` vs `-x`) are collinear, so
    // §4.2.2 MUST preserve the component and `pieces_cross_point` MUST report
    // the missed small loop.
    let n_lat = 10;
    let n_lon = 12;
    let overlap = 0.004;
    let half_seg = PI / (n_lon as f64);
    let d = 2.0 - overlap;
    let (mesh_a, tf_a, faces_a) = closed_uv_sphere([0.0, 0.0, 0.0], 1.0, n_lat, n_lon);
    let (mesh_b, tf_b, faces_b) = closed_uv_sphere(
        [d * half_seg.cos(), d * half_seg.sin(), 0.0],
        1.0,
        n_lat,
        n_lon,
    );

    let (contacts, _stats) = contact_pairs_with_stats(
        &mesh_a,
        &tf_a,
        &faces_a,
        &mesh_b,
        &tf_b,
        &faces_b,
        &|_, _| false,
    )
    .expect("valid Stage-1 lineage");

    assert!(
        !contacts.is_empty(),
        "interior small intersection loop between two spheres must be preserved"
    );
}

#[test]
fn contact_pairs_preserves_trim_clipped_graze_and_filters_parallel_non_crossing() {
    // 1. Sub-sagitta cylinder graze clipped by cap rims (`grazing_pair(0.005)`):
    //    must still emit lateral contacts.
    let (c15, s15) = (15.0_f64.to_radians().cos(), 15.0_f64.to_radians().sin());
    let d_graze = 2.0 - 0.005;
    let cyl_a = rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 2.0);
    let cyl_b = rj_cylinder(
        [d_graze * c15, d_graze * s15, 0.3],
        [0.0, 0.0, 1.0],
        1.0,
        2.0,
    );
    let (contacts, _) = contact_pairs_with_stats(
        cyl_a.as_mesh(),
        cyl_a.tri_face(),
        cyl_a.faces(),
        cyl_b.as_mesh(),
        cyl_b.tri_face(),
        cyl_b.faces(),
        &|_, _| false,
    )
    .expect("valid lineage");
    assert!(
        contacts
            .iter()
            .any(|c| cyl_a.tri_face()[c.tri_a as usize] == 0
                && cyl_b.tri_face()[c.tri_b as usize] == 0),
        "lateral graze must be preserved: {contacts:?}"
    );

    // 2. Parallel non-intersecting cylinders within `2d_ε` (`d = 2.002` along `+x`
    //    so inscribed polygon vertices face each other at `d_coarse = 0.002 <= reach ≈ 0.017`,
    //    while the exact cylinders stay `0.002` apart): Gauss maps overlap (`+x̂` vs `-x̂`),
    //    so `pieces_cross_point` is the necessary second stage that rejects the
    //    non-intersecting pair (`contacts.is_empty()`, proving `pieces_cross_point`
    //    is NOT redundant!).
    let cyl_b_far = rj_cylinder([2.002, 0.0, 0.3], [0.0, 0.0, 1.0], 1.0, 2.0);
    let (disjoint_contacts, disjoint_stats) = contact_pairs_with_stats(
        cyl_a.as_mesh(),
        cyl_a.tri_face(),
        cyl_a.faces(),
        cyl_b_far.as_mesh(),
        cyl_b_far.tri_face(),
        cyl_b_far.faces(),
        &|_, _| false,
    )
    .expect("valid lineage");
    assert!(
        disjoint_stats.refined > 0 && disjoint_contacts.is_empty(),
        "parallel facing cylinders within 2d_ε pass Gauss-map overlap and rely on pieces_cross_point: {disjoint_stats:?}"
    );
}
