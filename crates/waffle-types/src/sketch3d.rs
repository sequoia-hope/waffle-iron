//! 3D sketch — spatial reference geometry (`specs/sketch3d.md`).
//!
//! A [`Sketch3d`] is an open or closed chain (or several, or a branching
//! graph) of lines and arcs in space, plus the points that define them. It
//! produces no body and has no region: it is reference geometry, and its
//! consumers read [`Chain3d`]s out of it — a sweep path
//! (`specs/b6_general_sweep.md`), a frame's centre-line graph, something to
//! measure or snap to.
//!
//! **No constraint solver** (spec §4 decision A). A point is either a literal
//! coordinate, an expression the engine evaluated into that coordinate before
//! calling here, or an [`Attachment`] that DERIVES the coordinate in one
//! deterministic pass. The two things a path must get exactly right are
//! coincidence — which is a shared point id, not a constraint — and tangency
//! at a bend, which is the [`Sketch3dEntity::Fillet`] generator, exact by
//! construction rather than converged to a solver tolerance.
//!
//! Evaluation order is fixed and pure: attachments (in dependency order), then
//! fillet expansion, then chain extraction. Nothing here iterates a `HashMap`,
//! so the output is bit-identical across runs (the solver's determinism rule,
//! `sketch-solver/src/solver.rs:14`).
//!
//! Expressions (`xyz_expr`, `radius_expr`) are NOT evaluated here —
//! `feature-engine` evaluates them against the design parameters and writes
//! the results into `xyz` / `radius` before evaluation, exactly as it does for
//! `ExtrudeParams::depth_expr`.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::geom_ref::GeomRef;
use crate::path::PATH_TANGENT_TOLERANCE;
use crate::sketch::generated_entity_id_base;

/// Below this, a length or a cross-product magnitude is treated as zero.
/// One nanometre in a metre-unit model — two orders below `MIN_FEATURE_SIZE`,
/// so a segment this short is degenerate by any measure the kernel applies.
pub const SKETCH3D_LENGTH_EPS: f64 = 1e-9;

/// A world axis, for [`Attachment::AlongAxis`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    fn unit(self) -> [f64; 3] {
        match self {
            Axis::X => [1.0, 0.0, 0.0],
            Axis::Y => [0.0, 1.0, 0.0],
            Axis::Z => [0.0, 0.0, 1.0],
        }
    }
}

/// How a point's coordinates are derived, when they are not literal.
///
/// Every variant resolves by construction in one pass — there is no iteration
/// and no convergence. `Offset` and `AlongAxis` reference another point of the
/// same sketch and so impose a dependency order; the rest resolve against
/// model geometry through [`ExternalAnchors`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Attachment {
    /// Coincident with a model vertex.
    Vertex { reference: GeomRef },
    /// At parameter `t ∈ [0, 1]` along a model edge.
    EdgePoint { reference: GeomRef, t: f64 },
    /// At `uv` on a model face or datum plane.
    OnPlane { reference: GeomRef, uv: [f64; 2] },
    /// A fixed world-space offset from another sketch point.
    Offset { from: u32, delta: [f64; 3] },
    /// A run of `distance` along a world axis from another sketch point —
    /// the axis-locked segment that most frame geometry is made of.
    AlongAxis {
        from: u32,
        axis: Axis,
        distance: f64,
    },
}

/// An entity of a 3D sketch. Ids are unique within the sketch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Sketch3dEntity {
    /// A point. `xyz` always holds the last evaluated coordinates, so a reader
    /// that does not evaluate still sees the sketch's shape; `attach` and
    /// `xyz_expr`, when present, are what *derive* `xyz` at the next rebuild.
    Point {
        id: u32,
        xyz: [f64; 3],
        /// Boxed: an `Attachment` carries a `GeomRef`, which is large, and
        /// there is one entity per POINT here — a frame path is thousands of
        /// them, almost all with no attachment at all. Boxing keeps the
        /// variant small; serde sees through the `Box`, so the wire form is
        /// unchanged.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attach: Option<Box<Attachment>>,
        /// Per-component driving expressions (mm-space, like every other
        /// `*_expr` in the tree). Evaluated by `feature-engine`, not here.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        xyz_expr: Option<[Option<String>; 3]>,
        #[serde(default)]
        construction: bool,
    },
    /// Straight segment between two points.
    Line {
        id: u32,
        start_id: u32,
        end_id: u32,
        #[serde(default)]
        construction: bool,
    },
    /// Arc through three distinct, non-collinear points: from `start_id` to
    /// `end_id`, passing through `via_id`. Three points determine centre,
    /// radius and plane, so the arc carries no stored frame that could
    /// disagree with its own endpoints.
    Arc {
        id: u32,
        start_id: u32,
        end_id: u32,
        via_id: u32,
        #[serde(default)]
        construction: bool,
    },
    /// A tangent fillet of `radius` at the joint between the two straight
    /// segments meeting at `at_point_id`. A GENERATOR: expanded at evaluation
    /// into an arc plus the two trimmed segment ends, the way
    /// [`crate::sketch::SketchEntity::Gear`] and `Sprocket` are expanded.
    /// Tangency is exact by construction and survives moving the segments.
    Fillet {
        id: u32,
        at_point_id: u32,
        radius: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        radius_expr: Option<String>,
    },
}

impl Sketch3dEntity {
    pub fn id(&self) -> u32 {
        match self {
            Sketch3dEntity::Point { id, .. }
            | Sketch3dEntity::Line { id, .. }
            | Sketch3dEntity::Arc { id, .. }
            | Sketch3dEntity::Fillet { id, .. } => *id,
        }
    }

    /// Construction entities are excluded from nothing here — a 3D sketch has
    /// no region to exclude them from — but consumers and the renderer style
    /// them differently, and a path is often drawn as construction geometry.
    pub fn is_construction(&self) -> bool {
        match self {
            Sketch3dEntity::Point { construction, .. }
            | Sketch3dEntity::Line { construction, .. }
            | Sketch3dEntity::Arc { construction, .. } => *construction,
            Sketch3dEntity::Fillet { .. } => false,
        }
    }
}

/// Evaluation state, mirroring [`crate::sketch::SolveStatus`]'s role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Sketch3dStatus {
    /// Never evaluated (the state a writer that has not run the engine
    /// leaves behind; the next rebuild replaces it).
    #[default]
    Unevaluated,
    /// Every point resolved and every generator expanded.
    Ok,
    /// Evaluation failed; `reason` is the `Display` of the [`Sketch3dError`].
    Failed { reason: String },
}

/// A 3D sketch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Sketch3d {
    pub id: Uuid,
    pub entities: Vec<Sketch3dEntity>,
    /// Resolved world coordinates by point id, written by [`Self::evaluate`].
    /// `BTreeMap` (not `HashMap`) so the serialized bytes are stable — the
    /// trap that made `SaveVerifier` hash `to_value` rather than `to_string`.
    #[serde(default)]
    pub resolved: BTreeMap<u32, [f64; 3]>,
    #[serde(default)]
    pub status: Sketch3dStatus,
}

/// Model geometry a point can attach to. Implemented by `feature-engine`
/// over `KernelIntrospect`; `waffle-types` stays free of kernel access.
pub trait ExternalAnchors {
    fn vertex(&self, reference: &GeomRef) -> Option<[f64; 3]>;
    fn edge_point(&self, reference: &GeomRef, t: f64) -> Option<[f64; 3]>;
    fn plane_point(&self, reference: &GeomRef, uv: [f64; 2]) -> Option<[f64; 3]>;
}

/// An [`ExternalAnchors`] that resolves nothing — for a sketch whose points
/// are all literal or sketch-relative, and for unit tests.
pub struct NoAnchors;

impl ExternalAnchors for NoAnchors {
    fn vertex(&self, _: &GeomRef) -> Option<[f64; 3]> {
        None
    }
    fn edge_point(&self, _: &GeomRef, _: f64) -> Option<[f64; 3]> {
        None
    }
    fn plane_point(&self, _: &GeomRef, _: [f64; 2]) -> Option<[f64; 3]> {
        None
    }
}

/// One edge of an extracted chain, in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge3d {
    /// The entity this edge came from — a `Line`/`Arc` id, or the `Fillet`
    /// id for an arc a fillet minted.
    pub entity_id: u32,
    pub kind: Edge3dKind,
    pub a: [f64; 3],
    pub b: [f64; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Edge3dKind {
    Line,
    /// Traversal `a → b` is counterclockwise about `normal`.
    Arc {
        center: [f64; 3],
        normal: [f64; 3],
        radius: f64,
    },
}

impl Edge3d {
    /// Unit tangent at the start, pointing along the traversal.
    pub fn start_tangent(&self) -> [f64; 3] {
        self.tangent_at(self.a)
    }

    /// Unit tangent at the end, pointing along the traversal.
    pub fn end_tangent(&self) -> [f64; 3] {
        self.tangent_at(self.b)
    }

    fn tangent_at(&self, p: [f64; 3]) -> [f64; 3] {
        match self.kind {
            Edge3dKind::Line => norm(sub(self.b, self.a)),
            Edge3dKind::Arc { center, normal, .. } => norm(cross(normal, sub(p, center))),
        }
    }

    /// Exact arc length.
    pub fn length(&self) -> f64 {
        match self.kind {
            Edge3dKind::Line => len(sub(self.b, self.a)),
            Edge3dKind::Arc {
                center,
                normal,
                radius,
            } => {
                let u = sub(self.a, center);
                let v = sub(self.b, center);
                let sweep = dot(cross(u, v), normal).atan2(dot(u, v));
                let sweep = if sweep < 0.0 {
                    sweep + std::f64::consts::TAU
                } else {
                    sweep
                };
                sweep * radius
            }
        }
    }
}

/// A maximal run of edges joined end to end.
///
/// Chains are split at every point where the degree is not 2, so a branching
/// centre-line graph (a truss) yields one chain per member run and a consumer
/// that wants a single path complains for itself. A component with no such
/// point is a cycle and comes back `closed`.
#[derive(Debug, Clone, PartialEq)]
pub struct Chain3d {
    pub edges: Vec<Edge3d>,
    pub closed: bool,
    /// Tangent continuity at each interior joint: `g1[i]` is the joint between
    /// `edges[i]` and `edges[i + 1]`. Length `edges.len() - 1` when open, and
    /// `edges.len()` when closed (the last entry is the wrap-around joint).
    /// A sweep reads this to choose a smooth join over a mitre
    /// (`specs/b6_general_sweep.md` §4) without reclassifying the geometry.
    pub g1: Vec<bool>,
}

impl Chain3d {
    pub fn length(&self) -> f64 {
        self.edges.iter().map(Edge3d::length).sum()
    }
}

/// Why a 3D sketch does not evaluate, or does not yield chains.
#[derive(Debug, Clone, PartialEq)]
pub enum Sketch3dError {
    DuplicateEntityId {
        id: u32,
    },
    /// A segment names an id that is not a point of this sketch.
    PointNotFound {
        entity_id: u32,
        point_id: u32,
    },
    /// An attachment references a point that is not in this sketch.
    AttachmentPointNotFound {
        point_id: u32,
        from: u32,
    },
    /// `Offset`/`AlongAxis` attachments form a cycle.
    CyclicAttachment {
        point_id: u32,
    },
    /// A `Vertex`/`EdgePoint`/`OnPlane` attachment did not resolve — the
    /// referenced geometry is gone, or the reference does not name that kind
    /// of entity.
    UnresolvedAttachment {
        point_id: u32,
    },
    NonFiniteCoordinate {
        point_id: u32,
    },
    /// A line whose endpoints coincide, or an arc of zero radius.
    DegenerateSegment {
        entity_id: u32,
    },
    /// An arc's three points are collinear (no circle through them) or two
    /// of them coincide.
    ArcPointsDegenerate {
        entity_id: u32,
    },
    /// A fillet's joint does not have exactly two incident segments.
    FilletNotTwoSegments {
        fillet_id: u32,
        at_point_id: u32,
        found: usize,
    },
    /// A fillet's joint has an arc on one side; only line–line is supported.
    FilletNeedsStraightSegments {
        fillet_id: u32,
        at_point_id: u32,
    },
    /// The two segments are collinear and continue straight on — there is no
    /// corner to round.
    FilletCollinear {
        fillet_id: u32,
        at_point_id: u32,
    },
    /// The two segments double back on each other; the bisector is undefined.
    FilletReversal {
        fillet_id: u32,
        at_point_id: u32,
    },
    /// The radius does not fit: the tangent point would fall beyond the far
    /// end of one of the two segments (`max_radius` is the largest that fits,
    /// accounting for any fillet at the segments' other ends).
    FilletTooLarge {
        fillet_id: u32,
        at_point_id: u32,
        radius: f64,
        max_radius: f64,
    },
    /// Two fillets name the same joint.
    DuplicateFillet {
        at_point_id: u32,
    },
    /// A fillet's radius is not finite and positive.
    FilletBadRadius {
        fillet_id: u32,
        radius: f64,
    },
}

impl std::fmt::Display for Sketch3dError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateEntityId { id } => {
                write!(f, "3D sketch: entity id {id} is used more than once")
            }
            Self::PointNotFound {
                entity_id,
                point_id,
            } => write!(
                f,
                "3D sketch: entity {entity_id} references point {point_id}, which is not a \
                 point of this sketch"
            ),
            Self::AttachmentPointNotFound { point_id, from } => write!(
                f,
                "3D sketch: point {point_id} is attached to point {from}, which is not a \
                 point of this sketch"
            ),
            Self::CyclicAttachment { point_id } => write!(
                f,
                "3D sketch: point {point_id} is part of a cycle of attachments"
            ),
            Self::UnresolvedAttachment { point_id } => write!(
                f,
                "3D sketch: point {point_id}'s attachment does not resolve — the geometry it \
                 names is missing or is not of that kind"
            ),
            Self::NonFiniteCoordinate { point_id } => {
                write!(f, "3D sketch: point {point_id} has a non-finite coordinate")
            }
            Self::DegenerateSegment { entity_id } => {
                write!(f, "3D sketch: segment {entity_id} has zero length")
            }
            Self::ArcPointsDegenerate { entity_id } => write!(
                f,
                "3D sketch: arc {entity_id}'s three points are collinear or coincident, so no \
                 arc passes through them"
            ),
            Self::FilletNotTwoSegments {
                fillet_id,
                at_point_id,
                found,
            } => write!(
                f,
                "3D sketch: fillet {fillet_id} is at point {at_point_id}, where {found} \
                 segments meet (a fillet needs exactly 2)"
            ),
            Self::FilletNeedsStraightSegments {
                fillet_id,
                at_point_id,
            } => write!(
                f,
                "3D sketch: fillet {fillet_id} at point {at_point_id} meets an arc; only a \
                 line–line corner can be filleted"
            ),
            Self::FilletCollinear {
                fillet_id,
                at_point_id,
            } => write!(
                f,
                "3D sketch: fillet {fillet_id} at point {at_point_id} has no corner to round — \
                 the two segments are collinear"
            ),
            Self::FilletReversal {
                fillet_id,
                at_point_id,
            } => write!(
                f,
                "3D sketch: fillet {fillet_id} at point {at_point_id} joins two segments that \
                 double back on each other"
            ),
            Self::FilletTooLarge {
                fillet_id,
                at_point_id,
                radius,
                max_radius,
            } => write!(
                f,
                "3D sketch: fillet {fillet_id} at point {at_point_id} has radius {radius}, \
                 which does not fit between its neighbours (largest that fits: {max_radius})"
            ),
            Self::DuplicateFillet { at_point_id } => write!(
                f,
                "3D sketch: more than one fillet is placed at point {at_point_id}"
            ),
            Self::FilletBadRadius { fillet_id, radius } => write!(
                f,
                "3D sketch: fillet {fillet_id} has radius {radius}, which is not finite and \
                 positive"
            ),
        }
    }
}

impl std::error::Error for Sketch3dError {}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

impl Sketch3d {
    pub fn new(id: Uuid, entities: Vec<Sketch3dEntity>) -> Self {
        Self {
            id,
            entities,
            resolved: BTreeMap::new(),
            status: Sketch3dStatus::Unevaluated,
        }
    }

    /// Resolve every point into [`Self::resolved`] and set [`Self::status`].
    ///
    /// Literal points take their `xyz`; attached points derive theirs, in
    /// dependency order for the sketch-relative kinds. Idempotent: evaluating
    /// an already-evaluated sketch gives bit-identical results.
    pub fn evaluate(&mut self, anchors: &dyn ExternalAnchors) -> Result<(), Sketch3dError> {
        match self.resolve_points(anchors) {
            Ok(resolved) => {
                self.resolved = resolved;
                self.status = Sketch3dStatus::Ok;
                Ok(())
            }
            Err(e) => {
                self.resolved = BTreeMap::new();
                self.status = Sketch3dStatus::Failed {
                    reason: e.to_string(),
                };
                Err(e)
            }
        }
    }

    fn check_unique_ids(&self) -> Result<(), Sketch3dError> {
        let mut seen = BTreeSet::new();
        for e in &self.entities {
            if !seen.insert(e.id()) {
                return Err(Sketch3dError::DuplicateEntityId { id: e.id() });
            }
        }
        Ok(())
    }

    fn resolve_points(
        &self,
        anchors: &dyn ExternalAnchors,
    ) -> Result<BTreeMap<u32, [f64; 3]>, Sketch3dError> {
        self.check_unique_ids()?;

        // Point id -> (literal, attachment). BTreeMap so the walk order below
        // is declaration-independent and deterministic.
        let mut points: BTreeMap<u32, (&[f64; 3], Option<&Attachment>)> = BTreeMap::new();
        for e in &self.entities {
            if let Sketch3dEntity::Point {
                id, xyz, attach, ..
            } = e
            {
                points.insert(*id, (xyz, attach.as_deref()));
            }
        }

        let mut out: BTreeMap<u32, [f64; 3]> = BTreeMap::new();
        // Iterative DFS with a three-state mark, so a cycle is reported
        // against the point that closes it rather than blowing the stack.
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            Open,
            Done,
        }
        let mut mark: BTreeMap<u32, Mark> = BTreeMap::new();

        for &pid in points.keys() {
            if mark.get(&pid) == Some(&Mark::Done) {
                continue;
            }
            // Walk the dependency spine from `pid` down to something resolvable,
            // then unwind.
            let mut stack: Vec<u32> = vec![pid];
            while let Some(&cur) = stack.last() {
                if mark.get(&cur) == Some(&Mark::Done) {
                    stack.pop();
                    continue;
                }
                let (literal, attach) =
                    *points
                        .get(&cur)
                        .ok_or(Sketch3dError::AttachmentPointNotFound {
                            point_id: cur,
                            from: cur,
                        })?;

                let dependency = match attach {
                    Some(Attachment::Offset { from, .. })
                    | Some(Attachment::AlongAxis { from, .. }) => Some(*from),
                    _ => None,
                };

                if let Some(dep) = dependency {
                    if !points.contains_key(&dep) {
                        return Err(Sketch3dError::AttachmentPointNotFound {
                            point_id: cur,
                            from: dep,
                        });
                    }
                    if mark.get(&dep) != Some(&Mark::Done) {
                        if mark.get(&dep) == Some(&Mark::Open) {
                            return Err(Sketch3dError::CyclicAttachment { point_id: dep });
                        }
                        mark.insert(cur, Mark::Open);
                        stack.push(dep);
                        continue;
                    }
                }

                let value = match attach {
                    None => *literal,
                    Some(Attachment::Vertex { reference }) => anchors
                        .vertex(reference)
                        .ok_or(Sketch3dError::UnresolvedAttachment { point_id: cur })?,
                    Some(Attachment::EdgePoint { reference, t }) => anchors
                        .edge_point(reference, *t)
                        .ok_or(Sketch3dError::UnresolvedAttachment { point_id: cur })?,
                    Some(Attachment::OnPlane { reference, uv }) => anchors
                        .plane_point(reference, *uv)
                        .ok_or(Sketch3dError::UnresolvedAttachment { point_id: cur })?,
                    Some(Attachment::Offset { from, delta }) => add(out[from], *delta),
                    Some(Attachment::AlongAxis {
                        from,
                        axis,
                        distance,
                    }) => add(out[from], scale(axis.unit(), *distance)),
                };
                if !value.iter().all(|c| c.is_finite()) {
                    return Err(Sketch3dError::NonFiniteCoordinate { point_id: cur });
                }
                out.insert(cur, value);
                mark.insert(cur, Mark::Done);
                stack.pop();
            }
        }
        Ok(out)
    }

    /// Extract every maximal chain, with fillets expanded.
    ///
    /// Requires [`Self::evaluate`] to have run (it reads [`Self::resolved`]).
    /// Chains come back ordered by their lowest entity id, and each chain's
    /// direction is the one that starts at its lowest-id terminal edge, so
    /// the output is a deterministic function of the sketch.
    pub fn chains(&self) -> Result<Vec<Chain3d>, Sketch3dError> {
        self.check_unique_ids()?;
        let mut pieces = self.build_pieces()?;
        self.apply_fillets(&mut pieces)?;
        Ok(walk_chains(pieces))
    }

    /// Segments in entity-id order, endpoints resolved.
    fn build_pieces(&self) -> Result<Vec<Piece>, Sketch3dError> {
        let point_ids: BTreeSet<u32> = self
            .entities
            .iter()
            .filter_map(|e| match e {
                Sketch3dEntity::Point { id, .. } => Some(*id),
                _ => None,
            })
            .collect();

        let pos = |entity_id: u32, pid: u32| -> Result<[f64; 3], Sketch3dError> {
            if !point_ids.contains(&pid) {
                return Err(Sketch3dError::PointNotFound {
                    entity_id,
                    point_id: pid,
                });
            }
            self.resolved
                .get(&pid)
                .copied()
                .ok_or(Sketch3dError::PointNotFound {
                    entity_id,
                    point_id: pid,
                })
        };

        let mut pieces = Vec::new();
        for e in &self.entities {
            match e {
                Sketch3dEntity::Line {
                    id,
                    start_id,
                    end_id,
                    ..
                } => {
                    let a = pos(*id, *start_id)?;
                    let b = pos(*id, *end_id)?;
                    if len(sub(b, a)) <= SKETCH3D_LENGTH_EPS {
                        return Err(Sketch3dError::DegenerateSegment { entity_id: *id });
                    }
                    pieces.push(Piece {
                        edge: Edge3d {
                            entity_id: *id,
                            kind: Edge3dKind::Line,
                            a,
                            b,
                        },
                        start_pid: *start_id,
                        end_pid: *end_id,
                    });
                }
                Sketch3dEntity::Arc {
                    id,
                    start_id,
                    end_id,
                    via_id,
                    ..
                } => {
                    let a = pos(*id, *start_id)?;
                    let b = pos(*id, *end_id)?;
                    let via = pos(*id, *via_id)?;
                    let (center, normal, radius) = circle_through(a, via, b)
                        .ok_or(Sketch3dError::ArcPointsDegenerate { entity_id: *id })?;
                    pieces.push(Piece {
                        edge: Edge3d {
                            entity_id: *id,
                            kind: Edge3dKind::Arc {
                                center,
                                normal,
                                radius,
                            },
                            a,
                            b,
                        },
                        start_pid: *start_id,
                        end_pid: *end_id,
                    });
                }
                Sketch3dEntity::Point { .. } | Sketch3dEntity::Fillet { .. } => {}
            }
        }
        Ok(pieces)
    }

    /// Replace each filleted corner with a tangent arc, shortening the two
    /// segments that meet there.
    ///
    /// Every fillet's geometry is computed from the UNTRIMMED segments first,
    /// then all of them are checked for fit together, then all are applied —
    /// so two fillets at the two ends of one segment see each other's setback
    /// and neither is silently shrunk by the other.
    fn apply_fillets(&self, pieces: &mut Vec<Piece>) -> Result<(), Sketch3dError> {
        let fillets: Vec<(u32, u32, f64)> = self
            .entities
            .iter()
            .filter_map(|e| match e {
                Sketch3dEntity::Fillet {
                    id,
                    at_point_id,
                    radius,
                    ..
                } => Some((*id, *at_point_id, *radius)),
                _ => None,
            })
            .collect();
        if fillets.is_empty() {
            return Ok(());
        }
        let mut at_seen = BTreeSet::new();
        for (_, at, _) in &fillets {
            if !at_seen.insert(*at) {
                return Err(Sketch3dError::DuplicateFillet { at_point_id: *at });
            }
        }

        // Point -> the pieces touching it, in entity-id order.
        let mut incident: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for (i, p) in pieces.iter().enumerate() {
            incident.entry(p.start_pid).or_default().push(i);
            incident.entry(p.end_pid).or_default().push(i);
        }

        struct Planned {
            fillet_id: u32,
            /// (piece index, which end, tangent point, setback along that side)
            sides: [(usize, Tip, [f64; 3], f64); 2],
            arc: Edge3d,
        }
        let mut planned: Vec<Planned> = Vec::new();

        for (fid, at, radius) in &fillets {
            if !(radius.is_finite() && *radius > 0.0) {
                return Err(Sketch3dError::FilletBadRadius {
                    fillet_id: *fid,
                    radius: *radius,
                });
            }
            let inc = incident.get(at).map(Vec::as_slice).unwrap_or(&[]);
            if inc.len() != 2 {
                return Err(Sketch3dError::FilletNotTwoSegments {
                    fillet_id: *fid,
                    at_point_id: *at,
                    found: inc.len(),
                });
            }
            let (i0, i1) = (inc[0], inc[1]);
            for &i in &[i0, i1] {
                if !matches!(pieces[i].edge.kind, Edge3dKind::Line) {
                    return Err(Sketch3dError::FilletNeedsStraightSegments {
                        fillet_id: *fid,
                        at_point_id: *at,
                    });
                }
            }
            let e0 = pieces[i0].end_at(*at);
            let e1 = pieces[i1].end_at(*at);
            let p = pieces[i0].point_at(e0);
            // Unit directions from the corner out along each segment.
            let u0 = norm(sub(pieces[i0].point_at(e0.other()), p));
            let u1 = norm(sub(pieces[i1].point_at(e1.other()), p));

            let cos_phi = dot(u0, u1).clamp(-1.0, 1.0);
            // φ = π: the segments continue straight; φ = 0: they double back.
            if 1.0 + cos_phi <= PATH_TANGENT_TOLERANCE {
                return Err(Sketch3dError::FilletCollinear {
                    fillet_id: *fid,
                    at_point_id: *at,
                });
            }
            if 1.0 - cos_phi <= PATH_TANGENT_TOLERANCE {
                return Err(Sketch3dError::FilletReversal {
                    fillet_id: *fid,
                    at_point_id: *at,
                });
            }
            let phi = cos_phi.acos();
            let half = phi / 2.0;
            let setback = radius / half.tan();
            let t0 = add(p, scale(u0, setback));
            let t1 = add(p, scale(u1, setback));
            let center = add(p, scale(norm(add(u0, u1)), radius / half.sin()));
            let normal = norm(cross(sub(t0, center), sub(t1, center)));
            let arc = Edge3d {
                entity_id: *fid,
                kind: Edge3dKind::Arc {
                    center,
                    normal,
                    radius: *radius,
                },
                a: t0,
                b: t1,
            };
            planned.push(Planned {
                fillet_id: *fid,
                sides: [(i0, e0, t0, setback), (i1, e1, t1, setback)],
                arc,
            });
        }

        // Fit check: the two setbacks a segment carries must leave it positive.
        let mut consumed: BTreeMap<(usize, Tip), f64> = BTreeMap::new();
        for pl in &planned {
            for (i, end, _, setback) in pl.sides {
                consumed.insert((i, end), setback);
            }
        }
        for pl in &planned {
            for (i, end, _, setback) in pl.sides {
                let total_len = len(sub(pieces[i].edge.b, pieces[i].edge.a));
                let other = consumed.get(&(i, end.other())).copied().unwrap_or(0.0);
                if setback + other >= total_len - SKETCH3D_LENGTH_EPS {
                    // Largest radius that fits on this side, holding the other
                    // end's setback fixed: r = available · tan(φ/2), and
                    // tan(φ/2) = radius / setback for the planned pair.
                    let available = (total_len - other).max(0.0);
                    let tan_half = match pl.arc.kind {
                        Edge3dKind::Arc { radius, .. } => radius / setback,
                        Edge3dKind::Line => 0.0,
                    };
                    let at_point_id = pieces[i].pid_at(end);
                    return Err(Sketch3dError::FilletTooLarge {
                        fillet_id: pl.fillet_id,
                        at_point_id,
                        radius: match pl.arc.kind {
                            Edge3dKind::Arc { radius, .. } => radius,
                            Edge3dKind::Line => 0.0,
                        },
                        max_radius: available * tan_half,
                    });
                }
            }
        }

        // Apply: trim the segments back to their tangent points, re-point them
        // at synthetic joint ids, and append the arc between those ids. The
        // synthetic ids live in the fillet's own generated range, so they
        // collide neither with the sketch's points nor with another fillet's.
        for pl in &planned {
            for (i, end, tangent, _) in pl.sides {
                pieces[i].trim_to(end, tangent);
            }
            let base = generated_entity_id_base(pl.fillet_id);
            let (s0, s1) = (base, base + 1);
            pieces[pl.sides[0].0].set_pid(pl.sides[0].1, s0);
            pieces[pl.sides[1].0].set_pid(pl.sides[1].1, s1);
            pieces.push(Piece {
                edge: pl.arc,
                start_pid: s0,
                end_pid: s1,
            });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Chain walking
// ---------------------------------------------------------------------------

/// Which end of a segment a joint sits at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tip {
    Start,
    End,
}

impl Tip {
    fn other(self) -> Tip {
        match self {
            Tip::Start => Tip::End,
            Tip::End => Tip::Start,
        }
    }
}

#[derive(Debug, Clone)]
struct Piece {
    edge: Edge3d,
    start_pid: u32,
    end_pid: u32,
}

impl Piece {
    fn end_at(&self, pid: u32) -> Tip {
        if self.start_pid == pid {
            Tip::Start
        } else {
            Tip::End
        }
    }
    fn pid_at(&self, end: Tip) -> u32 {
        match end {
            Tip::Start => self.start_pid,
            Tip::End => self.end_pid,
        }
    }
    fn point_at(&self, end: Tip) -> [f64; 3] {
        match end {
            Tip::Start => self.edge.a,
            Tip::End => self.edge.b,
        }
    }
    fn set_pid(&mut self, end: Tip, pid: u32) {
        match end {
            Tip::Start => self.start_pid = pid,
            Tip::End => self.end_pid = pid,
        }
    }
    fn trim_to(&mut self, end: Tip, p: [f64; 3]) {
        match end {
            Tip::Start => self.edge.a = p,
            Tip::End => self.edge.b = p,
        }
    }
    /// The edge oriented so it leaves `pid`.
    fn oriented_from(&self, pid: u32) -> Edge3d {
        if self.start_pid == pid {
            self.edge
        } else {
            let mut e = self.edge;
            e.a = self.edge.b;
            e.b = self.edge.a;
            if let Edge3dKind::Arc {
                center,
                normal,
                radius,
            } = self.edge.kind
            {
                // Reversing the traversal reverses the sense about the normal.
                e.kind = Edge3dKind::Arc {
                    center,
                    normal: scale(normal, -1.0),
                    radius,
                };
            }
            e
        }
    }
}

fn walk_chains(pieces: Vec<Piece>) -> Vec<Chain3d> {
    let mut incident: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, p) in pieces.iter().enumerate() {
        incident.entry(p.start_pid).or_default().push(i);
        incident.entry(p.end_pid).or_default().push(i);
    }
    let degree = |pid: u32| incident.get(&pid).map(Vec::len).unwrap_or(0);

    let mut used = vec![false; pieces.len()];
    let mut chains: Vec<Chain3d> = Vec::new();

    // Open runs first: start at every terminal / branch point, in point order.
    let terminals: Vec<u32> = incident
        .iter()
        .filter(|(_, inc)| inc.len() != 2)
        .map(|(&pid, _)| pid)
        .collect();
    for pid in terminals {
        let starts = incident.get(&pid).cloned().unwrap_or_default();
        for start_piece in starts {
            if used[start_piece] {
                continue;
            }
            let mut edges = Vec::new();
            let mut at = pid;
            let mut cur = start_piece;
            loop {
                used[cur] = true;
                let e = pieces[cur].oriented_from(at);
                edges.push(e);
                let next_pid = if pieces[cur].start_pid == at {
                    pieces[cur].end_pid
                } else {
                    pieces[cur].start_pid
                };
                if degree(next_pid) != 2 {
                    break;
                }
                let inc = &incident[&next_pid];
                let Some(&nxt) = inc.iter().find(|&&i| i != cur) else {
                    break;
                };
                if used[nxt] {
                    break;
                }
                at = next_pid;
                cur = nxt;
            }
            chains.push(finish_chain(edges, false));
        }
    }

    // Whatever is left is a cycle: every point on it has degree 2.
    for i in 0..pieces.len() {
        if used[i] {
            continue;
        }
        let mut edges = Vec::new();
        let start_pid = pieces[i].start_pid;
        let mut at = start_pid;
        let mut cur = i;
        loop {
            used[cur] = true;
            edges.push(pieces[cur].oriented_from(at));
            let next_pid = if pieces[cur].start_pid == at {
                pieces[cur].end_pid
            } else {
                pieces[cur].start_pid
            };
            if next_pid == start_pid {
                break;
            }
            let inc = &incident[&next_pid];
            let Some(&nxt) = inc.iter().find(|&&j| j != cur) else {
                break;
            };
            if used[nxt] {
                break;
            }
            at = next_pid;
            cur = nxt;
        }
        chains.push(finish_chain(edges, true));
    }

    chains.sort_by_key(|c| c.edges.iter().map(|e| e.entity_id).min().unwrap_or(0));
    chains
}

fn finish_chain(edges: Vec<Edge3d>, closed: bool) -> Chain3d {
    let n = edges.len();
    let joints = if closed { n } else { n.saturating_sub(1) };
    let mut g1 = Vec::with_capacity(joints);
    for i in 0..joints {
        let out = edges[i].end_tangent();
        let inn = edges[(i + 1) % n].start_tangent();
        g1.push(1.0 - dot(out, inn) <= PATH_TANGENT_TOLERANCE);
    }
    Chain3d { edges, closed, g1 }
}

// ---------------------------------------------------------------------------
// Small vector helpers (local: `cad-primitives` types are the kernel's, and a
// 3D sketch is a document type that must not depend on kernel geometry)
// ---------------------------------------------------------------------------

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
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
fn len(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn norm(a: [f64; 3]) -> [f64; 3] {
    let l = len(a);
    if l <= 0.0 {
        [0.0, 0.0, 0.0]
    } else {
        scale(a, 1.0 / l)
    }
}

/// Circle through three points: `(centre, unit normal, radius)`, with the
/// normal oriented so `a → via → b` runs counterclockwise about it. `None`
/// when the points are collinear or two of them coincide.
///
/// Circumcentre with the origin at `b`: `((|u|²v − |v|²u) × (u × v)) /
/// (2|u × v|²)` for `u = a − b`, `v = via − b`.
fn circle_through(a: [f64; 3], via: [f64; 3], b: [f64; 3]) -> Option<([f64; 3], [f64; 3], f64)> {
    let u = sub(a, b);
    let v = sub(via, b);
    let uxv = cross(u, v);
    let denom = 2.0 * dot(uxv, uxv);
    if !denom.is_finite() || denom <= SKETCH3D_LENGTH_EPS * SKETCH3D_LENGTH_EPS {
        return None;
    }
    let num = cross(sub(scale(v, dot(u, u)), scale(u, dot(v, v))), uxv);
    let center = add(b, scale(num, 1.0 / denom));
    let radius = len(sub(a, center));
    if !(radius.is_finite() && radius > SKETCH3D_LENGTH_EPS) {
        return None;
    }
    // CCW about (a − via) × (b − via) takes a → via → b the short way round.
    let normal = norm(cross(sub(via, a), sub(b, via)));
    if len(normal) <= 0.5 {
        return None;
    }
    Some((center, normal, radius))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(id: u32, xyz: [f64; 3]) -> Sketch3dEntity {
        Sketch3dEntity::Point {
            id,
            xyz,
            attach: None,
            xyz_expr: None,
            construction: false,
        }
    }
    fn line(id: u32, start_id: u32, end_id: u32) -> Sketch3dEntity {
        Sketch3dEntity::Line {
            id,
            start_id,
            end_id,
            construction: false,
        }
    }
    fn evaluated(entities: Vec<Sketch3dEntity>) -> Sketch3d {
        let mut s = Sketch3d::new(Uuid::nil(), entities);
        s.evaluate(&NoAnchors).expect("evaluates");
        s
    }
    /// A reference to a vertex of a feature that does not exist — `NoAnchors`
    /// resolves nothing, so any well-formed reference exercises the refusal.
    fn missing_vertex_ref() -> GeomRef {
        GeomRef {
            kind: crate::topo::TopoKind::Vertex,
            anchor: crate::geom_ref::Anchor::Datum {
                datum_id: Uuid::nil(),
            },
            selector: crate::geom_ref::Selector::Position {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            policy: Default::default(),
            scope: None,
        }
    }
    fn close(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() <= tol)
    }

    #[test]
    fn literal_points_resolve_to_themselves() {
        let s = evaluated(vec![pt(1, [1.0, 2.0, 3.0]), pt(2, [4.0, 5.0, 6.0])]);
        assert_eq!(s.resolved[&1], [1.0, 2.0, 3.0]);
        assert_eq!(s.resolved[&2], [4.0, 5.0, 6.0]);
        assert_eq!(s.status, Sketch3dStatus::Ok);
    }

    #[test]
    fn along_axis_chains_in_dependency_order_regardless_of_declaration_order() {
        // 3 depends on 2 depends on 1, declared backwards.
        let entities = vec![
            Sketch3dEntity::Point {
                id: 3,
                xyz: [0.0; 3],
                attach: Some(Box::new(Attachment::AlongAxis {
                    from: 2,
                    axis: Axis::Z,
                    distance: 5.0,
                })),
                xyz_expr: None,
                construction: false,
            },
            Sketch3dEntity::Point {
                id: 2,
                xyz: [0.0; 3],
                attach: Some(Box::new(Attachment::Offset {
                    from: 1,
                    delta: [0.0, 2.0, 0.0],
                })),
                xyz_expr: None,
                construction: false,
            },
            pt(1, [1.0, 0.0, 0.0]),
        ];
        let s = evaluated(entities);
        assert_eq!(s.resolved[&2], [1.0, 2.0, 0.0]);
        assert_eq!(s.resolved[&3], [1.0, 2.0, 5.0]);
    }

    #[test]
    fn attachment_cycle_is_loud() {
        let entities = vec![
            Sketch3dEntity::Point {
                id: 1,
                xyz: [0.0; 3],
                attach: Some(Box::new(Attachment::Offset {
                    from: 2,
                    delta: [1.0, 0.0, 0.0],
                })),
                xyz_expr: None,
                construction: false,
            },
            Sketch3dEntity::Point {
                id: 2,
                xyz: [0.0; 3],
                attach: Some(Box::new(Attachment::Offset {
                    from: 1,
                    delta: [1.0, 0.0, 0.0],
                })),
                xyz_expr: None,
                construction: false,
            },
        ];
        let mut s = Sketch3d::new(Uuid::nil(), entities);
        let err = s.evaluate(&NoAnchors).unwrap_err();
        assert!(matches!(err, Sketch3dError::CyclicAttachment { .. }));
        assert!(matches!(s.status, Sketch3dStatus::Failed { .. }));
    }

    #[test]
    fn unresolved_external_attachment_is_loud() {
        let entities = vec![Sketch3dEntity::Point {
            id: 1,
            xyz: [0.0; 3],
            attach: Some(Box::new(Attachment::Vertex {
                reference: missing_vertex_ref(),
            })),
            xyz_expr: None,
            construction: false,
        }];
        let mut s = Sketch3d::new(Uuid::nil(), entities);
        assert!(matches!(
            s.evaluate(&NoAnchors).unwrap_err(),
            Sketch3dError::UnresolvedAttachment { point_id: 1 }
        ));
    }

    #[test]
    fn evaluation_is_idempotent() {
        let mut s = evaluated(vec![pt(1, [1.0, 2.0, 3.0])]);
        let first = s.resolved.clone();
        s.evaluate(&NoAnchors).unwrap();
        assert_eq!(first, s.resolved);
    }

    #[test]
    fn open_chain_of_two_lines_has_one_joint() {
        let s = evaluated(vec![
            pt(1, [0.0, 0.0, 0.0]),
            pt(2, [1.0, 0.0, 0.0]),
            pt(3, [1.0, 1.0, 0.0]),
            line(4, 1, 2),
            line(5, 2, 3),
        ]);
        let chains = s.chains().unwrap();
        assert_eq!(chains.len(), 1);
        assert!(!chains[0].closed);
        assert_eq!(chains[0].edges.len(), 2);
        assert_eq!(chains[0].g1, vec![false]); // a square corner is not G1
        assert!((chains[0].length() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn collinear_joint_is_g1() {
        let s = evaluated(vec![
            pt(1, [0.0, 0.0, 0.0]),
            pt(2, [1.0, 0.0, 0.0]),
            pt(3, [2.0, 0.0, 0.0]),
            line(4, 1, 2),
            line(5, 2, 3),
        ]);
        let chains = s.chains().unwrap();
        assert_eq!(chains[0].g1, vec![true]);
    }

    #[test]
    fn chain_order_is_independent_of_declaration_order() {
        let forward = evaluated(vec![
            pt(1, [0.0, 0.0, 0.0]),
            pt(2, [1.0, 0.0, 0.0]),
            pt(3, [1.0, 1.0, 0.0]),
            line(4, 1, 2),
            line(5, 2, 3),
        ]);
        let backward = evaluated(vec![
            line(5, 2, 3),
            line(4, 1, 2),
            pt(3, [1.0, 1.0, 0.0]),
            pt(2, [1.0, 0.0, 0.0]),
            pt(1, [0.0, 0.0, 0.0]),
        ]);
        assert_eq!(forward.chains().unwrap(), backward.chains().unwrap());
    }

    #[test]
    fn closed_square_is_a_closed_chain_with_four_joints() {
        let s = evaluated(vec![
            pt(1, [0.0, 0.0, 0.0]),
            pt(2, [1.0, 0.0, 0.0]),
            pt(3, [1.0, 1.0, 0.0]),
            pt(4, [0.0, 1.0, 0.0]),
            line(5, 1, 2),
            line(6, 2, 3),
            line(7, 3, 4),
            line(8, 4, 1),
        ]);
        let chains = s.chains().unwrap();
        assert_eq!(chains.len(), 1);
        assert!(chains[0].closed);
        assert_eq!(chains[0].edges.len(), 4);
        assert_eq!(chains[0].g1.len(), 4);
        assert!(chains[0].g1.iter().all(|&g| !g));
        assert!((chains[0].length() - 4.0).abs() < 1e-12);
    }

    #[test]
    fn a_branch_point_splits_the_graph_into_runs() {
        // A "T": three segments meeting at point 1.
        let s = evaluated(vec![
            pt(1, [0.0, 0.0, 0.0]),
            pt(2, [1.0, 0.0, 0.0]),
            pt(3, [-1.0, 0.0, 0.0]),
            pt(4, [0.0, 1.0, 0.0]),
            line(5, 1, 2),
            line(6, 1, 3),
            line(7, 1, 4),
        ]);
        let chains = s.chains().unwrap();
        assert_eq!(chains.len(), 3);
        assert!(chains.iter().all(|c| c.edges.len() == 1 && !c.closed));
    }

    #[test]
    fn a_three_point_arc_recovers_its_circle() {
        // Quarter circle of radius 1 in the XY plane, centre at the origin.
        let s = evaluated(vec![
            pt(1, [1.0, 0.0, 0.0]),
            pt(2, [0.0, 1.0, 0.0]),
            pt(
                3,
                [
                    std::f64::consts::FRAC_1_SQRT_2,
                    std::f64::consts::FRAC_1_SQRT_2,
                    0.0,
                ],
            ),
            Sketch3dEntity::Arc {
                id: 4,
                start_id: 1,
                end_id: 2,
                via_id: 3,
                construction: false,
            },
        ]);
        let chains = s.chains().unwrap();
        let Edge3dKind::Arc {
            center,
            normal,
            radius,
        } = chains[0].edges[0].kind
        else {
            panic!("expected an arc");
        };
        assert!(close(center, [0.0, 0.0, 0.0], 1e-12));
        assert!((radius - 1.0).abs() < 1e-12);
        assert!(close(normal, [0.0, 0.0, 1.0], 1e-12));
        assert!((chains[0].length() - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    }

    #[test]
    fn collinear_arc_points_are_loud() {
        let s = evaluated(vec![
            pt(1, [0.0, 0.0, 0.0]),
            pt(2, [2.0, 0.0, 0.0]),
            pt(3, [1.0, 0.0, 0.0]),
            Sketch3dEntity::Arc {
                id: 4,
                start_id: 1,
                end_id: 2,
                via_id: 3,
                construction: false,
            },
        ]);
        assert!(matches!(
            s.chains().unwrap_err(),
            Sketch3dError::ArcPointsDegenerate { entity_id: 4 }
        ));
    }

    #[test]
    fn a_fillet_is_exactly_tangent_to_both_segments() {
        // Right-angle corner at the origin, legs along +X and +Y, r = 0.25.
        let r = 0.25;
        let s = evaluated(vec![
            pt(1, [-1.0, 0.0, 0.0]),
            pt(2, [0.0, 0.0, 0.0]),
            pt(3, [0.0, 1.0, 0.0]),
            line(4, 1, 2),
            line(5, 2, 3),
            Sketch3dEntity::Fillet {
                id: 6,
                at_point_id: 2,
                radius: r,
                radius_expr: None,
            },
        ]);
        let chains = s.chains().unwrap();
        assert_eq!(chains.len(), 1);
        let c = &chains[0];
        assert_eq!(c.edges.len(), 3, "two trimmed legs plus the arc");

        // Both joints are tangent — that is the whole point of a fillet.
        assert_eq!(c.g1, vec![true, true]);

        let arc = c
            .edges
            .iter()
            .find(|e| matches!(e.kind, Edge3dKind::Arc { .. }))
            .unwrap();
        let Edge3dKind::Arc { center, radius, .. } = arc.kind else {
            unreachable!()
        };
        assert!((radius - r).abs() < 1e-15);
        // For a 90° corner the setback equals the radius and the centre sits
        // at (−r, r) from the corner.
        assert!(close(center, [-r, r, 0.0], 1e-12));

        // Tangent points are exactly on the legs.
        let ends: Vec<[f64; 3]> = vec![arc.a, arc.b];
        assert!(ends.iter().any(|p| close(*p, [-r, 0.0, 0.0], 1e-12)));
        assert!(ends.iter().any(|p| close(*p, [0.0, r, 0.0], 1e-12)));

        // Total length: two 0.75 legs plus a quarter circle of radius r.
        let expect = 0.75 + 0.75 + std::f64::consts::FRAC_PI_2 * r;
        assert!((c.length() - expect).abs() < 1e-12);
    }

    #[test]
    fn a_fillet_out_of_plane_is_still_exactly_tangent() {
        // Corner between +X and a leg heading into +Y+Z: the arc's plane is
        // not a coordinate plane, and tangency must still be exact.
        let s = evaluated(vec![
            pt(1, [-1.0, 0.0, 0.0]),
            pt(2, [0.0, 0.0, 0.0]),
            pt(3, [0.0, 1.0, 1.0]),
            line(4, 1, 2),
            line(5, 2, 3),
            Sketch3dEntity::Fillet {
                id: 6,
                at_point_id: 2,
                radius: 0.2,
                radius_expr: None,
            },
        ]);
        let c = &s.chains().unwrap()[0];
        assert_eq!(c.g1, vec![true, true]);
    }

    #[test]
    fn a_fillet_too_large_for_its_legs_is_loud() {
        let s = evaluated(vec![
            pt(1, [-1.0, 0.0, 0.0]),
            pt(2, [0.0, 0.0, 0.0]),
            pt(3, [0.0, 1.0, 0.0]),
            line(4, 1, 2),
            line(5, 2, 3),
            Sketch3dEntity::Fillet {
                id: 6,
                at_point_id: 2,
                radius: 5.0,
                radius_expr: None,
            },
        ]);
        let err = s.chains().unwrap_err();
        let Sketch3dError::FilletTooLarge { max_radius, .. } = err else {
            panic!("expected FilletTooLarge, got {err}");
        };
        // A 90° corner on unit legs admits r ≤ 1.
        assert!(
            (max_radius - 1.0).abs() < 1e-12,
            "max_radius = {max_radius}"
        );
    }

    #[test]
    fn two_fillets_on_one_segment_see_each_others_setback() {
        // A 1 m middle segment with a 90° corner at each end: each fillet eats
        // `r` of it, so r = 0.6 twice cannot fit even though 0.6 < 1.
        let entities = |r: f64| {
            vec![
                pt(1, [0.0, -1.0, 0.0]),
                pt(2, [0.0, 0.0, 0.0]),
                pt(3, [1.0, 0.0, 0.0]),
                pt(4, [1.0, 1.0, 0.0]),
                line(5, 1, 2),
                line(6, 2, 3),
                line(7, 3, 4),
                Sketch3dEntity::Fillet {
                    id: 8,
                    at_point_id: 2,
                    radius: r,
                    radius_expr: None,
                },
                Sketch3dEntity::Fillet {
                    id: 9,
                    at_point_id: 3,
                    radius: r,
                    radius_expr: None,
                },
            ]
        };
        assert!(matches!(
            evaluated(entities(0.6)).chains().unwrap_err(),
            Sketch3dError::FilletTooLarge { .. }
        ));
        let ok = evaluated(entities(0.4));
        let c = &ok.chains().unwrap()[0];
        assert_eq!(c.edges.len(), 5);
        assert_eq!(c.g1, vec![true, true, true, true]);
    }

    #[test]
    fn a_fillet_on_a_straight_joint_is_loud() {
        let s = evaluated(vec![
            pt(1, [0.0, 0.0, 0.0]),
            pt(2, [1.0, 0.0, 0.0]),
            pt(3, [2.0, 0.0, 0.0]),
            line(4, 1, 2),
            line(5, 2, 3),
            Sketch3dEntity::Fillet {
                id: 6,
                at_point_id: 2,
                radius: 0.1,
                radius_expr: None,
            },
        ]);
        assert!(matches!(
            s.chains().unwrap_err(),
            Sketch3dError::FilletCollinear { .. }
        ));
    }

    #[test]
    fn a_fillet_at_a_free_end_is_loud() {
        let s = evaluated(vec![
            pt(1, [0.0, 0.0, 0.0]),
            pt(2, [1.0, 0.0, 0.0]),
            line(3, 1, 2),
            Sketch3dEntity::Fillet {
                id: 4,
                at_point_id: 2,
                radius: 0.1,
                radius_expr: None,
            },
        ]);
        assert!(matches!(
            s.chains().unwrap_err(),
            Sketch3dError::FilletNotTwoSegments { found: 1, .. }
        ));
    }

    #[test]
    fn a_fillet_against_an_arc_is_refused_not_approximated() {
        let s = evaluated(vec![
            pt(1, [1.0, 0.0, 0.0]),
            pt(2, [0.0, 1.0, 0.0]),
            pt(
                3,
                [
                    std::f64::consts::FRAC_1_SQRT_2,
                    std::f64::consts::FRAC_1_SQRT_2,
                    0.0,
                ],
            ),
            pt(4, [0.0, 2.0, 0.0]),
            Sketch3dEntity::Arc {
                id: 5,
                start_id: 1,
                end_id: 2,
                via_id: 3,
                construction: false,
            },
            line(6, 2, 4),
            Sketch3dEntity::Fillet {
                id: 7,
                at_point_id: 2,
                radius: 0.1,
                radius_expr: None,
            },
        ]);
        assert!(matches!(
            s.chains().unwrap_err(),
            Sketch3dError::FilletNeedsStraightSegments { .. }
        ));
    }

    #[test]
    fn degenerate_and_duplicate_inputs_are_loud() {
        let dup = Sketch3d::new(Uuid::nil(), vec![pt(1, [0.0; 3]), pt(1, [1.0; 3])]);
        assert!(matches!(
            dup.chains().unwrap_err(),
            Sketch3dError::DuplicateEntityId { id: 1 }
        ));

        let zero = evaluated(vec![pt(1, [0.0; 3]), pt(2, [0.0; 3]), line(3, 1, 2)]);
        assert!(matches!(
            zero.chains().unwrap_err(),
            Sketch3dError::DegenerateSegment { entity_id: 3 }
        ));

        let missing = evaluated(vec![pt(1, [0.0; 3]), line(2, 1, 99)]);
        assert!(matches!(
            missing.chains().unwrap_err(),
            Sketch3dError::PointNotFound {
                entity_id: 2,
                point_id: 99
            }
        ));
    }

    #[test]
    fn a_planar_3d_chain_matches_the_geometry_a_planar_sketch_would_give() {
        // The sketch3d §10 oracle, in its purely geometric half: a chain whose
        // points are coplanar has edges lying exactly in that plane.
        let s = evaluated(vec![
            pt(1, [0.0, 0.0, 2.0]),
            pt(2, [1.0, 0.0, 2.0]),
            pt(3, [1.0, 1.0, 2.0]),
            line(4, 1, 2),
            line(5, 2, 3),
            Sketch3dEntity::Fillet {
                id: 6,
                at_point_id: 2,
                radius: 0.25,
                radius_expr: None,
            },
        ]);
        let c = &s.chains().unwrap()[0];
        for e in &c.edges {
            assert!((e.a[2] - 2.0).abs() < 1e-15);
            assert!((e.b[2] - 2.0).abs() < 1e-15);
            if let Edge3dKind::Arc { center, normal, .. } = e.kind {
                assert!((center[2] - 2.0).abs() < 1e-15);
                assert!(normal[0].abs() < 1e-15 && normal[1].abs() < 1e-15);
            }
        }
    }
}
