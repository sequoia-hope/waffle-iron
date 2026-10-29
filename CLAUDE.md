# Waffle Iron — Claude Code Session Guide

Global instructions for any Claude Code session working on Waffle Iron.

## Document Precedence

When rules conflict, the following precedence applies (highest first):

1. `/governance/*` — Engineering Constitution, FIP, DoD, Architectural Invariants
2. `/agents/*` — Roles, skills, orchestration
3. `AGENTS.md` — Repo-level team structure
4. This file — Session workflow and coding conventions
5. Sub-project `CLAUDE.md` files — Project-specific instructions

## Kernel: kernel-v2 (migration COMPLETE 2026-06-11)

**The legacy `crates/kernel/` is DELETED.** The app, feature-engine, and all
tests run on `kernel-v2` through the `Kernel`/`KernelIntrospect` traits in
`waffle_types::kernel` (implemented by `kernel_v2::KernelV2Adapter`). The WASM
bundle is built from this stack on **stable Rust** with standard `wasm-pack`.

### Crate layout

```
crates/waffle-types/     — public types + the kernel contract (traits, shared
                           types, units; MockKernel behind the `mock-kernel` feature)
crates/cad-primitives/   — shared geometry types & constants (Point3, Vector3, BoolOp, …)
crates/nurbs-rs/         — shared Piegl & Tiller B-spline / NURBS math (deps cad-primitives)
crates/cherchi-rs/       — Cherchi 2020+2022 mesh boolean (pure Rust, clean-room predicates)
crates/ssi-rs/           — analytical SSI solvers (Patrikalakis Ch.5)
crates/yang-rs/          — Yang 2025 pipeline (deps cherchi-rs + ssi-rs)
crates/kernel-v2/        — clean B-Rep + tessellation + Kernel trait adapter (deps yang-rs)
```

Dependency layering is compiler-enforced via each crate's `Cargo.toml`. A crate
higher in the stack may not be imported by one lower down.

### Agent routing rules

| Task | Crate to work in |
|---|---|
| Mesh boolean (Cherchi port) | `crates/cherchi-rs/` |
| Analytical SSI solver | `crates/ssi-rs/` |
| B-spline / NURBS math | `crates/nurbs-rs/` |
| Yang pipeline stage | `crates/yang-rs/` |
| B-Rep / Euler ops / primitives / tessellation / trait adapter | `crates/kernel-v2/` |
| Shared primitive type (Point3 etc.) | `crates/cad-primitives/` |
| Kernel trait / shared kernel types / MockKernel | `crates/waffle-types/` (`src/kernel/`) |

### Known capability boundaries (NotSupported, loud)

kernel-v2 returns typed `KernelError::NotSupported` (or typed yang errors) for
operations it does not implement yet. These surface as error toasts in the app
and as `#[ignore]`-tagged tests / `test.skip` GUI quarantines in the suites.
They are ROADMAP ITEMS, not bugs:

- **Coplanar boolean inputs** the §4.5.5 Stage-0 overlay cannot resolve —
  the typed `coplanar input face pair` wall stays LOUD. Two different
  refusals share that text (`map_yang_error`, `kernel-v2/src/boolean/mod.rs`):
  cherchi-rs's coplanar deferral (N17, RESOLVED 2026-10-27 — nothing
  constructs it any more) and Stage 0's n-ary refusals (N25). R0023's
  2026-10-13 wall was Stage 0's `mixed-arc-lateral-unpaired` (N25, the named
  M8 follow-up), NOT N17; R0023 now fails earlier, as kernel-v2
  `TessellationFailed { FaceId(9), "outer loop has fewer than 3 vertices" }`
  (a cap segment face)
- **Holed revolve profiles** and **arc/spline profiles without a chord
  polygon** — typed `NotSupported` at profile staging (no corpus case)
- **Fillet / chamfer / shell** — deferred indefinitely (see below)

Everything else the roadmap once listed here has LANDED and is no longer a
`NotSupported` boundary — STEP export (2026-09-08, `kernel_v2::step_export`:
analytic AP214, multi-body and placed assembly export, truck round-trip
oracle in `wasm-bridge/tests/step_export_roundtrip.rs`), revolve (KV6, incl. cones, tori, spheres, on-axis
lathe shapes and the apex cone as a boolean operand), cylinder×cylinder and
other degree-4 SSI (M5 surface-pair curves, re-entering chained booleans
since K11, 2026-09-04), gear/arc profiles. What remains for those families is
the loud ERROR tail (typed yang Stage-3/4/5 STOPs on specific geometry —
`docs/yang_tail_triage.md`), which is kernel capability work, not a
`NotSupported` marker. A stale `#[ignore = "KV6…"]` or `test.skip` quarantine
is itself a defect: un-quarantine when the capability lands, in the same PR
(grep the milestone tag, e.g. `M8`, in `#[ignore =` and `test.skip`).

## DEFERRED INDEFINITELY: Fillet, Chamfer, Shell

**Fillet, chamfer, and shell operations are DEFERRED INDEFINITELY. Do NOT work on them.**

Experimental implementations exist (Sprint 18) and MockKernel tests pass, but these operations depend on boolean reliability which is itself under development. UI dialogs display warning banners with disabled Apply buttons.

**Never suggest fillet, chamfer, or shell as "next steps" or "what to work on."** If a session plan includes fillet/chamfer/shell work, skip it and choose a different task.

## Yang/Cherchi Deviations

Track known divergences between the implementation and Yang 2025 / Cherchi 2022 in `docs/yang_deviations.md`. Before working on a Yang stage, compare the code against the paper section it claims to implement. A deviation discovered mid-investigation halts the cycle — the deviation is the bug; do not debug downstream symptoms. (Rule: `feedback_yang_only.md`.)

**A delegation is not an approval (governance A15.7, 2026-10-14).** The
2026-07-17 "no opinion on A B C" answer that made forty-three ledger entries
PERMANENT was not a sign-off; those entries (plus N57, N62) are `DELEGATED` —
open paper-compliance debt, re-adjudicated when their mechanism is next
touched. Record any "no opinion" / "I don't care" as DELEGATED, never
PERMANENT; ask for a sign-off in prose with a paper cite, never as a menu; a
P5 self-sign proves a fix SAFE, not FAITHFUL. **The P2 same-commit ledger
rule already existed and was skipped for 38 conversions (2026-10-05 →
10-14)** — check `git log -1 -- docs/yang_deviations.md` against the kernel
log at session start; the catch-up debt and its order (N5 curved Stage 1,
N29 §4.5.3 victim rule, the N47/N55/N56 weld band, then the rest) is the
ledger's 2026-10-14 directive. **The cleanup's plan of record is
`specs/yang_faithful_cleanup.md`** — read it before any yang-rs change: it
maps every Stage-1 sample arm and Stage-4 repair pass to the paper
mechanism it stands in for (§4.5.2, §4.4.1) and fixes the order of
retirement; a new arm or pass is not added, the paper mechanism is built.
The pre-commit hook now blocks a kernel commit whose ledger did not move
(`SKIP_LEDGER_CHECK=1` only for a mechanism-free refactor).

## Current Priorities

When asked "what should I work on?", choose from these areas **in order**.
Do NOT skip to lower-priority items because they are easier.

**Review remediation (adopted 2026-10-29, `docs/review/remediation_2026-10-28.md`).**
F2 has ONE owner, and no other kernel milestone (F3, F5, F6) starts until F2
lands; parallel agents take the plan's non-kernel lanes (A, S, D, P, U, L, H)
or its gate/oracle items instead. F2's repair-pass retirements wait for the
harness on-surface oracle (plan O1–O3), because today's corpus cannot see an
off-surface vertex.

1. **Faithfulness to Yang 2025 — the STANDING PRIORITY (2026-10-27).**
   **`docs/yang_faithfulness_roadmap.md` sets the work order:** take the
   first unchecked item of the earliest open milestone (F0 bookkeeping →
   F1 classify-after-update N80 → F2 §4.4.1 one per-patch update → F3 §4.5
   locality / collection / Case-IV rule-out / transits → F4 §4.3
   tangency & Gauss map → F5 Stage 1 as §4.1 → F6 Stage 0 & emission → F7
   adjudication). Progress is the roadmap's scoreboard (ledger debt 64,
   matrix not-faithful rows 100 at adoption) **with the corpus held**, never
   the corpus score alone. Tail cases are *customers* of a milestone, not
   work items of their own (roadmap §4); a WRONG still preempts everything.
   - **Plans of record:**
     - `docs/yang_faithfulness_roadmap.md` — the sequence and the finish line (above).
     - `docs/yang_functional_roadmap.md` — `LabeledArrangement` interface and milestones M0–M8 (M0, M1, M2, M6, M7 and Phase-6 migration COMPLETE; `cherchi-rs` is live in production).
     - `specs/yang_faithful_cleanup.md` — ACTIVE cleanup plan (from 2026-10-14; governance A15.7: *"ad hoc patches will destroy a kernel like this. the fixes must be principled. the principles are the paper"*). Retires Stage-1 pre-emptive sample arms and Stage-4 post-hoc repair passes in favor of the paper's §4.5.2 local refinement loop (`boolean::refine_loop`) and §4.4.1 mesh updating (`stage4_construct::update_patches`).
     - `docs/audits/yang_2025_audit.md` — 194-row paper-ordered compliance matrix (rebuilt 2026-10-16) mapping every procedural statement of §3–§4 to its code site, status, ledger ID, and pin. Read the rows for your section before changing a stage, and update them in the same commit as `docs/yang_deviations.md`.
     - `docs/yang_tail_triage.md` — Full chronological conversion log (P0001–P0062, seeds 1–5) and per-case triage ledger.
   - **Current canonical corpus status (2026-10-25):** **360C / 0W / 9E / 5EE / 0T over 374** (release, 8 jobs, 900 s; wall ~930 s, the critical path is F0067 at 672 s CPU).
     - **Open actionable ERROR tail (1 row):** **F0064** (2026-10-25: P0045 CONVERTED, the torus band seam bridge crossed a window loop, N5 amendment; F0067 / P0056 / P0061 CONVERTED, the §4.5.2 loop no longer stops on a repeated failure set and its ceiling is density 16, N67 amendment).
     - **Loud-by-design / adjudicated ERROR rows (8 rows):** six C-series walls (`C0046`, `C0107`, `C0108`, `C0109`, `C0111`, `C0113`), `P0026` (adjudicated minimizer-minted exact tangency), and `R0023` (refined cap segment face / M8 follow-up).
     - **Key permanent lessons from recent conversions:**
       - *Never enumerate surface types or curve maps inside a general geometric rule:* Stage-4 triple-corner gate admits by incidence count (three distinct surfaces, Yang §4.5.1), and fan-rebuild winding uses `surface_normal_at` for all surface types.
       - *Loud ceiling (N66):* `boolean::refine_loop` adopts only an EMPTY failure set; surviving failures STOP loudly as `YangError::RefinementExhausted`.
       - *Slow and correct first (N67 amendment, user-signed 2026-10-25):* the loop never stops on a repeated failure set, because a failure set is a STEP function of density (P0056 reads 9/9/9 through ×8 and clears at ×16). Speedups come later as recorded deviations measured against the correct loop, never as early-stop guesses.
       - *Crease arc extent (N96):* Anchor a T2 fire's position against its crease's ARC (`YANG_S45_BOUNDARY_DOMAIN=census`) before reading it as a mechanism.
       - *A relocation is §4.3.2's closest point, never an azimuth slide (N97):* the cone ∩ plane generator slide slipped vertices ALONG their section (R0049 / P0015 / P0029). A step that leaves its face is a recorded FAILURE, repaired after optimization from the midpoint of its converged bounds (`s451_plan_repairs`, N90) — never continued from the failing point itself: P0005's "Fig. 12" fires were overshoots past a triple point its own neighbour already sat on. Continuation on S1 + q1/q2 are unwired (no customer measured). The cylinder twin `project_onto_ellipse_via_cylinder` still slides, behind a band ladder.
       - *Timing slow cases:* Time with `YANG_STAGE_TIME_PROBE=1` / `CHERCHI_TIME_PROBE=1` (`ASSAY_JOBS=1 … single_case`) before designing for cost.
   - **Working the tail & assay prospector (subordinate to the roadmap, §4 there):**
     - Anchor each open tail item against its un-minimized lineage first (`target/prospect/seed-{4,5}/candidates/` and `findings/`, or `WAFFLE_PATH=… user_case_probe replay_waffle_env` — minimizer rounding can mint contacts or shift sites).
     - Attach it to the roadmap milestone whose mechanism answers it; build no case-specific fix that no milestone names.
     - Prospector seeds are the regression net, run at milestone checkpoints: `prospect_run` / `prospect_minimize` for the next seed (`docs/TESTING.md` §"Running the assay prospector").
     - The boundary-point certificate's inventory (`YANG_S45_BOUNDARY_DOMAIN_LOG`) names the out-of-domain relocations the gear cases (`R0003` ×42, `R0004`, `R0032`, `R0049`, `R0070`) survive today — the §4.5.1 corridor family's open customers.
   - **Running the categorized assay:**
     - Correctness oracle is **reference parity against the Cherchi C++ sidecar** (roadmap §6) plus the categorized kernel-v2 assay in **`--release`** (per-case timeouts are CPU-time-budgeted and load-insensitive):
       `ASSAY_JOBS=8 ASSAY_CASE_TIMEOUT_SECS=900 cargo test -p test-harness --test assay_kv2 --release full_corpus_categorized -- --ignored --nocapture`
     - Re-run any lone `TIMEOUT` as `single_case` before recording it. See `docs/TESTING.md` §"Running the categorized assay".

   **The paper IS the spec.** Read `refs/yang2025_hybrid_boolean.pdf` before
   each session. Implement what the paper describes.

   **Reading the papers efficiently.** Run `./scripts/extract-papers.sh` once
   per session (idempotent; ~2s when up-to-date) to produce text views of
   `refs/*.pdf` at `refs/text/*.txt`. Both `refs/` and `refs/text/` are
   gitignored (papers are license-restricted), so this is a local-only
   workflow. Cite line numbers from the `.txt` views when discussing
   specific paper sections (e.g. `refs/text/yang2025_hybrid_boolean.txt:574-605`
   for §4.4.2). When spawning sub-agents that need to read papers, point
   them at the extracted `.txt` paths.

   **Current architecture and next steps live in
   `docs/yang_functional_roadmap.md`** — the single source of truth for the
   Yang effort. Read it before working on the pipeline. The native
   `cherchi-rs` arrangement + boolean is COMPLETE (M6/M7: pure Rust,
   clean-room predicates, WASM-clean) and is yang-rs's production backend;
   the C++ sidecars remain as dev-only parity oracles
   (`scripts/build_sidecars.sh`).

   **Key references:**
   - Cherchi 2020 C++ reference (arrangement): `github.com/gcherchi/FastAndRobustMeshArrangements`
   - Cherchi 2022 C++ reference (full Boolean pipeline + ray-cast in/out):
     `github.com/gcherchi/InteractiveAndRobustMeshBooleans`
   - Livesu et al. 2021 (simplified earcut CDT): "Deterministic Linear Time Constrained Triangulation Using Simplified Earcut"
   - **Compliance matrix:** `docs/audits/yang_2025_audit.md` (rebuilt
     2026-10-16) — the paper walked in order, one row per procedural
     statement of §3–§4 (194 rows: FAITHFUL / PARTIAL / DEVIATES / MISSING /
     ORDER / SILENT / N/A, each with its code site, ledger id and pin), the
     paper's pipeline order against `boolean.rs`, and ranked cross-cutting
     findings. It is the paper-first complement of the cleanup spec's
     code-first §2 table. Read the rows for your section before changing a
     stage, and update them in the same commit as the ledger.

   **Reference parity is not optional.** When the algorithm we're porting has a
   public reference implementation (Cherchi 2020/2022 C++), build differential
   testing against that reference *as part of the initial port*, not as a future
   audit. Treat the reference as a black-box oracle: feed it the same inputs,
   compare canonicalized outputs. We do NOT copy its source — we build it as a
   sidecar (vendored or external clone), invoke its public API, diff the output.
   If the reference is unavailable or unbuildable, document why explicitly.
   Reference parity is how we know the port is correct; internal oracles
   (`pipeline_oracles.rs`) measure local stage contracts but cannot detect a
   port that diverges from the reference upstream of the oracle's check.
2. **SSI solvers** — Complete the A15.4 matrix. Solvers feed stage 4 (geometry
   refinement) of the hybrid pipeline. Only work on SSI if Yang pipeline work is
   blocked. Priority: pairs #5, #6, #10 (partial status).
3. **GUI test coverage** — Expand Playwright tests in `app/tests/gui/`. Cover all drawing modes, feature dialogs, and viewport interactions with both click-click and click-drag.
4. **Cross-crate integration tests** — Expand `crates/test-harness/` with multi-operation scenarios: sketch → extrude → boolean → tessellation → verify.

## Session Start Checklist

1. Run `git status` — understand what branch you're on and what's changed.
2. Read `ARCHITECTURE.md` — understand the system.
3. Read `INTERFACES.md` — understand the type contracts.
4. Read `/governance/ENGINEERING_CONSTITUTION.md` — understand the engineering law.
5. Read `/agents/ORCHESTRATION.md` — understand the agent workflow.
6. **Identify your sub-project and route accordingly:**
   - **Kernel-stack work** (`cad-primitives`, `nurbs-rs`, `waffle-types`, `cherchi-rs`,
     `ssi-rs`, `yang-rs`, `kernel-v2`, and the two sidecar crates) is routed by
     **`docs/yang_functional_roadmap.md`** (the plan of record) plus the crate
     routing table under "Kernel: kernel-v2" above — **not** by any
     `projects/NN-*/` dossier. Those kernel dossiers (`projects/01-kernel-fork`)
     describe the retired truck/`crates/kernel` code and are ARCHIVED; do not
     follow them. Pick the next uncompleted milestone (M0–M8) in the roadmap.
   - **Non-kernel sub-projects** (sketch-solver, wasm-bridge, 3d-viewport,
     sketch-ui, feature-engine, modeling-ops, ui-chrome, file-format,
     test-harness, dev-infrastructure) still use their `projects/NN-*/`
     dossier: read that sub-project's `CLAUDE.md`, then its `PLAN.md` and pick
     the highest-priority uncompleted task. Sanity-check any dossier against
     the live tree first — some list truck-era history in completed-milestone
     notes.

## While Coding

- **Stay within your sub-project directory.** Do not modify files in other sub-projects.
- **Import types from interfaces, not other crates' internals.** The shared types in INTERFACES.md are the contracts. Never reach into another crate's `src/` for types.
- **Run tests frequently.** `cargo test -p <your-crate>` after every meaningful change.
- **Keep commits atomic.** One logical change per commit. Commit messages explain why, not what.

## Before Committing

1. Run `cargo test -p <your-crate>` — all tests pass.
2. Run `cargo clippy --all-targets -p <your-crate> -- -D warnings` — no
   warnings. (`--all-targets` is REQUIRED: the CI lint job lints test code
   too; a plain `cargo clippy` misses test-only lints and goes red in CI —
   measured 2026-08-15.)
3. Run `cargo fmt --check -p <your-crate>` — properly formatted.
4. Update PLAN.md — mark completed tasks, add discovered tasks. (For the new
   kernel crates, update milestones/notes in `docs/yang_functional_roadmap.md`
   instead — those crates have no per-crate PLAN.md.)

**Pushing to main is gated** (`.githooks/pre-push`, decision D-1 of the
remediation plan). The push is refused when the latest completed `Rust tests`
run on main is not green (a `cancelled` run there means the 120-min limit was
hit), and when the pushed commits change a kernel-stack crate with no passing
`./scripts/test.sh rewrite` (or `fast`/`full`) recorded against exactly that
kernel content; a passing run records itself. Overrides take a reason and
are logged: `CI_GATE_OVERRIDE="…"` (the push is the fix for red CI),
`REWRITE_GATE_OVERRIDE="…"` (mechanism-free change). Never merge or push on
top of a red or timed-out main.

## Test Tiers

Run the appropriate test tier for your workflow:

- `./scripts/test.sh rewrite` — Kernel-stack inner loop: the seven kernel
  crates + the FFI-feature cherchi-rs suite (~490s).
- `./scripts/test.sh fast` — Rewrite tier + consumer crates (waffle-types with
  `mock-kernel`, feature-engine, modeling-ops, file-format, wasm-bridge) +
  test-harness fast binaries (~500s).
- `./scripts/test.sh full` — Everything: fast + the complete test-harness
  suite (~27min). **All tiers are green** as of 2026-07-28; a red test is a
  regression, not legacy noise.

  These timings were re-measured on 2026-07-28 and are much larger than the
  ~70s/~80s/~2min previously documented. The old figures were not wrong so much
  as they timed TRUNCATED runs: without `--no-fail-fast`, cargo aborted a crate
  at its first failing test binary, so the tier "finished" early while silently
  skipping the rest. It is what hid `assay_kv2` for eleven days. The tiers now
  run everything, and cost what they actually cost.
- `./scripts/test.sh gui-fast` — Quick GUI smoke tests (~2min)
- `./scripts/test.sh gui-full` — All ~55 GUI spec files (~5min)
- `./scripts/test.sh all-fast` — Rust fast + GUI fast combined
- `./scripts/test.sh all` — Full Rust + full GUI (pre-merge)

Capability-pending tests are `#[ignore]`-tagged (Rust) or `test.skip`
quarantined (GUI) with milestone reasons (M8; M5 K11 inc-2) — un-quarantine
them when their milestone lands. Anything else red is a real regression.

See `docs/TESTING.md` for tier definitions and how to add tests.

## Structural Fixes First (default posture)

**Prefer real, robust, structural fixes over tolerance/relocation-band tuning —
basically always.** The kernel's remaining failure tail is dominated by missing
*capability* (Yang §4.4.1 mesh-updating, §4.5.2 local refinement, corner-junction
assembly, conformal reassembly, CDT re-triangulation), not by tolerances. Adding
another Stage-4 acceptance band moves ~0 cases now. Reach for a band ONLY to
convert a NEW silent-wrong into a loud STOP (a P10 safety net) — never to squeak a
case through, and never in place of the structural fix the STOP is naming. See
`memory` `feedback_stop_band_tuning_build_mesh_updating` and
`docs/yang_functional_roadmap.md`.

A structural fix legitimately spans a whole session (or several). That is
expected, not a scoping failure — land it as deliberate, atomic checkpoints
(design spec → de-risked gated-off primitive with tests → one wired increment
proven byte-identical), the way #137 (`specs/yang_137_torus_plane_grazing_corner.md`)
and #168 (`specs/yang_n2_stage4_cdt_mesh_updating.md`) are structured.

## If Stuck

- **Don't loop.** If a *specific approach* isn't working after a few attempts,
  stop and reassess the approach — this is about avoiding unproductive spinning,
  NOT about abandoning a large structural task because it takes more than one
  commit.
- **Document in PLAN.md** under "Blockers" — what you tried, what failed, what you think the issue is.
- **A long gap between commits is NOT itself a signal to narrow scope.** Building
  the mesh-updating machinery is inherently multi-commit and multi-session; judge
  progress by "am I still learning / advancing the structural fix?", not by a
  wall-clock commit cadence. Break work down when an approach is genuinely thrashing,
  not merely because it is large.

## Test Philosophy

- **Every public function gets a test.**
- **Mock dependencies.** Use `waffle_types::kernel::MockKernel` (feature `mock-kernel`) for unit tests; kernel-v2 for real-geometry tests.
- **Tests must be deterministic.** No random values, no system time, no filesystem side effects.
- **Tests are permanent.** Never delete a passing test. Fix it if it's wrong.
- **Property-based tests** where applicable: Euler's formula (V-E+F=2), watertightness, manifoldness.

## Architecture Boundaries

- **Rust crates produce data** (meshes, entity lists, solve results). They do NOT render.
- **Rendering happens in Svelte/three.js.** The `three.js` boundary is absolute.
- **WASM ↔ JS communication** goes through wasm-bridge only. No direct WASM imports in UI components.
- **Kernel types don't leak.** Use the Kernel/KernelIntrospect traits. Never expose kernel internals to other crates.

## Fix It Right or Don't Fix It (P9–P10)

- **If you can't explain why a test fails, don't change code to make it pass.**
  No tolerance widening, no special-case branches, no fallback paths that produce
  right answers for wrong reasons. Document in PLAN.md and move on.
- **If the plan's diagnosis is wrong, abort the fix and report what you learned.**
  Do not improvise an alternative. Plans are cheap; reverting hacks is expensive.

## Analytical Primacy & Hybrid Boolean Pipeline (Invariant A15)

- **Target architecture**: Yang et al. 2025 hybrid B-Rep/mesh boolean [#24].
  Meshes are an *exact computational tool* for deriving correct B-Rep topology,
  not a degradation. The paper's pipeline (Section 4.5.5 + Sections 4.1–4.5):

  **Stage 0: Coplanar preprocessing** (Section 4.5.5) — detect coplanar face
  pairs between the two solids BEFORE tessellation. Perform 2D Boolean on
  coplanar planes to segment into A-only, B-only, and overlap regions. Replace
  overlap with a shared trimmed surface and generate identical meshes for both
  models. Overlap boundaries become intersection curves. **This is critical** —
  without it, tessellation produces non-identical meshes on coplanar faces,
  causing conformal edge explosions and incorrect face survival.

  Stage 1: Tessellate with bijective mapping → Stage 2: exact mesh boolean
  (Cherchi 2020 §4 indirect predicates + Cherchi 2022 §5 ray-cast in/out) →
  Stage 3: extract topology → Stage 4: refine to SSI curves → Stage 5: assemble B-Rep.

  Analytical surfaces survive through the pipeline. The paper is the blueprint
  — read it (`refs/yang2025_hybrid_boolean.pdf`) before working on the pipeline.
- **SSI solvers** (A15.1): Quadric SSI solvers remain essential — they provide
  the geometry refinement in stage 4 of the hybrid pipeline. Continue implementing
  missing solvers (see A15.4 matrix).
- **The legacy S-H clipping + tolerance-escalation pipeline was DELETED with
  `crates/kernel/` at the Phase 6 migration (2026-06-11).** Its failure mode
  (masking classification errors with tolerance widening and synthetic fill
  triangles) is the cautionary tale behind P9/P10 — never reintroduce that
  pattern in the new stack.
- See governance/ARCHITECTURAL_INVARIANTS.md A15 for the full invariant.

## GUI Test Rules

- **NEVER swallow assertion errors.** No try/catch around expected-state waits.
  If drawing should produce 3 entities, `waitForEntityCount(page, 3, 5000)`
  must throw on timeout — that IS the test failure.
- **Every drawing mode needs BOTH click-click AND click-drag tests.**
  Use `drawLine()` for click-click, `dragLine()` for click-drag.
- **Verify tool state, not just outputs.** Check `getToolState()` and
  `getDrawingState()` at each step, not just final entity counts.
- **Never use `__waffle.addSketchEntity()` to test drawing.** Drawing tests
  must use real pointer events. API entity creation is only for test SETUP
  (e.g., creating fixtures for constraint tests).
- **Run `sketch-drawing-regression.spec.js` before every commit that touches
  sketch code.** It's the canary — if it fails, drawing is broken.
- **WASM crash detection**: Use `collectCrashErrors(page)` + `expectNoAnyCrash()` from helpers/state.js.
  NEVER use `getState().engineReady` as a crash oracle — it is not reliably reset on crash.
  With panic=unwind enabled (nightly + -Zbuild-std), catch_unwind catches kernel panics gracefully.
  Use `expectNoAnyCrash` (strict, zero crashes) for new tests.
- **Chrome must scroll or collapse, never overflow.** The page cannot scroll
  (`html, body { overflow: hidden }`), so anything that outgrows its box is
  clipped and unreachable. Every region either scrolls (`overflow-y: auto`
  panels, dialogs with `max-height`), collapses (the toolbar measures itself
  and walks a collapse ladder — see the comment above `collapseLevel` in
  `Toolbar.svelte`), or is sized relative to the window (`min(300px, 40vw)`).
  `tests/gui/layout-overflow.spec.js` (fast tier) is the oracle: it sweeps
  desktop widths in the widest UI states and fails on any interactive element
  outside the window. **Never click a toolbar button by raw testid in a spec**
  — use `clickTool()` / `clickToolbarAction()` from helpers/toolbar.js, which
  find the button whether it is inline, in "More ▾", or in the ⋮ menu at the
  current width.

## WASM Rebuild Workflow

Standard stable-Rust `wasm-pack` since the Phase 6 migration (the nightly +
`-Zbuild-std` two-step died with the legacy kernel's panic=unwind machinery).

After any Rust crate changes that affect the WASM bridge:

1. Build:
   ```
   wasm-pack build crates/wasm-bridge --release --target web --no-default-features
   ```
2. Copy to app: `cp crates/wasm-bridge/pkg/wasm_bridge{_bg.wasm,.js} app/static/pkg/`
3. Verify dev server still works: `npm run dev`

Notes:
- The sketch solver is pure Rust (Levenberg-Marquardt + nalgebra) and compiles
  to wasm32-unknown-unknown natively — no Emscripten, no separate WASM module.
- wasm-bindgen CLI version must match the crate version in Cargo.lock
  (wasm-pack downloads the right one automatically).
- The wasm32 rustflags in `.cargo/config.toml` keep an enlarged 4MB stack for
  the exact-arithmetic recursion depth. Do not reintroduce `panic=unwind`.

Include the updated WASM bundle in the same commit as the Rust changes so the app stays in sync.

## Sub-Project Directory Layout

Each sub-project under `projects/` contains:
- `ARCHITECTURE.md` — Technical design for this sub-project
- `PLAN.md` — Task list with milestones, status, blockers, and interface change requests
- `INTERFACES.md` — Types this sub-project implements and consumes
- `CLAUDE.md` — Agent-specific instructions for this sub-project

## Dependency Graph

```
Phase 1 (parallel):  01-kernel + 02-sketch-solver
Phase 2 (parallel):  03-wasm-bridge + 04-3d-viewport
Phase 3:             05-sketch-ui
Phase 4 (parallel):  06-feature-engine + 07-modeling-ops
Phase 5:             08-ui-chrome
Phase 6:             09-file-format
Phase 7:             10-assemblies (deferred)
```
