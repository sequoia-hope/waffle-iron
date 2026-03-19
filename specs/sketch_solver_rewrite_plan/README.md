# Sketch Solver Rewrite — Execution Plan

Implementation plan for `specs/sketch_solver_rewrite.md`. Organized as waves
of parallel work, each decomposed into forks, each fork decomposed into worker
tasks.

**Execution model**: Opus orchestrates. Claude forks handle architectural
boundaries. Gemini workers handle implementation within each fork.

## Key Design Decisions

- **nalgebra internally**: `Point2<f64>`, `Vector2<f64>`, `DMatrix`/`DVector`
  throughout the solver. `(f64, f64)` only at the `SolvedSketch` boundary.
- **Typed index wrappers**: `PointIdx`, `LineIdx`, `RadiusIdx` — zero-cost
  newtypes over `usize` for the flat parameter vector.
- **LM as primary solver** (not NR + fallback). See `research/r2_results.md`.
- **Weak springs + Marquardt damping + static row scaling** as the unified
  algorithm. See `research/r4_results.md`.
- **SVD for diagnostics only** — NOT in the inner LM loop. Weak springs
  guarantee full-column rank, so Cholesky on normal equations suffices.
- **Implicit arc radius**: `Radius(arc)` → `DistancePP(center, start_point)`.
- **Arc-arc tangency supported** in solver (`TangentArcArc` variant with
  `internal: bool`), even though waffle-types doesn't have the variant yet.
- **SameOrientation = no-op** in 2D (matches existing behavior + oracle test).
- **In-crate feature gate** for SVG/PNG render pipeline.

## Research Status

| Brief | Status | Key finding |
|-------|--------|-------------|
| R1: Jacobian cookbook | Filed | All 21 constraint derivatives, singularity guards |
| R2: NR in practice | Filed | LM as primary, SVD for diagnostics, dense throughout |
| R3: Rank analysis | Filed | SVD over QR, redundant/conflicting classification |
| R4: Under-constrained + scaling | Filed | Weak springs, row-only scaling, Nielsen λ update |
| R5: Proptest geometry | Pending | Not critical path (Wave 4) |
| R6: SVG rendering | Pending | Not critical path (Fork C) |
| R7: nalgebra WASM | Pending | Not critical path (Wave 5) |

## Wave Dependency Graph

```
Wave 1: Scaffold (Opus + Gemini workers)
   │
   ├─── creates: core/ types, ParamLayout, ConstraintEq trait,
   │    ConstraintImpl enum, typed index wrappers, builder
   │
   ▼
Wave 2: Parallel Implementation (3 forks)
   │
   │  Fork A: All constraint residual/jacobian implementations
   │  Fork B: LM solver + SVD rank analysis + status classification
   │  Fork C: SVG render pipeline (feature-gated, independent)
   │
   │  A and B must merge before Wave 3. C is independent.
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
   │  Fork D: Proptest suite + mathematical correctness tests
   │  Fork F: Eliminate JS solver + feature gate (spec Phase 2)
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
  types are richer (nalgebra, typed indices), but the public API stays compatible.
- **Wave 4+**: Optionally expand `SolvedSketch` per spec (Phase 3: Enhanced
  Diagnostics). This requires coordinated changes to waffle-types + wasm-bridge
  + UI, so it's a separate PR.

## Directory Structure

```
specs/sketch_solver_rewrite_plan/
├── README.md                          ← you are here
├── research/
│   ├── README.md                      ← index of research briefs
│   ├── r1_results.md                  ← Jacobian cookbook
│   ├── r2_results.md                  ← LM as primary solver
│   ├── r3_results.md                  ← SVD rank analysis
│   └── r4_results.md                  ← Weak springs + scaling
├── wave1_scaffold/
│   └── plan.md                        ← Types, traits, param layout, builder
├── wave2_parallel/
│   ├── fork_a_constraints/
│   │   └── plan.md                    ← Constraint residuals + Jacobians
│   ├── fork_b_numerics/
│   │   └── plan.md                    ← LM solver, SVD rank, status
│   └── fork_c_render/
│       └── plan.md                    ← SVG/PNG pipeline
├── wave3_integration/
│   └── plan.md                        ← Wire up, pass 59 tests
├── wave4_parallel/
│   ├── fork_d_proptest/
│   │   └── plan.md                    ← Property + mathematical correctness tests
│   ├── fork_e_lm_fallback/
│   │   └── plan.md                    ← COLLAPSED into Fork B
│   └── fork_f_js_elimination/
│       └── plan.md                    ← Remove JS solver + feature gate
└── wave5_wasm_verification/
    └── plan.md                        ← WASM build, GUI tests
```
