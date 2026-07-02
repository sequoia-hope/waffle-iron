# M8 — Chained-output vertex canonicalization (femto-twin elimination)

**Status:** spec (FIP Phase 1) — REWRITTEN 2026-07-02 after instrumentation
disproved the draft's cross-solid design (see §8 history). **Change class:**
bug fix (modeling-related), M8 workstream — the unified root behind the
femto-twin walls. **Crate:** `kernel-v2` (`to_yang_brep`, directly after
`canonicalize_sibling_planes` — the KV10 completion).

## 1. Goal

A chained boolean output re-enters the next boolean with vertex coordinates
that are EXACTLY consistent with its canonicalized face planes, so that
intended-straight edges are exactly straight and intended-plane-constant
coordinates are bit-constant. Today each output vertex carries independent
~1e-16 rounding (arrangement/relocation history), so a re-imported face loop
is **femto-crooked**: R0076's operand quad has its "vertical" edge endpoints
4.4e-16 apart in the sweep coordinate (measured, `YANG_POLY_PROBE`).

The Stage-0 exact overlay faithfully arranges that crookedness: femto-wide
sweep slabs, needle cells (`RoundingCollapse` — R0076 sliver area 1.6e-17,
R0081 3.6e-21), femto-twin boundary/split vertices in neighbor rings
(ear-clip B4 stalls — R0046, R0070, F0061, F0063), and near-coincident
cross-input vertices inside cherchi (`LabelMismatch` — R0088 layer 1, R0070
op 2). One vertex-hygiene pass at the producer boundary addresses all of
these at the root — per the KV10 precedent (plane-bit canonicalization) and
A8.2 (explicit, diagnosable healing; no silent downstream tolerance).

## 2. Mechanism

In `to_yang_brep`, after `canonicalize_sibling_planes`, re-derive each
vertex from its incident CANONICAL planes — restricted (this increment) to
vertices whose incident faces are ALL planar:

- Collect the vertex's incident faces' canonical `(n, d)` planes, deduped by
  exact plane identity (same or exactly-negated bits = one plane).
- **≥3 pairwise-independent planes** → the vertex is the exact rational
  solution of the first 3 independent ones (exact 3×3 solve; independence =
  exact determinant ≠ 0 with a conditioning floor, see B6); round once to
  f64.
- **Exactly 2 independent planes** → project the vertex exactly onto the two
  planes' intersection line (rational), round once. (Edge-subdivision
  vertices: straightens intended-collinear chains.)
- **< 2** → unchanged.
- **Displacement guard**: if the re-derived point moves the vertex by more
  than the KV10-scale band `TAU_WORK·(1 + |coord|)` (per component), leave
  the vertex UNCHANGED (loud probe tag `vertex-canon-over-band`) — the
  vertex genuinely isn't at those planes' intersection; never force it.

Vertices with any curved incident face are untouched (arc/rim endpoints must
stay exactly on their curves — cylinder rim bit-sharing is load-bearing).

### 2b. In-frame coordinate clustering (AMENDED mid-GREEN, measured)

The world-space pass alone closed R0070 (axis-aligned) but NOT R0076/R0081:
their operand quads' WORLD coordinates are already consistent (intended-equal
z pairs are bitwise equal), yet their pair-frame u coordinates split by
~4.4e-16 — the femto-crookedness is minted by the f64 FRAME PROJECTION
(`(p−o)·e1` rounds independently per vertex), so an intended-frame-vertical
edge of an OBLIQUE solid is femto-off vertical and the exact sweep still
builds femto slabs → needle cells → `RoundingCollapse` (now at different
triangle ids — measured).

Second layer, in `stage0.rs` where the pair's 2D polygons are built: cluster
the PROJECTED u coordinates (and, independently, v) of BOTH faces' loop
vertices — values within the pair `band` snap to the cluster's first-seen
representative (greedy in a deterministic vertex order; A's loop first).
Clusters are isolated (features are ≥ MIN_FEATURE_SIZE apart, six orders
above band — the KV10 margin argument), so chain drift is impossible. Both
polygons and every downstream consumer of projected input coordinates
(corner keys, rim maps) read the SAME clustered values, so intended-equal
frame coordinates are BIT-equal across the pair — §4.5.5's identical
boundary sampling in the domain where the overlay actually operates.

Additional branch rows:

| # | Configuration | Behavior |
|---|---|---|
| C1 | Two projected coords within band (same or cross polygon) | Snap to first-seen representative (per axis, independent) |
| C2 | Coords farther than band | Untouched |
| C3 | All-distinct clusters (generic oblique geometry) | Byte-identical path |

Additional invariants:

- I7: after clustering, no two input-polygon coordinates on one axis differ
  by a nonzero amount ≤ band (twin-free events).
- I8: per-coordinate displacement ≤ band; cluster representatives are
  members (no averaging — a representative is an original projected value).

**Scope limit (measured, GREEN-2):** clustering applies to PURE-POLYGON
pairs only (`rim_a`/`rim_b` empty). A disc rim's 2D samples are projections
of exact 3D ring points bit-shared with the cylinder lateral, and a regular
ring's symmetric samples legitimately carry femto-near-equal coordinates —
naive clustering welded them and broke the rim-chord ↔ lateral exact
correspondence (m8_disc_coplanar `cylinder_cap_crossing` regressed to
cherchi `LabelMismatch`); an immovable-seed variant (rims never move,
polygon coords snap TO them) regressed three disc fixtures to earlier
Stage-0 walls instead (the snapped corners violate the disc-pair
machinery's exact expectations). Per P10 both variants were reverted, not
iterated blindly: rim-aware clustering is a FOLLOW-UP cycle with its own
design. Consequence: F0061 (rim-carrying femto-twin case) stays loudly
walled; its tracker stays RED naming this gap. I7 therefore holds over
pure-polygon pairs only.

## 3. Branch table

| # | Vertex configuration | Behavior |
|---|---|---|
| B1 | All-planar incident, ≥3 independent canonical planes | Exact 3-plane solve, round once, adopt if within band |
| B2 | All-planar incident, exactly 2 independent planes | Exact projection onto the intersection line, adopt if within band |
| B3 | All-planar, <2 independent planes (coplanar-only incidence) | Unchanged |
| B4 | Re-derived point moves > band in any component | Unchanged + probe (loud residue, never forced) |
| B5 | Any curved incident face | Unchanged (curve exactness owns the vertex) |
| B6 | ≥3 planes but near-dependent triple (exact det ≠ 0 yet ill-conditioned: fall below an exact independence floor — `|det|² ≤ tol²·(row-norm products)` in rationals) | Degrade to B2 with the best-conditioned pair; if none, B3 |
| B7 | Already-exact vertex (bit-equal to the re-derivation) | Byte-identical no-op |

## 4. Invariants

- I1 (straightness): two vertices sharing the same 2 canonical planes lie on
  those planes' exact intersection line to within one f64 rounding each — in
  an axis-aligned fixture, plane-constant coordinates become BIT-constant.
- I2 (bounded motion): every adopted displacement ≤ band per component
  (verified exactly); B4 guards the rest.
- I3 (idempotence): running the pass twice is byte-identical to once.
- I4 (already-exact inputs byte-identical; AMENDED pre-RED): a vertex that
  already satisfies its planes bit-exactly is untouched (B7) — axis-aligned
  fixtures are byte-identical. An OBLIQUE fresh extrude's non-anchor corners
  carry ~1e-16 residuals by construction (Newell normal + first-vertex `d`
  anchoring), so they legitimately move ≤ band onto their own planes'
  exact intersection — canonicalization inverts derived-plane vs vertex
  authority by a sub-band amount, which is the point. The behavioral gate
  for that blast radius is fuzz_boxes 900/900 CORRECT + the full-assay I6,
  NOT byte-identity of oblique pipelines.
- I5 (determinism): plane collection in face-index order; first-3
  independent selection deterministic.
- I6 (regression gates): full assay `SUPPORTED_WRONG == 0`; rewrite tier
  green; no `SUPPORTED_CORRECT` lost (30s-cap flips long-cap verified).

## 5. Oracles

- kernel-v2 unit (via `to_yang_brep` on hand-built arenas):
  - A femto-crooked box (each corner independently perturbed ±≤2e-16 off the
    exact plane intersections; planes carried exactly) → every corner
    bit-equal to the exact tri-plane intersection (B1, I1); repeat → I3.
  - A subdivided edge vertex (2 planes + collinear-intended) perturbed
    femto-off the line → lands exactly on the line (B2).
  - A corner perturbed 1e-6 (≫ band) → unchanged (B4 guard).
  - An exact box → byte-identical (B7/I4); a cylinder-incident vertex →
    unchanged (B5).
- E2E RED→GREEN trackers (new file `m8_vertex_canon_campaign.rs`):
  red_r0076, red_r0081 (RoundingCollapse walls today), plus re-point the
  existing m8_earclip trackers' expectations where cases progress
  (R0046/F0061 stall walls) and red_r0070. Tracker target: the coplanar wall
  string absent (success or a different typed error pass).
- I6: full assay on a quiet box.

## 6. Failure modes

- A vertex genuinely off its faces' intersection (defeatured/tolerant
  geometry) → B4 unchanged + probe; downstream walls stay loud as today.
- Near-parallel plane triples (thin wedges) → B6 conditioning floor keeps
  the solve stable or degrades. ADVERSARY-MEASURED (aa1de833): the floor is
  LOAD-BEARING, not a mere backstop — without it an ill-conditioned solve
  is rejected by the band guard and the vertex stays crooked; with it the
  triple degrades to B2 and the vertex is correctly straightened onto the
  well-conditioned pair's line. Also measured: the negated/duplicate plane
  dedup is structurally redundant with the floor (det ≡ 0 for any dependent
  triple) — kept for legibility/perf; the correct behavior is pinned.
- Mixed exact/inexact chains: partial adoption is safe — each vertex's
  adoption is independent and band-bounded; conformality across faces of
  the SAME solid is preserved because all its faces read the same mutated
  vertex array.

## 7. Research basis

Representation hygiene at the producer boundary, completing PR-KV10 [roadmap
M8 slice d]: planes were canonicalized there, vertices here. Exact
plane-intersection re-derivation is standard rational linear algebra (no
published-algorithm gap); the band-guarded adopt mirrors KV10's cluster
band. Yang 2025 §4.5.5 [#24] motivates WHY: the overlay's identical-mesh
guarantee presumes inputs whose intended-coincident geometry is exactly
coincident; femto-crooked chains violate that upstream of the overlay.

### 7a. Analytical vs approximate

Exact rational solves/projections, rounded once to f64 per adopted vertex.
No SSI, no surface approximation.

## 8a. Gate failure + unwiring (P9/P10 record, 2026-07-02)

The full-assay gate with BOTH layers wired flipped **R0064 and F0047 to
SUPPORTED_WRONG** (F0047: 53 unpaired edges, 45 non-manifold, Euler 42 —
a silently wrecked mesh; R0064: 1 degenerate render triangle). Bisection:
the WORLD-SPACE pass (§2) is the trigger for both; the in-frame clustering
(§2b) alone leaves them fully correct. Tracker matrix: both layers = 6/7
GREEN but disqualified; clustering-only = R0070/R0076/R0081/R0098 GREEN,
R0046/R0088 regress to their walls (their femto-twins are world-level),
F0061 unchanged (rim-aware gap). No static predicate separates the
population §2 fixes from the one it breaks (both are planar-solid ×
curved-solid classes), so §2 is UNWIRED — the primitive and its unit suite
stand (banked, N2-1 pattern) and wiring returns WITH the downstream
investigation: femto-band input motion must never yield a silent
non-manifold output — F0047's wrecked-mesh path is a missing loud gate in
the ellipse-junction pipeline, to be fixed first.

## 8. History (P10 record)

The 2026-07-02 DRAFT designed a cross-solid near-collinear edge snap
(A/B shared-boundary reconciliation). Instrumentation on R0076
(`YANG_POLY_PROBE`) DISPROVED it: the failing pair has ZERO near-collinear
A/B edge pairs (A is a 4-vertex quad vs a 476-vertex gear chain); the femto
structure is INTRA-solid — A's own chained-output corners are femto-off
their plane intersections. The cross-solid snap would not have fixed the
measured cases; per P10 that design is aborted, not improvised around.
