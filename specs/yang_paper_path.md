# Yang paper path — in place, or a second path

**Status: DECIDED — in place, in roadmap order** (owner, 2026-11-16: "in
place, go ahead"). The outer layer of substitutes is retired in place on the
owner's ruling of 2026-11-13 that a small, loud drop in the corpus score is
acceptable when what was removed was not faithful to the paper (§6). The core
converts in place too, in the roadmap's order F5a → F2 → I3b → F3 → …, each
increment held to the corpus less its named dips (§6, "The core"). The
roadmap's hold on wiring new increments into the production Stage 4 is
lifted. This spec holds the question, one experiment (2026-11-12) and what it
settles, and the reasons for the choice.

## 1. The question

The kernel reaches the paper's mechanisms through substitutes that grew one
corpus case at a time (`specs/yang_faithful_cleanup.md` §2). The roadmap offers
two ways to remove them:

- **in place** — one wired increment at a time on the production path, the
  corpus held at every commit;
- **a second path** — the paper's order assembled from existing primitives
  behind one switch, the production path as its differential oracle, and the
  corpus required at the flip.

The case for a second path is that the substitutes are tuned to each other, so
that removing one breaks cases another protects. The experiment below measures
how true that is for the outer layer of substitutes.

## 2. The experiment

Branch `exp/paper-skeleton` adds one environment variable, `YANG_SKELETON`,
that switches named mechanisms off in release builds
(`crates/yang-rs/src/skeleton.rs`). With the variable unset every gate reads
false and the code path is `main`'s. The branch (at `95d9089f`, on `723c0283`)
is never merged.

| Switch | What it turns off | Ledger | Source |
|---|---|---|---|
| `s1.rim` | `phantom_min_rim_segments`, the operand-wide rim boost | cleanup §2a | `boolean.rs` |
| `s1.junction` | `rim_junction_overrides`, rim-junction insertion | N82 | `boolean/rim_junction.rs` (957 lines) |
| `s1.p3a` | `junction_stage1_overrides`, the edge × face pierce mints | N82 | `boolean/junction.rs` (1,424) |
| `s1.eip` | `edge_in_plane::conformity_overrides`, §4.5.5 one dimension down, arm 2 | N64 | `stage0/edge_in_plane.rs` (822, both arms) |
| `s1fold` | the cylinder-pair phantom fold and the thin-band rim fold | N88 | `stage1_tessellate.rs` |
| `s4` | fold-merge / absorb / rehome, `collapse_subtauwork_mesh_edges` (both sites), `repair_fan_cluster`, the late pinch-vertex split | cleanup §2b (N2, N58) | `stage5_topology/fold_merge.rs` (2,577), `stage4_correct/collapse.rs` |
| `transit` | `solve_crease_transit` and `corner_transit_apply` | N87 | `stage4_transit.rs` (2,551), `stage4_correct/corner_transit.rs` (1,950) |

`s1` names the four `s1.*` switches together; `all` names every row. The line
counts are whole files and so an upper bound on what each switch removes.

**Left on**, and so not measured: the construct pass (`run_construct_passes`),
`update_patches`, the §4.3 dedup collapses, `boolean::refine_loop`, all of
Stage 0 including its relocation (`stage0/reloc.rs`) and edge-in-plane arm 1,
Stage 1's self-contact guard, chart refine rounds and planar-neighbour chord
split, the I2d d(T) refusal, and the classify-before-update order.

**Measurement.** The categorized corpus (374 cases, release, 900 s CPU budget
per case), once per configuration. Two readings per run:

- the five-bucket score, and which cases left SUPPORTED_CORRECT;
- the per-case canonical B-Rep hash (`WAFFLE_GEOMETRY_HASH_OUT`), compared with
  the control run. A case whose hash differs has a different output, bit for
  bit, whether or not its verdict moved.

`YANG_SKELETON_LOG` records, once per case, each gate that was reached, and
with the switch unset whether each transit acted.

## 3. Results

Every run: 0 SUPPORTED_WRONG, 5 EXPECTED_ERROR. The control run reproduces the
committed baseline case for case (360 C / 0 W / 9 E / 5 EE / 0 T).

| Switched off | Correct | Cases lost (CORRECT → ERROR) | Outputs changed | Gate reached in |
|---|---|---|---|---|
| nothing (control) | 360 | — | — | — |
| `transit` | 360 | none | 0 | 11 (crease), 179 (corner) |
| `s1.eip` | 360 | none | 0 | 208 |
| `s1.rim` | 360 | none | 8 | 317 |
| `s1fold` | 360 | none | 13 | — |
| `s1.junction` | 359 | F0059 | 27 | 222 |
| `s1.p3a` | 358 | C0058, P0029 | 119 | 208 |
| `s1` (the four together) | 357 | C0058, F0059, P0029 | 146 | — |
| `s4` | 356 | F0058, F0060, P0020, P0057 | 4, and F0064's error text | — |
| `s1,s4,transit` | 354 | C0058, F0058, F0059, F0060, P0029, P0057 | not hashed | — |
| `all` | 354 | the same six (P0020 survives) | 153 | — |

"Outputs changed" counts the lost cases too. "Gate reached" is the number of
cases in which the switched-off call site was executed; it bounds how many
cases the mechanism could have acted on, and does not say it acted.

**How the seven fail.** All are typed errors.

| Case | Group | Error |
|---|---|---|
| C0058 | `s1.p3a` | yang-rs: reassembled output would be non-2-manifold |
| P0029 | `s1.p3a` | kernel-v2 `SelfIntersectingBooleanOutput { penetrations: 5 }` |
| F0059 | `s1.junction` | yang-rs §4.5.2 loop ended with 4 `IllegalIntersection` sites; §4.4.1 update refused a patch |
| F0058 | `s4` | kernel-v2 `NonManifoldVertex` |
| F0060 | `s4` | yang-rs: reassembled output would be non-2-manifold |
| P0057 | `s4` | yang-rs: reassembled output would be non-2-manifold |
| P0020 | `s4` alone | yang-rs: reassembled output would be non-2-manifold |

P0029 and F0058 are refused by kernel-v2's validation of the finished body,
not by a yang-rs stop: yang-rs returned a body it believed good.

**The one interaction.** P0020 is lost with `s4` off and CORRECT again when
`s1` is off as well. The Stage-4/5 repairs there mend damage a Stage-1 arm
causes. Apart from it the losses add: three for `s1`, four for `s4`, six
together.

**The `s1` split is exact.** The union of the cases its sub-switches change
is the 146 cases `s1` changes. `s1.rim` shares no case with the other two;
`s1.junction` and `s1.p3a` share eight. With `s1.junction` off, the P3a mints
run on the pairs where rim-junction insertion would have pre-empted them, so
that row measures the replacement, not a bare absence.

**Planar-only cases.** 111 of the 374 cases have no curved face (107 CORRECT,
the three planar C-series walls, one expected error). They skip Stage 4, and of
the switches only `s1.p3a` changes any of them: 49 outputs, none lost. One was inspected (C0001, a plate with two rectangular through-holes):
same 14 faces, same 52 triangles, same volume to thirteen digits; the
difference is in the low bits of coordinates.

**The transits act, and the result does not depend on it.** With the switch
unset, the crease transit is attempted in 11 cases and returns a junction in 8
(F0082, P0040, P0050, P0053, R0003, R0019, R0044, R0095), and the corner
transit changes the mesh in 3 (R0011, R0044, R0074). With the switch set, all
ten cases produce the same B-Rep, bit for bit. Two were probed
(`YANG_452_PROBE`). In P0040 and in R0011 the run at natural density fails
with or without the transit and the §4.5.2 loop takes over; in R0011 the
transit removes one failure of eleven from a run the loop then replaces. That
is where the paper sends boundary and corner points (N87).

## 4. What this shows

1. **No silent wrong result.** With every listed substitute off, the corpus
   has no SUPPORTED_WRONG. A missing substitute costs a loud error.
2. **The outer layer is not tightly coupled.** Removing all of it at once
   (files totalling about ten thousand lines) loses six of 360 cases, and the losses are the
   sum of the per-group losses less one. The claim that the substitutes
   protect each other's cases holds for P0020 and for no other corpus case.
3. **Four switches have no corpus customer.** The transits and edge-in-plane
   arm 2 change no output at all; the ten cases where a transit acts are
   answered by the §4.5.2 loop either way. The rim boost and the cross-patch
   folds change 8 and 13 outputs and every one stays CORRECT. No corpus
   customer is not no customer: arm 2 has one the corpus does not hold (§5).
4. **Seven cases are the customers of the rest**: F0059 of rim-junction
   insertion; C0058 and P0029 of the P3a mints; F0058, F0060, P0020 and P0057
   of the Stage-4/5 repairs. Every other corpus case passes without them.

## 5. What this does not show

- **Nothing about the core.** The construct pass, Stage-0 relocation, Stage-1
  meshing, the d(T) refusal, the classify-before-update order and the
  3D-points representation were on throughout. The in-place trials that
  stopped on regressions (N85's d_ε scale, the I3b classify flip, the N106
  refusal retirement) are all there.
- **The corpus is a sample.** For the four customer-less switches a
  prospector seed and the fast tier were run afterwards (§6 item 1). For
  the other three groups (`s1.junction`, `s1.p3a`, `s4`) neither has been.
- **All three checks hold each case in one orientation, and that hid arm 2's
  customer.** The corpus, the seed and the fast tier agreed that edge-in-plane
  arm 2 does nothing. Its anchoring pair (P0001's octagon and star) moved
  rigidly says otherwise: with arm 2 off, 68 of 640 motions stop with
  `NonManifoldOutput`; with it on, none (ledger N64, pin
  `s455_rigid_motions_all_union_at_the_control_volume`). P0001 is
  axis-aligned, where a float holds the identified edge exactly; arm 2 is for
  the planes where it cannot. A mechanism built against rounding is tested by
  moving its anchoring case, and that check precedes a removal (§6).
- **A changed output that stays CORRECT has passed the oracles, no more.**
  The 153 changed cases were not compared for quality.
- **`s4` was not split.** Which of its four passes owns each of its four
  cases is not measured.

## 6. The outer layer: in place. The core: open

**Owner ruling, 2026-11-13.** "If there's something that's not faithful to
the paper or is otherwise not a good implementation and it lowers the score
a little bit, that's OK." The roadmap records it as the *named dip* (§1
item 4): a removal may turn a few cases into typed ERRORs, each named against
the milestone that answers it; never a WRONG. The owner did not say "in
place"; that the ruling settles the outer layer is this spec's reading, since
the dip was the one thing §3's data left for the owner to decide there.

**The outer layer does not need a second path.** It is retired in place,
in the order §3 gives:

1. The switches with no corpus customer, one per commit: the rim boost,
   the two cross-patch folds (N88) and the transits (N87, RESOLVED). **All
   three are deleted (2026-11-13).** Edge-in-plane arm 2 (N64) is off this
   list: it has a
   customer outside the corpus (§5) and stays, with its paper status an F7
   adjudication. Each removal is preceded by a rigid-motion sweep of the
   cases its mechanism acts on, switch on against switch off; a loss there
   is a named dip like any other. The two checks §5 listed as missing were
   run on 2026-11-13 with all four off together. Prospector seed 6: 188C /
   0W / 10E / 2T, the same verdict as the control on each of 200 candidates.
   Fast tier: three failures, all pins of the removed mechanisms — two
   Stage-1 fold pins, and one real loss, a coaxial tube with a 1e-5 wall
   that the rim boost carried and the §4.5.2 loop cannot clear at its
   ceiling (now a loud stop, customer of F3's Case-IV rule-out). The corpus
   is held throughout; where outputs change, the gate is the oracle verdict,
   since the hash cannot be. For the transits the rigid-motion sweep was
   the whole corpus: 360 CORRECT cases, four motions each, with and without
   them, 1,440 verdicts and none different. The same sweep, run late for
   the rim boost, names a dip the corpus could not show: C0117 stops at the
   §4.5.2 ceiling in 4 of 4 motions without the boost (F3 item I2's
   customer). The sweep is `crates/test-harness/tests/rigid_motion_sweep.rs`.
2. Rim-junction insertion, the P3a mints and the Stage-4/5 repairs, at a
   named dip of at most seven corpus cases, which become customers of F2
   (§4.4.1) and F3 (§4.5). The same checks come first — a seed, the fast
   tier and the rigid-motion sweep — and a case lost in any of them is
   named too. `s4` is split before it is retired, so that
   each of its four passes goes in its own commit with its own cases. Two of
   the seven (P0029, F0058) are stopped by kernel-v2's validation of the
   finished body and not by yang-rs; each gets a yang-rs stop that names the
   site before its mechanism goes, so the dip is loud at the stage that
   causes it.

**The core: in place, in roadmap order (owner, 2026-11-16).** This section
had proposed answering the core question the same way: put the three
paper-order changes that had each been tried alone (the I3b flip, the I2d
refusal off, N85's d_ε scale) behind the switch, run them together, and
count. That run is not cheap, and it would not answer the question:

- Two of the three trials were never committed. The I3b trial
  (2026-10-27/30) lived in a working tree; `agy/f1-i3-full-arrangement`
  holds only the unwired I3a primitives. Rebuilding it is the F1 I3 item
  itself: Stage 4 on the full arrangement with the keep, flip and membrane
  rule after it, the flood keyed by side label, and `full_arrangement`
  passed true through the construct, fold-merge and decimation passes that
  `reconstruct_topology_stage4` calls with `false`. The N85 trial
  (`specs/yang_41_deps_from_bbox.md` §5) was reverted; its wiring reaches
  49 call sites of the chord-bound functions, and the scalar ones (sphere,
  torus, cone, ellipse) are given no model to scale by. Only I2d is a
  one-line switch.
- Each trial's recorded losses already name their cause, and in every case
  it is a missing earlier milestone, not another core change:

  | Trial alone | Losses | Named against |
  |---|---|---|
  | N85, model-AABB d_ε (2026-10-27) | P0046, R0032, P0007, F0084 CORRECT → ERROR | F2 (§4.4.1 update, N2), F3 (§4.5.2 locality, N62; §4.5.4, N83) |
  | I3b, classify after update (2026-10-30) | the c0058 pin; F0060 WRONG in the smoke corpus | F2 (pre-CDT 2D relabelling, N2), N74/N78 |
  | I2d off (2026-11-06) | corpus held, 0 WRONG; two yang-mesh volume pins lose 20–23 % | F5b (Stage-1 interior samples, N5) |

The dependency the record shows is one of ORDER: each core change was tried
before the mechanism the paper puts in front of it. That is what the
roadmap's sequence encodes, and it is what an in-place conversion follows.
A combined run before F2 exists would re-measure "F2 is missing". The F0060
WRONG under I3b is the exception that would have argued for a second path
under the rule this section first wrote; it was recorded before F2 Steps
1–2 and N106 increment 2 landed, traces to a named F2 gap, and is re-read
when I3b is wired — a WRONG there still stops that increment.

So every core milestone lands in place, in order: F5a (result-neutral, gated
by the canonical hash), then F2, then I3b, then F3 and on. Each increment
holds the corpus less its named dips, runs the rigid-motion sweep of the
cases its mechanism touches, and never admits a WRONG.

## 7. Reproduce

```
git worktree add ~/wt/paper-skeleton exp/paper-skeleton
cd ~/wt/paper-skeleton
YANG_SKELETON=s4 \
YANG_SKELETON_LOG=/tmp/reach-s4.txt \
WAFFLE_GEOMETRY_HASH_OUT=/tmp/hash-s4.txt \
ASSAY_JOBS=12 ASSAY_CASE_TIMEOUT_SECS=900 \
  cargo test -p test-harness --test assay_kv2 --release \
  full_corpus_categorized -- --ignored --nocapture
```

Unset `YANG_SKELETON` for the control run. Diff the hash files line by line
after sorting; each line is `assay/<case> geom=<hash> errors=<hash>`. A corpus
run rewrites `app/tests/cases/assay/results.json`; restore it with
`git checkout` afterwards. The 900 s budget is CPU time, so a slower machine
times out the two longest cases (F0072, F0085); re-run those with
`ASSAY_CASE=<id> … single_case`. The three `s1.rim` / `s1.junction` /
`s1.p3a` runs here were made on sequoia-server and those two cases re-run on
the development machine; both are CORRECT under all three switches.
