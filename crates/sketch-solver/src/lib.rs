// Module selection: legacy (slvs) or clean (levenberg-marquardt) solver.
// The legacy path is default through PR-SS1c; PR-SS1d removes it.
//
// Both paths expose the same module names (entity_mapping, solver, etc.) so
// that crate-internal references resolve identically within each cfg branch.

#[cfg(feature = "legacy-solver")]
pub mod entity_mapping;

#[cfg(not(feature = "legacy-solver"))]
#[path = "clean/entity_mapping.rs"]
pub mod entity_mapping;

#[cfg(feature = "legacy-solver")]
pub mod constraint_mapping;

#[cfg(feature = "legacy-solver")]
pub mod status;

#[cfg(feature = "legacy-solver")]
pub mod solver;

#[cfg(not(feature = "legacy-solver"))]
#[path = "clean/solver.rs"]
pub mod solver;

pub mod profiles;
pub mod types;

pub use profiles::extract_profiles;
pub use solver::solve_sketch;
pub use types::*;
