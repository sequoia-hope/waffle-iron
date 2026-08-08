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
pub mod boolean;
/// Native five-axiom input census (the `mesh_booleans_inputcheck` analog,
/// localizing) — diagnostic oracle for the M8 Stage-0 operand contract.
pub mod inputcheck;
pub mod labeled_arrangement;
/// Boolean labeling (Cherchi 2022 §5) over the AR3b arrangement soup
/// (M6 BL* slices). Pure Rust since PR-CR-M7c (clean-room predicates).
pub mod labeling;
pub mod mesh;
pub mod predicates;
pub mod processing;
pub mod triangulation;

pub use arrangements::{mesh_arrangement, ArrangementError, ArrangementSoup};
pub use boolean::MeshBoolean;
pub use inputcheck::{census, detect_improper_contacts, ImproperContacts, NativeInputCheck};
pub use labeled_arrangement::{InputId, LabeledArrangement};
pub use labeling::{native_labeled_arrangement, NativeBoolean, NativeBooleanError};
pub use mesh::Mesh;
pub use triangulation::{
    cdt_polygon_with_holes, cdt_polygon_with_holes_keep_interior, cdt_polygon_with_holes_refined,
    cdt_polygon_with_holes_refined_seeded, cdt_with_interior_constraints, CdtError,
};
