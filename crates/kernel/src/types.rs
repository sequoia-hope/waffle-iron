use serde::{Deserialize, Serialize};

use crate::units::{MIN_FEATURE_SIZE, TAU_MODEL, TAU_WORK};

// Re-export shared types from waffle-types
pub use waffle_types::{CircleProfile, ClosedProfile, SplineSegment, TopoKind, TopoSignature};

/// Opaque handle to a solid in the geometry kernel.
/// NEVER persisted. Valid only for the current kernel session.
#[derive(Debug, Clone)]
pub struct KernelSolidHandle(pub(crate) u64);

impl KernelSolidHandle {
    pub(crate) fn id(&self) -> u64 {
        self.0
    }
}

/// Transient kernel-internal entity identifier.
/// Stable within a single kernel session but NOT across rebuilds.
/// NEVER persisted — use GeomRef for persistent references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KernelId(pub u64);

/// Structured error types for boolean operation failures.
/// Distinguishes failure stages (intersection, classification, stitching, topology validation)
/// so that callers can diagnose and potentially retry with adjusted parameters.
#[derive(Debug, Clone, thiserror::Error)]
pub enum BooleanError {
    #[error("invalid input topology: {detail}")]
    InvalidInput { detail: String },

    #[error("tolerance configuration error: {detail}")]
    ToleranceError { detail: String },

    #[error("intersection construction failed: {detail}")]
    IntersectionFailed { detail: String },

    #[error("face classification ambiguous: {detail}")]
    ClassificationFailed { detail: String },

    #[error("shell assembly failed: {detail}")]
    StitchingFailed { detail: String },

    #[error("result topology invalid: {detail}")]
    InvalidResult { detail: String },
}

/// Errors from kernel operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum KernelError {
    #[error("boolean operation failed: {reason}")]
    BooleanFailed { reason: String },

    #[error("fillet failed: {reason}")]
    FilletFailed { reason: String },

    #[error("shell failed: {reason}")]
    ShellFailed { reason: String },

    #[error("tessellation failed: {reason}")]
    TessellationFailed { reason: String },

    #[error("entity not found: {id:?}")]
    EntityNotFound { id: KernelId },

    #[error("operation not supported: {operation}")]
    NotSupported { operation: String },

    #[error("kernel error: {message}")]
    Other { message: String },
}

impl From<BooleanError> for KernelError {
    fn from(err: BooleanError) -> Self {
        KernelError::BooleanFailed {
            reason: err.to_string(),
        }
    }
}

/// Simplified diagnostic report from a boolean operation.
#[derive(Debug, Clone, Default)]
pub struct BooleanDiagnosticsSummary {
    pub tau_model: f64,
    pub faces_classified: usize,
    pub vertices_welded: usize,
    pub edges_canonicalized: usize,
    pub total_duration_ms: u64,
    pub warnings: Vec<String>,
    pub successful_strategy: String,
    pub perturbation_attempts: u32,
    pub perturbation_elapsed_ms: u64,
    pub preheal_vertices_unified: usize,
    pub recovery_level: u8,
}

/// Tessellated triangle mesh for rendering in three.js.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderMesh {
    /// Flat array of vertex positions [x0, y0, z0, x1, y1, z1, ...].
    pub vertices: Vec<f32>,
    /// Flat array of vertex normals [nx0, ny0, nz0, nx1, ny1, nz1, ...].
    pub normals: Vec<f32>,
    /// Triangle indices into the vertex array.
    pub indices: Vec<u32>,
    /// Mapping from triangle ranges to logical faces.
    pub face_ranges: Vec<FaceRange>,
}

/// Maps a contiguous range of triangles to a logical face.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceRange {
    pub face_id: KernelId,
    pub start_index: u32,
    pub end_index: u32,
}

/// Sharp edge data for rendering edge overlays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeRenderData {
    pub vertices: Vec<f32>,
    pub edge_ranges: Vec<EdgeRange>,
}

/// Maps a contiguous range of edge line-segment vertices to a logical edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeRange {
    pub edge_id: KernelId,
    pub start_vertex: u32,
    pub end_vertex: u32,
}

/// Options controlling tolerance layering for boolean operations.
#[derive(Debug, Clone)]
pub struct BooleanOptions {
    pub tau_model: f64,
    pub tau_mesh: f64,
    pub tau_weld: f64,
    pub tau_work: f64,
    pub tau_coplanar: f64,
    pub min_feature_size: f64,
}

impl Default for BooleanOptions {
    fn default() -> Self {
        Self {
            tau_model: TAU_MODEL,
            tau_mesh: TAU_MODEL,
            tau_weld: 0.4 * TAU_MODEL,
            tau_work: TAU_WORK,
            tau_coplanar: TAU_MODEL,
            min_feature_size: MIN_FEATURE_SIZE,
        }
    }
}

impl BooleanOptions {
    pub fn for_scale(extent: f64) -> Self {
        use crate::units::{TAU_COINCIDENT, TAU_WELD_FACTOR};
        // Scale-adaptive: TAU_WELD_FACTOR (1e-7) * extent, clamped between
        // TAU_COINCIDENT (1e-9) and 1e-5 (10× MIN_FEATURE_SIZE).
        let tau_model = (extent * TAU_WELD_FACTOR).clamp(TAU_COINCIDENT, MIN_FEATURE_SIZE * 10.0);
        Self {
            tau_model,
            tau_mesh: tau_model,
            tau_weld: 0.4 * tau_model,
            tau_work: TAU_WORK,
            tau_coplanar: tau_model,
            min_feature_size: 10.0 * tau_model,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.tau_model <= 0.0 {
            return Err(format!(
                "tau_model must be positive, got {}",
                self.tau_model
            ));
        }
        if self.tau_mesh <= 0.0 {
            return Err(format!("tau_mesh must be positive, got {}", self.tau_mesh));
        }
        if self.tau_weld <= 0.0 {
            return Err(format!("tau_weld must be positive, got {}", self.tau_weld));
        }
        if self.tau_work <= 0.0 {
            return Err(format!("tau_work must be positive, got {}", self.tau_work));
        }
        if self.tau_coplanar <= 0.0 {
            return Err(format!(
                "tau_coplanar must be positive, got {}",
                self.tau_coplanar
            ));
        }
        if self.min_feature_size <= 0.0 {
            return Err(format!(
                "min_feature_size must be positive, got {}",
                self.min_feature_size
            ));
        }
        if self.tau_mesh > self.tau_model {
            return Err(format!(
                "tau_mesh ({}) must be <= tau_model ({})",
                self.tau_mesh, self.tau_model
            ));
        }
        if self.tau_work >= self.tau_model {
            return Err(format!(
                "tau_work ({}) must be < tau_model ({})",
                self.tau_work, self.tau_model
            ));
        }
        if self.tau_weld < 0.1 * self.tau_model {
            return Err(format!(
                "tau_weld ({}) must be >= 0.1 * tau_model ({})",
                self.tau_weld, self.tau_model
            ));
        }
        if self.min_feature_size < self.tau_model {
            return Err(format!(
                "min_feature_size ({}) must be >= tau_model ({})",
                self.min_feature_size, self.tau_model
            ));
        }
        Ok(())
    }

    pub fn for_boolean_tol(tol: f64) -> Self {
        Self {
            tau_model: tol,
            tau_mesh: tol,
            tau_weld: tol * 0.4,
            tau_work: TAU_WORK,
            tau_coplanar: tol,
            min_feature_size: tol * 10.0,
        }
    }
}

// Custom Serialize/Deserialize for KernelId
impl Serialize for KernelId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for KernelId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        u64::deserialize(deserializer).map(KernelId)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kernel_error_display() {
        let err = KernelError::BooleanFailed {
            reason: "test".to_string(),
        };
        assert_eq!(format!("{}", err), "boolean operation failed: test");

        let err = KernelError::NotSupported {
            operation: "fillet".to_string(),
        };
        assert_eq!(format!("{}", err), "operation not supported: fillet");
    }

    #[test]
    fn test_boolean_options_default() {
        let opts = BooleanOptions::default();
        assert!((opts.tau_model - 1e-7).abs() < 1e-15);
        assert!(opts.validate().is_ok());
    }

    #[test]
    fn test_boolean_options_for_scale() {
        let small = BooleanOptions::for_scale(0.001);
        assert!(small.validate().is_ok());
        let large = BooleanOptions::for_scale(100.0);
        assert!(large.validate().is_ok());
    }

    #[test]
    fn test_boolean_options_validate_rejects_bad() {
        let bad = BooleanOptions {
            tau_mesh: 1e-6,
            ..BooleanOptions::default()
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn test_kernel_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(KernelId(1));
        set.insert(KernelId(2));
        set.insert(KernelId(1));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_boolean_error_to_kernel_error() {
        let bool_err = BooleanError::ClassificationFailed {
            detail: "ambiguous".to_string(),
        };
        let kernel_err: KernelError = bool_err.into();
        match &kernel_err {
            KernelError::BooleanFailed { reason } => {
                assert!(reason.contains("classification"));
            }
            other => panic!("Expected BooleanFailed, got {:?}", other),
        }
    }
}
