# 11 — Test Harness

`crates/test-harness` is the cross-crate integration and regression suite. It
drives the real stack (feature engine, bridge, kernel-v2) from Rust. How to
run it, the test tiers, and the recipes for the assay, the sketch corpus, the
prospector and the oracles are in [`docs/TESTING.md`](../../docs/TESTING.md).

## What is in the crate

- **`ModelBuilder`** (`src/workflow.rs`) — a fluent API for scripted
  workflows: sketch → extrude / revolve / pipe → boolean → undo / redo /
  suppress / reorder → tessellate. `ModelBuilder::kernel_v2()` runs on the
  real kernel, `ModelBuilder::mock()` on `MockKernel`.
- **Oracles** (`src/oracle.rs`, `src/assertions.rs`) — topology (Euler
  characteristic, manifold edges), mesh quality (degenerate triangles,
  normals, watertightness) and provenance checks, each returning an
  `OracleVerdict`.
- **`ModelReport`** (`src/report.rs`) — a structured text description of a
  model for test diagnostics.
- **Assay** (`src/assay/`) — the categorized boolean corpus runner
  (`tests/assay_kv2.rs`, cases in `app/tests/cases/assay/`), its generators,
  and its oracles: exact volume and membership, on-surface geometry,
  topology, determinism, and the case prospector (`src/assay/prospect/`).
- **Cherchi sidecar** (`src/cherchi_sidecar.rs`) — reference parity against
  the Cherchi C++ implementation.
- **Sketch oracle** (`src/sketch_rank.rs`, `tests/sketch_rank_oracle.rs`) —
  an independent computation of a sketch's `params`, `rows`, `rank`, `dof`
  and null space (constraint residuals from their published equations,
  central finite differences, SVD), compared against the solver's
  `SketchSolveReport` for every `SketchConstraint` variant.
- **Sketch corpus** (`src/sketch_corpus.rs`, `tests/sketch_corpus.rs`) —
  authored expectations for `app/tests/cases/sketch/`, replayed through the
  solver, the agent tools and the oracle.
- **STL** (`src/stl.rs`) — STL export from a `RenderMesh`.

## Rules

- Tests are deterministic: no random values without a fixed seed, no system
  time, no filesystem side effects outside a temp directory.
- A capability the kernel does not have yet is `#[ignore = "<milestone>"]`
  with the milestone tag. Remove the ignore in the commit that lands the
  capability. Anything else red is a regression.
- Run the assay in `--release`.

## Known solver defects pinned here

Each pin asserts today's measured behavior and fails with a message saying
to delete the pin and assert the fix. The defects belong to `sketch-solver`.

| Defect | Pin (`tests/sketch_rank_oracle.rs`) |
|---|---|
| `SOLVE_TOL` is absolute (1e-6). The same rectangle converges to the same relative accuracy at every scale, and flips to `SolveFailed` once authored large enough for the absolute threshold to be crossed. | `an_absolute_solve_tol_refuses_the_same_rectangle_once_it_is_authored_large` |
| `MOVED_EPS` is absolute (1e-9). At kilometer scale a pinned point that did not move is listed in `moved`. | `an_absolute_moved_eps_lists_a_pinned_point_as_moved_once_the_sketch_is_large` |

## Open tasks

None planned in this dossier. Assay and oracle work is driven by
`docs/kernel_1_0.md` and `docs/yang_faithfulness_roadmap.md`.
