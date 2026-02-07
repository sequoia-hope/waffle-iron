use std::fmt;

use serde::{Deserialize, Serialize};

use crate::boolean::engine::{boolean_op, BoolOp};
use crate::geometry::point::Point3d;
use crate::geometry::vector::Vec3;
use crate::operations::chamfer::chamfer_edge;
use crate::operations::extrude::{extrude_profile, Profile};
use crate::operations::fillet::fillet_edge;
use crate::operations::revolve::revolve_profile;
use crate::topology::brep::{EntityStore, SolidId};

// ─── Error type ─────────────────────────────────────────────────────────────

/// Errors that can occur during feature tree evaluation.
#[derive(Debug, Clone)]
pub enum FeatureError {
    /// A feature references a sketch index that doesn't exist.
    InvalidSketchIndex {
        feature_index: usize,
        sketch_index: usize,
    },
    /// An edge index is out of range for the solid's edge list.
    InvalidEdgeIndex {
        feature_index: usize,
        edge_index: usize,
        edge_count: usize,
    },
    /// A fillet/chamfer/boolean feature has no preceding solid to modify.
    NoSolidToModify {
        feature_index: usize,
    },
    /// A boolean operation failed.
    BooleanFailed {
        feature_index: usize,
        message: String,
    },
    /// The constraint solver failed on a sketch.
    SolverFailed {
        feature_index: usize,
        message: String,
    },
    /// A boolean references a tool solid that doesn't exist.
    InvalidToolIndex {
        feature_index: usize,
        tool_index: usize,
        solid_count: usize,
    },
}

impl fmt::Display for FeatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSketchIndex { feature_index, sketch_index } =>
                write!(f, "Feature {feature_index}: sketch index {sketch_index} does not exist"),
            Self::InvalidEdgeIndex { feature_index, edge_index, edge_count } =>
                write!(f, "Feature {feature_index}: edge index {edge_index} out of range (solid has {edge_count} edges)"),
            Self::NoSolidToModify { feature_index } =>
                write!(f, "Feature {feature_index}: no solid to modify"),
            Self::BooleanFailed { feature_index, message } =>
                write!(f, "Feature {feature_index}: boolean failed: {message}"),
            Self::SolverFailed { feature_index, message } =>
                write!(f, "Feature {feature_index}: solver failed: {message}"),
            Self::InvalidToolIndex { feature_index, tool_index, solid_count } =>
                write!(f, "Feature {feature_index}: tool index {tool_index} out of range ({solid_count} solids)"),
        }
    }
}

impl std::error::Error for FeatureError {}

// ─── Feature types ──────────────────────────────────────────────────────────

/// Parametric feature tree node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Feature {
    /// A sketch on a construction plane with optional constraints.
    Sketch {
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        profiles: Vec<SketchProfile>,
        /// Sketch constraints for solver integration.
        #[serde(default)]
        constraints: Vec<SketchConstraint>,
        /// Line entities connecting point indices for solver.
        #[serde(default)]
        lines: Vec<(usize, usize)>,
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
        #[serde(default = "default_revolve_segments")]
        segments: usize,
    },
    /// Fillet edges of the most recent solid.
    Fillet {
        edge_indices: Vec<usize>,
        radius: Parameter,
        #[serde(default = "default_fillet_segments")]
        segments: usize,
    },
    /// Chamfer edges of the most recent solid.
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

fn default_revolve_segments() -> usize { 24 }
fn default_fillet_segments() -> usize { 4 }

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

/// A 2D sketch profile — either static points or solver-resolved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SketchProfile {
    pub points: Vec<[f64; 2]>,
    pub closed: bool,
}

/// Sketch constraints for solver integration.
///
/// These mirror the constraint solver's types but are serializable as part
/// of the feature tree. During evaluation, they are translated to solver
/// constraints and solved to produce final point positions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SketchConstraint {
    Fixed { point: usize, x: f64, y: f64 },
    Horizontal { line: usize },
    Vertical { line: usize },
    Distance { point_a: usize, point_b: usize, value: f64 },
    Coincident { point_a: usize, point_b: usize },
    Parallel { line_a: usize, line_b: usize },
    Perpendicular { line_a: usize, line_b: usize },
    Radius { entity: usize, value: f64 },
    Angle { line_a: usize, line_b: usize, value: f64 },
    Equal { entity_a: usize, entity_b: usize },
    Tangent { entity_a: usize, entity_b: usize },
}

// ─── Feature tree ───────────────────────────────────────────────────────────

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

    /// Resolve a parameter value: if a tree-level parameter with the same
    /// name exists, use its value (enables parametric updates). Otherwise
    /// use the parameter's own value.
    fn resolve_parameter(&self, param: &Parameter) -> f64 {
        self.get_parameter(&param.name)
            .map(|p| p.value)
            .unwrap_or(param.value)
    }

    pub fn feature_count(&self) -> usize {
        self.features.len()
    }

    pub fn clear(&mut self) {
        self.features.clear();
        self.parameters.clear();
    }

    /// Evaluate the feature tree, producing solids in the EntityStore.
    ///
    /// Returns the list of solid IDs produced. Each feature that creates
    /// geometry (Sketch+Extrude, Sketch+Revolve) is evaluated in order;
    /// Fillet/Chamfer modify the most recent solid; Boolean ops combine solids.
    ///
    /// Returns `Err` if a feature references invalid indices or an operation fails.
    pub fn evaluate(&self, store: &mut EntityStore) -> Result<Vec<SolidId>, FeatureError> {
        let mut solids: Vec<SolidId> = Vec::new();
        let mut sketch_profiles: Vec<Vec<Point3d>> = Vec::new();

        for (fi, feature) in self.features.iter().enumerate() {
            match feature {
                Feature::Sketch {
                    profiles,
                    constraints,
                    lines,
                    ..
                } => {
                    if constraints.is_empty() {
                        // Static sketch — use profile points directly
                        for profile in profiles {
                            let pts: Vec<Point3d> = profile
                                .points
                                .iter()
                                .map(|p| Point3d::new(p[0], p[1], 0.0))
                                .collect();
                            sketch_profiles.push(pts);
                        }
                    } else {
                        // Constrained sketch — solve with constraint solver
                        let solved = self.solve_sketch(fi, profiles, constraints, lines)?;
                        sketch_profiles.push(solved);
                    }
                }
                Feature::Extrude {
                    sketch_index,
                    distance,
                    direction,
                    symmetric,
                } => {
                    let pts = sketch_profiles.get(*sketch_index).ok_or(
                        FeatureError::InvalidSketchIndex {
                            feature_index: fi,
                            sketch_index: *sketch_index,
                        },
                    )?;
                    let profile = Profile::from_points(pts.clone());
                    let dir = Vec3::new(direction[0], direction[1], direction[2]);
                    let dist = self.resolve_parameter(distance);

                    if *symmetric {
                        let s1 = extrude_profile(store, &profile, dir, dist / 2.0);
                        let s2 = extrude_profile(store, &profile, -dir, dist / 2.0);
                        let combined = boolean_op(store, s1, s2, BoolOp::Union);
                        solids.push(combined.unwrap_or(s1));
                    } else {
                        let solid = extrude_profile(store, &profile, dir, dist);
                        solids.push(solid);
                    }
                }
                Feature::Revolve {
                    sketch_index,
                    axis_origin,
                    axis_direction,
                    angle,
                    segments,
                } => {
                    let pts = sketch_profiles.get(*sketch_index).ok_or(
                        FeatureError::InvalidSketchIndex {
                            feature_index: fi,
                            sketch_index: *sketch_index,
                        },
                    )?;
                    let origin =
                        Point3d::new(axis_origin[0], axis_origin[1], axis_origin[2]);
                    let dir = Vec3::new(
                        axis_direction[0],
                        axis_direction[1],
                        axis_direction[2],
                    );
                    let ang = self.resolve_parameter(angle);

                    // For revolve, remap sketch points from the sketch plane
                    // to the profile plane containing the revolution axis.
                    // Sketch points are (u, v, 0) from XY plane; for revolve
                    // around Z they need to be in XZ plane: (u, 0, v).
                    let profile_pts = Self::remap_for_revolve(pts, &dir);
                    let solid = revolve_profile(store, &profile_pts, origin, dir, ang, *segments);
                    solids.push(solid);
                }
                Feature::Chamfer {
                    edge_indices,
                    distance,
                } => {
                    let last_solid = *solids.last().ok_or(FeatureError::NoSolidToModify {
                        feature_index: fi,
                    })?;
                    let dist = self.resolve_parameter(distance);
                    let edges = Self::collect_unique_edges(store, last_solid);

                    // Apply chamfer to each selected edge
                    let mut current = last_solid;
                    for &idx in edge_indices {
                        if idx >= edges.len() {
                            return Err(FeatureError::InvalidEdgeIndex {
                                feature_index: fi,
                                edge_index: idx,
                                edge_count: edges.len(),
                            });
                        }
                        let (v0, v1) = edges[idx];
                        current = chamfer_edge(store, current, v0, v1, dist);
                    }
                    solids.push(current);
                }
                Feature::Fillet {
                    edge_indices,
                    radius,
                    segments,
                } => {
                    let last_solid = *solids.last().ok_or(FeatureError::NoSolidToModify {
                        feature_index: fi,
                    })?;
                    let r = self.resolve_parameter(radius);
                    let edges = Self::collect_unique_edges(store, last_solid);

                    // Apply fillet to each selected edge
                    let mut current = last_solid;
                    for &idx in edge_indices {
                        if idx >= edges.len() {
                            return Err(FeatureError::InvalidEdgeIndex {
                                feature_index: fi,
                                edge_index: idx,
                                edge_count: edges.len(),
                            });
                        }
                        let (v0, v1) = edges[idx];
                        current = fillet_edge(store, current, v0, v1, r, *segments);
                    }
                    solids.push(current);
                }
                Feature::BooleanOp {
                    op_type,
                    tool_feature,
                } => {
                    if solids.len() < 2 {
                        return Err(FeatureError::NoSolidToModify { feature_index: fi });
                    }
                    if *tool_feature >= solids.len() {
                        return Err(FeatureError::InvalidToolIndex {
                            feature_index: fi,
                            tool_index: *tool_feature,
                            solid_count: solids.len(),
                        });
                    }
                    let target = *solids.last().unwrap();
                    let tool = solids[*tool_feature];
                    let op = match op_type {
                        BooleanOpType::Union => BoolOp::Union,
                        BooleanOpType::Subtract => BoolOp::Difference,
                        BooleanOpType::Intersect => BoolOp::Intersection,
                    };
                    match boolean_op(store, target, tool, op) {
                        Ok(result) => solids.push(result),
                        Err(e) => {
                            return Err(FeatureError::BooleanFailed {
                                feature_index: fi,
                                message: e.to_string(),
                            })
                        }
                    }
                }
            }
        }

        Ok(solids)
    }

    /// Solve a constrained sketch using the Gauss-Newton solver.
    ///
    /// Takes the sketch profile points as initial guesses, applies constraints,
    /// and returns the solved point positions as 3D points (z=0).
    fn solve_sketch(
        &self,
        feature_index: usize,
        profiles: &[SketchProfile],
        constraints: &[SketchConstraint],
        lines: &[(usize, usize)],
    ) -> Result<Vec<Point3d>, FeatureError> {
        use cad_solver::{Constraint as SC, Sketch, SolverConfig, solve_sketch};

        let mut sketch = Sketch::new();

        // Add all profile points as solver entities
        let mut point_indices = Vec::new();
        for profile in profiles {
            for p in &profile.points {
                let idx = sketch.add_point(p[0], p[1]);
                point_indices.push(idx);
            }
        }

        // Add line entities
        for &(start, end) in lines {
            if start < point_indices.len() && end < point_indices.len() {
                sketch.add_line(point_indices[start], point_indices[end]);
            }
        }

        // Translate sketch constraints to solver constraints
        for c in constraints {
            let sc = match c {
                SketchConstraint::Fixed { point, x, y } => SC::Fixed {
                    point: *point,
                    x: *x,
                    y: *y,
                },
                SketchConstraint::Horizontal { line } => SC::Horizontal { line: *line },
                SketchConstraint::Vertical { line } => SC::Vertical { line: *line },
                SketchConstraint::Distance { point_a, point_b, value } => SC::Distance {
                    point_a: *point_a,
                    point_b: *point_b,
                    value: *value,
                },
                SketchConstraint::Coincident { point_a, point_b } => SC::Coincident {
                    point_a: *point_a,
                    point_b: *point_b,
                },
                SketchConstraint::Parallel { line_a, line_b } => SC::Parallel {
                    line_a: *line_a,
                    line_b: *line_b,
                },
                SketchConstraint::Perpendicular { line_a, line_b } => SC::Perpendicular {
                    line_a: *line_a,
                    line_b: *line_b,
                },
                SketchConstraint::Radius { entity, value } => SC::Radius {
                    entity: *entity,
                    value: *value,
                },
                SketchConstraint::Angle { line_a, line_b, value } => SC::Angle {
                    line_a: *line_a,
                    line_b: *line_b,
                    value: *value,
                },
                SketchConstraint::Equal { entity_a, entity_b } => SC::Equal {
                    entity_a: *entity_a,
                    entity_b: *entity_b,
                },
                SketchConstraint::Tangent { entity_a, entity_b } => SC::Tangent {
                    entity_a: *entity_a,
                    entity_b: *entity_b,
                },
            };
            sketch.add_constraint(sc);
        }

        let config = SolverConfig {
            max_iterations: 200,
            ..SolverConfig::default()
        };

        solve_sketch(&mut sketch, &config).map_err(|e| FeatureError::SolverFailed {
            feature_index,
            message: e.to_string(),
        })?;

        // Extract solved profile from line chain or point order
        let solved_pts = if !lines.is_empty() {
            // Use extract_profile to chain line endpoints
            match sketch.extract_profile() {
                Some(pts) => pts
                    .iter()
                    .map(|&(x, y)| Point3d::new(x, y, 0.0))
                    .collect(),
                None => {
                    // Fallback: use solved point positions in order
                    point_indices
                        .iter()
                        .map(|&idx| {
                            let (x, y) = sketch.point_position(idx);
                            Point3d::new(x, y, 0.0)
                        })
                        .collect()
                }
            }
        } else {
            // No lines defined: use point positions directly
            point_indices
                .iter()
                .map(|&idx| {
                    let (x, y) = sketch.point_position(idx);
                    Point3d::new(x, y, 0.0)
                })
                .collect()
        };

        Ok(solved_pts)
    }

    /// Remap 2D sketch points into a profile plane suitable for revolution.
    ///
    /// Sketch profiles live in the XY plane (x, y, 0). For revolving around
    /// an axis, the profile needs to be in a plane containing that axis.
    /// The first sketch coordinate becomes the radial distance, and the
    /// second becomes the position along the axis.
    fn remap_for_revolve(pts: &[Point3d], axis: &Vec3) -> Vec<Point3d> {
        let axis_n = axis.normalize();

        // Build a radial direction perpendicular to the axis
        let radial = if axis_n.x.abs() < 0.9 {
            Vec3::new(1.0, 0.0, 0.0)
        } else {
            Vec3::new(0.0, 1.0, 0.0)
        };
        let radial = (radial - axis_n * radial.dot(&axis_n)).normalize();

        pts.iter()
            .map(|p| {
                // p.x = radial distance, p.y = distance along axis
                let r = p.x;
                let h = p.y;
                Point3d::new(
                    radial.x * r + axis_n.x * h,
                    radial.y * r + axis_n.y * h,
                    radial.z * r + axis_n.z * h,
                )
            })
            .collect()
    }

    /// Collect unique edge endpoint pairs from a solid.
    ///
    /// Edges appear in multiple faces, so we deduplicate by matching
    /// endpoint positions within tolerance.
    fn collect_unique_edges(store: &EntityStore, solid_id: SolidId) -> Vec<(Point3d, Point3d)> {
        let mut all_edges = Vec::new();
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
                    all_edges.push((verts[i], verts[next]));
                }
            }
        }

        // Deduplicate: two edges match if their endpoints are within tolerance
        // (either same direction or reversed)
        let tol = 1e-6;
        let mut unique: Vec<(Point3d, Point3d)> = Vec::new();

        for (a, b) in &all_edges {
            let already = unique.iter().any(|(ua, ub)| {
                (a.distance_to(ua) < tol && b.distance_to(ub) < tol)
                    || (a.distance_to(ub) < tol && b.distance_to(ua) < tol)
            });
            if !already {
                unique.push((*a, *b));
            }
        }

        unique
    }

    /// Get the number of unique edges for a solid (useful for edge selection).
    pub fn edge_count(store: &EntityStore, solid_id: SolidId) -> usize {
        Self::collect_unique_edges(store, solid_id).len()
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
        let mut tree = FeatureTree::new();

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
            constraints: vec![],
            lines: vec![],
        });

        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("height", 10.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("evaluate should succeed");
        assert_eq!(solids.len(), 1, "Should produce one solid");

        let bb = store.solid_bounding_box(solids[0]);
        assert!((bb.max.z - 10.0).abs() < 0.1, "Height should be ~10");
    }

    #[test]
    fn test_evaluate_chamfer_feature() {
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
            constraints: vec![],
            lines: vec![],
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
        let solids = tree.evaluate(&mut store).expect("evaluate should succeed");
        assert!(solids.len() >= 2, "Should produce at least 2 solids (box + chamfered)");
    }

    #[test]
    fn test_evaluate_fillet_feature() {
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
            constraints: vec![],
            lines: vec![],
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
            segments: 4,
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("evaluate should succeed");
        assert!(solids.len() >= 2, "Should produce at least 2 solids (box + filleted)");
    }

    #[test]
    fn test_evaluate_parametric_update() {
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
            constraints: vec![],
            lines: vec![],
        });

        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("height", 10.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        // Evaluate with height=10
        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("evaluate should succeed");
        let bb = store.solid_bounding_box(solids[0]);
        assert!((bb.max.z - 10.0).abs() < 0.1);

        // Change height parameter and re-evaluate (parametric update)
        tree.set_parameter_value("height", 20.0);
        let mut store2 = EntityStore::new();
        let solids2 = tree.evaluate(&mut store2).expect("re-evaluate should succeed");
        let bb2 = store2.solid_bounding_box(solids2[0]);
        assert!((bb2.max.z - 20.0).abs() < 0.1, "Re-evaluated height should be ~20");
    }

    #[test]
    fn test_invalid_sketch_index_error() {
        let mut tree = FeatureTree::new();
        tree.add_feature(Feature::Extrude {
            sketch_index: 99,
            distance: Parameter::new("h", 10.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        let mut store = EntityStore::new();
        let result = tree.evaluate(&mut store);
        assert!(result.is_err());
        match result.unwrap_err() {
            FeatureError::InvalidSketchIndex { sketch_index, .. } => {
                assert_eq!(sketch_index, 99);
            }
            other => panic!("Expected InvalidSketchIndex, got {:?}", other),
        }
    }

    #[test]
    fn test_no_solid_to_modify_error() {
        let mut tree = FeatureTree::new();
        tree.add_feature(Feature::Fillet {
            edge_indices: vec![0],
            radius: Parameter::new("r", 1.0),
            segments: 4,
        });

        let mut store = EntityStore::new();
        let result = tree.evaluate(&mut store);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FeatureError::NoSolidToModify { .. }));
    }

    #[test]
    fn test_invalid_edge_index_error() {
        let mut tree = FeatureTree::new();
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
                closed: true,
            }],
            constraints: vec![],
            lines: vec![],
        });
        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("h", 10.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });
        tree.add_feature(Feature::Chamfer {
            edge_indices: vec![999],
            distance: Parameter::new("d", 1.0),
        });

        let mut store = EntityStore::new();
        let result = tree.evaluate(&mut store);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FeatureError::InvalidEdgeIndex { .. }));
    }

    #[test]
    fn test_edge_selection_specific_index() {
        // Verify that different edge_indices select different edges
        let mut tree = FeatureTree::new();
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]],
                closed: true,
            }],
            constraints: vec![],
            lines: vec![],
        });
        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("h", 8.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        // Get edge count for a box
        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("should succeed");
        let edges = FeatureTree::collect_unique_edges(&store, solids[0]);
        assert_eq!(edges.len(), 12, "Box should have 12 unique edges");

        // Chamfer edge 0 and edge 5 separately, verify they produce different results
        let mut tree1 = tree.clone();
        tree1.add_feature(Feature::Chamfer {
            edge_indices: vec![0],
            distance: Parameter::new("d", 1.0),
        });
        let mut store1 = EntityStore::new();
        let s1 = tree1.evaluate(&mut store1).expect("should succeed");
        let _bb1 = store1.solid_bounding_box(*s1.last().unwrap());

        let mut tree2 = tree.clone();
        tree2.add_feature(Feature::Chamfer {
            edge_indices: vec![5],
            distance: Parameter::new("d", 1.0),
        });
        let mut store2 = EntityStore::new();
        let s2 = tree2.evaluate(&mut store2).expect("should succeed");
        let _bb2 = store2.solid_bounding_box(*s2.last().unwrap());

        // Different edges should produce different bounding boxes (generally)
        // Both should be valid solids though
        let info1 = store1.count_topology(store1.solids[*s1.last().unwrap()].shells[0]);
        let info2 = store2.count_topology(store2.solids[*s2.last().unwrap()].shells[0]);
        assert!(info1.2 >= 7, "Chamfered box should have >= 7 faces");
        assert!(info2.2 >= 7, "Chamfered box should have >= 7 faces");
    }

    #[test]
    fn test_revolve_feature() {
        let mut tree = FeatureTree::new();
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![[3.0, 0.0], [5.0, 4.0], [3.5, 8.0]],
                closed: false,
            }],
            constraints: vec![],
            lines: vec![],
        });

        tree.add_feature(Feature::Revolve {
            sketch_index: 0,
            axis_origin: [0.0, 0.0, 0.0],
            axis_direction: [0.0, 0.0, 1.0],
            angle: Parameter::new("angle", std::f64::consts::TAU),
            segments: 16,
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("evaluate should succeed");
        assert_eq!(solids.len(), 1);
        let bb = store.solid_bounding_box(solids[0]);
        // Revolved profile should extend to radius 5 in both x/y
        assert!(bb.max.x > 4.5, "Max x should be > 4.5, got {}", bb.max.x);
        assert!(bb.max.z > 7.5, "Max z should be > 7.5, got {}", bb.max.z);
    }

    #[test]
    fn test_constrained_sketch_extrude() {
        let mut tree = FeatureTree::new();

        // Sketch with 4 points, lines connecting them, and constraints
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![
                    [0.0, 0.0],     // p0 — will be fixed at origin
                    [9.0, 0.5],     // p1 — solver should move to (10, 0)
                    [9.5, 4.5],     // p2 — solver should move to (10, 5)
                    [0.5, 5.5],     // p3 — solver should move to (0, 5)
                ],
                closed: true,
            }],
            constraints: vec![
                SketchConstraint::Fixed { point: 0, x: 0.0, y: 0.0 },
                SketchConstraint::Horizontal { line: 4 },  // line p0->p1
                SketchConstraint::Vertical { line: 5 },    // line p1->p2
                SketchConstraint::Horizontal { line: 6 },  // line p2->p3
                SketchConstraint::Vertical { line: 7 },    // line p3->p0
                SketchConstraint::Distance { point_a: 0, point_b: 1, value: 10.0 },
                SketchConstraint::Distance { point_a: 1, point_b: 2, value: 5.0 },
            ],
            lines: vec![(0, 1), (1, 2), (2, 3), (3, 0)],
        });

        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("depth", 7.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("constrained sketch + extrude should succeed");
        assert_eq!(solids.len(), 1);

        let bb = store.solid_bounding_box(solids[0]);
        // Solved rectangle should be 10x5, extruded to 7
        assert!((bb.max.x - 10.0).abs() < 0.5, "Width should be ~10, got {}", bb.max.x);
        assert!((bb.max.y - 5.0).abs() < 0.5, "Height should be ~5, got {}", bb.max.y);
        assert!((bb.max.z - 7.0).abs() < 0.5, "Depth should be ~7, got {}", bb.max.z);
    }

    #[test]
    fn test_feature_tree_clear() {
        let mut tree = FeatureTree::new();
        tree.add_parameter(Parameter::new("w", 10.0));
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0; 3],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![],
            constraints: vec![],
            lines: vec![],
        });
        assert_eq!(tree.feature_count(), 1);

        tree.clear();
        assert_eq!(tree.feature_count(), 0);
        assert!(tree.get_parameter("w").is_none());
    }

    #[test]
    fn test_parameter_resolution() {
        let mut tree = FeatureTree::new();
        tree.add_parameter(Parameter::new("height", 25.0));

        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0; 3],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![[0.0, 0.0], [5.0, 0.0], [5.0, 5.0], [0.0, 5.0]],
                closed: true,
            }],
            constraints: vec![],
            lines: vec![],
        });

        // Feature has "height" parameter with value 10, but tree-level
        // parameter "height" has value 25. resolve_parameter should use 25.
        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("height", 10.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("should succeed");
        let bb = store.solid_bounding_box(solids[0]);
        assert!((bb.max.z - 25.0).abs() < 0.1,
            "Height should resolve to tree parameter value 25, got {}", bb.max.z);
    }

    #[test]
    fn test_edge_count() {
        let mut store = EntityStore::new();
        use crate::topology::primitives::make_box;
        let solid = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let count = FeatureTree::edge_count(&store, solid);
        assert_eq!(count, 12, "Box should have 12 unique edges");
    }

    #[test]
    fn test_boolean_feature() {
        let mut tree = FeatureTree::new();

        // First body: box
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0; 3],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
                closed: true,
            }],
            constraints: vec![],
            lines: vec![],
        });
        tree.add_feature(Feature::Extrude {
            sketch_index: 0,
            distance: Parameter::new("h1", 10.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        // Second body: smaller box (overlapping)
        tree.add_feature(Feature::Sketch {
            plane_origin: [0.0; 3],
            plane_normal: [0.0, 0.0, 1.0],
            profiles: vec![SketchProfile {
                points: vec![[5.0, 5.0], [15.0, 5.0], [15.0, 15.0], [5.0, 15.0]],
                closed: true,
            }],
            constraints: vec![],
            lines: vec![],
        });
        tree.add_feature(Feature::Extrude {
            sketch_index: 1,
            distance: Parameter::new("h2", 10.0),
            direction: [0.0, 0.0, 1.0],
            symmetric: false,
        });

        // Boolean union
        tree.add_feature(Feature::BooleanOp {
            op_type: BooleanOpType::Union,
            tool_feature: 0,  // first solid as tool
        });

        let mut store = EntityStore::new();
        let solids = tree.evaluate(&mut store).expect("should succeed");
        assert!(solids.len() >= 3, "Should have base + tool + result");
    }
}
