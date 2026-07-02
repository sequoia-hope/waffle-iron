# N2-3a — Stage-4 Fig-11(b) junction-cluster merge onto the relocated point — Spec

Third N2 increment (parents: `specs/n2_stage4_mesh_updating.md`,
`specs/n2_stage4_dt_recompute.md`; design history:
`specs/yang_n2_stage4_cdt_mesh_updating.md`). Yang 2025 §4.4.1 / Fig 11(b):
"if an endpoint **p** of the split edge is too close to **q**, we merge p with
q" — q being the intersection point ON the exact curve
(`refs/text/yang2025_hybrid_boolean.txt:546-573`).

## 0. Grounding (2026-07-02 diagnostic — what redirected N2-3)

Instrumented findings that redefine this increment (all measured, not
inferred):

1. **The phase-3 STOPs have no live consumer.** Probes at all four Stage-4
   repair STOPs (`lib.rs` ~9392/9495/9542 `LocalRefinementRequired`, ~9994
   `DegenerateTriangle`) hit ZERO times across the full yang-rs suite, the
   whole m8_samenormal campaign, and the 194-case assay. The §5c.5 "full-ring
   periodic-θ re-mesh" wiring therefore stays **deferred until a consumer
   exists** (demand-driven rule; the primitives from N2-1/N2-2 are ready for
   it).
2. **The live Stage-4 §4.4.1 defect is a merge-target bug.** R0072 (same-normal
   campaign) fails `VertexOffSurface { FaceId(11) }` at kernel-v2's debug-only
   loop-vertex tripwire: residual **1.607e-6** off a cylinder of radius
   2.13e-4. Causal chain (probed): B's side plane is near-tangent to A's
   cylinder at a cap corner; the exact arrangement legitimately mints **one
   geometric junction as three vertices** — v11 (on capA∩planeB exactly), v7
   (on both planes exactly, 1.6e-6 inside the cylinder), v8 (an LPI on a rim
   chord, 1.5e-6 inside); cluster diameter 4.0e-7. Stage-4 relocates **only
   v11** (the cyl∩planeB generator-line endpoint) onto the exact triple point
   q (on generator, cylinder, and pp line to 8.7e-20). The Fig-11(b)-shaped
   (3c) sub-feature merge then collapses **v8 → v7** (lowest index, an
   OFF-curve position) instead of merging the cluster **onto q**. v7's
   off-surface position survives to the output B-Rep. Without (3c) the case
   STOPs loudly; with it the wrongness flows downstream — and the kernel
   tripwire that catches it is `#[cfg(debug_assertions)]`-only, so a release/
   WASM build ships the off-surface vertex silently.
3. R0021's union portion is oracle-correct except one zero-area render
   triangle; its blocker is the Stage-1 partial-patch re-entry wall (a
   different milestone). The campaign's "Mode 2 → R0021, R0072" table is
   STALE and is corrected alongside this increment.

## 1. Goal

After the Stage-4 relocation loops, merge each **junction cluster** — the
near-duplicate arrangement vertices minted for ONE geometric junction — onto
its relocated on-curve junction vertex q, faithfully to Fig 11(b). Family
scope for this increment: clusters around a relocated **LineSegment-curve
endpoint** (the live R0072 class: cyl∩plane generator line ending on a cap /
pp-line junction). Other curve families keep today's behavior byte-identical.

## 2. Parameters

No new public API and no new tunables. The merge gate reuses two DERIVED
quantities already in Stage 4:

- `band(v)` — the relocated line endpoint's propagated band budget
  (`LineReloc.band_budget`, PR-F3b) **+ `d_eps`** (the Stage-1 chord band from
  `stage4_chord_band`): the same combined bound the line×circle junction
  relocation already gates on (`lib.rs:9152`). Not a new tolerance.
- `MIN_FEATURE_SIZE` / `TAU_WORK` — pre-existing floors, unchanged roles.

Inputs in hand at the merge site (all existing): `mesh`, `attribution`,
`vert_line: BTreeMap<u32, LineReloc>`, the relocated-vertex positions, the
`processed`/`endpoints` sets, `moved`, `collapse_vertex`.

## 3. Branch table

Insertion point: immediately after the relocation loops complete (after the
line+circle junction loop and the no-skip audit, before the (3) §4.5.3
junction-dedup/sweep at `lib.rs:~9330`), so the (3c) sub-feature merge only
ever sees clusters this step has already resolved.

For each relocated vertex q that is a **LineSegment-curve endpoint**
(`vert_line` member whose relocation landed it at a curve endpoint /
junction), examine every mesh-adjacent vertex p (sharing a mesh edge with q):

| Case | Condition on p | Behavior |
|---|---|---|
| Cluster member | p is NOT itself a relocation endpoint (`!endpoints.contains(p)`), NOT already merged this pass, and `|p − q| ≤ band(q)` | Fig 11(b) merge: `collapse_vertex(victim=p, survivor=q)` — p's incident triangles re-attach to q; degenerate slivers between them drop. Deterministic order: ascending q, then ascending p. |
| Adjacent but out of band | `|p − q| > band(q)` | Untouched (a real mesh vertex at resolvable distance — never absorbed). |
| Adjacent relocated vertex | p ∈ `endpoints` (it has its own exact relocation) | Untouched (two relocated vertices are two distinct exact points; if they truly coincide, the existing bit-exact junction dedup at ~9333 handles them). |
| Non-line families | q relocated by circle/ellipse/cone/torus loops | Untouched this increment (their junction classes exist: PR-F3 line×circle, PR-KV11 ellipse×pp). |

No new implicit modes; the (3c) merge and (3d) edge-split remain as backstops,
byte-identical for inputs with no line-endpoint cluster.

## 4. Invariants (measurable)

- **I1 (on-surface output — the R0072 defect):** after Stage 4, every vertex
  referenced by a triangle attributed to a face lies on that face's `Surface`
  within the kernel import band `1e-9·(1+max(r,‖p‖∞))` — in particular the
  junction survivor sits at q (on the exact curve), and NO vertex sits at a
  cluster member's old off-curve position.
- **I2 (merge is onto q, not within the cluster):** the survivor of a cluster
  merge is the RELOCATED vertex q; asserting the survivor's position equals
  q's relocated position bitwise.
- **I3 (watertight preserved):** `check_watertight_2manifold` passes after the
  merge (collapse_vertex preserves half-edge pairing; the existing gate is the
  proof).
- **I4 (locality / no over-eating):** a vertex farther than `band(q)` from
  every relocated line endpoint is never moved or merged (mesh vertex count
  decreases by EXACTLY the cluster population).
- **I5 (no-op guarantee):** inputs with no line-endpoint junction cluster
  produce byte-identical meshes to today (regression tier + fuzz_boxes pin
  this).
- **I6 (determinism):** repeat runs byte-identical.

## 5. Oracles

- **RED reproduction (bug-fix variant, FIP §8):** a yang-rs integration
  fixture reproducing the R0072 class — cap plane + near-tangent side plane ×
  cylinder, hand-built arrangement minting a 3-vertex junction cluster (one
  on the generator line's endpoint, two near-duplicates off-curve within the
  amplified band) → TODAY: output contains a vertex ≥1e-6 off the cylinder
  (assert the defect); AFTER: all cylinder-patch vertices within the import
  band, single junction vertex at the exact triple point, watertight, Euler
  χ=2.
- **Campaign:** `red_r0072` replay — the `VertexOffSurface` failure
  disappears. If the case then passes the FULL oracle gauntlet, un-`#[ignore]`
  it; if a different downstream wall surfaces, repoint the `#[ignore]` reason
  to the new mode (honest-harness rule).
- **R0096 probe:** the assay's other Stage-4 junction error ("relocation
  region around vertex 7 is invalid: LocalRefinementRequired") — determine
  during the test phase whether it is this same class; if yes it becomes a
  second oracle, if no its mode is documented and left loud.
- **Locality adversary (I4):** same fixture with the duplicate vertices pushed
  JUST outside `band(q)` → they must survive unmerged (and the loud
  DegenerateTriangle/watertight outcome, whatever it is, must not be a silent
  wrong).
- **Regression:** `./scripts/test.sh rewrite` green; assay **0
  SUPPORTED_WRONG and no SUPPORTED_CORRECT lost**; campaign always-on tests
  green.
- **Mutation sanity:** survivor flipped to lowest-index (the old (3c)
  behavior) must fail I1/I2.

## 6. Failure modes

- Cluster merge produces a triangle that still fails
  `validate_relocated_triangles` or the watertight gate → existing loud STOPs
  (unchanged; never silently accepted).
- A cluster candidate that is mesh-adjacent to TWO different relocated line
  endpoints within both bands → ambiguous ownership: loud
  `Stage4RegionInvalid { LocalRefinementRequired }` naming the vertex (never
  guess; expected never to fire on current corpus — pinned by a constructed
  adversary fixture if constructible).

## 7. Research basis

- **#24 Yang et al. 2025 §4.4.1 Fig 11(b)** — the merge operation this
  implements: p merged **with q** (the on-curve intersection point), not with
  an arbitrary neighbor (`refs/text/yang2025_hybrid_boolean.txt:556-560`,
  "If an endpoint p of the split edge is too close to q, we merge p with q").
- **Shipped precedents:** PR-F3 line×circle junction relocation
  (`vert_junction`, the `band_budget + d_eps` gate) and PR-KV11 ellipse×pp
  triple-point class — same derived-band philosophy, same loud-STOP posture.
- **P9 note:** the band is derived (tangency amplification of the Stage-1
  chord sagitta, ≈12.8× on R0072), not tuned; the increment REMOVES a
  silent-in-release wrongness rather than widening anything.

### 7a. Analytical vs approximate

Exact. q is the closed-form relocated point (already on the exact curve /
triple point); the merge changes mesh topology only. No SSI performed here;
A15 coverage N/A.

## 8. Scope / non-goals

- No re-mesh wiring (`stage4_mesh_update` / `d_of_t` stay unwired — deferred
  with cause per §0.1; they remain the machinery for the first future consumer).
- No changes to (3c)/(3d) logic, to non-line junction classes, or to
  kernel-v2's tripwire (its debug-only nature is flagged to the kernel-v2
  sub-project, not fixed here).
- R0021's re-entry wall and render-triangle blemish: out of scope (different
  milestones).
