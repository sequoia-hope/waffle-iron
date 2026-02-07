use serde::{Deserialize, Serialize};

use crate::boolean::engine::{boolean_op, BoolOp};
use crate::geometry::point::Point3d;
use crate::geometry::vector::Vec3;
use crate::operations::chamfer::chamfer_edge;
use crate::operations::extrude::{extrude_profile, Profile};
use crate::operations::fillet::fillet_edge;
use crate::operations::revolve::revolve_profile;
use crate::topology::brep::{EntityStore, SolidId};

/// Parametric feature tree node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Feature {
    /// A sketch on a construction plane.
    Sketch {
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        #[serde(skip)]
        profiles: Vec<SketchProfile>,
    },
    /// Extrude a sketch profile.
    Extrude {
        sketch_index: usize,
        distance: Parameter,
        direction: [f64; 3],
        symmetric: bool,
    },
    /// Revolve a sketch profile around an axis.
    Revolve {
        sketch_index: usize,
        axis_origin: [f64; 3],
        axis_direction: [f64; 3],
        angle: Parameter,
    },
    /// Fillet edges.
    Fillet {
        edge_indices: Vec<usize>,
        radius: Parameter,
    },
    /// Chamfer edges.
    Chamfer {
        edge_indices: Vec<usize>,
        distance: Parameter,
    },
    /// Boolean operation between bodies.
    BooleanOp {
        op_type: BooleanOpType,
        tool_feature: usize,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BooleanOpType {
    Union,
    Subtract,
    Intersect,
}

/// A named parameter with optional expression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub value: f64,
    pub expression: Option<String>,
}

impl Parameter {
    pub fn new(name: &str, value: f64) -> Self {
        Self {
            name: name.to_string(),
            value,
            expression: None,
        }
    }

    pub fn with_expression(name: &str, value: f64, expr: &str) -> Self {
        Self {
            name: name.to_string(),
            value,
            expression: Some(expr.to_string()),
        }
    }
}

/// A 2D sketch profile (placeholder — will connect to the constraint solver).
#[derive(Debug, Clone)]
pub struct SketchProfile {
    pub points: Vec<[f64; 2]>,
    pub closed: bool,
}

/// The feature tree that defines a parametric model.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeatureTree {
    pub features: Vec<Feature>,
    pub parameters: Vec<Parameter>,
}

impl FeatureTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_feature(&mut self, feature: Feature) -> usize {
        let idx = self.features.len();
        self.features.push(feature);
        idx
    }

    pub fn add_parameter(&mut self, param: Parameter) -> usize {
        let idx = self.parameters.len();
        self.parameters.push(param);
        idx
    }

    pub fn get_parameter(&self, name: &str) -> Option<&Parameter> {
        self.parameters.iter().find(|p| p.name == name)
    }

    pub fn set_parameter_value(&mut self, name: &str, value: f64) -> bool {
        if let Some(param) = self.parameters.iter_mut().find(|p| p.name == name) {
            param.value = value;
            true
        } else {
            false
        }
    }

    /// Evaluate the feature tree, producing solids in the EntityStore.
    ///
    /// Returns the list of solid IDs produced (one per body-creating feature).
    /// Each feature that creates geometry (Sketch+Extrude, Sketch+Revolve) is
    /// evaluated in order; Boolean ops combine previous results.
    pub fn evaluate(&self, store: &mut EntityStore) -> Vec<SolidId> {
        let mut solids: Vec<SolidId> = Vec::new();
        let mut sketch_profiles: Vec<Vec<Point3d>> = Vec::new();

        for feature in &self.features {
            match feature {
                Feature::Sketch { profiles, .. } => {
                    for profile in profiles {
                        let pts: Vec<Point3d> = profile
                            .points
                            .iter()
                            .map(|p| Point3d::new(p[0], p[1], 0.0))
                            .collect();
                        sketch_profiles.push(pts);
                    }
                }
                Feature::Extrude {
                    sketch_index,
                    distance,
                    direction,
                    symmetric,
                } => {
                    if let Some(pts) = sketch_profiles.get(*sketch_index) {
                        let profile = Profile::from_points(pts.clone());
                        let dir = Vec3::new(direction[0], direction[1], direction[2]);
                        let dist = distance.value;

                        if *symmetric {
                            // Extrude in both directions
                            let s1 = extrude_profile(store, &profile, dir, dist / 2.0);
                            let s2 = extrude_profile(store, &profile, -dir, dist / 2.0);
                            let combined = boolean_op(store, s1, s2, BoolOp::Union);
                            solids.push(combined.unwrap_or(s1));
                        } else {
                            let solid = extrude_profile(store, &profile, dir, dist);
                            solids.push(solid);
                        }
                    }
                }
                Feature::Revolve {
                    sketch_index,
                    axis_origin,
                    axis_direction,
                    angle,
                } => {
                    if let Some(pts) = sketch_profiles.get(*sketch_index) {
                        let origin =
                            Point3d::new(axis_origin[0], axis_origin[1], axis_origin[2]);
                        let dir = Vec3::new(
                            axis_direction[0],
                            axis_direction[1],
                            axis_direction[2],
                        );
                        let solid = revolve_profile(store, pts, origin, dir, angle.value, 24);
                        solids.push(solid);
                    }
                }
                Feature::Chamfer {
                    edge_indices: _,
                    distance,
                } => {
                    // Apply chamfer to the most recent solid
                    if let Some(&last_solid) = solids.last() {
                        let edges = Self::collect_edge_endpoints(store, last_solid);
                        if let Some((v0, v1)) = edges.first() {
                            let result = chamfer_edge(store, last_solid, *v0, *v1, distance.value);
                            solids.push(result);
                        }
                    }
                }
                Feature::Fillet {
                    edge_indices: _,
                    radius,
                } => {
                    // Apply fillet to the most recent solid
                    if let Some(&last_solid) = solids.last() {
                        let edges = Self::collect_edge_endpoints(store, last_solid);
                        if let Some((v0, v1)) = edges.first() {
                            let result = fillet_edge(store, last_solid, *v0, *v1, radius.value, 4);
                            solids.push(result);
                        }
                    }
                }
                Feature::BooleanOp {
                    op_type,
                    tool_feature,
                } => {
                    if solids.len() >= 2 && *tool_feature < solids.len() {
                        let target = solids[solids.len() - 2];
                        let tool = solids[*tool_feature];
                        let op = match op_type {
                            BooleanOpType::Union => BoolOp::Union,
                            BooleanOpType::Subtract => BoolOp::Difference,
                            BooleanOpType::Intersect => BoolOp::Intersection,
                        };
                        if let Ok(result) = boolean_op(store, target, tool, op) {
                            solids.push(result);
                        }
                    }
                }
            }
        }

        solids
    }

    /// Collect edge endpoint pairs from a solid for chamfer/fillet operations.
    fn collect_edge_endpoints(store: &EntityStore, solid_id: SolidId) -> Vec<(Point3d, Point3d)> {
        let mut edges = Vec::new();
        let solid = &store.solids[solid_id];
        for &shell_id in &solid.shells {
            let shell = &store.shells[shell_id];
            for &face_id in &shell.faces {
                let face = &store.faces[face_id];
                let loop_data = &store.loops[face.outer_loop];
                let verts: Vec<Point3d> = loop_data
                    .half_edges
                    .iter()
                    .map(|&he_id| store.vertices[store.half_edges[he_id].start_vertex].point)
                    .collect();
                for i in 0..verts.len() {
                    let next = (i + 1) % verts.len();
                    edges.push((verts[i], verts[next]));
                }
            }
        }
        edges
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_tree_creation() {
        let mut tree = FeatureTree::new();
        let idx = tree.add_parameter(Parameter::new("width", 10.0));
        assert_eq!(idx, 0);
        assert_eq!(tree.get_parameter("width").unwrap().value, 10.0);
    }

    #[test]
    fn test_parameter_update() {
        let mut tree = FeatureTree::new();
        tree.add_parameter(Parameter::new("height", 5.0));
        assert!(tree.set_parameter_value("height", 15.0));
        assert_eq!(tree.get_parameter("height").unwrap().value, 15.0);
    }

    #[test]
    fn test_evaluate_extrude() {
        use crate::topology::brep::EntityStore;

        let mut tree = FeatureTree::new();

        // Add a rectangle sketch
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![
                    [-5.0, -3.0],
                    [5.0, -3.0],
                    [5.0, 3.0],
                    [-5.0, 3.0],
                ],
                closed: true,
            }],
        });

        // Extrude it
        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("height", 10.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store);
        assert_eq!(solids.len(), 1, "Should produce one solid");

        // Check bounding box
        let bb = store.solid_bounding_box(solids[0]);
        assert!((bb.max.z - 10.0).abs() < 0.1, "Height should be ~10");
    }

    #[test]
    fn test_evaluate_chamfer_feature() {
        use crate::topology::brep::EntityStore;

        let mut tree = FeatureTree::new();

        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![
                    [0.0, 0.0],
                    [10.0, 0.0],
                    [10.0, 10.0],
                    [0.0, 10.0],
                ],
                closed: true,
            }],
        });

        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("height", 10.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        tree.add_feature(Feature::Chamfer {
            edge_indices: vec![0],
            distance: Parameter::new("chamfer_dist", 1.0),
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store);
        assert!(solids.len() >= 2, "Should produce at least 2 solids (box + chamfered)");
    }

    #[test]
    fn test_evaluate_fillet_feature() {
        use crate::topology::brep::EntityStore;

        let mut tree = FeatureTree::new();

        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![
                    [0.0, 0.0],
                    [10.0, 0.0],
                    [10.0, 10.0],
                    [0.0, 10.0],
                ],
                closed: true,
            }],
        });

        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("height", 10.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        tree.add_feature(Feature::Fillet {
            edge_indices: vec![0],
            radius: Parameter::new("fillet_radius", 1.5),
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store);
        assert!(solids.len() >= 2, "Should produce at least 2 solids (box + filleted)");
    }

    #[test]
    fn test_evaluate_parametric_update() {
        use crate::topology::brep::EntityStore;

        let mut tree = FeatureTree::new();
        tree.add_parameter(Parameter::new("height", 10.0));

        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![
                    [0.0, 0.0],
                    [5.0, 0.0],
                    [5.0, 5.0],
                    [0.0, 5.0],
                ],
                closed: true,
            }],
        });

        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("height", 10.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        // Evaluate with height=10
        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store);
        let bb = store.solid_bounding_box(solids[0]);
        assert!((bb.max.z - 10.0).abs() < 0.1);

        // Change height parameter and re-evaluate
        tree.set_parameter_value("height", 20.0);
        // Update the feature's distance parameter
        if let Feature::Extrude { distance, .. } = &mut tree.features[1] {
            distance.value = 20.0;
        }
        let mut store2 = EntityStore::new();
        let solids2 = tree.evaluate(&mut store2);
        let bb2 = store2.solid_bounding_box(solids2[0]);
        assert!((bb2.max.z - 20.0).abs() < 0.1, "Re-evaluated height should be ~20");
    }
}
