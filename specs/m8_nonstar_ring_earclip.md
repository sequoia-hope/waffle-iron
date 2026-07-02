# M8 — Non-star subdivided-ring triangulation: exact ear-clip fallback

**Status:** spec (FIP Phase 1). **Change class:** bug fix (modeling-related), M8 workstream.
**Crate:** `yang-rs` (`stage0.rs::triangulate_ring`).

## 1. Goal

Stage-0's split-neighbor re-triangulation (`build_stage0_mesh` →
`triangulate_ring`) must handle a **non-convex (reflex)** planar neighbor face
whose boundary was subdivided by coplanar-overlay split points. Today
`triangulate_ring` tries (a) a boundary-vertex apex fan and (b) an
interior-centroid fan (star-shaped only) and returns `None` otherwise → the
loud `build-mesh-triangulate` / `CoplanarFacesUnsupported` wall.

Probed corpus blockers in this class (2026-07-02): R0046 (ring 9), R0088
(ring 12), R0098 (ring 11), F0061 (ring 23); TIMEOUT-category cases F0067,
F0069, F0071 share the signature.

## 2. Parameters

None user-facing. All arithmetic exact (`RBig` over the existing dominant-frame
2D projection). No tolerances introduced (A14.3).

## 3. Branch table

| # | Ring configuration | Behavior (after) |
|---|---|---|
| B1 | Boundary-vertex apex fan succeeds | Unchanged (byte-identical), tried first |
| B2 | Interior-centroid fan succeeds (star-shaped) | Unchanged, tried second |
| B3 | NEW: both fans fail (reflex ring) | Exact EAR-CLIP: repeatedly clip a ring-consecutive triple `(a, b, c)` with strictly positive exact cross whose CLOSED triangle contains **no other ring vertex** (exact containment); succeed iff the ring fully clips and the exact coverage certificate `Σ clip areas == ring area` holds |
| B4 | Ear-clip stalls (no clippable ear — e.g. every candidate diagonal passes exactly through a split point) | `None` — the loud wall persists (honest residue, never a guess) |
| B5 | Degenerate ring (n < 3, zero exact area, non-finite projection) | Unchanged: `None` |
| B6 | AMENDMENT (measured on R0046/R0098/F0061, 2026-07-02): the ring carries **consecutive bit-identical duplicate indices** (a split point interned to the same mesh vertex as a ring corner → a zero-length ring edge; also a duplicated ring closure). Collapse consecutive duplicates (and a first==last closure) by EXACT index equality BEFORE strategy selection — a zero-length edge carries no geometry, the vertex itself survives via its other copy, so no point is chorded over. This is exact identity, never a tolerance weld. |

### Measured residue (out of scope, stays B4-loud)

R0046 (f=4) and F0061 (f=2) rings additionally carry **femto-twin runs**:
consecutive DISTINCT vertices ~1e-16–1e-17 apart — the same geometric split
point minted twice (once per side of the overlay/edge-split machinery), the
known 1-ulp §4.5.5 conformality-break class (`kernel_v2_m8_coplanar_landscape`
memo). The ring zigzags at femto scale, so no strictly-positive ear adjacent
to the twins survives closed containment → B4 stall, loud. The fix is
UPSTREAM split-point identity (mint ONE shared point), a separate mechanism —
a tolerance weld here is prohibited (P9; gear-flange banked lesson). R0088
additionally blocks on a cherchi `LabelMismatch` (same femto-twin family,
inside the arrangement).

## 4. Invariants

- I1 (no chord over a split point): every ring boundary segment
  `(ring[i], ring[i+1])` appears as an edge of exactly one emitted triangle —
  the gap-free boundary tiling that makes the re-tessellated neighbor
  edge-conform with the overlay face (the YR26 T-junction hazard this
  function exists to prevent). The closed-containment ear test enforces this:
  a candidate ear whose triangle touches any other ring vertex (interior OR
  boundary) is rejected.
- I2 (strict positivity): every emitted triangle has strictly positive exact
  area in the ring's orientation frame; no zero-area triangle is emitted or
  silently dropped.
- I3 (exact coverage): `Σ` exact clip areas `==` exact ring area (P9 gate,
  same certificate as the fans).
- I4 (no new vertices): B3 adds no vertex (unlike B2's centroid) — output
  indices ⊆ ring indices.
- I5 (byte-identical fast paths): rings that today succeed via B1/B2 produce
  byte-identical output (the ear-clip runs only after both fail).
- I6 (determinism): fixed scan order (first clippable ear in ring order) —
  same ring → same triangulation.
- I7 (regression gate): full assay `SUPPORTED_WRONG == 0`; no
  `SUPPORTED_CORRECT` lost (timeout flips are noise).

## 5. Oracles

- Unit (in `stage0.rs` tests, via the `triangulate_ring` seam):
  - L-shaped (reflex) ring with split points on ≥2 edges (incl. collinear
    runs) → `Some`, I1 boundary-tiling check (every consecutive ring pair is
    an edge of exactly one triangle), I2 strict positivity, I3 coverage,
    I4 no new vertex.
  - Convex subdivided ring → still handled by B1/B2 (assert the vertex count
    stays `ring.len()` for B1 or `ring.len()+1` for B2 as today — I5 guard).
  - A ring the ear-clip cannot finish (construct a stall or degenerate case,
    e.g. bowtie/self-touching ring) → `None` (B4/B5 guard).
- B6 unit: a ring with a consecutive duplicate index (and a duplicated
  closure) triangulates as if deduplicated; boundary tiling (I1) holds over
  the DEDUPED ring; `verts.len()` unchanged.
- E2E RED→GREEN trackers (campaign file `m8_intra_opposite_campaign.rs` or a
  sibling): R0046, R0088, R0098, F0061 — assert the failure set does not
  contain the coplanar wall (success or a different typed error both pass).
  AMENDMENT: with B6, R0098 goes GREEN; R0046/F0061 stay RED on the
  femto-twin residue and R0088 on `LabelMismatch` (see "Measured residue") —
  their trackers stay `#[ignore]`d RED with the blocker named.
- I7: full assay.

## 6. Failure modes

- Stall (B4): loud `None` → existing typed wall; never emit a partial or
  overlapping triangulation (the coverage certificate would catch it anyway).
- A ring with duplicate vertex indices or repeated coordinates: containment
  test sees the duplicate as "another ring vertex on the triangle" → clips
  around it or stalls loudly; never panics.
- Collinear split points are never clipped as ears (zero cross ⇒ not
  strictly positive) and never chorded over (I1); they are consumed as base
  edges of ears clipped elsewhere.

## 7. Research basis

Two-ears theorem (Meisters 1975) guarantees a simple polygon has a clippable
ear; the closed-containment variant on a weakly-simple subdivided ring is the
standard robust ear-clipping formulation — the same algorithm family as
Livesu et al. 2021 [#39] (the project's cited triangulation reference) and
kernel-v2's exact `ear_clip` (tessellate.rs), here in exact rationals over
the Stage-0 shared frame. Where the theorem's precondition fails (weakly
simple with exact-on-diagonal vertices), B4 stalls loudly rather than
degrading — per A15.2/P9 no approximate fallback is permitted.

### 7a. Analytical vs approximate

Pure 2D exact triangulation of an existing planar ring; no SSI, no surface
approximation. f64 enters only at the pre-existing projection boundary
(`ExactPoint2::from_f64` of already-f64 ring coordinates — exact embedding).
