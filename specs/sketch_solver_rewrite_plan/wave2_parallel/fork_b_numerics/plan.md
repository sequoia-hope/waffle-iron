# Wave 2 / Fork B: Numerical Core

**Executor**: Claude fork (worktree), with Gemini workers
**Depends on**: Wave 1 (ParamLayout, ConstraintEq trait)
**Parallel with**: Fork A (constraints), Fork C (render)
**Estimated scope**: ~500 lines

## Goal

Implement the Newton-Raphson solver, QR rank analysis, and status classification.
This is Layer 2 of the spec's three-layer architecture.

## Worker Breakdown

### Worker B1: Levenberg-Marquardt solver (`core/lm.rs`)

LM is the primary solver (not a fallback). Per R2 research, pure NR with
ad-hoc damping is a hack. LM elegantly handles warm starts (small λ),
cold starts (large λ), and near-singular Jacobians in one algorithm.

See `research/r2_results.md` for full rationale.

```rust
pub struct SolveOptions {
    pub max_iterations: usize,     // 50 (spec)
    pub tolerance: f64,            // TAU_MODEL = 1e-7 (A14)
    pub lambda_init: f64,          // 1e-3 (warm start) or 1.0 (cold start)
    pub lambda_up: f64,            // 10.0 (increase on divergent step)
    pub lambda_down: f64,          // 0.1 (decrease on successful step)
}

pub struct SolveOutcome {
    pub params: Vec<f64>,          // Solved parameter vector
    pub converged: bool,
    pub iterations: usize,
    pub final_residual_norm: f64,
    pub jacobian: DMatrix<f64>,    // Final Jacobian (for diagnostics)
}

pub fn lm_solve(
    x0: &[f64],
    constraints: &[ConstraintImpl],
    num_equations: usize,
    options: &SolveOptions,
) -> SolveOutcome;
```

**Algorithm:**
1. `x = x0.clone()`, `λ = lambda_init`
2. Loop up to `max_iterations`:
   a. Build residual vector F and Jacobian J [un-augmented, dense DMatrix]
   b. Check convergence: `‖F‖∞ < tolerance` → done
   c. Compute LM step: `(J^T J + λI) δ = -J^T F`
      - For under-constrained (m < n): use SVD minimum-norm with λ regularization
   d. Trial: `x_new = x + δ`, compute `F_new`
   e. If `‖F_new‖ < ‖F‖` (actual reduction):
      - Accept: `x = x_new`, `λ *= lambda_down`
   f. Else:
      - Reject step, `λ *= lambda_up`
   g. Also check: `‖δ‖∞ < tolerance` (params not moving → stuck or converged)
3. Return SolveOutcome with final Jacobian for diagnostics

**Key implementation details:**
- Dense `nalgebra::DMatrix<f64>` throughout (≤200 params)
- SVD for under-constrained minimum-norm (nalgebra QR doesn't handle wide matrices)
- Jacobian scaling: `D_row * J * D_col` for mixed-unit conditioning
- λ naturally handles warm/cold starts — no separate codepaths

### Worker B2: SVD rank analysis (`core/rank.rs`)

Per R3 research results — use SVD (not QR) for all diagnostics.
See `research/r3_results.md` for full nalgebra API reference.

```rust
pub struct RankAnalysis {
    pub rank: usize,
    pub dof: usize,                        // num_params - rank
    pub dependent_constraints: Vec<DepInfo>, // Which constraints are dependent
    pub free_params: Vec<FreeParam>,        // Which params are free and in which direction
}

pub enum DepInfo {
    Redundant { constraint_ids: Vec<usize> },
    Conflicting { constraint_ids: Vec<usize>, magnitude: f64 },
}

pub fn analyze_rank(
    jacobian: &DMatrix<f64>,
    residual: &DVector<f64>,
    num_params: usize,
    num_equations: usize,
    eq_to_constraint: &[usize],  // equation row → constraint index
) -> RankAnalysis;
```

**Algorithm (from R3 research):**
1. SVD of J^T: `J^T = U Σ V^T`
2. Rank threshold: `ε = max(m,n) · f64::EPSILON · σ₁` (relative)
3. rank = count of σ_i > ε
4. DOF = num_params - rank
5. For each null vector `v_k = vt.row(i)` where `i >= rank`:
   - Project residual: `r_k = v_k · F(x*)`
   - `|r_k| ≤ TAU_MODEL` → redundant
   - `|r_k| > TAU_MODEL` → conflicting
   - Involved constraints: indices where `|v_k[j]| > 0.01`, mapped through eq_to_constraint
6. Free params: SVD of J directly, last columns of V for null space directions
   - Classify per-point FreeAxis from null vector components (simplified heuristic)

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
- If `!outcome.converged` and rank indicates dependent equations →
  `OverConstrained { conflicts }` (map dependent eq indices back to
  constraint IDs — this requires a mapping from equation row → constraint index)
- If `!outcome.converged` otherwise → `SolveFailed { reason }`

### Worker B4: Integration tests for numerics

Test the solver on known systems WITHOUT going through the full sketch API:
- 2 params, 2 equations (fully constrained point)
- 4 params, 3 equations (under-constrained, DOF=1)
- 2 params, 3 equations (over-constrained)
- Near-singular Jacobian (parallel lines + small angle constraint)
- Convergence from distant initial guess

## Deliverables

- `core/newton.rs`: Newton-Raphson implementation
- `core/rank.rs`: QR rank analysis
- `core/status.rs`: Status classification (new file, replaces old `status.rs`)
- `tests/newton_tests.rs`: Numerical solver tests

## Verification

- `cargo test -p sketch-solver -- newton` — solver tests pass
- `cargo test -p sketch-solver -- rank` — rank analysis tests pass
- Verify convergence in ≤ 50 iterations for all test cases
