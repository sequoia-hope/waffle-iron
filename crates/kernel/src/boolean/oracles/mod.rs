//! Per-stage diagnostic oracles consumed by `pipeline_oracles::run_pipeline_oracles`.
//!
//! Each submodule implements one `StageOracle` for a specific Yang 2025
//! pipeline stage. Oracles are diagnostic-only — they observe a
//! `PipelineState` snapshot and return a verdict; they never mutate state
//! and have no production side effects.
//!
//! Refs: Yang 2025 §4 (pipeline); Cherchi 2022 §3-4 (arrangement) + §5
//! (in/out). See `crates/kernel/src/boolean/pipeline_oracles.rs` for the
//! harness API.

pub(crate) mod arrangement_wellformed;
pub(crate) mod coplanar_identical;
pub(crate) mod label_consistency;
