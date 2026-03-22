# R4: Under-Constrained System Strategies

**Feeds into**: Wave 2 / Fork B (numerics)
**Priority**: High

## What We Know

The spec prescribes weak springs: `minimize ‖F(x)‖² + μ‖x - x₀‖²` with
μ = 1e-6. This keeps unconstrained geometry near its current position.

## What We Need

The weak springs approach has subtleties. We need to get them right or
the solver will produce surprising behavior during interactive sketching.

## Specific Questions

### Q1: μ value selection
- μ = 1e-6 is prescribed. Is this relative to constraint residual scale?
- If constraints produce residuals in meters (distances) and radians
  (angles), and parameters are in meters, does μ = 1e-6 make sense
  for both?
- Does μ need to scale with the number of parameters?
- What do SolveSpace and FreeCAD use?

### Q2: Interaction with dragging
The spec says "during dragging, μ can be tuned for responsiveness."
- When dragging a point, the dragged constraint is strong (fixes position).
  The rest of the sketch should respond minimally.
- Should μ be LARGER during dragging (stronger springs = less motion)?
  Or smaller (weaker springs = more fluid response)?
- Is there a different strategy entirely for dragging? (e.g., solve the
  constrained system exactly, then project unconstrained params to nearest?)

### Q3: Minimum-norm vs weak springs
Two approaches for under-constrained systems:
- **Weak springs**: augment the objective with `μ‖x - x₀‖²`
- **Minimum-norm**: solve `min ‖x - x₀‖ subject to F(x) = 0`

These are related but not identical. Minimum-norm is the limit as μ→0.
- Which gives better interactive behavior?
- Is minimum-norm achievable via QR? (The minimum-norm solution to
  `Jδ = -F` is `δ = J^T (J J^T)^{-1} (-F)` — is this what QR
  naturally gives for wide rectangular systems?)

### Q4: Multiple solution basins
Under-constrained systems have continuous families of solutions. But they
may also have discrete solution basins (e.g., a distance constraint has
two solutions — point on either side).
- How do weak springs interact with basin selection?
- Can weak springs cause the solver to "snap" to a different basin
  during dragging?
- How do production solvers handle this?

### Q5: Warm starting between solves
The spec mentions caching previous solution as initial guess.
- When the user adds a constraint, the previous solved positions are
  the warm start. This is natural.
- When the user drags, each frame's solved position is the warm start
  for the next frame. This gives smooth motion.
- Are there cases where warm starting causes problems? (e.g., getting
  stuck in a local minimum when a global solution exists elsewhere)

## Desired Output

1. Recommended μ value with justification
2. Whether μ should differ for drag vs non-drag solves
3. Minimum-norm vs weak springs recommendation
4. Basin selection strategy
5. Warm start best practices

## References

- SolveSpace system.cpp — how SolveByNewton handles under-constrained
- FreeCAD GCS.cpp — qsolve(), how it handles redundant DOF
- Nocedal & Wright — regularization and trust region methods
