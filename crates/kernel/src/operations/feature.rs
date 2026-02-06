use serde::{Deserialize, Serialize};


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
}
