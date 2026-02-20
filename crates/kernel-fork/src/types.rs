use serde::{Deserialize, Serialize};

// Re-export shared types from waffle-types
pub use waffle_types::{ClosedProfile, TopoKind, TopoSignature};

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
    /// The KernelId of the face this range belongs to.
    pub face_id: KernelId,
    /// Start index in the indices array (inclusive).
    pub start_index: u32,
    /// End index in the indices array (exclusive).
    pub end_index: u32,
}

/// Sharp edge data for rendering edge overlays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeRenderData {
    /// Flat array of edge vertex positions [x0, y0, z0, x1, y1, z1, ...].
    pub vertices: Vec<f32>,
    /// Mapping from vertex ranges to logical edges.
    pub edge_ranges: Vec<EdgeRange>,
}

/// Maps a contiguous range of edge line-segment vertices to a logical edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeRange {
    /// The KernelId of the edge this range belongs to.
    pub edge_id: KernelId,
    /// Start index in the vertices array (in floats, not vertices).
    pub start_vertex: u32,
    /// End index in the vertices array.
    pub end_vertex: u32,
}

/// Options controlling tolerance layering for boolean operations.
///
/// Each tolerance serves a different stage of the boolean pipeline:
/// - `tau_model`: coincidence decisions, join/weld admissibility
/// - `tau_mesh`: tessellation and polyline construction
/// - `tau_weld`: vertex/edge snapping during stitching
/// - `tau_work`: iterative solver convergence (numeric floor)
/// - `tau_coplanar`: same-plane / same-surface detection
/// - `min_feature_size`: smallest geometric feature to preserve
///
/// Invariants (enforced by `validate()`):
/// - `tau_work < tau_model`
/// - `tau_mesh <= tau_model`
/// - `tau_weld >= tau_model`
/// - `min_feature_size >= tau_model`
/// - All values must be positive (> 0)
#[derive(Debug, Clone)]
pub struct BooleanOptions {
    /// Model absolute tolerance — coincidence decisions, join/weld admissibility.
    /// Default: 1e-7 (preserves 1 um features at 10x margin).
    pub tau_model: f64,
    /// Meshing/intersection tolerance — tessellation and polyline construction.
    /// Must satisfy: tau_mesh <= tau_model.
    pub tau_mesh: f64,
    /// Vertex/edge welding tolerance — snapping during stitching.
    /// Derived as 2 * tau_model.
    pub tau_weld: f64,
    /// Numeric floor / working precision — iterative solver convergence.
    /// Must satisfy: tau_work << tau_model.
    pub tau_work: f64,
    /// Coplanar detection tolerance — same-plane / same-surface decisions.
    pub tau_coplanar: f64,
    /// Minimum preserved feature size. Default: 1e-6 (1 micrometer).
    pub min_feature_size: f64,
}

impl Default for BooleanOptions {
    fn default() -> Self {
        let tau_model = 1e-7;
        Self {
            tau_model,
            tau_mesh: 0.5 * tau_model,
            tau_weld: 2.0 * tau_model,
            tau_work: 1e-12,
            tau_coplanar: 5.0 * tau_model,
            min_feature_size: 1e-6,
        }
    }
}

impl BooleanOptions {
    /// Create tolerances scaled to geometry bounding-box extent.
    ///
    /// `extent` is the maximum dimension of the combined bounding boxes.
    /// `tau_model` is clamped to `[1e-9, 1e-5]` to stay within meaningful
    /// numeric ranges for both micro-parts and large assemblies.
    pub fn for_scale(extent: f64) -> Self {
        let tau_model = (extent * 1e-7).clamp(1e-9, 1e-5);
        Self {
            tau_model,
            tau_mesh: 0.5 * tau_model,
            tau_weld: 2.0 * tau_model,
            tau_work: 1e-12,
            tau_coplanar: 5.0 * tau_model,
            min_feature_size: 10.0 * tau_model,
        }
    }

    /// Validate that all invariants hold.
    ///
    /// Returns `Err` with a description if any invariant is violated.
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
        if self.tau_weld < self.tau_model {
            return Err(format!(
                "tau_weld ({}) must be >= tau_model ({})",
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

    /// Backward-compatibility wrapper: create options from an existing boolean
    /// tolerance value (as returned by `compute_adaptive_tol`).
    ///
    /// This preserves the current operational behavior while providing the
    /// full layered-tolerance struct for callers that need it.
    pub fn for_boolean_tol(tol: f64) -> Self {
        Self {
            tau_model: tol,
            tau_mesh: tol * 0.5,
            tau_weld: tol * 2.0,
            tau_work: 1e-12,
            tau_coplanar: tol * 5.0,
            min_feature_size: tol * 10.0,
        }
    }

    /// Convert to the truck-shapeops `BooleanTolerance` struct for the boolean pipeline.
    ///
    /// Maps the kernel-level layered tolerances to the per-stage truck tolerances:
    /// - `tau_model` → `tau_model` (intersection/coincidence precision)
    /// - `tau_mesh` → `tau_mesh` (triangulation accuracy)
    /// - `tau_weld` → `tau_weld` (vertex unification in weld_coincident_edges)
    /// - `tau_coplanar` → `tau_coplanar` (normal parallelism + plane distance)
    pub fn to_boolean_tolerance(&self) -> truck_shapeops::BooleanTolerance {
        truck_shapeops::BooleanTolerance {
            tau_model: self.tau_model,
            tau_mesh: self.tau_mesh,
            tau_weld: self.tau_weld,
            tau_coplanar: self.tau_coplanar,
        }
    }
}

// Custom Serialize/Deserialize for KernelId (needed for FaceRange/EdgeRange serialization)
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

    /// KernelError Display impls via thiserror produce expected messages.
    #[test]
    fn test_kernel_error_display() {
        let err = KernelError::BooleanFailed {
            reason: "test".to_string(),
        };
        assert_eq!(format!("{}", err), "boolean operation failed: test");

        let err = KernelError::FilletFailed {
            reason: "bad".to_string(),
        };
        assert_eq!(format!("{}", err), "fillet failed: bad");

        let err = KernelError::ShellFailed {
            reason: "thin".to_string(),
        };
        assert_eq!(format!("{}", err), "shell failed: thin");

        let err = KernelError::TessellationFailed {
            reason: "mesh".to_string(),
        };
        assert_eq!(format!("{}", err), "tessellation failed: mesh");

        let err = KernelError::EntityNotFound { id: KernelId(42) };
        assert_eq!(format!("{}", err), "entity not found: KernelId(42)");

        let err = KernelError::NotSupported {
            operation: "fillet".to_string(),
        };
        assert_eq!(format!("{}", err), "operation not supported: fillet");

        let err = KernelError::Other {
            message: "oops".to_string(),
        };
        assert_eq!(format!("{}", err), "kernel error: oops");
    }

    /// KernelSolidHandle Debug output.
    #[test]
    fn test_kernel_solid_handle_debug() {
        let handle = KernelSolidHandle(42);
        let debug = format!("{:?}", handle);
        assert!(debug.contains("42"), "Debug should contain the ID");
    }

    /// FaceRange Debug output contains face_id.
    #[test]
    fn test_face_range_debug() {
        let fr = FaceRange {
            face_id: KernelId(100),
            start_index: 0,
            end_index: 6,
        };
        let debug = format!("{:?}", fr);
        assert!(debug.contains("100"));
        assert!(debug.contains("0"));
        assert!(debug.contains("6"));
    }

    /// EdgeRange Debug output contains edge_id.
    #[test]
    fn test_edge_range_debug() {
        let er = EdgeRange {
            edge_id: KernelId(200),
            start_vertex: 0,
            end_vertex: 10,
        };
        let debug = format!("{:?}", er);
        assert!(debug.contains("200"));
    }

    /// RenderMesh can be constructed and has expected structure.
    #[test]
    fn test_render_mesh_construction() {
        let mesh = RenderMesh {
            vertices: vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
            normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            indices: vec![0, 1, 2],
            face_ranges: vec![FaceRange {
                face_id: KernelId(1),
                start_index: 0,
                end_index: 3,
            }],
        };
        assert_eq!(mesh.vertices.len(), 9);
        assert_eq!(mesh.normals.len(), 9);
        assert_eq!(mesh.indices.len(), 3);
        assert_eq!(mesh.face_ranges.len(), 1);
        assert_eq!(mesh.face_ranges[0].face_id, KernelId(1));
    }

    /// EdgeRenderData can be constructed and has expected structure.
    #[test]
    fn test_edge_render_data_construction() {
        let data = EdgeRenderData {
            vertices: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            edge_ranges: vec![EdgeRange {
                edge_id: KernelId(5),
                start_vertex: 0,
                end_vertex: 2,
            }],
        };
        assert_eq!(data.vertices.len(), 6);
        assert_eq!(data.edge_ranges.len(), 1);
        assert_eq!(data.edge_ranges[0].edge_id, KernelId(5));
    }

    /// KernelId Clone and Copy semantics.
    #[test]
    fn test_kernel_id_clone_copy() {
        let id = KernelId(7);
        let id2 = id; // Copy
        let id3 = id.clone(); // Clone
        assert_eq!(id, id2);
        assert_eq!(id, id3);
    }

    /// KernelId Hash produces consistent values.
    #[test]
    fn test_kernel_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(KernelId(1));
        set.insert(KernelId(2));
        set.insert(KernelId(1)); // duplicate
        assert_eq!(set.len(), 2);
    }

    /// KernelError Clone works correctly.
    #[test]
    fn test_kernel_error_clone() {
        let err = KernelError::Other {
            message: "test".to_string(),
        };
        let err2 = err.clone();
        assert_eq!(format!("{}", err), format!("{}", err2));
    }

    /// KernelSolidHandle id() accessor returns the inner value.
    #[test]
    fn test_kernel_solid_handle_id() {
        let handle = KernelSolidHandle(99);
        assert_eq!(handle.id(), 99);
    }

    /// Each BooleanError variant has a meaningful display string.
    #[test]
    fn test_boolean_error_display() {
        let cases = vec![
            (
                BooleanError::InvalidInput {
                    detail: "empty shell".to_string(),
                },
                "invalid input topology: empty shell",
            ),
            (
                BooleanError::ToleranceError {
                    detail: "tau_mesh > tau_model".to_string(),
                },
                "tolerance configuration error: tau_mesh > tau_model",
            ),
            (
                BooleanError::IntersectionFailed {
                    detail: "curve diverged".to_string(),
                },
                "intersection construction failed: curve diverged",
            ),
            (
                BooleanError::ClassificationFailed {
                    detail: "ray cast ambiguous".to_string(),
                },
                "face classification ambiguous: ray cast ambiguous",
            ),
            (
                BooleanError::StitchingFailed {
                    detail: "open edges remain".to_string(),
                },
                "shell assembly failed: open edges remain",
            ),
            (
                BooleanError::InvalidResult {
                    detail: "non-manifold".to_string(),
                },
                "result topology invalid: non-manifold",
            ),
        ];

        for (err, expected) in cases {
            assert_eq!(
                format!("{}", err),
                expected,
                "BooleanError display mismatch for {:?}",
                err
            );
        }
    }

    /// From<BooleanError> for KernelError produces BooleanFailed variant.
    #[test]
    fn test_boolean_error_to_kernel_error() {
        let bool_err = BooleanError::ClassificationFailed {
            detail: "ambiguous face".to_string(),
        };
        let kernel_err: KernelError = bool_err.into();

        match kernel_err {
            KernelError::BooleanFailed { reason } => {
                assert_eq!(reason, "face classification ambiguous: ambiguous face");
            }
            other => panic!("Expected KernelError::BooleanFailed, got {:?}", other),
        }
    }

    // ── BooleanOptions tests ──────────────────────────────────────

    /// BooleanOptions::default() produces spec-compliant values.
    #[test]
    fn test_boolean_options_default() {
        let opts = BooleanOptions::default();
        assert!(
            (opts.tau_model - 1e-7).abs() < 1e-15,
            "tau_model should be 1e-7, got {}",
            opts.tau_model
        );
        assert!(
            (opts.tau_mesh - 5e-8).abs() < 1e-15,
            "tau_mesh should be 5e-8, got {}",
            opts.tau_mesh
        );
        assert!(
            (opts.tau_weld - 2e-7).abs() < 1e-15,
            "tau_weld should be 2e-7, got {}",
            opts.tau_weld
        );
        assert!(
            (opts.tau_work - 1e-12).abs() < 1e-20,
            "tau_work should be 1e-12, got {}",
            opts.tau_work
        );
        assert!(
            (opts.tau_coplanar - 5e-7).abs() < 1e-15,
            "tau_coplanar should be 5e-7, got {}",
            opts.tau_coplanar
        );
        assert!(
            (opts.min_feature_size - 1e-6).abs() < 1e-14,
            "min_feature_size should be 1e-6, got {}",
            opts.min_feature_size
        );
    }

    /// BooleanOptions::default() satisfies all invariants (R5).
    #[test]
    fn test_boolean_options_invariants() {
        let opts = BooleanOptions::default();
        assert!(
            opts.tau_work < opts.tau_model,
            "tau_work ({}) must be < tau_model ({})",
            opts.tau_work,
            opts.tau_model
        );
        assert!(
            opts.tau_mesh <= opts.tau_model,
            "tau_mesh ({}) must be <= tau_model ({})",
            opts.tau_mesh,
            opts.tau_model
        );
        assert!(
            opts.tau_weld >= opts.tau_model,
            "tau_weld ({}) must be >= tau_model ({})",
            opts.tau_weld,
            opts.tau_model
        );
        assert!(
            opts.min_feature_size >= opts.tau_model,
            "min_feature_size ({}) must be >= tau_model ({})",
            opts.min_feature_size,
            opts.tau_model
        );
    }

    /// for_scale produces correctly scaled values at different scales.
    #[test]
    fn test_boolean_options_for_scale() {
        // Small geometry (1mm = 0.001m)
        let small = BooleanOptions::for_scale(0.001);
        let expected_tau: f64 = (0.001_f64 * 1e-7).clamp(1e-9, 1e-5);
        assert!(
            (small.tau_model - expected_tau).abs() < 1e-20,
            "for_scale(0.001) tau_model should be {}, got {}",
            expected_tau,
            small.tau_model
        );
        assert!(
            (small.tau_mesh - expected_tau * 0.5).abs() < 1e-20,
            "tau_mesh should be half of tau_model"
        );
        assert!(
            small.validate().is_ok(),
            "Small-scale options should be valid"
        );

        // Large geometry (100m)
        let large = BooleanOptions::for_scale(100.0);
        let expected_tau: f64 = (100.0_f64 * 1e-7).clamp(1e-9, 1e-5);
        assert!(
            (large.tau_model - expected_tau).abs() < 1e-15,
            "for_scale(100) tau_model should be {}, got {}",
            expected_tau,
            large.tau_model
        );
        assert!(
            (large.tau_weld - expected_tau * 2.0).abs() < 1e-15,
            "tau_weld should be 2x tau_model"
        );
        assert!(
            large.validate().is_ok(),
            "Large-scale options should be valid"
        );
    }

    /// validate() rejects invalid configurations.
    #[test]
    fn test_boolean_options_validate_rejects_bad() {
        // tau_mesh > tau_model
        let bad_mesh = BooleanOptions {
            tau_mesh: 1e-6,
            ..BooleanOptions::default()
        };
        assert!(
            bad_mesh.validate().is_err(),
            "Should reject tau_mesh > tau_model"
        );

        // tau_work >= tau_model
        let bad_work = BooleanOptions {
            tau_work: 1e-7,
            ..BooleanOptions::default()
        };
        assert!(
            bad_work.validate().is_err(),
            "Should reject tau_work >= tau_model"
        );

        // tau_weld < tau_model
        let bad_weld = BooleanOptions {
            tau_weld: 1e-8,
            ..BooleanOptions::default()
        };
        assert!(
            bad_weld.validate().is_err(),
            "Should reject tau_weld < tau_model"
        );

        // min_feature_size < tau_model
        let bad_feature = BooleanOptions {
            min_feature_size: 1e-8,
            ..BooleanOptions::default()
        };
        assert!(
            bad_feature.validate().is_err(),
            "Should reject min_feature_size < tau_model"
        );

        // Negative value
        let bad_neg = BooleanOptions {
            tau_model: -1.0,
            ..BooleanOptions::default()
        };
        assert!(
            bad_neg.validate().is_err(),
            "Should reject negative tau_model"
        );

        // Zero value
        let bad_zero = BooleanOptions {
            tau_coplanar: 0.0,
            ..BooleanOptions::default()
        };
        assert!(
            bad_zero.validate().is_err(),
            "Should reject zero tau_coplanar"
        );
    }

    /// validate() accepts valid configurations.
    #[test]
    fn test_boolean_options_validate_accepts_good() {
        assert!(
            BooleanOptions::default().validate().is_ok(),
            "Default options should be valid"
        );
        assert!(
            BooleanOptions::for_scale(1.0).validate().is_ok(),
            "for_scale(1.0) should be valid"
        );
        assert!(
            BooleanOptions::for_scale(0.001).validate().is_ok(),
            "for_scale(0.001) should be valid"
        );
        assert!(
            BooleanOptions::for_scale(100.0).validate().is_ok(),
            "for_scale(100.0) should be valid"
        );
        assert!(
            BooleanOptions::for_boolean_tol(0.05).validate().is_ok(),
            "for_boolean_tol(0.05) should be valid"
        );
    }

    /// for_scale clamps to bounds at extreme scales.
    #[test]
    fn test_boolean_options_for_scale_clamps() {
        // Very tiny: extent=1e-6 → tau_model = (1e-6 * 1e-7) = 1e-13, clamped to 1e-9
        let tiny = BooleanOptions::for_scale(1e-6);
        assert!(
            (tiny.tau_model - 1e-9).abs() < 1e-20,
            "Tiny scale should clamp tau_model to 1e-9, got {}",
            tiny.tau_model
        );
        assert!(tiny.validate().is_ok());

        // Very large: extent=1e10 → tau_model = (1e10 * 1e-7) = 1e3, clamped to 1e-5
        let huge = BooleanOptions::for_scale(1e10);
        assert!(
            (huge.tau_model - 1e-5).abs() < 1e-15,
            "Huge scale should clamp tau_model to 1e-5, got {}",
            huge.tau_model
        );
        assert!(huge.validate().is_ok());
    }

    /// for_boolean_tol backward-compatibility wrapper produces consistent ratios.
    #[test]
    fn test_boolean_options_for_boolean_tol() {
        let tol = 0.05;
        let opts = BooleanOptions::for_boolean_tol(tol);
        assert!(
            (opts.tau_model - tol).abs() < 1e-15,
            "tau_model should equal input tol"
        );
        assert!(
            (opts.tau_mesh - tol * 0.5).abs() < 1e-15,
            "tau_mesh should be tol/2"
        );
        assert!(
            (opts.tau_weld - tol * 2.0).abs() < 1e-15,
            "tau_weld should be 2*tol"
        );
        assert!(
            opts.validate().is_ok(),
            "for_boolean_tol result should be valid"
        );
    }
}
