//! Recovered-region → kernel `ProfileEdge` conversion helpers for the adapter
//! (move-only F9 split from `adapter.rs`; byte-identical): polygon/area/point
//! predicates and the arc-polygon reconstruction used to turn recovered sketch
//! regions into profiles. See `super`'s adapter docs.

use super::*;

/// Map recovered region edges to kernel `ProfileEdge`s (plane coordinates).
pub(super) fn region_edges_to_profile(
    edges: &[waffle_types::RegionEdge],
) -> Vec<crate::ProfileEdge> {
    edges
        .iter()
        .map(|e| match *e {
            waffle_types::RegionEdge::Line { a, b } => crate::ProfileEdge::Line {
                a: Point2::new(a.0, a.1),
                b: Point2::new(b.0, b.1),
            },
            waffle_types::RegionEdge::Arc {
                a,
                b,
                center,
                radius,
                ccw,
            } => crate::ProfileEdge::Arc {
                a: Point2::new(a.0, a.1),
                b: Point2::new(b.0, b.1),
                center: Point2::new(center.0, center.1),
                radius,
                ccw,
            },
        })
        .collect()
}

// ── KV14 hole-assembly helpers (f64 heuristics; Profile::new is the exact gate) ─

/// Even-odd point-in-polygon test. Used only to assign an inner loop to its
/// containing outer; `Profile::new` re-checks containment exactly.
pub(super) fn point_in_polygon_2d(p: Point2, poly: &[Point2]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let (px, py) = (p.x(), p.y());
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (poly[i].x(), poly[i].y());
        let (xj, yj) = (poly[j].x(), poly[j].y());
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Absolute shoelace area — picks the SMALLEST containing outer for a hole.
pub(super) fn polygon_area_abs(poly: &[Point2]) -> f64 {
    let n = poly.len();
    let mut a = 0.0;
    let mut j = n - 1;
    for i in 0..n {
        a += (poly[j].x() + poly[i].x()) * (poly[j].y() - poly[i].y());
        j = i;
    }
    (a * 0.5).abs()
}

/// Closed polyline → `ProfileEdge::Line` loop (a polygon/circle hole carried
/// into a Tier-2 `ArcPolygon` as straight edges).
pub(super) fn pts_to_line_edges(pts: &[Point2]) -> Vec<crate::ProfileEdge> {
    let n = pts.len();
    (0..n)
        .map(|i| crate::ProfileEdge::Line {
            a: pts[i],
            b: pts[(i + 1) % n],
        })
        .collect()
}

/// Polygonize a circle rim into an N-gon — only when a circle outer carries
/// holes (a true holed circle needs a polygon outer). N is print-grade.
pub(super) fn polygonize_circle(center: Point2, radius: f64) -> Vec<Point2> {
    const N: usize = 64;
    (0..N)
        .map(|i| {
            let a = std::f64::consts::TAU * (i as f64) / (N as f64);
            Point2::new(center.x() + radius * a.cos(), center.y() + radius * a.sin())
        })
        .collect()
}

// ── KV12 Tier 2 reconstruction (arc_segments + chord polygon → ArcPolygon) ──

/// Append minor (`sweep < π`) sub-arcs covering chord-sample vertices
/// `vstart ..= vend` (indices into `pts`, wrapping mod `n`) of one circular
/// `arc_segment`, splitting at sample points so each sub-arc clears the
/// arena's minor-arc requirement. Returns `false` if a sample is off the
/// circle beyond the import band or a degenerate (zero-sweep) sub-arc would
/// result — the caller then falls back to the Tier-1 chord polygon.
pub(super) fn push_minor_subarcs(
    out: &mut Vec<crate::ProfileEdge>,
    pts: &[Point2],
    n: usize,
    vstart: usize,
    vend: usize,
    center: Point2,
    radius: f64,
) -> bool {
    // Split before the cumulative sweep reaches π (with margin); each
    // consecutive sample step is small, so this yields the minimum count of
    // < π sub-arcs (a semicircle → two ≈90° patches).
    const MAX_SWEEP: f64 = std::f64::consts::PI * 0.9;
    let band = 1e-9 * radius.max(1.0);
    let radial = |k: usize| {
        let p = pts[k % n];
        (p.x() - center.x(), p.y() - center.y())
    };
    // Reject samples off the circle (faithfulness of the exact arc model).
    for k in vstart..=vend {
        let (rx, ry) = radial(k);
        if ((rx * rx + ry * ry).sqrt() - radius).abs() > band {
            return false;
        }
    }
    let mut group_start = vstart;
    let mut acc = 0.0;
    let mut k = vstart;
    while k < vend {
        let (ux, uy) = radial(k);
        let (vx, vy) = radial(k + 1);
        let step = (ux * vy - uy * vx).atan2(ux * vx + uy * vy).abs();
        if acc + step > MAX_SWEEP && k > group_start {
            out.push(crate::ProfileEdge::Arc {
                a: pts[group_start % n],
                b: pts[k % n],
                center,
                radius,
                ccw: true,
            });
            group_start = k;
            acc = 0.0;
        }
        acc += step;
        k += 1;
    }
    if acc <= 0.0 || group_start % n == vend % n {
        return false; // degenerate (zero-sweep) trailing sub-arc
    }
    out.push(crate::ProfileEdge::Arc {
        a: pts[group_start % n],
        b: pts[vend % n],
        center,
        radius,
        ccw: true,
    });
    true
}

/// Reconstruct an exact `ProfileEdge` loop (lines + minor arcs) from a
/// chord-sample polygon `pts` and its `arc_segments` (PR-KV12 Tier 2, §3).
/// Each `arc_segment` covers a vertex run `[start ..= end]`; the edges it
/// spans collapse to minor sub-arcs, every other edge stays a line. Returns
/// `None` (→ Tier-1 chord fallback) on any malformed segment (out-of-range
/// or non-increasing indices, overlapping segments, off-circle samples).
pub(super) fn reconstruct_arc_polygon_edges(
    pts: &[Point2],
    arc_segments: &[waffle_types::ArcSegment],
) -> Option<Vec<crate::ProfileEdge>> {
    let n = pts.len();
    if n < 3 || arc_segments.is_empty() {
        return None;
    }
    // edge_arc[i] = the arc covering edge (i → i+1), if any. An arc run
    // `[s ..= e]` covers edges `s .. e`; when `e < s` the run wraps through
    // the closing edge (e.g. a D-shape whose diameter line is drawn first,
    // so the arc closes onto vertex 0). A run wrapping the 0/n boundary is
    // simply split there into two same-circle sub-arcs by the walk below —
    // geometrically harmless (an extra split point).
    let mut edge_arc: Vec<Option<usize>> = vec![None; n];
    for (ai, seg) in arc_segments.iter().enumerate() {
        let (s, e) = (seg.start_vertex_index, seg.end_vertex_index);
        if s >= n || e >= n || s == e {
            return None;
        }
        let edge_indices: Vec<usize> = if s < e {
            (s..e).collect()
        } else {
            (s..n).chain(0..e).collect()
        };
        for edge in edge_indices {
            if edge_arc[edge].is_some() {
                return None; // overlapping arc segments
            }
            edge_arc[edge] = Some(ai);
        }
    }
    let mut edges = Vec::new();
    let mut i = 0;
    while i < n {
        match edge_arc[i] {
            None => {
                edges.push(crate::ProfileEdge::Line {
                    a: pts[i],
                    b: pts[(i + 1) % n],
                });
                i += 1;
            }
            Some(ai) => {
                let mut j = i;
                while j < n && edge_arc[j] == Some(ai) {
                    j += 1;
                }
                let seg = &arc_segments[ai];
                let center = Point2::new(seg.center_u, seg.center_v);
                if !push_minor_subarcs(&mut edges, pts, n, i, j, center, seg.radius) {
                    return None;
                }
                i = j;
            }
        }
    }
    Some(edges)
}
