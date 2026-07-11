//! Isolated-ring twin dedup (user case step_extrude.waffle, task #139
//! follow-up): a closed degree-2 ring of lines+arcs yields TWO minimal
//! faces over the SAME entity set — one per traversal direction. With arcs
//! reduced to chords the walk cannot tell the bounded face from the
//! unbounded one, so both used to survive as profiles; the CW twin then
//! corrupted kernel staging (ProfileRepeatedVertex from the always-forward
//! densifier, NewellMismatch from mis-grouped loops). extract_profiles must
//! return exactly ONE CCW profile per isolated ring, regardless of the
//! stored winding of its entities.

use std::collections::HashMap;

use waffle_types::profiles::extract_profiles;
use waffle_types::SketchEntity;

fn line(id: u32, start_id: u32, end_id: u32) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id,
        end_id,
        construction: false,
    }
}

fn arc(id: u32, center_id: u32, start_id: u32, end_id: u32) -> SketchEntity {
    SketchEntity::Arc {
        id,
        center_id,
        start_id,
        end_id,
        construction: false,
    }
}

/// A rounded square wound CLOCKWISE (the offset tool pre-normalization
/// shape): 4 lines + 4 corner arcs, entity direction following the CW walk,
/// arcs stored CCW start→end per the entity convention (so their endpoints
/// are swapped relative to the ring direction).
fn cw_rounded_square() -> (Vec<SketchEntity>, HashMap<u32, (f64, f64)>) {
    let r = 0.002_f64; // corner radius
    let h = 0.01_f64; // half side
                      // Joint points, walking CW from the top-left tangent of the top edge:
                      // top edge R→L is CW in a y-up sketch when we start at top-right.
    let mut positions: HashMap<u32, (f64, f64)> = HashMap::new();
    let mut entities: Vec<SketchEntity> = Vec::new();

    // 8 tangent points (two per side), 4 arc centers.
    // Sides at ±h; tangents inset by r.
    let pts = [
        (h - r, h),   // 1: top edge, right tangent
        (-h + r, h),  // 2: top edge, left tangent
        (-h, h - r),  // 3: left edge, top tangent
        (-h, -h + r), // 4: left edge, bottom tangent
        (-h + r, -h), // 5: bottom edge, left tangent
        (h - r, -h),  // 6: bottom edge, right tangent
        (h, -h + r),  // 7: right edge, bottom tangent
        (h, h - r),   // 8: right edge, top tangent
    ];
    for (i, p) in pts.iter().enumerate() {
        positions.insert(i as u32 + 1, *p);
    }
    let centers = [
        (-h + r, h - r),  // 11: top-left
        (-h + r, -h + r), // 12: bottom-left
        (h - r, -h + r),  // 13: bottom-right
        (h - r, h - r),   // 14: top-right
    ];
    for (i, c) in centers.iter().enumerate() {
        positions.insert(i as u32 + 11, *c);
    }

    // CW ring: 1→2 (top edge leftwards), arc 2→3 (top-left corner),
    // 3→4, arc 4→5, 5→6, arc 6→7, 7→8, arc 8→1.
    // Lines stored in walk direction; arcs stored CCW start→end, which for
    // a CW walk means endpoints swapped (walk enters at the entity's END).
    entities.push(line(21, 1, 2));
    entities.push(arc(22, 11, 3, 2)); // CCW 3→2; walked 2→3 (reversed)
    entities.push(line(23, 3, 4));
    entities.push(arc(24, 12, 5, 4)); // walked 4→5 (reversed)
    entities.push(line(25, 5, 6));
    entities.push(arc(26, 13, 7, 6)); // walked 6→7 (reversed)
    entities.push(line(27, 7, 8));
    entities.push(arc(28, 14, 1, 8)); // walked 8→1 (reversed)

    (entities, positions)
}

#[test]
fn cw_ring_yields_exactly_one_ccw_profile() {
    let (entities, positions) = cw_rounded_square();
    let profiles = extract_profiles(&entities, &positions);
    assert_eq!(
        profiles.len(),
        1,
        "an isolated ring must produce ONE profile, got {}: {:?}",
        profiles.len(),
        profiles
            .iter()
            .map(|p| (&p.entity_ids, p.is_outer))
            .collect::<Vec<_>>()
    );
    assert!(
        profiles[0].is_outer,
        "the surviving twin must be the CCW (outer) walk"
    );
    assert_eq!(profiles[0].entity_ids.len(), 8);
}

#[test]
fn two_disjoint_rings_yield_one_profile_each() {
    let (mut entities, mut positions) = cw_rounded_square();
    // Second ring: same shape translated +0.05 in x, ids offset by 100.
    let (ents2, pos2) = cw_rounded_square();
    for e in ents2 {
        match e {
            SketchEntity::Line {
                id,
                start_id,
                end_id,
                construction,
            } => entities.push(SketchEntity::Line {
                id: id + 100,
                start_id: start_id + 100,
                end_id: end_id + 100,
                construction,
            }),
            SketchEntity::Arc {
                id,
                center_id,
                start_id,
                end_id,
                construction,
            } => entities.push(SketchEntity::Arc {
                id: id + 100,
                center_id: center_id + 100,
                start_id: start_id + 100,
                end_id: end_id + 100,
                construction,
            }),
            _ => {}
        }
    }
    for (id, (x, y)) in pos2 {
        positions.insert(id + 100, (x + 0.05, y));
    }

    let profiles = extract_profiles(&entities, &positions);
    assert_eq!(
        profiles.len(),
        2,
        "two disjoint rings must produce two profiles, got {}",
        profiles.len()
    );
    assert!(profiles.iter().all(|p| p.is_outer));
}
