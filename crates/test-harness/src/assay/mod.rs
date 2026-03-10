//! Assay — property-based testing module for boolean operations.
//!
//! Provides composable proptest strategies for generating CAD scenarios,
//! oracle-based property checkers, determinism verification, regression
//! corpus management, and coverage tracking.
//!
//! # Strategy Levels
//!
//! - Level 0: Dimension ranges
//! - Level 1: Sketch profiles (rect, circle)
//! - Level 2: Solid body specs (profile + extrude)
//! - Level 3: Boolean scenarios (two bodies + operation)
//! - Level 4: Degeneracy families (coplanar, coincident, tangential)
//! - Level 5: Chains (2-5 sequential operations)

pub mod catalog;
pub mod corpus;
pub mod coverage;
pub mod determinism;
pub mod gen;
pub mod properties;
pub mod properties_v2;
pub mod randomized_runner;
pub mod regions;
pub mod runner;
pub mod scoring;
pub mod strategies;
pub mod strategies_v2;
