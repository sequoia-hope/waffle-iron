//! Analytic edge sampling — interior sample points for arc / ellipse /
//! hyperbola / surface-pair half-edges at the chord-bound resolution,
//! twin-symmetric. Move-only split from the tessellate god-module
//! (design review 2026-07-12 F9); byte-identical.

use super::*;

/// Axis-canonical in-plane frame for GRID-ALIGNED arc sampling: derived
/// from the circle NORMAL alone (no anchor vertex, no start point), so
/// every coaxial arc — any radius, any station along the axis, either twin
/// orientation — measures azimuth from the same origin. Returns
/// `(g1, g2, s)` with `(g1, g2, n_c)` right-handed around the
/// sign-canonicalized axis `n_c = s·normal` (`s = ±1` flips the axis so its
/// largest-magnitude component is positive; a unit vector's largest
/// component is ≥ 1/√3, so the pick is well-defined).
fn axis_grid_frame(normal: crate::arena::UnitVector3) -> ([f64; 3], [f64; 3], f64) {
    let n = [normal.x, normal.y, normal.z];
    let mut imax = 0usize;
    let mut imin = 0usize;
    for k in 1..3 {
        if n[k].abs() > n[imax].abs() {
            imax = k;
        }
        if n[k].abs() < n[imin].abs() {
            imin = k;
        }
    }
    let s = if n[imax] < 0.0 { -1.0 } else { 1.0 };
    let nc = [s * n[0], s * n[1], s * n[2]];
    // Seed: the coordinate axis least aligned with the axis (|component|
    // ≤ 1/√3 < 1, so the rejection below cannot vanish).
    let mut seed = [0.0f64; 3];
    seed[imin] = 1.0;
    let d = seed[0] * nc[0] + seed[1] * nc[1] + seed[2] * nc[2];
    let g1_raw = [
        seed[0] - d * nc[0],
        seed[1] - d * nc[1],
        seed[2] - d * nc[2],
    ];
    let l = (g1_raw[0] * g1_raw[0] + g1_raw[1] * g1_raw[1] + g1_raw[2] * g1_raw[2]).sqrt();
    let g1 = [g1_raw[0] / l, g1_raw[1] / l, g1_raw[2] / l];
    let g2 = [
        nc[1] * g1[2] - nc[2] * g1[1],
        nc[2] * g1[0] - nc[0] * g1[2],
        nc[0] * g1[1] - nc[1] * g1[0],
    ];
    (g1, g2, s)
}

/// Interior samples of the arc `(center, normal, radius, start→end CCW by
/// `sweep`)`, as `(walk-fraction, point)` pairs ordered along the walk.
/// Pure geometry — the arena-facing wrappers below resolve the canonical
/// half-edge, gather the conforming vertex pool, and reverse for the twin.
///
/// Two conformality mechanisms replace the former per-arc uniform
/// subdivision anchored at each arc's own start vertex (the KV9-F2a/R0054
/// family: a thin strip between coaxial rim arcs folds when a boundary
/// node lands mid-chord of an opposing chord that sags below the surface
/// deeper than the strip is wide — the mesh polylines the §4.4.2
/// restoration replaced were phase-locked for free by Stage-1's shared
/// revolve grid):
///
/// 1. **Global azimuth grid**: samples sit on `{j·2π/n_seg}` measured in
///    the [`axis_grid_frame`], so every coaxial arc — any radius, station,
///    start vertex, or twin orientation — samples at the SAME azimuths and
///    opposing chords pair into aligned ladder rungs (sample-vs-sample
///    grazes cannot form needles).
/// 2. **Conforming vertex inserts**: for each pool point (the incident
///    faces' boundary vertices) whose exact 3D distance to this circle is
///    within `4×` the arc's own max chord sag, an interior sample is
///    inserted at the point's azimuth — a junction vertex of an opposing
///    coaxial arc then faces a chord ENDPOINT, never a mid-chord
///    (vertex-vs-sample grazes cannot form needles). Points below the f32
///    render quantum off the circle are skipped (a sample there would
///    coincide with the vertex and trip the CDT's coincidence rejection);
///    beyond `4×` the sag, a needle's apex clears the chord by ≥ 4× its
///    deviation — far outside the fold margin.
///
/// Interior grid steps are exactly `2π/n_seg` and end/insert-adjacent
/// steps are shorter, so the per-chord sag bound is preserved. Samples
/// closer to an endpoint or predecessor than the f32 render quantum are
/// dropped — such a sample would mint a boundary sub-edge below render
/// resolution (the B2 degeneracy class).
pub(crate) fn arc_grid_samples(
    center: Point3,
    normal: crate::arena::UnitVector3,
    radius: f64,
    start: Point3,
    sweep: f64,
    n_seg: u32,
    conform: &[Point3],
) -> Vec<(f64, Point3)> {
    use std::f64::consts::PI;
    let (g1, g2, s) = axis_grid_frame(normal);
    let ds = [
        start.x() - center.x(),
        start.y() - center.y(),
        start.z() - center.z(),
    ];
    // Walk azimuth of the start around `normal` from `g1`: α = s·β with β
    // the azimuth in the canonical frame (ν×g1 = s·g2).
    let alpha_s = s
        * (ds[0] * g2[0] + ds[1] * g2[1] + ds[2] * g2[2])
            .atan2(ds[0] * g1[0] + ds[1] * g1[1] + ds[2] * g1[2]);
    let two_pi = 2.0 * PI;
    let delta = two_pi / f64::from(n_seg);
    // f32 end-guard: the render-precision quantum at this circle's
    // coordinate magnitude, as an angle.
    let scale = center
        .x()
        .abs()
        .max(center.y().abs())
        .max(center.z().abs())
        .max(radius)
        .max(1.0);
    let lin_guard = 8.0 * f64::from(f32::EPSILON) * scale;
    let ang_guard = lin_guard / radius;
    let on_circle = |beta: f64| -> Point3 {
        let (sb, cb) = beta.sin_cos();
        Point3::new(
            center.x() + radius * (cb * g1[0] + sb * g2[0]),
            center.y() + radius * (cb * g1[1] + sb * g2[1]),
            center.z() + radius * (cb * g1[2] + sb * g2[2]),
        )
    };
    let mut samples: Vec<(f64, Point3)> = Vec::new();
    for j in 0..n_seg {
        let beta = f64::from(j) * delta;
        // Walk parameter of this grid azimuth: t ∈ [0, 2π) past the start.
        let t = (s * beta - alpha_s).rem_euclid(two_pi);
        if t <= ang_guard || t >= sweep - ang_guard {
            continue;
        }
        samples.push((t, on_circle(beta)));
    }
    // Conforming vertex inserts (mechanism 2 above).
    let sag_max = radius * (1.0 - (delta / 2.0).cos());
    for p in conform {
        let dp = [p.x() - center.x(), p.y() - center.y(), p.z() - center.z()];
        let x = dp[0] * g1[0] + dp[1] * g1[1] + dp[2] * g1[2];
        let y = dp[0] * g2[0] + dp[1] * g2[1] + dp[2] * g2[2];
        let dz = dp[0] * normal.x + dp[1] * normal.y + dp[2] * normal.z;
        let rho = (x * x + y * y).sqrt();
        if !(rho.is_finite() && rho > 0.0) {
            continue; // on the axis — no azimuth
        }
        let d3 = (dz * dz + (rho - radius) * (rho - radius)).sqrt();
        if d3 < lin_guard || d3 > 4.0 * sag_max {
            continue;
        }
        let beta = y.atan2(x);
        let t = (s * beta - alpha_s).rem_euclid(two_pi);
        if t <= ang_guard || t >= sweep - ang_guard {
            continue;
        }
        samples.push((t, on_circle(beta)));
    }
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Deterministic sweep-dedup: drop any sample within the f32 quantum of
    // its kept predecessor (grid points are ≥ Δ apart; only inserts can
    // crowd, and a dropped GRID point leaves a step ≤ Δ + guard).
    let mut kept: Vec<(f64, Point3)> = Vec::with_capacity(samples.len());
    let mut t_prev = 0.0f64; // the start endpoint
    for (t, p) in samples {
        if t - t_prev <= ang_guard {
            continue;
        }
        t_prev = t;
        kept.push((t / sweep, p));
    }
    kept
}

/// Gate for the conforming CURVE-SAMPLE pool (spec
/// `yang_434_output_chord_refinement.md` inc-8a): `KV2_ARC_CONFORM_CURVES=1`
/// extends the arc's conforming pool with the incident boundary CURVES' own
/// sample points. DEFAULT OFF — built as the completion of inc-4's design
/// sentence ("…or CDT-split the graze") while anchoring R0003 face 577, but
/// that fold's defect measured as the ellipse sampler's density contract
/// (inc-8), and no corpus case names this mechanism yet. A future arc-vs-
/// curve graze (both sag-bound, still closer than one band) is its customer;
/// such a configuration fails LOUD (the fold tripwire), never silent, so
/// off-by-default opens no silent-wrong window.
fn curve_conform_enabled() -> bool {
    matches!(std::env::var("KV2_ARC_CONFORM_CURVES"), Ok(v) if v == "1" || v == "on")
}

/// Interior sample points of one boundary half-edge for the conforming
/// POOL of a nearby arc (inc-8): the points this edge will contribute to
/// its own face chains, so the arc can conform to them exactly like it
/// conforms to B-Rep vertices. `Arc`/`Circle` pool edges use their PURE
/// grid samples (grid azimuths are fixed, so no recursion into their own
/// conforming pass — and a coaxial arc's grid samples dedup against ours
/// anyway); the conic kinds use their own canonical samplers verbatim.
/// `LineSegment` contributes nothing (its endpoints are already in the
/// vertex pool). Errors propagate: a pool edge that cannot be sampled
/// fails ITS OWN face chain the same way in the same tessellation.
fn boundary_curve_pool_samples(
    arena: &BrepArena,
    h: crate::arena::HalfEdgeId,
    n_seg: u32,
) -> Result<Vec<Point3>, KernelV2Error> {
    let he = arena.half_edge(h)?;
    match he.curve {
        Curve::LineSegment => Ok(Vec::new()),
        Curve::Arc { .. } | Curve::Circle { .. } => {
            let canon = h.min(he.twin);
            let che = arena.half_edge(canon)?;
            let start = arena.vertex(che.origin)?.point;
            let (center, normal, radius, sweep) = match che.curve {
                Curve::Arc {
                    center,
                    normal,
                    radius,
                } => {
                    let end = arena.vertex(arena.half_edge(che.next)?.origin)?.point;
                    let n_arr = [normal.x, normal.y, normal.z];
                    let Some(sweep) = crate::geom::ccw_sweep(center, n_arr, start, end) else {
                        let fid = arena.loop_(che.loop_id)?.face;
                        return Err(KernelV2Error::TessellationFailed {
                            face: fid,
                            reason: "degenerate arc (endpoint has no radial direction)",
                        });
                    };
                    (center, normal, radius, sweep)
                }
                Curve::Circle {
                    center,
                    normal,
                    radius,
                } => (center, normal, radius, 2.0 * std::f64::consts::PI),
                _ => unreachable!("outer match narrowed to Arc | Circle"),
            };
            Ok(
                arc_grid_samples(center, normal, radius, start, sweep, n_seg, &[])
                    .into_iter()
                    .map(|(_, p)| p)
                    .collect(),
            )
        }
        Curve::EllipseArc { .. } => ellipse_interior_samples(arena, h, n_seg),
        Curve::HyperbolaArc { .. } => hyperbola_interior_samples(arena, h, n_seg),
        Curve::SurfacePair { .. } => surface_pair_edge_samples(arena, h, n_seg),
    }
}

/// Interior sample points of an arc half-edge on the global azimuth grid
/// (endpoints excluded), as `(walk-fraction, point)` pairs IN THE
/// HALF-EDGE'S WALK DIRECTION.
///
/// Bitwise twin-symmetric: the samples are computed on the CANONICAL
/// (lower-id) half-edge of the twin pair and reversed for the other side,
/// so the two faces sharing the arc emit identical sample positions —
/// load-bearing for cross-face watertightness (a planar annulus face and
/// the cylinder patch share their intersection-circle arcs).
pub(crate) fn arc_interior_samples_frac(
    arena: &BrepArena,
    h: crate::arena::HalfEdgeId,
    n_seg: u32,
) -> Result<Vec<(f64, Point3)>, KernelV2Error> {
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
    // Conforming pool: every boundary vertex of the two incident faces
    // except this arc's own endpoints, PLUS (inc-8, the R0003 face-577
    // family) every boundary CURVE'S own interior sample points — a
    // non-coaxial curve grazing this arc realizes its closeness in its
    // samples, not its vertices, and without conforming inserts the arc's
    // chord sag swallows the strip between them (inverted-lift fold).
    // Resolved from the CANONICAL half-edge, so both faces sharing the arc
    // gather the identical pool (twin symmetry of the inserted samples).
    // Which pool points actually insert a sample is decided geometrically
    // in [`arc_grid_samples`] (the 4×sag constructive-coverage window).
    let end_vid = arena.half_edge(che.next)?.origin;
    let fid_twin = arena.loop_(arena.half_edge(che.twin)?.loop_id)?.face;
    let curve_pool = curve_conform_enabled();
    let mut conform: Vec<Point3> = Vec::new();
    let mut pool_faces = vec![fid];
    if fid_twin != fid {
        pool_faces.push(fid_twin);
    }
    let mut n_vert_pool = 0usize;
    for pf in pool_faces {
        let face = arena.face(pf)?;
        let mut lids = vec![face.outer_loop];
        lids.extend(face.inner_loops.iter().copied());
        for lid in lids {
            for h2 in arena.loop_half_edges(lid)? {
                let he2 = arena.half_edge(h2)?;
                let o = he2.origin;
                if o != che.origin && o != end_vid {
                    conform.push(arena.vertex(o)?.point);
                    n_vert_pool += 1;
                }
                if curve_pool && h2.min(he2.twin) != canon {
                    conform.extend(boundary_curve_pool_samples(arena, h2, n_seg)?);
                }
            }
        }
    }
    let mut samples = arc_grid_samples(center, normal, radius, start, sweep, n_seg, &conform);
    // Dev-only conforming-pool probe (inc-8): per-arc pool composition +
    // window decisions, filtered to one incident face id.
    if std::env::var("KV2_ARC_CONFORM_PROBE")
        .is_ok_and(|v| v == format!("{}", fid.0) || v == format!("{}", fid_twin.0))
    {
        let (g1, g2, _) = axis_grid_frame(normal);
        let delta = 2.0 * std::f64::consts::PI / f64::from(n_seg);
        let sag_max = radius * (1.0 - (delta / 2.0).cos());
        let mut d3min = f64::INFINITY;
        let mut n_in_window = 0usize;
        for p in &conform {
            let dp = [p.x() - center.x(), p.y() - center.y(), p.z() - center.z()];
            let x = dp[0] * g1[0] + dp[1] * g1[1] + dp[2] * g1[2];
            let y = dp[0] * g2[0] + dp[1] * g2[1] + dp[2] * g2[2];
            let dz = dp[0] * normal.x + dp[1] * normal.y + dp[2] * normal.z;
            let rho = (x * x + y * y).sqrt();
            let d3 = (dz * dz + (rho - radius) * (rho - radius)).sqrt();
            d3min = d3min.min(d3);
            if d3 <= 4.0 * sag_max {
                n_in_window += 1;
            }
        }
        eprintln!(
            "[conform-probe] canon={canon:?} faces=({},{}) r={radius:.6e} sweep={sweep:.6e} \
             n_seg={n_seg} sag_max={sag_max:.3e} pool_verts={n_vert_pool} \
             pool_total={} in_window={n_in_window} d3min={d3min:.3e} emitted={}",
            fid.0,
            fid_twin.0,
            conform.len(),
            samples.len()
        );
    }
    if h != canon {
        samples.reverse();
        for (frac, _) in &mut samples {
            *frac = 1.0 - *frac;
        }
    }
    Ok(samples)
}

/// [`arc_interior_samples_frac`] without the walk fractions, for callers
/// that only chain the polyline (introspection, sphere/torus/planar loop
/// sampling).
pub(crate) fn arc_interior_samples(
    arena: &BrepArena,
    h: crate::arena::HalfEdgeId,
    n_seg: u32,
) -> Result<Vec<Point3>, KernelV2Error> {
    Ok(arc_interior_samples_frac(arena, h, n_seg)?
        .into_iter()
        .map(|(_, p)| p)
        .collect())
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
    // inc-8 (spec `yang_434_output_chord_refinement.md`, the R0003 face-577
    // fold): the uniform-parameter grid alone bounds chord sag only at the
    // ELLIPSE'S OWN scale (max chord sag ≈ R_maj·(1−cos(π/n_seg))) — a
    // steep plane×cone section has R_maj far above the surface's local
    // radius, so its chords cut deeper into the face than any on-surface
    // render feature and the patch CDT folds against them. The density
    // contract every other boundary sampler honors is the circle-step sag
    // at the SURFACE'S local scale (`surface_pair_edge_samples` uses the
    // smallest defining-surface radius; the arc grid uses its own circle's
    // radius). Bring the ellipse to the same contract: bisect each grid
    // span while its measured sag exceeds `r_local·(1−cos(π/n_seg))`,
    // r_local the smallest local radius of the two incident faces'
    // surfaces at the edge endpoints. Planes contribute no scale of their
    // own; if neither face has one, the ellipse's own scale stands (the
    // prior behavior, and the grid already meets it).
    let fid_twin = arena.loop_(arena.half_edge(che.twin)?.loop_id)?.face;
    let mut r_local = f64::INFINITY;
    for pf in [fid, fid_twin] {
        if let Some(surf) = &arena.face(pf)?.surface {
            for p in [start, end] {
                if let Some(r) = face_surface_local_scale(surf, p) {
                    r_local = r_local.min(r);
                }
            }
        }
    }
    if !r_local.is_finite() {
        r_local = major_radius.max(minor_radius);
    }
    // Dev off-knob: `KV2_ELLIPSE_SAG=0|off` restores the pure k-grid
    // (an infinite tol makes every span pass, byte-identically).
    let tol = if matches!(std::env::var("KV2_ELLIPSE_SAG"), Ok(v) if v == "0" || v == "off") {
        f64::INFINITY
    } else {
        r_local * (1.0 - (std::f64::consts::PI / f64::from(n_seg)).cos())
    };
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
        let pm = crate::geom::ellipse_point_at(center, nu, mr, a, b, tm);
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
            return Err("ellipse-arc refinement depth cap exceeded");
        }
        refine(center, nu, mr, a, b, (ta, pa, tm, pm), tol, depth - 1, out)?;
        out.push(pm);
        refine(center, nu, mr, a, b, (tm, pm, tb, pb), tol, depth - 1, out)
    }
    let ep = |j: u32| -> (f64, Point3) {
        let t = t0 + sweep * f64::from(j) / f64::from(k);
        if j == 0 {
            (t, start)
        } else if j == k {
            (t, end)
        } else {
            (
                t,
                crate::geom::ellipse_point_at(center, nu, mr, major_radius, minor_radius, t),
            )
        }
    };
    let mut samples = Vec::with_capacity(k as usize - 1);
    for j in 0..k {
        let (ta, pa) = ep(j);
        let (tb, pb) = ep(j + 1);
        refine(
            center,
            nu,
            mr,
            major_radius,
            minor_radius,
            (ta, pa, tb, pb),
            tol,
            SURFACE_PAIR_REFINE_DEPTH,
            &mut samples,
        )
        .map_err(|reason| KernelV2Error::TessellationFailed { face: fid, reason })?;
        if j + 1 < k {
            samples.push(pb);
        }
    }
    if h != canon {
        samples.reverse();
    }
    Ok(samples)
}

/// Local density scale of a face's surface at a point — the radius the
/// circle-step sag contract applies at. `None` for a plane (no curvature
/// of its own to bound sampling against).
fn face_surface_local_scale(surface: &crate::arena::Surface, p: Point3) -> Option<f64> {
    use crate::arena::Surface;
    match *surface {
        Surface::Plane(_) => None,
        Surface::Cylinder { radius, .. } | Surface::Sphere { radius, .. } => Some(radius),
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
            ..
        } => {
            let a = [axis_dir.x, axis_dir.y, axis_dir.z];
            let d = [p.x() - apex.x(), p.y() - apex.y(), p.z() - apex.z()];
            let h = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
            Some(h.abs() * half_angle.tan())
        }
        Surface::Torus { minor_radius, .. } => Some(minor_radius),
    }
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

/// Convergence FLOOR of the surface-pair Newton projection — mirrors
/// yang-rs's tested `relocate_onto_implicit_pair` contract: both residuals
/// ≤ `max(1e-13, 8·ε·L)`, `L` the SEED's coordinate magnitude. Every pair
/// residual is a LENGTH, so at coordinate magnitude `L` no residual can be
/// evaluated below ~`8·ε·L`; the bare 1e-13 (the pre-2026-07-28 yang
/// contract this constant mirrored) is ~100× below one ULP at the R0044
/// scale (L ≈ 6.2e3) and a fully converged root ran out of iterations
/// ("did not converge", 2026-08-19). At unit scale 8·ε·L ≈ 2e-15 < 1e-13,
/// so meters-scale behavior is unchanged. `L` is the seed's, never the
/// iterate's, so a diverging iterate cannot inflate its own acceptance.
const SURFACE_PAIR_PROJECT_TAU: f64 = 1e-13;

/// See [`SURFACE_PAIR_PROJECT_TAU`].
fn surface_pair_project_tau(seed: [f64; 3]) -> f64 {
    let l = seed[0].abs().max(seed[1].abs()).max(seed[2].abs());
    SURFACE_PAIR_PROJECT_TAU.max(8.0 * f64::EPSILON * l)
}

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
    let tau = surface_pair_project_tau(x);
    for _ in 0..32 {
        let Some((f1, g1)) = crate::geom::pair_surface_residual_gradient(a, x) else {
            return Err("surface-pair projection hit a defining surface's axis");
        };
        let Some((f2, g2)) = crate::geom::pair_surface_residual_gradient(b, x) else {
            return Err("surface-pair projection hit a defining surface's axis");
        };
        if f1.abs() <= tau && f2.abs() <= tau {
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
/// SMALLEST LOCAL radius along the edge (`pair_surface_local_scale` at both
/// endpoints of both surfaces — a cone's local radius `|h|·tanα`, a
/// cylinder's constant radius) — the same density contract the circle
/// sampling uses, so no new tolerance is introduced. (Until 2026-08-19 this
/// used the constant `pair_surface_scale`, which is 0 for a cone, so every
/// cyl×cone / cone×cone pair edge dead-ended on "needs a positive finite
/// chord tolerance" — R0020/R0044. An edge through the apex still yields 0
/// and STOPs loudly there: a pair curve through the apex is degenerate.)
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
    let r_scale = [start, end]
        .into_iter()
        .flat_map(|p| {
            [
                crate::geom::pair_surface_local_scale(&a, p),
                crate::geom::pair_surface_local_scale(&b, p),
            ]
        })
        .fold(f64::INFINITY, f64::min);
    let step = std::f64::consts::PI / f64::from(n_seg);
    let tol = r_scale * (1.0 - step.cos());
    let mut samples = surface_pair_interior_samples(&a, &b, start, end, tol)
        .map_err(|reason| KernelV2Error::TessellationFailed { face: fid, reason })?;
    if h != canon {
        samples.reverse();
    }
    Ok(samples)
}
