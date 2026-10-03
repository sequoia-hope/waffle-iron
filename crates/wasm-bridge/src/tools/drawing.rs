//! The drawing tools (`specs/drawings_and_mbd.md` §8, D4a):
//! `drawing_view_add`, `drawing_view_edit`, `drawing_annotation_add`.
//!
//! Shaped on [`crate::tools::assembly`], tool for tool: each edit mutates the
//! open Drawing tab's content and sends one `EditDrawing`, which re-evaluates
//! the tab and writes the view layouts back; the answer is the evaluated
//! state read after that. A drawing edit is not an undo step — undo and redo
//! act on a Part tab's feature tree — so a refused edit never reaches
//! `EditDrawing`, and a view that fails to rebuild leaves the document as the
//! edit left it with the failure named per view.
//!
//! One exception, in `drawing_annotation_add`: an annotation the rebuild
//! could not resolve is rolled back out of the document. The rebuild reports
//! such a failure per annotation rather than failing the view, because a
//! dimension whose entity the model has since lost must not blank a sheet —
//! but an annotation being added right now never worked, so there is nothing
//! to preserve and leaving it would be a drawing carrying a dimension that
//! draws nothing.
//!
//! The gate is "a Drawing tab is active", the mirror of the feature tools'
//! Part-tab gate (G7) and the assembly tools' Assembly-tab one.
//!
//! **What these tools cannot author**, by design:
//!
//! - **A dimension's value.** There is no `value` argument. The number is
//!   measured from the model every rebuild (`Measured::FromGeometry`); a
//!   literal is refused at the engine boundary
//!   (`feature_engine::drawing::check_measured`) and is not reachable from
//!   here at all, which is the point of §7's refusal.
//! - **An anchor that is not a persistent id.** Anchors are given as pids
//!   (`entity_pid`, D0), because a selector that can rebind is how an
//!   annotation comes to dimension the wrong edge.
//! - **A section or detail view.** Those are D4b's, and the variants do not
//!   exist to be named.

use feature_engine::drawing::{
    auto_placement_mm, dimension_kind_from_tag, Drawing, DrawingView, NamedView,
    ProjectedDirection, Projection, Sheet, ViewSource, DEFAULT_VIEW_GAP_MM,
};
use modeling_ops::KernelBundle;
use serde_json::{json, Value};
use uuid::Uuid;
use waffle_types::annotation::{Annotation, Measured, Placement2};
use waffle_types::geom_ref::{Anchor, GeomRef, OutputKey, ResolvePolicy, Selector};
use waffle_types::kernel::TopoKind;

use super::{Answer, ToolFailure};
use crate::engine_state::EngineState;
use crate::messages::{EngineToUi, UiToEngine};

/// The drawing tools: the ones whose gate is "a Drawing tab is active".
pub const DRAWING_TOOLS: &[&str] = &[
    "drawing_view_add",
    "drawing_view_edit",
    "drawing_annotation_add",
];

/// The annotation kinds these tools author, as their `type` tags.
pub const ANNOTATION_TAGS: &[&str] = &["Dimension", "Note", "CentreMark", "CentreLine", "Datum"];

/// The dimension kinds, as their tags. `Ordinate` is absent: it reads one raw
/// view-plane coordinate measured from the view frame's origin, which is a
/// property of the projection rather than of the part, so its printed value
/// cannot be read off the sheet (§7's open item). It becomes authorable when
/// it gains an origin anchor.
pub const DIMENSION_TAGS: &[&str] = &[
    "Distance",
    "PointLineDistance",
    "HDistance",
    "VDistance",
    "Angle",
    "Radius",
    "Diameter",
];

/// The active Drawing tab, or the G7-shaped refusal.
fn require_drawing_tab(state: &EngineState) -> Result<crate::session::TabInfo, ToolFailure> {
    let active = state.session.active_tab_id().to_string();
    let tab = state.session.tabs().into_iter().find(|t| t.id == active);
    let kind = tab
        .as_ref()
        .map(|t| t.kind.clone())
        .unwrap_or_else(|| "Part".to_string());
    match tab {
        Some(tab) if kind == "Drawing" => Ok(tab),
        _ => Err(ToolFailure::new(
            "TabKindNotSupported",
            format!(
                "The active tab is a {kind} tab; drawing tools need a Drawing tab \
                 (tab_add kind:\"Drawing\" or tab_switch)."
            ),
            json!({ "kind": kind }),
        )),
    }
}

/// The open drawing's content.
fn drawing_of(state: &EngineState, tab_id: &str) -> Result<Drawing, ToolFailure> {
    state
        .session
        .drawing(tab_id)
        .cloned()
        .map_err(|e| ToolFailure::new("Internal", e.to_string(), json!({})))
}

/// Send one `EditDrawing`, restoring the previous content if the engine
/// refuses it.
fn commit(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    tab_id: &str,
    mut drawing: Drawing,
) -> Result<(), ToolFailure> {
    let previous = state.session.drawing(tab_id).cloned().ok();
    // Caches are derived; the evaluation writes them. Clearing them here
    // means a refused edit cannot leave a layout from the attempt behind.
    for sheet in &mut drawing.sheets {
        for view in &mut sheet.views {
            view.cache = None;
        }
    }
    let response = crate::dispatch::dispatch(
        state,
        UiToEngine::EditDrawing {
            tab_id: tab_id.to_string(),
            drawing,
        },
        kb,
    );
    if let EngineToUi::Error { message, .. } = response {
        if let Some(previous) = previous {
            // Best effort: the refusal is what the agent gets either way.
            let _ = state.session.set_drawing(tab_id, previous);
        }
        return Err(ToolFailure::new(
            "DrawingEditFailed",
            format!("The drawing edit failed: {message}"),
            json!({ "reason": message }),
        ));
    }
    // The evaluation (re)built part engines whose bodies have no meshes yet;
    // only a top-level `ModelUpdated` is tessellated by `process_message`, so
    // the tool does it here, as the assembly edits do.
    crate::tessellation_runner::tessellate_missing_meshes(state, kb);
    Ok(())
}

/// The evaluated drawing as an answer: the sheets, their views, and what the
/// evaluation had to say about each.
///
/// Every edit answers with this, which is also how an agent learns the view
/// ids it needs for the next call. The layouts themselves are NOT here —
/// they are curve lists, a megabyte of them on a real part, and an agent that
/// wants the drawing itself asks for `export_svg` or `export_dxf`.
fn drawing_state(state: &EngineState, tab_id: &str) -> Answer {
    let drawing = drawing_of(state, tab_id)?;
    let open = state.drawing.as_ref().filter(|d| d.tab_id == tab_id);
    let sheets: Vec<Value> = drawing
        .sheets
        .iter()
        .map(|sheet| {
            let views: Vec<Value> = sheet
                .views
                .iter()
                .map(|view| {
                    let cached = view.cache.as_ref();
                    json!({
                        "id": view.id,
                        "name": view.name,
                        "source": { "tab_id": view.source.tab_id, "bodies": view.source.bodies },
                        "projection": view.projection,
                        "scale": view.scale,
                        "placement_mm": view.placement_mm,
                        "style": view.style,
                        "annotations": view.annotations.len(),
                        // What the view actually drew, so an agent can tell a
                        // view that produced nothing from one it never
                        // rebuilt without asking for the curves.
                        "curves": cached.map(|c| c.curves.len()),
                        "bbox": cached.and_then(|c| c.bbox),
                    })
                })
                .collect();
            json!({
                "id": sheet.id,
                "name": sheet.name,
                "size": sheet.size,
                "orientation": sheet.orientation,
                "extent_mm": sheet.extent_mm(),
                "views": views,
            })
        })
        .collect();
    Ok(json!({
        "tab_id": tab_id,
        "sheets": sheets,
        "projection_angle": drawing.projection_angle,
        "declines": open.map(|d| d.declines.clone()).unwrap_or_default(),
        "errors": open.map(|d| d.errors.clone()).unwrap_or_default(),
        "warnings": open.map(|d| d.warnings.clone()).unwrap_or_default(),
    }))
}

/// The sheet an argument names, or the drawing's only/first one.
fn sheet_index(drawing: &Drawing, args: &Value) -> Result<usize, ToolFailure> {
    match uuid_arg(args, "sheet_id")? {
        Some(id) => drawing
            .sheets
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| {
                ToolFailure::new(
                    "NotFound",
                    format!("This drawing has no sheet {id}."),
                    json!({ "sheet_id": id }),
                )
            }),
        None => {
            if drawing.sheets.is_empty() {
                return Err(ToolFailure::new(
                    "NotFound",
                    "This drawing has no sheets.".to_string(),
                    json!({}),
                ));
            }
            Ok(0)
        }
    }
}

/// `drawing_view_add {tab_id, bodies?, view? | direction?+up? | parent_view_id?+direction?,
/// sheet_id?, name?, scale?, placement_mm?}`.
pub(crate) fn drawing_view_add(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab = require_drawing_tab(state)?;
    let mut drawing = drawing_of(state, &tab.id)?;
    let sheet = sheet_index(&drawing, args)?;
    let source = view_source(state, &tab.id, args)?;
    let projection = projection_arg(&drawing.sheets[sheet], args)?;
    let scale = scale_arg(args)?.unwrap_or(1.0);

    let count = drawing.sheets[sheet].views.len();
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_view_name(&projection, count));
    let mut view = DrawingView::new(name, source, projection.clone());
    view.scale = scale;
    view.placement_mm = match placement_arg(args, "placement_mm")? {
        Some(p) => p,
        // No placement given: a projected view goes clear of its parent, and
        // anything else goes in the middle of the sheet. Both are computed
        // from the extents the LAST evaluation measured, which is why this
        // runs before the view is added rather than after.
        None => default_placement(state, &drawing, sheet, &projection),
    };
    let id = view.id;
    drawing.sheets[sheet].views.push(view);
    commit(state, kb, &tab.id, drawing)?;
    let mut answer = json!({ "view_id": id });
    super::tabs::merge(&mut answer, drawing_state(state, &tab.id)?);
    Ok(answer)
}

/// `drawing_view_edit {view_id, name?, scale?, placement_mm?, bodies?,
/// hidden_lines?, silhouettes?}`.
pub(crate) fn drawing_view_edit(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab = require_drawing_tab(state)?;
    let mut drawing = drawing_of(state, &tab.id)?;
    let view_id = required_uuid(args, "view_id")?;
    let scale = scale_arg(args)?;
    let placement = placement_arg(args, "placement_mm")?;
    let bodies = bodies_arg(args)?;
    let sheet = drawing
        .sheet_of_view_mut(view_id)
        .ok_or_else(|| view_not_found(view_id))?;
    let view = sheet
        .view_mut(view_id)
        .ok_or_else(|| view_not_found(view_id))?;
    if let Some(name) = args
        .get("name")
        .and_then(Value::as_str)
        .filter(|n| !n.is_empty())
    {
        view.name = name.to_string();
    }
    if let Some(scale) = scale {
        view.scale = scale;
    }
    if let Some(placement) = placement {
        view.placement_mm = placement;
    }
    if let Some(bodies) = bodies {
        view.source.bodies = bodies;
    }
    if let Some(v) = args.get("hidden_lines").filter(|v| !v.is_null()) {
        view.style.hidden_lines = truthy(v);
    }
    if let Some(v) = args.get("silhouettes").filter(|v| !v.is_null()) {
        view.style.silhouettes = truthy(v);
    }
    commit(state, kb, &tab.id, drawing)?;
    drawing_state(state, &tab.id)
}

/// `drawing_annotation_add {view_id, annotation, kind?, anchors, text?, label?,
/// precision?, dual_unit?, placement?}`.
pub(crate) fn drawing_annotation_add(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab = require_drawing_tab(state)?;
    let mut drawing = drawing_of(state, &tab.id)?;
    let view_id = required_uuid(args, "view_id")?;
    let annotation = annotation_arg(args)?;
    let sheet = drawing
        .sheet_of_view_mut(view_id)
        .ok_or_else(|| view_not_found(view_id))?;
    let view = sheet
        .view_mut(view_id)
        .ok_or_else(|| view_not_found(view_id))?;
    let index = view.annotations.len();
    view.annotations.push(annotation);
    let before = drawing_of(state, &tab.id)?;
    commit(state, kb, &tab.id, drawing)?;

    // An annotation that could not be resolved does not stay in the
    // document. The rebuild reports such a failure per annotation rather than
    // failing the view — a dimension whose entity the model lost must not
    // blank the sheet — but an annotation the CALLER is adding right now is a
    // different case: it never worked, so there is nothing to preserve, and
    // leaving it would be a drawing carrying a dimension that draws nothing.
    let failed = state
        .drawing
        .as_ref()
        .is_some_and(|d| d.annotation_errors.contains(&(view_id, index)));
    if failed {
        let reason = state
            .drawing
            .as_ref()
            .map(|d| d.errors.join("; "))
            .unwrap_or_default();
        commit(state, kb, &tab.id, before)?;
        return Err(ToolFailure::new(
            "AnnotationNotMeasurable",
            format!("The annotation was not added: {reason}"),
            json!({ "reason": reason }),
        ));
    }
    let mut answer = json!({ "annotation_index": index });
    super::tabs::merge(&mut answer, drawing_state(state, &tab.id)?);
    Ok(answer)
}

// ------------------------------------------------------------- arguments

fn view_not_found(id: Uuid) -> ToolFailure {
    ToolFailure::new(
        "NotFound",
        format!("This drawing has no view {id}."),
        json!({ "view_id": id }),
    )
}

fn uuid_arg(args: &Value, name: &str) -> Result<Option<Uuid>, ToolFailure> {
    let Some(value) = args.get(name).filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let text = value.as_str().unwrap_or_default();
    Uuid::parse_str(text).map(Some).map_err(|_| {
        ToolFailure::new(
            "InvalidArgument",
            format!("{name} must be a UUID, not `{text}`."),
            json!({ "path": format!("/{name}") }),
        )
    })
}

fn required_uuid(args: &Value, name: &str) -> Result<Uuid, ToolFailure> {
    uuid_arg(args, name)?.ok_or_else(|| {
        ToolFailure::new(
            "InvalidArgument",
            format!("{name} is required."),
            json!({ "path": format!("/{name}") }),
        )
    })
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// The view's scale, refused when it is not a drawable ratio. Checked here as
/// well as in the rebuild so the agent is told which ARGUMENT is wrong.
fn scale_arg(args: &Value) -> Result<Option<f64>, ToolFailure> {
    let Some(v) = args.get("scale").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let n = v.as_f64().unwrap_or(f64::NAN);
    if !(n.is_finite() && n > 0.0) {
        return Err(ToolFailure::new(
            "InvalidArgument",
            format!("scale must be a positive ratio (1 is 1:1, 0.1 is 1:10), not {v}."),
            json!({ "path": "/scale" }),
        ));
    }
    Ok(Some(n))
}

fn placement_arg(args: &Value, name: &str) -> Result<Option<[f64; 2]>, ToolFailure> {
    let Some(v) = args.get(name).filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let bad = || {
        ToolFailure::new(
            "InvalidArgument",
            format!("{name} must be two finite numbers, millimetres on the sheet."),
            json!({ "path": format!("/{name}") }),
        )
    };
    let array = v.as_array().ok_or_else(bad)?;
    if array.len() != 2 {
        return Err(bad());
    }
    let mut out = [0.0; 2];
    for (i, item) in array.iter().enumerate() {
        out[i] = item.as_f64().filter(|f| f.is_finite()).ok_or_else(bad)?;
    }
    Ok(Some(out))
}

fn bodies_arg(args: &Value) -> Result<Option<Vec<String>>, ToolFailure> {
    let Some(v) = args.get("bodies").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let array = v.as_array().ok_or_else(|| {
        ToolFailure::new(
            "InvalidArgument",
            "bodies must be an array of body names.".to_string(),
            json!({ "path": "/bodies" }),
        )
    })?;
    Ok(Some(
        array
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
    ))
}

/// The view's source: a Part or Assembly tab of this document, and which of
/// its bodies.
fn view_source(
    state: &EngineState,
    drawing_tab: &str,
    args: &Value,
) -> Result<ViewSource, ToolFailure> {
    let tab_id = args
        .get("tab_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let tabs = state.session.tabs();
    let found = tabs.iter().find(|t| t.id == tab_id);
    match found {
        // A drawing of itself has no bodies and would recurse through
        // `OpenDrawing`; refused by name.
        Some(t) if t.id == drawing_tab => Err(ToolFailure::new(
            "InvalidArgument",
            "A drawing cannot draw itself; name a Part or Assembly tab.".to_string(),
            json!({ "path": "/tab_id" }),
        )),
        Some(t) if t.kind == "Part" || t.kind == "Assembly" => Ok(ViewSource {
            tab_id,
            bodies: bodies_arg(args)?.unwrap_or_default(),
        }),
        Some(t) => Err(ToolFailure::new(
            "TabKindNotSupported",
            format!(
                "Tab {} is a {} tab; a view draws a Part or an Assembly.",
                t.name, t.kind
            ),
            json!({ "kind": t.kind }),
        )),
        None => Err(ToolFailure::new(
            "TabNotFound",
            format!("This document has no tab {tab_id} to draw."),
            json!({ "tab_id": tab_id }),
        )),
    }
}

/// The view's projection from `view` / `direction` + `up` / `parent_view_id` +
/// `direction_from_parent`. Giving more than one is a refusal, not a silent
/// precedence rule — the same call `export_dxf`'s `view_arguments` makes.
fn projection_arg(sheet: &Sheet, args: &Value) -> Result<Projection, ToolFailure> {
    let named = args.get("view").filter(|v| !v.is_null());
    let direction = args.get("direction").filter(|v| !v.is_null());
    let parent = args.get("parent_view_id").filter(|v| !v.is_null());
    if [named.is_some(), direction.is_some(), parent.is_some()]
        .iter()
        .filter(|x| **x)
        .count()
        > 1
    {
        return Err(ToolFailure::new(
            "InvalidArgument",
            "Give one of view, direction or parent_view_id.".to_string(),
            json!({ "path": "/view" }),
        ));
    }
    if parent.is_some() {
        let parent_id = required_uuid(args, "parent_view_id")?;
        if sheet.view(parent_id).is_none() {
            return Err(ToolFailure::new(
                "NotFound",
                format!(
                    "Sheet `{}` has no view {parent_id} to project from.",
                    sheet.name
                ),
                json!({ "parent_view_id": parent_id }),
            ));
        }
        let tag = args
            .get("direction_from_parent")
            .and_then(Value::as_str)
            .unwrap_or("right");
        let direction = match tag.to_ascii_lowercase().as_str() {
            "left" => ProjectedDirection::Left,
            "right" => ProjectedDirection::Right,
            "up" => ProjectedDirection::Up,
            "down" => ProjectedDirection::Down,
            other => {
                return Err(ToolFailure::new(
                    "InvalidArgument",
                    format!("direction_from_parent is left, right, up or down, not `{other}`."),
                    json!({ "path": "/direction_from_parent" }),
                ))
            }
        };
        return Ok(Projection::ProjectedFrom {
            parent: parent_id,
            direction,
        });
    }
    if let Some(v) = direction {
        let dir = vector3(v, "/direction")?;
        let up = match args.get("up").filter(|v| !v.is_null()) {
            None => None,
            Some(v) => Some(vector3(v, "/up")?),
        };
        // Refused here rather than as a per-view rebuild failure, so the
        // agent is told which argument is at fault.
        if waffle_types::kernel::ViewFrame::from_parts(Some(dir), up)
            .basis()
            .is_none()
        {
            return Err(ToolFailure::new(
                "InvalidArgument",
                "direction and up give no view plane (up must not be parallel to the line of \
                 sight, and direction must have a length)."
                    .to_string(),
                json!({ "path": "/direction" }),
            ));
        }
        return Ok(Projection::Custom { dir, up });
    }
    let name = match named {
        None => "front",
        Some(Value::String(s)) => s.as_str(),
        Some(other) => {
            return Err(ToolFailure::new(
                "InvalidArgument",
                format!("view must be one of the named views, not {other}."),
                json!({ "path": "/view" }),
            ))
        }
    };
    let found = NamedView::ALL
        .iter()
        .find(|v| v.tag().eq_ignore_ascii_case(name))
        .copied()
        .ok_or_else(|| {
            ToolFailure::new(
                "InvalidArgument",
                format!(
                    "`{name}` is not a named view; use one of {}.",
                    NamedView::ALL
                        .iter()
                        .map(|v| v.tag().to_ascii_lowercase())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                json!({ "path": "/view" }),
            )
        })?;
    Ok(Projection::Named { view: found })
}

fn vector3(v: &Value, path: &str) -> Result<[f64; 3], ToolFailure> {
    let bad = || {
        ToolFailure::new(
            "InvalidArgument",
            format!("{path} must be three finite numbers, not all zero."),
            json!({ "path": path }),
        )
    };
    let array = v.as_array().ok_or_else(bad)?;
    if array.len() != 3 {
        return Err(bad());
    }
    let mut out = [0.0; 3];
    for (i, item) in array.iter().enumerate() {
        out[i] = item.as_f64().filter(|f| f.is_finite()).ok_or_else(bad)?;
    }
    if out == [0.0; 3] {
        return Err(bad());
    }
    Ok(out)
}

/// A freshly added view's name: the projection's own name, numbered when the
/// sheet already carries one like it.
fn default_view_name(projection: &Projection, count: usize) -> String {
    match projection {
        Projection::Named { view } => view.tag().to_string(),
        Projection::Custom { .. } => format!("View {}", count + 1),
        Projection::ProjectedFrom { direction, .. } => format!("{direction:?} of parent"),
    }
}

/// Where a freshly added view goes when the caller gives no placement.
///
/// A projected view goes clear of its parent's DRAWN extent, which is why it
/// reads the last evaluation's layouts rather than guessing: the gap must be
/// between the drawings, not between their centres (the same mistake D3's
/// dimension layout made once). A view whose parent has not been evaluated
/// yet, or any other kind of view, goes in the middle of the sheet.
fn default_placement(
    state: &EngineState,
    drawing: &Drawing,
    sheet: usize,
    projection: &Projection,
) -> [f64; 2] {
    let sheet_ref = &drawing.sheets[sheet];
    let centre = {
        let extent = sheet_ref.extent_mm();
        [extent[0] / 2.0, extent[1] / 2.0]
    };
    let Projection::ProjectedFrom { parent, direction } = projection else {
        return centre;
    };
    let Some(parent_view) = sheet_ref.view(*parent) else {
        return centre;
    };
    let parent_extent = drawn_extent_mm(parent_view);
    // The new view's own extent is unknown until it is projected; the
    // parent's is the best available estimate of it, and a projected view of
    // the same part is the same size in one axis by construction.
    let _ = state;
    auto_placement_mm(
        parent_view.placement_mm,
        parent_extent,
        parent_extent,
        *direction,
        DEFAULT_VIEW_GAP_MM,
    )
}

/// A view's drawn size in sheet millimetres, from its cached layout.
fn drawn_extent_mm(view: &DrawingView) -> [f64; 2] {
    match view.cache.as_ref().and_then(|c| c.bbox) {
        Some([min, max]) => [
            (max[0] - min[0]) * 1000.0 * view.scale,
            (max[1] - min[1]) * 1000.0 * view.scale,
        ],
        None => [0.0, 0.0],
    }
}

/// The annotation an argument set describes.
fn annotation_arg(args: &Value) -> Result<Annotation, ToolFailure> {
    let tag = args
        .get("annotation")
        .and_then(Value::as_str)
        .unwrap_or("Dimension")
        .to_string();
    let placement = match placement_arg(args, "placement")? {
        Some(p) => Placement2::new(p[0], p[1]),
        None => Placement2::default(),
    };
    match tag.as_str() {
        "Dimension" => {
            let kind_tag = args
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("Distance");
            let kind = dimension_kind_from_tag(kind_tag).ok_or_else(|| {
                ToolFailure::new(
                    "InvalidArgument",
                    format!(
                        "`{kind_tag}` is not a dimension kind; use one of {}.",
                        DIMENSION_TAGS.join(", ")
                    ),
                    json!({ "path": "/kind" }),
                )
            })?;
            let anchors = anchors_arg(args, kind.arity())?;
            Ok(Annotation::Dimension {
                kind,
                anchors,
                // Never from an argument. See the module docs.
                value: Measured::FromGeometry,
                precision: precision_arg(args)?,
                dual_unit: args
                    .get("dual_unit")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                placement,
            })
        }
        "Note" => {
            let text = args
                .get("text")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    ToolFailure::new(
                        "InvalidArgument",
                        "a Note needs text.".to_string(),
                        json!({ "path": "/text" }),
                    )
                })?
                .to_string();
            let leader = match anchors_arg(args, 1) {
                Ok(mut a) => Some(a.remove(0)),
                // A note with no leader is a legal note.
                Err(_) if args.get("anchors").is_none() => None,
                Err(e) => return Err(e),
            };
            Ok(Annotation::Note {
                text,
                leader,
                placement,
            })
        }
        "CentreMark" => Ok(Annotation::CentreMark {
            anchor: anchors_arg(args, 1)?.remove(0),
        }),
        "CentreLine" => {
            let mut anchors = anchors_arg(args, 2)?;
            let b = anchors.remove(1);
            let a = anchors.remove(0);
            Ok(Annotation::CentreLine { anchors: [a, b] })
        }
        "Datum" => Ok(Annotation::Datum {
            label: args
                .get("label")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("A")
                .to_string(),
            anchor: anchors_arg(args, 1)?.remove(0),
            placement,
        }),
        other => Err(ToolFailure::new(
            "InvalidArgument",
            format!(
                "`{other}` is not an annotation kind; use one of {}.",
                ANNOTATION_TAGS.join(", ")
            ),
            json!({ "path": "/annotation" }),
        )),
    }
}

fn precision_arg(args: &Value) -> Result<Option<u8>, ToolFailure> {
    let Some(v) = args.get("precision").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let n = v.as_u64().filter(|n| *n <= 9).ok_or_else(|| {
        ToolFailure::new(
            "InvalidArgument",
            format!("precision is a whole number of decimal places, 0 to 9, not {v}."),
            json!({ "path": "/precision" }),
        )
    })?;
    Ok(Some(n as u8))
}

/// The annotation's anchors: `arity` persistent ids, each with the kind of
/// entity it names.
///
/// An anchor is `{pid, kind?}` or a bare number (an edge, the common case).
/// `root_pid` equals `pid`: for edges and vertices D0 makes the two the same,
/// and for a face the caller who has a pid from `entity_pid` has its root
/// too — the argument is kept simple until something needs the distinction.
fn anchors_arg(args: &Value, arity: usize) -> Result<Vec<GeomRef>, ToolFailure> {
    let path = json!({ "path": "/anchors" });
    let array = args
        .get("anchors")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ToolFailure::new(
                "InvalidArgument",
                format!("anchors must be an array of {arity} persistent id(s)."),
                path.clone(),
            )
        })?;
    if array.len() != arity {
        return Err(ToolFailure::new(
            "InvalidArgument",
            format!(
                "this annotation measures {arity} anchor(s), {} given.",
                array.len()
            ),
            path.clone(),
        ));
    }
    let mut out = Vec::with_capacity(arity);
    for item in array {
        let (pid, kind) = match item {
            Value::Number(n) => (n.as_u64(), TopoKind::Edge),
            Value::Object(o) => {
                let kind = match o.get("kind").and_then(Value::as_str).unwrap_or("Edge") {
                    "Edge" => TopoKind::Edge,
                    "Face" => TopoKind::Face,
                    "Vertex" => TopoKind::Vertex,
                    other => {
                        return Err(ToolFailure::new(
                            "InvalidArgument",
                            format!("an anchor names an Edge, a Face or a Vertex, not `{other}`."),
                            path.clone(),
                        ))
                    }
                };
                (o.get("pid").and_then(Value::as_u64), kind)
            }
            _ => (None, TopoKind::Edge),
        };
        let pid = pid.ok_or_else(|| {
            ToolFailure::new(
                "InvalidArgument",
                "each anchor is a persistent id (entity_pid), or {pid, kind}.".to_string(),
                path.clone(),
            )
        })?;
        out.push(GeomRef {
            kind,
            // The anchor is immaterial for a `Selector::Pid`: the pid is
            // resolved against the view's own projection, not through a
            // feature output. Kept at the nil feature so the shape is
            // explicit rather than absent.
            anchor: Anchor::FeatureOutput {
                feature_id: Uuid::nil(),
                output_key: OutputKey::Main,
            },
            selector: Selector::Pid { pid, root_pid: pid },
            policy: ResolvePolicy::Strict,
            scope: None,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dimension_tags_are_exactly_the_kinds_the_engine_accepts() {
        // The tool's enumeration and the engine's parser must agree, or an
        // agent is offered a kind that is then refused.
        for tag in DIMENSION_TAGS {
            assert!(
                dimension_kind_from_tag(tag).is_some(),
                "{tag} is offered but not accepted"
            );
        }
        // And `Ordinate` is deliberately not offered (§7's open item).
        assert!(!DIMENSION_TAGS.contains(&"Ordinate"));
    }

    #[test]
    fn every_annotation_tag_builds_the_variant_it_names() {
        for tag in ANNOTATION_TAGS {
            let args = json!({
                "annotation": tag,
                "kind": "Distance",
                "anchors": [11, 12],
                "text": "note",
                "label": "A",
            });
            // `Distance` takes two anchors; the single-anchor kinds get the
            // arity refusal, which is itself the check that arity is read
            // from the kind rather than from the argument.
            match annotation_arg(&args) {
                Ok(a) => assert_eq!(feature_engine::drawing::annotation_tag(&a), *tag),
                Err(e) => assert_eq!(e.details["path"], "/anchors", "{tag}"),
            }
        }
        let err = annotation_arg(&json!({ "annotation": "FeatureControlFrame" })).unwrap_err();
        assert_eq!(err.code, "InvalidArgument");
        assert!(err.message.contains("FeatureControlFrame"));
    }

    #[test]
    fn a_dimension_never_takes_a_value_from_an_argument() {
        // The authoring half of §7's refusal: there is no `value` argument,
        // so a literal cannot enter a document through this door even when
        // the caller sends one.
        let args = json!({
            "annotation": "Dimension",
            "kind": "Radius",
            "anchors": [7],
            "value": 0.123,
        });
        let Annotation::Dimension { value, .. } = annotation_arg(&args).unwrap() else {
            panic!("not a dimension");
        };
        assert_eq!(value, Measured::FromGeometry);
    }

    #[test]
    fn an_anchor_is_a_persistent_id_and_nothing_else() {
        let args =
            json!({ "annotation": "CentreMark", "anchors": [{ "pid": 42, "kind": "Face" }] });
        let Annotation::CentreMark { anchor } = annotation_arg(&args).unwrap() else {
            panic!("not a centre mark");
        };
        assert_eq!(anchor.kind, TopoKind::Face);
        assert!(matches!(
            anchor.selector,
            Selector::Pid {
                pid: 42,
                root_pid: 42
            }
        ));
        // A bare number is an edge, the common case.
        let args = json!({ "annotation": "CentreMark", "anchors": [9] });
        let Annotation::CentreMark { anchor } = annotation_arg(&args).unwrap() else {
            panic!("not a centre mark");
        };
        assert_eq!(anchor.kind, TopoKind::Edge);
        // Anything that is not a pid is refused by name.
        let err = annotation_arg(&json!({ "annotation": "CentreMark", "anchors": ["face-3"] }))
            .unwrap_err();
        assert_eq!(err.details["path"], "/anchors");
    }

    #[test]
    fn a_scale_that_cannot_be_drawn_is_refused_as_an_argument() {
        for bad in [0.0, -2.0] {
            let err = scale_arg(&json!({ "scale": bad })).unwrap_err();
            assert_eq!(err.code, "InvalidArgument");
            assert_eq!(err.details["path"], "/scale");
        }
        assert_eq!(scale_arg(&json!({ "scale": 0.5 })).unwrap(), Some(0.5));
        assert_eq!(scale_arg(&json!({})).unwrap(), None);
    }

    #[test]
    fn a_projection_argument_set_names_one_projection_or_refuses() {
        let sheet = Sheet::new("S");
        // The default is the front view, the drafter's first.
        assert!(matches!(
            projection_arg(&sheet, &json!({})).unwrap(),
            Projection::Named {
                view: NamedView::Front
            }
        ));
        assert!(matches!(
            projection_arg(&sheet, &json!({ "view": "TOP" })).unwrap(),
            Projection::Named {
                view: NamedView::Top
            }
        ));
        // Two ways of saying it is a refusal, not a precedence rule.
        let err = projection_arg(
            &sheet,
            &json!({ "view": "top", "direction": [0.0, 0.0, -1.0] }),
        )
        .unwrap_err();
        assert_eq!(err.code, "InvalidArgument");
        // An unorientable pair is refused as an ARGUMENT, not as a rebuild
        // failure.
        let err = projection_arg(
            &sheet,
            &json!({ "direction": [0.0, 0.0, -1.0], "up": [0.0, 0.0, 1.0] }),
        )
        .unwrap_err();
        assert!(err.message.contains("no view plane"), "{}", err.message);
        // A parent that is not on the sheet.
        let err = projection_arg(
            &sheet,
            &json!({ "parent_view_id": Uuid::nil().to_string() }),
        )
        .unwrap_err();
        assert_eq!(err.code, "NotFound");
    }
}
