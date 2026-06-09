# Waffle Iron — Claude Code Session Guide

Global instructions for any Claude Code session working on Waffle Iron.

## Document Precedence

When rules conflict, the following precedence applies (highest first):

1. `/governance/*` — Engineering Constitution, FIP, DoD, Architectural Invariants
2. `/agents/*` — Roles, skills, orchestration
3. `AGENTS.md` — Repo-level team structure
4. This file — Session workflow and coding conventions
5. Sub-project `CLAUDE.md` files — Project-specific instructions

## Kernel Rewrite In Progress

**The current `crates/kernel/` is being replaced.** Yang / Cherchi / boolean code in the old kernel grew tangled with legacy S-H clipping, polygon-clipping fallback, and tolerance-escalation masking. Rather than continue patching it, we are clean-sheet rewriting the kernel as a layered set of new crates.

### New crate layout

```
crates/cad-primitives/   — shared types & constants (Point3, Vector3, BoolOp, …)
crates/cherchi-rs/       — Cherchi 2020+2022 mesh boolean (pure Rust port)
crates/ssi-rs/           — analytical SSI solvers (Patrikalakis Ch.5)
crates/yang-rs/          — Yang 2025 pipeline (deps cherchi-rs + ssi-rs)
crates/kernel-v2/        — clean B-Rep + tessellation + Kernel trait (deps yang-rs)
```

Dependency layering is compiler-enforced via each crate's `Cargo.toml`. A crate higher in the stack may not be imported by one lower down.

### Agent routing rules

When asked to work on boolean / Yang / SSI / B-Rep code:

| Task | Crate to work in | DO NOT touch |
|---|---|---|
| Mesh boolean (Cherchi port) | `crates/cherchi-rs/` | `crates/kernel/src/boolean/cherchi/` |
| Analytical SSI solver | `crates/ssi-rs/` | `crates/kernel/src/ssi/` |
| Yang pipeline stage | `crates/yang-rs/` | `crates/kernel/src/boolean/yang_integration.rs` |
| B-Rep / Euler ops / primitives / tessellation | `crates/kernel-v2/` | `crates/kernel/src/` (except Kernel trait signature reference) |
| Shared primitive type (Point3 etc.) | `crates/cad-primitives/` | n/a |
| Public Kernel trait refinement | `crates/kernel-v2/` + `crates/waffle-types/` | `crates/kernel/` |

When asked to "fix a Yang bug" or "make Y62-style probe" on the existing code: **do not**. The existing Yang code is being deleted. Any new work goes into the new crates.

**Maintenance policy (decided 2026-06-09): only the Yang rewrite is maintained. Everything legacy is being actively removed, incrementally.** Concretely:

- **No legacy patches.** The former "urgent legacy patch" exception is revoked. Do not fix bugs in `crates/kernel/`, its boolean/SSI/Yang-integration code, or the legacy WASM bundle — not even small ones. A legacy bug report is a non-event; the answer is the rewrite.
- **Legacy test failures are expected and are NOT work items.** The legacy kernel's failing tests (34 lib tests, the red legacy portions of `test.sh fast`: file-format STEP export, modeling-ops `truck_*`, wasm-bridge boolean) stay red until their code is deleted. Do not fix, do not widen tolerances, do not delete individual tests to get green.
- **The app's WASM bundle is frozen** at its last build (May 2026). No rebuilds for legacy kernel changes (there should be none). The next bundle rebuild is the Phase 5 migration to `kernel-v2`.
- **Deletion is incremental, tied to rewrite milestones.** When a rewrite milestone makes a legacy area redundant, delete that legacy area in the same PR or the immediately following one (e.g. yang-rs functional boolean → delete `crates/kernel/src/boolean/yang_integration.rs` + the Yang assay plumbing; kernel-v2 trait implementation → delete `crates/kernel/` wholesale per Phase 5). Do not delete ahead of the milestone that replaces the capability the app actually uses.

### What stays unchanged

- `crates/waffle-types/` — public types crate
- `crates/sketch-solver/`, `crates/modeling-ops/`, `crates/feature-engine/`, `crates/file-format/`, `crates/test-harness/` — these are consumers / siblings of the kernel and do not need to change until the Phase 5 migration
- `crates/wasm-bridge/` — will be updated in Phase 5 migration PR to depend on `kernel-v2` instead of `kernel`
- The `Kernel` and `KernelIntrospect` trait shape lives in `waffle-types`; `kernel-v2` implements it. The trait can be refined (drop dead methods, tighten signatures), but consumers are updated in the migration PR, not piecemeal.
- All of `app/`, `governance/`, `agents/`, `refs/` — unchanged

### Phase tracker

- **Phase 0** (this PR): Boundary establishment — skeleton crates created, dependency layering locked, root CLAUDE.md updated
- **Phase 1**: `cherchi-rs` port — indirect predicates, mesh arrangement, boolean labeling. Reference parity via C++ sidecar.
- **Phase 2**: `yang-rs` pipeline — Stage 1 bijective tessellation → Stage 6 B-Rep reassembly, layered on top of cherchi-rs
- **Phase 3**: `ssi-rs` analytical solvers — 15 quadric pairs
- **Phase 4**: `kernel-v2` — clean B-Rep + Euler ops + tessellation + Kernel trait
- **Phase 5**: Migration — switch `wasm-bridge` and `feature-engine` to `kernel-v2`, delete `crates/kernel/`, remove this section from CLAUDE.md

> **These phases are crate *layers*, not a strict work order.** The actual
> work order toward a functional boolean is the milestone sequence M0–M8 in
> `docs/yang_functional_roadmap.md`, which interleaves the layers (e.g. the
> interim C++ sidecar producing real Stage-2 labels lets `yang-rs` Stage 5/6
> become real *before* the native `cherchi-rs` arrangement exists). Phase 1's
> "indirect predicates" are now built **demand-driven** by the native
> arrangement, not ported speculatively ahead of a consumer.

### Why this rewrite

- The Y62 / Y63 cycles found the Yang code was patching around legacy assumptions (face stored_normal didn't track polygon walk; legacy boolean output preserved wrong normals after subtract; `tessellate_planar_face_bounded` was force-aligning to mask upstream defects)
- yang_fast is 12/157 currently, but most of that is Yang inheriting broken inputs from legacy assembly, not Yang itself being wrong
- Reference parity against Cherchi C++ was deferred until PR-Y29 instead of being load-bearing from day one (per `feedback_external_coherence.md`)
- The "tests pass" metric (1250/34 in current kernel) measures how legacy + Yang patches handle the corpus, NOT how Yang handles it — false project status signal

The rewrite path is more honest and architecturally clean. Test counts will drop during transition (kernel-v2 has zero tests at scaffold time) and recover as each phase completes. Per-cycle work happens inside ONE crate at a time, isolated from the others by both crate boundaries and CLAUDE.md scope rules.

---

## DEFERRED INDEFINITELY: Fillet, Chamfer, Shell

**Fillet, chamfer, and shell operations are DEFERRED INDEFINITELY. Do NOT work on them.**

Experimental implementations exist (Sprint 18) and MockKernel tests pass, but these operations depend on boolean reliability which is itself under development. UI dialogs display warning banners with disabled Apply buttons.

**Never suggest fillet, chamfer, or shell as "next steps" or "what to work on."** If a session plan includes fillet/chamfer/shell work, skip it and choose a different task.

## Yang/Cherchi Deviations

Track known divergences between the implementation and Yang 2025 / Cherchi 2022 in `docs/yang_deviations.md`. Before working on a Yang stage, compare the code against the paper section it claims to implement. A deviation discovered mid-investigation halts the cycle — the deviation is the bug; do not debug downstream symptoms. (Rule: `feedback_yang_only.md`.)

## Current Priorities

When asked "what should I work on?", choose from these areas **in order**.
Do NOT skip to lower-priority items because they are easier.

1. **Hybrid boolean pipeline (Yang 2025) — in the NEW crates.** This is the #1
   priority. **The plan of record is `docs/yang_functional_roadmap.md`** — read
   it first; it defines the `LabeledArrangement` interface and milestones M0–M8.
   Do NOT fix legacy code. Build Yang as described in the paper.

   The next concrete work is M0 (operationalize the parity oracle — build the
   C++ sidecars via `scripts/build_sidecars.sh`) and M1 (make `yang-rs` Stage 1
   emit Cherchi-`inputcheck`-clean meshes — the real gate, since Cherchi hangs
   on malformed input).

   > The legacy oracle guidance that used to sit here (`spotlight_<CASE>_oracles`,
   > `default_oracle_registry` in `crates/kernel/src/boolean/pipeline_oracles.rs`,
   > the Y48–Y57 canary family, `yang_fast 12/157`) all concern the **legacy**
   > `crates/kernel/` port and its assay. They do not apply to the new crates.
   > In the new world the correctness oracle is **reference parity against the
   > Cherchi C++ sidecar** (roadmap §6: GREEN ::= matches the sidecar on a
   > corpus subset), not the legacy stage-invariant registry.

   **The paper IS the spec.** Read `refs/yang2025_hybrid_boolean.pdf` before
   each session. Implement what the paper describes — do NOT adapt it to fit
   legacy code. Legacy code (old conformal repair, tolerance escalation,
   S-H clipping) is being REPLACED, not accommodated.

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
   NEW-crate Yang effort. Read it before working on the pipeline.

   > The Stage 0–6 "working" list that used to sit here described the *legacy*
   > `crates/kernel/` port (`mesh_arrangement.rs`, `flood_fill_patches`,
   > `label_cells`), NOT the new crates. It was stale and misleading and has
   > been removed. Honest new-crate status (≈29 PRs of foundations, **zero
   > working booleans end to end**, the `LabeledArrangement` interface, and the
   > M0–M8 milestones) is in the roadmap.

   The condensed plan: the path to a *functional* Yang is decoupled from a
   complete native arrangement via a producer-agnostic `LabeledArrangement`
   interface. An interim **patched C++ sidecar** supplies real Stage-2 labels
   now (so `yang-rs` Stage 5/6 become real and produce a first mesh-approximate
   boolean); the native `cherchi-rs` arrangement is built later behind the same
   interface with the sidecar as its parity oracle. The real gate to a first
   boolean is **Stage-1 mesh validity** (Cherchi hangs forever on non-manifold /
   non-watertight input), not the labels — see roadmap M1.

   **Key references:**
   - Cherchi 2020 C++ reference (arrangement): `github.com/gcherchi/FastAndRobustMeshArrangements`
   - Cherchi 2022 C++ reference (full Boolean pipeline + ray-cast in/out):
     `github.com/gcherchi/InteractiveAndRobustMeshBooleans`
   - Livesu et al. 2021 (simplified earcut CDT): "Deterministic Linear Time Constrained Triangulation Using Simplified Earcut"
   - Yang fast test: `YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast --ignored --nocapture`
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
6. Identify your sub-project. Read that sub-project's `CLAUDE.md`.
7. Read that sub-project's `PLAN.md` — pick the highest-priority uncompleted task.
   **For the new kernel crates (`cherchi-rs`, `yang-rs`, `cherchi-sidecar-rs`,
   `indirect-predicates-sidecar-rs`, `ssi-rs`, `kernel-v2`) there is no per-crate
   `PLAN.md`** — `docs/yang_functional_roadmap.md` is the plan of record for all
   of them; pick the next uncompleted milestone (M0–M8) there.

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

- `./scripts/test.sh rewrite` — **The gating tier for maintained code.** All seven
  new kernel crates + the FFI-feature cherchi-rs suite (~70s). Must be green
  before every commit touching the rewrite.
- `./scripts/test.sh fast` — Rewrite tier + legacy MockKernel/pure-logic tests
- `./scripts/test.sh full` — Rewrite tier + all legacy Rust tests (~5min)
- `./scripts/test.sh gui-fast` — Quick GUI smoke tests (~2min)
- `./scripts/test.sh gui-full` — All ~55 GUI spec files (~5min)
- `./scripts/test.sh all-fast` — Rust fast + GUI fast combined
- `./scripts/test.sh all` — Full Rust + full GUI (pre-merge)

**Legacy portions of `fast`/`full` are red and stay red** (see Maintenance
policy above) — known failures in file-format, modeling-ops `truck_*`, kernel
tessellation, and wasm-bridge boolean are unmaintained code awaiting deletion,
not work items. Gate on `rewrite` being green.

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
  (Cherchi 2020 §4 indirect predicates + Cherchi 2022 §5 ray-cast in/out) →
  Stage 3: extract topology → Stage 4: refine to SSI curves → Stage 5: assemble B-Rep.

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

> **Frozen during the rewrite (policy 2026-06-09).** The shipped bundle in
> `app/static/pkg/` is pinned at its last legacy build; legacy kernel changes
> no longer trigger rebuilds (there should be no legacy kernel changes). This
> workflow next applies at the Phase 5 migration, when `wasm-bridge` switches
> to `kernel-v2` (which targets stable Rust — at that point standard
> `wasm-pack` replaces the nightly two-step below).

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
