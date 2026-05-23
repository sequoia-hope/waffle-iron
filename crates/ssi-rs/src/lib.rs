//! Analytical surface-surface intersection (SSI) solvers.
//!
//! ## Scope
//!
//! Closed-form intersection curves between pairs of analytical surfaces:
//! plane, cylinder, cone, sphere, torus. Used by `yang-rs` Stage 3
//! (refinement of mesh-approximate intersection curves to surface-exact).
//!
//! Each solver answers: given surface A and surface B, what are the
//! analytical intersection curves (lines, circles, ellipses, conics, or
//! general parameterized curves) on both surfaces?
//!
//! ## References
//!
//! - Patrikalakis & Maekawa 2002, "Shape Interrogation for Computer Aided
//!   Design and Manufacturing," Chapter 5 (SSI algorithms)
//! - Yang et al. 2025, §4.3 (SSI in the hybrid boolean pipeline)
//!
//! ## Solver matrix (target)
//!
//! Symmetric: pair (A, B) === pair (B, A).
//!
//! | A \ B    | Plane | Cylinder | Cone | Sphere | Torus |
//! |----------|-------|----------|------|--------|-------|
//! | Plane    | ✓     |          |      |        |       |
//! | Cylinder | ✓     | ✓        |      |        |       |
//! | Cone     | ✓     | ✓        | ✓    |        |       |
//! | Sphere   | ✓     | ✓        | ✓    | ✓      |       |
//! | Torus    | ✓     | ✓        | ✓    | ✓      | ✓     |
//!
//! 15 unique solvers total.

// Skeleton — content fills in during Phase 3.
