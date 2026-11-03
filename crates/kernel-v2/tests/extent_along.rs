//! `extent::extent_along`: a solid's extent along a direction is its
//! SUPPORT interval — vertices, edge-curve critical points, and the
//! sphere / torus / cone-apex face-interior critical points that lie in the
//! face — never the B-Rep vertices alone (prospector seed 6, 2026-11-03: a
//! cylinder's two seam vertices reversed a direction-less cut).

use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::{
    boolean_op, extent_along, extrude, revolve, vertex_extent_along, BrepArena, Profile,
};

fn unit(d: [f64; 3]) -> [f64; 3] {
    let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    [d[0] / n, d[1] / n, d[2] / n]
}

fn assert_interval(got: Option<(f64, f64)>, want: (f64, f64), tol: f64, what: &str) {
    let (lo, hi) = got.unwrap_or_else(|| panic!("{what}: no extent"));
    assert!(
        (lo - want.0).abs() <= tol && (hi - want.1).abs() <= tol,
        "{what}: got [{lo:.9}, {hi:.9}], want [{:.9}, {:.9}] (tol {tol:e})",
        want.0,
        want.1
    );
}

/// Cylinder of radius `r` about the z-axis, z ∈ [0, h].
fn cylinder(arena: &mut BrepArena, r: f64, h: f64) -> kernel_v2::SolidId {
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        r,
    )
    .expect("circle profile");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), h)
        .expect("cylinder")
        .solid
}

/// Axis-aligned box `[x0, x1] × [y0, y1] × [z0, z1]`.
fn cuboid(arena: &mut BrepArena, lo: [f64; 3], hi: [f64; 3]) -> kernel_v2::SolidId {
    let profile = Profile::new(
        Point3::new(0.0, 0.0, lo[2]),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        vec![
            Point2::new(lo[0], lo[1]),
            Point2::new(hi[0], lo[1]),
            Point2::new(hi[0], hi[1]),
            Point2::new(lo[0], hi[1]),
        ],
        vec![],
    )
    .expect("rectangle profile");
    extrude(arena, &profile, Vector3::new(0.0, 0.0, 1.0), hi[2] - lo[2])
        .expect("box")
        .solid
}

#[test]
fn a_cylinders_extent_is_its_rims_not_its_seam() {
    let mut arena = BrepArena::new();
    let s = cylinder(&mut arena, 1.0, 2.0);
    let d = unit([1.0, 0.0, 1.0]);
    // p · d over r = 1, z ∈ [0, 2]: (x + z)/√2 with x ∈ [-1, 1].
    let want = (-1.0 / 2f64.sqrt(), 3.0 / 2f64.sqrt());
    assert_interval(extent_along(&arena, s, d).unwrap(), want, 1e-12, "cylinder");
    // The vertex reading — what the engine used — sees only the seam (at
    // x = +1), so it misses the far side of the rims by √2.
    let (vlo, _) = vertex_extent_along(&arena, s, d).unwrap().unwrap();
    assert!(
        (vlo - (1.0 / 2f64.sqrt())).abs() < 1e-12,
        "seam-only vertex reading starts at {vlo}, not the rims' {}",
        want.0
    );
}

#[test]
fn a_box_extent_equals_its_vertex_extent() {
    let mut arena = BrepArena::new();
    let s = cuboid(&mut arena, [-1.0, -2.0, 0.0], [3.0, 1.0, 0.5]);
    for d in [[1.0, 0.0, 0.0], [0.3, -0.7, 0.2], [0.0, 0.0, -1.0]] {
        assert_eq!(
            extent_along(&arena, s, d).unwrap(),
            vertex_extent_along(&arena, s, d).unwrap(),
            "polyhedron along {d:?}"
        );
    }
}

#[test]
fn a_spheres_extent_is_its_center_plus_minus_radius() {
    let mut arena = BrepArena::new();
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(5.0, 0.0),
        1.0,
    )
    .expect("on-axis circle");
    let s = revolve(
        &mut arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("closed sphere")
    .solid;
    for d in [[0.0, 0.0, 1.0], [0.6, 0.0, 0.8], [1.0, 1.0, 1.0]] {
        let d = unit(d);
        let c = 5.0 * d[0];
        assert_interval(
            extent_along(&arena, s, d).unwrap(),
            (c - 1.0, c + 1.0),
            1e-12,
            "sphere",
        );
    }
}

#[test]
fn a_cut_sphere_excludes_the_pole_it_lost() {
    // Sphere (5, 0, 0), r = 1, minus everything below z = 0: the north pole
    // stays (an interior critical point IN the face), the south pole is gone
    // (outside the face — the cut disc bounds the extent at 0).
    let mut arena = BrepArena::new();
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(5.0, 0.0),
        1.0,
    )
    .expect("on-axis circle");
    let sphere = revolve(
        &mut arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("closed sphere")
    .solid;
    let cutter = cuboid(&mut arena, [2.0, -3.0, -3.0], [8.0, 3.0, 0.0]);
    let dome = boolean_op(&mut arena, sphere, cutter, BoolOp::Subtract).expect("dome");
    assert_interval(
        extent_along(&arena, dome, [0.0, 0.0, 1.0]).unwrap(),
        (0.0, 1.0),
        1e-12,
        "dome along +z",
    );
    assert_interval(
        extent_along(&arena, dome, [0.0, 0.0, -1.0]).unwrap(),
        (-1.0, 0.0),
        1e-12,
        "dome along -z",
    );
}

#[test]
fn a_half_cylinders_arcs_keep_only_their_own_critical_point() {
    // r = 1 cylinder minus x < 0: its rims are ARCS spanning x ≥ 0. Along -x
    // the far critical point (-1, 0, z) is not on them; the flat cut face
    // bounds the extent at 0.
    let mut arena = BrepArena::new();
    let cyl = cylinder(&mut arena, 1.0, 2.0);
    let cutter = cuboid(&mut arena, [-2.0, -2.0, -1.0], [0.0, 2.0, 3.0]);
    let half = boolean_op(&mut arena, cyl, cutter, BoolOp::Subtract).expect("half cylinder");
    assert_interval(
        extent_along(&arena, half, [-1.0, 0.0, 0.0]).unwrap(),
        (-1.0, 0.0),
        1e-12,
        "half cylinder along -x",
    );
    let d = unit([0.0, 1.0, 1.0]);
    assert_interval(
        extent_along(&arena, half, d).unwrap(),
        (-1.0 / 2f64.sqrt(), 3.0 / 2f64.sqrt()),
        1e-12,
        "half cylinder along (y + z)/√2",
    );
}

#[test]
fn a_torus_extent_is_its_outer_equator_off_axis() {
    // R = 3, r = 1 about the x-axis through the origin.
    let mut arena = BrepArena::new();
    let profile = Profile::circle(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 3.0),
        1.0,
    )
    .expect("circle profile");
    let s = revolve(
        &mut arena,
        &profile,
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        2.0 * PI,
    )
    .expect("closed torus")
    .solid;
    assert_interval(
        extent_along(&arena, s, [0.0, 0.0, 1.0]).unwrap(),
        (-4.0, 4.0),
        1e-12,
        "torus across its axis",
    );
    let d = unit([1.0, 0.0, 1.0]);
    // Critical points c ± R ê ± r d̂ with ê = z: values ±R/√2 ± r.
    let m = 3.0 / 2f64.sqrt() + 1.0;
    assert_interval(
        extent_along(&arena, s, d).unwrap(),
        (-m, m),
        1e-12,
        "torus oblique",
    );
    // Along the axis the critical set is two circles; the face's render
    // vertices stand in for them (on the surface, within its sagitta band).
    assert_interval(
        extent_along(&arena, s, [1.0, 0.0, 0.0]).unwrap(),
        (-1.0, 1.0),
        1e-3 * 4.0,
        "torus along its axis",
    );
}
