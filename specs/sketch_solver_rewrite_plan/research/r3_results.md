# R3 Research Results: Rank Analysis and Constraint Diagnostics

**Source**: Gemini Deep Research
**Status**: Reviewed, actionable
**Feeds into**: Fork B (numerics), Wave 3 (integration)

---

## Key Decisions from Research

### 1. SVD over QR for diagnostics

Use `nalgebra::SVD` (not `ColPivQR`) for all rank analysis and constraint
diagnostics. QR is fine for the Newton-Raphson linear solve step, but
diagnostics require SVD for:
- Reliable rank determination in ill-conditioned systems
- Clean null space extraction for both free parameters and dependent constraints
- Guaranteed accuracy at our matrix sizes (≤200)

Performance: SVD on 200×200 matrix < few milliseconds. Well within 16ms
interactive budget.

### 2. Redundant vs conflicting classification algorithm

Given an over-constrained system (rank(J) < m):

1. Compute SVD of J^T: `J^T = U Σ V^T`
2. rank = number of singular values > ε
3. For each null vector `v_k = v_t.row(i)` where `i >= rank`:
   - Compute projection: `r_k = v_k · F(x*)`
   - If `|r_k| ≤ TAU_MODEL` → **redundant** (constraints agree, deactivate one)
   - If `|r_k| > TAU_MODEL` → **conflicting** (constraints contradict)
4. Conflicting constraint IDs = indices where `|v_k[j]| > τ_kin`

### 3. Two separate thresholds

- **Rank threshold** (numerical): `ε = max(m,n) · ε_mach · σ₁` (relative to
  largest singular value). Used for SVD rank determination.
- **Conflict threshold** (geometric): `TAU_MODEL = 1e-7` (from spec A14).
  Used for redundant/conflict classification via residual projection.

These serve different purposes. Do not conflate them.

### 4. FreeAxis classification (simplified for v1)

Given right null space vectors of J (columns of V from SVD of J where σ < ε):

For each point `i` with param indices `(2i, 2i+1)`:
- Extract `dx = v[2i]`, `dy = v[2i+1]` from each null vector
- `|dx| > τ_kin AND |dy| ≤ τ_kin` → `FreeAxis::X`
- `|dx| ≤ τ_kin AND |dy| > τ_kin` → `FreeAxis::Y`
- Both significant in same or spanning null vectors → `FreeAxis::Both`
- Default to `Both` if unclear

`τ_kin = 0.1` (threshold for "significant" component in normalized null vector)

Radial detection (pivot-finding) deferred to Phase 3 Enhanced Diagnostics.

### 5. Equation-to-constraint ID mapping

Critical bookkeeping: some constraints produce multiple equations:
- Coincident → 2 equations
- Midpoint → 2 equations
- Symmetric → 2 equations
- SymmetricH/V → 2 equations
- Dragged → 2 equations
- All others → 1 equation

Maintain `eq_to_constraint: Vec<usize>` mapping each equation row index to
its parent constraint index. When SVD identifies dependent equations via
null vector components, aggregate back to constraint IDs before reporting.

---

## nalgebra API Reference

```rust
use nalgebra::DMatrix;

// Build Jacobian (m equations × n params)
let j: DMatrix<f64> = build_jacobian(&params, &constraints, m, n);

// --- For Newton-Raphson linear solve step: use QR ---
let qr = j.clone().qr();
let delta = qr.solve(&neg_residual).unwrap_or_else(|| {
    // Least-squares fallback for rectangular systems
    let jt = j.transpose();
    let jtj = &jt * &j;
    jtj.qr().solve(&(&jt * &neg_residual)).unwrap()
});

// --- For diagnostics: use SVD on J^T ---
let jt = j.transpose();
let svd = jt.svd(true, true);

// Rank determination (relative threshold)
let sigma_max = svd.singular_values[0];
let eps = (m.max(n) as f64) * f64::EPSILON * sigma_max;
let rank = svd.singular_values.iter().filter(|s| **s > eps).count();

// Degrees of freedom
let dof = n - rank;

// Dependent constraints (right null space of J^T = rows of V^T for σ < ε)
let vt = svd.v_t.as_ref().unwrap();
let residual: DVector<f64> = compute_residual(&params, &constraints);

for i in rank..m {
    let null_vec = vt.row(i);
    let projection: f64 = null_vec.dot(&residual);

    if projection.abs() <= TAU_MODEL {
        // Redundant — constraints are consistent but dependent
        let involved: Vec<usize> = (0..m)
            .filter(|&j| null_vec[j].abs() > 0.01)
            .map(|eq_idx| eq_to_constraint[eq_idx])
            .collect();
        // Report as redundant, suggest removing one
    } else {
        // Conflicting — constraints contradict
        let conflicting: Vec<usize> = (0..m)
            .filter(|&j| null_vec[j].abs() > 0.01)
            .map(|eq_idx| eq_to_constraint[eq_idx])
            .collect();
        // Report as conflicting with magnitude = projection.abs()
    }
}

// Free parameter directions (right null space of J = columns of U from J^T SVD)
// Or equivalently: SVD of J directly, last columns of V
let j_svd = j.svd(true, true);
let v = j_svd.v_t.as_ref().unwrap();
for i in rank..n {
    let null_vec = v.row(i); // free direction in param space
    // Classify per-point FreeAxis from components
}
```

---

## Test Oracle Data

### Oracle 1: Fully constrained rectangle
- 4 points (8 params), 8 constraint equations
- rank(J) = 8, DOF = 0
- All singular values > ε
- No null vectors, no FreeAxis classifications
- Status: `FullyConstrained`

### Oracle 2: Under-constrained triangle
- 3 points (6 params), 5 constraints (fix p1, fix p2.y, dist p1-p3)
- rank(J) = 5, DOF = 1
- 1 null vector in param space → p3 can rotate around p1
- FreeAxis for p3: `Radial` (or `Both` in simplified v1)
- Status: `UnderConstrained { dof: 1 }`

### Oracle 3: Redundant over-constraint
- 3 points (6 params), 7 constraints (triangle + angle implied by Pythagorean)
- rank(J) = 6, 1 dependent constraint combination
- Null vector in constraint space: coefficients on dist(p2,p3) and angle constraint
- Residual projection ≈ 0 → **redundant**
- Status: `OverConstrained { conflicts: [] }` (or custom "redundant" status)

### Oracle 4: Conflicting over-constraint
- 3 points (6 params), 7 constraints (dist p2-p3 = 6 AND dist p2-p3 = 12)
- rank(J) = 6, 1 dependent constraint combination
- Null vector isolates constraints 4 and 5
- Residual projection = 4.24 >> TAU_MODEL → **conflicting**
- Status: `OverConstrained { conflicts: [4, 5] }`
