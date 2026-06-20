//! Clean-room entity mapping: maps `SketchEntity` to a flat parameter vector.
//!
//! Parameter allocation (per `specs/clean_room_constraint_solver.md`):
//!   Point  → (x, y)        : 2 params
//!   Circle → (r)           : 1 param (center is a point entity)
//!   Line   → no extra params (defined by its 2 point endpoints)
//!   Arc    → no extra params (defined by 3 points: center, start, end)
//!   Spline → not supported in PR-SS1
//!   Gear   → expanded to primitives before reaching the solver
//!
//! Determinism: the parameter vector is built by iterating `entities` in
//! declaration order (a `Vec`, not a `HashMap`). The same input always
//! produces the same `ParamLayout`. No `HashMap` iteration occurs in the
//! solve path — `HashMap` is used only for O(1) ID→index lookups.

use std::collections::HashMap;

use crate::types::{EntityKind, SketchEntity};

/// Maps sketch entities to indices in a flat parameter vector.
///
/// Built deterministically from the entity list. Owns the initial parameter
/// values extracted from entity declarations (`Point { x, y }`, `Circle { radius }`).
pub struct ParamLayout {
    /// Flat parameter vector, initialized from entity declarations.
    pub params: Vec<f64>,

    /// Point entity ID → (x_param_index, y_param_index).
    pub point_indices: HashMap<u32, (usize, usize)>,

    /// Circle/Arc entity ID → radius_param_index.
    pub radius_indices: HashMap<u32, usize>,

    /// Circle entity ID → center_point_id.
    pub circle_centers: HashMap<u32, u32>,

    /// Entity ID → EntityKind (for constraint dispatch).
    pub entity_kinds: HashMap<u32, EntityKind>,

    /// Line entity ID → (start_point_id, end_point_id).
    pub line_endpoints: HashMap<u32, (u32, u32)>,

    /// Arc entity ID → (center_point_id, start_point_id, end_point_id).
    pub arc_endpoints: HashMap<u32, (u32, u32, u32)>,
}

impl ParamLayout {
    /// Build a `ParamLayout` from a slice of sketch entities.
    ///
    /// Iterates entities in order: points first (allocating x,y params),
    /// then curves (circles allocate a radius param; lines/arcs reference
    /// existing point params). Spline and Gear entities are skipped (not
    /// in PR-SS1 scope).
    pub fn build(entities: &[SketchEntity]) -> Self {
        let mut layout = ParamLayout {
            params: Vec::new(),
            point_indices: HashMap::new(),
            radius_indices: HashMap::new(),
            circle_centers: HashMap::new(),
            entity_kinds: HashMap::new(),
            line_endpoints: HashMap::new(),
            arc_endpoints: HashMap::new(),
        };

        // Pass 1: Points — each point allocates 2 params (x, y).
        for entity in entities {
            if let SketchEntity::Point { id, x, y, .. } = entity {
                let xi = layout.params.len();
                layout.params.push(*x);
                let yi = layout.params.len();
                layout.params.push(*y);
                layout.point_indices.insert(*id, (xi, yi));
                layout.entity_kinds.insert(*id, EntityKind::Point);
            }
        }

        // Pass 2: Lines, Circles, Arcs — reference existing point params.
        // Circles allocate 1 additional param (radius); lines/arcs add none.
        for entity in entities {
            match entity {
                SketchEntity::Line {
                    id,
                    start_id,
                    end_id,
                    ..
                } => {
                    layout.line_endpoints.insert(*id, (*start_id, *end_id));
                    layout.entity_kinds.insert(*id, EntityKind::Line);
                }
                SketchEntity::Circle {
                    id,
                    center_id,
                    radius,
                    ..
                } => {
                    let ri = layout.params.len();
                    layout.params.push(*radius);
                    layout.radius_indices.insert(*id, ri);
                    layout.circle_centers.insert(*id, *center_id);
                    layout.entity_kinds.insert(*id, EntityKind::Circle);
                }
                SketchEntity::Arc {
                    id,
                    center_id,
                    start_id,
                    end_id,
                    ..
                } => {
                    layout
                        .arc_endpoints
                        .insert(*id, (*center_id, *start_id, *end_id));
                    layout.entity_kinds.insert(*id, EntityKind::Arc);
                }
                SketchEntity::Point { .. } => {} // handled in pass 1
                SketchEntity::Spline { .. } => {} // not in PR-SS1 scope
                SketchEntity::Gear { .. } => {}   // expanded before reaching solver
            }
        }

        layout
    }

    /// Number of scalar parameters in the layout.
    pub fn n_params(&self) -> usize {
        self.params.len()
    }

    /// Extract solved positions from the parameter vector.
    ///
    /// After LM minimization, call this with the optimized parameter vector
    /// to recover point positions keyed by entity ID.
    pub fn extract_positions(&self, params: &[f64]) -> HashMap<u32, (f64, f64)> {
        let mut positions = HashMap::new();
        for (&id, &(xi, yi)) in &self.point_indices {
            positions.insert(id, (params[xi], params[yi]));
        }
        positions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Group 1: Entity mapping ────────────────────────────────────────────

    fn point(id: u32, x: f64, y: f64) -> SketchEntity {
        SketchEntity::Point {
            id,
            x,
            y,
            construction: false,
        }
    }

    fn line(id: u32, start: u32, end: u32) -> SketchEntity {
        SketchEntity::Line {
            id,
            start_id: start,
            end_id: end,
            construction: false,
        }
    }

    fn circle(id: u32, center: u32, radius: f64) -> SketchEntity {
        SketchEntity::Circle {
            id,
            center_id: center,
            radius,
            construction: false,
        }
    }

    fn arc(id: u32, center: u32, start: u32, end: u32) -> SketchEntity {
        SketchEntity::Arc {
            id,
            center_id: center,
            start_id: start,
            end_id: end,
            construction: false,
        }
    }

    #[test]
    fn empty_sketch_zero_params() {
        let layout = ParamLayout::build(&[]);
        assert_eq!(layout.n_params(), 0);
        assert!(layout.point_indices.is_empty());
        assert!(layout.radius_indices.is_empty());
    }

    #[test]
    fn single_point_two_params() {
        let layout = ParamLayout::build(&[point(1, 3.0, 7.0)]);
        assert_eq!(layout.n_params(), 2);
        let (xi, yi) = layout.point_indices[&1];
        assert_eq!(layout.params[xi], 3.0);
        assert_eq!(layout.params[yi], 7.0);
    }

    #[test]
    fn line_two_points_four_params() {
        let entities = vec![point(1, 0.0, 0.0), point(2, 10.0, 5.0), line(10, 1, 2)];
        let layout = ParamLayout::build(&entities);
        assert_eq!(layout.n_params(), 4, "line adds no params beyond its points");
        assert_eq!(layout.point_indices.len(), 2);
        assert!(!layout.radius_indices.contains_key(&10));
        assert_eq!(layout.line_endpoints[&10], (1, 2));
        assert_eq!(layout.entity_kinds[&10], EntityKind::Line);
    }

    #[test]
    fn circle_center_plus_radius_three_params() {
        let entities = vec![point(1, 5.0, 5.0), circle(10, 1, 3.0)];
        let layout = ParamLayout::build(&entities);
        assert_eq!(layout.n_params(), 3, "point=2 + radius=1");
        let ri = layout.radius_indices[&10];
        assert_eq!(layout.params[ri], 3.0);
        assert_eq!(layout.entity_kinds[&10], EntityKind::Circle);
    }

    #[test]
    fn arc_three_points_six_params() {
        let entities = vec![
            point(1, 0.0, 0.0),
            point(2, 5.0, 0.0),
            point(3, 0.0, 5.0),
            arc(10, 1, 2, 3),
        ];
        let layout = ParamLayout::build(&entities);
        assert_eq!(
            layout.n_params(),
            6,
            "arc adds no params beyond its 3 points"
        );
        assert_eq!(layout.arc_endpoints[&10], (1, 2, 3));
        assert_eq!(layout.entity_kinds[&10], EntityKind::Arc);
    }

    #[test]
    fn mixed_sketch_correct_param_count() {
        // 5 points (10 params) + 2 lines (0) + 1 circle (1) + 1 arc (0) = 11
        let entities = vec![
            point(1, 0.0, 0.0),
            point(2, 10.0, 0.0),
            point(3, 10.0, 5.0),
            point(4, 0.0, 5.0),
            point(5, 5.0, 2.5),
            line(10, 1, 2),
            line(11, 2, 3),
            circle(20, 5, 2.0),
            arc(30, 5, 4, 3),
        ];
        let layout = ParamLayout::build(&entities);
        assert_eq!(layout.n_params(), 11);
        assert_eq!(layout.point_indices.len(), 5);
        assert_eq!(layout.line_endpoints.len(), 2);
        assert_eq!(layout.radius_indices.len(), 1);
        assert_eq!(layout.arc_endpoints.len(), 1);
    }

    #[test]
    fn determinism_same_input_identical_layout() {
        let entities = vec![
            point(1, 3.0, 4.0),
            point(2, 7.0, 1.0),
            line(10, 1, 2),
            circle(20, 1, 5.0),
        ];

        let layout_a = ParamLayout::build(&entities);
        let layout_b = ParamLayout::build(&entities);

        // Param vector identical
        assert_eq!(layout_a.params, layout_b.params);

        // Indices identical
        assert_eq!(layout_a.point_indices, layout_b.point_indices);
        assert_eq!(layout_a.radius_indices, layout_b.radius_indices);
        assert_eq!(layout_a.line_endpoints, layout_b.line_endpoints);
        assert_eq!(layout_a.entity_kinds, layout_b.entity_kinds);
    }

    #[test]
    fn extract_positions_round_trip() {
        let entities = vec![point(1, 3.0, 4.0), point(2, 7.0, 1.0)];
        let layout = ParamLayout::build(&entities);

        let positions = layout.extract_positions(&layout.params);
        assert_eq!(positions[&1], (3.0, 4.0));
        assert_eq!(positions[&2], (7.0, 1.0));
    }
}
