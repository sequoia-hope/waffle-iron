//! Preprocessing helpers for Cherchi 2020's exact-arithmetic pipeline.
//!
//! Functions here are not predicates per se — they're the coordinate
//! conditioning steps that bring f64 inputs into ranges where the exact
//! predicates can operate reliably.
//!
//! ## Submodules
//!
//! - `multiplier`: power-of-2 scaling factor computation (PR-CR2)
//!
//! Future modules (one PR each):
//! - `multiply_coordinates`: apply the scaling factor to a coord array
//! - `approximate_coordinates`: inverse scaling for output
//! - `dedup`: remove degenerate and duplicated triangles

pub mod multiplier;

pub use multiplier::compute_multiplier;
