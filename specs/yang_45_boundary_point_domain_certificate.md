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

