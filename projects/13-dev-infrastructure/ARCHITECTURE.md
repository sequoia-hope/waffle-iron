# Sub-project 13: Architecture

## Purpose

Provide programmatic, unambiguous tools for agents to script complex multi-step CAD workflows, verify correctness at every step, and get clear diagnostic output when anything fails.

## Crate: `crates/test-harness/`

### Module Map

```
src/
  lib.rs              # Re-exports: ModelBuilder, HarnessError, OracleVerdict, ModelReport
  workflow.rs          # ModelBuilder fluent API (wraps dispatch())
  oracle.rs            # Verification oracles (topology, mesh, provenance)
  report.rs            # Text-based model reports for agent consumption
  stl.rs               # STL export from RenderMesh (binary + ASCII)
  assertions.rs        # Rich assertion helpers with diagnostics
  helpers.rs           # HarnessError, GeomRef constructors, profile builders, mesh math
tests/
  workflow_tests.rs    # ModelBuilder API tests (10)
  oracle_tests.rs      # Oracle function tests (15)
  stl_tests.rs         # STL export tests (6)
  report_tests.rs      # Report generation tests (8)
  scenarios_mock.rs    # Complex workflows against MockKernel (15)
  scenarios_truck.rs   # Complex workflows against TruckKernel (7, 3 ignored)
```

### Key Design Decisions

1. **ModelBuilder wraps dispatch()** — calls the same function the WASM worker uses, testing the real dispatch path.

2. **Named features** — all methods accept string names (`builder.extrude("box", "sketch1", 10.0)`). Internal map of names → UUIDs.

3. **Auto-managed sketch lifecycle** — `rect_sketch()` / `circle_sketch()` handle BeginSketch → AddEntity×N → FinishSketch in one call.

4. **Oracles return verdicts, not panics** — `OracleVerdict { name, passed, detail, value }` lets agents collect all failures in one pass.

5. **Reports are structured text** — agents read natural language with fixed-width columns better than raw JSON.

6. **Position-based watertight check** — MockKernel generates per-face vertices (non-shared), so edge matching uses quantized positions, not index pairs.

7. **STL lives in test-harness** — operates on already-tessellated `RenderMesh`, kernel-independent.

### Dependency Graph

```
test-harness
├── kernel-fork (Kernel, KernelIntrospect, MockKernel, TruckKernel, types)
├── feature-engine (Feature, Operation, *Params, Engine)
├── modeling-ops (KernelBundle, OpResult, Role, TopoSnapshot)
├── wasm-bridge (dispatch, EngineState, UiToEngine, EngineToUi)
├── waffle-types (GeomRef, SketchEntity, ClosedProfile, TopoKind, etc.)
└── file-format (save/load project)
```
