//! Half-edge B-Rep topology data structures and Euler operators.
//!
//! References:
//! - [#16] Mantyla, "An Introduction to Solid Modeling" (Euler operators)
//! - [#33] Stroud, "Boundary Representation Modelling Techniques", Ch.4

pub mod arena;
pub mod euler_ops;
pub mod half_edge;
pub mod validate;
