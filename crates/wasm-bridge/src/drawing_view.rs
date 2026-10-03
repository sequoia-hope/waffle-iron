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

use feature_engine::assembly::{AssemblyTree, PartRef};
use feature_engine::drawing::{rebuild_view, Drawing, DrawingError, ViewSource};
use feature_engine::types::FeatureTree;
use feature_engine::Engine;
use modeling_ops::KernelBundle;
use uuid::Uuid;
use waffle_types::annotation::layout::ViewLayout;
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
    pub parts: Vec<(PartRef, Engine)>,
}

/// Rebuild every view of `drawing`.
///
/// `reuse` is the part-engine pool (`EngineState::take_part_engines`);
/// whatever is left in it afterwards the caller parks. Errors never abort the
/// pass: a sheet of eight views reports the one that failed and draws the
/// other seven, because a drawing is useful incomplete and useless absent.
pub fn evaluate(
    drawing: &Drawing,
    part_trees: &HashMap<String, FeatureTree>,
    assembly_trees: &HashMap<String, AssemblyTree>,
    sources: &feature_engine::sources::SourceStore,
    kb: &mut dyn KernelBundle,
    reuse: &mut Vec<(PartRef, Engine)>,
) -> DrawingEval {
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
                let bodies = match bodies_of_tab(
                    &view.source,
                    part_trees,
                    assembly_trees,
                    sources,
                    kb,
                    reuse,
                    &mut out,
                ) {
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
            match rebuild_view(view, &frame, &chosen, kb) {
                Ok(built) => {
                    out.declines.merge(&built.declines);
                    for (index, e) in &built.annotation_errors {
                        out.errors.push(describe(view.name.as_str(), e));
                        out.annotation_errors.push((view.id, *index));
                    }
                    out.layouts.insert(view.id, built.layout);
                    out.geometry.insert(view.id, built.geometry);
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
    part_trees: &HashMap<String, FeatureTree>,
    assembly_trees: &HashMap<String, AssemblyTree>,
    sources: &feature_engine::sources::SourceStore,
    kb: &mut dyn KernelBundle,
    reuse: &mut Vec<(PartRef, Engine)>,
    out: &mut DrawingEval,
) -> Result<Vec<ProjectionBody>, String> {
    let part = PartRef {
        source_id: None,
        tab_id: source.tab_id.clone(),
    };
    if let Some(tree) = part_trees.get(&source.tab_id) {
        let engine = build_part(part.clone(), tree, sources, kb, reuse);
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
        out.parts.push((part, engine));
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
    kb: &mut dyn KernelBundle,
    reuse: &mut Vec<(PartRef, Engine)>,
) -> Engine {
    let wanted = serde_json::to_value(tree).ok();
    let cached = wanted.and_then(|wanted| {
        reuse.iter().position(|(p, e)| {
            *p == part && serde_json::to_value(&e.tree).ok() == Some(wanted.clone())
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
            engine.rebuild_from_scratch(kb);
            engine
        }
    }
}
