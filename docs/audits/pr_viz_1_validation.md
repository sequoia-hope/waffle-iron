# PR-VIZ-1 — adversary-10 validation memo

**Verdict: ACCEPT**

Plan: `~/.claude/plans/reactive-juggling-sloth.md` sub-phase 0c.
Spec: `specs/yang_pr_viz_1_per_stage_obj_dump.md`.
Implementer: implementer-n (sub-phase 0b).
Adversary: adversary-10 (NEW per `feedback_oracle_credibility_via_role_separation.md`).

Dev tooling, not a defect fix. Lighter validation than fix-PR per plan.
All hard contracts hold: probe-off no-op, probe-on file structure, mutation
test, byte-clean diff. One spec-permitted deviation accepted.

---

## §1 Probe-off no-op verification

Independent test environment: empty tempdir at
`/tmp/pr_viz_1_validation/probe_off_test`.

| Variant | Env | Test exercising kernel boolean | Files in tempdir after |
|---------|-----|--------------------------------|------------------------|
| (a)     | NONE (`env -i PATH=$PATH`) | `cargo test -p test-harness --test f0001_debug` | 0 |
| (b)     | `YANG_BOOLEAN=1` only | `cargo test -p test-harness --test f0001_debug` | 0 |

Both variants: tempdir untouched (`ls -la` = `total 8` / dot-entries only).
The probe path requires both `YANG_CONFORMAL_PROBE=1` AND `YANG_STAGE_DUMP=<dir>`,
so the inner-`if let` is unreachable when either is unset. Confirmed: dev
tooling is fully gated, byte-identical production behavior.

The smoke test `pr_viz_1_smoke` (which IS gated `#[ignore]`) was also confirmed
to skip cleanly without `--ignored`: `1 ignored; 0 passed`.

## §2 Probe-on smoke test independent re-run

Wrote a temporary persistent runner (deleted before commit) at
`crates/test-harness/tests/_adv_viz_persistent.rs` that calls
`run_single_case(F0031, true)` so dump artifacts survive past the smoke
test's tempdir cleanup. Ran:

```
env -i PATH=… YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  YANG_STAGE_DUMP=/tmp/pr_viz_1_validation/probe_on_persistent \
  cargo test -p test-harness --test _adv_viz_persistent -- --ignored --nocapture
```

Result: **11 OBJ + 11 CSV files** under `<dir>/F0031/` — matches implementer-n's
headline. File structure:

| Stage tag                                                                 | OBJ | CSV |
|---------------------------------------------------------------------------|----:|----:|
| `stage_A`                                                                 |   ✓ |   ✓ |
| `stage_Bb`                                                                |   ✓ |   ✓ |
| `stage_B`                                                                 |   ✓ |   ✓ |
| `stage_C`                                                                 |   ✓ |   ✓ |
| `stage_E_lod=Render`                                                      |   ✓ |   ✓ |
| `stage_E_lod=Adaptive___d_epsilon__0.010636327205198413__`                |   ✓ |   ✓ |
| `stage_F.0` … `stage_F.4`                                                 | 5×  | 5×  |

= 4 conformal + 1 LOD-Render + 1 LOD-Adaptive + 5 Stage F = 11. Stage tag
sanitization works: `{`, `}`, `:`, ` `, `,` all collapsed to `_` while `=`, `.`
preserved verbatim per `sanitize_stage_tag` at `yang_integration.rs:1590`.

Note (non-blocking, see §"Recommendation" below): Stage E_lod=Adaptive's
probe fires TWICE for F0031 (once per cyl operand, both at d_epsilon=
0.010636327205198413), yet only one OBJ/CSV survives — the second call's
identical filename overwrites the first. Both probe verts/tris counts
appear in the conformal-probe stderr log, but only the SECOND is captured
on disk.

## §3 Visual sanity check — Stage A

Probe report (from stderr):
```
[conformal-probe] stage=A unpaired=0 multi_paired=0 euler_chi=4
                  well_formed=true verts=28 tris=48 unique_edges=72
```

OBJ inspection (`stage_A.obj`):
- `grep -c '^v ' = 28` → matches probe `verts=28` ✓
- `grep -c '^f ' = 48` → matches probe `tris=48` ✓
- `wc -l stage_A_labels.csv = 49` → 1 header + 48 data rows = matches OBJ tri count ✓
- OBJ format: `v {f64-20-digit-mantissa} ×3` + `f {1-indexed-idx} ×3` per spec §3 ✓
- CSV header `tri_idx,origin` per spec §5 ✓; rows like `0,A` … `47,B` ✓

Cross-stage spot check:
- Stages A/Bb/B/C: OBJ v=28 / probe verts=28 ✓ (uses `subdivided.verts`, no dup)
- Stages E_lod=Render and F.0–F.4: OBJ v=60 vs probe verts=26 — **expected**:
  the OBJ writes raw RenderMesh `vertices` (per-face-duplicated) while the
  probe's verts is `next_canon` count post nanometer-quantization at
  `oracles/conformal_mesh.rs:97`. Two different measurement domains, both
  legitimate. Risk #5 in plan was preemptive; not a defect.
- Stage E_lod=Adaptive's surviving (last-write-wins) OBJ has v=38, probe
  verts=20 — same per-face-dup explanation.

## §4 Mutation test

Commented out the OBJ-writer + CSV-writer block at Stage A's probe site
(`topology_extract.rs:1717-1736`), keeping the outer `if let Ok(_dump_dir)
= std::env::var("YANG_STAGE_DUMP")` shell intact (preserves env-var read).

Re-ran with `YANG_STAGE_DUMP=/tmp/pr_viz_1_validation/probe_on_persistent_mut`:
- **10 OBJ + 10 CSV** files (was 11+11)
- `stage_A.obj` and `stage_A_labels.csv` MISSING
- All 9 other stages unaffected (Bb, B, C, E_lod=Render, E_lod=Adaptive,
  F.0–F.4) — confirms each site is independent and Stage A's writer is
  load-bearing on file existence

Mutation reverted; ad-hoc test file deleted.

## §5 Spec deviation review — Stage C `inside` column

Implementer-n flagged Stage C's CSV as `tri_idx,origin` (no `inside`),
deviating from spec §5's row `| C | yes | yes | no | tri_idx,origin,inside |`.

**Spec permits this.** §5 closes with: *"CSV omits unavailable columns
(the table is the contract, not a stricture to emit blanks)."* The Stage C
implementation note (`topology_extract.rs:788-792`) explains:

> Inside-flag is omitted at this stage — the boolean op is not threaded
> into flood_fill_patches, and these tris are already post-survival so
> the inheritance is implicit. Per spec §5: CSV omits unavailable columns.

Acceptable. The `BoolOp` is not in scope inside `flood_fill_patches`;
adding it would require a function-signature change — out of scope for a
dev-tooling PR. Stage Bb/B carry inside fine because they have op
context locally. **No amendment needed.**

## §6 Byte-clean diff verification

Post-mutation-revert + ad-hoc-test-file deletion:

```
$ git diff --stat
 app/tests/cases/assay/results.json                 |   6 +-
 crates/kernel/src/boolean/topology_extract.rs      | 138 +++++++++++
 crates/kernel/src/boolean/yang_integration.rs      | 128 ++++++++-
 crates/kernel/src/lib.rs                           |   7 ++
 crates/kernel/src/tessellation/mod.rs              |  42 +++++
 crates/test-harness/src/assay/randomized_runner.rs |   8 ++
 6 files changed, 324 insertions(+), 5 deletions(-)
Untracked: crates/test-harness/tests/pr_viz_1_smoke.rs,
           specs/yang_pr_viz_1_per_stage_obj_dump.md
```

Matches implementer-n's tree exactly.

`results.json` change (`"generated": "2026-05-04" → "2026-05-05"` plus the
F0031 row updating from boolean-watertight failure to revolve-normals
failure) is a pre-existing implementer-n side effect: the smoke test
calls `run_single_case(..., use_kernel=true)` which triggers
`update_single_result` per `randomized_runner.rs:154-157`. The dump
infrastructure itself is non-mutating, but the smoke test's `use_kernel=true`
re-classification IS visible in `results.json`. Team-lead: consider whether
to keep this `results.json` change in the PR-VIZ-1 commit (it reflects
current truth) or revert (it's noise from running the smoke test once).

## Recommendation for next cycle (self-canaried)

**E_lod=Adaptive last-write-wins overwriting** for cases with multiple
analytical-LOD operands (F0031 cube + 2 cyls). Self-canary: confirmed via
stderr log that both Adaptive calls used `d_epsilon=0.010636327205198413`,
producing identical sanitized filenames (the bigger 36-tri operand
overwrote the 12-tri operand). Investigators wanting per-operand
visibility today get only the LAST call's mesh.

Suggested fix (low priority, NOT load-bearing on PR-VIZ-1 ship):
augment `ensure_stage_dump_case_dir` or the dump call sites to append a
per-stage atomic seq counter when the file already exists. ~5-10 LOC
change in `yang_integration.rs`. Spec §4 already mentions a fallback
counter for the unset case-id path; this would extend it to filename
disambiguation.

Per memory `feedback_adversary_recommendations_need_canary.md`: this
recommendation IS canary-verified (the stderr log shows both Adaptive
calls fire with identical d_epsilon and one OBJ survives; that IS the
overwrite I'm warning about). I am NOT recommending a cheaper proxy;
I am identifying a real second-order limitation worth a future tiny PR.

## Verdict summary

| Check | Result |
|-------|--------|
| §1 Probe-off no-op (env-unset)             | PASS — 0 files, 2 variants |
| §1 Probe-off no-op (YANG_BOOLEAN=1 only)   | PASS — 0 files |
| §2 Probe-on smoke test re-run              | PASS — 11 OBJ + 11 CSV |
| §3 Visual sanity (vert/tri counts)         | PASS (with documented per-face-dup explanation) |
| §4 Mutation test                           | PASS — Stage A files missing under mutation |
| §5 Stage C `inside` deviation              | ACCEPT — spec §5 permits |
| §6 Byte-clean diff verification            | PASS — matches implementer-n's tree |

**ACCEPT.** Ship to sub-phase 0d.
