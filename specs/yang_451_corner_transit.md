# §4.5.1 Corner Transit — the §4-I9 corner-crosser repair (I13f rehome kin)

**Status (2026-08-31c): TWO CONVERSIONS — R0011 AND R0074 are
SUPPORTED_CORRECT under the gates (`YANG_451_TRANSIT`, R0074 also
`YANG_441_TORUS_CHART`).** inc-2a/2b/2c-0/2c-1 (2026-08-30): planner + walk
census + all-roots solvers — every family corridor DETERMINED
(§3c–§3g). inc-2c-2+3a (2026-08-30b): corridor ASSEMBLY +
cycle-surgery census — repair units as typed data (§3h). inc-2c-3b-0:
the corrected-cycle PLANNER — R0011 18/18, 0 declines (§3i).
inc-2c-3b-1 (2026-08-31): the gated mutation (`YANG_451_TRANSIT`,
default OFF) — first live apply; far arm FAN-LOCAL (§3j).
**inc-2c-3b-2 (2026-08-31, second session): the §4.4.1 ABSORB arm
MEASURED + LANDED — certificate = the SIGN of the ring's connector
dot (defects at exactly −1.0000: v26 AND v46; never a band — d_eps
would over-absorb healthy ends), absorbed fans live on already-planned
keys only. R0011 applies removed=13, completes end-to-end, and
CONVERTS to SUPPORTED_CORRECT (χ=0 adjudicated TRUE genus 1 by
exact-involute voxel-CSG + 1×/2×/4× ladder; `euler_target` authored 0
+ pinned — the R0091/R0063 protocol). The apply generalizes across
densities (2×: one corridor; 4×: the §4.5.4 refine-retry repairs after
an honest natural decline). Flow correction: R0011 runs ONE design
boolean — the "op-2" invocation everywhere in §3h/§3i is the §4.5.4
refine RETRY. **inc-2c-3b-3 (2026-08-31, third session): the
fan-local TORUS chart (`YANG_441_TORUS_CHART`, default OFF) —
R0074 CONVERTS to SUPPORTED_CORRECT (the SECOND conversion, §3k).**
**inc-2c-3b-4 (same session): curve-aware host admission
(`arc_host_admit`) — the arc-host wall FALLS; R0044 op-1 declines
11 → 3 (§3l).** NEXT = inc-2c-3b-5, the v142/v144 mirrored-pair
corridor structure (census first, §3l), then inc-3 (full-corpus
gated measurement, two-proof flip; R0085 stays walled on operand
quality).** Epic opened by
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

## 3h. inc-2c-2 — corridor ASSEMBLY (the pure apply-plan builder;
design 2026-08-30)

The mutation must consume REPAIR UNITS, not per-site walks: the measured
family has two corridors each sampled by TWO sites (v42+v78, v76+v105)
and two walked in BOTH directions (the clips) — applying per-site would
double-mint every shared junction and rebuild the same facets twice.
inc-2c-2 builds the unit: `assemble_corridors` in `stage4_transit.rs`,
pure and census-printed, consumed verbatim by the mutation (inc-2c-3).

Structures (`stage4_transit.rs`):
- `CorridorJunction { sol, faces: (from, to), edge, disposition }`,
  `JunctionDisposition::{Mint, Splice{vertex, d}}` — splice identity at
  the junction-CONTRACT band `1e-9·(1+scale)` (`contract_band`), never
  the evaluation band (§3g's splice-band note; under it v142's corridor
  ends at its FIRST owned junction, one facet in).
- `CorridorRepair { far, walk_op, phantoms, corners, junctions, runs }`
  — `runs[i]` crosses the single facet shared by `junctions[i]` and
  `junctions[i+1]`; `RunSource::{Samples(Vec<pos>), ExistingChain(Vec<
  vertex>)}`.

Assembly steps, every failure a typed `AssemblyDecline`:
1. **Per-site walks under APPLY semantics** — the §3g walker with the
   splice terminal at the CONTRACT band (`WalkBand::Contract`; the
   census keeps `Eval` so inc-2b lines stay byte-stable). Real
   candidate → entry junction; walk junctions include the terminal.
   Any non-terminal end (`NoExit`/`AmbiguousExit`/`PartnerUnresolved`/
   `TooLong`) declines the SITE (`WalkFailed`).
2. **Cross-site merge by POSITION identity** (the MINTGROUP lesson
   generalized): walks sharing any junction within the contract band
   (same walk operand + far patch) cluster; the longest walk is the
   SPINE and every clustered walk must match a CONTIGUOUS run of it,
   forward or reversed (`SpineMismatch` otherwise). One cluster = one
   corridor; its phantom set = the clustered sites.
3. **Face-chain verification**: consecutive junctions must name the
   crossed facet consistently (`faces.to(i) == faces.from(i+1)`), the
   run's facet (`FaceChainBroken` otherwise).
4. **Dispositions**: per merged junction, the existing-vertex lookup at
   the contract band → `Splice`, else `Mint`. Shared-view identity is
   free — merged junctions are ONE entry (mint-once-by-position).
5. **Run sourcing**: per facet run, the EXISTING far∩facet chain census
   (v80-class: mesh vertices carrying {far, facet} — the healthy chains
   §3d requires re-anchoring). A chain whose ends land at the run's two
   junctions (contract band) becomes `ExistingChain` (the mutation
   splices its ends, minting nothing interior). No chain → fresh
   `Samples` from `sample_run_chord`: recursive chord midpoint
   subdivision against far×facet (`relocate_onto_implicit_pair`,
   seeded at midpoints), accepting when the midpoint sag ≤ d_eps (the
   Stage-1 chord band — the paper's §4.3.4 refinement criterion),
   depth-capped loud (`RunSampleFailed`). A chain present but with ends
   NOT at the junctions is inc-2c-2 census data (printed, site
   declined `ChainEndsUnmatched`) — the mutation must not guess.
6. **Consumption check**: every fired site either belongs to exactly
   one corridor or carries a typed decline; a site in two corridors is
   `CorridorConflict` (measured family: none).

Census lines (`YANG_S4_CARRIER_DOMAIN-CORRIDOR`): per corridor —
phantoms, corners, junction list (disposition + faces + d-to-existing),
per-run source (chain vertices / sample count + max sag), declines
per non-assembled site. Unit fixtures: two-walk merge (spine +
reversed subsequence), clip both-directions dedupe, chord subdivision
depth/acceptance, contract-band disposition split.

**inc-2c-3 (the mutation, next)** consumes `CorridorRepair` per §3d:
split each phantom into its two chain-end junctions, mint `Mint`
junctions ONCE, splice `Splice` junctions and `ExistingChain` ends,
insert runs conformally on BOTH sides (far patch + traversed facets;
`split_boundary_edge` for junctions on lattice edges, fan delete +
`refill_fan_hole` for the phantom regions — the f2c vocabulary), then
§4-I9 re-check + oracle gate, all behind `YANG_451_TRANSIT`.

### §3h measured (2026-08-30, LANDED same day; census family-wide)

The first assembly run CORRECTED two design points before anything
mutates — both measured, neither visible from the per-site walks:

1. **"Corridor merge" is CROSS-INVOCATION consistency, not a merge.**
   R0011 runs TWO boolean ops; v27/v37/v42 fire in op-1's Stage 4 and
   v78 in op-2's (R0044 likewise: v8…v144 op-1, v105 op-2). The
   bit-equal junctions (v42 junc0/junc1 ≡ v78's pair to the last bit,
   in two DIFFERENT meshes) are the same analytic curve recurring, not
   one repair unit: each invocation assembles and repairs its OWN
   corridors. Within-op grouping is still real — it dedupes the clip's
   two walk directions (measured on every clip) and any true same-op
   pair.
2. **Union-find over shared junctions is the WRONG grouping.** R0044's
   v142/v144 share exactly ONE junction — their shared rim mint, an
   ENDPOINT of both — and are TWO corridors (the mirrored pair).
   Grouping is now greedy longest-spine with contiguous-sub-run
   absorption (fwd/rev); a non-sub-run walk starts its own corridor;
   cross-corridor overlap beyond one endpoint-to-endpoint junction is
   the loud `CorridorConflict` (unit-tested both ways). Shared
   endpoint mints print as `SHARED-MINT` — the apply mints ONCE across
   corridors (the MINTGROUP contract).
3. **Run sourcing must be LOCAL.** R0044's run facets carry healthy
   far∩facet chains ~1.7e3 away — the far cone's OTHER angular
   crossing region on the same gear band. Components are filtered by
   nearest approach to the run's chord (≤ max(d_eps, contract));
   distant chains are patch neighbours, never sources.

Measured per case (debug census; corridors are per-invocation):
- **R0011: 4 corridors (op-1: v27 3-junc, v37 4-junc, v42 4-junc;
  op-2: v78 2-junc), ALL applyable — every junction Mint, every run
  fresh `SAMPLES n=0`** (far is a plane: the runs are straight, the
  bare chord meets d_eps). consumed=3/3 and 1/1.
- **R0074: 1 corridor (v129 clip, 2 mints, 1 run n=0), applyable** —
  the torus-far per-edge Newton fallback holds under apply semantics.
- **R0044 op-1: 6/6 applyable, consumed 6/6.** v76 6-junc, v75 4-junc,
  v8/v89 2-junc — all-Mint with fresh runs once the locality filter
  ignores the distant chains; **v142/v144: two corridors, each
  1 Splice (v157 @1.2e-11 / v160 @5.9e-12) + the SHARED rim mint
  (`SHARED-MINT corridors=(#4,#5)`), runs measured in the `Spliced`
  shape** — v144's healthy rider v161 keeps its chain and bridges
  fresh to the mint (`SPLICED chain=[161]`), v142's trims to pure
  samples. The wrong edges being replaced are the riders' edges into
  the TWIN phantom (v157→v144, v161→v142). v105 rides op-2.
- **R0085: the honest wall, typed end to end.** v467's corridor:
  junc0 = the FIRST live `Splice` (existing traveller v6232 at
  1.6e-15 — the contract band doing exactly its job), junc1 Mint; its
  run declines `ChainEndsUnmatched`: the healthy 58-vertex chain ends
  at v6232 (exact) and at v6182, which is 1.5e-2 off junc1 and
  adjacency-attached to the OTHER rider phantom v6071 — the
  mixed-conformal operand residual, measured to the vertex. Riders
  v4216/v6071 decline `WalkFailed AmbiguousExit(2)`. Nothing consumed;
  R0085 stays gated behind operand quality (op-3 input-n2m), exactly
  as inc-1 recorded.

**Phantom chain-end attachment rule (measured on every R0011 site).**
The -ANAT curve neighbours of each phantom are its two chain ends, and
each names its corridor junction by PATCH MEMBERSHIP: v78's v75
{A2,B1} → the (1,180) junction, v80 {A2,B181} → (180,181); v27's v26
{A2,B1} → (1,214), v20 {A2,B211,B212} → (213,212). Unique on every
measured site — the mutation's attachment certificate (decline if not
unique). The connector edge (neighbour → its junction) crosses ONE
facet beyond the corridor (the neighbour's own facet) and replaces the
neighbour→phantom mesh edge; its chord density is certifiable by the
same `sample_run_chord`.

**Family verdict at the plan level (final binary): R0011 4/4
corridors applyable (consumed 3/3 + 1/1), R0074 1/1, R0044 7/7 (op-1
6/6 incl. both v142/v144 spliced corridors, op-2 v105 1/1), R0085 0
consumed — honestly walled, all declines typed.**

**inc-2c-3 therefore splits** (the base-side cycle surgery — which
side of the curve each patch's region lies on, where q's crease path
rejoins — must be measured, not sketched):
- **3a: cycle-surgery planner census** (LANDED with this increment:
  the `-CYCLES` lines) — per applyable corridor: the attachment
  certificate live (each phantom's on-curve neighbours → their
  junction by patch membership, `NOT-UNIQUE` loud), and per affected
  patch (far ∪ run facets ∪ the two terminal-outer patches) the
  connected component + boundary cycles with surgery sites marked —
  phantom position, junction HOST edges (the boundary edge whose
  segment carries the junction), ±4-vertex windows tagged
  P/Q/N. Report-only; measured on R0011 (see below).
- **3b: the gated mutation** (`YANG_451_TRANSIT`) — apply the planned
  cycles via wholesale per-component rebuilds (`rebuild_patch_planar`
  vocabulary; Plane+Cylinder charts first — R0011/R0074 class; cone
  bands decline typed until the chart lands), mint-once via the
  SHARED-MINT registry, §4-I9 re-check + oracle gate.

### §3h-3a measured (R0011 corridor #1 = v42; same shapes on #0/#2 and
op-2's clip; 2026-08-30)

- **Attachment certificate: UNIQUE at every phantom** (`-CYCLES phantom`
  lines). v42: v39 {A2,B1} → junc0, v43 {A2,B183,B184} → junc3. v37 has
  THREE on-curve mesh neighbours (v35/v39 → junc0, v46 → junc3) — the
  cycle, not the fan, names the true chain topology: on B1's boundary
  v37's cycle-neighbours are exactly q and v39.
- **The far patch (A2)'s surgery site is a HOLE cycle**: cycles
  [24,15,7]; the 7-cycle [v41 v43 v42[P] v39[N] v37[P] v46 v45] is the
  hole where B's teeth poke through — BOTH phantoms ride it, each
  between its two [N] neighbours. Surgery: replace each phantom with
  its corridor chain oriented neighbour-junction-first. A2 hosts no
  junction (they lie interior to A2's face) — the chain vertices enter
  as new cycle vertices.
- **Junctions host on their model-CREASE mesh edges** (never on curve
  edges): v42's junc0 hosts on B1's (v686,v682) = the (1,180) crease,
  and on B180's (v682,v686); junc1 on B180's (v686,v273) = the
  (180,181) crease — each crease is ONE mesh edge here, present in both
  incident patches' cycles ✓ conformal split points.
- **The base (B1) surgery is measured to the vertex**: one 398-edge
  cycle carries the whole far∩base curve + creases; window
  [... v39[N] → v42[P] → v687[Q] → v688 → v686 → v682 ...]. Corrected:
  connector (v39 → J0) replaces the phantom edge, the cycle TURNS at J0
  onto the crease toward v682, and the sub-path [v42, v687, v688, v686]
  is EXCISED — those vertices stay mesh vertices on their other
  patches' cycles (q "stays a patch vertex" is exactly this).
- **Run facets are cut**: B180 = a 2-triangle quad [v271 v682 J0-host
  J1-host v273]; the run J0→J1 splits it and the v686-side sliver is
  the OUT side (inside operand A) — removed by rebuilding from the
  corrected cycle [v271, v682, J0, run, J1, v273]. Sub-path side
  selection is certifiable by the far surface's SIGN at the sub-path's
  vertices (mixed signs = loud decline).
- **R0085 sharpened the admission rule**: v467's corridor is applyable
  in isolation (57-vertex SPLICED chain, on-far ends, n=0 bridges) but
  its surgery would rewire an edge into the UNCONSUMED rider phantom
  v6071 (declined `WalkFailed`). **3b applies only when EVERY fired
  site of the invocation is consumed by an applyable corridor** —
  R0011 (3/3, 1/1), R0074 (1/1), R0044 (6/6, 1/1) qualify; R0085
  (1/3 + riders) declines wholesale, keeping the honest STOP.

3b's remaining work is then mechanical: corrected cycles per affected
component (splice chains/samples in, excise phantom-side sub-paths,
split host edges at mints), wholesale re-CDT per component, mint-once
registry, apply as one batch, §4-I9 re-check + full oracle gate.

## 3i. inc-2c-3b-0 — the CORRECTED-CYCLE planner (LANDED 2026-08-30,
same session; measured on R0011: 18 plans, 0 declines)

`stage4_corridor.rs`: the §3h-3a surgeries as ONE uniform primitive —
`replace_subpath(cycles, from, to, via, removable)`: walk the directed
boundary cycle forward from a surviving vertex, consume certified
removable interior, stop at a surviving vertex, splice the corridor
path in. `MintPool` interns junction mints + fresh run samples by
position at the contract band (the SHARED-MINT registry);
`corridor_path` linearizes a corridor end-to-end (mint refs, splice
refs, run chains). `plan_invocation` runs three edit GENERATORS per
affected component, each certificate-gated, each orientation resolved
by clone-try (exactly one certifying arrangement or a typed decline):
- **A (far patch)**: phantom flanked by two attachment neighbours →
  swap in the whole path, oriented pred-attachment-first (attachments
  must be the corridor's ends).
- **B (B-side patch with the phantom)**: one attachment neighbour +
  the attached junction's HOST edge → connector, turn at the mint,
  excision through the host edge (both orientations tried across all
  host edges; exactly one certifies).
- **C (run facet)**: consecutive hosted junctions → host-to-host
  excision through the OUT side, run path spliced between.

Removability is anchored on the corridor's own crossed CORNER: the
§4-I9 fire means the traveller crossed q, so q lies between the wrong
and the true curve — the REMOVED side's sign is sign(far(q)),
certificate-guarded (|far(q)| above band, all corners agreeing).
9 unit fixtures (all three generators on the measured shapes, the
ambiguous-anchor decline, the kept-interior refusal).

**Measured on R0011 (`-PLAN3B`): op-1 13 component plans + op-2 5,
11+2 mints, ZERO declines. R0074: 4 plans, 2 mints, ZERO declines**
(the swapped-operand torus-far clip plans cleanly — base A0, facets
A162/A163, the B2 torus patch's boundary re-routed through the two
mints). **R0044: the ARC-HOST wall, typed** — 3+2 plans (the far/base
components) + 11 `HostNotFound` declines: the gear creases are CIRCLE
arcs and the host search measures junction-to-CHORD distance, which is
the chord sag (≫ eval band) — the chord-vs-arc lesson a third time
(inc-1b's curve-aware ranking kin). The R0044 slice of 3b-1 needs a
curve-aware host search (band = max(eval, d_eps) on the crease's own
curve); until then the admission rule refuses R0044 wholesale (partial
plans never apply). Every §3h-3a prediction reproduced: the
A2 hole cycle carries all three corridor swaps ([v43 N7 N6 N5 N4 v39
N0 N1 N2 N3 v46 …] — v42's and v37's paths meeting at v39); the base
curve dips between tooth corridors through the single healthy vertex
v39 ([… v679 N0 v39 N4 v682 …]); B180's corrected pentagon [v682 N4 N5
v273 v271] exactly as predicted; the crease between teeth 212/213
re-terminates at the minted junction ([N10 v308] on both patches).

**Measured REFUTATION of §3d's "q stays a patch vertex": the crossed
corners are excised from EVERY patch** (q687, q663, q685, q834 all in
the removed ledgers) — a crossed corner lies between the wrong and the
true curve, i.e. strictly inside the far operand, so it is not in the
result at all. The sign anchor makes this sound by construction: if a
corner were genuinely kept, the anchor certificate itself
(|far(q)| > band with a consistent sign) plus the walks' NotRemovable
refusals would decline the plan loudly. The creases the corner
terminated re-terminate at the corridor's minted junctions.

**inc-2c-3b-1 (the mutation, next)**: append `MintPool` verts, re-CDT
each planned component from its corrected cycles (Plane charts first —
R0011's class; Cylinder next; cone bands decline typed), replace the
components' triangles in one `apply_rebuild_batch`-style transaction
(dropped = the removed ledgers, foreign-reference-scanned), then
re-run the §4-I9 postcondition + the standing oracle gate — all behind
`YANG_451_TRANSIT`.

## 3j. inc-2c-3b-1 — the gated CORRIDOR MUTATION (LANDED 2026-08-31;
FIRST LIVE APPLY on R0011 op-1: §4-I9 fires REPAIRED, the case moves
to a new downstream wall)

`corner_transit_apply` in `stage4_correct.rs`, called immediately
before the §4-I9 postcondition, gated `YANG_451_TRANSIT=1|on`
(default OFF, byte-identical; `corner_crossing_fires` is the
postcondition's two-leg detection as a quiet fire list, kept in
lockstep). The driver re-runs the measured pipeline quietly (site
planner → contract-band walks → corridor assembly → corrected-cycle
planning — all the landed library pieces with mirrored closures), and
mutates ONLY when every admission certificate holds: zero assembly
declines, every corridor applyable, every fired site consumed, zero
plan declines, every affected patch key planned. Every refusal prints
`[451-transit] REFUSE: …`, rolls back completely (appended mints
truncated), and leaves the standing STOP to fire exactly as today
(P10).

**Two refusals measured live before the first apply, each naming its
fix:**
1. The far patch is a CYLINDER (radius 6277 — the revolve lateral;
   the "R0011 far is a plane" assumption was wrong), and a WHOLESALE
   far-patch re-CDT fails exactly as the I6/I13 lesson on
   `rebuild_merge_fan` records: `TriangulationFailed` (pre-existing
   folds elsewhere on the big lateral's boundary) on op-1 and
   `ChordDegradation` 705→5393 (the §4.4.1 like-for-like d(T) gate)
   on op-2. **The far arm is therefore FAN-LOCAL**: delete the
   phantom's far fan (`delete_boundary_fan`; the link's open ends ARE
   the two attachment neighbours), refill the link + corridor-chain
   polygon in a local chart window (`refill_fan_hole` — the f2c
   vocabulary, local θ-unwrap, local like-for-like budget). The
   generator-A ComponentPlan stays as the coherence certificate.
2. B-side components (teeth facets, the base plane) rebuild WHOLESALE
   from their corrected cycles (`rebuild_patch_planar`) — measured
   fine at their sizes (2–5 tris; the 396-tri planar base).

**Measured (R0011, gate on): `APPLIED corridors=3 plans=13 mints=11
removed=11` — op-1's three corridors repair, the §4-I9 postcondition
PASSES, op-1's boolean COMPLETES, and the case's failure moves
DOWNSTREAM:** op-2's auto-union now fails tessellating the REPAIRED
op-1 output as its input — `TessellationFailed { face: FaceId(402),
"ring rejected by CDT (degenerate/self-intersecting)" }`. Gate off:
byte-identical original ERROR (re-measured). No oracle complaints on
the completed op-1 boolean.

**inc-2c-3b-2: the FaceId(402) ring — ANCHORED (same session, via the
existing `KV2_RING_REJECT_PROBE`).** Face 402 IS the repaired far
cylinder face (124-vert outer + 2 holes = the corridor-edited tooth
crossings). The rejection is NOT curve provenance: hole[0] (v27's
tooth) reads `… → (1787.733, 498.610, -4368.651) →
N8(1791.632, 500.944, -4367.541) → N9 → N10 → v20 …` — the vertex
BEFORE the minted junction is **v26, the chain-end neighbour itself,
sitting 4.7 PAST the junction with an ANTI-PARALLEL connector step**
(the 2D chart shows the doubled-back spike; hole[1] carries the same
~4-unit reversal at its junction). v26 is a healthy-looking relocated
chain vertex whose own position micro-overshoots the facet boundary —
a NON-CORNER out-of-domain slide §4-I9 can never fire on (it crossed
a crease-edge INTERIOR, no still corner vertex). The corridor repair
correctly ends the curve at J0; the pre-existing neighbour overshoot
then folds the emitted ring.

Fix shape (the paper's own §4.4.1 sentence): NEAR-CURVE REMOVAL at
the corridor ends — a chain-end neighbour within band of the minted
junction (or on the wrong side of it along the curve: the
connector-direction certificate, dot(chain-arrival, connector) > 0)
is ABSORBED into the junction (the Fig-11 merge), splicing the chain
one vertex earlier. Two sub-questions to measure first: the band (the
chord band d_eps vs a junction-contract multiple — v26 measured at
4.7 from N8 at scale 7e3), and whether the absorbed vertex's B-side
fans need the same excision treatment as the corner (its position is
past the crease, i.e. inside the far operand).

**inc-2c-3b-2 MEASURED + LANDED (2026-08-31) — the §4.4.1 ABSORB arm;
R0011 CONVERTS to SUPPORTED_CORRECT (the epic's first conversion).**

The ABSORB census (`-CYCLES ABSORB` lines: per chain-end attachment at
an END junction — d(w, J) vs d_eps and the contract band, the
connector-direction dot with the chain continuation, signed surface
values at w and its predecessor against the junction's three surfaces,
chain spacing, corner far-values) answered both sub-questions:

- **The certificate is the SIGN of the connector dot, never a band.**
  Defects sit at exactly −1.0000 (v26 at junc0 of v27's corridor,
  d_j = 4.68; **v46 at junc3 of v37's corridor, d_j = 4.11 — hole[1]'s
  reversal, named**); every healthy end reads ≥ +0.456. A d_eps band
  would OVER-absorb (healthy ends sit at d_j = 53.7–99.4 ≈
  0.3–0.5·d_eps); the contract band (4e-6) catches nothing.
  Corroborating signature: the overshot vertex's value on the
  junction's facet-side face FLIPS SIGN vs its chain predecessor
  (v26: +4.674 vs v25's −59.3; v46: +0.221 vs v45's −4.73) — it is ON
  far and ON the outer face but PAST the junction.
- **No separate B-side machinery.** The absorbed vertex's fans live
  entirely on far + the corridor's own TERMINAL-OUTER patch (v26:
  {A2, B1}; v46: {A2, B186}) — both already planned keys; the
  wholesale cycle rebuild absorbs them for free once the corrected
  cycle splices one vertex earlier. The corner-sign removability test
  CANNOT certify an absorbed vertex (far(w) ≈ 0 — it is ON the curve);
  it needs its own verdict (`Removability::Absorbed`).

Landed shape (planner = the single absorb authority):

- `stage4_corridor.rs`: `absorb_anchor` — on the located cycle, walk
  the anchor back along the chain while dot(anchor − continuation,
  junction − anchor) < 0, depth-capped 4, typed declines; wired into
  generator A (both ends) and generator B (the connector anchor);
  `Removability::Absorbed` accepted by `replace_subpath`; `PlanCtx.pos`
  + a `refpos` that resolves NEW-ref continuations from the mint pool
  (live refusal: v39 — the base-curve DIP vertex between two tooth
  corridors — has the neighbouring corridor's already-spliced mint as
  its cycle continuation; declining on New refs killed the whole op-1
  plan. An absorb LANDING on a New anchor still declines: that is a
  genuine cross-corridor entanglement).
- Far arm (`corner_transit_apply`): victims = the far plan's `removed`
  set flooded from the phantom THROUGH removed vertices (surviving
  anchors bound the flood exactly — one absorb authority, the fan arm
  executes it); `delete_boundary_fan_set` (stage4_construct) deletes
  the joint region; link ends re-certified by PATCH-MEMBERSHIP
  junction resolution (post-absorb they are chain continuations, no
  longer the phantom's own neighbours); refuses if the flood reaches
  another corridor's phantom.

**Measured (R0011, gate on): `APPLIED corridors=3 plans=13 mints=11
removed=13` (11 + v26 + v46), the FaceId(402) ring is CLEAN, the
boolean AND the output tessellation complete — and the case surfaces
`SUPPORTED_WRONG mesh_euler_characteristic χ=0 (expected 2)`,
ADJUDICATED as ORACLE AUTHORING, not a defect:** the union of the
14-tooth gear prism with the 295.56° revolve band is TRUE genus 1.
Three independent proofs (the R0091/R0063 protocol):
(a) exact-involute voxel-CSG from the authored numbers — the prism
touches the band ONLY near its start cap (spine t < 150; the root
circle r=2055 misses the band r∈[4708, 6277] entirely), in exactly
TWO disjoint adjacent-tooth patches ⇒ genus k−1 = 1; (b) the density
ladder — χ = 0 at 1×/2×/4× through three DIFFERENT fire anatomies
(3 corridors / 1 corridor / refine-retry apply after a NoRealCandidate
natural decline: the apply generalizes across densities); (c) volume +
watertight + single-shell + improper=0 all pass. `euler_target` 2 → 0
authored in `R0011.meta.json`, pinned in `assay_euler_consistency.rs`.
Gate off: byte-identical original ERROR.

**Family measurement (gate on, same session): every walled case keeps
its standing STOP, typed and loud.** R0074: natural stops UPSTREAM of
§4-I9 (OffCurve v91 — unchanged honest ERROR); the refine-retry's
apply fires (129,127) and refuses at `far fan refill: NonPlanarPatch`
— **R0074's far is a TORUS and `SurfaceChart` has no torus chart**
(the R0074 slice; the splice module's UnsupportedSurface note lists
Sphere/Cone/Torus). R0044: both invocations refuse on the known
ARC-HOST wall (10× HostNotFound + one NotRemovable-Ambiguous).
R0085: op-1 refuses `site v387: NoRealCandidate`, op-2 stays on its
input-n2m wall — both standing ERRORs unchanged.

**Flow correction (this measurement): R0011 runs ONE design boolean.**
The second §4-I9 invocation every census saw (v78/v834, 999 verts) is
the §4.5.4 refine RETRY of the same union at boosted rim density (the
`boolean()` detect-then-refine second pass), not a second design op —
"op-2" in §3h/§3i reads correctly as natural-vs-refined of the SAME
op (the bit-equal junction consistency across the two is the same
analytic curve at two densities). The 3b-1 "op-2 tessellating the
repaired output" was the RENDER tessellation of the output body.

## 3k. inc-2c-3b-3 — the fan-local TORUS chart (LANDED 2026-08-31,
third session; R0074 CONVERTS — the epic's SECOND conversion)

The R0074 wall typed by 3b-2's measurement: the refine-retry
invocation fires (129, 127), the corridor plans cleanly (4 plans,
2 mints, zero declines), and the apply refuses at
`far fan refill v129: NonPlanarPatch{592}` — R0074's far is a TORUS
(revolve(circle)) and `refill_fan_hole`'s `SurfaceChart` had no torus
chart.

Landed shape (`stage4_project.rs` + `stage4_construct.rs`):

- **`SurfaceChart::Torus`** with the PINNED `stage4_dt::eval_uv` §2
  embedding — param `(θ, φ)` = azimuth about the axis + tube angle,
  `center + (R + r·cos φ)(cos θ·e1 + sin θ·e2) + r·sin φ·â`, the same
  `ortho_basis(axis)` frame — so the like-for-like d(T) budget
  (which projects through the chart and certifies through `d_of_t`)
  speaks one convention. Ring torus only (`R > r > 0`, the
  `validate_surface` rule). A unit test pins chart.lift ≡ eval_uv and
  the on-surface round-trip.
- **`SurfaceChart::new_local`**, the FAN-LOCAL constructor: everything
  `new` charts, plus the Torus under `YANG_441_TORUS_CHART=1|on`
  (default OFF — byte-identical everywhere by construction).
  `new`/`supports` are UNCHANGED: a torus is doubly periodic and a
  whole patch may wrap a full period in either direction, so
  wholesale-patch holders (construct / splice / stage-5 holder gates,
  which chart entire boundary cycles) keep today's typed refusals
  until seam machinery exists. Only `refill_fan_hole` consumes
  `new_local` — a fan hole's link polygon is corner-local, where
  chain-unwrap is sound.
- **Double-periodic handling in `refill_fan_hole`**: the predecessor-
  relative chain-unwrap and the `< 2π` span guard now apply to φ
  exactly as to θ (torus only; cylinder/cone byte-identical), and the
  budget's old-fan unwrap re-centres both coordinates toward the
  window mid. Unit fixture: a fan window straddling BOTH seams
  (θ = π AND φ = π) fills with the far-arm polygon shape (link +
  corridor mint) and refuses `NonPlanarPatch` with the knob off. The
  first fixture attempt dropped a corner from the polygon and the
  like-for-like budget honestly refused it (ChordDegradation 1.13 vs
  0.61) — the budget arm works on tori; the far-arm polygon (same
  footprint, corridor mints closing the hole) is the satisfiable
  shape, exactly why §3j's far arm passes the whole link + path.

**Measured (R0074, `YANG_451_TRANSIT=1 YANG_441_TORUS_CHART=1`):
`APPLIED corridors=1 plans=4 mints=2 removed=2`, the §4.5.4
refine-retry invocation completes (the natural pass still stops at
OffCurve v91 and the retry repairs at 2733 verts), and the case is
`SUPPORTED_CORRECT — all checks passed` (10.4s), with the STANDING
authored expectations — no oracle adjudication needed.** Proofs:
gates off → byte-identical honest ERROR (OffCurve v91); transit-only
(torus knob off) → the fire + `REFUSE far fan refill v129:
NonPlanarPatch{592}` reproduced verbatim, ERROR unchanged (the torus
knob IS the delta); R0011 converts identically under transit-only and
transit+torus (the knob is inert for its cylinder far).

## 3l. inc-2c-3b-4 — the CURVE-AWARE host admission (LANDED 2026-08-31,
third session; the arc-host wall falls — R0044 advances to the
mirrored-pair sub-walls)

The `-HOSTS` census (`YANG_451_HOSTS=census`, kept, default off: per
candidate within `max(eval, d_eps)` — distance, unclamped chord
projection `t`, and the junction's residual on the surfaces the edge
separates, against the CONTRACT band) measured the discrimination
structure before anything was built:

- **True hosts carry a curve certificate.** The junction's residual on
  BOTH surfaces the chord separates reads ≤ 5e-13 (contract band
  3.5e-6, ten ORDERS of separation): the chord is a chord of the
  junction's OWN curve. Same-curve non-hosting neighbours separate by
  the projection param (hosting t ∈ [0.01, 0.82]; non-hosting
  t ∈ {−1.56…−1.17} ∪ {+1.02…+4.2} — clean gap).
- **A blunt `max(eval, d_eps)` distance band alone POISONS healthy
  components** (measured live first): on comp B:1 the junction sits
  45.8 OFF the surface (own = −4.58e1) yet its cycle edges admitted at
  d = 45.8 ≤ d_eps — the spurious hosts turned every HostNotFound into
  a HostMismatch and killed whole plans. The distance band identifies
  WHERE along a curve; only the residual certificate identifies WHICH
  curve.

Landed rule (`s4t::arc_host_admit`, one function, both `hosts_on`
closures — driver AND the §4-I9 census mirror — in lockstep):
today's eval arm VERBATIM (junction on the chord: the R0011-proven
straight-crease population, `on_curve` never consulted), else the
CERTIFIED ARC arm — `d ≤ max(eval, d_eps)` (the sag the tessellation
certifies) AND unclamped `t ∈ [0, 1]` AND the junction on the chord's
own curve (both separated surfaces at contract). Unit-pinned on the
measured shapes.

**Measured (R0044, both gates): op-1's declines collapse 11 → 3** —
`(1, NotRemovable{v92, Ambiguous})` (the pre-existing removability
sub-wall) + corridors #4/#5 `HostNotFound{junction: 1}`: the
v142/v144 MIRRORED PAIR at shared corner q=v513 — junc1 is genuinely
absent from the failing components' curves (own = −16.5 on B:377);
this is the §I13(f)-style view-entanglement anatomy inc-1 named, not
a band problem. The retry invocation's v105 corridor now PLANS and
advances to the far arm, refusing at link-end attachment
`(Some(0), None)` — the next measured sub-wall. **R0011 and R0074
re-measured under the new rule: byte-identical APPLIED lines, both
still SUPPORTED_CORRECT.** inc-2c-3b-5 = the mirrored-pair corridor
structure (view dedup / shared-junction splice), census first.

## 3m. inc-2c-3b-5 — the MIRRORED-PAIR planner stage (LANDED
2026-08-31, third session; R0044 op-1 PLANS COMPLETELY — the refusal
moves to the far arm's joint-region guard)

Census-first throughout (`-HOSTS` grew `[451-comp]` per-component
host/phantom rows, `[451-win]` phantom cycle windows with far values,
`[451-corner]` corner anchors, `[451-att]` attachment resolution).
Six measured findings, each landing its own mechanism:

1. **The corner-clip C pair.** The mirrored corridors' shared junction
   (far∩378∩379, the bit-equal mint at q=v513) is a TRIPLE point: the
   OTHER view's facet component hosts it TWICE (the intersection-curve
   chord AND the crease chord) with no phantom present. Generator C
   demanded consecutive pairs (j, j+1) → HostNotFound. Now a
   same-junction pair of two DISTINCT host edges plans the corner
   sliver's excision through the single mint (the ja == jb slice); the
   sign walk certifies every consumed vertex (q reads removed, §3i's
   refutation), orientation uniqueness as before. Unit fixture.
2. **View dedup by the shared-mint identity.** The second mirror's
   clip on the same component is the SAME excision — its minted ref is
   already spliced into the corrected cycle (MintPool position
   interning). A corner-clip pair whose New ref is already present
   SKIPS (idempotence); Old refs never skip.
3. **Decline tuples carry the component** (`(corridor, comp,
   decline)`) — the 3b-5 censuses needed the failing component, not
   just the corridor.
4. **`absorb_anchor` is cycle-bounded, not cap-4.** Corridor #1
   (v75) measured a 4-deep doubled-back chain (v102 v90 v91 v92 — all
   on-curve, far ≈ 4.5e-13) that the cap declined ONE short
   (`NotRemovable{v92}`). Overshoot depth is a property of the defect,
   never a tunable; every absorbed vertex carries its own sign
   certificate, so the walk's only bound is termination (the cycle).
5. **Generator B absorbs the UNATTACHED flank too (best-effort).**
   v142's far-comp walk blocked on v141 — an on-curve remnant BETWEEN
   the twin phantoms, past the mint with a doubled-back ring step —
   the same §4.4.1 certificate as the connector anchor, on the other
   side. Best-effort: a flank that does not absorb falls through to
   the sign walk (which stays loud); never a plan-killer (the first
   strict version broke the R0011-shape fixture on unreadable flank
   geometry — measured, reverted to best-effort).
6. **`affected_keys` is splice-aware.** A Splice-disposition end
   junction mints nothing — its terminal-outer patch has no work, and
   expecting a plan there refused the whole invocation ((B,377) /
   (B,380) for the v142/v144 corridors).

**Measured (R0044, both gates): op-1's plan declines are GONE — the
invocation reaches the far-arm APPLY and refuses at the joint-region
flood guard: `far fan of v76 floods into another corridor's phantom`.**
Corridor #0 (v76+v105 merged) and corridor #1 (v75) are ADJACENT on
the far patch (v75 is v76's mesh neighbour); their removed regions
connect, and the per-corridor fan-local far surgery cannot describe
the joint hole. **inc-2c-3b-6 = JOINT far-fan regions**: flood across
corridors, one `delete_boundary_fan_set` for the joint victim set,
polygon = link + the corrected-cycle segment between the link ends
(read from the far plan — the planner already resolved the
interleaving). The retry's v105 link-end refusal is the same anatomy.
**R0011 and R0074 re-measured: byte-identical APPLIED lines, both
SUPPORTED_CORRECT.**

## 3n. inc-2c-3b-6 — JOINT far regions, the arc-stitch polygon, the
SEEDED refill, and the wrap-band dispatch (LANDED 2026-08-31, third
session; FIVE R0044 walls fall in sequence — the case stops typed at
the removed-membership closure, named inc-2c-3b-7)

The `-joint` census first: the v76+v75 joint region (7 victims, 10
triangles) decomposes into TWO link runs ([84,78,79] interior chain;
[77,93] chord) alternating with two boundary gaps whose replacements
are the corridors' minted paths — the single-fan contract
(`Closed{fan:10}`) cannot express it, and the polygon is
runs ⨯ corrected-cycle arcs. Landed:

- **`delete_boundary_fan_runs`** — the multi-run generalization (the
  single-run fn is now its wrapper, refusal shapes preserved).
- **`stitch_fan_polygon`** — alternate link runs with corrected-cycle
  arcs whose INTERIOR is all-New (an Old inside an arc means the
  selection left the region); every choice must be unique, every
  failure typed. Replaces the per-corridor path/junction-attachment
  polygon — the single-corridor case is the one-run instance.
- **The far arm consumes phantoms GLOBALLY** (far-mates flood into ONE
  joint delete; a foreign-far phantom in the flood still refuses).
- **`refill_fan_hole_seeded`** — when the boundary-only fill certifies
  coarser than the fossil, insert the worst triangle's chart centroid
  (LIFTED exactly onto the analytic surface) and re-CDT, capped by the
  fossil's own vertex spend (victims count): like-for-like in DENSITY
  as well as d(T) — §4.4.1 inserts vertices, it never legalizes a
  coarser fill. Exhaustion still refuses ChordDegradation (v8's far
  fill: 532→544 boundary-only, PASSES seeded).
- **The wrap-band dispatch**: gear bands refuse the WHOLESALE rebuild
  by its own typed verdicts — ThetaUnwrap (B:154 winds a full period —
  no disc chart exists), ChordDegradation (B:155 blew up 61×), Cdt
  (B:359's projected boundary self-crosses, the I6/I13 fold anatomy).
  On any of the three, the component routes through the SAME fan-local
  region machinery (per connected removed region: delete + stitch +
  seeded refill). Wholesale stays the proven default (R0011's B-side
  path untouched).

**Measured (R0044, both gates): the far arm and every component
rebuild PASS; the batch-integrity scan refuses typed —
`removed v107 still referenced by tri [249,107,79] att=(A,3)`.** The
`-foreign` census names the closure: ten foreign holdings, ALL on
boundary cycles — (a) **B:0** (the prism base — the corner q=v513's
THIRD face, holding the twin phantoms v142/v144 + v141 + q itself):
should have been planned, but `affected_keys` never includes the
crossed corner's other faces; (b) **A:3** (the operand-A neighbour
face — v107's fan: the old curve wandered across the A2∩A3 crease);
(c) **B:370/B:371** (incursion strips — the deep overshoot chain
v90/91/92 crossed neighbouring bands whose reclaimed territory needs
a TWO-SIDED conformal update, not independent fan fills). R0011's
3b-2 premise "absorbed fans live on planned keys only" is R0011-local.
**inc-2c-3b-7 = the removed-membership closure**: affected comps are
discovered from the removed set's memberships; phantom/host comps
plan as today, incursion-strip comps need the two-sided §4.4.1
machinery. **R0011 and R0074: byte-identical APPLIED lines, both
SUPPORTED_CORRECT; 797 lib tests green.** (The retry invocation's
v105 refill stays walled at ChordDegradation 53.9→74.4 — seeding
worsens it there, a centroid-vs-edge-split question — moot while the
natural pass is the live path.)

## 3o. inc-2c-3b-7 — the removed-membership closure, PARTIAL (LANDED
2026-08-31, third session; R0044 stops typed at the base-boundary
adjudication)

- **Phantom-membership keys**: the driver's pull-in (and the census
  mirror, lockstep) extends each corridor's key set with its phantoms'
  patch memberships, and `plan_invocation` treats phantom-presence on
  a component's cycles as affectedness regardless of the key formula —
  the phantom must vanish everywhere (the I13 interference-group
  lesson). R0044's prism base B:0 (the crossed corner q=v513's third
  face, carrying the twin phantoms + v141 + q) now PLANS.
- **The closure SWEEP**: a comp holding removed vertices but neither
  planned nor far-processed gets the same fan-local surgery with a
  SYNTHESIZED correction (its own cycle minus the removed run — empty
  stitch arcs; direct reconnection at chord accuracy, the absorb
  splice seen from the other patch). Conformality stays adjudicated
  downstream (§4-I9 re-run, stage-5/6 manifoldness, oracle gate) —
  loud, never silent, and the apply is gated. A 2-vertex hole over one
  triangle rebuilds EMPTY (the sliver shape, measured on A:3's v107
  fan — the `fan_rebuild_core` precedent).

**Measured (R0044, both gates): the invocation advances to
`(4, 11, AttachmentMismatch{phantom: 142})` — B:0's generator cannot
anchor: the phantom's cycle-neighbours there are v141 (the remnant)
and q itself, neither resolving to a junction.** The removed run's
vertices on B:0 read far ≈ 0 — they lie ON far∩base — so whether the
base re-closes by mint-splice, by direct reconnection, or along a
far∩base segment is a genuine geometric adjudication (the corner may
truly be cut by the far body at the base). **inc-2c-3b-8 = the
base-boundary adjudication: census B:0's corner anatomy against the
paper's §4.5.5/§4.3 corner vocabulary before building.** R0011/R0074:
byte-identical APPLIED lines, both SUPPORTED_CORRECT; 797 lib tests
green.

**3b-8 ANCHOR (same session, the `-base` census —
`[451-base]` lines, kept):** the crossed corner's THIRD-face
junctions EXIST — solving {far, B:0, F} seeded at q=v513 converges
for every junction face: **{far, base, 378} at d_q = 47.8 and
{far, base, 379} at d_q = 104.2, and the mirrored corridors #4/#5
share them BIT-EQUAL** (the shared-mint identity extends to the
base). The far body truly cuts the base creases near the excised
corner; the removed run on B:0 (far ≈ 0 verts) lay along the
far∩base curve between these junctions. **The corner-crossing has a
SECOND LEG on the base**: the repair = two new base junctions + a
far∩base run (the `sample_run_chord` vocabulary on {far, base}) +
B:0's cycle surgery, with the far comp's boundary gaining the same
leg — a corridor-shaped structure the walk never solved because it
crossed the facet fan, not the base. inc-2c-3b-8 builds the BASE LEG
(assembly + planning + apply), census-first on the cycle shapes.

## 3p. inc-2c-3b-8 — the base-leg REFUTATION and the total-excision
closure (LANDED 2026-08-31, fourth session; R0044's natural
invocation APPLIES — corridors=6 plans=20 mints=15 removed=25 — and
the case stops typed at the batch boundary-conformality wall, named
inc-2c-3b-9)

**The cycle-shape census (part 2 of `[451-base]`, all probes kept:
`[451-bleg-rim]` B-Rep rim roots with in-domain flags,
`[451-bleg-edge]` face-loop inventories, `[451-bleg-v]` near-corner
memberships, `[451-bleg-tri]` phantom stars, `[451-bleg-host]` host
admission, `[451-bleg-cyc]` cycle windows, `[451-bleg-run]` leg
sourcing, `[451-bleg-ph]` phantom→solution distances) REFUTED the
base-leg reading before anything was built:**

- **The candidate junctions are OUT-OF-DOMAIN.** On the REAL creases
  (B-Rep base-loop edge 72 = the 378∩base seam [q → bv73, length
  51.7]; edge 71 = the 379∩base seam [bv71 → q]) the far roots sit at
  **t = −0.924 and t = +1.787 — beyond the corner, off both
  segments**. The wall-bottom seam corner is WHOLLY inside the far
  body (far < 0 along edges 71/72 end to end; far(q) = −30); the base
  face's real boundary crossings sit at d_q ≈ 538 and 712 (edges
  98/100), other seam sites that never fired.
- **The anchor's census was CIRCULAR.** v144 IS the {far, base,
  378-extended} triple solution (distance 0.0, bit-equal) and v142 IS
  the {far, base, 379-extended} one (2.9e-12): Newton seeded at q
  converges onto the fired travellers' OWN relocated positions —
  §4.3 relocation had already pulled them onto the extended-surface
  triples. The "bit-equal across the mirrors" observation is the same
  seed and surfaces, not evidence of a boundary junction. **An
  extended-surface triple convergence is an IDENTIFICATION, not a
  domain certificate — adjudicate against the B-Rep edge domains
  (`face_edge_roots_probe`) before reading it as topology.**
- **The true local anatomy:** the carried chain [v144 → v141 → v142]
  is INTERIOR far∩base curve crossing the base face's reflex sector
  at the corner; the sliver comp B:0/11 (exactly 2 triangles
  [142,513,141]/[141,513,144]) is the wrongly-kept pocket between
  that chain and q. The three standing plans (far, band 378, band
  379) already produce the true transit topology
  [v160, v161, MINT, v157] — v161 is the healthy mid-transit vertex.
  **comp 11's true plan is EMPTY — total excision.**

Landed (three mechanisms, each measured against its own wall):

1. **The planner fall-through** (`plan_invocation`'s (None, None)
   attachment arm): a phantom neither of whose cycle neighbours
   resolves to a junction, on a component with NO hosted junction for
   the corridor, is the wholly-condemned pocket — leave it UNPLANNED
   (no decline) for the driver's closure sweep; a hosted component
   keeps the typed `AttachmentMismatch`. Unit-pinned both ways.
2. **The sweep's whole-component excision + the contained-strip
   2-gon closure.** A component whose EVERY vertex is
   removed-certified rebuilds EMPTY as one unit (comp 11). The 2-gon
   hole arm generalizes from `old_tris == 1` to the structural
   containment certificate — every deleted triangle's vertex ∈
   victims ∪ rim — measured on B:371's 3-triangle overshoot strip
   (victims v90/v91/v92, the §3m absorb chain, chord distances
   77.8–119.3 vs d_eps 127.7). NO distance band: the precedent sliver
   (A:3 v107) reads 128.6 vs d_eps 127.7 — 1.007× — so any band
   tight enough to mean something breaks the precedent, and the
   victims' removal certificates (phantom/sign/absorb, already held
   by every sweep victim by construction) are the sound authority.
3. **Batch-carried seeds.** `refill_fan_hole_seeded` seeds now ride
   `PatchRebuild::new_verts` through `apply_rebuild_batch`'s I2e
   remap (plan stamp = the pre-seed baseline) instead of eager
   mid-mutation appends — R0044's first live seed had moved the
   vertex baseline and staled every other rebuild's plan stamp
   (`StalePlan` with equal printed tris; the failing arm was verts).

**Measured (R0044, both gates): the natural invocation APPLIES —
`corridors=6 plans=20 mints=15 removed=25` — all six fires consumed;
the op proceeds to Stage 6 and stops typed at
`NonManifoldOutput` (s6-boundary-walk-deadend v13998), and the §4.5.4
refine retry's own corner fire (v105) still refuses at the standing
ChordDegradation wall (no longer moot — it is ON the path when the
natural output is broken).** The post-batch watertightness audit
(`[451-audit]`, census-gated, kept) localizes the whole wall: **16
unpaired directed edges in 4 repair neighbourhoods**, one family —
the batch rebuilds each patch to its corrected cycle independently,
and boundary edges pair only between patches sharing the same
corrected chains. Two sub-defects:

- **(A) Cross-plan retained-removed vertices**: comp B:372/391's plan
  keeps v90 as a host-edge survivor (`to` endpoint of host (102,90))
  while corridor #0's far plan REMOVES v90 — the corrected cycle
  references a globally-removed vertex, and the batch-integrity scan
  misses it because it checks only OLD triangles outside `replaced`,
  never the plans' corrected cycles / new_tris.
- **(B) Un-planned neighbours never adopt the mints** splitting their
  shared chains: A:3's sweep closes (79, 249) directly while the far
  refill routes through mint 16360 between them; B:371's empty
  closure leaves (13799, 13831) unpaired against band 372's minted
  chain. The missing arm is the paper's §4.4.1 Fig-11(a) on the
  NEIGHBOUR side: locate the constrained edge containing the
  junction and SPLIT it (combinatorial, position already exact at
  contract).

**inc-2c-3b-9 = the batch boundary-conformality closure**: (A) treat
`removed_all` as a batch-wide substitution/exclusion authority over
every corrected cycle (the I13 interference-group lesson at the PLAN
level), and (B) a neighbour-side edge-split pass for mints landing on
chains shared with un-planned patches — census-first on the 16
measured edges. R0011 (`corridors=3 plans=13 mints=11 removed=13`)
and R0074 (`corridors=1 plans=4 mints=2 removed=2`): byte-identical
APPLIED lines, both SUPPORTED_CORRECT; gates-off R0044 reproduces the
honest ERROR verbatim; 799 lib tests green.

## 3q. inc-2c-3b-9a — the conformality witness census, the removed-union
filter (A), and the survivor-testimony orientation certificate (C)
(LANDED 2026-08-31, fourth session; 16 unpaired edges → 10, all one
family — the ABSORBED CONTINUATION anatomy, named 3b-9b)

**The `[451-audit-edge]` witness census** (per unpaired edge: the
incident triangles with post-batch attributions) decomposed the 16
edges into three sub-defects; two are LANDED:

- **(A) The removed-union filter** (`plan_invocation`, pure,
  unit-pinned): a vertex removed by ANY plan vanishes from EVERY
  corrected cycle — generator B's host-edge `to` survivors (v35 on
  comp 167, v107 on comp 13, v90 on comp 391) were retained while
  their far plans' absorb floods removed them; each retained
  reference was an unpaired edge. One pass (dropping an
  already-removed vertex adds nothing to the union); degenerate
  cycles stay for the mutation's typed refusals.
- **(C) The survivor-testimony orientation certificate**
  (`refill_fan_hole_seeded`, ALWAYS-ON — full-corpus proof): a rim
  edge shared with a surviving same-patch triangle must be traversed
  in opposite directions by survivor and refill. The fossil
  area-vector arm is only a heuristic — the relocation that made the
  region a defect can FOLD the deleted triangles (comp 398: v142
  moved 104 across the corner; the folded fossil flipped the sum and
  ONE inverted refill triangle produced every unpaired edge at that
  corner). Survivors are healthy by construction; triangles touching
  the batch's condemned set are excluded from testimony (a
  neighbouring region's fan is as unreliable as this region's
  fossil); mixed testimony is a loud DegenerateOrientation; where no
  survivor touches the rim the area arm stands. Byte-identity
  argument: testimony acts only on CONFLICT, and a conflicted refill
  is an inverted refill, which no green case can contain (it would be
  non-manifold at stage 6).

**Measured: 16 unpaired edges → 10.** The residue is ONE anatomy —
**the ABSORBED CONTINUATION**: the §3j/§3m absorb consumed carried
CREASE-CROSSING JUNCTION vertices (3-face memberships) whose true
refined crossings exist in-domain BEYOND the corridor's terminal
junction. The connector-dot sign certificate is necessary but not
sufficient: it proves the chain lies past the junction, NOT that the
curve ends there. The rim-domain census (`[451-bleg-rim]`, corridor
#1) is decisive:

- Every band crease near v1188 carries TWO in-arc far roots (54–104
  near the corner + ~1400 far); **the 371∩372 crease has a true
  in-domain crossing at d_q = 97.1 that no plan minted** — its
  carried representative was v90 ({far, 371, 372}), absorbed. v92
  ({far, 370, 371}) is the carried 370∩371 crossing, likewise
  absorbed. The corridor's j0 is itself the band-END-edge crossing
  (edge 2017/821 at 77.6 — the bands end on a serrated seam at the
  corner, the v513 anatomy again).
- N1 mirror: v35 = the carried {far, 153∩154} crossing (corridor
  #2), absorbed; the far boundary now jumps from v34 (band 153)
  straight to the (154,155) mint with no 153∩154 junction.
- N2: v107 = the curve crossing OPERAND A's OWN crease — its
  memberships are {A:2, A:3}: the intersection curve passes from far
  face A:2 onto A:3, where the healthy carried curve (v248/v249)
  continues. The far FACE CHANGES mid-curve; the corridor vocabulary
  assumes one far patch.

**inc-2c-3b-9b = the CONTINUATION arm**: an absorb candidate carrying
a third-face membership is not absorbable-into-the-mint — it marks
the curve CONTINUING across further boundary curves. The repair is
the paper's Fig-12(e) applied at the corridor end: RESUME the walk
from the terminal junction across the additional creases (the
all-roots rim solver already exists), mint the true crossings, sample
the runs, and terminate on the healthy chain — B-side creases first
(N1/N3); the A-side far-face transition (N2) is its own sub-slice
(the corridor gains a far-face CHANGE at an {A-crease × B-face}
junction). Census data for both lives in this section's audit rows.
R0011 (3/13/11/13) + R0074 (1/4/2/2) byte-identical, both
SUPPORTED_CORRECT, after (A)+(C); 800 lib tests green.

## 3r. inc-2c-3b-9b — the STANDING-JUNCTION certificate (+ the
continuation walk as the displaced arm) (LANDED 2026-08-31, fourth
session; the post-batch audit reads CLEAN — zero unpaired edges — and
R0044's boolean COMPLETES; the case moves to the output-tessellation
ring wall)

**The measured pivot: the primary defect was not missing junctions —
it was the absorb DELETING true ones.** 3b-8's circular-census fact
(§4.3 relocation places carried crossings exactly onto their triple
solutions) is load-bearing in reverse: v35 ({far,153∩154}), v90
({far,371∩372}), v92 ({far,370∩371}), and v107 ({A2,A3,B1} — the
far-op crease shape) all sit ON their own triples at contract. The
connector-dot sign read them "doubled back" only because the test was
anchored on a DIFFERENT junction 20–75 away — necessary, not
sufficient. **The STANDING-JUNCTION certificate** (Fig-11(b) made
exact: the vertex carries a 3-face membership and lies within the
CONTRACT band of its own `relocate_onto_implicit_triple` solution)
now gates both the absorb (`PlanCtx::standing`; a standing vertex is
never consumed — it anchors) and the continuation span walk. Both
triple shapes: {far, two walk-op faces} and {two far-op faces, one
walk-op face}. With it, the plans RETAIN the carried chains through
the standing junctions and every conformality cluster dissolves:
**16 → 10 → 3 → 0 unpaired edges; the natural batch is WATERTIGHT
(removed = 20), the §4-I9 re-run passes, and the design boolean
COMPLETES.**

The CONTINUATION arm (the corridor extension) also landed and
validated live — resumed walks found exactly the census-predicted
crossings ((372,371)+(371,370) for corridor #1; (154,153) for #2) —
but with standing in force no R0044 span retains a displaced crosser,
so no extension currently fires there; it remains the arm for
genuinely displaced crossings. Its three measured misfire guards:
(i) extend only past MINT terminals (a Splice end is an existing
junction — the existing curve owns the continuation; the v513
corridors' splice ends read healthy continuations as spans through
the degenerate connect≈0 of the splice vertex); (ii) the span walk
skips the junction's own carrier; (iii) `attachments` disambiguates
multi-hits by NEAREST junction (extension junctions share band faces
and uniqueness dies — a wrong pick still fails the generators'
orientation-unique checks loudly). Plus the machinery ripple:
`CorridorRepair::absorbed` (extension-certified spans), outward
anchor resolution past fired ∪ absorbed in the phantom generators,
and the (None,None)+hosts fall-through to generator C (the comp-391
anatomy: corner + phantom + span between two hosted rim chords is a
host-to-host excision; the hosted-unattached guard now declines
`HostNotFound` from C's own pair rule instead of
`AttachmentMismatch`).

**Measured (R0044, both gates): `APPLIED corridors=6 plans=20
mints=15 removed=20`, `[451-audit]` silent (zero unpaired directed
edges), the boolean emits its B-Rep, and the case stops at the next
frontier: output tessellation `TessellationFailed FaceId(459), ring
rejected by CDT` (the R0011 FaceId(402) ring-spike family, §3j) with
the §4.5.4 refine retry still walled at v105's ChordDegradation.**
R0011 (3/13/11/13) + R0074 (1/4/2/2): byte-identical APPLIED, both
SUPPORTED_CORRECT (standing is inert on their anatomies — v26/v46
are genuinely displaced, not standing). Gates-off R0044: the honest
ERROR verbatim. 800 lib tests green.

**3b-10 ANCHOR (same session, `[451-farcyc]` census + the
`KV2_RING_REJECT_PROBE` ring dump):** FaceId(459)'s rejected ring
reproduces the far plan's corrected cycle VERBATIM — and the zigzag
is the ORIGINAL pre-repair carried chain (natural-resolution far∩
bands-370..372, verts v90…v116): a FOLD-ORDERED sequence alternating
between two tracks ~40–60 apart (the band width), dozens of cos≈+1.0
reversal spikes, 16 self-intersections in the chart. The retry-density
(2×) carried chain in the same region is monotone. **This is the
paper's §4.5.3 reversed-intersection anatomy** ("the surface
intersection may exhibit a reverse sequence of points after
convergence") — an UNMASKED LATENT: the fold predates every repair;
the case simply never reached output tessellation before the boolean
completed. The landed §4.5.3 sweep (`sweep_reversed_intersections`)
did not act on it — prime suspect (verify first in 3b-10): its
ELIGIBILITY gate scans only cycles whose intersection edges carry
CONIC curves (Circle/Ellipse/LineSegment); the far-cylinder × band
curves here are the degree-4/procedural family (M5 territory), so the
chain was never swept. The standing-junction chain [v90 v91 v92] is
PART of the fold — its vertices sit on true junctions while the
SEQUENCE through them reverses. inc-2c-3b-10 = extend the §4.5.3
correction to the non-conic chains (census the sweep's verdicts on
this loop first; the M5 Option-B procedural surface-pair curve is the
existing vocabulary).

## 3s. inc-2c-3b-10 — the §4.5.3 SURFACE-PAIR tangent arm (LANDED
2026-08-31, fourth session; the FaceId(459) ring wall falls — the case
moves to FaceId(626) "patch triangulation folded")

The `YANG_453_PAIR=census` + `YANG_T145_SWEEP_PROBE` run adjudicated
the 3b-10 anchor's suspect one layer deeper: the fold sites ARE
detected by the task-#145 probe — `[t145-sweep] mixed-cycle conic
U-turn skip … cn=SurfacePair { a: Cylinder(r=2327.8), b: Cone(…) }`,
890 lines — and the #145 shared-conic parameter arm IS landed, but
`mixed_cycle_shared_conic` branch-12-skips `Curve::SurfacePair` and
`conic_param_deltas` has no parameterization for it (branch 11): the
M5 Option-B procedural curves (the far cylinder × the CONE bands)
were vocabulary-invisible to the sweep. The bands are cones — one
half-angle per band — and the fold chain's edges all carry
SurfacePair curves.

Landed (always-on, the #145 branch table unchanged):

- `mixed_cycle_shared_conic` admits `SurfacePair` — identity = exact
  surface equality, unordered (storage order is a frame choice).
- `conic_param_deltas` gains the TANGENT arm: at a SurfacePair site
  the curve's OWN tangent at `p_r` is exact — T = n_a × n_b from the
  two surface gradients — and the deltas are the signed tangent
  projections of the neighbours (lengths, no wrap). This is the
  paper's "progress along the intersection curve" evaluated
  analytically AT the site: a coarse-but-monotone chain has its
  neighbours on OPPOSITE sides of T and reads healthy whatever its
  turn angle, so the P10-disproven angle-band false-positive class
  cannot arise. Degenerate/unreadable tangent = branch 11 (cannot
  diagnose). Victim selection and the 2·d_ε resolution gate are the
  existing shared path, verbatim.

**Measured (R0044, transit+torus+SPAIR gates): the fires, the APPLIED
line (6/20/15/20), and the retry's v105 wall are all byte-identical;
the FaceId(459) ring wall FALLS (the fold chain is §4.5.3-collapsed
before topology), and the case stops at the next output-tessellation
frontier — `FaceId(626): patch triangulation folded (inverted
triangle)`, measured by `KV2_CHORD_DEPTH_CENSUS`: `kind=dev
w_facet=325.9 r_unroll=3683.1 n_split=1 max_split_dev=3.60
max_chord_sag=2.4e-12 min_h2d=1.18 fold=inverted` — the KV9-F2
inverted arm with dev ≫ sag despite the F2b lift-faithful refinement
criterion, on a near-sliver 2D height (min_h2d 1.18).** R0011
(3/13/11/13) + R0074 (1/4/2/2): byte-identical, both
SUPPORTED_CORRECT under the arm.

**RESOLVED (inc-2c-3b-11, 2026-08-31): FaceId(626) = the one-sided
conforming-insert fold — kernel-v2 inc-8b closes it.** The full
anatomy, measured (`[chain-probe]`+`[chain-node]` final chart table,
`KV2_PATCH_FOLD_PROBE`, `[conform-pt]`): face 626 is a LEGITIMATE
carried B-band — a ~304° near-sliver cone strip (tan α=1.596)
between coaxial rims r=3681.154/3683.060, chart width 1.19 vs facet
chord sag 3.60. The R0054 grid alignment rung-pairs the two rails
bitwise for the whole strip EXCEPT one node: a face-627 boundary
vertex at d3=1.656 from rail 1's circle (azimuth 58.495Δ) mints a
mechanism-2 conforming insert on rail 1 — and rail 2's pool (its
incident faces are 625/626, not 627) can never see that vertex,
though it sits well inside rail 2's window too. The pool is
EDGE-local; the fold constraint is FACE-local. The unpaired insert
lands mid-chord of rail 2's rung (sag 3.6 ≫ strip 1.19, and the
inversion needs the cone's tilted normal — a cylinder merely
degrades); F2b then fires on the inverted all-on-surface triangle
(dev≈0 < sag ✓), LEPP-splits the rung, and its ArcSample split rule
lerps the mint ON the 3D chord (dev=3.605 — the census's split; the
T-junction closure contract, correct as designed), after which
`dev ≥ sag` rightly refuses to chase the arm's own off-development
node. The fix is NOT the split rule and NOT a band: **kernel-v2
inc-8b** (spec `yang_434_output_chord_refinement.md`) completes the
gated inc-8a curve pool (`KV2_ARC_CONFORM_CURVES`) to depth 1 — a
pool arc contributes its grid samples PLUS its own vertex-pool
inserts (static B-Rep data, no recursion into curves), making the
pool view of an arc EQUAL to its chain view's insert set; the
azimuth-set closure is exact at depth 1. R0044 is inc-8a's FIRST
corpus customer. Unit-pinned both ways
(`pool_curves_carry_their_vertex_inserts_across_the_strip`); gate
off, the fold stays the pinned loud wall. Under
transit+torus+SPAIR+`KV2_ARC_CONFORM_CURVES=1` (the gate set is now
FOUR knobs) face 626 tessellates clean (`n_split=0 fold=0`,
min_h2d=1.19 = the strip width) and the case advances to
**`FaceId(627): ring rejected by CDT`** — an UNMASKED latent (626's
fold was the loud stop hiding it; the R0053 lesson shape, §3o).
ANCHORED same session (`KV2_RING_REJECT_PROBE` +
`[chain-probe]` 627): face 627 is the next band down (rims
3681.154/3548.944) carrying the corner-transit notch itself —
SurfacePair → 2×HyperbolaArc (per-mesh-piece, n_interior=0) → arc
chains. The junction vertex X=(−1813.598774,−2388.465072,
−6104.296761) between the SurfacePair edge and the first
HyperbolaArc is **ON face 627's own cone** (7.7e-6 — the uniform
rim-derived fit residual EVERY ring point shares, not a per-point
defect) but sits **0.827 PAST the station of the r=3681.154 rim that
BOUNDS the face** (h=2124.192 vs the rim's 2123.365). X is on the
EXTENDED surface, in the parametric band belonging to the NEIGHBOUR
face 626: in 626's own frame X lands at h=2307.550, INSIDE 626's
[2306.722, 2307.916] band (0.827 past the shared rail of a 1.194-wide
strip), 0.060 off 626's cone — the crease departure between two cones
(tan α 1.7336 vs 1.5958). **This is the 3b-8
identification-vs-domain shape at the EMISSION layer**: a point valid
on the extended surface, OUT OF DOMAIN for the face's actual B-Rep
boundary. In the chart the ring therefore pokes above its own top
edge and self-intersects the rim run TWICE — the incoming SurfacePair
edge at u=63.446 (against the negative-going run) and the outgoing
hyperbola edge at u=65.738 (against the positive-going run) — so the
CDT rejects CORRECTLY, over a yang-side emission defect. The rim node
at X's exact azimuth (Δu=7e-13) is X's OWN always-on mechanism-2
vertex insert on 627's chain, so the crossing exists with or without
inc-8b. Face 626 carries NO notch (a clean 8-edge loop: 6 arcs + 2
line segments), so the crossing's transit into the 626 band was never
built — now the MEASURED leading reading of the three this anchor
opened.**

**ADJUDICATED same session — the transit reading WINS, exactly
(`[chain-probe]` SurfacePair/HyperbolaArc parameter dump + closed-form
root solve).** The three surfaces meeting at X are named: the
SurfacePair is **far Cylinder** (r=2327.818) **× cone 627**, and the
HyperbolaArc's normal is exactly −(cylinder axis), so it is the
cylinder's **CAP PLANE × cone 627**. X is therefore the
cylinder-end-circle ∩ cone triple point — and it is EXACT: plane
residual −2.0e-13, cylinder 4.5e-13, cone 4.5e-13, and the
closed-form solve for that circle × cone 627 returns X itself to
**8.2e-13**. **X is not a stray or mis-relocated vertex; it is the
correctly-computed triple point of the EXTENDED cone 627, placed
0.827 outside 627's own domain.** The identification is right and the
domain check is missing — the 3b-8 lesson, verbatim, at the emission
layer.

The true corner topology, all of it measured:
- **The real junction J EXISTS on face 626**: the same cylinder end
  circle × **cone 626** has a root at station h=2307.654 — INSIDE
  626's [2306.722, 2307.916] band (0.932 past the shared rail, 0.262
  short of the far rail), 0.138 from X. J is where the junction
  belongs.
- **Both crease crossings EXIST on the shared rim** (r=3681.154):
  the rim meets the cylinder LATERAL at
  P_lat=(−1814.091366,−2390.548529,−6103.426403) (2.311 from X) and
  the CAP PLANE at
  P_cap=(−1813.637849,−2388.505205,−6102.509823) (1.788 from X).
- So the correct emission is: 627's SurfacePair chain terminates at
  P_lat and its hyperbola chain at P_cap (both ON the crease); the
  shared rim splits at both; and **face 626 gains the notch** —
  lateral∩cone626 from P_lat to J, cap∩cone626 from J to P_cap. Today
  626 carries NO notch (a clean 8-edge loop: 6 arcs + 2 line
  segments) and 627 carries the whole excursion on its extended
  surface. The excursion is small but real: chart width 2.29, height
  0.827.

**inc-2c-3b-12 is therefore a DETERMINED build, not an
investigation**: split the crease at P_lat/P_cap, re-terminate 627's
two chains there, and construct the 626-side notch through J. The
domain postcondition it needs is the §4-I9 shape already in the
stack — a chain terminal must be certified against its OWN face's
bounding-rim domain, not merely against its surface (the
identification-vs-domain rule, now with a second measured customer).**

*Correction, same session:* an earlier draft of this block recorded X
as "0.15 off face 627's own cone", an UNMEASURED figure. The measured
values are above: X is on 627's cone and out of 627's DOMAIN. The
error inverted the diagnosis (an off-surface emission would be a
geometry defect; an out-of-domain terminal is a topology/transit one)
— the 3b-8 lesson, incurred again by asserting a distance without
computing it.

**GATED `YANG_453_SPAIR` (default OFF) by the always-on corpus run's
verdict: ONE E→W flip — R0053, the M8 coplanar-graze case.** Its
fold's ring rejection (`FaceId(474)`) was the loud stop masking the
coplanar capability gap; with the fold collapsed the case COMPLETES
at χ=0 against the authored 2 (`mesh_euler_characteristic` — the
composition oracle keeps it loud as W, but the canonical bar is
0 WRONG, enforced). The arm itself behaved exactly as designed there
— the adjudication owed is R0053's: either its χ expectation is
oracle authoring (the R0011 `euler_target` precedent — true topology
changed by the graze) or the completion is a genuine M8 silent-wrong
(then the flip waits on M8 Stage-0). **The flip condition is that
adjudication — never a narrower band.** Default-off corpus:
byte-identical canonical 273C/0W/34E/1EE/0T (re-proven).

## 3t. inc-2c-3b-12 — the relocation DOMAIN certificate (LANDED, GATED)

**The defect, anchored empirically** (`YANG_TRIPLE_WATCH` backtrace on
the exact solution): R0044's out-of-domain junction X is minted by
`stage4_relocate_and_correct_inner`'s **triple-junction relocation
arm** — `relocate_onto_implicit_triple(seed, Cylinder, Plane, Cone627)`
on mesh vertex **v47**, travel 18.07. The arm's only acceptance gate is
the displacement corridor `tangent_plane_corridor(d_eps, sin θ)`, which
passes it easily: across the case's 306 triple relocations the
displacement distribution is p25 3.4 / p50 6.1 / p75 13.9 / max 182.8,
so 18.07 sits near p80 and is in no way anomalous. **The arm has no
domain postcondition at all** — it accepts any exact solution of the
three EXTENDED implicits.

**Why the check must be ANALYTIC, not mesh-derived.** v47's seed lies
17.98 from the cylinder, 10.53 from the crease circle, and 5.09 from
its own cone; all four exact candidate features (X, the true junction
J, and the two crease crossings P_lat/P_cap) sit 16.6–18.4 away, i.e.
inside ONE mesh chord. A mesh-based containment test cannot separate a
0.827 overrun from a legitimate landing at that resolution. The crease
is analytic, so the certificate is too.

**The paper is explicit here, and this is its stated trigger** —
§4.5.1, `refs/text/yang2025_hybrid_boolean.txt:672-690`: *"Instead of
taking a full step length that takes the point to a position `p1`
**outside the surface `S2`** where the point is initially located, we
truncate the step so that the point moves to `p` on the boundary curve
`C_b` between `S2` and the neighboring surface `S1`. In the next
iteration, the optimization step of `p` is computed using the
parameterization of `S1` … After obtaining the correct position of
`p`, we first solve the intersection points `q1` and `q2` on `C_b`."*
That is, in order: the domain test, the truncation, the transit, and
the q-points — P_lat and P_cap being exactly the q-points here.
`stage4_truncate`'s own header predicted this join: it records that its
mechanism and §4.5.1's stated trigger "can be joined without borrowing"
for the class that converges exactly *as equations* but not *within its
domain*. v47 is precisely that class.

**Built (detection half only), gated `YANG_451_TRIPLE_DOMAIN`**
(unset/`0`/`off` = OFF and byte-identical, `census` = report and
continue, `1`/`on` = typed STOP `RelocationCrossedCrease`):

1. **`crease_circle_from_pair`** (`stage4_boundary_curve.rs`) — the
   analytic boundary curve `C_b`, generalizing `rim_circle_from_pair`
   past Cylinder×Plane to **Cone×Cone** (coaxial, distinct openings:
   `h·tanα₀ = (h+δ)·tanα₁`), **Cone×Plane** (⊥ axis) and
   **Cylinder×Cone** (coaxial). Only configurations whose intersection
   is exactly a CIRCLE are answered; everything else declines rather
   than approximating (a near-coaxial pair meets in a quartic).
2. **`creases_by_surface` / `creases_for_surfaces`** — the crease index
   keyed BY SURFACE, built once per stage. Sourcing this from edges at
   the moving vertex was measured WRONG and fixed: v47 sits 10.5 from
   the crease it overruns, so a vertex-incident sourcing sees nothing.
   The domain a relocation must not leave belongs to the FACE.
3. **`crease_crossed_by_step`** — the certificate. A step violates its
   domain when the pre- and post-positions lie on strictly OPPOSITE
   sides of a crease plane bounding one of the vertex's own surfaces.
   Two exemptions, both membership statements rather than thresholds:
   a vertex that lies ON the crease (`on_crease`: satisfies BOTH
   forming surfaces within their own `junction_certificate_band`) may
   glide along it; and the residual band is **PROPAGATED** — the crease
   plane is DERIVED, so its band is its own plus both parents'. The
   plane's band alone understates the construction badly (its reference
   magnitude omits the crease radius entirely, and for a
   near-cylindrical cone omits an apex magnitude four orders larger
   than the geometry it describes).

**Measured on R0044** (`census`): **8 fires, 5 noise fires eliminated by
the propagated band.** The two populations separate by TEN orders —
material overruns 0.309 … 40.08 against bands of order 1e-11:

| v | ρ | d_pre | d_post |
|---|---|---|---|
| 47 | 18.07 | −0.194 | +0.827 |
| 75 | 84.95 | +6.054 | −30.653 |
| 76 | 60.43 | +16.200 | −31.727 |
| 89 | 58.60 | +40.078 | −12.133 |
| 38 | 7.60 | +4.989 | −0.309 |
| 39 | 7.56 | +0.984 | −4.197 |
| 59 | 9.76 | +1.129 | −4.983 |
| 105 | 5.62 | +2.452 | −2.584 |

The five exempted rode a crease with residuals of 1.6e-11 … 1.4e-10 at
both ends, sign meaningless. **v105 is a cross-confirmation**: the
independent §4-I9 carrier-domain check (`YANG_S4_CARRIER_DOMAIN`) fires
on that same vertex by a different mechanism, and v105 is also the
§4.5.4 retry's own `ChordDegradation` wall — three unrelated
instruments naming one site.

7 unit tests (`tests_unit/s451_crease_domain.rs`) pin the crease
geometry (both directions, including the declines), the material-overrun
fire, the noise exemption at R0044's own magnitudes, the on-crease
exemption, and the by-surface sourcing.

**FULL-CORPUS CENSUS (312 cases, `census` mode).** Categorized score is
the canonical **273C/0W/34E/1EE/0T** — census mode is behaviour-neutral
corpus-wide, as built. The certificate fires in **exactly TWO cases**:

| case | verdict | fires | overrun range |
|---|---|---|---|
| R0044 | ERROR | 8 | 0.309 … 40.08 |
| R0003 | **SUPPORTED_CORRECT** | 6 | 0.00078 … 0.265 |

R0003's six (v1983, v7611, v8658, v8809, v9336, v11356; ρ 0.0043 …
0.459) are the increment's most consequential measurement: **a
SUPPORTED_CORRECT case carries genuine out-of-domain triple relocations
and still produces correct output.** So the condition is real but not
always fatal — it is fatal in R0044 (where the overrun self-intersects
face 627's output ring) and silent in R0003. The same shape as the F2a
chord census, where "folding is the sliver lottery on a UBIQUITOUS
depth defect": the defect is the overrun, and whether it kills the case
is downstream luck.

**Consequence, and it is binding: the STOP must never be armed
always-on as it stands** — it would convert R0003 from CORRECT to
ERROR, breaking the 0W/273C bar for a defect that case survives. The
certificate's role is diagnosis and the repair's precondition, not a
wall.

**A discriminator deliberately NOT taken.** R0003's largest overrun
(0.265) and R0044's smallest (0.309) happen not to overlap. That gap is
1.2×, sits on six and eight samples, and separates nothing structural —
using it would be exactly the band-tuning P10 forbids, and it would
"squeak a case through" while leaving the defect in place. The two
populations are the same defect at different magnitudes; only the
repair distinguishes them, by fixing both.

**Not built here — the repair (3b-12b).** Detection only. The paper's
remaining three steps are: truncate the step to `C_b`, transit onto the
neighbouring surface (which for v47 yields J, the true junction, on
cone 626 at station 2307.654 INSIDE 626's band), and solve the
q-points P_lat/P_cap on `C_b`, splitting the crease there so face 627's
two chains terminate on it and face 626 receives the notch it currently
lacks. The gate stays OFF until that arm exists: today the certificate
would only convert one confusing downstream stop (a rejected output
ring on a different face) into a precise local one, which is worth
having but is not a conversion.

## 3u. inc-2c-3b-12b-0 — the REPAIR SOLVER (LANDED, pure; census-only)

The §3t certificate answers *which* crease a relocation crossed. This
increment answers what the paper prescribes next, as a PURE solve:
`solve_crease_transit` (`stage4_boundary_curve.rs`) executes §4.5.1's
four steps in the paper's own order, and nothing else — no mesh
mutation, no topology side effect, no wiring into the default path.

1. **Truncate to `C_b`.** The signed distance to a plane is affine along
   a segment, so the crossing parameter is exact in one division; the
   paper's `p` is then the nearest point of the crease CIRCLE to that
   crossing (`project_onto_curve`, which returns `None` exactly on the
   axis — the one place it is not unique).
2. **Transit onto `S1`.** Re-solve the triple with `s_own` replaced by
   the neighbouring surface, seeded at `p`. This is the paper's *"the
   optimization step of `p` is computed using the parameterization of
   `S1`"*. The neighbour needs no new machinery: the §3t crease index
   already carries it as the pair's other surface.
3. **Certify.** The result must satisfy all three of its own surfaces
   (`satisfies_all_surfaces`) AND must not itself leave `S1`'s domain —
   the same certificate re-applied to the corrected step. Without that
   postcondition a "repair" could simply carry the overrun one face
   further; with it, a multi-crease transit is a typed DECLINE rather
   than an iteration into unmeasured territory.
4. **The `q`-points on `C_b`.** Each of the two other surfaces is solved
   against the crease circle exactly (`circle_surface_roots`, ALL roots,
   already built for the inc-2c-0 step solver), taking the root nearest
   the corrected junction. The selection MARGIN — how much closer the
   winner is — is carried on the result, so a near-tie is reported
   rather than hidden.

Every non-answer is typed (`CreaseTransitFailure`), and the declines
carry their own measured residuals rather than merely naming themselves.

**VALIDATION — the solver reproduces §3s's independently measured
values.** Those numbers were derived in the previous increment by a
different route (a closed-form circle x cone solve and a by-hand station
comparison), before this code existed:

| quantity | §3s recorded | 3b-12b-0 solver |
|---|---|---|
| correction \|X − J\| | 0.138 | `1.387470e-1` |
| `q1` = P_lat | (−1814.091366, −2390.548529, −6103.426403) | agrees to **1.4e-6** — its last recorded digit |
| `q2` = P_cap | (−1813.637849, −2388.505205, −6102.509823) | agrees to **1.1e-6** — its last recorded digit |

**Two independent cross-validations fell out of the census**, neither
designed for:

* v38's `q2` and v47's `j` agree to **6.4e-13**, against an evaluation
  band of ~1.1e-11 at that coordinate magnitude — the SAME physical
  point reached by two unrelated paths. It must be: v47's corrected
  junction lies on the cylinder end circle (cylinder ∩ cap plane), which
  is exactly the crease v38 crosses, so v47's `j` is v38's cone-626
  `q`-point. (They are not bit-identical, and the record says 6.4e-13
  rather than "identical".)
* R0003's v8658 and v11356 — two mesh vertices at one junction — return
  junctions agreeing to **2.8e-14**.

**CENSUS (both firing cases). 11 of the 14 out-of-domain sites have a
DETERMINED repair.**

| case | verdict | fires | determined | corrections |
|---|---|---|---|---|
| R0044 | ERROR | 8 | **5** (v47, v89, v38, v39, v105) | 0.138, 0.747, 0.642, 8.72, 0.157 |
| R0003 | SUPPORTED_CORRECT | 6 | **6** (all) | 1.48e-3 … 3.82e-2 |

The three R0044 declines are all `TransitLeavesNeighbour`, each with a
material second crossing — v75 (+11.17 → −17.25), v76 (+1.83 → −30.46),
v59 (+3.86 → −0.307). They are honest declines, not solver artifacts:
those steps overrun 17–30 units past a second crease, so a single
transit does not reach their true junction.

**What this measures for the eventual flip.** §3t's binding constraint
was that the STOP can never be armed, because R0003 carries the same
defect and survives it. The repair does not inherit that constraint: it
is DETERMINED on all six of R0003's sites, at corrections of 1e-3 …
4e-2 — sub-chord nudges toward positions that are, by construction,
exactly on all three of their surfaces and inside their face's domain.
That is the shape §3t asked for — *"only the repair distinguishes them,
by fixing both"* — rather than a magnitude discriminator between them.

**NOT built here: the emission half (3b-12b-1).** Moving v47 to `J`
alone does NOT fix R0044's `FaceId(627)`, and the census makes the
reason precise: `J` lies on cone 626 BY CONSTRUCTION, so it is no more
inside 627's domain than `X` was (both sit past 627's bounding rim; §3s
measured `X` at 627-station 2124.192 against a rim at 2123.365). The
repair is not a relocation but a RE-TERMINATION: face 627's two chains
must end at `q1`/`q2`, which are ON the crease that bounds 627, the
crease must split there, and the span beyond belongs to face 626 as the
notch it currently lacks (through `J`). That is topology construction
across two faces, and it is the next increment. This one exists to make
that build determined rather than exploratory — and to establish, before
any of it is wired, that the analytic half reproduces values measured
independently of it.

## 3v. inc-2c-3b-12b-1 — the EMISSION-half site ANATOMY (LANDED, pure;
census-only)

§3u's solver answers WHERE the corner belongs. This answers WHAT the mesh has
there today — the thing the emission half must edit — and it is a measurement,
not a plan. `transit_site_anatomy` (`stage4_boundary_curve.rs`) is pure and
returns only counts and distances:

* the site's incident fan, each triangle with its input-face attribution and
  its two other corners' crease-plane distances;
* the one ring, each neighbour classified `Home` / `On` / `Past` — membership
  first (`on_crease`, the trigger's own exemption), then sign;
* the distinct input faces in the fan with their counts, descending;
* for each q-point, the mesh edge lying ON the crease that is nearest it, with
  that edge's `len`, the q-point's `dist` from it, and whether it is IN the
  fan. The q-point is exact on the crease circle, so `dist` is the mesh
  chain's own sag, never an error in `q`.

Reachable only under `YANG_451_TRANSIT_ANATOMY` inside the existing
`YANG_451_TRIPLE_DOMAIN=census` block; the default path is untouched.

**CENSUS — all 11 determined sites, both firing cases.**

| case | v | fan | home/on/past | fan faces | q-host edge | len | q sag |
|---|---|---|---|---|---|---|---|
| R0044 | 47 | 6 | 4/2/0 | (B,168)×4, (A,2), (A,3) | 981–6911 **in fan** | 558.53 | 10.389 / 10.363 |
| R0044 | 89 | 5 | 3/1/1 | (A,2)×2, (B,1)×2, (B,381) | 1194–14132 | 141.01 | 1.348 / 4.8e-12 |
| R0044 | 38 | 5 | 4/1/0 | (A,2)×3, (B,166), (B,167) | 9–11 | 294.44 | **497.87 / 497.81** |
| R0044 | 39 | 5 | 2/1/2 | (A,2)×3, (B,165), (B,166) | 9–11 | 294.44 | **497.87 / 498.95** |
| R0044 | 105 | 7 | 4/1/2 | (A,2)×5, (B,1), (B,360) | 1339–16095 | 184.43 | 0.217 / 6.1e-12 |
| R0003 | 1983 | 6 | 2/4/0 | (A,8)×4, (B,0), (B,4) | 1963–1967 / 1965–1968 **in fan** | 21.22 / 8.36 | 1.7e-12 / 2.5e-13 |
| R0003 | 7611 | 6 | 4/2/0 | (A,324)×4, (B,0), (B,3) | 7608–7609 | 24.34 | 7.470 / 7.956 |
| R0003 | 8658 | 9 | 5/4/0 | (A,376)×4, (B,4)×3, (B,0)×2 | 8638–8642 / 8640–8643 **in fan** | 19.18 / 11.04 | 2.6e-14 / 2.2e-14 |
| R0003 | 8809 | 6 | 4/2/0 | (A,386)×4, (B,0), (B,4) | 8811–8813 | 29.94 | 10.532 / 9.194 |
| R0003 | 9336 | 7 | 1/2/4 | (A,419)×4, (B,0)×2, (B,4) | 9338–9340 | 29.49 | 9.121 / 7.969 |
| R0003 | 11356 | 6 | 4/2/0 | (A,376)×4, (B,0), (B,4) | 11328–11332 / 11330–11333 **in fan** | 19.18 / 1.00 | 7.6e-15 / 6.5e-14 |

**Reading 1 — the anatomy is ONE shape, eleven times.** Every site's fan
straddles EXACTLY three input faces: one dominant patch (4–5 triangles) and
two others. That is the corner itself — two chains leaving the vertex,
separating three patches — and it is the same object in the ERROR case and in
the SUPPORTED_CORRECT one. So the repair's edit unit is that three-face fan,
and it is not R0044-shaped.

For v47 the three are named exactly: `(B,168)` is the cone patch (4
triangles), `(A,2)` the cylinder lateral and `(A,3)` the cylinder cap, ONE
triangle each. The fan cycle is `45 → 44 → 280 → 981 → 6911 → 6945 → 45`, so
edge `v47–45` separates lateral from cone (the SurfacePair chain) and
`v47–280` separates cap from cone (the hyperbola chain) — the two chains the
emission half has to re-terminate, identified from the attribution rather than
from the curve labels.

**Reading 2 — the `Past` neighbours are already-relocated SIBLINGS, not
independent defects.** R0044's v39 ring carries v38 at `−3.092587e-1`, which
is v38's own `d_post` from §3t's table against the SAME crease (`1/5`), bit
for bit: the loop relocates v38 before it examines v39. So R0044's v38 / v39 /
v59 are one adjacent cluster, and a per-site repair would edit a fan whose
neighbour has already moved. The repair unit for that cluster is the cluster —
the same lesson §3d reached for the corridor, now at the emission layer.

**Reading 3 — and this is the consequential one — the q-points'
REPRESENTABILITY splits the population, structurally.** Three distinct
situations, and only the first is a pure relocation:

1. **The mesh already carries the corner** (R0003 v1983, v8658, v11356): both
   q-points ARE existing one-ring vertices of the site, agreeing to
   **7.6e-15 … 1.7e-12** (host parameter `t = 1`, i.e. the edge's far
   endpoint, and every one of them classified `On`). The q-points there need
   no insertion. *(This block originally read "the repair is the relocation
   and nothing else". §3w Reading 5 supersedes that: the q-points need no
   insertion, but the site's FAN still reaches crease-chain vertices 5.5°
   outside the corner, so the cut is non-monotone at these sites too and the
   fan needs refining.)*
2. **The crease is carried but too coarsely** (R0044 v47): the host edge is in
   the fan and on the crease, but it is a **558.53-long rim chord with the
   q-points 10.39 / 10.36 off it**, at `t ≈ 0.428` — mid-chord, where the sag
   is largest. The two q-points are 2.285 apart on a chord 558 long. Splitting
   this is a REFINEMENT of the rim chain against its analytic circle, not an
   insertion into it.
3. **The crease is not carried at all near the site** (the remaining 7). For
   R0044 v38/v39 the nearest crease-carrying edge in the whole mesh is
   **497.9 away** and is 294.4 long, while their own q-points sit ~7 from the
   site: the crease they overrun (`1/5`, the cylinder's own end circle) has no
   local mesh representation whatsoever.

This is the discriminator §3t went looking for and correctly refused to find
in a magnitude band. The two populations are not separated by how far they
overrun; they are separated by **whether the mesh carries the crease at the
resolution the corner needs**. R0003 survives its out-of-domain relocations
with the corner already present in the mesh; R0044 does not.

**The corrected topology of `FaceId(627)`, from the emitted loop's own
coordinates.** Charting all 13 half-edges into face 627's cone frame (station
`h`, radius, azimuth about the shared axis):

| node | h | radius | azimuth |
|---|---|---|---|
| B (notch start) | 2047.1031 | 3548.9438 | −65.705° |
| A (SurfacePair) | 2090.5715 | 3624.3025 | −64.418° |
| **X (notch tip)** | **2124.1922** | 3682.5887 | −63.406° |
| C (hyperbola) | 2080.7189 | 3607.2216 | −62.840° |
| D (notch end) | 2047.1031 | 3548.9438 | −62.382° |
| E1 → E2 (generator) | 2047.1031 → 2123.3647 | | −59.705° |
| H → I (generator) | 2123.3647 → 2047.1031 | | −4.263° |
| q1 = P_lat | 2123.3647 | 3681.1541 | −63.4308° |
| q2 = P_cap | 2123.3647 | 3681.1541 | −63.3952° |

The band is `h ∈ [2047.1031, 2123.3647]` and the face is a **304.56° sector**
of it, closed by the two generators. Three facts follow, all computed:

* the notch spans the FULL band height — both its ends, B and D, are exactly
  on the LOWER rim (2047.1031 / 3548.9438), not merely near it;
* its tip exceeds the TOP rim by **0.827** (2124.1922 − 2123.3647);
* the exit window on the top rim is **0.0356° = 2.285 of arc**, which is §3s's
  independently measured "chart width 2.29", now reproduced from the loop's
  own coordinates; and the loop's own top-rim run (E2 at −59.705° sweeping to
  H at −4.263°) covers the notch's azimuth, which is exactly why the ring
  self-intersects it twice.

Because the notch already reaches the bottom rim, the corrected emission does
not merely DENT face 627's top edge — it **cuts the sector in two**: a small
piece spanning azimuth [−63.395°, −59.705°] (hyperbola leg, bottom rim D→E1,
generator, top rim back to q2) and the rest [−63.431° → the long way →
−4.263°]. Face 626 is dented only, as §3s measured (`J` is 0.932 past the
shared rail of a 1.194-wide strip and 0.262 short of the far one), and its
loop is confirmed here to be the clean 8-edge one — 6 arcs + 2 line segments,
sharing 627's top-rim nodes H/G/F/E2 in reverse.

**That face split is not something the repair constructs.** It falls out of
`flood_fill_patches` once the mesh past the crease is re-attributed: the
patch simply has two components. Which is the argument for doing the emission
half in the MESH and letting Stage 5 emit what it finds, rather than editing
loops — and it is why the anatomy above (fan, attribution, crease chain) is
the right thing to have measured first.

**Not built here.** The mesh edit. The census says it is three builds, not
one, and names the cheapest: situation 1 is a relocation with no surgery, and
it covers 3 of the 11 sites — all in the case that is already CORRECT, so it
converts nothing on its own and is the honest place to prove the machinery.
Situation 2 (v47, the case that actually fails) needs the rim chain refined at
two analytically determined points before the corner is representable at all.

## 3w. inc-2c-3b-12b-2 — the CUT PATH across the own patch (LANDED, pure;
census-only)

§3v measured what the mesh has at a site. This turns that into the EDIT, still
as a pure function returning crossings rather than mutations:
`transit_cut_path` walks the fan, identifies the site's own patch by SURFACE
(resolving each triangle's `(input, face)` attribution against the input
BRep), and returns the arc the crease cuts across it — from one chain
termination to the other — plus which own triangles that arc splits and which
cross wholesale. Gated with §3v under `YANG_451_TRANSIT_ANATOMY`.

**Reading 1 — the corner has THREE chains, and they do not play the same
role.** The first model of this function required two and declined at every
one of the 11 sites (`ChainCount { found: 3 }` at R0044 v47/v38 and R0003
v7611/v8809; `found: 1` at R0003 v1983/v8658/v11356). The census was right and
the model was wrong:

* two chains involve the OWN surface (own × other). They cross the crease, and
  their crossings ARE the repair's q-points — which is what makes those points
  the re-termination targets;
* the third joins the two OTHER surfaces, never involves the own face, and so
  never meets the crease as a termination. It is the **CARRIER**: the
  correction `X → J` is a step ALONG that curve.

For R0044's v47 the carrier is the edge `v47–44`, the cylinder's own END
CIRCLE — and `X` and `J` are exactly its intersections with cone 627 and cone
626 (§3s). So the 0.138 correction is a glide along the carrier, not a jump
across open space, and the emission half must not split that edge.

With the three-chain model, **7 of the 11 sites yield a determined cut**; the
other 4 are exactly the sites with a `Past` neighbour.

| case | v | verdict | carrier | past | split | `Refined` lift | q dist (q0/q1) | cut span | q gap |
|---|---|---|---|---|---|---|---|---|---|
| R0044 | 47 | OK | 44 | 1 | 3 | 0.1435 | 0.786 / 0.447 | 1115.17 | 2.285 |
| R0044 | 38 | OK | 8182 | 0 | 3 | 0.9795 | 0.799 / 0.393 | 184.55 | 0.0600 |
| R0044 | 89 | `PastNeighbour{1192}` | | | | | | | |
| R0044 | 39 | `PastNeighbour{38}` | | | | | | | |
| R0044 | 105 | `PastNeighbour{130}` | | | | | | | |
| R0003 | 1983 | OK | 2134 | 2 | 2 | 1.53e-2 | **at vertices**: 1.7e-12 / 2.5e-13 | 58.84 | 0.3104 |
| R0003 | 7611 | OK | 7549 | 0 | 4 | 1.40e-3 | 0.451 / 0.802 | 49.20 | 0.4866 |
| R0003 | 8658 | OK | 8809 | 2 | 2 | 4.99e-3 | **at vertices**: 9.5e-14 / 4.2e-14 | 60.35 | 0.1080 |
| R0003 | 8809 | OK | 8658 | 0 | 4 | 5.70e-2 | 0.553 / 1.305 | 61.25 | 1.3390 |
| R0003 | 9336 | `PastNeighbour{8959}` | | | | | | | |
| R0003 | 11356 | OK | 11610 | 2 | 2 | 3.98e-5 | **at vertices**: 6.2e-14 / 7.7e-14 | 40.26 | 0.1080 |

*The site's out-of-domain position is passed to the cut EXPLICITLY rather than
read from the mesh. The caller detects the defect before committing the
relocation, so the mesh still holds the seed, on the HOME side of the crease —
and a crossing computed from there is an extrapolation behind the site rather
than a crease crossing. A first version of this table was computed that way;
its `dist` and `lift` columns were wrong by the whole relocation (e.g. v47's
lift read 10.18 rather than 0.1435), and the "the lift reproduces §3v's
rim-chord sag" cross-validation it appeared to show was an artifact of that
error, now RETRACTED. The structural columns — verdict, carrier, past/split —
are position-independent and are unchanged.*

**Reading 2 — matching a chain to its q-point by PROXIMITY is wrong at three
of the four sites where both chains are crossed edges**: the recorded margin
is NEGATIVE for one of the two chains at R0044 v38 (−0.0526), R0003 v7611
(−0.361) and v8809 (−0.775). (At v47 both margins are positive, so proximity
happens to agree there.) The rule that works is IDENTITY: `q[i]` is where the
site's `others[i]` meets the crease, so the chain whose other face IS
`others[i]` is the one that terminates there. `CreaseTransit` now carries
`others` alongside `q1`/`q2` so that correspondence is transported rather than
re-derived, and a chain whose face does not resolve to one of them is a typed
`QSurfaceUnmatched` decline — never resolved by distance. Same
membership-over-band discipline as §3t's `on_crease` exemption and 3b-4's
curve-aware host admission.

**Reading 3 — the cut has ONE shape at all 7 determined sites.** Every one is

```
q-termination → (Vertex | Refined)* → q-termination
```

with **exactly one `Refined` crossing**. So the edit per site is: two
q-terminations — already present as mesh vertices at 3 sites, to
4.2e-14 … 1.7e-12 — and exactly ONE new vertex on the crease circle, whose
`lift` off the chord is 4e-5 … 0.98.

**Reading 4 — the declines are a clean structural partition.** All four are
`PastNeighbour`, i.e. exactly the sites §3v identified as having an
already-relocated sibling in their one ring. There is no third failure mode
among the 11: the population is 7 v-local corners and 4 cluster corners.

**Reading 5 — and this is the increment's binding result — EVERY cut is
NON-MONOTONE along the crease.** Each node lies on the crease circle by
construction, so its angle there is well defined, and the sequence in cut
order is:

| case | v | node angles along the crease (degrees, cut order) | q gap |
|---|---|---|---|
| R0044 | 47 | −63.395, **−59.705**, **−68.407**, −63.457, −63.431 | 0.036° |
| R0044 | 38 | 154.670, **152.442**, 154.713, 154.669 | 0.0015° |
| R0003 | 1983 | −10.447, **−4.190**, −10.603, **−12.819**, −10.356 | 0.091° |
| R0003 | 7611 | −15.465, **−21.448**, −15.442, **−12.819**, −15.637 | 0.17° |
| R0003 | 8658 | −9.687, **−4.190**, −9.713, **−12.819**, −9.656 | 0.031° |
| R0003 | 8809 | 9.786, **12.819**, 10.762, **4.190**, 10.171 | 0.39° |
| R0003 | 11356 | −9.687, **−4.190**, −9.665, −9.943, −9.656 | 0.031° |

The TRUE cut is the crease arc BETWEEN the two q-points — a sweep of 0.0015°
to 0.39°. Every measured cut instead leaves that interval by 0.04° to 6.3° and
comes back, because the site's one ring reaches crease-chain vertices well
outside the corner. The `span` column says the same in length: 40 … 1115 of
cut against q gaps of 0.06 … 2.3.

**The reason, measured**: the gap between a site's two `On` interior
neighbours — i.e. the crease chain's own step there — is

| site | chain step |
|---|---|
| R0044 v47 | 8.70164° |
| R0003 v1983 / v7611 / v8658 / v8809 | 8.62890 / 8.62891 / 8.62890 / 8.62890° |
| R0003 v11356 | 5.75260° = exactly ⅔ of 8.62890° |

against corners of 0.0015° … 0.39°. **The crease chain is two to three orders
COARSER than the corner it has to carry**, uniformly, in both cases.

*(A genuine consistency check, replacing the retracted one: R0044 v47's chain
step derived from the ANGLES is 8.701640°, and derived from §3v's independently
recorded chord LENGTH of that same edge — 558.5283 on a crease radius of
3681.15411531651 — it is 8.701643°. Agreement to 3.0e-6°, which is what
confirms both chain endpoints lie on the analytic crease circle rather than
merely near it.)*

So the emission half is **not** a re-attribution of existing triangles at any
of the 11 sites, including the three where the q-points already exist as
vertices. Those three have their q-points adjacent on the chain (v11356's are
0.031° apart) but their FANS still reach 5.5° away, so the boundary the
re-attribution would hand Stage 5 doubles back along the arc it is supposed to
represent. This supersedes §3v's reading that situation 1 needs "no mesh
surgery at all": the q-points need none, but the fan does.

**Not built here.** The mutation — and Reading 5 changes what it has to be.
The enumerated edit (relocate the site along its carrier to `J`; insert the
two q-points, skipping those the mesh already has; insert the single
refinement vertex on the crease circle; split the named triangles;
re-attribute the own-patch triangles between the cut and the site, after which
the face split §3v derived falls out of `flood_fill_patches`) is necessary but
NOT sufficient: it re-attributes the fan's past side, whose boundary is the
non-monotone cut above. The missing precondition is a LOCAL REFINEMENT of the
site's own fan so that its one ring brackets the corner instead of straddling
it — which is Yang §4.5.2's local refinement, applied at an analytically
determined place rather than as a density ladder. (The 2026-08-29 census
adjudicated the *global* ladder out precisely because it refines everywhere
and resolves nothing; this is the opposite instrument.)

## 3x. inc-2c-3b-12b-3 — the EMISSION PLAN: what the mesh must ACQUIRE
(LANDED, pure; census-only)

§3w closed by naming the missing precondition: the cut across the existing fan
is non-monotone at every determined site, so the emission half needs Yang
§4.5.2's LOCAL REFINEMENT — *"we increase the mesh resolution of the parametric
surfaces associated with the erroneous regions"*
(`refs/text/yang2025_hybrid_boolean.txt:659-676`) — applied at an analytically
determined place rather than as a density ladder. This measures what that
refinement IS, per site, still as a pure function returning measurements:
`transit_emission_plan` says which edges must be split, where along them, how
far off-chord, and whether the corner arc is clear. Gated with §3v/§3w under
`YANG_451_TRANSIT_ANATOMY`.

**The acquisition has TWO independent sides.** A q-point is where a chain meets
the crease, so it must become a vertex of BOTH: of the chain that terminates
there (`QAcquire`) and of the crease's own mesh chain (`CreaseAcquire`). The
census measured a site that already carries one and not the other (the fixture's
base shape, and R0003 v7611/v8809 on the corpus), which is why the plan reports
them separately rather than as one verdict.

| case | v | corner° | fan span° | over-reach | chain side | crease side | arc sag |
|---|---|---|---|---|---|---|---|
| R0003 | 1983 | 9.14876e-2 | 8.62890 | 94× | AtVertex ×2 | **AtEnd ×2** | 6.20e-5 |
| R0003 | 8658 | 3.09179e-2 | 8.62890 | 279× | AtVertex ×2 | **AtEnd ×2** | 7.28e-6 |
| R0003 | 11356 | 3.09179e-2 | 5.75260 | 186× | AtVertex ×2 | **AtEnd ×2** | 7.28e-6 |
| R0044 | 47 | 3.55643e-2 | 8.70164 | 245× | Split (0.786/0.447) | **Interior ×2** | 1.77e-4 |
| R0003 | 7611 | 1.72339e-1 | 8.62890 | 50× | Split (0.451/0.802) | **NoChain ×2** | 1.83e-4 |
| R0003 | 8809 | 3.85568e-1 | 8.62890 | 22× | Split (0.553/1.305) | **NoChain ×2** | 1.13e-3 |
| R0044 | 38 | 1.47720e-3 | 2.22776 | 1508× | Split (0.799/0.393) | **NoChain ×2** | 1.93e-7 |

**Reading 1 — the crease side partitions the 7 determined sites 3/1/3, and the
partition is structural, not a magnitude band.** Three sites already carry both
q-points as crease vertices (`AtEnd`); one carries the crease but as a single
chord the corner falls *inside* (`Interior`); three carry no crease edge in the
fan at all (`NoChain`). That is the same three-way split §3v read off the
q-point representability, now derived from the fan's own edges rather than from
a nearest-edge search, and it agrees site for site.

**Reading 2 — and this is the increment's binding result — at the three
`AtEnd` sites the mesh's crease chain COVERS THE CORNER TWICE, and the doubled
cover IS the corner.** Two of the fan's crease edges each run from one q-point
PAST the other, so their angular intervals overlap, and the overlap equals
`corner_deg`:

| case | v | edges | overlap° | corner° | residual° |
|---|---|---|---|---|---|
| R0003 | 1983 | (1967,1963) ∩ (1965,1968) | 0.09148757941292729 | 0.09148757941339447 | 4.67e-13 |
| R0003 | 8658 | (8642,8638) ∩ (8640,8643) | 0.03091792306808472 | 0.03091792306804919 | −3.55e-14 |
| R0003 | 11356 | (11332,11328) ∩ (11330,11333) | 0.03091792306807761 | 0.03091792306804919 | −2.84e-14 |

Each residual decomposes EXACTLY into the two q-vertices' own angular offsets
from the analytic q-points (v1983: 4.494e-13 at `q1` plus 1.78e-14 at `q2` =
4.67e-13, the printed difference). So the identity is derived, not fitted: each
edge starts at one q-point and ends beyond the other, hence their intersection
is `[q1, q2]` by construction.

**What that changes.** At those three sites the defect is NOT insufficient
resolution. The mesh has both q-points, has both chain edges, and needs no new
vertex anywhere — its chain is simply covering one arc twice because the site's
out-of-domain position dragged the boundary back across itself. The repair there
is a RE-ORDERING (run the chain `q1 → q2` once, hand the notch to the
neighbour), and §4.5.2 refinement is the wrong instrument for them. §3w's
conclusion that the emission half needs local refinement stands for the other
four sites and is narrowed here for these three.

**Reading 3 — the corner arc itself needs no refinement at any site.** Its
sagitta off a single straight chord between the two q-points is 1.9e-7 …
1.1e-3 across the population. So the corner is ONE edge, and every insert the
plan asks for is at a q-point — never in between. That closes the question
§3w's `Refined` node left open (whether the arc would need its own samples).

**Reading 4 — the over-reach is 22× to 1508×.** `fan_span_deg` is the fan's own
angular footprint on the crease, i.e. what a re-attribution of the existing
triangles would have handed the neighbour, against `corner_deg`, which is what
the corner actually is. The ratio is the quantitative form of §3w's
non-monotonicity, and it is why the plan is expressed as inserts rather than as
a re-labelling.

**Reading 5 — R0044 v47's interior insert reproduces §3v's independent
reading.** The plan puts both q-points inside the crease edge `981–6911` at
`t = 0.4280` / `0.4240`, `off_chord = 10.38885` / `10.36319` on a 558.5283-long
edge — against §3v's separately measured 10.39 / 10.36 for that same rim chord.
That edge is shared with the neighbouring face, so the split has to be
conforming on BOTH sides: the 3b-11 one-sided-insert lesson, one layer down in
the working mesh rather than in the output tessellation.

**A band test in this function was wrong, and the census caught it.** The first
version asked "is a crease-edge endpoint within the contract band of this
q-point?" and answered NO at R0003 v1983, whose `q1` vertex sits 1.7364e-12
from the solved point against a ~1.1e-12 band. It therefore reported the corner
as unclear and demanded an interior insert on a chain edge that already ends at
the point. The rule that works is the one this epic keeps arriving at:
IDENTITY, from the termination the cut already resolved — `AtEnd` iff the
vertex the cut named as the q-termination is an endpoint of a fan crease edge.
Same discipline as §3w's surface-identity q matching and §3t's `on_crease`
exemption. Pinned by a unit test whose fixture pushes the offset to 5× the band
along the crease and toward `q2`, so the vertex also lands strictly inside its
own corner interval; without the identity rule that test reports
`Interior { t: 0.401, off_chord: 1.71 }` for a point the mesh carries as a
vertex.

4 new unit tests on the §3u fixture (all closed-form: the corner sweep is
derived from `x = 66` and `y = 88` meeting the `r = 100` circle, never
transcribed). Census-only: reachable solely under `YANG_451_TRIPLE_DOMAIN` +
`YANG_451_TRANSIT_ANATOMY`, default path untouched.

**Not built here.** The mutation. What it now has, per site, is a determined
insert list rather than a re-attribution: split the named chain edges at the
exact q-points (chain side), split or create the crease chain at the same points
(crease side), and — at the three `AtEnd` sites — no insert at all, only the
removal of the doubled cover.

## 3y. inc-2c-3b-12b-4 — the EMISSION EDIT LIST: what the mesh must DO
(LANDED, pure; census-only)

§3x said what each site must ACQUIRE. This says what has to be touched to give
it that, and — the reason the increment exists — how far outside the fan the
touching reaches. `transit_emission_edits` turns a determined plan into the
mints and the triangle sets, still as a pure function returning measurements,
under the same `YANG_451_TRIPLE_DOMAIN` + `YANG_451_TRANSIT_ANATOMY` gates.

**Reading 1 — the edit list is determined at exactly ONE site in the corpus,
and it is the face-627 site.** Of the seven sites §3x left with a determined
plan, six decline STRUCTURALLY and one yields edits:

| case | v | verdict |
|---|---|---|
| R0044 | **47** | **OK** — 2 mints, host `(981, 6911)`, 7 triangles touched |
| R0044 | 38 | `ChainAbsent` |
| R0003 | 1983 | `AlreadyCarried { overlap_deg: 0.09148757941292729 }` |
| R0003 | 8658 | `AlreadyCarried { overlap_deg: 0.03091792306808472 }` |
| R0003 | 11356 | `AlreadyCarried { overlap_deg: 0.030917923068077613 }` |
| R0003 | 7611 | `ChainAbsent` |
| R0003 | 8809 | `ChainAbsent` |

The `AlreadyCarried` declines carry the plan's own measured overlap rather than
re-deriving one, so a caller that has to choose an instrument gets the reason
and its magnitude together.

**Reading 2 — and this is the increment's binding result — R0003 yields NO
edits at any of the five sites where §3x gave it a determined plan.** The
SUPPORTED_CORRECT case is untouchable by this repair as scoped, not because a
magnitude band excludes it but because its crease side is `AtEnd` or `NoChain`
at every site. §3t refused
the 1.2× overrun gap between the two cases as a discriminator and required the
populations be separated by FIXING both; the emission half separates them one
better — the insertion repair has no work to do on R0003 at all, so arming it
cannot regress the case. That is a structural safety property of the mutation,
established before the mutation exists.

**Reading 3 — the repair's reach outside the fan is exactly ONE triangle, and
it belongs to the neighbour.** At v47:

| quantity | value |
|---|---|
| crease host | `(981, 6911)` |
| triangles carrying it | `13112` (in fan) + `13037` (**not** in fan) |
| their input faces | `(B, 168)` and `(B, 167)` |
| chain edges | `(47, 280)` and `(47, 45)`, 2 fan triangles each |
| touched | `41, 279, 13037, 13110, 13111, 13112, 13113` — 7 |
| outside the fan | `13037` — 1 |
| relabelled wholesale | `13112` |

So the crease chord is genuinely SHARED between two input faces of operand B,
and refining it is not a fan-local act: the neighbour's triangle must receive
the same two vertices or the mesh T-junctions along the very curve the repair
exists to make conformal. That is the 3b-11 one-sided-insert lesson, one layer
down in the working mesh rather than in the output tessellation — and here it
is measured in advance instead of being discovered as a fold. The attribution
reading also confirms the destination directly: the notch passes from `(B, 168)`
to `(B, 167)` because that is who owns the far side of the chord, rather than
by an assumption about which face "should" receive it.

**Reading 4 — the wholesale relabel and the chord split are the SAME
triangle.** `relabel` is `[13112]` and `crease_tris` is `[13037, 13112]`, so the
own-patch triangle that changes face wholesale is also one of the two carrying
the chord the mints go into. The two edits are therefore not independent: the
mutation cannot relabel first and split afterwards (the split would re-derive
attribution from a triangle that has already moved) nor split first and relabel
by id (the id no longer names one triangle). It has to re-triangulate `13112`
and attribute the children — the notch's to `(B, 167)`, the rest to `(B, 168)`
— in one act. Worth stating because the natural implementation order is the
broken one.

**Reading 5 — the insert order follows the CHORD, not the solver's q
numbering.** Both mints go into one chord, so the refined chain must connect
them in the order they occur along it. At v47 the chord runs `981 → 6911` and
`q2` (t = 0.42395608063854284) precedes `q1` (t = 0.4280468276336979) — the
solver's numbering is the other way round. Ordering by q index would invert the
notch. `inserts` is therefore sorted along the chord and `chain_tris` is
permuted with it, so slot `i`'s triangles carry slot `i`'s edge. A unit test
names the same fixture chord from both ends and asserts the order reverses and
every parameter complements; without the sort it fails.

**What is NOT determined, and typed rather than guessed.** `AlreadyCarried`
(the re-ordering §3x identified — the mesh has the corner twice and needs no
mint), `ChainAbsent` (no local crease chain to refine; it must be CREATED,
which needs the neighbour patch's mesh too), plus `CreaseHostsDiffer`,
`ChainAlreadyCarried`, `HostNotManifold`, `CornerNotClear` and `SiteAmbiguous`
for shapes the corpus does not currently exhibit. The site itself is DERIVED
from the fan (the intersection of its triangles' vertex sets) rather than
passed — §3w is what passing a redundant `v` costs.

5 new unit tests on the §3u fixture, including one that adds the neighbour's
triangle across the chord so the fixture carries the same two-sided reach the
corpus does, and one that pins the boundary case (a chord no neighbour carries
has an empty reach — measured, not assumed).

**Not built here.** The mutation. It now has, for the one site that needs it, a
closed edit: mint `q2` and `q1` at their exact positions, re-triangulate the six
fan triangles and the neighbour's one, relabel `13112` to `(B, 167)`.

## 3z. inc-2c-3b-12b-5 — the EMISSION REGION: where the edits can be
APPLIED (LANDED, pure; census-only)

§3y closed the edit list — two mints, one crease chord, four chain-carrier
triangles, one wholesale relabel. This asks the question that has to be
answered before any of it is written to a mesh: whether those edits COMPOSE.
They are stated per-edge, and a per-edge split is the natural implementation of
a per-edge list. `transit_emission_region` derives the neighbourhood the edits
land in and measures what that implementation would actually produce, still as
a pure function, under the same `YANG_451_TRIPLE_DOMAIN` +
`YANG_451_TRANSIT_ANATOMY` gates.

**Reading 1 — the region is a topological DISK, and the site is on its
boundary.** At R0044 v47 the six host-carrier triangles
`[41, 279, 13037, 13111, 13112, 13113]` are bounded by exactly one cycle, the
octagon `44 → 280 → 981 → 994 → 6911 → 47 → 6945 → 45`. Note what is NOT in
it: `13110` — the one fan triangle carrying no host edge — which is why the
site appears on the boundary rather than interior to it. Both mints land
strictly inside (all three host edges are carried by two region triangles
each). So the mutation is well-posed as a polygon fill: remove six triangles,
re-triangulate an octagon with two interior points, ten triangles out.

**Reading 2 — and this is the increment's binding result — the edit list
cannot be applied edge by edge, and the reason is structural rather than
incidental.** Each mint has two host edges: the chain it terminates and the
crease chord it refines. The chord's own-patch carrier is `13112`, which is the
triangle apexed at the SITE — §3y Reading 4 already named it, as the wholesale
relabel. So splitting that chord fans a new edge from the site to each mint,
which is the very edge the chain split has already created. Both `(site, mint)`
edges come out carried by FOUR triangles:

| edge | incident |
|---|---|
| `(47, 16355)` | 4 |
| `(47, 16356)` | 4 |
| `(981, 16355)` | 4 |

The mesh is non-manifold along the three edges the repair exists to make
conformal. The fixture reproduces it in every orientation — with and without
the neighbour across the chord, and in both fan orders — because the premise
(the chord carrier is apexed at the site) is the anatomy, not an arrangement.

**Reading 3 — the coincident FIN, by contrast, is order-dependent, so one
orientation is not evidence about the other.** A mint's chain edge and the
chord end it is nearest can be PAIRED by the fan's cyclic order or CROSSED by
it. Paired, the chain split and the chord split emit the same triangle twice in
opposite windings — a zero-area fin. R0044 v47 is paired: `[47, 981, 16355]`,
and the third over-carried edge above is its residue. The fixture is crossed in
its forward orientation and paired when the same chord is named from the other
end — measured both ways, which is what stops the fin from being reported as
the general statement when the over-carried edges are.

This is the 3b-11 one-sided-insert lesson arriving a third time, and the
sharpest form of it yet: §3y measured that the repair REACHES outside the fan,
and this measures that it cannot be DECOMPOSED along the edges it reaches
through. An edit list stated per-edge is not an edit list applied per-edge.

**What is typed rather than guessed.** `TriangleInBothRoles` (a triangle
carrying a host edge in two roles — its children are not defined by a single
split), `NotADisk` and `BoundaryPinched` (the region's boundary does not chain
into one simple cycle, so there is no polygon to fill). None occur in the
corpus.

5 new unit tests on the §3u fixture: the disk and the site's place on its
boundary, the doubling in all four fixture configurations, the fin's
order-dependence in both (asserting it ABSENT in one and present in the other,
so the detector cannot be vacuous), the interference's locality — every
over-carried edge touches a mint and no boundary edge is disturbed — and the
hand-built two-role decline.

**Not built here.** The mutation. It now has a determined region and a refuted
decomposition: fill the octagon `44 → 280 → 981 → 994 → 6911 → 47 → 6945 → 45`
with `16355` and `16356` interior, in ONE act, attributing the notch to
`(B, 167)` and the rest to `(B, 168)`.

## 3aa. inc-2c-3b-12b-6 — the FACE PARTITION and the boundary PINCH: the
fill's unit, and the NOTCH it cuts (LANDED, pure; census-only)

§3z gave the mutation a determined region and refuted its decomposition along
the edges. This finds the unit that does work. `transit_emission_parts` splits
the region by input face and closes each part at the site;
`transit_boundary_pinch` inserts the mints a part's boundary carries and splits
the cycle where it repeats. Both pure, same gates.

**Reading 1 — the region is not the fill's unit, and the own patch is CUT IN
TWO until one triangle rejoins it.** The region spans both operands, so it has
no single chart. Partitioned by face at R0044 v47:

| face | tris | components | closure | closed boundary |
|---|---|---|---|---|
| `(A, 2)` | `[41]` | 1 | — | `[44, 47, 45]` |
| `(A, 3)` | `[279]` | 1 | — | `[44, 280, 47]` |
| `(B, 167)` | `[13037]` | 1 | — | `[981, 994, 6911]` |
| `(B, 168)` | `[13111, 13112, 13113]` | **2** | `[13110]` | `[45, 47, 280, 981, 6911, 6945]` |

The own patch's three triangles touch only at the site — `13112` and `13113`
share no edge — and the single triangle that reconnects them is `13110`: the
same one §3z found leaving the site on the region's boundary. Excluding it both
opened the region and cut the own patch in half, because it is the one fan
triangle carrying no host edge. Closed, every part is a disk.

**Reading 2 — and this is the increment's binding result — the doubling §3z
refuted on the EDGES is, on the BOUNDARY, the corner itself.** The own patch
carries all three host edges, so each mint lands on its boundary twice:

```
[45, 16356, 47, 16355, 280, 981, 16355, 16356, 6911, 6945]
```

A cycle that repeats a vertex pinches there, and the two pinches partition it:

| loop | is |
|---|---|
| `[16355, 280, 981]` | keeps `(B, 168)` |
| `[16356, 47, 16355]` | **THE NOTCH** — goes to `(B, 167)` |
| `[45, 16356, 6911, 6945]` | keeps `(B, 168)` |

The notch is a single triangle: the site and both mints, bounded by the two
chain stubs and the corner edge. That is §3x's 0.0356° corner against the
fan's 8.70° footprint — the 245× over-reach expressed as a decomposition
rather than a ratio. No other part pinches: each carries a host edge in one
role only, so `(A, 2)` and `(A, 3)` become quads and `(B, 167)` a pentagon,
every one of them receiving on its own boundary exactly the mints that lie on
it. That is the conformality §3y demanded, obtained by construction instead of
by a second pass.

So the same measurement that killed the per-edge form supplies the correct one.
The per-edge split emitted each mint once per host and collided; the per-part
fill inserts each mint once per host edge THE PART CARRIES, and the collision
becomes the cut.

**Reading 3 — the clean cut has a precondition, and the fixture reaches both
sides of it.** Two pinch points decompose a cycle into a corner and its
remainders only when their repeat spans nest or stay disjoint. INTERLEAVED,
they do not: the site's loop swells to most of the patch and there is no corner
to hand over. R0044 v47 is not interleaved; naming the fixture's chord from its
other end reaches the shape that is, and there the pinch reports `interleaved`
and declines to name a notch rather than handing back a loop that is not one.
This is the same order-dependence §3z measured for the coincident fin, now
carrying a consequence — so it is typed, not narrated.

**Orientation comes for free.** Every loop edge is a directed consecutive step
of the inserted boundary, including the one closing back to the pinch point, so
the loops inherit the part's winding and the fill does not have to re-derive an
orientation. Pinned by a test rather than assumed, because the §3n far-region
work is what it costs to get that wrong.

A mint visited more than twice on one boundary would mean the cycle is not the
polygon this assumes; it is a structural decline, not a fallback.

2 new unit tests, each run in BOTH fixture orientations: the partition (exact,
the own patch's two components, the single-triangle closure, every closed part
a disk) and the pinch (one insert per host edge carried, only the own patch
pinches, the notch is the corner triangle bounded by both mints, the pinch
partitions the cycle, and the interleaved arrangement yields no notch).

**Not built here.** The mutation. Its plan is now closed end to end: close the
own patch with `13110`, insert `16355`/`16356` on the four part boundaries that
carry them, fill each polygon in its own chart — `(A, 2)` and `(A, 3)` quads,
`(B, 167)` a pentagon, `(B, 168)` a triangle plus a quad plus the notch — and
attribute the notch to `(B, 167)`.

## 3ab. inc-2c-3b-12b-7 — the EMISSION FILL: the mutation planned end to
end and CERTIFIED (LANDED, pure; census-only)

§3aa closed the plan. This composes it — `transit_emission_fill` — as a pure
function that returns the mutation as DATA (what goes, what comes, where every
vertex stands) together with certificates on the RESULT, computed against the
whole mesh before anything is written. Same gates as §3v–§3aa.

**What it does.** Every part is pinched (§3aa) and the structure checked
first — the own patch must pinch cleanly into a notch, no other part may pinch
at all — and only then is each loop filled in its OWN face's chart: a 3-loop
is one triangle; a longer loop is chart-projected (chain θ-unwrap and the
apex / period guards the fan refill uses) and CDT'd boundary-only, so every
polygon comes out as exactly `n − 2` triangles on its own vertices, no Steiner
points. The site is projected at the CORRECTED junction `J`, the mints at their
exact q-points. Structural declines are established on every part BEFORE any
chart projection, so a structural verdict is never masked by a chart failure
on an earlier part — the interleaved fixture reported `Cdt` before that
ordering existed, which is why the order is a design decision and not an
accident.

**Reading 1 — orientation is taken from the LOOP, not from an area
heuristic.** §3aa proved every loop edge is a directed consecutive step of
the part's inserted boundary, so a fill triangle that carries a loop step must
traverse it the way the loop does — that is exactly what pairs it with
whatever lies across the step. The fill reads the direction its CDT triangles
carry loop steps: all forward, keep; all reversed, flip; none or both,
`OrientationUndefined`. The §3q lesson (a folded fossil corrupts the area
vector) cannot arise, because no fossil is consulted.

**Reading 2 — the notch's destination is derived from the mesh and checked
against the analysis.** The destination is the face of the crease-chord
carrier outside the own face — §3y's "who owns the far side of the chord" —
and its surface is compared with the transit's `s_nbr`. At R0044 v47 the two
routes agree, bit-identically: `(B, 167)`, `notch_surface_agrees = Some(true)`.

**Reading 3 — and this is the increment's binding result — the fill at R0044
v47 is CERTIFIED against the whole mesh:**

| certificate | value |
|---|---|
| removed | `[41, 279, 13037, 13110, 13111, 13112, 13113]` — 7; `touched_delta = []` (≡ §3y's `touched`, derived by a different route) |
| polygons / triangles | 6 / 11 |
| edge incidence defects | **none** — every edge the edit touches has the incidence a manifold requires: re-created edges keep their count, created edges are carried twice, consumed edges vanish with no dangling survivor |
| survivor orientation | opposed 7, folded 0, folds inside the fill 0 |
| notch | `[16356, 47, 16355]` → `(B, 167)` |

The polygons are exactly §3aa's prediction, now triangulated:

| face | polygon | triangles |
|---|---|---|
| `(A, 2)` | `[44, 47, 16356, 45]` | `[44, 47, 16356]`, `[44, 16356, 45]` |
| `(A, 3)` | `[44, 280, 16355, 47]` | `[44, 280, 16355]`, `[44, 16355, 47]` |
| `(B, 167)` | `[981, 994, 6911, 16356, 16355]` | `[981, 994, 16355]`, `[994, 6911, 16356]`, `[994, 16356, 16355]` |
| `(B, 168)` | `[16355, 280, 981]` | itself |
| `(B, 168)` | `[45, 16356, 6911, 6945]` | `[45, 16356, 6911]`, `[45, 6911, 6945]` |
| **notch → `(B, 167)`** | `[16356, 47, 16355]` | itself |

The seven opposed edges are the closed region's outer boundary — the octagon
with `13110` folded in reduces `(6911, 47), (47, 6945)` to `(6911, 6945)`, seven
edges, each shared with one survivor and traversed the other way.

**Reading 4 — the like-for-like chord bound: no face receives a fill coarser
than what it gave up, and both cones get FINER.** Yang §4.1.2's `d(T)`, the
removed triangles of each face against the added ones, in that face's chart:

| face | old max `d(T)` | new max `d(T)` |
|---|---|---|
| `(A, 2)` | 39.93967926994923 | 39.93967926994923 |
| `(A, 3)` | 0 (plane) | 0 |
| `(B, 167)` | 22.216300502868222 | **14.119462702848725** |
| `(B, 168)` | 140.5730166901681 | **75.52504192842936** |

`(A, 2)` is not a plane — the equality holds to the last digit because the
worst control point's distance is realised on the edge `(44, 45)`, which the
old triangle and the new `[44, 16356, 45]` share. The two cones halve: the
mints ARE §4.5.2's local refinement, placed at an analytically determined
point rather than by a density ladder, and the bound says so in the paper's
own metric.

**Positions.** `site_at = J = (−1813.5112911476508, −2388.375219816396,
−6104.356125165558)`; mint `16355` at `q2 = P_cap`, `16356` at `q1 = P_lat`
— §3u's independently recorded values, to their last digits.

**Reading 5 — the fixture had to become geometrically faithful, and the CDT
is what said so.** The first run declined `Cdt { face: (A, 2),
TriangulationFailed }`. The fixture's plane_x chain end `v1` had been placed
by crease ANGLE (`home(70°)`) for the topological increments and does not lie
on plane_x; in the `x = 66` chart the exact q-point lands 13 units outside its
chord's projection and the quad `[0, 6, 1, m1]` is a bow-tie (edges `0 → 6`
and `1 → m1` cross between `y = 76` and `y = 80` — checked arithmetically
before anything was changed). The corpus chain edges DO lie on their planes.
The fill fixture therefore puts both chain ends on their planes and on cone
A's home side — `v1 = (66, 60, √(66² + 60²))`, `v5 = (40, 88, √(40² + 88²))`
— with topology, sides and attributions unchanged, so anatomy through parts
are exactly the §3y–§3aa fixture's. First increment of the epic whose fixture
geometry, and not only its topology, is load-bearing.

**Also caught, first by the fixture and then confirmed by the corpus:** the
first test asserted each mint ends on exactly FOUR fill triangles. The fixture
gave 5, and so does the corpus — `16355` is on five, because the pentagon fans
from `994`. The count is the CDT's diagonal choice, not an invariant. The
invariant is the four REQUIRED edges per mint — the stub to the site, the stub
to its chain end, the half-chord to its chord end, the corner edge to the
other mint — each carried by exactly two fill triangles. Pinned that way.

**Typed declines.** `OwnPartMissing`, `PartNotADisk`, `PinchUndefined`,
`Interleaved`, `NoNotch`, `UnexpectedPinch`, `NotchDestinationUnknown`,
`NotchDestinationAmbiguous`, `NoChart`, `ApexInPolygon`, `ThetaUnwrap`, `Cdt`,
`OrientationUndefined`. `notch_surface_agrees` is a measured field rather than
a decline, so a disagreement would be reported alongside the rest of the
certificate instead of hiding it; the apply arm will treat `false` as a
refusal.

7 new unit tests: the fill (manifold, conformal, wound by its loops; one
notch, attributed to the neighbour; `n − 2` triangles per polygon; the four
required edges per mint), exact positions (mints on the crease circle, site at
`J`), a folded survivor REPORTED (opposed 1 / folded 1 — the certificate is
not vacuous), the interleaved arrangement declines, a chord no neighbour
carries has no destination, the destination checked against `s_nbr`
(`Some(false)` with the wrong surface, `NoChart` with none), and the chord
budget (planes 0 / 0, cones finite and no coarser).

**Verification.** Rewrite tier GREEN — 173 binaries, 2792 tests, 0 failures,
1206s (the first run in this fresh container failed 7 cherchi-rs binaries
with "FFI shim not linked" / "mesh_booleans binary not found": the parity
sidecar was absent, `scripts/build_sidecars.sh` rebuilt it, and the re-run
was green — environmental, and the loud non-skip refusal is what made it
visible). Full corpus 273C/0W/34E/1EE/0T, canonical, `results.json`
byte-identical, F0085 honest SUPPORTED_CORRECT at 301.4s. Clippy
`--all-targets` clean. WASM rebuilt in the same commit.

**Not built here.** WRITING it: append the two mints, move the site to `J`,
overwrite the seven removed slots and append the remaining four with their
attribution, then the relocation bookkeeping for a site whose incidence is
now `(A, 2)`, `(A, 3)`, `(B, 167)` — the corrected junction's own three
faces. That is 3b-12b-8, the apply arm: gated, measured on R0044 for what
the standing wall past v47 becomes.

## 3ac. inc-2c-3b-12b-8 — WRITING the fill: the gated apply arm, the LIFT
certificate, and what it caught (LANDED, gated; REFUSES at the corpus site —
measured)

§3ab certified the fill's incidence, winding and chord bound against the whole
mesh. This builds the write — `transit_emission_write`, slot-stable: the mints
appended at exactly the ids the plan named, the site moved to `J`, the removed
slots overwritten in place and the surplus appended, attribution parallel,
refusing on any unclean certificate and leaving the mesh untouched when it
does — gated by `YANG_451_TRANSIT_EMIT=1|on` inside the §3v–§3ab chain. On a
write the relocation loop takes `J` as the site's position and the
`RelocationCrossedCrease` STOP does not fire; otherwise the standing STOP
applies exactly as before. And it adds the certificate the write demanded
before it could be trusted: the LIFT sense. A directed-edge check certifies
WINDING; it cannot see a triangle that pairs consistently along every edge
yet lifts folded onto its surface (the KV9-F2b lesson). Per face,
`LiftSense` counts removed (old positions) and added (planned positions)
triangles whose 3D normal lies along / against the face surface's gradient at
their centroid; `lift_flips` are added triangles against the fossil's
majority (or the added minority where the fossil disagrees with itself);
`FillPolygon.lift` carries it per triangle.

**Reading 1 — the write REFUSES at R0044 v47, on the lift certificate, and
R0003 stays SUPPORTED_CORRECT untouched** (no apply activity at all — every
R0003 site declines at §3y, as designed).

| face | fossil along/against | fill along/against |
|---|---|---|
| `(A, 2)` | 1 / 0 | 2 / 0 |
| `(A, 3)` | 1 / 0 | 2 / 0 |
| `(B, 168)` | 4 / 0 | 3 / 0 |
| `(B, 167)` | 1 / 0 | **1 / 3** |

Per triangle on `(B, 167)`, the cosine of the triangle normal with the cone
gradient at its centroid: `[981, 994, 16355]` +1.0000; `[994, 6911, 16356]`
**−0.8062**; `[994, 16356, 16355]` **−0.7230**; the notch `[16356, 47, 16355]`
**−1.0000**.

**Reading 2 — the survivors decide which way is outward.** Over the whole
mesh, `(B, 167)`'s 69 surviving triangles ALL lift along (0 against);
`(B, 168)` 71 / 0; `(A, 3)` 67 / 0; `(A, 2)` 194 / 13 (thirteen against on
the cylinder — a pre-existing population, recorded here and not chased). So
on cone 626 outward IS along, and the notch — a 2-unit triangle of three
exact cone-626 points (residuals ≤ 4.5e-13) on a surface of radius 3681,
whose lift is faithful by construction — is INVERTED. Its consistent edge
pairing with the A-quads and the pentagon (§3ab's opposed 7 / folded 0) is a
fold, not a proof.

**Reading 3 — and this is the increment's binding result — the neighbour face
is a BAND, and the corner does not lie inside its host carrier.** `(B, 167)`
is a conical band between two creases — 168|167 at station 2306.7221 and
167|166 at 2307.9162 — of width **1.194**. `994` is ON the far crease (its
azimuth is `981`'s to the last digit, −149.705457°), so `13037 =
[981, 994, 6911]` is the band's full-width sliver. `J` is at station
2307.6544, **0.932** into the band; at its azimuth (−153.4045°) the sliver's
far edge `994 → 6911` is at station 2307.4086, so `J` lies **0.2457 BEYOND**
the host carrier, inside `13038 = [6911, 994, 6877]` (barycentric
0.219 / 0.575 / 0.206). Both stub midpoints land inside `13037`, so the
stubs cross the edge `994–6911`. §3z's region — the host carriers — is
INCOMPLETE on the neighbour side, and §3aa's "hand the notch to `(B, 167)`"
as a triangle DOUBLE-COVERS cone 626: `13038` already covers where `J` is.

**Reading 4 — what the corner IS: a BITE, not a triangle.** Walk the fan at a
mint, say `16355`: `(B, 168)` keeps the sector from its chain end `280` round
to the crease vertex `981`; `(B, 167)` keeps the sector from `981` round to
`J`; and A's quad `[44, 280, 16355, 47]` spans the remaining sector from `J`
back to `280` — ACROSS the corner direction. The corner segment
`16355–16356` lies under A's face, inside A's material wedge, and is an edge
of NOTHING that is kept. The pinch's notch loop is the flap of 627's fan that
overran the crease — the A-wedge on 627's extension — and it is DROPPED; on
626 the corner enters as a reflex detour of the neighbour polygon's boundary,
`6911 → 16356 → J → 16355 → 981`: the neighbour part is `13037 ∪ 13038 =
[981, 994, 6877, 6911]` with the mints inserted and the corner segment
replaced by the stub path through `J`. No interior point, no constraint
edge — a plain polygon, whose region is enlarged by the triangles containing
`J` and crossed by the stubs. §3aa's decomposition stands (the pinch
correctly isolates the corner loop); what it is FOR was mis-read — it names
the region 626 loses, not a triangle it gains.

**Reading 5 — the band is under-resolved along its creases by four times its
own width, so no fill of it can lift faithfully at this resolution.** The
crease chord `981–6911` (558.528) on a circle of radius 3681.154 has sagitta
10.608; its midpoint lies 5.633 below the cone along the normal, and its
surface foot is **4.773** station units from the crease — 4× the band's
1.194. Any triangle with a full chord and a vertex on the arc or in the band
has its plane rotated far from the tangent plane: those are the −0.81 and
−0.72 cosines. The fossil `13037` lifts along (+0.9999) only because its
short edge runs along the generator; its `d(T) = 22.2` (§3ab) was saying the
same thing. This is Yang §4.5.2's local refinement in its literal form —
*"increase the mesh resolution of the parametric surfaces associated with the
erroneous regions"* — with the region now NAMED: sag ≤ 0.1 (a tenth of the
width) needs chords ≤ 54.3, about ten segments per current chord, on BOTH
creases of the band, with conforming splits on `(B, 168)` and `(B, 166)`
across them. §3x's Reading 3 ("the corner arc needs no refinement", sag
1.8e-4) was right about the corner and silent about the band, whose width
nobody had measured.

**Reading 6 — the fixture's own fossil is folded two against two.** Its fan
is topological, not angularly monotone; the certificate measured it rather
than assuming it away, and the fill's three replacements agree with each
other (0 / 3). The write test pins the site's post-transit incidence to
exactly the corrected junction's three faces, `(A, 2)`, `(A, 3)`, `(B, 8)`.

Typed: `EmissionWriteFailure::{CertificateFailed { what }, MintIdsStale,
AttributionLength, FewerAddedThanRemoved}` (a fill smaller than what it
removes would need slot deletion and an index shift — not built, refused).
2 new unit tests: the write lands slot-stably (survivors keep slot, triangle
and attribution; every fill triangle present with its face; whole-mesh
incidence 1 or 2 with pre-existing edges at their old count and every mint
edge at two; no directed edge twice; the site's incidence), and every
refusal leaves the mesh untouched.

**Verification.** Rewrite tier GREEN — 173 binaries, 2794 tests, 0 failures,
1217s. Full corpus 273C/0W/34E/1EE/0T, canonical, `results.json`
byte-identical (gate off), F0085 honest SUPPORTED_CORRECT at 298.8s. Gate
on: R0044 `[s451-apply] REFUSE CertificateFailed { what: "lift_flips" }`,
verdict unchanged; R0003 SUPPORTED_CORRECT with zero apply lines. Clippy
`--all-targets` clean. WASM rebuilt in the same commit.

**Not built here.** 3b-12b-9 — the BITE: enlarge the neighbour part by the
triangles containing `J` and crossed by the stubs, drop the notch loop,
detour the neighbour polygon through `J`. 3b-12b-10 — §4.5.2 band
refinement: split the band's crease chains to sag ≪ width, conforming across
168 / 167 / 166. The lift certificate is the acceptance test for both; the
write arm stays gated until it passes at the corpus site.

## 3ad. inc-2c-3b-12b-9 — the BITE: the corner as a detour of the
neighbour's boundary, and the ONE triangle the band still cannot lift
(LANDED, pure; the write stays gated and REFUSES — measured)

§3ac read the corner's landing: 0.2457 beyond the neighbour's host carrier,
inside the next triangle of a 1.194-wide band, and the pinch's notch loop
not a triangle 626 gains but the region it loses. This builds that reading.

**What changed.** `transit_bite_region` finds, on the neighbour face, the
surviving triangles whose chart footprint contains the corner and the ones
the two chain stubs cross on their way from the mints to it — per triangle,
chain-unwrapped about its first corner and shifted whole periods to sit
nearest the corner's azimuth, so a seam-straddling triangle far away cannot
be unwrapped into a false hit (the §3ac probe's own artefact, fixed at the
source). `transit_emission_fill` then: enlarges the neighbour part by those
triangles and re-derives its boundary (`BiteNotADisk` if it is not one
cycle); inserts the mints; finds the corner STEP between them
(`CornerStepMissing` otherwise) and DETOURS it through the site; refuses a
corner within the feature floor of a polygon vertex (`CornerCoincident`);
DROPS the own patch's notch loop (`dropped`, for the census); and fills as
before. The lift certificate's reference is now the face's SURVIVORS — the
whole face's testimony — and the fossil only where the face has none,
because the fossil is the very thing the defect may have folded.

**Reading 1 — at R0044 v47 the bite is exactly §3ac's prediction.**
`contains_corner = [13038]`, `crossed = []`, `dropped = [16356, 47, 16355]`;
`removed` grows to eight, `touched_delta = [13038]` — the bite is precisely
what the corner adds beyond the host carriers. The neighbour polygon is
`[981, 994, 6877, 6911, 16356, 47, 16355]` and the fill is five triangles
fanned from the corner:

| triangle | cos(n, ∇) | area | centroid below the cone |
|---|---|---|---|
| `[981, 994, 47]` | +1.0000 | 267 | 1.705 |
| `[981, 47, 16355]` | +1.0000 | 206 | 1.700 |
| **`[994, 6877, 47]`** | **−0.8331** | 2783 | 7.129 |
| `[6877, 6911, 47]` | +1.0000 | 361 | 3.119 |
| `[6911, 16356, 47]` | +0.9997 | 270 | 3.101 |

Certified otherwise clean: `edge_defects = []`, opposed 8 / folded 0 /
added_folds 0, `(B, 167)` `d(T)` 22.216 → **12.595** (finer than §3ab's
14.119), `(B, 168)` 140.573 → 75.525, planes unchanged. `lift_flips` **3 →
1**. The write still refuses, on that one triangle, and R0003 stays
SUPPORTED_CORRECT with zero apply activity.

**Reading 2 — and this is the increment's binding result — the one flip is
the band's far-crease chord, named in advance.** `[994, 6877, 47]` spans the
167|166 crease from azimuth −149.705457° to −158.407100° — a chord of
558.817 on a circle of radius 3683.060, sagitta **10.614** — with its apex
the corner, **0.2619** in-surface from that crease. Its plane follows the
chord's dip (centroid 7.129 below the cone) rather than the surface; the
other four, whose bases are a generator edge, a 236-chord or a 320-chord
with the corner 0.9 away, lift at +1.000. This is §3ac Reading 5 reduced to
one triangle: the corner is now placed correctly, and what remains is
purely that the band's crease chains are an order of magnitude too coarse
for its width. Yang §4.5.2 — *"increase the mesh resolution of the
parametric surfaces associated with the erroneous regions"* — with the
region now a single named chord.

**Reading 3 — the survivors are the reference, and the fixture said so
first.** With survivors as the lift's reference, the fixture's two
survivors — placed a session ago for the orientation certificate — came out
FOLDED: both third vertices sat on the site's side of their shared edge,
consistent by directed edges, inverted in the chart; and the wide
neighbour's apex sat on cone B's axis, where it has no azimuth, so the bite
polygon's CDT produced a fill lifting both ways. Same class as §3ab's
bow-tie: a topological fixture meeting a geometric certificate. Both
survivors moved across their edges and the apex onto the cone; the fold
test now pins the double report (a flipped survivor is a fold AND the lift
reference for its face). The fixture's own fossil stays 2 / 2 — measured,
asserted, not hidden.

**Reading 4 — a band fixture reproduces the corpus shape.** `bite_fixture`
makes the neighbour a band between stations 50 and 60 on cone B: a
full-width sliver `[2, w3, 3]` with its apex above chord end `3`, and a
second triangle `[2, w2, w3]`. At the corner's azimuth (53.1°) the sliver's
far edge is at station 54.4 and the corner at 55: 0.6 beyond, inside the
second triangle — which the bite pulls in (`contains_corner = [it]`,
`touched_delta = [it]`) and the fill certifies clean: no incidence defect,
no fold, no flip, the corner segment carried by nothing, each stub by two,
the band's open rim by one.

Typed additions: `NeighbourPartMissing`, `BiteNotADisk`,
`CornerStepMissing`, `CornerCoincident { v, dist }`. Tests corrected to the
bite semantics (no notch triangle; one bite polygon; the corner between the
mints on its boundary; the corner segment nobody's) plus the band fixture:
44 in `s451_crease_domain`.

**Verification.** Rewrite tier GREEN — 173 binaries, 2795 tests, 0 failures,
1216s. Full corpus 273C/0W/34E/1EE/0T, canonical, `results.json`
byte-identical with the gate off, F0085 honest SUPPORTED_CORRECT at 299.7s.
Gate on: R0044 REFUSE `lift_flips` (1), verdict unchanged; R0003
SUPPORTED_CORRECT, no apply lines. Clippy `--all-targets` clean. WASM
rebuilt in the same commit.

**Not built here.** 3b-12b-10 — §4.5.2 refinement of the band: split the
far-crease chord `994–6877` (and, by the same rule, any band chord whose
sag exceeds a fraction of the band's width) with exact crease-circle
vertices, conforming on `(B, 166)` across it — the 3b-11 one-sided-insert
lesson once more — and re-fill. The lift certificate at `[994, 6877, 47]`
is the acceptance test; when it passes, the write arm's first real
measurement is what the R0044 run past v47 becomes.

## 3ae. inc-2c-3b-12b-10 — §4.5.2 REFINEMENT until the lift is faithful:
the cascade through the bands, and the FIRST WRITE at the corpus site
(LANDED, gated; measured)

§3ad left one flipped triangle: `[994, 6877, 47]`, spanning the neighbour
band's far-crease chord. This builds Yang §4.5.2 as the certificate demands
it — *"increase the mesh resolution of the parametric surfaces associated
with the erroneous regions"* — with the erroneous region named by the lift
certificate and the resolution increased by exact crease-circle vertices,
constant-free, until every fill triangle lifts the way its face's survivors
do.

**What changed.** The two hard-wired mints became a LIST: `HostMints` keys
every mint by the undirected mesh edge that hosts it, at its parameter from
the lower-id end, so a mint is inserted once per host edge each part
carries, in along-edge order, whichever way the part traverses it
(`transit_boundary_pinch` is now `insert_mints` + `pinch_cycle`, unchanged
in behaviour). `ExtraMint { at, host, t }` is a refinement mint; the fill
takes a list of them, and every triangle carrying an extra's host — on BOTH
faces across it, the 3b-11 one-sided-insert lesson built in — joins the
work list, a face the parts did not name getting a part of its own. The
write mints the whole list. `transit_emission_refine` is the loop: plan the
fill; while any triangle lifts against its face's reference, act on each
such triangle; plan again; `cap` iterations, then a loud `CapReached` that
carries the halvings and the last fill.

**Reading 1 — halving alone DIVERGES, and the census said why.** The first
loop halved the flipped triangle's crease chord at the exact arc midpoint.
At R0044 v47 it hit the cap with 129 halvings and the flips risen from 1 to
**128**: every halving on the neighbour's far chord `994–6877` put a mint
the face beyond, `(B, 166)`, had to receive, and `(B, 166)`'s fan from its
far vertex `993` over the sub-chords lifted against the cone at EVERY chord
length — 279, 140, 70, 35, 17 — while its fossil `[993, 6877, 994]` and all
69 of its survivors lift along. A band's strip triangulation is faithful
only because each apex sits above its base's END; a fan from one far vertex
over refined sub-chords has its apex five degrees off along the crease,
and its long edges dip below the surface by more than the band is wide. No
halving of the base can restore that. The fold was the APEX's crease being
coarse, not the base's.

**Reading 2 — the rule that converges: MATCHED splits.** For a flipped
triangle whose base is a crease chord on `C`: if its apex lies on ANOTHER
crease `C'` of the face (`on_crease` — the triangle is a band's), split
`C'` at the azimuth of the base's midpoint (`project_onto_curve` of the
midpoint onto `C'`), on the apex's own crease edge that brackets the point
(`MatchBeyondNeighbours` otherwise) — the matched vertex the strip needs;
else (the apex is interior, as the corner is) halve the base. A matched
split's host lies on the next crease, so the next face joins; the cascade
runs outward through the bands until a face certifies. Typed:
`RefineKind::{Halve, Matched}`, `MatchBeyondNeighbours`,
`ReferenceUndecided`, `HostAmbiguous`, `MidpointDegenerate`,
`FlipWithoutCreaseChord`, `CapReached`.

**Reading 3 — and this is the increment's binding result — at R0044 v47 the
refinement CONVERGES in seven rounds, cascading through five bands, and the
write LANDS.**

| round | face | kind | flipped triangle | step acted on | host split | len | dist |
|---|---|---|---|---|---|---|---|
| 1 | `(B, 167)` | Halve | `[994, 6877, 47]` | `(994, 6877)` | `(994, 6877)` | 558.82 | sag 10.61 |
| 2 | `(B, 167)` | Halve | `[994, 16357, 47]` | `(994, 16357)` | `(994, 6877)` | 279.61 | sag 2.65 |
| 3 | `(B, 166)` | Matched | `[993, 6877, 16357]` | `(6877, 16357)` | `(993, 6843)` | 279.61 | 9.05 |
| 4 | `(B, 165)` | Matched | `[983, 6843, 16359]` | `(6843, 16359)` | `(983, 6809)` | 140.04 | 11.77 |
| 5 | `(B, 164)` | Matched | `[982, 6809, 16360]` | `(6809, 16360)` | `(982, 6775)` | 70.19 | 15.87 |
| 6 | `(B, 163)` | Matched | `[985, 6775, 16361]` | `(6775, 16361)` | `(985, 6741)` | 35.21 | 20.27 |
| 7 | `(B, 162)` | Matched | `[984, 6741, 16362]` | `(6741, 16362)` | `(984, 6707)` | 17.67 | 24.74 |

(`dist` for a matched round is the base midpoint's distance to the matched
point — the band's width at that azimuth: 167 is 1.19 wide, then 9.05,
11.77, 15.87, 20.27, 24.74; `(B, 161)` certified without a split.) Each
matched vertex lands at the previous base's midpoint, so the next band's
end triangle keeps a long diagonal of ever-shorter base and ever-wider
band, until the band is wider than the diagonal's dip. The certificate
decided where that was; no constant did.

The converged fill: **11 polygons, 37 triangles, on 10 faces**, every one
lifting along (`lift_flips = 0`, `lift_uncertified = 0`); `removed` 19 (the
§3ad eight plus a strip pair from each of five bands and one from 161),
`touched_delta` = those twelve; `edge_defects = []`, opposed 19 / folded 0
/ added_folds 0; 9 mints. And in Yang's own `d(T)`, every face no coarser
and the bands far finer:

| face | old max | new max |
|---|---|---|
| `(B, 168)` | 140.573 | 75.525 |
| `(B, 167)` | 22.216 | **7.778** |
| `(B, 166)` | 24.128 | **11.971** |
| `(B, 165)` | 25.995 | 16.787 |
| `(B, 164)` | 27.815 | 22.903 |
| `(B, 163)` | 29.586 | 27.065 |
| `(B, 162)` | 31.303 | 30.034 |
| `(B, 161)` | 32.964 | 32.848 |
| `(A, 2)` / `(A, 3)` | 39.940 / 0 | 39.940 / 0 |

The neighbour's bite polygon is now `[981, 994, 16358, 16357, 6877, 6911,
16356, 47, 16355]` — seven triangles, all along. With the gate armed the
write LANDED: `WRITTEN { site: 47, mints: [16355, 16356], minted: 9,
removed: 19, added: 37, overwritten: 19, appended: 18 }` — the first
emission at the corpus site — and the run proceeds past v47 to the standing
wall it has always had, `RelocationCrossedCarrierVertex` at vertex 8 (in
census mode the seven other R0044 fires still carry their defects, six of
them undetermined at §3y). R0003 stays SUPPORTED_CORRECT with zero apply
activity.

**Reading 4 — a first refinement census panicked in the PRINTER, not the
write.** With nine mints the census's per-triangle position printer still
knew only two; `index out of bounds: the len is 16355 but the index is
16358`, and the harness reported a worker death as `TIMEOUT (11.5s)`. The
fill and the write were fine. Recorded because a census probe that cannot
count is a census probe that can hide a write.

**Fixtures.** `narrow_band_fixture` — the band's far crease at station 56 so
the corner sits 1 from it while the far chord dips 3.4 in-surface — flips on
the far chord and is fixed by halving (mints exact on the far crease circle,
the face beyond receiving every one, every sub-chord carried twice, the
unsplit chord gone). `cascade_fixture` — a second band 0.2 wide beyond it,
then a wide planar face — reproduces the corpus cascade: the first band's
far chord halved, the thin band refined by MATCHED splits (mints exact on
its crease circles, its far crease among them), the wide face joining, the
whole fill lying one way. A cap of zero is a loud `CapReached` carrying the
last fill. 4 new tests; 48 in `s451_crease_domain`.

**Verification.** Rewrite tier GREEN — 173 binaries, 2799 tests, 0 failures,
1228s. Full corpus 273C/0W/34E/1EE/0T, canonical, `results.json`
byte-identical with the gate off, F0085 honest SUPPORTED_CORRECT at 311.1s
(the run preceded a no-op type alias and a test-only assertion edit). Gate
on: R0044 WRITTEN at v47, verdict unchanged at the v8 wall; R0003
SUPPORTED_CORRECT, no apply lines. Clippy `--all-targets` clean. WASM
rebuilt in the same commit.

**Not built here.** The write arm stays GATED. What the corpus run past v47
needs next is the other seven R0044 fires — v38's `ChainAbsent`, the
`AlreadyCarried` re-ordering shape, the `TransitLeavesNeighbour` sites —
and the v8 `RelocationCrossedCarrierVertex` wall; and the write's
downstream bookkeeping (the nine mints as exact junction/crease vertices
for the standing-junction certificate and the Stage-5 ring walk) is
unmeasured until a case reaches them.

## 3af. inc-2c-3b-12b-11 — the CORPUS under the gate set, the union CERTIFIED,
and the TOPOLOGY reference (LANDED, instruments only; no kernel change)

§3ae landed the write and left the run at "the standing v8 wall" — measured
with the emission's own gates alone. The v8 wall is the CORRIDOR family's
(`YANG_451_TRANSIT`, §3j), not the emission's; the two arms had never been
run together. This increment runs them together, corpus-wide, attributes
every delta to its knob, and — because the categorized runner never sees a
union whose case then stops at a later op — validates R0044's union on its
own, against every oracle the runner has and against the reference.

**Reading 1 — with the four-knob set (§3s) AND the emission, R0044's union
COMPLETES and the case moves to a typed boundary.** Same binary, single case:

| gates | outcome |
|---|---|
| none | ERROR — `RelocationCrossedCarrierVertex` at v8 (canonical) |
| emission chain only (`TRIPLE_DOMAIN=census`+`ANATOMY`+`EMIT`) | WRITTEN at v47, then the v8 wall (§3ae's reading) |
| four knobs, no emission | ERROR — `FaceId(627)` ring rejected (§3s's standing wall) |
| four knobs + emission | **`UNSUPPORTED(curved-profile)`** — the union completes; the circle CUT refuses at Stage 1: `curved partial-patch operand face FaceId(458) [curved lateral degree-4 boundary (surface-pair edge)] (a previous curved boolean's result cannot re-enter yang-rs Stage 1 — no partial-patch tessellation yet)` |

Ablating one knob at a time with the emission on: without `YANG_453_SPAIR`
the union stops at `FaceId(459)` (§3s's fold); without `YANG_451_TRANSIT` at
v8; without `YANG_441_TORUS_CHART` it converts; without
`KV2_ARC_CONFORM_CURVES` it converts. **R0044's minimal converting set is
therefore `YANG_451_TRANSIT=1 YANG_453_SPAIR=1` plus the emission** — the
§3ae refinement put exact crease vertices on face 626's rails, so inc-8b's
pool-arc inserts (§3s) are no longer needed at this site. The census chain
past v47 is unchanged: v75/v76/v59 `TransitLeavesNeighbour`, v89/v39/v105
`PastNeighbour`, v38 `ChainAbsent`; op-2's v105 retry `ChordDegradation`
wall stands.

**Reading 2 — the corpus under the full set is fully attributed, and the one
WRONG is the surface-pair arm's known R0053, not the emission's.** Full
corpus, release, 8 jobs, 360 s CPU budget, wall 581.5 s, F0085 309.3 s,
under `YANG_451_TRANSIT=1 YANG_441_TORUS_CHART=1 YANG_453_SPAIR=1
KV2_ARC_CONFORM_CURVES=1 YANG_451_TRIPLE_DOMAIN=census
YANG_451_TRANSIT_ANATOMY=1 YANG_451_TRANSIT_EMIT=1`:
**275C / 1W / 30E / 1EE / 0T (+5 UNSUPPORTED)** against the canonical
273C / 0W / 34E / 1EE / 0T (+4). Exactly four cases move:

| case | canonical | gated | knob |
|---|---|---|---|
| R0011 | ERROR (v27 crossed carrier) | SUPPORTED_CORRECT | `YANG_451_TRANSIT` (§3j) |
| R0074 | ERROR (v91 off-curve) | SUPPORTED_CORRECT | `YANG_441_TORUS_CHART` (§3k) |
| R0044 | ERROR (v8) | UNSUPPORTED(curved-profile) | `TRANSIT` + `SPAIR` + emission |
| R0053 | ERROR (`FaceId(474)` ring) | **SUPPORTED_WRONG** χ = 0 vs 2 | `YANG_453_SPAIR` alone |

R0053 single-knob runs: gates off, `TRANSIT`, `TORUS_CHART`, `ARC_CONFORM`
each keep the `FaceId(474)` ERROR; `SPAIR` alone completes at
`V(28014) − E(84040) + F(56026) = 0`; all four at `28016 / 84046 / 56030 = 0`.
The emission arm has no work anywhere but R0044 v47 (§3y: R0003 yields no
edits), and the corpus confirms it: zero deltas beyond the four.

**Reading 3 — R0044's union passes every oracle the runner has, and the one
it "fails" is an authored expectation the reference overturns.** A gated
probe (`tests/s451_r0044_union_probe.rs`) truncates the document to the two
revolves and runs the runner's full validation on the union (gates off it
reports the v8 wall — the control):

| oracle | reading |
|---|---|
| watertight | all residue edges paired (86046 exact-closed, 6 residue, T-junction-subdivided) |
| consistent / outward / unit normals | 57367 / 57367; 100 % outward; 58241 unit |
| degenerate / indices / face ranges | none / all valid / 469 ranges, no gaps |
| self-intersection | none (109746 face pairs) |
| signed volume | 1.017912e11 (magnitude in band) |
| volume composition (§oracle, grid 64) | **Agree** rel 4.661e−5 ≤ band 2.696e−4 |
| Euler | `V(28685) − E(86052) + F(57367) = 0`, 1 shell — genus 1 |

The corpus meta's `euler_target = 2` names the three-op result and assumes
genus 0 throughout; for this prefix the expectation had to be ADJUDICATED,
not assumed (the R0011 precedent, `euler_target = 0`).

**Reading 4 — a lattice cannot adjudicate a grazing union; the reference
can.** A new independent TOPOLOGY oracle (`assay::topology_oracle`) voxelises
the SET UNION of the isolated operand solids — the volume oracle's own
trusted route — and reads the cubical complex's `V − E + F − C`, streaming
two layers at a time (1536³ in 36 s; 4096³ in 8.6 min), with a lattice-phase
control. Every synthetic shape reads exactly (ball 1, torus 0, genus-2 −1,
two balls 2, cavity 2; streamed ≡ in-memory, phase-invariant — 12 unit
tests). On the corpus it does NOT converge:

| case | n (phase ½) | χ |
|---|---|---|
| R0053 composed | 64 / 128 / 256 / 384 / 512 / 768 / 1024 / 1536 / 2048 / 3072 | −2 / 0 / 0 / 5 / 8 / −1 / 0 / 0 / 26 / 1 |
| R0053 composed, phase ¼ / ¾ | 1024; 1536 | −13 / 12; 19 / −20 |
| R0044 union composed | 128 / 256 / 512 / 1024 / 2048 / 4096 | 0 / 0 / 1 / 0 / 0 / −2 |
| R0044 union, phase ¼ / ¾ | 1024 | 1 / −1 |

(one face-connected component throughout; the KERNEL outputs voxelise
IDENTICALLY to the composed operands at every rung tried — cube counts
within 0.004 % — so at the lattice's resolution the kernel result is
set-equal to the true union in both cases.) The swings are the lattice
aliasing features that are thin at EVERY scale — the signature of operands
that GRAZE — so the instrument reports UNSTABLE and decides nothing. The
reference does: the Cherchi 2022 sidecar (roadmap §6) unions the operand
tessellations exactly, chained pairwise in operation order (its N-way form
refuses R0053 with *"a fully implicit patch that requires exact rationals"*
— the coplanar-graze signature), and `V − E + F` is read off its result
(`cherchi_sidecar::surface_topology`, exact-welded, shells and edge
manifoldness counted):

| case | reference union | kernel |
|---|---|---|
| R0044 revolve ∪ revolve | `V 28111 − E 84333 + F 56222 = 0`, 1 closed manifold shell — **genus 1** | χ = 0, 1 shell — **agrees** |
| R0053 ops 0..1 (rect revolve ∪ box) | χ = 0 — genus 1 (the box bridges the C-ring) | — |
| R0053 ops 0..2 (∪ gear revolve) | `V 26031 − E 78177 + F 52118 = −28`, 1 closed manifold shell — **genus 15** | χ = 0 under `SPAIR` — **14 handles short** |

Both gears carry 16 teeth; the fourteen extra handles are the gear's grooves
tunnelling under the ring at the graze. (The operand tessellations are
resolution-fixed — 55548 triangles for the gear revolve at either scale,
unchanged under `ORACLE_TOL_SCALE` 0.3 / 1 / 3 because the oracle tolerance
clamps at 1e−3 — so tessellation-invariance of the reference genus is NOT
measured here; recorded, not assumed.)

**What this decides.**

1. **The epic's target conversion is real.** R0044's union — the standing
   `FaceId(627)` / v8 walls — completes under `TRANSIT + SPAIR + emission`
   and is CORRECT by every runner oracle and by reference topology. The
   case's next wall is a different capability: the cut's operand is a
   curved boolean result whose surface-pair-bounded lateral cannot re-enter
   Stage 1 (partial-patch tessellation; the existing `UNSUPPORTED(curved-
   profile)` boundary, now with a third customer).
2. **§3s's flip condition is adjudicated, on its second branch.** R0053's
   completion under `YANG_453_SPAIR` is a genuine silent-wrong: the loud
   `FaceId(474)` stop was masking an M8 coplanar-graze defect the arm has
   nothing to do with, and the authored χ = 2 catching it is luck (the
   reference says −28; neither 2 nor 0 is the truth). `SPAIR` stays gated;
   its flip waits on M8 Stage-0 making the graze LOUD at R0053 — the P10
   route — never on a band. Because R0044's minimal set needs `SPAIR`,
   R0044's conversion is queued behind that.
3. **An authored `euler_target` can now be adjudicated by instrument** —
   `assay_topology_oracle` (`TOPO_SIDECAR=1`) — instead of by hand; the
   lattice ladder is the cheap first look, and its instability is itself
   a graze detector.

**Not built here.** No kernel code changed — the canonical corpus is
byte-identical by construction. Open: M8 Stage-0 at R0053 (the `SPAIR`
flip condition); the Stage-1 partial-patch re-entry (R0044's new wall and
the `UNSUPPORTED(curved-profile)` family); the emission arm's own flip
(zero corpus deltas without `TRANSIT`+`SPAIR`, so its two proofs are
byte-identity plus the R0044 probe); the seven other R0044 fires.

> **SUPERSEDED (§3ah, 2026-09-03).** Readings 4's R0053 verdict and the
> addendum below are RETRACTED: by exact analytic membership on a stable
> ladder the union is genus 1 — the kernel's reading — and the sidecar's
> genus-15 mesh has 606 faces strictly inside the true solid. There are no
> fourteen handles; §3s's flip condition is met on its first branch. The
> text is kept as the record of how a mesh-borne reference misadjudicated.

**Addendum (same session) — WHERE R0053 loses the fourteen handles: at
Stage 0–2, not at the surface-pair arm.** `YANG_CHI_AUDIT=1` on R0053, with
and without `YANG_453_SPAIR`, reads identically through Stage 2: op-1
(rect revolve ∪ box) inputs χ = 2 / 2 → `stage2-kept-submesh V(70) − E(210)
+ F(140) = 0` (genus 1, the box bridging the C-ring — the reference agrees);
op-2 (∪ gear revolve) inputs χ = 0 / 2 → **`stage2-kept-submesh V(15931) −
E(47793) + F(31862) = 0`** — genus 1 already, every edge 2-used, zero
pinches, before any Stage-4 arm runs. The `SPAIR` arm changes nothing
topological (post-construct 15504 / 46512 / 31008 = 0 with it, 15538 /
46614 / 31076 = 0 without). So the arm is exonerated as designed; the
handles are absent from the exact arrangement's kept set. Stage 0 IS active
on both booleans (`stage0: true`, one opposite-facing coplanar pair each —
the box's base flush with the revolves' θ = 0 caps), and Stage 1 tessellates
the gear at 16128 raw / 17369 overlaid vertices against the harness's
27776. A like-for-like reference on yang's OWN Stage-2 inputs
(`YANG_STAGE0_DUMP_DIR`, then the sidecar) is INCONCLUSIVE: on the overlaid
meshes the sidecar returns non-manifold results (op-1: 4 non-manifold
edges, χ = 1; op-2: 264 boundary + 578 non-manifold edges, χ = 0) — its
documented limitation on fully implicit coplanar patches — so the reference
verdict stands only on the independently tessellated operands (genus 15,
closed, manifold). Whether the tunnels are analytic truth (the gear tips
penetrate the ring; yang's coarser tips fall short — a §4.5.2 resolution
question) or a tessellation-side artefact of a tangent graze is exactly the
M8 coplanar-graze adjudication R0053 has always owed, now with its stage
named: the classification/overlay at Stage 0–2, not Stage 4.

## 3ag. inc-3a — the CORRIDOR arm and the TORUS chart FLIPPED always-on
(LANDED 2026-09-03; the epic's first two conversions enter the canonical
score: 273C → 275C)

§3af measured the corridor family in combination and attributed every
delta to its knob. The two knobs whose conversions carry no regression and
need nothing else — `YANG_451_TRANSIT` (§3j, R0011) and
`YANG_441_TORUS_CHART` (§3k, R0074; inert without the corridor arm) — flip
here under the household pattern the I13a cone chart and the f4 rehome
flip set: unset/other = ON, `0|off` = the dev A/B off-knob. Two-proof
corpus protocol, release, 8 jobs, 360 s CPU budget:

| run | score (+4 UNSUPPORTED) | `results.json` | wall |
|---|---|---|---|
| pre-flip, `YANG_451_TRANSIT=1` only | 274C / 0W / 33E / 1EE / 0T | R0011 E→C; R0044 ERROR detail advances past v8; zero other category OR detail moves | 531.1 s |
| pre-flip, `TRANSIT=1 TORUS_CHART=1` | 275C / 0W / 32E / 1EE / 0T | + R0074 E→C; R0044's detail = `FaceId(459)` (§3s's fold); nothing else | 534.6 s |
| **post-flip default** | **275C / 0W / 32E / 1EE / 0T** | **BYTE-IDENTICAL to the pre-flip two-knob run** (F0085 307.2 s, F0065 170.2 s honest) | 538.2 s |
| post-flip `TRANSIT=0 TORUS_CHART=0` | 273C / 0W / 34E / 1EE / 0T | **BYTE-IDENTICAL to the committed canonical** | 541.8 s |

The committed `results.json` moves with the conversions (the UI snapshot);
no smoke-table pin names R0011 or R0074. yang-rs unit suite 1276 / 0
failed; the torus-chart knob test now sets `0` explicitly for its off-arm.
`YANG_453_SPAIR`, `KV2_ARC_CONFORM_CURVES` and the emission arm stay gated
(§3af: `SPAIR` waits on M8 at R0053; the other two convert nothing on their
own). **New canonical: 275C / 0W / 32E / 1EE / 0T.** R0044 now walls
canonically at `FaceId(459)` — the §3s surface-pair fold — instead of v8.

## 3ah. inc-3b — R0053 ADJUDICATED by exact membership: genus 1, the
authored χ was the default, the sidecar reference was a mesh inside the
solid; `YANG_453_SPAIR` FLIPPED always-on (LANDED 2026-09-03)

§3af closed on two readings of R0053's union that both went through a
mesh: the lattice ladder over the operand TESSELLATIONS (unstable) and the
Cherchi sidecar's union of those same tessellations (χ = −28, "genus 15",
attributed to "fourteen gear-groove tunnels at the graze" — an inference,
never located). Its addendum then found the exact arrangement's kept
submesh already at genus 1 and named "the overlay/classification at Stage
0–2" as where the fourteen handles were lost. This increment removes the
mesh from the question and finds there were never fourteen handles to
lose.

**The configuration, measured from the authored numbers.** All three
sketches share one plane; the sketch x axis is the revolve axis direction
(`SketchPlaneBasis` puts `x̂ = −â` exactly). In the sketch frame `(u, v, w)`
(`w` = the plane normal, the extrude's direction): the ring is the
rectangle `|u| ≤ 20.814, |v| ≤ 44.556` revolved 287.61° about the line
`v = −62.443` (radial band 17.89…107.00), the box is `|u| ≤ 41.494,
|v| ≤ 52.075, w ∈ [0, 100.27]`, and the gear (16 teeth, module 7.455:
root 50.323, base 56.046, addendum 67.098; half-tooth 6.479° at the base,
2.119° at the tip) is revolved 301.92° about the PARALLEL line
`v = −89.464`, 27.021 further out. Both revolves sweep toward `−w` first
(`m̂ = â × ŵ = −n̂`), so their gaps lie on the box's side and the box
bridges the C-ring (§3af's op-1 reading, confirmed). The ring's
cross-section lives entirely inside the gear's axial band `|u| ≤ 20.814`,
so it can meet only the teeth whose flanks reach that band: a census of
the ring solid against the gear's meridian disc
(`s453_r0053_exact_topology::r0053_ring_vs_teeth`) finds ring material in
exactly THREE teeth — 247.5°, 270°, 292.5° — and in their two grooves from
the root radius up to the addendum, i.e. the ring FILLS those grooves
from the root side and embeds the teeth; it never covers a groove with an
open floor. Thirteen teeth never touch the ring at all.

**The exact-membership ladder** (`s453_r0053_exact_topology::
r0053_exact_ladder`): each operand is a closed-form point predicate — the
revolves through `(ρ, θ, v_unrotated)` with kernel-v2's sign conventions,
the gear disc through the involute half-width `π/(2N) + inv(α) − inv(t)`,
`t = √((r/r_b)² − 1)` — and the cubical `V − E + F − C` of the SET UNION is
read on the in-memory grid (`VoxelGrid::from_fn`) over the joint bounding
box at cell size `h`:

| set | h = 2 | 1 | 0.7 | 0.5 | 0.4 | phase ¼ at 0.7 / 0.5 / 0.4 |
|---|---|---|---|---|---|---|
| ring / box / gear alone | 1 / 1 / 1 | 1 / 1 / 1 | | | | |
| ring ∪ box | 0 | 0 | | | | |
| ring ∪ gear | | | 1 | 1 | 1 | 1 / 1 / 1 |
| **ring ∪ box ∪ gear** | **0** | **0** | **0** | **0** | **0** | **0 / 0 / 0** |

One face-connected component throughout; the finest rung is 94.7 M cubes
(`n = [346, 793, 793]`, 7.9 s). A predicate cannot alias a coplanar
overlap (the union is solid across the shared plane) and cannot mint a
sliver, so the ladder has nothing to be unstable about — and it is not.
**R0053's union is genus 1** (χ_solid 0, boundary χ 0, one shell): the box
bridges the C-ring, and the gear adds no handle. Volume 6.062e6 ± 0.1 %
across rungs.

**The kernel is right; the reference was never the union's boundary.**
Face-by-face classification against the exact predicate (probe each
centroid ± δ along its normal, δ = 0.05…2.0; a boundary face has inside
on one side and outside on the other at the smallest δ that clears the
chord band):

| mesh | faces | strictly INSIDE the exact solid | strictly outside | where |
|---|---|---|---|---|
| kernel-v2 operand tessellations (controls) | 468 / 12 / 55548 | 0 | 0 | — (every face within 0.5 of the exact surface: the predicates match the kernel's gear) |
| sidecar ring ∪ box | 552 | 9 (area ≈ 4011) | 0 | the ring's θ = 0 cap region, solid on both sides |
| **sidecar ring ∪ box ∪ gear (χ = −28)** | 52118 | **606 in 65 clusters** (largest 85 faces, area 1552, the gear-cap ∩ box-base overlap at `w = 0`; then 59 / 59 / 45 / 28 / 14 / 13 / 11 more coplanar membranes) | 5 | membranes at the shared plane, and sliver strips of area 0.06–0.5 at `u = ±20.814` along the ring-end / tooth-flank crossings (φ ≈ 251° and 289°, r ≈ 64.4–65.4; eleven such strips among the twenty largest clusters, at θ₂ = 43°…286°) |
| **kernel-v2 R0053 under `SPAIR`** | 56026 | **38** (4 × 9 micro-faces of area 0.011 at the box-edge / root-arc crossings, 2 faces of area 0.35 on the root arc) | 1 (area 0) | all at `w = 0` inside the root arc's chord band — tessellation-band residue, not geometry |

The sidecar meshes are closed 2-manifolds (0 boundary, 0 non-manifold
edges); dropping their inside faces leaves 13 / 670 boundary edges, so
the false walls are WOVEN INTO the surface — the closed surface runs
through the solid. Its `V − E + F` is therefore not the union's; "genus
15" was the pairwise sidecar's coplanar handling on this input (the same
limitation whose N-way form refuses the case outright), and the fourteen
"tunnels" were its sliver strips. The kernel's output reads χ = 0 with one
shell by the runner's Euler oracle (`V(28014) − E(84040) + F(56026)`),
signed volume 6.048e6 (the tessellated operands' inscribed deficit; the
sidecar's 6.0479e6 sits at the same value), and carries no face inside the
exact solid beyond the root arc's chord band.

**What this decides.**

1. **§3s's flip condition is met on its FIRST branch**: R0053's authored
   `euler_target = 2` was the generator's genus-0 default, refuted by an
   exact derivation from the authored numbers (the R0011 protocol,
   `historical_authoring_fixes_pinned`). The meta is corrected to 0 and
   pinned; the completion under the surface-pair arm is CORRECT.
2. **§3af's second-branch adjudication is RETRACTED**, and with it the
   addendum's "handles lost at Stage 0–2" and the M8 silent-wrong at
   R0053: there is no coplanar-graze defect here for Stage 0 to make loud.
   Stage 0 handles the flush caps correctly. The lesson is the reference
   oracle's own (`feedback_reference_oracle_invalidates_in_both_
   directions`): a reference whose output was not checked against the
   input's own membership adjudicated a kernel result WRONG that was
   right. A reference mesh must be certified — closed, manifold, AND no
   face inside the exact solid — before its χ is a verdict.
3. **`YANG_453_SPAIR` flips always-on** (household pattern, `0|off` = dev
   A/B). Two-proof corpus (release, 8 jobs, 360 s CPU budget), with the
   corrected meta in place for every run:

| run | score (+4 UNSUPPORTED) | `results.json` | wall |
|---|---|---|---|
| pre-flip, `YANG_453_SPAIR=1` | 276C / 0W / 31E / 1EE / 0T | vs canonical 275C: exactly TWO moves — R0053 ERROR → SUPPORTED_CORRECT ("all checks passed"); R0044's ERROR detail `FaceId(459)` ring rejected → `FaceId(626)` patch triangulation folded (§3s's reading, the §4.5.3 fold collapsed); nothing else | 557.6 s (F0085 298.8 s) |
| **post-flip default** | **276C / 0W / 31E / 1EE / 0T** | **BYTE-IDENTICAL to the pre-flip proof** | 571.2 s (F0085 311.6 s, F0065 171.7 s honest) |
| post-flip `YANG_453_SPAIR=0` | 275C / 0W / 32E / 1EE / 0T | **BYTE-IDENTICAL to the committed canonical** | 532.5 s (F0085 301.6 s) |

The committed `results.json` moves with the conversion (the UI snapshot);
no smoke-table pin names R0053. **New canonical: 276C / 0W / 31E / 1EE /
0T.** R0044 now walls canonically at `FaceId(626)` — the KV9-F2 fold on
the cone face the emission arm (§3ac–§3ae, still gated) repairs — and its
minimal converting set is now the emission chain alone on top of the
defaults.

**Instruments landed.** `tests/s453_r0053_exact_topology.rs` (the exact
predicates, the ladder, the ring-vs-teeth census — R0053's authored numbers
inline, so the probe reads the same document the corpus does without a
kernel in the loop) and `tests/s453_r0053_output_obj.rs` (the kernel's
R0053 output to OBJ for the face classifier). The classifier itself ran
as a scratch script this session. The generalisation landed the same day:
`assay::exact_membership` parses any corpus document into closed-form
predicates (polygon / circle / involute-gear profiles, extrude and revolve
with the feature engine's own semantics, cuts included) and lays its
lattice in the document's sketch frame — a lattice OBLIQUE to the model's
planes perforates frame-aligned thin features at every cell size (R0053
in the world frame: χ_solid +3 / −3 / −6 / −8 at h = 1.04 / 0.75 / 0.52 /
0.37 (phase ¼); in the sketch frame 0 at every rung from 2 to 0.3 on two
phases). The pinned adjudications (F0001, R0053, R0011, R0099, C0075,
R0091's volume, R0063 as a heavy pin) are its tests; the corpus sweep
found three kernel silent-wrong classes the mesh-borne oracles cannot see
(`docs/audits/exact_membership_sweep_2026_09_03.md`).

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
- **inc-2c-2 + 3a** (2026-08-30, second session): corridor ASSEMBLY
  LANDED + MEASURED family-wide (§3h) — `assemble_corridors` (greedy
  spine grouping, contract-band splice dispositions, locality-filtered
  run sourcing with `Spliced` chains, cross-corridor conflict/
  SHARED-MINT identity) + the cycle-surgery planner census (`-CYCLES`:
  attachment certificates, junction host edges, tagged windows). THREE
  designs corrected by measurement (cross-invocation "merges";
  endpoint-shared corridors; distant-chain locality). R0011 4/4 +
  R0074 1/1 + R0044 7/7 applyable & fully consumed; R0085 walled with
  the all-consumed admission rule named (§3h-3a).
- **inc-2c-3b-0** (2026-08-30, same session): the corrected-cycle
  PLANNER landed (`stage4_corridor.rs`: `replace_subpath` + three
  certificate-gated generators + the corner-anchored removability
  sign + `MintPool`/`corridor_path`) and MEASURED on R0011 — 18
  component plans across both invocations, 0 declines, every §3h-3a
  prediction reproduced; §3d's "q stays" REFUTED (crossed corners are
  excised everywhere, soundly — §3i).
- **inc-2c-3b-1** (2026-08-31): the gated mutation LANDED
  (`corner_transit_apply`, `YANG_451_TRANSIT`, default OFF) — full
  admission-certified pipeline + fan-local far arm (the wholesale
  far rebuild REFUTED live: cylinder lateral, TriangulationFailed /
  ChordDegradation — the I6 lesson) + wholesale B-side rebuilds +
  all-or-nothing batch with rollback. **FIRST LIVE APPLY: R0011 op-1
  repairs (3 corridors / 13 plans / 11 mints / 11 removed), §4-I9
  passes, the boolean completes; the case moves to the downstream
  FaceId(402) ring-rejection wall** (§3j).
- **inc-2c-3b-2 ANCHOR** (2026-08-31, same session): FaceId(402) = the
  repaired far cylinder face; the rejected ring's spike is the
  chain-end neighbour v26 sitting 4.7 PAST the minted junction with an
  anti-parallel connector (a NON-corner out-of-domain slide §4-I9
  cannot see; both holes show the same shape; provenance suspicion
  refuted). Fix shape: §4.4.1 near-curve removal at corridor ends —
  absorb junction-band/wrong-side chain-end neighbours into the mint
  (Fig-11 merge), connector-direction certificate. Band + B-side-fan
  questions to measure first (§3j).
- **inc-2c-3b-2 MEASURED + LANDED** (2026-08-31, next session): the
  ABSORB census answered both sub-questions (certificate = the SIGN of
  the connector dot, never a band — defects at exactly −1.0000 vs
  healthy ≥ +0.456, d_eps would over-absorb; absorbed fans live on
  far + terminal-outer only — already-planned keys), and the absorb
  arm landed (`absorb_anchor` in generators A/B with
  `Removability::Absorbed` + mint-pool NEW-ref continuations;
  far arm = flood the far plan's removed set from the phantom,
  `delete_boundary_fan_set`, membership-resolved link ends). **R0011
  APPLIES removed=13, completes end-to-end, and CONVERTS —
  SUPPORTED_CORRECT** after the χ=0 verdict was adjudicated as ORACLE
  AUTHORING (true genus 1, proven by exact-involute voxel-CSG + the
  1×/2×/4× density ladder + volume/watertight/shell; `euler_target`
  0 authored + pinned). Flow correction: R0011 runs ONE design
  boolean; the "op-2" §4-I9 invocation is the §4.5.4 refine retry
  (the apply generalizes: 2× fires one corridor and repairs it; 4×'s
  natural declines NoRealCandidate and the refine-retry repairs).
  (§3j.)
- **inc-2c-3b-3** (2026-08-31, third session): the fan-local TORUS
  chart LANDED (`SurfaceChart::Torus` on the pinned dt embedding;
  `new_local`, `YANG_441_TORUS_CHART` default OFF; `refill_fan_hole`
  double-periodic unwrap+guards; `new`/`supports` untouched — the
  wholesale torus-seam question stays a named future slice). **R0074
  CONVERTS — SUPPORTED_CORRECT, all standing checks, the epic's
  SECOND conversion**; torus-knob-off reproduces the NonPlanarPatch
  refusal verbatim; gates-off byte-identical; R0011 knob-inert (§3k).
- **inc-2c-3b-4** (2026-08-31, third session): curve-aware host
  admission LANDED (`arc_host_admit`: eval arm verbatim + certified
  arc arm — `t ∈ [0,1]` + junction-on-the-chord's-own-curve at the
  CONTRACT band; the blunt d_eps band alone measured as POISON —
  HostMismatch killed whole plans). R0044 op-1 declines 11 → 3; the
  arc-host wall falls; remaining: NotRemovable{v92} + the v142/v144
  mirrored-pair entanglement + v105's far link-end attachment.
  R0011/R0074 byte-identical under the new rule. `-HOSTS` census
  probe kept (§3l).
- **inc-2c-3b-5** (2026-08-31, third session): the mirrored-pair
  PLANNER stage LANDED — corner-clip C pairs (same-junction two-edge
  excision), shared-mint view dedup, component-tagged declines,
  cycle-bounded absorb (the cap-4 declined a measured 4-deep reversal
  one short), best-effort unattached-flank absorb in generator B
  (v141, the between-twin-phantoms remnant), splice-aware
  affected_keys. **R0044 op-1 PLANS COMPLETELY**; the refusal moves to
  the far arm's joint-region flood guard (§3m).
- **inc-2c-3b-6** (2026-08-31, third session): JOINT far regions +
  `stitch_fan_polygon` (link runs ⨯ all-New corrected arcs) +
  `refill_fan_hole_seeded` (density-capped Steiner refinement) +
  the wrap-band dispatch (wholesale's own typed verdicts route
  full-wrap/fold bands through the fan-local machinery). FIVE R0044
  walls fall; the case stops typed at the batch-integrity scan (§3n).
- **inc-2c-3b-7 PARTIAL** (2026-08-31, third session): phantom-
  membership keys + planner phantom-presence affectedness (B:0 plans)
  + the closure SWEEP (synthesized correction, sliver-empty rebuilds).
  R0044 stops typed at B:0's `AttachmentMismatch{v142}` (§3o).
- **inc-2c-3b-8** (2026-08-31, fourth session): the base-leg
  REFUTATION + the total-excision closure (§3p). The rim-domain
  census (`face_edge_roots_probe` on the B-Rep loops) adjudicated the
  candidate base junctions OUT-OF-DOMAIN (t = −0.924 / +1.787 beyond
  the corner; the census had converged onto the phantoms' own
  relocated positions — circular); the wall-bottom seam corner is
  wholly inside the far body and comp B:0/11 is a 2-triangle
  wrongly-kept pocket. Landed: the planner (None,None)+no-hosts
  fall-through, the sweep's whole-component excision + contained-strip
  2-gon closure (structural certificate, NO band), batch-carried
  seeds (the StalePlan anatomy). **R0044's natural invocation APPLIES
  — corridors=6 plans=20 mints=15 removed=25 — and stops typed at
  Stage-6 NonManifoldOutput (v13998)**; the `[451-audit]` post-batch
  watertightness census names 16 unpaired directed edges in 4 repair
  neighbourhoods. R0011/R0074 byte-identical, gates-off verbatim.
- **inc-2c-3b-9a** (2026-08-31, fourth session): the conformality
  witness census + (A) the removed-union filter (plan-level; three
  retained host survivors measured) + (C) the survivor-testimony
  orientation certificate (ALWAYS-ON: the folded-fossil area vector
  flipped one refill triangle; full-corpus proof). 16 unpaired edges
  → 10; the residue is ONE anatomy — the ABSORBED CONTINUATION
  (§3q).
- **inc-2c-3b-9b** (2026-08-31, fourth session): the
  STANDING-JUNCTION certificate (§3r) — a 3-membership vertex within
  CONTRACT of its own triple solution is never absorbed (both triple
  shapes, incl. the far-op crease v107); the absorb was DELETING true
  junctions the relocation had already placed. The continuation walk
  landed + live-validated (found the census-predicted crossings) with
  its misfire guards (Mint-terminal-only, junction-carrier skip,
  nearest-junction attachments) but no R0044 span retains a displaced
  crosser once standing holds. **Audit CLEAN — 0 unpaired edges;
  R0044's design boolean COMPLETES (removed=20)**; next walls:
  output-tessellation FaceId(459) ring spike (the §3j FaceId(402)
  family) + the v105 retry ChordDegradation. R0011/R0074
  byte-identical; gates-off verbatim.
- **inc-2c-3b-10** (2026-08-31, fourth session): the §4.5.3
  SURFACE-PAIR tangent arm (§3s) — `mixed_cycle_shared_conic` admits
  SurfacePair (unordered exact equality) and `conic_param_deltas`
  gains the analytic-tangent projection test (T = n_a × n_b at the
  site; monotone chains healthy at any coarseness). R0044's
  FaceId(459) ring wall falls → FaceId(626) "triangulation folded"
  (unmeasured). **GATED `YANG_453_SPAIR` default OFF: the always-on
  corpus run measured ONE E→W (R0053, M8 coplanar-graze, χ=0 vs 2 —
  the fold's ring rejection was the loud stop masking the M8 gap);
  flip condition = R0053's χ adjudication or M8 Stage-0.** R0044's
  conversion path now reads
  `YANG_451_TRANSIT=1 YANG_441_TORUS_CHART=1 YANG_453_SPAIR=1`.
- **inc-2c-3b-11** (2026-08-31, fifth session): the FaceId(626) wall
  ANCHORED and RESOLVED (§3s "RESOLVED" block) — the one-sided
  conforming-insert fold on a legitimate ~304° near-sliver carried
  band (pool is EDGE-local, fold constraint is FACE-local; a face-627
  vertex inserts on rail 1 only, mid-chord of rail 2's sagging rung).
  Fixed in kernel-v2 as **inc-8b** (`sampling.rs`,
  spec `yang_434_output_chord_refinement.md`): the gated inc-8a curve
  pool completed to depth 1 — pool arcs contribute grid samples PLUS
  their own vertex-pool inserts; azimuth-set closure exact at depth 1;
  R0044 is inc-8a's first corpus customer. Unit-pinned both ways;
  default path untouched (inside the `KV2_ARC_CONFORM_CURVES` gate,
  still default OFF pending its own corpus flip proof). R0044's
  conversion path is now FOUR knobs (`… KV2_ARC_CONFORM_CURVES=1`);
  face 626 tessellates clean and the case advances to the UNMASKED
  `FaceId(627)` ring rejection, anchored same session: the notch
  chain's SurfacePair→HyperbolaArc junction X sits ON face 627's cone
  but 0.827 PAST the station of the rim that BOUNDS it, landing inside
  the neighbour band (face 626, which carries no notch) — the 3b-8
  identification-vs-domain shape at the emission layer; the ring
  self-intersects its own rim run twice and the CDT rejects correctly.
  ADJUDICATED same session: X is the EXACT cylinder-end-circle ∩ cone
  triple point (three residuals ~1e-13, closed-form solve returns it to
  8.2e-13) — correctly computed on the EXTENDED cone, 0.827 outside its
  domain. The true junction J exists on cone 626 (station 2307.654,
  inside 626's band) and both crease crossings exist on the shared rim
  (P_lat, P_cap). inc-2c-3b-12 = split the crease at P_lat/P_cap,
  re-terminate 627's chains there, build the 626-side notch through J.
- **inc-2c-3b-12** (2026-08-31, fifth session): the relocation DOMAIN
  certificate LANDED, GATED `YANG_451_TRIPLE_DOMAIN` (§3t). Anchor
  confirmed empirically by backtrace: R0044's out-of-domain junction is
  minted by the TRIPLE-JUNCTION relocation arm, whose only acceptance
  gate is a displacement corridor the 18.07 travel passes at p80 of the
  case's own 306-relocation distribution — the arm has NO domain
  postcondition. Yang §4.5.1 states this trigger verbatim ("a full step
  … outside the surface S2 where the point is initially located") and
  prescribes truncate → transit → q-points; `stage4_truncate`'s header
  had already predicted this exact join. Built: `crease_circle_from_pair`
  (Cone×Cone coaxial, Cone×Plane ⊥, Cylinder×Cone — circles only, every
  other configuration declines), the BY-SURFACE crease index (sourcing
  from the vertex's own edges measured wrong — v47 sits 10.5 from the
  crease it overruns), and `crease_crossed_by_step` with two membership
  exemptions rather than thresholds (on-crease gliding; a PROPAGATED
  band, since a derived plane cannot be certified more tightly than its
  parents). R0044: 8 material fires (0.309 … 40.08) against a ~5e-11
  band — ten orders of separation — with five crease-riding noise fires
  correctly exempted. v105 is named independently by §4-I9 and by the
  §4.5.4 retry. 7 unit tests; default path byte-identical.
  **Full-corpus census: canonical 273C/0W/34E/1EE/0T (census mode is
  behaviour-neutral), firing in exactly TWO cases — R0044 (ERROR, 8
  fires, 0.309…40.08) and R0003 (SUPPORTED_CORRECT, 6 fires,
  0.00078…0.265). A CORRECT case carries genuine out-of-domain
  relocations and survives them, so the STOP must NEVER be armed as it
  stands; the 1.2× gap between the two cases' overrun ranges is
  explicitly REFUSED as a discriminator (six and eight samples, nothing
  structural — the band-tuning P10 forbids).** Detection only; the
  repair (truncate → transit → q-points, splitting the crease so 626
  receives its notch) is 3b-12b.
- **inc-2c-3b-12b-0** (2026-09-01): the §4.5.1 REPAIR SOLVER landed as
  a pure function (§3u) — `solve_crease_transit` executes the paper's
  four steps in its order (truncate to `C_b` → transit onto `S1` →
  certify → solve `q1`/`q2` on `C_b`), composing four primitives that
  already existed (`project_onto_curve`, `crease_circle_from_pair`,
  `relocate_onto_implicit_triple`, `circle_surface_roots`). Its honesty
  postcondition re-applies the §3t certificate to the corrected step, so
  a transit that leaves the NEIGHBOUR's domain in turn is a typed
  decline carrying its own measured residuals, never an iteration.
  **Validated against §3s's independently-derived values**: the
  correction reproduces 0.138 and both q-points agree to their last
  recorded digit (1.4e-6 / 1.1e-6) — and two unplanned cross-validations
  fell out, v38's `q2` ≡ v47's `j` to 6.4e-13 (inside the ~1.1e-11
  evaluation band; the same physical point by two unrelated paths) and
  R0003's v8658/v11356 to 2.8e-14. **Census: 11 of 14 sites DETERMINED —
  R0044 5/8 (corrections 0.138 … 8.72; three honest
  `TransitLeavesNeighbour` declines at 17–30-unit second overruns) and
  R0003 6/6 (1.48e-3 … 3.82e-2).** That R0003 row is what §3t's binding
  constraint asked for: the repair is determined on the SUPPORTED_CORRECT
  case too, so the two populations are separated by fixing both rather
  than by a magnitude band. 5 new unit tests on an exactly-constructed
  fixture (66² + 88² = 110², so every expected value is closed-form
  rather than transcribed). Census-only: reachable solely under
  `YANG_451_TRIPLE_DOMAIN`, default path untouched. The emission half —
  re-terminating 627's chains at the q-points, splitting the crease and
  building 626's notch through `J` — is 3b-12b-1, and the census
  establishes WHY it cannot be a relocation: `J` is on cone 626 by
  construction, so it is no more inside 627's domain than `X` was.
- **inc-2c-3b-12b-1** (2026-09-01, same session): the EMISSION-half site
  ANATOMY landed pure + census-only (`transit_site_anatomy`, gated
  `YANG_451_TRANSIT_ANATOMY`), and MEASURED on all 11 determined sites
  (§3v). Three readings. (a) The anatomy is ONE shape eleven times: every
  fan straddles exactly THREE input faces, and for v47 the attribution
  names the two chains directly (`v47–45` lateral/cone = the SurfacePair,
  `v47–280` cap/cone = the hyperbola). (b) The `Past` one-ring
  neighbours are already-relocated SIBLINGS — R0044's v39 ring carries
  v38 at its own recorded `d_post` (−3.092587e-1, same crease) — so
  v38/v39/v59 are one cluster and the repair unit is the cluster.
  (c) **The q-points' REPRESENTABILITY splits the population
  structurally**, which is the discriminator §3t refused to take as a
  magnitude band: in 3 sites (R0003 v1983/v8658/v11356) both q-points ARE
  existing one-ring vertices to 7.6e-15 … 1.7e-12, so the repair is the
  relocation and nothing else; in R0044 v47 the crease is carried but as
  a 558.53-long rim chord with the q-points 10.39/10.36 off it at
  mid-chord, so the corner is not representable without refining the
  chain; and in the remaining 7 the crease has no local mesh chain at all
  (v38/v39: nearest is 497.9 away). Also charted `FaceId(627)`'s emitted
  loop in its own cone frame: the notch spans the FULL band height (both
  ends exactly on the lower rim), exceeds the top rim by 0.827, and exits
  through a 2.285-arc window — reproducing §3s's independently measured
  2.29 chart width from the loop's own coordinates — so the corrected
  emission CUTS the 304.56° sector in two rather than denting it, a split
  that falls out of `flood_fill_patches` once the mesh is re-attributed
  rather than being constructed. 3 new unit tests on the §3u fixture.
- **inc-2c-3b-12b-2** (2026-09-01, same session): the CUT PATH across the
  own patch landed pure + census-only (`transit_cut_path`, same gate) and
  MEASURED (§3w). The corner has THREE chains, not two — the first model
  required two and declined at all 11 sites — and they differ in role: two
  involve the own surface and terminate at the q-points, the third joins
  the two OTHER surfaces and is the CARRIER the site glides along (for v47
  it is the cylinder's own END circle, of which `X` and `J` are the cone-627
  and cone-626 intersections, so the 0.138 correction is a step along it).
  With that model **7 of 11 sites yield a determined cut**; the other 4 are
  exactly the `Past`-neighbour cluster sites, so the population partitions
  cleanly with no third failure mode. Two further measurements. (a)
  Assigning a chain to its q-point by PROXIMITY is wrong at EVERY site
  where both chains are crossed edges — margins −1.855, −0.0525, −0.361,
  −0.775 — because both chain edges leave the same site, so both chord
  crossings cluster near it; the rule that works is surface IDENTITY, and
  `CreaseTransit` now carries `others` so the q↔surface correspondence is
  transported rather than re-derived (a chain that does not resolve is a
  typed `QSurfaceUnmatched` decline, never a nearest guess). (b) The cut has
  ONE shape at all 7: `q → (Vertex|Refined)* → q` with EXACTLY ONE
  refinement crossing, whose `lift` reproduces §3v's independent chain-sag
  reading (10.181 at v47 against the anatomy's 10.39/10.36 for the same rim
  chord; 4.138 at v38; 0.047 … 0.564 across R0003). Unplanned
  cross-check: R0003's v8658 and v8809 are each other's carrier vertex.
  3 new unit tests, including a deliberately adversarial fixture where
  proximity picks the wrong q.
- **inc-2c-3b-12b-3** (2026-09-01, same session): the EMISSION PLAN landed
  pure + census-only (`transit_emission_plan`, same gate) and MEASURED on all
  7 determined sites (§3x). It reports the acquisition as TWO independent
  sides — the chain that terminates at a q-point and the crease's own mesh
  chain — because a site can already carry one and not the other. **The crease
  side partitions the population 3/1/3 structurally**: three sites already have
  both q-points as crease vertices (`AtEnd`), R0044 v47 has the crease as a
  single 558.5283 chord the corner falls inside (`Interior`, t = 0.4280/0.4240,
  off_chord 10.38885/10.36319 — reproducing §3v's independently measured
  10.39/10.36), and three carry no crease edge in the fan at all (`NoChain`).
  **BINDING: at the three `AtEnd` sites the chain COVERS THE CORNER TWICE and
  the doubled cover IS the corner** — two fan crease edges each running from
  one q-point past the other, overlapping by 0.0309179230680… / 0.0914875794…°
  against corners of the same value, each residual (−2.8e-14 … 4.7e-13)
  decomposing exactly into the two q-vertices' own angular offsets. So at those
  three the defect is not resolution at all: the mesh needs no new vertex, only
  the removal of a doubled cover the site's out-of-domain position dragged into
  it, and the repair is a RE-ORDERING. §3w's "the emission half needs §4.5.2
  local refinement" stands for the other four and is narrowed for these three.
  Also measured: the corner arc's own sagitta is 1.9e-7 … 1.1e-3, so the arc is
  ONE edge and every insert is AT a q-point, never between them; and the fan's
  angular over-reach — what a re-attribution would have claimed — is 22× …
  1508× the corner. **A band test in the first version was WRONG and the census
  caught it**: v1983's q1 vertex sits 1.7364e-12 from the solved point against
  a ~1.1e-12 band, so the band disowned a q-point the mesh demonstrably carries
  and demanded an insert on an edge that already ends at it; the rule is
  IDENTITY from the cut's own termination. 4 unit tests, all closed-form, one
  of them pinning that fix at 5× the band.
- **inc-2c-3b-12b-4** (2026-09-01, same session): the EMISSION EDIT LIST
  landed pure + census-only (`transit_emission_edits`, same gates) and
  MEASURED on all 7 determined sites (§3y). It turns §3x's acquisition into
  mints and triangle sets, and measures the one thing the plan could not: how
  far outside the fan the edit reaches. **The edit list is determined at
  exactly ONE site in the corpus — R0044 v47, the face-627 site** — and the
  other six decline structurally (3 `AlreadyCarried`, each carrying the plan's
  own measured overlap, and 3 `ChainAbsent`).
  **BINDING: R0003 yields NO edits at any of the five sites where §3x gave it
  a determined plan**, so
  the SUPPORTED_CORRECT case is untouchable by this repair as scoped — a
  structural safety property established BEFORE the mutation exists, and the
  form §3t's "separate the populations by fixing both, never by a magnitude
  band" takes at the emission layer. **The reach outside the fan is exactly one
  triangle and it belongs to the neighbour**: the crease chord `(981, 6911)` is
  carried by `13112` (in fan, input face `(B, 168)`) and `13037` (outside,
  `(B, 167)`), 7 triangles touched in total — so refining a crease chord is not
  a fan-local act, the 3b-11 one-sided-insert lesson one layer down in the
  working mesh, measured in advance rather than discovered as a fold. The
  attribution reading also names the notch's destination directly rather than
  assuming it. Fourth: **the wholesale relabel and the chord split are the
  SAME triangle** (`relabel = [13112]` ⊂ `crease_tris`), so the two edits
  cannot be sequenced — `13112` must be re-triangulated and its children
  attributed in one act, and the natural implementation order is the broken
  one. Fifth: **the insert order follows the CHORD, not the solver's q
  numbering** — at v47 `q2` (t = 0.42396) precedes `q1` (t = 0.42805), so
  ordering by q index would invert the notch; `inserts` is sorted along the
  chord and `chain_tris` permuted with it. The site is DERIVED from the fan
  rather than passed (§3w's lesson). 5 unit tests, one adding the
  neighbour's triangle so the fixture carries the corpus's two-sided reach, one
  pinning the chord-order fix by naming the same chord from both ends.
- **inc-2c-3b-12b-5** (2026-09-02): the EMISSION REGION landed
  (`transit_emission_region`, pure, same gates; §3z). First: **the region
  is a topological DISK with the site on its boundary** — R0044 v47's six
  host-carrier triangles `[41, 279, 13037, 13111, 13112, 13113]` bounded by
  one cycle, the octagon `44 → 280 → 981 → 994 → 6911 → 47 → 6945 → 45`,
  both mints strictly interior; the one fan triangle carrying no host edge
  (`13110`) is what leaves the site ON the boundary. So the mutation is
  well-posed as a polygon fill. Second, and BINDING: **the edit list cannot
  be applied edge by edge, and the reason is structural.** Each mint has two
  host edges, and the chord's own-patch carrier is `13112` — the triangle
  apexed at the SITE, which §3y already named as the wholesale relabel — so
  the chord split fans an edge from the site to each mint that the chain
  split has already created. Both `(site, mint)` edges come out at FOUR
  incidences (`(47, 16355)`, `(47, 16356)`, plus `(981, 16355)`): the mesh
  is non-manifold along the very edges the repair exists to make conformal.
  Reproduced in all four fixture configurations, because the premise is the
  anatomy and not an arrangement. Third: **the coincident FIN is
  order-dependent** — a mint's chain edge and its nearest chord end can be
  paired by the fan's cyclic order or crossed by it; paired, both splits emit
  the same triangle in opposite windings (v47 is paired: `[47, 981, 16355]`).
  Measured in BOTH fixture orientations, so the order-dependent symptom is not
  mistaken for the general statement. The 3b-11 one-sided-insert lesson a
  third time and its sharpest form: an edit list stated per-edge is not an
  edit list applied per-edge. `TriangleInBothRoles` / `NotADisk` /
  `BoundaryPinched` typed, none exhibited. 5 unit tests; corpus
  273C/0W/34E/1EE/0T with `results.json` byte-identical; rewrite tier green
  (173 binaries, 2783 tests, 1228s).
- **inc-2c-3b-12b-6** (2026-09-02, same session): the FACE PARTITION and
  the boundary PINCH landed (`transit_emission_parts`,
  `transit_boundary_pinch`, pure, same gates; §3aa). First: **the region is
  not the fill's unit** — it spans both operands, and its own-patch part
  `[13111, 13112, 13113]` is in TWO edge-connected components, rejoined by
  exactly `[13110]`: the same triangle §3z found leaving the site on the
  region's boundary, because it is the one fan triangle carrying no host
  edge. Closed, all four parts are disks. Second, and BINDING: **the
  doubling §3z refuted on the EDGES is, on the BOUNDARY, the corner
  itself.** The own patch carries all three hosts, so its boundary reads
  `[45, 16356, 47, 16355, 280, 981, 16355, 16356, 6911, 6945]` and pinches
  at both repeats into `[16355, 280, 981]`, **`[16356, 47, 16355]` (THE
  NOTCH — the site and both mints, one triangle)** and
  `[45, 16356, 6911, 6945]`. §3x's 0.0356° corner against the fan's 8.70°
  footprint, expressed as a decomposition instead of a 245× ratio. No other
  part pinches: `(A, 2)` and `(A, 3)` become quads, `(B, 167)` a pentagon,
  each receiving exactly the mints on its own boundary — §3y's conformality
  by construction rather than by a second pass. So the measurement that
  killed the per-edge form supplies the correct one. Third: **the clean cut
  has a precondition** — the mints' repeat spans must not INTERLEAVE, or the
  site's loop swells to most of the patch; v47 does not interleave, the
  fixture's other chord naming does, and there the pinch declines to name a
  notch. Orientation comes free: every loop edge is a directed consecutive
  step of the inserted boundary, pinned by test. 2 unit tests, each run in
  both fixture orientations; corpus 273C/0W/34E/1EE/0T with `results.json`
  byte-identical; rewrite tier green (173 binaries, 2785 tests, 1241s).
- **inc-2c-3b-12b-7** (2026-09-02, same session): the EMISSION FILL
  LANDED, pure and census-only (§3ab). `transit_emission_fill` composes
  §3aa's plan into chart fills — structure checked on every part before
  any projection, each loop filled in its OWN face's chart with no Steiner
  points, wound by the LOOP rather than by an area heuristic, the notch
  attributed to the crease chord's far-side owner and that face's surface
  checked against the transit's `s_nbr` — and certifies the RESULT against
  the whole mesh. BINDING: at R0044 v47 the fill is CERTIFIED — 7 triangles
  out, 11 in, `touched_delta = []`, ZERO edge-incidence defects, 7 opposed /
  0 folded, `notch_surface_agrees = Some(true)` bit-identical, and both
  cones get FINER in Yang's own `d(T)` (`(B, 168)` 140.57 → 75.53,
  `(B, 167)` 22.22 → 14.12; planes 0; `(A, 2)` equal to the last digit
  because the worst control point sits on a shared edge). The polygons are
  exactly §3aa's prediction, triangulated. The fixture had to become
  GEOMETRICALLY faithful (a chain end placed by crease angle made the
  plane_x quad a bow-tie — the CDT refused it; verified arithmetically
  before the fix), and a "four triangles per mint" assertion was refuted
  by fixture and corpus alike (the count is the CDT's diagonal choice; the
  invariant is the four required edges, each carried twice). 7 unit tests.
- **inc-2c-3b-12b-8** (2026-09-02, same session): the gated WRITE arm +
  the LIFT certificate LANDED (§3ac). `transit_emission_write` is
  slot-stable and refuses on any unclean certificate;
  `YANG_451_TRANSIT_EMIT` arms it inside the §3v–§3ab chain, default OFF.
  The lift certificate (per-face along/against the surface gradient, the
  fold a directed-edge check cannot see) REFUSES the write at R0044 v47:
  `(B, 167)` receives 1 / 3, the notch at cos −1.0000 while all 69
  survivors of the face lift along. BINDING: `(B, 167)` is a conical BAND
  1.194 wide between two creases; `J` is 0.932 in, 0.2457 BEYOND the host
  carrier `13037`'s far edge, inside `13038` — §3z's region is incomplete
  on the neighbour side and §3aa's notch-as-triangle double-covers cone
  626. The corner is a BITE: the notch loop is A's wedge (dropped), and the
  neighbour polygon detours `6911 → 16356 → J → 16355 → 981` over
  `13037 ∪ 13038`. And the band's crease chords (558.5, sagitta 10.608)
  sag 4.773 station units in-surface — 4× the band width — so no fill of
  the band lifts faithfully at this resolution: Yang §4.5.2 refinement, ~10
  segments per chord on both creases, conforming across three faces.
  R0003 untouched (no apply activity). 2 unit tests.
- **inc-2c-3b-12b-9** (2026-09-02, same session): the BITE LANDED, pure
  (§3ad). `transit_bite_region` (corner-containing + stub-crossed survivors
  of the neighbour face, seam-safe per-triangle unwrap) and the fill's
  correction: neighbour part enlarged, boundary detoured through the
  corner, the own patch's notch loop DROPPED, survivors as the lift's
  reference. R0044 v47: `contains_corner = [13038]`, `touched_delta =
  [13038]`, 5-triangle fan from the corner, `edge_defects = []`, opposed 8,
  `(B, 167)` d(T) 22.2 → 12.6, lift_flips **3 → 1**. BINDING: the one flip
  is `[994, 6877, 47]` — the far-crease chord (558.8, R 3683, sagitta
  10.61) with the corner 0.26 from that crease — §3ac Reading 5 reduced to
  a single named chord; §4.5.2 refinement of it is 3b-12b-10. The fixture's
  survivors were themselves folded (moved across their edges) and its
  apex was on the axis (moved onto the cone); a band fixture reproduces
  the corpus shape and certifies clean. 44 tests; R0003 untouched.
- **inc-2c-3b-12b-10** (2026-09-02, same session): §4.5.2 REFINEMENT
  LANDED, gated (§3ae). Mints generalized to a hosted LIST (`HostMints`,
  `ExtraMint`; every carrier of a host on both faces joins the fill);
  `transit_emission_refine` = plan → act on each flipped triangle → plan
  again, capped loudly. Halving alone DIVERGED at R0044 v47 (129 halvings,
  flips 1 → 128): a band's fan from one far vertex over refined sub-chords
  folds at every chord length — the APEX's crease was coarse. BINDING: the
  MATCHED-split rule (apex on the face's other crease ⇒ split that crease
  at the base midpoint's azimuth) CONVERGES in 7 rounds, cascading through
  bands 166 → 162 (widths 9.05 … 24.74), 11 polygons / 37 triangles on 10
  faces, zero flips, zero incidence defects, every d(T) no coarser
  ((B,167) 22.2 → 7.8). With the gate armed the write LANDED — the FIRST
  emission at the corpus site (9 minted, 19 out, 37 in) — and the run
  proceeds to the standing v8 wall; R0003 untouched. A first census
  panicked in the PRINTER (two-mint assumption), reported as TIMEOUT —
  fixed. 4 new tests (narrow band halving; the cascade fixture's matched
  splits; the cap).
- **inc-2c-3b-12b-11** (2026-09-03): the CORPUS under the gate set, the
  union CERTIFIED, the TOPOLOGY reference (§3af; instruments only, no
  kernel change). Four-knob set + emission: R0044's union COMPLETES and the
  case moves to `UNSUPPORTED(curved-profile)` (the cut's partial-patch
  Stage-1 re-entry); minimal converting set `TRANSIT + SPAIR + emission`
  (torus chart and arc-conform no longer needed at this site). Full corpus
  gated 275C/1W/30E/1EE/0T (+5 UNSUPPORTED) vs canonical 273C/0W/34E/1EE/0T
  (+4): R0011 (`TRANSIT`), R0074 (`TORUS_CHART`), R0044, and R0053 E→W by
  `SPAIR` alone (single-knob attributed). Union-only probe
  (`s451_r0044_union_probe`): every runner oracle passes, composition Agree
  rel 4.66e−5, Euler 0 with 1 shell. New independent topology oracle
  (`assay::topology_oracle`, streaming cubical χ, phase control): exact on
  synthetic shapes, UNSTABLE on both grazing unions (χ swings −20…+26 across
  phase/resolution); the Cherchi sidecar reference (chained pairwise;
  `cherchi_sidecar::surface_topology`) reads R0044's union genus 1 = the
  kernel's, and R0053's genus 15 vs the kernel's 1 under `SPAIR` ⇒ §3s's
  flip condition adjudicated as a genuine M8 silent-wrong; `SPAIR` stays
  gated pending M8 Stage-0 at R0053, and R0044's conversion queues behind
  it. 16 new unit tests; clippy clean.
- **inc-3a** (2026-09-03, same session): `YANG_451_TRANSIT` and
  `YANG_441_TORUS_CHART` FLIPPED ALWAYS-ON (§3ag; household pattern,
  `0|off` = dev A/B). Two-proof corpus: off byte-identical to the committed
  canonical 273C/0W/34E/1EE/0T; default 275C/0W/32E/1EE/0T byte-identical
  to the pre-flip two-knob proof — R0011 and R0074 CORRECT, R0044's ERROR
  detail moves v8 → `FaceId(459)`, zero other moves. yang-rs 1276/0;
  `results.json` moved; WASM rebuilt. **New canonical 275C/0W/32E/1EE/0T.**
- **inc-3b** (2026-09-03, later session; §3ah): R0053 ADJUDICATED by EXACT
  analytic membership — no mesh anywhere. Closed-form predicates for the
  three authored operands (`tests/s453_r0053_exact_topology.rs`); the set
  union's cubical χ = 0 with one component at h = 2 / 1 / 0.7 / 0.5 / 0.4
  and two lattice phases (94.7 M cubes at the finest) ⇒ **genus 1**, the
  kernel's reading under `SPAIR`. The ring meets only three teeth
  (247.5°–292.5°) and fills their grooves from the root side — there were
  never fourteen handles. Face classification against the predicates: the
  sidecar's "genus-15" union has 606 faces strictly INSIDE the true solid
  (coplanar membranes at the shared plane, sliver strips at the ring-end
  / flank crossings) woven into a closed manifold — not the union's
  boundary; the kernel's output has 38, all root-arc chord-band residue
  at `w = 0`. §3af's second-branch verdict and its "lost at Stage 0–2"
  addendum RETRACTED; the meta's `euler_target` corrected 2 → 0 and
  pinned (`historical_authoring_fixes_pinned`). **`YANG_453_SPAIR`
  FLIPPED ALWAYS-ON** (household pattern). Two-proof corpus: pre-flip
  `SPAIR=1` 276C/0W/31E/1EE/0T (R0053 E→C, R0044's detail
  `FaceId(459)` → `FaceId(626)`, nothing else); post-flip default
  BYTE-IDENTICAL to it; post-flip `SPAIR=0` 275C/0W/32E/1EE/0T
  BYTE-IDENTICAL to the committed canonical. yang-rs suite green; clippy
  clean; `results.json` moved; WASM rebuilt. **New canonical
  276C/0W/31E/1EE/0T.**
- Also open: the retry-path v105 refill (ChordDegradation under
  centroid seeding — an edge-split question; NO LONGER moot: the
  §4.5.4 refine retry fires whenever the natural output is broken,
  and its corner fire keeps the retry walled), and the incursion
  strips' remaining two-sided cases beyond the 3b-8 contained-strip
  closure (subsumed into 3b-9's conformality work).
- inc-3: fixed-point integration (multiple sites per case; R0085 has two
  failing ops) + full-corpus gated measurement; flip under the standing
  two-proof protocol. R0085 stays honestly walled on operand quality
  (op-2 input-n2m).
