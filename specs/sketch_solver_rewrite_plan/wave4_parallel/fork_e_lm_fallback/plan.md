# Wave 4 / Fork E: Levenberg-Marquardt Fallback

**Executor**: Claude fork (worktree), with Gemini workers
**Depends on**: Wave 3 (working Newton-Raphson solver)
**Parallel with**: Fork D (proptest), Fork F (JS elimination)
**Optional**: Spec marks this as Phase 4 (optional). Implement if time allows.
**Estimated scope**: ~200 lines

## Goal

Add Levenberg-Marquardt as a fallback when Newton-Raphson fails to converge.
Per spec: "damped Newton: (J^T J + λI) δ = -J^T f" with adaptive λ.

## Worker Breakdown

### Worker E1: LM solver (`core/lm.rs`)

```rust
pub fn lm_solve(
    x0: &[f64],
    constraints: &[ConstraintImpl],
    num_equations: usize,
    options: &SolveOptions,
) -> SolveOutcome;
```

**Algorithm:**
1. `x = x0.clone()`, `λ = 1e-3`
2. Loop up to `max_iterations`:
   a. Build residual F and Jacobian J
   b. Compute `H = J^T * J + λ * I`
   c. Compute `g = -J^T * F`
   d. Solve `H * δ = g` (Cholesky or QR)
   e. Trial: `x_new = x + δ`
   f. If `‖F(x_new)‖ < ‖F(x)‖`:
      - Accept: `x = x_new`, `λ *= 0.1`
   g. Else:
      - Reject: `λ *= 10.0`
   h. If `‖F(x)‖ < tolerance`: converged
3. Return SolveOutcome

### Worker E2: Fallback integration

Modify `newton_solve` (or the orchestrator in `solver.rs`) to try LM when
NR fails:

```rust
let outcome = newton_solve(&x0, &constraints, num_eq, &options);
if !outcome.converged {
    let lm_outcome = lm_solve(&x0, &constraints, num_eq, &options);
    if lm_outcome.converged {
        return lm_outcome;
    }
    // Both failed — return NR outcome (usually has better diagnostics)
}
```

### Worker E3: Tests

- Cases where NR diverges but LM converges (near-singular Jacobian)
- Cases where both converge (LM should give same answer, possibly slower)
- Performance: LM should not be significantly slower than NR for well-conditioned systems

## Deliverables

- `core/lm.rs`: Levenberg-Marquardt implementation
- Updated `solver.rs`: fallback chain
- `tests/lm_tests.rs`: LM-specific tests

## Verification

- `cargo test -p sketch-solver -- lm` — all pass
- Existing 59 oracle tests still pass (LM is a fallback, shouldn't change results)
