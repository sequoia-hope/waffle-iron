//! Cross-backend mesh-boolean trait.
//!
//! Implementations:
//! - `cherchi_sidecar_rs::SidecarBoolean` (today; subprocess to C++
//!   `mesh_booleans` binary; **not WASM-compatible**)
//! - Native cherchi-rs arrangement-based boolean (future; pure Rust;
//!   WASM-portable; gated on Stages 2-4 of the arrangement port +
//!   LGPL-or-RBig decision for intersection-point representation)
//!
//! Both implementations satisfy the same contract. Consumers
//! (yang-rs, eventually) should target `dyn MeshBoolean` so the
//! backend can be swapped without changing call sites.

use cad_primitives::BoolOp;
use std::error::Error;

use crate::Mesh;

/// A backend that can compute mesh booleans.
///
/// The boxed-error return allows each backend its own concrete error
/// type without forcing this trait crate to know about subprocess
/// errors, file I/O, LGPL-predicate-failures, etc. Callers that need
/// specific error handling should downcast.
///
/// The trait is object-safe — `Box<dyn MeshBoolean>` works.
pub trait MeshBoolean {
    fn boolean(&self, a: &Mesh, b: &Mesh, op: BoolOp)
        -> Result<Mesh, Box<dyn Error + Send + Sync>>;

    /// Produce the full Stage-2 [`LabeledArrangement`](crate::labeled_arrangement::LabeledArrangement)
    /// for `a` and `b`: the exact mesh arrangement plus per-triangle
    /// surface/inside/patch labels.
    ///
    /// Default impl errors (`NotSupported`) so existing backends that only
    /// implement [`MeshBoolean::boolean`] compile unchanged. Producers that
    /// can surface labels (e.g. the patched `cherchi-sidecar-rs`) override it.
    fn labeled_arrangement(
        &self,
        _a: &Mesh,
        _b: &Mesh,
    ) -> Result<
        crate::labeled_arrangement::LabeledArrangement,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        Err("labeled_arrangement not supported by this backend".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopBoolean;

    impl MeshBoolean for NoopBoolean {
        fn boolean(
            &self,
            _a: &Mesh,
            _b: &Mesh,
            _op: BoolOp,
        ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
            Ok(Mesh::empty())
        }
    }

    #[test]
    fn trait_is_object_safe() {
        let backend: Box<dyn MeshBoolean> = Box::new(NoopBoolean);
        let m = Mesh::empty();
        let result = backend.boolean(&m, &m, BoolOp::Union).unwrap();
        assert_eq!(result.num_verts(), 0);
    }
}
