//! Regression: a merge extrude that auto-unions with a spatially-disjoint
//! existing body must yield TWO bodies (the union result is one solid with two
//! disjoint shells; kernel-v2 splits it via `split_solid_into_bodies`), not one.
//!
//! This is the F0015-class bug: two `merge=true` extrudes on far-apart planes
//! showed a single body in the list and body count.

use test_harness::ModelBuilder;

#[test]
fn disjoint_merge_extrudes_yield_two_bodies() {
    let mut b = ModelBuilder::kernel_v2();

    // Two far-apart unit squares on the XY plane, each extruded +Z (merge=true).
    b.rect_sketch("S1", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, 1.0, 1.0)
        .unwrap();
    b.extrude("E1", "S1", 1.0).unwrap();

    b.rect_sketch("S2", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 5.0, 5.0, 1.0, 1.0)
        .unwrap();
    // E2 (merge) auto-unions with the disjoint E1.
    let e2 = b.extrude("E2", "S2", 1.0).unwrap();

    // E1 is consumed into E2's result.
    assert!(
        b.distinct_solid_count() >= 1,
        "the union result feature exists"
    );

    // The consuming feature's result carries two disjoint bodies.
    let meshes = b.tessellate_all("E2").unwrap();
    assert_eq!(
        meshes.len(),
        2,
        "disjoint union must split into two bodies, got {}",
        meshes.len()
    );
    // Sanity: each body tessellated to real geometry.
    for m in &meshes {
        assert!(!m.vertices.is_empty() && !m.indices.is_empty());
    }
    let _ = e2;
}

#[test]
fn overlapping_merge_extrudes_yield_one_body() {
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("S1", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, 2.0, 2.0)
        .unwrap();
    b.extrude("E1", "S1", 2.0).unwrap();
    b.rect_sketch("S2", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 1.0, 2.0, 2.0)
        .unwrap();
    b.extrude("E2", "S2", 2.0).unwrap();

    let meshes = b.tessellate_all("E2").unwrap();
    assert_eq!(meshes.len(), 1, "overlapping union is a single body");
}
