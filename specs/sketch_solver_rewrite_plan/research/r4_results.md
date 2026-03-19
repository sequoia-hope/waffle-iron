# R4/R8 Research Results: Under-Constrained Strategy & Jacobian Scaling

**Source**: Gemini Deep Research
**Status**: Reviewed, actionable — major corrections to Fork B plan
**Feeds into**: Fork B (numerics), Wave 3 (integration)

---

## Summary

Resolves two critical design gaps: (1) how to handle under-constrained systems
during both static edits and interactive drag, and (2) where/how to apply
Jacobian scaling for mixed distance/angle units.

## Key Architectural Decisions

### 1. Three complementary regularization mechanisms

| Mechanism | Space | Scope | Purpose |
|-----------|-------|-------|---------|
| LM λ·diag(H) | Velocity | Per-iteration | Step size control, matrix invertibility |
| Weak springs μ‖x-x₀‖² | Position | Global across iterations | Prevent manifold drift |
| SVD minimum-norm | Velocity | Per-iteration | Diagnostic only (NOT in solver loop) |

These compose — they are NOT redundant.

- **LM damping** guarantees the normal equations are invertible and controls
  trust region size. Operates in velocity space (minimizes ‖δ‖).
- **Weak springs** prevent accumulated drift across multiple iterations by
  anchoring to a fixed global position x_init. Operates in position space.
- **SVD** is reserved for out-of-band diagnostics. NOT used in the inner loop.

### 2. SVD is OUT of the inner LM loop

This corrects our R2 plan which suggested SVD for under-constrained solve.

With weak springs appended to J (adding n rows of √μ·I), the augmented
Jacobian J_aug is guaranteed full-column rank. Therefore:
- J_aug^T · J_aug is strictly positive-definite
- Standard Cholesky (or QR) on the normal equations works perfectly
- No need for expensive O(mn²) SVD per iteration

SVD is used ONLY for:
- Rank determination (DOF count)
- Null space extraction (free parameter directions)
- Redundant/conflicting constraint detection
- Executed once per solve (or on-demand for UI), not per iteration

### 3. Weak spring implementation

Augment the Jacobian and residual:

```
J_aug = [ D_row · J  ]    F_aug = [ D_row · F(x)        ]
        [ √μ · I_n   ]            [ √μ · (x_k - x_init) ]
```

- μ = 1e-6 (small enough that geometric constraints dominate)
- x_init = pre-solve anchor (NOT the evolving iteration state)
- The √μ factor appears in both J and F because we're minimizing
  ‖J_aug·δ + F_aug‖², and the spring term expands to μ·‖x - x_init‖²

### 4. Drag operation protocol

1. **Mouse-down**: Capture x_init = current solved positions (entire sketch)
2. **Each frame**: Solve with:
   - Hard constraint: dragged point → cursor position (strong, like Dragged)
   - Weak springs: ALL other params → x_init (prevents null-space drift)
   - LM starting guess: x_prev (previous frame's solution = warm start)
3. **Mouse-up**: Remove Dragged constraint, keep solved positions

This guarantees under-constrained geometry moves ONLY if kinematically
forced by the drag chain. Zero null-space drift.

### 5. Warm start vs spring anchor (critical distinction)

| | LM starting guess (x₀) | Weak spring target |
|---|---|---|
| **Static edit** (dimension change) | x_prev (warm start) | x_init (pre-edit state) |
| **Drag frame** | x_prev (previous frame) | x_init (mouse-down state) |
| **File load** (cold start) | entity positions from file | same (x₀ = x_init) |

The starting guess and spring anchor serve different purposes:
- Starting guess → convergence speed (close to solution)
- Spring anchor → prevents drift (fixed reference point)

### 6. Row-only scaling (D_col = I)

All parameters are in meters (point coordinates + radii). No column
scaling needed — the parameter space is already dimensionally homogeneous.

Row scaling for mixed-unit residuals:
- Distance-type constraints: `D_row[i,i] = 1.0`
- Angle-type constraints: `D_row[i,i] = L_c`

Where L_c = characteristic length = bounding box diagonal of x_init,
clamped: `L_c = max(bbox_diagonal, 1.0)`.

Effect: angle residuals (radians) × L_c (meters) = arc-length (meters).
All residuals become dimensionless after scaling. All singular values
in the same unit space.

### 7. Static scaling (compute once, not per-iteration)

Scaling MUST be computed once before the LM loop from x_init, then frozen.
If L_c fluctuates per-iteration (as geometry moves), the objective function
landscape shifts under the solver, corrupting the gain ratio ρ and causing
rejected steps / stalling.

### 8. Marquardt damping: λ·diag(H) not λI

Use `H_damped = H + λ·diag(H)` where `H = J_aug^T · J_aug`.

`diag(H)` scales the trust region per-parameter by local curvature.
More principled than identity damping for systems where different
parameters have different gradient magnitudes.

### 9. Nielsen λ update rule

On accepted step (ρ > 0):
  `λ ← λ · max(1/3, 1 - (2ρ-1)³)`, reset ν = 2

On rejected step (ρ ≤ 0):
  `λ ← λ · ν`, then `ν ← 2ν`

This is mathematically grounded (adapts based on how well the linear
model predicted actual improvement), not heuristic like simple ×0.1/×10.

### 10. SVD diagnostics on SCALED, UN-AUGMENTED J

Two requirements that must both hold:
- **Scaled** (apply D_row): so singular values are in homogeneous units
  and rank threshold ε is meaningful
- **Un-augmented** (no weak springs): so springs don't artificially inflate
  rank, hiding true DOF and constraint dependencies

```rust
// Diagnostic SVD (out of band, not per-iteration)
let j_raw = build_jacobian(&x, &constraints);
let j_scaled = &d_row * &j_raw;  // row-scale only
let svd = j_scaled.transpose().svd(true, true);
// ... rank analysis on svd (per R3 algorithm)
```

---

## Revised LM Algorithm (Pseudocode)

```
fn lm_solve(x_init, constraints, options) -> SolveOutcome:
    // Phase 1: Static scaling
    L_c = max(bbox_diagonal(x_init), 1.0)
    D_row = diagonal(constraints.map(|c| if c.is_angle() { L_c } else { 1.0 }))
    μ = 1e-6
    n = x_init.len()

    // Phase 2: LM loop
    x = x_init.clone()  // or x_prev for warm start
    λ = 1e-3
    ν = 2.0

    for iter in 0..max_iterations:
        // Evaluate raw state
        F = residuals(x, constraints)           // m×1
        J = jacobian(x, constraints)            // m×n

        // Apply static row scaling
        F_s = D_row * F
        J_s = D_row * J

        // Check convergence on scaled residuals
        if ‖F_s‖∞ < tolerance:
            break  // converged

        // Augment with weak springs
        J_aug = vstack(J_s, √μ · I_n)          // (m+n)×n
        F_aug = vstack(F_s, √μ · (x - x_init)) // (m+n)×1

        // Normal equations with Marquardt damping
        H = J_aug.T * J_aug                     // n×n, positive definite
        g = J_aug.T * F_aug                     // n×1
        H_damped = H + λ · diag(H)
        δ = solve(H_damped, -g)                 // Cholesky

        // Trial step
        x_new = x + δ
        F_new = residuals(x_new, constraints)
        F_new_s = D_row * F_new
        F_aug_new = vstack(F_new_s, √μ · (x_new - x_init))

        // Gain ratio
        E_actual = ‖F_aug‖² - ‖F_aug_new‖²
        E_predicted = ‖F_aug‖² - ‖F_aug + J_aug * δ‖²
        ρ = E_actual / E_predicted

        if ρ > 0:
            x = x_new
            λ *= max(1.0/3.0, 1.0 - (2.0*ρ - 1.0).powi(3))
            ν = 2.0
        else:
            λ *= ν
            ν *= 2.0

        // Stuck check
        if ‖δ‖∞ < tolerance:
            break  // params not moving

    // Phase 3: Diagnostics (out of band)
    J_final = jacobian(x, constraints)
    J_scaled = D_row * J_final
    // SVD on scaled, un-augmented J for rank/DOF/conflicts

    return SolveOutcome { params: x, converged, iterations, jacobian: J_scaled }
```

---

## Constraint Type Classification for Scaling

| Constraint | Scale type | D_row entry |
|-----------|-----------|-------------|
| Coincident | Distance | 1.0 |
| Horizontal | Distance | 1.0 |
| Vertical | Distance | 1.0 |
| Parallel | **Angle** | L_c |
| Perpendicular | **Angle** | L_c |
| Tangent | Distance | 1.0 |
| Equal (length) | Distance | 1.0 |
| Equal (radius) | Distance | 1.0 |
| Symmetric (perp eq) | **Angle** | L_c |
| Symmetric (midpoint eq) | Distance | 1.0 |
| SymmetricH | Distance | 1.0 |
| SymmetricV | Distance | 1.0 |
| Midpoint | Distance | 1.0 |
| Distance (P-P) | Distance | 1.0 |
| Distance (P-L) | Distance | 1.0 |
| Angle | **Angle** | L_c |
| Radius | Distance | 1.0 |
| Diameter | Distance | 1.0 |
| OnEntity | Distance | 1.0 |
| Dragged | Distance | 1.0 |
| EqualAngle | **Angle** | L_c |
| Ratio | Distance | 1.0 |
| EqualPointToLine | Distance | 1.0 |
| SameOrientation | **Angle** | L_c |

Note: Symmetric(arbitrary line) has 2 equations — perpendicularity (angle-type)
and midpoint-on-line (distance-type). They get DIFFERENT D_row entries.
