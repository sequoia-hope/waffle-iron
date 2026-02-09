use serde::{Deserialize, Serialize};
use uuid::Uuid;
use waffle_types::{GeomRef, Sketch};

/// The ordered list of modeling features.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureTree {
    /// Ordered list of features. Index 0 is the first feature.
    pub features: Vec<Feature>,
    /// Features after this index are suppressed during rebuild.
    /// None means all features are active.
    pub active_index: Option<usize>,
}

impl FeatureTree {
    pub fn new() -> Self {
        Self {
            features: Vec::new(),
            active_index: None,
        }
    }

    /// Return active features (up to active_index).
    pub fn active_features(&self) -> &[Feature] {
        match self.active_index {
            Some(idx) => &self.features[..=idx.min(self.features.len().saturating_sub(1))],
            None => &self.features,
        }
    }
}

impl Default for FeatureTree {
    fn default() -> Self {
        Self::new()
    }
}

/// A single feature in the parametric feature tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    /// Unique identifier.
    pub id: Uuid,
    /// User-visible name.
    pub name: String,
    /// The modeling operation this feature performs.
    pub operation: Operation,
    /// Whether this feature is suppressed.
    pub suppressed: bool,
    /// GeomRefs to geometry that this feature depends on.
    pub references: Vec<GeomRef>,
}

/// A parametric modeling operation with its parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Operation {
    Sketch { sketch: Sketch },
    Extrude { params: ExtrudeParams },
    Revolve { params: RevolveParams },
    Fillet { params: FilletParams },
    Chamfer { params: ChamferParams },
    Shell { params: ShellParams },
    BooleanCombine { params: BooleanParams },
}

/// Parameters for an extrude operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtrudeParams {
    pub sketch_id: Uuid,
    pub profile_index: usize,
    pub depth: f64,
    pub direction: Option<[f64; 3]>,
    pub symmetric: bool,
    pub cut: bool,
    pub target_body: Option<GeomRef>,
}

/// Parameters for a revolve operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevolveParams {
    pub sketch_id: Uuid,
    pub profile_index: usize,
    pub axis_origin: [f64; 3],
    pub axis_direction: [f64; 3],
    pub angle: f64,
}

/// Parameters for a fillet operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilletParams {
    pub edges: Vec<GeomRef>,
    pub radius: f64,
}

/// Parameters for a chamfer operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChamferParams {
    pub edges: Vec<GeomRef>,
    pub distance: f64,
}

/// Parameters for a shell operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellParams {
    pub faces_to_remove: Vec<GeomRef>,
    pub thickness: f64,
}

/// Parameters for a boolean combine operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BooleanParams {
    pub body_a: GeomRef,
    pub body_b: GeomRef,
    pub operation: BooleanOp,
}

/// Boolean operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BooleanOp {
    Union,
    Subtract,
    Intersect,
}

/// Errors from the feature engine.
#[derive(Debug, Clone, thiserror::Error)]
pub enum EngineError {
    #[error("feature not found: {id}")]
    FeatureNotFound { id: Uuid },

    #[error("sketch not found: {id}")]
    SketchNotFound { id: Uuid },

    #[error("profile index {index} out of range (sketch has {count} profiles)")]
    ProfileOutOfRange { index: usize, count: usize },

    #[error("GeomRef resolution failed: {reason}")]
    ResolutionFailed { reason: String },

    #[error("kernel error: {0}")]
    KernelError(#[from] kernel_fork::KernelError),

    #[error("operation error: {0}")]
    OpError(#[from] modeling_ops::OpError),

    #[error("rebuild failed at feature {feature_name}: {reason}")]
    RebuildFailed {
        feature_name: String,
        reason: String,
    },
}
