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

/// A 2D point with exact rational coordinates (e.g. a bridge midpoint,
/// which is generally NOT representable in f64).
pub(crate) type RPoint2 = (RBig, RBig);

/// Lossless f64 → rational. Total for finite input (pre-checked).
pub(crate) fn r(x: f64) -> RBig {
    let fb: FBig = FBig::try_from(x).expect("finite f64 → FBig is total");
    RBig::try_from(fb).expect("FBig → RBig is total")
}

/// Exact rational midpoint of two points.
pub(crate) fn midpoint(a: Point2, b: Point2) -> RPoint2 {
    let half = RBig::from_parts(1.into(), 2u8.into());
    (
        (r(a.x()) + r(b.x())) * half.clone(),
        (r(a.y()) + r(b.y())) * half,
    )
}

/// Exact orientation of `c` relative to the directed line `a → b`:
/// `Greater` = left, `Less` = right, `Equal` = collinear.
pub(crate) fn orient2d(a: Point2, b: Point2, c: Point2) -> Ordering {
    let det = (r(b.x()) - r(a.x())) * (r(c.y()) - r(a.y()))
        - (r(b.y()) - r(a.y())) * (r(c.x()) - r(a.x()));
    det.cmp(&RBig::ZERO)
}

/// [`orient2d`] with a rational query point `q`.
pub(crate) fn orient2d_rq(a: Point2, b: Point2, q: &RPoint2) -> Ordering {
    let det = (r(b.x()) - r(a.x())) * (q.1.clone() - r(a.y()))
        - (r(b.y()) - r(a.y())) * (q.0.clone() - r(a.x()));
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
fn on_collinear_segment(a: Point2, b: Point2, q: Point2) -> bool {
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

/// [`point_strictly_inside`] with a rational query point. The caller
/// guarantees `q` is not on the boundary (the bridge-blocking pass already
/// rejected any boundary contact). Doubled (corridor) edges traversed once
/// in each direction toggle twice and cancel — correct, since they are not
/// an inside/outside boundary.
pub(crate) fn point_strictly_inside_rq(q: &RPoint2, pts: &[Point2]) -> bool {
    let mut inside = false;
    let n = pts.len();
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let a_above = r(a.y()) > q.1;
        let b_above = r(b.y()) > q.1;
        if a_above == b_above {
            continue;
        }
        let upward = b.y() > a.y();
        let o = orient2d_rq(a, b, q);
        if (upward && o == Ordering::Greater) || (!upward && o == Ordering::Less) {
            inside = !inside;
        }
    }
    inside
}

/// Is `q` inside or on the CLOSED triangle `a, b, c` (which must be CCW —
/// the ear-clipping caller checked `orient2d(a, b, c) == Greater`)?
pub(crate) fn inside_or_on_triangle(a: Point2, b: Point2, c: Point2, q: Point2) -> bool {
    orient2d(a, b, q) != Ordering::Less
        && orient2d(b, c, q) != Ordering::Less
        && orient2d(c, a, q) != Ordering::Less
}

/// Exact squared distance (for deterministic shortest-first bridge
/// candidate ordering).
pub(crate) fn squared_distance(a: Point2, b: Point2) -> RBig {
    let dx = r(b.x()) - r(a.x());
    let dy = r(b.y()) - r(a.y());
    dx.clone() * dx + dy.clone() * dy
}
