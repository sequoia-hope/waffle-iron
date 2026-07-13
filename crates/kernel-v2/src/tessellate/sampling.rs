//! Analytic edge sampling — interior sample points for arc / ellipse /
//! hyperbola / surface-pair half-edges at the chord-bound resolution,
//! twin-symmetric. Move-only split from the tessellate god-module
//! (design review 2026-07-12 F9); byte-identical.

use super::*;

/// Interior sample points of an arc half-edge at the chord-bound angular
/// resolution (endpoints excluded), IN THE HALF-EDGE'S WALK DIRECTION.
///
/// Bitwise twin-symmetric: the samples are computed on the CANONICAL
/// (lower-id) half-edge of the twin pair and reversed for the other side,
/// so the two faces sharing the arc emit identical sample positions —
/// load-bearing for cross-face watertightness (a planar annulus face and
/// the cylinder patch share their intersection-circle arcs).
pub(crate) fn arc_interior_samples(
    arena: &BrepArena,
    h: crate::arena::HalfEdgeId,
    n_seg: u32,
) -> Result<Vec<Point3>, KernelV2Error> {
    let he = arena.half_edge(h)?;
    if !matches!(he.curve, Curve::Arc { .. }) {
        return Ok(Vec::new());
    }
    let canon = h.min(he.twin);
    let che = arena.half_edge(canon)?;
    let Curve::Arc {
        center,
        normal,
        radius,
    } = che.curve
    else {
        // Twin curve consistency is a validated invariant.
        return Err(KernelV2Error::CurveTwinMismatch { half_edge: canon });
    };
    let fid = arena.loop_(che.loop_id)?.face;
    let start = arena.vertex(che.origin)?.point;
    let end = arena.vertex(arena.half_edge(che.next)?.origin)?.point;
    let n_arr = [normal.x, normal.y, normal.z];
    let Some(sweep) = crate::geom::ccw_sweep(center, n_arr, start, end) else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "degenerate arc (endpoint has no radial direction)",
        });
    };
    // e1 anchored at the canonical start so sample 0 continues from it.
    let Some((e1, e2)) = circle_frame(center, normal, start) else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "degenerate arc frame (start not radial)",
        });
    };
    let step = 2.0 * std::f64::consts::PI / f64::from(n_seg);
    let k = (sweep / step).ceil().max(1.0) as u32;
    let mut samples = Vec::with_capacity(k as usize - 1);
    for j in 1..k {
        let theta = sweep * f64::from(j) / f64::from(k);
        let (s, c) = theta.sin_cos();
        samples.push(Point3::new(
            center.x() + radius * (c * e1[0] + s * e2[0]),
            center.y() + radius * (c * e1[1] + s * e2[1]),
            center.z() + radius * (c * e1[2] + s * e2[2]),
        ));
    }
    if h != canon {
        samples.reverse();
    }
    Ok(samples)
}

/// Interior sample points of an ELLIPSE-arc half-edge (PR-KV9), endpoints
/// excluded, in the half-edge's walk direction. Twin-canonical exactly like
/// [`arc_interior_samples`]: computed on the lower-id half-edge of the twin
/// pair and reversed for the other side, so both incident faces emit
/// identical positions. The parametric step is the SAME angular step the
/// circle sampling uses (`2π/n_seg`): for a cylinder-section ellipse the
/// parameter equals the cylinder azimuth, so per-chord surface deviation
/// matches the lateral's own chord bound (shared contract, no new
/// tolerance).
pub(crate) fn ellipse_interior_samples(
    arena: &BrepArena,
    h: crate::arena::HalfEdgeId,
    n_seg: u32,
) -> Result<Vec<Point3>, KernelV2Error> {
    let he = arena.half_edge(h)?;
    if !matches!(he.curve, Curve::EllipseArc { .. }) {
        return Ok(Vec::new());
    }
    let canon = h.min(he.twin);
    let che = arena.half_edge(canon)?;
    let Curve::EllipseArc {
        center,
        normal,
        major_axis,
        major_radius,
        minor_radius,
    } = che.curve
    else {
        return Err(KernelV2Error::CurveTwinMismatch { half_edge: canon });
    };
    let fid = arena.loop_(che.loop_id)?.face;
    let start = arena.vertex(che.origin)?.point;
    let end = arena.vertex(arena.half_edge(che.next)?.origin)?.point;
    let nu = [normal.x, normal.y, normal.z];
    let mr = [major_axis.x, major_axis.y, major_axis.z];
    let (Some(t0), Some(sweep)) = (
        crate::geom::ellipse_param(center, nu, mr, major_radius, minor_radius, start),
        crate::geom::ellipse_ccw_sweep(center, nu, mr, major_radius, minor_radius, start, end),
    ) else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "degenerate ellipse arc (endpoint projects to the center)",
        });
    };
    let step = 2.0 * std::f64::consts::PI / f64::from(n_seg);
    let k = (sweep / step).ceil().max(1.0) as u32;
    let mut samples = Vec::with_capacity(k as usize - 1);
    for j in 1..k {
        let t = t0 + sweep * f64::from(j) / f64::from(k);
        samples.push(crate::geom::ellipse_point_at(
            center,
            nu,
            mr,
            major_radius,
            minor_radius,
            t,
        ));
    }
    if h != canon {
        samples.reverse();
    }
    Ok(samples)
}

/// Interior sample points of a HYPERBOLA-arc half-edge (KV16, spec
/// `kv16_hyperbola_arc_vocabulary`), endpoints excluded, in the half-edge's
/// walk direction. Twin-canonical exactly like [`arc_interior_samples`]
/// (computed on the lower-id half-edge, reversed for the other side).
///
/// Sampling is closed-form recursive parameter bisection (the
/// [`surface_pair_interior_samples`] shape with exact evaluation instead of
/// Newton): split `[t0, t1]` while the parametric midpoint deviates from the
/// chord midpoint by more than the sag tolerance
/// `max(a,b)·(1 − cos(π/n_seg))` — the same circle-step sag contract the
/// surface-pair sampling uses, at the hyperbola's own scale. Depth-capped
/// with a typed failure (never a silent chord fallback, P9).
pub(crate) fn hyperbola_interior_samples(
    arena: &BrepArena,
    h: crate::arena::HalfEdgeId,
    n_seg: u32,
) -> Result<Vec<Point3>, KernelV2Error> {
    let he = arena.half_edge(h)?;
    if !matches!(he.curve, Curve::HyperbolaArc { .. }) {
        return Ok(Vec::new());
    }
    let canon = h.min(he.twin);
    let che = arena.half_edge(canon)?;
    let Curve::HyperbolaArc {
        center,
        normal,
        major_axis,
        semi_transverse,
        semi_conjugate,
    } = che.curve
    else {
        return Err(KernelV2Error::CurveTwinMismatch { half_edge: canon });
    };
    let fid = arena.loop_(che.loop_id)?.face;
    let start = arena.vertex(che.origin)?.point;
    let end = arena.vertex(arena.half_edge(che.next)?.origin)?.point;
    let nu = [normal.x, normal.y, normal.z];
    let mr = [major_axis.x, major_axis.y, major_axis.z];
    let (Some(t0), Some(t1)) = (
        crate::geom::hyperbola_param(center, nu, mr, semi_conjugate, start),
        crate::geom::hyperbola_param(center, nu, mr, semi_conjugate, end),
    ) else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "degenerate hyperbola arc (endpoint has no parameter)",
        });
    };
    let step = std::f64::consts::PI / f64::from(n_seg);
    let tol = semi_transverse.max(semi_conjugate) * (1.0 - step.cos());
    #[allow(clippy::too_many_arguments)]
    fn refine(
        center: Point3,
        nu: [f64; 3],
        mr: [f64; 3],
        a: f64,
        b: f64,
        seg: (f64, Point3, f64, Point3),
        tol: f64,
        depth: u32,
        out: &mut Vec<Point3>,
    ) -> Result<(), &'static str> {
        let (ta, pa, tb, pb) = seg;
        let tm = 0.5 * (ta + tb);
        let pm = crate::geom::hyperbola_point_at(center, nu, mr, a, b, tm);
        let mid = [
            0.5 * (pa.x() + pb.x()),
            0.5 * (pa.y() + pb.y()),
            0.5 * (pa.z() + pb.z()),
        ];
        let sag =
            ((pm.x() - mid[0]).powi(2) + (pm.y() - mid[1]).powi(2) + (pm.z() - mid[2]).powi(2))
                .sqrt();
        if sag <= tol {
            return Ok(());
        }
        if depth == 0 || sag.is_nan() {
            return Err("hyperbola-arc refinement depth cap exceeded");
        }
        refine(center, nu, mr, a, b, (ta, pa, tm, pm), tol, depth - 1, out)?;
        out.push(pm);
        refine(center, nu, mr, a, b, (tm, pm, tb, pb), tol, depth - 1, out)
    }
    let mut samples = Vec::new();
    refine(
        center,
        nu,
        mr,
        semi_transverse,
        semi_conjugate,
        (t0, start, t1, end),
        tol,
        SURFACE_PAIR_REFINE_DEPTH,
        &mut samples,
    )
    .map_err(|reason| KernelV2Error::TessellationFailed { face: fid, reason })?;
    if h != canon {
        samples.reverse();
    }
    Ok(samples)
}

/// Convergence tolerance of the surface-pair Newton projection — mirrors
/// yang-rs's tested `relocate_onto_implicit_pair` contract (both residuals
/// ≤ 1e-13; meters-scale models).
const SURFACE_PAIR_PROJECT_TAU: f64 = 1e-13;

/// Depth cap of the recursive chord refinement (2¹² sub-chords per edge —
/// producer edges are sub-facet sized; hand-built edges spanning a large
/// sweep still fit comfortably).
const SURFACE_PAIR_REFINE_DEPTH: u32 = 12;

/// Project `p` onto the intersection curve of the pair by Gauss–Newton on
/// the two implicit residuals (Ref #24 Yang et al. 2025 §4.3 — the paper's
/// local refinement; same operator as yang-rs `relocate_onto_implicit_pair`).
/// With unit gradients g₁, g₂ the normal-equations step is
/// `x ← x − (λ₁g₁ + λ₂g₂)`, `[1 c; c 1]·λ = f`, `c = g₁·g₂` — undefined
/// (typed failure) when the normals are parallel (tangency; det = sin²θ).
fn project_onto_surface_pair(
    a: &crate::arena::PairSurface,
    b: &crate::arena::PairSurface,
    p: Point3,
) -> Result<Point3, &'static str> {
    let mut x = [p.x(), p.y(), p.z()];
    for _ in 0..32 {
        let Some((f1, g1)) = crate::geom::pair_surface_residual_gradient(a, x) else {
            return Err("surface-pair projection hit a defining surface's axis");
        };
        let Some((f2, g2)) = crate::geom::pair_surface_residual_gradient(b, x) else {
            return Err("surface-pair projection hit a defining surface's axis");
        };
        if f1.abs() <= SURFACE_PAIR_PROJECT_TAU && f2.abs() <= SURFACE_PAIR_PROJECT_TAU {
            return Ok(Point3::new(x[0], x[1], x[2]));
        }
        let c = g1[0] * g2[0] + g1[1] * g2[1] + g1[2] * g2[2];
        let det = 1.0 - c * c;
        // NaN-safe gate: a NaN det must fail too, not fall through.
        if det <= 1e-12 || det.is_nan() {
            return Err("surface-pair projection at a tangency (parallel surface normals)");
        }
        let l1 = (f1 - c * f2) / det;
        let l2 = (f2 - c * f1) / det;
        for k in 0..3 {
            x[k] -= l1 * g1[k] + l2 * g2[k];
        }
        if !x.iter().all(|v| v.is_finite()) {
            return Err("surface-pair projection diverged (non-finite iterate)");
        }
    }
    Err("surface-pair Newton projection did not converge")
}

/// Interior render samples of a procedural surface-pair curve piece between
/// two CERTIFIED on-curve endpoints (M5, `specs/m5_surface_pair_curve.md`
/// K9): recursive chord bisection, each midpoint Newton-projected onto BOTH
/// surfaces, splitting while the chord sag exceeds `chord_tol`. Endpoints
/// excluded; samples in `start → end` order. Typed failure (never a silent
/// chord fallback, P9) on tangency, non-convergence, or a projection that
/// leaves the chord's neighborhood (basin escape).
pub fn surface_pair_interior_samples(
    a: &crate::arena::PairSurface,
    b: &crate::arena::PairSurface,
    start: Point3,
    end: Point3,
    chord_tol: f64,
) -> Result<Vec<Point3>, &'static str> {
    if !(chord_tol.is_finite() && chord_tol > 0.0) {
        return Err("surface-pair refinement needs a positive finite chord tolerance");
    }
    fn refine(
        a: &crate::arena::PairSurface,
        b: &crate::arena::PairSurface,
        p0: Point3,
        p1: Point3,
        chord_tol: f64,
        depth: u32,
        out: &mut Vec<Point3>,
    ) -> Result<(), &'static str> {
        let m = Point3::new(
            0.5 * (p0.x() + p1.x()),
            0.5 * (p0.y() + p1.y()),
            0.5 * (p0.z() + p1.z()),
        );
        let mp = project_onto_surface_pair(a, b, m)?;
        let sag =
            ((mp.x() - m.x()).powi(2) + (mp.y() - m.y()).powi(2) + (mp.z() - m.z()).powi(2)).sqrt();
        if sag <= chord_tol {
            return Ok(());
        }
        if depth == 0 {
            return Err("surface-pair refinement depth cap exceeded");
        }
        let chord =
            ((p1.x() - p0.x()).powi(2) + (p1.y() - p0.y()).powi(2) + (p1.z() - p0.z()).powi(2))
                .sqrt();
        // NaN-safe gate (a NaN sag/chord must fail, not recurse).
        if sag >= chord || sag.is_nan() || chord.is_nan() {
            return Err("surface-pair projection left the chord neighborhood");
        }
        refine(a, b, p0, mp, chord_tol, depth - 1, out)?;
        out.push(mp);
        refine(a, b, mp, p1, chord_tol, depth - 1, out)
    }
    let mut out = Vec::new();
    refine(
        a,
        b,
        start,
        end,
        chord_tol,
        SURFACE_PAIR_REFINE_DEPTH,
        &mut out,
    )?;
    Ok(out)
}

/// Interior sample points of a surface-pair half-edge at the render chord
/// bound, endpoints excluded, in the half-edge's walk direction.
/// Twin-canonical exactly like [`arc_interior_samples`] (computed on the
/// lower-id half-edge, reversed for the other side). The absolute sag
/// tolerance is the chord sag of the `2π/n_seg` circle step on the pair's
/// SMALLER radius — the same density contract the circle sampling uses, so
/// no new tolerance is introduced.
pub(crate) fn surface_pair_edge_samples(
    arena: &BrepArena,
    h: crate::arena::HalfEdgeId,
    n_seg: u32,
) -> Result<Vec<Point3>, KernelV2Error> {
    let he = arena.half_edge(h)?;
    if !matches!(he.curve, Curve::SurfacePair { .. }) {
        return Ok(Vec::new());
    }
    let canon = h.min(he.twin);
    let che = arena.half_edge(canon)?;
    let Curve::SurfacePair { a, b } = che.curve else {
        return Err(KernelV2Error::CurveTwinMismatch { half_edge: canon });
    };
    let fid = arena.loop_(che.loop_id)?.face;
    let start = arena.vertex(che.origin)?.point;
    let end = arena.vertex(arena.half_edge(che.next)?.origin)?.point;
    let r_scale = crate::geom::pair_surface_scale(&a).min(crate::geom::pair_surface_scale(&b));
    let step = std::f64::consts::PI / f64::from(n_seg);
    let tol = r_scale * (1.0 - step.cos());
    let mut samples = surface_pair_interior_samples(&a, &b, start, end, tol)
        .map_err(|reason| KernelV2Error::TessellationFailed { face: fid, reason })?;
    if h != canon {
        samples.reverse();
    }
    Ok(samples)
}
