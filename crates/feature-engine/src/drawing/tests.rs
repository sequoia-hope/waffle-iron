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
fn a_literal_dimension_value_is_refused_and_an_expression_is_authorable() {
    // §7's open item "nothing refuses `Measured::Value`", closed. A drawing
    // whose number was typed in is the exact failure the increment exists to
    // prevent.
    let view = Uuid::new_v4();
    assert_eq!(check_measured(view, 0, &Measured::FromGeometry), Ok(()));
    let err = check_measured(view, 3, &Measured::Value { value: 0.04 }).unwrap_err();
    assert!(
        matches!(err, DrawingError::LiteralValue { index: 3, .. }),
        "{err}"
    );
    assert!(err.to_string().contains("never typed in"), "{err}");
    // D2 moved this one: `Measured::Expr` is now a legal AUTHORED value (the
    // measurement functions exist), so the authoring boundary passes it and
    // whether it EVALUATES is the rebuild's question — see
    // `an_expression_dimension_refuses_in_a_rebuild_with_no_environment`
    // below. Before D2 this was refused here, which was the honest answer
    // when nothing could evaluate one.
    assert_eq!(
        check_measured(
            view,
            1,
            &Measured::Expr {
                expr: "width * 2".to_string(),
            },
        ),
        Ok(())
    );
}

#[test]
fn an_expression_dimension_refuses_in_a_rebuild_with_no_environment() {
    // D2: the value of a `Measured::Expr` dimension comes from the
    // EXPRESSION, so a rebuild with nothing to evaluate it against must say
    // so by name. Falling back to what the anchors measure would print a
    // different number than the one authored, silently.
    let kernel = waffle_types::kernel::MockKernel::new();
    let mut view = DrawingView::new(
        "Front",
        ViewSource::whole_tab("t"),
        Projection::Named {
            view: NamedView::Front,
        },
    );
    let mut annotation = dimension_with(Selector::Pid {
        pid: 7,
        root_pid: 7,
    });
    if let Annotation::Dimension { value, .. } = &mut annotation {
        *value = Measured::Expr {
            expr: "bore / 2".to_string(),
        };
    }
    view.annotations.push(annotation);
    let out = rebuild_view(&view, &ViewFrame::TOP, &[], &kernel, None)
        .expect("the view still builds — one annotation is not the view");
    let (index, err) = out
        .annotation_errors
        .first()
        .expect("the expression dimension is reported");
    assert_eq!(*index, 0);
    assert!(
        matches!(err, DrawingError::ExprNotEvaluated { ref expr, .. } if expr == "bore / 2"),
        "{err}"
    );
    assert!(
        err.to_string().contains("no expression environment"),
        "{err}"
    );
    // The loud half: the annotation is NOT laid out. A dimension whose number
    // could not be produced must leave nothing on the sheet for a renderer to
    // draw a blank or a stale value into — the error IS the output.
    assert!(
        out.layout.annotations.is_empty(),
        "{:?}",
        out.layout.annotations
    );
}

#[test]
fn an_expression_dimension_whose_expression_fails_is_loud_and_draws_nothing() {
    // The other half of D2's expression dimension, and the one that happens
    // in a real document: the environment IS there, and the expression is
    // broken — a vanished entity name, a parameter that is gone, an angle
    // where a length belongs. The authoring tool refuses such an annotation
    // outright, so this is the case where it WORKED and the model moved
    // underneath it.
    //
    // What must not happen is a number: not a blank, not the value from the
    // last rebuild, and not what the anchors happen to measure. The
    // annotation is dropped from the layout and the reason is reported
    // against its index.
    struct Broken;
    impl ExprDimensions for Broken {
        fn value_of(&self, expression: &str, _kind: DimensionKind) -> Result<f64, String> {
            Err(format!(
                "radius(\"rim\"): the name does not resolve ({expression})"
            ))
        }
        fn text_of(&self, expression: &str) -> Result<String, String> {
            Err(format!(
                "radius(\"rim\"): the name does not resolve ({expression})"
            ))
        }
    }
    let kernel = waffle_types::kernel::MockKernel::new();
    let mut view = DrawingView::new(
        "Front",
        ViewSource::whole_tab("t"),
        Projection::Named {
            view: NamedView::Front,
        },
    );
    let mut annotation = dimension_with(Selector::Pid {
        pid: 7,
        root_pid: 7,
    });
    if let Annotation::Dimension { value, .. } = &mut annotation {
        *value = Measured::Expr {
            expr: "radius(rim) * 2".to_string(),
        };
    }
    view.annotations.push(annotation);
    let out = rebuild_view(&view, &ViewFrame::TOP, &[], &kernel, Some(&Broken))
        .expect("the view still builds — one annotation is not the view");
    let [(index, err)] = out.annotation_errors.as_slice() else {
        panic!("{:?}", out.annotation_errors);
    };
    assert_eq!(*index, 0);
    assert!(
        matches!(err, DrawingError::ExprFailed { ref expr, .. } if expr == "radius(rim) * 2"),
        "{err}"
    );
    // The message carries BOTH halves an author needs: the expression, and
    // why it failed.
    let message = err.to_string();
    assert!(message.contains("radius(rim) * 2"), "{message}");
    assert!(message.contains("does not resolve"), "{message}");
    assert!(
        out.layout.annotations.is_empty(),
        "a failed expression dimension draws nothing: {:?}",
        out.layout.annotations
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
    let err =
        rebuild_view(&view, &ViewFrame::TOP, &[], &kernel, None).expect_err("no scale, no view");
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
    let err =
        rebuild_view(&view, &ViewFrame::TOP, &bodies, &kernel, None).expect_err("mock cannot");
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
    let out =
        rebuild_view(&view, &ViewFrame::TOP, &[], &kernel, None).expect("the view still builds");
    // The annotation fails; the VIEW does not. A dimension whose anchor is
    // unusable must not blank the sheet (see `ViewRebuild::annotation_errors`).
    assert!(out.layout.annotations.is_empty());
    assert!(
        matches!(
            out.annotation_errors.as_slice(),
            [(
                0,
                DrawingError::AnchorNotPid {
                    selector: "Selector::Position",
                    ..
                }
            )]
        ),
        "{:?}",
        out.annotation_errors
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
    let out =
        rebuild_view(&view, &ViewFrame::TOP, &[], &kernel, None).expect("the view still builds");
    assert!(out.layout.annotations.is_empty());
    let [(index, err)] = out.annotation_errors.as_slice() else {
        panic!("{:?}", out.annotation_errors);
    };
    assert_eq!(*index, 0);
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
    let out =
        rebuild_view(&view, &ViewFrame::TOP, &[], &kernel, None).expect("an empty view is a view");
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

// =========================================================== D4b: sections,
// details, the title block and the cache key.

/// A sheet with one front view, and that view's id.
fn sheet_with_front() -> (Sheet, Uuid) {
    let mut sheet = Sheet::new("S");
    let front = DrawingView::new(
        "Front",
        ViewSource::whole_tab("t"),
        Projection::Named {
            view: NamedView::Front,
        },
    );
    let id = front.id;
    sheet.views.push(front);
    (sheet, id)
}

#[test]
fn a_vertical_cutting_line_on_the_front_view_sections_as_the_side_view_of_the_half_it_keeps() {
    // The whole of the section rule, derived rather than recorded. On the
    // front view (u = +x, v = +z, sight +y) a VERTICAL cutting line at
    // x = 0.01 names the plane x = 0.01, and the view of it looks along the
    // plane's normal from the discarded side — which for the half kept is
    // exactly `NamedView::Left`'s frame (dir +x, up +z): the left-hand view.
    let (mut sheet, front) = sheet_with_front();
    let mut cut = DrawingView::new(
        "A",
        ViewSource::whole_tab("t"),
        Projection::Section {
            parent: front,
            from: [0.01, -0.02],
            to: [0.01, 0.02],
            flip: false,
            label: "A".to_string(),
        },
    );
    let cut_id = cut.id;
    cut.name = "SECTION A-A".to_string();
    sheet.views.push(cut);

    let frame = sheet
        .view_frame(cut_id, ProjectionAngle::Third)
        .expect("a section of a named view has a frame");
    assert!(
        same_frame(&frame, &NamedView::Left.frame()),
        "a vertical cut on the front view is the left view of the kept half, got {frame:?}"
    );
    // The plane itself: through the line, normal along the parent's `u`.
    let parent = ViewFrame::FRONT.basis().unwrap();
    let plane = section_plane(&parent, [0.01, -0.02], [0.01, 0.02], false).expect("a real line");
    assert!((plane.origin[0] - 0.01).abs() < 1e-12, "{:?}", plane.origin);
    // The normal points at the DISCARDED side, which is the convention
    // `section_with_plane` keeps: every kept point has (p − o)·n̂ ≤ 0.
    assert!((plane.normal[0] + 1.0).abs() < 1e-12, "{:?}", plane.normal);
    // So the kept half is x ≥ 0.01, and the view's own origin sits on the
    // plane — the frame the cap's loops are then rotated into.
    assert_eq!(frame.origin, plane.origin);

    // `flip` reverses the arrows without redrawing the line: the same line,
    // the other half, the opposite named view.
    let flipped = section_plane(&parent, [0.01, -0.02], [0.01, 0.02], true).expect("a real line");
    assert!(
        (flipped.normal[0] - 1.0).abs() < 1e-12,
        "{:?}",
        flipped.normal
    );
    assert!(same_frame(
        &section_frame(&parent, &flipped),
        &NamedView::Right.frame()
    ));
}

#[test]
fn a_horizontal_cutting_line_sections_as_the_top_or_bottom_view_rather_than_one_turned_over() {
    // The case the paper-up fallback exists for. A HORIZONTAL cut line on the
    // front view puts the plane's normal along the parent's own paper up, so
    // "the direction in the section's plane closest to the parent's up"
    // vanishes — and a careless fallback gives a top view with paper up +z,
    // which is a view of the top drawn upside down. The sign has to follow
    // `projected_frame`'s own Up/Down rows, and this is the assertion that
    // pins it.
    let parent = ViewFrame::FRONT.basis().unwrap();
    let up_cut = section_plane(&parent, [-0.02, 0.005], [0.02, 0.005], false).expect("a real line");
    // The line runs +u, so its left normal is +v = world +z: the kept half is
    // BELOW the plane and the viewer looks down at it.
    assert!(
        (up_cut.normal[2] - 1.0).abs() < 1e-12,
        "{:?}",
        up_cut.normal
    );
    assert!(
        same_frame(&section_frame(&parent, &up_cut), &NamedView::Top.frame()),
        "a horizontal cut keeping the lower half is the top view, got {:?}",
        section_frame(&parent, &up_cut)
    );
    let down_cut =
        section_plane(&parent, [-0.02, 0.005], [0.02, 0.005], true).expect("a real line");
    assert!(
        same_frame(
            &section_frame(&parent, &down_cut),
            &NamedView::Bottom.frame()
        ),
        "flipped it is the bottom view, got {:?}",
        section_frame(&parent, &down_cut)
    );
}

#[test]
fn the_projection_standard_flips_which_side_of_its_parent_a_section_is_placed_on() {
    // A section follows the SAME standard as any other projected view: third
    // angle places it on the side it is viewed FROM (against the arrows),
    // first angle on the side they point to. Derived from the one rule, so
    // the two standards cannot drift apart.
    let parent = ViewFrame::FRONT.basis().unwrap();
    let cut = section_plane(&parent, [0.01, -0.02], [0.01, 0.02], false).expect("a real line");
    // Sight is +x (the arrows point right on the paper); the viewer is on the
    // left.
    let third = section_paper_step(&cut, ProjectionAngle::Third);
    let first = section_paper_step(&cut, ProjectionAngle::First);
    assert!(third[0] < 0.0 && third[1].abs() < 1e-12, "{third:?}");
    assert_eq!(first, [-third[0], -third[1]]);

    // And the placement itself clears both drawings, as the four-way form
    // does — through the box's support, which for an axis step is exactly the
    // half-extent.
    let axis = auto_placement_step_mm([100.0, 100.0], [40.0, 20.0], [40.0, 20.0], [1.0, 0.0], 15.0);
    assert_eq!(
        axis,
        auto_placement_mm(
            [100.0, 100.0],
            [40.0, 20.0],
            [40.0, 20.0],
            ProjectedDirection::Right,
            15.0
        )
    );
    // An oblique step clears the CORNER, which is what the support function
    // buys: a 45° step past two 40 × 20 boxes reaches
    // (20 + 10)/√2 × 2 + 15 along the diagonal.
    let oblique = auto_placement_step_mm([0.0, 0.0], [40.0, 20.0], [40.0, 20.0], [1.0, 1.0], 15.0);
    let s = 1.0 / 2.0_f64.sqrt();
    let reach = 2.0 * (20.0 * s + 10.0 * s) + 15.0;
    assert!((oblique[0] - s * reach).abs() < 1e-9, "{oblique:?}");
    assert!((oblique[1] - s * reach).abs() < 1e-9, "{oblique:?}");
    // A step of no length places at the parent rather than at infinity.
    assert_eq!(
        auto_placement_step_mm([5.0, 6.0], [1.0, 1.0], [1.0, 1.0], [0.0, 0.0], 15.0),
        [5.0, 6.0]
    );
}

#[test]
fn a_cutting_line_of_no_length_is_refused_rather_than_cut_at_an_arbitrary_normal() {
    let (mut sheet, front) = sheet_with_front();
    let cut = DrawingView::new(
        "A",
        ViewSource::whole_tab("t"),
        Projection::Section {
            parent: front,
            from: [0.01, 0.0],
            to: [0.01, 0.0],
            flip: false,
            label: "A".to_string(),
        },
    );
    let id = cut.id;
    sheet.views.push(cut);
    let err = sheet
        .view_frame(id, ProjectionAngle::Third)
        .expect_err("a line of no length names no plane");
    assert!(
        matches!(err, DrawingError::DegenerateCut { view, .. } if view == id),
        "{err:?}"
    );
    // The validator says so too, as a warning rather than a load failure.
    let drawing = Drawing {
        sheets: vec![sheet],
        ..Drawing::default()
    };
    assert!(
        drawing
            .validate()
            .iter()
            .any(|w| w.contains("cutting line of no length")),
        "{:?}",
        drawing.validate()
    );
}

#[test]
fn a_detail_view_keeps_its_parents_frame_and_crops_to_the_disc_it_was_given() {
    // A detail is the SAME projection magnified, so re-deriving its frame
    // would be the next thing to disagree with the view it crops.
    let (mut sheet, front) = sheet_with_front();
    let mut detail = DrawingView::new(
        "Detail A",
        ViewSource::whole_tab("t"),
        Projection::Detail {
            parent: front,
            center: [0.01, 0.002],
            radius: 0.004,
            label: "A".to_string(),
        },
    );
    detail.scale = 2.0;
    let id = detail.id;
    sheet.views.push(detail);
    let frame = sheet
        .view_frame(id, ProjectionAngle::Third)
        .expect("a frame");
    assert!(same_frame(&frame, &ViewFrame::FRONT));

    // And the crop: the view's BOX is the disc's box, so the detail is laid
    // out on the crop rather than on whatever curve happened to survive.
    // 2 × 0.004 m at 2:1 is 16 mm of paper in each direction.
    let kernel = waffle_types::kernel::MockKernel::new();
    let view = sheet.view(id).unwrap();
    let extras = ViewExtras {
        clip: Some(ClipCircle {
            center: [0.01, 0.002],
            radius: 0.004,
        }),
        ..ViewExtras::default()
    };
    let out =
        rebuild_view_in(view, &frame, &[], &extras, &kernel, None).expect("a detail of nothing");
    assert_eq!(out.extent_mm, [16.0, 16.0]);
    assert_eq!(
        out.layout.clip,
        Some(ClipCircle {
            center: [0.01, 0.002],
            radius: 0.004
        })
    );
}

#[test]
fn a_detail_with_a_crop_that_is_not_a_disc_is_refused() {
    let kernel = waffle_types::kernel::MockKernel::new();
    let view = DrawingView::new(
        "D",
        ViewSource::whole_tab("t"),
        Projection::Named {
            view: NamedView::Top,
        },
    );
    for radius in [0.0, -1.0, f64::NAN] {
        let extras = ViewExtras {
            clip: Some(ClipCircle {
                center: [0.0, 0.0],
                radius,
            }),
            ..ViewExtras::default()
        };
        let err = rebuild_view_in(&view, &ViewFrame::TOP, &[], &extras, &kernel, None)
            .expect_err("a crop of no area is not a crop");
        assert!(
            matches!(err, DrawingError::BadCropRadius { .. }),
            "{radius}: {err:?}"
        );
    }
}

#[test]
fn a_parents_marks_are_its_childrens_own_geometry_and_the_arrows_point_into_the_kept_half() {
    // Marks are derived from the children, not stored on the parent: a stored
    // mark is a second record of the child's geometry, free to survive the
    // child's deletion.
    let (mut sheet, front) = sheet_with_front();
    sheet.views.push(DrawingView::new(
        "A",
        ViewSource::whole_tab("t"),
        Projection::Section {
            parent: front,
            from: [0.01, -0.02],
            to: [0.01, 0.02],
            flip: false,
            label: "A".to_string(),
        },
    ));
    sheet.views.push(DrawingView::new(
        "B",
        ViewSource::whole_tab("t"),
        Projection::Detail {
            parent: front,
            center: [0.0, 0.0],
            radius: 0.003,
            label: "B".to_string(),
        },
    ));
    let marks = sheet.marks_on(front);
    assert_eq!(marks.len(), 2);
    match &marks[0] {
        ViewMark::Section {
            from,
            to,
            sight,
            label,
        } => {
            assert_eq!((*from, *to), ([0.01, -0.02], [0.01, 0.02]));
            assert_eq!(label, "A");
            // The line runs +v, its left normal is −u (the discarded side),
            // so the arrows point +u — into the half the section keeps.
            assert!(
                (sight[0] - 1.0).abs() < 1e-12 && sight[1].abs() < 1e-12,
                "{sight:?}"
            );
        }
        other => panic!("expected a section mark, got {other:?}"),
    }
    assert!(matches!(&marks[1], ViewMark::Detail { label, .. } if label == "B"));
    // Nothing is marked on a view nothing derives from.
    assert!(sheet.marks_on(sheet.views[1].id).is_empty());

    // Labels are the next FREE letter, so deleting A and cutting again
    // re-uses A rather than minting a second B.
    assert_eq!(sheet.next_label(), "C");
    sheet.views.retain(|v| v.projection.label() != Some("A"));
    assert_eq!(sheet.next_label(), "A");
}

#[test]
fn the_label_sequence_is_the_drafting_one_and_does_not_run_out_at_z() {
    assert_eq!(label_at(0), "A");
    assert_eq!(label_at(25), "Z");
    assert_eq!(label_at(26), "AA");
    assert_eq!(label_at(27), "AB");
    assert_eq!(label_at(51), "AZ");
    assert_eq!(label_at(52), "BA");
}

#[test]
fn a_section_and_a_detail_name_their_parent_through_one_accessor() {
    // One place, so the cycle check, the delete cascade and the validator
    // cannot each learn about a new derived kind separately.
    let p = Uuid::new_v4();
    assert_eq!(
        Projection::Named {
            view: NamedView::Top
        }
        .parent(),
        None
    );
    assert_eq!(
        Projection::Custom {
            dir: [0.0, 0.0, 1.0],
            up: None
        }
        .parent(),
        None
    );
    for projection in [
        Projection::ProjectedFrom {
            parent: p,
            direction: ProjectedDirection::Right,
        },
        Projection::Section {
            parent: p,
            from: [0.0, 0.0],
            to: [1.0, 0.0],
            flip: false,
            label: "A".into(),
        },
        Projection::Detail {
            parent: p,
            center: [0.0, 0.0],
            radius: 1.0,
            label: "A".into(),
        },
    ] {
        assert_eq!(projection.parent(), Some(p), "{}", projection.tag());
    }
}

#[test]
fn a_cap_is_rotated_into_the_section_views_own_frame_rather_than_the_cut_planes() {
    // The cap comes back in the frame the kernel derives from the normal
    // alone; the view's paper up is chosen to agree with its parent. Without
    // the rotation the hatch arrives turned against the drawing it fills.
    // Checked on a square cap: in the view frame its corners must be the ones
    // the view would project, not the ones the cut plane reported.
    let view = ViewBasis {
        origin: [0.0, 0.0, 0.0],
        u: [0.0, 1.0, 0.0],
        v: [0.0, 0.0, 1.0],
        w: [1.0, 0.0, 0.0],
    };
    // A cap frame that is the SAME plane with its u/v swapped for a quarter
    // turn: u = +z, v = −y, same w.
    let cap_basis = ViewBasis {
        origin: [0.0, 0.0, 0.0],
        u: [0.0, 0.0, 1.0],
        v: [0.0, -1.0, 0.0],
        w: [1.0, 0.0, 0.0],
    };
    let loops = vec![
        SectionLoop {
            curves: vec![waffle_types::kernel::projection::Curve2::Line {
                start: cad_point(1.0, 0.0),
                end: cad_point(1.0, 2.0),
            }],
            signed_area: 4.0,
            exact: true,
        },
        SectionLoop {
            curves: vec![waffle_types::kernel::projection::Curve2::Circle {
                center: cad_point(0.0, 0.0),
                radius: 0.5,
                start_angle: 0.0,
                end_angle: std::f64::consts::TAU,
            }],
            signed_area: -0.785,
            exact: true,
        },
    ];
    let (hatch, dropped) = cap_loops_in_view(&loops, &cap_basis, &view);
    assert_eq!(dropped, 0);
    assert_eq!(hatch.len(), 2);
    // Outer first, hole second — the kernel's measured direction, kept.
    assert!(!hatch[0].hole && hatch[1].hole);
    // The cap's `u` is the view's `v`, so (1, 0) in the cap lands at (0, 1).
    let LayoutCurve::Line { start, end } = &hatch[0].curves[0] else {
        panic!("a line must stay a line");
    };
    assert!(
        start[0].abs() < 1e-12 && (start[1] - 1.0).abs() < 1e-12,
        "{start:?}"
    );
    assert!(
        (end[0] + 2.0).abs() < 1e-12 && (end[1] - 1.0).abs() < 1e-12,
        "{end:?}"
    );
    // A loop whose every curve maps keeps its exactness flag.
    assert!(hatch[0].exact && hatch[1].exact);
}

fn cad_point(x: f64, y: f64) -> waffle_types::kernel::projection::Point2 {
    waffle_types::kernel::projection::Point2::new(x, y)
}

// ------------------------------------------------------------- title block

#[test]
fn the_title_block_fills_the_rows_the_engine_knows_and_leaves_the_rest_to_be_typed() {
    let (mut sheet, _) = sheet_with_front();
    sheet.views[0].scale = 0.5;
    sheet.title_block.fields.push(TitleBlockField::with_text(
        TitleBlockKey::Material,
        "AISI 304",
    ));
    sheet.title_block.fields.push(TitleBlockField::with_text(
        TitleBlockKey::Custom {
            label: "Finish".into(),
        },
        "Ra 1.6",
    ));
    let layout = title_block_layout(
        &sheet.title_block,
        &sheet,
        &TitleBlockContext {
            document_name: "Bracket",
            sheet_number: 2,
            sheet_count: 3,
            angle: ProjectionAngle::First,
        },
        None,
    )
    .layout;
    let rows: Vec<(&str, &str)> = layout
        .rows
        .iter()
        .map(|r| (r.label.as_str(), r.value.as_str()))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("Title", "Bracket"),
            ("Sheet", "2 / 3"),
            ("Scale", "1:2"),
            ("Projection", "First angle"),
            ("Date", ""),
            ("Drawn by", ""),
            ("Material", "AISI 304"),
            ("Finish", "Ra 1.6"),
        ]
    );
}

#[test]
fn a_derived_row_ignores_authored_text_rather_than_printing_a_second_truth() {
    // A title block whose sheet number disagrees with the sheet it is printed
    // on is worse than one a person cannot overrule.
    let (sheet, _) = sheet_with_front();
    let block = TitleBlock {
        show: true,
        fields: vec![
            TitleBlockField::with_text(TitleBlockKey::SheetNumber, "7 / 7"),
            TitleBlockField::with_text(TitleBlockKey::Scale, "100:1"),
        ],
        extra: Map::new(),
    };
    let layout = title_block_layout(
        &block,
        &sheet,
        &TitleBlockContext {
            document_name: "D",
            sheet_number: 1,
            sheet_count: 1,
            angle: ProjectionAngle::Third,
        },
        None,
    )
    .layout;
    assert_eq!(layout.rows[0].value, "1 / 1");
    assert_eq!(layout.rows[1].value, "1:1");
    assert!(TitleBlockKey::SheetNumber.is_derived());
    assert!(!TitleBlockKey::Date.is_derived());
}

/// An expression environment that answers one spelling and refuses the rest,
/// so a test can tell "the row was evaluated" from "the row was filled some
/// other way".
struct OneExpr(&'static str, &'static str);

impl ExprDimensions for OneExpr {
    fn value_of(&self, _expression: &str, _kind: DimensionKind) -> Result<f64, String> {
        Err("this double answers title blocks only".to_string())
    }
    fn text_of(&self, expression: &str) -> Result<String, String> {
        if expression == self.0 {
            Ok(self.1.to_string())
        } else {
            Err(format!("`{expression}` does not resolve"))
        }
    }
}

#[test]
fn a_title_block_expression_row_prints_its_evaluated_text_and_keeps_its_source() {
    // D4c: §8's "title block fields are expressions over document metadata
    // and the measurement functions". The ROW prints the value; the DOCUMENT
    // keeps what was written, so the two are never the same record.
    let (sheet, _) = sheet_with_front();
    let block = TitleBlock {
        show: true,
        fields: vec![TitleBlockField::with_expr(
            TitleBlockKey::Custom {
                label: "Mass".into(),
            },
            "volume(plate) * 0.00000785",
        )],
        extra: Map::new(),
    };
    let fill = title_block_layout(
        &block,
        &sheet,
        &TitleBlockContext {
            document_name: "D",
            sheet_number: 1,
            sheet_count: 1,
            angle: ProjectionAngle::Third,
        },
        Some(&OneExpr("volume(plate) * 0.00000785", "7.85 mm³")),
    );
    assert!(fill.errors.is_empty(), "{:?}", fill.errors);
    assert_eq!(fill.layout.rows[0].label, "Mass");
    assert_eq!(fill.layout.rows[0].value, "7.85 mm³");
    // The source survives in the document, unevaluated.
    assert_eq!(
        block.fields[0].expr.as_deref(),
        Some("volume(plate) * 0.00000785")
    );
}

#[test]
fn a_title_block_expression_that_cannot_be_evaluated_blanks_its_row_and_is_named() {
    // Two ways a row fails and both print NOTHING rather than their own
    // source text: a title block reading `mass(part)` is the failure D4b
    // declined to ship, and a title block reading the LAST rebuild's number
    // is the failure the whole spec exists to prevent.
    let (sheet, _) = sheet_with_front();
    let block = TitleBlock {
        show: true,
        fields: vec![
            TitleBlockField::with_expr(TitleBlockKey::Material, "volume(gone)"),
            // A row whose text and expr are BOTH set: the expression wins,
            // and the authoring door refuses the pair outright so this is
            // only reachable from a hand-edited file.
            TitleBlockField {
                text: Some("AISI 304".into()),
                expr: Some("volume(gone)".into()),
                ..TitleBlockField::new(TitleBlockKey::Revision)
            },
        ],
        extra: Map::new(),
    };
    let ctx = TitleBlockContext {
        document_name: "D",
        sheet_number: 1,
        sheet_count: 1,
        angle: ProjectionAngle::Third,
    };
    // An environment that refuses the spelling.
    let fill = title_block_layout(&block, &sheet, &ctx, Some(&OneExpr("other", "x")));
    assert_eq!(fill.layout.rows[0].value, "");
    assert_eq!(fill.layout.rows[1].value, "");
    assert_eq!(fill.errors.len(), 2);
    let message = fill.errors[0].to_string();
    assert!(message.contains("Material"), "{message}");
    assert!(message.contains("volume(gone)"), "{message}");

    // NO environment at all — a sheet whose views draw no single source tab.
    let fill = title_block_layout(&block, &sheet, &ctx, None);
    assert_eq!(fill.layout.rows[0].value, "");
    assert!(matches!(
        fill.errors[0],
        DrawingError::TitleBlockExprNotEvaluated { .. }
    ));
    assert!(
        fill.errors[0].to_string().contains("no single source tab"),
        "{}",
        fill.errors[0]
    );
}

#[test]
fn a_derived_row_ignores_an_expression_the_same_way_it_ignores_text() {
    // The derived rows stay the document's own, whichever way someone tries
    // to overrule them — and an ignored expression is NOT reported, because
    // nothing was asked of the evaluator.
    let (sheet, _) = sheet_with_front();
    let block = TitleBlock {
        show: true,
        fields: vec![TitleBlockField::with_expr(
            TitleBlockKey::SheetNumber,
            "1 + 1",
        )],
        extra: Map::new(),
    };
    let fill = title_block_layout(
        &block,
        &sheet,
        &TitleBlockContext {
            document_name: "D",
            sheet_number: 1,
            sheet_count: 4,
            angle: ProjectionAngle::Third,
        },
        Some(&OneExpr("1 + 1", "2")),
    );
    assert_eq!(fill.layout.rows[0].value, "1 / 4");
    assert!(fill.errors.is_empty());
}

#[test]
fn a_sheet_whose_views_disagree_about_scale_prints_the_standard_note_and_ignores_details() {
    let (mut sheet, front) = sheet_with_front();
    assert_eq!(sheet_scale_label(&sheet), "1:1");
    // A DETAIL is excluded: its scale prints under its own label, and
    // counting it would make every sheet with a detail read AS SHOWN.
    let mut detail = DrawingView::new(
        "Detail A",
        ViewSource::whole_tab("t"),
        Projection::Detail {
            parent: front,
            center: [0.0, 0.0],
            radius: 0.001,
            label: "A".into(),
        },
    );
    detail.scale = 2.0;
    sheet.views.push(detail);
    assert_eq!(sheet_scale_label(&sheet), "1:1");
    // Two ORDINARY views at different scales do say so.
    let mut other = DrawingView::new(
        "Top",
        ViewSource::whole_tab("t"),
        Projection::Named {
            view: NamedView::Top,
        },
    );
    other.scale = 0.2;
    sheet.views.push(other);
    assert_eq!(sheet_scale_label(&sheet), "AS SHOWN");
    // And an empty sheet has no scale rather than a made-up one.
    assert_eq!(sheet_scale_label(&Sheet::new("empty")), "—");
    assert_eq!(scale_ratio_label(2.0), "2:1");
    assert_eq!(scale_ratio_label(1.0 / 2.5), "1:2.50");
    assert_eq!(scale_ratio_label(0.0), "—");
}

// -------------------------------------------------------------- cache key

#[test]
fn a_cache_key_is_pinned_stable_and_moves_when_anything_the_layout_depends_on_moves() {
    // A PERSISTED key, so its value is part of the format: it has to be the
    // same on every machine and every toolchain, which is why the digest is
    // FNV-1a and not `DefaultHasher`. This pins the digest itself.
    assert_eq!(digest_hex(b""), "cbf29ce484222325");
    assert_eq!(digest_hex(b"a"), "af63dc4c8601ec8c");
    // The first two are FNV-1a's own published vectors, so they also check
    // that this IS FNV-1a and not something that merely looks like it; the
    // third is measured here.
    assert_eq!(digest_hex(b"waffle"), "f56519974fde1e12");

    // A fixed view id and fixed inputs give a fixed key.
    let id = Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap();
    let inputs = CacheInputs {
        sheet_recipe: "aaaa".into(),
        source_recipe: "bbbb".into(),
        body_pids: "cccc".into(),
    };
    let key = view_cache_key(id, &inputs);
    assert_eq!(key, "d4b-9ac7b19a20b149ea");
    assert_eq!(view_cache_key(id, &inputs), key, "the key is a function");

    // Every input moves it, and so does the view it is for.
    for changed in [
        CacheInputs {
            sheet_recipe: "aaab".into(),
            ..inputs.clone()
        },
        CacheInputs {
            source_recipe: "bbbc".into(),
            ..inputs.clone()
        },
        CacheInputs {
            body_pids: "cccd".into(),
            ..inputs.clone()
        },
    ] {
        assert_ne!(view_cache_key(id, &changed), key);
    }
    assert_ne!(view_cache_key(Uuid::new_v4(), &inputs), key);
}

#[test]
fn editing_any_view_of_a_sheet_moves_the_sheets_cache_keys() {
    // The sheet-wide half covers EVERY view's recipe, because a projected,
    // section or detail view's frame is derived from its parent's: a key over
    // the one view would read as valid after the parent was re-aimed. It
    // over-covers, which is the safe direction.
    let (mut sheet, front) = sheet_with_front();
    sheet.views.push(DrawingView::new(
        "Right",
        ViewSource::whole_tab("t"),
        Projection::ProjectedFrom {
            parent: front,
            direction: ProjectedDirection::Right,
        },
    ));
    let child = sheet.views[1].id;
    let before = CacheInputs::for_sheet(&sheet, ProjectionAngle::Third);

    // Re-aiming the PARENT moves the child's key.
    sheet.views[0].projection = Projection::Named {
        view: NamedView::Top,
    };
    let after = CacheInputs::for_sheet(&sheet, ProjectionAngle::Third);
    assert_ne!(
        view_cache_key(child, &before),
        view_cache_key(child, &after),
        "a child's cache must not survive its parent being re-aimed"
    );

    // So does the projection standard, which decides what a projected view
    // SHOWS.
    let first = CacheInputs::for_sheet(&sheet, ProjectionAngle::First);
    assert_ne!(view_cache_key(child, &after), view_cache_key(child, &first));

    // But writing a cache does NOT: the key is of the recipe, so storing the
    // answer must not change the question.
    let mut with_cache = sheet.clone();
    with_cache.views[0].cache = Some(ViewLayout::default());
    with_cache.views[0].cache_key = Some("stale".into());
    assert_eq!(
        CacheInputs::for_sheet(&with_cache, ProjectionAngle::Third),
        after,
        "a stored cache is not part of the recipe it was built from"
    );
}

#[test]
fn a_body_pid_digest_is_order_independent_and_changes_with_the_ids() {
    // `all_entity_pids` makes no ordering promise, so the digest sorts first:
    // a key that moved with the kernel's traversal order would report every
    // cache stale on every rebuild, which is the same as having no key.
    let kernel = waffle_types::kernel::MockKernel::new();
    let a = ProjectionBody {
        handle: KernelSolidHandle::from_raw(1),
        name: "A".into(),
        placement: None,
    };
    let b = ProjectionBody {
        handle: KernelSolidHandle::from_raw(2),
        name: "B".into(),
        placement: None,
    };
    let ab = body_pid_digest(&[a.clone(), b.clone()], &kernel);
    assert_eq!(ab, body_pid_digest(&[a.clone(), b.clone()], &kernel));
    // The body NAMES are part of it, so renaming a body is a new drawing.
    let renamed = ProjectionBody {
        name: "A2".into(),
        ..a.clone()
    };
    assert_ne!(ab, body_pid_digest(&[renamed, b], &kernel));
    assert_ne!(ab, body_pid_digest(&[a], &kernel));
}

#[test]
fn the_d4b_additions_round_trip_through_serde_and_cost_an_untouched_sheet_nothing() {
    let (mut sheet, front) = sheet_with_front();
    sheet.views.push(DrawingView::new(
        "A",
        ViewSource::whole_tab("t"),
        Projection::Section {
            parent: front,
            from: [0.0, -0.01],
            to: [0.0, 0.01],
            flip: true,
            label: "A".into(),
        },
    ));
    sheet.views.push(DrawingView::new(
        "B",
        ViewSource::whole_tab("t"),
        Projection::Detail {
            parent: front,
            center: [0.002, 0.003],
            radius: 0.004,
            label: "B".into(),
        },
    ));
    sheet.views[1].cache_key = Some("d4b-0123456789abcdef".into());
    sheet.title_block_cache = Some(TitleBlockLayout {
        rows: vec![TitleBlockRow {
            label: "Title".into(),
            value: "Bracket".into(),
        }],
    });
    let drawing = Drawing {
        sheets: vec![sheet],
        projection_angle: ProjectionAngle::First,
        ..Drawing::default()
    };
    let json = serde_json::to_string(&drawing).unwrap();
    let back: Drawing = serde_json::from_str(&json).unwrap();
    assert_eq!(
        serde_json::to_string(&back).unwrap(),
        json,
        "the D4b fields must survive a round trip byte for byte"
    );
    assert!(back.validate().is_empty(), "{:?}", back.validate());

    // A drawing written by D4a — no `title_block`, no `cache_key` — loads with
    // the defaults, and a title block it never asked for is the ON default,
    // because a drawing without one is not a controlled document.
    let old = r#"{"sheets":[{"id":"00000000-0000-4000-8000-000000000002","name":"S"}]}"#;
    let back: Drawing = serde_json::from_str(old).unwrap();
    assert!(back.sheets[0].title_block.show);
    assert_eq!(back.sheets[0].title_block.fields.len(), 6);
    assert!(back.sheets[0].title_block_cache.is_none());
}
