//! Yang 2025 hybrid B-Rep / mesh boolean pipeline.
//!
//! ## Scope (aspirational)
//!
//! Implements the pipeline described in Yang et al. 2025, "A robust hybrid
//! Boolean operations method for mesh-and-surface hybrid models":
//!
//! - **Stage 0** (§4.5.5): Coplanar preprocessing
//! - **Stage 1** (§4.1): Bijective tessellation — PR-YR2: planar B-Reps;
//!   PR-YR7: cylinder; PR-YR12: sphere (Cone still rejects loudly)
//! - **Stage 2** (§4.2): Mesh boolean — delegate to `cherchi-rs`
//! - **Stage 3** (§4.3): SSI refinement — delegate to `ssi-rs`
//! - **Stage 4** (§4.4.1): Mesh updating — RELOCATION of intersection crossings
//!   onto the exact curve (+ §4.5.3 reversed-point sweep), watertightness
//!   inherited from the mesh boolean. The paper's CDT remesh / split-merge-insert
//!   is **NOT implemented** (deviation N2 in `docs/yang_deviations.md`); the
//!   sidecar's trimmed mesh is trusted and `check_watertight_2manifold` gates the
//!   output. Likewise §4.5.4 illegal-self-intersection removal is **NOT
//!   implemented** (deviation N6, roadmap-tracked).
//! - **Stage 5** (§4.4.2): Patch segmentation (flood-fill)
//! - **Stage 6** (§4.4.2): B-Rep reassembly
//!
//! ## Current implementation status (PR-YR5)
//!
//! - **Stage 1 PLANAR** (PR-YR2): `BRep::new(verts, edges, faces)`
//!   fan-triangulates each planar face from its first vertex; produces
//!   a 1:1 bijection (no Steiner points). Convex faces only; no inner
//!   loops; `Surface::Plane` only.
//! - **`boolean()` vertex provenance** (PR-YR3): every output mesh
//!   vertex is spatially matched against input A then B (within
//!   [`MATCH_TOLERANCE`]). On match, the corresponding input's
//!   `TessellationSource` is copied; unmatched verts get
//!   `TessellationSource::Intersection`.
//! - **`boolean()` triangle attribution** (PR-YR4): every output
//!   triangle is attributed to an input `(InputId, face_idx)` via
//!   majority-vote (≥2 of 3) over the vertices' provenance.
//!   Accessible via [`BRep::triangle_attribution`].
//! - **`boolean()` topology reconstruction** (PR-YR5): output `BRep`
//!   gets non-empty `vertices` (1:1 with mesh), `edges`, and `faces`
//!   via patch flood-fill on triangle attribution + boundary cycle
//!   recovery + surface inheritance from input faces.
//!   None-attributed (cut surface) triangles are intentionally
//!   skipped — output is a "kept-portions skeleton."
//! - **`BRep::from_mesh()` degenerate path** (PR-YR1 compat): empty
//!   topology; all-`Unknown` TessellationMap; empty
//!   TriangleAttributionMap.
//!
//! **Honest framing**: PR-YR3 + PR-YR4 + PR-YR5 are NOT real Yang
//! Stage 5/6. Real Stage 5/6 needs per-triangle labels from Stage 2's
//! arrangement which the C++ sidecar doesn't expose. The current
//! pipeline is a sidecar-feasible substitute.
//!
//! **PR-YR5 output is intentionally NOT 2-manifold** (rule-4
//! deviation): faces cover input-derived ("kept") portions only.
//! Cut-surface faces (`None`-attributed triangles → new BRepFaces with
//! reconstructed surfaces) are PR-YR6, which also re-enables the
//! 2-manifold contract.
//!
//! Banked for future PRs:
//! - PR-YR2b: ear-cutting for non-convex faces
//! - PR-YR2c: inner loops (holes) — currently → `NonManifoldOutput`
//! - PR-YR2d: curved surfaces (`Surface::Cylinder`, `Sphere`, NURBS)
//! - PR-YR2e: Steiner points + dε tolerance
//! - PR-YR2f: CDT at shared edges
//! - PR-YR4b: precomputed vertex→edge / edge→face incidence indices
//! - PR-YR5b: edge deduplication across faces (each face owns its edges in v1)
//! - PR-YR5c: inner-loop / hole support in patch boundary recovery
//! - PR-YR6: cut-surface face generation + 2-manifold validation
//! - PR-YR7+: edge curve recovery beyond `Curve::LineSegment`
//! - Real Stage 5/6: gated on labeled arrangement output
//!
//! ## Input / output
//!
//! - Input: two B-Rep solids (`BRep`)
//! - Output: one B-Rep solid
//! - Non-manifold detection is **not yet implemented** in PR-YR2.
//!
//! ## References
//!
//! - Yang et al. 2025 — `refs/text/yang2025_hybrid_boolean.txt`

// Stage 0 (Yang §4.5.5) coplanar-overlay geometric engine — M8 slice a
// (PR-YR25). NOT yet wired into `boolean()`; that's M8 slice b.
pub mod coplanar_overlay;
mod stage0;
// N2 increment 2: the §4.1.2 / Fig 6 per-triangle `d(T)` bound + its pinned
// parametric embedding. NOT yet wired into `stage4_relocate_and_correct`;
// that is N2-3. Spec: `specs/n2_stage4_dt_recompute.md`.
pub mod stage4_dt;
// N2 increment 1: the §4.4.1 mesh-updating primitive (Fig 11 split/merge/insert
// + interior-constraint CDT). NOT yet wired into `stage4_relocate_and_correct`;
// that is N2-3. Spec: `specs/n2_stage4_mesh_updating.md`.
mod boolean;
mod brep;
pub use boolean::boolean;
pub(crate) use boolean::*;
mod errors;
mod geom;
mod stage1_tessellate;
mod stage3_ssi;
pub(crate) use stage3_ssi::*;
mod stage4_correct;
mod stage5_topology;
pub(crate) use stage5_topology::*;
mod stage4_relocate;
pub use brep::{
    BRep, BRepEdge, BRepFace, BRepVertex, InputId, TessellationMap, TessellationSource,
    TriangleAttribution, TriangleAttributionMap, MATCH_TOLERANCE,
};
pub use stage1_tessellate::tessellate_torus_patch;
pub(crate) use stage1_tessellate::*;
pub(crate) use stage4_correct::*;
pub(crate) use stage4_relocate::*;
pub mod stage4_update;
pub use errors::{SsiRefinementError, Stage4InvalidReason, YangError};
pub(crate) use geom::{ellipse_param, ellipse_point, ellipse_tangent, surface_normal_at};
pub use geom::{hyperbola_point, parabola_point, signed_distance_to_surface, Curve, Surface};

pub use cad_primitives::{BoolOp, Point3, Vector3};
pub use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
pub use cherchi_rs::{Mesh, MeshBoolean};
pub use cherchi_rs::{NativeBoolean, NativeBooleanError};
// The constrained-Delaunay primitive, re-exported for the kernel-v2 render
// tessellation cores (its `tessellate.rs` patch/planar triangulation). kernel-v2
// may depend on yang-rs but NOT on cherchi-rs directly, so it consumes the CDT
// through this seam — the same pattern as `NativeBoolean` above and the torus
// UV-patch consumer's existing use of this primitive.
pub use cherchi_rs::triangulation::{
    cdt_polygon_with_holes, cdt_polygon_with_holes_floodfill, CdtError,
};
// `ArrangementError` is re-exported so that kernel-v2 (whose dep rules allow
// `yang-rs` but NOT `cherchi-rs`) can pattern-match the M8 boundary inside
// `NativeBooleanError::Arrangement` — specifically
// `ArrangementError::CoplanarPairDeferred`, which kernel-v2 maps to its
// typed `UnsupportedCoplanar` error. Public-surface addition only.
pub use cherchi_rs::ArrangementError;

/// Construct the PRODUCTION boolean backend: the native, in-process
/// cherchi-rs pipeline ([`NativeBoolean`]) — `mesh_arrangement` → labeling →
/// `keep_set(op)`. Reference parity vs the upstream C++ `mesh_booleans`
/// binary is the M6 gate (cherchi-rs `tests/parity_native_vs_sidecar.rs`);
/// the C++ subprocess sidecar (`cherchi-sidecar-rs`) is demoted to a
/// test-only parity oracle (PR-CR-BL3c).
///
/// Always `Some` since PR-CR-M7c: the predicates are clean-room pure Rust
/// (`cherchi-rs::predicates::indirect`) — there is no FFI stub build left to
/// guard against, and the backend is WASM-clean. The `Option` signature is
/// retained for the many existing
/// `let Some(nb) = yang_rs::native_backend() else { /* skip */ }` call
/// sites (their skip arms are now dead but harmless).
pub fn native_backend() -> Option<NativeBoolean> {
    Some(NativeBoolean)
}

// =========================================================================
// Tests
// =========================================================================
#[cfg(test)]
mod tests_unit;
