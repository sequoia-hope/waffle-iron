# M8 — Femto-slab sliver T-subdivision at the overlay emission gate

**Status: REFUTED — P10 abort record (2026-07-10).** The candidate fix
(local emission-gate surgery: exact T-subdivision + same-class quad
flips) was implemented, measured against all three corpus targets, and
REVERTED: two of the three measured sub-mechanisms are not repairable by
ANY local retriangulation over the fixed rounded vertex set (see §8).
What ships from this cycle: the `[sliver-probe]`/`[pocket-probe]`
structure census in the gate's error path (env-gated `YANG_POLY_PROBE`),
the corpus wall pins (`yang-rs/tests/m8_overlay_femto_slab_emission.rs`:
active stays-loud pin + `#[ignore]`d green target), and this record.
The sections below are kept as the design that was tried, plus the
measured refutation — the green target's oracle stack (§5) remains the
acceptance bar for whichever mechanism eventually lands.

**Crate:** `yang-rs` (`src/coplanar_overlay.rs`, step-6 sliver-collapse gate)
**Corpus targets:** F0067, C0048, R0053 (`overlay-failed RoundingCollapse`);
mechanism (ii) of the 2026-07-10 coplanar-wall census (roadmap M8 item 3c).

## 8. Refutation (measured 2026-07-10, C0048 pair=(1,0) + R0053)

The prototype (T-subdivision candidates over all three edges longest-
first with exact-positivity + strict-progress validity; same-class exact
quad-flip fallback; skip-and-retry worklist over the sliver set; loud on
a no-progress pass) repaired the clean slab needles (the first C0048
sliver repaired exactly as designed) and then hit two structures no
local surgery can fix:

1. **Chord-collinear mint triples (C0048).** The twin-corner clusters
   mint crossing points EXACTLY collinear on an input chord (probe:
   overlay verts 19, 22, 26 all on A's chord line, 19/22 one ULP apart).
   The degenerate region's only off-chord vertices round onto the SAME
   f64 event column as the twins, so EVERY triangle over the cluster's
   vertex set is either exactly degenerate (all-chord) or f64-collinear
   (all-column). A valid emission needs new geometry — per-region
   re-emission that fans femto boundary sub-segments to FAR apexes
   (off-chord AND off-column), i.e. a constrained snap-rounding-grade
   re-triangulation, not local surgery.
2. **Rounded-order-inverted twin pairs (R0053).** Two mints on one input
   edge, on the UNION BOUNDARY (probe: verts 1476/1469, exact gap
   sub-ULP), whose ROUNDED y-order is inverted relative to their exact
   order. Any triangle carrying the twin edge rounds degenerate or
   FLIPPED regardless of apex (measured: exact-positive piece with
   rounded cross ≈ −2.4e-15). No triangulation over the fixed rounded
   vertex set exists; the fix belongs at the MINT SITE (collapse
   sub-representable twin mints at emission — the KV15b / A14.2 class,
   precedent: M8 increment 4's Stage-0 sub-floor shared-mint collapse).
3. **Termination trap (for the record):** free chaining of T-subdivision
   through sliver neighbours CYCLES (measured: 5 slivers / 151 tris
   fixpoint churn under a 4n cap); the strict-progress gate (pieces must
   not be new collinear slivers) restores termination but leaves the two
   structures above stuck, which is what makes them honest walls.

## 1. Goal

Mirrored disc-rim samples on chained coplanar operands carry 1–2-ULP-split
frame projections (rim coordinates are §2c-excluded from clustering, spec
`m8_shared_boundary_identity` — input welding is a P10-reverted dead end).
The exact trapezoidal sweep faithfully builds femto event-column slabs from
the twins. Each collapsed slab cell ear-clips into triangles that are
**exactly CCW-positive** but whose three vertices round to **distinct,
collinear f64 points** (measured on all three corpus targets: all three
verts share one f64 x; two verts are 1-ULP/femto y-twins, the third is
far). Today the step-6 rounding gate rejects these loudly
(`RoundingCollapse`) because dropping such a sliver alone would leave a
T-junction: the neighbor across the sliver's LONG edge keeps the chord
`A–B` while the chain neighbors keep `A–C`, `C–B`.

The fix (N22 pattern — `yang_stage6_sliver_topology` §B loop
T-subdivision, applied at the 2D emission gate): **drop the sliver and
T-subdivide its long-edge neighbor at the middle vertex.** For sliver
`(A,B,C)` with middle vertex `C` (exactly interior to the span of long
edge `A–B`) and neighbor `(B,A,X)`:

```
drop (A,B,C);   replace (B,A,X) with (B,C,X) + (C,A,X)
```

This is exact-coverage-neutral for the union — the wedge the split adds
to the neighbor's side equals the dropped sliver region, by the signed
identity `area(B,A,X) + area(A,B,C) = area(B,C,X) + area(C,A,X)` — and
restores edge conformality (`A–C`/`C–B` now bound the split pieces). Both
pieces are exactly CCW (C and X lie strictly on opposite sides of line
`A–B`, and C's foot lies strictly inside `A–B`'s span; asserted at run
time, loud on violation). The split pieces are also better
f64-conditioned: each has the full off-column base, so their rounded
areas keep the dominant representable term.

**Long-edge selection:** the long edge is the triangle's exactly-LONGEST
edge (exact squared length; tie broken by smallest edge index, for
determinism). The middle vertex is the opposite vertex. The foot of the
altitude onto a triangle's longest side always lies strictly inside that
side (classical), so a "no interior middle vertex" branch cannot arise
and is refactored away (Constitution §7); the piece exact-positivity
asserts are the loud backstop.

**Interior femto-cluster slivers (measured on C0048 behind the slab
needle):** ULP-split input corners mint crossing clusters whose interior
micro-triangles have their longest edge exactly ON an input chord (two
consecutive crossing mints along it). T-subdivision is unsound there
(B8), but such a sliver sits in a same-class pocket — its chain-edge
neighbors share its class — so an exact 2-2 quad flip (B9)
retriangulates the same-class quad without touching the on-chord edge:
coverage- and class-exact, strict-progress-gated (both new triangles
must be rounded-positive).

**Measured structure (2026-07-10 `[sliver-probe]` census, all three
targets):** sliver = far vertex + 1-ULP y-twin pair, all on one f64 x
column; middle vertex = inner twin; long edge NOT on any input segment,
neighbor exists and has the SAME class as the sliver (so even the
per-class exact identity is undisturbed for these cases); the femto twin
edge lies ON an input segment but is a chain edge — untouched by repair
(its triangle-use count is preserved: the dropped sliver's use is
replaced by a split piece's use).

No tolerance is introduced. The repair happens strictly AFTER the step-5
exact coverage post-conditions (which validate the exact 2D boolean on
the full pre-repair overlay) and only ever rewrites triangles whose f64
images are degenerate (zero/negative rounded area).

## 2. Parameters

`coplanar_overlay(a, b)` — no new inputs, no new tolerances. One internal
constant: an iteration cap for the repair worklist (`4 × initial triangle
count`, generous — each collapsed slab cell resolves in ≤ 2 splits per
side; termination backstop only, exceeded ⇒ loud `RoundingCollapse`).

## 3. Branch table

| # | Triangle disposition after f64 rounding | Before | After |
|---|---|---|---|
| B1 | Strictly CCW-positive | kept | kept (unchanged) |
| B2 | Coincident-pair needle (two verts round to the SAME f64 point) | dropped (benign; interner welds) | dropped (unchanged) |
| B3 | Collinear sliver; longest edge off-input with a neighbor | loud `RoundingCollapse` | repaired: sliver dropped, neighbor T-subdivided at middle vertex; pieces re-enter the gate |
| B4 | Collinear sliver, longest edge is a BOUNDARY edge (no neighbor) | loud `RoundingCollapse` | T-subdivision inapplicable → falls through to B9 flip; loud only if B9 also inapplicable |
| B6 | Repair worklist exceeds iteration cap | — | loud `RoundingCollapse` (termination backstop) |
| B7 | T-subdivision split piece not exactly CCW-positive | — | loud `TriangulationFailed` (internal invariant; cannot happen — middle and X strictly on opposite sides of the longest edge, foot strictly inside its span) |
| B8 | Collinear sliver whose LONGEST edge lies exactly ON an input segment | loud `RoundingCollapse` | T-subdivision inapplicable (splitting would insert an off-segment vertex into the input-edge tiling — the M-A exact-matching hazard) → falls through to B9 flip |
| B9 | Interior femto sliver (T-subdivision inapplicable) with a SAME-CLASS neighbor forming an exactly-flippable quad | loud `RoundingCollapse` | repaired by exact 2-2 flip: quad `a→y→b→c→a` (sliver `(a,b,c)` + neighbor `(b,a,y)`) retriangulated as `(a,y,c)` + `(y,b,c)`. Coverage-exact (same quad), class-exact (same class); allowed only if the flipped-away edge `(a,b)` is off-input, both new triangles are exactly CCW AND rounded-CCW (strict progress: sliver count decreases). Edges tried in deterministic k order |
| B10 | Collinear sliver, no applicable T-subdivision or flip | loud `RoundingCollapse` | loud `RoundingCollapse` (unchanged) |

Chained-sliver case (a collapsed slab cell ear-clips into TWO adjacent
slivers; the first sliver's long edge is the cell diagonal, its neighbor
is the second sliver): B3 applies recursively — splitting a sliver
neighbor yields two smaller slivers whose long edges are the cell walls,
each adjacent to a real (off-column) cell; the worklist converges.

## 4. Invariants

- I1 (exact union coverage): the union of emitted triangles equals the
  union of the pre-repair overlay exactly — every repair step preserves
  covered region by the signed-area identity above. Post-repair
  certificate: `Σ area_exact(all classes)` is identical before and after
  repair.
- I2 (exact positivity): every emitted triangle has strictly positive
  exact area (asserted per split piece).
- I3 (conformality): every undirected edge of the emitted triangulation
  bounds ≤ 2 triangles, and the multiset of BOUNDARY edges (exactly-one-
  triangle edges) of the whole overlay is unchanged by repair (repair
  never touches boundary geometry — B4 stays loud). Input-edge tiling
  (yr25 property 4) is preserved: chain edges keep their use count (the
  dropped sliver's use is replaced by a split piece's), and the vanished
  longest edge is never on an input segment (B8 guard).
- I4 (f64 emittability): every kept triangle is strictly CCW-positive in
  the rounded f64 coordinates (same gate as before; the repair loop runs
  until no collinear sliver remains or fails loud).
- I5 (class absorption, documented deviation): a repaired sliver's femto
  region is absorbed into its long-edge neighbor's class. The absorbed
  exact area is sub-f64-representable (its f64 image has zero/negative
  area), so no rounded coordinate or downstream f64 mesh changes; the
  per-class exact identity `area(XOnly) + area(Overlap) = area(X)` may
  shift by the absorbed femto areas AFTER repair. The step-5 coverage
  check runs BEFORE repair and is authoritative for input validation.
- I6 (determinism): the worklist processes slivers in ascending triangle
  index; all structures stay index/BTree-ordered. Bit-identical outputs
  for identical inputs.

## 5. Oracles

- Unit RED→GREEN `c0048_mirrored_rim_slab_repair`: the C0048 corpus pair
  (mirrored 14-gon outer loops, verbatim f64 coordinates incl. the
  1-ULP-split rim samples). RED: `RoundingCollapse`. GREEN: overlay
  succeeds; assert I2 (exact area > 0 per triangle), I3 (edge count ≤ 2),
  I4 (f64 CCW per triangle), and exact union-area conservation
  (`AOnly + Overlap == area(A)` holds pre-repair by construction; post
  repair assert `Σ all-class exact area == exact union area` via a
  reference union computed from the same engine on the un-twinned model
  is NOT required — instead assert `area(AOnly)+area(Overlap)` differs
  from `input_area(A)` by at most the summed absorbed sliver areas,
  i.e. a quantity that rounds to 0.0 in f64).
- Unit synthetic `femto_slab_t_subdivision_branches`: constructed
  fixtures per branch —
  - B3 single sliver with real neighbor (assert triangle count = pre + 1
    − 1 + ... net: sliver dropped, neighbor → 2);
  - B3-chained double sliver (trapezoid cell fully collapsed);
  - B4 boundary long edge → still `RoundingCollapse`.
- Corpus: F0067, C0048 → the coplanar boolean proceeds past Stage 0
  (target SUPPORTED_CORRECT or a typed downstream error, never
  `overlay-failed`); R0053 same (revolve-tagged).
- Full assay zero-lost gate vs baseline 226C/0W/53E/15U/0T.

## 6. Failure modes

- B4/B5/B6/B7 as tabled — all loud, typed, never silent.
- Repair introduces NO fallback path for exact-arithmetic failures: only
  triangles whose f64 image is already degenerate are rewritten, and
  only in ways that preserve exact coverage of the union.

## 7. Research basis

- [#24] Yang et al. 2025 §4.5.5 — coplanar preprocessing must emit one
  shared triangulation with identical meshes for both models; emission
  must be f64-representable for the downstream exact mesh boolean.
- N22 (`specs/yang_stage6_sliver_topology.md` §B) — loop T-subdivision at
  on-segment foreign vertices: the established repair pattern for
  chord-vs-chain conformality defects; here applied in 2D at the
  emission gate where the exact geometry is still authoritative.
- Sterbenz lemma (classical FP) — the split pieces subtract nearby
  quantities exactly; their rounded signed areas keep the dominant
  representable term, which is why the repair terminates at real cells.
- Input welding at the twin site is explicitly DEAD: two P10-reverted
  variants, spec `m8_shared_boundary_identity` §2b/§2c scope limits.
