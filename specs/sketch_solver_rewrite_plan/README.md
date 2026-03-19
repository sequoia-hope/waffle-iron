# Sketch Solver Rewrite — Execution Plan

Implementation plan for `specs/sketch_solver_rewrite.md`. Organized as waves
of parallel work, each decomposed into forks, each fork decomposed into worker
tasks.

**Execution model**: Opus orchestrates. Claude forks handle architectural
boundaries. Gemini workers handle implementation within each fork.

## Wave Dependency Graph

```
Wave 1: Scaffold (sequential, Opus)
   │
   ├─── creates: core/ module structure, ParamLayout, Constraint trait
   │    these are the types everything else compiles against
   │
   ▼
Wave 2: Parallel Implementation (3 forks)
   │
   │  Fork A: All 21 constraint residual/jacobian implementations
   │  Fork B: LM solver + SVD rank analysis + status classification
   │  Fork C: SVG render pipeline (feature-gated, independent)
   │
   │  A and B must merge before Wave 3. C is independent.
   │  Fork B uses Levenberg-Marquardt as PRIMARY solver (not NR + fallback).
   │  See research/r2_results.md for rationale.
   │
   ▼
Wave 3: Integration (sequential, Opus)
   │
   │  Wire solver.rs, pass all 59 oracle tests
   │  Delete slvs-specific modules
   │  This is the "it works" gate
   │
   ▼
Wave 4: Parallel Hardening (2 forks)
   │
   │  Fork D: Proptest suite + new per-constraint tests
   │  Fork E: COLLAPSED into Fork B (LM is primary, not fallback)
   │  Fork F: Eliminate JS solver + feature gate (spec Phase 2)
   │
   │  D and F are independent.
   │
   ▼
Wave 5: WASM Verification (sequential, Opus)
   │
   │  WASM build, GUI tests, final cleanup
   │
   ▼
Done. Success criteria from spec satisfied.
```

## Type Contract Note

The spec proposes expanded output types (`Uuid` keys, `radii` map, `FreeParam`,
`reference_values`). The existing `SolvedSketch` uses `u32` keys and simpler
status variants. Strategy:

- **Waves 1–3**: Keep existing `SolvedSketch` contract. The solver's internal
  types can be richer, but the public API stays compatible.
- **Wave 4+**: Optionally expand `SolvedSketch` per spec (Phase 3: Enhanced
  Diagnostics). This requires coordinated changes to waffle-types + wasm-bridge
  + UI, so it's a separate PR.

## Directory Structure

```
specs/sketch_solver_rewrite_plan/
├── README.md                          ← you are here
├── wave1_scaffold/
│   └── plan.md                        ← Opus: trait design, param layout
├── wave2_parallel/
│   ├── fork_a_constraints/
│   │   └── plan.md                    ← 21 constraint implementations
│   ├── fork_b_numerics/
│   │   └── plan.md                    ← LM solver, SVD rank, status
│   └── fork_c_render/
│       └── plan.md                    ← SVG/PNG pipeline
├── wave3_integration/
│   └── plan.md                        ← Wire up, pass 59 tests
├── wave4_parallel/
│   ├── fork_d_proptest/
│   │   └── plan.md                    ← Property-based testing
│   ├── fork_e_lm_fallback/
│   │   └── plan.md                    ← COLLAPSED into Fork B
│   └── fork_f_js_elimination/
│       └── plan.md                    ← Remove JS solver + feature gate
└── wave5_wasm_verification/
    └── plan.md                        ← WASM build, GUI tests
```
