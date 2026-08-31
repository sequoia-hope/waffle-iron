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
//!   output. §4.5.4 illegal-self-intersection DETECTION shipped 2026-07-17
//!   (task #173, two layers — spec `specs/yang_173_selfx_detector.md`): the
//!   production loud STOP is kernel-v2's render-resolution boolean-output
//!   gate; the exact mesh-level test is banked here as the `YANG_SELFX_PROBE`
//!   diagnostic (`stage5_topology`, pre-`emit_topology`). REMOVAL (the
//!   paper's local refinement) is **NOT implemented** — N6 was closed as
//!   detection-shipped (user ratification 2026-07-17); the removal half is
//!   tracked under deviation N2 / the #169 mesh-update loop.
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
pub(crate) use boolean::*;
pub use boolean::{boolean, union_operands_strictly_disjoint};
mod errors;
mod geom;
mod stage1_tessellate;
mod stage3_ssi;
pub(crate) use stage3_ssi::*;
mod stage4_correct;

/// Yang §4.2.3 provenance-derived surface incidence (read-only measurement of
/// the deviation-N10 residual; see the module docs).
mod stage4_incidence;
// #188 inc-0: read-only YANG_S5_OSCULATION_PROBE (spec
// `specs/yang_188_f0082_j3_envelope_selection.md` §5). Probe-only module;
// nothing exported.
mod stage5_osculation_probe;
// Read-only `YANG_S6_LOOP_SIMPLICITY` census scan for the
// planar-and-self-intersecting emitted-loop class (F0067's CDT wall, anchored
// at commit 922a9892). Measurement only — never a gate; nothing exported.
mod stage5_loop_simplicity;
// #188 inc-1: §3.2 envelope-resolution primitives (exact switch-point
// solver + §7.6 op-resolved band classifier), UNWIRED — de-risked on the
// F0082 pinned fixture (`tests_unit/s188_envelope.rs`); inc-2 wires them
// into `emit_topology` behind `YANG_S5_ENVELOPE_ENABLE`.
pub mod stage5_envelope;
mod stage5_output_refine;
mod stage5_seam_merge;
mod stage5_topology;
pub(crate) use stage5_topology::*;
mod stage4_relocate;
pub use brep::{
    BRep, BRepEdge, BRepFace, BRepVertex, InputId, TessellationMap, TessellationSource,
    TriangleAttribution, TriangleAttributionMap, MATCH_TOLERANCE,
};
pub use stage1_tessellate::patch_tessellators::{tessellate_sphere_patch, tessellate_torus_patch};
pub(crate) use stage1_tessellate::*;
pub(crate) use stage4_correct::*;
pub(crate) use stage4_relocate::*;
mod stage4_boundary_curve;
mod stage4_phantom;
mod stage4_rehome;
mod stage4_slit;
// §4.5.1 corner-transit planner (spec `specs/yang_451_corner_transit.md`
// inc-2a): the pure recognize/discriminate/classify unit for §4-I9
// corner-crosser sites. Census-wired report-only from the §4-I9 branch;
// the gated apply arm is inc-2b+.
mod stage4_transit;
// §4.5.1 corner-transit APPLY planner (spec §3h, inc-2c-3b-0): the pure
// corrected-cycle machinery (mint pool, corridor path, directed sub-path
// replacement). Census-wired report-only; the gated mutation is 3b-1.
mod stage4_corridor;
// N2-3a (Yang §4.4.1): the fold-risk planner — decides WHICH relocations need
// the Fig-11 mesh update. Pure decision function, NOT wired into
// `stage4_relocate_and_correct`; wiring the merge arm onto its plan is N2-3b.
// `pub` for the same reason `stage4_dt` / `stage4_update` are: an unwired
// primitive has no in-crate caller, and hiding it behind `allow(dead_code)`
// would also hide a genuinely unreachable arm once it IS wired.
pub mod stage4_fold_risk;
mod stage4_project;
// #169 Phase B — the splice loop joining the chart / extraction / two-sided
// driver layers to the forward mesh. WIRED behind `YANG_MESHUP_ENABLE` from
// `stage5_topology::run_meshup_splice_passes` (N2-3b step 2, 2026-08-06).
mod stage4_construct;
mod stage4_splice;
// #169 / N2 — Yang step truncation. TWO primitives, both UNWIRED:
// `max_simple_step` (loop-simplicity trigger, a borrow justified by its own
// census) and `max_in_domain_step` (§4-I10: measures where a step leaves its
// carrier's bounded domain). NEITHER is §4.5.1's continuation for the §4-I9
// class: the paper EXCLUDES boundary points gliding along boundary curves from
// its first strategy (Fig-13), and all 24 sites measure as exactly that, so they
// belong to §4.5.2 local refinement. See spec §4-I10 (f).
mod stage4_truncate;
pub mod stage4_update;
pub use errors::{SsiRefinementError, Stage4InvalidReason, YangError};
pub(crate) use geom::{
    ellipse_param, ellipse_point, ellipse_tangent, hyperbola_param, surface_normal_at,
};
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
