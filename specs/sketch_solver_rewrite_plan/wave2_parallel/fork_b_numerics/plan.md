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
