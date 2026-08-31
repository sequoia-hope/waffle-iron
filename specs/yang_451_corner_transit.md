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
- inc-2c-3b-9b: the CONTINUATION arm — a 3-membership absorb
  candidate marks the curve continuing across further creases;
  resume the walk from the terminal junction (Fig-12(e)), mint the
  true crossings (the 371∩372 root at 97.1 etc.), terminate on the
  healthy chain. B-side creases first (N1/N3); the A-side far-face
  transition (N2: v107 crosses A2∩A3 — the far face changes) as its
  own sub-slice (§3q).
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
