//! Stage-1 chart simplicity (Yang §4.5.4 "removing illegal intersections",
//! 2026-09-05): a face's unrolled boundary polygon must be SIMPLE before the
//! CDT sees it. [`chart_polygon_crossings`] finds every proper crossing
//! between boundary chords (a sweep-pruned pair scan), and
//! [`cone_chart_rim_demand`] turns the crossings that involve a RIM chord
//! into the rim segment count that clears them — the crossed vertex's radial
//! distance to the rim halves the allowed sagitta, the same factor-2 margin
//! the thin-band chart guard (`face_rim_pair_phantom_n`) uses for rim pairs.
//!
//! Anchor: R0044 face 173 at N = 131 — the thin-band guard cleared its two
//! rims (gap 2.25, sag 1.06) but a rim chord 176 units long still passed
//! over the hyperbola × surface-pair junction vertex that sits 0.5 units
//! inside the band; no rim-pair rule can see that vertex, the scan can.

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

/// The rim segment count that clears `crossings` on a CONE chart (the
/// isometric development, where a rim circle is an arc about the origin):
/// for every crossing whose chord `c` belongs to a rim circle (`rim_radius(c)
/// = Some(r)`, the circle's 3-D radius), the rim's chart radius is
/// `ℓ = ‖c.first‖`, the crossed chord's endpoints `q` sit at radial distance
/// `d = min |‖q‖ − ℓ|` from the rim, and the rim's chords must keep
/// `sag(r, N) = r(1 − cos(π/N)) ≤ d / 2` — the 3-D sagitta bounds the
/// development sagitta (`× sin α`), so this is conservative. The demand is
/// the max over crossings; `None` when no crossing involves a rim chord, a
/// crossed vertex sits ON the rim (`d ≤ 0`), or the density would exceed
/// 4096 (a true near-tangency: the loud stop stands, P9).
pub(crate) fn cone_chart_rim_demand(
    polys: &[Vec<cad_primitives::Point2>],
    crossings: &[(ChartSeg, ChartSeg)],
    rim_radius: impl Fn(ChartSeg) -> Option<f64>,
) -> Option<usize> {
    let pt = |(pi, k): ChartSeg, second: bool| -> (f64, f64) {
        let poly = &polys[pi];
        let p = poly[(k + usize::from(second)) % poly.len()];
        (p.x(), p.y())
    };
    let norm = |(x, y): (f64, f64)| (x * x + y * y).sqrt();
    let mut demand: Option<usize> = None;
    for &(a, b) in crossings {
        for (rim, other) in [(a, b), (b, a)] {
            let Some(r) = rim_radius(rim) else {
                continue;
            };
            let ell = norm(pt(rim, false));
            let d = (norm(pt(other, false)) - ell)
                .abs()
                .min((norm(pt(other, true)) - ell).abs());
            let positive = |x: f64| x.partial_cmp(&0.0) == Some(std::cmp::Ordering::Greater);
            if !positive(d) || !positive(r) {
                continue; // the crossed vertex is ON the rim / NaN / a degenerate rim
            }
            let sag = |n: usize| r * (1.0 - (std::f64::consts::PI / n as f64).cos());
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
