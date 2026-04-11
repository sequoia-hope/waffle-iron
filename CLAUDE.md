# Waffle Iron — Claude Code Session Guide

Global instructions for any Claude Code session working on Waffle Iron.

## Document Precedence

When rules conflict, the following precedence applies (highest first):

1. `/governance/*` — Engineering Constitution, FIP, DoD, Architectural Invariants
2. `/agents/*` — Roles, skills, orchestration
3. `AGENTS.md` — Repo-level team structure
4. This file — Session workflow and coding conventions
5. Sub-project `CLAUDE.md` files — Project-specific instructions

## DEFERRED INDEFINITELY: Fillet, Chamfer, Shell

**Fillet, chamfer, and shell operations are DEFERRED INDEFINITELY. Do NOT work on them.**

Experimental implementations exist (Sprint 18) and MockKernel tests pass, but these operations depend on boolean reliability which is itself under development. UI dialogs display warning banners with disabled Apply buttons.

**Never suggest fillet, chamfer, or shell as "next steps" or "what to work on."** If a session plan includes fillet/chamfer/shell work, skip it and choose a different task.

## Current Priorities

When asked "what should I work on?", choose from these areas **in order**.
Do NOT skip to lower-priority items because they are easier.

1. **Hybrid boolean pipeline (Yang 2025)** — This is the #1 priority.
   The goal is `YANG_BOOLEAN=1` passing more assay cases (currently 8/190).
   Do NOT fix the old S-H pipeline. Do NOT add fallback paths from Yang to S-H.

   **Current diagnosis**: B-Rep reassembly (Phase 5) produces catastrophically
   wrong topology — Euler characteristic of -62 (should be 2), hundreds of
   unpaired edges, self-intersections. R-series cases timeout or produce zero
   surviving faces. This is not an edge case problem — the face assembly
   algorithm is fundamentally broken.

   **What to work on**: Pick ONE specific failing case (e.g., F0003 box-box
   subtract). Trace it through the pipeline step by step: tessellation →
   exact intersection → cell labeling → face survival → boundary extraction
   → B-Rep assembly. Find exactly where correct intermediate results become
   wrong output. Fix that ONE thing. Do not try to fix the whole pipeline
   in one session.

   **What NOT to do**: Do not add retessellation workarounds, mesh passthrough
   hacks, "accept invalid" paths, or tolerance tweaks. If the face assembly
   is wrong, fix the face assembly.
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
6. Identify your sub-project. Read that sub-project's `CLAUDE.md`.
7. Read that sub-project's `PLAN.md` — pick the highest-priority uncompleted task.

## While Coding

- **Stay within your sub-project directory.** Do not modify files in other sub-projects.
- **Import types from interfaces, not other crates' internals.** The shared types in INTERFACES.md are the contracts. Never reach into another crate's `src/` for types.
- **Run tests frequently.** `cargo test -p <your-crate>` after every meaningful change.
- **Keep commits atomic.** One logical change per commit. Commit messages explain why, not what.

## Before Committing

1. Run `cargo test -p <your-crate>` — all tests pass.
2. Run `cargo clippy -p <your-crate>` — no warnings.
3. Run `cargo fmt --check -p <your-crate>` — properly formatted.
4. Update PLAN.md — mark completed tasks, add discovered tasks.

## Test Tiers

Run the appropriate test tier for your workflow:

- `./scripts/test.sh fast` — During development, runs MockKernel + pure logic tests (~30s)
- `./scripts/test.sh full` — Before committing, runs all ~910 Rust tests (~5min)
- `./scripts/test.sh gui-fast` — Quick GUI smoke tests (~2min)
- `./scripts/test.sh gui-full` — All ~55 GUI spec files (~5min)
- `./scripts/test.sh all-fast` — Rust fast + GUI fast combined
- `./scripts/test.sh all` — Full Rust + full GUI (pre-merge)

See `docs/TESTING.md` for tier definitions and how to add tests.

## If Stuck

- **Don't loop.** If something isn't working after a few attempts, stop.
- **Document in PLAN.md** under "Blockers" — what you tried, what failed, what you think the issue is.
- **Move to the next task.** Don't burn context on one problem.
- **If no commit in 15 minutes,** the task scope is too broad. Break it down into smaller tasks in PLAN.md.

## Test Philosophy

- **Every public function gets a test.**
- **Mock dependencies.** Use MockKernel, not WaffleKernel, for unit tests.
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
  (Cherchi indirect predicates) → Stage 3: extract topology → Stage 4: refine
  to SSI curves → Stage 5: assemble B-Rep.

  Analytical surfaces survive through the pipeline. The paper is the blueprint
  — read it (`refs/yang2025_hybrid_boolean.pdf`) before working on the pipeline.
- **SSI solvers** (A15.1): Quadric SSI solvers remain essential — they provide
  the geometry refinement in stage 4 of the hybrid pipeline. Continue implementing
  missing solvers (see A15.4 matrix).
- **DEPRECATED — do not improve, do not delete yet**: The S-H clipping + tolerance
  escalation pipeline (`classify_face`, `stitch.rs` progressive pairing, tessellation
  repair loops, `fill_boundary_holes`, `close_near_boundary_chains`). These mask
  classification errors with up to 5000× tolerance widening and synthetic fill
  triangles. The self-intersection oracle confirms 0/10 R-series produce correct
  meshes. **Removal requires the Yang pipeline to be operational first** — see the
  migration plan in `specs/yang_hybrid_migration.md`.
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

After any Rust crate changes that affect the WASM bridge:

1. Build with nightly + build-std (required for panic=unwind on WASM):
   ```
   cargo +nightly build -p wasm-bridge --target wasm32-unknown-unknown --release --no-default-features -Zbuild-std
   ```
2. Generate JS bindings:
   ```
   wasm-bindgen target/wasm32-unknown-unknown/release/wasm_bridge.wasm --out-dir crates/wasm-bridge/pkg --target web --no-typescript
   ```
3. Copy to app: `cp crates/wasm-bridge/pkg/wasm_bridge{_bg.wasm,.js} app/static/pkg/`
4. Verify dev server still works: `npm run dev` (port 8083)

**Note**: `wasm-pack` cannot be used because it doesn't support `-Zbuild-std`. The two-step
`cargo build` + `wasm-bindgen` process is required to enable `panic=unwind` on wasm32-unknown-unknown.
See `.cargo/config.toml` for WASM target rustflags.

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
