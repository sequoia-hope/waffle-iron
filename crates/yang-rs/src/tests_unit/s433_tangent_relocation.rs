//! Yang §4.3.3 tangency: the exact cylinder×cylinder tangent-point closed form
//! (§6, the Stage-1 mint) and the Stage-4 relocation move guard (§5) — spec
//! `specs/yang_433_tangent_point_mesh_update.md`.
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

// =========================================================================
// The §4.3.3 closed form (spec §6): `cyl_cyl_tangent_points`. Two cylinders
// are tangent where their (radial) normals are collinear, so the shared
// direction is `m = ±(û × v̂)/|û × v̂|` and the pair is tangent iff
// `s_A·R_A − s_B·R_B = δ`, the signed axis offset along `m`. Every expectation
// below is derived from the configuration, not transcribed.
// =========================================================================

use crate::boolean::cyl_cyl_tangent_points;

fn cyl(p: [f64; 3], d: [f64; 3], r: f64) -> (Point3, Vector3, f64) {
    (
        Point3::new(p[0], p[1], p[2]),
        Vector3::new(d[0], d[1], d[2]),
        r,
    )
}

/// C0058: equal radii, axes crossing at (0,0,1) at 30°. The common normal is
/// `û × v̂ = ŷ`, so the two tangency points are `(0, ±R, 1)` — measured in the
/// corpus mesh at exactly those coordinates.
#[test]
pub(crate) fn s433_equal_radius_crossing_axes_give_two_tangent_points() {
    let beta: f64 = std::f64::consts::FRAC_PI_6;
    let a = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], R);
    let b = cyl([0.0, 0.0, 1.0], [beta.sin(), 0.0, beta.cos()], R);
    let pts = cyl_cyl_tangent_points(a, b).expect("non-parallel axes");
    assert_eq!(pts.len(), 2, "got {pts:?}");
    for p in &pts {
        assert!(p.x().abs() < 1e-15 && (p.z() - 1.0).abs() < 1e-15, "{p:?}");
    }
    let mut ys: Vec<f64> = pts.iter().map(Point3::y).collect();
    ys.sort_by(f64::total_cmp);
    assert!(
        (ys[0] + R).abs() < 1e-15 && (ys[1] - R).abs() < 1e-15,
        "{ys:?}"
    );
}

/// F0058's shape: the same configuration at 90°. Still two points, still on the
/// common normal — the angle between the axes does not move them.
#[test]
pub(crate) fn s433_perpendicular_equal_radius_gives_two_tangent_points() {
    let a = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.2);
    let b = cyl([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.2);
    let pts = cyl_cyl_tangent_points(a, b).expect("non-parallel axes");
    assert_eq!(pts.len(), 2, "got {pts:?}");
    for p in &pts {
        assert!(
            p.x().abs() < 1e-15 && p.z().abs() < 1e-15 && (p.y().abs() - 0.2).abs() < 1e-15,
            "{p:?}"
        );
    }
}

/// UNEQUAL radii with INTERSECTING axes are NOT tangent: `δ = 0` needs
/// `s_A·R_A = s_B·R_B`, impossible for `R_A ≠ R_B`. This is the gate that keeps
/// the mint off every ordinary crossing-cylinder pair in the corpus.
#[test]
pub(crate) fn s433_unequal_radius_crossing_axes_are_not_tangent() {
    let a = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.4);
    let b = cyl([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], 0.25);
    let pts = cyl_cyl_tangent_points(a, b).expect("non-parallel axes");
    assert!(pts.is_empty(), "got {pts:?}");
}

/// Unequal radii DO touch when the perpendicular axis offset equals `R_A − R_B`
/// exactly (the R0050 certificate's shape, at cylinder grade): one tangency,
/// on the common normal at the larger radius.
#[test]
pub(crate) fn s433_offset_axes_touch_when_offset_equals_the_radius_difference() {
    let (ra, rb) = (0.4f64, 0.25f64);
    let a = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], ra);
    // û × v̂ = ŷ for v̂ = x̂; offset B's axis along +ŷ by exactly R_A − R_B.
    let b = cyl([0.0, ra - rb, 1.0], [1.0, 0.0, 0.0], rb);
    let pts = cyl_cyl_tangent_points(a, b).expect("non-parallel axes");
    assert_eq!(pts.len(), 1, "got {pts:?}");
    let p = pts[0];
    assert!(
        p.x().abs() < 1e-15 && (p.y() - ra).abs() < 1e-15 && (p.z() - 1.0).abs() < 1e-15,
        "{p:?}"
    );
}

/// A NEAR-tangency is not a tangency: perturbing the offset by 1e-6 — far above
/// the `TAU_WORK·(1+scale)` rounding band, far below `TAU_MODEL`-scale
/// features — yields no mint. Fusing this would be the R0053 error.
#[test]
pub(crate) fn s433_near_tangency_beyond_the_rounding_band_is_refused() {
    let (ra, rb) = (0.4f64, 0.25f64);
    let a = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], ra);
    let b = cyl([0.0, ra - rb + 1e-6, 1.0], [1.0, 0.0, 0.0], rb);
    let pts = cyl_cyl_tangent_points(a, b).expect("non-parallel axes");
    assert!(pts.is_empty(), "got {pts:?}");
}

/// PARALLEL axes are tangent along a whole GENERATOR, not at isolated points —
/// `None` from the POINT form; the generator form below owns them.
#[test]
pub(crate) fn s433_parallel_axes_return_none() {
    let a = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.3);
    let b = cyl([0.6, 0.0, 0.0], [0.0, 0.0, 1.0], 0.3);
    assert!(cyl_cyl_tangent_points(a, b).is_none());
}

// =========================================================================
// §11 (2026-09-17): the GENERATOR arm — parallel axes tangent along a line.
// `m` is the unit perpendicular from A's axis to B's, `δ = |w⊥|`, and the
// point form's identity `s_A·R_A − s_B·R_B = δ` selects the sign pair; the
// line is `a + s_A·R_A·m + t·û`. Every expectation is derived from the
// configuration.
// =========================================================================

use crate::boolean::cyl_cyl_tangent_generator;

/// C0056: A = cylinder r 1 on the z-axis, B = cylinder r 0.5 on the axis
/// through (0.5, 0, 1.4) pointing DOWN. `δ = 0.5 = R_A − R_B` — internal
/// contact, `(+,+)`, along the generator x = 1, y = 0. The foot is exactly
/// (1, 0, 0) (A's axis point plus R_A·x̂, both exact) and the axis is A's own
/// unit axis, regardless of B's antiparallel direction.
#[test]
pub(crate) fn s433_generator_internal_contact_is_the_outer_radius_foot() {
    let a = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
    let b = cyl([0.5, 0.0, 1.4], [-0.0, -0.0, -1.0], 0.5);
    let (p0, u) = cyl_cyl_tangent_generator(a, b).expect("internally tangent");
    assert_eq!(p0.as_array(), [1.0, 0.0, 0.0], "{p0:?}");
    assert_eq!(u, [0.0, 0.0, 1.0]);
}

/// External contact: two r = 0.3 cylinders whose axes are 0.6 apart touch
/// along x = 0.3 — `δ = R_A + R_B`, sign pair `(+,−)`, foot `a + R_A·m`.
#[test]
pub(crate) fn s433_generator_external_contact_between_the_axes() {
    let a = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.3);
    let b = cyl([0.6, 0.0, 0.0], [0.0, 0.0, 1.0], 0.3);
    let (p0, u) = cyl_cyl_tangent_generator(a, b).expect("externally tangent");
    assert!(
        (p0.x() - 0.3).abs() < 1e-15 && p0.y().abs() < 1e-15 && p0.z().abs() < 1e-15,
        "{p0:?}"
    );
    assert_eq!(u, [0.0, 0.0, 1.0]);
}

/// A inside B: A is r 0.25 at the origin, B is r 0.4 centred 0.15 along +x
/// (`δ = R_B − R_A`, sign pair `(−,−)`). The contact is on A's FAR side,
/// `a − R_A·m = (−0.25, 0, 0)` — which is also `b − R_B·m`.
#[test]
pub(crate) fn s433_generator_inner_operand_touches_on_its_far_side() {
    let a = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.25);
    let b = cyl([0.15, 0.0, 0.0], [0.0, 0.0, 1.0], 0.4);
    let (p0, _) = cyl_cyl_tangent_generator(a, b).expect("A inside B, tangent");
    assert!(
        (p0.x() + 0.25).abs() < 1e-15 && p0.y().abs() < 1e-15,
        "{p0:?}"
    );
    assert!(
        (0.15 - 0.4 - p0.x()).abs() < 1e-15,
        "also B's far foot: {p0:?}"
    );
}

/// The axial offset between the two axis POINTS is irrelevant: B's axis
/// point 3.7 higher along the shared axis direction changes nothing but the
/// foot's height, which is A's axis point's.
#[test]
pub(crate) fn s433_generator_ignores_the_axial_offset_of_the_axis_points() {
    let a = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
    let b = cyl([0.5, 0.0, 3.7], [0.0, 0.0, 1.0], 0.5);
    let (p0, _) = cyl_cyl_tangent_generator(a, b).expect("still tangent");
    assert_eq!(p0.as_array(), [1.0, 0.0, 0.0], "{p0:?}");
}

/// A NEAR-tangency is not a tangency (the R0053 rule, as in the point form):
/// 1e-6 beyond the rounding band → `None`.
#[test]
pub(crate) fn s433_generator_near_tangency_beyond_the_rounding_band_is_refused() {
    let a = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
    let b = cyl([0.5 + 1e-6, 0.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    assert!(cyl_cyl_tangent_generator(a, b).is_none());
}

/// Coaxial cylinders have no generator contact (coincident or nested
/// surfaces), and crossing axes belong to the point form — both `None`.
#[test]
pub(crate) fn s433_generator_declines_coaxial_and_crossing_axes() {
    let a = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
    let coaxial = cyl([0.0, 0.0, 0.5], [0.0, 0.0, 1.0], 1.0);
    assert!(cyl_cyl_tangent_generator(a, coaxial).is_none());
    let crossing = cyl([0.0, 0.5, 1.0], [1.0, 0.0, 0.0], 0.5);
    assert!(cyl_cyl_tangent_generator(a, crossing).is_none());
    // …and the point form does own that crossing pair.
    assert!(cyl_cyl_tangent_points(a, crossing).is_some());
}

// =========================================================================
// §13 (2026-09-21): the CROSSING arm — parallel axes whose cross-section
// circles cross transversally, along two rulings `a + x·m ± y·n`,
// `x = (R_A² − R_B² + δ²)/(2δ)`, `y = √(R_A² − x²)`. Every expectation is
// derived from the configuration.
// =========================================================================

use crate::boolean::cyl_cyl_crossing_generators;

fn radial_distance(p: Point3, (ap, ad, _): (Point3, Vector3, f64)) -> f64 {
    let u = normalize3(ad.as_array());
    let w = [p.x() - ap.x(), p.y() - ap.y(), p.z() - ap.z()];
    let h = w[0] * u[0] + w[1] * u[1] + w[2] * u[2];
    let r = [w[0] - h * u[0], w[1] - h * u[1], w[2] - h * u[2]];
    (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt()
}

/// The `cyl_cyl_grazing_ruling_kv2` pair: A r 1 on the z-axis, B r 1.15
/// on the axis through (0.16, 0). Two feet, mirror images in y, each on
/// BOTH circles, and the crossing angle between the radial directions is
/// the grazing 2.98°.
#[test]
pub(crate) fn s433_crossing_rulings_lie_on_both_circles() {
    let a = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
    let b = cyl([0.16, 0.0, 0.0], [0.0, 0.0, 1.0], 1.15);
    let (feet, u) = cyl_cyl_crossing_generators(a, b).expect("transversal crossing");
    assert_eq!(u, [0.0, 0.0, 1.0]);
    let x: f64 = (1.0 - 1.15 * 1.15 + 0.16 * 0.16) / (2.0 * 0.16);
    let y = (1.0 - x * x).sqrt();
    assert!(
        (feet[0].x() - x).abs() < 1e-15 && (feet[0].y() - y).abs() < 1e-15,
        "{feet:?}"
    );
    assert!(
        (feet[1].x() - x).abs() < 1e-15 && (feet[1].y() + y).abs() < 1e-15,
        "{feet:?}"
    );
    for p in feet {
        assert!((radial_distance(p, a) - 1.0).abs() < 1e-15, "{p:?}");
        assert!((radial_distance(p, b) - 1.15).abs() < 1e-15, "{p:?}");
        assert!(p.z().abs() < 1e-15, "{p:?}");
    }
    let na = [x, y];
    let nb = [(x - 0.16) / 1.15, y / 1.15];
    let angle = (na[0] * nb[0] + na[1] * nb[1]).acos().to_degrees();
    assert!((angle - 2.975).abs() < 1e-2, "crossing angle {angle}");
}

/// R0038's REAL geometry (the corpus case's op 2, read off the document):
/// A's outer cylinder r 13.4185 and B's outer cylinder r 15.2175 on
/// parallel OBLIQUE axes 1.9303 apart. The vertex the Stage-4 STOP probe
/// printed on the collapsed chain, `(−2.5584, −5.8076, 13.2564)`, lies on
/// the returned ruling to 1e-12 — the case is a crossing ruling, not the
/// plane tangency §5c.10 recorded.
#[test]
pub(crate) fn s433_crossing_rulings_match_the_r0038_probe_vertex() {
    let axis = [0.40337748311323784, 0.9150336639256664, 0.0];
    let a = cyl(
        [-10.871657291983455, -6.2441717370226595, 2.0832269500797818],
        axis,
        13.418501040494824,
    );
    let b = cyl(
        [-11.206382487514404, -6.09661366104041, 0.18787820533635902],
        axis,
        15.217518737937626,
    );
    let (feet, u) = cyl_cyl_crossing_generators(a, b).expect("transversal crossing");
    let probe = Point3::new(-2.5584423736243105, -5.807640305735914, 13.256391723795382);
    let on_line = feet.iter().any(|p0| {
        let w = [probe.x() - p0.x(), probe.y() - p0.y(), probe.z() - p0.z()];
        let h = w[0] * u[0] + w[1] * u[1] + w[2] * u[2];
        let d = [w[0] - h * u[0], w[1] - h * u[1], w[2] - h * u[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() < 1e-12
    });
    assert!(
        on_line,
        "probe vertex off both rulings: feet {feet:?} u {u:?}"
    );
    for p in feet {
        assert!(
            (radial_distance(p, a) - 13.418501040494824).abs() < 1e-12,
            "{p:?}"
        );
        assert!(
            (radial_distance(p, b) - 15.217518737937626).abs() < 1e-12,
            "{p:?}"
        );
    }
}

/// The crossing form is DISJOINT from the tangent form in δ and declines
/// everything else: exact tangency (the band is the generator arm's),
/// nested and disjoint circles, coaxial and non-parallel axes.
#[test]
pub(crate) fn s433_crossing_declines_tangent_nested_disjoint_coaxial_and_crossing_axes() {
    let a = cyl([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
    let tangent = cyl([0.5, 0.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    assert!(cyl_cyl_crossing_generators(a, tangent).is_none());
    assert!(cyl_cyl_tangent_generator(a, tangent).is_some());
    let nested = cyl([0.3, 0.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    assert!(cyl_cyl_crossing_generators(a, nested).is_none());
    let disjoint = cyl([2.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    assert!(cyl_cyl_crossing_generators(a, disjoint).is_none());
    let coaxial = cyl([0.0, 0.0, 0.5], [0.0, 0.0, 1.0], 1.0);
    assert!(cyl_cyl_crossing_generators(a, coaxial).is_none());
    let crossing_axes = cyl([0.0, 0.5, 1.0], [1.0, 0.0, 0.0], 0.5);
    assert!(cyl_cyl_crossing_generators(a, crossing_axes).is_none());
    // Just beyond the tangency band on the crossing side: two rulings a
    // hair apart, both honest.
    let barely = cyl([0.5 + 1e-6, 0.0, 0.0], [0.0, 0.0, 1.0], 0.5);
    let (feet, _) = cyl_cyl_crossing_generators(a, barely).expect("crossing");
    assert!((feet[0].y() - feet[1].y()).abs() > 0.0);
}

// =========================================================================
// §7 (2026-09-13, later): the amplified band is not a bound on the MOVE.
//
// The morning's move check compares `az_move` against `gate = amp · budget`,
// and `amp = 1/sin α` is precisely what diverges at a tangency. On the 30°
// SYMMETRIC Steinmetz pair (`tests/tangency_pinch_split.rs`: r = 0.4, axes
// crossing at the ORIGIN, tangencies at (0, ±0.4, 0)) that gate reaches
// 1.7×–3.3× the cylinder's own radius, so the check passes and the azimuth
// closed form slides the vertex to the OPPOSITE arm of its section — across
// the tangent point. These pin both halves: the gate really is vacuous there,
// and the nearest point really does stay on the vertex's own arm.
// =========================================================================

/// E1 (the steep section, `z = k₊·x`) for the SYMMETRIC pair — the same closed
/// form as [`e1_reloc`] with the axis crossing at the origin instead of
/// (0, 0, 1). Its measured `plane_n` in the pipeline is (−0.9659, 0, 0.2588).
fn e1_reloc_symmetric() -> EllipseReloc {
    let mut er = e1_reloc();
    let beta: f64 = std::f64::consts::FRAC_PI_6;
    let k = beta.sin() / (1.0 - beta.cos());
    // Plane `k·x − z = 0` through the origin: same normal, `d = 0`.
    let n_len = (k * k + 1.0).sqrt();
    er.plane_n = Vector3::new(k / n_len, 0.0, -1.0 / n_len);
    er.normal = er.plane_n;
    er.plane_d = 0.0;
    er.center = Point3::new(0.0, 0.0, 0.0);
    let b_dir = b_axis_dir();
    let (_, _, budget) = er.second_cyl.expect("cyl×cyl fixture");
    er.second_cyl = Some((
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(b_dir[0], b_dir[1], b_dir[2]),
        budget,
    ));
    er
}

/// The symmetric pair's −y tangency, and the unit tangent of E1 there. A point
/// of E1 near the tangency has `(x, z) = s·(1, k₊)`, so the SIGN of `s` names
/// the arm — the invariant a relocation must not flip.
fn symmetric_tangency() -> Point3 {
    Point3::new(0.0, -R, 0.0)
}
fn e1_arm(q: Point3) -> f64 {
    let beta: f64 = std::f64::consts::FRAC_PI_6;
    let k = beta.sin() / (1.0 - beta.cos());
    let t = symmetric_tangency().as_array();
    let d = [q.x() - t[0], q.z() - t[2]];
    (d[0] + k * d[1]) / (1.0 + k * k).sqrt()
}

/// The measured pre-relocation position of the 30° fixture's v53 — the worst
/// slide in the corpus of this class (`KV11_PROBE`, 2026-09-13).
fn v53() -> Point3 {
    Point3::new(
        0.084_390_239_309_566_68,
        -0.380_738_478_575_368_36,
        -0.055_003_409_871_931_31,
    )
}

/// The fixture is the symmetric configuration, and v53 sits a quarter of a
/// radius from the tangency with its E1 foot on the NEGATIVE arm.
#[test]
pub(crate) fn s433sym_fixture_is_the_30deg_symmetric_tangency() {
    let er = e1_reloc_symmetric();
    let t = symmetric_tangency().as_array();
    let radial = (t[0] * t[0] + t[1] * t[1]).sqrt();
    assert!((radial - R).abs() < 1e-15, "tangency off A's cylinder");
    let n = er.plane_n.as_array();
    let h = n[0] * t[0] + n[1] * t[1] + n[2] * t[2] + er.plane_d;
    assert!(h.abs() < 1e-15, "tangency off E1's plane: {h:.3e}");
    // The pipeline's measured plane normal, up to sign.
    assert!(
        (n[0].abs() - 0.965_925_8).abs() < 1e-6 && (n[2].abs() - 0.258_819_0).abs() < 1e-6,
        "plane_n {n:?} is not E1's measured (±0.9659, 0, ∓0.2588)"
    );
    assert!(
        (dist(v53(), symmetric_tangency()) - 0.102_558).abs() < 1e-5,
        "v53 stands {} from the tangency",
        dist(v53(), symmetric_tangency())
    );
}

/// HALF ONE — the gate is vacuous at THIS vertex, stated WITHOUT a budget. `gate = amp ·
/// budget`, and the amplification alone is ~7.8 here, so a combined Stage-1
/// chord budget of barely 4.9e-2 already admits a 3.78e-1 slide — 95 % of the
/// cylinder's radius. In the pipeline the operands' real budget put the gate at
/// **6.986e-1** (1.7× the radius) at this very vertex, and 1.302e0 at v77, so
/// the move check passed and the slide went through. No narrowing of this band
/// is available, because the band IS `1/sin α` and `sin α → 0` is the tangency
/// itself — which is why §7's obvious repair (take the nearest point on this arm
/// unconditionally) is recorded there as BUILT, MEASURED and REFUTED rather than
/// landed: it is corpus-neutral, converts nothing, and turns
/// `c0058_authored_geometry_union_mints_its_tangent_points` red.
#[test]
pub(crate) fn s433sym_amplification_admits_a_slide_of_a_whole_radius() {
    let er = e1_reloc_symmetric();
    let (ap2, ad2, _) = er.second_cyl.expect("cyl×cyl fixture");
    let amp = cyl_cyl_point_amplification(v53(), (er.axis_point, er.axis_dir), (ap2, ad2))
        .expect("v53 is near-tangent but not AT the tangency");
    assert!(
        (amp - 7.82).abs() < 0.05,
        "the 1/sin α amplification at v53 is {amp:.4} (≈7.82)"
    );
    let (az, _) = project_onto_ellipse_via_cylinder(v53(), &er).expect("azimuth projection");
    let admitting_budget = dist(v53(), az) / amp;
    assert!(
        admitting_budget < 5.0e-2,
        "a combined chord budget of only {admitting_budget:.3e} already makes \
         `amp · budget` admit the {:.3e} azimuth slide",
        dist(v53(), az)
    );
}

/// HALF TWO — what the slide actually does. The azimuth projection crosses the
/// tangent point onto the OPPOSITE arm of E1 and moves ~4× further than the
/// nearest point, which stays on v53's own arm.
#[test]
pub(crate) fn s433sym_azimuth_crosses_the_tangency_and_nearest_does_not() {
    let er = e1_reloc_symmetric();
    let (az, _) = project_onto_ellipse_via_cylinder(v53(), &er).expect("azimuth projection");
    let (near, _) = project_onto_ellipse_nearest(v53(), &er).expect("nearest projection");
    let (az_move, near_move) = (dist(v53(), az), dist(v53(), near));

    assert!(
        (az_move - 0.378_178).abs() < 1e-4,
        "azimuth move {az_move:.6} (measured 3.781780e-1 in the pipeline)"
    );
    assert!(
        (near_move - 0.097_65).abs() < 1e-4,
        "nearest move {near_move:.6} (measured 9.765e-2 in the pipeline)"
    );
    // The arm invariant: v53's own foot is on the NEGATIVE arm; the azimuth
    // projection lands on the POSITIVE one, i.e. across the tangent point.
    assert!(
        e1_arm(near) < 0.0,
        "the nearest point must stay on v53's own arm, got s = {:.6e}",
        e1_arm(near)
    );
    assert!(
        e1_arm(az) > 0.0,
        "the azimuth point must be the one that crosses, got s = {:.6e}",
        e1_arm(az)
    );
    // Both are still exactly ON the section — the choice is WHICH exact point.
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
