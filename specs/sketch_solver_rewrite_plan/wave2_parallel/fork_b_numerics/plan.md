# Wave 2 / Fork B: Numerical Core

**Executor**: Claude fork (worktree), with Gemini workers
**Depends on**: Wave 1 (ParamLayout, ConstraintEq trait)
**Parallel with**: Fork A (constraints), Fork C (render)
**Estimated scope**: ~500 lines

## Goal

Implement the Newton-Raphson solver, QR rank analysis, and status classification.
This is Layer 2 of the spec's three-layer architecture.

## Worker Breakdown

### Worker B1: Newton-Raphson (`core/newton.rs`)

The core solve loop, per spec §Layer 2:

```rust
pub struct SolveOptions {
    pub max_iterations: usize,     // 50 (spec)
    pub tolerance: f64,            // TAU_MODEL = 1e-7 (A14)
    pub spring_mu: f64,            // 1e-6 (spec: under-constrained penalty)
}

pub struct SolveOutcome {
    pub params: Vec<f64>,          // Solved parameter vector
    pub converged: bool,
    pub iterations: usize,
    pub final_residual_norm: f64,
}

pub fn newton_solve(
    x0: &[f64],
    constraints: &[ConstraintImpl],
    num_equations: usize,
    options: &SolveOptions,
) -> SolveOutcome;
```

**Algorithm:**
1. `x = x0.clone()`
2. Loop up to `max_iterations`:
   a. Build residual vector `F` (length = num_equations)
   b. Build Jacobian as sparse triplets, convert to dense `nalgebra::DMatrix`
   c. If under-constrained (more params than equations), augment system:
      - Append `sqrt(μ) * (x_i - x0_i)` to residuals
      - Append `sqrt(μ) * I` rows to Jacobian
   d. QR decomposition of (augmented) Jacobian
   e. Solve `J * δ = -F` via QR
   f. `x += δ`
   g. If `‖F‖ < tolerance`: converged, break
3. Return SolveOutcome

**Key implementation details:**
- Dense matrices via `nalgebra::DMatrix<f64>` — spec says this is fine for
  <200 params, and typical sketches are well under that
- QR via `nalgebra::linalg::QR`
- Sparse → dense conversion: iterate triplets, set entries in DMatrix

### Worker B2: QR rank analysis (`core/rank.rs`)

Per spec §Rank analysis:

```rust
pub struct RankAnalysis {
    pub rank: usize,
    pub dof: usize,                    // num_params - rank
    pub dependent_equations: Vec<usize>, // Which constraint equations are redundant
}

pub fn analyze_rank(
    jacobian: &DMatrix<f64>,
    num_params: usize,
    num_equations: usize,
    tolerance: f64,
) -> RankAnalysis;
```

**Algorithm:**
- Column-pivoted QR of the Jacobian (`nalgebra` ColPivQR)
- Rank = number of diagonal R entries with |r_ii| > tolerance
- DOF = num_params - rank
- Dependent equations: rows that map to zero-magnitude R diagonals
  (these correspond to redundant or conflicting constraints)

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
