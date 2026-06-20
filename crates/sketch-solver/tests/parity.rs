//! Parity harness: compares the clean-room solver against libslvs.
//!
//! Feature-gated behind `libslvs-oracle`. Not compiled by default.
//! Populated in PR-SS1d; deleted at cutover when the `libslvs-oracle`
//! feature is removed.
//!
//! See `specs/clean_room_constraint_solver.md` §"Parity harness".

#![cfg(feature = "libslvs-oracle")]

// PR-SS1d will populate this with:
//   - Hand-curated degenerate fixtures (zero-length line, r=0 circle, etc.)
//   - 20 random sketches from the assay corpus (seed 42)
//   - Position agreement assertions (1e-6)
//   - SolveStatus variant agreement
//   - DOF agreement for UnderConstrained cases
