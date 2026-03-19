# Sketch Constraint Solver Rewrite

Specification for replacing the libslvs-based sketch solver with a clean-sheet
pure-Rust implementation.

**Status**: Design spec
**References**: Zou et al. (2022) arXiv:2202.13795, Bettig & Hoffmann (2011),
Hoffmann-Lomonosov-Sitharam (2001), Owen (1991), SolveSpace system.cpp,
FreeCAD PlaneGCS/GCS.cpp
**Governance**: P2, P8, A1, A3, A14

---

## Motivation

The current sketch solver is a dual-implementation liability:

1. **Two separate codepaths.** Rust `sketch-solver` crate (native builds via
   `slvs` FFI to libslvs C library) and JavaScript `slvs-solver.js` (Emscripten
   WASM binary with hand-marshaled C structs). Both implement the same logic
   independently with divergence risk.

2. **C FFI fragility.** The JS solver manually packs `Slvs_Param` (16 bytes),
   `Slvs_Entity` (56 bytes), `Slvs_Constraint` (56 bytes), and `Slvs_System`
   (60 bytes) into Emscripten heap memory at exact byte offsets. One offset
   wrong → silent corruption.

3. **WASM feature gate.** libslvs cannot compile to wasm32-unknown-unknown via
   the standard Rust toolchain, hence the `native-solver` feature gate. The
   WASM build uses a separately compiled Emscripten binary, bypassing the Rust
   crate entirely.

4. **Design constraints from libslvs.** SolveSpace's API has quirks that leak
   into our abstractions:
   - `SymmetricHoriz`/`SymmetricVert` naming is inverted from intuition
   - `WhereDragged` removes 2 DOF (no 1D sliding)
   - Circle-line tangent panics in the Rust mapping
   - Radius stored as diameter internally
   - HDistance/VDistance require virtual axis entity creation
   - No constraint-level diagnostic feedback (just "failed" list)
   - `EqPtLnDistances` workaround for point-line distance

5. **No graph decomposition.** SolveSpace solves monolithically. This works
   for small sketches but provides no structural DOF analysis — only numerical
   rank computation post-solve.

A pure-Rust solver eliminates the dual codepath, compiles natively to WASM,
and lets us design the constraint vocabulary and solver behavior from scratch.

---

## Goal

Replace both solver implementations with a single Rust crate (`sketch-solver`)
that:

1. Compiles to native and wasm32-unknown-unknown without feature gates
2. Solves all 24 currently supported constraint types
3. Reports DOF, under/over-constrained status, and conflicting constraint IDs
4. Supports interactive dragging with minimal-motion behavior
5. Extracts closed profiles from solved geometry (existing algorithm preserved)
6. Matches or exceeds libslvs solve quality on the existing test suite

---

## Architecture

### Three-Layer Design

```
┌──────────────────────────────────────────────────┐
│           Layer 3: Interactive                   │
│  Dragging, incremental re-solve, solution cache  │
├──────────────────────────────────────────────────┤
│           Layer 2: Numerical Core                │
│  Newton-Raphson, LM fallback, QR rank analysis   │
├──────────────────────────────────────────────────┤
│           Layer 1: Constraint Graph              │
│  DOF analysis, decomposition, status reporting   │
└──────────────────────────────────────────────────┘
```

### Layer 1 — Constraint Graph

Structural analysis of the constraint system before numerical solving.

**Inputs:** Entity list, constraint list.

**Responsibilities:**
- Build bipartite graph: entities ↔ constraints
- Count structural DOF: `2 * num_points + num_radii - num_constraint_equations`
- Detect structurally over-constrained subsystems (more equations than unknowns
  in a subgraph)
- Decompose into independent subsystems (biconnected components) for parallel
  solving
- Report which constraints are redundant vs. conflicting

**Algorithm:** Pebble game (Jacobs & Hendrickson 1997) for Laman-style rigidity
analysis in 2D. Falls back to Jacobian rank analysis for geometric degeneracies
that structural analysis cannot detect.

**Deferral note:** Graph decomposition is a performance optimization. The solver
MUST work without it (monolithic solve). Decomposition can be added later without
changing the API.

### Layer 2 — Numerical Core

Solves the nonlinear system `F(x) = 0` where `F` is the vector of constraint
residuals and `x` is the vector of free parameters.

**Primary algorithm:** Newton-Raphson with analytic Jacobian.
- Each constraint type provides `residual(params) → f64` and
  `gradient(params) → Vec<(usize, f64)>` (sparse row of Jacobian)
- Linear system solved via QR decomposition (nalgebra)
- Convergence tolerance: `TAU_MODEL` (1e-7) from kernel units policy (A14)
- Maximum iterations: 50 (matches SolveSpace)

**Fallback:** Levenberg-Marquardt for systems where Newton-Raphson fails to
converge (near-singular Jacobian, difficult initial configuration).
- Use `argmin` crate or hand-roll damped Newton: `(J^T J + λI) δ = -J^T f`
- Adaptive λ: increase on divergence, decrease on progress

**Under-constrained handling:** When DOF > 0, solve in least-squares sense
with penalty toward current configuration:
- Minimize `‖F(x)‖² + μ ‖x - x₀‖²` where `x₀` is current positions
- Small `μ` (1e-6) acts as weak spring, keeping unconstrained geometry near
  its current location
- During dragging, `μ` can be tuned for responsiveness

**Rank analysis:** QR decomposition of the Jacobian provides:
- Rank = number of independent constraints
- DOF = num_parameters - rank
- Null space columns identify which parameters are free
- Pivots identify which constraints are dependent (redundant/conflicting)

### Layer 3 — Interactive

Real-time solving for UI interaction.

**Dragging:**
- Dragged point adds 2 temporary equations: `x = target_x`, `y = target_y`
- These are strong constraints (not penalty terms) — point follows cursor exactly
- Rest of sketch adjusts via least-squares minimal-motion
- On drag end, temporary constraints are removed and system re-solves

**Incremental re-solve:**
- Cache previous solution as initial guess for next solve
- When a single constraint is added/removed, only parameters in the affected
  subsystem need re-solving (requires Layer 1 decomposition)
- Without decomposition: full re-solve with warm start (still fast for <200 params)

**Solution selection:**
- Newton-Raphson naturally converges to the nearest solution basin
- Initial configuration = current sketch positions → solver returns the
  "closest" valid solution → no unexpected flips

---

## Constraint Types

Every constraint is an equation `f(params) = 0` with an analytic gradient.

### Geometric Constraints (remove DOF without dimensional value)

| Constraint | Equation | DOF removed |
|-----------|----------|-------------|
| Coincident(P₁, P₂) | `x₁ - x₂ = 0`, `y₁ - y₂ = 0` | 2 |
| Horizontal(L) | `y_start - y_end = 0` | 1 |
| Vertical(L) | `x_start - x_end = 0` | 1 |
| Parallel(L₁, L₂) | `dx₁·dy₂ - dy₁·dx₂ = 0` | 1 |
| Perpendicular(L₁, L₂) | `dx₁·dx₂ + dy₁·dy₂ = 0` | 1 |
| Tangent(Line, Arc) | distance(line, center) - radius = 0 | 1 |
| Tangent(Circle, Line) | distance(line, center) - radius = 0 | 1 |
| Tangent(Arc, Arc) | distance(c₁, c₂) - (r₁ ± r₂) = 0 | 1 |
| Midpoint(P, L) | `P - (L.start + L.end)/2 = 0` | 2 |
| PointOnLine(P, L) | signed distance P to L = 0 | 1 |
| PointOnCircle(P, C) | `‖P - center‖ - radius = 0` | 1 |
| Symmetric(P₁, P₂, L) | P₁ reflected across L = P₂ | 2 |
| SymmetricH(P₁, P₂) | `x₁ + x₂ = 0`, `y₁ - y₂ = 0` | 2 |
| SymmetricV(P₁, P₂) | `x₁ - x₂ = 0`, `y₁ + y₂ = 0` | 2 |
| EqualLength(L₁, L₂) | `‖L₁‖ - ‖L₂‖ = 0` | 1 |
| EqualRadius(C₁, C₂) | `r₁ - r₂ = 0` | 1 |

### Dimensional Constraints (constrain to a specific value)

| Constraint | Equation | DOF removed |
|-----------|----------|-------------|
| Distance(P₁, P₂, d) | `‖P₁ - P₂‖ - d = 0` | 1 |
| PointLineDistance(P, L, d) | `signed_dist(P, L) - d = 0` | 1 |
| HDistance(P₁, P₂, d) | `x₂ - x₁ - d = 0` | 1 |
| VDistance(P₁, P₂, d) | `y₂ - y₁ - d = 0` | 1 |
| Radius(C, r) | `r_c - r = 0` | 1 |
| Diameter(C, d) | `r_c - d/2 = 0` | 1 |
| Angle(L₁, L₂, θ) | `atan2(cross, dot) - θ = 0` | 1 |
| LengthRatio(L₁, L₂, k) | `‖L₁‖ - k·‖L₂‖ = 0` | 1 |

### Interaction Constraints (temporary)

| Constraint | Equation | DOF removed |
|-----------|----------|-------------|
| Dragged(P, x, y) | `x_p - x = 0`, `y_p - y = 0` | 2 |

### Reference Constraints

Reference constraints (`reference: true`) are NOT sent to the solver. Their
values are recomputed from solved positions post-solve. Same as current behavior.

### New constraints (future)

The architecture supports adding new constraint types by implementing:
```rust
trait Constraint {
    fn residuals(&self, params: &[f64]) -> Vec<f64>;
    fn jacobian_entries(&self, params: &[f64]) -> Vec<(usize, usize, f64)>;
    fn num_equations(&self) -> usize;
}
```

Candidates for future addition: Fix (pin single axis), Colinear, Concentric,
Block (rigid group), Pattern (linear/circular array), Offset curves.

---

## Parameters

The solver's parameter vector `x` is built from entities:

| Entity | Parameters | Count |
|--------|-----------|-------|
| Point | x, y | 2 |
| Line | (uses endpoint points) | 0 (indirect) |
| Circle | center.x, center.y, radius | 3 |
| Arc | center.x, center.y, start.x, start.y, end.x, end.y | 6 |
| Spline | control_point.x, control_point.y per CP | 2n |

Lines and arcs reference point entities, so their parameters come from the
referenced points.

---

## Outputs

```rust
pub struct SolveResult {
    /// Updated positions for all point entities
    pub positions: HashMap<Uuid, (f64, f64)>,

    /// Updated radii for circle/arc entities
    pub radii: HashMap<Uuid, f64>,

    /// Solve status
    pub status: SolveStatus,

    /// Closed profiles extracted from solved geometry
    pub profiles: Vec<Profile>,

    /// Recomputed values for reference constraints
    pub reference_values: HashMap<Uuid, f64>,
}

pub enum SolveStatus {
    /// All DOF consumed, unique solution found
    FullyConstrained,

    /// Solution found but geometry can still move
    UnderConstrained {
        dof: u32,
        /// Which parameters are free (for UI highlighting)
        free_params: Vec<FreeParam>,
    },

    /// Conflicting constraints — no solution exists
    OverConstrained {
        /// Indices of conflicting constraints (for UI highlighting)
        conflicting: Vec<Uuid>,
    },

    /// Solver failed to converge
    Failed {
        reason: String,
    },
}

pub struct FreeParam {
    pub entity_id: Uuid,
    pub axis: FreeAxis,  // X, Y, Both, Radial
}
```

---

## Integration Plan

### Phase 1: Core Solver (no UI changes)

Replace the internals of `crates/sketch-solver/` with the new solver. The
public API (`solve_sketch(&Sketch) → SolvedSketch`) stays the same. Existing
tests must pass.

- Remove `slvs` crate dependency
- Implement Newton-Raphson + analytic Jacobian for all 24 constraint types
- QR-based rank/DOF analysis
- Profile extraction unchanged (already in `waffle-types`)

### Phase 2: Eliminate JS Solver

Remove `app/src/lib/engine/slvs-solver.js` and the Emscripten `slvs.wasm`
binary. Route `SolveSketchLocal` through the WASM bridge instead:

- Remove `native-solver` feature gate — solver always compiles
- `SolveSketch` handler in `dispatch.rs` becomes unconditional
- Remove `SolveSketchLocal` intercept in `worker.js`
- Remove `slvs-solver.js`, `app/static/pkg/slvs/slvs.wasm`, `slvs.js`
- Update `worker.js` to send `SolveSketch` to WASM for all builds

### Phase 3: Enhanced Diagnostics

Improve status reporting beyond what libslvs provides:

- Per-constraint conflict identification (not just "failed" list)
- Free parameter axis identification for UI DOF visualization
- Structural vs. geometric over-constraint distinction

### Phase 4: Interactive Improvements (optional)

- Levenberg-Marquardt fallback for difficult configurations
- Graph decomposition for large sketches
- Incremental re-solve caching

---

## Dependencies

| Crate | Purpose | WASM compatible |
|-------|---------|-----------------|
| nalgebra | Dense linear algebra, QR/SVD | Yes |
| petgraph | Constraint graph analysis | Yes |

Both are pure Rust, no C FFI, wasm32-unknown-unknown compatible.

Remove: `slvs` crate (C FFI to libslvs).

---

## Removed Files (Phase 2)

```
app/src/lib/engine/slvs-solver.js      — JS solver wrapper (delete)
app/static/pkg/slvs/slvs.wasm          — Emscripten binary (delete)
app/static/pkg/slvs/slvs.js            — Emscripten glue (delete)
```

---

## Quirks Resolved by Rewrite

| Current quirk | Resolution |
|--------------|------------|
| SymmetricHoriz/Vert naming confusion | Name constraints by what they do, not libslvs convention |
| Circle-line tangent panics | Implement directly: `dist(line, center) - r = 0` |
| WhereDragged removes 2 DOF always | Support 1D dragging (slide along line) via single-axis constraint |
| Virtual axis entities for HDistance/VDistance | Direct equations: `x₂ - x₁ - d = 0` |
| Radius stored as diameter internally | Store as radius, the natural unit |
| EqPtLnDistances workaround | Direct point-line distance equation |
| Manual struct packing in JS | Eliminated — pure Rust, no FFI |
| Two divergent codepaths | Single implementation compiles everywhere |

---

## Testing Strategy

### Existing Tests (must pass)

All 59 tests in `crates/sketch-solver/tests/solve_tests.rs` must pass with
the new solver. These are the oracle — if behavior changes, investigate
before adapting.

### New Tests

Per FIP, each constraint type needs:
- Canonical case (simple geometry, known analytical result)
- Edge case (near-degenerate, zero-length, coincident entities)
- Over-constrained case (adding one too many constraints → correct status)
- Under-constrained case (removing one constraint → correct DOF count)

### Numerical Stability Tests

- Near-parallel lines with angle constraint → Jacobian near-singular
- Very small/large dimensions (1e-6 to 1e6 meters per A14)
- Coincident points with distance constraint → degenerate
- 100+ entity sketches → performance regression test

---

## Research Basis

| Topic | Reference |
|-------|-----------|
| Quadric constraint solving survey | Zou et al. (2022) arXiv:2202.13795 |
| GCS in parametric CAD | Bettig & Hoffmann (2011) ASME JCISE |
| Graph decomposition | Hoffmann, Lomonosov & Sitharam (2001) |
| Constructive solving | Owen (1991), Joan-Arinyo et al. (2003) |
| 2D rigidity / pebble game | Jacobs & Hendrickson (1997) |
| SolveSpace internals | solvespace/solvespace system.cpp |
| FreeCAD PlaneGCS | FreeCAD/FreeCAD App/planegcs/GCS.cpp |
| Adaptive precision | Shewchuk (1997) — already in REFERENCES.md as [#4] |

---

## Success Criteria

- [ ] All 59 existing sketch-solver tests pass
- [ ] All GUI sketch tests pass (Playwright)
- [ ] No feature gate — single build for native and WASM
- [ ] slvs-solver.js and slvs.wasm deleted
- [ ] DOF reporting matches or exceeds libslvs accuracy
- [ ] Solve time ≤ 2x libslvs for sketches under 100 entities
- [ ] Conflicting constraint identification at constraint granularity
- [ ] No C/C++ dependencies in sketch-solver crate
- [ ] Tolerances from units.rs (A14 compliance)

---

## Non-Goals

- 3D constraint solving (sketch solver is 2D only)
- Assembly constraints (separate system, different scope)
- Spline constraints (deferred — splines are data-only geometry currently)
- Real-time collaborative solving (single-user for now)
