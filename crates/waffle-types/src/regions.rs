//! Minimal-face (region) extraction from a solved sketch via a planar arrangement.
//!
//! Whole-loop profile extraction (`crate::profiles`) returns each closed loop of
//! the sketch as one profile. That cannot represent the *sub-regions* created
//! when shapes overlap: two concentric circles give an inner disk and an
//! annulus; two crossing circles give a lens and two crescents. This module
//! computes every minimal closed face of the sketch so the UI can select the
//! smallest one under a click and extrude it.
//!
//! The arrangement is produced with the `i_overlay` slice operation: a padded
//! bounding box is sliced by every (tessellated) non-construction curve, and
//! each resulting `Shape` (outer contour + hole contours) becomes a [`Region`].
//! This is the single source of truth — the JS UI only hit-tests the returned
//! regions.
//!
//! ## Analytical preservation (Invariant A15)
//!
//! A region whose boundary coincides with a single whole-entity profile and has
//! **no holes** carries [`Region::profile_entity_ids`]. The UI maps those entity
//! ids back to a `profile_index` and extrudes through the existing analytical
//! path (`Profile::circle` for circles, exact loops otherwise), so the common
//! non-overlapping case is byte-for-byte unchanged. Genuine sub-regions
//! (annulus, lens, crescent) have `profile_entity_ids == None`; their boundary
//! is still extruded with TRUE curves: [`recover_edges`] re-derives the circular
//! arc runs from the tessellated boundary (vertices placed exactly on the source
//! circle, full/major arcs split into minor sub-arcs) and the kernel builds
//! exact cylinder walls via `Profile::arc_polygon`. The tessellated `outer`/
//! `holes` remain for hit-testing and as a loud fallback.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use i_overlay::core::fill_rule::FillRule;
use i_overlay::float::slice::FloatSlice;

use crate::sketch::SketchEntity;

/// A minimal closed face of the sketch, in sketch UV coordinates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Region {
    /// Outer boundary, counter-clockwise. Curved boundaries are tessellated.
    pub outer: Vec<(f64, f64)>,
    /// Hole boundaries (clockwise), each a closed loop. Empty for simple faces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Vec<(f64, f64)>>,
    /// Filled area (outer minus holes). The UI selects the smallest region whose
    /// interior contains the click. Defaulted on deserialize so the extrude path
    /// can send a geometry-only region (just `outer`/`holes`).
    #[serde(default)]
    pub area: f64,
    /// When `Some`, this region equals one whole-entity profile and has no holes:
    /// these are that profile's `entity_ids`. The UI resolves them to a
    /// `profile_index` and uses the existing analytical extrude path. When
    /// `None`, the region is a genuine sub-region and must be extruded from its
    /// explicit `outer`/`holes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_entity_ids: Option<Vec<u32>>,
    /// Curve-aware outer boundary: the same loop as `outer`, but with runs that
    /// lie on a source circle/arc recovered as [`RegionEdge::Arc`] (split into
    /// minor arcs, vertices placed exactly on the circle). The extrude path
    /// builds exact cylinder walls from these.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outer_edges: Vec<RegionEdge>,
    /// Curve-aware hole boundaries (parallel to `holes`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hole_edges: Vec<Vec<RegionEdge>>,
}

/// One boundary edge of a region, in sketch UV coordinates. Mirrors the
/// kernel's `ProfileEdge` so a sub-region extrudes with true curved walls.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum RegionEdge {
    /// Straight segment `a → b`.
    Line { a: (f64, f64), b: (f64, f64) },
    /// Minor circular arc `a → b` (sweep `< π`) about `center` of `radius`.
    Arc {
        a: (f64, f64),
        b: (f64, f64),
        center: (f64, f64),
        radius: f64,
        ccw: bool,
    },
}

/// Default relative chord tolerance for tessellating curved boundaries.
pub const DEFAULT_CHORD_TOLERANCE: f64 = 1.0e-3;

/// A sliced face awaiting region classification: (outer area, outer, holes).
type Face = (f64, Vec<(f64, f64)>, Vec<Vec<(f64, f64)>>);

/// Compute every minimal closed face of the solved sketch.
///
/// `chord_tolerance` is the relative sagitta bound used to tessellate circles
/// and arcs (smaller ⇒ more segments). Construction entities are ignored.
pub fn compute_regions(
    entities: &[SketchEntity],
    positions: &HashMap<u32, (f64, f64)>,
    chord_tolerance: f64,
) -> Vec<Region> {
    // Tessellate every non-construction curve into a polyline ("string line").
    let mut strings: Vec<Vec<[f64; 2]>> = Vec::new();
    for entity in entities {
        if entity.is_construction() {
            continue;
        }
        if let Some(poly) = tessellate_entity(entity, positions, chord_tolerance) {
            if poly.len() >= 2 {
                strings.push(poly);
            }
        }
    }
    if strings.is_empty() {
        return Vec::new();
    }

    // Bounding box padded beyond all geometry, so the unbounded face is the
    // unique largest-area shape returned by the slice.
    let bbox = match bounding_box(&strings) {
        Some(b) => b,
        None => return Vec::new(),
    };

    let shapes = bbox.slice_by(&strings, FillRule::NonZero);
    if shapes.is_empty() {
        return Vec::new();
    }

    // Drop the background face (largest outer contour).
    // Each face: (outer area, outer loop, hole loops).
    let mut faces: Vec<Face> = Vec::new();
    for shape in &shapes {
        if shape.is_empty() {
            continue;
        }
        let outer = contour_to_uv(&shape[0]);
        if outer.len() < 3 {
            continue;
        }
        let outer_area = polygon_area_abs(&outer);
        let holes: Vec<Vec<(f64, f64)>> = shape[1..]
            .iter()
            .map(|c| contour_to_uv(c))
            .filter(|h| h.len() >= 3)
            .collect();
        faces.push((outer_area, outer, holes));
    }
    if faces.is_empty() {
        return Vec::new();
    }
    // Remove the single largest outer-area face (the bounding-box background).
    let bg_idx = faces
        .iter()
        .enumerate()
        .max_by(|a, b| {
            a.1 .0
                .partial_cmp(&b.1 .0)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i);
    if let Some(i) = bg_idx {
        faces.remove(i);
    }

    // Source circles/arcs, for recovering true-curve boundary edges.
    let circles = source_circles(entities, positions);

    // Canonical whole-entity profiles, for analytical provenance matching.
    let profiles = crate::profiles::extract_profiles(entities, positions);
    let profile_outlines: Vec<(f64, (f64, f64), &crate::sketch::ClosedProfile)> = profiles
        .iter()
        .filter_map(|p| {
            let poly = profile_outline(p, entities, positions);
            if poly.len() < 3 {
                return None;
            }
            Some((polygon_area_abs(&poly), polygon_centroid(&poly), p))
        })
        .collect();

    let mut regions = Vec::new();
    for (outer_area, outer, holes) in faces {
        let hole_area: f64 = holes.iter().map(|h| polygon_area_abs(h)).sum();
        let area = (outer_area - hole_area).max(0.0);
        if area <= AREA_EPS {
            continue;
        }

        // Analytical only when the face has no holes and matches a whole profile.
        let profile_entity_ids = if holes.is_empty() {
            let centroid = polygon_centroid(&outer);
            profile_outlines
                .iter()
                .find(|(pa, pc, _)| {
                    areas_match(*pa, outer_area) && points_close(*pc, centroid, *pa)
                })
                .map(|(_, _, p)| p.entity_ids.clone())
        } else {
            None
        };

        // Recover true-curve edges (arc runs on source circles → minor arcs).
        let outer_edges = recover_edges(&outer, &circles);
        let hole_edges: Vec<Vec<RegionEdge>> =
            holes.iter().map(|h| recover_edges(h, &circles)).collect();

        regions.push(Region {
            outer,
            holes,
            area,
            profile_entity_ids,
            outer_edges,
            hole_edges,
        });
    }
    regions
}

// ── true-curve recovery ─────────────────────────────────────────────────────

/// A source circle/arc carrier: any boundary point lying on it is on this circle.
struct CircleCurve {
    center: (f64, f64),
    radius: f64,
}

fn source_circles(
    entities: &[SketchEntity],
    positions: &HashMap<u32, (f64, f64)>,
) -> Vec<CircleCurve> {
    let mut out = Vec::new();
    for e in entities {
        if e.is_construction() {
            continue;
        }
        match e {
            SketchEntity::Circle {
                center_id, radius, ..
            } => {
                if let Some(&c) = positions.get(center_id) {
                    out.push(CircleCurve {
                        center: c,
                        radius: *radius,
                    });
                }
            }
            SketchEntity::Arc {
                center_id,
                start_id,
                ..
            } => {
                if let (Some(&c), Some(&s)) = (positions.get(center_id), positions.get(start_id)) {
                    let r = ((s.0 - c.0).powi(2) + (s.1 - c.1).powi(2)).sqrt();
                    if r > 0.0 {
                        out.push(CircleCurve {
                            center: c,
                            radius: r,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn signed_area(p: &[(f64, f64)]) -> f64 {
    let n = p.len();
    let mut a = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        a += p[i].0 * p[j].1 - p[j].0 * p[i].1;
    }
    a / 2.0
}

/// Maximum sweep per emitted arc edge. Strictly under π so every edge is the
/// MINOR arc the kernel assembler expects.
const MAX_ARC_SWEEP: f64 = 2.0;

/// One maximal run of consecutive boundary segments sharing a circle (or none).
struct Run {
    circle: Option<usize>,
    seg0: usize, // first segment index (= the run's start point index)
    len: usize,  // number of segments
}

/// Recover the curve-aware edge loop for a tessellated boundary `pts` (cyclic,
/// no repeated closing vertex). A segment lies on a circle when both endpoints
/// and its midpoint do; consecutive same-circle segments form an arc run that is
/// re-emitted as minor arcs with vertices placed EXACTLY on the circle (by
/// angle), and run boundaries placed at the exact circle∩circle intersection (or
/// projected onto the circle at an arc↔line junction). Straight segments stay
/// `Line`. The exactness is required by the kernel's arc validator.
fn recover_edges(pts: &[(f64, f64)], circles: &[CircleCurve]) -> Vec<RegionEdge> {
    if pts.len() < 2 {
        return Vec::new();
    }
    // The kernel's arc assembler takes EVERY loop CCW (it reverses holes itself),
    // so normalize a clockwise input (i_overlay's hole convention) to CCW first.
    let ccw_owned;
    let pts: &[(f64, f64)] = if signed_area(pts) < 0.0 {
        ccw_owned = pts.iter().rev().copied().collect::<Vec<_>>();
        &ccw_owned
    } else {
        pts
    };
    let n = pts.len();

    // Tag each cyclic segment with the circle it lies on, if any.
    let seg_circle: Vec<Option<usize>> = (0..n)
        .map(|i| {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            let mid = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
            circles.iter().position(|c| {
                // circle∩circle corners are chord-intersections ~sagitta inside,
                // so allow that slack; a straight line's midpoint is far off,
                // which is what rejects line segments.
                let end_tol = (5.0e-3 * c.radius).max(1.0e-9);
                let mid_tol = (5.0e-3 * c.radius).max(1.0e-9);
                (dist(a, c.center) - c.radius).abs() < end_tol
                    && (dist(b, c.center) - c.radius).abs() < end_tol
                    && (dist(mid, c.center) - c.radius).abs() < mid_tol
            })
        })
        .collect();

    // No arcs at all → straight loop (tessellation == exact for lines).
    if seg_circle.iter().all(|s| s.is_none()) {
        return (0..n)
            .map(|i| RegionEdge::Line {
                a: pts[i],
                b: pts[(i + 1) % n],
            })
            .collect();
    }

    // A single full-circle loop: split 2π into minor arcs, vertices on-circle.
    if seg_circle.iter().all(|s| *s == seg_circle[0]) {
        let c = &circles[seg_circle[0].unwrap()];
        let v0 = project_to_circle(pts[0], c);
        let sweep = loop_signed_sweep(pts, c);
        return split_arc(v0, v0, c, sweep);
    }

    // Build runs cyclically, rotating so a run never straddles index 0.
    let start = (0..n)
        .find(|&i| seg_circle[i] != seg_circle[(i + n - 1) % n])
        .unwrap_or(0);
    let mut runs: Vec<Run> = Vec::new();
    let mut k = 0;
    while k < n {
        let seg = (start + k) % n;
        let circle = seg_circle[seg];
        let mut len = 0;
        while k + len < n && seg_circle[(start + k + len) % n] == circle {
            len += 1;
        }
        runs.push(Run {
            circle,
            seg0: seg,
            len,
        });
        k += len;
    }

    // Exact boundary vertex between run[j-1] and run[j] (at point runs[j].seg0).
    let m = runs.len();
    let verts: Vec<(f64, f64)> = (0..m)
        .map(|j| {
            let prev = runs[(j + m - 1) % m].circle;
            let cur = runs[j].circle;
            exact_corner(pts[runs[j].seg0], prev, cur, circles)
        })
        .collect();

    // Emit each run between its exact boundary vertices.
    let mut edges = Vec::new();
    for j in 0..m {
        let run = &runs[j];
        let vs = verts[j];
        let ve = verts[(j + 1) % m];
        match run.circle {
            None => {
                // Straight run: keep interior tessellated vertices (e.g. polygon
                // corners), with exact endpoints at the run boundaries.
                let mut a = vs;
                for s in 1..run.len {
                    let p = pts[(run.seg0 + s) % n];
                    edges.push(RegionEdge::Line { a, b: p });
                    a = p;
                }
                edges.push(RegionEdge::Line { a, b: ve });
            }
            Some(cid) => {
                let c = &circles[cid];
                let sweep = run_signed_sweep(pts, n, run.seg0, run.len, c);
                edges.extend(split_arc(vs, ve, c, sweep));
            }
        }
    }
    edges
}

fn project_to_circle(p: (f64, f64), c: &CircleCurve) -> (f64, f64) {
    let d = (p.0 - c.center.0, p.1 - c.center.1);
    let len = (d.0 * d.0 + d.1 * d.1).sqrt();
    if len < 1.0e-300 {
        return (c.center.0 + c.radius, c.center.1);
    }
    (
        c.center.0 + c.radius * d.0 / len,
        c.center.1 + c.radius * d.1 / len,
    )
}

fn point_at_angle(c: &CircleCurve, theta: f64) -> (f64, f64) {
    (
        c.center.0 + c.radius * theta.cos(),
        c.center.1 + c.radius * theta.sin(),
    )
}

/// Signed sweep of a full-circle loop (≈ ±2π), summed from per-segment deltas.
fn loop_signed_sweep(pts: &[(f64, f64)], c: &CircleCurve) -> f64 {
    let n = pts.len();
    (0..n).map(|i| seg_delta(pts[i], pts[(i + 1) % n], c)).sum()
}

/// Signed sweep of an arc run (segments seg0..seg0+len on circle `c`).
fn run_signed_sweep(pts: &[(f64, f64)], n: usize, seg0: usize, len: usize, c: &CircleCurve) -> f64 {
    (0..len)
        .map(|j| {
            let i = (seg0 + j) % n;
            seg_delta(pts[i], pts[(i + 1) % n], c)
        })
        .sum()
}

fn seg_delta(a: (f64, f64), b: (f64, f64), c: &CircleCurve) -> f64 {
    let aa = (a.1 - c.center.1).atan2(a.0 - c.center.0);
    let ab = (b.1 - c.center.1).atan2(b.0 - c.center.0);
    let mut d = ab - aa;
    while d > std::f64::consts::PI {
        d -= std::f64::consts::TAU;
    }
    while d < -std::f64::consts::PI {
        d += std::f64::consts::TAU;
    }
    d
}

/// Split the arc on `c` from `vs` to `ve` (both exactly on `c`) sweeping `sweep`
/// into minor sub-arcs (< π each), interior vertices placed on-circle by angle
/// so the kernel's exact arc validator accepts them. `vs`/`ve` are used verbatim
/// as the run's exact boundary vertices.
fn split_arc(vs: (f64, f64), ve: (f64, f64), c: &CircleCurve, sweep: f64) -> Vec<RegionEdge> {
    let kk = ((sweep.abs() / MAX_ARC_SWEEP).ceil() as usize).max(1);
    let theta0 = (vs.1 - c.center.1).atan2(vs.0 - c.center.0);
    let ccw = sweep > 0.0;
    let mut out = Vec::with_capacity(kk);
    for i in 0..kk {
        let a = if i == 0 {
            vs
        } else {
            point_at_angle(c, theta0 + sweep * (i as f64 / kk as f64))
        };
        let b = if i + 1 == kk {
            ve
        } else {
            point_at_angle(c, theta0 + sweep * ((i + 1) as f64 / kk as f64))
        };
        if dist(a, b) < 1.0e-12 {
            continue;
        }
        out.push(RegionEdge::Arc {
            a,
            b,
            center: c.center,
            radius: c.radius,
            ccw,
        });
    }
    out
}

/// Exact run-boundary vertex near `tess` between a run on `prev` and a run on
/// `cur`. Arc↔arc → the circle∩circle intersection nearest `tess`; arc↔line →
/// the tessellated point projected onto the arc's circle; line↔line → `tess`.
fn exact_corner(
    tess: (f64, f64),
    prev: Option<usize>,
    cur: Option<usize>,
    circles: &[CircleCurve],
) -> (f64, f64) {
    match (prev, cur) {
        (Some(pa), Some(cb)) if pa != cb => {
            circle_intersection(&circles[pa], &circles[cb], tess).unwrap_or(tess)
        }
        (Some(ci), _) | (_, Some(ci)) => project_to_circle(tess, &circles[ci]),
        _ => tess,
    }
}

/// The intersection point of two circles nearest `near`, if they intersect.
fn circle_intersection(a: &CircleCurve, b: &CircleCurve, near: (f64, f64)) -> Option<(f64, f64)> {
    let dx = b.center.0 - a.center.0;
    let dy = b.center.1 - a.center.1;
    let d2 = dx * dx + dy * dy;
    let d = d2.sqrt();
    if d < 1.0e-12 || d > a.radius + b.radius || d < (a.radius - b.radius).abs() {
        return None;
    }
    // Distance from a.center to the radical line, and half-chord height.
    let aa = (a.radius * a.radius - b.radius * b.radius + d2) / (2.0 * d);
    let h2 = a.radius * a.radius - aa * aa;
    let h = if h2 > 0.0 { h2.sqrt() } else { 0.0 };
    let mx = a.center.0 + aa * dx / d;
    let my = a.center.1 + aa * dy / d;
    let ox = -dy / d * h;
    let oy = dx / d * h;
    let p1 = (mx + ox, my + oy);
    let p2 = (mx - ox, my - oy);
    Some(if dist(p1, near) <= dist(p2, near) {
        p1
    } else {
        p2
    })
}

// ── tessellation ──────────────────────────────────────────────────────────

fn tessellate_entity(
    entity: &SketchEntity,
    positions: &HashMap<u32, (f64, f64)>,
    chord_tolerance: f64,
) -> Option<Vec<[f64; 2]>> {
    match entity {
        SketchEntity::Line {
            start_id, end_id, ..
        } => {
            let a = positions.get(start_id)?;
            let b = positions.get(end_id)?;
            Some(vec![[a.0, a.1], [b.0, b.1]])
        }
        SketchEntity::Circle {
            center_id, radius, ..
        } => {
            let c = positions.get(center_id)?;
            let n = circle_segment_count(chord_tolerance);
            let mut pts = Vec::with_capacity(n + 1);
            for i in 0..=n {
                let a = std::f64::consts::TAU * (i as f64) / (n as f64);
                pts.push([c.0 + radius * a.cos(), c.1 + radius * a.sin()]);
            }
            Some(pts)
        }
        SketchEntity::Arc {
            center_id,
            start_id,
            end_id,
            ..
        } => {
            let c = positions.get(center_id)?;
            let s = positions.get(start_id)?;
            let e = positions.get(end_id)?;
            let radius = ((s.0 - c.0).powi(2) + (s.1 - c.1).powi(2)).sqrt();
            let start_angle = (s.1 - c.1).atan2(s.0 - c.0);
            let mut end_angle = (e.1 - c.1).atan2(e.0 - c.0);
            // CCW sweep (mirrors profiles.js arc sampling).
            if end_angle <= start_angle {
                end_angle += std::f64::consts::TAU;
            }
            let sweep = end_angle - start_angle;
            let full = circle_segment_count(chord_tolerance);
            let n = (((sweep / std::f64::consts::TAU) * full as f64).ceil() as usize).max(2);
            let mut pts = Vec::with_capacity(n + 1);
            for i in 0..=n {
                let t = i as f64 / n as f64;
                let a = start_angle + t * sweep;
                pts.push([c.0 + radius * a.cos(), c.1 + radius * a.sin()]);
            }
            Some(pts)
        }
        SketchEntity::Spline { point_ids, .. } => {
            let ctrl: Vec<(f64, f64)> = point_ids
                .iter()
                .filter_map(|id| positions.get(id).copied())
                .collect();
            if ctrl.len() < 2 {
                return None;
            }
            let samples = 16usize.max(ctrl.len() * 4);
            let mut pts = Vec::with_capacity(samples + 1);
            for i in 0..=samples {
                let t = i as f64 / samples as f64;
                let (x, y) = crate::bspline::evaluate_bspline(&ctrl, t, 3, None);
                pts.push([x, y]);
            }
            Some(pts)
        }
        _ => None,
    }
}

/// Segment count for a full circle from a relative chord (sagitta) tolerance.
/// Mirrors `kernel_v2::tessellate::circle_segment_count` (unreachable here).
fn circle_segment_count(rel_chord_tolerance: f64) -> usize {
    let rel = rel_chord_tolerance.clamp(1.0e-6, 0.5);
    let n = (std::f64::consts::PI / (1.0 - rel).acos()).ceil() as i64;
    n.clamp(8, 512) as usize
}

// ── geometry helpers (f64; exactness not required for selection) ────────────

const AREA_EPS: f64 = 1.0e-14;

fn bounding_box(strings: &[Vec<[f64; 2]>]) -> Option<Vec<[f64; 2]>> {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for poly in strings {
        for p in poly {
            if !p[0].is_finite() || !p[1].is_finite() {
                continue;
            }
            min[0] = min[0].min(p[0]);
            min[1] = min[1].min(p[1]);
            max[0] = max[0].max(p[0]);
            max[1] = max[1].max(p[1]);
        }
    }
    if !min[0].is_finite() || !max[0].is_finite() {
        return None;
    }
    let diag = ((max[0] - min[0]).powi(2) + (max[1] - min[1]).powi(2)).sqrt();
    let pad = (0.05 * diag).max(1.0e-6);
    Some(vec![
        [min[0] - pad, min[1] - pad],
        [max[0] + pad, min[1] - pad],
        [max[0] + pad, max[1] + pad],
        [min[0] - pad, max[1] + pad],
    ])
}

fn contour_to_uv(contour: &[[f64; 2]]) -> Vec<(f64, f64)> {
    contour.iter().map(|p| (p[0], p[1])).collect()
}

fn polygon_area_abs(poly: &[(f64, f64)]) -> f64 {
    let n = poly.len();
    if n < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        a += poly[i].0 * poly[j].1 - poly[j].0 * poly[i].1;
    }
    (a / 2.0).abs()
}

fn polygon_centroid(poly: &[(f64, f64)]) -> (f64, f64) {
    // Area-weighted centroid; falls back to vertex average for degenerate area.
    let n = poly.len();
    let mut a = 0.0;
    let mut cx = 0.0;
    let mut cy = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        let cross = poly[i].0 * poly[j].1 - poly[j].0 * poly[i].1;
        a += cross;
        cx += (poly[i].0 + poly[j].0) * cross;
        cy += (poly[i].1 + poly[j].1) * cross;
    }
    if a.abs() < AREA_EPS {
        let sx: f64 = poly.iter().map(|p| p.0).sum();
        let sy: f64 = poly.iter().map(|p| p.1).sum();
        return (sx / n as f64, sy / n as f64);
    }
    (cx / (3.0 * a), cy / (3.0 * a))
}

fn areas_match(a: f64, b: f64) -> bool {
    let m = a.max(b).max(1.0e-12);
    (a - b).abs() / m < 1.0e-2
}

fn points_close(a: (f64, f64), b: (f64, f64), area: f64) -> bool {
    // Tolerance scales with feature size (~sqrt(area)).
    let scale = area.max(1.0e-12).sqrt();
    let d = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    d < 1.0e-2 * scale.max(1.0e-9)
}

/// Build an outline polygon for a whole-entity profile (for analytical matching).
fn profile_outline(
    profile: &crate::sketch::ClosedProfile,
    entities: &[SketchEntity],
    positions: &HashMap<u32, (f64, f64)>,
) -> Vec<(f64, f64)> {
    if let Some(c) = &profile.circle {
        let n = 64;
        let mut pts = Vec::with_capacity(n);
        for i in 0..n {
            let a = std::f64::consts::TAU * (i as f64) / (n as f64);
            pts.push((
                c.center_u + c.radius * a.cos(),
                c.center_v + c.radius * a.sin(),
            ));
        }
        return pts;
    }
    if !profile.vertex_ids.is_empty() {
        return profile
            .vertex_ids
            .iter()
            .filter_map(|id| positions.get(id).copied())
            .collect();
    }
    // Fallback: use the start point of each line/arc entity in order.
    let mut pts = Vec::new();
    for eid in &profile.entity_ids {
        for e in entities {
            if e.id() != *eid {
                continue;
            }
            let start = match e {
                SketchEntity::Line { start_id, .. } | SketchEntity::Arc { start_id, .. } => {
                    Some(*start_id)
                }
                _ => None,
            };
            if let Some(sid) = start {
                if let Some(p) = positions.get(&sid) {
                    pts.push(*p);
                }
            }
        }
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(pairs: &[(u32, f64, f64)]) -> HashMap<u32, (f64, f64)> {
        pairs.iter().map(|(id, x, y)| (*id, (*x, *y))).collect()
    }

    fn circle_r(id: u32, center_id: u32, radius: f64) -> SketchEntity {
        SketchEntity::Circle {
            id,
            center_id,
            radius,
            construction: false,
        }
    }

    /// Is the point inside outer and outside every hole?
    fn region_contains(r: &Region, p: (f64, f64)) -> bool {
        point_in_poly(p, &r.outer) && r.holes.iter().all(|h| !point_in_poly(p, h))
    }

    fn point_in_poly(p: (f64, f64), poly: &[(f64, f64)]) -> bool {
        let mut inside = false;
        let n = poly.len();
        let mut j = n - 1;
        for i in 0..n {
            let (xi, yi) = poly[i];
            let (xj, yj) = poly[j];
            if (yi > p.1) != (yj > p.1) && p.0 < (xj - xi) * (p.1 - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    #[test]
    fn concentric_circles_yield_inner_disk_and_annulus() {
        let positions = pos(&[(1, 0.0, 0.0), (2, 0.0, 0.0)]);
        let entities = vec![circle_r(10, 1, 5.0), circle_r(20, 2, 2.0)];

        let regions = compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE);
        assert_eq!(regions.len(), 2, "expected inner disk + annulus");

        // Inner disk: contains origin, no holes, analytical (matches small circle).
        let inner = regions
            .iter()
            .find(|r| r.holes.is_empty())
            .expect("inner disk");
        assert!(region_contains(inner, (0.0, 0.0)));
        assert!((inner.area - std::f64::consts::PI * 4.0).abs() < 0.1);
        assert_eq!(inner.profile_entity_ids.as_deref(), Some(&[20u32][..]));

        // Annulus: one hole, contains a point at r=3.5, not analytical.
        let annulus = regions
            .iter()
            .find(|r| !r.holes.is_empty())
            .expect("annulus");
        assert!(region_contains(annulus, (3.5, 0.0)));
        assert!(!region_contains(annulus, (0.0, 0.0)));
        assert!((annulus.area - std::f64::consts::PI * (25.0 - 4.0)).abs() < 0.5);
        assert!(annulus.profile_entity_ids.is_none());

        // Smallest containing region at origin is the inner disk.
        let at_origin: Vec<_> = regions
            .iter()
            .filter(|r| region_contains(r, (0.0, 0.0)))
            .collect();
        assert_eq!(at_origin.len(), 1);
        assert!(at_origin[0].holes.is_empty());
    }

    #[test]
    fn crossing_circles_yield_lens_and_two_crescents() {
        let positions = pos(&[(1, -1.5, 0.0), (2, 1.5, 0.0)]);
        let entities = vec![circle_r(10, 1, 3.0), circle_r(20, 2, 3.0)];

        let regions = compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE);
        assert_eq!(regions.len(), 3, "expected lens + 2 crescents");
        // All are genuine sub-regions: none analytical.
        assert!(regions.iter().all(|r| r.profile_entity_ids.is_none()));

        // Center point (0,0) is in the lens (smallest region).
        let containing: Vec<_> = regions
            .iter()
            .filter(|r| region_contains(r, (0.0, 0.0)))
            .collect();
        assert_eq!(containing.len(), 1, "lens contains the center");
        let lens = containing[0];
        let smallest = regions
            .iter()
            .min_by(|a, b| a.area.partial_cmp(&b.area).unwrap())
            .unwrap();
        assert_eq!(lens.area, smallest.area, "lens is the smallest region");
    }

    #[test]
    fn nested_rectangles_yield_inner_and_frame() {
        // Outer rect (lines 10-13) and inner rect (lines 20-23).
        let positions = pos(&[
            (1, -10.0, -10.0),
            (2, 10.0, -10.0),
            (3, 10.0, 10.0),
            (4, -10.0, 10.0),
            (5, -4.0, -4.0),
            (6, 4.0, -4.0),
            (7, 4.0, 4.0),
            (8, -4.0, 4.0),
        ]);
        let line = |id, a, b| SketchEntity::Line {
            id,
            start_id: a,
            end_id: b,
            construction: false,
        };
        let entities = vec![
            line(10, 1, 2),
            line(11, 2, 3),
            line(12, 3, 4),
            line(13, 4, 1),
            line(20, 5, 6),
            line(21, 6, 7),
            line(22, 7, 8),
            line(23, 8, 5),
        ];

        let regions = compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE);
        assert_eq!(regions.len(), 2, "inner rect + frame");

        let inner = regions.iter().find(|r| r.holes.is_empty()).expect("inner");
        assert!(region_contains(inner, (0.0, 0.0)));
        assert!((inner.area - 64.0).abs() < 1e-6);
        // Inner rect matches a whole 4-line profile.
        assert!(inner.profile_entity_ids.is_some());

        let frame = regions.iter().find(|r| !r.holes.is_empty()).expect("frame");
        assert!(region_contains(frame, (7.0, 7.0)));
        assert!(!region_contains(frame, (0.0, 0.0)));
        assert!((frame.area - (400.0 - 64.0)).abs() < 1e-6);
        assert!(frame.profile_entity_ids.is_none());
    }

    #[test]
    fn lone_circle_is_one_analytical_region() {
        let positions = pos(&[(1, 0.0, 0.0)]);
        let entities = vec![circle_r(10, 1, 3.0)];
        let regions = compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE);
        assert_eq!(regions.len(), 1);
        assert!(regions[0].holes.is_empty());
        assert_eq!(regions[0].profile_entity_ids.as_deref(), Some(&[10u32][..]));
    }

    #[test]
    fn construction_entities_ignored() {
        let positions = pos(&[(1, 0.0, 0.0)]);
        let entities = vec![SketchEntity::Circle {
            id: 10,
            center_id: 1,
            radius: 3.0,
            construction: true,
        }];
        assert!(compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE).is_empty());
    }

    #[test]
    fn empty_sketch_yields_no_regions() {
        let positions = HashMap::new();
        assert!(compute_regions(&[], &positions, DEFAULT_CHORD_TOLERANCE).is_empty());
    }

    // ── true-curve recovery ────────────────────────────────────────────

    fn arc_count(edges: &[RegionEdge]) -> usize {
        edges
            .iter()
            .filter(|e| matches!(e, RegionEdge::Arc { .. }))
            .count()
    }
    fn line_count(edges: &[RegionEdge]) -> usize {
        edges
            .iter()
            .filter(|e| matches!(e, RegionEdge::Line { .. }))
            .count()
    }
    /// Every arc edge must be a minor arc (sweep < π) for the kernel assembler,
    /// with both endpoints exactly on its stated circle.
    fn assert_arcs_valid(edges: &[RegionEdge]) {
        for e in edges {
            if let RegionEdge::Arc {
                a,
                b,
                center,
                radius,
                ..
            } = e
            {
                let on = |p: &(f64, f64)| {
                    let d = (((p.0 - center.0).powi(2) + (p.1 - center.1).powi(2)).sqrt() - radius)
                        .abs();
                    assert!(d < 1e-9 * radius.max(1.0), "arc endpoint off circle by {d}");
                };
                on(a);
                on(b);
                let ang = |p: &(f64, f64)| (p.1 - center.1).atan2(p.0 - center.0);
                let mut d = (ang(b) - ang(a)).abs();
                if d > std::f64::consts::PI {
                    d = std::f64::consts::TAU - d;
                }
                assert!(d < std::f64::consts::PI, "arc sweep {d} must be < π");
            }
        }
    }

    #[test]
    fn annulus_outer_and_hole_are_all_arcs_minor_on_circle() {
        let positions = pos(&[(1, 0.0, 0.0), (2, 0.0, 0.0)]);
        let entities = vec![circle_r(10, 1, 5.0), circle_r(20, 2, 2.0)];
        let regions = compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE);
        let annulus = regions
            .iter()
            .find(|r| !r.holes.is_empty())
            .expect("annulus");

        assert_eq!(
            line_count(&annulus.outer_edges),
            0,
            "circle outer has no line edges"
        );
        assert!(
            arc_count(&annulus.outer_edges) >= 3,
            "full circle splits into ≥3 minor arcs"
        );
        assert_arcs_valid(&annulus.outer_edges);

        assert_eq!(annulus.hole_edges.len(), 1);
        assert_eq!(line_count(&annulus.hole_edges[0]), 0);
        assert!(arc_count(&annulus.hole_edges[0]) >= 3);
        assert_arcs_valid(&annulus.hole_edges[0]);
    }

    #[test]
    fn lens_boundary_is_arcs_only() {
        let positions = pos(&[(1, -1.5, 0.0), (2, 1.5, 0.0)]);
        let entities = vec![circle_r(10, 1, 3.0), circle_r(20, 2, 3.0)];
        let regions = compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE);
        let lens = regions
            .iter()
            .min_by(|a, b| a.area.partial_cmp(&b.area).unwrap())
            .unwrap();
        assert_eq!(line_count(&lens.outer_edges), 0, "lens has no line edges");
        assert!(arc_count(&lens.outer_edges) >= 2, "lens has ≥2 arc edges");
        assert_arcs_valid(&lens.outer_edges);
    }

    #[test]
    fn crescent_major_arc_is_split_minor() {
        let positions = pos(&[(1, -1.5, 0.0), (2, 1.5, 0.0)]);
        let entities = vec![circle_r(10, 1, 3.0), circle_r(20, 2, 3.0)];
        let regions = compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE);
        let mut by_area: Vec<&Region> = regions.iter().collect();
        by_area.sort_by(|a, b| b.area.partial_cmp(&a.area).unwrap());
        for crescent in by_area.iter().take(2) {
            assert_eq!(
                line_count(&crescent.outer_edges),
                0,
                "crescent has no lines"
            );
            assert!(arc_count(&crescent.outer_edges) >= 2);
            assert_arcs_valid(&crescent.outer_edges);
        }
    }

    #[test]
    fn nested_rectangle_edges_are_all_lines() {
        let positions = pos(&[
            (1, -10.0, -10.0),
            (2, 10.0, -10.0),
            (3, 10.0, 10.0),
            (4, -10.0, 10.0),
            (5, -4.0, -4.0),
            (6, 4.0, -4.0),
            (7, 4.0, 4.0),
            (8, -4.0, 4.0),
        ]);
        let line = |id, a, b| SketchEntity::Line {
            id,
            start_id: a,
            end_id: b,
            construction: false,
        };
        let entities = vec![
            line(10, 1, 2),
            line(11, 2, 3),
            line(12, 3, 4),
            line(13, 4, 1),
            line(20, 5, 6),
            line(21, 6, 7),
            line(22, 7, 8),
            line(23, 8, 5),
        ];
        let regions = compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE);
        for r in &regions {
            assert_eq!(arc_count(&r.outer_edges), 0, "rectangles have no arcs");
            assert!(line_count(&r.outer_edges) >= 4);
        }
    }
}
