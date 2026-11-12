# 13 — Development Infrastructure: Architecture

The tooling that lets an agent or a developer script a CAD workflow, check
it, and know whether a change is safe to push: the `ModelBuilder` harness,
the test runner, the git hooks and CI.

Test tiers, recipes and rules for adding tests are in
[`docs/TESTING.md`](../../docs/TESTING.md). The contents of
`crates/test-harness` are listed in `projects/11-test-harness/PLAN.md`.

## ModelBuilder (`crates/test-harness/src/workflow.rs`)

`ModelBuilder` scripts a model from Rust the way the app does.

- **It wraps `wasm_bridge::dispatch`.** Every operation is a `UiToEngine`
  message through the same function the WASM worker calls, so a test
  exercises the real dispatch path. Agent tools run through `agent_tool`.
- **Features are named.** Methods take string names
  (`m.extrude("box", "sketch1", 10.0)`); the builder maps names to UUIDs.
- **Sketch shortcuts manage the sketch lifecycle.** `rect_sketch`,
  `circle_sketch` and `polygon_sketch` begin the sketch, add entities and
  finish it in one call. `begin_sketch` … `finish_sketch_manual` builds a
  custom shape.
- **Two kernels.** `ModelBuilder::kernel_v2()` runs on the real kernel.
  `ModelBuilder::mock()` runs on `MockKernel`, which is fast and
  deterministic but produces placeholder topology.
- **Oracles return verdicts.** `check_mesh` and `check_topology` return
  `Vec<OracleVerdict>` rather than panicking, so one pass collects every
  failure.
- **Reports are text.** `report()` returns a `ModelReport`; `to_text()`
  prints the feature tree with topology counts and roles, mesh summaries,
  the bounding box, oracle results and engine errors.

## Test runner (`scripts/test.sh`)

| Subcommand | Runs |
|---|---|
| `rewrite` | The kernel-stack crates and the FFI-feature cherchi-rs suite |
| `fast` | `rewrite` + consumer crates + the fast test-harness binaries |
| `full` | `fast` + the complete test-harness suite |
| `parity` | Reference-parity suites |
| `gui-fast`, `gui-full`, `gui-relay` | Playwright tiers |
| `relay` | The agent-link relay's Python suite |
| `all-fast`, `all` | Rust + GUI combined |
| `assay-quick`, `assay`, `assay-deep` | Assay corpus runs |
| `profile` | Per-crate and per-spec timings |

Tiers run with `--no-fail-fast`, so one failing binary does not hide the
rest. A passing `rewrite`, `fast` or `full` run records itself against the
kernel fingerprint (`scripts/kernel_fingerprint.sh`).

## Git hooks (`.githooks/`, enabled by `scripts/setup-hooks.sh`)

**pre-commit**

1. Stages a regenerated assay snapshot (`app/tests/cases/assay/results.json`).
2. Blocks the commit when the WASM bundle in `app/static/pkg/` does not match
   the committed sources (`scripts/wasm-fingerprint.sh`; rebuild with
   `scripts/build-wasm.sh`).
3. Blocks a kernel commit whose deviations ledger (`docs/yang_deviations.md`)
   did not move. `SKIP_LEDGER_CHECK=1` is for a mechanism-free refactor.

**pre-push** (pushes to `main`)

1. Refuses when the latest completed `Rust tests` or `Rust lint` run on main
   is not green.
2. Refuses when the pushed commits change a kernel-stack path and no passing
   `rewrite` / `fast` / `full` run is recorded against that kernel content.

Overrides take a reason and are logged: `CI_GATE_OVERRIDE="…"`,
`REWRITE_GATE_OVERRIDE="…"`.

## CI (`.github/workflows/`)

| Workflow | Trigger | What it gates |
|---|---|---|
| `rust-lint.yml` | push to main, PR | `fmt --check`, `clippy --all-targets -D warnings` over every shipping crate; a guard against clock calls in crates that ship in the WASM bundle |
| `rust-test.yml` | push to main, PR | Full suites of the shipping crates including the parity oracles (C++ sidecars built and cached), render-view parity, the JSON Schema goldens, the assay smoke gate, a `wasm32` check; a second job runs every always-on test-harness binary in release and the determinism parity script |
| `gui-tests.yml` | push to main, PR | Playwright fast tier |
| `gui-cross-browser.yml` | nightly | GUI suite on other browsers |
| `relay-tests.yml` | push to main, PR | Relay suite, lint, and the tool-manifest guard |
| `deploy.yml` | push to main | GitHub Pages deploy |

Lint debt must be measured with the same package set CI uses. Feature
unification can hide warnings from a per-crate `cargo clippy -p <crate>`: a
module behind a feature another crate's dev-dependency enables is not
compiled when the crate is linted alone.

## Scripts (`scripts/`)

| Script | Purpose |
|---|---|
| `build-wasm.sh` | Build the WASM bundle, install it into the app, record its fingerprint |
| `build_sidecars.sh` | Build the Cherchi C++ reference sidecars |
| `extract-papers.sh` | Text views of `refs/*.pdf` |
| `faithfulness_score.sh`, `kernel_1_0_score.sh` | Scoreboards from the ledger, the audit matrix and corpus reports |
| `check-icons.mjs`, `check-gui-swallowed-waits.mjs` | Static gates run in the GUI tier |
| `determinism_parallel_parity.sh` | Determinism hashes with cherchi-rs `parallel` off and on |
| `profile-rust.sh`, `profile-gui.sh` | Timing profiles |
