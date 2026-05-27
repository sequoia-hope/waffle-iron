//! Yang 2025 hybrid B-Rep / mesh boolean pipeline.
//!
//! ## Scope (aspirational)
//!
//! Implements the pipeline described in Yang et al. 2025, "A robust hybrid
//! Boolean operations method for mesh-and-surface hybrid models":
//!
//! - **Stage 0** (§4.5.5): Coplanar preprocessing. Detect coplanar face
//!   pairs pre-discretization; 2D-boolean their planes; replace overlap
//!   with a shared trimmed surface + identical meshes
//! - **Stage 1** (§4.1): Tessellate B-Rep faces with bijective mapping.
//!   Every mesh vertex maps uniquely to a B-Rep feature (vertex, edge with
//!   parameter, or face with (u, v))
//! - **Stage 2** (§4.2): Mesh boolean — delegate to `cherchi-rs`
//! - **Stage 3** (§4.3): SSI refinement — delegate per-pair to `ssi-rs`,
//!   refine mesh-approximate curves to surface-exact
//! - **Stage 4** (§4.4.1): Mesh updating — re-mesh along refined curves
//!   using CDT
//! - **Stage 5** (§4.4.2): Patch segmentation — flood-fill mesh patches
//!   bounded by intersection curves
//! - **Stage 6** (§4.4.2): B-Rep reassembly — emit output B-Rep from
//!   labeled patches + refined edges
//!
//! ## Current implementation status (PR-YR1)
//!
//! **None of the 6 stages are implemented yet.** PR-YR1 establishes:
//!
//! - The public types (`BRep`, `YangError`) and entry point (`boolean()`)
//! - End-to-end dispatch via a `MeshBoolean` backend (the sidecar today,
//!   the eventual native `cherchi-rs` implementation later)
//!
//! `boolean()` is currently degenerate: it extracts meshes from `BRep`
//! inputs, calls `backend.boolean()`, and wraps the result. Future PRs
//! (PR-YR2..N) insert the actual Yang stages.
//!
//! ## Input / output
//!
//! - Input: two B-Rep solids (this crate defines its own `BRep`; it does
//!   NOT import from `kernel-v2`). Caller is responsible for the
//!   conversion at the boundary.
//! - Output: one B-Rep solid (same type as input)
//! - Per-stage non-manifoldness is allowed internally; INPUT and OUTPUT
//!   are 2-manifold by contract. PR-YR1 does NOT detect non-manifold
//!   inputs yet (`YangError::NonManifoldInput` is defined but never
//!   returned); future PRs add detection.
//!
//! ## References
//!
//! - Yang et al. 2025 — `refs/text/yang2025_hybrid_boolean.txt`
//! - The pipeline IS the spec. Read the paper before working on this crate.

use std::error::Error;
use std::fmt;

pub use cad_primitives::BoolOp;
pub use cherchi_rs::{Mesh, MeshBoolean};

/// Boundary-Representation solid for yang-rs's boolean pipeline.
///
/// PR-YR1 ships a degenerate `BRep` that wraps a [`Mesh`]. Future PRs
/// add:
/// - Per-face analytical surface info (Plane / Cylinder / Sphere / ...)
/// - Per-edge analytical curve info
/// - `TessellationMap` (the bijection from mesh elements to B-Rep features)
///
/// External consumers (eventually `kernel-v2`) target this type as the
/// boolean input/output. Adding fields later is non-breaking: consumers
/// construct via [`BRep::from_mesh`] and inspect via [`BRep::as_mesh`].
#[derive(Clone, Debug, PartialEq)]
pub struct BRep {
    mesh: Mesh,
    // Future PRs: faces: Vec<Face>, edges: Vec<Edge>, tessellation: Option<TessellationMap>
}

impl BRep {
    /// Construct from a `Mesh`. Currently the only constructor; future
    /// PRs will add analytical-surface-aware builders.
    pub fn from_mesh(mesh: Mesh) -> Self {
        Self { mesh }
    }

    /// Borrow the underlying mesh.
    pub fn as_mesh(&self) -> &Mesh {
        &self.mesh
    }

    /// Consume into the underlying mesh.
    pub fn into_mesh(self) -> Mesh {
        self.mesh
    }

    pub fn num_verts(&self) -> usize {
        self.mesh.num_verts()
    }

    pub fn num_tris(&self) -> usize {
        self.mesh.num_tris()
    }
}

/// Errors from the yang-rs pipeline.
#[derive(Debug)]
pub enum YangError {
    /// Input is not 2-manifold. **Not yet detected in PR-YR1**;
    /// defined for forward compatibility.
    NonManifoldInput,
    /// Reassembly would produce a non-2-manifold result. **Not yet
    /// returned in PR-YR1**; Stages 5/6 will surface this.
    NonManifoldOutput,
    /// The mesh boolean backend (sidecar or native) failed.
    MeshBooleanFailed(Box<dyn Error + Send + Sync>),
}

impl fmt::Display for YangError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonManifoldInput => write!(f, "yang-rs: input B-Rep is not 2-manifold"),
            Self::NonManifoldOutput => {
                write!(f, "yang-rs: reassembled output would be non-2-manifold")
            }
            Self::MeshBooleanFailed(source) => {
                write!(f, "yang-rs: mesh boolean backend failed: {source}")
            }
        }
    }
}

impl Error for YangError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::MeshBooleanFailed(source) => Some(&**source),
            _ => None,
        }
    }
}

/// Boolean operation on two B-Rep solids via a `MeshBoolean` backend.
///
/// PR-YR1: extracts meshes from the inputs, dispatches to the backend,
/// wraps the result in a fresh `BRep`. Future PRs insert Stages 0-6
/// around the backend call.
///
/// The `backend` is `&dyn MeshBoolean` so consumers can swap sidecar
/// (today) and the eventual native `cherchi-rs` (someday) without
/// changing call sites.
pub fn boolean(
    _a: &BRep,
    _b: &BRep,
    _op: BoolOp,
    _backend: &dyn MeshBoolean,
) -> Result<BRep, YangError> {
    // RED stub
    Err(YangError::MeshBooleanFailed(
        "RED stub: not implemented yet".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_primitives::Point3;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    fn sample_mesh() -> Mesh {
        Mesh::new(
            vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
            vec![[0, 1, 2]],
        )
    }

    /// Mock backend for unit tests (no subprocess required).
    struct MockBackend {
        result: Result<Mesh, &'static str>,
    }

    impl MockBackend {
        fn ok(mesh: Mesh) -> Self {
            Self { result: Ok(mesh) }
        }
        fn err() -> Self {
            Self {
                result: Err("mock backend failure"),
            }
        }
    }

    impl MeshBoolean for MockBackend {
        fn boolean(
            &self,
            _a: &Mesh,
            _b: &Mesh,
            _op: BoolOp,
        ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
            self.result
                .as_ref()
                .map(|m| m.clone())
                .map_err(|s| -> Box<dyn Error + Send + Sync> { Box::from(*s) })
        }
    }

    // ----- Group 1: BRep construction + accessors -----

    #[test]
    fn brep_from_mesh_as_mesh_round_trip() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.as_mesh(), &m);
    }

    #[test]
    fn brep_into_mesh_returns_wrapped() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.into_mesh(), m);
    }

    #[test]
    fn brep_counts_delegate_to_mesh() {
        let m = sample_mesh();
        let b = BRep::from_mesh(m.clone());
        assert_eq!(b.num_verts(), m.num_verts());
        assert_eq!(b.num_tris(), m.num_tris());
    }

    // ----- Group 2: YangError contract -----

    #[test]
    fn yang_error_display_non_empty() {
        for e in [
            YangError::NonManifoldInput,
            YangError::NonManifoldOutput,
            YangError::MeshBooleanFailed(Box::from("test")),
        ] {
            let msg = format!("{}", e);
            assert!(!msg.is_empty(), "empty Display for {e:?}");
        }
    }

    #[test]
    fn yang_error_source_propagates_backend_error() {
        let inner: Box<dyn Error + Send + Sync> = Box::from("inner failure");
        let e = YangError::MeshBooleanFailed(inner);
        let src = e.source().expect("source should be Some for MeshBooleanFailed");
        assert_eq!(src.to_string(), "inner failure");
    }

    // ----- Group 3: boolean() dispatch via mock -----

    #[test]
    fn boolean_with_ok_backend_returns_ok() {
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let mock = MockBackend::ok(Mesh::empty());
        let result = boolean(&a, &b, BoolOp::Union, &mock).unwrap();
        assert_eq!(result.num_verts(), 0);
        assert_eq!(result.num_tris(), 0);
    }

    #[test]
    fn boolean_with_err_backend_returns_mesh_boolean_failed() {
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let mock = MockBackend::err();
        let result = boolean(&a, &b, BoolOp::Union, &mock);
        match result {
            Err(YangError::MeshBooleanFailed(_)) => {}
            other => panic!("expected MeshBooleanFailed, got {:?}", other),
        }
    }

    #[test]
    fn boolean_dispatches_all_four_ops() {
        let a = BRep::from_mesh(sample_mesh());
        let b = BRep::from_mesh(sample_mesh());
        let mock = MockBackend::ok(Mesh::empty());
        for op in [
            BoolOp::Union,
            BoolOp::Intersect,
            BoolOp::Subtract,
            BoolOp::Xor,
        ] {
            assert!(
                boolean(&a, &b, op, &mock).is_ok(),
                "op {op:?} should succeed"
            );
        }
    }
}
