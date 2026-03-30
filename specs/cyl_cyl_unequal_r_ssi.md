# Cylinder–Cylinder Unequal-Radius SSI

Specification for analytical surface-surface intersection of two non-parallel
circular cylinders with different radii.

**Status**: Implementation spec
**References**: [#1] Patrikalakis Ch.5, [#25] Yang et al. (2023)
**Governance**: A15.1, A15.2 (analytical primacy)
**Parent**: `specs/ssi_solver_matrix.md` — Pair #5, sub-case "non-parallel, unequal-R"

---

## Goal

Extend the cylinder-cylinder SSI solver to handle non-parallel cylinders with
different radii, returning analytical degree-4 parametric intersection curves
instead of `KernelError::NotSupported`.

This removes one of three remaining `NotSupported` sub-cases for pair #5
(Cylinder–Cylinder), the highest-frequency incomplete SSI pair.

---

## Parameters

| Parameter | Type | Unit | Description |
|-----------|------|------|-------------|
| `cyl_a_origin` | `[f64; 3]` | meters | Point on cylinder A's axis |
| `cyl_a_axis` | `[f64; 3]` | unit vec | Cylinder A axis direction |
| `cyl_a_radius` | `f64` | meters | Cylinder A radius (> 0) |
| `cyl_b_origin` | `[f64; 3]` | meters | Point on cylinder B's axis |
| `cyl_b_axis` | `[f64; 3]` | unit vec | Cylinder B axis direction |
| `cyl_b_radius` | `f64` | meters | Cylinder B radius (> 0) |

**Constraints**:
- Both radii > TAU_NORMALIZE
- Axes are non-parallel: |cos(angle)| < 1 - TAU_PARALLEL
- Inter-axis angle ≥ 15° (|cos(α)| ≤ SSI_CYL_CYL_MIN_ANGLE_COS)
- Axes are non-skew: closest approach < SSI_SKEW_FACTOR × max(R_A, R_B)

---

## Branch Table

| Sub-case | Condition | Expected Output |
|----------|-----------|-----------------|
| B1: Full intersection, R_B ≥ R_A | R_B/R_A ≥ 1, surfaces overlap | Two closed `Degree4CylCyl` curves (+ and − branches) |
| B2: Partial intersection, R_B < R_A | R_B/R_A < 1, |sin θ| ≤ R_B/R_A restricts domain | Two `Degree4CylCyl` curves with restricted θ-range |
| B3: Tangent (R_B = R_A sin α at critical θ) | Discriminant touches zero | Single degenerate curve (or two touching curves) |
| B4: Disjoint | Surfaces don't overlap after transform | Empty vec |
| B5: Near-equal radii | |R_A − R_B|/max(R_A,R_B) < SSI_RADII_RELATIVE_TOL | Delegate to existing equal-R dual-ellipse solver |

---

## Analytical Method

### Canonical frame construction

Given two cylinders in general position:

1. Compute closest-approach point between axes (midpoint = center)
2. Build orthonormal frame {e1, e2, e3} where:
   - e1 = cylinder A axis direction
   - e2 = component of cylinder B axis perpendicular to e1 (normalized)
   - e3 = e1 × e2
3. In this frame, cylinder A is along e1, cylinder B is in the e1-e2 plane
   at angle α from e1

### Parametric curve formula

In the canonical frame, the intersection curves are parametrized by angle θ
on cylinder A:

```
x(θ) = R_A cos θ
y(θ) = R_A sin θ
z(θ) = (R_A cos θ cos α ± √(R_B² − R_A² sin²θ)) / sin α
```

**Domain**: θ ∈ [0, 2π) when R_B ≥ R_A; θ restricted to
arcs where R_B² − R_A² sin²θ ≥ 0 when R_B < R_A.

### Transform back

Multiply each (x, y, z) by the frame matrix [e3, e2_perp, e1]ᵀ⁻¹ and
add the center point to get world-space coordinates.

### Research Basis

- **[#1] Patrikalakis Ch.5**: Establishes that the intersection of two
  quadric surfaces is a degree-4 algebraic curve. Provides the implicit
  equation approach. Our parametric approach is equivalent but more
  convenient for evaluation.
- **[#25] Yang et al.**: Topology-guaranteed SSI. Confirms the degree-4
  nature and provides numerical validation strategies.

---

## Invariants

1. **On-surface**: Every point P on the returned curve satisfies both
   `dist(P, axis_A) = R_A ± TAU_MODEL` and `dist(P, axis_B) = R_B ± TAU_MODEL`
2. **Symmetry**: Swapping cylinder A and B produces equivalent curves
   (same point set, possibly different parametrization)
3. **Continuity**: The curve is C∞ smooth except possibly at domain endpoints
   (when R_B < R_A)
4. **Degeneration to equal-R**: When R_A ≈ R_B, the degree-4 curves should
   approximate the dual-ellipse solution from the equal-R solver

---

## Oracles

| Oracle | Method |
|--------|--------|
| On-surface A | For N sample points on curve, `|dist_to_axis_A − R_A| < TAU_MODEL` |
| On-surface B | For N sample points on curve, `|dist_to_axis_B − R_B| < TAU_MODEL` |
| Curve count | B1: 2 curves, B2: 2 curves, B3: 1-2 curves, B4: 0 curves |
| Equal-R consistency | For R_A ≈ R_B at 90°, compare with dual-ellipse result (point distance < TAU_MODEL) |

---

## Failure Modes

| Condition | Behavior |
|-----------|----------|
| Near-parallel (< 15°) | Return `KernelError::NotSupported` (existing guard) |
| Skew axes | Return `KernelError::NotSupported` (existing guard) |
| Zero radius | Return `KernelError::NotSupported` |
| sin α ≈ 0 | Caught by near-parallel guard |
| Discriminant < −TAU_WORK | No real intersection at that θ; restrict domain |

---

## SSICurve::Degree4CylCyl Variant

New enum variant storing the evaluation parameters:

```rust
SSICurve::Degree4CylCyl {
    /// Center point (midpoint of closest approach)
    center: [f64; 3],
    /// Frame: column vectors [e3, e2, e1] for local-to-world transform
    frame: [[f64; 3]; 3],
    /// Cylinder A radius
    r_a: f64,
    /// Cylinder B radius
    r_b: f64,
    /// Cosine of inter-axis angle
    cos_alpha: f64,
    /// Sine of inter-axis angle
    sin_alpha: f64,
    /// Sign: +1.0 or -1.0 for the two branches
    sign: f64,
    /// Valid θ range: (θ_min, θ_max). Full [0, 2π) when R_B ≥ R_A.
    theta_range: (f64, f64),
}
```

Evaluation: `evaluate(theta: f64) -> [f64; 3]` computes the world-space
point using the parametric formula and frame transform.

---

*Created: 2026-03-30*
