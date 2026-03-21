//! Pure-Rust constraint solver core.
//!
//! Module layout:
//! - `types`: Typed index wrappers, solver options/outcome
//! - `params`: Parameter vector layout (entity → param indices)
//! - `constraint`: Constraint trait + internal enum with typed indices
//! - `builder`: SketchConstraint → ConstraintImpl dispatch
//! - `lm`: Levenberg-Marquardt solver
//! - `rank`: SVD rank analysis, DOF, conflict detection
//! - `status`: Solver outcome → SolveStatus classification

pub mod builder;
pub mod constraint;
pub mod error;
pub mod graph;
pub mod lm;
pub mod params;
pub mod rank;
pub mod status;
pub mod types;
