# yang-rs — KV15: per-vertex planar near-weld for MIXED operands

**Status:** spec (FIP Phase 1) — root cause fully measured 2026-07-10.
**Change class:** bug fix (modeling-related). **Crate:** `yang-rs`
(`boolean()` step (2), the post-arrangement vertex weld).

## 1. Goal

A boolean whose operands carry planar femto-twin vertices (two distinct
f64 points within the scale-relative rounding band, produced by upstream
chained arithmetic) must reassemble a valid 2-manifold output B-Rep even
when the model also contains curved faces. Today the six-case
`InvalidBooleanOutput("an undirected output edge is not used by exactly
two directed edges")` corpus class (F0070 / F0076 / F0079 / F0081 /
F0084 / R0076 — chained stacked-Z and off-axis extrude scenarios) fails
at kernel-v2's edge-pairing validation.

## 2. Measured mechanism (2026-07-10, F0070 Extrude-12 union et al.)

1. Upstream ops mint planar vertex twins ≤ ~3e-14 apart. Measured mint
   sites differ per subfamily — F0070/R0076: the Stage-0 trapezoidal
   overlay's `lift_or_snap` branch mints two distinct exact sweep
   vertices in ONE pair's overlay (probe `[stage0-twin-probe]`, pair
   (322,1), overlay verts 668/675, dist 9.7e-16); F0081: minted before
   the failing boolean by non-boolean machinery (twins already in input
   A's B-Rep, verts 17/18, dist 9.8e-17, no Stage-0 pair in the chain).
   The mint sites are legion; the choke point is shared.
2. The PR-KV10 near-aware weld at the arrangement boundary
   (`lib.rs` step (2), band `TAU_WORK·(1+scale)`) exists precisely to
   reconcile this class — but it is gated on `all_planar` (EVERY face of
   BOTH operands planar). Every failing case has a circle/gear profile
   somewhere in the accumulated chain, so the weld falls back to
   bit-exact and the twins survive.
3. Downstream, the kept mesh carries the twins' femto membrane
   (measured F0070: top-plane degenerate tris (842,845,843)/(843,845,844)
   + lateral degenerate (1685,845,843); undirected edge (842,845) used by
   ONE triangle, edges (842,843)/(843,845) by THREE). Stage-6 patch
   boundary walks then legitimately disagree — the top patch swallows
   twin 843 as patch-interior and walks the chord (842,845); the lateral
   patch walks (845,843),(843,842) — and kernel-v2's manifold edge
   pairing rejects the output. The §4B loop T-subdivision cannot repair
   it (vertex 843 projects AT the endpoint, t≈1, not interior; gate
   `had_fold_sliver` false at every failing site — measured
   `[s6-split-probe]`).

## 3. Design

Replace the weld's whole-model `all_planar` gate with PER-VERTEX
eligibility, leaving the all-planar path byte-identical:

- **Eligibility:** an arrangement vertex is *planar-only* when every
  incident arrangement triangle descends — via `la.source[t]` and the
  operand's Stage-1 `tri_face` map — from a face whose surface is
  `Surface::Plane`. A triangle with EMPTY provenance (`la.source[t]`
  empty, e.g. the sidecar parity producer) or an out-of-range map entry
  marks its vertices INELIGIBLE (conservative: today's behavior).
  Eligibility propagates through bit-exact weld clusters (a root is
  ineligible if ANY member is).
- **Weld:** in the mixed branch, after the existing bit-exact weld,
  union planar-only cluster roots within the per-pair band
  `TAU_WORK·(1+max|coord|)` — the identical criterion, grid, and
  min-index-survivor rule as the shipped all-planar KV10 weld.
  Curved-adjacent vertices NEVER near-weld (kv9: cyl×cyl junction
  duplicates are structurally distinct, one copy per incident surface's
  chord ring; Stage-4 owns their collapse — welding them collapses
  lens-tip seam edges into degenerate loops, found RED by
  `kv9_cyl_cyl_special` on the first attempt).
- The existing downstream machinery is unchanged: welded triangles with
  repeated indices drop at kept-mesh compaction; the PR-6
  coincident-cylinder rim weld composes after, as today.

This is the same reconciliation principle as KV10 (representation
hygiene at the producer boundary, §4.5.5 snap / Yang Fig. 11(b) merge):
genuinely distinct model features are ≥ MIN_FEATURE_SIZE apart — six
orders beyond the band — so only redundant reconstructions of one
geometric point fuse. It is NOT a tolerance bucket (the reverted F0057
hazard): membership is provenance-gated, the band is representability-
scale, and the survivor keeps its own exact coordinates.

## 4. Branch table

| # | Configuration | Behavior |
|---|---|---|
| W1 | All-planar operands | Byte-identical to shipped KV10 near-weld (code path untouched) |
| W2 | Mixed operands, near-pair with BOTH roots planar-only | Welded (NEW — the fix) |
| W3 | Mixed operands, near-pair with EITHER root curved-adjacent | Bit-exact only (unchanged, kv9 protection) |
| W4 | Triangle with empty/out-of-range provenance | Its vertices ineligible (unchanged behavior for sidecar producer) |
| W5 | Genuinely distinct features (≥ MIN_FEATURE_SIZE) | Outside band, never fuse |
| W6 | Bit-exact duplicates (any adjacency) | Welded as today (first pass unchanged) |

## 5. Invariants

- I1: survivor of every weld cluster is the minimum original index;
  coordinates unchanged (`la.mesh.verts[welded]` stays valid).
- I2: no vertex with a curved-face-descended incident triangle is ever
  near-welded (band > 0) to anything.
- I3: the all-planar branch output is bit-identical to pre-KV15.
- I4: per-pair band is `TAU_WORK·(1+max component magnitude of the two
  points)` — identical to KV10's.

## 6. Oracles

- RED trackers (test-harness `edge_pairing_twin_weld_campaign.rs`):
  corpus replays of R0076 / F0070 / F0081 assert NO auto-union failure
  containing "not used by exactly two directed edges". RED before the
  fix; F0070/F0081 GREEN after. **Measured split (2026-07-10): R0076 is
  a DIFFERENT subfamily** — its failing twins arrive in the chained
  input at ~3.9e-8 apart (genuinely distinct exact crossings of
  near-parallel geometry, sub-floor but eight orders above the
  representability band), so this weld correctly excludes them; welding
  at the feature floor is the reverted-R0091 hazard. Its tracker is
  quarantined `#[ignore = "KV15b …"]`; the fix belongs at the minting
  boolean (A14.2 sub-floor collapse at emission), a separate cycle.
- Unit (yang-rs): W2 welds a planar femto pair in a mixed model; W3
  keeps a curved-adjacent femto pair distinct; W5 keeps ≥-floor pairs
  distinct (band arithmetic, mirroring `pr6_rim_weld_fuses_only_sub_ulp_
  duplicates`).
- Regression gates: yang-rs + kernel-v2 + cherchi-rs suites; full assay
  ZERO-LOST (no SUPPORTED_CORRECT regressions; target F0070/F0076/F0079/
  F0081/F0084/R0076 ERROR→CORRECT or downstream-typed).

## 7. Failure modes

- A welded twin whose surviving topology is still inconsistent fails
  loudly downstream at the existing Stage-6 / kernel-v2 validations
  (unchanged error surfaces).
- Empty provenance producers (sidecar oracle) keep today's bit-exact
  behavior — parity runs unaffected.

## 8. Research basis

- [#24] Yang et al. 2025 §4.4.1(b) (merge of sub-feature-size vertex
  pairs; the exact point survives) and §4.5.5 (identical-mesh
  reconciliation across coincident regions).
- Cherchi 2020/2022: the exact arrangement legitimately keeps
  ULP-distinct input points; reconciliation is the consumer's
  representation-hygiene concern (PR-KV10 precedent, roadmap M8).
- Governance A14.2: two points closer than the smallest representable
  feature ARE the same point.

### 8a. Analytical vs approximate

Index-level topology + exact per-pair band test on f64 coordinates. No
surface approximation, no vertex motion (survivor keeps its exact
coordinates), no SSI.
