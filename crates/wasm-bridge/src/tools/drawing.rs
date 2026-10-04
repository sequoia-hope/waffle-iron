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
//! - **A title block row the engine fills.** `DocumentName`, `SheetNumber`,
//!   `Scale` and `ProjectionAngle` come from the document (D4b); giving one of
//!   them `text` is refused by name rather than dropped, because an agent that
//!   typed a sheet number would otherwise believe the number it typed is on
//!   the paper.
//!
//! Since D4b the tools also author a **section** (`section_mm`, a cutting
//! line in the parent view's own plane) and a **detail** (`detail_mm`, a crop
//! disc), and `drawing_sheet_edit` carries the sheet's paper, its title block
//! and the drawing's projection standard.

use feature_engine::drawing::{
    dimension_kind_from_tag, Drawing, NamedView, Orientation, ProjectedDirection, Projection,
    ProjectionAngle, Sheet, SheetSize, TitleBlockField, TitleBlockKey, ViewSource,
};
use modeling_ops::KernelBundle;
use serde_json::{json, Value};
use uuid::Uuid;
use waffle_types::kernel::TopoKind;

use super::{Answer, ToolFailure};
use crate::engine_state::EngineState;
use crate::messages::{
    DrawingAnchorSpec, DrawingAnnotationSpec, DrawingEdit, EngineToUi, UiToEngine,
};

/// The drawing tools: the ones whose gate is "a Drawing tab is active".
pub const DRAWING_TOOLS: &[&str] = &[
    "drawing_view_add",
    "drawing_view_edit",
    "drawing_annotation_add",
    "drawing_sheet_edit",
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
fn drawing_state(state: &EngineState, tab_id: &str, include_anchors: bool) -> Answer {
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
                        // A section's hatched cap regions and a parent's
                        // marks (D4b), as counts: an agent that wants the
                        // loops themselves asks for the drawing.
                        "hatch_loops": cached.map(|c| c.hatch.len()),
                        "marks": cached.map(|c| c.marks.len()),
                        // What the cache was built from. An agent comparing
                        // it across two calls can tell a view that was
                        // rebuilt from one that was not.
                        "cache_key": view.cache_key,
                        // The ids an annotation can anchor on. A COUNT by
                        // default and the list on request: a real part's view
                        // has thousands of edges, and an answer that carried
                        // them all on every add would be most of the wire.
                        "anchors": open
                            .map(|d| d.anchors.get(&view.id).map(Vec::len).unwrap_or(0))
                            .unwrap_or(0),
                        "anchor_list": (include_anchors)
                            .then(|| open.and_then(|d| d.anchors.get(&view.id)).cloned())
                            .flatten(),
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
                // The title block as AUTHORED and as FILLED (D4b). Both,
                // because they answer different questions: the fields say
                // what an edit would replace, the rows say what the paper
                // prints — and for a derived row those differ by design.
                "title_block": {
                    "show": sheet.title_block.show,
                    "fields": sheet.title_block.fields,
                    "rows": sheet.title_block_cache,
                },
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

/// Apply one edit through `drawing_view::apply_edit` — the same function the
/// page's `DrawingEdit` message uses — and answer with the evaluated drawing.
///
/// One implementation of "what this instruction does to the document", so a
/// tool and the UI cannot make different documents out of the same edit. The
/// ARGUMENTS are checked here first, where a refusal can name the argument at
/// fault; `apply_edit`'s own refusals are about the document.
fn edit(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    tab_id: &str,
    edit: DrawingEdit,
) -> Result<Uuid, ToolFailure> {
    let mut drawing = drawing_of(state, tab_id)?;
    let id = crate::drawing_view::apply_edit(&mut drawing, &edit).map_err(|reason| {
        ToolFailure::new("NotFound", reason.clone(), json!({ "reason": reason }))
    })?;
    commit(state, kb, tab_id, drawing)?;
    Ok(id)
}

/// `drawing_view_add {tab_id, bodies?, view? | direction?+up? |
/// parent_view_id?+direction_from_parent?, sheet_id?, name?, scale?,
/// placement_mm?, include_anchors?}`.
pub(crate) fn drawing_view_add(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab = require_drawing_tab(state)?;
    let drawing = drawing_of(state, &tab.id)?;
    let sheet = sheet_index(&drawing, args)?;
    let source = view_source(state, &tab.id, args)?;
    let projection = projection_arg(&drawing.sheets[sheet], args)?;
    let id = edit(
        state,
        kb,
        &tab.id,
        DrawingEdit::AddView {
            sheet_id: Some(drawing.sheets[sheet].id),
            source_tab: source.tab_id,
            bodies: source.bodies,
            projection,
            name: args
                .get("name")
                .and_then(Value::as_str)
                .filter(|n| !n.is_empty())
                .map(str::to_string),
            scale: scale_arg(args)?,
            placement_mm: placement_arg(args, "placement_mm")?,
        },
    )?;
    let mut answer = json!({ "view_id": id });
    super::tabs::merge(
        &mut answer,
        drawing_state(
            state,
            &tab.id,
            args.get("include_anchors").is_some_and(truthy),
        )?,
    );
    Ok(answer)
}

/// `drawing_view_edit {view_id, name?, scale?, placement_mm?, bodies?,
/// hidden_lines?, silhouettes?, include_anchors?}`.
pub(crate) fn drawing_view_edit(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab = require_drawing_tab(state)?;
    let view_id = required_uuid(args, "view_id")?;
    edit(
        state,
        kb,
        &tab.id,
        DrawingEdit::EditView {
            view_id,
            name: args
                .get("name")
                .and_then(Value::as_str)
                .filter(|n| !n.is_empty())
                .map(str::to_string),
            scale: scale_arg(args)?,
            placement_mm: placement_arg(args, "placement_mm")?,
            bodies: bodies_arg(args)?,
            hidden_lines: args
                .get("hidden_lines")
                .filter(|v| !v.is_null())
                .map(truthy),
            silhouettes: args.get("silhouettes").filter(|v| !v.is_null()).map(truthy),
        },
    )?;
    drawing_state(
        state,
        &tab.id,
        args.get("include_anchors").is_some_and(truthy),
    )
}

/// `drawing_annotation_add {view_id, annotation?, kind?, anchors, text?,
/// label?, precision?, dual_unit?, placement?}`.
pub(crate) fn drawing_annotation_add(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab = require_drawing_tab(state)?;
    let view_id = required_uuid(args, "view_id")?;
    let spec = annotation_arg(args)?;
    // Which index it will land at, read before the edit so the rollback
    // check below knows which annotation is the new one.
    let index = drawing_of(state, &tab.id)?
        .find_view(view_id)
        .map(|(_, v)| v.annotations.len())
        .ok_or_else(|| view_not_found(view_id))?;
    let before = drawing_of(state, &tab.id)?;
    edit(
        state,
        kb,
        &tab.id,
        DrawingEdit::AddAnnotation {
            view_id,
            annotation: spec,
        },
    )?;

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
    super::tabs::merge(&mut answer, drawing_state(state, &tab.id, false)?);
    Ok(answer)
}

/// `drawing_sheet_edit {sheet_id?, name?, size?, orientation?,
/// projection_angle?, title_block?, title_block_fields?, add_sheet?,
/// delete_sheet?}` (D4b).
///
/// One door for everything that is the SHEET's rather than a view's, plus the
/// drawing's projection standard — which closes D4a's "`projection_angle` has
/// no setter". It sits here because the standard is what a title block prints
/// and what a drafter changes once per document, not per view.
pub(crate) fn drawing_sheet_edit(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab = require_drawing_tab(state)?;
    if args.get("add_sheet").is_some_and(truthy) {
        if args
            .get("delete_sheet")
            .is_some_and(|v| !v.is_null() && truthy(v))
        {
            return Err(ToolFailure::new(
                "InvalidArgument",
                "Give add_sheet or delete_sheet, not both.".to_string(),
                json!({ "path": "/add_sheet" }),
            ));
        }
        let id = edit(
            state,
            kb,
            &tab.id,
            DrawingEdit::AddSheet {
                name: string_arg(args, "name"),
                size: sheet_size_arg(args)?,
                orientation: orientation_arg(args)?,
            },
        )?;
        let mut answer = json!({ "sheet_id": id });
        super::tabs::merge(&mut answer, drawing_state(state, &tab.id, false)?);
        return Ok(answer);
    }
    if args.get("delete_sheet").is_some_and(truthy) {
        let sheet_id = required_uuid(args, "sheet_id")?;
        let id = edit(state, kb, &tab.id, DrawingEdit::DeleteSheet { sheet_id })?;
        let mut answer = json!({ "sheet_id": id });
        super::tabs::merge(&mut answer, drawing_state(state, &tab.id, false)?);
        return Ok(answer);
    }
    let id = edit(
        state,
        kb,
        &tab.id,
        DrawingEdit::EditSheet {
            sheet_id: uuid_arg(args, "sheet_id")?,
            name: string_arg(args, "name"),
            size: sheet_size_arg(args)?,
            orientation: orientation_arg(args)?,
            projection_angle: projection_angle_arg(args)?,
            title_block_show: args.get("title_block").filter(|v| !v.is_null()).map(truthy),
            title_block_fields: title_block_fields_arg(args)?,
        },
    )?;
    let mut answer = json!({ "sheet_id": id });
    super::tabs::merge(&mut answer, drawing_state(state, &tab.id, false)?);
    Ok(answer)
}

// ------------------------------------------------------------- arguments

fn string_arg(args: &Value, name: &str) -> Option<String> {
    args.get(name)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// `A4` … `A0`, `Letter`, `Tabloid`, or `[width_mm, height_mm]` for a custom
/// sheet.
fn sheet_size_arg(args: &Value) -> Result<Option<SheetSize>, ToolFailure> {
    let Some(v) = args.get("size").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    if let Some(pair) = v.as_array() {
        let pair = numbers(&Value::Array(pair.clone()), 2, "/size")?;
        if !pair.iter().all(|x| *x > 0.0) {
            return Err(ToolFailure::new(
                "InvalidArgument",
                "A custom sheet's width and height are positive millimetres.".to_string(),
                json!({ "path": "/size" }),
            ));
        }
        return Ok(Some(SheetSize::Custom {
            width_mm: pair[0],
            height_mm: pair[1],
        }));
    }
    let tag = v.as_str().unwrap_or_default();
    Ok(Some(match tag.to_ascii_uppercase().as_str() {
        "A4" => SheetSize::A4,
        "A3" => SheetSize::A3,
        "A2" => SheetSize::A2,
        "A1" => SheetSize::A1,
        "A0" => SheetSize::A0,
        "LETTER" => SheetSize::Letter,
        "TABLOID" => SheetSize::Tabloid,
        _ => {
            return Err(ToolFailure::new(
                "InvalidArgument",
                format!(
                    "`{tag}` is not a sheet size; use A4, A3, A2, A1, A0, Letter, Tabloid, or \
                     [width_mm, height_mm]."
                ),
                json!({ "path": "/size" }),
            ))
        }
    }))
}

fn orientation_arg(args: &Value) -> Result<Option<Orientation>, ToolFailure> {
    let Some(v) = args.get("orientation").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    match v.as_str().unwrap_or_default().to_ascii_lowercase().as_str() {
        "landscape" => Ok(Some(Orientation::Landscape)),
        "portrait" => Ok(Some(Orientation::Portrait)),
        other => Err(ToolFailure::new(
            "InvalidArgument",
            format!("orientation is landscape or portrait, not `{other}`."),
            json!({ "path": "/orientation" }),
        )),
    }
}

fn projection_angle_arg(args: &Value) -> Result<Option<ProjectionAngle>, ToolFailure> {
    let Some(v) = args.get("projection_angle").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    // Both spellings a drafter uses, and the bare number from a drawing
    // standard. Not a free-text field: "1st" and "third" are the same
    // standard and must not become two.
    let text = match v {
        Value::Number(n) => n.to_string(),
        other => other.as_str().unwrap_or_default().to_string(),
    };
    match text.to_ascii_lowercase().as_str() {
        "third" | "3" | "3rd" | "third angle" => Ok(Some(ProjectionAngle::Third)),
        "first" | "1" | "1st" | "first angle" => Ok(Some(ProjectionAngle::First)),
        other => Err(ToolFailure::new(
            "InvalidArgument",
            format!(
                "projection_angle is `third` (the ISO/ASME default: the view placed to the right \
                 shows the right-hand side) or `first`, not `{other}`."
            ),
            json!({ "path": "/projection_angle" }),
        )),
    }
}

/// The title block's rows, as `[{key, text?, expr?}, …]`. The whole list,
/// replaced. `expr` is D4c's expression row: its evaluated text is printed
/// and its source is kept.
fn title_block_fields_arg(args: &Value) -> Result<Option<Vec<TitleBlockField>>, ToolFailure> {
    let Some(v) = args.get("title_block_fields").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let rows = v.as_array().ok_or_else(|| {
        ToolFailure::new(
            "InvalidArgument",
            "title_block_fields is a list of {key, text?}.".to_string(),
            json!({ "path": "/title_block_fields" }),
        )
    })?;
    let mut out = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let path = format!("/title_block_fields/{i}");
        let tag = row
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let text = row
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|t| !t.is_empty());
        let expr = row
            .get("expr")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|t| !t.trim().is_empty());
        let label = row.get("label").and_then(Value::as_str);
        let key = match (TitleBlockKey::from_tag(&tag), label) {
            (Some(key), _) => key,
            // A row whose key is not one of the known ones is a CUSTOM row
            // named by its label — the one way to print a field this version
            // has no name for, rather than refusing the whole title block.
            (None, Some(label)) if !label.is_empty() => TitleBlockKey::Custom {
                label: label.to_string(),
            },
            (None, _) => {
                return Err(ToolFailure::new(
                    "InvalidArgument",
                    format!(
                        "`{tag}` is not a title block field; use one of {} — or give a label for \
                         a row of your own.",
                        TitleBlockKey::AUTHORABLE.join(", ")
                    ),
                    json!({ "path": path }),
                ))
            }
        };
        if key.is_derived() && (text.is_some() || expr.is_some()) {
            // Named, not quietly dropped: an agent that typed a sheet number
            // must be told the engine fills it, or it will believe the number
            // it typed is on the paper.
            return Err(ToolFailure::new(
                "InvalidArgument",
                format!(
                    "`{tag}` is filled from the document (the name, the sheet number, the scale, \
                     the projection standard), so it takes neither text nor an expression."
                ),
                json!({ "path": path }),
            ));
        }
        if text.is_some() && expr.is_some() {
            // Refused rather than resolved by precedence (D4c). Both given is
            // an author who does not know which one the paper will show, and
            // silently picking the expression would print a number where the
            // caller believes its own text is.
            return Err(ToolFailure::new(
                "InvalidArgument",
                format!(
                    "the `{tag}` row was given both `text` and `expr`; a row prints one or the \
                     other, so send only the one you mean."
                ),
                json!({ "path": path }),
            ));
        }
        out.push(TitleBlockField {
            key,
            text,
            expr,
            extra: serde_json::Map::new(),
        });
    }
    Ok(Some(out))
}

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
    let section = args.get("section_mm").filter(|v| !v.is_null());
    let detail = args.get("detail_mm").filter(|v| !v.is_null());
    if [
        named.is_some(),
        direction.is_some(),
        // A section and a detail BOTH need a parent, so the parent argument
        // alone is not what distinguishes them — it is the geometry.
        parent.is_some() && section.is_none() && detail.is_none(),
        section.is_some(),
        detail.is_some(),
    ]
    .iter()
    .filter(|x| **x)
    .count()
        > 1
    {
        return Err(ToolFailure::new(
            "InvalidArgument",
            "Give one of view, direction, parent_view_id, section_mm or detail_mm.".to_string(),
            json!({ "path": "/view" }),
        ));
    }
    if let Some(line) = section {
        let parent_id = section_parent(sheet, args, "section_mm")?;
        let line = numbers(line, 4, "/section_mm")?;
        // Millimetres on the wire, meters in the document. The whole MCP
        // surface takes millimetres for a length an agent reads off a
        // drawing, and a section line typed in meters would be a cut a
        // thousand times further out than intended — a plane that misses the
        // part, answered as an empty section.
        let from = [line[0] / 1000.0, line[1] / 1000.0];
        let to = [line[2] / 1000.0, line[3] / 1000.0];
        let label = label_arg(sheet, args)?;
        if feature_engine::drawing::section_plane(&parent_basis(sheet, parent_id)?, from, to, false)
            .is_none()
        {
            return Err(ToolFailure::new(
                "InvalidArgument",
                "section_mm's two ends are the same point, so the line names no cut plane."
                    .to_string(),
                json!({ "path": "/section_mm" }),
            ));
        }
        return Ok(Projection::Section {
            parent: parent_id,
            from,
            to,
            flip: args.get("flip").is_some_and(truthy),
            label,
        });
    }
    if let Some(disc) = detail {
        let parent_id = section_parent(sheet, args, "detail_mm")?;
        let disc = numbers(disc, 3, "/detail_mm")?;
        if !(disc[2].is_finite() && disc[2] > 0.0) {
            return Err(ToolFailure::new(
                "InvalidArgument",
                format!(
                    "detail_mm's third number is the crop RADIUS and must be positive, not {}.",
                    disc[2]
                ),
                json!({ "path": "/detail_mm" }),
            ));
        }
        return Ok(Projection::Detail {
            parent: parent_id,
            center: [disc[0] / 1000.0, disc[1] / 1000.0],
            radius: disc[2] / 1000.0,
            label: label_arg(sheet, args)?,
        });
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

/// The parent a section or detail cuts, which it must have: there is nothing
/// for a cutting line to be drawn ON otherwise.
fn section_parent(sheet: &Sheet, args: &Value, what: &str) -> Result<Uuid, ToolFailure> {
    let id = uuid_arg(args, "parent_view_id")?.ok_or_else(|| {
        ToolFailure::new(
            "InvalidArgument",
            format!(
                "{what} is drawn ON another view, so parent_view_id names the view it is taken \
                 from."
            ),
            json!({ "path": "/parent_view_id" }),
        )
    })?;
    if sheet.view(id).is_none() {
        return Err(ToolFailure::new(
            "NotFound",
            format!("Sheet `{}` has no view {id}.", sheet.name),
            json!({ "parent_view_id": id }),
        ));
    }
    Ok(id)
}

/// The parent view's basis, for checking a cutting line before it is stored.
fn parent_basis(
    sheet: &Sheet,
    parent: Uuid,
) -> Result<waffle_types::kernel::projection::ViewBasis, ToolFailure> {
    // The angle is immaterial for the CHECK (a degenerate line is degenerate
    // under either standard) and the drawing's own is not in hand here, so the
    // default is used and documented rather than threaded through.
    sheet
        .view_frame(parent, feature_engine::drawing::ProjectionAngle::default())
        .ok()
        .and_then(|f| f.basis())
        .ok_or_else(|| {
            ToolFailure::new(
                "InvalidArgument",
                format!("View {parent} has no view plane to draw a cutting line on."),
                json!({ "parent_view_id": parent }),
            )
        })
}

/// A section or detail's letter: the one given, or the next free one on the
/// sheet.
fn label_arg(sheet: &Sheet, args: &Value) -> Result<String, ToolFailure> {
    match args.get("label").filter(|v| !v.is_null()) {
        None => Ok(sheet.next_label()),
        Some(Value::String(s)) if !s.is_empty() => Ok(s.clone()),
        Some(other) => Err(ToolFailure::new(
            "InvalidArgument",
            format!("label is the letter the cut is known by, not {other}."),
            json!({ "path": "/label" }),
        )),
    }
}

/// Exactly `n` finite numbers.
fn numbers(v: &Value, n: usize, path: &str) -> Result<Vec<f64>, ToolFailure> {
    let bad = || {
        ToolFailure::new(
            "InvalidArgument",
            format!("{path} must be {n} finite numbers."),
            json!({ "path": path }),
        )
    };
    let array = v.as_array().ok_or_else(bad)?;
    if array.len() != n {
        return Err(bad());
    }
    let out: Vec<f64> = array.iter().filter_map(Value::as_f64).collect();
    if out.len() != n || !out.iter().all(|x| x.is_finite()) {
        return Err(bad());
    }
    Ok(out)
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

/// The annotation an argument set describes, as the primitive SPEC the edit
/// carries.
///
/// Note what it cannot express: a dimension's `value`. There is no such
/// argument, so a literal number cannot enter a document through this door —
/// the authoring half of §7's refusal. `expr` (D4c) is the one that IS
/// offered, because an expression is re-measured on every rebuild and so
/// cannot go stale.
fn annotation_arg(args: &Value) -> Result<DrawingAnnotationSpec, ToolFailure> {
    // `value` is REFUSED by name rather than ignored (D4c). Before `expr`
    // existed there was nothing to offer instead, and an unread argument was
    // at least not a literal in the document; but an agent that sent a number
    // and heard nothing believes that number is on the paper — the same
    // argument the derived title-block rows are refused by.
    if args.get("value").is_some_and(|v| !v.is_null()) {
        return Err(ToolFailure::new(
            "InvalidArgument",
            "A drawing dimension is measured from the model, never typed in. Omit `value` to \
             measure the anchors, or pass `expr` for an expression the engine re-measures on \
             every rebuild."
                .to_string(),
            json!({ "path": "/value" }),
        ));
    }
    let annotation = args
        .get("annotation")
        .and_then(Value::as_str)
        .unwrap_or("Dimension")
        .to_string();
    if !ANNOTATION_TAGS.contains(&annotation.as_str()) {
        return Err(ToolFailure::new(
            "InvalidArgument",
            format!(
                "`{annotation}` is not an annotation kind; use one of {}.",
                ANNOTATION_TAGS.join(", ")
            ),
            json!({ "path": "/annotation" }),
        ));
    }
    let kind = args
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("Distance")
        .to_string();
    // The arity is a property of the KIND, and checking it here is what lets
    // the refusal name `/anchors` rather than describe the document.
    let arity = if annotation == "Dimension" {
        dimension_kind_from_tag(&kind)
            .ok_or_else(|| {
                ToolFailure::new(
                    "InvalidArgument",
                    format!(
                        "`{kind}` is not a dimension kind; use one of {}.",
                        DIMENSION_TAGS.join(", ")
                    ),
                    json!({ "path": "/kind" }),
                )
            })?
            .arity()
    } else {
        match annotation.as_str() {
            "CentreLine" => 2,
            // A note's leader is optional: no anchors is a legal note.
            "Note" => usize::from(args.get("anchors").is_some()),
            _ => 1,
        }
    };
    let anchors = anchors_arg(args, arity)?;
    Ok(DrawingAnnotationSpec {
        annotation,
        kind: Some(kind),
        anchors,
        text: args.get("text").and_then(Value::as_str).map(str::to_string),
        label: args
            .get("label")
            .and_then(Value::as_str)
            .map(str::to_string),
        expr: args
            .get("expr")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string),
        precision: precision_arg(args)?,
        dual_unit: args
            .get("dual_unit")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        placement: placement_arg(args, "placement")?,
    })
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
/// An anchor is a bare id (an edge, the common case) or `{pid, kind}`. The id
/// may be a NUMBER or a decimal STRING, and a caller that goes through
/// JavaScript must use the string: a `u64` above `2^53` is not exact as a
/// JSON number there, and the rounded value resolves to nothing
/// (`waffle_types::pid_str`). `root_pid` equals `pid` — for
/// edges and vertices D0 makes the two the same, and a caller holding a face
/// pid from `entity_pid` holds its root too.
fn anchors_arg(args: &Value, arity: usize) -> Result<Vec<DrawingAnchorSpec>, ToolFailure> {
    let path = json!({ "path": "/anchors" });
    if arity == 0 {
        return Ok(Vec::new());
    }
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
        let (raw, kind) = match item {
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
                (o.get("pid").cloned().unwrap_or(Value::Null), kind)
            }
            other => (other.clone(), TopoKind::Edge),
        };
        let pid = match &raw {
            Value::Number(n) => n.as_u64(),
            Value::String(s) => s.parse::<u64>().ok(),
            _ => None,
        };
        let pid = pid.ok_or_else(|| {
            ToolFailure::new(
                "InvalidArgument",
                "each anchor is a persistent id — a number, or a decimal string for one above \
                 2^53 — or {pid, kind}."
                    .to_string(),
                path.clone(),
            )
        })?;
        out.push(DrawingAnchorSpec { pid, kind });
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
    fn every_annotation_tag_parses_to_the_spec_it_names() {
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
            // from the KIND rather than from the argument.
            match annotation_arg(&args) {
                Ok(spec) => assert_eq!(spec.annotation, *tag),
                Err(e) => assert_eq!(e.details["path"], "/anchors", "{tag}"),
            }
        }
        let err = annotation_arg(&json!({ "annotation": "FeatureControlFrame" })).unwrap_err();
        assert_eq!(err.code, "InvalidArgument");
        assert!(err.message.contains("FeatureControlFrame"));
        // And every tag the tool offers builds a real annotation, which is
        // the half the spec alone does not prove.
        for (tag, anchors) in [
            ("Dimension", vec![json!(11), json!(12)]),
            ("Note", vec![json!(11)]),
            ("CentreMark", vec![json!(11)]),
            ("CentreLine", vec![json!(11), json!(12)]),
            ("Datum", vec![json!(11)]),
        ] {
            let spec = annotation_arg(&json!({
                "annotation": tag, "kind": "Distance", "anchors": anchors,
                "text": "note", "label": "A",
            }))
            .unwrap_or_else(|e| panic!("{tag}: {}", e.message));
            let built = crate::drawing_view::build_annotation(&spec)
                .unwrap_or_else(|e| panic!("{tag}: {e}"));
            assert_eq!(feature_engine::drawing::annotation_tag(&built), tag);
        }
    }

    #[test]
    fn a_dimension_never_takes_a_value_from_an_argument() {
        // The authoring half of §7's refusal: there is no `value` argument,
        // so a literal cannot enter a document through this door. Since D4c
        // it is refused BY NAME rather than ignored — an agent that sent a
        // number and heard nothing believes that number is on the paper —
        // and the refusal names `expr`, which is the thing that does work.
        let err = annotation_arg(&json!({
            "annotation": "Dimension",
            "kind": "Radius",
            "anchors": [7],
            "value": 0.123,
        }))
        .expect_err("a typed-in value is refused");
        assert_eq!(err.code, "InvalidArgument");
        assert!(err.message.contains("expr"), "{}", err.message);

        // With no `value` and no `expr`, the dimension measures its anchors.
        let spec =
            annotation_arg(&json!({ "annotation": "Dimension", "kind": "Radius", "anchors": [7] }))
                .unwrap();
        let built = crate::drawing_view::build_annotation(&spec).unwrap();
        let waffle_types::annotation::Annotation::Dimension { value, .. } = built else {
            panic!("not a dimension");
        };
        assert_eq!(value, waffle_types::annotation::Measured::FromGeometry);

        // `expr` builds a `Measured::Expr` — D4c's authoring half of D2's
        // evaluator. An empty or whitespace `expr` is NOT one: it would be an
        // expression nothing can evaluate, where measuring the anchors is
        // what the caller plainly meant.
        let spec = annotation_arg(&json!({
            "annotation": "Dimension",
            "kind": "Radius",
            "anchors": [7],
            "expr": " radius(rim) / 2 ",
        }))
        .unwrap();
        let waffle_types::annotation::Annotation::Dimension { value, .. } =
            crate::drawing_view::build_annotation(&spec).unwrap()
        else {
            panic!("not a dimension");
        };
        assert_eq!(
            value,
            waffle_types::annotation::Measured::Expr {
                expr: "radius(rim) / 2".to_string()
            }
        );
        let spec = annotation_arg(&json!({
            "annotation": "Dimension",
            "kind": "Radius",
            "anchors": [7],
            "expr": "   ",
        }))
        .unwrap();
        assert!(spec.expr.is_none());
    }

    #[test]
    fn a_persistent_id_is_accepted_as_a_string_because_json_numbers_lose_it() {
        // The measured defect: a pid is a `u64` and a JSON number in
        // JavaScript is an `f64`, so `2216071694111992607` arrives as
        // …992000 and the dimension anchored on it refuses as "resolves to no
        // geometry". Every caller that goes through the page sends the
        // decimal string, and it must parse EXACTLY.
        let big: u64 = 2216071694111992607;
        assert!(big > (1u64 << 53), "the fixture must exceed f64's integers");
        let spec = annotation_arg(&json!({
            "annotation": "CentreMark",
            "anchors": [{ "pid": big.to_string(), "kind": "Face" }],
        }))
        .unwrap();
        assert_eq!(spec.anchors[0].pid, big);
        assert_eq!(spec.anchors[0].kind, TopoKind::Face);
        // A number is still accepted — it is exact below 2^53 and a
        // hand-written argument should not have to be quoted.
        let spec = annotation_arg(&json!({ "annotation": "CentreMark", "anchors": [9] })).unwrap();
        assert_eq!(spec.anchors[0].pid, 9);
        assert_eq!(spec.anchors[0].kind, TopoKind::Edge);
        // Anything that is not an id is refused by name.
        let err = annotation_arg(&json!({ "annotation": "CentreMark", "anchors": ["face-3"] }))
            .unwrap_err();
        assert_eq!(err.details["path"], "/anchors");
    }

    #[test]
    fn a_view_anchor_crosses_the_wire_as_a_string() {
        // The other half of the same rule: what the engine SENDS must be a
        // string too, or the app reads a rounded id back.
        let anchor = feature_engine::drawing::ViewAnchor {
            pid: 2216071694111992607,
            kind: TopoKind::Edge,
            shape: feature_engine::drawing::AnchorShape::Line,
            at: Some([0.0, 0.0]),
            radius: None,
        };
        let json = serde_json::to_value(&anchor).unwrap();
        assert_eq!(json["pid"], json!("2216071694111992607"));
        let back: feature_engine::drawing::ViewAnchor = serde_json::from_value(json).unwrap();
        assert_eq!(back.pid, anchor.pid);
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
