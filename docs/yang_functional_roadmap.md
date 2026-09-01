# Yang Functional Roadmap — Single Source of Truth

> **Status:** authored 2026-05-28. This document supersedes the per-crate
> `PLAN.md` concept for the Yang effort (there are none in the new crates) and
> supersedes the stale "Current architecture (what's built)" block that used to
> live in the root `CLAUDE.md`. When this roadmap and a crate `CLAUDE.md`
> disagree on sequencing, this roadmap wins; when this roadmap and the Yang 2025
> paper disagree on *algorithm*, the paper wins (see `docs/yang_deviations.md`).

## 0. Honest status (refreshed 2026-06-26)

The kernel rewrite is **live in the app**. The legacy `crates/kernel/` is
DELETED; the app, feature-engine, and all tests run on `kernel-v2` through the
`Kernel`/`KernelIntrospect` traits, and the WASM bundle builds on **stable
Rust**. M0–M7 are COMPLETE, the Phase-6 migration shipped (2026-06-11), and the
curved-geometry stack (cylinder/cone/torus, partial+full revolve) is in
production. The native `cherchi-rs` Stage 2 (arrangement + boolean labeling +
clean-room indirect predicates, WASM-clean) is the production backend; the C++
sidecar and the LGPL IP FFI are dev-only parity oracles. `ssi-rs` drives the
exact analytical intersection curves.

So the foundations are deep and the boolean works across a broad range of real
geometry. **What is NOT yet faithful to the paper is the remaining set of Yang
deviations** (`docs/yang_deviations.md`) — and per the project's standing rule
("Paper-Spec Compliance is MANDATORY; deviations are errors"), closing those, not
chasing a score, is the work.

### 0.-1 ADDENDUM 2026-08-08 — the 5 oracle flags are ALL kernel silent-wrongs; priority reordered

The independent volume oracle's five set-level flags were anchored
(`docs/audits/volume_oracle_flags_anchored.md`) and **every one is a kernel
defect**: R0090 (−58%) and R0030 (−30%) lose the TARGET BODY at a `merge:true`
union with a (near-)disjoint tool — the correct answer is two lumps
(`split_solid_into_bodies`), the measured output is ONE lump equal to the
tool; R0040/R0057/R0059 (−2.8/−1.3/−1.0%) lose material at REVOLVE-union
steps (operands certified by divergence×winding×Pappus; deficits
tessellation-invariant). Masks that let them grade CORRECT: the categorized
runner never checks meta `volume_monotonicity`; `properties_v2` downgrades
I9–I12 failures to advisory passes; the merge-incomplete check asked a body
question with a documented FEATURE counter (`distinct_solid_count` cannot see
`Body{index}` leftovers).

**The corrected check is volume COMPOSITION, not body count** (an interim
live-body-count fix flipped 29 cases and was itself corrected same-day: a
free-space disjoint boss is a generator-sanctioned shape — `assay/gen.rs`
repairs only no-op shapes — and two-body disjoint merges are spec'd
`disjoint_merge_bodies.rs` behavior, so 27 of the 29 were the check's false
positives, certified volumetrically by the oracle's own agree verdicts). The
categorized runner now runs the oracle in-line for every multi-op case
(`assay/volume_oracle_doc.rs`): output ≟ union of the ops' isolated solids.
**Honest baseline: 256C / 5W / 47E / 0T** — the 5W are exactly the five
anchored kernel wrongs, now loud. Also retracted: the "stacked coplanar
towers / M8-family" reading of these cases (measured: arbitrary plane
angles, no coplanar contact — an inference from the `EndCapPositive` selector
name, never measured).

**Priority order (P10: confirmed silent-wrongs outrank loud ERRORs):**
1. **Base-drop class — FIXED same day (custody invariant).** The WAFFLE_BOOL_PROBE
   sequence exonerated the kernel (every union returned correct 2-output
   splits); the defect was feature-engine's `MostRecentLegacy` collecting only
   a feature's MAIN output while consumption hid the WHOLE feature — a
   `Body{index}` leftover was deleted by bookkeeping. Fix:
   `find_most_recent_solid_outputs` (all live bodies of the target feature
   enter the fold; regression `disjoint_merge_custody.rs`). Outcome: R0030 →
   CORRECT; R0090/R0040 → honest ERRORs (their now-attempted unions STOP at
   the CDT ring-reject / non-convex CDT walls). **Baseline: 257C/2W/49E/0T.**
2. **Revolve-union deficit class — ANCHORED and FIXED (same day): 259C/0W/49E/0T.**
   The ~1 % losses were patch-tessellation chord error, not set errors
   (analytic B-Reps confirmed by face census). Fix: deterministic interior
   grid seeding in the UV-CDT (`cherchi_rs::cdt_polygon_with_holes_refined_seeded`)
   at the STRUCTURED tessellator's own spacing (torus: minor chord at the
   worst radius, `[s, s·major/(major+minor)]`; sphere: `[s, s]`), since an
   area budget cannot bound edge length and spade has no edge-length
   refinement. Measured: patch sag 8× → 1.39× band (pinned 2.0×,
   `torus_patch_edges_meet_chord_band`); R0057/R0059 → SUPPORTED_CORRECT;
   corpus delta exactly those two. The developable (cylinder/cone) engine
   already carried a per-edge width bound and needed nothing. Remaining
   recorded design question: the adapter still ignores its tolerance param
   (fixed canonical band by design — crate hard rule 5).
   Full anchor: `docs/audits/volume_oracle_flags_anchored.md` §deficit-class.
3. **§4.4.1-as-written epic** — `specs/yang_441_trim_cdt_construction.md`:
   replace relocate-in-place with the paper's curve-authoritative trim + CDT
   (the ~27-case ERROR family; closes N2's core). **I1 LANDED 2026-08-09**
   (gated `YANG_441_CONSTRUCT`, byte-identical off): unconditional line-seam
   collapse over planar pairs — mechanism sound, 0 conversions, and the
   decline census measured the I1b design. **I1b LANDED same day** (same
   gate): per-patch simultaneous collapse + single-sided tolerance-free
   re-CDT + batch write-back — on F0067 all 39 eligible seams collapse in
   ONE pass (I1: 39 passes, 11 blocked, ×500 declines); fixpoint is 9
   declines with exact signatures (×8 collapsed-seam×own-outline crossings
   at boundary junctions — an upstream junction/outline placement defect,
   the Fig-11(a) configuration; ×1 femto pair ANCHORED verts 1049+1050 at
   4.4e-16 — Root-C upstream double-mint). Gate-ON full assay
   258C/0W/49E/1T vs 259C/0W/49E/0T: zero WRONGs, zero conversions;
   F0085 = budget clip; R0095 = UNMASKED LATENT (minimal repro ONE clean
   collapse; the disturbed face's woven double-chain boundary pre-exists
   gate-OFF — spec §4-I1b has the full anchor + the new bisect probes
   `YANG_441_APPLY_BOOL_CAP`/`_SEAM_CAP`/`_VERBOSE`). Straightness identity
   gate `chain_straightness` landed (1e-9, loud skip; measured
   nonstraight=0). **TF-8 ANCHORED: the seam junction OVERSHOOTS the face
   corner by a uniform 1.339e-3 on every rib. I1c census DONE: the
   beyond-corner trim never runs (block gated on minted junction keys;
   this boolean mints none — reach gap by call-site, not predicate), and
   the overshot endpoint is the UNIQUE relocated vertex in each declined
   cycle — a Stage-4 SEAM-ENDPOINT AUTHORITY defect. I1d DONE: the
   authority is the `vert_pp_circle_junction` relocation (task #146) —
   F0067 is a wheel; radial spoke seams end 1.34e-3 short of the rim
   circle (design gap), but edge incidence at the spoke end carries both
   curves, so the classifier computes the exact UNBOUNDED line∩circle
   junction OUTSIDE the wall's kept footprint and relocates q (the
   face-boundary exit) onto it; the corridor gate sees magnitude, not
   kept content. I1e incidence census SELF-RETRACTED the "mis-attribution"
   read: every chained edge is [A:Plane, B:Cyl] — the RIB's outer end is
   CYLINDRICAL at the rim radius; the chord-chain and junction
   relocations are the paper's own resample. What breaks is the CORNER:
   the exact junction (r=0.2088) and the chord-anchored Stage-1 wall∩cap
   corner (r=0.20751) are two authorities' versions of ONE B-Rep corner,
   and the boundary folds between them. I1f (near-curve removal, landed
   gated) measured an honest NO-OP — its holder census separates
   discretization verts (2 holders) from topological corners (3).
   **I1g (three increments, all sub-gated `YANG_441_CORNER_MERGE`):
   selector VALIDATED (Fig-11(a) split-edge containment — every hit at
   the exact 1.344e-3 corner gap, zero over-fire), both mechanisms
   MEASURED OUT (substitution: refused on shared-triangle clusters;
   bare `collapse_vertex` weld: pinches ALONE — the 2026-08-05
   recorded-negative pattern; Fig-11's merge lives INSIDE the re-CDT).
   STATUS: BLOCKED ON I2 — the corner cluster's holders include the
   CURVED cap patch, unbuildable in planar scope; after I2, the
   cluster unifies via substitution-inside-rebuild with the batch
   extended to curved holders. So the epic's NEXT increment is I2
   (curved-patch single-sided rebuild: interior-vertex carry + d(T)
   recompute) — which also fronts the R-series reach.** Spec §4-I1 has
   the full measurement. **I2a/I2b LANDED 2026-08-09 (main gate,
   corpus-verified; F0085 ERROR→CORRECT is the epic's first
   conversion). I2c-1 LANDED sub-gated (`YANG_441_INPUT_REFINE`)
   2026-08-10 — and its measurement REFUTES the I1e reading recorded
   above: the rib end cap is a PLANE (every TF-8 wall run is
   plane×plane-EXACT at the designed r = 0.207507; nothing is
   chord-anchored at the corner; the 91 NonParallelAxis refusals name
   the plane-cap CIRCLE class as I2c's real tail), so the wheel corner
   is the ORIGINAL I1d verdict after all — the junction relocation
   keeps an exact UNBOUNDED junction 1.339e-3 outside the wall's kept
   footprint. Also landed: the assembly-loop LIVELOCK fix (a
   degenerating merge-only holder now blocks its merge pairs instead of
   restarting the identical batch forever — 18,322 measured spins on
   main with the merge sub-gate applied). Spec §4-I2c has the full
   measurement. J1 — boundary-exit junction authority (Fig-11(a): q ON
   the kept boundary) — LANDED sub-gated `YANG_441_BOUNDARY_EXIT`
   2026-08-11 and VALIDATED on F0067: the junction terminal merges INTO
   the exact designed corner through the batch (22 of 28 corners close;
   42 seams / 84 patches apply pass-0; re-point-safe holders exempt the
   encircling lateral; pull-scoped blame). The corner family's residual
   blocker is the A-TOP RIM-WEAVE (relocated rim-circle chain woven
   against plain chord edges at r≈0.2085–0.2098, pre-existing, also the
   canonical non-2-manifold family). RIM-TRIM landed same day sub-gated
   `YANG_441_RIM_TRIM` (side-aware circle near-curve removal with its
   own holder closure + blame arm): the 2-holder chord-sliver debris
   removes cleanly and two tops convert; the RESIDUAL is the
   triangle-scale FLUSH-INTERFACE SLIVER FRAGMENTS whose deletion needs
   Stage-0 overlap knowledge — i.e. the rim-weave's remainder is
   ATTRIBUTED TO THE M8 COPLANAR RESIDUE (item 5 below), not to this
   epic's construction machinery. Spec §4-J1 has both measurements.
   The attributed residue is now ANCHORED AND IN WORK (2026-08-11, spec
   `m8_stage0_rim_membership_refine.md`): Stage-0's §4.5.5 2D Boolean
   classifies membership against the disc's CHORD polygon, so partner
   features in sag crescents misclassify AOnly (F0067: 126 gear
   root-region profile corners at dr −3.1e-4..−1.34e-3 inside the exact
   circle — the rim-trim "sliver fragments" are DESIGN wall strips,
   correctly un-removable; the removal framing is dead). Increment 1
   LANDED gated `YANG_STAGE0_RIM_REFINE` (pre-overlay rim membership
   refinement + 2D-femto shared-mint grouping): gate-OFF full assay
   byte-identical to canonical 258C/0W/50E/0T (committed results.json
   unchanged); gate-ON 258C/0W/50E/0T with ZERO category deltas and ONE
   within-ERROR drift (R0050 LabelMismatch → Stage-4 LRR, one stage
   deeper); overlay membership measured FIXED (all 168 in-circle
   corners → Overlap/BOnly; the flank junctions mint) and F0067's
   gate-ON wall advances from `s6-planar-loop-nonplanar` to the NAMED
   M-B trio-wedge emission-identification residual (spec §3b).
   Increment 2 LANDED (2026-08-14, same gate): the trio-wedge root
   cause was one layer upstream — the shared-mint grouping's 3D
   sub-floor band enrolled a DISTINCT neighboring-column mint whose
   radial image lands 8.5e-7 from the junction (2D pre-image 8.9e-6
   away), re-writing chain topology (`i6-edge-overuse`). Fix:
   `mint_group_admits` — gate-ON identity reads the 2D pre-image at
   feature-floor width (R0072's micro twins at ~1e-7 MUST identify;
   F0067's corner-column mint at 8.9e-6 must NOT — no 3D band holds
   both) plus a rounding-noise 3D tier for coincident images. Gate-OFF
   byte-identical 258C/0W/50E/0T; gate-ON 258C/0W/50E/0T ZERO category
   deltas (drifts: F0067 advances Stage-6 non-2-manifold → render CDT
   ring rejection; R0050 one stage deeper; R0015 vertex renumber). The
   NAMED next wall — FULLY ANCHORED 2026-08-14 (spec §3d; the initial
   §4.5.3-class adjudication is RETRACTED — v22 is the EXACT
   flank×circle junction and the seam loop is monotone): the FaceId-4005
   ring tack is the **fold-revert ↔ Stage-4 junction-election
   inconsistency** at boundary-exit corners — the N2-3a fold gate
   reverts the flank-crossing junction trio to chord (A's outline chain
   cuts at chord level, keeping a crescent lift past the junction) while
   Stage-4 still elects the neighboring on-circle mint onto the exact
   junction (the seam ends at J) → a 3.6e-5 backward tack →
   self-intersecting ring. Consistent states are all-chord (ships and
   passes at the y≈−0.034 tooth) or all-junction (= the §4.4.1
   mesh-updating epic #169, the structural fix). Census-loop refinement
   is P10-DISPROVEN (spec §3d: +2 features/round self-regeneration —
   density is not membership). RESOLVED 2026-08-14 (third session,
   spec §3e, same gate): the fold gate's own Fig-11(b→c) merge arm
   could not SEE the boundary-exit corner — the §15 collapse group's
   zero-length bit-twin ring edge spread the backtrack sandwich 3
   apart, invisible to the 4-gon walk, so the ladder fell through to
   the revert. Fix = twin-mid sandwich walk in `fig11_backtrack_pair`
   (gated param; wedge sites unchanged) + sagitta containment lookup
   through the bit-equal collapse representative for lift-absorbed
   merge targets (the mirrored tooth's `sagitta=None` refusal). Both
   boundary-exit merges fire; the trios keep the exact junction;
   outline and seam agree on all-junction. **F0067 gate-ON: ERROR →
   SUPPORTED_CORRECT.** The one-sided intermediate (walk without the
   sagitta fallback) measured SUPPORTED_WRONG — 2 collinear zero-area
   slivers at the mirrored tooth, the unmasked twin of the retired
   loud reject, caught by the in-line composition oracle and closed
   same-session (spec §3e P10 record; `ASSAY_DEGEN_PROBE` added to
   the harness oracle for locating degenerate render triangles).
   GATE FLIPPED to always-on same session (spec §3f census): corpus
   259C/0W/49E/0T becomes the NEW CANONICAL BASELINE (the F0067
   conversion is the only category delta vs 258C); five stale
   pre-refinement adversary pins re-anchored (`n2_rim_mint_adversary`
   — one had pinned the §4-I1d phantom unbounded junction as ground
   truth; refinement re-anchors ring samples pair-dependently);
   capability ledger: exact plane-through-sample coincidence now
   dead-ends loud LRR (recorded wall, pinned), the 1-ULP-inside class
   now builds valid (gain). The flip also enables the machinery in the
   WASM app for the first time (env vars read `None` on wasm32).**
   **I3 FLIPPED 2026-08-15 — the §4.4.1 construct pass
   (`YANG_441_CONSTRUCT`, increments I1–I2c) is ALWAYS-ON** (spec
   §4-I3): the post-rim-refine gate-ON corpus measured 259C/0W/49E/0T
   with a case-identical ERROR set, satisfying every per-wall-class
   flip census in one measurement. The flip surfaced and closed the
   **I2d latent** (spec §4-I2d): the first-ever always-on run of the
   yang-rs pin suites caught the curved-patch rebuild silently
   coarsening a cylinder wall into secant fans (kv6b revolve∪box:
   watertight, on-surface vertices, −10 % witness-mesh volume — union
   monotonicity broken; invisible to the corpus because the
   composition oracle measures the render tessellation re-derived from
   the output B-Rep, never yang's pipeline mesh). Fix = the paper's
   own §4.4.1 closing sentence wired: `stage4_dt::d_of_t` (N2-2, first
   production consumer) certifies pre- vs post-rebuild max d(T) per
   curved patch; a rebuild that certifies coarser than what it
   replaces declines `ChordDegradation` loudly (budget = the patch's
   own pre-rebuild certified max — like for like, tolerance-free;
   planar patches exempt by identity). The assay budget guidance moved
   240 → 300s under the construct fixpoint's added CPU (F0065 ≈ 241s
   honest CORRECT, F0085 ≈ 242s honest ERROR were the new heaviest
   cases). **I2e LANDED same day** (spec §4-I2e): a
   `ChordDegradation`-declined curved rebuild retries with a
   deterministic interior seed grid at the patch's OWN pre-rebuild
   θ-arc sampling scale (halved once on a second failure; the I2d gate
   re-verifies every attempt — a rescue is never taken on faith;
   attempt-0 stays seedless and byte-identical wherever it already
   passes). Seeds are chart-lifted exactly-on-surface vertices,
   appended by the batch write-back (`PatchRebuild::new_verts`,
   `plan_verts + k` remap). Measured: kv6b's wall seeds 9 points at
   exactly the original 22.5° banding scale, seam 5 applies, the union
   sandwich holds; the squashed-drum twin passes seedless (attempt-0
   byte-stability pinned). **I4 RESOLVED BY CENSUS + I4-1 same day**
   (spec §4-I4): the census found no retireable relocate-in-place path
   remaining — every live Stage-4 pass implements quoted paper text or
   is a P10 gate; the two named selector-drivers (`stage4_fold_risk`,
   `detect_nonmanifold_seams`) are already diagnostics/banked with
   recorded re-entry conditions; `trim_beyond_corner_phantoms` is a
   local Fig-11(a) implementation whose retirement condition is the J1
   flip. I4-1 DELETED the last relocate-era hack arm — the N50
   `weld_enabled("f32")` render-twin weld (the weld audit's sole
   confirmed hack; ledger updated; the primitive survives unit-tested
   banked). Corpus byte-identical. The epic's remaining tail is
   CAPABILITY: §4.3.4 h/l/α seam-polyline density refinement, the
   §4.5.2 local-refinement loop (spec §5), and the sub-gated
   increments' flip conditions. The named follow-up "re-measure the
   #168 replan gate post-I2e" was RESOLVED 2026-08-15: full gate-ON
   corpus BYTE-IDENTICAL to canonical (259C/0W/49E/0T, zero deltas) —
   the premise did NOT dissolve (the Stage-4 STOP is upstream of the
   Stage-5 construct pass, and the sole firing case R0038 self-rejects
   at the degree-2 gate). The replan stays banked; re-entry conditions
   in `specs/yang_n2_stage4_cdt_mesh_updating.md` §5c.12. **I5-0 (the
   §4.3.4 seam-density census) MEASURED same day** (spec §4-I5): every
   conic seam at ordinary model scale fails the paper's h/l acceptance
   on ~every pair, 2–3 orders deep, α nearly always passing — implied
   densification 21×–381×, bounded; sub-unit models already compliant.
   **I5-1 (the gated `conic_eval`-midpoint insert) LANDED 2026-08-15**
   (`YANG_434_INSERT`; gate-off byte-identical; gate-ON safe —
   256C/0W/49E/3T, zero drift on completing cases — but the trio
   F0047/F0048/F0059 blew the 300s budget). **Cost LOCALIZED 2026-08-16
   (task #88, spec §4-I5 table)**: Stage 6 emits one B-Rep edge per
   mesh seam segment (F0059 E: 124 → 16 848, faces unchanged), the
   render mesh inflates 44–110×, and 98% of the timed-out case is the
   assay's `no_self_intersection` oracle (1227s vs a 300s budget; every
   other phase totals 27–36s). The "chained booleans compound the
   density" attribution is retracted — the trio are 2-op single-boolean
   cases. **I5-1b (Stage-6 conic seam chain-merge, `YANG_434_MERGE`)
   LANDED GATED same day** (task #89, spec §4-I5-1b):
   `stage5_seam_merge.rs` coalesces certified same-conic seam runs
   into analytic arc edges — trio 1254.6s → **3.39s** (F0059; SI
   oracle 1227s → 0.79s), merged bodies SMALLER than gate-off (F0059
   E 124→88). Both-gates corpus: 258C/0W/48E+1EE/**0T** — TIMEOUTs
   gone, but five category deltas + chained detail drifts, ALL in the
   chained-reentry class (to_yang re-tessellation of merged
   intermediates perturbs sample-sensitive walls: C0117 CORRECT→ERROR
   arc-ring CDT, F0067/R0099 → honest M8 coplanar boundary, C0105/
   R0028 ERROR→CORRECT). Merge-only fires too (coarse relocated
   chains certify). **I5-2 (a) C0117 ANCHORED + FIXED 2026-08-19** —
   NOT a to_yang re-entry: the failing call was kernel-v2's own render
   gate on the OUTPUT annular cap; the merged 4-arc rims (split verts
   chosen per rim) defeated `recover.rs`'s canonical-lateral pairing
   (which needed an existing azimuth-aligned vertex pair — an implicit
   shared-lattice contract), the caps fell to the general planar path,
   and the hole ring sampled 0.86° out of phase crossed the outer at a
   1e-4 wall (sagitta 4.8e-4). Fix: recover.rs two-pass pairing — pass 2
   MINTS the exact seam foot, phase-locked to an already-anchored
   coaxial lateral (the constructor's holed-profile convention); pins in
   `kernel-v2/tests/s434_typed_rim_seam_mint.rs`. Census: gate-off
   BYTE-IDENTICAL (zero deltas); merge-only and both-gates
   **260C/0W/47E/1EE/0T, 0T** — C0105/R0028 ERROR→CORRECT, F0067
   CORRECT→UNSUPPORTED (ANCHORED: M8 overlay `RoundingCollapse`
   knife-edge at Extrude 10, flipped by a 1e-15 re-fitted plane tilt —
   masked gate-off, not minted); R0099/C0117 no longer move; six
   ERROR→ERROR detail drifts. **Then the I5-1b pass was found NOT to be
   the identity on a zero-merge output** (edge re-indexing + loop
   re-rotation flipped kernel-v2's rotation-sensitive patch tessellation,
   KV7-F1, on the M5 cyl×cyl pin) — FIXED (`zero_merge_pass_is_identity`);
   with that, EVERY earlier "chained re-entry" delta vanished: merge-only
   and both-gates corpora are category- and detail-identical to canonical
   except **F0085 ERROR→CORRECT (a genuine conversion, 296–302s)** and the
   R0070 renumber. **I5-2 FLIPPED SAME DAY — both gates ALWAYS-ON
   (`=0|off` dev knobs); NEW CANONICAL 260C/0W/48E/1EE/0T at a ≥360s
   budget.** yr9 `t1` §7.3 restated on ring vertices (granularity-
   agnostic). Spec §4-I5-2 has the full record. **Post-epic Stage-4 STOP
   census 2026-08-19 (spec `yang_n2_stage4_cdt_mesh_updating.md` §5c.13):**
   every `Stage4RegionInvalid` now goes through the `#[track_caller]`
   constructor `YangError::stage4_region_invalid` (permanent site
   attribution under `YANG_LRR_PROBE`); the census attributed
   R0009/R0047 to the §4.4.1(a) unzip loop's pass cap and the per-action
   shape probe showed the loop acting on HEALTHY triangles (h/l 0.007–0.4)
   because its degeneracy test was the ABSOLUTE `MIN_FEATURE_SIZE²` area
   floor — scale-dependent, mis-firing at 1e-4 model scale (R0009 4-action
   ping-pong; R0047 5168 actions/62 s; silent edge flips on CORRECT
   R0091/R0072/R0063). Replaced, across every gate sharing the metric, by
   the scale-free collinearity IDENTITY (`tri_is_degenerate`,
   min-height/max-edge ≤ 1e-9 — the `chain_straightness` band) + a
   ping-pong certificate STOP. **Corpus 261C/0W/47E/1EE/0T — R0016
   ERROR→CORRECT (a genuine conversion), R0009/R0047 advance to deeper
   pre-existing walls, zero other deltas. NEW CANONICAL.** Same day, the
   census's other shared site — `relocate_onto_implicit_pair` → `None` on
   R0032/R0044/R0053 — was traced (`YANG_PAIR_NEWTON_TRACE`) to the CONE
   step overshoot (raw radial residual × unit normal = sec α too long;
   ratio exactly 1 − sec α; the KV16 fix the TRIPLE solver already had),
   then R0044 peeled the same-type SurfacePair junction (one-slot map →
   `same_type_junction`), kernel-v2's K9 cone sag radius (`pair_surface_
   scale(Cone)=0` → `pair_surface_local_scale`) and the projector's bare
   1e-13 tau (→ 8·ε·L) — four prose-shared-rule failures, all closed (M5
   spec §"2026-08-19"). Corpus category-identical (261C/0W/47E/1EE/0T);
   R0020/R0032/R0044/R0053 advance to Stage-6 non-2-manifold / render
   ring-reject / KV9-F2 walls. The Stage-4 LRR tail is now R0038
   (tangency), R0050 (near-coincident revolve incidence), C0067 (circle×
   circle junction) + the OffCurve five. **Same day, the Stage-6
   non-2-manifold census (`NONMANIFOLD_SITE_PROBE`, ledger table) found the
   SAME absolute `MIN_FEATURE_SIZE²` floor alive in four more gates (Stage-6
   E2 curved/planar loop guards, the §4A fold-sliver `triangle_is_degenerate`,
   the attribution's kept-triangle degenerate branch) — R0047's fatal loop
   was a healthy 2.3e-6 × 1.2e-7 quad (ratio 8.6e-2; C0058's genuine
   figure-eight reads 5.9e-16). All four moved onto the shared identity
   (`loop_is_degenerate` = Newell/extent², spec §5c.14); the Stage-1
   INPUT-face `DegenerateFace` stays the A14.2 feature-floor contract
   (`m1_adversarial` pin). Corpus 261C/0W/47E/1EE/0T category-identical;
   R0047 advanced to kernel-v2's ellipse-endpoint incidence gate (4.8e-6
   relative residual) — ANCHORED the same session (`YANG_OUT_INCIDENCE_
   PROBE` + `YANG_V_PROBE_NEAR=x,y,z,r`, two new permanent probes): the
   Stage-6 KV15b sub-resolution collapse merged a CERTIFIED plane∩cone₁∩
   cone₂ crease junction into its cone₁∩plane neighbour and the I1b "adopt
   the richer endpoint's coordinates" rule counted PLANES only (1–1 tie) →
   generalized to surface incidence within `junction_certificate_band`
   (spec `kv15b_mint_site_subresolution_collapse.md` I1b-curved; pin
   red-verified); R0047 op 2 then emitted every conic endpoint on-curve and
   the case advanced to op 3's kernel-v2 `to_yang` wall: a 4-edge CONE
   lateral `[HyperbolaArc, Line, EllipseArc, Line]` (a partial-revolve
   flank clipped by two box planes) fell to the typed "non-{canonical,
   partial,torus} edge pattern" wall because the KV14 Slice-D/E CDT
   re-entry routed only NON-4-edge or holed laterals — the edge COUNT was
   a proxy for the pattern. Routed by PATTERN (`four_edge_structured`;
   pin `four_edge_non_structured_cone_lateral_reenters`, a tilted slab
   minus a 60° frustum wedge, red-verified). **R0047 ERROR → SUPPORTED_
   CORRECT. Corpus 262C/0W/46E/1EE/0T — NEW CANONICAL (the only delta).**
   **Then R0053 (same census, `i6-input-overuse`) ANCHORED: the Stage-0
   mesh of the FRESH gear revolve was non-conformal — `collect_edge_
   splits`' EXACT 2D collinearity test dropped the boundary subdivision of
   B's planar end-cap edge (180,181) at an 8.4e-16 rounding miss (census:
   522 misses ≤1e-13 vs 216 ≥1e-4, nothing between), so the adjacent cone
   flank never received the split (T-junction). FIXED: a side-region
   BOUNDARY vertex collinear to the scale-free identity registers (spec
   `m8_stage0_inputcheck_clean_emission.md` addendum; pin red-verified).
   R0053 → render ring-reject; and **C0075 completed for the first time
   and exposed its authored `euler_target: 2` as WRONG — the two
   interleaved 12-tooth gears enclose two through-pockets (genus 2, χ=−2,
   independently derived by grid flood-fill); meta corrected + pinned in
   `historical_authoring_fixes_pinned`. C0075 ERROR → SUPPORTED_CORRECT.
   Corpus 263C/0W/45E/1EE/0T — NEW CANONICAL.**
   **§4.4.1 I6 — Fig-11(b)→(c) FOLD MERGE, landed gated
   (`YANG_441_FOLD_MERGE`) 2026-08-19d.** Census over the nine `ring
   rejected by CDT` cases (`YANG_S6_LOOP_SIMPLICITY` + `YANG_S5_FOLD_PROBE`;
   ledger "CDT ring-reject fold census"): **every measurable non-simple
   output loop is `class=MINTED_BY_S4`, `cross_inherited=0` family-wide.**
   Mechanism, per vertex: the arrangement crosses two INSCRIBED meshes, so
   the exact analytic junction lies on the FAR side of the neighbouring rim
   grid vertex, and Stage 4's relocation steps OVER its own neighbour (F0045:
   2.382e-2 move across a 1.283e-2 spacing; the neighbour's turn goes 27.69°
   — exactly the rim's 360/13 grid step — to 167.34°, with zero residual on
   both its surfaces). That is Yang Fig-11 verbatim, reached from the other
   side, and the paper's remedy is (b)→(c): merge the overrun vertex into the
   relocated one. Selector `stage4_fold_risk::fold_merge_sites` is
   threshold-free (apex still ∧ chord order inverted across Stage 4 ∧ the
   overrun END moved; the sign of the chord parameter picks the survivor, so
   there is no distance tie-break). Its moved-oracle is `S4_PRE_POS`, NOT
   `relocations` — measured: that vector is EMPTY on R0074/R0085/R0095/R0025
   while 59–83 vertices per loop moved, so a `relocations`-keyed condition
   rejects the whole family. Repair `stage4_construct::rebuild_merge_fan` is a
   LOCAL re-triangulation of the victim's fan, not `collapse_vertex` (measured
   negative 2026-08-05) and not `rebuild_patch_planar` (measured 2026-08-19:
   `ThetaUnwrap` on the encircling laterals F0045/R0090 merge on, and
   `TriangulationFailed` where the patch still carries other folds). The fan
   is θ-unwrapped against the VICTIM's own branch, so no global span exists to
   fall outside of; every holder rebuilds (all-holders-or-none) and the batch
   applies with an EMPTY substitution map after verifying no triangle outside
   the fans still holds the victim — so no index is ever relabelled without
   being re-triangulated. **NEW CANONICAL: 265C/0W/43E/1EE/0T — F0045 and
   R0090 ERROR→SUPPORTED_CORRECT, ZERO other category or detail deltas;
   rewrite tier green with the pass on.**
   The residue is TWO defects the census separates (`apex_minted = 0`
   family-wide, so every rejected inversion has an apex that genuinely moved;
   the split is on whether BOTH the apex's incident cycle edges are
   intersection-curve edges): **ON-CURVE** — R0044 163/188 + 96/109, R0053
   62/83 + 12/12, R0095 13/20 — two vertices of one chain crossed, i.e. §4.3.4
   chain ORDER (`ReorderConic`, I2b), on `Hyperbola`/`SurfacePair` curves that
   I5-1b already records as per-segment; and **OFF-CURVE** — R0011, R0025,
   R0074, R0085, 100 % of their inversions — a RELOCATED vertex crossed a
   neighbour on a PLAIN boundary. **ANCHORED 2026-08-19e (spec §4-I7): the
   relocation is correct** (the torus arm's `tangent_plane_corridor` gate is
   satisfied with room to spare — worst ratios 0.69/0.32/0.23 at sinθ
   0.90–1.00) **but `d_eps` is 27–1000× the local segment**, so a move well
   inside the off-curve budget is still many local edges long. Split by
   displacement over the corner's own shorter edge, it is TWO classes with
   different owners: **LOCAL** (R0011 median 1.48×, most of R0074 — F0045, which
   converted, sits at 1.86×) needs only the both-moved survivor rule
   (surface-incidence richness, KV15b I1b) added to the I6 merge; **GROSS**
   (R0085 median 6.07×, 42 % beyond 10×, max 1737×) is **§4.5.2 local
   refinement**'s own trigger — item 4 below, not an unowned class.
   **§4.4.1 I8 — the Fig-11 merge's two preconditions, landed gated 2026-08-20
   (`YANG_441_FAN_OF_ONE`, `YANG_441_MERGE_CARRIER`; spec §4-I8).** (a) The
   `FanNotSimple` refusals on R0011/R0074/R0085 were NOT "pinched victims" —
   that was an inference from the variant's name. The variant now carries a
   reason (`Degenerate`/`Pinch`/`Split`/`Short`) and all three measure
   `Short { fan: 1, link: 2 }`: the victim has a SINGLE triangle in the
   declining patch, so the merge degenerates it and the correct rebuild is the
   EMPTY one. (b) With that repaired, the next decline exposed the real
   precondition: **a merge may IDENTIFY two positions only when
   `carried(victim) ⊆ carried(survivor)`**, certified at
   `junction_certificate_band`. Measured — F0045 `{B:0,B:2} ⊂ {A:2,B:0,B:2}`
   and R0090 likewise (both still APPLY, still CORRECT); R0011
   `{B:1,B:180,B:181}` vs `{A:2,B:1,B:181}` and R0074/R0044/R0085 the same
   shape: equal-size sets that DIFFER, i.e. a model CORNER and a curve∩edge
   junction 5–7 local units apart, which no merge can join. Closes a latent
   silent-wrong (those sites are refused today only because a small holder
   happens not to contain the survivor). (c) **I7's named next increment is
   RETRACTED**: R0011's site is not a both-moved corner (its overrun end never
   moved). The four blocked sites carry one sharp certificate instead — the
   victim lies EXACTLY on the survivor's travel segment (off-travel 6e-13 to 0,
   at t = 0.04–0.67) versus 5–6.6 % of travel for the two that converted. The
   relocation slid along a straight carrier (the model edge it stayed exactly
   on) and overshot that carrier's ENDPOINT, so **the relocated position is
   outside its carrier's domain** — §4.5.2 local refinement's trigger (item 4),
   not a merge and not `ReorderConic`. The honest intermediate step, not built:
   a relocation-domain STOP at the `(2s)`/`(2t)` arms.
   **§4.4.1 I9 — the RELOCATION-DOMAIN postcondition, ARMED 2026-08-20**
   (`YANG_S4_CARRIER_DOMAIN`, on by default; `0|off` dev knob, `census` =
   report-only). I8 named the class; I9 names it where it HAPPENS instead of
   three stages downstream. Two legs, both required: (1) a still neighbour lies
   ON the traveller's `pre → post` segment, strictly inside, at the shared 1e-9
   relative collinearity identity (`point_on_segment_interior`, now shared with
   `on_segment_interior` so the two gates cannot drift apart); (2) that
   neighbour carries a surface the relocated position is OFF — a domain
   ENDPOINT, not a plain sample of the traveller's own carrier, which Yang's
   near-curve vertex removal legitimately owns. It is a POSTCONDITION over the
   whole stage (one entry snapshot, one check at the end) because relocation
   happens at 13 sites plus `apply_boundary_relocations`, and every repair that
   might dissolve the configuration runs before the stage ends. Full-corpus
   census before arming: fires on R0004/R0011/R0044/R0074/R0085 only — **not one
   SUPPORTED_CORRECT case** — while leg 2 exempts R0051 (a sample; its
   self-intersection has another cause) and F0064 (samples; leg 1 alone would
   have demoted a known coplanar capability gap to a failure). Armed corpus:
   **265C/0W/43E/1EE/0T unchanged, four detail deltas, zero category deltas.**
   The REPAIR for this class already exists for the minted half —
   `trim_beyond_corner_phantoms` (P3b inc-4b) collapses the overrun phantom into
   a Stage-1 MINTED corner, under a patch-subset guard that is the face-level
   sibling of I8's surface-level containment. The I9 sites differ only in that
   their corner is INHERITED. Extending that trim to inherited corners is the
   next increment and is a repair, not a STOP.

   **§4.4.1 I10 — that increment is REFUTED, and the class is REASSIGNED
   (2026-08-20, measurement only).** The census now reports the face-level and
   surface-level columns, and inc-4b's own patch-subset guard refuses **24 of
   24** sites. Mint-ness was never the operative distinction: it PROXIED "does
   the corner carry the traveller's far-operand face?", and an inherited model
   corner is single-operand by construction, so it never can. What the sites
   actually are: the traveller rides its carrier model edge (exactly on both
   near surfaces at pre AND post) chasing a FAR surface whose distance falls to
   `d_q > 0` at the edge's own endpoint and reaches 0 only past it — a linear
   extrapolation predicts the measured overrun to 0.3–3.6 %. That is Yang §4.5's
   stated trigger verbatim ("cannot converge to a distance of 0 **within their
   domains**", `refs/text/yang2025_hybrid_boolean.txt:652-656`), and §4.5.1
   `:672-690` describes the defect in the same words. Consequence:
   `stage4_truncate.rs` already holds §4.5.1's truncation mechanism and records
   that it BORROWS it under a non-paper trigger because "our relocations converge
   exactly" — that premise does not cover this class, so the borrow is
   unnecessary here. **Next increment: measure the paper's own strategy-selection
   predicate** (`:740-744` — §4.5.1 only when the failure points are bounded by
   two successfully optimized points on the SAME surface, else §4.5.2 local
   refinement) over the 24 sites; the measurement picks the strategy, not a
   preference. **TAKEN (`YANG_S45_SELECT`) — then CORRECTED: SECOND
   STRATEGY (§4.5.2), 24/24.** The first reading said §4.5.1 because the census
   implemented only the second of §4.5's TWO selector clauses. The first
   (`refs/text/yang2025_hybrid_boolean.txt:637-651`) excludes this class outright:
   *"the first strategy only applies to the interior points but not to the
   boundary points that glide along the boundary curves"*, and Fig-13(c) draws
   the topology error crossing the corner causes — which IS §4-I9's
   `RelocationCrossedCarrierVertex`. Measured: every traveller is on TWO surfaces
   of one operand at both ends of its step (riding its boundary curve) and every
   crossed `q` is on THREE (Fig-13's corner `s`). **24/24 excluded**, so §4-I8's
   original assignment to §4.5.2 stands, and the §4-I9 STOP is the answer until
   §4.5.2 exists. `stage4_truncate::max_in_domain_step` survives as a domain-exit
   measurement (which §4.5.2 also needs), not as a step of §4.5.1.
   The bounding data from the superseded first reading still stands and is kept
   for the record — the erroneous region is ONE point, every bound (2–6 per site,
   all 1 hop away) is converged, all bounds share EXACTLY ONE surface (the
   near-operand CARRIER face; R0011's four sites all name the same
   `Cylinder{r=6277.3}` = face B:1), and the traveller is on it too. Sound data,
   wrong verdict: it answers §4.5's second clause, and the first clause decides.
   Also corrected: I9's R0074 armed detail is `OffCurveBeyondChordBand`
   v91, not `RelocationCrossedCarrierVertex` (counts unaffected). Spec §4-I10.

   **§4.4.1 I11 — does §4.5.1 have ANY customer? YES: 5 vertices in 3 cases
   (2026-08-20, measurement only).** Widened the census from the I9 list to the
   paper's own failure population over all 312 cases, taken from BOTH exits of
   Stage 4 (a run that STOPs never reaches the end-of-stage postcondition, and
   the hardest cases all STOP; even then the STOP'd vertex is never WRITTEN, so
   it is classified directly). Coverage: 125 all-planar cases never run Stage 4
   (`if has_conic`, 0 of them report as predicted); of 187 curved, 113 report
   from the postcondition, 16 from a STOP, 65 produce no conic output edge.
   Result over 30 287 curve vertices / 10 194 moved — **completing Stage 4: 36 of
   36 failure members are BOUNDARY points (§4.5.2's), interior = 0**, extending
   I10 (f) from the I9 list to the whole completing population; **Stage-4 STOP
   vertices: 6 of 12 are INTERIOR**, of which five are candidates — C0065 (v3,
   v8), R0003 (v4233, v10583), R0028 (v64), all `OffCurveBeyondChordBand` with
   carrier `(A0,B1)` = Fig-12(a) "the intersection of the meshes is shifted onto
   S2, completely bypassing S1"; C0065's STOP site is the owner-face hull check =
   Fig-12(c). R0050 is excluded on inspection (converged `(A1,B1)`; its STOP is a
   torus-endpoint scope wall). **Still untested and it decides: §4.5's SECOND
   clause (bounded by two converged points on the same surface) has not been run
   on these five** — they are CANDIDATES, not confirmed customers. Next
   measurement: the bounding walk from the STOP vertex on C0065/R0003/R0028.
   Instrument validated by reproducing the I9 fire list exactly (24). Census cost:
   R0038 ERROR→TIMEOUT under census only. Spec §4-I11.

   **§4.4.1 I12 — clause 2 from the STOP vantage: the five candidates SPLIT
   (2026-08-22, measurement only).** The clause-2 bounding walk (extracted as
   `selector_clause2_walk`, postcondition output unchanged on R0074) now runs
   from the STOP exit, with "successfully optimized" = converged ∧ not an
   §4-I9-style out-of-domain crosser (`vertex_crossed_domain_endpoint`,
   cross-checked `true` at the postcondition's own fire site). Result:
   **R0003's v4233 + v10583 are §4.5.1's FIRST CONFIRMED customers** — interior,
   bounds 1 hop away on every branch, all sharing cone+plane, traveller on one
   of them: Fig-12 to the letter. **C0065 (v3+v8 — one curve-adjacent region
   reaching a degree-4 junction) and R0028 (no converged bound in 64 hops both
   ways) fall to §4.5.2** by the paper's own sentence ("if such bound cannot be
   found … the second strategy"). Vantage caveat recorded: the STOP freezes the
   mesh mid-sweep; the paper selects post-sweep — the two non-confirmations
   could flip there, the confirmations cannot. Build order (spec §4-I12 (d);
   design spec `specs/yang_451_optimize_across_boundaries.md`):
   (1) §4.5.1 gated at the refusal site — midpoint of bounds +
   `max_in_domain_step` truncation + cross-boundary continuation + q1/q2 on
   C_b + §4.3.4 refine; pin R0003; (2) refusal → record-and-continue loop
   conversion (the paper's collect-then-repair, `:652-670`), which also puts
   the selector at the paper's vantage; (3) §4.5.2 local refinement.
   **REVISED the same day by inc-0/inc-1 (spec
   `yang_451_optimize_across_boundaries.md` §7–8):** inc-0 measured that a
   mid-sweep refusal cannot see a bound the sweep has not reached (the
   convergence pattern IS sweep order), so record-and-continue moved from
   step 2 to the FRAME of step 1 and is LANDED as a census
   (`YANG_451=census`: gates record-and-skip, post-sweep selector at the
   paper's vantage, first error returns unchanged; default byte-identical).
   At the paper's vantage: R0003 = ~45 failures/invocation, 100 % interior
   + own-conic-bounded (regions len 1–12 ⇒ the repair needs the region
   collapse from the start); C0065/R0028's frozen-vantage non-confirmations
   FLIP — their interior failures are §4.5.1 candidates too (torus-carried;
   need surface-pair region identity, a follow-on sub-increment); only
   C0065's two boundary gliders are Fig-13-excluded §4.5.2. **inc-2a/2b (same
   day): the repair-variant preview measured R0003 as 11/11 DRIFT regions
   with certificating midpoint projections (0 straddle — the cross-boundary
   half of §4.5.1 has NO measured customer and stays unbuilt), and the DRIFT
   repair LANDED GATED (`YANG_451=1`): plan-then-apply, §4-I8-checked
   collapses, shared-certificate + scale-sanity acceptance, red-verified by
   two mutations. Under the gate R0003 CLEARS Stage 4 (11/11) and advances
   to the KV9-F2 developable fold — a masked latent of the developable-ring
   family (fold cone tan 2.3961 matches no repaired cone), not a mint.
   Torus-carried cases decline to their exact original errors. **FLIPPED
   ALWAYS-ON same day (spec §11): corpus category-identical, exactly one
   explained detail delta (R0003 → the KV9-F2 fold); `YANG_451=0`
   restores the abort; baseline results.json updated.** The torus-region arm
   (inc-3b, 2026-08-22) repaired R0028's v64 and C0065's repair was refused
   by its own hull (#137 triply anchored). **inc-4 (2026-08-24, always-on):
   the torus arm's repair was MINTING at the triple point — v64 is a
   torus∩plane∩cylinder q-point (the paper's q on C_b) and the pair
   completion solved 2 of its 3 surfaces; fixed by (4a) certificate on the
   vertex's FULL inc0 constraint set in BOTH arms + (4b) solve set = the
   constraint set (3 surfaces → `relocate_onto_implicit_triple`),
   red-verified by force-degrading the triple. Behind it, R0028's REAL
   recorded fold: the torus-block pair-Newton relocates near-junction curve
   vertices PAST the junction (entry cap −3.6e-5 → +2.0e-4 — retracting the
   2026-08-04 'Stage 4 relocated NOTHING' exclusion; `n_relocations` is
   blind to torus-block moves). Owner: §4.5.3, which covered TYPED chains
   only — the NEW pair-chain reversal sweep
   (`specs/yang_453_pair_chain_reversal.md`, deviation N59, FLIPPED
   ALWAYS-ON same day) tests progression-sign along n₀×n₁ at untyped pair
   sites and collapses the overshooters through the existing
   gate/collapse/restart path. Corpus: **267C/0W/40E/1EE/0T** — R0028 and
   R0025 (both recorded ring-fold cases) ERROR→SUPPORTED_CORRECT, R0032's
   non-2-manifold wall peeled onto the recognized curved partial-patch
   NotSupported boundary (⇒ UNSUPPORTED(curved-profile)); off-knob
   `YANG_453_PAIR=0` restores each prior error. R0044 (whose row named
   this vehicle) is census-quiet — its crease reversal is
   junction-adjacent, outside same-pair eligibility; junction-site
   handling is the recorded next increment.** §4.3.4
   refine-after-repair stays deferred with reasoning (spec §10). Spec
   §4-I12 + `specs/yang_451_optimize_across_boundaries.md` §14–15.
   Remaining loud refusals: unchartable cone/torus holders (R0044, 13 sites).
   Spec §4-I6, §4-I8, §4-I9.
   **§4.4.2 carried-edge curve restoration (2026-08-25/26, spec
   `yang_434_output_chord_refinement.md`, deviation N60 → RESOLVED):**
   the KV9-F2a "deep chord" family measured as CARRIED INPUT RIM CIRCLES
   (15077/15077 matched at 1e-11) tessellated at Stage-1 mesh density and
   never re-typed — the owner is §4.4.2's "original boundary curves", not
   §4.3.4. `restore_carried_edge_curves` re-types eligible chords onto
   their input circles (certified with from_yang's own band + midpoint
   DOMAIN check); the always-on I5-1b merge coalesces; consumers sample at
   their own density. **FLIPPED ALWAYS-ON 2026-08-26** after fixing both
   blockers structurally: R0054 by kernel-v2 CONFORMAL grid-aligned arc
   sampling (axis-canonical global azimuth grid + conforming vertex
   inserts — coaxial rims phase-lock the way Stage-1's shared mesh grid
   did), F0085 by aligning the strict 1e-12 loop-vertex planarity check
   with its documented debug-only tier (the F1 `TAU_EVAL` gate is the
   production boolean-output contract), plus the barrel-arm HOLE-WINDOW
   latent the flip unmasked (a hole outside the assembled ring's window
   is silently filled — C0105's selfx root; red-verified by
   `curved_output_reentry_through_boss`). Corpus: **271C/0W/36E/1EE/0T**
   (+R0020 +R0095 +C0105; zero CORRECT regressions; seven explained
   detail shifts; `YANG_434_OUT=0|off` = dev off-knob). The conformal
   sampling also flips the [M5]/#172 seam-sampling pin: the general
   unequal-radius 90° cyl×cyl union now completes validated
   (`unequal_perpendicular_now_supported`). New quarantined
   capability gap: M8 n-ary mixed-arc seam-split (4-arc) strip lateral
   (fixture-only, zero corpus customers). Next walls unchanged: the
   rim×cut junction boundary hook (R0003 face 437 / R0100 face 15 /
   R0004's family) and R0017's F2b inversion.
   **I13 (2026-08-25, FLIPPED ALWAYS-ON same day — spec
   `yang_441_trim_cdt_construction.md` §I13): the rim×cut hook ANCHORED
   and its first repair LIVE.** Face
   437 = an out-and-back spur past the solved rim junction (the interior
   curve vertex relocated onto the conic at its preserved azimuth, 4.3
   strip-widths beyond the terminal). Three gated mechanisms:
   `YANG_441_CONE_CHART` (SurfaceChart::Cone — the I2 tail's named item;
   five chartability pre-filters consolidated onto `supports()`),
   `YANG_441_OPEN_CONIC_PARAM` (`conic_param` += Hyperbola with a
   periodic/open split guarding every angle-domain consumer;
   `order_along_curve` open-conic ordering), `YANG_441_ONCURVE_MERGE`
   (the Fig-11 on-curve terminal-overrun selector arm + the
   construct/fold-merge alternation; fold-merge pass cap now derived
   from the true runaway bound). Flip proofs: default corpus
   bit-identical pre-flip; post-flip corpus category-identical
   271C/0W/36E/1EE/0T with exactly two explained detail rows — R0003
   advances 437→467 (202 merges apply; the NEXT family is the
   out-of-band terminal RUN at the junction) and R0004's ring-CDT
   subtract wall CLEARS (its unrelated RevolveAxisIntersectsProfile
   first error remains). R0100 face 15 unmoved — not this family; own
   anchor owed. Off-knobs:
   `YANG_441_CONE_CHART|OPEN_CONIC_PARAM|ONCURVE_MERGE=0|off`.
   **I13d (2026-08-25, FLIPPED ALWAYS-ON same day — spec §I13(d);
   off-knob `YANG_441_RUN_ABSORB=0|off`, `census` mode kept): run-level
   junction absorption.** Flip proofs: default corpus bit-identical
   pre-flip; gated corpus category-identical 271C/0W/36E/1EE/0T with
   exactly one explained detail row (R0003 face 467 → 517); heaviest
   case 320s. Walk-back probe revises
   the anchor: face 467's run = the JUNCTION relocated 0.67 onto its
   rim×cut solve, HOPPING its first two chain samples in curve
   parameter (the samples barely move). Certificate = symmetric
   pre/post pair-ORDER inversion of (sample, junction) along the typed
   conic + a minted chord-inversion witness (structurally refuses the
   junction's other in-domain chain) + strictly-richer carrier; repair
   = `rebuild_run_fan`, ONE region rebuild per holder absorbing every
   run victim into the junction (per-victim fans are structurally
   refused — each link polygon holds the still-folded sibling).
   Measured gated on R0003: 31 sites first fixed point (runs to 8
   victims), 25 absorb cleanly, **face 467 CLEARS → face 517** = the
   residual SIX single-overrun sites on the wall plane, measured
   (`YANG_441_FAN_PROBE`) as genuinely self-intersecting fan polygons
   of mutually INTERLOCKED PAIRS (adjacent strips' deep overruns cross
   on the wall) — next increment I13e cross-site group absorption,
   spec §I13(d) tail.
   **KV9-F2b (2026-08-27, FLIPPED ALWAYS-ON — spec
   `specs/kv9_f2b_lift_faithful_refinement.md`; off-knob
   `KV2_PATCH_LIFT_REFINE=0|off`): the developable patch refinement
   refines until the chart→3D LIFT is orientation-faithful. NEW
   CANONICAL 272C/0W/35E/1EE/0T** (+R0017). Anchored on R0017 (0.09 s
   vehicle, ~500× faster than R0003). Two new instruments named it:
   `KV2_PATCH_ASPECT_PROBE` (surface-metric worst aspect, CDT vs
   refined — the fold face goes 204 → 3473 while its SAME-development
   control face holds at 109.80 exactly, so the refinement MINTS the
   sliver) and `KV2_PATCH_MINT_PROBE` (per-split parent/child aspect —
   the minting split bisects a needle of base 17.0 against 1075 sides).
   The LEPP walk is a faithful Rivara implementation and its
   parent→child degradation there is 66.8 → 132.3, EXACTLY the factor
   of 2 its theorem promises: Rivara bounds CHART angles, and the fold
   is a property of the lift. Moving the refinement into the cone's
   isometric development was measured and REFUTED in both forms it can
   take (spec §4) — mixed metrics fold the control face; the true
   developed midpoint lies OFF the chart edge it splits and
   self-overlaps the patch (R0032 regressed to `mixed 2D orientation`).
   Repair = a second work-queue criterion, `dev < sag`, comparing what
   bisection CAN remove (ideal chart-lift sagitta) against what it
   cannot (nodes off the development) — no tuned constant, and
   self-terminating because sag falls quadratically. That comparison
   IS the F2a/F2b discriminator and the two separate by 21 orders of
   magnitude (R0017 f17 `dev/sag = 7.9e-14` refine; R0003 f577
   `dev/sag = 5.1e+07` decline). Flip proofs: gate-off corpus
   BYTE-IDENTICAL to the committed baseline (530.9 s); gate-on
   **272C/0W/35E/1EE/0T with exactly ONE category move and ZERO detail
   moves**, zero CORRECT regressions, marginally faster (521.6 s).
   R0017's fold clears on FIVE extra splits (109 → 114).
   **§4.3.3 Case-IV corner phantom (2026-08-27 — spec
   `specs/yang_433_case_iv_corner_phantom.md`): R0100's face-15 ring-CDT
   wall ANCHORED as a PHANTOM intersection loop; census built
   (`YANG_433_PHANTOM=census`); Stage-1 corner-graze guard BUILT but
   flip REFUSED by corpus measurement — the wall stays, correctly
   named.** The extrude-cut prism's cap-corner wedge analytically MISSES
   face-15's cone by ≥1.33 (the whole true cut is on the neighboring
   shoulder cone and never crosses their shared rim), but Stage-1's
   inscribed mesh sags 2.26–2.29 there and mints a 3-vertex loop;
   relocation solves each vertex to an exact-but-VIRTUAL pierce point
   (both edge-line×cone roots outside the B-edge's own segment AND the
   face's station band — the paper's §4.3.3 "no solution in the
   parametric domains ⇒ rule out Case IV" clause, `refs/text:518-537`),
   everting the loop 12× across the rim → the misnamed CDT reject.
   Corpus census: exactly FIVE cases carry certified phantom claims —
   ALL ERRORs, ZERO of the 272 CORRECT (R0100 = the corner shape;
   R0004/R0011/R0044 behind other walls; R0053's 64 claims are a
   seam-population/coplanar-graze signature, M8 territory, not this
   family; R0003 f577 has NO claims — its F2a fold stays with §4.4.2).
   The gated guard (fourth `boolean()` rim-N arm,
   `edge_graze_min_rim_segments`: buried-corner clusters of ≥2
   inside-grazing non-piercing LineSegment edges vs a station-banded
   cone/cylinder face; exact roots + Lipschitz-certified clearance →
   `sag ≤ g/2` N) converts R0100 AND R0049 gated, but two full gate-on
   sweeps measured 26 cases boosted and 6+ CORRECT regressions
   (R0017 breaks under an N=33 mesh perturbation; R0011 ERROR→silent
   WRONG) — mesh-density triggers are structurally too broad here, and
   the downstream rule-out needs the B-side's MISSED mirror crossings
   (Case III) created, i.e. the phase-3 junction-layer conformal mesh
   update. Guard stays `YANG_433_GUARD=1|on` dev-gated with 4 unit
   tests; the census is the family's permanent instrument.
   **inc-8 ellipse density contract (2026-08-28 — spec
   `yang_434_output_chord_refinement.md` inc-8): R0003's FaceId(577) F2a
   fold ANCHORED and FIXED in kernel-v2 sampling.** The off-development
   node (dev 9.122e-2) descends from an EllipseArc sub-chord:
   `ellipse_interior_samples` was the ONE boundary sampler with no
   surface-scale sag contract (uniform in parameter ⇒ max chord sag ≈
   R_maj·(1−cos(π/n_seg)); the steep cut-plane×cone section's R_maj ≈ 93
   dwarfs the strip it bounds). Fixed by sag-bound recursive bisection at
   the incident faces' smallest local surface radius (the surface-pair
   sampler's precedent; off-knob `KV2_ELLIPSE_SAG=0|off`), plus unifying
   the EllipseArc chain walk on the per-sample wrapped-Δθ mechanism (the
   cylinder fraction shortcut assumed uniform samples). A conforming
   curve-sample pool for arcs was built GATED OFF
   (`KV2_ARC_CONFORM_CURVES=1`) — no corpus customer. R0003 advances to
   FaceId(903): a WALL-PLANE ring-CDT reject whose crossing is entirely
   between B-Rep VERTEX origins — pre-existing yang-side geometry
   unmasked from behind 577. **ANCHORED same day as §I13(f)** (spec
   `yang_441_trim_cdt_construction.md`): an INVERTED JUNCTION PAIR on
   the S3-band hyperbola — the cut-corner triple {S3,S2,S4} is a
   junction-level PHANTOM (its exact solve lies outside the band's
   rim-bounded domain: the §4.3.3 rule-out clause, R0100's Case-IV
   lesson at a junction), the S3-band's 0.04 wall sliver does not exist
   in exact geometry, and the true topology needs the MISSED mirror
   crossing {S0,S2,S4} minted on the adjacent band's hyperbola. I13d's
   selector fires its certificates and correctly refuses at
   `strictly_richer` (both are true corners). Repair = junction
   RE-HOMING across a rim (certify phantom by the domain clause → mint
   the mirror crossing once → update the affected cycles) — the
   phase-3 junction-layer mesh update's first SMALL customer, the
   right vehicle before R0100's loop-level case.
   [STATUS 2026-08-28 (5 sessions): f0 census + f1 planner + f2 gated
   apply arm + f2b material discriminator + f2c re-homing arm + f2c-2
   junction-layer hole re-fill ALL BUILT GATED (`YANG_441_REHOME`) and
   ON-measured — **f903's ring-CDT RESOLVES gated-ON** with a CLEAN
   edge-use audit; the composition oracle then flagged the completed
   result SUPPORTED_WRONG (χ=2 vs 6 for 3 shells), and the same-day
   `YANG_CHI_AUDIT` instrument localized a genuine stage-2 genus-2.
   **The f2c-3 bridge census (`stage4_slit.rs`, `YANG_441_SLIT`) +
   density-ladder adjudication (`YANG_NSEG_FLOOR` 41/82/164, χ
   [−2,2,2] at every rung) then REFUTED the planned slit repair: the
   genus is TRUE topology** — two micro-filament handles where the
   gear flange corners arch over the pocket-corner void (verified
   closed-form: void under the film, both ends attached). The wrong
   party is the composition oracle's per-shell genus-0 credit, whose
   formula telescopes to χ=2·shells and cannot express R0003's truth
   (spec §I13(f) item 6 RESCOPE). At 4× density the case COMPLETES
   end-to-end with no rehome machinery (ring ladder: 41→f903 ERROR,
   82→f904 ERROR, 164→completes; finished B-Rep χ=2, 3 shells) — at
   that density the ONLY gap is the oracle formula. **The oracle
   genus term LANDED 2026-08-29** (optional hand-adjudicated
   `expected_shell_count` in the meta oracles, enforced STRICTLY —
   exact shell count, no extra-shell allowance, χ == euler_target
   exactly; R0003 authored (2, 3 shells) and pinned in
   `historical_authoring_fixes_pinned` with the derivation;
   measured: R0003 at 4× = SUPPORTED_CORRECT all-checks-pass,
   baseline byte-identical canonical ERROR). **f4 FLIP LANDED 2026-08-29:
   `YANG_441_REHOME` ALWAYS-ON — gate-off corpus byte-identical;
   gate-on exactly ONE move: R0003 ERROR → SUPPORTED_CORRECT, all
   oracles pass. NEW CANONICAL 273C/0W/34E/1EE/0T.** The §I13(f)
   inverted-junction-pair epic is CLOSED end-to-end (phantom re-homed
   across the rim, junction-layer hole re-fill, genus-aware oracle).]
   Stage-6 non-2-manifold family map (ledger table): F0058 = equal-R
   perpendicular cyl CUT whose A-seam passes through the exact tangency
   point — the kept upper/lower sheets both fan onto the lower seam
   segment (pinch-vertex construction defect, `s6-wedge-walk-not-outgoing`
   precedes); F0060 = cylinder tangent to both caps along a line (line-
   pinch solid, not 2-manifold-representable); R0032 torus × two-cone
   junction double cover; C0058 tangency-neck figure-eight (§4.3.3
   tangent-point insertion milestone); C0107/C0108 designed 0D tangency;
   C0044 M8 flush stack; R0053 chained INPUT not watertight
   (`i6-input-overuse`).**
4. **§4.5.4 removal / §4.5.2 guard-shell loop** (item 3d/4 below) after the
   construction lands. *§4.5.2-as-RECOVERY ADJUDICATED OUT 2026-08-29
   (`specs/yang_452_local_refinement.md`): the uniform density ladder
   (`YANG_CHORD_REFINE`, debug-only, all surface types) measured the whole
   10-case `Stage4RegionInvalid` family at 2×/4×(/8×) — ZERO convert; the
   persist class is tangency/junction-topology (scale-free), R0038 alone
   completes at ≥4× (ladder-stable plausibly-true genus-1; owner stays
   #169 C/D), and refinement UNMASKS defects in R0050/R0077 rather than
   fixing them. The typed STOPs + in-line oracles ARE the faithful
   guard-shell posture; do not build the recovery loop. The §4.5.4 REMOVAL
   half (SELFX fire-list on ~33 CORRECT cases) is unmeasured and stays
   open.* *The corner-crosser sub-family the same census identified
   (R0011/R0044/R0074/R0085 — `RelocationCrossedCarrierVertex`, I13f
   rehome kin) has its own epic: `specs/yang_451_corner_transit.md`.
   inc-0/inc-1 (2026-08-29) confirmed feasibility 46/46 and validated
   the corner-incident-edge discriminator; inc-2a+2b (2026-08-30) landed
   the pure transit PLANNER (`stage4_transit.rs`, 23/23 verdicts = 12
   transit + 5 clip + 6 typed declines, shared mints deduped by
   POSITION identity across m1 edge copies) AND the corridor-walk
   census: the repair unit is the fan-walking CORRIDOR (truncate-at-
   the-corner refuted; the v42→v78 corridor merge proven bit-equal;
   existing-healthy-junction splice terminals measured on R0044;
   R0085 walled on its own operand quality). inc-2c-0 (same day) landed the
   all-roots per-edge step solver (circle×quadric quartic, certified)
   and ADJUDICATED v76 (wrong-root Newton artifact — the true exit
   exists). inc-2c-1 (same day) swapped the
   walk onto it (margin guard, ReachedExistingJunction splice terminal,
   torus Newton fallback): v76 RESOLVED (its corridor merges bit-equal
   with v105's clip) — EVERY family corridor is now determined.
   inc-2c-2+3a (2026-08-30, second session) landed the corridor
   ASSEMBLY (`assemble_corridors`: greedy spine grouping, contract-band
   splice dispositions, locality-filtered `Spliced` run sourcing,
   SHARED-MINT identity) + the `-CYCLES` cycle-surgery census, and the
   measurements corrected three designs (the "merges" are
   CROSS-INVOCATION consistency — 2 ops/case, each op repairs its own
   mesh; v142/v144 are TWO corridors sharing one endpoint mint; run
   sourcing must be chord-local). R0011 4/4 + R0074 1/1 + R0044 7/7
   corridors applyable and fully consumed; R0085 walled by the
   ALL-CONSUMED admission rule. The mutation's surgery is measured to
   the vertex (A2 hole-cycle swap; junctions host on crease mesh
   edges; base excision turning at J0; run facets cut with far-sign
   side selection). inc-2c-3b-0…3b-2 (2026-08-30/31) landed the
   corrected-cycle planner, the gated mutation (`YANG_451_TRANSIT`,
   default OFF), and the §4.4.1 ABSORB arm — **R0011 CONVERTS to
   SUPPORTED_CORRECT under the gate (the epic's FIRST conversion;
   χ=0 adjudicated TRUE genus 1, `euler_target` authored + pinned)**.
   inc-2c-3b-3…3b-7 (2026-08-31, third session) landed the fan-local
   TORUS chart (`YANG_441_TORUS_CHART`, default OFF) — **R0074
   CONVERTS (the SECOND conversion, standing oracles)** — plus the
   curve-aware host admission, the mirrored-pair planner stage, JOINT
   far regions with the arc-stitch polygon + density-capped seeded
   refill + wrap-band dispatch, and the removed-membership closure:
   R0044 advances through eleven typed walls to the B:0 base-boundary
   adjudication (spec §3o). inc-2c-3b-8 (2026-08-31, fourth session)
   REFUTED the base-leg reading by rim-domain census (the candidate
   junctions sit out-of-domain beyond the corner; the anchor census
   had converged onto the phantoms' own relocated positions) and
   landed the total-excision closure (planner fall-through + sweep
   whole-component/contained-strip arms + batch-carried seeds):
   **R0044's natural invocation now APPLIES (corridors=6 plans=20
   mints=15 removed=25)**. inc-2c-3b-9a/9b closed the
   batch boundary-conformality wall (spec §3q/§3r): the removed-union
   filter, the ALWAYS-ON survivor-testimony refill orientation
   certificate (full-corpus proven neutral), and the
   STANDING-JUNCTION certificate (a 3-face vertex within CONTRACT of
   its own triple solution is never absorbed — the absorb had been
   DELETING true junctions the relocation had already placed; both
   triple shapes incl. the far-op crease v107) — **16 → 0 unpaired
   edges, the batch is watertight, and R0044's design boolean
   COMPLETES**. inc-2c-3b-10 landed the §4.5.3 SURFACE-PAIR
   analytic-tangent arm GATED (`YANG_453_SPAIR`, default OFF: the
   always-on corpus run measured ONE E→W — R0053, M8 coplanar-graze,
   whose fold's ring rejection was a loud stop MASKING the M8 gap;
   flip = that χ adjudication or M8 Stage-0): under it R0044's
   FaceId(459) ring wall falls → FaceId(626) folded-triangulation.
   inc-2c-3b-11 (2026-08-31, fifth session) RESOLVED FaceId(626) in
   kernel-v2 (F2a spec inc-8b): the fold was a ONE-SIDED conforming
   insert on a legitimate ~304° near-sliver carried band (the
   conforming pool is EDGE-local, the fold constraint FACE-local — a
   face-627 vertex inserted on one rail only, mid-chord of the
   opposing sagging rung); the gated inc-8a curve pool
   (`KV2_ARC_CONFORM_CURVES`, still default OFF) completed to depth 1
   — pool arcs now contribute their vertex-pool inserts, R0044 being
   inc-8a's FIRST corpus customer. Face 626 tessellates clean; the
   case advances to the UNMASKED FaceId(627) ring rejection, anchored
   same session: the corner notch's SurfacePair→HyperbolaArc junction
   sits ON face 627's cone but 0.827 PAST the station of the rim that
   BOUNDS the face, landing inside the neighbour band (face 626, which
   carries no notch) — the 3b-8 identification-vs-domain shape at the
   emission layer; the chart ring self-intersects its own rim run
   twice and the CDT rejects correctly. ADJUDICATED same session by
   closed-form solve: X is the EXACT cylinder-end-circle ∩ cone triple
   point (residuals ~1e-13) — correctly computed on the EXTENDED cone
   627, 0.827 outside its domain; the true junction J lies on cone 626
   (station 2307.654, inside its band) and both crease crossings exist
   on the shared rim. inc-2c-3b-12 is a DETERMINED build: split the
   crease, re-terminate 627's two chains on it, construct the 626-side
   notch through J, and certify chain terminals against their own
   face's DOMAIN (not merely its surface). Its DETECTION half LANDED
   the same session, gated `YANG_451_TRIPLE_DOMAIN` (default OFF,
   byte-identical): the anchor is confirmed by backtrace as the
   TRIPLE-JUNCTION relocation arm, whose only acceptance gate is a
   displacement corridor R0044's 18.07 travel passes at p80 of the
   case's own 306-relocation distribution — the arm has NO domain
   postcondition, accepting any exact solution of the three EXTENDED
   implicits, which is Yang §4.5.1's stated trigger verbatim. Built:
   `crease_circle_from_pair` (Cone×Cone coaxial, Cone×Plane ⊥,
   Cylinder×Cone — circles only, everything else declines), a
   BY-SURFACE crease index (the domain belongs to the FACE, not to
   edges at the moving vertex — v47 sits 10.5 from the crease it
   overruns), and `crease_crossed_by_step` with membership exemptions
   rather than thresholds (on-crease gliding; a PROPAGATED band, since
   a derived plane is no more certifiable than its parents). R0044:
   8 material fires (0.309 … 40.08) against bands of order 1e-11 — ten
   of separation — with five crease-riding noise fires exempted; v105
   is named independently by §4-I9 and by the §4.5.4 retry. Full-corpus
   census: canonical 273C/0W/34E/1EE/0T (census mode is
   behaviour-neutral), firing in exactly TWO cases — R0044 (ERROR) and
   **R0003 (SUPPORTED_CORRECT, 6 fires, overruns 0.00078…0.265)**. A
   correct case therefore carries genuine out-of-domain relocations and
   survives them, so the STOP must never be armed as it stands, and the
   1.2× gap between the two cases' overrun ranges is refused as a
   discriminator. **inc-2c-3b-12b-0** (2026-09-01) then landed the
   REPAIR itself as a PURE solve (spec §3u): `solve_crease_transit`
   runs the paper's four steps in its order — truncate to `C_b`,
   transit onto the neighbouring surface `S1`, certify, and solve the
   q-points on `C_b` — composing four primitives that already existed,
   with an honesty postcondition that re-applies the §3t certificate to
   the corrected step (a transit leaving the NEIGHBOUR's domain in turn
   is a typed decline carrying its measured residuals, never an
   iteration). It reproduces §3s's independently-derived numbers — the
   0.138 correction and both q-points to their last recorded digit —
   and two unplanned cross-validations fell out: v38's q-point and
   v47's junction agree to 6.4e-13 inside a ~1.1e-11 evaluation band
   (one physical point by two unrelated paths), and R0003's v8658 /
   v11356 to 2.8e-14. **Census: 11 of the 14 sites are DETERMINED —
   R0044 5/8 (0.138 … 8.72, three honest declines on 17–30-unit second
   overruns) and R0003 6/6 (1.48e-3 … 3.82e-2).** That last row is what
   §3t's binding constraint asked for: the repair is determined on the
   SUPPORTED_CORRECT case too, so the two populations separate by
   fixing both rather than by a magnitude band. Census-only; default
   path untouched. The EMISSION half is 3b-12b-1, and the census makes
   it precise that it cannot be a relocation — `J` lies on cone 626 by
   construction, so it is no more inside 627's domain than `X` was; the
   repair is a RE-TERMINATION of 627's chains at the q-points (which
   are ON the crease bounding 627), a crease split there, and 626's
   notch through `J`. **inc-2c-3b-12b-1** (2026-09-01, same session)
   then MEASURED what the mesh actually has at those sites, pure and
   census-only (`transit_site_anatomy`, `YANG_451_TRANSIT_ANATOMY`;
   spec §3v): the fan with per-triangle input-face attribution, the one
   ring classified Home/On/Past, and for each q-point the crease-carrying
   mesh edge nearest it with that edge's length and the q-point's sag
   off it. Three readings across all 11 determined sites. (a) The
   anatomy is ONE shape eleven times — every fan straddles EXACTLY
   three input faces, and for v47 the attribution names the two chains
   directly. (b) The `Past` one-ring neighbours are already-relocated
   SIBLINGS (v39's ring carries v38 at its own recorded `d_post`), so
   R0044's v38/v39/v59 is a cluster and the repair unit is the cluster.
   (c) **The q-points' REPRESENTABILITY splits the population
   structurally** — the discriminator §3t rightly refused to take as a
   magnitude band. In 3 sites (R0003 v1983/v8658/v11356) both q-points
   ARE existing one-ring vertices to 7.6e-15 … 1.7e-12, so the repair
   is the relocation and nothing else; in R0044 v47 the crease is
   carried but as a 558.53-long rim chord with the q-points 10.39/10.36
   off it at mid-chord; in the remaining 7 the crease has no local mesh
   chain at all (v38/v39: nearest 497.9 away). So the emission half is
   THREE builds, not one. Charting `FaceId(627)`'s emitted loop in its
   own cone frame also showed the notch spans the FULL band height
   (both ends exactly on the lower rim) and exits through a 2.285-arc
   window — reproducing §3s's independently measured 2.29 — so the
   corrected emission CUTS the 304.56° sector in two rather than
   denting it, and that split falls out of `flood_fill_patches` once
   the mesh is re-attributed rather than being constructed.
   **inc-2c-3b-12b-2** (2026-09-01, same session) then turned that into the
   EDIT, still pure (`transit_cut_path`, spec §3w): the cut the crease makes
   ACROSS the site's own patch, from one chain termination to the other. Its
   first model required two chains and declined at all 11 sites — the corner
   has THREE, and they differ in role: two involve the own surface and
   terminate at the q-points, the third joins the two OTHER surfaces and is
   the CARRIER the site glides along (for v47 it is the cylinder's own end
   circle, of which `X` and `J` are the cone-627 and cone-626 intersections).
   With the corrected model **7 of 11 sites yield a determined cut** and the
   remaining 4 are exactly the `Past`-neighbour cluster sites — a clean
   partition with no third failure mode. Two measurements came out of it:
   assigning a chain to its q-point by PROXIMITY is wrong at every site where
   both chains are crossed edges (margins −1.855, −0.0525, −0.361, −0.775,
   because both chain edges leave the same site), so the rule is surface
   IDENTITY with a typed decline when a face does not resolve; and the cut
   has ONE shape everywhere — `q → (Vertex|Refined)* → q` with EXACTLY ONE
   refinement crossing, whose lift reproduces §3v's independent chain-sag
   reading (10.181 at v47 against 10.39/10.36 for the same rim chord).
   R0044's gate set (FOUR knobs): `YANG_451_TRANSIT=1 YANG_441_TORUS_CHART=1
   YANG_453_SPAIR=1 KV2_ARC_CONFORM_CURVES=1`. Also open: the v105
   retry ChordDegradation. R0085 stays walled on operand quality.
   Then inc-3: full-corpus gated measurement, two-proof flip.*
5. **Oracle increments**: cut-op coverage (138 not-covered cases), then
   promote the oracle into the categorized assay for all-boss cases so the
   deficit class cannot re-hide. Mask retirements (meta monotonicity into the
   categorized runner; the advisory downgrade) each need a census first.

### 0.0 Compliance endgame — PLAN OF RECORD (committed 2026-07-16)

**Thesis:** the kernel is Yang-compliant in architecture (all six stages exist;
the exact Stage-2 core is native and sidecar-parity-certified). What remains is
the **junction layer** — the seam where exact mesh geometry meets analytic
B-Rep geometry at places where curves meet: grazing corners, near-duplicate
junction verts, degenerate seams, missing intersection curves. Every recent
refutation (#168 R0038, #169 Phase B ×2, N54) points there. The paper is thin
on multi-surface junction assembly; closing it means building what the paper
implies and signing the extensions into the deviations ledger.

**Definition of done (measurable):**
(a) OPEN deviation count = 0 (`docs/yang_deviations.md`: N2 — the sole
remaining OPEN entry since N6's user-ratified 2026-07-17 closure as
detection-shipped; N2's remit now includes the §4.5.4 removal half);
(b) every corpus case is CORRECT or a signed-off scope boundary — 0 WRONG, no
unexplained ERRORs; (c) Cherchi sidecar parity stays green; (d) everything else
in the ledger is PERMANENT with user sign-off.

**Phases (dependency-ordered; baseline 240C/0W at 6d6141ef):**

1. **Triage ledger** — `docs/yang_tail_triage.md`: every failing case gets a
   confirmed root cause + fix vehicle BEFORE machinery is built against it.
   Case-first is the standing discipline: no wiring against an unconfirmed
   bucket. *COMPLETE 2026-07-18 (#171 pass 2): the PROBE queue is EMPTY —
   26 → 14 (pass 1, 2026-07-17) → 0. Pass 2's targeted digs (new env-gated
   `YANG_LRR_SITE` / `YANG_TORUS_STOP` / `CHERCHI_PATCH_PROBE`
   instrumentation) split the tail into sharply-defined classes: the R0044
   surface-pair endpoint-mix bucket grew to 4 cases (R0020/R0035/R0070-op2),
   the torus pair-Newton/containment family to 6 (R0015/R0026 REFUTE N51's
   "no-curve-type" — they are C0065-class containment STOPs at micro scale),
   S5/S6 output-ring assembly defects account for the CDT ring-rejects
   (F0045 is a FIRST-boolean mint, refuting the chained-input theory;
   R0016's ring carries the #146 near-dup spikes), a missing cone-generator
   LineSegment closed form is a 2-case quick-win (R0008/R0085-op2), R0053 +
   R0050 expose a Stage-2/3 incidence gap between near-coincident revolve
   surfaces, and C0043/C0056/C0046/C0075 are designed degeneracies whose
   loud STOPs are the correct posture (sign-off candidates). See the
   ledger's pass-2 section + rollup for the full map.*
2. **Analytical completeness (M5)** — finish the `ssi-rs` degree-4 matrix
   (torus×torus, cyl×cyl lateral∩lateral). Goes first among builds because
   Stage 4 can only relocate onto curves that exist, and downstream junction
   fixes need the true curves. Confirmed customers: R0044, R0096.
   *torus×torus half SHIPPED 2026-07-17 (#172 increment 1): the Stage-4
   torus-block scope lift admits a second torus as an implicit-pair partner
   (base = first torus at the vertex, `or_insert`-stable), so torus×torus
   lateral edges and torus×torus×plane junctions relocate through the
   EXISTING pair/triple Newton — no new curve type needed (P8 procedural
   model). R0096 ERROR→CORRECT. The probe also REFUTED R0044's N52
   torus×torus diagnosis: its v11 is a cylinder×cone `SurfacePair` endpoint
   that is also a conic endpoint (surface-pair endpoint-mix STOP) —
   re-vehicled to the phase-3 junction layer in `docs/yang_tail_triage.md`.
   cyl×cyl half SHIPPED 2026-07-17 (#172 increment 2, spec
   `specs/yang_172_case_iii_graze_guard.md`): the Case-III graze guard —
   the exact mirror of the shipped Case-IV phantom guard — detects
   cross-operand cylinder pairs whose surfaces intersect at a penetration
   the chord meshes never sample (Yang §4.2.1 Fig. 8 Case III) and
   rebuilds both operands at the derived rim N (`sag_a+sag_b ≤ depth/2`),
   so the SHIPPED SurfacePair Stage-3/4 machinery refines the wedge.
   C0116 ERROR→CORRECT (the #173 gate's root fix). Scope lines, all
   derived: phase-aware filter (natural meshes already touching = not a
   Case-III miss, byte-identical — C0057's vertex-phase sliver);
   render-observability floor (`depth ≤ 2·1e-3·(r_a+r_b)` cannot be
   represented at any output resolution → status quo, routed to the
   phase-3 §4.5.2 LOCAL refinement); sub-sagitta STOP (depth above
   authoring noise but below the rim-N cap ⇒ typed
   `SubSagittaGrazeIntersection`, new designed-ERROR corpus case C0118,
   corpus 311→312). M5 is CLOSED as a phase; the sub-render near-tangent
   residue (C0057-class unfused shell-credited lenses) is phase-3
   §4.5.2/P3d territory.*
3. **The junction layer (closes N2)** — design grounded by the 2026-07-17
   research session: **`docs/yang_junction_research_findings.md`** (read it
   before writing any P3 spec; its "junction contract" is binding: mint once
   exactly, share by identity/handle, multiplicity is a loud STOP, refinement
   is only a guarded shell):
   a. **#146 conformal junction sampling at Stage 1** (RE-SCOPED per findings
      Q4: the near-dup mint is Stage-1's independent non-conformal sampling
      near shared junctions, NOT Stage 2/3; the exact arrangement is
      exonerated, N48 sidecar-certified). Faces incident to a shared curve use
      the SAME boundary sample points; corners inserted once into both meshes.
      Non-goal: no new tolerance merge (R0091 hazard) — existing STOPs stay.
      *STATUS 2026-07-18 (spec `specs/yang_146_conformal_junction_sampling.md`
      §4, commits fb3ecde8/c16a8d51/9c368672/1284f062): increments 0–2
      SHIPPED — pierce enumeration, Stage-1 edge-polyline + face-interior
      insertion, full wiring banked behind `YANG_JUNCTION_SAMPLING_ENABLE`
      (gate-OFF byte-identical, assay-verified). Gate-ON measurement: 0
      WRONG; the MECHANISM IS PROVEN (F0082's v588/v601 near-dup mint
      eliminated at the site), but no P3a bucket case converts (multi-defect
      chained models) and F0016/F0084 flip CORRECT→ERROR via the I6
      coincident-tri guard: the arrangement mints sub-weld-band crossings
      from near-parallel CHAINED-operand residue (minted in-boolean, NOT
      inherited B-Rep twins — twin-origin probe negative), and the insertion
      re-rolls triangulations so the §4.3 weld fuses them coincident.
      Increment 3 (always-on) is gated on characterizing/resolving that
      chained near-parallel crossing mint — next increment: per-vertex
      crossing provenance at the I6 site (probe-first, plan-of-record
      discipline).*
      *CROSSING PROVENANCE CONFIRMED 2026-07-18 (probe
      `CHERCHI_VERT_PROVENANCE` — exact per-vertex generator provenance +
      exact rational pair separation at the arrangement level, joined to
      the I6 site via the `NONMANIFOLD_SITE_PROBE` i6-cluster arm):
      "near-parallel" REFUTED (crossings well-conditioned, sin 0.36–0.70);
      arrangement-dedup-gap REFUTED (d_exact ≈ 1e-18 > 0 — genuinely
      distinct exact points, upstream-faithful). Real class = flush/chained
      operands carry INTENDED-EXACT contacts at sub-weld f64 residue:
      shared corner vertices grazed by CDT edges at 1e-18 (F0016) and a
      vertex-on-face contact at 5e-15 spawning an LPI fan (F0084). The
      insertion AMPLIFIES the pre-existing class (3 → 33 sub-weld pairs on
      F0016) by densifying edges near junction corners; the I6 weld then
      collapses slivers into coincident-tri pairs / over-used edges.
      Next increment (spec-first): post-weld collapsed-wedge resolution —
      exact structural dedup of same-winding same-label tri pairs that
      share a raw edge with weld-fused tips; a4-class genuine coincident
      faces still STOP. See spec §4 "Blocker (1) CHARACTERIZED".*
      *Increment 3a SHIPPED same day (spec
      `specs/yang_146_collapsed_wedge_dedup.md`, always-on): the I6-site
      collapsed-wedge dedup. Locality arm corrected by measurement:
      same-B-Rep-face via `tri_face` maps (parent-tri adjacency REFUTED —
      the strip edge is intersection-minted). F0016 gate-ON →
      SUPPORTED_CORRECT; gate-OFF corpus behaviorally unchanged (0 dedup
      fires; sole delta = F0090 timeout flake); gate-ON 250C/0W/56E/2T,
      regression set {F0016,F0084,F0085} → {F0084}. Remaining inc-3
      blockers: F0084's edge-level over-use shadow at reassembly, then
      the bucket models' next defect layer.*
      *F0084 shadow ROOT-CAUSED + FIXED 2026-07-18 (task #179, spec
      `specs/yang_stage1_cdt_parity_flap.md`) — framing CORRECTED: the
      over-used edge entered on the OPERAND meshes, not at the weld.
      Stage-1's all-segment planar CDT was the last production caller of
      the f64 centroid-parity classifier, which on near-collinear
      boundary triples keeps an exterior zero-area FLAP triangle →
      non-2-manifold Stage-1 meshes in BOTH gate states (production
      survives by downstream luck; junction insertion amplifies the
      class). Fix = flood-fill classifier migration (the F0047 fix the
      curved path + kernel-v2 already had). Measured: gate-OFF
      251C/0W/55E/2T (zero regressions; F0082's Extrude-7 failure also
      fixed by it); gate-ON 251C/0W/55E/2T **category-identical
      per-case to gate-OFF** — the P3a gate-ON regression set is {}.
      Remaining inc-3 blockers before always-on: (1) the insertion
      rebuild still mints NON-CONFORMAL operand meshes gate-ON (near-dup
      ~0.003 T-junction pairs, `i6-input-overuse` probe; no case fails
      today but it is an axiom violation) — characterize + fix, or add
      the loud rebuilt-operand 2-manifold postcondition; (2) the bucket
      models' next defect layer (F0082 Extrude-11 ring-reject class).*
      *Blocker (1) FIXED 2026-07-19 (task #180, spec
      `specs/yang_146_keep_interior_floodfill.md`) — framing CORRECTED
      again: the insertion machinery is fine; every gate-ON imbalance is
      ONE EXTRA SLIVER between a split edge polyline and its un-split
      chord, kept by the f64 centroid parity classifier inside
      `cdt_polygon_with_holes_keep_interior` (the CDT every
      interior-junction face routes through) — the #179 class in its
      keep-interior guise. Fix = the same flood-fill migration, applied
      to BOTH interior-capable variants (`keep_interior` + the N2
      `cdt_with_interior_constraints`); bit-exact red→green fixtures at
      the cherchi-rs primitive level and the yang-rs Stage-1 level
      (`p3a_insertion_conformality.rs`). Measured: F0084 gate-ON
      `i6-input-overuse` fires ZERO times, SUPPORTED_CORRECT; gate-OFF
      corpus category-identical (250C/0W/55E/3T — sole deltas the known
      F0072/F0085/F0090 timeout flakes); gate-ON 251C/0W/54E/3T, 0
      WRONG, regression set stays {} (R0019 E→T under doubled assay
      load re-verified: loud ERROR at 90.6s CPU with headroom; F0090
      flake landed CORRECT). Remaining inc-3 blocker = (2) only, the
      bucket models' next defect layer. Known residue (out of scope):
      `cdt_polygon_with_holes_refined` (render channel) still
      classifies HOLES by f64 centroid parity.*
      *Blocker (2) CHARACTERIZED + RE-CLASSIFIED to P3b (2026-07-19,
      task #181, measured — spec §"Blocker (2) CHARACTERIZED"): F0082
      Extrude-11's ring-reject is a curve-endpoint CORNER defect minted
      by the failing union itself — the output section-Ellipse arc
      terminates at a relocated chord-crossing vertex at t≈π/2 (on-curve
      to 4e-16) instead of the never-minted ellipse × wall-plane corner
      junction 2.76e-3 away along-curve, so the face ring
      self-intersects and the #173 render gate STOPs (identically in
      both gate states). P3a cannot reach it (intersection-curve ×
      boundary-edge junction = the phase-b corner stitch below). Inc-3
      always-on is NOT gated on F0082; P3a's remaining gate is the
      standard ledger (recovered cases + 0 regressions + sidecar
      parity). Probes banked: `KV2_OUT_VERT_PROBE` (output-B-Rep vertex
      + incident edge/face dump in `kernel_v2::boolean_op`),
      `KV2_RING_REJECT_PROBE` now also dumps the 3D ring.*
      ***Increment 3 SHIPPED 2026-07-19 (task #182): P3a junction
      sampling is ALWAYS-ON in production.** Ledger: gate-ON full assay
      251C/0W/55E/2T, category-identical per-case to the gate-OFF
      baseline (zero diffs — corpus-neutral, mechanism-superior);
      sidecar parity green gate-ON (r0046 + stage0 inputcheck + the
      18-case flagship suite); yang-rs lib suite green on the flipped
      default. `YANG_JUNCTION_SAMPLING_ENABLE=off|0` remains as a dev
      A/B knob (compliance-ledger measurement, `weld_enabled` pattern);
      `=edge|face` stay as diagnostic halves. **#146/P3a is COMPLETE**;
      the junction layer continues at P3b (F0082 corner stitch, below).*
   b. **#137 grazing-corner insert + stitch** — the proven triple-junction
      primitive (N-137.1, stronger than anything in the literature per
      findings Q1) gets the Urick-style stitch: mint the corner ONCE, insert
      into both operands as one shared arrangement vertex, split both incident
      chains at it. First validated wiring point for the banked two-sided
      conformal driver + `SurfaceChart`, under the findings-Q2 seam contract
      (one canonical seam polyline, constrained input to BOTH CDTs).
      *P3b SPLIT BY MEASUREMENT 2026-07-19 (spec
      `specs/yang_169_p3b_curved_partner_pierce.md`, increment-0 probe
      `YANG_P3B_PIERCE_PROBE`): the F0082 ellipse×wall corner is NOT a
      grazing corner — it is a TRANSVERSAL (0.474) line-edge × cylinder-face
      pierce, enumerable by the P3a Stage-1 mint mechanism: operand A's wall
      edge 2424 (planar-incident, already in the owner channel scope) ×
      operand B's cylinder face 2 at t=0.767 reproduces the inc-3c true
      corner to 9 decimals; the t=0.232 root covers the arc's other end
      (the v915 near-dup region). Fix = widen P3a's partner scope to
      canonical-tube cylinder faces (quadratic pierce + axial containment +
      exact-bits 3-fan grid insertion), increments 1–4 in the spec. The
      Urick stitch + refinement remains for the genuinely tangential class
      (C0065/R0074, the #137 spec).*
      *P3b STATUS 2026-07-19 (increments 0–4b done; spec §5 is the source
      of truth): pierce primitive + tube-grid insertion + gated wiring
      SHIPPED (inc-1..3, `YANG_P3B_PIERCE_ENABLE`); gate-ON F0082 carries
      the exact minted corner (mechanism proven). inc-4a SHIPPED always-on:
      moved×minted §4.3 weld + orbit boundary-cycle extraction (legacy
      fallback at tangency 4-sheet edges). inc-4b SHIPPED always-on: the
      beyond-corner conformal trim — mesh-edge-adjacent moved×mint pairs
      collapse when the sample lies beyond an owner plane whose OP-RESOLVED
      zero-content verdict fires (Union: reflex; Subtract: base-convex /
      tool-reflex; Intersect: convex; Xor: never), on the other owner
      plane, within the chord corridor, and patch-subset-safe. Ledger:
      gate-OFF 250C/0W category-identical (F0090 flake); gate-ON R0061
      trims 19 phantoms but STOPs one layer deeper (over-used
      minted×minted edge with same-winding near-dup-tip slivers = inc-4c,
      the flip blocker); gate-ON F0082 REFUTED as a lone-phantom case —
      it is a 3-junction micro-complex over a ~1e-4 near-coplanar cap
      (J2 = cap-rim×wall CURVED-owner pierce + J3 = tube∩cap∩top triple)
      → inc-4d curved-owner/rim-corner widening is its named vehicle.
      inc-4c SHIPPED 2026-07-20/21 (fan re-CDT + seam-order
      canonicalization + §4.3.4 chain-sample drop; R0061 gate-ON
      CORRECT, that flip blocker cleared, N58 resolved). inc-4d SHIPPED
      2026-07-21 (circle-rim×planar-face pierce + rim-ring channel with
      opposite-rim mirrors + all-overrides composition; gate-OFF
      byte-identical): gate-ON F0082's J/J2 corner assembles EXACTLY
      (phantom gone) and the STOP moves to the named J3 layer —
      rim×section-ellipse OSCULATION on the tube (tangential edge×edge,
      #137-family / §4.3 sliver, spec §7.6). inc-4e SHIPPED 2026-07-21
      (task #186, spec §7.7) — ALL THREE flip blockers resolved:
      (1) C0102/C0103 → the §3.3 "2+2 edge-split fan" arm (within weld
      band of a grid EDGE → split both incident triangles; vertex-band
      stays loud) — both SUPPORTED_CORRECT gate-ON; (2) R0091's "silent
      χ=−4" REFUTED as a kernel defect: the output χ=−4 (genus 3 — the
      tilted cut tube leaves 4 corner pillars) was VERIFIED via Cherchi
      sidecar reference parity on the exact operand meshes + an
      independent voxel-CSG derivation from the authored numbers; the
      meta euler_target=2 (naive 3-op default) was the authoring error,
      corrected → R0091 gate-ON SUPPORTED_CORRECT (gate-OFF keeps its
      honest merge-budget LRR STOP). The spec-§3b ranked merge survivor
      (`sub_feature_merge_direction`, Yang Fig. 11(b)) is now WIRED
      always-on (its bank condition was exactly this verification);
      r0009/r0091 campaign trackers un-ignored. (3) F0082 J3 = honest
      STOP in BOTH gate states (flip-neutral), stays the named
      #137-family follow-up. Flip (inc-5) is now gated only on the
      standard ledger (gate-ON ⊇ gate-OFF correctness, 0 WRONG).
      inc-5 SHIPPED 2026-07-21 (task #187, spec §7.8): the pierce arms
      are ALWAYS-ON in production; `YANG_P3B_PIERCE_ENABLE=off|0` is a
      dev A/B knob (the P3a inc-3 / `weld_enabled` pattern). The flip
      exposed one defect OUTSIDE the corpus, fixed pre-flip: when both
      rims of a lateral pierce the same wall, each rim's own mint and
      the opposite rim's azimuth-mirror (ulps apart, never bit-equal)
      both survived the bitwise cross-mirror dedup → sub-weld ring
      near-dups → needle triangles → loud NonManifoldInput on the n2
      near-tangent fixture; fixed by deferring mirror placement after
      all own mints with band dedup (`TAU_MODEL·(1+scale)`, own mint
      wins; azimuth-preserving projection keeps rings 1:1). NEW
      PRODUCTION BASELINE: 252C/0W/55E/1T; committed results.json is
      the arms-on ledger (sole category deltas vs inc-4e gate-OFF:
      R0091 ERROR→CORRECT + the F0085/F0090 flake pairs; R0016/F0082
      within-ERROR detail drifts characterized in spec §7.8). P3b's
      remaining named follow-up = the F0082 J3 layer — RE-CHARACTERIZED
      2026-07-21 (task #188): NOT osculation-stitch/#137-family but a
      Stage-5/6 boundary-envelope selection defect at the antipodal
      ellipse↔rim triple point (already minted); plan of record =
      `specs/yang_188_f0082_j3_envelope_selection.md` (pierce spec §7.9
      has the measurement).*
      *#188 COMPLETE — FLIPPED ALWAYS-ON 2026-07-22 (commits
      57d7b93b→a298d544, spec §§10.7–11): kernel-v2 pinched-ring
      shared-vertex canonicalization (always-on); §3.3 envelope
      selection + §10.5 neighbor propagation; §10.8 notch seal patches
      as standalone CAVITY-SENSE faces (the inner-loop form escaped
      the outer cycle and spliced a phantom handle); §10.9 band-conic
      typing of rewritten-chain rim chords (the fired-patch slice of
      the #158/F6 migration). F0082's flagship union (Extrude 11)
      FIXED IN PRODUCTION — the corpus baseline detail moves to
      Extrude 12, whose two defects are OUT of #188 scope (spec
      §10.10): the M8 coplanar-residue family (#130) and a
      sub-TAU_WORK edge-connected arrangement twin from operand
      self-grazing (new spec needed; candidate = collapse
      sub-TAU_WORK mesh EDGES, sparing KV9's unconnected ring
      duplicates). Ledger 312/312 category-identical, 0-WRONG; parity
      18/18; detector NOT promoted (§11).*
   c. **Curved-seam re-CDT** (R0072 class) and whatever the triage promotes.
      *R0044 SURFACE-PAIR ENDPOINT-MIX BUCKET CLOSED always-on 2026-07-28 —
      corpus 255C/0W/55E/0T → **256C/0W/54E/0T**, exactly one category delta
      (R0035 ERROR→CORRECT), zero CORRECT→ERROR, zero WRONG. Probed first, and
      the probe killed the framing: every vertex in the bucket (R0044 v8/v12
      `{cyl_A, plane_B, cone_B}`, R0020 v44 `{plane_A, cone_A, cyl_B}`, R0035
      v194/195 `{cyl_A, cyl_B, plane_B}`) has EXACTLY 3 incident surfaces —
      the plain triple point the increment-5 conic triple-junction block
      (`stage4_correct.rs`, `relocate_onto_implicit_triple`) already resolves.
      The bucket existed only because that block's candidate set enumerated the
      six CONIC maps and not `vert_surface_pair`: an ellipse × surface-pair
      junction scored `n_maps == 1`, fell out of the block, and reached the
      surface-pair loop's `endpoint_set` guard as the "mixes closed-form and
      implicit-pair relocations — out of v1 scope" STOP. A procedural
      surface-pair curve is a curve through the vertex exactly as a conic is;
      it is held apart from the conic bookkeeping only because it has no
      parameter `t`. **The mix was never the difficulty, and "out of v1 scope"
      named a case the code could already do.** Only R0035 was a
      single-defect model: R0044 and R0020 now STOP at a PURE surface-pair
      `relocate_onto_implicit_pair` divergence (new M5 pair-Newton vehicle,
      kin to the torus `pair_newton_none` trio), R0020 fatally at a kernel-v2
      `surface-pair refinement needs a positive finite chord tolerance` on an
      output `Curve::SurfacePair` edge, and R0070 raises no LRR at all. All
      re-vehicled in `docs/yang_tail_triage.md`. Sites were located with a
      temporary `#[track_caller]` shim over every `LocalRefinementRequired`
      construction — worth rebuilding rather than guessing next time.*
      *Stage-4 CONE-APEX GENERATOR arm SHIPPED always-on 2026-07-28 — the
      triage's one self-contained "small closed-form vocabulary gap"
      (`docs/yang_tail_triage.md`). Corpus 254C/0W/56E/0T → **255C/0W/55E/0T**,
      both runs same-session back-to-back; **exactly one category delta,
      R0008 ERROR→CORRECT, zero CORRECT→ERROR, zero WRONG.** The "missing
      closed form" diagnosis was WRONG: `ssi_rs::plane_cone` has emitted the
      through-apex `SsiCurve::Line` all along and Stage 3 has banded it via
      `cone_chord_tol_for_owner` since PR-YR17. Both gaps were Stage-4 WIRING —
      (a) the `Curve::LineSegment` pair match binned `Surface::Cone` as
      `other_curved` and STOPped before selection; (b) once admitted, the
      tie-break called the R0072-only `select_disjoint_parallel_line`, whose
      parallelism precheck rejects the two CROSSING apex generators. **(b) is
      the more instructive half: N45 (#163, 9fca8393) generalized Stage 3 to
      the parallelism-free `select_disjoint_line_by_distance` and did not
      update this site, so the two stages ran DIFFERENT tie-breaks for two
      weeks under a comment asserting they used "the SAME rule".** A shared
      rule stated in prose is not a shared rule; the wrapper's own doc even
      named the generalization it was not being asked for. Three cases were in
      this class, not the ledger's two — R0081 sat under `#153 / SUSPECTED`,
      unprobed, and surfaced only when re-probing after the fix showed the
      identical `lineseg_combo` cone×plane site. R0085-op2 and R0081 advance a
      layer and stay ERROR (torus×line endpoint-mix; Stage-6 non-2-manifold).
      Test: `crates/yang-rs/tests/cone_apex_generator.rs` — two frustum
      fixtures, each red-verified against its own half of the fix (a 45° cone
      does NOT reach the tie-break; only a near-flat one puts both generators
      inside the band).*
   d. **§4.5.2 as guard shell only** (findings Q3: it recovers ~zero current
      cases — every confirmed LRR case is tangential/missing-solver/micro-
      feature): transversality entry gate, per-pass strict-decrease monitor,
      budget, watertight-gated output — so refinement can only STOP, never
      silently accept. *Q3 RE-CONFIRMED by direct measurement 2026-08-29
      (`specs/yang_452_local_refinement.md`): the post-I13f family census —
      uniform 2×/4×(/8×) refinement over all 10 `Stage4RegionInvalid` cases —
      converts zero. The existing typed STOPs are the guard shell; no
      recovery loop gets built against this tail.*
   N2 closes as the sum of these wirings, not as an abstract epic.
4. **N6 detector-first** — §4.5.4 illegal-self-intersection detection as a new
   loud STOP class; removal comes after. *DETECTION SHIPPED 2026-07-17 (task
   #173, spec `specs/yang_173_selfx_detector.md`) with a measured CORRECTION
   to findings Q5: the exact mesh-level test and the C0116 silent-wrong class
   see DISJOINT defects. The exact non-adjacent tri–tri test on the Stage-4
   mesh fires on 53 cases (33 CORRECT) of relocation-minted seam
   chord-crossings — the §4.5.4 artifacts whose remedy is REMOVAL (#169
   increment 2), so it is banked as the `YANG_SELFX_PROBE` diagnostic (its
   fire-list is the removal worklist), NOT a STOP. The production STOP is
   kernel-v2's render-resolution boolean-output gate (`validate::selfx`, a
   semantics-identical port of the corpus-calibrated assay oracle): sub-
   sagitta B-Rep-level penetrations are only observable where the true
   surfaces are sampled finely. Converts C0116 AND C0105 WRONG→ERROR.
   Removal (increment 2) routes into the phase-3 mesh-update loop.
   **N6 CLOSED 2026-07-17 by user ratification** ("close as
   detection-shipped, removal tracked under N2") — this phase is DONE;
   the removal worklist lives in N2/#169.*
5. **Capability tails** (interleavable): M8 coplanar residue (#130) +
   rim-projection (#144), KV6 revolve leftovers, non-convex/curved profiles,
   #153 NonPlanarFace wall.
6. **On-surface (`VertexOffSurface`) tail — NEW 2026-07-28.** kernel-v2's
   geometric tripwires (loop vertex on its face's analytic surface, at
   `import_band`) were `cfg(debug_assertions)`-only, so the `--release` assay
   was BLIND to them. Now compiled into the corpus build via the
   `kernel-v2/strict-validation` feature (`crates/test-harness/Cargo.toml`;
   `docs/TESTING.md`). **The honest baseline is 252C/0W/58E/0T** — three cases
   that had been silently passing are now loud:
   - **F0083** — Extrude 3 auto-union, `VertexOffSurface` face 388.
   - **R0027** — fails in **`revolve`**, face 654: NOT a boolean defect, so
     this tail is not yang-only.
   - **R0099** — `boolean_subtract`, face 18. (Note its history: the earlier
     R0099 WRONG was resolved as an oracle-authoring error; this is a
     different, real defect.)
   The measured class so far (spec-level, from #195): **one intermediate
   cylinder-patch loop vertex left at its Stage-1 CHORD position while both
   azimuthal neighbours are exact** — same axial height, residual ≈ the chord
   sagitta of the neighbours' angular span. It is never relocated because
   `build_intersection_curves` skips same-input rim edges
   (`stage3_ssi.rs:534`), so Stage 4 never claims those vertices. The fix is
   §4.4.1 relocation of overlay-inserted rim vertices onto their analytic rim
   (Fig-11(b) "boundary curves map to boundary curves") — i.e. the #169
   mesh-updating remit. Diagnose with `KV2_OFFSURF_PROBE=1`, which dumps the
   offending point AND its whole loop (the neighbours are what identify the
   class).
   *SPLIT 2026-07-29 (`docs/yang_tail_triage.md` §"strict-validation
   VertexOffSurface tail"): the tail is TWO classes. (1) **Validator false
   positives at coordinate scale** — the canonical strict bands sat BELOW the
   f64 evaluation floor `8·ε·L`, so a 1-ulp-exact vertex failed; fixed by
   `validate::eval_floor_linear` max'd into all six canonical-band sites.
   **R0027 CONVERTED (ERROR→CORRECT), corpus 256C→257C/0W/53E/0T, exactly
   two deltas, zero regressions**; R0025 peeled to a ring-reject. (2) **Real
   off-surface defects** — F0083 (2.3e-3) and R0099 (8.65e-2, 2.8% of
   radius): the on_both gate (deviation N10) skips true intersection edges
   whose endpoint drifted, and NO edge-local or chain-level discriminator can
   exist (six refuted — `specs/yang_s3_intersection_edge_provenance.md` §2).
   Fix = N10's named durable target, arrangement-side intersection-edge
   provenance; spec inc-0 written, inc-1 = cherchi-rs constrained-edge marks
   → `LabeledArrangement::intersection_edges`.*
   *LANDED 2026-07-30 — inc-1 (harvest, corpus byte-identical) + inc-2
   (provenance-first classification + witness selection + certificate
   relocation) SHIPPED ALWAYS-ON: **F0083 and R0063 ERROR→CORRECT,
   259C/0W/51E/0T, zero regressions**; F0082/F0085 advance ops. R0099
   measured NOT this class (zero constraint edges at its failing op) —
   its 8.65e-2 vertex needs its own probe.*

**Continuous:** the verification substrate ratchets — assay 0-WRONG gate, weld
delta (`YANG_WELD_ENABLE=all` vs prod may only shrink), deviations OPEN-count
ratchet, sidecar parity, resolution-sweep for any "finer mesh?" question.
**Assay coverage grows with the scenario space** (user directive 2026-07-17):
whenever a phase names a scenario class the corpus does not exercise, add assay
cases for it BEFORE (or with) the fix. *Audit + first tranche DONE 2026-07-17
(task #176, spec `specs/assay_junction_scenario_corpus.md`): the six charter
classes were audited against the 295-case corpus and the gaps landed as
Group 7 (C0102–C0117, corpus now 311). Corrections to the charter's suspicions:
cyl×cyl lateral∩lateral was ALREADY covered (C0051–C0058; 6 of 8 pass
chord-accurately today — M5 #172 owes them analytical refinement, not
existence); sphere∩plane∩plane transversal was covered (C0067). Real gaps
closed: grazing corners beyond the torus (C0103 cyl, C0104 sphere), curated
cone∩plane∩plane (C0105), curved×curved×plane cap-through-crossing corner
(C0106), sphere tangencies / curved 0D-1D contact (C0107–C0110), micro-feature
ABSOLUTE-scale sweep incl. the TAU_MODEL rung (C0111–C0113), zero-thickness
results (C0114–C0115), #173 red-phase hazard fixtures (C0116–C0117).*

*All four #176 silent-wrong exposures are now loud (2026-07-17): #173
converted C0105/C0116; task #178 (spec
`yang_178_subres_coplanar_gap_stop.md`, deviation N57) converted
C0111/C0113 via the Stage-0 sub-resolution coplanar-gap STOP — a matched
cross pair of two genuinely DISTINCT parallel planes (offset gap above the
rounding-noise class `TAU_WORK·(1+scale)`; corpus-measured legitimate
femto-twin max 2.7e-12, ≥40× below the line) rejects with typed
`SubResolutionCoplanarGap` before any overlay work. Committed baseline
**261C / 0 WRONG / 47E / 0T on the 312-case corpus** (2026-07-31,
amendment-18 congruent-rim cross-solid table ELECTION ALWAYS-ON, spec
`m8_stage0_multiclass_cavity_arm.md` §16 — **C0048 ERROR→CORRECT**, its
whole canonical wall chain (azimuth-merge 68v67 → cherchi DegenerateTpi)
retired by the amendment 16→18 coherence arc; prior rungs 260C/48E at
amendment-17 (sub-band lift absorption; F0067 clears cherchi to the
§4.5.2 wall) and 260C/48E at
amendment-16 group-atomic mint-collapse revert ALWAYS-ON, spec
`m8_stage0_multiclass_cavity_arm.md` §14 — the C0048 #144 azimuth-merge
count wall structurally DEAD and F0067's desync-manufactured N17 deferral
gone, typed coplanar tail 3→2 with F0064/F0072 the genuine remainder;
prior rungs 260C/47E at the 2026-07-30/31 amendments 14+15 — R0099
ERROR→CORRECT via the Fig-11(a) vertex-inserting split, F0064
ERROR→UNSUPPORTED via the open-link pure-SLIDE splice + settle order cert;
259C/51E at 2026-07-30, F0083 +
R0063 ERROR→CORRECT via the always-on Stage-3 intersection-edge PROVENANCE,
`specs/yang_s3_intersection_edge_provenance.md`; prior rungs 257C at the
2026-07-29 R0027
ERROR→CORRECT via the `eval_floor_linear` strict-validation floor,
256C after the R0035 triple-junction surface-pair count, 255C after the R0008
cone-apex arm, 254C at the #195 inc-5 + §4.4.1 inc-2 flip, 252C honest
strict-validation baseline; before the strict-validation re-basing:
**255C / 0 WRONG / 54E / 1T** at 2026-07-22, task #130
DegenerateLoop-duo retirement — UNSUPPORTED(coplanar-boolean) tail EMPTY,
R0007/R0071/F0069 recovered; prior rung 252C/0W/55E/1T at the #169 P3b/#188
flips; before that
250C / 0 WRONG / 55E / 3T on the 312-case corpus, 2026-07-17, after the
#172 Case-III graze guard: C0116 ERROR→CORRECT, new designed-ERROR C0118;
F0072/F0085/F0090 are 120s-budget-borderline TIMEOUT artifacts — F0090
solo-verifies SUPPORTED_CORRECT at 115.6s vs a 115.4s pre-guard solo, i.e.
the guard costs it ~0.1%; both states loud. Prior rungs: 250C/57E on 311
after the #172 torus×torus lift; 249C/58E before it) — DoD (b)'s 0-WRONG
clause holds corpus-wide and the #174 ratchet can bind the whole corpus.*

**Expectations:** phase 3 is the multi-session structural core (~60–70% of
remaining effort; per the structural-fixes-first policy that is expected, not
scope creep) and will produce PERMANENT-by-extension ledger entries needing
user sign-off — "the paper, plus a signed junction-assembly appendix" is the
correct end state.

### 0.1 Posture (2026-06-26 directive): implement Yang faithfully; deviations are errors

The plan of record is **the paper**. We implement what Yang 2025 describes,
*generally*, not the narrowest special case that moves a metric. Two consequences
that override older phrasing in this document:

- **The assay is a regression detector, not a goal.** `SUPPORTED_CORRECT` counts
  prove we didn't break working geometry and help localize failures; they are NOT
  the target and we do NOT prioritize work by "which increment bumps the score."
  A faithful Yang stage that lights up zero new corpus cases is still correct work
  and ships; a score-chasing special case that papers over a structural deviation
  does not. (This reframes the per-slice "assay N→M" framing throughout §4/§4b:
  those numbers are history, not the objective.)
  *(2026-07-05: the corpus grew 194 → 294 with the curated C-series complexity
  cases — genus-N topology, long interleaved chains, near-degeneracy, and named
  milestone trackers per family: [M8] C0041–C0050, [M5/KV9] C0051–C0058,
  [KV6] C0059–C0070, [KV7] C0071–C0074. Trackers flip green when their
  milestone lands — un-pin them in the same PR. Group 1/3 cases carry EXACT
  volume oracles (`expected_volume` in the meta). Spec:
  `specs/assay_complexity_corpus.md`.)*
- **General over piecemeal.** Where the paper gives one algorithm for a family of
  cases (coplanarity, face provenance, mesh updating), we implement that one
  algorithm — not a lattice of shape-specific handlers (planar-only,
  opposite-normal-only, non-holed-only, single-pair-only) each with its own loud
  wall. The piecemeal Stage-0 slices (YR25–27, disc/gear specializations) were
  real machinery progress but are explicitly an interim scaffold to be subsumed by
  the general §4.5.5 implementation below.

### 0.2 Open paper-faithfulness deviations (the real backlog)

These are the substantive divergences from Yang 2025 in the live `yang-rs`
pipeline (full entries in `docs/yang_deviations.md`). They — not the assay — are
the roadmap's remaining work:

1. **Stage 0 §4.5.5 coplanarity is NOT general** (the keystone; see §4 M8 below).
   Today a lattice of shape/normal-specific handlers ships and everything else
   walls loud (`CoplanarFacesUnsupported`): same-normal overlaps, holed faces,
   curved coplanar surfaces, a face in >1 pair, intra-solid near pairs. The paper
   specifies ONE general 2D-Boolean-before-discretization with a single shared
   trimmed surface and identical meshes for both models — **no same/opposite
   distinction, any surface type** (§4.5.5; Fig. 16).

   > **M8 wall decomposition (probed 2026-07-02, `YANG_COPLANAR_PROBE` full
   > corpus): the 22 remaining `UNSUPPORTED(coplanar-boolean)` cases resolve
   > into exactly FIVE mechanisms**, two of which shipped the same day:
   > 1. ✅ **Intra-solid opposite-normal femto-negated step pairs** (10 cases)
   >    — sign-aware sibling-plane canonicalization (kernel-v2) + exactly-
   >    negated benign intra exclusion (yang-rs scan). Spec
   >    `specs/m8_intra_opposite_plane_canonicalization.md`; commits
   >    3dd32340 (RED) → 36d36789 (GREEN) → e0b422fa (adversary). R0064 →
   >    SUPPORTED_CORRECT; R0022/R0031/R0025/R0061 et al. progress to their
   >    next honest wall; several become >30s-cap TIMEOUTs (real exact-
   >    arithmetic compute, the known gear-perf class).
   > 2. ✅ **Non-star subdivided neighbor rings** (4 walled + 3 TIMEOUT-class)
   >    — exact closed-containment ear-clip fallback + B6 exact consecutive-
   >    duplicate collapse in `triangulate_ring`. Spec
   >    `specs/m8_nonstar_ring_earclip.md`; commits 69f3c8a8 (RED) →
   >    18dea32f (GREEN) → f5386e56 (B6 oracle). R0098 clears end to end.
   > 3. **Femto-twin shared-boundary identity — PARTIALLY SHIPPED
   >    (2026-07-02, spec `specs/m8_shared_boundary_identity.md`).** Root
   >    measured as TWO layers, not cross-solid constraints (§8 P10 record):
   >    (a) chained-output vertices femto-off their canonical planes, and
   >    (b) the f64 FRAME PROJECTION minting femto-split coordinates for
   >    OBLIQUE solids even on consistent world coords. **Shipped +
   >    assay-certified (0 WRONG, quiet box): §2b in-frame coordinate
   >    clustering for pure-polygon pairs** (04f14094 + adversary bbbcd407)
   >    — R0070, R0076, R0081 clear the wall (with the earlier ear-clip,
   >    R0098 too). **Banked, deliberately UNWIRED: the world-space vertex
   >    canonicalization** (49a373eb primitive + unit suite; unwired
   >    3cf6f40b) — wiring it flipped R0064/F0047 to SUPPORTED_WRONG (§8a):
   >    its femto motion drives `tessellate_cylinder_patch` into sub-f32
   >    slivers. **The F0047 hole is now GATED LOUD** (spec
   >    `specs/kv2_patch_render_degeneracy_gate.md`, 9eede895→80f3cc73:
   >    always-on f32 render-precision gate; I2 assay-verified
   >    category-neutral). **Root fix SHIPPED 2026-07-03 (spec
   >    `specs/kv2_cdt_triangulation_core.md`, 7c4641ba…f8d68ac1): both
   >    kernel-v2 render cores (cylinder patch + planar) moved from greedy
   >    exact ear-clip + f64 flip to the exact-predicate CDT (cherchi-rs
   >    flood-fill variant via a yang-rs re-export) with a grid-degeneracy
   >    flip pass (M1, cocircular ties), flood-fill interior classification
   >    (M2), pinch/keyhole ring splitting + shared-vertex welding (M3a/b/c),
   >    and the planar G1 gate. Full-assay diff vs baseline = F0042
   >    ERROR→CORRECT only (82 CORRECT / 0 WRONG). Re-wire experiment
   >    re-run (§8a-ii): NO silent-WRONG remains under the world-space
   >    pass and the R0046/R0088/F0063 coplanar walls lift into deeper loud
   >    errors, but F0016/F0024 flip CORRECT→loud Stage-6
   >    "reassembled output would be non-2-manifold" (net −1 CORRECT) ⇒
   >    unwired at that point.** **THEN (same day): the Stage-6 class was
   >    root-caused and fixed (spec `specs/yang_stage6_sliver_topology.md`,
   >    8064537f; deviation N22: fold-sliver exclusion from patch boundary
   >    walks + loop T-subdivision at on-segment foreign vertices), and the
   >    world-space pass is now PERMANENTLY WIRED (14bc23ce, m8 §8a-iii):
   >    re-wire gate 83↔83 CORRECT / 0 WRONG / zero lost; F0022
   >    ERROR→CORRECT even unwired. Assay now 83 CORRECT.** Also new:
   >    KV9-F3 output seam femto-twin quarantine (spec
   >    `kv2_cdt_triangulation_core` §6a — an output-identity defect, §8b
   >    target). **2026-07-03 (same day, two more cycles): §2c rim-aware
   >    clustering SHIPPED (781e2e2e — polygon-chain domain, rim samples
   >    excluded; F0061's Stage-0 coplanar wall closed) and the
   >    LabelMismatch class RETIRED via reference parity
   >    (`specs/cherchi_patch_label_tolerance.md`, 0f9e2982: the C++
   >    label-homogeneity assert is debug-only; L2a subset-compatible
   >    floods proceed, L2b disjoint stays loud, deviation N23; assay
   >    83/0, zero lost).** **Item (v) SHIPPED 2026-07-03 (spec
   >    `specs/m8_stage0_inputcheck_clean_emission.md`): Stage-0
   >    inputcheck-clean overlap emission.** Increment-0 measurement (new
   >    per-operand diagnosis: `YANG_STAGE0_DUMP_DIR` dump +
   >    `cherchi_rs::inputcheck` native five-axiom census + sidecar
   >    oracle) proved ALL violations Stage-0-INTRODUCED (every pre mesh
   >    clean; the F0063 "chained-inherited" hypothesis disproven) and
   >    localized TWO mechanisms: **M-A** — the §2b/§2c clustering rewrote
   >    the overlay's 2D domain while `collect_edge_splits` re-projected
   >    edge endpoints RAW, so its exact collinearity test silently
   >    dropped every boundary split on a clustering-moved edge (holes +
   >    T-junction improper contacts; F0063 op0: 567 boundary edges);
   >    **M-B** — the many-to-one 2D→3D vertex resolution emitted
   >    `[u,u,v]` degenerate triangles (+ orphaned vertices that SEGFAULT
   >    the reference inputcheck binary). Fixes: endpoint projections
   >    routed through the pair's cluster map; resolved-degenerate
   >    triangles dropped at emission; unreferenced-vertex compaction.
   >    **Outcomes: F0063 → SUPPORTED_CORRECT end-to-end; F0016/F0024 +
   >    F0061 land CORRECT; assay 96 CORRECT / 0 WRONG / zero lost;
   >    corpus-wide operand sweep
   >    (`docs/audits/stage0_operand_inputcheck_sweep_2026-07-03.tsv`):
   >    356 Stage-0 operands, only 10 with introduced defects.**
   >    Remaining after (v): (i-residual) R0046 progresses to a NEW loud
   >    kernel-v2 wall (`InvalidBooleanOutput("output loop with fewer
   >    than 3 edges...")`); ~~R0088 keeps ONE edge-pairing instance —
   >    measured **M-C**~~ **M-C SHIPPED 2026-07-04 (spec
   >    `specs/m8_stage0_band_scale_crossing_verts.md`): the root was the
   >    rim-override insertion's ANGULAR merge_tol dedup silently dropping
   >    the second of each band-close override twin pair (R0088-a: 4+4
   >    drops = the 12 boundary edges + pinch; R0070-b: 2 ULP-twin drops).
   >    Fix = exact bit-identity dedup (a tolerance REMOVAL). R0088's
   >    edge-pairing wall GONE (operands five-axiom clean; op now stops
   >    loudly at `EmptyBooleanResult`); R0070's defective op stops loudly
   >    pre-backend (`azimuth-merge rims have mismatched samples` — the
   >    opposite-rim f64 azimuth projection collapses ULP twins; named
   >    follow-up: exact opposite-rim projection).** **Fold-pair emission
   >    class SHIPPED 2026-07-04 (spec
   >    `specs/m8_stage0_fold_pair_emission.md`): the disc-pair builders'
   >    angle-only annulus merge fanned inner chords to outer corners on
   >    the CENTER side of the chord's line (silently orientation-flipped
   >    into same-side overlap pleats — F0027/28/29, R0007, R0023, R0038,
   >    R0089). Fix = exact half-plane visibility guard on each advance +
   >    exact shoelace coverage certificate, loud
   >    Wall("disc-annulus-tri") on deadlock. F0029 + R0089 lose their
   >    E2E ERRORs outright; R0038 drops to an M5-class Stage-3 SSI wall;
   >    all sweep introduced-dirty operands now emit clean.**
   >    **(iii) KV9-F3 output vertex identity SHIPPED 2026-07-04 (spec
   >    `specs/kv9_f3_output_vertex_identity.md`): unmoved junction
   >    duplicates (both twins already on-curve within TAU_WORK) escaped
   >    the §4.4.1(b) sub-feature merge's moved-only scan — eligibility
   >    extended to conic-endpoint-touching triangles (criterion stays the
   >    MIN_FEATURE_SIZE floor); plus from_yang_brep now accepts genuine
   >    2-arc LENS BIGONS on distinct curves (the twin artifact had been
   >    masking them; CurveKey pairing already supported them). Both KV9-F3
   >    kv9 quarantines un-quarantined; F0041 + F0057 ERROR→CORRECT;
   >    assay 88/0 zero lost.** **KV9-F1 layer 1 SHIPPED 2026-07-04
   >    (spec `specs/kv9_f1_tangency_inout_labels.md`, deviation N24):
   >    the steinmetz "tangency crack" was measured down to a PREDICATE
   >    soundness hole — Shewchuk adaptive orient3d/orient2d certify a
   >    FALSE Zero under subnormal UNDERFLOW (the in/out ray's graze on a
   >    femto-skewed seam edge), silently discarding ALL of input B. Fix
   >    = exact-rational zero-certification in the wrappers (only exact
   >    arithmetic certifies Zero). F0056 ERROR→CORRECT; steinmetz
   >    progresses to a LOUD Stage-4 OffCurveBeyondChordBand at a
   >    tangency-adjacent vertex — the ellipse-junction relocation at
   >    tangency grade is the named next increment (kv9 quarantine tags
   >    updated).** **KV9-F1 Increment 0c SHIPPED 2026-07-04 (spec §2c):
   >    a `vert_ell_junction` whose two ellipses name the SAME unordered
   >    cylinder pair is always the pair's surface-tangency point; the
   >    pinch vertex's standoff is second-order (√(2rB)+B derived band,
   >    B = combined chord budget) — gate converted from the
   >    inapplicable first-order 2·d_ε/|d̂·r̂| line metric (which stays
   >    byte-identical for the KV11 box-edge class). Steinmetz SUBTRACT
   >    now clears yang-rs with the exact-volume oracle
   >    (yang-rs kv9f1_tangency_junction trio green). Named next walls
   >    (kv9 tags updated): (a) UNION stops at Stage-6
   >    s6-curved-degenerate-loop — extract_boundary_cycles interleaves
   >    the top/bottom lens cycles at the now-4-valent tangency junction
   >    into a Newell-cancelling figure-eight; junction-aware
   >    continuation pairing needed. (b) SUBTRACT walls at kernel-v2
   >    import NonManifoldVertex — FOUR elliptical arcs share both
   >    endpoints (two per ellipse), defeating vertex-pair edge keying
   >    (the M8 disc∩disc CurveKey lesson, now same-curve bigons). New
   >    kept probes: KV9_JUNCTION_PROBE, NONMANIFOLD_SITE_PROBE
   >    (self-localizing NonManifoldOutput gates).** Still open:
   >    (iv) R0078 azimuth/coplanar. The P10 records stand:
   >    no mesh-level kept-set gate (`yang_kept_mesh_manifold_gate.md`
   >    §2b); kernel-v2's post-subdivision edge pairing stays the honest
   >    downstream wall.
   > 3a. **MIXED Line+Arc planar faces SHIPPED 2026-07-09 (spec
   >    `specs/m8_mixed_loop_coplanar_overlay.md`, ad88e942)** — the
   >    2026-07-09 residue census's largest class (`face-unsupported`:
   >    R0021 R0026 R0051 R0059 F0075). Planar faces mixing LineSegment +
   >    Circle edges enter the general overlay: loops spliced from Stage 1's
   >    own chains (`loop_polyline_attributed`), one `RimChordCtx` per
   >    curved edge, `collect_mixed_crossings` propagating splits into the
   >    arc chain AND the strip lateral's opposite arc (exact axial
   >    projection), Stage-1 arc-chain `rim_overrides` insertion, and
   >    straight-edge-only split keying (the semicircle-arc/diameter-chord
   >    vertex-pair collision). `build_disc_pair` excludes mixed partners
   >    (chord-geometry hazard closed). Amendment 1 P10 record in the spec:
   >    the planned wall-on-curved-subdivision slice was dead on arrival
   >    (the trapezoidal overlay subdivides curved chords at every partner
   >    event column) — the full disc-stack generalization was required.
   >    Assay 216 CORRECT / 0 WRONG, zero lost: F0075 → CORRECT, R0059 →
   >    typed ring-reject ERROR. Remaining sub-walls (typed):
   >    `mixed-arc-lateral-not-cylinder`, `mixed-arc-lateral-unpaired`
   >    (hole-free Slice-D laterals — the increment-2 one-sided mechanism
   >    plausibly applies, no corpus case targets it), Ellipse edges
   >    (`face-unsupported`, no ellipse mint).
   > 3b. **Increment 2 — HOLED (chain-consuming) laterals SHIPPED
   >    2026-07-10 (spec `specs/m8_mixed_arc_lateral_holed.md`, 36ee1b88).**
   >    `mixed-arc-lateral-holed` lifted: a holed cylinder lateral takes the
   >    KV14 unroll+CDT path, which splices its boundary loops from the
   >    shared per-edge chains (`loop_polyline`) — no strip index-pairing
   >    constraint, so the crossing split point inserts ONE-SIDED into the
   >    arc's own chain. `arc_lateral_opposite` classifies Strip vs
   >    ChainConsuming and verifies loop spliceability before admitting
   >    (a non-spliceable loop keeps the typed wall rather than degrading
   >    to Stage-1 MalformedTopology). R0021 → CORRECT; R0026/R0051 →
   >    typed downstream walls (Stage-3 AmbiguousCurve conic class,
   >    Stage-6 Newell-normal validation). TEST-PHASE FINDING (spec
   >    amendment 1, named follow-up `kv14-lateral-cdt-chord-bound`): the
   >    KV14 holed-lateral CDT is boundary-only EARCUT with no
   >    triangle-quality bound — unsampled seam rulings make it fan wall
   >    triangles to window corners (radial sag ~5× the one-chord sagitta
   >    in the windowed-half-cylinder fixture, mesh under-fills ~15%
   >    watertight). Union oracles use delta-volume (sag cancels).
   > 3c. **Exact frame projection in ring triangulation SHIPPED 2026-07-10
   >    (spec `specs/m8_ring_exact_projection.md`).** The 2026-07-10
   >    census of the 14 remaining coplanar walls (probe re-run): 8×
   >    overlay-admitted-but-emission-fails + 3× `multi-pair` (R0046,
   >    R0081, F0073) + 3× `rim-lateral-none` (row 4). The emission-8
   >    decompose into THREE mechanisms, all downstream of ONE geometric
   >    fact — mirrored disc-rim samples carry 1–2-ULP-split frame
   >    projections (rim coords are §2c-excluded from clustering), and
   >    the exact trapezoidal sweep faithfully builds femto event-column
   >    slabs from them:
   >    (i) `build-mesh-triangulate` (F0068/F0069/C0075) — the femto
   >    Steiner twins propagate as splits into neighbor face rings, and
   >    `triangulate_ring`'s **f64** frame projection ALIASED the twins
   >    onto one bit-identical 2D point (zero-length exact edge → every
   >    fan/ear rejected). FIXED: the ring projection is now evaluated
   >    over exact rationals (fixed f64 basis; faithfulness I-EP1, no
   >    tolerance). F0068+F0069 → SUPPORTED_CORRECT end-to-end; C0075 →
   >    typed downstream edge-not-2-directed ERROR (KV15b/§4B family).
   >    Assay 226 CORRECT / 0 WRONG / 53 ERROR / 15 UNSUPPORTED / 0
   >    TIMEOUT, zero-lost (diff = exactly the three targets).
   >    (ii) `overlay-failed RoundingCollapse` (F0067, C0048, R0053) —
   >    femto-slab CELLS whose exact-positive triangles round to
   >    f64-collinear slivers (measured: all three verts share one f64
   >    x). **2026-07-10: the emission-gate T-subdivision candidate was
   >    prototyped and REFUTED (P10 abort record in spec
   >    `m8_overlay_femto_slab_emission` §8).** Clean slab needles ARE
   >    locally repairable, but the measured walls are not: (ii-a)
   >    C0048's twin corners mint crossing points EXACTLY collinear on
   >    an input chord while every off-chord vertex rounds onto the same
   >    f64 event column — every local apex is exactly degenerate or
   >    f64-collinear; needs per-region re-emission fanning femto
   >    boundary sub-segments to FAR apexes (constrained snap-rounding
   >    grade). (ii-b) R0053 has twin mints on one input edge (union
   >    boundary) whose ROUNDED order inverts their exact order — NO
   >    triangulation over the fixed rounded vertex set exists; fix
   >    belongs at the MINT SITE (sub-representable twin collapse at
   >    emission = the KV15b/A14.2 class, increment-4 precedent), which
   >    joins this sub-case to mechanism (iii) below. Free T-subdivision
   >    chaining CYCLES (measured fixpoint churn); strict-progress
   >    restores termination but cannot pass the two walls. Banked:
   >    `[sliver-probe]`/`[pocket-probe]` structure census on the gate's
   >    error path + corpus wall pins
   >    (`yang-rs/tests/m8_overlay_femto_slab_emission.rs`). Input
   >    welding remains NOT the path (two P10-reverted variants, spec
   >    `m8_shared_boundary_identity` §2b/2c scope limits).
   >    (iii) `overlay-failed DegenerateLoop` (R0007, R0071) — MICRO-scale
   >    models (~1e-4): chained inputs carry SUB-FLOOR (~7e-8 <
   >    MIN_FEATURE_SIZE) corner twin pairs; the §2b/§2c band clustering
   >    correctly welds them and the polygon then has bit-identical
   >    consecutive vertices → loud `exact_loops` reject. This is the
   >    KV15b mint-site class (collapse sub-floor twins at the EMITTING
   >    boolean, A14.2) — fixing it here (consecutive-dup collapse) would
   >    desynchronize the pair face from neighbors that still carry both
   >    corners.
   > 4. **Disc rim on a non-cylinder lateral** (`rim-lateral-none`: R0050,
   >    R0025 2nd wall) — `lateral_for_cap` is cylinder-only; R0025's rim
   >    lateral is a TORUS (circle-revolve), so crossing propagation +
   >    the downstream boolean are KV6d-torus-gated (R0015 ceiling class).
   > 5. **Swiss-cheese holed discs** (F0086–F0090) — holed-disc overlay
   >    routing (the reverted 2026-06-24 implementation is re-derivable; its
   >    old blocker, the same-normal gate, is gone).
   >    **2026-07-05/06 status (spec `m8_holed_disc_coplanar_overlay`):
   >    increments 1–3 SHIPPED.** The isolated holed-disc pair is GREEN for
   >    BOTH partners: polygon (increment 2, exact axial projection) and
   >    disc-with-rim-interior-to-the-annulus (increment 3: exact ULP-twin
   >    ring ordering `exact_rim_ccw_tiebreak` + flood-fill Stage-1 planar
   >    CDT with exact hole parity + §4.4.1(b) merge at Stage-4 entry).
   >    Remaining for this row: the CHAINED F0086–F0090 corpus cases.
   >    **Increment 5 (2026-07-06, task #62): chained cut 2 GREEN** — wrap-
   >    aware cyclic azimuth-merge pairing (yang) + sweep-aware closed-rim
   >    arc fallback (kernel-v2 recover). F0086/F0089 ERROR→UNSUPPORTED
   >    (typed re-entry boundary at cut 3). The WHOLE family now bottlenecks
   >    on one root: Stage-0 mints rim crossings ON CHORDS (off-circle by
   >    the sagitta) → mixed chains defeat recover's canonical anchor +
   >    VertexOffSurface residues (F0087/88/90). Boundary pinned:
   >    `kernel-v2/tests/m8_swiss_cheese_chain.rs`.
   >    **Increment 4 (2026-07-06/07, task #61): SHIPPED** — Stage-0
   >    sub-floor shared-mint collapse (minted on-circle vertices closer
   >    than MIN_FEATURE_SIZE collapse per rim circle to ONE shared target,
   >    crossing-branch preferred) + fold-gate skip of never-emitted
   >    3D-degenerate triangles. R0072-class adversary
   >    `crossing_one_ulp_inside_rim_sample` un-quarantined GREEN; assay
   >    175 CORRECT / 0 WRONG / zero lost. Task #62 (chained family
   >    re-measure on top of this) is now unblocked. Spec:
   >    `n2_stage4_junction_cluster_merge` §3 amendment 3 +
   >    `m8_holed_disc_coplanar_overlay` §8.
   >    **Increments 6+7 (2026-07-07, task #62): SHIPPED** — annular
   >    rim-mint contexts (one `RimChordCtx` per rim circle; cut-3 typed
   >    re-entry wall LIFTED, direct 5-hole chain GREEN) + constrained
   >    Lawson flip repair in the fold gate (amendment 4: the production
   >    sketch frame's rotated sweep order builds femto-strip slivers any
   >    on-circle rim mint inverts; repairable folds now re-triangulate
   >    locally instead of reverting to chord positions). **F0086 corpus
   >    replay SUPPORTED_CORRECT**; F0089 6→2 / F0090 27→22 errors; family
   >    residual = a second, constraint-bounded fold class (rim-mint
   >    clusters displaced ~4.6e-2, no legal flip) + F0088's typed
   >    partial-patch re-entry. Oracles: `m8_swiss_cheese_chain.rs`
   >    `engine_frame_*` (the corpus geometry, direct constructors).
   >    **Increment 8 (2026-07-07, task #62): SHIPPED** — Fig-11
   >    delete-and-reinsert cavity relocation in the fold gate (amendment
   >    5: star carve + constrained visibility growth deferring at
   >    intersection-curve/domain-boundary edges + constrained exact
   >    ear-clip when the cavity is not star-shaped from the mint). The
   >    rim-mint COLUMN-HOP class is repaired; F0087 cut 7 GREEN. Corpus:
   >    0 WRONG / 183 CORRECT (+5) / zero lost; F0087/F0089/F0090
   >    ERROR→typed UNSUPPORTED. **The whole residual family
   >    F0087–F0090 now sits on ONE wall: curved partial-patch operand
   >    re-entry (to_yang) — the next lever for this row.**
   >    **Increment 9 (2026-07-07, task #64): SHIPPED** — amendment 6
   >    JOINT region relocation: when per-vertex Fig-11 cavities are
   >    exactly NON-SIMPLE because two (or more) rim mints interact
   >    across one multi-column strip (F0087 cut 9: the plate-rim mint
   >    and a hole-rim mint each appear on the OTHER's cavity polygon,
   >    whose collapsed spokes cross), the seeds' star-UNION region is
   >    re-triangulated jointly: single closed boundary cycle, no
   >    interior vertex, single class, then the SAME shared constrained
   >    exact ear-clip (`earclip_cavity_polygon`, factored from
   >    amendment 5 byte-identically). `f0087_cut9` pin retired →
   >    positive regression; the full TEN-hole F0087 chain green with
   >    its volume oracle; **corpus F0087 flips ERROR →
   >    SUPPORTED_CORRECT** (the corpus-path partial-patch degradation
   >    disappeared with the strip repaired at Stage 0). Remaining tail
   >    of this row: F0088/F0089/F0090 (VertexOffSurface + one Stage-3
   >    AmbiguousCurve on F0088 — deeper strips/next mechanism, measure
   >    before designing) and C0048.
   >    **Increment 10 (2026-07-07, task #67): SHIPPED** — amendment 7
   >    CLASS-PARTITIONED joint region relocation. Probe census of the
   >    F0088/89/90 tail found the dominant wall: rim mints are minted
   >    exactly ON the intersection curve, so the amendment-6 star-union
   >    straddles the class boundary and its single-class guard rejected
   >    wholesale (`multi-class region` — F0089's only error, most of
   >    F0090's 18). The star-union is now partitioned by `RegionClass`;
   >    each FOLDED sub-region is relocated independently (valid-only
   >    sub-regions skipped — termination), and class-boundary edges become
   >    sub-region boundary by construction, so the intersection curve is
   >    never re-triangulated across. `f0089_cut11` pin retired → positive
   >    regression; 11-hole F0089 chain green with volume oracle; **corpus
   >    F0089 flips ERROR → SUPPORTED_CORRECT** (F0086/F0087 unchanged ✓).
   >    Residual tail, measured: F0090 (18× VertexOffSurface — each cut
   >    also folds a SECOND triangle whose per-vertex relocations all
   >    reject `interior vertex with constraint-blocked fan`, so the joint
   >    path never triggers; plus one `region polygon not simple`), F0088
   >    (2× VertexOffSurface via non-simple rings whose joint seeds stay
   >    SINGLETON + one `cavity polygon not CCW`, and one Stage-3
   >    `AmbiguousCurve{candidates:0}` SSI wall — a different subsystem),
   >    and C0048.
   >    **Increment 11 (2026-07-07, task #68): SHIPPED** — amendment 8
   >    REGION GROWTH TO SIMPLICITY. The post-amendment-7 census showed
   >    the dominant residual was `class … region polygon not simple`: a
   >    femto-strip sub-region's boundary is a BOW-TIE under the minted
   >    positions (the strip's two long sides cross exactly). The region
   >    form now grows across a crossing edge's single external same-class
   >    neighbor (constraint edges never crossed, apex-pinch guard) and
   >    rebuilds the boundary until the ring is exactly simple — the
   >    region analog of amendment 5's constrained visibility growth.
   >    `f0090_cut7` pin retired → positive regression; 7-hole F0090
   >    chain green with volume oracle; F0086/F0087/F0089 corpus ✓
   >    unchanged. F0090's 30-cut corpus chain now repairs all but ONE
   >    fold (24 reverts → 1; 93 growth events, 28+ cuts succeed vs 12
   >    before) — the corpus case flips ERROR(18) → TIMEOUT in the
   >    container because each newly-succeeding cut adds rim circles
   >    (overlay reaches 58 circles; the known heavy-chain container
   >    limit, not a hang — per-op timing curve verified monotone and
   >    finite). Remaining measured tail: ONE F0090 revert (a 33-seed
   >    star-union whose class sub-region boundary is NOT a single closed
   >    cycle), F0088's three walls (singleton non-simple seeds,
   >    `cavity polygon not CCW`, Stage-3 `AmbiguousCurve{candidates:0}`),
   >    and C0048.
   >    **Increment 12 (2026-07-07, task #69): SHIPPED** — amendment 9
   >    CONNECTED-COMPONENT SPLIT of class sub-regions (deterministic BFS
   >    through shared edges; each folded component is its own Fig-11
   >    instance). Unit-proven (two disjoint folded stars under one seed
   >    set both commit); required coverage for multi-strip joint
   >    triggers. **Post-ship measurement: the F0090 33-seed site is
   >    CONNECTED and ANNULAR** (vert 151's per-vertex ring alone has 40+
   >    edges; the ~30 ring mints inflate the joint region into a band
   >    encircling a hole — multiple boundary cycles on one component,
   >    still the loud `not a single closed cycle` reject). Next lever
   >    for the F0090 tail: measure the annular region's cycle structure,
   >    then either narrow the amendment-6 seed set to crossing-edge
   >    endpoints (keep the region a strip) or bridge-edge annular
   >    ear-clip. F0086/F0087/F0089 unchanged ✓; chain suite 13/0.
   >    **Increment 13 (2026-07-07, task #70): SHIPPED** — amendment 10
   >    CROSSING-ENDPOINT SEED NARROWING. The cycle probe confirmed the
   >    annular structure (2 cycles [32, 20]); root = the amendment-6
   >    trigger seeded EVERY minted vertex on the non-simple ring (~30 on
   >    vert 151's hole-encircling ring). `EarclipErr::NotSimple` now
   >    carries the crossing pair's endpoint positions; ring_mints narrows
   >    to mints ON the crossing (the interacting set — Fig-11 locality).
   >    The joint region stays a strip. **F0090 probe: 0 reverts, 0
   >    annular rejects in the container window (chain reaches 60-circle
   >    overlays)** — the family's fold-gate is clean as far as the
   >    container can run; F0090 corpus stays TIMEOUT on legit chain
   >    weight only. All 13 chain pins green (F0087 cut-9 interacting-pair
   >    semantics preserved). **The F0086–F0090 family's remaining
   >    pipeline walls are now F0088-only:** singleton non-simple seeds,
   >    `cavity polygon not CCW` (vert 674), and the Stage-3
   >    `AmbiguousCurve{candidates:0}` SSI wall (different subsystem) —
   >    re-measure F0088 on the current binary before designing. Plus
   >    C0048 (coplanar auto-union, separate row).
   >    **Increment 14 (2026-07-07, task #71): SHIPPED** — amendment 11
   >    SIMPLICITY BEFORE ORIENTATION. F0088's two VertexOffSurface
   >    errors (ops 14/15) both reverted at vert 674: a hair-thin
   >    full-height NET-CW BOW-TIE cavity (return edge crosses the
   >    up-chain, net 2A = −4.2e-3) died at the ear-clip's CCW guard
   >    BEFORE the crossing scan ran — a self-intersecting cycle's
   >    shoelace sign is not an orientation. Guard order swapped; the
   >    bow-tie now surfaces `NotSimple` → joint path (commits with seeds
   >    [672,674,677]). Singleton-trigger relaxation prototyped and
   >    REVERTED (3 seeds — untested branch, no measured case). **Corpus
   >    F0088: 3 errors → 1.** New chain regression
   >    `f0088_engine_frame_chain_no_offsurface_residue` (15 cuts,
   >    skip-on-error, volume oracle). **The ENTIRE F0086–F0090 fold-gate
   >    campaign is now closed** — the family's sole remaining pipeline
   >    wall is F0088's Stage-3 `AmbiguousCurve{candidates:0, matched:0}`
   >    for intersection edge (2,344): the SSI-refinement subsystem
   >    (Stage 3/4), not Stage-0/mesh-updating. Next: measure that edge's
   >    geometry (what curve should refine it, why zero candidates), plus
   >    C0048 (separate row).
   >    **AmbiguousCurve{0,0} DIAGNOSED (2026-07-07, task #72 — increment
   >    15 pending):** `ssi_rs::intersect` returns EMPTY — the plate outer
   >    cylinder and F0088 hole-4's tool cylinder are ANALYTICALLY
   >    DISJOINT (internal gap = R − d_axes − r = 0.0115) but the plate's
   >    N=14 chord facets dip inward by the sagitta ≈ 0.032 > gap, so the
   >    MESHES intersect. This is Yang Fig. 8 **Case IV** ("the meshes
   >    detect intersections that do not exist in surfaces",
   >    `refs/text/yang2025_hybrid_boolean.txt:436-447`) — the paper
   >    FILTERS it; the output topology follows analytic truth (the thin
   >    wall SURVIVES; it is 100× MIN_FEATURE_SIZE, a real feature).
   >    Planned fix: Stage-1 phantom guard — for each analytically
   >    disjoint cyl×cyl face pair whose chord bands overlap (gap <
   >    sagitta_A + sagitta_B), raise the affected circles' `min_n_seg`
   >    (the Stage-1 hook already exists) until the combined sagitta
   >    clears the gap (F0088 needs N ≥ 24; terminating criterion is
   >    ANALYTIC, unlike the rejected global NSEG floor for the
   >    column-hop class). RED = F0088 cut-4 direct-chain pin.
   >    **Increment 15 (2026-07-07, task #72): SHIPPED** — the Case-IV
   >    phantom guard. `phantom_min_rim_segments(a, b)` derives the
   >    minimal N with `sag(r_a,N)+sag(r_b,N) ≤ gap/2` over all disjoint
   >    cyl×cyl pairs (external + nested + skew; self-limiting natural-N
   >    gate; 4096 cap → the loud stop stays for true near-tangency);
   >    `boolean()` rebuilds both operands at that N, and the forced N is
   >    a BRep property (`forced_rim_n`) honored by ALL of Stage-0's
   >    internal re-tessellations (`disc_rim_ring`, `build_stage0_mesh`,
   >    the coincident-cylinder shared-N path) — plumbing WITHOUT which
   >    the boost was silently discarded (measured: identical edge ids at
   >    N=34). Pin retired → `f0088_cut4_phantom_intersection_filtered`
   >    (thin wall survives, volume oracle); the 15-cut F0088 direct
   >    chain green with NO AmbiguousCurve and NO VertexOffSurface.
   >    yang-rs 558/0 (4 new guard unit tests + mutation caught), chain
   >    suite 15/0, F0086/F0087/F0089 corpus ✓ unchanged. **Corpus F0088
   >    residual (measured): ops 7/15 `face 0: CDT triangulation failed`**
   >    — the corpus-path (sketch-extrude + auto-union) chained re-entry
   >    feeds a recovered cap whose CDT fails at the boosted rim density;
   >    a distinct pre-existing wall the coarser N never reached (the
   >    direct chain does NOT reproduce it). Next lever: measure that CDT
   >    input (dump the failing polygon; recover.rs vs
   >    cdt_polygon_with_holes at dense rims).
   >    **Increment 16 (2026-07-07, task #73): SHIPPED — the F0086–F0090
   >    CAMPAIGN IS COMPLETE.** The corpus CDT failures were the
   >    INTRA-solid form of the Case-IV criterion: the chained body's own
   >    hole-4-lateral sits 0.0115 from its plate wall, so ANY
   >    tessellation at natural N=14 puts the cap's outer-rim chords
   >    (dip 0.032) across the hole rim → crossing CDT constraints. A
   >    `boolean()`-level intra scan was tried and REVERTED (it made the
   >    corpus worse — 10 conversion-time failures: boosted outputs
   >    re-entered `BRep::new` at natural N, and the guard cannot reach
   >    conversion). The fold lives in **Stage 1's own N selection**
   >    (`stage1_tessellate_inner`; shared `cyl_pair_phantom_n` with the
   >    cross guard), where conversion, Stage-0 rebuilds, and guard
   >    rebuilds all pick it up natively. **Corpus: F0086 ✓ F0087 ✓
   >    F0088 ✓ (SUPPORTED_CORRECT, 289s solo — heavy-chain band)
   >    F0089 ✓; F0090 fold-clean (container-TIMEOUT only).** Family
   >    campaign closed end-to-end. Residual bookkeeping: F0088 runs
   >    ~290s solo in the container (the parallel 240s cap will class it
   >    TIMEOUT there — same band as F0090; the user's box decides the
   >    committed baseline).
2. **N4 — face provenance by centroid-proximity, not §4.2.3 barycentric
   provenance.** Stage-6 attributes each kept triangle by centroid-in-plane
   distance + a tolerance tier (`tol_for`). The paper maps each point to BOTH
   surfaces via the arrangement's per-triangle barycentric coordinates. N4 is the
   *structural root* of the `FaceResolutionFailed` fragility (and the
   tolerance-band/unmasking churn). Its blocker (N1, sidecar-only solid-level
   labels) is GONE now that the native arrangement is complete — so N4 is
   actionable and should be done *with* the general Stage-0 work.
3. **N2 — Stage-4 mesh updating is relocation-only (§4.4.1 CDT absent).** We
   relocate existing intersection vertices; the paper does full CDT mesh updating
   (split/merge/insert + per-triangle d(T) recompute). This is why Stage 4 hits
   loud `DegenerateTriangle`/`LocalRefinementRequired` stops on harder coplanar
   and curved configs. **N2-1 done (2026-07-01):** the faithful §4.4.1
   mesh-updating *primitive* now exists, unit-tested in isolation —
   `cherchi_rs::cdt_with_interior_constraints` (Fig 11 `split`, interior
   constraint CDT) + `yang_rs::stage4_update::stage4_mesh_update` (Fig 11
   split/merge/insert over the parametric domain). NOT yet wired into
   `stage4_relocate_and_correct`. **N2-2 done (2026-07-02):** per-triangle
   `d(T)` recompute — `yang_rs::stage4_dt::{eval_uv, d_of_t}` computes the
   certified Fig-6 bound from exact rational-Bézier surface-of-revolution
   control nets (one general constructor for cylinder/cone/sphere/torus; plane
   trivially 0; convex-hull certificate; pinned `eval_uv` parameterization the
   N2-3 patch extraction must share). Unit + adversary suites incl. a
   mutation-kill matrix (`tests/n2_dt_adversary.rs`). **N2-3 grounding
   (2026-07-02, instrumented):** the premise above is STALE — probes at all
   four Stage-4 repair STOPs hit ZERO times across the yang-rs suites, the
   same-normal campaign, and the 194-case assay, so wiring the primitives has
   NO live consumer today and stays deferred until one appears (demand-driven;
   the primitives are ready). The trail instead led to **N2-3a (done
   2026-07-02):** Stage-0 was minting coplanar-overlay vertices that subdivide
   a disc-rim chord at CHORD positions (raw `frame.lift` fallback) — off the
   exact rim circle by the sagitta, the dominant `VertexOffSurface` class
   (R0072), silent in release builds (the kernel tripwire is debug-only). Fix:
   mint on the exact rim `Curve::Circle` (radial projection for x-event
   subdivisions, exact circle∩line for rim×other-input-edge crossings),
   behind a fold-validity gate (a mint that would invert an incident overlay
   triangle reverts to the chord lift, deterministic fixpoint) — unconditional
   exactness folds coarse-rim overlays (R0013's 9-gon, sagitta 0.53). **The
   gate's revert population is the first recorded LIVE consumer for
   overlay-level mesh updating** (repositioned boundary vertices need local
   re-triangulation, Yang Fig 11) — that, not Stage-4, is where the N2-1/N2-2
   machinery should first wire in. Assay 0 WRONG, no CORRECT lost (60s run:
   82 CORRECT; R0013 30s-cap TIMEOUT flip is proven timing noise). R0072's
   blocker moved downstream to kernel-v2 `TessellationFailed { FaceId(19),
   "inverted final triangle" }`; R0021's is the Stage-1 partial-patch
   re-entry wall. R0096 = torus×torus v1 wall (different mode).
   **R0072 face-19 diagnosed (2026-07-02, `KV2_EARCLIP_PROBE`):** the output
   face's outer loop carries a collinear direction-REVERSAL micro-spur
   (p0→p1 4.4e-6 forward, p1→p2 1.4e-6 BACK along the same line, at face
   extent ~2e-4) — a §4.5.3-class reversed point on a straight boundary run
   that the exact ear-clip correctly refuses; sibling zigzags on the
   face-9/10/12–15 plane family. The fix is in yang OUTPUT loop emission
   (reconstruct_topology boundary-cycle ordering / spur elimination), not in
   kernel-v2 tessellation.
   Specs: `specs/n2_stage4_mesh_updating.md`, `specs/n2_stage4_dt_recompute.md`,
   `specs/n2_stage4_junction_cluster_merge.md` (amended ×2 — the grounding
   trail is recorded in its §0).
   **§4.5.3 junction-protected collapse SHIPPED 2026-07-08 (spec
   `specs/yang_453_junction_protected_collapse.md`):** the ERROR-census
   ellipse-endpoint class (R0009/R0011/R0091,
   `InvalidBooleanOutput("output ellipse-arc endpoint does not lie on its
   ellipse")`) was TWO Stage-4 victim-selection bugs, both violating Yang's
   "points along ONE curve C" scope. **(a) SHIPPED:** the §4.5.3 reversal
   sweep collapsed the NEXT point even when it was a curve-JUNCTION vertex
   (the exact shared endpoint of two conic sections — R0011's gear-flank ×
   revolve-cylinder loop lost all seven junctions, merging arcs of different
   ellipses into single output edges). Fix `reversal_collapse_direction`:
   junction p_n survives; the overshooting p_r is the victim. R0011
   ERROR(ellipse wall) → deeper loud render-CDT wall (R0072 class).
   **(b) WIRED 2026-07-21 (task #186):** the §4.4.1(b) sub-feature merge
   picked its survivor by LOWER INDEX, destroying an exactly-relocated
   conic endpoint in favor of an unrelocated chord vertex (R0091 AND
   R0009's ellipse walls — both micro scale). The ranked-survivor
   primitive `sub_feature_merge_direction` (junction > conic endpoint >
   plain; equal rank keeps the index rule) is now WIRED always-on: its
   bank condition (the unverifiable R0091 χ) was resolved by verifying
   the output's true χ=−4 via Cherchi sidecar reference parity + an
   independent voxel-CSG derivation from the authored numbers — the meta
   euler_target=2 was the authoring error (corrected; see the P3b spec
   §7.7). Trackers `s453_junction_collapse_campaign.rs`: all three GREEN
   and un-ignored (measured: the R0009/R0091 ellipse walls had already
   drifted to the merge-budget LRR wall, the #171 u32::MAX class).
   **(c) §3c straight-run reversal sweep SHIPPED 2026-07-08 (same spec):**
   the sweep's `all_conic` loop gate never corrected reversed sequences on
   STRAIGHT intersection runs — R0072's seam mints (chord-crossing
   positions) become out-of-order when their neighbor triple point is
   exactly relocated (Yang Fig. 15 verbatim, on a line), and the output
   loop doubles back → kernel-v2 `"ring rejected by CDT"`. Shipped scope:
   mixed cycles sweep straight-run (LineSegment-pair) sites ONLY, with run
   identity via unordered incidence surface pairs, the n_A×n_B tangent
   (paper Fig. 15; §4.5.5 coincident pairs undiagnosable — checked before
   the U-turn arm), junction/run-end-protected victims, and a 2·d_ε
   resolution gate. PLUS a Stage-0 admission wall: partner disc rim
   STRICTLY CROSSING an annular face's HOLE rim → typed
   `CoplanarFacesUnsupported` (probe `annular-hole-rim-crossing`) — the
   `annular_cap_hole_crossing_stays_loud` pin's documented boundary, which
   previously held only via an accidental downstream NonManifoldOutput.
   R0072's FaceId(9) spur is repaired (wall→FaceId(11), a conic-site
   mixed-cycle reversal — DISPROVEN sweep class, see spec §3c P10 records:
   coarse 7-gon chords false-positive the 45° band per the corner_in_band
   adversary; no mesh-level manifold gate can backstop it per the standing
   §2b record, re-confirmed on the Steinmetz tangency seam). Trackers
   `s453_line_run_reversal.rs`: R0072/F0045 documented `#[ignore]` RED
   (F0045 = MACRO self-intersection at FaceId(9), a different mechanism).
   New probes kept: `YANG_S6_CYCLE_DUMP`, `annular-hole-rim-crossing`.
   **N2/F0059 epic increment 1 SHIPPED 2026-07-10 (spec
   `specs/yang_collapse_membrane_cancellation.md`, task #121):** the F0059
   "Stage-6 double-cover" origin is the Stage-4 **PR-KV9 junction-twin
   collapse itself** — identifying the two arrangement vertices minted for
   ONE Steinmetz seam apex turns the pleat spanning the twin gap into an
   exact opposite-winding duplicate pair (zero-volume flap, count-4 fan
   edges, `s6-wedge-walk-not-outgoing`). Fixed at the mint site:
   `collapse_vertex` now cancels opposite-winding exact-duplicate pairs
   (both copies; unit red→green ×3 branches). The 2026-07-08 diagnosis's
   deeper layers are corrected: the χ=4 "two-shell stitching gap" was an
   artifact of that experiment's exclusion-style workaround and does NOT
   exist; with the (still banked-unwired, `YANG_TRIPLE_JUNCTION_EXPERIMENT`)
   conic triple-junction handler + this fix, F0059's boolean COMPLETES and
   the wall moves to kernel-v2 render CDT `FaceId(7)` ring-reject — the
   cap-disc's four segment lobes meet the trim chords exactly ON the rim
   circle and the chord-sampled rim crosses the trim chords near those
   junctions (§4.3.3 Case-IV **rim-junction-insertion class**, suspected
   same wall as the F0045/R0011 ring-reject family; M8 incr-15
   `forced_rim_n` is the precedent machinery). Epic increment 2 = rim
   junction insertion; increment 3 = wire the handler (green = F0059
   end-to-end). Probes banked: `YANG_DOUBLECOVER_PROBE` (dup-triple scans +
   collapse-site tags), wedge-dump under `NONMANIFOLD_SITE_PROBE`.
   **N2/F0059 epic increments 2+3 SHIPPED 2026-07-10 (spec
   `specs/yang_rim_junction_insertion.md`, task #122) — F0059
   ERROR→SUPPORTED_CORRECT end-to-end; assay 232 CORRECT / 0 WRONG /
   45 ERROR / 0 TIMEOUT, zero-lost (R0015 also advances: its junction
   resolves and it lands on the TYPED M8 coplanar boundary,
   ERROR→UNSUPPORTED).** Increment 2 = Stage-1 rim junction insertion:
   `rim_junctions_against` derives the exact §4.3.3 Case-IV points where
   a full-circle rim transversally crosses the other operand's
   parallel-axis cylinder laterals (circle∩line closed forms; lateral
   extent from loop circles ±TAU_WORK keeps boundary triple corners;
   tangency gate = root pair < TAU_MODEL), wired via
   `from_topology_with_rim_overrides` + the M8-incr-6
   `stage1_tessellate_with_rim_overrides` vocabulary, scope-gated to
   pairs with NO Stage-0 interaction (the incr-15 pass-through trap is
   avoided, not threaded). Increment 3's final form is a **pre-scan
   exactness certificate** (measured: the corners trip INSERT-TIME
   detectors — the line∩line "out of scope" STOP — so the planned
   post-scan escape can never run): `exact_junctions` = vertices with ≥3
   distinct inc0 surfaces all within TAU_WORK, skipped by every Stage-4
   map insertion; everything inexact keeps today's loud walls. The
   twice-reverted Newton triple-junction handler is UNNECESSARY for this
   class and was removed (its spec stays as the design record).
   Green pins un-ignored: truncated-Steinmetz exact-volume union +
   never-stops-at-Stage-4 (`yang-rs/tests/rim_junction_insertion.rs`).
   Remaining named residue: quartic-class rim junctions (transversal
   axis), partial-arc rims, the ring-reject family (F0045/R0011 — no rim
   junctions, different mechanism), the 13 other Stage-4 LRR configs.
   **N2/F0059 epic increments 4+5 SHIPPED 2026-07-10 (specs
   `yang_rim_junction_insertion` §4 + `yang_stage4_conic_triple_junction`
   now WIRED, task #123) — the 6-case cone-hyperbola Stage-4 LRR family
   (R0004/R0017/R0019/R0044/R0047/R0049) measured and its junction wall
   retired; every case advances to a distinct deeper wall. Assay 232
   CORRECT / 0 WRONG / 45 ERROR / 0 TIMEOUT — totals byte-identical to
   the pre-increment baseline, ZERO category flips (zero-lost held).**
   Measured
   shape (per-surface signed-distance probe): coaxial cone-band lathe rims
   (PARTIAL-ARC edges — the operands are partial revolves) crossing a
   plane face of the other operand; the junction vertex is exact on the
   plane and rim-chord-sagitta INSIDE both cones (identical f on both =
   the rim-chord signature). Increment 4 = the spec's deferred plane-face
   arm: rim×plane circle∩line closed form with polygon AND disc/annulus
   containment (even-odd over segments + circle parity), arc-rim in-sweep
   filtering (endpoint/seam duplication guards), coaxial azimuth
   propagation rebuilt as ANGLE-SPACE cluster reconciliation (one shared
   th_eps = TAU_MODEL/r_min per coaxial group — per-radius chord
   tolerances desynchronize band-partner arcs by one point: the R0019
   161-vs-162 strip stop), a cone frustum-band azimuth-merge route
   (shared `tessellate_band_azimuth_merge`, cylinder path byte-identical),
   and the §4d SCALE-AWARE exactness certificate band max(TAU_WORK,
   8·ε·L) (absolute 1e-12 is ~2 ULP at the R0017 coordinate magnitude
   4000 — measured already-exact junctions failing certification on
   evaluation noise). Increment 5 = the design-record ≥3-surface conic
   triple-junction relocation WIRED (trigger: vertex in ≥2 single-curve
   conic maps, inc0 dedups to exactly 3 surfaces — the prism-EDGE ×
   cone-lateral interior junction, R0017 v101, which has no rim to insert
   into), reusing `relocate_onto_implicit_triple` with the same
   scale-aware Newton tolerance (byte-identical at unit scale) + the
   torus-block 2·d_ε/sinθ displacement gate. Regression trail (the
   plane arm's final v1 scope): ungated, the arm fired on CYLINDER rims
   corpus-wide and (a) regressed F0047/R0006/R0075/F0081 — the inserted
   rim vertex ULP-twins the arrangement's own crossing vertex (incl.
   plane∩plane `vert_pp_planes` vertices whose 2-surface incidence
   certifies nothing) into a render-precision sliver, invisible to the
   §4.4.1(b) merge because certificate/triple handling REMOVES resolved
   vertices from every map (the KV9-F3 scan-exemption trap) — and (b)
   unmasked R0091's banked-§3b unverifiable-χ path (ERROR→WRONG χ=−4).
   A merge-eligibility + scale-derived ULP-band fix recovered (a) but
   its floor-based first draft broke the micro-scale adversary
   (`extreme_magnitudes_valid_or_loud` — the KV15b sub-floor lesson,
   re-proven), a sub-resolution-pair gate for (b) was REFUTED (the
   disc-cap fixture has R0091's same relative spacing, legitimately),
   and after scoping the plane arm to CONE-FLANKED rims (the entire
   measured class; the cylinder population is proven healthy without
   insertion) the eligibility machinery lost its demanding case and
   its pins (P4) — REMOVED; the trail + `YANG_TWIN_SCAN` probe make
   reintroduction cheap when a cone-class twin demands it. Green pins:
   `rim_junction_insertion.rs` lathe∖tilted-slab (hyperbola×ellipse
   junction, unit + ×4000 scale, Simpson-referenced volume) +
   frustum∖corner-notch (edge-pierce triple junction, both scales) +
   axis-parallel same-type sibling. Case walls after: R0017 → kernel-v2
   `UnsupportedBooleanOutputCurve(Hyperbola)` (output-vocabulary),
   R0019 → input-B-Rep-not-2-manifold (chained), R0044 → a different
   Stage-4 junction config (v265), R0047 → §4.4.1(b) merge-pass budget
   exhausted (u32::MAX sentinel), R0049 → Stage-6 non-2-manifold,
   R0004 → its separate RevolveAxisIntersectsProfile engine error.
   Probes kept: enriched `[s4-exact-junction]` per-surface distances,
   `[s4-triple-junction]`, `YANG_TWIN_SCAN`, `[rim-junction]` operand
   dumps, kernel-v2 ring dump under `KV2_RENDER_GATE_PROBE`.
   **N2/R0017 epic increment 6 SHIPPED 2026-07-11 (spec
   `specs/kv16_hyperbola_arc_vocabulary.md`, task #124) — the
   `UnsupportedBooleanOutputCurve(Hyperbola)` output-vocabulary wall is
   GONE, plus two Stage-4 root causes and the cone-patch EllipseArc
   extension found by driving R0017 through it.** kernel-v2 gains
   `Curve::HyperbolaArc` end-to-end (classify with on-branch endpoint
   certification, bit-identical twins — the open-branch/SurfacePair
   traversal convention, not the ellipse's directional one; sag-bisection
   render sampling; planar + developable-patch tessellation arms; planar
   winding midpoints; exact `ab·(Δt − sinh Δt)` planar segment area;
   `to_yang_brep` re-entry at both conversion sites) and yang-rs Stage 1
   ingests `Curve::Hyperbola` chains (asinh param, sag bisection,
   `loop_polyline` + both lateral CDT gates + the S3
   `ellipse_rim_chord_bound` vocabulary). Stage-4 fixes: (a) SAME-TYPE
   hyperbola×hyperbola junctions (prism-edge × cone pierce — both curves
   in the ONE `vert_cone_hyperbola` slot) are detected at insert and
   routed to the increment-5 triple relocation (`same_type_junction`);
   (b) `relocate_onto_implicit_triple` now feeds Newton TRUE cone
   distances (`f·cosα`) — the radial-deviation form diverges at 60°
   half-angles (sec α ≈ 2 overshoot; why 30° fixtures never saw it).
   Both mutation-verified on R0017. kernel-v2 cone patches also admit
   EllipseArc boundaries (endpoint-azimuth walk + sampled unroll — the
   KV6c increment-5 "later slice" reject retired; `signed_volume` conic
   flux stays typed). R0017's auto-union now SUCCEEDS; the case stops at
   its op-3 cut's Stage-3 `AmbiguousCurve{0,0}` (the R0003/R0008 class).
   Tests: `kv16_hyperbola_boolean.rs` (exact-volume union + re-entry
   chain), geom round-trips, rim_junction same-type pierce pin. Named
   residue: sibling conic maps (`vert_cone_ellipse`/`vert_parabola`) have
   the same latent overwrite trap (no corpus driver); d>1 wrapping-loop
   cone patches still can't re-enter Stage 1 (KV14 Slice E). Probes:
   `KV_HYPERBOLA_PROBE`, `YANG_SAMETYPE_PROBE` (+ `[triple-bail]`).
   **N2 epic increment 7 SHIPPED 2026-07-11 (spec
   `specs/kv16b_cone_ellipse_same_type_junction.md`, task #127) — the
   `vert_cone_ellipse` same-type overwrite residue is FIXED (the ellipse
   sibling of increment 6's item 1; it turned out to have FOUR corpus
   drivers, all failing kernel-v2's `"output ellipse-arc endpoint does not
   lie on its ellipse"` certification).** Measured on R0004: a narrow cone
   (2.53° half-angle, far apex) sectioned by two prism facet planes, both
   ellipses, meeting at a chord vertex (off-cone 1.06e-3) → second
   `vert_cone_ellipse.insert` silently overwrote the first → single-curve
   relocation onto the surviving ellipse → 8e-5 off the other. Fix =
   insert-time differing-descriptor detection into `same_type_junction`
   (the KV16 recipe verbatim; the existing increment-5 triple relocation
   consumes it). Unit fixture
   `same_type_ellipse_edge_pierce_endpoints_on_curve` (30° frustum ∖
   45°-rotated diamond prism — all four prism faces section the cone in
   ellipses; corner edges pierce the lateral): RED at unit scale (9.77e-4
   residual — unlike KV16's benign hyperbola pierce) → GREEN. Case walls
   after: R0004 → cone-patch "exactly one material-CCW loop" (FaceId 517),
   R0100 → holed-lateral CDT (the F0072/R0061 class). R0009/R0091 keep the
   ellipse-endpoint MESSAGE but are a DIFFERENT mechanism — micro-scale
   ellipses (a≈1e-4, b≈1e-5, coords at the 100µm floor, residuals 2-8e-8
   vs the 1e-9 band; no junction involved) — the KV15b sub-floor
   mint-accuracy family, NOT the junction class. `vert_parabola` keeps the
   latent trap (still no driver). New probes: `YANG_RUN_PROBE`
   (boolean-call separator for multi-op probe streams),
   `YANG_S3_ELLIPSE_PROBE` (Stage-3 ellipse assignment census with
   surface pair).
   **ERROR-census campaign 3 SHIPPED 2026-07-08 (spec
   `specs/cut_consumes_body.md`):** the EmptyBooleanResult cluster
   (R0023/R0027/R0058/R0088) is ONE mechanism — a cut whose tool ENGULFS
   the whole target; yang's all-inside classification and kernel-v2's
   typed `EmptyBooleanResult` are both CORRECT, but the engine recorded an
   operation ERROR instead of applying body-lifetime policy. Fix: new
   typed `KernelError::BooleanEmptyResult` (waffle-types, A6.2) mapped by
   the adapter; `modeling_ops::execute_boolean` turns Subtract/Intersect
   empties into zero-output OpResults with a consumed-body warning
   (Union-empty stays loud — a kernel defect must not masquerade as
   consumption; mutation-killed); the engine's Cut/Intersect arm forwards
   per-target diagnostics into engine_warnings. Campaign suite
   `test-harness/tests/cut_consumes_body_campaign.rs` (engine fixture +
   disjoint-volume intersect adversary + 4 corpus trackers) 6/6 GREEN.
   **ERROR-census campaign 4 SHIPPED 2026-07-08 (spec
   `kv9_f3_output_vertex_identity` §4 row E-V6):** the short-loop cluster
   (R0046/F0064/R0088, `"output loop with fewer than 3 edges"`) was a
   vocabulary gap — a genuine D-FACE (circular-segment face: chord + conic
   arc between the same two vertices; R0046's plane∩cylinder cap fragment,
   chord 0.197 on the r=0.130 circle). `from_yang_brep` now accepts 2-edge
   loops with exactly one `Seg` + one conic arc (two `Seg`s and same-curve
   arc pairs stay rejected; mutation-killed). R0046's long-standing
   output-loop wall is GONE (→ typed UNSUPPORTED curved re-entry); F0064 →
   face-normal/Newell disagreement wall; R0088 → render tessellation wall
   (FaceId 492). Trackers `dface_bigon_campaign.rs` 3/3 GREEN.
   **UNIFYING EPIC (2026-07-16, task #169, plan
   `specs/yang_mesh_updating_epic.md`):** this N2 thread + #137 (torus∩plane
   grazing-corner assembly) + #168 (degenerate re-CDT) are ONE machinery — the
   §4.4.1 mesh-update + §4.5.2 local-refinement LOOP. The built-but-unwired
   primitives (`stage4_mesh_update`, `cdt_polygon_with_holes_keep_interior`,
   `replan_degenerate_cylinder_patches` gated, `torus_plane_clip_junction`) all
   stalled on the SAME wall: **two-sided conformality** (re-mesh one operand's
   patch → the neighbour across the shared curve must get the identical vertex
   chain, else non-manifold). The epic solves that ONCE (Phase A) then wires the
   rest (Phase B §4.4.1 → C §4.5.2 → D #137 corner). This is now the DEFAULT
   kernel priority over Stage-4 relocation-band tuning (data: ~45 of ~54 corpus
   failures are structural, not tolerance — memory
   `feedback_stop_band_tuning_build_mesh_updating`). Route M5 torus∩torus
   (R0044/R0096) to the SSI track, NOT here.
4. **N5 — Stage-1 discretization bypasses the unified §4.1 d_ε-iterate + §4.1.2
   CDT framework** (per-surface ad-hoc Newell fans / rim rings instead).
5. **N6 — §4.5.4 illegal-self-intersection detection/removal is absent.**
6. **Scope (deferred, signed off):** **N7** — Stage-3 SSI is exact closed-form
   (`ssi-rs`) rather than Yang's Newton/geometric mesh optimization; this is a
   *sound, superior* substitution for analytic surfaces (invariant A15) and the
   correct choice — it only becomes a gap under NURBS. **NURBS/Bézier** (§4.1.1
   subdivision, §4.1.2 NURBS-boundary CDT) is a separate architectural milestone.

The shortest honest path to a first functional boolean was the M0–M8 milestones
(§4); the path to a kernel that **replaced legacy** was the Phase 1–6 completion
roadmap (§4b) — both now largely DONE. The path to a kernel that is **faithful to
Yang** is closing the deviations above, led by the general Stage-0 program.

### 0.3 yang-rs module layout (2026-07-10 decomposition)

`crates/yang-rs/src/lib.rs` (22.6k lines) was split move-only into
stage-aligned modules (spec `specs/yang_rs_lib_decomposition.md`, 10
commits, zero behavior change; public API unchanged via `lib.rs`
re-exports). Historical `lib.rs:<line>` references in the milestone
logs below remain valid as history — grep the FUNCTION NAME to find
code today. The map:

| Module | Contents |
|---|---|
| `lib.rs` (~160 lines) | mod decls + `pub use` re-exports + `native_backend()` |
| `geom.rs` | `Surface`/`Curve` + conic evaluators, `signed_distance_to_surface` |
| `brep.rs` | B-Rep topology types, `TessellationMap`, attribution, `BRep` |
| `errors.rs` | `YangError`, `Stage4InvalidReason`, `SsiRefinementError` |
| `stage0.rs`, `coplanar_overlay.rs` | (pre-existing) Stage-0 coplanar preprocessing |
| `stage1_tessellate.rs` | Stage-1 tessellators, chord bounds, rim/band builders |
| `stage3_ssi.rs` | Stage-3 SSI refinement (`build_intersection_curves`) |
| `stage4_relocate.rs` | relocation primitives (`relocate_onto_implicit_pair/triple`, Reloc types) |
| `stage4_correct.rs` | Phase-A census, `collapse_vertex`, `stage4_relocate_and_correct`, sweeps |
| `stage4_dt.rs`, `stage4_update.rs` | (pre-existing) N2 CDT + mesh updating |
| `stage5_topology.rs` | `reconstruct_topology(_stage4)`, `emit_topology`, patches/loops |
| `boolean.rs` | the `boolean()` driver, provenance, coplanar-scan glue |
| `tests_unit/` | the former in-file `mod tests`, split by campaign group |

`stage0.rs` got the same treatment same-day (spec
`specs/stage0_decomposition.md`): now `stage0/` — mod.rs (doc + structs +
`stage0_preprocess`) with frame / reloc / rim_chords / disc_pair /
mesh_build / cylinder siblings, test mods with their subject files.
Remaining follow-ups: tighten the `use crate::*` / `use super::*` wildcard
imports the moves left behind; `kernel-v2/src/boolean.rs` is the next
god-module candidate.

## 1. Thesis: decouple "functional Yang" from "native arrangement complete"

The prior roadmap gated real Yang Stage 5/6 on a *complete native `cherchi-rs`
arrangement* — a large, entirely unwritten graph algorithm. That coupling is why
the project shipped throwaway substitutes instead of a validated vertical slice.

We break the coupling with a producer-agnostic **`LabeledArrangement`**
interface (§2). An *interim* producer (patched C++ sidecar, §3a) satisfies it
now, so `yang-rs` Stage 5/6 becomes **real** in weeks. The *native* `cherchi-rs`
arrangement (§3b) is then built behind the **same** interface, with the sidecar
as its differential-parity oracle.

## 2. The `LabeledArrangement` interface (the contract)

Defined **once, here**. Crate `CLAUDE.md` files reference this section; they must
not redefine the shape. Freeze it only after round-tripping the two validation
cases in §3a.

> **Revised 2026-05-29 (M2), solid-level provenance.** The original contract
> below asked for `source: SmallVec<(InputId, parent_tri_index)>`. Inspecting the
> Cherchi 2022 C++ source showed the input-*triangle* index is **lost** during
> arrangement subdivision — daughter triangles inherit only the parent's
> mesh-level label (`labels.surface`, a bitset of which input *solid*). Recovering
> a triangle index would need an invasive patch to arrangement internals we don't
> own. Yang reassembles *faces*, not triangles, and the face is recoverable from
> solid-id + plane-membership (the exact arrangement keeps each non-coplanar
> sub-triangle in its source face's plane) — which yang-rs does in M3. So the
> contract is **solid-level**; the triangle index is dropped. See
> `specs/yang_m2_labeled_arrangement.md`.

The output of Yang Stage 2 — the **full** arrangement mesh (all sub-triangles,
before any op filter; yang-rs does its own op selection). Per **output triangle**:

- `surface: Vec<InputId>` — which input solid(s) the triangle lies on. **len ≥1
  normally; len ≥2 only at coplanar overlap** (an output triangle can belong to
  both A and B — Cherchi 2022 §3). A scalar would silently mis-attribute coplanar
  faces (the case the legacy port died on).
- `inside: Vec<bool>` — in/out per input solid (`inside[k]` = the triangle is
  inside solid `k`). Captured **before** the op filter collapses it.
- `patch_id: u32` — Cherchi's connected same-surface patch (its own Stage-5
  grouping), one per triangle.

**Division of labour.** `yang-rs` owns the mesh→B-Rep mapping via its Stage-1
`TessellationMap` + geometric plane-membership. The producer reports only
**solid-level** provenance + in/out + patch; `yang-rs` composes: output tri →
(producer: solid A/B) + (geometry: which of that solid's face-planes contains it)
→ B-Rep face.

## 3. Producers

### 3a. Interim — patched `mesh_booleans` sidecar  *(chosen path)*

Cherchi 2022 already tracks per-output-triangle origin internally (§3 of the
paper: *"for each output triangle we propagate information on its origin"*) and
classifies patches in/out per input. The work is to **emit** it, not compute it.

- **Patch location:** reach into `customBooleanPipeline` (pre-filter), not just
  `main.cpp` — the op-specific selection collapses the per-input in/out vector,
  so dump the labels *before* that filter.
- **Format:** a sidecar file written alongside the result OBJ, encoding the
  `LabeledArrangement` shape from §2 (per-tri source list + patch id; patch
  in/out table).
- **Validate before freezing the interface:** round-trip on (1) two tetrahedra
  (clean 1:1 provenance) AND (2) one coplanar-overlap case (multi-attribution).
  Only then is §2's shape frozen.

`cherchi-sidecar-rs` owns this producer and the C++ patch.

### 3b. Native — `cherchi-rs` Stage 2, same interface

**Status: ✅ COMPLETE (M6, 2026-06-10).** Landed as PR-CR-AR1→AR3b (arrangement)
+ PR-CR-BL1→BL3 (labeling/boolean): `NativeBoolean` is parity-green vs the C++
sidecar on the BL3b corpus and is yang-rs's production backend (BL3c); the
sidecar remains as the test-only parity oracle. M7 (clean-room predicates →
WASM) is ✅ COMPLETE (PR-CR-M7a/b/c, 2026-06-10); remaining native-track work
is M8 (coplanar Stage 0).

Built incrementally, **diffed against the sidecar** on the corpus. The IP-FFI
predicates (`indirect-predicates-sidecar-rs`) are consumed **demand-driven** by
this code — we stop porting predicates ahead of a caller. Per user directive,
the native path used the FFI predicates *first*; the clean-room
reimplementation from Attene's paper then restored WASM (M7 ✅, PR-CR-M7c
swapped every consumer to `predicates::indirect`).

## 4. Critical path & milestones (ORDERED)

> The real gate to a first boolean is **not** the label interface — it is
> Stage-1 mesh *validity*. Cherchi loops forever on malformed input: fed real
> F0002 tessellation, `mesh_booleans` failed all three `inputcheck` predicates
> and ran ~6 h before being killed. The native arrangement would hit the same
> wall. So M1 precedes M2.

- **M0 — Operationalize the parity oracle.** ✅ **DONE** (`scripts/build_sidecars.sh`;
  the C++ sidecars build, `indirect-predicates-sidecar-rs` runs in available mode
  (42 tests), and the `cherchi-sidecar-rs` / `cherchi-rs` parity tests exercise
  the real binary instead of self-skipping).
- **M1 — Stage 1 emits Cherchi-`inputcheck`-clean meshes.** ✅ **DONE** (convex
  planar scope). `yang-rs` Stage 1 (`BRep::new`) canonicalizes each face's
  triangle winding to its analytic `Surface::Plane.normal` (Newell normal +
  dot-sign reverse); degenerate/sub-feature-area faces → `YangError::DegenerateFace`.
  Cube + tetrahedron pass all five `inputcheck` axioms against the real binary.
  Spec: `specs/yang_m1_stage1_orientation.md`; commits `f423581d` (spec) →
  `a66460f6` (RED) → `7da238d4` (GREEN) → `24e73307`/`d356297b` (adversarial
  area-threshold fix). **Scope:** convex planar faces only; non-convex/holes are
  banked (PR-YR2b–d) and not yet made inputcheck-clean.
- **M2 — Patched sidecar emits `LabeledArrangement`.** ✅ **DONE.** A
  version-controlled C++ patch (`patches/cherchi2022_labeled_arrangement.patch`,
  applied by `scripts/build_sidecars.sh`) dumps, per arrangement triangle, the
  surface solid(s) + per-solid in/out + Cherchi patch id; `cherchi_sidecar_rs::
  labeled_arrangement()` parses it into a `cherchi_rs::LabeledArrangement` (the
  **frozen, solid-level** §2 shape). Acceptance oracle green: `keep_set(op)`
  reproduces the stock `boolean(op)` triangle set for all four ops; coplanar
  cubes yield real multi-attribution; deterministic (TBB pinned to 1 thread).
  Spec: `specs/yang_m2_labeled_arrangement.md`; commits `0d321e6a` (spec+§2) →
  `3add0ebd` (RED) → `cd78d15b` (C++ patch+build) → `68bceb66` (GREEN Rust) →
  `b091553d` (adversarial + env hardening).
- **M3 — `yang-rs` Stage 5/6 consume true labels → FIRST functional boolean.**
  ✅ **DONE.** `boolean()` consumes the `LabeledArrangement` (via the new
  `MeshBoolean::labeled_arrangement` seam), welds the arrangement mesh,
  `keep_set(op)`-selects + orients (`flip_for_op`) the kept tris, resolves each
  tri's source face geometrically (centroid-in-plane, `TAU_WORK`; degenerate
  edge-slivers attributed to the lowest tied face), and reassembles via
  `reconstruct_topology` into a **watertight 2-manifold B-Rep**. Verified on
  independent interpenetration geometry (not just the canonical cubes): signed
  volumes exact (union/intersect/subtract), 0 unpaired half-edges, Euler V−E+F=2.
  **Scope:** Union/Intersect/Subtract on interpenetrating convex planar solids.
  **Deferred:** Xor (multi-shell — gated loudly via `YangError::UnsupportedOp`);
  coplanar overlap (M8, `FaceResolutionFailed`); curved surfaces/SSI (M5).
  Spec: `specs/yang_m3_functional_boolean.md`;
  commits `4f206b27`→`4bac08cb`→`a945e037`→`d81eeda4`→`f43294c2`.
- **PR-YR5c — B-Rep faces with inner loops (holes).** ✅ **DONE.** When one solid
  pierces a hole through another's face, `reconstruct_topology` now builds the
  annular face (multi-cycle boundary extraction; outer = largest-|area| cycle,
  the rest are holes; cavity-wall normals flipped to point result-outward) instead
  of erroring `NonManifoldOutput`. **Impact:** the randomized box-boolean fuzz
  (`tests/fuzz_boxes.rs`, 900 cases) went from **75.2% → 100%** correct
  (aligned 86.2→100%, rotated 64.2→100%), eliminating the entire
  `NonManifoldOutput` bucket, with `SILENT_WRONG` still 0. Genuine non-manifold
  (T-junction/dead-end) and nested holes still error loudly.
  Spec: `specs/yang_pr_yr5c_inner_loops.md`; commits
  `ed550ae5`→`bbb14283`→`d90aa5f1`→`59771f86`→`287ea5ee`.
- **M4 — Retain YR3/4/5 substitutes as a `#[cfg(test)]` differential oracle.**
  ✅ **DONE** (bundled with M3). `match_with_input`/`face_candidates`/
  `majority_vote` moved to `#[cfg(test)]`; differential test cross-checks the
  real-label attribution against the substitute. Not deleted.
- **M5 — Stage 3/4 SSI + CDT refinement** (faceted → surface-exact). `ssi-rs`
  solvers + mesh-updating CDT along refined curves. Stage 5/6's
  *patch-segmentation* logic is durable; only its *curve-source* changes — build
  the seam there.
  - **Decision (curve representation):** the kernel uses **true analytical
    curves** (a plane∩sphere edge is a `Circle`, not a polyline) with **f64
    parameters** — zero shape error; topology robustness stays in the exact mesh
    predicates (cherchi/dashu). This is the Yang/Parasolid/ACIS model and is NOT a
    deviation (SSI *is* Yang Stage 3). Faceted-but-displayed-smooth was rejected
    (inexact geometry that compounds through chained ops).
  - **Step 1 — `ssi-rs` exact-SSI foundation (PR-SSI1) ✅ DONE.** Stood up the
    crate's public surface (`QuadricSurface{Plane,Sphere}`, `SsiCurve{Line,Circle}`,
    `SsiError`, `eval`, deterministic `in_plane_basis`) + the first 3 closed-form
    solvers (`plane_plane`, `plane_sphere`, `sphere_sphere`, each citing
    Patrikalakis §5.8) + `intersect()` dispatch. On-surface oracle (sample curve →
    satisfy both surfaces) + analytical-geometry + symmetry + determinism oracles.
    Adversary: no bugs; near-tangent guards short-circuit before `sqrt` (no NaN);
    solvers relative-correct to ≥1e9 scale; the absolute on-surface oracle is
    bounded to coordinate magnitude ~1e8 (recorded in spec — relative residual for
    larger-scale Stage-3 consumers). Spec `specs/ssi_pr_ssi1_foundation.md`;
    commits `8b1c7282`→`7255b380`→`c001101e` (RED)→`a508e865` (GREEN)→`c4e1efe0`
    (adversary). 28/28 ssi-rs tests; CI gate clean; no sibling/legacy changes.
  - **Step 2 — `ssi-rs` plane∩cylinder (PR-SSI2) ✅ DONE.** A15.4 pair #2: adds
    the `QuadricSurface::Cylinder` surface and the first non-circular curve
    (`SsiCurve::Ellipse` + its `eval`), with the `plane_cylinder` solver (C1
    perpendicular→Circle, C2 oblique→Ellipse `a=r/|c|`, C3a parallel-secant→two
    Lines, C3b tangent→one Line, C3c disjoint→[], E1 degenerate→Err) and the first
    triggerable `AnalyticalSolutionNotAvailable` path (sphere∩cylinder). Stays in
    closed-form conic territory — no Degree-4 quartics. **Adversary found a real
    bug:** the C1 band, gated on `1−|c|`, let the snap-to-perpendicular circle sit
    up to `√(2·TAU)·r ≈ 4.5e-4·r` off the cutting plane (~4000× tolerance) because
    the off-plane error scales with the *sine* `√(1−c²)`. Fixed (RED→GREEN
    sub-cycle) by gating C1 on `|proj|=√(1−c²)<TAU_MODEL` (the axis's in-plane
    projection norm, which also unifies with C2's `normalize(proj)` guard) →
    off-plane error bounded by `r·TAU_MODEL`. Adversary also confirmed a C2
    ellipse's on-surface residual tracks `r`, not the (possibly huge) major axis.
    Spec `specs/ssi_pr_ssi2_plane_cylinder.md`; commits `b53e566c`→`22729f1f` (RED)
    →`394f772a` (GREEN)→`5a3cded6` (spec fix)→`9a8c6c37` (adversary RED)→`37e17ff7`
    (fix GREEN). 55/55 ssi-rs tests; CI gate clean; no sibling/legacy changes.
  - **Step 3 — `ssi-rs` plane∩cone, bounded sections (PR-SSI3) ✅ DONE.** A15.4
    pair #3: adds the `QuadricSurface::Cone` surface (infinite double cone, pure
    quadric) and `plane_cone` for the **bounded** sections — C1 circle (plane ⟂
    axis) + C2 ellipse (closed section) — reusing `Circle`/`Ellipse`. Classifies
    via the two symmetry-plane generators `g_±=cosα·â±sinα·û`; ellipse params from
    the vertex method + closed-form `b²=(d·â)²/cos²α−|d|²`. **Scope (user decision):
    bounded first** — parabola/hyperbola (PH) and through-apex (AP) return loud
    `Err` (`AnalyticalSolutionNotAvailable`/`DegenerateInput`), a deliberate staged
    gap removed in PR-SSI4, never a fallback (A15.2). On-surface oracle uses a cone
    **radial** residual (length). Adversary (17 attacks) confirmed the dangerous
    ellipse↔parabola boundary is robust (huge `a` stays finite + on-surface; clean
    flip to `Err` at the `gd_±` gate; no NaN/Inf/misclassification) and flagged a
    minor C1-gate conditioning inconsistency vs `plane_cylinder` — fixed for
    consistency (gate on the stable `|n̂−k·â|`, not `√(1−k²)`; reuse `proj` for `û`).
    Spec `specs/ssi_pr_ssi3_plane_cone.md`; commits `d0f3bfe1`→`f16f9fbd` (RED)→
    `014e7445` (GREEN)→`f3cacaae` (adversary)→`64047b06` (spec fix)→`ddc5e2be`
    (consistency fix). 86/86 ssi-rs tests; CI gate clean; no sibling/legacy changes.
  - **Step 4 — `ssi-rs` plane∩cone unbounded conics (PR-SSI4) ✅ DONE.** Completes
    the **four proper** plane∩cone sections: adds the first two **unbounded**
    `SsiCurve` types — `Parabola { vertex, normal, axis_dir, focal_length }` and
    `Hyperbola { center, normal, major_axis, semi_transverse, semi_conjugate }` —
    each with its own `eval`, and the `plane_cone` PARA/HYPE branches replacing the
    SSI3 staged `Err`. On the infinite double cone a hyperbola returns **two**
    branch curves (`±major_axis`, `+m̂` first). Constructions hand-verified before
    coding (hyperbola α=π/4 plane x=1 → center(1,0,0),a=b=1,vertices(1,0,±1);
    parabola → vertex(½,0,½),f=1/(2√2),eval(1)=(0,−1,1) on both surfaces). The new
    PH contract obsoleted PR-SSI3's staged-gap assertions (2 `ssi3.rs` placeholders
    + 6 `ssi3_adversary` attacks) — migrated to the new contract, **adversary-
    verified faithful** (no attack weakened; the sweep guard tightened). Adversary
    (13 attacks): no bugs; both classification boundaries clean (no NaN/blow-up/
    misclassification); parabola on-surface to coord ~1e8, hyperbola exact to T≈7.
    Through-apex degenerate conics (point/line/two-lines) **deferred to PR-SSI5**
    (still `Err(DegenerateInput)`). Spec `specs/ssi_pr_ssi4_plane_cone_unbounded.md`;
    commits `d7cbbd8a`→`39841dc5` (RED)→`b1f5da9f` (GREEN + faithful fixture
    migration)→`38f7d553` (adversary). 108/108 ssi-rs tests; CI gate clean.
    **plane∩{plane,sphere,cylinder,cone} now complete for all proper conics.**
  - **Step 5 — `ssi-rs` plane∩cone through-apex degenerate conics (PR-SSI5) ✅
    DONE — plane∩cone now COMPLETE.** Replaced the AP `Err(DegenerateInput)` with
    the degenerate result: point → `Ok([])` (`|k|>sinα`, incl. ⟂); one Line
    (`|k|=sinα`, tangent generator); two Lines (`|k|<sinα`, crossed generators
    through the apex). No new curve types (reuses `Line`); sub-case classified by
    the proven `gd_±` sign test (`gd₊·gd₋=k²−sinα²`); two-line dirs `cφ·m̂±sφ·ŵ`.
    Hand-verified (α=π/4, n̂=(1,0,0) → (0,∓1,1)/√2 = `z²=y²`; tangent → (−1,0,1)/√2;
    ⟂ → []). The new AP contract obsoleted PR-SSI3's AP assertions (2 ssi3 tests +
    1 adversary attack) — migrated to the new contract, **adversary-verified
    faithful**. Adversary (13 attacks): no bugs; clean monotone point↔line↔two-line
    boundary; clean AP-detection band; lines exact on both surfaces. Spec note: the
    tangent sub-case is a ~1.4e-7-wide k-window (intrinsic to the exact `k=sinα`
    degenerate). Spec `specs/ssi_pr_ssi5_plane_cone_through_apex.md`; commits
    `e974295b`→`476fc663` (RED)→`c2d9ed47` (GREEN)→`9d109bef` (adversary). 129/129
    ssi-rs tests; CI gate clean. **plane∩{plane,sphere,cylinder,cone} fully done.**
  - **Step 6 — `ssi-rs` sphere∩cylinder coaxial (first degree-4 pair, PR-SSI6) ✅
    DONE.** The degree-4 pairs' **coaxial/special** configs reduce to analytic
    conics (research-confirmed; the legacy code does the same). PR-SSI6 ships the
    first: coaxial sphere∩cylinder (axis through sphere center) → circles
    (`z²=r_s²−r_c²`): X2 two circles at `C±h·â`, X1 one tangent great circle, X0
    empty. Reuses `Circle` — no new curve type, no enum-match migration.
    **Non-coaxial (general degree-4) → `Err(AnalyticalSolutionNotAvailable)`** — a
    staged gap (the general degree-4 curve needs a new `SsiCurve` variant, deferred),
    never a fallback. Establishes the coaxial-detect→reduce-to-circles→general-ASNA
    pattern for the other circle-reducible pairs. **UPDATE (2026-07-08, M5 Option
    B ✅ SHIPPED for cyl×cyl):** the staged ASNA is now resolved for
    cylinder×cylinder — the general degree-4 curve is the **procedural
    surface-pair curve** (`SsiCurve::SurfacePair{a,b}` → yang `Curve::SurfacePair`
    → kernel-v2 `Curve::SurfacePair`), defined implicitly by its two exact
    surfaces and certified by Newton projection onto both (P8 degree-4
    clarification; `specs/m5_surface_pair_curve.md`). Stage-4 relocation reuses
    `relocate_onto_implicit_pair` as a sibling of the torus block. Corpus C0052
    ERROR→CORRECT; `unequal_perpendicular_now_supported` green. Cone-pair operand
    (R0008/R0003 AmbiguousCurve class) is the next producer. Adversary (15 attacks): no bugs;
    clean tangent + coaxial-detection boundaries; two characterized absolute-`TAU`
    ceilings (on-surface ~1e9, coaxial-detection ~1e8 → conservatively NC). Spec
    `specs/ssi_pr_ssi6_sphere_cylinder_coaxial.md`; commits `16dca4a0`→`818f2882`
    (RED)→`de7926c4` (GREEN)→`614292b1` (adversary). 141/141 ssi-rs tests; CI clean.
  - **Step 7 — `ssi-rs` sphere∩cone coaxial (second degree-4 pair, PR-SSI7) ✅
    DONE.** Reuses the SSI6 coaxial-detect→reduce-to-circles→general-ASNA pattern.
    Coaxial sphere∩cone (sphere center on the cone axis) reduces to one/two circles
    via `sec²α·h² − 2h0·h + (h0²−r_s²)=0`, roots `h=(h0±√D)·cos²α`,
    `D=sec²α·r_s²−h0²tan²α`. Gate on the **linear** gap `g=r_s−|h0|·sinα`
    (`sign(D)=sign(g)`, since `D=sec²α(r_s−|h0|sinα)(r_s+|h0|sinα)`) per the
    SSI2/3/6 lesson, so X2 (`g>TAU`) guarantees `D>0` and `√D` is safe: X2 two
    circles (`+√D` first), X1 one tangent circle (`|g|≤TAU` at `h_t=h0·cos²α`), X0
    empty (`g<−TAU`). Reuses `Circle` — no new curve type, no enum-match migration.
    **Non-coaxial (general degree-4) → `Err(AnalyticalSolutionNotAvailable)`** —
    staged, never a fallback. Adversary (18 attacks): no bugs; clean tangent
    (α≠π/4 exercised) + coaxial-detection boundaries; characterized absolute-`TAU`
    ceilings (on-surface ~1e8→1e9, coaxial-detection ~1e8→1e9 → conservatively NC)
    and the apex-grazing `r_s=|h0|` radius-0 point-circle degeneracy (downstream
    filters it). Spec `specs/ssi_pr_ssi7_sphere_cone_coaxial.md`; commits
    `6d58f415` (spec)→`9144dfd4` (RED)→`8b12402d` (GREEN)→`d575280b` (adversary).
    189/189 ssi-rs tests; CI clean.
  - **Step 8 — `ssi-rs` cylinder∩cone coaxial (third degree-4 pair, PR-SSI8) ✅
    DONE.** Reuses the SSI6/SSI7 coaxial-detect→reduce-to-circles→general-ASNA
    pattern. Coaxial cyl∩cone (axes parallel AND cyl axis_point on the cone axis
    line) reduces to **exactly two circles** at `h = ± r_c·cotα` via the classical
    `|h|·tanα = r_c` reduction. **Unlike SSI6/SSI7 there is NO discriminant / √ /
    tangent / empty branch** — the cone's `[0,∞)` per-nappe radial range meets the
    constant `r_c` at exactly one height per nappe, so valid coaxial input is
    *always* two circles; manufacturing a discriminant to mirror SSI7 would be a
    hack-to-pattern (P9/P10) and is prohibited. Branches: X2 (two circles, h>0
    nappe first) / **NC** (non-coaxial → `Err(AnalyticalSolutionNotAvailable)`,
    staged, never a fallback) / E1 (`r_c≤0`/non-finite, bad α, zero cone/cyl axis →
    `DegenerateInput`). Reuses `Circle` — no new curve type, no enum-match
    migration. RED enforces the anti-hack invariant (a 5×5 α/r_c sweep asserting
    `len()==2` always). Adversary (13 attacks): no bugs; parallelism + on-axis gate
    boundaries (each flips at `TAU_MODEL`), α near both E1 limits, reversed/
    antiparallel axes, a 525-config determinism sweep, and characterized
    absolute-`TAU` ceilings (on-surface oracle holds to r_c≈1e8 → breaks at 1e9;
    `d_ax` coaxial band holds to scale ~7e8 → conservatively flips to ASNA by ~1e9)
    plus the in-band snap-to-cone-axis slack. Spec
    `specs/ssi_pr_ssi8_cylinder_cone_coaxial.md`; commits `7d820153` (spec)→
    `45c4eed1` (RED)→`d25fa0cb` (GREEN)→`e3285699` (adversary). 217/217 ssi-rs
    tests; CI clean.
  - **Step 9 — `ssi-rs` cone∩cone coaxial (fourth & LAST circle-reducible
    degree-4 pair, PR-SSI9) ✅ DONE.** Reuses the SSI6/7/8
    coaxial-detect→reduce-to-circles→general-ASNA pattern. Coaxial cone∩cone (axes
    parallel AND apex₂ on the cone₁ axis line) reduces along the shared axis via
    `|t|·tanα₁ = |t−δ|·tanα₂` (`δ` = signed apex offset, `t` = axial height from
    apex₁) to the quadratic `(m₁²−m₂²)t² + 2m₂²δt − m₂²δ² = 0`. **No manufactured
    discriminant/√ sign gate (P9/P10):** the discriminant `(2m₁m₂δ)²` is a
    **perfect square** ⇒ always ≥0, so unequal-α offset input is *always* exactly
    two circles; the equal/unequal split and the apex-collapse are gated on the
    **linear** quantities `|α₁−α₂|` and `|δ|`, never on a square. Branches: X2 (two
    circles at `t=(−m₂²δ±m₁m₂|δ|)/(m₁²−m₂²)`, larger-t first) / X1 (equal α, offset
    → one circle at the bisector `t=δ/2`) / X0 (unequal α, apexes coincide →
    `Ok(vec![])` radius-0 point-circle) / **CO** (equal α + coincident → identical
    double cone → `Err(DegenerateInput)`) / **NC** (non-coaxial →
    `Err(AnalyticalSolutionNotAvailable)`, staged, never a fallback) / E1 (bad α
    either cone, zero axis either cone → `DegenerateInput`). Reuses `Circle` — no
    new curve type, no enum-match migration. RED enforces the anti-hack invariant
    (unequal-α × δ≠0 sweep asserting `len()==2` always). Adversary (10 attacks): no
    bugs; parallelism + on-axis gate boundaries (each flips at `TAU_MODEL`), the
    `|α₁−α₂|` equal/unequal and `|δ|` collapse boundaries, reversed/antiparallel
    axis-sign set-invariance, α near both E1 limits, a 40-config determinism sweep,
    cross-branch argument-swap symmetry, and characterized absolute-`TAU` ceilings
    (on-surface oracle holds to ~1e8 → breaks at 1e9; coaxial band conservatively
    flips to ASNA at large scale) plus the apex-grazing radius-0 (X0) collapse.
    Spec `specs/ssi_pr_ssi9_cone_cone_coaxial.md`; commits `da98380e` (spec)→
    `cc61f1bb` (RED)→`f960895d` (GREEN)→`6027d7c1` (adversary). 245/245 ssi-rs
    tests; CI clean.
  - **Step 10 — `ssi-rs` cylinder∩cylinder parallel axes → lines (PR-SSI10) ✅
    DONE.** First of the two cyl∩cyl special cases. Parallel-axis cyl∩cyl reduces
    to **circle∩circle** in the plane ⟂ the shared axis `û`, lifted along `û` →
    **lines** (reuse `SsiCurve::Line`). Inter-axis distance `d = |rel − (rel·û)·û|`;
    chord offset `a = (d²+r₁²−r₂²)/(2d)` along `n̂`, half-chord `h = √(r₁²−a²)`,
    `p̂ = û×n̂`, points `Q₁+a·n̂ ± h·p̂`. Gate on the **linear** `d` vs `r₁±r₂`:
    E1 (`DegenerateInput`) → NP (`|û₁×û₂|≥TAU` → ASNA) → coincident (d≤TAU, equal r
    → `DegenerateInput`, 2D overlap) → concentric (d≤TAU, unequal r → empty) →
    disjoint/contained (empty) → tangent (one line) → secant (two lines, +h·p̂
    first). Non-parallel stays ASNA (the equal-R intersecting → ellipses case is
    PR-SSI11). Reuses `Line` — no new curve type, no enum-match migration.
    **Adversary found a real bug:** a non-finite `axis_point` (NaN/Inf) leaked a
    NaN-bearing `Line` instead of erroring — `d=NaN` compares false against every
    branch threshold, so control fell through to the secant branch; the radius and
    axis_dir guards did not cover the point. Fixed with an early `axis_point`
    finiteness guard → `DegenerateInput` (the coaxial-detect siblings degrade to
    NC→ASNA on a NaN point, so the leak was unique to cyl∩cyl's curve-producing
    fall-through). Adversary 13 attacks (parallelism/tangent/coincident boundaries,
    argument-swap line-SET symmetry, antiparallel/non-unit axes, 36-config
    determinism sweep, characterized absolute-`TAU` oracle ceiling ~1e8 via an
    oblique config). **Process note:** the SSI10 worker hit the account usage limit
    mid-cycle after committing spec+RED; the interactive driver completed GREEN
    against the worker's frozen RED suite (test-author ≠ implementer preserved),
    spawned a distinct Adversary sub-agent, and fixed its finding. Spec
    `specs/ssi_pr_ssi10_cylinder_cylinder_parallel.md`; commits `fed67c3c` (spec)→
    `b53e55a2` (RED)→`7100c143` (GREEN)→`f0927aec` (adversary)→`721d7b23` (fix)→
    `7a18bb66` (adversary verify). 277/277 ssi-rs tests; fmt + clippy clean.
  - **Step 11 — `ssi-rs` cylinder∩cylinder equal-R intersecting axes → two
    ellipses (PR-SSI11) ✅ DONE.** Second of the two cyl∩cyl special cases, and
    the LAST circle/conic-reducible quadric pair. Two cylinders of **equal radius**
    whose axes are **coplanar and intersect** (non-parallel) meet in **two
    ellipses** lying in the angle-bisecting planes (Patrikalakis & Maekawa §5.8) —
    reuses the existing `SsiCurve::Ellipse` variant, no new curve type, no
    enum-match migration. **Unequal-radius or skew (non-coplanar) axes stay staged
    `Err(AnalyticalSolutionNotAvailable)`** — the general degree-4 curve, deferred,
    never a fallback (A15.2). Built via the role-separated RED/GREEN/ADVERSARY
    cycle (test-author ≠ implementer). Spec
    `specs/ssi_pr_ssi11_cyl_cyl_equal_r_ellipses.md`; commits `7f6e2d44` (RED)→
    `6bdcb05a` (GREEN)→`2e5e6e6f` (adversary). With Step 11, **ALL
    circle/conic-reducible coaxial & special-case quadric pairs are now complete.**
  - **PR-YR6 — curved `Surface`/`Curve` types + loud rejection (first Phase-2
    step). ✅ DONE.** Extends `yang-rs`'s `Surface` enum (`Sphere`, `Cylinder`,
    `Cone`) and `Curve` enum (`Circle`, `Ellipse`) with field shapes mirroring
    `ssi-rs` `QuadricSurface`/`SsiCurve` field-for-field (so the future Stage-3
    mapping is a trivial copy; radially-outward convention, no `sense` field).
    The pipeline **accepts curved faces at the type level** but **rejects them
    LOUDLY** — new `YangError::CurvedSurfaceNotYetSupported { face }` returned at
    the three `Surface::Plane` destructure sites (`BRep::new` winding
    canonicalization is the observable one; `boolean()` `plane_dist` closure and
    `reconstruct_topology` surface inheritance are defensive). P9/P10: never a
    panic, silent skip, or planar approximation. **No `ssi-rs` call and no curved
    tessellation exist yet** — this is a pure type extension. Spec
    `specs/yang_rs_curved_surface_curve_types.md`; role-separated cycle, commits
    `441e8748`/`076bf661` (RED + integration-test contract migration)→`0afdc6a3`
    (GREEN)→`07f6d12e` (adversary).
  - **PR-YR7 — P2a curved Stage-1 tessellation: CYLINDER only. ✅ DONE.**
    First curved-geometry *processing* step. `BRep::new` now dispatches by face
    surface type: a closed-solid cylinder (encoded with a seam edge — lateral
    `Surface::Cylinder` + 2 planar disk caps, no `BRepFace` two-loop change)
    tessellates into a watertight, chord-error-bounded mesh (`d_ε = 1e-2 ×
    AABB_diag`, `N` from `r·(1−cos(π/N)) ≤ d_ε`) with a correct
    `TessellationMap`. A shared per-`Circle`-edge rim-ring pre-pass gives
    cap+lateral identical rim vertices (watertight via shared indices, not
    snap-weld); `ortho_basis` is shared by sampling AND the new infallible
    `BRep::eval_source` bijection inverse (the round-trip oracle); the
    opposite-rim-normal twist is resolved by axis-frame azimuth alignment.
    Adds `signed_distance_to_surface` (Plane+Cylinder; Sphere/Cone loud) wired
    into `boolean()`'s distance closure. **No boolean wiring, no `ssi-rs` call,
    no exact intersection curves.** Sphere/Cone still reject loudly; the planar
    box path is unchanged; `reconstruct_topology` still defers cylinder (P2c).
    Cylinder-on-a-triangle is now `MalformedTopology` (lacks its 2 `Circle`
    rims), not `CurvedSurfaceNotYetSupported`. Spec
    `specs/yang_pr_yr7_cylinder_tessellation.md`; role-separated cycle, commits
    `16570a20` (spec)→`aca9d7e4` (RED + contract migration)→`b3dc3f65`
    (GREEN)→`81a3abcf` (adversary).
  - **PR-YR8 — P2c first curved boolean: cylinder ∪ box (mesh-approximate). ✅ DONE.**
    A curved solid runs through the WHOLE pipeline for the first time:
    `boolean(cylinder, box, Union)` flows Stage 2 (sidecar `LabeledArrangement`)
    → Stage 5/6 reassembly, and a kept lateral patch emits a `BRepFace` carrying
    `Surface::Cylinder` with the **input's exact parameters** (governance A15 —
    the mesh is a tool, the analytic surface is the truth). Two honest fixes:
    **(Blocker 1)** Stage-6 face resolution gains a **per-face membership
    tolerance** — `TAU_WORK` for `Plane`, the surface's own Stage-1 chord bound
    `d_ε` for `Cylinder` (NOT tolerance widening; the same bound Stage 1
    guarantees). The `1e-2 × analytic-AABB-diag` math is extracted into ONE
    shared `curved_chord_bound` helper consumed by both `BRep::new` (Stage-1
    `n_seg`) and face resolution (A14.3 single source). Applied to BOTH the
    non-degenerate count rule AND the degenerate-sliver branch (the sidecar
    emits a near-zero-area sliver ON the cylinder lateral whose centroid is
    ~`d_ε` inside the analytic surface — the §4-literal "keep TAU_WORK" was a
    planar-world assumption; the governing principle applies to any triangle on
    a curved face). Byte-for-byte identical for all-planar inputs (every face
    uses `TAU_WORK`; an all-planar solid has `band == None`) — **fuzz_boxes
    900/900 correct, 0 silent-wrong**. **(Blocker 2)** `reconstruct_topology`
    gains a `Surface::Cylinder` branch BEFORE the planar Newell/flip machinery:
    inherit the surface unchanged (Union = no cavity → no sense flip; curved
    Subtract cavity-sense deferred), reuse `patch_boundary_cycle`, keep the E2
    degenerate-loop guard, DROP the E3/`positive_count` + inherited-normal flip,
    deterministic loop assignment (most-edges = outer, tie-break lowest min
    start-vertex), edges = `Curve::LineSegment`. Sphere/Cone still loudly reject
    everywhere. **Verified against the live Cherchi sidecar:** cylinder ∪ box is
    watertight (0 unpaired half-edges), Euler V−E+F=2, analytic `Surface::Cylinder`
    survives — no F3 tie, no `NonManifoldOutput` (spec §5 STOP conditions all
    clear). **No `ssi-rs` call yet; intersection edges stayed mesh-approximate
    polylines — now made exact in PR-YR9 (P3).** Spec
    `specs/yang_pr_yr8_curved_boolean.md`; role-separated cycle, commits
    `c2a81e05` (RED)→`da85f4bd` (GREEN)→`56f395ba` (adversary).
  - **PR-YR9 — P3 Stage 3: exact intersection edges via `ssi-rs`. ✅ DONE.**
    The **first real use of `ssi-rs` inside the boolean** (Yang 2025 §4.3).
    `cylinder ∪ box` output intersection edges no longer carry P2c
    mesh-approximate `Curve::LineSegment` polylines — they carry the **EXACT
    analytical conic** from `ssi_rs::intersect`. An output intersection edge is
    an undirected mesh boundary edge incident to two patches of **different
    `InputId`** (one on a `Surface::Cylinder`, one on a box-cap `Surface::Plane`);
    `ssi_rs::intersect(Plane, Cylinder)` of those inherited surfaces is the
    plane∩cylinder solver → a `Circle` (cap ⟂ axis, canonical), `Ellipse`
    (oblique), or `Line`s (parallel). New helpers in `crates/yang-rs/src/lib.rs`:
    `surface_to_quadric` (yang `Surface` → ssi `QuadricSurface`; `Plane` point =
    `-d·n`), `ssi_curve_to_curve` (field-for-field; `Line`→`LineSegment`),
    `curve_contains_point` (implicit on-curve residual, no parameter solving),
    and `build_intersection_curves` (per A↔B edge: intersect, select the
    **unique** conic passing both endpoints within the cylinder owner's Stage-1
    chord bound `d_ε` — `TAU_WORK` for plane∩plane — keyed by canonical edge).
    `reconstruct_topology` refactored into two passes (a `PatchInfo` first pass
    owning the face-range check + inherited lookup in one place; an emission pass
    that sets each edge's `curve` via canonical-key lookup, falling back to
    `LineSegment` ONLY for non-intersection edges). The Newell/flip/E2/E3
    machinery is byte-unchanged. **P9 STOP**: a genuine `ssi_rs::intersect`
    failure or a non-unique selection (`matched != 1`) returns
    `Err(YangError::SsiRefinementFailed { edge, reason: SsiRefinementError })` —
    **never** a silent fallback to the polyline. **Scope held**: planar
    `fuzz_boxes` corpus stays all-`LineSegment` (plane∩plane → `Line` →
    `LineSegment`); same-input rim/seam edges keep `LineSegment` (no SSI entry);
    sphere/cone still loudly reject. Adversary proved the conic is analytic, not
    a mesh re-fit (**byte-identical cap `Circle` across N=8 vs N=16 facet mocks**).
    Role-separated cycle, commits `6e73a74d` (RED)→`f1c401f4` (GREEN)→`ec2b71d0`
    (adversary); spec `specs/yang_pr_yr9_stage3_ssi.md`.
  - **PR-YR10 — Stage 4: relocate mesh intersection points onto the exact
    curves + §4.5.3 reversed-point correction. ✅ DONE.** Yang 2025 §4.4.1 +
    §4.5.3 mesh updating. PR-YR9 gave the output edges exact analytical conics,
    but the **mesh** still had its intersection-edge vertices on the faceted
    polygon chords (inside the true circle by up to the Stage-1 chord bound
    `d_ε`). Stage 4 now **relocates** those crossing points radially onto the
    exact `Curve::Circle` (closed-form `project_onto_circle`, reusing
    `ortho_basis` so the angle `t` round-trips through `eval_source`), retags
    each moved/on-curve vertex's `TessellationSource` to `BRepEdge{edge,t}`, and
    runs the §4.5.3 reversed-intersection sweep on the ordered, oriented conic
    loops (discrete tangent `t̃` vs curve tangent; reversal ⟺ unsigned angle ∈
    (45°,135°), 1e-6 rad slack; edge-collapse the next point, reconnect, repeat;
    collinear `t̃` = healthy). **Watertightness is INHERITED** from the
    mesh-boolean output and gated by a combinatorial `check_watertight_2manifold`
    (half-edge pairing + Euler χ=2) — **not** a global CDT (per §4.4.3). Seam:
    `reconstruct_topology` takes `&mut Mesh`, enters Stage 4 on ANY analytic
    conic edge (Circle **or** Ellipse), and returns the per-vertex source vector
    `boolean()` uses for the output `TessellationMap`; Phase-B emission is
    otherwise unchanged. **Verified on the live sidecar**: `cylinder ∪ box`
    relocates every cap-ring crossing onto the exact circle to `TAU_MODEL`,
    chord deviation drops, output stays watertight χ=2; the adversary's
    independent threshold-free geometric audit (1000×1000 per-cap winding sweep +
    net-signed-area = exact analytic region to ~1e-16) found **NO cap fold**.
    **Scope / loud STOPs (P9/P10)**: circle projection only — an `Ellipse`
    intersection edge → `Err(Stage4RegionInvalid{EllipseProjectionUnsupported})`;
    §4.5.2 real local refinement is a loud `LocalRefinementRequired` STOP (the
    canonical fixture never triggers it); `OnAxis`/`OffCurveBeyondChordBand` are
    defensive guards (public-path-unreachable — the upstream YR9
    `curve_contains_point(·, d_ε)` selection is the same, strictly-tighter band,
    so a pathological crossing is rejected by `SsiRefinementFailed`/
    `FaceResolutionFailed` first; a `processed`-set no-skip audit forbids silently
    passing any conic endpoint — the failure mode of the **disproven**
    insert-and-fan attempt, branch `wip/yr10-insert-fan-disproven` commit
    `46980456`, which must NOT be repeated). **Planar path byte-identical**
    (Stage 4 early-returns when no conic edge exists; `fuzz_boxes` 900/900
    unchanged). One faithful spec→reality correction (adversary-verified, NOT a
    hack-to-green): the §4.5 step-4 literal per-facet "winding vs analytic
    normal, dot>0" gate was **removed** — on a faceted curved surface a facet
    normal legitimately deviates from the pointwise centroid normal, and a cap
    facet's kept winding is reconciled downstream by `reconstruct_topology`'s
    Newell orientation pass; orientation correctness is delegated to the §4.5.3
    sweep (loop monotonicity) + the watertight gate, exactly where Yang §4.4.3
    places it. Sphere/Cone still loudly reject. Role-separated cycle, commits
    `5a2da9f0`/`d7540578` (spec)→`d4bbe446`/`03464b29` (RED + fixture
    recalibration)→`e49a5a93` (GREEN)→`d402aa80` (adversary); spec
    `specs/yang_pr_yr10_stage4_relocate.md`.
  - **PR-YR11 — Stage 4 OBLIQUE: relocate onto the exact ellipse. ✅ DONE.**
    Lifts PR-YR10's `EllipseProjectionUnsupported` STOP for oblique
    `cylinder ∪ box`: an `Ellipse` intersection edge now relocates via the
    **cylinder parameterization** (snap radius at angle θ, then snap axial to the
    cutting plane → lands on `cylinder ∩ plane` exactly, closed-form, no quartic;
    Yang §4.3.2), with the §4.5.3 reversal sweep extended to `Ellipse` loops and
    the N3 degenerate-tangent fix preserved. **Verified on the live sidecar:** a
    *contained* oblique `cylinder ∪ box` (tilt `unit([1,0,3])`, axis through the
    box centre so both cap ellipses + the body stay inside the unit box — no
    side-face exit) relocates every crossing onto the exact ellipse (on BOTH the
    cylinder and the cutting plane to `TAU_MODEL`), chord deviation drops, output
    watertight χ=2. yr10 `t4` migrated Err→Ok (the Ellipse edge now relocates, not
    rejects — faithful contract migration). **Out of scope (deferred):**
    side-face-exit / ellipse∩line corner (triple-point) configs — the contained
    fixture avoids them; a loud-STOP guard for them is a follow-up. Commit
    `e72f2313`; `tests/yr11_stage4_ellipse.rs`. 170/170 yang-rs; fmt + clippy clean.
  - **PR-YR12 — P2b sphere Stage-1 tessellation. ✅ DONE.**
    The remaining curved Stage-1 primitive (after the PR-YR7 cylinder). A closed
    solid sphere — one `Surface::Sphere` face bounded by a single `Curve::Circle`
    meridian seam + 2 pole `BRepVertex` (no `BRepFace` two-loop change) —
    tessellates via `BRep::new` into a watertight (χ=2) lat/long grid mesh with a
    bijective `TessellationMap`. Fixed **z-up** parameterization
    `face_eval(u,v)=center+r·(cos v cos u, cos v sin u, sin v)` (a sphere is
    isotropic — an oriented sphere is a documented out-of-scope limitation, like
    the cylinder needing `axis_dir`); chord bound **`d_ε = 1e-2·2r√3`** (the
    sphere's exact AABB space diagonal — diameter `2r` ≠ diagonal); grid `n_lon`/
    `n_lat` refined honestly (segments sized to `d_ε/2` so triangle *centroids*
    stay ≤ full `d_ε` — more triangles, **never** tolerance widening; worst
    centroid dev 0.82·d_ε at n_lon=17/n_lat=9 for the unit sphere). Poles are the
    shared seam-vertex indices and the seam column is reused via modular wrap →
    watertight with no weld/snap/synthetic fill (verified on the live Cherchi
    `inputcheck` sidecar). `eval_source` Sphere FACE arm is byte-identical to
    `face_eval` (round-trip exact to 1e-9 over pole/seam/interior verts);
    `signed_distance_to_surface` Sphere → `|x−center|−r`. The rim-ring pre-pass
    excludes sphere-seam Circle edges so the **cylinder path is byte-for-byte
    unchanged** (`tests/yr7_cylinder.rs` diff empty). **Cone still rejects**;
    sphere-on-a-triangle is now `MalformedTopology` (lacks its seam Circle), a
    faithful guard migration swept across yr6/yr7/yr8/yr9 (cone arms keep their
    exact `CurvedSurfaceNotYetSupported { face: N }` assertions). **No boolean
    wiring, no `ssi-rs`, no exact intersection curves, no NURBS.** Spec
    `specs/yang_pr_yr12_sphere_tessellation.md`; role-separated cycle, commits
    `07c8cbe3` (spec)→`ee66cca3` (RED)→`b5b17e47` (GREEN)→`7e96e070` (adversary).
    184/184 yang-rs; fmt + clippy clean.
  - **PR-YR13 — curved `Subtract` box − cylinder, cavity-sense via
    `BRepFace.reversed`. ✅ DONE.** The first M5 increment after the curved
    `Union` chain (PR-YR8–YR11) and the curved Stage-1 primitives (PR-YR7/YR12).
    Closes the curved cavity-sense gap banked in PR-YR8 for the **`box − cylinder`
    BLIND POCKET** (genus 0, χ=2). A new `BRepFace.reversed: bool` records that a
    face's effective outward normal (outward from the result solid) is the
    **negation** of the surface's canonical analytic outward normal: the surviving
    cylinder-lateral cavity wall is emitted as `Surface::Cylinder` with the input's
    **exact** params and `reversed == true`, so its effective normal points
    **toward the axis** (into the pocket). `reversed` is derived from the SAME
    `flip_for_op` signal that flips the mesh winding —
    `op == Subtract && info.input == InputId::B` (threaded `boolean()` →
    `reconstruct_topology_stage4` → `emit_topology`) — so face sense and mesh
    winding are **provably consistent** (witnessed absolutely: the emitted
    cavity-wall mesh-triangle winding normals point toward-axis and the result has
    positive signed volume). Planar faces keep encoding sense in the
    possibly-flipped `Plane.normal` and stay `reversed == false` (no double-flip);
    surface params are never perturbed to signal sense. Union + planar Subtract
    are byte-identical (`reversed == false` everywhere). Faithful `reversed: false`
    migration swept across all `tests/*.rs` + the `#[cfg(test)]` lib fixtures
    (additive only). Adversary independently witnessed mesh↔`reversed` consistency
    on a second outward-oriented mock + mutation-verified the derivation is
    load-bearing. **Remaining curved-Subtract gaps:** through-hole (genus 1, χ=0),
    sphere/cone cavities (`Cone` still rejects loudly), box-as-subtrahend,
    side-face-exit / corner (triple-point) guard; cut-surface faces (PR-YR5
    deferral) still open. **No new `ssi-rs`.** Spec
    `specs/yr13_subtract_cylinder_cavity_sense.md`; role-separated cycle, commits
    `c4abc69d` (spec)→`78f73f65` (RED)→`42972890` (GREEN)→`3839f558`/`41819459`
    (RED fixups)→`85abbc10`/`86791834` (adversary). 195/195 yang-rs; fmt + clippy
    clean.
  - **PR-YR14 — through-hole genus-1 Subtract; per-shell Euler gate generalized
    to χ=2−2g. ✅ DONE.** Extends curved `Subtract` from PR-YR13's BLIND POCKET
    (genus 0, χ=2) to a **THROUGH-HOLE**: the cylinder passes fully through the
    box (both caps OUTSIDE the box) → a cylindrical tunnel, which is a single
    connected closed orientable 2-manifold of **genus 1 → χ = 0**. The ONE
    production change is in `check_watertight_2manifold`: the per-shell Euler gate
    was `V−E+F == 2` ("each shell is a sphere"), which wrongly rejected the χ=0
    result. Generalized to accept **χ = 2−2g for g ≥ 0** (χ even, ≤ 2) and reject
    odd χ or χ > 2 — impossible for a closed orientable manifold, so still a LOUD
    `NonManifoldOutput` (NOT a tolerance/fallback relaxation; P9/P10). The directed
    half-edge pairing loop stays strict and untouched. Everything else the
    through-hole needs already worked and is REUSED unchanged: the curved cavity-
    sense (`BRepFace.reversed` from `op==Subtract && input==B`, PR-YR13) on the
    tube wall, the annular box top+bottom faces (PR-YR5c multi-cycle /
    `positive_count==1`), the two-rim tube wall (one connected same-attribution
    patch → `patch_boundary_cycle` returns its two boundary cycles → curved branch
    emits outer+inner loops), and the **two** exact `Circle` rim edges (cylinder ∩
    box-top at z=2 AND cylinder ∩ box-bottom at z=0). Adversary independently
    witnessed mesh-winding ↔ `reversed` consistency on a SECOND outward-oriented
    through-hole mock (r=1.5, N=24, signed_volume>0, χ=0, wall winding toward-axis)
    and mutation-verified the χ relaxation is LOAD-BEARING for the accept path
    (reverting to `!= 2` turns the through-hole oracles red). Honest coverage note:
    the χ-clause's REJECT branch (odd/`>2`) is mutually shadowed on the reachable
    corpus by the half-edge-pairing and coincident-triangle guards — defects are
    still loudly rejected, never `Ok` (oracle `a6` pins this). All genus-0 cases
    (`fuzz_boxes`, YR8–YR13, YR13 blind-pocket χ=2) byte-unchanged. **Remaining
    curved-Subtract gaps:** sphere/cone cavities (`Cone` still rejects loudly),
    box-as-subtrahend, side-face-exit / corner (triple-point) guard; cut-surface
    faces (PR-YR5 deferral) still open. **No new `ssi-rs`.** Spec
    `specs/yr14_subtract_through_hole.md`; role-separated cycle, commits
    `36d2a7c4` (spec)→`aeefb4cc` (RED)→`b52a78a3` (GREEN)→`6995b6ec` (adversary).
    208/208 yang-rs; fmt + clippy clean.
  - **PR-YR15 — box − sphere HEMISPHERICAL DIMPLE (genus 0); `Surface::Sphere`
    wiring + `sphere_chord_bound` single-source helper. ✅ DONE.** Extends the
    curved `Subtract` cavity path to a **spherical** cavity: a sphere centred ON
    one box face (poking through exactly that face) so `box − sphere` carves a
    hemispherical dimple — a single shell, **genus 0 (χ=2)**, ONE exact great-
    `Circle` rim (`sphere ∩ box-face plane`, great because the centre is ON the
    plane), and a cavity wall that is the inside hemisphere (`Surface::Sphere`,
    `reversed=true`, effective outward normal pointing INTO the dimple toward the
    centre). The cavity-sense mechanism (`BRepFace.reversed` from
    `op==Subtract && input==B`, PR-YR13) and the per-shell Euler gate (χ=2−2g,
    PR-YR14) are surface-agnostic and REUSED unchanged. The work was honest wiring
    of an already-type-supported surface, NOT new mechanism: `Surface::Sphere` was
    loudly rejected at production sites that each mirror the existing `Cylinder`
    arm. **The plan named three sites; the live-sidecar oracle surfaced two more**
    — both governed by the spec's I-sphere-band invariant (a sphere face uses its
    OWN Stage-1 chord bound `sphere_chord_bound(radius)=1e-2·2r√3`, NOT the rim-
    AABB `curved_chord_bound`=2r√2, which underestimates). Final FIVE faithful
    edits: (1) `surface_to_quadric` Sphere→`QuadricSurface::Sphere` (enables the
    exact `plane ∩ sphere` rim); (2) `sphere_chord_bound` free helper extracted
    from `tessellate_sphere_face` (A14.3 single source) + the `tol_for` Sphere arm;
    (3) `emit_topology` curved-branch guard broadened to
    `Cylinder | Sphere` (body unchanged — already surface-agnostic);
    (4) `build_intersection_curves` selection tol (factored to
    `chord_tol_for_curved_owner`; sphere arm uses `sphere_chord_bound`);
    (5) `stage4_chord_band` relocation budget via new `input_curved_chord_bound`
    (max of rim-AABB and per-sphere-face bound). **No tolerance widening, no
    fallback** — each uses the surface's GUARANTEED Stage-1 bound; `Cone` stays a
    loud reject at every site; cylinder/all-planar inputs byte-for-byte. Adversary
    mutation-verified BOTH extra sites are load-bearing (each mutation reds a
    distinct oracle: `AmbiguousCurve{matched:0}` and `Stage4RegionInvalid{
    OffCurveBeyondChordBand}`), witnessed mesh-winding ↔ `reversed` consistency on
    a SECOND independent OUTWARD-authored off-axis mock (center (1,−0.5,5), r=1.5,
    different facet counts; χ=2, signed_volume>0, cap winding toward-centre), and
    confirmed no migration weakened. **Honest coverage note:** the two extra
    band sites are exercised only via the sidecar-backed oracle (the C++
    `mesh_booleans` binary is present in this env, so they ARE verified here); the
    mock-driven oracles 1–4 bypass real SSI/Stage-4, so on a machine WITHOUT the
    sidecar those two edits are untested — a future mock-path Stage-3/4 sphere-rim
    oracle would close that gap. All prior cases (`fuzz_boxes`, YR8–YR14)
    byte-unchanged. **Remaining curved-Subtract gaps:** cone cavities (`Cone`
    still rejects loudly), fully-internal spherical voids (multi-shell),
    through-sphere, box-as-subtrahend, side-face-exit / corner (triple-point)
    guard. **No new `ssi-rs`** (the `plane_sphere` solver already existed). Spec
    `specs/yr15_subtract_sphere_dimple.md`; role-separated cycle, commits
    `4b0b5af0` (spec)→`945c1c12` (RED)→`6e6239f0` (GREEN)→`bcdeecc4` (adversary).
    219/219 yang-rs; fmt + clippy clean.
  - **PR-YR16 — CONE Stage-1 tessellation (CONE only, no boolean); all three
    curved primitives now tessellate. ✅ DONE.** The cone was the last curved
    primitive still rejecting everywhere (`Surface::Cone` →
    `CurvedSurfaceNotYetSupported`). This PR teaches `BRep::new` to tessellate a
    closed solid cone, verified by the same 4-oracle contract as PR-YR7/YR12
    (surface-to-mesh ≤ d_ε, watertight + 2-manifold + env-gated `inputcheck`,
    bijection round-trip, Euler χ=2) over a corpus of 4 (z-up unit, wide-short,
    tall-thin, off-axis non-unit axis). **Encoding (minimal, justified):** the
    cone lateral is topologically a DISK — its only boundary is the base circle,
    the apex is a single interior singular point — so NO seam edge (unlike the
    cylinder). `verts=[apex, base_seam]`; one shared base-rim `Curve::Circle`
    (shared by lateral + base cap = the watertightness mechanism); faces
    `[Cone lateral, Plane base cap]`. The apex is a pre-seeded `BRepVertex`
    located in `tessellate_cone_face` by exact `TAU_MODEL` position match (no
    duplicate → watertight + Euler hold; round-trip via `BRepVertex`).
    **Tessellation = apex fan + base cap fan** over the shared rim ring (cap
    reuses the existing `tessellate_cap_face` unchanged). Because the cone is
    **ruled** (straight generators apex→rim, exactly on the surface), the worst
    residual anywhere on a lateral triangle — including its centroid — is the
    base-rim sagitta `R·(1−cos π/N)`; there is NO centroid amplification (unlike
    the sphere) and NO `/2` factor in N-sizing. **`cone_chord_bound(h,α)=
    1e-2·√((2R)²+h²)`** is the new single source (A14.3), folded into the rim
    pre-pass via `min(curved_chord_bound, cone_chord_bound)` ONLY when a cone
    face is present (cylinder/sphere/planar paths byte-for-byte) — the min is
    load-bearing for wide-short cones (`h<2R`, where the rim-AABB bound
    overestimates the honest bound). **Tilted outward normal** (A15.5)
    `n̂=unit(r̂−tanα·â)` ⟂ the generator (new `cone_outward_normal`). Three Stage-1
    production sites changed (`BRep::new` dispatch, `eval_source` cone-FACE arm,
    `signed_distance_to_surface` signed radial residual); the boolean-path cone
    rejections (`surface_to_quadric`, `emit_topology`, Stage-6 reassembly) stay
    LOUD — the cone never enters the boolean this PR. After this PR
    `CurvedSurfaceNotYetSupported` is no longer reachable from `BRep::new` for any
    curved surface on a triangle (all → `MalformedTopology`); it survives only on
    the boolean Stage-6 paths. **Guard migration:** the plan named 9 sites; the
    spec sweep surfaced 3 more (+1 inline `src/lib.rs` test) — exactly the
    under-enumeration the `yang_curved_primitive_guard_migration` lesson and the
    YR15 precedent anticipate — all migrated faithfully (only the expected
    outcome changed; every structural assertion preserved). **Two honest
    adversary findings (documented, no defect):** (1) oracle 1's 3-verts+centroid
    sampling could not distinguish the rim-only N from the correct N (the
    distinguishing sample is the base-edge MIDPOINT at f=1) — the adversary added
    `adv_wide_short_base_edge_midpoint_within_cone_bound`, which reds the
    dropped-`min` mutation with the exact residual-exceeds-bound assertion, so the
    min IS mutation-verified load-bearing; (2) the tilted normal produces a
    BYTE-IDENTICAL mesh to the pure-radial normal for the current pure apex-fan
    (`orient_tri`'s binary flip is identical at every steepness since both r̂ and
    n̂ share the fan triangle's half-space) — it is the *correct* surface normal
    but **orientation-dead-code until interior-ring (non-fan) triangles appear**
    (i.e. PR-YR17 cone cavity → a YR17 winding canary). The adversary pinned the
    math witness (`n̂·ĝ≈0` vs `r̂·ĝ≈0.565`) rather than fabricating a false catch.
    Independent second off-axis mock witnessed outward winding + `signed_volume>0`
    (per `yang_mock_orientation_witness`). 232/232 yang-rs; fmt + clippy
    `--all-targets` clean. Spec `specs/yr16_cone_tessellation.md`; role-separated
    cycle, commits `8e569c14` (spec)→`8ceb8d65`/`8d2fe8c6` (RED + clippy
    fixup)→`7f0dfe4e` (GREEN)→`6013a1fc` (adversary). **Next: PR-YR17 cone cavity
    `box − cone`.**
  - **PR-YR17 — box − cone CONICAL POCKET (curved `Subtract`, genus 0). ✅ DONE.**
    Closes the loop PR-YR16 opened: a cone with its **apex inside the box** (pocket
    bottom, `(0,0,0.5)`) and its **base above the box top** carves a conical pocket
    via `box − cone`. Result = a single genus-0 shell (χ=2): cavity wall = the cone
    lateral apex→rim (`Surface::Cone`, `reversed == true`), rim = exact `Circle`
    (`cone ∩ box-top plane`, a **perpendicular** cut → `ssi-rs` `plane_cone` C1
    branch → `radius = |h|·tanα`), apex = a singular pocket-bottom vertex closing
    the fan, box-top = an annular planar face (rim hole). This is pure
    **composition** — the cavity-sense mechanism (`BRepFace.reversed = op==Subtract
    && input==B`) is surface-agnostic and unchanged; the job was flipping the cone's
    loud-rejects to real wiring mirroring the Cylinder/Sphere precedent.
    **Production sites (`src/lib.rs` only, NO `ssi-rs` change):** the spec named
    FOUR — `emit_topology` curved-branch `matches!` (admit `Surface::Cone`),
    `emit_topology` defensive arm (drop Cone), `surface_to_quadric` (field-for-field
    → `QuadricSurface::Cone`, enabling the exact rim `Circle`), and `tol_for` (the
    cone's OWN `cone_chord_bound`, **height derived from the rim Circle in the cone
    face's outer loop** via the Stage-1 pre-pass idiom `|(rim_center−apex)·â|` —
    single-source bound, NOT tolerance widening). The GREEN implementer surfaced a
    **FIFTH** site (exactly the `yang_curved_primitive_guard_migration`
    under-enumeration the YR15/YR16 cycles anticipate): `build_intersection_curves`
    Stage-4 rim-curve selection had Cylinder/Sphere chord-tol arms but no Cone arm,
    so a cone∩plane rim edge fell to `TAU_WORK` and failed `curve_contains_point`
    against the exact circle (`AmbiguousCurve{matched:0}`). Added
    `cone_chord_tol_for_owner`, a faithful mirror of `chord_tol_for_curved_owner`
    (same loud-on-missing-rim producer-fault path, the cone's own single-source
    `cone_chord_bound`). **Confirm-or-STOP (P9/P10):** the `tol_for` cone-height
    anchor was verified (temporary `eprintln!`, removed) before coding; no
    widening, no fallback. **Honest adversary findings (no defect):** a second,
    distinct conical-pocket mock (shallow box, apex `(0,0,0.25)`, `tanα=2`) witnessed
    winding ↔ `reversed` sampling **edge midpoints**; mutation-verified that flipping
    `reversed` (M1), perturbing the cone params (M2), killing the SSI Cone arm (M3a),
    and breaking the fifth-site bound→`TAU_WORK` (M3b, reds the sidecar oracle), and
    flipping the tilted-normal **sign** (M4) each red a DISTINCT oracle. **M4b**
    (pure-radial, *correct* sign) reds NO oracle — **confirming** the YR16 finding:
    the cone cavity wall is still a pure apex-fan, so `orient_tri`'s binary flip is
    byte-identical for `r̂` and `n̂=unit(r̂−tanα·â)`; the **sign** is load-bearing, the
    tilt **magnitude** stays orientation-dead-code until interior-ring (non-fan)
    triangles appear (per `yang_cone_tessellation_oracle_findings`). The fifth site
    is verdicted a **faithful extension, not a tolerance hack** (M3b proves the bound
    does real work). The env-gated `Subtract` sidecar-parity oracle ran for REAL
    (default sidecar present), exercising the full pipeline (real `plane_cone` →
    Circle) end-to-end. Curved `Subtract` now covers **cylinder + sphere + cone**.
    **Still deferred (LOUD):** through-cone / cone-base-subtracted (two rims),
    **oblique cuts** (ellipse / parabola / hyperbola rims — the `plane_cone` non-C1
    branches), fully-internal cone void (multi-shell), side-face / corner
    (triple-point) exit, box-as-subtrahend. Full `yang-rs` crate green; fmt + clippy
    `--all-targets` clean. Spec `specs/yr17_subtract_cone_cavity.md`; role-separated
    cycle, commits `f9d597d8` (spec)→`f6a06012` (RED)→`f21434a1` (GREEN)→`741b50f1`
    (adversary).
  - **Non-convex / holed planar Stage-1 tessellation** ✅ (PR-NC1): planar faces
    with a reflex vertex (non-convex outer loop) **or** inner loops (holes) now
    tessellate via a constrained Delaunay triangulation
    (`cherchi_rs::cdt_polygon_with_holes`, backed by `spade` v2) instead of the
    fan path. No interior Steiner points, no boundary subdivision (the
    `TessellationMap` 1:1-on-boundary bijection is preserved); convex/box faces
    stay byte-for-byte on the existing fan path (`fuzz_boxes` 900/900
    unregressed). Resolves the D1-class (no ear-clipping) concern for the new
    kernel's planar Stage 1. Spec `specs/yang_pr_nc1_nonconvex_cdt.md`; deviation
    ledger **N9**.
  - **Curved boolean fuzz (the robustness-envelope map)** ✅ (PR-CF1): the curved
    analog of `fuzz_boxes` — a deterministic N=300 **correct-or-loud** fuzz over
    `boolean({cylinder|sphere|cone}, box, {Union|Subtract}, &sidecar)`
    (`tests/fuzz_curved.rs`, SEED=`0xcf1cadef00d2026`). Every `Ok` is audited
    (watertight `unpaired==0` / χ **==sidecar-reference χ** & even / analytic-surface
    survival with exact params / on-surface residual ≤ `TAU_MODEL` sampled on the
    exact `Curve::Circle/Ellipse` against BOTH incident surfaces / `vol>0` /
    chord-band volume envelope scaled from the Stage-1 `d_ε`); every `Err` is
    bucketed by `YangError` variant **and sub-reason**. Empty-result agreement
    (both engines ∅ → `ok_correct`; disagreement → silent-wrong) is a deliberate
    contract interpretation, not a relaxation. **Histogram (300 cases, all
    accounted for): `ok_correct=42`, `SILENT_WRONG=0`, `classified_err=257`,
    `skipped_bad_input=0`.** This **is the M5-gap map**: the dominant loud refusals
    are `SsiRefinementFailed::AmbiguousCurve` (183 — the SSI rim-selection gap),
    `FaceResolutionFailed` (54), and cone's `Stage4RegionInvalid::LocalRefinementRequired`
    (17); cone Union/Subtract are 0/0 correct (all loud refusals — oblique/ssi gaps),
    while sphere & cylinder land some correct results. Most `Subtract` cases are
    correctly loud because `boolean(prim, box, Subtract)` = `prim − box` =
    **box-as-subtrahend**, the DEFERRED direction (opposite of the `box − prim`
    demos). **ONE genuine production defect surfaced (a P9 violation): `boolean()`
    PANICKED** on sphere − box at case#23 — `emit_topology`'s curved branch indexed
    `cycles[outer_idx]` on an **empty** `cycles`. **GREEN fix** (`src/lib.rs`,
    commit `a568d9e6`): a minimal `if cycles.is_empty() { return
    Err(NonManifoldOutput); }` guard on the curved branch (+ a defensive mirror on
    the structurally-identical planar branch, latent since the all-planar fuzz never
    produces empty cycles), mirroring the adjacent E2/E3 degenerate-reassembly
    guards — converting the panic into a loud classified `Err`, so the fuzz now holds
    correct-or-loud (panic→Err ⇒ `PANICKED=0`). **Mechanism correction (adversary):**
    the case#23 sidecar reference is **NON-empty** (272 tris, vol≈0.0485) — the empty
    curved cycles are a reassembly artifact of the **deferred box-as-subtrahend
    direction**, NOT an enclosed-sphere empty solid; the loud `Err` is a legitimate
    refusal of an out-of-scope direction, not a suppressed wrong `Ok`. **Adversary
    verdict (`tests/cf1_adversary.rs`, commit `0771dcc6`):** all invariants real and
    discriminating (proved inside-out caught by `vol>0`; determinism replays case#23
    from SEED), GREEN fix principled; one noted **non-blocking GAP** — the chord-band
    volume alone is a coarse dropped-chunk detector (~0.38 at r=0.6), so χ==ref +
    on-surface residual are the real dropped-chunk gates (by design). The asserting
    fuzz stays `#[ignore]`d (default `cargo test -p yang-rs` green); an `#[ignore]`d
    `demonstrator_case23_*` pins the seed. **Follow-up increments seeded by the
    histogram:** the SSI `AmbiguousCurve` rim-selection gap (the single biggest
    blocker to curved `ok_correct`), `FaceResolutionFailed` coverage, cone Stage-4
    `LocalRefinementRequired`, and eventually the deferred box-as-subtrahend
    direction. Spec `specs/yang_pr_cf1_curved_boolean_fuzz.md`; role-separated cycle,
    commits `f0ea2e24` (spec)→`884726f5` (RED)→`a568d9e6` (GREEN)→`0771dcc6`
    (adversary).
  - **PR-YR18 — Stage-5 intersection-edge attribution fix (the CF1 `AmbiguousCurve`
    dominant-refusal). ✅ DONE.** Re-diagnoses CF1's biggest loud bucket: the
    `SsiRefinementFailed::AmbiguousCurve` mass (183/300 in the CF1 histogram) is
    **NOT** the "SSI rim-selection gap" the CF1 note guessed — a driver
    investigation found **0 cases with `matched ≥ 2`; every `AmbiguousCurve` is
    `matched == 0`**, and the bulk is **cylinder + sphere** (both fully handled by
    `ssi-rs`), not missing conics. It is a **surface-attribution defect**:
    `compute_phase_a` pushes a patch's single `info.inherited` face surface onto
    *every* boundary edge of the patch cycle (`src/lib.rs:3279-3289`), so a seam
    edge shared by two patches gets tagged `(surfA, surfB)` and handed to
    `ssi_rs::intersect` even when **one endpoint is genuinely off one surface**
    (decisive case: a cylinder∩plane edge, `tol≈3.1e-2`, one endpoint on both
    surfaces, the other `~8.9e-2` — ~2.9× the chord band — off the plane). Such an
    edge is an internal facet edge of a *single* surface, not a true intersection
    arc; the returned curve cannot pass through both endpoints → `matched == 0`.
    The SSI math is correct (`candidates == 1`); the defect is the
    **classification**. **GREEN fix (`src/lib.rs` `build_intersection_curves`
    only):** reorder so the Stage-1 chord band `tol` is computed FIRST, then gate
    each candidate edge with an **on-both-surfaces predicate** — both mesh
    endpoints must satisfy `|signed_distance_to_surface(surf, p)| <= tol` for BOTH
    attributed surfaces — *before* `ssi_rs::intersect`. A failing edge `continue`s
    and falls through to the unchanged `Curve::LineSegment` fallback in
    `emit_topology` instead of raising `AmbiguousCurve`. **No tolerance widening
    (P9/P10):** the gate reuses the SAME per-edge `tol` the selection already uses
    (the producer-fault helpers' diagnostic-only `candidates` arg is passed `0` in
    the pre-intersect position — untested). **No-regression invariant (proof):**
    the intersection curve lies ON both surfaces, so any edge that currently
    selects `matched == 1` necessarily passes the gate — the gate is a *necessary
    condition* of existing success and cannot regress YR8–YR17 or the planar
    corpus; it only reclassifies edges that today raise `AmbiguousCurve` with an
    endpoint off a surface beyond `tol`. Coincident-plane / yr9 loud STOPs
    preserved (both endpoints on both surfaces → pass the gate → reach the loud
    path); cone conics stay loud (a true cone∩plane edge passes the gate then still
    hits `matched != 1` because `curve_contains_point` returns `false` for conics —
    correct, the deferred analytic-conic follow-up). **Before/after counts:** CF1
    baseline = `AmbiguousCurve == 183` (cylinder + sphere `matched == 0` bulk). The
    **empirical post-fix sidecar-fuzz histogram could NOT be obtained in this
    container** — the Cherchi sidecar subprocesses zombie out and the
    `fuzz_curved` harness hangs without printing a final histogram (pervasive
    un-reaped `<defunct>`/`Z` processes, some days old, independent of this
    change); repeated N=300/120/40 runs all stalled. Per
    `feedback_no_regression_chasing` / "don't loop", no numbers were fabricated.
    Correctness evidence is instead **deterministic, sidecar-free**: a RED fixture
    (`tests/yr18_attribution.rs`) that reproduces the EXACT cylinder∩plane
    `matched == 0` case (`AmbiguousCurve { candidates: 1, matched: 0 }`, edge
    `(0,1)`, off endpoint 2.90× the band) and goes GREEN under the fix; the
    no-regression invariant (proof, statically audited by the adversary); and the
    adversary over-skip guard (`tests/yr18_adversary.rs`) proving genuine cap-ring
    cylinder∩plane edges still pass the gate and emit `Curve::Circle` (the RED
    test's negative-only assertions cannot catch a degenerate skip-everything
    "fix"). Full `cargo test -p yang-rs` green; `cargo fmt -p yang-rs -- --check`
    and `cargo clippy -p yang-rs --all-targets -- -D warnings` clean. Spec
    `specs/yr18_intersection_edge_attribution.md`; role-separated cycle, commits
    `ea94cc1c` (spec)→`5536432b` (RED)→`2345b791` (GREEN)→`44dc1cde` (clippy
    chore)→docs+adversary. **Empirical delta (driver-verified post-merge, curved
    fuzz N=90 same seed, before→after):** `ok_correct` **11 → 37** (3.4×);
    `AmbiguousCurve` **56 → 30**; **cylinder `AmbiguousCurve` eliminated entirely
    (21 → 0)**; sphere materially improved (20 → 15); cone unchanged (15 → 15, the
    deferred-conic share); **`SILENT_WRONG` 0 → 0** (safety bar held). The worker
    itself could not obtain these numbers (Cherchi sidecar subprocesses zombie out
    in-container); the driver reproduced the run successfully at N=90. **Deferred
    follow-ups:** (a) analytic-conic support (`Parabola`/`Hyperbola` for oblique
    cone cuts) so true cone∩plane edges that pass the gate stop being loud — the
    remaining cone `AmbiguousCurve=15`; (b) the residual **sphere**
    `AmbiguousCurve=15` (a distinct, smaller cause — the gate only partially
    cleared sphere; needs its own diagnosis).
  - **PR-YR19 — sphere∩plane chord-band metric consistency (the residual sphere
    `AmbiguousCurve`). ✅ DONE.** Diagnoses PR-YR18's deferred follow-up (b): the
    15 residual sphere `AmbiguousCurve` cases are all `surf0=Sphere`,
    `surf1=Plane`, `candidates == 1` (sphere∩plane is never ambiguous). The mesh
    endpoints PASS the YR18 on-both-surfaces gate (within `tol` of both surfaces
    along the surface normal) but FAIL `curve_contains_point` because the
    **in-plane radial** deviation `|radial − r_circle|` exceeds the flat `d_ε`,
    even though the **sphere-normal** distance is within `d_ε`. A **metric
    inconsistency**, not a real off-curve point: `d_ε` bounds the surface-normal
    error; a vertex within `d_ε` of the sphere along its normal projects (on the
    cut plane) to an in-plane radial deviation up to `(R/r_circle)·d_ε`
    (derivation: `|p−C| = √(h²+radial²)`, `d/d(radial)√(h²+radial²) ≈ r_c/R` at
    `radial=r_c`, so `dr ≈ (R/r_c)·d_sphere`). When the cut plane is far from the
    sphere centre, `r_c` is small and `R/r_c` is large. **Approach (A)
    projection-scaled radial band** (`src/lib.rs` only): the in-plane radial band
    becomes `(R/r_circle)·d_ε` while the axial (out-of-plane) band stays `d_ε`
    (the cut plane is exact). Surface-type-gated on a `Surface::Sphere` owner via
    `source_radius: Option<f64>` — every non-sphere path (`None`) is byte-identical;
    near-tangent guard (`r_circle > MIN_FEATURE_SIZE`) fails closed. **Two sites,
    both load-bearing:** (1) selection — `curve_contains_point` + caller
    `build_intersection_curves`; (2) Stage-4 relocation — `vert_circle` extended
    to carry the source sphere radius, the combined `circle_residual > d_eps`
    guard split into per-component axial/radial bands via `circle_residual_split`.
    Fixing only site 1 would convert `AmbiguousCurve` → `OffCurveBeyondChordBand`
    with **zero net `ok_correct` gain**, so the success criterion is `ok_correct`
    **rising**, not the `AmbiguousCurve` count alone. **NOT tolerance widening
    (P9/P10):** the band is the exact geometric propagation of the same `d_ε`,
    derived not picked; a point off by more than the propagated band still STOPs
    loudly. RED `tests/yr19_sphere_chord_band.rs` (a small-cap dimple, `r_c≈0.31`,
    `R/r_c≈3.2`, rim verts authored at `dr ∈ (d_ε, (R/r_c)·d_ε)` so the band is
    magnitude-load-bearing without the sidecar) reproduces the `AmbiguousCurve`
    today and goes GREEN under both fixes. Spec `specs/yr19_sphere_chord_band.md`;
    deviation **N11** (cross-refs N10). **Driver-verified empirical delta**
    (curved fuzz N=90, same seed, before→after; the worker could not run the
    sidecar fuzz — `curved_fuzz_sidecar_zombie_blocker`): **sphere `AmbiguousCurve`
    15 → 0 (eliminated)**; sphere `ok_correct` 15 → 30 (Union 4→14, Subtract
    11→16); total `ok_correct` 37 → **52**; total `AmbiguousCurve` 30 → 15 (now
    ALL cone); **`SILENT_WRONG` 0 → 0**. Critically, **no conversion to
    `Stage4RegionInvalid::OffCurveBeyondChordBand`** (sphere has zero Stage-4
    errors post-fix) — confirming the dual-site fix yields real `Ok`, not a
    downstream swap. **Deferred (still LOUD):** the cone analytic-conic share
    (`Parabola`/`Hyperbola`, oblique cone∩plane, the remaining 15) is unaffected
    and stays out of scope.
  - **PR-YR20 — Stage-6 tiered face-resolution tie-break (the largest non-cone
    `FaceResolutionFailed` bucket). ✅ DONE.** A driver investigation (env-gated
    prints, since reverted) found 12/12 sampled curved-fuzz `FaceResolutionFailed`
    cases share ONE uniform root cause — NOT a no-match. Stage-6 geometric face
    resolution (non-degenerate branch) attributes a kept triangle to the input
    face whose surface contains the triangle **centroid** within that face's
    per-face tolerance `tol_for` (`TAU_WORK` for a `Plane`, the Stage-1 chord band
    `d_ε` for `Cylinder`/`Sphere`/`Cone`): exactly 1 hit → attribute, 0 or ≥2 →
    `FaceResolutionFailed`. Every refusal is an `n_hits == 2` tie of one shape —
    a triangle lying **exactly on a planar cap near the rim** (`dist ≈ 5.5e-17`,
    `tol = TAU_WORK = 1e-12` → HIT) ALSO falls inside the curved lateral's
    necessarily-loose chord band (`dist ≈ 7.6e-3`, `tol ≈ 2.4e-2` → HIT) →
    spurious second hit → tie → F3. The rule wrongly treated an **exact**
    `TAU_WORK` planar hit and an **approximate** `d_ε` chord-band hit as equal
    weight; the triangle's true face is the cap. **The fix (tiered tie-break,
    `src/lib.rs` non-degenerate branch only):** rank hits by **tier** — EXACT
    (`dist < TAU_WORK`, the centroid lies ON the surface) dominates BAND
    (`TAU_WORK ≤ dist < tol_for`). Attribute to the unique hit at the minimum
    populated tier; ≥2 at that tier, or no hit, still `FaceResolutionFailed`.
    `tol_for` is untouched — each face keeps its own A14.3 single-source band; we
    only break ties by the exact-vs-band tier. **All-planar byte-identity (the
    critical non-regression):** for a `Plane` `tol_for == TAU_WORK`, so a hit
    (`dist < tol_for`) means `dist < TAU_WORK` ⇒ ALWAYS EXACT tier; the BAND tier
    is unreachable for planar faces. So for an all-planar input the BAND tier is
    empty, `n_exact` == the old hit count, and the new `match` reduces
    **byte-for-byte** to the old "exactly one face within `TAU_WORK`" rule — the
    box fuzz, the m3 coplanar-tie tests, and the yr5c planar-sliver tests are
    unaffected, and genuine coplanar / multi-solid ties (≥2 EXACT) still STOP
    loudly. **Tier-by-distance, NOT a `dist/tol` ratio:** a ratio would
    distinguish two sub-`TAU_WORK` planar hits and silently flip a current planar
    F3 to an attribution, breaking that safety property. **NOT tolerance widening
    (P9/P10):** `TAU_WORK` is the existing planar tolerance reused as the tier
    boundary, not a new looser constant. The degenerate-sliver branch is left
    unchanged (it never raises F3 for a tie; minimal regression surface). RED
    `tests/yr20_tiered_tiebreak.rs` (a closed-cylinder boolean with a near-rim
    cap triangle authored at the `n_hits == 2` cap-vs-lateral tie, tie magnitude
    asserted load-bearing without the sidecar) + an all-planar coplanar-tie safety
    canary that MUST still F3; adversary adds a 0-EXACT + 2-BAND two-cylinder
    curved tie that MUST still F3. Spec
    `specs/yr20_face_resolution_tiered_tiebreak.md`; deviation **N12** (refines
    N4). **Calibrated metric:** total `FaceResolutionFailed → ~0`, cylinder
    `ok_correct` rises (the cap-tie unblocks it), **ZERO new silent-wrong / no new
    `NonManifoldOutput`**. **Driver-verified empirical delta** (curved fuzz N=90,
    same seed, before→after; worker hit `curved_fuzz_sidecar_zombie_blocker`, did
    NOT fabricate): **total `FaceResolutionFailed` 16 → 0 (eliminated)**; cylinder
    `ok_correct` 22 → 31 (Subtract now 12/12, Union 19/20 — the 1 remaining is an
    unrelated `NonManifoldOutput`); total `ok_correct` 52 → **61**; **`SILENT_WRONG`
    0 → 0**. As calibrated, cone `FaceResolutionFailed` 7 → 0 but cone `ok_correct`
    stayed 0 — the refusal shifted to the deferred `AmbiguousCurve` conics (15 → 21)
    + `LocalRefinementRequired`, exactly the intended sibling-variant shift, not a
    real failure. **Deferred (still LOUD):** cone `ok_correct` stays 0 — a cone
    triangle that stops being an F3 tie simply refuses later for the deferred
    analytic-conic reason (`Parabola`/`Hyperbola`, oblique cone∩plane; see N7 /
    N10 / N11). That is correct, not a regression.
  - **Cone analytic-conic sequence (PR-YR21→YR24, PLANNED 2026-06-03).** Cone is
    `0/26` in the curved fuzz, blocked across ALL non-perpendicular sections. The
    analytic math is DONE in `ssi-rs` (`plane_cone` returns Circle/Ellipse/
    Parabola/Hyperbola); this is purely `yang-rs` integration. Missing pieces:
    `Curve::Parabola`/`Hyperbola` variants, their `ssi_curve_to_curve` +
    `curve_contains_point` arms, `eval_source` parametric eval, and — the
    keystone — **Stage-4 relocation for cone sections** (the existing ellipse
    relocation is hard-wired to the *cylinder* parameterization, YR11 §4.3.2, so a
    cone+plane edge hits `LocalRefinementRequired` at lib.rs ~3616; this breaks
    cone ELLIPSE too, not just parabola/hyperbola). **Design:** a
    **cone-section parameterization** relocation (`project_onto_cone_section`):
    for a mesh vertex take its angle θ around the cone axis, intersect that
    generator with the cutting plane → the exact conic point. Type-AGNOSTIC
    (ellipse/parabola/hyperbola identical), the cone analog of YR11's
    cylinder-ellipse projector, avoids generic foot-of-perpendicular quartics.
    **Sequence:**
    - **PR-YR21 — cone-section relocation foundation + cone ellipse. ✅ DONE
      (2026-06-04).** Shipped `project_onto_cone_section` (closed-form: relocate a
      vertex along its azimuth's generator `g = nappe·cosα·â + sinα·r̂`, solving
      `s` so `apex+s·g` lies on the cutting plane → on BOTH cone and plane = on the
      conic; type-agnostic, reused by YR22/YR23) + `ConeEllipseReloc` +
      `cone_chord_budget_from_owner` (per-cone-face budget `cone_chord_bound`,
      height from the cone owner's rim Circle — the single source). The Stage-4
      `Curve::Ellipse` arm now branches on incidence: cylinder+plane → the YR11
      path **byte-identical**; cone+plane → the new cone relocation loop; neither →
      the existing loud STOP. cone ELLIPSE now lands end-to-end (RED oracle1/2/3/4 +
      **real-sidecar E2E oracle8** green; held loud-STOPs — asymptotic/through-apex/
      parabola/hyperbola — stay LOUD per oracle6 + the adversary suite). Zero crate
      regressions; cyl/sphere `stage4_chord_band` untouched. **Loud STOPs (P9/P10):**
      `OnAxis` (ρ<MIN_FEATURE_SIZE), `LocalRefinementRequired` for `|n·g|≈0`
      (generator ∥ plane / asymptotic) and `s≤0` (wrong-nappe / through-apex).
      **Findings:** (1) the *spec's "secondary site"* Stage-4 cone budget gate
      `OffCurveBeyondChordBand` is **defensively redundant** — it is shadowed by the
      identical upstream `on_both` gate in `build_intersection_curves` (same
      `cone_chord_bound` tol), so a beyond-band vertex is demoted to `LineSegment`
      before Stage-4 (adversary-verified; kept as a fail-closed backstop, not
      load-bearing through the public surface). (2) oracle3's chord-deviation tight
      check inherited yr11's coarse 200k-sample `dist_to_ellipse_sampled` whose
      ~1.8e-5 half-spacing floor (perimeter-7.26 ellipse) cannot resolve TAU_MODEL
      — fixed with a resolution-independent two-level refined sampler; the rigorous
      on-ellipse guarantee remains enforced by oracle2 + the real sidecar. **Step-0
      cone-refusal split / curved-fuzz `ok_correct` delta deferred to the driver**
      per the `curved_fuzz_sidecar_zombie_blocker` (the bounded E2E oracle8 on the
      real `mesh_booleans` binary stands in as the live-boolean proof; no fabricated
      fuzz numbers). Gate: cone ELLIPSE `LocalRefinementRequired` → 0 (mock + real
      sidecar). **Driver-verified delta + Step-0 split** (curved fuzz N=90, same
      seed, before→after): cone `ok_correct` **0 → 5** (Union 2, Subtract 3 — cone's
      FIRST successes); cone `LocalRefinementRequired` **5 → 0** (eliminated); total
      `ok_correct` 61 → **66**; `SILENT_WRONG` 0 → 0. **Cone-refusal split** (of the
      26 cone cases): **5 ellipse** (now ✅), **21 parabola+hyperbola** (the
      `AmbiguousCurve`, YR22/YR23 targets), **0 axis-parallel/through-apex** in this
      sample. **Next: PR-YR22 (parabola), then YR23 (hyperbola) — target the 21.**
    - **PR-YR22 — Parabola end-to-end. ✅ DONE (2026-06-04).** `Curve::Parabola`
      (mirrors `SsiCurve`) + `ssi_curve_to_curve` + `curve_contains_point` +
      `parabola_point(t)` eval (`vertex + (t²/4f)·axis_dir + t·(normal×axis_dir)`)
      + point→t (conjugate-axis) inversion; Stage-4 relocation reuses YR21's
      `project_onto_cone_section`. **Recovered finish-from-RED across a session
      limit:** the worker's RED phase (6 oracles + a real-sidecar E2E oracle8 +
      the migrated yr21 oracle6) was preserved and committed; a GREEN subagent
      implemented production; it STOPPED at a verified RED-author fixture/oracle
      bug (oracle4's per-triangle winding check false-positived on the mock's
      ring-closure scaffold). Resolution (driver + second-opinion review +
      adversary): **reframe oracle4** to the invariant production actually enforces
      — boolean output is a consistently-oriented watertight 2-manifold (0 unpaired
      half-edges, χ=2, signed volume > 0) + the per-facet degenerate-area floor —
      since production deliberately does NOT do a per-facet winding test
      (`validate_relocated_triangles`, Yang §4.4.1/§4.4.3). Adversary added 9
      canaries (no silent-wrong, eval round-trip vs independent re-impl, no
      ellipse/cylinder/circle regression, hyperbola+axis-parallel stay loud, local
      fold breaks watertight). Full `cargo test -p yang-rs` green; fmt+clippy clean.
      Commits `3cf1f482` (RED)→`18909a5d` (GREEN)→`4fc114f2` (oracle4 reframe)→
      `955ef698` (adversary). **Fuzz delta = 0 BY CONSTRUCTION** (driver-verified
      N=90, unchanged from YR21): an exact parabola section needs the cut plane
      EXACTLY parallel to a generator (θ=α), which is **measure-zero** in the random
      fuzz — random box cuts give ellipses (YR21) or hyperbolas, never exact
      parabolas. So the parabola capability is real (proven by the θ=α oracles +
      E2E) but invisible to the random fuzz. **⇒ the 21 remaining cone
      `AmbiguousCurve` are (near-)all HYPERBOLA — PR-YR23 is the high-leverage one
      for the cone fuzz number.**
    - **PR-YR23 — Hyperbola end-to-end. ✅ DONE (2026-06-04).** `Curve::Hyperbola`
      (mirrors `SsiCurve`) + `ssi_curve_to_curve` + `curve_contains_point` +
      `hyperbola_point(t)` eval (`center + a·cosh(t)·major + b·sinh(t)·(normal×
      major)`) + point→t `asinh(v/b)` inversion (the bijective `sinh` coordinate);
      Stage-4 relocation reuses YR21's `project_onto_cone_section` /
      `cone_plane_residual` / `cone_chord_budget_from_owner` UNCHANGED (no new
      relocation method). **The new mechanism = two-branch selection:**
      `ssi_rs::intersect(Plane,Cone)` returns **2** `Hyperbola` for the HYPE case
      (one per nappe, opposite `major_axis`); the existing `matched==1` loop in
      `build_intersection_curves` needed NO structural change — `curve_contains_point`'s
      `(u/a)²−(v/b)²=1` membership **with the `u>0` discriminator** rejects the
      wrong-nappe branch (`u<0`), so exactly one matches and `matched==2/0` stays a
      LOUD `AmbiguousCurve`. Membership band = the geometric residual `|F|/|∇F|`
      (first-order perpendicular distance, the hyperbola analog of the
      ellipse/parabola arms), **NOT** a flat widening (P9/P10 held; adversary
      re-verified the band scales ~linearly with off-distance). Role-separated FIP
      cycle: spec → RED (8 oracles incl. an independent 2-candidate ssi oracle, a
      two-branch-selection oracle, the YR22 reframe oracle4 invariant, and a
      real-sidecar E2E) → GREEN (production-only; **STOPPED at oracle7 P9/P10**
      rather than widen a tolerance) → **driver oracle7 reframe** (the 2·cone_d_eps
      beyond-band fixture rejects honestly with `FaceResolutionFailed` — the
      reloc-band guard is geometrically UNREACHABLE for an on-plane ring since the
      YR18 on-both gate skips the edge first, and a narrower δ lands in a
      LineSegment-fallback dead zone that would SILENTLY succeed; "spec principle
      over literal", mirroring the YR22 oracle4 reframe) → Adversary (7 attacks,
      all PASS: wrong-nappe rejection witnessed by u-sign, band-not-flat,
      no YR21/YR22 regression, no silent-wrong across δ∈{0.5,1,1.2,2}·d_ε,
      independent oracle7-honesty recompute (centroid 1.33–1.60·d_ε off-cone),
      from-scratch eval round-trip). Commits `c2e088e6` (spec)→`0e22956d` (RED)→
      `c3dc4f13` (GREEN)→`713c9901` (oracle7 reframe)→`cca14e25` (adversary). Full
      `cargo test -p yang-rs` green (yr23 8/8 + yr23_adversary 7/7); fmt+clippy
      clean; kernel-v2 (consumer) still builds. **Fuzz delta: this DOES move the
      number** (unlike the measure-zero parabola) — a random box cut of a cone is
      (near-)always a hyperbola (`plane_cone` HYPE), so the ~21 remaining cone
      `AmbiguousCurve` are (near-)all hyperbola and cone `ok_correct` should rise
      from 5 toward ~26. **NOT fabricated here** (curved fuzz can't complete
      in-container per `curved_fuzz_sidecar_zombie_blocker`); capability proven by
      the unit oracles + the real-sidecar E2E (oracle8 ran green against the C++
      binary). The worker PREDICTED cone `ok_correct` 5→~26.
      **DRIVER-VERIFIED DELTA (CORRECTION — the prediction was wrong; cone is NOT
      closed):** curved fuzz N=90 (same seed) post-YR23: cone `AmbiguousCurve`
      **21 → 4** (hyperbola selection WORKS), but cone `ok_correct` only **5 → 6**
      and the bulk **shifted to `LocalRefinementRequired` 0 → 16** (Union 5,
      Subtract 11); overall `ok_correct` 66 → 67; `SILENT_WRONG` 0. The hyperbola
      SELECTION is correct, but the YR21 cone-section relocation **breaks down for
      hyperbola points reaching toward the ASYMPTOTIC generator** (where
      `|n·g|→0` ⇒ the `project_onto_cone_section` guard fires
      `LocalRefinementRequired`). The RED oracle + E2E sample near the vertex (where
      relocation works), so they passed; the random fuzz generates arcs extending
      toward the asymptote and exposes the gap. **This is the "moved-the-failure-
      to-a-sibling-variant" pattern** (cf. memory [[fix_all_gates_sharing_a_metric]]):
      the gate is cone `ok_correct` rising, not `AmbiguousCurve` dropping. **⇒ Cone
      is NOT closed. Closing it needs a Stage-4 hyperbola near-asymptote relocation
      cycle (a real geometric gap, larger than the PR-YR24 axis-parallel triage) —
      OR an explicit decision to leave near-asymptote hyperbola arcs as a sanctioned
      LOUD `LocalRefinementRequired` (out-of-scope), which is honest but caps cone
      coverage.** Shipped YR23 is sound (selection + near-vertex relocation, zero
      silent-wrong) and not a regression — it is progress that revealed a deeper
      gap.
    - **PR-YR24 — residual triage (likely small).** Remaining
      `LocalRefinementRequired` (axis-parallel / through-apex sections): confirm
      genuinely-degenerate ones correctly stay LOUD (out of scope, not a
      regression); close out cone. May fold into YR23.
    Each is a full RED→GREEN→Adversary cycle with the calibrated fuzz gate (cone
    `ok_correct` rises for the targeted section type; ZERO new silent-wrong;
    driver-verified delta). Cavity-sense / watertightness / on-surface oracle are
    surface-agnostic, so Subtract comes along per type.
  - **Next M5 increments (sequenced):** ~~curved `Subtract` cavity-sense~~
    (PR-YR13 ✅, box − cylinder blind pocket; ~~through-hole genus-1~~ PR-YR14 ✅;
    ~~box − sphere hemispherical dimple~~ PR-YR15 ✅; ~~box − cone conical pocket~~
    PR-YR17 ✅)
    + cut-surface handling (deferred in PR-YR8/YR5; through-cone / oblique cone cuts
    + internal spherical/conical voids still open)
    → side-face-exit / corner (triple-point) loud-STOP guard (oblique
    out-of-scope case) → broader SSI surface/pair coverage (cyl∩cyl) → the **general
    degree-4 curve** (a new parametric `SsiCurve` variant + the 5 general-position
    solvers) + torus pairs (rest of A15.4) → ~~curved `Surface` variants~~ (PR-YR6
    ✅) → ~~P2a curved cylinder tessellation~~ (PR-YR7 ✅) → ~~P2b sphere Stage-1
    tessellation~~ (PR-YR12 ✅) → ~~P3: Stage 3 wire
    `ssi-rs`~~ (PR-YR9 ✅). The general degree-4 cyl∩cyl curve requires a NEW
    parametric `SsiCurve` variant + general-position solvers, and **MUST be planned
    with a human before implementation.**
- **M6 — Native `cherchi-rs` Stage 2** behind the same interface, parity-green
  vs the sidecar on the corpus. ✅ **COMPLETE (2026-06-10, PR-CR-BL3c)** — all
  three BL3 slices landed; the sidecar is now a test-only parity oracle and
  yang-rs's default build runs the native backend (unconditional since M7c —
  the former `native-boolean` feature is removed; WASM restored at M7). **The biggest milestone — a faithful port of the
  MIT Cherchi C++ (`/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/`)
  to native Rust, removing the `cherchi-sidecar-rs` subprocess.** Reference parity
  vs the C++ sidecar is the LOAD-BEARING oracle (every PR diffs native vs sidecar
  on a corpus subset; CLAUDE.md hard-rule #2). MIT attribution header on every
  ported file. Used the `indirect-predicates-sidecar-rs` FFI for LPI/TPI
  until M7 (M6 landed native-but-not-yet-WASM; M7 clean-roomed the predicates →
  restored WASM at M7c). **Foundations already in `cherchi-rs`:** predicates (CR1–10),
  FastTrimesh+Tree (CR11–12c), Stage-1 pair detection (CR13), the CDT (NC1), the
  `MeshBoolean` trait + `LabeledArrangement` contract, and the IP FFI
  (`lambda3d_lpi/tpi`, `orient3d`). **Decomposition (PR-CR-AR* arrangement /
  PR-CR-BL* boolean-labeling; reference-parity-gated; demand-drive any missing IP
  predicate wrapper per CLAUDE.md #8):**
  - **PR-CR-AR1 — tri-tri intersection → implicit points. [DONE]** Ported
    `arrangements/code/intersection_classification.cpp`: for each CR13 candidate
    pair, classify (sign-pattern decoders cpp:834-925) and construct the typed
    intersection-vertex set per pair (`classify_pair` / `classify_all` in
    `crates/cherchi-rs/src/arrangements/intersection_points.rs`). **First FFI
    consumer inside `cherchi-rs`**, gated behind the off-by-default
    `indirect-predicates` feature (WASM still builds with the feature off; CI runs
    the crate both ways). **Scope (source-faithful, deviation N13):** builds
    **explicit + LPI only** — the source constructs *no* TPI here; TPI lives in
    `triangulation.cpp::createTPI`, deferred to **AR2**. AR1 ports the generic
    non-coplanar **transversal** crossing (`checkSingleNoCoplanarEdgeIntersection`
    → LPI via `ImplicitPoint3DLpi` + `lambda3d_lpi_*`; `checkVtxInTriangleIntersection`
    → explicit). Fully-coplanar and single-coplanar-edge pairs are emitted with a
    loud `Deferred(..)` marker (not dropped) for a later slice. Correctness oracle:
    each LPI vertex lies on BOTH supporting triangles' planes, asserted via exact
    indirect `orient3d == Zero` (not a float tolerance); plus CR9-agreement and
    hand-verified transversal cases. Full sidecar-corpus parity engages at AR3/BL3.
  - **PR-CR-AR2 — per-triangle constrained re-triangulation** (port
    `arrangements/code/triangulation.cpp`, ~1366 lines — **split into two
    slices**; NOT the spade NC1 CDT, which is f64-Delaunay and cannot handle
    exact/implicit points — port Cherchi's incremental insertion on implicit
    points via exact predicates + the CR12c `splitTri`/`splitEdge` API):
    - **PR-CR-AR2a — point/edge insertion. ✅ DONE.** Ported
      `triangulateSingleTriangle`'s point-collection (`aux_structure.rs`
      `group_intersection_points` → per-base-tri interior/edge buckets) +
      `splitSingleTriangle` (`retriangulate.rs` `split_single_triangle`): inserts
      AR1's intersection POINTS (interior → `split_tri`, on-edge → `split_edge`)
      into the per-triangle submesh with **exact** point-location via the FFI
      generic dispatch on `vert_coords`. Produces a valid covering
      sub-triangulation whose vertices include every intersection point (segments
      not yet enforced as edges). **Precursor CR-IP6b** added the implicit 2D
      predicates `orient2d_xy/yz/zx` + `point_in_triangle` to
      `indirect-predicates-sidecar-rs`; **Cycle 2** generalized `FastTrimesh`
      vertex storage to typed `VertexCoords { Explicit, Lpi }`. Oracle: exact
      covering triangulation (pure-dashu `RBig` signed-area-sum + same-sign
      winding, LPI coords from exact line-plane intersection — independent of the
      FFI split path), all intersection points are vertices, completeness/incidence
      via the exact FFI. Deviation **N14** (readable `splitSingleTriangle` with a
      uniform on-edge check; structural LPI dedup). **AR2b is next.**
    - **PR-CR-AR2b — constraint segments + TPI.** Decomposed A/B/C.
      - **Cycle A (done)** — FFI segment predicates (`inner_segments_cross` /
        `point_in_inner_segment` / `point_in_segment`).
      - **Cycle B (done)** — exact `point_in_segment_3d` (swaps the N13 raw-`f64`
        guard for the CR1 collinearity predicate) + `ConstraintSegment` grouping.
      - **Cycle C1 (done, PR-CR-AR2b Cycle C1)** — real `ImplicitPoint3DTpi`
        handle routing: `VertexCoords::Tpi` now flows through the
        per-base-triangle re-triangulation as an exact TPI handle (replacing the
        Cycle-B `sum/9` centroid placeholder), with macro-generated E/L/T
        predicate dispatch (`with_gp!`). Exact on-3-planes `orient3d==Zero`
        oracle. **The N13 TPI-handle deferral is RESOLVED at the routing layer.**
      - **Cycle C2 (remaining → AR3-coupled)** — `addConstraintSegment`
        enforcement + the segment-crossing `createTPI`. **STOP banked (P9/P10):**
        `createTPI`'s 2nd/3rd supporting-plane sourcing
        (`computeTriangleOfSegment` → global `seg2tris` + `jollyPoint` coplanar
        fallback) is AR3-level global state — the Cycle-B `source_tri` covers only
        an original transversal's witness, not mid-recursion sub-segments or the
        coplanar fallback. Deferred to Cycle C2 / AR3 rather than improvised.
  - **PR-CR-AR3 — constraint enforcement + global conforming soup** (absorbs the
    AR2b-deferred Cycle-C2 enforcement, which needs global cross-triangle state).
    **Parity-oracle correction (2026-06-08):** there is NO standalone C++
    arrangement binary (the 2020 arrangement code is embedded library-only — no
    main, no CMake target; only `mesh_booleans` (full boolean) is built). So AR3
    does NOT diff against a C++ arrangement. **AR3 oracle = structural + EXACT
    predicate invariants** (no self-intersections via exact `orient3d`; every
    detected intersecting pair realized as shared/constrained edges; consistent
    topology; Euler sanity) — strong for an exact arrangement. **Full C++
    reference parity engages at BL3** (the existing `mesh_booleans` binary
    transitively validates the arrangement: a wrong arrangement → wrong boolean →
    parity fail), honoring the parity-rule intent without speculative C++
    arrangement-dump infra. (Build such a sidecar later only if the structural
    oracle proves insufficient.) **Split:**
    - **PR-CR-AR3a — constraint-edge enforcement (DONE, 2026-06-08).** Ported
      `triangulation.cpp::addConstraintSegment` (cpp:597) + `createTPI` (cpp:1007)
      + helpers (`findIntersectingElements`, `boundaryWalker`, `earcutLinear`,
      `segmentsIntersectInside`, `pointInsideSegment`, `splitSegmentInSubSegments`)
      into `arrangements/enforce.rs`: realizes each AR2b `ConstraintSegment` as
      constraint-flagged mesh edge(s), constructing `createTPI` (real
      `ImplicitPoint3DTpi` from C1) at segment crossings. Public surface
      `SegmentSpec` / `EnforceError` / `enforce_constraint_segments` /
      `enforce_constraints`. The C++ global `seg2tris` is replaced by a
      per-work-item carried `source_tri` plus a `constraint_planes` side map keyed
      by sorted vertex-id pair (the minimal `TriangleSoup`). Oracle met
      (structural + EXACT, no C++ arrangement binary): constraints realized
      end-to-end; TPI exact on all 3 planes (`orient3d == Zero`); exact conforming
      sub-triangulation (pure-`dashu` covering); no spurious TPI (one crossing →
      one TPI, robust to endpoint/spec ordering — adversary-verified). The TPI
      handle/predicate dispatch (C1) was factored into a shared
      `arrangements/gp_dispatch.rs` (pure move) and reused. **Deferred to AR3b
      (the STOP walls, P9/P10):** `computeTriangleOfSegment`'s global `seg2tris`
      sourcing and the coplanar `jollyPoint` fallback — surfaced as the
      `EnforceError::SourcePlaneUnavailable` / `DegenerateTpi` errors (not hit by
      the in-scope original-transversal crossings; the multi-crossing case
      resolves its planes from the recorded sub-edge planes).
    - **PR-CR-AR3b — global conforming soup + topology (DONE, 2026-06-09).**
      `mesh_arrangement` (`arrangements/soup.rs`) wires the full
      detect→classify→group/canonicalize→split→enforce→assemble pipeline into a
      global non-self-intersecting soup: input scaling (`compute_multiplier`),
      global vertex dedup/weld (`merge_duplicated_vertices` +
      degenerate/duplicate-triangle removal), per-pair AR1 classification, global
      intersection-point grouping with N18 EXACT-coordinate canonicalization
      (coincident LPI/TPI points reached via different generator tuples weld to
      one identity across triangles), per-base-triangle fast-path-or-split+enforce,
      and a global weld of the emitted submeshes. Oracle met (structural + EXACT,
      RED 5-invariant + hand corpus; no C++ arrangement binary): conforming,
      jolly-tailed, label-aligned, no-degenerate, implicit-points-welded. An
      independently-authored ADVERSARY module pins input-ordering invariance
      (winding/order/label-swap), multi-crossing faces (conform or loud
      `DeepRecursionRequired`), the `SingleCoplanarEdge` loud-defer branch, N18
      anti-over-weld, and planar fast-path fidelity (resolved-position parity).
      **Still deferred (loud, P9/P10):** the AR3a `SourcePlaneUnavailable` /
      `DegenerateTpi` walls remain typed errors where unreached; coplanar
      overlap + single-coplanar-edge-through-interior are loud
      `CoplanarPairDeferred` (the §4.5.5 2D-Boolean pre-pass is **M8**). Feeds
      BL*. **(UPDATE 2026-06-20:** the single-coplanar-edge class is now
      CONSTRUCTED end-to-end — contained, edge-CROSSING, and the
      tvX-corner / through-interior sub-configs all classify with sidecar
      parity; only fully-coplanar `0 0 0` whole-triangle overlap still defers as
      `DeferReason::Coplanar`. See `single_coplanar_edge_parity.rs` and
      `soup.rs::adversary_coplanar_edge_through_interior_is_constructed`.**)
  - **PR-CR-BL1 — patch flood-fill (DONE, 2026-06-09).** Ported
    `computeAllPatches` / `computeSinglePatch` (booleans.cpp:396/426, serial
    variant) into the new feature-gated `labeling/patches.rs`:
    `compute_all_patches(&ArrangementSoup) -> Patches { patches, tri_to_patch,
    border_verts }` — ascending seed scan + stack flood across manifold edges
    (≤2 incident tris), stop at non-manifold intersection edges, border-vert
    marking for BL2 `findRayEndpoints`. Oracle (10 invariants): partition /
    label-constant / manifold-maximality / intersection-cuts / border-verts ==
    non-manifold endpoints / disjoint + enclosed + point-touch degeneracies /
    determinism / loud errors. Independently-authored ADVERSARY (12 tests):
    ordering/winding/concat invariance, 3-solid two-loop chain, through-cut
    (3 patches per solid), hand-built 3-incident edge, LabelMismatch path.
    Deviations: adjacency built from the soup (Rust FastTrimesh is
    per-base-triangle), serial-only (rule #5), sorted-Vec patches, returned
    border set, loud error for the C++ assert. **Scope note:** the `foctree`
    octree is NOT built here — it has no consumer until the BL2 ray-cast
    (demand-driven, CLAUDE.md #8 spirit); port it in BL2 alongside
    `findRayEndpoints`/`computeInsideOut`.
  - **PR-CR-BL2 — ray-cast in/out (2022 §5).** The robust per-patch in/out.
    - **Cycle A (DONE, 2026-06-10)** — `labeling/inside_out.rs`:
      `compute_inside_out(&soup, &patches) -> Vec<Label>` (per-patch inner
      labels). Full port of `findRayEndpoints` (explicit-origin branch) /
      `fast2DCheckIntersectionOnRay` / `checkIntersectionInsideTriangle3D` /
      `perturbX|Y|ZRay` + `perturbRayAndFindIntersTri` /
      `sortIntersectedTrisAlong*` (exact LPI sort keys via FFI
      `lessThanOnX/Y/Z`, btree-set equal-key-drop semantics) /
      `analyzeSortedIntersections`. Structural prerequisite: the soup now
      carries the prepped original `in_tris`/`in_labels` (C++ `arr_in_tris`).
      Oracle (5 invariants) + independent ADVERSARY (11 tests) — the
      adversary found 3 real bugs, all fixed: winner-less perturbation
      events now SKIP (N19, C++ `winner != -1` semantics; the fatal error
      was wrong on grazing input) and ray-parameter-ZERO hits are discarded
      (N20 — the C++ keeps them and silently mislabels point-touching
      solids; justified deviation, see docs/yang_deviations.md). Port
      finding: the C++ octree rayAABB query is semantically LOAD-BEARING
      (excludes behind-origin events); the brute-force port reproduces it
      with an explicit ray-AABB pre-filter.
    - **Cycle B (DONE, 2026-06-10)** — the C++ "generated ray" branch:
      synthetic origin at a patch triangle's approx centroid −0.1 along its
      dominant-normal axis (pure-f64 LPI/TPI approx eval; CR1/CR4 gates),
      EXACT validation (orient3d straddle + strict interior passage via
      gp_dispatch E/L/T), `seed_tri` recorded, and the sort's
      seed-plane-side discard (C++ `ray.tv` branch). Through-cut bands +
      hole discs now classify correctly (RED draft's expectation was
      itself corrected: a pierced solid's through-hole DISCS are inside
      the peg). ADVERSARY (9 more tests): X/Y-axis cuts, two pegs, peg
      through two stacked cubes, behind/forward seed-plane third solids,
      45° diamond peg, 0.01 sliver peg — no Cycle-B bugs.
    - **Cycle C (DONE, 2026-06-10)** — octree candidate producer:
      `labeling/octree.rs` `TriOctree` (build over `in_tris` AABBs;
      `query_aabb` = the booleans.cpp:580 `intersects_box` stack walk;
      deterministic params, sorted output). NOTE: the C++ pipeline uses
      `cinolib::Octree` — upstream `code/foctree.h` is NOT used by
      booleans.cpp and was not ported. Design invariant: the octree is a
      pure SUPERSET producer; the exact per-tri `in_ray_aabb` filter (the
      load-bearing behind-origin exclusion from Cycle A) applies to every
      candidate unconditionally, so octree parameters cannot affect
      labels. Oracles: superset vs brute (incl. degenerate zero-thickness
      ray AABBs), end-to-end label equivalence vs the permanent
      `#[cfg(test)]` brute path, determinism. Ring/edge searches stay
      full-scan (documented deviation; both complete). **BL2 COMPLETE.**
  - **PR-CR-AR3c — input-order-invariant constraint realization (DONE,
    2026-06-10).** Fixed at the diagnosed anchor: AR1/aux point identity
    was STRUCTURAL (generator-tuple) where the C++ interns by EXACT
    geometry (aux_structure.cpp:230) — one geometric point reached via two
    generator tuples counted twice, and `group_constraint_segments`
    silently dropped any pair resolving to ≠2 ids. New `PointInterner`
    keyed by exact rational coords (pure-dashu Lpi/Tpi evaluation) interns
    at SOURCE; >2 geometric endpoints is the loud
    `TransversalEndpointOvercount`; N18's post-hoc `canonicalize_points`
    folded into the interner (see amended N18 in yang_deviations.md).
    Oracles: anchor pair both orders, `segments_per_tri` keyed by triangle
    geometry invariant under reversal/concat-swap, end-to-end 16-fence +
    patch-count invariance; the BL2 RED witness un-ignored and green.
    Originally: **(OPENED 2026-06-10, blocked BL3 corpus parity).** The BL2-Cycle-B adversary
    found AR3b's constraint realization is input-order-DEPENDENT on
    CLOSED intersection loops: reversing global triangle order or
    swapping the two solids' concat order on a through-cut fixture
    leaves 4 intersection-loop fence segments unrealized as shared
    multiplicity-4 edges (two realized on only one side, two on
    neither), so the BL1 flood leaks and 6 patches collapse to 2. The
    AR3b conforming oracle (no interior AREA overlap) is structurally
    blind to a constraint segment missing from a perpendicular face's
    re-triangulation. RED witness: `#[ignore]`d
    `adversary_b_generated_ray_permutation_invariance`
    (labeling/inside_out_adversary_tests.rs) — un-ignore when fixed.
    Fix belongs in the `mesh_arrangement` orchestration (soup.rs):
    every intersection-curve segment must end as a shared edge of BOTH
    incident surfaces regardless of input presentation. New invariant
    for the AR3b oracle suite: per-pair constraint segments appear as
    multiplicity-4 edges, asserted under order/winding permutations.
  - **PR-CR-BL3 — emit `LabeledArrangement` + native `MeshBoolean` impl.**
    Assemble the per-tri source + patch_id + per-input in/out, implement
    `MeshBoolean` natively, **parity-green vs the sidecar on the corpus**, then
    switch `yang-rs` to the native backend behind the trait (the sidecar stays as
    the `#[cfg(test)]` parity oracle).
    - **PR-CR-BL3a ✅ DONE (2026-06-10).** `labeling/native.rs`:
      `native_labeled_arrangement` (concat → AR3b soup → BL1 patches → BL2
      in/out → frozen Stage-2 contract, exact-rational → descaled-f64 vertex
      emission) + `NativeBoolean` (`keep_set(op)` + per-op orientation flips
      at emission). Hand-fixture volumes exact.
    - **PR-CR-BL3b ✅ DONE (2026-06-10) — the M6 reference-parity gate.**
      `tests/parity_native_vs_sidecar.rs`: 12 generic-position fixtures × 4
      ops + concat-swap invariance (60 sidecar-compared cells), all GREEN
      under a triangulation-independent metric (exact vertex weld →
      watertight 2-manifold / Xor even-multiplicity, signed volume + area at
      1e-9 relative, Euler characteristic, vertex-set Hausdorff-0 at 1e-6).
      Native↔C++ agreement is ~1 ulp on volumes/areas across the corpus.
      Coplanar overlap stays a loud exclusion (N17 → M8); RED surfaced that
      an edge lying EXACTLY in the other solid's face plane triggers the
      same loud `SingleCoplanarEdge` deferral (N13 family) — promote a
      deliberate edge-in-plane fixture when that C++ path is ported.
      ~~Remaining BL3 work: switch `yang-rs` to the native backend behind the
      trait.~~ → BL3c below.
    - **PR-CR-BL3c ✅ DONE (2026-06-10) — yang-rs on the native backend; M6
      CLOSED.** yang-rs gained a default-ON `native-boolean` feature (enabled
      `cherchi-rs/indirect-predicates`; both features REMOVED at M7c — the
      backend is unconditional and WASM-clean now) and
      `yang_rs::native_backend() -> Option<NativeBoolean>` (None on the FFI
      stub build, P9 skip-loud). The whole yang-rs suite (334 tests) runs on
      the native backend unchanged — zero invariant re-anchors; the sidecar
      survives only as (a) the yang-level dual-backend parity suite
      (`tests/backend_parity.rs`), (b) the inputcheck Stage-1 oracle, and
      (c) the reference-mesh oracle inside the two #[ignore]d deep-fuzz
      harnesses. Coplanar M8 inventory: EMPTY — no yang-rs test ever passed
      coplanar input through the sidecar (yang's Stage-6 F2 gate already
      rejected it); on native the loud error simply fires earlier
      (`CoplanarPairDeferred` at the arrangement). Banked finding: with
      bit-exact mesh-level parity and identical yang-level vertex sets / χ /
      surfaces, curved-case output VOLUME is triangulation-dependent after
      Stage-4 relocation (~2e-4 on cylinder∪box) — yang-level parity uses
      the chord band d_ε × A_lateral for curved volume, 1e-9 relative for
      planar.
- **M7 — Clean-room indirect predicates from Attene's paper → restore WASM.**
  ✅ **COMPLETE (2026-06-10, PR-CR-M7a/M7b/M7c).** The LGPL FFI is demoted to a
  dev-only differential oracle; cherchi-rs / yang-rs / kernel-v2 all compile
  for wasm32-unknown-unknown with no feature flags.
  - **PR-CR-M7a DONE (2026-06-10): pattern-setter slice.** New
    `crates/predicate-gen` (dev tool, zero deps): SSA polynomial IR + FPG
    (Meyer-Pion 2008) forward error analysis → Attene App. A semi-static
    constants `δ(1)`/degree, emitting the checked-in
    `cherchi-rs/src/predicates/indirect/generated.rs` (byte-frozen by a
    freshness test; `cargo run -p predicate-gen` regenerates). cherchi-rs
    gains the UNGATED pure-Rust `predicates::indirect` module:
    `GenericPoint3D` (owned Explicit/Lpi/Tpi, §5.4 lambda caching) +
    `orient3d_indirect` over all 14 canonical instances (§6 reduction),
    cascading semi-static filter (§5.1) → interval arithmetic (§5.2,
    next_up/next_down outward rounding — no FPU modes, WASM-clean) →
    exact RBig (§5.3, `d == 0` → Undefined).
    `cargo check -p cherchi-rs --target wasm32-unknown-unknown` PASSES —
    first WASM proof-point. Oracles: filter soundness (95.6% generic
    implicit hit rate, gate ≥90%), ~357-case differential parity vs the
    black-box FFI sidecar (C++ orient3d is the MIRROR of Shewchuk's
    convention), δ cross-checks vs Cherchi's published εdL/εdT constants
    (ours 5.11e-15 / 8.89e-14 vs 4.885e-15 / 8.704e-14 — same degrees,
    few % more conservative). **Banked lesson:** the semi-static
    worst-case bound alone certifies only ~52% of generic TPI-heavy
    cases (degree up to 39) — the paper's interval tier is load-bearing,
    not an optimization; plan it into every remaining M7 predicate
    slice.
  - **PR-CR-M7b DONE (2026-06-10): full catalog slice.** predicate-gen
    grows a shared instance emitter (per-instance D′ parity slots: only
    ODD-multiplicity denominators flip; even ones feed undefinedness
    checks) + two families: `orient2d_{xy,yz,zx}` (27 instances, the
    exact Cherchi 2020 Appendix A set, canonical rank L < T < E pivoting
    on the FIRST argument; one-implicit LEE/TEE use the appendix's
    factored degree-5/8 form) and `less_than_on_{x,y,z}` (15 instances,
    Appendix B POINTCOMPARE; EE = direct f64 compare). All 13 published
    Appendix A/B filter constants matched: degrees EXACTLY, our δ 6-12%
    above (conservative band). cherchi-rs `predicates::indirect` adds
    the public catalog (`*_indirect` + `_filtered`/`_exact` tiers), the
    composites built purely on the primitives (`point_in_triangle`
    closed containment via first-non-degenerate projection;
    `inner_segments_cross` 4-orientation proper-crossing;
    `point_in_{inner_,}segment` collinearity gate + separating-axis
    betweenness — symmetric, deliberately fixing the FFI's documented EE
    order-sensitivity) and `approx_lpi` (interval-midpoint readback, the
    `lambda3d_lpi_interval` consumer's swap target). Oracles: hit rates
    orient2d 0.928/0.967/0.996, lessThan 1.000×3 (gate 0.90); composite
    parity vs independent pure-RBig formulations (~1000 coplanar
    configs); FFI differential parity incl. EE-quirk mappings; suites
    386 default / 549 gated; wasm32 check still green.
  - **PR-CR-M7c DONE (2026-06-10): consumer swap → M7 COMPLETE.** Every
    `arrangements/` + `labeling/` production call site swapped from the
    FFI to `predicates::indirect`: gp_dispatch's `Backing`/`Gp<'a>`/
    `with_gp!` 3^N handle machinery collapsed to a one-match
    `VertexCoords → GenericPoint3D` conversion (owned, lifetime-free,
    internal lambda caching preserves the per-vertex reuse pattern —
    inside_out's ray-sort arena is a `Vec<GenericPoint3D>`); enforce's
    fwd||rev EE-quirk workaround collapsed to ONE symmetric native
    `point_in_inner_segment_indirect` call; the two orient3d production
    sites (inside_out straddle / behind-seed-plane tests) are
    sign-RELATIVE, so the native↔FFI sign mirror needs no flip
    (annotated per-site); `lpi_approx` keeps its midpoint fallback on
    the native `approx_lpi`'s degenerate `None`. `init_fpu` removed from
    production (native predicates need no FPU mode). The
    `indirect-predicates` feature is REMOVED (modules always compiled);
    the FFI crate is a dev-dependency oracle only
    (`tests/indirect_*_parity.rs` keep `require_ffi_shim`, now in
    `tests/indirect_common`); yang-rs's `native-boolean` feature removed
    (`native_backend()` unconditional); test.sh's duplicate feature run
    dropped. Gates: cherchi-rs 557 (incl. the 60-cell
    native-vs-sidecar parity corpus — end-to-end proof the swap changed
    nothing), yang-rs 334 (incl. backend_parity), kernel-v2, rewrite
    tier, and `cargo check --target wasm32-unknown-unknown` green for
    cherchi-rs + yang-rs + kernel-v2.
- **M8 — Stage 0 coplanar preprocessing** hardened last (special case that
  complicates everything earlier). **Verified a genuine native need** (deviation
  N8, 2026-06-02): the patched sidecar emits multi-solid-labeled
  (`surface.len()==2`) triangles on coplanar overlap (test
  `c3_coplanar_face_yields_multi_attribution`), which surface in `yang-rs` as a
  loud `FaceResolutionFailed` (F2) — coplanarity is NOT delegated away, so M8 must
  implement the §4.5.5 2D-Boolean pre-pass (currently a correct loud-STOP
  deferral). **Also folded into M8: §4.5.4 illegal-self-intersection
  detection/removal** (deviation N6) — absent in the new crates; currently benign
  for analytic inputs (sidecar mesh validly trimmed + `check_watertight_2manifold`
  gate), to be added as a post-trim detector here.

  > **Reframe (2026-06-26): M8 is now the GENERAL §4.5.5 program, not a lattice
  > of special cases.** The slices below (a–e) built and shipped real machinery —
  > the exact 2D overlay engine (YR25), its wiring into `boolean()` (YR26),
  > Stage-6 hardening (YR27), the intra-solid plane-bit chained case (KV10), and
  > the flat-disc∩polygon containment (M8-disc). They are kept as the historical
  > record and the substrate to build on. But they are an **interim scaffold**:
  > each handles a specific face shape / normal config and walls the rest. Per the
  > §0.1 posture, the objective is now to implement §4.5.5 **once, generally**, and
  > retire the shape-specific gates. The general algorithm (paper §4.5.5, Fig. 16;
  > `refs/text/yang2025_hybrid_boolean.txt:716-760`):
  >
  > 1. **Detect every coplanar surface pair** between A and B (planar first; the
  >    paper's method extends to coplanar curved surfaces — Fig. 24b's 24 coplanar
  >    cylinders — via the same overlap-region machinery).
  > 2. **2D Boolean of the overlapping trims, BEFORE discretization.** Segment the
  >    coplanar region into three parts: A-only, B-only, and the overlap.
  > 3. **Replace the overlap with ONE shared trimmed surface**, meshed
  >    **identically for both models**; the overlap boundary becomes the
  >    intersection curve; all three parts share identical boundary samples. No
  >    same/opposite-normal branching — the result/keep rule is applied downstream
  >    from in/out classification, not by special-casing the normal at Stage 0.
  >
  > **Done generally, this subsumes** same-normal overlaps, holed/multi-loop faces,
  > a face in >1 pair (n-ary overlay — LIFTED for the pure-polygon class at
  > slice f, 2026-07-11), and curved coplanar pairs — the rest currently
  > walled. **Co-requisite: deviation N4** (§4.2.3 barycentric per-triangle
  > provenance). The current `tol_for` centroid-proximity attribution is the
  > structural reason coplanar overlaps are fragile (multi-attributed overlap
  > triangles can't be resolved by a single nearest plane); replacing it with the
  > arrangement's intrinsic provenance removes the tolerance-band machinery and the
  > "lift a gate → unmask a downstream tolerance bug" churn (the R0082 class). The
  > general Stage-0 and N4 should land together. **The 6-mode same-normal campaign
  > (`m8_samenormal_campaign.rs`) is the concrete first proving ground** for the
  > general path — its modes are the downstream fixes the general overlap exposes.
  - **M8 slice a ✅ (PR-YR25, 2026-06-10): the EXACT 2D overlay engine**
    (`yang_rs::coplanar_overlay`, standalone — NOT yet wired into
    `boolean()`). Two polygons-with-holes in one shared-plane frame → ONE
    conforming classified triangulation (AOnly/BOnly/Overlap) via exact
    rational (dashu `RBig`) edge arrangement (proper crossings, T-junctions,
    collinear partial overlaps; shared A/B edges dedup to one constraint) +
    exact vertical (trapezoidal) decomposition + exact ear-clip per cell;
    parity classification at cell centroids. Coverage post-conditions are
    exact and loud: `area(XOnly)+area(Overlap) == area(X)` in rationals,
    every input edge tiled gap-free by triangle edges, no zero-exact-area
    triangle; f64 rounding happens LAST and a rounding-collapsed sliver is a
    typed `RoundingCollapse` error, never silence. Derived queries: exact
    per-class areas + interface/region-boundary polylines (the future
    intersection curves). `cherchi_rs::cdt_polygon_with_holes` was evaluated
    and rejected (loops-only contract, no interior constraints, f64-only).
    10 oracle tests in `tests/yr25_coplanar_overlay.rs` (yang-rs 336→346).
  - **M8 slice b ✅ (PR-YR26, 2026-06-10): wired into `boolean()`** —
    coplanar-overlap booleans now produce correct MESH-level results through
    the native backend. The YR24 gate became the Stage-0 DETECTOR
    (`scan_near_coplanar`, ALL cross pairs); `stage0_preprocess`
    (`yang-rs/src/stage0.rs`) snaps both faces of each planar A×B pair onto
    ONE canonical plane (face A's — the §4.5.5 "trimmed common planar
    surface" where femto residuals are reconciled; cross-solid corner weld
    by exact in-plane coordinates), runs the PR-YR25 exact overlay, and
    re-tessellates Stage 1: face A gets AOnly+Overlap, face B BOnly+Overlap,
    Overlap triangles BIT-IDENTICAL in both meshes (wound per solid);
    boundary subdivision propagates into adjacent faces (Fig. 16 "identical
    sampling points"), re-triangulated by an exact strictly-positive
    apex-fan with an exact area-coverage certificate (a generic ear-clip can
    chord across the femto-crooked subdivided chain and re-create YR24-style
    sliver patches — found on the R0029 oblique corpus geometry). Downstream
    the duplicates weld into multi-label `{A,B}` triangles; cherchi-rs now
    restores removed duplicates into `in_tris`/`in_labels`
    (`DuplTriInfo` + the `addDuplicateTrisInfoInStructures` port,
    booleans.cpp:179-313/358-393/1530-1539) so each input stays a closed
    single-label shell for BL2. The C++ keep-rules keep the zero-volume
    overlap sheet for EVERY op (verified vs the sidecar); `boolean()`
    resolves it by the result-boundary rule (keep iff exactly one side of
    the plane is inside the result): opposite normals → keep only for
    Subtract; equal normals → keep only for Union/Intersect. Plane∩Plane
    intersection edges short-circuit to `Curve::LineSegment` (exact; SSI
    correctly refuses coincident planes — the §4.5.5 seam curve comes from
    the overlay). Oracles: stacked/near-stacked/partial/pocket fixtures with
    exact volumes + watertight + χ=2 + analytic sidecar-deviation pins
    (`tests/yr26_coplanar_boolean.rs`, 12), in/out restoration
    (`cherchi-rs/tests/coplanar_dupl_restore.rs`, 4), and **R0029 — the
    KV4-F1 corpus case — now unions successfully end to end** (yr24 test
    flipped to success). Unsupported residue keeps the typed wall:
    intra-solid near pairs (chained-output class), curved faces in a pair,
    faces in >1 pair, holed/non-continuous neighbor rings, overlay engine
    failures. yang-rs 346→358; cherchi-rs 559→563; kernel-v2 + rewrite tier
    + wasm32 checks green. §4.5.4 illegal-self-intersection detection (N6)
    remains the open M8 remainder, and B-Rep-level shared-face output
    topology (the slice-c question: today the kept sheet attributes to input
    A's face) is deferred to a future slice.
  - **M8 slice c ✅ (PR-YR27, 2026-06-10): Stage-6 face resolution hardened**
    — four probe-verified findings from the independent YR26 review.
    (1) *Finite-extent membership*: a multi-hit Stage-6 membership tier is
    narrowed by EXACT strict point-in-face containment
    (`point_strictly_in_planar_face`, rational 2D in the face's plane
    frame) before being declared a tie — kills the infinite-plane false
    positives (an L-profile cap CDT triangle whose centroid lies bit-exactly
    on a side plane; a chained input carrying same-plane sibling faces).
    Curved/undecidable faces are never excluded; unresolved ties stay the
    loud `FaceResolutionFailed` (now with precise Display text — the old
    "coplanar multi-solid label" wording was misleading). (2) *Keyed pair
    membership*: faces that went through a Stage-0 pair are measured against
    the CANONICAL pair plane (`PairPlane.face_a/face_b`), since the snap put
    their mesh there — fixes near-partial overlaps with residual in
    (TAU_WORK, band]; keyed per pair, no global tolerance change.
    (3) *Same-plane output-face merge* (`merge_same_plane_patches`):
    edge-adjacent patches on one plane (unit-normalized agreement within
    TAU_WORK, same orientation) emit as ONE output face (stacked union:
    10→6 faces), so chained booleans never see bit-identical-plane sibling
    faces; non-adjacent same-plane patches stay separate. (4) *Unmasked
    Stage-1 latent*: the fan path emitted a ZERO-AREA glue triangle for a
    convex outer loop with a COLLINEAR boundary run (re-fed outputs carry
    such subdivided edges); the next arrangement drops it → T-junction →
    non-watertight kept set. Collinear-run loops now route to the CDT
    (`planar_outer_loop_fan_unsafe`); the yr5c chained-subtract adversary
    passes its REAL branch (two 2-holed faces, vol 0.92). Assay: **ERROR
    1→0** (F0066 ERROR → honest typed UNSUPPORTED(coplanar-boolean) — its
    residue is the intra-solid chained wall), F0008 WRONG→CORRECT
    (SUPPORTED_CORRECT 5→6). Oracles: `tests/yr27_face_resolution.rs` (7).
    yang-rs 358→365; cherchi-rs 563; kernel-v2 55 (+ restored kv3
    coplanar-touching Intersect cell); rewrite tier + wasm32 + clippy/fmt
    green. Still open in M8: §4.5.4 illegal-self-intersection detection
    (N6) and the intra-solid near-coplanar chained-output class
    (`CoplanarFacesUnsupported`, F0066's residue).
  - **M8 slice d ✅ (PR-KV10, 2026-06-12): the intra-solid chained-output
    class RESOLVED for planar chains** — two stacked rounding-identity
    fixes, found by a probe-instrumented survey of ALL 54 coplanar-walled
    corpus cases (per-case residue distribution: 20 intra-solid /
    19 curved-face-in-pair / 8 arrangement-deferred / 4 build-mesh /
    3 overlay-RoundingCollapse).
    (1) *Sibling plane-bit canonicalization* (`to_yang_brep`): disjoint
    fragments of ONE plane (a box side split in two by a crossing union)
    emitted femto-distinct `(normal, d)` bits — per-fragment Newell
    normals + per-face first-vertex `d` derivation — and
    `scan_near_coplanar`'s benign intra exclusion is BIT-identity, so a
    fragment-carrying output could not enter ANY further boolean even
    when the incoming solid shared no plane (F0016-class, the dominant
    sub-class). Planar faces whose unit normals agree within `TAU_WORK`
    and offsets within `TAU_WORK·(1+|d|)` now adopt the first face's
    exact bits (greedy, deterministic; legitimate parallel planes are
    ≥ MIN_FEATURE_SIZE apart — six orders beyond the band).
    (2) *Near-aware I6 weld for ALL-PLANAR input pairs* (`boolean()`):
    behind the wall sat a second latent — chained oblique inputs make
    adjacent same-face tessellation triangles span femto-different EXACT
    planes, so the arrangement legitimately mints distinct copies of one
    junction point (~1e-16·scale apart, one per generating tri pair);
    left distinct they chain into sliver fans in the output B-Rep and
    break the NEXT boolean's patch boundaries (`NonManifoldOutput`
    dead-end / duplicate-CDT-vertex). The weld now clusters vertices
    within the scale-relative band (grid-bucketed, exact per-pair check —
    quantization only nominates, the KV8c lesson) and welds to the lowest
    member index. CURVED inputs keep the bit-exact weld: Stage-4 owns
    junction duplicate collapse there, and the near-weld collapsed
    cyl×cyl lens-tip seam edges (caught RED by kv9_cyl_cyl_special on the
    first attempt — the planar-only scope is load-bearing).
    Oracles: `kernel-v2/tests/kv10_plane_canonicalization.rs` (2: the
    bit-identity invariant + an oblique F0016-class chain with exact
    volume); m3 a4 adversary updated (the near-coincident gap it
    documented is now CLOSED for planar inputs — the perturbed duplicate
    trips the same NonManifoldInput guard as the bit-exact one).
    Corpus: SUPPORTED_CORRECT 42→46 (F0017/F0020/F0023/F0024),
    UNSUPPORTED(coplanar) 54→43, SUPPORTED_WRONG 0→0; ERROR 40→47 — six
    of the seven new ERRORs are ONE named pre-existing bug whose corpus
    footprint the wall had been masking: **KV4-F1 `NoExplicitRayOrigin`**
    (cherchi ray-cast: no explicit non-border patch vertex and no
    generated-ray triangle passes the exact straddle test — the point
    where the C++ reference exits "requires rationals"); the seventh is a
    gear-replay timeout (performance class). KV4-F1 is now the top
    M8-adjacent lever (~6 cases) and needs its own cycle (likely a
    rational-ray or implicit-origin extension of `find_ray_endpoints`).
    Still open in M8: §4.5.4 (N6), curved faces in a pair (19 cases,
    needs an arc-capable overlay), faces in >1 pair (n-ary overlay),
    arrangement-deferred exact-coplanar tri pairs (8 cases),
    build-mesh/overlay robustness (7 cases). Stage-0's survey probes are
    kept env-gated (`YANG_COPLANAR_PROBE=1`) for future residue sizing.
  - **M8 slice e ✅ (PR-M8-disc, 2026-06-15): the flat-disc∩convex-polygon
    containment class** — the dominant `face-unsupported` sub-class. A
    probe-instrumented survey (`YANG_COPLANAR_PROBE=1`, both-face curve
    histograms) showed EVERY `face-unsupported` coplanar-wall hit is the same
    shape: a flat circular DISC (a planar face bounded by a single closed
    `Curve::Circle` — a cylinder end-cap) coplanar with a planar POLYGON face
    (a 4-sided box face or a high-segment tessellated profile); no disc∩disc,
    no ellipse, no partial arc in that class. The disc is now built DIRECTLY
    (NOT through the sweep overlay, which re-subdivides the disc rim at every
    sweep-line crossing and would break conformality with the cylinder lateral
    that shares the rim): (1) `scan_near_coplanar`'s per-face AABB now expands
    by a `Circle` edge's analytic box `center ± r·√(1−n_k²)` — previously a
    disc face's AABB was just its seam vertex (a single point), so a coplanar
    disc∩polygon pair was detected only when the seam happened to overlap the
    other face (the `polygon ⊆ disc` orientation was missed entirely);
    (2) `build_disc_pair` (`stage0.rs`) extracts the disc's exact Stage-1 rim
    ring from Stage 1's OWN output (bit-identical → conformal with the
    cap/lateral mesh), tests strict containment (disc ⊆ poly or poly ⊆ disc,
    exact), and emits a shared rim/boundary triangulation as the overlap
    (rim fan for a disc inner, ear-clip for a polygon inner) plus an
    angular-merge annulus (the region between two nested convex rings,
    star-shaped about the inner centroid — no keyhole, no Steiner points, so
    every boundary vertex stays bit-shared) on the larger face, wound
    frame-CCW and distributed per the existing (op, normal-agreement) rule.
    Scope: pure containment with a CONVEX polygon. The loud residue keeps:
    disc∩disc (the newly-unmasked dominant remainder — coaxial cap pairs,
    86 probe hits), disc×polygon CROSSING (circle×segment is irrational on
    the sampled ring + needs boundary-split propagation, 13 hits),
    non-convex / holed polygons (gears). Corpus: **SUPPORTED_CORRECT 58→62**
    (F0026, F0030, F0062 + R0067 — the prior KV4-F1c render-tess ERROR, now
    chains through), SUPPORTED_WRONG 0→0, coplanar-walled 41→39; several
    ERROR↔coplanar lateral moves (R0079/R0092 ERROR→loud-typed coplanar wall,
    R0023/R0089 coplanar→a distinct downstream ERROR), zero CORRECT→worse
    regressions. Oracle: `crates/yang-rs/tests/m8_disc_coplanar.rs` (both
    containment orientations end-to-end watertight + volume; the crossing
    case pinned to stay `CoplanarFacesUnsupported`). Next M8 disc lever:
    **disc∩disc containment** (coaxial caps — the same direct construction
    with two rims instead of a rim + polygon), then disc×polygon crossing.
  - **M8 slice f ✅ (task #129, 2026-07-11): the plane-grouped n-ary
    overlay — "face in >1 pair" LIFTED for the pure-polygon class** (spec
    `specs/m8_plane_group_nary_overlay.md`). Driver: user case
    `error_coplanar.waffle` — a bridge slab whose bottom face is flush with
    BOTH tower tops of a U-shaped solid (2 pairs sharing one B face + 4
    zero-overlap corner-flush side pairs). Design: cross pairs are grouped
    into PLANE GROUPS (connected components over shared faces,
    `stage0::nary::build_plane_groups`); a singleton group runs the
    historical 1×1 path byte-identically, a multi-pair group runs ONE
    n-ary exact overlay — `coplanar_overlay_multi`, side = a SET of
    interior-disjoint polygons, parity over the side's combined edge set,
    per-triangle containing-polygon attribution (`poly_a`/`poly_b`), and
    the coverage identity enforced PER POLYGON (overlapping same-side
    inputs = loud `CoverageMismatch`). Snap/cross-weld/§2b clustering run
    per GROUP on one canonical plane (lowest A face); per-face overrides
    are attribution-scoped; `collect_edge_splits` runs per group face.
    Scope walls (typed residue): a multi-pair group carrying a
    disc/annular/mixed face (`nary-face-unsupported`) or mixed per-side
    orientation (`nary-mixed-orientation`). Oracles:
    `yang-rs/tests/m8_bridge_nary_overlay.rs` (4 e2e: union χ=0 frame with
    exact volume for both the round-number inset fixture AND the user's
    exact mm geometry, subtract-leaves-A, intersect-empty),
    yr25 n-ary engine tests (exact per-class + per-polygon attributed
    areas, 1×1 delegation bit-identity, loud overlapping-inputs), and a
    stage0-level attribution oracle (`nary_overrides_are_disjoint_and_owned`
    — added after a FIP §6.3 mutation check proved the mesh-level e2e
    oracles are INSENSITIVE to a dropped/swapped attribution filter:
    downstream duplicate welding + same-plane merge mask it). Corpus
    (full assay 2026-07-11, release JOBS=6, 240s cap): **235 CORRECT /
    0 WRONG / 46 ERROR / 13 UNSUPPORTED / 1 EXPECTED_ERROR / 0 TIMEOUT**
    (295 cases) — NEW case **C0101** (user-exact geometry, exact chain
    volume, χ=0) SUPPORTED_CORRECT; **F0073** UNSUPPORTED→SUPPORTED_CORRECT
    (its wedge auto-union sat on this wall, mislabeled UNSUPPORTED(revolve)
    by the failing-feature name); **R0081** lateral wall-to-wall move —
    same mislabeled coplanar wall lifted, the case now marches to the
    pre-existing Stage-4 `LocalRefinementRequired` class (N2 epic). Zero
    CORRECT→worse. Residue finding: the ROUND-NUMBER corner-flush variant
    of the TOWER unions (tower sharing three side planes with the base at
    x=−end) dies earlier at the pre-existing chiral `edge-not-2-directed`
    output wall — the user's irregular coordinates do not; C0101 pins the
    user's actual boundary.
  - **M8 residue census 2026-07-11 (task #130): the 7-case
    UNSUPPORTED(coplanar) tail = 4 named mechanisms** (probe re-run,
    `YANG_COPLANAR_PROBE` per case): (1) R0007+R0071
    `overlay-failed DegenerateLoop("zero-length edge")` — the KV15b
    profile-congenital micro-twin class (floor welds ruled out; needs a
    scale/floor POLICY decision, not a local fix); (2) F0067+C0048
    `overlay-failed RoundingCollapse` — the femto-slab class whose local
    emission surgery is P10-REFUTED (`m8_overlay_femto_slab_emission` §8;
    needs per-region re-emission / constrained snap-rounding grade);
    (3) R0025+R0050 `rim-lateral-none` — their coplanar disc caps' rims
    are TORUS profile circles (revolved-circle profiles;
    `YANG_RIMLAT_PROBE` shows the incident lateral is `Surface::Torus` in
    both), and `lateral_for_cap` is cylinder-only → **task #131** (torus
    admission + poloidal opposite-rim projection + profile-chain grid
    conformality in `tessellate_torus_face`/`_band`; downstream is
    KV6d-torus-gated, expect a deeper typed wall); (4) R0046
    `nary-face-unsupported` — a disc/mixed face inside a multi-pair plane
    group (slice f's pure-polygon scope gate) → **task #132** (slice g:
    n-ary admission for disc/annular/mixed faces).
  - **M8 slice g SHIPPED 2026-07-11 (task #132, spec
    `specs/m8_nary_tessellated_faces.md`): disc/annular/mixed faces in
    n-ary plane groups.** `overlay_nary_group` runs the 1×1 tessellated
    machinery per group face: `face_polygon_2d_tessellated` polygons +
    merged corner/rim key maps, rim-aware §2c clustering, per-face mint
    ctxs (`rim_chord_ctxs`/`mixed_chord_ctxs` generalized to take ALL
    other-side polygons — 1×1 call sites pass `slice::from_ref`,
    byte-identical), the shared-mint collapse, a REDUCED fold gate
    (amendment-4 flips constrained to same class AND same `poly_a`/`poly_b`
    attribution — a face-boundary edge is as immovable as a class boundary
    — plus amendment-2 revert; NO cavity relocation yet), and per-face
    crossing collectors threading NEW `rim_overrides` params. The 1×1
    `annular-hole-rim-crossing` wall ports pairwise
    (`annular_hole_rim_crossing` factored into stage0/frame.rs); crossing
    full-circle rims are NOT walled (the 1×1 path already resolves them —
    `disc_disc_crossing_union_succeeds`). Oracles:
    `yang-rs/tests/m8_nary_tessellated_overlay.rs` ({mixed,mixed}×{disc}
    flush-pocket subtract/union with a partition cross-oracle
    (removed+added = tool volume within rim-sag band), disc×disc group
    with one crossing + one contained pair, 1×1 canaries) + stage0 unit
    `nary_tessellated_group_stage0_meshes` (watertight emission, I3
    bit-identical overlap triangles). R0046 now clears admission and lands
    on `rim-lateral-none` — its disc's lateral is the revolve TORUS, i.e.
    it JOINS task #131's class (R0025/R0050/R0046). Two pre-existing gaps
    found by fixture probing (out of scope, tasks #133/#134): partial-depth
    pocket floor arc-chain non-conformality (plain-path NonManifoldOutput),
    and chained disjoint-lump unions losing `Curve::Circle` rim vocabulary
    (Stage-3 producer fault).
  - **M8 torus-profile rim crossings SHIPPED 2026-07-11 (task #131,
    b0f0c8e4, spec `specs/m8_torus_profile_rim_crossing.md`):** the
    `rim-lateral-none` mechanism is retired. `lateral_for_cap` → typed
    `CapLateral` (cylinder arm byte-identical + torus arm);
    `collect_ring_crossings` projects a torus-cap rim crossing onto the
    OPPOSITE profile circle at the same intrinsic poloidal angle
    φ = atan2(τ, ρ−R); `tessellate_torus_face` keeps the uniform slot grid
    as the first arm (bit-identical) and adds a φ-value column path for
    rings with inserted crossing samples (index-wise pairing of sorted
    seam-anchored offsets, fixed 1e-9 band — a min-gap tolerance collapses
    under femto-close crossing twins). Corpus: R0046
    UNSUPPORTED→SUPPORTED_CORRECT, R0025/R0050 → the Stage-4
    LocalRefinementRequired class (N2 epic), R0085's UNSUPPORTED(revolve)
    verdict was hiding this wall → the pre-existing CDT wall. Assay
    **237 CORRECT / 0 WRONG / 49 ERROR / 9 UNSUPPORTED / 0 TIMEOUT**.
    The UNSUPPORTED(coplanar) tail is now 4 cases / 2 mechanisms, both
    design increments: KV15b DegenerateLoop micro twins (R0007/R0071,
    scale/floor policy) + femto-slab RoundingCollapse (F0067/C0048,
    per-region re-emission).
  - **M8 fused-emission collapse SHIPPED 2026-07-12 (task #142, spec
    `specs/m8_overlay_fused_emission_collapse.md`): the femto-slab
    `RoundingCollapse` wall is retired.** The step-6 rounding gate now
    repairs sub-f64-resolution degenerate complexes by constrained edge
    collapse ([#51] Hoppe fold condition in EXACT arithmetic, [#52] Hobby
    snap-rounding family; KV15b/A14.2 precedent): trigger = exact f64
    degeneracy of the rounded image; eligibility ceiling = exact edge
    length² < TAU_MODEL² (supra-TAU collinear slivers stay the honest loud
    wall — pinned at ×1e9 scale); survivor preference = input-loop vertex
    over mint, else min index, keeping own exact bits; validity gate =
    every remapped triangle keeps exact area > 0; fusion published as
    `ClassifiedOverlay::fused` (loser→survivor, fully resolved). The
    no-sliver path is byte-identical (FNV-golden-pinned needle fixture).
    Full FIP cycle: Test Author red (C0048 verbatim pair + synthetic
    ULP-split parallelograms) → Implementer green → Adversary mutation
    matrix (survivor-inversion caught by a new input-loop-survivor pin;
    ceiling-widen caught by the supra-TAU pin; validity-gate B7 pinned by
    an internal hand-built-soup unit). The yr25
    `rounding_stress_sliver_collapse_is_loud` pin was superseded by the
    spec and updated to `rounding_stress_subresolution_sliver_fuses`.
    Corpus: F0067 + C0048 + **R0053 (bonus — the refuted trio's third
    member)** leave UNSUPPORTED(coplanar) for their next honest typed
    walls — C0048: kernel-v2 rim-crossing override coincides with uniform
    sample k=12 (silent merge refused → **task #143**, intentional
    shared-sample merge); F0067: 2× Newell-normal disagreement + azimuth-
    merge rims mismatched samples (572 vs 571 — the named exact
    opposite-rim projection follow-up); R0053: Stage-2 patch flood-fill
    LabelMismatch. Assay **238 CORRECT / 0 WRONG / 53 ERROR /
    4 UNSUPPORTED / 0 TIMEOUT**, zero-lost (exactly the 3 movers).
    Remaining UNSUPPORTED: R0007/R0071 (profile-congenital micro-twin
    scale/floor POLICY class), R0015 (coplanar — mechanism to re-census;
    it was not among the 2026-07-11 four-case tail), + C0063
    (curved-profile, by design).
  - **Rim-override uniform-sample intentional merge SHIPPED 2026-07-12
    (task #143, spec `specs/m8_rim_override_uniform_merge.md`): the
    "silent merge refused" wall is retired for fused twins.** Probe
    measurement (C0048): the refused override is the task-#142 fused
    survivor sitting **3 ULPs** (7.4e-16) off the rim's own uniform
    sample k=12/14 — the ULP-split mirrored-rim twin that MUST be the
    one shared §4.5.5 point. Both `stage1_tessellate` override sites
    (full rim + arc chain) now MERGE a coinciding override when it is a
    sub-TAU_MODEL twin of the uniform sample: the slot keeps its uniform
    angular key and takes the override's exact bits — ring length
    unchanged, so the uniform `(N−k)` lateral pairing holds and the rim
    is NOT routed to azimuth-merge; a bit-exact merge is byte-identical.
    Fail-closed walls kept loud: real-scale (≥ TAU_MODEL) grazes,
    seam-vertex / arc-endpoint bit mismatches (B-Rep vertices are
    authoritative; bit-exact copies dedup), and two distinct overrides
    claiming one slot. FIP cycle: 6 red tests → merge implementation →
    adversary (arc-site walls + merge-plus-insert interplay; 6-mutation
    matrix, every mutation killed by a named test). Corpus: C0048 leaves
    this wall for its next honest typed ERROR — "azimuth-merge rims have
    mismatched samples (66 vs 69)" — JOINING F0067 in the named exact
    opposite-rim projection follow-up class. SESSION TRAP (repeat of the
    KV15b incident): a `git checkout <file>` used to revert a mutation
    probe wiped the uncommitted implementation — mutations are now
    applied/reverted via scratchpad `cp` backups only.
  - **Task #144 P10 REFUTATION (2026-07-12, 6ab839e3, spec
    `m8_exact_opposite_rim_projection.md`): the "exact opposite-rim
    projection" follow-up (C0048 66v69 / F0067 572v571) is measured to its
    root and the planned fix is REFUTED.** Mechanism: same-ray radial twin
    pairs (a #142 chord-depth fused survivor + its on-circle twin at
    bit-identical exact azimuth) collapse to ONE image in
    `collect_ring_crossings`' radial renormalisation; merge counts are
    SYMMETRIC (`[ring-build]` probe, 3=3) — precision cannot help (the exact
    on-circle images coincide). The exact-translation arm fixed counts but
    mirrored chord-deep samples off-surface onto rims with no own crossings
    (unrelocated scaffolding) — caught by `n2_rim_mint_adversary`
    (SILENT-WRONG), reverted per P10. The real fix is snap-rounding grade
    (deterministic tangential separation, on-circle within the stage1 band,
    merge-mirroring, exact-order-consistent, cross-cap bit-absorbing) — five
    constraints banked in the spec. Probes + unit pins banked; the
    azimuth-merge count wall stays the honest LOUD verdict.
  - **Task #145 SHIPPED 2026-07-12 (spec
    `specs/yang_453_mixed_cycle_conic_backtrack.md`): the re-entry CDT
    zigzag class is retired — R0061 ERROR → SUPPORTED_CORRECT; R0063 /
    R0095 / F0085 leave the CDT wall for distinct deeper typed walls.**
    Two stacked mechanisms, each measured RED before its fix:
    (1) **§4.5.3 shared-conic sites in MIXED cycles were structurally
    unsweepable** (§3c scoped conic sites out after the angle-band P10
    disproofs). New arm: exact conic PARAMETER-ORDER reversal — deltas
    `d1 = t_r−t_b, d2 = t_n−t_r` wrapped to (−π, π] on the shared
    Circle/Ellipse (identity up to normal SIGN, exact bit-negation);
    reversal ⟺ d1·d2 < 0; victim = p_r onto its parameter-NEARER
    neighbor (the actual overshoot — `reversal_collapse_direction`'s
    junction rule picks the FAR junction here and the 2·d_ε gate rightly
    refuses whole arcs); angle band never consulted (the corner_in_band
    7-gon turns 51° < 90° and stays healthy; a legit steep sinusoid peak
    on a near-tangent ellipse turns ~160° in 3D but progresses in
    parameter — the I2 adversary pin that kills a turn-angle mutant).
    (2) **Azimuth-slide relocation on near-tangent sections**
    (`project_onto_ellipse_via_cylinder` preserves cylinder azimuth; the
    axial solve amplifies by 1/(n·â) — R0061's gear-graze facets at
    |n·â| = 0.0084 slid corridor vertices 3.4e-3…1.03e-1 ALONG their
    ellipses while every surface residual passed its band). Fix: when
    the azimuth move exceeds the per-site gate, relocate to the IN-PLANE
    nearest point ([#1] point-to-curve Newton from the atan2 seed),
    accepted against the derived corridor budget `2·gate/sin θ` at the
    relocated point (θ = cylinder-radial vs plane-normal angle — the
    KV9/KV11 gradient-band pattern; a FLAT move gate is the wrong metric:
    the kv11 box∪cylinder pin measures a LEGIT 2.7e-2 move at
    sin θ ≈ 0.55 against a 1.8e-2 band). Cyl×cyl arm byte-identical
    (its gate already carries per-point amplification). The repair is
    R2's REPLACEMENT, not rejection: corridor vertices land beside their
    ~1e-4-distant junctions and the macro spikes vanish at source;
    residual sub-band overshoots are exactly what the new sweep arm
    collapses — both mechanisms required. The nearest-point solver is
    BISECTION on the first-quadrant bracket (`f(0) ≥ 0 ≥ f(π/2)`
    unconditionally), NOT Newton from the atan2 seed — the first full
    assay caught F0047 CORRECT→ERROR (vertex 42: Newton diverged to a
    stationary point 2.6 away on an eccentric ellipse; bisection fixed
    it and F0047 returned CORRECT). Probes banked:
    `YANG_T145_SWEEP_PROBE` (site/arm/gate), `YANG_T145_RELOC_PROBE`
    (azimuth vs nearest move). Corpus movers: **R0061 + R0059 + R0072
    ERROR → CORRECT** (R0059/R0072 were the ring-reject census members
    whose FaceId walls were this same producer defect; R0072's
    `red_r0072_stage3_ambiguous_parallel_lines` tracker un-ignored,
    `wall_is_lifted_for_same_normal` repointed → R0063);
    R0063 → Newell-normal disagreement (JOINS the F0064/R0051/F0067
    class, task #146); R0095 / F0085 → NonManifoldOutput reassembly.
    Assay: **241 CORRECT / 0 WRONG / 50 ERROR / 4 UNSUPPORTED /
    0 TIMEOUT** — zero-lost (exactly the 3 movers up).
  - **M8 slice h SHIPPED 2026-07-14 (task #147, spec
    `specs/m8_mixed_orientation_nary.md`, deviation N44): mixed-orientation
    side-A faces in an n-ary plane group are admitted.** Driver R0015: a
    coplanar plane group whose side-A faces carry BOTH orientations vs the
    frame (`A-dots=[(0,+1),(1,+1),(7,−1),(8,−1)]`) — a VALID non-convex
    solid, since opposite-normal coplanar faces must be 2D-disjoint on a
    manifold (probe-verified: `+n̂ {0,1}` has zero self-overlap with
    `−n̂ {7,8}`; the disc B0 spans both). The exact overlay classifies
    coverage winding-independently, so the ONLY fix is per-A-face override
    winding `face_swap_a = face_dot < 0` (a `−n̂` face swaps like an opposing
    B face) — strictly byte-identical for uniform `+n̂` groups (every
    currently-supported group). FULL release corpus (295 cases,
    budget-TIMEOUTs resolved) = **byte-stable except R0015**:
    **239C / 0W / 52E / 3U / 1EE** vs the pre-N44 baseline 239C/0W/51E/4U/1EE
    — the SINGLE net change is R0015 UNSUPPORTED→ERROR (CORRECT unchanged,
    WRONG still 0). R0015
    `UNSUPPORTED(coplanar-boolean)` → `ERROR` (advances past Stage-0 to the
    PRE-EXISTING `Stage-4 OffCurveBeyondChordBand` N2/LRR gap, R0003 class).
    **UNSUPPORTED(coplanar) tail 3→2** (R0007, R0071 DegenerateLoop micro-
    twins remain; + C0063 curved-profile by design). Oracle:
    `nary_mixed_orientation_group_stage0_watertight` (offset flush-stack,
    mutation-killable via the reverted swap).
  - **M8 DegenerateLoop duo RETIRED 2026-07-22 (task #130, spec
    `specs/m8_profile_subresolution_point_removal.md`):
    UNSUPPORTED(coplanar-boolean) tail 2→0 — the bucket is EMPTY.** Three
    absolute-vs-scale-relative defects, one increment each: (1)
    kernel-v2 `Profile::new` now performs Yang §4.3 sub-resolution point
    removal at the ingestion gate — consecutive loop points at POSITIVE
    separation < TAU_MODEL·(1+scale) collapse to the min-index survivor
    (open interval: exact repeats stay the loud `ProfileRepeatedVertex`);
    retires the profile-congenital micro-twin class (R0007: 96 pairs @
    7.790e-8; R0071: 36 @ 9.460e-8; legit floors 2.337e-7 / 1.089e-7).
    (2) Harness `check_outward_normals`/`check_consistent_normals`
    degenerate-triangle filter was ABSOLUTE (`area_sq < 1e-20`) — R0007's
    healthy 280-tri final mesh (max area_sq 8.05e-21) read as "no valid
    triangles" SUPPORTED_WRONG; now sine-based per-triangle
    (|cross| < 1e-6·e_max², scale-free, above f32 vertex noise). (3) The
    §2b/§2c in-frame clustering band reused the coplanar DETECTION band
    (TAU_MODEL floor) though it reconciles frame-projection rounding
    (O(scale·ε), measured 1e-19..1e-21); at micro scale it welded LEGIT
    1.089e-7 features per-axis when diagonal (√2·band effective radius) →
    bit-identical consecutive verts → DegenerateLoop; now clamped to
    min(band, 1e-9·scale) inside `cluster_frame_coords_rim_aware`.
    **NEW PRODUCTION BASELINE 255C / 0W / 54E / 1T on 312** (was
    252C/0W/55E/2U/1T): R0007+R0071 UNSUPPORTED→CORRECT, **F0069
    ERROR→CORRECT bonus** (#153's 3e-8@2m off-plane emission wall was
    manufactured by the un-clamped clustering band's legal 1e-7 coord
    moves — half of #153 resolved; F0072 remains, budget-TIMEOUT), R0081
    detail-only shift. Parity + rewrite + fast tiers green.
    **Same-day follow-on: the F0082 Extrude-12 "M8 coplanar-residue"
    layer (#188 spec §10.10 defect 1) is REFUTED as a Stage-0 class** —
    new banked probes `YANG_SCAN_NEARMISS_PROBE` + `YANG_OPFACE_DUMP`
    measured NO near-coplanar A×B pair anywhere in the F0082 chain (all
    55 near-misses ≥400× band, misaligned normals; Extrude 12's 8-plane
    tool is a third sketch-plane orientation and no tool plane contains
    the defect-cluster verts). The live Extrude-12 STOP = §10.10
    defect 2 exactly (edge-connected sub-TAU_WORK twin v971/v972 @
    5.5e-14 + shared-sketch-vert re-mints) — the WHOLE residual routes
    to task #194 (arrangement-level, spec-first). **Task #130 CLOSED:
    the census's four mechanisms are all retired and the
    UNSUPPORTED(coplanar-boolean) tail is empty.** Remaining M8-family
    open items are the deeper typed walls of the ex-RoundingCollapse
    trio (#144 opposite-rim snap-rounding for F0067/C0048) and #178's
    cylinder/tilt analogs (need corpus cases first).
  - **#194 sub-TAU_WORK edge collapse SHIPPED always-on 2026-07-22
    (spec `specs/yang_194_subtauwork_edge_collapse.md`):**
    `collapse_subtauwork_mesh_edges` — all mesh edges with resolved
    length in `(0, TAU_WORK·(1+scale))` collapse to the min-index
    survivor (KV15b sweep rules; no provenance restriction — the
    five-orders-tighter band does the scoping; KV9's unconnected ring
    duplicates carry no joining edge and cannot be touched). Two sites:
    `stage4_relocate_and_correct` immediately before the (4b)
    watertightness gate (the driver's STOP site) + the stage5 emission
    block (all-planar path). Measured on F0082 Extrude 12: fires
    exactly on the §10.10 twin (972→971 @5.487e-14); the twin edge +
    zero-area flap leave the double-cover set. **F0082's residual layer
    RE-CHARACTERIZED: the χ=3 imbalance persists off SIX REAL-length
    double-cover edges ((930,931)@1.5e-3, (930,934)@8.2e-5, …) —
    operand self-OVERLAP in the seal neighborhood (cap fan to the axis
    vert vs seal-region triangles), the #146-family conformality scale,
    NOT sub-resolution; the loud STOP is correct until the
    seal-neighborhood emission is overlap-free (follow-up task #195).**
    Corpus: byte-identical to the 255C/0W/54E/1T baseline (zero
    category or detail deltas); yang-rs suite + rewrite + parity green.
  - **#195 CHARACTERIZED 2026-07-22 (probe-only, spec
    `specs/yang_195_seal_neighborhood_self_overlap.md`): the F0082
    residual is a PRODUCER defect — Extrude-11's union output B-Rep is
    SELF-INTERSECTING at the wall-masked seal corner.** New probes: the
    `s4-dc-attr` arm (double-cover-edge triangle attribution at the (4b)
    gate) + `YANG_INPUT_SELFX_PROBE` (exact
    `detect_improper_contacts` + double-cover scan + involved-face loop
    dump on every operand mesh handed to the arrangement). Measured:
    every boolean in the F0082 chain is clean EXCEPT Extrude-12's
    operand A (5 improper tri-tri contacts); the four faces involved
    (cap disc 362, wall 368, coplanar seal-plane duo 370/371, tube
    lateral 373) share the B-Rep boundary vertex **v925 — #188's
    antipodal triple point, +1.25e-3 BEYOND wall face 368's plane** —
    i.e. a boundary vertex strictly inside the union's material. The
    #188 "submerged rim run" was made output boundary. **Inc-1 (same
    day) measured the producing-op mechanism**: the producing union's
    inputs are CLEAN but its kept mesh already carries 7 improper
    contacts at the seal corner (`YANG_SELFX_PROBE` chain sweep, incl.
    an intra-tool (B,0)×(B,2) pair) — the crossing is
    RELOCATION-MINTED in-boolean (true cap×wall penetration is
    sub-sagitta in the input chords; Stage-4 mints v925 beyond the
    wall) = the Yang §4.5.4 illegal-self-intersection class whose
    removal half is N2's remit, now with its first 0-WRONG-blocking
    customer. The #173 render gate misses it (depth 5.6× the grazing
    band; suspected PR-KV11 vertex-adjacency skip — unverified). Fix
    vehicles (producer-side, spec §3): §4.5.4 removal / wall-junction
    trim (the deferred J3 osculation corner assembly) vs #172-pattern
    pre-tessellation graze rebuild; consumer-side normalization
    REJECTED (P9).
  - **#195 inc-2 SHIPPED 2026-07-22 GATED OFF (spec §5,
    `YANG_RIM_PLANE_GRAZE_ENABLE=1|on`): rim×plane graze-guard arm**
    — the vehicle decision was measurement-grounded
    (Phase-0 `YANG_NSEG_FLOOR` release sweep: baseline ERROR; floor 32
    silent WRONG χ=1; 40/41/48/64 CORRECT with the whole chain's
    operand meshes scanning CLEAN — the paper's §4.5.4
    detect-and-refine remedy holds end-to-end on the shipped
    pipeline). New `rim_plane_graze_n` /
    `rim_plane_graze_min_segments` (boolean/rim_junction.rs) mirror
    the #172 Case-III guard for a Circle rim shallowly crossing a
    partner Plane face: `depth = r·k − |m̂·c+d̂|`, demand the minimal N
    with `sag(r,N) ≤ depth/2` (the floor-32 WRONG row is the measured
    proof the factor-2 margin is load-bearing), #178 noise line +
    `2·10⁻³·r` render-observability line + self-limiting natural-N
    gate; NO SubSagitta STOP arm and NO phase filter (spec §5c —
    F0082's wall face legitimately intersects the tube elsewhere, a
    face-global touch test would veto the needed boost). On F0082 the
    guard fires on three tube unions (N=53/43/22), the producing
    union's emitted B-Rep is no longer self-intersecting, and
    **Extrude 12 SUCCEEDS — the #195 characterized defect is FIXED by
    the mechanism; the case's frontier moves to never-before-reached
    Extrude 14**: a disordered output-face boundary loop (one mid-arc
    sample appended after the chain end → self-crossing loop,
    correctly rejected loud by the input-conversion CDT at the next
    boolean). That is a DISTINCT defect (#145/#184 sample-order
    family, minted in op-11's output/`from_yang` path) → inc-3.
    **Gate-ON corpus (spec §5e, post-triage 099c8d39): 256C/0W/52E/2T
    — NET +1 CORRECT over baseline, zero silent wrongs.** The
    first-run "R0063 silent-WRONG flip blocker" was REFUTED as a meta
    authoring error (euler_target 2→0; genus-1 derived exactly from
    the authored numbers, the R0091/#186 pattern) — R0063 is the
    THIRD conversion alongside R0072/R0095. Remaining gate-ON deltas
    all route to KNOWN walls (no new defect class): R0021 = F0045
    render ring-reject, the SAME output-ring detour class as F0082's
    gate-ON Extrude-14 frontier (§5f — one inc-3 fix plausibly clears
    both); R0061 = the u32::MAX LRR split_max_passes §4.5.2 shell
    (#171 class); **F0085 = a REAL blocker, not timing — at a 400s
    budget it COMPLETES as silent-WRONG χ=1 (odd χ = impossible for
    any valid closed surface; the 120s TIMEOUT merely masks it);
    #145 family = a third inc-3 output-ring customer.** The arm
    stays gated per the zero-regression flip precedent; gate-OFF
    corpus byte-identical.
    Inc-3 = the output-ring boundary-selection detour (the #188
    dead-side fingerprint at the newly-minted junction, spec §5f);
    its fix is the flip path. `YANG_CDT_PROBE` extended with a 3D
    global-vert + outer-edge/chain dump; `KV2_OUT_TOPO_PROBE` banked
    (per-output-face pinch / outer-inner shared-vert scan).
  - **#195 inc-3 (2026-07-23, probe-only, spec §5g): the §5f
    "output-ring / envelope arc-trim" hypothesis is REFUTED for
    F0082-E14 and the blocker RE-ROUTED to #130.** Three measurements:
    (1) the #188 envelope machinery is INERT gate-ON
    (`YANG_S5_ENVELOPE_PROBE` 0 lines) — the graze boost samples the
    crossing directly, so there is no osculating pair to select;
    (2) the producing union's RAW output face-370 loop is CLEAN (0
    self-intersections, `KV2_RECOVER_PROBE`) and carries a spoke to the
    **tube AXIS point** (`v1387` = the tube `axis_point` to 5e-6) — the
    M8 coplanar tool-base signature (§10.10 finding #1, the SAME
    structure the gate-OFF Extrude-12 χ=3 decomposed into, now surfacing
    gate-ON one op later); (3) the self-cross is minted in the kernel-v2
    round-trip — `recover.rs` correctly fuses the rim chords to a
    `Circle` arc and `to_yang` re-samples it, and the near-rim junction
    v1277 (1.457e-3 inside the circle) is attached by wall-chord + spoke
    edges that BOTH cross the re-sampled arc (2 self-intersections),
    CDT-rejected LOUD. So F0082's remaining gate-ON blocker is the
    **#130 M8 coplanar tool-base residue** (real root: the seal face
    should carry the tube rim as an inner-loop hole, no axis spoke),
    OUTSIDE #195's rim-plane-graze scope; neutering recover's (correct)
    fusion to launder it would be a P9 violation. **#195's F0082 work is
    COMPLETE** (§5a: the §4.5.4 seal self-overlap is fixed; all chain
    operand meshes scan clean gate-ON). The always-on flip stays blocked
    by KNOWN walls only — #130 M8 (F0082) and the §4.5.2 LRR shell
    (R0061). R0021/F0085 must be re-probed: the unified "output-ring"
    class is refuted for F0082-E14 and can no longer be assumed to be
    one inc-3 fix.
  - **#195 inc-3 continued (2026-07-23, probe-only, spec §5h):
    R0021/F0085 RE-PROBED — both are the graze guard's OWN over-firing,
    NOT external walls, and the arm is NOT flip-safe as designed.**
    R0021 is a **false positive**: gate-OFF CORRECT at natural rim N=11,
    but the guard fires (`req=12 natural=(MAX,11)` — a real
    depth>render-line rim×plane crossing) and the forced global
    rim-rebuild to N=12 degenerates an unrelated thin two-rim strip's
    render ring (FaceId 11, measured NON-self-intersecting → DEGENERATE,
    F0045 family). Crux: **under-sampling a shallow rim×plane crossing
    does NOT imply a defect** — R0021 is correct at natural N with the
    same crossing, so the guard's premise is false in general (F0082's
    malignant relocation-minted crossing is the special case). F0085 is
    the same root **at scale**: on a 6000+v chain the guard fires on
    MANY ops with high N (66/65/59/70/67, `YANG_SPLIT_PROBE`); the
    repeated global N≈60–70 rebuilds ARE the 400s/timeout and the χ=1
    emission destabilization. **The flip path is NARROWING the guard
    (inc-4: a malignant-only detection proxy and/or surgical single-rim
    rebuild scope), not waiting on #130/§4.5.2.** The arm correctly
    stays gated; the unified "output-ring" class is fully refuted.
  - **#195 inc-4 SHIPPED gated (2026-07-23, spec §5i): detect-then-refine
    (paper §4.5.4) REPLACES the eager pre-tessellation boost —
    `YANG_RIM_PLANE_GRAZE_ENABLE` now drives a wrapper around
    `boolean_once(..., refine_rim_plane)`: pass 1 at natural resolution →
    cheap graze gate → input-side (`NonManifoldInput`) skip → #173 selfx
    detect → pass 2 refine ONLY a broken output → accept iff not-worse
    (`improper <= n`). A CORRECT natural output is NEVER refined, so the
    eager false-positive class is structurally excluded. **Gate-OFF
    byte-identical (255C/0W/54E/1T); gate-ON 258C/0W/50E/2T — net +3,
    ZERO correctness regressions**: R0063/R0072/R0095 ERROR→CORRECT
    (conversions banked), R0021/R0061 stay CORRECT (eager's CORRECT→ERROR
    regressions eliminated), F0085 an honest in-budget ERROR (was TIMEOUT
    masking silent-WRONG χ=1), R0081 ERROR→TIMEOUT (a pre-existing slow
    ERROR whose legit LRR refine doubles it past budget — not a
    correctness regression). F0082 unchanged (Extrude-14 = #130 M8). The
    always-on flip precondition (zero CORRECT→ERROR, #169 P3b inc-5
    precedent) is MET; flip = inc-5.
  - **#195 inc-5 SHIPPED ALWAYS-ON (2026-07-28, spec §5j) — the gate is
    GONE**, flipped in one commit together with the §4.4.1 rim-snap pass
    (`YANG_S4_RIM_SNAP_ENABLE`, spec `yang_s4_boundary_curve_relocation.md`
    §18), which it DEPENDS on. Re-measured against the honest
    `strict-validation` baseline, two back-to-back full-corpus runs:
    **baseline 252C/0W/58E/0T → gates-ON 254C/0W/56E/0T; per-case diff over
    all 312 cases = exactly two deltas, R0072 and R0095 ERROR→CORRECT, zero
    CORRECT→ERROR.** Three corrections to the inc-4 reading: the "+3" is
    really **+2** (R0063 is ERROR in BOTH states — the `VertexOffSurface`
    ledger catches it independently); **R0081 does NOT go TIMEOUT** (honest
    in-budget ERROR both states at 240s, so inc-4's ERROR→TIMEOUT was a
    120s budget artifact, not a cost of the refinement); and the arm is
    **not independent** — boosting the rim exposes the same latent Stage-4
    relocation gap rim-snap closes, so `n2_junction_cluster::i1` is GREEN
    with both gates off, RED with the graze arm ALONE (1 vertex 6.84e-7 off
    the cylinder vs a 1.00e-9 band), GREEN with both. `YANG_S4_TRIPLE_POINT_ENABLE`
    (§4.4.1 inc-3) stays GATED — it was not in the measured combination.
  - **AMENDMENTS 12–16 SHIPPED 2026-07-30/31 (spec
    `specs/m8_stage0_multiclass_cavity_arm.md` §3–§14, ALWAYS-ON).** The
    fold-gate repair ladder grew, in order: the per-class WEDGE cavity
    decomposition (§3, amendment 12); the Fig-11(b→c) MERGE arm behind the
    rim-chain boundary-order settle check (§10, amendment 13 — R0059's
    boundary-order inversion fixed en route); the Fig-11(a)
    vertex-inserting SPLIT (§11, amendment 14 — **R0099 ERROR→CORRECT,
    260C**); the open-link pure-SLIDE splice with the settle predicate as
    a preventive commit certificate on BOTH split arms (§13, amendment 15
    — **F0064 ERROR→UNSUPPORTED(coplanar)**, its ops[3] deferral proven a
    GENUINE N17 real-overlap pair; canonical C0048's stranded split
    re-cohered); and the GROUP-ATOMIC mint-collapse revert (§14,
    amendment 16): the task-#61 sub-floor shared-mint groups now revert
    WHOLE — every member to ONE shared chord target — at BOTH revert
    authorities (the amendment-2 fallback and the settle check), closing
    the tear that shipped real-scale phantom same-ray station pairs.
    Amendment-16 results: **the C0048 #144 azimuth-merge count wall is
    structurally DEAD** (68v67 was an ulp lottery over torn groups; the
    case advances to the deeper cherchi `DegenerateTpi` arrangement wall
    the 2026-07-12 refuted translation arm also uncovered), and
    **F0067's N17 deferral is exposed as a DESYNC ARTIFACT** of the same
    tearing — with coherent fused interface meshes (§4.5.5 mesh identity
    restored) its coplanar pair CONSTRUCTS and the case proceeds to a
    cherchi `LabelMismatch` flood-fill wall (UNSUPPORTED→ERROR, the typed
    coplanar tail 3→2; F0064/F0072 remain genuine). Canonical
    **260C/0W/48E/0T**. The `specs/m8_exact_opposite_rim_projection.md`
    tangential-separation design is OBSOLETE for this customer class —
    the "inseparable same-ray twins" were ONE feature torn in two, not
    two legitimate samples.
  - **AMENDMENT 17 SHIPPED 2026-07-31 (spec §15, ALWAYS-ON): sub-band
    LIFT absorption into mint-collapse groups — F0067's cherchi
    LabelMismatch traced to a NON-minted femto-cluster member.** One
    geometric crossing (A's gear edge × B's rim) existed as a 2D femto
    cluster (spread 4.3e-14): two rim MINTS (fused by #61 to the exact
    on-circle value) plus one chord-world LIFT that can never join a
    collapse group (groups form over `minted_info` only) — it resolved
    1 ulp away, BOTH values shipped, the two solids' interface chains
    diverged, and cherchi's flood-fill caught the label stitch across
    the resulting manifold gap. Fix: during the #61 pass, absorb every
    non-minted, non-corner, non-rim-anchored vertex within the
    rounding-noise band `TAU_WORK·(1+uv_scale)` of a qualified group's
    elected member (five orders above the measured cluster, three below
    the protected E-C1b R0088/R0070 distinct-twin population — band
    pinned by a unit tripwire) and ENROLL it as a full group member so
    the amendment-16 atomic revert covers it both ways. Corpus: ZERO
    category deltas, one justified detail drift — **F0067 clears
    cherchi entirely (arrangement + labeling + emission) and advances
    two stages to the NAMED §4.5.2 `LocalRefinementRequired` wall**
    (the mesh-updating epic's territory). Probes banked:
    `CHERCHI_PATCH_PROBE` crossing anatomy, boolean.rs face
    attribution, `[s0-build-probe]` Stage-0 container drill-down.
    Canonical unchanged 260C/0W/48E/0T — the amendment removes a
    divergence CLASS (one value per femto crossing cluster, §4.5.5
    mesh identity at every absorbed site corpus-wide).
  - **AMENDMENT 18 SHIPPED 2026-07-31 (spec §16, ALWAYS-ON):
    congruent-rim cross-solid table ELECTION — C0048
    ERROR→SUPPORTED_CORRECT, canonical 261C/0W/47E/0T.** C0048's
    DegenerateTpi wall traced to a femto NEEDLE input triangle whose
    ulp-twin corners are one `rim_a` anchor + one `rim_b` anchor at
    the SAME junction azimuth of the SHARED congruent r=1.5 circle
    (flush plates; A's frame angle 1.1220 ≡ B's 5.1612) — protected
    from every existing identification BY DESIGN (rim-aware
    clustering must not move on-circle points; #61 covers only mints;
    §15 excludes rim anchors). Fix: post-cluster table ELECTION
    (adopt the lexicographically-smaller 3D bit pattern's (uv, point)
    wholesale; rewrite the losing table key, polygon corners, and the
    E7 cluster map) + ring propagation (elected point →
    `rim_overrides` of the resolved losing cap edge; the #143
    uniform-slot merge adopts the bits — the losing corner is a
    uniform slot since the sextet class sits at chord endpoints,
    which the override endpoint window excludes — counts unchanged
    by construction; unresolvable losing edge SKIPS the fusion
    whole). Census: 13 pairs ALL in C0048 (F0064/F0072/R0059/F0086
    zero) — the class is the congruent-rim flush-stack geometry.
    Corpus: exactly ONE delta (the conversion), zero detail drift.
    The C++ AR3b jolly-plane fallback
    (computeTriangleOfSegmentInCoplanarCase, extracted and recorded
    in spec §16a) stays a SEPARATE cherchi robustness increment.
  - **Epic #169 §4.4.1 MUTUAL-PAIR arm SHIPPED ALWAYS-ON 2026-07-31
    (spec `specs/yang_n2_stage4_cdt_mesh_updating.md` §5c.11).** F0067's
    post-amendment LRR wall anchored: `degenerate_no_longedge` was a
    DEADLOCK — two zero-area triangles astride ONE shared long edge
    (off-vertices strictly interleaved ON the segment; a relocation-moved
    endpoint), each the other's long-edge neighbour, so the committed
    strip-unzip's "defer until the neighbour resolves" never fires. Fix:
    when no simple action exists, drop both members and Fig-11(a)-split
    the two OUTER neighbours so both sides adopt the identical fine chain
    `a–bL–bH–c` — two-sided conformal by construction, pure connectivity,
    per-edge fwd/rev balance conserved; unit-oracled on a closed pillow
    fixture. F0067 (both quads) and R0038 (its §5c.10 triple = mutual
    pair + one chained simple action — two-sided insertion removes the
    tangency caps the one-sided re-CDT refutation could not) clear the
    STOP and advance to PRE-EXISTING walls the STOP had masked: F0067 →
    the (4b) Stage-4 watertight gate (edge fwd=1 rev=2 in a region the
    arm never touched — flush-stack coincident-sheet, #146-class
    upstream family); R0038 → `s6-planar-loop-nonplanar` (#137 tangency
    reassembly). Sweep 312/312: ZERO category deltas (261C/0W/47E/0T
    verbatim), exactly the two justified detail deltas. LRR site census
    banked: R0009/R0047 = `split_max_passes` runaway (own queue);
    R0032/R0050/C0067 specific-vertex region-invalid; R0044 = M5.
  - **TWO-SIDEDNESS PRECONDITION SHIPPED ALWAYS-ON 2026-07-31 (spec §5c.11a)
    — R0038's advance above was MINTED by the arm, not unmasked.** The
    "PRE-EXISTING walls" claim holds for F0067 (balance-census confirmed)
    but was extended to R0038 without its own measurement. An
    attribution-planarity census (0 off-plane attributed triangle vertices
    at Stage-6 entry, 2 immediately after Stage 4; arm-disable switch: ON
    → 2, OFF → 0) shows the arm mints it. Mechanism: R0038's two OUTER
    neighbours SHARE their third vertex (dd=17 both sides) — one fan over
    the chain, not two opposite sides — so `nl`'s piece `[bl,bh,dd]` and
    `nh`'s piece `[bl,bh,dd]` are the same triangle and the update emits
    it TWICE. That double cover imports a foreign vertex into an unrelated
    planar face's loop `|ac|` = 7.5056 off its plane (the long edge runs
    parallel to that face's normal), surfacing as `NonManifoldOutput`.
    Fix: `mutual_pair_candidate` rejects when the outer neighbours share
    their third vertex — exact index equality, precisely the condition
    under which the four pieces are not distinct; honest deferral to the
    loud STOP. Arm-firing sweep 312/312: only F0067 (6, all two-sided) and
    R0038 (1, the only same-apex) fire it at all — **no SUPPORTED_CORRECT
    case fires it**, so no passing output carried a silent double cover.
    Corpus: 261C/0W/47E/0T verbatim, one detail delta (R0038 →
    `LocalRefinementRequired`). The same-apex fan's real repair (a
    3-triangle refan) is NOT built — it is the next #169 increment.
  - **AMENDMENT 19 SHIPPED ALWAYS-ON 2026-07-31 (spec
    `specs/m8_stage0_multiclass_cavity_arm.md` §17): sub-band lift
    absorption extended to SINGLETON mint clusters — F0067's Stage-4
    crack field 16 unbalanced edges → 0.** After the mutual-pair arm,
    F0067 stood at the (4b) Stage-4 watertight gate; the new
    `YANG_S4_BALANCE_PROBE` census reported the SAME 16 unbalanced
    edges at `s4-entry`, `pre-degen-loop` and `post-degen-loop`,
    proving the imbalance arrives from upstream and that no Stage-4
    pass mints it. Root: the coplanar interface (A face 328 × B face
    0) carries femto twin vertices — 74 pairs in operand A, 33 in B,
    **the same two 1-ulp values present in BOTH solids** — because the
    §15 absorption is nested inside `for g in groups.filter(|g| g.len()
    > 1)` and a cluster with exactly ONE mint plus N lifts forms a
    SINGLETON group the filter drops. Measured: F0067 fires 72 groups
    and 68 absorbs while the offending cluster (`mint(rev)` + five
    `lift`) is touched by none; the new `[mint-collapse] SINGLETON`
    census finds 24 such sites in that case. Fix: the §15 predicate is
    factored into `absorbable_sub_band_lifts` and shared by the
    multi-mint path, the singleton path and the census — **band
    unchanged**, only its reach; a singleton that absorbs nothing
    returns before any mutation (byte-identical), one that absorbs
    becomes an ordinary group covered by the amendment-16 atomic
    revert. F0067 clears the Stage-4 gate and advances to
    `s6-planar-loop-nonplanar` (face 888, off-plane 4.1e-5 — a REAL
    defect, #153/#146 family). Corpus 312/312: **zero category deltas,
    canonical 261C/0W/47E/0T preserved**, one detail delta — R0050,
    bisected rather than assumed: its pre-Stage-0 inputs are
    byte-identical between states, every singleton fusion in both
    customers measures **sub-ulp to 2-ulp** (banked ulp census), and
    the 3.75e-2 defect it exposes is four orders beyond any possible
    fusion displacement — a latent Stage-6 defect behind a Stage-4 STOP
    that used to fire first. The band was deliberately NOT tightened
    (it would fork §15's metric and is the tuning the constitution
    forbids).
  - **LOOP-SIMPLICITY CENSUS 2026-08-03 (`YANG_S6_LOOP_SIMPLICITY`,
    read-only; `crates/yang-rs/src/stage5_loop_simplicity.rs`) — the
    planar-and-self-intersecting emitted-loop class MEASURED corpus-wide,
    and the 922a9892 anchor's inherited 8-case membership CORRECTED in
    three places.** Every Stage-6 gate is per-VERTEX (`s6-planar-loop-nonplanar`,
    `s6-planar-degenerate-loop`), but simplicity is a property of the whole
    CYCLE — so a self-intersecting loop leaves the producer clean and is first
    refused one crate away by kernel-v2's exact CDT. The scan closes that
    measurement hole: exact `dashu` orientation + on-segment predicates over a
    dominant-axis projection (which copies the surviving f64 coordinates
    verbatim), four separated columns — `cross` (proper transversal), `touch`
    (pinch / collinear overlap), `spike` (adjacent-pair backtrack), `degen`
    (zero-length segment). It runs BEFORE the non-planarity gate deliberately:
    gating it on a wall would blind it to its own subject. ~5% overhead
    (F0067 62→65s), off by default. Sweep = subprocess-per-case
    `single_case` ×312 (the `ASSAY_JOBS` driver nulls child stderr).
    **186,234 planar loops scanned, 0 unmeasurable; 6,870 curved faces NOT
    scanned (no exact 2D projection) and 47 cases where Stage-6 emission never
    ran — both reported, never silently dropped.** Results:
    (1) **THE SCOPING ANSWER — a proper CROSSING separates the corpus
    perfectly: 0 of 261 SUPPORTED_CORRECT, 0 of 3 UNSUPPORTED, 0 of 1
    EXPECTED_ERROR, 9 of 47 ERROR.** `touch`/`spike` do NOT: F0055
    (SUPPORTED_CORRECT, 33-pt loop, `touch=4`) and F0064 both survive
    non-simplicity. So the actionable predicate is `cross > 0`, not
    non-simplicity — a STOP on the latter would REGRESS F0055.
    (2) **R0028 REFUTED.** It reports the IDENTICAL wall text ("ring rejected
    by CDT (degenerate/self-intersecting)") yet all 10 of its planar producer
    loops are simple. It shares the WALL, not the DEFECT — either the
    degenerate branch of a generic message, or kernel-v2's RE-SAMPLED render
    ring rather than the Stage-6 emitted loop (cf. the FaceId(11)
    non-self-intersecting→DEGENERATE render ring above). **The wall text was a
    bad proxy for the class.**
    (3) **R0051 and R0100 ADDED** — self-crossing producer loops that fail at
    DIFFERENT walls (`SelfIntersectingBooleanOutput` face_a 8 / face_b 10, 88
    penetrations; `patch triangulation folded (inverted tri)`). The class is
    larger than the wall that named it.
    (4) F0067 confirmed and enlarged: **17 faces / 150 crossings in its final
    op, not one ring**, with `max_s4_disp/min_seg` up to 5.2e4 (the anchor's
    5.8× is the mild end); `max_s4_disp=6.0602e-3` reproduces the anchored
    sagitta to the digit. Its faces 357/359 carry `touch=5 spike=2` with ZERO
    crossings — a pinch/backtrack sub-mechanism the ring-reject probe could not
    separate from a crossing.
    Confirmed members: F0045 F0067 R0004 R0011 R0049 R0051 R0074 R0085 R0100.
    Canonical **261C/0W/47E/0T reproduced verbatim with the probe ON**.
    **RING-REJECT SWEEP 2026-08-04 (all 312, `KV2_RING_REJECT_PROBE` then
    `KV2_RING_PROVENANCE` on the hits).** Exactly **8 cases reject a CDT ring
    and ALL 8 rings SELF-CROSS** (every one `TriangulationFailed`; the wall
    string's "degenerate/" half fires NOWHERE, and no rejected ring is simple).
    Producer split — the `idx=0` provenance line preceding each reject names
    the face and its core, and matches the wall face in all 8: **planar
    (`sampled_loop_points`) = F0045 F0067 R0011 R0074 R0085; developable
    (`tessellate_developable_patch`) = R0004 R0028 R0049.** Both cores funnel
    through `triangulate_ring`, which is why one string covered two producers.
    R0028 anchored separately —
    `specs/r0028_developable_ring_cap_overshoot.md`: 3 B-Rep vertices 3.60e-4
    beyond the face's own cap plane, Stage 4 relocating nothing
    (`n_relocations=0`), and a SINGLETON even within the developable trio
    (R0004 3.3e-16 / R0049 2.8e-17 rim overshoot = ulp noise; neither of their
    crossings touches a rim row). **Scoping consequence: R0004 and R0049 hold
    BOTH defects** — self-crossing planar loops AND a developable wall that
    fires first — so planar loop-coherence alone cannot convert them. The
    realistic §4.5.2 candidate set is **F0067 R0011 R0074 R0085** (+F0045,
    already attributed to a §4.5.3 seam reversal); R0051/R0100 fail at
    unrelated walls; R0028 was never a member. The
    repair remains §4.5.2 loop-coherent local refinement under epic #169 (carry
    the neighbours or refuse); a producer-side simplicity STOP stays a P10 net
    only — now quantified as 9 rewordings and 0 repairs.
  - **MULTI-CLASS cavity arm SPEC WRITTEN 2026-07-30 —
    `specs/m8_stage0_multiclass_cavity_arm.md` (amendment 12).** The R0099
    producing-op probe (1f576621) named the fold gate's structural gap: a
    rim-crossing mint sits ON the intersection curve by construction, so
    its constraint-blocked cavity is multi-class by construction, and the
    amendment-5 ear-clip's single-class polygon form rejects
    (`multi-class cavity with constraint-blocked fan`) → amendment-2
    reverts the correct on-circle mint to its chord lift →
    `VertexOffSurface` (or silent-wrong pre-strict-validation; Stage 4
    never runs on coplanar-only ops, so nothing downstream can rescue).
    Design: per-class WEDGE decomposition of the grown cavity — cut the
    link at class-transition spokes (the intersection polyline through the
    mint, moved WITH it), re-fan/ear-clip each wedge against its own class
    with the shared `earclip_cavity_polygon`; conformality across the
    moved polyline is by shared spoke identity (Yang §4.4.1 Fig 11 at the
    overlay level; the #169 Phase-A principle one stage earlier). inc-0 =
    the fold-revert corpus census (both probe sites banked in 1f576621);
    inc-1 primitive + R0099 chain pin; inc-2 flip; inc-3 joint-form and
    inc-4 n-ary reduced-gate (slice-g B8) parity, census-gated.
    post-#143), with two fresh class diagnoses:**
    (1) **6× render ring-reject** (F0045 R0011 R0016 R0028 R0059 R0072) —
    `TessellationFailed("ring rejected by CDT")` on a SUCCESSFUL solid;
    R0072's FaceId(11) ring (probe `KV2_RING_REJECT_PROBE`) is a 167-vertex
    micro-scale loop with overlapping collinear SPURS along a mid-line (the
    ring doubles back on itself) — the known F0045 near-coincident-junction
    family (N2/LRR-adjacent).
    (2) **3× Newell-normal disagreement** (F0064 ×2 ops, R0051; F0067 ×2
    ops on top of its #144 wall; + R0063 post-#145) — `KV11_PROBE` on F0064
    shows the output "planar" faces carry loop vertices OFF-PLANE at REAL
    scale (1.7e-3 and 4.5e-3, alternating between two nearby parallel
    planes 4.5e-3 apart) — an output loop-assembly / junction-relocation
    defect (N2 family), NOT a winding-midpoint issue; the wall is honest.
    **ROOT LOCALIZED 2026-07-12 (task #146, probe `YANG_T146_PROBE` in
    `emit_topology` planar emission + `YANG_V_PROBE` map dump):** F0064's
    offenders (mesh verts 57/62/82/83 op-3, 1198/1219 op-4) sit BIT-EXACTLY
    on their side plane pre-relocation and are registered in BOTH
    `vert_circle` AND `vert_pp_planes` — a CIRCLE × (plane∩plane line)
    triple point. PR-KV11 fix 3 reroutes only the ELLIPSE × pp-line
    combination into the junction closed form; the CIRCLE arm has NO
    analog, so the plain circle relocation wins and slides the vertex
    along the circle off the pp-line's planes by the observed 1.7–4.5e-3.
    **Increment 1 SHIPPED same day (spec
    `specs/yang_stage4_circle_pp_line_junction.md`):** new Stage-4 pass
    reroutes `vert_circle ∩ vert_pp_planes` into
    `vert_pp_circle_junction` (dedup to exactly ONE distinct pp-line as
    UNORDERED plane pairs, else loud LRR — the KV11 rule) with its own
    relocation arm: junction = pp-line ∩ SPHERE(C, r) quadratic + a
    circle-plane residual certificate (exact for BOTH the in-plane
    configuration this class exhibits — the pp-line lies IN the circle's
    plane, where PR-F3's transversal plane-piercing form is degenerate —
    and the transversal one; no inclination tolerance branch), gated by
    the derived crossing amplification `2·d_ε/sin θ` (θ = line direction
    vs circle tangent at the junction). PR-F3 / KV11 arms byte-identical;
    the five over-determined audits treat the new map like the existing
    junction maps. Oracles: `s146_*` unit trio (closed form in-plane /
    transversal / miss / inside-circle piercing; pp_line; unordered
    dedup). Corpus: **F0064's and R0051's Newell walls RETIRED** (both
    move to a deeper NonManifoldOutput reassembly wall; F0064 2→1 failing
    unions); R0063 still Newell (DIFFERENT sub-mechanism, measured same
    day: `KV11_PROBE` face 4 is a MICRO SLIVER — 5-Seg loop, extent
    ~1.8e-4 × 2.8e-4 × 4.8e-6 at case scale 1.74e-3 — with ONE vertex
    8.9e-8 off the other four's plane; the 0.44° Newell tilt is that
    sub-band positional noise on a tiny lever arm. This is the
    R0009/R0091 KV15b micro-scale MINT-ACCURACY family, not a junction
    routing defect — a future increment needs the minting-site accuracy
    treatment, not more Stage-4 rerouting); F0067 moves to a face-272
    re-entry CDT wall (#145-adjacent, on top of its #144 wall).
    (3) **4× re-entry CDT failure** (R0061 R0095 `holed lateral CDT
    failed`; R0063 F0085 `CDT triangulation failed`) — a SUCCESSFUL op's
    output B-Rep fails conversion at the NEXT op. R0061 face 2 (probe
    `YANG_T133_PROBE`): the unrolled ribbon polygon zigzags — the loop
    chain ALTERNATES between original edge-polyline samples (globals
    128/129/130) and chain-inserted override samples (261-264); sorted by
    unroll-u the point set is cleanly monotone, but the ORIGINAL samples
    128/129 are geometrically SWAPPED relative to their stored order.
    **Localized same day (`YANG_T145_PROBE`, banked in
    `tessellate_lateral_holed_cdt`):** the face's boundary is a chain of
    139 tiny per-segment EllipseArc edges (per-pair conic vocabulary;
    consecutive edges share centers with flipped normal signs), each
    contributing start + 1 interior Steiner; every arc's own rebuilt chain
    is monotone — the STORED VERTEX ORDER is inverted along the curve
    (loop vertices 128/129/130 at unroll-u 0.0448/0.0436/0.0481, zigzag
    amplitude ~1.2e-3 real scale). The PREVIOUS op minted edges between
    misordered curve samples; suspect Stage-4 relocation near tangency
    swapping neighbours along the intersection curve (the task-#137
    family). Fix direction: restore per-curve parameter order BEFORE
    Stage-6 edge minting, or a validate_solid monotone-parameter check to
    make the producing op loud. Re-entry-side repair is impossible (edges
    already minted between the wrong vertex pairs). Task #145.
    (4) Remainder: 3× Stage-3 AmbiguousCurve conic (C0043 R0008 R0026, M5
    class), Stage-4 LRR/OffCurve (R0044 R0074 R0081 + subtract-chain
    singletons), 3× non-2-manifold reassembly (C0044 C0058 F0082, §4.3.3
    tangency family), 2× planar-triangle render collapse (R0012 R0098),
    C0046 NonManifoldVertex, C0075 edge-not-2-directed, R0091
    ellipse-endpoint, R0004 revolve-axis, singleton Stage-N walls.
  - **KV4-F1 ✅ RESOLVED (PR-KV4-F1, 2026-06-12): the rational-ray
    fallback** — `rational_ray_inner_label` in cherchi-rs
    `labeling/inside_out.rs` implements the branch the C++ reference
    acknowledges and exits on ("a fully implicit patch that requires
    exact rationals for evaluation… does not support rationals",
    booleans.cpp:578 → deviation N21). Trigger class diagnosed on F0016:
    a sub-f64-resolution NEEDLE patch — an input edge piercing a triangle
    femto-close to its corner mints an LPI ~1e-17 from an existing vertex
    (the same f64-crooked-input root as PR-KV10, one layer down), so
    every explicit patch vertex is a border vertex and the approximated
    triangle defeats the f64 generated ray. The fallback classifies in
    pure RBig: exact centroid of a patch triangle as origin (strictly
    interior by positive exact area — the border restriction dissolves),
    exact axis-ray crossing/strictly-inside/sort over ALL `in_tris`, the
    nearest-hit rule reduced to `inner ⇔ n_k > 0` (pinned against the
    f64 `orient3d` convention by a dedicated oracle), exact grazes retry
    X→Y→Z then the loud typed `RationalRayDegenerate`. Set semantics
    mirror the f64 sort (equal-key collapse + the N20 `t ≤ 0` discard).
    Oracles: needle-fixture trio in `inside_out.rs` (sub-ulp LPI whose
    f64 approximation collapses onto the explicit vertex; inside → {B},
    outside → {}), convention pin, full cherchi suite + sidecar parity
    untouched (the fallback fires only where the reference terminates).
    Corpus: **SUPPORTED_CORRECT 46→52** (F0016/F0018/F0019/F0021/F0025 +
    R0086), ERROR 47→41, WRONG 0→0. Residue findings: **KV4-F1b** F0022
    classifies fine through the fallback but its third union fails yang
    reassembly (`NonManifoldOutput` — patch-boundary layer, distinct
    cycle); **KV4-F1c** R0067's boolean now SUCCEEDS (was the curved-
    patch NoExplicitRayOrigin) but the result fails kernel-v2 render
    tessellation — moved one layer, still ERROR.
  - **PR-KV11 ✅ (2026-06-13): the ellipse-arc junction class** — the
    largest coherent ERROR class ("output ellipse-arc endpoint does not
    lie on its ellipse", 8 cases: F0046–F0050, R0041, R0095, F0076 —
    cylinder × oblique box). FIVE stacked fixes, each one layer deeper:
    (1) *Stage-4 ellipse junction detection generalized to the
    cylinder+plane arm* (`insert_ellipse_or_junction`, shared with the
    PR-KV9 cyl×cyl arm): a vertex ending TWO different cylinder∩plane
    ellipses (the box-EDGE crossing) was silently overwritten in
    `vert_ellipse` and relocated onto only the last-scanned ellipse,
    staying off the first by the Stage-1 chord error (~1e-4 ≫ the 1e-9
    import band).
    (2) *§4.5.3 reversal test scoped to ONE curve* (paper
    `refs/text/yang2025_hybrid_boolean.txt:709-745`: p_b/p_r/p_n progress
    along the intersection curve C): at a junction where the loop
    TRANSITIONS between conics the discrete tangent legitimately kinks;
    the angle test false-positived and the sweep collapsed the junction
    loop vertex-by-vertex (the kv11 fixture's vanishing back-bulge —
    watertight but missing material).
    (3) *Ellipse × (plane∩plane line) TRIPLE points* (`vert_pp_planes`):
    a pp-segment (capB∩faceA trace) is exact, but its endpoint on the
    chordized lateral is the rim triple point — relocation onto the
    ellipse alone slid it off the cap plane (the "plane normal disagrees
    with Newell" rejects). Resolved into the ellipse-junction closed form
    `(plane ∩ plane) ∩ cylinder` with a synthetic second member; the
    junction gate carries the derived `1/|d̂·r̂|` along-line amplification
    (the KV9 gradient-band pattern — flat 2·d_ε under-gates).
    (4) *EllipseArc midpoint augmentation in from_yang pass-1d Newell*
    (the KV9 crescent fix existed in `validate::winding_points` but NOT
    in boolean.rs — the fix-all-gates-sharing-a-metric trap).
    (5) *Hybrid exact/quantized mesh oracles* (test-harness): the pure
    position-weld ALIASED junction-pinch thin-wedge tessellation (sliver
    fans hugging boundary arcs within the grid cell) into false
    non-manifold/Euler verdicts — the KV8c gear lesson on PARTIALLY
    exact-paired meshes. Exactly-paired (f32-bitwise) edges are provably
    closed and excluded; only the cross-face residue is quantized +
    T-subdivided; vertices weld by cell only where they bound residue
    edges. Self-intersection skip widened to vertex-adjacent pairs
    (chord sagitta at a shared junction vertex is tolerance-band
    geometry, ~1e-3 ≫ the weld threshold; real penetrations still fail
    via non-adjacent pairs). Plus a mixed-2D-orientation fold tripwire at
    patch emit.
    Oracles: `yang-rs/tests/kv11_ellipse_edge_junction.rs` (box ∪
    edge-piercing oblique cylinder: endpoints on BOTH ellipses, junctions
    ON the edge, back-bulge present; RED→GREEN through fixes 1–2).
    Corpus: **SUPPORTED_CORRECT 52→58** (F0046–F0050 + R0041),
    **ERROR 41→31**, WRONG 0→0; R0006/R0095/F0076 now stop at the KV7
    curved partial-patch re-entry wall (typed, honest). Remaining ERROR
    classes by size: 14 gear timeouts (perf), 5 geometric face
    resolution (R0011/R0073/R0082/R0090), 5 NonManifoldOutput reassembly
    (R0079/R0092/F0022/F0056 + F0076 residue), 2 Stage-4 DegenerateTriangle
    (R0009), 2 patch folds (F0042/F0045, KV9-F2), singles F0041/F0057/
    F0058/F0059/R0067.

- **KV12 — arc-segment profile extrude (gears).** A gear/arc profile already
  carries a complete sampled `vertex_ids` polygon (the app samples each arc into
  16 chord points); `arc_segments` are annotations indexing into it
  (`ArcSegment.start/end_vertex_index`), exactly like the `spline_segments` form
  PR-KV8 already extrudes.
  - **Tier 1 (chord-approx, the unblock) — ✅ DONE (2026-06-13).** The
    `arc_segments` wall in `make_faces_from_profiles` (`adapter.rs`) now mirrors
    the spline wall: an arc-segment profile is rejected ONLY when it has no
    `vertex_ids` polygon; with a polygon it routes through the existing polygon
    path (the chords are the samples the solver/viewport already use — no new
    approximation, documented/loud not silent). Bores = an inner loop of the
    profile (single extrude, no boolean). Sufficient for 3D printing. Tests:
    `kernel-v2/tests/kv8_gear_profile.rs` (with-polygon extrudes / no-polygon
    walled / E2E arc prism volume), GUI `app/tests/gui/arc-profile-extrude.spec.js`
    (closed D-shape line+arc → extrude → body). WASM rebuilt.
  - **Tier 2 (exact): SPEC WRITTEN — `specs/kv12_tier2_arc_extrude.md`** (Phase
    E, 2026-06-14). New `ProfileRegion::ArcPolygon` variant + an extrude
    assembler that emits arc-bearing planar caps + per-edge walls (planar for
    segments, **cylinder patch** for arcs — an arc swept along the normal is a
    cylinder lateral). Surface vocab (cylinder faces, planar-arc caps,
    `signed_volume` for arc faces) + the assembler templates (`extrude_circle`,
    `build_partial_revolve`) already exist. Bulk of the cost is **exact arc-loop
    simplicity validation** (arc–segment / arc–arc intersection predicates) — the
    only piece without scaffolding (P9: exact, not sampled). Increments E1–E4 +
    the B-Rep target, predicates, and acceptance are in the spec. Downstream:
    extruded-arc solids as boolean operands hit the KV7 curved partial-patch
    re-entry wall — a separate gap (extrude-only).
  - **Sequencing:** Tier 1 is the gearbox unblock (prototype-release Phase B);
    Tier 2 is a quality follow-up (Phase E). Scoped to extrude-only — booleans on
    the result are out of scope. **Comes before KV6 revolve** (independent).
    Driven by `docs/prototype_release_roadmap.md`.

- **KV13 — provenance / topological naming (face → creating feature).** 🔜 **PLANNED.**
  "Click any face → the feature that *introduced* it, through chained
  booleans/extrudes," + the inverse (feature → its faces), surviving rebuilds.
  The persistent-naming problem (`docs/PERSISTENT-NAMING.md`). Capstone of the
  prototype-release arc (Phase F); week-scale, multi-subagent.
  - **Substrate (exists):** yang-rs `TriangleAttribution` `(InputId, input_face_idx)`
    per output triangle; modeling-ops `Provenance` (`created`/`deleted`/`modified`
    with `Rewrite{before,after,reason}` + `role_assignments`); GeomRef `Role`/
    `Signature` selectors.
  - **Work:** (F1) `FaceOrigin { created_by, derived_from }` keyed on persistent
    identity, not churning `KernelId`. (F2) boolean origin attribution via
    `TriangleAttribution` → operand `feature_id`; new cut-faces → the boolean
    feature; inherited faces chain back (loud on `None`-attribution, P9).
    (F3) lineage propagation across rebuild (feature-engine, re-derived per
    rebuild like body-name inheritance). (F4) **persistent identity hardening**
    (the long pole — survive upstream-sketch edits; pragmatic role+signature
    scope, short of full Parasolid-grade naming, boundary documented).
    (F5) `get_face_data` emits resolved `created_by_feature`. (F6) UI: face →
    feature highlight + inverse. (F7) verification matrix incl. edit-and-reresolve
    + adversarial no-mislabel.
  - **Note:** a coarse **Tier 1** (click face → the body's *producing* feature
    via the GeomRef anchor already on the wire) is a separate cheap app-only win
    (prototype-release Phase D), exact for single-feature bodies; KV13 is the
    full through-boolean lineage. Strictly after the gearbox.
    Driven by `docs/prototype_release_roadmap.md`.

- **KV14 — adapter hole assembly (single-sketch holed extrude / ring gears).**
  ✅ **DONE (2026-06-13).** `make_faces_from_profiles` previously built every
  `ClosedProfile` as a hole-less `Profile` (passed empty holes, ignored
  `is_outer`), so a plate-with-bore / ring gear drawn as one sketch did not
  extrude as an annulus (kernel-v2 `extrude` always supported holes — the gap
  was adapter-side). Now a three-pass assembler: (1) classify each profile
  (circle / polygon, `is_outer`); (2) group each inner (`is_outer=false`) loop
  into the **strictly-larger** containing outer (centroid witness + area filter
  — rejects the app region-detector's redundant same-loop pairing, where a loop
  is emitted as both outer and hole); (3) stage one `Profile::new(outer, holes)`
  per outer, output aligned 1:1 with input profiles (the `profile_index`
  contract). A circle rim that carries holes is polygonized (64-gon) since a
  holed circle needs a polygon outer; plain circles stay exact. `Profile::new`
  remains the exact gate (containment/disjointness/nesting + CCW-normalize), so
  the f64 grouping can't produce a wrong solid. Tests: `kv8_gear_profile.rs`
  holed-square→annulus (volume == (outer−hole)×depth, output len == input len);
  GUI `holed-extrude.spec.js` (two nested rects → annulus, inner-wall face
  count). kernel-v2 (17 bins) + test-harness (29) + GUI (46) green; WASM
  rebuilt. Resolves prototype-release C2.

## 4b. Completion roadmap — Phases 1–6 (the full path to replacing legacy)

The M0–M8 list above is the *milestone* sequence for the boolean. This section is
the **completion** view: what it takes for `kernel-v2` to **replace** the legacy
kernel — handle planar + curved + coplanar + non-convex, implement the `Kernel`
trait, pass assay at parity-or-better, and run in WASM. It reconciles M5–M8 with
the **under-tracked** `kernel-v2` driver (Phase 4) + migration (Phase 6).

**"Complete Yang" ::=** kernel-v2 implements `Kernel`/`KernelIntrospect`; planar +
curved + coplanar + non-convex all handled; assay ≥ legacy on the supported
corpus; runs in WASM; `crates/kernel/` deleted — with **reference parity vs the
Cherchi C++ sidecar maintained throughout** (the non-negotiable correctness oracle).

```
Phase 1 ─► Phase 2 ─► Phase 3 ─► Phase 4 ─┐
                                          ├─► Phase 6 (migrate, assay, delete legacy)
Phase 5 (native arrangement + WASM) ──────┘   [parallel track, joins before 6]
```

- **Phase 1 — Finish the analytical SSI engine (`ssi-rs`).** *[in progress; ⊂ M5]*
  PR-SSI4 (parabola/hyperbola + through-apex), then the remaining A15.4 pairs
  (Degree-4: sphere∩cyl, cyl∩cyl, cone∩cone, sphere∩cone, cyl∩cone; torus pairs).
  **Exit:** all 15 quadric pairs analytically solved, adversary-hardened,
  on-surface-exact. **Risk:** low–moderate. **Size:** medium (~10–15 PRs).
  *Frontier (out of scope):* revolving an arbitrary profile → non-quadric surface
  of revolution → numerical/marching SSI (Patrikalakis Case F), a later capability.
- **Phase 2 — Curves enter the pipeline (Stages 1/3/4/6 curved).** *[⊂ M5; the
  heart, highest risk]* **First step done (PR-YR6 ✅):** curved `Surface`/`Curve`
  enum variants exist (mirroring `ssi-rs` field shapes) and the pipeline rejects
  them LOUDLY (`YangError::CurvedSurfaceNotYetSupported`) — no curved
  tessellation or `ssi-rs` call yet. **Stage-1 cylinder tessellation done
  (PR-YR7 ✅)**, **first end-to-end curved boolean done (PR-YR8 ✅):
  cylinder ∪ box flows through Stages 2/5/6, watertight + Euler=2, analytic
  `Surface::Cylinder` survives with exact params**, and **Stage 3 exact
  intersection edges done (PR-YR9 ✅): `ssi_rs::intersect` now refines the
  cylinder∪cap arrangement edges to the exact `Curve::Circle`/`Ellipse` (no
  longer mesh-approximate), with a P9 STOP on intersect/selection failure**, and
  **Stage 4 mesh updating done (PR-YR10 ✅): the mesh crossing points are
  RELOCATED onto the exact circle (Yang §4.4.1, not a global CDT) + §4.5.3
  reversed-point correction; watertightness inherited; cylinder ∪ box is now
  exact-edge AND on-curve, adversary-verified fold-free.**
  **Stage-1 sphere tessellation done (PR-YR12 ✅):** closed solid sphere → a
  watertight z-up lat/long mesh with a bijective `TessellationMap` (`d_ε =
  1e-2·2r√3`; cone still rejects loudly).
  Remaining: Stage 1 curved tessellation for the rest (cone; non-convex
  profile triangulation via Livesu earcut-CDT, for gears; Steiner points);
  ~~Stage 3 refine arrangement edges to the exact SSI curve (wire `ssi-rs` in —
  P3)~~ (PR-YR9 ✅); ~~Stage 4 conform mesh to refined curves~~ (PR-YR10 ✅,
  circle only — ellipse relocation + §4.5.2 real local refinement still loud
  STOPs);
  ~~Stage 6 curved cavity-sense for Subtract (deferred in PR-YR8)~~ (PR-YR13 ✅,
  `box − cylinder` blind pocket via `BRepFace.reversed`; ~~through-hole genus-1~~
  PR-YR14 ✅ via per-shell Euler gate χ=2−2g; ~~box − sphere hemispherical
  dimple~~ PR-YR15 ✅ via `Surface::Sphere` wiring + `sphere_chord_bound`;
  CONE cavities + internal spherical voids + box-as-subtrahend still open) +
  cut-surface
  faces (deferred in PR-YR5). **Exit:** cylinder ∪ box ✅ (exact edges + on-curve mesh),
  sphere − cylinder → correct curved B-Rep, sidecar mesh-parity + analytically
  exact edges. **Risk:** HIGH (paper-critical). **Size:** large.
- **Phase 3 — Coplanar preprocessing (Stage 0).** *[= M8]* detect coplanar face
  pairs pre-tessellation; 2D boolean → A-only/B-only/overlap; shared trimmed
  surface + identical meshes; overlap boundaries → intersection curves. **Exit:**
  flush/stacked faces + multi-plane cross-booleans work without conformal-edge
  explosions. **Risk:** moderate–high. **Size:** medium–large.
- **Phase 4 — The `kernel-v2` driver (Kernel trait).** *[NEW — the integration
  unlock; not in M0–M8]* **Phase 4a COMPLETE (2026-06-10):** PR-KV1 (arena +
  Euler operators) → PR-KV2 (Profile + lamina/extrude constructors) →
  PR-KV3 (boolean via yang-rs, exact tessellation, introspection) →
  **PR-KV4 (EXIT)**: `KernelV2Adapter` in test-harness implements the legacy
  `Kernel`/`KernelIntrospect` traits over kernel-v2 (polygon profiles,
  extrude, booleans, tessellate→RenderMesh, extract_edges, signatures;
  revolve / curved profiles / fillet-chamfer-shell are loud `NotSupported`),
  and `tests/assay_kv2.rs` replays the 190-case corpus categorized.
  **First honest corpus score (2026-06-10): 0 SUPPORTED_CORRECT / 5
  SUPPORTED_WRONG / 173 UNSUPPORTED (137 curved-profile, 24 revolve, 12
  coplanar-boolean) / 12 ERROR.** Every corpus case has ≥2 ops; the planar
  multi-op cases all hit either the M8 coplanar wall or real yang-rs boolean
  defects, so the always-on SUPPORTED_CORRECT gate is synthetic dispatch-path
  scenarios (single/oblique/L-profile extrudes pass ALL mesh oracles;
  subtract/intersect/cut/union pass with exact volumes but fail render-mesh
  conformity — finding KV4-F3). **Findings logged in PR-KV4:**
  KV4-F1 yang-rs `NoExplicitRayOrigin` in/out classification failure on
  oblique-box unions (R0029, F0016, F0018, F0019, F0021, F0025);
  KV4-F2 yang-rs "geometric face resolution failed (coplanar multi-solid
  label)" on coincident/oblique unions (F0001, F0017, F0022–F0024) and
  "input B-Rep is not 2-manifold" on a union-of-union round trip (F0020);
  KV4-F3 kernel-v2 tessellation drops exactly-collinear chain vertices
  per face → boolean render meshes are not position-paired watertight
  (B-Rep validates, volumes exact); KV4-F4 disjoint-operand unions return
  valid 2-shell solids that the legacy meta scores as wrong (F0011–F0015,
  χ=4 vs euler_target 2 — needs triage: correct output vs legacy
  expectation). The curved/coplanar walls confirm Phases 2–3 as the score
  unlock (137 + 12 cases). **Exit (met):** feature-engine builds +
  tessellates through kernel-v2; assay runs categorized
  (`cargo test -p test-harness --test assay_kv2 -- --ignored --nocapture`,
  report at `target/assay_kv2_report.json`).
  **PR-TH1 (2026-06-10) — oracle fixes + KV4-F4 triage:** the assay's mesh
  oracles were mis-scoring correct solids. Fixed in `test-harness/src/oracle.rs`
  (the NEW kernel's measurement instrument): (1) T-junction-aware
  watertight/χ pairing — edges are split at position-quantized vertices lying
  exactly on them before pairing, since kernel-v2's tessellation legitimately
  subdivides a shared boundary on one side only (KV4-F3's false-positive
  half); edges that do not close under subdivision still fail; (2) the
  penetration-depth guard now normalizes plane equations (it compared
  geometric thresholds against |n|-scaled distances, |n|≈2·area ~1e4, so
  f32-noise grazing contacts were flagged); a pair counts as penetrating only
  if each triangle crosses the other's plane by > the weld-tolerance depth;
  (3) KV4-F4 RESOLVED: disjoint-union outputs are correct 2-shell solids —
  χ now expects `euler_target + 2·(#shells−1)` with shells derived from the
  welded mesh. **Corpus score: 14 SUPPORTED_CORRECT / 1 SUPPORTED_WRONG /
  0 ERROR** (was 6/9/0). Movers: F0003, F0009, F0010 (T-junction FPs),
  F0011–F0015 (KV4-F4), F0012/13/15 grazing FPs. The one remaining WRONG is
  **R0029 — a REAL defect**: T-junction-aware pairing exposed a latent seam
  the raw pairing could not see — 4 coincident sheets along a split box edge,
  χ=3 (two spheres glued along an arc); the PR-YR24 near-coplanar gate does
  not fire for it. KV4-F3 NARROWED: union/pocket/through-hole smoke scenarios
  now pass the FULL oracle set; subtract/intersect still emit one degenerate
  sliver triangle (real tessellation defect, allowance kept for exactly
  `watertight_mesh` + `no_degenerate_triangles` on those two).
  **PR-KV5a (2026-06-11) — circle profiles → cylinder solids:** kernel-v2
  curved core (vertex-anchored closed `Curve::Circle` edges, `Surface::Cylinder`,
  V2/E3/F3 topology deliberately matching the yang-rs M5 fixture), curved-aware
  `validate_solid`, cap/lateral tessellation under the sagitta band, analytic
  volume/area.
  **PR-KV5b (2026-06-11) — cylinder booleans wired through yang-rs:** the
  curved boolean boundary is REAL. Survey-driven (yang's native cylinder×box
  outputs carry `Plane`+`Cylinder` surfaces and `LineSegment` + Circle-ARC
  edges — never full circles; original rims come back faceted at Stage-1
  resolution): `to_yang_brep` emits canonical cylinder solids as the shared-edge
  M5 fixture shape; `from_yang_brep` reassembles partial cylinder patches
  (`Curve::Arc` vocabulary, cavity sense on `Surface::Cylinder::reversed`,
  unrolled-winding orientation validation — the developable Newell analog);
  cylinder-patch render tessellation (unrolled cut + exact ear-clip +
  Delaunay-flip quality pass + Euclidean-LEPP chord-bound refinement); adapter
  maps legacy `CircleProfile`. End-to-end GREEN: cylinder∪box, blind pocket,
  through-hole (genus 1), intersect, canonical round-trip (bitwise analytic
  volume). Typed walls: Ellipse/Parabola/Hyperbola outputs named
  (`UnsupportedBooleanOutputCurve`), cyl×cyl surfaces yang's Stage-3 SSI text,
  partial-patch results cannot re-enter yang Stage 1 (`UnsupportedCurvedBoolean`).
  **Corpus: 17 SUPPORTED_CORRECT / 11 WRONG / 18 ERROR / 144 UNSUPPORTED
  (70 curved-profile, 38 revolve, 36 coplanar)** — first fully-correct curved
  cases R0006, R0083, F0044. **Findings:** KV5b-F1 yang emits one ULP-LENGTH
  edge per intersection circle (two verts 1 ulp apart; I6 weld is bit-exact
  only — Stage-4 relocation dedup needed upstream; collapses to zero-area
  slivers in f32 render meshes); KV5b-F2 euler oracle double-counts shells when
  the case meta's `euler_target` already encodes a multi-body expectation
  (F0031–F0040: correct 2-shell χ=4 scored against 4+2); KV5b-F3 yang walls hit
  by the corpus: Stage-3 `AmbiguousCurve` (cyl×cyl: F0041/43/45/58),
  `NoExplicitRayOrigin` (R0067/R0086), Stage-4 `LocalRefinementRequired`
  (F0056/57), non-2-manifold reassembly (R0092, F0052), an output edge used ≠2
  times (F0059), and one debug-tier `VertexOffSurface` import-band trip (F0042).
- **Phase 5 — Native arrangement + WASM.** *[= M6 + M7; parallel track]* M6 native
  `cherchi-rs` Stage-2 behind the `LabeledArrangement` seam, parity-green vs the
  sidecar (retires the C++ subprocess) — **✅ COMPLETE (M6: PR-CR-BL3c; M7:
  PR-CR-M7a/b/c, 2026-06-10)**: clean-room indirect predicates from Attene's
  paper replaced the LGPL FFI in every production path and the WASM build is
  restored (`cargo check --target wasm32-unknown-unknown` green for
  cherchi-rs / yang-rs / kernel-v2, no feature flags). Browser-side
  validation happens with the Phase-6 wasm-bridge migration.
  **Exit (met):** pure-Rust boolean compiling to WASM.
- **Phase 6 — Migration + assay.** **✅ COMPLETE (2026-06-11, user-directed
  early cutover).** The user overrode the parity-or-better exit gate ("the
  legacy kernel is useless and its scores are irrelevant" — its boolean output
  was geometrically wrong, e.g. F0038 rendered without cylinder walls). Landed
  as six commits: known-red legacy test purge → kernel contract
  (traits/types/units/MockKernel) moved to `waffle_types::kernel` →
  `KernelV2Adapter` moved into kernel-v2 as the production trait impl → all
  consumers swapped (wasm-bridge, file-format `export_step` over
  `dyn KernelBundle`, test-harness `ModelBuilder::kernel()` deleted) →
  **`crates/kernel/` deleted (64k lines)** → WASM bundle rebuilt on STABLE
  wasm-pack (3.0MB, was 4.9MB; catch_unwind/panic=unwind machinery deleted).
  `test.sh fast`/`full` fully green for the first time since the rewrite
  began; boolean GUI specs 60/60 on kernel-v2; F0038 renders correctly in
  the app. Capability-pending tests carry `#[ignore]`/`test.skip` milestone
  tags (M8 coplanar, M5 degree-4, CDT profile tail) — un-quarantine when
  each milestone lands.

- **KV6b — booleans over revolve solids. ✅ COMPLETE (2026-06-12, PR-KV6b-1/2).**
  yang Stage-1 ingests the revolve vocabulary: the NEW directional input-arc
  convention (Circle edge with start≠end = CCW sweep around curve.normal,
  unique in (0,2π) — π and major arcs unambiguous; outputs unchanged at
  SSI sub-arc granularity), per-edge sample CHAINS (shared, watertight),
  generalized planar CDT over spliced chains (annular sectors, holed circle
  caps), partial-cylinder quad strips, reversed input walls (+ Stage-6
  reversed XOR propagation). kernel-v2 to_yang_brep converts all revolve
  face classes; the operand wall narrowed to boolean-OUTPUT re-entry.
  End-to-end: revolve×box union/subtract incl. exactly-π and 350°
  operands, washer cuts with cavity-sense preservation, R0084 = first
  corpus revolve boolean SUPPORTED_CORRECT (31 total). FINDINGS for later
  milestones: **KV6b-F1** R0060 union output carries 2 non-manifold edges
  (yang triage); **KV6b-F2** most corpus 'rectangle' revolves are OBLIQUE
  to their axis (the assay generator's axis basis ≠ the engine's staging
  basis) → they sweep CONES and wall at KV6c — corpus revolve movement is
  gated by KV6c (12 cases) + KV6d torus (14 cases); **KV6b-F3** crossing
  booleans where a box face is PARALLEL to the revolve axis hit the
  plane×cylinder SSI LINE case (ssi-rs pair #2 'partial') — Stage-4
  relocates intersection points off-surface, caught loudly by the output
  Newell check / AmbiguousCurve.

- **KV6 on-axis slice 1 — lathe shaft (rectangle touching the axis). ✅
  SHIPPED (2026-07-07, task #65, spec
  `specs/kv6_on_axis_revolve_rectangle.md`).** `revolve` no longer
  conflates axis-CROSSING (invalid input) with axis-TOUCHING: a full-turn
  4-gon with exactly one on-axis edge — the most common lathe op — now
  builds the canonical KV5a solid cylinder by DELEGATING to the
  extrude-of-circle construction (no new topology code; analytic π·r²·h
  volume bitwise-adjacent). C0061/C0062/C0069 ERROR→SUPPORTED_CORRECT
  including their chained groove-cut booleans; corpus 183→186 CORRECT /
  0 WRONG. Remaining on-axis shapes (C0063/C0064 solid cones/frusta —
  apex or on-axis edge + oblique) stay on the typed boundary = KV6c
  slice 2's vocabulary.

- **KV6 on-axis slice 2 increment A — solid frustum (oblique off-axis
  edge). ✅ SHIPPED (2026-07-07, task #66, spec
  `specs/kv6_on_axis_revolve_oblique.md`).** The on-axis recovery
  classifier dispatches on the off-axis edge class: axis-parallel keeps
  the slice-1 extrude-of-circle delegation; OBLIQUE builds the SOLID
  FRUSTUM via a direct assembler mirroring `extrude_circle` (same Stroud
  §3.1.4 single-fake-edge census — 2 seam vertices, 2 rims + 1 seam
  ruling, 3 faces) with the analytic `Surface::Cone` from the slant
  (EdgeClass::Oblique formulas). Everything downstream is the existing
  KV6c vocabulary: `validate_cone_face`, exact `(πH/3)(r₀²+r₀r₁+r₁²)`
  flux volume, `tessellate_cone_lateral`, and the 5c boolean path —
  frustum − ⊥ slab chains end-to-end in the oracle suite. **C0064 (three
  coaxial stacked frusta, interpenetrating unions through cone×cone
  coaxial-circle SSI) flips ERROR → SUPPORTED_CORRECT on its exact-volume
  oracle**, pinned green in `named_case_categories`. Adversary findings:
  a mixed-sign oblique quad passing the perpendicular-cap gates always
  self-intersects the on-axis edge (rejected upstream as
  `ProfileNotSimple`, pinned); pencil quads (oblique CAP edge) and
  partial-angle on-axis profiles stay on the typed boundary. Remaining
  slice-2 shape: the on-axis APEX TRIANGLE (C0063 primary — solid cone,
  apex an interior singular point of the lateral) = increment B: 1-rim
  apex branches in `validate_cone_face` / `signed_volume` / the cone
  tessellator; apex-cone boolean operands stay typed
  (`UnsupportedCurvedBoolean` — the 1-half-edge lateral loop fails
  `to_yang`'s 4-edge pattern loudly).

- **KV6 on-axis slice 2 increment B — solid apex cone (C0063 primary). ✅
  SHIPPED (2026-07-07, task #66, same spec).** The on-axis APEX TRIANGLE
  (one perpendicular cap edge + one oblique edge reaching the axis) builds
  the SOLID CONE: 1 seam vertex, 1 edge (base rim), 2 faces — the apex is
  an INTERIOR SINGULAR POINT of the lateral (yang's own cone model), not a
  topological vertex; V−E+F = 2 holds and the vertex-fan orbit closes at
  arity 2. New 1-rim apex branches in `validate_cone_face` (outward sense
  = rim traversal axis toward the apex; `reversed` apex cavities rejected
  typed — no producer), `signed_volume` (same frustum flux with ρ_lo = 0 —
  exact π·r²·h/3), and `tessellate_cone_lateral` (base ring in the cap's
  bitwise frame + apex fan reusing the 2-rim strip's orientation
  transform; watertight against the disc cap). Apex-cone boolean OPERANDS
  stay typed (`UnsupportedCurvedBoolean` → adapter NotSupported — the
  1-half-edge lateral loop fails `to_yang`'s 4-edge pattern). **C0063
  moves ERROR → UNSUPPORTED(curved-profile)**: the primary cone builds;
  the case's real boundary is its OBLIQUE slab cut (conic-bounded cone
  patch — the genuine cone-patch vocabulary, still future work with
  KV6c 5c's oblique-cut note). Adversary: fan-winding and lateral-rim-
  sense mutations both caught; bicone triangles pinned typed;
  reversed-apex fixture pinned typed. Remaining KV6 revolve gaps after
  this slice: KV6d torus boolean-output recovery, C0070 non-alternating
  profiles, oblique cone cuts (conic-bounded patches), partial-angle
  on-axis profiles.

- **KV6c increment 5 — partial revolve of oblique edges: the arc-bounded
  cone patch. ✅ SHIPPED (2026-07-08, tasks #81/#82, spec
  `specs/kv6c_partial_revolve_cone_patch.md`, commits 710a783b + 92f21d15).**
  The single largest capability wall in the corpus: 34 of the 38
  UNSUPPORTED(revolve) cases were this one mechanism (2026-07-08 census).
  Increment 1 (kernel-v2): `build_partial_revolve` passes the
  `EdgeClass::Oblique` params through as the `Surface::Cone` wall (the
  [seg, arc, seg, arc] loop was already class-generic); new
  `validate_cone_patch` (the cylinder unrolled-winding analysis in the
  cone's (θ, τ) development, per-arc on-cone agreement `r = τ·tan α`);
  new `cone_arc_patch_flux` (per-arc Green's-theorem closed form — on a
  cone `x·n̂` is τ-independent, so each arc contributes
  `−(τ²/2)·(apex·(t̂s−t̂e) − tanα·(apex·â)·Δθ)`; reproduces the shipped
  full-band form at ±2π); render tessellation via ONE shared
  developable-patch engine (the cylinder patch unroll+CDT machinery
  parameterized by a `DevSurface` chart — radius-at-v and normal tilt are
  the only differences; `r_unroll` = max boundary radial distance).
  Increment 2 (yang): the partial cone STRIP arm in `tessellate_cone_face`
  (arc chains sample by sweep fraction of the shared n_seg —
  radius-independent, so a wall's two chains always pair), rims/arcs split
  explicitly (an open chain in the frustum-band arm would phantom-wrap
  silent-wrong geometry — now impossible); the kernel-v2 conversion gate
  removed. The FULL chain (cherchi, Stage-3/4 cone sections, output
  recovery into the new patch vocabulary) worked with no further changes —
  exact truncation volumes through the wall-crossing subtract.
  **Corpus: +9 SUPPORTED_CORRECT (R0002/10/18/33/37/55/68/69/80), 0 WRONG,
  zero lost.** The remaining 27 of the group's 36 cases moved to their
  next honest boundaries: 15 loud downstream ERRORs (several Stage-3
  `AmbiguousCurve{2,2}` cone-pair matching stops — M5-family — plus
  boolean/tessellation failures to census), 10 deeper typed UNSUPPORTED
  walls (curved re-entry, multi-shell, coplanar), 2 container TIMEOUTs
  (R0052/R0094). R0008's pin reconciled to ERROR (Stage-3 AmbiguousCurve).
  Adversary: flux-sign / reversed-sense / tolerance-widening mutations all
  caught (the third needed a new negative test, added); finding —
  near-cylindrical cones (apex ~1e9 away) lose ~8 digits to f64
  cancellation in the flux (~7 sig figs retained, documented, not a
  defect). Remaining KV6 revolve stock after this slice: KV6d closed
  torus (C0065/C0067), oblique cone sections (ellipse-arc vocabulary —
  the `#[ignore]`d boundary probe in `kv6c_partial_cone_boolean.rs`),
  boolean-output partial-patch re-entry (R0051 class), C0070
  non-alternating profiles.
  **Post-slice ERROR census (the 15 new loud stops), by mechanism:**
  4× Stage-4 `LocalRefinementRequired` (R0017/32/47/49); 2× Stage-3
  `IntersectFailed(AnalyticalSolutionNotAvailable)` (R0019/44 — missing
  SSI solver, M5); 2× Stage-3 `AmbiguousCurve` (R0008 {2,2}, R0003
  {1,0}); 3× render KV9-F2 fold tripwire on boolean-output cone patches
  (R0034/54/65 — DIAGNOSED via the new `KV2_PATCH_FOLD_PROBE`: the
  trimmed cone face's boundary is a conic-section CHORD polyline at
  yang's coarser d_ε (~1.1 off-surface at r≈519) interleaved with
  render-tolerance on-surface interior points at ~0.2 triangle height →
  inherent fold; the exact-arc retag in recover.rs only covers ⊥-plane
  circle sections, and these cuts are oblique box faces → conics. The
  honest fix is the cone conic-arc vocabulary (ellipse/hyperbola
  sections — see R0100's explicit `UnsupportedBooleanOutputCurve
  ("Hyperbola")` wall), not tolerance games); 1× CDT ring rejection
  (R0016, likely same family); 1× `VertexOffSurface` (R0099); 1×
  R0004 partial on-axis revolve (a KV6 gap: the on-axis recovery is
  full-turn-only) behind a second boolean failure.

- **KV6a-tilted — full-turn revolve of non-alternating profiles (C0070).
  ✅ SHIPPED (2026-07-11, task #135, spec
  `specs/kv6a_nonalternating_full_revolve.md`).** C0070 (rectangle
  revolved 360° about the tilted in-plane axis (1,1,1)/√3 — all four
  edges OBLIQUE, four cone frusta, zero annuli) died at
  `build_full_revolve`'s wall/annulus alternation gate. Analysis: the
  gate was protecting nothing — `rim_on_edge`'s twin arithmetic is
  class-agnostic, and the wall rim-normal rule is wall-wall consistent
  by construction (`reversed ⟺ sign(Δt)` along the CCW profile, so every
  wall rim carries one sign of `â` at its head vertex and the opposite
  at its tail; adjacent walls meet head-to-tail → twin rims always
  traverse oppositely, at t-monotone junctions AND t-extreme crests).
  The gate narrowed to the one real residual (consecutive ANNULAR edges
  — a subdivided radial edge → coplanar adjacent faces, typed
  `NotImplemented`); `RevolveResult.start_cap`/`end_cap` became
  `Option<FaceId>` (an all-wall ring has no planar face to name).
  Red→green in `kv6a_revolve.rs` §8: all-oblique diamond ring (V=4 E=8
  F=4 R=0 G=1 χ=0, 2+2 outward/reversed cones, capless, Pappus 8π at
  1e-12, watertight mesh), staircase pentagon with a wall-wall junction
  (V=5 E=8 F=5 R=2, caps ∓â, 20π), consecutive-annuli rejection,
  determinism. **Corpus: C0070 ERROR → SUPPORTED_CORRECT; assay
  238 CORRECT / 0 WRONG / 48 ERROR / 9 UNSUPPORTED / 0 TIMEOUT,
  zero-lost (verified per-case against the 2026-07-11 baseline).** Its meta
  `euler_target` 2→0 was an R0099-class authoring correction (a
  full-turn revolve of a simple profile strictly off-axis is a
  solid-torus ring, genus 1 — forced by `validate_revolve_geometry`);
  fixed in `gen_complexity.rs` + the committed meta (hand-edit, no
  regen). Remaining KV6 revolve stock: KV6d closed torus (C0065) +
  sphere via on-axis circle revolve (C0067), oblique cone sections
  (ellipse-arc vocabulary), boolean-output partial-patch re-entry
  (R0051 class).

- **KV6d closed torus — full-turn circle revolve (C0065). ✅ SHIPPED
  (2026-07-11, task #136, spec `specs/kv6d_closed_torus_revolve.md`).**
  A strictly-off-axis circle profile revolved exactly 360° now builds
  the CLOSED ring torus: the minimal CW structure of T² (Stroud §3.1.4
  seam representation) — 1 seam anchor at the outer equator, 2 closed
  seam circles (poloidal profile + toroidal outer equator), 1
  `Surface::Torus` face whose outer loop is the aba⁻¹b⁻¹ square with
  BOTH twin pairs internal (`assemble_closed_torus`, validated by the
  existing arena invariants unchanged; V−E+F−R = 0 = 2(S−G), G=1).
  Render: `tessellate_torus_lateral` grew a `closed` arm (θ rows wrap;
  seam recovered from the equator CIRCLE when no seam ARC exists).
  Boolean re-entry: `to_yang` gained the all-Circle `closed_torus`
  4-edge pattern (twin checks on both pairs); yang Stage 1 gained
  `tessellate_torus_closed` — a doubly periodic (θ × φ) grid over the
  two seam rings (φ-value column convention from #131; unit oracle
  `torus_closed_full_turn_doubly_periodic`: every undirected edge
  exactly 2, χ = 0, area fills 4π²Rr). Red→green in
  `kv6d_closed_torus.rs`: census (V=1 E=2 F=1 G=1), Pappus 2π²Rr²,
  watertight, determinism, on-axis sphere wall
  (`RevolveOnAxisCircleUnsupported`, C0067's honest message), crossing
  circle stays `RevolveAxisIntersectsProfile`, and a meridian-plane
  half-cut boolean (exactly half Pappus volume, χ=2). **Stage-4
  hardening discovered by the C0065 e2e:** the shaft's outer wall
  (x=1.45) is near-tangent to the outer equator (ρ=1.5, gap ≈ the
  Stage-1 sagitta), so the inscribed mesh's intersection oval closes
  EARLY — entirely inside the bounded wall — minting a phantom lens
  lump; the torus implicit-pair relocation then dragged that loop onto
  the infinite-surface curve OUTSIDE the wall face (|y| = 0.384 vs the
  wall's 0.25) and the output carried a silent overlapping double cover
  (10 penetrations, SUPPORTED_WRONG). Fix: a **bounded-face containment
  guard** after the wedge displacement gate — a relocated torus-edge
  vertex must stay inside every matching PLANE partner face's hull
  (loop vertices + curve extents, the t134 closed-loop lesson) + d_ε;
  escape = typed `OffCurveBeyondChordBand` STOP (mutation-checked;
  adversary pin `closed_torus_near_tangent_shaft_stays_loud`). The
  honest conversion of that configuration is the §4.3.3 near-tangency
  increment (C0058 resolution-neck class — mesh topology ≠ exact
  topology when a face gap dips under the sagitta). **Corpus: C0065
  UNSUPPORTED(revolve) → typed ERROR (Stage-4 OffCurveBeyondChordBand);
  C0067 UNSUPPORTED(revolve) with the new SPHERE message (KV6d
  increment 2 = the remaining closed-surface revolve gap); C0066 and
  the torus classes byte-stable. Assay 238 CORRECT / 0 WRONG /
  49 ERROR / 8 UNSUPPORTED / 0 TIMEOUT — zero-lost (C0065 is the only
  category mover; R0074's failure moved one op EARLIER, its Revolve-2
  auto-union output previously fed a downstream CDT death and now stops
  typed at the relocation wall).** Probe `kv6d_c0065_probe.rs` banked
  (replica + partial-torus control: the SAME shaft against a 350° tube
  dies at the same Stage-4 wall — the near-tangency gap pre-dates the
  closed torus).

- **KV6d increment 2 — on-axis full-turn circle revolve → SPHERE
  (C0067). ✅ SHIPPED (2026-07-12, task #136, spec
  `specs/kv6d_sphere_revolve.md`).** A full-turn revolve of a circle
  profile centered ON the axis now builds the CLOSED solid sphere —
  the `RevolveOnAxisCircleUnsupported` typed wall is REMOVED (variant
  deleted), and with it the LAST revolve capability gap: the assay's
  UNSUPPORTED(revolve) bucket is EMPTY. New kernel-v2 vocabulary:
  `Surface::Sphere { center, radius, reversed }` +
  `geom::sphere_residual` + `validate_sphere_face` (topology-agnostic,
  torus-validator precedent). Topology (`assemble_closed_sphere`):
  minimal seam structure of S² mirroring the PR-YR12 yang contract —
  V=2 poles at `center ± r·ẑ` (CANONICAL world z-up regardless of the
  revolve axis; the sphere is isotropic and yang's lat/long
  parameterization is fixed z-up), E=1 meridian seam `Curve::Arc` twin
  pair through `center + r·x̂` (normals ∓ŷ), F=1, χ = 2 = 2(S−G) with
  G=0. Render: `tessellate_sphere_closed` (z-up lat/long grid, poles
  single-vertex fans, modular longitude wrap) +
  `tessellate_sphere_patch` for boolean outputs — a new PUBLIC yang-rs
  UV-CDT consumer (the torus-patch recipe on the (lon, lat) plane):
  non-wrapping loops → disk + period-shifted holes (with an
  orientation gate — a complement-bounding loop, sphere minus a side
  cap with both poles inside the region, returns None instead of
  silently rendering the cap); a single-wrap outer loop → pole-cap
  bridge (wu·reversed picks the contained pole; the two meridian seam
  copies and the subdivided UV pole line carry BIT-IDENTICAL 3D points,
  welded post-CDT into a watertight fan). **Trap (measured):** the UV
  pole line as ONE 2π·r constraint edge has a diametral circle covering
  most of the domain — spade's `keep_constraint_edges` refinement then
  refuses nearly every Steiner insertion (19% area deficit, hemisphere
  rendered at 1.32 vs 2.09 fan volume); subdividing the pole line at
  the chord budget restores full refinement. Full-circle patch rims
  densify with BITWISE the adjacent cap's `circle_frame` samples (the
  cylinder-lateral recipe) — shared-rim watertightness by construction.
  Boolean re-entry: `to_yang` emits the pristine closed sphere as the
  PR-YR12 fixture (2 pole verts + 1 seam Circle, start=south); a
  boolean-OUTPUT sphere patch re-entering a second boolean stays a
  typed `UnsupportedCurvedBoolean` wall (later slice, same shipping
  order as the torus); `from_yang` gained `FaceSurf::Sphere` (+ sphere
  in the full-circle-edge sense derivation — a cap-cut sphere shares
  its rim with a planar cap). Stages 1–5 needed NOTHING (PR-YR12/YR15
  were already sphere-capable). Red→green: `kv6d_sphere_revolve.rs`
  (census, ball volume 4/3·π·r³, watertight, determinism, partial-angle
  on-axis and crossing rejections unchanged, and an equatorial half-cut
  boolean e2e — exactly half the ball, the wrapping pole-cap render
  path); yang `kv6d_sphere_patch.rs` (hemisphere coverage + boundary
  watertightness + bit-exact boundary passthrough; complement-loop
  rejection). **Corpus: C0067 UNSUPPORTED(revolve) → typed ERROR — the
  notch-corner {sphere, wall, wall} triple junctions hit the Stage-4
  `LocalRefinementRequired` wall (the N2 conic-junction class, F0059
  family). Assay 238 CORRECT / 0 WRONG / 50 ERROR / 7 UNSUPPORTED /
  0 TIMEOUT — zero-lost (C0067 is the only category mover).**

- **Tangency pinch-vertex split + figure-eight wedge walk (the KV9-F1
  union follow-up). ✅ SHIPPED at yang unit level (2026-07-08, task #86,
  spec `specs/yang_tangency_pinch_split.md`); C0058 corpus residual
  DIAGNOSED and banked.** Two mechanisms landed: (1)
  `split_pinch_vertices` at the Stage-4 exit — a mesh vertex whose star is
  ≥ 2 edge-connected CLOSED fans (the weld of a tangency pinch) splits
  into one vertex per sheet (identical position bits, relocation tags
  duplicated), riding the §4.5.3 Phase-A recompute path. Red-phase
  discovery (Test Author): the SYMMETRIC weld read χ=0 and SILENTLY
  passed the even-χ shell gate as genus-1 — a pinched sphere masquerading
  as a torus — so this fixes a silent-wrong-topology class, not just the
  corpus path's loud χ=1 asymmetric weld. (2) The `patch_boundary_cycle`
  figure-eight WEDGE WALK: at a boundary vertex with out-degree > 1 (a
  patch pinched at a mesh-manifold vertex), the continuation is the
  wedge-consistent edge found by rotating the patch's triangle fan from
  the incoming edge's owning triangle — naive lowest-first could chain
  two lobes into one self-crossing cycle. Byte-identical at out-degree-1
  vertices. Direct 30°/C0058-geometry unions now complete end-to-end with
  χ=2, per-sheet coincident vertex pairs at the exact tangencies, and
  band-exact Steinmetz-family volumes (`tests/tangency_pinch_split.rs`).
  Adversary: split-disable and both guard mutations caught (two needed
  new in-src unit tests — open-fan no-split pin, bowtie two-cycle walk
  pin); noted guard-then-assume coupling on the closed-fan precondition
  (pinned). **C0058 corpus-path residual (still ERROR, honest; diagnosis
  REFINED by probes):** on the kernel-v2 tessellation route there is NO
  mesh vertex at the tangency at all (`YANG_S4_TWIN_PROBE` finds zero
  sub-1e-9·scale twins at Stage-4 exit) — the two removed lobes connect
  through a RESOLUTION-SCALE NECK the mesh cannot separate, so the kept
  face's boundary is genuinely ONE 64-edge figure-eight cycle whose
  Newell cancels; the s6-curved-degenerate-loop E2 guard rejects it (and
  kernel-v2's unrolled-winding rules would wall the same loop next: net
  wrap 0 with zero area, or ±2). The honest fix is Yang §4.3.3
  TANGENT-POINT INSERTION — detect the collinear-normal tangent point
  during intersection optimization and insert it as an explicit
  arrangement point, splitting the neck exactly (the shipped pinch split
  + wedge walk then finish the topology). That is a paper-faithfulness
  milestone needing its own planned cycle (P10 stop recorded here).
  Sibling class also named: the PERPENDICULAR equal-R union welds
  tangency along a shared mesh EDGE (2 four-valent edges, Ok + χ=0
  today) — the EDGE-pinch analog, separate follow-up.

- **KV6b-F3 — plane∥axis × cylinder line case. ✅ RESOLVED (2026-06-12,
  PR-F3 + PR-F3b).** ssi-rs pair #2's C3a/C3b line branches were already
  correct; the defects were ALL in yang Stage 4: (1) `LineSegment`
  intersection edges got NO relocation (chord points stayed off the exact
  line and off the cylinder → `VertexOffSurface`), and Stage 4 wasn't even
  entered when lines were the only intersection curves; (2) a TRIPLE point
  shared by a line edge and a circle edge relocated onto the circle alone,
  off the cutting plane (the Newell failure); (3) the Line membership band
  lacked the propagated factor `r/√(r²−d²)` — the radial Stage-1 chord
  contract measured in the cutting plane's in-plane metric (the line analog
  of PR-YR19's sphere `(R/r_c)` scaling) → `AmbiguousCurve{2,0}` on the
  app's through-box geometry. Fixes: line relocation arm (exact line
  recomputed from cylinder+plane incidence via ssi-rs, same selection rule
  as Stage 3), line∩circle-plane junction relocation, and
  `line_band_amplification` carried in every gate sharing the metric
  (Stage-3 matching, Stage-4 re-matching, both relocation gates) with
  surface-normal backstops unscaled. NOT revolve-specific — plain sideways
  cylinder×box overlaps were broken too. Oracles:
  `kernel-v2/tests/kv6f3_line_ssi.rs` (exact segment-slab volumes, on-line
  vertex pins, the KV6b probe, the exact app geometry). App: revolve(270°)
  + through-box auto-union produces ONE merged body. Corpus 30/2/19
  unchanged (no corpus case exercises the geometry). Walls that remain:
  two DIFFERENT lines through one vertex (box corner ruling piercing a
  cylinder), line+ellipse/cone-conic junctions, near-tangent planes — all
  loud STOPs.

- **KV8/KV9 — gear profiles, cyl×cyl special cases, EllipseArc vocabulary.
  ✅ COMPLETE (2026-06-12, PR-KV8/KV8b/KV8c/KV9/KV9b).** Corpus
  34 → 42-class SUPPORTED_CORRECT (audit-driven autonomous session).
  - **KV8 gear profiles** (was 73 walled cases): `make_faces_from_profiles`
    consumes the authored `vertex_ids` polygon of spline-annotated profiles
    (the gear's canonical sampled boundary from `generate_gear_profile`;
    the SplineSegment entries are fitted-control-point annotations) — no
    new sampling introduced; exact shoelace-volume oracles. Non-convex
    booleans flow through yang NC1 CDT. Arc-segment profiles stay walled
    (their samples shadow representable exact cylinder walls).
  - **KV8b**: planarity tripwire made SCALE-RELATIVE
    (`1e-12·(1+max|coord|)` — the absolute form mis-flagged legitimate f64
    mapping rounding at world scale ~70).
  - **KV8c**: assay oracles gain the EXACT-PAIRING fast path (bitwise edge
    closure is provably watertight; the quantization grid ALIASED distinct
    exact edges at gear density into false non-manifolds) and the
    resolution-relative degenerate-triangle rule (flat at the f32
    channel's resolution, height < 4 ulps of scale).
  - **KV9 parallel cyl×cyl** (F0041/42/43/45 class): ssi-rs's parallel
    branch (2 ruling lines) plumbed through Stage 3/4 — law-of-cosines
    gradient band `1/sin α`, combined owner chord budgets, Stage-4 line
    arm accepts (Cylinder, Cylinder); from_yang pass-1d Newell is
    arc-midpoint-augmented (crescent caps); KV7 recovery gains the
    minimum-loop-arity repair (lens caps).
  - **KV9b EllipseArc vocabulary** (was the 8-case named Ellipse wall):
    the oblique plane × cylinder section end-to-end — arena curve,
    classification (parametric minor side), twin rules, validation,
    twin-canonical sampling, patch unrolling (parameter = azimuth for
    cylinder sections), exact closed forms (elliptical segment
    `ab(Δt−sinΔt)`; sinusoid Green's flux verified against the circle
    special case). Oblique sections re-enter nothing (typed wall stays).
    Stage-3/4 cyl×cyl ELLIPSE plumbing (per-point gradient bands,
    tangent-direction discrimination, ellipse×ellipse junction relocation
    to (plane∩plane)∩cylinder with duplicate collapse).
  - **FINDINGS**: **KV9-F1** Steinmetz tangency-junction patch cycles
    disagree (equal-radius perpendicular pair quarantined; Stage-3/4 hold).
    **KV9-F2** (with KV7-F1, one class): `tessellate_cylinder_patch` folds
    on ring-holed partial laterals (F0042) and mid-arc loop rotations —
    now caught LOUDLY by the fold tripwire at patch emit (winding vs
    outward radial); the unrolled ear-clip/refinement needs a dedicated
    robustness cycle. ~15 gear cases ERROR on the 90s replay timeout
    (heavy exact arrangements — performance, not correctness).
  - Remaining top blockers by size (post-PR-KV11, corpus 58 CORRECT):
    KV6c/d revolve (~51), M8 coplanar (~44, of which 19 are the
    curved-face-in-pair sub-class), gear-replay timeouts (14, perf),
    geometric face resolution (5), NonManifoldOutput reassembly (5,
    incl. KV4-F1b F0022), patch-fold robustness (KV7-F1/KV9-F2, 2),
    KV4-F1c R0067 render tessellation. KV4-F1 RESOLVED (rational-ray
    fallback, deviation N21); ellipse-junction class RESOLVED (PR-KV11).

- **KV7 — boolean output curve recovery ("output curve tagging").
  ✅ COMPLETE (2026-06-12, PR-KV7).** Curved booleans are CHAINABLE: the
  former `UnsupportedCurvedBoolean` re-entry wall is gone. kernel-v2
  `recover.rs` rewrites yang outputs to B-Rep granularity before
  `from_yang_brep` pass 1, on the Yang-paper principle that output
  surfaces are exact (boundary = surface∩surface): chord runs between a
  cylinder face and a ⊥ plane retag onto the computed exact circle;
  valence-2 co-curve vertices fuse (T-vertex seg runs → single segs,
  circle chords → sub-π arcs / closed rims); 2-closed-rim cylinder faces
  with an azimuth-aligned anchor pair re-emit as the canonical
  `[rim, seam, rim, seam]` lateral (constructor vocabulary — `to_yang`
  unchanged). Supporting fixes: Stage-1 `all_line` inspects inner loops
  (seg-bounded faces with circle rings route to curved CDT);
  `stage4_chord_band = max(A,B)`; Stage-6 cylinder-face AXIAL tie-break
  (two faces of one infinite cylinder — drill stubs); general planar
  tessellation accepts full-circle edges; `tessellate_cylinder_lateral`
  rows sampled bitwise-in-the-cap-frame (watertight by construction).
  NEW typed wall `UnsupportedMultiShellBoolean` (voids / disjoint shells
  cannot re-enter yang reassembly — previously shadowed by the curved
  wall). Corpus 30→33 SUPPORTED_CORRECT, 2→0 SUPPORTED_WRONG (R0029 +
  R0060 — the KV6b-F1 non-manifold T-junction class — FIXED by the
  collinear fusion; F0066 flips CORRECT). Chains proven by
  `kernel-v2/tests/kv7_output_curve_tagging.rs` + `boolean_chains.rs`
  (boss→pocket, hole→hole, boss→concentric-hole tube, 3-deep, exact
  volumes). FINDINGS: **KV7-F1** `tessellate_cylinder_patch` folds its
  unrolled triangulation when a partial-lateral loop starts mid-arc, at
  specific chord densities (reproduced on R0084's oblique revolve at rel
  tol 1e-3; recovery canonicalizes loop rotation to start at a seg,
  which avoids it — the patch path bug itself is unfixed). **KV7-F2**
  re-entry of bodies with internal voids is the next chain wall (yang
  BRep input has no shell structure; reassembly cannot rebuild voids —
  the XOR-class limitation).
  **KV7-F2 RESOLVED 2026-07-10 (spec `kv2_multishell_boolean_operands` +
  amendment 1): the multi-shell operand wall is REMOVED.** Multi-shell
  operands of every flavor — disjoint lumps (disjoint auto-unions /
  multi-region extrudes), interlocking lumps (the
  `split_solid_into_bodies` under-split shape, R0035), and INTERNAL VOIDS
  (fully-enclosed subtracts, C0071) — re-enter `boolean_op`.
  `to_yang_brep_indexed` always emitted every shell into one
  multi-component BRep; only the guard blocked it. The pipeline is
  component- and cavity-agnostic: Cherchi 2022 §2.4/§5 in/out labeling is
  ray-cast parity against each whole input mesh (a cavity-interior point
  crosses two boundaries → OUTSIDE), the Stage-4 Euler gate is per
  connected shell, and `from_yang_brep` + `face_components` already
  assemble multi-component outputs into multi-shell solids. The PR-KV7
  "reassembly cannot rebuild voids" claim was measured STALE
  (`KV2_MULTISHELL_PROBE` bypass: C0071's genuine void operand runs
  SUPPORTED_CORRECT with exact volume);
  `KernelV2Error::UnsupportedMultiShellBoolean` is deleted. Tests:
  `kernel-v2/tests/multishell_boolean_operands.rs` (exact-volume
  union/subtract/intersect on 2-lump operands, lump-consumed subtract,
  voided-box suite incl. a tunnel that OPENS the cavity and an
  intersect straddling the cavity wall); the kv6b re-entry pin flipped
  to the positive `revolve_boolean_voided_output_reenters_boolean`.
  Assay: the entire `UNSUPPORTED(multi-shell)` category (6 cases)
  clears — C0071/C0072/C0073/C0074 → SUPPORTED_CORRECT, R0035 → typed
  Stage-4 `LocalRefinementRequired` ERROR, R0076 → typed
  `InvalidBooleanOutput` edge-pairing ERROR (both join their honest
  shared error classes).

- **KV15 — per-vertex planar near-weld for MIXED operands. ✅ SHIPPED
  2026-07-10 (spec `kv15_mixed_operand_planar_near_weld`).** The
  edge-not-2-directed `InvalidBooleanOutput` class (6 chained-extrude
  cases) measured to one choke point: upstream ops mint planar femto
  twins (≤ ~3e-14; Stage-0 overlay `lift_or_snap` for the stacked-Z
  subfamily, non-boolean machinery for the off-axis subfamily), and the
  PR-KV10 near-weld that reconciles exactly this class was gated on the
  WHOLE model being planar — one circle/gear profile anywhere in the
  chain dropped it to bit-exact, leaving a femto membrane (measured:
  odd 3-triangle edge uses) that makes Stage-6 patch walks disagree
  (chord vs twin-stopover chain; the §4B T-subdivision can't repair it —
  the twin projects AT the chord endpoint, t≈1). Fix: eligibility is now
  PER VERTEX — near-weld (same band `TAU_WORK·(1+scale)`, same grid,
  min-index survivor) unions only vertices whose every incident
  arrangement triangle positively proves planar descent via `la.source`
  + `tri_face`; curved-adjacent / empty-provenance / sentinel vertices
  keep bit-exact (kv9 junction-duplicate protection, sidecar producer
  unchanged; all-planar branch byte-identical). Tests:
  `edge_pairing_twin_weld_campaign` corpus trackers (F0070/F0081
  RED→GREEN) + `kv15_*` unit branch coverage, mutation-checked.
  **Assay 224 CORRECT / 0 WRONG / 52 ERROR / 17 UNSUPPORTED / 0 TIMEOUT**
  (was 221/0/55/17/0), zero-lost — F0070/F0076/F0081 ERROR→CORRECT, no
  other movement. **Residue census (named follow-ups):** (1) KV15b —
  R0076's twins arrive in the chained input at ~3.9e-8: genuinely
  distinct near-parallel crossings, sub-floor but 8 orders above the
  representability band; welding at the feature floor is the
  reverted-R0091 hazard, so the fix belongs at the MINTING boolean
  (A14.2 collapse at emission); tracker quarantined `#[ignore =
  "KV15b …"]`. (2) F0079 (+ sites in F0083/F0084) — the residual
  edge-pairing sites are NOT twins: three single-use edges over
  COLLINEAR vertices at real scale (chord on one face vs 2-edge chain
  on the neighbors) — the `yang_stage6_sliver_topology` §4B
  T-subdivision domain (its `had_fold_sliver` gate / exact-betweenness
  doesn't reach these sites).

- **KV15b — sub-resolution intersection-segment collapse at boolean
  emission. ✅ SHIPPED 2026-07-10 (spec
  `kv15b_mint_site_subresolution_collapse`).** The KV15 residue (1)
  measured to its mint site: R0076's gear-cut subtract is ALL-PLANAR, so
  `has_conic` is false and the Stage-4 §4.4.1(b) merge — the only
  existing collapse pass — never runs; the exact arrangement's two
  near-parallel crossings (gear flank grazing a box edge; measured
  `KV2_SUBFLOOR_TWIN_PROBE` pairs at 3.999e-8/6.472e-8, edge-connected)
  are emitted verbatim and poison the next union (Stage-6 patch walks
  disagree, kernel-v2 edge pairing rejects). Fix: one pass in
  `reconstruct_topology_stage4` before Phase-B emission, on EVERY path —
  collapse intersection segments (keys of `intersection_curves`, full
  provenance) whose resolved length is in (0, `TAU_MODEL`), min-index
  survivor keeping its own bits, single sweep (no chain drift), then
  compact + Phase-A recompute (the §4.5.3 machinery). `TAU_MODEL` (1e-7,
  the central A8.1/A14 vertex-merge resolution — NOT the 10×-coarser
  `MIN_FEATURE_SIZE` floor of the reverted-R0091 hazard) is forced by
  consistency: the Stage-0 clustering band floor downstream is exactly
  `TAU_MODEL`, so an emitted sub-`TAU_MODEL` pair is guaranteed to weld
  into a degenerate loop at the next coplanar op. Tests: tracker
  `r0076_no_edge_pairing_wall` un-quarantined RED→GREEN + 5 `kv15b_*`
  unit branch tests, mutation-checked (band widening and gate drop each
  kill their test). **PLAN-CORRECTION RECORD (mini-P10): the roadmap's
  "KV15b class" was FOUR cases; measurement splits it.** New probe
  `KV2_SUBFLOOR_TWIN_PROBE` (adapter-level sub-floor twin census after
  every op) shows R0007/R0071's sub-TAU pairs are PROFILE-CONGENITAL —
  emitted by the extrude/revolve constructors from micro-scale gear
  profiles (96/36 pairs at bit-identical spacing 7.790e-8/9.460e-8
  BEFORE any boolean; the models also carry hundreds of LEGITIMATE
  sub-`MIN_FEATURE_SIZE` profile features, so any absolute-floor weld
  stays dead) — and R0053's twins are sub-representable overlay mints
  inside its failing op (zero sub-floor pairs in any chained B-Rep).
  Neither is a boolean-emission miss; both need their own cycles
  (profile-ingestion hygiene vs overlay mint-site collapse). **Assay 229
  CORRECT / 0 WRONG / 49 ERROR / 15 UNSUPPORTED / 0 TIMEOUT** (was
  226/0/53/15/0), zero-lost — R0076 + F0078 + R0088 ERROR→CORRECT (the
  same emitted-twin mechanism sat in all three chains) and F0084
  ERROR→typed UNSUPPORTED(coplanar-boolean) (its chain now clears the
  twin wall and reaches the M8 boundary).

- **S3 ellipse-rim chord bound — the `AmbiguousCurve {0,0}` producer
  fault. ✅ SHIPPED 2026-07-10 (spec `yang_s3_ellipse_rim_chord_bound`).**
  2026-07-10 census of the 8-case Stage-3 `AmbiguousCurve` ERROR class
  (probe `YANG_S3_AMBIG_PROBE`, both the selector and the producer-fault
  arms): the `{candidates: 0, matched: 0}` trio F0082/F0083/F0085 (5
  fault sites, ALL in `chord_tol_for_curved_owner`'s cylinder arm) is a
  re-entering body whose single cylinder face carries ONLY ellipse rims
  (oblique plane∩cylinder trims from a prior boolean, KV14 vocabulary) —
  zero `Curve::Circle` edges, so the Circle-rim-AABB bound lookup
  returned `None` and blamed the producer. Fix: the Stage-1 ellipse
  chain bound (`d_ε = 1e-2·major_radius`) factored into ONE source
  (`ellipse_chord_bound`, consumed by the KV14 pre-pass AND a new
  Stage-3 fallback `ellipse_rim_chord_bound` = max over the owner's
  ellipse edges); owners with neither rim keep the loud fault. Trackers
  `s3_ellipse_rim_chord_bound.rs` (3 cases) RED→GREEN + 2 unit tests,
  mutation-checked (minor-radius swap kills the max test).
  **Amendment 1 (same session): Stage-4's `input_curved_chord_bound`
  had the IDENTICAL Circle-only gap** — with Stage-3 fixed the trio hit
  the relocation ENTRY producer fault (`vertex u32::MAX` LRR; probe
  `YANG_S4_MERGE_PROBE` proved the §4.4.1(b) merge budget guard never
  fires). Fix = the same fallback composed fallback-only
  (`curved_chord_bound(..).or_else(ellipse_rim_chord_bound)`),
  byte-identical whenever a Circle rim or sphere exists; tracker
  extension `*_no_stage4_band_fault` RED→GREEN. Net: the ellipse-rim
  booleans now COMPLETE — the trio's remaining failures are their known
  downstream classes (F0083 Extrude-11 edge-not-2-directed §4B site,
  F0082 reassembled-non-2-manifold, F0085 late-chain CDT re-entry),
  failing-op counts 3→2 / 3→2 / 3→1. **Census of the remaining
  AmbiguousCurve sub-classes (named follow-ups):** (1) C0043/C0056 —
  INTERNALLY TANGENT cylinder pairs (0.6+0.4 = 1.0, 0.5+0.5 = 1.0):
  exact surfaces touch along one generator, the chordal meshes cross in
  a wide band → phantom intersection edges far off the tangent line;
  this is the paper's §4.3.3 Case-IV/tangent-point machinery (C0058's
  wall — the tangency campaign's standing follow-up). (2) R0026 —
  near-tangent plane∥axis × cylinder secant: both parallel-generator
  candidates exist but BOTH endpoints sit outside the amplified band
  (sub-band chord sag; the R0072 position tie-break needs matched ≥ 1
  and never engages). (3) R0003/R0008 — cone∩plane conic selection at
  extreme cones (R0008 half-angle 88.95° blows `cone_chord_bound` to
  3e1; R0003 one endpoint beyond the band) — the M5-corrected
  "cone∩plane conic + Stage-4 N2" family. **Assay 229 CORRECT / 0
  WRONG / 49 ERROR / 15 UNSUPPORTED / 0 TIMEOUT — unchanged totals,
  zero-lost (with amendment 1 the trio moved WITHIN ERROR from the
  Stage-3 fault through the Stage-4 entry fault to their late-chain
  downstream walls). The LRR class stands at 14 genuine
  relocation-region cases — the N2 mesh-updating epic remains the top
  target.**
  > **AMENDMENT (N52, task #167, 2026-07-15):** the "14 relocation-region
  > cases / mesh-updating epic is the top target" framing is INACCURATE — a
  > per-case reject-site census (deviations N52, probe `YANG_LRR_PROBE`)
  > shows the cluster is a heterogeneous tail: M5 torus∩torus (R0044/R0096),
  > #137 near-tangency torus∩plane (C0065/R0074), an unclaimed-conic-endpoint
  > vocabulary gap (R0038), `InvalidBooleanOutput`/`AmbiguousCurve` cases that
  > never reach a relocation region, and only R0003 in a conic band gate (and
  > it is a multi-map over-band chain, not a §4.5.1 region). Wiring the Fig-11
  > mesh-update would clear only a small subset, not 14. Attack the census's
  > root classes per-case (M5 / #137 first), not the mesh-update as a cluster
  > cure.

- **S7 — the certainly-fatal chord split (§4B follow-up). ✅ SHIPPED
  2026-07-10 (spec `yang_stage6_sliver_topology` amendment 1 + 1a).**
  The F0079-class edge-not-2-directed residue (KV15 census follow-up 2)
  measured at its site (`[s6-split-probe]`): a loop walks a spur+chord
  over a vertex that is f64-collinear (perpendicular distance 0.0) but
  sub-ULP OFF the exact segment, on a patch with NO fold sliver — both
  the §4B eligibility gate and the exact collinearity test miss it, and
  the chord's undirected use-count of 1 makes kernel-v2's rejection
  CERTAIN. Fix: a second split arm in
  `subdivide_loops_at_shared_vertices` on EVERY patch, gated on
  certain-fatality (use(a,b)==1 AND both complementary sub-segments
  walked AND v within TAU_WORK of the open segment, 0<t<1) — it can
  never alter a passing output (valid outputs use every segment exactly
  twice), so benign-T-junction reference parity is preserved
  structurally. Amendment 1a: the split alone left χ odd (the spur
  became a zero-width slit); S7 finishes by cancelling adjacent inverse
  pairs with a split-inserted member (null-excursion removal, fixpoint).
  Tracker `f0079_no_edge_pairing_wall` RED→GREEN; 4 `s7_*` unit tests,
  mutation-checked (gate drop caught after the benign fixture was
  strengthened to isolate the use==1 gate). **Assay 231 CORRECT / 0
  WRONG / 47 ERROR / 15 UNSUPPORTED / 0 TIMEOUT** (was 229/0/49/15/0),
  zero-lost: **F0079 AND F0083 ERROR→SUPPORTED_CORRECT** (F0083's
  residual §4B sites were this same class — combined with the same-day
  ellipse-rim chord-bound fixes its 12-op chain now completes
  end-to-end). C0075 measured NOT this class (six unpaired vertical
  seam segments, no on-segment vertex) — stays in the
  edge-not-2-directed residue with F0084's sites.

- **KV6a — revolve (kernel-v2). ✅ COMPLETE (2026-06-11, PR-KV6a).**
  Partial angles (0,2π) AND full 360° for polygon profiles with
  axis-parallel/perpendicular edges, axis in-plane, profile strictly one
  side. Partial output = the KV5b partial-patch vocabulary (sweep arcs,
  reversed inner-bore cylinder patches, arc-bounded annular sectors); 360°
  = the genus-1 washer (annular ring caps + canonical cylinders, incl. the
  first REVERSED canonical lateral — validate/tessellate/signed_volume
  vocabulary extended with mirrored rim rules, arc-loop closed forms
  (Green's theorem over the unrolled boundary; Pappus at 1e-12, washer
  bitwise 9π), and annular-cap strips). Adapter maps degrees→radians;
  axis-through-profile is INVALID INPUT (KernelError::Other — the
  F0073/F0074 expected-rebuild-error path), capability walls stay
  NotSupported. Corpus 28→30 SUPPORTED_CORRECT, UNSUPPORTED(revolve)
  38→26 (rect-revolves now stop at the KV6b boolean wall, label shifts to
  curved-profile). GUI: all 4 revolve spec files + the revolve
  input-validation block un-quarantined and GREEN (46 tests) after fixing
  their March-2026 staleness (the UX overhaul made axis-picking mandatory;
  specs now set the axis via the `__waffle.setRevolveAxis` test API with
  the engine's plane basis). Walls for later: KV6b (yang Stage-1 ingestion
  of partial patches + holed caps — unwalls boss/cut revolve corpus bulk),
  KV6c cones (oblique edges), KV6d torus (circle profiles), on-axis
  profiles (solid-of-revolution without bore — common CAD flow, currently
  rejected as touching).

**Where the risk lives:** almost all of it is Phase 2 (curved Stage 3/4). Phase 1
is a steady low-risk grind; Phases 4/6 are large but mechanical once geometry
works; Phase 5 is a contained predicates problem with a ready oracle. **Scale:**
multi-month, not a few sessions.

## 5. Risks & decisions

- **Coplanar multi-attribution** (S1): the `LabeledArrangement` source must be a
  list, and in/out a per-input vector. Locked in §2.
- **Stage-1 cleanliness is the true gate** (S2): M1 before M3. A label producer
  cannot mask inputs that violate Cherchi's axioms.
- **Substitutes retained, not deleted** (S3): M4.
- **`cherchi-rs` layering amended** (S5): its `CLAUDE.md` Hard-rule #7 (dashu
  only) was amended to permit a *temporary, feature-gated, non-WASM* dependency
  on `indirect-predicates-sidecar-rs`. **Amendment retired at M7c** — the
  clean-room predicates restored the pure-Rust/WASM end state; the FFI is a
  dev-dependency oracle only.
- **Dockerfile stays thin** (build caution): do **not** add a `RUN make` layer —
  it costs ~22 min and ~8 GB per image rebuild. Install only build prerequisites
  (cmake/clang are already present) and run `scripts/build_sidecars.sh` at
  container-create / first-test, with `CHERCHI2022_BIN` / `INDIRECT_PREDICATES_SRC`
  env defaults.

## 6. PR granularity for the arrangement

The micro RED→GREEN PR style (15–50 LOC) suited isolated predicates but cannot
meaningfully slice a graph algorithm. For Stage 2, use **vertical,
behavior-tested slices** where **GREEN ::= "matches the sidecar oracle on corpus
subset N"**, not "compiles + unit test". The oracle is what makes large slices
safe; this is why M0 (operationalize parity) comes first.

## 7. Doc-edit ledger

This re-charting touched:

| File | Change |
|---|---|
| `docs/yang_functional_roadmap.md` | this file (new SSOT) |
| `CLAUDE.md` | rewrote stale "Current architecture" block; re-sequenced phase tracker & priorities; PLAN.md note |
| `crates/cherchi-rs/CLAUDE.md` | Hard-rule #7 amended (interim IP-FFI dep); Stage-2 = `LabeledArrangement` producer |
| `crates/cherchi-sidecar-rs/CLAUDE.md` | elevated to interim `LabeledArrangement` producer + label-emission mission |
| `crates/yang-rs/CLAUDE.md` | `LabeledArrangement` consumption; retain substitutes as test oracle |
| `crates/indirect-predicates-sidecar-rs/CLAUDE.md` | predicates demand-driven; clean-room/WASM end-state |
| `docs/yang_deviations.md` | appended interim labels-from-sidecar deviation |
| `Dockerfile` + `scripts/build_sidecars.sh` | operationalize parity (thin image + build script) |
| `docs/prototype_release_roadmap.md` | **NEW** — epic doc for the planetary-gearbox / prototype-release arc; sequences kernel (KV12/KV13) + app/UX + tooling phases toward the 3D-print acceptance test |
| §4 KV12 / KV13 (this file) | added planned stubs — arc-segment profile extrude (gears) and provenance/topological naming — driven by the prototype-release roadmap |
| `projects/{04-3d-viewport,08-ui-chrome,13-dev-infrastructure}/PLAN.md` | pointer line → active forward work tracked in `prototype_release_roadmap.md` (those PLANs are accurate as-built snapshots, all milestones closed) |

## 8. Deviations from Yang 2025

The interim path takes Stage-2 labels from the C++ sidecar rather than from a
native arrangement, as the paper assumes. This is a tracked deviation — see
`docs/yang_deviations.md`. It is resolved: M6 (native Stage 2) and M7 (WASM)
are both complete.
