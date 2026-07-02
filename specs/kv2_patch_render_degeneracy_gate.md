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

## 6a. Adversary findings (496225b5)

- B3-only degeneracy is REAL and reachable (three distinct f32 positions on
  one ruling, zero f32 cross) — dedicated witness added; the B3 arm is the
  correctness-carrying check. B2 is structurally subsumed by B3 (a bitwise
  pair makes the f32 cross exactly zero) — kept as the cheaper early check.
- No over-firing at 2×-f32-ulp spacing; bitwise-f32 tracks coordinate scale
  automatically (pinned at magnitude ~1e3).
- I3 (always-on) is killed only by RELEASE-mode runs of the existing tests
  (`cargo test --release` on this module) — a CI-mode decision, recorded.
- Measured en route: a 2e-7 twin inside a rectangle boundary already drives
  the LEPP refinement into sub-f32 slivers — the refinement cascade is the
  confirmed sliver source for the root-fix follow-up.

## 6b. Root-fix investigation (2026-07-02, measured — design space for the follow-up)

Full mechanism chain for the F0047-with-canon wreck, measured with
`KV2_PATCH_MINLEN_PROBE` / `KV2_PATCH_PASS_PROBE`:

1. The failing patch boundary is HEALTHY (27 verts, min edge = min pair =
   7.2e-3) — no tiny input feature. The unroll is clean (wraps 0, u-extent
   0.77, w_facet 0.018).
2. The femto vertex motion flips ONE exact-collinearity knife-edge in the
   boundary ring: baseline ear-clips 57 triangles (an exactly-collinear 2D
   corner dropped), canon 56 (the corner kept, femto-off-collinear) — the
   kept corner spawns a sub-f32-THIN sliver the Delaunay flip pass cannot
   remove (boundary edges fixed).
3. The LEPP refinement then propagates it: 796 → 1465 splits, 1576 → 2947
   triangles, minting 64 degenerate triangles — ALL of class B2 (two
   f32-identical vertices; zero B3-only).
4. **B2-drop DISPROVEN as the root fix** (the overlay CoincidentNeedle
   closure argument does NOT extend to needle CHAINS abutting real
   triangles): with all 64 B2 triangles dropped, F0047 still leaks 16
   unpaired render edges (14 boundary, 2 non-manifold; Euler 0). Reverted.
5. R0064's single degenerate triangle is from a DIFFERENT tessellation
   path (not the cylinder patch) — a separate small leak, unmeasured.
6. Canon-side straightening would NOT survive the unroll: exactly-collinear
   3D points map through the nonlinear (θ·r, h) chart to only
   near-collinear 2D — the baseline's exact-2D-collinearity was a
   numerical accident, not a property to restore.

Remaining root-fix candidates (a dedicated cycle): a sliver-robust patch
triangulation (constrained-Delaunay of the unrolled ring replacing
ear-clip+flip, or f32-resolution-aware boundary preprocessing with an
explicit conformality story). Until then the loud gate IS the correct
boundary: femto-shifted inputs fail typed, never silently.

## 7. Research basis

Producer-side validation of discretization output (A8.2 explicit healing /
A9 diagnosability). The degeneracy predicate mirrors the existing corpus
oracle (`no_degenerate_triangles`); no new tolerance is introduced — f32
bitwise identity is exact.
