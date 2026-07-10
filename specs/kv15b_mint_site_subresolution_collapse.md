# yang-rs — KV15b: sub-resolution intersection-segment collapse at boolean emission

**Status:** spec (FIP Phase 1) — root cause measured 2026-07-10.
**Change class:** bug fix (modeling-related). **Crate:** `yang-rs`
(`reconstruct_topology_stage4`, immediately before Phase-B emission).

## 1. Goal

A boolean whose exact arrangement mints two intersection-curve vertices
closer than the model tolerance (`TAU_MODEL` = 1e-7 m) must not EMIT them
as two distinct output B-Rep vertices joined by a sub-resolution edge.
Today the gear-cut subtract in R0076 emits two such pairs (measured
`KV2_SUBFLOOR_TWIN_PROBE`: 3.999e-8 and 6.472e-8, both edge-connected,
minted by `boolean_subtract OUTPUT`, arriving verbatim as
`boolean_union operand A`), and the downstream union fails at kernel-v2
edge pairing (`InvalidBooleanOutput("an undirected output edge is not
used by exactly two directed edges")`) — the ERROR class this cycle
retires.

## 2. Measured mechanism (2026-07-10)

1. The minting subtract's operands are ALL-PLANAR (gear-polygon extrude ×
   box). `has_conic` is false, so Stage 4 — and with it the §4.4.1(b)
   sub-feature-size merge, the only existing collapse pass — NEVER RUNS.
   The exact arrangement legitimately produces two crossings of
   near-parallel geometry (gear flank grazing a box edge) 3.9e-8 apart;
   nothing between arrangement and emission reconciles them.
2. The emitted BRep carries the pair as a sub-resolution intersection
   segment. The next boolean inherits them; its Stage-6 patch walks
   disagree (one patch swallows a twin as patch-interior and walks the
   chord; the adjacent patch walks the twin-stopover chain — the
   measured F0070/KV15 mechanism at sub-floor rather than femto scale)
   and kernel-v2's manifold edge pairing rejects.
3. Class scope per 2026-07-10 measurement: **R0076 only.** The other
   cases grouped under "KV15b" in the roadmap have DIFFERENT roots:
   R0007/R0071's sub-TAU pairs are PROFILE-CONGENITAL (emitted by the
   extrude/revolve constructors from micro-scale gear profiles — 96/36
   pairs at bit-identical spacing 7.790e-8/9.460e-8; no boolean mints
   them), C0075's twins are ULP-scale (§4B family), and R0053's are
   sub-representable overlay mints inside the failing op. Those are
   separate cycles; this spec deliberately does not touch them.

## 3. Design

One pass in `reconstruct_topology_stage4`, after the Stage-4 block
(relocated positions are final) and before `emit_topology`:

- For every intersection edge `(u, v)` in `intersection_curves`
  (deterministic `BTreeMap` order), resolve `u`/`v` through the
  collapses already performed by this pass (min-index union-find; skip
  self-pairs), and if `0 < |P(ru) − P(rv)| < TAU_MODEL`, collapse the
  higher-resolved-index vertex onto the lower via the existing
  watertight-preserving `collapse_vertex` (survivor keeps its own exact
  coordinates — never an average).
- Single sweep over the ORIGINAL intersection-segment set: a segment
  whose resolved length is ≥ `TAU_MODEL` is never collapsed, even if
  earlier collapses moved its resolved endpoints (no chain drift — the
  KV10/KV15 isolation argument: genuinely distinct features are ≥
  `MIN_FEATURE_SIZE` apart, an order above the band).
- If any collapse happened: `compact_unreferenced_verts` + recompute
  Phase A (the exact machinery the §4.5.3 collapse path already uses),
  then emit.

**Why `TAU_MODEL` and not `MIN_FEATURE_SIZE`:** A8.1/A14 make
`TAU_MODEL` the central vertex-merge resolution — two points closer
than it ARE one model point. The Stage-0 coplanar clustering band floor
downstream is exactly `TAU_MODEL` (`TAU_MODEL.max(scale·TAU_WORK)`), so
any emitted sub-`TAU_MODEL` pair is GUARANTEED to be welded into a
degenerate loop by a downstream coplanar op — emission hygiene must
match the consumer's resolution floor. The reverted-R0091 hazard
(§4.4.1(b) global widening, spec `m8_holed_disc_coplanar_overlay`) fired
at the 10×-coarser `MIN_FEATURE_SIZE` floor over ALL degenerate
triangles; this pass is one order tighter AND restricted to consecutive
intersection-curve vertices (full provenance, the increment-4 pattern).

## 4. Parameters

None user-facing. Constants: `TAU_MODEL` (central policy, A14.3 — no new
tolerance is introduced).

## 5. Branch table

| # | Configuration | Behavior |
|---|---|---|
| B1 | Intersection segment, resolved length in (0, TAU_MODEL) | Collapsed: higher resolved index onto lower; survivor coordinates unchanged |
| B2 | Intersection segment, length ≥ TAU_MODEL | Untouched |
| B3 | Intersection segment, length exactly 0 (shared-mint 3D-identical pair) | Untouched by this pass (existing M-B emission identification handles it) |
| B4 | Non-intersection mesh edge, any length | Untouched (inherited operand geometry — the micro-profile population — never collapses here) |
| B5 | Chain u–v–w, both links < TAU_MODEL | Both collapse onto min index via resolution; a segment whose RESOLVED length grows ≥ TAU_MODEL stays (single-sweep, no re-scan) |
| B6 | No sub-resolution intersection segment (the common case) | Byte-identical emission (no compaction, no Phase-A recompute) |

## 6. Invariants

- I1: survivor of every collapse is the minimum resolved vertex index;
  its coordinates are bit-unchanged.
- I2: no vertex pair whose resolved distance is ≥ TAU_MODEL is ever
  collapsed by this pass.
- I3: only vertex pairs that appear (after resolution) as CONSECUTIVE
  intersection-curve vertices — keys of `intersection_curves` — are
  eligible. Operand-inherited geometry is untouched (B4).
- I4: when no collapse fires the emitted output is byte-identical to
  pre-KV15b.
- I5: the emitted output B-Rep contains no intersection edge shorter
  than TAU_MODEL (the emission-hygiene contract).

## 7. Oracles

- E2E corpus tracker (RED today): un-ignore
  `r0076_no_edge_pairing_wall` (test-harness
  `edge_pairing_twin_weld_campaign.rs`) — replay must not contain the
  edge-not-2-directed failure.
- Unit (yang-rs, new fn `collapse_subresolution_intersection_segments`):
  - B1: synthetic mesh, sub-TAU intersection segment → collapsed, min
    index survives with original bits, degenerate tris dropped.
  - B2/B4: ≥-TAU segment and a sub-TAU NON-intersection edge both
    untouched (mutation guard: widening the band or dropping the
    intersection-membership gate must fail these).
  - B5: three-vertex chain both-links-sub-TAU → single survivor, no
    drift beyond the original twins.
- Structural: full yang-rs suite; kernel-v2 suite (emitted outputs still
  pass `from_yang` validation).
- Regression gate: full release assay, **0 WRONG, zero SUPPORTED_CORRECT
  lost** vs committed results.json; target R0076 ERROR→CORRECT (or a
  downstream typed error — measured, not assumed). Contingency (P10): if
  the assay shows any curved-path case flipping, the pass gains a
  `!has_conic`-scoped fallback ONLY via a spec amendment recording the
  measurement.

## 8. Failure modes

- A collapse that leaves inconsistent topology fails loudly at the
  existing Phase-A/Stage-6/kernel-v2 validations (unchanged surfaces).
- Sub-resolution segments in CURVED intersection curves collapse under
  the same criterion; any Stage-4 relocation entry of a collapsed-away
  vertex is dropped by `compact_unreferenced_verts` (existing PR-YR11
  contract).

## 9. Research basis

- [#24] Yang et al. 2025 §4.4.1(b) (Fig. 11(b): endpoints of a split
  edge too close are merged; the exact point survives) — this pass is
  that merge applied at the emission boundary for the no-conic path
  Stage 4 never covers.
- Cherchi 2020/2022: the exact arrangement legitimately keeps distinct
  crossings; sub-resolution reconciliation is the consumer's
  representation-hygiene concern (KV10/KV15 entry-weld precedent — this
  is its emission-side dual).
- Governance A8.1/A14.2/A14.3: TAU_MODEL is the single central
  resolution for vertex merging; no new tolerance introduced.

### 9a. Analytical vs approximate

Index-level topology surgery + one exact-f64 distance test per
intersection segment. No surface approximation, no vertex motion, no
SSI involvement.
