//! Boolean labeling — Cherchi 2022 §5 (M6 `PR-CR-BL*` slices).
//!
//! Consumes the AR3b global conforming soup (`arrangements::soup`) and
//! produces the per-patch / per-input labeling that `LabeledArrangement`
//! requires: patch flood-fill (BL1, this module), ray-cast in/out (BL2),
//! and the native `MeshBoolean` assembly (BL3).

pub mod patches;

pub use patches::{compute_all_patches, PatchError, Patches};
