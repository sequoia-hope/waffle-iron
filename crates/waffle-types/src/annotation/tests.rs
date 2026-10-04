//! Unit tests for the annotation document model (`specs/drawings_and_mbd.md`
//! §7). The value computation is tested in `measure/tests.rs` and the
//! handoff record in `layout/tests.rs`.

use super::*;
use crate::geom_ref::{Anchor, OutputKey, ResolvePolicy, Selector};
use crate::topo::TopoKind;
use uuid::Uuid;

/// A pid-selected reference — what an annotation anchor should be (D0 item 4).
fn pid_ref(kind: TopoKind, pid: u64) -> GeomRef {
    GeomRef {
        kind,
        anchor: Anchor::FeatureOutput {
            feature_id: Uuid::from_u128(0x1111_2222_3333_4444_5555_6666_7777_8888),
            output_key: OutputKey::Main,
        },
        selector: Selector::Pid { pid, root_pid: pid },
        policy: ResolvePolicy::Strict,
        scope: None,
    }
}

fn linear(a: u64, b: u64) -> Annotation {
    Annotation::Dimension {
        kind: DimensionKind::Distance,
        anchors: vec![pid_ref(TopoKind::Edge, a), pid_ref(TopoKind::Edge, b)],
        value: Measured::FromGeometry,
        precision: Some(2),
        tolerance: None,
        dual_precision: None,
        dual_unit: None,
        placement: Placement2::default(),
    }
}

#[test]
fn a_fresh_dimension_measures_from_geometry_and_stores_no_number() {
    let Annotation::Dimension { value, .. } = linear(7, 9) else {
        panic!("not a dimension");
    };
    assert_eq!(value, Measured::FromGeometry);
    assert_eq!(Measured::default(), Measured::FromGeometry);
}

#[test]
fn the_seven_sketch_kinds_plus_ordinate_have_the_arity_they_measure() {
    // §7: "`DimensionKind` reuses the seven sketch dimension kinds, with
    // `Ordinate` added." Seven two-or-one-anchor kinds plus Ordinate = 8.
    let all = [
        DimensionKind::Distance,
        DimensionKind::PointLineDistance,
        DimensionKind::HDistance,
        DimensionKind::VDistance,
        DimensionKind::Angle,
        DimensionKind::Radius,
        DimensionKind::Diameter,
        DimensionKind::Ordinate {
            axis: OrdinateAxis::U,
        },
    ];
    assert_eq!(all.len(), 8, "seven sketch kinds plus Ordinate");
    for kind in all {
        assert!(
            kind.arity() == 1 || kind.arity() == 2,
            "{kind:?} arity {}",
            kind.arity()
        );
    }
    assert_eq!(DimensionKind::Radius.arity(), 1);
    assert_eq!(DimensionKind::Diameter.arity(), 1);
    assert_eq!(
        DimensionKind::Ordinate {
            axis: OrdinateAxis::V
        }
        .arity(),
        1
    );
    assert_eq!(DimensionKind::Distance.arity(), 2);
    assert_eq!(DimensionKind::Angle.arity(), 2);
}

#[test]
fn only_the_angle_kind_is_angular() {
    assert!(DimensionKind::Angle.is_angular());
    for kind in [
        DimensionKind::Distance,
        DimensionKind::PointLineDistance,
        DimensionKind::HDistance,
        DimensionKind::VDistance,
        DimensionKind::Radius,
        DimensionKind::Diameter,
        DimensionKind::Ordinate {
            axis: OrdinateAxis::U,
        },
    ] {
        assert!(!kind.is_angular(), "{kind:?}");
    }
}

#[test]
fn anchors_are_reported_in_order_for_every_variant() {
    let d = linear(7, 9);
    let got: Vec<u64> = d.anchors().iter().map(selector_pid).collect();
    assert_eq!(got, vec![7, 9]);

    let note = Annotation::Note {
        text: "DEBURR".into(),
        leader: Some(pid_ref(TopoKind::Face, 3)),
        placement: Placement2::new(0.002, -0.001),
    };
    assert_eq!(
        note.anchors().iter().map(selector_pid).collect::<Vec<_>>(),
        vec![3]
    );

    let bare = Annotation::Note {
        text: "GENERAL NOTE".into(),
        leader: None,
        placement: Placement2::default(),
    };
    assert!(
        bare.anchors().is_empty(),
        "a note with no leader anchors nothing"
    );

    let mark = Annotation::CentreMark {
        anchor: pid_ref(TopoKind::Edge, 11),
    };
    assert_eq!(
        mark.anchors().iter().map(selector_pid).collect::<Vec<_>>(),
        vec![11]
    );

    let cl = Annotation::CentreLine {
        anchors: [pid_ref(TopoKind::Edge, 11), pid_ref(TopoKind::Edge, 12)],
    };
    assert_eq!(
        cl.anchors().iter().map(selector_pid).collect::<Vec<_>>(),
        vec![11, 12]
    );

    let datum = Annotation::Datum {
        label: "A".into(),
        anchor: pid_ref(TopoKind::Face, 5),
        placement: Placement2::default(),
    };
    assert_eq!(
        datum.anchors().iter().map(selector_pid).collect::<Vec<_>>(),
        vec![5]
    );
}

fn selector_pid(r: &&GeomRef) -> u64 {
    match r.selector {
        Selector::Pid { pid, .. } => pid,
        ref other => panic!("an annotation anchor must be a Pid selector, got {other:?}"),
    }
}

#[test]
fn only_the_label_bearing_variants_have_a_placement() {
    assert!(linear(1, 2).placement().is_some());
    assert!(
        Annotation::Note {
            text: "x".into(),
            leader: None,
            placement: Placement2::new(1.0, 2.0),
        }
        .placement()
            == Some(Placement2::new(1.0, 2.0))
    );
    assert!(Annotation::CentreMark {
        anchor: pid_ref(TopoKind::Edge, 1)
    }
    .placement()
    .is_none());
    assert!(Annotation::CentreLine {
        anchors: [pid_ref(TopoKind::Edge, 1), pid_ref(TopoKind::Edge, 2)],
    }
    .placement()
    .is_none());
}

#[test]
fn a_zero_placement_is_the_default_and_says_so() {
    assert!(Placement2::default().is_zero());
    assert!(!Placement2::new(0.0, 1e-9).is_zero());
    assert_eq!(Placement2::new(1.5, -2.5).dx, 1.5);
    assert_eq!(Placement2::new(1.5, -2.5).dy, -2.5);
}

#[test]
fn an_annotation_round_trips_through_json() {
    for annotation in [
        linear(7, 9),
        Annotation::Dimension {
            kind: DimensionKind::Ordinate {
                axis: OrdinateAxis::V,
            },
            anchors: vec![pid_ref(TopoKind::Vertex, 4)],
            value: Measured::Expr {
                expr: "distance(a, b) * 2".into(),
            },
            precision: None,
            tolerance: None,
            dual_precision: None,
            dual_unit: Some("in".into()),
            placement: Placement2::new(0.001, 0.002),
        },
        Annotation::Note {
            text: "BREAK SHARP EDGES".into(),
            leader: None,
            placement: Placement2::default(),
        },
        Annotation::CentreMark {
            anchor: pid_ref(TopoKind::Edge, 11),
        },
        Annotation::CentreLine {
            anchors: [pid_ref(TopoKind::Edge, 11), pid_ref(TopoKind::Edge, 12)],
        },
        Annotation::Datum {
            label: "A".into(),
            anchor: pid_ref(TopoKind::Face, 5),
            placement: Placement2::default(),
        },
    ] {
        let json = serde_json::to_string(&annotation).unwrap();
        let back: Annotation = serde_json::from_str(&json).unwrap();
        // Compared by serialized form — see the note on `Annotation`'s derive.
        assert_eq!(serde_json::to_string(&back).unwrap(), json);
    }
}

#[test]
fn the_optional_display_fields_are_omitted_when_absent() {
    // Keeps a `.waffle` file from growing a `"precision": null` on every
    // dimension once D4a persists these.
    let json = serde_json::to_value(linear(1, 2)).unwrap();
    let obj = json.as_object().unwrap();
    assert_eq!(obj["type"], "Dimension");
    assert_eq!(obj["precision"], 2);
    assert!(!obj.contains_key("dual_unit"), "{json}");
}

#[test]
fn a_dimension_with_the_display_fields_missing_still_reads() {
    // Forward-compatibility in the direction that matters: every display
    // field defaults, so a writer that omits them all produces a readable
    // annotation.
    let json = serde_json::json!({
        "type": "Dimension",
        "kind": { "type": "Radius" },
        "anchors": [],
    });
    let back: Annotation = serde_json::from_value(json).unwrap();
    let Annotation::Dimension {
        value,
        precision,
        dual_unit,
        placement,
        ..
    } = back
    else {
        panic!("not a dimension");
    };
    assert_eq!(value, Measured::FromGeometry);
    assert_eq!(precision, None);
    assert_eq!(dual_unit, None);
    assert!(placement.is_zero());
}
