# M8 — Stage-0 rim membership refinement (partner-feature-conservative rim sampling)

**Status:** spec (FIP Phase 1) — §2 measured mechanism COMPLETE (2026-08-11,
F0067 overlay dump); increment 1 (gated) IN PROGRESS.
**Change class:** capability increment, M8 coplanar workstream — the
flush-interface residue the §4.4.1 epic attributed here (spec
`yang_441_trim_cdt_construction.md` §4-J1 rim-trim status block; roadmap §0
item 3 → capability-tails item).
**Crates:** `yang-rs` (`stage0/rim_chords.rs`, `stage0/mod.rs`).
**Gate:** `YANG_STAGE0_RIM_REFINE` (env, default OFF — byte-identical off).
**Paper:** Yang §4.5.5 (coplanar 2D Boolean, Fig. 16) + §4.2.1
(error-bounded discretization as the foundation of *conservative*
intersection decisions). The paper's d_eps=1e-7 sampling makes mesh-level
region membership agree with the exact 2D Boolean by brute resolution; our
fixed coarse chord band does not, and this increment restores the agreement
*locally* where it is violated.

## 1. Defect statement

The Stage-0 coplanar overlay performs the §4.5.5 2D Boolean **against the
disc face's chord polygon** (the Stage-1 rim ring, e.g. a 13-gon on F0067's
r=0.208846 flush rim, sagitta 6.06e-3), not against the exact circle. Every
partner-outline feature that lies strictly INSIDE the exact circle but
OUTSIDE the chord polygon — i.e. in a chord's sag crescent — is
misclassified `AOnly` when its exact membership is `Overlap`:

- Overlay-vertex event columns subdivide the chords, but subdivision points
  lie ON the chords: the region boundary stays the chord polygon, so
  density does NOT remove the crescents.
- A partner edge entering the circle through a crescent (one endpoint in
  the crescent) crosses the exact circle once but the chord polygon ZERO
  times → **no junction is minted on that edge** → the covered outline
  content stays on the kept boundary.
- Stage 4 then relocates the rim ring onto the exact circle
  (`resolve_rim_chord_vertex` → OnCircle), sweeping the on-circle chain
  across the misclassified crescent content → the boundary weaves
  (curved-seam × plain crossings) → Stage-6 non-2-manifold.

## 2. Measured mechanism (F0067, 2026-08-11)

Failing boolean: Extrude-10 auto-union, pair `face_a=328` (gear top,
Extrude-9 gear z 1.6402→1.751898) × `face_b=0` (disc bottom, circle
r=0.20884629067185412), opposite normals, `YANG_STAGE0_DUMP_DIR` overlay
dump `overlay_008_pair328_0.txt`:

- Classes: 1540 AOnly / 795 Overlap / 697 BOnly; ring: 13 `rim_b` samples
  ON-circle + 618 mints (event columns) + 11 rimsnap.
- **126 `corner_a` vertices strictly inside the exact circle, dr =
  −3.089e-4 / −1.081e-3 / −1.339e-3 (three gear-profile radii 0.208537422 /
  0.207765007 / 0.207506897), every one classified `AOnly`.** These are the
  gear tooth root-region profile corners; their extruded wall faces are the
  full-height narrow strips (1–4 triangles, top width 2.6e-4..1.1e-3 ×
  height 0.1117) that the §4.4.1 rim-trim measured as un-removable
  "fragment" holders.
- Downstream census (`YANG_441_RIM_CENSUS` on the same run): the kept A-top
  tooth patch boundary walks flank → v787(dr −1.081e-3, plain, INSIDE) →
  16-vertex on-circle chain (sag 3.6e-6) → v820(−1.339e-3) → v813
  (−3.089e-4) → other flank: the design corners poke through the exact
  chain = the A-top rim-weave; NO junction vertex exists on the entering
  flank edges (v785→v787 class). This is J1-0's "exactly one unrelocated
  ≥3-holder corner per seam" (uniform t, 1.339e-3 design gap) measured at
  its mint.
- The 2026-08-11 rim-trim correctly refuses to remove these vertices
  (their wall-strip holders' cycles would degenerate): they are DESIGN
  vertices, not discretization debris. The removal framing is dead; the
  classification must be fixed where it happens — Stage 0.

## 3. Design — increment 1 (gated)

**Before the overlay is built** (in the general-overlay pair path, after
the §2b/§2c clustering and the amendment-18 rim-table fusion, immediately
before `coplanar_overlay(&poly_a, &poly_b)`): for each pair face that has
rim rings and no curved masks (pure disc / annular), **subdivide rim spans
with exact on-circle samples until no partner chain vertex strictly inside
the exact circle lies strictly outside the chord polygon.**

Per rim ring (disc outer via `disc_circle_edge`; annular outer + each hole
via `annular_disc_face` — the hole predicate is IDENTICAL: a vertex inside
the hole circle but outside its inscribed chord polygon is exactly the
misclassified crescent case):

1. **Feature set:** the partner polygon's chain coordinates (outer + holes)
   EXCLUDING the partner's own rim-sample coordinates (the §2c chain/rim
   domain split — rim samples of a partner disc are on-circle geometry, not
   plane features; congruent-rim identity is amendment-18's job).
   Keep only vertices with `radius − dist > 1e-9·(1+radius)` (the Stage-1
   rim band): a partner vertex within band of the circle is on-circle
   content (junction/tangency machinery owns it), and the floor bounds the
   refinement depth.
2. **Violation predicate (exact):** for span `(s,e)` and feature `q` (all
   `ExactPoint2` rationals): `q` strictly inside the circle
   (`|q−c|² < r²`, center/radius per the `RimChordCtx` convention) AND
   strictly on the non-center side of the chord
   (`sign(cross(e−s, q−s)) = −sign(cross(e−s, c−s))`, both nonzero).
   Consecutive-sample spans subtend < π, so crescents are disjoint and the
   violated span is unique per feature.
3. **Subdivision:** insert the span's azimuth-midpoint sample: 2D at
   `c2 + r·(cos θm, sin θm)` (f64 azimuths, wraparound-correct; a span
   subtending ≥ π or a non-finite coordinate is a LOUD skip → pair_err);
   3D by the SAME radial projection the x-event mints use
   (`center + radius·normalize(lift(u,v) − center)` — on-circle to ULP, in
   the cap plane through the exact 3D center). Iterate rounds (split every
   violated span once per round) to a fixpoint; depth cap 32 → LOUD
   `pair_err` (P10 — never silently accept a residual crescent feature).
4. **Propagation (all four consumers, bit-shared):**
   - `poly` ring: insert the 2D sample in boundary order (the overlay's
     region boundary tightens — membership now agrees with exact for every
     partner feature);
   - `rim` map: `ExactPoint2(u,v) → Point3` (overlay-vertex resolution);
   - `rim_overrides[cap_edge]`: push the 3D point (the Stage-1 ring build
     for the lateral and any other rim consumer inserts it);
   - `rim_overrides[opp_edge]`: push the exact 1:1 opposite-rim image via
     the SAME cylinder-axial / torus-poloidal projection
     `collect_ring_crossings` uses (factored helper), keeping the
     azimuth-merge sample counts matched (the C0048 #143/#144 lesson).
   The refined samples are far from uniform slots (midpoints of existing
   spans), so the #143 uniform-slot merge leaves counts intact.

Out of scope (recorded): MIXED Line+Arc faces (`collect_mixed_crossings`
domain — same defect class, arc-chain analog); the n-ary plane-group path
(`nary.rs` — same classifier, same follow-up shape); partner features whose
clearance is inside the rim band (femto/tangency class — stays with the
junction machinery).

## 3b. Increment-1 measurements (2026-08-11, gate-ON F0067)

- Refinement fires on the flush pair: `[rim-refine] face=0 edge=0
  inserted=57 rounds=3 feats_inside=168 ring_len=70` (13-gon → 70).
- **Membership FIXED at the mint**: overlay dump re-census — all 168
  in-circle `corner_a` vertices are now incident ONLY to
  `BOnly`/`Overlap` triangles (was: 126 `AOnly`); Overlap 795→1179,
  BOnly 697→1049 (the crescents reassigned). The entering flank edges
  now mint their junctions (`OnCircle{crossing:true}` splits observed on
  the (v785→v787)-class edges).
- **First follow-on wall (fixed in-increment): junction femto-trios
  resolve divergently.** The sweep mints ULP-twin columns at each new
  junction (measured trio within 4e-15 in 2D: overlay v576/v577/v583);
  the crossing member resolves to the exact circle∩line junction, the
  column twins take the pure x-event RADIAL projection —
  O(sag·tan(flank tilt)) = 3.3e-5 apart, ABOVE the sub-floor 3D
  collapse key — two 3D points for one semantic arrangement vertex →
  degenerate wall-ring content → loud `build-mesh-triangulate f=390`
  (`UNSUPPORTED(coplanar-boolean)`). Fix (same gate): the shared-mint
  collapse ALSO groups mints whose overlay 2D pre-images are within
  `TAU_WORK·(1+scale)` — 2D-femto-identical mints are ONE vertex, and
  the existing election lets the crossing member's junction win.
- **Named residual (follow-on increment, M-B emission-identification
  class): trio-wedge drop leaves an unpaired resolved edge.** With the
  trio collapsed 3D-identical, the M-B degenerate-wedge drop removes
  FOUR stacked wedges at the corner (measured e8drop census, corner_a
  761: 2 `AOnly` + 2 `Overlap` wedges over verts 1204/1211/1218) — the
  pair-identification contract ("neighbors' resolved edges pair
  directly") holds for twin PAIRS but not for a ≥3-member mixed-class
  cluster: the A operand mesh ships `i6-edge-overuse` /
  `s4-halfedge-pairing fwd=1 rev=0` on the junction↔lift edge
  ((1298,2905)/(2905,3486)-class). F0067 gate-ON therefore lands on the
  canonical ERROR verdict (Stage-6 non-2-manifold), one layer past the
  gate-OFF `s6-planar-loop-nonplanar` (face 388, off-plane 3.435e-5)
  wall, with the defect now scoped to the emission-identification
  logic, not to classification.

## 3c. Increment-2 (2026-08-14): trio-wedge root cause = grouping, not emission

The §3b "M-B emission-identification" attribution was one layer short
(again): the emission contract was fine — the defect was the shared-mint
grouping's 3D admission tier ENROLLING a semantically distinct arrangement
vertex.

- **Measured (F0067 corner_a 761, `YANG_SPLIT_PROBE`):** the trio =
  {1204 crossing, 1211 ULP-twin column mate, 1218}. 1204/1211 are one
  arrangement vertex (2D-femto, 5e-17) — grouping them is the designed
  §3b fix. 1218 is the CORNER-COLUMN chord mint — 2D pre-image 8.9e-6
  away (its own column) whose RADIAL image happens to land 8.5e-7 from
  the junction: inside the `MIN_FEATURE_SIZE` (1e-6) 3D band, so the 3D
  tier enrolled it. Collapsing it onto J re-writes chain topology: the
  cap's 2D-interior edge (1218→1219) resolves onto the boundary segment
  (J→corner), which the wall face also uses as an earcut diagonal of its
  near-collinear top-chain ribbon — FOUR incident triangles on one edge
  (`i6-edge-overuse` (1298,2905), measured). A hand pairing-census with
  1218 left distinct pairs EVERY edge (t1691/t1692 stop being
  bit-degenerate and become the real flank↔arc slivers).
- **Fix (same gate):** `mint_group_admits` (stage0/mesh_build.rs) —
  gate-ON identity is read where it lives: **2D pre-images closer than
  the feature floor (`MIN_FEATURE_SIZE`) are ONE arrangement vertex**
  (subsumes the §3b femto tier), plus a rounding-noise 3D tier
  (`TAU_WORK·(1+scale)`) for the (222,286) coincident-image class.
  Gate-OFF byte-identical (historical 3D sub-floor band, no 2D tier).
  The 2D pre-image is the identity key; 3D proximity beyond rounding
  noise measures DIVERGENCE.
- **Why the 2D tier is floor-width, not femto (measured on R0072):** a
  first cut tightened the gate-ON 3D tier to rounding noise with only
  the femto 2D tier — full gate-ON corpus flagged ONE category delta:
  R0072 SUPPORTED_CORRECT → ERROR (`Stage-4 LocalRefinementRequired`,
  the R0072 micro class re-minted). R0072's model is micro-scale
  (~2e-4 m): its twin mints sit 1.1e-7..9.5e-7 apart in BOTH spaces
  (`YANG_STAGE0_TWIN_SCAN`) and MUST identify — nearly the same
  absolute 3D distance as F0067's 8.5e-7 must-NOT-identify pair, so no
  3D band separates the two cases. The 2D pre-image distance does,
  exactly: R0072 twins ~1e-7 < floor; F0067's corner-column mint
  8.9e-6 > floor. With the floor-width 2D tier both verdicts hold
  (R0072 CORRECT, F0067 advanced).
- **Fold-gate interaction (measured, overlay re-dump):** with 1218
  distinct, the on-circle configuration folds an incident sliver, and
  the N2-3a fold-validity gate reverts the corner's mints to chord lifts
  (`mint(rev)` 4→6) — its designed role: record the demand for §4.4.1
  mesh updating instead of shipping a fold. The emission is CONSISTENT
  (Stage-6 assembles 2-manifold; `NONMANIFOLD_SITE_PROBE` silent). A
  second corner un-groups too (v463/v470, a 2.5e-7 pair): its wedges
  survive as real kept triangles (e8drop 209→207, kept_a/b +1 each).
  Post-change `YANG_SPLIT_PROBE`: every group is a {column-twin,
  crossing} PAIR electing the crossing member; no ≥3 clusters remain.
- **Named residual (next increment):** F0067 gate-ON advances one stage
  deeper — Stage-6 assembles; the RENDER tessellation rejects a gear-top
  tooth ring at a DIFFERENT azimuth (`TessellationFailed FaceId(4005)`,
  "ring rejected by CDT"). Measured (`KV2_RING_REJECT_PROBE` + exact
  census): the 25-vertex ring doubles back 3.6e-5 at
  v21=(0.19862607144761782, 0.06441664730277269) — v21→v22 is exactly
  backward along the incoming flank segment (collinear to 1e-21), and
  (v22,v23) properly crosses (v20,v21). v21 is a Stage-0 flank lift
  BYTE-IDENTICAL pre/post at that tooth; v22 is minted downstream
  (absent from every Stage-0 overlay dump).
  ~~Shape and seam type match the tracked §4.5.3 straight-run reversal
  class~~ — RETRACTED 2026-08-14 (same day, §3d): that adjudication
  rested on an arithmetic slip (v22's radius mis-evaluated as 3e-5
  inside the circle; it is EXACTLY R). §3d has the true anchor.

## 3d. FaceId-4005 ring anchor (2026-08-14): fold-revert ↔ Stage-4 junction
## election inconsistency — NOT a §4.5.3 reversal

Full-stack measurement of the increment-2 named residual (probes:
`YANG_T145_SWEEP_PROBE`, `YANG_V_PROBE`, overlay dump re-census at the
failing tooth, azimuth ≈ 17.9°):

- **v22 IS the exact flank×circle junction** (r = R to the last bit; on
  the flank line to 1e-21). It is arrangement vertex 1482 — Stage-4's
  junction authority relocated overlay chord mint v1553 (on-circle,
  1.2e-5 azimuthally away) onto the exact junction. The intersection
  loop through it (1482 → 1478 → 1474, all ON-circle) is MONOTONE in
  circle parameter — the §4.5.3 sweep tests these sites (t145-arm
  probe: `reversed=false`) and is CORRECT to leave them. The s453
  straight-run trackers (R0072/F0045) are a different mechanism; no
  sweep extension is warranted here (branch-5 coincident-pair
  undiagnosability is not even reached).
- **A's flank-edge split chain has NO junction vertex.** The flank×chord
  crossing trio at this tooth ({1538, 1539, 1545}) minted the junction —
  then the N2-3a fold gate REVERTED it to chord level (`mint(rev)`,
  r = 0.2087873; the splits on edge (809,810) carry the reverted
  position TWICE at t ≈ 0.80488). The outline chain therefore cuts at
  chord level.
- **v21 (= overlay v1552) is a crescent lift**: a design-corner event
  column (corner_a 803, u = 0.0644166473027744) crosses the flank at
  r = 0.2088105 — strictly inside the exact circle, strictly outside
  the chord polygon (local chord 0.2087873) — so it classifies `AOnly`
  and the surviving A-top patch keeps flank content PAST the junction.
- **The defect**: two mechanisms disagree about the tooth's corner
  state. The Stage-0 fold-revert puts A's outline world at the CHORD
  (consistent locally — the y ≈ −0.034 tooth ships this state and
  passes); Stage-4's junction election still installs the EXACT
  junction on the seam (v1553 → J). The output ring then stitches
  outline-at-chord (through v1552) to seam-at-junction: a 3.6e-5
  backward tack → self-intersecting ring → the render CDT correctly
  refuses. The consistent states are (a) everything local at chord, or
  (b) everything at circle+junction — (b) is the §4.4.1 mesh-updating
  epic (#169, `specs/yang_n2_stage4_cdt_mesh_updating.md`); the
  RESIDUAL here is that the fold-revert does not extend to (or inform)
  the neighboring span mints and the Stage-4 junction election.

**P10 record — census-loop refinement DISPROVEN (do not retry):** a
post-overlay census loop (feed every overlay exact vertex to
`refine_rim_membership`, rebuild the overlay, iterate) DIVERGES
structurally: every inserted on-circle sample spawns a new event column
whose partner-edge crossing lands in the residual (quartered) crescent —
measured on F0067 Extrude-5's pair as +2 features per round, ring
15→18→20→22→24→26→28→30→32 with no fixpoint above the band floor
(≈12+ rounds away). This is the recorded lesson *density is not
membership* playing out mechanically: crescent content is
self-regenerating under subdivision, because a partner edge crossing the
circle ALWAYS traverses some residual crescent. Reverted same-day,
never landed.

## 3e. Increment 3 (2026-08-14, third session): the fold gate's Fig-11(b→c)
## merge arm could not SEE the boundary-exit corner — twin-mid backtrack walk

The §3d inconsistency is resolved at its root, in the all-junction
direction (#169's target state), by extending the EXISTING Fig-11(b→c)
merge arm's candidate recognition — no new mechanism, the paper's own
operation ("if an endpoint p of the split edge is too close to q, we
merge p with q", Fig. 11(b→c)).

Measured ladder at the failing tooth (YANG_SPLIT_PROBE, pair (328,0),
azimuth ≈ 17.9°): the crossing trio groups to ONE on-circle junction
mint 1545 = J (§3c grouping; 1538 bit-twin, 1539 lift-absorbed); tri
2234 (1551, 1552, 1545) folds (area −1.3e-7); the flip arm rejects
(edge (1552,1545) domain-boundary, the others' replacements invalid);
the singleton cavity is non-simple; the region grow's final 9-ring has
crossing pair e5 = (1560→1552) — the FLANK, domain boundary,
ungrowable — × e8 = (1538→1530), intersection-curve edge, ungrowable
⇒ reject ⇒ amendment-2 revert (the §3d `mint(rev)`). But the ring
BETWEEN the crossing edges reads e6 = (1552→1545) — p = 1552 the
crescent lift, unminted + mergeable, 3.6e-5 from q = 1545 — then e7 =
(1545→1538), a ZERO-LENGTH bit-twin edge from the §15 collapse. That
is the canonical Fig-11(b) backtrack sandwich, invisible to
`fig11_backtrack_pair`'s 4-gon walk only because the degenerate twin
edge spreads the crossing edges 3 apart.

**Change 1 — twin-mid sandwich walk** (`stage0/reloc.rs`,
`fig11_backtrack_pair`, new `twin_mids: bool` param): under the gate,
an inter-crossing span qualifies iff it contains EXACTLY ONE
non-degenerate edge; bit-identical-endpoint edges are skipped. The mid
edge's single-mint requirement, the split-edge line, and the overshoot
computation are unchanged (skipped twins are bit-equal to p's flanking
crossing edge's endpoint). Wedge call sites pass `false` (unchanged);
the region-ungrowable site reads the gate. Gate-OFF the walk is the
verbatim 4-gon form. Unit tests ×4 (4-gon flag-agreement, twin-mid
recognized only under the flag, two-live-mids refusal, mirrored
orientation).

**Change 2 — sagitta lookup through the collapse representative**
(`stage0/mod.rs` merge arm): the mirrored tooth's q = 1549 is a §15
LIFT-ABSORBED group member — `minted_info` has no entry for it, so the
containment guard read `sagitta=None` and refused the merge. Under the
gate the lookup falls back to any `minted_info` entry at bit-equal
resolved coords (the group representative 1542 carries the slot).
Env-gated: the historical gate-OFF reject stays byte-identical.

**P10 record — the one-sided intermediate state was SUPPORTED_WRONG,
and the in-line composition oracle caught it.** With change 1 alone,
the +x tooth's merge fired (ring reject GONE) while the mirrored
tooth's merge still refused on `sagitta=None` and reverted — the −x
tooth's §3d inconsistency, no longer masked by the +x tooth's loud
CDT rejection, shipped as 2 zero-area collinear slivers along the
flank line through J' (measured with the new `ASSAY_DEGEN_PROBE`
harness probe: tris (2648,2649,2650) and (5250,5251,5255), heights
1.4–1.8e-9). Verdict F0067 SUPPORTED_WRONG (2 of 4492 degenerate) —
never committed; change 2 closes it. LESSON: a mirrored-pair defect
fixed on one side converts the other side's loud STOP into silent
degeneracy — run the composition oracle before trusting any
single-site advance.

**Verification (full corpus, canonical binary, pinned environment):**
gate-OFF 258C/0W/50E/0T with the committed results.json BYTE-IDENTICAL
(git-clean after the run); gate-ON 259C/0W/49E/0T — the category
delta is exactly ONE (F0067 ERROR → SUPPORTED_CORRECT) and the only
detail drifts are the previously-anchored gate-ON pair (R0015 vertex
84→82 renumber on the same OffCurveBeyondChordBand wall; R0050
LabelMismatch → Stage-4 LRR one stage deeper). yang-rs suite green;
fmt/clippy clean.

**Measured end state (both changes): F0067 gate-ON =
SUPPORTED_CORRECT (95.7s), all oracle checks pass.** Both
boundary-exit merges fire ([fold-merge] p=1552→q=1545 and
p=1557→q=1549; gap 3.61e-5 ≤ disp 5.94e-5; overshoot ≈ 3e-18 ≤
sagitta 6.18e-7); the trios KEEP the exact junction (no revert at
either tooth); outline and seam agree on all-junction — the §3d
consistent state (b). Additional harvested merges: pairs (272,0) ×2
and (293,1) ×2 accept region-derived candidates with overshoot = 0;
the y ≈ ±0.034 Overlap-class corners refuse on the unchanged guards
(mergeable=false / gap > disp / overshoot > sagitta) and revert as
before — that family's all-chord state ships exactly as it did.

## 4. Verification plan

- Unit: refinement function on a synthetic ring+partner (corner in a
  crescent → violation cleared, ring stays on-circle and ordered, override
  counts cap=opposite; on-chord-exact feature untouched; depth cap fires
  loudly on a sub-band-engineered fixture... excluded by the band floor —
  cap test via a contrived deep-crescent instead).
- F0067 gate-ON single case: overlay dump shows the 126 corners
  reclassified `Overlap`; flank junctions minted (crossing overrides on the
  entering flank edges); downstream weave gone or verdict advanced.
- Full corpus assay gate-OFF (byte-identical to canonical 258C/0W/50E/0T)
  and gate-ON (censused; any delta anchored before flip).

## 5. Status log

- 2026-08-11: spec written; mechanism measured (§2). Increment 1
  implementation begun (gated).
- 2026-08-11 (same session): increment 1 LANDED gated —
  `refine_rim_membership` (`stage0/rim_chords.rs`; exact violation
  predicate, band floor, orientation-aware azimuth-midpoint
  subdivision, depth cap 32 loud, cap+opposite override propagation via
  the factored `opposite_rim_image`) + the pair-path call site
  (`stage0/mod.rs`, both faces, chain/rim domain split) + the 2D-femto
  shared-mint grouping extension (same gate). Unit tests
  `tests_unit/m8_rim_refine.rs` (4). Measurements in §3b; the M-B
  trio-wedge residual is the NAMED next increment.
- 2026-08-14: increment 2 LANDED (same gate) — the trio-wedge root
  cause re-anchored one layer upstream to the grouping admission (§3c);
  `mint_group_admits` factored (2D floor tier + 3D rounding-noise tier,
  R0072-corrected) + 5 unit tests. F0067 gate-ON retires the Stage-6
  non-2-manifold wall; the named residual is the §4.5.3 straight-run
  reversal (CDT ring rejection, FaceId 4005).
- 2026-08-14 (second session): the FaceId-4005 residual FULLY ANCHORED
  (§3d) — the s453 adjudication RETRACTED (v22 is the exact junction;
  the seam loop is monotone and the sweep is correct); the true defect
  is the fold-revert ↔ Stage-4 junction-election inconsistency at
  boundary-exit corners, which belongs to the §4.4.1 mesh-updating
  epic (#169). Census-loop refinement attempted, measured DIVERGENT
  (+2 features/round self-regeneration), reverted same-day — P10
  record in §3d.
- 2026-08-14 (third session): increment 3 LANDED (same gate, §3e) —
  the §3d inconsistency fixed in the all-junction direction by
  extending the fold gate's own Fig-11(b→c) merge recognition
  (twin-mid backtrack walk + sagitta lookup through the collapse
  representative). F0067 gate-ON: ERROR → SUPPORTED_CORRECT (the
  epic's first gate-ON corpus conversion of this case). One-sided
  intermediate measured SUPPORTED_WRONG and closed same-session (§3e
  P10 record). `ASSAY_DEGEN_PROBE` added to the harness oracle.
