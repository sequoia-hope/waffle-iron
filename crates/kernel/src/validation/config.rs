//! Configuration for the B-Rep validation system.

use super::types::ValidationLevel;

/// Tolerance thresholds for validation checks.
#[derive(Debug, Clone, Copy)]
pub struct ToleranceConfig {
    /// Position resolution — points closer than this are coincident (meters).
    pub resolution: f64,
    /// Maximum allowed vertex tolerance.
    pub max_vertex_tol: f64,
    /// Maximum allowed edge gap (SameParameter).
    pub max_edge_tol: f64,
    /// Angular tolerance for G1 continuity checks (radians).
    pub angular_tol: f64,
    /// Maximum allowed ratio of child-to-parent tolerance.
    pub max_growth_factor: f64,
}

impl Default for ToleranceConfig {
    fn default() -> Self {
        Self {
            resolution: 1e-7,
            max_vertex_tol: 1e-3,
            max_edge_tol: 1e-4,
            angular_tol: 0.017, // ~1 degree
            max_growth_factor: 10.0,
        }
    }
}

/// Configuration controlling which checks are run and their parameters.
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// The maximum validation level to run.
    pub level: ValidationLevel,
    /// Tolerance thresholds.
    pub tolerance: ToleranceConfig,
    /// Number of sample points along each edge for SameParameter/continuity checks.
    pub sampling_density: u32,
    /// Whether to run G0/G1/G2 continuity checks (only at Full level).
    pub check_continuity: bool,
    /// Whether to run triangle-based self-intersection checks (expensive).
    pub check_self_intersection: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            level: ValidationLevel::Full,
            tolerance: ToleranceConfig::default(),
            sampling_density: 23,
            check_continuity: true,
            check_self_intersection: false,
        }
    }
}

impl ValidationConfig {
    /// Topology-only validation (fastest).
    pub fn topology() -> Self {
        Self {
            level: ValidationLevel::Topology,
            check_continuity: false,
            check_self_intersection: false,
            ..Self::default()
        }
    }

    /// Topology + geometric consistency.
    pub fn geometry() -> Self {
        Self {
            level: ValidationLevel::Geometry,
            check_continuity: false,
            check_self_intersection: false,
            ..Self::default()
        }
    }

    /// Topology + geometry + spatial coherence.
    pub fn spatial() -> Self {
        Self {
            level: ValidationLevel::Spatial,
            check_continuity: false,
            check_self_intersection: true,
            ..Self::default()
        }
    }

    /// All levels including continuity.
    pub fn full() -> Self {
        Self {
            level: ValidationLevel::Full,
            check_continuity: true,
            check_self_intersection: true,
            ..Self::default()
        }
    }
}
