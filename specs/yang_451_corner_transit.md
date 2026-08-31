# §4.5.1 Corner Transit — the §4-I9 corner-crosser repair (I13f rehome kin)

**Status (2026-08-31b): FIRST CONVERSION — R0011 is SUPPORTED_CORRECT
under the gate.** inc-2a/2b/2c-0/2c-1 (2026-08-30): planner + walk
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
refine RETRY. NEXT = R0044's curve-aware host search (the arc-host
wall, §3i), R0074/R0085 family measurement, then inc-3 (full-corpus
gated measurement, two-proof flip).** Epic opened by
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
- inc-3: fixed-point integration (multiple sites per case; R0085 has two
  failing ops) + full-corpus gated measurement; flip under the standing
  two-proof protocol.
