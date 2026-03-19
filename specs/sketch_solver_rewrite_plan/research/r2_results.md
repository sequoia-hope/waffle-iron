# R2 Research Results: Newton-Raphson in Practice

**Source**: Gemini Deep Research
**Status**: Reviewed, architectural decisions extracted
**Feeds into**: Fork B (numerics), Fork E (LM — now core, not optional)

---

## Key Architectural Decisions

### 1. Primary solver: Levenberg-Marquardt (not pure Newton-Raphson)

The spec prescribes "Newton-Raphson primary, LM fallback." Research reveals
this is the wrong framing. Pure NR with no damping diverges when the linear
approximation is poor. Bolting on ad-hoc line search or step-halving to
compensate is a hack (P9 violation).

The correct architecture: **LM as the primary solver.** LM is a damped Newton
method that continuously interpolates between Newton (fast convergence near
solution) and gradient descent (guaranteed progress far from solution) via
adaptive λ:

```
(J^T J + λI) δ = -J^T F
```

- Small λ → near-Newton behavior (quadratic convergence)
- Large λ → gradient descent (guaranteed residual decrease)
- Adaptive: decrease λ on successful steps, increase on divergent steps

This is one clean algorithm. No fallback chain, no retry loops, no step-halving.
FreeCAD uses this as its primary approach. SolveSpace's pure NR works only
because it relies on warm starts being very close to the solution.

For our solver: LM with adaptive λ handles both warm starts (small λ, fast
convergence) and cold starts (large λ, safe convergence) naturally.

### 2. SVD for everything ≤200 params

Research confirms + R3 establishes:
- **Diagnostics**: SVD on un-augmented J for rank, DOF, conflict detection
- **Under-constrained solve**: SVD gives minimum-norm solution directly via
  `Δx = V Σ⁺ U^T b` — nalgebra's QR does NOT give minimum-norm for wide matrices
- **Dense throughout**: ≤200 params is firmly in the dense regime. Sparse deferred.

One SVD decomposition serves both diagnostic and solve purposes.

### 3. Rank analysis on un-augmented J, then augment for solve

Strict sequential order:
1. Build un-augmented Jacobian J (pure geometric derivatives)
2. SVD of J for rank analysis and diagnostics
3. Augment with λI for the LM solve step
4. Solve augmented system for parameter update δ

Never analyze rank on the augmented matrix — the regularization artificially
inflates rank, hiding real DOF and constraint dependencies.

### 4. Jacobian scaling for mixed units

Distance constraints produce residuals in meters, angle constraints in radians.
Without scaling, the condition number of J explodes.

**Column scaling** (variable normalization):
- Point coordinates (meters): scale by characteristic sketch size
- Radii (meters): same scale
- Result: all params roughly unit-scale

**Row scaling** (constraint weighting):
- Distance-type residuals: divide by characteristic length
- Angle-type residuals: leave unscaled (already ~unit)
- Result: all residuals roughly unit-scale

Implementation: `J_scaled = D_row * J * D_col`, solve in scaled space,
then unscale the parameter update: `δ_real = D_col * δ_scaled`.

Characteristic length = bounding box diagonal of current sketch positions.
For empty sketches, default to 1.0 (meters).

### 5. Cold start strategy

On file load (no warm start available):
- Initialize with high λ (conservative, gradient-descent-like steps)
- LM naturally relaxes λ as it converges
- No special code needed — the adaptive λ handles this automatically

This is why LM-as-primary is elegant: it handles cold starts and warm
starts with the same algorithm, just different effective λ.

### 6. Convergence criterion

**Absolute residual norm**: `‖F(x)‖∞ < TAU_MODEL` (1e-7)

Using infinity norm (max component) rather than L2 norm because:
- Single badly-satisfied constraint is caught
- Scale-independent (doesn't grow with number of constraints)
- TAU_MODEL = 1e-7 is appropriate for meter-scale geometry (A14)

Also check parameter step: `‖δ‖∞ < TAU_MODEL` — if params aren't moving,
we've either converged or are stuck.

Converged = both conditions met.
Stuck = parameter step small but residual still large → over-constrained.

### 7. Iteration counts (expected, to be validated)

- Fully constrained warm start (rectangle): 3-5 iterations
- Under-constrained warm start: 2-4 iterations
- Cold start (file load): 10-20 iterations
- Difficult (near-singular): up to 50 iterations
- Maximum: 50 iterations (spec)

These are estimates — the 59 oracle tests will provide ground truth.

### 8. Dense vs sparse: not our problem

Dense QR/SVD is fine up to ~300 params. Typical sketches have 20-100 params.
Sparse deferred indefinitely. If someone builds a 500-param sketch, they can
wait 50ms for the solve.

---

## Revised Fork B / Fork E Architecture

The research collapses Fork B (numerics) and Fork E (LM fallback) into
a single coherent design. Fork E is no longer optional — LM IS the solver.

**Revised solver pipeline:**
```
1. Build ParamLayout from entities
2. Build ConstraintImpl list from constraints
3. Scale Jacobian columns and residual rows
4. LM solve loop:
   a. Compute F(x) and J(x) [un-augmented]
   b. SVD of J → rank, DOF, diagnostics
   c. If converged → break
   d. Augment: (J^T J + λI) δ = -J^T F
      OR for under-constrained: SVD minimum-norm with spring penalty
   e. Update x, adjust λ based on actual vs predicted reduction
5. Unscale parameters
6. Classify status from SVD diagnostics
7. Extract positions and profiles
```
