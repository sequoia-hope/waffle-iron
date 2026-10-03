//! Crossing-search pins — the three pair kinds, their closed forms, and the
//! degeneracies each one declines or deliberately ignores.

use super::*;
use std::f64::consts::{FRAC_PI_2, PI};

fn p2(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(ax: f64, ay: f64, bx: f64, by: f64) -> Curve2 {
    Curve2::Line {
        start: p2(ax, ay),
        end: p2(bx, by),
    }
}

fn circle(cx: f64, cy: f64, r: f64, t0: f64, t1: f64) -> Curve2 {
    Curve2::Circle {
        center: p2(cx, cy),
        radius: r,
        start_angle: t0,
        end_angle: t1,
    }
}

fn ellipse(cx: f64, cy: f64, major: f64, minor: f64, axis: [f64; 2]) -> Curve2 {
    Curve2::Ellipse {
        center: p2(cx, cy),
        major_axis: axis,
        major_radius: major,
        minor_radius: minor,
        start_param: 0.0,
        end_param: TAU,
    }
}

/// Every crossing of two curves, with the tangency count beside it.
fn xs(a: &Curve2, b: &Curve2, band: f64) -> (Vec<Crossing2>, u32) {
    let (da, db) = (Decomposed::of(a), Decomposed::of(b));
    let mut out = Vec::new();
    let mut budget = u64::MAX;
    let t = crossings(&da, &db, band, &mut out, &mut budget);
    (out, t)
}

/// Every crossing's reported parameters must actually name the SAME point on
/// the two curves — the property that catches a parameter-mapping mistake in
/// any of the three closed forms at once.
fn crossings_are_consistent(a: &Curve2, b: &Curve2, band: f64, tol: f64) -> usize {
    let (found, _) = xs(a, b, band);
    for c in &found {
        let pa = a.eval(c.a).expect("a evaluates");
        let pb = b.eval(c.b).expect("b evaluates");
        let d = (pa.x() - pb.x()).hypot(pa.y() - pb.y());
        assert!(
            d <= tol,
            "the crossing at ({}, {}) names {pa:?} on one curve and {pb:?} on the other ({d} apart)",
            c.a,
            c.b
        );
    }
    found.len()
}

// --- segment × segment ---

#[test]
fn two_crossing_segments_report_one_crossing_at_the_right_parameters() {
    let a = line(-1.0, 0.0, 1.0, 0.0);
    let b = line(0.25, -1.0, 0.25, 1.0);
    let (found, tang) = xs(&a, &b, 0.0);
    assert_eq!(found.len(), 1);
    assert_eq!(tang, 0);
    // `a`'s parameter is `[0, 1]` along itself: x = 0.25 is five eighths.
    assert!((found[0].a - 0.625).abs() < 1e-15, "{:?}", found[0]);
    assert!((found[0].b - 0.5).abs() < 1e-15, "{:?}", found[0]);
    assert_eq!(crossings_are_consistent(&a, &b, 0.0, 1e-15), 1);
}

#[test]
fn segments_that_miss_report_nothing() {
    let a = line(-1.0, 0.0, 1.0, 0.0);
    assert_eq!(xs(&a, &line(2.0, -1.0, 2.0, 1.0), 0.0).0.len(), 0);
    assert_eq!(xs(&a, &line(0.0, 1.0, 1.0, 2.0), 0.0).0.len(), 0);
}

/// A near miss within the chord band DOES cross: a polyline is an inscribed
/// approximation of its source, so a crossing the true curves make can land
/// just off the chords.
#[test]
fn the_chord_band_recovers_a_crossing_the_chords_only_nearly_make() {
    let a = line(-1.0, 0.0, 1.0, 0.0);
    // Ends 0.01 short of `a`.
    let b = line(0.25, 0.01, 0.25, 1.0);
    assert_eq!(xs(&a, &b, 0.0).0.len(), 0, "no band, no crossing");
    assert_eq!(
        xs(&a, &b, 0.02).0.len(),
        1,
        "inside the band it is a crossing"
    );
    // And the recovered parameter is CLAMPED into the curve's own domain, so a
    // caller never has to handle one outside it.
    let (found, _) = xs(&a, &b, 0.02);
    assert_eq!(found[0].b, 0.0);
}

/// Parallel and coincident pairs are NOT tangency declines: they have no
/// transversal crossing and need no split, and counting them would bury the
/// real declines under the ordinary degeneracy of an axis-aligned view.
#[test]
fn parallel_and_coincident_segments_are_not_declined() {
    let a = line(0.0, 0.0, 1.0, 0.0);
    for b in [
        line(0.0, 0.5, 1.0, 0.5),     // parallel, apart
        line(0.0, 0.0, 1.0, 0.0),     // identical
        line(0.5, 0.0, 1.5, 0.0),     // collinear, overlapping
        line(0.0, 1e-18, 1.0, 1e-18), // parallel, within any band
    ] {
        let (found, tang) = xs(&a, &b, 1e-9);
        assert_eq!(tang, 0, "{b:?} must not be a tangency decline");
        assert!(found.is_empty(), "{b:?} must report no crossing: {found:?}");
    }
}

// --- segment × conic ---

#[test]
fn a_chord_through_a_circle_crosses_it_twice() {
    let c = circle(0.0, 0.0, 1.0, 0.0, TAU);
    let a = line(-2.0, 0.0, 2.0, 0.0);
    let (found, tang) = xs(&a, &c, 0.0);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(tang, 0);
    assert_eq!(crossings_are_consistent(&a, &c, 0.0, 1e-12), 2);
    // The two roots are the circle's `0` and `π` — reported inside the
    // circle's own `[0, 2π)` window.
    let mut angles: Vec<f64> = found.iter().map(|c| c.b).collect();
    angles.sort_by(f64::total_cmp);
    assert!((angles[0] - 0.0).abs() < 1e-12 || (angles[0] - TAU).abs() < 1e-12);
    assert!((angles[1] - PI).abs() < 1e-12 || (angles[1] - TAU - PI).abs() < 1e-12);
}

/// A chord whose endpoints fall OUTSIDE the arc's angular window is not a
/// crossing of the arc, only of its circle — the test that keeps a quarter rim
/// from being split by something on the other side of the part.
#[test]
fn a_chord_outside_an_arcs_window_is_not_a_crossing_of_the_arc() {
    let quarter = circle(0.0, 0.0, 1.0, 0.0, FRAC_PI_2);
    // The line `y = 0` meets the circle at angles 0 and π; only 0 is in range,
    // and it is the arc's own endpoint.
    let (found, _) = xs(&line(-2.0, 0.0, 2.0, 0.0), &quarter, 0.0);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].b.abs() < 1e-12);
    // And a line through the opposite side meets the circle twice and the arc
    // not at all.
    let (found, _) = xs(&line(-2.0, -0.5, 2.0, -0.5), &quarter, 0.0);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_line_tangent_to_a_circle_is_declined_rather_than_split() {
    let c = circle(0.0, 0.0, 1.0, 0.0, TAU);
    // Exactly tangent at the top.
    let (found, tang) = xs(&line(-2.0, 1.0, 2.0, 1.0), &c, 1e-6);
    assert_eq!(tang, 1, "a tangency must be counted");
    // It may report the double root or nothing; what it must not do is report
    // a transversal pair.
    assert!(found.len() <= 2, "{found:?}");
    // Clear of the circle by more than the band: nothing at all.
    let (found, tang) = xs(&line(-2.0, 1.1, 2.0, 1.1), &c, 1e-6);
    assert_eq!((found.len(), tang), (0, 0));
}

#[test]
fn a_line_crosses_a_rotated_ellipse_in_its_own_frame() {
    let e = ellipse(0.5, -0.25, 3.0, 1.0, [0.6, 0.8]);
    // A line through the centre must cross twice, and the reported parameters
    // must name the same two points on both curves.
    assert_eq!(
        crossings_are_consistent(&line(-5.0, -0.25, 5.0, -0.25), &e, 0.0, 1e-9),
        2
    );
    // And so must a chord that clips one end.
    let n = crossings_are_consistent(&line(2.0, -5.0, 2.0, 5.0), &e, 0.0, 1e-9);
    assert_eq!(n, 2, "a vertical chord clips the ellipse twice");
}

// --- conic × conic ---

#[test]
fn two_overlapping_circles_cross_twice() {
    let a = circle(0.0, 0.0, 1.0, 0.0, TAU);
    let b = circle(1.0, 0.0, 1.0, 0.0, TAU);
    let (found, tang) = xs(&a, &b, 0.0);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(tang, 0);
    assert_eq!(crossings_are_consistent(&a, &b, 0.0, 1e-9), 2);
    // The crossings are at `x = 0.5`, `y = ±√3/2`.
    for c in &found {
        let p = a.eval(c.a).expect("eval");
        assert!((p.x() - 0.5).abs() < 1e-9, "{p:?}");
        assert!((p.y().abs() - 3f64.sqrt() / 2.0).abs() < 1e-9, "{p:?}");
    }
}

/// THE defect this module was debugged against: two circles that are the same
/// circle (to the last bit of their radii) must report NO crossings.
///
/// Measured 2026-10-03 — a through hole's two rim circles come back with radii
/// differing in the last bit, so the implicit of one along the other is ~3e-16
/// and the sign noise around zero minted thirteen spurious roots, cutting one
/// rim into alternating visible and hidden arcs. Like a parallel segment pair,
/// a coincident conic pair needs the coincidence merge, not a split.
#[test]
fn two_coincident_circles_report_no_crossings() {
    let a = circle(0.02, 0.015, 0.006, 0.0, TAU);
    for r in [0.006, 0.006 + f64::EPSILON * 0.006, 0.006 * (1.0 - 1e-15)] {
        let b = circle(0.02, 0.015, r, 0.0, TAU);
        let (found, tang) = xs(&a, &b, 1e-5);
        assert!(
            found.is_empty() && tang == 0,
            "radius {r}: {} crossing(s), {tang} tangenc(ies)",
            found.len()
        );
    }
    // A sub-ARC of the same circle is just as coincident.
    let arc = circle(0.02, 0.015, 0.006, 0.5, 2.0);
    let (found, tang) = xs(&a, &arc, 1e-5);
    assert!(found.is_empty() && tang == 0, "{found:?} {tang}");
}

#[test]
fn two_separate_circles_report_nothing() {
    let a = circle(0.0, 0.0, 1.0, 0.0, TAU);
    assert_eq!(xs(&a, &circle(5.0, 0.0, 1.0, 0.0, TAU), 0.0).0.len(), 0);
    // And one strictly inside the other.
    assert_eq!(xs(&a, &circle(0.0, 0.0, 0.5, 0.0, TAU), 0.0).0.len(), 0);
}

#[test]
fn two_internally_tangent_circles_are_declined_rather_than_split() {
    let a = circle(0.0, 0.0, 1.0, 0.0, TAU);
    let b = circle(0.5, 0.0, 0.5, 0.0, TAU);
    let (found, tang) = xs(&a, &b, 1e-6);
    assert_eq!(tang, 1, "the tangency must be counted, got {found:?}");
}

/// The companion of the internal case, and the one that reaches the other
/// branch: two circles touching from OUTSIDE. The implicit of one along the
/// other never changes sign at all, so there is no bracket to find and the
/// contact is only visible as a sample inside the band.
#[test]
fn two_externally_tangent_circles_are_declined_rather_than_split() {
    let a = circle(0.0, 0.0, 1.0, 0.0, TAU);
    // Touching at `(1, 0)`, so the contact sits exactly on `a`'s sample at
    // `t = 0` and on `b`'s at `t = π` — the configuration a zero-counts-as-a-side
    // sign test turns into two spurious roots.
    let b = circle(2.0, 0.0, 1.0, 0.0, TAU);
    let (found, tang) = xs(&a, &b, 1e-6);
    assert!(
        found.is_empty(),
        "an external tangency is a contact, not a crossing: {found:?}"
    );
    assert_eq!(tang, 1, "and it must be counted");
    // Off the sample grid too: nudged so the contact lands between samples.
    let c = circle(2.0, 0.013, 1.0, 0.0, TAU);
    let (found, _) = xs(&a, &c, 1e-6);
    assert!(
        found.len() <= 2,
        "a near-tangential pair reports its contact or nothing, never a \
         bracketing storm: {found:?}"
    );
    assert_eq!(crossings_are_consistent(&a, &c, 1e-6, 1e-3), found.len());
}

/// A circle and an ellipse OSCULATING — tangent, and with the same curvature
/// there, which is the hardest contact of the three because the implicit stays
/// inside the band over a whole neighbourhood rather than at one sample.
///
/// The ellipse `u²/4 + v² = 1` has radius of curvature `b²/a = 1/2` at
/// `(0, ±1)`, so the circle of radius `1/2` centred at `(0, 1/2)` touches it
/// there and agrees to second order. The pair must report a contact or
/// nothing, never a run of spurious roots through the band, and it must not be
/// mistaken for the SAME conic.
#[test]
fn an_osculating_circle_and_ellipse_are_a_contact_and_not_the_same_conic() {
    let e = ellipse(0.0, 0.0, 2.0, 1.0, [1.0, 0.0]);
    let c = circle(0.0, 0.5, 0.5, 0.0, TAU);
    let (found, tang) = xs(&e, &c, 1e-6);
    assert!(
        found.len() <= 2,
        "an osculating pair is a contact, not a bracketing storm: {found:?}"
    );
    assert_eq!(crossings_are_consistent(&e, &c, 1e-6, 1e-2), found.len());
    assert!(
        tang > 0 || !found.is_empty(),
        "the contact must be reported one way or the other"
    );
    // And the coincidence test must NOT swallow it: these are two different
    // conics that agree only to second order at one point.
    let far = (0..64)
        .map(|i| {
            let t = TAU * f64::from(i) / 64.0;
            let pe = e.eval(t).expect("eval");
            let pc = c.eval(t).expect("eval");
            (pe.x() - pc.x()).hypot(pe.y() - pc.y())
        })
        .fold(0.0f64, f64::max);
    assert!(
        far > 1.0,
        "the two conics are far apart away from the contact"
    );
}

/// Concentric circles of different radii: no crossing, no contact, and
/// crucially NOT the coincidence verdict, which would be the right answer only
/// if they were the same circle.
#[test]
fn concentric_circles_of_different_radii_report_nothing_at_all() {
    let a = circle(0.02, 0.015, 0.006, 0.0, TAU);
    for r in [0.0059, 0.0061, 0.003, 0.012] {
        let b = circle(0.02, 0.015, r, 0.0, TAU);
        let (found, tang) = xs(&a, &b, 1e-6);
        assert!(
            found.is_empty() && tang == 0,
            "radius {r}: {} crossing(s), {tang} tangenc(ies)",
            found.len()
        );
    }
}

#[test]
fn a_circle_and_an_ellipse_cross_where_a_dense_sampling_says_they_do() {
    let c = circle(0.0, 0.0, 2.0, 0.0, TAU);
    let e = ellipse(0.0, 0.0, 3.0, 1.0, [1.0, 0.0]);
    let (found, _) = xs(&c, &e, 0.0);
    assert_eq!(found.len(), 4, "a circle and an ellipse meet four times");
    assert_eq!(crossings_are_consistent(&c, &e, 0.0, 1e-9), 4);
}

// --- the budget ---

#[test]
fn the_budget_stops_the_search_rather_than_running_forever() {
    let a = circle(0.0, 0.0, 2.0, 0.0, TAU);
    let b = ellipse(0.0, 0.0, 3.0, 1.0, [1.0, 0.0]);
    let (da, db) = (Decomposed::of(&a), Decomposed::of(&b));
    let mut out = Vec::new();
    let mut budget = 0u64;
    assert_eq!(crossings(&da, &db, 0.0, &mut out, &mut budget), 0);
    assert!(out.is_empty(), "a zero budget does no work: {out:?}");
    // And a budget large enough is spent, not ignored.
    let mut budget = 10_000u64;
    crossings(&da, &db, 0.0, &mut out, &mut budget);
    assert!(budget < 10_000, "the search must charge its budget");
    assert_eq!(out.len(), 4);
}

// --- a point, and the empty decomposition ---

#[test]
fn a_point_crosses_nothing() {
    let p = Curve2::Point(p2(0.0, 0.0));
    assert!(Decomposed::of(&p).is_empty());
    assert!(Decomposed::of(&Curve2::Polyline {
        points: Vec::new(),
        closed: false,
    })
    .is_empty());
    assert_eq!(xs(&p, &line(-1.0, 0.0, 1.0, 0.0), 0.0).0.len(), 0);
}

// --- polylines ---

#[test]
fn a_polyline_crosses_on_the_chord_it_actually_crosses() {
    let pl = Curve2::Polyline {
        points: vec![p2(0.0, -1.0), p2(0.0, 0.0), p2(0.0, 1.0), p2(1.0, 1.0)],
        closed: false,
    };
    let (found, _) = xs(&line(-1.0, 0.5, 1.0, 0.5), &pl, 0.0);
    assert_eq!(found.len(), 1, "{found:?}");
    // The polyline's parameter is chord index plus fraction, so `y = 0.5` on
    // the second chord is 1.5.
    assert!((found[0].b - 1.5).abs() < 1e-15, "{:?}", found[0]);
    assert_eq!(
        crossings_are_consistent(&line(-1.0, 0.5, 1.0, 0.5), &pl, 0.0, 1e-15),
        1
    );
}

#[test]
fn a_closed_polyline_carries_its_closing_chord() {
    let tri = Curve2::Polyline {
        points: vec![p2(0.0, 0.0), p2(2.0, 0.0), p2(1.0, 2.0)],
        closed: true,
    };
    // A horizontal line at y = 1 cuts the two slanted sides, and the closing
    // chord is one of them.
    let (found, _) = xs(&line(-5.0, 1.0, 5.0, 1.0), &tri, 0.0);
    assert_eq!(found.len(), 2, "{found:?}");
    let mut on: Vec<f64> = found.iter().map(|c| c.b).collect();
    on.sort_by(f64::total_cmp);
    assert!(on[0] > 1.0 && on[0] < 2.0, "the second chord: {on:?}");
    assert!(on[1] > 2.0 && on[1] < 3.0, "the closing chord: {on:?}");
}

// --- folds ---

#[test]
fn a_polylines_fold_is_where_its_direction_reverses() {
    // Out along +x and back: the fold is the middle vertex.
    let there_and_back = Curve2::Polyline {
        points: vec![p2(0.0, 0.0), p2(1.0, 0.0), p2(0.5, 0.0)],
        closed: false,
    };
    assert_eq!(folds(&there_and_back), vec![1.0]);
    // A monotone polyline has none, and neither does a gentle turn.
    assert!(folds(&Curve2::Polyline {
        points: vec![p2(0.0, 0.0), p2(1.0, 0.0), p2(2.0, 0.5)],
        closed: false,
    })
    .is_empty());
    // And the analytic arms cannot carry one.
    assert!(folds(&line(0.0, 0.0, 1.0, 0.0)).is_empty());
    assert!(folds(&circle(0.0, 0.0, 1.0, 0.0, TAU)).is_empty());
    assert!(folds(&ellipse(0.0, 0.0, 2.0, 1.0, [1.0, 0.0])).is_empty());
    // Too short to have an interior vertex.
    assert!(folds(&Curve2::Polyline {
        points: vec![p2(0.0, 0.0), p2(1.0, 0.0)],
        closed: false,
    })
    .is_empty());
}

// --- the box prune and the point/segment distance ---

#[test]
fn boxes_overlap_with_the_band_as_slack() {
    let a = [0.0, 0.0, 1.0, 1.0];
    assert!(boxes_overlap(&a, &[0.5, 0.5, 2.0, 2.0], 0.0));
    assert!(!boxes_overlap(&a, &[1.5, 0.0, 2.0, 1.0], 0.0));
    assert!(boxes_overlap(&a, &[1.5, 0.0, 2.0, 1.0], 0.6));
    assert!(!boxes_overlap(&a, &[0.0, 2.0, 1.0, 3.0], 0.5));
}

#[test]
fn the_point_segment_distance_is_the_clamped_one() {
    let (a, b) = (p2(0.0, 0.0), p2(1.0, 0.0));
    assert!((point_segment_distance(p2(0.5, 2.0), a, b) - 2.0).abs() < 1e-15);
    // Past the end, so the distance is to the endpoint and not to the line.
    assert!((point_segment_distance(p2(3.0, 0.0), a, b) - 2.0).abs() < 1e-15);
    // A degenerate segment is its own point.
    assert!((point_segment_distance(p2(3.0, 4.0), a, a) - 5.0).abs() < 1e-15);
}
