# PR-Y15a — Phase 1 Validation

**Author:** adversary-2 (PR-Y15a Phase 1)
**Date:** 2026-05-04
**Spec:** `specs/yang_pr_y15a_downstream_investigation.md`
**Phase 0 diagnostic:** `docs/audits/pr_y15a_phase0_diagnostic.md`
**Stage Bb probe site:** `crates/kernel/src/boolean/topology_extract.rs:1810-1835`
(post-`label_cells`, pre-`face_survival_detect`)

## Verdict

**ACCEPT** — proceed to PR-Y15c. PR-Y15a Phase 0's row-4 attribution is
empirically airtight: all 10 cases in F0031–F0040 are homogeneous on
the conformal-probe axis, and the Stage Bb probe is mutation-confirmed
load-bearing on the row-attribution decision. The conformal-probe
family A/Bb/B/C is genuinely exhausted on this cohort.

**One important narrowing finding (memo §6):** the `[topo-extract]
summary:` instrumentation already in `topology_extract.rs` (gated on
`TWIN_DEBUG=1`) reports `paired=N, unpaired=0, ambiguous=0` on all 10
F0031–F0040 cases. **The half-edge graph is internally well-paired at
Step 7 immediately after construction.** This means PR-Y15c's
candidate-1 anchor (Step 7 half-edge construction) is **already
empirically refuted**; PR-Y15c's spec can collapse to a single Stage E
probe at `tessellate_waffle_solid` (render LOD), saving an investigation
half-cycle. This is the "cheaper proxy" team-lead asked about.

## §1. Decision-tree verdict per case

I re-ran the F0031–F0040 batch with `YANG_BOOLEAN=1
YANG_CONFORMAL_PROBE=1` and captured all 40 probe lines (4 stages × 10
cases) plus the per-case Waffle outcome. Verbatim summary:

| Case | Stage A | Stage Bb | Stage B | Stage C | Waffle outcome | Row |
|---|---|---|---|---|---|---|
| F0031 | well_formed=true | well_formed=true | well_formed=true | well_formed=true | Failed (12 unpaired edges out of 60) | **4** |
| F0032 | true | true | true | true | Failed (16 unpaired edges out of 44) | **4** |
| F0033 | true | true | true | true | Failed (16 unpaired edges out of 44) | **4** |
| F0034 | true | true | true | true | Failed (28 unpaired edges out of 62) | **4** |
| **F0035** | true | true | true | true | Failed (16 unpaired edges out of 44) | **4** |
| F0036 | true | true | true | true | Failed (16 unpaired edges out of 62, 8 reversed normals) | **4** |
| F0037 | true | true | true | true | Failed (12 unpaired out of 66, 14 reversed normals) | **4** |
| **F0038** | true | true | true | true | Failed (20 unpaired out of 70, 14 reversed normals) | **4** |
| F0039 | true | true | true | true | Failed (40 unpaired out of 86) | **4** |
| F0040 | true | true | true | true | Failed (20 unpaired out of 70, 14 reversed normals) | **4** |

**All 10 cases fire row 4 — homogeneous.** Spot-checks F0035 and F0038
(team-lead's requested expansion) match implementer-e's table exactly:
F0035 has verts=26 tris=44 (matches implementer-e §"Cluster homogeneity
verdict"); F0038 has verts=46 tris=84 (matches). The full 10-case
batch verifies homogeneity at 10/10 — far exceeds the requested 5/10
minimum.

The decision tree row 4 selector ("Stage A/Bb/B/C all true AND Waffle
still fails") is satisfied uniformly. Spec §4 row 4 mandate: "Spec a
new investigation PR-Y15c with a Stage D probe added immediately
before the half-edge validator."

## §2. Cluster homogeneity expansion verdict — 10/10 HOMOGENEOUS

| Case | verts | tris | unique_edges | All 4 stages well_formed |
|---|---:|---:|---:|---|
| F0031 | 28 | 48 | 72 | ✓ |
| F0032 | 26 | 44 | 66 | ✓ |
| F0033 | 26 | 44 | 66 | ✓ |
| F0034 | 30 | 52 | 78 | ✓ |
| **F0035** | 26 | 44 | 66 | ✓ |
| F0036 | 46 | 84 | 126 | ✓ |
| F0037 | 46 | 84 | 126 | ✓ |
| **F0038** | 46 | 84 | 126 | ✓ |
| F0039 | 42 | 76 | 114 | ✓ |
| F0040 | 46 | 84 | 126 | ✓ |

The 5+5 split (F0031–F0035 = box-minus-cyl; F0036–F0040 = cyl-minus-box)
shows distinct mesh sizes per group but identical conformal-probe
behavior. **Operand order does not matter on this axis.** PR-Y15c may
safely collapse to F0031 + F0040 (one from each operand-order group)
as a 2-case spot-validation set — a 5x reduction in spot-check cost
without losing coverage.

## §3. Mutation test result — Stage Bb is load-bearing on attribution

**Mutation:** Added `report.is_well_formed = false;` immediately after
the `check_conformal` call in Stage Bb (line 1832), forcing the
emitted line to report `well_formed=false` regardless of the actual
mesh topology. Re-built and re-ran F0031–F0040 batch.

**Result with mutation applied** (verbatim — first 4 lines from F0031):

```
[conformal-probe] stage=A  unpaired=0 multi_paired=0 euler_chi=4 well_formed=true  verts=28 tris=48 unique_edges=72
[conformal-probe] stage=Bb unpaired=0 multi_paired=0 euler_chi=4 well_formed=false verts=28 tris=48 unique_edges=72  ← FORCED
[conformal-probe] stage=B  unpaired=0 multi_paired=0 euler_chi=4 well_formed=true  verts=28 tris=48 unique_edges=72
[conformal-probe] stage=C  unpaired=0 multi_paired=0 euler_chi=4 well_formed=true  verts=28 tris=48 unique_edges=72
```

The Stage Bb line shows `well_formed=false` while the underlying
counts (`unpaired=0 multi_paired=0`) say it should be true. **The probe
is reading and emitting the value of `is_well_formed` — the field
flows through to the printed output.** Stage A/B/C are unchanged
(different probe sites with their own `check_conformal` calls).

If this mutation were left in place, every case in F0031–F0040 would
read as decision-tree row 2 (Stage A=true, Bb=false, B=true, C=true) —
which would route the anchor recommendation to "label_cells corrupting
the conformal mesh" instead of "downstream of Step C." **The Stage Bb
signal is therefore load-bearing on the row-attribution decision.**
This refutes any claim that Stage Bb is dead-code instrumentation.

**Mutation reverted.** Verified by re-running the F0031–F0040 batch
with no source modifications: all 40 probe lines back to the
implementer-e-documented all-true state. `git diff` against
implementer-e's commit is byte-clean.

## §4. PR-Y15c escalation recommendation — ACCEPT (with anchor narrowing)

### The conformal-probe family is genuinely exhausted on this cohort

I considered three "could there be additional probes inside the
existing window" alternatives before accepting the escalation:

1. **A probe at `face_survival_detect` mid-execution (between Stage Bb
   and Stage B):** Could reveal whether survival labeling drops or
   marks any specific tri unexpectedly. **Refuted by data:** F0031's
   Stage Bb (post-`label_cells`, FULL `tris_a + tris_b` = 48 tris) and
   Stage B (post-survival, also 48 tris from `survival.groups`) report
   IDENTICAL signatures — same vert/tri/edge counts, same
   `well_formed=true`. Survival is a no-op on conformality for this
   cohort. A mid-execution survival probe would surface no new
   information.

2. **A label-correctness check within Stage Bb:** the conformal probe
   measures triangle-mesh topology (V−E+F=χ) but NOT label semantic
   correctness. Could a label-validity check catch a F0031 defect?
   **Refuted by the `[yang-diag] after label_cells: A outside=12
   inside=0 cosurface=0, B outside=0 inside=36 cosurface=0` line** —
   F0031's labels look reasonable for a "box-minus-enclosed-cyl"
   operation: A has 12 outside (the box hull's 12 tris), B has 36
   inside (the cyl interior's 36 tris). A label check at this site
   would pass. The defect is geometric, not semantic.

3. **A probe at the `subdivide_mesh_pair` exit (before label_cells):**
   This site is already covered by Stage A. Adding a second probe
   there would not add information.

**Conformal-probe exhaustion is real.** No additional probe within the
A/Bb/B/C window will narrow the anchor.

### "Label correctness vs mesh topology" insight HOLDS

implementer-e's §"Spec ambiguities" item 1 is empirically defensible.
The conformal-probe family measures triangle-mesh topology only; if
`label_cells` produces semantically wrong labels (e.g., flipping
inside/outside selection), the conformal probe would still report
`well_formed=true` because the triangles themselves remain a closed
manifold — they would just SELECT the wrong subset. The label-
correctness defect class is genuinely a Stage D / Stage E concern, not
a Stage A/Bb/B/C concern.

For the F0031–F0040 cohort specifically, the labels look reasonable
(per `[yang-diag] after label_cells:` lines), so I don't expect this
to be the root cause — but ruling it out conclusively requires a probe
downstream of label consumption (i.e., Stage E at the render mesh).

## §5. Re-segmentation recommendation for PR-S2 TSV

implementer-e's §"Spec ambiguities" item 2 raises a valid concern: the
PR-Y15a spec §1's example failure mode (`half_edge[N].twin = 0 but
twin.twin = M`) was the F0002 / PR-Y14a anchor. The F0031–F0040
cohort's actual failure mode is `watertight_mesh: N unpaired edges out
of M total` — a distinct downstream-oracle failure, not a half-edge
self-consistency error.

**Recommendation: re-segment the PR-S2 TSV's "Waffle=Failed × Cherchi=valid"
cell (284 rows / 160 unique cases) by failure-mode signature** before
PR-Y15c finalizes its reproducer set. Specifically:

- **Sub-cohort A (watertight-oracle violators, F0031–F0040 type):**
  cases failing `watertight_mesh: N unpaired edges` and/or
  `mesh_euler_characteristic: V-E+F != expected`. PR-Y15c's primary
  cohort. Likely render-LOD retessellation defect.
- **Sub-cohort B (half-edge twin-pair violators, F0002 pre-PR-Y15b
  type):** cases failing `yang_boolean: result validation failed:
  half_edge[N].twin = 0 but twin.twin = M`. May still exist post-PR-Y15b
  in some R-cases (per PR-Y15b validation §1 — 44 of 51
  `combined_failures` rows persist; check those for half-edge errors).
- **Sub-cohort C (consistent-normals / outward-normals / orientation
  failures):** F0036–F0040 show `consistent_normals: 14 of 40 reversed`
  or `outward_normals: only 26 of 40 (65.0%)` in addition to
  watertight failures. Could be a separate winding-orientation defect
  class in the render LOD, OR could be a direct downstream consequence
  of the watertight defect; PR-Y15c's investigation should distinguish.

**This re-segmentation should be done by PR-Y15c's spec writer (NOT by
me, NOT by implementer-e)** — it requires combing through the
`results.json` `detail` field per case, which is a different artifact
than the PR-S2 TSV's `cherchi_detail` column. The two-shot prediction
ratification from PR-Y15b validation (asymmetric-defect insight) gives
us confidence the cohort IS heterogeneous on the failure-mode axis.

For PR-Y15c-Phase-0's reproducer set, F0031 + F0040 (operand-order
coverage) plus 1-2 sub-cohort-B cases (likely from R0058, R0063,
R0081, R0085 per PR-Y15b validation §2's residual `combined_failures`
list) should suffice. The full re-segmentation can wait for PR-Y15c's
spec writer to consume both.

## §6. Cheaper-proxy investigation — STEP 7 IS ALREADY PROVED INNOCENT

This is the most consequential finding of Phase 1.

**`crates/kernel/src/boolean/topology_extract.rs:1031-1048`** already
contains an unpaired-half-edge check at the END of `flood_fill_patches`
Step 7, gated on `TWIN_DEBUG=1`. I ran F0031–F0040 with that env var:

```
$ TWIN_DEBUG=1 cargo test -p test-harness --test assay_randomized \
      --release -- batch_enclosed_subtract_fix --ignored --nocapture
[topo-extract] summary: paired=30, unpaired=0, ambiguous=0   (F0031)
[topo-extract] summary: paired=28, unpaired=0, ambiguous=0   (F0032)
[topo-extract] summary: paired=28, unpaired=0, ambiguous=0   (F0033)
[topo-extract] summary: paired=32, unpaired=0, ambiguous=0   (F0034)
[topo-extract] summary: paired=28, unpaired=0, ambiguous=0   (F0035)
[topo-extract] summary: paired=48, unpaired=0, ambiguous=0   (F0036)
[topo-extract] summary: paired=48, unpaired=0, ambiguous=0   (F0037)
[topo-extract] summary: paired=48, unpaired=0, ambiguous=0   (F0038)
[topo-extract] summary: paired=44, unpaired=0, ambiguous=0   (F0039)
[topo-extract] summary: paired=48, unpaired=0, ambiguous=0   (F0040)
```

**Every case reports `unpaired=0` at Step 7's exit.** The half-edge
graph is internally well-paired immediately after construction. The
flood_fill_patches Step 7 anchor is **empirically refuted** by data
already collectable from existing instrumentation.

This narrows the PR-Y15c candidate set from 2 anchors to 1:

- ~~Step 7 half-edge construction~~ (refuted: paired=N, unpaired=0)
- **`tessellate_waffle_solid` render LOD retessellation** ← only
  remaining candidate

**Why this is huge:** PR-Y15c's spec can skip Stage D (post-Step-7
half-edge graph probe) entirely. The TWIN_DEBUG output IS the Stage D
signal. PR-Y15c only needs to add Stage E at the post-render-LOD
output, then directly compare:
- Stage C says: `verts=28 tris=48 unique_edges=72 well_formed=true`
- Watertight oracle says: `V=26 E=60 F=36, 12 unpaired`
- Stage D (TWIN_DEBUG): `paired=30, unpaired=0` (already known)
- Stage E (NEW): the post-LOD render mesh — what does it look like?

The vert delta 28 → 26 and tri delta 48 → 36 between Stage C and the
watertight oracle is consistent with retessellation that drops 12
degenerate triangles (likely the inside-cyl wall tris after LOD
threshold filter) WITHOUT re-stitching the resulting boundary. The
PR14 "Render LOD per-face byte-identity defect" anchor from
`MEMORY.md/yang_implementation_status.md` 2026-05-02 entry is now the
SOLE remaining suspect for this cohort.

### Spec amendment recommendation for PR-Y15c

Spec PR-Y15c's Phase 0 SHOULD:
1. Skip the Stage D probe — explicitly cite this validation memo's §6.
   The TWIN_DEBUG `[topo-extract] summary: unpaired=0` data is the
   Stage D signal.
2. Add Stage E at the post-`tessellate_waffle_solid` render mesh.
3. Use F0031 + F0040 as the 2-case reproducer set (operand-order
   coverage; cluster is 10/10 homogeneous so 2 cases suffice).
4. Anchor pre-verification (per `feedback_anchor_before_fix.md`): add
   `[stage-e-canary] reached after tessellate_waffle_solid` BEFORE
   coding the real probe.

**Estimated PR-Y15c Phase 0 effort:** 1-2 hours (vs. PR-Y15a Phase 0's
~3 hours, since the Stage D probe is no longer needed).

## §7. Bonus: spec ambiguity item 3 (libtest --nocapture quirk) confirmed

implementer-e's §"Spec ambiguities" item 3 documented a libtest quirk
where the first probe-on run via `cargo test ... --nocapture 2>&1 > file`
captured zero `[conformal-probe]` lines. I encountered the same issue
during Phase 1 when running with simple stdout/stderr redirection.
Workaround: `cargo test ... --nocapture --test-threads=1 2>stderr_file
>stdout_file` (separate the streams; libtest's `--nocapture` only
releases stderr WITHOUT stdout merging). Documenting here so PR-Y15c's
test-author doesn't re-discover this for the third time.

## §8. Verification deltas

- PR-Y15a Phase 0 captured **3 reproducers** (F0031/F0032/F0040) with
  3 mandatory + the 10-case batch confirmation. My Phase 1 verified
  the **full 10-case batch** independently and confirmed implementer-e's
  table is empirically correct (every cell matches verts/tris/edges).
- The Stage Bb mutation test confirms the probe is **load-bearing** on
  the attribution decision (not dead instrumentation).
- The TWIN_DEBUG check confirms Step 7 half-edge construction is
  **innocent** for this cohort, narrowing PR-Y15c's anchor candidates
  from 2 to 1.
- F0035 and F0038 spot-checks (team-lead's requested expansion):
  signatures match implementer-e's table exactly; both fire row 4.

## §9. Working-tree state

- Mutation reverted; `git diff` against implementer-e's Phase 0 commit
  is byte-clean (`crates/kernel/src/boolean/topology_extract.rs`
  matches the ~33-line additive Stage Bb probe block exactly).
- No new untracked source files. New deliverable file:
  `docs/audits/pr_y15a_validation.md` (this memo).
- `app/tests/cases/assay/results.json` was modified during the
  validation runs — implementer-e's Phase 0 already updated it; my
  re-runs further refreshed the timestamp but pass/fail counts are
  unchanged (still 11 passed, 179 failed per PR-Y15b post-fix).
- `cargo clippy -p kernel --no-deps`: 91 warnings (matches
  implementer-e's baseline; my mutation test added/reverted produces 0
  warnings net).

## Verdict summary

**ACCEPT — proceed to PR-Y15c with anchor narrowing.**

- ✅ All 10 cases (not just 3) homogeneously fire row 4
- ✅ Stage Bb mutation-confirmed load-bearing
- ✅ Conformal-probe family genuinely exhausted (3 alternative probe
  sites considered and refuted)
- ✅ Step 7 half-edge construction empirically innocent (TWIN_DEBUG
  evidence — narrows PR-Y15c from 2 anchors to 1)
- ✅ Re-segmentation insight valid; route to PR-Y15c spec writer
- ✅ Mutation cleanly reverted, byte-clean diff against Phase 0 commit

Recommend manager Phase 4 commits implementer-e's Phase 0 work as-is +
sketches PR-Y15c spec scope in the commit message. The Stage E probe
at `tessellate_waffle_solid` render LOD is the **single load-bearing
deliverable** for PR-Y15c; F0031 + F0040 as 2-case reproducers; cite
this memo §6 to justify skipping Stage D.
