# SPEC — M8 Stage-0 overlay mesh-updating: the MULTI-CLASS cavity arm (amendment 12)

**Status: inc-0 CENSUS COMPLETE (2026-07-30, see §7). Baseline 259C/0W/51E/0T
(post 1a9cee36/1f576621). Next: inc-1.**

Named target of the R0099 producing-op probe (`docs/yang_tail_triage.md`
§"R0099 producing-op probe COMPLETE (2026-07-30)", commit 1f576621). This is
the next amendment in the fold-gate series hosted at
`specs/n2_stage4_junction_cluster_merge.md` §3 (amendments 2–11, M8
increments 7–14) and is a **structural mesh-updating fix**, not a band
(memory `feedback_stop_band_tuning_build_mesh_updating`; the amendment-2
revert stays the loud terminal fallback for everything this arm rejects).

## 0. The failure, measured (R0099 is the witness)

R0099 op 3 (revolve cut, wedge profile) contacts the tube **only coplanarly**:
its θ=0/θ=180 profile rectangles lie exactly in the bottom-cap plane (Stage-0
cross-pairs, gaps 0 and 1.8e-15). Zero transversal intersections ⇒
`has_conic = false` ⇒ **Stage 4 never runs on this op** — no rim-snap, no
relocation, no provenance. Whatever Stage 0 emits is final.

Stage-0's overlay mints exact on-rim-circle vertices where the wedge
rectangles cross the cap's rim chords (u-extent ±3.1205 vs r=3.1251 — the
crossings hug the rim). Moving those mints chord→circle folds local overlay
slivers, and the repair ladder fails end to end:

- amendment-4 flips: constraint-blocked (`class-boundary` /
  `domain-boundary` / `replacements invalid`);
- amendment-5 per-vertex cavity relocation: **`multi-class cavity with
  constraint-blocked fan`** (verts 4/9/116/153/182…, `reloc.rs:428`) and one
  `cavity polygon not simple` (vert 120);
- amendment-6 joint relocation: never fires for the multi-class verts (the
  trigger requires a NonSimple sighting, `stage0/mod.rs:1218`);
- amendment-2 fallback REVERTS the mints to chord lifts —
  `[fold-revert] vert=9 → chord (-2.24898, -7.43299, 8.03287)` **is** the
  `VertexOffSurface(18)` point digit-for-digit.

Three reverted mints survive into face 18's boundary at 6.1e-2 / 8.7e-2 /
9.1e-2 inside the outer cylinder (chord depths of the 13-gon rim; 2.8% of
r=3.125). Until `kernel-v2/strict-validation` (5b891ec2) this was SILENTLY
WRONG; now it is a loud ERROR. The revert is working as designed — the
missing piece is the repair it falls back from.

**Why the reject is structural, not incidental:** a rim-crossing mint sits
**on the intersection curve** — that is what a rim crossing is (amendment 7's
own founding observation, M8 increment 10). So its cavity spans ≥2 region
classes *by construction*, and every constraint-blocked fold at such a mint
lands in the one arm the per-vertex machinery refuses. The reject class is
exactly the mint class Stage-0 exists to create.

## 1. The paper requirement (this is the spec)

Yang §4.4.1 Mesh updating (`refs/text/yang2025_hybrid_boolean.txt:546-566`,
Fig 11): the intersection polyline is inserted as **constrained edges that
are the boundary of the trimmed triangle meshes**; a moved/inserted point q
splits its constrained edge, a too-close endpoint p **merges with q** —
i.e. the boundary moves WITH the point — and CDT re-triangulates each
trimmed side against that polyline, "contains no flipping triangles".

Yang §4.5.5 (`…:718-758`, Fig 16): the coplanar-overlap boundaries "are
regarded as intersection curves between the two models", and the common
part and the other parts "share identical sampling points on their
boundaries".

Composed, the paper's contract for a moved vertex ON the overlay's
class-boundary polyline is: **carve per class along the polyline, keep the
polyline (moved with the mint) as the shared boundary of both sides, and
re-triangulate each side independently against it.** That is Fig 11 at the
overlay level — the #169 Phase-A principle one stage earlier
(`stage4_update.rs:291-334`: drive both sides from ONE shared seam identity;
re-triangulate interiors only; conformality across the seam is automatic).

## 2. Why the current machinery cannot do this (reject inventory)

All in `crates/yang-rs/src/stage0/`:

| Arm | Site | Behavior at an on-curve mint |
|---|---|---|
| amendment-4 flip | `mod.rs:1096` | class-boundary edges unflippable (correct — the curve is immovable) |
| amendment-5 fan | `reloc.rs:413` | fine when no growth deferral (multi-class fan preserves all spokes) |
| amendment-5 ear-clip | `reloc.rs:423-429` | **rejects**: `interior vertex with constraint-blocked fan` (closed link), `multi-class cavity with constraint-blocked fan` (open link) — the single-class, v-excluded polygon form cannot preserve v's constraint spokes |
| amendment-6 trigger | `mod.rs:1218` | fires only on `saw_nonsimple && seeds ≥ 2` — pure multi-class rejects never reach the joint form; singleton NonSimple (F0088 wall) neither |
| amendment-7 region partition | `reloc.rs:566-622` | per-class sub-regions relocated independently; for an on-curve mint a class wedge can be a single triangle (`region too small`, `reloc.rs:649`) or its cycle blocked by surrounding constraints (`crossing edges ungrowable`) |
| amendment-2 revert | `mod.rs:1244-1266` | the loud fallback — today's leak site |
| n-ary reduced gate | `nary.rs:525-640` | **no relocation at all** (slice-g B8 deferral, `specs/m8_nary_tessellated_faces.md` B8): unflippable folds revert directly; probe `nary-fold-revert` (1f576621) makes it observable |

The ear-clip's single-class guard is not a bug — the polygon form it builds
(`[v, w₀…w_k]`, v's spokes removed, one class for all ears) genuinely cannot
express a cavity whose interior contains constraint spokes. The multi-class
case needs a *different polygon decomposition*, not a relaxed guard.

## 3. The design — amendment 12: per-class WEDGE decomposition

### 3a. Wedge decomposition of the deferred cavity

Inside `relocate_minted_vertex`, replacing both constraint-blocked-fan
rejects (`reloc.rs:423-429`):

The amendment-5 growth loop (`reloc.rs:352-407`) runs **unchanged** — it
already defers at class boundaries, domain boundaries, and pinches, so the
grown link's class runs are exactly the wedges. Then, instead of demanding
one class:

1. **Cut the grown link at class transitions.** A transition between link
   entries `(aᵢ,bᵢ,clsᵢ)` and `(aᵢ₊₁,bᵢ₊₁,clsᵢ₊₁)` with `clsᵢ ≠ clsᵢ₊₁`
   happens exactly at the shared spoke `v→bᵢ` — which IS a class-boundary
   edge through v, i.e. the intersection polyline through the mint
   (star triangles flanking an edge differ in class iff the edge is a
   class boundary). Maximal same-class runs are the **wedges**.
   - Open link (boundary mint — R0099's class): wedges are runs between
     the chain ends and the transitions. ≥2 wedges iff multi-class.
   - Closed link (interior mint on the curve): transitions come in pairs
     (≥2); wedges are the cyclic runs between them. A closed link with
     ZERO transitions stays today's `interior vertex` reject — that class
     (constraint LINE crossed, segment elsewhere) is not on the curve and
     ear-clipping it would orphan v.
   - A junction mint (>2 constraint spokes at v) yields one wedge per
     consecutive spoke pair — the same walk, no special case.
2. **Per wedge, run amendment 5's own step 3.** Wedge polygon
   `[v, aᵢ, bᵢ, …, b_j]` — v INCLUDED, its two bounding spokes (`v→aᵢ`,
   `b_j→v`) as polygon edges. Each spoke is either a constraint edge (the
   moved polyline — preserved, at the mint's CURRENT position: the boundary
   moves WITH the mint, Fig 11(b→c)) or a domain-boundary end (preserved,
   as today). If the wedge saw no growth deferral and its fan triangles are
   all valid → fan; else → the SHARED `earclip_cavity_polygon`
   (`reloc.rs:45`), verbatim, with `cls0 = the wedge's class`.
3. **Commit atomically.** All wedges must produce replacements
   (Σ per-wedge ears = grown-cavity size, the amendment-5 accounting per
   wedge); then overwrite the cavity slots in place exactly as today
   (`reloc.rs:484-508`). Any wedge reject ⇒ `RelocOutcome::Rejected`, NO
   mutation (build-then-commit) ⇒ amendment-2 revert stays the loud
   fallback. A wedge `EarclipErr::NotSimple` propagates
   `RelocOutcome::NonSimple{ring_mints}` with the crossing-narrowed seeds
   (amendment 10 semantics unchanged) so the joint path still triggers.

### 3b. The conformality invariant (what makes this the two-sided form)

The two wedges flanking a constraint spoke re-triangulate against the SAME
spoke — same two vertex ids, same current coordinates. Each wedge's ear-clip
covers its polygon exactly (the existing ear-clip contract), so the union
covers the grown cavity exactly: **no gap and no overlap across the moved
polyline, by shared identity rather than by numerical agreement** — the
#169 `two_sided_conformal_update` principle at the overlay level. The
intersection curve is never re-triangulated across (each ear's class is its
wedge's class; class-boundary edges are wedge-polygon boundary by
construction) — amendment 7's invariant, now honored *through* the mint
instead of by rejecting it.

### 3c. Guards (each a loud reject → amendment-2 revert)

| Case | Behavior |
|---|---|
| closed link, zero class transitions, deferred | reject `interior vertex with constraint-blocked fan` (unchanged — not this arm's class) |
| wedge polygon exactly non-simple | `NonSimple{ring_mints}` propagated (joint trigger, amendment-10 narrowing) |
| wedge polygon pinched / not CCW / non-finite / no clippable ear | reject, no mutation (existing `EarclipErr::Other` strings, now wedge-scoped) |
| single-class deferred cavity | **unchanged** — today's `[v, w₀…w_k]` ear-clip path is the 1-wedge open-link special case; keep its exact code path byte-identical (it excludes v from interior ears only when v has no interior constraint spokes, which is the 1-wedge case by definition) |
| replacement/cavity size mismatch | reject, no mutation (unchanged) |

### 3d. Termination and determinism

Unchanged contracts. Purely combinatorial (`coords` fixed): a committed
relocation replaces the cavity with gate-valid triangles and no other
triangle changes shape, so the gate's folded count strictly decreases.
Growth is the existing monotone scan. Deterministic: the link start is
already deterministic (unique open-chain start / smallest tail), wedge order
follows link order, first-clippable-ear order within each wedge (I6).

### 3e. Gating and flip discipline

Build behind `YANG_S0_MULTICLASS_RELOC_ENABLE`; OFF is byte-identical to
today (the arm replaces a reject, so OFF = reject as before). Measure
corpus OFF/ON back-to-back; flip always-on in the same increment on zero
CORRECT→ERROR (the #195 inc-5 / provenance inc-2 precedent). The env var
then disappears — no permanent mode.

## 4. Increments

- **inc-0 — this spec + the fold-revert CENSUS. ✅ COMPLETE 2026-07-30,
  results in §7.** Both leak sites observable (1f576621): `[fold-revert]`
  under `YANG_SPLIT_PROBE` (1×1, `mod.rs:1257`) and `nary-fold-revert`
  under `YANG_COPLANAR_PROBE` (`nary.rs:624`). Headline: 19 revert cases
  (10 ERROR + 9 CORRECT-latent), zero n-ary events, interior arm 616 vs
  multi-class 139 reject events.
- **inc-1 — the amendment-12 primitive, env-gated.** Wedge decomposition in
  `relocate_minted_vertex` per §3a-3c. **Census addendum (§7.2): extend the
  interior-reject probe line with the link's class-transition count**, so
  the first gated run measures the arm's true coverage of the dominant
  616-event interior class (≥2 transitions covered; 0 transitions stays
  rejected by design). Unit fixtures in `reloc_tests`
  (P4, z=0 identity-frame style): (a) boundary mint, 2 wedges, both fan —
  commit; (b) boundary mint, folded wedge ear-clips while the other fans —
  commit, exact-cover oracle, constraint spokes present in the result's
  edge map; (c) interior on-curve mint (closed link, 2 transitions) —
  commit; (d) junction mint, 3 wedges; (e) 1-triangle wedge, valid at
  minted coords — trivial commit; (f) 1-triangle folded wedge, ungrowable —
  reject with NO mutation; (g) wedge NonSimple propagates `ring_mints`
  (amendment-10 semantics); (h) single-class path byte-identical (existing
  tests re-run gate-ON). Plus the R0099 engine-frame chain fixture
  (extrude-boss + extrude-cut + revolve-cut, direct constructors, the
  `m8_swiss_cheese_chain.rs` pattern): RED gate-OFF (VertexOffSurface),
  expected GREEN gate-ON, volume oracle.
- **inc-2 — flip.** `ASSAY_CASE=R0099 single_case` first, then full corpus
  OFF/ON back-to-back; flip always-on on zero CORRECT→ERROR. Candidate
  conversions: R0099 + whatever the inc-0 census rostered. Ledger + triage
  rows updated; `docs/yang_deviations.md` if any paper deviation is taken.
- **inc-3 — joint-form parity (census-gated).** Only with a measured case:
  (a) widen the amendment-6 trigger to fire on ≥2 multi-class-rejected
  seeds without a NonSimple sighting; (b) region-form wedge parity — teach
  `relocate_region_single_class` callers that a 1-triangle class wedge at
  an on-curve seed is the per-vertex arm's job, or grow it within class
  (`region too small` today). Measure first; no speculative branches
  (the increment-14 singleton-relaxation revert is the cautionary tale).
- **inc-4 — n-ary gate parity (census-gated).** Lift the slice-g B8
  deferral: wire the amendment 4→5(+12)→6 ladder into `nary.rs`'s reduced
  gate, with face attribution as an additional wedge-cut axis (an edge
  between different `(poly_a, poly_b)` attributions is a face boundary —
  as immovable as a class boundary, already the flip constraint at
  `nary.rs:566-571`; wedges cut at class OR attribution transitions).
  Only if a census shows n-ary `nary-fold-revert` leak cases; update
  `specs/m8_nary_tessellated_faces.md` B8 in the same increment.
  **inc-0 census verdict: ZERO n-ary events corpus-wide — DEFERRED with
  no current customers (§7.1).**

## 5. Oracles and validation

- Existing `reloc_tests` + chain pins (F0086–F0090 family, 13 pins) must
  stay green gate-ON — the single-class and joint paths are untouched.
- The R0099 chain fixture is the conversion pin (RED pre, GREEN post,
  stash-verified per the yr18-oracle3 precedent).
- Corpus: full categorized assay, release, `ASSAY_JOBS=8
  ASSAY_CASE_TIMEOUT_SECS=240`; the score is the 5-bucket exhaustive form;
  zero CORRECT→ERROR is the flip bar. `kernel-v2/strict-validation` stays
  the on-surface watchdog — the arm must not merely silence the tripwire
  but place the mints ON their rim circles (the fixture asserts the exact
  on-circle positions survive into the output boundary, not just absence
  of the error).
- Determinism: assay replay byte-identical gate-OFF (the provenance inc-1
  bar); gate-ON deltas are exactly the conversion list.

## 6. Non-goals

- **No Stage-4/#169 changes** — `stage4_update.rs` two-sided machinery is
  cited as the principle, not touched; R0099's op never reaches Stage 4
  (`has_conic = false`), which is precisely why Stage 0 must repair itself.
- **No bands, no tolerance widening** (P10) — the arm re-triangulates at
  exact minted positions; anything it cannot repair stays a loud revert
  caught by strict-validation.
- **No mint-placement changes** — the on-circle mints are correct (they are
  the §4.5.5 "intersection curves"); only the triangulation around them is
  repaired. Reverting correct mints less often is the whole point.
- **No F0090 TIMEOUT work** — its residual is legit chain weight
  (increment-13 finding), not this arm.
- The Stage-3/provenance route (`yang_s3_intersection_edge_provenance.md`)
  is disjoint by construction: this op class has ZERO arrangement
  constraint edges — there is nothing to vouch for. Kin in theme (Fig-11
  mesh updating), disjoint in mechanism.

## 7. inc-0 CENSUS COMPLETE (2026-07-30) — 19 cases, zero n-ary, interior arm dominates 616:139

**Method.** The `ASSAY_JOBS` driver nulls child stderr (`assay_kv2.rs:659`),
so the census ran as a parallel sweep of `single_case` subprocesses (the
sanctioned manual-probe path, same invocation the driver uses), 8 jobs,
`YANG_SPLIT_PROBE=1 YANG_COPLANAR_PROBE=1`, in-child wall guard 300s,
stderr captured per case. **312/312 completed, zero exit failures, zero
verdict drift vs the canonical 259C/0W/51E/0T baseline** — the census is
directly comparable. Counts below are probe EVENTS (the gate loop
re-attempts folds across passes, and one fold's revert covers several
mints), not distinct vertices.

### The 19-case roster

| case | canonical | flips | reloc | region | **reverts** | dominant rejects |
|---|---|---:|---:|---:|---:|---|
| C0048 | ERROR | 4 | 7 | 3 | **43** | interior ×34, not-simple ×20, multi-class ×13, not-CCW ×5; region: ungrowable ×14, all-rejected ×10 |
| F0064 | ERROR | 1 | 0 | 0 | **17** | interior ×11, multi-class ×8 |
| F0067 | ERROR | 31 | 18 | 0 | **183** | interior ×243, multi-class ×60 |
| F0072 | ERROR | 21 | 11 | 0 | **77** | interior ×87, multi-class ×25 |
| R0025 | ERROR | 3 | 0 | 0 | **22** | interior ×32 |
| R0026 | ERROR | 0 | 0 | 0 | **4** | interior ×3, multi-class ×2 |
| R0050 | ERROR | 0 | 0 | 0 | **7** | multi-class ×5, interior ×3 |
| R0051 | ERROR | 1 | 0 | 0 | **5** | interior ×4, multi-class ×2 |
| R0085 | ERROR | 8 | 0 | 0 | **65** | interior ×130, multi-class ×5 |
| R0099 | ERROR | 0 | 0 | 0 | **6** | multi-class ×4, interior ×1, not-simple ×1; region: ungrowable/too-small/all-rejected ×1 each |
| F0090 | CORRECT | 56 | 35 | 28 | **4** | not-simple ×47, interior ×7 |
| R0007 | CORRECT | 2 | 2 | 0 | **4** | interior ×4 |
| R0013 | CORRECT | 0 | 0 | 0 | **5** | interior ×9 |
| R0021 | CORRECT | 0 | 0 | 0 | **4** | interior ×3, multi-class ×2 |
| R0024 | CORRECT | 2 | 2 | 0 | **7** | interior ×8 |
| R0059 | CORRECT | 2 | 0 | 0 | **16** | interior ×10, multi-class ×8 |
| R0063 | CORRECT | 0 | 0 | 0 | **2** | interior ×1, multi-class ×1 |
| R0072 | CORRECT | 4 | 0 | 0 | **9** | interior ×6, multi-class ×4 |
| R0088 | CORRECT | 3 | 0 | 0 | **12** | interior ×20 |

Reject-event totals: `interior vertex with constraint-blocked fan` **616**,
`multi-class cavity with constraint-blocked fan` **139**, `cavity polygon
not simple` 69, `not CCW` 5; region form: `crossing edges ungrowable` 16,
`every folded class sub-region rejected` 12, `too small` 1, `not CCW` 1.
Ladder successes on the same 19 cases: 165 flips, 103 per-vertex
relocations, 37 region relocations — F0090 is the showcase (119 commits,
4 reverts, CORRECT), R0099 the anti-showcase (0 commits, 6 reverts, ERROR).

### Findings

1. **Zero n-ary reverts corpus-wide.** The slice-g B8 reduced gate has no
   corpus customers today — **inc-4 stays deferred** (probe stays banked;
   revisit only if a future census shows events).
2. **The interior arm out-weighs the multi-class arm ~4.4:1.** The wedge
   decomposition (§3a) covers interior on-curve mints (closed link, ≥2
   class transitions); a zero-transition interior reject stays rejected by
   design. The census cannot split those two sub-classes from reject
   strings — **inc-1 must extend the reject probe line with the link's
   class-transition count** so the arm's true coverage is measured before
   the flip.
3. **ERROR-bucket customers: R0099 is the only PROVEN revert-caused ERROR**
   (`[fold-revert] vert=9` = the VertexOffSurface point digit-for-digit).
   The other nine carry reverts but their canonical errors name varied
   walls, several with existing vehicles: R0026 → Stage-3 AmbiguousCurve
   (provenance spec's loud stop), R0025 → ring-reject row, F0067 + C0048 →
   the #144 opposite-rim snap-rounding family, F0064/F0067/R0085 →
   TessellationFailed, C0048/F0072 → azimuth-merge rim mismatch, R0050 →
   Stage-4 relocation region, R0051 → SelfIntersectingBooleanOutput.
   Plausible kin (chord-lift verts feeding downstream walls), **causality
   unproven** — the fourth error-string-proximity lesson applies: expected
   inc-2 conversions = R0099 + whatever the arm actually flips; anchor
   each survivor separately before claiming it.
4. **The latent class is real: 9 SUPPORTED_CORRECT cases carry 63
   chord-lift revert events that pass every current check** (canonical
   verdicts INCLUDE `strict-validation` — the reverted verts either don't
   survive into kept faces or sit inside the validator bands). Includes
   three freshly-converted cases (R0063, R0072, R0021) whose headline
   defects were fixed by other arms while Stage-0 reverts persist
   elsewhere in their chains. This list is inc-2's regression watch AND
   the P10 argument for the arm: today's gate is already emitting
   chord-position vertices into CORRECT outputs — repairing the
   triangulation is strictly better than reverting correct mints.
5. Region-form rejects are rare (30 events total) and concentrated in
   C0048/R0099 — consistent with §2's analysis that the joint form's gaps
   are secondary to the per-vertex arms. inc-3 keeps its census gate.

Raw logs: session-scratchpad `census/` (ephemeral); these tables are the
durable record.
