//! Region → extrude integration (kernel-v2): a minimal sub-region of
//! overlapping sketch shapes extrudes into the expected solid.
//!
//! Covers the new path added across waffle-types `compute_regions`, the
//! `ExtrudeParams.region` field, and `KernelBundle::make_face_from_region`.

use std::collections::HashMap;

use feature_engine::types::{DepthMode, ExtrudeParams};
use test_harness::workflow::ModelBuilder;
use uuid::Uuid;
use waffle_types::{compute_regions, regions::DEFAULT_CHORD_TOLERANCE, Region, SketchEntity};

fn circle(id: u32, center_id: u32, radius: f64) -> SketchEntity {
    SketchEntity::Circle {
        id,
        center_id,
        radius,
        construction: false,
    }
}

fn region_params(region: Region, depth: f64) -> ExtrudeParams {
    ExtrudeParams {
        combine: None,
        targets: None,
        sketch_id: Uuid::nil(), // overridden by extrude_advanced
        profile_index: 0,
        depth,
        direction: None,
        symmetric: false,
        cut: false,
        merge: true,
        target_body: None,
        depth_mode: DepthMode::Blind,
        second_direction: None,
        region: Some(region),
        regions: Vec::new(),
        depth_expr: None,
    }
}

/// Build a single sketch from `entities`/`positions` and extrude the given
/// region `depth` deep, returning the resulting body mesh.
fn extrude_region(
    entities: &[SketchEntity],
    positions: &HashMap<u32, (f64, f64)>,
    region: Region,
    depth: f64,
) -> waffle_types::kernel::RenderMesh {
    let mut m = ModelBuilder::kernel_v2();
    m.begin_sketch([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    m.finish_sketch_manual(
        "sk",
        positions.clone(),
        waffle_types::extract_profiles(entities, positions),
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
    )
    .unwrap();
    m.extrude_advanced("region", "sk", region_params(region, depth))
        .unwrap();
    m.assert_no_errors().unwrap();
    m.tessellate_last().unwrap()
}

/// Concentric circles → the annulus sub-region extrudes into a prism with a
/// through-hole (inner walls present), volume = annulus area × depth.
#[test]
fn annulus_region_extrudes_to_holed_prism() {
    let positions: HashMap<u32, (f64, f64)> = [(1u32, (0.0, 0.0)), (2u32, (0.0, 0.0))]
        .into_iter()
        .collect();
    let entities = vec![circle(10, 1, 5.0), circle(20, 2, 2.0)];

    let annulus = compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE)
        .into_iter()
        .find(|r| !r.holes.is_empty())
        .expect("annulus region with a hole");
    let expected_volume = annulus.area * 10.0;

    let mesh = extrude_region(&entities, &positions, annulus, 10.0);
    let volume = test_harness::helpers::mesh_signed_volume(&mesh).abs();
    assert!(
        (volume - expected_volume).abs() / expected_volume < 1e-3,
        "annulus volume {volume} should match area×depth {expected_volume}"
    );
    // TRUE CURVES: outer + inner walls are a handful of exact cylinder patches
    // (a few per circle) + 2 caps — NOT a faceted prism (which would be ~140
    // faces at this tessellation). Inner walls present ⇒ > 6 faces.
    let faces = mesh.face_ranges.len();
    assert!(
        (7..=24).contains(&faces),
        "annulus should be a holed solid with exact cylinder walls (got {faces} faces)"
    );
}

/// Crossing circles → the lens sub-region extrudes into a single solid,
/// volume = lens area × depth.
#[test]
fn lens_region_extrudes_to_solid() {
    let positions: HashMap<u32, (f64, f64)> = [(1u32, (-1.5, 0.0)), (2u32, (1.5, 0.0))]
        .into_iter()
        .collect();
    let entities = vec![circle(10, 1, 3.0), circle(20, 2, 3.0)];

    let lens = compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE)
        .into_iter()
        .min_by(|a, b| a.area.partial_cmp(&b.area).unwrap())
        .expect("lens is the smallest region");
    assert!(lens.profile_entity_ids.is_none(), "lens is a sub-region");
    let expected_volume = lens.area * 4.0;

    let mesh = extrude_region(&entities, &positions, lens, 4.0);
    let volume = test_harness::helpers::mesh_signed_volume(&mesh).abs();
    assert!(
        (volume - expected_volume).abs() / expected_volume < 1e-3,
        "lens volume {volume} should match area×depth {expected_volume}"
    );
    // TRUE CURVES: the lens is bounded by two exact cylinder walls (each maybe
    // split once) + 2 caps — a single-digit face count, not a faceted prism.
    let faces = mesh.face_ranges.len();
    assert!(
        faces <= 10,
        "lens should have exact cylinder walls (got {faces} faces)"
    );
}

fn point(id: u32, x: f64, y: f64, positions: &mut HashMap<u32, (f64, f64)>) -> SketchEntity {
    positions.insert(id, (x, y));
    SketchEntity::Point {
        id,
        x,
        y,
        construction: false,
    }
}

fn line(id: u32, start_id: u32, end_id: u32) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id,
        end_id,
        construction: false,
    }
}

/// The user's reported scenario: a rectangle with a line through it is two
/// selectable regions. Extruding BOTH (the multi-region union path) must produce
/// ONE merged body — the whole rectangle prism — NOT just one half. The two
/// halves share the dividing wall AND have collinear (coplanar) outer side
/// walls, so a naive 3D boolean union would hit the Yang Stage-0 coplanar wall;
/// the fix unions their 2D footprints first, so there is no 3D boolean at all.
#[test]
fn split_rectangle_multi_region_extrudes_one_merged_body() {
    let mut positions: HashMap<u32, (f64, f64)> = HashMap::new();
    // 10×10 rectangle, split by a horizontal line at y = 5.
    let entities = vec![
        point(1, 0.0, 0.0, &mut positions),
        point(2, 10.0, 0.0, &mut positions),
        point(3, 10.0, 10.0, &mut positions),
        point(4, 0.0, 10.0, &mut positions),
        point(5, 0.0, 5.0, &mut positions),
        point(6, 10.0, 5.0, &mut positions),
        line(10, 1, 2),
        line(11, 2, 3),
        line(12, 3, 4),
        line(13, 4, 1),
        line(14, 5, 6), // the divider
    ];

    let regions = compute_regions(&entities, &positions, DEFAULT_CHORD_TOLERANCE);
    assert_eq!(regions.len(), 2, "rectangle + divider = two regions");

    // Extrude BOTH regions in one operation via the multi-region path.
    let depth = 3.0;
    let params = ExtrudeParams {
        combine: None,
        targets: None,
        sketch_id: Uuid::nil(),
        profile_index: 0,
        depth,
        direction: None,
        symmetric: false,
        cut: false,
        merge: true,
        target_body: None,
        depth_mode: DepthMode::Blind,
        second_direction: None,
        region: None,
        regions: regions.clone(),
        depth_expr: None,
    };

    let mut m = ModelBuilder::kernel_v2();
    m.begin_sketch([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    m.finish_sketch_manual(
        "sk",
        positions.clone(),
        waffle_types::extract_profiles(&entities, &positions),
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
    )
    .unwrap();
    m.extrude_advanced("both", "sk", params).unwrap();
    m.assert_no_errors().unwrap();

    // ONE merged body, not two; neither half dropped.
    assert_eq!(m.distinct_solid_count(), 1, "must be a single merged body");

    let mesh = m.tessellate_last().unwrap();
    let volume = test_harness::helpers::mesh_signed_volume(&mesh).abs();
    let expected = 10.0 * 10.0 * depth; // the WHOLE rectangle, both halves
    assert!(
        (volume - expected).abs() / expected < 1e-3,
        "merged body volume {volume} should be the full rectangle {expected} \
         (got the dropped-region symptom if it is ~half = {})",
        expected / 2.0
    );
}
