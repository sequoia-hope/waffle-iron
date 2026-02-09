use slvs::entity::EntityHandle;
use slvs::entity::{
    ArcOfCircle, Circle as SlvsCircle, Distance, LineSegment, Normal, Point, Workplane,
};
use slvs::group::Group;
use slvs::utils::make_quaternion;
use slvs::System;
use std::collections::HashMap;

use crate::types::{EntityKind, SketchEntity};

/// Maps sketch entities to slvs system entities.
/// Holds the slvs System and all entity handle mappings.
pub struct SketchToSlvs {
    pub system: System,
    pub group: Group,
    pub workplane: EntityHandle<Workplane>,
    pub normal_3d: EntityHandle<Normal>,
    pub point_handles: HashMap<u32, EntityHandle<Point>>,
    pub line_handles: HashMap<u32, EntityHandle<LineSegment>>,
    pub circle_handles: HashMap<u32, EntityHandle<SlvsCircle>>,
    pub arc_handles: HashMap<u32, EntityHandle<ArcOfCircle>>,
    pub distance_handles: HashMap<u32, EntityHandle<Distance>>,
    pub normal_on_wp: Option<EntityHandle<Normal>>,
    pub entity_types: HashMap<u32, EntityKind>,
}

impl Default for SketchToSlvs {
    fn default() -> Self {
        Self::new()
    }
}

impl SketchToSlvs {
    /// Create a new mapping context with an XY workplane.
    pub fn new() -> Self {
        let mut system = System::new();

        // Group 1: workplane definition
        let g1 = system.add_group();
        let origin = system
            .sketch(Point::new_in_3d(g1, [0.0, 0.0, 0.0]))
            .expect("failed to create origin point");
        let normal_3d = system
            .sketch(Normal::new_in_3d(
                g1,
                make_quaternion([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            ))
            .expect("failed to create normal");
        let workplane = system
            .sketch(Workplane::new(g1, origin, normal_3d))
            .expect("failed to create workplane");

        // Group 2: sketch entities and constraints
        let group = system.add_group();

        SketchToSlvs {
            system,
            group,
            workplane,
            normal_3d,
            point_handles: HashMap::new(),
            line_handles: HashMap::new(),
            circle_handles: HashMap::new(),
            arc_handles: HashMap::new(),
            distance_handles: HashMap::new(),
            normal_on_wp: None,
            entity_types: HashMap::new(),
        }
    }

    /// Get or create a workplane-local normal (needed for circles and arcs).
    fn get_wp_normal(&mut self) -> EntityHandle<Normal> {
        if let Some(n) = self.normal_on_wp {
            return n;
        }
        let n = self
            .system
            .sketch(Normal::new_on_workplane(self.group, self.workplane))
            .expect("failed to create workplane normal");
        self.normal_on_wp = Some(n);
        n
    }

    /// Add all sketch entities to the slvs system.
    /// Points are added first, then curves (which reference points).
    pub fn add_entities(&mut self, entities: &[SketchEntity]) {
        // Pass 1: Points
        for entity in entities {
            if let SketchEntity::Point { id, x, y, .. } = entity {
                let handle = self
                    .system
                    .sketch(Point::new_on_workplane(
                        self.group,
                        self.workplane,
                        [*x, *y],
                    ))
                    .expect("failed to add point");
                self.point_handles.insert(*id, handle);
                self.entity_types.insert(*id, EntityKind::Point);
            }
        }

        // Pass 2: Lines, Circles, Arcs
        for entity in entities {
            match entity {
                SketchEntity::Line {
                    id,
                    start_id,
                    end_id,
                    ..
                } => {
                    let start = self.point_handles[start_id];
                    let end = self.point_handles[end_id];
                    let handle = self
                        .system
                        .sketch(LineSegment::new(self.group, start, end))
                        .expect("failed to add line");
                    self.line_handles.insert(*id, handle);
                    self.entity_types.insert(*id, EntityKind::Line);
                }
                SketchEntity::Circle {
                    id,
                    center_id,
                    radius,
                    ..
                } => {
                    let center = self.point_handles[center_id];
                    let dist = self
                        .system
                        .sketch(Distance::new(self.group, *radius))
                        .expect("failed to add distance");
                    self.distance_handles.insert(*id, dist);
                    let wp_normal = self.get_wp_normal();
                    let handle = self
                        .system
                        .sketch(SlvsCircle::new(self.group, wp_normal, center, dist))
                        .expect("failed to add circle");
                    self.circle_handles.insert(*id, handle);
                    self.entity_types.insert(*id, EntityKind::Circle);
                }
                SketchEntity::Arc {
                    id,
                    center_id,
                    start_id,
                    end_id,
                    ..
                } => {
                    let center = self.point_handles[center_id];
                    let start = self.point_handles[start_id];
                    let end = self.point_handles[end_id];
                    let handle = self
                        .system
                        .sketch(ArcOfCircle::new(
                            self.group,
                            self.workplane,
                            center,
                            start,
                            end,
                        ))
                        .expect("failed to add arc");
                    self.arc_handles.insert(*id, handle);
                    self.entity_types.insert(*id, EntityKind::Arc);
                }
                SketchEntity::Point { .. } => {} // already handled
            }
        }
    }
}
