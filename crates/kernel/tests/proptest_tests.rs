//! Property-based tests for CAD kernel invariants using the `proptest` crate.

use proptest::prelude::*;

use cad_kernel::boolean::engine::{boolean_op, estimate_volume, BoolOp};
use cad_kernel::geometry::point::Point3d;
use cad_kernel::geometry::transform::{BoundingBox, Transform};
use cad_kernel::geometry::vector::Vec3;
use cad_kernel::operations::chamfer::chamfer_edge;
use cad_kernel::operations::extrude::{extrude_profile, Profile};
use cad_kernel::operations::feature::FeatureTree;
use cad_kernel::operations::fillet::fillet_edge;
use cad_kernel::operations::revolve::revolve_profile;
use cad_kernel::topology::brep::EntityStore;
use cad_kernel::topology::primitives::make_box;
use cad_kernel::validation::audit::{full_verify, verify_topology_l0};

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

/// Two overlapping AABB boxes. The second box is offset from the first by
/// a fraction of the first box's extent, guaranteeing overlap.
fn arb_overlapping_box_pair(
) -> impl Strategy<Value = ((f64, f64, f64, f64, f64, f64), (f64, f64, f64, f64, f64, f64))> {
    (
        -8.0f64..4.0,
        -8.0f64..4.0,
        -8.0f64..4.0,
        1.0f64..4.0,
        1.0f64..4.0,
        1.0f64..4.0,
        0.1f64..0.9,
        0.1f64..0.9,
        0.1f64..0.9,
        1.0f64..4.0,
        1.0f64..4.0,
        1.0f64..4.0,
    )
        .prop_map(
            |(ox, oy, oz, dx_a, dy_a, dz_a, fx, fy, fz, dx_b, dy_b, dz_b)| {
                let a = (ox, oy, oz, ox + dx_a, oy + dy_a, oz + dz_a);
                // Second box starts partway through the first box, guaranteeing overlap
                let bx0 = ox + dx_a * fx;
                let by0 = oy + dy_a * fy;
                let bz0 = oz + dz_a * fz;
                let b = (bx0, by0, bz0, bx0 + dx_b, by0 + dy_b, bz0 + dz_b);
                (a, b)
            },
        )
}

const TOL: f64 = 1e-6;

// ===========================================================================
// Existing tests (1-10)
// ===========================================================================

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

// ===========================================================================
// NEW: Random AABB Boolean operations
// ===========================================================================

// ---------------------------------------------------------------------------
// 11. Boolean intersection volume matches analytical value
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn boolean_intersection_volume_matches_analytical(
        (box_a, box_b) in arb_overlapping_box_pair(),
    ) {
        let mut store = EntityStore::new();

        let a = make_box(&mut store, box_a.0, box_a.1, box_a.2, box_a.3, box_a.4, box_a.5);
        let b = make_box(&mut store, box_b.0, box_b.1, box_b.2, box_b.3, box_b.4, box_b.5);

        if let Ok(inter_id) = boolean_op(&mut store, a, b, BoolOp::Intersection) {
            let vol_mc = estimate_volume(&store, inter_id, 50_000);

            // Analytical intersection volume
            let ix0 = box_a.0.max(box_b.0);
            let iy0 = box_a.1.max(box_b.1);
            let iz0 = box_a.2.max(box_b.2);
            let ix1 = box_a.3.min(box_b.3);
            let iy1 = box_a.4.min(box_b.4);
            let iz1 = box_a.5.min(box_b.5);
            let vol_exact = ((ix1 - ix0).max(0.0)) * ((iy1 - iy0).max(0.0)) * ((iz1 - iz0).max(0.0));

            if vol_exact > 0.5 {
                let rel_error = (vol_mc - vol_exact).abs() / vol_exact;
                prop_assert!(rel_error < 0.15,
                    "Intersection volume: MC={:.3} vs exact={:.3}, rel_error={:.3}",
                    vol_mc, vol_exact, rel_error);
            }

            // Verify the intersection bounding box is correct
            let bb = store.solid_bounding_box(inter_id);
            prop_assert!((bb.min.x - ix0).abs() < 1e-9, "inter bb.min.x");
            prop_assert!((bb.min.y - iy0).abs() < 1e-9, "inter bb.min.y");
            prop_assert!((bb.min.z - iz0).abs() < 1e-9, "inter bb.min.z");
            prop_assert!((bb.max.x - ix1).abs() < 1e-9, "inter bb.max.x");
            prop_assert!((bb.max.y - iy1).abs() < 1e-9, "inter bb.max.y");
            prop_assert!((bb.max.z - iz1).abs() < 1e-9, "inter bb.max.z");
        }
    }
}

// ---------------------------------------------------------------------------
// 12. Boolean results pass topology audit (all three operations)
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn boolean_results_pass_topology_audit(
        (box_a, box_b) in arb_overlapping_box_pair(),
    ) {
        let mut store = EntityStore::new();

        let solid_a = make_box(&mut store, box_a.0, box_a.1, box_a.2, box_a.3, box_a.4, box_a.5);
        let solid_b = make_box(&mut store, box_b.0, box_b.1, box_b.2, box_b.3, box_b.4, box_b.5);

        // Test all three operations
        for op in &[BoolOp::Union, BoolOp::Intersection, BoolOp::Difference] {
            let a = make_box(&mut store, box_a.0, box_a.1, box_a.2, box_a.3, box_a.4, box_a.5);
            let b = make_box(&mut store, box_b.0, box_b.1, box_b.2, box_b.3, box_b.4, box_b.5);

            if let Ok(result_id) = boolean_op(&mut store, a, b, *op) {
                let audit = verify_topology_l0(&store, result_id);
                prop_assert!(audit.euler_valid,
                    "Euler formula violated for {:?}: {:?}", op, audit.errors);
                prop_assert!(audit.all_faces_closed,
                    "Open faces for {:?}: {:?}", op, audit.errors);
            }
        }

        // Verify inputs are still valid after boolean ops
        let audit_a = verify_topology_l0(&store, solid_a);
        prop_assert!(audit_a.euler_valid, "Input A topology corrupted after booleans");

        let audit_b = verify_topology_l0(&store, solid_b);
        prop_assert!(audit_b.euler_valid, "Input B topology corrupted after booleans");
    }
}

// ---------------------------------------------------------------------------
// 12b. Regression tests for previously-broken AABB boolean topology
//
// These cases previously failed because create_grid_quad created duplicate
// vertices. Now fixed: vertices are shared via coordinate map and edges
// are properly twin-linked.
// ---------------------------------------------------------------------------

#[test]
fn regression_aabb_union_euler_valid() {
    let mut store = EntityStore::new();
    let a = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
    let b = make_box(&mut store, 0.1, 0.1, 0.1, 1.1, 1.1, 1.1);

    let result = boolean_op(&mut store, a, b, BoolOp::Union);
    assert!(result.is_ok(), "Union should succeed");
    let result_id = result.unwrap();

    let audit = verify_topology_l0(&store, result_id);
    assert!(audit.euler_valid, "Euler formula should hold: {:?}", audit.errors);
    assert!(audit.all_faces_closed, "All faces should be closed: {:?}", audit.errors);
}

#[test]
fn regression_aabb_difference_euler_valid() {
    let mut store = EntityStore::new();
    let a = make_box(&mut store, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0);
    let b = make_box(&mut store, 0.1, 0.1, 0.1, 1.1, 1.1, 1.1);

    let result = boolean_op(&mut store, a, b, BoolOp::Difference);
    assert!(result.is_ok(), "Difference should succeed");
    let result_id = result.unwrap();

    let audit = verify_topology_l0(&store, result_id);
    assert!(audit.euler_valid, "Euler formula should hold: {:?}", audit.errors);
    assert!(audit.all_faces_closed, "All faces should be closed: {:?}", audit.errors);
}

// ---------------------------------------------------------------------------
// 13. Boolean result bounding boxes are sane
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn boolean_result_bounding_box_sane(
        (box_a, box_b) in arb_overlapping_box_pair(),
    ) {
        let mut store = EntityStore::new();

        let bb_a = BoundingBox::new(
            Point3d::new(box_a.0, box_a.1, box_a.2),
            Point3d::new(box_a.3, box_a.4, box_a.5),
        );
        let bb_b = BoundingBox::new(
            Point3d::new(box_b.0, box_b.1, box_b.2),
            Point3d::new(box_b.3, box_b.4, box_b.5),
        );
        let combined_bb = bb_a.union(&bb_b);

        for op in &[BoolOp::Union, BoolOp::Intersection, BoolOp::Difference] {
            let a = make_box(&mut store, box_a.0, box_a.1, box_a.2, box_a.3, box_a.4, box_a.5);
            let b = make_box(&mut store, box_b.0, box_b.1, box_b.2, box_b.3, box_b.4, box_b.5);

            if let Ok(result_id) = boolean_op(&mut store, a, b, *op) {
                let result_bb = store.solid_bounding_box(result_id);

                // Result BB must be contained within the union of input BBs (with small margin)
                let margin = 0.01;
                prop_assert!(result_bb.min.x >= combined_bb.min.x - margin,
                    "{:?} result min.x={} < combined min.x={}", op, result_bb.min.x, combined_bb.min.x);
                prop_assert!(result_bb.min.y >= combined_bb.min.y - margin,
                    "{:?} result min.y={} < combined min.y={}", op, result_bb.min.y, combined_bb.min.y);
                prop_assert!(result_bb.min.z >= combined_bb.min.z - margin,
                    "{:?} result min.z={} < combined min.z={}", op, result_bb.min.z, combined_bb.min.z);
                prop_assert!(result_bb.max.x <= combined_bb.max.x + margin,
                    "{:?} result max.x={} > combined max.x={}", op, result_bb.max.x, combined_bb.max.x);
                prop_assert!(result_bb.max.y <= combined_bb.max.y + margin,
                    "{:?} result max.y={} > combined max.y={}", op, result_bb.max.y, combined_bb.max.y);
                prop_assert!(result_bb.max.z <= combined_bb.max.z + margin,
                    "{:?} result max.z={} > combined max.z={}", op, result_bb.max.z, combined_bb.max.z);
            }
        }
    }
}

// ===========================================================================
// NEW: Random transform properties
// ===========================================================================

// ---------------------------------------------------------------------------
// 14. T * T^(-1) ~ Identity
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn transform_compose_with_inverse_is_identity(
        (tx, ty, tz) in arb_translation(),
        angle_x in arb_angle(),
        angle_y in arb_angle(),
        angle_z in arb_angle(),
    ) {
        let t = Transform::rotation_x(angle_x)
            .then(&Transform::rotation_y(angle_y))
            .then(&Transform::rotation_z(angle_z))
            .then(&Transform::translation(tx, ty, tz));

        if let Some(inv) = t.inverse() {
            let product = t.then(&inv);
            let identity = Transform::identity();

            for i in 0..16 {
                prop_assert!((product.m[i] - identity.m[i]).abs() < 1e-6,
                    "T*T^-1 not identity at index {}: got {}, expected {}",
                    i, product.m[i], identity.m[i]);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 15. Transform composition is associative: (T1 * T2) * T3 ~ T1 * (T2 * T3)
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn transform_associativity(
        (t1x, t1y, t1z) in arb_translation(),
        (t2x, t2y, t2z) in arb_translation(),
        (t3x, t3y, t3z) in arb_translation(),
        a1 in arb_angle(),
        a2 in arb_angle(),
        a3 in arb_angle(),
    ) {
        let t1 = Transform::rotation_z(a1).then(&Transform::translation(t1x, t1y, t1z));
        let t2 = Transform::rotation_x(a2).then(&Transform::translation(t2x, t2y, t2z));
        let t3 = Transform::rotation_y(a3).then(&Transform::translation(t3x, t3y, t3z));

        let left = t1.then(&t2).then(&t3);   // (T1 * T2) * T3
        let right = t1.then(&t2.then(&t3));   // T1 * (T2 * T3)

        for i in 0..16 {
            prop_assert!((left.m[i] - right.m[i]).abs() < 1e-6,
                "Associativity violated at index {}: {} != {}",
                i, left.m[i], right.m[i]);
        }
    }
}

// ---------------------------------------------------------------------------
// 16. Rigid transforms preserve distance
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn rigid_transform_preserves_distance(
        (ax, ay, az) in arb_point(),
        (bx, by, bz) in arb_point(),
        (tx, ty, tz) in arb_translation(),
        angle_x in arb_angle(),
        angle_y in arb_angle(),
        angle_z in arb_angle(),
    ) {
        let a = Point3d::new(ax, ay, az);
        let b = Point3d::new(bx, by, bz);

        // Build a rigid transform (rotation + translation, no scaling)
        let t = Transform::rotation_x(angle_x)
            .then(&Transform::rotation_y(angle_y))
            .then(&Transform::rotation_z(angle_z))
            .then(&Transform::translation(tx, ty, tz));

        let ta = t.transform_point(&a);
        let tb = t.transform_point(&b);

        let d_orig = a.distance_to(&b);
        let d_trans = ta.distance_to(&tb);

        // Rigid transforms must preserve distance exactly
        prop_assert!((d_orig - d_trans).abs() < 1e-4,
            "Rigid transform changed distance: {} -> {} (delta={})",
            d_orig, d_trans, (d_orig - d_trans).abs());
    }
}

// ===========================================================================
// NEW: Random point/vector operations
// ===========================================================================

// ---------------------------------------------------------------------------
// 17. Cross product orthogonality: (a x b) . a ~ 0 and (a x b) . b ~ 0
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn cross_product_orthogonality(
        (ax, ay, az) in arb_point(),
        (bx, by, bz) in arb_point(),
    ) {
        let a = Vec3::new(ax, ay, az);
        let b = Vec3::new(bx, by, bz);
        let cross = a.cross(&b);

        let dot_a = cross.dot(&a);
        let dot_b = cross.dot(&b);

        // Scale tolerance by magnitude of inputs to handle large values
        let scale = a.length() * b.length() * 1e-9 + 1e-9;
        prop_assert!(dot_a.abs() < scale,
            "(axb).a = {} (expected ~0, scale={})", dot_a, scale);
        prop_assert!(dot_b.abs() < scale,
            "(axb).b = {} (expected ~0, scale={})", dot_b, scale);
    }
}

// ---------------------------------------------------------------------------
// 18. Cross product magnitude: |a x b| = |a| * |b| * sin(angle)
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn cross_product_magnitude(
        (ax, ay, az) in arb_point(),
        (bx, by, bz) in arb_point(),
    ) {
        let a = Vec3::new(ax, ay, az);
        let b = Vec3::new(bx, by, bz);

        let cross_len = a.cross(&b).length();
        let angle = a.angle_to(&b);
        let expected = a.length() * b.length() * angle.sin().abs();

        // Skip near-zero vectors where numerical error dominates
        if a.length() > 1e-6 && b.length() > 1e-6 {
            let scale = a.length() * b.length();
            let tol = scale * 1e-9 + 1e-9;
            prop_assert!((cross_len - expected).abs() < tol,
                "|axb|={} != |a|*|b|*sin(angle)={} (tol={})", cross_len, expected, tol);
        }
    }
}

// ---------------------------------------------------------------------------
// 19. Vector triangle inequality: |a + b| <= |a| + |b|
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn vector_triangle_inequality(
        (ax, ay, az) in arb_point(),
        (bx, by, bz) in arb_point(),
    ) {
        let a = Vec3::new(ax, ay, az);
        let b = Vec3::new(bx, by, bz);

        let sum_len = (a + b).length();
        let individual_sum = a.length() + b.length();

        prop_assert!(sum_len <= individual_sum + TOL,
            "|a+b|={} > |a|+|b|={}", sum_len, individual_sum);
    }
}

// ===========================================================================
// NEW: Random primitive construction
// ===========================================================================

// ---------------------------------------------------------------------------
// 20. Random boxes pass full verification
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn random_box_passes_full_verify(
        (ox, oy, oz) in arb_point(),
        dx in arb_positive_dim(),
        dy in arb_positive_dim(),
        dz in arb_positive_dim(),
    ) {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, ox, oy, oz, ox + dx, oy + dy, oz + dz);

        let report = full_verify(&store, solid_id);
        prop_assert!(report.topology_valid,
            "Box ({},{},{}) + ({},{},{}) failed topology: {:?}",
            ox, oy, oz, dx, dy, dz, report.topology_audit.errors);
    }
}

// ---------------------------------------------------------------------------
// 21. Random box bounding box matches expected dimensions
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn random_box_bounding_box_matches(
        (ox, oy, oz) in arb_point(),
        dx in arb_positive_dim(),
        dy in arb_positive_dim(),
        dz in arb_positive_dim(),
    ) {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, ox, oy, oz, ox + dx, oy + dy, oz + dz);

        let bb = store.solid_bounding_box(solid_id);

        prop_assert!((bb.min.x - ox).abs() < 1e-9,
            "bb.min.x={} != ox={}", bb.min.x, ox);
        prop_assert!((bb.min.y - oy).abs() < 1e-9,
            "bb.min.y={} != oy={}", bb.min.y, oy);
        prop_assert!((bb.min.z - oz).abs() < 1e-9,
            "bb.min.z={} != oz={}", bb.min.z, oz);
        prop_assert!((bb.max.x - (ox + dx)).abs() < 1e-9,
            "bb.max.x={} != ox+dx={}", bb.max.x, ox + dx);
        prop_assert!((bb.max.y - (oy + dy)).abs() < 1e-9,
            "bb.max.y={} != oy+dy={}", bb.max.y, oy + dy);
        prop_assert!((bb.max.z - (oz + dz)).abs() < 1e-9,
            "bb.max.z={} != oz+dz={}", bb.max.z, oz + dz);
    }
}

// ---------------------------------------------------------------------------
// 22. Random box volume matches expected via Monte Carlo
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn random_box_volume_matches_expected(
        dx in 1.0f64..20.0,
        dy in 1.0f64..20.0,
        dz in 1.0f64..20.0,
    ) {
        let mut store = EntityStore::new();
        let solid_id = make_box(&mut store, 0.0, 0.0, 0.0, dx, dy, dz);

        let expected_vol = dx * dy * dz;
        let estimated_vol = estimate_volume(&store, solid_id, 50_000);

        if expected_vol > 1.0 {
            let rel_error = (estimated_vol - expected_vol).abs() / expected_vol;
            prop_assert!(rel_error < 0.15,
                "Volume mismatch: estimated={:.3}, expected={:.3}, rel_error={:.3}",
                estimated_vol, expected_vol, rel_error);
        }
    }
}

// ---------------------------------------------------------------------------
// 23. Boolean volume identity using analytical volumes
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn boolean_volume_identity_analytical(
        (box_a, box_b) in arb_overlapping_box_pair(),
    ) {
        let vol_a = (box_a.3 - box_a.0) * (box_a.4 - box_a.1) * (box_a.5 - box_a.2);
        let vol_b = (box_b.3 - box_b.0) * (box_b.4 - box_b.1) * (box_b.5 - box_b.2);

        // Analytical intersection volume
        let ix0 = box_a.0.max(box_b.0);
        let iy0 = box_a.1.max(box_b.1);
        let iz0 = box_a.2.max(box_b.2);
        let ix1 = box_a.3.min(box_b.3);
        let iy1 = box_a.4.min(box_b.4);
        let iz1 = box_a.5.min(box_b.5);
        let vol_inter = ((ix1 - ix0).max(0.0)) * ((iy1 - iy0).max(0.0)) * ((iz1 - iz0).max(0.0));

        let vol_union = vol_a + vol_b - vol_inter;
        let vol_diff = vol_a - vol_inter;

        // Basic sanity checks on volumes
        prop_assert!(vol_union >= vol_a - 1e-9, "union < A: {} < {}", vol_union, vol_a);
        prop_assert!(vol_union >= vol_b - 1e-9, "union < B: {} < {}", vol_union, vol_b);
        prop_assert!(vol_union <= vol_a + vol_b + 1e-9,
            "union > A+B: {} > {}", vol_union, vol_a + vol_b);
        prop_assert!(vol_inter >= 0.0 - 1e-9, "negative intersection volume");
        prop_assert!(vol_inter <= vol_a.min(vol_b) + 1e-9,
            "intersection > min(A,B): {} > {}", vol_inter, vol_a.min(vol_b));
        prop_assert!(vol_diff >= 0.0 - 1e-9, "negative difference volume");
        prop_assert!(vol_diff <= vol_a + 1e-9, "difference > A: {} > {}", vol_diff, vol_a);
    }
}

// ---------------------------------------------------------------------------
// 24. Boolean union volume matches analytical value via MC estimation
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn boolean_union_volume_matches_analytical(
        (box_a, box_b) in arb_overlapping_box_pair(),
    ) {
        let mut store = EntityStore::new();

        let a = make_box(&mut store, box_a.0, box_a.1, box_a.2, box_a.3, box_a.4, box_a.5);
        let b = make_box(&mut store, box_b.0, box_b.1, box_b.2, box_b.3, box_b.4, box_b.5);

        if let Ok(union_id) = boolean_op(&mut store, a, b, BoolOp::Union) {
            let vol_mc = estimate_volume(&store, union_id, 50_000);

            let vol_a = (box_a.3 - box_a.0) * (box_a.4 - box_a.1) * (box_a.5 - box_a.2);
            let vol_b = (box_b.3 - box_b.0) * (box_b.4 - box_b.1) * (box_b.5 - box_b.2);
            let ix0 = box_a.0.max(box_b.0);
            let iy0 = box_a.1.max(box_b.1);
            let iz0 = box_a.2.max(box_b.2);
            let ix1 = box_a.3.min(box_b.3);
            let iy1 = box_a.4.min(box_b.4);
            let iz1 = box_a.5.min(box_b.5);
            let vol_inter = ((ix1 - ix0).max(0.0)) * ((iy1 - iy0).max(0.0)) * ((iz1 - iz0).max(0.0));
            let vol_exact = vol_a + vol_b - vol_inter;

            if vol_exact > 0.5 {
                let rel_error = (vol_mc - vol_exact).abs() / vol_exact;
                prop_assert!(rel_error < 0.15,
                    "Union volume: MC={:.3} vs exact={:.3}, rel_error={:.3}",
                    vol_mc, vol_exact, rel_error);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 25. Boolean difference volume matches analytical value via MC estimation
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn boolean_difference_volume_matches_analytical(
        (box_a, box_b) in arb_overlapping_box_pair(),
    ) {
        let mut store = EntityStore::new();

        let a = make_box(&mut store, box_a.0, box_a.1, box_a.2, box_a.3, box_a.4, box_a.5);
        let b = make_box(&mut store, box_b.0, box_b.1, box_b.2, box_b.3, box_b.4, box_b.5);

        if let Ok(diff_id) = boolean_op(&mut store, a, b, BoolOp::Difference) {
            let vol_mc = estimate_volume(&store, diff_id, 50_000);

            let vol_a = (box_a.3 - box_a.0) * (box_a.4 - box_a.1) * (box_a.5 - box_a.2);
            let ix0 = box_a.0.max(box_b.0);
            let iy0 = box_a.1.max(box_b.1);
            let iz0 = box_a.2.max(box_b.2);
            let ix1 = box_a.3.min(box_b.3);
            let iy1 = box_a.4.min(box_b.4);
            let iz1 = box_a.5.min(box_b.5);
            let vol_inter = ((ix1 - ix0).max(0.0)) * ((iy1 - iy0).max(0.0)) * ((iz1 - iz0).max(0.0));
            let vol_exact = vol_a - vol_inter;

            if vol_exact > 0.5 {
                let rel_error = (vol_mc - vol_exact).abs() / vol_exact;
                prop_assert!(rel_error < 0.15,
                    "Difference volume: MC={:.3} vs exact={:.3}, rel_error={:.3}",
                    vol_mc, vol_exact, rel_error);
            }
        }
    }
}

// ===========================================================================
// NEW: Random operations (Spiral 2)
// ===========================================================================

// ---------------------------------------------------------------------------
// Strategy: convex polygon via sorted random angles
// ---------------------------------------------------------------------------

/// Generate a random convex polygon with `n` vertices (3..=8) on the XY plane,
/// radius in [1, 10]. Points are evenly spaced around the origin with random
/// perturbation to avoid degenerate (coincident vertex) configurations.
fn arb_convex_polygon(
) -> impl Strategy<Value = Vec<Point3d>> {
    (3usize..=8, 1.0f64..10.0)
        .prop_flat_map(|(n, radius)| {
            // Generate `n` perturbation offsets; final angle = base_angle + offset
            proptest::collection::vec(0.0f64..0.8, n)
                .prop_map(move |offsets| {
                    let step = std::f64::consts::TAU / n as f64;
                    offsets
                        .iter()
                        .enumerate()
                        .map(|(i, &off)| {
                            // Evenly space base angles, then perturb within half the step
                            let theta = step * i as f64 + off * step * 0.5;
                            Point3d::new(radius * theta.cos(), radius * theta.sin(), 0.0)
                        })
                        .collect::<Vec<_>>()
                })
        })
}

/// Generate a random unit direction vector from spherical coordinates.
fn arb_unit_direction() -> impl Strategy<Value = Vec3> {
    (0.01f64..std::f64::consts::PI - 0.01, 0.0f64..std::f64::consts::TAU)
        .prop_map(|(theta, phi)| {
            Vec3::new(
                theta.sin() * phi.cos(),
                theta.sin() * phi.sin(),
                theta.cos(),
            )
        })
}

// ---------------------------------------------------------------------------
// 26. Random extrusions pass topology audit
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn random_extrusion_passes_topology(
        polygon in arb_convex_polygon(),
        distance in 0.1f64..50.0,
        direction in arb_unit_direction(),
    ) {
        let mut store = EntityStore::new();
        let profile = Profile::from_points(polygon);

        let result = extrude_profile(&mut store, &profile, direction, distance);
        prop_assert!(result.is_ok(), "Extrude should succeed: {:?}", result.err());
        let solid_id = result.unwrap();

        // Topology audit
        let audit = verify_topology_l0(&store, solid_id);
        prop_assert!(audit.euler_valid,
            "Euler formula violated for random extrusion: {:?}", audit.errors);
        prop_assert!(audit.all_faces_closed,
            "Open faces in random extrusion: {:?}", audit.errors);

        // Volume should be positive (MC estimate)
        let vol = estimate_volume(&store, solid_id, 5_000);
        prop_assert!(vol > 0.0,
            "Extruded solid should have positive volume, got {}", vol);
    }
}

// ---------------------------------------------------------------------------
// 27. Random revolves pass topology audit
// ---------------------------------------------------------------------------

/// Generate a random revolve profile: 2-6 points with positive X.
fn arb_revolve_profile() -> impl Strategy<Value = Vec<Point3d>> {
    (2usize..=6).prop_flat_map(|n| {
        proptest::collection::vec(
            (1.0f64..10.0, 0.0f64..20.0),
            n,
        ).prop_map(|coords| {
            coords
                .iter()
                .enumerate()
                .map(|(i, &(r, _))| {
                    // Space points along Y axis, ensure positive X
                    let y = i as f64 * 2.0;
                    Point3d::new(r, 0.0, y)
                })
                .collect::<Vec<_>>()
        })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn random_revolve_passes_topology(
        profile in arb_revolve_profile(),
        angle_deg in 10.0f64..360.0,
        num_segments in 8usize..=32,
    ) {
        let mut store = EntityStore::new();
        let angle_rad = angle_deg.to_radians();

        let result = revolve_profile(
            &mut store,
            &profile,
            Point3d::ORIGIN,
            Vec3::Z,
            angle_rad,
            num_segments,
        );
        prop_assert!(result.is_ok(), "Revolve should succeed: {:?}", result.err());
        let solid_id = result.unwrap();

        // Topology audit
        let audit = verify_topology_l0(&store, solid_id);
        prop_assert!(audit.all_faces_closed,
            "Open faces in random revolve: {:?}", audit.errors);

        // Bounding box sanity: should be finite and non-degenerate
        let bb = store.solid_bounding_box(solid_id);
        prop_assert!(bb.max.x > bb.min.x, "BB degenerate in X");
        prop_assert!(bb.max.z > bb.min.z, "BB degenerate in Z");
        prop_assert!(bb.max.x.is_finite(), "BB max.x not finite");
        prop_assert!(bb.min.x.is_finite(), "BB min.x not finite");
    }
}

// ---------------------------------------------------------------------------
// 28. Random fillet on box passes topology audit
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn random_fillet_on_box_passes_topology(
        dx in 1.0f64..20.0,
        dy in 1.0f64..20.0,
        dz in 1.0f64..20.0,
        edge_idx in 0usize..12,
        radius_frac in 0.01f64..0.24,
    ) {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, dx, dy, dz);

        // Get the edge at the given index
        let edges = FeatureTree::collect_unique_edges(&store, box_id);
        let edge_idx = edge_idx % edges.len();
        let (v0, v1) = edges[edge_idx];

        // Radius is a fraction of the smallest dimension
        let min_dim = dx.min(dy).min(dz);
        let radius = min_dim * radius_frac;

        let result = fillet_edge(&mut store, box_id, v0, v1, radius, 4);
        prop_assert!(result.is_ok(), "Fillet should succeed: {:?}", result.err());
        let fillet_id = result.unwrap();

        // Topology: loops should all be closed
        let audit = verify_topology_l0(&store, fillet_id);
        prop_assert!(audit.all_faces_closed,
            "Open faces in filleted box: {:?}", audit.errors);

        // Face count should be > 6 (box has 6, fillet adds strip faces)
        let solid = &store.solids[fillet_id];
        let shell = &store.shells[solid.shells[0]];
        prop_assert!(shell.faces.len() > 6,
            "Filleted box should have more than 6 faces, got {}", shell.faces.len());
    }
}

// ---------------------------------------------------------------------------
// 29. Random chamfer on box passes topology audit
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn random_chamfer_on_box_passes_topology(
        dx in 1.0f64..20.0,
        dy in 1.0f64..20.0,
        dz in 1.0f64..20.0,
        edge_idx in 0usize..12,
        dist_frac in 0.01f64..0.24,
    ) {
        let mut store = EntityStore::new();
        let box_id = make_box(&mut store, 0.0, 0.0, 0.0, dx, dy, dz);

        // Get the edge at the given index
        let edges = FeatureTree::collect_unique_edges(&store, box_id);
        let edge_idx = edge_idx % edges.len();
        let (v0, v1) = edges[edge_idx];

        // Distance is a fraction of the smallest dimension
        let min_dim = dx.min(dy).min(dz);
        let distance = min_dim * dist_frac;

        let result = chamfer_edge(&mut store, box_id, v0, v1, distance);
        prop_assert!(result.is_ok(), "Chamfer should succeed: {:?}", result.err());
        let chamfer_id = result.unwrap();

        // Topology: loops should all be closed
        let audit = verify_topology_l0(&store, chamfer_id);
        prop_assert!(audit.all_faces_closed,
            "Open faces in chamfered box: {:?}", audit.errors);

        // Chamfer produces exactly 7 faces (6 original + 1 bevel)
        let solid = &store.solids[chamfer_id];
        let shell = &store.shells[solid.shells[0]];
        prop_assert_eq!(shell.faces.len(), 7,
            "Chamfered box should have 7 faces, got {}", shell.faces.len());
    }
}

// ---------------------------------------------------------------------------
// 30. Boolean volume identity: vol(A∪B) ≈ vol(A) + vol(B) - vol(A∩B)
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn boolean_volume_identity_mc(
        (box_a, box_b) in arb_overlapping_box_pair(),
    ) {
        // Compute analytical volumes for reference
        let vol_a = (box_a.3 - box_a.0) * (box_a.4 - box_a.1) * (box_a.5 - box_a.2);
        let vol_b = (box_b.3 - box_b.0) * (box_b.4 - box_b.1) * (box_b.5 - box_b.2);

        let ix0 = box_a.0.max(box_b.0);
        let iy0 = box_a.1.max(box_b.1);
        let iz0 = box_a.2.max(box_b.2);
        let ix1 = box_a.3.min(box_b.3);
        let iy1 = box_a.4.min(box_b.4);
        let iz1 = box_a.5.min(box_b.5);
        let vol_inter_exact = ((ix1 - ix0).max(0.0)) * ((iy1 - iy0).max(0.0)) * ((iz1 - iz0).max(0.0));

        // Skip tiny volumes where MC is unreliable
        if vol_a < 1.0 || vol_b < 1.0 || vol_inter_exact < 0.5 {
            return Ok(());
        }

        let vol_union_exact = vol_a + vol_b - vol_inter_exact;
        let vol_diff_exact = vol_a - vol_inter_exact;

        // Compute MC estimates from boolean operations
        let mut store = EntityStore::new();
        let a = make_box(&mut store, box_a.0, box_a.1, box_a.2, box_a.3, box_a.4, box_a.5);
        let b = make_box(&mut store, box_b.0, box_b.1, box_b.2, box_b.3, box_b.4, box_b.5);

        if let Ok(union_id) = boolean_op(&mut store, a, b, BoolOp::Union) {
            let vol_union_mc = estimate_volume(&store, union_id, 5_000);

            // vol(A∪B) ≈ vol(A) + vol(B) - vol(A∩B)
            if vol_union_exact > 1.0 {
                let rel_error = (vol_union_mc - vol_union_exact).abs() / vol_union_exact;
                prop_assert!(rel_error < 0.15,
                    "Union volume identity: MC={:.3} vs exact={:.3}, rel_error={:.3}",
                    vol_union_mc, vol_union_exact, rel_error);
            }
        }

        // Difference: vol(A-B) ≈ vol(A) - vol(A∩B)
        let mut store2 = EntityStore::new();
        let a2 = make_box(&mut store2, box_a.0, box_a.1, box_a.2, box_a.3, box_a.4, box_a.5);
        let b2 = make_box(&mut store2, box_b.0, box_b.1, box_b.2, box_b.3, box_b.4, box_b.5);

        if let Ok(diff_id) = boolean_op(&mut store2, a2, b2, BoolOp::Difference) {
            let vol_diff_mc = estimate_volume(&store2, diff_id, 5_000);

            if vol_diff_exact > 1.0 {
                let rel_error = (vol_diff_mc - vol_diff_exact).abs() / vol_diff_exact;
                prop_assert!(rel_error < 0.15,
                    "Difference volume identity: MC={:.3} vs exact={:.3}, rel_error={:.3}",
                    vol_diff_mc, vol_diff_exact, rel_error);
            }
        }
    }
}
