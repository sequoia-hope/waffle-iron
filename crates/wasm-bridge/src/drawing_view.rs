//! Evaluating a `Drawing` tab (`specs/drawings_and_mbd.md` §8, D4a).
//!
//! A drawing's views name OTHER tabs. Evaluating one therefore means building
//! those tabs' bodies and then projecting them — the same two-step
//! [`crate::assembly_view`] does for an assembly's instances, and it reuses
//! the same part engines through the same pool, so switching to a drawing tab
//! and back does not rebuild a part that did not change.
//!
//! ```text
//! Drawing → per view: source tab → bodies (part engine, or an assembly's
//!           placed leaves) → ViewFrame → feature_engine::drawing::rebuild_view
//!         → ViewLayout per view
//! ```
//!
//! Nothing here renders and nothing here measures: the measuring is
//! `feature_engine::drawing`'s and the drawing is the app's. What this module
//! owns is *which bodies*, which is the only question the document model
//! cannot answer on its own.

use std::collections::{HashMap, HashSet};

use feature_engine::assembly::{AssemblyTree, PartBuild, PartRef};
use feature_engine::drawing::{
    auto_placement_step_mm, body_pid_digest, cap_loops_in_view, dimension_kind_from_tag,
    rebuild_view_in, section_frame, section_paper_step, section_plane, title_block_layout,
    view_cache_key, CacheInputs, Drawing, DrawingError, DrawingView, ExprDimensions, Projection,
    ProjectionAngle, Sheet, TitleBlockContext, TitleBlockLayout, ViewAnchor, ViewExtras,
    ViewSource, DEFAULT_VIEW_GAP_MM,
};
use feature_engine::types::FeatureTree;
use feature_engine::Engine;
use modeling_ops::KernelBundle;
use uuid::Uuid;
use waffle_types::annotation::layout::{ClipCircle, HatchLoop, ViewLayout};
use waffle_types::annotation::{Annotation, Measured, Placement2};
use waffle_types::geom_ref::{Anchor, GeomRef, OutputKey, ResolvePolicy, Selector};
use waffle_types::kernel::projection::{ProjectionBody, ProjectionDeclines, ViewBasis};

/// The open drawing tab's last evaluation, minus the drawing itself.
///
/// The drawing LIVES in the session (its views' caches were written there by
/// the evaluation), so keeping a second copy here would be two sources of
/// truth for the same sheets. What this holds is only what the evaluation
/// learned and the document does not record: the declines and the problems.
#[derive(Debug, Clone, Default)]
pub struct OpenDrawing {
    pub tab_id: String,
    /// The projection's declines, by name, non-zero ones only (D1c).
    pub declines: std::collections::BTreeMap<String, u32>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// `(view, annotation index)` of every annotation that did not resolve.
    pub annotation_errors: Vec<(Uuid, usize)>,
    /// Per view, the anchors an annotation may be authored on.
    pub anchors: std::collections::BTreeMap<Uuid, Vec<ViewAnchor>>,
}

/// What evaluating a drawing tab produced.
#[derive(Default)]
pub struct DrawingEval {
    /// The layout of every view that rebuilt, by view id. A view that did
    /// NOT rebuild is absent rather than stale — see
    /// `DocumentSession::set_drawing_caches`.
    pub layouts: HashMap<Uuid, ViewLayout>,
    /// The same views' curves kernel-side, for the DXF export: the writer
    /// takes analytic `Curve2`s, so an arc reaches the file as an `ARC`
    /// rather than as the chord polyline a round trip through the layout
    /// record would make of it. Not persisted and not sent to the app.
    pub geometry: HashMap<Uuid, waffle_types::kernel::projection::ViewGeometry>,
    /// Per view, the entities it drew with the persistent ids an annotation
    /// anchors on — the authoring half of the layout record's deliberate
    /// model-blindness (`ViewRebuild::anchors`). Sent to the app, not
    /// persisted: the document stores the annotations, not the ids available
    /// to make one from.
    pub anchors: std::collections::BTreeMap<Uuid, Vec<ViewAnchor>>,
    /// The sheet-wide total of what every view's projection declined to
    /// decide (D1c). Carried for the same reason the DXF export carries it:
    /// the counts are what tell a decided drawing from a quiet one.
    pub declines: ProjectionDeclines,
    /// Per-view failures, each naming its view.
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// Which `(view, annotation index)` pairs failed — the same failures
    /// `errors` describes in prose, in the form a caller can MATCH on.
    ///
    /// What needs it: `drawing_annotation_add` has to know whether the
    /// annotation IT just added is the one that failed, so it can roll back
    /// rather than leave an unmeasurable annotation in the document. Reading
    /// that out of an error string would be a parser of our own prose.
    pub annotation_errors: Vec<(Uuid, usize)>,
    /// Per view, the validity key of the layout this pass produced (D4b) —
    /// written to the document beside the cache so a later reader can tell a
    /// current sheet from a stored one. Absent for a view that did not
    /// rebuild, exactly as `layouts` is.
    pub cache_keys: HashMap<Uuid, String>,
    /// Per sheet, the title block's filled rows (D4b). A derived hint on the
    /// same terms as a view's layout: recomputed every pass, persisted so a
    /// reader with no engine draws the paper complete.
    pub title_blocks: std::collections::BTreeMap<Uuid, TitleBlockLayout>,
    /// The part engines this pass built or reused, to be parked for the next
    /// one (the same contract as `AssemblyView::parts`).
    pub parts: Vec<(PartBuild, Engine)>,
}

/// Apply one targeted edit to `drawing` (D4a), or say why not.
///
/// One place, two callers: the page's `DrawingEdit` message and — through
/// the same vocabulary — the authoring tools. Putting it here rather than in
/// each means the page and an agent cannot make different documents out of
/// the same instruction.
pub fn apply_edit(
    drawing: &mut Drawing,
    edit: &crate::messages::DrawingEdit,
) -> Result<Uuid, String> {
    use crate::messages::DrawingEdit as E;
    match edit {
        E::AddView {
            sheet_id,
            source_tab,
            bodies,
            projection,
            name,
            scale,
            placement_mm,
        } => {
            if source_tab.is_empty() {
                return Err("a drawing view needs the id of the tab it draws".to_string());
            }
            let index = match sheet_id {
                Some(id) => drawing
                    .sheets
                    .iter()
                    .position(|s| s.id == *id)
                    .ok_or_else(|| format!("this drawing has no sheet {id}"))?,
                None => {
                    if drawing.sheets.is_empty() {
                        return Err("this drawing has no sheets".to_string());
                    }
                    0
                }
            };
            if let Some(parent) = projection.parent() {
                if drawing.sheets[index].view(parent).is_none() {
                    return Err(format!("this sheet has no view {parent} to project from"));
                }
            }
            let scale = scale.unwrap_or(1.0);
            if !(scale.is_finite() && scale > 0.0) {
                return Err(format!(
                    "a view's scale must be a positive ratio (1 is 1:1), not {scale}"
                ));
            }
            let count = drawing.sheets[index].views.len();
            let mut view = DrawingView::new(
                name.clone()
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| default_view_name(projection, count)),
                ViewSource {
                    tab_id: source_tab.clone(),
                    bodies: bodies.clone(),
                },
                projection.clone(),
            );
            view.scale = scale;
            view.placement_mm = placement_mm.unwrap_or_else(|| {
                default_placement(&drawing.sheets[index], projection, drawing.projection_angle)
            });
            let id = view.id;
            drawing.sheets[index].views.push(view);
            Ok(id)
        }
        E::EditSheet {
            sheet_id,
            name,
            size,
            orientation,
            projection_angle,
            title_block_show,
            title_block_fields,
        } => {
            // The projection standard is the DRAWING's, not the sheet's (§8:
            // "a document setting"), and it is set here because this is the
            // sheet-level door — a drawing whose sheets disagreed about which
            // side a projected view shows would be two standards in one
            // document.
            if let Some(angle) = projection_angle {
                drawing.projection_angle = *angle;
            }
            let index = match sheet_id {
                Some(id) => drawing
                    .sheets
                    .iter()
                    .position(|s| s.id == *id)
                    .ok_or_else(|| format!("this drawing has no sheet {id}"))?,
                None if drawing.sheets.is_empty() => {
                    // Nothing but the angle to set, and that is already done.
                    return Ok(Uuid::nil());
                }
                None => 0,
            };
            let sheet = &mut drawing.sheets[index];
            if let Some(name) = name.clone().filter(|n| !n.is_empty()) {
                sheet.name = name;
            }
            if let Some(size) = size {
                sheet.size = *size;
            }
            if let Some(orientation) = orientation {
                sheet.orientation = *orientation;
            }
            if let Some(show) = title_block_show {
                sheet.title_block.show = *show;
            }
            if let Some(fields) = title_block_fields {
                sheet.title_block.fields = fields.clone();
            }
            Ok(sheet.id)
        }
        E::AddSheet {
            name,
            size,
            orientation,
        } => {
            let mut sheet = Sheet::new(
                name.clone()
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| format!("Sheet {}", drawing.sheets.len() + 1)),
            );
            if let Some(size) = size {
                sheet.size = *size;
            }
            if let Some(orientation) = orientation {
                sheet.orientation = *orientation;
            }
            let id = sheet.id;
            drawing.sheets.push(sheet);
            Ok(id)
        }
        E::DeleteSheet { sheet_id } => {
            if drawing.sheets.len() <= 1 {
                // A drawing with no sheet is not a drawing: the panel would
                // show nothing and the exports would refuse by name, which
                // reads as a broken tab rather than as an empty one.
                return Err("a drawing keeps at least one sheet".to_string());
            }
            if !drawing.sheets.iter().any(|s| s.id == *sheet_id) {
                return Err(format!("this drawing has no sheet {sheet_id}"));
            }
            drawing.sheets.retain(|s| s.id != *sheet_id);
            Ok(*sheet_id)
        }
        E::EditView {
            view_id,
            name,
            scale,
            placement_mm,
            bodies,
            hidden_lines,
            silhouettes,
        } => {
            if let Some(scale) = scale {
                if !(scale.is_finite() && *scale > 0.0) {
                    return Err(format!(
                        "a view's scale must be a positive ratio (1 is 1:1), not {scale}"
                    ));
                }
            }
            let sheet = drawing
                .sheet_of_view_mut(*view_id)
                .ok_or_else(|| format!("this drawing has no view {view_id}"))?;
            let view = sheet
                .view_mut(*view_id)
                .ok_or_else(|| format!("this drawing has no view {view_id}"))?;
            if let Some(name) = name.clone().filter(|n| !n.is_empty()) {
                view.name = name;
            }
            if let Some(scale) = scale {
                view.scale = *scale;
            }
            if let Some(placement) = placement_mm {
                view.placement_mm = *placement;
            }
            if let Some(bodies) = bodies {
                view.source.bodies = bodies.clone();
            }
            if let Some(on) = hidden_lines {
                view.style.hidden_lines = *on;
            }
            if let Some(on) = silhouettes {
                view.style.silhouettes = *on;
            }
            Ok(*view_id)
        }
        E::DeleteView { view_id } => {
            let sheet = drawing
                .sheet_of_view_mut(*view_id)
                .ok_or_else(|| format!("this drawing has no view {view_id}"))?;
            sheet.views.retain(|v| v.id != *view_id);
            // Every view DERIVED from it goes too — a projection, a section,
            // a detail — through the one accessor, so a kind added later
            // cannot be left behind naming a parent the sheet does not have.
            // TRANSITIVELY, to a fixpoint: a detail of a section of the
            // deleted view is just as orphaned as the section, and D4a's
            // single pass left it on the sheet to fail its rebuild forever.
            // Bounded by the view count, since each pass removes at least one
            // view or stops.
            loop {
                let ids: HashSet<Uuid> = sheet.views.iter().map(|v| v.id).collect();
                let before = sheet.views.len();
                sheet
                    .views
                    .retain(|v| v.projection.parent().is_none_or(|p| ids.contains(&p)));
                if sheet.views.len() == before {
                    break;
                }
            }
            Ok(*view_id)
        }
        E::AddAnnotation {
            view_id,
            annotation,
        } => {
            let built = build_annotation(annotation)?;
            let sheet = drawing
                .sheet_of_view_mut(*view_id)
                .ok_or_else(|| format!("this drawing has no view {view_id}"))?;
            let view = sheet
                .view_mut(*view_id)
                .ok_or_else(|| format!("this drawing has no view {view_id}"))?;
            view.annotations.push(built);
            Ok(*view_id)
        }
        E::DeleteAnnotation { view_id, index } => {
            let sheet = drawing
                .sheet_of_view_mut(*view_id)
                .ok_or_else(|| format!("this drawing has no view {view_id}"))?;
            let view = sheet
                .view_mut(*view_id)
                .ok_or_else(|| format!("this drawing has no view {view_id}"))?;
            if *index >= view.annotations.len() {
                return Err(format!(
                    "view {view_id} has {} annotation(s), not an index {index}",
                    view.annotations.len()
                ));
            }
            view.annotations.remove(*index);
            Ok(*view_id)
        }
        E::EditAnnotation {
            view_id,
            index,
            precision,
            clear_precision,
            dual_unit,
            expr,
            text,
            label,
            placement,
        } => {
            let sheet = drawing
                .sheet_of_view_mut(*view_id)
                .ok_or_else(|| format!("this drawing has no view {view_id}"))?;
            let view = sheet
                .view_mut(*view_id)
                .ok_or_else(|| format!("this drawing has no view {view_id}"))?;
            let count = view.annotations.len();
            let annotation = view.annotations.get_mut(*index).ok_or_else(|| {
                format!("view {view_id} has {count} annotation(s), not an index {index}")
            })?;
            edit_annotation_in_place(
                annotation,
                AnnotationChanges {
                    precision: *precision,
                    clear_precision: *clear_precision,
                    dual_unit: dual_unit.as_deref(),
                    expr: expr.as_deref(),
                    text: text.as_deref(),
                    label: label.as_deref(),
                    placement: *placement,
                },
            )?;
            Ok(*view_id)
        }

        E::Batch { edits } => {
            if edits.is_empty() {
                return Err("a batch of drawing edits cannot be empty".to_string());
            }
            if edits.iter().any(|e| matches!(e, E::Batch { .. })) {
                return Err("a batch of drawing edits cannot contain another batch".to_string());
            }
            // Applied to a SCRATCH copy, so a refusal part-way leaves the
            // document untouched: the point of a batch is that one Ctrl+Z
            // undoes it, which requires that one refusal undoes it too.
            let mut scratch = drawing.clone();
            let mut last = None;
            for edit in edits {
                last = Some(apply_edit(&mut scratch, edit)?);
            }
            *drawing = scratch;
            last.ok_or_else(|| "a batch of drawing edits cannot be empty".to_string())
        }
    }
}

/// The fields `DrawingEdit::EditAnnotation` may change, borrowed.
#[derive(Debug, Clone, Copy, Default)]
pub struct AnnotationChanges<'a> {
    pub precision: Option<u8>,
    pub clear_precision: bool,
    pub dual_unit: Option<&'a str>,
    pub expr: Option<&'a str>,
    pub text: Option<&'a str>,
    pub label: Option<&'a str>,
    pub placement: Option<[f64; 2]>,
}

/// Change `annotation` in place (D4f), or say which given field its kind
/// does not have.
///
/// Every field is matched against the variant, and a field the variant lacks
/// is a refusal that names it — the same rule the derived title-block rows
/// follow: a caller that set `text` on a dimension and heard nothing would
/// believe the dimension now says it. Checked BEFORE anything is written, so
/// a refused edit changes no field at all.
pub fn edit_annotation_in_place(
    annotation: &mut Annotation,
    changes: AnnotationChanges<'_>,
) -> Result<(), String> {
    let kind = feature_engine::drawing::annotation_tag(annotation);
    let refuse =
        |field: &str| -> Result<(), String> { Err(format!("a {kind} has no `{field}` to change")) };
    let is_dimension = matches!(annotation, Annotation::Dimension { .. });
    if !is_dimension {
        if changes.precision.is_some() || changes.clear_precision {
            refuse("precision")?;
        }
        if changes.dual_unit.is_some() {
            refuse("dual_unit")?;
        }
        if changes.expr.is_some() {
            refuse("expr")?;
        }
    }
    if changes.text.is_some() && !matches!(annotation, Annotation::Note { .. }) {
        refuse("text")?;
    }
    if changes.label.is_some() && !matches!(annotation, Annotation::Datum { .. }) {
        refuse("label")?;
    }
    if changes.placement.is_some() && annotation.placement().is_none() {
        refuse("placement")?;
    }
    if changes.precision.is_some() && changes.clear_precision {
        return Err("give precision or clear_precision, not both".to_string());
    }
    if changes.text.is_some_and(|t| t.is_empty()) {
        return Err("a note needs text".to_string());
    }
    if changes.label.is_some_and(|l| l.is_empty()) {
        return Err("a datum needs a label".to_string());
    }

    match annotation {
        Annotation::Dimension {
            value,
            precision,
            dual_unit,
            placement,
            ..
        } => {
            if let Some(p) = changes.precision {
                *precision = Some(p);
            }
            if changes.clear_precision {
                *precision = None;
            }
            if let Some(unit) = changes.dual_unit {
                *dual_unit = Some(unit.to_string()).filter(|u| !u.is_empty());
            }
            if let Some(expr) = changes.expr {
                // An expression dimension still anchors — the anchors are
                // where it is drawn — so this only changes what it SAYS,
                // exactly as `build_annotation` reads `spec.expr`.
                *value = match expr.trim() {
                    "" => Measured::FromGeometry,
                    e => Measured::Expr {
                        expr: e.to_string(),
                    },
                };
            }
            if let Some(p) = changes.placement {
                *placement = Placement2::new(p[0], p[1]);
            }
        }
        Annotation::Note {
            text, placement, ..
        } => {
            if let Some(t) = changes.text {
                *text = t.to_string();
            }
            if let Some(p) = changes.placement {
                *placement = Placement2::new(p[0], p[1]);
            }
        }
        Annotation::Datum {
            label, placement, ..
        } => {
            if let Some(l) = changes.label {
                *label = l.to_string();
            }
            if let Some(p) = changes.placement {
                *placement = Placement2::new(p[0], p[1]);
            }
        }
        Annotation::CentreMark { .. } | Annotation::CentreLine { .. } => {
            // Nothing editable on either: every given field was refused
            // above, so reaching here means nothing was given.
        }
    }
    Ok(())
}

/// The document annotation one `DrawingAnnotationSpec` means.
///
/// Note what it cannot build: a dimension with a `value`. The spec carries no
/// such field, so a literal number is not expressible at this boundary at all
/// — which is the authoring half of §7's refusal (the rebuild refuses one
/// that arrives another way, `drawing::check_measured`).
///
/// It CAN build a `Measured::Expr` since D4c, from `spec.expr`. The
/// distinction that makes one authorable and the other not is that an
/// expression is re-evaluated against the model on every rebuild, so it
/// cannot go stale; a literal is a number that was once true.
pub fn build_annotation(
    spec: &crate::messages::DrawingAnnotationSpec,
) -> Result<Annotation, String> {
    let placement = spec
        .placement
        .map(|p| Placement2::new(p[0], p[1]))
        .unwrap_or_default();
    let anchors: Vec<GeomRef> = spec
        .anchors
        .iter()
        .map(|a| pid_ref(a.kind, a.pid))
        .collect();
    let need = |n: usize| -> Result<(), String> {
        if anchors.len() == n {
            Ok(())
        } else {
            Err(format!(
                "this annotation anchors on {n} entity(ies), {} given",
                anchors.len()
            ))
        }
    };
    match spec.annotation.as_str() {
        "Dimension" => {
            let tag = spec.kind.as_deref().unwrap_or("Distance");
            let kind = dimension_kind_from_tag(tag)
                .ok_or_else(|| format!("`{tag}` is not a dimension kind"))?;
            need(kind.arity())?;
            Ok(Annotation::Dimension {
                kind,
                anchors,
                // An expression dimension still anchors: the anchors are
                // WHERE it is drawn, and the expression only what it says.
                // That is why the arity check above is unconditional.
                value: match spec
                    .expr
                    .as_deref()
                    .map(str::trim)
                    .filter(|e| !e.is_empty())
                {
                    Some(expr) => Measured::Expr {
                        expr: expr.to_string(),
                    },
                    None => Measured::FromGeometry,
                },
                precision: spec.precision,
                dual_unit: spec.dual_unit.clone().filter(|u| !u.is_empty()),
                placement,
            })
        }
        "Note" => Ok(Annotation::Note {
            text: spec
                .text
                .clone()
                .filter(|t| !t.is_empty())
                .ok_or_else(|| "a note needs text".to_string())?,
            leader: anchors.into_iter().next(),
            placement,
        }),
        "CentreMark" => {
            need(1)?;
            Ok(Annotation::CentreMark {
                anchor: anchors.into_iter().next().expect("checked"),
            })
        }
        "CentreLine" => {
            need(2)?;
            let mut it = anchors.into_iter();
            let a = it.next().expect("checked");
            let b = it.next().expect("checked");
            Ok(Annotation::CentreLine { anchors: [a, b] })
        }
        "Datum" => {
            need(1)?;
            Ok(Annotation::Datum {
                label: spec
                    .label
                    .clone()
                    .filter(|l| !l.is_empty())
                    .unwrap_or_else(|| "A".to_string()),
                anchor: anchors.into_iter().next().expect("checked"),
                placement,
            })
        }
        other => Err(format!("`{other}` is not an annotation kind")),
    }
}

/// An annotation anchor on a persistent id — the only selector a drawing
/// annotation may use (D0 item 4: it never rebinds).
fn pid_ref(kind: waffle_types::TopoKind, pid: u64) -> GeomRef {
    GeomRef {
        kind,
        // Immaterial for a `Selector::Pid`: the pid is resolved against the
        // view's own projection, not through a feature output. Explicit at
        // the nil feature rather than absent.
        anchor: Anchor::FeatureOutput {
            feature_id: Uuid::nil(),
            output_key: OutputKey::Main,
        },
        selector: Selector::Pid { pid, root_pid: pid },
        policy: ResolvePolicy::Strict,
        scope: None,
    }
}

/// A freshly added view's name: the projection's own, numbered when the
/// sheet already carries one like it.
fn default_view_name(projection: &Projection, count: usize) -> String {
    match projection {
        Projection::Named { view } => view.tag().to_string(),
        Projection::Custom { .. } => format!("View {}", count + 1),
        // `direction.label()` rather than the variant's `Debug`: a corner
        // placement is an ISOMETRIC, and `UpRight of parent` names the paper
        // position where a drafter needs to read what the view is (D4e).
        // Unchanged for the four axis placements, whose label IS their name.
        Projection::ProjectedFrom { direction, .. } => {
            format!("{} of parent", direction.label())
        }
        // The standard titles, which is what the sheet prints over the view:
        // a cut is `SECTION A-A` (the letter twice, once per arrow) and a crop
        // is `DETAIL A`. The scale a detail is drawn at is NOT in the name —
        // it is `DrawingView::scale`, and a name carrying it would be a second
        // copy of the number, stale the moment the view is rescaled.
        Projection::Section { label, .. } => format!("SECTION {label}-{label}"),
        Projection::Detail { label, .. } => format!("DETAIL {label}"),
    }
}

/// A view's DRAWN extent in sheet millimetres, from the layout the last
/// evaluation cached — `[0, 0]` for a view that has none yet.
///
/// The layout's `bbox` is in view-plane meters, so the two factors are the
/// unit and the view's own scale. One copy, because the auto-placement reads
/// it for the parent and for every view already on the sheet.
fn drawn_extent_mm(view: &DrawingView) -> [f64; 2] {
    match view.cache.as_ref().and_then(|c| c.bbox) {
        Some([min, max]) => [
            (max[0] - min[0]) * 1000.0 * view.scale,
            (max[1] - min[1]) * 1000.0 * view.scale,
        ],
        None => [0.0, 0.0],
    }
}

/// Where a freshly added view goes when the caller gives no placement: clear
/// of its parent's DRAWN extent for a projected view, the middle of the sheet
/// otherwise.
///
/// It reads the last evaluation's cached layouts rather than guessing,
/// because the gap must be between the DRAWINGS and not between their
/// centres — two views of a long part placed a fixed centre distance apart
/// overlap, which is the mistake D3's dimension layout made once.
fn default_placement(sheet: &Sheet, projection: &Projection, angle: ProjectionAngle) -> [f64; 2] {
    let extent = sheet.extent_mm();
    let centre = [extent[0] / 2.0, extent[1] / 2.0];
    // A view with no parent has nothing to step clear OF, which used to mean
    // the sheet centre every time — so a second named view landed exactly on
    // the first (found by D4d, 2026-10-04). `free_placement_mm` steps clear of
    // everything drawn instead.
    let occupied = || -> Vec<([f64; 2], [f64; 2])> {
        sheet
            .views
            .iter()
            .map(|v| (v.placement_mm, drawn_extent_mm(v)))
            .collect()
    };
    let Some(parent_id) = projection.parent() else {
        return feature_engine::drawing::free_placement_mm(
            extent,
            &occupied(),
            DEFAULT_VIEW_GAP_MM,
        );
    };
    let Some(parent_view) = sheet.view(parent_id) else {
        return feature_engine::drawing::free_placement_mm(
            extent,
            &occupied(),
            DEFAULT_VIEW_GAP_MM,
        );
    };
    let parent_extent = drawn_extent_mm(parent_view);
    // The child's own extent is unknown until it is projected; a projected
    // view of the same part matches its parent in one axis by construction,
    // so the parent's is the best estimate available.
    let step = match projection {
        Projection::ProjectedFrom { direction, .. } => direction.paper_step(),
        Projection::Section { from, to, flip, .. } => {
            // The cut normal, in the parent's paper, through the same rule
            // `projected_frame` reads forwards or backwards — so a section
            // and a projected view cannot end up on opposite sides of the
            // same standard.
            let Some(basis) = sheet
                .view_frame(parent_id, angle)
                .ok()
                .and_then(|f| f.basis())
            else {
                return centre;
            };
            match section_plane(&basis, *from, *to, *flip) {
                Some(cut) => section_paper_step(&cut, angle),
                None => return centre,
            }
        }
        // A detail has no side to be on: it is a magnified crop, placed
        // wherever there is room, and the standard says only that it is
        // labelled. Beside its parent, to the right, so a freshly added one
        // is visible rather than on top of the view it crops.
        Projection::Detail { .. } => [1.0, 0.0],
        _ => return centre,
    };
    auto_placement_step_mm(
        parent_view.placement_mm,
        parent_extent,
        parent_extent,
        step,
        DEFAULT_VIEW_GAP_MM,
    )
}

/// One view's expression environment (D2): the source tab's parameter table
/// plus a measurer over its built geometry.
///
/// Both halves come from the SAME engine, so a dimension reading
/// `plate_w * 2` and one reading `distance(wall_a, wall_b)` are answered
/// from one document — the one the view draws.
struct ViewExprs<'a> {
    env: feature_engine::expr::Env,
    measurer: feature_engine::measure::TreeMeasurer<'a>,
}

impl<'a> ViewExprs<'a> {
    fn new(engine: &'a Engine, kernel: &'a dyn KernelBundle) -> Self {
        Self {
            env: feature_engine::params::cached_env(&engine.tree.parameters),
            measurer: feature_engine::measure::TreeMeasurer::new(
                &engine.tree,
                &engine.feature_results,
                kernel.as_introspect(),
                kernel.as_measure(),
                &engine.pid_to_feature,
            ),
        }
    }
}

impl ExprDimensions for ViewExprs<'_> {
    fn value_of(
        &self,
        expression: &str,
        kind: waffle_types::annotation::DimensionKind,
    ) -> Result<f64, String> {
        use waffle_types::annotation::DimensionKind;
        let q = feature_engine::expr::evaluate_measured(expression, &self.env, &self.measurer)
            .map_err(|e| e.to_string())?;
        // The unit the layout carries, which is the unit
        // `annotation::measure` produces for the same field: METERS for
        // every length kind, RADIANS for an angle. Accepted at the typed
        // boundary, so an angle expression in a length dimension is refused
        // by name rather than read as millimetres.
        //
        // No ordering floor is set: a drawing annotation has no position in
        // the source tab's feature tree, so there is nothing for it to be
        // circular with respect to — it reads geometry that is already
        // built, and it drives none of it.
        match kind {
            DimensionKind::Angle => q.as_angle_radians(),
            _ => q.as_length_meters(),
        }
        .map_err(|e| e.to_string())
    }

    fn text_of(&self, expression: &str) -> Result<String, String> {
        // No typed boundary: a title-block row is TEXT, so the dimension is
        // PRINTED rather than accepted — `area(top)` is a length² that no
        // field takes and that a title block prints perfectly well. The unit
        // is the working space's (mm, degrees), which is the space the
        // expression was written in.
        Ok(
            feature_engine::expr::evaluate_measured(expression, &self.env, &self.measurer)
                .map_err(|e| e.to_string())?
                .display_text(),
        )
    }
}

/// Rebuild every view of `drawing`.
///
/// The document-level inputs a drawing evaluation reads, bundled.
///
/// Four borrows that always travel together and never change during a pass:
/// the tabs it can draw, the source store, and (P2) the document parameter
/// table every part resolves through, plus the document's NAME for the title
/// block. One struct rather than five parameters because they were threaded
/// through three call levels and the next addition would have made a
/// seven-argument function an eight-argument one at each — which is exactly
/// what D4b's `document_name` did when the two branches met, so it joins the
/// struct rather than widening the signature again.
#[derive(Clone, Copy)]
pub struct DocumentInputs<'a> {
    pub name: &'a str,
    pub part_trees: &'a HashMap<String, FeatureTree>,
    pub assembly_trees: &'a HashMap<String, AssemblyTree>,
    pub sources: &'a feature_engine::sources::SourceStore,
    pub document_parameters: &'a [feature_engine::types::DesignParameter],
}

/// `reuse` is the part-engine pool (`EngineState::take_part_engines`);
/// whatever is left in it afterwards the caller parks. Errors never abort the
/// pass: a sheet of eight views reports the one that failed and draws the
/// other seven, because a drawing is useful incomplete and useless absent.
pub fn evaluate(
    drawing: &Drawing,
    doc: DocumentInputs<'_>,
    kb: &mut dyn KernelBundle,
    reuse: &mut Vec<(PartBuild, Engine)>,
) -> DrawingEval {
    let mut out = DrawingEval::default();
    out.warnings.extend(drawing.validate());

    // Bodies per source tab, built at most once even when six views draw the
    // same part — which is the ordinary case for a six-view layout.
    let mut by_tab: HashMap<String, Vec<ProjectionBody>> = HashMap::new();
    // The recipe digest of each source tab, for the cache key (D4b). Beside
    // the bodies because it is the same question asked of the same tab.
    let mut recipe_of_tab: HashMap<String, String> = HashMap::new();

    let sheet_count = drawing.sheets.len();
    for (sheet_index, sheet) in drawing.sheets.iter().enumerate() {
        let sheet_inputs = CacheInputs::for_sheet(sheet, drawing.projection_angle);
        for view in &sheet.views {
            let frame = match sheet.view_frame(view.id, drawing.projection_angle) {
                Ok(frame) => frame,
                Err(e) => {
                    out.errors.push(describe(view.name.as_str(), &e));
                    continue;
                }
            };
            if !by_tab.contains_key(&view.source.tab_id) {
                let bodies = match bodies_of_tab(&view.source, &doc, kb, reuse, &mut out) {
                    Ok(bodies) => bodies,
                    Err(message) => {
                        out.errors.push(format!("view `{}`: {message}", view.name));
                        continue;
                    }
                };
                recipe_of_tab.insert(
                    view.source.tab_id.clone(),
                    source_recipe(&view.source.tab_id, doc.part_trees, doc.assembly_trees),
                );
                by_tab.insert(view.source.tab_id.clone(), bodies);
            }
            let all = &by_tab[&view.source.tab_id];
            let chosen: Vec<ProjectionBody> = all
                .iter()
                .filter(|b| view.source.includes(&b.name))
                .cloned()
                .collect();
            // A named body that matched nothing is named back. Silently
            // drawing the other bodies would be a view the author believes
            // shows something it does not.
            for wanted in &view.source.bodies {
                if !all.iter().any(|b| &b.name == wanted) {
                    out.warnings.push(format!(
                        "view `{}`: tab `{}` has no body `{wanted}`",
                        view.name, view.source.tab_id
                    ));
                }
            }
            // What this view draws besides its own projection (D4b): the
            // marks its children put on it, and — for a section — the cut
            // itself, which REPLACES the bodies projected.
            let mut extras = ViewExtras {
                marks: sheet.marks_on(view.id),
                ..ViewExtras::default()
            };
            let mut bodies = chosen;
            if let Projection::Section { .. } = &view.projection {
                match cut_bodies(view, sheet, drawing, &bodies, kb) {
                    Ok(cut) => {
                        out.warnings.extend(
                            cut.warnings
                                .iter()
                                .map(|w| format!("view `{}`: {w}", view.name)),
                        );
                        extras.hatch = cut.hatch;
                        bodies = cut.bodies;
                    }
                    Err(message) => {
                        // A section the kernel refused is reported and the
                        // view draws NOTHING, rather than quietly drawing the
                        // uncut part: a drawing labelled SECTION A-A that
                        // shows the outside of the solid is a wrong drawing,
                        // where a missing view is a visible gap.
                        out.errors.push(format!("view `{}`: {message}", view.name));
                        continue;
                    }
                }
            }
            if let Projection::Detail { center, radius, .. } = &view.projection {
                extras.clip = Some(ClipCircle {
                    center: *center,
                    radius: *radius,
                });
            }

            // D2: a `Measured::Expr` dimension is evaluated against the
            // SOURCE TAB's expression environment — its parameter table and
            // its own built geometry — which `bodies_of_tab` has just put in
            // `out.parts`. The alternative, the drawing tab's own
            // environment, would measure a different document than the one
            // the view draws.
            //
            // The bodies are D4b's `bodies` rather than `chosen`: a section
            // view projects the CUT halves, and a dimension on one has to be
            // measured against what the view actually draws.
            //
            // The borrows are kept inside this block so the `match` below
            // can push into `out` again.
            let built = {
                let kernel: &dyn KernelBundle = &*kb;
                let engine = out
                    .parts
                    .iter()
                    .find(|(p, _)| p.part.tab_id == view.source.tab_id)
                    .map(|(_, e)| e);
                let exprs = engine.map(|e| ViewExprs::new(e, kernel));
                rebuild_view_in(
                    view,
                    &frame,
                    &bodies,
                    &extras,
                    kernel,
                    exprs.as_ref().map(|x| x as &dyn ExprDimensions),
                )
            };
            match built {
                Ok(built) => {
                    out.declines.merge(&built.declines);
                    out.warnings.extend(
                        built
                            .warnings
                            .iter()
                            .map(|w| format!("view `{}`: {w}", view.name)),
                    );
                    for (index, e) in &built.annotation_errors {
                        out.errors.push(describe(view.name.as_str(), e));
                        out.annotation_errors.push((view.id, *index));
                    }
                    let inputs = sheet_inputs.clone().with_source(
                        recipe_of_tab
                            .get(&view.source.tab_id)
                            .map(String::as_str)
                            .unwrap_or_default(),
                        &body_pid_digest(&bodies, kb),
                    );
                    out.cache_keys
                        .insert(view.id, view_cache_key(view.id, &inputs));
                    out.layouts.insert(view.id, built.layout);
                    out.geometry.insert(view.id, built.geometry);
                    out.anchors.insert(view.id, built.anchors);
                }
                Err(e) => out.errors.push(describe(view.name.as_str(), &e)),
            }
        }

        // The title block is filled AFTER this sheet's views, which is what
        // D4c's expression rows need: the environment they measure is a
        // SOURCE TAB's engine, and `bodies_of_tab` is what put one in
        // `out.parts`. Filling it first — as D4b did, when no row could
        // measure anything — would have handed every expression row a
        // `None`.
        //
        // **Which document a title-block expression measures: the one tab
        // this sheet's views draw.** A sheet of six views of one part is the
        // ordinary case and `volume(plate)` means something in it. A sheet
        // whose views draw TWO parts has no "the part" whose mass to print,
        // and a sheet with no views has no document at all, so both refuse by
        // name rather than picking the first — the same choice D2 made for an
        // assembly source, which has no single engine either.
        let fill = {
            let kernel: &dyn KernelBundle = &*kb;
            let mut tabs = sheet.views.iter().map(|v| v.source.tab_id.as_str());
            let single = match (tabs.next(), tabs.next()) {
                (Some(first), None) => Some(first),
                (Some(first), Some(_)) => sheet
                    .views
                    .iter()
                    .all(|v| v.source.tab_id == first)
                    .then_some(first),
                _ => None,
            };
            let engine = single.and_then(|tab| {
                out.parts
                    .iter()
                    .find(|(p, _)| p.part.tab_id == tab)
                    .map(|(_, e)| e)
            });
            let exprs = engine.map(|e| ViewExprs::new(e, kernel));
            title_block_layout(
                &sheet.title_block,
                sheet,
                &TitleBlockContext {
                    document_name: doc.name,
                    sheet_number: sheet_index + 1,
                    sheet_count,
                    angle: drawing.projection_angle,
                },
                exprs.as_ref().map(|x| x as &dyn ExprDimensions),
            )
        };
        out.title_blocks.insert(sheet.id, fill.layout);
        out.errors.extend(
            fill.errors
                .iter()
                .map(|e| format!("sheet `{}`: {e}", sheet.name)),
        );
    }
    out
}

/// A section view's own bodies: the HALVES of its source's bodies that the
/// cut keeps, plus the cap regions to hatch (D4b).
struct SectionCut {
    bodies: Vec<ProjectionBody>,
    hatch: Vec<HatchLoop>,
    warnings: Vec<String>,
}

/// Cut `bodies` with the plane the view's cutting line names.
///
/// The cut is the KERNEL's (`section_with_plane`, D1d), one body at a time,
/// and the halves it hands back are what the section view then projects with
/// D1a–c — so a section view is an ordinary view of extraordinary bodies and
/// shares every line of the projection path with the view it was cut on.
///
/// A placed body (an assembly leaf) is cut in its OWN frame: the plane goes
/// back through the placement, because `section_with_plane` has no placement
/// argument and a solid's arena coordinates are its own. The cap then comes
/// back in that local plane's frame and its basis is carried forward through
/// the placement, which is what keeps an assembly section's hatch in the same
/// `(u, v)` as the curves it fills.
fn cut_bodies(
    view: &DrawingView,
    sheet: &Sheet,
    drawing: &Drawing,
    bodies: &[ProjectionBody],
    kb: &mut dyn KernelBundle,
) -> Result<SectionCut, String> {
    let Projection::Section {
        parent,
        from,
        to,
        flip,
        ..
    } = &view.projection
    else {
        return Err("not a section view".to_string());
    };
    let parent_basis = sheet
        .view_frame(*parent, drawing.projection_angle)
        .map_err(|e| e.to_string())?
        .basis()
        .ok_or_else(|| format!("view {parent} has no view basis to cut in"))?;
    let cut = section_plane(&parent_basis, *from, *to, *flip)
        .ok_or_else(|| "the cutting line has no length, so it names no plane".to_string())?;
    let view_basis = section_frame(&parent_basis, &cut)
        .basis()
        .ok_or_else(|| "the cut plane has no view basis".to_string())?;

    let mut out = SectionCut {
        bodies: Vec::new(),
        hatch: Vec::new(),
        warnings: Vec::new(),
    };
    for body in bodies {
        let placement = body.placement;
        let (origin, normal) = match &placement {
            Some(p) => (p.inverse_apply(cut.origin), p.inverse_dir(cut.normal)),
            None => (cut.origin, cut.normal),
        };
        let section = kb
            .section_with_plane(&body.handle, origin, normal)
            .map_err(|e| format!("body `{}` could not be sectioned: {e}", body.name))?;
        let Some(handle) = section.cut_solid else {
            // Typed, not an error: the plane keeps none of this body. A
            // multi-body part sectioned at one end legitimately drops the
            // bodies at the other, and saying so is how a reader tells that
            // from a body that failed.
            out.warnings.push(format!(
                "the cut keeps no part of body `{}`, so it is not drawn",
                body.name
            ));
            continue;
        };
        out.bodies.push(ProjectionBody {
            handle,
            name: body.name.clone(),
            placement,
        });
        // The cap's own frame, in WORLD coordinates: for a placed body the
        // kernel answered in the body's frame.
        let cap_basis = match &placement {
            Some(p) => ViewBasis {
                origin: p.apply(section.plane_basis.origin),
                u: p.apply_dir(section.plane_basis.u),
                v: p.apply_dir(section.plane_basis.v),
                w: p.apply_dir(section.plane_basis.w),
            },
            None => section.plane_basis,
        };
        let (mut loops, dropped) = cap_loops_in_view(&section.cap_loops, &cap_basis, &view_basis);
        if dropped > 0 {
            // A region missing one of its boundary curves is not a region,
            // and hatching it would run the lines out through the gap.
            out.warnings.push(format!(
                "{dropped} cap loop(s) of body `{}` could not be placed in the view and are not hatched",
                body.name
            ));
        }
        if section.cap_loops.iter().any(|l| !l.exact) {
            out.warnings.push(format!(
                "body `{}` has a sampled cap edge, so its hatch boundary is a chord polygon",
                body.name
            ));
        }
        if section.cap_shared_with_model {
            // The §4.5.5 Stage-0 signature: the cut plane is coplanar with a
            // model face. A legitimate section, and worth saying, because it
            // is the one configuration where the cap was found by its plane
            // rather than by its descent from the cutting half-space.
            out.warnings.push(format!(
                "body `{}` was cut along one of its own faces (the cap is the shared surface)",
                body.name
            ));
        }
        out.hatch.append(&mut loops);
    }
    if out.bodies.is_empty() {
        return Err("the cut keeps no material at all, so there is nothing to draw".to_string());
    }
    Ok(out)
}

/// The recipe a source tab builds its bodies from, for the cache key — the
/// tab's tree in its document form, CANONICALIZED.
///
/// ## Why it goes through `Value` rather than straight to a string
///
/// `serde_json::to_string(tree)` is not a stable digest input. A
/// `FeatureTree` holds `HashMap`s — `Sketch::solved_positions` is one — and a
/// `HashMap` serializes in its own iteration order, which is seeded per
/// process and per insertion history. Measured: two rebuilds of the SAME
/// unedited box in one process produced two 1851-byte strings differing only
/// in the order of `solved_positions`' four keys, so the key moved on every
/// rebuild — a cache that is always stale, which is indistinguishable from
/// having no key at all and was exactly the bug this function's first version
/// shipped with.
///
/// `serde_json::Value`'s object is a `BTreeMap` (this workspace does not
/// enable serde_json's `preserve_order`), so converting to a `Value` first
/// sorts every key at every depth and the string that comes out is canonical.
/// `build_part`'s own reuse check compares `to_value` for the same reason,
/// which is why IT was never wrong about whether a tree had changed.
///
/// An ASSEMBLY tab contributes its own tree AND the trees of the part tabs it
/// instantiates: a drawing of an assembly changes when any of its parts does,
/// and a digest over the assembly alone would read as valid after one of them
/// was rebuilt. The part tabs are taken in sorted id order, for the same
/// stability reason.
fn source_recipe(
    tab_id: &str,
    part_trees: &HashMap<String, FeatureTree>,
    assembly_trees: &HashMap<String, AssemblyTree>,
) -> String {
    let canonical = |value: Option<serde_json::Value>| -> String {
        value
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok())
            .unwrap_or_default()
    };
    if let Some(tree) = part_trees.get(tab_id) {
        return canonical(serde_json::to_value(tree).ok());
    }
    if let Some(tree) = assembly_trees.get(tab_id) {
        let mut parts: Vec<&String> = part_trees.keys().collect();
        parts.sort();
        let mut acc = canonical(serde_json::to_value(tree).ok());
        for id in parts {
            acc.push('|');
            acc.push_str(id);
            acc.push('=');
            acc.push_str(&canonical(
                part_trees
                    .get(id)
                    .and_then(|t| serde_json::to_value(t).ok()),
            ));
        }
        return acc;
    }
    String::new()
}

impl DrawingEval {
    /// This evaluation as the state the open tab keeps.
    pub fn open(&self, tab_id: &str) -> OpenDrawing {
        OpenDrawing {
            tab_id: tab_id.to_string(),
            declines: self
                .declines
                .counts()
                .into_iter()
                .filter(|(_, n)| *n > 0)
                .map(|(name, n)| (name.to_string(), n))
                .collect(),
            errors: self.errors.clone(),
            warnings: self.warnings.clone(),
            annotation_errors: self.annotation_errors.clone(),
            anchors: self.anchors.clone(),
        }
    }
}

/// A view's error with the view's NAME in front of it. The typed error
/// already carries the view's uuid; a person reading a toast needs the name.
fn describe(view_name: &str, e: &DrawingError) -> String {
    format!("view `{view_name}`: {e}")
}

/// Every projectable body of the tab `source` names.
fn bodies_of_tab(
    source: &ViewSource,
    doc: &DocumentInputs<'_>,
    kb: &mut dyn KernelBundle,
    reuse: &mut Vec<(PartBuild, Engine)>,
    out: &mut DrawingEval,
) -> Result<Vec<ProjectionBody>, String> {
    let DocumentInputs {
        part_trees,
        assembly_trees,
        sources,
        document_parameters,
        ..
    } = *doc;
    let part = PartRef {
        source_id: None,
        tab_id: source.tab_id.clone(),
    };
    if let Some(tree) = part_trees.get(&source.tab_id) {
        let engine = build_part(part.clone(), tree, sources, document_parameters, kb, reuse);
        for (fid, msg) in &engine.errors {
            let name = engine
                .tree
                .features
                .iter()
                .find(|f| f.id == *fid)
                .map(|f| f.name.as_str())
                .unwrap_or("?");
            out.errors
                .push(format!("tab `{}` feature `{name}`: {msg}", source.tab_id));
        }
        let (bodies, warnings) =
            crate::dispatch::projection_bodies(&engine, kb.as_introspect(), "", None);
        out.warnings.extend(warnings);
        out.parts.push((PartBuild::plain(part), engine));
        return Ok(bodies);
    }
    if let Some(tree) = assembly_trees.get(&source.tab_id) {
        // An assembly source is evaluated by the assembly's own machinery —
        // the placements are a solve, not a field — and its leaves are then
        // projected at their world poses, which is what
        // `project_bodies` takes. Reusing `assembly_view::evaluate` rather
        // than re-deriving the poses is what keeps a drawing of an assembly
        // agreeing with the assembly tab beside it.
        let view = crate::assembly_view::evaluate(
            tree.clone(),
            part_trees,
            assembly_trees,
            sources,
            document_parameters,
            kb,
            reuse,
        );
        out.errors.extend(
            view.errors
                .iter()
                .map(|e| format!("tab `{}`: {e}", source.tab_id)),
        );
        out.warnings.extend(
            view.warnings
                .iter()
                .map(|w| format!("tab `{}`: {w}", source.tab_id)),
        );
        let mut bodies = Vec::new();
        for leaf in &view.leaves {
            let (_, engine) = &view.parts[leaf.part];
            let prefix = view.leaf_name(&leaf.path);
            let placement = crate::dispatch::rigid_placement_of(&leaf.transform);
            let (mut got, warnings) = crate::dispatch::projection_bodies(
                engine,
                kb.as_introspect(),
                &prefix,
                Some(placement),
            );
            out.warnings.extend(warnings);
            bodies.append(&mut got);
        }
        out.parts.extend(view.parts);
        return Ok(bodies);
    }
    Err(format!(
        "tab `{}` is not a Part or Assembly tab of this document",
        source.tab_id
    ))
}

/// A part engine for `tree`: the pooled one when it is a build of exactly
/// this tree, a fresh rebuild otherwise.
///
/// Trees are compared by their document form, as `assembly_view` does and for
/// the reason stated there: `FeatureTree` carries no equality of its own, and
/// serializing one costs nothing next to rebuilding it.
fn build_part(
    part: PartRef,
    tree: &FeatureTree,
    sources: &feature_engine::sources::SourceStore,
    document_parameters: &[feature_engine::types::DesignParameter],
    kb: &mut dyn KernelBundle,
    reuse: &mut Vec<(PartBuild, Engine)>,
) -> Engine {
    // A view of a PART tab draws the part as its own tab defines it, so this
    // is always the default build: a drawing view names a tab, never an
    // instance, and has no overrides to apply. (A view of an ASSEMBLY tab
    // goes through `assembly_view::evaluate` above, which does.)
    let build = PartBuild::plain(part);
    let wanted = serde_json::to_value(tree).ok();
    let doc_sig = feature_engine::params::table_signature(document_parameters);
    let cached = wanted.and_then(|wanted| {
        reuse.iter().position(|(p, e)| {
            *p == build
                && feature_engine::params::table_signature(&e.document_parameters) == doc_sig
                && serde_json::to_value(&e.tree).ok() == Some(wanted.clone())
        })
    });
    match cached {
        Some(i) => {
            let (_, mut engine) = reuse.swap_remove(i);
            engine.sources = sources.clone();
            engine
        }
        None => {
            let mut engine = Engine::new();
            engine.tree = tree.clone();
            engine.sources = sources.clone();
            engine.document_parameters = document_parameters.to_vec();
            engine.rebuild_from_scratch(kb);
            engine
        }
    }
}

// --------------------------------------------- D4e: probing a view not added

/// What one hypothetical view would be: the answer
/// [`crate::messages::EngineToUi::DrawingViewProbed`] carries.
pub struct ProbedViews {
    /// The source bodies' world AABB in meters, `[min, max]`.
    pub bounds: Option<[[f64; 3]; 2]>,
    pub views: Vec<crate::messages::ProbedDrawingView>,
    pub warnings: Vec<String>,
    /// The part engines the probe built or took from the pool, on the same
    /// contract as `DrawingEval::parts`: the caller parks them, so a second
    /// probe of the same source (the next hover) builds nothing.
    pub parts: Vec<(PartBuild, Engine)>,
}

/// Answer `projections` for a view of `source` on `sheet`, WITHOUT adding
/// anything (D4e, `specs/drawings_and_mbd.md` §8).
///
/// Three things per projection, and each is the same expression the add path
/// uses rather than a second copy of it:
///
/// - the placement, from [`default_placement`] — which is exactly what an
///   `AddView` with no `placement_mm` calls, so a tool that probes and then
///   adds with the answer produces the view the panel would have produced;
/// - the frame, from [`Sheet::view_frame`] over a TEMPORARY view pushed onto a
///   clone of the sheet. A clone rather than a second frame derivation: a
///   `ProjectedFrom` chain, a section's cut and a detail's inheritance are all
///   already in that one function, and re-deriving any of them here is how a
///   ghost comes to disagree with the view it previews;
/// - what a projected view SHOWS, from `shown_side` — the first-angle flip,
///   answered rather than re-implemented in the UI.
///
/// The bounds are the source's, once, because a UI sizes its ghost from them
/// and they do not depend on the projection.
pub fn probe_views(
    drawing: &Drawing,
    sheet_id: Option<Uuid>,
    source: &ViewSource,
    projections: &[Projection],
    doc: DocumentInputs<'_>,
    kb: &mut dyn KernelBundle,
    reuse: &mut Vec<(PartBuild, Engine)>,
) -> Result<ProbedViews, String> {
    let sheet = match sheet_id {
        Some(id) => drawing
            .sheets
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| format!("this drawing has no sheet {id}"))?,
        None => drawing
            .sheets
            .first()
            .ok_or_else(|| "this drawing has no sheets".to_string())?,
    };
    let angle = drawing.projection_angle;

    // The bodies, through the same door the evaluation uses — so a source tab
    // already built for this drawing comes out of the pool rather than being
    // rebuilt for a hover.
    let mut out = DrawingEval::default();
    let bodies = bodies_of_tab(source, &doc, kb, reuse, &mut out)?;
    let bounds = bodies_bounds(&bodies, kb);
    let mut warnings = std::mem::take(&mut out.warnings);
    if bounds.is_none() && !bodies.is_empty() {
        warnings.push(format!(
            "tab `{}` has no body the kernel will bound, so a placement ghost has no size",
            source.tab_id
        ));
    }

    let mut views = Vec::with_capacity(projections.len());
    for projection in projections {
        let placement_mm = default_placement(sheet, projection, angle);
        let shows = match projection {
            Projection::ProjectedFrom { direction, .. } => {
                Some(feature_engine::drawing::shown_side(*direction, angle))
            }
            _ => None,
        };
        let name = default_view_name(projection, sheet.views.len());
        // The temporary view: the one call that knows every projection kind.
        let mut probe_sheet = sheet.clone();
        let temp = DrawingView::new("probe", source.clone(), projection.clone());
        let temp_id = temp.id;
        probe_sheet.views.push(temp);
        let (dir, up, error) = match probe_sheet.view_frame(temp_id, angle) {
            Ok(frame) => (frame.dir, frame.up, None),
            // `[0, 0, 0]` rather than a guessed frame: a caller must not be
            // able to draw a ghost from a frame the engine refused, and a
            // zero direction has no basis, which every consumer already
            // handles.
            Err(e) => ([0.0; 3], [0.0; 3], Some(e.to_string())),
        };
        views.push(crate::messages::ProbedDrawingView {
            placement_mm,
            dir,
            up,
            name,
            shows,
            error,
        });
    }
    Ok(ProbedViews {
        bounds,
        views,
        warnings,
        parts: out.parts,
    })
}

/// The world AABB of `bodies` in meters, or `None` when nothing bounds.
///
/// `solid_aabb` first — it is analytic and documented CONSERVATIVE, so it
/// bounds the solid from above, which is the direction a layout ghost must err
/// in. It declines a solid carrying a surface-pair curve (D1a's note), and for
/// those the render tessellation's own vertices are used instead: INSCRIBED,
/// so short of the true extent by the chord deficit, which is the one caveat
/// `ghostExtentMm` records on the app side.
///
/// A placed body (an assembly leaf) is bounded by transforming its box's eight
/// CORNERS and taking their extent — transforming the min and max alone gives
/// a box that is not even a box under a rotation.
fn bodies_bounds(bodies: &[ProjectionBody], kb: &mut dyn KernelBundle) -> Option<[[f64; 3]; 2]> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut any = false;
    for body in bodies {
        let box3 = match kb.as_introspect().solid_aabb(&body.handle) {
            Some((min, max)) => Some((min, max)),
            None => mesh_bounds(body, kb),
        };
        let Some((min, max)) = box3 else { continue };
        if ![min, max].iter().all(|p| p.iter().all(|x| x.is_finite())) {
            continue;
        }
        for i in 0..8 {
            let corner = [
                if i & 1 == 0 { min[0] } else { max[0] },
                if i & 2 == 0 { min[1] } else { max[1] },
                if i & 4 == 0 { min[2] } else { max[2] },
            ];
            let p = match &body.placement {
                Some(place) => {
                    let r = &place.rotation;
                    [
                        r[0][0] * corner[0]
                            + r[0][1] * corner[1]
                            + r[0][2] * corner[2]
                            + place.translation[0],
                        r[1][0] * corner[0]
                            + r[1][1] * corner[1]
                            + r[1][2] * corner[2]
                            + place.translation[1],
                        r[2][0] * corner[0]
                            + r[2][1] * corner[1]
                            + r[2][2] * corner[2]
                            + place.translation[2],
                    ]
                }
                None => corner,
            };
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
            any = true;
        }
    }
    any.then_some([lo, hi])
}

/// A body's render-tessellation bounds, for a solid `solid_aabb` declines.
fn mesh_bounds(body: &ProjectionBody, kb: &mut dyn KernelBundle) -> Option<([f64; 3], [f64; 3])> {
    // 0.1 mm, the chord tolerance `tessellation_runner` renders at: the ghost
    // is then bounded by the same mesh the viewport shows.
    let mesh = kb.tessellate(&body.handle, 0.0001).ok()?;
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut any = false;
    for v in mesh.vertices.chunks_exact(3) {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k] as f64);
            hi[k] = hi[k].max(v[k] as f64);
        }
        any = true;
    }
    any.then_some((lo, hi))
}
