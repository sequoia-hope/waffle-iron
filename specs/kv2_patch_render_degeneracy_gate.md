# kernel-v2 — Cylinder-patch render-precision degeneracy gate (F0047 class)

**Status:** spec (FIP Phase 1). **Change class:** bug fix (modeling-related).
**Crate:** `kernel-v2` (`tessellate.rs::tessellate_cylinder_patch`).

## 1. Goal

`tessellate_cylinder_patch` must never emit a mesh containing triangles that
are DEGENERATE AT RENDER (f32) PRECISION — it must stop loudly instead.

Measured (2026-07-02, F0047 with the banked vertex-canonicalization pass
repro-wired): a femto-band shift of one operand's vertices drives the patch's
f64 ear-clip + LEPP refinement into emitting 2947 triangles (baseline 1576)
of which 64 are degenerate and 22 render edges have bitwise-identical f32
endpoint keys — a silently wrecked, non-manifold render mesh (53 quantized
unpaired edges, Euler 42) that flipped the assay to `SUPPORTED_WRONG`. The
existing loud gates (KV9-F2 fold tripwire, self-overlap, refinement budget)
all pass — the coverage hole is f64-valid-but-sub-f32 geometry.

This is the prerequisite for re-wiring the banked world-space vertex
canonicalization (`m8_shared_boundary_identity` §8a): with the gate, that
pass's worst case is a loud typed error, never silent garbage.

## 2. Mechanism

At patch emission (after refinement, where triangles receive their final
positions), reject the face with a typed
`TessellationFailed { face, reason: "patch triangle collapsed at render precision" }`
when any emitted triangle has two vertices whose f32-rounded positions are
bitwise identical, or all three collinear at f32 (zero f32 cross product) —
the same degeneracy predicate the assay's `no_degenerate_triangles` oracle
uses, applied producer-side and always-on (NOT debug-gated).

## 3. Branch table

| # | Emitted patch triangle | Behavior |
|---|---|---|
| B1 | All three f32 positions distinct, non-collinear | Emitted (unchanged) |
| B2 | Two vertices bitwise-equal after f32 rounding | Loud `TessellationFailed` |
| B3 | Distinct but f32-collinear (zero f32 cross) | Loud `TessellationFailed` |

## 4. Invariants

- I1: no silent degenerate output — any B2/B3 configuration fails the WHOLE
  face loudly (no dropping, no snapping — P9).
- I2: the current corpus is unaffected: every `SUPPORTED_CORRECT` case
  already passes the assay's `no_degenerate_triangles` oracle, so the gate
  fires on none of them (full-assay regression gate: category counts
  unchanged, 0 WRONG).
- I3: the gate is always-on (release and debug).

## 5. Oracles

- Unit RED: a hand-built cylinder-patch arena whose boundary chain carries
  two vertices spaced below f32 resolution at the patch's coordinate
  magnitude (~1e-12 at scale 1) → today tessellation SUCCEEDS with a
  degenerate triangle (demonstrate by scanning the emitted mesh); after the
  gate → `TessellationFailed` with the typed reason (B2).
- Unit guard: the canonical patch fixtures (existing kv5b tests) still
  tessellate (B1, I2 at unit level).
- E2E: full assay — no category movement, 0 WRONG (I2). The F0047-with-canon
  configuration is NOT commitable as a test (the canon pass is unwired);
  the reproduction is recorded here and in `m8_shared_boundary_identity`
  §8a; re-wiring that pass re-validates against this gate.

## 6. Failure modes

- A legitimate future patch whose geometry genuinely needs sub-f32 features
  (model scale ≫ 1e6 or ≪ 1e-6): outside A14's supported feature range;
  loud failure is correct.
- The planar/other tessellation paths may share the coverage hole — OUT OF
  SCOPE here (no measured case); noted for a follow-up sweep.

## 7. Research basis

Producer-side validation of discretization output (A8.2 explicit healing /
A9 diagnosability). The degeneracy predicate mirrors the existing corpus
oracle (`no_degenerate_triangles`); no new tolerance is introduced — f32
bitwise identity is exact.
