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
use waffle_types::annotation::layout::{AnchorGeometry, AnnotationLayout, LayoutCurve, ViewLayout};
use waffle_types::annotation::measure::{measure, MeasureError};
use waffle_types::annotation::{Annotation, DimensionKind, Measured};
use waffle_types::geom_ref::{GeomRef, Selector};
use waffle_types::kernel::projection::{
    CurveKind, ProjectOpts, ProjectionBody, ProjectionDeclines, ViewBasis, ViewFrame, ViewGeometry,
    Visibility,
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
    /// `Measured::Expr` needs D2's measurement functions in the expression
    /// environment, which do not exist yet. Named rather than silently
    /// measured from geometry, because those are different numbers.
    #[error(
        "annotation {index} of view {view} measures the expression `{expr}`, which needs the \
         measurement functions (D2); it is not evaluated yet"
    )]
    ExprNotEvaluated {
        view: Uuid,
        index: usize,
        expr: String,
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
                if let Projection::ProjectedFrom { parent, .. } = &view.projection {
                    if !view_ids.contains(parent) {
                        out.push(format!(
                            "view {} is projected from {}, which is not on sheet {}",
                            view.id, parent, sheet.id
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
                if self.view(*parent).is_none() {
                    return Err(DrawingError::UnknownParent {
                        view,
                        parent: *parent,
                    });
                }
                let parent_frame = self.view_frame_inner(*parent, angle, chain)?;
                let basis = parent_frame.basis().ok_or(DrawingError::DegenerateFrame {
                    view: *parent,
                    dir: parent_frame.dir,
                    up: parent_frame.up,
                })?;
                projected_frame(&basis, *direction, angle)
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
            extra: Map::new(),
        }
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
///
/// `Section` and `Detail` (§8) are D4b's and are deliberately absent rather
/// than present-and-refusing: a serde-tagged variant that exists cannot be
/// removed, and one that exists but cannot be rebuilt is a document a user can
/// author and the engine cannot draw.
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
    let step = direction.paper_step();
    let axis = usize::from(step[1] != 0.0);
    let reach = 0.5 * parent_extent_mm[axis] + 0.5 * own_extent_mm[axis] + gap_mm;
    let mut out = parent_centre_mm;
    out[axis] += step[axis] * reach;
    out
}

/// The gap a freshly added projected view leaves between its drawing and its
/// parent's: 15 mm, about a dimension line's clearance plus its text at the
/// ISO 3098 default height, so an added view does not land on top of the
/// parent's own dimensions.
pub const DEFAULT_VIEW_GAP_MM: f64 = 15.0;

// ----------------------------------------------------------------- rebuild

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
}

/// Project `bodies` into `frame` and resolve `view`'s annotations against the
/// result — the whole of §8's "rebuild of a drawing tab", for one view.
///
/// `frame` comes from [`Sheet::view_frame`] (so a `ProjectedFrom` chain is
/// already followed) and `bodies` from the source tab's rebuild, filtered by
/// [`ViewSource::includes`]. Keeping both as arguments is what lets this
/// function be pure with respect to the document: it does not reach into
/// another tab, and a test can hand it one body.
pub fn rebuild_view(
    view: &DrawingView,
    frame: &ViewFrame,
    bodies: &[ProjectionBody],
    kernel: &dyn KernelBundle,
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

    let drawn: Vec<usize> = (0..geometry.curves.len())
        .filter(|i| {
            let c = &geometry.curves[*i];
            view.style.draws(c.kind, c.visibility)
        })
        .collect();

    // The pid → geometry index, over the curves this view actually DRAWS: an
    // annotation must not measure a curve the style suppressed, because the
    // dimension would then point at nothing on the sheet.
    let anchors = AnchorIndex::build(&geometry, &drawn, bodies, kernel);

    let mut layout = ViewLayout {
        curves: drawn
            .iter()
            .map(|i| {
                waffle_types::annotation::layout::LayoutCurveEntry::from_projected(
                    &geometry.curves[*i],
                )
            })
            .collect(),
        bbox: None,
        annotations: Vec::new(),
    };
    layout.bbox = bbox_of(&layout);

    let mut resolved = Vec::with_capacity(view.annotations.len());
    for (index, annotation) in view.annotations.iter().enumerate() {
        resolved.push(resolve_annotation(
            view.id, index, annotation, &anchors, &basis, bodies, kernel,
        )?);
    }
    layout.annotations = resolved;

    let extent_mm = match layout.bbox {
        Some([min, max]) => [
            (max[0] - min[0]) * 1000.0 * view.scale,
            (max[1] - min[1]) * 1000.0 * view.scale,
        ],
        None => [0.0, 0.0],
    };

    Ok(ViewRebuild {
        layout,
        declines: geometry.declines,
        extent_mm,
    })
}

/// The bounding box of a layout's curves, in view-plane meters.
fn bbox_of(layout: &ViewLayout) -> Option<[[f64; 2]; 2]> {
    let mut out: Option<[[f64; 2]; 2]> = None;
    for entry in &layout.curves {
        for p in curve_extremes(&entry.geometry) {
            out = Some(match out {
                None => [p, p],
                Some([min, max]) => [
                    [min[0].min(p[0]), min[1].min(p[1])],
                    [max[0].max(p[0]), max[1].max(p[1])],
                ],
            });
        }
    }
    out
}

/// The corners of a layout curve's own box. Conic arms report the whole
/// conic's box rather than the arc's — an over-estimate by at most the
/// arc's own sagitta, and the alternative is re-deriving
/// `Curve2::bbox`'s exact arc arithmetic on the serde-able twin.
fn curve_extremes(curve: &LayoutCurve) -> Vec<[f64; 2]> {
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
        drawn: &[usize],
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
        for i in drawn {
            let curve = &geometry.curves[*i];
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
    _bodies: &[ProjectionBody],
    _kernel: &dyn KernelBundle,
) -> Result<AnnotationLayout, DrawingError> {
    match annotation {
        Annotation::Dimension {
            kind,
            anchors: refs,
            value,
            precision,
            dual_unit,
            placement,
        } => {
            check_measured(view, index, value)?;
            let resolved = refs
                .iter()
                .map(|r| resolve_anchor(view, index, r, anchors, basis))
                .collect::<Result<Vec<_>, _>>()?;
            let measured =
                measure(*kind, &resolved).map_err(|source| DrawingError::NotMeasurable {
                    view,
                    index,
                    source,
                })?;
            Ok(AnnotationLayout::Dimension {
                kind: *kind,
                anchors: resolved,
                value: measured,
                precision: *precision,
                dual_unit: dual_unit.clone(),
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
    }
}

/// The boundary §7's "nothing refuses `Measured::Value`" names: a dimension
/// whose number did not come from the model is refused here, before anything
/// draws it.
///
/// Called by the rebuild AND by the authoring tools, so a literal cannot
/// enter a document in the first place.
pub fn check_measured(view: Uuid, index: usize, value: &Measured) -> Result<(), DrawingError> {
    match value {
        Measured::FromGeometry => Ok(()),
        Measured::Value { value } => Err(DrawingError::LiteralValue {
            view,
            index,
            value: *value,
        }),
        Measured::Expr { expr } => Err(DrawingError::ExprNotEvaluated {
            view,
            index,
            expr: expr.clone(),
        }),
    }
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

#[cfg(test)]
mod tests;
