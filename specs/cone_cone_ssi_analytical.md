# Spec: Analytical Cone-Cone SSI Solver (Degree4ConeCone)

## Goal

Replace the sampling-based stubs in `cone_cone_ssi` (same-apex and general
position sub-cases) with an exact analytical solver that returns parametric
`Degree4ConeCone` intersection curves. This eliminates A15.1 violations for
SSI pair #9.

## Parameters

| Parameter | Type | Units | Valid range | Description |
|-----------|------|-------|-------------|-------------|
| apex_a | [f64; 3] | meters | any | Apex point of cone A |
| axis_a | [f64; 3] | unit | ‖·‖ = 1 | Axis direction of cone A |
| half_angle_a | f64 | radians | (0, π/2) | Half-angle of cone A |
| height_range_a | (f64, f64) | meters | 0 ≤ min < max | Valid height range on cone A |
| apex_b | [f64; 3] | meters | any | Apex point of cone B |
| axis_b | [f64; 3] | unit | ‖·‖ = 1 | Axis direction of cone B |
| half_angle_b | f64 | radians | (0, π/2) | Half-angle of cone B |
| height_range_b | (f64, f64) | meters | 0 ≤ min < max | Valid height range on cone B |

## Branch Table

| # | Sub-case | Condition | Method | Output |
|---|----------|-----------|--------|--------|
| 1 | Coaxial, different angles | axes collinear, !same_apex | analytical | 0–1 Circle (existing, unchanged) |
| 2 | Coaxial, same angle | axes collinear, tan_a ≈ tan_b | analytical | Empty (existing, unchanged) |
| 3 | Same apex, different axes | apex_dist < TOL | **analytical (NEW)** | 0–N Degree4ConeCone curves |
| 4 | Same apex, same axis | apex_dist < TOL, axes_collinear | analytical | Empty (degenerate) |
| 5 | General position, intersecting | bounding sphere overlap | **analytical (NEW)** | 0–N Degree4ConeCone curves |
| 6 | General position, disjoint | no bounding sphere overlap | analytical | Empty (existing, unchanged) |
| 7 | Near-tangent | discriminant ≈ 0 at a single θ | analytical | Empty (filtered by MIN_FEATURE_SIZE) |

## Algorithm

### Core technique: Cone-A θ-parameterization + quadratic h solve

Parametrize cone A by (h, θ):
```
P(h,θ) = apex_a + h·axis_a + h·tan(α_a)·(cosθ·u_a + sinθ·v_a)
```
where (u_a, v_a) are an orthonormal basis perpendicular to axis_a.

Substitute into cone B's implicit equation:
```
|P - apex_b|² · cos²(β_b) = ((P - apex_b) · axis_b)²
```

This yields a quadratic in h for each θ:
```
a(θ)·h² + b(θ)·h + c = 0
```

**Precomputed constants** (stored in Degree4ConeCone variant):
- A = apex_a − apex_b
- p = axis_a · axis_b
- q_u = u_a · axis_b, q_v = v_a · axis_b
- m_a = A · axis_a, m_u = A · u_a, m_v = A · v_a, m_b = A · axis_b
- cos²β = cos²(half_angle_b)
- sec²α = 1/cos²(half_angle_a) = 1 + tan²(half_angle_a)
- c_const = |A|²·cos²β − m_b² (constant term, independent of θ)

**Quadratic coefficients**:
```
D_axis_b(θ) = p + tan_a·(cosθ·q_u + sinθ·q_v)
A_dot_D(θ)  = m_a + tan_a·(cosθ·m_u + sinθ·m_v)

a(θ) = sec²α · cos²β − D_axis_b(θ)²
b(θ) = 2·cos²β · A_dot_D(θ) − 2·m_b · D_axis_b(θ)
c    = c_const
```

Solutions: h(θ) = (−b(θ) ± √(b²−4ac)) / (2a(θ))

**Validity filter**:
- h > TOL (above apex of cone A)
- h ∈ height_range_a
- h_b = (P(h,θ) − apex_b) · axis_b > TOL and h_b ∈ height_range_b

**Theta range**: Determined by finding where the discriminant b²−4ac crosses
zero. Scan [0, 2π) in N steps, then refine zero-crossings with bisection.
The valid θ range is where discriminant ≥ 0.

### Same-apex specialization

When apex_a ≈ apex_b (same apex), A ≈ 0, so:
- c ≈ 0 (one root is always h ≈ 0, the apex itself)
- The non-trivial root is h = −b(θ)/a(θ) (linear)
- This naturally produces curves passing through or near the shared apex

The same Degree4ConeCone formulation handles this case without a separate code path.

## Invariants

1. **On-surface oracle**: Every sampled point on a Degree4ConeCone curve must
   lie on both cone surfaces within TAU_MODEL:
   - |perp_dist_from_axis_a − h_a·tan(α_a)| < TAU_MODEL
   - |perp_dist_from_axis_b − h_b·tan(β_b)| < TAU_MODEL

2. **Height validity**: h_a > 0 and h_b > 0 for all curve points (both cones
   only exist above their apices).

3. **No sampling artifacts**: No `SSI_SAMPLE_ON_SURFACE_TOL` usage in the
   cone-cone code path. No grid scanning loops.

4. **Coaxial preservation**: Existing coaxial circle results unchanged.

5. **Disjoint preservation**: Bounding-sphere reject still works.

6. **Curve type**: Same-apex and general position cases return Degree4ConeCone
   (not Line).

## Oracles

| Oracle | Check |
|--------|-------|
| On-surface A | \|perp_dist − h·tan(α)\| < TAU_MODEL for 32 sampled points |
| On-surface B | \|perp_dist − h_b·tan(β)\| < TAU_MODEL for 32 sampled points |
| No NaN | All coordinates finite |
| Height positive | h_a > 0 and h_b > 0 |
| Curve non-empty | General overlapping cases produce ≥ 1 curve |
| Disjoint empty | Far-apart cones produce 0 curves |

## Failure Modes

| Condition | Expected behavior |
|-----------|-------------------|
| tan(half_angle) ≈ 0 | Return empty (degenerate line-cone) |
| height_range invalid | Return empty |
| Bounding spheres disjoint | Return empty |
| Discriminant always negative | Return empty (no intersection) |
| Near-tangent (thin extent) | Return empty (filtered by MIN_FEATURE_SIZE) |

## Research Basis

- [#1] Patrikalakis et al. Ch.5 — SSI algorithms for quadric surface pairs.
  The θ-parameterization-into-implicit technique is the standard approach for
  degree-4 algebraic intersection curves between quadrics.
- [#25] Yang et al. (2023) — Topology-guaranteed SSI via Dixon resultant.
  Confirms degree-4 algebraic curve for cone-cone intersection.
- Same technique as existing Degree4CylCone solver (cylinder θ → cone implicit),
  here adapted as cone θ → cone implicit.
