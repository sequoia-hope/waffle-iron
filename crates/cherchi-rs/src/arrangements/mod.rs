//! Mesh arrangement data structures and algorithms.
//!
//! Cherchi 2020 §4–§5. PR-CR11/CR12a/CR12b/CR12c ship the data-structure
//! layer (`FastTrimesh` + `Tree`); arrangement algorithm itself lands later.

#[cfg(feature = "indirect-predicates")]
pub mod aux_structure;
pub mod fast_trimesh;
pub mod intersection_detection;
#[cfg(feature = "indirect-predicates")]
pub mod intersection_points;
#[cfg(feature = "indirect-predicates")]
pub mod retriangulate;
pub mod tree;

pub use fast_trimesh::{FastTrimesh, FastTrimeshError, Plane};
pub use intersection_detection::detect_intersecting_pairs;
#[cfg(feature = "indirect-predicates")]
pub use intersection_points::{
    classify_all, classify_pair, DeferReason, IntersectionVertex, PairClassification,
};
pub use tree::{Node, Tree};
