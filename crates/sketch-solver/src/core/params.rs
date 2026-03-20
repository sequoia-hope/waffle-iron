//! Parameter vector layout: maps sketch entity IDs to typed indices.
//!
//! Built once from the entity list. All index lookups return typed wrappers
//! that prevent accidental index misuse in constraint implementations.

use std::collections::HashMap;

use crate::types::{EntityId, PointId, SketchEntity};

use super::types::{LineIdx, PointIdx, RadiusDef, RadiusIdx};

pub struct ParamLayout {
    point_indices: HashMap<PointId, PointIdx>,
    radius_indices: HashMap<EntityId, RadiusIdx>,
    arc_points: HashMap<EntityId, (PointId, PointId)>, // arc_id → (center_id, start_id)
    line_points: HashMap<EntityId, (PointId, PointId)>, // line_id → (start_id, end_id)
    num_params: usize,
}

impl ParamLayout {
    /// Build parameter layout from sketch entities.
    ///
    /// - Points: 2 params each (x, y)
    /// - Circles: 1 param each (radius). Center is a Point.
    /// - Arcs: 0 own params. Center/start/end are Points; radius is implicit.
    /// - Lines: 0 own params. Start/end are Points.
    /// - Splines, Gears: skipped.
    pub fn from_entities(entities: &[SketchEntity]) -> Self {
        let mut point_indices = HashMap::new();
        let mut radius_indices = HashMap::new();
        let mut arc_points = HashMap::new();
        let mut line_points = HashMap::new();
        let mut offset = 0;

        for entity in entities {
            if let SketchEntity::Point { id, .. } = entity {
                point_indices.insert(*id, PointIdx(offset));
                offset += 2;
            }
        }

        for entity in entities {
            if let SketchEntity::Circle { id, .. } = entity {
                radius_indices.insert(EntityId::from(*id), RadiusIdx(offset));
                offset += 1;
            }
        }

        for entity in entities {
            match entity {
                SketchEntity::Arc {
                    id,
                    center_id,
                    start_id,
                    ..
                } => {
                    arc_points.insert(EntityId::from(*id), (*center_id, *start_id));
                }
                SketchEntity::Line {
                    id,
                    start_id,
                    end_id,
                    ..
                } => {
                    line_points.insert(EntityId::from(*id), (*start_id, *end_id));
                }
                _ => {}
            }
        }

        ParamLayout {
            point_indices,
            radius_indices,
            arc_points,
            line_points,
            num_params: offset,
        }
    }

    pub fn initial_params(&self, entities: &[SketchEntity]) -> Vec<f64> {
        let mut params = vec![0.0; self.num_params];
        for entity in entities {
            match entity {
                SketchEntity::Point { id, x, y, .. } => {
                    if let Some(idx) = self.point_indices.get(id) {
                        params[idx.x()] = *x;
                        params[idx.y()] = *y;
                    }
                }
                SketchEntity::Circle { id, radius, .. } => {
                    if let Some(idx) = self.radius_indices.get(&EntityId::from(*id)) {
                        params[idx.0] = *radius;
                    }
                }
                _ => {}
            }
        }
        params
    }

    pub fn point(&self, id: PointId) -> PointIdx {
        self.point_indices[&id]
    }

    pub fn radius(&self, id: EntityId) -> RadiusIdx {
        self.radius_indices[&id]
    }

    pub fn radius_def(&self, entity_id: EntityId) -> RadiusDef {
        if let Some(r) = self.radius_indices.get(&entity_id) {
            RadiusDef::Param(*r)
        } else if let Some((_center_id, start_id)) = self.arc_points.get(&entity_id) {
            RadiusDef::Implicit(self.point(*start_id))
        } else {
            panic!(
                "radius_def() called for entity {:?} which is not a circle or arc",
                entity_id
            );
        }
    }

    pub fn line(&self, line_id: EntityId) -> LineIdx {
        let (start_id, end_id) = self.line_points[&line_id];
        LineIdx {
            start: self.point(start_id),
            end: self.point(end_id),
        }
    }

    pub fn num_params(&self) -> usize {
        self.num_params
    }

    /// Get the (center_id, start_id) for an arc entity.
    pub fn arc_center_start(&self, arc_id: EntityId) -> (PointIdx, PointIdx) {
        let (center_id, start_id) = self.arc_points[&arc_id];
        (self.point(center_id), self.point(start_id))
    }

    /// Check if an entity has arc data.
    pub fn is_arc(&self, entity_id: EntityId) -> bool {
        self.arc_points.contains_key(&entity_id)
    }

    /// Check if an entity has a radius parameter (circle).
    pub fn has_radius(&self, entity_id: EntityId) -> bool {
        self.radius_indices.contains_key(&entity_id)
    }

    pub fn extract_positions(&self, params: &[f64]) -> HashMap<PointId, (f64, f64)> {
        let mut positions = HashMap::new();
        for (id, idx) in &self.point_indices {
            positions.insert(*id, (params[idx.x()], params[idx.y()]));
        }
        positions
    }

    /// Extract solved radii for all circles and arcs.
    ///
    /// - Circles: read from the optimized radius parameter.
    /// - Arcs: compute distance(center, start) from solved positions.
    pub fn extract_radii(&self, params: &[f64]) -> HashMap<EntityId, f64> {
        let mut radii = HashMap::new();
        for (id, idx) in &self.radius_indices {
            radii.insert(*id, params[idx.0]);
        }
        for (id, (center_id, start_id)) in &self.arc_points {
            let cx = params[self.point(*center_id).x()];
            let cy = params[self.point(*center_id).y()];
            let sx = params[self.point(*start_id).x()];
            let sy = params[self.point(*start_id).y()];
            let r = ((sx - cx).powi(2) + (sy - cy).powi(2)).sqrt();
            radii.insert(*id, r);
        }
        radii
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ArcId, CircleId, LineId};

    fn sample_entities() -> Vec<SketchEntity> {
        vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 1.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: 1.0,
                y: 1.0,
                construction: false,
            },
            SketchEntity::Line {
                id: LineId(10),
                start_id: PointId(1),
                end_id: PointId(2),
                construction: false,
            },
            SketchEntity::Circle {
                id: CircleId(20),
                center_id: PointId(3),
                radius: 0.5,
                construction: false,
            },
        ]
    }

    #[test]
    fn layout_point_indices() {
        let layout = ParamLayout::from_entities(&sample_entities());
        assert_eq!(layout.point(PointId(1)).x(), 0);
        assert_eq!(layout.point(PointId(1)).y(), 1);
        assert_eq!(layout.point(PointId(2)).x(), 2);
        assert_eq!(layout.point(PointId(3)).x(), 4);
        assert_eq!(layout.num_params(), 7); // 3 points * 2 + 1 radius
    }

    #[test]
    fn layout_initial_params() {
        let entities = sample_entities();
        let layout = ParamLayout::from_entities(&entities);
        let params = layout.initial_params(&entities);
        assert_eq!(params, vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.5]);
    }

    #[test]
    fn layout_line_idx() {
        let layout = ParamLayout::from_entities(&sample_entities());
        let l = layout.line(EntityId(10));
        assert_eq!(l.start, layout.point(PointId(1)));
        assert_eq!(l.end, layout.point(PointId(2)));
    }

    #[test]
    fn layout_radius_def_circle() {
        let layout = ParamLayout::from_entities(&sample_entities());
        assert!(matches!(
            layout.radius_def(EntityId(20)),
            RadiusDef::Param(_)
        ));
    }

    #[test]
    fn layout_radius_def_arc() {
        let entities = vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 1.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: 0.0,
                y: 1.0,
                construction: false,
            },
            SketchEntity::Arc {
                id: ArcId(30),
                center_id: PointId(1),
                start_id: PointId(2),
                end_id: PointId(3),
                construction: false,
            },
        ];
        let layout = ParamLayout::from_entities(&entities);
        assert!(matches!(
            layout.radius_def(EntityId(30)),
            RadiusDef::Implicit(_)
        ));
    }

    #[test]
    fn layout_extract_positions() {
        let entities = sample_entities();
        let layout = ParamLayout::from_entities(&entities);
        let params = layout.initial_params(&entities);
        let positions = layout.extract_positions(&params);
        assert_eq!(positions[&PointId(1)], (0.0, 0.0));
        assert_eq!(positions[&PointId(2)], (1.0, 0.0));
        assert_eq!(positions[&PointId(3)], (1.0, 1.0));
    }
}
