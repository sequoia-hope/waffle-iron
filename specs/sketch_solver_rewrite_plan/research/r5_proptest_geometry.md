# R5: Property-Based Testing Strategies for Constraint Solvers

**Feeds into**: Wave 4 / Fork D (proptest)
**Priority**: High

## What We Know

We want proptest to exercise the solver with randomly generated sketches.
The spec's testing strategy defines: canonical cases, edge cases,
over-constrained cases, under-constrained cases, and numerical stability.

## What We Need

Practical proptest strategy design — how to generate random but meaningful
constraint systems, and what properties to check.

## Specific Questions

### Q1: Generating satisfiable constraint systems
The "seed and measure" approach: generate valid geometry, measure properties,
inject as constraints, perturb, solve, verify recovery.

- This only generates CONSISTENT constraint systems. How do we also
  generate INCONSISTENT ones (for testing over-constrained detection)?
- Strategy: take a consistent system and add one contradictory constraint?
- How do we know the added constraint is actually contradictory and not
  just redundant?

### Q2: Interesting vs trivial sketches
Random points + random constraints usually produce trivially solvable or
trivially unsolvable systems. The interesting cases are:

- Near-degenerate: almost-collinear points, almost-parallel lines
- Exactly degenerate: collinear, coincident, zero-length
- Large systems: 50+ constraints with tight coupling
- Mixed constraint types: geometric + dimensional on the same entities

How do we bias proptest generation toward these interesting cases?
- Custom strategies that construct specific topologies?
- "Mutation" approach: start with a known good sketch, randomly mutate?

### Q3: Shrinking geometric configurations
When proptest finds a failing case, it shrinks toward a minimal reproduction.
For geometric configurations:

- Shrinking point coordinates toward zero makes the sketch collapse.
  This may not find the actual minimal failure.
- Better shrinking: reduce the number of entities/constraints while
  preserving the failure?
- Are there existing proptest shrinkers for geometric data?

### Q4: Determinism
The solver must be deterministic (A4.2, A8). How to test this?

- Run the same sketch 100 times → assert identical results?
- This catches floating-point non-determinism from reordering, but
  not from platform differences.
- Any known sources of non-determinism in nalgebra's QR?

### Q5: Rotational invariance
A 2D constraint solver should produce rotationally consistent results:
if you rotate all input points by θ, the solved positions should also
rotate by θ.

- Is this actually true for all constraint types? Horizontal/Vertical
  constraints break rotational invariance (they reference absolute axes).
- Which constraints ARE rotation-invariant? Distance, Angle, Parallel,
  Perpendicular, Equal, Symmetric(about arbitrary line), Tangent, etc.
- This is a valuable property test for the subset of rotation-invariant
  constraints.

### Q6: Convergence guarantees
For well-constrained systems with a unique solution:
- Newton-Raphson should converge from ANY initial guess (given enough
  iterations). Is this true? (No — NR has finite basins of attraction.)
- How large is the basin of attraction for typical CAD sketches?
- Can we test: "solve from 10 random initial guesses → all converge to
  the same solution"?

## Desired Output

1. 5-7 concrete proptest strategies with code-level detail
2. Shrinking recommendations
3. List of properties to test, categorized by constraint type subset
4. Recommended proptest case counts for CI vs stress testing
5. Known false-positive patterns to watch for

## References

- proptest crate documentation — custom strategies and shrinkers
- geo crate's proptest usage (if any)
- kurbo crate's property tests (Raph Levien's geometry crate)
- QuickCheck papers on shrinking strategies
