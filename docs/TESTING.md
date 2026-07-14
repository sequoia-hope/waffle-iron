# Waffle Iron — Testing Guide

*(Tier list updated 2026-07-12 to match `scripts/test.sh` — the previous
version predated the Phase 6 migration and described the deleted legacy
`kernel` crate; design review G10.)*

## Test Tiers

### Kernel Rewrite (`rewrite`, ~70s)

The kernel-stack inner loop — run after every meaningful kernel change:

- **cad-primitives, cherchi-rs, ssi-rs, yang-rs, kernel-v2** — the live
  kernel stack
- **cherchi-sidecar-rs, indirect-predicates-sidecar-rs** — dev-only parity
  oracle shims
- **predicate-gen** — guards that `generated.rs` (the clean-room exact
  predicate core) is byte-identical to generator output and that filter
  constants match the published FPG/Cherchi values

Note: the cherchi-rs suite includes the flagship reference-parity test
(`parity_native_vs_sidecar`), which **panics loudly if the C++ sidecar
binary is missing** — build it once with `scripts/build_sidecars.sh`.

### Parity (`parity`, ~20s)

The `#[ignore]`d "binding reference" sidecar oracles
(`r0046_patch_label_parity`, `stage0_operand_inputcheck`), run with
`--ignored`. Included in `full`; run standalone when touching cherchi-rs
coplanar/inputcheck paths.

### Rust Fast (`fast`, ~80s)

Rewrite tier + consumer crates:

- **waffle-types** (with `mock-kernel` feature) — kernel contract + MockKernel
- **sketch-solver** — Constraint solver logic
- **feature-engine** — Feature tree and rebuild pipeline
- **modeling-ops** — Modeling operation dispatching
- **wasm-bridge** (`--no-default-features`) — WASM API surface
- **file-format** — Serialization/deserialization
- **test-harness** — Fast binaries: `scenarios_mock`, `workflow_tests`, `oracle_tests`, `report_tests`, `scenarios_advanced`, `stl_tests`

### Rust Full (`full`, ~2min)

Everything: rewrite + parity + all consumer crates + the complete
test-harness suite.

### GUI Fast (~36 spec files, <2min)

Quick smoke tests for sketch drawing, UI chrome, and basic interactions:

- Sketch drawing (lines, rectangles, circles, arcs, polygons)
- Feature tree interactions
- Keyboard shortcuts and tool switching
- Snapping and grid behavior
- Dimension input and constraints
- Basic viewport interactions

### GUI Full (~55 spec files, <5min)

Everything in GUI Fast, plus heavy workflow and infrastructure specs:

- WASM engine integration (extrude, revolve, pipeline)
- Multi-feature workflow tests
- Infrastructure and dev tooling specs
- Advanced scenario tests

## Running Tests

```bash
./scripts/test.sh rewrite    # Kernel-stack inner loop (~70s)
./scripts/test.sh parity     # Ignored sidecar reference oracles (~20s)
./scripts/test.sh fast       # Rewrite + consumer crates (~80s)
./scripts/test.sh full       # All Rust tests incl. parity (~2min)
./scripts/test.sh gui-fast   # Quick GUI smoke tests
./scripts/test.sh gui-full   # All GUI tests
./scripts/test.sh all-fast   # Rust fast + GUI fast
./scripts/test.sh all        # Everything
./scripts/test.sh assay      # Corpus replay + proptest assays
./scripts/test.sh profile    # Run timing profiler
```

## Tier Assignment Rules

### Rust Tests

| Condition | Tier |
|-----------|------|
| Kernel-stack crate (`cad-primitives`…`kernel-v2`, sidecars, `predicate-gen`) | Rewrite (and fast/full) |
| Uses `ModelBuilder::mock()` or `MockKernel` | Fast |
| Uses the real kernel (`KernelV2Adapter`) via test-harness heavy binaries | Full (slow) |
| `#[ignore]`d sidecar reference oracle | Parity (and full) |
| Pure type/logic tests (no kernel) | Fast |

### GUI Tests

| Condition | Tier |
|-----------|------|
| Tests pure UI (sketch drawing, feature tree, keyboard) | Fast |
| Tests involving WASM engine (extrude, revolve, pipeline, workflow) | Full |
| Infrastructure/dev tooling specs | Full |

## Adding New Tests

### New Rust Crate

Add the crate name to the appropriate array in `scripts/test.sh`:

- `FAST_CRATES` — for crates with only fast tests
- The full test run includes all crates automatically

### New test-harness Binary

Add the binary name to either:

- `FAST_HARNESS_BINS` — if it only uses MockKernel
- The full list runs all binaries automatically

### New GUI Spec File

- Add to the `GUI_FAST_SPECS` array in `scripts/test.sh` if it tests pure UI
- Otherwise it will be included automatically in `gui-full`

## Profiling

Run profiling scripts to identify slow tests:

```bash
./scripts/profile-rust.sh    # Per-crate Rust timing
./scripts/profile-gui.sh     # Per-spec GUI timing
```

Results are saved to:

- `test-timings-rust.log` — Rust crate-by-crate timing data
- `test-timings-gui.json` — GUI spec-by-spec timing data

These files are gitignored and not committed.

## WASM Crash Detection in GUI Tests

### The WASM build is panic=abort — `catch_unwind` does NOT catch kernel panics

**(Updated 2026-07-12, design review G1. The section this replaces described
the pre-Phase-6 nightly + `-Zbuild-std` + `panic=unwind` build, which was
retired with the legacy kernel on 2026-06-11. Do not write tests assuming
`catch_unwind` works in WASM — it does not.)**

Since the Phase 6 migration the WASM bundle is built with standard **stable**
`wasm-pack` (see `rust-toolchain.toml` and the note in `.cargo/config.toml`).
On `wasm32-unknown-unknown` this forces `panic="abort"`: a panic emits a WASM
`unreachable` trap that kills the module, and `std::panic::catch_unwind` is a
no-op. (`Cargo.toml`'s `panic = "unwind"` profile setting applies to NATIVE
test targets only, where `catch_unwind` still works.)

The kernel-v2 stack is designed for this: it is **Result-based end to end** —
errors surface as typed `KernelError`/`YangError` values, not panics, so a
WASM trap indicates a genuine kernel bug, not a routine failure path. The
defenses, in order:

1. **Primary: typed errors.** Operations that fail return errors that cross
   the bridge as structured diagnostics (error toasts in the app).
2. **Safety net: worker auto-restart.** A trap raises
   `WebAssembly.RuntimeError` in the worker's `processMessage()`; the worker
   re-fetches the module and re-inits (see flow below).
3. **Test oracle: `collectCrashErrors` + `expectNoAnyCrash`** — any trap,
   recovered or not, fails the test.

### DO NOT use `engineReady` as a crash oracle

`getState().engineReady` is set `true` once at engine init and is only reset when the
bridge receives a `needsRestart: true` flag from the worker **and** the restart fails.
After a successful auto-restart, it stays `true` even though all features were lost.
This makes it unreliable for crash detection.

### Use `collectCrashErrors` + `expectNoAnyCrash`

Import from `helpers/state.js`:

```js
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

test('my test', async ({ waffle }) => {
    const page = waffle.page;
    const crashTracker = collectCrashErrors(page);  // Set up BEFORE operations

    // ... do operations ...

    expectNoAnyCrash(crashTracker);  // Fails on ANY crash (strict)
});
```

`collectCrashErrors` returns a tracker with `all` (every crash) and `unrecovered`
(crashes where restart failed) arrays. Use `expectNoAnyCrash` (strict, zero crashes)
for new tests. `expectNoCrash` (tolerant of recovered crashes) exists for legacy tests.

### Worker crash recovery flow (safety net)

When the module traps (any kernel panic, or stack overflow beyond the 4MB
limit):

1. Boolean operation panics → WASM `unreachable` trap
2. Worker catches `WebAssembly.RuntimeError` in `processMessage()`
3. Worker fetches fresh JS module via blob URL (bypasses `import()` cache)
4. Worker calls `default(wasmBinaryUrl)` to instantiate new WASM module
5. Worker calls `init()` to create fresh engine state
6. Response includes `needsRestart: false` (recovery succeeded) or `true` (failed)

## Measured Timings (2026-02-21) — HISTORICAL

**This table predates the Phase 6 migration; the `kernel` rows refer to the
deleted legacy crate.** Re-run `./scripts/test.sh profile` for current
numbers; current tier ballparks are in the tier list above.

Baseline timing data from `profile-rust.sh` run:

| Target | Time | Notes |
|--------|------|-------|
| waffle-types | <1s | Pure types |
| sketch-solver | <1s | Constraint math |
| feature-engine | 1s | MockKernel-based |
| file-format | 6s | Serialization |
| modeling-ops | 22s | MockKernel ops |
| wasm-bridge | <1s | Needs `--no-default-features` |
| kernel (full) | 407s | Dominated by boolean tests |
| test-harness (all) | 1817s | See binary breakdown below |

### test-harness binary breakdown

| Binary | Time | Tier |
|--------|------|------|
| scenarios_mock | <1s | Fast |
| workflow_tests | <1s | Fast |
| oracle_tests | <1s | Fast |
| report_tests | <1s | Fast |
| scenarios_advanced | <1s | Fast |
| stl_tests | <1s | Fast |
| extrude_on_extrude | 1s | Full |
| geomref_kernel | 1s | Full |
| auto_union_detection | 2s | Full |
| boolean_workflows | 26s | Full |
| scenarios_kernel | 29s | Full |
| size_probe | 33s | Full |
| boolean_failures | 158s | Full |
| **extrude_chains** | **1659s** | Full (96% of harness time) |

## Running the categorized assay (the kernel-v2 corpus score)

**You CAN run the full assay reliably, including in a sandbox / under other
compute load. Run it in `--release`.** The recurring "the sandbox can't run the
assay cleanly" belief is a **debug-mode artifact**, not a real limitation — see
the "why it's reliable" note below.

The corpus is 295 cases. The runner is the `#[ignore]`d
`full_corpus_categorized` test in `crates/test-harness/tests/assay_kv2.rs`; it
prints the category table (CORRECT / WRONG / ERROR / UNSUPPORTED / …) and writes
`target/assay_kv2_report.json` + the committed `app/tests/cases/assay/results.json`.

```
# Compile once (~9s incremental), then run the full corpus:
cargo test -p test-harness --test assay_kv2 --release --no-run
ASSAY_JOBS=8 ASSAY_CASE_TIMEOUT_SECS=120 \
  cargo test -p test-harness --test assay_kv2 --release full_corpus_categorized \
  -- --ignored --nocapture
```

**Why it's reliable (even under load):** with `ASSAY_JOBS > 1` each case runs as
a killable subprocess whose per-case timeout is budgeted on **CPU time**, not
wall time (`assay_kv2.rs`, `replay_case_subprocess`). A case starved by siblings
or other machine load accrues wall time but **not** CPU time, so its verdict is
**load-insensitive** — judged the same alone or under contention. Release mode
slashes per-case CPU time, so heavy cases finish well under budget. The
debug-mode "~20 false TIMEOUTs" episodes were unoptimized cases exceeding the
CPU budget, *not* contention. (Debug + `ASSAY_JOBS=1` budgets on WALL and DOES
give false timeouts under load — avoid that combination.)

**Env knobs:**
- `ASSAY_JOBS` — parallel cases (default 4; 8 is fine on a ≥12-core box). CPU
  budgeting keeps verdicts stable; higher just risks a few borderline-slow cases
  needing a bigger budget.
- `ASSAY_CASE_TIMEOUT_SECS` — per-case **CPU**-time budget (default 30).
  **Use ≥120** for a clean full run — a too-tight budget flips genuinely-heavy
  cases (e.g. `extrude_chains`-scale) to a spurious `TIMEOUT`.
- `ASSAY_FAST=1` — skip only the un-judgeable (previously timed-out) slow-list
  cases for a quick partial baseline.

**Handling a budget `TIMEOUT`:** it means "exceeded the CPU budget," not a real
hang. Re-run that single case serially with a large budget to get its true
verdict:

```
ASSAY_CASE=<ID> ASSAY_CASE_TIMEOUT_SECS=280 \
  cargo test -p test-harness --test assay_kv2 --release single_case -- --ignored --nocapture
# or, avoiding a rebuild, invoke the built binary directly:
ASSAY_CASE=<ID> ASSAY_CASE_TIMEOUT_SECS=280 \
  ./target/release/deps/assay_kv2-<hash> single_case --ignored --nocapture
```

`ASSAY_CASE=<ID> … single_case` is also the go-to for debugging or
byte-stability spot-checks of one case (fast, deterministic, generous budget).

**Zero-regression gate for a kernel change:** run the full `--release` corpus
before and after (or lean on the byte-stability argument for the unchanged
paths, then confirm on the full run). Compare the category table; investigate
any case that moved. A budget `TIMEOUT` is not a regression — resolve it to its
true verdict with `single_case` before comparing.

> ⚠️ The committed `app/tests/cases/assay/results.json` is overwritten by every
> full run and **auto-staged by the pre-commit hook** (below). If a run left
> budget-artifact `TIMEOUT`s in it, either re-run with a larger
> `ASSAY_CASE_TIMEOUT_SECS` to regenerate it clean, or
> `git checkout app/tests/cases/assay/results.json` before committing — don't
> commit artifact timeouts as if they were true verdicts.

## Assay UI snapshot (`results.json`) — a committed file served on GitHub Pages

The in-app **AssayBrowser** shows per-case pass/fail/error status by fetching
`/assay/results.json`. That file is a **committed snapshot**, not computed at
deploy time:

- Source of truth: `app/tests/cases/assay/results.json` (committed).
- The deploy's `prebuild` step (`app/scripts/sync-assay.mjs`) copies it into
  `app/static/assay/` → served at `/assay/results.json`. **The GitHub Pages
  deploy does NOT re-run the assay** — it just builds the committed files. So
  if the snapshot is stale, the published AssayBrowser disagrees with the
  shipped WASM.
- Regenerate after any kernel/engine change that moves the corpus (a FULL
  `--release` run writes the committed snapshot; a FAST/partial run does not):

  ```
  cargo test -p test-harness --test assay_kv2 --release -- --ignored --nocapture
  git add app/tests/cases/assay/results.json
  ```

A **pre-commit hook** (`.githooks/pre-commit`) auto-stages this file whenever it
has unstaged changes, so a fresh assay run's `results.json` is never left out of
a commit (we re-run the assay often; this guarantees the update rides along).
Enable it once per clone:

  ```
  ./scripts/setup-hooks.sh     # sets core.hooksPath -> .githooks
  ```
