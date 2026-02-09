use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ── Sketch Input Types ──────────────────────────────────────────────────────

/// A 2D sketch on a plane. Contains geometric entities and constraints.
/// The sketch is the input to the constraint solver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sketch {
    pub id: Uuid,
    pub plane: GeomRef,
    pub entities: Vec<SketchEntity>,
    pub constraints: Vec<SketchConstraint>,
    pub solve_status: SolveStatus,
}

/// A geometric entity in a sketch.
/// Each entity has a unique ID (u32) for referencing in constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SketchEntity {
    Point {
        id: u32,
        x: f64,
        y: f64,
        construction: bool,
    },
    Line {
        id: u32,
        start_id: u32,
        end_id: u32,
        construction: bool,
    },
    Circle {
        id: u32,
        center_id: u32,
        radius: f64,
        construction: bool,
    },
    Arc {
        id: u32,
        center_id: u32,
        start_id: u32,
        end_id: u32,
        construction: bool,
    },
}

impl SketchEntity {
    pub fn id(&self) -> u32 {
        match self {
            SketchEntity::Point { id, .. }
            | SketchEntity::Line { id, .. }
            | SketchEntity::Circle { id, .. }
            | SketchEntity::Arc { id, .. } => *id,
        }
    }

    pub fn is_construction(&self) -> bool {
        match self {
            SketchEntity::Point { construction, .. }
            | SketchEntity::Line { construction, .. }
            | SketchEntity::Circle { construction, .. }
            | SketchEntity::Arc { construction, .. } => *construction,
        }
    }
}

/// A constraint between sketch entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SketchConstraint {
    Coincident {
        point_a: u32,
        point_b: u32,
    },
    Horizontal {
        entity: u32,
    },
    Vertical {
        entity: u32,
    },
    Parallel {
        line_a: u32,
        line_b: u32,
    },
    Perpendicular {
        line_a: u32,
        line_b: u32,
    },
    Tangent {
        line: u32,
        curve: u32,
    },
    Equal {
        entity_a: u32,
        entity_b: u32,
    },
    Symmetric {
        entity_a: u32,
        entity_b: u32,
        symmetry_line: u32,
    },
    SymmetricH {
        point_a: u32,
        point_b: u32,
    },
    SymmetricV {
        point_a: u32,
        point_b: u32,
    },
    Midpoint {
        point: u32,
        line: u32,
    },
    Distance {
        entity_a: u32,
        entity_b: u32,
        value: f64,
    },
    Angle {
        line_a: u32,
        line_b: u32,
        value_degrees: f64,
    },
    Radius {
        entity: u32,
        value: f64,
    },
    Diameter {
        entity: u32,
        value: f64,
    },
    OnEntity {
        point: u32,
        entity: u32,
    },
    Dragged {
        point: u32,
    },
    EqualAngle {
        line_a: u32,
        line_b: u32,
        line_c: u32,
        line_d: u32,
    },
    Ratio {
        entity_a: u32,
        entity_b: u32,
        value: f64,
    },
    EqualPointToLine {
        point_a: u32,
        point_b: u32,
        line: u32,
    },
    SameOrientation {
        entity_a: u32,
        entity_b: u32,
    },
}

// ── Solver Output Types ─────────────────────────────────────────────────────

/// Result of running the constraint solver.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SolveStatus {
    FullyConstrained,
    UnderConstrained { dof: u32 },
    OverConstrained { conflicts: Vec<u32> },
    SolveFailed { reason: String },
}

/// Output of the constraint solver: solved positions and extracted profiles.
#[derive(Debug, Clone)]
pub struct SolvedSketch {
    pub positions: HashMap<u32, (f64, f64)>,
    pub profiles: Vec<ClosedProfile>,
    pub status: SolveStatus,
}

/// A closed loop of sketch entities suitable for extrusion or revolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosedProfile {
    pub entity_ids: Vec<u32>,
    pub is_outer: bool,
}

// ── Geometry Reference Types ────────────────────────────────────────────────

/// Persistent geometry reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeomRef {
    pub kind: TopoKind,
    pub anchor: Anchor,
    pub selector: Selector,
    pub policy: ResolvePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TopoKind {
    Vertex,
    Edge,
    Face,
    Shell,
    Solid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Anchor {
    FeatureOutput {
        feature_id: Uuid,
        output_key: OutputKey,
    },
    Datum {
        datum_id: Uuid,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OutputKey {
    Main,
    Body { index: usize },
    Profile { index: usize },
    Datum { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Selector {
    Role { role: Role, index: usize },
    Signature { signature: TopoSignature },
    Query { query: TopoQuery },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResolvePolicy {
    Strict,
    BestEffort,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Role {
    EndCapPositive,
    EndCapNegative,
    SideFace { index: usize },
    RevStartFace,
    RevEndFace,
    FilletFace { index: usize },
    ChamferFace { index: usize },
    ShellInnerFace { index: usize },
    ProfileFace,
    PatternInstance { index: usize },
    BooleanBodyAFace { index: usize },
    BooleanBodyBFace { index: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopoSignature {
    pub surface_type: Option<String>,
    pub area: Option<f64>,
    pub centroid: Option<[f64; 3]>,
    pub normal: Option<[f64; 3]>,
    pub bbox: Option<[f64; 6]>,
    pub adjacency_hash: Option<u64>,
    pub length: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopoQuery {
    pub filters: Vec<Filter>,
    pub tie_break: Option<TieBreak>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Filter {
    SurfaceType { surface_type: String },
    NormalDirection { direction: [f64; 3], tolerance: f64 },
    NearPoint { point: [f64; 3], distance: f64 },
    AreaRange { min: f64, max: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TieBreak {
    LargestArea,
    NearestTo { point: [f64; 3] },
    SmallestIndex,
}

// ── Entity Kind (for constraint resolution) ─────────────────────────────────

/// Internal classification of entity types for constraint dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    Point,
    Line,
    Circle,
    Arc,
}
