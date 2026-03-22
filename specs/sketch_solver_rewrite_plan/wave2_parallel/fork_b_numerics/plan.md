# Wave 2 / Fork B: Numerical Core

**Executor**: Claude fork (worktree), with Gemini workers
**Depends on**: Wave 1 (ParamLayout, ConstraintEq trait)
**Parallel with**: Fork A (constraints), Fork C (render)
**Estimated scope**: ~600 lines

## Goal

Implement the Levenberg-Marquardt solver with weak spring augmentation,
row-scaled Jacobian, SVD rank analysis, and status classification.
This is Layer 2 of the spec's three-layer architecture.

Uses `nalgebra::DMatrix<f64>` / `DVector<f64>` throughout. Constraints
provide sparse triplets via `ConstraintEq::jacobian()`; the solver
assembles these into dense matrices for the LM loop.

**Primary references**:
- `research/r2_results.md` — LM as primary solver
- `research/r3_results.md` — SVD rank analysis
- `research/r4_results.md` — weak springs, scaling, unified algorithm

## Worker Breakdown

### Worker B1: Augmented LM solver (`core/lm.rs`)

LM is the primary solver (not a fallback). Per R2 research, pure NR with
ad-hoc damping is a hack. Per R4 research, weak springs compose with LM
damping to provide both velocity-space and position-space regularization.

```rust
pub struct SolveOptions {
    pub max_iterations: usize,     // 50 (spec)
    pub tolerance: f64,            // TAU_MODEL = 1e-7 (A14)
    pub lambda_init: f64,          // 1e-3 (warm start) or 1.0 (cold start)
    pub spring_mu: f64,            // 1e-6 (weak spring stiffness)
}

pub struct SolveOutcome {
    pub params: Vec<f64>,          // Solved parameter vector
    pub converged: bool,
    pub iterations: usize,
    pub final_residual_norm: f64,
    pub jacobian_scaled: DMatrix<f64>,  // Final SCALED Jacobian (for diagnostics)
}

pub fn lm_solve(
    x0: &[f64],                    // Starting guess (warm start or initial)
    x_anchor: &[f64],              // Spring anchor (pre-edit or mouse-down state)
    constraints: &[ConstraintImpl],
    constraint_scale_types: &[ScaleType], // Distance or Angle per equation
    num_equations: usize,
    options: &SolveOptions,
) -> SolveOutcome;

pub enum ScaleType { Distance, Angle }
```

**Algorithm (from R4 unified pseudocode):**

Phase 1: Static scaling (compute once)
1. `L_c = max(bbox_diagonal(x_anchor), 1.0)`
2. Build `D_row`: diagonal m×m, `1.0` for distance eqs, `L_c` for angle eqs
3. `μ = spring_mu`, `n = x0.len()`

Phase 2: LM loop
1. `x = x0.clone()`, `λ = lambda_init`, `ν = 2.0`
2. Loop up to `max_iterations`:
   a. Evaluate raw `F(x)` and `J(x)` [un-augmented, dense DMatrix]
   b. Apply static row scaling: `F_s = D_row · F`, `J_s = D_row · J`
   c. Check convergence: `‖F_s‖∞ < tolerance` → done
   d. Augment with weak springs:
      - `J_aug = vstack(J_s, √μ · I_n)` — size (m+n)×n
      - `F_aug = vstack(F_s, √μ · (x - x_anchor))` — size (m+n)×1
   e. Normal equations: `H = J_aug^T · J_aug`, `g = J_aug^T · F_aug`
   f. Marquardt damping: `H_damped = H + λ · diag(H)`
   g. Solve: `H_damped · δ = -g` (Cholesky, fallback to QR)
   h. Trial: `x_new = x + δ`, compute `F_new`, scale, augment → `F_aug_new`
   i. Gain ratio: `ρ = (‖F_aug‖² - ‖F_aug_new‖²) / (‖F_aug‖² - ‖F_aug + J_aug·δ‖²)`
   j. If ρ > 0 (actual improvement):
      - Accept: `x = x_new`
      - Nielsen update: `λ *= max(1/3, 1 - (2ρ-1)³)`, reset `ν = 2`
   k. Else (step worsened):
      - Reject step (keep x)
      - `λ *= ν`, `ν *= 2`
   l. Stuck check: `‖δ‖∞ < tolerance` → break
3. Return SolveOutcome with final scaled Jacobian for diagnostics

**Key implementation details:**
- Dense `nalgebra::DMatrix<f64>` throughout (≤200 params)
- NO SVD in the inner loop — weak springs guarantee J_aug full-column rank
- Cholesky on H_damped (positive definite by construction)
- Marquardt damping `λ·diag(H)` not `λI` — adapts to local curvature
- Nielsen λ update (mathematically grounded, not heuristic ×0.1/×10)
- Static row scaling freezes L_c before the loop (no per-iteration recompute)
- Spring anchor x_anchor is FIXED throughout the solve (not the evolving x)

**Warm start vs spring anchor protocol (from R4):**

| Scenario | LM starting guess (x0) | Spring anchor (x_anchor) |
|----------|------------------------|--------------------------|
| Static edit (dimension change) | x_prev (last solved positions) | x_init (pre-edit state) |
| Drag frame | x_prev (previous frame result) | x_init (mouse-down snapshot) |
| Cold start (file load) | Entity positions from file | Same as x0 |

The starting guess and spring anchor serve different purposes:
- Starting guess → convergence speed (close to solution)
- Spring anchor → prevents drift (fixed reference point, never updated mid-solve)

**Scale type classification (from R4, reference for D_row construction):**

| Constraint | Scale type | D_row entry |
|-----------|-----------|-------------|
| Coincident, Horizontal, Vertical | Distance | 1.0 |
| Midpoint, SymmetricH, SymmetricV | Distance | 1.0 |
| Dragged, Radius, Diameter | Distance | 1.0 |
| Distance (P-P), Distance (P-L) | Distance | 1.0 |
| OnEntity, Tangent, Equal, Ratio | Distance | 1.0 |
| EqualPointToLine | Distance | 1.0 |
| Parallel, Perpendicular | **Angle** | L_c |
| Angle, EqualAngle, SameOrientation | **Angle** | L_c |
| Symmetric (perpendicularity eq) | **Angle** | L_c |
| Symmetric (midpoint-on-line eq) | Distance | 1.0 |

Note: Symmetric(arbitrary line) has 2 equations with DIFFERENT scale types.

### Worker B2: SVD rank analysis (`core/rank.rs`)

Per R3 research — use SVD (not QR) for all diagnostics.
Per R4 research — SVD on SCALED, UN-AUGMENTED Jacobian.

```rust
pub struct RankAnalysis {
    pub rank: usize,
    pub dof: usize,                        // num_params - rank
    pub dependent_constraints: Vec<DepInfo>,
    pub free_params: Vec<FreeParam>,
}

pub enum DepInfo {
    Redundant { constraint_ids: Vec<usize> },
    Conflicting { constraint_ids: Vec<usize>, magnitude: f64 },
}

pub fn analyze_rank(
    jacobian_scaled: &DMatrix<f64>,    // SCALED, un-augmented J
    residual_scaled: &DVector<f64>,    // SCALED residual
    num_params: usize,
    num_equations: usize,
    eq_to_constraint: &[usize],
) -> RankAnalysis;
```

**Algorithm (from R3, updated per R4):**
1. SVD of J_scaled^T: `J_scaled^T = U Σ V^T`
2. Rank threshold: `ε = max(m,n) · f64::EPSILON · σ₁` (relative)
   - Works correctly because J_scaled is dimensionless (all units homogenized)
3. rank = count of σ_i > ε
4. DOF = num_params - rank
5. Dependent constraint detection (null vectors of J_scaled^T):
   - For each `v_k = vt.row(i)` where `i >= rank`:
     - Project scaled residual: `r_k = v_k · F_scaled`
     - `|r_k| ≤ TAU_MODEL` → redundant
     - `|r_k| > TAU_MODEL` → conflicting
     - Involved constraints: indices where `|v_k[j]| > 0.01`, mapped through eq_to_constraint
6. Free params: SVD of J_scaled directly, last rows of V^T for null space directions
   - Classify per-point FreeAxis from null vector components

### Worker B3: Status classification (`core/status.rs`)

Replace current `status.rs` (which reads slvs results) with:

```rust
pub fn classify_solve(
    outcome: &SolveOutcome,
    rank: &RankAnalysis,
    num_params: usize,
) -> SolveStatus;
```

**Logic:**
- If `outcome.converged` and `rank.dof == 0` → `FullyConstrained`
- If `outcome.converged` and `rank.dof > 0` → `UnderConstrained { dof }`
- If `!outcome.converged` and rank has conflicts →
  `OverConstrained { conflicts }` (constraint IDs from eq_to_constraint)
- If `!outcome.converged` otherwise → `SolveFailed { reason }`

### Worker B4: Integration tests for numerics

Test the solver on known systems WITHOUT going through the full sketch API:
- 2 params, 2 equations (fully constrained point) — converges in <10 iterations
- 4 params, 3 equations (under-constrained, DOF=1) — weak springs keep geometry stable
- 2 params, 3 equations (over-constrained) — correct conflict detection
- Near-singular Jacobian (parallel lines + small angle constraint)
- Convergence from distant initial guess (cold start, high initial λ)
- Drag simulation: verify no null-space drift over 100 sequential solves
- Mixed distance + angle constraints: verify scaling produces good condition number

## Deliverables

- `core/lm.rs`: Augmented Levenberg-Marquardt with weak springs + row scaling
- `core/rank.rs`: SVD rank analysis on scaled Jacobian
- `core/status.rs`: Status classification (new file, replaces old `status.rs`)
- `tests/lm_tests.rs`: Numerical solver tests
- `tests/rank_tests.rs`: Rank analysis tests

## Verification

- `cargo test -p sketch-solver -- lm` — solver tests pass
- `cargo test -p sketch-solver -- rank` — rank analysis tests pass
- Verify convergence in ≤ 50 iterations for all test cases
- Verify no NaN/Inf from near-singular systems
- Verify drag stability (no drift over sequential solves)
