# §4.5.3 straight-run OVERTAKE arm (yang-rs Stage 4)

Amendment to the §4.5.3 reversed-intersection sweep
(`stage4_correct::reversal::sweep_reversed_intersections`; prior amendments:
`yang_453_junction_protected_collapse.md`, `yang_453_mixed_cycle_conic_backtrack.md`,
`yang_453_pair_chain_reversal.md`). Bug-fix cycle per FIP §8.

## 1. The measured customer (R0059, 2026-09-11 — every number live)

R0059 (`extrude(circle) ∪ revolve(circle) ∪ extrude(rectangle)`, scale 3.8e2;
0.7 s) was one of the three ULP-parse latents unmasked by the 2026-09-07
`float_roundtrip` loader change (ledger `docs/yang_tail_triage.md` "Unmasked
2026-09-07 by exact float parsing"). Op 3's union STOPs in kernel-v2 at
`TessellationFailed { face: FaceId(29), reason: "ring rejected by CDT" }`: a
planar 5-vertex ring `P0 → P1 → P2 → P3 → P4 → P0` whose P3, P0, P4 are
collinear on the box's base edge (P0 at t = 0.831 of P3 → P4, off-line
8.9e-15) — the parallelogram `P3 → P0 → P1 → P2` (sides 91.7 / 115.1, both
pairs parallel) with P4 an 18.6-unit spike past P0 (`KV2_RING_REJECT_PROBE`).

Provenance (`YANG_S6_LOOP_PROV`, `YANG_V_PROBE_NEAR`): the box's base is
sketched ON the previous body's cap (`stage0=true`: a coplanar overlay of
A's cap plane and B's base plane), so the base edge L1 lies in the cap and
the overlay subdivides it at every cap-triangulation crossing — 417, 415,
413, 411, 409 (all unmoved, `disp=0`, exact on L1). Where L1 exits the cap
the overlay crosses the rim CHORD; that vertex, v20 (`line=true
pp_planes=true endpoint=true`), is the L1 × rim-circle junction and Stage 4
relocates it 20.6 units ALONG L1 to the exact circle (pre (−355.70, 100.93)
→ (−371.04, 114.67), z = 28.07 — a grazing rim crossing amplifies the chord
sagitta along the line). Two overlay vertices, 417 (−357.15) and 415
(−369.42), lie between v20's old and new positions: the run now walks
20 → 417 → 415 → 413 forward-back-forward, and the emitted loop
self-overlaps. Yang §4.5.3, verbatim: "the surface intersection may exhibit
a reverse sequence of points after convergence … if the consecutive points
that exhibit reversal are collinear, t̃ is almost degenerate. In such a
case, we directly detect the reversal" (`refs/text/yang2025_hybrid_boolean.txt:736-745`).

Why the shipped sweep cannot see it: the run's edges are overlay seams
between two COINCIDENT planes, so the exact tangent `n_A × n_B` vanishes and
`is_reversed`'s branch 5 returns healthy before the collinear U-turn test
(kept so Stage-0 crossing artifacts stay loud —
`annular_cap_hole_crossing_stays_loud`). And even where the tangent exists,
the paper's remove-next cascade would collapse 415 → 417 (12.3) and finally
417 → P3 (82) against the 2·d_ε resolution gate (d_ε = 5.92 here) — the
overtaken points are not within THEIR chord bands of anything; they are
inside the JUNCTION's certified displacement.

## 2. Branch table

For a §4.5.3 site `(p_b, p_r, p_n)` in a mixed cycle, evaluated BEFORE
`is_reversed` (arm gate `YANG_453_OVERTAKE`; `0|off` = the pre-arm sweep):

| # | condition | action |
|---|---|---|
| 1 | not a straight run (`same_line_run` ≠ `Some(true)`: an edge is not `LineSegment`, or the unordered surface pair changes at `p_r`) | not this arm (fall through) |
| 2 | `p_r` moved, or NEITHER / BOTH of `p_b`, `p_n` moved | not this arm |
| 3 | the moved neighbour `J` has no entry position (minted during Stage 4) or `|J_new − J_old| ≤ TAU_WORK·(1+scale)` | not this arm |
| 4 | the polyline does not double back exactly at `p_r` (`|unit(p_r−p_b) + unit(p_n−p_r)| ≥ TAU_WORK`) | not this arm (a corner) |
| 5 | `p_r` not strictly inside `[J_old, J_new]` along `unit(J_new − J_old)`, or off that line by > `TAU_WORK·(1+scale)` | not this arm (an unrelated U-turn — stays loud downstream as before) |
| 6 | otherwise | **collapse `p_r` onto `J`** (`collapse_vertex`; the 2·d_ε resolution gate is bypassed — see I2) |

The sweep restarts after every collapse (existing behaviour), so a run of
several overtaken points resolves one site at a time (R0059: 417 then 415).

## 3. Invariants

- I1 (no motion): nothing moves; the survivor is the relocated junction at
  its Stage-4-certified position; victims are unmoved run vertices.
- I2 (certificate): every collapse length is `≤ |J_new − J_old|`, a
  displacement Stage 4 already certified through `J`'s own relocation gate
  (the line × circle junction gate `band + d_ε`, whose along-line
  amplification is exactly what put the phantoms inside the segment). No
  new tolerance; the 2·d_ε gate (derived for chord-band-bounded points) is
  not the bound for this configuration.
- I3 (fail closed): every branch 1–5 leaves the site to `is_reversed`
  unchanged, so any case without a moved-and-entry-positioned neighbour,
  including every Stage-0 crossing artifact, is byte-identical.
- I4 (surface-agnostic): the arm reads only the run's own geometry; a
  coincident-plane seam is diagnosable.

## 4. Oracles

- Unit (`tests_unit/s453_line_overtake.rs`): the R0059-shaped run (J moved
  along the x-axis over two subdivision points; both collapse onto J in
  sequence; the far run point beyond `J_new` does not); every fail-closed
  branch (no / both / self moved; no entry position; J moved the other way;
  a genuine corner; a pair change).
- Assay: R0059 ERROR → SUPPORTED_CORRECT (smoke-pinned); full corpus 0
  WRONG, no CORRECT lost; `annular_cap_hole_crossing_stays_loud` green.

## 5. Ledger

- 2026-09-11: spec; arm + 2 pin functions (`tests_unit/s453_line_overtake.rs`) SHIPPED always-on. R0059: the two predicted collapses (417, 415 onto v20) ⇒ SUPPORTED_CORRECT 2.8 s. Corpus (release, 8 jobs, 600 s; wall 719.7 s): 280C / 0W / 26E / 4EE / 0T, exactly one category move (R0059), zero detail moves — the arm fires on no other corpus case. `annular_cap_hole_crossing_stays_loud` green (rewrite tier).
- 2026-10-12 (later): §6 — the CARRIED-CURVE arm (P0036). See §6's own ledger line.

## 6. The CARRIED-CURVE arm — the same rule on an operand's own typed edge (P0036, 2026-10-12)

### 6.1 The measured customer (every number live)

P0036 (prospector seed 3 index 102, minimized 7→3 `convex4:boss circle:rev
circle:cut`, scale 2.9e-3; the un-minimized lineage raises the same
signature verbatim at `FaceId(51)`): the cut STOPs in kernel-v2 at
`TessellationFailed { face: FaceId(12), reason: "ring rejected by CDT
(degenerate/self-intersecting)" }`. Face 12 is the revolve's START cap — a
disc of radius 4e-4 about (−2e-4, −1e-4) in the plane z = −1e-4 — minus the
cutter, a vertical cylinder of radius 1.25e-3 about (−6.25e-5, 1.04e-3).
The exact face is a lune bounded by a rim arc (178.37° → −12.12°, through
−90°) and a cutter arc, meeting at the two rim ∩ cutter corners A (−12.124°)
and B (178.369°).

`KV2_RING_REJECT_PROBE`: a 50-point ring with ONE crossing, at A. The walk
reaches a point at −10.141° on the rim and steps BACK to A at −12.124°.
`KV2_RING_PROVENANCE` names the loop kernel-v2 was given: … `Arc` from
−34.615° (5 interior samples) → a vertex at **−6.923°** → `Arc` (1 interior
sample) → A. The vertex at −6.923° is `disc = 0`, `cut = −3.38e-5`: a rim
point 3.4e-5 INSIDE the exact cutter. The −10.141° point is a render GRID
sample (`−2 · 2π/71`, to 3.5e-14°) emitted by BOTH arcs; kernel-v2's
sweep-dedup removed the spike's duplicate and the vertex with it, and the
CDT saw the remaining crossing — a correct refusal of a wrong loop.

`YANG_FACE_CENSUS` + `YANG_S6_CYCLE_DUMP=all` + `YANG_V_PROBE=5,7,9`: the
cap's Stage-1 rim is a **13-gon** (vertex v7 at −6.923°), the cutter a
**12-gon** (sagitta `1.25e-3·(1 − cos 15°) = 4.26e-5`). v7 lies 3.4e-5
inside the exact cutter and OUTSIDE its chord polygon, so Stage 2 keeps it;
the arrangement's rim × cutter corner v9 sits at −5.864° (`disc = −1.75e-6`,
on the cutter chord), 1° PAST v7. Stage 4 relocates v9 in closed form onto
the exact corner A (both residuals ≤ 1e-17) — 6.3° along the rim, over v7.
The cap cycle `[…, 5, 7, 9, …]` then walks v5 (−34.6°) → v7 (−6.9°) → A
(−12.1°): forward and back along ONE circle. The §4.5.3 sweep never tested
the site: `(5,7)` and `(7,9)` are rim edges — solid edges of operand A with
no Stage-3 `curves` entry — so the both-untyped branch handed them to the
PAIR arm, which needs both inputs on the edge and declined. The §4.5
boundary-point domain certificate is silent too (`YANG_S45_BOUNDARY_DOMAIN_LOG`
appends nothing): A glides within the cap plane and crosses no crease —
a boundary point gliding along its boundary curve, the class the paper
exempts from §4.5.1 and the certificate honours.

### 6.2 The paper

§4.5.3 (`refs/text/yang2025_hybrid_boolean.txt:736-745`): *"the surface
intersection may exhibit a reverse sequence of points after convergence …
we remove the reversed point"*, with the criterion *"If p_b, p_r, and p_n
progress along the intersection curve in sequence"*. v7 is exactly R0059's
overtaken run vertex (§1): a point the resolution artifact (the cutter's
chord sagitta) left on the wrong side of the junction, inside the
junction's own certified displacement. The run is a CARRIED curve — the
operand's rim circle — so the straight arm's collinear test (branch 4)
does not apply; the exact progression test is the curve's own parameter
order, which the task-#145 shared-conic arm already uses
(`conic_param_deltas`).

### 6.3 Branch table (`carried_curve_overtake_site`, evaluated in the both-untyped branch BEFORE the pair arm; gate shared with §2: `YANG_453_OVERTAKE`)

| # | condition | action |
|---|---|---|
| 1 | either incident edge is typed (in `curves`), or either edge's incidence does not dedup to EXACTLY two entries of ONE input on two DISTINCT surfaces, or the two edges' surface pairs differ (a corner of the operand) | not this arm (the pair arm / `is_reversed`, byte-identical) |
| 2 | `p_r` moved, or NEITHER / BOTH of `p_b`, `p_n` moved | not this arm |
| 3 | the moved neighbour `J` has no entry position, or `\|J_new − J_old\| ≤ TAU_WORK·(1+scale)` | not this arm |
| 4 | the operand's B-Rep has no edge shared by the two faces carrying the run's surfaces that passes within `TAU_WORK·(1+scale)` of `p_r` — or several with DIFFERENT curves, or a curve with no point distance here (`SurfacePair`, open conic) (`carried_edge_curve`) | not this arm |
| 5 | carried `LineSegment`: the straight arm's own geometry (`segment_overtake` — §2 branches 4–5) | collapse `p_r` onto `J` iff it certifies |
| 6 | carried periodic conic (`Circle` / `Ellipse`): the walk does NOT reverse at `p_r` in parameter order (`conic_param_deltas`, `d1·d2 ≥ 0`) | not this arm (a monotone run) |
| 7 | `p_r`'s parameter not STRICTLY inside `J`'s displacement arc `(t(J_old), t(J_new))`, measured the short way round (`wrap_pi`) | not this arm (an unrelated U-turn — stays loud downstream) |
| 8 | otherwise | **collapse `p_r` onto `J`** (`collapse_vertex`; the 2·d_ε gate bypassed exactly as in §2 branch 6) |

### 6.4 Invariants

- I1–I4 of §3 hold verbatim: nothing moves; every collapse is bounded by
  `J`'s certified displacement (the arc from `t(J_old)` to `t(J_new)` on
  the carried curve); every declining branch is byte-identical to the
  pre-arm sweep; the arm reads the run's own geometry — the operand's
  typed edge — never a surface-pair tangent.
- I5 (curve identity is the OPERAND'S): the run's curve comes from the
  operand's B-Rep edge shared by the two faces the Phase-A incidence
  names (bit-exact `Surface` equality — `inherited` IS the operand face's
  surface), never reconstructed from the surface pair.
- I6 (vocabulary): every typed input curve with a parameter and a point
  distance — `LineSegment`, `Circle`, `Ellipse` — is a run; a carried
  `SurfacePair` / open conic declines (branch 4), loud downstream as
  before. The same "every typed input curve" principle as the §4.4.2
  restoration (`yang_434_output_chord_refinement.md`).

### 6.5 Oracles

- Unit (`tests_unit/s453_carried_curve_overtake.rs`): the P0036-shaped
  run on a unit rim (a closed-cylinder B-Rep supplies the carried edge;
  both walk directions collapse v7 onto A); every fail-closed branch (no /
  both / self moved; no entry position; J moved the other way; a monotone
  walk; a typed edge; a mixed-input edge; a surface-pair change; a sub-band
  displacement); `carried_edge_curve` resolves the shared rim and declines
  off the rim and for faces sharing no edge; `segment_overtake` keeps the
  R0059 geometry.
- Chain (`kernel-v2/tests/s453_carried_rim_overtake_chain.rs`): P0036's
  tube − cylinder through `kernel_v2::revolve` + `boolean_op` — RED with
  `YANG_453_OVERTAKE=0` (the verbatim STOP), GREEN with the arm: the cut
  validates and no cap rim vertex survives inside the cutter.
- Assay: P0036 ERROR → SUPPORTED_CORRECT (smoke pin); the un-minimized
  index-102 lineage builds through all seven ops; full corpus exactly one
  category move, zero detail drift.

### 6.6 Ledger

- 2026-10-12 (later): spec; arm + 4 pin functions SHIPPED always-on under
  the §2 gate. P0036: the one predicted collapse (v7 onto v9 = A) ⇒
  SUPPORTED_CORRECT 0.7 s; the index-102 lineage's Main body completes
  (volume 3.007e-9, every oracle PASS). Corpus: see `docs/yang_tail_triage.md`
  §2026-10-12 (later).
