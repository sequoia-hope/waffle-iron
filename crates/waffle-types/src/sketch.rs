use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

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
    #[serde(default, with = "u32_key_map")]
    pub solved_positions: HashMap<u32, (f64, f64)>,
    /// Closed profiles extracted from the solved geometry.
    #[serde(default)]
    pub solved_profiles: Vec<ClosedProfile>,
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
    Spline {
        id: u32,
        point_ids: Vec<u32>,
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
            | SketchEntity::Spline { id, .. } => *id,
        }
    }

    pub fn is_construction(&self) -> bool {
        match self {
            SketchEntity::Point { construction, .. }
            | SketchEntity::Line { construction, .. }
            | SketchEntity::Circle { construction, .. }
            | SketchEntity::Arc { construction, .. }
            | SketchEntity::Spline { construction, .. } => *construction,
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
        };

        let json = serde_json::to_string(&sketch).unwrap();
        let d: Sketch = serde_json::from_str(&json).unwrap();

        assert_eq!(d.solved_positions.len(), 3);
        assert_eq!(d.solved_positions[&1], (0.0, 0.0));
        assert_eq!(d.solved_positions[&2], (3.14, -2.7));
        assert_eq!(d.solved_positions[&100], (999.0, 0.5));
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
            profiles: vec![ClosedProfile {
                entity_ids: vec![1, 2],
                is_outer: false,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
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
}
