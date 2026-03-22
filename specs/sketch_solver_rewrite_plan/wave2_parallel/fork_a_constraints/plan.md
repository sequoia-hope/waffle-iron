# Wave 2 / Fork A: Constraint Implementations

**Executor**: Claude fork (worktree), with Gemini workers
**Depends on**: Wave 1 (ParamLayout, ConstraintEq trait, ConstraintImpl enum)
**Parallel with**: Fork B (numerics), Fork C (render)
**Estimated scope**: ~900 lines

## Goal

Implement `ConstraintEq` for all `ConstraintImpl` variants. Each variant
needs `residuals()` and `jacobian()` — the equations from the spec's constraint
table, translated to code with typed index wrappers and nalgebra geometry.

**Primary reference**: `research/r1_results.md` — full analytic Jacobian cookbook.

## Key Implementation Rules (from R1)

1. **nalgebra geometry**: Use `PointIdx::read()` → `Point2<f64>`,
   `LineIdx::delta()` → `Vector2<f64>`. Write math naturally:
   `na::distance(&p1, &p2)`, `d1.x * d2.y - d1.y * d2.x`, etc.
2. **Normalized point-line distance** — always divide cross product by ‖L‖.
   Unnormalized form destroys condition number.
3. **atan2 for all angle work** — never use acos. atan2 handles all quadrants.
4. **Symmetry(arbitrary line) via midpoint + perpendicular decomposition** —
   2 equations, division-free, avoids cubic gradient manifolds.
5. **Singularity guard**: `D = D.max(1e-12)` for all distance denominators.
6. **Finite-difference verification**: `h = 1e-5`, tolerance `1e-7`.
   Central differences: `(f(x+h) - f(x-h)) / 2h`.

## Constraint Breakdown by Worker

### Worker A1: Linear constraints (Group 1) — 11 variants

All have constant Jacobians (±1.0 or ±0.5). No singularities.
Typed indices make implementation clean:

```rust
// Example: Coincident
ConstraintImpl::Coincident { p1, p2 } => {
    let a = p1.read(params);
    let b = p2.read(params);
    out[0] = a.x - b.x;
    out[1] = a.y - b.y;
}
```

| Variant | Equations | Scale types |
|---------|-----------|-------------|
| Coincident | 2 | [Dist, Dist] |
| Horizontal | 1 | [Dist] |
| Vertical | 1 | [Dist] |
| SymmetricH | 2 | [Dist, Dist] |
| SymmetricV | 2 | [Dist, Dist] |
| Midpoint | 2 | [Dist, Dist] |
| Dragged | 2 | [Dist, Dist] |
| Radius | 1 | [Dist] |
| Diameter | 1 | [Dist] |
| HDistance | 1 | [Dist] |
| VDistance | 1 | [Dist] |

### Worker A2: Nonlinear fundamentals (Group 2) — 5 variants

Establish the singularity guard pattern. Using nalgebra:

```rust
// Example: DistancePP
ConstraintImpl::DistancePP { p1, p2, d } => {
    let dist = na::distance(&p1.read(params), &p2.read(params));
    out[0] = dist - d;
}

// Jacobian: chain rule through distance
let delta = p2.read(params) - p1.read(params);
let dist = delta.norm().max(1e-12);  // singularity guard
let grad = delta / dist;  // unit vector
// ∂f/∂p1 = -grad, ∂f/∂p2 = +grad
```

| Variant | Equation | Singularity | Guard |
|---------|----------|-------------|-------|
| DistancePP | √(Δx²+Δy²) - d | D=0 | `.max(1e-12)` |
| EqualLength | D₁ - D₂ | D₁=0 or D₂=0 | clamp independently |
| Parallel | cross(d1,d2) | None (bilinear) | — |
| Perpendicular | dot(d1,d2) | None (bilinear) | — |
| Angle | atan2(cross,dot) - θ | D₁²=0 or D₂²=0 | clamp D² |

**Angle derivative** (from R1): denominator `X²+Y²` = `D₁²·D₂²`.
Partial derivatives are `±coord / D²` — elegant with nalgebra.

### Worker A3: Distance/dimensional + ratio (Groups 4+7) — 4 variants

| Variant | Equation | Notes |
|---------|----------|-------|
| DistancePL | cross/‖L‖ - d | **Normalized** — quotient rule, 6 params |
| HDistance | x₂-x₁-d | Linear |
| VDistance | y₂-y₁-d | Linear |
| Ratio | D₁ - k·D₂ | EqualLength scaled by k |

**DistancePL is the hardest derivative** — quotient rule across all line
endpoint params. Test exhaustively against finite differences.

### Worker A4: Point-on-entity + equal radius (Group 3) — 3 variants

| Variant | Equation | Notes |
|---------|----------|-------|
| OnLine | cross product = 0 | **Unnormalized** (d=0 so ‖L‖ cancels) |
| OnCircle | ‖P-center‖ - r = 0 | Singularity at P=center |
| EqualRadius | r₁ - r₂ = 0 | Linear, trivial |

### Worker A5: Tangent + compound + symmetric (Groups 5+6+7) — 6 variants

| Variant | Equation | Notes |
|---------|----------|-------|
| TangentLineCircle | norm_dist(line,center) ± r | Normalized P-L distance to arc center |
| TangentArcArc | dist(c1,c2) - (r1±r2) | external: r1+r2; internal: \|r1-r2\| per `internal` flag |
| SymmetricLine | dot(P1P2,L)=0, cross(L,mid)=0 | 2 eqs, **mixed scale**: [Angle, Distance] |
| EqualAngle | atan2₁₂ - atan2₃₄ | 4 lines, 16 params, mirrored Angle blocks |
| EqualPointToLine | Ux(yp1-yp2)-Uy(xp1-xp2) | Division-free (denominators cancel) |
| SameOrientation | no-op | Returns empty residuals, no Jacobian entries |

**TangentArcArc** (from R1 Group 5):
```rust
// External: f = dist(c1, c2) - (r1 + r2) = 0
// Internal: f = dist(c1, c2) - |r1 - r2| = 0  (with r1 > r2 assumed at build time)
// Spatial derivatives: same as DistancePP on centers
// Radius derivatives: ∂f/∂r1 = -1, ∂f/∂r2 = -1 (external)
//                     ∂f/∂r1 = -1, ∂f/∂r2 = +1 (internal, assuming r1 > r2)
// Singularity: D_c = 0 (concentric) — same ε clamp
```

**SymmetricLine** mixed scale types — perpendicularity equation is Angle-type
(D_row = L_c), midpoint-on-line equation is Distance-type (D_row = 1.0).
The `scale_types()` method returns `&[ScaleType::Angle, ScaleType::Distance]`.

### Worker A6: Unit tests + finite-difference verification

**Finite-difference test harness:**
```rust
fn verify_jacobian(c: &ConstraintImpl, params: &[f64], h: f64, tol: f64) {
    let n = params.len();
    let m = c.num_equations();
    let mut f_plus = vec![0.0; m];
    let mut f_minus = vec![0.0; m];
    let mut analytic_entries = Vec::new();
    c.jacobian(params, 0, &mut analytic_entries);

    for j in 0..n {
        let mut x_plus = params.to_vec();
        let mut x_minus = params.to_vec();
        x_plus[j] += h;
        x_minus[j] -= h;
        c.residuals(&x_plus, &mut f_plus);
        c.residuals(&x_minus, &mut f_minus);
        for i in 0..m {
            let fd = (f_plus[i] - f_minus[i]) / (2.0 * h);
            let analytic = analytic_entries.iter()
                .find(|(r, c, _)| *r == i && *c == j)
                .map(|(_, _, v)| *v)
                .unwrap_or(0.0);
            assert!((analytic - fd).abs() < tol,
                "Jacobian mismatch: [{i},{j}] analytic={analytic}, fd={fd}");
        }
    }
}
```

**Test matrix:**
- Every variant tested at 3+ random configurations
- Every nonlinear variant tested at singularity boundary (verify no NaN/Inf)
- Satisfied configurations verify residuals ≈ 0
- Perturbed configurations verify residual sign/magnitude
- All FD checks pass within 1e-7

## Deliverables

- `core/constraint.rs`: `impl ConstraintEq for ConstraintImpl` — all variants complete
- `tests/constraint_tests.rs`: unit tests for each constraint type
- `tests/jacobian_tests.rs`: finite-difference verification for all nonlinear variants

## Verification

- `cargo test -p sketch-solver -- constraint` — all unit tests pass
- `cargo test -p sketch-solver -- jacobian` — all FD checks pass within 1e-7
- No NaN/Inf from singularity guard tests
- TangentArcArc tested for both internal and external cases
