//! The scanline's own properties, on polygons whose answer is arithmetic.

use super::*;
use crate::annotation::layout::LayoutCurve;

fn ring(points: &[[f64; 2]]) -> HatchLoop {
    HatchLoop {
        curves: vec![LayoutCurve::Polyline {
            points: points.to_vec(),
            closed: true,
        }],
        hole: false,
        exact: true,
    }
}

fn params(spacing: f64, angle_deg: f64) -> HatchParams {
    HatchParams {
        spacing,
        angle_rad: angle_deg.to_radians(),
        sagitta: 1e-4,
    }
}

/// The lowest and highest `y` any segment endpoint reaches.
fn y_span(fill: &HatchFill) -> (f64, f64) {
    fill.segments
        .iter()
        .flatten()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p[1]), hi.max(p[1]))
        })
}

#[test]
fn a_square_hatched_horizontally_is_one_line_per_grid_step_inside_it() {
    // A 10 × 10 square at the origin, 2 apart, horizontal: the grid is
    // multiples of 2 from the ORIGIN, so the candidate scanlines are
    // y = 0, 2, 4, 6, 8, 10.
    let square = ring(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]);
    let fill = hatch_segments(&[square], &params(2.0, 0.0));
    assert!(fill.warnings.is_empty(), "{:?}", fill.warnings);
    // FIVE lines, not six, and that is the half-open crossing test
    // `(y0 <= y) != (y1 <= y)` showing itself: the square's own `y` range is
    // half-open, so the scanline on the BOTTOM edge belongs to the shape (two
    // crossings, through the two vertical edges) and the one on the TOP edge
    // does not (zero crossings — every edge there has both ends `<= y`).
    // Which end is included is a convention; counting each exactly once is
    // not, and that is what this measures.
    assert_eq!(fill.segments.len(), 5, "{:?}", fill.segments);
    for seg in &fill.segments {
        let width = (seg[1][0] - seg[0][0]).abs();
        assert!((width - 10.0).abs() < 1e-9, "{seg:?}");
    }
    let (lo, hi) = y_span(&fill);
    assert!(
        (lo - 0.0).abs() < 1e-9 && (hi - 8.0).abs() < 1e-9,
        "the lines run from the bottom edge to one step below the top: {lo} … {hi}"
    );
}

#[test]
fn the_grid_is_anchored_at_the_origin_so_two_caps_of_one_view_line_up() {
    // The property D4b named, stated as the one measurement that shows it:
    // two squares at DIFFERENT heights, hatched together, put every line on
    // the same multiples of the spacing. Anchoring each cap on its own
    // minimum would give the upper one lines at 3.5, 5.5, … instead.
    let lower = ring(&[[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]]);
    let upper = ring(&[[0.0, 7.5], [4.0, 7.5], [4.0, 11.5], [0.0, 11.5]]);
    let fill = hatch_segments(&[lower, upper], &params(2.0, 0.0));
    for seg in &fill.segments {
        let y = seg[0][1];
        assert!(
            (y / 2.0 - (y / 2.0).round()).abs() < 1e-9,
            "a line at y = {y} is not on the global grid"
        );
    }
    assert!(fill.segments.len() >= 4, "{:?}", fill.segments);
}

#[test]
fn a_hole_is_not_hatched_and_the_scanline_through_its_extreme_counts_once() {
    // The even-odd claim AND the half-open crossing test in one fixture: a
    // 12 × 12 square with a 4 × 4 square hole exactly in the middle, hatched
    // at a spacing that puts a scanline exactly on the hole's top and bottom
    // edges (y = 4 and y = 8), which is where the classic double-count bug
    // fires.
    let outer = ring(&[[0.0, 0.0], [12.0, 0.0], [12.0, 12.0], [0.0, 12.0]]);
    let hole = HatchLoop {
        hole: true,
        ..ring(&[[4.0, 4.0], [8.0, 4.0], [8.0, 8.0], [4.0, 8.0]])
    };
    let fill = hatch_segments(&[outer, hole], &params(2.0, 0.0));
    assert!(fill.warnings.is_empty(), "{:?}", fill.warnings);
    // No segment may pass through the hole's interior. Checked on the
    // SEGMENT, not on its endpoints: a line straight across the bore has its
    // ends outside it.
    for seg in &fill.segments {
        let y = seg[0][1];
        let (x0, x1) = (seg[0][0].min(seg[1][0]), seg[0][0].max(seg[1][0]));
        let inside_y = y > 4.0 + 1e-9 && y < 8.0 - 1e-9;
        let crosses = x0 < 4.0 - 1e-9 && x1 > 8.0 + 1e-9;
        assert!(
            !(inside_y && crosses),
            "a hatch line at y = {y} ran straight through the hole: {seg:?}"
        );
    }
    // The rows strictly inside the hole's y range are TWO spans each (left of
    // the hole and right of it); the rows outside it are one.
    let at = |y: f64| {
        fill.segments
            .iter()
            .filter(|s| (s[0][1] - y).abs() < 1e-9)
            .count()
    };
    assert_eq!(at(6.0), 2, "the middle row straddles the hole");
    assert_eq!(at(2.0), 1, "a row below the hole is one span");
    assert_eq!(at(10.0), 1, "a row above the hole is one span");
    // And the scanlines exactly ON the hole's edges are TWO spans and ONE
    // span — never three, which is what a crossing counted twice through a
    // vertex would give. The asymmetry is the half-open rule: the hole's `y`
    // range is `[4, 8)`, so its bottom edge is inside it and its top edge is
    // not. Whichever end is included, each vertex counts exactly once, which
    // is the property that matters — a double count would open a gap in the
    // hatch at the one row a reader looks at to see whether the bore is
    // hatched.
    assert_eq!(at(4.0), 2, "the scanline on the hole's bottom edge");
    assert_eq!(at(8.0), 1, "the scanline on the hole's top edge");
}

#[test]
fn a_forty_five_degree_hatch_runs_at_forty_five_degrees_and_fills_the_square() {
    let square = ring(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]);
    let fill = hatch_segments(std::slice::from_ref(&square), &params(2.0, 45.0));
    assert!(!fill.segments.is_empty());
    for seg in &fill.segments {
        let (dx, dy) = (seg[1][0] - seg[0][0], seg[1][1] - seg[0][1]);
        assert!(
            (dy - dx).abs() < 1e-9,
            "a +45° line has dy = dx, got {dx} and {dy}"
        );
    }
    // The sign of the angle is the lean, and it reverses: a −45° hatch runs
    // the other way. This is the one thing the paper/view frame difference
    // turns on, so it is asserted rather than assumed.
    let fill = hatch_segments(&[square], &params(2.0, -45.0));
    for seg in &fill.segments {
        let (dx, dy) = (seg[1][0] - seg[0][0], seg[1][1] - seg[0][1]);
        assert!((dy + dx).abs() < 1e-9, "a −45° line has dy = −dx");
    }
}

#[test]
fn a_circular_cap_is_hatched_from_its_analytic_boundary() {
    // The arm that needs `Curve2::flatten`: a cap whose boundary is a true
    // circle, not a polyline. A radius-5 disc hatched at 1 has ~10 lines and
    // the longest is the diameter.
    let disc = HatchLoop {
        curves: vec![LayoutCurve::Circle {
            center: [0.0, 0.0],
            radius: 5.0,
            start_angle: 0.0,
            end_angle: std::f64::consts::TAU,
        }],
        hole: false,
        exact: true,
    };
    let fill = hatch_segments(&[disc], &params(1.0, 0.0));
    assert!(fill.warnings.is_empty(), "{:?}", fill.warnings);
    assert_eq!(fill.segments.len(), 9, "y = −4 … 4: {:?}", fill.segments);
    let longest = fill
        .segments
        .iter()
        .map(|s| (s[1][0] - s[0][0]).abs())
        .fold(0.0, f64::max);
    assert!(
        (longest - 10.0).abs() < 0.01,
        "the widest chord of a radius-5 disc is 10, got {longest}"
    );
}

#[test]
fn degenerate_inputs_are_named_rather_than_drawn_or_hung_on() {
    let square = ring(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]);
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let fill = hatch_segments(std::slice::from_ref(&square), &params(bad, 0.0));
        assert!(fill.segments.is_empty(), "spacing {bad} drew something");
        assert_eq!(fill.warnings.len(), 1, "spacing {bad}");
    }
    let fill = hatch_segments(std::slice::from_ref(&square), &params(2.0, f64::NAN));
    assert!(fill.segments.is_empty());
    assert!(fill.warnings[0].contains("angle"), "{:?}", fill.warnings);
    // A spacing so fine the cap would need more lines than the cap: truncated
    // WITH a warning, never silently short.
    let fill = hatch_segments(std::slice::from_ref(&square), &params(1e-4, 0.0));
    assert_eq!(fill.segments.len(), MAX_LINES);
    assert!(
        fill.warnings.iter().any(|w| w.contains("stopped at")),
        "{:?}",
        fill.warnings
    );
    // Nothing to hatch is not an error.
    assert_eq!(hatch_segments(&[], &params(2.0, 0.0)), HatchFill::default());
    // A loop that is a single point has no interior and says so.
    let fill = hatch_segments(
        &[HatchLoop {
            curves: vec![LayoutCurve::Point { at: [1.0, 1.0] }],
            hole: false,
            exact: true,
        }],
        &params(2.0, 0.0),
    );
    assert!(fill.segments.is_empty());
    assert_eq!(fill.warnings.len(), 1, "{:?}", fill.warnings);
}

#[test]
fn a_cap_that_came_back_unhatched_says_why_even_when_nothing_was_malformed() {
    // The module's own contract: "a cap that came back unhatched should say
    // why". Two paths used to reach the end QUIETLY — zero segments, zero
    // warnings — and a section view whose cap is unhatched for an unnamed
    // reason reads as a cap nobody sectioned.

    // (a) A boundary of three DISTINCT points enclosing no area. It passes the
    // `>= 3` ring test, so none of the earlier refusals fires; its crossings
    // never pair into a span.
    let collinear = ring(&[[0.0, 0.0], [5.0, 0.0], [10.0, 0.0]]);
    let fill = hatch_segments(std::slice::from_ref(&collinear), &params(2.0, 0.0));
    assert!(fill.segments.is_empty(), "{:?}", fill.segments);
    assert_eq!(fill.warnings.len(), 1, "{:?}", fill.warnings);
    assert!(
        fill.warnings[0].contains("encloses no area"),
        "{:?}",
        fill.warnings
    );

    // (b) A cap narrower than the hatch pitch, with no grid multiple between
    // its extremes. A legitimate drawing — a thin web sections to a cap thinner
    // than 3 mm of paper — so it is a WARNING and not an error; what it must
    // not be is unexplained, because the reader of the sheet sees a blank cap.
    let sliver = ring(&[[0.0, 10.1], [10.0, 10.1], [10.0, 11.4], [0.0, 11.4]]);
    let fill = hatch_segments(std::slice::from_ref(&sliver), &params(2.0, 0.0));
    assert!(fill.segments.is_empty(), "{:?}", fill.segments);
    assert_eq!(fill.warnings.len(), 1, "{:?}", fill.warnings);
    assert!(
        fill.warnings[0].contains("narrower than"),
        "{:?}",
        fill.warnings
    );

    // And the rule does not fire on a cap that DID hatch: a warning on every
    // ordinary section would be noise nobody reads.
    let square = ring(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]);
    let fill = hatch_segments(std::slice::from_ref(&square), &params(2.0, 45.0));
    assert!(!fill.segments.is_empty());
    assert!(fill.warnings.is_empty(), "{:?}", fill.warnings);
}

#[test]
fn a_holed_circular_cap_hatched_at_forty_five_degrees_leaves_its_bore_clear() {
    // The PRODUCTION configuration, which no other test here assembles: the
    // real angle (45°), a CURVED boundary (so `Curve2::flatten` is in the
    // path), and a HOLE (so even-odd has to work on a chord polygon rather
    // than on exact vertices). The existing tests take these one at a time —
    // the hole case is an axis-aligned square at 0°, the circular case has no
    // hole — and a bore that filled in would be the most visible wrong thing
    // this module can produce.
    let disc = |radius: f64, hole: bool| HatchLoop {
        curves: vec![LayoutCurve::Circle {
            center: [0.0, 0.0],
            radius,
            start_angle: 0.0,
            end_angle: std::f64::consts::TAU,
        }],
        hole,
        exact: true,
    };
    let fill = hatch_segments(&[disc(10.0, false), disc(4.0, true)], &params(1.0, 45.0));
    assert!(fill.warnings.is_empty(), "{:?}", fill.warnings);
    assert!(!fill.segments.is_empty());

    // Nothing drawn crosses the bore. Checked along each segment rather than at
    // its ends, because a span that bridged the hole would have both ends in
    // the annulus — which is exactly how a hole comes to be filled in.
    for s in &fill.segments {
        for i in 0..=40 {
            let t = i as f64 / 40.0;
            let p = [
                s[0][0] + t * (s[1][0] - s[0][0]),
                s[0][1] + t * (s[1][1] - s[0][1]),
            ];
            let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
            assert!(
                r > 4.0 - 0.01,
                "a hatch line reaches r = {r}, inside the 4-radius bore"
            );
            assert!(r < 10.0 + 0.01, "a hatch line reaches r = {r}, outside the cap");
        }
    }
    // And the bore really is bridged by the SCANLINE, so the clearance above is
    // the fill rule working and not an artefact of no line reaching the middle:
    // the rows through the bore come back as TWO spans, one per side.
    let through_middle = fill
        .segments
        .iter()
        .filter(|s| {
            // The scanline's own coordinate is `−x sin θ + y cos θ`; a row
            // through the centre has it near zero.
            let (c, s_) = (45.0_f64.to_radians().cos(), 45.0_f64.to_radians().sin());
            (-s[0][0] * s_ + s[0][1] * c).abs() < 0.5
        })
        .count();
    assert!(
        through_middle >= 2,
        "a scanline through the bore must be two spans, got {through_middle}: {:?}",
        fill.segments
    );
}
