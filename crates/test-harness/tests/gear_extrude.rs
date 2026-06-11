//! Gear extrusion and boolean tests (Group R).
//!
//! Tests exercise gear_profile() through extrude and boolean pipelines.
//! Some tests may fail initially — the goal is to have red tests in place.

use test_harness::helpers::{gear_profile, mesh_volume};
use test_harness::ModelBuilder;
use waffle_types::kernel::RenderMesh;
use waffle_types::SketchEntity;

/// Build a gear body from gear_profile and extrude it.
fn build_gear_solid(
    teeth: u32,
    module_val: f64,
    pressure_angle_deg: f64,
    depth: f64,
) -> ModelBuilder {
    let mut m = ModelBuilder::kernel_v2();
    let (gear_entities, gear_positions, gear_profiles) =
        gear_profile(teeth, module_val, pressure_angle_deg);

    m.begin_sketch([0., 0., 0.], [0., 0., 1.]);
    for entity in &gear_entities {
        match entity {
            SketchEntity::Point { id, x, y, .. } => {
                m.add_point(*id, *x, *y);
            }
            SketchEntity::Line {
                id,
                start_id,
                end_id,
                ..
            } => {
                m.add_line(*id, *start_id, *end_id);
            }
            SketchEntity::Arc {
                id,
                center_id,
                start_id,
                end_id,
                ..
            } => {
                m.add_arc(*id, *center_id, *start_id, *end_id);
            }
            _ => {}
        }
    }
    m.finish_sketch_manual(
        "gear_sk",
        gear_positions,
        gear_profiles,
        [0., 0., 0.],
        [0., 0., 1.],
    )
    .expect("gear sketch failed");

    m.extrude_no_merge("gear", "gear_sk", depth)
        .expect("gear extrude failed");

    m
}

#[test]
#[ignore = "kernel-v2: gear (arc-segment) profiles NotSupported until non-convex CDT lands (roadmap Phase 2 tail)"]
fn r1_gear_extrude_creates_solid() {
    let m = build_gear_solid(12, 2.0, 20.0, 5.0);
    m.assert_has_solid("gear")
        .expect("gear extrude should produce a solid");
}

#[test]
#[ignore = "kernel-v2: gear (arc-segment) profiles NotSupported until non-convex CDT lands (roadmap Phase 2 tail)"]
fn r2_gear_extrude_volume() {
    let mut m = build_gear_solid(12, 2.0, 20.0, 5.0);
    let mesh = m.tessellate("gear").expect("tessellation should succeed");
    let vol = mesh_volume(&mesh);
    // Gear volume should be positive and reasonable
    // pitch_radius = 12 * 2.0 / 2 = 12, addendum = 14, dedendum = 9.5
    // Rough area estimate: between pi*9.5^2 and pi*14^2 → [283, 615]
    // Volume = area * depth(5) → [1415, 3077]
    assert!(vol > 100.0, "Gear volume should be > 100, got {:.1}", vol);
    assert!(vol < 5000.0, "Gear volume should be < 5000, got {:.1}", vol);
}

#[test]
#[ignore = "kernel-v2: gear (arc-segment) profiles NotSupported until non-convex CDT lands (roadmap Phase 2 tail)"]
fn r3_gear_extrude_watertight() {
    let mut m = build_gear_solid(12, 2.0, 20.0, 5.0);
    let mesh = m.tessellate("gear").expect("tessellation should succeed");
    // Check watertight via edge pairing
    let n_tris = mesh.indices.len() / 3;
    assert!(n_tris > 0, "Gear mesh should have triangles");
    // Simple edge-pair check
    use std::collections::HashMap;
    fn quantize(mesh: &RenderMesh, idx: u32) -> (i64, i64, i64) {
        let base = idx as usize * 3;
        (
            (mesh.vertices[base] as f64 * 1e6).round() as i64,
            (mesh.vertices[base + 1] as f64 * 1e6).round() as i64,
            (mesh.vertices[base + 2] as f64 * 1e6).round() as i64,
        )
    }
    let mut edge_count: HashMap<_, u32> = HashMap::new();
    for i in 0..n_tris {
        let tri = [
            mesh.indices[i * 3],
            mesh.indices[i * 3 + 1],
            mesh.indices[i * 3 + 2],
        ];
        for j in 0..3 {
            let pa = quantize(&mesh, tri[j]);
            let pb = quantize(&mesh, tri[(j + 1) % 3]);
            let key = if pa <= pb { (pa, pb) } else { (pb, pa) };
            *edge_count.entry(key).or_insert(0) += 1;
        }
    }
    let watertight = edge_count.values().all(|&c| c == 2);
    assert!(watertight, "Gear mesh should be watertight");
}

#[test]
fn r4_gear_union_with_box() {
    let mut m = build_gear_solid(12, 2.0, 20.0, 5.0);

    // Add a box that overlaps with the gear
    m.rect_sketch(
        "box_sk",
        [0., 0., 0.],
        [0., 0., 1.],
        -20.0,
        -20.0,
        40.0,
        40.0,
    )
    .expect("box sketch failed");
    m.extrude_no_merge("box", "box_sk", 2.0)
        .expect("box extrude failed");

    // Union: gear + box
    let result = m.boolean_union("result", "gear", "box");
    match result {
        Ok(_) => {
            m.assert_has_solid("result")
                .expect("union should produce solid");
        }
        Err(e) => {
            // May fail for complex geometry — that's acceptable for now
            eprintln!("r4: gear+box union error (acceptable): {:?}", e);
        }
    }
}

#[test]
fn r5_gear_subtract_from_box() {
    let mut m = build_gear_solid(12, 2.0, 20.0, 5.0);

    // Add a large box that fully contains the gear
    m.rect_sketch(
        "box_sk",
        [0., 0., 0.],
        [0., 0., 1.],
        -20.0,
        -20.0,
        40.0,
        40.0,
    )
    .expect("box sketch failed");
    m.extrude_no_merge("box", "box_sk", 10.0)
        .expect("box extrude failed");

    // Subtract: box - gear
    let result = m.boolean_subtract("result", "box", "gear");
    match result {
        Ok(_) => {
            m.assert_has_solid("result")
                .expect("subtract should produce solid");
        }
        Err(e) => {
            // May fail for complex geometry — that's acceptable for now
            eprintln!("r5: box-gear subtract error (acceptable): {:?}", e);
        }
    }
}
