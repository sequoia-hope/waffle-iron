# §4.5.1 Corner Transit — the §4-I9 corner-crosser repair (I13f rehome kin)

**Status: inc-2a PLANNER LANDED + VALIDATED family-wide, 2026-08-30 —
`stage4_transit.rs` (pure recognize/discriminate/classify, typed declines)
now owns the inc-1 instrument; 23/23 site verdicts reproduce (and on R0085
refine) the inc-1 classification: 12 TRANSIT + 5 CLIP + 6 typed declines;
shared mints auto-detected by position identity. The anatomy census REFUTED
the §3 "truncate at q" apply sketch: the repair unit is the CORRIDOR (§3d).
NEXT = inc-2b (corridor-walk census), then inc-2c (the gated apply arm).** Epic opened by
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
     candidate list in general. v27: the base-chain end is cand-0's
     junction mid base∩B213 edge (in-domain per the 1.458 chord-sag
     read), one facet over — corridor length 1. v42: cand-0 solves a
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

## 3e. inc-2b — the corridor-walk census (NEXT; report-only)

Per planned site, walk the far∩facet curve across the fan from the real
corner-side junction: build the per-operand POSITION-KEYED edge-copy
adjacency (m1: LineSegment copies pair by endpoint positions, curved
edges by index), then per facet solve the candidate exits {far, facet,
partner} over the facet's loop edges and certificate in-domain via the
shared instrument. Terminate at: an EXISTING healthy mesh junction
(identity: carried triple + position within band — e.g. v80's chain-end),
another fired site's junction (corridor MERGE — v42+v78), the other
chain's mid-lattice exit (v27), or a typed decline. Report per corridor:
facet list, junction list with certificates, termination kind, and which
existing mesh chains re-anchor where. The walk census turns §3d's
two-site inference into per-corridor DATA before the apply arm is built.

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
- inc-2b: the corridor-walk census (§3e) — corridors, junction lists,
  termination kinds, chain re-anchors, as report-only data.
- inc-2c: the gated apply arm (`YANG_451_TRANSIT`) — corridor splice per
  §3d: split phantoms, mint-once-share-by-position junctions, corridor
  runs at chord density, two-sided re-fill, §4-I9 re-check + oracle gate.
- inc-3: fixed-point integration (multiple sites per case; R0085 has two
  failing ops) + full-corpus gated measurement; flip under the standing
  two-proof protocol.
