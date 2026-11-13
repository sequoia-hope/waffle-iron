# Yang paper path — in place, or a second path

**Status:** for the core the choice is OPEN and is the owner's. For the outer
layer of substitutes it is made: they are retired in place (§6), on the
owner's ruling of 2026-11-13 that a small, loud drop in the corpus score is
acceptable when what was removed was not faithful to the paper. This spec
holds the question, one experiment (2026-11-12) and what it settles, and what
the F5a design session still has to measure. The roadmap's hold stays in
force for the core until §6 records that choice: no new increment is wired
into the production Stage 4 (`docs/yang_faithfulness_roadmap.md` §3). A
retirement is not an increment.

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
   folds change 8 and 13 outputs and every one stays CORRECT.
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

1. The four switches with no corpus customer, one per commit: the rim boost
   (**deleted 2026-11-13**), the cross-patch folds (N88), the transits (N87),
   and edge-in-plane arm 2 (N64, whose paper status is an F7 adjudication,
   so "retire" there means "ask"). The two checks §5 listed as missing were
   run on 2026-11-13 with all four off together. Prospector seed 6: 188C /
   0W / 10E / 2T, the same verdict as the control on each of 200 candidates.
   Fast tier: three failures, all pins of the removed mechanisms — two
   Stage-1 fold pins, and one real loss, a coaxial tube with a 1e-5 wall
   that the rim boost carried and the §4.5.2 loop cannot clear at its
   ceiling (now a loud stop, customer of F3's Case-IV rule-out). The corpus
   is held throughout; where outputs change, the gate is the oracle verdict,
   since the hash cannot be.
2. Rim-junction insertion, the P3a mints and the Stage-4/5 repairs, at a
   named dip of at most seven corpus cases, which become customers of F2
   (§4.4.1) and F3 (§4.5). The same two checks come first, and a seed case
   lost with them is named too. `s4` is split before it is retired, so that
   each of its four passes goes in its own commit with its own cases. Two of
   the seven (P0029, F0058) are stopped by kernel-v2's validation of the
   finished body and not by yang-rs; each gets a yang-rs stop that names the
   site before its mechanism goes, so the dip is loud at the stage that
   causes it.

**For the core the question is still open, and the same method answers it
cheaply.** Put the paper-order changes that have each been tried alone (the
I3b flip, the I2d refusal off, N85's d_ε scale) behind the same switch, run them together,
and count. If the losses add as they do here, convert in place against a named
customer list. If they compound, or a SUPPORTED_WRONG appears, build the
second path. That run is the first item of the F5a design session.

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
