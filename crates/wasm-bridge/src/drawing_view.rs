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

use std::collections::HashMap;

use feature_engine::assembly::{AssemblyTree, PartBuild, PartRef};
use feature_engine::drawing::{
    auto_placement_mm, dimension_kind_from_tag, rebuild_view, Drawing, DrawingError, DrawingView,
    ExprDimensions, Projection, Sheet, ViewAnchor, ViewSource, DEFAULT_VIEW_GAP_MM,
};
use feature_engine::types::FeatureTree;
use feature_engine::Engine;
use modeling_ops::KernelBundle;
use uuid::Uuid;
use waffle_types::annotation::layout::ViewLayout;
use waffle_types::annotation::{Annotation, Measured, Placement2};
use waffle_types::geom_ref::{Anchor, GeomRef, OutputKey, ResolvePolicy, Selector};
use waffle_types::kernel::projection::{ProjectionBody, ProjectionDeclines};

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
            if let Projection::ProjectedFrom { parent, .. } = projection {
                if drawing.sheets[index].view(*parent).is_none() {
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
            view.placement_mm = placement_mm
                .unwrap_or_else(|| default_placement(&drawing.sheets[index], projection));
            let id = view.id;
            drawing.sheets[index].views.push(view);
            Ok(id)
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
            sheet.views.retain(|v| match &v.projection {
                Projection::ProjectedFrom { parent, .. } => parent != view_id,
                _ => true,
            });
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
    }
}

/// The document annotation one `DrawingAnnotationSpec` means.
///
/// Note what it cannot build: a dimension with a `value`. The spec carries no
/// such field, so a literal number is not expressible at this boundary at all
/// — which is the authoring half of §7's refusal (the rebuild refuses one
/// that arrives another way, `drawing::check_measured`).
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
                value: Measured::FromGeometry,
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
        Projection::ProjectedFrom { direction, .. } => format!("{direction:?} of parent"),
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
fn default_placement(sheet: &Sheet, projection: &Projection) -> [f64; 2] {
    let extent = sheet.extent_mm();
    let centre = [extent[0] / 2.0, extent[1] / 2.0];
    let Projection::ProjectedFrom { parent, direction } = projection else {
        return centre;
    };
    let Some(parent_view) = sheet.view(*parent) else {
        return centre;
    };
    let parent_extent = match parent_view.cache.as_ref().and_then(|c| c.bbox) {
        Some([min, max]) => [
            (max[0] - min[0]) * 1000.0 * parent_view.scale,
            (max[1] - min[1]) * 1000.0 * parent_view.scale,
        ],
        None => [0.0, 0.0],
    };
    // The child's own extent is unknown until it is projected; a projected
    // view of the same part matches its parent in one axis by construction,
    // so the parent's is the best estimate available.
    auto_placement_mm(
        parent_view.placement_mm,
        parent_extent,
        parent_extent,
        *direction,
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
}

/// Rebuild every view of `drawing`.
///
/// The document-level inputs a drawing evaluation reads, bundled.
///
/// Four borrows that always travel together and never change during a pass:
/// the tabs it can draw, the source store, and (P2) the document parameter
/// table every part resolves through. One struct rather than four parameters
/// because they were threaded through three call levels and the next addition
/// would have made a seven-argument function an eight-argument one at each.
#[derive(Clone, Copy)]
struct DocumentInputs<'a> {
    part_trees: &'a HashMap<String, FeatureTree>,
    assembly_trees: &'a HashMap<String, AssemblyTree>,
    sources: &'a feature_engine::sources::SourceStore,
    document_parameters: &'a [feature_engine::types::DesignParameter],
}

/// `reuse` is the part-engine pool (`EngineState::take_part_engines`);
/// whatever is left in it afterwards the caller parks. Errors never abort the
/// pass: a sheet of eight views reports the one that failed and draws the
/// other seven, because a drawing is useful incomplete and useless absent.
pub fn evaluate(
    drawing: &Drawing,
    part_trees: &HashMap<String, FeatureTree>,
    assembly_trees: &HashMap<String, AssemblyTree>,
    sources: &feature_engine::sources::SourceStore,
    document_parameters: &[feature_engine::types::DesignParameter],
    kb: &mut dyn KernelBundle,
    reuse: &mut Vec<(PartBuild, Engine)>,
) -> DrawingEval {
    let doc = DocumentInputs {
        part_trees,
        assembly_trees,
        sources,
        document_parameters,
    };
    let mut out = DrawingEval::default();
    out.warnings.extend(drawing.validate());

    // Bodies per source tab, built at most once even when six views draw the
    // same part — which is the ordinary case for a six-view layout.
    let mut by_tab: HashMap<String, Vec<ProjectionBody>> = HashMap::new();

    for sheet in &drawing.sheets {
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
            // D2: a `Measured::Expr` dimension is evaluated against the
            // SOURCE TAB's expression environment — its parameter table and
            // its own built geometry — which `bodies_of_tab` has just put in
            // `out.parts`. The alternative, the drawing tab's own
            // environment, would measure a different document than the one
            // the view draws.
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
                rebuild_view(
                    view,
                    &frame,
                    &chosen,
                    kernel,
                    exprs.as_ref().map(|x| x as &dyn ExprDimensions),
                )
            };
            match built {
                Ok(built) => {
                    out.declines.merge(&built.declines);
                    for (index, e) in &built.annotation_errors {
                        out.errors.push(describe(view.name.as_str(), e));
                        out.annotation_errors.push((view.id, *index));
                    }
                    out.layouts.insert(view.id, built.layout);
                    out.geometry.insert(view.id, built.geometry);
                    out.anchors.insert(view.id, built.anchors);
                }
                Err(e) => out.errors.push(describe(view.name.as_str(), &e)),
            }
        }
    }
    out
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
