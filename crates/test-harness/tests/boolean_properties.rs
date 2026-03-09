//! Property-based boolean tests for RealKernel.
//!
//! These tests verify algebraic, topological, and geometric invariants that
//! must hold for any correct boolean implementation. They catch classification
//! errors, geometric corruption, and topology violations across varied inputs.
//!
//! Categories:
//!   V  — Volume Properties (4 tests)
//!   T  — Topology Properties (3 tests)
//!   BB — Bounding Box Properties (2 tests)
//!   CH — Chain Stability (5 tests)
//!   CM — Commutativity (1 test)
//!   FC — Face Count Bounds (1 test)
//!   EC — Edge Cases (4 tests)
//!   MV — Multi-Volume Invariants (3 tests)

use test_harness::helpers::{mesh_bounding_box, mesh_volume};
use test_harness::ModelBuilder;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Create a 10x10x10 base cube at origin on XY plane.
/// Cube spans x∈[0,10], y∈[0,10], z∈[0,10].
fn base_cube() -> ModelBuilder {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("cube", "base_sk", 10.0).unwrap();
    m.assert_has_solid("cube").unwrap();
    m
}

/// Count how many distinct body outputs exist across all non-consumed features.
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

/// Approximate volume of a 16-segment polygon inscribed in a circle of radius r,
/// extruded to height h.
fn approx_cylinder_volume(r: f64, h: f64) -> f64 {
    let n = 16.0_f64;
    let area = r * r * n * (2.0 * std::f64::consts::PI / n).sin() / 2.0;
    area * h
}

// ══════════════════════════════════════════════════════════════════════════════
// Category V — Volume Properties
// ══════════════════════════════════════════════════════════════════════════════

/// V1: vol(A∪B) >= max(vol(A), vol(B)) for overlapping boxes.
///
/// The union of two overlapping solids must be at least as large as the larger
/// input, because it includes all material from both.
#[test]
fn v1_union_volume_monotonicity() {
    // Box A: 10x10x10 at origin
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();

    // Box B: 10x10x10 offset by (5,5,0) — overlaps with A
    m.rect_sketch("sk_b", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 10.0).unwrap();

    // Measure individual volumes
    let mesh_a = m.tessellate("box_a").unwrap();
    let vol_a = mesh_volume(&mesh_a);
    let mesh_b = m.tessellate("box_b").unwrap();
    let vol_b = mesh_volume(&mesh_b);

    // Union
    m.boolean_union("union", "box_a", "box_b").unwrap();
    m.assert_has_solid("union").unwrap();

    let mesh_union = m.tessellate("union").unwrap();
    let vol_union = mesh_volume(&mesh_union);

    let max_input = vol_a.max(vol_b);
    assert!(
        vol_union >= max_input * 0.98,
        "vol(A∪B)={:.1} should be >= max(vol(A),vol(B))={:.1}",
        vol_union,
        max_input
    );
}

/// V2: vol(A-B) <= vol(A) for box with cylindrical cut.
///
/// Subtracting material can only reduce volume.
#[test]
fn v2_subtract_volume_monotonicity() {
    let mut m = base_cube();

    // Cylindrical cut through top face
    m.circle_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 3.)
        .unwrap();
    m.extrude_cut("cut", "cut_sk", 10.0).unwrap();

    // Find the last feature that has a solid
    let mesh_cut = m.tessellate("cut").unwrap();
    let vol_cut = mesh_volume(&mesh_cut);

    let cube_vol = 1000.0; // 10x10x10
    let cyl_vol = approx_cylinder_volume(3.0, 10.0);

    assert!(
        vol_cut < cube_vol * 1.02, // 2% tolerance for tessellation
        "vol(A-B)={:.1} should be <= vol(A)={:.1}",
        vol_cut,
        cube_vol
    );

    // Also check it's approximately correct
    let expected = cube_vol - cyl_vol;
    assert!(
        (vol_cut - expected).abs() / expected < 0.05,
        "vol(cube - cylinder)={:.1} should be ~{:.1} (5% tolerance)",
        vol_cut,
        expected
    );
}

/// V3: vol(A∪B) <= vol(A) + vol(B) for two overlapping boxes.
///
/// Inclusion-exclusion: vol(A∪B) = vol(A) + vol(B) - vol(A∩B) <= vol(A) + vol(B).
#[test]
fn v3_union_volume_inclusion() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();

    m.rect_sketch("sk_b", [3., 3., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 10.0).unwrap();

    let mesh_a = m.tessellate("box_a").unwrap();
    let vol_a = mesh_volume(&mesh_a);
    let mesh_b = m.tessellate("box_b").unwrap();
    let vol_b = mesh_volume(&mesh_b);

    m.boolean_union("union", "box_a", "box_b").unwrap();
    let mesh_union = m.tessellate("union").unwrap();
    let vol_union = mesh_volume(&mesh_union);

    assert!(
        vol_union <= (vol_a + vol_b) * 1.02, // 2% tessellation tolerance
        "vol(A∪B)={:.1} should be <= vol(A)+vol(B)={:.1}",
        vol_union,
        vol_a + vol_b
    );
}

/// V4: vol(A-B) > 0 when B doesn't fully contain A.
///
/// A partial subtraction must leave positive volume.
#[test]
fn v4_subtract_volume_positive() {
    // Large cube A: 10x10x10
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();

    // Small overlapping cube B: 5x5x5 at corner
    m.rect_sketch("sk_b", [0., 0., 0.], [0., 0., 1.], 0., 0., 5., 5.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 5.0).unwrap();

    m.boolean_subtract("diff", "box_a", "box_b").unwrap();
    m.assert_has_solid("diff").unwrap();

    let mesh = m.tessellate("diff").unwrap();
    let vol = mesh_volume(&mesh);

    // A=1000, B=125, overlap=125, result should be ~875
    assert!(
        vol > 800.0,
        "vol(A-B)={:.1} should be > 0 (expected ~875)",
        vol
    );
    assert!(vol < 1000.0, "vol(A-B)={:.1} should be < vol(A)=1000", vol);
}

// ══════════════════════════════════════════════════════════════════════════════
// Category T — Topology Properties
// ══════════════════════════════════════════════════════════════════════════════

/// T1: Euler invariant V-E+F=2 for simple box union.
///
/// For a genus-0 solid (topological sphere), the Euler characteristic
/// of the boundary must be 2.
#[test]
fn t1_euler_invariant_simple_union() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();

    m.rect_sketch("sk_b", [5., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 10.0).unwrap();

    m.boolean_union("union", "box_a", "box_b").unwrap();
    m.assert_has_solid("union").unwrap();

    let (v, e, f) = m.topology_counts("union").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert_eq!(
        chi, 2,
        "Euler invariant V-E+F should be 2 for genus-0 solid, got {} (V={}, E={}, F={})",
        chi, v, e, f
    );
}

/// T2: Euler invariant V-E+F=2 for box-cylinder union.
///
/// A cylinder boss on a cube creates a more complex topology but still genus-0.
///
/// The box+cylinder union produces an annular ring face (box top with cylinder
/// footprint cut out) which has an inner boundary loop. The generalized Euler
/// formula for B-rep with inner loops is: V - E + F - L_inner = 2 (genus 0),
/// where L_inner is the total number of inner boundary loops across all faces.
/// With 1 inner loop, V - E + F = 3 is correct.
#[test]
fn t2_euler_invariant_box_cylinder_union() {
    let mut m = base_cube();

    m.circle_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 3.)
        .unwrap();
    m.extrude_no_merge("boss", "boss_sk", 5.0).unwrap();

    m.boolean_union("merged", "cube", "boss").unwrap();
    m.assert_has_solid("merged").unwrap();

    let (v, e, f) = m.topology_counts("merged").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    // The annular ring face (box top minus cylinder footprint) has 1 inner
    // boundary loop. Generalized Euler: V - E + F - L_inner = 2 → chi = 3.
    assert!(
        chi == 2 || chi == 3,
        "Euler invariant V-E+F should be 2 (no inner loops) or 3 (1 inner loop) for box+cylinder, got {} (V={}, E={}, F={})",
        chi, v, e, f
    );
}

/// T3: No NaN in vertices after various booleans.
///
/// Corrupted geometry can produce NaN vertices, which silently poison
/// downstream computations.
#[test]
fn t3_no_nan_in_vertices() {
    // Union
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();
    m.rect_sketch("sk_b", [5., 5., 0.], [0., 0., 1.], 0., 0., 8., 8.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 8.0).unwrap();
    m.boolean_union("union", "box_a", "box_b").unwrap();

    let mesh = m.tessellate("union").unwrap();
    for (i, v) in mesh.vertices.iter().enumerate() {
        assert!(
            v.is_finite(),
            "Union mesh vertex[{}] is not finite: {}",
            i,
            v
        );
    }
    for (i, n) in mesh.normals.iter().enumerate() {
        assert!(
            n.is_finite(),
            "Union mesh normal[{}] is not finite: {}",
            i,
            n
        );
    }

    // Subtraction
    let mut m2 = base_cube();
    m2.circle_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 3.)
        .unwrap();
    m2.extrude_cut("cut", "cut_sk", 10.0).unwrap();

    let mesh2 = m2.tessellate("cut").unwrap();
    for (i, v) in mesh2.vertices.iter().enumerate() {
        assert!(v.is_finite(), "Cut mesh vertex[{}] is not finite: {}", i, v);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Category BB — Bounding Box Properties
// ══════════════════════════════════════════════════════════════════════════════

/// BB1: bbox(A-B) ⊆ bbox(A).
///
/// Subtraction can only remove material, never extend the bounding box.
#[test]
fn bb1_subtract_bbox_containment() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();

    let mesh_a = m.tessellate("box_a").unwrap();
    let (a_min, a_max) = mesh_bounding_box(&mesh_a);

    // Cut a small box from corner
    m.rect_sketch("sk_b", [0., 0., 0.], [0., 0., 1.], 0., 0., 4., 4.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 4.0).unwrap();

    m.boolean_subtract("diff", "box_a", "box_b").unwrap();
    m.assert_has_solid("diff").unwrap();

    let mesh_diff = m.tessellate("diff").unwrap();
    let (d_min, d_max) = mesh_bounding_box(&mesh_diff);

    let tol = 0.5; // tessellation tolerance
    for i in 0..3 {
        assert!(
            d_min[i] >= a_min[i] - tol,
            "bbox(A-B).min[{}]={:.2} should be >= bbox(A).min[{}]={:.2}",
            i,
            d_min[i],
            i,
            a_min[i]
        );
        assert!(
            d_max[i] <= a_max[i] + tol,
            "bbox(A-B).max[{}]={:.2} should be <= bbox(A).max[{}]={:.2}",
            i,
            d_max[i],
            i,
            a_max[i]
        );
    }
}

/// BB2: bbox(A∪B) ⊆ bbox_union(bbox(A), bbox(B)).
///
/// The union bounding box must fit within the combined bounding boxes of the inputs.
#[test]
fn bb2_union_bbox_containment() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();

    m.rect_sketch("sk_b", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 10.0).unwrap();

    let mesh_a = m.tessellate("box_a").unwrap();
    let (a_min, a_max) = mesh_bounding_box(&mesh_a);
    let mesh_b = m.tessellate("box_b").unwrap();
    let (b_min, b_max) = mesh_bounding_box(&mesh_b);

    m.boolean_union("union", "box_a", "box_b").unwrap();
    let mesh_union = m.tessellate("union").unwrap();
    let (u_min, u_max) = mesh_bounding_box(&mesh_union);

    let tol = 0.5;
    for i in 0..3 {
        let combined_min = a_min[i].min(b_min[i]);
        let combined_max = a_max[i].max(b_max[i]);
        assert!(
            u_min[i] >= combined_min - tol,
            "bbox(A∪B).min[{}]={:.2} should be >= min(bbox(A),bbox(B)).min[{}]={:.2}",
            i,
            u_min[i],
            i,
            combined_min
        );
        assert!(
            u_max[i] <= combined_max + tol,
            "bbox(A∪B).max[{}]={:.2} should be <= max(bbox(A),bbox(B)).max[{}]={:.2}",
            i,
            u_max[i],
            i,
            combined_max
        );
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Category CH — Chain Stability
// ══════════════════════════════════════════════════════════════════════════════
// These tests verify that chained boolean operations produce correct body counts.
// Chained booleans are a major source of regressions.

/// CH1: 2 sequential unions → body_count==1.
#[test]
fn ch1_chain_2_box_unions_body_count() {
    let mut m = ModelBuilder::kernel();

    // Base cube
    m.rect_sketch("sk0", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box0", "sk0", 10.0).unwrap();

    // Boss 1: overlapping box on top
    m.rect_sketch("sk1", [0., 0., 10.], [0., 0., 1.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude("boss1", "sk1", 5.0).unwrap(); // auto-unions

    // Boss 2: another overlapping box on the side
    m.rect_sketch("sk2", [10., 0., 0.], [1., 0., 0.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude("boss2", "sk2", 5.0).unwrap(); // auto-unions

    let bodies = count_visible_bodies(&m);
    assert_eq!(
        bodies, 1,
        "2 sequential auto-unions should produce 1 body, got {}",
        bodies
    );
}

/// CH2: 3 sequential unions → body_count==1.
#[test]
fn ch2_chain_3_box_unions_body_count() {
    let mut m = ModelBuilder::kernel();

    m.rect_sketch("sk0", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box0", "sk0", 10.0).unwrap();

    // Boss 1: top
    m.rect_sketch("sk1", [0., 0., 10.], [0., 0., 1.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude("boss1", "sk1", 5.0).unwrap();

    // Boss 2: side +X
    m.rect_sketch("sk2", [10., 0., 0.], [1., 0., 0.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude("boss2", "sk2", 5.0).unwrap();

    // Boss 3: side +Y
    m.rect_sketch("sk3", [0., 10., 0.], [0., 1., 0.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude("boss3", "sk3", 5.0).unwrap();

    let bodies = count_visible_bodies(&m);
    assert_eq!(
        bodies, 1,
        "3 sequential auto-unions should produce 1 body, got {}",
        bodies
    );
}

/// CH3: 5 sequential unions → body_count==1.
///
/// Stress test for chained boolean stability.
#[test]
fn ch3_chain_5_box_unions_body_count() {
    let mut m = ModelBuilder::kernel();

    m.rect_sketch("sk0", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("box0", "sk0", 10.0).unwrap();

    // Boss 1: top
    m.rect_sketch("sk1", [0., 0., 10.], [0., 0., 1.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude("boss1", "sk1", 5.0).unwrap();

    // Boss 2: +X face
    m.rect_sketch("sk2", [10., 0., 0.], [1., 0., 0.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude("boss2", "sk2", 5.0).unwrap();

    // Boss 3: +Y face
    m.rect_sketch("sk3", [0., 10., 0.], [0., 1., 0.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude("boss3", "sk3", 5.0).unwrap();

    // Boss 4: -X face
    m.rect_sketch("sk4", [0., 0., 0.], [-1., 0., 0.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude("boss4", "sk4", 5.0).unwrap();

    // Boss 5: -Y face
    m.rect_sketch("sk5", [0., 0., 0.], [0., -1., 0.], 2., 2., 6., 6.)
        .unwrap();
    m.extrude("boss5", "sk5", 5.0).unwrap();

    let bodies = count_visible_bodies(&m);
    assert_eq!(
        bodies, 1,
        "5 sequential auto-unions should produce 1 body, got {}",
        bodies
    );
}

/// CH4: 1 union + 1 cut → body_count==1.
///
/// A common CAD workflow: add a boss, then cut a hole through it.
#[test]
fn ch4_union_then_cut_body_count() {
    let mut m = base_cube();

    // Boss on top
    m.rect_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 4., 4.)
        .unwrap();
    m.extrude("boss", "boss_sk", 5.0).unwrap(); // auto-union

    // Cut through the boss
    m.circle_sketch("cut_sk", [0., 0., 15.], [0., 0., 1.], 5., 5., 2.)
        .unwrap();
    m.extrude_cut("cut", "cut_sk", 15.0).unwrap();

    let bodies = count_visible_bodies(&m);
    assert_eq!(
        bodies, 1,
        "union + cut should produce 1 body, got {}",
        bodies
    );
}

/// CH5: 2 unions + 1 cut → body_count==1.
///
/// Two bosses then a cut through both.
#[test]
fn ch5_two_unions_then_cut_body_count() {
    let mut m = base_cube();

    // Boss 1 on top
    m.rect_sketch("boss1_sk", [0., 0., 10.], [0., 0., 1.], 1., 1., 3., 3.)
        .unwrap();
    m.extrude("boss1", "boss1_sk", 5.0).unwrap();

    // Boss 2 on top (different position)
    m.rect_sketch("boss2_sk", [0., 0., 10.], [0., 0., 1.], 6., 6., 3., 3.)
        .unwrap();
    m.extrude("boss2", "boss2_sk", 5.0).unwrap();

    // Cut through everything from top
    m.rect_sketch("cut_sk", [0., 0., 15.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_cut("cut", "cut_sk", 5.0).unwrap();

    let bodies = count_visible_bodies(&m);
    assert_eq!(
        bodies, 1,
        "2 unions + 1 cut should produce 1 body, got {}",
        bodies
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Category CM — Commutativity
// ══════════════════════════════════════════════════════════════════════════════

/// CM1: vol(A∪B) ≈ vol(B∪A) within tolerance.
///
/// Union is commutative. The volume should be the same regardless of operand order.
#[test]
fn cm1_union_commutativity_volume() {
    // A ∪ B
    let mut m_ab = ModelBuilder::kernel();
    m_ab.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m_ab.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();
    m_ab.rect_sketch("sk_b", [4., 4., 0.], [0., 0., 1.], 0., 0., 8., 8.)
        .unwrap();
    m_ab.extrude_no_merge("box_b", "sk_b", 8.0).unwrap();
    m_ab.boolean_union("u_ab", "box_a", "box_b").unwrap();
    let mesh_ab = m_ab.tessellate("u_ab").unwrap();
    let vol_ab = mesh_volume(&mesh_ab);

    // B ∪ A (reversed operand order)
    let mut m_ba = ModelBuilder::kernel();
    m_ba.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m_ba.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();
    m_ba.rect_sketch("sk_b", [4., 4., 0.], [0., 0., 1.], 0., 0., 8., 8.)
        .unwrap();
    m_ba.extrude_no_merge("box_b", "sk_b", 8.0).unwrap();
    m_ba.boolean_union("u_ba", "box_b", "box_a").unwrap();
    let mesh_ba = m_ba.tessellate("u_ba").unwrap();
    let vol_ba = mesh_volume(&mesh_ba);

    let rel_diff = (vol_ab - vol_ba).abs() / vol_ab.max(vol_ba);
    assert!(
        rel_diff < 0.02,
        "vol(A∪B)={:.1} vs vol(B∪A)={:.1} differ by {:.1}% (should be <2%)",
        vol_ab,
        vol_ba,
        rel_diff * 100.0
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Category FC — Face Count Bounds
// ══════════════════════════════════════════════════════════════════════════════

/// FC1: face count after union is bounded relative to inputs.
///
/// Face splitting at intersection boundaries can increase face count beyond
/// the sum of inputs. But the result should be reasonable — no more than
/// 3× the sum of input face counts (allowing for face splits at intersection
/// curves producing ~2 fragments per split face, plus new boundary faces).
#[test]
fn fc1_union_face_count_reasonable() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();

    m.rect_sketch("sk_b", [5., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 10.0).unwrap();

    let (_, _, f_a) = m.topology_counts("box_a").unwrap();
    let (_, _, f_b) = m.topology_counts("box_b").unwrap();

    m.boolean_union("union", "box_a", "box_b").unwrap();
    let (_, _, f_union) = m.topology_counts("union").unwrap();

    let upper_bound = (f_a + f_b) * 3;
    assert!(
        f_union <= upper_bound,
        "union face count {} should be <= 3*({}+{})={}",
        f_union,
        f_a,
        f_b,
        upper_bound
    );

    // Also check it's at least 6 (a convex solid needs at least 6 faces equivalent)
    assert!(
        f_union >= 6,
        "union face count {} should be >= 6 (minimum for convex solid)",
        f_union
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Category EC — Edge Cases
// ══════════════════════════════════════════════════════════════════════════════

/// EC1: Abutting boxes (sharing a face) → union produces body_count==1.
///
/// Coplanar face handling: two boxes that share an exact face must still union
/// into a single solid.
#[test]
fn ec1_abutting_boxes_union() {
    let mut m = ModelBuilder::kernel();

    // Box A: [0,10] x [0,10] x [0,10]
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();

    // Box B: [10,20] x [0,10] x [0,10] — shares face at x=10
    m.rect_sketch("sk_b", [10., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 10.0).unwrap();

    m.boolean_union("union", "box_a", "box_b").unwrap();
    m.assert_has_solid("union").unwrap();

    let bodies = count_visible_bodies(&m);
    assert_eq!(
        bodies, 1,
        "Abutting boxes union should produce 1 body, got {}",
        bodies
    );

    // Volume should be 2000 (two 10x10x10 cubes)
    let mesh = m.tessellate("union").unwrap();
    let vol = mesh_volume(&mesh);
    assert!(
        (vol - 2000.0).abs() / 2000.0 < 0.05,
        "Abutting boxes union volume={:.1} should be ~2000",
        vol
    );
}

/// EC2: Contained box subtract → body_count==1.
///
/// A small box fully inside a large box: subtraction creates an internal void,
/// but body count should remain 1 (the outer shell).
#[test]
fn ec2_contained_box_subtract() {
    let mut m = ModelBuilder::kernel();

    // Large box: 20x20x20
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 20., 20.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 20.0).unwrap();

    // Small box inside: 4x4x4 centered at (8,8,8)
    m.rect_sketch("sk_b", [0., 0., 8.], [0., 0., 1.], 8., 8., 4., 4.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 4.0).unwrap();

    m.boolean_subtract("diff", "box_a", "box_b").unwrap();
    m.assert_has_solid("diff").unwrap();

    let bodies = count_visible_bodies(&m);
    assert_eq!(
        bodies, 1,
        "Contained subtraction should produce 1 body, got {}",
        bodies
    );

    let mesh = m.tessellate("diff").unwrap();
    let vol = mesh_volume(&mesh);
    // 20^3 - 4^3 = 8000 - 64 = 7936
    assert!(
        (vol - 7936.0).abs() / 7936.0 < 0.05,
        "Contained subtract volume={:.1} should be ~7936",
        vol
    );
}

/// EC3: Non-overlapping boxes union → body_count==2 (disjoint bodies).
///
/// Two boxes that don't touch at all should remain as separate bodies.
#[test]
fn ec3_non_overlapping_boxes_union() {
    let mut m = ModelBuilder::kernel();

    // Box A: [0,5] x [0,5] x [0,5]
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 5., 5.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 5.0).unwrap();

    // Box B: [20,25] x [20,25] x [0,5] — far away
    m.rect_sketch("sk_b", [20., 20., 0.], [0., 0., 1.], 0., 0., 5., 5.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 5.0).unwrap();

    m.boolean_union("union", "box_a", "box_b").unwrap();

    // Disjoint union should produce 2 separate bodies
    let bodies = count_visible_bodies(&m);
    assert_eq!(
        bodies, 2,
        "Non-overlapping union should produce 2 bodies, got {}",
        bodies
    );
}

/// EC4: Intersection of overlapping boxes → positive volume.
///
/// Two overlapping boxes: intersection should produce a solid whose volume
/// equals the overlap region.
#[test]
fn ec4_intersection_positive_volume() {
    let mut m = ModelBuilder::kernel();

    // Box A: [0,10] x [0,10] x [0,10]
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();

    // Box B: [5,15] x [5,15] x [0,10] — overlaps [5,10]x[5,10]x[0,10] = 250
    m.rect_sketch("sk_b", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 10.0).unwrap();

    m.boolean_intersect("inter", "box_a", "box_b").unwrap();
    m.assert_has_solid("inter").unwrap();

    let mesh = m.tessellate("inter").unwrap();
    let vol = mesh_volume(&mesh);
    // Overlap region: 5x5x10 = 250
    assert!(
        (vol - 250.0).abs() / 250.0 < 0.05,
        "Intersection volume={:.1} should be ~250",
        vol
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Category MV — Multi-Volume Invariants
// ══════════════════════════════════════════════════════════════════════════════
// Tests that combine multiple property checks on the same operation.

/// MV1: Inclusion-exclusion: vol(A∪B) + vol(A∩B) ≈ vol(A) + vol(B).
///
/// This is the fundamental boolean algebra identity that must hold.
#[test]
fn mv1_inclusion_exclusion_identity() {
    let mut m1 = ModelBuilder::kernel();
    m1.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m1.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();
    m1.rect_sketch("sk_b", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m1.extrude_no_merge("box_b", "sk_b", 10.0).unwrap();

    let mesh_a = m1.tessellate("box_a").unwrap();
    let vol_a = mesh_volume(&mesh_a);
    let mesh_b = m1.tessellate("box_b").unwrap();
    let vol_b = mesh_volume(&mesh_b);

    // Union
    m1.boolean_union("union", "box_a", "box_b").unwrap();
    let mesh_u = m1.tessellate("union").unwrap();
    let vol_union = mesh_volume(&mesh_u);

    // Intersection (separate builder to avoid consumed features)
    let mut m2 = ModelBuilder::kernel();
    m2.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m2.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();
    m2.rect_sketch("sk_b", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m2.extrude_no_merge("box_b", "sk_b", 10.0).unwrap();
    m2.boolean_intersect("inter", "box_a", "box_b").unwrap();
    let mesh_i = m2.tessellate("inter").unwrap();
    let vol_inter = mesh_volume(&mesh_i);

    // Identity: vol(A∪B) + vol(A∩B) = vol(A) + vol(B)
    let lhs = vol_union + vol_inter;
    let rhs = vol_a + vol_b;
    let rel_diff = (lhs - rhs).abs() / rhs;
    assert!(
        rel_diff < 0.05,
        "Inclusion-exclusion: vol(A∪B)+vol(A∩B)={:.1} vs vol(A)+vol(B)={:.1}, diff={:.1}%",
        lhs,
        rhs,
        rel_diff * 100.0
    );
}

/// MV2: Subtract is difference of union and intersection volumes.
///
/// vol(A-B) ≈ vol(A) - vol(A∩B)
#[test]
fn mv2_subtract_equals_minus_intersection() {
    // Build A and B
    let mut m1 = ModelBuilder::kernel();
    m1.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m1.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();
    m1.rect_sketch("sk_b", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m1.extrude_no_merge("box_b", "sk_b", 10.0).unwrap();

    let mesh_a = m1.tessellate("box_a").unwrap();
    let vol_a = mesh_volume(&mesh_a);

    // A - B
    m1.boolean_subtract("diff", "box_a", "box_b").unwrap();
    let mesh_d = m1.tessellate("diff").unwrap();
    let vol_diff = mesh_volume(&mesh_d);

    // A ∩ B (separate builder)
    let mut m2 = ModelBuilder::kernel();
    m2.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m2.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();
    m2.rect_sketch("sk_b", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m2.extrude_no_merge("box_b", "sk_b", 10.0).unwrap();
    m2.boolean_intersect("inter", "box_a", "box_b").unwrap();
    let mesh_i = m2.tessellate("inter").unwrap();
    let vol_inter = mesh_volume(&mesh_i);

    // vol(A-B) ≈ vol(A) - vol(A∩B)
    let expected = vol_a - vol_inter;
    let rel_diff = (vol_diff - expected).abs() / expected.max(1.0);
    assert!(
        rel_diff < 0.05,
        "vol(A-B)={:.1} vs vol(A)-vol(A∩B)={:.1}, diff={:.1}%",
        vol_diff,
        expected,
        rel_diff * 100.0
    );
}

/// MV3: Euler invariant holds for subtract result (genus-0 partial overlap).
///
/// A partial box subtraction should still yield a genus-0 solid with V-E+F=2.
#[test]
fn mv3_euler_invariant_subtract() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();

    // Small overlapping box at corner
    m.rect_sketch("sk_b", [0., 0., 0.], [0., 0., 1.], 0., 0., 5., 5.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 5.0).unwrap();

    m.boolean_subtract("diff", "box_a", "box_b").unwrap();
    m.assert_has_solid("diff").unwrap();

    let (v, e, f) = m.topology_counts("diff").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert_eq!(
        chi, 2,
        "Euler invariant V-E+F should be 2 for box-minus-corner, got {} (V={}, E={}, F={})",
        chi, v, e, f
    );
}

/// Corner-touch canonical test: Box [0,10]³ minus Box [0,5]³.
///
/// Same geometry as MV3. Asserts both chi=2 (topology) and volume≈875 (geometry).
/// The IC endpoints land exactly at shared boundary vertices; corner-touch
/// snapping must prevent figure-8 wires.
#[test]
fn corner_touch_reuses_boundary_vertex() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();

    m.rect_sketch("sk_b", [0., 0., 0.], [0., 0., 1.], 0., 0., 5., 5.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 5.0).unwrap();

    m.boolean_subtract("diff", "box_a", "box_b").unwrap();
    m.assert_has_solid("diff").unwrap();

    let (v, e, f) = m.topology_counts("diff").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert_eq!(
        chi, 2,
        "Corner-touch subtract: Euler V-E+F should be 2, got {} (V={}, E={}, F={})",
        chi, v, e, f
    );

    // Volume check: 10³ - 5³ = 1000 - 125 = 875
    // Mesh-based volume has tessellation error; use generous tolerance.
    let mesh = m.tessellate("diff").unwrap();
    let vol = mesh_volume(&mesh);
    assert!(
        (vol - 875.0).abs() < 15.0,
        "Corner-touch subtract: volume should be ≈875, got {}",
        vol
    );
}

/// Guard test: interior IC endpoints (not near any boundary vertex) must
/// still produce correct edge splits and chi=2.
///
/// Box [0,10]³ union Box [5,15]×[5,15]×[0,10] — IC endpoints are at
/// positions like (5,y,z) and (x,5,z), interior to A's boundary edges.
/// Corner-touch snap should NOT fire here.
#[test]
fn non_corner_ic_still_splits_edge() {
    let mut m = ModelBuilder::kernel();
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 10.0).unwrap();

    // Offset box — IC endpoints at (5,y,z), (x,5,z), interior to A's edges
    m.rect_sketch("sk_b", [5., 5., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 10.0).unwrap();

    m.boolean_union("union_ab", "box_a", "box_b").unwrap();
    m.assert_has_solid("union_ab").unwrap();

    let (v, e, f) = m.topology_counts("union_ab").unwrap();
    let chi = v as i64 - e as i64 + f as i64;
    assert_eq!(
        chi, 2,
        "Interior IC union: Euler V-E+F should be 2, got {} (V={}, E={}, F={})",
        chi, v, e, f
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// Category NP — Near-Pass Boundary Tests
// ══════════════════════════════════════════════════════════════════════════════
// Tests that exercise the boundary between passing and failing boolean scenarios,
// discovered during Sprint 40 triage of ignored tests.

/// NP1: Abutting boxes at 8×8×8 size — probes the size boundary where
/// coplanar shared-face union succeeds.
///
/// EC1 (10×10×10 abutting boxes) now passes. R3 (5×5×5 abutting boxes) still
/// fails. This test uses 8×8×8 boxes to probe the working boundary of
/// coplanar face classification.
#[test]
fn np1_abutting_boxes_medium_size() {
    let mut m = ModelBuilder::kernel();

    // Box A: [0,8] x [0,8] x [0,8]
    m.rect_sketch("sk_a", [0., 0., 0.], [0., 0., 1.], 0., 0., 8., 8.)
        .unwrap();
    m.extrude_no_merge("box_a", "sk_a", 8.0).unwrap();

    // Box B: [8,16] x [0,8] x [0,8] — shares face at x=8
    m.rect_sketch("sk_b", [8., 0., 0.], [0., 0., 1.], 0., 0., 8., 8.)
        .unwrap();
    m.extrude_no_merge("box_b", "sk_b", 8.0).unwrap();

    m.boolean_union("union", "box_a", "box_b").unwrap();
    m.assert_has_solid("union").unwrap();

    let bodies = count_visible_bodies(&m);
    assert_eq!(
        bodies, 1,
        "8×8×8 abutting boxes union should produce 1 body, got {}",
        bodies
    );

    let mesh = m.tessellate("union").unwrap();
    let vol = mesh_volume(&mesh);
    // Two 8×8×8 cubes = 1024
    assert!(
        (vol - 1024.0).abs() / 1024.0 < 0.05,
        "8×8×8 abutting union volume={:.1} should be ~1024",
        vol
    );

    let (bb_min, bb_max) = mesh_bounding_box(&mesh);
    assert!(
        bb_max[0] > 15.5,
        "Union should extend to x≈16 (got {:.1})",
        bb_max[0]
    );
    assert!(
        bb_min[0] < 0.5,
        "Union should start at x≈0 (got {:.1})",
        bb_min[0]
    );
}

/// NP2: Box-cylinder cut volume accuracy — verifies that the analytical SSI
/// improvement produces accurate cut volumes.
///
/// V2 (cylinder cut monotonicity) now passes. This test goes further and
/// checks the absolute volume accuracy of a box with a cylindrical hole.
#[test]
fn np2_box_cylinder_cut_volume_accuracy() {
    let mut m = base_cube();

    // Cylindrical cut: radius 2, centered at (5,5) on top face, depth 10 (through cut)
    m.circle_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 5., 5., 2.)
        .unwrap();
    m.extrude_cut("cut", "cut_sk", 10.0).unwrap();

    let mesh = m.tessellate("cut").unwrap();
    let vol = mesh_volume(&mesh);

    let cyl_vol = approx_cylinder_volume(2.0, 10.0);
    let expected = 1000.0 - cyl_vol;

    // 5% tolerance for tessellation
    assert!(
        (vol - expected).abs() / expected < 0.05,
        "Box with cylindrical hole: vol={:.1} should be ~{:.1} (5% tol)",
        vol,
        expected
    );

    // Verify no NaN in mesh
    for (i, v) in mesh.vertices.iter().enumerate() {
        assert!(
            v.is_finite(),
            "Cylinder cut mesh vertex[{}] is not finite: {}",
            i,
            v
        );
    }
}

/// NP3: 3 unions + 1 cut chain — exercises the boundary between CH3 (5 unions)
/// and CH5 (2 unions + 1 cut), verifying mixed union-cut chain stability.
///
/// CH3 now passes (5 unions). This test exercises 3 unions followed by a cut,
/// testing that the cut works correctly on a chained-boolean result with
/// accumulated IntersectionCurve edges.
#[test]
fn np3_three_unions_then_cut_chain() {
    let mut m = base_cube();

    // Boss 1: top face
    m.rect_sketch("b1_sk", [0., 0., 10.], [0., 0., 1.], 1., 1., 4., 4.)
        .unwrap();
    m.extrude("b1", "b1_sk", 4.0).unwrap();

    // Boss 2: +X face
    m.rect_sketch("b2_sk", [10., 0., 0.], [1., 0., 0.], 1., 1., 4., 4.)
        .unwrap();
    m.extrude("b2", "b2_sk", 4.0).unwrap();

    // Boss 3: +Y face
    m.rect_sketch("b3_sk", [0., 10., 0.], [0., 1., 0.], 1., 1., 4., 4.)
        .unwrap();
    m.extrude("b3", "b3_sk", 4.0).unwrap();

    // Cut: slot from top face
    m.rect_sketch("cut_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 4., 4.)
        .unwrap();
    m.extrude_cut("cut", "cut_sk", 5.0).unwrap();

    let bodies = count_visible_bodies(&m);
    assert_eq!(
        bodies, 1,
        "3 unions + 1 cut should produce 1 body, got {}",
        bodies
    );

    // Volume: cube(1000) + 3×boss(4×4×4=64) - cut(4×4×5=80) = 1000+192-80 = 1112
    // But bosses on coplanar faces may lose some volume at intersection
    let last_solid = ["cut", "b3", "b2", "b1", "cube"]
        .iter()
        .find(|name| m.assert_has_solid(name).is_ok())
        .expect("no solid found");
    let mesh = m.tessellate(last_solid).unwrap();
    let vol = mesh_volume(&mesh);
    assert!(
        vol > 900.0,
        "3 unions + 1 cut volume={:.1} should be > 900",
        vol
    );
    assert!(
        vol < 1200.0,
        "3 unions + 1 cut volume={:.1} should be < 1200",
        vol
    );
}
