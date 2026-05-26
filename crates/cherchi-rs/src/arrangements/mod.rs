//! Mesh arrangement data structures and algorithms.
//!
//! Cherchi 2020 §4–§5. PR-CR11/CR12a/CR12b/CR12c ship the data-structure
//! layer (`FastTrimesh` + `Tree`); arrangement algorithm itself lands later.

pub mod fast_trimesh;
pub mod tree;

pub use fast_trimesh::{FastTrimesh, FastTrimeshError, Plane};
pub use tree::{Node, Tree};
