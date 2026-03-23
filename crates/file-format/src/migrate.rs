use feature_engine::types::{
    ExtrudeParams, Feature, FeatureTree, Operation, PlaneDefinition, SecondDirection,
};
use waffle_types::{ClosedProfile, Sketch, SketchConstraint, SketchEntity};

use crate::errors::LoadError;

/// Scale factor for v1→v2 migration: mm-scale → meters.
const MM_TO_METERS: f64 = 0.001;

/// Apply format migrations from `from_version` to `to_version`.
///
/// Migrations are applied sequentially: v1→v2, v2→v3, etc.
pub fn migrate(
    mut tree: FeatureTree,
    from_version: u32,
    to_version: u32,
) -> Result<FeatureTree, LoadError> {
    let mut current = from_version;
    while current < to_version {
        tree = match current {
            1 => migrate_v1_to_v2(tree),
            2 => tree, // v2→v3: feature tree format unchanged (structural change only)
            _ => {
                return Err(LoadError::MigrationFailed {
                    from: current,
                    to: to_version,
                    reason: format!("no migration path from v{} to v{}", current, to_version),
                });
            }
        };
        current += 1;
    }
    Ok(tree)
}

/// Migrate v1 (mm-scale coordinates) → v2 (meters).
///
/// Scales all length-valued f64 fields by 0.001.
/// Angles and direction vectors are left unchanged.
fn migrate_v1_to_v2(mut tree: FeatureTree) -> FeatureTree {
    for feature in &mut tree.features {
        migrate_feature_v1_to_v2(feature);
    }
    tree
}

fn migrate_feature_v1_to_v2(feature: &mut Feature) {
    match &mut feature.operation {
        Operation::Sketch { sketch } => migrate_sketch(sketch),
        Operation::Extrude { params } => migrate_extrude(params),
        Operation::Revolve { params } => {
            // Scale axis_origin (position), NOT axis_direction (unit vector) or angle
            for v in params.axis_origin.iter_mut() {
                *v *= MM_TO_METERS;
            }
        }
        Operation::Fillet { params } => {
            params.radius *= MM_TO_METERS;
        }
        Operation::Chamfer { params } => {
            params.distance *= MM_TO_METERS;
        }
        Operation::Shell { params } => {
            params.thickness *= MM_TO_METERS;
        }
        Operation::DatumPlane { params } => match &mut params.definition {
            PlaneDefinition::PointNormal { origin, normal: _ } => {
                for v in origin.iter_mut() {
                    *v *= MM_TO_METERS;
                }
            }
            PlaneDefinition::Offset { distance, .. } => {
                *distance *= MM_TO_METERS;
            }
        },
        Operation::BooleanCombine { .. } => {
            // No length fields
        }
    }
}

fn migrate_sketch(sketch: &mut Sketch) {
    // Scale plane_origin (NOT plane_normal — it's a direction)
    for v in sketch.plane_origin.iter_mut() {
        *v *= MM_TO_METERS;
    }

    // Scale entity coordinates
    for entity in &mut sketch.entities {
        match entity {
            SketchEntity::Point { x, y, .. } => {
                *x *= MM_TO_METERS;
                *y *= MM_TO_METERS;
            }
            SketchEntity::Circle { radius, .. } => {
                *radius *= MM_TO_METERS;
            }
            SketchEntity::Line { .. }
            | SketchEntity::Arc { .. }
            | SketchEntity::Spline { .. }
            | SketchEntity::Gear { .. } => {
                // No direct length fields (positions come from solved_positions / expansion)
            }
        }
    }

    // Scale constraint values (only length-valued ones, NOT angles)
    for constraint in &mut sketch.constraints {
        match constraint {
            SketchConstraint::Distance { value, .. }
            | SketchConstraint::Radius { value, .. }
            | SketchConstraint::Diameter { value, .. } => {
                *value *= MM_TO_METERS;
            }
            // Angle, Ratio, and all other constraints have no length fields
            _ => {}
        }
    }

    // Scale solved_positions
    for pos in sketch.solved_positions.values_mut() {
        pos.0 *= MM_TO_METERS;
        pos.1 *= MM_TO_METERS;
    }

    // Scale solved_profiles
    for profile in &mut sketch.solved_profiles {
        migrate_profile(profile);
    }
}

fn migrate_profile(profile: &mut ClosedProfile) {
    if let Some(circle) = &mut profile.circle {
        circle.center_u *= MM_TO_METERS;
        circle.center_v *= MM_TO_METERS;
        circle.radius *= MM_TO_METERS;
    }
    for seg in &mut profile.spline_segments {
        for cp in &mut seg.control_points {
            cp.0 *= MM_TO_METERS;
            cp.1 *= MM_TO_METERS;
        }
    }
}

fn migrate_extrude(params: &mut ExtrudeParams) {
    params.depth *= MM_TO_METERS;
    if let Some(SecondDirection::Blind { depth }) = &mut params.second_direction {
        *depth *= MM_TO_METERS;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use feature_engine::types::*;
    use std::collections::HashMap;
    use uuid::Uuid;
    use waffle_types::*;

    fn make_datum_geomref() -> GeomRef {
        GeomRef {
            kind: TopoKind::Face,
            anchor: Anchor::Datum {
                datum_id: Uuid::new_v4(),
            },
            selector: Selector::Role {
                role: Role::ProfileFace,
                index: 0,
            },
            policy: ResolvePolicy::BestEffort,
        }
    }

    fn make_test_sketch() -> Sketch {
        let sketch_id = Uuid::new_v4();
        let mut positions = HashMap::new();
        positions.insert(1, (5.0, 10.0));
        positions.insert(2, (15.0, 10.0));
        positions.insert(3, (15.0, 20.0));
        positions.insert(4, (5.0, 20.0));

        Sketch {
            id: sketch_id,
            plane: make_datum_geomref(),
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities: vec![
                SketchEntity::Point {
                    id: 1,
                    x: 5.0,
                    y: 10.0,
                    construction: false,
                },
                SketchEntity::Point {
                    id: 2,
                    x: 15.0,
                    y: 10.0,
                    construction: false,
                },
                SketchEntity::Circle {
                    id: 10,
                    center_id: 1,
                    radius: 5.0,
                    construction: false,
                },
            ],
            constraints: vec![
                SketchConstraint::Distance {
                    entity_a: 1,
                    entity_b: 2,
                    value: 10.0,
                },
                SketchConstraint::Radius {
                    entity: 10,
                    value: 5.0,
                },
                SketchConstraint::Angle {
                    line_a: 1,
                    line_b: 2,
                    value_degrees: 45.0,
                },
            ],
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: positions,
            solved_profiles: vec![ClosedProfile {
                entity_ids: vec![1, 2, 3, 4],
                is_outer: true,
                vertex_ids: vec![],
                circle: Some(CircleProfile {
                    center_u: 10.0,
                    center_v: 15.0,
                    radius: 5.0,
                }),
                spline_segments: vec![SplineSegment {
                    start_point_index: 0,
                    end_point_index: 1,
                    control_points: vec![(5.0, 10.0), (10.0, 15.0), (15.0, 10.0)],
                }],
                arc_segments: vec![],
            }],
        }
    }

    #[test]
    fn test_v1_to_v2_sketch_migration() {
        let sketch = make_test_sketch();
        let tree = FeatureTree {
            features: vec![Feature {
                id: Uuid::new_v4(),
                name: "Sketch1".to_string(),
                operation: Operation::Sketch { sketch },
                suppressed: false,
                references: vec![],
            }],
            active_index: None,
        };

        let migrated = migrate(tree, 1, 2).unwrap();
        let feature = &migrated.features[0];
        if let Operation::Sketch { sketch } = &feature.operation {
            // Point coordinates scaled
            if let SketchEntity::Point { x, y, .. } = &sketch.entities[0] {
                assert!(
                    (x - 0.005).abs() < 1e-10,
                    "Point x: expected 0.005, got {x}"
                );
                assert!(
                    (y - 0.010).abs() < 1e-10,
                    "Point y: expected 0.010, got {y}"
                );
            }

            // Circle radius scaled
            if let SketchEntity::Circle { radius, .. } = &sketch.entities[2] {
                assert!(
                    (radius - 0.005).abs() < 1e-10,
                    "Circle radius: expected 0.005, got {radius}"
                );
            }

            // Distance constraint scaled
            if let SketchConstraint::Distance { value, .. } = &sketch.constraints[0] {
                assert!(
                    (value - 0.010).abs() < 1e-10,
                    "Distance: expected 0.010, got {value}"
                );
            }

            // Radius constraint scaled
            if let SketchConstraint::Radius { value, .. } = &sketch.constraints[1] {
                assert!(
                    (value - 0.005).abs() < 1e-10,
                    "Radius: expected 0.005, got {value}"
                );
            }

            // Angle constraint NOT scaled
            if let SketchConstraint::Angle { value_degrees, .. } = &sketch.constraints[2] {
                assert!(
                    (value_degrees - 45.0).abs() < 1e-10,
                    "Angle should stay 45.0, got {value_degrees}"
                );
            }

            // Solved positions scaled
            let pos = sketch.solved_positions.get(&1).unwrap();
            assert!(
                (pos.0 - 0.005).abs() < 1e-10,
                "solved pos x: expected 0.005, got {}",
                pos.0
            );
            assert!(
                (pos.1 - 0.010).abs() < 1e-10,
                "solved pos y: expected 0.010, got {}",
                pos.1
            );

            // Circle profile scaled
            let cp = sketch.solved_profiles[0].circle.as_ref().unwrap();
            assert!(
                (cp.center_u - 0.010).abs() < 1e-10,
                "circle profile center_u"
            );
            assert!(
                (cp.center_v - 0.015).abs() < 1e-10,
                "circle profile center_v"
            );
            assert!((cp.radius - 0.005).abs() < 1e-10, "circle profile radius");

            // Spline control points scaled
            let seg = &sketch.solved_profiles[0].spline_segments[0];
            assert!((seg.control_points[0].0 - 0.005).abs() < 1e-10);
            assert!((seg.control_points[1].1 - 0.015).abs() < 1e-10);
        } else {
            panic!("Expected Sketch operation");
        }
    }

    #[test]
    fn test_v1_to_v2_extrude_migration() {
        let tree = FeatureTree {
            features: vec![Feature {
                id: Uuid::new_v4(),
                name: "Extrude1".to_string(),
                operation: Operation::Extrude {
                    params: ExtrudeParams {
                        sketch_id: Uuid::new_v4(),
                        profile_index: 0,
                        depth: 10.0,
                        direction: None,
                        symmetric: false,
                        cut: false,
                        merge: true,
                        target_body: None,
                        depth_mode: DepthMode::Blind,
                        second_direction: Some(SecondDirection::Blind { depth: 5.0 }),
                    },
                },
                suppressed: false,
                references: vec![],
            }],
            active_index: None,
        };

        let migrated = migrate(tree, 1, 2).unwrap();
        if let Operation::Extrude { params } = &migrated.features[0].operation {
            assert!(
                (params.depth - 0.010).abs() < 1e-10,
                "depth: expected 0.010, got {}",
                params.depth
            );
            if let Some(SecondDirection::Blind { depth }) = &params.second_direction {
                assert!(
                    (depth - 0.005).abs() < 1e-10,
                    "second depth: expected 0.005, got {depth}"
                );
            } else {
                panic!("Expected SecondDirection::Blind");
            }
        }
    }

    #[test]
    fn test_v1_to_v2_revolve_migration() {
        let tree = FeatureTree {
            features: vec![Feature {
                id: Uuid::new_v4(),
                name: "Revolve1".to_string(),
                operation: Operation::Revolve {
                    params: feature_engine::types::RevolveParams {
                        sketch_id: Uuid::new_v4(),
                        profile_index: 0,
                        axis_origin: [10.0, 20.0, 30.0],
                        axis_direction: [0.0, 1.0, 0.0],
                        angle: 360.0,
                        cut: false,
                        merge: false,
                    },
                },
                suppressed: false,
                references: vec![],
            }],
            active_index: None,
        };

        let migrated = migrate(tree, 1, 2).unwrap();
        if let Operation::Revolve { params } = &migrated.features[0].operation {
            assert!(
                (params.axis_origin[0] - 0.010).abs() < 1e-10,
                "axis_origin[0]"
            );
            assert!(
                (params.axis_origin[1] - 0.020).abs() < 1e-10,
                "axis_origin[1]"
            );
            // Direction should NOT be scaled
            assert!(
                (params.axis_direction[1] - 1.0).abs() < 1e-10,
                "axis_direction should stay 1.0"
            );
            // Angle should NOT be scaled
            assert!(
                (params.angle - 360.0).abs() < 1e-10,
                "angle should stay 360.0"
            );
        }
    }

    #[test]
    fn test_v1_to_v2_datum_plane_offset() {
        let tree = FeatureTree {
            features: vec![Feature {
                id: Uuid::new_v4(),
                name: "DatumPlane1".to_string(),
                operation: Operation::DatumPlane {
                    params: DatumPlaneParams {
                        name: "Offset Plane".to_string(),
                        definition: PlaneDefinition::Offset {
                            base_plane_id: Uuid::new_v4(),
                            distance: 25.0,
                        },
                    },
                },
                suppressed: false,
                references: vec![],
            }],
            active_index: None,
        };

        let migrated = migrate(tree, 1, 2).unwrap();
        if let Operation::DatumPlane { params } = &migrated.features[0].operation {
            if let PlaneDefinition::Offset { distance, .. } = &params.definition {
                assert!(
                    (distance - 0.025).abs() < 1e-10,
                    "offset distance: expected 0.025, got {distance}"
                );
            }
        }
    }

    #[test]
    fn test_same_version_no_migration() {
        let tree = FeatureTree {
            features: vec![],
            active_index: None,
        };
        let result = migrate(tree, 2, 2).unwrap();
        assert!(result.features.is_empty());
    }

    #[test]
    fn test_future_version_error() {
        let tree = FeatureTree {
            features: vec![],
            active_index: None,
        };
        let result = migrate(tree, 3, 4);
        assert!(result.is_err());
    }
}
