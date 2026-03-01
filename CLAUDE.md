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

Experimental TruckKernel implementations exist (Sprint 18) and MockKernel tests pass, but these operations depend on boolean reliability which is itself fragile. UI dialogs display warning banners with disabled Apply buttons.

**Never suggest fillet, chamfer, or shell as "next steps" or "what to work on."** If a session plan includes fillet/chamfer/shell work, skip it and choose a different task.

## Current Priorities

When asked "what should I work on?", choose from these areas:

1. **Boolean shapeops reliability** — Improve truck boolean robustness in `vendor/truck/`. Fix box-cylinder, coplanar, and chained operation failures.
2. **GUI test coverage** — Expand Playwright tests in `app/tests/gui/`. Cover all drawing modes, feature dialogs, and viewport interactions with both click-click and click-drag.
3. **Cross-crate integration tests** — Expand `crates/test-harness/` with multi-operation scenarios: sketch → extrude → boolean → tessellation → verify.
4. **Extrude/revolve pipeline polish** — Improve reliability and UX of the working feature creation operations.

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
- **Mock dependencies.** Use MockKernel, not TruckKernel, for unit tests.
- **Tests must be deterministic.** No random values, no system time, no filesystem side effects.
- **Tests are permanent.** Never delete a passing test. Fix it if it's wrong.
- **Property-based tests** where applicable: Euler's formula (V-E+F=2), watertightness, manifoldness.

## Architecture Boundaries

- **Rust crates produce data** (meshes, entity lists, solve results). They do NOT render.
- **Rendering happens in Svelte/three.js.** The `three.js` boundary is absolute.
- **WASM ↔ JS communication** goes through wasm-bridge only. No direct WASM imports in UI components.
- **Kernel types don't leak.** Use the Kernel/KernelIntrospect traits. Never expose truck types to other crates.

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
  With panic=unwind enabled (nightly + -Zbuild-std), catch_unwind catches truck panics gracefully.
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
Phase 1 (parallel):  01-kernel-fork + 02-sketch-solver
Phase 2 (parallel):  03-wasm-bridge + 04-3d-viewport
Phase 3:             05-sketch-ui
Phase 4 (parallel):  06-feature-engine + 07-modeling-ops
Phase 5:             08-ui-chrome
Phase 6:             09-file-format
Phase 7:             10-assemblies (deferred)
```
