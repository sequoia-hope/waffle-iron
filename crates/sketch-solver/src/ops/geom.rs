//! Planar geometry the sketch operations are built on.
//!
//! A direct port of `app/src/lib/sketch/geometry-utils.js`, tolerances and
//! degenerate-case branches included. The tolerances are not re-derived here:
//! they are what the shipped tools behaved with, and the GUI specs for trim,
//! offset and fillet are acceptance tests for the port
//! (`specs/agent_mechanical_design.md` §10.1), so changing one would be a
//! behaviour change dressed as a port.

use std::collections::HashMap;

use crate::types::SketchEntity;

/// A planar point or vector, in sketch-local coordinates (meters).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

impl Point2 {
    pub fn new(x: f64, y: f64) -> Self {
        Point2 { x, y }
    }
    pub fn minus(self, o: Point2) -> Point2 {
        Point2::new(self.x - o.x, self.y - o.y)
    }
    pub fn plus(self, o: Point2) -> Point2 {
        Point2::new(self.x + o.x, self.y + o.y)
    }
    pub fn scale(self, k: f64) -> Point2 {
        Point2::new(self.x * k, self.y * k)
    }
    pub fn dot(self, o: Point2) -> f64 {
        self.x * o.x + self.y * o.y
    }
    pub fn cross(self, o: Point2) -> f64 {
        self.x * o.y - self.y * o.x
    }
    pub fn len(self) -> f64 {
        self.x.hypot(self.y)
    }
    pub fn dist(self, o: Point2) -> f64 {
        self.minus(o).len()
    }
    /// Unit vector, or `None` below `1e-12` (JS `|| 1` fallbacks are replaced
    /// by an explicit absence: a direction nobody can compute must not be
    /// silently `(1, 0)`).
    pub fn unit(self) -> Option<Point2> {
        let l = self.len();
        (l > 1e-12).then(|| self.scale(1.0 / l))
    }
    /// Left-hand perpendicular (+90°).
    pub fn perp(self) -> Point2 {
        Point2::new(-self.y, self.x)
    }
    pub fn midpoint(self, o: Point2) -> Point2 {
        Point2::new((self.x + o.x) / 2.0, (self.y + o.y) / 2.0)
    }
}

pub const TWO_PI: f64 = std::f64::consts::TAU;

/// Normalize an angle into `[0, 2π)`.
pub fn norm_2pi(a: f64) -> f64 {
    let r = a % TWO_PI;
    if r < 0.0 {
        r + TWO_PI
    } else {
        r
    }
}

/// Intersection of the INFINITE lines through `p1 p2` and `p3 p4`; `None`
/// when they are parallel or coincident (`|denom| < 1e-12`).
pub fn line_line_intersection(p1: Point2, p2: Point2, p3: Point2, p4: Point2) -> Option<Point2> {
    let d1 = p2.minus(p1);
    let d2 = p4.minus(p3);
    let denom = d1.cross(d2);
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = ((p3.x - p1.x) * d2.y - (p3.y - p1.y) * d2.x) / denom;
    Some(p1.plus(d1.scale(t)))
}

/// Intersections of the INFINITE line through `a b` with a circle. Zero, one
/// (tangent) or two points, in increasing line parameter.
pub fn line_circle_intersections(a: Point2, b: Point2, center: Point2, radius: f64) -> Vec<Point2> {
    let d = b.minus(a);
    let f = a.minus(center);
    let qa = d.dot(d);
    if qa < 1e-12 {
        return Vec::new(); // degenerate line
    }
    let qb = 2.0 * f.dot(d);
    let qc = f.dot(f) - radius * radius;
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < -1e-10 {
        return Vec::new();
    }
    if disc < 1e-10 {
        let t = -qb / (2.0 * qa);
        return vec![a.plus(d.scale(t))];
    }
    let s = disc.sqrt();
    vec![
        a.plus(d.scale((-qb - s) / (2.0 * qa))),
        a.plus(d.scale((-qb + s) / (2.0 * qa))),
    ]
}

/// Is `angle` inside the CCW sweep from `start` to `end`? A sweep under
/// `1e-10` is read as a full circle, matching the JS.
pub fn angle_in_arc(angle: f64, start: f64, end: f64) -> bool {
    let a = norm_2pi(angle - start);
    let mut sweep = norm_2pi(end - start);
    if sweep < 1e-10 {
        sweep = TWO_PI;
    }
    a <= sweep + 1e-10
}

/// Intersections of an infinite line with an ARC, filtered to the arc's CCW
/// angular span.
#[allow(clippy::too_many_arguments)]
pub fn arc_line_intersections(
    center: Point2,
    radius: f64,
    start_angle: f64,
    end_angle: f64,
    a: Point2,
    b: Point2,
) -> Vec<Point2> {
    line_circle_intersections(a, b, center, radius)
        .into_iter()
        .filter(|p| {
            angle_in_arc(
                (p.y - center.y).atan2(p.x - center.x),
                start_angle,
                end_angle,
            )
        })
        .collect()
}

/// Parameter of `p` projected onto the line through `a b`: 0 at `a`, 1 at `b`.
/// `0.0` for a degenerate segment.
pub fn parameter_on_segment(p: Point2, a: Point2, b: Point2) -> f64 {
    let d = b.minus(a);
    let len_sq = d.dot(d);
    if len_sq < 1e-12 {
        return 0.0;
    }
    p.minus(a).dot(d) / len_sq
}

/// Foot of the perpendicular from `p` onto the INFINITE line through `a b`.
pub fn perpendicular_foot(p: Point2, a: Point2, b: Point2) -> Point2 {
    let d = b.minus(a);
    let len_sq = d.dot(d);
    if len_sq < 1e-12 {
        return a;
    }
    a.plus(d.scale(p.minus(a).dot(d) / len_sq))
}

/// Unit bisector of two directions. Anti-parallel inputs bisect
/// perpendicular to the first; a zero-length input yields `(1, 0)` — the JS
/// fallback, kept so the fillet tool's refusal cases stay identical.
pub fn angle_bisector(d1: Point2, d2: Point2) -> Point2 {
    let (Some(n1), Some(n2)) = (d1.unit(), d2.unit()) else {
        return Point2::new(1.0, 0.0);
    };
    let b = n1.plus(n2);
    match b.unit() {
        Some(u) => u,
        None => n1.perp(),
    }
}

/// Positions of a sketch's points, as the ops read them.
pub type Positions = HashMap<u32, Point2>;

/// A sketch's solved positions, falling back to each `Point`'s declared
/// coordinates where the solve published none.
///
/// Ops run on the geometry the user is LOOKING at, which is the solved
/// position — trimming a line at a screen point and then splitting it at its
/// declared coordinates would cut somewhere else entirely.
pub fn positions_of(entities: &[SketchEntity], solved: &HashMap<u32, (f64, f64)>) -> Positions {
    let mut out: Positions = HashMap::new();
    for e in entities {
        if let SketchEntity::Point { id, x, y, .. } = e {
            out.insert(*id, Point2::new(*x, *y));
        }
    }
    for (id, (x, y)) in solved {
        out.insert(*id, Point2::new(*x, *y));
    }
    out
}

/// A curve's radius as the geometry shows it: a circle's own parameter, an
/// arc's center→start distance.
pub fn entity_radius(entity: &SketchEntity, positions: &Positions) -> Option<f64> {
    match entity {
        SketchEntity::Circle { radius, .. } => Some(*radius),
        SketchEntity::Arc {
            center_id,
            start_id,
            ..
        } => {
            let c = positions.get(center_id)?;
            let s = positions.get(start_id)?;
            Some(c.dist(*s))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossing_lines_meet_at_the_origin() {
        let p = line_line_intersection(
            Point2::new(-1.0, -1.0),
            Point2::new(1.0, 1.0),
            Point2::new(-1.0, 1.0),
            Point2::new(1.0, -1.0),
        )
        .expect("an X crosses");
        assert!(p.x.abs() < 1e-12 && p.y.abs() < 1e-12);
    }

    #[test]
    fn parallel_lines_do_not_meet() {
        assert!(line_line_intersection(
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
            Point2::new(1.0, 1.0),
        )
        .is_none());
    }

    #[test]
    fn a_line_through_a_circle_center_cuts_it_twice() {
        let pts = line_circle_intersections(
            Point2::new(-5.0, 0.0),
            Point2::new(5.0, 0.0),
            Point2::new(0.0, 0.0),
            2.0,
        );
        assert_eq!(pts.len(), 2);
        assert!((pts[0].x + 2.0).abs() < 1e-12, "{:?}", pts);
        assert!((pts[1].x - 2.0).abs() < 1e-12, "{:?}", pts);
    }

    #[test]
    fn a_tangent_line_touches_once() {
        let pts = line_circle_intersections(
            Point2::new(-5.0, 2.0),
            Point2::new(5.0, 2.0),
            Point2::new(0.0, 0.0),
            2.0,
        );
        assert_eq!(pts.len(), 1, "{:?}", pts);
    }

    #[test]
    fn a_missing_line_touches_never() {
        assert!(line_circle_intersections(
            Point2::new(-5.0, 3.0),
            Point2::new(5.0, 3.0),
            Point2::new(0.0, 0.0),
            2.0,
        )
        .is_empty());
    }

    #[test]
    fn an_arc_span_filters_the_circle_hits() {
        // Upper half arc (0 → π); a horizontal line at y = 1 crosses the full
        // circle twice but the arc also twice; at y = -1 the arc not at all.
        let c = Point2::new(0.0, 0.0);
        let upper = arc_line_intersections(
            c,
            2.0,
            0.0,
            std::f64::consts::PI,
            Point2::new(-5.0, 1.0),
            Point2::new(5.0, 1.0),
        );
        assert_eq!(upper.len(), 2, "{upper:?}");
        let lower = arc_line_intersections(
            c,
            2.0,
            0.0,
            std::f64::consts::PI,
            Point2::new(-5.0, -1.0),
            Point2::new(5.0, -1.0),
        );
        assert!(lower.is_empty(), "{lower:?}");
    }

    #[test]
    fn the_segment_parameter_is_zero_at_the_start_and_one_at_the_end() {
        let a = Point2::new(1.0, 1.0);
        let b = Point2::new(3.0, 1.0);
        assert!(parameter_on_segment(a, a, b).abs() < 1e-15);
        assert!((parameter_on_segment(b, a, b) - 1.0).abs() < 1e-15);
        assert!((parameter_on_segment(Point2::new(2.0, 9.0), a, b) - 0.5).abs() < 1e-15);
    }

    #[test]
    fn the_bisector_of_two_axes_points_at_45_degrees() {
        let b = angle_bisector(Point2::new(1.0, 0.0), Point2::new(0.0, 1.0));
        assert!((b.x - b.y).abs() < 1e-12);
        assert!((b.len() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn anti_parallel_directions_bisect_perpendicular() {
        let b = angle_bisector(Point2::new(1.0, 0.0), Point2::new(-1.0, 0.0));
        assert!(
            b.x.abs() < 1e-12 && (b.y.abs() - 1.0).abs() < 1e-12,
            "{b:?}"
        );
    }

    #[test]
    fn solved_positions_win_over_declared_ones() {
        let entities = vec![SketchEntity::Point {
            id: 1,
            x: 0.0,
            y: 0.0,
            construction: false,
        }];
        let mut solved = HashMap::new();
        solved.insert(1, (3.0, 4.0));
        let p = positions_of(&entities, &solved);
        assert_eq!(p[&1], Point2::new(3.0, 4.0), "ops run on solved geometry");
    }
}
