//! Geometric predicates ported from Cherchi 2020 + 2022 (and cinolib).
//!
//! All predicates here use Shewchuk-style exact arithmetic (via the
//! `geometry-predicates` crate) or `dashu`-backed rationals where exact
//! integer arithmetic is required. None of these predicates use tolerance
//! thresholds — they are EXACT.
//!
//! ## Submodules
//!
//! - `collinearity`: 3D collinearity tests (PR-CR1)
//! - `indirect`: clean-room pure-Rust indirect predicates over implicit
//!   (LPI/TPI) points — Attene 2025 + Cherchi 2020 §4.2.2 (PR-CR-M7a).
//!   Ungated, WASM-clean; replaces the FFI sidecar tier by tier (M7).

pub mod collinearity;
pub mod indirect;
pub mod orient;
pub mod orientation;
pub mod point_in_triangle;
pub mod segment_triangle;
pub mod triangle_intersect;
pub mod triangle_pair;

pub use collinearity::{point_strictly_inside_segment_3d, points_are_collinear_3d};
pub use orient::{orient2d, orient3d, Sign};
pub use orientation::{max_component_in_triangle_normal, Axis};
pub use point_in_triangle::{point_in_triangle_3d, PointLocation};
pub use segment_triangle::{segment_intersects_triangle_3d, SegmentTriangleIntersection};
pub use triangle_intersect::{triangle_intersects_triangle_3d, TriangleIntersection};
pub use triangle_pair::triangles_are_coplanar;
