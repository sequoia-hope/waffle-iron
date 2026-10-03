//! Tests for the layout handoff record: the `Curve2` mirror, the witness
//! points a dimension measures from, and the JSON the app's renderer reads.

use super::*;
use cad_primitives::Point2;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

#[test]
fn every_curve2_arm_has_a_layout_twin_that_keeps_its_numbers() {
    // The pin that keeps `LayoutCurve` arm-for-arm with `Curve2`. A new
    // `Curve2` arm makes `from_curve2`'s match non-exhaustive, which is a
    // compile error; this test is the second half — that each arm carries its
    // data across unchanged rather than collapsing into a polyline.
    let cases: Vec<(Curve2, LayoutCurve)> = vec![
        (
            Curve2::Point(p(1.0, 2.0)),
            LayoutCurve::Point { at: [1.0, 2.0] },
        ),
        (
            Curve2::Line {
                start: p(0.0, 0.0),
                end: p(3.0, 4.0),
            },
            LayoutCurve::Line {
                start: [0.0, 0.0],
                end: [3.0, 4.0],
            },
        ),
        (
            Curve2::Circle {
                center: p(1.0, 1.0),
                radius: 0.5,
                start_angle: 0.25,
                end_angle: 2.0,
            },
            LayoutCurve::Circle {
                center: [1.0, 1.0],
                radius: 0.5,
                start_angle: 0.25,
                end_angle: 2.0,
            },
        ),
        (
            Curve2::Ellipse {
                center: p(-1.0, 2.0),
                major_axis: [0.6, 0.8],
                major_radius: 2.0,
                minor_radius: 1.0,
                start_param: 0.1,
                end_param: 1.2,
            },
            LayoutCurve::Ellipse {
                center: [-1.0, 2.0],
                major_axis: [0.6, 0.8],
                major_radius: 2.0,
                minor_radius: 1.0,
                start_param: 0.1,
                end_param: 1.2,
            },
        ),
        (
            Curve2::Polyline {
                points: vec![p(0.0, 0.0), p(1.0, 0.5)],
                closed: true,
            },
            LayoutCurve::Polyline {
                points: vec![[0.0, 0.0], [1.0, 0.5]],
                closed: true,
            },
        ),
    ];
    assert_eq!(cases.len(), 5, "Curve2 has five arms");
    for (curve, expected) in cases {
        assert_eq!(LayoutCurve::from_curve2(&curve), expected, "{curve:?}");
    }
}

#[test]
fn a_witness_point_is_a_lines_midpoint_and_a_circles_centre() {
    assert_eq!(
        LayoutCurve::Line {
            start: [0.0, 0.0],
            end: [0.04, 0.02],
        }
        .witness_point(),
        Some([0.02, 0.01])
    );
    assert_eq!(
        LayoutCurve::Circle {
            center: [0.01, 0.02],
            radius: 0.005,
            start_angle: 0.0,
            end_angle: std::f64::consts::TAU,
        }
        .witness_point(),
        Some([0.01, 0.02])
    );
    assert_eq!(
        LayoutCurve::Point { at: [1.0, 2.0] }.witness_point(),
        Some([1.0, 2.0])
    );
}

#[test]
fn a_polyline_has_no_witness_point() {
    assert_eq!(
        LayoutCurve::Polyline {
            points: vec![[0.0, 0.0], [1.0, 0.0]],
            closed: false,
        }
        .witness_point(),
        None
    );
}

#[test]
fn only_a_circle_or_an_ellipse_reports_a_radius() {
    assert_eq!(
        LayoutCurve::Circle {
            center: [0.0, 0.0],
            radius: 0.007,
            start_angle: 0.0,
            end_angle: 1.0,
        }
        .radius(),
        Some(0.007)
    );
    assert_eq!(
        LayoutCurve::Ellipse {
            center: [0.0, 0.0],
            major_axis: [1.0, 0.0],
            major_radius: 0.009,
            minor_radius: 0.004,
            start_param: 0.0,
            end_param: 1.0,
        }
        .radius(),
        Some(0.009)
    );
    assert_eq!(
        LayoutCurve::Line {
            start: [0.0, 0.0],
            end: [1.0, 0.0],
        }
        .radius(),
        None
    );
    assert_eq!(LayoutCurve::Point { at: [0.0, 0.0] }.radius(), None);
}

#[test]
fn a_direction_is_unit_length_and_only_a_line_has_one() {
    let d = LayoutCurve::Line {
        start: [1.0, 1.0],
        end: [4.0, 5.0],
    }
    .direction()
    .unwrap();
    assert!(
        (d[0] - 0.6).abs() < 1e-15 && (d[1] - 0.8).abs() < 1e-15,
        "{d:?}"
    );
    assert_eq!(
        LayoutCurve::Line {
            start: [1.0, 1.0],
            end: [1.0, 1.0],
        }
        .direction(),
        None,
        "a zero-length line has no direction"
    );
    assert_eq!(LayoutCurve::Point { at: [0.0, 0.0] }.direction(), None);
}

#[test]
fn a_view_layout_mirrors_the_projected_view_including_its_tags_and_box() {
    let view = ViewGeometry::new(vec![
        ProjectedCurve {
            geometry: Curve2::Line {
                start: p(0.0, 0.0),
                end: p(0.04, 0.0),
            },
            visibility: Visibility::Visible,
            kind: CurveKind::Edge,
            source: None,
            depth: None,
        },
        ProjectedCurve {
            geometry: Curve2::Line {
                start: p(0.0, 0.02),
                end: p(0.04, 0.02),
            },
            visibility: Visibility::Hidden,
            kind: CurveKind::Silhouette,
            source: None,
            depth: None,
        },
    ]);
    let layout = ViewLayout::from_view(&view);
    assert_eq!(layout.curves.len(), 2);
    assert_eq!(layout.curves[0].visibility, Visibility::Visible);
    assert_eq!(layout.curves[0].kind, CurveKind::Edge);
    assert_eq!(layout.curves[1].visibility, Visibility::Hidden);
    assert_eq!(layout.curves[1].kind, CurveKind::Silhouette);
    assert_eq!(layout.bbox, Some([[0.0, 0.0], [0.04, 0.02]]));
    assert!(layout.annotations.is_empty());
}

#[test]
fn an_empty_view_has_no_bounding_box_rather_than_a_zero_one() {
    let layout = ViewLayout::from_view(&ViewGeometry::default());
    assert!(layout.curves.is_empty());
    assert_eq!(layout.bbox, None);
    let json = serde_json::to_value(&layout).unwrap();
    assert!(!json.as_object().unwrap().contains_key("bbox"), "{json}");
}

#[test]
fn a_layout_round_trips_through_json_with_every_annotation_arm() {
    let layout = ViewLayout::from_view(&ViewGeometry::new(vec![ProjectedCurve {
        geometry: Curve2::Circle {
            center: p(0.0, 0.0),
            radius: 0.005,
            start_angle: 0.0,
            end_angle: std::f64::consts::TAU,
        },
        visibility: Visibility::Visible,
        kind: CurveKind::Edge,
        source: None,
        depth: None,
    }]))
    .with_annotations(vec![
        AnnotationLayout::Dimension {
            kind: DimensionKind::Diameter,
            anchors: vec![AnchorGeometry::curve(LayoutCurve::Circle {
                center: [0.0, 0.0],
                radius: 0.005,
                start_angle: 0.0,
                end_angle: std::f64::consts::TAU,
            })],
            value: 0.01,
            precision: Some(2),
            dual_unit: Some("in".into()),
            placement: Placement2::new(0.001, 0.002),
        },
        AnnotationLayout::Note {
            text: "2 HOLES".into(),
            leader: Some(AnchorGeometry::point([0.0, 0.0])),
            placement: Placement2::default(),
        },
        AnnotationLayout::CentreMark {
            at: [0.0, 0.0],
            half_size: 0.0065,
        },
        AnnotationLayout::CentreLine {
            from: [-0.02, 0.0],
            to: [0.02, 0.0],
        },
        AnnotationLayout::Datum {
            label: "A".into(),
            anchor: AnchorGeometry::point([0.0, -0.005]),
            placement: Placement2::default(),
        },
    ]);
    let json = serde_json::to_string(&layout).unwrap();
    let back: ViewLayout = serde_json::from_str(&json).unwrap();
    assert_eq!(back, layout);
    assert_eq!(back.annotations.len(), 5);
}

#[test]
fn the_layout_record_carries_no_geom_ref_and_no_expression() {
    // The architectural claim the record exists to make: a renderer reading
    // it cannot reach the model, so it has no way to draw a value other than
    // the measured one. Checked on the serialized form, which is what
    // actually crosses to the app.
    let layout = ViewLayout::default().with_annotations(vec![AnnotationLayout::Dimension {
        kind: DimensionKind::Distance,
        anchors: vec![
            AnchorGeometry::point([0.0, 0.0]),
            AnchorGeometry::point([0.04, 0.0]),
        ],
        value: 0.04,
        precision: None,
        dual_unit: None,
        placement: Placement2::default(),
    }]);
    let json = serde_json::to_string(&layout).unwrap();
    for forbidden in [
        "selector",
        "anchor\":{\"type\":\"FeatureOutput",
        "Expr",
        "signature",
    ] {
        assert!(!json.contains(forbidden), "{forbidden} in {json}");
    }
    assert!(json.contains("\"value\":0.04"), "{json}");
}
