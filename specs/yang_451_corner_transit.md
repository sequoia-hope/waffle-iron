# §4.5.1 Corner Transit — the §4-I9 corner-crosser repair (I13f rehome kin)

**Status: inc-2a PLANNER + inc-2b CORRIDOR-WALK CENSUS LANDED + MEASURED
family-wide, 2026-08-30 — `stage4_transit.rs` owns the inc-1 instrument
(23/23 verdicts: 12 TRANSIT + 5 CLIP + 6 typed declines; mints deduped by
POSITION identity); the anatomy census REFUTED the "truncate at q" sketch
— the repair unit is the fan-walking CORRIDOR (§3d), and the walk census
MEASURED every corridor (§3e): R0011/R0074 fully determined (the v42→v78
merge bit-equal), R0044 determined up to two named refinements, R0085
walled on its own operand quality. inc-2c-0/-1 (same day) landed the
all-roots per-edge solvers + the v2 walk (margin guard, splice
terminal, torus Newton fallback): v76 resolved, every family corridor
DETERMINED (§3f/§3g). NEXT = inc-2c-2: the gated APPLY (phantom splits,
mints, splices, re-fill) against fully-measured corridors.** Epic opened by
the §4.5.2 adjudication (`specs/yang_452_local_refinement.md`): the recovery
loop the paper prescribes for this class was measured out (zero conversions at
2×/4×/8× uniform density), so the class needs the repair the paper does NOT
describe — deterministic transit across a model corner, made possible by the
analytic constraint sets our B-Rep-extended pipeline carries
(`feedback_yang_brep_extension_over_cherchi_pure_mesh`).

Cases: R0011, R0044, R0074, R0085 (4 of the 34 canonical ERRORs; the
`RelocationCrossedCarrierVertex` family, §4-I9's fire list).

## 1. The defect (measured; §4-I8/I9/I10 + the 2026-08-29 probes)

One uniform configuration across every fired site
(`YANG_S4_CARRIER_DOMAIN=census`):

- The traveller is a mesh intersection-curve endpoint carrying
  {far, base, facet_k} — it sits where the chord-resolution mesh curve
  crosses the model edge base∩facet_k of a many-facet operand (R0011:
  giant-cylinder base B:1 r≈6277 + planar facets, far = A:2; R0044:
  cone-band facets, far cone; R0074: operands swapped, scale ~1e-1).
- The relocation solves the exact triple {far, base, facet_k} — and that
  junction EXISTS but is a PHANTOM: it lies past the still model corner
  q = base∩facet_k∩facet_j (post is ON all three surfaces to ≤3e-14 at
  scale 5e3, and outside facet_k's actual extent). The §4-I9 postcondition
  catches exactly this (q strictly interior to the pre→post segment,
  off-travel ≤6e-13; overrun 0.3–3.6 % of the exact-zero extrapolation,
  §4-I10 (f)).
- The corner q ALREADY EXISTS as a mesh vertex (`TruncateAtVertex`
  t = 0.14–0.32 names it). Nothing needs minting at the corner itself.
- The TRUE junction is {far, base, facet_j} on the adjacent facet — the
  exact crossing of the curve far∩base with the next facet's surface.

Why neither existing vehicle claims it today (measured 2026-08-29):
- `YANG_441_REHOME=census` prints NOTHING on R0011 — the rehome census hooks
  the I13d selector's `not_richer` branch, and this flow never enters the
  I13d selector; it dies at the §4-I9 stage-end postcondition.
- §4.5.1 proper (Fig-12) is selector-excluded: the traveller is a BOUNDARY
  glider (on two surfaces of one operand at both step ends), Fig-13's
  exclusion, §4-I10 (f) 24/24.
- §4.5.2 refinement: adjudicated out by direct measurement (the crossing is
  binary in the overrun sign — any nonzero overrun at any density fires).

## 2. inc-0 — corrected-junction feasibility census (report-only, LANDED in
the §4-I9 census branch)

`YANG_S4_CARRIER_DOMAIN-TRANSIT` lines, printed per fired site in census
mode: split pv/pq into far = pv\pq, next = pq\pv, shared = pv∩pq; solve BOTH
candidate corrected triples {far, shared_i, next} with
`relocate_onto_implicit_triple` seeded at q; report convergence, distance
from q and from the phantom post, the overrun, and
`planar_partner_hull_contains` for the next facet (planes only; None = no
verdict) at the Stage-1 chord band.

Measured so far:

- **R0011 — 4/4 sites: the {far, base, facet_j} triple CONVERGES, next-facet
  hull Some(true).** v78 reads like a textbook figure: true junction 7.30
  past the corner vs phantom overrun 7.37, solution 0.43 from the phantom.
  v27: d_from_q = 115.2 (overrun 257.6) — the phantom badly overshoots while
  the true junction is mid-facet. The second candidate {far, facet_k,
  facet_j} ALSO converges in-hull at these sites, so hull membership alone
  cannot discriminate the continuing edge — the discriminator must be the
  next EDGE's own segment domain (inc-1).
- **R0074 — 1/1: same shape at scale 1e-4** (far B:2, base A:0, next A:163;
  d_from_q = 5.9e-4 ≈ overrun 5.2e-4; phantom only 7.4e-5 from the true
  junction; hull Some(true)).
- **R0044 — 7 sites, every candidate CONVERGES; hull None (cone facets, the
  planes-only check has no verdict — the cone band's axial-extent reading is
  an inc-1 item).** Four sites (v75/v76/v89/v105) read clean: {far, base,
  facet_j} at d_from_q ≈ overrun, phantom-adjacent (d_from_post 5.3, 1.5,
  0.75, 0.16). v8 is anomalous — neither candidate lands near the phantom
  (d_from_q 51.8/17.5 vs overrun 14.6): the first site the inc-1
  discriminator must adjudicate rather than pattern-match. **v142/v144 are a
  MIRRORED PAIR at the SAME corner q=v513** — each names the other's facet
  as `next`, each one's overrun equals the other's candidate d_from_q, and
  one solution point is shared bit-equal between their candidate lists: the
  §I13(f) "two W↔K-mirrored views of one defect" anatomy, verbatim.
- **R0085 — 11 sites, every candidate CONVERGES, hull Some(true) (planar
  facets).** Two presentations at shared corners: base∩facet riders (v387 at
  q=v388, v467 at q=v468) AND crease riders on facet∩facet edges whose
  corrected triple re-introduces the base (v4165 at the SAME q=v388, v6071
  at the SAME q=v468, v4359 with v401 at q=v402). The crease riders form a
  WALKING CHAIN — v4169→v4174→v4180→v4197→v4216 cross consecutive crease
  corners (A:228/229 → … → A:232/233) with adjacent candidate solutions
  BIT-EQUAL (each site's far-side solution IS the next site's near-side
  one): the mesh curve crosses a facet fan, and every vertex's phantom
  overshoots its own corner by one facet.

**inc-0 VERDICT (2026-08-29): feasibility CONFIRMED family-wide — 46/46
candidate triples at 23/23 sites across all four cases converge; in-domain
Some(true) at every site where the planar-hull verdict applies.** The repair
is well-posed everywhere; what remains is discrimination, not solvability.
inc-1 therefore owns three measured structures: (a) continuing-edge choice
via the model edge's segment domain (both candidates converge in-hull at
planar sites — hull membership cannot discriminate); (b) view dedup at
shared corners (mirrored pairs + crease/base rider pairs name the same
corner; the I13f AmbiguousViews lesson applies — pair-local order/side
tests were REFUTED there, so design for a corner-local shared plan, not
per-view discrimination); (c) the R0044 v8 anomaly and the cone-band
in-domain reading.

## 3. The repair shape (design sketch — inc-1+ will firm this against
measurement; the junction contract of
`docs/yang_junction_research_findings.md` is binding: mint once exactly,
share by identity, multiplicity is a loud STOP)

At the §4-I9 detection (or pre-emptively at the (2s)/(2t) relocation arms):
1. Recognize: relocation target is an exact triple {far, base, facet_k}
   whose path crosses still corner q carrying {base, facet_k, facet_j}.
2. Discriminate at q by the corner-incident-edge rule (inc-1, MEASURED —
   see §3b; the pre-measurement "accept {far, base, facet_j}" presumption
   was refuted): a candidate junction is REAL iff it lies within band ON a
   model edge with an endpoint at q, inside that edge's segment/arc domain.
   Sites classify as 1-real (single transit — crease OR base side, both
   occur) or 2-real (corner clip: two junctions + the connecting segment
   across the adjacent face). No real candidate, or an unresolvable
   carrier-authority read → typed decline (AmbiguousTransit — the honest
   gated fixed point, I13f-f2b precedent).
3. Apply: truncate the traveller's step at q (share the EXISTING corner
   vertex — no mint), relocate the traveller to the corrected junction,
   and re-route the curve/patch structure so facet_k's chain ends at q and
   facet_j gains [q → newJ] — the I13f f2c/f2c-2 re-route + re-fill
   vocabulary (rebuild fans, two-sided attributed CDT) is the apply-arm
   toolbox.
4. Postconditions: the §4-I9 check must pass on the repaired site; the
   apply must be χ/component-neutral per the I13f audit discipline; full
   oracle gate stays (P10).

Paper position: Yang §4.5 (Fig-13) declines to cross corners because the
mesh-only pipeline cannot predict the direction past s; our analytic
constraint sets make the transit deterministic and certificated. This is a
junction-layer EXTENSION — when it flips always-on, sign it into
`docs/yang_deviations.md` per the compliance-endgame plan (N2 remit).

## 3b. inc-1 — the continuing-edge discriminator (census arm LANDED
2026-08-29, same session; measurements below)

The census arm gains, per candidate, an EDGE-DOMAIN report: over the union
of both candidate faces' loop edges, the edge nearest the solution (by
clamped chord projection), its owner tag (S = shared_i's loops, N = next's,
SN = converter-shared curved), the q-endpoint residual, and the parameter
verdict (LineSegment exact; Circle arcs read via `circle_frame` wrapped
angles both orientations). Two structural facts surfaced immediately:

- **Face pairs share no edge INDICES for straight edges** — `to_yang_brep`
  emits one directed yang edge PER HALF-EDGE for LineSegments (the m1
  per-loop-copy convention); only curved edges (Arc/Circle twins) are
  shared. Any index-intersection adjacency test finds nothing on facet
  chains; adjacency must be read geometrically (or via twin-position
  matching).
- **`off_line` against a LineSegment is chord deviation, not domain
  exclusion**, when the B-Rep edge is a chord over a curved carrier. The
  discriminator is EXACT only where the true carrier is straight.

**Measured verdicts.** The inc-0 "clean pattern = {far, base, facet_j}"
reading is REFUTED by the edge data (the d_from_q ≈ overrun signature was a
coincidence of the fan geometry — hypotheses hardened into pattern-matching
is exactly the failure mode the I13f epic warned about):

- **R0011 (4 sites, all straight carriers — axial prism-skirt facets on the
  giant cylinder): the rule works.** A candidate is REAL iff its solution
  lies within band ON a corner-incident model edge (q_end = 0) in-segment.
  v27/v37/v42: exactly ONE real candidate — the CREASE triple
  {F, facet_k, facet_j}, exact (off ≤ 2.3e-13) at t = 0.009–0.017 just past
  the corner; the base-side junction lies on NO corner-incident edge
  (nearest q_end 90–199). **Clean crease transit.** v78: TWO real
  candidates, both exact on corner-incident edges (crease t = 0.0008, base
  edge t = 0.11) — the far plane passes so close to the corner that the
  exact curve CLIPS it, running a short segment across the adjacent facet
  between two true junctions. **The repair vocabulary is therefore
  per-site: 1-real → single transit junction; 2-real → mint the two-junction
  corner clip + the connecting segment (Yang Fig-13(c)'s error, fixed by
  minting the exact clip topology).**
- **R0074 (1 site): the corner-clip class again** — both candidates exact
  on corner-incident straight edges (base-side t = 0.257, crease t = 0.0027).
- **R0044 (7 sites, gear-profile partial revolve — cone bands with SHARED
  Circle rim edges): fully classified once the ranking metric became
  curve-aware.** The first pass ranked every loop edge by clamped CHORD
  distance, which is biased against arcs (a rim's chord sits far from a
  point exactly ON the arc), so straight cap/meridian edges won spuriously
  and the sites looked "carrier-authority blocked" (off_line 3–90 ≈ what
  was actually the sol-to-unrelated-chord distance). With Circle edges
  ranked by their true circle distance √(axial² + (radial−r)²), every
  crease candidate finds the converter-shared rim (own=SN, q_end = 0) and
  the wrapped-arc test reads a consistent orientation (in_ccw uniformly;
  span_ccw = 5.3155 rad = the meta's 304.56° revolve). Classification:
  v75/v76 = crease transits (rim junction 71.6/45.8 along the arc from the
  corner; base candidate NOT corner-incident); v8/v89/v105 = CORNER CLIPS
  (base edge exact in-segment AND rim in-arc); **v142/v144 (the mirrored
  pair) land on the SAME rim edge 2025 at the SAME arc parameter
  (sol_ccw = 0.0761) — one mint, two views, the I13f anatomy verbatim; the
  base candidates at both views are off-edge (41.5/90.5), so the pair is a
  single crease transit, deduplicated by the shared mint.** No
  carrier-authority wall exists on this case; the operand's rims are exact
  shared Circle edges. (Lesson for the record: a census RANKING metric can
  manufacture a phantom capability wall — make the instrument curve-aware
  before concluding about the data.)
- **R0085 (11 sites, planar facets at scale ~3): the rule's EXCLUSION arm
  is validated, and the operand's own lattice quality surfaces.** v467 and
  v4216 are clean BASE transits (exact on corner-incident edges, t = 0.26 /
  0.35, off ≤ 5.6e-17) with the crease candidates excluded while lying
  exactly ON the crease's line but PAST its far endpoint (t = 1.0170 and
  1.0013, off ≤ 5.9e-16, in_segment = false) — the walking-chain adjacency
  measured: each crease junction lands at/past the NEXT corner in the fan.
  The crease-rider sites' nearest base edges carry small NONZERO residuals
  (q_end 0.01–0.03, off 2e-4–5e-3 at scale ~3, several different sites
  landing on the SAME long base edge at increasing t) — the base face's
  loop and the facet chain disagree at authoring scale: an operand
  edge-lattice INCONSISTENCY (two sides of one boundary), consistent with
  R0085's separate op-3 `input B-Rep is not 2-manifold` wall. For those
  sites the transit repair is downstream of operand quality — record, don't
  force. *Re-measured under the curve-aware ranking (inc-1b): byte-identical
  — R0085's loop edges are all straight, so the arc-ranking artifact is
  excluded and these residuals are REAL. The base loop passes exactly
  through SOME facet corners (v4216 q_end = 0, off = 0) and misses others
  by 0.01–0.03: mixed conformality of the operand's own boundary.*

## 3c. inc-2a — the pure planner (LANDED 2026-08-30, verdicts measured
family-wide)

`crates/yang-rs/src/stage4_transit.rs`: `read_site` (anatomy split far/next/
shared with typed `AnatomyMismatch` off the measured (1,1,2) shape; both
triple solves seeded at q; the inc-1 edge-domain instrument extracted as a
structured read — curve-aware ranking, owner tag, corner-endpoint residual,
segment/arc domain verdict) + `classify` (the corner-incident-edge rule:
1-real → `Transit`, 2-real → `Clip`, else `NoRealCandidate` /
`JunctionAtCorner`). The §4-I9 census branch is now planner-driven (ONE
instrument for census and the future apply arm), prints a `PLAN` verdict per
site, the site ANATOMY (§3d), and position-keyed `MINTGROUP` lines. Unit
tests: 7 fixtures (crease/base/clip/no-real/at-corner/anatomy/arc-domain).

Measured verdicts (debug census, natural density, 2026-08-30):

- **R0011**: v27/v37/v42 `Transit{crease}` + v78 `Clip` — 4/4 ≡ inc-1.
- **R0074**: v129 `Clip` — 1/1 ≡ inc-1.
- **R0044**: v75/v76 `Transit{crease}` (rim junction: own=SN, in_ccw,
  d_on_circle 6.3e-12 ≤ the 1.2e-11 eval band at scale 3.5e3 — the
  exactness band VALIDATED on arcs, live) + v8/v89/v105 `Clip` +
  v142/v144 `Transit` auto-grouped: `MINTGROUP edge 2025, spread 0.0` —
  one rim mint, two views, instrument-confirmed.
- **R0085**: v467+v4216 `Transit{base}` ≡ inc-1; the planner REFINES the
  inc-1 rider reading — v6071, v401, v4359 are `Transit` with EXACT real
  junctions (≤5.6e-17): v401/v4359 auto-grouped (spread 0.0), and
  **v467/v6071 solve the SAME junction bit-equal but name DIFFERENT edge
  indices (351 vs 2520) at parameters t and 1−t — the two m1 per-loop
  COPIES of one physical edge, antiparallel.** Junction identity is
  therefore POSITION on an operand, never an edge index (the MINTGROUP
  key was fixed accordingly). Only the six walking-chain riders (v387,
  v4165, v4169, v4174, v4180, v4197) decline `NoRealCandidate` — their
  nearest-edge residuals (2.8e-4 … 2.2e-2) are the operand's own
  mixed-conformal boundary (inc-1's finding stands: record, don't force).

Tally: 12 Transit + 5 Clip + 6 declines = 23/23. Physical junction count
after view dedup: R0085's five transit views name THREE junctions.

## 3d. The measured apply anatomy — the repair unit is the CORRIDOR
(anatomy census 2026-08-30; this SUPERSEDES the §3 sketch's "truncate the
step at q / share the corner vertex" — measured wrong in general)

Per-site `-ANAT` lines (curve-neighbour patch sets, per-attribution fans,
the v–q wedge triangles) establish, uniformly across R0011/R0074/R0085:

1. **Two mesh curve chains arrive at every fired traveller** — a
   far∩shared_a chain and a far∩shared_b chain (plain 2-patch curve
   vertices: v26, v75, v80; or an adjacent JUNCTION vertex where the
   curve crossed the previous lattice edge: v20 {A2,B211,B212}, v43
   {A2,B183,B184}, v1402). The phantom FUSED the two chains' true
   endpoints into one vertex.
2. **The true local structure is two junctions + a connecting run across
   `next`**, not one relocated vertex:
   - CLIP (v78, v129, v8/v89/v105): both endpoints are this site's two
     real candidates, corner-incident (v78: base edge t=0.11 + crease
     t=0.0008); the run crosses `next` beside the corner. The corner
     vertex q keeps its patch-boundary role but is generally NOT on the
     repaired intersection curve.
   - TRANSIT (v27-class): the corner-side endpoint is the real
     candidate; the OTHER chain's true endpoint is NOT in this site's
     candidate list in general. v27: the base-chain end sits mid-lattice
     one-plus facets over (*inc-2b correction: the walk measured the
     corridor as 213→214→base — cand-0's mid-lattice solve was a nearby
     OFF-domain root ~1.5 from the true base∩214 exit; the 1.458
     chord-sag reading was real but belonged to the wrong edge*). v42: cand-0 solves a
     DIFFERENT facet's plane (off-line 5.2 near edge 606 — the fan's
     planes are nearly parallel, adjacent solves land ~5 apart); the
     true base exit measured at v78's cand-0 (edge 606, t=0.11, exact):
     **v42 and v78 sample ONE curve corridor that walks facets
     183→182→181→180**, its middle crease junction (182∩181) owned by
     NO fired site (the mesh far∩B181 chain v80 exists and re-anchors
     there). The walking-chain adjacency R0085 measured (bit-equal
     adjacent solutions) is this same structure.
3. **Fans at the traveller**: ~3 far-side triangles + 1–2 per shared
   face; the v–q wedge is exactly 2 triangles (one per shared face)
   riding the phantom's crossing. `next` has NO triangles at v — the
   corridor's chart region is untouched by the traveller today.

**Apply design (inc-2c input).** The apply unit is the CORRIDOR between
the two mesh chain-ends: split the phantom into the two true junction
endpoints, mint the intermediate crease junctions the walk discovers
(each shared by identity when several views/sites name it — position on
the operand, never edge index), build the run's curve chain across each
traversed facet at chord density, re-anchor every existing healthy chain
the corridor meets (v80-class), and re-fill the affected patch regions
two-sidedly (the I13f f2c/f2c-2 fan/CDT vocabulary). q stays a patch
vertex; nothing is truncated AT q. Postconditions: §4-I9 re-check clean,
junction-contract identity (mint once, share by position), full oracle
gate (P10).

## 3e. inc-2b — the corridor-walk census (LANDED 2026-08-30; measured)

`walk_corridor` + `build_edge_adjacency` in `stage4_transit.rs`: per
planned site, walk the far∩facet curve across the operand's face lattice
from each real junction — per facet, solve every loop edge's candidate
exit {far, facet, partner} seeded at the entry junction, certify it ON
that edge in-domain (`edge_domain_of`, the shared instrument's
single-edge certifier), step across the unique certified exit. Edge-copy
adjacency is POSITION-KEYED (m1 copies pair by endpoint bit-keys; the
v467/v6071 lesson). Terminal states are typed: `ReachedOtherChain` /
`NoExit` / `AmbiguousExit(n)` / `PartnerUnresolved(n)` / `TooLong` — a
walk is never guessed past a non-unique read. Census `-WALK` lines
annotate each junction with the nearest existing mesh vertex carrying
its triple. Unit tests: clip walk (1 step onto the other real
candidate's junction, bit-equal) + a two-facet corridor.

**Measured (2026-08-30) — R0011 + R0074: every walk terminates
`ReachedOtherChain`; zero ambiguity, zero dead ends:**

- v27: **2 steps** — 213→214 (crease junction, NO site) → 214→base
  exit. The inc-0 cand-0 solve ({far, base, 213}) had converged to a
  nearby OFF-domain root ~1.5 away from the true base exit (which is on
  the base∩214 edge) — its `not-corner-incident` reading was correct,
  and the §3d "corridor length 1" inference for v27 is CORRECTED by the
  instrument: the walk, not the discarded candidate, owns the far
  endpoint.
- v37: 3 steps — 187→188→189→base. v42: 3 steps — 182→181→180→base,
  and **v42's step-1 junction ≡ v78's crease candidate BIT-EQUAL, its
  step-2 ≡ v78's base candidate BIT-EQUAL: the corridors MERGE — v78's
  entire clip is the tail of v42's corridor.** The two sites are one
  repair unit spanning creases 183∩182 (v42's real), 182∩181
  (walk-discovered, NO fired site), 181∩180 and the base exit (v78's
  pair).
- v78 (clip, both directions) and R0074 v129 (both directions): 1 step
  each, landing bit-equal on the site's OTHER real candidate — the clip
  class self-validates symmetrically.
- `near_mesh=NONE` at every walk junction: NO existing mesh vertex
  carries any intermediate junction's triple. Every corridor junction
  must be MINTED; existing chains (e.g. v80's far∩181 chain) re-anchor
  at corridor junctions by having their ENDS spliced there — tracing
  each chain's far end is apply-arm work (inc-2c), not census work.
- All junction-to-edge residuals ≤ 6e-12, inside the evaluation band at
  scale 5e3.

**R0044 (arc lattice) — measured same day; three NEW structures for the
apply arm:**

- v8/v89 (clips): both directions 1 step, bit-equal onto the other real
  candidate — the clip class self-validates on arcs too. v75: 3-step
  corridor 373→374→375→wall, `ReachedOtherChain`, residuals ≤ 7.1e-12.
- **v142/v144's walk junctions land BIT-NEAR EXISTING HEALTHY
  travellers** (v157/v156/v152/v160 at 0.9e-12–3.3e-12, all moved,
  in-domain, no §4-I9 fire): the fan between the shared rim mint and the
  existing curve is already healthily owned. The corridor must TERMINATE
  at the first such junction (`ReachedExistingJunction`) and SPLICE —
  minting nothing there. v142's effective corridor is ONE facet (378) to
  v157; the census walk, lacking that terminal, kept walking and later
  hit `AmbiguousExit(2)` — post-termination noise, not corridor data.
  (The apply-arm walk takes an existing-junction lookup and stops; the
  census annotates `near_mesh` per junction, which is how this was
  seen.)
- **v76: `NoExit`** — from its real rim junction into band 363, no loop
  edge certifies an exit under the Newton step. *ADJUDICATED by the
  inc-2c-0 all-roots probe (same day): the true exit EXISTS — the next
  rim (363∩362, edge 1977) carries a certified in-arc root 3.36 from
  the entry. The Newton step missed it because two coaxial cone
  surfaces intersect in MORE than the shared rim edge — the surface-
  triple solve converged to a point off the model edge, failed the
  edge certificate, and reported nothing. The dip hypothesis is
  REFUTED for this site: the entry rim's second in-arc root is 1757
  away (the gear's other angular crossing region), not a local dip.
  Structural finding: EVERY rim here carries TWO certified in-arc far
  roots (the far cone crosses each rim at two angular regions), so the
  walk-step upgrade needs nearest-root selection with a loud margin
  guard, not just all-roots enumeration.*
- `AmbiguousExit(2)` (v142 step 3, v144 step 1, both post-existing-
  junction): the far cylinder × cone-band curve is degree-4 with two
  branches on one facet — certified crossings of BOTH branches. With
  `ReachedExistingJunction` termination these sites never reach the
  ambiguity; if a live corridor ever does, the branch-continuation
  choice (nearest-along-curve from the entry) must be certificated, not
  guessed.

**R0085 (mixed-conformal operand) — measured same day: the walks read
the operand's own quality wall, exactly as inc-1 predicted:**

- **v467: 14 steps across the fan 352→353→…→365→base, EVERY junction
  bit-near an existing healthy moved traveller (8.2e-16–1.6e-13)** —
  the far∩fan curve is ALREADY correctly built in the mesh; v467's real
  junction is the missing base-side END of that existing chain (pure
  splice, zero mints). Its OTHER chain (far∩A:351) has NO exact ending
  at this corner (both its candidates off-line 2.2e-2 / past-end — the
  mixed-conformal residual), so the 351-chain's disposition stays
  downstream of operand quality; the walk's post-base `AmbiguousExit`
  is past-termination noise.
- **v401: the v467 shape again** — 8 steps 233→…→240→base, every
  junction bit-near an existing healthy traveller (≤3.1e-14): pure
  splice, zero mints, then post-base-re-entry ambiguity (noise).
- v4216 / v6071 / v4359: `AmbiguousExit(2)` at step 0 — their `next`
  is the BASE face, and a base face is not a narrow facet: the long
  far∩base conic certifies two crossings immediately. Walking INTO a
  base face is ill-posed without direction control; for these
  crease-rider shapes the other chain's end is again the
  mixed-conformal wall. R0085's transit repairs stay gated behind
  operand quality (its op-3 has an independent input-not-2-manifold
  wall), exactly as recorded in inc-1.

**inc-2b VERDICT.** The corridor unit is CONFIRMED as the apply shape,
and the walk instrument is sufficient on the clean-lattice cases:
R0011/R0074 fully determined (merge proven bit-equal), R0044 determined
up to two named refinements (`ReachedExistingJunction` splice terminal;
all-roots far∩edge-curve step solving, which also unlocks v76's
same-edge re-exit), R0085 honestly walled on operand quality. The
apply-arm (inc-2c) requirements are therefore MEASURED, not sketched:
1. walk with existing-junction termination + splice;
2. per-edge all-roots step solving (exact circle/line carriers);
3. branch/direction-aware continuation only if a live corridor ever
   reaches an ambiguity (none does today after refinement 1);
4. mint-once-by-position across views/sites (the MINTGROUP identity);
5. never walk INTO a base face — entering the other chain's face IS the
   termination.

## 3f. inc-2c-0 — the ALL-ROOTS per-edge step solver (LANDED 2026-08-30,
same session; the §3e requirement-2 primitive, de-risked gated-off)

`stage4_transit.rs`: `circle_surface_roots` (circle × quadric via the
trig-quadric tan-half QUARTIC — deterministic Durand–Kerner + 1D Newton
polish + on-surface certification at the shared evaluation band; `None`
for non-quadric far surfaces — a typed non-answer) + `quadric_form`
(algebraic plane/sphere/cylinder/cone forms) + `poly_real_roots`
(degree ≤ 4, fixed seeds; tangency-grade double roots may be dropped —
which leaves a loud NoExit standing, never mints a wrong junction).
Line carriers reuse the pre-existing `stage4_phantom::segment_surface_
roots`. `face_edge_roots_probe` prints, at every walk `NoExit`, EVERY
loop edge's certified roots with in-domain verdicts (`-WALKROOTS`).
4 unit fixtures (plane 2-root closed-form angles; cylinder 2-root;
genuine-quartic 4-root; cone dip pair certified). Live validation: the
probe reproduces v76's entry junction at d 5.7e-12 and adjudicated the
site (§3e). R0011 census walks byte-stable under the new build.

inc-2c-1 (landed same session, §3g) swapped the walk step onto this
solver with the Newton per-edge fallback, the 4× margin guard, and the
`ReachedExistingJunction` splice terminal — v76 resolved,
R0011/R0074 unchanged.

## 3g. inc-2c-1 — the ALL-ROOTS walk step (LANDED 2026-08-30, same
session; measured family-wide)

`walk_corridor` v2: per face, EVERY loop edge's candidate exits are
enumerated by the bounded solvers (lines `segment_surface_roots`,
circles `circle_surface_roots`) with the inc-2b Newton step as the
per-edge FALLBACK where a carrier×far pair has no bounded solver
(measured need: R0074's far is a TORUS — revolve(circle); the first
all-roots cut silently skipped its edges and turned the determined clip
corridor into a spurious NoExit). ONE certification authority for every
candidate (entry-exclusion by POSITION — same-edge re-exit
representable; on-far at the evaluation band; in-domain via the shared
single-edge certifier). Selection: nearest exit by distance from the
entry, guarded — the second-nearest must be ≥4× farther or the face is
a loud `AmbiguousExit`. New terminal `ReachedExistingJunction{vertex}`:
the walk stops when a discovered junction lies within the evaluation
band of an existing mesh vertex carrying its triple (the census
`existing` lookup; unit-tested). `WalkCtx` bundles the operand context.

Measured:
- **R0011: corridors IDENTICAL** (same faces/edges/positions; straight-
  edge residuals improve to exact 0 — the root is on the line by
  construction).
- **R0074: restored bit-equal** through the Newton fallback (1 step
  each way onto the other candidate's junction).
- **R0044: v76 RESOLVES — 5 steps 363→362→361→360→359→wall,
  `ReachedOtherChain`, and its step-3/step-4 junctions are BIT-EQUAL to
  v105's clip pair: the v76 corridor MERGES with v105 exactly as v42's
  merged with v78's on R0011.** v142 → `ReachedExistingJunction{v152}`
  (3 steps), v144 → `{v165}` (3 steps) — no post-termination ambiguity
  noise remains. All other sites unchanged.
- Splice-band note for the apply arm: the census terminal uses the
  EVALUATION band, under which v142's step-0 junction (1.24e-11 from
  v157, band ≈ 6.6e-12) did NOT terminate and the walk ran two more
  steps to a tighter match. The apply's splice identity should use the
  junction-contract identity band (1e-9·(1+scale)) — under it the
  corridor ends at the FIRST owned junction.

**The walk instrument is now COMPLETE on the family: every corridor is
determined** — R0011 4 sites (2 merged pairs of corridors + 2 solo),
R0074 1 clip, R0044 7 sites (v76+v105 merged; v142/v144 splice to the
existing curve), R0085 honestly walled on operand quality. What remains
for inc-2c is pure APPLY: phantom splits, mints, runs, splices,
re-fill, postconditions.

## 4. Increment ledger

- **inc-0** (2026-08-29): feasibility census LANDED + COMPLETE (this file
  §2); 23 sites / 46 candidate triples across R0011+R0074+R0044+R0085, 100%
  convergence, planar-hull in-domain wherever the verdict applies. Mirrored
  pairs, crease-rider chains, and the v8 anomaly recorded for inc-1.
- **inc-1** (2026-08-29, same session): edge-domain discriminator census
  LANDED + MEASURED family-wide, then the ranking metric made CURVE-AWARE
  (chord ranking is biased against arcs and had manufactured a phantom
  "carrier-authority wall" on R0044). Rule VALIDATED in both directions —
  inclusion (R0011 4/4: 3 crease transits + 1 corner clip; R0074 1/1
  clip; R0044 7 sites: 2 crease transits + 3 corner clips + the v142/v144
  mirrored pair sharing ONE rim mint; R0085 v467+v4216 base transits) AND
  exclusion (R0085's crease candidates rejected on-line past-end; R0044's
  non-real candidates not corner-incident / off-edge). Remaining open:
  the R0085 rider-site residual reading (re-measured under the
  curve-aware ranking — see §3b). §3b above.
- **inc-2a** (2026-08-30): the pure PLANNER landed (`stage4_transit.rs`)
  + census planner-driven + site anatomy + position-keyed mint grouping;
  verdicts measured family-wide, 23/23 ≡/refining inc-1 (§3c). The
  carrier-authority item dissolved with inc-1b (the R0044 rims are exact
  shared Circle edges; the arc band validated live, §3c). The anatomy
  census refuted the "truncate at q" apply sketch — the repair unit is
  the CORRIDOR (§3d).
- **inc-2b** (2026-08-30, same session): the corridor-walk census LANDED
  + MEASURED family-wide (§3e). R0011/R0074: every walk
  `ReachedOtherChain`, the v42→v78 corridor merge proven BIT-EQUAL, v27
  corrected to 2 facets. R0044: existing-healthy-junction re-anchors
  measured at 1e-12 (the `ReachedExistingJunction` splice terminal named),
  v76 `NoExit` names the all-roots per-edge step solver, degree-4 branch
  ambiguity typed. R0085: honestly walled on operand quality (v467's fan
  already healthily owned — pure splice; rider walks ambiguous into the
  base face). The five apply-arm requirements are MEASURED (§3e verdict).
- **inc-2c-0** (2026-08-30, same session): the all-roots per-edge step
  solver + NoExit probe LANDED (§3f); v76 ADJUDICATED (wrong-root
  artifact of the surface-triple Newton — the true exit exists, 3.36
  along the next rim; dip refuted; every rim carries TWO in-arc far
  roots ⇒ nearest-root selection + margin guard named for inc-2c-1).
- **inc-2c-1** (2026-08-30, same session): the all-roots walk step
  LANDED + measured (§3g) — v76 RESOLVED (5-step corridor, merging
  bit-equal with v105's clip), v142/v144 terminate at existing
  junctions, R0011 identical, R0074 restored via the per-edge Newton
  fallback (torus far). Every family corridor is now DETERMINED.
- inc-2c: the gated apply arm (`YANG_451_TRANSIT`) — corridor splice per
  §3d/§3e: split phantoms, walk with existing-junction termination,
  per-edge all-roots step solving, mint-once-share-by-position junctions,
  corridor runs at chord density, two-sided re-fill, §4-I9 re-check +
  oracle gate.
- inc-3: fixed-point integration (multiple sites per case; R0085 has two
  failing ops) + full-corpus gated measurement; flip under the standing
  two-proof protocol.
