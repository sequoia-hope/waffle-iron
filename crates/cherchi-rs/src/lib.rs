//! Pure-Rust port of Cherchi 2020 + 2022 mesh booleans.
//!
//! ## Scope
//!
//! Takes two triangle soups in, produces a labeled triangle soup out (where
//! each triangle is labeled inside/outside relative to each input mesh).
//! Boolean ops (union / intersect / subtract) are derived from labels.
//!
//! Does NOT do B-Rep work — analytical surfaces, edge curves, face geometry
//! are entirely outside this crate's concern. That's `yang-rs`'s job.
//!
//! ## References
//!
//! - Cherchi et al. 2020, "Fast and Robust Mesh Arrangements using
//!   Floating-point Arithmetic" — `refs/cherchi2020.pdf` (foundation:
//!   indirect predicates + arrangement)
//! - Cherchi et al. 2022, "Interactive and Robust Mesh Booleans" —
//!   `refs/cherchi2022_interactive_robust_mesh_booleans.pdf` (boolean
//!   labels via ray-cast + result construction)
//! - Upstream C++ (MIT):
//!   - `github.com/gcherchi/FastAndRobustMeshArrangements`
//!   - `github.com/gcherchi/InteractiveAndRobustMeshBooleans`
//!
//! ## Load-bearing oracle
//!
//! Every commit to this crate runs a differential diff against the upstream
//! C++ sidecar. Reference parity is the correctness criterion — internal
//! consistency is not sufficient. If the Rust port diverges from the C++
//! reference, the Rust port is wrong (until proven otherwise on a case-by-
//! case basis with paper-cited justification).

pub mod arrangements;
pub mod predicates;
pub mod processing;
