# Open-Source Parametric CAD System — Agent Team Guide

## Setup

After cloning, run this once to enable the pre-push hook that keeps README renders current:

```bash
git config core.hooksPath .githooks
```

## Project Vision

Build a production-grade open-source parametric CAD system competitive with SolidWorks and Onshape. The core bottleneck is the CAD kernel — no adequate open-source B-Rep kernel exists today. OpenCASCADE is unmaintainable, FreeCAD has never been good enough, and every other option is incomplete. Coding agents make a clean-sheet kernel feasible for the first time.

**The strategic goal is not to replicate OpenCASCADE.** It is to build a clean, well-tested Rust kernel that handles the 90% case reliably, with architecture that allows incremental hardening over time. Start with analytic surfaces (planes, cylinders, cones, spheres, tori) — these cover an enormous range of machined parts. General NURBS comes later.

This is production software. Every architectural decision should consider long-term maintainability, not just getting the next test to pass. Shortcuts taken now become months of rework later. Think like you're building infrastructure that will be maintained for decades.

## Current State of the Codebase

> **⚠️ IMPORTANT: This project was started in Claude Code Web, which crashed repeatedly during early development. The existing code may be incomplete, inconsistent, or partially implemented. Files may have been written out of order or left in intermediate states. Do not assume anything works — verify everything.**

Because the initial work was done without Agent Teams (single-session, crash-prone), the code was not organized for parallel development. Some reorganization is expected and encouraged:

- Trait interfaces between modules may be missing or inconsistent
- Tests may be incomplete or absent for code that appears to be "done"
- Module boundaries may not align with the architecture described below
- Some code may duplicate functionality or have conflicting approaches

**Before starting new work, each agent should audit its area of responsibility, assess what exists, what compiles, what passes tests, and report back to the team lead.** Treat the existing code as a rough draft, not a foundation.

## Development Methodology: Incremental Commitment Spiral

This project follows an **Incremental Commitment Spiral** development model. This means:

1. **Each increment is a complete vertical slice** — not a horizontal layer. Don't build "all geometry primitives" then "all topology" then "all Booleans." Instead, build enough geometry to support a basic Boolean, enough topology to represent the result, and enough testing to verify it. Then spiral outward.

2. **Commit incrementally to complexity.** Start each capability at the simplest tier that demonstrates the architecture works. Harden and expand only after the simple case is solid. For Booleans, this means: axis-aligned box-box first → box-cylinder → arbitrary convex → curved faces → concave → degenerate cases.

3. **Every spiral includes a risk-reduction checkpoint.** Before expanding scope, verify that the current increment is well-tested, that cross-module interfaces are stable, and that the architecture can support the next level of complexity without rework.

4. **Long-term planning, incremental execution.** Agents should understand the full architecture and where their work fits in the long game, but focus execution on the current spiral increment. Don't gold-plate Phase 1 code to handle Phase 4 requirements, but do ensure Phase 1 interfaces won't need to be torn out when Phase 4 arrives.

5. **Working software over comprehensive documentation.** But for a CAD kernel, "working" means "passes all topology audits, property tests, and oracle comparisons" — not just "compiles and runs one test."

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    Web GUI (React)                    │
│         Three.js viewport + constraint sketcher       │
├─────────────────────────────────────────────────────┤
│                 Command / History Layer               │
│          Parametric feature tree + undo/redo          │
├─────────────────────────────────────────────────────┤
│               Constraint Solver (2D + 3D)            │
│          Geometric constraints + dimensions           │
├─────────────────────────────────────────────────────┤
│              Modeling Operations Layer                │
│     Extrude, Revolve, Sweep, Loft, Fillet, etc.      │
├─────────────────────────────────────────────────────┤
│                 Boolean Engine                        │
│        Union, Intersection, Difference, Split         │
├─────────────────────────────────────────────────────┤
│                B-Rep Kernel Core                      │
│    Topology (HalfEdge) + Geometry (NURBS/Analytic)   │
├─────────────────────────────────────────────────────┤
│             Tessellation / Visualization              │
│          NURBS → triangle mesh for display            │
├─────────────────────────────────────────────────────┤
│              STEP / IGES / Native I/O                │
│           Import/export interoperability              │
└─────────────────────────────────────────────────────┘
```

**Stack:** Rust kernel → WebAssembly → React/Three.js GUI

Rust chosen for: memory safety without GC, excellent testing infrastructure (proptest, cargo fuzz), algebraic types for topology, WASM compilation, and compiler-driven feedback that helps agents catch bugs at compile time.

## Agent Team Structure

### Team Lead

The team lead coordinates all agents, maintains the shared task list, resolves cross-team interface questions, and synthesizes status reports. The team lead should:

- Maintain a `STATUS.md` tracking what each agent is working on and current blockers
- Ensure agents don't work on overlapping files without coordination
- Prioritize work according to the spiral model — don't let any agent run ahead of the current increment
- Call for code review across agents when cross-module interfaces change

### Kernel Agent

**Scope:** Geometry primitives, NURBS, surface/curve intersections, B-Rep topology (half-edge data structure), Euler operators, primitive solid construction.

This is the most critical and hardest part of the system. The kernel agent should be deeply methodical and never rush. Every function must include topology audits and structured tracing.

Key principles for kernel work:
- Every comparison uses the explicit `Tolerance` struct — no magic numbers
- Every curve/surface type implements validation traits (`CurveValidation`, `SurfaceValidation`)
- Every topology mutation calls `validate_brep()` in debug/test mode
- Use robust predicates (Shewchuk-style adaptive precision) for geometric tests
- Implement analytic surface types first. NURBS later.

Current spiral priority: Point/Vector/Transform → Analytic curves → Analytic surfaces → Half-edge topology → Euler operators → Primitive solids (box, cylinder, sphere)

### Boolean Agent

**Scope:** Surface-surface intersection, face splitting, face classification, solid reconstruction — the core Boolean engine.

Booleans are the crux of the entire project. A rough but working Boolean engine is more valuable than a perfect anything else. Follow the tiered hardening strategy strictly:

```
Tier 1: Box-Box (axis-aligned)
Tier 2: Box-Cylinder, Box-Sphere
Tier 3: Arbitrary convex polyhedra
Tier 4: Convex-Convex with curved faces
Tier 5: Concave shapes, shapes with holes
Tier 6: Tangent/coincident faces
Tier 7: Degenerate cases (zero-thickness results, etc.)
```

Do not advance to the next tier until the current tier passes all topology audits, Monte Carlo volume verification, proptest fuzzing (10,000+ configurations), and OCC oracle comparison. Tiers 1–4 cover ~80% of real-world CAD operations.

Structured failure reporting is critical — when a Boolean op fails, return a `BooleanFailure` enum with full diagnostic info (face IDs, surface types, bounding boxes, classification results), not just an error string.

### Modeling Agent

**Scope:** Extrude, revolve, fillet, chamfer, sweep, loft — the operations that build on the kernel and Boolean engine.

Depends on stable kernel and Boolean APIs. Write integration tests against both. Start with extrude (the simplest and most common operation), verify it end-to-end including topology audit, then expand to revolve, then fillet.

Use the trait interfaces defined in `ARCHITECTURE.md` — don't reach into kernel internals. If the trait interface is insufficient, coordinate with the kernel agent to extend it rather than bypassing it.

### Constraint Solver Agent

**Scope:** 2D sketch constraint solver, 3D assembly constraints.

This is a largely independent mathematical subsystem. The solver interfaces with the kernel only through profiles and transforms. Every constraint is a residual function `f(params) → f64` where 0 means satisfied; the solver minimizes `Σ f_i²`. This is clean, testable, and verifiable by checking residuals.

Start with the 2D sketch solver (the more immediately useful component). Support: coincident, horizontal, vertical, parallel, perpendicular, tangent, equal, distance, angle, radius constraints. Track degrees of freedom — a fully constrained sketch has DOF = 0.

Assembly constraints (3D) come later and use a similar solver architecture with rigid body transforms.

### Test Agent

**Scope:** Verification oracle, property-based testing, fuzzing campaigns, coverage monitoring, regression test curation.

**This agent runs continuously and is arguably the most important agent on the team.** The test agent is responsible for the multi-layered verification system:

```
L0: Topological invariants (Euler-Poincaré, manifold checks)
L1: Geometric consistency (edges match face boundaries, normals consistent)
L2: Volumetric verification (Monte Carlo volume estimation)
L3: Cross-reference with OpenCASCADE via python-occ as oracle
L4: Visual regression (render + perceptual diff)
L5: Round-trip verification (export STEP → import → compare)
```

Key responsibilities:
- Maintain and expand proptest generators for random geometric configurations
- Run fuzzing campaigns against all kernel and Boolean operations
- Curate regression tests — when a bug is found, its test case becomes permanent
- Monitor code coverage per module (kernel target: 90%, GUI: 70%)
- Maintain the OCC oracle comparison infrastructure
- Run CI benchmarks and flag performance regressions > 10%

The test agent should be proactive: when new code lands from any agent, the test agent should immediately write additional test cases, run proptest campaigns, and report any failures back to the responsible agent.

### Research Agent (on-demand)

**Scope:** Literature review, algorithm selection, feasibility analysis for hard problems.

Not a permanent team member — spawned when a specific technical challenge requires deeper investigation before committing to an implementation approach. Examples:

- Best algorithm for NURBS-NURBS surface intersection (marching vs. subdivision vs. Bézier clipping)
- Persistent naming strategies for parametric rebuild (what do commercial kernels actually do?)
- Optimal half-edge data structure for cache-friendly traversal in Rust
- Survey of open-source STEP parser crates and their maturity

The research agent should produce a concise recommendation document with: problem statement, options considered, tradeoffs, recommended approach, and references. The team lead decides whether to accept the recommendation.

### GUI Agent (later phase)

**Scope:** React/Three.js frontend, WASM bridge, sketch mode UI, feature tree panel, E2E tests.

Not needed in the first spiral increment. The kernel must be functional before the GUI has anything to display. When activated, this agent should:

- Build the WASM bridge first (Rust → wasm-bindgen → JS)
- Run the kernel in a Web Worker so Boolean ops don't freeze the UI
- Use Playwright for E2E testing with programmatic test scenarios
- Implement screenshot comparison testing for visual regression

## Cross-Team Coordination

All cross-team interfaces are defined as Rust traits in `ARCHITECTURE.md`. Agents work against trait definitions. Mock implementations allow parallel development. CI runs integration tests when real implementations replace mocks.

```rust
// Kernel exposes, Boolean and Modeling consume:
pub trait BooleanEngine {
    fn union(&self, a: &Solid, b: &Solid) -> Result<Solid, BooleanFailure>;
    fn subtract(&self, a: &Solid, b: &Solid) -> Result<Solid, BooleanFailure>;
    fn intersect(&self, a: &Solid, b: &Solid) -> Result<Solid, BooleanFailure>;
}

// Constraint Solver exposes, GUI consumes:
pub trait SketchSolver {
    fn solve(&self, sketch: &Sketch) -> Result<SolvedSketch, SolverFailure>;
    fn dof(&self, sketch: &Sketch) -> usize;
}
```

**Rule: No agent modifies another agent's files without coordination through the team lead.**

## Instrumentation for Autonomous Debugging

This project must be debuggable by agents, not humans staring at a debugger. Every module must include:

1. **Structured tracing** via the `tracing` crate with JSON output. Every geometric decision gets logged with numeric details. When a test fails, the agent gets a full trace of what happened.

2. **Topology audits** after every mutation — `validate_brep()` in debug mode catches invariant violations immediately.

3. **Monte Carlo volume oracle** — for any closed solid, estimate volume via random ray shooting. Compare before/after for Boolean ops using `vol(A ∪ B) = vol(A) + vol(B) - vol(A ∩ B)`.

4. **Structured failure types** — never return bare error strings. Return enums with full diagnostic context (face IDs, surface types, intermediate state).

5. **Debug visualization server** — a WebSocket server that the kernel can push geometry to for rendering in a Three.js viewer. Useful for both agent debugging and human oversight.

## Key Reminders

- **This is a marathon, not a sprint.** A CAD kernel that handles the common cases reliably is worth more than one that attempts everything and fails on edge cases.
- **Test coverage is not optional.** Every line of kernel code must be exercised by tests. Untested code is broken code you haven't found yet.
- **The compiler is your first debugger.** Rust's type system prevents entire categories of bugs. Use `Result<>` everywhere, make illegal states unrepresentable, prefer strong types over primitives.
- **Don't try to be clever.** Use well-known algorithms from the literature. The goal is robust engineering, not novel research. Let the research agent investigate when there's a genuine open question.
- **Communicate through code.** Traits, type signatures, and doc comments are the primary communication medium between agents. If another agent can't understand your API from its types and docs, it needs to be clearer.
