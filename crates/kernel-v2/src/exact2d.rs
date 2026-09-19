//! Shared EXACT 2D predicates over `dashu` rationals (crate-internal).
//!
//! Every finite `f64` converts losslessly to `RBig`, so the sign
//! evaluations here are *decision procedures*, not approximations — the
//! same rationale documented in [`crate::profile`]'s simplicity-validation
//! decision (which originally housed these predicates; PR-KV3 promoted
//! them to a shared module so the tessellation ear-clipping pass uses the
//! identical exact arithmetic instead of duplicating it).
//!
//! All callers guarantee coordinate finiteness before calling (profile
//! validation checks it; tessellation operates on validated solids whose
//! geometry came through those same constructors or through the exact
//! boolean pipeline), so the `f64 → RBig` conversions are total.

use cad_primitives::Point2;
use dashu::float::FBig;
use dashu::rational::RBig;
use std::cmp::Ordering;

/// Lossless f64 → rational. Total for finite input (pre-checked).
pub(crate) fn r(x: f64) -> RBig {
    let fb: FBig = FBig::try_from(x).expect("finite f64 → FBig is total");
    RBig::try_from(fb).expect("FBig → RBig is total")
}

/// Shewchuk's static forward-error bound for the 2D orientation
/// determinant (`ccwerrboundA = (3 + 16ε)·ε`, ε = 2⁻⁵³): when the f64
/// determinant's magnitude exceeds this fraction of the term-magnitude
/// sum, its SIGN is provably the exact sign. (Shewchuk 1997, "Adaptive
/// Precision Floating-Point Arithmetic", §4.2 — the same filtered→exact
/// cascade structure as cherchi-rs's predicates.)
const ORIENT2D_ERRBOUND_A: f64 = (3.0 + 16.0 * f64::EPSILON) * f64::EPSILON;

/// Orientation of `c` relative to the directed line `a → b`:
/// `Greater` = left, `Less` = right, `Equal` = collinear.
///
/// EXACT decision procedure with a filtered fast path (PR-KV5b): the f64
/// determinant decides when it clears Shewchuk's static error bound —
/// provably the same sign the rational evaluation would produce — and
/// everything inside the bound falls through to the lossless `RBig`
/// evaluation. Same decisions as the original all-rational form,
/// byte-for-byte; the filter exists because the KV5b cylinder-patch
/// ear-clipping runs O(n³) orientation tests on ~10²-node rings, which the
/// all-rational form made pathologically slow in debug builds.
pub(crate) fn orient2d(a: Point2, b: Point2, c: Point2) -> Ordering {
    let detleft = (b.x() - a.x()) * (c.y() - a.y());
    let detright = (b.y() - a.y()) * (c.x() - a.x());
    let det = detleft - detright;
    let detsum = detleft.abs() + detright.abs();
    if det.is_finite() && detsum.is_finite() {
        let bound = ORIENT2D_ERRBOUND_A * detsum;
        if det > bound {
            return Ordering::Greater;
        }
        if det < -bound {
            return Ordering::Less;
        }
    }
    let det = (r(b.x()) - r(a.x())) * (r(c.y()) - r(a.y()))
        - (r(b.y()) - r(a.y())) * (r(c.x()) - r(a.x()));
    det.cmp(&RBig::ZERO)
}

/// Spike test at vertex `b` of the path `a → b → c`: the incident edges
/// are collinear AND `c` heads back toward `a` (exact dot > 0).
pub(crate) fn doubles_back(a: Point2, b: Point2, c: Point2) -> bool {
    if orient2d(a, b, c) != Ordering::Equal {
        return false;
    }
    let dot = (r(c.x()) - r(b.x())) * (r(a.x()) - r(b.x()))
        + (r(c.y()) - r(b.y())) * (r(a.y()) - r(b.y()));
    dot.cmp(&RBig::ZERO) == Ordering::Greater
}

/// `q` is known collinear with `a`–`b`; is it within the segment's
/// closed bounding box? (f64 comparisons are exact — no arithmetic.)
pub(crate) fn on_collinear_segment(a: Point2, b: Point2, q: Point2) -> bool {
    q.x() >= a.x().min(b.x())
        && q.x() <= a.x().max(b.x())
        && q.y() >= a.y().min(b.y())
        && q.y() <= a.y().max(b.y())
}

/// Do CLOSED segments `p1p2` and `p3p4` share any point (proper
/// crossing, endpoint touch, or collinear overlap)? Exact.
pub(crate) fn closed_segments_intersect(p1: Point2, p2: Point2, p3: Point2, p4: Point2) -> bool {
    let d1 = orient2d(p3, p4, p1);
    let d2 = orient2d(p3, p4, p2);
    let d3 = orient2d(p1, p2, p3);
    let d4 = orient2d(p1, p2, p4);
    if ((d1 == Ordering::Greater && d2 == Ordering::Less)
        || (d1 == Ordering::Less && d2 == Ordering::Greater))
        && ((d3 == Ordering::Greater && d4 == Ordering::Less)
            || (d3 == Ordering::Less && d4 == Ordering::Greater))
    {
        return true; // proper crossing
    }
    (d1 == Ordering::Equal && on_collinear_segment(p3, p4, p1))
        || (d2 == Ordering::Equal && on_collinear_segment(p3, p4, p2))
        || (d3 == Ordering::Equal && on_collinear_segment(p1, p2, p3))
        || (d4 == Ordering::Equal && on_collinear_segment(p1, p2, p4))
}

/// Is the candidate hole bridge `p → h` blocked by boundary edge `a → b`?
///
/// Like [`closed_segments_intersect`] but a touch EXACTLY at a shared
/// endpoint (an edge incident to the bridge's own endpoints, by exact
/// coordinate equality) does not block — every boundary edge incident to
/// `p` or `h` necessarily touches the bridge there. Blocking conditions:
/// proper crossing; a non-shared edge endpoint anywhere on the closed
/// bridge; a bridge endpoint strictly interior to the edge; an edge that
/// coincides with the bridge entirely (both endpoints shared) — which also
/// subsumes collinear overlap, since an overlap beyond a shared endpoint
/// always puts some endpoint inside the other segment.
pub(crate) fn bridge_blocked_by(p: Point2, h: Point2, a: Point2, b: Point2) -> bool {
    let shared_a = a == p || a == h;
    let shared_b = b == p || b == h;
    if shared_a && shared_b {
        return true; // edge coincides with the bridge
    }
    let d1 = orient2d(p, h, a);
    let d2 = orient2d(p, h, b);
    let d3 = orient2d(a, b, p);
    let d4 = orient2d(a, b, h);
    if ((d1 == Ordering::Greater && d2 == Ordering::Less)
        || (d1 == Ordering::Less && d2 == Ordering::Greater))
        && ((d3 == Ordering::Greater && d4 == Ordering::Less)
            || (d3 == Ordering::Less && d4 == Ordering::Greater))
    {
        return true; // proper crossing
    }
    // Non-shared edge endpoint on the closed bridge.
    if !shared_a && d1 == Ordering::Equal && on_collinear_segment(p, h, a) {
        return true;
    }
    if !shared_b && d2 == Ordering::Equal && on_collinear_segment(p, h, b) {
        return true;
    }
    // Bridge endpoint strictly interior to the edge.
    if d3 == Ordering::Equal && on_collinear_segment(a, b, p) && p != a && p != b {
        return true;
    }
    if d4 == Ordering::Equal && on_collinear_segment(a, b, h) && h != a && h != b {
        return true;
    }
    false
}

/// Exact sign of the loop's shoelace sum (`Greater` = CCW).
pub(crate) fn signed_area_sign(pts: &[Point2]) -> Ordering {
    let mut sum = RBig::ZERO;
    let n = pts.len();
    for i in 0..n {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        sum += r(p.x()) * r(q.y()) - r(q.x()) * r(p.y());
    }
    sum.cmp(&RBig::ZERO)
}

/// Exact crossing-parity point-in-polygon, STRICT interior. The caller
/// guarantees `q` does not lie on the boundary (loop disjointness is
/// established first), which rules out the `orient == Equal` crossing
/// ambiguity: a straddling edge with `q` on its supporting line would
/// put `q` on the segment itself.
pub(crate) fn point_strictly_inside(q: Point2, pts: &[Point2]) -> bool {
    let mut inside = false;
    let n = pts.len();
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        // Half-open straddle test (exact f64 comparisons).
        if (a.y() > q.y()) == (b.y() > q.y()) {
            continue;
        }
        let upward = b.y() > a.y();
        let o = orient2d(a, b, q);
        // Ray +x from q crosses an upward edge iff q is strictly left
        // of it, a downward edge iff strictly right.
        if (upward && o == Ordering::Greater) || (!upward && o == Ordering::Less) {
            inside = !inside;
        }
    }
    inside
}

// =========================================================================
// Exact arc predicates (PR-KV12 Tier 2, increment E3)
// =========================================================================
//
// `Profile::arc_polygon` must reject a self-intersecting line/arc boundary
// EXACTLY. The intersection points of a line with a circle, or of two
// circles, are degree-2 algebraic numbers — generally NOT rational — so we
// never COMPUTE them. Instead each candidate point is a root of a rational
// quadratic in a 1-D parameter (segment `t`, or the radical-line parameter
// `u`), and every membership constraint is LINEAR in that parameter:
//
// - segment interior:  `t ∈ (0, 1)`;
// - arc interior (MINOR arc only, which every Tier-2 arc is): the point lies
//   strictly on the OPPOSITE side of the chord `ab` from the centre `c` —
//   and `orient(a, b, ·)` is affine in the point, hence affine in the
//   parameter.
//
// So the decision reduces to: "does a root of `A·x² + B·x + C` satisfy a set
// of strict linear sign constraints", which is settled by an exact
// compare-root-against-rational predicate. All arithmetic is `RBig`; the
// f64 → RBig conversion is lossless, so this is a decision procedure.
//
// THE CIRCLE AN ARC STANDS FOR (2026-09-19, found by the ISO 606 sprocket's
// flank/tip corners): an arc arrives as f64 endpoints `a`, `b` plus an f64
// `centre` and `radius`, and in general NO rational circle passes through
// all of that — `|a − centre|² ≠ radius²` by rounding. Lifting the authored
// centre/radius verbatim made two arcs that meet transversally at a shared
// vertex `J` cross, exactly, at a point a few ulps AWAY from `J` (the
// rounded circles cross near `J`, not at it), and that phantom point lay
// inside both open arcs about half the time — a valid loop was rejected as
// non-simple by the luck of rounding. So every predicate here lifts an arc
// to [`exact_circle`]: the rational circle through BOTH endpoints whose
// centre is the point of the chord's perpendicular bisector nearest the
// authored centre. The endpoints are then exactly on the circle, a shared
// vertex is exactly on both adjacent circles, and the only crossing at a
// corner is the corner itself (excluded as the permitted junction). An
// authored centre already on the bisector (every integer-coordinate test
// fixture) lifts to itself, so the decision on those inputs is unchanged.

/// Sign of a rational as `-1 / 0 / +1`.
fn sgn(x: &RBig) -> i32 {
    match x.cmp(&RBig::ZERO) {
        Ordering::Greater => 1,
        Ordering::Less => -1,
        Ordering::Equal => 0,
    }
}

/// Exact rational orientation determinant of `c` wrt directed line `a → b`
/// (`> 0` left, `< 0` right, `0` collinear).
fn orient_det(a: Point2, b: Point2, c: Point2) -> RBig {
    (r(b.x()) - r(a.x())) * (r(c.y()) - r(a.y())) - (r(b.y()) - r(a.y())) * (r(c.x()) - r(a.x()))
}

/// [`orient_det`] with a rational query point `(qx, qy)`.
fn orient_det_rq(a: Point2, b: Point2, qx: &RBig, qy: &RBig) -> RBig {
    (r(b.x()) - r(a.x())) * (qy.clone() - r(a.y()))
        - (r(b.y()) - r(a.y())) * (qx.clone() - r(a.x()))
}

/// The rational circle a Tier-2 arc stands for (see the module notes): it
/// passes EXACTLY through both endpoints; its centre is the point of the
/// chord's perpendicular bisector nearest the authored `centre`.
pub(crate) struct ExactCircle {
    pub cx: RBig,
    pub cy: RBig,
    /// Squared radius, `|a − c|² = |b − c|²` exactly.
    pub r2: RBig,
}

/// Lift an authored arc (`a`, `b`, `centre`) to its [`ExactCircle`]. The
/// caller guarantees `a != b` (validated upstream), so the bisector is a
/// line and the projection is total.
pub(crate) fn exact_circle(a: Point2, b: Point2, centre: Point2) -> ExactCircle {
    let (ax, ay, bx, by) = (r(a.x()), r(a.y()), r(b.x()), r(b.y()));
    let half = RBig::ONE / r(2.0);
    let mx = (ax.clone() + bx.clone()) * half.clone();
    let my = (ay.clone() + by.clone()) * half;
    // Bisector direction n ⊥ chord.
    let nx = -(by - ay.clone());
    let ny = bx - ax.clone();
    let nn = nx.clone() * nx.clone() + ny.clone() * ny.clone();
    let t = ((r(centre.x()) - mx.clone()) * nx.clone() + (r(centre.y()) - my.clone()) * ny.clone())
        / nn;
    let cx = mx + t.clone() * nx;
    let cy = my + t * ny;
    let dx = ax - cx.clone();
    let dy = ay - cy.clone();
    let r2 = dx.clone() * dx + dy.clone() * dy;
    ExactCircle { cx, cy, r2 }
}

/// Side of chord `a → b` on which the exact circle's centre lies
/// (`+1` left, `−1` right; never `0` for a validated minor arc).
fn centre_side(a: Point2, b: Point2, circle: &ExactCircle) -> i32 {
    sgn(&orient_det_rq(a, b, &circle.cx, &circle.cy))
}

/// Compare the quadratic root `x_s = (−B + s·√Δ) / (2A)` (with `A > 0`,
/// `Δ = B² − 4AC ≥ 0`, `s = ±1`) against a rational `c`, EXACTLY.
///
/// `x_s − c = (s√Δ − L)/(2A)` with `L = 2Ac + B`, and `2A > 0`, so the
/// ordering of `x_s` vs `c` equals the ordering of `s√Δ` vs `L`, decided by
/// comparing `Δ` against `L²` with the correct sign casework (no irrational
/// arithmetic).
fn cmp_root(a: &RBig, b: &RBig, disc: &RBig, s: i32, c: &RBig) -> Ordering {
    let l = r(2.0) * a.clone() * c.clone() + b.clone();
    let l2 = l.clone() * l.clone();
    if s > 0 {
        // compare √Δ vs L
        if l < RBig::ZERO {
            Ordering::Greater // √Δ ≥ 0 > L
        } else {
            disc.cmp(&l2) // both ≥ 0 ⇒ √Δ vs L ⟺ Δ vs L²
        }
    } else {
        // compare −√Δ vs L
        if l > RBig::ZERO {
            Ordering::Less // −√Δ ≤ 0 < L
        } else {
            // both ≤ 0: −√Δ vs L ⟺ reverse(√Δ vs −L) ⟺ reverse(Δ vs L²)
            disc.cmp(&l2).reverse()
        }
    }
}

/// Does the quadratic root selected by `s` satisfy the strict linear sign
/// constraint `sign(p + q·x) == target` (`target ∈ {−1, +1}`)? When `q == 0`
/// the constraint is constant; otherwise the sign flips at `x = −p/q`, and
/// the root's side of that rational threshold is decided exactly by
/// [`cmp_root`]. A root landing exactly on the threshold (`p + q·x = 0`)
/// fails a STRICT constraint.
#[allow(clippy::too_many_arguments)]
fn linear_side_ok(
    a: &RBig,
    b: &RBig,
    disc: &RBig,
    s: i32,
    p: &RBig,
    q: &RBig,
    target: i32,
) -> bool {
    if sgn(q) == 0 {
        return sgn(p) == target;
    }
    let tstar = -(p.clone()) / q.clone();
    match cmp_root(a, b, disc, s, &tstar) {
        Ordering::Equal => false,
        Ordering::Greater => sgn(q) == target, // x > −p/q ⇒ sign(p+qx) = sign(q)
        Ordering::Less => -sgn(q) == target,
    }
}

/// Is the root selected by `s` strictly inside the open interval `(lo, hi)`?
fn root_in_open(a: &RBig, b: &RBig, disc: &RBig, s: i32, lo: &RBig, hi: &RBig) -> bool {
    cmp_root(a, b, disc, s, lo) == Ordering::Greater
        && cmp_root(a, b, disc, s, hi) == Ordering::Less
}

/// Do segments `p1p2` and `p3p4` cross at a point strictly interior to BOTH
/// (a proper crossing — collinear overlap and endpoint touches are excluded;
/// the caller handles those via endpoint-incidence tests)?
pub(crate) fn segments_properly_cross(p1: Point2, p2: Point2, p3: Point2, p4: Point2) -> bool {
    let d1 = orient2d(p3, p4, p1);
    let d2 = orient2d(p3, p4, p2);
    let d3 = orient2d(p1, p2, p3);
    let d4 = orient2d(p1, p2, p4);
    ((d1 == Ordering::Greater && d2 == Ordering::Less)
        || (d1 == Ordering::Less && d2 == Ordering::Greater))
        && ((d3 == Ordering::Greater && d4 == Ordering::Less)
            || (d3 == Ordering::Less && d4 == Ordering::Greater))
}

/// Is the point `v` exactly on the circle?
fn on_circle(v: Point2, circle: &ExactCircle) -> bool {
    let dx = r(v.x()) - circle.cx.clone();
    let dy = r(v.y()) - circle.cy.clone();
    (dx.clone() * dx + dy.clone() * dy).cmp(&circle.r2) == Ordering::Equal
}

/// Is point `v` on the CLOSED minor arc from `a` to `b` about `centre` of
/// `radius`? (Endpoints by exact equality; otherwise exactly on the circle
/// AND strictly on the far side of chord `ab` from the centre — the
/// minor-arc characterisation. The centre is never on `ab` for a minor arc,
/// so its side sign is nonzero.)
pub(crate) fn point_on_closed_arc(
    v: Point2,
    a: Point2,
    b: Point2,
    centre: Point2,
    radius: f64,
) -> bool {
    if v == a || v == b {
        return true;
    }
    let _ = radius; // the exact circle is fixed by the endpoints (module notes)
    let circle = exact_circle(a, b, centre);
    if !on_circle(v, &circle) {
        return false;
    }
    let side_c = centre_side(a, b, &circle);
    sgn(&orient_det(a, b, v)) == -side_c
}

/// Do the minor arc `(a→b about centre, radius)` and the segment `p→q` cross
/// at a point strictly interior to BOTH (open arc, open segment)? Exact.
#[allow(clippy::too_many_arguments)]
pub(crate) fn arc_segment_interior_cross(
    a: Point2,
    b: Point2,
    centre: Point2,
    radius: f64,
    p: Point2,
    q: Point2,
) -> bool {
    // Segment X(t) = p + t·(q − p); circle |X − c|² = r² (the exact circle
    // through a and b).
    let _ = radius;
    let circle = exact_circle(a, b, centre);
    let (dx, dy) = (r(q.x()) - r(p.x()), r(q.y()) - r(p.y()));
    let (ex, ey) = (r(p.x()) - circle.cx.clone(), r(p.y()) - circle.cy.clone());
    let aa = dx.clone() * dx.clone() + dy.clone() * dy.clone(); // |D|² > 0
    let bb = r(2.0) * (ex.clone() * dx.clone() + ey.clone() * dy.clone());
    let cc = ex.clone() * ex.clone() + ey.clone() * ey.clone() - circle.r2.clone();
    let disc = bb.clone() * bb.clone() - r(4.0) * aa.clone() * cc.clone();
    if disc < RBig::ZERO {
        return false;
    }
    // Arc-side: target sign = opposite of the centre's side of chord ab.
    let target = -centre_side(a, b, &circle);
    // g(t) = orient(a, b, X(t)) = g0 + g1·t (affine).
    let g0 = orient_det(a, b, p);
    let g1 = orient_det(a, b, q) - g0.clone();
    let (zero, one) = (RBig::ZERO, RBig::ONE);
    for s in [1, -1] {
        // Segment interior t ∈ (0, 1).
        if !root_in_open(&aa, &bb, &disc, s, &zero, &one) {
            continue;
        }
        if linear_side_ok(&aa, &bb, &disc, s, &g0, &g1, target) {
            return true;
        }
    }
    false
}

/// Count transversal crossings of the minor arc `(a→b about centre, radius)`
/// with the OPEN segment `p→q` (the same geometry as
/// [`arc_segment_interior_cross`] but returning 0/1/2 instead of a bool — for
/// ray-cast point-in-region parity). Returns `None` if the segment is exactly
/// TANGENT to the supporting circle at a qualifying point (a measure-zero
/// degeneracy that would corrupt the parity); the caller retries with another
/// ray/witness.
#[allow(clippy::too_many_arguments)]
pub(crate) fn arc_segment_interior_crossings(
    a: Point2,
    b: Point2,
    centre: Point2,
    radius: f64,
    p: Point2,
    q: Point2,
) -> Option<usize> {
    let _ = radius;
    let circle = exact_circle(a, b, centre);
    let (dx, dy) = (r(q.x()) - r(p.x()), r(q.y()) - r(p.y()));
    let (ex, ey) = (r(p.x()) - circle.cx.clone(), r(p.y()) - circle.cy.clone());
    let aa = dx.clone() * dx.clone() + dy.clone() * dy.clone();
    let bb = r(2.0) * (ex.clone() * dx.clone() + ey.clone() * dy.clone());
    let cc = ex.clone() * ex.clone() + ey.clone() * ey.clone() - circle.r2.clone();
    let disc = bb.clone() * bb.clone() - r(4.0) * aa.clone() * cc.clone();
    if disc < RBig::ZERO {
        return Some(0);
    }
    let target = -centre_side(a, b, &circle);
    let g0 = orient_det(a, b, p);
    let g1 = orient_det(a, b, q) - g0.clone();
    let (zero, one) = (RBig::ZERO, RBig::ONE);
    let qualifies = |s: i32| -> bool {
        root_in_open(&aa, &bb, &disc, s, &zero, &one)
            && linear_side_ok(&aa, &bb, &disc, s, &g0, &g1, target)
    };
    if disc == RBig::ZERO {
        // Tangent double root: a touch, not a crossing — degenerate for parity.
        return if qualifies(1) { None } else { Some(0) };
    }
    Some([1, -1].into_iter().filter(|&s| qualifies(s)).count())
}

/// Do two minor arcs cross at a point strictly interior to BOTH? Exact.
///
/// The common points of the two supporting circles lie on their radical
/// line `n·X = d` (`n = 2(c2 − c1)`, rational). Parametrising that line and
/// intersecting with circle 1 gives a rational quadratic in `u`; each root's
/// membership in BOTH open arcs is a pair of strict linear sign constraints.
/// Concentric circles (`n = 0`) share no radical line — cocircular-overlap
/// is a measure-zero degeneracy the Tier-2 adapter inputs never produce, and
/// is reported as "no transversal crossing" here (documented gap).
#[allow(clippy::too_many_arguments)]
pub(crate) fn arc_arc_interior_cross(
    a1: Point2,
    b1: Point2,
    c1: Point2,
    r1: f64,
    a2: Point2,
    b2: Point2,
    c2: Point2,
    r2: f64,
) -> bool {
    let _ = (r1, r2);
    let circle1 = exact_circle(a1, b1, c1);
    let circle2 = exact_circle(a2, b2, c2);
    let (c1x, c1y) = (circle1.cx.clone(), circle1.cy.clone());
    let (c2x, c2y) = (circle2.cx.clone(), circle2.cy.clone());
    let nx = r(2.0) * (c2x.clone() - c1x.clone());
    let ny = r(2.0) * (c2y.clone() - c1y.clone());
    let nn = nx.clone() * nx.clone() + ny.clone() * ny.clone();
    if nn == RBig::ZERO {
        return false; // concentric (see doc comment)
    }
    let r1sq = circle1.r2.clone();
    let r2sq = circle2.r2.clone();
    // Radical line n·X = d, d = (|c2|² − r2²) − (|c1|² − r1²).
    let d = (c2x.clone() * c2x.clone() + c2y.clone() * c2y.clone() - r2sq.clone())
        - (c1x.clone() * c1x.clone() + c1y.clone() * c1y.clone() - r1sq.clone());
    // Foot X0 = (d/|n|²)·n on the radical line; direction τ = (−ny, nx).
    let x0x = d.clone() / nn.clone() * nx.clone();
    let x0y = d.clone() / nn.clone() * ny.clone();
    let (tx, ty) = (-ny.clone(), nx.clone());
    // Circle 1: |X0 + u·τ − c1|² = r1².
    let (wx, wy) = (x0x.clone() - c1x.clone(), x0y.clone() - c1y.clone());
    let aa = tx.clone() * tx.clone() + ty.clone() * ty.clone(); // |τ|² = nn > 0
    let bb = r(2.0) * (wx.clone() * tx.clone() + wy.clone() * ty.clone());
    let cc = wx.clone() * wx.clone() + wy.clone() * wy.clone() - r1sq;
    let disc = bb.clone() * bb.clone() - r(4.0) * aa.clone() * cc.clone();
    if disc < RBig::ZERO {
        return false;
    }
    // Arc-side constraints: g_k(u) = orient(a_k, b_k, X0 + u·τ) = p_k + q_k·u.
    let p1 = orient_det_rq(a1, b1, &x0x, &x0y);
    let q1 = orient_det_rq(
        a1,
        b1,
        &(x0x.clone() + tx.clone()),
        &(x0y.clone() + ty.clone()),
    ) - p1.clone();
    let target1 = -centre_side(a1, b1, &circle1);
    let p2 = orient_det_rq(a2, b2, &x0x, &x0y);
    let q2 = orient_det_rq(
        a2,
        b2,
        &(x0x.clone() + tx.clone()),
        &(x0y.clone() + ty.clone()),
    ) - p2.clone();
    let target2 = -centre_side(a2, b2, &circle2);
    for s in [1, -1] {
        if linear_side_ok(&aa, &bb, &disc, s, &p1, &q1, target1)
            && linear_side_ok(&aa, &bb, &disc, s, &p2, &q2, target2)
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod arc_predicate_tests {
    use super::*;
    use cad_primitives::Point2;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    /// A point at polar angle `deg` on the circle (centre, radius).
    fn on(cx: f64, cy: f64, radius: f64, deg: f64) -> Point2 {
        let t = deg.to_radians();
        p(cx + radius * t.cos(), cy + radius * t.sin())
    }

    // ---- arc ∩ segment ----------------------------------------------------

    #[test]
    fn arc_segment_crossing_and_misses() {
        // Circle (0,0) r=5; minor arc a=(4,3) → b=(3,4) (arc side x+y > 7).
        let (a, b, c) = (p(4.0, 3.0), p(3.0, 4.0), p(0.0, 0.0));
        // The y=x diagonal pierces the arc at (5/√2, 5/√2), interior to both.
        assert!(arc_segment_interior_cross(
            a,
            b,
            c,
            5.0,
            p(0.0, 0.0),
            p(10.0, 10.0)
        ));
        // Same line, opposite ray: hits the circle on the far (centre) side,
        // NOT on the arc.
        assert!(!arc_segment_interior_cross(
            a,
            b,
            c,
            5.0,
            p(0.0, 0.0),
            p(-10.0, -10.0)
        ));
        // A segment that never reaches the circle.
        assert!(!arc_segment_interior_cross(
            a,
            b,
            c,
            5.0,
            p(6.0, 0.0),
            p(6.0, 10.0)
        ));
        // A segment crossing the supporting circle but only at the arc's
        // endpoint (t = 1, segment-open excludes it).
        assert!(!arc_segment_interior_cross(
            a,
            b,
            c,
            5.0,
            p(0.0, 0.0),
            p(4.0, 3.0)
        ));
    }

    #[test]
    fn arc_segment_adversarial_near_touch() {
        // Wide minor arc a=(4,3) (≈36.87°) → b=(0,5) (90°) on circle r=5.
        let (a, b, c) = (p(4.0, 3.0), p(0.0, 5.0), p(0.0, 0.0));
        // A ray from the origin whose circle hit is JUST BELOW a's angle is
        // outside the arc; JUST ABOVE is inside. Decided exactly on the
        // rational inputs — no tolerance.
        assert!(!arc_segment_interior_cross(
            a,
            b,
            c,
            5.0,
            p(0.0, 0.0),
            p(80.0, 59.0)
        ));
        assert!(arc_segment_interior_cross(
            a,
            b,
            c,
            5.0,
            p(0.0, 0.0),
            p(80.0, 61.0)
        ));
    }

    // ---- arc ∩ arc --------------------------------------------------------

    #[test]
    fn arc_arc_crossing_and_disjoint() {
        // Circle1 (0,0) and Circle2 (2,0), both r=√2, meet at (1,1),(1,-1).
        let r = 2.0_f64.sqrt();
        // arc1 = the RIGHT side of circle1 (brackets ±45°), arc2 = the LEFT
        // side of circle2: both contain (1,±1) interiors ⇒ they cross.
        let (a1, b1) = (on(0.0, 0.0, r, 80.0), on(0.0, 0.0, r, -80.0));
        let (a2, b2) = (on(2.0, 0.0, r, 100.0), on(2.0, 0.0, r, -100.0));
        assert!(arc_arc_interior_cross(
            a1,
            b1,
            p(0.0, 0.0),
            r,
            a2,
            b2,
            p(2.0, 0.0),
            r
        ));
        // arc2' = the RIGHT side of circle2 (away from the origin): the
        // common points (1,±1) are NOT on it ⇒ no interior crossing.
        let (a2r, b2r) = (on(2.0, 0.0, r, 80.0), on(2.0, 0.0, r, -80.0));
        assert!(!arc_arc_interior_cross(
            a1,
            b1,
            p(0.0, 0.0),
            r,
            a2r,
            b2r,
            p(2.0, 0.0),
            r
        ));
    }

    #[test]
    fn arc_arc_concentric_is_no_crossing() {
        // Concentric circles share no radical line; the predicate reports no
        // transversal crossing (documented measure-zero gap).
        let (a1, b1) = (on(0.0, 0.0, 2.0, 10.0), on(0.0, 0.0, 2.0, 80.0));
        let (a2, b2) = (on(0.0, 0.0, 3.0, 10.0), on(0.0, 0.0, 3.0, 80.0));
        assert!(!arc_arc_interior_cross(
            a1,
            b1,
            p(0.0, 0.0),
            2.0,
            a2,
            b2,
            p(0.0, 0.0),
            3.0
        ));
    }

    /// Two arcs meeting transversally at a shared f64 vertex — an ISO 606
    /// sprocket's flank arc (9 teeth, 08B chain) running into its tip arc,
    /// coordinates as the generator emitted them. Lifting the authored
    /// centre/radius verbatim put the rounded circles' crossing a few ulps
    /// off the corner and inside both open arcs: a phantom self-intersection.
    /// The endpoint-exact circle puts the crossing AT the corner.
    #[test]
    fn transversal_corner_is_not_a_crossing() {
        let corner = p(0.010979405354598229, 0.017772566589039057);
        // Flank: seat end → corner, centre off to the left, r = re.
        let (fa, fb, fc, fr) = (
            p(0.01011793776711934, 0.013428035533350745),
            corner,
            p(-0.00350858251460195, 0.018387683312522003),
            0.014501040000000003,
        );
        // Tip: corner → next flank's start, centred on the sprocket axis.
        let (ta, tb, tc, tr) = (
            corner,
            p(0.00990179147925926, 0.018394727250048485),
            p(0.0, 0.0),
            0.02089046349659116,
        );
        assert!(!arc_arc_interior_cross(fa, fb, fc, fr, ta, tb, tc, tr));
        assert!(!arc_arc_interior_cross(ta, tb, tc, tr, fa, fb, fc, fr));
        // Both endpoints are exactly on the lifted circle, whatever the
        // rounding of the authored centre.
        let c = exact_circle(fa, fb, fc);
        assert!(on_circle(fa, &c) && on_circle(fb, &c));
        // A centre already on the bisector lifts to itself.
        let c = exact_circle(p(4.0, 3.0), p(0.0, 5.0), p(0.0, 0.0));
        assert_eq!(c.cx, RBig::ZERO);
        assert_eq!(c.cy, RBig::ZERO);
        assert_eq!(c.r2, r(25.0));
    }

    // ---- closed-arc membership -------------------------------------------

    #[test]
    fn point_on_closed_arc_membership() {
        // Circle (0,0) r=5, minor arc (4,3) → (0,5).
        let (a, b, c) = (p(4.0, 3.0), p(0.0, 5.0), p(0.0, 0.0));
        assert!(point_on_closed_arc(a, a, b, c, 5.0)); // endpoint
        assert!(point_on_closed_arc(b, a, b, c, 5.0)); // endpoint
        assert!(point_on_closed_arc(p(3.0, 4.0), a, b, c, 5.0)); // interior (on circle, arc side)
        assert!(!point_on_closed_arc(p(5.0, 0.0), a, b, c, 5.0)); // on circle, wrong span
        assert!(!point_on_closed_arc(p(1.0, 1.0), a, b, c, 5.0)); // not on circle
    }
}
