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
//! (annulus, lens, crescent) have `profile_entity_ids == None` and are extruded
//! from their explicit tessellated boundary — curved sub-region boundaries are
//! therefore faceted, an accepted and documented limitation.

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
    /// interior contains the click.
    pub area: f64,
    /// When `Some`, this region equals one whole-entity profile and has no holes:
    /// these are that profile's `entity_ids`. The UI resolves them to a
    /// `profile_index` and uses the existing analytical extrude path. When
    /// `None`, the region is a genuine sub-region and must be extruded from its
    /// explicit `outer`/`holes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_entity_ids: Option<Vec<u32>>,
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

        regions.push(Region {
            outer,
            holes,
            area,
            profile_entity_ids,
        });
    }
    regions
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
}
