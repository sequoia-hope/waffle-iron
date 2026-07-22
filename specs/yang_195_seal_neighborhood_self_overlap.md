# #195 — F0082 Extrude-12 residual: seal-neighborhood operand self-overlap

Status: CHARACTERIZATION (probe-first, plan-of-record discipline)
Predecessors: #194 (`specs/yang_194_subtauwork_edge_collapse.md`, retired the
sub-TAU_WORK twin layer), #188 (`specs/yang_188_f0082_j3_envelope_selection.md`,
§10.10 named this residual "secondary near-dups").

## 1. Problem statement

F0082's Extrude-12 union STOPs loud at `s4-shell-euler χ=3` off SIX
REAL-length double-cover edges in the seal neighborhood (the wall-plane
column at x≈0.30945, z≈2.0942). #194 proved these are NOT sub-resolution
twins: every edge is balanced (fwd=rev=2) but carries FOUR distinct
triangles — two sheets crossing through each other.

## 2. Measurements (2026-07-22, this session)

### 2a. Output-side attribution (`NONMANIFOLD_SITE_PROBE` s4-dc-attr arm)

All 24 double-cover-edge triangles attribute to operand **A** (the chained
seal-carrying body = Extrude-11's union output), across FOUR faces forming
THREE crossing seams:

| Seam | Faces | Double-cover edges |
|---|---|---|
| cap-disc × wall | A#362 (plane n≈(0.0506,−0.0178,0.9986)) × A#368 (wall plane n≈(0.9987,0.0009,−0.0506)) | (930,931), (930,934), (934,936) |
| wall × seal-plane | A#368 × A#370 (plane n≈(−0.0682,0.0516,−0.9963), d=2.1030) | (931,971) |
| seal-plane′ × tube | A#371 (SAME plane params as 370 — a distinct coplanar face) × A#373 (tube cylinder r=0.2123, axis through v935) | (932,994), (971,994) |

Each face covers BOTH directions of each shared edge — a 4-page book seam,
i.e. two kept sheets crossing, not a shared boundary. Face 362's fan
reaches the tube-axis vert v935; its far verts (e.g. v932 =
(0.31075, 0.09002, 2.09411)) measure **+1.25e-3 beyond the wall plane**
— the #188 masked-triple beyond-wall band (+1.29e-3) exactly.

Note: A#370 and A#371 are two distinct faces with IDENTICAL plane
parameters (same normal AND d, same orientation) — an intra-operand
same-plane face pair that `scan_near_coplanar`'s intra arm does not STOP.

### 2b. Input-side exact scan (`YANG_INPUT_SELFX_PROBE`, new)

`cherchi_rs::detect_improper_contacts` + double-cover scan on every
operand mesh handed to the arrangement, whole F0082 chain:

- **Every boolean in the chain is clean (improper=0) EXCEPT Extrude-12's
  operand A** (2012 tris): **5 improper pairs**, exactly the same four
  faces (1771 on 362 × 1891/1892 on 368; 1892 on 368 × 1897 on 370 /
  1994 on 373; 1917 on 371 × 1994 on 373).
- The self-overlap is therefore INHERITED by Extrude 12 from the
  producing op: **Extrude-11's union output B-Rep re-tessellates into a
  self-intersecting operand mesh.** The flagship union (#188) succeeds
  but emits a body whose cap-disc sheet penetrates its wall sheet by
  ~1.25e-3 in the seal neighborhood — a silent-wrong at op 11 caught
  loudly one op later.

### 2c. Producer attribution CONFIRMED — the submerged corner is B-Rep v925

The involved-face loop dump (same probe, loop arm) on Extrude-12's
operand A (= Extrude-11's output B-Rep):

- **The beyond-wall point IS a B-Rep boundary vertex: v925 =
  (0.31075, 0.09002, 2.09411)** — #188's antipodal ellipse↔rim triple
  point, to every digit. It appears in the loops of FOUR faces:
  **362** (cap disc: v925,v926,v927,v928,v929,v930,v931), **370** (seal
  plane: v925,v931,v930,v929,v942,…), **371** (coplanar twin:
  v951,v950,v949,v948,v925,v959,v960) and **373** (tube lateral, both
  loops). Wall face **368**'s loop does NOT contain it (it carries the
  on-wall seal cluster v926,v948,v949,v950,v951 instead).
- Faces 362 and 370 share the straight collinear chain
  v929–v930–v931–v925 (opposite traversal) — the intersection line of
  their two planes; **B-Rep v931 = the tube-axis point** is a boundary
  vertex of both.
- v925 measures **+1.25e-3 beyond wall face 368's plane** — i.e. a
  boundary vertex of the union output lies strictly INSIDE the union's
  material. The emitted envelope passes through the interior: the
  near-v925 regions of faces 362/370/371/373 overlap face 368's region.
  **Extrude-11's union emits a self-intersecting B-Rep** (the #188
  "wall-masked triple / submerged rim run" made boundary).

### 2d. Producing-op mechanism MEASURED (inc-1, same day)

`YANG_SELFX_PROBE` (the banked #173 exact final-mesh scan) across the
whole F0082 chain, joined with §2b's input scan:

- **The producing union's own kept mesh is already self-crossing at the
  seal corner: 7 improper pairs** (chain boolean #10, output 1956 tris;
  the dump's coords are the seal-corner cluster — v925, v926, v948, the
  tube-axis point, all at z≈2.094). Attribution: (A,361)×(B,2),
  (A,366)×(B,0), (A,366)×(B,2) and an INTRA-TOOL pair (B,0)×(B,2) —
  A=accumulated chain body (361 cap-plane, 366 wall), B=the tube tool
  (0 = cap disc, 2 = seal plane).
- **That boolean's INPUT meshes are clean (§2b: improper=0 on both).**
  So the crossing is MINTED IN-BOOLEAN, not inherited: the true
  cap-surface × wall-surface penetration (~1.25e-3) is SUB-SAGITTA in
  the input chord meshes (no input tri-tri crossing → the exact
  arrangement rightly mints no cap×wall curve → labeling keeps the
  whole cap), and **Stage-4 relocation then mints the true junction
  v925 BEYOND the wall**, pulling the rim/cap sheets into crossing
  position — the classic Yang §4.5.4 relocation-minted illegal
  self-intersection (the N2-remit removal half, exactly the class the
  #173 exact STOP was P10-refuted over: it fires on 33 CORRECT cases
  because most relocation crossings are benign chord-noise; THIS one is
  wall-masked, survives emission, and detonates one op later).
- Gate ledger at the producing op: the exact probe SEES it
  (improper=7, probe-only by design); the #173 production render gate
  does NOT fire — depth 1.25e-3 is 5.6× the grazing band
  (max_abs·TAU_WELD_MAX ≈ 2.2e-4), so the suspected miss is the
  PR-KV11 vertex-adjacency skip (362/368 adjacent via shared edge
  v926–v927; planar render CDT makes large corner tris sharing those
  verts). Not yet verified in the gate itself.

## 3. Fix directions (producer-side; both live, spec-first)

The producing union must not emit the submerged v925-corner sheet
regions as boundary. Candidate vehicles:

1. **§4.5.4 removal via corner-junction trim (structural, paper's own
   remedy)**: at the producing op's Stage-4/5, the relocation-minted
   submerged regions (beyond the wall) must be removed and the boundary
   terminated at the wall-crossing junction curve — the J3 osculation
   corner assembly #188 §10.10 deferred; the needed junction is the
   "old phantom to every digit" triple point (#188 inc-4d). This is the
   N2 removal half with its first 0-WRONG-blocking customer.
2. **Graze-guard extension (#172 pattern)**: the cap×wall penetration
   is a genuine sub-sagitta Case-III-class graze of the TRUE surfaces
   (inputs clean, true surfaces cross by 1.25e-3). Detect it
   cross-operand pre-tessellation and rebuild at derived rim N so the
   arrangement samples the crossing and labeling trims the overhang
   naturally (scope lines derived as in
   `specs/yang_172_case_iii_graze_guard.md`).
3. **P10 net only (never in place of 1/2)**: producer-side loud STOP
   when an emitted boundary vertex measures beyond an adjacent face's
   surface by more than the derived band — converts the producing-op
   silent-wrong into a loud producer STOP (fails F0082 one op earlier,
   honestly).

Consumer-side normalization (re-arranging inherited self-overlap at
Stage 1 of the next boolean) is REJECTED: it would silently launder
invalid producer output (P9).

## 5. Inc-2 — vehicle DECIDED: rim×plane graze-guard arm (#172 pattern)

### 5a. Phase-0 measurement (2026-07-22, release single_case sweep,
`YANG_NSEG_FLOOR` with the debug gate locally lifted)

| rim-N floor | F0082 verdict |
|---|---|
| baseline | ERROR (the loud Extrude-12 χ=3 STOP) |
| 32 | **SUPPORTED_WRONG χ=1** — under-sampling is worse than none (silent) |
| 40 / 41 / 44 / 48 / 64 | SUPPORTED_CORRECT (all oracles) |

Mechanism verified at floor 48 with `YANG_INPUT_SELFX_PROBE`: **every
operand mesh in the whole chain scans clean** (baseline: Extrude-12's
operand A improper=5). The producing union's emitted B-Rep stops being
self-intersecting — the paper's §4.5.4 remedy ("we detect these illegal
intersections and perform local refinement … these illegal intersections
are eliminated", `refs/text/yang2025_hybrid_boolean.txt:752-757`) holds
end-to-end on the shipped pipeline: once the Stage-1 mesh samples the
cap×wall crossing, the arrangement mints the true curve, labeling trims
the overhang, the junction/envelope machinery (#146/#169/#188) assembles
the corner, and no submerged relocation occurs. This is the #172 Case-III
fingerprint exactly (transversal graze, ERROR→CORRECT once sampled — not
the #137 tangential-wander class).

Vehicle 1 (Stage-4/5 corner-junction trim) is NOT needed for this class:
the junctions assemble themselves once the crossing is sampled. It
remains the named vehicle for true tangential/osculation classes (#137).

### 5b. Detection class

Cross-operand pair: a **Circle rim edge** (closed or arc) of operand X ×
a **Plane face** of operand Y, both directions. With rim (center c, unit
normal n, radius r) and plane (unit normal m̂, m̂·p + d̂ = 0):

- `k = √(1 − (n·m̂)²)` (sine of the plane/rim-plane angle),
- signed circle-point distance to the plane spans `s ± r·k`, `s = m̂·c + d̂`,
- the rim crosses the plane iff `|s| < r·k`; the shallow-side extent is
  `depth = r·k − |s|`.

The rim's chords recede radially inward by at most `sag(r,N) =
r(1−cos(π/N))`, so a crossing with `depth ≤ sag` can be missed by the
mesh (the plane's own mesh is exact — single-sided recession, unlike
#172's two-cylinder sum). Demand the smallest N with
`sag(r, N) ≤ depth/2` — the same factor-2 margin as #172/Case-IV
(guaranteed mesh-level penetration ≥ depth/2 regardless of chord phase;
A14.3: a finer N is always chord-valid). The floor-32 WRONG row is the
measured justification for the margin: bare sampling (sag < depth) is
not enough; margin ≥ depth/2 is (F0082: derived N=41, measured green).

### 5c. Branch table

| Case | Behavior |
|---|---|
| `depth ≤ 0` (no crossing / rim in-plane k→0) | None |
| `0 < depth ≤ noise` (#178-calibrated `max(TAU_MODEL, scale·TAU_WORK)/100`) | authored-coincidence residue → None (flush-assembly rims must not boost) |
| `noise < depth ≤ 2·10⁻³·r` (render-observability line, single-radius form of #172 §3) | None — sub-render lens, §4.5.2 local-refinement territory (P3d); bounds derived N ≈ 71 |
| `depth > 2·10⁻³·r`, derived N ≤ both naturals | self-limiting gate drops it → byte-identical |
| `depth > 2·10⁻³·r`, derived N > natural | **Boost** both operands via `rebuilt_with_min_rim_segments` (same site as Case-IV/III) |
| derived N > 4096 | None for inc-2 — NO SubSagitta STOP arm yet (unlike #172): the class detonates loudly at the next boolean's (4b) gate when emitted; a producer-side STOP (§3 vehicle 3) needs the plane-face extent witness and is a named follow-up, not silently folded in |
| infinite-plane crossing off the bounded face | false boost, cost only (bounded N ≈ 71) — boost arm needs no extent check (mirror of #172) |
| no phase-aware filter for this arm | #172's face-global tri-touch filter is WRONG here: F0082's wall face IS legitimately crossed by the tube lateral elsewhere, yet the cap-rim crossing is still missed — a face-global "meshes touch" test would veto the needed boost. The render line bounds N ≈ 71, so the C0057-class TIMEOUT hazard the filter existed for does not arise; the corpus assay is the P10 verdict |

### 5d. Oracles

- Unit (`tests_unit/s195_rim_plane_graze.rs`): F0082-analog pair →
  Boost(41); deep crossing → tiny N (absorbed by natural N); depth
  below noise → None; sub-render depth → None; rim
  parallel-in-plane → None; tilted rim k-scaling.
- Gate-ON F0082: the producing union's emitted B-Rep scans clean
  (`YANG_INPUT_SELFX_PROBE` improper 5→0) and Extrude 12 succeeds.
- Gate-OFF full assay: byte-identical to the 255C/0W/54E/1T baseline.

### 5e. Gate-ON corpus ledger (2026-07-22 full release assay) — flip BLOCKED

The guard ships GATED OFF (`YANG_RIM_PLANE_GRAZE_ENABLE=1|on` to
enable). Gate-ON measured 255C/1W/52E/2T with these per-case deltas:

| Case | Delta | Reading |
|---|---|---|
| F0082 | ERROR detail moves Extrude 12 → Extrude 14 | the #195 defect is FIXED; new frontier = disordered output-face loop (inc-3, distinct #145/#184-family defect in op-11's output path) |
| R0072 | ERROR → **CORRECT** | conversion banked (the long-standing curved re-CDT micro-scale STOP clears once the graze is sampled) |
| R0095 | ERROR → **CORRECT** | conversion banked |
| R0063 | ERROR → **SUPPORTED_WRONG χ=0** | **THE FLIP BLOCKER** — the boost's chord-phase shift un-masks a silent-wrong path (R0063 is #145 misorder family; likely same class as the F0082 Extrude-14 frontier) |
| R0021, R0061 | CORRECT → ERROR | loud regressions (R0061 = the known dense-mint-sensitive case, third recurrence) |
| F0085 | ERROR → TIMEOUT | boost-induced slowdown |
| 17 cases | detail-only churn within ERROR | the guard fires broadly; STOPs move within the same class |

Flip preconditions (in order): (1) R0063 gate-ON must not be WRONG —
characterize via the same output-loop-disorder lens as inc-3 (they are
likely one class); (2) R0021/R0061 gate-ON regressions resolved or
refuted; (3) F0085 timing within budget. Conversions R0072/R0095 are
the payoff waiting on the flip.

**Blocker triage 2026-07-22 (same day):**

- **R0063 "silent WRONG" REFUTED — meta authoring error, THIRD
  conversion.** The gate-ON χ=0 is the TRUE topology: exact derivation
  from the authored sketch numbers (concentric prisms; w/2 = 4.761e-4
  > r = 4.538e-4 so the partial-depth slot spans the full cylinder;
  h/2 = 4.222e-4 < r so two crescents survive; rect ⊂ gear-root disc
  8.49e-4; gear-top 6.3365e-4 < cut-floor 6.393e-4 by 5.64e-6) — the
  slit band's two crescents form one cycle between the top disc and
  the gear slab: genus 1, single shell, χ=0. The volume-monotonicity
  oracle (increase/decrease/increase, passing) pins the cut direction.
  Meta `euler_target` corrected 2→0 (the R0091/#186 pattern — a target
  never validated because the case never completed at baseline);
  pinned in `assay_euler_consistency.rs`. Gate-ON R0063 is now
  SUPPORTED_CORRECT; gate-OFF unchanged (loud LRR STOP, meta not
  consulted). **No silent WRONG remains gate-ON.**
- **R0021 gate-ON** (boost 11→12): render ring-reject
  `TessellationFailed(FaceId 11, "ring rejected by CDT")` — the F0045
  output-ring family, same class as F0082's gate-ON Extrude-14 detour
  (§5f). One inc-3 fix plausibly clears both.
- **R0061 gate-ON** (boost 9→19): the u32::MAX
  `LocalRefinementRequired` STOP — the known split_max_passes §4.5.2
  shell wall (#171 triage class; R0063's own baseline wall). Loud and
  honest; a pre-existing capability gap re-routed to, not minted.
- Consolidated: the guard mints NO new defect class — the chord-phase
  shift re-routes cases among existing walls. Flip blockers = the
  inc-3 output-ring assembly fix (F0082-E14 + R0021) and the §4.5.2
  LRR shell (R0061), plus F0085 timing.
- **Re-measured gate-ON ledger (post-meta-fix, full release assay):
  256C/0W/52E/2T — NET +1 CORRECT over baseline with zero silent
  wrongs at the standard budget** (deltas exactly:
  R0063/R0072/R0095 ERROR→CORRECT; R0021/R0061 CORRECT→ERROR;
  F0085 ERROR→TIMEOUT). The arm stays gated per the flip precedent
  (#169 P3b inc-5 required zero CORRECT→ERROR regressions); the flip
  lands when inc-3 clears the output-ring class and R0061 is resolved
  or refuted.
- **F0085 "timing" downgraded to a REAL blocker (same day, 400s
  run): gate-ON F0085 COMPLETES past the 120s budget as
  SUPPORTED_WRONG χ=1** — an odd χ is impossible for any valid
  closed surface, so this is an emitted-topology defect (no meta
  correction can apply), merely MASKED as TIMEOUT at the standard
  budget. F0085 is #145 misorder family — a third customer of the
  inc-3 output-ring class alongside F0082-E14 and R0021. The flip
  therefore strictly requires inc-3; do NOT flip on a
  budget-raise-plus-accept basis.

### 5f. Inc-3 first measurement (2026-07-22, gate-ON
`YANG_CDT_PROBE=370` 3D dump) — the disorder is a BOUNDARY-SELECTION
detour, not sample misorder

The failing face-370 outer loop (10 verts, 5 B-Rep edges):
`v1280 →seg→ v1302 →arc(5 samples)→ v1310 →arc(4 samples)→ v1308
→seg→ v1277 →seg→ v1280`. Both v1308 = (0.30912, −0.10843, 2.0839)
and **v1277 = (0.30946, 0.08934, 2.09417)** sit ON the wall column
(x≈0.3095, the #188 wall trace) — v1277 is the on-wall seal-corner
junction the boost now correctly mints (the CORRECTED position of the
old beyond-wall v925, 1.3e-3 away). The 2D projection shows v1277
landing mid-arc (its azimuth is INSIDE the second arc chain's sweep)
and the closing chord v1277→v1280 crossing the arc chain in-plane:
the ring arc OVERSHOOTS the junction azimuth and the loop doubles
back via wall chords — the #188 inc-0 "dead-side detour" fingerprint
(fix = boundary SELECTION at the junction / arc split at v1277's
azimuth), now at the producing op's output-loop assembly with the
properly-minted junction. Inc-3 = envelope-selection/arc-trim at the
new junction on the seal-plane face (the #188 machinery's remit, one
recursion deeper).

## 6. Ledger

- 2026-07-22: task opened (#194 close-out). Output attribution + input
  selfx probes landed (`s4-dc-attr` arm in `stage4_correct.rs` (4b) gate;
  `YANG_INPUT_SELFX_PROBE` incl. double-cover + involved-face loop dump
  in `boolean.rs`). Measurements §2a–§2c: the class is a PRODUCER
  defect — the producing union's output B-Rep is self-intersecting at
  the wall-masked seal corner v925 (+1.25e-3 beyond wall face 368, kept
  as a boundary vertex of faces 362/370/371/373).
- 2026-07-22 inc-2 (same day): vehicle 2 DECIDED, built, SHIPPED
  **GATED OFF** (`YANG_RIM_PLANE_GRAZE_ENABLE`, §5) —
  `rim_plane_graze_n` + `rim_plane_graze_min_segments` in
  `boolean/rim_junction.rs`, folded into the `boolean()` guard req
  alongside the Case-IV/III arms; 4 unit tests
  (`tests_unit/s195_rim_plane_graze.rs`, incl. the F0082 analog
  deriving the measured-green N=41). Gate-ON F0082: the guard fires on
  three tube unions (N=53/43/22); the producing union's emitted B-Rep
  scans CLEAN (`YANG_INPUT_SELFX_PROBE`: baseline improper=5 → 0) and
  **Extrude 12 succeeds — the characterized #195 defect is FIXED by
  the mechanism**. The case's frontier moves one op deeper into
  never-before-reached territory: Extrude 14's input conversion
  rejects a DISORDERED output-face boundary loop (face 370, 10 verts,
  one mid-arc sample appended after the chain end → self-crossing
  loop; the loud CDT reject is correct) — a distinct #145/#184-family
  defect minted in op-11's output/`from_yang` path → inc-3.
  Gate-ON corpus ledger §5e: R0072/R0095 conversions banked, but
  R0063 ERROR→silent-WRONG χ=0 + R0021/R0061 CORRECT→ERROR +
  F0085→TIMEOUT block the always-on flip (P10) — hence the gate.
  `YANG_CDT_PROBE` extended with a 3D global-vert + outer-edge/chain
  dump for the inc-3 trace.
- 2026-07-22 inc-1 (same day): producing-op mechanism MEASURED (§2d)
  via `YANG_SELFX_PROBE` chain sweep — inputs clean, kept mesh dirty
  (7 improper pairs at the seal corner incl. an intra-tool pair) ⇒ the
  crossing is relocation-minted in-boolean (Yang §4.5.4 class, N2
  removal remit), wall-masked, emitted, detonating at the next
  boolean's (4b) gate. Both §3 vehicles remain live (removal/trim at
  Stage-4/5 vs pre-tessellation graze rebuild); next increment picks
  one spec-first, grounded in
  `docs/yang_junction_research_findings.md` (refinement = guarded
  shell) + the §4.5.2/§4.5.4 paper text.
