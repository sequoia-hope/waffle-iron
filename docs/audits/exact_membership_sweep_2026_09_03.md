# Exact-membership sweep of the assay corpus — 2026-09-03

**Instrument:** `test_harness::assay::exact_membership` — every corpus
document as a closed-form point predicate (extrude / revolve of polygon,
circle and involute-gear profiles; bosses union, cuts subtract, in feature
order, with the feature engine's own semantics for direction, symmetric and
second-direction depths, through-all depth and cut auto-reversal), read on
a cubical lattice laid in the document's first sketch frame. No
tessellation and no kernel anywhere in the reading. Landed with spec
`yang_451_corner_transit.md` §3ah after the R0053 adjudication; tests in
`crates/test-harness/tests/assay_exact_membership.rs` (pinned adjudications
+ the `corpus_sweep` / `one_case_ladder` instruments); companion probe
`s453_r0053_output_obj.rs` dumps the kernel's result, a chain prefix, or a
single operand (a cut's tool as a boss) to OBJ.

**Why it matters.** Every other reference the harness has goes through a
mesh: the composition oracle compares the kernel's output against
kernel-v2 tessellations of the operands (and is SKIPPED on cut chains),
the Euler oracle reads the output mesh, the sidecar unions tessellations.
Two of this session's findings are invisible to all of them: a wrong
revolve wedge that is watertight and genus-0, and cuts that remove nothing
(the categorized runner never calls `check_volume_monotonicity`). The
exact predicate sees both.

## Coverage and scope

- Covered: 309 / 312 cases. Not covered (typed): C0084 (`BooleanCombine`),
  C0097–C0100 region extrudes (`params.regions`) — 4 features; C0100 is
  the "two adjacent sub-regions as one body" case.
- **Lattice scope.** A reading is the solid's topology when every rung
  agrees at cell sizes below the thinnest feature and the narrowest gap.
  The lattice must be laid in the model's frame: an oblique lattice
  perforates frame-aligned thin features at every cell size (R0053 read
  χ ∈ {+3, −3, −6, −8} at h = 1.04…0.37 in the world frame and 0 at every
  rung from h = 2 to 0.3 in the sketch frame). Features that TAPER to zero
  thickness (crescent pillars, knife edges) cannot be pinned at any cell
  size (R0091's pillars: genus 1 / 5 / 1 at 256 / 512 / 1024 cells);
  sub-cell gaps speckle a slab into cavities (R0063's 5.64e-6 gap reads
  χ_solid 295…−3508 until h < 2.8e-6, then 0). The VOLUME converges far
  earlier than the topology (R0091: 4.2624e-12 ± 0.01 % across all rungs).
- The corpus's near-degenerate C-series (1e-3 walls, 1e-4 floors) is below
  the sweep's 256-cell rung; their χ rows below are lattice-limited, not
  findings.

## Kernel volume against the exact volume (`EXACT_KERNEL=1`, 256 cells)

The full sweep (32 min, 229 cases with a completed kernel result; 36 had
none — the ERROR cases). Rows with |rel| below ≈ 2 % are within the
lattice's own volume error on thin-toothed profiles at 256 cells and are
not findings without a finer rung. Worst 25 by |rel|, classified:

| rel | cases | reading |
|---|---|---|
| +∞ | R0027, R0088 | exact EMPTY (class C: the cut tool contains the body); the kernel keeps material |
| +2.6e304 | C0117 | a 1e-4 curved tube wall: the exact solid is below the 256-cell lattice (lattice-limited, not a finding) |
| −0.87 / −0.65 | R0045, R0096 | class A (the > 180° band's complement), see the classes below |
| −0.80 / −0.80 | C0074, C0081 | open — C-series complexity cases; adjudicate per case (cut semantics vs my bbox mid-extent rule, or thin features) |
| −0.54 / −0.46 / −0.20 | C0098, C0099, C0097 | my parser read the first of two crossing/concentric loops; these are REGION extrudes — now typed NotCovered (multi-profile sketches) |
| +0.36 | R0034 | class C |
| −0.25 / −0.15 / −0.12 / −0.12 / −0.09 | C0091, C0040, C0092, C0095, C0096 | open — C-series; per-case |
| +0.063 | R0058 | open — gear teeth at 256 cells; needs a finer rung |
| ≤ 0.017 | C0035, C0039, F0042, C0023, F0089, C0077, F0002, C0024, C0075 … | within the lattice band |

(R0091's −0.67 sits just outside the top 25 because its exact volume
includes the sausage; it is the anchored class-A case.)

| case | kernel / exact − 1 | chain | status |
|---|---|---|---|
| R0045 | −0.869 | revolve(circle) boss ∖ revolve(rect) cut | **KERNEL WRONG** (class B below): boss and tool each match the kernel's own meshes in volume AND centroid; they are ≈ 1e-2 apart at 5e-3 scale (exact intersection ≈ 0), yet the kernel keeps a 7 % fragment of the boss at a third location. SUPPORTED_CORRECT in the corpus (watertight, χ = 2, volume > 0). |
| R0091 | −0.668 | revolve(circle) boss + extrude box + extrude(circle) cut | **KERNEL WRONG** (class A below): the untouched revolve body comes back as the COMPLEMENTARY 140.6° wedge of its 219.4° sweep after the cut (2.541e-12 vs 3.966e-12 by Pappus; intact after the union, flipped by the cut). SUPPORTED_CORRECT in the corpus. |
| R0096 | −0.649 | revolve(circle) boss ∖ revolve(circle) cut | **KERNEL WRONG** (class B): tool matches the kernel's (8.93e-7 vs 8.95e-7); exact removes 18 %, the kernel 71 %. |
| R0034 | +0.361 | box boss ∖ circle cut (tool contains the box) + gear revolve | **KERNEL WRONG** (class C): the cut removes NOTHING in the kernel (result after 2 ops = the intact box, 1.5233e7 exact) where the tool — a 288.6-radius, 695-deep cylinder on the base plane — contains the 209 × 235 × 309 box entirely; the authored monotonicity says decrease. |
| R0007, R0027, R0088 | (exact result EMPTY) | cut tools containing the whole body | class C: the exact chain is empty after the cut, the kernel completes with material. Each needs the same anchor as R0034. |
| R0058 | +0.063 | gear boss ∖ gear cut + gear boss | open — lattice error on gear teeth at 256 cells is a few %, so this needs a finer rung before it is a finding. |
| R0075 | +0.069 | gear boss + circle boss ∖ gear cut | open — as R0058. |
| R0009, F0042, F0002, F0010, R0020, R0031, F0012, R0039, R0094, F0005, R0078 | 0.5–1.8 % | mixed | within the lattice band at 256 cells; not findings. |

## Classes

- **A — boolean-output torus band takes the principal-branch longitude
  interval.** Anchor: yang-rs `stage1_tessellate/patch_tessellators.rs`,
  `tessellate_torus_patch`, the two-meridian-wrapping-loop ("band") case:
  each rim's longitude is inverted with `atan2` into (−π, π] and the ribbon
  is laid between the two values as they come, so a band spanning more
  than 180° is tessellated as its complement. Only the boolean-OUTPUT
  patch path is affected (the modeling lateral is structured and correct
  — the isolated revolve reads 3.960e-12 vs Pappus 3.966e-12). The side
  is determined by orientation, not span: kernel-v2's loops wind
  material-CCW about the outward normal (`validate/faces.rs`, the
  unrolled-winding rule for cylinder bands: the +1 wrap at the lower
  height), and the torus chart `(u = meridian, v = longitude)` with
  `(e1, e2, axis)` right-handed has `∂u × ∂v = −(R + r cos u)·r·n_out`, so
  a material-CCW loop appears CW in the chart and the band lies at
  DECREASING v from the +1-wrapping rim (mirrored for `reversed`). The
  fix is to unwrap `mc`'s longitude onto that side of `pc` before the seam
  bridge. **FIXED the same day** (`specs/yang_torus_band_side.md`): the
  consumer takes `reversed`, shifts `mc` by whole periods onto the
  orientation-dictated side; R0091's revolve reads 0° → 219.4° again and
  the three-op volume moves 1.957e-12 → 4.219e-12 (exact 4.262e-12).
- **B — (collapsed into A)** R0045's and R0096's outputs occupy the
  angular stations 280° → 360° and the tail of 281° → 360° about their
  boss axes: the boss's > 180° torus band rendered as its complement,
  exactly class A — the subtract itself was not at fault. Re-measured
  after the fix in the roadmap entry.
- **C — a cut whose tool contains the whole body removes nothing.** R0034
  anchored at the engine+kernel level (result = the intact box); R0007,
  R0027, R0088 by the exact chain reading EMPTY. Localised past the feature
  engine: `YANG_RUN_PROBE=1` on R0034's two-op prefix shows yang-rs
  running `op=Subtract a: 8v/6f b: 2v/3f` — the box against the
  cylinder, un-reversed — and returning the box intact, where A ⊂ B has
  the empty set as its answer. So the defect is yang-rs's / the labeling
  stage's treatment of an arrangement with NO intersection curves (the
  in/out classification of A's single patch against B, Cherchi 2022 §5).
  **RESOLVED the same evening — and NOT a kernel defect.** The synthetic
  box-inside-cylinder (and box-inside-box) subtract through kernel-v2
  returns `EmptyBooleanResult` correctly (`crates/kernel-v2/tests/
  containment_subtract.rs`, four pins). modeling-ops turns that into
  "cut consumed the entire target body" with zero outputs (spec
  `cut_consumes_body` §3), and the FEATURE ENGINE's most-recent-body walk
  (`find_most_recent_solid_outputs`) then stepped past the consuming
  feature to the consumed body's own feature and RESURRECTED it: R0034's
  gear revolve auto-unioned with the pre-cut box, R0007's second cut
  re-cut the cylinder, R0058's third boss merged with the consumed gear.
  Fixed by threading `already_consumed` into the walks (spec
  `cut_consumes_body.md` §7). Post-fix: R0034 4.126e7 vs exact 4.152e7,
  R0058 −0.2 %, R0023 −1.1 %; R0007 / R0027 / R0088 error loudly (no
  body left to cut) and their `expect_rebuild_error` is corrected to
  `true` (adjudicated by the exact chain reading EMPTY after the
  consuming cut). Corpus: 276C/0W/31E/1EE/0T → **273C/0W/31E/4EE/0T**,
  exactly those three cases moving.

## Topology rows (exact-only sweep, 64 / 128 / 256 cells, sketch frame)

57 unstable ladders and 50 boundary-χ disagreements at these rungs. The
bulk are (i) disjoint-body chains where the authored `euler_target = 2`
names one shell (the exact reading is 2 × components: R0001, R0004,
R0014, R0030, R0043, R0055, R0061, R0062, R0066, R0068, R0069, R0074,
R0075, R0083, R0084, F0011–F0014, F0058, F0060, C0051, C0080, C0082,
C0094), (ii) the near-degenerate C-series below the rung (C0029–C0040,
C0111–C0113 read genus 0 where a 1e-3 wall makes the authored genus 1 —
C0029's kernel and exact VOLUMES agree at 0.8005 vs 0.8008), and (iii)
the class-C empties (R0007, R0027, R0088, C0035). Left for adjudication
with finer rungs: R0006 (exact 2 vs authored 0), C0065 (−2 vs 2), C0074
(2 vs 4), C0092 (2 vs −4), C0095 (0 vs −2), C0107 / C0108 (2 vs 4),
R0026 (an ERROR case: exact genus 1 vs authored 0).


## Addendum 2026-09-04 — the open rows adjudicated; bodies, holes and combine modes modelled

Every "open" row above was the ORACLE's scope, not the kernel's: the two
−80 % rows (C0074, C0081) are combine-mode `Intersect` extrudes the
predicate read as unions, and the −9…−25 % rows (C0091, C0092, C0095,
C0096) are holed profiles read as their outer loop. The oracle now mirrors
the engine's body model exactly (`exact_membership.rs`, module docs):

- **Holed regions** as the kernel adapter stages them
  (`make_faces_from_profiles`, KV14): one face per input profile; an
  `is_outer` loop takes every inner loop whose centroid it contains and
  whose area it strictly exceeds (each hole to the SMALLEST such outer); an
  inner loop's own index stages that loop alone (C0094's second body).
- **Combine verbs** (`normalize_combine`): legacy `cut` / `merge` /
  `target_body` → `Cut` / `Add` / `NewBody` on the most recent
  solid-bearing feature's bodies; explicit `combine` + `targets`
  (feature-output anchors — a body answers to every feature that produced
  or last modified it: C0074's `Intersect` targets its own cut).
  `Add` folds the tool with the targets it touches and re-emits targets
  whose box misses the tool's as this feature's LEFTOVER bodies (the
  engine's disjoint-merge rule); `Cut` / `Intersect` act per target;
  share-a-face auto-targeting (`combine` set, no `targets`) is typed out.
- **Per-body readout**, summed — the kernel emits one mesh per body and
  the runner reads their concatenation, so two overlapping `NewBody`
  bodies count their volume twice (C0083's authored 1.64). The sweep's
  kernel-side scan does the same now; it used to scan the concatenated
  soup, which read C0083 as the set union (1.4005).
- **The first-target rule.** The engine decides a cut's auto-reversal and
  a through-all depth from the vertices of its FIRST combine target
  (`rebuild.rs`: `combine_targets.first()`, `find_most_recent_solid`).
  R0075 showed why this matters: its gear cut follows an `Add` whose circle
  boss stayed disjoint from the gear boss, so the most recent feature's
  outputs are `[circle, gear]` and the engine measured the circle alone
  (`FE_CUT_TRACE=1`: `target verts=2 … reverse=true`); the reversed tool
  never reaches the gear boss it overlaps and the kernel correctly removes
  nothing. The oracle had measured the merged extent, did not reverse, and
  read a 6.6 % deficit (2.5818e6 at 512 / 1024 cells vs the kernel's
  2.7610e6); with the engine's rule it reads 2.7252e6 at 256 cells, +1.3 %
  — the gear-teeth lattice band. R0091's pinned volume moved the same way
  (4.2624e-12 → 4.2232e-12; the engine measures the box, `target
  verts=8`; the kernel's 4.2190e-12 is 0.1 % from it). When a fold's
  tool-alone extent would flip the decision, the parse notes it.

  *Engine observation, not a kernel defect:* a legacy cut after a disjoint
  merge takes its direction from the merge's TOOL body, so it can miss the
  older body it overlaps (R0075). Whether that is the intended policy is a
  feature-engine question; recorded here, not changed.

### The rows, re-read (kernel = tessellated output at the oracle tolerance, scanned per body at 256; exact at 256 cells unless noted)

| case | kernel | exact | rel | reading |
|---|---|---|---|---|
| C0074 | 2.5158 | 2.5059 (2.5538 / 2.5179 / 2.5059 at 64 / 128 / 256) | +0.4 % | `Intersect` targeting the cut's output; χ = 4 = authored (one component with a cavity) |
| C0081 | 0.33579 | 0.33600 (all rungs) | −0.06 % | `Intersect`; authored 0.336 |
| C0091 | 1.5000 | 1.5000 | 0 | annular square, χ = 0 |
| C0092 | 2.0977 | 2.0988 | −0.05 % | three holes, χ = −4 |
| C0095 | 3.9245 | 3.9433 | −0.5 % | hole + through-cut, χ = −2 |
| C0096 | 2.4460 | 2.4364 | +0.4 % | L outer with a hole, χ = 0 |
| C0040 | 1.0000e4 | lattice-limited | — | a 1-thick slab over 100 units is 3–5 cells thick at every rung (1.56e4 / 7.8e3 / 1.17e4); by hand 1e4 − 1e-6, the 1 mm hole below any cubic lattice — not a finding |
| C0079 | 2.4980 | 2.5000 | −0.08 % | `Add [A, B]` fold, one body |
| C0080 | 2.9198 | 2.9061 | +0.5 % | three bodies, explicit `Cut` pockets B only |
| C0082 | 2.1798 | 2.1763 | +0.2 % | two bodies |
| C0083 | 1.6400 | 1.6515 | −0.7 % | two overlapping `NewBody` bodies, summed |
| C0094 | 1.2000 | 1.1976 | +0.2 % | two bodies from one sketch (profile 0 and 1) |
| R0058 | — | 1.4262e-2 / 1.4183e-2 / 1.4281e-2 at 256 / 512 / 1024 | ±0.7 % per rung | gear-teeth band; the −0.2 % kernel row stands, not a finding |
| R0075 | 2.7610e6 | 2.7252e6 (first-target rule) | +1.3 % | see above; the exact set has two components, the disjoint circle boss (17 849 of 267 432 cubes) |
| R0091 | 4.2190e-12 | 4.2232e-12 (box 2.586e-13 + sausage 3.9646e-12) | −0.1 % | pin corrected |

### Topology rows left open above

- **C0065** (an ERROR case: the cut fails at Stage 4,
  `OffCurveBeyondChordBand`): the authored "through-notch severs the ring,
  χ = 2" is wrong. The tube spans radius 0.9…1.5 and the 0.5-wide block
  only 0.95…1.45 (and 0.98…1.47 at its y-edges), so the block WINDOWS the
  tube and leaves an inner and an outer bridge — genus 2, boundary χ = −2,
  one component, stable at 128 / 256 / 512 cells on phases ½ and ¼ (volume
  2.0005; the slivers are the 0.01 the notch cannot take). `euler_target`
  corrected 2 → −2 and pinned (`assay_exact_membership::
  c0065_reads_genus_two`, `assay_euler_consistency`) so a future conversion
  is scored honestly.
- **R0026** (ERROR): genus 1, one component at 128 / 256 / 512 on two
  phases (volume 4.998e-4 converged); the generator's default 2 was never
  adjudicated. Corrected 2 → 0 and pinned.
- **R0006**: the exact set has two components (the holed box, χ = 0, and
  the disjoint circle boss, χ = 2 — total 2 against the authored 0). That
  is the runner's own rule: `check_mesh_euler_characteristic_with_shells`
  credits every shell beyond the authored count +2, so a disjoint boss
  never trips it. This explains the whole "(i) disjoint-body chains"
  family above — consistent, not a finding.
- **C0107 / C0108** (a sphere point-tangent to a cylinder / two tangent
  spheres): a 0-D contact bridges the lattice at every cell size (one
  component, χ = 2 at 128 / 256 / 512); the authored two shells is the
  B-Rep convention. Outside the lattice's scope.
- **R0075**'s two components are the disjoint circle boss (above).

### Coverage and the re-sweep

Exact-only sweep (64 / 128 / 256 cells, 116 s): covered 309 / 312; not
covered, typed: F0074 (a revolve whose axis is not in its sketch plane),
C0084 (`BooleanCombine`), C0100 (a region extrude).
The multi-body C-series cases (C0079–C0083, C0094) and the holed profiles
are now in. 57 ladders are unstable at these rungs (the tapered-feature /
sub-cell-gap / thin-tooth population of the scope note; the finer-rung
pins above are where a reading is claimed), 48 boundary-χ and 40
component-count rows disagree with the authored oracles — the (i) disjoint
bodies (the runner's +2-per-shell rule), (ii) the sub-rung C-series and
(iii) the R0007 / R0027 / R0088 / C0035 empties, as before. The
kernel-vs-exact table is re-run below.

### The oracle in the categorized runner (2026-09-04, later the same session)

`assay_kv2::categorize` now runs `exact_volume_verdict` on every covered
case: the kernel's result volume (signed volume of the tessellated live
bodies at `oracle_tol`, summed) against the exact chain at 256 cells.
Cut chains are covered, unlike the composition oracle.

**The band is the reading's own uncertainty.** Two designs were refuted on
the corpus first. (1) A 128 → 256 rung step (Richardson, ×2) plus a 0.5 %
floor flagged 14 CORRECT cases at 0.8–2.0 % (R0020, R0031, R0071, R0072,
R0092, F0010, F0090, C0023, C0035, C0039, C0075 and the three region
cases): thin features quantise with the lattice PHASE, not only its pitch
— R0031 reads 2.8810e-5 at phase ½ and 2.8204e-5 at phase ¼ on the same
256 rung (2.1 %) while its rung step is 0.02 %; and a thin slab rounds
to the same layer count at every rung and phase — C0039's 0.1-thick slab
on a 1-unit footprint is 12.8 / 25.6 cells and reads 13 / 26 layers,
+1.56 %, at 128 and 256 on both phases (the kernel's 0.1000004 is exact).
(2) Adding the phase difference to the step would have left C0039 flagged.
What covers both is the volume of the lattice's SURFACE cubes (occupied
cubes with an empty face neighbour — the cubes the boundary decided):
`band = surface_cubes · h³ / V + 5e-3`. A reading whose surface cubes
exceed a quarter of its volume is too coarse to author an expectation
(C0040's 1 mm hole in a 100-unit slab, C0117's 1e-4 tube wall, the
1e-3-wall C-series); a chain whose cut auto-reversal the document leaves
indeterminate (the sketch plane at the target's mid-extent, margin below
1e-9 relative: C0086, F0058, F0060, R0003, R0020, R0070) is declined; so
is anything the parser types out.

**Region extrudes** (`params.region` — the singular field; C0097–C0099)
are the engine's own footprint polygon with its holes
(`profile_footprint_2d`), now read from the region: C0097's annulus
1.2505 vs kernel 1.2513 (genus 1 = authored), C0098 0.3691 vs 0.3703,
C0099 0.4329 vs 0.4330.

**Adjudicated on finer ladders, none a finding:**

| case | kernel | exact | reading |
|---|---|---|---|
| R0009 | 1.9600e-16 | 1.77 / 1.96 / 2.06 e-16 at 256 / 512 / 1024 (φ ½), 1.82 / 2.16 / 1.97 (φ ¼) | a fragmented sub-lattice solid (4–6 components, scale 1e-4); the kernel sits inside the spread; both reversals match the engine (`target verts=2/4`, both `reverse=true`) |
| R0055 | 2.1955e3 | 2.117 / 2.204 / 2.174 e3 (φ ½), 2.232 / 2.173 / 2.181 (φ ¼) | converging to ≈ 2.18e3, kernel +0.7 % |
| R0078 | 2.0841e-5 | 2.0469e-5 and 2.0462e-5 at 1024 on both phases | the remainder is 13 % of the box; the kernel's inscribed 140-gon tool (1.7367e-4 vs π r² d = 1.7387e-4, −0.12 %) leaves 2.1e-7 more material = +1.0 % of the remainder; the rest is the strips' quantisation (1.2e-3 wide, 9 cells at 1024) |
| R0099 | 2.9004e1 | 2.9104e1 / 2.9106e1 at 1024 on both phases | −0.35 % |

**Proof of zero false flags:** the full categorized corpus under the
oracle (release, 8 jobs, 360 s; F0065 172 s, F0085 304 s honest) is
BYTE-IDENTICAL to the canonical 273C / 0W / 31E / 4EE / 0T. The oracle's
cost is negligible against the kernel's (F0090: 132 s with and without
it). The sweep's silent-wrong classes (7 %–∞) can no longer pass as
CORRECT; a defect below the lattice's band (C0065's 0.5 % slivers) is the
topology oracle's to catch, with the corrected `euler_target`s.

*Open observation (not adjudicated):* the OBJ probe's tessellation at the
oracle tolerance shows boundary edges on three canonical-CORRECT curved
cases (R0091: 30, R0009: 160, R0099: 11) where the runner's coarser
`tess_tol` mesh is watertight. Whether that is a tolerance-dependent
tessellation defect is a question for a later session.

### C0063 — an authored expectation the document cannot produce (2026-09-04)

Found while re-censusing the `UNSUPPORTED(curved-profile)` walls (spec
`yang_stage1_curved_holed_patch.md`): C0063 ("full cone + oblique slab cut,
conic-bounded patch, χ = 2") reads EMPTY by exact membership at 128 and 256
cells on two phases; the cone alone reads 0.8045 (π·0.8²·1.2/3 = 0.8042).
The engine decides the cut's auto-reversal on the cone's single B-Rep vertex
(`FE_CUT_TRACE`: `target verts=1 proj=[0.24, 0.24] sketch_proj=1.199
reverse=true`), which lays the 2 × 2 × 1.5 slab back through the whole cone
(0.24…1.145 along the slab normal against the slab's −0.30…1.20; footprint
covered). The kernel never gets there today — the apex-cone operand refuses
at Stage 1 — so nothing scores wrong; when that wall falls the meta must be
re-authored. Recorded, not changed.
