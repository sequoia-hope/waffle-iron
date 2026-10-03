//! Value-computation tests. These measure hand-built view-plane geometry;
//! the real-geometry pins (a box projected top-down, a cylinder rim) live in
//! `crates/test-harness/tests/d3_annotation_measure.rs`, where a kernel is
//! available to do the projecting.

use super::*;

fn line(start: [f64; 2], end: [f64; 2]) -> AnchorGeometry {
    AnchorGeometry::curve(LayoutCurve::Line { start, end })
}

fn circle(center: [f64; 2], radius: f64) -> AnchorGeometry {
    AnchorGeometry::curve(LayoutCurve::Circle {
        center,
        radius,
        start_angle: 0.0,
        end_angle: std::f64::consts::TAU,
    })
}

#[test]
fn an_aligned_distance_between_two_points_is_the_straight_line_between_them() {
    let v = measure(
        DimensionKind::Distance,
        &[
            AnchorGeometry::point([0.0, 0.0]),
            AnchorGeometry::point([0.03, 0.04]),
        ],
    )
    .unwrap();
    assert!((v - 0.05).abs() < 1e-15, "{v}");
}

#[test]
fn an_aligned_distance_between_two_parallel_lines_is_the_gap_not_the_centre_offset() {
    // Two 40 mm walls 25 mm apart, offset along their own direction by 10 mm
    // so a witness-point-to-witness-point measurement would read 25 mm only
    // by luck. The gap is 25 mm; sqrt(25² + 10²) = 26.9 mm would be wrong.
    let v = measure(
        DimensionKind::Distance,
        &[
            line([0.0, 0.0], [0.04, 0.0]),
            line([0.01, 0.025], [0.05, 0.025]),
        ],
    )
    .unwrap();
    assert!((v - 0.025).abs() < 1e-15, "{v}");
}

#[test]
fn two_non_parallel_lines_refuse_rather_than_pick_a_pair_of_points() {
    let err = measure(
        DimensionKind::Distance,
        &[
            line([0.0, 0.0], [0.04, 0.0]),
            line([0.0, 0.01], [0.04, 0.03]),
        ],
    )
    .unwrap_err();
    let MeasureError::AnchorsNotParallel { degrees } = err else {
        panic!("{err:?}");
    };
    assert!(degrees > 25.0 && degrees < 27.0, "{degrees}");
}

#[test]
fn parallelism_is_judged_on_the_angle_so_it_does_not_depend_on_the_lines_length() {
    // The same 1e-8 rad skew on a 1 mm line and a 1 m line: both inside
    // PARALLEL_SIN_TOL, both measurable. A length-based tolerance would have
    // accepted one and refused the other.
    //
    // The gap read back is 0.02 plus the skew's own rise at the witness
    // point, `len/2 · 1e-8` — 5e-12 m on the short line and 5e-9 m on the
    // long one. That is the skew, not an error in the measurement, so the
    // assertion is scaled to it rather than to a fixed figure.
    for len in [1e-3_f64, 1.0] {
        let v = measure(
            DimensionKind::Distance,
            &[
                line([0.0, 0.0], [len, 0.0]),
                line([0.0, 0.02], [len, 0.02 + len * 1e-8]),
            ],
        )
        .unwrap_or_else(|e| panic!("len {len}: {e}"));
        let skew_rise = 0.5 * len * 1e-8;
        assert!(
            (v - (0.02 + skew_rise)).abs() < 1e-15,
            "len {len}: {v}, expected {}",
            0.02 + skew_rise
        );
    }
    // And a skew an order of magnitude over the tolerance refuses at BOTH
    // lengths.
    for len in [1e-3_f64, 1.0] {
        let err = measure(
            DimensionKind::Distance,
            &[
                line([0.0, 0.0], [len, 0.0]),
                line([0.0, 0.02], [len, 0.02 + len * 1e-5]),
            ],
        )
        .unwrap_err();
        assert!(
            matches!(err, MeasureError::AnchorsNotParallel { .. }),
            "len {len}: {err:?}"
        );
    }
}

#[test]
fn a_point_to_line_distance_is_perpendicular_and_ignores_where_along_the_line_it_falls() {
    // The point sits well beyond the segment's end; the dimension measures to
    // the infinite line, as a drafter's leader does.
    let v = measure(
        DimensionKind::PointLineDistance,
        &[
            AnchorGeometry::point([0.5, 0.012]),
            line([0.0, 0.0], [0.01, 0.0]),
        ],
    )
    .unwrap();
    assert!((v - 0.012).abs() < 1e-15, "{v}");
}

#[test]
fn a_point_to_line_distance_needs_a_line_as_its_second_anchor() {
    let err = measure(
        DimensionKind::PointLineDistance,
        &[AnchorGeometry::point([0.0, 0.0]), circle([0.0, 0.0], 0.01)],
    )
    .unwrap_err();
    assert!(matches!(err, MeasureError::NotMeasurable { .. }), "{err:?}");
}

#[test]
fn the_horizontal_and_vertical_kinds_read_one_component_each() {
    let anchors = [
        AnchorGeometry::point([0.01, 0.002]),
        AnchorGeometry::point([0.04, 0.009]),
    ];
    let h = measure(DimensionKind::HDistance, &anchors).unwrap();
    let v = measure(DimensionKind::VDistance, &anchors).unwrap();
    assert!((h - 0.03).abs() < 1e-15, "{h}");
    assert!((v - 0.007).abs() < 1e-15, "{v}");
    // Both are magnitudes: reversing the anchors changes nothing.
    let flipped = [anchors[1].clone(), anchors[0].clone()];
    assert_eq!(measure(DimensionKind::HDistance, &flipped).unwrap(), h);
    assert_eq!(measure(DimensionKind::VDistance, &flipped).unwrap(), v);
}

#[test]
fn a_linear_dimension_on_two_lines_measures_their_midpoints_componentwise() {
    // HDistance on two vertical walls reads the wall spacing — the usual
    // drawing dimension across a plate.
    let v = measure(
        DimensionKind::HDistance,
        &[
            line([0.0, 0.0], [0.0, 0.02]),
            line([0.065, 0.0], [0.065, 0.02]),
        ],
    )
    .unwrap();
    assert!((v - 0.065).abs() < 1e-15, "{v}");
}

#[test]
fn an_angular_dimension_reads_the_acute_angle_between_two_edges() {
    let thirty = measure(
        DimensionKind::Angle,
        &[
            line([0.0, 0.0], [0.01, 0.0]),
            line(
                [0.0, 0.0],
                [
                    0.01 * (30.0_f64).to_radians().cos(),
                    0.01 * (30.0_f64).to_radians().sin(),
                ],
            ),
        ],
    )
    .unwrap();
    assert!((thirty.to_degrees() - 30.0).abs() < 1e-12, "{thirty}");

    // A projected edge carries no traversal direction, so the reversed edge
    // must measure the same 30° rather than 150°.
    let reversed = measure(
        DimensionKind::Angle,
        &[
            line([0.0, 0.0], [0.01, 0.0]),
            line(
                [
                    0.01 * (30.0_f64).to_radians().cos(),
                    0.01 * (30.0_f64).to_radians().sin(),
                ],
                [0.0, 0.0],
            ),
        ],
    )
    .unwrap();
    assert!((reversed - thirty).abs() < 1e-15, "{reversed} vs {thirty}");

    let perpendicular = measure(
        DimensionKind::Angle,
        &[line([0.0, 0.0], [0.01, 0.0]), line([0.0, 0.0], [0.0, 0.01])],
    )
    .unwrap();
    assert!(
        (perpendicular - std::f64::consts::FRAC_PI_2).abs() < 1e-15,
        "{perpendicular}"
    );
}

#[test]
fn an_angular_dimension_needs_two_straight_edges() {
    let err = measure(
        DimensionKind::Angle,
        &[line([0.0, 0.0], [0.01, 0.0]), circle([0.0, 0.0], 0.01)],
    )
    .unwrap_err();
    assert!(matches!(err, MeasureError::NotMeasurable { .. }), "{err:?}");
}

#[test]
fn a_radius_reads_the_circle_and_a_diameter_reads_twice_it() {
    let r = measure(DimensionKind::Radius, &[circle([0.02, 0.03], 0.006)]).unwrap();
    assert_eq!(r, 0.006);
    let d = measure(DimensionKind::Diameter, &[circle([0.02, 0.03], 0.006)]).unwrap();
    assert_eq!(d, 0.012);
}

#[test]
fn a_radius_on_an_obliquely_projected_hole_reads_its_major_radius() {
    // A circular hole seen at an angle projects to an ellipse whose MAJOR
    // radius is the hole's true radius; the minor one is foreshortened.
    let anchor = AnchorGeometry::curve(LayoutCurve::Ellipse {
        center: [0.0, 0.0],
        major_axis: [1.0, 0.0],
        major_radius: 0.005,
        minor_radius: 0.0025,
        start_param: 0.0,
        end_param: std::f64::consts::TAU,
    });
    assert_eq!(measure(DimensionKind::Radius, &[anchor]).unwrap(), 0.005);
}

#[test]
fn a_radius_on_a_straight_edge_refuses() {
    let err = measure(DimensionKind::Radius, &[line([0.0, 0.0], [0.01, 0.0])]).unwrap_err();
    assert!(matches!(err, MeasureError::NotMeasurable { .. }), "{err:?}");
}

#[test]
fn a_zero_radius_circle_is_a_degenerate_refusal_not_a_zero_dimension() {
    let err = measure(DimensionKind::Radius, &[circle([0.0, 0.0], 0.0)]).unwrap_err();
    assert!(matches!(err, MeasureError::Degenerate { .. }), "{err:?}");
}

#[test]
fn an_ordinate_dimension_reads_one_signed_coordinate() {
    let anchor = AnchorGeometry::point([-0.012, 0.034]);
    assert_eq!(
        measure(
            DimensionKind::Ordinate {
                axis: OrdinateAxis::U
            },
            std::slice::from_ref(&anchor)
        )
        .unwrap(),
        -0.012
    );
    assert_eq!(
        measure(
            DimensionKind::Ordinate {
                axis: OrdinateAxis::V
            },
            &[anchor]
        )
        .unwrap(),
        0.034
    );
}

#[test]
fn a_sampled_polyline_has_no_witness_point_and_refuses() {
    let poly = AnchorGeometry::curve(LayoutCurve::Polyline {
        points: vec![[0.0, 0.0], [0.01, 0.001], [0.02, 0.0]],
        closed: false,
    });
    let err = measure(
        DimensionKind::Distance,
        &[AnchorGeometry::point([0.0, 0.0]), poly],
    )
    .unwrap_err();
    assert!(matches!(err, MeasureError::NotMeasurable { .. }), "{err:?}");
}

#[test]
fn the_wrong_number_of_anchors_is_caught_before_any_geometry_is_touched() {
    let err = measure(
        DimensionKind::Distance,
        &[AnchorGeometry::point([0.0, 0.0])],
    )
    .unwrap_err();
    assert_eq!(
        err,
        MeasureError::WrongArity {
            kind: "Distance",
            expected: 2,
            got: 1
        }
    );
    let err = measure(
        DimensionKind::Radius,
        &[circle([0.0, 0.0], 0.01), circle([0.0, 0.0], 0.02)],
    )
    .unwrap_err();
    assert!(
        matches!(
            err,
            MeasureError::WrongArity {
                expected: 1,
                got: 2,
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn a_zero_length_line_is_degenerate_rather_than_a_zero_distance() {
    let err = measure(
        DimensionKind::PointLineDistance,
        &[
            AnchorGeometry::point([0.0, 0.01]),
            line([0.0, 0.0], [0.0, 0.0]),
        ],
    )
    .unwrap_err();
    assert!(matches!(err, MeasureError::Degenerate { .. }), "{err:?}");

    // And as either anchor of an aligned distance. Before this was checked,
    // the zero-length line had no `direction()`, the parallel branch declined
    // it, and the fallback measured 10 mm from its midpoint to the other
    // line's — a plausible number for a malformed anchor.
    for anchors in [
        [
            line([0.0, 0.0], [0.0, 0.0]),
            line([0.01, 0.0], [0.01, 0.02]),
        ],
        [
            line([0.01, 0.0], [0.01, 0.02]),
            line([0.0, 0.0], [0.0, 0.0]),
        ],
    ] {
        let err = measure(DimensionKind::Distance, &anchors).unwrap_err();
        assert!(matches!(err, MeasureError::Degenerate { .. }), "{err:?}");
    }
}

#[test]
fn an_obtuse_pair_of_edges_reads_its_acute_supplement() {
    // Two edges 120° apart. The projection carries no traversal direction, so
    // 120° and 60° are the same undirected pair and the acute one is the only
    // answer available — `angle_between`'s documented contract, pinned with a
    // hand-computed value rather than left to the reversed-30° case to imply.
    let v = measure(
        DimensionKind::Angle,
        &[
            line([0.0, 0.0], [0.01, 0.0]),
            line(
                [0.0, 0.0],
                [
                    0.01 * (120.0_f64).to_radians().cos(),
                    0.01 * (120.0_f64).to_radians().sin(),
                ],
            ),
        ],
    )
    .unwrap();
    assert!((v.to_degrees() - 60.0).abs() < 1e-12, "{v}");
}

#[test]
fn two_parallel_edges_measure_a_zero_angle_rather_than_refusing() {
    // The acute angle between two parallel edges IS zero, so there is nothing
    // to refuse: this is a true measurement of a dimension no drafter wants.
    // Pinned because the SVG layout relies on it — with no apex to swing an
    // arc about it draws nothing and reports the omission, and that contract
    // would break silently if this ever became an error instead.
    let v = measure(
        DimensionKind::Angle,
        &[
            line([0.0, 0.0], [0.01, 0.0]),
            line([0.0, 0.02], [0.03, 0.02]),
        ],
    )
    .unwrap();
    assert_eq!(v, 0.0);
}

#[test]
fn a_non_finite_coordinate_cannot_become_a_dimension() {
    // The one place a NaN could reach a drawing: an upstream anchor that
    // resolved to garbage. It must not be rendered as a number.
    let err = measure(
        DimensionKind::HDistance,
        &[
            AnchorGeometry::point([f64::NAN, 0.0]),
            AnchorGeometry::point([0.01, 0.0]),
        ],
    )
    .unwrap_err();
    assert!(matches!(err, MeasureError::Degenerate { .. }), "{err:?}");
}

#[test]
fn every_error_arm_names_the_dimension_kind_in_its_message() {
    let err = measure(DimensionKind::Radius, &[line([0.0, 0.0], [0.01, 0.0])]).unwrap_err();
    assert!(err.to_string().contains("Radius"), "{err}");
    let err = measure(
        DimensionKind::Ordinate {
            axis: OrdinateAxis::U,
        },
        &[],
    )
    .unwrap_err();
    assert!(err.to_string().contains("Ordinate"), "{err}");
}
