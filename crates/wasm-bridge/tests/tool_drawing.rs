//! The drawing tools in the engine (`specs/drawings_and_mbd.md` §8, D4a):
//! `drawing_view_add`, `drawing_view_edit`, `drawing_annotation_add`, and
//! `export_dxf` over a sheet.
//!
//! Real kernel throughout: a drawing view is a projection of a B-Rep, which
//! `MockKernel` has none of. The pure-document half — the frame algebra, the
//! paper layout, the refusals made before the kernel is asked — is pinned in
//! `feature_engine::drawing`'s unit tests; what these reach is the whole path
//! from a tool call to the curves on a sheet.
//!
//! ```text
//! cargo test -p wasm-bridge --test tool_drawing
//! ```

use feature_engine::types::*;
use serde_json::{json, Value};
use uuid::Uuid;
use waffle_types::annotation::layout::{AnnotationLayout, LayoutCurve, ViewLayout};
use waffle_types::kernel::projection::Visibility;
use waffle_types::*;
use wasm_bridge::*;

fn tool(
    state: &mut EngineState,
    kernel: &mut kernel_v2::KernelV2Adapter,
    name: &str,
    args: Value,
) -> ToolResult {
    execute_tool(state, kernel, name, &args, None)
}

fn ok(
    state: &mut EngineState,
    kernel: &mut kernel_v2::KernelV2Adapter,
    name: &str,
    args: Value,
) -> Value {
    let result = tool(state, kernel, name, args);
    assert!(!result.is_error, "{name} refused: {result:?}");
    result.structured_content
}

fn refused(
    state: &mut EngineState,
    kernel: &mut kernel_v2::KernelV2Adapter,
    name: &str,
    args: Value,
) -> Value {
    let result = tool(state, kernel, name, args);
    assert!(result.is_error, "{name} was expected to refuse: {result:?}");
    result.structured_content["error"].clone()
}

fn point(id: u32, x: f64, y: f64) -> SketchEntity {
    SketchEntity::Point {
        id,
        x,
        y,
        construction: false,
    }
}

/// The box's authored size, in meters.
const W: f64 = 0.020;
const D: f64 = 0.010;
const H: f64 = 0.005;

/// A `W × D × H` box on one Part tab, plus a Drawing tab, with the Drawing
/// tab active. Returns `(state, kernel, part_tab_id, drawing_tab_id)`.
fn box_and_drawing() -> (EngineState, kernel_v2::KernelV2Adapter, String, String) {
    let mut state = EngineState::new();
    let mut kernel = kernel_v2::KernelV2Adapter::new();
    let part_tab = state.session.active_tab_id().to_string();

    let corners = [(1, 0.0, 0.0), (2, W, 0.0), (3, W, D), (4, 0.0, D)];
    let mut entities: Vec<SketchEntity> =
        corners.iter().map(|&(id, x, y)| point(id, x, y)).collect();
    for (id, (a, b)) in [(10, (1, 2)), (11, (2, 3)), (12, (3, 4)), (13, (4, 1))] {
        entities.push(SketchEntity::Line {
            id,
            start_id: a,
            end_id: b,
            construction: false,
        });
    }
    let sketch = Operation::Sketch {
        sketch: Sketch {
            id: Uuid::new_v4(),
            plane_face: None,
            plane: GeomRef {
                kind: TopoKind::Face,
                anchor: Anchor::Datum {
                    datum_id: Uuid::new_v4(),
                },
                selector: Selector::Role {
                    role: Role::EndCapPositive,
                    index: 0,
                },
                policy: ResolvePolicy::BestEffort,
                scope: None,
            },
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            // The sketch's own +x pinned to world +x, so the test can predict
            // which authored extent lands on which view axis rather than
            // recording whatever the derived basis chose (the D3 harness's
            // finding).
            plane_x_axis: Some([1.0, 0.0, 0.0]),
            entities,
            constraints: Vec::new(),
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: corners.iter().map(|&(id, x, y)| (id, (x, y))).collect(),
            projected: Vec::new(),
            solved_profiles: vec![ClosedProfile {
                entity_ids: vec![10, 11, 12, 13],
                is_outer: true,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            }],
        },
    };
    let added = ok(
        &mut state,
        &mut kernel,
        "feature_add",
        json!({ "operation": serde_json::to_value(sketch).expect("a sketch operation") }),
    );
    let sketch_id = added["feature_id"].clone();
    ok(
        &mut state,
        &mut kernel,
        "feature_add",
        json!({ "operation": {
            "type": "Extrude",
            "params": {
                "sketch_id": sketch_id,
                "profile_index": 0,
                "profile_entity_ids": [10, 11, 12, 13],
                "depth": H,
                "symmetric": false,
                "cut": false,
            }
        } }),
    );

    let drawing_tab = ok(
        &mut state,
        &mut kernel,
        "tab_add",
        json!({ "kind": "Drawing" }),
    )["tab_id"]
        .as_str()
        .expect("the drawing tab's id")
        .to_string();
    (state, kernel, part_tab, drawing_tab)
}

/// The cached layout of `view_id` — the record the app draws, read from the
/// tab the evaluation wrote it into.
fn layout(state: &EngineState, drawing_tab: &str, view_id: &str) -> ViewLayout {
    let id = Uuid::parse_str(view_id).expect("a view id");
    state
        .session
        .drawing(drawing_tab)
        .expect("a drawing tab")
        .find_view(id)
        .unwrap_or_else(|| panic!("the drawing has no view {id}"))
        .1
        .cache
        .clone()
        .unwrap_or_else(|| panic!("view {id} has no cached layout"))
}

/// `(visible, hidden)` curve counts of a view's layout.
fn counts(layout: &ViewLayout) -> (usize, usize) {
    let visible = layout
        .curves
        .iter()
        .filter(|c| c.visibility == Visibility::Visible)
        .count();
    (visible, layout.curves.len() - visible)
}

/// `(visible, hidden)` counts of the straight LINES only — a box's edges,
/// leaving out the vertical edges a plan view sees end-on and reports as
/// `Curve2::Point`.
fn line_counts(layout: &ViewLayout) -> (usize, usize) {
    let lines: Vec<_> = layout
        .curves
        .iter()
        .filter(|c| matches!(c.geometry, LayoutCurve::Line { .. }))
        .collect();
    let visible = lines
        .iter()
        .filter(|c| c.visibility == Visibility::Visible)
        .count();
    (visible, lines.len() - visible)
}

/// Every curve with its visibility, for a failure message that says what was
/// drawn rather than only that the count was wrong.
fn described(layout: &ViewLayout) -> Vec<String> {
    layout
        .curves
        .iter()
        .map(|c| format!("{:?} {:?}", c.visibility, c.geometry))
        .collect()
}

// ── The gate ────────────────────────────────────────────────────────────

#[test]
fn the_drawing_tools_need_a_drawing_tab() {
    let (mut state, mut kernel, part_tab, drawing_tab) = box_and_drawing();
    ok(
        &mut state,
        &mut kernel,
        "tab_switch",
        json!({ "tab_id": part_tab }),
    );
    let err = refused(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": drawing_tab }),
    );
    assert_eq!(err["code"], "TabKindNotSupported");
    assert_eq!(err["details"]["kind"], "Part");

    // And a drawing of itself is refused by name: it has no bodies, and
    // evaluating it would recurse through `OpenDrawing`.
    ok(
        &mut state,
        &mut kernel,
        "tab_switch",
        json!({ "tab_id": drawing_tab }),
    );
    let err = refused(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": drawing_tab }),
    );
    assert_eq!(err["code"], "InvalidArgument");
    assert!(
        err["message"]
            .as_str()
            .unwrap()
            .contains("cannot draw itself"),
        "{err}"
    );
}

// ── The views ───────────────────────────────────────────────────────────

#[test]
fn a_top_view_of_a_box_draws_four_edges_and_no_hidden_line_behind_them() {
    // The D1a–c stack through the whole document path, and a result worth
    // stating carefully because the obvious expectation is wrong.
    //
    // A box seen down its own axis has four edges on the near face and four
    // exactly behind them. Both sets project onto the SAME four lines, so one
    // might expect four visible and four hidden. The drawing carries four
    // lines, all visible, and that is D1c being right rather than D1c missing
    // something: a face hides a curve only by standing BETWEEN it and the
    // viewer, and the ray from a far edge of a prismatic solid leaves through
    // the near face's own BOUNDARY — it grazes, and "a face the ray merely
    // grazes separates nothing" (`ProjectionDeclines::ray_grazes_face`). The
    // far edges are therefore visible, coincident with the near ones, and the
    // coincident-and-same-visibility merge leaves one line each. Four lines
    // is also what a drafter draws.
    //
    // The four vertical edges are seen end-on and come back as
    // `Curve2::Point`, which are not lines of the drawing.
    let (mut state, mut kernel, part_tab, drawing_tab) = box_and_drawing();
    let view = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "top" }),
    );
    let view_id = view["view_id"].as_str().expect("the view id").to_string();
    let layout = layout(&state, &drawing_tab, &view_id);

    assert_eq!(
        line_counts(&layout),
        (4, 0),
        "a top view of a box: four edges, none hidden behind them; drew {:?}",
        described(&layout)
    );
    // And the grazing really is what happened, rather than a depth test that
    // quietly found nothing: the view reports it.
    let declines = &state
        .drawing
        .as_ref()
        .expect("the drawing is open")
        .declines;
    assert!(
        declines.get("ray_grazes_face").copied().unwrap_or(0) > 0,
        "an axis-aligned view of a prismatic solid grazes faces; declines were {declines:?}"
    );

    // And the box it drew is the box that was authored: the view's own box
    // spans the authored width and depth, which is what makes the counts a
    // statement about THIS part rather than about any eight lines.
    let [min, max] = layout.bbox.expect("the view has a box");
    let mut spans = [max[0] - min[0], max[1] - min[1]];
    spans.sort_by(f64::total_cmp);
    assert!(
        (spans[0] - D).abs() < 1e-12 && (spans[1] - W).abs() < 1e-12,
        "the top view spans {spans:?}, the box is {D} × {W}"
    );
}

#[test]
fn a_view_projected_from_the_top_view_draws_the_box_from_the_side() {
    // Third angle: the view placed to the right of a top view shows the side
    // the top view's right edge belongs to. Its extents are therefore the
    // box's HEIGHT and one plan dimension — which is the check that the frame
    // was derived from the parent rather than defaulted.
    let (mut state, mut kernel, part_tab, drawing_tab) = box_and_drawing();
    let top_id = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "top" }),
    )["view_id"]
        .as_str()
        .unwrap()
        .to_string();
    let side_id = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({
            "tab_id": part_tab,
            "parent_view_id": top_id,
            "direction_from_parent": "right",
        }),
    )["view_id"]
        .as_str()
        .unwrap()
        .to_string();
    let layout = layout(&state, &drawing_tab, &side_id);

    assert_eq!(
        line_counts(&layout).0,
        4,
        "a side view of a box has four visible edges; drew {:?}",
        described(&layout)
    );

    // One extent is the box's height: this is a side view, not another plan.
    let [min, max] = layout.bbox.expect("the view has a box");
    let spans = [max[0] - min[0], max[1] - min[1]];
    assert!(
        spans.iter().any(|s| (s - H).abs() < 1e-12),
        "a side view must show the {H} height; spans {spans:?}"
    );

    // And the auto-layout put it clear of its parent, to the right.
    let drawing = state.session.drawing(&drawing_tab).unwrap();
    let parent = drawing
        .find_view(Uuid::parse_str(&top_id).unwrap())
        .unwrap()
        .1;
    let child = drawing
        .find_view(Uuid::parse_str(&side_id).unwrap())
        .unwrap()
        .1;
    assert!(
        child.placement_mm[0] > parent.placement_mm[0],
        "the right-hand view sits to the right: {:?} vs {:?}",
        child.placement_mm,
        parent.placement_mm
    );
    assert_eq!(
        child.placement_mm[1], parent.placement_mm[1],
        "a sideways projection keeps the row"
    );
}

#[test]
fn an_iso_view_of_a_box_draws_nine_visible_edges_and_three_hidden() {
    // The classic count, and the reason an isometric view is the test of a
    // hidden-line pipeline: of a box's twelve edges, nine bound the three
    // faces you can see and three bound only the three you cannot.
    let (mut state, mut kernel, part_tab, drawing_tab) = box_and_drawing();
    let view_id = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "iso" }),
    )["view_id"]
        .as_str()
        .unwrap()
        .to_string();
    let layout = layout(&state, &drawing_tab, &view_id);
    assert_eq!(
        counts(&layout),
        (9, 3),
        "an iso view of a box: nine visible edges, three hidden; drew {:?}",
        described(&layout)
    );
}

#[test]
fn turning_hidden_lines_off_leaves_only_the_visible_ones() {
    let (mut state, mut kernel, part_tab, drawing_tab) = box_and_drawing();
    let view_id = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "iso" }),
    )["view_id"]
        .as_str()
        .unwrap()
        .to_string();
    let before = counts(&layout(&state, &drawing_tab, &view_id));
    assert!(before.1 > 0, "the fixture must have hidden lines to remove");

    ok(
        &mut state,
        &mut kernel,
        "drawing_view_edit",
        json!({ "view_id": view_id, "hidden_lines": false }),
    );
    let after = counts(&layout(&state, &drawing_tab, &view_id));
    assert_eq!(
        after,
        (before.0, 0),
        "the visible curves are untouched and the hidden ones are gone"
    );
}

#[test]
fn a_views_scale_and_placement_are_editable_and_a_bad_scale_is_refused() {
    let (mut state, mut kernel, part_tab, drawing_tab) = box_and_drawing();
    let view_id = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "top" }),
    )["view_id"]
        .as_str()
        .unwrap()
        .to_string();
    ok(
        &mut state,
        &mut kernel,
        "drawing_view_edit",
        json!({ "view_id": view_id, "scale": 0.5, "placement_mm": [100.0, 50.0], "name": "Plan" }),
    );
    {
        let drawing = state.session.drawing(&drawing_tab).unwrap();
        let view = drawing
            .find_view(Uuid::parse_str(&view_id).unwrap())
            .unwrap()
            .1;
        assert_eq!(view.scale, 0.5);
        assert_eq!(view.placement_mm, [100.0, 50.0]);
        assert_eq!(view.name, "Plan");
    }

    let err = refused(
        &mut state,
        &mut kernel,
        "drawing_view_edit",
        json!({ "view_id": view_id, "scale": 0.0 }),
    );
    assert_eq!(err["code"], "InvalidArgument");
    assert_eq!(err["details"]["path"], "/scale");
    // The refusal changed nothing.
    assert_eq!(
        state
            .session
            .drawing(&drawing_tab)
            .unwrap()
            .find_view(Uuid::parse_str(&view_id).unwrap())
            .unwrap()
            .1
            .scale,
        0.5
    );
}

// ── Annotations ─────────────────────────────────────────────────────────

/// Two opposite walls of the box, as `(pid, pid, the distance between them)`.
///
/// Computed from the KERNEL, not from the rebuild: `all_entity_pids` over the
/// part's edges plus one projection in the top view's own frame, matched by
/// projected geometry. Deriving it independently is what makes the dimension
/// assertion a check on the rebuild rather than a restatement of it.
///
/// It is also the one thing an agent cannot do today — nothing in the MCP
/// surface hands out EDGE pids, so `drawing_annotation_add` takes ids a
/// caller has no tool to discover. Recorded as an open item in the spec; this
/// test reaches past it because it runs in-process.
fn wall_pids(
    state: &mut EngineState,
    kernel: &mut kernel_v2::KernelV2Adapter,
    part_tab: &str,
    drawing_tab: &str,
) -> (u64, u64, f64) {
    use std::collections::BTreeMap;
    use waffle_types::kernel::projection::{CurveKind, KernelProjection, ProjectOpts, ViewFrame};
    use waffle_types::kernel::{KernelIntrospect, ProjectionBody};

    ok(state, kernel, "tab_switch", json!({ "tab_id": part_tab }));
    let handles: Vec<waffle_types::kernel::KernelSolidHandle> = state
        .engine
        .tree
        .features
        .iter()
        .filter_map(|f| state.engine.feature_results.get(&f.id))
        .flat_map(|r| {
            r.outputs
                .iter()
                .filter(|(k, _)| matches!(k, OutputKey::Main | OutputKey::Body { .. }))
                .map(|(_, b)| b.handle.clone())
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(handles.len(), 1, "the fixture is one body");

    let pid_of: BTreeMap<u64, u64> = kernel
        .all_entity_pids(&handles[0], TopoKind::Edge)
        .into_iter()
        .map(|(id, pid)| (id.0, pid.pid))
        .collect();
    let bodies: Vec<ProjectionBody> = handles.iter().cloned().map(ProjectionBody::solo).collect();
    let projected = kernel
        .project_bodies(&bodies, &ViewFrame::TOP, &ProjectOpts::default())
        .expect("the top view projects");

    // The walls across view axis 0, VISIBLE only: the hidden four are
    // coincident with them, and a pid that names two drawn curves is
    // ambiguous — which the rebuild refuses, and which this avoids tripping.
    let mut walls: Vec<(f64, u64)> = projected
        .curves
        .iter()
        .filter(|c| c.kind == CurveKind::Edge && c.visibility == Visibility::Visible)
        .filter_map(|c| {
            let source = c.source?;
            let pid = *pid_of.get(&source.0)?;
            match LayoutCurve::from_curve2(&c.geometry) {
                LayoutCurve::Line { start, end }
                    if (end[0] - start[0]).abs() < 1e-12 && (end[1] - start[1]).abs() > 1e-9 =>
                {
                    Some((start[0], pid))
                }
                _ => None,
            }
        })
        .collect();
    walls.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert!(
        walls.len() >= 2,
        "a top view of the box has two walls across axis 0, found {}",
        walls.len()
    );
    let (lo_u, lo) = walls[0];
    let (hi_u, hi) = *walls.last().unwrap();
    ok(
        state,
        kernel,
        "tab_switch",
        json!({ "tab_id": drawing_tab }),
    );
    (lo, hi, hi_u - lo_u)
}

#[test]
fn a_linear_dimension_on_a_top_view_measures_the_authored_box() {
    // The whole D3 path with a real producer: the pids come from the kernel
    // (`entity_pid`), the anchors are `Selector::Pid`, and the number in the
    // layout is the one the box was authored with — never a typed one.
    let (mut state, mut kernel, part_tab, drawing_tab) = box_and_drawing();
    let (lo, hi, span) = wall_pids(&mut state, &mut kernel, &part_tab, &drawing_tab);
    // The span is one of the authored extents — which one depends on the view
    // basis, so it is derived rather than assumed.
    assert!(
        (span - W).abs() < 1e-12 || (span - D).abs() < 1e-12,
        "the wall pair spans {span}, the box is {W} × {D}"
    );
    let view_id = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "top" }),
    )["view_id"]
        .as_str()
        .unwrap()
        .to_string();

    let answer = ok(
        &mut state,
        &mut kernel,
        "drawing_annotation_add",
        json!({
            "view_id": view_id,
            "annotation": "Dimension",
            "kind": "Distance",
            "anchors": [lo, hi],
            "precision": 2,
        }),
    );
    assert_eq!(answer["annotation_index"], 0);
    let layout = layout(&state, &drawing_tab, &view_id);
    assert_eq!(layout.annotations.len(), 1);
    let AnnotationLayout::Dimension { value, .. } = &layout.annotations[0] else {
        panic!("not a dimension: {:?}", layout.annotations[0]);
    };
    assert!(
        (value - span).abs() < 1e-12,
        "the dimension measured {value}, the walls are {span} apart"
    );
}

#[test]
fn an_expression_dimension_is_authorable_and_prints_the_expression_not_the_anchors() {
    // D4c: `Measured::Expr` became EVALUABLE with D2 and the rebuild stopped
    // refusing it then — but nothing could author one, because
    // `build_annotation` always wrote `Measured::FromGeometry` and the tool
    // had no `expr` argument. This is the authoring half.
    //
    // The pin is that the printed number is the EXPRESSION's and not the
    // anchors': the dimension is anchored on the same wall pair the test
    // above measures, and the expression asks for half of it, so one number
    // cannot be mistaken for the other.
    let (mut state, mut kernel, part_tab, drawing_tab) = box_and_drawing();
    let (lo, hi, span) = wall_pids(&mut state, &mut kernel, &part_tab, &drawing_tab);
    ok(
        &mut state,
        &mut kernel,
        "tab_switch",
        json!({ "tab_id": part_tab }),
    );
    ok(
        &mut state,
        &mut kernel,
        "parameters_set",
        json!({ "parameters": [{ "name": "half_span", "expression": format!("{}", span * 500.0) }] }),
    );
    ok(
        &mut state,
        &mut kernel,
        "tab_switch",
        json!({ "tab_id": drawing_tab }),
    );
    let view_id = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "top" }),
    )["view_id"]
        .as_str()
        .unwrap()
        .to_string();
    ok(
        &mut state,
        &mut kernel,
        "drawing_annotation_add",
        json!({
            "view_id": view_id,
            "annotation": "Dimension",
            "kind": "Distance",
            // The anchors are still required — they are WHERE it is drawn.
            "anchors": [lo, hi],
            "expr": "half_span",
        }),
    );
    let laid_out = layout(&state, &drawing_tab, &view_id);
    let AnnotationLayout::Dimension { value, .. } = &laid_out.annotations[0] else {
        panic!("not a dimension: {:?}", laid_out.annotations[0]);
    };
    // The layout carries METERS, and the expression was written in mm.
    assert!(
        (value - span / 2.0).abs() < 1e-12,
        "the expression asked for {} m, the layout says {value}",
        span / 2.0
    );
    assert!(
        (value - span).abs() > 1e-6,
        "it printed what the anchors measure, so the expression did nothing"
    );

    // A `value` is STILL not expressible at this door, which is §7's refusal
    // and the reason an expression is allowed where a literal is not: an
    // expression is re-measured on every rebuild and cannot go stale.
    let err = refused(
        &mut state,
        &mut kernel,
        "drawing_annotation_add",
        json!({
            "view_id": view_id,
            "annotation": "Dimension",
            "kind": "Distance",
            "anchors": [lo, hi],
            "value": 0.123,
        }),
    );
    assert_eq!(err["code"], "InvalidArgument", "{err}");

    // An expression that does not resolve takes the annotation down loudly
    // and leaves nothing behind.
    let err = refused(
        &mut state,
        &mut kernel,
        "drawing_annotation_add",
        json!({
            "view_id": view_id,
            "annotation": "Dimension",
            "kind": "Distance",
            "anchors": [lo, hi],
            "expr": "no_such_parameter",
        }),
    );
    let message = serde_json::to_string(&err).unwrap_or_default();
    assert!(message.contains("no_such_parameter"), "{err}");
    assert_eq!(
        layout(&state, &drawing_tab, &view_id).annotations.len(),
        1,
        "the refused annotation was rolled back"
    );
}

#[test]
fn an_annotation_whose_anchor_is_absent_refuses_rather_than_dimensioning_a_neighbour() {
    // D0's never-rebinding `Selector::Pid`, at the drawing boundary: an
    // anchor on a pid this view does not draw has no geometry at all, so it
    // cannot measure a different edge and print a plausible number.
    let (mut state, mut kernel, part_tab, drawing_tab) = box_and_drawing();
    let view_id = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "top" }),
    )["view_id"]
        .as_str()
        .unwrap()
        .to_string();
    let err = refused(
        &mut state,
        &mut kernel,
        "drawing_annotation_add",
        json!({
            "view_id": view_id,
            "annotation": "Dimension",
            "kind": "Distance",
            "anchors": [1, 2],
        }),
    );
    assert_eq!(err["code"], "AnnotationNotMeasurable");
    let message = err["details"]["reason"].as_str().unwrap_or_default();
    assert!(
        message.contains("resolves to no geometry") && message.contains("pid 1"),
        "the refusal must name the unresolved anchor: {message}"
    );
    // The refused edit left no annotation behind — and the view still draws.
    let layout = layout(&state, &drawing_tab, &view_id);
    assert!(layout.annotations.is_empty());
    assert_eq!(
        line_counts(&layout),
        (4, 0),
        "the rolled-back annotation did not take the view's curves with it"
    );
}

// ── Export ──────────────────────────────────────────────────────────────

/// The DXF text of an `export_dxf` answer delivered to the agent.
fn exported_dxf(result: &ToolResult) -> String {
    assert!(!result.is_error, "{result:?}");
    result
        .content
        .iter()
        .find(|c| c["type"] == "resource")
        .and_then(|c| c["resource"]["text"].as_str())
        .expect("the DXF rides inline for deliver:agent")
        .to_string()
}

/// `(min_x, min_y, max_x, max_y)` from a DXF's `$EXTMIN` / `$EXTMAX` header
/// variables, in the file's own units (millimetres).
fn dxf_extents(dxf: &str) -> (f64, f64, f64, f64) {
    let lines: Vec<&str> = dxf.lines().map(str::trim).collect();
    let read = |name: &str| -> (f64, f64) {
        let at = lines
            .iter()
            .position(|l| *l == name)
            .unwrap_or_else(|| panic!("{name} is not in the DXF header"));
        // `9 / $EXTMIN / 10 / x / 20 / y`
        let x: f64 = lines[at + 2].parse().expect("an x");
        let y: f64 = lines[at + 4].parse().expect("a y");
        (x, y)
    };
    let (min_x, min_y) = read("$EXTMIN");
    let (max_x, max_y) = read("$EXTMAX");
    (min_x, min_y, max_x, max_y)
}

#[test]
fn a_sheet_exports_every_view_placed_in_paper_millimetres() {
    // The sheet DXF: two views, at different scales and positions, in one
    // file. The check is the file's own extents — a composition that ignored
    // the placements would stack both views at the origin.
    let (mut state, mut kernel, part_tab, _drawing_tab) = box_and_drawing();
    ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "top", "scale": 1.0, "placement_mm": [50.0, 50.0] }),
    );
    ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "front", "scale": 2.0, "placement_mm": [200.0, 150.0] }),
    );
    let dxf = exported_dxf(&tool(
        &mut state,
        &mut kernel,
        "export_dxf",
        json!({ "deliver": "agent" }),
    ));
    let (min_x, min_y, max_x, max_y) = dxf_extents(&dxf);
    // Each view is centred on its own placement, so the sheet spans from the
    // first view's left edge to the second's right.
    assert!(
        min_x < 50.0 && max_x > 200.0,
        "the sheet must span both placements, got x {min_x}..{max_x}"
    );
    assert!(
        min_y < 50.0 && max_y > 150.0,
        "and both rows, got y {min_y}..{max_y}"
    );
}

#[test]
fn one_view_exports_alone_at_the_paper_origin() {
    let (mut state, mut kernel, part_tab, _drawing_tab) = box_and_drawing();
    let view_id = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "top", "placement_mm": [200.0, 150.0] }),
    )["view_id"]
        .as_str()
        .unwrap()
        .to_string();
    let dxf = exported_dxf(&tool(
        &mut state,
        &mut kernel,
        "export_dxf",
        json!({ "deliver": "agent", "view_id": view_id }),
    ));
    let (min_x, min_y, max_x, max_y) = dxf_extents(&dxf);
    // At the origin, not at the sheet position: a cutting table given one
    // part should not have to find it at the coordinates of a drawing it is
    // not reading.
    assert!(
        min_x.abs() < 1e-6 && min_y.abs() < 1e-6,
        "one view alone starts at the origin, got ({min_x}, {min_y})"
    );
    // The authored box at 1:1, in millimetres, either way round.
    let mut spans = [max_x - min_x, max_y - min_y];
    spans.sort_by(f64::total_cmp);
    assert!(
        (spans[0] - D * 1000.0).abs() < 1e-6 && (spans[1] - W * 1000.0).abs() < 1e-6,
        "the view is the authored box in mm, got {spans:?}"
    );
}

#[test]
fn the_model_view_arguments_are_refused_on_a_drawing_tab_and_the_sheet_ones_off_it() {
    let (mut state, mut kernel, part_tab, _drawing_tab) = box_and_drawing();
    ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "top" }),
    );
    // On a Drawing tab a model-view direction names a projection the sheet
    // does not have — refused rather than ignored.
    let err = refused(
        &mut state,
        &mut kernel,
        "export_dxf",
        json!({ "deliver": "agent", "view": "top" }),
    );
    assert_eq!(err["code"], "InvalidArgument");
    assert_eq!(err["details"]["path"], "/view");

    // And off it, a sheet id names a view no Part tab has.
    ok(
        &mut state,
        &mut kernel,
        "tab_switch",
        json!({ "tab_id": part_tab }),
    );
    let err = refused(
        &mut state,
        &mut kernel,
        "export_dxf",
        json!({ "deliver": "agent", "sheet_id": Uuid::new_v4().to_string() }),
    );
    assert_eq!(err["code"], "TabKindNotSupported");
}

// ── The document ────────────────────────────────────────────────────────

#[test]
fn a_drawing_survives_a_save_and_reload_with_its_annotation_and_its_cache() {
    // What the file format persists, through the bridge's own save path: the
    // views, their annotations, and the cached layouts — so a reader with no
    // kernel can draw the sheet.
    let (mut state, mut kernel, part_tab, drawing_tab) = box_and_drawing();
    let (lo, hi, _) = wall_pids(&mut state, &mut kernel, &part_tab, &drawing_tab);
    let view_id = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "top" }),
    )["view_id"]
        .as_str()
        .unwrap()
        .to_string();
    ok(
        &mut state,
        &mut kernel,
        "drawing_annotation_add",
        json!({
            "view_id": view_id,
            "annotation": "Dimension",
            "kind": "Distance",
            "anchors": [lo, hi],
        }),
    );
    let before = layout(&state, &drawing_tab, &view_id);
    assert!(!before.curves.is_empty() && before.annotations.len() == 1);

    let response = wasm_bridge::dispatch(
        &mut state,
        wasm_bridge::messages::UiToEngine::SaveDocument,
        &mut kernel,
    );
    let wasm_bridge::messages::EngineToUi::SaveReady { json_data } = response else {
        panic!("{response:?}")
    };
    let loaded = file_format::load_document(&json_data).expect("the drawing document loads");
    assert!(
        loaded.warnings.is_empty(),
        "a drawing of a tab in the same document warns about nothing: {:?}",
        loaded.warnings
    );
    let tab = loaded
        .document
        .tabs
        .iter()
        .find(|t| t.id == drawing_tab)
        .expect("the drawing tab");
    let drawing = tab.drawing_tree().expect("it is a drawing");
    let view = drawing
        .find_view(Uuid::parse_str(&view_id).unwrap())
        .expect("the view survived")
        .1;
    assert_eq!(view.annotations.len(), 1);
    let cache = view.cache.as_ref().expect("the cache was persisted");
    assert_eq!(cache.curves.len(), before.curves.len());
    assert_eq!(cache.annotations, before.annotations);
}

// ── Mirrors ─────────────────────────────────────────────────────────────

/// A view's projected box as `(u_min, u_max, v_min, v_max)` in model meters.
fn signed_box(layout: &ViewLayout) -> (f64, f64, f64, f64) {
    let [min, max] = layout.bbox.expect("the view has a box");
    (min[0], max[0], min[1], max[1])
}

fn boxes_match(got: (f64, f64, f64, f64), want: (f64, f64, f64, f64)) -> bool {
    (got.0 - want.0).abs() < 1e-9
        && (got.1 - want.1).abs() < 1e-9
        && (got.2 - want.2).abs() < 1e-9
        && (got.3 - want.3).abs() < 1e-9
}

#[test]
fn every_named_view_projects_the_box_to_the_side_of_the_origin_it_is_on() {
    // The MIRROR pin, and the reason it is a signed box rather than a span.
    //
    // `box_and_drawing` authors its box in the POSITIVE octant,
    // `x ∈ [0, W], y ∈ [0, D], z ∈ [0, H]`, with the world origin on one of
    // its corners. So each view's projected box says which way its paper
    // axes point, not merely how big the part is: a top view with `u = −x`
    // instead of `+x` draws the same four lines over the same span, and
    // every other test in this file would pass. Only the SIGN catches it —
    // and a mirrored manufacturing drawing is a part machined the wrong way
    // round, so it is worth a test of its own.
    //
    // The table is read off `NamedView::frame`'s own documented axes, not
    // derived from it, which is what makes it an independent check rather
    // than the implementation restated:
    //
    // | view   | u    | v    |
    // |--------|------|------|
    // | Top    | `+x` | `+y` |
    // | Bottom | `+x` | `−y` |
    // | Front  | `+x` | `+z` |
    // | Back   | `−x` | `+z` |
    // | Right  | `+y` | `+z` |
    // | Left   | `−y` | `+z` |
    //
    // `Bottom` shares `u = +x` with `Front` and `Top` deliberately — that is
    // the projection group sharing a paper axis — so it is the `v` flip that
    // makes it a bottom view rather than a second plan.
    let expected = [
        ("top", (0.0, W, 0.0, D)),
        ("bottom", (0.0, W, -D, 0.0)),
        ("front", (0.0, W, 0.0, H)),
        ("back", (-W, 0.0, 0.0, H)),
        ("right", (0.0, D, 0.0, H)),
        ("left", (-D, 0.0, 0.0, H)),
    ];
    let (mut state, mut kernel, part_tab, drawing_tab) = box_and_drawing();
    for (view, want) in expected {
        let view_id = ok(
            &mut state,
            &mut kernel,
            "drawing_view_add",
            json!({ "tab_id": part_tab, "view": view }),
        )["view_id"]
            .as_str()
            .expect("the view id")
            .to_string();
        let got = signed_box(&layout(&state, &drawing_tab, &view_id));
        assert!(
            boxes_match(got, want),
            "the {view} view projects the box to {got:?}, expected {want:?} — \
             a sign here is a mirrored drawing"
        );
    }
}

#[test]
fn a_projected_view_shows_the_near_side_in_third_angle_and_the_far_side_in_first() {
    // The projection standard, pinned against real projected geometry rather
    // than against the frame algebra `feature_engine::drawing`'s unit tests
    // already cover. Placed to the RIGHT of the front view, third angle
    // shows the right-hand side and first angle shows the left — and the two
    // are mirror images, so the only thing that distinguishes them is the
    // SIGN of `u` (`+y` for the right side, `−y` for the left), which a span
    // test cannot see.
    for (angle, want) in [("Third", (0.0, D, 0.0, H)), ("First", (-D, 0.0, 0.0, H))] {
        let (mut state, mut kernel, part_tab, drawing_tab) = box_and_drawing();
        // No tool sets the document's projection angle yet (an open item —
        // `ProjectionAngle::First` is reachable only from the document
        // model), so it is set on the session and the next edit
        // re-evaluates with it.
        let mut drawing = state
            .session
            .drawing(&drawing_tab)
            .expect("a drawing")
            .clone();
        drawing.projection_angle = match angle {
            "First" => feature_engine::drawing::ProjectionAngle::First,
            _ => feature_engine::drawing::ProjectionAngle::Third,
        };
        state
            .session
            .set_drawing(&drawing_tab, drawing)
            .expect("the angle is set");

        let front = ok(
            &mut state,
            &mut kernel,
            "drawing_view_add",
            json!({ "tab_id": part_tab, "view": "front" }),
        )["view_id"]
            .as_str()
            .expect("the view id")
            .to_string();
        let side = ok(
            &mut state,
            &mut kernel,
            "drawing_view_add",
            json!({
                "tab_id": part_tab,
                "parent_view_id": front,
                "direction_from_parent": "right",
            }),
        )["view_id"]
            .as_str()
            .expect("the view id")
            .to_string();
        let got = signed_box(&layout(&state, &drawing_tab, &side));
        assert!(
            boxes_match(got, want),
            "{angle} angle, placed right of the front view: projected {got:?}, \
             expected {want:?}"
        );
    }
}

#[test]
fn editing_the_part_regenerates_the_drawing_rather_than_leaving_the_cached_view() {
    // The cache is persisted with the document, so a stale one is a drawing
    // that shows a part the document no longer holds. Opening the drawing tab
    // re-evaluates every view, which is what makes the staleness invisible to
    // a user — pinned here so a future change that reuses a cache has to
    // decide what keys it.
    //
    // (It is NOT regenerated while the drawing tab is in the background: an
    // edit to the part, then a save with the part tab still active, writes
    // the previous layout. See the review note.)
    let (mut state, mut kernel, part_tab, drawing_tab) = box_and_drawing();
    let view_id = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "front" }),
    )["view_id"]
        .as_str()
        .expect("the view id")
        .to_string();
    let before = signed_box(&layout(&state, &drawing_tab, &view_id));
    assert!(boxes_match(before, (0.0, W, 0.0, H)), "{before:?}");

    // Twice the extrude depth on the part tab: a front view's `v` extent IS
    // that depth, so the drawing has to move or it is showing the old part.
    ok(
        &mut state,
        &mut kernel,
        "tab_switch",
        json!({ "tab_id": part_tab }),
    );
    let summary = ok(&mut state, &mut kernel, "model_summary", json!({}));
    let extrude_id = summary["features"]
        .as_array()
        .expect("the feature list")
        .iter()
        .find(|f| f["kind"] == "Extrude")
        .map(|f| f["id"].as_str().expect("the feature id").to_string())
        .expect("the fixture's extrude");
    let mut extrude = ok(
        &mut state,
        &mut kernel,
        "feature_get",
        json!({ "feature_id": extrude_id }),
    )["operation"]
        .clone();
    extrude["params"]["depth"] = json!(2.0 * H);
    ok(
        &mut state,
        &mut kernel,
        "feature_edit",
        json!({ "feature_id": extrude_id, "operation": extrude }),
    );

    // Back to the drawing: the view is re-projected, not replayed.
    ok(
        &mut state,
        &mut kernel,
        "tab_switch",
        json!({ "tab_id": drawing_tab }),
    );
    let after = signed_box(&layout(&state, &drawing_tab, &view_id));
    assert!(
        boxes_match(after, (0.0, W, 0.0, 2.0 * H)),
        "the front view should follow the part to {:?}, drew {after:?}",
        (0.0, W, 0.0, 2.0 * H)
    );
}

// ═══════════════════════════════════════════════════════════ D4b: sections,
// details, the title block.

/// The bore's radius, in meters — comfortably inside the `W × D` footprint so
/// a horizontal section cuts a rectangle with ONE hole in it.
const BORE_R: f64 = 0.002;

/// [`box_and_drawing`]'s box with a through bore on its own axis, plus the
/// Drawing tab. Returns `(state, kernel, part_tab, drawing_tab)`.
///
/// The bore is extruded SYMMETRICALLY and three times the box's height, so
/// neither of its end caps is coplanar with a face of the box: a cut operand
/// ending exactly on the box's top face would be a §4.5.5 coplanar overlay
/// case, which is a different thing to be testing than a section.
fn bored_box_and_drawing() -> (EngineState, kernel_v2::KernelV2Adapter, String, String) {
    bored_box_and_drawing_at(W / 2.0, D / 2.0)
}

/// The same plate, bored at an arbitrary `(u, v)` of its sketch plane.
///
/// Parameterized for the kept-side pin: with the bore CENTRED, a cut through
/// the middle leaves two halves that are congruent and project identically, so
/// no assertion on the drawing can tell which one the kernel kept. The bore has
/// to be off centre for a mirrored kept half to be a measurable difference.
fn bored_box_and_drawing_at(
    bore_u: f64,
    bore_v: f64,
) -> (EngineState, kernel_v2::KernelV2Adapter, String, String) {
    let (mut state, mut kernel, part_tab, drawing_tab) = box_and_drawing();
    ok(
        &mut state,
        &mut kernel,
        "tab_switch",
        json!({ "tab_id": part_tab }),
    );
    let sketch = Operation::Sketch {
        sketch: Sketch {
            id: Uuid::new_v4(),
            plane_face: None,
            plane: GeomRef {
                kind: TopoKind::Face,
                anchor: Anchor::Datum {
                    datum_id: Uuid::new_v4(),
                },
                selector: Selector::Role {
                    role: Role::EndCapPositive,
                    index: 0,
                },
                policy: ResolvePolicy::BestEffort,
                scope: None,
            },
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            plane_x_axis: Some([1.0, 0.0, 0.0]),
            entities: Vec::new(),
            constraints: Vec::new(),
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: Default::default(),
            projected: Vec::new(),
            solved_profiles: vec![ClosedProfile {
                entity_ids: vec![20],
                is_outer: true,
                vertex_ids: vec![],
                circle: Some(CircleProfile {
                    center_u: bore_u,
                    center_v: bore_v,
                    radius: BORE_R,
                }),
                spline_segments: vec![],
                arc_segments: vec![],
            }],
        },
    };
    let added = ok(
        &mut state,
        &mut kernel,
        "feature_add",
        json!({ "operation": serde_json::to_value(sketch).expect("a sketch operation") }),
    );
    let sketch_id = added["feature_id"].clone();
    ok(
        &mut state,
        &mut kernel,
        "feature_add",
        json!({ "operation": {
            "type": "Extrude",
            "params": {
                "sketch_id": sketch_id,
                "profile_index": 0,
                "profile_entity_ids": [20],
                "depth": 3.0 * H,
                "symmetric": true,
                "cut": true,
            }
        } }),
    );
    ok(
        &mut state,
        &mut kernel,
        "tab_switch",
        json!({ "tab_id": drawing_tab }),
    );
    (state, kernel, part_tab, drawing_tab)
}

/// Which hatch loop is the hole.
fn hole_index(layout: &ViewLayout) -> usize {
    layout
        .hatch
        .iter()
        .position(|l| l.hole)
        .expect("a bored cap has a hole")
}

#[test]
fn a_horizontal_section_of_a_bored_box_hatches_one_outer_loop_and_one_hole() {
    // The whole section path, end to end: a cutting line drawn on a front
    // view, the kernel's own Intersect against the half-space it names (D1d),
    // the cut half projected with D1a-c, and the cap handed to the sheet as a
    // region to fill. The assertion is the SHAPE of the cap — one outer
    // boundary and one hole — because that is what distinguishes a section of
    // a bored part from a section of a solid one, and it is what a hatch has
    // to respect.
    let (mut state, mut kernel, part_tab, drawing_tab) = bored_box_and_drawing();
    let front = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "front", "placement_mm": [100.0, 100.0] }),
    )["view_id"]
        .as_str()
        .expect("the front view's id")
        .to_string();

    // A horizontal cutting line at mid height, in the front view's own
    // (u, v) = (world x, world z), MILLIMETRES on the wire. It runs past both
    // ends of the part, as a drafter draws it.
    let answer = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({
            "tab_id": part_tab,
            "parent_view_id": front,
            "section_mm": [-2.0, H * 1000.0 / 2.0, W * 1000.0 + 2.0, H * 1000.0 / 2.0],
            "label": "A",
        }),
    );
    let section = answer["view_id"]
        .as_str()
        .expect("the section's id")
        .to_string();
    let laid_out = layout(&state, &drawing_tab, &section);

    let outer = laid_out.hatch.iter().filter(|l| !l.hole).count();
    let holes = laid_out.hatch.iter().filter(|l| l.hole).count();
    assert_eq!(
        (outer, holes),
        (1, 1),
        "the cap of a bored box is one rectangle with one hole in it, got {} loop(s): {:?}",
        laid_out.hatch.len(),
        laid_out
            .hatch
            .iter()
            .map(|l| (l.hole, l.curves.len()))
            .collect::<Vec<_>>()
    );
    // Every boundary curve stayed analytic, so the hatch boundary IS the cap
    // and not a chord polygon of it.
    assert!(
        laid_out.hatch.iter().all(|l| l.exact),
        "a box bored by a cylinder has an exact cap: {:?}",
        laid_out.hatch
    );
    // The hole IS the bore, at the bore's own radius, in the SECTION's frame
    // rather than the cut plane's — the rotation that makes the hatch line up
    // with the curves it fills.
    let hole = &laid_out.hatch[hole_index(&laid_out)];
    let radius = hole
        .curves
        .iter()
        .find_map(|c| match c {
            LayoutCurve::Circle { radius, .. } => Some(*radius),
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "the bore's cap edge should be a circle, got {:?}",
                hole.curves
            )
        });
    assert!(
        (radius - BORE_R).abs() < 1e-9,
        "the hole's radius should be the bore's {BORE_R}, got {radius}"
    );

    // And the PARENT carries the cutting line and the letter, so the pair
    // reads as one drawing.
    let parent_layout = layout(&state, &drawing_tab, &front);
    let marks = &parent_layout.marks;
    assert_eq!(marks.len(), 1, "{marks:?}");
    match &marks[0] {
        waffle_types::annotation::layout::ViewMark::Section { sight, label, .. } => {
            assert_eq!(label, "A");
            // The line runs +u, so its arrows point at the half the section
            // keeps — downwards on the parent's paper.
            assert!(
                sight[1] < -0.5 && sight[0].abs() < 1e-9,
                "the arrows should point into the kept half, got {sight:?}"
            );
        }
        other => panic!("expected a section mark, got {other:?}"),
    }

    // The view is named the way the standard titles it, and the tool reports
    // the cap it hatched.
    let reported = answer["sheets"][0]["views"]
        .as_array()
        .expect("views")
        .iter()
        .find(|v| v["id"] == section.as_str())
        .expect("the section in the answer")
        .clone();
    assert_eq!(reported["name"], "SECTION A-A");
    assert_eq!(reported["hatch_loops"], 2);
    assert_eq!(reported["projection"]["type"], "Section");
}

#[test]
fn a_section_whose_plane_keeps_nothing_says_so_rather_than_drawing_the_uncut_part() {
    // A drawing labelled SECTION A-A showing the OUTSIDE of the solid is a
    // wrong drawing; a missing view is a visible gap. So a cut that keeps no
    // material is reported and the view draws nothing.
    let (mut state, mut kernel, part_tab, drawing_tab) = bored_box_and_drawing();
    let front = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "front" }),
    )["view_id"]
        .as_str()
        .expect("an id")
        .to_string();
    // A cutting line well BELOW the part, keeping the half below it.
    let answer = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({
            "tab_id": part_tab,
            "parent_view_id": front,
            "section_mm": [-2.0, -5.0, W * 1000.0 + 2.0, -5.0],
        }),
    );
    let section = answer["view_id"].as_str().expect("an id").to_string();
    let errors = answer["errors"].as_array().cloned().unwrap_or_default();
    assert!(
        errors
            .iter()
            .any(|e| e.as_str().is_some_and(|s| s.contains("keeps no material"))),
        "the empty cut must be reported, got {errors:?}"
    );
    assert!(
        state
            .session
            .drawing(&drawing_tab)
            .expect("a drawing")
            .find_view(Uuid::parse_str(&section).expect("a uuid"))
            .expect("the view")
            .1
            .cache
            .is_none(),
        "a section that kept nothing must have no layout, not an uncut one"
    );
}

#[test]
fn a_detail_view_at_two_to_one_doubles_the_paper_length_of_the_same_crop() {
    // A detail is a magnified crop, so the SAME disc at 2:1 must measure
    // twice what it does at 1:1 on paper — which is the one property that
    // tells a magnification from a bigger circle.
    let (mut state, mut kernel, part_tab, _drawing) = bored_box_and_drawing();
    let top = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "top", "placement_mm": [100.0, 100.0] }),
    )["view_id"]
        .as_str()
        .expect("an id")
        .to_string();
    // A 4 mm-radius disc around the bore, which is where a drafter would put
    // one.
    let disc = json!([W * 1000.0 / 2.0, D * 1000.0 / 2.0, 4.0]);
    let mut spans = Vec::new();
    for scale in [1.0, 2.0] {
        let detail = ok(
            &mut state,
            &mut kernel,
            "drawing_view_add",
            json!({
                "tab_id": part_tab,
                "parent_view_id": top,
                "detail_mm": disc,
                "scale": scale,
            }),
        )["view_id"]
            .as_str()
            .expect("an id")
            .to_string();
        let dxf = exported_dxf(&tool(
            &mut state,
            &mut kernel,
            "export_dxf",
            json!({ "deliver": "agent", "view_id": detail }),
        ));
        let (min_x, _, max_x, _) = dxf_extents(&dxf);
        spans.push(max_x - min_x);
    }
    assert!(
        (spans[1] - 2.0 * spans[0]).abs() < 1e-6 * spans[1].max(1.0),
        "2:1 must double the paper span of the same crop: {spans:?}"
    );
}

#[test]
fn a_detail_of_the_whole_part_keeps_every_curve_and_one_of_a_corner_keeps_fewer() {
    // The cull is a bounding-box test against the disc's box: conservative,
    // so it never drops a curve that reaches the disc, and the renderer then
    // clips what is left exactly. The property to pin is that it culls at all
    // — a detail carrying the whole part's curves would be a magnified whole
    // part.
    let (mut state, mut kernel, part_tab, drawing_tab) = bored_box_and_drawing();
    let top = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "top" }),
    )["view_id"]
        .as_str()
        .expect("an id")
        .to_string();
    let whole = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "parent_view_id": top,
                "detail_mm": [W * 1000.0 / 2.0, D * 1000.0 / 2.0, 100.0] }),
    )["view_id"]
        .as_str()
        .expect("an id")
        .to_string();
    let corner = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "parent_view_id": top,
                "detail_mm": [0.0, 0.0, 1.0] }),
    )["view_id"]
        .as_str()
        .expect("an id")
        .to_string();
    let parent_curves = layout(&state, &drawing_tab, &top).curves.len();
    assert_eq!(
        layout(&state, &drawing_tab, &whole).curves.len(),
        parent_curves,
        "a disc containing the whole part crops nothing"
    );
    assert!(
        layout(&state, &drawing_tab, &corner).curves.len() < parent_curves,
        "a 1 mm disc at one corner must not carry the whole part"
    );
    // The crop rides on the layout so the renderer can clip to it exactly.
    assert!(layout(&state, &drawing_tab, &corner).clip.is_some());
}

#[test]
fn a_title_block_expression_row_prints_the_measured_model_and_keeps_its_source() {
    // D4c, §8: "title block fields are expressions over document metadata and
    // the measurement functions, so `mass(part)` and a parameter table work
    // with no special casing". The plate is 20 × 10 × 5 mm, so its volume is
    // 1000 mm³ and a 7.85 g/cm³ steel plate weighs 7.85 g — which is the
    // `mass(part)` row §8 names, written in the language that exists rather
    // than waiting for M1's material table.
    let (mut state, mut kernel, part_tab, _drawing) = box_and_drawing();
    ok(
        &mut state,
        &mut kernel,
        "tab_switch",
        json!({ "tab_id": part_tab }),
    );
    let body = ok(&mut state, &mut kernel, "model_summary", json!({}))["bodies"][0]["body_id"]
        .as_str()
        .expect("the plate's body id")
        .to_string();
    ok(
        &mut state,
        &mut kernel,
        "body_rename",
        json!({ "body_id": body, "new_name": "plate" }),
    );
    ok(
        &mut state,
        &mut kernel,
        "parameters_set",
        json!({ "parameters": [{ "name": "lot", "expression": "42" }] }),
    );
    let drawing_tab = state
        .session
        .tabs()
        .into_iter()
        .find(|t| t.kind == "Drawing")
        .expect("the drawing tab")
        .id;
    ok(
        &mut state,
        &mut kernel,
        "tab_switch",
        json!({ "tab_id": drawing_tab }),
    );
    ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "top" }),
    );
    let answer = ok(
        &mut state,
        &mut kernel,
        "drawing_sheet_edit",
        json!({
            "title_block_fields": [
                { "label": "Mass", "expr": "volume(plate) * 0.00785" },
                { "label": "Lot", "expr": "lot" },
                { "label": "Stock", "expr": "20mm * 10mm" },
                { "key": "Material", "text": "AISI 304" },
            ],
        }),
    );
    let rows = answer["sheets"][0]["title_block"]["rows"]["rows"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| panic!("the filled rows: {answer}"));
    let value = |i: usize| rows[i]["value"].as_str().unwrap_or_default().to_string();
    // A measurement: 1000 mm³ × 0.00785 — the unit is the expression's own
    // dimension, printed, which is what makes the row checkable.
    assert_eq!(value(0), "7.85 mm³", "{answer}");
    // A design PARAMETER, through the same environment.
    assert_eq!(value(1), "42", "{answer}");
    // Arithmetic with unit literals: a length² prints its exponent rather
    // than being refused, because a title block is text, not a field.
    assert_eq!(value(2), "200 mm²", "{answer}");
    // And a literal row is untouched by any of it.
    assert_eq!(value(3), "AISI 304");
    // The SOURCE is what the document carries — never the evaluated text, or
    // a reopened document would print a number nothing recomputes.
    let fields = answer["sheets"][0]["title_block"]["fields"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| panic!("the authored fields: {answer}"));
    assert_eq!(fields[0]["expr"], "volume(plate) * 0.00785");
    assert!(fields[0]["text"].is_null());

    // An expression that cannot be evaluated BLANKS its row and is reported.
    // Not its own source text on the paper, and not the last good number.
    let answer = ok(
        &mut state,
        &mut kernel,
        "drawing_sheet_edit",
        json!({ "title_block_fields": [{ "label": "Mass", "expr": "volume(gone)" }] }),
    );
    assert_eq!(
        answer["sheets"][0]["title_block"]["rows"]["rows"][0]["value"],
        ""
    );
    let errors = serde_json::to_string(&answer["errors"]).unwrap_or_default();
    assert!(errors.contains("volume(gone)"), "{answer}");

    // `expr` on a DERIVED key is refused by name, like `text` is: an agent
    // that wrote a sheet number must be told the engine fills it.
    let error = refused(
        &mut state,
        &mut kernel,
        "drawing_sheet_edit",
        json!({ "title_block_fields": [{ "key": "Scale", "expr": "1" }] }),
    );
    assert_eq!(error["code"], "InvalidArgument");
    // And both at once is refused rather than resolved by precedence.
    let error = refused(
        &mut state,
        &mut kernel,
        "drawing_sheet_edit",
        json!({ "title_block_fields": [{ "key": "Revision", "text": "A", "expr": "1" }] }),
    );
    assert_eq!(error["code"], "InvalidArgument");
}

#[test]
fn the_title_block_prints_what_the_document_knows_and_the_fields_a_person_typed() {
    let (mut state, mut kernel, part_tab, _drawing) = bored_box_and_drawing();
    ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "top", "scale": 0.5 }),
    );
    let answer = ok(
        &mut state,
        &mut kernel,
        "drawing_sheet_edit",
        json!({
            "projection_angle": "first",
            "title_block_fields": [
                { "key": "DocumentName" },
                { "key": "SheetNumber" },
                { "key": "Scale" },
                { "key": "ProjectionAngle" },
                { "key": "Author", "text": "A. Drafter" },
                { "key": "Material", "text": "AISI 304" },
                { "key": "unknown-to-this-build", "label": "Finish", "text": "Ra 1.6" },
            ],
        }),
    );
    let rows = answer["sheets"][0]["title_block"]["rows"]["rows"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "the filled rows: {}",
                answer["sheets"][0]["title_block"].clone()
            )
        });
    let pairs: Vec<(String, String)> = rows
        .iter()
        .map(|r| {
            (
                r["label"].as_str().unwrap_or_default().to_string(),
                r["value"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let labels: Vec<&str> = pairs.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "Title",
            "Sheet",
            "Scale",
            "Projection",
            "Drawn by",
            "Material",
            "Finish"
        ]
    );
    // The derived rows, filled from the document rather than from a string
    // somebody stored.
    assert_eq!(pairs[1].1, "1 / 1");
    assert_eq!(pairs[2].1, "1:2");
    assert_eq!(pairs[3].1, "First angle");
    // And the typed ones, printed as typed.
    assert_eq!(pairs[4].1, "A. Drafter");
    assert_eq!(pairs[5].1, "AISI 304");
    assert_eq!(pairs[6].1, "Ra 1.6");
    assert_eq!(answer["projection_angle"]["type"], "First");

    // A derived row that was TYPED is refused by name: an agent that typed a
    // sheet number must be told the engine fills it, or it will believe the
    // number it typed is on the paper.
    let error = refused(
        &mut state,
        &mut kernel,
        "drawing_sheet_edit",
        json!({ "title_block_fields": [{ "key": "SheetNumber", "text": "7 / 7" }] }),
    );
    assert_eq!(error["code"], "InvalidArgument");

    // A second sheet renumbers the first: the row is the sheet's POSITION,
    // not a stored string.
    let answer = ok(
        &mut state,
        &mut kernel,
        "drawing_sheet_edit",
        json!({ "add_sheet": true, "size": "A4", "orientation": "portrait" }),
    );
    assert_eq!(answer["sheets"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        answer["sheets"][0]["title_block"]["rows"]["rows"][1]["value"],
        "1 / 2"
    );
    assert_eq!(answer["sheets"][1]["extent_mm"], json!([210.0, 297.0]));

    // The second sheet goes; the LAST one cannot, because a drawing with no
    // sheet shows nothing and refuses every export by name.
    let second = answer["sheets"][1]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    let answer = ok(
        &mut state,
        &mut kernel,
        "drawing_sheet_edit",
        json!({ "delete_sheet": true, "sheet_id": second }),
    );
    let first = answer["sheets"][0]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    let error = refused(
        &mut state,
        &mut kernel,
        "drawing_sheet_edit",
        json!({ "delete_sheet": true, "sheet_id": first }),
    );
    assert_eq!(error["code"], "NotFound");
}

#[test]
fn the_projection_standard_flips_where_a_freshly_added_section_is_placed() {
    // A section follows the same standard as any other projected view: third
    // angle places it on the side it is viewed FROM, first angle on the side
    // the arrows point to. So the SAME cutting line, authored under the two
    // standards, lands on opposite sides of its parent.
    let (mut state, mut kernel, part_tab, _drawing) = bored_box_and_drawing();
    let mut placements = Vec::new();
    for angle in ["third", "first"] {
        ok(
            &mut state,
            &mut kernel,
            "drawing_sheet_edit",
            json!({ "projection_angle": angle }),
        );
        let front = ok(
            &mut state,
            &mut kernel,
            "drawing_view_add",
            json!({ "tab_id": part_tab, "view": "front", "placement_mm": [150.0, 150.0] }),
        )["view_id"]
            .as_str()
            .expect("an id")
            .to_string();
        let answer = ok(
            &mut state,
            &mut kernel,
            "drawing_view_add",
            json!({
                "tab_id": part_tab,
                "parent_view_id": front,
                // A VERTICAL cutting line, so the section is placed left or
                // right of its parent and the flip shows in x.
                "section_mm": [W * 1000.0 / 2.0, -2.0, W * 1000.0 / 2.0, H * 1000.0 + 2.0],
            }),
        );
        let id = answer["view_id"].as_str().expect("an id").to_string();
        let view = answer["sheets"][0]["views"]
            .as_array()
            .expect("views")
            .iter()
            .find(|v| v["id"] == id.as_str())
            .expect("the section")
            .clone();
        placements.push(view["placement_mm"][0].as_f64().expect("an x"));
    }
    assert!(
        (placements[0] - 150.0) * (placements[1] - 150.0) < 0.0,
        "the two standards must place the section on opposite sides of its parent at x = 150, \
         got {placements:?}"
    );
}

#[test]
fn a_views_cache_carries_the_key_it_was_built_from_and_the_key_is_a_function_of_the_document() {
    // The D4a open item this closes: without a key a persisted layout is
    // indistinguishable from a current one, so a document opened in a build
    // with no kernel draws last week's sheet with no sign of it.
    let (mut state, mut kernel, part_tab, _drawing) = bored_box_and_drawing();
    let answer = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "top" }),
    );
    let view = answer["view_id"].as_str().expect("an id").to_string();
    let key_of = |answer: &Value| -> String {
        answer["sheets"][0]["views"]
            .as_array()
            .expect("views")
            .iter()
            .find(|v| v["id"] == view.as_str())
            .expect("the view")["cache_key"]
            .as_str()
            .unwrap_or_else(|| panic!("the view has no cache key: {answer}"))
            .to_string()
    };
    let first = key_of(&answer);
    assert!(first.starts_with("d4b-"), "{first}");

    // The view's NAME is part of its recipe (it is printed over the view), so
    // renaming moves the key — and renaming to the same name twice gives the
    // same key, which is the "it is a function of the document" half.
    let renamed = key_of(&ok(
        &mut state,
        &mut kernel,
        "drawing_view_edit",
        json!({ "view_id": view, "name": "Plan" }),
    ));
    let again = key_of(&ok(
        &mut state,
        &mut kernel,
        "drawing_view_edit",
        json!({ "view_id": view, "name": "Plan" }),
    ));
    assert_ne!(first, renamed, "an edit must move the key");
    // THE regression this test exists for. The first version of the key
    // digested the source tab's tree with `serde_json::to_string`, and a
    // `FeatureTree` holds `HashMap`s (`Sketch::solved_positions`) — so two
    // rebuilds of the same unedited part produced two strings differing in
    // key ORDER, the key moved on every rebuild, and a cache that is always
    // stale is the same as no cache key at all. Both digests now go through
    // `serde_json::Value`, whose objects are `BTreeMap`s.
    assert_eq!(
        renamed, again,
        "the key must be a function of the document, not of a HashMap's iteration order"
    );
    let third = key_of(&ok(
        &mut state,
        &mut kernel,
        "drawing_view_edit",
        json!({ "view_id": view, "name": "Plan" }),
    ));
    assert_eq!(renamed, third, "and stable across any number of rebuilds");

    // And the key goes with the cache, both ways: a view the rebuild could
    // not produce has neither.
    let drawing = state.session.drawing(&_drawing).expect("a drawing").clone();
    for sheet in &drawing.sheets {
        for v in &sheet.views {
            assert_eq!(
                v.cache.is_some(),
                v.cache_key.is_some(),
                "view `{}`: a key without a layout says a layout that is not there is current",
                v.name
            );
        }
    }
}

/// The half a section KEEPS is the half its arrows point into — measured on a
/// part that can tell the two apart (D4b review).
///
/// ## Why the other section tests cannot catch this
///
/// A section view looks ALONG the cut normal, so the kept half's extent in
/// that direction is edge-on and invisible in the view's 2-D box; and the cap
/// is the solid's cross-section at the plane, which is the SAME for both
/// halves whichever one is kept. On a plate bored down the middle, the two
/// halves are congruent and project identically — `flip` could invert the
/// kernel's kept side, or the arrows could point the wrong way, and every
/// assertion in `a_horizontal_section_of_a_bored_box_hatches_one_outer_loop_and_one_hole`
/// would still pass. That test pins the cap's SHAPE and the arrow's direction
/// against the algebra; this one pins the algebra against the solid.
///
/// ## The measurement
///
/// Bore the plate OFF CENTRE in `u` (at `W/4`) and cut vertically at `W/2`, so
/// the bore lies entirely in one half. The derivation says which:
/// `cut_line_2d` returns the line's left perpendicular, which points at the
/// DISCARDED side, and `KernelProjection::section_with_plane` keeps
/// `(p − origin)·n̂ ≤ 0`. For a line drawn `+v` at `u = W/2` that normal is
/// `−u`, so the kept half is `u ≥ W/2` — which EXCLUDES a bore at `W/4`. With
/// `flip` the kept half includes it.
///
/// The oracle is frame-independent, because the section's own `(u, v)` depends
/// on a basis this test is not about: a curve is INTERIOR when all of its
/// points sit strictly inside the view's own box along one axis. The plain
/// half is a rectangular prism seen end-on, so every curve lies on the
/// boundary; the bore contributes its cylinder's silhouette, which runs
/// through the middle. Zero interior curves against at least one is the whole
/// difference, and it is exactly the difference a mirrored kept side inverts.
#[test]
fn a_sections_kept_half_is_the_one_its_arrows_point_into() {
    let (mut state, mut kernel, part_tab, drawing_tab) = bored_box_and_drawing_at(W / 4.0, D / 2.0);
    let front = ok(
        &mut state,
        &mut kernel,
        "drawing_view_add",
        json!({ "tab_id": part_tab, "view": "front", "placement_mm": [100.0, 100.0] }),
    )["view_id"]
        .as_str()
        .expect("the front view's id")
        .to_string();

    // A VERTICAL cutting line at x = W/2, drawn upwards, running past both
    // ends of the part as a drafter draws it. The front view's (u, v) is
    // (world x, world z), and the wire takes millimetres.
    let cut_u = W * 1000.0 / 2.0;
    let mut interiors = Vec::new();
    for flip in [false, true] {
        let answer = ok(
            &mut state,
            &mut kernel,
            "drawing_view_add",
            json!({
                "tab_id": part_tab,
                "parent_view_id": front,
                "section_mm": [cut_u, -2.0, cut_u, H * 1000.0 + 2.0],
                "flip": flip,
            }),
        );
        let id = answer["view_id"]
            .as_str()
            .expect("the section's id")
            .to_string();
        let laid_out = layout(&state, &drawing_tab, &id);
        interiors.push((flip, interior_curve_count(&laid_out), laid_out.curves.len()));
    }

    let (_, plain_interior, plain_total) = interiors[0];
    let (_, bored_interior, bored_total) = interiors[1];
    assert_eq!(
        plain_interior, 0,
        "the unflipped cut keeps u >= W/2, which has no bore in it, so every curve should lie on \
         the view's own boundary — got {plain_interior} interior of {plain_total} curves"
    );
    assert!(
        bored_interior > 0,
        "the flipped cut keeps u <= W/2, which contains the bore at W/4, so the bore's silhouette \
         should run through the middle of the view — got {bored_interior} interior of \
         {bored_total} curves"
    );
}

/// How many of `layout`'s curves lie strictly inside its own box along one
/// axis — see `a_sections_kept_half_is_the_one_its_arrows_point_into`.
///
/// One millimetre of inset, in model units: the bore sits `D/2 − BORE_R` = 3 mm
/// from the nearer wall, so the band is comfortably clear of both the boundary
/// curves it must exclude and the bore curves it must find.
fn interior_curve_count(layout: &ViewLayout) -> usize {
    let Some([[min_u, min_v], [max_u, max_v]]) = layout.bbox else {
        return 0;
    };
    const INSET: f64 = 0.001;
    layout
        .curves
        .iter()
        .filter(|c| {
            let pts = curve_extent_points(&c.geometry);
            if pts.is_empty() {
                return false;
            }
            let inside_u = pts
                .iter()
                .all(|p| p[0] > min_u + INSET && p[0] < max_u - INSET);
            let inside_v = pts
                .iter()
                .all(|p| p[1] > min_v + INSET && p[1] < max_v - INSET);
            inside_u || inside_v
        })
        .count()
}

/// Points that bound a layout curve: its ends for a segment, its box corners
/// for a conic. Conservative — a conic's box contains the arc, so a curve is
/// only called interior when it certainly is.
fn curve_extent_points(curve: &LayoutCurve) -> Vec<[f64; 2]> {
    match curve {
        LayoutCurve::Point { at } => vec![*at],
        LayoutCurve::Line { start, end } => vec![*start, *end],
        LayoutCurve::Circle { center, radius, .. } => vec![
            [center[0] - radius, center[1] - radius],
            [center[0] + radius, center[1] + radius],
        ],
        LayoutCurve::Ellipse {
            center,
            major_radius,
            ..
        } => vec![
            [center[0] - major_radius, center[1] - major_radius],
            [center[0] + major_radius, center[1] + major_radius],
        ],
        LayoutCurve::Polyline { points, .. } => points.clone(),
    }
}
