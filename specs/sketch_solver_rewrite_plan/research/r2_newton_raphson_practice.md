# R2: Newton-Raphson in Practice for 2D Geometric Constraint Systems

**Feeds into**: Wave 2 / Fork B (numerics)
**Priority**: Critical

## What We Know

Our spec prescribes:
- Newton-Raphson with analytic Jacobian (Layer 2)
- QR decomposition via nalgebra for the linear solve step
- Convergence at TAU_MODEL = 1e-7
- Max 50 iterations
- Weak springs (μ=1e-6 penalty toward x₀) for under-constrained systems
- LM fallback when NR fails

## What We Need

Practical implementation guidance — not the textbook algorithm, but the
engineering decisions that make it work reliably for CAD sketches.

## Specific Questions

### Q1: Line search / damping
Plain Newton takes the full step `x += δ` each iteration. For geometric
constraints, does this reliably converge, or do we need line search
(backtracking along δ until residual actually decreases)?

- Does SolveSpace use line search? Does FreeCAD?
- What's the simplest effective damping strategy?
- Is there a case where full-step Newton diverges on a typical CAD sketch?

### Q2: The augmented system for under-constrained sketches
The spec says: minimize `‖F(x)‖² + μ‖x - x₀‖²`. This means augmenting
the system with extra rows in the Jacobian and residual.

- Should we literally append `sqrt(μ)·I` rows to J and `sqrt(μ)·(x-x₀)` to F?
  Or is there a more numerically stable formulation?
- Does this change the QR rank analysis? (The augmented system is always
  full-rank if μ > 0, which defeats rank-based DOF detection.)
- **Recommendation**: should we do rank analysis on the UN-augmented Jacobian,
  then solve the augmented system? That seems right but want confirmation.

### Q3: Convergence criterion
- Residual norm `‖F(x)‖ < τ` vs parameter step `‖δ‖ < τ` vs both?
- SolveSpace checks... what exactly?
- For dimensional constraints (distance = 0.1m), the residual is in meters.
  For angular constraints (angle = 45°), the residual is in radians.
  These have different scales. Does this cause convergence issues?
- Should we use relative convergence `‖F‖/‖F₀‖ < τ` or absolute?

### Q4: Initial guess sensitivity
- The spec says "initial configuration = current sketch positions."
- When a user adds a constraint to an existing sketch, positions are already
  close to a solution. This is the easy case.
- When loading a sketch from scratch (all points at default positions),
  the initial guess may be far from any solution.
- How do production solvers handle cold starts?
- Is there any preprocessing (e.g., solving linear constraints first)?

### Q5: Iteration count in practice
- For a well-conditioned fully-constrained sketch (rectangle, 8 params,
  8 constraint equations), how many NR iterations are typical?
- At what sketch complexity does 50 iterations become insufficient?
- Is there a relationship between DOF and convergence speed?

### Q6: The linear solve step
- We're using QR decomposition (nalgebra) for `J·δ = -F`.
- For a square well-conditioned system, this is standard.
- For a rectangular system (under-constrained: more params than equations),
  QR gives the minimum-norm solution — is this what we want, or do we
  want the least-squares solution?
- For over-constrained (more equations than params), QR least-squares
  minimizes `‖Jδ + F‖`. Is this the right behavior?

### Q7: Sparse vs dense
- nalgebra DMatrix (dense) vs nalgebra_sparse or faer (sparse)?
- For N < 200 params, dense QR is likely fine. Confirm this.
- At what N does sparse become necessary?
- SolveSpace uses... dense? FreeCAD uses... Eigen dense?

## Desired Output

A practical implementation guide:
1. Recommended damping strategy (with code-level detail)
2. Exact convergence criterion to use
3. Augmentation strategy for under-constrained systems (how to
   separate rank analysis from the solve)
4. Any preprocessing steps worth implementing
5. Expected iteration counts for canonical sketches
6. Dense vs sparse recommendation with threshold

## References to Consult

- SolveSpace `src/system.cpp` — SolveByNewton(), SolveRank()
- FreeCAD `src/Mod/Sketcher/App/planegcs/GCS.cpp` — solve(), solveSubSystem()
- Nocedal & Wright, "Numerical Optimization" — Newton methods chapter
- Kelley, "Iterative Methods for Linear and Nonlinear Equations"
