# PR-Y31 — Plumb Waffle `MeshBooleanOp` through Cherchi differential diff harness

**Author:** spec-y31 · **Date:** 2026-05-08 · **Plan:** `/home/claude/.claude/plans/optimized-wandering-wind.md`
**Canary:** `docs/audits/pr_y31_anchor_canary.md` (canary-y31, commit `988efa4`) — **PIVOT CONFIRMED**.
F0044 Stage B's "48 extras" reported in PR-Y30 is **not a Waffle production defect**; it is a
test-harness mis-config. The harness invokes Cherchi with hard-coded `union` while F0044's
first boolean op is `Subtract`. Three independent canary probes (arrangement, classification,
op-selection) all refute the production-fix hypothesis. Direct Cherchi re-invocation with
`subtraction` matches Waffle Stage B **byte-identically** (136/136 at 1µm quantization).

**Scope:** TEST-HARNESS-ONLY. Zero production code changes. Plumb the boolean op already
known to Waffle for the dumped pair into the Cherchi CLI invocation so both sides run the
same operator.

**FIP §3.2 + DoD §6 Infrastructure variant.**

---

## §1 Goal

Plumb the actual Waffle `MeshBooleanOp` used for the boolean operation captured at the
Stage B dump site into the harness's Cherchi `mesh_booleans` invocation, replacing the
hard-coded `cmd.arg("union")` at `crates/test-harness/tests/cherchi_differential_diff.rs:286`
with `cmd.arg(op_to_cli_str(op))`. The harness's Cherchi reference and Waffle production
output will then be compared op-aligned, not op-mismatched.

**Acceptance:**

- F0044 Stage B `extras` count: **48 → 0** (canary §4 empirical verification).
- F0020 Stage B `extras` count: unchanged (107 — F0020 is all-Union, harness already correct).
- Cohort F0045 / R0092: harness runs without ERROR; new baselines captured (canary §6
  predicts F0045 unchanged because all-Union; R0092 unknown depending on which op is dumped).
- All existing kernel/test-harness/Playwright tests remain GREEN. No production-side counter
  moves because no production code is touched.

Per `feedback_no_last_bug.md`: PR-Y31 fixes ONE measurement-side artifact. F0020's 107
extras, F0045's 466 extras, R0092's 368 extras, F0020 Status:Failed, all remain real
defects to be addressed in PR-Y32+.

---

## §2 Background

### §2.1 Verbatim from canary memo `988efa4`

Canary §6, lines 256–289 (`docs/audits/pr_y31_anchor_canary.md`):

> ### Fix shape
>
> Plumb the actual boolean op used by Waffle for the boolean being diffed into the
> harness's Cherchi invocation, so that `mesh_booleans` runs the matching op
> (`union` / `subtraction` / `intersection`).

> ### Paper citation (Cherchi 2022 §3, verbatim)
>
> > "Our method takes as input a set of input meshes M1, M2, ..., Mn, and **a Boolean
> > operator, namely union, intersection, subtraction**. ... The output is a mesh B
> > that contains the result of applying the Boolean operator to the input meshes."
> > — Cherchi 2022 §3, lines 232–236
>
> The reference algorithm's output is parameterized by the boolean operator. Comparing
> Waffle-Subtract against Cherchi-Union is not a reference-parity check; it's a
> category error.

Canary §4 (verbatim, empirical verification):

> ```
> Cherchi mesh_booleans subtraction: 72 verts, 136 tris
> Diff against Waffle Stage B at 1µm grid: 0 missing, 0 extras, 136 common
> ```
>
> **Perfect byte-identical match.** The 48 extras vanish entirely under correct
> Cherchi op selection.

### §2.2 PR-Y30 baselines that become INVALID under PR-Y31

The Stage B baselines in `docs/audits/pr_y30_stage_b_baselines.md` were captured under
hard-coded `union`. Any baseline for a case whose first dumped boolean op is **not** Union
compares wrong-op outputs and is therefore not a load-bearing oracle until PR-Y31 lands.

| Case | First-op (from `.waffle`) | PR-Y30 baseline status |
|---|---|---|
| F0020 | Union (`cut=false`) | VALID — harness already correct |
| F0044 | Subtract (`cut=true` on second extrude) | **INVALID** — extras=48 is harness artifact |
| F0045 | Union (all `cut=false`) | VALID — harness already correct |
| R0092 | Mixed (one Subtract + one Union; canary §6 banked: dumped-op identity unverified) | UNKNOWN — re-baseline post-fix |

PR-Y31 implementation MUST re-capture and document new baselines for all four cases at
`docs/audits/pr_y31_post_fix_baselines.md`. F0044 is the load-bearing assertion; the others
are cohort guards.

### §2.3 Meta-error trail and why earlier audits did not catch this

This section is required reading for adv-y31 and future PRs.

1. **PR-Y29 — harness construction.** Built `cherchi_differential_diff.rs` with
   `invoke_cherchi_union` as the only Cherchi entrypoint. The hard-coding choice
   reflected the F0020 spotlight (which is all-Union) and was not revisited when the
   cohort expanded.
2. **PR-Y30 — Stage C → Stage B calibration.** Switched the harness from comparing
   Cherchi-output to Waffle-Stage-C (downstream B-Rep assembly) to Waffle-Stage-B
   (post-`face_survival_detect` boolean result). The calibration was correct **for
   F0020 / all-Union cases**. The op-hardcoding survived because nobody in PR-Y30
   asked whether the cohort cases shared F0020's all-Union assumption.
3. **PR-Y30 banked finding "F0044 hypothesis REFUTED."** PR-Y30 measured F0044
   Stage B = Stage C in extras (48 = 48) and ruled out `flood_fill_patches` /
   `topology_extract` as the defect anchor. This was the load-bearing pivot signal
   but was mis-read as "defect is upstream of Stage B" rather than "defect is on
   the comparison side, not the production side."
4. **Canary stages PR-Y25 / Y26 / Y27 / Y28.** Four canary cycles ABORTed because
   the hypothesized production anchors did not survive in-situ probing. Each abort
   reasoned about Waffle internals. None of them inverted the question to ask "is
   the reference oracle itself wrong?" Per
   `feedback_reference_oracle_invalidates_in_both_directions.md`, this inversion
   should have come earlier; canary-y31 finally executed it by re-running Cherchi
   directly with `subtraction` and observing the byte-identical match.

The pattern PR-Y31 corrects: **trusting an oracle's framing without verifying the
framing's parameters against the case it is judging**. Cherchi 2022 §3 lines 232–236
make explicit that "the output is a mesh B that contains the result of applying the
Boolean operator to the input meshes" — i.e., op-parameterized. The harness froze the
op parameter without checking whether the corpus held it constant. It does not.

### §2.4 Yang 2025 §4.4.2 op-aligned in/out selection (paper invariant)

Yang 2025 §4.4.2 (`refs/text/yang2025_hybrid_boolean.txt` lines 574–605, verbatim):

> *Mesh Booleans.* After trimming the meshes using the intersection curves, we directly
> apply a standard inside/outside classification step [Cherchi et al. 2022] to identify
> the triangles that need to be retained, thus completing the mesh Boolean operation.

Waffle's `face_survival_detect` (`crates/kernel/src/boolean/topology_extract.rs:1871,
1884-1886`) implements the same per-op selection table Cherchi 2022 uses internally:

| Op | A-keep | B-keep | flip-B |
|---|---|---|---|
| Union | Outside | Outside | false |
| Subtract | Outside | Inside | true |
| Intersect | Inside | Inside | false |

Comparing Waffle-Subtract against Cherchi-Union compares disjoint selection rules
(Outside, Inside, flip_b=true vs Outside, Outside, flip_b=false), which is the
category error canary §6 names. The two outputs CAN agree only when the geometry
happens to make the selector difference invisible — they did not on F0044.

---

## §3 Parameters

**None** — this PR changes test-harness code only. The harness already knows the case_id
and runs the full Waffle pipeline; the boolean op for the dumped pair is a property of
the `.waffle` model and can be read at runtime.

The op is sourced as follows: read the `.waffle` JSON file under
`app/tests/cases/assay/<CASE_ID>.waffle` and inspect the **first** `"cut"` field that
follows the **second** extrude operation (the second extrude is the operand that triggers
the first boolean — the first extrude produces solid A; the second's boolean op against
solid A is the first dumped pair). Mapping:

- `"cut": false` → `MeshBooleanOp::Union`
- `"cut": true` → `MeshBooleanOp::Subtract`
- (No assay case currently exercises `Intersect`; the harness MUST emit a clear error
  if it encounters one rather than silently defaulting.)

Alternative (canary §6 mentions both): instrument `run_single_case` /
`run_waffle_and_collect_dumps` to capture the op as a side-channel return field. The
JSON-read path is simpler and adequate for the current corpus.

---

## §4 Branch Table

Single behavioral change row (test-harness only).

| Site | Before | After |
|---|---|---|
| `cherchi_differential_diff.rs:286` (`invoke_cherchi_union`) | `cmd.arg("union")` hard-coded | `cmd.arg(op_to_cli_str(op))` where `op` is plumbed from the case's `.waffle` model |
| `cherchi_differential_diff.rs:277-318` (function name + signature) | `fn invoke_cherchi_union(bin, path_a, path_b, path_out, case_id)` | `fn invoke_cherchi(bin, path_a, path_b, path_out, case_id, op)` |
| `cherchi_differential_diff.rs:322-475` (`run_diff_for_case`) | Calls `invoke_cherchi_union` directly | Reads `.waffle` first-op → calls `invoke_cherchi(..., op)` |

No new branches in production code. No new failure paths in the harness beyond explicit
error messages on unrecognized op or missing `.waffle` file.

---

## §5 Invariants (paper-cited, measurable)

### I1 — Cherchi 2022 §3 reference-output identity

Cherchi 2022 §3 lines 232–236 (verbatim,
`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt`):

> "Our method takes as input a set of input meshes M1, M2, ..., Mn, and a Boolean
> operator, namely union, intersection, subtraction. ... The output is a mesh B that
> contains the result of applying the Boolean operator to the input meshes."

Cherchi 2022 §3 line 252–254:

> "the result of a Boolean operation between two watertight manifold meshes that do not
> touch tangentially is guaranteed to be manifold watertight."

**Measurable consequence:** when the harness invokes Cherchi with op = Waffle's op,
Cherchi's output is the reference boolean RESULT for that op. The diff oracle then
measures Waffle-vs-reference correctness for the SAME boolean.

### I2 — Yang 2025 §4.4.2 op-aligned in/out selection

Yang 2025 §4.4.2 (lines 574–605) cites Cherchi 2022 §5 for the in/out classification
step and uses the same per-op selection table. Waffle's `face_survival_detect`
(`crates/kernel/src/boolean/topology_extract.rs:1884-1886`) implements:

- Union: A-Outside ∪ B-Outside
- Subtract: A-Outside ∪ B-Inside-flipped
- Intersect: A-Inside ∪ B-Inside

**Measurable consequence:** comparing Waffle-X-op output against Cherchi-Y-op output
is invalid when X ≠ Y. PR-Y31 enforces X = Y at the harness invocation.

### I3 — F0044 Stage B extras post-fix (load-bearing)

After PR-Y31 lands, the harness's F0044 Stage B diff MUST report:

```
extras == 0 AND missing == 0 AND common == 136
```

Canary §4 directly verified this by manual Cherchi-subtraction invocation against
Waffle Stage B at the 1µm quantization grid. The same quantization function
(`quantize_tri` at `cherchi_differential_diff.rs:162`) is used in the harness, so the
post-fix run MUST reproduce 0/0/136.

If extras stay > 0 on F0044, the empirical prediction is wrong and the PR is ABORTED
(see §7).

### I4 — Cohort guard (F0020 unchanged)

F0020's `.waffle` first op is `Union` (all `cut=false`). The harness already invokes
Cherchi with `union` correctly for F0020. PR-Y31's plumbing change MUST resolve to the
SAME `cmd.arg("union")` invocation for F0020 (because the JSON read returns Union),
producing **byte-identical** Cherchi output and therefore IDENTICAL extras / missing /
common counts.

Specifically: F0020 Stage B extras MUST stay ≤ 107 (the PR-Y30 baseline). Anything else
indicates the plumbing changed the Union code path, which it should not.

Cherchi non-determinism on F0020 (PR-Y30 banked finding: union output varies 246–295
tris across runs even at `TBB_NUM_THREADS=1`) means F0020 extras MAY vary across runs.
The guard is `≤ 107` not `== 107` to absorb that variance.

### I5 — Cohort guard (F0045 / R0092 must not ERROR)

The plumbed op invocation MUST NOT make Cherchi error out or timeout on F0045 / R0092
post-fix. The post-fix baselines MUST be captured and recorded.

PR-Y31 does NOT assert F0045 or R0092 reach extras = 0 — those cases may carry
independent defects (tessellation-grid divergence, Cherchi non-determinism). Those are
PR-Y32+ targets.

### I6 — Zero production-code impact

The Yang kernel baseline (1254 pass / 25 ignored / 42 reflecting the pre-PR state at
commit `988efa4`) MUST be unchanged post-PR. Test count and ignored count IDENTICAL.
WASM bridge byte-identity to pre-PR (no Rust kernel sources modified → identical
build output). This is structurally guaranteed by anti-scope §9; I6 is the
measurement that proves it.

---

## §6 Oracles

The following are the concrete validation mechanisms test-y31 will implement and
impl-y31 will assert.

### O1 — F0044 Stage B extras count (load-bearing)

In `crates/test-harness/tests/pr_y31_harness_op_plumb_regression.rs`:

- Test `pr_y31_f0044_extras_zero`:
  1. Run the harness diff on F0044.
  2. Parse the harness's printed `extras_count` from stderr (existing pattern; see
     `cherchi_differential_diff.rs:400-475`).
  3. Assert `extras_count == 0`.
  4. Assert `missing_count == 0`.
  5. Assert `common_count == 136`.
- Red-phase requirement: at `988efa4` (pre-fix), this test MUST fail with
  `extras=48, common=88`.

### O2 — F0020 Stage B no-regression (cohort guard)

In the same regression file:

- Test `pr_y31_f0020_no_regression`:
  1. Run the harness diff on F0020.
  2. Assert `extras_count ≤ 107`.
- Red-phase: at `988efa4`, this test MUST pass (current behavior).

### O3 — F0045 / R0092 no-ERROR (cohort guard)

In the same regression file:

- Test `pr_y31_f0045_r0092_no_error`:
  1. Run the harness diff on F0045 and on R0092.
  2. Assert neither errors / panics / times out (the harness's existing skip-on-error
     paths in `invoke_cherchi*` return None on timeout / non-zero exit; the test
     asserts the function returns `Some(_)` for both cases).
- Red-phase: at `988efa4`, this test MUST pass (current behavior is op-correct only
  for the union sub-cases, but Cherchi-union does run cleanly on the inputs).

### O4 — Existing `cherchi2022_reference_parity.rs` tests stay GREEN

`crates/test-harness/tests/cherchi2022_reference_parity.rs` exercises the harness's
sibling reference-parity test pattern. PR-Y31 MUST NOT regress it.

### O5 — Kernel test count baseline

`cargo test -p kernel` MUST report 1254 pass / 25 ignored / 42 (or whatever the
commit-`988efa4` baseline is — adv-y31 captures the exact baseline at canary's
commit, NOT at HEAD, to avoid drift). PR-Y31 changes zero kernel files; structurally
this is guaranteed. O5 is a guard against accidental drift.

### O6 — clippy + fmt clean

`cargo clippy -p test-harness -- -D warnings` clean.
`cargo fmt -p test-harness -- --check` clean.

### O7 — Post-fix baselines documented

Impl-y31 writes `docs/audits/pr_y31_post_fix_baselines.md` containing:

- F0044 Stage B extras / missing / common (predicted: 0 / 0 / 136)
- F0020 Stage B extras / missing / common (predicted: ~107 / ~30 / ~95 — same as PR-Y30)
- F0045 Stage B extras / missing / common (predicted: ~466 / ~0 / ~0 — unchanged)
- R0092 Stage B extras / missing / common (impl-y31 measures, no prediction)

Adv-y31 reads this memo and confirms F0044 prediction held.

---

## §7 Failure Modes

| Symptom | Diagnosis | Disposition |
|---|---|---|
| F0044 extras stays at 48 post-fix | Canary's empirical prediction was wrong, OR the dumped pair is not the Subtract pair (a different boolean op was captured under `YANG_DUMP_OBJ_BASE`) | **ABORT.** Re-canary to determine which boolean call the dump captures. The empirical Cherchi-subtraction verification in canary §4 was direct measurement on the dumped A.obj / B.obj — if extras stay at 48 with op=Subtract, the dump-site capture identity is wrong, which is a deeper investigation. |
| F0044 extras drops to 0 BUT F0020 extras grows above 107 | The plumbing accidentally perturbed the Union code path | **ABORT.** F0020 should resolve to `op=Union → cmd.arg("union")` — identical to today's behavior. If F0020 changes, the plumbing has a bug (e.g., wrong JSON parse, wrong op enum mapping). Fix the harness before shipping. |
| F0045 or R0092 Cherchi invocation errors out (non-zero exit, timeout) | The op for that case requires inputs Cherchi's binary rejects (e.g., the dumped pair was preprocessed for Union and is malformed for Subtract) | **ABORT and bank.** Per anti-scope §9, F0045/R0092 production defects survive this PR. Banking is fine; ERRORING is not. Investigate the dump-site identity for the failing case. |
| `cargo test -p kernel` regresses from baseline | A production file got touched (against anti-scope) | **ABORT.** Verify `git diff` shows ONLY test-harness changes. If yes, the regression is a flake — retry with `--test-threads=1`. If no, revert the production change. |
| PR-Y22 / PR-Y24 contract regressions | Structurally impossible (zero production code touched) | **ABORT** if observed; treat as kernel-test flake or environmental contamination. |
| clippy lint failure on `invoke_cherchi` rename | Stale callsites or signature drift | Fix and rerun; not an ABORT trigger. |
| JSON parsing fails on a `.waffle` file | Unexpected schema variation | **Fail loudly.** Do not default to Union. Per `feedback_yang_only.md` no fallback paths — emit a panic with the case_id and ask for case-specific investigation. |

---

## §8 Research Basis

### Yang 2025 — hybrid B-Rep / mesh boolean

`refs/text/yang2025_hybrid_boolean.txt`:

- **§3 lines 240–296.** Algorithm input: "B-Rep models and specified Boolean operations,
  including intersection, union, and subtraction." Yang's pipeline is op-parameterized
  from input.
- **§4.4.2 lines 574–605.** "Mesh Booleans. After trimming the meshes using the
  intersection curves, we directly apply a standard inside/outside classification step
  [Cherchi et al. 2022] to identify the triangles that need to be retained, thus
  completing the mesh Boolean operation." Yang explicitly delegates the per-op
  selection to Cherchi 2022 §5.
- **§4.4.3 lines 599–605.** "The watertightness of our result is inherited from the
  mesh Boolean output, ensuring the mesh has no geometric gaps." The mesh Boolean
  output IS op-dependent — comparing op-mismatched outputs is comparing different
  watertight results that happen to use the same arrangement.

### Cherchi 2022 — interactive and robust mesh booleans

`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt`:

- **§3 lines 232–236.** "Our method takes as input a set of input meshes M1, M2, ...,
  Mn, and **a Boolean operator, namely union, intersection, subtraction**. ... The
  output is a mesh B that contains the result of applying the Boolean operator to the
  input meshes." The output cardinality and identity depend on the operator argument.
- **§3 lines 252–254.** "the result of a Boolean operation between two watertight
  manifold meshes that do not touch tangentially is guaranteed to be manifold
  watertight." The result is op-specific — not the arrangement alone.
- **§4 lines 293–319.** Mesh arrangement step is op-INDEPENDENT (it produces a
  simplicial complex of the input geometry). This is consistent with canary §1's
  finding that Waffle and Cherchi both produce 136 sub-triangles at the arrangement
  step regardless of the downstream op.
- **§5 lines 385–460.** Inside/outside classification (Algorithm 1) labels patches
  as inside/outside each input mesh. The op is applied AFTER labeling to filter
  patches: "the union of two triangle meshes M1 and M2 (M1 ∪ M2) is the set of patches
  of M1 that are outside M2 plus the patches of M2 that are outside M1" (§3 lines
  282–286). Subtraction and intersection use different filters; these are the
  Yang §4.4.2 selection table rows.

### Canary memo (empirical verification)

`docs/audits/pr_y31_anchor_canary.md` (commit `988efa4`):

- **§4** — direct empirical: Cherchi-subtraction vs Waffle Stage B = 136/136 at 1µm.
- **§5** — layer attribution table: arrangement / classification / op-selection all
  REFUTED; only the harness invocation is the defect anchor.
- **§6** — fix-shape recommendation: plumb the op, ~15–35 LOC test-harness only.
- **§7** — strategic: the harness diff oracle is salvageable only if op-aligned.

---

## §9 Anti-Scope

The following are explicitly **not** changed in PR-Y31. PR-Y32+ targets them.

- **F0020's real 107-extras defect.** F0020's `face_survival_detect` probe (canary §6
  banked finding 1) shows mixed labels (outside=44 inside=8 on b#1 first call), so the
  defect is not a simple op-mismatch. Re-canary on F0020 with corrected harness.
- **F0045's tessellation-grid divergence.** F0045's Stage B common count is 0 at 1µm,
  meaning Waffle and Cherchi produce non-matching vertex positions even when given the
  same `.obj` inputs. This is a Yang §4.1.1 discretization issue, not an op issue.
- **R0092's Cherchi non-determinism.** Cherchi's union output varies 153 / 295 / 405
  tris across runs even with `TBB_NUM_THREADS=1`. Banked from PR-Y30.
- **Cherchi non-determinism investigation.** PR-Y30 banked; treat as upstream
  reference-side artifact; do not gate Waffle's correctness on it.
- **Production code changes.** PR-Y31 is INFRASTRUCTURE-ONLY. Any patch to
  `crates/kernel/` is out of scope and an ABORT trigger.
- **F0044's other 6 boolean ops** (3 Union + 3 more Subtract). The harness dumps only
  the FIRST qualifying op pair; subsequent ops are invisible to the diff oracle. The
  cross-op cumulative correctness check is PR-Y32+ work.
- **The 5-vertex-count discrepancy at the arrangement step** (canary §1: Cherchi 77
  verts vs Waffle 72 verts on F0044). Both produce 136 well-formed sub-triangles. The
  vertex-count delta does not affect the boolean result. Documented in canary §5
  banked; out of scope.
- **Yang §4.4.1 mesh updating layer** (CDT remeshing along refined intersection
  curves). Not under PR-Y31 attention.
- **Fillet / chamfer / shell.** Deferred indefinitely per CLAUDE.md.

---

## §10 Test Phase Recommendations (for test-y31)

### New file

`crates/test-harness/tests/pr_y31_harness_op_plumb_regression.rs` — 3 tests:

1. **`pr_y31_f0044_extras_zero`** (load-bearing, O1)
   - Invoke `run_diff_for_case("F0044")` (or the rename'd equivalent if test-y31
     prefers a smaller scope-call).
   - Capture stderr; parse `extras=`, `missing=`, `common=` counts.
   - Assert `extras == 0`, `missing == 0`, `common == 136`.
   - Mark `#[ignore]` if `CHERCHI2022_BIN` env var is required and not set (the
     harness already skips with a printed `[diff-harness {case} ] SKIP` line; test
     should detect that and bail with `eprintln + return` rather than asserting on
     missing data).

2. **`pr_y31_f0020_no_regression`** (cohort guard, O2)
   - Invoke harness diff on F0020.
   - Assert `extras_count <= 107` (absorb non-determinism variance per I4).

3. **`pr_y31_f0045_r0092_no_error`** (cohort guard, O3)
   - Invoke harness diff on F0045 and R0092 sequentially.
   - Assert the harness reports a diff (non-None Cherchi output) for both.

### Red-phase verification at `988efa4` (pre-fix)

Test-y31 MUST verify on `988efa4`:

- Test 1 FAILS with `extras=48` (this is the red signal).
- Test 2 PASSES (current behavior).
- Test 3 PASSES (current behavior).

Without test 1 RED at `988efa4`, the regression test isn't load-bearing. Document the
verification result in the test phase ship message.

### Skip / ignore policy

If `CHERCHI2022_BIN` is unset or the binary missing, all three tests SKIP (do not fail).
The harness already handles this via the `[diff-harness {case}] SKIP` log line; the
regression tests should detect this and return early with `eprintln` instead of
asserting. This mirrors `cherchi2022_reference_parity.rs`'s skip pattern.

---

## §11 Impl Phase Recommendations (for impl-y31)

### Touch ONLY these files

- `crates/test-harness/tests/cherchi_differential_diff.rs` (the harness itself).
- `crates/test-harness/tests/pr_y31_harness_op_plumb_regression.rs` (test-y31's new
  file, if it lands before impl).
- `docs/audits/pr_y31_post_fix_baselines.md` (NEW — post-fix baseline capture).

**Do NOT touch any file under `crates/kernel/`, `crates/wasm-bridge/`, or `app/`.**
If a diff against `crates/kernel/` shows non-zero lines, ABORT and revisit.

### Implementation outline

1. **Add a small enum locally to the test file** (since `MeshBooleanOp` is `pub(crate)`
   in `crates/kernel/src/boolean/exact_mesh.rs:1179` and not exported):
   ```rust
   #[derive(Debug, Clone, Copy)]
   enum HarnessBoolOp { Union, Subtract, Intersect }
   ```
   Or pick a name that doesn't shadow kernel types (e.g., `CherchiOp`). Do NOT
   expose `MeshBooleanOp` from the kernel — that's a production change.

2. **`op_to_cli_str` helper:**
   ```rust
   fn op_to_cli_str(op: HarnessBoolOp) -> &'static str {
       match op {
           HarnessBoolOp::Union => "union",
           HarnessBoolOp::Subtract => "subtraction",
           HarnessBoolOp::Intersect => "intersection",
       }
   }
   ```

3. **`read_first_boolean_op(case_id) -> HarnessBoolOp`** that reads
   `app/tests/cases/assay/<CASE_ID>.waffle` and returns the op of the second extrude
   (first boolean target). The schema check from canary §6 banked: assay corpus uses
   `"cut": true/false`. JSON parse via `serde_json` (already in test-harness's dep
   tree; verify before adding).

4. **Rename `invoke_cherchi_union` → `invoke_cherchi`**, add `op: HarnessBoolOp`
   parameter, replace `cmd.arg("union")` with `cmd.arg(op_to_cli_str(op))`. Update
   the file path naming (`{case}_cherchi_union.obj` → `{case}_cherchi_{op_str}.obj`)
   so the output path identifies the op for human inspection. Update doc comment
   above the function.

5. **`run_diff_for_case`**: before calling `invoke_cherchi`, call
   `read_first_boolean_op(case_id)`; pass it down.

6. **LOC budget:** canary §6 estimated 15–35 LOC. Hold to ≤35 LOC for the production
   harness change, +~50 LOC for the regression test file (test-y31's budget).

### Post-fix baseline capture

After the fix lands and tests pass, run the harness on all four cases (F0020 / F0044 /
F0045 / R0092) and capture stderr to `docs/audits/pr_y31_post_fix_baselines.md`. Format
mirrors `docs/audits/pr_y30_stage_b_baselines.md`. Required content:

- Case ID.
- Op resolved from `.waffle` JSON.
- Cherchi CLI arg used.
- Cherchi output verts / tris.
- Waffle Stage B verts / tris.
- Set diff: extras / missing / common at 1µm.
- TOP_N first-divergence triangles (use existing harness output).

This memo replaces `docs/audits/pr_y30_stage_b_baselines.md` as the load-bearing
baseline.

### Banked findings to surface to adv-y31

- The harness `quantize_tri` grid is 1µm (`QUANTIZE_GRID = 1e-6`). Adv-y31 will want
  to confirm this is appropriate for the F0044 case (canary §4 used it without issue,
  but future cases may need finer / coarser grids).
- `read_first_boolean_op` reads JSON synchronously; this is fine for the assay corpus
  size (~250 cases × ~few KB) but should not be expanded to per-test-case in a hot
  loop without caching.
- The `Intersect` enum branch is present for completeness but the current assay corpus
  doesn't exercise it. If impl-y31's regression test attempts Intersect, it should be
  marked `#[ignore]` until a real Intersect case lands.

---

## §12 Acceptance Gate for PR-Y31

Adv-y31 ACCEPTS when ALL of the following hold:

1. **O1 GREEN** — F0044 Stage B `extras=0, missing=0, common=136` on canary's
   commit `988efa4` + the PR's diff.
2. **O2 GREEN** — F0020 Stage B `extras <= 107`.
3. **O3 GREEN** — F0045 and R0092 harness invocations succeed (return `Some(_)`).
4. **O4 GREEN** — `cherchi2022_reference_parity.rs` tests pass.
5. **O5 GREEN** — `cargo test -p kernel` count baseline preserved (test count and
   ignored count IDENTICAL to commit `988efa4`).
6. **O6 GREEN** — clippy + fmt clean.
7. **O7 PRESENT** — `docs/audits/pr_y31_post_fix_baselines.md` exists, lists all four
   cases with their resolved op + counts.
8. **Diff scope GREEN** — `git diff HEAD` shows ONLY changes under
   `crates/test-harness/tests/` and `docs/audits/pr_y31_post_fix_baselines.md`.
9. **No production code touched** — `git diff HEAD --stat` shows ZERO lines under
   `crates/kernel/`, `crates/wasm-bridge/`, or `app/`.
10. **No WASM rebuild needed** — because no kernel code changed.

ABORT triggers (any one of):

- O1 FAILS (F0044 extras > 0 post-fix).
- O2 FAILS (F0020 extras > 107 post-fix).
- O5 FAILS (kernel count drift).
- Diff scope shows changes outside test-harness + audit doc.

---

## §13 Notes for adv-y31

- Verify the empirical claim in §5 I3 via the canary memo §4 quoted block AND by
  running the impl's post-fix harness against F0044. Two independent measurements
  required.
- Verify the `.waffle` JSON read returns Subtract for F0044 (second-extrude
  `"cut": true`) and Union for F0020 / F0045 (`"cut": false` for all extrudes).
  This is the contract that makes I4 (F0020 cohort guard) structurally guaranteed.
- The `Intersect` branch is presently unreachable in the corpus. Adv-y31 may wave it
  through as scaffolding; do not require a test for it without a corpus case.
- Per `feedback_adversary_no_destructive_git.md`, do NOT `git stash` / `checkout`
  during adv work. Use `git show <ref>:<file>` or `git worktree add` for baseline
  reads.
- Per `feedback_implementer_anti_fabrication_diff.md`, impl-y31's ship report MUST
  end with `git diff HEAD --stat` + numstat + first 50 lines + `wc -l` to forestall
  fabrication accusations.
