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
//!
//! Future modules (one PR each):
//! - `orientation`: orient2d/orient3d wrappers + indirect variants
//! - `intersection`: triangle-triangle, segment-triangle, etc.

pub mod collinearity;
pub mod orient;
pub mod orientation;
pub mod point_in_triangle;
pub mod triangle_pair;

pub use collinearity::points_are_collinear_3d;
pub use orient::{orient3d, Sign};
pub use orientation::{max_component_in_triangle_normal, Axis};
pub use point_in_triangle::{point_in_triangle_3d, PointLocation};
pub use triangle_pair::triangles_are_coplanar;
