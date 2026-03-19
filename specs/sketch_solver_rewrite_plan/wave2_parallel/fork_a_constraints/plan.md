# Wave 2 / Fork A: Constraint Implementations

**Executor**: Claude fork (worktree), with Gemini workers
**Depends on**: Wave 1 (ParamLayout, ConstraintEq trait, ConstraintImpl enum)
**Parallel with**: Fork B (numerics), Fork C (render)
**Estimated scope**: ~800 lines

## Goal

Implement `ConstraintEq` for all 21 `ConstraintImpl` variants. Each variant
needs `residuals()` and `jacobian()` — the equations from the spec's constraint
table, translated to code with pre-resolved param indices.

**Primary reference**: `research/r1_results.md` — full analytic Jacobian cookbook.

## Key Implementation Rules (from R1)

1. **Normalized point-line distance** — always divide cross product by ‖L‖.
   Unnormalized form destroys condition number.
2. **atan2 for all angle work** — never use acos. atan2 handles all quadrants.
3. **Symmetry(arbitrary line) via midpoint + perpendicular decomposition** —
   2 equations, division-free, avoids cubic gradient manifolds.
4. **Singularity guard**: `D = max(D, 1e-12)` for all distance denominators.
5. **Finite-difference verification**: `h = 1e-5`, tolerance `1e-7`.
   Central differences: `(f(x+h) - f(x-h)) / 2h`.
6. **Entity-type dispatch**: `Equal` dispatches to EqualLength (lines) or
   EqualRadius (circles). `Distance` dispatches to P-P or P-L. `OnEntity`
   dispatches to PointOnLine or PointOnCircle.

## Constraint Breakdown by Worker

### Worker A1: Linear constraints (Group 1) — 8 constraints

All have constant Jacobians (±1.0 or ±0.5). No singularities.

| Constraint | Equations | Params | Jacobian entries |
|-----------|-----------|--------|-----------------|
| Coincident | x₁-x₂=0, y₁-y₂=0 | 4 | ±1 |
| Horizontal | y_start - y_end = 0 | 2 | ±1 |
| Vertical | x_start - x_end = 0 | 2 | ±1 |
| SymmetricH | x₁+x₂=0, y₁-y₂=0 | 4 | ±1 |
| SymmetricV | x₁-x₂=0, y₁+y₂=0 | 4 | ±1 |
| Midpoint | P-(L.start+L.end)/2=0 (2 eqs) | 6 | 1, -0.5, -0.5 |
| Dragged | x_p-tx=0, y_p-ty=0 | 2 | 1 |
| Radius/Diameter | r_c - target = 0 | 1 | 1 |

### Worker A2: Nonlinear fundamentals (Group 2) — 5 constraints

Direction-vector and distance-based. Establish the singularity guard pattern.

| Constraint | Equation | Singularity | Guard |
|-----------|----------|-------------|-------|
| Distance(P,P,d) | √(Δx²+Δy²) - d = 0 | D=0 (coincident) | clamp D ≥ ε |
| EqualLength | D₁ - D₂ = 0 | D₁=0 or D₂=0 | clamp independently |
| Parallel | Δx₁·Δy₂ - Δy₁·Δx₂ = 0 | None (bilinear) | — |
| Perpendicular | Δx₁·Δx₂ + Δy₁·Δy₂ = 0 | None (bilinear) | — |
| Angle(L₁,L₂,θ) | atan2(Y,X) - θ = 0 | D₁²=0 or D₂²=0 | clamp D² ≥ ε |

**Angle derivative** (from R1): `∂f/∂(line_i_params) = ±Δcoord / D_i²`.
The denominator `X²+Y²` simplifies to `D₁²·D₂²`.

### Worker A3: Distance/dimensional + ratio (Group 4+7) — 4 constraints

| Constraint | Equation | Notes |
|-----------|----------|-------|
| Distance(P,L,d) | cross/(‖L‖) - d = 0 | **Normalized** — quotient rule for line endpoints |
| HDistance(P₁,P₂,d) | x₂-x₁-d = 0 | Linear, trivial |
| VDistance(P₁,P₂,d) | y₂-y₁-d = 0 | Linear, trivial |
| Ratio(L₁,L₂,k) | D₁ - k·D₂ = 0 | Like EqualLength scaled by k |

**Point-line distance is the hardest derivative** — quotient rule across 6 params.
Test exhaustively against finite differences.

### Worker A4: Point-on-entity + circle (Group 3) — 3 constraints

| Constraint | Equation | Notes |
|-----------|----------|-------|
| OnEntity(P,Line) | cross product = 0 | Division-free (unnormalized OK for d=0!) |
| OnEntity(P,Circle) | ‖P-center‖ - r = 0 | sqrt singularity at center |
| EqualRadius | r₁ - r₂ = 0 | Linear, trivial (from Equal dispatch) |

**Important**: PointOnLine uses the **unnormalized** cross product
(not normalized distance) because the target distance is zero — the
normalization factor cancels. This avoids the line-length singularity.

### Worker A5: Tangent + compound (Group 5+6+7) — 5 constraints

| Constraint | Equation | Notes |
|-----------|----------|-------|
| Tangent(Line,Curve) | norm_dist(line,center) ± r = 0 | Normalized P-L distance to arc center |
| Symmetric(P₁,P₂,L) | dot(P₁P₂,L)=0, cross(L,mid-L_s)=0 | 2 eqs, division-free |
| EqualAngle(4 lines) | atan2(Y₁₂,X₁₂)-atan2(Y₃₄,X₃₄)=0 | 16 params, mirrored Angle blocks |
| EqualPointToLine | Ux(yp1-yp2)-Uy(xp1-xp2)=0 | Division-free (denominators cancel) |
| SameOrientation | atan2(dy₁,dx₁)-atan2(dy₂,dx₂)=0 | Same singularities as Angle |

**EqualPointToLine optimization** (from R1): both distances share the same
line, so ‖L‖ cancels in the subtraction. Use unnormalized cross product
difference — no division, no singularity.

### Worker A6: Constraint builder + unit tests

Implement `build_constraints()` that maps `SketchConstraint` variants to
`ConstraintImpl` variants using `ParamLayout`. This is the equivalent of the
current `constraint_mapping.rs` but targeting our own types.

**Entity-type dispatch logic:**
- `Equal { entity_a, entity_b }` → if both lines: EqualLength; if both circles: EqualRadius
- `Distance { entity_a, entity_b, value }` → if both points: P-P distance;
  if point+line: normalized P-L distance; if point+point via HDistance/VDistance: linear
- `OnEntity { point, entity }` → if line: PointOnLine; if circle: PointOnCircle

**Unit test framework:**
- For each constraint: known-satisfied config, verify residuals ≈ 0
- Perturb one param, verify residual changes sign/magnitude correctly
- Central finite-difference Jacobian verification:
  `∂f/∂x ≈ (f(x+h) - f(x-h)) / 2h` with `h = 1e-5`, tolerance `1e-7`
- Every nonlinear constraint must pass FD check at 3+ different configurations
- Test singularity guards: verify no NaN/Inf when points are coincident

## Deliverables

- `core/constraint.rs`: `impl ConstraintEq for ConstraintImpl` with all 21 variants
- `core/builder.rs`: `build_constraints()` function
- `tests/constraint_tests.rs`: unit tests for each constraint type
- `tests/jacobian_tests.rs`: finite-difference jacobian verification

## Verification

- `cargo test -p sketch-solver -- constraint` — all constraint unit tests pass
- `cargo test -p sketch-solver -- jacobian` — all FD checks pass within 1e-7
- No NaN/Inf from singularity guard tests
