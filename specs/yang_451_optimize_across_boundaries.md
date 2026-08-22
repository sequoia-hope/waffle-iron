# §4.5.1 "Optimize across boundaries" — design spec (increment 1 of the §4.5 build)

**Status: DESIGN (2026-08-22).** No code yet. Follows §4-I12
(`specs/yang_441_trim_cdt_construction.md`), which confirmed this strategy's
first customers by measurement. Pin case: **R0003** (two erroneous regions,
both `OffCurveBeyondChordBand` STOPs, both clause-confirmed §4.5.1).

## 1. What the paper says (the spec)

Yang 2025 §4.5.1 (`refs/text/yang2025_hybrid_boolean.txt:672-690`), given an
erroneous region bounded by two successfully optimized points `v0`, `v1` on
the same surface:

1. Remove the points in the erroneous region; replace with the midpoint `p0`
   of `v0` and `v1`.
2. Optimize `p0` with the §4.3.2 geometric method. Where a full step would
   leave the surface `S2` the point is initially on, TRUNCATE the step to land
   on the boundary curve `C_b` between `S2` and the neighbouring surface `S1`
   (Fig-12 (c)).
3. Next iteration continues using `S1`'s parameterization — a smooth transit
   across the boundary without reparametrization (Fig-12 (d)).
4. After `p` converges, solve the intersections `q1`, `q2` between the curve
   and `C_b` (Newton, Fig-12 (e)), then refine the curve per §4.3.4.

Selector (both clauses, per §4-I10 (f) + I12): the point must be INTERIOR
(Fig-13 excludes boundary-gliders) AND bounded by two successfully optimized
points on the same surface. *"If such bound cannot be found or after crossing
boundaries the optimization still fails"* → §4.5.2 (`:659-670`). The loop
repeats and terminates because mesh intersections converge to the true
intersections under refinement.

## 2. Measured facts the design builds on (§4-I12)

- **The STOP this repairs**: `OffCurveBeyondChordBand` fires in the Stage-4
  relocation sweep (`stage4_relocate_and_correct_inner`'s per-classification
  loops — ten gate sites, e.g. `stage4_correct.rs:6338` for the circle loop)
  when a vertex's CURRENT position is beyond the chord band `d_ε` from its
  assigned analytic curve, i.e. the discrete intersection point is too far
  off-curve to be a trustworthy initialization — the paper's §4.5 trigger
  verbatim. The gate fires BEFORE the arm writes anything.
- **R0003's two regions** (separate invocations — main run + the in-line
  composition oracle's re-run — and genuinely different cones):
  - v10583: degree-2 curve vertex, bounds `v10564`/`v10585` both converged at
    1 hop, common surfaces = {Cone, Plane}, traveller on one. The paper's
    Fig-12 picture exactly. **Erroneous region = ONE vertex.**
  - v4233: degree-4 curve junction; three branches bound at 1 hop
    (`v4167`/`v4169`/`v4183`), one refuses (further branch point); all bounds
    share {Cone, Plane}. Region again ONE vertex, but the bounds outnumber
    the paper's two — see §4 Q1.
- `i9_style_crossers_skipped = 0` — no out-of-domain converged vertices
  masquerade as bounds at these sites.
- Vantage: at the gate, earlier-swept neighbours are already relocated; the
  measured bounds are converged AT the refusal vantage. The design does not
  assume post-sweep state.

## 3. Design

### 3.1 Where the repair lives

A single choke-point helper called where the ten gates currently
`return Err(OffCurveBeyondChordBand)`:

```
try_451_or_stop(v, assigned_curve, ctx) -> Result<RepairOutcome, YangError>
```

Gated by `YANG_451` (`census` = selector + would-repair logging only, `1` =
repair on, unset/`0` = today's STOP — byte-identical). Every gate site routes
through the helper so the selector's two clauses are evaluated ONCE, the same
way, at every site (the fix-all-gates-sharing-a-metric rule).

The helper:
1. Runs clause 1 (Fig-13 interior/boundary discriminator at the vertex's
   CURRENT position — the I11/I12 `carrier_counts` reading).
2. Runs clause 2: the `selector_clause2_walk` bounding walk, restricted to the
   branches of the vertex's OWN assigned curve (§3.2), with the I12 `good`
   predicate (converged ∧ not `v` ∧ not `vertex_crossed_domain_endpoint`).
3. Both hold → repair (§3.3). Any clause fails → the existing STOP, unchanged
   (P10: §4.5.2's customers keep their loud refusal until §4.5.2 exists).

### 3.2 Region and bounds are defined along the vertex's OWN assigned curve

The sweep already knows which analytic curve `v` belongs to (its
classification-map entry — the very assignment whose residual the gate
checked). The erroneous region is the run of consecutive off-curve/failed
vertices along THAT curve's polyline, and `v0`/`v1` are the first converged
vertices past its two ends. At a curve-graph junction (v4233), branches are
paired by the edge-level conic assignment (`inc0`/`curves0`): the two
branches carrying `v`'s own conic are its chain; crossing chains belong to
their own map entries and are NOT part of this region. **This pairing is a
hypothesis inc-0 must confirm** (§5 Q1) — if edges at the junction do not
carry usable per-conic identity, the honest fallback is to repair only
degree-2 regions and keep the STOP at junctions.

### 3.3 The repair (mesh terms)

For a k-vertex region on one curve chain between bounds `v0`, `v1`:

1. **Region collapse to one survivor.** k = 1 needs NO topology change (the
   pin case). For k > 1, collapse region vertices onto the survivor using the
   existing §4.4.1 machinery (Fig-11 merge / `collapse_vertex`), honouring
   §4-I8's containment precondition per collapse. First increment may gate on
   k = 1 only — R0003 needs nothing more (§5 Q2).
2. **Reposition** the survivor to `midpoint(v0, v1)`.
3. **Re-optimize with truncated steps.** Iterate the existing relocation arm
   for the assigned surface pair from the midpoint. Each step is passed
   through `stage4_truncate::max_in_domain_step` (built + red-verified in
   §4-I10 (g)): `FullStepInDomain` → take it; `TruncateAtVertex{t, at}` → land
   EXACTLY on `at`'s stored position (the primitive's contract — never
   `lerp`), then continue the next iteration against the NEIGHBOUR face
   across that domain vertex (the paper's "using the parameterization of
   S1"): re-resolve the surface pair as (neighbour face's surface, far
   operand's surface) and step from there. Hard iteration budget (paper
   Fig-12 shows 2 legs; budget 8 with a counter print); on exhaustion or a
   step that fails to reduce the residual → restore nothing, return the
   original STOP (the paper's own "after crossing boundaries the
   optimization still fails" → second strategy).
4. **Acceptance is the SHARED certificate** — the converged position must lie
   on a surface of EACH operand at `junction_certificate_band` (the same
   `vertex_converged` reading the selector used). No new band, no widening
   (P10). Fail → original STOP.
5. **q1/q2 + refine.** If the transit crossed `C_b`, the true curve crosses
   the model edge there: mint the crossing point with the existing junction
   vocabulary (`vert_junction` / triple-point insert) and let §4.3.4's
   refine pass (seam insert / reorder) own the segment's density — do not
   hand-roll a second refine.
6. **Bookkeeping**: drop the repaired vertex's stale entries from the sweep's
   classification maps (or mark processed) so later loops neither re-gate nor
   double-relocate it; record the repair in `relocations` so the fold
   validation and §4-I9 postcondition see it. The §4-I9 postcondition runs
   unchanged at stage end — a repair that slid out of domain gets caught by
   the existing net.

### 3.4 What this increment does NOT do

- No record-and-continue sweep conversion (that is §4.5 build step 2 — it
  changes the selector's vantage to the paper's and re-adjudicates
  C0065/R0028; recorded in §4-I12 (d)).
- No §4.5.2 refinement.
- No repair where any selector clause fails, where the walk refuses
  (branches/64-hop), or where bounds do not pair along the assigned curve.

## 4. Increments (each lands separately, gated, measured)

- **inc-0 (probe, no behaviour):** at each OffCurve gate under `YANG_451=census`,
  print site (`#[track_caller]` pattern per `YANG_LRR_PROBE`), the assigned
  curve, the own-curve branch pairing at the vertex, and the selector verdict.
  DECIDES Q1 (v4233's pairing) and confirms the R0003 gate sites. Corpus
  census run to enumerate ALL would-repair sites (expected small; I12 found
  only R0003's two among STOPs, but gates that fire without being the FIRST
  STOP of a run are invisible in I12's data — this probe sees every fire,
  including ones currently masked behind earlier STOPs in other cases).
- **inc-1 (primitive):** `stage451_repair` as a pure-ish function on
  (mesh, region, bounds, assigned pair) implementing §3.3 steps 2–4, unit
  tests on synthetic cone∩plane fixtures + red-verify (mutate the truncation
  landing and the acceptance band; both must fail).
- **inc-2 (wiring):** the `try_451_or_stop` choke point behind `YANG_451=1`;
  `single_case` R0003 both invocations; then full corpus gated-on vs gate-off
  (expect: R0003 ERROR→? and byte-identical elsewhere; any other delta is an
  unmasked latent to census per the masked-vs-MINTED method).
- **inc-3 (flip):** always-on per the flip bar (byte-identical corpus or
  every delta explained), spec + ledger + roadmap updates.

## 5. Open questions (deciding measurements named)

- **Q1 (inc-0 decides):** does the edge-level conic assignment pair v4233's
  four branches into two chains, with `v`'s own conic bounding exactly two of
  the three measured bounds? If not → k=1 degree-2 only, junction STOP stands.
- **Q2:** are there k>1 regions anywhere? (inc-0's census answers; I12 saw
  only k=1.) If none, region collapse (§3.3.1) stays unbuilt — do not build
  machinery without a customer.
- **Q3:** does repairing invocation 1 of R0003 (v4233's region) expose further
  STOPs downstream in the same run? Expected and fine — each newly exposed
  site gets the same selector; count them in inc-2's measurement.
- **Q4:** cost — the walk + repair run only at would-STOP sites (today: run
  death), so budget impact is bounded by repair iterations; confirm on
  R0003's timing (~30s today).

## 6. Constraints carried from governance/memory

- P9/P10: no tolerance widening anywhere in the path; acceptance = the shared
  certificate band only; every non-confirmed configuration keeps its loud
  STOP. A repair that cannot certify restores the original error unchanged.
- The paper IS the spec: §4.5.1's steps in order, nothing invented; where our
  closed-form arms replace §4.3.2's tangent-plane iteration, the STEP comes
  from the existing arm — this spec adds only midpoint re-initialization,
  domain truncation, and the cross-boundary continuation.
- `feedback_stop_band_tuning_build_mesh_updating`: this is capability, not a
  band. `max_in_domain_step` is reused as built; no new thresholds.
