# Yang Functional Roadmap — Single Source of Truth

> **Status:** authored 2026-05-28. This document supersedes the per-crate
> `PLAN.md` concept for the Yang effort (there are none in the new crates) and
> supersedes the stale "Current architecture (what's built)" block that used to
> live in the root `CLAUDE.md`. When this roadmap and a crate `CLAUDE.md`
> disagree on sequencing, this roadmap wins; when this roadmap and the Yang 2025
> paper disagree on *algorithm*, the paper wins (see `docs/yang_deviations.md`).

## 0. Honest status

The kernel rewrite (tiered crates `cad-primitives` → `cherchi-rs`/`ssi-rs` →
`yang-rs` → `kernel-v2`) has, after ~29 PRs, built real foundations:

- `cherchi-rs`: pure-Rust predicates (via `geometry-predicates` + `dashu`),
  `FastTrimesh`/`Tree` data structures, and arrangement **Stage 1 only**
  (intersecting-pair detection). The arrangement *algorithm* (classify → split →
  re-triangulate → assemble → label) is **not written**.
- `yang-rs`: **Stage 1** bijective tessellation, real for planar convex faces.
  Stages 2/5/6 are explicitly-labeled **substitutes** (post-hoc spatial
  vertex-matching + majority-vote attribution + flood-fill), not real Yang.
- `cherchi-sidecar-rs`: subprocess wrapper of the C++ `mesh_booleans` binary,
  returns a result mesh only (no labels).
- `indirect-predicates-sidecar-rs`: FFI to Attene's LGPL predicates; ported
  IP1–IP6 with **no consumer yet**; intentionally non-WASM.
- `ssi-rs`, `kernel-v2`: empty scaffolds.

**There is no working boolean end to end.** The historical metrics
(`yang_fast 12/157`, `1250/34` kernel tests) measure the *legacy* `crates/kernel/`,
not the new crates — do not cite them as new-kernel progress.

This roadmap charts the shortest honest path to a first **functional** Yang
boolean and then to the full analytical pipeline.

## 1. Thesis: decouple "functional Yang" from "native arrangement complete"

The prior roadmap gated real Yang Stage 5/6 on a *complete native `cherchi-rs`
arrangement* — a large, entirely unwritten graph algorithm. That coupling is why
the project shipped throwaway substitutes instead of a validated vertical slice.

We break the coupling with a producer-agnostic **`LabeledArrangement`**
interface (§2). An *interim* producer (patched C++ sidecar, §3a) satisfies it
now, so `yang-rs` Stage 5/6 becomes **real** in weeks. The *native* `cherchi-rs`
arrangement (§3b) is then built behind the **same** interface, with the sidecar
as its differential-parity oracle.

## 2. The `LabeledArrangement` interface (the contract)

Defined **once, here**. Crate `CLAUDE.md` files reference this section; they must
not redefine the shape. Freeze it only after round-tripping the two validation
cases in §3a.

> **Revised 2026-05-29 (M2), solid-level provenance.** The original contract
> below asked for `source: SmallVec<(InputId, parent_tri_index)>`. Inspecting the
> Cherchi 2022 C++ source showed the input-*triangle* index is **lost** during
> arrangement subdivision — daughter triangles inherit only the parent's
> mesh-level label (`labels.surface`, a bitset of which input *solid*). Recovering
> a triangle index would need an invasive patch to arrangement internals we don't
> own. Yang reassembles *faces*, not triangles, and the face is recoverable from
> solid-id + plane-membership (the exact arrangement keeps each non-coplanar
> sub-triangle in its source face's plane) — which yang-rs does in M3. So the
> contract is **solid-level**; the triangle index is dropped. See
> `specs/yang_m2_labeled_arrangement.md`.

The output of Yang Stage 2 — the **full** arrangement mesh (all sub-triangles,
before any op filter; yang-rs does its own op selection). Per **output triangle**:

- `surface: Vec<InputId>` — which input solid(s) the triangle lies on. **len ≥1
  normally; len ≥2 only at coplanar overlap** (an output triangle can belong to
  both A and B — Cherchi 2022 §3). A scalar would silently mis-attribute coplanar
  faces (the case the legacy port died on).
- `inside: Vec<bool>` — in/out per input solid (`inside[k]` = the triangle is
  inside solid `k`). Captured **before** the op filter collapses it.
- `patch_id: u32` — Cherchi's connected same-surface patch (its own Stage-5
  grouping), one per triangle.

**Division of labour.** `yang-rs` owns the mesh→B-Rep mapping via its Stage-1
`TessellationMap` + geometric plane-membership. The producer reports only
**solid-level** provenance + in/out + patch; `yang-rs` composes: output tri →
(producer: solid A/B) + (geometry: which of that solid's face-planes contains it)
→ B-Rep face.

## 3. Producers

### 3a. Interim — patched `mesh_booleans` sidecar  *(chosen path)*

Cherchi 2022 already tracks per-output-triangle origin internally (§3 of the
paper: *"for each output triangle we propagate information on its origin"*) and
classifies patches in/out per input. The work is to **emit** it, not compute it.

- **Patch location:** reach into `customBooleanPipeline` (pre-filter), not just
  `main.cpp` — the op-specific selection collapses the per-input in/out vector,
  so dump the labels *before* that filter.
- **Format:** a sidecar file written alongside the result OBJ, encoding the
  `LabeledArrangement` shape from §2 (per-tri source list + patch id; patch
  in/out table).
- **Validate before freezing the interface:** round-trip on (1) two tetrahedra
  (clean 1:1 provenance) AND (2) one coplanar-overlap case (multi-attribution).
  Only then is §2's shape frozen.

`cherchi-sidecar-rs` owns this producer and the C++ patch.

### 3b. Native — `cherchi-rs` Stage 2, same interface

Built incrementally, **diffed against the sidecar** on the corpus. The IP-FFI
predicates (`indirect-predicates-sidecar-rs`) are consumed **demand-driven** by
this code — we stop porting predicates ahead of a caller. Per user directive,
the native path uses the FFI predicates *first*, then a clean-room
reimplementation from Attene's paper restores WASM (M7).

## 4. Critical path & milestones (ORDERED)

> The real gate to a first boolean is **not** the label interface — it is
> Stage-1 mesh *validity*. Cherchi loops forever on malformed input: fed real
> F0002 tessellation, `mesh_booleans` failed all three `inputcheck` predicates
> and ran ~6 h before being killed. The native arrangement would hit the same
> wall. So M1 precedes M2.

- **M0 — Operationalize the parity oracle.** ✅ **DONE** (`scripts/build_sidecars.sh`;
  the C++ sidecars build, `indirect-predicates-sidecar-rs` runs in available mode
  (42 tests), and the `cherchi-sidecar-rs` / `cherchi-rs` parity tests exercise
  the real binary instead of self-skipping).
- **M1 — Stage 1 emits Cherchi-`inputcheck`-clean meshes.** ✅ **DONE** (convex
  planar scope). `yang-rs` Stage 1 (`BRep::new`) canonicalizes each face's
  triangle winding to its analytic `Surface::Plane.normal` (Newell normal +
  dot-sign reverse); degenerate/sub-feature-area faces → `YangError::DegenerateFace`.
  Cube + tetrahedron pass all five `inputcheck` axioms against the real binary.
  Spec: `specs/yang_m1_stage1_orientation.md`; commits `f423581d` (spec) →
  `a66460f6` (RED) → `7da238d4` (GREEN) → `24e73307`/`d356297b` (adversarial
  area-threshold fix). **Scope:** convex planar faces only; non-convex/holes are
  banked (PR-YR2b–d) and not yet made inputcheck-clean.
- **M2 — Patched sidecar emits `LabeledArrangement`.** ✅ **DONE.** A
  version-controlled C++ patch (`patches/cherchi2022_labeled_arrangement.patch`,
  applied by `scripts/build_sidecars.sh`) dumps, per arrangement triangle, the
  surface solid(s) + per-solid in/out + Cherchi patch id; `cherchi_sidecar_rs::
  labeled_arrangement()` parses it into a `cherchi_rs::LabeledArrangement` (the
  **frozen, solid-level** §2 shape). Acceptance oracle green: `keep_set(op)`
  reproduces the stock `boolean(op)` triangle set for all four ops; coplanar
  cubes yield real multi-attribution; deterministic (TBB pinned to 1 thread).
  Spec: `specs/yang_m2_labeled_arrangement.md`; commits `0d321e6a` (spec+§2) →
  `3add0ebd` (RED) → `cd78d15b` (C++ patch+build) → `68bceb66` (GREEN Rust) →
  `b091553d` (adversarial + env hardening).
- **M3 — `yang-rs` Stage 5/6 consume true labels → FIRST functional boolean.**
  ✅ **DONE.** `boolean()` consumes the `LabeledArrangement` (via the new
  `MeshBoolean::labeled_arrangement` seam), welds the arrangement mesh,
  `keep_set(op)`-selects + orients (`flip_for_op`) the kept tris, resolves each
  tri's source face geometrically (centroid-in-plane, `TAU_WORK`; degenerate
  edge-slivers attributed to the lowest tied face), and reassembles via
  `reconstruct_topology` into a **watertight 2-manifold B-Rep**. Verified on
  independent interpenetration geometry (not just the canonical cubes): signed
  volumes exact (union/intersect/subtract), 0 unpaired half-edges, Euler V−E+F=2.
  **Scope:** Union/Intersect/Subtract on interpenetrating convex planar solids.
  **Deferred:** Xor (multi-shell — gated loudly via `YangError::UnsupportedOp`);
  coplanar overlap (M8, `FaceResolutionFailed`); curved surfaces/SSI (M5).
  Spec: `specs/yang_m3_functional_boolean.md`;
  commits `4f206b27`→`4bac08cb`→`a945e037`→`d81eeda4`→`f43294c2`.
- **PR-YR5c — B-Rep faces with inner loops (holes).** ✅ **DONE.** When one solid
  pierces a hole through another's face, `reconstruct_topology` now builds the
  annular face (multi-cycle boundary extraction; outer = largest-|area| cycle,
  the rest are holes; cavity-wall normals flipped to point result-outward) instead
  of erroring `NonManifoldOutput`. **Impact:** the randomized box-boolean fuzz
  (`tests/fuzz_boxes.rs`, 900 cases) went from **75.2% → 100%** correct
  (aligned 86.2→100%, rotated 64.2→100%), eliminating the entire
  `NonManifoldOutput` bucket, with `SILENT_WRONG` still 0. Genuine non-manifold
  (T-junction/dead-end) and nested holes still error loudly.
  Spec: `specs/yang_pr_yr5c_inner_loops.md`; commits
  `ed550ae5`→`bbb14283`→`d90aa5f1`→`59771f86`→`287ea5ee`.
- **M4 — Retain YR3/4/5 substitutes as a `#[cfg(test)]` differential oracle.**
  ✅ **DONE** (bundled with M3). `match_with_input`/`face_candidates`/
  `majority_vote` moved to `#[cfg(test)]`; differential test cross-checks the
  real-label attribution against the substitute. Not deleted.
- **M5 — Stage 3/4 SSI + CDT refinement** (faceted → surface-exact). `ssi-rs`
  solvers + mesh-updating CDT along refined curves. Stage 5/6's
  *patch-segmentation* logic is durable; only its *curve-source* changes — build
  the seam there.
  - **Decision (curve representation):** the kernel uses **true analytical
    curves** (a plane∩sphere edge is a `Circle`, not a polyline) with **f64
    parameters** — zero shape error; topology robustness stays in the exact mesh
    predicates (cherchi/dashu). This is the Yang/Parasolid/ACIS model and is NOT a
    deviation (SSI *is* Yang Stage 3). Faceted-but-displayed-smooth was rejected
    (inexact geometry that compounds through chained ops).
  - **Step 1 — `ssi-rs` exact-SSI foundation (PR-SSI1) ✅ DONE.** Stood up the
    crate's public surface (`QuadricSurface{Plane,Sphere}`, `SsiCurve{Line,Circle}`,
    `SsiError`, `eval`, deterministic `in_plane_basis`) + the first 3 closed-form
    solvers (`plane_plane`, `plane_sphere`, `sphere_sphere`, each citing
    Patrikalakis §5.8) + `intersect()` dispatch. On-surface oracle (sample curve →
    satisfy both surfaces) + analytical-geometry + symmetry + determinism oracles.
    Adversary: no bugs; near-tangent guards short-circuit before `sqrt` (no NaN);
    solvers relative-correct to ≥1e9 scale; the absolute on-surface oracle is
    bounded to coordinate magnitude ~1e8 (recorded in spec — relative residual for
    larger-scale Stage-3 consumers). Spec `specs/ssi_pr_ssi1_foundation.md`;
    commits `8b1c7282`→`7255b380`→`c001101e` (RED)→`a508e865` (GREEN)→`c4e1efe0`
    (adversary). 28/28 ssi-rs tests; CI gate clean; no sibling/legacy changes.
  - **Step 2 — `ssi-rs` plane∩cylinder (PR-SSI2) ✅ DONE.** A15.4 pair #2: adds
    the `QuadricSurface::Cylinder` surface and the first non-circular curve
    (`SsiCurve::Ellipse` + its `eval`), with the `plane_cylinder` solver (C1
    perpendicular→Circle, C2 oblique→Ellipse `a=r/|c|`, C3a parallel-secant→two
    Lines, C3b tangent→one Line, C3c disjoint→[], E1 degenerate→Err) and the first
    triggerable `AnalyticalSolutionNotAvailable` path (sphere∩cylinder). Stays in
    closed-form conic territory — no Degree-4 quartics. **Adversary found a real
    bug:** the C1 band, gated on `1−|c|`, let the snap-to-perpendicular circle sit
    up to `√(2·TAU)·r ≈ 4.5e-4·r` off the cutting plane (~4000× tolerance) because
    the off-plane error scales with the *sine* `√(1−c²)`. Fixed (RED→GREEN
    sub-cycle) by gating C1 on `|proj|=√(1−c²)<TAU_MODEL` (the axis's in-plane
    projection norm, which also unifies with C2's `normalize(proj)` guard) →
    off-plane error bounded by `r·TAU_MODEL`. Adversary also confirmed a C2
    ellipse's on-surface residual tracks `r`, not the (possibly huge) major axis.
    Spec `specs/ssi_pr_ssi2_plane_cylinder.md`; commits `b53e566c`→`22729f1f` (RED)
    →`394f772a` (GREEN)→`5a3cded6` (spec fix)→`9a8c6c37` (adversary RED)→`37e17ff7`
    (fix GREEN). 55/55 ssi-rs tests; CI gate clean; no sibling/legacy changes.
  - **Next M5 increments (sequenced):** more solver pairs (plane-cone,
    sphere-cylinder; the Degree4 conic/quartic curves) → curved `Surface` variants
    + curved Stage-1 tessellation in `yang-rs` → curved face resolution + the
    planar-assumption migration → Stage 3 (wire `ssi-rs` into `yang-rs` to refine
    mesh edges → SSI curves) → Stage 4 (CDT remesh along refined curves).
- **M6 — Native `cherchi-rs` Stage 2** behind the same interface, parity-green
  vs the sidecar on the corpus.
- **M7 — Clean-room indirect predicates from Attene's paper → restore WASM.**
  Removes the LGPL FFI dependency and the `compile_error!` WASM block.
- **M8 — Stage 0 coplanar preprocessing** hardened last (special case that
  complicates everything earlier).

## 5. Risks & decisions

- **Coplanar multi-attribution** (S1): the `LabeledArrangement` source must be a
  list, and in/out a per-input vector. Locked in §2.
- **Stage-1 cleanliness is the true gate** (S2): M1 before M3. A label producer
  cannot mask inputs that violate Cherchi's axioms.
- **Substitutes retained, not deleted** (S3): M4.
- **`cherchi-rs` layering amended** (S5): its `CLAUDE.md` Hard-rule #7 (dashu
  only) is amended to permit a *temporary, feature-gated, non-WASM* dependency
  on `indirect-predicates-sidecar-rs`. Without this the constitution blocks the
  plan. The clean-room (M7) restores the pure-Rust/WASM end state.
- **Dockerfile stays thin** (build caution): do **not** add a `RUN make` layer —
  it costs ~22 min and ~8 GB per image rebuild. Install only build prerequisites
  (cmake/clang are already present) and run `scripts/build_sidecars.sh` at
  container-create / first-test, with `CHERCHI2022_BIN` / `INDIRECT_PREDICATES_SRC`
  env defaults.

## 6. PR granularity for the arrangement

The micro RED→GREEN PR style (15–50 LOC) suited isolated predicates but cannot
meaningfully slice a graph algorithm. For Stage 2, use **vertical,
behavior-tested slices** where **GREEN ::= "matches the sidecar oracle on corpus
subset N"**, not "compiles + unit test". The oracle is what makes large slices
safe; this is why M0 (operationalize parity) comes first.

## 7. Doc-edit ledger

This re-charting touched:

| File | Change |
|---|---|
| `docs/yang_functional_roadmap.md` | this file (new SSOT) |
| `CLAUDE.md` | rewrote stale "Current architecture" block; re-sequenced phase tracker & priorities; PLAN.md note |
| `crates/cherchi-rs/CLAUDE.md` | Hard-rule #7 amended (interim IP-FFI dep); Stage-2 = `LabeledArrangement` producer |
| `crates/cherchi-sidecar-rs/CLAUDE.md` | elevated to interim `LabeledArrangement` producer + label-emission mission |
| `crates/yang-rs/CLAUDE.md` | `LabeledArrangement` consumption; retain substitutes as test oracle |
| `crates/indirect-predicates-sidecar-rs/CLAUDE.md` | predicates demand-driven; clean-room/WASM end-state |
| `docs/yang_deviations.md` | appended interim labels-from-sidecar deviation |
| `Dockerfile` + `scripts/build_sidecars.sh` | operationalize parity (thin image + build script) |

## 8. Deviations from Yang 2025

The interim path takes Stage-2 labels from the C++ sidecar rather than from a
native arrangement, as the paper assumes. This is a tracked deviation — see
`docs/yang_deviations.md`. It is resolved at M6 (native Stage 2) / M7 (WASM).
