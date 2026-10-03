//! Where two projected curves cross in `(u, v)` — the SPLIT half of **D1c**
//! (`specs/drawings_and_mbd.md` §5.2 increment 3).
//!
//! A projected curve's visibility is not constant along it: an edge runs out
//! from behind the solid's outline, a silhouette branch dives under the near
//! side of its own torus. Those changes happen where the curve crosses
//! ANOTHER projected curve, so classification has to split first and classify
//! the pieces. This module finds the split parameters; [`super::visibility`]
//! decides what each piece is.
//!
//! ## Exact where the vocabulary is exact
//!
//! Each [`Curve2`] decomposes into **pieces** of two kinds, and no more:
//!
//! | curve | pieces |
//! |---|---|
//! | [`Curve2::Line`] | one segment |
//! | [`Curve2::Polyline`] | one segment per chord |
//! | [`Curve2::Circle`], [`Curve2::Ellipse`] | one conic `c + cos t·A + sin t·B` |
//! | [`Curve2::Point`] | none — a dot crosses nothing |
//!
//! so three pair kinds carry the whole problem, and two of the three are
//! closed form:
//!
//! - **segment × segment** — one 2×2 solve.
//! - **segment × conic** — in the conic's own normalized frame (where the
//!   conic IS the unit circle and the segment is still a segment) the
//!   condition is a QUADRATIC in the segment's parameter. Exact for a circle
//!   and an ellipse alike, which is the point of carrying the ellipse
//!   analytically through D1a at all.
//! - **conic × conic** — a quartic, and solved here instead by bracketing
//!   sign changes of the second conic's implicit function along the first and
//!   bisecting to float precision. The implicit is exact, so a transversal
//!   root converges to it; what a sample can miss is a pair of roots closer
//!   together than the sample spacing, which is a near-TANGENTIAL contact and
//!   is detected and DECLINED (see below) rather than passed over in silence.
//!   This is the same shape of answer D1b's clip gives for a torus branch
//!   against a surface-pair boundary.
//!
//! ## The chord band
//!
//! A [`Curve2::Polyline`] is an inscribed approximation of its 3-D source, so
//! a crossing the true curves make can show up on the chords as a NEAR MISS,
//! by up to the chord sagitta. Every piece-pair test therefore accepts a root
//! that lands within `band` of a piece's own parameter window, with `band`
//! the caller's chord band in model units (the view's own size times the
//! render sagitta — [`super::visibility`] derives it). An analytic pair needs
//! no band and is unaffected by it, because its roots land strictly inside.
//!
//! ## What is declined, and why a tangency is one
//!
//! A **tangential contact** — two curves that touch without crossing, or two
//! roots within the band of each other — is counted as
//! [`ProjectionDeclines::split_tangency`] and no split is emitted. Both
//! choices would be wrong some of the time: splitting at a true tangency
//! produces two pieces of the same visibility that the merge immediately
//! rejoins (harmless), while NOT splitting at a contact that was really a
//! crossing leaves one piece spanning two visibilities, reported as the one
//! its midpoint has. The count is what makes the second case visible instead
//! of silent.
//!
//! A **parallel or coincident** piece pair is NOT a tangency and is not
//! counted. Two curves that project onto each other have no transversal
//! crossing to find and need no split — what they need is the coincidence
//! MERGE, which is [`super::visibility`]'s, and counting them here would bury
//! the real declines under the ordinary degeneracy of an axis-aligned view
//! (where a box's front and back faces project exactly on top of each other).

use std::f64::consts::TAU;

use cad_primitives::Point2;
use waffle_types::kernel::projection::Curve2;

/// Relative slack below which a 2×2 solve's determinant counts as PARALLEL,
/// as a sine of the angle between the two pieces.
const PARALLEL_REL: f64 = 1e-12;

/// Bisection steps for a conic × conic root. `TAU` over `2^48` is far below
/// any band downstream; the iteration is cheap and bounded.
const BISECT_STEPS: u32 = 48;

/// Below this, the second conic's implicit function is ZERO along the whole of
/// the first and the two are THE SAME conic — a float-noise test, since two
/// distinct conics meet in at most four points and cannot agree on an arc.
/// Like a parallel segment pair, a coincident conic pair has no transversal
/// crossing to find and needs no split; what it needs is the coincidence
/// merge. Measured 2026-10-03: a through hole's two rim circles come back with
/// radii differing in the last bit, so their implicit against each other is
/// ~3e-16, and without this test the sign noise around zero minted THIRTEEN
/// spurious roots and cut one rim into alternating visible and hidden arcs.
const COINCIDENT_IMPLICIT: f64 = 1e-9;

/// Samples per full turn when bracketing a conic × conic root. Two conics
/// meet in at most four points, so this resolves every transversal pair
/// whose roots are more than ~1.2° apart; closer than that is the
/// near-tangential case, which is detected and declined.
const CONIC_SAMPLES_PER_TURN: f64 = 512.0;

/// One crossing of two curves: the parameter on each, in that curve's own
/// [`Curve2::param_range`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Crossing2 {
    pub a: f64,
    pub b: f64,
}

/// What one piece pair costs in the budget's units, so the budget bounds WORK
/// and not merely the number of pairs: a 2×2 solve is the unit, a quadratic
/// four of them, and a bracketed conic pair its own sample count.
const COST_SEGMENT_SEGMENT: u64 = 1;
const COST_SEGMENT_CONIC: u64 = 4;

/// A projected curve decomposed for the crossing search, with each piece's
/// view-plane bounding box alongside it.
pub(crate) struct Decomposed {
    pieces: Vec<Piece>,
    /// Bounding box of the whole curve: `[min_u, min_v, max_u, max_v]`.
    pub bbox: [f64; 4],
}

impl Decomposed {
    /// The curve's pieces, or an empty decomposition for a point or an empty
    /// polyline.
    pub fn of(curve: &Curve2) -> Decomposed {
        let pieces = pieces_of(curve);
        let mut bbox = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for p in &pieces {
            let b = p.bbox();
            bbox[0] = bbox[0].min(b[0]);
            bbox[1] = bbox[1].min(b[1]);
            bbox[2] = bbox[2].max(b[2]);
            bbox[3] = bbox[3].max(b[3]);
        }
        Decomposed { pieces, bbox }
    }

    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }
}

/// Whether two curve boxes overlap, with `band` of slack on each side.
pub(crate) fn boxes_overlap(a: &[f64; 4], b: &[f64; 4], band: f64) -> bool {
    a[0] <= b[2] + band && b[0] <= a[2] + band && a[1] <= b[3] + band && b[1] <= a[3] + band
}

/// A piece of a decomposed curve: the two shapes every [`Curve2`] arm reduces
/// to, each carrying the map back to its parent's parameter.
enum Piece {
    /// `p + s·d` for `s ∈ [0, 1]`; the parent parameter is `t0 + s·dt`.
    Segment {
        p: [f64; 2],
        d: [f64; 2],
        t0: f64,
        dt: f64,
    },
    /// `c + cos t·a + sin t·b` over the parent's own `[t0, t1]` — the
    /// parameter IS the parent's.
    Conic {
        c: [f64; 2],
        a: [f64; 2],
        b: [f64; 2],
        t0: f64,
        t1: f64,
    },
}

fn pieces_of(curve: &Curve2) -> Vec<Piece> {
    match *curve {
        Curve2::Point(_) => Vec::new(),
        Curve2::Line { start, end } => vec![Piece::Segment {
            p: [start.x(), start.y()],
            d: [end.x() - start.x(), end.y() - start.y()],
            t0: 0.0,
            dt: 1.0,
        }],
        Curve2::Polyline { ref points, closed } => {
            let n = points.len();
            if n < 2 {
                return Vec::new();
            }
            let chords = if closed { n } else { n - 1 };
            (0..chords)
                .map(|i| {
                    let a = points[i];
                    let b = points[(i + 1) % n];
                    Piece::Segment {
                        p: [a.x(), a.y()],
                        d: [b.x() - a.x(), b.y() - a.y()],
                        t0: i as f64,
                        dt: 1.0,
                    }
                })
                .collect()
        }
        Curve2::Circle {
            center,
            radius,
            start_angle,
            end_angle,
        } => vec![Piece::Conic {
            c: [center.x(), center.y()],
            a: [radius, 0.0],
            b: [0.0, radius],
            t0: start_angle,
            t1: end_angle,
        }],
        Curve2::Ellipse {
            center,
            major_axis,
            major_radius,
            minor_radius,
            start_param,
            end_param,
        } => vec![Piece::Conic {
            c: [center.x(), center.y()],
            a: [major_axis[0] * major_radius, major_axis[1] * major_radius],
            b: [-major_axis[1] * minor_radius, major_axis[0] * minor_radius],
            t0: start_param,
            t1: end_param,
        }],
    }
}

impl Piece {
    fn bbox(&self) -> [f64; 4] {
        match *self {
            Piece::Segment { p, d, .. } => {
                let q = [p[0] + d[0], p[1] + d[1]];
                [
                    p[0].min(q[0]),
                    p[1].min(q[1]),
                    p[0].max(q[0]),
                    p[1].max(q[1]),
                ]
            }
            // Conservative rather than exact: the extreme of
            // `cos t·a + sin t·b` is `√(aᵢ² + bᵢ²) ≤ |aᵢ| + |bᵢ|` per axis,
            // and this box is only ever used to SKIP work.
            Piece::Conic { c, a, b, .. } => {
                let (ru, rv) = (
                    (a[0] * a[0] + b[0] * b[0]).sqrt(),
                    (a[1] * a[1] + b[1] * b[1]).sqrt(),
                );
                [c[0] - ru, c[1] - rv, c[0] + ru, c[1] + rv]
            }
        }
    }

    /// The piece's own scale, for turning a model-unit band into a parameter
    /// margin.
    fn speed(&self) -> f64 {
        match *self {
            Piece::Segment { d, .. } => (d[0] * d[0] + d[1] * d[1]).sqrt(),
            Piece::Conic { a, b, .. } => (a[0] * a[0] + a[1] * a[1])
                .sqrt()
                .max((b[0] * b[0] + b[1] * b[1]).sqrt()),
        }
    }

    fn eval(&self, t: f64) -> [f64; 2] {
        match *self {
            Piece::Segment { p, d, t0, dt } => {
                let s = if dt != 0.0 { (t - t0) / dt } else { 0.0 };
                [p[0] + s * d[0], p[1] + s * d[1]]
            }
            Piece::Conic { c, a, b, .. } => {
                let (sn, cs) = t.sin_cos();
                [c[0] + cs * a[0] + sn * b[0], c[1] + cs * a[1] + sn * b[1]]
            }
        }
    }
}

/// A conic in its own normalized frame: the map taking a view-plane point to
/// coordinates where the conic is the UNIT CIRCLE, plus the parameter window.
struct Normalized {
    c: [f64; 2],
    /// Rows of `[a b]⁻¹`.
    inv: [[f64; 2]; 2],
    t0: f64,
    t1: f64,
    /// The band in model units expressed in the normalized frame — the
    /// LARGEST such conversion, which is the conservative direction.
    band_scale: f64,
}

impl Normalized {
    fn of(piece: &Piece, band: f64) -> Option<Normalized> {
        let Piece::Conic { c, a, b, t0, t1 } = *piece else {
            return None;
        };
        let det = a[0] * b[1] - a[1] * b[0];
        let (la, lb) = (
            (a[0] * a[0] + a[1] * a[1]).sqrt(),
            (b[0] * b[0] + b[1] * b[1]).sqrt(),
        );
        if !det.is_finite() || det.abs() <= PARALLEL_REL * la * lb {
            return None; // a degenerate conic: the caller falls back
        }
        let smallest = la.min(lb);
        Some(Normalized {
            c,
            inv: [[b[1] / det, -b[0] / det], [-a[1] / det, a[0] / det]],
            t0,
            t1,
            band_scale: if smallest > 0.0 { band / smallest } else { 0.0 },
        })
    }

    /// Normalized coordinates of a view-plane point.
    fn local(&self, p: [f64; 2]) -> [f64; 2] {
        let v = [p[0] - self.c[0], p[1] - self.c[1]];
        [
            self.inv[0][0] * v[0] + self.inv[0][1] * v[1],
            self.inv[1][0] * v[0] + self.inv[1][1] * v[1],
        ]
    }

    /// `|p|² − 1` in the normalized frame: zero exactly on the conic,
    /// negative inside.
    fn implicit(&self, p: [f64; 2]) -> f64 {
        let l = self.local(p);
        l[0] * l[0] + l[1] * l[1] - 1.0
    }

    /// The conic's own parameter at a point known to be on it, if that
    /// parameter falls inside the arc's window (with the band's margin).
    fn param_of(&self, p: [f64; 2]) -> Option<f64> {
        let l = self.local(p);
        let raw = l[1].atan2(l[0]);
        let margin = self.band_scale;
        // Unwrap into the window: the window spans at most one turn, so at
        // most two candidate lifts can land in it.
        let k = ((self.t0 - raw) / TAU).floor();
        for i in [k, k + 1.0, k + 2.0] {
            let t = raw + i * TAU;
            if t >= self.t0 - margin && t <= self.t1 + margin {
                return Some(t.clamp(self.t0, self.t1));
            }
        }
        None
    }
}

/// Every crossing of two projected curves, appended to `out`; the return is
/// the number of near-TANGENTIAL contacts declined (see the module docs).
///
/// `band` is the chord band in model units. `budget` is decremented by the
/// work actually done and the search stops when it runs out, which the caller
/// turns into a whole-view decline.
pub(crate) fn crossings(
    a: &Decomposed,
    b: &Decomposed,
    band: f64,
    out: &mut Vec<Crossing2>,
    budget: &mut u64,
) -> u32 {
    let mut tangencies = 0u32;
    // Only the pieces inside the two curves' overlap region can cross, which
    // is what keeps a pair of long polylines from costing their full product.
    let region = [
        a.bbox[0].max(b.bbox[0]) - band,
        a.bbox[1].max(b.bbox[1]) - band,
        a.bbox[2].min(b.bbox[2]) + band,
        a.bbox[3].min(b.bbox[3]) + band,
    ];
    let near: Vec<(&Piece, [f64; 4])> = a
        .pieces
        .iter()
        .map(|p| (p, p.bbox()))
        .filter(|(_, bb)| boxes_overlap(bb, &region, band))
        .collect();
    let other: Vec<(&Piece, [f64; 4])> = b
        .pieces
        .iter()
        .map(|p| (p, p.bbox()))
        .filter(|(_, bb)| boxes_overlap(bb, &region, band))
        .collect();

    for (pa, ba) in &near {
        for (pb, bb) in &other {
            if !boxes_overlap(ba, bb, band) {
                continue;
            }
            if *budget == 0 {
                return tangencies;
            }
            let cost = match (pa, pb) {
                (Piece::Segment { .. }, Piece::Segment { .. }) => COST_SEGMENT_SEGMENT,
                (Piece::Conic { .. }, Piece::Conic { .. }) => conic_samples(pa).max(1) as u64,
                _ => COST_SEGMENT_CONIC,
            };
            *budget = budget.saturating_sub(cost);
            tangencies += match (pa, pb) {
                (Piece::Segment { .. }, Piece::Segment { .. }) => {
                    segment_segment(pa, pb, band, out)
                }
                (Piece::Segment { .. }, Piece::Conic { .. }) => {
                    segment_conic(pa, pb, band, false, out)
                }
                (Piece::Conic { .. }, Piece::Segment { .. }) => {
                    segment_conic(pb, pa, band, true, out)
                }
                (Piece::Conic { .. }, Piece::Conic { .. }) => conic_conic(pa, pb, band, out),
            };
        }
    }
    tangencies
}

/// Closed form: one 2×2 solve, with the chord band as a parameter margin at
/// each piece's ends.
fn segment_segment(pa: &Piece, pb: &Piece, band: f64, out: &mut Vec<Crossing2>) -> u32 {
    let Piece::Segment {
        p: p1,
        d: d1,
        t0: a0,
        dt: adt,
    } = *pa
    else {
        return 0;
    };
    let Piece::Segment {
        p: p2,
        d: d2,
        t0: b0,
        dt: bdt,
    } = *pb
    else {
        return 0;
    };
    let den = d1[0] * d2[1] - d1[1] * d2[0];
    let (l1, l2) = (pa.speed(), pb.speed());
    if !den.is_finite() || den.abs() <= PARALLEL_REL * l1 * l2 {
        // Parallel or coincident: no transversal crossing, and deliberately
        // NOT a tangency decline (see the module docs).
        return 0;
    }
    let r = [p2[0] - p1[0], p2[1] - p1[1]];
    let s = (r[0] * d2[1] - r[1] * d2[0]) / den;
    let t = (r[0] * d1[1] - r[1] * d1[0]) / den;
    let (ms, mt) = (
        if l1 > 0.0 { band / l1 } else { 0.0 },
        if l2 > 0.0 { band / l2 } else { 0.0 },
    );
    if s >= -ms && s <= 1.0 + ms && t >= -mt && t <= 1.0 + mt {
        out.push(Crossing2 {
            a: a0 + adt * s.clamp(0.0, 1.0),
            b: b0 + bdt * t.clamp(0.0, 1.0),
        });
    }
    0
}

/// Closed form: in the conic's normalized frame the segment is still a
/// segment and the conic is the unit circle, so the condition is a quadratic
/// in the segment's parameter.
fn segment_conic(
    seg: &Piece,
    conic: &Piece,
    band: f64,
    swapped: bool,
    out: &mut Vec<Crossing2>,
) -> u32 {
    let Piece::Segment { p, d, t0, dt } = *seg else {
        return 0;
    };
    let Some(n) = Normalized::of(conic, band) else {
        return 0;
    };
    let l0 = n.local(p);
    let l1 = n.local([p[0] + d[0], p[1] + d[1]]);
    let ld = [l1[0] - l0[0], l1[1] - l0[1]];
    let qa = ld[0] * ld[0] + ld[1] * ld[1];
    let qb = 2.0 * (l0[0] * ld[0] + l0[1] * ld[1]);
    let qc = l0[0] * l0[0] + l0[1] * l0[1] - 1.0;
    if !(qa.is_finite() && qa > 0.0) {
        return 0;
    }
    let disc = qb * qb - 4.0 * qa * qc;
    let seg_len = seg.speed();
    let ms = if seg_len > 0.0 { band / seg_len } else { 0.0 };
    // The quadratic's minimum, which is how far the segment's line passes
    // from the conic in the normalized frame. `|q|/2` is the normalized
    // distance to first order, so this is the tangency test.
    let s_min = -qb / (2.0 * qa);
    let q_min = qc - qb * qb / (4.0 * qa);
    let grazes = q_min.abs() <= 2.0 * n.band_scale && s_min >= -ms && s_min <= 1.0 + ms;
    if disc < 0.0 {
        // A contact the quadratic says the segment misses — but by less than
        // the band, so the two really touch and the sign of `disc` is a float
        // residual rather than a geometric fact. SPLIT at the contact anyway,
        // at the quadratic's own exact minimizer, and still count the decline.
        //
        // Not doing so made the split depend on which way that residual fell,
        // and the asymmetry is visible on the plainest fixture there is: an
        // oblique cylinder's far rim is tangent to BOTH of its silhouette
        // rulings, `disc` came out a hair positive at one and a hair negative
        // at the other, so only one contact was split and the rim's hidden arc
        // came back 0.0150 against the exact 0.0228 — a third of it drawn
        // solid. The module docs already give the rule this follows: splitting
        // at a true tangency makes two pieces of the same visibility that the
        // MERGE rejoins, which costs nothing, while not splitting at a contact
        // that was really a crossing leaves one piece spanning two
        // visibilities, which is a wrong drawing.
        if grazes {
            let s_in = s_min.clamp(0.0, 1.0);
            let pt = [p[0] + s_in * d[0], p[1] + s_in * d[1]];
            if let Some(tc) = n.param_of(pt) {
                let ts = t0 + dt * s_in;
                out.push(if swapped {
                    Crossing2 { a: tc, b: ts }
                } else {
                    Crossing2 { a: ts, b: tc }
                });
            }
        }
        return u32::from(grazes);
    }
    let root = disc.sqrt();
    let mut tangency = 0u32;
    // Two roots within the band of each other are a tangency, not two
    // crossings.
    if root / qa.max(f64::MIN_POSITIVE) <= 2.0 * ms && grazes {
        tangency = 1;
    }
    for s in [(-qb - root) / (2.0 * qa), (-qb + root) / (2.0 * qa)] {
        if !(s >= -ms && s <= 1.0 + ms) {
            continue;
        }
        let s_in = s.clamp(0.0, 1.0);
        let pt = [p[0] + s_in * d[0], p[1] + s_in * d[1]];
        let Some(tc) = n.param_of(pt) else {
            continue; // on the conic's circle but outside its arc
        };
        let ts = t0 + dt * s_in;
        out.push(if swapped {
            Crossing2 { a: tc, b: ts }
        } else {
            Crossing2 { a: ts, b: tc }
        });
    }
    tangency
}

/// How many samples a conic piece's own parameter span earns.
fn conic_samples(piece: &Piece) -> usize {
    let Piece::Conic { t0, t1, .. } = *piece else {
        return 0;
    };
    let span = (t1 - t0).abs();
    (((span / TAU) * CONIC_SAMPLES_PER_TURN).ceil() as usize).clamp(16, 1024)
}

/// Two conics: bracket the STRICT sign changes of the second's implicit
/// function along the first and bisect. See the module docs for why this is
/// not a quartic solve.
///
/// "Strict" is load-bearing, and so is the separate pass over the samples that
/// land ON the other conic. A tangency puts an exact zero in the sample array,
/// and a test that counts a zero as one side of a sign change reports TWO
/// spurious transversal roots around it — measured 2026-10-03 on two
/// internally tangent circles, where the tangency sits exactly on the sample
/// at `t = 0`. So a zero never brackets; instead a sample inside the band is a
/// CONTACT, and the signs on either side of it say which kind: opposite signs
/// are a transversal root that happened to land on a sample, equal signs are a
/// tangency, which is declined.
fn conic_conic(pa: &Piece, pb: &Piece, band: f64, out: &mut Vec<Crossing2>) -> u32 {
    let Piece::Conic { t0: a0, t1: a1, .. } = *pa else {
        return 0;
    };
    let Some(nb) = Normalized::of(pb, band) else {
        return 0;
    };
    let n = conic_samples(pa);
    // A closed conic is sampled CYCLICALLY, so a contact sitting on its own
    // parameter seam is one contact and not one at each end of the window.
    let closed = (a1 - a0).abs() >= TAU - 1e-12;
    let count = if closed { n } else { n + 1 };
    let step = (a1 - a0) / n as f64;
    let at = |i: usize| a0 + step * i as f64;
    let f = |t: f64| nb.implicit(pa.eval(t));
    let vals: Vec<f64> = (0..count).map(|i| f(at(i))).collect();
    // The same conic twice: no crossing, no tangency, nothing to split.
    if vals
        .iter()
        .all(|v| v.is_finite() && v.abs() <= COINCIDENT_IMPLICIT)
    {
        return 0;
    }
    // The band in the normalized frame, as an implicit value: `|q|/2` is the
    // normalized distance to the conic, to first order.
    let graze = 2.0 * nb.band_scale;

    let push = |t: f64, out: &mut Vec<Crossing2>| {
        if let Some(tb) = nb.param_of(pa.eval(t)) {
            out.push(Crossing2 { a: t, b: tb });
        }
    };

    // 1. Strict sign changes between neighbouring samples: a transversal root
    //    strictly between them, bisected to float precision.
    let pairs = if closed { count } else { count - 1 };
    for i in 0..pairs {
        let j = (i + 1) % count;
        let (fa, fb) = (vals[i], vals[j]);
        if !(fa.is_finite() && fb.is_finite()) || fa * fb >= 0.0 {
            continue;
        }
        let (mut lo, mut hi) = (at(i), at(i + 1));
        let mut flo = fa;
        for _ in 0..BISECT_STEPS {
            let mid = 0.5 * (lo + hi);
            let fm = f(mid);
            if (flo <= 0.0) == (fm <= 0.0) {
                lo = mid;
                flo = fm;
            } else {
                hi = mid;
            }
        }
        push(0.5 * (lo + hi), out);
    }

    // 2. Samples that land ON the other conic. An open conic's own endpoints
    //    are skipped: a contact exactly at an arc's end has only one side to
    //    read, and under-reporting a decline there is the safe direction.
    let mut tangencies = 0u32;
    let first = usize::from(!closed);
    let last = if closed { count } else { count - 1 };
    for i in first..last {
        if !(vals[i].is_finite() && vals[i].abs() <= graze) {
            continue;
        }
        let prev = vals[(i + count - 1) % count];
        let next = vals[(i + 1) % count];
        // Only the lowest sample of a run inside the band speaks for it.
        if !(vals[i].abs() <= prev.abs() && vals[i].abs() <= next.abs()) {
            continue;
        }
        if prev.is_finite() && next.is_finite() && prev * next < 0.0 {
            // A transversal root that landed on a sample: step 1 could not
            // bracket it, because neither of its two pairs changed sign
            // strictly.
            push(at(i), out);
        } else {
            tangencies += 1;
        }
    }
    tangencies
}

/// The parameters at which a curve's own projection FOLDS — where its 2-D
/// tangent reverses, so the 3-D curve is momentarily parallel to the line of
/// sight and the pieces on either side can differ in visibility.
///
/// Only a [`Curve2::Polyline`] can carry one: a line's projection is a line,
/// and a projected circle or ellipse has a non-vanishing derivative
/// everywhere its minor radius is positive — the fold of a circle seen EDGE ON
/// is why `project_circle` reports that case as a segment rather than a
/// degenerate ellipse, and the fold is then not on this curve but inside it
/// (its two halves project onto the same segment, and `visibility` lifts such
/// a point to its NEAREST pre-image, which is the half a drawing shows).
pub(crate) fn folds(curve: &Curve2) -> Vec<f64> {
    let Curve2::Polyline { points, closed } = curve else {
        return Vec::new();
    };
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }
    let chords = if *closed { n } else { n - 1 };
    let dir = |i: usize| -> [f64; 2] {
        let a = points[i % n];
        let b = points[(i + 1) % n];
        [b.x() - a.x(), b.y() - a.y()]
    };
    let mut out = Vec::new();
    let first = usize::from(!*closed);
    for i in first..chords {
        let (p, q) = (dir((i + chords - 1) % chords), dir(i));
        let dot = p[0] * q[0] + p[1] * q[1];
        let (lp, lq) = (
            (p[0] * p[0] + p[1] * p[1]).sqrt(),
            (q[0] * q[0] + q[1] * q[1]).sqrt(),
        );
        if lp > 0.0 && lq > 0.0 && dot < 0.0 {
            // A reversal: the projected direction turned by more than 90°,
            // which a chord-refined sampling of a smooth curve only does at a
            // fold.
            out.push(i as f64);
        }
    }
    out
}

/// Distance from `p` to the segment `a → b` in the view plane.
pub(crate) fn point_segment_distance(p: Point2, a: Point2, b: Point2) -> f64 {
    let (vx, vy) = (b.x() - a.x(), b.y() - a.y());
    let len2 = vx * vx + vy * vy;
    let t = if len2 <= 0.0 {
        0.0
    } else {
        (((p.x() - a.x()) * vx + (p.y() - a.y()) * vy) / len2).clamp(0.0, 1.0)
    };
    (p.x() - (a.x() + t * vx)).hypot(p.y() - (a.y() + t * vy))
}

#[cfg(test)]
mod tests;
