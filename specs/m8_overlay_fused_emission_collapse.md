# M8 — Fused emission via constrained edge collapse at the overlay rounding gate

**Status: SPEC (2026-07-12).** Successor to the P10 abort record in
`specs/m8_overlay_femto_slab_emission.md` §8 — this is the "per-region
re-emission / mint-site collapse" mechanism that record names as the honest
fix. The refuted approach (local T-subdivision + quad flips) searched for a
valid triangulation over the FIXED rounded vertex set; the refutation proved
none exists for the measured corpus structures (chord-collinear mint triples;
whole clusters rounding onto one f64 event column). This spec removes the
fixed-vertex-set premise: vertices of a sub-resolution degenerate complex are
FUSED (constrained edge collapse), after which the complex's f64-degenerate
triangles vanish by construction and the remaining triangulation is
f64-emittable.

**Crate:** `yang-rs` (`src/coplanar_overlay.rs`, step-6 rounding gate only —
no Stage-0 wiring changes in this increment).
**Corpus targets:** F0067, C0048 (`overlay-failed RoundingCollapse`, the two
remaining cases of task #130 mechanism (2)).

## 0. Measured context (Manager diagnosis, 2026-07-12)

- C0048's failing pair (verbatim fixture in
  `yang-rs/tests/m8_overlay_femto_slab_emission.rs`): mirrored 14-gon disc
  rims whose corner/sample chains are split by 1–2 ULPs. The overlay is
  exactly correct (step-5 coverage certifies), but the strip between the two
  chains rounds onto ONE f64 x-column (x = -1.1727472237020446): the pocket
  census shows needles and full-height collinear slivers spanning all three
  region classes on that single column.
- F0067: same signature at x = -0.2043166720325753 (arc-chain sample
  polygons, pair (27,0)); femto y-twins at ±0.01837738522865587..91, with
  real-scale neighbors ~1.5e-4 away.
- Passthrough experiment (temporary gate mutation, reverted): letting the
  slivers through moves the failure downstream — C0048 dies at the
  kernel-v2 rim-override refusal ("rim-crossing override … coincides with
  uniform sample k=12"), F0067 at Stage-0 `build-mesh-triangulate f=336`.
  Conclusion: raw passthrough is NOT the fix; the femto structure must be
  fused at the overlay emission so downstream consumers see clean geometry.
  Those downstream sites are the cases' NEXT honest walls and are expected
  to remain after this increment (each is its own follow-up); the overlay
  wall itself is what this increment retires.

## 1. Goal

`coplanar_overlay` / `coplanar_overlay_multi` must emit an f64-representable
shared triangulation (every kept triangle strictly CCW-positive in BOTH the
exact rational and the rounded f64 coordinates) for inputs whose exact
arrangement contains sub-f64-resolution structure, instead of failing
`RoundingCollapse`. Yang §4.5.5 requires identical meshes for both models on
the overlap region; for near-identical (ULP-split) boundary chains the
faithful f64 emission IS the fused chain — two chains with the same rounded
image merge into one, and the sub-representable strip between them is
absorbed (it has no f64 image; per A14.2 nothing below MIN_FEATURE_SIZE can
be a real feature, and the fusion ceiling here is far below even TAU_MODEL).

## 2. Parameters

`coplanar_overlay(a, b)` / `coplanar_overlay_multi(a, b)` — no new inputs.
One constant, from the centralized tolerance policy (A14.3, no ad-hoc
epsilon): a collapse-candidate edge is eligible only if its EXACT squared
length is `< TAU_MODEL²` (`cad_primitives::TAU_MODEL`, 1e-7 — the KV15b
precedent constant; the R0091 revert proved MIN_FEATURE_SIZE kills micro
models). This is an eligibility CEILING that makes the repair fail closed on
real-scale degeneracy (three real-scale points exactly f64-collinear stay a
loud wall); the repair TRIGGER is exact f64 degeneracy of the rounded image,
not a distance test.

## 3. Mechanism

Step 6 of the overlay (rounding gate) becomes:

1. Compute the rounded disposition of every triangle
   (`rounded_tri_disposition`, unchanged).
2. If NO triangle is `CollinearSliver`: legacy path, byte-identical —
   `Positive` kept, `CoincidentNeedle` dropped (the proven benign-weld
   path). This preserves bit-identical output for every currently-passing
   input (zero-regression requirement).
3. Otherwise enter the fused-emission repair loop:
   - Worklist: triangles with non-`Positive` disposition (needles AND
     slivers — inside repair mode both fuse), ascending triangle index.
   - For each such triangle, try its edges as collapse candidates in
     ascending EXACT squared-length order (deterministic tie-break:
     lexicographically smaller `[min(i,j), max(i,j)]` index pair first).
     A candidate is eligible iff its exact squared length `< TAU_MODEL²`.
   - Collapse survivor selection: an input-loop vertex (exact coords equal
     to a vertex of an input polygon loop of either side — the overlay
     re-derives this set from its own inputs) survives over a minted
     arrangement vertex; if both or neither are input-loop vertices, the
     smaller overlay index survives. The survivor keeps its own exact bits
     (KV15b min-index precedent; never an average). Rationale: fusing mints
     INTO existing input geometry minimizes downstream churn (a mint that
     fuses into a rim sample needs no rim override at all).
   - Validity gate (P9 — no silent damage): tentatively remap
     loser → survivor over all kept triangles. Triangles that become
     index-degenerate (a repeated vertex index) are dropped — their exact
     area is absorbed (bounded by candidate-edge length × local extent,
     sub-TAU_MODEL × domain scale). Every OTHER remapped triangle must keep
     EXACT area > 0. Any violation ⇒ reject this candidate, try the
     triangle's next edge; all candidates rejected ⇒ leave the triangle for
     a later pass.
   - Commit: apply the remap, record `fused[loser] = survivor`, restart
     disposition bookkeeping incrementally (a previously-Positive triangle
     may only change by vertex remap, and the gate re-checks any remapped
     triangle's disposition).
   - Termination: every committed collapse removes one referenced vertex,
     so committed collapses ≤ V. A full pass over the worklist with ZERO
     committed collapses while a `CollinearSliver` remains ⇒ loud
     `RoundingCollapse` (the wall is preserved, honest — B5).
4. On success (no `CollinearSliver` remains): drop remaining index-degenerate
   triangles and any remaining `CoincidentNeedle`s (rounded-coincident pairs
   whose collapse candidates were ineligible — same benign-weld argument as
   the legacy path), keep everything else. Publish the fusion record.

No vertex compaction (matches the legacy needle-drop behavior; the Stage-0
emission already compacts unreferenced vertices on its side).

`ClassifiedOverlay` gains one field:
`pub fused: BTreeMap<u32, u32>` — loser overlay index → surviving overlay
index (fully resolved: values are never themselves keys). Empty on the
legacy path. Consumers may use it to translate exact-coordinate identities
across the fusion (this increment adds no consumer; the Stage-0 wiring
follow-ups will).

## 4. Branch table

| # | Condition | Behavior |
|---|---|---|
| B1 | Triangle rounds strictly CCW-positive | kept (unchanged) |
| B2 | `CoincidentNeedle`(s), NO `CollinearSliver` anywhere | legacy drop, byte-identical output, `fused` empty |
| B3 | ≥1 `CollinearSliver` | repair loop entered |
| B4 | Repair: eligible candidate passes validity gate | collapse committed; index-degenerate triangles dropped; `fused` records loser→survivor |
| B5 | Repair pass commits nothing while a sliver remains (no eligible candidate, or all candidates fail the gate) | loud `RoundingCollapse { tri }` (first stuck sliver) |
| B6 | Candidate edge exact length² ≥ TAU_MODEL² | candidate ineligible (real-scale geometry is never fused) |
| B7 | Remapped triangle would go exact-nonpositive | candidate rejected (inner branch of the validity gate) |
| B8 | Needle remains after successful repair (its candidates were ineligible/rejected but no sliver remains) | dropped (benign weld, same as B2) |

## 5. Invariants

- **I1 (exact coverage, unchanged):** the step-5 exact coverage identity is
  certified on the FULL pre-repair overlay — the exact 2D Boolean is
  validated before any fusion. Authoritative for input validation.
- **I2 (exact positivity):** every emitted triangle has strictly positive
  exact area over the post-fusion `exact_verts`.
- **I3 (conformality):** every undirected edge of the emitted triangulation
  bounds ≤ 2 triangles.
- **I3' (fused input-edge tiling):** for every input edge of A and B, the
  emitted triangulation exactly tiles the edge's FUSED image: substitute
  every fused vertex by its survivor in the edge's on-edge vertex chain;
  the resulting chain of emitted triangle edges covers the (possibly
  fused-endpoint) segment with no gaps. (This restates the refuted spec's
  exact-tiling oracle: exact tiling of the ORIGINAL chains and I4 are
  jointly unsatisfiable when two input chains share one rounded image —
  demanding both is demanding f64-degenerate triangles. Edges with no fused
  on-edge vertices keep the original exact tiling.)
- **I4 (f64 emittability):** every emitted triangle is strictly CCW-positive
  in the rounded f64 coordinates. Now unconditional — the gate never emits
  a rounded-degenerate triangle and never silently drops one outside the
  documented B2/B8 needle-weld path.
- **I5 (class absorption, documented deviation):** dropped index-degenerate
  triangles absorb their exact area into the emitted complex; fusion moves
  each fused vertex by < TAU_MODEL. The per-class exact identity
  `area(XOnly) + area(Overlap) = area(X)` holds within
  Σ(absorbed areas) + Σ(fusion motion × incident perimeter) — for the
  corpus fixtures (scale ≤ ~160, motions ≤ 1e-13) far below the 1e-12 test
  bound. The step-5 check remains the pre-repair authority.
- **I6 (determinism):** worklist and candidate orders are index-based;
  survivor selection is deterministic; all maps are BTree. Bit-identical
  output for identical input.

## 6. Oracles

- **Unit green target (existing, oracle stack updated per I3'):**
  `c0048_mirrored_rim_slab_repair` in
  `yang-rs/tests/m8_overlay_femto_slab_emission.rs` — un-quarantined. The
  C0048 verbatim pair must classify successfully; assert I2, I3, I3'
  (fused tiling via the published `fused` map), I4, I5 (per-class identity
  within 1e-12; Overlap dominant ≈ 6.6, Only-regions < 1e-9), I6
  (bit-identical re-run). The `c0048_mirrored_rim_slab_stays_loud` wall pin
  is DELETED in the same PR (its documented retirement condition).
- **Unit branch fixtures (new):**
  - B2: a needle-only overlay (two squares sharing a sub-ULP-split edge…
    any existing passing fixture with needles) — byte-identical to the
    pre-change output, `fused` empty. (Zero-regression witness.)
  - B4/B8: synthetic femto-slab pair (scaled-down C0048 pattern or a
    minimal 2-chain strip) — fused emission, `fused` non-empty, all
    oracles.
  - B5/B6: real-scale exactly-f64-collinear sliver (construct via crossing
    segments whose mints are f64-collinear at real spacing — or, if no
    such polygon fixture is constructible, a direct `#[cfg(test)]` unit on
    the repair routine's internals with a hand-built triangle soup) —
    stays loud `RoundingCollapse`.
  - B7: internal unit on the validity gate — a collapse that would flip a
    real neighbor is rejected.
- **Mutation sanity (Adversary):** (a) drop the validity gate (accept all
  candidates) ⇒ a B7-class test must fail; (b) invert survivor preference
  (prefer mints over input verts) ⇒ a fused-tiling/downstream-identity
  test must fail or the B4 fixture's `fused` survivor assertions must
  fail; (c) widen the eligibility ceiling to MIN_FEATURE_SIZE ⇒ a
  micro-scale guard test must fail (KV15b R0091 lesson).
- **Corpus:** F0067 and C0048 leave `overlay-failed RoundingCollapse`
  (UNSUPPORTED(coplanar)) — expected landing spots per §0 are their next
  honest walls (C0048: kernel-v2 rim-override coincide refusal; F0067:
  Stage-0 build-mesh ring), each becoming a named follow-up. Any better
  landing (further downstream / CORRECT) is acceptable; regressing any
  OTHER case is not.
- **Full assay zero-lost gate** vs baseline 238C/0W/50E/7U/0T (HEAD
  7f205d4c results.json).

## 7. Failure modes

- B5: loud `RoundingCollapse` — unchanged error type, same probe hook
  (`probe_sliver`) before returning.
- Internal invariant breaks (e.g. remap bookkeeping) ⇒
  `TriangulationFailed`, loud.
- No silent drops beyond the documented B2/B8 needle-weld path; no
  tolerance-based acceptance — the repair trigger is exact f64 degeneracy,
  and TAU_MODEL bounds only what MAY be fused, never what must be accepted.

## 8. Research basis

- [#24] Yang et al. 2025 §4.5.5 — coplanar preprocessing emits ONE shared
  trimmed-surface triangulation with identical meshes for both models;
  emission must be f64-representable for the downstream exact mesh boolean.
  Fusing same-rounded-image boundary chains is the §4.5.5 identical-mesh
  requirement applied at f64 resolution.
- [#51] Hoppe 1996 (Progressive Meshes) — edge collapse as the standard
  validity-gated local mesh simplification operator; our validity gate
  (no surviving triangle may invert) is the classical link/fold condition
  in exact arithmetic.
- [#52] Hobby 1999 (snap rounding) — rounding an exact arrangement to a
  representable grid by merging sub-pixel structure; this repair is a
  cluster-limited snap-round where the "pixels" are f64-degenerate
  complexes and everything representable is untouched.
- In-repo precedents: KV15b mint-site sub-resolution collapse at boolean
  emission (`specs/kv15b_mint_site_subresolution_collapse.md` — TAU_MODEL
  ceiling, min-index survivor keeps own bits); Stage-0 increment-4
  sub-floor shared-mint collapse + M-B emission identification
  (`specs/m8_holed_disc_coplanar_overlay` §8); the P10 abort record
  (`specs/m8_overlay_femto_slab_emission.md` §8) proving fixed-vertex-set
  local surgery cannot repair these structures.
- A14.2 (feature size floor) — nothing below MIN_FEATURE_SIZE is a
  feature; fusion ceiling TAU_MODEL is an order below that. A14.3 — the
  constant comes from `cad-primitives`, no ad-hoc epsilon.

### 8a. Analytical vs. approximate method justification (FIP §3.2.7a)

The exact 2D Boolean (arrangement, classification, coverage certificates)
is unchanged and remains fully exact. Fusion happens strictly at the
f64-emission boundary, below representable resolution — it approximates
nothing an f64 consumer could ever observe as geometry; it removes
structure that HAS no f64 image. No surface-surface intersection method is
affected.
