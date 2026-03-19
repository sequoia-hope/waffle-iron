# Fork E: COLLAPSED into Fork B

**Status**: Merged into Wave 2 / Fork B (numerics)

Per R2 research results, Levenberg-Marquardt is the **primary solver**, not a
fallback. The "NR primary + LM fallback" architecture from the original spec
is replaced by LM-as-primary, which elegantly handles both warm starts
(small λ → near-Newton) and cold starts (large λ → gradient descent) in
one algorithm.

See:
- `research/r2_results.md` — full rationale
- `wave2_parallel/fork_b_numerics/plan.md` Worker B1 — LM solver spec

Fork E no longer exists as independent work. Its test cases (near-singular
Jacobian convergence) are absorbed into Fork B Worker B4.
