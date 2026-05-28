# yang-rs — Scope Rules

Phase 2 of the clean-sheet kernel rewrite (see root `CLAUDE.md`). Implements the Yang 2025 hybrid boolean pipeline.

## What this crate does

- 6-stage pipeline (Stage 0 coplanar preprocessing → Stage 6 B-Rep reassembly) per Yang 2025
- Owns its own B-Rep input/output types — does NOT import kernel-v2's B-Rep. Conversion happens at the kernel-v2/yang-rs boundary, not inside this crate.
- Owns its own intermediate mesh type for use during Stages 1-4. This mesh type SUPPORTS non-manifold structures (coincident triangles from coplanar handling, post-Cherchi arrangement complexity).
- Owns the `TessellationMap` type (the bijective map from mesh elements to B-Rep features, established at Stage 1 and consulted at Stage 5/6).

## What this crate does NOT do

- Mesh boolean labeling — that's `cherchi-rs` (Stage 2 calls into it)
- Analytical SSI curves — that's `ssi-rs` (Stage 3 calls into it)
- Public `Kernel` trait — that's `kernel-v2` (kernel-v2 calls into us for boolean ops)
- Render tessellation — entirely out of scope. Render tessellation is in `kernel-v2`.

## Hard rules

1. **Dep layering enforced:** `cherchi-rs`, `ssi-rs`, `cad-primitives` only. NO dep on kernel-v2 (which depends on US — would be a cycle) and NO dep on the old `kernel` crate.
2. **Paper IS the spec.** Implement what Yang 2025 describes. Do NOT adapt to fit legacy code shapes. If the paper and reality conflict, the paper wins; if the paper is ambiguous, cite the C++ reference (`docs/sidecar/cherchi2022_build_guide.md` — the relevant subset Cherchi implements as part of their open-source release).
3. **TessellationMap is first-class.** Stage 1 must establish the bijection explicitly; Stages 5/6 consult it. No ad-hoc tracking via `face_ranges` / `source_face` annotations on mesh triangles like the legacy port did.
4. **Non-manifold support is internal-only.** kernel-v2's B-Rep is 2-manifold by contract; yang-rs's intermediate mesh during the pipeline can be non-manifold. Output must be 2-manifold or yang-rs returns `Err(YangError::NonManifoldOutput)`.
5. **Reference parity at Stage 2 boundary.** Stage 2's input mesh and Stage 2's output (from cherchi-rs) must match what Yang's paper describes for those interfaces. Differential test against Cherchi C++ sidecar for Stage 2 specifically.
6. **No `unsafe`, no `panic!` in production paths.** All errors return `Result<>`.
7. **Single-threaded.** Same rationale as cherchi-rs (determinism > speed during port).
8. **Stages 5/6 consume a `LabeledArrangement`** (roadmap §2), not post-hoc
   guesses. The producer (interim patched sidecar, later native `cherchi-rs`)
   reports mesh-level provenance; yang-rs maps mesh→B-Rep itself via its Stage-1
   `TessellationMap`. This makes Stage 5/6 **real** — see roadmap M3.
9. **Retain the YR3/4/5 substitutes as a `#[cfg(test)]` differential oracle —
   do NOT delete them.** The old spatial-vertex-matching / majority-vote
   attribution was a *substitute* for real labels, but as a second independent
   attribution method it cross-checks the true-label path: disagreement on a
   case localizes a label-path bug. Demote it from the production path to a
   test-only oracle module (roadmap M4); deleting it discards a free oracle.

## When working on this crate

You may read:
- Everything inside `crates/yang-rs/`, `crates/cherchi-rs/`, `crates/ssi-rs/`, `crates/cad-primitives/`
- `refs/text/yang2025_hybrid_boolean.txt` (the spec) and any cited references
- `docs/sidecar/cherchi2022_build_guide.md`

You may NOT read:
- `crates/kernel/src/boolean/yang_integration.rs` and friends — that's the legacy port being replaced. Looking at it will steer you toward replicating the entanglements we're trying to escape.
- `crates/kernel/src/boolean/cherchi/` — same issue.
- Old debug-queue audit memos (`docs/audits/yang_*`, `docs/audits/pr_y*`). Those concern the legacy port's drift.
- `crates/kernel-v2/` internals — you don't need them. Define your own B-Rep input/output types; conversion is the kernel-v2 author's problem.

## Stage development order

Recommended (each stage gates the next):

1. Define yang-rs's `BRep` input/output type (minimal, 2-manifold half-edge or whatever fits the pipeline cleanly)
2. Define `IntermediateMesh` (supports non-manifold) and `TessellationMap`
3. Stage 1 (bijective tessellation) — simple cases first (single planar face); validate the TessellationMap is consulted correctly
4. Stage 2 wiring — obtain a `LabeledArrangement` (roadmap §2). Interim:
   `cherchi-sidecar-rs` (patched binary). **Gate Stage-1 output on
   `mesh_booleans_inputcheck` first** — Cherchi loops forever on
   non-manifold / non-watertight / self-intersecting input (roadmap M1, the
   real gate to a first boolean).
5. Stage 5/6 reassembly — consume the real labels; even before Stage 3/4 SSI
   refinement, the pipeline should produce a (mesh-approximate) B-Rep output
   (roadmap M3). Stage 5/6's patch-segmentation logic is durable; only its
   *curve-source* changes when SSI lands — build that seam deliberately.
6. Stage 3/4 SSI refinement — once mesh-only output works, layer in the analytical refinement
7. Stage 0 coplanar preprocessing — last, because it's a special case that complicates everything earlier

This is the opposite of how the legacy port grew (which started with Stage 0 / coplanar handling and tangled up). Build the happy path first, then add the degenerate cases. Full milestone sequence: `docs/yang_functional_roadmap.md` §4 (M0–M8).
