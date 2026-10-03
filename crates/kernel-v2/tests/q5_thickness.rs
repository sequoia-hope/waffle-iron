//! Q5 oracles — `specs/agent_mechanical_design.md` §4.4 "Thickness", widened
//! to the four cases the increment names.
//!
//! Every expected number here is a closed form written out in the assertion,
//! never a value copied from a previous run. Where the answer is SAMPLED
//! rather than exact, the assertion is a band derived from the sampling — the
//! reported `spacing` — and not a band chosen to make the test pass.
//!
//! | case | what it proves |
//! |---|---|
//! | plate | a planar pair reports its thickness to rounding, and the thinnest site names the two planes |
//! | tube | a curved pair reports `r_outer − r_inner`, exactly, because site and hit are both refined onto the analytic surfaces |
//! | stepped plate | a thin SECTION is found and located, with a closed-form minimum |
//! | wedge | a taper's thinnest place is its ACUTE CORNER, where the true minimum is 0 — the measured correction to §4.4's "the thin edge", and the clearest case of why `min` is an upper bound |
//! | rigid motion | the same body moved and turned reports the same minimum and maximum; its SITE SET does not survive, because the render CDT is frame-dependent |
//! | solid cylinder | the far side of the site's OWN face is measurable (the self band is local, not global) |

use std::f64::consts::PI;

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use kernel_v2::measure::thickness::thickness;
use kernel_v2::{boolean_op, extrude, transform_solid, BrepArena, Profile, SolidId};
use waffle_types::kernel::RigidPlacement;

// -------------------------------------------------------------------------
// Fixtures (meters, as the kernel works in)
// -------------------------------------------------------------------------

/// A prism over the polygon `pts` in the `z = 0` plane, `h` tall.
fn prism(arena: &mut BrepArena, pts: Vec<Point2>, h: f64) -> SolidId {
    let p = Profile::new(
        Point3::new(0.0, 0.0, 0.0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        pts,
        vec![],
    )
    .expect("polygon profile");
    extrude(arena, &p, Vector3::new(0.0, 0.0, 1.0), h)
        .expect("extrude")
        .solid
}

fn block(arena: &mut BrepArena, sx: f64, sy: f64, sz: f64) -> SolidId {
    prism(
        arena,
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(sx, 0.0),
            Point2::new(sx, sy),
            Point2::new(0.0, sy),
        ],
        sz,
    )
}

fn cylinder(arena: &mut BrepArena, z0: f64, r: f64, h: f64) -> SolidId {
    let p = Profile::circle(
        Point3::new(0.0, 0.0, z0),
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Point2::new(0.0, 0.0),
        r,
    )
    .expect("circle profile");
    extrude(arena, &p, Vector3::new(0.0, 0.0, 1.0), h)
        .expect("extrude")
        .solid
}

#[track_caller]
fn close(got: f64, want: f64, rel: f64, what: &str) {
    let scale = want.abs().max(got.abs()).max(f64::MIN_POSITIVE);
    assert!(
        (got - want).abs() <= rel * scale,
        "{what}: got {got:e}, want {want:e} (relative {:e} > {rel:e})",
        (got - want).abs() / scale
    );
}

// -------------------------------------------------------------------------

/// A plate reports its thickness on the planar pair, to rounding — and the
/// thinnest site is between the two plates' faces, not between a face and
/// itself.
///
/// The plate is 40 × 40 × 5 mm, so its thickness (5 mm) is well under its
/// other two dimensions and the minimum must be the plate direction.
#[test]
fn a_plate_reports_its_thickness_on_the_planar_pair() {
    let (sx, sy, t) = (0.04, 0.04, 0.005);
    let mut arena = BrepArena::new();
    let s = block(&mut arena, sx, sy, t);
    let r = thickness(&arena, s, None).expect("a plate has walls");

    // A plane's facets are exact and the ray is perpendicular to the opposite
    // plane, so this is a rounding-level equality, not a chord band.
    close(r.min, t, 1e-15, "the minimum wall is the plate thickness");
    assert!(r.samples > 100, "a plate is sampled densely: {}", r.samples);
    assert_eq!(
        r.declines,
        kernel_v2::measure::thickness::Declines::default(),
        "every site of a closed convex plate casts to a wall"
    );
    // The thinnest site sits on one of the 40 × 40 faces and lands on the
    // other: its two z coordinates are the two plate planes.
    let (a, b) = (r.thinnest.point.z(), r.thinnest.opposite.z());
    assert!(
        (a.min(b) - 0.0).abs() < 1e-15 && (a.max(b) - t).abs() < 1e-15,
        "the thinnest site spans the two plate planes, got z = {a} → {b}"
    );
    assert_ne!(
        r.thinnest.from, r.thinnest.to,
        "a wall is between two faces"
    );
    // The widest wall a cast can find on this plate is its 40 mm side, and the
    // diagonal is NOT a wall: a ray along an inward normal is axis-parallel
    // here.
    close(
        r.max,
        sx,
        1e-15,
        "the thickest wall is the plate's own side",
    );
    assert!(
        r.mean > r.min && r.mean < r.max,
        "the mean lies inside the range: {} not in ({}, {})",
        r.mean,
        r.min,
        r.max
    );
    // The histogram counts every site exactly once.
    assert_eq!(
        r.histogram.iter().map(|b| b.count).sum::<usize>(),
        r.samples,
        "the histogram accounts for every site"
    );
    assert!(
        r.histogram[0].lo == r.min && (r.histogram[r.histogram.len() - 1].hi - r.max).abs() < 1e-15,
        "the bins span exactly [min, max]: {:?}",
        r.histogram
    );
}

/// A tube reports `r_outer − r_inner`. EXACTLY, not within the chord band:
/// the site is projected onto the outer cylinder and the hit is refined onto
/// the inner one, so neither end of the measurement is a chord.
///
/// This is the case that would read low by the tessellation's sagitta if
/// either refinement were missing — which is why the test also pins that
/// every site was refined.
#[test]
fn a_tube_reports_the_wall_between_its_two_cylinders() {
    let (r_out, r_in, h) = (0.010, 0.007, 0.030);
    let mut arena = BrepArena::new();
    let outer = cylinder(&mut arena, 0.0, r_out, h);
    let bore = cylinder(&mut arena, -0.001, r_in, h + 0.002);
    let tube =
        boolean_op(&mut arena, outer, bore, BoolOp::Subtract).expect("a bore through a cylinder");

    let r = thickness(&arena, tube, None).expect("a tube has walls");
    close(r.min, r_out - r_in, 1e-12, "the wall is r_out − r_in");
    assert!(
        r.refined > 0,
        "a curved wall is measured on the analytic surfaces, not on chords"
    );
    // The two faces of the thinnest site are both cylindrical, and the site is
    // at the outer radius or the inner one.
    let radius = |p: Point3| (p.x() * p.x() + p.y() * p.y()).sqrt();
    let (a, b) = (radius(r.thinnest.point), radius(r.thinnest.opposite));
    close(
        a.max(b),
        r_out,
        1e-9,
        "the far end of the wall is the outer radius",
    );
    close(
        a.min(b),
        r_in,
        1e-9,
        "the near end of the wall is the bore radius",
    );
    // The tube's height is a wall too — between the two annular caps — and it
    // is the widest one here, since the bore's own diameter is not a wall
    // (a ray along an end cap's inward normal is parallel to the bore).
    close(r.max, h, 1e-9, "the thickest wall is the tube's height");
}

/// A STEPPED plate reports its thin section exactly, and the thinnest site is
/// inside that section.
///
/// This is §4.4's "a wedge reports the thin edge as the minimum site" in the
/// form where the answer is a closed form: the section is 6 mm thick for
/// `x < L/2` and 2 mm for `x > L/2`, every corner is a right angle, so the
/// body's true minimum wall IS 2 mm and a planar pair measures it to
/// rounding. (The genuine wedge is the next test, and it does NOT have a
/// closed-form minimum — see there.)
#[test]
fn a_stepped_plate_reports_its_thin_section_exactly() {
    let (l, t0, t1, w) = (0.040, 0.006, 0.002, 0.020);
    let mut arena = BrepArena::new();
    let s = prism(
        &mut arena,
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(l, 0.0),
            Point2::new(l, t1),
            Point2::new(l / 2.0, t1),
            Point2::new(l / 2.0, t0),
            Point2::new(0.0, t0),
        ],
        w,
    );
    let r = thickness(&arena, s, None).expect("a stepped plate has walls");

    close(r.min, t1, 1e-15, "the minimum wall is the thin section");
    // And it is measured IN the thin section, between the two planes that
    // bound it.
    assert!(
        r.thinnest.point.x() > l / 2.0,
        "the thinnest site is in the thin half (x = {}, step at {})",
        r.thinnest.point.x(),
        l / 2.0
    );
    let (a, b) = (r.thinnest.point.y(), r.thinnest.opposite.y());
    assert!(
        (a.min(b) - 0.0).abs() < 1e-15 && (a.max(b) - t1).abs() < 1e-15,
        "the thinnest site spans the thin section's two planes, got y = {a} → {b}"
    );
    close(r.max, l, 1e-15, "the widest wall is the plate's own length");
}

/// A real WEDGE is thinnest at its ACUTE CORNER, not at its thin end — and the
/// sampler finds the corner. This is §4.4's oracle corrected by measurement.
///
/// The section is a trapezoid 6 mm thick at `x = 0` and 2 mm at `x = 40 mm`.
/// Its thin END has a perpendicular wall of `t1·cos α`, but the corner where
/// the slant meets the thick end face is 84.3° — ACUTE — and a body is
/// arbitrarily thin near an acute corner: the wall measured along `+x` from
/// the end face at height `y` is `(t0 − y)/slope`, which goes to zero as `y`
/// approaches `t0`. So the wedge's true minimum wall is 0 and no sampler can
/// report it; what it reports is the thinnest SITE's wall, bounded by how
/// close a site gets to the corner.
///
/// A taper cannot avoid this: a right trapezoid's slant makes one base angle
/// `90° − α`, so every tapered section has exactly one acute corner. The
/// closed-form case is therefore the STEPPED plate above, and this test pins
/// the behaviour that makes `min` an UPPER bound rather than a measurement of
/// the body: it is below the thin end's wall, and it sits at the corner.
#[test]
fn a_wedge_is_thinnest_at_its_acute_corner_which_no_sampler_can_bound() {
    let (l, t0, t1, w) = (0.040, 0.006, 0.002, 0.020);
    let mut arena = BrepArena::new();
    let s = prism(
        &mut arena,
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(l, 0.0),
            Point2::new(l, t1),
            Point2::new(0.0, t0),
        ],
        w,
    );
    let r = thickness(&arena, s, None).expect("a wedge has walls");

    let slope = (t0 - t1) / l;
    let cos_a = 1.0 / (1.0 + slope * slope).sqrt();
    assert!(
        r.min > 0.0 && r.min < t1 * cos_a,
        "the acute corner is thinner than the thin end's perpendicular wall \
         {:e}: got {:e}",
        t1 * cos_a,
        r.min
    );
    // The thinnest site is AT that corner — the edge where `x = 0` meets
    // `y = t0` — within one sample spacing of it.
    let d = (r.thinnest.point.x().powi(2) + (t0 - r.thinnest.point.y()).powi(2)).sqrt();
    assert!(
        d <= r.spacing,
        "the thinnest site is at the acute corner: {:e} away, spacing {:e} \
         (site {:?})",
        d,
        r.spacing,
        r.thinnest.point
    );
    // And the wall there is the corner geometry's own: the `+x` distance from
    // the end face at the site's height.
    close(
        r.min,
        (t0 - r.thinnest.point.y()) / slope,
        1e-9,
        "the corner wall is (t0 − y)/slope",
    );
    close(r.max, l, 1e-9, "the widest wall is the taper's length");
    assert_eq!(w, 0.020, "the extrusion width is not what is measured here");
}

/// The same body, moved and turned, reports the same minimum and maximum wall
/// — the invariance §4.4 asks for.
///
/// What is NOT invariant, and is measured here rather than assumed: the SITE
/// SET. The render CDT is computed from the copy's own coordinates, so a
/// rotated plate triangulates differently, the sub-facet subdivision lands
/// differently, and the site COUNT changes (measured 8528 → 4744 for this
/// plate under a 30° turn). The values survive because a planar wall is the
/// same everywhere on it; on a body whose thin spot is a corner (see the
/// wedge) a rotation can move the reported minimum by up to the spacing, which
/// is exactly what `Sampled` means.
#[test]
fn thickness_is_invariant_under_a_rigid_motion() {
    let (sx, sy, t) = (0.04, 0.025, 0.005);
    let mut arena = BrepArena::new();
    let s = block(&mut arena, sx, sy, t);
    let here = thickness(&arena, s, None).expect("a plate has walls");

    // 30° about z, then a translation well away from the origin.
    let (c, sn) = (PI / 6.0).cos_sin_pair();
    let placement = RigidPlacement {
        rotation: [[c, -sn, 0.0], [sn, c, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.17, -0.09, 0.31],
    };
    let moved = transform_solid(&mut arena, s, &placement).expect("a rigid copy");
    let there = thickness(&arena, moved, None).expect("the copy has walls");

    close(there.min, here.min, 1e-12, "the minimum wall is invariant");
    close(there.max, here.max, 1e-12, "so is the maximum");
    // The thinnest site need not be the same POINT (the CDT moved), but it
    // must still be a site of the same wall: its own span is the minimum, and
    // both ends lie in the moved body.
    close(
        dist3(there.thinnest.point, there.thinnest.opposite),
        here.min,
        1e-12,
        "the thinnest site's own span is the minimum it reports",
    );
    let lo = placement_apply(&placement, Point3::new(0.0, 0.0, 0.0));
    let hi = placement_apply(&placement, Point3::new(0.0, 0.0, t));
    let d = |p: Point3, q: [f64; 3]| {
        ((p.x() - q[0]).powi(2) + (p.y() - q[1]).powi(2) + (p.z() - q[2]).powi(2)).sqrt()
    };
    // The plate's two large faces moved to the planes through `lo` and `hi`;
    // the thinnest site spans them, so each end is on one of the two.
    for end in [there.thinnest.point, there.thinnest.opposite] {
        let on_lo = ((end.x() - lo[0]) * placement.rotation[0][2]
            + (end.y() - lo[1]) * placement.rotation[1][2]
            + (end.z() - lo[2]) * placement.rotation[2][2])
            .abs();
        let on_hi = ((end.x() - hi[0]) * placement.rotation[0][2]
            + (end.y() - hi[1]) * placement.rotation[1][2]
            + (end.z() - hi[2]) * placement.rotation[2][2])
            .abs();
        assert!(
            on_lo < 1e-12 || on_hi < 1e-12,
            "each end of the thinnest wall is on one of the moved plate faces: \
             {end:?} is {on_lo:e} / {on_hi:e} off them (|lo| {:e})",
            d(end, lo)
        );
    }
}

/// A solid cylinder's wall is its DIAMETER, measured across the same face the
/// site sits on — which only works because the self-hit band is local to the
/// site's own facet rather than a band on the whole body.
///
/// A global band would have thrown this away or, on a thin body, thrown
/// everything away.
#[test]
fn a_solid_cylinder_measures_across_its_own_lateral_face() {
    let (r_cyl, h) = (0.012, 0.050);
    let mut arena = BrepArena::new();
    let s = cylinder(&mut arena, 0.0, r_cyl, h);
    let r = thickness(&arena, s, None).expect("a cylinder has walls");

    // The caps are 50 mm apart and the lateral face is 24 mm across, so the
    // minimum is the diameter and the maximum is the height.
    close(r.min, 2.0 * r_cyl, 1e-9, "the minimum wall is the diameter");
    close(r.max, h, 1e-12, "the maximum wall is the height");
    assert_eq!(
        r.thinnest.from, r.thinnest.to,
        "the diameter is measured from the lateral face to ITSELF"
    );
    assert_eq!(
        r.declines.below_self_band, 0,
        "a wall 24 mm wide is nowhere near the local sagitta band"
    );
}

/// A denser `spacing` is honoured and reported, and it cannot change the
/// answer on a body whose walls are flat: more sites, same minimum.
#[test]
fn a_requested_spacing_adds_sites_without_moving_a_flat_wall() {
    let mut arena = BrepArena::new();
    let s = block(&mut arena, 0.04, 0.04, 0.005);
    let coarse = thickness(&arena, s, Some(0.008)).expect("walls");
    let fine = thickness(&arena, s, Some(0.001)).expect("walls");

    assert!(
        fine.samples > coarse.samples * 4,
        "an 8× denser spacing is many more sites: {} vs {}",
        fine.samples,
        coarse.samples
    );
    assert!(
        fine.spacing < coarse.spacing,
        "the reported spacing follows the request: {:e} vs {:e}",
        fine.spacing,
        coarse.spacing
    );
    close(fine.min, coarse.min, 1e-15, "a flat wall does not move");
    close(fine.min, 0.005, 1e-15, "and it is the plate thickness");
}

/// A spacing that is not a positive length is refused, typed — never silently
/// replaced by the default.
#[test]
fn a_degenerate_spacing_is_refused() {
    let mut arena = BrepArena::new();
    let s = block(&mut arena, 0.04, 0.04, 0.005);
    for bad in [0.0, -0.001, f64::NAN, f64::INFINITY] {
        let err = thickness(&arena, s, Some(bad)).expect_err("a bad spacing is refused");
        assert!(
            format!("{err}").contains("spacing"),
            "the refusal names the argument: {err}"
        );
    }
}

// -------------------------------------------------------------------------

fn dist3(a: Point3, b: Point3) -> f64 {
    ((a.x() - b.x()).powi(2) + (a.y() - b.y()).powi(2) + (a.z() - b.z()).powi(2)).sqrt()
}

/// `R·p + t`, for the invariance check.
fn placement_apply(p: &RigidPlacement, q: Point3) -> [f64; 3] {
    let v = q.as_array();
    [0, 1, 2].map(|i| {
        p.rotation[i][0] * v[0]
            + p.rotation[i][1] * v[1]
            + p.rotation[i][2] * v[2]
            + p.translation[i]
    })
}

/// `(cos, sin)` without pulling in a trait for it.
trait CosSin {
    fn cos_sin_pair(self) -> (f64, f64);
}
impl CosSin for f64 {
    fn cos_sin_pair(self) -> (f64, f64) {
        (self.cos(), self.sin())
    }
}
