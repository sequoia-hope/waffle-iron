//! Regression: an extrude on an OFFSET (or non-XY) sketch plane must produce a
//! body at the correct WORLD position — lined up with the sketch.
//!
//! ## Why this test exists
//!
//! A user reported "draw a rectangle on a plane offset from origin, extrude, and
//! the body doesn't line up with the sketch." It could not be reproduced (the
//! geometry was correct in every path), but the investigation surfaced a real
//! COVERAGE GAP: no test asserted the *world position* of an offset-plane
//! extrude. The pre-existing checks were blind to exactly this failure mode:
//!
//!   - `pipeline_tests::non_xy_plane_sketch_extrude` runs on `MockKernel` (fake
//!     geometry) and only asserts `sketch.plane_origin` is STORED.
//!   - `pipeline_tests::sketch_extrude_mesh_bounding_box` only asserts the bbox
//!     is non-degenerate, never WHERE it is.
//!
//! These run on the real `kernel-v2` adapter and assert the body's bounding box
//! lands exactly where the sketch plane + profile place it: the near cap on the
//! plane, the far cap `depth` along the normal, and the in-plane extent matching
//! the rectangle. A body that dropped the plane origin (the reported symptom)
//! would fail the near-cap assertion.

use test_harness::helpers::mesh_bounding_box;
use test_harness::ModelBuilder;

/// Build `rect (x,y)+(w,h)` on plane `(origin, +Z-style normal)`, extrude `depth`
/// along the normal, and return the body's bounding box `(min, max)`.
fn extrude_bbox(
    origin: [f64; 3],
    normal: [f64; 3],
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    depth: f64,
) -> ([f32; 3], [f32; 3]) {
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("s", origin, normal, x, y, w, h).unwrap();
    b.extrude("e", "s", depth).unwrap();
    let mesh = b.tessellate("e").unwrap();
    mesh_bounding_box(&mesh)
}

const EPS: f32 = 1e-5;

#[test]
fn extrude_on_z_offset_plane_lands_at_offset() {
    // Rectangle on the +Z plane offset to z = 5; extrude 3 along +Z.
    let (mn, mx) = extrude_bbox([0.0, 0.0, 5.0], [0.0, 0.0, 1.0], 0.0, 0.0, 10.0, 10.0, 3.0);
    // Near cap sits ON the sketch plane (z = 5), far cap at z = 8 — NOT at the
    // origin (the reported "body at z=0 while sketch at z=5" symptom).
    assert!((mn[2] - 5.0).abs() < EPS, "near cap z {} != 5 (offset dropped?)", mn[2]);
    assert!((mx[2] - 8.0).abs() < EPS, "far cap z {} != 8", mx[2]);
    // In-plane extent is the 10×10 rectangle (basis for +Z: x∈[0,10], y∈[-10,0]).
    assert!((mx[0] - mn[0] - 10.0).abs() < EPS, "x extent {}", mx[0] - mn[0]);
    assert!((mx[1] - mn[1] - 10.0).abs() < EPS, "y extent {}", mx[1] - mn[1]);
}

#[test]
fn extrude_offset_matches_base_shifted_along_normal() {
    // The offset body must be the base-plane body translated by exactly the
    // offset along the normal — proves the origin is applied, nothing else moves.
    let (bn, bx) = extrude_bbox([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, 10.0, 10.0, 3.0);
    let (on, ox) = extrude_bbox([0.0, 0.0, 5.0], [0.0, 0.0, 1.0], 0.0, 0.0, 10.0, 10.0, 3.0);
    for k in 0..2 {
        assert!((on[k] - bn[k]).abs() < EPS, "in-plane min axis {k} moved");
        assert!((ox[k] - bx[k]).abs() < EPS, "in-plane max axis {k} moved");
    }
    assert!((on[2] - (bn[2] + 5.0)).abs() < EPS, "offset not applied along normal");
}

#[test]
fn extrude_in_plane_offset_rect_keeps_position() {
    // An off-origin rectangle (corner at u=50,v=50) must extrude there, not at
    // the sketch origin — guards the in-plane-shift symptom.
    let (mn, mx) = extrude_bbox([0.0, 0.0, 5.0], [0.0, 0.0, 1.0], 50.0, 50.0, 10.0, 10.0, 3.0);
    // For the +Z basis (x = +X-from-normal gives x∈[u,u+w] shifted): just assert
    // the in-plane box is 10×10 and NOT centred on the origin.
    assert!((mx[0] - mn[0] - 10.0).abs() < EPS);
    assert!((mx[1] - mn[1] - 10.0).abs() < EPS);
    let centred_on_origin = mn[0] <= 0.0 && mx[0] >= 0.0 && mn[1] <= 0.0 && mx[1] >= 0.0;
    assert!(!centred_on_origin, "off-origin rect collapsed to the sketch origin");
    assert!((mn[2] - 5.0).abs() < EPS, "near cap off the plane");
}

#[test]
fn extrude_on_top_and_right_builtin_planes_line_up() {
    // Non-Z normals exercise the basis (tangent_x_from_normal). The extrude must
    // grow ALONG the normal and keep the 10×10 face in-plane.
    let (mn, mx) = extrude_bbox([0.0, 5.0, 0.0], [0.0, 1.0, 0.0], 0.0, 0.0, 10.0, 10.0, 3.0);
    assert!((mn[1] - 5.0).abs() < EPS, "Top-offset near cap y {} != 5", mn[1]);
    assert!((mx[1] - 8.0).abs() < EPS, "Top-offset far cap y {} != 8", mx[1]);

    let (mn, mx) = extrude_bbox([7.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.0, 0.0, 10.0, 10.0, 3.0);
    assert!((mn[0] - 7.0).abs() < EPS, "Right-offset near cap x {} != 7", mn[0]);
    assert!((mx[0] - 10.0).abs() < EPS, "Right-offset far cap x {} != 10", mx[0]);
}
