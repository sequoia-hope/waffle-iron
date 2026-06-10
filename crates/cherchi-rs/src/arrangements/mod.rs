//! Mesh arrangement data structures and algorithms.
//!
//! Cherchi 2020 §4–§5. PR-CR11/CR12a/CR12b/CR12c ship the data-structure
//! layer (`FastTrimesh` + `Tree`); AR1–AR3b ship the arrangement itself.
//!
//! Pure Rust since PR-CR-M7c: every implicit-point (LPI/TPI) predicate
//! routes through the clean-room `crate::predicates::indirect` module, so
//! the whole arrangement compiles unconditionally — including for
//! wasm32-unknown-unknown. The LGPL C++ FFI shim
//! (`indirect-predicates-sidecar-rs`) survives only as a dev-dependency
//! differential oracle (`tests/indirect_*_parity.rs` + in-src `#[cfg(test)]`
//! exactness oracles).

pub mod aux_structure;
pub mod enforce;
pub mod fast_trimesh;
pub(crate) mod gp_dispatch;
pub mod intersection_detection;
pub mod intersection_points;
pub mod retriangulate;
pub mod soup;
pub mod tree;

pub use aux_structure::{
    group_constraint_segments, group_intersection_points, ConstraintSegment,
    ConstraintSegmentError, TriangleAuxPoints, TypedPoint,
};
pub use enforce::{enforce_constraint_segments, enforce_constraints, EnforceError, SegmentSpec};
pub use fast_trimesh::{FastTrimesh, FastTrimeshError, Plane};
pub use intersection_detection::detect_intersecting_pairs;
pub use intersection_points::{
    classify_all, classify_pair, DeferReason, IntersectionVertex, PairClassification,
};
pub use retriangulate::{split_single_triangle, RetriangulateError};
pub use soup::{mesh_arrangement, ArrangementError, ArrangementSoup, DuplTriInfo, Label};
pub use tree::{Node, Tree};
