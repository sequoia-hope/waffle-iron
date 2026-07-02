//! N2-2 ADVERSARY — independent adversarial probes on the Stage-4 `d(T)`
//! primitive (`yang_rs::stage4_dt::{eval_uv, d_of_t}`; spec
//! `specs/n2_stage4_dt_recompute.md`; FIP §6 validation phase).
//!
//! This file is the ADVERSARY half of a role-separated FIP cycle: TESTS ONLY —
//! it never edits production or the unit-test module inside `stage4_dt.rs`.
//! Per the repo convention that integration-test files cannot share helpers,
//! the point-to-triangle distance oracle is re-declared here (same
//! Ericson-style region/clamp algorithm as the unit tests' oracle, but an
//! independently typed copy, not an import).
//!
//! Adversarial contract (what this file tries to break):
//!   1. **I1 certification under stress** — dense 40-subdivision barycentric
//!      grids (861 samples/triangle, > the unit tests' 20): sliver uv
//!      triangles hugging a full azimuth turn, extreme aspect ratios, a
//!      thin-ring torus (R = 1 + 1e-3, r = 1) whose tangent-intersection
//!      middle control points land at NEGATIVE radial coordinate near the
//!      inner equator, sphere triangles straddling the equator and pinned to
//!      BOTH poles, cones at both ends of the legal half-angle interval,
//!      extreme magnitudes (r = 1e8 / 1e-8, datum at 1e6), and generic
//!      oblique triangles on every curved surface.
//!   2. **Degenerate-but-legal inputs** — collinear / coincident uv corners
//!      must return Ok, finite, >= 0, and STILL certify (the oracle degrades
//!      to segment/point distance for degenerate 3D triangles).
//!   3. **Failure-mode boundary precision** — the exact f64 on each side of
//!      every §6 boundary (2π azimuth span, ±π/2 sphere latitude, cone v = 0
//!      / −0.0 / −5e-324), and validation ORDER (finiteness strictly first).
//!   4. **No-panic sweep** — a deterministic 13 × 16 = 208-combination grid:
//!      every call returns Ok(finite ≥ 0, bit-deterministic) or a typed Err;
//!      never a panic, never a NaN.
//!
//! Mutation sanity (FIP §6.3) was executed before this file was finalized:
//! (a) azimuth middle-control scale 1/cos(Δu/2) → cos(Δu/2), (b) u-subdivision
//! forced to 1 slice, (c) profile-arc middle tangent-intersection scale
//! r/cos(Δv/2) → r. Each mutation is caught (kill matrix in the PR report);
//! all mutations were reverted before commit.

use cad_primitives::{Point2, Point3, Vector3};
use std::f64::consts::{FRAC_PI_2, PI};
use yang_rs::stage4_dt::{d_of_t, eval_uv, DtError};
use yang_rs::Surface;

// =========================================================================
// Fixtures.
// =========================================================================

fn uv(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> [Point2; 3] {
    [
        Point2::new(a.0, a.1),
        Point2::new(b.0, b.1),
        Point2::new(c.0, c.1),
    ]
}

fn cyl(axis_point: Point3, axis_dir: Vector3, radius: f64) -> Surface {
    Surface::Cylinder {
        axis_point,
        axis_dir,
        radius,
    }
}

fn cyl_z(radius: f64) -> Surface {
    cyl(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        radius,
    )
}

/// A half-angle DELIBERATELY just below π/2 (π/2 ≈ 1.5707963…; gap ≈ 9.6e-5,
/// tan ≈ 1.04e4): probes the wide end of the legal open interval (0, π/2).
/// This is intentionally NOT an approximation of the constant π/2 — the whole
/// point is that it is strictly inside the legal range.
#[allow(clippy::approx_constant)]
const NEAR_RIGHT_ANGLE: f64 = 1.5707;

fn cone_z(half_angle: f64) -> Surface {
    Surface::Cone {
        apex: Point3::new(0.0, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle,
    }
}

fn sphere(center: Point3, radius: f64) -> Surface {
    Surface::Sphere { center, radius }
}

fn torus(center: Point3, axis_dir: Vector3, major: f64, minor: f64) -> Surface {
    Surface::Torus {
        center,
        axis_dir,
        major_radius: major,
        minor_radius: minor,
    }
}

/// Thin-ring torus: R barely above r. Near the inner equator (v ≈ π) the
/// profile-arc tangent-intersection middle control point has radial
/// coordinate `R + (r/cos(Δv/2))·cos(v_mid)` ≈ 1.001 − r·sec(Δv/2) < 0 —
/// a NEGATIVE-ρ control point. The rational-arc algebra is linear in ρ, so
/// the net (and the convex-hull certificate) must still be exact there.
fn thin_torus() -> Surface {
    torus(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        1.0 + 1e-3,
        1.0,
    )
}

// =========================================================================
// Independent point-to-triangle distance oracle (re-declared; integration
// tests cannot share helpers). Region/clamp closest-point algorithm
// (Ericson §5.1.5): interior perpendicular foot → plane distance, else the
// minimum over the three CLAMPED point-segment distances (covers edges and
// vertices). Degenerate 3D triangles (n² = 0) skip the interior branch and
// degrade to segment/point distance — exactly the degenerate-input contract
// of spec §3 step 4.
// =========================================================================

fn sub(a: Point3, b: Point3) -> [f64; 3] {
    [a.x() - b.x(), a.y() - b.y(), a.z() - b.z()]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn point_segment_distance(p: Point3, a: Point3, b: Point3) -> f64 {
    let ab = sub(b, a);
    let ap = sub(p, a);
    let len2 = dot(ab, ab);
    // Zero-length segment (coincident endpoints) → plain point distance.
    let t = if len2 == 0.0 {
        0.0
    } else {
        (dot(ap, ab) / len2).clamp(0.0, 1.0)
    };
    let d = [ap[0] - t * ab[0], ap[1] - t * ab[1], ap[2] - t * ab[2]];
    dot(d, d).sqrt()
}

fn point_triangle_distance(p: Point3, a: Point3, b: Point3, c: Point3) -> f64 {
    let edge_min = point_segment_distance(p, a, b)
        .min(point_segment_distance(p, b, c))
        .min(point_segment_distance(p, c, a));
    let v0 = sub(b, a);
    let v1 = sub(c, a);
    let v2 = sub(p, a);
    let n = cross(v0, v1);
    let n2 = dot(n, n);
    if n2 > 0.0 {
        let d00 = dot(v0, v0);
        let d01 = dot(v0, v1);
        let d11 = dot(v1, v1);
        let d20 = dot(v2, v0);
        let d21 = dot(v2, v1);
        let denom = d00 * d11 - d01 * d01; // == n2 > 0
        let bv = (d11 * d20 - d01 * d21) / denom;
        let bw = (d00 * d21 - d01 * d20) / denom;
        if bv >= 0.0 && bw >= 0.0 && bv + bw <= 1.0 {
            return dot(v2, n).abs() / n2.sqrt();
        }
    }
    edge_min
}

// =========================================================================
// The I1 certification hammer: a 40-subdivision barycentric grid — 861
// on-surface samples per triangle, denser than the unit tests' 20-grid —
// every sample's distance to the 3D triangle must be <= d(T) + slack.
// Dense sampling is a LOWER bound on the true max (spec §7a): it can refute
// certification, never confirm it — which is exactly the adversary's job.
// =========================================================================

const GRID_N: usize = 40;

/// Certify I1/I7 for one triangle. `slack` is the certification headroom:
/// `1e-12` at unit scale (the spec-I1 constant), scaled by the coordinate
/// magnitude for extreme-magnitude probes (each call site justifies its
/// value). Returns the bound for further assertions.
fn certify(surface: &Surface, tri: [Point2; 3], slack: f64, label: &str) -> f64 {
    let d = d_of_t(surface, tri)
        .unwrap_or_else(|e| panic!("{label}: d_of_t must succeed on legal input, got {e:?}"));
    assert!(
        d.is_finite() && d >= 0.0,
        "{label}: I7 violated: d(T) = {d}"
    );
    let c: Vec<Point3> = tri
        .iter()
        .map(|&p| eval_uv(surface, p).unwrap_or_else(|e| panic!("{label}: corner eval: {e:?}")))
        .collect();
    for i in 0..=GRID_N {
        for j in 0..=(GRID_N - i) {
            // (N − i − j)/N keeps b0 + b1 + b2 == 1 exactly at the corners,
            // so corner samples reproduce the corner uv bit-exactly.
            let b0 = i as f64 / GRID_N as f64;
            let b1 = j as f64 / GRID_N as f64;
            let b2 = (GRID_N - i - j) as f64 / GRID_N as f64;
            let s = Point2::new(
                b0 * tri[0].x() + b1 * tri[1].x() + b2 * tri[2].x(),
                b0 * tri[0].y() + b1 * tri[1].y() + b2 * tri[2].y(),
            );
            let q = eval_uv(surface, s)
                .unwrap_or_else(|e| panic!("{label}: sample eval at ({i},{j}): {e:?}"));
            let dist = point_triangle_distance(q, c[0], c[1], c[2]);
            assert!(
                dist <= d + slack,
                "{label}: I1 VIOLATED at grid ({i},{j}) uv=({}, {}): \
                 sample dist {dist} > d(T) {d} + slack {slack}",
                s.x(),
                s.y()
            );
        }
    }
    d
}

/// Spec-I1 slack at unit scale (coordinates O(1)): the spec's own `1e-12`.
const UNIT_SLACK: f64 = 1e-12;

// =========================================================================
// 1. Certification under stress (I1, the crown jewel).
// =========================================================================

/// Wrap-around NEEDLE hugging a full azimuth turn: u-span 2π − 1e-6 (4
/// sub-rectangles, each ≈ π/2) with a 1e-9 v extent, and the third corner
/// right next to the first — the 3D triangle is a microscopic needle near
/// (r, 0, 0), but the triangle's long uv EDGE wraps the whole cylinder, so
/// its parametric footprint reaches the antipode (−r, 0, ·) a full diameter
/// away. The bound MUST dominate ≈ 2r. This is also the killer for the
/// "skip u-subdivision" mutation (a single slice over a > π span puts the
/// tangent-intersection middle point on the WRONG side of the axis, near the
/// needle, collapsing the bound to ≈ 0 while true distances stay ≈ 2r).
#[test]
fn adv_i1_cylinder_full_turn_needle() {
    let s = cyl_z(1.0);
    let tri = uv((0.0, 0.0), (2.0 * PI - 1e-6, 0.0), (5e-7, 1e-9));
    // Unit-scale coordinates → the spec-I1 slack.
    let d = certify(&s, tri, UNIT_SLACK, "cyl full-turn needle");
    // Grid samples on the long edge reach u ≈ π, distance ≈ 2r = 2 from the
    // needle; 1e-3 headroom covers the 1e-6/1e-9 sliver offsets.
    assert!(
        d >= 2.0 - 1e-3,
        "bound {d} cannot be below the antipodal sample distance ≈ 2"
    );
    // I5 on a stress input: repeat call is bit-identical.
    let d2 = d_of_t(&s, tri).expect("repeat");
    assert_eq!(d.to_bits(), d2.to_bits(), "I5 violated on needle input");
}

/// A full-turn sliver whose third corner sits at u = π: the antipode is a
/// triangle CORNER (distance 0) and the true max distance is the ≈ r chord
/// offset at u = π/2 — a differently-shaped stress on the same wrap.
#[test]
fn adv_i1_cylinder_full_turn_straddle_sliver() {
    let s = cyl_z(1.0);
    let tri = uv((0.0, 0.0), (2.0 * PI - 1e-6, 0.0), (PI, 1e-9));
    let d = certify(&s, tri, UNIT_SLACK, "cyl full-turn straddle");
    // Sample (0, ±1, ·) at u = π/2 lies ≈ r = 1 from the long chord.
    assert!(d >= 1.0 - 1e-6, "bound {d} must see the u=π/2 chord offset");
}

/// Same wrap-around needle on a torus (outer equator, v ≈ 0.1): both u and
/// the tiny v extent go through the Arc profile path.
#[test]
fn adv_i1_torus_full_turn_needle() {
    let s = torus(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        3.0,
        1.0,
    );
    let tri = uv((0.0, 0.1), (2.0 * PI - 1e-3, 0.1), (5e-4, 0.1 + 1e-7));
    // Unit-scale (coordinates O(4)) → spec-I1 slack.
    let d = certify(&s, tri, UNIT_SLACK, "torus full-turn needle");
    // Far side of the ring is 2(R + r cos 0.1) ≈ 7.99 from the needle; 7.5
    // leaves headroom for the 1e-3 wrap gap.
    assert!(
        d >= 7.5,
        "bound {d} must dominate the ≈ 8 far-side distance"
    );
}

/// Extreme aspect ratios on the cylinder: a needle 3 units of azimuth wide ×
/// 1e-9 tall, and a needle 1e-8 wide × 10 units tall.
#[test]
fn adv_i1_cylinder_extreme_aspect_ratios() {
    let s = cyl_z(1.0);
    // Wide-flat: u-span 3.0 > π/2 forces subdivision; v-span 1e-9.
    certify(
        &s,
        uv((0.0, 0.0), (3.0, 0.0), (1.5, 1e-9)),
        UNIT_SLACK, // unit scale
        "cyl wide-flat needle",
    );
    // Tall-thin: u-span 1e-8; v-span 10. Degree-1 axial profile is exact over
    // any span — the bound comes purely from the microscopic azimuth bulge.
    certify(
        &s,
        uv((0.0, 0.0), (1e-8, 0.0), (5e-9, 10.0)),
        UNIT_SLACK, // unit scale
        "cyl tall-thin needle",
    );
}

/// Thin-ring torus (R = 1.001, r = 1) around the inner equator: the profile
/// middle control points land at NEGATIVE radial coordinates (see
/// `thin_torus`) and the triangle straddles v = π. Certification must hold
/// through the sign flip.
#[test]
fn adv_i1_thin_torus_inner_equator_straddles_pi() {
    let s = thin_torus();
    // v-span 2.4 > π/2 → 2 v-slices; v_mid of each slice is near π where
    // cos ≈ −1 → ρ_mid = R − r·sec(Δv/2)·|cos v_mid| < 0.
    certify(
        &s,
        uv((0.0, PI - 1.2), (1.0, PI + 1.2), (0.5, PI)),
        UNIT_SLACK, // unit scale
        "thin torus inner-equator straddle",
    );
    // Narrower band exactly at the inner equator, wide in u (forces azimuth
    // subdivision at the smallest surviving radius R − r = 1e-3).
    certify(
        &s,
        uv((0.0, PI - 0.2), (2.5, PI + 0.15), (1.2, PI)),
        UNIT_SLACK, // unit scale
        "thin torus inner-equator azimuth-wide",
    );
}

/// Sphere triangle straddling the equator (v = 0 strictly inside the
/// v-range), oblique in both parameters.
#[test]
fn adv_i1_sphere_straddles_equator() {
    let s = sphere(Point3::new(0.1, 0.2, 0.3), 2.0);
    certify(
        &s,
        uv((0.1, -0.7), (1.3, 0.6), (0.7, 0.9)),
        UNIT_SLACK, // unit scale
        "sphere equator straddle",
    );
}

/// Sphere triangle with corners AT both poles (v = ±π/2 exactly): the
/// azimuth rings degenerate to points at both ends of a π-long v-span
/// (2 v-slices), and the covering rectangle's u-extent is meaningless at the
/// poles themselves.
#[test]
fn adv_i1_sphere_pole_to_pole() {
    let s = sphere(Point3::new(0.0, 0.0, 0.0), 1.5);
    certify(
        &s,
        uv((0.2, FRAC_PI_2), (1.0, -FRAC_PI_2), (2.5, 0.0)),
        UNIT_SLACK, // unit scale
        "sphere pole-to-pole",
    );
}

/// Cone at both extremes of the legal half-angle interval (0, π/2):
/// 1.5707 (tan ≈ 1.04e4 — a near-plane splay) and 1e-6 (a near-line spike).
#[test]
fn adv_i1_cone_extreme_half_angles() {
    // Near-π/2: coordinates reach ρ = v·tan(1.5707) ≈ 1e4. Slack scales with
    // the coordinate magnitude: 1e-12 relative at scale 1e4 → 1e-8 absolute
    // (f64 rounding at 1e4 is ~1e-12 per op; 1e-8 leaves 4 orders of margin
    // while staying 12 orders below the ~1e4 geometry).
    let wide = cone_z(NEAR_RIGHT_ANGLE);
    certify(
        &wide,
        uv((0.0, 0.2), (1.2, 0.5), (0.6, 1.0)),
        1e-12 * 1e4,
        "cone half_angle just below π/2",
    );
    // Near-0: the patch hugs the axis (ρ ≤ 2e-6); distances are O(1e-6) so
    // the unit-scale slack is still 6 orders below the geometry.
    let narrow = cone_z(1e-6);
    certify(
        &narrow,
        uv((0.0, 0.5), (2.0, 1.0), (1.0, 2.0)),
        UNIT_SLACK,
        "cone half_angle 1e-6",
    );
}

/// Cone triangle with one corner exactly at the apex (v = 0) and a u-span
/// forcing subdivision — the apex ring is a single point; the net must
/// degenerate cleanly.
#[test]
fn adv_i1_cone_apex_corner_subdivided() {
    let s = cone_z(0.6);
    certify(
        &s,
        uv((0.7, 0.0), (2.3, 0.8), (1.5, 1.6)),
        UNIT_SLACK, // unit scale
        "cone apex corner",
    );
}

/// Extreme magnitudes: cylinder radius 1e8 and 1e-8.
#[test]
fn adv_i1_cylinder_extreme_radii() {
    // r = 1e8: coordinates are O(1e8); one f64 op rounds at ~1e-8 absolute.
    // Slack = 1e-12 relative × 1e8 scale = 1e-4 absolute — 4 orders above
    // per-op rounding, 12 orders below the geometry (d itself is O(1e7)).
    let big = cyl_z(1e8);
    let d_big = certify(
        &big,
        uv((0.0, 0.0), (2.0, 0.0), (1.0, 1e8)),
        1e-12 * 1e8,
        "cyl r=1e8",
    );
    assert!(
        d_big > 1e6,
        "an O(1e8) patch must have an O(≥1e6) bulge bound"
    );
    // r = 1e-8 with unit axial extent: mixed scale; coordinates ≤ O(1), so
    // the spec's unit-scale slack applies (distances are ≥ O(1e-8), still
    // 4 orders above the slack — the check stays meaningful).
    let small = cyl_z(1e-8);
    certify(
        &small,
        uv((0.0, 0.0), (2.0, 0.0), (1.0, 1e-8)),
        UNIT_SLACK,
        "cyl r=1e-8",
    );
}

/// Datum far from the origin: axis_point (1e6, −1e6, 1e6). Every evaluated
/// point carries ~1e6·ε ≈ 2e-10 absolute rounding before distances are even
/// formed; slack = 1e-12 relative × 1e6 scale = 1e-6 absolute dominates the
/// cancellation noise while staying 6 orders below the unit-radius geometry.
#[test]
fn adv_i1_far_datum_cylinder() {
    let s = cyl(
        Point3::new(1e6, -1e6, 1e6),
        Vector3::new(0.0, 0.0, 1.0),
        1.0,
    );
    let tri = uv((0.0, 0.0), (FRAC_PI_2, 0.0), (0.0, 1.0));
    let d = certify(&s, tri, 1e-12 * 1e6, "far-datum cylinder");
    // I6 cross-check against the same triangle at the origin: the bound is
    // geometric. Tolerance 1e-9 mirrors spec I6.
    let d0 = d_of_t(&cyl_z(1.0), tri).expect("origin twin");
    assert!(
        (d - d0).abs() < 1e-9,
        "far-datum bound {d} drifted from origin bound {d0}"
    );
}

/// Profile-arc ISOLATION: near-zero u-span with a large v-span, so the
/// certified bound comes almost entirely from the PROFILE-arc middle control
/// points (the tangent-intersection scale r/cos(Δv/2)), not from the azimuth
/// bulge. This is the dedicated killer for the "drop the profile-arc middle
/// tangent scale" mutation — with the middle point pulled back onto the
/// circle, the hull under-covers the meridian sagitta and I1 fails here even
/// when azimuth-dominated probes still pass.
#[test]
fn adv_i1_profile_arc_isolated_thin_lunes() {
    // Sphere: pole-to-pole thin lune (u-span 1e-9, v-span π → 2 v-slices).
    certify(
        &sphere(Point3::new(0.0, 0.0, 0.0), 2.0),
        uv((0.0, -FRAC_PI_2), (1e-9, FRAC_PI_2), (5e-10, 0.0)),
        UNIT_SLACK, // unit scale
        "sphere pole-to-pole thin lune",
    );
    // Sphere: single-slice profile arc (v-span 1.5 < π/2, NO v-subdivision):
    // exactly one tangent-intersection middle point carries the bound.
    certify(
        &sphere(Point3::new(0.0, 0.0, 0.0), 2.0),
        uv((0.0, -0.75), (1e-9, 0.75), (5e-10, 0.0)),
        UNIT_SLACK, // unit scale
        "sphere single-arc thin lune",
    );
    // Torus: thin meridian band straddling the inner equator v = π (where
    // the thin-ring middle control points also go radially negative).
    certify(
        &thin_torus(),
        uv((0.0, PI - 1.0), (1e-9, PI + 1.0), (5e-10, PI)),
        UNIT_SLACK, // unit scale
        "thin-torus meridian band at v=π",
    );
}

/// Generic oblique triangles for every curved surface. NOTE (pigeonhole):
/// three corners realize four covering-rectangle extremes (u0, u1, v0, v1),
/// so at least ONE corner always coincides with a rectangle corner — a
/// triangle touching NO rect corner is impossible. These fixtures are the
/// adversarial minimum: exactly one corner on a rect corner, the rectangle
/// substantially larger than the triangle (the max-slack Fig-6 shape).
#[test]
fn adv_i1_generic_oblique_all_curved_surfaces() {
    // Corners (0.3, 0.8) / (1.9, 0.4) / (1.0, 1.5): rect [0.3,1.9]×[0.4,1.5];
    // only (1.9, 0.4) is a rect corner.
    let tri = uv((0.3, 0.8), (1.9, 0.4), (1.0, 1.5));
    certify(&cyl_z(1.0), tri, UNIT_SLACK, "oblique cylinder");
    certify(&cone_z(0.5), tri, UNIT_SLACK, "oblique cone");
    certify(
        &sphere(Point3::new(0.1, 0.2, 0.3), 2.0),
        // Sphere latitudes must stay in [−π/2, π/2]: same shape, shifted v.
        uv((0.3, 0.3), (1.9, -0.1), (1.0, 1.0)),
        UNIT_SLACK,
        "oblique sphere",
    );
    certify(
        &torus(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0), // non-canonical axis, too
            3.0,
            1.0,
        ),
        tri,
        UNIT_SLACK,
        "oblique torus (y-axis)",
    );
}

// =========================================================================
// 2. Degenerate-but-legal inputs (spec §3 step 1 / step 4).
// =========================================================================

/// All three uv corners collinear → zero-area 3D triangle; the oracle (and
/// the implementation) degrade to segment distance. Must be Ok, finite,
/// >= 0, and still certified.
#[test]
fn adv_degenerate_collinear_uv_corners() {
    // Diagonal line in uv on a cylinder: the 3D image is a helix chord fan.
    certify(
        &cyl_z(1.0),
        uv((0.0, 0.0), (1.0, 0.25), (2.0, 0.5)),
        UNIT_SLACK, // unit scale
        "cyl collinear-diagonal",
    );
    // Vertical uv line on a sphere (fixed u): the patch is a meridian arc,
    // the "triangle" a chord — d(T) must dominate the sagitta.
    let d = certify(
        &sphere(Point3::new(0.0, 0.0, 0.0), 2.0),
        uv((0.5, -0.3), (0.5, 0.4), (0.5, 0.1)),
        UNIT_SLACK, // unit scale
        "sphere meridian chord",
    );
    // Sagitta of a radius-2 arc over 0.7 rad ≈ r(1 − cos(0.35)) ≈ 0.121:
    // strictly positive bound (1e-3 floor is ~100× below the true sagitta).
    assert!(d > 1e-3, "meridian-chord bound {d} must see the sagitta");
    // Horizontal uv line on a torus (fixed v): azimuth arc vs chord.
    certify(
        &torus(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            3.0,
            1.0,
        ),
        uv((0.0, 1.0), (2.0, 1.0), (1.0, 1.0)),
        UNIT_SLACK, // unit scale
        "torus latitude chord",
    );
}

/// Two coincident corners → the 3D triangle is a segment (one zero-length
/// edge). Ok + certified.
#[test]
fn adv_degenerate_two_coincident_corners() {
    certify(
        &cyl_z(1.0),
        uv((0.5, 0.5), (0.5, 0.5), (1.5, 1.0)),
        UNIT_SLACK, // unit scale
        "cyl two-coincident",
    );
    certify(
        &thin_torus(),
        uv((0.2, 3.0), (0.2, 3.0), (1.4, 3.5)),
        UNIT_SLACK, // unit scale
        "thin-torus two-coincident",
    );
}

/// All three corners coincident → zero-area uv triangle AND zero-area
/// covering rectangle: the net collapses to the single surface point, the 3D
/// triangle to that same point, so d(T) must be exactly 0 (and certified).
#[test]
fn adv_degenerate_all_corners_coincident() {
    for (s, label) in [
        (cyl_z(1.0), "cyl point"),
        (cone_z(0.5), "cone point"),
        (sphere(Point3::new(0.1, 0.2, 0.3), 2.0), "sphere point"),
        (thin_torus(), "torus point"),
    ] {
        let tri = uv((1.0, 0.5), (1.0, 0.5), (1.0, 0.5));
        let d = certify(&s, tri, UNIT_SLACK, label);
        // Point net vs the same point: exactly zero, no tolerance needed.
        assert_eq!(d, 0.0, "{label}: point-degenerate d(T) must be exactly 0");
    }
}

// =========================================================================
// 3. Failure-mode boundary probing (spec §6) — the exact f64 on each side.
// =========================================================================

/// Azimuth span: EXACTLY 2π is legal (one full period); the very next
/// representable f64 above 2π must be AzimuthSpanTooLarge (the implemented
/// contract is strictly `span > 2π`).
#[test]
fn adv_boundary_azimuth_span_next_up() {
    let s = cyl_z(1.0);
    let full = 2.0 * PI;
    // Exactly 2π: legal AND certified (4 exact π/2 slices).
    certify(
        &s,
        uv((0.0, 0.0), (full, 0.0), (PI, 0.5)),
        UNIT_SLACK, // unit scale
        "cyl span exactly 2π",
    );
    // One ulp above: u1 − u0 = next_up(2π) − 0 > 2π strictly → Err.
    assert_eq!(
        d_of_t(&s, uv((0.0, 0.0), (full.next_up(), 0.0), (PI, 0.5))),
        Err(DtError::AzimuthSpanTooLarge),
        "span one ulp above 2π must be rejected"
    );
    // Torus: the u check applies there too; v has NO span limit (it is a
    // periodic tube angle; spec §6 lists no torus v constraint) — a 3π
    // v-span subdivides into 6 slices and still certifies.
    let t = torus(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        3.0,
        1.0,
    );
    assert_eq!(
        d_of_t(&t, uv((0.0, 0.0), (full.next_up(), 0.0), (PI, 0.5))),
        Err(DtError::AzimuthSpanTooLarge)
    );
    certify(
        &t,
        uv((0.0, 0.0), (1.0, 3.0 * PI), (0.5, 1.5 * PI)),
        UNIT_SLACK, // unit scale
        "torus 3π v-span (legal wrap)",
    );
}

/// Sphere latitude: v = ±π/2 exactly is legal (poles); one ulp beyond either
/// pole is PolarRangeOutOfBounds.
#[test]
fn adv_boundary_sphere_polar_next_up() {
    let s = sphere(Point3::new(0.0, 0.0, 0.0), 1.0);
    // Full-range pole-to-pole is legal (also certified in
    // adv_i1_sphere_pole_to_pole).
    assert!(d_of_t(&s, uv((0.0, FRAC_PI_2), (1.0, -FRAC_PI_2), (0.5, 0.0))).is_ok());
    // One ulp past the north pole.
    assert_eq!(
        d_of_t(&s, uv((0.0, FRAC_PI_2.next_up()), (1.0, 0.0), (0.5, 0.2))),
        Err(DtError::PolarRangeOutOfBounds)
    );
    // One ulp past the south pole.
    assert_eq!(
        d_of_t(
            &s,
            uv((0.0, (-FRAC_PI_2).next_down()), (1.0, 0.0), (0.5, 0.2))
        ),
        Err(DtError::PolarRangeOutOfBounds)
    );
    // Same boundary through eval_uv.
    assert!(eval_uv(&s, Point2::new(0.3, FRAC_PI_2)).is_ok());
    assert!(eval_uv(&s, Point2::new(0.3, -FRAC_PI_2)).is_ok());
    assert_eq!(
        eval_uv(&s, Point2::new(0.3, FRAC_PI_2.next_up())),
        Err(DtError::PolarRangeOutOfBounds)
    );
}

/// Cone axial range: v = 0 is legal (apex). v = −0.0 is ALSO legal under the
/// implemented `v < 0.0` test (IEEE: −0.0 < 0.0 is false) and evaluates to
/// the apex — bitwise sign noise on a semantically-zero coordinate does not
/// spuriously reject. (Spec §6 says "any v < 0"; −0.0 is not < 0, so Ok is
/// the contract-consistent reading — flagged in the adversary report as
/// worth a one-line spec clarification.) The smallest negative subnormal
/// (−5e-324) IS < 0 and must be rejected: the boundary is exact, not fuzzy.
#[test]
fn adv_boundary_cone_apex_signed_zero_and_subnormal() {
    let s = cone_z(0.5);
    // v = 0 exactly: legal.
    assert!(d_of_t(&s, uv((0.0, 0.0), (1.0, 1.0), (0.5, 0.5))).is_ok());
    // v = −0.0: legal, and eval_uv lands exactly on the apex.
    let tri_nz = uv((0.0, -0.0), (1.0, 1.0), (0.5, 0.5));
    assert!(
        d_of_t(&s, tri_nz).is_ok(),
        "−0.0 is not < 0; must not be rejected"
    );
    let apex = eval_uv(&s, Point2::new(0.7, -0.0)).expect("−0.0 evaluates");
    assert_eq!(
        (apex.x(), apex.y(), apex.z()),
        (0.0, 0.0, 0.0),
        "v = −0.0 must evaluate to the apex exactly"
    );
    // v = −5e-324 (smallest negative subnormal): strictly < 0 → rejected.
    assert_eq!(
        d_of_t(&s, uv((0.0, -5e-324), (1.0, 1.0), (0.5, 0.5))),
        Err(DtError::NegativeConeAxialRange)
    );
    assert_eq!(
        eval_uv(&s, Point2::new(0.0, -5e-324)),
        Err(DtError::NegativeConeAxialRange)
    );
}

/// Validation ORDER: finiteness strictly outranks structural validity
/// (spec §6: "NonFiniteInput — any NaN/∞ in uv or in surface fields";
/// the implementation documents finiteness-first). Every fixture here is
/// BOTH non-finite AND structurally invalid — the error must always be
/// NonFiniteInput.
#[test]
fn adv_boundary_nonfinite_outranks_invalid() {
    let tri = uv((0.0, 0.0), (1.0, 0.0), (0.5, 1.0));
    // ∞ radius on a cylinder whose axis is ALSO zero (two defects).
    let c = cyl(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 0.0),
        f64::INFINITY,
    );
    assert_eq!(d_of_t(&c, tri), Err(DtError::NonFiniteInput));
    // −∞ apex coordinate + out-of-range half_angle (2.0 > π/2).
    let k = Surface::Cone {
        apex: Point3::new(f64::NEG_INFINITY, 0.0, 0.0),
        axis_dir: Vector3::new(0.0, 0.0, 1.0),
        half_angle: 2.0,
    };
    assert_eq!(d_of_t(&k, tri), Err(DtError::NonFiniteInput));
    // NaN major radius + minor_radius 0 (horn-degenerate).
    let t = torus(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
        f64::NAN,
        0.0,
    );
    assert_eq!(d_of_t(&t, tri), Err(DtError::NonFiniteInput));
    // NaN in uv + invalid surface: the uv finiteness check fires first.
    let bad_surface = cyl_z(-1.0);
    assert_eq!(
        d_of_t(&bad_surface, uv((f64::NAN, 0.0), (1.0, 0.0), (0.5, 1.0))),
        Err(DtError::NonFiniteInput)
    );
    // ∞ (not just NaN) in uv.
    assert_eq!(
        d_of_t(
            &cyl_z(1.0),
            uv((f64::INFINITY, 0.0), (1.0, 0.0), (0.5, 1.0))
        ),
        Err(DtError::NonFiniteInput)
    );
    // eval_uv: non-finite point + invalid surface → NonFiniteInput.
    assert_eq!(
        eval_uv(&bad_surface, Point2::new(f64::INFINITY, 0.0)),
        Err(DtError::NonFiniteInput)
    );
}

/// Order between the two RANGE checks: a triangle violating BOTH the v-range
/// and the 2π azimuth limit reports the v-range error (the implementation
/// validates v before the azimuth span; spec §6 does not pin this pair's
/// order — asserted here so any silent reordering is a conscious change).
#[test]
fn adv_boundary_vrange_checked_before_azimuth() {
    // Cone: v < 0 AND u-span 7 > 2π.
    assert_eq!(
        d_of_t(&cone_z(0.5), uv((0.0, -0.5), (7.0, 1.0), (3.0, 0.5))),
        Err(DtError::NegativeConeAxialRange)
    );
    // Sphere: v beyond pole AND u-span 7 > 2π.
    assert_eq!(
        d_of_t(
            &sphere(Point3::new(0.0, 0.0, 0.0), 1.0),
            uv((0.0, 2.0), (7.0, 0.0), (3.0, 0.5))
        ),
        Err(DtError::PolarRangeOutOfBounds)
    );
}

/// Planes are exempt from the azimuth-span check (u/v are unbounded in-plane
/// lengths, not angles) and from every range check — and always return
/// exactly 0.0 (I2), even for coordinates that would be far out of range on
/// any angular surface.
#[test]
fn adv_boundary_plane_exempt_from_angular_checks() {
    let p = Surface::Plane {
        normal: Vector3::new(1.0, 2.0, 3.0),
        d: 0.7,
    };
    // u-span 200 (≫ 2π), v-span 2000: still Ok(0.0) exactly.
    assert_eq!(
        d_of_t(&p, uv((-100.0, -1000.0), (100.0, 0.0), (0.0, 1000.0))),
        Ok(0.0)
    );
    // But a plane is NOT exempt from finiteness/structural validation.
    assert_eq!(
        d_of_t(
            &Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 0.0),
                d: 0.0,
            },
            uv((0.0, 0.0), (1.0, 0.0), (0.0, 1.0))
        ),
        Err(DtError::InvalidSurface)
    );
    assert_eq!(
        d_of_t(
            &Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: f64::NAN,
            },
            uv((0.0, 0.0), (1.0, 0.0), (0.0, 1.0))
        ),
        Err(DtError::NonFiniteInput)
    );
}

// =========================================================================
// 4. No-panic sweep: 13 surfaces × 16 triangles = 208 deterministic
// combinations. Every call must return Ok (finite, >= 0, bit-deterministic
// on repeat) or a typed Err — never panic, never NaN. Illegal-range
// triangles are INCLUDED on purpose: for the surfaces where they are out of
// range they must surface as typed errors, on the others as certified Oks.
// =========================================================================

#[test]
fn adv_no_panic_deterministic_sweep() {
    let surfaces: Vec<(&str, Surface)> = vec![
        (
            "plane-z",
            Surface::Plane {
                normal: Vector3::new(0.0, 0.0, 1.0),
                d: -2.0,
            },
        ),
        (
            "plane-oblique",
            Surface::Plane {
                normal: Vector3::new(1.0, -2.0, 0.5),
                d: 3.0,
            },
        ),
        ("cyl-unit", cyl_z(1.0)),
        ("cyl-1e8", cyl_z(1e8)),
        ("cyl-1e-8", cyl_z(1e-8)),
        (
            "cyl-far-tilted",
            cyl(
                Point3::new(1e6, -1e6, 1e6),
                Vector3::new(1.0, 1.0, 1.0),
                2.0,
            ),
        ),
        ("cone-0.5", cone_z(0.5)),
        ("cone-near-right", cone_z(NEAR_RIGHT_ANGLE)),
        ("cone-1e-6", cone_z(1e-6)),
        ("sphere-unit", sphere(Point3::new(0.0, 0.0, 0.0), 1.0)),
        ("sphere-off", sphere(Point3::new(0.1, 0.2, 0.3), 2.0)),
        (
            "torus-3-1",
            torus(
                Point3::new(0.0, 0.0, 0.0),
                Vector3::new(0.0, 0.0, 1.0),
                3.0,
                1.0,
            ),
        ),
        (
            "torus-thin-tilted",
            torus(
                Point3::new(0.5, -0.5, 0.5),
                Vector3::new(1.0, 0.0, 1.0),
                1.0 + 1e-3,
                1.0,
            ),
        ),
    ];
    let triangles: Vec<(&str, [Point2; 3])> = vec![
        ("canonical", uv((0.0, 0.0), (FRAC_PI_2, 0.0), (0.0, 1.0))),
        (
            "full-turn-sliver",
            uv((0.0, 0.0), (2.0 * PI - 1e-6, 0.0), (PI, 1e-9)),
        ),
        ("exact-2pi", uv((0.0, 0.0), (2.0 * PI, 0.0), (PI, 0.5))),
        // > 2π: typed Err on curved surfaces, Ok(0.0) on planes.
        ("over-2pi", uv((0.0, 0.0), (2.0 * PI + 0.5, 0.0), (PI, 0.5))),
        ("point", uv((1.0, 0.5), (1.0, 0.5), (1.0, 0.5))),
        ("two-coincident", uv((0.5, 0.5), (0.5, 0.5), (1.5, 1.0))),
        ("collinear", uv((0.0, 0.0), (1.0, 0.25), (2.0, 0.5))),
        // v up to 3: PolarRangeOutOfBounds on spheres, Ok elsewhere.
        ("tall-thin", uv((0.0, 0.0), (1e-8, 0.0), (5e-9, 3.0))),
        // Negative v: NegativeConeAxialRange on cones, Ok elsewhere.
        ("negative-v", uv((0.0, -0.5), (1.0, -0.2), (0.5, -0.8))),
        // v in [2, 3]: PolarRangeOutOfBounds on spheres, Ok elsewhere.
        ("beyond-pole", uv((0.0, 2.0), (1.0, 2.5), (0.5, 3.0))),
        (
            "pole-to-pole",
            uv((0.0, -FRAC_PI_2), (1.0, FRAC_PI_2), (2.0, 0.0)),
        ),
        // Huge azimuth OFFSET (span stays 2): trig argument reduction path.
        (
            "u-offset-1e8",
            uv((1e8, 0.0), (1e8 + 2.0, 0.0), (1e8 + 1.0, 1.0)),
        ),
        ("negative-u", uv((-3.0, 0.0), (-1.0, 0.5), (-2.0, 1.0))),
        ("zero-u-span", uv((1.0, 0.0), (1.0, 1.0), (1.0, 0.5))),
        // 3π v-span: sphere rejects; torus wraps; cone/cylinder are axial.
        (
            "v-span-3pi",
            uv((0.0, 0.0), (1.0, 3.0 * PI), (0.5, 1.5 * PI)),
        ),
        ("generic-oblique", uv((0.3, 0.8), (1.9, 0.4), (1.0, 1.5))),
    ];
    let mut calls = 0usize;
    let mut oks = 0usize;
    for (sname, s) in &surfaces {
        for (tname, tri) in &triangles {
            calls += 1;
            // A panic anywhere in here fails the test — that IS the no-panic
            // assertion; the (sname, tname) pair prints via the panic payload
            // of the assert messages below or the library's own panic site.
            let r1 = d_of_t(s, *tri);
            let r2 = d_of_t(s, *tri);
            match (r1, r2) {
                (Ok(d1), Ok(d2)) => {
                    oks += 1;
                    assert!(
                        d1.is_finite() && d1 >= 0.0,
                        "[{sname} × {tname}] I7 violated: d(T) = {d1}"
                    );
                    assert_eq!(
                        d1.to_bits(),
                        d2.to_bits(),
                        "[{sname} × {tname}] I5 violated: {d1} vs {d2}"
                    );
                }
                (Err(e1), Err(e2)) => {
                    assert_eq!(e1, e2, "[{sname} × {tname}] nondeterministic error");
                }
                (a, b) => panic!("[{sname} × {tname}] nondeterministic Ok/Err: {a:?} vs {b:?}"),
            }
            // eval_uv at the first corner must agree with d_of_t on
            // acceptability for range/finite reasons... except the azimuth
            // span, which is a TRIANGLE property, not a point property — so
            // only assert eval never panics and never returns NaN.
            if let Ok(q) = eval_uv(s, tri[0]) {
                assert!(
                    q.x().is_finite() && q.y().is_finite() && q.z().is_finite(),
                    "[{sname} × {tname}] eval_uv produced a non-finite point"
                );
            }
        }
    }
    assert_eq!(calls, 208, "sweep must cover the full 13 × 16 grid");
    // Sanity that the sweep is not vacuously erroring: most combos are legal.
    assert!(
        oks >= 150,
        "expected the majority of the sweep to be legal, got {oks}/208 Ok"
    );
}
