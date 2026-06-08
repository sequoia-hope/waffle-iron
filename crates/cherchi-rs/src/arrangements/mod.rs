//! Mesh arrangement data structures and algorithms.
//!
//! Cherchi 2020 §4–§5. PR-CR11/CR12a/CR12b/CR12c ship the data-structure
//! layer (`FastTrimesh` + `Tree`); arrangement algorithm itself lands later.

#[cfg(feature = "indirect-predicates")]
pub mod aux_structure;
#[cfg(feature = "indirect-predicates")]
pub mod enforce;
pub mod fast_trimesh;
#[cfg(feature = "indirect-predicates")]
pub(crate) mod gp_dispatch;
pub mod intersection_detection;
#[cfg(feature = "indirect-predicates")]
pub mod intersection_points;
#[cfg(feature = "indirect-predicates")]
pub mod retriangulate;
#[cfg(feature = "indirect-predicates")]
pub mod soup;
pub mod tree;

#[cfg(feature = "indirect-predicates")]
pub use aux_structure::{
    group_constraint_segments, group_intersection_points, ConstraintSegment, TriangleAuxPoints,
    TypedPoint,
};
#[cfg(feature = "indirect-predicates")]
pub use enforce::{enforce_constraint_segments, enforce_constraints, EnforceError, SegmentSpec};
pub use fast_trimesh::{FastTrimesh, FastTrimeshError, Plane};
pub use intersection_detection::detect_intersecting_pairs;
#[cfg(feature = "indirect-predicates")]
pub use intersection_points::{
    classify_all, classify_pair, DeferReason, IntersectionVertex, PairClassification,
};
#[cfg(feature = "indirect-predicates")]
pub use retriangulate::{split_single_triangle, RetriangulateError};
#[cfg(feature = "indirect-predicates")]
pub use soup::{mesh_arrangement, ArrangementError, ArrangementSoup, Label};
pub use tree::{Node, Tree};
