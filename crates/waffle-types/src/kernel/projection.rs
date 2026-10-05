//! The drawing-projection contract — `specs/drawings_and_mbd.md` §5.1, the
//! trait and types D1 is built against.
//!
//! A drawing view is an **orthographic projection** of B-Rep geometry onto a
//! view plane. The kernel's job stops at *curves and numbers*: it answers 2-D
//! analytic curves in the view plane, each tagged visible or hidden, and the
//! app lays out arrowheads, line weights and text (spec §3, "Rust produces
//! curves and numbers; the app draws them"). Nothing here renders, measures
//! text, or knows about sheets.
//!
//! ## What D1a implements
//!
//! [`KernelProjection::project`] over every B-Rep edge, with analytic types
//! surviving where the projection preserves them (spec §5.2 increment 1):
//!
//! | 3-D edge curve | projects to |
//! |---|---|
//! | line | [`Curve2::Line`], or [`Curve2::Point`] when the line runs along the view direction |
//! | circle / circular arc | [`Curve2::Ellipse`], [`Curve2::Circle`] (circle plane ⊥ the view direction), or [`Curve2::Line`] (circle plane containing it) |
//! | everything else (ellipse arc, hyperbola arc, surface-pair/SSI curve) | [`Curve2::Polyline`] at the chord tolerance |
//!
//! Every curve is tagged [`Visibility::Visible`] at this increment, which is a
//! wireframe view. Hidden-line classification (D1c) and
//! [`KernelProjection::section_with_plane`] (D1d) are separate increments;
//! their methods and enum arms exist here so the consumers compile against the
//! finished shape, and the unimplemented ones answer a typed
//! [`KernelError::NotSupported`] — loud, never a silent empty result.
//!
//! ## What D1b adds
//!
//! Each curved face's **silhouette** — the locus where the surface normal
//! turns away from the viewer — as [`CurveKind::Silhouette`] curves whose
//! `source` is the FACE, appended after the edges (spec §5.2 increment 2). A
//! cylinder's and cone's rulings stay [`Curve2::Line`]; a sphere's great
//! circle and a torus's coordinate circles come back through the same
//! reconstruction the rim edges use, so they stay [`Curve2::Circle`],
//! [`Curve2::Ellipse`] or an edge-on [`Curve2::Line`]; only a torus's oblique
//! branches are a [`Curve2::Polyline`], chord-refined. They are tagged
//! [`Visibility::Visible`] too — a silhouette can be hidden, and deciding
//! that is D1c.
//!
//! This is what makes the §5.3 projected-bbox oracle an EQUALITY for a curved
//! solid: its extreme points lie on a silhouette, not on an edge.
//!
//! ## What D1c adds
//!
//! **Visibility** (spec §5.2 increment 3). Every projected curve is split at
//! its `(u, v)` crossings with every other projected curve and at its own
//! cusps, and each piece is tagged [`Visibility::Visible`] or
//! [`Visibility::Hidden`] by whether a face of the solid stands in front of
//! it. Each classified piece carries the [`CurveDepth`] the verdict was
//! reached at, so a hidden piece knows what hides it.
//!
//! D1c also puts the kernel's own DECLINES on the contract, as
//! [`ProjectionDeclines`] on [`ViewGeometry`]. A projection can meet a
//! configuration it refuses to decide — a silhouette whose boundary crossings
//! are all tangential, a near-tangential curve crossing that may not have been
//! split, a curve of one body not tested against another body's geometry — and
//! before D1c those were print-only (`KV2_SILHOUETTE_CENSUS`), so no oracle
//! could pin them and a regression that started declining everything would
//! have looked like a clean drawing. They are counted now, by kind. Every one
//! of them is an UNDER-report (a missing dashed line, a missing arc) except
//! [`ProjectionDeclines::cross_body`], which is named separately for exactly
//! that reason.
//!
//! ## What D1d adds
//!
//! [`KernelProjection::section_with_plane`] (spec §5.2 increment 4): the solid
//! cut by a plane, as the cap's hatchable boundary loops ([`SectionLoop`]) plus
//! the cut solid for the caller to run `project` over. The cut is the kernel's
//! own Intersect boolean against a half-space box — not a separate trimming
//! routine — so a section and a user's own Subtract against the same plane
//! cannot disagree. See [`SectionResult`] for the three deviations from §5.1's
//! sketch and [`KernelProjection::section_with_plane`] for the typed outcomes.
//!
//! ## Deviations from the spec's sketch, and why
//!
//! - **`ProjectedCurve::source` is a [`KernelId`], not a `GeomRef`.** A
//!   `GeomRef` needs an [`crate::geom_ref::Anchor`] — which feature produced
//!   the entity — and that is feature-engine's knowledge, not the kernel's.
//!   The kernel reports the handle it does own and the caller lifts it.
//! - **`Curve2` has a `Point` arm.** Spec §5.2 says a line projects "to a line
//!   or a point", so the point has to be representable; a zero-length `Line`
//!   would make every consumer re-derive the degeneracy.
//! - **`ViewGeometry::bbox` is an `Option`.** A solid with no projected curves
//!   has no bounding box, and an empty-but-present box is a lie.
//! - **Handles are [`KernelSolidHandle`]**, the vocabulary of the rest of the
//!   kernel contract, where the spec sketch wrote the kernel-internal
//!   `SolidId`.
//! - **`section_with_plane` takes an origin/normal pair**, since the kernel
//!   contract has no shared `Plane` type to borrow.

use super::types::{KernelError, KernelId, KernelSolidHandle, RigidPlacement};

/// The point type every curve here is built on, re-exported so a consumer can
/// CONSTRUCT a [`Curve2`] without taking a dependency on `cad_primitives`
/// itself. Reading one never needed it (`Point2::x`/`y` come out as `f64`);
/// building one does, and a crate two layers up should not have to name a
/// crate two layers down for the sake of one constructor.
pub use cad_primitives::Point2;

/// Where the viewer stands and which way is up on the paper.
///
/// `dir` is the direction of sight — it points *from* the viewer *into* the
/// scene, so a point's depth grows away from the viewer. `up` need not be
/// perpendicular to `dir`; the perpendicular component is what counts, and a
/// `up` parallel to `dir` has none and is refused.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewFrame {
    /// The view-plane origin: the world point that projects to `(0, 0)`.
    pub origin: [f64; 3],
    /// Direction of sight, away from the viewer. Need not be unit length.
    pub dir: [f64; 3],
    /// Which world direction points up on the paper.
    pub up: [f64; 3],
}

impl ViewFrame {
    /// Looking straight down: `+x` right, `+y` up on the paper. The default
    /// view of a flat-pattern export.
    pub const TOP: ViewFrame = ViewFrame {
        origin: [0.0, 0.0, 0.0],
        dir: [0.0, 0.0, -1.0],
        up: [0.0, 1.0, 0.0],
    };

    /// Looking along `+y`: `+x` right, `+z` up on the paper.
    pub const FRONT: ViewFrame = ViewFrame {
        origin: [0.0, 0.0, 0.0],
        dir: [0.0, 1.0, 0.0],
        up: [0.0, 0.0, 1.0],
    };

    /// Looking along `-x`: `+y` right, `+z` up on the paper.
    pub const RIGHT: ViewFrame = ViewFrame {
        origin: [0.0, 0.0, 0.0],
        dir: [-1.0, 0.0, 0.0],
        up: [0.0, 0.0, 1.0],
    };

    /// A frame looking along `dir` with a world up chosen for it: `+z` unless
    /// the direction is within 1e-6 of the `z` axis, where `+y` is used
    /// instead. Convenience for a caller that has a direction and no opinion
    /// about paper orientation; a drawing view always states its own `up`.
    pub fn looking_along(dir: [f64; 3]) -> ViewFrame {
        let len = norm(dir);
        let along_z = len > 0.0 && (dir[0] / len).abs() < 1e-6 && (dir[1] / len).abs() < 1e-6;
        ViewFrame {
            origin: [0.0, 0.0, 0.0],
            dir,
            up: if along_z {
                [0.0, 1.0, 0.0]
            } else {
                [0.0, 0.0, 1.0]
            },
        }
    }

    /// The frame a caller's OPTIONAL direction and up mean: no direction is
    /// the [`ViewFrame::TOP`] flat-pattern view, and an explicit `up`
    /// overrides the one [`ViewFrame::looking_along`] would pick.
    ///
    /// One place, because two callers need the same answer from the same
    /// pair: the bridge builds the frame it projects with, and a tool that
    /// validates arguments has to know whether THAT frame will have a basis
    /// before it hands the work to the kernel. Deriving it twice is how the
    /// validation and the projection come to disagree.
    pub fn from_parts(dir: Option<[f64; 3]>, up: Option<[f64; 3]>) -> ViewFrame {
        match dir {
            None => ViewFrame::TOP,
            Some(dir) => {
                let mut frame = ViewFrame::looking_along(dir);
                if let Some(up) = up {
                    frame.up = up;
                }
                frame
            }
        }
    }

    /// The orthonormal view basis, or `None` when the frame is degenerate
    /// (`dir` of zero length, or `up` parallel to `dir`).
    pub fn basis(&self) -> Option<ViewBasis> {
        let w = unit(self.dir)?;
        // Gram-Schmidt `up` against the line of sight, then right = w × v so
        // that `(u, v, -w)` is right-handed: with `dir = -z, up = +y` (the
        // top view) this is `u = +x, v = +y`.
        let t = dot(self.up, w);
        let v = unit([
            self.up[0] - t * w[0],
            self.up[1] - t * w[1],
            self.up[2] - t * w[2],
        ])?;
        let u = cross(w, v);
        Some(ViewBasis {
            origin: self.origin,
            u,
            v,
            w,
        })
    }
}

/// An orthonormal frame ready to project with: `(u, v)` span the view plane,
/// `w` is the unit line of sight, and `origin` is the world point at `(0, 0)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewBasis {
    /// World point that projects to `(0, 0)`.
    pub origin: [f64; 3],
    /// Unit paper-right direction.
    pub u: [f64; 3],
    /// Unit paper-up direction.
    pub v: [f64; 3],
    /// Unit line of sight, away from the viewer; depth grows along it.
    pub w: [f64; 3],
}

impl ViewBasis {
    /// `(view-plane position, depth)` of a world point. Depth grows away from
    /// the viewer, so a smaller depth is nearer (what D1c's hidden-line test
    /// compares).
    pub fn project(&self, p: [f64; 3]) -> (Point2, f64) {
        let d = [
            p[0] - self.origin[0],
            p[1] - self.origin[1],
            p[2] - self.origin[2],
        ];
        (Point2::new(dot(d, self.u), dot(d, self.v)), dot(d, self.w))
    }

    /// `(u, v)` components of a world *direction* — the projection's linear
    /// part, with no origin offset.
    pub fn project_dir(&self, d: [f64; 3]) -> [f64; 2] {
        [dot(d, self.u), dot(d, self.v)]
    }

    /// The same view, expressed in the local coordinates of a body placed by
    /// `placement`.
    ///
    /// Projecting a placed body is projecting `R·p + t`; pre-rotating the
    /// basis by `Rᵀ` and moving the origin to `Rᵀ(origin − t)` gives the same
    /// numbers from the body's own untransformed points, so an assembly's
    /// bodies can be projected into one view without copying geometry.
    /// `RigidPlacement`'s rotation is orthonormal with determinant +1, so the
    /// rotated basis is still orthonormal and still right-handed.
    pub fn in_body_frame(&self, placement: &RigidPlacement) -> ViewBasis {
        let inv = |d: [f64; 3]| {
            let m = &placement.rotation;
            // Rᵀ·d — the rotation is orthonormal, so the transpose inverts it.
            [
                m[0][0] * d[0] + m[1][0] * d[1] + m[2][0] * d[2],
                m[0][1] * d[0] + m[1][1] * d[1] + m[2][1] * d[2],
                m[0][2] * d[0] + m[1][2] * d[1] + m[2][2] * d[2],
            ]
        };
        let t = placement.translation;
        ViewBasis {
            origin: inv([
                self.origin[0] - t[0],
                self.origin[1] - t[1],
                self.origin[2] - t[2],
            ]),
            u: inv(self.u),
            v: inv(self.v),
            w: inv(self.w),
        }
    }
}

/// How much the projection may approximate what it cannot keep analytic.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ProjectOpts {
    /// Chord tolerance for the curves that project to polylines, RELATIVE to
    /// the curve's own scale, in the kernel's render sense. `None` — the
    /// default — asks for the kernel's render density, so an extracted
    /// drawing curve and the rendered edge agree point for point.
    pub rel_chord_tolerance: Option<f64>,
}

/// Whether the viewer can see a curve.
///
/// D1a and D1b tagged everything `Visible`; since D1c the kernel splits each
/// projected curve at its crossings and classifies each piece, so `Hidden` is
/// a produced answer and a drawing's HIDDEN layer is populated.
///
/// Serde-able (unlike [`Curve2`], which is built on the serde-less
/// `cad_primitives::Point2`) so the app's layout record
/// ([`crate::annotation::layout`]) can carry the tag straight across the
/// WASM boundary instead of mirroring it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Visibility {
    Visible,
    Hidden,
}

/// Where a classified curve sits in depth along the line of sight (D1c).
///
/// Carried so that a consumer can order coincident curves, and so a `Hidden`
/// piece knows WHAT hides it rather than only that something does — which is
/// what a section view (D1d) needs when it has to decide whether the occluder
/// is the part of the solid the section removed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurveDepth {
    /// Depth of the curve's own 3-D source at the point the classification
    /// sampled — the curve's parameter midpoint. Depth grows AWAY from the
    /// viewer (see [`ViewBasis::project`]), so smaller is nearer.
    pub at_midpoint: f64,
    /// Depth of the NEAREST face found in front of the curve, when one was
    /// found. `Some` exactly when the curve came back
    /// [`Visibility::Hidden`]; `None` for a visible one, where by definition
    /// nothing was in front.
    ///
    /// Measured at the point the classification actually decided at, which is
    /// the curve's midpoint unless the cast there was degenerate and had to be
    /// redone elsewhere on the same curve — so this is an occluder of the
    /// curve, at a point of it, rather than specifically the one over
    /// `at_midpoint`.
    pub occluder: Option<f64>,
}

/// Configurations a projection DECLINED to decide, by kind and counted.
///
/// The kernel's projection under-reports rather than guesses: a silhouette arc
/// it cannot clip is dropped, a crossing it cannot locate is not split, a
/// depth it cannot recover leaves the curve visible. Each of those is a real
/// difference between the drawing and the solid, so each is counted here and
/// travels with the [`ViewGeometry`] — an oracle can pin the counts, and a
/// caller can tell a clean drawing from a quiet one.
///
/// Every field is an under-report of HIDDEN or of silhouette arcs — a drawing
/// missing a line — except [`ProjectionDeclines::cross_body`], which is an
/// over-report of VISIBLE and is named apart for that reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProjectionDeclines {
    /// D1b: a closed silhouette path whose only boundary crossings were
    /// TANGENTIAL, so the arc a hole removes from it has no transversal
    /// crossing to find. The whole path is dropped rather than drawn through
    /// the hole.
    pub silhouette_grazing_removal: u32,
    /// D1b: a closed silhouette path whose crossings did not alternate
    /// enter/exit — a tangency the sign test missed, or a boundary running
    /// along the silhouette. Declined rather than paired arbitrarily.
    pub silhouette_non_alternating: u32,
    /// D1b: a closed silhouette path with no crossings that the face's own
    /// render triangles placed OFF the face. A decision, not a decline, in
    /// the generic case — counted because it is the branch that pays for the
    /// one non-local question, and a path running within a chord sagitta
    /// outside the boundary lands here too.
    pub silhouette_off_face: u32,
    /// D1b: a face with no render triangles at all, so that question could
    /// not be asked.
    pub silhouette_no_triangles: u32,
    /// D1c: a near-tangential crossing of two projected curves — a contact
    /// where the curves touch without crossing, within the band the pair can
    /// be resolved to. The split may be missing, so one classified piece may
    /// span two visibilities and report only the one at its midpoint.
    pub split_tangency: u32,
    /// D1c: the all-pairs crossing search hit its work budget on this view,
    /// so the curves past that point were classified UNSPLIT. A whole-view
    /// decline, counted once.
    pub split_budget: u32,
    /// D1c: a curve piece whose own 3-D depth could not be recovered from its
    /// source, so nothing could be tested in front of it and it stays
    /// visible.
    pub depth_unliftable: u32,
    /// D1c: the view ray TOUCHED a candidate triangle without crossing its
    /// interior — it ran in the triangle's plane, or met it exactly on an
    /// edge or at a vertex. Such a face contributes no occlusion, and that is
    /// a decision rather than a fallback: a face hides a curve only by
    /// standing BETWEEN it and the viewer, which means the ray crosses from
    /// one side of it to the other, so a face the ray merely grazes separates
    /// nothing. Counted once per classified curve, because the configuration
    /// is systematic rather than accidental — a rim at a bore's own radius
    /// grazes the whole inscribed wall, and an axis-aligned view of a
    /// prismatic solid grazes every face parallel to the line of sight — and
    /// the count is how a caller tells a decided drawing from a degenerate
    /// one.
    pub ray_grazes_face: u32,
    /// D1c: a classified piece whose visibility is NOT constant along it —
    /// two interior points of the one piece were each decided, and they
    /// disagreed. The piece claims one visibility for its whole length, so
    /// this says the drawing carries a half-hidden curve drawn whole, and the
    /// cause is upstream: the crossing where the visibility changes was not
    /// split (a declined tangency, a dropped silhouette arc, a discarded
    /// fold).
    ///
    /// It is detected rather than voted on. The verdict reported is the one at
    /// the point the classification decided at — the midpoint, or the first
    /// non-degenerate re-cast — and the disagreement is counted, because a
    /// majority over a handful of probe points would turn a known-wrong curve
    /// into a confidently-wrong one. Counted once per piece.
    pub piece_spans_change: u32,
    /// D1c: bodies whose curves were classified against their OWN geometry
    /// only. Visibility is computed per body, so in a multi-body view a curve
    /// hidden behind a DIFFERENT body is still reported visible. Counted once
    /// per body in a view of more than one.
    pub cross_body: u32,
}

impl ProjectionDeclines {
    /// Every counter with its name — so a report cannot drift from the struct.
    pub fn counts(&self) -> [(&'static str, u32); 10] {
        [
            (
                "silhouette_grazing_removal",
                self.silhouette_grazing_removal,
            ),
            (
                "silhouette_non_alternating",
                self.silhouette_non_alternating,
            ),
            ("silhouette_off_face", self.silhouette_off_face),
            ("silhouette_no_triangles", self.silhouette_no_triangles),
            ("split_tangency", self.split_tangency),
            ("split_budget", self.split_budget),
            ("depth_unliftable", self.depth_unliftable),
            ("ray_grazes_face", self.ray_grazes_face),
            ("piece_spans_change", self.piece_spans_change),
            ("cross_body", self.cross_body),
        ]
    }

    /// Total declines of every kind.
    pub fn total(&self) -> u32 {
        self.counts().iter().map(|(_, n)| *n).sum()
    }

    /// Add another projection's declines to these — how the per-body counts of
    /// a multi-body view add up.
    pub fn merge(&mut self, other: &ProjectionDeclines) {
        self.silhouette_grazing_removal = self
            .silhouette_grazing_removal
            .saturating_add(other.silhouette_grazing_removal);
        self.silhouette_non_alternating = self
            .silhouette_non_alternating
            .saturating_add(other.silhouette_non_alternating);
        self.silhouette_off_face = self
            .silhouette_off_face
            .saturating_add(other.silhouette_off_face);
        self.silhouette_no_triangles = self
            .silhouette_no_triangles
            .saturating_add(other.silhouette_no_triangles);
        self.split_tangency = self.split_tangency.saturating_add(other.split_tangency);
        self.split_budget = self.split_budget.saturating_add(other.split_budget);
        self.depth_unliftable = self.depth_unliftable.saturating_add(other.depth_unliftable);
        self.ray_grazes_face = self.ray_grazes_face.saturating_add(other.ray_grazes_face);
        self.piece_spans_change = self
            .piece_spans_change
            .saturating_add(other.piece_spans_change);
        self.cross_body = self.cross_body.saturating_add(other.cross_body);
    }
}

/// What the curve is in the drawing.
///
/// Serde-able for the same reason as [`Visibility`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum CurveKind {
    /// The projection of a B-Rep edge.
    Edge,
    /// The locus on a curved face where the normal turns away from the viewer
    /// (D1b). `source` names the FACE, not an edge.
    Silhouette,
    /// A loop of a section cut's cap (D1d).
    SectionOutline,
}

/// A curve in the view plane, in the view's `(u, v)` coordinates and the
/// model's units (meters — the kernel models in meters; an exporter that
/// writes another unit scales at the boundary).
///
/// Angles and parameters run **counter-clockwise** in `(u, v)` and `start <
/// end` always: a projected curve is a point set to draw, so traversal
/// direction is deliberately not preserved (the 3-D edge keeps it).
#[derive(Debug, Clone, PartialEq)]
pub enum Curve2 {
    /// A line that projects along the line of sight.
    Point(Point2),
    Line {
        start: Point2,
        end: Point2,
    },
    /// Circular arc, or a full circle when `end_angle - start_angle == 2π`.
    /// Angles are measured from `+u`, counter-clockwise.
    Circle {
        center: Point2,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
    },
    /// Elliptical arc, or a full ellipse when `end_param - start_param == 2π`.
    ///
    /// The point set is
    /// `center + major_radius·cos t·major_axis + minor_radius·sin t·perp(major_axis)`
    /// for `t ∈ [start_param, end_param]`, where `perp((x, y)) = (−y, x)`.
    /// `major_axis` is unit and `major_radius ≥ minor_radius > 0`.
    Ellipse {
        center: Point2,
        major_axis: [f64; 2],
        major_radius: f64,
        minor_radius: f64,
        start_param: f64,
        end_param: f64,
    },
    /// A sampled curve: whatever the projection could not keep analytic, at
    /// the chord tolerance [`ProjectOpts`] asked for.
    Polyline {
        points: Vec<Point2>,
        /// `true` when the last point joins the first (the point list does
        /// NOT repeat it).
        closed: bool,
    },
}

impl Curve2 {
    /// Point at parameter `t`.
    ///
    /// Every arm is parameterized, and [`Curve2::param_range`] gives the
    /// domain: `[0, 1]` along a line, the angular or elliptic interval of a
    /// circular or elliptic arc, `[0, chords]` along a polyline (so the
    /// integer parameters ARE its vertices), and the degenerate `[0, 0]` of a
    /// point. `None` only for an empty polyline, which has no points to
    /// answer with.
    ///
    /// The parameterization is what D1c splits on, and it is deliberately the
    /// curve's OWN: a sub-curve of a circular arc is the same circle over a
    /// sub-interval, with no resampling and no accumulated error.
    pub fn eval(&self, t: f64) -> Option<Point2> {
        match *self {
            Curve2::Point(p) => Some(p),
            Curve2::Polyline { ref points, closed } => {
                let chords = polyline_chords(points.len(), closed)?;
                if chords == 0 {
                    return Some(points[0]); // a one-point polyline IS that point
                }
                // `floor` then clamp: the last chord owns `t == chords`, and a
                // parameter outside the domain evaluates on the nearest chord's
                // own line rather than refusing — the clip's callers hand this
                // function roots that can sit a float epsilon outside.
                let i = (t.floor() as isize).clamp(0, chords as isize - 1) as usize;
                let f = t - i as f64;
                let a = points[i];
                let b = points[(i + 1) % points.len()];
                Some(Point2::new(
                    a.x() + f * (b.x() - a.x()),
                    a.y() + f * (b.y() - a.y()),
                ))
            }
            Curve2::Line { start, end } => Some(Point2::new(
                start.x() + t * (end.x() - start.x()),
                start.y() + t * (end.y() - start.y()),
            )),
            Curve2::Circle { center, radius, .. } => Some(Point2::new(
                center.x() + radius * t.cos(),
                center.y() + radius * t.sin(),
            )),
            Curve2::Ellipse {
                center,
                major_axis,
                major_radius,
                minor_radius,
                ..
            } => {
                let p = [-major_axis[1], major_axis[0]];
                let (c, s) = (t.cos(), t.sin());
                Some(Point2::new(
                    center.x() + major_radius * c * major_axis[0] + minor_radius * s * p[0],
                    center.y() + major_radius * c * major_axis[1] + minor_radius * s * p[1],
                ))
            }
        }
    }

    /// The curve's parameter domain — see [`Curve2::eval`]. `None` only for an
    /// empty polyline.
    pub fn param_range(&self) -> Option<(f64, f64)> {
        match *self {
            Curve2::Point(_) => Some((0.0, 0.0)),
            Curve2::Polyline { ref points, closed } => {
                Some((0.0, polyline_chords(points.len(), closed)? as f64))
            }
            Curve2::Line { .. } => Some((0.0, 1.0)),
            Curve2::Circle {
                start_angle,
                end_angle,
                ..
            } => Some((start_angle, end_angle)),
            Curve2::Ellipse {
                start_param,
                end_param,
                ..
            } => Some((start_param, end_param)),
        }
    }

    /// The curve's endpoints, or `None` for a closed curve (a full circle or
    /// ellipse, a closed polyline) and for a point.
    pub fn endpoints(&self) -> Option<(Point2, Point2)> {
        match self {
            Curve2::Point(_) => None,
            Curve2::Line { start, end } => Some((*start, *end)),
            Curve2::Circle { .. } | Curve2::Ellipse { .. } => {
                let (t0, t1) = self.param_range()?;
                if (t1 - t0) >= std::f64::consts::TAU - 1e-12 {
                    return None;
                }
                Some((self.eval(t0)?, self.eval(t1)?))
            }
            Curve2::Polyline { points, closed } => {
                if *closed || points.len() < 2 {
                    None
                } else {
                    Some((points[0], points[points.len() - 1]))
                }
            }
        }
    }

    /// The curve's EXACT axis-aligned bounding box: endpoints plus every
    /// axis-extreme parameter that falls inside the range, so a quarter arc
    /// does not claim its whole circle's box and a full one does.
    pub fn bbox(&self) -> Aabb2 {
        match self {
            Curve2::Point(p) => Aabb2::point(*p),
            Curve2::Line { start, end } => Aabb2::point(*start).united_point(*end),
            Curve2::Polyline { points, .. } => {
                let mut it = points.iter();
                match it.next() {
                    None => Aabb2::point(Point2::new(0.0, 0.0)),
                    Some(first) => {
                        let mut bb = Aabb2::point(*first);
                        for p in it {
                            bb = bb.united_point(*p);
                        }
                        bb
                    }
                }
            }
            Curve2::Circle { .. } | Curve2::Ellipse { .. } => {
                let (t0, t1) = self.param_range().expect("parameterized arm has a range");
                let mut bb = Aabb2::point(self.eval(t0).expect("parameterized arm evaluates"))
                    .united_point(self.eval(t1).expect("parameterized arm evaluates"));
                for t in self.axis_extremes() {
                    if t > t0 && t < t1 {
                        bb = bb.united_point(self.eval(t).expect("parameterized arm evaluates"));
                    }
                }
                bb
            }
        }
    }

    /// Parameters at which `du/dt` or `dv/dt` vanishes, unwrapped into every
    /// `2π` period the curve's range can reach.
    fn axis_extremes(&self) -> Vec<f64> {
        use std::f64::consts::{PI, TAU};
        let (t0, t1) = match self.param_range() {
            Some(r) => r,
            None => return Vec::new(),
        };
        // `x(t) = cx + A cos t + B sin t` is extremal where `tan t = B / A`.
        let seeds: [f64; 2] = match *self {
            Curve2::Circle { .. } => [0.0, PI / 2.0],
            Curve2::Ellipse {
                major_axis,
                major_radius,
                minor_radius,
                ..
            } => {
                let p = [-major_axis[1], major_axis[0]];
                [
                    (minor_radius * p[0]).atan2(major_radius * major_axis[0]),
                    (minor_radius * p[1]).atan2(major_radius * major_axis[1]),
                ]
            }
            _ => return Vec::new(),
        };
        let mut out = Vec::with_capacity(8);
        for seed in seeds {
            // Both `seed` and `seed + π` are extremal (opposite ends).
            for base in [seed, seed + PI] {
                let k0 = ((t0 - base) / TAU).floor();
                for k in [k0, k0 + 1.0, k0 + 2.0] {
                    out.push(base + k * TAU);
                }
            }
        }
        out.retain(|t| *t > t0 && *t < t1);
        out
    }

    /// Arc length, in model units.
    ///
    /// Exact for a point, a line and a circular arc. An ellipse has no
    /// closed form, so its length is a composite Simpson quadrature of
    /// `|dP/dt|` over [`ELLIPSE_QUADRATURE_STEPS`] intervals.
    ///
    /// That quadrature is **deterministic**, which is the property the
    /// invariance oracles rest on, but its accuracy falls off with the
    /// ellipse's ASPECT RATIO: `|dP/dt|` approaches `|sin t|` as the minor
    /// radius vanishes, and Simpson converges slowly near that cusp. Measured
    /// against a 2,000,000-interval reference: ≤ 1e-13 relative up to aspect
    /// 20, 9e-11 at 100, and ~2e-7 at 1000 and beyond. A projection reaches
    /// those ratios — `kernel_v2::projection` reports an ellipse until the
    /// minor radius falls under `TAU_MODEL`, which is aspect 8e4 on an 8 mm
    /// rim — so a caller needing a tight length on a nearly edge-on ellipse
    /// must integrate it itself. Determinism is NOT accuracy, and the two are
    /// separately load-bearing here.
    ///
    /// A polyline's length is the length of the polyline, which is what the
    /// drawing shows.
    pub fn length(&self) -> f64 {
        self.length_with_steps(ELLIPSE_QUADRATURE_STEPS)
    }

    /// [`Self::length`] with the ellipse arm's composite-Simpson quadrature at
    /// `steps` intervals instead of [`ELLIPSE_QUADRATURE_STEPS`].
    ///
    /// Every other arm has a closed form and ignores `steps`, so this differs
    /// from [`Self::length`] on the ellipse arm alone. `steps` is rounded down
    /// to an even number and floored at 2, which is what Simpson needs.
    ///
    /// **Why it is public** (Q6 of `specs/agent_mechanical_design.md` §4.2).
    /// An arc length an agent is handed must say which tier it is, and the
    /// ellipse arm's tier is "a quadrature, not a closed form". The honest
    /// statement of its accuracy is a MEASURED one, so `kernel_v2`'s
    /// `edge_length` reports `|length_with_steps(2N) − length()|` as the
    /// convergence witness alongside the value. Re-deriving the integrand
    /// there would have been a second integrator of the same curve; this is
    /// the one.
    pub fn length_with_steps(&self, steps: usize) -> f64 {
        match *self {
            Curve2::Point(_) => 0.0,
            Curve2::Line { start, end } => {
                ((end.x() - start.x()).powi(2) + (end.y() - start.y()).powi(2)).sqrt()
            }
            Curve2::Circle {
                radius,
                start_angle,
                end_angle,
                ..
            } => radius * (end_angle - start_angle),
            Curve2::Ellipse {
                major_axis,
                major_radius,
                minor_radius,
                start_param,
                end_param,
                ..
            } => {
                let p = [-major_axis[1], major_axis[0]];
                let speed = |t: f64| {
                    let (c, s) = (t.cos(), t.sin());
                    let dx = -major_radius * s * major_axis[0] + minor_radius * c * p[0];
                    let dy = -major_radius * s * major_axis[1] + minor_radius * c * p[1];
                    (dx * dx + dy * dy).sqrt()
                };
                let n = steps.max(2) & !1;
                let h = (end_param - start_param) / n as f64;
                let mut acc = speed(start_param) + speed(end_param);
                for i in 1..n {
                    let t = start_param + h * i as f64;
                    acc += if i % 2 == 1 { 4.0 } else { 2.0 } * speed(t);
                }
                acc * h / 3.0
            }
            Curve2::Polyline { ref points, closed } => {
                let mut total = 0.0;
                for w in points.windows(2) {
                    total += ((w[1].x() - w[0].x()).powi(2) + (w[1].y() - w[0].y()).powi(2)).sqrt();
                }
                if closed && points.len() > 2 {
                    let (a, b) = (points[points.len() - 1], points[0]);
                    total += ((b.x() - a.x()).powi(2) + (b.y() - a.y()).powi(2)).sqrt();
                }
                total
            }
        }
    }

    /// The curve as a polyline whose chord deviation from the true curve is at
    /// most `sagitta` (model units), for a consumer that can only draw
    /// segments — an R12 DXF, an SVG path.
    ///
    /// A circle's segment count comes from the sagitta identity
    /// `ρ(1 − cos(Δ/2)) ≤ sagitta`; an ellipse reuses it with its MAJOR
    /// radius, which is conservative because the ellipse is the image of a
    /// circle of that radius under a map whose operator norm is 1, and such a
    /// map cannot increase the deviation.
    pub fn flatten(&self, sagitta: f64) -> Vec<Point2> {
        match self {
            Curve2::Point(p) => vec![*p],
            Curve2::Line { start, end } => vec![*start, *end],
            Curve2::Polyline { points, .. } => points.clone(),
            Curve2::Circle { radius, .. }
            | Curve2::Ellipse {
                major_radius: radius,
                ..
            } => {
                let (t0, t1) = self.param_range().expect("parameterized arm has a range");
                let n = segment_count(*radius, t1 - t0, sagitta);
                (0..=n)
                    .map(|i| {
                        let t = t0 + (t1 - t0) * (i as f64) / (n as f64);
                        self.eval(t).expect("parameterized arm evaluates")
                    })
                    .collect()
            }
        }
    }

    /// Whether the curve closes on itself — a full circle or ellipse, or a
    /// closed polyline.
    pub fn is_closed(&self) -> bool {
        match self {
            Curve2::Point(_) => false,
            Curve2::Line { .. } => false,
            Curve2::Polyline { closed, .. } => *closed,
            Curve2::Circle { .. } | Curve2::Ellipse { .. } => self
                .param_range()
                .is_some_and(|(a, b)| b - a >= std::f64::consts::TAU - 1e-12),
        }
    }

    /// The part of this curve between parameters `t0` and `t1`, as the SAME
    /// kind of curve over a sub-interval.
    ///
    /// This is D1c's splitting primitive, and keeping the kind is the point:
    /// half of a circular arc is a circular arc of the same centre and radius,
    /// not a polyline, so a drawing that has been split for hidden-line
    /// removal still carries true `ARC` entities. A polyline's sub-curve keeps
    /// the vertices strictly inside `(t0, t1)` and adds the two interpolated
    /// ends, so it is a sub-polyline of the original and nothing is resampled.
    ///
    /// `None` when the interval is empty or degenerate (`t1 <= t0`), when it
    /// falls outside [`Curve2::param_range`], or for the arms with nothing to
    /// cut — a point, and an empty polyline. A sub-curve of a CLOSED curve is
    /// an open one, which is why a full turn asked for in full comes back
    /// unchanged rather than as an arc of itself.
    pub fn subcurve(&self, t0: f64, t1: f64) -> Option<Curve2> {
        let (lo, hi) = self.param_range()?;
        if !(t0.is_finite() && t1.is_finite()) || t1 <= t0 {
            return None;
        }
        // The caller's interval, clamped to the domain. A root the clip found
        // can sit a float epsilon outside it.
        let (t0, t1) = (t0.max(lo), t1.min(hi));
        if t1 <= t0 {
            return None;
        }
        if t0 <= lo && t1 >= hi {
            return Some(self.clone());
        }
        match *self {
            Curve2::Point(_) => None,
            Curve2::Line { .. } => {
                let (a, b) = (self.eval(t0)?, self.eval(t1)?);
                Some(Curve2::Line { start: a, end: b })
            }
            Curve2::Circle { center, radius, .. } => Some(Curve2::Circle {
                center,
                radius,
                start_angle: t0,
                end_angle: t1,
            }),
            Curve2::Ellipse {
                center,
                major_axis,
                major_radius,
                minor_radius,
                ..
            } => Some(Curve2::Ellipse {
                center,
                major_axis,
                major_radius,
                minor_radius,
                start_param: t0,
                end_param: t1,
            }),
            Curve2::Polyline { ref points, closed } => {
                let chords = polyline_chords(points.len(), closed)?;
                let mut out = vec![self.eval(t0)?];
                // The original's own vertices, which are the integers.
                let first = (t0.floor() as usize) + 1;
                for i in first..=chords {
                    if (i as f64) <= t0 || (i as f64) >= t1 {
                        continue;
                    }
                    out.push(points[i % points.len()]);
                }
                out.push(self.eval(t1)?);
                out.dedup_by(|b, a| (b.x() - a.x()).abs() <= 0.0 && (b.y() - a.y()).abs() <= 0.0);
                if out.len() < 2 {
                    return None;
                }
                Some(Curve2::Polyline {
                    points: out,
                    closed: false,
                })
            }
        }
    }

    /// The parts of this curve inside the closed disc `|p − center| ≤ radius`
    /// (`specs/drawings_and_mbd.md` §8, D4c) — a detail view's crop, TRIMMED
    /// rather than culled.
    ///
    /// **Each piece keeps its kind where a kind survives the cut**, which is
    /// the point: a line clipped by a circle is a line, and an ARC of a
    /// circle clipped by another circle is an ARC of the same circle, so a
    /// detail view's DXF carries true `LINE` and `ARC` entities rather than
    /// chord polylines. The crossing parameters are solved in closed form for
    /// those two arms (a quadratic for a chord, `A cos t + B sin t = K` for a
    /// circle) and [`Curve2::subcurve`] does the cutting.
    ///
    /// **An ELLIPSE is flattened first**, at `sagitta`. Its intersection with
    /// a circle is a quartic, and the DXF writer flattens every ellipse
    /// anyway (R12 has no `ELLIPSE` entity), so nothing is lost that a reader
    /// of the file could have had — where solving the quartic would be a new
    /// root-finder for one arm with no customer. `sagitta` is ignored by
    /// every other arm, which are exact.
    ///
    /// A non-finite or non-positive `radius` clips everything away: the caller
    /// that authored the disc is the one that can say what a bad radius means
    /// (`DrawingError::BadCropRadius`), and silently treating it as "no crop"
    /// would put the whole part inside a detail view.
    ///
    /// A closed curve that crosses the boundary comes back as separate open
    /// pieces, including the two that meet at its own seam — they are two
    /// entities where one would do, and joining them would mean reasoning
    /// about the seam for the sake of a file's entity count.
    ///
    /// A TANGENCY is a cut even though it crosses nothing. Each interval's
    /// verdict is read at its MIDPOINT, and a curve that touches the boundary
    /// at exactly its own midpoint — the ordinary shape of a full circle
    /// written from its own start angle — would otherwise read as inside and
    /// survive a crop it never enters. Cutting there puts the touch point on
    /// an interval boundary instead, where it decides nothing; consecutive
    /// kept intervals are then rejoined, so a curve tangent from INSIDE still
    /// comes back as one piece rather than two arcs meeting at the point it
    /// grazes.
    pub fn clipped_to_disc(&self, center: [f64; 2], radius: f64, sagitta: f64) -> Vec<Curve2> {
        if !(radius.is_finite() && radius > 0.0 && center[0].is_finite() && center[1].is_finite()) {
            return Vec::new();
        }
        let inside = |p: Point2| -> bool {
            let (dx, dy) = (p.x() - center[0], p.y() - center[1]);
            dx * dx + dy * dy <= radius * radius
        };
        // The ellipse arm becomes a polyline and is clipped as one. Done by
        // RECURSION on the polyline arm rather than by a second clipper, so
        // the two cannot disagree about which pieces are inside.
        if let Curve2::Ellipse { .. } = self {
            let points = self.flatten(sagitta);
            if points.len() < 2 {
                return points
                    .into_iter()
                    .filter(|p| inside(*p))
                    .map(Curve2::Point)
                    .collect();
            }
            return Curve2::Polyline {
                points,
                closed: self.is_closed(),
            }
            .clipped_to_disc(center, radius, sagitta);
        }
        if let Curve2::Point(p) = self {
            return if inside(*p) {
                vec![self.clone()]
            } else {
                vec![]
            };
        }
        let Some((lo, hi)) = self.param_range() else {
            return Vec::new();
        };
        let mut cuts = self.disc_crossings(center, radius);
        cuts.retain(|t| t.is_finite() && *t > lo && *t < hi);
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|b, a| (*b - *a).abs() <= 1e-15 * (1.0 + a.abs()));
        let mut bounds = Vec::with_capacity(cuts.len() + 2);
        bounds.push(lo);
        bounds.extend(cuts);
        bounds.push(hi);
        let mut out = Vec::new();
        // Consecutive kept intervals are ONE piece. They arise where the cut
        // was a TANGENCY — the curve met the boundary without leaving the
        // disc — and emitting them separately would put a seam in a hole's rim
        // on the sheet. The run never spans `lo`/`hi`, so a closed curve's own
        // seam still comes back as two pieces, which is the documented answer.
        let mut run: Option<(f64, f64)> = None;
        let flush = |run: &mut Option<(f64, f64)>, out: &mut Vec<Curve2>| {
            if let Some((a, b)) = run.take() {
                if let Some(piece) = self.subcurve(a, b) {
                    out.push(piece);
                }
            }
        };
        for pair in bounds.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if b <= a {
                continue;
            }
            // The MIDPOINT decides, not an endpoint: an endpoint of a piece
            // sits exactly on the boundary by construction, where `inside`'s
            // comparison is a coin toss. This is sound only because a
            // tangency is one of the cuts — otherwise an interval that TOUCHES
            // the boundary at its own midpoint and lies outside everywhere
            // else would answer "inside" and be kept whole.
            let keep = match self.eval(0.5 * (a + b)) {
                Some(mid) => inside(mid),
                None => false,
            };
            if !keep {
                flush(&mut run, &mut out);
                continue;
            }
            match &mut run {
                Some(r) if r.1 == a => r.1 = b,
                _ => {
                    flush(&mut run, &mut out);
                    run = Some((a, b));
                }
            }
        }
        flush(&mut run, &mut out);
        out
    }

    /// The parameters at which this curve crosses the circle
    /// `|p − center| = radius`, in closed form (D4c). Not de-duplicated and
    /// not range-filtered — [`Curve2::clipped_to_disc`] does both.
    ///
    /// Empty for the arms that have no closed form here (a point, an
    /// ellipse); `clipped_to_disc` routes those elsewhere rather than
    /// treating "no crossings" as "no boundary", which would keep or drop a
    /// whole ellipse by its midpoint alone.
    fn disc_crossings(&self, center: [f64; 2], radius: f64) -> Vec<f64> {
        // |a + t·d − c|² = r², i.e. (d·d)t² + 2(d·(a−c))t + (|a−c|² − r²) = 0,
        // returning the roots in `(0, 1)` — a chord's own parameter.
        let chord = |a: Point2, b: Point2| -> Vec<f64> {
            let (dx, dy) = (b.x() - a.x(), b.y() - a.y());
            let (ex, ey) = (a.x() - center[0], a.y() - center[1]);
            let qa = dx * dx + dy * dy;
            if qa <= 0.0 {
                return Vec::new();
            }
            let qb = 2.0 * (dx * ex + dy * ey);
            let qc = ex * ex + ey * ey - radius * radius;
            let disc = qb * qb - 4.0 * qa * qc;
            if disc < 0.0 {
                return Vec::new();
            }
            // `disc == 0` is a TANGENCY, and it is reported — as the double
            // root, which the caller dedups to one cut. It touches without
            // crossing, so it changes no interval's inside/outside verdict;
            // what it does is keep the touch point off any interval's
            // MIDPOINT, which is what the verdict is read from. Without it a
            // chord tangent at its own midpoint reads as inside and the whole
            // chord survives a crop it never enters.
            let root = disc.sqrt();
            [(-qb - root) / (2.0 * qa), (-qb + root) / (2.0 * qa)]
                .into_iter()
                .filter(|t| *t > 0.0 && *t < 1.0)
                .collect()
        };
        match *self {
            Curve2::Point(_) | Curve2::Ellipse { .. } => Vec::new(),
            Curve2::Line { start, end } => chord(start, end),
            Curve2::Polyline { ref points, closed } => {
                let Some(chords) = polyline_chords(points.len(), closed) else {
                    return Vec::new();
                };
                let mut out = Vec::new();
                for i in 0..chords {
                    let a = points[i];
                    let b = points[(i + 1) % points.len()];
                    out.extend(chord(a, b).into_iter().map(|t| i as f64 + t));
                }
                out
            }
            // On the circle `p(t) = c₀ + r₀(cos t, sin t)`:
            //   |p − c|² = |c₀ − c|² + r₀² + 2r₀((c₀ − c)·(cos t, sin t)) = r²
            // so `A cos t + B sin t = K`, which is `R cos(t − φ) = K` with
            // `R = hypot(A, B)` and `φ = atan2(B, A)`. Every solution in the
            // curve's own (possibly multi-turn, possibly negative) parameter
            // range is wanted, so both roots are unwrapped across every turn
            // the range can reach.
            Curve2::Circle {
                center: c0,
                radius: r0,
                start_angle,
                end_angle,
            } => {
                use std::f64::consts::TAU;
                if !(r0.is_finite() && r0 > 0.0) {
                    return Vec::new();
                }
                let (ex, ey) = (c0.x() - center[0], c0.y() - center[1]);
                let a = 2.0 * r0 * ex;
                let b = 2.0 * r0 * ey;
                let k = radius * radius - (ex * ex + ey * ey) - r0 * r0;
                let r = (a * a + b * b).sqrt();
                if r <= 0.0 {
                    // Concentric: the whole circle is inside or outside, and
                    // either way it crosses nowhere.
                    return Vec::new();
                }
                let ratio = k / r;
                // `|ratio| == 1` is a tangency and IS reported, for the
                // reason the chord's double root is: it keeps the touch point
                // off a candidate interval's midpoint. `acos` gives `0` there
                // (the two branches coincide and dedup to one) or `π` (they
                // differ by a full turn, which the turn sweep enumerates
                // anyway).
                if ratio.abs() > 1.0 {
                    return Vec::new();
                }
                let phi = b.atan2(a);
                let delta = ratio.acos();
                let mut out = Vec::new();
                let first = ((start_angle - phi) / TAU).floor() - 1.0;
                let last = ((end_angle - phi) / TAU).ceil() + 1.0;
                let mut turn = first;
                while turn <= last {
                    out.push(phi + delta + turn * TAU);
                    out.push(phi - delta + turn * TAU);
                    turn += 1.0;
                }
                out
            }
        }
    }

    /// This curve scaled about the view-plane origin by `scale` and then
    /// moved by `offset` — the paper-space placement of a drawing view
    /// (`specs/drawings_and_mbd.md` §8, D4a).
    ///
    /// `scale` is UNIFORM and must be positive and finite; `None` otherwise.
    /// Uniform is not a limitation here but the whole point: a drawing scale
    /// is one ratio, and under a uniform positive scale a circle stays a
    /// circle, an ellipse keeps its axis directions, and every angle
    /// parameter is unchanged — so the analytic arms survive placement
    /// instead of degrading to polylines on the way to the sheet. An
    /// anisotropic scale would turn a circle into an ellipse and an ellipse
    /// into one with a different major direction, which is a different curve
    /// family and a different arm.
    pub fn transformed(&self, scale: f64, offset: [f64; 2]) -> Option<Curve2> {
        self.transformed_by(scale, [1.0, 0.0], offset)
    }

    /// This curve scaled by `scale`, ROTATED about the view-plane origin by
    /// `rotation = [cos θ, sin θ]`, then moved by `offset`
    /// (`specs/drawings_and_mbd.md` §8, D4b).
    ///
    /// What needs the rotation: a section cap comes back from
    /// [`KernelProjection::section_with_plane`] in the cut plane's OWN
    /// `(u, v)` — a frame the kernel derives from the normal alone
    /// ([`SectionResult::plane_basis`]) — while the section VIEW's paper up is
    /// chosen to agree with the parent view it was cut on. Both frames share
    /// the line of sight and the handedness, so they differ by exactly one
    /// planar rotation, and the cap's hatch boundary has to be expressed in
    /// the view's frame or the hatch arrives rotated against the drawing it
    /// fills.
    ///
    /// A rotation is a similarity like the scale, so the same argument carries:
    /// a circle stays a circle and an ellipse keeps its axis LENGTHS, with the
    /// major direction rotated and the angle parameters shifted — the analytic
    /// arms survive, which is what keeps the DXF's `CIRCLE` a circle. The
    /// rotation must be a unit `[cos, sin]` pair; `None` otherwise, since a
    /// non-unit pair would scale the curve a second time by a factor the
    /// caller did not name.
    ///
    /// A `Circle`'s angles are measured from `+u`, so both shift by θ; an
    /// `Ellipse`'s parameters are measured from its own `major_axis`, so they
    /// are UNCHANGED and only the axis turns. Getting that backwards rotates
    /// an elliptical cap twice.
    pub fn transformed_by(
        &self,
        scale: f64,
        rotation: [f64; 2],
        offset: [f64; 2],
    ) -> Option<Curve2> {
        if !(scale.is_finite() && scale > 0.0 && offset[0].is_finite() && offset[1].is_finite()) {
            return None;
        }
        let (c, s) = (rotation[0], rotation[1]);
        if !(c.is_finite() && s.is_finite()) || (c * c + s * s - 1.0).abs() > 1e-9 {
            return None;
        }
        let theta = s.atan2(c);
        let map = |p: Point2| {
            Point2::new(
                (p.x() * c - p.y() * s) * scale + offset[0],
                (p.x() * s + p.y() * c) * scale + offset[1],
            )
        };
        let turn = |d: [f64; 2]| [d[0] * c - d[1] * s, d[0] * s + d[1] * c];
        Some(match *self {
            Curve2::Point(p) => Curve2::Point(map(p)),
            Curve2::Line { start, end } => Curve2::Line {
                start: map(start),
                end: map(end),
            },
            Curve2::Circle {
                center,
                radius,
                start_angle,
                end_angle,
            } => {
                // Both angles shift by θ, and the pair is then slid back by
                // whole turns so `start_angle` lands in `[0, 2π)`. The slide
                // keeps the span and so keeps the arc: without it a rotated
                // arc can carry a start of, say, 7.1 rad, which the DXF
                // writer turns into 407° — a number a reader is free to
                // reject. The span itself is NOT wrapped: a full circle's
                // `end − start == 2π` is what tells it from a zero arc.
                let turns = (start_angle + theta).div_euclid(std::f64::consts::TAU);
                let shift = theta - turns * std::f64::consts::TAU;
                Curve2::Circle {
                    center: map(center),
                    radius: radius * scale,
                    start_angle: start_angle + shift,
                    end_angle: end_angle + shift,
                }
            }
            Curve2::Ellipse {
                center,
                major_axis,
                major_radius,
                minor_radius,
                start_param,
                end_param,
            } => Curve2::Ellipse {
                center: map(center),
                major_axis: turn(major_axis),
                major_radius: major_radius * scale,
                minor_radius: minor_radius * scale,
                start_param,
                end_param,
            },
            Curve2::Polyline { ref points, closed } => Curve2::Polyline {
                points: points.iter().map(|p| map(*p)).collect(),
                closed,
            },
        })
    }
}

/// How many chords a polyline of `n` points has: one fewer than its points
/// when open, and one per point when closed (the last returning to the
/// first). `None` for an empty one, and zero chords for a single point.
fn polyline_chords(n: usize, closed: bool) -> Option<usize> {
    match n {
        0 => None,
        1 => Some(0),
        n => Some(if closed { n } else { n - 1 }),
    }
}

/// Simpson intervals used by [`Curve2::length`] on an ellipse. Even, as
/// composite Simpson requires.
pub const ELLIPSE_QUADRATURE_STEPS: usize = 2048;

/// Segments needed to hold an arc of `sweep` radians on a curve of radius
/// `radius` within `sagitta` of its chords. At least one; at least four for a
/// full turn, so a circle never degenerates into a triangle.
fn segment_count(radius: f64, sweep: f64, sagitta: f64) -> usize {
    let sweep = sweep.abs();
    if !(radius.is_finite() && radius > 0.0) || !sweep.is_finite() || sweep <= 0.0 {
        return 1;
    }
    let ratio = (1.0 - (sagitta.max(0.0) / radius)).clamp(-1.0, 1.0);
    let step = 2.0 * ratio.acos();
    if !(step.is_finite() && step > 0.0) {
        return 1;
    }
    let n = (sweep / step).ceil() as usize;
    let floor = ((sweep / std::f64::consts::TAU) * 4.0).ceil() as usize;
    n.max(floor).max(1)
}

/// An axis-aligned box in the view plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb2 {
    pub min: Point2,
    pub max: Point2,
}

impl Aabb2 {
    /// The degenerate box at one point.
    pub fn point(p: Point2) -> Aabb2 {
        Aabb2 { min: p, max: p }
    }

    /// A box from plain pairs — for a caller that has `[f64; 2]`s and no
    /// reason to depend on `cad_primitives` for the one conversion (D4b: a
    /// detail view's crop box, built from a centre and a radius).
    pub fn from_pairs(min: [f64; 2], max: [f64; 2]) -> Aabb2 {
        Aabb2 {
            min: Point2::new(min[0], min[1]),
            max: Point2::new(max[0], max[1]),
        }
    }

    /// Whether the two boxes overlap, touching included.
    pub fn intersects(self, other: Aabb2) -> bool {
        self.max.x() >= other.min.x()
            && self.min.x() <= other.max.x()
            && self.max.y() >= other.min.y()
            && self.min.y() <= other.max.y()
    }

    /// This box grown to contain `p`.
    pub fn united_point(self, p: Point2) -> Aabb2 {
        Aabb2 {
            min: Point2::new(self.min.x().min(p.x()), self.min.y().min(p.y())),
            max: Point2::new(self.max.x().max(p.x()), self.max.y().max(p.y())),
        }
    }

    /// This box grown to contain `other`.
    pub fn united(self, other: Aabb2) -> Aabb2 {
        self.united_point(other.min).united_point(other.max)
    }

    /// Whether `self` lies inside `other` with `slack` to spare on each side.
    pub fn within(&self, other: &Aabb2, slack: f64) -> bool {
        self.min.x() >= other.min.x() - slack
            && self.min.y() >= other.min.y() - slack
            && self.max.x() <= other.max.x() + slack
            && self.max.y() <= other.max.y() + slack
    }
}

/// One projected curve of a view.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectedCurve {
    /// The curve, in view-plane `(u, v)`, model units.
    pub geometry: Curve2,
    pub visibility: Visibility,
    pub kind: CurveKind,
    /// The 3-D entity it came from — an edge for [`CurveKind::Edge`], a face
    /// for a silhouette — or `None` when the kernel cannot name one. See the
    /// module docs for why this is a `KernelId` and not a `GeomRef`.
    pub source: Option<KernelId>,
    /// Where the curve sits in depth, and what hides it — `Some` once D1c has
    /// classified it, `None` for an unclassified wireframe curve.
    pub depth: Option<CurveDepth>,
}

impl ProjectedCurve {
    /// An unclassified curve: visible, with no depth recorded. What the edge
    /// and silhouette passes produce before D1c's classification runs over
    /// them, and what a caller asking only for a wireframe gets.
    pub fn visible(geometry: Curve2, kind: CurveKind, source: Option<KernelId>) -> ProjectedCurve {
        ProjectedCurve {
            geometry,
            visibility: Visibility::Visible,
            kind,
            source,
            depth: None,
        }
    }
}

/// Everything a drawing view needs from the kernel.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViewGeometry {
    pub curves: Vec<ProjectedCurve>,
    /// Exact bounding box of every curve, or `None` when there are none.
    pub bbox: Option<Aabb2>,
    /// What the projection declined to decide while producing these curves.
    pub declines: ProjectionDeclines,
}

impl ViewGeometry {
    /// A view of `curves`, with the bounding box computed from them and no
    /// declines.
    pub fn new(curves: Vec<ProjectedCurve>) -> ViewGeometry {
        ViewGeometry::with_declines(curves, ProjectionDeclines::default())
    }

    /// A view of `curves` with the declines its production accumulated.
    pub fn with_declines(
        curves: Vec<ProjectedCurve>,
        declines: ProjectionDeclines,
    ) -> ViewGeometry {
        let bbox = curves
            .iter()
            .map(|c| c.geometry.bbox())
            .reduce(|a, b| a.united(b));
        ViewGeometry {
            curves,
            bbox,
            declines,
        }
    }

    /// Append another view's curves — how several bodies land in one view.
    /// Both must already be in the same view frame. Declines add up, since
    /// each body's are the same kind of statement about the same view.
    pub fn extend(&mut self, other: ViewGeometry) {
        self.curves.extend(other.curves);
        self.bbox = match (self.bbox, other.bbox) {
            (Some(a), Some(b)) => Some(a.united(b)),
            (a, b) => a.or(b),
        };
        self.declines.merge(&other.declines);
    }

    /// This view placed in paper space: every curve scaled by `scale` about
    /// the view-plane origin and moved by `offset`.
    ///
    /// How several views become ONE drawing (D4a's sheet): each view is
    /// projected in its own frame, then placed, then the placed views are
    /// [`ViewGeometry::extend`]ed together and written as one file. `None`
    /// when the transform is not a positive finite similarity (see
    /// [`Curve2::transformed`]); a curve arm that cannot be transformed is
    /// DROPPED rather than approximated, which the `Option` cannot express —
    /// there is no such arm today, and `transformed` returning `None` for the
    /// whole view is the honest answer if one ever appears.
    ///
    /// [`CurveDepth`] is left in MODEL units deliberately: depth is not a
    /// paper quantity, it is what orders coincident curves, and a positive
    /// uniform scale preserves that order. Scaling it would invent a number
    /// in units nothing reads.
    pub fn transformed(&self, scale: f64, offset: [f64; 2]) -> Option<ViewGeometry> {
        let mut curves = Vec::with_capacity(self.curves.len());
        for c in &self.curves {
            curves.push(ProjectedCurve {
                geometry: c.geometry.transformed(scale, offset)?,
                visibility: c.visibility,
                kind: c.kind,
                source: c.source,
                depth: c.depth,
            });
        }
        Some(ViewGeometry {
            curves,
            bbox: self.bbox.map(|b| Aabb2 {
                min: Point2::new(b.min.x() * scale + offset[0], b.min.y() * scale + offset[1]),
                max: Point2::new(b.max.x() * scale + offset[0], b.max.y() * scale + offset[1]),
            }),
            declines: self.declines,
        })
    }

    /// Total length of every curve tagged `visibility`.
    pub fn total_length(&self, visibility: Visibility) -> f64 {
        self.curves
            .iter()
            .filter(|c| c.visibility == visibility)
            .map(|c| c.geometry.length())
            .sum()
    }
}

/// One body to project into a view: the solid, the name a consumer shows for
/// it, and its world placement (an assembly instance's pose; `None` =
/// identity). The projection sibling of
/// [`StepExportBody`](super::types::StepExportBody).
#[derive(Debug, Clone)]
pub struct ProjectionBody {
    pub handle: KernelSolidHandle,
    pub name: String,
    pub placement: Option<RigidPlacement>,
}

impl ProjectionBody {
    /// One unplaced body.
    pub fn solo(handle: KernelSolidHandle) -> ProjectionBody {
        ProjectionBody {
            handle,
            name: "Body".to_string(),
            placement: None,
        }
    }
}

/// One boundary loop of a section cap (D1d): the curves, closed as a chain,
/// plus the signed area the kernel measured while it still had the loop's
/// traversal direction.
///
/// ## Why the area is stored rather than derived
///
/// A [`Curve2`] arc is normalized counter-clockwise with `start < end` (see
/// [`crate::kernel::projection`]'s module docs), so a curve cannot represent a
/// clockwise traversal — a hole's loop and an outer loop with the same point
/// set are the SAME curve list. The direction survives only in the B-Rep walk
/// the loop was read from, which is inside the kernel. A consumer that
/// re-derived the sign from nesting would be guessing at exactly the
/// configuration (a cap with several outer loops, each with holes) where
/// nesting is the thing it wanted to learn. So the kernel reports the number
/// it measured: **positive for an outer loop, negative for a hole**, in the
/// cap's own `(u, v)` frame.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionLoop {
    /// The loop's curves in B-Rep walk order. Consecutive curves share an
    /// endpoint and the last shares one with the first, but each curve's OWN
    /// direction is the projection's normalization, not the walk's.
    pub curves: Vec<Curve2>,
    /// Green's-theorem area in the cap frame: `+` outer, `−` hole. Exact for
    /// a cap bounded by lines, circular arcs and elliptical arcs; a polyline
    /// edge contributes its chord polygon's area, which is LOW by the
    /// polyline's own sagitta deficit.
    pub signed_area: f64,
    /// Whether every curve of this loop is an exact analytic arm
    /// ([`Curve2::Line`], [`Curve2::Circle`], [`Curve2::Ellipse`]) — so
    /// `signed_area` is exact — or at least one is a sampled
    /// [`Curve2::Polyline`].
    pub exact: bool,
}

/// A planar section cut (D1d): `specs/drawings_and_mbd.md` §5.2 increment 4.
///
/// ## Deviations from §5.1's sketch, and why
///
/// - **`cut_solid` is an `Option`.** The spec writes a bare handle, but a cut
///   that removes ALL the material has no solid to name and kernel-v2 has no
///   empty solid ([`super::types::KernelError`]'s `EmptyBooleanResult` family
///   exists for exactly that). An empty-but-present handle would be a lie, the
///   same argument that made [`ViewGeometry::bbox`] an `Option` at D1a.
/// - **The loops are [`SectionLoop`]s, not bare `Vec<Curve2>`**, so each
///   carries the signed area the kernel measured — see [`SectionLoop`].
/// - **`plane_basis` is reported.** The loops are in "the cut plane's own
///   `(u, v)`" and nothing in the spec's sketch says WHICH `(u, v)`. A
///   consumer that re-derived a frame from the origin and normal would be free
///   to pick a different one and silently rotate the hatch against the view.
///
/// Not `PartialEq`: [`KernelSolidHandle`] is deliberately not comparable (it is
/// a session-scoped opaque id), so an equality on the whole result would have
/// to either ignore the handle or invent an ordering for it.
#[derive(Debug, Clone)]
pub struct SectionResult {
    /// The cap's boundary loops — outer plus inner, hatchable — in
    /// `plane_basis`'s `(u, v)`. Empty when the plane misses the solid.
    pub cap_loops: Vec<SectionLoop>,
    /// The half-space result, for the caller to project — `None` when the cut
    /// keeps no material at all.
    pub cut_solid: Option<KernelSolidHandle>,
    /// The frame `cap_loops` is expressed in: `origin` is the plane origin the
    /// caller passed and the line of sight is the NEGATED plane normal, so the
    /// viewer stands on the discarded side and looks at the cap with the kept
    /// material behind it — the drafting convention, and the frame that makes
    /// an outer loop's `signed_area` positive.
    pub plane_basis: ViewBasis,
    /// Whether at least one cap face was identified by its PLANE rather than
    /// by its descent from the cutting half-space — the §4.5.5 Stage-0
    /// signature of a cut plane COPLANAR with a model face, whose overlap
    /// region the overlay replaces with one shared trimmed surface that may
    /// be attributed to the model operand.
    ///
    /// Carried because a coplanar cut is a legitimate section the drawing must
    /// show, and this is the one configuration where the kernel's own lineage
    /// cannot name the cap: reporting it is the difference between a section
    /// that is known to have gone through Stage 0 and one that silently came
    /// back with no cap at all.
    pub cap_shared_with_model: bool,
}

impl SectionResult {
    /// Net cap area: outer loops minus holes. Zero for an empty section.
    pub fn cap_area(&self) -> f64 {
        self.cap_loops.iter().map(|l| l.signed_area).sum()
    }

    /// Whether every loop's every curve stayed analytic, so [`Self::cap_area`]
    /// is exact.
    pub fn exact(&self) -> bool {
        self.cap_loops.iter().all(|l| l.exact)
    }
}

/// Orthographic projection and planar section of B-Rep solids
/// (`specs/drawings_and_mbd.md` §5.1).
///
/// Every method defaults to a typed [`KernelError::NotSupported`], so the
/// trait is additive for every implementor and an unimplemented increment is
/// loud rather than empty. `kernel_v2::KernelV2Adapter` implements `project`,
/// `project_bodies` and `export_dxf` (D1a); `MockKernel` implements none of
/// them — it has no B-Rep to project, and a trivial answer from a test double
/// would be indistinguishable from a working projection of an empty solid.
pub trait KernelProjection {
    /// Orthographic projection of one solid into `view`: its B-Rep edges
    /// (D1a) followed by its curved faces' silhouettes (D1b).
    fn project(
        &self,
        _solid: &KernelSolidHandle,
        _view: &ViewFrame,
        _opts: &ProjectOpts,
    ) -> Result<ViewGeometry, KernelError> {
        Err(KernelError::NotSupported {
            operation: "project".to_string(),
        })
    }

    /// Several placed bodies projected into ONE view — a multi-body part, or
    /// an assembly's leaf bodies at their world poses.
    fn project_bodies(
        &self,
        _bodies: &[ProjectionBody],
        _view: &ViewFrame,
        _opts: &ProjectOpts,
    ) -> Result<ViewGeometry, KernelError> {
        Err(KernelError::NotSupported {
            operation: "project_bodies".to_string(),
        })
    }

    /// D1d: cut `solid` with the plane through `plane_origin` with unit
    /// `plane_normal`, keeping the half-space the normal points away from
    /// (every kept point satisfies `(p − plane_origin)·plane_normal ≤ 0`), and
    /// report the cap's loops plus the cut solid.
    ///
    /// ## The typed outcomes
    ///
    /// - The plane cuts the solid ⇒ `cap_loops` non-empty and `cut_solid`
    ///   `Some`.
    /// - The plane misses the solid on the KEPT side ⇒ `cap_loops` empty and
    ///   `cut_solid` `Some` (the whole solid survives). Empty, typed, NOT an
    ///   error.
    /// - The plane misses the solid on the DISCARDED side ⇒ `cap_loops` empty
    ///   and `cut_solid` `None`. Also typed, also not an error.
    /// - The kernel's boolean DECLINES — a coplanar input face pair that the
    ///   §4.5.5 Stage-0 overlay cannot resolve, an edge-contact graze, any
    ///   Stage-3/4/5 STOP ⇒ `Err`, naming the kernel's own error. A cut plane
    ///   coplanar with a model face is a legitimate section (the cut passes
    ///   THROUGH a face), so it is attempted and whatever Stage 0 answers is
    ///   what comes back — a result when it resolves, that refusal when it
    ///   does not. Never a silent empty section.
    ///
    /// Takes `&mut self` because the cut solid is a new body in the kernel's
    /// arena: a section produces geometry, unlike [`Self::project`].
    fn section_with_plane(
        &mut self,
        _solid: &KernelSolidHandle,
        _plane_origin: [f64; 3],
        _plane_normal: [f64; 3],
    ) -> Result<SectionResult, KernelError> {
        Err(KernelError::NotSupported {
            operation: "section_with_plane".to_string(),
        })
    }

    /// One view of `bodies` as a DXF drawing (`specs/drawings_and_mbd.md` §8
    /// export, and §12's early deliverable): the flat-pattern file a laser,
    /// waterjet or plasma table consumes, with no sheet, title block or
    /// annotation — together with what the projection DECLINED to decide.
    ///
    /// The declines travel with the file because a DXF is a deliverable and
    /// the counts are the only thing that tells a decided drawing from a
    /// quiet one. Every counter but `cross_body` is a MISSING line — a dashed
    /// arc the drawing does not carry — and a file with tens of thousands of
    /// `ray_grazes_face` is a degenerate view, not a clean one. Dropping them
    /// at the export door would mean a shop drawing could be accepted with no
    /// sign of it.
    fn export_dxf_with_declines(
        &self,
        _bodies: &[ProjectionBody],
        _view: &ViewFrame,
        _opts: &ProjectOpts,
    ) -> Result<(String, ProjectionDeclines), KernelError> {
        Err(KernelError::NotSupported {
            operation: "export_dxf".to_string(),
        })
    }

    /// [`KernelProjection::export_dxf_with_declines`] for a caller with
    /// nowhere to put the declines.
    fn export_dxf(
        &self,
        bodies: &[ProjectionBody],
        view: &ViewFrame,
        opts: &ProjectOpts,
    ) -> Result<String, KernelError> {
        self.export_dxf_with_declines(bodies, view, opts)
            .map(|(text, _)| text)
    }
}

// --- small vector helpers; cad-primitives' Vector3 is storage only ---

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let n = norm(a);
    if !(n.is_finite() && n > super::units::TAU_NORMALIZE) {
        return None;
    }
    Some([a[0] / n, a[1] / n, a[2] / n])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{PI, TAU};

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-12 * (1.0 + a.abs().max(b.abs()))
    }

    #[test]
    fn the_top_view_puts_x_right_and_y_up() {
        let b = ViewFrame::TOP.basis().expect("top view is well formed");
        assert_eq!(b.u, [1.0, 0.0, 0.0]);
        assert_eq!(b.v, [0.0, 1.0, 0.0]);
        assert_eq!(b.w, [0.0, 0.0, -1.0]);
        // A point above the plane is NEARER the viewer: negative depth.
        let (uv, depth) = b.project([2.0, 3.0, 5.0]);
        assert_eq!(uv.as_array(), [2.0, 3.0]);
        assert_eq!(depth, -5.0);
    }

    #[test]
    fn the_named_views_are_right_handed_and_consistent() {
        for frame in [ViewFrame::TOP, ViewFrame::FRONT, ViewFrame::RIGHT] {
            let b = frame.basis().expect("named view is well formed");
            // (u, v, -w) right-handed: u × v = -w.
            let uv = cross(b.u, b.v);
            for (i, c) in uv.iter().enumerate() {
                assert!(close(*c, -b.w[i]), "{frame:?} is left-handed");
            }
            assert!(close(dot(b.u, b.v), 0.0));
            assert!(close(dot(b.u, b.w), 0.0));
            assert!(close(norm(b.u), 1.0));
        }
    }

    #[test]
    fn a_degenerate_frame_has_no_basis() {
        assert!(ViewFrame {
            origin: [0.0; 3],
            dir: [0.0; 3],
            up: [0.0, 0.0, 1.0],
        }
        .basis()
        .is_none());
        assert!(ViewFrame {
            origin: [0.0; 3],
            dir: [0.0, 0.0, 1.0],
            up: [0.0, 0.0, -2.0],
        }
        .basis()
        .is_none());
    }

    #[test]
    fn from_parts_is_the_frame_an_optional_direction_and_up_mean() {
        assert_eq!(ViewFrame::from_parts(None, None), ViewFrame::TOP);
        // Without a direction there is no paper to orient, so a lone `up` is
        // not a view: TOP keeps its own.
        assert_eq!(
            ViewFrame::from_parts(None, Some([1.0, 0.0, 0.0])),
            ViewFrame::TOP
        );
        let f = ViewFrame::from_parts(Some([0.0, 1.0, 0.0]), Some([0.0, 0.0, 1.0]));
        assert_eq!((f.dir, f.up), ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]));
        // An explicit up WINS over the one `looking_along` would pick.
        let f = ViewFrame::from_parts(Some([0.0, 0.0, -1.0]), Some([1.0, 0.0, 0.0]));
        assert_eq!(f.up, [1.0, 0.0, 0.0]);
        assert_eq!(
            ViewFrame::from_parts(Some([0.0, 0.0, -1.0]), None).up,
            ViewFrame::looking_along([0.0, 0.0, -1.0]).up
        );
        // The degenerate pair a caller must be told about rather than handed
        // to the kernel.
        assert!(
            ViewFrame::from_parts(Some([0.0, 1.0, 0.0]), Some([0.0, -3.0, 0.0]))
                .basis()
                .is_none()
        );
    }

    #[test]
    fn looking_along_picks_an_up_that_is_never_parallel() {
        for dir in [
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
            [1.0, 0.0, 0.0],
            [0.3, -0.4, 0.5],
        ] {
            assert!(
                ViewFrame::looking_along(dir).basis().is_some(),
                "no basis looking along {dir:?}"
            );
        }
    }

    #[test]
    fn a_body_frame_basis_reproduces_the_placed_projection() {
        let basis = ViewFrame::FRONT.basis().expect("front view");
        let placement = RigidPlacement {
            translation: [0.5, -1.25, 3.0],
            rotation: RigidPlacement::rotation_matrix([0.0, 0.0, 1.0], 0.7),
        };
        let local = basis.in_body_frame(&placement);
        for p in [[0.0, 0.0, 0.0], [1.0, 2.0, -3.0], [-0.25, 0.5, 0.125]] {
            let (direct, dd) = basis.project(placement.apply(p));
            let (via, vd) = local.project(p);
            assert!(close(direct.x(), via.x()), "{p:?}: u");
            assert!(close(direct.y(), via.y()), "{p:?}: v");
            assert!(close(dd, vd), "{p:?}: depth");
        }
    }

    #[test]
    fn a_quarter_arcs_bbox_is_not_its_whole_circle() {
        let arc = Curve2::Circle {
            center: Point2::new(0.0, 0.0),
            radius: 2.0,
            start_angle: 0.0,
            end_angle: PI / 2.0,
        };
        let bb = arc.bbox();
        assert!(close(bb.min.x(), 0.0) && close(bb.min.y(), 0.0));
        assert!(close(bb.max.x(), 2.0) && close(bb.max.y(), 2.0));

        let full = Curve2::Circle {
            center: Point2::new(1.0, 1.0),
            radius: 2.0,
            start_angle: 0.0,
            end_angle: TAU,
        };
        let bb = full.bbox();
        assert!(close(bb.min.x(), -1.0) && close(bb.min.y(), -1.0));
        assert!(close(bb.max.x(), 3.0) && close(bb.max.y(), 3.0));
    }

    #[test]
    fn an_axis_aligned_ellipses_bbox_is_its_radii() {
        let e = Curve2::Ellipse {
            center: Point2::new(0.0, 0.0),
            major_axis: [1.0, 0.0],
            major_radius: 3.0,
            minor_radius: 1.0,
            start_param: 0.0,
            end_param: TAU,
        };
        let bb = e.bbox();
        assert!(close(bb.min.x(), -3.0) && close(bb.max.x(), 3.0));
        assert!(close(bb.min.y(), -1.0) && close(bb.max.y(), 1.0));
    }

    #[test]
    fn a_rotated_ellipses_bbox_matches_a_dense_sampling() {
        let e = Curve2::Ellipse {
            center: Point2::new(0.5, -0.25),
            major_axis: [0.6, 0.8],
            major_radius: 3.0,
            minor_radius: 1.5,
            start_param: 0.3,
            end_param: 0.3 + 2.1,
        };
        let bb = e.bbox();
        let mut sampled = Aabb2::point(e.eval(0.3).expect("eval"));
        for i in 0..=20_000 {
            let t = 0.3 + 2.1 * (i as f64) / 20_000.0;
            sampled = sampled.united_point(e.eval(t).expect("eval"));
        }
        for (a, b) in [
            (bb.min.x(), sampled.min.x()),
            (bb.min.y(), sampled.min.y()),
            (bb.max.x(), sampled.max.x()),
            (bb.max.y(), sampled.max.y()),
        ] {
            assert!((a - b).abs() < 1e-6, "exact {a} vs sampled {b}");
        }
        assert!(sampled.within(&bb, 1e-9), "the exact box must contain it");
    }

    #[test]
    fn lengths_are_exact_where_a_closed_form_exists() {
        assert_eq!(Curve2::Point(Point2::new(1.0, 2.0)).length(), 0.0);
        assert!(close(
            Curve2::Line {
                start: Point2::new(0.0, 0.0),
                end: Point2::new(3.0, 4.0),
            }
            .length(),
            5.0
        ));
        assert!(close(
            Curve2::Circle {
                center: Point2::new(0.0, 0.0),
                radius: 2.0,
                start_angle: 0.0,
                end_angle: TAU,
            }
            .length(),
            2.0 * TAU
        ));
        // A circle is the aspect-ratio-1 ellipse; the quadrature must agree.
        let circle_as_ellipse = Curve2::Ellipse {
            center: Point2::new(0.0, 0.0),
            major_axis: [1.0, 0.0],
            major_radius: 2.0,
            minor_radius: 2.0,
            start_param: 0.0,
            end_param: TAU,
        };
        assert!((circle_as_ellipse.length() - 2.0 * TAU).abs() < 1e-10);
    }

    /// Q6's convergence witness: doubling the step count moves the ellipse
    /// arm and nothing else, and the movement is the size the residual is
    /// reported as.
    #[test]
    fn only_the_ellipse_arm_reads_the_step_count() {
        for closed_form in [
            Curve2::Point(Point2::new(1.0, 2.0)),
            Curve2::Line {
                start: Point2::new(0.0, 0.0),
                end: Point2::new(3.0, 4.0),
            },
            Curve2::Circle {
                center: Point2::new(0.0, 0.0),
                radius: 2.0,
                start_angle: 0.0,
                end_angle: TAU,
            },
            Curve2::Polyline {
                points: vec![Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)],
                closed: false,
            },
        ] {
            assert_eq!(
                closed_form.length(),
                closed_form.length_with_steps(8),
                "a closed form must ignore the step count: {closed_form:?}"
            );
        }

        // A well-conditioned ellipse: `length()` already sits at f64
        // resolution, so the witness is tiny but it is MEASURED, not assumed.
        let e = Curve2::Ellipse {
            center: Point2::new(0.0, 0.0),
            major_axis: [1.0, 0.0],
            major_radius: 2.0,
            minor_radius: 1.0,
            start_param: 0.0,
            end_param: TAU,
        };
        let residual = (e.length_with_steps(2 * ELLIPSE_QUADRATURE_STEPS) - e.length()).abs();
        assert!(
            residual < 1e-12,
            "a 2:1 ellipse's quadrature has converged: residual {residual}"
        );
        // A coarse quadrature of the SAME curve has not, which is what makes
        // the witness worth reporting at all.
        let coarse = (e.length_with_steps(8) - e.length()).abs();
        assert!(
            coarse > residual,
            "8 intervals must be visibly worse than {ELLIPSE_QUADRATURE_STEPS}: {coarse}"
        );
        // An odd or absurd step count is rounded into Simpson's domain rather
        // than dividing by zero or integrating half the curve.
        assert!(e.length_with_steps(0).is_finite());
        assert_eq!(e.length_with_steps(7), e.length_with_steps(6));
    }

    /// What the ellipse quadrature actually delivers, against a far finer
    /// reference, so the doc comment's numbers are anchored and a change that
    /// silently worsens them is caught.
    ///
    /// The pairs are `(minor radius, the relative error to allow)` for a unit
    /// major radius. The point is the SHAPE of the curve: accuracy falls off
    /// as the ellipse flattens, because `|dP/dt|` tends to `|sin t|` and
    /// Simpson converges slowly near that cusp.
    #[test]
    fn the_ellipse_quadratures_accuracy_falls_off_with_the_aspect_ratio() {
        // A reference integral of the same speed function, far finer.
        let reference = |minor: f64| -> f64 {
            let speed = |t: f64| (t.sin() * t.sin() + minor * minor * t.cos() * t.cos()).sqrt();
            let n = 2_000_000usize;
            let h = TAU / n as f64;
            let mut acc = speed(0.0) + speed(TAU);
            for i in 1..n {
                acc += if i % 2 == 1 { 4.0 } else { 2.0 } * speed(h * i as f64);
            }
            acc * h / 3.0
        };
        for (minor, allow) in [(1.0, 1e-15), (0.05, 1e-13), (1e-2, 1e-10), (1e-3, 1e-6)] {
            let e = Curve2::Ellipse {
                center: Point2::new(0.0, 0.0),
                major_axis: [1.0, 0.0],
                major_radius: 1.0,
                minor_radius: minor,
                start_param: 0.0,
                end_param: TAU,
            };
            let want = reference(minor);
            let rel = (e.length() - want).abs() / want;
            assert!(
                rel <= allow,
                "aspect {}: relative error {rel} over the allowed {allow}",
                1.0 / minor
            );
        }

        // DETERMINISM is the separate property the invariance oracles rest
        // on, and it survives the inaccuracy: the speed function is
        // π-periodic, so the same ellipse parameterized half a turn along —
        // which is what a 180° view rotation produces — integrates to the
        // SAME number, not merely a close one.
        let flat = |start: f64, axis: [f64; 2]| Curve2::Ellipse {
            center: Point2::new(0.0, 0.0),
            major_axis: axis,
            major_radius: 1.0,
            minor_radius: 1e-3,
            start_param: start,
            end_param: start + TAU,
        };
        assert_eq!(
            flat(0.0, [1.0, 0.0]).length(),
            flat(PI, [-1.0, 0.0]).length(),
            "a half turn must give the same number, bit for bit"
        );
    }

    #[test]
    fn a_polylines_length_counts_its_closing_chord_only_when_closed() {
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
        ];
        assert!(close(
            Curve2::Polyline {
                points: pts.clone(),
                closed: false,
            }
            .length(),
            2.0
        ));
        assert!(close(
            Curve2::Polyline {
                points: pts,
                closed: true,
            }
            .length(),
            2.0 + 2f64.sqrt()
        ));
    }

    #[test]
    fn flatten_holds_the_sagitta_it_promises() {
        let sagitta = 1e-4;
        for curve in [
            Curve2::Circle {
                center: Point2::new(0.0, 0.0),
                radius: 1.0,
                start_angle: 0.0,
                end_angle: TAU,
            },
            Curve2::Ellipse {
                center: Point2::new(0.0, 0.0),
                major_axis: [0.6, 0.8],
                major_radius: 1.0,
                minor_radius: 0.2,
                start_param: 0.0,
                end_param: TAU,
            },
        ] {
            let pts = curve.flatten(sagitta);
            assert!(pts.len() > 4, "{} points", pts.len());
            // Midpoint of each chord must be within `sagitta` of the curve.
            let (t0, t1) = curve.param_range().expect("range");
            let n = pts.len() - 1;
            for i in 0..n {
                let tm = t0 + (t1 - t0) * (i as f64 + 0.5) / (n as f64);
                let on = curve.eval(tm).expect("eval");
                let mid = Point2::new(
                    0.5 * (pts[i].x() + pts[i + 1].x()),
                    0.5 * (pts[i].y() + pts[i + 1].y()),
                );
                let d = ((on.x() - mid.x()).powi(2) + (on.y() - mid.y()).powi(2)).sqrt();
                assert!(d <= sagitta * 1.001, "deviation {d} over {sagitta}");
            }
        }
    }

    #[test]
    fn flatten_passes_the_other_arms_through_unchanged() {
        let p = Point2::new(1.0, 2.0);
        assert_eq!(Curve2::Point(p).flatten(1e-3), vec![p]);
        let q = Point2::new(3.0, 4.0);
        assert_eq!(Curve2::Line { start: p, end: q }.flatten(1e-3), vec![p, q]);
        assert_eq!(
            Curve2::Polyline {
                points: vec![p, q],
                closed: true,
            }
            .flatten(1e-3),
            vec![p, q]
        );
    }

    #[test]
    fn closure_and_endpoints_agree() {
        let full = Curve2::Circle {
            center: Point2::new(0.0, 0.0),
            radius: 1.0,
            start_angle: 0.0,
            end_angle: TAU,
        };
        assert!(full.is_closed());
        assert!(full.endpoints().is_none());

        let arc = Curve2::Circle {
            center: Point2::new(0.0, 0.0),
            radius: 1.0,
            start_angle: 0.0,
            end_angle: PI,
        };
        assert!(!arc.is_closed());
        let (a, b) = arc.endpoints().expect("an arc has endpoints");
        assert!(close(a.x(), 1.0) && close(a.y(), 0.0));
        assert!(close(b.x(), -1.0) && b.y().abs() < 1e-15);
    }

    #[test]
    fn a_view_geometrys_bbox_is_the_union_of_its_curves() {
        let mk = |c: Curve2| ProjectedCurve::visible(c, CurveKind::Edge, None);
        let empty = ViewGeometry::new(Vec::new());
        assert!(empty.bbox.is_none());
        assert_eq!(empty.total_length(Visibility::Visible), 0.0);

        let mut vg = ViewGeometry::new(vec![mk(Curve2::Line {
            start: Point2::new(0.0, 0.0),
            end: Point2::new(1.0, 0.0),
        })]);
        vg.extend(ViewGeometry::new(vec![mk(Curve2::Line {
            start: Point2::new(-2.0, 3.0),
            end: Point2::new(-2.0, 4.0),
        })]));
        let bb = vg.bbox.expect("two curves have a box");
        assert_eq!(bb.min.as_array(), [-2.0, 0.0]);
        assert_eq!(bb.max.as_array(), [1.0, 4.0]);
        assert!(close(vg.total_length(Visibility::Visible), 2.0));
        assert_eq!(vg.total_length(Visibility::Hidden), 0.0);
    }

    #[test]
    fn segment_count_refuses_to_degenerate() {
        assert_eq!(segment_count(0.0, TAU, 1e-3), 1);
        assert_eq!(segment_count(1.0, 0.0, 1e-3), 1);
        // A sagitta larger than the radius would ask for a 2-gon.
        assert!(segment_count(1.0, TAU, 10.0) >= 4);
        assert!(segment_count(1.0, TAU, 1e-6) > 1000);
    }

    #[test]
    fn default_opts_ask_for_the_kernels_own_density() {
        assert_eq!(ProjectOpts::default().rel_chord_tolerance, None);
    }

    // --- D1c: the parameterization, and the splitting primitive over it ---

    #[test]
    fn every_arm_is_parameterized_and_its_domain_evaluates() {
        let p = Point2::new(1.0, 2.0);
        let cases = [
            Curve2::Point(p),
            Curve2::Line {
                start: p,
                end: Point2::new(4.0, 6.0),
            },
            Curve2::Circle {
                center: p,
                radius: 2.0,
                start_angle: 0.3,
                end_angle: 1.7,
            },
            Curve2::Ellipse {
                center: p,
                major_axis: [0.6, 0.8],
                major_radius: 3.0,
                minor_radius: 1.0,
                start_param: -0.5,
                end_param: 2.0,
            },
            Curve2::Polyline {
                points: vec![p, Point2::new(2.0, 2.0), Point2::new(2.0, 5.0)],
                closed: false,
            },
            Curve2::Polyline {
                points: vec![p, Point2::new(2.0, 2.0), Point2::new(2.0, 5.0)],
                closed: true,
            },
        ];
        for c in &cases {
            let (t0, t1) = c.param_range().expect("every arm has a domain");
            assert!(t1 >= t0, "{c:?}: empty domain {t0}..{t1}");
            assert!(c.eval(t0).is_some() && c.eval(t1).is_some(), "{c:?}");
        }
        // The polyline's integer parameters ARE its vertices, which is what
        // makes a sub-polyline exact rather than resampled.
        let line = Curve2::Polyline {
            points: vec![p, Point2::new(2.0, 2.0), Point2::new(2.0, 5.0)],
            closed: false,
        };
        assert_eq!(line.param_range(), Some((0.0, 2.0)));
        assert_eq!(line.eval(1.0), Some(Point2::new(2.0, 2.0)));
        assert_eq!(line.eval(1.5), Some(Point2::new(2.0, 3.5)));
        // Closed: one more chord, back to the first point.
        let ring = Curve2::Polyline {
            points: vec![p, Point2::new(2.0, 2.0), Point2::new(2.0, 5.0)],
            closed: true,
        };
        assert_eq!(ring.param_range(), Some((0.0, 3.0)));
        assert_eq!(ring.eval(3.0), Some(p));
        // The degenerate ends, which must answer rather than panic.
        assert!(Curve2::Polyline {
            points: Vec::new(),
            closed: false
        }
        .param_range()
        .is_none());
        let single = Curve2::Polyline {
            points: vec![p],
            closed: true,
        };
        assert_eq!(single.param_range(), Some((0.0, 0.0)));
        assert_eq!(single.eval(7.0), Some(p));
    }

    /// One of each arm, for the transform tests below.
    fn one_of_each_arm() -> Vec<Curve2> {
        let p = Point2::new(1.0, 2.0);
        vec![
            Curve2::Point(p),
            Curve2::Line {
                start: p,
                end: Point2::new(4.0, 6.0),
            },
            Curve2::Circle {
                center: p,
                radius: 2.0,
                start_angle: 0.3,
                end_angle: 1.7,
            },
            Curve2::Ellipse {
                center: p,
                major_axis: [0.6, 0.8],
                major_radius: 3.0,
                minor_radius: 1.0,
                start_param: -0.5,
                end_param: 2.0,
            },
            Curve2::Polyline {
                points: vec![p, Point2::new(2.0, 2.0), Point2::new(2.0, 5.0)],
                closed: true,
            },
        ]
    }

    #[test]
    fn a_paper_transform_maps_every_arm_to_its_own_kind_and_moves_every_point() {
        // D4a places a view on a sheet by scaling and translating it. The
        // claim the sheet export rests on is that this is a SIMILARITY: every
        // arm survives as the same arm, so the analytic entities (a DXF
        // CIRCLE, an ARC) reach the file instead of being flattened on the
        // way. Asserted point-wise through `eval`, not on the fields, so a
        // transform that got a radius right and a centre wrong still fails.
        let (scale, offset) = (0.25, [10.0, -4.0]);
        for curve in one_of_each_arm() {
            let moved = curve
                .transformed(scale, offset)
                .expect("a positive finite similarity applies to every arm");
            assert_eq!(
                std::mem::discriminant(&moved),
                std::mem::discriminant(&curve),
                "{curve:?} changed arm under a similarity"
            );
            let (t0, t1) = curve.param_range().unwrap();
            for i in 0..=8 {
                let t = t0 + (t1 - t0) * (i as f64) / 8.0;
                let a = curve.eval(t).unwrap();
                let b = moved.eval(t).unwrap();
                assert!(
                    (b.x() - (a.x() * scale + offset[0])).abs() < 1e-12
                        && (b.y() - (a.y() * scale + offset[1])).abs() < 1e-12,
                    "{curve:?} at t={t}: {a:?} should map to {b:?}"
                );
            }
            // And the length scales with it, which is the property a drawn
            // dimension's paper length depends on.
            assert!(
                (moved.length() - curve.length() * scale).abs() < 1e-12,
                "{curve:?}: length {} should be {}",
                moved.length(),
                curve.length() * scale
            );
        }
    }

    #[test]
    fn a_paper_transform_refuses_a_scale_that_is_not_a_positive_finite_number() {
        // A zero or negative scale is not a drawing scale: zero collapses the
        // view to a point and a negative one mirrors it, which on a
        // manufacturing drawing is a part machined the wrong way round. The
        // ratio is refused here rather than normalized to its absolute value.
        let line = Curve2::Line {
            start: Point2::new(0.0, 0.0),
            end: Point2::new(1.0, 0.0),
        };
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(
                line.transformed(bad, [0.0, 0.0]).is_none(),
                "scale {bad} should be refused"
            );
        }
        assert!(line.transformed(1.0, [f64::NAN, 0.0]).is_none());
    }

    #[test]
    fn a_placed_view_keeps_its_declines_and_rebuilds_its_box() {
        // The sheet composes placed views; the declines are a statement about
        // what the PROJECTION declined, so placing a view on paper cannot
        // change them, and the box has to follow the curves or the sheet's
        // auto-layout places the view by a stale extent.
        let mut view = ViewGeometry::with_declines(
            vec![ProjectedCurve::visible(
                Curve2::Line {
                    start: Point2::new(0.0, 0.0),
                    end: Point2::new(2.0, 1.0),
                },
                CurveKind::Edge,
                None,
            )],
            ProjectionDeclines {
                ray_grazes_face: 7,
                ..ProjectionDeclines::default()
            },
        );
        view.curves[0].depth = Some(CurveDepth {
            at_midpoint: 0.5,
            occluder: None,
        });
        let placed = view.transformed(0.5, [100.0, 200.0]).expect("places");
        assert_eq!(placed.declines.ray_grazes_face, 7);
        let bbox = placed.bbox.expect("a placed curve has a box");
        assert_eq!((bbox.min.x(), bbox.min.y()), (100.0, 200.0));
        assert_eq!((bbox.max.x(), bbox.max.y()), (101.0, 200.5));
        // Depth is model-space and is NOT scaled: it orders curves, and the
        // order is what a positive scale preserves.
        assert_eq!(placed.curves[0].depth.unwrap().at_midpoint, 0.5);
    }

    #[test]
    fn a_subcurve_keeps_the_curves_own_kind() {
        let arc = Curve2::Circle {
            center: Point2::new(1.0, 1.0),
            radius: 2.0,
            start_angle: 0.0,
            end_angle: TAU,
        };
        let half = arc.subcurve(0.0, PI).expect("half of a circle");
        match half {
            Curve2::Circle {
                center,
                radius,
                start_angle,
                end_angle,
            } => {
                assert_eq!((center.x(), center.y()), (1.0, 1.0));
                assert_eq!(radius, 2.0);
                assert_eq!((start_angle, end_angle), (0.0, PI));
            }
            other => panic!("half a circle is a circular arc, got {other:?}"),
        }
        assert!(!half.is_closed());
        assert!(close(half.length(), 2.0 * PI));

        let e = Curve2::Ellipse {
            center: Point2::new(0.0, 0.0),
            major_axis: [1.0, 0.0],
            major_radius: 3.0,
            minor_radius: 1.0,
            start_param: 0.0,
            end_param: TAU,
        };
        assert!(matches!(
            e.subcurve(1.0, 2.0),
            Some(Curve2::Ellipse {
                start_param,
                end_param,
                major_radius,
                ..
            }) if start_param == 1.0 && end_param == 2.0 && major_radius == 3.0
        ));

        let l = Curve2::Line {
            start: Point2::new(0.0, 0.0),
            end: Point2::new(4.0, 0.0),
        };
        assert_eq!(
            l.subcurve(0.25, 0.75),
            Some(Curve2::Line {
                start: Point2::new(1.0, 0.0),
                end: Point2::new(3.0, 0.0),
            })
        );
    }

    #[test]
    fn a_polyline_subcurve_keeps_the_interior_vertices_it_spans() {
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(3.0, 0.0),
        ];
        let pl = Curve2::Polyline {
            points: pts,
            closed: false,
        };
        let Some(Curve2::Polyline { points, closed }) = pl.subcurve(0.5, 2.5) else {
            panic!("a polyline's sub-curve is a polyline");
        };
        assert!(!closed, "a sub-curve of anything is open");
        // The two interpolated ends plus the two vertices strictly inside.
        assert_eq!(
            points,
            vec![
                Point2::new(0.5, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(2.0, 0.0),
                Point2::new(2.5, 0.0),
            ]
        );
        assert!(close(pl.subcurve(0.5, 2.5).expect("sub").length(), 2.0));
    }

    #[test]
    fn a_subcurve_of_the_whole_domain_is_the_curve_itself() {
        let ring = Curve2::Polyline {
            points: vec![
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(1.0, 1.0),
            ],
            closed: true,
        };
        assert_eq!(ring.subcurve(0.0, 3.0).as_ref(), Some(&ring));
        // And a request wider than the domain is clamped to it, not refused.
        assert_eq!(ring.subcurve(-1.0, 99.0).as_ref(), Some(&ring));
    }

    #[test]
    fn a_degenerate_subcurve_is_refused_rather_than_zero_length() {
        let l = Curve2::Line {
            start: Point2::new(0.0, 0.0),
            end: Point2::new(1.0, 0.0),
        };
        assert!(l.subcurve(0.5, 0.5).is_none());
        assert!(l.subcurve(0.7, 0.3).is_none());
        assert!(l.subcurve(f64::NAN, 1.0).is_none());
        // A point has nothing to cut.
        assert!(Curve2::Point(Point2::new(1.0, 1.0))
            .subcurve(0.0, 0.0)
            .is_none());
    }

    // ─────────────────────────────────────────────── D4c: the disc clip

    /// A disc centred on the origin, radius 1 — the fixture every clip test
    /// below uses, so the arithmetic is readable.
    const UNIT: ([f64; 2], f64) = ([0.0, 0.0], 1.0);

    fn clip(curve: &Curve2) -> Vec<Curve2> {
        curve.clipped_to_disc(UNIT.0, UNIT.1, 1e-4)
    }

    #[test]
    fn a_line_crossing_the_disc_is_clipped_to_a_line_at_the_two_crossings() {
        // The headline claim: the piece keeps its KIND, so a detail view's
        // DXF carries a `LINE` and not a chord polyline of one.
        let l = Curve2::Line {
            start: Point2::new(-3.0, 0.0),
            end: Point2::new(3.0, 0.0),
        };
        let pieces = clip(&l);
        assert_eq!(pieces.len(), 1);
        let Curve2::Line { start, end } = &pieces[0] else {
            panic!("a clipped line must still be a line: {:?}", pieces[0]);
        };
        assert!(close(start.x(), -1.0) && close(start.y(), 0.0), "{start:?}");
        assert!(close(end.x(), 1.0) && close(end.y(), 0.0), "{end:?}");

        // Wholly inside: unchanged, not re-derived.
        let inside = Curve2::Line {
            start: Point2::new(-0.5, 0.0),
            end: Point2::new(0.5, 0.0),
        };
        assert_eq!(clip(&inside), vec![inside.clone()]);

        // Wholly outside: nothing. Including a line whose INFINITE extension
        // would cross — the roots are outside `(0, 1)` and are discarded.
        for away in [
            Curve2::Line {
                start: Point2::new(2.0, 2.0),
                end: Point2::new(3.0, 3.0),
            },
            Curve2::Line {
                start: Point2::new(2.0, 0.0),
                end: Point2::new(3.0, 0.0),
            },
        ] {
            assert!(clip(&away).is_empty(), "{away:?}");
        }

        // A TANGENT line is outside everywhere but the one point it touches,
        // so NOTHING of it survives the crop — see
        // `a_curve_that_only_touches_the_disc_is_clipped_away_whatever_its_midpoint`,
        // which is where the rule and its three other shapes are pinned. An
        // inequality here (`len() <= 1`) would pass on the answer "the whole
        // four-unit line, inside a disc of radius one".
        let tangent = Curve2::Line {
            start: Point2::new(-2.0, 1.0),
            end: Point2::new(2.0, 1.0),
        };
        assert!(clip(&tangent).is_empty(), "{:?}", clip(&tangent));
    }

    #[test]
    fn an_arc_clipped_by_the_disc_stays_an_arc_of_the_same_circle() {
        // A circle of radius 1 centred at (1, 0): it passes through the
        // origin and through (2, 0), so exactly half of it — the half with
        // `x < 0.5` — lies inside the unit disc. The two circles meet where
        // `x = 1/2`, i.e. at `t = ±2π/3` on the moved circle.
        let c = Curve2::Circle {
            center: Point2::new(1.0, 0.0),
            radius: 1.0,
            start_angle: 0.0,
            end_angle: TAU,
        };
        let pieces = clip(&c);
        // A full turn crossing the boundary comes back as the pieces its own
        // seam and the two crossings cut it into; the seam at `t = 0` is
        // OUTSIDE the disc here (the point (2, 0)), so there is exactly one.
        assert_eq!(pieces.len(), 1, "{pieces:?}");
        let Curve2::Circle {
            center,
            radius,
            start_angle,
            end_angle,
        } = &pieces[0]
        else {
            panic!("a clipped arc must still be an arc: {:?}", pieces[0]);
        };
        assert!(close(center.x(), 1.0) && close(*radius, 1.0));
        // 2π/3 … 4π/3 — the half of the circle nearer the origin.
        assert!(
            close(*start_angle, 2.0 * PI / 3.0) && close(*end_angle, 4.0 * PI / 3.0),
            "got {start_angle} … {end_angle}"
        );
        // Both endpoints sit ON the crop boundary, which is the test that the
        // roots are the real crossings and not an approximation of them.
        for t in [*start_angle, *end_angle] {
            let p = pieces[0].eval(t).expect("an arc evaluates");
            assert!(
                close(p.x() * p.x() + p.y() * p.y(), 1.0),
                "the cut landed at {p:?}, which is not on the unit circle"
            );
        }

        // A circle entirely inside comes back whole — not as an arc of
        // itself, which would put a seam in a hole's rim on the sheet.
        let small = Curve2::Circle {
            center: Point2::new(0.0, 0.0),
            radius: 0.5,
            start_angle: 0.0,
            end_angle: TAU,
        };
        assert_eq!(clip(&small), vec![small.clone()]);
        // Concentric and LARGER: outside, and the concentric case has no
        // crossing to find, so it must be decided by the midpoint.
        let big = Curve2::Circle {
            center: Point2::new(0.0, 0.0),
            radius: 2.0,
            start_angle: 0.0,
            end_angle: TAU,
        };
        assert!(clip(&big).is_empty());
    }

    #[test]
    fn a_full_circle_cut_at_its_own_seam_comes_back_as_two_arcs() {
        // The documented consequence: the seam is a parameter boundary like
        // any other, so a circle whose `t = 0` point is INSIDE the disc comes
        // back as two arcs that happen to meet there. Two entities where one
        // would do — correct geometry, and joining them would mean reasoning
        // about the seam for the sake of an entity count.
        let c = Curve2::Circle {
            center: Point2::new(1.0, 0.0),
            radius: 1.5,
            start_angle: -PI,
            end_angle: PI,
        };
        // `t = ±π` is the point (−0.5, 0), inside the unit disc.
        let seam = c.eval(PI).expect("evaluates");
        assert!(seam.x() * seam.x() + seam.y() * seam.y() < 1.0);
        let pieces = clip(&c);
        assert_eq!(pieces.len(), 2, "{pieces:?}");
        for piece in &pieces {
            assert!(matches!(piece, Curve2::Circle { .. }));
            for t in [
                piece.param_range().unwrap().0,
                piece.param_range().unwrap().1,
            ] {
                let p = piece.eval(t).expect("evaluates");
                let r2 = p.x() * p.x() + p.y() * p.y();
                // Each end is either on the crop boundary or at the seam.
                assert!(
                    close(r2, 1.0) || close(t.abs(), PI),
                    "a piece ends at {p:?} (r² = {r2}) at t = {t}"
                );
            }
        }
    }

    #[test]
    fn a_polyline_is_clipped_chord_by_chord_into_the_pieces_inside() {
        // A zig-zag that enters, leaves and re-enters: three chords, two
        // pieces, and the interior vertex of each piece is kept (that is
        // `subcurve`'s contract, exercised through the clip).
        let p = Curve2::Polyline {
            points: vec![
                Point2::new(-3.0, 0.0),
                Point2::new(0.0, 0.0),
                Point2::new(0.0, 3.0),
                Point2::new(3.0, 3.0),
            ],
            closed: false,
        };
        let pieces = clip(&p);
        assert_eq!(pieces.len(), 1, "{pieces:?}");
        let Curve2::Polyline { points, closed } = &pieces[0] else {
            panic!("a clipped polyline is a polyline: {:?}", pieces[0]);
        };
        assert!(!closed, "a clipped piece of a polyline is open");
        // (−1, 0) … (0, 0) … (0, 1): the two crossings with the vertex
        // between them.
        assert_eq!(points.len(), 3, "{points:?}");
        assert!(close(points[0].x(), -1.0) && close(points[0].y(), 0.0));
        assert!(close(points[1].x(), 0.0) && close(points[1].y(), 0.0));
        assert!(close(points[2].x(), 0.0) && close(points[2].y(), 1.0));

        // A closed polyline straddling the disc: every piece is open, and
        // together they stay inside.
        let square = Curve2::Polyline {
            points: vec![
                Point2::new(-2.0, -0.5),
                Point2::new(2.0, -0.5),
                Point2::new(2.0, 0.5),
                Point2::new(-2.0, 0.5),
            ],
            closed: true,
        };
        let pieces = clip(&square);
        assert_eq!(pieces.len(), 2, "{pieces:?}");
        for piece in &pieces {
            for t in [
                piece.param_range().unwrap().0,
                piece.param_range().unwrap().1,
            ] {
                let q = piece.eval(t).expect("evaluates");
                assert!(
                    q.x() * q.x() + q.y() * q.y() <= 1.0 + 1e-9,
                    "a piece reaches {q:?}, outside the crop"
                );
            }
        }
    }

    #[test]
    fn an_ellipse_is_clipped_as_the_polyline_the_writer_would_flatten_it_to() {
        // The one arm with no closed form. The answer is still inside the
        // disc and still spans it, which is what a detail view needs; what it
        // is NOT is an analytic ellipse arc, and that costs nothing because
        // R12 has no `ELLIPSE` entity to carry one.
        let e = Curve2::Ellipse {
            center: Point2::new(0.0, 0.0),
            major_axis: [1.0, 0.0],
            major_radius: 3.0,
            minor_radius: 0.5,
            start_param: 0.0,
            end_param: TAU,
        };
        let pieces = clip(&e);
        assert!(!pieces.is_empty(), "the ellipse crosses the disc");
        assert!(
            pieces.iter().all(|p| matches!(p, Curve2::Polyline { .. })),
            "{pieces:?}"
        );
        for piece in &pieces {
            for p in piece.flatten(1e-4) {
                assert!(
                    p.x() * p.x() + p.y() * p.y() <= 1.0 + 1e-6,
                    "a clipped ellipse piece reaches {p:?}, outside the crop"
                );
            }
        }
        // An ellipse wholly inside survives as a polyline too — the honest
        // consequence of flattening the arm, and the reason the sagitta is
        // the caller's to choose.
        let small = Curve2::Ellipse {
            center: Point2::new(0.0, 0.0),
            major_axis: [1.0, 0.0],
            major_radius: 0.5,
            minor_radius: 0.25,
            start_param: 0.0,
            end_param: TAU,
        };
        let pieces = clip(&small);
        assert_eq!(pieces.len(), 1);
        assert!(matches!(pieces[0], Curve2::Polyline { closed: true, .. }));
    }

    #[test]
    fn a_point_is_kept_or_dropped_and_a_bad_radius_clips_everything() {
        assert_eq!(
            clip(&Curve2::Point(Point2::new(0.5, 0.0))),
            vec![Curve2::Point(Point2::new(0.5, 0.0))]
        );
        assert!(clip(&Curve2::Point(Point2::new(5.0, 0.0))).is_empty());
        // A radius that is not a disc clips everything away rather than
        // meaning "no crop": the caller that authored it is the one that can
        // say what a bad radius means, and treating it as no crop would put
        // the whole part inside a detail view.
        let l = Curve2::Line {
            start: Point2::new(-3.0, 0.0),
            end: Point2::new(3.0, 0.0),
        };
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(
                l.clipped_to_disc([0.0, 0.0], bad, 1e-4).is_empty(),
                "radius {bad}"
            );
        }
    }

    #[test]
    fn an_arc_that_dips_out_of_the_disc_between_two_inside_ends_is_split_in_two() {
        // Both ENDS inside, the middle outside — the case a clip that only
        // looked at endpoints, or that gave up on a multi-turn parameter
        // range, would answer with one piece spanning the gap.
        //
        // The same circle as above: centred at (1, 0), radius 1, so the part
        // inside the unit disc is `t ∈ (2π/3, 4π/3)` and the part outside is
        // `t ∈ (4π/3, 8π/3)`. An arc from just before `4π/3` to just after
        // `8π/3` therefore starts inside, leaves, and comes back.
        let arc = Curve2::Circle {
            center: Point2::new(1.0, 0.0),
            radius: 1.0,
            start_angle: 4.0 * PI / 3.0 - 0.2,
            end_angle: 8.0 * PI / 3.0 + 0.2,
        };
        for t in [arc.param_range().unwrap().0, arc.param_range().unwrap().1] {
            let p = arc.eval(t).unwrap();
            assert!(
                p.x() * p.x() + p.y() * p.y() < 1.0,
                "the fixture's end at {p:?} must be inside the disc"
            );
        }
        let pieces = clip(&arc);
        assert_eq!(pieces.len(), 2, "{pieces:?}");
        // The two crossings are the cut, and both are a full turn apart from
        // the ones the one-turn solve would have found.
        assert!(close(pieces[0].param_range().unwrap().1, 4.0 * PI / 3.0));
        assert!(close(pieces[1].param_range().unwrap().0, 8.0 * PI / 3.0));
        for piece in &pieces {
            assert!(matches!(piece, Curve2::Circle { .. }));
            for t in [
                piece.param_range().unwrap().0,
                piece.param_range().unwrap().1,
            ] {
                let p = piece.eval(t).unwrap();
                assert!(
                    p.x() * p.x() + p.y() * p.y() <= 1.0 + 1e-9,
                    "a piece ends at {p:?}, outside the crop"
                );
            }
        }
    }

    #[test]
    fn a_curve_that_only_touches_the_disc_is_clipped_away_whatever_its_midpoint() {
        // A tangency is the one place the midpoint rule can be asked a
        // question it cannot answer: the interval touches the boundary at
        // exactly one point and is outside everywhere else, so if that point
        // happens to BE the midpoint, `|p − c| ≤ r` says "inside" and the
        // whole curve is kept. The crop then carries geometry that lies
        // entirely outside it — a detail view's DXF showing a part of the
        // model the detail circle does not enclose. The tangency parameter is
        // therefore a cut like any other, so each piece's midpoint is strictly
        // off the boundary.
        //
        // Every fixture here is built so the touch point IS the parameter
        // midpoint, which is the ordinary shape of a full circle written from
        // its own start angle.

        // A line tangent at (0, 1), whose `t = 0.5` is that very point.
        let tangent = Curve2::Line {
            start: Point2::new(-2.0, 1.0),
            end: Point2::new(2.0, 1.0),
        };
        assert!(close(tangent.eval(0.5).unwrap().y(), 1.0));
        assert!(clip(&tangent).is_empty(), "{:?}", clip(&tangent));

        // A circle EXTERNALLY tangent at (1, 0), written from `t = 0` so its
        // midpoint `t = π` is the touch point.
        let outside = Curve2::Circle {
            center: Point2::new(2.0, 0.0),
            radius: 1.0,
            start_angle: 0.0,
            end_angle: TAU,
        };
        assert!(close(outside.eval(PI).unwrap().x(), 1.0));
        assert!(clip(&outside).is_empty(), "{:?}", clip(&outside));

        // A polyline whose middle chord grazes the boundary: the two outer
        // chords are outside, and the grazing one is kept or dropped on the
        // same terms as the tangent line.
        let graze = Curve2::Polyline {
            points: vec![
                Point2::new(-2.0, 1.0),
                Point2::new(-1.0, 1.0),
                Point2::new(1.0, 1.0),
                Point2::new(2.0, 1.0),
            ],
            closed: false,
        };
        assert!(clip(&graze).is_empty(), "{:?}", clip(&graze));

        // The other side of the same rule: a circle INTERNALLY tangent at
        // (1, 0) is wholly inside the CLOSED disc, so it survives — and as
        // ONE curve, not as two arcs meeting at the point it touches. A clip
        // that cut at the tangency and never rejoined would put a seam in a
        // hole's rim on the sheet.
        let inside = Curve2::Circle {
            center: Point2::new(0.5, 0.0),
            radius: 0.5,
            start_angle: -PI,
            end_angle: PI,
        };
        assert!(close(inside.eval(0.0).unwrap().x(), 1.0));
        assert_eq!(clip(&inside), vec![inside.clone()]);
    }

    #[test]
    fn declines_count_by_kind_and_add_up() {
        let mut a = ProjectionDeclines::default();
        assert_eq!(a.total(), 0);
        assert_eq!(a.counts().len(), 10, "every field must be in `counts`");
        a.split_tangency = 2;
        a.ray_grazes_face = 1;
        a.piece_spans_change = 4;
        let mut b = ProjectionDeclines {
            cross_body: 3,
            split_tangency: 1,
            ..Default::default()
        };
        b.merge(&a);
        assert_eq!(b.split_tangency, 3);
        assert_eq!(b.ray_grazes_face, 1);
        assert_eq!(b.cross_body, 3);
        assert_eq!(b.piece_spans_change, 4);
        assert_eq!(b.total(), 11);
        // Named counters, so a report cannot drift from the struct.
        let named: Vec<&str> = b
            .counts()
            .iter()
            .filter(|(_, n)| *n > 0)
            .map(|(k, _)| *k)
            .collect();
        assert_eq!(
            named,
            vec![
                "split_tangency",
                "ray_grazes_face",
                "piece_spans_change",
                "cross_body"
            ],
            "counts() reports in field order"
        );
    }

    #[test]
    fn a_views_declines_merge_when_bodies_are_appended() {
        let mut one = ViewGeometry::with_declines(
            Vec::new(),
            ProjectionDeclines {
                depth_unliftable: 1,
                ..Default::default()
            },
        );
        one.extend(ViewGeometry::with_declines(
            Vec::new(),
            ProjectionDeclines {
                depth_unliftable: 2,
                cross_body: 1,
                ..Default::default()
            },
        ));
        assert_eq!(one.declines.depth_unliftable, 3);
        assert_eq!(one.declines.cross_body, 1);
        // `new` is the no-decline door, and the default view has none.
        assert_eq!(ViewGeometry::new(Vec::new()).declines.total(), 0);
        assert_eq!(ViewGeometry::default().declines.total(), 0);
    }

    #[test]
    fn an_unclassified_curve_carries_no_depth() {
        let c = ProjectedCurve::visible(
            Curve2::Point(Point2::new(0.0, 0.0)),
            CurveKind::Silhouette,
            None,
        );
        assert_eq!(c.visibility, Visibility::Visible);
        assert!(c.depth.is_none());
    }

    #[test]
    fn projection_defaults_are_loud() {
        struct Nothing;
        impl KernelProjection for Nothing {}
        let mut n = Nothing;
        let h = KernelSolidHandle::from_raw(0);
        let err = n.project(&h, &ViewFrame::TOP, &ProjectOpts::default());
        assert!(matches!(
            err,
            Err(KernelError::NotSupported { ref operation }) if operation == "project"
        ));
        assert!(n
            .project_bodies(&[], &ViewFrame::TOP, &ProjectOpts::default())
            .is_err());
        assert!(n
            .export_dxf(&[], &ViewFrame::TOP, &ProjectOpts::default())
            .is_err());
        assert!(n.section_with_plane(&h, [0.0; 3], [0.0, 0.0, 1.0]).is_err());
    }

    // ---------------------------------------------------------- D4b rotation

    #[test]
    fn a_quarter_turn_rotates_a_curve_without_rescaling_it() {
        // The map a section cap needs: the cut plane's frame into the section
        // view's. A quarter turn is the one case whose answer can be written
        // down exactly.
        let rot = [0.0, 1.0]; // cos 90°, sin 90°
        let line = Curve2::Line {
            start: Point2::new(1.0, 0.0),
            end: Point2::new(3.0, 0.0),
        };
        let Some(Curve2::Line { start, end }) = line.transformed_by(1.0, rot, [0.0, 0.0]) else {
            panic!("a line must stay a line");
        };
        assert!((start.x() - 0.0).abs() < 1e-12 && (start.y() - 1.0).abs() < 1e-12);
        assert!((end.x() - 0.0).abs() < 1e-12 && (end.y() - 3.0).abs() < 1e-12);

        // A circle's angles are measured from +u, so they shift with the
        // frame; its radius does not move.
        let circle = Curve2::Circle {
            center: Point2::new(1.0, 0.0),
            radius: 2.0,
            start_angle: 0.0,
            end_angle: std::f64::consts::FRAC_PI_2,
        };
        let Some(Curve2::Circle {
            center,
            radius,
            start_angle,
            end_angle,
        }) = circle.transformed_by(1.0, rot, [0.0, 0.0])
        else {
            panic!("a circle must stay a circle");
        };
        assert!((center.x()).abs() < 1e-12 && (center.y() - 1.0).abs() < 1e-12);
        assert_eq!(radius, 2.0);
        assert!((start_angle - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!((end_angle - std::f64::consts::PI).abs() < 1e-12);

        // An ellipse's parameters are measured from its OWN major axis, so
        // the axis turns and the parameters do not. Rotating both would turn
        // the ellipse twice.
        let ellipse = Curve2::Ellipse {
            center: Point2::new(0.0, 0.0),
            major_axis: [1.0, 0.0],
            major_radius: 3.0,
            minor_radius: 1.0,
            start_param: 0.25,
            end_param: 1.25,
        };
        let Some(Curve2::Ellipse {
            major_axis,
            major_radius,
            minor_radius,
            start_param,
            end_param,
            ..
        }) = ellipse.transformed_by(1.0, rot, [0.0, 0.0])
        else {
            panic!("an ellipse must stay an ellipse");
        };
        assert!((major_axis[0]).abs() < 1e-12 && (major_axis[1] - 1.0).abs() < 1e-12);
        assert_eq!((major_radius, minor_radius), (3.0, 1.0));
        assert_eq!((start_param, end_param), (0.25, 1.25));

        // Rotation preserves every point's distance from the origin — the
        // property that makes it a similarity of ratio 1.
        let before = ellipse.eval(0.7).unwrap();
        let after = ellipse
            .transformed_by(1.0, rot, [0.0, 0.0])
            .unwrap()
            .eval(0.7)
            .unwrap();
        assert!((before.x().hypot(before.y()) - after.x().hypot(after.y())).abs() < 1e-12);
    }

    #[test]
    fn a_rotation_that_is_not_a_unit_pair_is_refused_rather_than_normalized() {
        // A non-unit pair scales the curve a second time by a factor the
        // caller never named — exactly the silent mis-scaling the positive
        // finite `scale` check exists to refuse.
        let line = Curve2::Line {
            start: Point2::new(0.0, 0.0),
            end: Point2::new(1.0, 0.0),
        };
        assert!(line.transformed_by(1.0, [2.0, 0.0], [0.0, 0.0]).is_none());
        assert!(line
            .transformed_by(1.0, [f64::NAN, 0.0], [0.0, 0.0])
            .is_none());
        // And `transformed` is `transformed_by` at the identity rotation, so
        // D4a's placement is unchanged.
        assert_eq!(
            line.transformed(2.0, [1.0, 1.0]),
            line.transformed_by(2.0, [1.0, 0.0], [1.0, 1.0])
        );
    }

    #[test]
    fn a_rotated_arc_keeps_its_span_and_starts_inside_one_turn() {
        // Shifting by θ alone can leave a start angle outside [0, 2π), which
        // the DXF writer turns into a degree value past 360 — a number a
        // reader is free to reject. The pair slides back by whole turns, and
        // the SPAN is what must survive.
        let arc = Curve2::Circle {
            center: Point2::new(0.0, 0.0),
            radius: 1.0,
            start_angle: 6.0,
            end_angle: 6.5,
        };
        let turn = 2.0_f64; // radians
        let rot = [turn.cos(), turn.sin()];
        let Some(Curve2::Circle {
            start_angle,
            end_angle,
            ..
        }) = arc.transformed_by(1.0, rot, [0.0, 0.0])
        else {
            panic!("a circle must stay a circle");
        };
        assert!(
            (0.0..std::f64::consts::TAU).contains(&start_angle),
            "start {start_angle} outside one turn"
        );
        assert!((end_angle - start_angle - 0.5).abs() < 1e-12);

        // A FULL circle keeps its 2π span, which is what tells it from a
        // zero-length arc.
        let full = Curve2::Circle {
            center: Point2::new(0.0, 0.0),
            radius: 1.0,
            start_angle: 0.0,
            end_angle: std::f64::consts::TAU,
        };
        let Some(Curve2::Circle {
            start_angle,
            end_angle,
            ..
        }) = full.transformed_by(1.0, rot, [0.0, 0.0])
        else {
            panic!("a circle must stay a circle");
        };
        assert!((end_angle - start_angle - std::f64::consts::TAU).abs() < 1e-12);
    }
}
