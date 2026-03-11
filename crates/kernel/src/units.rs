//! Tolerance and units constants for the geometry kernel.
//!
//! All distances are in meters. The kernel supports features down to 1 micrometer.
//! These constants are the single source of truth for tolerance policy (A14).

/// Default model tolerance: 1e-7 meters (0.1 micrometers).
/// Used for coincidence decisions, join/weld admissibility.
pub const TAU_MODEL: f64 = 1e-7;

/// Minimum feature size: 1e-6 meters (1 micrometer).
/// Features smaller than this may be collapsed.
pub const MIN_FEATURE_SIZE: f64 = 1e-6;

/// Numeric working precision floor: 1e-12 meters.
/// Used for iterative solver convergence.
pub const TAU_WORK: f64 = 1e-12;

/// Zero-length vector guard: 1e-15.
/// Used to prevent division by zero in vector normalization.
pub const TAU_NORMALIZE: f64 = 1e-15;

/// Near-parallel / coplanar threshold: 1e-6.
/// Used for dot-product checks against 1.0 (parallel) or 0.0 (perpendicular).
pub const TAU_PARALLEL: f64 = 1e-6;
