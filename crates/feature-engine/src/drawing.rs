//! Drawings (`specs/drawings_and_mbd.md` §8, increment D4a): a `Drawing` tab
//! holds **sheets** of **views** of another tab's bodies, each view a
//! projection direction plus a paper placement, each carrying the
//! [`Annotation`]s drawn on it.
//!
//! This module is the document model and the rebuild. It is the drawing
//! sibling of [`crate::assembly`], and it sits here for the same reason that
//! does: `file-format`'s `TabKind` holds it, so the type has to live below
//! `file-format` and above `waffle-types`, which owns the annotations and the
//! projection contract.
//!
//! ## What a rebuild does
//!
//! ```text
//! view → ViewFrame (named, custom, or derived from a parent view)
//!      → KernelProjection::project_bodies   (D1a–c: curves, visibility)
//!      → pid → projected geometry map       (D0: EntityPid per edge/face/vertex)
//!      → resolve each annotation's anchors, measure it  (D3)
//!      → ViewLayout                          (the record the app draws)
//! ```
//!
//! [`rebuild_view`] is that pipeline. The app never sees a `GeomRef` or a
//! kernel handle: it gets a [`ViewLayout`], which carries the curves and the
//! already-measured numbers, so it has no way to draw a value other than the
//! measured one (the D3 argument, now with a producer).
//!
//! ## Deviations from the §8 sketch, and why
//!
//! - **`annotations` hang off the VIEW, not the tab.** §8 puts
//!   `annotations: Vec<Annotation>` on `TabKind::Drawing`. A dimension is
//!   measured in view-plane `(u, v)` ([`waffle_types::annotation::measure`]),
//!   so an annotation with no view has no coordinate system to be measured in
//!   — a tab-level list would need a view id on every entry anyway, and then
//!   an entry naming a deleted view is a state the type permits. Per view,
//!   that state does not exist.
//! - **`Sheet` is a list, and the tab holds sheets rather than one sheet.**
//!   §8's `sheet: Sheet` admits one sheet per tab; a part with six views and a
//!   detail sheet is the ordinary case, and multiple tabs for one drawing
//!   would split the title block (D4b) from the views it describes.
//! - **`source` is a [`ViewSource`], not a `GeomRef`.** §8 writes
//!   `source: GeomRef // RefScope → part tab or assembly instance`. A
//!   `GeomRef` names ONE entity — it has a `TopoKind` and a selector — and a
//!   view projects a SET of bodies. `ViewSource` names the tab and, optionally,
//!   which of its bodies; that is exactly the argument
//!   [`KernelProjection::project_bodies`] takes.
//! - **`cache` is a [`ViewLayout`], and it is the sheet preview.** §8 has
//!   `cache: Option<ViewGeometry>` per view plus `preview: Option<SheetPreview>`
//!   per tab. `ViewGeometry` has no serde (it is built on
//!   `cad_primitives::Point2`); `ViewLayout` is its persistable form and
//!   carries the annotations too, so one field does both jobs. It is a
//!   **derived hint** on the same terms as `AssemblyTree::placements`:
//!   recomputed on every rebuild, persisted so a reader without a kernel (a
//!   thumbnail, a script, a freshly opened document) can draw the sheet, never
//!   authoritative.
//! - **No `tangent_edges` in [`ViewStyle`].** §8 lists one. The projection
//!   has no tangent-edge classification to switch on:
//!   [`CurveKind`](waffle_types::kernel::projection::CurveKind) is
//!   `Edge | Silhouette | SectionOutline`. A checkbox wired to nothing is
//!   worse than a missing one. `hatch` is D4b's, with sections.
//!
//! ## Persistence
//!
//! A `Drawing` tab does **not** move the format floor. `docs/FILE_FORMAT.md`
//! §13.3: since v4 a new tab kind needs no bump, because a reader that does
//! not know it keeps the whole tab as `TabKind::Unknown` and re-emits it
//! verbatim. That is why the `Selector::Pid` inside an annotation anchor —
//! which DID move the floor to v7 when `FeatureTree.names` carried one — costs
//! nothing here: an old reader never deserializes it. See the D4a
//! implementation notes in the spec.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use modeling_ops::KernelBundle;
use waffle_types::annotation::layout::{
    AnchorGeometry, AnnotationLayout, ClipCircle, HatchLoop, LayoutCurve, ToleranceLayout,
    ViewLayout, ViewMark,
};
use waffle_types::annotation::measure::{measure, MeasureError};
use waffle_types::annotation::{Annotation, DimensionKind, Measured};
use waffle_types::geom_ref::{GeomRef, Selector};
use waffle_types::kernel::projection::{
    Aabb2, CurveKind, ProjectOpts, ProjectionBody, ProjectionDeclines, SectionLoop, ViewBasis,
    ViewFrame, ViewGeometry, Visibility,
};
use waffle_types::kernel::TopoKind;

// ------------------------------------------------------------------- errors

/// Why a drawing view could not be built. Every arm names the view or the
/// annotation it is about, because a sheet of eight views reports these one at
/// a time and "the drawing failed" is not actionable.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DrawingError {
    /// A `ProjectedFrom` view names a parent that is not on the sheet.
    #[error("view {view} is projected from {parent}, which is not on this sheet")]
    UnknownParent { view: Uuid, parent: Uuid },
    /// `ProjectedFrom` chains that come back round to where they started.
    /// Refused rather than followed to a stack overflow.
    #[error("view {view} is projected from itself through a chain of {length} views")]
    ProjectionCycle { view: Uuid, length: usize },
    /// A view frame with no basis: a zero direction, or an `up` parallel to
    /// the line of sight.
    #[error("view {view} has a degenerate frame (dir {dir:?}, up {up:?}): no view basis")]
    DegenerateFrame {
        view: Uuid,
        dir: [f64; 3],
        up: [f64; 3],
    },
    /// A scale that cannot be drawn.
    #[error("view {view} has scale {scale}, which is not a positive finite number")]
    BadScale { view: Uuid, scale: f64 },
    /// A section view whose cutting line has no length, so it names no plane
    /// (D4b). Refused rather than cut at an arbitrary normal.
    #[error("section view {view} has a cutting line of no length ({from:?} to {to:?})")]
    DegenerateCut {
        view: Uuid,
        from: [f64; 2],
        to: [f64; 2],
    },
    /// A detail view whose crop disc is not a disc (D4b).
    #[error("detail view {view} has crop radius {radius}, which is not a positive finite number")]
    BadCropRadius { view: Uuid, radius: f64 },
    /// A dimension carrying a typed-in number.
    ///
    /// `Measured::Value` is documented as a cache or an imported nominal and
    /// is not authorable (D3). This is the boundary that enforces it: a
    /// drawing whose dimension does not come from the model is the exact
    /// failure the whole increment exists to prevent, so it is refused here
    /// rather than drawn.
    #[error(
        "annotation {index} of view {view} carries a literal value ({value}); a drawing \
         dimension is measured from the model, never typed in"
    )]
    LiteralValue {
        view: Uuid,
        index: usize,
        value: f64,
    },
    /// A `Measured::Expr` dimension in a rebuild with no expression
    /// environment to evaluate it against.
    ///
    /// Named rather than silently measured from the anchors' geometry,
    /// because those are different numbers: the whole point of an
    /// expression dimension is that it is NOT what the anchors measure.
    #[error(
        "annotation {index} of view {view} measures the expression `{expr}`, and this rebuild \
         has no expression environment to evaluate it in"
    )]
    ExprNotEvaluated {
        view: Uuid,
        index: usize,
        expr: String,
    },
    /// A `Measured::Expr` dimension whose expression failed (D2): a bad
    /// expression, a vanished entity name, a dimension the kind cannot
    /// take.
    #[error("annotation {index} of view {view}: the expression `{expr}` failed: {reason}")]
    ExprFailed {
        view: Uuid,
        index: usize,
        expr: String,
        reason: String,
    },
    /// A tolerance this annotation cannot carry (M1): an angular band on a
    /// linear dimension, an ISO 286 fit on something that is not a size or
    /// is outside the tables, an inverted band, a feature control frame
    /// malformed by ISO 1101.
    ///
    /// A typed error rather than a dropped tolerance. A sheet that silently
    /// omits the tolerance it was told to print is the same class of defect
    /// as one that prints a value the model disagrees with — the drawing
    /// says less than the author said, and nothing tells them.
    #[error("annotation {index} of view {view}: its tolerance was refused: {reason}")]
    ToleranceRefused {
        view: Uuid,
        index: usize,
        reason: String,
    },
    /// An anchor whose selector is not a persistent id.
    #[error(
        "annotation {index} of view {view} anchors by {selector}; a drawing annotation must \
         anchor by Selector::Pid, which never rebinds"
    )]
    AnchorNotPid {
        view: Uuid,
        index: usize,
        selector: &'static str,
    },
    /// An anchor whose pid names nothing in this view.
    ///
    /// The loud half of D0's never-rebinding `Selector::Pid`: the entity is
    /// gone, or it is not drawn in this view (a hole on the far side of the
    /// part), and either way there is no geometry to measure. It does NOT
    /// fall back to a neighbour.
    #[error(
        "annotation {index} of view {view}: {kind:?} pid {pid} resolves to no geometry in this \
         view"
    )]
    AnchorUnresolved {
        view: Uuid,
        index: usize,
        kind: TopoKind,
        pid: u64,
    },
    /// An anchor whose pid names more than one projected curve — two
    /// identical bodies in one view, say. Refused rather than disambiguated:
    /// a dimension must not silently pick one of two.
    #[error(
        "annotation {index} of view {view}: {kind:?} pid {pid} resolves to {count} projected \
         curves in this view; it is ambiguous"
    )]
    AnchorAmbiguous {
        view: Uuid,
        index: usize,
        kind: TopoKind,
        pid: u64,
        count: usize,
    },
    /// The dimension's anchors resolved but could not be measured (D3's four
    /// refusals: non-parallel lines, a sampled polyline, a radius on a
    /// straight edge, a degenerate anchor).
    #[error("annotation {index} of view {view}: {source}")]
    NotMeasurable {
        view: Uuid,
        index: usize,
        #[source]
        source: MeasureError,
    },
    /// The kernel refused the projection.
    #[error("view {view} could not be projected: {message}")]
    ProjectionFailed { view: Uuid, message: String },
    /// An annotation kind this increment does not resolve. Named, not
    /// skipped — an annotation a drafter authored and the sheet silently does
    /// not draw is worse than a loud gap.
    #[error("annotation {index} of view {view} is a {kind}, which D4a does not resolve yet")]
    AnnotationNotSupported {
        view: Uuid,
        index: usize,
        kind: &'static str,
    },
}

// -------------------------------------------------------------------- model

/// A `Drawing` tab's content.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Drawing {
    #[serde(default)]
    pub sheets: Vec<Sheet>,
    /// Which projection standard a `ProjectedFrom` view follows. A document
    /// setting, per §8, with the ISO/European default of third angle — the
    /// one §8 names.
    #[serde(default)]
    pub projection_angle: ProjectionAngle,
    /// Unknown keys preserved across load → save (v4 §2.6).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Drawing {
    /// A drawing with one empty sheet of the default size — what a freshly
    /// added `Drawing` tab holds.
    pub fn new() -> Drawing {
        Drawing {
            sheets: vec![Sheet::new("Sheet 1")],
            ..Drawing::default()
        }
    }

    pub fn sheet(&self, id: Uuid) -> Option<&Sheet> {
        self.sheets.iter().find(|s| s.id == id)
    }

    pub fn sheet_mut(&mut self, id: Uuid) -> Option<&mut Sheet> {
        self.sheets.iter_mut().find(|s| s.id == id)
    }

    /// The sheet holding `view`, and the view.
    pub fn find_view(&self, view: Uuid) -> Option<(&Sheet, &DrawingView)> {
        self.sheets
            .iter()
            .find_map(|s| s.view(view).map(|v| (s, v)))
    }

    /// The sheet holding `view`, mutably.
    pub fn sheet_of_view_mut(&mut self, view: Uuid) -> Option<&mut Sheet> {
        self.sheets
            .iter_mut()
            .find(|s| s.views.iter().any(|v| v.id == view))
    }

    /// Structural problems, as loader warnings — never a load failure, the
    /// same contract as [`crate::assembly::AssemblyTree::validate`].
    pub fn validate(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut sheet_ids = HashSet::new();
        for sheet in &self.sheets {
            if !sheet_ids.insert(sheet.id) {
                out.push(format!("duplicate sheet id {}", sheet.id));
            }
            let mut view_ids = HashSet::new();
            for view in &sheet.views {
                if !view_ids.insert(view.id) {
                    out.push(format!("duplicate view id {}", view.id));
                }
                if !(view.scale.is_finite() && view.scale > 0.0) {
                    out.push(format!(
                        "view {} has a non-positive scale ({})",
                        view.id, view.scale
                    ));
                }
            }
            for view in &sheet.views {
                if let Some(parent) = view.projection.parent() {
                    if !view_ids.contains(&parent) {
                        out.push(format!(
                            "view {} is {} view {}, which is not on sheet {}",
                            view.id,
                            match view.projection {
                                Projection::Section { .. } => "a section of",
                                Projection::Detail { .. } => "a detail of",
                                _ => "projected from",
                            },
                            parent,
                            sheet.id
                        ));
                    }
                }
                if let Projection::Detail { radius, .. } = &view.projection {
                    if !(radius.is_finite() && *radius > 0.0) {
                        out.push(format!(
                            "detail view {} has a non-positive crop radius ({radius})",
                            view.id
                        ));
                    }
                }
                if let Projection::Section { from, to, .. } = &view.projection {
                    if cut_line_2d(*from, *to, false).is_none() {
                        out.push(format!(
                            "section view {} has a cutting line of no length ({from:?} to {to:?})",
                            view.id
                        ));
                    }
                }
            }
        }
        out
    }
}

/// One sheet of paper.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Sheet {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub size: SheetSize,
    #[serde(default)]
    pub orientation: Orientation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<DrawingView>,
    /// The sheet's title block (D4b).
    #[serde(default)]
    pub title_block: TitleBlock,
    /// The last rebuild's title-block rows — a derived hint on exactly the
    /// terms a view's `cache` is one (see the module docs): recomputed every
    /// rebuild, persisted so a reader with no engine can draw the paper
    /// complete, never authoritative.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_block_cache: Option<TitleBlockLayout>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Sheet {
    /// An empty A3 landscape sheet with a fresh id.
    pub fn new(name: impl Into<String>) -> Sheet {
        Sheet {
            id: Uuid::new_v4(),
            name: name.into(),
            size: SheetSize::default(),
            orientation: Orientation::default(),
            views: Vec::new(),
            title_block: TitleBlock::default(),
            title_block_cache: None,
            extra: Map::new(),
        }
    }

    pub fn view(&self, id: Uuid) -> Option<&DrawingView> {
        self.views.iter().find(|v| v.id == id)
    }

    pub fn view_mut(&mut self, id: Uuid) -> Option<&mut DrawingView> {
        self.views.iter_mut().find(|v| v.id == id)
    }

    /// The sheet's `[width, height]` in millimetres, orientation applied.
    pub fn extent_mm(&self) -> [f64; 2] {
        self.size.extent_mm(self.orientation)
    }

    /// The view frame `view` projects with, following a `ProjectedFrom` chain
    /// to the named view it is ultimately derived from.
    ///
    /// `angle` is the document's projection standard, which only
    /// [`Projection::ProjectedFrom`] reads.
    pub fn view_frame(
        &self,
        view: Uuid,
        angle: ProjectionAngle,
    ) -> Result<ViewFrame, DrawingError> {
        self.view_frame_inner(view, angle, &mut Vec::new())
    }

    fn view_frame_inner(
        &self,
        view: Uuid,
        angle: ProjectionAngle,
        chain: &mut Vec<Uuid>,
    ) -> Result<ViewFrame, DrawingError> {
        if chain.contains(&view) {
            return Err(DrawingError::ProjectionCycle {
                view,
                length: chain.len(),
            });
        }
        chain.push(view);
        let v = self.view(view).ok_or(DrawingError::UnknownParent {
            view: *chain.first().unwrap_or(&view),
            parent: view,
        })?;
        let frame = match &v.projection {
            Projection::Named { view: named } => named.frame(),
            Projection::Custom { dir, up } => ViewFrame::from_parts(Some(*dir), *up),
            Projection::ProjectedFrom { parent, direction } => {
                let basis = self.parent_basis(view, *parent, angle, chain)?;
                projected_frame(&basis, *direction, angle)
            }
            Projection::Section {
                parent,
                from,
                to,
                flip,
                ..
            } => {
                let basis = self.parent_basis(view, *parent, angle, chain)?;
                let cut = section_plane(&basis, *from, *to, *flip).ok_or(
                    DrawingError::DegenerateCut {
                        view,
                        from: *from,
                        to: *to,
                    },
                )?;
                section_frame(&basis, &cut)
            }
            // A detail is the SAME projection as its parent, magnified: its
            // frame is the parent's, exactly. Re-deriving one would be the
            // next thing to disagree with the view it crops.
            Projection::Detail { parent, .. } => {
                let basis = self.parent_basis(view, *parent, angle, chain)?;
                ViewFrame {
                    origin: basis.origin,
                    dir: basis.w,
                    up: basis.v,
                }
            }
        };
        if frame.basis().is_none() {
            return Err(DrawingError::DegenerateFrame {
                view,
                dir: frame.dir,
                up: frame.up,
            });
        }
        Ok(frame)
    }

    /// The basis of `parent`, for a view derived from it.
    fn parent_basis(
        &self,
        view: Uuid,
        parent: Uuid,
        angle: ProjectionAngle,
        chain: &mut Vec<Uuid>,
    ) -> Result<ViewBasis, DrawingError> {
        if self.view(parent).is_none() {
            return Err(DrawingError::UnknownParent { view, parent });
        }
        let frame = self.view_frame_inner(parent, angle, chain)?;
        frame.basis().ok_or(DrawingError::DegenerateFrame {
            view: parent,
            dir: frame.dir,
            up: frame.up,
        })
    }

    /// What is marked on `view` because other views of this sheet were derived
    /// from it: each child section's cutting line, each child detail's circle
    /// (D4b).
    ///
    /// Derived here rather than stored on the parent, for the reason the
    /// delete cascade exists: a mark stored on the parent is a second record
    /// of the child's own geometry, free to survive the child's deletion.
    ///
    /// The arrows point along the direction of SIGHT, which is into the
    /// material the section keeps — so a reader can tell which half is drawn
    /// from the parent alone.
    pub fn marks_on(&self, view: Uuid) -> Vec<ViewMark> {
        let mut out = Vec::new();
        for child in &self.views {
            match &child.projection {
                Projection::Section {
                    parent,
                    from,
                    to,
                    flip,
                    label,
                } if *parent == view => {
                    let Some((_, normal2)) = cut_line_2d(*from, *to, *flip) else {
                        continue;
                    };
                    out.push(ViewMark::Section {
                        from: *from,
                        to: *to,
                        sight: [-normal2[0], -normal2[1]],
                        label: label.clone(),
                    });
                }
                Projection::Detail {
                    parent,
                    center,
                    radius,
                    label,
                } if *parent == view => out.push(ViewMark::Detail {
                    center: *center,
                    radius: *radius,
                    label: label.clone(),
                }),
                _ => {}
            }
        }
        out
    }

    /// The next free section/detail letter on this sheet: `A`, `B`, … `Z`,
    /// then `AA`.
    ///
    /// Letters rather than numbers because that is what the standard prints,
    /// and the next FREE one rather than a count so that deleting `A` and
    /// adding a cut does not mint a second `B`.
    pub fn next_label(&self) -> String {
        let taken: HashSet<&str> = self
            .views
            .iter()
            .filter_map(|v| v.projection.label())
            .collect();
        for n in 0..u32::from(u16::MAX) {
            let label = label_at(n as usize);
            if !taken.contains(label.as_str()) {
                return label;
            }
        }
        // Unreachable in practice (65 535 marks on one sheet); a loud name
        // beats a panic in a document model.
        "?".to_string()
    }
}

/// The `n`-th drafting label: `A`…`Z`, `AA`…`AZ`, … (bijective base 26).
fn label_at(n: usize) -> String {
    let mut n = n + 1;
    let mut out = Vec::new();
    while n > 0 {
        let rem = (n - 1) % 26;
        out.push(b'A' + rem as u8);
        n = (n - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_else(|_| "?".to_string())
}

/// Paper sizes, portrait dimensions in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum SheetSize {
    A4,
    /// The default: the smallest ISO size that carries a six-view layout of a
    /// palm-sized part at 1:1 without crowding.
    #[default]
    A3,
    A2,
    A1,
    A0,
    Letter,
    Tabloid,
    Custom {
        width_mm: f64,
        height_mm: f64,
    },
}

impl SheetSize {
    /// `[width, height]` in millimetres with `orientation` applied. A
    /// `Custom` size is taken as authored and is NOT swapped — a caller who
    /// typed both numbers meant both numbers.
    pub fn extent_mm(&self, orientation: Orientation) -> [f64; 2] {
        let portrait = match self {
            SheetSize::A4 => [210.0, 297.0],
            SheetSize::A3 => [297.0, 420.0],
            SheetSize::A2 => [420.0, 594.0],
            SheetSize::A1 => [594.0, 841.0],
            SheetSize::A0 => [841.0, 1189.0],
            SheetSize::Letter => [215.9, 279.4],
            SheetSize::Tabloid => [279.4, 431.8],
            SheetSize::Custom {
                width_mm,
                height_mm,
            } => return [*width_mm, *height_mm],
        };
        match orientation {
            Orientation::Portrait => portrait,
            Orientation::Landscape => [portrait[1], portrait[0]],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Orientation {
    #[default]
    Landscape,
    Portrait,
}

// --------------------------------------------------------------- title block

/// A sheet's title block (`specs/drawings_and_mbd.md` §8, D4b): the data
/// fields printed in the frame's bottom-right corner.
///
/// **The fields are KEYS and authored text, not expressions.** §8 writes
/// "title block fields are expressions over document metadata and the
/// measurement functions, so `mass(part)` and a parameter table work with no
/// special casing" — and that is right once D2 lands. It has not, and the
/// choice is the same one D4a made for `Measured::Expr`: an unevaluated
/// expression printed as a number would be a different number from the one
/// authored, and printed as its own source text would be a title block
/// reading `mass(part)`. So this increment offers the keys the engine can
/// actually fill plus literal text for the ones only a person knows, and
/// [`TitleBlockKey::Expr`] is deliberately absent — a variant that exists and
/// cannot be filled is a document a user can author and the engine cannot
/// draw (the D4a argument for leaving `Section` out until it worked).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct TitleBlock {
    /// Draw it. On by default: a drawing without a title block is not a
    /// controlled document, and a sheet that has one is the ordinary case.
    #[serde(default = "yes")]
    pub show: bool,
    /// The rows, in print order.
    #[serde(default = "TitleBlock::default_fields")]
    pub fields: Vec<TitleBlockField>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for TitleBlock {
    fn default() -> TitleBlock {
        TitleBlock {
            show: true,
            fields: TitleBlock::default_fields(),
            extra: Map::new(),
        }
    }
}

impl TitleBlock {
    /// The rows a fresh sheet carries: ISO 7200's mandatory data fields that
    /// this increment can actually fill or be told — the document's name, the
    /// sheet number, the scale and the projection standard (all derived), plus
    /// the date and the responsible person (both authored, and blank until
    /// someone types them, which on paper is the line you sign).
    ///
    /// The date is NOT generated. A rebuild that stamped today's date would
    /// make the document change when nothing changed, break every byte oracle
    /// over the sheet, and print an issue date the issue did not have.
    pub fn default_fields() -> Vec<TitleBlockField> {
        [
            TitleBlockKey::DocumentName,
            TitleBlockKey::SheetNumber,
            TitleBlockKey::Scale,
            TitleBlockKey::ProjectionAngle,
            TitleBlockKey::Date,
            TitleBlockKey::Author,
        ]
        .into_iter()
        .map(TitleBlockField::new)
        .collect()
    }
}

/// One title-block row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct TitleBlockField {
    pub key: TitleBlockKey,
    /// The authored value, for a key the engine cannot derive. Ignored for a
    /// derived key — see [`TitleBlockKey::is_derived`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl TitleBlockField {
    pub fn new(key: TitleBlockKey) -> TitleBlockField {
        TitleBlockField {
            key,
            text: None,
            extra: Map::new(),
        }
    }

    /// This field with authored text.
    pub fn with_text(key: TitleBlockKey, text: impl Into<String>) -> TitleBlockField {
        TitleBlockField {
            key,
            text: Some(text.into()),
            extra: Map::new(),
        }
    }
}

/// What a title-block row says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum TitleBlockKey {
    /// The document's own name — derived.
    DocumentName,
    /// `2 / 5` — derived from the sheet's position in the drawing.
    SheetNumber,
    /// The views' scale as a ratio, or the standard `AS SHOWN` when they
    /// differ — derived.
    Scale,
    /// `First angle` / `Third angle` — derived from the drawing's own setting.
    ProjectionAngle,
    Date,
    Author,
    Material,
    Revision,
    /// A row whose label a person chose.
    Custom {
        label: String,
    },
}

impl TitleBlockKey {
    /// The printed label.
    pub fn label(&self) -> &str {
        match self {
            TitleBlockKey::DocumentName => "Title",
            TitleBlockKey::SheetNumber => "Sheet",
            TitleBlockKey::Scale => "Scale",
            TitleBlockKey::ProjectionAngle => "Projection",
            TitleBlockKey::Date => "Date",
            TitleBlockKey::Author => "Drawn by",
            TitleBlockKey::Material => "Material",
            TitleBlockKey::Revision => "Rev",
            TitleBlockKey::Custom { label } => label.as_str(),
        }
    }

    /// Whether the engine fills this row, so an authored `text` on it would be
    /// a second source of truth for a number the document already knows.
    pub fn is_derived(&self) -> bool {
        matches!(
            self,
            TitleBlockKey::DocumentName
                | TitleBlockKey::SheetNumber
                | TitleBlockKey::Scale
                | TitleBlockKey::ProjectionAngle
        )
    }

    /// The tag a tool argument names this by.
    pub fn tag(&self) -> &str {
        match self {
            TitleBlockKey::DocumentName => "DocumentName",
            TitleBlockKey::SheetNumber => "SheetNumber",
            TitleBlockKey::Scale => "Scale",
            TitleBlockKey::ProjectionAngle => "ProjectionAngle",
            TitleBlockKey::Date => "Date",
            TitleBlockKey::Author => "Author",
            TitleBlockKey::Material => "Material",
            TitleBlockKey::Revision => "Revision",
            TitleBlockKey::Custom { .. } => "Custom",
        }
    }

    /// Every authorable key, for a tool's argument enumeration (`Custom` is
    /// named by its own label and so is not in the list).
    pub const AUTHORABLE: [&'static str; 8] = [
        "DocumentName",
        "SheetNumber",
        "Scale",
        "ProjectionAngle",
        "Date",
        "Author",
        "Material",
        "Revision",
    ];

    /// The key a tool argument names, or `None`.
    pub fn from_tag(tag: &str) -> Option<TitleBlockKey> {
        Some(match tag {
            "DocumentName" => TitleBlockKey::DocumentName,
            "SheetNumber" => TitleBlockKey::SheetNumber,
            "Scale" => TitleBlockKey::Scale,
            "ProjectionAngle" => TitleBlockKey::ProjectionAngle,
            "Date" => TitleBlockKey::Date,
            "Author" => TitleBlockKey::Author,
            "Material" => TitleBlockKey::Material,
            "Revision" => TitleBlockKey::Revision,
            _ => return None,
        })
    }
}

/// The title block's rows, filled — the record the renderer draws (D4b).
///
/// Flat label/value pairs for the same reason [`ViewLayout`] is flat: a
/// renderer holding one cannot reach the document, so it has no way to print a
/// value other than the one the engine resolved.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct TitleBlockLayout {
    pub rows: Vec<TitleBlockRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct TitleBlockRow {
    pub label: String,
    pub value: String,
}

/// What the derived title-block rows are filled from.
#[derive(Debug, Clone, Copy)]
pub struct TitleBlockContext<'a> {
    pub document_name: &'a str,
    /// 1-based, as it prints.
    pub sheet_number: usize,
    pub sheet_count: usize,
    pub angle: ProjectionAngle,
}

/// Fill `block`'s rows (D4b).
///
/// A derived key's authored `text` is IGNORED rather than preferred: a title
/// block whose sheet number disagrees with the sheet it is printed on is worse
/// than one a person cannot overrule.
pub fn title_block_layout(
    block: &TitleBlock,
    sheet: &Sheet,
    ctx: &TitleBlockContext,
) -> TitleBlockLayout {
    let rows = block
        .fields
        .iter()
        .map(|field| TitleBlockRow {
            label: field.key.label().to_string(),
            value: match &field.key {
                TitleBlockKey::DocumentName => ctx.document_name.to_string(),
                TitleBlockKey::SheetNumber => {
                    format!("{} / {}", ctx.sheet_number, ctx.sheet_count)
                }
                TitleBlockKey::Scale => sheet_scale_label(sheet),
                TitleBlockKey::ProjectionAngle => match ctx.angle {
                    ProjectionAngle::Third => "Third angle".to_string(),
                    ProjectionAngle::First => "First angle".to_string(),
                },
                _ => field.text.clone().unwrap_or_default(),
            },
        })
        .collect();
    TitleBlockLayout { rows }
}

/// A sheet's scale as a title block prints it: the one ratio every view shares,
/// `AS SHOWN` when they differ (the standard note), and `—` for a sheet with
/// no views.
///
/// A detail view is EXCLUDED from the comparison: its scale is printed under
/// its own label (`DETAIL A (2:1)`), and counting it would make every sheet
/// carrying a detail read `AS SHOWN` — which is true of the paper and useless
/// as a statement about the part.
pub fn sheet_scale_label(sheet: &Sheet) -> String {
    let mut scales = sheet
        .views
        .iter()
        .filter(|v| !matches!(v.projection, Projection::Detail { .. }))
        .map(|v| v.scale)
        .peekable();
    let Some(first) = scales.next() else {
        return "—".to_string();
    };
    if scales.any(|s| (s - first).abs() > 1e-9) {
        return "AS SHOWN".to_string();
    }
    scale_ratio_label(first)
}

/// `1:1`, `1:2`, `2:1` — the ratio a drafter reads, from the number.
pub fn scale_ratio_label(scale: f64) -> String {
    if !(scale.is_finite() && scale > 0.0) {
        return "—".to_string();
    }
    let round = |x: f64| {
        if (x - x.round()).abs() < 1e-6 {
            format!("{}", x.round())
        } else {
            format!("{x:.2}")
        }
    };
    if (scale - 1.0).abs() < 1e-9 {
        "1:1".to_string()
    } else if scale < 1.0 {
        format!("1:{}", round(1.0 / scale))
    } else {
        format!("{}:1", round(scale))
    }
}

/// Third angle (ISO/ASME "third-angle projection", the §8 default) or first.
///
/// The difference is which side of the parent a projected view is *of*: in
/// third angle the view placed to the right is the right-hand view, in first
/// angle it is the left-hand one. See [`projected_frame`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ProjectionAngle {
    #[default]
    Third,
    First,
}

/// One view on a sheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct DrawingView {
    pub id: Uuid,
    pub name: String,
    pub source: ViewSource,
    pub projection: Projection,
    /// Paper length per model length: `1.0` is 1:1, `0.1` is 1:10.
    #[serde(default = "unit_scale")]
    pub scale: f64,
    /// Where the view's CONTENT CENTRE sits on the sheet, in millimetres from
    /// the sheet's bottom-left corner, `+x` right and `+y` up.
    ///
    /// The centre rather than the view-plane origin, because the origin is a
    /// property of the projection and can be far outside the part (a plate
    /// sketched at `(1 m, 1 m)` projects nowhere near its own `(0, 0)`), so
    /// placing by it makes the auto-layout unpredictable. The centre is also
    /// what a drafter drags.
    #[serde(default)]
    pub placement_mm: [f64; 2],
    #[serde(default)]
    pub style: ViewStyle,
    /// The annotations drawn on this view. Their anchors are
    /// `Selector::Pid`s resolved against THIS view's projection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,
    /// The last rebuild's layout — a derived hint (see the module docs), so a
    /// reader with no kernel can still draw the sheet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<ViewLayout>,
    /// What `cache` was built FROM ([`view_cache_key`]) — D4b, closing D4a's
    /// "the cache has no validity key".
    ///
    /// Without it a persisted layout is indistinguishable from a current one,
    /// so a document opened in a build with no kernel (or opened and not yet
    /// rebuilt) draws last week's sheet with no sign that it is last week's.
    /// With it the same document says so. It is written beside the cache on
    /// every rebuild and is `None` exactly when `cache` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_key: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn unit_scale() -> f64 {
    1.0
}

impl DrawingView {
    /// A view of `source` with a fresh id, 1:1, at the sheet's origin corner.
    pub fn new(name: impl Into<String>, source: ViewSource, projection: Projection) -> DrawingView {
        DrawingView {
            id: Uuid::new_v4(),
            name: name.into(),
            source,
            projection,
            scale: 1.0,
            placement_mm: [0.0, 0.0],
            style: ViewStyle::default(),
            annotations: Vec::new(),
            cache: None,
            cache_key: None,
            extra: Map::new(),
        }
    }

    /// This view's own recipe — everything that decides what it draws, with
    /// the derived hints (`cache`, `cache_key`) taken out.
    ///
    /// Serialized rather than hashed field by field so that a field added
    /// later is covered by default. The alternative — a hand-written list —
    /// is a list someone has to remember to extend, and the failure mode of
    /// forgetting is a cache that reads as valid after the change that
    /// invalidated it.
    ///
    /// Through `Value` rather than straight to a string, which is what makes
    /// it CANONICAL: a `serde_json::Value` object is a `BTreeMap`, so every
    /// key at every depth comes out sorted. Nothing in a `DrawingView` is a
    /// `HashMap` today, but a feature tree is, and serializing one directly
    /// produced a different string on every rebuild — see
    /// `wasm_bridge::drawing_view::source_recipe`, which found it the hard
    /// way. A digest input is canonicalized here so a field added later
    /// cannot reintroduce it.
    fn recipe(&self) -> String {
        let mut bare = self.clone();
        bare.cache = None;
        bare.cache_key = None;
        serde_json::to_value(&bare)
            .ok()
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok())
            .unwrap_or_default()
    }
}

/// Which bodies a view projects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ViewSource {
    /// A tab of THIS document. A view of a linked `.waffle` source (the
    /// `PartRef::source_id` case) is not D4a's — the drawing would have to
    /// rebuild another document's tree to project it.
    pub tab_id: String,
    /// Body names, as the source tab's rebuild reports them. Empty means
    /// every live body of that tab, which is the ordinary case and the one a
    /// freshly added view gets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bodies: Vec<String>,
}

impl ViewSource {
    /// Every live body of `tab_id`.
    pub fn whole_tab(tab_id: impl Into<String>) -> ViewSource {
        ViewSource {
            tab_id: tab_id.into(),
            bodies: Vec::new(),
        }
    }

    /// Whether this source projects `name`.
    pub fn includes(&self, name: &str) -> bool {
        self.bodies.is_empty() || self.bodies.iter().any(|b| b == name)
    }
}

/// How a view is oriented.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Projection {
    /// One of the standard directions.
    Named { view: NamedView },
    /// A free direction, with an optional paper up.
    Custom {
        dir: [f64; 3],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        up: Option<[f64; 3]>,
    },
    /// Derived from another view on the same sheet by the projection standard.
    ProjectedFrom {
        parent: Uuid,
        direction: ProjectedDirection,
    },
    /// A cut (D4b): a CUTTING LINE drawn on `parent`, which with the parent's
    /// line of sight determines the cut plane, and a view looking along that
    /// plane's normal at the material the cut keeps.
    ///
    /// **The line is the authored thing, not the plane.** §8 sketches
    /// `Section { parent, plane: Plane }`. A plane in world coordinates can be
    /// authored to miss the parent view entirely, or to lie oblique to its
    /// line of sight — and then the cutting line drawn on the parent is a
    /// projection of the plane rather than the plane itself, so dragging the
    /// line on the sheet would not be an edit of the view. A line in the
    /// parent's own `(u, v)` is exactly what a drafter draws and exactly what
    /// a UI drags, and the plane follows from it with no degrees of freedom
    /// left over: it is the line swept along the parent's sight direction.
    Section {
        parent: Uuid,
        /// The cutting line's ends, in the parent view's `(u, v)` in meters.
        from: [f64; 2],
        to: [f64; 2],
        /// Keep the other half. Without it the half kept is fixed by the
        /// order of `from`/`to`, which is not something a drafter chooses:
        /// the arrows have to be reversible without redrawing the line.
        #[serde(default)]
        flip: bool,
        /// The letter the cut is known by: `A` prints at each arrow on the
        /// parent and the view is titled `SECTION A-A`.
        #[serde(default = "first_label")]
        label: String,
    },
    /// A magnified crop of `parent` (D4b): the disc the detail shows, drawn on
    /// the parent as a circle, and the detail's own scale.
    ///
    /// **A disc, not §8's `rect: Aabb2`.** ISO 128-30's detail boundary is a
    /// circle (a rectangle is the alternative the standard allows for a
    /// broken-out view), and a circle has no orientation to disagree with the
    /// view frame — a rectangle authored in the parent's `(u, v)` would have
    /// to be re-derived if the parent's paper up ever changed. §5.4 rules out
    /// detail views "beyond cropping a parent view's `ViewGeometry`", which is
    /// what this is.
    Detail {
        parent: Uuid,
        /// The crop's centre, in the parent view's `(u, v)` in meters.
        center: [f64; 2],
        /// In meters, in the parent's own units — NOT paper millimetres: the
        /// circle is a region of the model's projection, so it has to scale
        /// with the part and not with the paper.
        radius: f64,
        #[serde(default = "first_label")]
        label: String,
    },
}

fn first_label() -> String {
    "A".to_string()
}

impl Projection {
    /// The view this one is derived from, if any — a `ProjectedFrom`'s parent,
    /// a `Section`'s, a `Detail`'s.
    ///
    /// One place, so a new derived kind cannot be forgotten by the cycle
    /// check, the delete cascade and the validator independently.
    pub fn parent(&self) -> Option<Uuid> {
        match self {
            Projection::Named { .. } | Projection::Custom { .. } => None,
            Projection::ProjectedFrom { parent, .. }
            | Projection::Section { parent, .. }
            | Projection::Detail { parent, .. } => Some(*parent),
        }
    }

    /// The label a section or detail is known by.
    pub fn label(&self) -> Option<&str> {
        match self {
            Projection::Section { label, .. } | Projection::Detail { label, .. } => {
                Some(label.as_str())
            }
            _ => None,
        }
    }

    /// The tag a tool argument and a tool answer name this by.
    pub fn tag(&self) -> &'static str {
        match self {
            Projection::Named { .. } => "Named",
            Projection::Custom { .. } => "Custom",
            Projection::ProjectedFrom { .. } => "ProjectedFrom",
            Projection::Section { .. } => "Section",
            Projection::Detail { .. } => "Detail",
        }
    }
}

/// The standard view directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum NamedView {
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
    /// The isometric view from `(+1, +1, +1)`, `+z` up.
    Iso,
}

impl NamedView {
    /// This view's frame.
    ///
    /// `Top`, `Front` and `Right` are
    /// [`ViewFrame::TOP`]/[`ViewFrame::FRONT`]/[`ViewFrame::RIGHT`] — the
    /// kernel's own constants, not a second definition of them. The other
    /// four are their opposites with the paper up that keeps the view the
    /// right way round (a bottom view mirrors a top one; it does not turn it
    /// upside down).
    pub fn frame(&self) -> ViewFrame {
        match self {
            NamedView::Top => ViewFrame::TOP,
            NamedView::Front => ViewFrame::FRONT,
            NamedView::Right => ViewFrame::RIGHT,
            // Paper up is −y, not +y: a bottom view shares its horizontal
            // axis with the front view it sits under (+x to the right in
            // both), which is what makes a projection group readable. With
            // +y up it would come out mirrored against the front view — the
            // classic wrong bottom view.
            NamedView::Bottom => ViewFrame {
                origin: [0.0, 0.0, 0.0],
                dir: [0.0, 0.0, 1.0],
                up: [0.0, -1.0, 0.0],
            },
            NamedView::Back => ViewFrame {
                origin: [0.0, 0.0, 0.0],
                dir: [0.0, -1.0, 0.0],
                up: [0.0, 0.0, 1.0],
            },
            NamedView::Left => ViewFrame {
                origin: [0.0, 0.0, 0.0],
                dir: [1.0, 0.0, 0.0],
                up: [0.0, 0.0, 1.0],
            },
            NamedView::Iso => ViewFrame {
                origin: [0.0, 0.0, 0.0],
                dir: [-1.0, -1.0, -1.0],
                up: [0.0, 0.0, 1.0],
            },
        }
    }

    /// The tag used in a tool argument and a tool error.
    pub fn tag(&self) -> &'static str {
        match self {
            NamedView::Front => "Front",
            NamedView::Back => "Back",
            NamedView::Left => "Left",
            NamedView::Right => "Right",
            NamedView::Top => "Top",
            NamedView::Bottom => "Bottom",
            NamedView::Iso => "Iso",
        }
    }

    /// Every named view, for a tool's argument enumeration.
    pub const ALL: [NamedView; 7] = [
        NamedView::Front,
        NamedView::Back,
        NamedView::Left,
        NamedView::Right,
        NamedView::Top,
        NamedView::Bottom,
        NamedView::Iso,
    ];
}

/// Which way a projected view sits from its parent ON PAPER.
///
/// Named by the paper placement rather than by what is seen, because the
/// placement is what is fixed and the content is what the projection standard
/// decides: `Right` is the right-hand view in third angle and the left-hand
/// view in first angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ProjectedDirection {
    Left,
    Right,
    Up,
    Down,
}

impl ProjectedDirection {
    /// The paper offset direction, in sheet millimetres (`+x` right, `+y` up).
    pub fn paper_step(&self) -> [f64; 2] {
        match self {
            ProjectedDirection::Left => [-1.0, 0.0],
            ProjectedDirection::Right => [1.0, 0.0],
            ProjectedDirection::Up => [0.0, 1.0],
            ProjectedDirection::Down => [0.0, -1.0],
        }
    }

    /// The placement across the parent from this one — which is the whole of
    /// the difference between the two projection standards (see
    /// [`projected_frame`]).
    pub fn opposite(&self) -> ProjectedDirection {
        match self {
            ProjectedDirection::Left => ProjectedDirection::Right,
            ProjectedDirection::Right => ProjectedDirection::Left,
            ProjectedDirection::Up => ProjectedDirection::Down,
            ProjectedDirection::Down => ProjectedDirection::Up,
        }
    }
}

/// What a view draws.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ViewStyle {
    /// Draw the curves D1c classified `Hidden`. On by default: a hidden-line
    /// drawing is the drafting default, and D1c is what makes it possible.
    #[serde(default = "yes")]
    pub hidden_lines: bool,
    /// Draw curved faces' silhouettes (D1b). On by default — without them a
    /// cylinder has no outline at all.
    #[serde(default = "yes")]
    pub silhouettes: bool,
}

fn yes() -> bool {
    true
}

impl Default for ViewStyle {
    fn default() -> ViewStyle {
        ViewStyle {
            hidden_lines: true,
            silhouettes: true,
        }
    }
}

impl ViewStyle {
    /// Whether a curve of this kind and visibility is drawn.
    pub fn draws(&self, kind: CurveKind, visibility: Visibility) -> bool {
        if visibility == Visibility::Hidden && !self.hidden_lines {
            return false;
        }
        if kind == CurveKind::Silhouette && !self.silhouettes {
            return false;
        }
        true
    }
}

// ------------------------------------------------------------- frame algebra

/// The frame of a view projected from a parent with basis `parent`, placed
/// `direction` of it on the paper, under projection standard `angle`.
///
/// Third angle, with the parent a front view (`u = +x`, `v = +z`, `w = +y`):
///
/// | placement | sees | `dir` | `up` |
/// |---|---|---|---|
/// | `Right` | the right side | `−u` | `v` |
/// | `Left` | the left side | `+u` | `v` |
/// | `Up` | the top | `−v` | `+w` |
/// | `Down` | the bottom | `+v` | `−w` |
///
/// which for the front parent gives `Right → dir −x, up +z` (exactly
/// [`ViewFrame::RIGHT`], the right-side view) and `Up → dir −z, up +y`
/// (exactly [`ViewFrame::TOP`]) — the two the standard is usually stated
/// with.
///
/// **First angle is the table read across:** the view placed on one side
/// shows the side opposite, so a first-angle placement takes the THIRD-ANGLE
/// FRAME OF ITS OPPOSITE placement. Negating only `dir` and keeping `up` is
/// not the same thing and was this function's first bug: `Up` in first angle
/// then came out as a bottom view with paper up `+w`, mirrored horizontally
/// against the parent it sits above — the classic wrong bottom view, arrived
/// at from the other direction. Taking the opposite row makes the two
/// standards one rule and one table, and it is what the standard itself
/// says.
pub fn projected_frame(
    parent: &ViewBasis,
    direction: ProjectedDirection,
    angle: ProjectionAngle,
) -> ViewFrame {
    let (u, v, w) = (parent.u, parent.v, parent.w);
    let shown = match angle {
        ProjectionAngle::Third => direction,
        ProjectionAngle::First => direction.opposite(),
    };
    let (dir, up) = match shown {
        ProjectedDirection::Right => (neg(u), v),
        ProjectedDirection::Left => (u, v),
        ProjectedDirection::Up => (neg(v), w),
        ProjectedDirection::Down => (v, neg(w)),
    };
    ViewFrame {
        origin: [0.0, 0.0, 0.0],
        dir,
        up,
    }
}

fn neg(v: [f64; 3]) -> [f64; 3] {
    [-v[0], -v[1], -v[2]]
}

// ------------------------------------------------------------ section algebra

/// The plane a section view cuts with: a world origin on the cutting line and
/// a unit normal (D4b).
///
/// The kept half-space is the one the normal points AWAY from, which is the
/// convention [`KernelProjection::section_with_plane`] takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CutPlane {
    pub origin: [f64; 3],
    pub normal: [f64; 3],
    /// The normal's own `(u, v)` components in the PARENT view's plane — what
    /// the cutting line's arrows point against and what the section view's
    /// paper placement steps along.
    pub normal_in_parent: [f64; 2],
}

/// A cutting line's `(direction, normal)` in the parent view's `(u, v)`, both
/// unit, or `None` for a line of no length.
///
/// The normal is the line's left perpendicular (`perp(d) = (−dᵥ, dᵤ)`), with
/// `flip` reversing it. It points at the DISCARDED side, because that is the
/// sense `section_with_plane` keeps: every kept point satisfies
/// `(p − origin)·n̂ ≤ 0`.
fn cut_line_2d(from: [f64; 2], to: [f64; 2], flip: bool) -> Option<([f64; 2], [f64; 2])> {
    let d = [to[0] - from[0], to[1] - from[1]];
    if !d.iter().all(|x| x.is_finite()) || !from.iter().chain(to.iter()).all(|x| x.is_finite()) {
        return None;
    }
    let len = d[0].hypot(d[1]);
    // MIN_FEATURE_SIZE rather than an epsilon: a cutting line shorter than the
    // smallest feature the kernel will model is not a line a drafter drew, and
    // normalizing it amplifies the authored noise into the plane's normal.
    if !(len.is_finite() && len > waffle_types::kernel::units::MIN_FEATURE_SIZE) {
        return None;
    }
    let dir = [d[0] / len, d[1] / len];
    let sign = if flip { -1.0 } else { 1.0 };
    Some((dir, [-dir[1] * sign, dir[0] * sign]))
}

/// The cut plane a section view's line defines, given its parent's basis.
///
/// The plane contains the cutting line and the parent's LINE OF SIGHT: it is
/// the line swept straight back into the paper. That is what makes the line
/// the authored thing — there is no plane a drafter could mean by a line on a
/// drawing other than this one.
pub fn section_plane(
    parent: &ViewBasis,
    from: [f64; 2],
    to: [f64; 2],
    flip: bool,
) -> Option<CutPlane> {
    let (_, n2) = cut_line_2d(from, to, flip)?;
    let normal = [
        n2[0] * parent.u[0] + n2[1] * parent.v[0],
        n2[0] * parent.u[1] + n2[1] * parent.v[1],
        n2[0] * parent.u[2] + n2[1] * parent.v[2],
    ];
    let origin = [
        parent.origin[0] + from[0] * parent.u[0] + from[1] * parent.v[0],
        parent.origin[1] + from[0] * parent.u[1] + from[1] * parent.v[1],
        parent.origin[2] + from[0] * parent.u[2] + from[1] * parent.v[2],
    ];
    Some(CutPlane {
        origin,
        normal,
        normal_in_parent: n2,
    })
}

/// The frame a section view projects with: the line of sight is the NEGATED
/// cut normal, so the viewer stands on the discarded side and looks at the cap
/// with the kept material behind it — the same convention
/// [`SectionResult::plane_basis`] states, and the one that makes an outer cap
/// loop's signed area positive.
///
/// The paper up is the parent's up with the sight direction projected out —
/// the direction in the section's view plane CLOSEST to the parent's up, so a
/// side section shares its vertical axis with the view it was cut on, which is
/// what makes the pair readable as one group. When the cut normal is parallel
/// to the parent's up that projection vanishes (a horizontal cutting line),
/// and the fallback is the parent's own line of sight signed the way
/// [`projected_frame`]'s `Up`/`Down` rows sign it — so a horizontal cut on a
/// front view comes out as exactly the top or bottom view it ought to be,
/// rather than as one of them turned over.
pub fn section_frame(parent: &ViewBasis, cut: &CutPlane) -> ViewFrame {
    let d = neg(cut.normal);
    let t = dot3(parent.v, d) / dot3(d, d).max(f64::MIN_POSITIVE);
    let up0 = [
        parent.v[0] - t * d[0],
        parent.v[1] - t * d[1],
        parent.v[2] - t * d[2],
    ];
    let len = dot3(up0, up0).sqrt();
    // The threshold is on a UNIT vector's component, so it is a pure angle:
    // below it the parent's up is within ~6e-5 rad of the line of sight and
    // Gram-Schmidt would amplify the authored line's own rounding into the
    // paper orientation.
    let up = if len > 1e-4 {
        up0
    } else if dot3(cut.normal, parent.v) > 0.0 {
        parent.w
    } else {
        neg(parent.w)
    };
    ViewFrame {
        origin: cut.origin,
        dir: d,
        up,
    }
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Where a section view goes on the paper relative to its parent, as a unit
/// step in sheet millimetres.
///
/// A section follows the SAME standard as any other projected view: third
/// angle places a view on the side it is viewed FROM, so the section sits
/// against the arrows; first angle places it on the side the arrows point to.
/// Deriving it from the one rule rather than stating it twice is why
/// [`projected_frame`] is a table read forwards or backwards.
pub fn section_paper_step(cut: &CutPlane, angle: ProjectionAngle) -> [f64; 2] {
    let n = cut.normal_in_parent;
    match angle {
        ProjectionAngle::Third => n,
        ProjectionAngle::First => [-n[0], -n[1]],
    }
}

/// Where a projected view's centre goes, in sheet millimetres: clear of the
/// parent's drawn extent, clear of its own, plus `gap_mm` between them.
///
/// The two half-extents are what makes the gap the gap BETWEEN the drawings
/// rather than between their centres — two views of a long part placed with a
/// fixed centre distance overlap.
pub fn auto_placement_mm(
    parent_centre_mm: [f64; 2],
    parent_extent_mm: [f64; 2],
    own_extent_mm: [f64; 2],
    direction: ProjectedDirection,
    gap_mm: f64,
) -> [f64; 2] {
    auto_placement_step_mm(
        parent_centre_mm,
        parent_extent_mm,
        own_extent_mm,
        direction.paper_step(),
        gap_mm,
    )
}

/// [`auto_placement_mm`] for a step that is not one of the four
/// (`specs/drawings_and_mbd.md` §8, D4b): a section's cut normal, which an
/// oblique cutting line makes oblique.
///
/// The clearance is measured along the step through the two boxes' SUPPORT
/// (`|½w·sx| + |½h·sy|`), which for an axis step is exactly the half-extent
/// the four-way form uses and for an oblique one is the smallest offset that
/// still clears both boxes. A box's support function is the honest answer
/// here: taking the half-extent of whichever axis dominates would overlap the
/// corner a 45° section is placed toward.
pub fn auto_placement_step_mm(
    parent_centre_mm: [f64; 2],
    parent_extent_mm: [f64; 2],
    own_extent_mm: [f64; 2],
    step: [f64; 2],
    gap_mm: f64,
) -> [f64; 2] {
    let len = step[0].hypot(step[1]);
    if !(len.is_finite() && len > 0.0) {
        return parent_centre_mm;
    }
    let s = [step[0] / len, step[1] / len];
    let support = |e: [f64; 2]| 0.5 * (e[0] * s[0]).abs() + 0.5 * (e[1] * s[1]).abs();
    let reach = support(parent_extent_mm) + support(own_extent_mm) + gap_mm;
    [
        parent_centre_mm[0] + s[0] * reach,
        parent_centre_mm[1] + s[1] * reach,
    ]
}

/// The gap a freshly added projected view leaves between its drawing and its
/// parent's: 15 mm, about a dimension line's clearance plus its text at the
/// ISO 3098 default height, so an added view does not land on top of the
/// parent's own dimensions.
pub const DEFAULT_VIEW_GAP_MM: f64 = 15.0;

// ----------------------------------------------------------------- rebuild

/// What an anchor looks like on the drawing — the arms of
/// [`LayoutCurve`](waffle_types::annotation::layout::LayoutCurve), which is
/// what the projection made of the entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum AnchorShape {
    /// A line seen end-on, or a vertex: one point.
    Point,
    /// A straight edge — what a linear dimension measures between.
    Line,
    /// A circular rim seen square on; `radius` is its radius.
    Circle,
    /// A circular rim seen obliquely; `radius` is its MAJOR radius, which is
    /// the hole's true radius.
    Ellipse,
    /// A sampled curve. It has no witness point, so a dimension on one
    /// refuses — a polyline's midpoint moves with the chord tolerance that
    /// sampled it.
    Polyline,
}

/// One entity a view drew, as an annotation can anchor on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ViewAnchor {
    /// The persistent id (D0) — what `Selector::Pid` stores.
    ///
    /// A decimal STRING on the wire; see [`waffle_types::pid_str`] for the
    /// measurement that forced it and for why it is one rule rather than one
    /// per boundary. (This field and `DrawingAnchorSpec::pid` were D4a's
    /// local `pid_string` module; that module is now the shared one.)
    #[serde(with = "waffle_types::pid_str")]
    #[cfg_attr(feature = "json-schema", schemars(with = "String"))]
    pub pid: u64,
    /// What the anchor IS on the drawing — a corner, a straight edge, a rim.
    ///
    /// Without it an anchor list is not usable for picking: the plate's top
    /// view offers eight anchors, four walls and four corners, and a caller
    /// told only their witness points cannot tell which pair is the two
    /// parallel walls a width dimension measures. (It tried: the test that
    /// found this picked two corners and the dimension measured the
    /// diagonal.) `kind` says Edge or Vertex in the MODEL; this says what the
    /// projection made of it.
    pub shape: AnchorShape,
    pub kind: TopoKind,
    /// The one point a dimension measures from, in view-plane meters, or
    /// `None` for a sampled polyline (which has no canonical witness — see
    /// `LayoutCurve::witness_point`). Also what a UI hit-tests a click
    /// against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<[f64; 2]>,
    /// The radius, for a circle or an ellipse — so a caller can tell a rim
    /// from an edge before dimensioning it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
}

/// What one view's rebuild produced.
#[derive(Debug, Clone)]
pub struct ViewRebuild {
    /// The record the app draws.
    pub layout: ViewLayout,
    /// What the projection declined to decide — carried out of the rebuild
    /// for the same reason the DXF export carries it (D1c): the counts are
    /// what tell a decided drawing from a quiet one.
    pub declines: ProjectionDeclines,
    /// The view's drawn extent in sheet millimetres, `[width, height]`, after
    /// the view scale — what the auto-layout places and what a sheet-bounds
    /// check measures. `[0, 0]` when the view drew nothing.
    pub extent_mm: [f64; 2],
    /// The same curves `layout` carries, kernel-side: analytic
    /// [`Curve2`](waffle_types::kernel::projection::Curve2)s in view-plane
    /// coordinates, style filter already applied.
    ///
    /// Not a second source of truth but the same one in the other
    /// representation: the DXF writer takes `Curve2` (so an arc reaches the
    /// file as an `ARC` rather than as a chord polyline) while the renderer
    /// takes the serde-able twin. Both come from this one filtered set, so a
    /// sheet and a screen cannot disagree about which lines are on it.
    pub geometry: ViewGeometry,
    /// The entities this view DREW, each with the persistent id an annotation
    /// anchors on.
    ///
    /// Deliberately NOT part of [`ViewLayout`]: the layout record carries no
    /// model reference at all, which is what makes a renderer holding one
    /// unable to draw a value other than the measured one (D3, asserted on
    /// the schema's `$ref` closure). This list is the other half of that
    /// arrangement — the AUTHORING path. Picking an edge on a sheet to
    /// dimension it needs the edge's id, and a UI that had to ask the kernel
    /// for one per click would be reaching past the engine.
    ///
    /// A pid that names more than one drawn curve is absent: an annotation
    /// must not be offered an anchor that would then refuse as ambiguous.
    pub anchors: Vec<ViewAnchor>,
    /// The annotations that could NOT be resolved or measured, by their index
    /// in the view, each with the reason.
    ///
    /// An annotation's failure is not the VIEW's failure, which is why these
    /// come back beside a layout rather than instead of one. A dimension
    /// whose entity the model no longer has is exactly the case D0's
    /// never-rebinding pid exists to make loud — and blanking the whole view
    /// over it would take the other seven dimensions and every curve down
    /// with it, leaving a sheet of eight views with one missing and no
    /// drawing where the information was. The view draws, the annotations
    /// that resolved are on it, and the ones that did not are named.
    pub annotation_errors: Vec<(usize, DrawingError)>,
}

/// Project `bodies` into `frame` and resolve `view`'s annotations against the
/// result — the whole of §8's "rebuild of a drawing tab", for one view.
///
/// `frame` comes from [`Sheet::view_frame`] (so a `ProjectedFrom` chain is
/// already followed) and `bodies` from the source tab's rebuild, filtered by
/// [`ViewSource::includes`]. Keeping both as arguments is what lets this
/// function be pure with respect to the document: it does not reach into
/// another tab, and a test can hand it one body.
/// `exprs` evaluates a `Measured::Expr` dimension (D2). `None` is a caller
/// with no expression environment — such a dimension then refuses by name
/// rather than silently falling back to what its anchors measure, which is a
/// different number.
pub fn rebuild_view(
    view: &DrawingView,
    frame: &ViewFrame,
    bodies: &[ProjectionBody],
    kernel: &dyn KernelBundle,
    exprs: Option<&dyn ExprDimensions>,
) -> Result<ViewRebuild, DrawingError> {
    rebuild_view_in(view, frame, bodies, &ViewExtras::default(), kernel, exprs)
}

/// What a view draws BESIDES its own projection (`specs/drawings_and_mbd.md`
/// §8, D4b) — everything the view cannot derive from its own frame and bodies.
///
/// Why it is an argument rather than something [`rebuild_view_in`] works out:
/// a section's cap comes from [`KernelProjection::section_with_plane`], which
/// takes `&mut` (it makes a new body in the arena) where this function takes
/// `&` and makes none, and a parent's marks come from the OTHER views of the
/// sheet, which this function deliberately cannot see — keeping it pure with
/// respect to the document is what lets a test hand it one body.
#[derive(Debug, Clone, Default)]
pub struct ViewExtras {
    /// The cap regions to hatch, already in THIS view's `(u, v)` — see
    /// [`cap_loops_in_view`].
    pub hatch: Vec<HatchLoop>,
    /// What to mark on this view because other views derive from it
    /// ([`Sheet::marks_on`]).
    pub marks: Vec<ViewMark>,
    /// A detail view's crop disc, in this view's `(u, v)`.
    pub clip: Option<ClipCircle>,
}

/// [`rebuild_view`] with a section's hatch, a parent's marks and a detail's
/// crop (D4b).
///
/// `exprs` is [`rebuild_view`]'s own (D2): a view carrying a `Measured::Expr`
/// dimension needs its source tab's expression environment whether or not it
/// is a section, so the parameter rides through here rather than being
/// defaulted away for the D4b path.
pub fn rebuild_view_in(
    view: &DrawingView,
    frame: &ViewFrame,
    bodies: &[ProjectionBody],
    extras: &ViewExtras,
    kernel: &dyn KernelBundle,
    exprs: Option<&dyn ExprDimensions>,
) -> Result<ViewRebuild, DrawingError> {
    if !(view.scale.is_finite() && view.scale > 0.0) {
        return Err(DrawingError::BadScale {
            view: view.id,
            scale: view.scale,
        });
    }
    let basis = frame.basis().ok_or(DrawingError::DegenerateFrame {
        view: view.id,
        dir: frame.dir,
        up: frame.up,
    })?;

    // A view of no bodies is not asked of the kernel. Its answer is already
    // determined — nothing projects to nothing — so the only thing the call
    // could add is a `NotSupported` from a kernel that cannot project, which
    // says nothing about THIS view. A freshly added view of a tab with
    // nothing built yet is the ordinary case, and reporting it as a
    // projection failure would hide the real one.
    let geometry = if bodies.is_empty() {
        ViewGeometry::default()
    } else {
        kernel
            .project_bodies(bodies, frame, &ProjectOpts::default())
            .map_err(|e| DrawingError::ProjectionFailed {
                view: view.id,
                message: e.to_string(),
            })?
    };

    // The curves this view DRAWS, as a view of their own. One filtered
    // `ViewGeometry` rather than an index list, because it is also what the
    // sheet's DXF export places in paper space — the layout record carries
    // the serde-able twins of these curves and the writer needs the analytic
    // originals, so the two must come from the same filtered set or the
    // sheet and the screen disagree about which lines are on it. Its box is
    // recomputed by `with_declines`, exactly (`Curve2::bbox` is exact for an
    // arc, where a corner-of-the-whole-conic estimate is not).
    let mut drawn = ViewGeometry::with_declines(
        geometry
            .curves
            .iter()
            .filter(|c| view.style.draws(c.kind, c.visibility))
            .cloned()
            .collect(),
        geometry.declines,
    );

    // A detail view's crop (D4b). Two things happen here and both matter:
    //
    // - every curve that cannot REACH the disc is dropped, by its exact
    //   bounding box against the disc's. A box test rather than a disc test
    //   on purpose: it never drops a curve that does reach the disc, and
    //   whatever it keeps the renderer then clips exactly. The point of the
    //   cull is that a detail of a corner does not carry the whole part's
    //   thousands of edges into the layout, not that it carries exactly the
    //   right ones.
    // - the view's BOX becomes the disc's box, so the detail is centred and
    //   sized on the crop rather than on whatever happened to survive the
    //   cull. Without it a detail of an empty corner would be laid out around
    //   the one edge that reached it.
    if let Some(circle) = extras.clip {
        if !(circle.radius.is_finite() && circle.radius > 0.0) {
            return Err(DrawingError::BadCropRadius {
                view: view.id,
                radius: circle.radius,
            });
        }
        let crop = Aabb2::from_pairs(
            [
                circle.center[0] - circle.radius,
                circle.center[1] - circle.radius,
            ],
            [
                circle.center[0] + circle.radius,
                circle.center[1] + circle.radius,
            ],
        );
        let kept: Vec<_> = drawn
            .curves
            .iter()
            .filter(|c| c.geometry.bbox().intersects(crop))
            .cloned()
            .collect();
        drawn = ViewGeometry {
            curves: kept,
            bbox: Some(crop),
            declines: drawn.declines,
        };
    }

    // The pid → geometry index, over the drawn curves only: an annotation
    // must not measure a curve the style suppressed, because the dimension
    // would then point at nothing on the sheet.
    let anchors = AnchorIndex::build(&drawn, bodies, kernel);

    let mut layout = ViewLayout::from_view(&drawn);
    let mut resolved = Vec::with_capacity(view.annotations.len());
    let mut annotation_errors = Vec::new();
    for (index, annotation) in view.annotations.iter().enumerate() {
        match resolve_annotation(view.id, index, annotation, &anchors, &basis, exprs) {
            Ok(laid_out) => resolved.push(laid_out),
            Err(e) => annotation_errors.push((index, e)),
        }
    }
    layout.annotations = resolved;
    layout.hatch = extras.hatch.clone();
    layout.marks = extras.marks.clone();
    layout.clip = extras.clip;

    let extent_mm = match drawn.bbox {
        Some(b) => [
            (b.max.x() - b.min.x()) * 1000.0 * view.scale,
            (b.max.y() - b.min.y()) * 1000.0 * view.scale,
        ],
        None => [0.0, 0.0],
    };

    Ok(ViewRebuild {
        layout,
        declines: drawn.declines,
        extent_mm,
        geometry: drawn,
        anchors: anchors.offered(),
        annotation_errors,
    })
}

/// Every drawn curve of a view, indexed by the persistent id of the entity it
/// came from — the map D0 exists to make possible, and the only way an
/// annotation reaches geometry.
struct AnchorIndex {
    /// `(kind, pid)` → the drawn curves it names. More than one is an
    /// ambiguity, kept so the error can say how many.
    curves: HashMap<(TopoKind, u64), Vec<LayoutCurve>>,
    /// `pid` → the vertex's world position, for the point anchors a curve
    /// index cannot carry (a vertex projects to a point, not a curve).
    vertices: HashMap<u64, Vec<[f64; 3]>>,
}

impl AnchorIndex {
    fn build(
        geometry: &ViewGeometry,
        bodies: &[ProjectionBody],
        kernel: &dyn KernelBundle,
    ) -> AnchorIndex {
        let introspect = kernel.as_introspect();
        // `all_entity_pids` is the bulk form, and the trait says to prefer it:
        // deriving edge and vertex ids is a whole-body computation, so asking
        // per entity is quadratic in the body's size.
        let mut pid_of: HashMap<(TopoKind, u64), u64> = HashMap::new();
        let mut vertices: HashMap<u64, Vec<[f64; 3]>> = HashMap::new();
        for body in bodies {
            for kind in [TopoKind::Edge, TopoKind::Face, TopoKind::Vertex] {
                for (id, pid) in introspect.all_entity_pids(&body.handle, kind) {
                    pid_of.insert((kind, id.0), pid.pid);
                    if kind == TopoKind::Vertex {
                        if let Some(at) = vertex_position(id, kernel) {
                            vertices.entry(pid.pid).or_default().push(at);
                        }
                    }
                }
            }
        }

        let mut curves: HashMap<(TopoKind, u64), Vec<LayoutCurve>> = HashMap::new();
        for curve in &geometry.curves {
            let Some(source) = curve.source else { continue };
            let kind = match curve.kind {
                CurveKind::Edge => TopoKind::Edge,
                // A silhouette's `source` names the FACE it came off.
                CurveKind::Silhouette => TopoKind::Face,
                // A section outline belongs to no single entity.
                CurveKind::SectionOutline => continue,
            };
            let Some(pid) = pid_of.get(&(kind, source.0)) else {
                continue;
            };
            curves
                .entry((kind, *pid))
                .or_default()
                .push(LayoutCurve::from_curve2(&curve.geometry));
        }
        AnchorIndex { curves, vertices }
    }

    /// The anchors a caller may author on: every pid that names exactly ONE
    /// drawn entity. An ambiguous one is left out rather than offered and
    /// then refused.
    fn offered(&self) -> Vec<ViewAnchor> {
        let mut out: Vec<ViewAnchor> = self
            .curves
            .iter()
            .filter_map(|((kind, pid), curves)| {
                let [curve] = curves.as_slice() else {
                    return None;
                };
                Some(ViewAnchor {
                    pid: *pid,
                    kind: *kind,
                    shape: shape_of(curve),
                    at: curve.witness_point(),
                    radius: curve.radius(),
                })
            })
            .chain(self.vertices.iter().filter_map(|(pid, at)| {
                let [_] = at.as_slice() else { return None };
                // The projected position is the curve index's business; a
                // vertex's own is resolved per anchor, so `at` is left out
                // rather than projected a second way here.
                Some(ViewAnchor {
                    pid: *pid,
                    kind: TopoKind::Vertex,
                    shape: AnchorShape::Point,
                    at: None,
                    radius: None,
                })
            }))
            .collect();
        // Deterministic: a tool answer and a UI list must not depend on hash
        // iteration order.
        out.sort_by_key(|a| (a.kind as u8, a.pid));
        out
    }
}

/// What the projection made of a curve.
fn shape_of(curve: &LayoutCurve) -> AnchorShape {
    match curve {
        LayoutCurve::Point { .. } => AnchorShape::Point,
        LayoutCurve::Line { .. } => AnchorShape::Line,
        LayoutCurve::Circle { .. } => AnchorShape::Circle,
        LayoutCurve::Ellipse { .. } => AnchorShape::Ellipse,
        LayoutCurve::Polyline { .. } => AnchorShape::Polyline,
    }
}

/// A vertex's world position: its signature's centroid, which is what the
/// kernel contract's own `edge_polyline` default uses for the same question.
fn vertex_position(
    vertex: waffle_types::kernel::KernelId,
    kernel: &dyn KernelBundle,
) -> Option<[f64; 3]> {
    kernel
        .as_introspect()
        .compute_signature(vertex, TopoKind::Vertex)
        .centroid
}

/// Resolve and measure one annotation.
fn resolve_annotation(
    view: Uuid,
    index: usize,
    annotation: &Annotation,
    anchors: &AnchorIndex,
    basis: &ViewBasis,
    exprs: Option<&dyn ExprDimensions>,
) -> Result<AnnotationLayout, DrawingError> {
    match annotation {
        Annotation::Dimension {
            kind,
            anchors: refs,
            value,
            tolerance,
            precision,
            dual_unit,
            dual_precision,
            placement,
        } => {
            check_measured(view, index, value)?;
            // D2: an EXPRESSION dimension's number comes from the
            // expression, not from the anchors — that is what it is for — and
            // it is evaluated FIRST, before the anchors are resolved.
            //
            // The order is a choice. The alternative, anchors first, hides a
            // broken expression behind a missing anchor whenever both are
            // wrong, and an expression that cannot be evaluated is a defect
            // in what the dimension SAYS, where the anchors are only where
            // it is drawn. Reporting the value first names the thing the
            // author has to decide about. The anchors are still resolved
            // below and a vanished one is still loud; only one error per
            // annotation is reported either way.
            let from_expr = match value {
                Measured::Expr { expr } => {
                    let Some(values) = exprs else {
                        return Err(DrawingError::ExprNotEvaluated {
                            view,
                            index,
                            expr: expr.clone(),
                        });
                    };
                    Some(values.value_of(expr, *kind).map_err(|reason| {
                        DrawingError::ExprFailed {
                            view,
                            index,
                            expr: expr.clone(),
                            reason,
                        }
                    })?)
                }
                _ => None,
            };
            let resolved = refs
                .iter()
                .map(|r| resolve_anchor(view, index, r, anchors, basis))
                .collect::<Result<Vec<_>, _>>()?;
            let measured = match from_expr {
                Some(value) => value,
                None => {
                    measure(*kind, &resolved).map_err(|source| DrawingError::NotMeasurable {
                        view,
                        index,
                        source,
                    })?
                }
            };
            // M1: the tolerance is resolved HERE, against the measured
            // nominal, because an ISO 286 fit is only two numbers once it
            // has a size. A tolerance the dimension cannot carry — an
            // angular band on a linear dimension, a fit on a radius — is a
            // typed error naming the annotation, not a tolerance silently
            // dropped from the sheet.
            let tolerance = match tolerance {
                None => None,
                Some(t) => Some(ToleranceLayout::resolve(t, *kind, measured).map_err(
                    |source| DrawingError::ToleranceRefused {
                        view,
                        index,
                        reason: source.to_string(),
                    },
                )?),
            };
            Ok(AnnotationLayout::Dimension {
                kind: *kind,
                anchors: resolved,
                value: measured,
                tolerance,
                precision: *precision,
                dual_unit: dual_unit.clone(),
                dual_precision: *dual_precision,
                placement: *placement,
            })
        }
        Annotation::Note {
            text,
            leader,
            placement,
        } => {
            let leader = match leader {
                None => None,
                Some(r) => Some(resolve_anchor(view, index, r, anchors, basis)?),
            };
            Ok(AnnotationLayout::Note {
                text: text.clone(),
                leader,
                placement: *placement,
            })
        }
        Annotation::CentreMark { anchor } => {
            let resolved = resolve_anchor(view, index, anchor, anchors, basis)?;
            let at = resolved
                .witness_point()
                .ok_or(DrawingError::NotMeasurable {
                    view,
                    index,
                    source: MeasureError::NotMeasurable {
                        kind: "CentreMark",
                        why: "a sampled polyline has no centre",
                    },
                })?;
            // The cross is sized from the marked entity, per
            // `AnnotationLayout::CentreMark::half_size`: a radius plus a
            // fifth of it of overshoot, which is the ISO 128 look, and a
            // tiny hole gets a tiny cross.
            let half_size = resolved
                .as_curve()
                .and_then(LayoutCurve::radius)
                .map(|r| r * 1.2)
                .ok_or(DrawingError::NotMeasurable {
                    view,
                    index,
                    source: MeasureError::NotMeasurable {
                        kind: "CentreMark",
                        why: "only a circle, arc or ellipse has a centre to mark",
                    },
                })?;
            Ok(AnnotationLayout::CentreMark { at, half_size })
        }
        Annotation::CentreLine { anchors: refs } => {
            let a = resolve_anchor(view, index, &refs[0], anchors, basis)?;
            let b = resolve_anchor(view, index, &refs[1], anchors, basis)?;
            let (from, to) = match (a.witness_point(), b.witness_point()) {
                (Some(from), Some(to)) => (from, to),
                _ => {
                    return Err(DrawingError::NotMeasurable {
                        view,
                        index,
                        source: MeasureError::NotMeasurable {
                            kind: "CentreLine",
                            why: "a sampled polyline has no witness point",
                        },
                    })
                }
            };
            Ok(AnnotationLayout::CentreLine { from, to })
        }
        Annotation::Datum {
            label,
            anchor,
            placement,
        } => Ok(AnnotationLayout::Datum {
            label: label.clone(),
            anchor: resolve_anchor(view, index, anchor, anchors, basis)?,
            placement: *placement,
        }),
        // M1. The frame's content needs no measurement — a geometric
        // tolerance is authored, not derived — but it IS validated here, so
        // a frame that ISO 1101 calls malformed (flatness with a datum,
        // perpendicularity without one) is refused at the rebuild rather
        // than printed and exported.
        Annotation::FeatureControlFrame {
            tolerance,
            anchor,
            placement,
        } => {
            tolerance
                .validate()
                .map_err(|source| DrawingError::ToleranceRefused {
                    view,
                    index,
                    reason: source.to_string(),
                })?;
            Ok(AnnotationLayout::FeatureControlFrame {
                tolerance: tolerance.clone(),
                anchor: resolve_anchor(view, index, anchor, anchors, basis)?,
                placement: *placement,
            })
        }
    }
}

/// The boundary §7's "nothing refuses `Measured::Value`" names: a dimension
/// whose number did not come from the model is refused here, before anything
/// draws it.
///
/// Called by the rebuild AND by the authoring tools, so a literal cannot
/// enter a document in the first place. `Measured::Expr` passes: since D2 it
/// is a legal authored value, and whether it EVALUATES is the rebuild's
/// question, not the authoring boundary's.
pub fn check_measured(view: Uuid, index: usize, value: &Measured) -> Result<(), DrawingError> {
    match value {
        Measured::FromGeometry | Measured::Expr { .. } => Ok(()),
        Measured::Value { value } => Err(DrawingError::LiteralValue {
            view,
            index,
            value: *value,
        }),
    }
}

/// How a `Measured::Expr` dimension's value is obtained (D2).
///
/// A trait rather than the environment itself, because the environment a
/// measuring expression needs is the parameter table AND the live kernel
/// (`crate::measure::TreeMeasurer`), and `rebuild_view` is deliberately pure
/// with respect to the document — it does not reach into another tab. The
/// caller that HAS both builds one of these; a caller that has neither
/// passes `None` and such a dimension refuses, loudly, as it did before D2.
pub trait ExprDimensions {
    /// The value of `expression` for a dimension of `kind`, in the unit the
    /// layout carries: **meters** for every length kind, **radians** for
    /// [`DimensionKind::Angle`] — the same units
    /// `waffle_types::annotation::measure` produces, because the two feed
    /// one field and a renderer must not have to ask which it got.
    fn value_of(&self, expression: &str, kind: DimensionKind) -> Result<f64, String>;
}

/// One anchor's geometry in the view plane, or a typed refusal naming it.
fn resolve_anchor(
    view: Uuid,
    index: usize,
    geom_ref: &GeomRef,
    anchors: &AnchorIndex,
    basis: &ViewBasis,
) -> Result<AnchorGeometry, DrawingError> {
    let Selector::Pid { pid, .. } = &geom_ref.selector else {
        return Err(DrawingError::AnchorNotPid {
            view,
            index,
            selector: selector_tag(&geom_ref.selector),
        });
    };
    let kind = geom_ref.kind;
    if kind == TopoKind::Vertex {
        let found = anchors.vertices.get(pid).map(Vec::as_slice).unwrap_or(&[]);
        return match found {
            [] => Err(DrawingError::AnchorUnresolved {
                view,
                index,
                kind,
                pid: *pid,
            }),
            [at] => {
                let (p, _depth) = basis.project(*at);
                Ok(AnchorGeometry::point([p.x(), p.y()]))
            }
            many => Err(DrawingError::AnchorAmbiguous {
                view,
                index,
                kind,
                pid: *pid,
                count: many.len(),
            }),
        };
    }
    let found = anchors
        .curves
        .get(&(kind, *pid))
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    match found {
        [] => Err(DrawingError::AnchorUnresolved {
            view,
            index,
            kind,
            pid: *pid,
        }),
        [one] => Ok(AnchorGeometry::curve(one.clone())),
        many => Err(DrawingError::AnchorAmbiguous {
            view,
            index,
            kind,
            pid: *pid,
            count: many.len(),
        }),
    }
}

fn selector_tag(selector: &Selector) -> &'static str {
    match selector {
        Selector::Role { .. } => "Selector::Role",
        Selector::Signature { .. } => "Selector::Signature",
        Selector::Query { .. } => "Selector::Query",
        Selector::Position { .. } => "Selector::Position",
        Selector::Pid { .. } => "Selector::Pid",
    }
}

/// The name of an annotation's variant, for an error that has to say which
/// one it could not handle.
pub fn annotation_tag(annotation: &Annotation) -> &'static str {
    match annotation {
        Annotation::Dimension { .. } => "Dimension",
        Annotation::Note { .. } => "Note",
        Annotation::CentreMark { .. } => "CentreMark",
        Annotation::CentreLine { .. } => "CentreLine",
        Annotation::Datum { .. } => "Datum",
        Annotation::FeatureControlFrame { .. } => "FeatureControlFrame",
    }
}

/// The dimension kinds a tool may author, with their tags — the translation
/// §7's `DimensionKind` doc comment says is the UI's.
pub fn dimension_kind_from_tag(tag: &str) -> Option<DimensionKind> {
    Some(match tag {
        "Distance" => DimensionKind::Distance,
        "PointLineDistance" => DimensionKind::PointLineDistance,
        "HDistance" => DimensionKind::HDistance,
        "VDistance" => DimensionKind::VDistance,
        "Angle" => DimensionKind::Angle,
        "Radius" => DimensionKind::Radius,
        "Diameter" => DimensionKind::Diameter,
        _ => return None,
    })
}

// ------------------------------------------------------- the cap, in the view

/// A section cap's loops, re-expressed in the SECTION VIEW's `(u, v)` and
/// ready to hatch (`specs/drawings_and_mbd.md` §8, D4b).
///
/// The kernel reports a cap in the cut plane's own frame, which it derives
/// from the normal alone — `plane_basis`, whose paper up is whatever
/// `looking_along` picked. The section VIEW's up is chosen to agree with the
/// parent it was cut on ([`section_frame`]). Both frames share the line of
/// sight and the handedness, so the map between them is one planar rotation
/// plus a translation, and this applies it.
///
/// A loop whose curves cannot be mapped is DROPPED and counted, not silently
/// half-drawn: a region missing one of its boundary curves is not a region,
/// and hatching it would run the lines out through the gap.
pub fn cap_loops_in_view(
    cap: &[SectionLoop],
    cap_basis: &ViewBasis,
    view: &ViewBasis,
) -> (Vec<HatchLoop>, usize) {
    // cos/sin of the rotation taking cap coordinates to view coordinates:
    // where the cap frame's own `u` axis lands in the view's.
    let rot = [dot3(cap_basis.u, view.u), dot3(cap_basis.u, view.v)];
    let d = [
        cap_basis.origin[0] - view.origin[0],
        cap_basis.origin[1] - view.origin[1],
        cap_basis.origin[2] - view.origin[2],
    ];
    let offset = [dot3(d, view.u), dot3(d, view.v)];

    let mut out = Vec::with_capacity(cap.len());
    let mut dropped = 0usize;
    for loop_ in cap {
        let mut curves = Vec::with_capacity(loop_.curves.len());
        let mut ok = true;
        for c in &loop_.curves {
            match c.transformed_by(1.0, rot, offset) {
                Some(moved) => curves.push(LayoutCurve::from_curve2(&moved)),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok || curves.is_empty() {
            dropped += 1;
            continue;
        }
        out.push(HatchLoop {
            curves,
            // The kernel measured the direction while it still had the B-Rep
            // walk; a negative area is a hole. Zero is neither, and a
            // zero-area loop is a degenerate cap region rather than an outer
            // boundary — counted out rather than filled.
            hole: loop_.signed_area < 0.0,
            exact: loop_.exact,
        });
    }
    (out, dropped)
}

// ------------------------------------------------------------- cache validity

/// What a view's persisted layout was built from (`specs/drawings_and_mbd.md`
/// §8, D4b) — the inputs a [`view_cache_key`] covers.
///
/// Three digests, each of something the engine can recompute:
///
/// - `sheet_recipe` — every view of the sheet, with its caches taken out, plus
///   the drawing's projection standard. The whole SHEET rather than the one
///   view, because a projected, section or detail view's frame is derived from
///   its parent's: a key over the view alone would read as valid after the
///   parent was re-aimed. It over-covers (editing one view invalidates the
///   sheet's other keys) and that is the safe direction — every view is
///   rebuilt on every evaluation anyway, so the cost is a reader being told
///   "stale" more often than it strictly had to be, where under-covering
///   would mean drawing last week's sheet with no sign of it.
/// - `source_recipe` — the feature tree the source tab builds its bodies
///   from. A rebuild of the part changes the drawing.
/// - `body_pids` — the persistent ids of the bodies actually drawn
///   ([`body_pid_digest`]). The recipe digest cannot see a body whose
///   identity changed without its recipe changing (a linked source rebuilt
///   elsewhere, an imported body re-imported), and a pid is exactly what an
///   annotation anchors on.
#[derive(Debug, Clone, PartialEq)]
pub struct CacheInputs {
    pub sheet_recipe: String,
    pub source_recipe: String,
    pub body_pids: String,
}

impl CacheInputs {
    /// The sheet-wide half, which every view of the sheet shares.
    pub fn for_sheet(sheet: &Sheet, angle: ProjectionAngle) -> CacheInputs {
        let mut parts = String::new();
        parts.push_str(match angle {
            ProjectionAngle::Third => "third|",
            ProjectionAngle::First => "first|",
        });
        parts.push_str(&sheet.id.to_string());
        for view in &sheet.views {
            parts.push('|');
            parts.push_str(&view.recipe());
        }
        CacheInputs {
            sheet_recipe: digest_hex(parts.as_bytes()),
            source_recipe: String::new(),
            body_pids: String::new(),
        }
    }

    /// This, with the source tab's recipe and the drawn bodies' pids.
    pub fn with_source(mut self, source_recipe: &str, body_pids: &str) -> CacheInputs {
        self.source_recipe = digest_hex(source_recipe.as_bytes());
        self.body_pids = body_pids.to_string();
        self
    }
}

/// The validity key of one view's cache.
pub fn view_cache_key(view: Uuid, inputs: &CacheInputs) -> String {
    let joined = format!(
        "{view}|{}|{}|{}",
        inputs.sheet_recipe, inputs.source_recipe, inputs.body_pids
    );
    format!("d4b-{}", digest_hex(joined.as_bytes()))
}

/// The persistent ids of every entity of `bodies`, as one digest.
///
/// In the bulk form the trait asks for, and sorted per body before hashing:
/// `all_entity_pids` makes no ordering promise, and a key that changed with
/// the kernel's traversal order would report every cache stale on every
/// rebuild — which is indistinguishable from having no key.
pub fn body_pid_digest(bodies: &[ProjectionBody], kernel: &dyn KernelBundle) -> String {
    let introspect = kernel.as_introspect();
    let mut acc: u64 = FNV_OFFSET;
    for body in bodies {
        acc = fnv_bytes(acc, body.name.as_bytes());
        for kind in [TopoKind::Edge, TopoKind::Face, TopoKind::Vertex] {
            let mut pids: Vec<u64> = introspect
                .all_entity_pids(&body.handle, kind)
                .into_iter()
                .map(|(_, pid)| pid.pid)
                .collect();
            pids.sort_unstable();
            for pid in pids {
                acc = fnv_bytes(acc, &pid.to_le_bytes());
            }
        }
    }
    format!("{acc:016x}")
}

/// FNV-1a 64 of `bytes`, as lowercase hex.
///
/// Not `DefaultHasher`: the key is PERSISTED, and std's hasher is documented
/// as not stable between releases, so a pinned key would drift on a toolchain
/// bump and every cache in every saved document would read as stale at once.
/// FNV-1a is specified, trivially re-implementable, and stable by definition —
/// which is the property a stored key needs. It is not a cryptographic hash
/// and does not need to be: nothing here defends against a chosen collision,
/// it detects a document that changed.
pub fn digest_hex(bytes: &[u8]) -> String {
    format!("{:016x}", fnv_bytes(FNV_OFFSET, bytes))
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv_bytes(mut acc: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        acc ^= u64::from(*b);
        acc = acc.wrapping_mul(FNV_PRIME);
    }
    acc
}

#[cfg(test)]
mod tests;
