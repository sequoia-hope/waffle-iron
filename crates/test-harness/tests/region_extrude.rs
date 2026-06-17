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
    // A holed prism has inner walls: more than a plain box's 6 faces.
    assert!(
        mesh.face_ranges.len() > 6,
        "annulus prism should have inner walls (>6 faces), got {}",
        mesh.face_ranges.len()
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
}
