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

#[cfg(feature = "indirect-predicates")]
pub use aux_structure::{
    group_constraint_segments, group_intersection_points, ConstraintSegment, TriangleAuxPoints,
    TypedPoint,
};
pub use fast_trimesh::{FastTrimesh, FastTrimeshError, Plane};
pub use intersection_detection::detect_intersecting_pairs;
#[cfg(feature = "indirect-predicates")]
pub use intersection_points::{
    classify_all, classify_pair, DeferReason, IntersectionVertex, PairClassification,
};
#[cfg(feature = "indirect-predicates")]
pub use retriangulate::{split_single_triangle, RetriangulateError};
pub use tree::{Node, Tree};
