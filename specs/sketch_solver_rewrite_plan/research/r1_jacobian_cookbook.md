# R1: Analytic Jacobian Cookbook for 2D Geometric Constraints

**Feeds into**: Wave 2 / Fork A (constraint implementations)
**Priority**: Critical — every constraint needs correct derivatives

## What We Know

Our spec defines 24 constraint equations (21 code variants). The residual
functions are specified: `f(params) = 0`. We need the analytic partial
derivatives `∂f/∂xᵢ` for each constraint to build the Jacobian matrix.

The equations are listed in `specs/sketch_solver_rewrite.md` §Constraint Types.

## What We Need

For each of the following constraints, derive:

1. **The residual function** `f(x) → R` (or `R^n` for multi-equation constraints)
   written in terms of the parameter vector entries (x₁, y₁, x₂, y₂, etc.)

2. **Every non-zero partial derivative** `∂f/∂param` — the sparse Jacobian row

3. **Singularities**: where the Jacobian becomes undefined or zero (e.g., the
   derivative of `sqrt(dx²+dy²)` when `dx=dy=0`)

4. **Recommended guards**: how to handle each singularity numerically
   (clamp denominator? regularize? report degenerate?)

## Constraints to Cover

### Group 1: Linear (constant Jacobian)
- Coincident(P₁, P₂): `x₁-x₂=0, y₁-y₂=0`
- Horizontal(L): `y_start - y_end = 0`
- Vertical(L): `x_start - x_end = 0`
- HDistance(P₁,P₂,d): `x₂ - x₁ - d = 0`
- VDistance(P₁,P₂,d): `y₂ - y₁ - d = 0`
- Radius(C,r): `r_c - r = 0`
- Diameter(C,d): `r_c - d/2 = 0`
- SymmetricH(P₁,P₂): `x₁+x₂=0, y₁-y₂=0`
- SymmetricV(P₁,P₂): `x₁-x₂=0, y₁+y₂=0`
- Midpoint(P,L): `x_p - (x_s+x_e)/2 = 0, y_p - (y_s+y_e)/2 = 0`
- Dragged(P,tx,ty): `x_p-tx=0, y_p-ty=0`

These are trivial — include for completeness but focus effort on the nonlinear ones.

### Group 2: Nonlinear, common
- Distance(P₁,P₂,d): `sqrt((x₂-x₁)²+(y₂-y₁)²) - d = 0`
  - Singularity at P₁=P₂
- EqualLength(L₁,L₂): `sqrt(dx₁²+dy₁²) - sqrt(dx₂²+dy₂²) = 0`
  - Singularity when either line has zero length
- Parallel(L₁,L₂): `dx₁·dy₂ - dy₁·dx₂ = 0`
  - No singularity in Jacobian, but degenerate when both lines are zero-length
- Perpendicular(L₁,L₂): `dx₁·dx₂ + dy₁·dy₂ = 0`
  - Same as parallel
- Angle(L₁,L₂,θ): `atan2(dx₁·dy₂-dy₁·dx₂, dx₁·dx₂+dy₁·dy₂) - θ = 0`
  - Singularity when either line is zero-length
  - The atan2 derivative: `d/dx atan2(y,x) = ...` — derive carefully

### Group 3: Point-on-entity
- PointOnLine(P,L): signed distance from P to line through L.start, L.end
  - Express as: `(x_p-x_s)·(y_e-y_s) - (y_p-y_s)·(x_e-x_s) = 0` (cross product form)
  - Or normalized signed distance — which form has better numerical behavior?
- PointOnCircle(P,C): `sqrt((x_p-x_c)²+(y_p-y_c)²) - r = 0`
  - Singularity at P=center

### Group 4: Point-line distance
- PointLineDistance(P,L,d): signed distance from P to line L, minus d
  - Normalized form: `((x_p-x_s)(y_e-y_s) - (y_p-y_s)(x_e-x_s)) / ‖L‖ - d = 0`
  - Or unnormalized cross product form?
  - Which is better for the solver? Normalized has line-length in denominator (singularity).
    Unnormalized avoids the singularity but changes scale with line length.
  - **This is a real design question — need a recommendation.**

### Group 5: Tangent
- Tangent(Line,Arc): `dist(line, arc_center) - r = 0`
  - This is PointLineDistance with d=r. Same Jacobian structure.
- Tangent(Circle,Line): same formula with circle center
- Tangent(Arc,Arc): `dist(c₁,c₂) - (r₁ ± r₂) = 0`
  - Internal tangent: `dist - (r₁ - r₂) = 0`
  - External tangent: `dist - (r₁ + r₂) = 0`
  - How to decide internal vs external? Sign convention?

### Group 6: Symmetric about line
- Symmetric(P₁,P₂,L): P₁ reflected across L = P₂
  - This is 2 equations. What's the cleanest formulation?
  - Option A: midpoint of P₁P₂ lies on L, AND P₁P₂ is perpendicular to L
  - Option B: direct reflection formula
  - **Need the Jacobian for whichever formulation is recommended.**

### Group 7: Compound
- EqualAngle(L₁,L₂,L₃,L₄): angle(L₁,L₂) = angle(L₃,L₄)
- LengthRatio(L₁,L₂,k): `‖L₁‖ - k·‖L₂‖ = 0`
- EqualPointToLine(P₁,P₂,L): `dist(P₁,L) - dist(P₂,L) = 0`

## Desired Output

For each constraint:
```
## ConstraintName

Residual(s):
  f₁ = <expression in terms of param vector entries>
  f₂ = ... (if multi-equation)

Jacobian (non-zero entries):
  ∂f₁/∂x₁ = ...
  ∂f₁/∂y₁ = ...
  ...

Singularities:
  - When <condition>, <which derivative> is undefined
  - Guard: <recommended numerical treatment>

Notes:
  - <any implementation advice>
```

## References to Consult

- SolveSpace `src/constraint.cpp` — their constraint evaluation code
- FreeCAD `src/Mod/Sketcher/App/planegcs/Constraints.cpp`
- Zou et al. (2022) arXiv:2202.13795 — survey of GCS approaches
- Any textbook on geometric constraint solving
