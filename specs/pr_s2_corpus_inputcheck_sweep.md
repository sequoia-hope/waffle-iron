# PR-S2 — Cherchi Inputcheck Corpus Sweep (Diagnostic Test Contract)

This is a SHORT contract for a diagnostic test, not a full FIP §3.2
modeling spec. PR-S1 (commit `17792eb`) shipped the Cherchi 2022
sidecar wrapper and the F0002 reference-parity pinprick; PR-S2
generalizes that pinprick into a corpus-wide inputcheck sweep so
PR-S3's anchor decision rests on data from all 190 cases, not just one.

## 1. Goal

Run `mesh_booleans_inputcheck` on every assay case's pre-Cherchi A and
B meshes and classify each into one of seven mutually exclusive
buckets. Cross-tab Waffle pass/fail × Cherchi-input-validity to
localize the upstream-of-Cherchi defect: is the input-axiom violation
universal across the corpus (one defect surface) or case-specific
(several defect surfaces)? The cross-tab also surfaces the most
interesting cell — Waffle-failed × Cherchi-valid — which would be
evidence for a defect that lives DOWNSTREAM of the Cherchi input
contract, contradicting the F0002 finding's universality assumption.

## 2. Classification scheme

Cherchi's `mesh_booleans_inputcheck <mesh.obj>` prints exactly five
lines to stderr (per `docs/sidecar/cherchi2022_build_guide.md` and the
F0002 capture in `docs/audits/cherchi2022_sidecar_feasibility.md`
§"Build verified 2026-05-03"):

```
Manifold check:                   {passed|FAILED}
Watertight check:                 {passed|FAILED}
Local  Orientation check:         {passed|FAILED}
Global Orientation check:         {passed|FAILED}
Intersection check:               {passed|FAILED}
```

The result word is matched **case-insensitively** (the build guide
documents lower-case `failed`; the F0002 capture shows upper-case
`FAILED` — the parser must accept both). Compute a 5-bit failure mask
`(M, W, LO, GO, I)` where each bit is 1 if the corresponding line says
`failed`/`FAILED`. Map to a bucket:

| Bucket | Selector | Meaning |
|---|---|---|
| `valid` | exit 0 AND mask `00000` (all 5 lines `passed`) | Mesh satisfies Cherchi's input precondition. |
| `non_manifold` | only `M == 1` | Manifold check failed alone. |
| `non_watertight` | only `W == 1` | Watertight check failed alone. |
| `self_intersecting` | only `I == 1` | Intersection check failed alone. |
| `bad_orientation` | only `LO == 1` and/or only `GO == 1` (no other bit set) | Either or both orientation checks failed alone (Local+Global count as one orientation category for selector purposes). |
| `combined_failures` | mask has ≥ 2 distinct check categories failed | The catch-all (matches the F0002 pattern: `M`, `W`, `I` all failed). The TSV `cherchi_detail` column carries the raw 5-line output so the adversary can sub-classify without re-running. |
| `runaway` | subprocess killed by 10 s timeout (no parseable output) | `mesh_booleans_inputcheck` itself failed to terminate. Likely rare for inputcheck (validation is fast) but possible on extreme meshes. |

Mapping is total over the 32 possible masks plus the timeout state.
Test author MUST `panic!` on any mask the table does not cover —
adding a bucket requires amending this spec.

## 3. TSV schema

**One row per (case_id × side)**, so 380 rows for the current
190-case corpus. Header row first, then tab-separated:

| Column | Type | Description |
|---|---|---|
| `case_id` | string | E.g. `F0002`, `R0044`. |
| `side` | enum | `A` or `B`. |
| `waffle_status` | enum | `Passed` / `Failed` / `Errored` / `MissingDump` (the last when `YANG_DUMP_OBJ_BASE` was set but the OBJ wasn't written — e.g. PR13's AABB-disjoint short-circuit, or Waffle bailed before dump site). |
| `cherchi_class` | enum | One of the 7 buckets above. Empty when `waffle_status == MissingDump`. |
| `cherchi_detail` | string | Raw 5-line inputcheck stderr joined with `;`, truncated to 200 chars. Empty when `MissingDump` or `runaway`. For `runaway`, set to literal `runaway: subprocess killed at 10s`. For `MissingDump` rows where `waffle_status=Errored` due to a `WAFFLE_TIMEOUT` (kernel hang in `run_single_case` exceeding the per-case Waffle timeout — see §4), set `cherchi_detail` to literal `waffle-timeout: <SEC>s` (e.g., `waffle-timeout: 60s` for the canonical 60s default backported from `examples/inputcheck_sweep.rs` per PR-S2 adversary-2 §5). |

Output path: `docs/audits/cherchi_inputcheck_sweep_2026-05-03.tsv`.
Committed artifact for the findings memo to reference. Include the
header row.

## 4. Timeout policy

- **Per-OBJ inputcheck timeout**: 10 s wall clock. On overflow,
  `child.kill()`, classify as `runaway`, log
  `[inputcheck-sweep] runaway on <case_id> side <A|B>` to stderr,
  continue to next case. Mirror the `run_with_timeout` pattern at
  `cherchi2022_reference_parity.rs:79`.
- **Per-case Waffle timeout**: inherit `run_single_case`'s default
  (30 s per `assay/randomized_runner.rs`). PR-S2 introduces no new
  Waffle timeout policy — it just consumes the existing runner.
- **Whole-sweep operational budget**: 30 min. Per the plan's
  verification section, a 20-case dry run should land < 8 min before
  the full corpus is attempted. Not a code-level invariant.

## 5. Stdout summary

After the sweep completes, print:

```
[inputcheck-sweep] total=380 valid=N non_manifold=N non_watertight=N
                    self_intersecting=N bad_orientation=N combined_failures=N runaway=N
                    missing_dump=N
[inputcheck-sweep] cross-tab:
[inputcheck-sweep]   waffle=Passed × cherchi=valid: N
[inputcheck-sweep]   waffle=Passed × cherchi=combined_failures: N
[inputcheck-sweep]   waffle=Failed × cherchi=valid: N  ← interesting if >0
[inputcheck-sweep]   waffle=Failed × cherchi=combined_failures: N
```

Counts on the `total=` line are over OBJ invocations (i.e., the 380
TSV rows). `total` MUST equal the sum of all per-bucket counts +
`missing_dump` exactly. The cross-tab shows only the four
information-dense cells listed; the adversary can pivot the TSV for
finer breakdowns. The `← interesting if >0` annotation is a literal
part of the printed line — it flags the cell that, if non-zero, would
contradict the F0002 finding's universality.

## 6. Out of scope

- Running `mesh_booleans union/intersect/subtract` (the runaway
  operation). PR-S1 capped that subprocess at 30 s after it burned 6
  hours on F0002. PR-S2 only invokes `mesh_booleans_inputcheck`,
  which is fast.
- Production code changes. The existing `YANG_DUMP_OBJ_BASE` env-var
  path from PR-S1 already dumps the pre-Cherchi A and B meshes; the
  sweep just sets it per-case to a temp path.
- A new oracle. `mesh_booleans_inputcheck` IS the oracle.
- Comparing pre- vs post-Cherchi meshes (would require a second
  dump site).
- Any Waffle fix. PR-S2 is investigation only; PR-S3's spec consumes
  PR-S2's findings to pick the PR-Y15 anchor.
- Testing the inputcheck binary itself for correctness — it is
  treated as a black-box oracle.

## 6a. Operational notes

The `app/tests/cases/assay/results.json` snapshot used for the
findings memo's cross-tab MUST be taken BEFORE the sweep starts (the
sweep mutates `results.json` via `run_single_case`; reading it
post-sweep gives drifted Waffle-status counts that no longer reflect
the categories observed during classification). Adversary saves the
snapshot to `/tmp/results_during_sweep.json` (or equivalent) at
sweep-start and consumes that file for §5's cross-tab.

## 7. Test-author-c responsibilities

- Implement `crates/test-harness/tests/cherchi_inputcheck_corpus_sweep.rs`
  per this contract (~250 lines, `#[ignore]`'d, structurally mirroring
  `assay_randomized.rs::yang_fast` for case discovery + iteration).
- Reuse PR-S1's helpers: `cherchi_bin()` and `run_with_timeout()` from
  `cherchi2022_reference_parity.rs:79,117`. Two reasonable options for
  reuse, test author's call:
  - **Option A**: copy the two helpers into the new test file.
    Smaller diff; some duplication.
  - **Option B**: extract them into a shared module (e.g.
    `crates/test-harness/src/cherchi_helpers.rs`) and have both tests
    import. Cleaner long-term; one extra file in the diff.
  - Recommendation: **Option B if it's already needed elsewhere; Option A otherwise**. PR-S2 does not require the extraction.
- Discover cases via the existing `discover_cases` helper from
  `assay/randomized_runner.rs` (used by `yang_fast`). Don't reinvent
  case discovery.
- Per-case: invoke `run_single_case` with `YANG_BOOLEAN=1` +
  `YANG_DUMP_OBJ_BASE=<per-case temp path>`. Map `run_single_case`'s
  outcome to the `waffle_status` enum. If the OBJs landed, run
  inputcheck on each side; classify; record TSV row. If they didn't
  land, record `MissingDump` rows and skip the inputcheck calls.
- Per-OBJ temp path: under
  `std::env::temp_dir().join(format!("waffle_inputcheck_sweep_{}", std::process::id()))`,
  with a per-case subdirectory. Cleanup is optional — leftover files
  are useful diagnostic artifacts on a failed run.

**Test-author-c is NOT responsible for:**
- Writing the PR-Y15 spec (PR-S3's spec writer's job, after the
  adversary's findings memo lands).
- Running the sweep end-to-end and interpreting it (PR-S3 Adversary's
  job — Phase 3 in the plan).
- Any kernel changes.

## References

- `docs/sidecar/cherchi2022_build_guide.md` — `mesh_booleans_inputcheck`
  CLI signature + 5-line output format.
- `docs/audits/cherchi2022_sidecar_feasibility.md` §"Build verified
  2026-05-03" — F0002 capture (the `FAILED`/`passed` pattern this
  spec's classifier matches).
- `crates/test-harness/tests/cherchi2022_reference_parity.rs` — PR-S1
  helpers (`cherchi_bin`, `run_with_timeout`, `parse_obj`).
- `crates/test-harness/tests/assay_randomized.rs::yang_fast` (lines
  594–680) — the structural pattern for an `#[ignore]`'d 190-case
  sweep.
- `crates/test-harness/src/assay/randomized_runner.rs` —
  `run_single_case` + `discover_cases` + the `Passed/Failed/Errored`
  vocabulary the TSV inherits.
- Cherchi et al. 2022 [#38] §3 — input precondition (manifold,
  watertight, no self-intersections, well-oriented) the inputcheck
  binary enforces.
- Cherchi et al. 2020 [#9] §5 — well-formed simplicial complex
  guarantee that downstream Cherchi 2022 stages assume.
- Yang et al. 2025 [#24] §4.1.1, §4.1.2, §4.5.5 — Waffle's
  tessellation + coplanar preprocess sites where the upstream defect
  most likely lives.
