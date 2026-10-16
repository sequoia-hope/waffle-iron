//! KV14 Slice G — the Stage-1 chart chord contract (spec
//! `yang_stage1_curved_holed_patch` §"Slice G"; Yang §4.1: "triangulate the
//! rectangular u-v domain until reaching the given distance tolerance d_ε").
//!
//! Stage 3 / Stage 4 read the operand's chord budget back as an upper bound
//! on how far a mesh point sits from its analytic surface. For a flat
//! triangle on a cylinder of radius `r` the worst radial deficit is at the
//! midpoint of its widest-azimuth edge, `r·(1 − cos(Δθ/2))`. The canonical
//! tube meets the budget by construction (every edge spans one rim step);
//! a boundary-only chart CDT does not (R0026: 39°–41° diagonals against a
//! 32.7° rim step, 1.50 × d_ε). These helpers derive the domain-grid step
//! from the budget and census the emitted edges against it.

/// Wrapped azimuth difference `|θ_b − θ_a|` in `[0, π]`.
pub(crate) fn azimuth_span(a: f64, b: f64) -> f64 {
    let two_pi = 2.0 * std::f64::consts::PI;
    let d = (b - a).rem_euclid(two_pi);
    d.min(two_pi - d)
}

/// Radial deficit at the midpoint of a flat chord spanning `dtheta` of
/// azimuth on a cylinder of radius `radius`.
pub(crate) fn cylinder_chord_sag(radius: f64, dtheta: f64) -> f64 {
    radius * (1.0 - (0.5 * dtheta).cos())
}

/// What a cylinder chart's domain grid is derived from.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ChartBudget {
    /// The operand's chord budget as Stage 3/4 read it back
    /// (`curved_chord_bound`, else `ellipse_rim_chord_bound`).
    pub(crate) budget: Option<f64>,
    /// A per-face demand from the self-contact guard (`face_chord_demands`):
    /// a skin thinner than the surface's own chord sag. Never looser.
    pub(crate) demand: Option<f64>,
    /// The global rim step `2π / n_seg` the operand's rims were sampled at —
    /// a CAP on the grid step so the domain grid is phase-matched to the
    /// rims (it never tightens the bound: a short arc edge's chain step is
    /// its whole sweep, not the rim density — the R0026 first-cut trap).
    pub(crate) rim_step: Option<f64>,
}

/// The domain-grid azimuth step for a cylinder chart and the sagitta bound it
/// enforces: `sag_bound = min(budget, demand)` and `Δθ_seed = 2·acos(1 −
/// sag_bound / r)`, capped at the rim step and at the tube's N = 3 floor
/// (2π/3). The rim step, derived from `max_r`, is never coarser than the
/// budget's own step at `r ≤ max_r`, so a budget-bounded face gets a grid
/// phase-matched to its rims. `None` when no budget or demand is known —
/// the caller stays boundary-only rather than invent a constant.
pub(crate) fn cylinder_seed_step(radius: f64, b: ChartBudget) -> Option<(f64, f64)> {
    if !(radius.is_finite() && radius > 0.0) {
        return None;
    }
    let floor_step = 2.0 * std::f64::consts::PI / 3.0;
    let pos = |x: Option<f64>| x.filter(|v| v.is_finite() && *v > 0.0);
    let sag_bound = match (pos(b.budget), pos(b.demand)) {
        (Some(x), Some(y)) => x.min(y),
        (Some(x), None) | (None, Some(x)) => x,
        (None, None) => return None,
    };
    let c = 1.0 - sag_bound / radius;
    let mut step = if c <= -1.0 {
        floor_step
    } else {
        (2.0 * c.clamp(-1.0, 1.0).acos()).min(floor_step)
    };
    if let Some(r) = pos(b.rim_step) {
        step = step.min(r);
    }
    if !(step.is_finite() && step > 0.0) {
        return None;
    }
    Some((step, sag_bound))
}

/// Census of a cylinder chart triangulation's edges against a sagitta bound.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ChartSagCensus {
    pub(crate) n_interior: usize,
    pub(crate) n_boundary: usize,
    /// Interior (CDT-chosen) edges whose chord sagitta exceeds the bound.
    pub(crate) interior_violations: usize,
    /// Worst interior `sag / bound` (0 when there is no interior edge).
    pub(crate) interior_max_ratio: f64,
    /// Worst boundary-chord `sag / bound` (measured, never enforced here).
    pub(crate) boundary_max_ratio: f64,
}

/// Walk every undirected edge of `tris` once; `theta_of` gives a vertex's
/// azimuth about the cylinder axis, `is_boundary_chord` says whether the edge
/// is a boundary chord (governed by its own sampling contract) rather than an
/// interior diagonal the CDT chose. Violation = `sag > bound·(1 + 1e-9)`.
pub(crate) fn chart_sag_census(
    tris: &[[u32; 3]],
    radius: f64,
    bound: f64,
    theta_of: impl Fn(u32) -> f64,
    is_boundary_chord: impl Fn(u32, u32) -> bool,
) -> ChartSagCensus {
    let mut seen: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
    let mut c = ChartSagCensus::default();
    let tol = bound * (1.0 + 1e-9);
    for t in tris {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let key = (a.min(b), a.max(b));
            if !seen.insert(key) {
                continue;
            }
            let sag = cylinder_chord_sag(radius, azimuth_span(theta_of(a), theta_of(b)));
            let ratio = if bound > 0.0 {
                sag / bound
            } else {
                f64::INFINITY
            };
            if is_boundary_chord(a, b) {
                c.n_boundary += 1;
                c.boundary_max_ratio = c.boundary_max_ratio.max(ratio);
            } else {
                c.n_interior += 1;
                c.interior_max_ratio = c.interior_max_ratio.max(ratio);
                if sag > tol {
                    c.interior_violations += 1;
                }
            }
        }
    }
    c
}

thread_local! {
    /// Test-only override of the chart seed — per THREAD, so parallel unit
    /// tests never interfere: `Some(false)` forces the boundary-only CDT (the
    /// pre-Slice-G path), `Some(true)` or `None` (production) the seeded path.
    /// (The `YANG_S1_CHART_SEED` env gate was deleted 2026-10-16.)
    pub(crate) static CHART_SEED_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}
