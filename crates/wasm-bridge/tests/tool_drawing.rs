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
