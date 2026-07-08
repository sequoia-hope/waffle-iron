# §4.5.3 junction-protected reversal collapse (yang-rs Stage 4)

Amendment to `specs/yang_pr_yr10_stage4_relocate.md` §3 step 3 (the §4.5.3
reversed-intersection sweep). Bug-fix cycle per FIP §8.

## 1. Goal

The §4.5.3 sweep must never remove an exact **curve-junction vertex** — a loop
vertex where the intersection curve CHANGES (the endpoint shared by two arcs of
two different conics, e.g. the corner where two plane∩cylinder ellipse sections
of adjacent prism faces meet). Junctions are the exact endpoints of the
intersection curves; Stage 4 relocates them in closed form onto BOTH curves
(`vert_ell_junction` / `vert_circle_junction` / `vert_junction`). Removing one
merges arcs of two different curves into a single output edge, whose single
representative `Curve` cannot contain the surviving endpoints — the kernel-v2
import then rejects loudly:

```
InvalidBooleanOutput("output ellipse-arc endpoint does not lie on its ellipse")
```

Measured on R0011 (`KV9_JUNCTION_PROBE` + `YANG_V_PROBE`, 2026-07-08): the
revolve-cylinder × gear-prism union relocates interior vertex 16 onto its
single incident ellipse E16; the projection lands past E16's junction endpoint
(vertex 28, exactly on both E16 and the adjacent section E_out), producing a
genuine §4.5.3 reversal at p_r=16 — and the sweep then collapses p_n=28 (the
junction) onto 16, cascading through all seven junctions of the loop. The
output edge (16,26) spans multiple sections; endpoint 16 sits on E16, off the
representative curve by the junction offset (out-of-plane 8.6e-2 at radius
6.5e3, band 6.5e-6).

## 2. Parameters

None (no new user-facing inputs; internal control flow of
`sweep_reversed_intersections`).

## 3. Branch table

Reversal detected at `p_r` (per `is_reversed`, which already returns healthy
when the two edges at `p_r` carry DIFFERENT curves — PR-KV11):

| # | `p_n` is a curve junction (curve(p_r,p_n) ≠ curve(p_n,p_after)) | Action |
|---|---|---|
| 1 | no  | collapse `p_n` onto `p_r` (paper default — UNCHANGED behavior) |
| 2 | yes | collapse `p_r` onto `p_n` (the junction survives; the reversed point is `p_r`, whose relocation overshot the exact end of its curve) |

No third branch: `is_reversed` returning true implies the two edges at `p_r`
carry the SAME curve, so `p_r` itself is not a junction.

## 4. Invariants

- I1: after the sweep reaches its fixed point, every vertex that was a curve
  junction of two different conics before the sweep is still present in some
  surviving triangle (junction positions are exact; they are never victims).
- I2: branch 1 inputs (no junction adjacency) produce byte-identical results to
  the pre-fix sweep.
- I3: the collapse victim in branch 2 lies on the SAME curve as the junction
  survivor (edge (p_r,p_n) carries that curve), so the merged edge chain stays
  on one conic — output edges keep endpoints on their stored `Curve`.

## 5. Oracles

- Unit (branch table): `reversal_collapse_direction` returns `(p_n, p_r)` for
  same-curve `p_after`, `(p_r, p_n)` for different-curve `p_after` (both
  branches exercised; mutation-inverting the comparison must fail the tests).
- Corpus trackers (RED → GREEN): R0009 / R0011 / R0091 replays must not carry
  `"does not lie on its ellipse"` nor `"does not lie on its circle"` in their
  boolean-failure sets (the failure-moved analog for circle junctions).
- Regression: full `yang-rs` suite; `m8_swiss_cheese_chain` chain suite
  (the sweep is shared by the M8 fold-gate paths); F0086/F0087 corpus replays
  unchanged.

## 3b. Second mechanism (R0091 + R0009): §4.4.1(b) merge survivor selection — DIAGNOSED, BANKED-UNWIRED

> **Status (2026-07-08): the ranked-survivor fix below is implemented as the
> banked primitive `sub_feature_merge_direction` (+ unit tests, mutation-
> killed) but is DELIBERATELY NOT WIRED at the (3c) merge call site.** Wiring
> it clears R0091's ellipse-endpoint wall but flips the case ERROR →
> SUPPORTED_WRONG: the completed subtract tessellates to χ = −4 against the
> meta's euler_target 2, with watertight/volume/monotonicity/bbox all
> passing. χ = −4 (three handles, one shell) could not be verified OR refuted
> in-session: the meta χ is the naive 3-op default (`compute_euler_target`
> returns 2 for ≠2-op cases), but op 1 is a PARTIAL 219° circle-revolve
> (genus 0 sausage, not a torus), so the honest handle count needs a real
> derivation. Precedent: the world-space canonicalization pass stayed
> unwired while it flipped any case to SUPPORTED_WRONG (roadmap §0.2 item 1
> bullet 3). UNBLOCK PATH: verify the R0091 output's true χ via the Cherchi
> sidecar reference parity (roadmap §6) or refute the meta χ from the
> authored numbers (the R0078/C0035-F1 authoring-error protocol); then wire
> the ranked survivor and un-ignore the R0091 tracker.

The same output signature has a second producer, measured on R0091
(`YANG_V_PROBE`, 2026-07-08): the Stage-4 §4.4.1(b) sub-feature merge picks its
collapse survivor by LOWER INDEX. At micro model scale the merge legitimately
fires (features below the A14.2 floor are unrepresentable), but when the pair
is (exactly-relocated conic endpoint, plain chord vertex) and the chord vertex
has the lower index, the EXACT vertex is destroyed: the conic edge's surviving
endpoint is the unrelocated chord vertex (R0091: v15 on the ellipse exactly,
merged into v8 — a plane∩plane triple point 8.1e-8 off the ellipse; the
post-merge recompute assigns the merged edge (8,14) the ellipse, and kernel-v2
rejects endpoint 8).

Yang Fig. 11(b) ("if an endpoint p of the split edge is too close to q, we
merge p with q", `refs/text/yang2025_hybrid_boolean.txt` §4.4.1) merges INTO
the existing exact intersection point — q survives. Survivor selection must
rank exactness:

| # | rank(u) vs rank(v) | Survivor |
|---|---|---|
| 1 | equal | lower index (UNCHANGED — byte-identical to pre-fix) |
| 2 | u higher | u |
| 3 | v higher | v |

with rank: 2 = closed-form junction vertex (`vert_ell_junction` /
`vert_circle_junction` / `vert_junction` — exact on TWO curves),
1 = single-curve conic endpoint (the `conic_endpoint` scan set),
0 = plain mesh vertex. A plain vertex merged into a conic endpoint moves by
less than the feature floor (definitionally the same point, A14.2); its
incident plane∩plane `LineSegment` edges have no positional membership check
(endpoints implicit), so no counterpart wall exists on the line side.

### Additional oracles

- Unit: `sub_feature_merge_survivor` branch table above (equal-rank keeps the
  index rule; higher rank survives regardless of index order — both argument
  orders exercised).
- Corpus: R0091 tracker (spec §5) is the RED for this mechanism.

## 3c. Third mechanism (R0072/F0045 + R0011's deeper wall): straight-run reversals are never swept

Measured on R0072 (`YANG_S6_CYCLE_DUMP` + `YANG_V_PROBE` + `KV2_EARCLIP_PROBE`,
2026-07-08): the §4.5.3 sweep's loop eligibility requires EVERY cycle edge to
carry a conic (`all_conic`, Circle|Ellipse) — a reversal on a **straight
intersection run** (`Curve::LineSegment` seam) is never corrected. R0072's
A-face-0/B-face-5 seam carries Stage-0 mint vertices v197/v198 at chord-crossing
positions; their neighbor v6 is a seam × ruling-line triple point whose Stage-4
`vert_line` relocation lands EXACTLY on the true junction — at seam parameter
−2.95e-6, BEHIND the stale mints (t = 0, +1.44e-6). The output loop then runs
forward 1.44e-6 and doubles back 4.39e-6 along the exact same line (cos = −1.0,
cross = 4e-26): a self-intersecting ring that kernel-v2's exact CDT correctly
refuses (`TessellationFailed { "ring rejected by CDT" }`). This is verbatim
Yang §4.5.3 ("the surface intersection may exhibit a reverse sequence of
points after convergence") on a line instead of a conic.

### Design (extends §3's sweep, same victim rule)

1. **Eligibility is PER-SITE, not per-cycle** (amended after RED measurement:
   R0072's face-0 boundary MIXES solid edges with seam runs, so whole-cycle
   eligibility never fires on real mixed boundaries). Every patch cycle is
   scanned; a position `p_r` is a §4.5.3 site iff BOTH incident edges carry a
   Stage-3 `curves` entry (conic or `LineSegment`). A vertex where an
   intersection run meets a curve-less edge (solid edge, torus chain,
   gate-skipped seam) is a run BOUNDARY: never tested as `p_r`, and as `p_n`
   it is junction-protected (the run's exact endpoint — collapse `p_r` onto
   it). All-conic cycles behave byte-identically to the pre-§3c sweep; conic
   runs inside MIXED cycles become sweepable (previously skipped wholesale) —
   the same paper logic at newly reachable sites.
2. **Exact tangent for line edges** (paper Fig. 15: t_pr = n_A,pr × n_B,pr):
   from the edge's incidence surface pair via `surface_normal_at`. Cross
   magnitude ≈ 0 (tangent/parallel surfaces — e.g. §4.5.5 coplanar seams) →
   cannot diagnose → healthy skip.
3. **Junction guard for line edges**: `Curve::LineSegment` carries no payload,
   so run identity uses the edge's UNORDERED incidence surface pair. At `p_r`:
   both edges LineSegment with different pairs → corner, skip test (the
   PR-KV11 conic guard's analog). Conic edges keep the curve-equality guard
   byte-identically.
4. **Victim selection**: `reversal_collapse_direction` extended with the same
   pair test — a `p_n` where the surface pair changes is a junction (exact
   endpoint) and survives; the out-of-order `p_r` collapses onto it.

### Branch table additions

| # | Edge kinds at `p_r` | Pair/curve state | Action |
|---|---|---|---|
| 4 | both LineSegment | pairs differ | healthy (corner) — no test |
| 5 | both LineSegment | pairs equal, normals cross ≈ 0 | healthy (cannot diagnose) |
| 6 | both LineSegment | pairs equal, reversal detected, `p_n` pair changes ahead | collapse `p_r` onto junction `p_n` |
| 7 | both LineSegment | pairs equal, reversal detected, same pair ahead | collapse `p_n` onto `p_r` (paper default) |
| 8 | mixed conic/LineSegment at `p_r` | curves differ | healthy (existing conic guard, now reachable) |

### Additional oracles

- Unit: `surface_normal_at` (plane/cylinder/sphere/cone canonical points);
  line-run reversal branch table (4–7) on synthetic curve+incidence maps.
- Corpus trackers (RED → GREEN): R0072 and F0045 replays must reach
  tessellation with NO `"ring rejected by CDT"` and NO relocation of the wall
  into `"collapsed at render precision"` / `"inverted final triangle"`.
- Regression: full yang-rs suite (fuzz_boxes planar byte-identity), chain
  suite, F0086/F0087 + planar-heavy corpus spot checks.

### Status (2026-07-08, post-implementation measurement)

- **R0072: ERROR → SUPPORTED_CORRECT** (38 sweep collapses on the replay; the
  reversed seam mints are removed and the exact junction survives).
- **F0045 is a DIFFERENT mechanism** — its FaceId(9) 16-gon ring
  self-intersects at MACRO scale (segment 10→11 × 12→13, excursion ~5e-2 at
  model scale ~0.4), far above any MIN_FEATURE_SIZE reversal; the §4.5.3
  sweep correctly does not touch it. Its tracker stays `#[ignore]` RED as the
  pin for a future output-loop macro-ordering campaign.

### Final shipped design (after two RED adversary cycles)

Two pre-existing pins caught wider scopes converting loud walls into silent
or broken geometry: `annular_cap_hole_crossing_stays_loud` (unsupported
Stage-0 hole-rim crossing repaired into Ok(non-watertight)) and
`corner_in_band_reverts_keep_true_junction` (coarse 7-gon conic chords in a
mixed cycle false-positive the 45° reversal band — 2π/7 ≈ 51° corners — and a
cascade eats a genuine arc). Shipped scope:

1. **All-conic cycles: byte-identical pre-§3c semantics** (every position a
   site, curve-identity junction guard).
2. **Mixed cycles: straight-run sites ONLY** (both incident edges
   `LineSegment`), with the §3c guards: run identity by unordered incidence
   pair (branch 4), tangent availability n_A × n_B ≥ TAU_WORK checked BEFORE
   the U-turn arm (branch 5 — §4.5.5 coincident-pair seams are undiagnosable),
   junction/run-end-protected victim selection (branches 6–7 + run-end), and
   the resolution gate |victim − survivor| ≤ 2·d_ε (a §4.5.3 correction is a
   resolution artifact by definition; both points sit within their own chord
   band — derived, not widening).
3. **Stage-0 admission wall (the hole-rim pin's documented intent)**: an
   annular face whose HOLE rim circle is STRICTLY CROSSED by a partner disc
   rim (|r1−r2| < d < r1+r2 in the shared plane) walls loudly
   (`CoplanarFacesUnsupported`, probe tag `annular-hole-rim-crossing`) BEFORE
   any overlay build — arc∩arc crossing + bore-lateral split propagation is
   out of increment scope, and the general overlay otherwise emits doubled
   sheets whose symptoms surface only downstream.

**Corpus outcome**: R0072's FaceId(9) straight-run spur is repaired (wall
moves to FaceId(11), whose reversal sits on conic sites in a mixed cycle —
the disproven class below); its tracker stays `#[ignore]` RED as the pin for
a stable coarse-N conic-site criterion.

**P10 records (disproven alternatives — do not retry):**
- CONIC sites inside mixed cycles, under ANY of the tried eligibility rules
  (unrestricted; transversal incidence pair; coincident-seam cycle
  exclusion): the 45° band false-positives on coarse conic chords
  (`corner_in_band`, N=7 → 51° corners, and 2·d_ε ≈ 0.19 cannot gate 0.15
  excursions at that coarseness), and overlay-adjacent conic runs repair
  unsupported Stage-0 crossings into silent geometry (the hole-rim pin's
  bore-rim sites are cylinder×plane TRANSVERSAL, so transversality does not
  separate them).
- Tightening `check_watertight_2manifold` to reject `fwd == rev > 1` double
  covers false-positives on the Steinmetz subtract (its kept mesh is
  LEGITIMATELY edge-doubled along the surface-tangency seam; re-confirms
  `yang_kept_mesh_manifold_gate` §2b: no mesh-level manifold invariant
  survives the kept set).

## 6. Failure modes

- Branch 2 with `collapse_vertex` dropping zero triangles → existing loud
  `Stage4ReversalUnresolved` STOP (unchanged).
- A reversal whose BOTH neighbors are junctions of other curves cannot occur at
  `p_r` (PR-KV11 guard); if the loop degenerates below 3 vertices the existing
  `LoopTooSmall` STOP fires.

## 7. Research basis

- [#24] Yang et al. 2025 §4.5.3, Fig. 15
  (`refs/text/yang2025_hybrid_boolean.txt:709-745`): "p_r is a point on the
  intersection curve C between the two surfaces S_A and S_B" — the reversal
  test and the p_n removal are defined for consecutive points progressing
  along ONE intersection curve. A vertex where the loop transitions between
  curves is an intersection-curve ENDPOINT, outside the correction's scope;
  removing it destroys exact topology rather than repairing point order.
- Deviation record: this was a paper-faithfulness bug in the PR-YR10 port
  (the sweep treated whole conic loops as one curve for victim selection,
  though `is_reversed` had already been junction-guarded for the TEST in
  PR-KV11).
