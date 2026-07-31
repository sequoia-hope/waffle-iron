# SPEC — M8 Stage-0 overlay mesh-updating: the MULTI-CLASS cavity arm (amendment 12)

**Status: amendment-14 inc-3.2d FLIPPED ALWAYS-ON (2026-07-30, §11) —
the Fig-11(a) vertex-inserting SPLIT is live and R0099 is CONVERTED
(ERROR → SUPPORTED_CORRECT, the flip's only category change, seam-free
at i6). New canonical: 260C/0W/48E/0T.** The full amendment stack is
now always-on: wedge decomposition (12, §3), the Fig-11(b→c) MERGE arm +
rim-chain boundary-order settle check + split-table merge identification
(13, §10d), and the vertex-inserting split (14, §11). Residuals carried
(§12 re-census, 2026-07-30): inc-3 region-form parity DEMOTED to
census-armed-no-customer (zero proven customers post-conversion); next
anchors in evidence order = the F0064 collapsed-planar-triangle wall,
the C0048 #144 azimuth-merge family, and the split-open-link class (74
events). **§13 (2026-07-30, F0064 anchor investigation): anchors #1
and #3 are the SAME defect — F0064 is the split-open-link class's
proven loud customer; the open-link split is RE-ARMED as
amendment-15 (design frame §13f, anatomy census inc-0 §13g).** The latent chord-lift watch list is 5 CORRECT cases / 25
revert events. Amendment-12's §9
armed-joint-path win stands (R0085 fold-reverts 65 → 10).

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
- **inc-1 — the amendment-12 primitive, env-gated. ✅ COMPLETE 2026-07-30,
  measurements in §8.** Wedge decomposition in `relocate_minted_vertex`
  per §3a-3c (`reloc.rs`, `relocate_minted_vertex_impl` with the gate as
  an explicit parameter — reloc_tests exercise both states without env
  mutation; production reads `YANG_S0_MULTICLASS_RELOC_ENABLE`). The §7.2
  interior-reject probe now prints the link's cyclic class-transition
  count. Unit fixtures shipped as specced with ONE correction: fixture (a)
  "2 wedges, both fan" is UNREACHABLE — the deferred path only runs when
  some link edge kept an invalid fan triangle, and that edge's wedge can
  never fan, so every reachable decomposition ear-clips ≥ 1 wedge; (a)/(b)
  share the minimal reachable form (folded wedge ear-clips, other fans).
  Fixtures (c)–(h) as specced, incl. the closed-link interior commit, the
  3-wedge junction, and the crossing-narrowed NonSimple propagation. The
  R0099 engine-frame chain fixture
  (`kernel-v2/tests/m8_r0099_multiclass_chain.rs`) pins RED gate-OFF
  (VertexOffSurface, `op3`, FaceId(18) — digit-identical to the corpus);
  the gate-ON green oracle is quarantined on **inc-3**, not inc-2: at
  R0099's actual mints every folded wedge polygon is NON-SIMPLE (§8).
- **inc-2 — flip. ✅ COMPLETE 2026-07-30, results in §9.** R0099
  `single_case` OFF/ON first (ERROR both ways, digit-identical — as §8
  predicted), then full corpus OFF/ON back-to-back: gate-OFF byte-identical
  to canonical (0 category + 0 detail changes — the determinism bar), gate-ON
  ZERO category changes, zero conversions, 6 ERROR detail shifts. Bar met ⇒
  flipped always-on and the env var removed (no permanent mode, §3e). No
  paper deviation taken (`docs/yang_deviations.md` untouched — the arm IS
  the §4.4.1/§4.5.5 composition). Triage rows updated.
- **inc-3 — joint-form parity (census gate ARMED 2026-07-30, §8).** The
  measured case list exists: R0099 gate-ON reaches the joint path via the
  wedges' NonSimple propagation (region attempts [178,182] and
  [115,116,120,126]) and the REGION form is the wall — `crossing edges
  ungrowable (region polygon not simple)` (both sub-cases), `region too
  small` (the [115,…] Overlap sub-region — exactly the 1-triangle class
  wedge named in (b)), `every folded class sub-region rejected`. Note
  (a)'s trigger widening is now LESS urgent than drafted: the wedge arm
  itself supplies the NonSimple sightings that the old multi-class reject
  never did, so the trigger fires on these cases already; the residual
  work is the region form's own on-curve growth/decomposition — the
  region-level analog of the §3a wedge cut. Measure per sub-case before
  building (the increment-14 singleton-relaxation revert is the
  cautionary tale).
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

## 8. inc-1 MEASUREMENTS (2026-07-30) — coverage total, wall moved to the region form

Probe method: `single_case` release runs, `YANG_SPLIT_PROBE=1`, gate ON via
`YANG_S0_MULTICLASS_RELOC_ENABLE=1` (the driver-nulls-stderr trap from §7
applies — never census through the `ASSAY_JOBS` driver).

**§7.2 transition census — the interior class is ALL on-curve.** Every
interior-reject event across the five heaviest census cases carries exactly
2 cyclic class transitions (gate-OFF probe): R0099 1/1, R0085 130/130,
F0067 243/243, C0048 34/34, R0088 20/20 — 428/428, zero 0-transition
events. The by-design reject class (constraint LINE crossed, segment
elsewhere) is EMPTY in the measured set: the wedge arm structurally covers
the entire dominant census class. Coverage ≠ repair, though — see below.

**Gate-ON at the failing mints: the folded wedge polygon is NON-SIMPLE.**
R0099 (fixture + corpus, identical behavior): all four multi-class mints
(verts 4/9/116/182; vert 9 the closed interior form, wedges
`[(Overlap,3),(BOnly,3)]`) decompose and FAN their valid wedges, but each
FOLDED wedge's polygon is exactly non-simple — the interacting-mints
signature; vert 4's ring shows a neighbor mint's collapsed chord passing
through v's minted position. `NonSimple` propagates crossing-narrowed
seeds; the joint path fires (region attempts it never made gate-OFF); the
region form rejects (§4 inc-3). R0099 stays ERROR (VertexOffSurface(18)),
in-chain reverts 6 → 3.

**The indirect win is real and large.** R0085 gate-OFF → gate-ON:
per-vertex commits 0 → 1, REGION commits 0 → 6 (the wedges' NonSimple
seeds arm the joint path), fold-reverts **65 → 10**; verdict unchanged
(its own TessellationFailed(566) wall, as §7 predicted). Residual wedge
rejects on R0085: 25× `cavity polygon not simple`, 1× `not CCW` — the
same interacting class. Flip calculus for inc-2: conversions are NOT the
expected payoff; the payoff is chord-lift reverts repaired out of CORRECT
and ERROR meshes alike (the §7 finding-4 latent class), and the risk is
that those same triangulation changes perturb the 9 CORRECT-latent cases —
zero CORRECT→ERROR remains the bar.

## 9. inc-2 CORPUS RESULT (2026-07-30) — flipped always-on, zero category changes

- **Determinism bar:** full corpus gate-OFF vs the canonical 2026-07-30
  baseline — **0 category changes, 0 detail changes** across 312 cases
  (259C / 51E / 1 EXPECTED_ERROR / 1 UNSUPPORTED). inc-1's code is provably
  inert when gated off.
- **Flip bar:** gate-OFF vs gate-ON — **0 category changes** (all 259
  CORRECT stay CORRECT, incl. all nine §7 latent-revert watch cases; no new
  TIMEOUTs; zero conversions, as §8 predicted). **Bar met ⇒ always-on; the
  env var and the `_impl` gate parameter are removed** (fixture (h) became
  the single-class exact-output pin per the amendment-8 test-replacement
  precedent).
- **6 ERROR detail shifts** (ERROR→ERROR, acceptable): C0048, F0067, F0072,
  R0085 (same walls, shifted ids/details), and — notable — **R0025**
  (TessellationFailed(588) at subtract → `input B-Rep is not 2-manifold` at
  the Extrude-2 auto-union) and **R0026** (Stage-3 AmbiguousCurve → the
  same non-manifold-input signature). The repaired Stage-0 meshes moved
  both cases onto a SHARED wall — a candidate common defect to anchor
  during inc-3 (do not assume it is the region form; anchor first).
- The assay UI snapshot (`app/tests/cases/assay/results.json`) now carries
  the always-on detail strings.

## 10. AMENDMENT 13 — the Fig-11(b→c) MERGE arm (inc-3, measured 2026-07-30)

### 10a. The measured residual, fully anchored

inc-3.0 probes (`[reloc-region-ungrowable]` with per-edge block reasons +
ring mintedness, `[reloc-region-fig11]`, `[reloc-region-toosmall]`, and the
`YANG_INPUT_VERT_PROBE` branch classifier) pin R0099's two region rejects
to ONE shape — the **Z-fold backtrack**:

| ring | crossing | backtrack pair (p → q) | ‖p−q‖ | p branch |
|---|---|---|---|---|
| [178,181,184,187,185,182] BOnly | e3(187→185) × e5(182→178) | p=185 → q=182 | 0.029 | `lift_or_snap` |
| [109,115,120,126,132,127,121,116,110] AOnly | e5(127→121) × e7(116→110) | p=121 → q=116 | 0.0046 | `lift_or_snap` |

The boundary walks out past the mint to p, then BACKTRACKS to q (the mint),
so the overshooting chord crosses the mint's exit edge by a hair (q sits
~2.5e-5 past the (187,185) chord line; 184/185 share a sweep-column x
bit-exactly). Every crossing partner is a CONSTRAINT edge (domain chord
with 0 externals, or a curve edge whose external is the other class) —
amendment-8 growth is definitionally impossible. The `region too small`
reject is the SAME defect: tri 145 = [116(mint), **121(p)**, 122] — the
unmerged pair again. And p is UNMINTED + `lift_or_snap` in both cases: a
pure sweep-event discretization vertex, not an input corner, not a rim
sample.

### 10b. The paper operation (this is the spec)

Yang §4.4.1 Fig 11 caption, verbatim: *"(a). We locate the constrained
edge containing q (the red edge) and split it using q. (b). If an endpoint
p of the split edge is too close to q, we merge p with q as shown in (c)
to improve the mesh quality."* Body: *"we remove a mesh vertex if it is
too close to the intersection curve on the mesh."* The gate ladder today
has NONE of split/merge/remove — every arm re-triangulates a FIXED vertex
set. The measured residual is exactly the missing MERGE.

### 10c. Design — amendment 13

- **Trigger (exact, no distance band — P10-clean):** the region form's
  growth is exhausted AND `fig11_backtrack_pair` fires: the two crossing
  ring edges sit exactly two apart, sandwiching one edge joining unminted
  p ↔ minted q (`reloc.rs::fig11_backtrack_pair`, shipped with inc-3.0 as
  the probe). Guard: p must be MERGEABLE — resolution branch
  `lift_or_snap` (NOT a corner of either input, NOT a rim sample, NOT
  itself minted); the branch data lives in mod.rs's resolution maps, so
  the mergeable mask is precomputed there once per pair.
- **Action:** `coords[p] = coords[q]` — a POSITION merge; p becomes a
  bit-twin of q. No topology surgery: the existing machinery was built
  for twins (ring dedup in the ear-clip, shared-position/zero-length
  skips in `first_ring_crossing`, `gate_tri_degenerate` → M-B emission
  drop for the collapsed slivers).
- **Placement:** a LADDER arm in mod.rs between the joint attempt and the
  amendment-2 revert. The region form returns the candidate on reject
  (richer return), the ladder merges (probe `[fold-merge]`), sets
  `changed`, and the next gate pass re-attempts everything with the
  merged geometry — the per-vertex wedge rings containing p become simple
  too (p's UV appears in vert 116's rejected wedge ring), so the wedge
  arm commits without the joint form where possible.
- **Termination:** lexicographic. A merge strictly reduces the count of
  distinct resolved positions (bounded by V) and is idempotent (bit-equal
  after; the crossing scan skips shared positions, so the same pair never
  re-fires); between merges the existing strict-fold-decrease invariant
  holds. A merge may transiently fold p's other incident triangles —
  those get their own ladder passes.
- **Non-goals:** the SPLIT operation (Fig 11a — inserting q into a
  constrained edge as a new chain vertex) is NOT needed by any measured
  customer (q is always already a mesh vertex here); build it only when a
  census names a case. Singleton NonSimple at the wedge/single-class site
  (no joint trigger) keeps today's behavior — the measured customers all
  reach the joint path with ≥2 seeds.

### 10d. Increments

- **inc-3.0 — measurement + this design. ✅ COMPLETE 2026-07-30** (probe
  extensions: per-edge ungrowable reasons, ring mintedness dump,
  `fig11_backtrack_pair` detector, too-small context; `minted_mark`
  threaded through the region form).
- **inc-3.1 — the gated merge arm (`YANG_S0_FIG11_MERGE_ENABLE`).
  ✅ SHIPPED + MEASURED 2026-07-30.** Mergeable mask in mod.rs
  (`lift_or_snap` provenance); `RegionOutcome::MergeCandidate` on region
  reject; `RelocOutcome::NonSimple.merge_candidate` at BOTH per-vertex
  NotSimple sites (the census refuted §10c's singleton non-goal within
  hours: R0099 verts 4/9 are singleton NonSimple — empty `ring_mints` —
  and never reach the joint form); `fig11_backtrack_pair` tries both
  sandwich mids (a 4-ring's crossing sandwiches TWO edges; vert 4's true
  pair sat on the second). Ladder arm merges after the joint stage with
  TWO guards: provenance (`mergeable_mark`) and the **displacement
  guard** — ‖p−q‖ ≤ ‖q − chord(q)‖, i.e. p must lie inside the zone the
  mint's own displacement swept over. Per-mint and scale-free, not a
  tuned band; the base()-shaped combinatorial false positive (gap 0.89
  vs rim-snap-scale displacement) fails it by 10×, the measured true
  cases pass by 2–15× (gaps 0.0047/0.018/0.029 vs disps
  0.073/0.082/0.065). **R0099 chain gate-ON: THREE merges fire (121→116,
  185→182, and singleton vert 4's 1→4), fold-reverts 6 → 1.** The
  residual revert is vert 9 — a DIFFERENT paper operation (below).
- **inc-3.2 — the vertex-inserting split (census-armed by vert 9;
  detector + probes SHIPPED, actions deliberately NOT — two designs
  refuted by measurement).** The full anatomy, three probes deep:
  vert 9's 1-incident chord (10,6) is **the OTHER INPUT's real model
  edge** (B's rectangle edge), not the rim owner's chord. A's rim
  circle is near-TANGENT to that edge: the on-circle mint bulges
  ~1.7e-4 past it over a ~0.065 span, the chord-geometry arrangement
  never saw a crossing (**no junction vertex exists within 8e-2** —
  `YANG_INPUT_VERT_PROBE` scan), and the sliver's BOnly class — correct
  at chord geometry — is INVALIDATED by the mint (the strip up to B's
  edge becomes Overlap; the bulge past it becomes AOnly). Refuted
  fixes, with evidence: (a) deleting the sliver holes B's face-0
  emission (`i6-input-overuse` fwd=1 rev=0 at the mint — B's neighbors
  already route through it); (b) reversing it to "B-material winding"
  double-books the spoke (fwd=2 rev=0: the sliver and its Overlap
  spoke-neighbor are BOTH B-emitted). The truthful §4.4.1 operation
  needs a NEW VERTEX: split B's edge at the mint's projection,
  re-decompose the sliver into the Overlap strip + the AOnly bulge, and
  propagate the edge split into B's own tessellation
  (`collect_edge_splits` territory) — the overlay's first
  vertex-INSERTING operation. Design spec first (a §11 of its own);
  then the gated-primitive → fixtures → chain → corpus arc. What
  shipped in this increment: `fig11_split_chord`,
  `RelocOutcome::NonSimple.split_chord`, the `[fold-split-reject]`
  census probe (candidate + chord incidence), and the
  `i6-input-overuse` incident-triangle enrichment.
- **inc-3.3 — merge-arm corpus flip ATTEMPTED 2026-07-30: NO FLIP, bar
  failed on R0059.** Gate-OFF replay byte-identical to the inc-2
  canonical (all inc-3 machinery provably inert gated). Gate-ON: 3
  category changes — F0064/F0067 ERROR→UNSUPPORTED(coplanar-boolean)
  (their repaired meshes now reach the loud typed M8 wall; non-CORRECT
  recategorizations, acceptable) and **R0059 SUPPORTED_CORRECT → ERROR**
  (`reassembled output would be non-2-manifold`) — one CORRECT
  regression ⇒ the arm STAYS GATED. The counterexample, anchored:
  `[fold-merge] pair=(0,1) p=20 → q=25 gap=2.178 disp=2.781` at model
  scale ~300 — a coarse rim's snap displacement (2.78) admitted a
  2.18-unit merge that destroyed real geometry; the candidate's ring
  crossing is a UNIT-SCALE interpenetration of ~2–4-long edges, not a
  graze. The displacement guard is necessary but not sufficient.
- **inc-3.4 — the CONTAINMENT guard: SHIPPED but measured
  NON-DISCRIMINATING (2026-07-30).** Built as designed (detector
  measures overshoot(q → p's crossing edge) + chord length; ladder
  compares against the rim-slot sagitta). Measurement overturned the
  design premise: ALL these mints are JUNCTION mints (exact circle∩line
  onto that very edge), so overshoot is machine-zero by construction —
  R0099 1e-16..1e-17, **R0059 7e-15 (passes; still merges; still
  CORRECT→ERROR)**. The guard stays (cheap, and a true premise-check
  for any future non-junction candidate) but does not gate R0059.
- **inc-3.5 — the R0059 discriminator is NOT a guard predicate (three
  measured dead ends); the lead is MISSING PROPAGATION.** Measured:
  boundary status does not discriminate either (`[fold-merge-boundary]`
  probe: true cases p=1 and p=185 are ALSO union-boundary vertices,
  like R0059's p=20; only p=121 is interior). R0059's breakage is
  input **A**'s seam (i6: edges (47,49),(48,50) fwd=1 rev=0 at the
  moved neighborhood) — the position-merge moved a boundary vertex the
  face's OTHER meshes were built against, i.e. merges bypass the
  rim-override/edge-split propagation that MINTS get
  (`collect_rim_crossings`/`collect_edge_splits`). Next measurement:
  trace input-A verts 47/49/50's provenance (which propagation table
  references the un-merged position), then either propagate merges
  through the same override path or guard on "p referenced by a rim
  override". The merge arm STAYS GATED until that lands; then the
  corpus bar, and R0025/R0026's shared wall anchored at the flip.
- **inc-3.5 TRACE COMPLETE (2026-07-30): both "missing propagation"
  guesses REFUTED — the propagation tables are position-correct; the
  defect is a BOUNDARY-ORDER inversion between the two consumers of one
  rim table.** The full mechanism, measured end to end on R0059 op 001
  pair (0,1) (overlay dump + i6 + azimuth reconstruction):
  1. Face 0's outer rim chord (v31→v2) carries four crossing vertices;
     in chord-parameter order: junction mint v25 (t=0.061), mint v19
     (t=0.086), mint v13 (t=0.258), mint v7 (t=0.487). Gate-OFF the
     ladder reverts BOTH v19 and v25 to chord positions → every rim
     consumer agrees on order → conformal → CORRECT (canonical).
  2. Gate-ON, the merge (p=20→q=25) repairs enough folds that junction
     mint v25 SURVIVES on-circle while v19 still reverts. A junction
     mint is circle∩line — azimuthally displaced from its chord anchor
     by up to the snap displacement (2.78 here, a coarse 10-gon rim at
     r≈90.6). Measured azimuths: v31 −108.000°, **v19rev −105.055°,
     v25M −104.281°** (LEAPT PAST v19), v13 −98.934°, v7 −90.488°.
  3. The cap-side overlay emits the chain in chord-parameter order
     (v25 before v19); the ring builder
     (`stage1_tessellate.rs` slot sort) orders by azimuth (v19 before
     v25). The lateral's rim chain SWAPS the pair → 5 unpaired directed
     edges in mesh_a (i6 under-reports 2 of 5: its canonical-key loop
     skips a pair whose s<e direction never occurs — probe fix due).
  4. **The class is NOT merge-specific: canonical gate-OFF op 002
     carries the SAME latent seam** (kept junction leapt past reverted
     v23, i6 edges (48,50)/(49,51)) and survives only because the
     downstream arrangement happens to absorb it. The merge merely
     created the first instance that reaches the reassembly wall.
  Also found: `collect_edge_splits` dedups by exact parameter t only,
  so a SURVIVING merge leaves two same-position split entries on the
  other input's edge (measured: splits_b edge (5,6) t=0.62276 and
  t=0.62517, both at the junction) — a second, latent conformality
  wound for every merge whose p sits on an input edge.
  **Design (this increment, all gated on `YANG_S0_FIG11_MERGE_ENABLE`
  so gate-OFF stays byte-identical):**
  - *Rim-chain boundary-order settle check* (`settle_rim_chain_order`,
    rim_chords.rs): when a gate pass ends quiescent, re-scan every rim
    chord's crossing set (the exact collinearity + parameter-window
    predicate of `collect_ring_crossings`, so the policed set IS the
    propagated set); in chord-parameter order the resolved azimuthal
    offsets must be monotone. On an adjacent inversion, revert the
    DISPLACED member(s) (coords ≠ lift — two undisplaced chord points
    cannot invert, so a victim always exists) to their chord lift
    (amendment-2 semantics at chord granularity), restore any merge
    partners of a reverted target, mark it merge-ineligible, set
    `changed`, and let the gate ladder re-run. One inversion per
    firing; termination: the reverted set grows monotonically.
    P10-clean: an exact order invariant plus the sanctioned loud
    fallback — no acceptance band. The azimuth sort itself is the
    revolved lateral's arc-length parameterization and cannot honor a
    non-monotone chain (forcing boundary order would emit bowtie
    laterals); the chain must be MADE monotone, not the sort changed.
  - *Merge bookkeeping*: record (p, q, p_orig); ANY revert of q (settle
    check or amendment-2) restores p to p_orig — merges propagate
    through the revert path exactly like mints do.
  - *Split-table identification*: `collect_edge_splits` additionally
    dedups an entry whose resolved position equals a SURVIVING merge
    target's position (scoped to merge-produced twins via a
    merged-positions set, so gate-OFF stays byte-identical) — the
    §4.4.1 merge identification carried through the §4.5.5 propagation,
    the same argument as the M-B emission drop.
  - *i6 probe fix* (boolean.rs, diagnostic-only): aggregate per
    canonical pair before comparing so one-sided edges are reported.
  Then the corpus bar (R0099 win preserved, zero CORRECT→ERROR), and
  R0025/R0026's shared wall anchored at the flip.
- **inc-3.6 — FLIPPED ALWAYS-ON 2026-07-30; the env var is removed.**
  Increment results, measured end to end:
  - R0059 gate-ON `single_case`: **SUPPORTED_CORRECT** (the settle check
    reverts the leaping junction, restores partner v20; op 002's
    canonical-latent leap settles too — **0 i6 seam lines gate-ON vs 4 in
    canonical gate-OFF**). R0099: behavior identical to inc-3.1 (3 merges
    fire, settle never triggers — the check discriminates; the chain pin
    is green both modes; `single_case` stays the digit-identical
    VertexOffSurface(18) ERROR awaiting the split arm / region parity).
  - **Determinism bar:** full corpus gate-OFF vs the TRUE canonical —
    0 category + 0 detail changes across 312. (Found while comparing:
    commit 673a5426 had accidentally committed the inc-3.3 GATE-ON
    `results.json` — the runner overwrites that committed file on every
    full run, so a measured-then-rejected experiment leaves its
    regression in the tree unless restored. Corrected in this commit;
    compare future baselines against `git show`, not the working tree.)
  - **Flip bar:** gate-OFF vs gate-ON — **zero CORRECT→ERROR** (all 259
    CORRECT stay CORRECT, R0059 included). 2 category changes, both
    non-CORRECT recategorizations onto the loud typed M8 wall:
    **F0067, F0072 ERROR → UNSUPPORTED(coplanar-boolean)** (their
    repaired Stage-0 meshes now reach the coplanar-input-pair check —
    the inc-3.3 pattern, with F0072 replacing F0064, which stays ERROR
    under the settle check). 2 ERROR detail shifts: C0048, R0026.
    Bar met ⇒ merge arm + settle check + split-table identification
    always-on; the ON run's `results.json` is the new canonical
    (259C/0W/49E/0T + 2 recategorized UNSUPPORTED).
  - Residuals carried forward: the R0025/R0026 shared
    `input B-Rep is not 2-manifold` wall (anchor before assuming it is
    the region form), the inc-3.2 vertex-inserting split (vert 9 = the
    R0099 conversion), and inc-3 region-form parity.

## 11. AMENDMENT 14 — the Fig-11(a) vertex-inserting SPLIT (inc-3.2, designed 2026-07-30)

### 11a. The measured customer, exact (R0099 pair (0,0), vert 9)

In-frame numbers from the `[fold-split-anatomy]` probe (shipped this
increment — offline frame reconstruction is lossy at 1e-4 scale; these are
the gate's own projections):

- **The constrained chord C** = overlay edge (10,6) — a sub-segment of the
  OTHER input's real model edge (B's wedge-profile rectangle edge,
  subdivided at sweep columns into the chain v6 → v10 → v14), 1-incident
  (union boundary). C runs 6=(-9.565485, -3.670012) → 10=(-9.424864,
  -3.283462), length 0.41133.
- **The mint chain** = A's cap rim boundary v5 → v9 → v13 (v13 a `rim_a`
  sample; v5, v9 rim mints — v5 KEPT at (-9.602203, -3.916052), v9 the
  residual customer). This rim is SHARED with the outer cylinder lateral —
  the `VertexOffSurface(FaceId(18))` leak site.
- v9 chord UV (-9.424864, -3.535514) sits **8.616805e-2** on the inside of
  C's line; minted (-9.507622, -3.510301) lands **2.232875e-4 PAST it** —
  A's rim circle is near-tangent to B's edge; the chord-geometry
  arrangement never saw a crossing (no junction vertex within 8e-2).
- The moved chain crosses C twice: **q1** = (v5m→v9m) × C at
  t_chain=0.99552, t_edge=0.40847; **q2** = (v9m→v13) × C at
  t_chain=0.00258, t_edge=0.41872. Span |q1q2| = 4.216e-3, both strictly
  interior to C's segment. Note t_chain ≈ 1 and ≈ 0: the crossings are
  each within 0.5% of the mint along the chain — the paper's Fig-11(b)
  "too close" configuration, but BOTH merge directions are
  constraint-deadlocked (v9m must stay on A's exact circle; q1/q2 must
  stay on B's real edge) ⇒ the SPLIT with chain subdivision is the only
  exact resolution.
- Star damage at minted geometry (signed areas): t10 [9m,10,6] = −4.59e-5
  (THE fold, gate area −9.18e-5 in 2× convention); t9 [6,5m,9m] and t15
  [10,9m,13] stay positive but OVERSPILL past C by thin slivers (their
  9m-incident edges leave immediately from C's on-chord endpoints);
  stretched Overlap t14 [12,13,9m] covers the below-C lens correctly for
  BOTH sides but also overspills past C on B's side. Containment tests:
  the bulge point is covered by t14 (+) and folded t10 (−); the lens
  point by t14 alone. **A's emission is already truthful; the entire
  defect is B-side cover past its own boundary + the folded sliver.**

### 11b. The paper operation (this is the spec)

Yang §4.4.1 Fig 11 (`refs/text/yang2025_hybrid_boolean.txt:546-566`):
the inserted intersection polyline must be THE BOUNDARY of the trimmed
triangle meshes; "(a) We locate the constrained edge containing q (the
red edge) and split it using q"; CDT re-triangulates each trimmed side
against the polyline, "contains no flipping triangles". Composed with
§4.5.5 (shared boundaries sample identically on both models), the
overlay form is: **where the moved class-boundary chain crosses a
constrained chord, the crossing points become vertices of BOTH the chord
and the chain, and the mint's star re-triangulates so every sub-region
lies on one side of each** — the overlay's first vertex-INSERTING
operation (every prior arm re-triangulates a fixed vertex set).

### 11c. Design — amendment 14: cavity-scoped split

Trigger (exact, census-armed): the per-vertex ladder's `NonSimple` reject
carries `split_chord = C` (the shipped detector: exactly one crossing
ring edge touches v) AND the merge arm did not fire. Compute the
crossings of v's two class-boundary chain edges (at CURRENT positions)
with C's segment; the armed form requires EXACTLY TWO proper crossings
(entered and exited — the bulge), each with t_edge strictly interior to
C and t_chain ∈ (0,1). Any other multiplicity is a DIFFERENT class:
loud probe, amendment-2 revert unchanged.

Action (build-then-commit, all-or-nothing like every reloc arm):
1. **Mint q1, q2** as new overlay vertices ON C: UV = exact rational
   `sa_exact + t·(sb_exact − sa_exact)` with t the rational lift of the
   f64 crossing parameter — EXACTLY collinear with B's model edge in
   `exact_verts` arithmetic, so `collect_edge_splits` propagates them
   into B's adjacent faces with zero new machinery (the B-leg is
   automatic). `coords` = `frame.lift(uv)` (on the straight in-plane
   model edge by construction); `minted_mark = false` (they are not rim
   mints); mergeable = false (junction-like, immovable).
2. **Re-cut the star.** Carve star(v) (the amendment-5 cavity); the
   cavity ring is unchanged. Re-triangulate the ring polygon + interior
   vertex v (at its minted position) + q1 + q2 with the constraint set:
   chain pieces (prev→q1), (q1→v), (v→q2), (q2→next) and C pieces
   (ca→q1), (q1→q2), (q2→cb) as constrained edges. Sub-regions and
   classes (exact orientation predicates in the frame, the gate's own
   arithmetic):
   - below-C ∧ chain's non-material side → the two BOnly remnants
     (t9/t15 trimmed back to C at q1/q2);
   - below-C ∧ chain's material side → **Overlap** (the lens — B-side
     cover up to its true boundary; A-side unchanged truth);
   - past-C ∧ material side → **AOnly** (the bulge [q1, v, q2] — A pokes
     past B's edge, B correctly absent);
   - past-C ∧ non-material side cannot occur inside the cavity (no ring
     vertex is past C) — assert-guarded, loud reject if violated.
   C's sub-segment (q1,q2) becomes a 2-incident class boundary
   (AOnly | Overlap) — it IS B's face boundary there; (ca,q1)/(q2,cb)
   stay 1-incident union boundary.
3. **A-leg propagation (the rim side-channel).** The chain gains q1/q2,
   and the chain is A's cap rim shared with its lateral — without
   propagation this is a T-junction at exactly today's leak face. q1/q2
   lie 2.2e-4 INSIDE A's rim circle — within the ring builder's sagitta
   band (its on-rim validation admits [r − sagitta, r]) — so record them
   in a per-pair `extra_rim_points` side-channel that
   `collect_rim_crossings` merges into the cap edge's override entry
   (they are NOT UV-collinear with any rim chord — the standard scan
   cannot see them). Ordering across consumers is exactly what the
   inc-3.5 settle check polices; the settle check must treat q1/q2 as
   immovable chord points (minted_mark=false ⇒ never a revert victim ✓).
4. **Gate interplay.** The split sets `changed`; the next gate pass
   re-attempts everything on the new triangulation (the fold is gone by
   construction — every new triangle is on one side of each constraint).
   The split fires at most once per (v, C) pair (idempotence: after the
   split the chain no longer crosses C, so the detector cannot re-arm);
   vertex count grows by exactly 2 per firing, bounded by the folded-set
   size — termination composes with the existing lexicographic argument.

### 11d. Guards (each a loud reject → amendment-2 revert)

| Case | Behavior |
|---|---|
| crossing count ≠ 2 on C (single graze, colinear touch, >2) | reject `split-crossing-count`, probe with count |
| t_edge outside (0,1) or t_chain outside (0,1) | reject `split-param-window` (the crossing belongs to a NEIGHBOR chord — a different customer; census first) |
| bulge depth > the mint's own rim-slot sagitta | reject `split-bulge-depth` (premise check: the bulge is a near-tangency artifact; a deep crossing is real geometry the arrangement should have seen — never silently split it) |
| re-triangulation produces any invalid / degenerate sub-triangle | reject, NO mutation (build-then-commit) |
| q1/q2 UV fails the exact-collinearity self-check against B's model edge | reject `split-collinearity` (construction bug tripwire, P9) |

### 11e. Increments

- **inc-3.2a — detector + anatomy probes. ✅ COMPLETE 2026-07-30**
  (`fig11_split_chord`, `RelocOutcome::NonSimple.split_chord`,
  `[fold-split-reject]`, `i6-input-overuse` triangle enrichment; this
  session added `[fold-split-anatomy]` — exact in-frame UVs of the mint,
  chord, and class-boundary neighbors; §11a is its measured record).
  Two refuted non-inserting designs (delete / reverse) stripped per P9
  with i6 fwd/rev evidence (§10d inc-3.2).
- **inc-3.2b — the gated primitive (`YANG_S0_FIG11_SPLIT_ENABLE`).
  ✅ BUILT + MEASURED 2026-07-30.** `carve_star_cavity` extracted
  verbatim (shared walk+growth; 479 tests byte-green through the
  refactor); `fig11_split_cavity` in reloc.rs with every §11d guard.
  **Build correction, caught by i6:** the first cut built the material
  polygon THROUGH v (`[…, q_b, v, q_a]`), whose interior contains the
  bulge — a DOUBLE COVER (i6: fwd=2 edges at the mint) that still
  "converted" the corpus case. The §11c constrained-edge list was
  already right: the material closes along C's (q_b→q_a) sub-segment,
  v belongs to the BULGE alone, and the true count invariant is
  **cavity + 1** (the bulge is the only new cover). Post-fix: R0099
  single_case SUPPORTED_CORRECT, B-side seam-free, exactly the
  predicted A-leg T-junction remaining (6 unpaired edges: overlay
  chain [5, q_a, v, q_b, 13] vs lateral [5, v, 13]).
- **inc-3.2c — the A-leg. ✅ BUILT + MEASURED 2026-07-30.**
  `ExtraRimPoint` side-channel (owning sub-chord identified by exact
  endpoint equality; exact projection parameter for boundary order)
  consumed by `collect_ring_crossings`; the ladder fails the pair
  LOUDLY if any extra goes unconsumed (`split-extras-unconsumed` — the
  mixed-path / no-rim-crossing configurations can not silently
  T-junction). **R0099: SUPPORTED_CORRECT with i6-count ZERO** — the
  conversion is seam-free, not absorbed-by-luck. Chain fixture: the
  conversion oracle is GREEN under the env var; its original strict
  `v3 < v2` assertion was WRONG — op 3 is a MEASURE-ZERO cut (the
  36.31° unswept sector hugs the tube's side: uncovered when
  h < tan(36.31°)·ρ⊥ ≥ 4.58 > the tube's 1.95 height), so the truthful
  oracle is v3 in the same analytic annulus band as v2; the observed
  +0.069% is chord→circle mint volume RESTORED toward the analytic
  (the corpus meta's "decrease" is the op-type default; its evaluator
  is the weak `vol_r ≤ vol_a + tol` form). Fixed with the derivation
  in the fixture docs; quarantine reason updated to the amendment-14
  flip. Unit fixtures: the vert-9 miniature (commit + re-cut + extras
  + 2-incident mid-segment) and the §11d guard rows (bulge-depth,
  other-edge, own-chord — each no-mutation).
- **inc-3.2d — FLIPPED ALWAYS-ON 2026-07-30; the env var is removed.**
  Corpus gate-OFF byte-identical to canonical (0 category + 0 detail
  changes across 312 — all inc-3.2 machinery provably inert gated).
  Gate-ON: **the ONLY category change is R0099 ERROR →
  SUPPORTED_CORRECT — the conversion itself** — plus one C0048 ERROR
  detail shift. Zero CORRECT→ERROR ⇒ bar met ⇒ always-on. **New
  canonical: 260C/0W/48E/0T.** The chain conversion oracle is
  un-quarantined and the RED VertexOffSurface pin rewritten as the
  green completes-through-tripwire pin (per its own instruction).
  R0099 — the anti-showcase of the entire fold-gate series (0 commits,
  6 reverts at the §7 census) — is the amendment arc's headline
  conversion: wedge decomposition (12) armed the seeds, the merge arm +
  settle check (13) repaired three mints and protected the boundary
  order, and the vertex-inserting split (14) repaired the last.

### 11f. Non-goals

- No arrangement re-run, no exact re-classification — the split is a
  LOCAL cavity operation; classes outside the star are untouched.
- The single-crossing graze and >2-crossing forms stay rejected until a
  census names a customer (measure-first; the inc-3.1 "singleton
  non-goal" refutation is the cautionary precedent for guessing).
- No change to mint placement or to the merge arm; the split composes
  AFTER the merge attempt in the ladder (merge outranks split when both
  arm — cheaper op first, same §10c ordering).

## 12. inc-3 REGION-FORM PARITY RE-CENSUS (2026-07-30, post-amendment-14) — NO PROVEN CUSTOMER; DEMOTED to census-armed

**Method.** 312-case `single_case` sweep (the §7 driver-nulls-stderr
trap), 8 jobs, `YANG_SPLIT_PROBE=1 YANG_COPLANAR_PROBE=1`, 300s
in-child wall guard, binary at the amendment-14 flip (f1100cf8).
**312/312 completed, zero exit failures, ZERO verdict drift vs the
260C/0W/48E/0T canonical** — directly comparable. Counts are probe
EVENTS (gate passes re-attempt; one defect fires repeatedly).

### Ladder scoreboard, whole corpus

| arm | events | cases |
|---|---:|---:|
| per-vertex commits + region commits | (many) + 138 | — / 15 |
| merge commits | 25 | 6 |
| settle firings | 50 | 9 |
| split commits | **3** | 3 (R0099, C0048, F0064) |
| region-form rejects | 535 | 13 |
| split rejects | 158 | 12 |
| **fold-reverts (residual leak)** | **295** | **12** |

### The region-reject roster (the would-be inc-3 customers)

| case | verdict | region rejects | reverts | canonical wall |
|---|---|---:|---:|---|
| F0067 | UNSUPPORTED(coplanar) | 329 | 174 | typed M8 coplanar-input wall |
| F0072 | UNSUPPORTED(coplanar) | 51 | 22 | typed M8 coplanar-input wall |
| C0048 | ERROR | 42 | 36 | azimuth-merge rims mismatched (#144 family) |
| F0064 | ERROR | 25 | 17 | TessellationFailed(1800, planar tri collapsed at render precision) + non-2-manifold |
| R0059 | CORRECT | 25 | 14 | — (latent) |
| R0085 | ERROR | 21 | 10 | TessellationFailed(566, ring rejected by CDT) |
| R0050 | ERROR | 11 | 6 | Stage-4 relocation region invalid |
| R0021/R0026/R0051/R0072/R0063 | mixed | 3–6 each | 2–4 each | varied (R0026: Stage-3 AmbiguousCurve; R0051: SelfIntersectingBooleanOutput) |
| **R0099** | **CORRECT** | **5** | **0** | **converted — zero residual reverts** |

Reject reasons: `crossing edges ungrowable (region polygon not simple)`
270, `every folded class sub-region rejected` 219, `cavity polygon not
CCW` ≈ 44 (F0067's periodic seed clusters — one defect re-attempted per
pass), `region too small` 1.

### Verdict

1. **inc-3 has NO PROVEN customer.** The §7 census's only proven
   revert-caused ERROR (R0099) is converted; every remaining ERROR
   carrier's wall is named ELSEWHERE (azimuth-merge #144, two
   TessellationFailed classes, Stage-4 relocation, Stage-3
   AmbiguousCurve, SelfIntersectingBooleanOutput). Region-repair work
   here would be error-string-proximity guessing — the §7 finding-3
   lesson. **inc-3 is DEMOTED to census-armed-no-customer** (the inc-4
   n-ary precedent); re-arm only when an anchored investigation traces
   a wall INTO a region reject.
2. **The split found two customers beyond R0099**: F0064 (genuine —
   t_c 0.0895/0.0089, overshoot 6.96e-4) and C0048 (committed with
   t_edge ≈ 2.6e-15 — a femto-endpoint crossing, WATCH: the q lands
   within femto reach of the ring vertex; handled by the M-B
   degenerate machinery today, an endpoint window is NOT warranted
   without a failing customer). Both stay ERROR at their own walls.
3. **Split-reject census (the §11d guard classes, future §11
   extensions, census-armed):** `split-open-link` 74 (the
   boundary-vertex form — the largest named residual), 
   `split-crossing-count` 48 (single-graze / >2 forms),
   `split-class-pair` 18, `split-chord-not-boundary` 18 (2-incident
   chords — real triangles beyond, the larger re-cut op).
4. **The latent chord-lift class persists**: 5 CORRECT cases carry 25
   revert events (R0059 14, R0072 4, R0021 3, R0063 2, R0099 0) — the
   standing P10 watch list and the argument for continuing
   mesh-updating; today's arms repair 3 split-commits' + 25 merges'
   worth of mints the gate previously reverted.
5. The settle check fires corpus-wide (50 events, 9 cases) — the
   boundary-order invariant is doing standing work well beyond its
   R0059 birth case.

Next anchors, in evidence order: the F0064 wall (a COLLAPSED PLANAR
TRIANGLE at tessellation — nearest kin to Stage-0 emission, and the
split already fires in-chain there), the C0048 #144 azimuth-merge
family (named vehicle exists), and the split-open-link class (74
events). Raw logs: `/tmp/census3/` (ephemeral); this section is the
durable record.

## 13. F0064 ANCHOR INVESTIGATION (2026-07-30) — the wall traced INTO split-open-link; amendment-15 design frame

**Verdict up front: the §12 anchors #1 and #3 are the SAME defect.**
F0064's `TessellationFailed(1800, "planar triangle collapsed at render
precision")` is a proven, loud customer of the **split-open-link**
class (§12 finding 3, 74 events) — the §11 split's boundary-vertex
form, deliberately guarded off at `reloc.rs:1362`. Per §12 verdict 1's
re-arming rule (an anchored investigation tracing a wall INTO a
census-armed class), the open-link split is RE-ARMED as
**amendment-15**.

### 13a. The case, decoded

F0064 = 5 stacked coplanar extrudes. Poly1 (op 1) is a **PLUS/CROSS
12-gon** (arm half-width `w = 0.05656…`, reach `0.27571…`); op 2
stacks a circle `r = 0.16910…` whose disc SPILLS sideways past all
four arm edges (`w < r`). Both failures are rim×arm-edge junction
sites on a coplanar interface:

- **Extrude 3** (circle onto plus, interface z = 0.4771…, Stage-0 pair
  `(590,0)`): 9 fold-reverts; output dies at kernel-v2's G1 gate
  (`tessellate/mod.rs:1025`) on the AOnly cap fragment.
- **Extrude 4** (square poly2 onto the circle body, z = 0.7339…, pair
  `(1,1)`): 18 fold-reverts (+1 split commit vert 37, +1 merge commit
  62→59); dies `non-2-manifold` — same class, second interface
  (`[s4-exact-junction]` shows v=59/64/69/70 `exact=false` on poly2's
  y = −0.16210 edge lines).

### 13b. The mechanism — three generations of one crossing, two
authorities disagreeing

On the −x arm's y = −0.05656 edge (mirror on +x), the log carries the
same geometric point in three generations:

| generation | position (x on the arm edge) | who | state |
|---|---|---|---|
| chord world | −0.154147 (arr. v=1198; mirror v=1219) | Stage-1 13-gon × edge crossing | `exact=false`, cylinder residual −4.9e-3 = the n_seg=13 sagitta |
| stale station | −0.158111 (overlay vert 8; mirror 66) | a B-mesh-edge × arm-edge crossing, r=0.16792 BETWEEN chord and rim | AOnly, 1-incident, survives to the output loop |
| true junction | −0.159361 (4-surface junction) | rim × arm-edge | `exact=true` where Stage-0 committed |

Causal chain, all sites named:

1. **Stage 0** mints the chord→rim upgrade for the crossing (vert 12
   slides ALONG the arm edge, −0.154147 → −0.159361, exactly
   collinear with its own chord `(2,5)`). The move passes OVER the
   stale station vert 8 → local folds. Ladder: wedge/region reject on
   the collinear-overlap ring (`o=(0,0,0,0)`, "crossing edges
   ungrowable"); merge p=8→q=12 rejected by §10d containment
   (overshoot 1.248e-3 ≫ sagitta 9.86e-5 — CORRECT, they are distinct
   points); split rejected `split-open-link` (`reloc.rs:1362` — q=12
   is a BOUNDARY vertex, the unarmed class). Amendment-2 REVERTS.
   Settled overlay = coherent chord world. **The gate's verdict:
   "this upgrade cannot be absorbed without re-cutting the
   neighborhood."**
2. **Stage 4** (`stage4_correct.rs:5568`, the #146 circle×pp-line
   junction loop, spec `yang_stage4_circle_pp_line_junction` branches
   4–6): v=1198 fails the §exactness certificate (4 distinct surfaces,
   inexact on the cylinder), lands in `vert_pp_circle_junction`, and
   is reseated onto the exact line∩circle junction via
   `pp_line_circle_junction` + `project_onto_circle` (trig round-trip
   → the output's 1–2-ULP variant of Stage-0's refused mint position).
   **Stage 4 REDOES the exact move Stage 0 reverted** — correctly per
   its own contract, with a derived corridor gate — but with no fold
   gate, no boundary-order check, and no §4.4.1 mesh update: vert 8
   (now strictly INSIDE the disc) keeps its AOnly loop membership.
3. **Stage 5/6** emits vertices 1:1 (`stage5_topology.rs:772`). The
   output face loop walks corner → vert 8 (−0.158111) → junction
   (−0.159361): a **backtracking needle 3e-17 thin** along the arm
   edge. kernel-v2's always-on G1 render gate fails the face LOUDLY.
   (§4.4.1 rim-snap moved 0 verts; inc-5 triple bails at 4 surfaces;
   P3a junction minting skips on the Stage-0 path — all measured,
   `[s4-rim-snap] moved=0`.)

### 13c. Paper grounding — this IS Fig. 11's primary case

Yang §4.4.1 (`refs/text/yang2025_hybrid_boolean.txt:545-575`): "point
q is an intersection point **on the boundary curve** … (a) We locate
the constrained edge containing q (the red edge) and **split it using
q**. (b) If an endpoint p of the split edge is too close to q, **we
merge p with q** … To improve remeshing quality, **we remove a mesh
vertex if it is too close to the intersection curve** on the mesh."
The open-link split = Fig-11(a) applied to a DOMAIN-BOUNDARY vertex;
the stale-station absorption (verts 5/8) = the paper's
remove-too-close rule. Nothing here is invented mechanism.

### 13d. The end-state proof is inside the same case

Of the 8 rim×arm-edge junctions at z = 0.4771, **six committed at
Stage 0** and flowed through every stage as certified-exact 4-surface
junctions (`[s4-exact-junction] … exact=true`, e.g. v=1195 =
bit-identical output loop vertex), needle-free. The two that FOLDED
(and reverted) are the two that died. The committed-mint path is the
proven template; amendment-15 extends it to the open-link form.

### 13e. Standing P10 exposure (recorded, not actioned)

Two relocation authorities now demonstrably disagree on refused work:
every Stage-0 fold-REVERTED rim junction whose arrangement vertex is
claimed by a cross curve will be re-upgraded by the Stage-4 #146
reseat without the mesh update — a latent needle per revert (§12
scoreboard: 295 revert events / 12 cases). F0064 is the class's loud
witness; in a case where CDT dodges the collinear triple the needle
ships SILENTLY (sub-f32 sliver on a CORRECT verdict). The general
closure is #169's Stage-4-side "remove a mesh vertex too close to the
intersection curve" (the paper's own sentence); amendment-15 shrinks
the revert population at the designed site first. Do NOT "fix" this
by suppressing the Stage-4 reseat — the reseat is paper-correct; the
missing piece is the update, not the move.

### 13f. Amendment-15 — the OPEN-LINK split arm (design frame)

Scope: the §11 vertex-inserting split, extended to split candidates
whose q is a DOMAIN-BOUNDARY (open-link) vertex of the overlay — the
`reloc.rs:1362` reject class. Form observed in F0064 (both
interfaces): **q's mint target lies ON its own chord C** (exactly
collinear, a 1D slide along the host model edge), with 1–2 stale
stations between the chord position and the target. Design
obligations beyond the §11c closed-link op:

1. **Half-star cavity.** q's link is open (q sits on the face
   boundary); the re-cut region is bounded by the domain boundary
   itself. The §11c star walk must terminate at the two boundary
   edges instead of closing.
2. **Stale-station absorption** (the genuinely new part): stations
   passed over by the slide (F0064: verts 5, 8 — between old and new
   crossing on the host line) flip from own-only to Overlap side.
   Per Fig-11(b)/remove-too-close, absorb them into the re-cut (merge
   into q or re-classify their wedge) — LOUD pair-fail if any station
   is not consumed (the §11c unconsumed-extras posture).
3. **Boundary-order invariant at commit**: post-split, the host
   line's station sequence must be strictly monotone in the line
   parameter (the amendment-13 settle predicate, applied on the
   model-edge chain at the commit site). This is the needle's direct
   negation.
4. Guards kept from §11d: exactly-collinear chord (already exact
   here), class-pair, own-chord-exists, one-live-commit,
   build-then-commit unwind. Crossing-count re-derived for the
   half-star (the closed-link "exactly 2 proper crossings" becomes
   "exactly 1 interior crossing + the 2 boundary terminations" —
   measure in inc-0 before fixing the count).
5. **Acceptance**: F0064 Extrude-3 converts (G1 needle gone,
   `single_case` SUPPORTED_CORRECT or the next honest wall);
   Extrude-4's non-2-manifold re-measured (same class, second
   customer); corpus gate-OFF byte-identical; gate-ON zero
   CORRECT→ERROR (the §12 latent watch list is the regression
   canary); i6 fwd/rev clean at every commit site.

inc-0 (anatomy census over the 74 open-link events) runs first: how
many are the F0064 1D-slide form (mint exactly-collinear with C) vs
the perpendicular-bulge form (R0099-like but at a boundary vertex) —
the two forms need different cavity re-cuts, and the census decides
whether the bulge form is deferred to its own increment. Census
results append here as §13g.

### 13g. inc-0 anatomy census (2026-07-30) — the SLIDE form is the customer-bearing form; v1 scope = pure slides

**Method.** 312-case `single_case` sweep (§7 stderr trap respected),
8 jobs, `YANG_SPLIT_PROBE=1 YANG_COPLANAR_PROBE=1`, 300s in-child
guard, binary at f1b70016 (code identical to the f1100cf8 canonical).
**312/312 completed, ZERO verdict drift (260C/0W/48E/0T)** — directly
comparable. Per open-link event, the `[fold-split-anatomy]` block
yields: perpendicular offset of the MINT from chord C's supporting
line (`perp/c`), the same for the PRE-mint position (`pre_perp/c`),
and the mint's line parameter `t` on C.

**Form classification, CUSTOMERS ONLY** (F0067/F0072 excluded — their
41 events sit behind the typed UNSUPPORTED(coplanar) wall):

| form | definition | events / cases | verdicts |
|---|---|---|---|
| **pure SLIDE** | mint AND pre-position exactly on C (perp = 0 to f64), t strictly interior | **3 / 1 (F0064: q=63 t=0.0895, q=12 t=0.9105, q=22 t=0.8912)** | ERROR — **the anchor's needle sites themselves** |
| endpoint-coincident | mint exactly at a C endpoint (t = 0.0000 / 1.0000 ± ulp) | 6 / 1 (C0048) | ERROR (wall named elsewhere: #144 azimuth-merge; the §12 femto-endpoint WATCH family) |
| boundary bulge | pre on/near C, mint perpendicular off it (perp/c 2e-3…6.5e-2), t interior | 5 / 5 (F0064 q=20+q=29, R0021, R0026, R0050, R0059×2) | 2 latent-CORRECT, 3 ERROR with walls named elsewhere |
| non-host chord | t far outside (0,1) or perp/c ≳ 1 (C is not the host segment) | 4 / 2 (R0085×3, C0048 q=286) | ERROR, walls named elsewhere (CDT-ring / #144) |

**Verdict.**

1. **Amendment-15 v1 scope = the pure-slide form ONLY**: mint
   exactly-collinear with C (exact-rational test, both mint and
   pre-position), t strictly interior. Its full customer population
   is F0064's two interfaces — including both Extrude-3 needle
   mirrors (q=12/63) AND the Extrude-4 square-edge slide (q=22).
   Every non-slide form's carrier has its wall named elsewhere or is
   latent-CORRECT; arming them now would be error-string-proximity
   work (§7 finding-3).
2. The 1D slide needs NO 2D cavity re-cut design: the op is a
   STATION-ORDER repair along one host line — insert q at its exact
   position on C, then absorb the passed-over stations per Fig-11(b)
   / remove-too-close (the §13f obligations 2–3 collapse to the same
   1D monotonicity invariant). The half-star machinery (§13f-1) is
   only needed by the BULGE form — deferred, census-armed, with
   R0021/R0059's latent events as its future witnesses.
3. Extrude-4's q=20/q=29 (bulge/near-slide at the same interface) may
   still block its conversion after the slide arm ships — acceptance
   (§13f-5) treats Extrude-4 as re-measure, not promise.
4. C0048's endpoint-coincident family stays with the §12 femto WATCH
   (merge-at-endpoint territory, not a split).

Raw logs: scratchpad `census_ol/` (ephemeral); this table is the
durable record.

### 13h. Amendment-15 inc-1/inc-2 (2026-07-31) — the pure-SLIDE splice BUILT and FLIPPED ALWAYS-ON; F0064 ERROR → UNSUPPORTED(coplanar); new canonical 260C/0W/47E/0T

**The op as shipped** (`fig11_slide_splice`, `stage0/reloc.rs`; wired
inside `fig11_split_cavity`'s open-link branch): purely combinatorial —
NO vertex minted, NO coordinate moved. The side wedge's ring drops the
collinear tail (C's near endpoint + stations), closes `c_k → v`, and
re-ear-clips; the dropped tail re-embeds into the far collinear spoke
`(v, w*)` by splitting both flank fans (the exact T-junctions the slide
swept). Certificates, all exact-rational over the same frame
projections the ring tests use, ANY failure → the amendment-2 revert:
the §11c chord certs (1-incident, other-input real edge); v's PRE UV on
that model line AND v's mint exactly interior to C (the §13g slide
signature); exactly ONE chain end reachable through on-line side-class
edges; tail unminted, fully carved, strictly inside the `(v, w*)` span;
`w*` unique and 2-incident-carved; every rebuilt triangle exact-CCW and
gate-valid; count = cavity + |tail|; **per-class SIGNED area conserved
as an EXACT rational equality** (the folded cover's signed sum already
equals the clean cover's — any mis-fan breaks it). Unit oracles: the
F0064 vert-12 miniature (commit, 6→8, station handoff to
Overlap|BOnly), the vert-63 mirror (start-side tail), and four
no-mutation reject rows (`slide_tests`, yang-rs lib).

**inc-1.5 finding — the settle × slide fight** (measured before the
guard): a committed slide leaves v MINTED at a leaped position, so the
§10d `settle_rim_chain_order` check reverts it at quiescence
(`minted_mark[vi] && coords[vi] != lift`, rim_chords.rs) — UNDOING the
splice's structural rewiring while the topology stays spliced; the
interface meshes desync and cherchi defers
(`CoplanarPairDeferred`, measured F0064 ops[3] vert 22). Fix shipped:
the settle predicate applied PREVENTIVELY as a slide commit
certificate — gather v's OWN rim sub-chord's crossing set (the settle's
collection, verbatim) and reject when any adjacent pair's exact angular
order would invert with v at its mint
(`slide: would invert rim-chain angular order`). WATCH (not actioned):
the same fight can hit the §11c CLOSED-link split — the new F0064
ops[3] state shows the settle reverting a §11c-committed mint's
neighborhood (vert-37-class events); if a §11c customer ever measures a
settle-undone split, the same preventive cert belongs there.

**§13b ERRATUM (op attribution).** Assay failure labels are 0-BASED
("Extrude 3" = ops[3], the second polygon). The fold-reverts at pair
(590,0) fire in ops[2]'s union (the circle onto the plus), which
SUCCEEDS and ships the settled chord world in its OUTPUT B-Rep; the
needle is then created in ops[3]'s pipeline when its Stage-4 #146
reseat upgrades A's carried-in latent chord junction (v=1198-class)
past the stale station. The two disagreeing authorities live in
DIFFERENT boolean invocations — the §13e exposure is therefore
STRONGER than §13b stated: a fold-revert's chord junction survives a
full B-Rep round-trip and detonates one op later (the census's latent
chord-lift watch class, mechanized).

**Corpus (312/312, sweep at the gated build; then flipped):** exactly
ONE category change — **F0064 ERROR → UNSUPPORTED(coplanar-boolean)**;
ZERO same-category detail drift; R0059/R0099/C0048/R0085 spot-verified
byte-stable. With both ops[2] slides committed, ops[2]'s output carries
exact 4-surface junctions (Stage-4 certifies, no reseat, no needle) and
ops[3] proceeds past the old G1 wall into cherchi's N17 typed deferral
— a REAL-overlap coplanar pair the Stage-0 overlay does not yet
pre-handle (the F0067/F0072 wall class). The defect is gone; the
residual is a NAMED M8 capability boundary. Per §13f-5 acceptance:
"the next honest wall". **New canonical: 260C/0W/47E/0T** (UNSUPPORTED
+1). The stale `(vertex-inserting split not built — inc-3.2)` probe
annotation is retired to `(no split arm accepted)`.

Residuals carried: the bulge form (§13g, census-armed; R0021/R0059
latents as witnesses), the settle×closed-link-split WATCH above, and
F0064's own next wall (ops[3] N17 deferral — an M8 Stage-0 coverage
item, not a fold-gate item).

### 13i. inc-3 (2026-07-31) — the order cert extended to the CLOSED-link split; the strand class had TWO customers hiding; §13h deferral attribution corrected

**The change.** The §13h slide guard's angular-order check is extracted
as `chord_crossing_order_inverted` (reloc.rs) and now runs in BOTH
arms: the slide (inc-1.5, as before) and the §11c closed-link split —
inserted after the own-chord lookup, BEFORE q_a/q_b are minted, so a
reject leaves nothing to unwind
(`split-order-inversion (settle would strand the re-cut)`).
**Focus-narrowed** (measured necessity): only adjacent pairs INVOLVING
the committing vertex count — the whole-set form falsely rejected
C0048's femto split for an inversion among two OTHER in-flight mints,
which is the settle's own normal business (revert the displaced
member), not this vertex's veto. Unit oracle:
`split_rejects_settle_order_inversion_no_mutation` (the vert-37
configuration in the §11 fixture frame).

**The strand class had TWO measured customers, one hiding since
amendment-14 shipped:**

1. F0064 ops[3] vert 37 (the §13h WATCH) — split committed, settle
   reverted it same-pass.
2. **Canonical C0048 vert 265** — the inc-0 census log shows
   `[fold-split] vert 265` (t_c 0.6875 / 2.6e-15, the §12 femto
   commit) at line 553 and `[rim-order-settle] vert 265 → chord` at
   line 796: the split the census recorded as C0048's commit was
   STRANDED at quiescence all along. Its canonical azimuth-merge wall
   counts (`69 vs 68`) were measured on the stranded state; the
   coherent state reads `68 vs 67` — same wall (#144 family), same
   category, one fewer sample on each rim.

**§13h attribution CORRECTION.** With BOTH strand sources guarded (a
fully coherent ladder), F0064 ops[3]'s cherchi `CoplanarPairDeferred`
PERSISTS — so the deferral was never (only) a strand artifact: it is a
GENUINE N17 real-overlap coplanar pair the Stage-0 overlay does not
pre-handle — the true M8 coverage residual. The strands were real
incoherences worth eliminating (each was a latent
non-identical-interface hazard), but the typed wall stands on its own.

**Corpus (312/312, guard always-on):** ZERO category deltas
(260C/0W/47E/0T unchanged); exactly ONE same-category detail drift —
C0048's wall counts `69 vs 68 → 68 vs 67`, the strand-removal above.
R0099 re-verified SUPPORTED_CORRECT (its split has no inversion and
still commits); R0059 stays CORRECT.

Residuals: unchanged from §13h (bulge form census-armed, C0048 femto
WATCH — now with the sharper statement that its split coherently
REJECTS rather than committing-then-stranding — and F0064's genuine
N17 coverage wall).
