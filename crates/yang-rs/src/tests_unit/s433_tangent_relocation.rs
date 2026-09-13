//! Stage-4 cylinder×CYLINDER relocation at TANGENCY grade — the move guard
//! (spec `specs/yang_433_tangent_point_mesh_update.md` §5).
//!
//! Two cylinders of the SAME radius whose axes intersect touch their surfaces
//! at two isolated points. There the two surface normals are collinear, so
//! `cyl_cyl_point_amplification` (the `1/sin α` gradient metric) diverges and
//! the residual gate becomes `f64::INFINITY`. That gate bounds the vertex's
//! RESIDUAL to the exact section; it says nothing about how far the relocation
//! MOVES the vertex — and the azimuth projection
//! ([`project_onto_ellipse_via_cylinder`]) preserves the cylinder azimuth, so
//! near a tangency it slides the vertex an unbounded distance ALONG the
//! section.
//!
//! Pinned on the C0058 measurement (2026-09-13, `YANG_STAR_PROBE`): A is the
//! r = 0.4 cylinder on +ẑ through the origin, B the r = 0.4 cylinder whose axis
//! runs (0.5, 0, √3/2) through (0, 0, 1); the surfaces are tangent at
//! (0, ±0.4, 1). Mesh vertex v33 sat at (0.12191459396982109,
//! −0.36038754716096766, 1.0370674791033905) — 0.1334 from the tangency, on
//! A's own facet plane. The azimuth projection sent it **0.4427** away, onto a
//! point where two other vertices already sat; the in-plane nearest point is
//! **0.1151** away and lands 0.0678 from the tangency. All three readings are
//! reproduced below from the geometry, not transcribed.

#[allow(unused_imports)]
use super::*;

use crate::stage4_relocate::{
    cyl_cyl_point_amplification, project_onto_ellipse_nearest, project_onto_ellipse_via_cylinder,
    EllipseReloc,
};

const R: f64 = 0.4;
/// A's axis: +ẑ through the origin. B's axis: through (0, 0, 1), 30° off +ẑ.
fn b_axis_dir() -> [f64; 3] {
    let beta: f64 = std::f64::consts::FRAC_PI_6;
    [beta.sin(), 0.0, beta.cos()]
}

/// The measured pre-relocation position of C0058's v33.
fn v33() -> Point3 {
    Point3::new(
        0.121_914_593_969_821_09,
        -0.360_387_547_160_967_66,
        1.037_067_479_103_390_5,
    )
}

/// The two cylinders' exact surface-tangency points, `(0, ±R, 1)`: the axes
/// meet at (0, 0, 1) and the common normal direction is `û × v̂` normalized.
fn tangency() -> Point3 {
    Point3::new(0.0, -R, 1.0)
}

/// The steep section ellipse through the tangency (Yang's `E1`): for two
/// equal-R cylinders whose axes cross at half-angle β, the intersection
/// decomposes into the two PLANE sections `z = 1 ± k·x` with
/// `k₊ = sinβ/(1 − cosβ)` and `k₋ = −sinβ/(1 + cosβ)`. `E1` is `k₊`.
fn e1_reloc() -> EllipseReloc {
    let beta: f64 = std::f64::consts::FRAC_PI_6;
    let k = beta.sin() / (1.0 - beta.cos());
    // Plane `k·x − z + 1 = 0`.
    let n_raw = [k, 0.0, -1.0];
    let n_len = (n_raw[0] * n_raw[0] + n_raw[2] * n_raw[2]).sqrt();
    let n = [n_raw[0] / n_len, 0.0, n_raw[2] / n_len];
    let d = 1.0 / n_len;
    let a_hat = [0.0, 0.0, 1.0];
    let n_dot_a = n[0] * a_hat[0] + n[1] * a_hat[1] + n[2] * a_hat[2];
    // Section frame: the plane meets A's axis at (0, 0, 1); the major axis is
    // the in-plane steepest direction; semi-minor = R, semi-major = R/|n̂·â|.
    let maj_raw = [
        a_hat[0] - n_dot_a * n[0],
        a_hat[1] - n_dot_a * n[1],
        a_hat[2] - n_dot_a * n[2],
    ];
    let maj_len =
        (maj_raw[0] * maj_raw[0] + maj_raw[1] * maj_raw[1] + maj_raw[2] * maj_raw[2]).sqrt();
    let b_dir = b_axis_dir();
    EllipseReloc {
        axis_point: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        radius: R,
        plane_n: Vector3::new(n[0], n[1], n[2]),
        plane_d: d,
        center: Point3::new(0.0, 0.0, 1.0),
        normal: Vector3::new(n[0], n[1], n[2]),
        major_axis: Vector3::new(
            maj_raw[0] / maj_len,
            maj_raw[1] / maj_len,
            maj_raw[2] / maj_len,
        ),
        major_radius: R / n_dot_a.abs(),
        minor_radius: R,
        second_cyl: Some((
            Point3::new(0.0, 0.0, 1.0),
            Vector3::new(b_dir[0], b_dir[1], b_dir[2]),
            // The combined Stage-1 chord budget of the two 10-segment r=0.4
            // laterals: 2·R·(1 − cos(π/10)).
            2.0 * R * (1.0 - (std::f64::consts::PI / 10.0).cos()),
        )),
    }
}

fn dist(p: Point3, q: Point3) -> f64 {
    ((p.x() - q.x()).powi(2) + (p.y() - q.y()).powi(2) + (p.z() - q.z()).powi(2)).sqrt()
}

/// The fixture really is the C0058 configuration: v33 lies within a sixth of a
/// radius of the exact tangency, and the tangency is ON the section ellipse
/// (both sections pass through it — that is what makes it a junction).
#[test]
pub(crate) fn s433_fixture_is_the_c0058_tangency() {
    let er = e1_reloc();
    assert!(
        (dist(v33(), tangency()) - 0.133_440_3).abs() < 1e-6,
        "v33 stands {} from the tangency",
        dist(v33(), tangency())
    );
    // The tangency is on A's cylinder and in E1's plane.
    let t = tangency().as_array();
    let radial = (t[0] * t[0] + t[1] * t[1]).sqrt();
    assert!((radial - R).abs() < 1e-15, "tangency off A's cylinder");
    let n = er.plane_n.as_array();
    let h = n[0] * t[0] + n[1] * t[1] + n[2] * t[2] + er.plane_d;
    assert!(h.abs() < 1e-15, "tangency off E1's plane: {h:.3e}");
}

/// The residual gate is UNBOUNDED here — the tangency-grade `1/sin α` blow-up.
/// This is the reading that made the old `er.second_cyl.is_some() ||`
/// short-circuit unsafe: there is no finite band left to bound the move with.
#[test]
pub(crate) fn s433_tangency_amplification_gate_is_unbounded_near_the_tangency() {
    let er = e1_reloc();
    let (ap2, ad2, budget) = er.second_cyl.expect("cyl×cyl fixture");
    let at_tangency =
        cyl_cyl_point_amplification(tangency(), (er.axis_point, er.axis_dir), (ap2, ad2))
            .map_or(f64::INFINITY, |amp| amp * budget);
    assert!(
        !at_tangency.is_finite() || at_tangency > 1.0e3 * budget,
        "gate at the tangency point should blow up, got {at_tangency:.3e}"
    );
    // A vertex 0.13 away still reads a gate far above its own chord budget.
    let near = cyl_cyl_point_amplification(v33(), (er.axis_point, er.axis_dir), (ap2, ad2))
        .map_or(f64::INFINITY, |amp| amp * budget);
    assert!(
        near > budget,
        "amplified gate {near:.3e} should exceed the raw budget {budget:.3e}"
    );
}

/// The defect the guard closes: the azimuth projection's MOVE is an order of
/// magnitude larger than the in-plane nearest point's, and larger than the
/// amplified gate — so the move check (now applied to the cyl×cyl arm too)
/// rejects it and the nearest point is taken.
#[test]
pub(crate) fn s433_azimuth_move_exceeds_the_gate_and_nearest_does_not() {
    let er = e1_reloc();
    let (ap2, ad2, budget) = er.second_cyl.expect("cyl×cyl fixture");
    let gate = cyl_cyl_point_amplification(v33(), (er.axis_point, er.axis_dir), (ap2, ad2))
        .map_or(f64::INFINITY, |amp| amp * budget);
    let (az, _) = project_onto_ellipse_via_cylinder(v33(), &er).expect("azimuth projection");
    let (near, _) = project_onto_ellipse_nearest(v33(), &er).expect("nearest projection");
    let (az_move, near_move) = (dist(v33(), az), dist(v33(), near));

    assert!(
        (az_move - 0.442_7).abs() < 1e-3,
        "azimuth move {az_move:.6} (measured 0.4427 on C0058)"
    );
    assert!(
        (near_move - 0.115_06).abs() < 1e-3,
        "nearest move {near_move:.6} (measured 0.11506 on C0058)"
    );
    assert!(
        az_move > gate,
        "the azimuth move {az_move:.3e} must FAIL the gate {gate:.3e} — that is \
         what routes the cyl×cyl arm to the nearest point"
    );
    assert!(
        near_move < az_move / 3.0,
        "nearest {near_move:.3e} must be far below azimuth {az_move:.3e}"
    );
    // Both land ON the exact ellipse (the guard changes WHICH exact point is
    // taken, never whether the vertex ends up on the curve).
    for (name, q) in [("azimuth", az), ("nearest", near)] {
        let a = q.as_array();
        let n = er.plane_n.as_array();
        let h = n[0] * a[0] + n[1] * a[1] + n[2] * a[2] + er.plane_d;
        let radial = (a[0] * a[0] + a[1] * a[1]).sqrt();
        assert!(h.abs() < 1e-12, "{name} off the section plane: {h:.3e}");
        assert!(
            (radial - R).abs() < 1e-12,
            "{name} off A's cylinder: {radial}"
        );
    }
}

/// The nearest point near a tangency lands NEAR the tangent point but not ON
/// it — the residue that keeps C0058 walled (spec §6): a relocation cannot
/// create the mesh CROSSING the exact geometry has, only move vertices onto
/// curves the mesh already separates.
#[test]
pub(crate) fn s433_nearest_point_does_not_reach_the_tangent_point() {
    let er = e1_reloc();
    let (near, _) = project_onto_ellipse_nearest(v33(), &er).expect("nearest projection");
    let gap = dist(near, tangency());
    assert!(
        (gap - 0.067_8).abs() < 1e-3,
        "the nearest point stands {gap:.6} from the tangency (measured 0.0678)"
    );
    assert!(
        gap > 1e-3,
        "the in-plane nearest point is not the tangent point (gap {gap:.3e}); \
         if this ever becomes 0 the §4.3.3 insertion story changes"
    );
    assert!(
        gap < dist(v33(), tangency()),
        "but it does move TOWARD the tangency ({gap:.3e} < {:.3e})",
        dist(v33(), tangency())
    );
}
