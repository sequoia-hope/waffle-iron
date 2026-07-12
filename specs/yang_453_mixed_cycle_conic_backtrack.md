# §4.5.3 mixed-cycle conic backtrack sweep (yang-rs Stage 4) — task #145

Amendment to `specs/yang_453_junction_protected_collapse.md` §3c. Bug-fix
cycle per FIP §8. Retires the re-entry CDT zigzag class (R0061 / R0063 /
R0095 / F0085): a boolean SUCCEEDS while emitting an output loop whose
vertices double back along their intersection conics, and the NEXT op's
re-entry CDT (`tessellate_lateral_holed_cdt` unroll / planar CDT) fails on
the self-crossing chain — the error surfaces one op downstream of the fault.

## 1. Goal

Diagnosis (probes `YANG_T145_PROBE` re-entry side, `YANG_T145_SWEEP_PROBE`
producer side, R0061 2026-07-12): the producing op (cylinder boss ∪ gear
cut — cylinder r = 0.0200557 × ~70 chorded gear facets) relocates each
facet-arc's interior Steiner vertex onto its exact facet-plane∩cylinder
ellipse, and at near-tangency (major_radius up to 2.4 at minor 0.02) the
relocation lands the interior vertex BEHIND the arc's start junction (or
past its end): the chain backtracks ~1.2e-3 along the curve. ~44 such
sites on R0061's op 2, |t̃| from 4.5e-7 to 4.5e-2 — turn angles within
2.5° of exactly 180°.

The §4.5.3 sweep is the paper's mechanism for exactly this ("the surface
intersection may exhibit a reverse sequence of points"), but the shipped
§3c scope excludes ALL conic sites inside MIXED cycles (both P10 records:
the 45° angle-band false-positives on coarse conic chords, and
overlay-adjacent repairs of unsupported Stage-0 crossings). The zigzag
sites sit on the lateral face's boundary cycle, which mixes solid edges
with the conic chain — structurally unreachable by the current sweep.

This increment adds a THIRD site arm that does not reuse the disproven
angle-band test: **exact conic parameter-order reversal**. For a site
whose two incident edges carry the SAME conic (Circle/Ellipse, identity up
to the sign of the stored plane normal — a frame choice, not geometry),
compute the conic parameters t_b, t_r, t_n of the three points via the
shared exact parameterization (`project_onto_circle` / `ellipse_param`),
wrap consecutive deltas to (−π, π], and flag a reversal iff the deltas
have OPPOSITE signs — "p_b, p_r, p_n progress along the intersection curve
in sequence" (Yang §4.5.3) tested against the curve's own parameter, which
for our closed-form conics is the ground truth the paper's discrete
tangent test approximates.

## 2. Parameters

None (internal control flow of `sweep_reversed_intersections`). No new
tolerances: identity is exact field equality (f64 negation is exact); the
parameter test is a sign comparison; the existing 2·d_ε resolution gate
and junction-protected victim selection are reused unchanged.

## 3. Branch table (extends §3c's table)

Site `p_r` in a MIXED cycle, both incident edges carrying curves, NOT both
`LineSegment` (previously: unconditional skip):

| # | Incident curves at `p_r` | Parameter deltas d1 = t_r−t_b, d2 = t_n−t_r (wrapped) | Action |
|---|---|---|---|
| 9a | same conic (exact or up-to-normal-sign) | d1·d2 < 0, \|d1\| ≤ \|d2\| | reversal, backward overshoot: collapse `p_r` onto `p_b` (the parameter-nearer neighbor) + 2·d_ε gate |
| 9b | same conic (exact or up-to-normal-sign) | d1·d2 < 0, \|d2\| < \|d1\| | reversal, forward overshoot: collapse `p_r` onto `p_n` + 2·d_ε gate |
| 10 | same conic (exact or up-to-normal-sign) | d1·d2 ≥ 0 | healthy (monotone progression — coarse corners and steep sinusoid peaks land here) |
| 11 | same conic, parameter undefined at any point (degenerate projection) | — | healthy skip (cannot diagnose) |
| 12 | different conics / conic+LineSegment / non-conic payloads | — | skip (junction or vocabulary boundary — unchanged §3c behavior) |

Victim selection deliberately does NOT reuse `reversal_collapse_direction`
here (RED measurement, R0061 2026-07-12): at a (junction, interior,
junction) site that rule collapses `p_r` onto `p_n` — the FAR junction, a
whole facet arc away (measured up to 1e-1) — and the 2·d_ε gate rightly
refuses everything. The overshoot geometry says the survivor is the
parameter-NEARER bracketing neighbor: the collapse length is then the
ACTUAL overshoot (~1e-3, within the resolution gate). `p_r` is always the
victim (a same-conic site vertex is never a junction — junctions change
curve identity and land in branch 12).

All-conic cycles and straight-run sites are byte-identical to the shipped
§3c behavior (no change to `is_reversed`).

## 3b. Second mechanism (RED measurement, R0061 2026-07-12): azimuth-slide
relocation on near-tangent sections

With branches 9a/9b wired, R0061's producing op still emits macro spikes the
2·d_ε gate rightly refuses (probe `[t145-gate]`): each facet arc's interior
vertex sits 3.4e-3 … 1.03e-1 from BOTH its bracketing junctions, while the
junctions themselves cluster within ~1e-4 (near-tangency). These are not
resolution artifacts — the sweep must not repair them.

Root cause one layer down: `project_onto_ellipse_via_cylinder` preserves the
vertex's cylinder AZIMUTH and intersects that generator with the section
plane. On a near-tangent section (plane nearly parallel to the axis,
`|n·â|` = minor/major = 0.0084 on R0061's spike ellipse) the axial solve
amplifies by `1/(n·â)`: a vertex within its honest chord band (ρ ≤ d_ε) of
the curve is relocated up to `~ρ/(n·â)` ALONG the curve — a silent macro
slide masquerading as a §4.4.1 relocation. (The circle path is immune:
azimuth projection IS nearest-point there.)

Fix (branch table):

Scope: the cylinder×PLANE arm (`second_cyl: None`); the cyl×cyl arm's
`gate` already carries the KV9 per-point gradient amplification and stays
byte-identical.

| # | Azimuth projection move `|proj − p|` | Action |
|---|---|---|
| R1 | ≤ gate (the same per-site band the ρ gate uses) | keep the closed-form azimuth projection (byte-identical — the well-conditioned majority) |
| R2 | > gate | recompute as the IN-PLANE nearest point on the ellipse (drop `p` onto the section plane, first-quadrant symmetry reduction, then BISECTION of the distance stationarity `f(t) = (a²−b²)·cos t·sin t − \|u\|·a·sin t + \|v\|·b·cos t` on the guaranteed bracket `[0, π/2]` (`f(0) ≥ 0 ≥ f(π/2)` unconditionally) — [#1] Patrikalakis-Maekawa-Cho point-to-curve projection; a plain Newton from the `atan2` seed DIVERGES to a far stationary point on eccentric ellipses, the F0047 vertex-42 RED measurement); accept iff ITS move ≤ `2·gate/sin θ`, θ = angle between the cylinder radial and the plane normal AT the relocated point (the derived corridor amplification — the same gradient-band the circle-junction and pp-plane gates use; the KV11 box∪cylinder pin measures a LEGIT 2.7e-2 move against a 1.8e-2 flat gate at sin θ ≈ 0.55, so a flat move gate is the WRONG metric — RED measurement 2026-07-12) |
| R3 | nearest-point move > `2·gate/sin θ` | loud `Stage4RegionInvalid { OffCurveBeyondChordBand }` — the vertex cannot be relocated within the derived band; never emit the slide |

Note the repair for the R0061 class is R2's REPLACEMENT, not R3's
rejection: a corridor vertex's true nearest curve point is right beside it
(its bracketing junctions are ~1e-4 away), so the nearest projection kills
the macro slide by landing where the azimuth solve should have.

The in-plane Newton is intrinsically well-conditioned at 3D near-tangency
(it never divides by `n·â`; the flat flank of an eccentric ellipse has low
curvature, and points within the band are far inside the curvature radius
everywhere except the tips, where the seed is already near-exact).
`relocate_onto_implicit_pair` is NOT usable here: its parallel-normals
rank guard fires precisely at the tangency line where these vertices live.

Interaction with §3: after R2 placement, interior vertices land within the
band of their true positions; residual SUB-band overshoots past a junction
(e.g. R0061 face 2's original 1.2e-3 zigzag) are exactly the §4.5.3
resolution artifacts branches 9a/9b sweep. Both mechanisms are required.

Scope: the `vert_ellipse` (cylinder∩plane / cylinder∩cylinder) relocation
loop — the path the class exhibits. The cone∩plane (`vert_cone_ellipse`)
analog is a candidate follow-up if a case exhibits it.

## 4. Invariants

- I1: the new arm fires ONLY at sites both §3c arms skip today — any input
  with no same-conic mixed-cycle backtrack produces a byte-identical mesh.
- I2: a legit near-180° 3D turn with MONOTONE conic parameter (the steep
  sinusoid peak of a near-tangent plane∩cylinder ellipse) is NOT collapsed
  — the discriminator is parameter order, never turn angle.
- I3: coarse same-conic corners (the `corner_in_band` 7-gon, 51° turns)
  progress in parameter and are NOT collapsed (the §3c P10 record stays
  honored — the angle band is not consulted).
- I4: junction vertices survive (reversal_collapse_direction unchanged);
  the 2·d_ε resolution gate stands: a backtrack whose repair moves a
  vertex farther than 2·d_ε is left for downstream loud rejection.
- I5: normal-sign identity is exact (bit-negation), never tolerance-based.
- I6 (mechanism 2): a §4.4.1 ellipse relocation never moves a vertex farther
  than the per-site band gate — the closed-form azimuth projection is kept
  verbatim when it satisfies this (R1, byte-identical), replaced by the
  in-plane nearest point when it does not (R2), and the op STOPS loudly when
  no within-band relocation exists (R3). No silent macro slide survives.

## 5. Oracles

- Unit (branch table 9–12, helper level, `tests_unit/m5_case_iv.rs`):
  - backtrack on a shared circle (params 10°→5°→20°) → reversed;
  - monotone coarse 7-gon corner (0°→51.4°→102.9°) → healthy;
  - steep-peak adversary: eccentric ellipse (a=2.4, b=0.02), three points
    straddling the minor-axis peak with monotone params but 3D turn > 90°
    → healthy (kills a turn-angle/U-turn mutant);
  - sign-flipped identity: same ellipse stored with negated normal on the
    two edges → still one curve (site eligible, backtrack detected);
  - different conics at `p_r` → not a site;
  - `conics_equal_up_to_normal_sign` field-sensitivity (center/radius/
    major-axis mismatch → false).
- Unit (mechanism 2): near-tangent fixture (r = 0.02, |n·â| = 0.01 → a = 2)
  — a vertex displaced azimuthally by 1e-4 off the exact curve: the azimuth
  projection's move is ~1e-2 (the documented slide, ×100 amplification);
  `project_onto_ellipse_nearest` moves ≤ a small multiple of the residual,
  lands ON the ellipse, and returns the parameter of the true local point.
  Degenerate seed (ellipse center) diverges to a macro move → the loop's R3
  gate rejects it (helper-level assertion).
- Corpus (RED → GREEN): R0061 replay — op-3 auto-union must no longer fail
  `holed lateral CDT failed`; class siblings R0063 / R0095 / F0085
  re-measured (same producer mechanism suspected, verified individually).
- Regression: `cargo test -p yang-rs` full suite (fixed-point sweeps,
  annular_cap_hole_crossing_stays_loud + corner_in_band pins untouched),
  `./scripts/test.sh rewrite`, full assay vs baseline 238C/0W/53E/4U/0T —
  zero-lost gate.

## 6. Failure modes

- Degenerate parameter projection (point at conic center) → branch 11
  healthy skip; no panic.
- Collapse dropping zero triangles → existing loud
  `Stage4ReversalUnresolved` (unchanged).
- A backtrack farther than 2·d_ε from its junction → not repaired; the
  downstream consumer keeps rejecting loudly (honest ERROR preserved,
  never silent geometry).
- Chord aliasing: the (−π, π] wrap assumes each chord subtends < π, which
  Stage-1 resolution (≥ 8 segments/turn) and junction splitting guarantee;
  a hypothetical > π chord would misread direction but still only trigger
  a collapse bounded by the 2·d_ε gate.

## 7. Research basis

- [#24] Yang et al. 2025 §4.5.3, Fig. 15
  (`refs/text/yang2025_hybrid_boolean.txt:709-745`): reversal = points NOT
  progressing along the intersection curve in sequence. The paper's
  discrete-tangent 45°–135° band and the degenerate-t̃ direct arm are
  PROXIES for parameter order, needed because the general case has no
  global curve parameterization. Our Circle/Ellipse intersection curves
  have exact closed-form parameterizations (the shared PR-YR11 frame), so
  the parameter-order test implements the paper's criterion directly and
  sidesteps both documented failure modes of the band proxy.
- Deviation record: none introduced — this narrows the gap between the
  §3c implementation scope and the paper's §4.5.3 scope (the paper does
  not scope reversal correction by cycle composition).

## 7a. Analytical vs. approximate method

Exact: conic identity is exact field comparison; parameters come from the
closed-form conic frame. No mesh approximation is introduced; the repair
is the paper's own mesh-resolution correction bounded by the derived
Stage-1 chord band.
