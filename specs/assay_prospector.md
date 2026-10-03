# Assay prospector — searching for the corpus's next failing case

Status: **APPROVED 2026-09-27** (the user: "approved, proceed"). Increment
P0 landed the same day (commit c23778b3: the generative runners are loud).
P1 onward below.

Plan of record for growing the kernel-v2 assay corpus beyond the cases it
already scores 301 C / 0 W / 7 E on. The corpus is a SAMPLE, not a spec
(memory `kernel_v2_m8_crossing_corpus_neutral`): a perfect score on 312
cases says the kernel is right on 312 cases. This document is about finding
case 313.

---

## 1. Goal

A seeded, resumable search that

1. produces candidate documents the kernel has never seen, in the SAME
   `.waffle` + `.meta.json` shape the corpus uses;
2. judges each candidate with the SAME categorizer the corpus score uses
   (`SUPPORTED_CORRECT / SUPPORTED_WRONG / UNSUPPORTED(r) / EXPECTED_ERROR /
   ERROR / TIMEOUT`), plus reference-free metamorphic checks;
3. minimizes every non-CORRECT candidate to the smallest document with the
   same failure signature, de-duplicates by signature, and
4. promotes one representative per signature into the corpus as a new `P`
   series case whose meta pins the ORACLE's expectations, never the
   kernel's output.

The canonical score stays clean: promotion is curated, and the prospector's
own findings live in its own report until promoted.

## 2. What already exists, and what it was missing

| Piece | Where | State before this spec |
|---|---|---|
| Corpus + categorized runner | `app/tests/cases/assay`, `tests/assay_kv2.rs` | the oracle of record; verdict logic lives inside the test binary |
| Random generator | `assay::gen` (`assay_gen --seed --count`) | R-series: 3 profiles × {extrude, revolve}, 2–3 ops; last regenerated before patterns, symmetric/through-all, holed profiles, region extrudes, UnionAll, multi-target combines, pipe, sweep, mirror existed |
| Property runners | `tests/assay_generative*.rs`, `assay_*determinism.rs` | discarded panics / non-manifold / empty results as "known limitations"; boolean oracles advisory — **fixed in P0** |
| Sidecar fuzz | `yang-rs/tests/fuzz_boxes.rs`, `fuzz_curved.rs` | correct-or-loud against the Cherchi C++ reference; the curved one has never completed in this container (sidecar subprocesses zombie); the sidecar is not currently built |
| Reference-free oracles | `assay::exact_membership` (lattice volume from the document's closed-form solids), `volume_oracle_doc` (composition), `oracle::check_mesh_euler_characteristic_with_shells`, mesh checks | all in-line in the categorizer; derived from the `.waffle` itself (`ExactChain::from_waffle`) |

Three facts shape the design:

- **`ExactChain::from_waffle` turns any document into its own oracle.** No
  meta authoring is needed for the volume verdict; the document is the
  spec. Coverage is Sketch / Extrude (Blind, ThroughAll, Symmetric) /
  Revolve / BooleanCombine; anything else is `NotCovered`, never a verdict.
- **`ModelBuilder::save()` turns any built chain into a document.** So a
  candidate can be BUILT through the harness API (which knows every
  operation the engine has) and then judged as a document.
- **Uniform random sampling cannot reach the conditions that failed
  last.** Every tail conversion of the last two months was codimension-one
  geometry (R0038: a 2.81° grazing crossing; C0058: a tangent point; R0007:
  a cut at scale 1.2e-4; the cone parabola: a plane exactly along a
  generator — memory `measure_zero_capability_fuzz_invisible`). A search
  that only samples generically will confirm the score, not extend it.

## 3. Architecture

```
                 ┌──────────────┐   ┌──────────────┐   ┌──────────────┐
   seed ───────▶ │ generator v3 │   │   mutator    │   │  harvester   │
                 │ (recipes →   │   │ (document    │   │ (user docs → │
                 │  ModelBuilder│   │  knobs, one  │   │  per-feature │
                 │  → .waffle)  │   │  at a time)  │   │  prefixes)   │
                 └──────┬───────┘   └──────┬───────┘   └──────┬───────┘
                        └──────────┬───────┴──────────────────┘
                                   ▼
                        candidates/<id>.{waffle,meta.json,lineage.json}
                                   │
                                   ▼
                   ┌─────────────────────────────────────┐
                   │ verdict: SUBPROCESS per candidate    │
                   │  (current_exe + PROSPECT_CANDIDATE)  │
                   │  = assay::categorize::categorize()   │
                   │  + metamorphic checks, CPU-budgeted  │
                   └───────────────────┬─────────────────┘
                                       ▼
                     report.json  (every candidate, every verdict)
                                       │  non-CORRECT
                                       ▼
                   ┌─────────────────────────────────────┐
                   │ minimizer: shrink the RECIPE (drop   │
                   │ ops, un-snap, simplify profiles)     │
                   │ while the SIGNATURE is unchanged     │
                   └───────────────────┬─────────────────┘
                                       ▼
                   dedupe by signature → findings/<sig>/P-candidate
                                       │  curated
                                       ▼
                   promote: app/tests/cases/assay/P00NN.* + manifest
                            + the category pin in assay_kv2.rs
```

### 3.1 The categorizer moves into the library (P1)

`tests/assay_kv2.rs::replay_case` (load → NotSupported boundary → engine
errors → auto-union failures → tessellate last → mesh checks → exact volume
→ composition → Euler-with-shells → bbox → solid count) becomes
`test_harness::assay::categorize::categorize(waffle_json, &meta) ->
CaseOutcome`, with `Category` and `UnsupportedReason` alongside. The test
keeps its timeout / subprocess / parallel / results.json machinery and
calls the library. **Gate: the full categorized corpus is verdict-identical
before and after** (the committed `results.json` diff is empty apart from
timings).

One addition the prospector needs: `OracleExpectations.derived_meta`
(serde default `false`): the meta was derived from the document and carries
no authored expectation. A candidate has no adjudicated χ (a lattice does
not give genus); with the flag set the categorizer requires χ EVEN and
watertight instead of comparing with `euler_target`, and skips the
op-derived `minimum_triangle_count` floor (an op list read off a document
says nothing about how many triangles survive a chain of cuts — it flagged
a correct 8-triangle result as WRONG on the first run). Legacy metas are
unaffected.

### 3.2 Candidates are documents with a lineage

```
candidates/<id>.waffle          the document (ModelBuilder::save output)
candidates/<id>.meta.json       AssayMeta: operations derived FROM the document
                                (kind, profile_type, is_cut, plane), scale,
                                derived_meta = true, expect_watertight,
                                max_bbox_extent from the recipe's envelope,
                                expected_solid_count = None
candidates/<id>.lineage.json    Generated{seed, index, recipe} |
                                Mutated{parent, knob, delta} |
                                Harvested{source_path, feature_prefix}
```

Ids: `X<seed-hex8>-<index>` while candidates; `P00NN` once promoted.

### 3.3 Verdicts run in a subprocess

The prospector spawns `current_exe` with `PROSPECT_CANDIDATE=<path>` and
reads one outcome line, exactly the corpus runner's `replay_case_subprocess`
shape: CPU-time budget (default 600 s, `PROSPECT_BUDGET_SECS`), wall cap at
4×, panics and hangs isolated. A TIMEOUT is re-run once at 4× budget before
it is called a finding (a hang IS a finding; a slow case is not).

### 3.4 Signatures

A finding's signature is what the minimizer preserves and the deduper keys
on:

| Category | Signature |
|---|---|
| ERROR | the typed error after the feature name: kernel error variant + the yang stage/reason text with numbers stripped (`TessellationFailed{planar triangle collapsed at render precision}`, `Stage4 LocalRefinementRequired`) |
| SUPPORTED_WRONG | the set of failing oracle names (`exact_volume`, `mesh_euler_characteristic`, `watertight_mesh`, …) |
| UNSUPPORTED(r) | `r` — recorded, not promoted (a declared boundary is roadmap, not a finding) unless the boundary text names something CLAUDE.md says has landed |
| TIMEOUT | `timeout` + the last stage reached if the trace says |
| METAMORPHIC | the identity that broke (`rigid`, `scale`, `union_commute`, `boss_order`) |
| PANIC | the panic message with numbers stripped |

## 4. Generator v3 (`assay::prospect::gen3`)

Deterministic splitmix64 (the `fuzz_boxes.rs` pattern; no `rand`, no time).
A candidate is a **recipe** — a serializable list of steps — executed
through `ModelBuilder`; the recipe is what the minimizer edits.

**Steps** (each names the prior body it acts on; step 0 is always a boss):

| Step | Vocabulary |
|---|---|
| profile | convex n-gon (3–8), non-convex (perturbed), star (needle ratio 0.1–0.9), arc-polygon (true `Arc` entities), circle, gear (`helpers::gear_profile`), rectangle with 1–3 holes |
| plane | axis-aligned, tilted (θ, φ), a face of the prior body (`extrude_on_face`) |
| operation | extrude blind boss/cut, symmetric, through-all cut, directed; revolve boss/cut (partial 10°–350°, axis in-plane or offset); boolean union/subtract/intersect against a named earlier body (explicit refs, so disjoint and multi-body chains occur) |
| size | scale log-uniform 1e-4 … 1e3 m per candidate; profile radius 0.3–1.5× the prior envelope; depth 0.5–2× |
| length | 2–12 steps; the 20-op stacks are the corpus's heaviest cases and a long chain is the cheapest way to compound a small defect |

**Degeneracy snapping** — after a generic step is drawn, with probability
0.35 ONE quantity is snapped to an exact condition or to ε off it, where ε
∈ {0, 1e-12, 1e-9, TAU_MODEL 1e-7, MIN_FEATURE 1e-6, 1e-3}·scale, signed:

- depth = distance to a prior cap plane (coplanar cap / flush cut);
- plane origin on a prior face plane (coplanar sketch);
- profile radius = a prior circle's radius (tangent / coincident cylinders);
- a vertex on a prior AABB edge or corner; an edge collinear with a prior edge;
- plane rotation ∈ {0°, 90°, 0.5°, 2.8°, 45°} + ε (grazing);
- cutter depth = target thickness (through vs blind, R0007's class);
- revolve angle ∈ {180°, 360° − ε} (the seam).

v1 snaps against the prior body's AABB faces, edges and corners (exact for
axis-aligned boxes; a useful proxy otherwise); later increments read the
actual face planes from `face_signatures`.

## 5. Mutator (`assay::prospect::mutate`)

Operates on a DOCUMENT, so it works on the 301 CORRECT corpus cases, on the
prospector's own CORRECT candidates, and on real user documents (the bike
frame, the planetary gearbox, the Eiffel tower). One knob per mutant, so
every mutant has a known-good sibling one edit away:

| Knob | Delta |
|---|---|
| sketch point | move one point by ε in u or v |
| plane origin | ε along the normal (coplanar → ε-off) or in-plane |
| plane normal | rotate by 0.5° / 2.8° / ε about a random in-plane axis (then re-normalize; `plane_x_axis` rotated with it when present) |
| depth / angle | ± ε, ×(1 ± ε), = a sibling's depth |
| uniform scale | ×1e-3, ×1e3 (every coordinate, origin, depth, radius) |
| feature order | swap two boss features that do not reference each other |
| feature drop | delete one feature and everything that references it |
| flag | `symmetric`, `cut` direction, `merge` |

The mutant's meta is re-derived from the document (§3.2), never copied.

## 6. Oracles

Everything the categorizer already runs (mesh checks, exact-membership
volume, composition, χ-with-shells, bbox, solid count), with
`derived_meta` relaxing χ to "even". On top, **metamorphic
identities**, which need no reference at all — two kernel runs must agree
with each other or with arithmetic:

| Identity | Transform of the document | Must agree on |
|---|---|---|
| rigid motion | rotate every plane origin/normal/x-axis and revolve axis by one fixed R, translate by t | category, volume (band = oracle_tol), χ, shell count |
| uniform scale | ×s on every length | category, volume ×s³, χ |
| union commutes | swap `body_a`/`body_b` of a Union combine | category, volume, χ |
| boss order | swap two independent bosses | category, volume, χ |
| inclusion–exclusion | V(A)+V(B) = V(A∪B)+V(A∩B), built as four documents from the same two operands | volumes within band |
| de Morgan | (A−B)−C = A−(B∪C) | volumes, χ |

A disagreement where the two KERNEL runs agree on volume, χ and body
count but an in-line oracle flagged one of them is classed `ORACLE`, not
`METAMORPHIC`: the oracle invalidates in both directions (memory
`feedback_reference_oracle_invalidates_in_both_directions`), and the
first run found exactly that on the rotated revolve trackers. A
disagreement is otherwise a finding of class METAMORPHIC even when each
run is individually CORRECT by the lattice — which is the point: the lattice band
is ~0.5 %, a metamorphic pair is compared at tessellation precision.

The Cherchi sidecar parity joins as a seventh oracle when the sidecar is
built (`scripts/build_sidecars.sh`) and its zombie problem is confirmed
fixed; no increment below waits on it.

## 7. Minimizer

On the recipe (generated candidates) or the document (mutants, harvests):

1. drop the LAST step; if the signature holds, keep dropping;
2. drop any single middle step whose removal keeps the signature (bosses a
   later boolean references are re-pointed to the previous body);
3. un-snap: replace each snapped quantity with its generic draw; keep the
   snap only if the signature needs it (this is what tells a
   near-degeneracy finding from a generic one);
4. simplify profiles: n-gon → 4-gon, star → convex hull, arc-polygon → its
   chord polygon, holes removed one at a time;
5. round every coordinate to the coarsest decimal that preserves the
   signature.

Every step is a verdict subprocess. Budget: at most 60 verdicts per finding
(`PROSPECT_MIN_STEPS`).

## 8. Promotion

Manual, per signature, by the session that reviews the report:

1. cross-check the candidate's own oracles against each other (the C0035
   lesson: a self-contradictory meta is an authoring error, not a kernel
   finding), its smallest authored feature against `MIN_FEATURE_SIZE` (an
   input below the floor is the A14.2 wall by contract — seed 1 index 5),
   and its minimum against the un-rounded lineage (rounding can mint a
   contact that keeps the signature);
2. for SUPPORTED_WRONG, confirm with an independent reading (the lattice at
   two rungs, a hand calculation, or the sidecar) — the oracle invalidates
   in both directions;
3. write `P00NN.waffle` + `P00NN.meta.json` (`prospect::promote::write_case`)
   with the meta's expectations from the ORACLE: for a SUPPORTED_WRONG
   finding, `expected_volume` from the lattice when converged,
   `euler_target` adjudicated by hand and `derived_meta` cleared,
   `expected_solid_count` when the document says so. For an ERROR-class
   finding the categorizer returns before any oracle runs, so the pin IS
   the expectation: the meta keeps `derived_meta: true` and says so in
   its description; the χ/volume adjudication happens when the kernel
   converts the case (the conversion PR adjudicates and clears the flag,
   the same way an UNSUPPORTED → CORRECT conversion moves a pin);
4. add the manifest entry (`featured: true`, description = signature +
   lineage) and the category pin in `assay_kv2.rs`
   (`smoke_corpus_boundary_categories` for a wall; the pinned-adjudications
   block for a WRONG) so the score records it honestly: a P-case that is
   ERROR counts as E until the kernel converts it, and the conversion moves
   the pin — the same rule as every other series.

**Findings of the first P2 run (seed 1, 40 candidates), by signature —
none minimized or adjudicated yet (P3):**

| × | signature (ERROR unless noted) | smallest example |
|---|---|---|
| 4 | `boolean_subtract TessellationFailed "ring rejected by CDT (degenerate/self-intersecting)"` | `circle:boss star8(0.42):cut` — 2 ops |
| 2 | `boolean_union BooleanFailed "reassembled output would be non-2-manifold"` (auto-union) | `convex3:boss convex7:solo convex5:rev gear10:sym circle:boss circle:boss` |
| 1 | `boolean_subtract … "reassembled output would be non-2-manifold"` | `circle:boss nonconvex5:rev convex4:boss star7(0.69):−` |
| 1 | `boolean_subtract TessellationFailed "torus patch UV-CDT failed (self-intersecting projection / seam-crossing patch)"` | `nonconvex9:boss circle:rev-cut circle:cut convex7:boss convex3:− circle:cut` |
| 1 | `boolean_union … "yang-rs rejected the converted input B-Rep: face # is degenerate (zero-area / collinear)"` | `circle:boss convex5:solo gear17:boss nonconvex9:boss convex6:boss convex3:cut convex8:boss` |
| 1 | `boolean_union SelfIntersectingBooleanOutput { penetrations: 4 }` | `circle:boss circle:boss nonconvex6:boss convex4:∪` |
| 1 | `boolean_subtract … "cone periodic strip (2 encircling rims) not yet supported (KV14 Slice E …)"` — a documented deferred sub-slice, loud | `convex3:boss star3(0.49):boss convex3:rev star5(0.13):cut …` |
| 1 | `boolean_union … "yang-rs rejected the converted input B-Rep"` (second variant) | `star7(0.88):boss circle:solo star5(0.70):boss circle:boss gear10:boss gear15:∪` |
| 1 | UNSUPPORTED(coplanar-boolean): `Auto-union failed: … coplanar input face pair` | `nonconvex5:sym gear25:sym gear17:sym convex3:∪` |

Minimized forms (P3, `prospect_minimize`, ≤40 verdicts each; the
minimizer keeps a reduction only if the signature is byte-identical):

| signature | minimal recipe | promoted |
|---|---|---|
| ring rejected by CDT | `circle:boss star8(0.42):cut` (2 ops, already minimal) | **P0002** — CONVERTED 2026-09-27 (§4.3.3 Case-IV verdict on the vertex's own root, spec `yang_433_case_iv_corner_phantom.md` §9; meta adjudicated χ 2, analytic volume) |
| torus patch UV-CDT failed | `nonconvex9:boss circle:rev-cut` (6 → 2) | **P0003** |
| planar triangle collapsed at render precision (the P0 finding) | `octagon:boss star4(r_in 2, r_out 22):boss ∪` (3 ops) | **P0001** — CONVERTED 2026-09-27 (§4.5.5 edge-in-plane, spec `yang_455_edge_in_plane_conformity.md`; meta adjudicated χ 2, analytic volume) |
| subtract: reassembled non-2-manifold | `convex4:boss convex4:rev convex4:boss convex4:cut` (4 → 4, profiles simplified) | **P0004** (2026-09-28; site `s6-planar-loop-nonplanar`: face 13 vertex 127 off its plane by 1.2e-1 at scale 3.5 — a relocation left its planar face; signature identical at ×1e-3 and ×1e3) — **CONVERTED 2026-09-28 (night)**: a {cone, plane, plane} corner whose plane∩plane endpoint map counted zero toward the triple block (spec `yang_stage4_conic_triple_junction.md` top blockquote). The un-minimized lineage now STOPs one crate later (kernel-v2 `TessellationFailed` "patch triangulation folded — KV9-F2", face 54): a NEW signature to re-minimize, not this fix's customer |
| union (auto): reassembled non-2-manifold | `convex4:boss convex4:rev gear10:sym circle:boss` (6 → 4) | **P0005** (2026-09-28; site `s4-halfedge-pairing`: a doubled directed edge fwd 2 / rev 0 between A's cone face and B's plane — a Stage-4 membrane; ×1e-3 identical, ×1e3 ⇒ `LocalRefinementRequired`) — **CONVERTED 2026-09-28 (night)**: the §4.4.1(a) split wound a needle half GEOMETRICALLY and inverted it (A's cone chart had two rim mints 3e-3 apart, each with a seeded generator — a 3e-3 × 424 sliver quad the flank plane crosses 2.67e-7 apart); halves now inherit the parent's winding combinatorially (spec `yang_n2_stage4_cdt_mesh_updating.md` §5c.15). Meta adjudicated by the Cherchi sidecar reference: one shell, genus 1 (χ 0), two live bodies. The un-minimized index-19 lineage STOPped at a NEW site — re-minimized 2026-09-30 as **P0008 / P0009** (next rows) |
| cone periodic strip (KV14 Slice E) | `convex4:boss convex4:rev convex4:cut convex4:cut` (8 → 4) | — (documented sub-slice) |
| input face degenerate (zero-area / collinear) | `convex4:boss gear17:boss nonconvex9:boss convex4:boss` (7 → 4) | **NOT a finding** (2026-09-28): the drawn gear (module 2.26e-5 m) authors 2.6e-7 m profile segments, below `MIN_FEATURE_SIZE` — the A14.2 INPUT feature-floor wall by contract. The un-rounded 7-op candidate is SUPPORTED_CORRECT at ×100; the 4-op minimum at ×10…×1000 gives `NonManifoldVertex`, and THAT minimized to two square bosses whose 1-sig-digit coordinates put one box's lateral edge exactly in the other's cap (a 1-D contact — a legitimately non-manifold union, the 0-D-contact walls' class). Generator fixed: a gear below the floor is drawn as a circle (`gear_draw_respects_the_input_feature_floor`). |
| Stage-# chart polygon of face # cr… (input rejected) | `convex4:boss convex4:boss circle:boss gear10:boss convex4:boss` (6 → 5) | **P0006** (2026-09-28; the accumulated body's face 11 after the gear union crosses itself 8× in its Stage-1 chart, rim demand `None`; identical at ×1e-3 and ×1e3). **CONVERTED 2026-09-29** — the periodic strip's seam meridian was chosen from the window vertices alone and the lower encircling loop crossed it three times; the seam is now validated (one crossing per encircling loop, no window straddle, a simple ribbon) with a widest-first rescue. The un-minimized six-op lineage converts too (its face 13 crossed once per loop but the seam closure chord ran through a serpentine wall — the ribbon-simplicity clause). |
| SelfIntersectingBooleanOutput (penetrations 4) | `circle:boss circle:boss nonconvex6:boss convex4:∪` (4 → 4) | **P0007** (2026-09-28; kernel-v2's output self-intersection gate, faces 47 × 69; identical at ×1e3, ×1e-3 hits the feature-floor wall). **CONVERTED 2026-09-29** — the §4.4.2 carried-edge restoration read INPUT circles only, so the re-entered flank × cylinder ELLIPSE boundaries left as chords; it now admits `Ellipse` candidates. The harness learned the explicit Union combine (`BooleanCombine`) as a body MERGE in the exact-membership reader and a no-operand step in the topology instrument. The un-minimized index-16 lineage converts with it. |
| wrong[watertight_mesh] (SILENT: 6 non-manifold render edges, 4 triangles each) | `convex4:boss convex5:rev gear10:sym` (seed 1 index 19 re-minimized 2026-09-30, 6 → 3; rounding-dependent — other roundings read CORRECT) | **P0008** (2026-09-30): kernel-v2's boundary-only developable patch CDT clipped ears over three consecutive samples of a cone × gear-flank ELLIPSE arc and used chords between two of them as interior diagonals — three points of one plane span a triangle IN the flank's plane, coincident with the flank's own ear. **CONVERTED 2026-09-30**: the P0008 refinement criterion (planar-neighbor node tags; an interior chart edge whose ends share a planar neighbor splits at its on-surface midpoint). ×1e-3 identical, ×1e3 ⇒ `LocalRefinementRequired`. Adjudicated from the kernel readout under the in-line exact-membership oracle (2 bodies, main χ 2, vol 8.5486e8). |
| union (auto): reassembled non-2-manifold — `i6-edge-overuse` fwd=2 rev=1 on INPUT A | `convex4:boss convex5:rev gear10:sym convex4:boss` (seed 1 index 19 re-minimized 2026-09-30, 6 → 4; the P0008 prefix + a box) | **P0009** (2026-09-30): the loud twin — yang Stage 1's boundary-only cone chart CDT emits the same planar-neighbor ears/chords and Stage 0's input conformality check reports the doubled compact triangle one boolean later; same site on the un-minimized lineage. **CONVERTED 2026-09-30**: `stage1_tessellate/planar_neighbor_chords.rs` (same rule on the holed-lateral CDT's output, both kinds). Adjudicated the same way (2 bodies, main χ 2, vol 1.97727e9). |

All eight seed-1 signatures are adjudicated (2026-09-28): six promoted
(P0001–P0007 less the retired one), one a documented sub-slice, one an
authored sub-floor gear. The adjudication method, per finding: re-judge on
the current kernel (`PROSPECT_CANDIDATE=<stem> prospect_judge`), measure the
smallest authored segment against `MIN_FEATURE_SIZE`, judge the recipe at
×1e-3 / ×1e3 (a rescaled `.recipe.json` through `PROSPECT_RECIPE`), and
for a reassembly STOP run `NONMANIFOLD_SITE_PROBE=1` to name the site in
the pin. **Minimizer hazard (the retired finding's lesson):** rounding to
one significant digit can MINT a coincidence (two bosses meeting along an
edge) that keeps a number-stripped signature while changing the cause —
cross-check a minimum against its un-rounded lineage before promoting
(step 1 below).

**Mutation findings (P5, seed 1, 120 mutants; parent is SUPPORTED_CORRECT):**

| parent + knob | verdict |
|---|---|
| R0085 + depth ×1.001 | ERROR `Stage-4 relocation region around vertex # is invalid: RelocationCrossedCarrierVertex` (97 s) — the R0085 family, one knob from CORRECT |
| R0041 + scale ×1e3 | ERROR `boolean_subtract TessellationFailed "patch triangle collapsed at render precision"` — a scale-invariance break (km-scale); adjudicate against the absolute TAU_MODEL contract |
| R0004 + revolve angle +30.5° | ERROR `boolean_union InvalidBooleanOutput("full-circle edge sense is underivable …")` (R0004 was once AUTHORED-INVALID; re-check the mutant's authoring first) |
| C0043 + depth −0.1·L | ERROR `TessellationFailed "ring rejected by CDT"` — P0002's signature on a second geometry |
| C0064 + revolve angle −0.5° | ERROR `cone periodic strip (2 encircling rims) not yet supported (KV14 Slice E)` — documented sub-slice |
| C0043 + normal +1e-7°, F0009 + origin n+3e-9 | ERROR sub-resolution coplanar wall (`two DISTINCT parallel planes separated by …`, #178 contract) — loud by design; the mutator reaches it on purpose |

Seed 2 (120 mutants, 110 CORRECT) added: C0045 + normal −2.8° ⇒
`boolean_union NonManifoldVertex`; R0100 + revolve angle +115.5° ⇒
`TessellationFailed "patch triangulation folded (inverted triangle) — KV9-F2"`;
R0016 + scale ×1e-3 ⇒ `malformed B-Rep topology: interior junction`;
R0043 + scale ×1e-3 ⇒ `input face degenerate (zero-area / collinear)`;
C0057 + `symmetric` ⇒ `geometric face resolution failed for kept triangle #
(centroid off all face surfaces)`; F0071 + scale ×1e-3 ⇒ the coplanar
NotSupported wall; F0001 / R0051 ⇒ the sub-resolution wall. (Two more
mutator couplings were found and fixed on that seed: a sketch origin moved
along its normal must carry a revolve's `axis_origin`, and a revolve angle
must stay inside (0, 360].)

Seed 3 (120 mutants, 110 CORRECT, no mutator artifact left) added:
**F0004 + one point moved 5e-10** (v) ⇒ `boolean_union InvalidBooleanOutput
("an undirected output edge is not used by exactly two directed edges")`;
**C0050 + scale ×1e3** ⇒ the same InvalidBooleanOutput its ×1e-3 mutant
gives (P6) — a scale-sensitivity in BOTH directions; C0103 + normal +1e-7°
⇒ `VertexOffSurface`; F0060 + origin n+3e-2 ⇒ `TessellationFailed
"keyhole …"`; C0117 + normal −0.5° ⇒ `InvalidBooleanOutput("output face
plane normal disagrees with its outer-loop Newell normal")`; R0050 + origin
n−1.1e-6 ⇒ reassembly non-2-manifold; F0059 + `symmetric` ⇒ Stage-4
relocation region invalid; C0056 + `symmetric` ⇒ AmbiguousCurve; F0009 /
F0010 ⇒ the sub-resolution wall.

Re-creatable with `PROSPECT_SEED=<1|2|3> PROSPECT_COUNT=120 prospect_mutate`
(the mutant documents are under `target/prospect/mutate-<seed>/candidates/`).

**Seed 2 (`prospect_run`, 200 candidates, 991 s at 8 jobs, 2026-10-03):** 172
SUPPORTED_CORRECT, 21 ERROR, 3 SUPPORTED_WRONG, 2 TIMEOUT, 2
UNSUPPORTED(coplanar-boolean) over 17 signatures; `prospect_minimize`
(1572 s) reduced 16 of them to 2–5 steps. Adjudicated and promoted the same
day as **P0010–P0022** (corpus 321 → 334); the full narrative, the per-case
adjudication evidence and the retirements live in
`docs/yang_tail_triage.md` §"2026-10-03". Four lessons for this spec:

1. **Dedupe by error TEXT, not by signature.** The signature keys on the
   boolean VERB too, so the 21 ERROR rows presented as 15 signatures but are
   **eleven** families; four of those fifteen were the same text reached
   through a different verb or the auto-union path.
2. **A SUPPORTED_WRONG signature (`wrong[exact_volume]`) is far too coarse to
   dedupe on.** Seed 2's three WRONG rows share it and are three unrelated
   defects at three different ops. The minimizer only ever shrinks ONE example
   per signature, so the other two had to be localized by hand — by judging
   every chain PREFIX on its own and reading the exact-membership lattice at
   192/256/384/512 cells on two phases, which names the op where kernel and
   oracle part instead of guessing it. Future runs should treat
   `wrong[exact_volume]` as a bucket to be opened, not a finding.
3. **A chain PREFIX is a better minimum than a rounded one when it works.**
   P0010 and P0011 are prefixes-plus-dropped-ops of the generated documents
   with the original coordinates intact, so step 1's minted-contact hazard
   does not apply to them at all.
4. **A build-phase TIMEOUT used to name nothing.** Both seed-2 TIMEOUTs
   (indices 16 and 78) spent the whole budget inside `gen3::build`, so
   `judge_generated` never reached its lineage write and the report line
   carried `steps: 0` with an empty summary. `judge_generated` now writes
   `<id>.lineage.json` BEFORE the build (the build report is appended after),
   so such a candidate is re-playable through `PROSPECT_RECIPE` and
   minimizable like any other. `prospect_derive_meta`
   (`PROSPECT_CANDIDATE=<stem>`) re-derives a hand-edited finding's meta from
   its document, which §3.2 requires and hand-editing breaks.

**Two oracle holes seed 2 exposed** (both harness, neither promoted):

- **An empty document reads SUPPORTED_CORRECT.** A census of the live body
  count of all 172 CORRECT candidates found **4 with ZERO non-empty bodies**
  (indices 53, 84, 111, 199), and every one of them ends in an `∩` whose
  operands are disjoint. 2.3 % of the CORRECT column is "the chain
  annihilated itself". `categorize` should decline or flag a result with no
  solid, never call it correct — and the engine should STOP typed at the
  empty Intersect rather than leave the next op's `GeomRef` to fail.
- **The exact chain's cut auto-reversal is a bbox proxy for the engine's
  B-Rep-vertex decision**, and when its own `tool_alone` reading would flip
  the verdict it emits a note and picks one anyway. On index 159 the engine
  traced `reverse=true` and the chain had chosen `false`; authoring that
  cut's `direction` explicitly (so neither side auto-reverses) left the
  kernel's volume bit-identical and moved the ORACLE by 5 %. It should push
  an `indeterminate` entry in that case, not a volume verdict.

The FIRST promotion candidate exists already: the needle star (4 points,
r_in = 2, r_out = 22) on the X plane unioned with an octagon prism on the Y
plane, `TessellationFailed{FaceId, "planar triangle collapsed at render
precision"}` on `boolean_union` — found by `generative_chain` on its first
loud run (2026-09-27). It is the P1 acceptance case: the prospector must
reproduce, minimize and package it.

## 9. Running

```
# search (release, 8 verdict subprocesses, 600 s CPU per candidate)
PROSPECT_SEED=1 PROSPECT_COUNT=500 PROSPECT_JOBS=8 \
  cargo test -p test-harness --test prospect --release prospect_run -- --ignored --nocapture
# mutate the corpus's CORRECT cases (and any document directory)
PROSPECT_MUTATE=app/tests/cases/assay PROSPECT_COUNT=1000 … prospect_mutate
# report: target/prospect/<seed>/report.json + findings/<signature>/
```

Output lives under `target/prospect/` (never committed). Resumable: a
candidate whose verdict line is already in the report is skipped, so a
killed run continues where it stopped. Never run during a full assay (the
CPU budgets are honest but the box is not infinite).

## 10. Increments

| # | What | Gate |
|---|---|---|
| **P0 ✅ (c23778b3)** | generative runners loud; chain executor sums every body; determinism = outcome equality | four runners green loud; the needle-star finding recorded |
| **P1 ✅ (2026-09-27)** | `assay::categorize` lifted from `assay_kv2.rs`; `derived_meta`; `PROSPECT_CANDIDATE` subprocess entry; meta derivation from a document | **Met**: full corpus 312/312 verdict-identical (299 C + 4 EE + 7 E + 2 TIMEOUT at the 600 s budget under load; R0085 and F0072 re-run alone = SUPPORTED_CORRECT at 600 s / 740 s CPU); the needle-star document categorizes ERROR with the recorded signature (`tests/prospect.rs`) |
| **P2 ✅ (2026-09-27)** | generator v3 (recipes, vocabulary of §4 minus snapping), the search loop, report, resume | **Met** on seed 1 × 40 candidates (three passes; two generator defects fixed on the way — booleans targeting a body a merging boss had consumed, and a ThroughAll placeholder depth read as the scale): 27 SUPPORTED_CORRECT, 12 ERROR in 8 kernel-side signatures, 1 UNSUPPORTED(coplanar-boolean); ≈4 min at 6 jobs. The smallest finding is TWO ops: `circle:boss star8(0.42):cut` ⇒ `boolean_subtract TessellationFailed "ring rejected by CDT (degenerate/self-intersecting)"` (X00000001-00020) |
| **P3 ✅ (2026-09-27)** | minimizer (`prospect::minimize`: truncate, drop, un-snap, simplify profile / op, round) + signature dedupe + `findings/<slug>/` packaging (`prospect_minimize`); promotion (`prospect::promote`, `prospect_promote`) | **Met**: the eight seed-1 signatures minimized in 10–31 verdicts each (8→4, 6→2, 7→4, 6→5, 6→4 steps; two were already 2 ops); **P0001** (needle star ∪ octagon), **P0002** (circle boss, 8-point star cut ⇒ "ring rejected by CDT"), **P0003** (non-convex 9-gon boss, circle revolve-cut ⇒ "torus patch UV-CDT failed") in the corpus, pinned ERROR in `assay_kv2.rs`; corpus 315 cases |
| **P4** | degeneracy snapping | a snapped run's finding rate per candidate exceeds the generic run's (measured, recorded) |
| **P5 ✅ (2026-09-27)** | mutator over the corpus and user documents (`prospect::mutate`, `prospect_mutate`; knobs: one point ε, whole-sketch ε shift, plane origin ε, plane normal 1e-7°…45° (a revolve axis of that sketch rotates with it), Blind depth ±ε / ×(1±ε), revolve angle ±ε / to the full turn, uniform scale ×1e±3, `symmetric` toggle; gear/sprocket `params` lengths scale too) | **Met** on seed 1 × 120 mutants of the 301 CORRECT corpus cases (≈6 min at 6 jobs): 106 CORRECT; seven kernel-side STOPs one knob from a CORRECT parent (table below); the remaining seven rows were mutator defects, all fixed (a gear entity's `params` unscaled ⇒ a ×1e-3 mutant's bbox explodes / a revolve axis lands in its profile; "nothing to mutate" on sketches without `solved_positions`; a rotated normal leaving a revolve axis out of plane) |
| **P6 ✅ (2026-09-27)** | metamorphic identities (`prospect::metamorphic`, `prospect_metamorphic`): the explicit-x-axis reference (`axes`), a seeded rigid motion (`rigid`), a uniform scale ×1e±3 (`scale`), compared on category, volume (×s³), χ and body count through a measuring child mode | **Met** on the first 80 CORRECT corpus cases (≈3 min at 4–6 jobs): `axes` holds on all 80 (the engine honours an explicit `plane_x_axis` equal to its derived one); **kernel** disagreements: 4 × rotation ⇒ `Stage-3 SSI refinement failed … AmbiguousCurve` (C0043, C0056, C0057, C0058 — the M5/KV9 cylinder trackers fail once the cylinders are not axis-aligned), 1 × rotation ⇒ `sphere patch UV-CDT failed (pole-crossing — later slice)` (C0067), 4 × scale ×1e-3 ⇒ absolute-tolerance walls by contract (sub-resolution coplanar C0030/C0034, `ProfileTooFewVertices` C0032) plus one to adjudicate (C0050 ⇒ `InvalidBooleanOutput("an undirected output edge is not used by exactly two directed edges")` at mm scale); **oracle-only** disagreements: 6 × rotation on the KV6 revolve trackers (C0059, C0061, C0062, C0064, C0066, C0069) — the kernel volumes agree to 2e-5 but the exact-membership lattice's own reading moves by 16–70 % (its frame is the first sketch's; a rotated document converges worse) — a harness defect, reported as `ORACLE`, not a kernel finding. The first pass also caught the driver scaling circle volumes by 1e3 instead of 1e9 (solved-profile circles unscaled — fixed) |
| **P7** | harvester for user documents | the bike frame, gearbox and tower replay per-feature as candidates |

Each increment: tests, `cargo clippy --all-targets -p test-harness`, fmt,
a note here, commit, push.

## 11. What this is not

- Not a replacement for the corpus score. The prospector's numbers are a
  search log; the score is the 312 (+P) cases in `results.json`.
- Not a tolerance-tuning tool. A finding is minimized and promoted; it is
  fixed structurally (CLAUDE.md "Structural fixes first"), or it stays a
  loud wall with a pin.
- Not fillet / chamfer / shell (deferred indefinitely): the vocabulary
  excludes them.
