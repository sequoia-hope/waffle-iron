# R3: Rank Analysis and Conflicting Constraint Identification

**Feeds into**: Wave 2 / Fork B (numerics), Wave 3 (integration)
**Priority**: Critical — this is the hardest part of the solver

## What We Know

The spec requires:
- DOF = num_params - rank(J)
- Identify which specific constraints are conflicting (not just "system failed")
- Identify which parameters are free and in which direction (for UI)
- QR decomposition provides rank, pivots, null space

## What We Need

The gap between "QR gives you rank" and "tell the user constraint #7 conflicts
with constraint #12" is significant. This research fills that gap.

## Specific Questions

### Q1: Column-pivoted QR for rank determination
- nalgebra provides `ColPivQR`. How exactly do you determine rank from it?
- The diagonal of R has entries in decreasing magnitude (due to pivoting).
  Rank = number of |R_ii| > threshold. What threshold?
- Is TAU_MODEL (1e-7) the right threshold, or should it be relative to
  the largest R_ii?
- What does SolveSpace use for its rank determination?

### Q2: Mapping rank deficiency to conflicting constraints
When rank < num_equations, some constraints are dependent. We need to
identify WHICH ones.

- The column permutation from pivoted QR reorders constraints by
  "importance." The last (num_equations - rank) columns correspond to
  dependent constraints. Is this correct?
- Wait — QR column pivoting operates on columns (parameters), not rows
  (constraints). How do we identify dependent ROWS (constraints)?
- Do we need to do a ROW-pivoted decomposition instead? Or transpose
  and do column-pivoted on J^T?
- Alternative: compute the residual of each constraint independently
  after solving the over-determined system. Constraints with large
  residuals are the conflicting ones. Is this reliable?
- Alternative: use SVD instead of QR. The left singular vectors
  corresponding to zero singular values span the constraint dependency
  space. How do you map these back to specific constraint indices?

### Q3: Structural vs geometric rank deficiency
The spec mentions two kinds:

**Structural**: detectable from the constraint graph topology alone.
Example: 3 distance constraints on 2 points (3 equations, 4 params,
but the 3 equations aren't independent — you can't have 3 independent
distances between 2 points in 2D).

**Geometric**: only detectable numerically. Example: 3 collinear points
with distance constraints — structurally fine, but the Jacobian is
rank-deficient at the collinear configuration.

- How do real solvers distinguish these?
- For user feedback, does it matter? (Probably yes — structural means
  "you added too many constraints", geometric means "your geometry
  is in a degenerate configuration".)

### Q4: Identifying free parameters and their directions
When DOF > 0, the null space of J tells you which parameter combinations
are free. But we want to report this as "point X can move in direction Y."

- The null space of J (from QR or SVD) gives basis vectors in parameter
  space. Each null vector has entries for every parameter.
- How do you interpret `[0, 0, 0.7, 0.7, 0, 0, ...]` where indices 2,3
  are point P's x,y? That means P can move diagonally.
- For simple cases (single point free in X), the null vector will have
  a 1 in the x-index and 0 elsewhere. Easy.
- For complex cases (two points constrained to move together), the null
  vector mixes their parameters. How do you simplify this for the user?
- `FreeAxis` enum from the spec: `X, Y, Both, Radial`. How do you
  classify a null vector into these categories?

### Q5: Redundant vs conflicting
When a system is over-constrained:
- **Redundant**: constraint is implied by others (e.g., adding
  Horizontal + Vertical + Angle(90°) — the angle is redundant).
  The system is still satisfiable. Rank < equations but solution exists.
- **Conflicting**: constraints are contradictory (e.g., Distance(P,Q)=5
  AND Distance(P,Q)=10). No solution exists.

How do you distinguish these?
- Solve the least-squares system. If residual ≈ 0, constraints are
  redundant (consistent). If residual >> 0, constraints conflict.
- Is this reliable? What if the solver just didn't converge?

### Q6: Practical examples from SolveSpace and FreeCAD
- What does SolveSpace report for over-constrained? Just "inconsistent"?
- What does FreeCAD's sketcher report? It shows red constraints —
  how does it identify which ones?
- Are there better approaches in the literature?

## Desired Output

1. Step-by-step algorithm for identifying conflicting constraints
   from a QR (or SVD) decomposition of the Jacobian
2. Step-by-step algorithm for identifying free parameters and their
   directions from the null space
3. How to distinguish redundant from conflicting
4. Recommended nalgebra API calls for each step
5. Expected output for 3-4 concrete examples:
   - Fully constrained rectangle (rank = num_equations, DOF = 0)
   - Under-constrained triangle (missing one distance)
   - Over-constrained with redundancy (extra parallel constraint)
   - Over-constrained with conflict (contradictory distances)

## References to Consult

- Bettig & Hoffmann (2011) "Geometric Constraint Solving in Parametric
  CAD" — they discuss constraint status classification
- Hoffmann, Lomonosov & Sitharam (2001) — decomposition-recombination
- SolveSpace system.cpp SolveRank(), FindWhichToRemoveToFixSystem()
- FreeCAD planegcs/GCS.cpp diagnose()
- Golub & Van Loan, "Matrix Computations" — rank-revealing QR chapter
