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
| Yang pipeline stage | `crates/yang-rs/` |
| B-Rep / Euler ops / primitives / tessellation / trait adapter | `crates/kernel-v2/` |
| Shared primitive type (Point3 etc.) | `crates/cad-primitives/` |
| Kernel trait / shared kernel types / MockKernel | `crates/waffle-types/` (`src/kernel/`) |

### Known capability boundaries (NotSupported, loud)

kernel-v2 returns typed `KernelError::NotSupported` (or typed yang errors) for
operations it does not implement yet. These surface as error toasts in the app
and as `#[ignore]`-tagged tests / `test.skip` GUI quarantines in the suites.
They are ROADMAP ITEMS, not bugs:

- **Revolve** — KV6 milestone (38 corpus cases + 4 quarantined GUI specs)
- **Coplanar boolean inputs** (flush/stacked faces) — Yang Stage 0, roadmap M8
- **cyl×cyl lateral∩lateral and other degree-4 SSI** — roadmap M5
- **Gear / arc-segment profiles** (non-convex CDT) — Phase 2 tail
- **STEP export** — trait-default NotSupported
- **Fillet / chamfer / shell** — deferred indefinitely (see below)

When one of these milestones lands, un-quarantine its tests in the same PR
(grep for the milestone tag, e.g. `KV6` or `M8`, in `#[ignore =` and
`test.skip` annotations).

## DEFERRED INDEFINITELY: Fillet, Chamfer, Shell

**Fillet, chamfer, and shell operations are DEFERRED INDEFINITELY. Do NOT work on them.**

Experimental implementations exist (Sprint 18) and MockKernel tests pass, but these operations depend on boolean reliability which is itself under development. UI dialogs display warning banners with disabled Apply buttons.

**Never suggest fillet, chamfer, or shell as "next steps" or "what to work on."** If a session plan includes fillet/chamfer/shell work, skip it and choose a different task.

## Yang/Cherchi Deviations

Track known divergences between the implementation and Yang 2025 / Cherchi 2022 in `docs/yang_deviations.md`. Before working on a Yang stage, compare the code against the paper section it claims to implement. A deviation discovered mid-investigation halts the cycle — the deviation is the bug; do not debug downstream symptoms. (Rule: `feedback_yang_only.md`.)

## Current Priorities

When asked "what should I work on?", choose from these areas **in order**.
Do NOT skip to lower-priority items because they are easier.

1. **Hybrid boolean pipeline (Yang 2025).** This is the #1 priority. **The
   plan of record is `docs/yang_functional_roadmap.md`** — read it first; it
   defines the `LabeledArrangement` interface and milestones M0–M8 (M0, M1,
   M2, M6, M7 and the Phase-6 migration are COMPLETE; the kernel is live in
   the app). The remaining capability gaps, in priority order, are the
   NotSupported boundaries listed above: **KV6 revolve**, **M8 coplanar
   Stage 0**, **M5 degree-4 SSI (cyl×cyl)**, and the non-convex CDT profile
   tail. The correctness oracle is **reference parity against the Cherchi
   C++ sidecar** (roadmap §6) plus the categorized kernel-v2 assay
   (`cargo test -p test-harness --test assay_kv2 -- --ignored --nocapture`).

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
   - **Implementation audit:** `docs/audits/yang_2025_audit.md` — per-step assessment
     of what's CORRECT, INCOMPLETE, WRONG, or STUB vs the paper. Read this before
     working on the Yang pipeline to know what actually needs fixing.

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
   - **Kernel-stack work** (`cad-primitives`, `waffle-types`, `cherchi-rs`,
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
2. Run `cargo clippy -p <your-crate>` — no warnings.
3. Run `cargo fmt --check -p <your-crate>` — properly formatted.
4. Update PLAN.md — mark completed tasks, add discovered tasks. (For the new
   kernel crates, update milestones/notes in `docs/yang_functional_roadmap.md`
   instead — those crates have no per-crate PLAN.md.)

## Test Tiers

Run the appropriate test tier for your workflow:

- `./scripts/test.sh rewrite` — Kernel-stack inner loop: the seven kernel
  crates + the FFI-feature cherchi-rs suite (~70s).
- `./scripts/test.sh fast` — Rewrite tier + consumer crates (waffle-types with
  `mock-kernel`, feature-engine, modeling-ops, file-format, wasm-bridge) +
  test-harness fast binaries (~80s).
- `./scripts/test.sh full` — Everything: fast + the complete test-harness
  suite (~2min). **All tiers are green** since the Phase 6 migration; a red
  test is a regression, not legacy noise.
- `./scripts/test.sh gui-fast` — Quick GUI smoke tests (~2min)
- `./scripts/test.sh gui-full` — All ~55 GUI spec files (~5min)
- `./scripts/test.sh all-fast` — Rust fast + GUI fast combined
- `./scripts/test.sh all` — Full Rust + full GUI (pre-merge)

Capability-pending tests are `#[ignore]`-tagged (Rust) or `test.skip`
quarantined (GUI) with milestone reasons (KV6 / M8 / M5) — un-quarantine them
when their milestone lands. Anything else red is a real regression.

See `docs/TESTING.md` for tier definitions and how to add tests.

## If Stuck

- **Don't loop.** If something isn't working after a few attempts, stop.
- **Document in PLAN.md** under "Blockers" — what you tried, what failed, what you think the issue is.
- **Move to the next task.** Don't burn context on one problem.
- **If no commit in 15 minutes,** the task scope is too broad. Break it down into smaller tasks in PLAN.md.

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
