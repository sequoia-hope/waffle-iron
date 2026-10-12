//! Stage-1 chart simplicity (Yang §4.5.4 "removing illegal intersections",
//! 2026-09-05): a face's unrolled boundary polygon must be SIMPLE before the
//! CDT sees it. [`chart_polygon_crossings`] finds every proper crossing
//! between boundary chords (a sweep-pruned pair scan), and
//! [`chart_rim_demand`] turns the crossings that involve a RIM chord into
//! the rim segment count that clears them — the crossed vertex's radial
//! distance to the rim halves the allowed sagitta, the same factor-2 margin
//! the thin-band chart guard (`face_rim_pair_phantom_n`) uses for rim pairs.
//! [`cone_chart_rim_demand`] is its cone-development form; the planar CDT
//! path feeds it the circle itself.
//!
//! Anchors: R0044 face 173 at N = 131 — the thin-band guard cleared its two
//! rims (gap 2.25, sag 1.06) but a rim chord 176 units long still passed
//! over the hyperbola × surface-pair junction vertex that sits 0.5 units
//! inside the band; no rim-pair rule can see that vertex, the scan can. F0082
//! face 372 (2026-09-12) — a cylinder's base cap re-entering the next union
//! with the two corners where the near-coplanar stack's plane∩plane line
//! meets the rectangle walls 1.457e-3 inside the rim (r 0.2123) under a
//! 40° rim chord (N = 9, sag 1.3e-2): the planar CDT refused the
//! self-crossing ring loud; the scan derives N = 38 and the driver's retry
//! lands the cap.

/// A chord of chart polygon `poly`, from its vertex `seg` to `seg + 1`
/// (wrapping).
pub(crate) type ChartSeg = (usize, usize);

/// Every PROPER crossing between two chords of the chart polygons (the outer
/// boundary and the holes, all as closed loops). Chords that share a vertex
/// (adjacent chords of one loop, or two loops touching at a vertex) do not
/// cross; collinear overlaps are not reported (they are not what a coarse
/// sample produces — the CDT stop stays loud for those). Pairs are returned
/// with the lower `(poly, seg)` first, in scan order.
pub(crate) fn chart_polygon_crossings(
    polys: &[Vec<cad_primitives::Point2>],
) -> Vec<(ChartSeg, ChartSeg)> {
    struct Seg {
        id: ChartSeg,
        a: (f64, f64),
        b: (f64, f64),
        xmin: f64,
        xmax: f64,
        ymin: f64,
        ymax: f64,
    }
    let mut segs: Vec<Seg> = Vec::new();
    for (pi, poly) in polys.iter().enumerate() {
        let n = poly.len();
        if n < 2 {
            continue;
        }
        for k in 0..n {
            let (p, q) = (poly[k], poly[(k + 1) % n]);
            let a = (p.x(), p.y());
            let b = (q.x(), q.y());
            segs.push(Seg {
                id: (pi, k),
                a,
                b,
                xmin: a.0.min(b.0),
                xmax: a.0.max(b.0),
                ymin: a.1.min(b.1),
                ymax: a.1.max(b.1),
            });
        }
    }
    segs.sort_by(|s, t| {
        s.xmin
            .partial_cmp(&t.xmin)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(s.id.cmp(&t.id))
    });
    let orient = |o: (f64, f64), p: (f64, f64), q: (f64, f64)| -> f64 {
        (p.0 - o.0) * (q.1 - o.1) - (p.1 - o.1) * (q.0 - o.0)
    };
    let mut out: Vec<(ChartSeg, ChartSeg)> = Vec::new();
    for i in 0..segs.len() {
        let s = &segs[i];
        for t in &segs[i + 1..] {
            if t.xmin > s.xmax {
                break; // sorted by xmin: nothing further can overlap in x
            }
            if t.ymin > s.ymax || t.ymax < s.ymin {
                continue;
            }
            // Shared endpoint (adjacent chords, or loops touching at a vertex).
            if s.a == t.a || s.a == t.b || s.b == t.a || s.b == t.b {
                continue;
            }
            let o1 = orient(s.a, s.b, t.a);
            let o2 = orient(s.a, s.b, t.b);
            let o3 = orient(t.a, t.b, s.a);
            let o4 = orient(t.a, t.b, s.b);
            if o1 * o2 < 0.0 && o3 * o4 < 0.0 {
                let (lo, hi) = if s.id <= t.id {
                    (s.id, t.id)
                } else {
                    (t.id, s.id)
                };
                out.push((lo, hi));
            }
        }
    }
    out.sort();
    out
}

/// A rim circle as it appears in a face's chart: the centre of its chart
/// image, that image's chart radius `ell`, and the circle's 3-D `radius`
/// (the radius the Stage-1 sagitta `r(1 − cos(π/N))` is taken at). A cone's
/// development puts every rim about the origin at `ell = h / cos α`; a plane
/// carries the circle itself (`ell = radius`, the centre projected into the
/// face frame).
pub(crate) struct RimChart {
    pub(crate) center: (f64, f64),
    pub(crate) ell: f64,
    pub(crate) radius: f64,
}

/// The rim segment count that clears `crossings` on a chart whose rim
/// circles `rim` describes: for every crossing whose chord `c` belongs to a
/// rim (`rim(c) = Some(_)`), the crossed chord's endpoints `q` sit at radial
/// distance `d = |‖q − center‖ − ell|` from the rim's chart image, and the
/// rim must keep `sag(radius, N) = radius(1 − cos(π/N)) ≤ d / 2` (the
/// thin-band guard's factor-2 margin; on a cone the 3-D sagitta bounds the
/// development sagitta by `sin α`, so this is conservative there).
///
/// `d` is the SMALLER of the crossed chord's endpoint distances **among the
/// endpoints that are not on the rim** (2026-09-12, F0082 face 372): a chord
/// that leaves a rim vertex for a point just inside the rim is crossed
/// because of that inside point — the rim chords must pass outside IT — and
/// near the rim vertex the chord departs the circle more steeply than any
/// rim chord of a shorter span, so no rim chord can cross it there. An
/// endpoint within `1e-9·(1 + ell)` of the rim (the on-circle band the rim
/// sampler itself accepts) is "on the rim"; a crossed chord with both
/// endpoints on the rim derives nothing. The demand is the max over
/// crossings; `None` when no crossing involves a rim chord, no crossed
/// endpoint is off the rim, or the density would exceed 4096 (a true
/// near-tangency: the loud stop stands, P9).
pub(crate) fn chart_rim_demand(
    polys: &[Vec<cad_primitives::Point2>],
    crossings: &[(ChartSeg, ChartSeg)],
    rim: impl Fn(ChartSeg) -> Option<RimChart>,
) -> Option<usize> {
    let pt = |(pi, k): ChartSeg, second: bool| -> (f64, f64) {
        let poly = &polys[pi];
        let p = poly[(k + usize::from(second)) % poly.len()];
        (p.x(), p.y())
    };
    let positive = |x: f64| x.partial_cmp(&0.0) == Some(std::cmp::Ordering::Greater);
    let mut demand: Option<usize> = None;
    for &(a, b) in crossings {
        for (rim_seg, other) in [(a, b), (b, a)] {
            let Some(rc) = rim(rim_seg) else {
                continue;
            };
            if !positive(rc.radius) || !positive(rc.ell) {
                continue; // a degenerate rim / NaN
            }
            let dist = |q: (f64, f64)| -> f64 {
                let (dx, dy) = (q.0 - rc.center.0, q.1 - rc.center.1);
                ((dx * dx + dy * dy).sqrt() - rc.ell).abs()
            };
            let on_rim_band = 1e-9 * (1.0 + rc.ell);
            let d = [pt(other, false), pt(other, true)]
                .into_iter()
                .map(dist)
                .filter(|&d| d > on_rim_band)
                .fold(f64::INFINITY, f64::min);
            if !d.is_finite() {
                continue; // every crossed endpoint is ON the rim (or NaN)
            }
            let sag = |n: usize| rc.radius * (1.0 - (std::f64::consts::PI / n as f64).cos());
            let mut n = 3usize;
            let mut ok = true;
            while sag(n) > d / 2.0 {
                n += 1;
                if n > 4096 {
                    ok = false;
                    break;
                }
            }
            if ok {
                demand = Some(demand.map_or(n, |m: usize| m.max(n)));
            }
        }
    }
    demand
}

/// [`chart_rim_demand`] on a CONE chart (the isometric development, where a
/// rim circle is an arc about the origin): `rim_radius(c) = Some(r)` names a
/// rim chord's circle by its 3-D radius, and the rim's chart radius is
/// `ℓ = ‖c.first‖` (the chord's own first vertex lies on the rim's image).
pub(crate) fn cone_chart_rim_demand(
    polys: &[Vec<cad_primitives::Point2>],
    crossings: &[(ChartSeg, ChartSeg)],
    rim_radius: impl Fn(ChartSeg) -> Option<f64>,
) -> Option<usize> {
    chart_rim_demand(polys, crossings, |seg @ (pi, k): ChartSeg| {
        let radius = rim_radius(seg)?;
        let p = polys[pi][k % polys[pi].len()];
        let ell = (p.x() * p.x() + p.y() * p.y()).sqrt();
        Some(RimChart {
            center: (0.0, 0.0),
            ell,
            radius,
        })
    })
}

/// Is the universal-cover unroll of a sheared bounded lateral patch on
/// (P0038, 2026-10-12)? `YANG_S1_COVER_UNROLL=0|off` restores the
/// single-sheet STOP for dev A/B.
pub(crate) fn cover_unroll_on() -> bool {
    if let Some(v) = COVER_UNROLL_OVERRIDE.with(|c| c.get()) {
        return v;
    }
    !matches!(
        std::env::var("YANG_S1_COVER_UNROLL").as_deref(),
        Ok("0") | Ok("off")
    )
}

thread_local! {
    /// Test-only override of the `YANG_S1_COVER_UNROLL` gate — per THREAD, so
    /// parallel unit tests never race on process-global env: `Some(false)`
    /// forces the single-sheet STOP, `Some(true)` the cover unroll, `None`
    /// (production) reads the env gate.
    pub(crate) static COVER_UNROLL_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// A closed loop's azimuths lifted to the universal cover: the first kept
/// as given, each later one the previous plus the step wrapped to (−π, π]
/// (the `winding` convention). The loop's net winding is `last + (first −
/// last wrapped) − first`; a null-homotopic loop returns near its start.
pub(crate) fn unwrap_loop(theta: &[f64]) -> Vec<f64> {
    let two_pi = 2.0 * std::f64::consts::PI;
    let mut out: Vec<f64> = Vec::with_capacity(theta.len());
    let Some(&first) = theta.first() else {
        return out;
    };
    let mut th = first;
    out.push(th);
    for w in theta.windows(2) {
        let mut d = w[1] - w[0];
        while d > std::f64::consts::PI {
            d -= two_pi;
        }
        while d < -std::f64::consts::PI {
            d += two_pi;
        }
        th += d;
        out.push(th);
    }
    out
}

/// Even-odd point-in-polygon in the chart (a half-open ray along +x; a
/// vertex exactly on the ray counts once). Boundary points are not
/// distinguished — callers test points that are strictly interior or
/// strictly exterior by construction.
pub(crate) fn chart_point_in_polygon(
    p: cad_primitives::Point2,
    poly: &[cad_primitives::Point2],
) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let (px, py) = (p.x(), p.y());
    let mut inside = false;
    for k in 0..n {
        let (a, b) = (poly[k], poly[(k + 1) % n]);
        let (ax, ay, bx, by) = (a.x(), a.y(), b.x(), b.y());
        if (ay > py) != (by > py) {
            let x = ax + (py - ay) / (by - ay) * (bx - ax);
            if px < x {
                inside = !inside;
            }
        }
    }
    inside
}
