//! Tolerance and units constants for the geometry kernel.
//!
//! All distances are in meters. The kernel supports features down to 1 micrometer.
//! These constants are the single source of truth for tolerance policy (A14).
//!
//! Tolerance hierarchy (coarsest → finest):
//!   MIN_FEATURE_SIZE > TAU_PARALLEL > TAU_MODEL > TAU_COINCIDENT > TAU_WORK > TAU_NORMALIZE

/// Default model tolerance: 1e-7 meters (0.1 micrometers).
/// Used for coincidence decisions, join/weld admissibility, vertex welding.
pub const TAU_MODEL: f64 = 1e-7;

/// Minimum feature size: 1e-6 meters (1 micrometer).
/// Features smaller than this may be collapsed.
pub const MIN_FEATURE_SIZE: f64 = 1e-6;

/// Numeric working precision floor: 1e-12 meters.
/// Used for iterative solver convergence, degenerate-geometry detection (area/length),
/// and precision floor for cross-product / normal-length checks.
pub const TAU_WORK: f64 = 1e-12;

/// Zero-length vector guard: 1e-15.
/// Used to prevent division by zero in vector normalization.
/// Also used as TAU_NORMALIZE² ≈ 1e-30 for cross-product magnitude checks.
pub const TAU_NORMALIZE: f64 = 1e-15;

/// Near-parallel / coplanar threshold: 1e-6.
/// Used for dot-product checks against 1.0 (parallel) or 0.0 (perpendicular).
pub const TAU_PARALLEL: f64 = 1e-6;

/// Point-on-surface / SSI coincidence tolerance: 1e-9.
/// Used in SSI solvers for height-range containment, point-in-solid classification,
/// and Z-range comparisons. Finer than TAU_MODEL to avoid premature snapping,
/// coarser than TAU_WORK to tolerate floating-point accumulation.
pub const TAU_COINCIDENT: f64 = 1e-9;

/// Relative weld factor for scale-adaptive tolerance: 1e-7.
/// tau_weld = (model_diagonal * TAU_WELD_FACTOR).clamp(TAU_WELD_MIN, TAU_WELD_MAX).
pub const TAU_WELD_FACTOR: f64 = 1e-7;

/// Minimum weld tolerance (clamp floor): 1e-12.
pub const TAU_WELD_MIN: f64 = 1e-12;

/// Maximum weld tolerance (clamp ceiling): 1e-4.
pub const TAU_WELD_MAX: f64 = 1e-4;

/// Relative grid factor for f32 vertex deduplication in tessellation: 1e-5.
/// Applied as (max_abs * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN).
pub const TAU_TESS_GRID_FACTOR: f64 = 1e-5;

/// Minimum tessellation grid size: 1e-10.
pub const TAU_TESS_GRID_MIN: f64 = 1e-10;

/// Relative mesh dedup factor for f32 oracle grid: 2e-6.
/// Used in tessellation watertightness oracle as (max_abs * TAU_ORACLE_FACTOR).max(TAU_ORACLE_MIN).
pub const TAU_ORACLE_FACTOR: f32 = 2e-6;

/// Minimum oracle grid size: 1e-4.
pub const TAU_ORACLE_MIN: f32 = 1e-4;

/// Relative quantization step for intersection cache: 1e-3.
/// Applied as (tau * TAU_CACHE_STEP_FACTOR).max(TAU_NORMALIZE).
pub const TAU_CACHE_STEP_FACTOR: f64 = 1e-3;

/// Relative snap grid for polygon clipping coordinates: 1e-4.
/// Applied as tau * TAU_SNAP_FACTOR.
pub const TAU_SNAP_FACTOR: f64 = 1e-4;

/// Minimum clamp for scale-adaptive tessellation welding: 1e-8.
pub const TAU_TESS_WELD_MIN: f64 = 1e-8;

/// Maximum clamp for scale-adaptive tessellation welding: 1e-2.
pub const TAU_TESS_WELD_MAX: f64 = 1e-2;
