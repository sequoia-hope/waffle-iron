//! Geometric "share-a-face" predicates for the extrude auto-target default
//! (spec `specs/optional_booleans_multibody_extrude.md` §4.3(b)).
//!
//! Pure functions — no kernel dependency — so they are unit-testable and
//! reusable. The rebuild dispatch (`rebuild::resolve_share_a_face`) feeds them
//! face geometry pulled from `KernelIntrospect` and the sketch's profile
//! footprint.

use waffle_types::kernel::units::TAU_MODEL;

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize3(v: [f64; 3]) -> Option<[f64; 3]> {
    let len = dot3(v, v).sqrt();
    if len < 1e-12 {
        None
    } else {
        Some([v[0] / len, v[1] / len, v[2] / len])
    }
}

/// True iff a face's plane — outward normal `face_normal`, a point `face_point`
/// on it — is coincident with the sketch plane (origin `sketch_origin`, normal
/// `sketch_normal`): the normals are (anti)parallel within `TAU_MODEL` AND the
/// face point lies in the sketch plane within `TAU_MODEL`.
///
/// Antiparallel is coincident on purpose: a body's top face (normal +z) shares
/// its plane with a sketch whose normal points −z.
pub fn plane_coincident(
    sketch_origin: [f64; 3],
    sketch_normal: [f64; 3],
    face_normal: [f64; 3],
    face_point: [f64; 3],
) -> bool {
    let (sn, fnorm) = match (normalize3(sketch_normal), normalize3(face_normal)) {
        (Some(a), Some(b)) => (a, b),
        _ => return false,
    };
    // (anti)parallel normals
    if dot3(sn, fnorm).abs() < 1.0 - TAU_MODEL {
        return false;
    }
    // same plane offset (face point lies in the sketch plane)
    dot3(sub3(face_point, sketch_origin), sn).abs() <= TAU_MODEL
}

// ── 2D polygon overlap ───────────────────────────────────────────────────────

/// Signed twice-area orientation of (a, b, c): >0 CCW, <0 CW, ~0 collinear.
fn orient2d(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

const EPS2D: f64 = 1e-9;

/// Is `p` on the closed segment `a`–`b` (collinear and within its span)?
fn on_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> bool {
    if orient2d(a, b, p).abs() > EPS2D {
        return false;
    }
    p[0] >= a[0].min(b[0]) - EPS2D
        && p[0] <= a[0].max(b[0]) + EPS2D
        && p[1] >= a[1].min(b[1]) - EPS2D
        && p[1] <= a[1].max(b[1]) + EPS2D
}

/// Strictly-inside test: even-odd ray cast, but a point ON the boundary is NOT
/// strictly inside (so an edge-touch does not count as overlap).
fn point_strictly_in_poly(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    for i in 0..n {
        if on_segment(p, poly[i], poly[(i + 1) % n]) {
            return false;
        }
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (poly[i][0], poly[i][1]);
        let (xj, yj) = (poly[j][0], poly[j][1]);
        if ((yi > p[1]) != (yj > p[1])) && (p[0] < (xj - xi) * (p[1] - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Do segments `p1`–`p2` and `p3`–`p4` PROPERLY cross (interiors meet at a
/// single transversal point)? Collinear overlap and endpoint touches are NOT
/// proper crossings.
fn segments_properly_cross(p1: [f64; 2], p2: [f64; 2], p3: [f64; 2], p4: [f64; 2]) -> bool {
    let d1 = orient2d(p3, p4, p1);
    let d2 = orient2d(p3, p4, p2);
    let d3 = orient2d(p1, p2, p3);
    let d4 = orient2d(p1, p2, p4);
    (d1 > EPS2D && d2 < -EPS2D || d1 < -EPS2D && d2 > EPS2D)
        && (d3 > EPS2D && d4 < -EPS2D || d3 < -EPS2D && d4 > EPS2D)
}

fn centroid(poly: &[[f64; 2]]) -> [f64; 2] {
    let n = poly.len().max(1) as f64;
    let (sx, sy) = poly
        .iter()
        .fold((0.0, 0.0), |(sx, sy), p| (sx + p[0], sy + p[1]));
    [sx / n, sy / n]
}

/// True iff two 2D polygons (each an ordered `[x, y]` loop, ≥3 points) share
/// positive area. Detected by: any vertex or centroid of one strictly inside
/// the other, or any pair of edges properly crossing. Edge/vertex-only touching
/// (zero-area contact) returns false; identical polygons return true (via the
/// centroid test).
pub fn polygons_overlap_2d(a: &[[f64; 2]], b: &[[f64; 2]]) -> bool {
    if a.len() < 3 || b.len() < 3 {
        return false;
    }
    if a.iter().any(|&p| point_strictly_in_poly(p, b)) {
        return true;
    }
    if b.iter().any(|&p| point_strictly_in_poly(p, a)) {
        return true;
    }
    // Centroid test resolves the coincident-boundary case (identical polygons,
    // and one strictly containing the other with no vertex strictly inside).
    if point_strictly_in_poly(centroid(a), b) || point_strictly_in_poly(centroid(b), a) {
        return true;
    }
    for i in 0..a.len() {
        let a0 = a[i];
        let a1 = a[(i + 1) % a.len()];
        for j in 0..b.len() {
            if segments_properly_cross(a0, a1, b[j], b[(j + 1) % b.len()]) {
                return true;
            }
        }
    }
    false
}

/// Convex hull (CCW) of a 2D point set via Andrew's monotone chain. Returns the
/// input (deduped) when fewer than 3 distinct points. Used to reduce a profile
/// or face-boundary vertex set to an ordered polygon for the overlap test — a
/// documented conservative approximation for the share-a-face heuristic: a
/// concave footprint is treated as its hull, which can only *widen* the auto
/// merge, never miss a genuine overlap (the user can always override targets).
pub fn convex_hull_2d(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut pts: Vec<[f64; 2]> = points.to_vec();
    pts.sort_by(|a, b| {
        a[0].partial_cmp(&b[0])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a[1].partial_cmp(&b[1]).unwrap_or(std::cmp::Ordering::Equal))
    });
    pts.dedup_by(|a, b| (a[0] - b[0]).abs() < EPS2D && (a[1] - b[1]).abs() < EPS2D);
    if pts.len() < 3 {
        return pts;
    }
    let mut hull: Vec<[f64; 2]> = Vec::with_capacity(pts.len() + 1);
    // lower hull
    for &p in &pts {
        while hull.len() >= 2 && orient2d(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
            hull.pop();
        }
        hull.push(p);
    }
    // upper hull
    let lower_len = hull.len() + 1;
    for &p in pts.iter().rev() {
        while hull.len() >= lower_len
            && orient2d(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0
        {
            hull.pop();
        }
        hull.push(p);
    }
    hull.pop(); // last point equals the first
    hull
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hull_of_square_with_interior_point_is_the_square() {
        let pts = vec![
            [0.0, 0.0],
            [2.0, 0.0],
            [2.0, 2.0],
            [0.0, 2.0],
            [1.0, 1.0], // interior — must be dropped
        ];
        assert_eq!(convex_hull_2d(&pts).len(), 4);
    }

    #[test]
    fn centroid_of_identical_square_is_inside() {
        let sq = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        assert!(point_strictly_in_poly(centroid(&sq), &sq));
    }

    #[test]
    fn edge_touch_is_not_a_proper_cross() {
        // Collinear shared edge x=1.
        assert!(!segments_properly_cross(
            [1.0, 0.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [1.0, 1.0]
        ));
    }
}
