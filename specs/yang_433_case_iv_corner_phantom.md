# Spec: §4.3.3 Case-IV corner-phantom rule-out — the ring-CDT family's R0100 wall

**Status: inc-1 BUILT GATED (`YANG_433_GUARD=1|on`) — 2026-08-27. R0100
converts gated (SUPPORTED_CORRECT, all oracles, 1.2 s). inc-0 census run
corpus-wide; results in §5. Gate-on corpus measurement in flight.** Vehicle: R0100
(0.2 s), `TessellationFailed { face: FaceId(15), "ring rejected by CDT" }`
on the op-1 extrude-cut subtract. This is the third of the three standing
ring-CDT verdicts (R0003 face 577 / R0053 face 474 / R0100 face 15,
censused 2026-08-26 in `yang_441_trim_cdt_construction.md` §I13(e)); R0100
was measured OUT of the I13 rim×cut overrun family, the I13d run family,
and the I13e interlocked-pair family — this spec is its own anchor.

## 1. The anchor, measured (2026-08-27)

Instruments: `KV2_RING_REJECT_PROBE` (+ new `hole_pts3` line),
`YANG_441_FOLD_CENSUS` (OFFCURVE arm), `YANG_441_RUN_PROBE_AT` (+ new
`pre=` line), `YANG_S4_RIM_SNAP_TARGET`, offline exact solves
(`scratchpad/r0100_roots.py`, `r0100_truth3.py`).

R0100 = revolve(rectangle boss, 283.7°) − extrude(rectangle, cut). The
subtract's B is a PRISM (6 planes). Face 15 of A is a cone band
(half-angle 11.77°, apex (−158.662, 712.242, 1139.461), axis
(0, −0.62834, −0.77794)), station band [1429.2331, 1545.8089]; across its
bottom rim sits the 78.2° shoulder cone (apex (−158.662, −224.795,
−20.671)). One prism cap-corner (cap S1 = plane n=(0.73825, 0.18282,
0.64928) d=−31.6918; sides S2 = (0.63024, 0.15608, −0.76055)/−94.3444,
S3 = (−0.24038, 0.97068, 0)/114.9161; corner vertex (110.4800, −91.0278,
−51.1769)) sits BURIED in A's material, 2.118 under face-15's cone and
1.333 under the shoulder cone.

**The truth (exact, sampled at 601²–1201² over station ±10 / azimuth
±0.06 rad around the corner):**

- The cut region of face-15's cone near the corner — {S1<0 ∧ S2<0 ∧ S3<0}
  on the surface — is **EMPTY**. The corner wedge clears the surface by
  **≥ 1.33** everywhere (min over the window of max plane violation).
- The entire true local cut lies on the SHOULDER cone: station2
  ∈ [50.3, 61.3] against rim2 = 62.06 — it approaches the rim to 0.74 and
  **never crosses it**.
- Per prism edge at this corner, the exact edge-line × face-15-cone roots
  both lie OUTSIDE the edge's actual segment AND outside face-15's
  station band. Edge S1∩S2 (param t from the corner, wedge exit v0 at
  t=−62.7): roots at **t=+9.30** (behind the corner; station 1425.30,
  below the rim) and **t=−225.9** (beyond the far end; station 1568.7,
  above the band). The B-edge never pierces face 15.

**What the pipeline did:**

- Stage-1's inscribed cone mesh sags **2.26–2.29** under the true surface
  there (measured at the three pre-vertices) — deeper than the wedge's
  1.33 clearance — so the mesh facets clip the wedge: the arrangement
  mints a genuine MESH-level loop [v61, v62, v63] (pre extents 0.30–0.81,
  each pre-vertex ON its prism edge inside the segment, e.g. pre-v61 at
  t=−0.78 with S3=−0.78). **Yang Fig. 8 Case IV** — the meshes detect an
  intersection that does not exist between the surfaces
  (`refs/text/yang2025_hybrid_boolean.txt:436-447`).
- Stage-4 relocation solves each loop vertex onto its own carried triple
  (cone + two prism planes) EXACTLY (implicit residuals ≤ 1.7e-13,
  `YANG_S4_RIM_SNAP_TARGET`), and each solve is the correct nearest root
  (the other root is 225–689 away). But the solutions are VIRTUAL points:
  each violates the loop's remaining prism plane by **+3.00 / +3.13 /
  +9.30** (outside the very wedge the loop claims to bound), the loop
  everts (all three edges' pre→post conic pair-orders invert —
  `YANG_441_FOLD_CENSUS` OFFCURVE ×4, gaps 4.3–9.8 ≈ the full post edge
  lengths), blows up 12× (0.3–0.8 → 4.3–9.8), and two vertices land below
  the rim (stations 1425.30, 1429.03).
- Stage 6 emits the everted loop as face-15's hole; the hole pokes out of
  the outer ring (chart y 1425.30/1432.45/1429.03 against rim y 1429.233)
  and the render CDT rejects — a correct LOUD stop wearing the wrong
  name.

Downstream selectors correctly decline: I13d 22 runs / 44 terminals all
decline (`no_param`/`no_flip`/`no_inversion`/`not_richer`), I13e zero
groups — no vertex is a junction of another's curves; all three are peer
corner junctions of a phantom region. The I9 carrier-domain census
(`YANG_S4_CARRIER_DOMAIN=census`) reports 0 out-of-domain — its
population is curve-interior vertices, not these junctions; its zero is a
vantage statement.

Also real in the same neighborhood: the mesh cap-cycle
[v61, v0, v2, v22, v23, v62] shows the B-side pieces bounded by the
phantom points, and v23 = the S1∩S3-edge × SHOULDER pierce EXISTS in the
mesh, is a true corner (third plane −2.05), and relocated 0.73 correctly.
The mesh MISSED the S1∩S2-edge × shoulder pierce (true point v61n' =
(109.925, −88.786, −51.177), 2.31 from the corner) — the mirrored
Case-III miss on the neighbor face.

## 2. The paper's owner — §4.3.3, the Case-IV rule-out

§4.2.1 (`:436-447`): "Case IV can be filtered by … our optimization-based
intersection line computation." §4.3.3 (`:518-537`): "For both methods,
**if there is no solution in one of the two parametric domains, we regard
it as a solving failure and rule out the aforementioned Case IV** where
the meshes detect intersections that do not exist between the surfaces."

Our pipeline's optimization equivalent is the Stage-4 relocation; the
missing piece is the DOMAIN half of the paper's clause. The relocation
solved the surfaces and never asked whether the solution lies within the
faces' trimmed domains — the B-edge's segment, the A-face's band. The
shipped Case-IV guard (`yang_case_iv_phantom_guard.md`) is pairwise
cylinder×cylinder ANALYTIC-GAP only and CANNOT own this: every pairwise
surface pair here genuinely intersects (three real ellipses, radii
250–940); the phantom is the REGION (corner wedge vs surface), invisible
to any pairwise gap.

**The exact per-claim certificate (constant-free):** a junction vertex
claiming "B-edge E pierces A-surface S" is PHANTOM iff the exact
line(E)×S solve has **no root within E's own segment** (endpoint-grazing
roots = a real B-vertex tangency → out of scope, loud). R0100: both roots
outside [−62.7, 0] for all three edges' claims. The A-face band test is a
second, independent refutation (both roots outside [1429.23, 1545.81]) —
record both in the census; the B-edge segment test alone is already
decisive for edge-pierce claims.

## 3. Increments

- **inc-0 (census, read-only — `YANG_433_PHANTOM=census`):** at the
  Stage-4/5 vantage where patches, cycles, curves, relocations and both
  B-Reps are in scope, for every junction vertex whose carried set is
  {one A-surface, two same-input B-surfaces} (and the A↔B mirror): find
  the B-Rep edge between the two B-faces (Line-carried edges this
  increment; census the rest as `curved-edge` rows), solve line×surface
  exactly, and report per vertex: root params vs the edge's segment, root
  stations vs the A-face's own rim stations where derivable, third-plane
  violations, displacement, loop membership (cycle length, all-cross
  typed edges), and the loop-level verdict (all corners refuted / mixed /
  valid). Corpus-wide run: denominators — how many cases, which of the
  standing ERRORs join (R0003 f577? R0053 f474?), whether every phantom
  loop is fully refuted (no mixed loops).
- **inc-1 (decide the repair from the census).** Candidates, both
  recorded now:
  (a) **Stage-1 derived-density guard**, the shipped Case-III/IV pattern
  generalized from pairwise surface gap to **B-edge-segment ↔ A-surface
  clearance**: for proximal (edge segment, curved face) pairs whose
  segment approaches the surface within the natural chord band WITHOUT a
  root in the segment, derive the rim N with `sag(N) ≤ clearance/2` and
  rebuild both operands (also repairs the mirrored Case-III miss on the
  shoulder in the same stroke — the finer meshes sample v61n'). Fully
  a-priori, byte-identical when no pair qualifies.
  (b) **Downstream §4.3.3 rule-out + mesh update**: certify the phantom
  loop at Stage 4, remove it (A-side hole un-minted), and restore the
  B-side cycles through the true corner (insert B's corner vertex, reroute
  the three B-face slivers). Heavier: conformal multi-patch mesh update
  on the B side, plus the missed shoulder crossings still need (a)'s
  refinement or a §4.5.2 local pass.
  The census picks: if every corpus instance has the guard's shape
  (B-vertex buried within natural sag of an A-face with a clear
  edge-clearance derivation), (a) is the structural fix and (b)'s
  certificate survives as the loud last-resort STOP (a typed
  `PhantomIntersectionLoop` in place of today's misnamed CDT reject) for
  configurations a future derivation misses.
- **inc-2:** build the chosen repair gated; measure R0100 + corpus;
  flip per the standard proofs (gate-off byte-identical; gate-on
  category-improving with explained detail rows only).

## 4. Non-goals / guards

- No tolerance bands anywhere: the certificate is root-in-segment
  (exact); the guard's N is derived (A14.3 monotone-safe).
- A loop with MIXED corner verdicts is NOT ruled out — loud census row,
  no repair claim (P10).
- The shoulder's own near-rim notch tip (0.74 below rim2) is real and
  stays; nothing here touches rim-proximity as a criterion.

## 5. inc-0 census results (corpus-wide, 2026-08-27)

Manual per-case loop over all 312 cases (the parallel runner nulls child
stderr), `YANG_433_PHANTOM=census`. Two census refinements were measured in
during the run: extrude prisms do NOT share edge indices between faces (a
geometric endpoint-identity fallback finds the shared edge), and an edge
parallel to / lying in a target PLANE is a contact, never a refuted pierce
(F0064/F0067 would otherwise read false phantoms).

**Cases with true phantom claims: 5 — ALL ERRORs, ZERO in any of the 272
SUPPORTED_CORRECT cases.**

| case | claims | shape | floor sweep (debug `YANG_NSEG_FLOOR`) |
|---|---|---|---|
| R0100 | 3 | the anchored corner wedge (distinct edges, singleton claims) | ERROR at ≤28, **SUPPORTED_CORRECT at 30–64** |
| R0004 | 2 | corner-shaped (adjacent cone-target edges, near-tangent roots st≈44.48) | stays ERROR (first error is the unrelated `RevolveAxisIntersectsProfile`) |
| R0011 | 4 | cylinder-target, wild out-of-segment roots (chained B-Rep) | stays ERROR @48 |
| R0044 | 7 | cylinder-target clusters (chained B-Rep) | stays ERROR @48 |
| R0053 | 64 | NOT this family: seam-run populations (many vertices claiming the SAME edge at the SAME root, e.g. 12× e23 at t=8.000000, planes at constant z) — a coplanar-graze signature, M8 territory | stays ERROR @48 |

Denominator statement: the corner-phantom guard's measured addressable
population is R0100 (1 case). R0004/R0011/R0044 carry phantom claims behind
other walls (recorded; re-measure at flip time). R0053's population claims
are a different family. R0003 f577 (KV9-F2a) has ZERO phantom claims — its
fold stays with `yang_434_output_chord_refinement.md`.

## 6. inc-1 as built (gated)

`edge_graze_min_rim_segments` in `boolean/rim_junction.rs`, fourth element
of `boolean()`'s guard `req` max. Per (LineSegment B-Rep edge of one
operand) × (Cone/Cylinder face of the other, banded by its own rim
stations via `face_station_band`): exact roots from the shared
`stage4_phantom::segment_surface_roots`; a root inside the segment AND the
face band = real pierce (skip); otherwise the segment↔surface clearance
`g` (65-sample min minus the Lipschitz slack `len/128`, samples counted
only within the band extended by each sample's own distance — a
no-tuned-margin superset since the perpendicular foot lies within `d` of
the point) derives the smallest `N` with `sag(r_face_max, N) ≤ g/2`.
Fail-closed skips: touching (`g ≤ 0`), demand > 4096, Sphere/Torus
targets, curved edges, faces with no rim circle. Self-limited by
`natural_rim_n` like the sibling guards.

The band restriction is EXACT, not a tolerance: a face's inscribed mesh
lives strictly within its rim-station sweep (rim vertices lie ON the
rims), so a graze against the surface's infinite extension cannot mint a
phantom on that face. Measured: without banding R0100's op-2 union derived
N=28; with banding 107 (both ops' demands are in-band real geometry;
op-1's 217 unchanged). R0100 gated: the subtract's phantom never forms,
the case completes SUPPORTED_CORRECT in 1.2 s.

4 unit tests (`edge_graze_tests`): the R0100 corner edge derives within
[30, 64] (green floor 30, derivation ≈39); a piercing segment derives
nothing; a far segment's demand is absorbable; a touching segment stays
loud.

Known quirk shared with the sibling guards: `natural_rim_n` reports a
LOWER N (13) than Stage 1 actually chooses (24) — the self-limiting gate
is conservative in the boost-more direction. Not changed here (all three
sibling guards share it; changing it is its own measured increment).

## 7. inc-1 corpus measurement — FLIP REFUSED (2026-08-27)

Two full gate-on corpus sweeps (manual per-case loop, release,
`YANG_433_GUARD=1`):

**Sweep 1 (per-segment trigger):** 52 cases boosted (demands to N=1449
from near-tangent real geometry), R0100 converts, but EIGHT CORRECT cases
regress (R0017 breaks under a mere N=33 mesh change) and R0011's loud
ERROR turns silently WRONG (χ=0). Rejected.

**Sweep 2 (corner-cluster + inside-only trigger):** 26 cases boosted.
TWO conversions — R0100 AND R0049 (bonus) ERROR→SUPPORTED_CORRECT — and
the sweep-1 regressions of R0016/R0020/R0023 are rescued by the scoping.
Still regressing: F0067→UNSUPPORTED(coplanar), F0085→TIMEOUT,
R0054→TIMEOUT, R0017→ERROR, R0095→ERROR, R0011→SUPPORTED_WRONG, and
R0003/R0081 ERROR→TIMEOUT (cost). Rejected.

**Why this family cannot flip as a Stage-1 guard:** the sibling guards'
trigger populations are 1–2 cases; corner grazes are ubiquitous (26
cases), and ANY mesh-density change flips marginal cases (R0017's F2b
lift-refinement is balanced on its exact split sequence). Sharper triggers
(demand caps, `g < natural sag`) keep the same regressors — their clusters
are genuinely sub-sagitta. The trigger that would be exact — "this cluster
will mint a CLOSED junction-cornered loop" — is not derivable at Stage 1.

**Why the downstream §4.3.3 rule-out is not a shortcut either:** ruling
out the A-side loop leaves the B-side pieces bounded by the phantom
vertices; their true boundary routes through the prism corner and the
mirrored Case-III crossings the mesh MISSED (v61n′) — geometry that does
not exist in the mesh and must be created. That is the phase-3
junction-layer conformal mesh update (`compliance endgame`, epic #169),
not an increment of this spec.

**Disposition:** the guard stays BUILT, GATED OFF
(`YANG_433_GUARD=1|on` = dev knob), with its unit tests; the census
(`YANG_433_PHANTOM=census`) is the family's permanent instrument; R0100
keeps its loud wall, now correctly NAMED (Case-IV corner phantom, this
spec §1). The family's structural fix rides the junction layer. R0049's
gated conversion is recorded as a second would-be customer.
