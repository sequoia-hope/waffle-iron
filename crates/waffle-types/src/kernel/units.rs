//! Tolerance and units constants for the kernel *contract* layer.
//!
//! All distances are in meters. The kernel supports features down to
//! 1 micrometer (see A14).
//!
//! **Ownership (A14.3):** the three canonical numeric tolerances —
//! `TAU_MODEL`, `TAU_WORK`, `MIN_FEATURE_SIZE` — are defined ONCE, in
//! `cad-primitives`, and re-exported here so consumers of the kernel trait
//! see the same values the kernel stack computes with. Do not redefine them.
//!
//! This module additionally holds the handful of contract-level derived
//! constants that `BooleanOptions`, `MockKernel`, and the consumer crates
//! use. It is deliberately small: the ~66 legacy constants that served the
//! deleted S-H clipping kernel (SSI_*, STITCH_*, WINDING_*, TAU_EXACT_MESH_*,
//! hole-fill heuristics, escalation factors, …) were removed on 2026-07-12
//! (design review F3) after a workspace-wide census showed zero live uses.
//! In particular `STITCH_ESCALATION_FACTORS` — the tolerance-escalation
//! anti-pattern P9/A15.6 prohibits — is gone; do not reintroduce it.
//!
//! Tolerance hierarchy (coarsest → finest):
//!   MIN_FEATURE_SIZE > TAU_MODEL > TAU_COINCIDENT > TAU_WORK > TAU_NORMALIZE

/// Canonical numeric tolerances — single definition in `cad-primitives`.
///
/// - `TAU_MODEL` (1e-7 m): model coincidence — welds, joins, coincidence
///   decisions.
/// - `MIN_FEATURE_SIZE` (1e-6 m): degeneracy floor — features below this may
///   be collapsed.
/// - `TAU_WORK` (1e-12 m): working precision floor — solver convergence,
///   degenerate-geometry detection, cross-product/normal-length checks.
pub use cad_primitives::{MIN_FEATURE_SIZE, TAU_MODEL, TAU_WORK};

/// Zero-length vector guard: 1e-15.
/// Used to prevent division by zero in vector normalization.
pub const TAU_NORMALIZE: f64 = 1e-15;

/// Squared TAU_NORMALIZE: 1e-30.
/// Used as denominator guard for cross-product magnitude checks and
/// line-plane intersection denominators. Expressed as a constant to avoid
/// runtime multiplication and clarify intent.
pub const TAU_NORMALIZE_SQ: f64 = 1e-30;

/// Point-on-surface coincidence tolerance: the central `TAU_EVAL` rounding
/// tier (1e-9). Finer than TAU_MODEL to avoid premature snapping, coarser
/// than TAU_WORK to tolerate floating-point accumulation. Feeds
/// `BooleanOptions`.
pub const TAU_COINCIDENT: f64 = cad_primitives::TAU_EVAL;

/// Re-export of the central f64 evaluation/rounding band (see
/// `cad_primitives::TAU_EVAL` for the full contract).
pub use cad_primitives::TAU_EVAL;

/// Relative weld factor for scale-adaptive tolerance: 1e-7.
/// tau_weld = (model_diagonal * TAU_WELD_FACTOR), clamped to TAU_WELD_MAX.
pub const TAU_WELD_FACTOR: f64 = 1e-7;

/// Maximum weld tolerance (clamp ceiling): 1e-4.
pub const TAU_WELD_MAX: f64 = 1e-4;

/// Fraction of tau_model used to derive tau_weld in BooleanOptions: 0.4.
/// tau_weld = tau_model * TAU_WELD_MODEL_RATIO.
/// Ensures weld tolerance is sub-model-tolerance to avoid premature vertex
/// merging while remaining coarser than working precision.
pub const TAU_WELD_MODEL_RATIO: f64 = 0.4;

/// Minimum tau_weld-to-tau_model ratio for BooleanOptions validation: 0.1.
/// Ensures the weld tolerance is at least 10% of model tolerance.
pub const TAU_WELD_MODEL_MIN_RATIO: f64 = 0.1;

/// Relative grid factor for f32 vertex deduplication in tessellation: 1e-5.
/// Applied as (max_abs * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN).
pub const TAU_TESS_GRID_FACTOR: f64 = 1e-5;

/// Minimum tessellation grid size: 1e-10.
pub const TAU_TESS_GRID_MIN: f64 = 1e-10;

/// Cosine threshold for face-normal similarity in mock kernel face merging: 0.9.
/// Used in MockKernel boolean simulation to detect duplicate/opposite face pairs.
pub const COS_MOCK_FACE_SIMILARITY: f64 = 0.9;

/// Dot-product threshold for axis-hint selection in plane basis construction: 0.9.
/// When the reference axis has |dot| > this with the normal, an alternate
/// axis is chosen.
pub const BASIS_AXIS_ALIGNMENT: f64 = 0.9;
