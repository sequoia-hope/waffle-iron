# Yang Functional Roadmap — Single Source of Truth

> **Status:** authored 2026-05-28. This document supersedes the per-crate
> `PLAN.md` concept for the Yang effort (there are none in the new crates) and
> supersedes the stale "Current architecture (what's built)" block that used to
> live in the root `CLAUDE.md`. When this roadmap and a crate `CLAUDE.md`
> disagree on sequencing, this roadmap wins; when this roadmap and the Yang 2025
> paper disagree on *algorithm*, the paper wins (see `docs/yang_deviations.md`).

## 0. Honest status

The kernel rewrite (tiered crates `cad-primitives` → `cherchi-rs`/`ssi-rs` →
`yang-rs` → `kernel-v2`) has, after M0–M4 + 3 SSI increments, a **real but narrow
working boolean** plus deep foundations:

- `yang-rs`: **a functional boolean** — `boolean(brep_a, brep_b, op, backend)`
  produces a correct, topologized B-Rep + mesh for **planar, convex** solids
  (Union/Intersect/Subtract, **incl. holed faces / inner loops**, PR-YR5c). The
  randomized box-boolean fuzz (900 cases, aligned + rotated) is **100% correct,
  0 silent-wrong**. Real per-triangle labels flow from the patched sidecar.
- `cherchi-sidecar-rs` (M2/M3): patched C++ `mesh_booleans` emitting a
  `LabeledArrangement` (per-triangle labels + TBB-pinned determinism) — the
  interim Stage-2 producer the boolean runs on. **Native-only (no WASM).**
- `ssi-rs`: plane∩plane, plane∩sphere, sphere∩sphere, plane∩cylinder, and
  plane∩cone (bounded) — 5 analytical solver families, on-surface-exact,
  adversary-hardened. **Built in isolation; NOT yet wired into yang (Stage 3).**
- `cherchi-rs`: pure-Rust predicates + `FastTrimesh`/`Tree` + arrangement **Stage
  1 only**. The native arrangement *algorithm* (Stage 2) is **not written** (M6).
- `indirect-predicates-sidecar-rs`: FFI to Attene's LGPL predicates, IP1–IP6;
  intentionally non-WASM. Clean-room replacement is M7.
- `kernel-v2`: **empty scaffold** — does NOT implement the `Kernel` trait. There
  is **no path from a `.waffle`/feature tree to a yang-rs BRep** yet.

**What this is NOT yet:** no curved geometry in the boolean path (planes only),
no coplanar preprocessing (Stage 0), no non-convex tessellation, no `Kernel`-trait
surface, native-only (no WASM). The legacy metrics (`yang_fast 12/157`, `1250/34`)
measure *legacy* `crates/kernel/` — not new-kernel progress. The honest current
new-kernel metric is the planar-convex box-boolean fuzz (**100%**).

The shortest honest path to a first functional boolean is the M0–M8 milestones
(§4); the full path to a kernel that **replaces legacy** is the Phase 1–6
completion roadmap (§4b), which reconciles M5–M8 with the under-tracked
`kernel-v2` driver + migration work.

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
  - **Step 3 — `ssi-rs` plane∩cone, bounded sections (PR-SSI3) ✅ DONE.** A15.4
    pair #3: adds the `QuadricSurface::Cone` surface (infinite double cone, pure
    quadric) and `plane_cone` for the **bounded** sections — C1 circle (plane ⟂
    axis) + C2 ellipse (closed section) — reusing `Circle`/`Ellipse`. Classifies
    via the two symmetry-plane generators `g_±=cosα·â±sinα·û`; ellipse params from
    the vertex method + closed-form `b²=(d·â)²/cos²α−|d|²`. **Scope (user decision):
    bounded first** — parabola/hyperbola (PH) and through-apex (AP) return loud
    `Err` (`AnalyticalSolutionNotAvailable`/`DegenerateInput`), a deliberate staged
    gap removed in PR-SSI4, never a fallback (A15.2). On-surface oracle uses a cone
    **radial** residual (length). Adversary (17 attacks) confirmed the dangerous
    ellipse↔parabola boundary is robust (huge `a` stays finite + on-surface; clean
    flip to `Err` at the `gd_±` gate; no NaN/Inf/misclassification) and flagged a
    minor C1-gate conditioning inconsistency vs `plane_cylinder` — fixed for
    consistency (gate on the stable `|n̂−k·â|`, not `√(1−k²)`; reuse `proj` for `û`).
    Spec `specs/ssi_pr_ssi3_plane_cone.md`; commits `d0f3bfe1`→`f16f9fbd` (RED)→
    `014e7445` (GREEN)→`f3cacaae` (adversary)→`64047b06` (spec fix)→`ddc5e2be`
    (consistency fix). 86/86 ssi-rs tests; CI gate clean; no sibling/legacy changes.
  - **Step 4 — `ssi-rs` plane∩cone unbounded conics (PR-SSI4) ✅ DONE.** Completes
    the **four proper** plane∩cone sections: adds the first two **unbounded**
    `SsiCurve` types — `Parabola { vertex, normal, axis_dir, focal_length }` and
    `Hyperbola { center, normal, major_axis, semi_transverse, semi_conjugate }` —
    each with its own `eval`, and the `plane_cone` PARA/HYPE branches replacing the
    SSI3 staged `Err`. On the infinite double cone a hyperbola returns **two**
    branch curves (`±major_axis`, `+m̂` first). Constructions hand-verified before
    coding (hyperbola α=π/4 plane x=1 → center(1,0,0),a=b=1,vertices(1,0,±1);
    parabola → vertex(½,0,½),f=1/(2√2),eval(1)=(0,−1,1) on both surfaces). The new
    PH contract obsoleted PR-SSI3's staged-gap assertions (2 `ssi3.rs` placeholders
    + 6 `ssi3_adversary` attacks) — migrated to the new contract, **adversary-
    verified faithful** (no attack weakened; the sweep guard tightened). Adversary
    (13 attacks): no bugs; both classification boundaries clean (no NaN/blow-up/
    misclassification); parabola on-surface to coord ~1e8, hyperbola exact to T≈7.
    Through-apex degenerate conics (point/line/two-lines) **deferred to PR-SSI5**
    (still `Err(DegenerateInput)`). Spec `specs/ssi_pr_ssi4_plane_cone_unbounded.md`;
    commits `d7cbbd8a`→`39841dc5` (RED)→`b1f5da9f` (GREEN + faithful fixture
    migration)→`38f7d553` (adversary). 108/108 ssi-rs tests; CI gate clean.
    **plane∩{plane,sphere,cylinder,cone} now complete for all proper conics.**
  - **Step 5 — `ssi-rs` plane∩cone through-apex degenerate conics (PR-SSI5) ✅
    DONE — plane∩cone now COMPLETE.** Replaced the AP `Err(DegenerateInput)` with
    the degenerate result: point → `Ok([])` (`|k|>sinα`, incl. ⟂); one Line
    (`|k|=sinα`, tangent generator); two Lines (`|k|<sinα`, crossed generators
    through the apex). No new curve types (reuses `Line`); sub-case classified by
    the proven `gd_±` sign test (`gd₊·gd₋=k²−sinα²`); two-line dirs `cφ·m̂±sφ·ŵ`.
    Hand-verified (α=π/4, n̂=(1,0,0) → (0,∓1,1)/√2 = `z²=y²`; tangent → (−1,0,1)/√2;
    ⟂ → []). The new AP contract obsoleted PR-SSI3's AP assertions (2 ssi3 tests +
    1 adversary attack) — migrated to the new contract, **adversary-verified
    faithful**. Adversary (13 attacks): no bugs; clean monotone point↔line↔two-line
    boundary; clean AP-detection band; lines exact on both surfaces. Spec note: the
    tangent sub-case is a ~1.4e-7-wide k-window (intrinsic to the exact `k=sinα`
    degenerate). Spec `specs/ssi_pr_ssi5_plane_cone_through_apex.md`; commits
    `e974295b`→`476fc663` (RED)→`c2d9ed47` (GREEN)→`9d109bef` (adversary). 129/129
    ssi-rs tests; CI gate clean. **plane∩{plane,sphere,cylinder,cone} fully done.**
  - **Step 6 — `ssi-rs` sphere∩cylinder coaxial (first degree-4 pair, PR-SSI6) ✅
    DONE.** The degree-4 pairs' **coaxial/special** configs reduce to analytic
    conics (research-confirmed; the legacy code does the same). PR-SSI6 ships the
    first: coaxial sphere∩cylinder (axis through sphere center) → circles
    (`z²=r_s²−r_c²`): X2 two circles at `C±h·â`, X1 one tangent great circle, X0
    empty. Reuses `Circle` — no new curve type, no enum-match migration.
    **Non-coaxial (general degree-4) → `Err(AnalyticalSolutionNotAvailable)`** — a
    staged gap (the general degree-4 curve needs a new `SsiCurve` variant, deferred),
    never a fallback. Establishes the coaxial-detect→reduce-to-circles→general-ASNA
    pattern for the other circle-reducible pairs. Adversary (15 attacks): no bugs;
    clean tangent + coaxial-detection boundaries; two characterized absolute-`TAU`
    ceilings (on-surface ~1e9, coaxial-detection ~1e8 → conservatively NC). Spec
    `specs/ssi_pr_ssi6_sphere_cylinder_coaxial.md`; commits `16dca4a0`→`818f2882`
    (RED)→`de7926c4` (GREEN)→`614292b1` (adversary). 141/141 ssi-rs tests; CI clean.
  - **Step 7 — `ssi-rs` sphere∩cone coaxial (second degree-4 pair, PR-SSI7) ✅
    DONE.** Reuses the SSI6 coaxial-detect→reduce-to-circles→general-ASNA pattern.
    Coaxial sphere∩cone (sphere center on the cone axis) reduces to one/two circles
    via `sec²α·h² − 2h0·h + (h0²−r_s²)=0`, roots `h=(h0±√D)·cos²α`,
    `D=sec²α·r_s²−h0²tan²α`. Gate on the **linear** gap `g=r_s−|h0|·sinα`
    (`sign(D)=sign(g)`, since `D=sec²α(r_s−|h0|sinα)(r_s+|h0|sinα)`) per the
    SSI2/3/6 lesson, so X2 (`g>TAU`) guarantees `D>0` and `√D` is safe: X2 two
    circles (`+√D` first), X1 one tangent circle (`|g|≤TAU` at `h_t=h0·cos²α`), X0
    empty (`g<−TAU`). Reuses `Circle` — no new curve type, no enum-match migration.
    **Non-coaxial (general degree-4) → `Err(AnalyticalSolutionNotAvailable)`** —
    staged, never a fallback. Adversary (18 attacks): no bugs; clean tangent
    (α≠π/4 exercised) + coaxial-detection boundaries; characterized absolute-`TAU`
    ceilings (on-surface ~1e8→1e9, coaxial-detection ~1e8→1e9 → conservatively NC)
    and the apex-grazing `r_s=|h0|` radius-0 point-circle degeneracy (downstream
    filters it). Spec `specs/ssi_pr_ssi7_sphere_cone_coaxial.md`; commits
    `6d58f415` (spec)→`9144dfd4` (RED)→`8b12402d` (GREEN)→`d575280b` (adversary).
    189/189 ssi-rs tests; CI clean.
  - **Next M5 increments (sequenced):** the remaining circle-reducible coaxial
    pairs (cyl∩cone, cone∩cone) → cyl∩cyl special cases (equal-R
    intersecting → two ellipses; parallel → two lines) → the **general degree-4
    curve** (a new parametric `SsiCurve` variant + the 5 general-position solvers) +
    torus pairs (rest of A15.4) → curved `Surface` variants + curved Stage-1
    tessellation in `yang-rs` → curved face resolution + the planar-assumption
    migration → Stage 3 (wire `ssi-rs` into `yang-rs`) → Stage 4 (CDT remesh).
- **M6 — Native `cherchi-rs` Stage 2** behind the same interface, parity-green
  vs the sidecar on the corpus.
- **M7 — Clean-room indirect predicates from Attene's paper → restore WASM.**
  Removes the LGPL FFI dependency and the `compile_error!` WASM block.
- **M8 — Stage 0 coplanar preprocessing** hardened last (special case that
  complicates everything earlier).

## 4b. Completion roadmap — Phases 1–6 (the full path to replacing legacy)

The M0–M8 list above is the *milestone* sequence for the boolean. This section is
the **completion** view: what it takes for `kernel-v2` to **replace** the legacy
kernel — handle planar + curved + coplanar + non-convex, implement the `Kernel`
trait, pass assay at parity-or-better, and run in WASM. It reconciles M5–M8 with
the **under-tracked** `kernel-v2` driver (Phase 4) + migration (Phase 6).

**"Complete Yang" ::=** kernel-v2 implements `Kernel`/`KernelIntrospect`; planar +
curved + coplanar + non-convex all handled; assay ≥ legacy on the supported
corpus; runs in WASM; `crates/kernel/` deleted — with **reference parity vs the
Cherchi C++ sidecar maintained throughout** (the non-negotiable correctness oracle).

```
Phase 1 ─► Phase 2 ─► Phase 3 ─► Phase 4 ─┐
                                          ├─► Phase 6 (migrate, assay, delete legacy)
Phase 5 (native arrangement + WASM) ──────┘   [parallel track, joins before 6]
```

- **Phase 1 — Finish the analytical SSI engine (`ssi-rs`).** *[in progress; ⊂ M5]*
  PR-SSI4 (parabola/hyperbola + through-apex), then the remaining A15.4 pairs
  (Degree-4: sphere∩cyl, cyl∩cyl, cone∩cone, sphere∩cone, cyl∩cone; torus pairs).
  **Exit:** all 15 quadric pairs analytically solved, adversary-hardened,
  on-surface-exact. **Risk:** low–moderate. **Size:** medium (~10–15 PRs).
  *Frontier (out of scope):* revolving an arbitrary profile → non-quadric surface
  of revolution → numerical/marching SSI (Patrikalakis Case F), a later capability.
- **Phase 2 — Curves enter the pipeline (Stages 1/3/4/6 curved).** *[⊂ M5; the
  heart, highest risk]* Stage 1 curved tessellation (sample analytical surfaces →
  mesh, keep the surface tier via the bijection) + non-convex profile triangulation
  (Livesu earcut-CDT, for gears) + Steiner points; Stage 3 refine arrangement edges
  on two analytical surfaces to the exact SSI curve (wire `ssi-rs` in); Stage 4 CDT
  remesh conforming to refined curves; Stage 6 curved-face reassembly + 2-manifold
  closure (cut-surface faces deferred in PR-YR5). **Exit:** cylinder ∪ box,
  sphere − cylinder → correct curved B-Rep, sidecar mesh-parity + analytically
  exact edges. **Risk:** HIGH (paper-critical). **Size:** large.
- **Phase 3 — Coplanar preprocessing (Stage 0).** *[= M8]* detect coplanar face
  pairs pre-tessellation; 2D boolean → A-only/B-only/overlap; shared trimmed
  surface + identical meshes; overlap boundaries → intersection curves. **Exit:**
  flush/stacked faces + multi-plane cross-booleans work without conformal-edge
  explosions. **Risk:** moderate–high. **Size:** medium–large.
- **Phase 4 — The `kernel-v2` driver (Kernel trait).** *[NEW — the integration
  unlock; not in M0–M8]* implement `Kernel`/`KernelIntrospect` over yang-rs
  (`make_faces_from_profiles`, `extrude_face`, `revolve_face`, `boolean_*(_multi)`,
  `tessellate → RenderMesh`, `extract_edges`, introspection). **Strategic slice
  (Phase 4a):** a **planar-only driver can land early** — right after the current
  baseline — to get a *categorized* assay score and de-risk the feature-tree →
  kernel → mesh path before the geometry mountain; expand as Phases 2–3 land.
  **Exit:** feature-engine builds + tessellates through kernel-v2; assay runs
  (categorized supported/correct/unsupported). **Risk:** moderate. **Size:** large.
- **Phase 5 — Native arrangement + WASM.** *[= M6 + M7; parallel track]* M6 native
  `cherchi-rs` Stage-2 behind the `LabeledArrangement` seam, parity-green vs the
  sidecar (retires the C++ subprocess); M7 clean-room indirect predicates from
  Attene's paper (drops LGPL FFI) + restore the WASM build (`compile_error!`
  removed). **Exit:** pure-Rust boolean compiling to WASM, browser-parity with
  native. **Risk:** moderate–high (subtle predicates; IP-sidecar is the oracle).
  **Size:** large. **Runs in parallel** with Phases 2–4.
- **Phase 6 — Migration + assay.** *[the finish line]* swap wasm-bridge +
  feature-engine to kernel-v2; run the real assay; iterate to parity-or-better;
  **delete `crates/kernel/`**; rebuild the WASM bundle. **Exit:** legacy gone,
  assay ≥ legacy on the supported corpus, GUI on kernel-v2.

**Where the risk lives:** almost all of it is Phase 2 (curved Stage 3/4). Phase 1
is a steady low-risk grind; Phases 4/6 are large but mechanical once geometry
works; Phase 5 is a contained predicates problem with a ready oracle. **Scale:**
multi-month, not a few sessions.

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
