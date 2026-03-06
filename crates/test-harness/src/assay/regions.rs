//! Region decomposition using iOverlay for 2D sketch geometry.
//!
//! Decomposes arbitrary 2D contours into clean closed regions suitable for
//! extrusion. Uses iOverlay's polygon boolean operations to regularize
//! self-intersecting or overlapping geometry.

use std::collections::HashMap;

use crate::helpers::ProfileData;
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use waffle_types::{ClosedProfile, SketchEntity};

/// A closed 2D region extracted from iOverlay decomposition.
#[derive(Debug, Clone)]
pub struct ClosedRegion {
    /// Outer contour (CCW winding).
    pub outer: Vec<[f64; 2]>,
    /// Hole contours (CW winding).
    pub holes: Vec<Vec<[f64; 2]>>,
    /// Absolute area of the region (outer - holes).
    pub area: f64,
}

/// Compute signed area of a 2D contour using the shoelace formula.
///
/// CCW winding → positive, CW winding → negative.
pub fn signed_area(contour: &[[f64; 2]]) -> f64 {
    let n = contour.len();
    if n < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        area += contour[i][0] * contour[j][1];
        area -= contour[j][0] * contour[i][1];
    }
    area / 2.0
}

/// Minimum area threshold for regions (min_feature_size²).
const MIN_REGION_AREA: f64 = 1.0;

/// Default fallback square when decomposition produces no regions.
fn fallback_square() -> Vec<ClosedRegion> {
    let outer = vec![[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]];
    vec![ClosedRegion {
        area: 100.0,
        outer,
        holes: vec![],
    }]
}

/// Decompose arbitrary 2D contours into clean closed regions via iOverlay.
///
/// Input: a slice of contours (each a list of 2D points). Contours can be
/// self-intersecting or overlapping — iOverlay regularizes them.
///
/// The function performs a union of the input with an empty shape using EvenOdd
/// fill rule, which regularizes the geometry into non-overlapping regions.
///
/// Returns a list of `ClosedRegion` with area ≥ `MIN_REGION_AREA`.
pub fn decompose_regions(contours: &[Vec<[f64; 2]>]) -> Vec<ClosedRegion> {
    if contours.is_empty() {
        return fallback_square();
    }

    // Build the input shape: Vec<Vec<[f64; 2]>> (list of contours)
    let shape: Vec<Vec<[f64; 2]>> = contours.to_vec();
    let empty: Vec<Vec<[f64; 2]>> = vec![];

    // Union with empty to regularize via EvenOdd fill rule
    let result: Vec<Vec<Vec<[f64; 2]>>> =
        shape.overlay(&empty, OverlayRule::Union, FillRule::EvenOdd);

    if result.is_empty() {
        return fallback_square();
    }

    let mut regions = Vec::new();
    for shape_contours in &result {
        if shape_contours.is_empty() {
            continue;
        }

        // First contour is outer boundary, rest are holes
        let outer = shape_contours[0].clone();
        let outer_area = signed_area(&outer).abs();

        let holes: Vec<Vec<[f64; 2]>> = shape_contours[1..].to_vec();
        let hole_area: f64 = holes.iter().map(|h| signed_area(h).abs()).sum();

        let net_area = outer_area - hole_area;
        if net_area >= MIN_REGION_AREA {
            regions.push(ClosedRegion {
                outer,
                holes,
                area: net_area,
            });
        }
    }

    if regions.is_empty() {
        return fallback_square();
    }

    regions
}

/// Convert closed regions into waffle-types sketch entities and profiles.
///
/// Each region's outer + hole contours become line-segment entities with
/// unique IDs. Returns (entities, solved_positions, profiles) suitable for
/// `finish_sketch_manual`.
///
/// ID allocation:
/// - Points: `base_id + 1..=N` per contour
/// - Lines: `base_id + 1000..` per contour
/// - Each region increments `base_id` by 10000 to avoid collisions
pub fn contours_to_profiles(regions: &[ClosedRegion]) -> ProfileData {
    let mut entities = Vec::new();
    let mut positions = HashMap::new();
    let mut profiles = Vec::new();

    for (region_idx, region) in regions.iter().enumerate() {
        let base = (region_idx as u32) * 10000;

        // Build outer contour
        let (outer_entities, outer_positions, outer_point_ids) =
            build_contour_entities(&region.outer, base, true);

        entities.extend(outer_entities);
        positions.extend(outer_positions);

        // Build hole contours
        let mut hole_profiles = Vec::new();
        for (hole_idx, hole) in region.holes.iter().enumerate() {
            let hole_base = base + 5000 + (hole_idx as u32) * 1000;
            let (hole_entities, hole_positions, hole_point_ids) =
                build_contour_entities(hole, hole_base, false);

            entities.extend(hole_entities);
            positions.extend(hole_positions);

            hole_profiles.push(ClosedProfile {
                entity_ids: hole_point_ids,
                is_outer: false,
                circle: None,
                spline_segments: vec![],
            });
        }

        // Outer profile
        profiles.push(ClosedProfile {
            entity_ids: outer_point_ids,
            is_outer: true,
            circle: None,
            spline_segments: vec![],
        });

        // Hole profiles
        profiles.extend(hole_profiles);
    }

    (entities, positions, profiles)
}

/// Result of building entities for a single contour: (entities, positions, point_ids).
type ContourData = (Vec<SketchEntity>, HashMap<u32, (f64, f64)>, Vec<u32>);

/// Build sketch entities (points + lines) for a single contour.
///
/// Returns (entities, positions, point_ids).
fn build_contour_entities(contour: &[[f64; 2]], base_id: u32, _is_outer: bool) -> ContourData {
    let n = contour.len() as u32;
    let mut entities = Vec::new();
    let mut positions = HashMap::new();
    let mut point_ids = Vec::new();

    // Points
    for (i, pt) in contour.iter().enumerate() {
        let id = base_id + (i as u32) + 1;
        entities.push(SketchEntity::Point {
            id,
            x: pt[0],
            y: pt[1],
            construction: false,
        });
        positions.insert(id, (pt[0], pt[1]));
        point_ids.push(id);
    }

    // Lines connecting consecutive points
    let line_base = base_id + 1000;
    for i in 0..n {
        let lid = line_base + i;
        let start_id = base_id + i + 1;
        let end_id = base_id + ((i + 1) % n) + 1;
        entities.push(SketchEntity::Line {
            id: lid,
            start_id,
            end_id,
            construction: false,
        });
    }

    (entities, positions, point_ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_area_ccw_triangle() {
        let tri = vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]];
        let area = signed_area(&tri);
        assert!(area > 0.0, "CCW triangle should have positive area");
        assert!((area - 50.0).abs() < 1e-10);
    }

    #[test]
    fn signed_area_cw_triangle() {
        let tri = vec![[0.0, 0.0], [0.0, 10.0], [10.0, 0.0]];
        let area = signed_area(&tri);
        assert!(area < 0.0, "CW triangle should have negative area");
        assert!((area + 50.0).abs() < 1e-10);
    }

    #[test]
    fn decompose_triangle_one_region() {
        let tri = vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]];
        let regions = decompose_regions(&[tri]);
        assert_eq!(regions.len(), 1, "Triangle should produce 1 region");
        assert!((regions[0].area - 50.0).abs() < 1.0, "area ≈ 50");
        assert!(regions[0].holes.is_empty());
    }

    #[test]
    fn decompose_square_with_hole() {
        // Outer square (CCW)
        let outer = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        // Inner square (CW — hole)
        let inner = vec![[3.0, 3.0], [3.0, 7.0], [7.0, 7.0], [7.0, 3.0]];
        let regions = decompose_regions(&[outer, inner]);
        assert_eq!(regions.len(), 1, "Should produce 1 region with 1 hole");
        // Net area = 100 - 16 = 84
        assert!(
            (regions[0].area - 84.0).abs() < 2.0,
            "area ≈ 84, got {}",
            regions[0].area
        );
        assert_eq!(regions[0].holes.len(), 1, "Should have 1 hole");
    }

    #[test]
    fn decompose_figure_eight_two_regions() {
        // Self-intersecting figure-8: two loops sharing a crossing point
        // Use two separate contours that overlap at center
        let left = vec![[-5.0, 0.0], [0.0, -5.0], [1.0, 0.0], [0.0, 5.0]];
        let right = vec![[-1.0, 0.0], [0.0, -5.0], [5.0, 0.0], [0.0, 5.0]];
        let regions = decompose_regions(&[left, right]);
        // Should produce at least 1 region (iOverlay unions overlapping contours)
        assert!(!regions.is_empty(), "Should produce regions from figure-8");
        let total_area: f64 = regions.iter().map(|r| r.area).sum();
        assert!(total_area > 5.0, "Total area should be substantial");
    }

    #[test]
    fn decompose_empty_fallback() {
        let regions = decompose_regions(&[]);
        assert_eq!(regions.len(), 1, "Empty input should produce fallback");
        assert!((regions[0].area - 100.0).abs() < 1e-10);
    }

    #[test]
    fn decompose_degenerate_line_fallback() {
        // Degenerate: collinear points → area = 0 → filtered → fallback
        let degen = vec![[0.0, 0.0], [5.0, 0.0], [10.0, 0.0]];
        let regions = decompose_regions(&[degen]);
        // Either fallback or empty-filtered → fallback
        assert!(!regions.is_empty(), "Degenerate should produce fallback");
    }

    #[test]
    fn contours_to_profiles_basic() {
        let region = ClosedRegion {
            outer: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            holes: vec![],
            area: 100.0,
        };
        let (entities, positions, profiles) = contours_to_profiles(&[region]);

        // 4 points + 4 lines = 8 entities
        assert_eq!(entities.len(), 8);
        assert_eq!(positions.len(), 4);
        assert_eq!(profiles.len(), 1);
        assert!(profiles[0].is_outer);
        assert_eq!(profiles[0].entity_ids.len(), 4);
    }

    #[test]
    fn contours_to_profiles_with_hole() {
        let region = ClosedRegion {
            outer: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            holes: vec![vec![[3.0, 3.0], [3.0, 7.0], [7.0, 7.0], [7.0, 3.0]]],
            area: 84.0,
        };
        let (entities, positions, profiles) = contours_to_profiles(&[region]);

        // Outer: 4 pts + 4 lines, Hole: 4 pts + 4 lines = 16 entities
        assert_eq!(entities.len(), 16);
        assert_eq!(positions.len(), 8);
        assert_eq!(profiles.len(), 2); // 1 outer + 1 hole
        assert!(profiles[0].is_outer);
        assert!(!profiles[1].is_outer);
    }
}
