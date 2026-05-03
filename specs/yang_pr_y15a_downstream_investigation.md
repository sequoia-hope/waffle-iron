# PR-Y15a — Downstream-of-Cherchi defect Phase-0 investigation

**Status:** INVESTIGATION SPEC (pre-FIP-§3.2). NOT a full fix spec.
**Anchor empirical evidence:** `docs/audits/pr_s2_inputcheck_corpus_findings.md`
§3 (the 284-row Waffle=Failed × Cherchi=valid "interesting cell" — 160
unique cases, 126 with both sides Cherchi-valid; the F0031–F0040
ten-case stripe is the cleanest reproducer cluster).
**Reproducer:** F0031 (canonical, from the `boolean-watertight`
cluster). Spot-validated by F0032 + F0040.
**Plan reference:** `/home/claude/.claude/plans/reactive-juggling-sloth.md`
PR-S3 deliverable 3.

---

## 1. Goal

Localize WHICH Waffle code path produces the half-edge

```
yang_boolean: result validation failed:
half_edge[N].twin = 0 but twin.twin = M (expected N)
```

violation on Cherchi-VALID input. Output: a Phase-0 instrumentation
memo (`docs/audits/pr_y15a_phase0_anchor_findings.md`) that names the
exact buggy function. The actual fix spec (`PR-Y15a-fix`) is written
ONLY AFTER Phase 0 names the anchor.

The 78% Cherchi-valid cohort (PR-S2 §3) is the dominant Yang-pipeline
defect surface. PR-Y15b (the F0002-class minority) is concurrent and
independent. PR-Y15a's Phase 0 must run BEFORE any fix-spec is
attempted on this cohort.

## 2. Why this isn't a fix spec yet

Per `MEMORY.md/feedback_anchor_before_fix.md`: PR12, PR13, PR-Y14a,
PR-Y14b, and PR-Y14c each picked a wrong anchor for the F0002 /
twin-pairing class. PR-Y14c was proven wrong by the PR-S1 sidecar
oracle at commit `aee34ce`. The strategic-escalation rule
("three wrong anchors in a row → stop bisecting, build a reference
comparison") fired at PR-Y14c, and the PR-S1/S2 sweep showed F0002 was
the EXCEPTION — the 78% cohort is the dominant defect, and we have
zero anchor evidence for it yet.

Writing a full fix spec for the 78% cohort right now would be a fifth
wrong anchor with high probability. The Phase-0 instrumentation cycle
prevents that. Per FIP §3 + Engineering Constitution P10 (plan-first),
PR-Y15a's deliverable is investigation infrastructure + a decision
tree, not code.

## 3. Phase-0 instrumentation requirements

### 3.1 Reuse PR-Y14a's 3-probe pattern

PR-Y14a's conformal probes are gated on `YANG_CONFORMAL_PROBE=1` env
var and live in `crates/kernel/src/boolean/topology_extract.rs`:

| Probe | Where | What it checks |
|---|---|---|
| Stage A | ~L1669, post-tessellation / pre-arrangement input | `check_conformal` on the pre-Cherchi merged mesh |
| Stage B | ~L1880, post-survival labeling | `check_conformal` on the surviving-triangle mesh after `label_cells` filtering |
| Stage C | ~L787, post-`flood_fill_patches` Step 7, pre-half-edge-validation | `check_conformal` on the final half-edge graph before the validator runs |

Search `[conformal-probe]` in `topology_extract.rs` to find the exact
sites.

### 3.2 ADD ONE NEW probe — "Stage Bb"

Insert a new probe between Stage B and Stage C, immediately
POST-`label_cells` and PRE-`flood_fill_patches`. The probe dumps mesh
state at this exact pipeline point:

- **Why this position:** Stage B captures `label_cells` OUTPUT but
  doesn't separate `label_cells` corruption from
  `flood_fill_patches` corruption. The new Stage Bb captures the
  state BEFORE `flood_fill_patches` runs. If Stage Bb is well-formed
  and Stage C is broken, the anchor is `flood_fill_patches`. If
  Stage Bb is already broken, the anchor is `label_cells`.

- **Implementation pattern:** mirror the existing Stage A/B/C
  `eprintln!("[conformal-probe-stageX] ...")` lines. Same env-var
  gate (`YANG_CONFORMAL_PROBE=1`), same `check_conformal` invocation,
  same output format. New stage tag: `stageBb`.

- **Anchor-before-coding requirement (per
  `feedback_anchor_before_fix.md`):** before adding the
  `check_conformal` call, add an `eprintln!("[stageBb-canary]
  reached")` at the planned site, run the F0031 test with
  `YANG_CONFORMAL_PROBE=1` set, and confirm the canary fires. PR12
  through PR-Y14b had FIVE wrong-anchor cycles before instrumentation
  caught a non-executing anchor; do not skip this step.

### 3.3 Reproducer harness

Run the existing PR-S1 reference-parity-style harness on F0031:

- **Option A (preferred, smaller diff):** Add a new `#[ignore]`'d
  test to `crates/test-harness/tests/cherchi2022_reference_parity.rs`
  (or a new test file `crates/test-harness/tests/pr_y15a_phase0_probe.rs`)
  that runs F0031 with `YANG_CONFORMAL_PROBE=1` + `YANG_BOOLEAN=1`,
  captures the four-stage probe output (A, B, Bb, C), and writes the
  capture to `docs/audits/pr_y15a_phase0_anchor_findings.md`.
- **Option B:** Extend the existing F0002 conformal probe test
  pattern in `crates/test-harness/tests/yang_conformal_probe.rs` with
  a parallel `f0031_conformal_probe_pinned` (and `f0032`, `f0040`)
  test.

Implementer's choice. The deliverable is the four-stage probe
capture, not the test file structure.

## 4. Branch outcomes (decision tree)

The Stage A / Stage Bb / Stage C combination tells us the anchor:

| Stage A | Stage Bb (new) | Stage C | Anchor | Next PR |
|---|---|---|---|---|
| `well_formed=true` | `well_formed=true` | `well_formed=false` | `flood_fill_patches` twin-pairing logic in `topology_extract.rs` is corrupting the half-edge graph | PR-Y15a-fix targets `crates/kernel/src/boolean/topology_extract.rs::flood_fill_patches` (the PR12/PR13 spirit was correct; the prior anchors at "Step 6" were too narrow — the actual buggy line is identified by Phase 0) |
| `well_formed=true` | `well_formed=false` | `well_formed=false` | `label_cells` is corrupting the conformal mesh during inside/outside classification | PR-Y15a-fix targets `crates/kernel/src/boolean/exact_mesh.rs::label_cells` (or wherever Cherchi 2022 §5 + Algorithm 1's ray-cast classification is implemented in Waffle) |
| `well_formed=false` | (skip — no need to look further) | (skip) | F0031 is misclassified as 78%-cohort — it actually has a pre-Cherchi defect that PR-S2's `mesh_booleans_inputcheck` happened to accept (false negative on Cherchi's loose input check vs. our stricter conformal probe) | Re-bucket F0031 + cluster as F0002-class; fold the cluster into PR-Y15b's reproducer set, and write a follow-up note documenting the Cherchi-inputcheck false negative |
| `well_formed=true` | `well_formed=true` | `well_formed=true` AND Waffle still fails the half-edge validator | Anchor is in B-Rep assembly POST-`flood_fill_patches` (i.e., after Step 7, in the half-edge construction or validation logic itself) | Spec a new investigation PR-Y15c with a Stage D probe added immediately before the half-edge validator |

The four outcomes are MUTUALLY EXCLUSIVE and EXHAUSTIVE. Phase 0
must produce one of these four findings; if the data doesn't fit any
of the four (e.g., A broken AND C well-formed — non-monotonic), the
investigation has surfaced a new defect class and Phase 0 is extended
with additional probes per implementer's judgment, with a memo
explaining the anomaly.

## 5. Spot-check requirement

Run the same four-stage probe on F0032 + F0040 (two additional cases
from the F0031–F0040 boolean-watertight stripe). Compare:

- **If F0031, F0032, F0040 all fire the SAME row of the table above:**
  the cluster is homogeneous; PR-Y15a-fix's scope is the single
  anchor named by that row; the 10-case stripe is one defect surface.
- **If F0031, F0032, F0040 fire DIFFERENT rows:** the cluster is
  heterogeneous; PR-Y15a-fix becomes multi-anchor (one per fired row)
  OR PR-Y15a-fix is split into PR-Y15a-fix-1, PR-Y15a-fix-2, etc.
  Phase 0 memo SHALL document the split rationale per case.

The spot-check is mandatory. A single-case finding (F0031 alone)
is insufficient evidence for the PR-Y15a-fix anchor commitment.

## 6. Out of scope

- **Writing the actual fix code.** PR-Y15a-fix follows ONLY after
  Phase 0 names the anchor. Per FIP §3.2 + P10, fix code without an
  empirically-anchored fix spec is forbidden.
- **F0002-class investigation.** PR-Y15b's territory.
- **R0071 kernel hang.** A separate defect class (PR-S2 §5).
- **F0005's distinct probe signature.** Future work; F0005's anchor
  is separately addressed by PR-Y15b §6.4 (potentially split into
  PR-Y15b.2).
- **Sample size beyond F0031 + F0032 + F0040.** If broader coverage
  is desired, that's PR-Y15a-followup; Phase 0's exit criterion is
  three-case spot-check completion, not corpus-wide.
- **Comparing Phase-0 findings against the Cherchi 2022 reference
  implementation.** The reference doesn't expose a per-stage probe
  API at `flood_fill_patches` granularity (Cherchi's Algorithm 1
  outputs labeled patches, not half-edges; the half-edge
  reconstruction is Waffle-specific). Reference parity at this stage
  is impractical; PR-Y15a-fix's I8 instead asserts that Stage Bb /
  Stage C reports `well_formed=true` post-fix.

## 7. Implementation notes for the eventual PR-Y15a-fix spec writer

Per Phase-1 explore agent (in the PR-S3 plan): "Yang doesn't
prescribe half-edge reconstruction — that's Waffle's own post-Cherchi
B-Rep assembly. Cherchi 2022 Algorithm 1 outputs labeled patches,
not half-edges; our `flood_fill_patches` then reconstructs half-edge
adjacency."

This means the fix anchor is in Waffle code that the Yang 2025 paper
DOES NOT describe. PR-Y15a-fix's research basis can cite:

- **Mantyla [#16]** — Half-edge B-Rep data structure (the canonical
  reference for the Euler-operator-based half-edge graph our
  `flood_fill_patches` reconstructs).
- **Cherchi 2022 [#38] §5 + Algorithm 1** — Ray-cast inside/outside
  classification. This is what Waffle's `label_cells` implements;
  if the Stage Bb probe shows `label_cells` is the anchor, the fix
  spec must cite Algorithm 1 line-by-line and verify Waffle's port
  against it.
- **Yang 2025 [#24] §4.4.2** — Patch segmentation by ray-cast (the
  paper's reference for what `label_cells` does at the level above
  Cherchi 2022's Algorithm 1).

The fix spec MUST follow the FIP §3.2 template (Goal, Parameters,
Branch table, Invariants I1–I8, Oracles, Failure modes, Research
basis, Out of scope, Verification) and MUST include reference parity
(I8) at the level granted by the Phase-0 finding (see §6 — full
sidecar diff isn't possible at this stage; the I8 instead asserts
post-fix `well_formed=true` at the Phase-0-identified probe).

## 8. Reference parity for PR-Y15a-fix (forward-looking)

PR-Y15a-fix's I8 invariant SHALL be:

- **For "flood_fill_patches" anchor:** post-fix, F0031 + F0032 + F0040
  Stage C reports `well_formed=true` (or no twin-pairing self-loop),
  AND the F0031–F0040 ten-case stripe migrates from `auto-union-failed`
  / `boolean-watertight` to either `Passed` or a strictly-later
  failure mode in `app/tests/cases/assay/results.json`.
- **For "label_cells" anchor:** post-fix, F0031 + F0032 + F0040
  Stage Bb reports `well_formed=true`, with the same downstream
  cohort migration commitment as above.
- **For "B-Rep assembly post-flood_fill_patches" anchor (Stage D
  case):** post-fix, the Stage D probe (specified in PR-Y15c)
  reports `well_formed=true` on the F0031-cluster cases.

In all three cases, PR-Y15a-fix MUST also re-run the PR-S2 corpus
sweep and verify NO `valid` row migrates to `combined_failures` or
any other failure bucket (an analogous I6 to PR-Y15b's). The 78%
Cherchi-valid cohort's input precondition stays satisfied; the fix
is purely downstream of `subdivide_mesh_pair_full_cherchi`.

## 9. Phase-0 deliverable checklist

When Phase 0 is complete, the implementer of PR-Y15a (investigation
phase) SHALL produce:

1. `docs/audits/pr_y15a_phase0_anchor_findings.md` containing:
   - The four-stage probe capture for F0031 (verbatim
     `[conformal-probe-stageX]` stderr lines).
   - The same capture for F0032 + F0040 (spot-check).
   - The matched row of the §4 decision tree (one of the four).
   - The named anchor function (file path + line range).
   - For "homogeneous cluster" finding: confirmation that all three
     cases fire the same row.
   - For "heterogeneous cluster" finding: per-case split with
     proposed PR-Y15a-fix-1 / PR-Y15a-fix-2 / etc. scope.
2. The new Stage Bb probe code committed to
   `crates/kernel/src/boolean/topology_extract.rs`, gated on
   `YANG_CONFORMAL_PROBE=1`, default-off, production-safe (mirrors
   the existing Stage A/B/C pattern).
3. The reproducer test (Option A or B from §3.3) committed to
   `crates/test-harness/tests/`, `#[ignore]`'d, runnable as a
   single command.

Once these three deliverables land, the PR-Y15a-fix spec writer
(distinct agent per FIP role rotation) consumes the findings memo
and writes the fix spec per §7's template.
