//! Regression tests for AABB enclosure false positive in boolean union.
//!
//! When a non-convex polygon (L-shape, gear, etc.) has an AABB that encloses a
//! cylinder, but the cylinder is in the polygon's concave region (not actually
//! inside the volume), `box_cyl_boolean()` must NOT silently drop the cylinder.
//!
//! Root cause: `cyl_enclosed_in_box()` checks cylinder bounding circle against
//! the polygon's AABB, which is necessary but not sufficient for non-convex shapes.
//! Fix: refine with `point_in_solid()` when face count > 6 (non-rectangular extrude).

use test_harness::helpers::mesh_volume;
use test_harness::workflow::ModelBuilder;

/// CG1: L-shaped polygon boss + small circle boss at the concave notch.
///
/// The L-shape's AABB encloses the cylinder, but the cylinder is in the
/// concave region (NOT inside the L-shape volume). Union should produce
/// both shapes. If the AABB enclosure bug is present, the cylinder is
/// silently dropped and the result is just the L-shape.
#[test]
fn cg1_concave_polygon_circle_union_preserves_cylinder() {
    let mut m = ModelBuilder::kernel_v2();

    // Step 1: Small circle in the concave notch of the L-shape, extrude up 10.
    // UV center (-5, 12) is in the notch region x∈[-20,5), y∈(5,20].
    m.true_circle_sketch("cyl_sk", [0., 0., 0.], [0., 0., 1.], -5., 12., 3.0)
        .unwrap();
    m.extrude_no_merge("cyl_boss", "cyl_sk", 10.0).unwrap();

    // Verify cylinder exists and has volume
    let cyl_mesh = m.tessellate("cyl_boss").expect("cylinder tessellates");
    let cyl_vol = mesh_volume(&cyl_mesh);
    assert!(cyl_vol > 0.0, "cylinder must have positive volume");

    // Step 2: L-shaped polygon (concave) whose AABB encloses the cylinder
    // but whose volume does NOT contain the cylinder center.
    // L-shape covers: bottom y∈[-20,5] × x∈[-20,20], right column y∈[5,20] × x∈[5,20]
    // Concave notch (no material): x∈[-20,5), y∈(5,20]
    m.polygon_sketch(
        "l_sk",
        [0., 0., 0.],
        [0., 0., 1.],
        &[
            (-20.0, -20.0),
            (20.0, -20.0),
            (20.0, 20.0),
            (5.0, 20.0),
            (5.0, 5.0),
            (-20.0, 5.0),
        ],
    )
    .unwrap();
    m.extrude("l_boss", "l_sk", 10.0).unwrap(); // merge=true → attempts union

    // Step 3: The cylinder MUST NOT be silently consumed.
    // If the AABB enclosure bug fires:
    //   - box_cyl_boolean returns clone_solid_as_result(box_solid) = just L-shape
    //   - cylinder feature is consumed → tessellation fails
    // If the fix works:
    //   - point_in_solid returns false → xy_enclosed = false
    //   - Falls to NotSupported("partial box-cylinder union")
    //   - Auto-union fallback → cylinder NOT consumed → both render
    let cyl_after = m.tessellate("cyl_boss");
    assert!(
        cyl_after.is_ok(),
        "BUG: Cylinder was silently consumed by AABB enclosure false positive. \
         The cylinder at (-5, 12) is in the concave notch of the L-shape, \
         NOT inside its volume, but the AABB check says it's enclosed."
    );
}

/// CG2: Convex polygon + enclosed cylinder — union should still work.
///
/// Ensures the point-in-solid refinement doesn't break legitimate enclosure.
/// A rectangle fully encloses a small cylinder at its center.
#[test]
fn cg2_convex_polygon_enclosed_cylinder_union_works() {
    let mut m = ModelBuilder::kernel_v2();

    // Small cylinder at origin
    m.true_circle_sketch("cyl_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 2.0)
        .unwrap();
    m.extrude_no_merge("cyl_boss", "cyl_sk", 10.0).unwrap();

    // Large rectangle fully enclosing the cylinder
    m.rect_sketch("box_sk", [0., 0., 0.], [0., 0., 1.], -10., -10., 20., 20.)
        .unwrap();
    m.extrude("box_boss", "box_sk", 10.0).unwrap(); // merge=true → union

    // For a convex rectangle, the cylinder IS inside → union should succeed.
    // The result is just the rectangle (cylinder fully enclosed).
    // No errors expected.
    let errors = m.assert_no_errors();
    assert!(
        errors.is_ok(),
        "CG2: Convex enclosure union should succeed: {:?}",
        errors.err()
    );
}
