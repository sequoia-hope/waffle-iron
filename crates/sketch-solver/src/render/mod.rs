//! Feature-gated render module for visual verification of solver output.
//!
//! Produces SVG and PNG representations of solved sketches with:
//! - Grid background (10mm minor, 100mm major)
//! - Entity rendering (lines, circles, arcs, points)
//! - DOF status coloring (green/amber/red)
//! - Constraint annotation badges
//! - Profile highlighting (closed profiles in light blue)

pub mod png;
pub mod svg;

pub use png::render_sketch_png;
pub use svg::render_sketch_svg;
