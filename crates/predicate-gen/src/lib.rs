//! predicate-gen — code generator for cherchi-rs's clean-room indirect
//! predicates (kernel-rewrite milestone M7).
//!
//! Dev tooling, OUTSIDE the kernel crate layering: nothing ships this
//! crate; it emits checked-in Rust source. No dependencies.
//!
//! Pipeline:
//!
//! 1. [`ir`] — a small SSA expression IR (variables, `+`, `−`, `×`; no
//!    division — denominator signs are handled by Attene's parity rule).
//! 2. [`fpg`] — Meyer-Pion 2008 forward error analysis: propagate
//!    `(bound, error)` through the expression assuming `|v| ≤ 1`,
//!    rounding every constant conservatively UP; the final error is the
//!    semi-static filter constant `δ(1)` of Attene 2025 Appendix A.
//! 3. [`codegen`] — straight-line Rust emission (filtered f64 + exact
//!    `dashu::rational::RBig` over the same polynomial).
//! 4. [`orient3d`] — the concrete LPI/TPI lambdas and the 14 canonical
//!    `orient3d` instances; [`orient3d::generate_file`] assembles
//!    `crates/cherchi-rs/src/predicates/indirect/generated.rs`.
//!
//! Clean-room sources (papers only — the LGPL C++ implementation was
//! not consulted):
//!
//! - `refs/text/attene-predicates.txt` (Attene 2025)
//! - `refs/text/mesh_arrangement.txt` §4.2.2 (Cherchi 2020)
//! - `refs/text/meyer_pion2008_fpg.txt` (Meyer & Pion 2008)

pub mod codegen;
pub mod fpg;
pub mod ir;
pub mod orient3d;

/// Path of the emitted file, relative to this crate's manifest dir.
pub const OUTPUT_RELATIVE: &str = "../cherchi-rs/src/predicates/indirect/generated.rs";
