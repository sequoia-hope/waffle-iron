//! Property-based tests for CAD kernel invariants using the `proptest` crate.

use proptest::prelude::*;

use cad_kernel::geometry::point::Point3d;
use cad_kernel::geometry::transform::{BoundingBox, Transform};
use cad_kernel::geometry::vector::Vec3;
use cad_kernel::topology::brep::EntityStore;
use cad_kernel::topology::primitives::make_box;

// ---------------------------------------------------------------------------
// Strategy helpers
// ---------------------------------------------------------------------------

/// Arbitrary 3D coordinate tuple in a reasonable floating-point range.
fn arb_point() -> impl Strategy<Value = (f64, f64, f64)> {
    (-1000.0f64..1000.0, -1000.0f64..1000.0, -1000.0f64..1000.0)
}

/// Arbitrary translation offsets.
fn arb_translation() -> impl Strategy<Value = (f64, f64, f64)> {
    (-1000.0f64..1000.0, -1000.0f64..1000.0, -1000.0f64..1000.0)
}

/// Arbitrary positive dimension suitable for box extents (avoids degenerate zero-size).
fn arb_positive_dim() -> impl Strategy<Value = f64> {
    0.1f64..1000.0
}

/// Arbitrary rotation angle in radians.
fn arb_angle() -> impl Strategy<Value = f64> {
    -std::f64::consts::PI..std::f64::consts::PI
}

const TOL: f64 = 1e-6;

// ---------------------------------------------------------------------------
// 1. Point distance symmetry: distance(a, b) == distance(b, a)
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn point_distance_symmetry(
        (ax, ay, az) in arb_point(),
        (bx, by, bz) in arb_point(),
    ) {
        let a = Point3d::new(ax, ay, az);
        let b = Point3d::new(bx, by, bz);
        let d_ab = a.distance_to(&b);
        let d_ba = b.distance_to(&a);
        prop_assert!((d_ab - d_ba).abs() < TOL,
            "distance(a,b)={} != distance(b,a)={}", d_ab, d_ba);
    }
}

// ---------------------------------------------------------------------------
// 2. Point distance triangle inequality: d(a,c) <= d(a,b) + d(b,c)
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn point_distance_triangle_inequality(
        (ax, ay, az) in arb_point(),
        (bx, by, bz) in arb_point(),
        (cx, cy, cz) in arb_point(),
    ) {
        let a = Point3d::new(ax, ay, az);
        let b = Point3d::new(bx, by, bz);
        let c = Point3d::new(cx, cy, cz);
        let d_ac = a.distance_to(&c);
        let d_ab = a.distance_to(&b);
        let d_bc = b.distance_to(&c);
        prop_assert!(d_ac <= d_ab + d_bc + TOL,
            "triangle inequality violated: d(a,c)={} > d(a,b)+d(b,c)={}", d_ac, d_ab + d_bc);
    }
}

// ---------------------------------------------------------------------------
// 3. Vector dot product commutativity: a.dot(b) == b.dot(a)
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn vector_dot_commutativity(
        (ax, ay, az) in arb_point(),
        (bx, by, bz) in arb_point(),
    ) {
        let a = Vec3::new(ax, ay, az);
        let b = Vec3::new(bx, by, bz);
        let ab = a.dot(&b);
        let ba = b.dot(&a);
        prop_assert!((ab - ba).abs() < TOL,
            "a.dot(b)={} != b.dot(a)={}", ab, ba);
    }
}

// ---------------------------------------------------------------------------
// 4. Vector cross product anticommutativity: a.cross(b) == -b.cross(a)
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn vector_cross_anticommutativity(
        (ax, ay, az) in arb_point(),
        (bx, by, bz) in arb_point(),
    ) {
        let a = Vec3::new(ax, ay, az);
        let b = Vec3::new(bx, by, bz);
        let ab = a.cross(&b);
        let ba = b.cross(&a);
        let neg_ba = -ba;
        prop_assert!((ab.x - neg_ba.x).abs() < TOL, "x component: {} != {}", ab.x, neg_ba.x);
        prop_assert!((ab.y - neg_ba.y).abs() < TOL, "y component: {} != {}", ab.y, neg_ba.y);
        prop_assert!((ab.z - neg_ba.z).abs() < TOL, "z component: {} != {}", ab.z, neg_ba.z);
    }
}

// ---------------------------------------------------------------------------
// 5. Transform inverse roundtrip: T^{-1}(T(p)) == p
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn transform_inverse_roundtrip(
        (px, py, pz) in arb_point(),
        (tx, ty, tz) in arb_translation(),
        angle in arb_angle(),
    ) {
        let p = Point3d::new(px, py, pz);

        // Compose a non-singular transform: rotation then translation
        let t = Transform::rotation_z(angle)
            .then(&Transform::translation(tx, ty, tz));

        if let Some(inv) = t.inverse() {
            let transformed = t.transform_point(&p);
            let roundtrip = inv.transform_point(&transformed);
            prop_assert!((roundtrip.x - p.x).abs() < TOL,
                "x roundtrip: {} != {}", roundtrip.x, p.x);
            prop_assert!((roundtrip.y - p.y).abs() < TOL,
                "y roundtrip: {} != {}", roundtrip.y, p.y);
            prop_assert!((roundtrip.z - p.z).abs() < TOL,
                "z roundtrip: {} != {}", roundtrip.z, p.z);
        }
        // If inverse() returns None the transform is singular; skip that case.
    }
}

// ---------------------------------------------------------------------------
// 6. Translation preserves distance: d(T*a, T*b) == d(a, b)
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn translation_preserves_distance(
        (ax, ay, az) in arb_point(),
        (bx, by, bz) in arb_point(),
        (tx, ty, tz) in arb_translation(),
    ) {
        let a = Point3d::new(ax, ay, az);
        let b = Point3d::new(bx, by, bz);
        let t = Transform::translation(tx, ty, tz);

        let ta = t.transform_point(&a);
        let tb = t.transform_point(&b);

        let d_orig = a.distance_to(&b);
        let d_trans = ta.distance_to(&tb);
        prop_assert!((d_orig - d_trans).abs() < TOL,
            "translation changed distance: {} -> {}", d_orig, d_trans);
    }
}

// ---------------------------------------------------------------------------
// 7. Box Euler formula: for any valid box dimensions, V - E + F = 2
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn box_euler_formula(
        (ox, oy, oz) in arb_point(),
        dx in arb_positive_dim(),
        dy in arb_positive_dim(),
        dz in arb_positive_dim(),
    ) {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, ox, oy, oz, ox + dx, oy + dy, oz + dz);

        let solid = &store.solids[solid_id];
        let shell_id = solid.shells[0];
        let (v, e, f) = store.count_topology(shell_id);

        prop_assert_eq!(v, 8, "expected 8 vertices, got {}", v);
        prop_assert_eq!(e, 12, "expected 12 edges, got {}", e);
        prop_assert_eq!(f, 6, "expected 6 faces, got {}", f);

        let euler = v as i64 - e as i64 + f as i64;
        prop_assert_eq!(euler, 2, "Euler V-E+F={} != 2", euler);
    }
}

// ---------------------------------------------------------------------------
// 8. BoundingBox contains its own corner vertices
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn bounding_box_contains_own_vertices(
        (ox, oy, oz) in arb_point(),
        dx in arb_positive_dim(),
        dy in arb_positive_dim(),
        dz in arb_positive_dim(),
    ) {
        let min = Point3d::new(ox, oy, oz);
        let max = Point3d::new(ox + dx, oy + dy, oz + dz);
        let bb = BoundingBox::new(min, max);

        // All 8 corners must be contained
        let corners = [
            Point3d::new(min.x, min.y, min.z),
            Point3d::new(max.x, min.y, min.z),
            Point3d::new(min.x, max.y, min.z),
            Point3d::new(max.x, max.y, min.z),
            Point3d::new(min.x, min.y, max.z),
            Point3d::new(max.x, min.y, max.z),
            Point3d::new(min.x, max.y, max.z),
            Point3d::new(max.x, max.y, max.z),
        ];

        for (i, corner) in corners.iter().enumerate() {
            prop_assert!(bb.contains_point(corner),
                "corner {} ({:?}) not contained in bounding box {:?}", i, corner, bb);
        }
    }
}

// ---------------------------------------------------------------------------
// 9. Midpoint equidistant: d(a, mid) == d(b, mid)
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn midpoint_equidistant(
        (ax, ay, az) in arb_point(),
        (bx, by, bz) in arb_point(),
    ) {
        let a = Point3d::new(ax, ay, az);
        let b = Point3d::new(bx, by, bz);
        let mid = a.midpoint(&b);

        let d_a_mid = a.distance_to(&mid);
        let d_b_mid = b.distance_to(&mid);
        prop_assert!((d_a_mid - d_b_mid).abs() < TOL,
            "midpoint not equidistant: d(a,mid)={} d(b,mid)={}", d_a_mid, d_b_mid);
    }
}

// ---------------------------------------------------------------------------
// 10. Lerp boundaries: lerp(a, b, 0) == a and lerp(a, b, 1) == b
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn lerp_boundaries(
        (ax, ay, az) in arb_point(),
        (bx, by, bz) in arb_point(),
    ) {
        let a = Point3d::new(ax, ay, az);
        let b = Point3d::new(bx, by, bz);

        let at_zero = a.lerp(&b, 0.0);
        prop_assert!((at_zero.x - a.x).abs() < TOL, "lerp(0) x: {} != {}", at_zero.x, a.x);
        prop_assert!((at_zero.y - a.y).abs() < TOL, "lerp(0) y: {} != {}", at_zero.y, a.y);
        prop_assert!((at_zero.z - a.z).abs() < TOL, "lerp(0) z: {} != {}", at_zero.z, a.z);

        let at_one = a.lerp(&b, 1.0);
        prop_assert!((at_one.x - b.x).abs() < TOL, "lerp(1) x: {} != {}", at_one.x, b.x);
        prop_assert!((at_one.y - b.y).abs() < TOL, "lerp(1) y: {} != {}", at_one.y, b.y);
        prop_assert!((at_one.z - b.z).abs() < TOL, "lerp(1) z: {} != {}", at_one.z, b.z);
    }
}
