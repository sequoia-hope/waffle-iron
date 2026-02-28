//! Revolve+boolean failure regression tests.
//!
//! These tests capture realistic CAD workflows combining revolve operations
//! with boolean operations (union, subtract, intersect). All are currently
//! `#[ignore]` as known failures — revolve produces toroidal surfaces whose
//! intersection curves (torus-plane, torus-cylinder, torus-torus) are not yet
//! supported by the analytical SSI module or the mesh-based IC pipeline.
//!
//! Categories:
//!   RB — Revolve+Boolean workflows (8 tests)

use test_harness::helpers::mesh_bounding_box;
use test_harness::ModelBuilder;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn count_visible_bodies(m: &ModelBuilder) -> usize {
    let consumed = m.consumed_features();
    let mut body_count = 0;
    for feature in &m.state.engine.tree.features {
        if feature.suppressed {
            continue;
        }
        if consumed.contains(&feature.id) {
            continue;
        }
        if let Some(result) = m.state.engine.get_result(feature.id) {
            body_count += result.outputs.len();
        }
    }
    body_count
}

fn assert_mesh_finite(mesh: &kernel_fork::types::RenderMesh, label: &str) {
    for (i, v) in mesh.vertices.iter().enumerate() {
        assert!(
            v.is_finite(),
            "{}: vertex[{}] is not finite: {}",
            label,
            i,
            v
        );
    }
    for (i, n) in mesh.normals.iter().enumerate() {
        assert!(
            n.is_finite(),
            "{}: normal[{}] is not finite: {}",
            label,
            i,
            n
        );
    }
}

// ---------------------------------------------------------------------------
// RB1: Full revolve (360°) union with box
// ---------------------------------------------------------------------------

#[test]
#[ignore = "Torus-plane IC generation works (Phase F), but shell assembly fails with 8+ open edges from torus face fragments"]
fn rb1_revolve_union_with_box() {
    let mut m = ModelBuilder::truck();

    // Rectangular sketch on XZ plane, offset from Y axis to create torus-like solid.
    // Note: with normal=[0,0,1], sketch (u,v) maps to world (v, -u, 0),
    // so y>0 is needed to keep the rect off the Y revolve axis (world x=0).
    m.rect_sketch("sk_ring", [0., 0., 0.], [0., 0., 1.], 2., 2., 1., 5.)
        .unwrap();
    // Revolve 360° around Y axis → torus-like solid
    m.revolve("torus", "sk_ring", [0., 0., 0.], [0., 1., 0.], 360.0)
        .unwrap();
    m.assert_has_solid("torus").unwrap();

    // Create a 10x10x10 box at origin
    m.rect_sketch("sk_box", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box", "sk_box", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // Union
    m.boolean_union("merged", "torus", "box").unwrap();
    m.assert_has_solid("merged").unwrap();

    assert_eq!(count_visible_bodies(&m), 1, "union should produce 1 body");

    let mesh = m.tessellate("merged").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "merged mesh should have triangles"
    );
    assert_mesh_finite(&mesh, "rb1_merged");
}

// ---------------------------------------------------------------------------
// RB2: Full revolve subtract from box → toroidal groove
// ---------------------------------------------------------------------------

#[test]
#[ignore = "Torus-plane IC generation works (Phase F), but shell assembly fails with open edges from torus face fragments"]
fn rb2_revolve_subtract_from_box() {
    let mut m = ModelBuilder::truck();

    // Create a 10x10x10 box at origin
    m.rect_sketch("sk_box", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box", "sk_box", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // Create a circle sketch on XZ plane (center at (5, 0), radius 2)
    m.circle_sketch("sk_ring", [0., 0., 0.], [0., 0., 1.], 5., 0., 2.)
        .unwrap();
    // Revolve 360° around Y axis → torus
    m.revolve("torus", "sk_ring", [0., 0., 0.], [0., 1., 0.], 360.0)
        .unwrap();
    m.assert_has_solid("torus").unwrap();

    // Subtract torus from box → toroidal groove
    m.boolean_subtract("grooved", "box", "torus").unwrap();
    m.assert_has_solid("grooved").unwrap();

    assert_eq!(
        count_visible_bodies(&m),
        1,
        "subtract should produce 1 body"
    );

    let mesh = m.tessellate("grooved").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "grooved mesh should have triangles"
    );
    assert_mesh_finite(&mesh, "rb2_grooved");

    // Volume of grooved box should be less than original box (10*10*10 = 1000)
    let (bb_min, bb_max) = mesh_bounding_box(&mesh);
    // Should still be roughly box-sized
    assert!(
        bb_max[0] - bb_min[0] > 5.0,
        "grooved body should span at least 5 units in X"
    );
}

// ---------------------------------------------------------------------------
// RB3: Partial revolve 90° union with box
// ---------------------------------------------------------------------------

#[test]
fn rb3_partial_revolve_90_union() {
    let mut m = ModelBuilder::truck();

    // Rect sketch on XZ plane, offset from Y axis.
    // Note: with normal=[0,0,1], sketch (u,v) maps to world (v, -u, 0),
    // so y>0 is needed to keep the rect off the Y revolve axis (world x=0).
    m.rect_sketch("sk_wedge", [0., 0., 0.], [0., 0., 1.], 3., 2., 2., 4.)
        .unwrap();
    // Revolve 90° around Y axis → quarter-turn solid with start/end caps
    m.revolve("wedge", "sk_wedge", [0., 0., 0.], [0., 1., 0.], 90.0)
        .unwrap();
    m.assert_has_solid("wedge").unwrap();

    // Create a 10x10x10 box at origin
    m.rect_sketch("sk_box", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box", "sk_box", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // Union
    m.boolean_union("merged", "wedge", "box").unwrap();
    m.assert_has_solid("merged").unwrap();

    assert_eq!(count_visible_bodies(&m), 1, "union should produce 1 body");
}

// ---------------------------------------------------------------------------
// RB4: Partial revolve 180° subtract from box
// ---------------------------------------------------------------------------

#[test]
fn rb4_partial_revolve_180_subtract() {
    let mut m = ModelBuilder::truck();

    // Create a 15x15x15 box at origin
    m.rect_sketch("sk_box", [0., 0., 0.], [0., 0., 1.], 0., 0., 15., 15.)
        .unwrap();
    m.extrude_no_merge("box", "sk_box", 15.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // Rect sketch on XZ plane, offset from Y axis.
    // Note: with normal=[0,0,1], sketch (u,v) maps to world (v, -u, 0),
    // so y>0 is needed to keep the rect off the Y revolve axis (world x=0).
    m.rect_sketch("sk_half", [0., 0., 0.], [0., 0., 1.], 2., 2., 3., 10.)
        .unwrap();
    // Revolve 180° around Y axis → half-torus solid
    m.revolve("half_torus", "sk_half", [0., 0., 0.], [0., 1., 0.], 180.0)
        .unwrap();
    m.assert_has_solid("half_torus").unwrap();

    // Subtract half-torus from box
    m.boolean_subtract("carved", "box", "half_torus").unwrap();
    m.assert_has_solid("carved").unwrap();

    assert_eq!(
        count_visible_bodies(&m),
        1,
        "subtract should produce 1 body"
    );
}

// ---------------------------------------------------------------------------
// RB5: Full revolve then extrude cut (cylinder through torus)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "Cylinder-torus IC — needs analytical torus support"]
fn rb5_revolve_then_extrude_cut() {
    let mut m = ModelBuilder::truck();

    // Create a circle sketch on XZ plane (center at (5, 0), radius 1) for torus
    m.circle_sketch("sk_ring", [0., 0., 0.], [0., 0., 1.], 5., 0., 1.)
        .unwrap();
    // Revolve 360° around Y axis → donut/torus
    m.revolve("donut", "sk_ring", [0., 0., 0.], [0., 1., 0.], 360.0)
        .unwrap();
    m.assert_has_solid("donut").unwrap();

    // Create a small circle sketch on XY plane for cylinder that pierces the torus
    m.circle_sketch("sk_cyl", [0., 0., 0.], [0., 0., 1.], 5., 0., 0.5)
        .unwrap();
    // Extrude the small circle through the torus (no merge to keep separate)
    m.extrude_no_merge("cyl", "sk_cyl", 10.0).unwrap();
    m.assert_has_solid("cyl").unwrap();

    // Subtract cylinder from torus → donut with hole
    m.boolean_subtract("drilled", "donut", "cyl").unwrap();
    m.assert_has_solid("drilled").unwrap();

    assert_eq!(
        count_visible_bodies(&m),
        1,
        "subtract should produce 1 body"
    );
}

// ---------------------------------------------------------------------------
// RB6: Extrude box then revolve union (order sensitivity test)
// ---------------------------------------------------------------------------

#[test]
#[ignore = "Torus-plane IC generation works (Phase F), but shell assembly fails with open edges from torus face fragments"]
fn rb6_extrude_then_revolve_union() {
    let mut m = ModelBuilder::truck();

    // Create 10x10x10 box at origin first
    m.rect_sketch("sk_box", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box", "sk_box", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // Then create revolve: rect sketch on XZ plane, offset from Y axis
    m.rect_sketch("sk_ring", [0., 0., 0.], [0., 0., 1.], 2., 5., 1., 3.)
        .unwrap();
    // Revolve 360° around Y axis
    m.revolve("ring", "sk_ring", [0., 0., 0.], [0., 1., 0.], 360.0)
        .unwrap();
    m.assert_has_solid("ring").unwrap();

    // Union box + revolve (box is body_a, revolve is body_b)
    m.boolean_union("merged", "box", "ring").unwrap();
    m.assert_has_solid("merged").unwrap();

    assert_eq!(count_visible_bodies(&m), 1, "union should produce 1 body");

    let mesh = m.tessellate("merged").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "merged mesh should have triangles"
    );
    assert_mesh_finite(&mesh, "rb6_merged");
}

// ---------------------------------------------------------------------------
// RB7: Two revolves union (torus-torus IC)
// ---------------------------------------------------------------------------

#[test]
fn rb7_two_revolves_union() {
    let mut m = ModelBuilder::truck();

    // First torus: circle at (3, 0), radius 1, revolved around Y
    m.circle_sketch("sk_ring1", [0., 0., 0.], [0., 0., 1.], 3., 0., 1.)
        .unwrap();
    m.revolve("torus1", "sk_ring1", [0., 0., 0.], [0., 1., 0.], 360.0)
        .unwrap();
    m.assert_has_solid("torus1").unwrap();

    // Second torus: circle at (5, 0), radius 1, revolved around Y
    m.circle_sketch("sk_ring2", [0., 0., 0.], [0., 0., 1.], 5., 0., 1.)
        .unwrap();
    m.revolve("torus2", "sk_ring2", [0., 0., 0.], [0., 1., 0.], 360.0)
        .unwrap();
    m.assert_has_solid("torus2").unwrap();

    // Union the two toruses
    m.boolean_union("merged", "torus1", "torus2").unwrap();
    m.assert_has_solid("merged").unwrap();

    assert_eq!(count_visible_bodies(&m), 1, "union should produce 1 body");

    let mesh = m.tessellate("merged").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "merged mesh should have triangles"
    );
    assert_mesh_finite(&mesh, "rb7_merged");
}

// ---------------------------------------------------------------------------
// RB8: Full revolve intersect with box
// ---------------------------------------------------------------------------

#[test]
#[ignore = "Torus-plane IC generation works (Phase F), but shell assembly fails with open edges from torus face fragments"]
fn rb8_revolve_intersect_with_box() {
    let mut m = ModelBuilder::truck();

    // Create a circle sketch on XZ plane (center at (5, 0), radius 2) for torus
    m.circle_sketch("sk_ring", [0., 0., 0.], [0., 0., 1.], 5., 0., 2.)
        .unwrap();
    // Revolve 360° around Y axis → torus
    m.revolve("torus", "sk_ring", [0., 0., 0.], [0., 1., 0.], 360.0)
        .unwrap();
    m.assert_has_solid("torus").unwrap();

    // Create a 10x10x10 box
    m.rect_sketch("sk_box", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box", "sk_box", 10.0).unwrap();
    m.assert_has_solid("box").unwrap();

    // Intersect torus with box → only the overlapping portion
    m.boolean_intersect("clipped", "torus", "box").unwrap();
    m.assert_has_solid("clipped").unwrap();

    assert_eq!(
        count_visible_bodies(&m),
        1,
        "intersect should produce 1 body"
    );

    let mesh = m.tessellate("clipped").unwrap();
    assert!(
        !mesh.indices.is_empty(),
        "clipped mesh should have triangles"
    );
    assert_mesh_finite(&mesh, "rb8_clipped");

    // Intersection should be smaller than the full box
    let (bb_min, bb_max) = mesh_bounding_box(&mesh);
    let bbox_volume = (bb_max[0] - bb_min[0]) * (bb_max[1] - bb_min[1]) * (bb_max[2] - bb_min[2]);
    assert!(
        bbox_volume < 1000.0,
        "intersected bounding box volume ({:.1}) should be less than box volume (1000)",
        bbox_volume
    );
}
