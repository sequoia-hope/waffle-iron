# §4.5.1 Corner Transit — the §4-I9 corner-crosser repair (I13f rehome kin)

**Status: inc-0 (feasibility census) COMPLETE, 2026-08-29 — feasibility
CONFIRMED family-wide (46/46 triples converge at 23/23 sites); NEXT =
inc-1 (continuing-edge discriminator + corner-local view dedup).** Epic opened by
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
2. Discriminate the continuing edge at q (inc-1): the true transit target
   lies on the far∩base curve past q — accept {far, base, facet_j} iff the
   solution lies inside facet_j's edge/face domain (edge-segment parameter
   test against the model edge base∩facet_j, not a hull heuristic);
   typed decline otherwise (AmbiguousTransit — the honest gated fixed
   point, I13f-f2b precedent).
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

## 4. Increment ledger

- **inc-0** (2026-08-29): feasibility census LANDED + COMPLETE (this file
  §2); 23 sites / 46 candidate triples across R0011+R0074+R0044+R0085, 100%
  convergence, planar-hull in-domain wherever the verdict applies. Mirrored
  pairs, crease-rider chains, and the v8 anomaly recorded for inc-1.
- inc-1: continuing-edge discriminator census (edge-segment domain test at
  q; cone-band in-domain reading for R0044/R0085's facets).
- inc-2: gated apply arm (`YANG_451_TRANSIT` or an extension of
  `YANG_441_REHOME`'s household pattern) — truncate-share-relocate-reroute,
  audit-clean per I13f discipline.
- inc-3: fixed-point integration (multiple sites per case; R0085 has two
  failing ops) + full-corpus gated measurement; flip under the standing
  two-proof protocol.
