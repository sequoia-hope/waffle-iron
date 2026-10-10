# Yang §4.5 — the BOUNDARY-POINT domain certificate

**Status (2026-09-28): LANDED, always-on as a TRIGGER + INVENTORY.
P0003 CONVERTS (ERROR → SUPPORTED_CORRECT: the §4.5.2 ladder's first
rung, d_ε/2, emits fire-free and is adopted).** The hard STOP (`stop`
mode) is measured, not shipped — it converts five CORRECT gear cases to
ERROR whose sub-chord facet creases the ladder cannot reach (§7).
Corpus measurement in §6–§7.

Ledger: N66 (RESOLVED). Triage row: `docs/yang_tail_triage.md`
2026-09-28. Code: `stage4_boundary_curve.rs` (`CreaseIndex`,
`crease_divider`, `boundary_crease_crossed`), `stage4_correct.rs`
(`boundary_domain_postcondition`), unit pins
`tests_unit/s45_boundary_domain.rs`.

## 1. The defect, anchored (P0003, 2026-09-28)

P0003 (prospector seed 1 index 14, minimized 6 → 2 ops): a non-convex
9-gon boss (extrude 50 mm along +y) cut by a 340° revolve of a 49 mm
circle about an axis 110.5 mm from its centre — a torus with an open 20°
wedge whose two planar end caps pass through the boss. The op STOPped
one crate later, in kernel-v2's render tessellator: `TessellationFailed {
face: FaceId(22), reason: "torus patch UV-CDT failed" }`.

`KV2_TORUS_PATCH_EDGES=22` printed the output face's loop: one vertex at
**y = 0.0509928**, above the boss's top cap at y = 0.05, joined to the
loop by a 1.8 mm `LineSegment`. Nothing of A − B can lie above A.
`YANG_MESH_DUMP=1` located it: mesh vertex **v44**, which entered Stage
4 at (0.010135, 0.049702, −0.032414) and left relocation at
(0.009949, 0.050993, −0.032883). Both positions were identified
analytically (scratch script, not by eye):

| vertex | role (Stage-2 arrangement) | pre | post | verdict |
|---|---|---|---|---|
| v44 | B's end-cap **rim chord** (a `Circle` edge of B) crossing A's lateral face 4→5 | on the lateral plane, on the chord (1.15 mm inside the circle), y = 0.0497 | the exact **circle × lateral-plane root**: on the torus (6.9e-18), on the 20° cap plane, on the lateral plane — and y = 0.051, **past the lateral face's top edge** | phantom junction |
| v15 | A's **top edge** 4→5 (a `LineSegment` edge of A) crossing B's torus facet | on the top edge, angle 20.29° (inside the torus face) | the exact **line × torus root**: on the torus (8e-16), on the top edge, angle **18.73°** — inside the revolve's open (0°, 20°) wedge, **past the torus face's rim circle** | phantom junction |

The true topology at this corner is two junctions: J = rim circle × top
cap = (0.008767, 0.05, −0.032522), which is 1.23 mm inside the top face
from edge 4→5; and q₂ = top edge × end-cap plane = (0.010092, 0.05,
−0.032522), on the edge. The B rim's chord sag (1.15 mm at Stage-1
density) is of the same order as J's clearance from the edge, so at
chord resolution the rim appeared to exit the boss through the LATERAL
face and the top edge appeared to exit the cutter through the TORUS;
the exact solves faithfully completed those wrong crossings on the
extended surfaces. `YANG_433_PHANTOM=1` could not judge either (v44:
`CURVED-EDGE`, v15: `UNSUPPORTED` — no line × torus all-roots solver),
`YANG_451_*` printed nothing (the corridor family requires the crossed
corner to be an existing mesh vertex on the travel segment), the §3t
crease certificate is wired only into the conic triple arm and knows
only circle creases of cylinder/cone pairs.

**Density ladder (debug build, `YANG_CHORD_REFINE`): 1× ERROR; 2×, 4×,
8×, 16× all SUPPORTED_CORRECT.** The case is a resolution deficit and
§4.5.2 resolves it; what was missing is the trigger.

## 2. The paper

§4.5 (`refs/text/yang2025_hybrid_boolean.txt:648-651`): *"After
optimization, we collect the point pairs that cannot converge to a
distance of 0 **within their domains**."* Both vertices converged to a
distance of 0 — on the extended surfaces. Neither did so within its
domain.

§4.5.1 (`:637-651`, Fig. 13): the first strategy *"only applies to the
interior points but not to the boundary points that glide along the
boundary curves … For other cases, we apply the second strategy"* —
§4.5.2 local refinement. v44 rides B's rim (a boundary curve of B); v15
rides A's top edge (a boundary curve of A). Both are boundary points,
and the paper assigns them to refinement, not to corner transit.

§4.5.1's own words for the defect (`:672-676`): *"a full step length
that takes the point to a position p1 **outside the surface S2** where
the point is initially located"*. The certificate answers exactly that
question.

## 3. The certificate

Per relocated vertex, per face its attributed triangles carry, per
crease of that face: did the step `pre → post` cross the crease?

**Creases come from the operand's B-Rep edges** (`CreaseIndex::build`):
every loop edge of a face is a crease unless the surface across it is
the same surface (a seam or a split bounds no domain). The neighbour is
found by shared edge index or — the m1 convention emits one directed
copy per half-edge — by the unordered vertex pair with the same curve.
This replaces the §3t reconstruction from surface pairs, which could not
name a `LineSegment` crease between two planes at all and, for a torus ×
axial plane, would have to choose between two circles.

**Each crease has a DIVIDER plane and an EXTENT** (`crease_divider`):

| edge curve | divider | extent |
|---|---|---|
| `LineSegment` | the plane spanned by the edge and the face normal at its midpoint (transverse to the face along the whole edge) | the segment |
| `Circle` | the circle's plane | the circle |
| `Ellipse` / `Parabola` / `Hyperbola` | the curve's plane | the whole plane (its plane meets its surface in that one curve) |
| `SurfacePair` | declined (no plane; never approximated) | — |

**The predicate** (`boundary_crease_crossed`) is the §3t SIGN test —
both ends farther from the divider than the PROPAGATED
evaluation-precision band (divider + own surface + neighbour surface
bands, `junction_certificate_band`) and on strictly opposite sides —
**plus the extent test**: the point where the step meets the divider
must lie on the crease within `travel + band`. A mesh end sits within
one chord sag of the curve it samples and the step is that sag's
correction; nothing farther along the divider than the step's own
reach was crossed by it. The extent test is what makes the line
vocabulary safe on a non-convex face (the 9-gon's edge lines pass
through its interior) and the circle vocabulary safe on a torus rim
(the cap plane cuts the tube a second time half a turn away). A vertex
riding the crease — either end within the band — is a boundary point
gliding along its boundary curve and is exempt, as in §3t.

**The postcondition** (`boundary_domain_postcondition`, gate
`YANG_S45_BOUNDARY_DOMAIN`: unset = ON, `stop`, `census`, `0`/`off`)
runs at the end of `stage4_relocate_and_correct_inner`, after every
repair, on every vertex whose position differs from the entry snapshot.
It RECORDS the fires (a thread-local count, `boundary_domain_fires`;
one line per fire into `YANG_S45_BOUNDARY_DOMAIN_LOG` when set). The
op-level driver (`boolean::refine_452_domain`) reads the natural op's
count and, when it is nonzero, runs the §4.5.2 ladder (d_ε/2, d_ε/4 —
the same rungs and Q3 guard shell as `refine_452`) with one more
clause: a rung is adopted only when it emits a watertight 2-manifold
body AND records NO fire. When no rung does, the natural output stands
as it always has, and the fires are inventoried. `stop` is the hard
STOP (`Stage4RegionInvalid { RelocationCrossedCrease }`, the variant
§3t introduced for the same defect on the triple arm) — the A/B knob
the measurement below uses, NOT the production default; §7 records why.
No under-resolution certificate names a first rung: the overrun past a
crease is not the true junction's clearance from the corner (P0003's
0.99 mm overrun and 1.23 mm clearance coincide only by geometry), so
the fixed budget applies.

## 4. Why this and not the corner-transit corridor

The §4.5.1 machinery (`specs/yang_451_corner_transit.md`) repairs an
interior traveller whose corrected junction lies on the NEXT facet of
the same operand, with the crossed corner already a mesh vertex. Here
each traveller's true junction is on the OTHER operand's neighbouring
face (v44: the top cap, not the lateral; v15: the end cap, not the
torus), the corner is not a mesh vertex, and the two travellers'
junctions (J, q₂) are distinct points 1.3 mm apart — Fig. 13's shape,
which the paper excludes from transit by name. Refinement is the
prescribed remedy and it converges at the first rung.

## 5. What the certificate is NOT

Not a band. The sign test is at evaluation precision; the extent
tolerance is the step's own length. No threshold separates fires from
non-fires — a step either crossed a crease of the face it is on or it
did not. A fire never moves a vertex: it names the site and hands the
op to §4.5.2.

Not a wall either. §3t's binding measurement (R0003 carried six genuine
out-of-domain triple relocations and produced correct output) repeats
here at larger scale: the general certificate fires 42 times on R0003
alone, and on R0004, R0032, R0049, R0070 — every one a many-facet gear
operand (a revolved or extruded gear profile) whose facet creases are
closer than the chord band, so the relocations that cross them are the
§4.5.1 corridor family (`specs/yang_451_corner_transit.md` §1) and the
ladder cannot reach them (`stop` mode: no rung converges, budget
exhausted). Those cases pass every oracle they have. Converting them to
ERROR for a defect they survive would break the no-regression bar for
no capability gained — the §3t verdict, restated: the certificate is
the trigger and the inventory; the repair for that family is the
corridor epic, and the paper's remedy for boundary points is the
refinement this increment wires.

## 6. Measurement

### 6.1 P0003

`YANG_S45_BOUNDARY_DOMAIN=census`: 2 fires —
`v15 left B:2 across edge 1 f_pre=−3.84e-4 f_post=+1.64e-3` (the torus
face, its rim), `v44 left A:6 across edge 34 f_pre=−2.98e-4
f_post=+9.93e-4` (lateral face 4→5, its top edge). Armed: the STOP
fires, the ladder's d_ε/2 rung converges, **SUPPORTED_CORRECT** (2.2 s
release). Volume adjudicated independently: 2×10⁸-sample Monte-Carlo
over the exact membership (9-gon prism ∖ {tube distance < r ∧ angle ∈
[20°, 360°)}) = **1.2085e-4 m³ ± 1.3e-8**; pinned as `expected_volume`
with `expected_volume_tol_rel` 3e-3 (the render mesh's inscribed-chord
deficit on a torus, the P0002 precedent). The alternative wedge
convention (solid on [0°, 340°]) gives a cut volume of exactly 0 — the
cutter would not touch the boss — so the geometry itself certifies
which wedge is open.

### 6.2 The hard STOP, measured (`stop` mode, full corpus, release, 8 jobs, 900 s; wall 1174.7 s)

**299C / 0W / 12E / 4EE / 0T.** P0003 ERROR → CORRECT, and FIVE
CORRECT → ERROR: R0003 (`RelocationCrossedCrease v3089`, 65 s), R0004
(the inner `RelocationCrossedCarrierVertex v429` surfaces instead, demand
3080 ⇒ no rung), R0032, R0049 (`v242`, demand 184 ⇒ no rung), R0070.
Every one is a many-facet GEAR operand (revolve(gear) or extrude(gear)
in its recipe). The fire inventory (`YANG_S45_BOUNDARY_DOMAIN_LOG`,
natural density, one line per fire per Stage-4 invocation):

| case | recipe | fires (natural) | travel / overrun range | crossed |
|---|---|---|---|---|
| P0003 | 9-gon boss, circle revolve-cut | 2 | 1.4e-3 – 2.3e-3 / 1.0e-3 – 1.6e-3 | A lateral top edge; B torus rim |
| R0049 | rect revolve, gear cut (scale 4.3e-3) | 1 | 1.7e-5 / 7.2e-6 | a B gear flank's crease |
| R0004 | rect revolve ×2, gear cut (scale 1.42) | 3 | 4.4e-3 – 3.7e-2 / 7.8e-4 – 3.5e-2 | B:0 cap, B:222, B:290 flanks |
| R0032 | (gear operand) | 33 | — | A facet creases |
| R0003 | gear revolve, rect cut (scale 216) | 42 | 3.2e-3 – 9.45 / 1.8e-4 – 8.7 | 40 A revolved-gear facets, B:0, B:4 |
| R0070 | (gear operand) | 86 | — | A facet creases |

These are §3t's R0003 population at the general certificate's reach:
genuine out-of-domain relocations on sub-chord facet creases (the §4.5.1
corridor family — `specs/yang_451_corner_transit.md` §1: "the traveller
sits where the chord-resolution mesh curve crosses the model edge
base∩facet_k of a many-facet operand") that today's output survives by
every oracle the case has. A STOP converts them for no capability
gained; the §3t verdict stands and the production mode is §6.3.

### 6.3 The production mode, measured (record → ladder → adopt fire-free, else natural)

| case | natural fires | d_ε/2 | d_ε/4 | outcome | time |
|---|---|---|---|---|---|
| P0003 | 2 | Ok, unpaired 0, **fires 0** | — | **adopted → SUPPORTED_CORRECT** | 0.5 s |
| R0049 | 1 | fires 4 | fires 1 | natural stands, CORRECT | 4.3 s |
| R0070 | 86 | fires 41, improper 91 | fires 25, improper 64 | natural stands, CORRECT | 17.6 → 36.2 s |
| R0004 | 3 | (natural op is an inner Err; the §4.5.4 graze retry emits) | — | unchanged, CORRECT | 4.2 s |
| R0003 | 42 | (graze retry emits first; ladder not reached) | — | unchanged, CORRECT | 65 → 82.6 s |
| R0032 | 33 | (same) | — | unchanged, CORRECT | 97.6 → 82.4 s |

Refinement THINS the gear fires (R0070 86 → 41 → 25; R0049 1 → 4 → 1)
without clearing them — the facet creases are sub-chord at every
practical rung, the R0085 §8.1 shape. The cost is bounded by the fixed
[2, 4] budget (two extra ops) and only paid by ops that fired.

## 7. Corpus runs (2026-09-28)

**Canonical (production mode; release, 8 jobs, 900 s; wall 1163.3 s): **304C / 0W / 7E / 4EE / 0T + 0 UNSUPPORTED** over 315 cases — exactly ONE category move (P0003 ERROR → SUPPORTED_CORRECT) and ZERO detail moves against the 2026-09-27 (late) baseline (`results.json` diffed per id). The ERROR rows are the seven loud-by-design C-series walls; the P-series tail is EMPTY.**

The inventory (`YANG_S45_BOUNDARY_DOMAIN_LOG`, every Stage-4 invocation incl. retries and rungs; lines per case): R0070 242, R0003 84, R0019 66, R0032 33, R0049 6, F0082 6, F0064 4, R0044 3, R0004 3, R0095 2, R0059 2, R0028 2, R0026 2, P0003 2, R0025 1. Fifteen cases fire; all but P0003 keep their natural output and every one of them is SUPPORTED_CORRECT by its oracles — the certificate's standing customers for the §4.5.1 corridor epic.

The `stop`-mode run (§6.2) is the two-proof twin: same binary family, hard STOP armed — 299C / 12E, the five gear regressions named. Both runs are recorded; the production default is the record → ladder → adopt-fire-free mode.


## 8. The LOCAL ladder — §4.5.2 on the certificate's own sites (2026-10-10, P0031)

**Status: LANDED, always-on, first in the domain ladder.** P0031 CONVERTS
(ERROR → SUPPORTED_CORRECT at the local ladder's first rung). Kill switch
`YANG_452_LOCAL=0|off` = the §6.3 body-wide ladder alone. Code:
`stage4_correct.rs` (`DomainFire`, `boundary_domain_fire_records`),
`boolean/rim_junction.rs` (`domain_fire_local_rim_overrides`,
`coaxial_circle_closure_with_arcs`, `LOCAL_452_STEP_DIVISORS`, arc clipping
in `RimAngleOverrides::finish`), `boolean.rs` (`refine_452_domain`). Pins:
`tests_unit/s452_domain_lens.rs`, kernel-v2
`tests/s452_domain_lens_corner.rs` (mutation-checked RED with the knob
off), `assay_kv2` smoke pin `P0031 → SupportedCorrect` with a Monte-Carlo
volume.

### 8.1 The defect, anchored

P0031 (prospector seed 3 index 57, minimized 3 → 3 ops: circle boss r
11.052, height 7.233; a non-convex hexagon boss z ∈ [−0.6, 19.4] unioned
in; a 3-point star CUT). Same `SelfIntersectingBooleanOutput` on the
un-minimized lineage (`replay_waffle_env`: `FaceId(31) × FaceId(50)`,
3 penetrations), so the minimum is faithful. `KV2_SELFX_SITE_PROBE` names
the pair: output face 32, A's **cylinder** patch, pierces output face 55, a
**plane x = −10.2** bounded by the lines y = ±4.2559 (exactly where that
plane meets the cylinder) — and the star cut's sketch plane IS x = −10.2,
1.0 inside the cylinder, extruded +x: the plane is the cut's **start cap**.
The star's edge v5→v6 (in the cap's (y, z): z = 9.50 − 1.364·(y + 11.2))
crosses the floor z = 0 at y = −4.2335 and the cap∩cylinder line
y = −4.2559 at z = 0.0306. Four surfaces — cylinder, cap, floor, that star
side face — within 0.04 of one point.

The exact output has TWO triple corners there: **C = {cylinder, star face,
cap} = (−10.2, −4.2559, 0.0306)** and **T₃ = {star face, cap, floor} =
(−10.2, −4.2335, 0)**, joined by a 0.039 edge of the cap face; the
cylinder is continuous across the cap line below C (the tool does not
occupy x < −10.2). The kernel built the OTHER pair: `YANG_V_PROBE_NEAR`
shows the subtract's v140 (a floor vertex on the star-face∩floor line at
x = −10.149, where the CHORD cylinder — sag 0.276 at the natural N 14 —
meets that line) relocated to **T₁ = {cylinder, star face, floor} =
(−10.209, −4.2335, 0)**, 9.3e-3 BEYOND the cap plane, outside the tool;
and v139 to **T₂ = {cylinder, cap, floor} = (−10.2, −4.2559, 0)**. Both are
exact triple points of real surfaces; neither is a vertex of the exact
result. The cylinder face then ends at T₁ with no edge on the cap, the cap
face keeps the sliver (T₂, T₃, C), and the two overlap by it.

`YANG_S45_BOUNDARY_DOMAIN=census`: **2 fires** — `v21 left B:7 across
edge 34 f_post=9.3e-3 travel=2.47e-1` (the star side face's cap crease —
the step along the star-face∩floor line from the chord cylinder at
x = −9.963 to T₁) and `v22 left B:1 across edge 7 f_post=1.8e-2
travel=6.49e-1` (the cap face's star-edge crease — the step along the
cap∩floor line from y = −3.607 to T₂). The certificate already saw P0031;
it is P0003's mechanism exactly (§1: "the rim's chord sag is of the same
order as the junction's clearance from the corner, so at chord resolution
the rim appeared to exit through the LATERAL face … the exact solves
faithfully completed those wrong crossings"). What failed is the remedy:
`YANG_452_PROBE` — `d_ε/2 → fires=2`, `d_ε/4 → fires=2`, "no rung emitted
fire-free — the natural output stands"; with `YANG_452_ROUNDS=2,4,8,16,32`,
**d_ε/8 fires 2, d_ε/16 fires 0 ⇒ SUPPORTED_CORRECT**. A resolution
deficit the fixed [2, 4] budget cannot reach.

### 8.2 The paper's remedy is local

§4.5.2 (`refs/text/yang2025_hybrid_boolean.txt:659-670`): *"we increase
the mesh resolution of the parametric surfaces associated with the
erroneous regions … The surfaces requiring refinement include those
traversed by C_p (red regions) and the neighbors of a ring of them (orange
regions). We then compute the intersections between the meshes only in the
refined regions."* The §6.3 ladder re-derives BOTH operands body-wide — the
simplification that landed first. Its budget is what the gear operands
tolerate (§5: R0070 pays 17.6 → 36.2 s for two rungs that clear nothing),
and a deeper body-wide rung multiplies every curved face's density for a
corner on one of them. The #195 arm met the same wall from the other side
and paid its demand LOCALLY (`yang_195_seal_neighborhood_self_overlap` §5k).

### 8.3 The local ladder

A fire now carries its SITE (`DomainFire`: the vertex, its pre and post
positions, the crease it crossed, and every `(input, face)` its live
triangles are attributed to — the surfaces that meet there). The op-level
driver reads the natural op's records and, BEFORE the body-wide rungs, runs
`LOCAL_452_STEP_DIVISORS = [2, 4, 8, 16]`:

- For every incident face of every fire that is a **cylinder or cone**
  (the surfaces whose Stage-1 density is a rim azimuth set), the face's
  coaxial rim closure is taken **with arcs admitted**
  (`coaxial_circle_closure_with_arcs`): a chained operand carries its
  cylinders as arc-bounded strips `[Arc, Line, Arc, Line]` whose two arc
  chains the strip arm pairs index-for-index, exactly as the tube pairs
  its two rings — so the arcs of one strip are the band that moves
  together. Fail-closed as before on a non-coaxial circle edge.
- The lens is the #195 sampler, `submerged_arc_samples(apex, half_span,
  step)`: **apex = the fire's post azimuth** about the axis (canonical
  frame), **half_span = one natural rim step** `2π/N` (the paper's ring of
  neighbours; N = `natural_rim_n` raised by any standing `forced_rim_n`),
  **step = natural step / divisor** — `2·d − 1` on-circle samples per rim,
  identical azimuths on every rim of the closure. Rung d divides the local
  sagitta by ≈ d² (rung 4 = the equivalent of body-wide d_ε/256 on the lens
  alone, 31 samples per rim).
- `RimAngleOverrides::finish` now clips an ARC rim to the azimuths strictly
  inside its sweep (Stage 1 refuses an arc-chord override at or beyond an
  endpoint; the arc runs CCW about its own stored normal, CW in the
  closure's frame when the normal is reversed). A lens near an arc end is
  clipped on BOTH chains, so they still pair. Full rims are untouched.
- **Budget** (`LOCAL_452_MAX_SAMPLES_PER_N = 4`): a closure takes at most
  `4·N` lens samples per rung, N its operand's natural rim count; over it
  the lens is dropped (probe: `lens dropped`) and, with nothing derived,
  the body-wide rungs run as before. A lens the size of the body is no
  longer local — and a rim azimuth set propagates along the WHOLE coaxial
  closure (every band of a revolve), so the cost of an unbounded lens is
  the body's. Measured the hard way: the first corpus run without the gate
  turned R0070 (a revolved gear, 90 fires, N 13 / 12) from CORRECT at 36 s
  into a 900 s TIMEOUT — its rung 1 alone committed 340 + 300 samples
  (`improper=1045 fires=55`) and rung 2 did not finish; with the gate it
  derives nothing (`wants 270 samples over the budget 52`) and keeps its
  §6.3 path, 35.1 s. P0031 spends 6 of its 56.
- Adoption is the §6.3 clause verbatim: watertight (unpaired 0), **no
  fire**, `improper == 0` only under `YANG_452_REQUIRE_CLEAN`.
- A fire whose faces are all planar, spherical or toroidal derives no lens;
  the local ladder stops at the first rung that derives nothing and the
  body-wide rungs run as before. **P0003 (its curved face is a torus)
  therefore takes exactly its 2026-09-28 path**: `local: no lens derivable
  from the fires`, then `d_ε/2 → fires=0`, adopted.

Not a band and not a tolerance: the lens adds exact on-surface samples
where the certificate's sign test says the chord mesh mis-resolved the
corner, and every rung is judged by the same certificate. A14.3: a finer
rim only shrinks sagittas.

### 8.4 Measured

- **P0031 ⇒ SUPPORTED_CORRECT** at the FIRST local rung: `local step/2
  (pts_a=12 pts_b=0) -> Ok tris=248 unpaired=0 improper=0 fires=0` — two
  fires × three azimuths × the strip's two arcs. The output carries C
  (−10.2, −4.2559, 0.030559) and T₃ (−10.2, −4.2335, 0) joined by a
  `LineSegment`, and the star-face ellipse ends at C (`YANG_BREP_PROBE`).
  Volume: kernel 3311.11 vs a **2e8-sample Monte-Carlo over the exact
  membership 3312.71 ± 0.40** — a 4.8e-4 deficit, the render mesh's
  inscribed chords; pinned `expected_volume` with `tol_rel` 3e-3 (the
  P0003 convention). Un-minimized lineage: same conversion path.
- P0032–P0039 re-judged on the new binary: unchanged, all ERROR at their
  recorded sites (none of them is a domain-fire case).
- Generality pin (kernel-v2 `s452_domain_lens_corner.rs`): a cylinder
  r 10 unioned with a box that bites it, then a cut sketched on x = −9
  whose one edge crosses the floor 0.059 inside the cap∩cylinder line —
  two exact corners 0.097 apart against a natural sagitta of 0.29. With
  the knob off the natural output's cap face is refused one gate earlier
  than P0031's (`ring rejected by CDT`); with it the output carries both
  exact corners and nothing inside the cutter's profile on its cap.
- Corpus: §8.5.

### 8.5 Corpus runs (2026-10-10)

**Canonical (release, 8 jobs, 900 s; wall 936.7 s): 330C / 0W / 16E / 5EE
/ 0T + 0 UNSUPPORTED over 351 cases — exactly ONE category move (P0031
ERROR → SUPPORTED_CORRECT) and ZERO detail moves against the 2026-10-10
P0030 baseline (`results.json` diffed per id).** The ungated first run
(no `LOCAL_452_MAX_SAMPLES_PER_N`): 329C / 16E / **1T** — R0070 CORRECT →
TIMEOUT, the measurement behind §8.3's budget.
