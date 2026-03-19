//! Parameter vector layout: maps sketch entity IDs to typed indices.
//!
//! Built once from the entity list. All index lookups return typed wrappers
//! that prevent accidental index misuse in constraint implementations.

use std::collections::HashMap;

use crate::types::SketchEntity;

use super::types::{LineIdx, PointIdx, RadiusDef, RadiusIdx};

pub struct ParamLayout {
    point_indices: HashMap<u32, PointIdx>,
    radius_indices: HashMap<u32, RadiusIdx>,
    arc_points: HashMap<u32, (u32, u32)>, // arc_id → (center_id, start_id)
    line_points: HashMap<u32, (u32, u32)>, // line_id → (start_id, end_id)
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
                radius_indices.insert(*id, RadiusIdx(offset));
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
                    arc_points.insert(*id, (*center_id, *start_id));
                }
                SketchEntity::Line {
                    id,
                    start_id,
                    end_id,
                    ..
                } => {
                    line_points.insert(*id, (*start_id, *end_id));
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
                    if let Some(idx) = self.radius_indices.get(id) {
                        params[idx.0] = *radius;
                    }
                }
                _ => {}
            }
        }
        params
    }

    pub fn point(&self, id: u32) -> PointIdx {
        self.point_indices[&id]
    }

    pub fn radius(&self, id: u32) -> RadiusIdx {
        self.radius_indices[&id]
    }

    pub fn radius_def(&self, entity_id: u32) -> RadiusDef {
        if let Some(r) = self.radius_indices.get(&entity_id) {
            RadiusDef::Param(*r)
        } else if let Some((_center_id, start_id)) = self.arc_points.get(&entity_id) {
            RadiusDef::Implicit(self.point(*start_id))
        } else {
            panic!(
                "radius_def() called for entity {} which is not a circle or arc",
                entity_id
            );
        }
    }

    pub fn line(&self, line_id: u32) -> LineIdx {
        let (start_id, end_id) = self.line_points[&line_id];
        LineIdx {
            start: self.point(start_id),
            end: self.point(end_id),
        }
    }

    pub fn num_params(&self) -> usize {
        self.num_params
    }

    pub fn extract_positions(&self, params: &[f64]) -> HashMap<u32, (f64, f64)> {
        let mut positions = HashMap::new();
        for (id, idx) in &self.point_indices {
            positions.insert(*id, (params[idx.x()], params[idx.y()]));
        }
        positions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entities() -> Vec<SketchEntity> {
        vec![
            SketchEntity::Point { id: 1, x: 0.0, y: 0.0, construction: false },
            SketchEntity::Point { id: 2, x: 1.0, y: 0.0, construction: false },
            SketchEntity::Point { id: 3, x: 1.0, y: 1.0, construction: false },
            SketchEntity::Line { id: 10, start_id: 1, end_id: 2, construction: false },
            SketchEntity::Circle { id: 20, center_id: 3, radius: 0.5, construction: false },
        ]
    }

    #[test]
    fn layout_point_indices() {
        let layout = ParamLayout::from_entities(&sample_entities());
        assert_eq!(layout.point(1).x(), 0);
        assert_eq!(layout.point(1).y(), 1);
        assert_eq!(layout.point(2).x(), 2);
        assert_eq!(layout.point(3).x(), 4);
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
        let l = layout.line(10);
        assert_eq!(l.start, layout.point(1));
        assert_eq!(l.end, layout.point(2));
    }

    #[test]
    fn layout_radius_def_circle() {
        let layout = ParamLayout::from_entities(&sample_entities());
        assert!(matches!(layout.radius_def(20), RadiusDef::Param(_)));
    }

    #[test]
    fn layout_radius_def_arc() {
        let entities = vec![
            SketchEntity::Point { id: 1, x: 0.0, y: 0.0, construction: false },
            SketchEntity::Point { id: 2, x: 1.0, y: 0.0, construction: false },
            SketchEntity::Point { id: 3, x: 0.0, y: 1.0, construction: false },
            SketchEntity::Arc { id: 30, center_id: 1, start_id: 2, end_id: 3, construction: false },
        ];
        let layout = ParamLayout::from_entities(&entities);
        assert!(matches!(layout.radius_def(30), RadiusDef::Implicit(_)));
    }

    #[test]
    fn layout_extract_positions() {
        let entities = sample_entities();
        let layout = ParamLayout::from_entities(&entities);
        let params = layout.initial_params(&entities);
        let positions = layout.extract_positions(&params);
        assert_eq!(positions[&1], (0.0, 0.0));
        assert_eq!(positions[&2], (1.0, 0.0));
        assert_eq!(positions[&3], (1.0, 1.0));
    }
}
