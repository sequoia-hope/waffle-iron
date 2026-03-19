# Wave 2 / Fork A: Constraint Implementations

**Executor**: Claude fork (worktree), with Gemini workers
**Depends on**: Wave 1 (ParamLayout, ConstraintEq trait, ConstraintImpl enum)
**Parallel with**: Fork B (numerics), Fork C (render)
**Estimated scope**: ~800 lines

## Goal

Implement `ConstraintEq` for all 21 `ConstraintImpl` variants. Each variant
needs `residuals()` and `jacobian()` — the equations from the spec's constraint
table, translated to code with pre-resolved param indices.

## Constraint Breakdown by Worker

### Worker A1: Simple geometric (6 constraints)

These are trivial — direct coordinate differences.

| Constraint | Equations | Jacobian |
|-----------|-----------|----------|
| Coincident | x₁-x₂=0, y₁-y₂=0 | ±1 entries |
| Horizontal | y_start - y_end = 0 | ±1 |
| Vertical | x_start - x_end = 0 | ±1 |
| SymmetricH | x₁+x₂=0, y₁-y₂=0 | ±1 |
| SymmetricV | x₁-x₂=0, y₁+y₂=0 | ±1 |
| Midpoint | P-(L.start+L.end)/2=0 (2 eqs) | ±1, ±0.5 |

All have constant Jacobians (linear constraints).

### Worker A2: Line-pair geometric (4 constraints)

These involve direction vectors `(dx, dy)` computed from endpoint params.

| Constraint | Equation | Notes |
|-----------|----------|-------|
| Parallel | dx₁·dy₂ - dy₁·dx₂ = 0 | Cross product |
| Perpendicular | dx₁·dx₂ + dy₁·dy₂ = 0 | Dot product |
| EqualLength | ‖L₁‖ - ‖L₂‖ = 0 | sqrt in residual; chain rule in jacobian |
| Symmetric(L) | P₁ reflected across L = P₂ | 2 equations, depends on line direction |

EqualLength jacobian singularity: when a line has zero length, the derivative
of sqrt is undefined. Guard with `max(‖L‖, epsilon)` in denominator.

### Worker A3: Distance/dimensional (6 constraints)

| Constraint | Equation | Notes |
|-----------|----------|-------|
| Distance(P,P,d) | ‖P₁-P₂‖ - d = 0 | sqrt singularity at coincident points |
| PointLineDistance(P,L,d) | signed_dist - d = 0 | Signed distance to line |
| HDistance(P₁,P₂,d) | x₂-x₁-d = 0 | Linear, trivial |
| VDistance(P₁,P₂,d) | y₂-y₁-d = 0 | Linear, trivial |
| Angle(L₁,L₂,θ) | atan2(cross,dot)-θ = 0 | atan2 gradient |
| LengthRatio(L₁,L₂,k) | ‖L₁‖-k·‖L₂‖ = 0 | Same sqrt issue as EqualLength |

### Worker A4: Circle/arc constraints (4 constraints)

| Constraint | Equation | Notes |
|-----------|----------|-------|
| Radius(C,r) | r_c - r = 0 | Trivial: 1 entry in jacobian |
| Diameter(C,d) | r_c - d/2 = 0 | Same |
| OnEntity(P,Line) | signed_dist(P,L) = 0 | Same as PointLineDistance with d=0 |
| OnEntity(P,Circle) | ‖P-center‖ - r = 0 | sqrt singularity at center |

### Worker A5: Tangent + special (4 constraints)

| Constraint | Equation | Notes |
|-----------|----------|-------|
| Tangent(Line,Arc) | dist(line,center)-r = 0 | Point-line distance to arc center |
| Tangent(Circle,Line) | dist(line,center)-r = 0 | Same formula, circle center |
| Tangent(Arc,Arc) | dist(c₁,c₂)-(r₁±r₂)=0 | Internal vs external tangent |
| EqualAngle(4 lines) | angle(L₁,L₂)-angle(L₃,L₄)=0 | Compound atan2 |
| EqualPointToLine | dist(P₁,L)-dist(P₂,L)=0 | Difference of signed distances |
| Dragged(P,x,y) | x_p-x=0, y_p-y=0 | Same as Coincident with fixed target |

Note: `SameOrientation` is a no-op (2D sketch context). Skip.

### Worker A6: Constraint builder + unit tests

Implement `build_constraints()` that maps `SketchConstraint` variants to
`ConstraintImpl` variants using `ParamLayout`. This is the equivalent of the
current `constraint_mapping.rs` but targeting our own types.

Also: unit tests for each constraint in isolation.
- For each constraint: construct a known-satisfied configuration, verify
  residuals ≈ 0. Perturb one param, verify residual changes correctly.
- Verify jacobian entries against finite-difference approximation:
  `∂f/∂x ≈ (f(x+h) - f(x-h)) / 2h` with `h = 1e-7`.

## Deliverables

- `core/constraint.rs`: `impl ConstraintEq for ConstraintImpl` with all 21 variants
- `core/builder.rs`: `build_constraints()` function
- `tests/constraint_tests.rs`: unit tests for each constraint type
- `tests/jacobian_tests.rs`: finite-difference jacobian verification

## Verification

- `cargo test -p sketch-solver -- constraint` — all constraint unit tests pass
- `cargo test -p sketch-solver -- jacobian` — all FD checks pass within 1e-5
