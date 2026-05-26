//! Mesh arrangement data structures and algorithms.
//!
//! Cherchi 2020 §4–§5. PR-CR11 ships the data-structure layer
//! (`FastTrimesh`); arrangement algorithm itself lands later.

pub mod fast_trimesh;

pub use fast_trimesh::{FastTrimesh, FastTrimeshError, Plane};
