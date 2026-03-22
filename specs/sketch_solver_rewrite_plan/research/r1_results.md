# R1 Research Results: Analytic Jacobian Cookbook

**Source**: Gemini Deep Research
**Status**: Reviewed, actionable — ready for Fork A implementation
**Feeds into**: Fork A (constraint implementations)

---

## Summary

Complete analytic Jacobian derivations for all 7 constraint groups covering
20 of 21 `SketchConstraint` variants. Every nonlinear constraint includes
singularity analysis and numerical guard recommendations.

## Key Design Decisions Confirmed

### 1. Normalized point-line distance (mandatory)

The research definitively recommends normalized form:
`f = cross_product / ‖L‖ - d = 0`

Unnormalized form causes residuals to scale with line length, destroying
condition number when sketches contain both tiny and large lines. This
aligns with R2's Jacobian scaling strategy — normalization IS the
row-scaling for point-line constraints.

### 2. Symmetry via midpoint + perpendicular decomposition (Option A)

Two equations: (1) dot(P1P2, L_dir) = 0, (2) cross(L_dir, midpoint-L_start) = 0.
Division-free, avoids cubic gradient manifolds of direct reflection matrix.
Higher equation count is trivially handled by sparse solver.

### 3. atan2 for all angle work

Avoids acos domain restrictions and sign ambiguity. The atan2 derivative
`(X·∂Y/∂v - Y·∂X/∂v) / (X² + Y²)` simplifies to `1/D²` factors.

### 4. Arc-arc tangency needs explicit is_internal flag

Current `SketchConstraint::Tangent { line, curve }` only handles line-arc.
Arc-arc tangency (if needed) requires deciding internal vs external at
constraint creation time, not dynamically. The Jacobian has a sign flip
on `∂f/∂r₂` between external (-1.0) and internal (+1.0).

**Note**: Our current Tangent variant only takes `line` + `curve`, so
arc-arc tangency is not yet supported. This is fine for v1.

### 5. Singularity guard strategy: ε = 1e-12 denominator clamping

All distance-based singularities (coincident points, zero-length lines)
guarded by `D = max(D, 1e-12)`. When clamped:
- Derivatives evaluate to near-zero (constraint temporarily disconnected)
- LM's λ regularization provides alternative progress
- Once geometry separates, true gradient resumes

### 6. Finite-difference verification: h = 1e-5, tolerance = 1e-7

Central differences: `(f(x+h) - f(x-h)) / 2h` with `h = 1e-5`.
Every analytic derivative must agree within 1e-7 of finite-difference
approximation. This is a dev-time test, not a runtime check.

---

## Coverage Map: R1 → SketchConstraint Variants

| SketchConstraint | R1 Group | Equations | Nonlinear? | Singularities |
|-----------------|----------|-----------|------------|---------------|
| Coincident | 1 | 2 | No | None |
| Horizontal | 1 | 1 | No | None |
| Vertical | 1 | 1 | No | None |
| Parallel | 2 | 1 | Bilinear | Zero-length lines (rank-deficient) |
| Perpendicular | 2 | 1 | Bilinear | Zero-length lines (rank-deficient) |
| Tangent(line,arc) | 5 | 1 | Yes (÷D_L) | Zero-length line |
| Equal(line,line) | 2 | 1 | Yes (÷D) | Zero-length line |
| Equal(circle,circle) | — | 1 | No | None (r₁ - r₂ = 0) |
| Symmetric | 6 | 2 | Bilinear | None (division-free) |
| SymmetricH | 1 | 2 | No | None |
| SymmetricV | 1 | 2 | No | None |
| Midpoint | 1 | 2 | No | None |
| Distance(P,P) | 2 | 1 | Yes (÷D) | Coincident points |
| Distance(P,L) | 4 | 1 | Yes (÷D_L) | Zero-length line |
| Angle | 2 | 1 | Yes (÷D²) | Zero-length line |
| Radius | 1 | 1 | No | None |
| Diameter | 1 | 1 | No | None |
| OnEntity(P,Line) | 3 | 1 | Bilinear | None (cross product) |
| OnEntity(P,Circle) | 3 | 1 | Yes (÷D) | P at center |
| Dragged | 1 | 2 | No | None |
| EqualAngle | 7 | 1 | Yes (÷D²) | Zero-length line (×4) |
| Ratio | 7 | 1 | Yes (÷D) | Zero-length line (×2) |
| EqualPointToLine | 7 | 1 | Bilinear | None (cross products cancel ÷D_L) |
| **SameOrientation** | **NOT COVERED** | 1? | ? | ? |

### SameOrientation gap

`SameOrientation { entity_a, entity_b }` is not covered by R1. This
constrains two lines to point in the same direction (not just be parallel).
Formulation: `atan2(dy₁, dx₁) - atan2(dy₂, dx₂) = 0` or equivalently
the cross-product formulation from Parallel PLUS a dot-product sign check.

For v1: use `dx₁·dx₂ + dy₁·dy₂ > 0` (dot product positive) combined
with `dx₁·dy₂ - dy₁·dx₂ = 0` (parallel). This is 2 equations but the
sign constraint is tricky for gradient-based solvers. Alternative:
`atan2(dy₁,dx₁) - atan2(dy₂,dx₂) = 0` (1 equation, same singularities
as Angle constraint). Defer to implementation time.

---

## Derivative Reference by Group

### Group 1: Linear (constant Jacobian, no singularities)

All derivatives are ±1.0 or ±0.5. Implementation is trivial — hardcode
the sparse entries. No guards needed.

### Group 2: Nonlinear fundamentals

**Distance(P₁,P₂,d)**: `f = √(Δx²+Δy²) - d`
- ∂f/∂x₁ = -Δx/D, ∂f/∂y₁ = -Δy/D
- ∂f/∂x₂ = Δx/D, ∂f/∂y₂ = Δy/D
- Guard: D < ε → clamp to ε

**EqualLength(L₁,L₂)**: `f = D₁ - D₂`
- Same structure as Distance, applied to both lines
- Guard: D₁ or D₂ < ε → clamp independently

**Parallel(L₁,L₂)**: `f = Δx₁·Δy₂ - Δy₁·Δx₂`
- No division, bilinear in coordinates
- ∂f/∂x_{s1} = -Δy₂, etc. (cross product of direction partials)

**Perpendicular(L₁,L₂)**: `f = Δx₁·Δx₂ + Δy₁·Δy₂`
- No division, bilinear (dot product)

**Angle(L₁,L₂,θ)**: `f = atan2(Y,X) - θ` where Y=cross, X=dot
- ∂f/∂(line1_params) = ±Δy₁/D₁² or ±Δx₁/D₁²
- ∂f/∂(line2_params) = ±Δy₂/D₂² or ±Δx₂/D₂²
- Guard: D₁² or D₂² < ε → clamp

### Group 3: Point-on-entity

**PointOnLine**: Cross product form (division-free, no singularities)
**PointOnCircle**: Same as Distance but target = radius

### Group 4: Point-line distance (normalized)

Quotient-rule derivatives for line endpoints. Simpler ÷D_L for point.
Guard: D_L < ε → clamp.

### Group 5: Tangent

Line-arc: Point-line distance with d = ±r, plus ∂f/∂r = ±1.0
Arc-arc: Distance between centers ± sum/difference of radii

### Group 6: Symmetric (arbitrary line)

Two equations, both bilinear (dot product + cross product). No division.

### Group 7: Compound

**EqualAngle**: Two atan2 blocks with opposite signs. 16 params.
**Ratio**: EqualLength scaled by k.
**EqualPointToLine**: Cross products cancel denominator — division-free.

---

## Implementation Notes for Fork A Workers

1. **Start with Group 1** — trivial, builds scaffolding
2. **Group 2 next** — establishes the singularity guard pattern
3. **Group 4 (normalized P-L distance) is the hardest derivative** —
   quotient rule across 6 params. Test this exhaustively.
4. **Every constraint gets a finite-difference test** in the test suite
5. **The `Equal` variant dispatches by entity type** — line→EqualLength,
   circle→EqualRadius (trivial: `r₁ - r₂ = 0`)
6. **The `Distance` variant dispatches** — P-P distance vs P-L distance
   depending on entity_b type
7. **The `OnEntity` variant dispatches** — line→PointOnLine, circle→PointOnCircle
