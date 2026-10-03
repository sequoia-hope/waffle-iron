//! The sketch operation set (`specs/agent_mechanical_design.md` §10.1, S1).
//!
//! Before S1 there were two sketchers. The browser owned trim, offset and
//! sketch fillet as JavaScript with no Rust twin, extend did not exist at
//! all, and an agent editing a sketch could only rewrite the whole thing
//! through `sketch_create` (§2.2 item 10). Parity between what a user can do
//! and what an agent can do was a promise nobody could keep, because the two
//! paths shared no code.
//!
//! These types are the single operation vocabulary. [`SketchOp`] is what a
//! caller asks for — the UI's pointer handlers, and `sketch_edit` in S3 —
//! and [`SketchEdit`] is what came of it, as data: entities added, removed
//! and changed, constraints added and removed. The implementations live in
//! `sketch_solver::ops`; the types live here so the bridge and the engine can
//! name an operation without depending on the solver's internals.
//!
//! **Why an edit rather than a mutated sketch.** One undo step is one
//! `SketchEdit`. A tool that mutated the sketch in place would leave the undo
//! stack to reconstruct what changed by diffing, which is exactly the
//! guesswork S2 removed from the solver's own reporting.

use serde::{Deserialize, Serialize};

use crate::sketch::{SketchConstraint, SketchEntity};

/// Which end of a curve an operation acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum End {
    Start,
    End,
}

/// Which side of a chain an offset goes to, in traversal terms.
///
/// `Left` is the positive signed distance of the offset construction. A
/// caller that knows a cursor position rather than a traversal direction
/// (every interactive one) gets the sign from
/// `sketch_solver::ops::offset::signed_distance_to_chain` instead of guessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Side {
    Left,
    Right,
}

impl Side {
    /// The sign this side gives a distance.
    pub fn sign(self) -> f64 {
        match self {
            Side::Left => 1.0,
            Side::Right => -1.0,
        }
    }

    /// The side a signed distance names.
    pub fn of(signed: f64) -> Side {
        if signed < 0.0 {
            Side::Right
        } else {
            Side::Left
        }
    }
}

/// One point to project into the sketch, already resolved to world space by
/// the engine.
///
/// The op does the PLANE mapping (world → sketch uv) and mints the entities;
/// resolving a `GeomRef` to a position needs the model and stays on the
/// engine side of the boundary. `source` carries the binding the sketch
/// stores so a rebuild can reproject
/// (`specs/projected_sketch_geometry.md`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ProjectedPoint {
    /// The resolved 3D world position.
    pub world: [f64; 3],
    /// The binding to re-derive it on rebuild, when the caller has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<crate::sketch::ProjectedSource>,
}

/// What to build from a set of projected points.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ProjectShape {
    /// Loose points only (a projected vertex).
    Points,
    /// A polyline through the points in order; `closed` joins the last back
    /// to the first (a projected edge is the two-point open case; a face
    /// boundary is the closed one).
    Polyline { closed: bool },
}

/// One sketch operation. Applied in order, with a single solve at the end and
/// a single undo step for the batch (§10.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum SketchOp {
    /// Add an entity. Ids of `0` are allocated by the engine, so a caller
    /// that does not track ids (an agent) can still build a line by naming
    /// its points — see `sketch_solver::ops::apply_ops`.
    AddEntity {
        entity: SketchEntity,
    },
    /// Remove entities, cascading to the constraints and orphaned points
    /// that reference them.
    RemoveEntity {
        ids: Vec<u32>,
    },
    AddConstraint {
        constraint: SketchConstraint,
    },
    /// Remove the constraint at this index in the sketch's constraint array.
    RemoveConstraint {
        index: u32,
    },
    /// Retarget a dimension: a literal value, a driving expression, or both
    /// (the expression's last evaluated result is the value).
    SetDimension {
        index: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expression: Option<String>,
    },
    /// Flip an entity's construction flag.
    SetConstruction {
        entity: u32,
        construction: bool,
    },
    /// Move a point to a position. A drag: the solve that follows sees a
    /// `Dragged` hint on the point, which is dropped afterwards, so the move
    /// is a nudge the constraints may overrule — not a pin.
    MovePoint {
        id: u32,
        to: [f64; 2],
    },
    /// Trim the piece of `entity` that contains `at`, cutting at the
    /// intersections with every other entity. With no intersection the whole
    /// entity goes.
    Trim {
        entity: u32,
        at: [f64; 2],
    },
    /// Extend `entity` past `end` until it meets `to` (or the nearest
    /// reachable entity when `to` is absent).
    Extend {
        entity: u32,
        end: End,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<u32>,
    },
    /// Offset a connected chain (or a single circle) by `distance` to `side`.
    Offset {
        chain: Vec<u32>,
        distance: f64,
        side: Side,
    },
    /// Round the corner at a point shared by exactly two lines.
    Fillet {
        corner: u32,
        radius: f64,
    },
    /// Mirror entities across a line, adding the images as new geometry.
    Mirror {
        entities: Vec<u32>,
        axis: u32,
    },
    /// Bring external geometry in, mapped onto the sketch plane.
    Project {
        points: Vec<ProjectedPoint>,
        shape: ProjectShape,
    },
}

/// What an operation did, as data. One undo step.
///
/// `changed` carries whole entities, not deltas: a line whose endpoint moved
/// is the same id with a new field, and the caller applies it by replacement.
// No `PartialEq`: `SketchEntity` and `SketchConstraint` carry generator
// parameter structs that do not derive it, and deriving equality down that
// whole tree to compare two edits in a test would be a wide change for a
// narrow convenience. Tests compare the fields they are about.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct SketchEdit {
    pub added: Vec<SketchEntity>,
    pub removed: Vec<u32>,
    pub changed: Vec<SketchEntity>,
    pub constraints_added: Vec<SketchConstraint>,
    /// Indices into the constraint array AS IT WAS when the op ran, so a
    /// caller can report which constraint went. Removal is applied by the
    /// engine; a caller replaying an edit reads the resulting sketch.
    pub constraints_removed: Vec<u32>,
    /// Projected-point bindings the op added (`Project` only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projected_added: Vec<crate::sketch::ProjectedEntity>,
}

impl SketchEdit {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.changed.is_empty()
            && self.constraints_added.is_empty()
            && self.constraints_removed.is_empty()
            && self.projected_added.is_empty()
    }

    /// Fold another edit into this one, in order.
    pub fn extend(&mut self, other: SketchEdit) {
        self.added.extend(other.added);
        self.removed.extend(other.removed);
        self.changed.extend(other.changed);
        self.constraints_added.extend(other.constraints_added);
        self.constraints_removed.extend(other.constraints_removed);
        self.projected_added.extend(other.projected_added);
    }
}

/// Why an operation was refused.
///
/// Every variant is a refusal a user can act on, and none of them is a
/// silent no-op: the JS tools returned early on most of these, so a trim that
/// could not find its corner looked exactly like a trim that worked
/// (§2.2 item 10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum SketchOpError {
    /// No entity with this id in the sketch.
    NoSuchEntity { id: u32 },
    /// No constraint at this index.
    NoSuchConstraint { index: u32 },
    /// A point id the geometry needs has no position.
    MissingPosition { id: u32 },
    /// The op needs a different kind of entity than the one named.
    WrongEntityKind {
        id: u32,
        expected: String,
        found: String,
    },
    /// The constraint at this index carries no dimension value.
    NotADimension { index: u32 },
    /// A corner must be a point shared by exactly two lines.
    NotACorner { point: u32, lines: u32 },
    /// A fillet radius that does not fit between the two lines, or lines too
    /// nearly parallel to round.
    FilletDoesNotFit { corner: u32, radius: f64 },
    /// Nothing for an extend to reach.
    NothingToExtendTo { entity: u32 },
    /// The chain could not be ordered or offset; `reason` is the typed tag.
    OffsetRefused { reason: String },
    /// A mirror axis that is not a line, or geometry on the axis itself.
    MirrorRefused { reason: String },
    /// A projection with nothing to project, or a plane it cannot map onto.
    ProjectRefused { reason: String },
}

impl std::fmt::Display for SketchOpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SketchOpError::NoSuchEntity { id } => write!(f, "no sketch entity with id {id}"),
            SketchOpError::NoSuchConstraint { index } => {
                write!(f, "no constraint at index {index}")
            }
            SketchOpError::MissingPosition { id } => {
                write!(f, "point {id} has no position in this sketch")
            }
            SketchOpError::WrongEntityKind {
                id,
                expected,
                found,
            } => write!(f, "entity {id} is a {found}, and this needs a {expected}"),
            SketchOpError::NotADimension { index } => {
                write!(
                    f,
                    "the constraint at index {index} carries no dimension value"
                )
            }
            SketchOpError::NotACorner { point, lines } => write!(
                f,
                "point {point} is shared by {lines} lines; a corner needs exactly 2"
            ),
            SketchOpError::FilletDoesNotFit { corner, radius } => write!(
                f,
                "a fillet of radius {radius} does not fit at corner {corner}"
            ),
            SketchOpError::NothingToExtendTo { entity } => {
                write!(f, "entity {entity} has nothing to extend to")
            }
            SketchOpError::OffsetRefused { reason } => write!(f, "offset refused: {reason}"),
            SketchOpError::MirrorRefused { reason } => write!(f, "mirror refused: {reason}"),
            SketchOpError::ProjectRefused { reason } => write!(f, "projection refused: {reason}"),
        }
    }
}

impl std::error::Error for SketchOpError {}
