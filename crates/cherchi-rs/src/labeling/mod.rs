//! Boolean labeling — Cherchi 2022 §5 (M6 `PR-CR-BL*` slices).
//!
//! Consumes the AR3b global conforming soup (`arrangements::soup`) and
//! produces the per-patch / per-input labeling that `LabeledArrangement`
//! requires: patch flood-fill (BL1, this module), ray-cast in/out (BL2),
//! and the native `MeshBoolean` assembly (BL3).

#[cfg(test)]
mod adversary_tests;
pub mod inside_out;
#[cfg(test)]
mod inside_out_adversary_tests;
pub mod native;
pub mod octree;
pub mod patches;

pub use inside_out::{compute_inside_out, InsideOutError, Ray};
pub use native::{native_labeled_arrangement, NativeBoolean, NativeBooleanError};
pub use octree::TriOctree;
pub use patches::{compute_all_patches, PatchError, Patches};
