//! Shared Piegl & Tiller B-spline and NURBS curve/surface math.
//!
//! Depends only on [`cad_primitives`] (`Point3`, `Vector3`) and sits below
//! `waffle-types`, `cherchi-rs`, `ssi-rs`, `yang-rs`, and `kernel-v2` so both
//! `kernel-v2` Loft (`specs/loft.md`) and `yang-rs` Milestone F5 share a single
//! implementation.
//!
//! All items live in [`bspline`] and are also re-exported at the crate root.

pub mod bspline;

pub use bspline::*;
