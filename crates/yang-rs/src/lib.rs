//! Yang 2025 hybrid B-Rep / mesh boolean pipeline.
//!
//! ## Scope
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
//! ## Input / output
//!
//! - Input: two B-Rep solids (this crate defines its own B-Rep input type;
//!   it does NOT import from `kernel-v2`). Caller is responsible for the
//!   conversion at the boundary.
//! - Output: one B-Rep solid (same type as input)
//! - Per-stage non-manifoldness is allowed internally; INPUT and OUTPUT
//!   are 2-manifold by contract. If yang-rs detects non-manifold input,
//!   it returns `Err(YangError::NonManifoldInput)`. If reassembly would
//!   produce non-manifold output, it returns `Err(YangError::NonManifoldOutput)`.
//!
//! ## References
//!
//! - Yang et al. 2025 — `refs/text/yang2025_hybrid_boolean.txt`
//! - The pipeline IS the spec. Read the paper before working on this crate.

// Skeleton — content fills in during Phase 2.
