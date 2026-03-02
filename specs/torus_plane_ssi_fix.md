# Torus-Plane SSI Fix: Newton Divergence Bypass

## Status: SPEC READY

## Problem Summary

5 tests are blocked by torus-plane boolean failures:
- **RB1** (`rb1_revolve_union_with_box`) — full 360° rect revolve + box union
- **RB2** (`rb2_revolve_subtract_from_box`) — full 360° circle revolve + box subtract
- **RB6** (`rb6_extrude_then_revolve_union`) — box first, then revolve union
- **RB8** (`rb8_revolve_intersect_with_box`) — full 360° circle revolve + box intersect
- **MO4** (`mo4_revolve_then_boolean`) — revolve rect + box union

All share the same failure mode: shell assembly produces 8+ open edges from
uncut torus face fragments.

## Root Cause Analysis

### Pipeline Trace

The IC pipeline for torus-plane face pairs follows this path:

```
intersection_curves()                          [mod.rs:216]
  ├── try_analytical_torus_plane_ic()          [analytical.rs:1950]  ✅ succeeds
  ├── generate_polylines()                     [analytical.rs:2076]  ✅ 64-pt ellipse sampling
  ├── clip_polylines_to_domain()               [mod.rs:376]          ✅ clips to face bounds
  ├── refine_polyline()                        [analytical.rs:2039]  ✅ projects onto ellipse
  └── IntersectionCurveWithParameters::try_new()  [mod.rs:31]        ❌ FAILS HERE
        └── search_triple(i, 100)              [intersection_curve.rs:142]
              └── double_projection(None, None, pt, dir, 100)  [intersection_curve.rs:4]
                    ├── search_nearest_parameter(pt, None, 100) on S0  ✅ (Plane always works)
                    ├── search_nearest_parameter(pt, None, 100) on S1  ✅ (Torus/RevCurve works)
                    └── Newton iteration (4-var joint system)          ❌ DIVERGES
```

### Why Newton Diverges

`double_projection()` (`truck-geometry/src/decorators/intersection_curve.rs:4-96`)
solves a 4-variable system:

- **Unknowns**: `(u0, v0, u1, v1)` — parameters on surface0 and surface1
- **Equations 1-3**: `S0(u0,v0) - S1(u1,v1) = 0` (surfaces coincide in 3D)
- **Equation 4**: `plane_normal · (midpoint - plane_point) = 0` (tangent plane constraint)

For torus surfaces, Newton diverges because:

1. **Ill-conditioned Jacobian**: The torus surface `S(u,v) = (R + r·cos(v))·cos(u), (R + r·cos(v))·sin(u), r·sin(v)` has derivatives involving products of trig functions. Near the inner rim (where `R + r·cos(v)` is small), the Jacobian becomes singular.

2. **Tangent plane constraint mismatch**: The `plane_normal` parameter is the polyline segment direction (`leader.der(t)`), which approximates the IC tangent. For highly curved ICs on torus surfaces, this approximation can be poor, pushing Newton off the true intersection.

3. **Multiple solutions**: A torus-plane intersection can have 1-2 curves. Newton can oscillate between them.

### The Irony: Points Are Already Correct

The analytically-generated polylines from `generate_polylines()` are **already on both surfaces**:
- `sample_ellipse()` generates points on the exact analytical torus-plane intersection
- `refine_polyline()` projects back onto the ellipse
- `clip_polylines_to_domain()` verifies each point against both surfaces

Yet `try_new()` **re-solves** for these same points using Newton, which diverges.

### Secondary Failure: `from_is_curve`

Even if `try_new()` were fixed, `ShapesOpStatus::from_is_curve()` at
`loops_store/mod.rs:61-77` calls `search_triple(t, 100)` at the IC midpoint
to determine And/Or classification. This goes through the same
`double_projection` → Newton → divergence path. So both callsites must
be fixed.

## Proposed Fix: Initial Residual Early-Return in `double_projection`

### Core Insight

After `search_nearest_parameter` computes initial `(u0,v0)` on surface0 and
`(u1,v1)` on surface1, evaluate both surfaces. If they already agree within
TOLERANCE, **return immediately without Newton iteration**.

### Code Change

**File**: `vendor/truck/truck-geometry/src/decorators/intersection_curve.rs`

**Function**: `double_projection()` (line 4-96)

```rust
fn double_projection<S0, S1>(
    surface0: &S0,
    hint0: Option<(f64, f64)>,
    surface1: &S1,
    hint1: Option<(f64, f64)>,
    plane_point: Point3,
    plane_normal: Vector3,
    trials: usize,
) -> Option<(Point3, Point2, Point2)>
where
    S0: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
    S1: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
{
    let (ix, iy) =
        hint0.or_else(|| surface0.search_nearest_parameter(plane_point, hint0, trials))?;
    let (iz, iw) =
        hint1.or_else(|| surface1.search_nearest_parameter(plane_point, hint1, trials))?;

    // ── NEW: Initial residual early-return ──────────────────────────
    // For analytically-generated polylines, the initial parameters from
    // search_nearest_parameter are already accurate. Skip Newton when
    // both surfaces agree at the initial parameters.
    let pt0_init = surface0.subs(ix, iy);
    let pt1_init = surface1.subs(iz, iw);
    if (pt0_init - pt1_init).magnitude() < TOLERANCE {
        let point = pt0_init.midpoint(pt1_init);
        return Some((point, Point2::new(ix, iy), Point2::new(iz, iw)));
    }
    // ── END NEW ─────────────────────────────────────────────────────

    // ... existing Newton iteration unchanged ...
}
```

### Why This Works

| Scenario | Initial residual | Behavior |
|---|---|---|
| Analytical polyline (points on both surfaces) | `< TOLERANCE` | Early return, Newton skipped |
| Mesh polyline (points between surfaces) | `> TOLERANCE` | Falls through to Newton as before |
| Degenerate/far-away point | `>> TOLERANCE` | Falls through to Newton as before |

**For analytical polylines**: Points from `generate_polylines()` are on the
exact intersection. `Plane.search_nearest_parameter` returns exact projection
(always works, no Newton). `Torus.search_nearest_parameter` uses analytical
atan2/asin (lines 154-187 of `torus.rs`). `RevolutedCurve.search_nearest_parameter`
uses projected curve + angle (lines 344-384 of `revolved_curve.rs`). Both compute
correct parameters for on-surface points.

**For mesh polylines**: Points are off-surface by mesh discretization error,
typically much larger than TOLERANCE. Newton iteration still runs. No regression.

### What This Fixes

1. **`try_new()` in `intersection_curve/mod.rs:31`** — each `search_triple` call goes through `double_projection`, now short-circuits for analytical points.

2. **`from_is_curve()` in `loops_store/mod.rs:61`** — midpoint `search_triple` call also benefits from the early return.

3. **`search_nearest_point()` in `intersection_curve.rs:157`** — uses a different Newton system but isn't in the critical path.

## Branch Table: Torus-Plane Cases

| Case | Surface pair | Analytical detection | `generate_polylines` | IC expected |
|---|---|---|---|---|
| Perpendicular plane through center | Torus + Plane | `detect_torus` ✅ | 2 circles | 2 closed ICs |
| Axial plane (through axis) | Torus + Plane | `detect_torus` ✅ | 2 circles | 2 closed ICs |
| Oblique plane | Torus + Plane | `detect_torus` ✅ | 1-2 ellipses | 1-2 ICs |
| Tangent plane | Torus + Plane | `detect_torus` ✅ | 1 degenerate | Edge case |
| RevSurf + Plane (cylinder-like) | RevCurve + Plane | `detect_revolution_axis` ✅ | Empty (revsurf path) | Via mesh |
| RevSurf + Plane (annular) | RevCurve + Plane | `detect_plane` on both? | N/A | Coplanar handling |

## Risk Assessment

### Low Risk
- **Single function change**: Only `double_projection` is modified.
- **Additive check**: The early return is a strict improvement — it short-circuits when we can prove the answer is already correct.
- **No regression for mesh path**: Mesh polylines have `> TOLERANCE` initial residual, so Newton still runs.
- **TOLERANCE threshold**: Using the existing `TOLERANCE` constant (same as what Newton convergence checks against).

### Medium Risk
- **Parameter accuracy**: The initial `search_nearest_parameter` might return slightly less accurate parameters than Newton would produce. For analytical polylines this is fine (parameters are already exact). But for mesh polylines near the threshold, this could accept slightly worse parameters. Mitigation: the threshold is `TOLERANCE` which is very small (~1e-6).

### Remaining Issues After This Fix
- **Shell assembly (8+ open edges)**: Even with all ICs found, torus face fragments may not assemble into a closed shell. The `force_merge_open_edges` at Level 3 handles 2-4 open edges; 8+ may need additional assembly work.
- **RB5** (cylinder-torus IC): Not addressed — different surface pair.
- **RevSurf faces (non-torus)**: Revolved rectangles create `RevolutedCurve<NurbsCurve>` faces that are geometrically cylinders/planes but typed as revolution surfaces. The analytical path uses `revsurf_plane` (no ellipses) → `generate_polylines` returns empty → falls to mesh. The early-return fix may not help if mesh extraction finds 0 triangles.

## Test Plan

### Un-ignore Tests (after verifying IC generation succeeds)
- [ ] RB1 — may still fail in assembly (8+ open edges)
- [ ] RB2 — may still fail in assembly
- [ ] RB6 — may still fail in assembly
- [ ] RB8 — may still fail in assembly
- [ ] MO4 — may still fail in assembly

**Strategy**: Un-ignore one at a time. If IC generation now succeeds but assembly
still fails, change the `#[ignore]` message from "IC generation" to "assembly"
and keep ignored. The IC fix is still valuable progress.

### New Unit Tests
1. **`test_double_projection_initial_residual_bypass`**: Construct a Torus + Plane pair with a known intersection point. Verify `double_projection` returns correct parameters without Newton.
2. **`test_try_new_analytical_torus_plane`**: Create an analytical torus-plane polyline, pass through `try_new`, verify it succeeds.
3. **`test_from_is_curve_torus_plane`**: After constructing an IC from analytical polyline, verify `from_is_curve` returns valid And/Or status.

### Regression Tests
- Run full `cargo test -p truck-shapeops` (306 existing tests)
- Run full `cargo test -p test-harness` (400+ tests)
- Specifically verify: K8, HP-1, HP-2, t1_euler, e3_boss, cs1, cs2, MV3 still pass

## Implementation Steps

1. Add initial residual early-return in `double_projection` (~5 lines)
2. Run `cargo test -p truck-shapeops` — verify no regressions
3. Run `cargo test -p test-harness` — verify no regressions
4. Temporarily un-ignore RB1, run it, observe if IC generation now succeeds
5. If IC succeeds but assembly fails, note the open edge count
6. Add unit tests for the new code path
7. Update ignore messages on tests that now fail in assembly (vs IC generation)
