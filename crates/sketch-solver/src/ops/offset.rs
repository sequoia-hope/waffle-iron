//! Chain offset: parallel-curve construction for an ordered run of
//! line/arc segments, or a circle, at a signed distance.
//!
//! A port of `app/src/lib/sketch/offset.js` (spec
//! `specs/sketch_chain_offset.md`, branch table 4-13, invariants O1-O5),
//! including the weld tolerance that scales with `|d|`, the miter window for
//! shallow outside corners, and the CCW normalization of a closed result
//! (invariant O5: CW rings scramble downstream profile extraction).
//!
//! Sign convention: `d > 0` offsets to the LEFT of the traversal direction.
//! The UI derives the sign from which side the cursor is on, so a user never
//! sees it.

use crate::ops::chain::{ChainError, ChainItem};
use crate::ops::geom::{
    line_circle_intersections, line_line_intersection, norm_2pi, Point2, Positions, TWO_PI,
};
use crate::types::SketchEntity;

/// Offset arcs whose radius would fall to or below this collapse.
pub const RADIUS_EPS: f64 = 1e-9;

/// Floor for the joint weld tolerance. The effective threshold scales with
/// `|d|`: a tangency angle error ε at the source joint opens a gap of ≈|d|·ε
/// between the offset endpoints, so solver-converged tangents (fillets) must
/// still weld while genuine corners (turn ≫ 1e-3 rad) must not.
pub const JOINT_WELD_TOL: f64 = 1e-9;

/// Outside line-line corners turning less than this miter instead of arcing.
pub const MITER_MAX_RAD: f64 = std::f64::consts::FRAC_PI_6;

/// One piece of a resolved or offset chain, in traversal order.
///
/// An `Arc` is traversed `a0 → a1` in the `ccw` sense (sketch arcs are CCW
/// start→end, so a reversed traversal is CW). A `Circle` only ever appears as
/// a whole-circle offset's single segment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    Line {
        p0: Point2,
        p1: Point2,
    },
    Arc {
        center: Point2,
        r: f64,
        a0: f64,
        a1: f64,
        ccw: bool,
    },
    Circle {
        center: Point2,
        r: f64,
    },
}

/// Why an offset could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffsetError {
    /// Zero distance, no segments, or every output segment collapsed.
    Degenerate,
    /// An offset arc's radius would reach zero.
    RadiusCollapse,
    /// A member entity or one of its points is missing from the sketch.
    MissingGeometry,
    /// A spline (chainable for SELECT, not offsettable).
    UnsupportedEntity,
    /// The chain itself could not be ordered.
    Chain(ChainError),
}

impl OffsetError {
    pub fn tag(self) -> &'static str {
        match self {
            OffsetError::Degenerate => "degenerate",
            OffsetError::RadiusCollapse => "radius-collapse",
            OffsetError::MissingGeometry => "missing-point",
            OffsetError::UnsupportedEntity => "unsupported-entity",
            OffsetError::Chain(e) => e.tag(),
        }
    }
}

impl Segment {
    pub fn start(&self) -> Point2 {
        match self {
            Segment::Line { p0, .. } => *p0,
            Segment::Arc { .. } => self.arc_point(self.a0_or(0.0)),
            Segment::Circle { center, r } => Point2::new(center.x + r, center.y),
        }
    }

    pub fn end(&self) -> Point2 {
        match self {
            Segment::Line { p1, .. } => *p1,
            Segment::Arc { a1, .. } => self.arc_point(*a1),
            Segment::Circle { center, r } => Point2::new(center.x + r, center.y),
        }
    }

    fn a0_or(&self, fallback: f64) -> f64 {
        match self {
            Segment::Arc { a0, .. } => *a0,
            _ => fallback,
        }
    }

    fn arc_point(&self, a: f64) -> Point2 {
        match self {
            Segment::Arc { center, r, .. } | Segment::Circle { center, r } => {
                Point2::new(center.x + r * a.cos(), center.y + r * a.sin())
            }
            Segment::Line { p0, .. } => *p0,
        }
    }

    /// Traversal-direction unit tangent at the segment's start or end.
    fn direction(&self, at_end: bool) -> Point2 {
        match self {
            Segment::Line { p0, p1 } => {
                let len = p0.dist(*p1);
                if len == 0.0 {
                    Point2::new(0.0, 0.0)
                } else {
                    p1.minus(*p0).scale(1.0 / len)
                }
            }
            Segment::Arc { a0, a1, ccw, .. } => {
                let a = if at_end { *a1 } else { *a0 };
                if *ccw {
                    Point2::new(-a.sin(), a.cos())
                } else {
                    Point2::new(a.sin(), -a.cos())
                }
            }
            Segment::Circle { .. } => Point2::new(0.0, 1.0),
        }
    }

    /// Sweep in the traversal sense, in `(0, 2π]`.
    pub fn sweep(&self) -> f64 {
        match self {
            Segment::Arc { a0, a1, ccw, .. } => {
                let raw = if *ccw {
                    norm_2pi(a1 - a0)
                } else {
                    norm_2pi(a0 - a1)
                };
                if raw < 1e-12 {
                    TWO_PI
                } else {
                    raw
                }
            }
            _ => TWO_PI,
        }
    }

    fn set_start(&mut self, p: Point2) {
        match self {
            Segment::Line { p0, .. } => *p0 = p,
            Segment::Arc { center, a0, .. } => *a0 = (p.y - center.y).atan2(p.x - center.x),
            Segment::Circle { .. } => {}
        }
    }

    fn set_end(&mut self, p: Point2) {
        match self {
            Segment::Line { p1, .. } => *p1 = p,
            Segment::Arc { center, a1, .. } => *a1 = (p.y - center.y).atan2(p.x - center.x),
            Segment::Circle { .. } => {}
        }
    }

    fn is_line(&self) -> bool {
        matches!(self, Segment::Line { .. })
    }
}

/// Resolve an ordered chain into traversal segments with concrete geometry.
pub fn resolve_chain_segments(
    items: &[ChainItem],
    entities: &[SketchEntity],
    positions: &Positions,
) -> Result<Vec<Segment>, OffsetError> {
    let mut segments = Vec::with_capacity(items.len());
    for item in items {
        let e = entities
            .iter()
            .find(|e| e.id() == item.id)
            .ok_or(OffsetError::MissingGeometry)?;
        match e {
            SketchEntity::Line {
                start_id, end_id, ..
            } => {
                let s = *positions
                    .get(start_id)
                    .ok_or(OffsetError::MissingGeometry)?;
                let t = *positions.get(end_id).ok_or(OffsetError::MissingGeometry)?;
                segments.push(if item.reversed {
                    Segment::Line { p0: t, p1: s }
                } else {
                    Segment::Line { p0: s, p1: t }
                });
            }
            SketchEntity::Arc {
                center_id,
                start_id,
                end_id,
                ..
            } => {
                let c = *positions
                    .get(center_id)
                    .ok_or(OffsetError::MissingGeometry)?;
                let s = *positions
                    .get(start_id)
                    .ok_or(OffsetError::MissingGeometry)?;
                let t = *positions.get(end_id).ok_or(OffsetError::MissingGeometry)?;
                let r = c.dist(s);
                let a_s = (s.y - c.y).atan2(s.x - c.x);
                let a_e = (t.y - c.y).atan2(t.x - c.x);
                segments.push(if item.reversed {
                    Segment::Arc {
                        center: c,
                        r,
                        a0: a_e,
                        a1: a_s,
                        ccw: false,
                    }
                } else {
                    Segment::Arc {
                        center: c,
                        r,
                        a0: a_s,
                        a1: a_e,
                        ccw: true,
                    }
                });
            }
            _ => return Err(OffsetError::UnsupportedEntity),
        }
    }
    Ok(segments)
}

/// Signed area enclosed by the chain, arcs sampled. Positive = CCW.
pub fn chain_signed_area(segments: &[Segment]) -> f64 {
    let mut pts: Vec<Point2> = Vec::new();
    for seg in segments {
        match seg {
            Segment::Line { p0, .. } => pts.push(*p0),
            Segment::Arc { a0, ccw, .. } => {
                let sweep = seg.sweep() * if *ccw { 1.0 } else { -1.0 };
                let n = 16;
                for i in 0..n {
                    pts.push(seg.arc_point(a0 + sweep * (i as f64) / (n as f64)));
                }
            }
            Segment::Circle { .. } => {
                let n = 16;
                for i in 0..n {
                    pts.push(seg.arc_point(TWO_PI * (i as f64) / (n as f64)));
                }
            }
        }
    }
    let mut area = 0.0;
    for i in 0..pts.len() {
        let p = pts[i];
        let q = pts[(i + 1) % pts.len()];
        area += p.x * q.y - q.x * p.y;
    }
    area / 2.0
}

/// Offset one segment by `d` (left of traversal). `None` on radius collapse.
fn offset_segment(seg: &Segment, d: f64) -> Option<Segment> {
    match seg {
        Segment::Line { p0, p1 } => {
            let u = seg.direction(false);
            let n = u.perp();
            Some(Segment::Line {
                p0: p0.plus(n.scale(d)),
                p1: p1.plus(n.scale(d)),
            })
        }
        Segment::Arc {
            center,
            r,
            a0,
            a1,
            ccw,
        } => {
            // Left of a CCW arc points toward the center, so r shrinks by d;
            // CW grows.
            let nr = if *ccw { r - d } else { r + d };
            (nr > RADIUS_EPS).then_some(Segment::Arc {
                center: *center,
                r: nr,
                a0: *a0,
                a1: *a1,
                ccw: *ccw,
            })
        }
        Segment::Circle { center, r } => {
            let nr = r + d;
            (nr > RADIUS_EPS).then_some(Segment::Circle {
                center: *center,
                r: nr,
            })
        }
    }
}

/// Intersection candidates of the infinite carriers of two offset segments.
fn carrier_intersections(a: &Segment, b: &Segment) -> Vec<Point2> {
    match (a, b) {
        (Segment::Line { p0: a0, p1: a1 }, Segment::Line { p0: b0, p1: b1 }) => {
            line_line_intersection(*a0, *a1, *b0, *b1)
                .map(|p| vec![p])
                .unwrap_or_default()
        }
        (Segment::Line { p0, p1 }, other) | (other, Segment::Line { p0, p1 }) => match other {
            Segment::Arc { center, r, .. } | Segment::Circle { center, r } => {
                line_circle_intersections(*p0, *p1, *center, *r)
            }
            Segment::Line { .. } => Vec::new(),
        },
        (av, bv) => {
            // circle-circle
            let (ac, ar) = match av {
                Segment::Arc { center, r, .. } | Segment::Circle { center, r } => (*center, *r),
                Segment::Line { .. } => return Vec::new(),
            };
            let (bc, br) = match bv {
                Segment::Arc { center, r, .. } | Segment::Circle { center, r } => (*center, *r),
                Segment::Line { .. } => return Vec::new(),
            };
            let delta = bc.minus(ac);
            let dd = delta.len();
            if dd < 1e-12 || dd > ar + br + 1e-12 || dd < (ar - br).abs() - 1e-12 {
                return Vec::new();
            }
            let t = (ar * ar - br * br + dd * dd) / (2.0 * dd);
            let h2 = ar * ar - t * t;
            let h = if h2 > 0.0 { h2.sqrt() } else { 0.0 };
            let m = ac.plus(delta.scale(t / dd));
            if h < 1e-12 {
                return vec![m];
            }
            vec![
                Point2::new(m.x - h * delta.y / dd, m.y + h * delta.x / dd),
                Point2::new(m.x + h * delta.y / dd, m.y - h * delta.x / dd),
            ]
        }
    }
}

/// Corner arc for an outside joint: centered at the SOURCE joint `j` with
/// radius `|d|` from `e` to `s` (both lie on that circle by construction).
/// The sweep sense is fixed by tangent continuity with the incoming segment.
fn corner_arc(j: Point2, d: f64, e: Point2, s: Point2, dir_out: Point2) -> Segment {
    let radial = e.minus(j);
    let ccw = radial.perp().dot(dir_out) >= 0.0;
    Segment::Arc {
        center: j,
        r: d.abs(),
        a0: (e.y - j.y).atan2(e.x - j.x),
        a1: (s.y - j.y).atan2(s.x - j.x),
        ccw,
    }
}

/// The result of an offset: the parallel segments and whether they close.
#[derive(Debug, Clone, PartialEq)]
pub struct OffsetResult {
    pub segments: Vec<Segment>,
    pub closed: bool,
}

/// Offset a resolved chain by signed distance `d` (positive = left of
/// traversal).
pub fn offset_chain_segments(
    segments: &[Segment],
    closed: bool,
    d: f64,
) -> Result<OffsetResult, OffsetError> {
    if segments.is_empty() || d.abs() < RADIUS_EPS {
        return Err(OffsetError::Degenerate);
    }
    // A whole circle has no joints: its offset is one concentric circle.
    if let [Segment::Circle { center, r }] = segments {
        let nr = r + d;
        if nr <= RADIUS_EPS {
            return Err(OffsetError::RadiusCollapse);
        }
        return Ok(OffsetResult {
            segments: vec![Segment::Circle {
                center: *center,
                r: nr,
            }],
            closed: true,
        });
    }

    struct Piece {
        src: Segment,
        off: Segment,
        corner_after: Option<Segment>,
    }
    let mut out: Vec<Piece> = Vec::with_capacity(segments.len());
    for seg in segments {
        let off = offset_segment(seg, d).ok_or(OffsetError::RadiusCollapse)?;
        out.push(Piece {
            src: *seg,
            off,
            corner_after: None,
        });
    }

    let weld_tol = JOINT_WELD_TOL.max(1e-3 * d.abs());
    let joint_count = if closed { out.len() } else { out.len() - 1 };
    for i in 0..joint_count {
        let next = (i + 1) % out.len();
        let e = out[i].off.end();
        let s = out[next].off.start();
        if e.dist(s) < weld_tol {
            // Tangent joint (fillet/slot chains): snap to a shared point.
            let m = e.midpoint(s);
            out[i].off.set_end(m);
            out[next].off.set_start(m);
            continue;
        }

        let j = out[i].src.end(); // source joint position
        let dir_out = out[i].src.direction(true);
        let dir_in = out[next].src.direction(false);
        let turn = dir_out.cross(dir_in);
        let outside = turn * d < 0.0 || (turn.abs() < 1e-12 && dir_out.dot(dir_in) < 0.0);

        if outside {
            let turn_angle = turn.abs().atan2(dir_out.dot(dir_in));
            if out[i].off.is_line() && out[next].off.is_line() && turn_angle < MITER_MAX_RAD {
                let (Segment::Line { p0: a0, p1: a1 }, Segment::Line { p0: b0, p1: b1 }) =
                    (out[i].off, out[next].off)
                else {
                    unreachable!("both are lines");
                };
                if let Some(p) = line_line_intersection(a0, a1, b0, b1) {
                    out[i].off.set_end(p);
                    out[next].off.set_start(p);
                    continue;
                }
            }
            out[i].corner_after = Some(corner_arc(j, d, e, s, dir_out));
            continue;
        }

        // Inside corner: trim/extend both to the carrier intersection nearest
        // the source joint. A degenerate miss welds at the midpoint.
        let candidates = carrier_intersections(&out[i].off, &out[next].off);
        let p = candidates
            .into_iter()
            .min_by(|a, b| {
                a.dist(j)
                    .partial_cmp(&b.dist(j))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or_else(|| e.midpoint(s));
        out[i].off.set_end(p);
        out[next].off.set_start(p);
    }

    let mut result: Vec<Segment> = Vec::new();
    for piece in &out {
        let degenerate = match piece.off {
            Segment::Line { p0, p1 } => p0.dist(p1) < 1e-12,
            _ => {
                let sweep = piece.off.sweep();
                !(1e-9..TWO_PI - 1e-9).contains(&sweep)
            }
        };
        if !degenerate {
            result.push(piece.off);
        }
        if let Some(corner) = piece.corner_after {
            let sweep = corner.sweep();
            if (1e-9..TWO_PI - 1e-9).contains(&sweep) {
                result.push(corner);
            }
        }
    }
    if result.is_empty() {
        return Err(OffsetError::Degenerate);
    }
    // Invariant O5 — deterministic winding: a closed output is always CCW.
    // The chain walk's direction is arbitrary, and downstream profile
    // extraction (outer-face classification, kernel loop staging) assumes
    // drawn-geometry winding; a CW ring scrambles it (NewellMismatch class).
    if closed && chain_signed_area(&result) < 0.0 {
        result.reverse();
        for seg in result.iter_mut() {
            *seg = match *seg {
                Segment::Line { p0, p1 } => Segment::Line { p0: p1, p1: p0 },
                Segment::Arc {
                    center,
                    r,
                    a0,
                    a1,
                    ccw,
                } => Segment::Arc {
                    center,
                    r,
                    a0: a1,
                    a1: a0,
                    ccw: !ccw,
                },
                circle @ Segment::Circle { .. } => circle,
            };
        }
    }
    Ok(OffsetResult {
        segments: result,
        closed,
    })
}

/// Signed perpendicular distance from a point to the chain: magnitude is the
/// distance to the nearest segment, sign is `+1` when the point is LEFT of
/// that segment's traversal. Drives the cursor-side offset preview.
pub fn signed_distance_to_chain(segments: &[Segment], pt: Point2) -> f64 {
    let mut best_dist = f64::INFINITY;
    let mut best_side = 1.0;
    for seg in segments {
        let (d, side) = match seg {
            Segment::Line { p0, p1 } => {
                let u = p1.minus(*p0);
                let len_sq = u.dot(u);
                let t = if len_sq > 0.0 {
                    (pt.minus(*p0).dot(u) / len_sq).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let c = p0.plus(u.scale(t));
                (
                    pt.dist(c),
                    if u.cross(pt.minus(*p0)) >= 0.0 {
                        1.0
                    } else {
                        -1.0
                    },
                )
            }
            // A whole circle has no traversal to be left of, so its sign is
            // OUTWARD-positive: a cursor outside grows the circle. That is
            // the convention the shipped tool had (`offsetCursorDistance`'s
            // `hypot(cursor − center) − r`) and the one a user reads off the
            // screen; pretending the circle is a CCW loop would make dragging
            // outward shrink it.
            Segment::Circle { center, r } => {
                let rr = pt.minus(*center).len();
                ((rr - r).abs(), if rr > *r { 1.0 } else { -1.0 })
            }
            Segment::Arc {
                center, r, a0, ccw, ..
            } => {
                let v = pt.minus(*center);
                let rr = v.len();
                let ang = v.y.atan2(v.x);
                let rel = norm_2pi(if *ccw { ang - a0 } else { a0 - ang });
                if rel <= seg.sweep() {
                    // Left of CCW traversal is toward the center; of CW, away.
                    ((rr - r).abs(), if *ccw == (rr < *r) { 1.0 } else { -1.0 })
                } else {
                    let e0 = seg.start();
                    let e1 = seg.end();
                    let near_start = pt.dist(e0) < pt.dist(e1);
                    let ep = if near_start { e0 } else { e1 };
                    let tan = seg.direction(!near_start);
                    (
                        pt.dist(ep),
                        if tan.cross(pt.minus(ep)) >= 0.0 {
                            1.0
                        } else {
                            -1.0
                        },
                    )
                }
            }
        };
        if d < best_dist {
            best_dist = d;
            best_side = side;
        }
    }
    if best_dist.is_infinite() {
        0.0
    } else {
        best_side * best_dist
    }
}

/// Sample offset segments into one polyline for the preview renderer.
pub fn segments_to_polyline(segments: &[Segment], closed: bool) -> Vec<[f64; 2]> {
    let mut pts: Vec<[f64; 2]> = Vec::new();
    for seg in segments {
        match seg {
            Segment::Line { p0, p1 } => {
                pts.push([p0.x, p0.y]);
                pts.push([p1.x, p1.y]);
            }
            Segment::Circle { .. } => {
                let mut poly = Vec::with_capacity(49);
                for i in 0..=48 {
                    let p = seg.arc_point(TWO_PI * (i as f64) / 48.0);
                    poly.push([p.x, p.y]);
                }
                return poly;
            }
            Segment::Arc { a0, ccw, .. } => {
                let sweep = seg.sweep() * if *ccw { 1.0 } else { -1.0 };
                let n = (((sweep.abs() / TWO_PI) * 48.0).ceil() as usize).max(8);
                for i in 0..=n {
                    let p = seg.arc_point(a0 + sweep * (i as f64) / (n as f64));
                    pts.push([p.x, p.y]);
                }
            }
        }
    }
    if closed {
        if let Some(first) = pts.first().copied() {
            pts.push(first);
        }
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(p0: (f64, f64), p1: (f64, f64)) -> Segment {
        Segment::Line {
            p0: Point2::new(p0.0, p0.1),
            p1: Point2::new(p1.0, p1.1),
        }
    }

    /// A unit square traversed CCW from the origin.
    fn square_ccw() -> Vec<Segment> {
        vec![
            line((0.0, 0.0), (1.0, 0.0)),
            line((1.0, 0.0), (1.0, 1.0)),
            line((1.0, 1.0), (0.0, 1.0)),
            line((0.0, 1.0), (0.0, 0.0)),
        ]
    }

    fn bbox(segments: &[Segment]) -> (f64, f64, f64, f64) {
        let mut b = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for s in segments {
            for p in [s.start(), s.end()] {
                b.0 = b.0.min(p.x);
                b.1 = b.1.max(p.x);
                b.2 = b.2.min(p.y);
                b.3 = b.3.max(p.y);
            }
        }
        b
    }

    #[test]
    fn a_square_offset_outward_grows_by_the_distance_and_gains_four_corner_arcs() {
        // CCW traversal: the OUTSIDE is to the right, so d is negative.
        let out = offset_chain_segments(&square_ccw(), true, -0.1).expect("a square offsets");
        let lines = out.segments.iter().filter(|s| s.is_line()).count();
        let arcs = out.segments.len() - lines;
        assert_eq!(lines, 4);
        assert_eq!(arcs, 4, "an outside corner becomes a |d| arc");
        let (min_x, max_x, min_y, max_y) = bbox(&out.segments);
        assert!((min_x + 0.1).abs() < 1e-12, "{min_x}");
        assert!((max_x - 1.1).abs() < 1e-12, "{max_x}");
        assert!((min_y + 0.1).abs() < 1e-12, "{min_y}");
        assert!((max_y - 1.1).abs() < 1e-12, "{max_y}");
    }

    #[test]
    fn a_square_offset_inward_shrinks_and_keeps_four_mitered_corners() {
        let out = offset_chain_segments(&square_ccw(), true, 0.1).expect("a square offsets");
        assert_eq!(out.segments.len(), 4, "inside corners trim, never arc");
        let (min_x, max_x, min_y, max_y) = bbox(&out.segments);
        assert!((min_x - 0.1).abs() < 1e-12, "{min_x}");
        assert!((max_x - 0.9).abs() < 1e-12, "{max_x}");
        assert!((min_y - 0.1).abs() < 1e-12, "{min_y}");
        assert!((max_y - 0.9).abs() < 1e-12, "{max_y}");
    }

    #[test]
    fn a_closed_offset_always_comes_back_counter_clockwise() {
        // Invariant O5, both traversal directions of the same square.
        let mut cw = square_ccw();
        cw.reverse();
        for s in cw.iter_mut() {
            if let Segment::Line { p0, p1 } = *s {
                *s = Segment::Line { p0: p1, p1: p0 };
            }
        }
        for (segments, d) in [(square_ccw(), 0.1), (cw, -0.1)] {
            let out = offset_chain_segments(&segments, true, d).expect("offsets");
            assert!(
                chain_signed_area(&out.segments) > 0.0,
                "closed output must wind CCW"
            );
        }
    }

    #[test]
    fn a_circle_offsets_to_a_concentric_circle() {
        let c = vec![Segment::Circle {
            center: Point2::new(1.0, 2.0),
            r: 0.5,
        }];
        let out = offset_chain_segments(&c, true, 0.25).expect("a circle offsets");
        assert_eq!(
            out.segments,
            vec![Segment::Circle {
                center: Point2::new(1.0, 2.0),
                r: 0.75
            }]
        );
    }

    #[test]
    fn an_offset_that_would_collapse_a_radius_is_refused_by_name() {
        let c = vec![Segment::Circle {
            center: Point2::new(0.0, 0.0),
            r: 0.5,
        }];
        assert_eq!(
            offset_chain_segments(&c, true, -0.5),
            Err(OffsetError::RadiusCollapse)
        );
    }

    #[test]
    fn a_zero_distance_offset_is_degenerate_not_a_copy() {
        assert_eq!(
            offset_chain_segments(&square_ccw(), true, 0.0),
            Err(OffsetError::Degenerate)
        );
    }

    #[test]
    fn an_open_run_offsets_without_closing_its_ends() {
        let run = vec![line((0.0, 0.0), (1.0, 0.0)), line((1.0, 0.0), (1.0, 1.0))];
        let out = offset_chain_segments(&run, false, 0.1).expect("an elbow offsets");
        assert!(!out.closed);
        // One joint, inside for d > 0 on this turn: two segments, no arc.
        assert_eq!(out.segments.len(), 2, "{:?}", out.segments);
    }

    #[test]
    fn a_shallow_outside_corner_miters_instead_of_arcing() {
        // A 10° turn: inside MITER_MAX_RAD (30°), so no corner arc.
        let a = line((0.0, 0.0), (1.0, 0.0));
        let ang = 10f64.to_radians();
        let b = Segment::Line {
            p0: Point2::new(1.0, 0.0),
            p1: Point2::new(1.0 + ang.cos(), ang.sin()),
        };
        let out = offset_chain_segments(&[a, b], false, -0.05).expect("offsets");
        assert_eq!(out.segments.len(), 2, "mitered: {:?}", out.segments);
    }

    #[test]
    fn a_sharp_outside_corner_arcs() {
        // A 90° turn: outside the miter window, so one corner arc appears.
        let out = offset_chain_segments(
            &[line((0.0, 0.0), (1.0, 0.0)), line((1.0, 0.0), (1.0, 1.0))],
            false,
            -0.05,
        )
        .expect("offsets");
        assert_eq!(out.segments.len(), 3, "{:?}", out.segments);
        assert!(!out.segments[1].is_line(), "the middle piece is the arc");
    }

    #[test]
    fn a_circles_signed_distance_is_positive_outside_it() {
        // The inherited convention (the shipped `offsetCursorDistance`): a
        // cursor outside the circle grows it. `Side::Left` is "positive", so
        // for a lone circle Left means outward — asserted here because it is
        // the one place the chain's left/right reading does not apply.
        let c = [Segment::Circle {
            center: Point2::new(0.0, 0.0),
            r: 1.0,
        }];
        assert!(signed_distance_to_chain(&c, Point2::new(1.5, 0.0)) > 0.0);
        assert!(signed_distance_to_chain(&c, Point2::new(0.5, 0.0)) < 0.0);
        assert!(
            (signed_distance_to_chain(&c, Point2::new(1.5, 0.0)) - 0.5).abs() < 1e-12,
            "and its magnitude is the gap"
        );
    }

    #[test]
    fn the_signed_distance_is_positive_left_of_the_traversal() {
        let run = [line((0.0, 0.0), (1.0, 0.0))];
        assert!(signed_distance_to_chain(&run, Point2::new(0.5, 0.2)) > 0.0);
        assert!(signed_distance_to_chain(&run, Point2::new(0.5, -0.2)) < 0.0);
        assert!(
            (signed_distance_to_chain(&run, Point2::new(0.5, 0.2)) - 0.2).abs() < 1e-12,
            "and its magnitude is the distance"
        );
    }

    #[test]
    fn a_polyline_of_a_closed_offset_returns_to_its_start() {
        let out = offset_chain_segments(&square_ccw(), true, 0.1).unwrap();
        let poly = segments_to_polyline(&out.segments, true);
        assert_eq!(poly.first(), poly.last());
    }
}
