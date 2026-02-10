use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::geom_ref::GeomRef;

/// Serde helper for HashMap<u32, (f64, f64)>.
/// JSON only supports string keys, so we need custom (de)serialization.
mod u32_key_map {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::HashMap;

    pub fn serialize<S>(map: &HashMap<u32, (f64, f64)>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Convert to string keys for JSON
        let string_map: HashMap<String, (f64, f64)> =
            map.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        string_map.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<HashMap<u32, (f64, f64)>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let string_map: HashMap<String, (f64, f64)> = HashMap::deserialize(deserializer)?;
        string_map
            .into_iter()
            .map(|(k, v)| {
                k.parse::<u32>()
                    .map(|key| (key, v))
                    .map_err(serde::de::Error::custom)
            })
            .collect()
    }
}

/// A 2D sketch on a plane. Contains geometric entities and constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sketch {
    /// Unique identifier for this sketch.
    pub id: Uuid,
    /// The plane this sketch lies on, referenced via GeomRef.
    pub plane: GeomRef,
    /// Geometric entities in this sketch.
    pub entities: Vec<SketchEntity>,
    /// Constraints between entities.
    pub constraints: Vec<SketchConstraint>,
    /// Current solve status (updated after each solve).
    pub solve_status: SolveStatus,
    /// Solved positions for all points. Key is point entity ID.
    #[serde(default, with = "u32_key_map")]
    pub solved_positions: HashMap<u32, (f64, f64)>,
    /// Closed profiles extracted from the solved geometry.
    #[serde(default)]
    pub solved_profiles: Vec<ClosedProfile>,
}

/// A geometric entity in a sketch.
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

/// Result of running the constraint solver.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SolveStatus {
    /// All constraints satisfied, zero degrees of freedom.
    FullyConstrained,
    /// All constraints satisfied, but geometry can still move.
    UnderConstrained { dof: u32 },
    /// Constraints are contradictory.
    OverConstrained { conflicts: Vec<u32> },
    /// Solver failed to converge.
    SolveFailed { reason: String },
}

/// Output of the constraint solver: solved positions and extracted profiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolvedSketch {
    /// Solved positions for all points. Key is point entity ID.
    pub positions: HashMap<u32, (f64, f64)>,
    /// Closed profiles extracted from the solved geometry.
    pub profiles: Vec<ClosedProfile>,
    /// Solve status.
    pub status: SolveStatus,
}

/// A closed loop of sketch entities suitable for extrusion or revolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosedProfile {
    /// Ordered entity IDs forming the closed loop.
    pub entity_ids: Vec<u32>,
    /// Whether the profile winds counter-clockwise (outward) or clockwise (hole).
    pub is_outer: bool,
}
