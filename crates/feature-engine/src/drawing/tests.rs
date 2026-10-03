//! Unit tests for the drawing document model and the rebuild's refusals.
//!
//! The rebuild's HAPPY path needs a kernel that can project, which
//! `MockKernel` deliberately cannot (`KernelProjection`'s every method
//! defaults to `NotSupported`, "a trivial answer from a test double would be
//! indistinguishable from a working projection of an empty solid"). That half
//! is pinned on real geometry in `crates/test-harness/tests/d4a_drawing.rs`.
//! What lives here is everything that is a function of the document: the frame
//! algebra, the paper layout, the validation, and the refusals the rebuild
//! makes before it ever asks the kernel.

use super::*;
use waffle_types::annotation::Placement2;
use waffle_types::geom_ref::{Anchor, OutputKey, ResolvePolicy};
use waffle_types::kernel::KernelSolidHandle;

/// Is `a` the same frame as `b`, up to the length of its vectors?
fn same_frame(a: &ViewFrame, b: &ViewFrame) -> bool {
    let (Some(x), Some(y)) = (a.basis(), b.basis()) else {
        return false;
    };
    let close = |p: [f64; 3], q: [f64; 3]| (0..3).all(|i| (p[i] - q[i]).abs() < 1e-12);
    close(x.u, y.u) && close(x.v, y.v) && close(x.w, y.w)
}

#[test]
fn every_named_view_is_orientable_and_the_three_the_kernel_names_are_its_own() {
    for named in NamedView::ALL {
        let frame = named.frame();
        assert!(frame.basis().is_some(), "{} has no view basis", named.tag());
    }
    // Not a second definition of the three the kernel already states.
    assert_eq!(NamedView::Top.frame(), ViewFrame::TOP);
    assert_eq!(NamedView::Front.frame(), ViewFrame::FRONT);
    assert_eq!(NamedView::Right.frame(), ViewFrame::RIGHT);
}

#[test]
fn every_named_view_shares_a_paper_axis_with_the_front_view_it_is_grouped_with() {
    // The property that makes a six-view layout readable: the three views in
    // the horizontal row (left, front, right, back) all have world +z up on
    // paper, and the three in the vertical column (top, front, bottom) all
    // have world +x to the right. A view whose up was picked carelessly
    // breaks this and comes out mirrored against its neighbours — which is
    // how the bottom view is usually got wrong.
    for named in [
        NamedView::Left,
        NamedView::Front,
        NamedView::Right,
        NamedView::Back,
    ] {
        let basis = named.frame().basis().unwrap();
        assert!(
            (basis.v[2] - 1.0).abs() < 1e-12,
            "{}: paper up is {:?}, not world +z",
            named.tag(),
            basis.v
        );
    }
    for named in [NamedView::Top, NamedView::Front, NamedView::Bottom] {
        let basis = named.frame().basis().unwrap();
        assert!(
            (basis.u[0] - 1.0).abs() < 1e-12,
            "{}: paper right is {:?}, not world +x",
            named.tag(),
            basis.u
        );
    }
}

#[test]
fn a_third_angle_projection_from_the_front_view_is_the_named_view_of_that_side() {
    // The whole of the third-angle rule, derived rather than recorded: the
    // view placed on a side SHOWS that side. So projecting the front view to
    // the right must give exactly the frame `NamedView::Right` states, and
    // first angle — which puts the far side's view on the near side — must
    // give its opposite.
    let front = ViewFrame::FRONT.basis().unwrap();
    let third = [
        (ProjectedDirection::Right, NamedView::Right),
        (ProjectedDirection::Left, NamedView::Left),
        (ProjectedDirection::Up, NamedView::Top),
        (ProjectedDirection::Down, NamedView::Bottom),
    ];
    for (placement, expected) in third {
        let got = projected_frame(&front, placement, ProjectionAngle::Third);
        assert!(
            same_frame(&got, &expected.frame()),
            "third angle {placement:?} of the front view should be {}: got {got:?}",
            expected.tag()
        );
    }
    let first = [
        (ProjectedDirection::Right, NamedView::Left),
        (ProjectedDirection::Left, NamedView::Right),
        (ProjectedDirection::Up, NamedView::Bottom),
        (ProjectedDirection::Down, NamedView::Top),
    ];
    for (placement, expected) in first {
        let got = projected_frame(&front, placement, ProjectionAngle::First);
        assert!(
            same_frame(&got, &expected.frame()),
            "first angle {placement:?} of the front view should be {}: got {got:?}",
            expected.tag()
        );
    }
}

#[test]
fn a_projection_of_a_projection_composes_to_the_right_frame() {
    // A right view projected UP from the right view of the front: the chain
    // is followed, so this is the top view seen with the right view's own
    // horizontal axis — not the top view of the front. Checked by composing
    // the rule twice rather than by naming the answer.
    let mut sheet = Sheet::new("S");
    let front = DrawingView::new(
        "Front",
        ViewSource::whole_tab("t"),
        Projection::Named {
            view: NamedView::Front,
        },
    );
    let right = DrawingView::new(
        "Right",
        ViewSource::whole_tab("t"),
        Projection::ProjectedFrom {
            parent: front.id,
            direction: ProjectedDirection::Right,
        },
    );
    let top_of_right = DrawingView::new(
        "TopOfRight",
        ViewSource::whole_tab("t"),
        Projection::ProjectedFrom {
            parent: right.id,
            direction: ProjectedDirection::Up,
        },
    );
    let (right_id, chained_id) = (right.id, top_of_right.id);
    sheet.views = vec![front, right, top_of_right];

    let right_frame = sheet
        .view_frame(right_id, ProjectionAngle::Third)
        .expect("the right view's frame");
    let expected = projected_frame(
        &right_frame.basis().unwrap(),
        ProjectedDirection::Up,
        ProjectionAngle::Third,
    );
    let got = sheet
        .view_frame(chained_id, ProjectionAngle::Third)
        .expect("the chained frame");
    assert!(same_frame(&got, &expected), "{got:?} vs {expected:?}");
    // It is a top view of SOMETHING: looking straight down.
    assert!(
        (got.basis().unwrap().w[2] + 1.0).abs() < 1e-12,
        "the line of sight should be −z, got {:?}",
        got.basis().unwrap().w
    );
}

#[test]
fn a_projection_chain_that_returns_to_itself_is_refused_rather_than_followed() {
    let mut sheet = Sheet::new("S");
    let a_id = Uuid::new_v4();
    let b_id = Uuid::new_v4();
    let mut a = DrawingView::new(
        "A",
        ViewSource::whole_tab("t"),
        Projection::ProjectedFrom {
            parent: b_id,
            direction: ProjectedDirection::Right,
        },
    );
    a.id = a_id;
    let mut b = DrawingView::new(
        "B",
        ViewSource::whole_tab("t"),
        Projection::ProjectedFrom {
            parent: a_id,
            direction: ProjectedDirection::Up,
        },
    );
    b.id = b_id;
    sheet.views = vec![a, b];
    let err = sheet
        .view_frame(a_id, ProjectionAngle::Third)
        .expect_err("a cycle has no frame");
    assert!(
        matches!(err, DrawingError::ProjectionCycle { view, .. } if view == a_id),
        "{err}"
    );
}

#[test]
fn a_projected_view_whose_parent_is_not_on_the_sheet_names_the_missing_parent() {
    let mut sheet = Sheet::new("S");
    let missing = Uuid::new_v4();
    let view = DrawingView::new(
        "Right",
        ViewSource::whole_tab("t"),
        Projection::ProjectedFrom {
            parent: missing,
            direction: ProjectedDirection::Right,
        },
    );
    let id = view.id;
    sheet.views = vec![view];
    let err = sheet
        .view_frame(id, ProjectionAngle::Third)
        .expect_err("no parent, no frame");
    assert_eq!(
        err,
        DrawingError::UnknownParent {
            view: id,
            parent: missing
        }
    );
    // And the same thing is a warning, not a failure, at load time.
    let drawing = Drawing {
        sheets: vec![sheet],
        ..Drawing::default()
    };
    let warnings = drawing.validate();
    assert!(
        warnings.iter().any(|w| w.contains(&missing.to_string())),
        "{warnings:?}"
    );
}

#[test]
fn a_custom_direction_with_an_up_parallel_to_it_is_refused_as_degenerate() {
    let mut sheet = Sheet::new("S");
    let view = DrawingView::new(
        "Bad",
        ViewSource::whole_tab("t"),
        Projection::Custom {
            dir: [0.0, 0.0, -1.0],
            up: Some([0.0, 0.0, 1.0]),
        },
    );
    let id = view.id;
    sheet.views = vec![view];
    let err = sheet
        .view_frame(id, ProjectionAngle::Third)
        .expect_err("up parallel to the line of sight is no frame");
    assert!(matches!(err, DrawingError::DegenerateFrame { .. }), "{err}");
}

#[test]
fn auto_placement_clears_both_drawings_rather_than_both_centres() {
    // The gap is between the DRAWINGS. A fixed centre distance would overlap
    // two views of a long part, which was the first version's bug in the D3
    // dimension layout for the same reason.
    let parent_extent = [80.0, 20.0];
    let own_extent = [40.0, 20.0];
    let p = auto_placement_mm(
        [100.0, 100.0],
        parent_extent,
        own_extent,
        ProjectedDirection::Right,
        DEFAULT_VIEW_GAP_MM,
    );
    assert_eq!(p[1], 100.0, "a sideways projection keeps the row");
    // The near edges are exactly the gap apart.
    let parent_right = 100.0 + parent_extent[0] / 2.0;
    let own_left = p[0] - own_extent[0] / 2.0;
    assert!(
        (own_left - parent_right - DEFAULT_VIEW_GAP_MM).abs() < 1e-12,
        "edges at {parent_right} and {own_left}"
    );
    // And the four directions go the four ways.
    let centre = [0.0, 0.0];
    let square = [10.0, 10.0];
    let go = |d| auto_placement_mm(centre, square, square, d, 0.0);
    assert_eq!(go(ProjectedDirection::Right), [10.0, 0.0]);
    assert_eq!(go(ProjectedDirection::Left), [-10.0, 0.0]);
    assert_eq!(go(ProjectedDirection::Up), [0.0, 10.0]);
    assert_eq!(go(ProjectedDirection::Down), [0.0, -10.0]);
}

#[test]
fn a_sheet_size_swaps_for_orientation_but_a_custom_one_is_taken_as_authored() {
    assert_eq!(
        SheetSize::A3.extent_mm(Orientation::Landscape),
        [420.0, 297.0]
    );
    assert_eq!(
        SheetSize::A3.extent_mm(Orientation::Portrait),
        [297.0, 420.0]
    );
    assert_eq!(
        SheetSize::A4.extent_mm(Orientation::Portrait),
        [210.0, 297.0]
    );
    // A4 is half of A3 the long way, which is what makes the series a series
    // — a check that the table is the real one and not a plausible one.
    assert_eq!(
        SheetSize::A3.extent_mm(Orientation::Portrait)[1],
        SheetSize::A4.extent_mm(Orientation::Portrait)[0] * 2.0
    );
    let custom = SheetSize::Custom {
        width_mm: 500.0,
        height_mm: 100.0,
    };
    assert_eq!(custom.extent_mm(Orientation::Portrait), [500.0, 100.0]);
    assert_eq!(custom.extent_mm(Orientation::Landscape), [500.0, 100.0]);
    // The default sheet is the one a new drawing tab gets.
    let drawing = Drawing::new();
    assert_eq!(drawing.sheets.len(), 1);
    assert_eq!(drawing.sheets[0].extent_mm(), [420.0, 297.0]);
    assert_eq!(drawing.projection_angle, ProjectionAngle::Third);
}

#[test]
fn validate_reports_duplicate_ids_and_an_undrawable_scale_without_failing() {
    let mut sheet = Sheet::new("S");
    let mut a = DrawingView::new(
        "A",
        ViewSource::whole_tab("t"),
        Projection::Named {
            view: NamedView::Top,
        },
    );
    let mut b = a.clone();
    b.id = a.id;
    a.scale = 0.0;
    sheet.views = vec![a, b];
    let mut other = sheet.clone();
    other.id = sheet.id;
    let drawing = Drawing {
        sheets: vec![sheet, other],
        ..Drawing::default()
    };
    let w = drawing.validate();
    assert!(w.iter().any(|m| m.contains("duplicate sheet id")), "{w:?}");
    assert!(w.iter().any(|m| m.contains("duplicate view id")), "{w:?}");
    assert!(w.iter().any(|m| m.contains("non-positive scale")), "{w:?}");
}

#[test]
fn a_view_style_decides_per_curve_and_defaults_to_the_drafting_default() {
    let on = ViewStyle::default();
    assert!(on.hidden_lines && on.silhouettes);
    assert!(on.draws(CurveKind::Edge, Visibility::Hidden));
    assert!(on.draws(CurveKind::Silhouette, Visibility::Visible));
    let off = ViewStyle {
        hidden_lines: false,
        silhouettes: false,
    };
    assert!(off.draws(CurveKind::Edge, Visibility::Visible));
    assert!(!off.draws(CurveKind::Edge, Visibility::Hidden));
    assert!(!off.draws(CurveKind::Silhouette, Visibility::Visible));
}

#[test]
fn a_literal_dimension_value_is_refused_and_so_is_an_unevaluated_expression() {
    // §7's open item "nothing refuses `Measured::Value`", closed. A drawing
    // whose number was typed in is the exact failure the increment exists to
    // prevent, and `Measured::Expr` needs D2's measurement functions — which
    // do not exist, so measuring it from geometry instead would print a
    // different number than the one authored.
    let view = Uuid::new_v4();
    assert_eq!(check_measured(view, 0, &Measured::FromGeometry), Ok(()));
    let err = check_measured(view, 3, &Measured::Value { value: 0.04 }).unwrap_err();
    assert!(
        matches!(err, DrawingError::LiteralValue { index: 3, .. }),
        "{err}"
    );
    assert!(err.to_string().contains("never typed in"), "{err}");
    let err = check_measured(
        view,
        1,
        &Measured::Expr {
            expr: "width * 2".to_string(),
        },
    )
    .unwrap_err();
    assert!(
        matches!(err, DrawingError::ExprNotEvaluated { ref expr, .. } if expr == "width * 2"),
        "{err}"
    );
}

/// An annotation anchored on `selector`, for the refusal tests.
fn dimension_with(selector: Selector) -> Annotation {
    Annotation::Dimension {
        kind: DimensionKind::Radius,
        anchors: vec![GeomRef {
            kind: TopoKind::Edge,
            anchor: Anchor::FeatureOutput {
                feature_id: Uuid::nil(),
                output_key: OutputKey::Main,
            },
            selector,
            policy: ResolvePolicy::Strict,
            scope: None,
        }],
        value: Measured::FromGeometry,
        precision: None,
        dual_unit: None,
        placement: Placement2::default(),
    }
}

#[test]
fn a_view_whose_scale_cannot_be_drawn_is_refused_before_the_kernel_is_asked() {
    // The order matters: a zero scale is a document error and must not be
    // reported as "the kernel could not project", which is what the caller
    // would be told if the projection ran first.
    let kernel = waffle_types::kernel::MockKernel::new();
    let mut view = DrawingView::new(
        "V",
        ViewSource::whole_tab("t"),
        Projection::Named {
            view: NamedView::Top,
        },
    );
    view.scale = 0.0;
    let err = rebuild_view(&view, &ViewFrame::TOP, &[], &kernel).expect_err("no scale, no view");
    assert!(
        matches!(err, DrawingError::BadScale { scale, .. } if scale == 0.0),
        "{err}"
    );
}

#[test]
fn a_kernel_that_cannot_project_is_reported_with_the_view_named() {
    // `MockKernel` implements no projection method, so this is the typed
    // `NotSupported` travelling out with the view's identity attached — what
    // a sheet of eight views needs in order to say WHICH one failed.
    let kernel = waffle_types::kernel::MockKernel::new();
    let view = DrawingView::new(
        "V",
        ViewSource::whole_tab("t"),
        Projection::Named {
            view: NamedView::Top,
        },
    );
    let bodies = vec![ProjectionBody::solo(KernelSolidHandle::from_raw(0))];
    let err = rebuild_view(&view, &ViewFrame::TOP, &bodies, &kernel).expect_err("mock cannot");
    match err {
        DrawingError::ProjectionFailed { view: id, message } => {
            assert_eq!(id, view.id);
            assert!(message.contains("project_bodies"), "{message}");
        }
        other => panic!("{other}"),
    }
}

#[test]
fn an_anchor_that_is_not_a_persistent_id_is_refused_by_the_selector_it_used() {
    // Anchoring a drawing dimension by signature or position is exactly how
    // it comes to dimension a different edge after a rebuild (D0 item 4), so
    // the rebuild refuses the selector rather than resolving it. Exercised
    // through the empty-body view, where the projection succeeds trivially
    // and the anchor is the only thing left to fail.
    let kernel = waffle_types::kernel::MockKernel::new();
    let mut view = DrawingView::new(
        "V",
        ViewSource::whole_tab("t"),
        Projection::Named {
            view: NamedView::Top,
        },
    );
    view.annotations = vec![dimension_with(Selector::Position {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    })];
    // No bodies: `project_bodies` is never called, so the layout is empty and
    // the anchor refusal is reached.
    let err = rebuild_view(&view, &ViewFrame::TOP, &[], &kernel).expect_err("not a pid");
    assert!(
        matches!(
            err,
            DrawingError::AnchorNotPid {
                selector: "Selector::Position",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn an_anchor_whose_pid_is_absent_refuses_rather_than_measuring_something_else() {
    let kernel = waffle_types::kernel::MockKernel::new();
    let mut view = DrawingView::new(
        "V",
        ViewSource::whole_tab("t"),
        Projection::Named {
            view: NamedView::Top,
        },
    );
    view.annotations = vec![dimension_with(Selector::Pid {
        pid: 4242,
        root_pid: 4242,
    })];
    let err = rebuild_view(&view, &ViewFrame::TOP, &[], &kernel).expect_err("nothing to measure");
    assert!(
        matches!(
            err,
            DrawingError::AnchorUnresolved {
                pid: 4242,
                kind: TopoKind::Edge,
                ..
            }
        ),
        "{err}"
    );
    assert!(err.to_string().contains("4242"), "{err}");
}

#[test]
fn a_view_of_no_bodies_is_an_empty_sheet_rather_than_an_error() {
    // A freshly added view of a tab with nothing built yet. It draws nothing
    // and has no box — the same call `ViewGeometry::bbox` makes — and that is
    // not a failure: the drawing is simply of an empty part.
    let kernel = waffle_types::kernel::MockKernel::new();
    let view = DrawingView::new(
        "V",
        ViewSource::whole_tab("t"),
        Projection::Named {
            view: NamedView::Top,
        },
    );
    let out = rebuild_view(&view, &ViewFrame::TOP, &[], &kernel).expect("an empty view is a view");
    assert!(out.layout.curves.is_empty());
    assert_eq!(out.layout.bbox, None);
    assert_eq!(out.extent_mm, [0.0, 0.0]);
    assert_eq!(out.declines.total(), 0);
}

#[test]
fn a_view_source_selects_bodies_by_name_and_an_empty_list_means_all_of_them() {
    let all = ViewSource::whole_tab("tab-1");
    assert!(all.includes("Extrude 1") && all.includes("anything"));
    let some = ViewSource {
        tab_id: "tab-1".to_string(),
        bodies: vec!["Extrude 1".to_string()],
    };
    assert!(some.includes("Extrude 1"));
    assert!(!some.includes("Extrude 2"));
}

#[test]
fn a_drawing_round_trips_through_serde_with_its_annotations_and_unknown_keys() {
    let mut view = DrawingView::new(
        "Top",
        ViewSource::whole_tab("tab-1"),
        Projection::Named {
            view: NamedView::Top,
        },
    );
    view.scale = 0.5;
    view.placement_mm = [120.0, 90.0];
    view.annotations = vec![dimension_with(Selector::Pid {
        pid: 11,
        root_pid: 11,
    })];
    let mut sheet = Sheet::new("Sheet 1");
    sheet.views = vec![view];
    let drawing = Drawing {
        sheets: vec![sheet],
        projection_angle: ProjectionAngle::First,
        extra: Map::new(),
    };
    let json = serde_json::to_value(&drawing).expect("serializes");
    let back: Drawing = serde_json::from_value(json.clone()).expect("deserializes");
    assert_eq!(
        serde_json::to_value(&back).unwrap(),
        json,
        "a round trip must be byte-stable — the file-format corpus pin saves twice"
    );
    assert_eq!(back.projection_angle, ProjectionAngle::First);
    assert_eq!(back.sheets[0].views[0].scale, 0.5);
    assert_eq!(back.sheets[0].views[0].annotations.len(), 1);

    // A key from a newer build survives a load → save, as every other
    // document type's `extra` does (v4 §2.6).
    let mut with_extra = json;
    with_extra["x-future"] = serde_json::json!({ "title_block": "D4b" });
    let back: Drawing = serde_json::from_value(with_extra.clone()).expect("tolerates");
    assert_eq!(serde_json::to_value(&back).unwrap(), with_extra);
}

#[test]
fn an_empty_drawing_serializes_without_the_keys_it_does_not_use() {
    // What keeps a Drawing tab from inflating the document: a sheet with no
    // views, a view with no annotations and no cache write no key at all.
    let drawing = Drawing::new();
    let json = serde_json::to_value(&drawing).unwrap();
    let sheet = &json["sheets"][0];
    assert!(sheet.get("views").is_none(), "{sheet}");
    assert_eq!(sheet["size"]["type"], "A3");
}

#[test]
fn the_dimension_kind_tags_are_the_seven_sketch_kinds() {
    // §7: "`DimensionKind` reuses the seven sketch dimension kinds". The
    // authoring door offers exactly those; `Ordinate` is deliberately absent
    // until it has an origin anchor (§7's open item, D4a's to leave open).
    for tag in [
        "Distance",
        "PointLineDistance",
        "HDistance",
        "VDistance",
        "Angle",
        "Radius",
        "Diameter",
    ] {
        assert!(
            dimension_kind_from_tag(tag).is_some(),
            "{tag} should be authorable"
        );
    }
    assert!(dimension_kind_from_tag("Ordinate").is_none());
    assert!(dimension_kind_from_tag("Nonsense").is_none());
}

#[test]
fn every_annotation_variant_has_a_tag() {
    let anchor = GeomRef {
        kind: TopoKind::Edge,
        anchor: Anchor::FeatureOutput {
            feature_id: Uuid::nil(),
            output_key: OutputKey::Main,
        },
        selector: Selector::Pid {
            pid: 1,
            root_pid: 1,
        },
        policy: ResolvePolicy::Strict,
        scope: None,
    };
    let cases = [
        (
            annotation_tag(&dimension_with(Selector::Pid {
                pid: 1,
                root_pid: 1,
            })),
            "Dimension",
        ),
        (
            annotation_tag(&Annotation::Note {
                text: "x".into(),
                leader: None,
                placement: Placement2::default(),
            }),
            "Note",
        ),
        (
            annotation_tag(&Annotation::CentreMark {
                anchor: anchor.clone(),
            }),
            "CentreMark",
        ),
        (
            annotation_tag(&Annotation::CentreLine {
                anchors: [anchor.clone(), anchor.clone()],
            }),
            "CentreLine",
        ),
        (
            annotation_tag(&Annotation::Datum {
                label: "A".into(),
                anchor,
                placement: Placement2::default(),
            }),
            "Datum",
        ),
    ];
    for (got, want) in cases {
        assert_eq!(got, want);
    }
}
