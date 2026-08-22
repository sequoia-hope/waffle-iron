# §4.5.1 "Optimize across boundaries" — design spec (increment 1 of the §4.5 build)

**Status: DESIGN v2 (2026-08-22).** inc-0's probe landed and its measurement
REVISED the design the same day — see §7. The headline: **repair-at-refusal is
refuted for the pin case** (the far bound is structurally invisible at the
refusal vantage because the sweep has not reached it yet), so the
record-and-continue conversion that v1 deferred to "§4.5 build step 2" is the
FRAME of increment 1. §§3–4 below are v1's design; §7 records the measured
revision and supersedes §3.1's interception model. Follows §4-I12
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
- **R0003's two regions** (separate stage-4 invocations of the case's one
  boolean — the second invocation's trigger is NOT yet attributed (an earlier
  "composition oracle re-run" reading was an unverified inference) — and
  genuinely different cones):
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

## 7. inc-0 MEASURED (2026-08-22, same session) — Q1 answered, and the design revised

The probe (`offcurve_beyond_chord_band`, `#[track_caller]`, all **14** gate
sites — the v1 spec said ten; the audit grep was head-truncated — plus the
own-curve chain walk under `YANG_451=census`) ran on R0003/R0028/C0065:

| vertex | firing arm (site) | own-curve chains |
|---|---|---|
| R0003 v4233 | cone-ELLIPSE residual (`:6872`) | via v4167: ends at 1 hop (v4167 = chain end, converged). Via v4234: **≥32 further ellipse vertices, walk capped**, all ids > 4233 |
| R0003 v10583 | cone-HYPERBOLA residual (`:6969`) | both directions end at 1 hop (v10564 / v10585 = the segment's endpoints, both converged) |
| C0065 v3, v8 | owner-face hull check (`:7546`) | (see log) |
| R0028 v64 | `:7440` | (see log) |

### Q1 — ANSWERED: own-curve pairing works, and the I12 walk over-counted

Only TWO of v4233's four curve-graph branches carry a Phase-A curve
assignment, and both carry the SAME `Ellipse` (bit-identical params): v4167
and v4234. The I12 all-branch walk's three "bounds" (v4167/v4169/v4183) were
partly cross-curve artifacts of the patch-span adjacency — under the own-curve
reading the correct picture is ONE chain with one near bound (v4167) and a
long unadjudicated far side. v10583's own-curve bounds coincide with the I12
walk's — it stays confirmed unchanged.

### The revision: the refusal vantage cannot see a bound the sweep has not reached

Across every measured vertex the convergence pattern matches SWEEP ORDER
(BTreeMap per-classification loops, ascending id): v4167/v4169/v4183 (< 4233)
converged; v4234…v4268 (> 4233) not converged — not because the region is
30+ vertices of true failure, but because the sweep aborts at the FIRST gate
fire and never processes them. (v10585 > 10583 converged is the benign
exception: a chain-end vertex already within the certificate band at its
minted position.) **A repair at the refusal site therefore cannot evaluate
clause 2 for any region whose far bound sweeps later — the selector is not
well-posed mid-sweep.** This is WHY the paper collects failures after
optimization (§4.5 `:652-656`) and repairs region-by-region.

### Consequence — increment order revised (supersedes §3.1's interception and §4-I12 (d)'s order)

**Increment 1 = record-and-continue with post-sweep repair**, structured so
every existing pass's precondition is preserved:

1. Each of the 14 gates, via the shared choke point: gate OFF → return the
   error exactly as today (byte-identical). Census/on → RECORD
   `(v, site, error)` and SKIP the vertex's relocation (per-site skip
   semantics audited individually — the skip must target the per-vertex
   loop).
2. The sweep's classification loops complete: every non-failed vertex
   relocates.
3. Post-sweep, pre-everything-else: for each recorded failure, the selector
   (clause 1 + own-curve clause-2 walk, `converged` now at the paper's
   vantage) → §3.3's repair where both clauses hold.
4. Any failure unrepaired → return the FIRST recorded error (bit-identical
   category+detail to today — P10: the run still cannot complete with
   unrepaired failures). All repaired → the stage proceeds (fold validation,
   §4.4.1 passes, §4.3.4, §4-I9 postcondition all see a fully-relocated
   mesh, their precondition unchanged).

Census mode = steps 1–3 with the repair replaced by verdict prints, then
step 4's error unconditionally: corpus-neutral, and it re-adjudicates
C0065/R0028 (and v4233's far side) at the paper's vantage — the measurement
§4-I12 (c) wanted.

Open sub-questions folded in: Q2 (is v4233's region k>1 at the paper's
vantage?) and the per-site skip-semantics audit are answered by the census
increment before any repair code runs.

## 8. inc-1 census LANDED and MEASURED (2026-08-22, same session) — the paper-vantage population, and Q2 flipped

The record-and-continue census (§7's structure) is implemented behind
`YANG_451=census`: every OffCurve gate routes through `s451_stop`
(record-and-skip under census; byte-identical abort otherwise; the labelled
`continue 'torus_verts` covers the hull-check site whose plain `continue`
would have fallen through to the relocation write), the conic no-skip audit
subtracts recorded failures from its expectation (a recorded failure is the
paper's collected "cannot converge" state, not a silent skip — audit
byte-identical when the set is empty), and `s451_post_sweep_census` reports
clause 1, I12's all-curve clause-2 walk, and the own-curve region per
failure, then returns the FIRST recorded error unchanged. Default-path
neutrality verified: yang-rs 1110/0 twice, R0074 + R0003 gate-off details
byte-identical to canonical.

### The measurement (paper's vantage, three cases)

- **R0003 — unanimous §4.5.1, and the region is BIGGER than the frozen
  vantage showed.** ~45 failures per invocation (37 cone-ellipse + 8
  cone-hyperbola sites), 100 % INTERIOR, 100 % bounded both ways on their
  own conic: contiguous regions of len 1–12 (v4233's region =
  {v4233..v4240}, 8 vertices, bounds v4167/v4241 both converged —
  Fig-12(b) drawn). The final propagated error stays v4233
  OffCurveBeyondChordBand, byte-identical.
- **Q2 ANSWERED — k>1 regions exist (len up to ~12), so the repair's
  region-collapse step (§3.3.1) is REQUIRED, not optional.** The k=1
  shortcut is refuted.
- **R0028 v64 and C0065's interior failures are §4.5.1 candidates at the
  paper's vantage after all** — I12's frozen non-confirmations FLIP:
  `allcurve_clause2=true` (converged bounds on common surfaces exist once
  the sweep completes). Their `owncurve_bounds=0/0` is an INSTRUMENT
  LIMITATION, not a verdict: both fire in the TORUS block (rho gate /
  hull check), whose vertices relocate by implicit surface-pair Newton —
  their curve is no conic, so `curves0` carries no edges for them and the
  own-curve reading is structurally empty. The combined verdict line
  prints §4.5.2 for them only because of that conservative
  operationalization.
- **C0065's population splits**: 4 interior (v3, v8, v231, v399 — hull
  check) = §4.5.1 candidates; 2 boundary gliders (v362, v397, carrier
  `(A0,B2)`, torus rho gate) = Fig-13-excluded, genuinely §4.5.2.
- **Continuing past failures unmasks more of the family**: C0065 2→6
  failures, R0003 2→~90 across invocations. I12's STOP-vertex table was
  the first-fire tip of each run's iceberg, as predicted by inc-0's
  "masked behind earlier STOPs" note.

### Consequences for the build

1. The repair increment's first customer stays **R0003** (conic-assigned,
   fully measured, unanimous). Its regions are k ≤ ~12 → §3.3 step 1 uses
   the §4.4.1 collapse machinery from the start.
2. **Torus/surface-pair failures need their own region identity** (the
   surface PAIR, not a `Curve` value) before they can be repaired — a
   separate sub-increment after the conic one; until then their STOP
   stands (they are C0065/R0028's owners).
3. The corpus-wide census (every case, paper-vantage family enumeration +
   proof that the first-error propagation is byte-identical corpus-wide)
   is the next measurement. NOTE: the parallel assay runner nulls child
   stderr, so enumeration needs the per-case single_case loop (the I11
   pattern), while the plain corpus run verifies neutrality only.

## 9. inc-2a — the repair-variant PREVIEW (census columns; written 2026-08-22, awaiting the corpus run to build)

§8 left the repair increment with one unmeasured fork: for a bounded region,
does the paper's midpoint re-optimization stay on ONE surface pair, or must
it cross a patch boundary? The variants differ by an order of magnitude of
machinery:

- **DRIFT region** (both bounds carry the SAME far-operand surface): the true
  curve segment lies on the region's own pair; repair = collapse region to
  one survivor → move to `midpoint(v0, v1)` → the arm's own closed-form
  projection (`project_onto_cone_section` for cone+plane, callable from the
  bare `Surface` values — no map plumbing) → shared certificate. No boundary
  crossing, no q1/q2.
- **STRADDLE region** (bounds carry DIFFERENT far-operand surfaces, e.g. two
  adjacent gear-facet cones): Fig-12's full mechanism — truncation at the
  patch boundary, continuation on the neighbour pair, q1/q2 minted as triple
  points (`relocate_onto_implicit_triple` exists), §4.3.4 re-densifies.

The preview computes, per region (deduped by bound pair), with no repair
code: the bounds' `carrier_surface_sets` (split out of `carrier_counts`),
the shared far-operand surface count (`kind=DRIFT|STRADDLE`), and — when the
shared pair is cone+plane — the midpoint's projection and its certificate
verdict on both surfaces (`YANG_451_PREVIEW … simple_projection …
certificate=true|false`). A `certificate=true` DRIFT region is repairable by
the simple variant outright; the counts over R0003's ~14 regions decide
what inc-2b builds first.

Known preview limits, stated: `ca==0` picks the far operand (hull-check
failures within band on both operands default to B — both sets print);
non-cone+plane pairs get no closed-form preview (R0003 is all cone+plane;
the torus-carried candidates are out of the conic instrument's scope
regardless, per §8).

## 10. inc-2b — the DRIFT repair LANDED GATED (`YANG_451=1`) and measured on the pin (2026-08-22, same session)

**Status: BUILT, GATED, RED-VERIFIED, MEASURED. Default path untouched
(yang-rs 1110/0; targeted gate-off re-runs byte-identical; rewrite tier).**

`s451_plan_repairs` plans every region READ-ONLY (walks never see a
half-collapsed mesh; bounds may be shared between adjacent regions — they are
converged, never victims), then the hook applies: §4-I8-checked collapses of
victims onto the smallest-id survivor, survivor moved to the projected
midpoint, retag param pushed to `relocations`, `collapsed_any` seeded so
§4.5.3's Phase-A recompute covers the mutations. ALL recorded failures must
belong to a planned region and every plan condition must hold, or the FIRST
recorded error returns unchanged (P10 — no partial acceptance). Conditions:
clause 1 interior; exactly one own-conic; two distinct converged bounds;
shared cone+plane pair; midpoint projection within the shared certificate
band on BOTH surfaces; `|proj − mid| ≤ |bounds|` (scale sanity — a far-side
conic landing becomes a loud decline, never an acceptance); Ellipse/Hyperbola
retag computable.

**Red-verified by two mutations**, each returning the ORIGINAL v4233 error
bit-identically: (1) scale gate forced (`chord·0`) → DECLINE (and the real
margins measured healthy: `|proj−mid|` = 1.47e-1 vs chord 8.5, 1.75e-4 vs
1.7); (2) certificate forced impossible (`band·0`) → DECLINE on the cone.

**The pin measurement:** R0003 under `YANG_451=1` repairs **11/11 regions**
across both invocations (k = 8, 17, 12 and eight k=1 — the plan's full-region
k exceeds the census's capped per-direction prints), **the Stage-4 wall is
CLEARED**, and the case advances to a NEW downstream wall:
`TessellationFailed FaceId(435)` — the KV9-F2 developable unroll fold.
**Masked latent, not minted (direct hypothesis refuted):** the folded face's
cone has `tan = 2.396066`; the repaired regions' cones are
{1.4016, 1.8510, 2.6977, 3.2322, 4.0759, 4.8549} — none matches. The fold is
the developable-ring family's pre-existing wall (R0028/R0049's ledger class),
reachable for the first time now that Stage 4 completes. (Indirect
topological effects of the collapses on neighbouring faces are not fully
excluded; the direct sparse-arc-folds-its-own-face mechanism is.)

R0028 and C0065 under the gate DECLINE cleanly (`own-curve count 0` — the
torus-carried instrument limit, §8) and keep their exact original errors.

**Deliberately deferred, with reasoning recorded:** the paper ends §4.5.1
with q1/q2 + §4.3.4 refinement. No measured region crossed a boundary (all
DRIFT), so q1/q2 has no customer. The refine-after-repair density debt is
real but did not cause this fold (render tessellation samples the analytic
edges, and the fold is not on a repaired arc); it will be judged by the
witness-volume oracles the day R0003 completes, and the paper's step stays on
the books as the follow-on for that day.

**Flip bar (unchanged posture):** the gate stays OFF until the corpus-wide
default run is byte-identical and a gated corpus run's deltas are each
explained. Next measurements: (a) corpus default-neutrality proof; (b) gated
corpus run to enumerate what else repairs (any conic-carried OffCurve case
masked behind first-fire aborts).

## 11. FLIPPED ALWAYS-ON (2026-08-22, same session) — the flip bar was met with one explained delta

The corpus measurements that justified the flip, in order:

1. **Default-mode corpus (pre-flip commit stack):** 265C/0W/43E/1EE/0T in
   544s, and the run's freshly-written `results.json` left the git tree
   CLEAN — bit-identical to the committed canonical baseline, details
   included. All five of the session's commits were corpus-neutral.
2. **Gated corpus (`YANG_451=1`):** category-identical (265C/0W/43E/1EE/0T,
   535s), and the ERROR-detail diff against the default run shows **exactly
   one changed row: R0003** (Stage-4 OffCurve v4233 → the KV9-F2 fold,
   FaceId 435). Every other case — every torus-carried OffCurve fire, every
   other family — keeps its exact original error. R0003 is the corpus's
   only conic-carried §4.5.1 customer today, and the repair declines
   everywhere else.

Flip semantics: default = repair; `YANG_451=0|off` restores the historical
abort-at-first-fire; `census` still measures. Verified post-flip: default
R0003 repairs 11/11 and reports the fold; `=0` reproduces the old OffCurve
error byte-identically; yang-rs 1110/0.

The canonical baseline (`results.json`) is updated in the flip commit with
R0003's new detail — the recorded wall now names the true next owner (the
developable-ring fold family) instead of the repaired one. Canonical score
stays **265C/0W/43E/1EE/0T**.

## 12. inc-3 preview — the pair-Newton half for the TORUS-CARRIED candidates; C0065 measured OUT of §4.5.1 (2026-08-22, same session)

`selector_clause2_walk` now returns its bounds + common surfaces (prints
unchanged — R0074 census parity byte-identical), and the post-sweep census
previews own-curve-less failures via `relocate_onto_implicit_pair` from the
bounds' midpoint (pair = the two common surfaces, or the single common one
plus a traveller carrier surface). Certificate + region-scale sanity only —
the owner-face HULL half of acceptance is deliberately not previewed.

Measured:

- **R0028 v64**: pair-Newton certificates at ~1e-18 with scale_ok — a
  genuine repair candidate pending the face-domain verdict.
- **C0065 v8/v231/v399**: certificate=true, scale_ok=true — and the
  projections land at `x = 1.450000000` exactly, `|y| ∈ [0.348, 0.374]`,
  against the case's long-recorded anchor: the wall face at x=1.45 spans
  `|y| ≤ 0.25`. The pair-Newton answer is ON both surfaces and OUTSIDE the
  bounded face — the midpoint re-initialization does NOT rescue C0065; its
  true curve segment between the bounds runs outside the face, which is the
  #137 grazing-loop / corner-insert diagnosis its ledger row has carried
  since N-137.1. **C0065 is measured OUT of the §4.5.1 repair** (the hull
  acceptance would refuse), not merely deferred. v3 (degree-4 junction, 4
  all-curve bounds) and the boundary gliders decline the 2-bound preview.

Next increment, precisely scoped: extract the torus block's planar-hull
check for reuse → preview the hull verdict on the pair-Newton projections →
build the torus-region repair only where BOTH halves pass (expected
customer: R0028; expected decline: C0065 → #137).

## 13. inc-3b — the TORUS-REGION arm LANDED (2026-08-22, same session): R0028 clears Stage 4; C0065 refused by its own hull, as measured

The plan gained an own-curve-less arm: when the failure's pair traces no
`Curve` conic, region and bounds come from the intersection-curve graph
(`selector_clause2_walk`'s bounds — k = 1 only, both bounds must be DIRECT
curve neighbours; no measured k>1 torus customer) and the re-optimization is
`relocate_onto_implicit_pair` from the bounds' midpoint. Acceptance = the
SAME three-part reading the torus gate uses: shared certificate on both pair
surfaces + region scale + the owner-face hull —
`planar_partner_hull_contains`, EXTRACTED from the torus block's inline
check (pure refactor; C0065 + R0015 off-mode details byte-identical through
the extracted fn). Torus repairs carry `retag: None` (the torus arm records
no `t`; bookkeeping is `moved` only, mirroring the block).

Measured under the flipped default:

- **R0028**: v64 repairs (k=1), **Stage 4 CLEARS**, and the case advances to
  `VertexOffSurface { FaceId(32) }` — FaceId 32 is R0028's OWN long-recorded
  developable-ring face (ledger: "ring rejected by CDT (FaceId 32),
  developable patch, closure fold"). The masked latent is the case's known
  next wall, surfacing one strict-validation layer above the recorded CDT
  reject.
- **C0065**: v8's repair is REFUSED by the hull on the wall plane
  `Plane { normal: [1,0,0], d: -1.45 }` — §12's prediction, now enforced by
  the repair's own acceptance — and the case keeps its byte-identical
  original error. v3 declines separately (4 all-curve bounds — the degree-4
  junction). **C0065 → #137 is now triply anchored**: the ledger diagnosis,
  the §12 preview, and the live repair's decline.

Corpus proof (same day): **265C/0W/43E/1EE/0T with exactly ONE baseline
delta — R0028's detail row** (Stage-4 OffCurve v64 → VertexOffSurface
FaceId 32); every other case byte-identical, C0065 included. The new wall's
FACE matches R0028's recorded developable-ring defect; the gate differs
(strict-validation VertexOffSurface vs the recorded CDT ring reject), so the
family match is anchored by the face id and the vertex-level anchor (which
vertex, off which surface) is the case's next investigation — recorded, not
assumed. Baseline updated in the inc-3b commit.
