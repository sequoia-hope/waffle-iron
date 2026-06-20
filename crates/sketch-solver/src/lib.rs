pub mod constraint_mapping;
pub mod entity_mapping;
pub mod profiles;
pub mod solver;
pub mod types;

// Dev-only legacy libslvs solver for parity validation.
// Never compiled in production or WASM builds.
#[cfg(feature = "legacy-oracle")]
pub mod legacy;

pub use profiles::extract_profiles;
pub use solver::solve_sketch;
pub use types::*;
