# Kernel 1.0 Gate 3.1 Surface-Pair Matrix Summary (`agy/pair-matrix`)

**Worktree:** `/home/claude/wt/agy-pair-matrix`  
**Branch:** `agy/pair-matrix` (base commit `246757b1`; matrix run at commit `4387f08f`)  
**Gate reference:** `docs/kernel_1_0.md` §2 Gate 3, item 1 ("Every surface pair")  
**Kernel & corpus impact:** Measurement-only harness in `crates/test-harness/` plus `scripts/kernel_1_0_score.sh`. Zero kernel crates, corpus cases, `docs/kernel_1_0.md`, `docs/yang_deviations.md`, or `docs/audits/yang_2025_audit.md` modified.

> **Measurement-only harness (not a CI gate):**  
> - `surface_pair_matrix` (`crates/test-harness/tests/assay_pair_matrix.rs:391`) is `#[ignore]`d — **nothing in CI runs these 21 surface pairs automatically**.  
> - **The test binary exits `ok` even if a row categorizes `SUPPORTED_WRONG`, `ERROR`, or `TIMEOUT`.** It does not assert `0 WRONG` or `21/21 CORRECT`; its job is to measure all 21 pairs on demand, write `target/pair-matrix/report.json` (stamped with the git commit hash and seed count so `scripts/kernel_1_0_score.sh` flags stale reports), and persist any `SUPPORTED_WRONG` case as `<id>.waffle`, `<id>.meta.json`, and `<id>.verdict.json` under `target/pair-matrix/findings/`.
> - **Observed `SUPPORTED_WRONG` (`WRONG`) findings at `4387f08f`:** `0` across all `135` buildable boolean runs (`15` analytic pairs × `3` ops × `3` seeds in `--release`). `target/pair-matrix/findings/` has `0` saved findings.

---

## 1. Observed 21-Row Surface-Pair Matrix (`PAIR_MATRIX_SEEDS=3`, `--release`)

Booked foreground command (default `ASSAY_CASE_TIMEOUT_SECS=900`, matching the corpus budget):

```text
CARGO_TARGET_DIR=/home/claude/workspace/target/wt-pair-matrix \
  PAIR_MATRIX_SEEDS=3 ASSAY_JOBS=6 \
  ~/.claude/bin/reserve -c 6 --here -l "pair-matrix release 3 seeds" -- \
  cargo test --release -p test-harness --test assay_pair_matrix surface_pair_matrix -- --ignored --exact --nocapture
```

Provenance line and verbatim 21-row table emitted by `surface_pair_matrix` (`r1_pair_matrix_release.log:151-180`):

```text
run: release build, 6 job(s), cpu-budget 900s/case, 24-way host, wall 210.6s

GATE 3.1 SURFACE-PAIR MATRIX (commit 4387f08f, 3 seed(s) per (pair, op)) — C(A/NC) = CORRECT(ExactChain Agree / NotCovered)
#   pair                   union C(A/NC)/W/E/T/U    cut C(A/NC)/W/E/T/U      intersect C(A/NC)/W/E/T/U has a CORRECT customer CORRECT with volume agreement
------------------------------------------------------------------------------------------------------------------------------------------------------------
00  plane × plane          3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   YES (3/3 ops)          YES (3/3 ops, 9/9 C)
01  plane × cylinder       3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   YES (3/3 ops)          YES (3/3 ops, 9/9 C)
02  plane × cone           3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   YES (3/3 ops)          YES (3/3 ops, 9/9 C)
03  plane × sphere         3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   YES (3/3 ops)          YES (3/3 ops, 9/9 C)
04  plane × torus          3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   YES (3/3 ops)          YES (3/3 ops, 9/9 C)
05  cylinder × cylinder    3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   YES (3/3 ops)          YES (3/3 ops, 9/9 C)
06  cylinder × cone        3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   YES (3/3 ops)          YES (3/3 ops, 9/9 C)
07  cylinder × sphere      0C(0A/0NC)/0W/3E/0T/0U   0C(0A/0NC)/0W/3E/0T/0U   0C(0A/0NC)/0W/3E/0T/0U   NO (0/3 ops)           NO (0/3 ops)
08  cylinder × torus       3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   YES (3/3 ops)          YES (3/3 ops, 9/9 C)
09  cone × cone            3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   YES (3/3 ops)          YES (3/3 ops, 9/9 C)
10  cone × sphere          0C(0A/0NC)/0W/3E/0T/0U   0C(0A/0NC)/0W/3E/0T/0U   0C(0A/0NC)/0W/3E/0T/0U   NO (0/3 ops)           NO (0/3 ops)
11  cone × torus           3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   3C(2A/1NC)/0W/0E/0T/0U   YES (3/3 ops)          YES (3/3 ops, 8/9 C)
12  sphere × sphere        3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   3C(2A/1NC)/0W/0E/0T/0U   YES (3/3 ops)          YES (3/3 ops, 8/9 C)
13  sphere × torus         0C(0A/0NC)/0W/3E/0T/0U   0C(0A/0NC)/0W/3E/0T/0U   0C(0A/0NC)/0W/3E/0T/0U   NO (0/3 ops)           NO (0/3 ops)
14  torus × torus          3C(3A/0NC)/0W/0E/0T/0U   3C(3A/0NC)/0W/0E/0T/0U   3C(2A/1NC)/0W/0E/0T/0U   YES (3/3 ops)          YES (3/3 ops, 8/9 C)
15  plane × B-spline       NOT BUILDABLE (no B-spline face source)                                    NO                     NO (NOT BUILDABLE (no B-spline face source))
16  cylinder × B-spline    NOT BUILDABLE (no B-spline face source)                                    NO                     NO (NOT BUILDABLE (no B-spline face source))
17  cone × B-spline        NOT BUILDABLE (no B-spline face source)                                    NO                     NO (NOT BUILDABLE (no B-spline face source))
18  sphere × B-spline      NOT BUILDABLE (no B-spline face source)                                    NO                     NO (NOT BUILDABLE (no B-spline face source))
19  torus × B-spline       NOT BUILDABLE (no B-spline face source)                                    NO                     NO (NOT BUILDABLE (no B-spline face source))
20  B-spline × B-spline    NOT BUILDABLE (no B-spline face source)                                    NO                     NO (NOT BUILDABLE (no B-spline face source))
------------------------------------------------------------------------------------------------------------------------------------------------------------
Score: 12/21 pairs have a CORRECT customer (12/21 all 3 ops); 12/21 CORRECT with volume agreement (12/21 all 3 ops); 15 buildable, 6 not buildable
Totals across 135 buildable runs: 108 CORRECT (105 Agree, 3 NotCovered) / 0 WRONG / 27 ERROR / 0 TIMEOUT / 0 UNSUPPORTED
Exact-volume verdicts (ExactChain 256-cell lattice): 105 Agree / 3 NotCovered / 0 Flag / 27 not reached
```

Gate 3 section printed by `./scripts/kernel_1_0_score.sh` at `4387f08f` when `target/pair-matrix/report.json` is present (if `HEAD` later advances past the report's `commit` stamp, `kernel_1_0_score.sh` appends `[STALE vs HEAD <sha>]`):

```text
Gate 3 — Capable
   surface-pair matrix (target/pair-matrix/report.json, commit 4387f08f, 3 seed(s)/op): 12/21 pairs have a CORRECT customer (12/21 all 3 ops; 12/21 CORRECT with volume agreement (12/21 all 3 ops); 15 buildable, 6 NOT BUILDABLE)
   runs: 108C(105A/3NC) / 0W / 27E / 0T / 0U over 135
   buildable pairs without a CORRECT customer (3): cylinder × sphere, cone × sphere, sphere × torus
```

### 1.1 What Each `SUPPORTED_CORRECT` Verdict Rests On

- **`derive_meta` does NOT supply a closed-form volume:** `assay::prospect::derive_meta` (`crates/test-harness/src/assay/prospect/mod.rs:251`) sets `expected_volume: None`. It supplies `expect_rebuild_error: false`, a loose `max_bbox_extent` bound, and heuristic `volume_monotonicity` / `check_volume_magnitude` (`volume ∈ [1e-8, 1e8]·scale³`) expectations.
- **Volume check inside `categorize`:** Inside `categorize` (`crates/test-harness/src/assay/categorize.rs:540-602`), the only volume oracle covering general union/cut/intersect booleans is `ExactChain`'s 256-cell lattice (`exact_volume_verdict`, `crates/test-harness/src/assay/exact_membership.rs:1854`, `EXACT_VOLUME_CELLS = 256`, `EXACT_VOLUME_TOO_COARSE_REL = 0.25`; note that `volume_oracle_doc::evaluate_composition` at `categorize.rs:513-525` runs only on pure-union chains without cuts).
- **Why `Agree` vs `NotCovered` is recorded explicitly:** When `exact_volume_verdict` returns `ExactVolumeVerdict::NotCovered(reason)`, `categorize` pushes no failure (`categorize.rs:595`), so a case that passes watertightness, manifoldness, non-self-intersection, `on_surface`, `mesh_on_surface`, and `check_volume_magnitude` still reads `SUPPORTED_CORRECT` **without** an exact-volume agreement check. In addition, every revolve in `pair_matrix` (`cone`, `sphere`, `torus`) uses an oblique revolve axis, and the `ExactChain` lattice has an open rotation-dependence defect on revolves (`assay_metamorphic`).
- **Observed `ExactChain` breakdown across the `108` `SUPPORTED_CORRECT` runs:**
  - **`105` `Agree`** (`0` `Flag`).
  - **`3` `NotCovered`** — all three are small `intersect` lenses whose boundary cube fraction exceeded `EXACT_VOLUME_TOO_COARSE_REL` (`25 %`) at `256` cells (`r1_pair_matrix_release.log:290, 299, 316`):
    1. `pm_11_cone_torus_intersect_s2`: `NotCovered(lattice too coarse: surface cubes are 27 % of the volume at 256 cells (h = 2.439e-2))`
    2. `pm_12_sphere_sphere_intersect_s2`: `NotCovered(lattice too coarse: surface cubes are 28 % of the volume at 256 cells (h = 3.166e-2))`
    3. `pm_14_torus_torus_intersect_s1`: `NotCovered(lattice too coarse: surface cubes are 32 % of the volume at 256 cells (h = 2.452e-2))`
  - In each of those three `(pair, intersect)` cells (`cone × torus`, `sphere × sphere`, `torus × torus`), the other `2` seeds achieved `Agree` (`2A/1NC`), so `12/21` pairs still have a `CORRECT` customer **with volume agreement** across all 3 ops (`12/21 all 3 ops`).

---

## 2. What Was Built

### 2.1 Shared CPU-Budgeted Case Runner (`crates/test-harness/src/assay/cpu_runner.rs`) and `categorize_with_exact_volume` (`crates/test-harness/src/assay/categorize.rs`)

- `categorize_with_exact_volume` (`categorize.rs:164`): executes the corpus verdict pipeline and returns `(CaseOutcome, Option<ExactVolumeVerdict>)` (`None` when replay stopped before reaching `exact_volume_verdict`); `categorize` (`categorize.rs:157`) delegates `.0` with unchanged behavior.
- Lifted the CPU-budgeted in-process and parallel subprocess case runner out of `crates/test-harness/tests/assay_kv2.rs` into `test_harness::assay::cpu_runner` (exported in `crates/test-harness/src/assay/mod.rs:22`) so `tests/assay_kv2.rs` and `tests/assay_pair_matrix.rs` execute through the same timeout and subprocess plumbing:
  - `OutcomeWithExtra` (`cpu_runner.rs:26`) and `RunProvenance` (`cpu_runner.rs:31`): captures `build_mode`, `jobs`, `budget_kind` (`"cpu"` on Linux with `jobs > 1`, `"wall"` otherwise), `per_case_budget_secs`, `wall_secs`, `host_parallelism`, and `generated_at_unix`.
  - `default_assay_jobs` (`cpu_runner.rs:105`): reads `ASSAY_JOBS` (default `4` capped at host parallelism, minimum `1`).
  - `emit_assay_outcome` (`cpu_runner.rs:121`), `emit_assay_outcome_with_extra` (`cpu_runner.rs:127`), and `ASSAY_OUTCOME_PREFIX` (`cpu_runner.rs:22`): structured stdout protocol (with optional `"extra"` JSON payload) between parent worker threads and `single_case` child processes.
  - `run_with_cpu_timeout` (`cpu_runner.rs:142`) and `run_with_cpu_timeout_map` (`cpu_runner.rs:151`): run one case on a dedicated thread watching `/proc/self/task/<tid>/stat` CPU seconds on Linux plus a `(4 * cpu_budget).max(cpu_budget + 120s)` wall safety cap.
  - `run_case_subprocess` (`cpu_runner.rs:228`) and `run_case_subprocess_with_extra` (`cpu_runner.rs:234`): spawn `current_exe() --exact single_case --ignored --nocapture` with `ASSAY_CASE=<id>`, poll `/proc/<pid>/stat` user+system ticks, and kill timed-out children cleanly so no orphan process lingers.
  - `run_batch` (`cpu_runner.rs:334`) and `run_batch_with_extra` (`cpu_runner.rs:350`): run case IDs serially in-process when `jobs <= 1` or across `jobs` scoped worker threads when `jobs > 1`, returning outcomes in deterministic input order.

### 2.2 Generator, General-Position Verifier & Report Model (`crates/test-harness/src/assay/pair_matrix.rs`)

- **21-pair enumeration:** `SurfaceKind` (`pair_matrix.rs:40`), `MatrixOp` (`pair_matrix.rs:129`), `SurfacePair` (`pair_matrix.rs:166`), and `ALL_SURFACE_PAIRS: [SurfacePair; 21]` (`pair_matrix.rs:216`) enumerate the 15 canonical unordered analytic-analytic pairs (`index` `0..=14`, including self-pairs) followed by the 6 pairs involving `SurfaceKind::BSpline` (`index` `15..=20`).
- **Surface feature recipes (`SurfaceKind::feature_recipe`, `pair_matrix.rs:102-111`):**

| Surface (`SurfaceKind`) | `kernel-v2` `surface_type` | Feature-engine recipe (`ModelBuilder`) |
| :--- | :--- | :--- |
| `plane` (`SurfaceKind::Plane`) | `"planar"` | `extrude_no_merge(polygon_sketch: 5-gon prism face)` |
| `cylinder` (`SurfaceKind::Cylinder`) | `"cylindrical"` | `extrude_no_merge(true_circle_sketch)` |
| `cone` (`SurfaceKind::Cone`) | `"conical"` | `revolve(polygon_sketch: on-axis frustum 4-gon, 360 deg)` |
| `sphere` (`SurfaceKind::Sphere`) | `"spherical"` | `revolve(true_circle_sketch: on-axis circle, 360 deg)` |
| `torus` (`SurfaceKind::Torus`) | `"toroidal"` | `revolve(true_circle_sketch: off-axis circle, 360 deg)` |
| `B-spline` (`SurfaceKind::BSpline`) | — | `NOT BUILDABLE (no B-spline face source)` |

Every pair's per-operand recipe is reported by `SurfacePair::feature_sources_summary` (`pair_matrix.rs:190`) and stored in `PairMatrixRow::{feature_a, feature_b}` (`pair_matrix.rs:1455-1456`):
- Rows `00`–`14`: `A (<surface_a>) = <recipe(surface_a)>; B (<surface_b>) = <recipe(surface_b)>`, followed by `boolean_union` (`MatrixOp::Union`), `boolean_subtract` (`MatrixOp::Cut`), or `boolean_intersect` (`MatrixOp::Intersect`).
- Rows `15`–`20`: `NOT BUILDABLE (no B-spline face source)` (`NOT_BUILDABLE_BSPLINE`, `pair_matrix.rs:36`).

- **Why the 6 B-spline pairs are `NOT BUILDABLE (no B-spline face source)` today:**
  1. `kernel_v2::arena::Surface` (`crates/kernel-v2/src/arena.rs:245-350`) defines only five variants: `Plane`, `Cylinder`, `Cone`, `Torus`, and `Sphere` — there is no `BSpline` variant in `kernel-v2` yet.
  2. `feature_engine::types::Operation` (`crates/feature-engine/src/types.rs:605-777`) has `Extrude`, `Revolve`, `Fillet`, `Chamfer`, `Shell`, `BooleanCombine`, `DatumPlane`, and `ScaleBody`, with no `Loft` operation yet (`crates/nurbs-rs` landed Loft L1a curve/surface math in commit `629442b6`; Loft L1b kernel/engine integration has not landed).
  3. Sketch `spline_segments` (`crates/kernel-v2/src/adapter.rs:23, 1163`) either extrude as planar chord polygons via polyline points or return `KernelError::NotSupported`.
  4. STEP B-spline faces (`crates/feature-engine/src/import_tiers.rs:13-18`) route to the mesh import tier ("not a boolean operand").

- **Anchored general-position generator & analytic check (`draw_pair_operands`, `pair_matrix.rs:1090`, and `verify_general_position`, `pair_matrix.rs:934`):**
  - Seeds an `assay::prospect::gen3::Rng` deterministically from `(pair.index, seed)` (`seeded_rng`, `pair_matrix.rs:716`) and samples an off-origin, non-axis-aligned anchor point `crossing_point` (`p_cross`) through which both target surfaces are constructed to pass (`|target_surface_residual(p_cross)| <= 1e-11`, away from caps, poles, equators, and meridian/longitude seams).
  - Enforces transverse surface normals at `p_cross` (`0.18 <= |n_a · n_b| <= 0.82`), oblique and strictly skew 3D axes whenever both operands carry axes (`0.15 <= |a_a · a_b| <= 0.85` and 3D skew distance `>= 0.20`, so axes never coincide or intersect), distinct radii / cone half-angles / torus radii, separated sphere/torus centers (`>= 0.55` from partner centers and `>= 0.25` from partner axes), non-parallel planar faces across A and B (`|n_i · n_j| <= 0.985`), no auxiliary plane-to-sphere or plane-to-cylinder tangency, and seam/pole clearance (`>= 0.16`) from all partner boundary surfaces.
  - Checks sphere pole/seam clearance (`OperandGeom::seam_vertices`, `pair_matrix.rs:591`) at the sphere's real oblique revolve poles `center ± R·axis_dir` and revolve sketch meridian `center + R·(sketch_normal × axis_dir)` (in addition to `assemble_closed_sphere`'s canonical world `±Z / +X` frame).
  - Confirms via closed-form `ExactChain` (`check_local_boolean_witnesses`, `pair_matrix.rs:1134`) that all four local signed-normal quadrants `(±n_a, ±n_b)` around `p_cross` are realized as `(in A, in B)`, `(in A, out B)`, `(out A, in B)`, and `(out A, out B)` — certifying non-empty, distinct `A ∪ B`, `A \ B`, and `A ∩ B` around the target surface crossing without calling the kernel.
  - Builds `.waffle` JSON via `ModelBuilder::mock()`, normalizes UUIDs and `document.{created,modified}` timestamps (`canonicalize_document`, `pair_matrix.rs:1245`) so output is byte-for-byte deterministic, and derives non-exact metadata (`expect_rebuild_error = false`, `expected_volume = None`, loose bbox extent, heuristic volume monotonicity/magnitude) with `assay::prospect::derive_meta` (`pair_matrix.rs:1226`).

- **Report, exact-volume ledger & scoreboard integration (`ExactVolumeRecord`, `pair_matrix.rs:1318`; `resolve_git_commit`, `pair_matrix.rs:1499`; `build_report`, `pair_matrix.rs:1530`; `format_report_table`, `pair_matrix.rs:1678`; `scripts/kernel_1_0_score.sh:68-115`):**
  - Stamps `commit` (`git rev-parse --short HEAD`) and `seeds_per_op` into `PairMatrixReport` (`report.json`).
  - Records per-case `ExactVolumeRecord` (`Agree`, `NotCovered`, `Flag`) in `PairCaseRecord::exact_volume` and prints a per-case exact-volume ledger underneath the 21-row table.
  - Aggregates per-`(pair, op)` `OpCounts` (`CORRECT (Agree / NotCovered) / WRONG / ERROR / TIMEOUT / UNSUPPORTED`) and computes `has_correct_customer`, `all_ops_have_correct_customer`, `has_correct_with_volume_agreement`, and `all_ops_have_correct_with_volume_agreement`.
  - Updates `scripts/kernel_1_0_score.sh` Gate 3 to display `commit <hash>` (flagging `[STALE vs HEAD <head>]` when `report.json`'s `commit` differs from `HEAD`), `<N> seed(s)/op`, `CORRECT with volume agreement`, and `C(A/NC)` run totals.

---

## 3. Finding for the Kernel Owner: Root Cause of the 3 Failing Analytic Pairs (`27/27 ERROR`)

Twelve of the 15 buildable analytic pairs score `9/9 SUPPORTED_CORRECT` (`3/3` seeds on each of `union`, `cut`, and `intersect`, `108/108` runs; `105 Agree`, `3 NotCovered`). The remaining 3 buildable analytic pairs fail `9/9` (`27/27` runs) with `Category::Error`:

- Row `07` (`cylinder × sphere`): `0C(0A/0NC)/0W/3E/0T/0U` on `union`, `cut`, and `intersect`
- Row `10` (`cone × sphere`): `0C(0A/0NC)/0W/3E/0T/0U` on `union`, `cut`, and `intersect`
- Row `13` (`sphere × torus`): `0C(0A/0NC)/0W/3E/0T/0U` on `union`, `cut`, and `intersect`

Every single one of the 27 errors fails at the exact same Stage-3 SSI curve-selection check:

```text
BooleanFailed("yang-rs: Stage-3 SSI refinement failed for intersection edge (.., ..): AmbiguousCurve { candidates: 1, matched: 0 }")
```

Read-only trace of why:
1. In `crates/ssi-rs/src/lib.rs`, non-coaxial `sphere_cylinder` (`:1262, 1304`), non-coaxial `sphere_cone` (`:1384, 1432`), and `torus_general` (`:452, 614-617`) all return `Ok(vec![SsiCurve::SurfacePair { a, b }])` (`candidates == 1`) with one operand equal to `ssi_rs::QuadricSurface::Sphere`.
2. In `crates/yang-rs/src/stage3_ssi.rs`, `quadric_to_surface` (`:185`) already maps `QuadricSurface::Sphere` to `Surface::Sphere`, so `SurfacePair` refinement itself supports spheres.
3. However, `curve_contains_point` (`crates/yang-rs/src/stage3_ssi.rs:458-530`) checks `SsiCurve::SurfacePair { a, b }` via a local `quadric_residual` closure that matches only `Cylinder` (`:461`), `Cone` (`:480`), `Torus` (`:499`), and `Plane` (`:515`), falling through to `_ => None` (`:522`) for `ssi_rs::QuadricSurface::Sphere` (whose stale comment at `:455-457` predates the promotion of non-coaxial `sphere_cylinder`, `sphere_cone`, and `sphere × torus` to `SurfacePair`). Because `quadric_residual` returns `None`, `curve_contains_point` unconditionally returns `false` (`matched == 0`), producing `AmbiguousCurve { candidates: 1, matched: 0 }` on 100% of general-position `cylinder × sphere`, `cone × sphere`, and `sphere × torus` booleans. (`plane × sphere` and `sphere × sphere` emit `SsiCurve::Circle` rather than `SsiCurve::SurfacePair`, so both pass `9/9`.)

Note why `crates/test-harness/tests/assay_generative_pairs.rs` did not catch this earlier: `assay_generative_pairs.rs` currently generates `plane`, `cylinder`, `cone`, and `torus` solids (`PrimKind` has no `Sphere` variant).

---

## 4. Tests Added & Exact `test result:` Lines Observed

### 4.1 Tests in `crates/test-harness/src/assay/cpu_runner.rs` and `crates/test-harness/tests/assay_pair_matrix.rs`

1. `assay::cpu_runner::tests::outcome_line_round_trips` (`cpu_runner.rs:495`, always-on): verifies `ASSAY_OUTCOME ` JSON serialization/parsing round-trip (including multi-line `detail` and optional `"extra"` JSON payload) and mismatched-ID rejection.
2. `enumerates_all_21_unordered_surface_pairs` (`assay_pair_matrix.rs:103`, always-on): verifies `ALL_SURFACE_PAIRS` contains 21 unique canonical unordered pairs (`a <= b`), `15` buildable analytic pairs with feature recipes, `6` `BSpline` pairs reporting `NOT BUILDABLE (no B-spline face source)`, and `parse_case_id` round-trips across all `(pair, op)` combinations.
3. `generated_documents_are_deterministic_and_in_general_position` (`assay_pair_matrix.rs:140`, always-on): across all 15 buildable pairs × seeds `0..3` × all 3 ops (`135` documents), verifies `verify_general_position`, byte-for-byte deterministic `.waffle` JSON output, `BooleanCombine` feature presence, `derive_meta` metadata (`!expect_rebuild_error`), and closed-form `ExactChain::from_waffle` coverage (`2` ops, `1` merge).
4. `each_surface_recipe_builds_its_target_analytic_surface_in_kernel_v2` (`assay_pair_matrix.rs:193`, always-on): builds one standalone operand for each of the 5 analytic surface kinds through `ModelBuilder::kernel_v2()`, asserting zero engine errors, expected `face_signatures` surface type (`"planar"`, `"cylindrical"`, `"conical"`, `"spherical"`, `"toroidal"`), watertight tessellation, and positive signed mesh volume.
5. `report_and_table_distinguish_volume_agree_not_covered_and_flag` (`assay_pair_matrix.rs:236`, always-on): verifies `DEFAULT_CASE_TIMEOUT_SECS == 900` (`parse_timeout_secs`), commit stamp propagation, and that `build_report_at_commit` / `format_report_table` distinguish `SUPPORTED_CORRECT` with `ExactVolumeRecord::Agree` (`has_correct_with_volume_agreement == true`) from `SUPPORTED_CORRECT` with `ExactVolumeRecord::NotCovered` (`has_correct_customer == true`, `has_correct_with_volume_agreement == false`) and `SUPPORTED_WRONG` with `ExactVolumeRecord::Flag`.
6. `sphere_revolve_poles_along_oblique_axis_are_checked_for_clearance` (`assay_pair_matrix.rs:303`, always-on): places an oblique sphere whose revolve pole `center + R·axis_dir` lies on a partner plane while world `±Z / +X` stay clear, asserting `verify_general_position` rejects it (`seam/pole vertex too close to partner surface`).
7. `single_case` (`assay_pair_matrix.rs:363`, `#[ignore]`, measurement helper): runs one `ASSAY_CASE=pm_<idx>_<a>_<b>_<op>_s<seed>` document under `evaluate_pair_case_with_timeout` (`DEFAULT_CASE_TIMEOUT_SECS = 900`) and emits `ASSAY_OUTCOME` (including `ExactVolumeRecord`) for manual diagnosis and `run_batch_with_extra` child workers.
8. `surface_pair_matrix` (`assay_pair_matrix.rs:391`, `#[ignore]`, **measurement-only**): runs all `15 × 3 × PAIR_MATRIX_SEEDS` buildable boolean cases through `run_batch_with_extra` + `categorize_with_exact_volume`, prints the 21-row table and per-case exact-volume ledger, writes `target/pair-matrix/report.json`, and saves any `SUPPORTED_WRONG` artifacts under `target/pair-matrix/findings/`. Exits `ok` regardless of row categories; not run in CI.

### 4.2 Exact Verification Commands & `test result:` Lines

All heavy commands were executed in the foreground booked via `~/.claude/bin/reserve` with `CARGO_TARGET_DIR=/home/claude/workspace/target/wt-pair-matrix`:

- **Formatting & lints:**
  - `cargo fmt --check -p test-harness` → exit `0`.
  - `cargo clippy --all-targets -p test-harness -- -D warnings` → exit `0` (`0` warnings).
- **Always-on `cpu_runner` unit test (`cargo test -p test-harness --lib cpu_runner`):**
  ```text
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 188 filtered out; finished in 0.00s
  ```
- **Always-on `assay_pair_matrix` tests (`cargo test -p test-harness --test assay_pair_matrix`):**
  ```text
  test result: ok. 5 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 1.81s
  ```
- **3-seed `--release` 21-pair matrix at `4387f08f` (`PAIR_MATRIX_SEEDS=3 ASSAY_JOBS=6 cargo test --release -p test-harness --test assay_pair_matrix surface_pair_matrix -- --ignored --exact --nocapture`):**
  ```text
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 210.65s
  ```
- **Single-case subprocess/manual entry smoke at `4387f08f` (`ASSAY_CASE=pm_08_cylinder_torus_cut_s0 cargo test --release -p test-harness --test assay_pair_matrix single_case -- --ignored --exact --nocapture`):**
  ```text
  pm_08_cylinder_torus_cut_s0: SUPPORTED_CORRECT [exact_volume: Agree(rel=-2.02e-3, band=5.69e-2)] (2.2s) — all checks passed; on_surface: max 5.940e-14 m, rel 1.368e-14, solids 1 at (-6.259172e-1, 2.287802e0, 3.958170e0); mesh_on_surface: plane rel 1.662e-16, chord ratio 8.700e-1 at (-6.232977e-1, 1.978859e0, 4.482380e0), solids 1; exact_vs_mesh: exact 0.000000000e0 mesh 0.000000000e0 rel NaN bound_ratio NaN (+1 unmeasured); monotonicity: 0 checked, 0 not covered, 0 violation(s)
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 2.22s
  ```

---

## 5. What Was Not Finished & Uncertainties / Caveats

### 5.1 What Was Not Finished
- Nothing in `AGY_BRIEF_PAIR_MATRIX.md` or `REVIEW_PAIR_MATRIX_1.md` was left unfinished.
- Per explicit scope boundaries, the missing `QuadricSurface::Sphere` arm in `crates/yang-rs/src/stage3_ssi.rs:522` was **not** patched (the kernel owner has a fix in flight), no B-spline face constructor was added, no cases were promoted into `app/tests/cases/assay/`, and `docs/kernel_1_0.md` was left untouched.

### 5.2 Uncertainties & Caveats
1. **`ExactChain` lattice coarseness & oblique-revolve rotation defect:** Three small-lens `intersect` runs (`pm_11_cone_torus_intersect_s2`, `pm_12_sphere_sphere_intersect_s2`, `pm_14_torus_torus_intersect_s1`) exceed `EXACT_VOLUME_TOO_COARSE_REL = 0.25` at `256` cells (`27 %`, `28 %`, and `32 %` surface cubes) and therefore read `SUPPORTED_CORRECT` without volume agreement (`NotCovered`), and `ExactChain` has an open rotation-dependence defect on oblique revolves (`assay_metamorphic`). Both are exposed per case via `ExactVolumeRecord` and the `"CORRECT with volume agreement"` column.
2. **Auxiliary cap faces on bounded solids:** Real feature-engine solids are closed 3-manifolds, so a `cylinder` (extruded circle) or `cone` (revolved 4-gon frustum) necessarily carries two planar end-cap faces in addition to its curved target lateral face, while `sphere` and `torus` are single-face closed revolves without caps. Every generated pair anchors `crossing_point` strictly inside both target lateral/curved faces (`alpha ∈ [0.36, 0.64]` along cylinder/cone height) and checks local 4-quadrant witnesses around `crossing_point` so the target surface × target surface intersection curve is always exercised; secondary intersections with planar caps may also occur elsewhere on the closed solid, just as in real CAD models.
3. **`torus × torus` `union` CPU cost (`105.4s`–`140.8s` per seed in `--release`):** At `4387f08f`, `pm_14_torus_torus_union_s{0,1,2}` took `105.4s`, `136.6s`, and `140.8s` in `--release` (compared with `0.7s`–`20.3s` for `torus × torus` `cut` and `intersect`). `DEFAULT_CASE_TIMEOUT_SECS` is `900` seconds (`assay_pair_matrix.rs:41`), matching the corpus's own `900s` CPU budget.
4. **Standalone full-sphere introspection vs tessellation:** `KernelIntrospect::solid_volume` currently rejects a standalone 1-face revolved sphere (`CurvedGeometryMismatch: sphere patch boundary reverses direction at a vertex`), whereas `categorize` computes volume from `tessellate_live_with_tol` / `tessellate` + `mesh_signed_volume` against `ExactChain`'s 256-cell lattice. Accordingly, `each_surface_recipe_builds_its_target_analytic_surface_in_kernel_v2` checks `face_signatures` + watertight tessellation + positive `mesh_signed_volume`.
