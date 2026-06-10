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
  adversary-hardened. **Wired into yang Stage 3 as of PR-YR9** (plane∩cylinder
  drives exact `cylinder ∪ box` intersection edges); the other pairs await their
  consuming geometry.
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
  - **Step 8 — `ssi-rs` cylinder∩cone coaxial (third degree-4 pair, PR-SSI8) ✅
    DONE.** Reuses the SSI6/SSI7 coaxial-detect→reduce-to-circles→general-ASNA
    pattern. Coaxial cyl∩cone (axes parallel AND cyl axis_point on the cone axis
    line) reduces to **exactly two circles** at `h = ± r_c·cotα` via the classical
    `|h|·tanα = r_c` reduction. **Unlike SSI6/SSI7 there is NO discriminant / √ /
    tangent / empty branch** — the cone's `[0,∞)` per-nappe radial range meets the
    constant `r_c` at exactly one height per nappe, so valid coaxial input is
    *always* two circles; manufacturing a discriminant to mirror SSI7 would be a
    hack-to-pattern (P9/P10) and is prohibited. Branches: X2 (two circles, h>0
    nappe first) / **NC** (non-coaxial → `Err(AnalyticalSolutionNotAvailable)`,
    staged, never a fallback) / E1 (`r_c≤0`/non-finite, bad α, zero cone/cyl axis →
    `DegenerateInput`). Reuses `Circle` — no new curve type, no enum-match
    migration. RED enforces the anti-hack invariant (a 5×5 α/r_c sweep asserting
    `len()==2` always). Adversary (13 attacks): no bugs; parallelism + on-axis gate
    boundaries (each flips at `TAU_MODEL`), α near both E1 limits, reversed/
    antiparallel axes, a 525-config determinism sweep, and characterized
    absolute-`TAU` ceilings (on-surface oracle holds to r_c≈1e8 → breaks at 1e9;
    `d_ax` coaxial band holds to scale ~7e8 → conservatively flips to ASNA by ~1e9)
    plus the in-band snap-to-cone-axis slack. Spec
    `specs/ssi_pr_ssi8_cylinder_cone_coaxial.md`; commits `7d820153` (spec)→
    `45c4eed1` (RED)→`d25fa0cb` (GREEN)→`e3285699` (adversary). 217/217 ssi-rs
    tests; CI clean.
  - **Step 9 — `ssi-rs` cone∩cone coaxial (fourth & LAST circle-reducible
    degree-4 pair, PR-SSI9) ✅ DONE.** Reuses the SSI6/7/8
    coaxial-detect→reduce-to-circles→general-ASNA pattern. Coaxial cone∩cone (axes
    parallel AND apex₂ on the cone₁ axis line) reduces along the shared axis via
    `|t|·tanα₁ = |t−δ|·tanα₂` (`δ` = signed apex offset, `t` = axial height from
    apex₁) to the quadratic `(m₁²−m₂²)t² + 2m₂²δt − m₂²δ² = 0`. **No manufactured
    discriminant/√ sign gate (P9/P10):** the discriminant `(2m₁m₂δ)²` is a
    **perfect square** ⇒ always ≥0, so unequal-α offset input is *always* exactly
    two circles; the equal/unequal split and the apex-collapse are gated on the
    **linear** quantities `|α₁−α₂|` and `|δ|`, never on a square. Branches: X2 (two
    circles at `t=(−m₂²δ±m₁m₂|δ|)/(m₁²−m₂²)`, larger-t first) / X1 (equal α, offset
    → one circle at the bisector `t=δ/2`) / X0 (unequal α, apexes coincide →
    `Ok(vec![])` radius-0 point-circle) / **CO** (equal α + coincident → identical
    double cone → `Err(DegenerateInput)`) / **NC** (non-coaxial →
    `Err(AnalyticalSolutionNotAvailable)`, staged, never a fallback) / E1 (bad α
    either cone, zero axis either cone → `DegenerateInput`). Reuses `Circle` — no
    new curve type, no enum-match migration. RED enforces the anti-hack invariant
    (unequal-α × δ≠0 sweep asserting `len()==2` always). Adversary (10 attacks): no
    bugs; parallelism + on-axis gate boundaries (each flips at `TAU_MODEL`), the
    `|α₁−α₂|` equal/unequal and `|δ|` collapse boundaries, reversed/antiparallel
    axis-sign set-invariance, α near both E1 limits, a 40-config determinism sweep,
    cross-branch argument-swap symmetry, and characterized absolute-`TAU` ceilings
    (on-surface oracle holds to ~1e8 → breaks at 1e9; coaxial band conservatively
    flips to ASNA at large scale) plus the apex-grazing radius-0 (X0) collapse.
    Spec `specs/ssi_pr_ssi9_cone_cone_coaxial.md`; commits `da98380e` (spec)→
    `cc61f1bb` (RED)→`f960895d` (GREEN)→`6027d7c1` (adversary). 245/245 ssi-rs
    tests; CI clean.
  - **Step 10 — `ssi-rs` cylinder∩cylinder parallel axes → lines (PR-SSI10) ✅
    DONE.** First of the two cyl∩cyl special cases. Parallel-axis cyl∩cyl reduces
    to **circle∩circle** in the plane ⟂ the shared axis `û`, lifted along `û` →
    **lines** (reuse `SsiCurve::Line`). Inter-axis distance `d = |rel − (rel·û)·û|`;
    chord offset `a = (d²+r₁²−r₂²)/(2d)` along `n̂`, half-chord `h = √(r₁²−a²)`,
    `p̂ = û×n̂`, points `Q₁+a·n̂ ± h·p̂`. Gate on the **linear** `d` vs `r₁±r₂`:
    E1 (`DegenerateInput`) → NP (`|û₁×û₂|≥TAU` → ASNA) → coincident (d≤TAU, equal r
    → `DegenerateInput`, 2D overlap) → concentric (d≤TAU, unequal r → empty) →
    disjoint/contained (empty) → tangent (one line) → secant (two lines, +h·p̂
    first). Non-parallel stays ASNA (the equal-R intersecting → ellipses case is
    PR-SSI11). Reuses `Line` — no new curve type, no enum-match migration.
    **Adversary found a real bug:** a non-finite `axis_point` (NaN/Inf) leaked a
    NaN-bearing `Line` instead of erroring — `d=NaN` compares false against every
    branch threshold, so control fell through to the secant branch; the radius and
    axis_dir guards did not cover the point. Fixed with an early `axis_point`
    finiteness guard → `DegenerateInput` (the coaxial-detect siblings degrade to
    NC→ASNA on a NaN point, so the leak was unique to cyl∩cyl's curve-producing
    fall-through). Adversary 13 attacks (parallelism/tangent/coincident boundaries,
    argument-swap line-SET symmetry, antiparallel/non-unit axes, 36-config
    determinism sweep, characterized absolute-`TAU` oracle ceiling ~1e8 via an
    oblique config). **Process note:** the SSI10 worker hit the account usage limit
    mid-cycle after committing spec+RED; the interactive driver completed GREEN
    against the worker's frozen RED suite (test-author ≠ implementer preserved),
    spawned a distinct Adversary sub-agent, and fixed its finding. Spec
    `specs/ssi_pr_ssi10_cylinder_cylinder_parallel.md`; commits `fed67c3c` (spec)→
    `b53e55a2` (RED)→`7100c143` (GREEN)→`f0927aec` (adversary)→`721d7b23` (fix)→
    `7a18bb66` (adversary verify). 277/277 ssi-rs tests; fmt + clippy clean.
  - **Step 11 — `ssi-rs` cylinder∩cylinder equal-R intersecting axes → two
    ellipses (PR-SSI11) ✅ DONE.** Second of the two cyl∩cyl special cases, and
    the LAST circle/conic-reducible quadric pair. Two cylinders of **equal radius**
    whose axes are **coplanar and intersect** (non-parallel) meet in **two
    ellipses** lying in the angle-bisecting planes (Patrikalakis & Maekawa §5.8) —
    reuses the existing `SsiCurve::Ellipse` variant, no new curve type, no
    enum-match migration. **Unequal-radius or skew (non-coplanar) axes stay staged
    `Err(AnalyticalSolutionNotAvailable)`** — the general degree-4 curve, deferred,
    never a fallback (A15.2). Built via the role-separated RED/GREEN/ADVERSARY
    cycle (test-author ≠ implementer). Spec
    `specs/ssi_pr_ssi11_cyl_cyl_equal_r_ellipses.md`; commits `7f6e2d44` (RED)→
    `6bdcb05a` (GREEN)→`2e5e6e6f` (adversary). With Step 11, **ALL
    circle/conic-reducible coaxial & special-case quadric pairs are now complete.**
  - **PR-YR6 — curved `Surface`/`Curve` types + loud rejection (first Phase-2
    step). ✅ DONE.** Extends `yang-rs`'s `Surface` enum (`Sphere`, `Cylinder`,
    `Cone`) and `Curve` enum (`Circle`, `Ellipse`) with field shapes mirroring
    `ssi-rs` `QuadricSurface`/`SsiCurve` field-for-field (so the future Stage-3
    mapping is a trivial copy; radially-outward convention, no `sense` field).
    The pipeline **accepts curved faces at the type level** but **rejects them
    LOUDLY** — new `YangError::CurvedSurfaceNotYetSupported { face }` returned at
    the three `Surface::Plane` destructure sites (`BRep::new` winding
    canonicalization is the observable one; `boolean()` `plane_dist` closure and
    `reconstruct_topology` surface inheritance are defensive). P9/P10: never a
    panic, silent skip, or planar approximation. **No `ssi-rs` call and no curved
    tessellation exist yet** — this is a pure type extension. Spec
    `specs/yang_rs_curved_surface_curve_types.md`; role-separated cycle, commits
    `441e8748`/`076bf661` (RED + integration-test contract migration)→`0afdc6a3`
    (GREEN)→`07f6d12e` (adversary).
  - **PR-YR7 — P2a curved Stage-1 tessellation: CYLINDER only. ✅ DONE.**
    First curved-geometry *processing* step. `BRep::new` now dispatches by face
    surface type: a closed-solid cylinder (encoded with a seam edge — lateral
    `Surface::Cylinder` + 2 planar disk caps, no `BRepFace` two-loop change)
    tessellates into a watertight, chord-error-bounded mesh (`d_ε = 1e-2 ×
    AABB_diag`, `N` from `r·(1−cos(π/N)) ≤ d_ε`) with a correct
    `TessellationMap`. A shared per-`Circle`-edge rim-ring pre-pass gives
    cap+lateral identical rim vertices (watertight via shared indices, not
    snap-weld); `ortho_basis` is shared by sampling AND the new infallible
    `BRep::eval_source` bijection inverse (the round-trip oracle); the
    opposite-rim-normal twist is resolved by axis-frame azimuth alignment.
    Adds `signed_distance_to_surface` (Plane+Cylinder; Sphere/Cone loud) wired
    into `boolean()`'s distance closure. **No boolean wiring, no `ssi-rs` call,
    no exact intersection curves.** Sphere/Cone still reject loudly; the planar
    box path is unchanged; `reconstruct_topology` still defers cylinder (P2c).
    Cylinder-on-a-triangle is now `MalformedTopology` (lacks its 2 `Circle`
    rims), not `CurvedSurfaceNotYetSupported`. Spec
    `specs/yang_pr_yr7_cylinder_tessellation.md`; role-separated cycle, commits
    `16570a20` (spec)→`aca9d7e4` (RED + contract migration)→`b3dc3f65`
    (GREEN)→`81a3abcf` (adversary).
  - **PR-YR8 — P2c first curved boolean: cylinder ∪ box (mesh-approximate). ✅ DONE.**
    A curved solid runs through the WHOLE pipeline for the first time:
    `boolean(cylinder, box, Union)` flows Stage 2 (sidecar `LabeledArrangement`)
    → Stage 5/6 reassembly, and a kept lateral patch emits a `BRepFace` carrying
    `Surface::Cylinder` with the **input's exact parameters** (governance A15 —
    the mesh is a tool, the analytic surface is the truth). Two honest fixes:
    **(Blocker 1)** Stage-6 face resolution gains a **per-face membership
    tolerance** — `TAU_WORK` for `Plane`, the surface's own Stage-1 chord bound
    `d_ε` for `Cylinder` (NOT tolerance widening; the same bound Stage 1
    guarantees). The `1e-2 × analytic-AABB-diag` math is extracted into ONE
    shared `curved_chord_bound` helper consumed by both `BRep::new` (Stage-1
    `n_seg`) and face resolution (A14.3 single source). Applied to BOTH the
    non-degenerate count rule AND the degenerate-sliver branch (the sidecar
    emits a near-zero-area sliver ON the cylinder lateral whose centroid is
    ~`d_ε` inside the analytic surface — the §4-literal "keep TAU_WORK" was a
    planar-world assumption; the governing principle applies to any triangle on
    a curved face). Byte-for-byte identical for all-planar inputs (every face
    uses `TAU_WORK`; an all-planar solid has `band == None`) — **fuzz_boxes
    900/900 correct, 0 silent-wrong**. **(Blocker 2)** `reconstruct_topology`
    gains a `Surface::Cylinder` branch BEFORE the planar Newell/flip machinery:
    inherit the surface unchanged (Union = no cavity → no sense flip; curved
    Subtract cavity-sense deferred), reuse `patch_boundary_cycle`, keep the E2
    degenerate-loop guard, DROP the E3/`positive_count` + inherited-normal flip,
    deterministic loop assignment (most-edges = outer, tie-break lowest min
    start-vertex), edges = `Curve::LineSegment`. Sphere/Cone still loudly reject
    everywhere. **Verified against the live Cherchi sidecar:** cylinder ∪ box is
    watertight (0 unpaired half-edges), Euler V−E+F=2, analytic `Surface::Cylinder`
    survives — no F3 tie, no `NonManifoldOutput` (spec §5 STOP conditions all
    clear). **No `ssi-rs` call yet; intersection edges stayed mesh-approximate
    polylines — now made exact in PR-YR9 (P3).** Spec
    `specs/yang_pr_yr8_curved_boolean.md`; role-separated cycle, commits
    `c2a81e05` (RED)→`da85f4bd` (GREEN)→`56f395ba` (adversary).
  - **PR-YR9 — P3 Stage 3: exact intersection edges via `ssi-rs`. ✅ DONE.**
    The **first real use of `ssi-rs` inside the boolean** (Yang 2025 §4.3).
    `cylinder ∪ box` output intersection edges no longer carry P2c
    mesh-approximate `Curve::LineSegment` polylines — they carry the **EXACT
    analytical conic** from `ssi_rs::intersect`. An output intersection edge is
    an undirected mesh boundary edge incident to two patches of **different
    `InputId`** (one on a `Surface::Cylinder`, one on a box-cap `Surface::Plane`);
    `ssi_rs::intersect(Plane, Cylinder)` of those inherited surfaces is the
    plane∩cylinder solver → a `Circle` (cap ⟂ axis, canonical), `Ellipse`
    (oblique), or `Line`s (parallel). New helpers in `crates/yang-rs/src/lib.rs`:
    `surface_to_quadric` (yang `Surface` → ssi `QuadricSurface`; `Plane` point =
    `-d·n`), `ssi_curve_to_curve` (field-for-field; `Line`→`LineSegment`),
    `curve_contains_point` (implicit on-curve residual, no parameter solving),
    and `build_intersection_curves` (per A↔B edge: intersect, select the
    **unique** conic passing both endpoints within the cylinder owner's Stage-1
    chord bound `d_ε` — `TAU_WORK` for plane∩plane — keyed by canonical edge).
    `reconstruct_topology` refactored into two passes (a `PatchInfo` first pass
    owning the face-range check + inherited lookup in one place; an emission pass
    that sets each edge's `curve` via canonical-key lookup, falling back to
    `LineSegment` ONLY for non-intersection edges). The Newell/flip/E2/E3
    machinery is byte-unchanged. **P9 STOP**: a genuine `ssi_rs::intersect`
    failure or a non-unique selection (`matched != 1`) returns
    `Err(YangError::SsiRefinementFailed { edge, reason: SsiRefinementError })` —
    **never** a silent fallback to the polyline. **Scope held**: planar
    `fuzz_boxes` corpus stays all-`LineSegment` (plane∩plane → `Line` →
    `LineSegment`); same-input rim/seam edges keep `LineSegment` (no SSI entry);
    sphere/cone still loudly reject. Adversary proved the conic is analytic, not
    a mesh re-fit (**byte-identical cap `Circle` across N=8 vs N=16 facet mocks**).
    Role-separated cycle, commits `6e73a74d` (RED)→`f1c401f4` (GREEN)→`ec2b71d0`
    (adversary); spec `specs/yang_pr_yr9_stage3_ssi.md`.
  - **PR-YR10 — Stage 4: relocate mesh intersection points onto the exact
    curves + §4.5.3 reversed-point correction. ✅ DONE.** Yang 2025 §4.4.1 +
    §4.5.3 mesh updating. PR-YR9 gave the output edges exact analytical conics,
    but the **mesh** still had its intersection-edge vertices on the faceted
    polygon chords (inside the true circle by up to the Stage-1 chord bound
    `d_ε`). Stage 4 now **relocates** those crossing points radially onto the
    exact `Curve::Circle` (closed-form `project_onto_circle`, reusing
    `ortho_basis` so the angle `t` round-trips through `eval_source`), retags
    each moved/on-curve vertex's `TessellationSource` to `BRepEdge{edge,t}`, and
    runs the §4.5.3 reversed-intersection sweep on the ordered, oriented conic
    loops (discrete tangent `t̃` vs curve tangent; reversal ⟺ unsigned angle ∈
    (45°,135°), 1e-6 rad slack; edge-collapse the next point, reconnect, repeat;
    collinear `t̃` = healthy). **Watertightness is INHERITED** from the
    mesh-boolean output and gated by a combinatorial `check_watertight_2manifold`
    (half-edge pairing + Euler χ=2) — **not** a global CDT (per §4.4.3). Seam:
    `reconstruct_topology` takes `&mut Mesh`, enters Stage 4 on ANY analytic
    conic edge (Circle **or** Ellipse), and returns the per-vertex source vector
    `boolean()` uses for the output `TessellationMap`; Phase-B emission is
    otherwise unchanged. **Verified on the live sidecar**: `cylinder ∪ box`
    relocates every cap-ring crossing onto the exact circle to `TAU_MODEL`,
    chord deviation drops, output stays watertight χ=2; the adversary's
    independent threshold-free geometric audit (1000×1000 per-cap winding sweep +
    net-signed-area = exact analytic region to ~1e-16) found **NO cap fold**.
    **Scope / loud STOPs (P9/P10)**: circle projection only — an `Ellipse`
    intersection edge → `Err(Stage4RegionInvalid{EllipseProjectionUnsupported})`;
    §4.5.2 real local refinement is a loud `LocalRefinementRequired` STOP (the
    canonical fixture never triggers it); `OnAxis`/`OffCurveBeyondChordBand` are
    defensive guards (public-path-unreachable — the upstream YR9
    `curve_contains_point(·, d_ε)` selection is the same, strictly-tighter band,
    so a pathological crossing is rejected by `SsiRefinementFailed`/
    `FaceResolutionFailed` first; a `processed`-set no-skip audit forbids silently
    passing any conic endpoint — the failure mode of the **disproven**
    insert-and-fan attempt, branch `wip/yr10-insert-fan-disproven` commit
    `46980456`, which must NOT be repeated). **Planar path byte-identical**
    (Stage 4 early-returns when no conic edge exists; `fuzz_boxes` 900/900
    unchanged). One faithful spec→reality correction (adversary-verified, NOT a
    hack-to-green): the §4.5 step-4 literal per-facet "winding vs analytic
    normal, dot>0" gate was **removed** — on a faceted curved surface a facet
    normal legitimately deviates from the pointwise centroid normal, and a cap
    facet's kept winding is reconciled downstream by `reconstruct_topology`'s
    Newell orientation pass; orientation correctness is delegated to the §4.5.3
    sweep (loop monotonicity) + the watertight gate, exactly where Yang §4.4.3
    places it. Sphere/Cone still loudly reject. Role-separated cycle, commits
    `5a2da9f0`/`d7540578` (spec)→`d4bbe446`/`03464b29` (RED + fixture
    recalibration)→`e49a5a93` (GREEN)→`d402aa80` (adversary); spec
    `specs/yang_pr_yr10_stage4_relocate.md`.
  - **PR-YR11 — Stage 4 OBLIQUE: relocate onto the exact ellipse. ✅ DONE.**
    Lifts PR-YR10's `EllipseProjectionUnsupported` STOP for oblique
    `cylinder ∪ box`: an `Ellipse` intersection edge now relocates via the
    **cylinder parameterization** (snap radius at angle θ, then snap axial to the
    cutting plane → lands on `cylinder ∩ plane` exactly, closed-form, no quartic;
    Yang §4.3.2), with the §4.5.3 reversal sweep extended to `Ellipse` loops and
    the N3 degenerate-tangent fix preserved. **Verified on the live sidecar:** a
    *contained* oblique `cylinder ∪ box` (tilt `unit([1,0,3])`, axis through the
    box centre so both cap ellipses + the body stay inside the unit box — no
    side-face exit) relocates every crossing onto the exact ellipse (on BOTH the
    cylinder and the cutting plane to `TAU_MODEL`), chord deviation drops, output
    watertight χ=2. yr10 `t4` migrated Err→Ok (the Ellipse edge now relocates, not
    rejects — faithful contract migration). **Out of scope (deferred):**
    side-face-exit / ellipse∩line corner (triple-point) configs — the contained
    fixture avoids them; a loud-STOP guard for them is a follow-up. Commit
    `e72f2313`; `tests/yr11_stage4_ellipse.rs`. 170/170 yang-rs; fmt + clippy clean.
  - **PR-YR12 — P2b sphere Stage-1 tessellation. ✅ DONE.**
    The remaining curved Stage-1 primitive (after the PR-YR7 cylinder). A closed
    solid sphere — one `Surface::Sphere` face bounded by a single `Curve::Circle`
    meridian seam + 2 pole `BRepVertex` (no `BRepFace` two-loop change) —
    tessellates via `BRep::new` into a watertight (χ=2) lat/long grid mesh with a
    bijective `TessellationMap`. Fixed **z-up** parameterization
    `face_eval(u,v)=center+r·(cos v cos u, cos v sin u, sin v)` (a sphere is
    isotropic — an oriented sphere is a documented out-of-scope limitation, like
    the cylinder needing `axis_dir`); chord bound **`d_ε = 1e-2·2r√3`** (the
    sphere's exact AABB space diagonal — diameter `2r` ≠ diagonal); grid `n_lon`/
    `n_lat` refined honestly (segments sized to `d_ε/2` so triangle *centroids*
    stay ≤ full `d_ε` — more triangles, **never** tolerance widening; worst
    centroid dev 0.82·d_ε at n_lon=17/n_lat=9 for the unit sphere). Poles are the
    shared seam-vertex indices and the seam column is reused via modular wrap →
    watertight with no weld/snap/synthetic fill (verified on the live Cherchi
    `inputcheck` sidecar). `eval_source` Sphere FACE arm is byte-identical to
    `face_eval` (round-trip exact to 1e-9 over pole/seam/interior verts);
    `signed_distance_to_surface` Sphere → `|x−center|−r`. The rim-ring pre-pass
    excludes sphere-seam Circle edges so the **cylinder path is byte-for-byte
    unchanged** (`tests/yr7_cylinder.rs` diff empty). **Cone still rejects**;
    sphere-on-a-triangle is now `MalformedTopology` (lacks its seam Circle), a
    faithful guard migration swept across yr6/yr7/yr8/yr9 (cone arms keep their
    exact `CurvedSurfaceNotYetSupported { face: N }` assertions). **No boolean
    wiring, no `ssi-rs`, no exact intersection curves, no NURBS.** Spec
    `specs/yang_pr_yr12_sphere_tessellation.md`; role-separated cycle, commits
    `07c8cbe3` (spec)→`ee66cca3` (RED)→`b5b17e47` (GREEN)→`7e96e070` (adversary).
    184/184 yang-rs; fmt + clippy clean.
  - **PR-YR13 — curved `Subtract` box − cylinder, cavity-sense via
    `BRepFace.reversed`. ✅ DONE.** The first M5 increment after the curved
    `Union` chain (PR-YR8–YR11) and the curved Stage-1 primitives (PR-YR7/YR12).
    Closes the curved cavity-sense gap banked in PR-YR8 for the **`box − cylinder`
    BLIND POCKET** (genus 0, χ=2). A new `BRepFace.reversed: bool` records that a
    face's effective outward normal (outward from the result solid) is the
    **negation** of the surface's canonical analytic outward normal: the surviving
    cylinder-lateral cavity wall is emitted as `Surface::Cylinder` with the input's
    **exact** params and `reversed == true`, so its effective normal points
    **toward the axis** (into the pocket). `reversed` is derived from the SAME
    `flip_for_op` signal that flips the mesh winding —
    `op == Subtract && info.input == InputId::B` (threaded `boolean()` →
    `reconstruct_topology_stage4` → `emit_topology`) — so face sense and mesh
    winding are **provably consistent** (witnessed absolutely: the emitted
    cavity-wall mesh-triangle winding normals point toward-axis and the result has
    positive signed volume). Planar faces keep encoding sense in the
    possibly-flipped `Plane.normal` and stay `reversed == false` (no double-flip);
    surface params are never perturbed to signal sense. Union + planar Subtract
    are byte-identical (`reversed == false` everywhere). Faithful `reversed: false`
    migration swept across all `tests/*.rs` + the `#[cfg(test)]` lib fixtures
    (additive only). Adversary independently witnessed mesh↔`reversed` consistency
    on a second outward-oriented mock + mutation-verified the derivation is
    load-bearing. **Remaining curved-Subtract gaps:** through-hole (genus 1, χ=0),
    sphere/cone cavities (`Cone` still rejects loudly), box-as-subtrahend,
    side-face-exit / corner (triple-point) guard; cut-surface faces (PR-YR5
    deferral) still open. **No new `ssi-rs`.** Spec
    `specs/yr13_subtract_cylinder_cavity_sense.md`; role-separated cycle, commits
    `c4abc69d` (spec)→`78f73f65` (RED)→`42972890` (GREEN)→`3839f558`/`41819459`
    (RED fixups)→`85abbc10`/`86791834` (adversary). 195/195 yang-rs; fmt + clippy
    clean.
  - **PR-YR14 — through-hole genus-1 Subtract; per-shell Euler gate generalized
    to χ=2−2g. ✅ DONE.** Extends curved `Subtract` from PR-YR13's BLIND POCKET
    (genus 0, χ=2) to a **THROUGH-HOLE**: the cylinder passes fully through the
    box (both caps OUTSIDE the box) → a cylindrical tunnel, which is a single
    connected closed orientable 2-manifold of **genus 1 → χ = 0**. The ONE
    production change is in `check_watertight_2manifold`: the per-shell Euler gate
    was `V−E+F == 2` ("each shell is a sphere"), which wrongly rejected the χ=0
    result. Generalized to accept **χ = 2−2g for g ≥ 0** (χ even, ≤ 2) and reject
    odd χ or χ > 2 — impossible for a closed orientable manifold, so still a LOUD
    `NonManifoldOutput` (NOT a tolerance/fallback relaxation; P9/P10). The directed
    half-edge pairing loop stays strict and untouched. Everything else the
    through-hole needs already worked and is REUSED unchanged: the curved cavity-
    sense (`BRepFace.reversed` from `op==Subtract && input==B`, PR-YR13) on the
    tube wall, the annular box top+bottom faces (PR-YR5c multi-cycle /
    `positive_count==1`), the two-rim tube wall (one connected same-attribution
    patch → `patch_boundary_cycle` returns its two boundary cycles → curved branch
    emits outer+inner loops), and the **two** exact `Circle` rim edges (cylinder ∩
    box-top at z=2 AND cylinder ∩ box-bottom at z=0). Adversary independently
    witnessed mesh-winding ↔ `reversed` consistency on a SECOND outward-oriented
    through-hole mock (r=1.5, N=24, signed_volume>0, χ=0, wall winding toward-axis)
    and mutation-verified the χ relaxation is LOAD-BEARING for the accept path
    (reverting to `!= 2` turns the through-hole oracles red). Honest coverage note:
    the χ-clause's REJECT branch (odd/`>2`) is mutually shadowed on the reachable
    corpus by the half-edge-pairing and coincident-triangle guards — defects are
    still loudly rejected, never `Ok` (oracle `a6` pins this). All genus-0 cases
    (`fuzz_boxes`, YR8–YR13, YR13 blind-pocket χ=2) byte-unchanged. **Remaining
    curved-Subtract gaps:** sphere/cone cavities (`Cone` still rejects loudly),
    box-as-subtrahend, side-face-exit / corner (triple-point) guard; cut-surface
    faces (PR-YR5 deferral) still open. **No new `ssi-rs`.** Spec
    `specs/yr14_subtract_through_hole.md`; role-separated cycle, commits
    `36d2a7c4` (spec)→`aeefb4cc` (RED)→`b52a78a3` (GREEN)→`6995b6ec` (adversary).
    208/208 yang-rs; fmt + clippy clean.
  - **PR-YR15 — box − sphere HEMISPHERICAL DIMPLE (genus 0); `Surface::Sphere`
    wiring + `sphere_chord_bound` single-source helper. ✅ DONE.** Extends the
    curved `Subtract` cavity path to a **spherical** cavity: a sphere centred ON
    one box face (poking through exactly that face) so `box − sphere` carves a
    hemispherical dimple — a single shell, **genus 0 (χ=2)**, ONE exact great-
    `Circle` rim (`sphere ∩ box-face plane`, great because the centre is ON the
    plane), and a cavity wall that is the inside hemisphere (`Surface::Sphere`,
    `reversed=true`, effective outward normal pointing INTO the dimple toward the
    centre). The cavity-sense mechanism (`BRepFace.reversed` from
    `op==Subtract && input==B`, PR-YR13) and the per-shell Euler gate (χ=2−2g,
    PR-YR14) are surface-agnostic and REUSED unchanged. The work was honest wiring
    of an already-type-supported surface, NOT new mechanism: `Surface::Sphere` was
    loudly rejected at production sites that each mirror the existing `Cylinder`
    arm. **The plan named three sites; the live-sidecar oracle surfaced two more**
    — both governed by the spec's I-sphere-band invariant (a sphere face uses its
    OWN Stage-1 chord bound `sphere_chord_bound(radius)=1e-2·2r√3`, NOT the rim-
    AABB `curved_chord_bound`=2r√2, which underestimates). Final FIVE faithful
    edits: (1) `surface_to_quadric` Sphere→`QuadricSurface::Sphere` (enables the
    exact `plane ∩ sphere` rim); (2) `sphere_chord_bound` free helper extracted
    from `tessellate_sphere_face` (A14.3 single source) + the `tol_for` Sphere arm;
    (3) `emit_topology` curved-branch guard broadened to
    `Cylinder | Sphere` (body unchanged — already surface-agnostic);
    (4) `build_intersection_curves` selection tol (factored to
    `chord_tol_for_curved_owner`; sphere arm uses `sphere_chord_bound`);
    (5) `stage4_chord_band` relocation budget via new `input_curved_chord_bound`
    (max of rim-AABB and per-sphere-face bound). **No tolerance widening, no
    fallback** — each uses the surface's GUARANTEED Stage-1 bound; `Cone` stays a
    loud reject at every site; cylinder/all-planar inputs byte-for-byte. Adversary
    mutation-verified BOTH extra sites are load-bearing (each mutation reds a
    distinct oracle: `AmbiguousCurve{matched:0}` and `Stage4RegionInvalid{
    OffCurveBeyondChordBand}`), witnessed mesh-winding ↔ `reversed` consistency on
    a SECOND independent OUTWARD-authored off-axis mock (center (1,−0.5,5), r=1.5,
    different facet counts; χ=2, signed_volume>0, cap winding toward-centre), and
    confirmed no migration weakened. **Honest coverage note:** the two extra
    band sites are exercised only via the sidecar-backed oracle (the C++
    `mesh_booleans` binary is present in this env, so they ARE verified here); the
    mock-driven oracles 1–4 bypass real SSI/Stage-4, so on a machine WITHOUT the
    sidecar those two edits are untested — a future mock-path Stage-3/4 sphere-rim
    oracle would close that gap. All prior cases (`fuzz_boxes`, YR8–YR14)
    byte-unchanged. **Remaining curved-Subtract gaps:** cone cavities (`Cone`
    still rejects loudly), fully-internal spherical voids (multi-shell),
    through-sphere, box-as-subtrahend, side-face-exit / corner (triple-point)
    guard. **No new `ssi-rs`** (the `plane_sphere` solver already existed). Spec
    `specs/yr15_subtract_sphere_dimple.md`; role-separated cycle, commits
    `4b0b5af0` (spec)→`945c1c12` (RED)→`6e6239f0` (GREEN)→`bcdeecc4` (adversary).
    219/219 yang-rs; fmt + clippy clean.
  - **PR-YR16 — CONE Stage-1 tessellation (CONE only, no boolean); all three
    curved primitives now tessellate. ✅ DONE.** The cone was the last curved
    primitive still rejecting everywhere (`Surface::Cone` →
    `CurvedSurfaceNotYetSupported`). This PR teaches `BRep::new` to tessellate a
    closed solid cone, verified by the same 4-oracle contract as PR-YR7/YR12
    (surface-to-mesh ≤ d_ε, watertight + 2-manifold + env-gated `inputcheck`,
    bijection round-trip, Euler χ=2) over a corpus of 4 (z-up unit, wide-short,
    tall-thin, off-axis non-unit axis). **Encoding (minimal, justified):** the
    cone lateral is topologically a DISK — its only boundary is the base circle,
    the apex is a single interior singular point — so NO seam edge (unlike the
    cylinder). `verts=[apex, base_seam]`; one shared base-rim `Curve::Circle`
    (shared by lateral + base cap = the watertightness mechanism); faces
    `[Cone lateral, Plane base cap]`. The apex is a pre-seeded `BRepVertex`
    located in `tessellate_cone_face` by exact `TAU_MODEL` position match (no
    duplicate → watertight + Euler hold; round-trip via `BRepVertex`).
    **Tessellation = apex fan + base cap fan** over the shared rim ring (cap
    reuses the existing `tessellate_cap_face` unchanged). Because the cone is
    **ruled** (straight generators apex→rim, exactly on the surface), the worst
    residual anywhere on a lateral triangle — including its centroid — is the
    base-rim sagitta `R·(1−cos π/N)`; there is NO centroid amplification (unlike
    the sphere) and NO `/2` factor in N-sizing. **`cone_chord_bound(h,α)=
    1e-2·√((2R)²+h²)`** is the new single source (A14.3), folded into the rim
    pre-pass via `min(curved_chord_bound, cone_chord_bound)` ONLY when a cone
    face is present (cylinder/sphere/planar paths byte-for-byte) — the min is
    load-bearing for wide-short cones (`h<2R`, where the rim-AABB bound
    overestimates the honest bound). **Tilted outward normal** (A15.5)
    `n̂=unit(r̂−tanα·â)` ⟂ the generator (new `cone_outward_normal`). Three Stage-1
    production sites changed (`BRep::new` dispatch, `eval_source` cone-FACE arm,
    `signed_distance_to_surface` signed radial residual); the boolean-path cone
    rejections (`surface_to_quadric`, `emit_topology`, Stage-6 reassembly) stay
    LOUD — the cone never enters the boolean this PR. After this PR
    `CurvedSurfaceNotYetSupported` is no longer reachable from `BRep::new` for any
    curved surface on a triangle (all → `MalformedTopology`); it survives only on
    the boolean Stage-6 paths. **Guard migration:** the plan named 9 sites; the
    spec sweep surfaced 3 more (+1 inline `src/lib.rs` test) — exactly the
    under-enumeration the `yang_curved_primitive_guard_migration` lesson and the
    YR15 precedent anticipate — all migrated faithfully (only the expected
    outcome changed; every structural assertion preserved). **Two honest
    adversary findings (documented, no defect):** (1) oracle 1's 3-verts+centroid
    sampling could not distinguish the rim-only N from the correct N (the
    distinguishing sample is the base-edge MIDPOINT at f=1) — the adversary added
    `adv_wide_short_base_edge_midpoint_within_cone_bound`, which reds the
    dropped-`min` mutation with the exact residual-exceeds-bound assertion, so the
    min IS mutation-verified load-bearing; (2) the tilted normal produces a
    BYTE-IDENTICAL mesh to the pure-radial normal for the current pure apex-fan
    (`orient_tri`'s binary flip is identical at every steepness since both r̂ and
    n̂ share the fan triangle's half-space) — it is the *correct* surface normal
    but **orientation-dead-code until interior-ring (non-fan) triangles appear**
    (i.e. PR-YR17 cone cavity → a YR17 winding canary). The adversary pinned the
    math witness (`n̂·ĝ≈0` vs `r̂·ĝ≈0.565`) rather than fabricating a false catch.
    Independent second off-axis mock witnessed outward winding + `signed_volume>0`
    (per `yang_mock_orientation_witness`). 232/232 yang-rs; fmt + clippy
    `--all-targets` clean. Spec `specs/yr16_cone_tessellation.md`; role-separated
    cycle, commits `8e569c14` (spec)→`8ceb8d65`/`8d2fe8c6` (RED + clippy
    fixup)→`7f0dfe4e` (GREEN)→`6013a1fc` (adversary). **Next: PR-YR17 cone cavity
    `box − cone`.**
  - **PR-YR17 — box − cone CONICAL POCKET (curved `Subtract`, genus 0). ✅ DONE.**
    Closes the loop PR-YR16 opened: a cone with its **apex inside the box** (pocket
    bottom, `(0,0,0.5)`) and its **base above the box top** carves a conical pocket
    via `box − cone`. Result = a single genus-0 shell (χ=2): cavity wall = the cone
    lateral apex→rim (`Surface::Cone`, `reversed == true`), rim = exact `Circle`
    (`cone ∩ box-top plane`, a **perpendicular** cut → `ssi-rs` `plane_cone` C1
    branch → `radius = |h|·tanα`), apex = a singular pocket-bottom vertex closing
    the fan, box-top = an annular planar face (rim hole). This is pure
    **composition** — the cavity-sense mechanism (`BRepFace.reversed = op==Subtract
    && input==B`) is surface-agnostic and unchanged; the job was flipping the cone's
    loud-rejects to real wiring mirroring the Cylinder/Sphere precedent.
    **Production sites (`src/lib.rs` only, NO `ssi-rs` change):** the spec named
    FOUR — `emit_topology` curved-branch `matches!` (admit `Surface::Cone`),
    `emit_topology` defensive arm (drop Cone), `surface_to_quadric` (field-for-field
    → `QuadricSurface::Cone`, enabling the exact rim `Circle`), and `tol_for` (the
    cone's OWN `cone_chord_bound`, **height derived from the rim Circle in the cone
    face's outer loop** via the Stage-1 pre-pass idiom `|(rim_center−apex)·â|` —
    single-source bound, NOT tolerance widening). The GREEN implementer surfaced a
    **FIFTH** site (exactly the `yang_curved_primitive_guard_migration`
    under-enumeration the YR15/YR16 cycles anticipate): `build_intersection_curves`
    Stage-4 rim-curve selection had Cylinder/Sphere chord-tol arms but no Cone arm,
    so a cone∩plane rim edge fell to `TAU_WORK` and failed `curve_contains_point`
    against the exact circle (`AmbiguousCurve{matched:0}`). Added
    `cone_chord_tol_for_owner`, a faithful mirror of `chord_tol_for_curved_owner`
    (same loud-on-missing-rim producer-fault path, the cone's own single-source
    `cone_chord_bound`). **Confirm-or-STOP (P9/P10):** the `tol_for` cone-height
    anchor was verified (temporary `eprintln!`, removed) before coding; no
    widening, no fallback. **Honest adversary findings (no defect):** a second,
    distinct conical-pocket mock (shallow box, apex `(0,0,0.25)`, `tanα=2`) witnessed
    winding ↔ `reversed` sampling **edge midpoints**; mutation-verified that flipping
    `reversed` (M1), perturbing the cone params (M2), killing the SSI Cone arm (M3a),
    and breaking the fifth-site bound→`TAU_WORK` (M3b, reds the sidecar oracle), and
    flipping the tilted-normal **sign** (M4) each red a DISTINCT oracle. **M4b**
    (pure-radial, *correct* sign) reds NO oracle — **confirming** the YR16 finding:
    the cone cavity wall is still a pure apex-fan, so `orient_tri`'s binary flip is
    byte-identical for `r̂` and `n̂=unit(r̂−tanα·â)`; the **sign** is load-bearing, the
    tilt **magnitude** stays orientation-dead-code until interior-ring (non-fan)
    triangles appear (per `yang_cone_tessellation_oracle_findings`). The fifth site
    is verdicted a **faithful extension, not a tolerance hack** (M3b proves the bound
    does real work). The env-gated `Subtract` sidecar-parity oracle ran for REAL
    (default sidecar present), exercising the full pipeline (real `plane_cone` →
    Circle) end-to-end. Curved `Subtract` now covers **cylinder + sphere + cone**.
    **Still deferred (LOUD):** through-cone / cone-base-subtracted (two rims),
    **oblique cuts** (ellipse / parabola / hyperbola rims — the `plane_cone` non-C1
    branches), fully-internal cone void (multi-shell), side-face / corner
    (triple-point) exit, box-as-subtrahend. Full `yang-rs` crate green; fmt + clippy
    `--all-targets` clean. Spec `specs/yr17_subtract_cone_cavity.md`; role-separated
    cycle, commits `f9d597d8` (spec)→`f6a06012` (RED)→`f21434a1` (GREEN)→`741b50f1`
    (adversary).
  - **Non-convex / holed planar Stage-1 tessellation** ✅ (PR-NC1): planar faces
    with a reflex vertex (non-convex outer loop) **or** inner loops (holes) now
    tessellate via a constrained Delaunay triangulation
    (`cherchi_rs::cdt_polygon_with_holes`, backed by `spade` v2) instead of the
    fan path. No interior Steiner points, no boundary subdivision (the
    `TessellationMap` 1:1-on-boundary bijection is preserved); convex/box faces
    stay byte-for-byte on the existing fan path (`fuzz_boxes` 900/900
    unregressed). Resolves the D1-class (no ear-clipping) concern for the new
    kernel's planar Stage 1. Spec `specs/yang_pr_nc1_nonconvex_cdt.md`; deviation
    ledger **N9**.
  - **Curved boolean fuzz (the robustness-envelope map)** ✅ (PR-CF1): the curved
    analog of `fuzz_boxes` — a deterministic N=300 **correct-or-loud** fuzz over
    `boolean({cylinder|sphere|cone}, box, {Union|Subtract}, &sidecar)`
    (`tests/fuzz_curved.rs`, SEED=`0xcf1cadef00d2026`). Every `Ok` is audited
    (watertight `unpaired==0` / χ **==sidecar-reference χ** & even / analytic-surface
    survival with exact params / on-surface residual ≤ `TAU_MODEL` sampled on the
    exact `Curve::Circle/Ellipse` against BOTH incident surfaces / `vol>0` /
    chord-band volume envelope scaled from the Stage-1 `d_ε`); every `Err` is
    bucketed by `YangError` variant **and sub-reason**. Empty-result agreement
    (both engines ∅ → `ok_correct`; disagreement → silent-wrong) is a deliberate
    contract interpretation, not a relaxation. **Histogram (300 cases, all
    accounted for): `ok_correct=42`, `SILENT_WRONG=0`, `classified_err=257`,
    `skipped_bad_input=0`.** This **is the M5-gap map**: the dominant loud refusals
    are `SsiRefinementFailed::AmbiguousCurve` (183 — the SSI rim-selection gap),
    `FaceResolutionFailed` (54), and cone's `Stage4RegionInvalid::LocalRefinementRequired`
    (17); cone Union/Subtract are 0/0 correct (all loud refusals — oblique/ssi gaps),
    while sphere & cylinder land some correct results. Most `Subtract` cases are
    correctly loud because `boolean(prim, box, Subtract)` = `prim − box` =
    **box-as-subtrahend**, the DEFERRED direction (opposite of the `box − prim`
    demos). **ONE genuine production defect surfaced (a P9 violation): `boolean()`
    PANICKED** on sphere − box at case#23 — `emit_topology`'s curved branch indexed
    `cycles[outer_idx]` on an **empty** `cycles`. **GREEN fix** (`src/lib.rs`,
    commit `a568d9e6`): a minimal `if cycles.is_empty() { return
    Err(NonManifoldOutput); }` guard on the curved branch (+ a defensive mirror on
    the structurally-identical planar branch, latent since the all-planar fuzz never
    produces empty cycles), mirroring the adjacent E2/E3 degenerate-reassembly
    guards — converting the panic into a loud classified `Err`, so the fuzz now holds
    correct-or-loud (panic→Err ⇒ `PANICKED=0`). **Mechanism correction (adversary):**
    the case#23 sidecar reference is **NON-empty** (272 tris, vol≈0.0485) — the empty
    curved cycles are a reassembly artifact of the **deferred box-as-subtrahend
    direction**, NOT an enclosed-sphere empty solid; the loud `Err` is a legitimate
    refusal of an out-of-scope direction, not a suppressed wrong `Ok`. **Adversary
    verdict (`tests/cf1_adversary.rs`, commit `0771dcc6`):** all invariants real and
    discriminating (proved inside-out caught by `vol>0`; determinism replays case#23
    from SEED), GREEN fix principled; one noted **non-blocking GAP** — the chord-band
    volume alone is a coarse dropped-chunk detector (~0.38 at r=0.6), so χ==ref +
    on-surface residual are the real dropped-chunk gates (by design). The asserting
    fuzz stays `#[ignore]`d (default `cargo test -p yang-rs` green); an `#[ignore]`d
    `demonstrator_case23_*` pins the seed. **Follow-up increments seeded by the
    histogram:** the SSI `AmbiguousCurve` rim-selection gap (the single biggest
    blocker to curved `ok_correct`), `FaceResolutionFailed` coverage, cone Stage-4
    `LocalRefinementRequired`, and eventually the deferred box-as-subtrahend
    direction. Spec `specs/yang_pr_cf1_curved_boolean_fuzz.md`; role-separated cycle,
    commits `f0ea2e24` (spec)→`884726f5` (RED)→`a568d9e6` (GREEN)→`0771dcc6`
    (adversary).
  - **PR-YR18 — Stage-5 intersection-edge attribution fix (the CF1 `AmbiguousCurve`
    dominant-refusal). ✅ DONE.** Re-diagnoses CF1's biggest loud bucket: the
    `SsiRefinementFailed::AmbiguousCurve` mass (183/300 in the CF1 histogram) is
    **NOT** the "SSI rim-selection gap" the CF1 note guessed — a driver
    investigation found **0 cases with `matched ≥ 2`; every `AmbiguousCurve` is
    `matched == 0`**, and the bulk is **cylinder + sphere** (both fully handled by
    `ssi-rs`), not missing conics. It is a **surface-attribution defect**:
    `compute_phase_a` pushes a patch's single `info.inherited` face surface onto
    *every* boundary edge of the patch cycle (`src/lib.rs:3279-3289`), so a seam
    edge shared by two patches gets tagged `(surfA, surfB)` and handed to
    `ssi_rs::intersect` even when **one endpoint is genuinely off one surface**
    (decisive case: a cylinder∩plane edge, `tol≈3.1e-2`, one endpoint on both
    surfaces, the other `~8.9e-2` — ~2.9× the chord band — off the plane). Such an
    edge is an internal facet edge of a *single* surface, not a true intersection
    arc; the returned curve cannot pass through both endpoints → `matched == 0`.
    The SSI math is correct (`candidates == 1`); the defect is the
    **classification**. **GREEN fix (`src/lib.rs` `build_intersection_curves`
    only):** reorder so the Stage-1 chord band `tol` is computed FIRST, then gate
    each candidate edge with an **on-both-surfaces predicate** — both mesh
    endpoints must satisfy `|signed_distance_to_surface(surf, p)| <= tol` for BOTH
    attributed surfaces — *before* `ssi_rs::intersect`. A failing edge `continue`s
    and falls through to the unchanged `Curve::LineSegment` fallback in
    `emit_topology` instead of raising `AmbiguousCurve`. **No tolerance widening
    (P9/P10):** the gate reuses the SAME per-edge `tol` the selection already uses
    (the producer-fault helpers' diagnostic-only `candidates` arg is passed `0` in
    the pre-intersect position — untested). **No-regression invariant (proof):**
    the intersection curve lies ON both surfaces, so any edge that currently
    selects `matched == 1` necessarily passes the gate — the gate is a *necessary
    condition* of existing success and cannot regress YR8–YR17 or the planar
    corpus; it only reclassifies edges that today raise `AmbiguousCurve` with an
    endpoint off a surface beyond `tol`. Coincident-plane / yr9 loud STOPs
    preserved (both endpoints on both surfaces → pass the gate → reach the loud
    path); cone conics stay loud (a true cone∩plane edge passes the gate then still
    hits `matched != 1` because `curve_contains_point` returns `false` for conics —
    correct, the deferred analytic-conic follow-up). **Before/after counts:** CF1
    baseline = `AmbiguousCurve == 183` (cylinder + sphere `matched == 0` bulk). The
    **empirical post-fix sidecar-fuzz histogram could NOT be obtained in this
    container** — the Cherchi sidecar subprocesses zombie out and the
    `fuzz_curved` harness hangs without printing a final histogram (pervasive
    un-reaped `<defunct>`/`Z` processes, some days old, independent of this
    change); repeated N=300/120/40 runs all stalled. Per
    `feedback_no_regression_chasing` / "don't loop", no numbers were fabricated.
    Correctness evidence is instead **deterministic, sidecar-free**: a RED fixture
    (`tests/yr18_attribution.rs`) that reproduces the EXACT cylinder∩plane
    `matched == 0` case (`AmbiguousCurve { candidates: 1, matched: 0 }`, edge
    `(0,1)`, off endpoint 2.90× the band) and goes GREEN under the fix; the
    no-regression invariant (proof, statically audited by the adversary); and the
    adversary over-skip guard (`tests/yr18_adversary.rs`) proving genuine cap-ring
    cylinder∩plane edges still pass the gate and emit `Curve::Circle` (the RED
    test's negative-only assertions cannot catch a degenerate skip-everything
    "fix"). Full `cargo test -p yang-rs` green; `cargo fmt -p yang-rs -- --check`
    and `cargo clippy -p yang-rs --all-targets -- -D warnings` clean. Spec
    `specs/yr18_intersection_edge_attribution.md`; role-separated cycle, commits
    `ea94cc1c` (spec)→`5536432b` (RED)→`2345b791` (GREEN)→`44dc1cde` (clippy
    chore)→docs+adversary. **Empirical delta (driver-verified post-merge, curved
    fuzz N=90 same seed, before→after):** `ok_correct` **11 → 37** (3.4×);
    `AmbiguousCurve` **56 → 30**; **cylinder `AmbiguousCurve` eliminated entirely
    (21 → 0)**; sphere materially improved (20 → 15); cone unchanged (15 → 15, the
    deferred-conic share); **`SILENT_WRONG` 0 → 0** (safety bar held). The worker
    itself could not obtain these numbers (Cherchi sidecar subprocesses zombie out
    in-container); the driver reproduced the run successfully at N=90. **Deferred
    follow-ups:** (a) analytic-conic support (`Parabola`/`Hyperbola` for oblique
    cone cuts) so true cone∩plane edges that pass the gate stop being loud — the
    remaining cone `AmbiguousCurve=15`; (b) the residual **sphere**
    `AmbiguousCurve=15` (a distinct, smaller cause — the gate only partially
    cleared sphere; needs its own diagnosis).
  - **PR-YR19 — sphere∩plane chord-band metric consistency (the residual sphere
    `AmbiguousCurve`). ✅ DONE.** Diagnoses PR-YR18's deferred follow-up (b): the
    15 residual sphere `AmbiguousCurve` cases are all `surf0=Sphere`,
    `surf1=Plane`, `candidates == 1` (sphere∩plane is never ambiguous). The mesh
    endpoints PASS the YR18 on-both-surfaces gate (within `tol` of both surfaces
    along the surface normal) but FAIL `curve_contains_point` because the
    **in-plane radial** deviation `|radial − r_circle|` exceeds the flat `d_ε`,
    even though the **sphere-normal** distance is within `d_ε`. A **metric
    inconsistency**, not a real off-curve point: `d_ε` bounds the surface-normal
    error; a vertex within `d_ε` of the sphere along its normal projects (on the
    cut plane) to an in-plane radial deviation up to `(R/r_circle)·d_ε`
    (derivation: `|p−C| = √(h²+radial²)`, `d/d(radial)√(h²+radial²) ≈ r_c/R` at
    `radial=r_c`, so `dr ≈ (R/r_c)·d_sphere`). When the cut plane is far from the
    sphere centre, `r_c` is small and `R/r_c` is large. **Approach (A)
    projection-scaled radial band** (`src/lib.rs` only): the in-plane radial band
    becomes `(R/r_circle)·d_ε` while the axial (out-of-plane) band stays `d_ε`
    (the cut plane is exact). Surface-type-gated on a `Surface::Sphere` owner via
    `source_radius: Option<f64>` — every non-sphere path (`None`) is byte-identical;
    near-tangent guard (`r_circle > MIN_FEATURE_SIZE`) fails closed. **Two sites,
    both load-bearing:** (1) selection — `curve_contains_point` + caller
    `build_intersection_curves`; (2) Stage-4 relocation — `vert_circle` extended
    to carry the source sphere radius, the combined `circle_residual > d_eps`
    guard split into per-component axial/radial bands via `circle_residual_split`.
    Fixing only site 1 would convert `AmbiguousCurve` → `OffCurveBeyondChordBand`
    with **zero net `ok_correct` gain**, so the success criterion is `ok_correct`
    **rising**, not the `AmbiguousCurve` count alone. **NOT tolerance widening
    (P9/P10):** the band is the exact geometric propagation of the same `d_ε`,
    derived not picked; a point off by more than the propagated band still STOPs
    loudly. RED `tests/yr19_sphere_chord_band.rs` (a small-cap dimple, `r_c≈0.31`,
    `R/r_c≈3.2`, rim verts authored at `dr ∈ (d_ε, (R/r_c)·d_ε)` so the band is
    magnitude-load-bearing without the sidecar) reproduces the `AmbiguousCurve`
    today and goes GREEN under both fixes. Spec `specs/yr19_sphere_chord_band.md`;
    deviation **N11** (cross-refs N10). **Driver-verified empirical delta**
    (curved fuzz N=90, same seed, before→after; the worker could not run the
    sidecar fuzz — `curved_fuzz_sidecar_zombie_blocker`): **sphere `AmbiguousCurve`
    15 → 0 (eliminated)**; sphere `ok_correct` 15 → 30 (Union 4→14, Subtract
    11→16); total `ok_correct` 37 → **52**; total `AmbiguousCurve` 30 → 15 (now
    ALL cone); **`SILENT_WRONG` 0 → 0**. Critically, **no conversion to
    `Stage4RegionInvalid::OffCurveBeyondChordBand`** (sphere has zero Stage-4
    errors post-fix) — confirming the dual-site fix yields real `Ok`, not a
    downstream swap. **Deferred (still LOUD):** the cone analytic-conic share
    (`Parabola`/`Hyperbola`, oblique cone∩plane, the remaining 15) is unaffected
    and stays out of scope.
  - **PR-YR20 — Stage-6 tiered face-resolution tie-break (the largest non-cone
    `FaceResolutionFailed` bucket). ✅ DONE.** A driver investigation (env-gated
    prints, since reverted) found 12/12 sampled curved-fuzz `FaceResolutionFailed`
    cases share ONE uniform root cause — NOT a no-match. Stage-6 geometric face
    resolution (non-degenerate branch) attributes a kept triangle to the input
    face whose surface contains the triangle **centroid** within that face's
    per-face tolerance `tol_for` (`TAU_WORK` for a `Plane`, the Stage-1 chord band
    `d_ε` for `Cylinder`/`Sphere`/`Cone`): exactly 1 hit → attribute, 0 or ≥2 →
    `FaceResolutionFailed`. Every refusal is an `n_hits == 2` tie of one shape —
    a triangle lying **exactly on a planar cap near the rim** (`dist ≈ 5.5e-17`,
    `tol = TAU_WORK = 1e-12` → HIT) ALSO falls inside the curved lateral's
    necessarily-loose chord band (`dist ≈ 7.6e-3`, `tol ≈ 2.4e-2` → HIT) →
    spurious second hit → tie → F3. The rule wrongly treated an **exact**
    `TAU_WORK` planar hit and an **approximate** `d_ε` chord-band hit as equal
    weight; the triangle's true face is the cap. **The fix (tiered tie-break,
    `src/lib.rs` non-degenerate branch only):** rank hits by **tier** — EXACT
    (`dist < TAU_WORK`, the centroid lies ON the surface) dominates BAND
    (`TAU_WORK ≤ dist < tol_for`). Attribute to the unique hit at the minimum
    populated tier; ≥2 at that tier, or no hit, still `FaceResolutionFailed`.
    `tol_for` is untouched — each face keeps its own A14.3 single-source band; we
    only break ties by the exact-vs-band tier. **All-planar byte-identity (the
    critical non-regression):** for a `Plane` `tol_for == TAU_WORK`, so a hit
    (`dist < tol_for`) means `dist < TAU_WORK` ⇒ ALWAYS EXACT tier; the BAND tier
    is unreachable for planar faces. So for an all-planar input the BAND tier is
    empty, `n_exact` == the old hit count, and the new `match` reduces
    **byte-for-byte** to the old "exactly one face within `TAU_WORK`" rule — the
    box fuzz, the m3 coplanar-tie tests, and the yr5c planar-sliver tests are
    unaffected, and genuine coplanar / multi-solid ties (≥2 EXACT) still STOP
    loudly. **Tier-by-distance, NOT a `dist/tol` ratio:** a ratio would
    distinguish two sub-`TAU_WORK` planar hits and silently flip a current planar
    F3 to an attribution, breaking that safety property. **NOT tolerance widening
    (P9/P10):** `TAU_WORK` is the existing planar tolerance reused as the tier
    boundary, not a new looser constant. The degenerate-sliver branch is left
    unchanged (it never raises F3 for a tie; minimal regression surface). RED
    `tests/yr20_tiered_tiebreak.rs` (a closed-cylinder boolean with a near-rim
    cap triangle authored at the `n_hits == 2` cap-vs-lateral tie, tie magnitude
    asserted load-bearing without the sidecar) + an all-planar coplanar-tie safety
    canary that MUST still F3; adversary adds a 0-EXACT + 2-BAND two-cylinder
    curved tie that MUST still F3. Spec
    `specs/yr20_face_resolution_tiered_tiebreak.md`; deviation **N12** (refines
    N4). **Calibrated metric:** total `FaceResolutionFailed → ~0`, cylinder
    `ok_correct` rises (the cap-tie unblocks it), **ZERO new silent-wrong / no new
    `NonManifoldOutput`**. **Driver-verified empirical delta** (curved fuzz N=90,
    same seed, before→after; worker hit `curved_fuzz_sidecar_zombie_blocker`, did
    NOT fabricate): **total `FaceResolutionFailed` 16 → 0 (eliminated)**; cylinder
    `ok_correct` 22 → 31 (Subtract now 12/12, Union 19/20 — the 1 remaining is an
    unrelated `NonManifoldOutput`); total `ok_correct` 52 → **61**; **`SILENT_WRONG`
    0 → 0**. As calibrated, cone `FaceResolutionFailed` 7 → 0 but cone `ok_correct`
    stayed 0 — the refusal shifted to the deferred `AmbiguousCurve` conics (15 → 21)
    + `LocalRefinementRequired`, exactly the intended sibling-variant shift, not a
    real failure. **Deferred (still LOUD):** cone `ok_correct` stays 0 — a cone
    triangle that stops being an F3 tie simply refuses later for the deferred
    analytic-conic reason (`Parabola`/`Hyperbola`, oblique cone∩plane; see N7 /
    N10 / N11). That is correct, not a regression.
  - **Cone analytic-conic sequence (PR-YR21→YR24, PLANNED 2026-06-03).** Cone is
    `0/26` in the curved fuzz, blocked across ALL non-perpendicular sections. The
    analytic math is DONE in `ssi-rs` (`plane_cone` returns Circle/Ellipse/
    Parabola/Hyperbola); this is purely `yang-rs` integration. Missing pieces:
    `Curve::Parabola`/`Hyperbola` variants, their `ssi_curve_to_curve` +
    `curve_contains_point` arms, `eval_source` parametric eval, and — the
    keystone — **Stage-4 relocation for cone sections** (the existing ellipse
    relocation is hard-wired to the *cylinder* parameterization, YR11 §4.3.2, so a
    cone+plane edge hits `LocalRefinementRequired` at lib.rs ~3616; this breaks
    cone ELLIPSE too, not just parabola/hyperbola). **Design:** a
    **cone-section parameterization** relocation (`project_onto_cone_section`):
    for a mesh vertex take its angle θ around the cone axis, intersect that
    generator with the cutting plane → the exact conic point. Type-AGNOSTIC
    (ellipse/parabola/hyperbola identical), the cone analog of YR11's
    cylinder-ellipse projector, avoids generic foot-of-perpendicular quartics.
    **Sequence:**
    - **PR-YR21 — cone-section relocation foundation + cone ellipse. ✅ DONE
      (2026-06-04).** Shipped `project_onto_cone_section` (closed-form: relocate a
      vertex along its azimuth's generator `g = nappe·cosα·â + sinα·r̂`, solving
      `s` so `apex+s·g` lies on the cutting plane → on BOTH cone and plane = on the
      conic; type-agnostic, reused by YR22/YR23) + `ConeEllipseReloc` +
      `cone_chord_budget_from_owner` (per-cone-face budget `cone_chord_bound`,
      height from the cone owner's rim Circle — the single source). The Stage-4
      `Curve::Ellipse` arm now branches on incidence: cylinder+plane → the YR11
      path **byte-identical**; cone+plane → the new cone relocation loop; neither →
      the existing loud STOP. cone ELLIPSE now lands end-to-end (RED oracle1/2/3/4 +
      **real-sidecar E2E oracle8** green; held loud-STOPs — asymptotic/through-apex/
      parabola/hyperbola — stay LOUD per oracle6 + the adversary suite). Zero crate
      regressions; cyl/sphere `stage4_chord_band` untouched. **Loud STOPs (P9/P10):**
      `OnAxis` (ρ<MIN_FEATURE_SIZE), `LocalRefinementRequired` for `|n·g|≈0`
      (generator ∥ plane / asymptotic) and `s≤0` (wrong-nappe / through-apex).
      **Findings:** (1) the *spec's "secondary site"* Stage-4 cone budget gate
      `OffCurveBeyondChordBand` is **defensively redundant** — it is shadowed by the
      identical upstream `on_both` gate in `build_intersection_curves` (same
      `cone_chord_bound` tol), so a beyond-band vertex is demoted to `LineSegment`
      before Stage-4 (adversary-verified; kept as a fail-closed backstop, not
      load-bearing through the public surface). (2) oracle3's chord-deviation tight
      check inherited yr11's coarse 200k-sample `dist_to_ellipse_sampled` whose
      ~1.8e-5 half-spacing floor (perimeter-7.26 ellipse) cannot resolve TAU_MODEL
      — fixed with a resolution-independent two-level refined sampler; the rigorous
      on-ellipse guarantee remains enforced by oracle2 + the real sidecar. **Step-0
      cone-refusal split / curved-fuzz `ok_correct` delta deferred to the driver**
      per the `curved_fuzz_sidecar_zombie_blocker` (the bounded E2E oracle8 on the
      real `mesh_booleans` binary stands in as the live-boolean proof; no fabricated
      fuzz numbers). Gate: cone ELLIPSE `LocalRefinementRequired` → 0 (mock + real
      sidecar). **Driver-verified delta + Step-0 split** (curved fuzz N=90, same
      seed, before→after): cone `ok_correct` **0 → 5** (Union 2, Subtract 3 — cone's
      FIRST successes); cone `LocalRefinementRequired` **5 → 0** (eliminated); total
      `ok_correct` 61 → **66**; `SILENT_WRONG` 0 → 0. **Cone-refusal split** (of the
      26 cone cases): **5 ellipse** (now ✅), **21 parabola+hyperbola** (the
      `AmbiguousCurve`, YR22/YR23 targets), **0 axis-parallel/through-apex** in this
      sample. **Next: PR-YR22 (parabola), then YR23 (hyperbola) — target the 21.**
    - **PR-YR22 — Parabola end-to-end. ✅ DONE (2026-06-04).** `Curve::Parabola`
      (mirrors `SsiCurve`) + `ssi_curve_to_curve` + `curve_contains_point` +
      `parabola_point(t)` eval (`vertex + (t²/4f)·axis_dir + t·(normal×axis_dir)`)
      + point→t (conjugate-axis) inversion; Stage-4 relocation reuses YR21's
      `project_onto_cone_section`. **Recovered finish-from-RED across a session
      limit:** the worker's RED phase (6 oracles + a real-sidecar E2E oracle8 +
      the migrated yr21 oracle6) was preserved and committed; a GREEN subagent
      implemented production; it STOPPED at a verified RED-author fixture/oracle
      bug (oracle4's per-triangle winding check false-positived on the mock's
      ring-closure scaffold). Resolution (driver + second-opinion review +
      adversary): **reframe oracle4** to the invariant production actually enforces
      — boolean output is a consistently-oriented watertight 2-manifold (0 unpaired
      half-edges, χ=2, signed volume > 0) + the per-facet degenerate-area floor —
      since production deliberately does NOT do a per-facet winding test
      (`validate_relocated_triangles`, Yang §4.4.1/§4.4.3). Adversary added 9
      canaries (no silent-wrong, eval round-trip vs independent re-impl, no
      ellipse/cylinder/circle regression, hyperbola+axis-parallel stay loud, local
      fold breaks watertight). Full `cargo test -p yang-rs` green; fmt+clippy clean.
      Commits `3cf1f482` (RED)→`18909a5d` (GREEN)→`4fc114f2` (oracle4 reframe)→
      `955ef698` (adversary). **Fuzz delta = 0 BY CONSTRUCTION** (driver-verified
      N=90, unchanged from YR21): an exact parabola section needs the cut plane
      EXACTLY parallel to a generator (θ=α), which is **measure-zero** in the random
      fuzz — random box cuts give ellipses (YR21) or hyperbolas, never exact
      parabolas. So the parabola capability is real (proven by the θ=α oracles +
      E2E) but invisible to the random fuzz. **⇒ the 21 remaining cone
      `AmbiguousCurve` are (near-)all HYPERBOLA — PR-YR23 is the high-leverage one
      for the cone fuzz number.**
    - **PR-YR23 — Hyperbola end-to-end. ✅ DONE (2026-06-04).** `Curve::Hyperbola`
      (mirrors `SsiCurve`) + `ssi_curve_to_curve` + `curve_contains_point` +
      `hyperbola_point(t)` eval (`center + a·cosh(t)·major + b·sinh(t)·(normal×
      major)`) + point→t `asinh(v/b)` inversion (the bijective `sinh` coordinate);
      Stage-4 relocation reuses YR21's `project_onto_cone_section` /
      `cone_plane_residual` / `cone_chord_budget_from_owner` UNCHANGED (no new
      relocation method). **The new mechanism = two-branch selection:**
      `ssi_rs::intersect(Plane,Cone)` returns **2** `Hyperbola` for the HYPE case
      (one per nappe, opposite `major_axis`); the existing `matched==1` loop in
      `build_intersection_curves` needed NO structural change — `curve_contains_point`'s
      `(u/a)²−(v/b)²=1` membership **with the `u>0` discriminator** rejects the
      wrong-nappe branch (`u<0`), so exactly one matches and `matched==2/0` stays a
      LOUD `AmbiguousCurve`. Membership band = the geometric residual `|F|/|∇F|`
      (first-order perpendicular distance, the hyperbola analog of the
      ellipse/parabola arms), **NOT** a flat widening (P9/P10 held; adversary
      re-verified the band scales ~linearly with off-distance). Role-separated FIP
      cycle: spec → RED (8 oracles incl. an independent 2-candidate ssi oracle, a
      two-branch-selection oracle, the YR22 reframe oracle4 invariant, and a
      real-sidecar E2E) → GREEN (production-only; **STOPPED at oracle7 P9/P10**
      rather than widen a tolerance) → **driver oracle7 reframe** (the 2·cone_d_eps
      beyond-band fixture rejects honestly with `FaceResolutionFailed` — the
      reloc-band guard is geometrically UNREACHABLE for an on-plane ring since the
      YR18 on-both gate skips the edge first, and a narrower δ lands in a
      LineSegment-fallback dead zone that would SILENTLY succeed; "spec principle
      over literal", mirroring the YR22 oracle4 reframe) → Adversary (7 attacks,
      all PASS: wrong-nappe rejection witnessed by u-sign, band-not-flat,
      no YR21/YR22 regression, no silent-wrong across δ∈{0.5,1,1.2,2}·d_ε,
      independent oracle7-honesty recompute (centroid 1.33–1.60·d_ε off-cone),
      from-scratch eval round-trip). Commits `c2e088e6` (spec)→`0e22956d` (RED)→
      `c3dc4f13` (GREEN)→`713c9901` (oracle7 reframe)→`cca14e25` (adversary). Full
      `cargo test -p yang-rs` green (yr23 8/8 + yr23_adversary 7/7); fmt+clippy
      clean; kernel-v2 (consumer) still builds. **Fuzz delta: this DOES move the
      number** (unlike the measure-zero parabola) — a random box cut of a cone is
      (near-)always a hyperbola (`plane_cone` HYPE), so the ~21 remaining cone
      `AmbiguousCurve` are (near-)all hyperbola and cone `ok_correct` should rise
      from 5 toward ~26. **NOT fabricated here** (curved fuzz can't complete
      in-container per `curved_fuzz_sidecar_zombie_blocker`); capability proven by
      the unit oracles + the real-sidecar E2E (oracle8 ran green against the C++
      binary). The worker PREDICTED cone `ok_correct` 5→~26.
      **DRIVER-VERIFIED DELTA (CORRECTION — the prediction was wrong; cone is NOT
      closed):** curved fuzz N=90 (same seed) post-YR23: cone `AmbiguousCurve`
      **21 → 4** (hyperbola selection WORKS), but cone `ok_correct` only **5 → 6**
      and the bulk **shifted to `LocalRefinementRequired` 0 → 16** (Union 5,
      Subtract 11); overall `ok_correct` 66 → 67; `SILENT_WRONG` 0. The hyperbola
      SELECTION is correct, but the YR21 cone-section relocation **breaks down for
      hyperbola points reaching toward the ASYMPTOTIC generator** (where
      `|n·g|→0` ⇒ the `project_onto_cone_section` guard fires
      `LocalRefinementRequired`). The RED oracle + E2E sample near the vertex (where
      relocation works), so they passed; the random fuzz generates arcs extending
      toward the asymptote and exposes the gap. **This is the "moved-the-failure-
      to-a-sibling-variant" pattern** (cf. memory [[fix_all_gates_sharing_a_metric]]):
      the gate is cone `ok_correct` rising, not `AmbiguousCurve` dropping. **⇒ Cone
      is NOT closed. Closing it needs a Stage-4 hyperbola near-asymptote relocation
      cycle (a real geometric gap, larger than the PR-YR24 axis-parallel triage) —
      OR an explicit decision to leave near-asymptote hyperbola arcs as a sanctioned
      LOUD `LocalRefinementRequired` (out-of-scope), which is honest but caps cone
      coverage.** Shipped YR23 is sound (selection + near-vertex relocation, zero
      silent-wrong) and not a regression — it is progress that revealed a deeper
      gap.
    - **PR-YR24 — residual triage (likely small).** Remaining
      `LocalRefinementRequired` (axis-parallel / through-apex sections): confirm
      genuinely-degenerate ones correctly stay LOUD (out of scope, not a
      regression); close out cone. May fold into YR23.
    Each is a full RED→GREEN→Adversary cycle with the calibrated fuzz gate (cone
    `ok_correct` rises for the targeted section type; ZERO new silent-wrong;
    driver-verified delta). Cavity-sense / watertightness / on-surface oracle are
    surface-agnostic, so Subtract comes along per type.
  - **Next M5 increments (sequenced):** ~~curved `Subtract` cavity-sense~~
    (PR-YR13 ✅, box − cylinder blind pocket; ~~through-hole genus-1~~ PR-YR14 ✅;
    ~~box − sphere hemispherical dimple~~ PR-YR15 ✅; ~~box − cone conical pocket~~
    PR-YR17 ✅)
    + cut-surface handling (deferred in PR-YR8/YR5; through-cone / oblique cone cuts
    + internal spherical/conical voids still open)
    → side-face-exit / corner (triple-point) loud-STOP guard (oblique
    out-of-scope case) → broader SSI surface/pair coverage (cyl∩cyl) → the **general
    degree-4 curve** (a new parametric `SsiCurve` variant + the 5 general-position
    solvers) + torus pairs (rest of A15.4) → ~~curved `Surface` variants~~ (PR-YR6
    ✅) → ~~P2a curved cylinder tessellation~~ (PR-YR7 ✅) → ~~P2b sphere Stage-1
    tessellation~~ (PR-YR12 ✅) → ~~P3: Stage 3 wire
    `ssi-rs`~~ (PR-YR9 ✅). The general degree-4 cyl∩cyl curve requires a NEW
    parametric `SsiCurve` variant + general-position solvers, and **MUST be planned
    with a human before implementation.**
- **M6 — Native `cherchi-rs` Stage 2** behind the same interface, parity-green
  vs the sidecar on the corpus. **The biggest milestone — a faithful port of the
  MIT Cherchi C++ (`/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/`)
  to native Rust, removing the `cherchi-sidecar-rs` subprocess.** Reference parity
  vs the C++ sidecar is the LOAD-BEARING oracle (every PR diffs native vs sidecar
  on a corpus subset; CLAUDE.md hard-rule #2). MIT attribution header on every
  ported file. Still uses the `indirect-predicates-sidecar-rs` FFI for LPI/TPI
  until M7 (so M6 lands native-but-not-yet-WASM; M7 clean-rooms the predicates →
  restores WASM). **Foundations already in `cherchi-rs`:** predicates (CR1–10),
  FastTrimesh+Tree (CR11–12c), Stage-1 pair detection (CR13), the CDT (NC1), the
  `MeshBoolean` trait + `LabeledArrangement` contract, and the IP FFI
  (`lambda3d_lpi/tpi`, `orient3d`). **Decomposition (PR-CR-AR* arrangement /
  PR-CR-BL* boolean-labeling; reference-parity-gated; demand-drive any missing IP
  predicate wrapper per CLAUDE.md #8):**
  - **PR-CR-AR1 — tri-tri intersection → implicit points. [DONE]** Ported
    `arrangements/code/intersection_classification.cpp`: for each CR13 candidate
    pair, classify (sign-pattern decoders cpp:834-925) and construct the typed
    intersection-vertex set per pair (`classify_pair` / `classify_all` in
    `crates/cherchi-rs/src/arrangements/intersection_points.rs`). **First FFI
    consumer inside `cherchi-rs`**, gated behind the off-by-default
    `indirect-predicates` feature (WASM still builds with the feature off; CI runs
    the crate both ways). **Scope (source-faithful, deviation N13):** builds
    **explicit + LPI only** — the source constructs *no* TPI here; TPI lives in
    `triangulation.cpp::createTPI`, deferred to **AR2**. AR1 ports the generic
    non-coplanar **transversal** crossing (`checkSingleNoCoplanarEdgeIntersection`
    → LPI via `ImplicitPoint3DLpi` + `lambda3d_lpi_*`; `checkVtxInTriangleIntersection`
    → explicit). Fully-coplanar and single-coplanar-edge pairs are emitted with a
    loud `Deferred(..)` marker (not dropped) for a later slice. Correctness oracle:
    each LPI vertex lies on BOTH supporting triangles' planes, asserted via exact
    indirect `orient3d == Zero` (not a float tolerance); plus CR9-agreement and
    hand-verified transversal cases. Full sidecar-corpus parity engages at AR3/BL3.
  - **PR-CR-AR2 — per-triangle constrained re-triangulation** (port
    `arrangements/code/triangulation.cpp`, ~1366 lines — **split into two
    slices**; NOT the spade NC1 CDT, which is f64-Delaunay and cannot handle
    exact/implicit points — port Cherchi's incremental insertion on implicit
    points via exact predicates + the CR12c `splitTri`/`splitEdge` API):
    - **PR-CR-AR2a — point/edge insertion. ✅ DONE.** Ported
      `triangulateSingleTriangle`'s point-collection (`aux_structure.rs`
      `group_intersection_points` → per-base-tri interior/edge buckets) +
      `splitSingleTriangle` (`retriangulate.rs` `split_single_triangle`): inserts
      AR1's intersection POINTS (interior → `split_tri`, on-edge → `split_edge`)
      into the per-triangle submesh with **exact** point-location via the FFI
      generic dispatch on `vert_coords`. Produces a valid covering
      sub-triangulation whose vertices include every intersection point (segments
      not yet enforced as edges). **Precursor CR-IP6b** added the implicit 2D
      predicates `orient2d_xy/yz/zx` + `point_in_triangle` to
      `indirect-predicates-sidecar-rs`; **Cycle 2** generalized `FastTrimesh`
      vertex storage to typed `VertexCoords { Explicit, Lpi }`. Oracle: exact
      covering triangulation (pure-dashu `RBig` signed-area-sum + same-sign
      winding, LPI coords from exact line-plane intersection — independent of the
      FFI split path), all intersection points are vertices, completeness/incidence
      via the exact FFI. Deviation **N14** (readable `splitSingleTriangle` with a
      uniform on-edge check; structural LPI dedup). **AR2b is next.**
    - **PR-CR-AR2b — constraint segments + TPI.** Decomposed A/B/C.
      - **Cycle A (done)** — FFI segment predicates (`inner_segments_cross` /
        `point_in_inner_segment` / `point_in_segment`).
      - **Cycle B (done)** — exact `point_in_segment_3d` (swaps the N13 raw-`f64`
        guard for the CR1 collinearity predicate) + `ConstraintSegment` grouping.
      - **Cycle C1 (done, PR-CR-AR2b Cycle C1)** — real `ImplicitPoint3DTpi`
        handle routing: `VertexCoords::Tpi` now flows through the
        per-base-triangle re-triangulation as an exact TPI handle (replacing the
        Cycle-B `sum/9` centroid placeholder), with macro-generated E/L/T
        predicate dispatch (`with_gp!`). Exact on-3-planes `orient3d==Zero`
        oracle. **The N13 TPI-handle deferral is RESOLVED at the routing layer.**
      - **Cycle C2 (remaining → AR3-coupled)** — `addConstraintSegment`
        enforcement + the segment-crossing `createTPI`. **STOP banked (P9/P10):**
        `createTPI`'s 2nd/3rd supporting-plane sourcing
        (`computeTriangleOfSegment` → global `seg2tris` + `jollyPoint` coplanar
        fallback) is AR3-level global state — the Cycle-B `source_tri` covers only
        an original transversal's witness, not mid-recursion sub-segments or the
        coplanar fallback. Deferred to Cycle C2 / AR3 rather than improvised.
  - **PR-CR-AR3 — constraint enforcement + global conforming soup** (absorbs the
    AR2b-deferred Cycle-C2 enforcement, which needs global cross-triangle state).
    **Parity-oracle correction (2026-06-08):** there is NO standalone C++
    arrangement binary (the 2020 arrangement code is embedded library-only — no
    main, no CMake target; only `mesh_booleans` (full boolean) is built). So AR3
    does NOT diff against a C++ arrangement. **AR3 oracle = structural + EXACT
    predicate invariants** (no self-intersections via exact `orient3d`; every
    detected intersecting pair realized as shared/constrained edges; consistent
    topology; Euler sanity) — strong for an exact arrangement. **Full C++
    reference parity engages at BL3** (the existing `mesh_booleans` binary
    transitively validates the arrangement: a wrong arrangement → wrong boolean →
    parity fail), honoring the parity-rule intent without speculative C++
    arrangement-dump infra. (Build such a sidecar later only if the structural
    oracle proves insufficient.) **Split:**
    - **PR-CR-AR3a — constraint-edge enforcement (DONE, 2026-06-08).** Ported
      `triangulation.cpp::addConstraintSegment` (cpp:597) + `createTPI` (cpp:1007)
      + helpers (`findIntersectingElements`, `boundaryWalker`, `earcutLinear`,
      `segmentsIntersectInside`, `pointInsideSegment`, `splitSegmentInSubSegments`)
      into `arrangements/enforce.rs`: realizes each AR2b `ConstraintSegment` as
      constraint-flagged mesh edge(s), constructing `createTPI` (real
      `ImplicitPoint3DTpi` from C1) at segment crossings. Public surface
      `SegmentSpec` / `EnforceError` / `enforce_constraint_segments` /
      `enforce_constraints`. The C++ global `seg2tris` is replaced by a
      per-work-item carried `source_tri` plus a `constraint_planes` side map keyed
      by sorted vertex-id pair (the minimal `TriangleSoup`). Oracle met
      (structural + EXACT, no C++ arrangement binary): constraints realized
      end-to-end; TPI exact on all 3 planes (`orient3d == Zero`); exact conforming
      sub-triangulation (pure-`dashu` covering); no spurious TPI (one crossing →
      one TPI, robust to endpoint/spec ordering — adversary-verified). The TPI
      handle/predicate dispatch (C1) was factored into a shared
      `arrangements/gp_dispatch.rs` (pure move) and reused. **Deferred to AR3b
      (the STOP walls, P9/P10):** `computeTriangleOfSegment`'s global `seg2tris`
      sourcing and the coplanar `jollyPoint` fallback — surfaced as the
      `EnforceError::SourcePlaneUnavailable` / `DegenerateTpi` errors (not hit by
      the in-scope original-transversal crossings; the multi-crossing case
      resolves its planes from the recorded sub-edge planes).
    - **PR-CR-AR3b — global conforming soup + topology (DONE, 2026-06-09).**
      `mesh_arrangement` (`arrangements/soup.rs`) wires the full
      detect→classify→group/canonicalize→split→enforce→assemble pipeline into a
      global non-self-intersecting soup: input scaling (`compute_multiplier`),
      global vertex dedup/weld (`merge_duplicated_vertices` +
      degenerate/duplicate-triangle removal), per-pair AR1 classification, global
      intersection-point grouping with N18 EXACT-coordinate canonicalization
      (coincident LPI/TPI points reached via different generator tuples weld to
      one identity across triangles), per-base-triangle fast-path-or-split+enforce,
      and a global weld of the emitted submeshes. Oracle met (structural + EXACT,
      RED 5-invariant + hand corpus; no C++ arrangement binary): conforming,
      jolly-tailed, label-aligned, no-degenerate, implicit-points-welded. An
      independently-authored ADVERSARY module pins input-ordering invariance
      (winding/order/label-swap), multi-crossing faces (conform or loud
      `DeepRecursionRequired`), the `SingleCoplanarEdge` loud-defer branch, N18
      anti-over-weld, and planar fast-path fidelity (resolved-position parity).
      **Still deferred (loud, P9/P10):** the AR3a `SourcePlaneUnavailable` /
      `DegenerateTpi` walls remain typed errors where unreached; coplanar
      overlap + single-coplanar-edge-through-interior are loud
      `CoplanarPairDeferred` (the §4.5.5 2D-Boolean pre-pass is **M8**). Feeds
      BL*.
  - **PR-CR-BL1 — patch flood-fill (DONE, 2026-06-09).** Ported
    `computeAllPatches` / `computeSinglePatch` (booleans.cpp:396/426, serial
    variant) into the new feature-gated `labeling/patches.rs`:
    `compute_all_patches(&ArrangementSoup) -> Patches { patches, tri_to_patch,
    border_verts }` — ascending seed scan + stack flood across manifold edges
    (≤2 incident tris), stop at non-manifold intersection edges, border-vert
    marking for BL2 `findRayEndpoints`. Oracle (10 invariants): partition /
    label-constant / manifold-maximality / intersection-cuts / border-verts ==
    non-manifold endpoints / disjoint + enclosed + point-touch degeneracies /
    determinism / loud errors. Independently-authored ADVERSARY (12 tests):
    ordering/winding/concat invariance, 3-solid two-loop chain, through-cut
    (3 patches per solid), hand-built 3-incident edge, LabelMismatch path.
    Deviations: adjacency built from the soup (Rust FastTrimesh is
    per-base-triangle), serial-only (rule #5), sorted-Vec patches, returned
    border set, loud error for the C++ assert. **Scope note:** the `foctree`
    octree is NOT built here — it has no consumer until the BL2 ray-cast
    (demand-driven, CLAUDE.md #8 spirit); port it in BL2 alongside
    `findRayEndpoints`/`computeInsideOut`.
  - **PR-CR-BL2 — ray-cast in/out (2022 §5).** The robust per-patch in/out.
    - **Cycle A (DONE, 2026-06-10)** — `labeling/inside_out.rs`:
      `compute_inside_out(&soup, &patches) -> Vec<Label>` (per-patch inner
      labels). Full port of `findRayEndpoints` (explicit-origin branch) /
      `fast2DCheckIntersectionOnRay` / `checkIntersectionInsideTriangle3D` /
      `perturbX|Y|ZRay` + `perturbRayAndFindIntersTri` /
      `sortIntersectedTrisAlong*` (exact LPI sort keys via FFI
      `lessThanOnX/Y/Z`, btree-set equal-key-drop semantics) /
      `analyzeSortedIntersections`. Structural prerequisite: the soup now
      carries the prepped original `in_tris`/`in_labels` (C++ `arr_in_tris`).
      Oracle (5 invariants) + independent ADVERSARY (11 tests) — the
      adversary found 3 real bugs, all fixed: winner-less perturbation
      events now SKIP (N19, C++ `winner != -1` semantics; the fatal error
      was wrong on grazing input) and ray-parameter-ZERO hits are discarded
      (N20 — the C++ keeps them and silently mislabels point-touching
      solids; justified deviation, see docs/yang_deviations.md). Port
      finding: the C++ octree rayAABB query is semantically LOAD-BEARING
      (excludes behind-origin events); the brute-force port reproduces it
      with an explicit ray-AABB pre-filter.
    - **Cycle B (DONE, 2026-06-10)** — the C++ "generated ray" branch:
      synthetic origin at a patch triangle's approx centroid −0.1 along its
      dominant-normal axis (pure-f64 LPI/TPI approx eval; CR1/CR4 gates),
      EXACT validation (orient3d straddle + strict interior passage via
      gp_dispatch E/L/T), `seed_tri` recorded, and the sort's
      seed-plane-side discard (C++ `ray.tv` branch). Through-cut bands +
      hole discs now classify correctly (RED draft's expectation was
      itself corrected: a pierced solid's through-hole DISCS are inside
      the peg). ADVERSARY (9 more tests): X/Y-axis cuts, two pegs, peg
      through two stacked cubes, behind/forward seed-plane third solids,
      45° diamond peg, 0.01 sliver peg — no Cycle-B bugs.
    - **Cycle C** — the `foctree` octree as the candidate-set producer
      (oracle: pruned ⊆ brute AND identical final labels). NOTE: Cycle A
      established the rayAABB filter is semantically LOAD-BEARING (not
      mere acceleration); the brute-force port carries an explicit
      ray-AABB pre-filter the octree must reproduce exactly.
  - **PR-CR-AR3c — input-order-invariant constraint realization (OPENED
    2026-06-10, blocks BL3 corpus parity).** The BL2-Cycle-B adversary
    found AR3b's constraint realization is input-order-DEPENDENT on
    CLOSED intersection loops: reversing global triangle order or
    swapping the two solids' concat order on a through-cut fixture
    leaves 4 intersection-loop fence segments unrealized as shared
    multiplicity-4 edges (two realized on only one side, two on
    neither), so the BL1 flood leaks and 6 patches collapse to 2. The
    AR3b conforming oracle (no interior AREA overlap) is structurally
    blind to a constraint segment missing from a perpendicular face's
    re-triangulation. RED witness: `#[ignore]`d
    `adversary_b_generated_ray_permutation_invariance`
    (labeling/inside_out_adversary_tests.rs) — un-ignore when fixed.
    Fix belongs in the `mesh_arrangement` orchestration (soup.rs):
    every intersection-curve segment must end as a shared edge of BOTH
    incident surfaces regardless of input presentation. New invariant
    for the AR3b oracle suite: per-pair constraint segments appear as
    multiplicity-4 edges, asserted under order/winding permutations.
  - **PR-CR-BL3 — emit `LabeledArrangement` + native `MeshBoolean` impl.**
    Assemble the per-tri source + patch_id + per-input in/out, implement
    `MeshBoolean` natively, **parity-green vs the sidecar on the corpus**, then
    switch `yang-rs` to the native backend behind the trait (the sidecar stays as
    the `#[cfg(test)]` parity oracle).
- **M7 — Clean-room indirect predicates from Attene's paper → restore WASM.**
  Removes the LGPL FFI dependency and the `compile_error!` WASM block.
- **M8 — Stage 0 coplanar preprocessing** hardened last (special case that
  complicates everything earlier). **Verified a genuine native need** (deviation
  N8, 2026-06-02): the patched sidecar emits multi-solid-labeled
  (`surface.len()==2`) triangles on coplanar overlap (test
  `c3_coplanar_face_yields_multi_attribution`), which surface in `yang-rs` as a
  loud `FaceResolutionFailed` (F2) — coplanarity is NOT delegated away, so M8 must
  implement the §4.5.5 2D-Boolean pre-pass (currently a correct loud-STOP
  deferral). **Also folded into M8: §4.5.4 illegal-self-intersection
  detection/removal** (deviation N6) — absent in the new crates; currently benign
  for analytic inputs (sidecar mesh validly trimmed + `check_watertight_2manifold`
  gate), to be added as a post-trim detector here.

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
  heart, highest risk]* **First step done (PR-YR6 ✅):** curved `Surface`/`Curve`
  enum variants exist (mirroring `ssi-rs` field shapes) and the pipeline rejects
  them LOUDLY (`YangError::CurvedSurfaceNotYetSupported`) — no curved
  tessellation or `ssi-rs` call yet. **Stage-1 cylinder tessellation done
  (PR-YR7 ✅)**, **first end-to-end curved boolean done (PR-YR8 ✅):
  cylinder ∪ box flows through Stages 2/5/6, watertight + Euler=2, analytic
  `Surface::Cylinder` survives with exact params**, and **Stage 3 exact
  intersection edges done (PR-YR9 ✅): `ssi_rs::intersect` now refines the
  cylinder∪cap arrangement edges to the exact `Curve::Circle`/`Ellipse` (no
  longer mesh-approximate), with a P9 STOP on intersect/selection failure**, and
  **Stage 4 mesh updating done (PR-YR10 ✅): the mesh crossing points are
  RELOCATED onto the exact circle (Yang §4.4.1, not a global CDT) + §4.5.3
  reversed-point correction; watertightness inherited; cylinder ∪ box is now
  exact-edge AND on-curve, adversary-verified fold-free.**
  **Stage-1 sphere tessellation done (PR-YR12 ✅):** closed solid sphere → a
  watertight z-up lat/long mesh with a bijective `TessellationMap` (`d_ε =
  1e-2·2r√3`; cone still rejects loudly).
  Remaining: Stage 1 curved tessellation for the rest (cone; non-convex
  profile triangulation via Livesu earcut-CDT, for gears; Steiner points);
  ~~Stage 3 refine arrangement edges to the exact SSI curve (wire `ssi-rs` in —
  P3)~~ (PR-YR9 ✅); ~~Stage 4 conform mesh to refined curves~~ (PR-YR10 ✅,
  circle only — ellipse relocation + §4.5.2 real local refinement still loud
  STOPs);
  ~~Stage 6 curved cavity-sense for Subtract (deferred in PR-YR8)~~ (PR-YR13 ✅,
  `box − cylinder` blind pocket via `BRepFace.reversed`; ~~through-hole genus-1~~
  PR-YR14 ✅ via per-shell Euler gate χ=2−2g; ~~box − sphere hemispherical
  dimple~~ PR-YR15 ✅ via `Surface::Sphere` wiring + `sphere_chord_bound`;
  CONE cavities + internal spherical voids + box-as-subtrahend still open) +
  cut-surface
  faces (deferred in PR-YR5). **Exit:** cylinder ∪ box ✅ (exact edges + on-curve mesh),
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
