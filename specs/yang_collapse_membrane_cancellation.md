# Spec: collapse_vertex membrane cancellation (opposite-winding duplicate pairs)

> Status (2026-07-10): SHIPPING — mint site measured on live code (task #121
> increment 1, the N2/F0059 epic's Stage-6 double-cover origin investigation).

## Goal

`collapse_vertex` (yang-rs, the §4.5.3 / PR-KV9 / §4.4.1(b) / KV15b shared
edge-collapse primitive) documents that "the two collapsed slivers' surviving
directed edges are mutual opposites that cancel" — but the cancellation was
never implemented. Measured consequence (F0059, Steinmetz cyl×cyl union):

- Stage-4's PR-KV9 junction-twin collapse identifies two arrangement vertices
  minted for ONE geometric junction (the Steinmetz seam apexes, e.g. victim 96
  → survivor 82 at `(0,0,0.35)`).
- The two-triangle **membrane** that spanned the twin gap (`[28,45,96]` +
  `[45,28,82]` — a zero-volume tent between the twins) maps to an EXACT
  duplicate pair with OPPOSITE windings (`[28,45,82]` + `[45,28,82]`).
- The pair is a zero-volume doubled flap: every fan edge it touches becomes
  undirected-count-4, which derails the Stage-6 figure-eight wedge walk
  (`s6-wedge-walk-not-outgoing` at the apex) → `NonManifoldOutput`.

This is the TRUE origin of the F0059 "Stage-6 double-cover" documented in
`specs/yang_stage4_conic_triple_junction.md` — it is minted by the Stage-4
collapse, NOT by the Stage-2 arrangement or Stage-5 flood-fill (both
candidate origins in the 2026-07-08 diagnosis are wrong; the kept submesh is
I6-guarded duplicate-free at `boolean()` compaction, and the probe chain
`YANG_DOUBLECOVER_PROBE` localizes the mint to the PR-KV9 collapse call).

## Parameters

- `collapse_vertex(mesh, attribution, victim, survivor)` — unchanged
  signature. Cancellation is an internal post-mapping step; no new public
  surface.

## Branch table

After the victim→survivor index mapping and the existing repeated-index
(degenerate) drop, group surviving triangles by sorted vertex triple:

| duplicate group | windings | Action |
|---|---|---|
| exactly 2 triangles | OPPOSITE (cyclic keys differ) | **cancel: drop BOTH** (zero-volume flap; their 6 directed edges are 3 mutual-reverse pairs that pair with each other) |
| exactly 2 triangles | SAME | leave untouched (a genuine same-winding double cover is non-manifold input — downstream loud STOPs own it, P9) |
| ≥ 3 triangles | any | leave untouched (ambiguous pairing — never silently pick; downstream loud STOPs own it) |
| no duplicate | — | byte-identical behavior (the overwhelmingly common path) |

Winding comparison: rotate each triangle so its smallest vertex index leads;
equal rotated triples ⇔ same cyclic winding.

## Invariants

- **I1 (watertightness preserved)**: the cancelled pair's directed edge
  multiset is 3 mutual-reverse pairs; removing both triangles leaves every
  remaining directed edge's pairing count unchanged.
- **I2 (manifold restoration)**: each undirected fan edge shared with real
  neighbours drops 4→2; the membrane-only edge (the twin-gap chord) drops
  2→0.
- **I3 (attribution lockstep)**: the parallel attribution vector drops the
  same indices (as the existing degenerate drop does).
- **I4 (no-op purity)**: a collapse that mints no duplicate pair returns a
  byte-identical mesh + attribution vs. the pre-fix code.
- **I5 (determinism)**: grouping by sorted triple + BTreeSet-ordered drop —
  no hash-order dependence in the output.

## Oracles

- **Unit (red→green)**: a closed two-tetra "twin tent" fixture — collapse the
  twin; assert the duplicate pair is gone, no undirected edge has count ≠ 2,
  and attribution stays in lockstep.
- **Unit (branch: same-winding kept)**: same fixture with one membrane
  triangle's winding flipped pre-collapse → both copies retained.
- **Unit (branch: no-dup byte-identity)**: a collapse with no membrane
  returns the identical mesh as before the fix (I4).
- **Assay**: full corpus — 0 WRONG preserved, no CORRECT lost. (No case
  converts on this fix ALONE — F0059 needs the Stage-4 triple-junction
  handler above it and then walls at the cap-rim junction-insertion class;
  the cancellation is a prerequisite layer, measured via the env-gated
  experiment chain.)

## Failure modes

- Same-winding duplicates / ≥3 copies: intentionally untouched, downstream
  `NonManifoldOutput` / kernel-v2 "not used by exactly two directed edges"
  STOPs stay loud.

## Research Basis

- [#24] Yang et al. 2025 §4.4.1(b) (merge of sub-resolution points; the
  collapse primitive this repairs implements that merge).
- [#38] Cherchi 2022 §5 (the arrangement/boolean invariant the flap violates:
  a solid boundary is a 2-manifold triangle set; coincident opposite-winding
  pairs are interior zero-volume artifacts, not boundary).
- Mirrors the existing repeated-index drop in `collapse_vertex` (same
  "collapse artifacts cancel" contract, extended from the degenerate-sliver
  form to the coincident-pair form).
