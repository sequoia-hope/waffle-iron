use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::gear::{generate_gear_profile, GearParams};
use crate::geom_ref::GeomRef;

/// Serde helper for HashMap<u32, (f64, f64)>.
/// JSON only supports string keys, so we need custom (de)serialization.
pub(crate) mod u32_key_map {
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
    /// Origin of the sketch plane in 3D world space.
    #[serde(default = "default_origin")]
    pub plane_origin: [f64; 3],
    /// Normal of the sketch plane in 3D world space.
    #[serde(default = "default_normal")]
    pub plane_normal: [f64; 3],
    /// Geometric entities in this sketch.
    pub entities: Vec<SketchEntity>,
    /// Constraints between entities.
    pub constraints: Vec<SketchConstraint>,
    /// Current solve status (updated after each solve).
    pub solve_status: SolveStatus,
    /// Solved positions for all points. Key is point entity ID.
    /// Derived data — serialized when populated (for WASM→JS bridge), skipped when empty.
    #[serde(
        default,
        with = "u32_key_map",
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub solved_positions: HashMap<u32, (f64, f64)>,
    /// Closed profiles extracted from the solved geometry.
    /// Derived data — serialized when populated (for WASM→JS bridge), skipped when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub solved_profiles: Vec<ClosedProfile>,
    /// Projected-geometry bindings: sketch points that are driven by external
    /// model geometry (a vertex/edge/face of an upstream feature). Each binding
    /// maps a local Point id to the source it reprojects from on rebuild. Empty
    /// for ordinary sketches. See `specs/projected_sketch_geometry.md`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projected: Vec<ProjectedEntity>,
}

/// A binding from a local sketch Point to the external geometry it projects.
/// The point remains an ordinary `SketchEntity::Point`; this side-table marks it
/// as externally driven so rebuild can re-derive its 2D position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectedEntity {
    /// The local Point entity id this binding drives.
    pub point_id: u32,
    /// Where the point's position comes from.
    pub source: ProjectedSource,
}

/// The external source a projected point reprojects from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectedSource {
    /// Reference to the source vertex/edge/face in an upstream feature output.
    pub geom_ref: GeomRef,
    /// How to derive a 3D point from the resolved source entity.
    pub kind: ProjectedKind,
}

/// How a projected point is derived from its resolved source entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProjectedKind {
    /// Source is a vertex; use its position directly.
    Vertex,
    /// Source is an edge; use the point at parameter `t` along it
    /// (`t = 0` / `t = 1` are the endpoints; interior is exact for straight edges).
    EdgeSample { t: f64 },
}

impl Sketch {
    /// Recompute derived data (`solved_positions` and `solved_profiles`) from entities.
    ///
    /// This must be called after deserialization, since these fields are not persisted.
    /// Reconstructs positions from Point entity x/y values, expands Gear entities,
    /// and extracts closed profiles from the entity graph.
    pub fn recompute_derived(&mut self) {
        // Step 1: Reconstruct solved_positions from Point entities' x/y values.
        // Only populate if empty (don't overwrite live session data).
        if self.solved_positions.is_empty() {
            for entity in &self.entities {
                if let SketchEntity::Point { id, x, y, .. } = entity {
                    self.solved_positions.insert(*id, (*x, *y));
                }
            }
        }

        // Step 2: Expand gear entities (populates positions + profiles from gear generator)
        self.expand_gears();

        // Step 3: Extract profiles from remaining entities if still empty
        if self.solved_profiles.is_empty() {
            self.solved_profiles =
                crate::profiles::extract_profiles(&self.entities, &self.solved_positions);
        }
    }

    /// Expand all `Gear` entities into their primitive equivalents (Points, Lines, Arcs, Splines).
    /// Populates `solved_positions` and `solved_profiles` from the gear profile results.
    /// This is a no-op if the sketch has no Gear entities.
    pub fn expand_gears(&mut self) {
        let has_gears = self
            .entities
            .iter()
            .any(|e| matches!(e, SketchEntity::Gear { .. }));
        if !has_gears {
            return;
        }

        let mut expanded_entities = Vec::new();
        for entity in &self.entities {
            match entity {
                SketchEntity::Gear { params, .. } => {
                    let result = generate_gear_profile(params);
                    expanded_entities.extend(result.entities);
                    self.solved_positions.extend(result.positions);
                    self.solved_profiles.extend(result.profiles);
                }
                other => {
                    expanded_entities.push(other.clone());
                }
            }
        }
        self.entities = expanded_entities;
    }

    /// Recompute derived data (`solved_positions` and `solved_profiles`) from
    /// the sketch's entities. Called during rebuild after deserialization, since
    /// these fields are not serialized.
    ///
    /// If `solved_profiles` is already non-empty, this is a no-op (preserves
    /// profiles set by interactive solving or gear expansion).
    pub fn recompute_derived_data(&mut self) {
        // B8: If profiles already exist (e.g., from interactive session), preserve them
        if !self.solved_profiles.is_empty() {
            return;
        }

        // Build solved_positions from Point entity coordinates
        if self.solved_positions.is_empty() {
            for entity in &self.entities {
                if let SketchEntity::Point { id, x, y, .. } = entity {
                    self.solved_positions.insert(*id, (*x, *y));
                }
            }
        }

        // Extract profiles from entities + positions
        if !self.entities.is_empty() {
            self.solved_profiles =
                crate::profiles::extract_profiles(&self.entities, &self.solved_positions);
        }
    }
}

fn default_origin() -> [f64; 3] {
    [0.0, 0.0, 0.0]
}

fn default_normal() -> [f64; 3] {
    [0.0, 0.0, 1.0]
}

/// A geometric entity in a sketch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SketchEntity {
    Point {
        id: u32,
        x: f64,
        y: f64,
        #[serde(default)]
        construction: bool,
    },
    Line {
        id: u32,
        start_id: u32,
        end_id: u32,
        #[serde(default)]
        construction: bool,
    },
    Circle {
        id: u32,
        center_id: u32,
        radius: f64,
        #[serde(default)]
        construction: bool,
    },
    Arc {
        id: u32,
        center_id: u32,
        start_id: u32,
        end_id: u32,
        #[serde(default)]
        construction: bool,
    },
    Spline {
        id: u32,
        point_ids: Vec<u32>,
        #[serde(default)]
        construction: bool,
    },
    /// A parametric gear profile. Stored compactly; expanded to primitives on demand.
    Gear {
        id: u32,
        params: GearParams,
        #[serde(default)]
        construction: bool,
    },
}

impl SketchEntity {
    pub fn id(&self) -> u32 {
        match self {
            SketchEntity::Point { id, .. }
            | SketchEntity::Line { id, .. }
            | SketchEntity::Circle { id, .. }
            | SketchEntity::Arc { id, .. }
            | SketchEntity::Spline { id, .. }
            | SketchEntity::Gear { id, .. } => *id,
        }
    }

    pub fn is_construction(&self) -> bool {
        match self {
            SketchEntity::Point { construction, .. }
            | SketchEntity::Line { construction, .. }
            | SketchEntity::Circle { construction, .. }
            | SketchEntity::Arc { construction, .. }
            | SketchEntity::Spline { construction, .. }
            | SketchEntity::Gear { construction, .. } => *construction,
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
    /// Solved radii for circles (and any radius-param entity), keyed by entity
    /// ID. A Diameter/Radius constraint solves the radius param; without this it
    /// never reaches the UI (only points flow through `positions`). Arcs are
    /// absent (their radius is the center→start distance, captured by points).
    #[serde(default)]
    pub radii: HashMap<u32, f64>,
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
    /// Ordered point IDs in geometric winding order for polygon construction.
    /// When non-empty, the kernel uses these instead of entity_ids or sorted keys.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vertex_ids: Vec<u32>,
    /// If this profile is a standalone circle, its center and radius in sketch UV coordinates.
    /// When present, the kernel constructs a true NURBS circular wire instead of a polygon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circle: Option<CircleProfile>,
    /// Segments that should be built as B-spline curves instead of lines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spline_segments: Vec<SplineSegment>,
    /// Arc segments within the polygon, used to assign cylindrical face geometry on extrude.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arc_segments: Vec<ArcSegment>,
}

/// Circle profile data in sketch-local UV coordinates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CircleProfile {
    pub center_u: f64,
    pub center_v: f64,
    pub radius: f64,
}

/// A segment of a profile that should be built as a B-spline curve instead of a line.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SplineSegment {
    /// Index into the profile's entity_ids where the spline starts.
    pub start_point_index: usize,
    /// Index into the profile's entity_ids where the spline ends.
    pub end_point_index: usize,
    /// Control points in sketch UV coordinates.
    pub control_points: Vec<(f64, f64)>,
}

/// An arc segment within a polygon profile, used to assign cylindrical face geometry on extrude.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArcSegment {
    /// Index into vertex_ids where the arc's sampled points begin.
    pub start_vertex_index: usize,
    /// Index into vertex_ids where the arc's sampled points end (inclusive).
    pub end_vertex_index: usize,
    /// Arc center in sketch UV coordinates.
    pub center_u: f64,
    pub center_v: f64,
    /// Arc radius.
    pub radius: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ── SketchEntity::id() ────────────────────────────────────────────

    #[test]
    fn entity_id_point() {
        let e = SketchEntity::Point {
            id: 42,
            x: 1.0,
            y: 2.0,
            construction: false,
        };
        assert_eq!(e.id(), 42);
    }

    #[test]
    fn entity_id_line() {
        let e = SketchEntity::Line {
            id: 7,
            start_id: 1,
            end_id: 2,
            construction: false,
        };
        assert_eq!(e.id(), 7);
    }

    #[test]
    fn entity_id_circle() {
        let e = SketchEntity::Circle {
            id: 99,
            center_id: 1,
            radius: 5.0,
            construction: false,
        };
        assert_eq!(e.id(), 99);
    }

    #[test]
    fn entity_id_arc() {
        let e = SketchEntity::Arc {
            id: 33,
            center_id: 1,
            start_id: 2,
            end_id: 3,
            construction: false,
        };
        assert_eq!(e.id(), 33);
    }

    // ── SketchEntity::is_construction() ───────────────────────────────

    #[test]
    fn is_construction_point() {
        let e = SketchEntity::Point {
            id: 1,
            x: 0.0,
            y: 0.0,
            construction: true,
        };
        assert!(e.is_construction());

        let e2 = SketchEntity::Point {
            id: 2,
            x: 0.0,
            y: 0.0,
            construction: false,
        };
        assert!(!e2.is_construction());
    }

    #[test]
    fn is_construction_line() {
        let e = SketchEntity::Line {
            id: 1,
            start_id: 1,
            end_id: 2,
            construction: true,
        };
        assert!(e.is_construction());
    }

    #[test]
    fn is_construction_circle() {
        let e = SketchEntity::Circle {
            id: 1,
            center_id: 1,
            radius: 5.0,
            construction: true,
        };
        assert!(e.is_construction());
    }

    #[test]
    fn is_construction_arc() {
        let e = SketchEntity::Arc {
            id: 1,
            center_id: 1,
            start_id: 2,
            end_id: 3,
            construction: true,
        };
        assert!(e.is_construction());
    }

    // ── SolveStatus serde roundtrip ───────────────────────────────────

    #[test]
    fn solve_status_fully_constrained_roundtrip() {
        let s = SolveStatus::FullyConstrained;
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("FullyConstrained"));
        let d: SolveStatus = serde_json::from_str(&json).unwrap();
        assert!(matches!(d, SolveStatus::FullyConstrained));
    }

    #[test]
    fn solve_status_under_constrained_roundtrip() {
        let s = SolveStatus::UnderConstrained { dof: 3 };
        let json = serde_json::to_string(&s).unwrap();
        let d: SolveStatus = serde_json::from_str(&json).unwrap();
        assert!(matches!(d, SolveStatus::UnderConstrained { dof: 3 }));
    }

    #[test]
    fn solve_status_over_constrained_roundtrip() {
        let s = SolveStatus::OverConstrained {
            conflicts: vec![1, 5, 9],
        };
        let json = serde_json::to_string(&s).unwrap();
        let d: SolveStatus = serde_json::from_str(&json).unwrap();
        if let SolveStatus::OverConstrained { conflicts } = d {
            assert_eq!(conflicts, vec![1, 5, 9]);
        } else {
            panic!("Expected OverConstrained");
        }
    }

    #[test]
    fn solve_status_solve_failed_roundtrip() {
        let s = SolveStatus::SolveFailed {
            reason: "diverged".to_string(),
        };
        let json = serde_json::to_string(&s).unwrap();
        let d: SolveStatus = serde_json::from_str(&json).unwrap();
        if let SolveStatus::SolveFailed { reason } = d {
            assert_eq!(reason, "diverged");
        } else {
            panic!("Expected SolveFailed");
        }
    }

    // ── SketchEntity serde roundtrip ──────────────────────────────────

    #[test]
    fn sketch_entity_point_serde() {
        let e = SketchEntity::Point {
            id: 1,
            x: 3.14,
            y: -2.7,
            construction: true,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"type\":\"Point\""));
        let d: SketchEntity = serde_json::from_str(&json).unwrap();
        assert_eq!(d.id(), 1);
        assert!(d.is_construction());
    }

    #[test]
    fn sketch_entity_arc_serde() {
        let e = SketchEntity::Arc {
            id: 10,
            center_id: 1,
            start_id: 2,
            end_id: 3,
            construction: false,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"type\":\"Arc\""));
        let d: SketchEntity = serde_json::from_str(&json).unwrap();
        assert_eq!(d.id(), 10);
        assert!(!d.is_construction());
    }

    #[test]
    fn sketch_entity_circle_serde() {
        let e = SketchEntity::Circle {
            id: 5,
            center_id: 1,
            radius: 7.5,
            construction: false,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"type\":\"Circle\""));
        let d: SketchEntity = serde_json::from_str(&json).unwrap();
        assert_eq!(d.id(), 5);
    }

    #[test]
    fn sketch_entity_spline_serde() {
        let e = SketchEntity::Spline {
            id: 20,
            point_ids: vec![1, 2, 3, 4],
            construction: false,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"type\":\"Spline\""));
        let d: SketchEntity = serde_json::from_str(&json).unwrap();
        assert_eq!(d.id(), 20);
        assert!(!d.is_construction());
        if let SketchEntity::Spline { point_ids, .. } = d {
            assert_eq!(point_ids, vec![1, 2, 3, 4]);
        } else {
            panic!("Expected Spline");
        }
    }

    #[test]
    fn sketch_entity_spline_construction_serde() {
        let e = SketchEntity::Spline {
            id: 21,
            point_ids: vec![5, 6],
            construction: true,
        };
        let json = serde_json::to_string(&e).unwrap();
        let d: SketchEntity = serde_json::from_str(&json).unwrap();
        assert_eq!(d.id(), 21);
        assert!(d.is_construction());
    }

    #[test]
    fn spline_segment_serde() {
        let s = SplineSegment {
            start_point_index: 0,
            end_point_index: 3,
            control_points: vec![(0.0, 0.0), (1.0, 2.0), (3.0, 1.0), (4.0, 0.0)],
        };
        let json = serde_json::to_string(&s).unwrap();
        let d: SplineSegment = serde_json::from_str(&json).unwrap();
        assert_eq!(d.start_point_index, 0);
        assert_eq!(d.end_point_index, 3);
        assert_eq!(d.control_points.len(), 4);
    }

    #[test]
    fn closed_profile_with_spline_segments_serde() {
        let p = ClosedProfile {
            entity_ids: vec![1, 2, 3],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![SplineSegment {
                start_point_index: 0,
                end_point_index: 2,
                control_points: vec![(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)],
            }],
            arc_segments: vec![],
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("spline_segments"));
        let d: ClosedProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(d.spline_segments.len(), 1);
        assert_eq!(d.spline_segments[0].control_points.len(), 3);
    }

    #[test]
    fn closed_profile_spline_segments_default_on_deserialize() {
        // Deserializing old format without spline_segments should default to empty vec
        let json = r#"{"entity_ids":[1,2],"is_outer":true}"#;
        let d: ClosedProfile = serde_json::from_str(json).unwrap();
        assert!(d.spline_segments.is_empty());
        assert!(d.circle.is_none());
    }

    // ── SketchConstraint serde roundtrip (all variants) ───────────────

    #[test]
    fn constraint_coincident_serde() {
        let c = SketchConstraint::Coincident {
            point_a: 1,
            point_b: 2,
        };
        let json = serde_json::to_string(&c).unwrap();
        let d: SketchConstraint = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            d,
            SketchConstraint::Coincident {
                point_a: 1,
                point_b: 2
            }
        ));
    }

    #[test]
    fn constraint_distance_serde() {
        let c = SketchConstraint::Distance {
            entity_a: 1,
            entity_b: 2,
            value: 42.5,
        };
        let json = serde_json::to_string(&c).unwrap();
        let d: SketchConstraint = serde_json::from_str(&json).unwrap();
        if let SketchConstraint::Distance {
            entity_a,
            entity_b,
            value,
        } = d
        {
            assert_eq!(entity_a, 1);
            assert_eq!(entity_b, 2);
            assert!((value - 42.5).abs() < 1e-10);
        } else {
            panic!("Expected Distance");
        }
    }

    #[test]
    fn constraint_angle_serde() {
        let c = SketchConstraint::Angle {
            line_a: 5,
            line_b: 6,
            value_degrees: 90.0,
        };
        let json = serde_json::to_string(&c).unwrap();
        let d: SketchConstraint = serde_json::from_str(&json).unwrap();
        if let SketchConstraint::Angle {
            line_a,
            line_b,
            value_degrees,
        } = d
        {
            assert_eq!(line_a, 5);
            assert_eq!(line_b, 6);
            assert!((value_degrees - 90.0).abs() < 1e-10);
        } else {
            panic!("Expected Angle");
        }
    }

    #[test]
    fn constraint_radius_diameter_serde() {
        let r = SketchConstraint::Radius {
            entity: 1,
            value: 5.0,
        };
        let d = SketchConstraint::Diameter {
            entity: 1,
            value: 10.0,
        };
        let jr = serde_json::to_string(&r).unwrap();
        let jd = serde_json::to_string(&d).unwrap();
        let dr: SketchConstraint = serde_json::from_str(&jr).unwrap();
        let dd: SketchConstraint = serde_json::from_str(&jd).unwrap();
        assert!(
            matches!(dr, SketchConstraint::Radius { value, .. } if (value - 5.0).abs() < 1e-10)
        );
        assert!(
            matches!(dd, SketchConstraint::Diameter { value, .. } if (value - 10.0).abs() < 1e-10)
        );
    }

    #[test]
    fn constraint_symmetric_variants_serde() {
        let sym = SketchConstraint::Symmetric {
            entity_a: 1,
            entity_b: 2,
            symmetry_line: 3,
        };
        let sym_h = SketchConstraint::SymmetricH {
            point_a: 1,
            point_b: 2,
        };
        let sym_v = SketchConstraint::SymmetricV {
            point_a: 3,
            point_b: 4,
        };
        for c in [sym, sym_h, sym_v] {
            let json = serde_json::to_string(&c).unwrap();
            let _d: SketchConstraint = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn constraint_remaining_variants_serde() {
        let variants: Vec<SketchConstraint> = vec![
            SketchConstraint::Horizontal { entity: 1 },
            SketchConstraint::Vertical { entity: 2 },
            SketchConstraint::Parallel {
                line_a: 1,
                line_b: 2,
            },
            SketchConstraint::Perpendicular {
                line_a: 1,
                line_b: 2,
            },
            SketchConstraint::Tangent { line: 1, curve: 2 },
            SketchConstraint::Equal {
                entity_a: 1,
                entity_b: 2,
            },
            SketchConstraint::Midpoint { point: 1, line: 2 },
            SketchConstraint::OnEntity {
                point: 1,
                entity: 2,
            },
            SketchConstraint::Dragged { point: 1 },
            SketchConstraint::EqualAngle {
                line_a: 1,
                line_b: 2,
                line_c: 3,
                line_d: 4,
            },
            SketchConstraint::Ratio {
                entity_a: 1,
                entity_b: 2,
                value: 2.0,
            },
            SketchConstraint::EqualPointToLine {
                point_a: 1,
                point_b: 2,
                line: 3,
            },
            SketchConstraint::SameOrientation {
                entity_a: 1,
                entity_b: 2,
            },
        ];
        for c in variants {
            let json = serde_json::to_string(&c).unwrap();
            let _d: SketchConstraint = serde_json::from_str(&json).unwrap();
        }
    }

    // ── u32_key_map serde ─────────────────────────────────────────────

    #[test]
    fn u32_key_map_roundtrip() {
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (3.14, -2.7));
        positions.insert(100, (999.0, 0.5));

        let sketch = Sketch {
            id: Uuid::nil(),
            plane: GeomRef {
                kind: crate::topo::TopoKind::Face,
                anchor: crate::geom_ref::Anchor::Datum {
                    datum_id: Uuid::nil(),
                },
                selector: crate::geom_ref::Selector::Role {
                    role: crate::Role::EndCapPositive,
                    index: 0,
                },
                policy: crate::geom_ref::ResolvePolicy::Strict,
            },
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities: vec![],
            constraints: vec![],
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: positions.clone(),
            solved_profiles: vec![],
            projected: vec![],
        };

        // solved_positions use skip_serializing_if = "HashMap::is_empty":
        // when populated, they ARE serialized (needed for WASM→JS bridge)
        let json = serde_json::to_string(&sketch).unwrap();
        assert!(json.contains("solved_positions"));
        let d: Sketch = serde_json::from_str(&json).unwrap();
        assert_eq!(d.solved_positions.len(), 3);
        assert_eq!(d.solved_positions[&1], (0.0, 0.0));

        // When empty, solved_positions are skipped (keeps .waffle files small)
        let mut empty_sketch = sketch.clone();
        empty_sketch.solved_positions = HashMap::new();
        let json2 = serde_json::to_string(&empty_sketch).unwrap();
        assert!(!json2.contains("solved_positions"));
        let d2: Sketch = serde_json::from_str(&json2).unwrap();
        assert_eq!(d2.solved_positions.len(), 0);
    }

    #[test]
    fn gear_entity_serde_roundtrip() {
        use crate::gear::GearParams;
        let entity = SketchEntity::Gear {
            id: 1,
            params: GearParams {
                tooth_count: 20,
                module: 0.002,
                ..Default::default()
            },
            construction: false,
        };
        let json = serde_json::to_string(&entity).unwrap();
        assert!(json.contains(r#""type":"Gear""#));
        let d: SketchEntity = serde_json::from_str(&json).unwrap();
        assert_eq!(d.id(), 1);
        assert!(!d.is_construction());
    }

    #[test]
    fn expand_gears_produces_entities() {
        use crate::gear::GearParams;
        let mut sketch = Sketch {
            id: Uuid::nil(),
            plane: GeomRef {
                kind: crate::topo::TopoKind::Face,
                anchor: crate::geom_ref::Anchor::Datum {
                    datum_id: Uuid::nil(),
                },
                selector: crate::geom_ref::Selector::Role {
                    role: crate::Role::EndCapPositive,
                    index: 0,
                },
                policy: crate::geom_ref::ResolvePolicy::Strict,
            },
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities: vec![SketchEntity::Gear {
                id: 1,
                params: GearParams {
                    tooth_count: 8,
                    module: 0.01,
                    ..Default::default()
                },
                construction: false,
            }],
            constraints: vec![],
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: HashMap::new(),
            solved_profiles: vec![],
            projected: vec![],
        };

        sketch.expand_gears();

        // After expansion: no Gear entities remain
        assert!(!sketch
            .entities
            .iter()
            .any(|e| matches!(e, SketchEntity::Gear { .. })));
        // Should have many primitives (8-tooth gear: ~8*3 + overhead entities)
        assert!(sketch.entities.len() > 50);
        // Should have solved_positions for all points
        assert!(!sketch.solved_positions.is_empty());
        // Should have at least one profile
        assert!(!sketch.solved_profiles.is_empty());
    }

    // ── default_origin / default_normal ───────────────────────────────

    #[test]
    fn sketch_defaults_applied_on_deserialize() {
        // Serialize a full sketch, then strip plane_origin/plane_normal and re-deserialize
        // to exercise the default_origin() and default_normal() functions.
        let sketch = Sketch {
            id: Uuid::nil(),
            plane: GeomRef {
                kind: crate::topo::TopoKind::Face,
                anchor: crate::geom_ref::Anchor::Datum {
                    datum_id: Uuid::nil(),
                },
                selector: crate::geom_ref::Selector::Role {
                    role: crate::Role::EndCapPositive,
                    index: 0,
                },
                policy: crate::geom_ref::ResolvePolicy::Strict,
            },
            plane_origin: [99.0, 99.0, 99.0],
            plane_normal: [99.0, 99.0, 99.0],
            entities: vec![],
            constraints: vec![],
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: HashMap::new(),
            solved_profiles: vec![],
            projected: vec![],
        };
        let mut val: serde_json::Value = serde_json::to_value(&sketch).unwrap();
        // Remove the fields so defaults kick in on deserialize
        val.as_object_mut().unwrap().remove("plane_origin");
        val.as_object_mut().unwrap().remove("plane_normal");
        let json = serde_json::to_string(&val).unwrap();
        let s: Sketch = serde_json::from_str(&json).unwrap();
        assert_eq!(s.plane_origin, [0.0, 0.0, 0.0]);
        assert_eq!(s.plane_normal, [0.0, 0.0, 1.0]);
    }

    // ── ClosedProfile serde ───────────────────────────────────────────

    #[test]
    fn closed_profile_serde() {
        let p = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let json = serde_json::to_string(&p).unwrap();
        let d: ClosedProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(d.entity_ids, vec![1, 2, 3, 4]);
        assert!(d.is_outer);
    }

    // ── SolvedSketch serde ────────────────────────────────────────────

    #[test]
    fn solved_sketch_serde() {
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (10.0, 0.0));

        let ss = SolvedSketch {
            positions,
            radii: HashMap::new(),
            profiles: vec![ClosedProfile {
                entity_ids: vec![1, 2],
                is_outer: false,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            }],
            status: SolveStatus::UnderConstrained { dof: 1 },
        };
        let json = serde_json::to_string(&ss).unwrap();
        let d: SolvedSketch = serde_json::from_str(&json).unwrap();
        assert_eq!(d.positions.len(), 2);
        assert_eq!(d.profiles.len(), 1);
        assert!(!d.profiles[0].is_outer);
        assert!(matches!(d.status, SolveStatus::UnderConstrained { dof: 1 }));
    }

    // ── recompute_derived_data tests ─────────────────────────────────

    /// Helper to build a Sketch with the standard plane boilerplate.
    fn make_sketch(entities: Vec<SketchEntity>) -> Sketch {
        Sketch {
            id: Uuid::nil(),
            plane: GeomRef {
                kind: crate::topo::TopoKind::Face,
                anchor: crate::geom_ref::Anchor::Datum {
                    datum_id: Uuid::nil(),
                },
                selector: crate::geom_ref::Selector::Role {
                    role: crate::Role::EndCapPositive,
                    index: 0,
                },
                policy: crate::geom_ref::ResolvePolicy::Strict,
            },
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities,
            constraints: vec![],
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: HashMap::new(),
            solved_profiles: vec![],
            projected: vec![],
        }
    }

    /// Projected-geometry bindings round-trip through JSON serialization, and an
    /// ordinary sketch omits the `projected` field entirely (serde skip-if-empty).
    #[test]
    fn projected_bindings_round_trip() {
        let mut sketch = make_sketch(vec![SketchEntity::Point {
            id: 7,
            x: 1.0,
            y: 2.0,
            construction: false,
        }]);
        sketch.projected = vec![
            ProjectedEntity {
                point_id: 7,
                source: ProjectedSource {
                    geom_ref: GeomRef {
                        kind: crate::topo::TopoKind::Vertex,
                        anchor: crate::geom_ref::Anchor::FeatureOutput {
                            feature_id: Uuid::nil(),
                            output_key: crate::geom_ref::OutputKey::Main,
                        },
                        selector: crate::geom_ref::Selector::Position {
                            x: 1.0,
                            y: 2.0,
                            z: 3.0,
                        },
                        policy: crate::geom_ref::ResolvePolicy::BestEffort,
                    },
                    kind: ProjectedKind::Vertex,
                },
            },
            ProjectedEntity {
                point_id: 9,
                source: ProjectedSource {
                    geom_ref: GeomRef {
                        kind: crate::topo::TopoKind::Edge,
                        anchor: crate::geom_ref::Anchor::FeatureOutput {
                            feature_id: Uuid::nil(),
                            output_key: crate::geom_ref::OutputKey::Main,
                        },
                        selector: crate::geom_ref::Selector::Position {
                            x: 0.0,
                            y: 0.0,
                            z: 0.0,
                        },
                        policy: crate::geom_ref::ResolvePolicy::BestEffort,
                    },
                    kind: ProjectedKind::EdgeSample { t: 0.5 },
                },
            },
        ];

        let json = serde_json::to_string(&sketch).unwrap();
        assert!(json.contains("\"projected\""), "projected must serialize");
        let back: Sketch = serde_json::from_str(&json).unwrap();
        assert_eq!(back.projected.len(), 2);
        assert_eq!(back.projected[0].point_id, 7);
        assert!(matches!(
            back.projected[0].source.kind,
            ProjectedKind::Vertex
        ));
        match back.projected[1].source.kind {
            ProjectedKind::EdgeSample { t } => assert!((t - 0.5).abs() < 1e-12),
            _ => panic!("expected EdgeSample"),
        }

        // An ordinary sketch (no bindings) omits the field thanks to skip-if-empty.
        let plain = make_sketch(vec![]);
        let plain_json = serde_json::to_string(&plain).unwrap();
        assert!(!plain_json.contains("\"projected\""));
        let plain_back: Sketch = serde_json::from_str(&plain_json).unwrap();
        assert!(plain_back.projected.is_empty());
    }

    /// B1: Rectangle sketch (4 Points + 4 Lines) produces 1 profile with 4 entity_ids.
    #[test]
    fn recompute_rectangle_produces_one_profile() {
        let mut sketch = make_sketch(vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 10.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 10.0,
                y: 5.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 5.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            SketchEntity::Line {
                id: 13,
                start_id: 4,
                end_id: 1,
                construction: false,
            },
        ]);

        sketch.recompute_derived_data();

        // Positions populated from Point entities
        assert_eq!(sketch.solved_positions.len(), 4);
        assert_eq!(sketch.solved_positions[&1], (0.0, 0.0));
        assert_eq!(sketch.solved_positions[&2], (10.0, 0.0));
        assert_eq!(sketch.solved_positions[&3], (10.0, 5.0));
        assert_eq!(sketch.solved_positions[&4], (0.0, 5.0));

        // One closed profile with 4 line entity IDs
        assert_eq!(sketch.solved_profiles.len(), 1);
        assert_eq!(sketch.solved_profiles[0].entity_ids.len(), 4);
    }

    /// B2: Circle sketch produces 1 profile with 1 entity_id.
    #[test]
    fn recompute_circle_produces_one_profile() {
        let mut sketch = make_sketch(vec![
            SketchEntity::Point {
                id: 1,
                x: 5.0,
                y: 5.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: 10,
                center_id: 1,
                radius: 3.0,
                construction: false,
            },
        ]);

        sketch.recompute_derived_data();

        assert_eq!(sketch.solved_positions.len(), 1);
        assert_eq!(sketch.solved_positions[&1], (5.0, 5.0));
        assert_eq!(sketch.solved_profiles.len(), 1);
        assert_eq!(sketch.solved_profiles[0].entity_ids.len(), 1);
        assert_eq!(sketch.solved_profiles[0].entity_ids[0], 10);
    }

    /// B3: Only construction entities produces 0 profiles.
    #[test]
    fn recompute_construction_only_produces_no_profiles() {
        let mut sketch = make_sketch(vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: true,
            },
            SketchEntity::Point {
                id: 2,
                x: 10.0,
                y: 0.0,
                construction: true,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: true,
            },
        ]);

        sketch.recompute_derived_data();

        assert_eq!(sketch.solved_profiles.len(), 0);
    }

    /// B4: Empty sketch produces 0 profiles and does not crash.
    #[test]
    fn recompute_empty_sketch_no_crash() {
        let mut sketch = make_sketch(vec![]);

        sketch.recompute_derived_data();

        assert_eq!(sketch.solved_positions.len(), 0);
        assert_eq!(sketch.solved_profiles.len(), 0);
    }

    /// B6: Arc entities in profile are extracted.
    #[test]
    fn recompute_arc_profile_extracted() {
        // Triangle-ish shape: two lines and one arc connecting endpoints
        let mut sketch = make_sketch(vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 10.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 5.0,
                y: 5.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 5.0,
                y: 2.0,
                construction: false,
            }, // arc center
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Arc {
                id: 12,
                center_id: 4,
                start_id: 3,
                end_id: 1,
                construction: false,
            },
        ]);

        sketch.recompute_derived_data();

        assert_eq!(sketch.solved_positions.len(), 4);
        assert!(
            !sketch.solved_profiles.is_empty(),
            "arc profile should be extracted"
        );
        // The profile should reference the arc entity
        let all_entity_ids: Vec<u32> = sketch
            .solved_profiles
            .iter()
            .flat_map(|p| p.entity_ids.iter().copied())
            .collect();
        assert!(
            all_entity_ids.contains(&12),
            "profile must include the arc entity"
        );
    }

    /// B7: Multiple closed loops produce multiple profiles.
    #[test]
    fn recompute_multiple_loops_multiple_profiles() {
        let mut sketch = make_sketch(vec![
            // First triangle: points 1-3, lines 10-12
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 5.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 2.5,
                y: 4.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 3,
                end_id: 1,
                construction: false,
            },
            // Second triangle: points 4-6, lines 13-15 (disjoint from first)
            SketchEntity::Point {
                id: 4,
                x: 20.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 5,
                x: 25.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 6,
                x: 22.5,
                y: 4.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 13,
                start_id: 4,
                end_id: 5,
                construction: false,
            },
            SketchEntity::Line {
                id: 14,
                start_id: 5,
                end_id: 6,
                construction: false,
            },
            SketchEntity::Line {
                id: 15,
                start_id: 6,
                end_id: 4,
                construction: false,
            },
        ]);

        sketch.recompute_derived_data();

        assert_eq!(sketch.solved_positions.len(), 6);
        assert!(
            sketch.solved_profiles.len() >= 2,
            "expected at least 2 profiles, got {}",
            sketch.solved_profiles.len()
        );
    }

    /// B8: Sketch that already has solved_profiles preserves them.
    #[test]
    fn recompute_preserves_existing_profiles() {
        let existing_profile = ClosedProfile {
            entity_ids: vec![99, 100, 101],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let mut sketch = make_sketch(vec![SketchEntity::Point {
            id: 1,
            x: 0.0,
            y: 0.0,
            construction: false,
        }]);
        sketch.solved_profiles = vec![existing_profile.clone()];

        sketch.recompute_derived_data();

        // Existing profiles must be preserved, not overwritten
        assert_eq!(sketch.solved_profiles.len(), 1);
        assert_eq!(sketch.solved_profiles[0].entity_ids, vec![99, 100, 101]);
    }

    // ── Adversarial / edge-case recompute tests ─────────────────────

    /// Degenerate triangle: three collinear points produce 0 profiles (zero area).
    #[test]
    fn recompute_degenerate_zero_area_triangle() {
        let mut sketch = make_sketch(vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 5.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 10.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 3,
                end_id: 1,
                construction: false,
            },
        ]);

        sketch.recompute_derived_data();

        assert_eq!(sketch.solved_positions.len(), 3);
        // All profiles (if any) should have near-zero area
        for profile in &sketch.solved_profiles {
            let area: f64 = {
                let verts = &profile.vertex_ids;
                if verts.len() < 3 {
                    0.0
                } else {
                    let n = verts.len();
                    let mut a = 0.0;
                    for i in 0..n {
                        let j = (i + 1) % n;
                        let (x1, y1) = sketch.solved_positions[&verts[i]];
                        let (x2, y2) = sketch.solved_positions[&verts[j]];
                        a += x1 * y2 - x2 * y1;
                    }
                    (a / 2.0).abs()
                }
            };
            assert!(
                area < 1e-10,
                "degenerate triangle area should be ~0, got {area}"
            );
        }
    }

    /// Circle entity whose center Point is absent from positions. Should not panic.
    #[test]
    fn recompute_circle_no_center_position() {
        let mut sketch = make_sketch(vec![
            // No Point entity for center_id 99 — it's missing
            SketchEntity::Circle {
                id: 10,
                center_id: 99,
                radius: 3.0,
                construction: false,
            },
        ]);

        sketch.recompute_derived_data();

        // Should still produce a profile for the circle
        assert_eq!(sketch.solved_profiles.len(), 1);
        assert_eq!(sketch.solved_profiles[0].entity_ids, vec![10]);
        // CircleProfile data should be None since center position is missing
        assert!(sketch.solved_profiles[0].circle.is_none());
    }

    /// Points with NaN coordinates. Should not panic; may or may not produce profiles.
    #[test]
    fn recompute_nan_coordinates() {
        let mut sketch = make_sketch(vec![
            SketchEntity::Point {
                id: 1,
                x: f64::NAN,
                y: f64::NAN,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 10.0,
                y: f64::NAN,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: f64::NAN,
                y: 5.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 3,
                end_id: 1,
                construction: false,
            },
        ]);

        // Must not panic
        sketch.recompute_derived_data();

        // Positions should still be populated (even if NaN)
        assert_eq!(sketch.solved_positions.len(), 3);
    }

    /// Rectangle at near-MIN_FEATURE_SIZE scale should still extract 1 profile.
    #[test]
    fn recompute_very_small_features() {
        let s = 1e-6; // MIN_FEATURE_SIZE
        let mut sketch = make_sketch(vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: s,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: s,
                y: s,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: s,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            SketchEntity::Line {
                id: 13,
                start_id: 4,
                end_id: 1,
                construction: false,
            },
        ]);

        sketch.recompute_derived_data();

        assert_eq!(sketch.solved_positions.len(), 4);
        assert_eq!(
            sketch.solved_profiles.len(),
            1,
            "tiny rectangle should still produce 1 profile"
        );
        assert_eq!(sketch.solved_profiles[0].entity_ids.len(), 4);
    }

    /// Calling recompute_derived_data() twice is idempotent.
    #[test]
    fn recompute_idempotent() {
        let mut sketch = make_sketch(vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 10.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 10.0,
                y: 5.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 5.0,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            SketchEntity::Line {
                id: 13,
                start_id: 4,
                end_id: 1,
                construction: false,
            },
        ]);

        sketch.recompute_derived_data();
        let profiles_after_first = sketch.solved_profiles.clone();
        let positions_after_first = sketch.solved_positions.clone();

        sketch.recompute_derived_data();

        assert_eq!(sketch.solved_positions, positions_after_first);
        assert_eq!(sketch.solved_profiles.len(), profiles_after_first.len());
        for (a, b) in sketch
            .solved_profiles
            .iter()
            .zip(profiles_after_first.iter())
        {
            assert_eq!(a.entity_ids, b.entity_ids);
            assert_eq!(a.is_outer, b.is_outer);
            assert_eq!(a.vertex_ids, b.vertex_ids);
            assert_eq!(a.circle, b.circle);
        }
    }

    /// Circle with a valid center position populates CircleProfile data.
    #[test]
    fn recompute_circle_with_center_populates_circle_data() {
        let mut sketch = make_sketch(vec![
            SketchEntity::Point {
                id: 1,
                x: 5.0,
                y: 7.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: 10,
                center_id: 1,
                radius: 3.5,
                construction: false,
            },
        ]);

        sketch.recompute_derived_data();

        assert_eq!(sketch.solved_profiles.len(), 1);
        let circle = sketch.solved_profiles[0]
            .circle
            .as_ref()
            .expect("CircleProfile must be Some when center position exists");
        assert!((circle.center_u - 5.0).abs() < 1e-12);
        assert!((circle.center_v - 7.0).abs() < 1e-12);
        assert!((circle.radius - 3.5).abs() < 1e-12);
    }
}
