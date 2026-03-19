# Research Specs for Sketch Solver Rewrite

Research briefs to be executed by deep research agents (web + paper access)
before implementation begins. Each brief targets a specific knowledge gap
in our execution plan.

**Format**: Each brief specifies:
- What we already know (from the spec)
- What we need to learn (the gap)
- Specific questions (answerable, not open-ended)
- Desired output format

**Execution**: These are meant to be run outside the swarm — proxy to deep
research agents with web/paper access. Results feed back into the wave plans
as implementation notes.

## Brief Index

| # | File | Feeds into | Priority |
|---|------|-----------|----------|
| R1 | `r1_jacobian_cookbook.md` | Fork A (constraints) | **critical** |
| R2 | `r2_newton_raphson_practice.md` | Fork B (numerics) | **critical** |
| R3 | `r3_rank_and_conflict.md` | Fork B (numerics), Wave 3 | **critical** |
| R4 | `r4_underconstrained_strategies.md` | Fork B (numerics) | high |
| R5 | `r5_proptest_geometry.md` | Fork D (proptest) | high |
| R6 | `r6_svg_constraint_rendering.md` | Fork C (render) | medium |
| R7 | `r7_nalgebra_wasm_cookbook.md` | Wave 5 (WASM) | medium |

R1–R3 are critical path — we can't write correct code without them.
R4–R5 make the testing much better. R6–R7 are nice-to-have.
