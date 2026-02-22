//! Tests that replay the user-reported .waffle test cases through TruckKernel.
//!
//! These tests recreate the feature sequences from:
//!   - app/tests/cases/several-extrudes.waffle
//!   - app/tests/cases/multi-cut.waffle
//!
//! The goal is to observe what happens (body counts, union success, cut success)
//! — NOT to fix anything. All results are printed via eprintln for diagnosis.

use test_harness::helpers::{mesh_bounding_box, mesh_volume};
use test_harness::ModelBuilder;

// ═══════════════════════════════════════════════════════════════════════════
// Helper: count total visible bodies across all non-consumed features
// ═══════════════════════════════════════════════════════════════════════════

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

/// Print full diagnostic info about the engine state.
fn print_diagnostics(m: &ModelBuilder, label: &str) {
    eprintln!("\n=== {} ===", label);

    let consumed = m.consumed_features();
    eprintln!("  Features: {}", m.state.engine.tree.features.len());
    eprintln!("  Consumed features: {:?}", consumed.len());

    for feature in &m.state.engine.tree.features {
        let consumed_marker = if consumed.contains(&feature.id) {
            " [CONSUMED]"
        } else {
            ""
        };
        let suppressed_marker = if feature.suppressed {
            " [SUPPRESSED]"
        } else {
            ""
        };

        if let Some(result) = m.state.engine.get_result(feature.id) {
            let output_count = result.outputs.len();
            let warnings = &result.diagnostics.warnings;
            let warn_str = if warnings.is_empty() {
                String::new()
            } else {
                format!(" WARNINGS: {:?}", warnings)
            };
            eprintln!(
                "  Feature '{}' ({}): {} output(s){}{}{}",
                feature.name,
                &feature.id.to_string()[..8],
                output_count,
                consumed_marker,
                suppressed_marker,
                warn_str,
            );
        } else {
            eprintln!(
                "  Feature '{}' ({}): NO RESULT{}{}",
                feature.name,
                &feature.id.to_string()[..8],
                consumed_marker,
                suppressed_marker,
            );
        }
    }

    // Engine errors
    let errors = &m.state.engine.errors;
    if errors.is_empty() {
        eprintln!("  Engine errors: NONE");
    } else {
        eprintln!("  Engine errors ({}):", errors.len());
        for (id, msg) in errors {
            eprintln!("    {} → {}", &id.to_string()[..8], msg);
        }
    }

    eprintln!("  Visible bodies: {}", count_visible_bodies(m));
}

// ═══════════════════════════════════════════════════════════════════════════
// Test Case 1: "Several extrudes" (several-extrudes.waffle)
// ═══════════════════════════════════════════════════════════════════════════
//
// Feature sequence (all sketches on YZ plane, normal=[1,0,0]):
//   1. Sketch (rect) at origin [0,0,0] → Extrude depth=10, cut=false, merge=true
//   2. Sketch (rect) at origin [10,*,*] → Extrude depth=10, cut=false, merge=true
//   3. Sketch (rect) at origin [20,*,*] → Extrude depth=10, cut=false, merge=true
//   4. Sketch (circle) at origin [30,*,*] → Extrude depth=10, cut=false, merge=true
//   5. Sketch (rect) at origin [0,*,*], normal=[-1,0,0] → Extrude depth=100, cut=true
//   6. Same sketch → Extrude depth=100, cut=true (duplicate)
//
// Coordinate mapping for normal=[1,0,0]:
//   tangent_x = [0,1,0], tangent_y = [0,0,1]
//   3D = origin + sketch_x * [0,1,0] + sketch_y * [0,0,1]
//
// For the .waffle file's first sketch:
//   Points: (11.73, 10.06), (-15.21, 10.06), (-15.21, -15.43), (11.73, -15.43)
//   → 3D rect from origin [0,0,0]:
//     y ∈ [-15.21, 11.73], z ∈ [-15.43, 10.06]
//   This is a ~27 x ~25 rectangle centered roughly around origin.
//
// For simplicity, use approximate rectangles of similar size.

#[test]
fn several_extrudes_replay() {
    eprintln!("\n╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║  TEST CASE 1: several-extrudes.waffle                      ║");
    eprintln!("╚══════════════════════════════════════════════════════════════╝");

    let mut m = ModelBuilder::truck();

    // --- Step 1: Rect sketch at origin [0,0,0], normal [1,0,0], extrude depth=10 ---
    // From .waffle: rect corners in sketch 2D ~ (-15, -15) to (12, 10)
    // For normal=[1,0,0]: sketch_x → +Y, sketch_y → +Z
    // 3D: y ∈ [-15, 12], z ∈ [-15, 10], x ∈ [0, 10]
    // Using approximate values from the file.
    m.rect_sketch(
        "sk1",
        [0., 0., 0.],
        [1., 0., 0.],
        -15.0,
        -15.0,
        27.0,
        25.0,
    )
    .unwrap();
    m.extrude("ext1", "sk1", 10.0).unwrap();

    let has_solid_1 = m.assert_has_solid("ext1").is_ok();
    let bodies_1 = count_visible_bodies(&m);
    eprintln!("\nStep 1: Rect extrude at x=0..10");
    eprintln!("  Has solid: {}", has_solid_1);
    eprintln!("  Visible bodies: {}", bodies_1);
    if has_solid_1 {
        let mesh = m.tessellate("ext1").unwrap();
        let vol = mesh_volume(&mesh);
        let (bb_min, bb_max) = mesh_bounding_box(&mesh);
        eprintln!("  Volume: {:.1}", vol);
        eprintln!(
            "  Bounding box: ({:.1}, {:.1}, {:.1}) → ({:.1}, {:.1}, {:.1})",
            bb_min[0], bb_min[1], bb_min[2], bb_max[0], bb_max[1], bb_max[2]
        );
    }

    // --- Step 2: Rect sketch at origin [10,*,*], normal [1,0,0], extrude depth=10 ---
    // From .waffle: rect ~ (-12, -24) to (12, -1) in sketch 2D
    // 3D: y ∈ [-12, 12], z ∈ [-24, -1], x ∈ [10, 20]
    m.rect_sketch(
        "sk2",
        [10., 0., 0.],
        [1., 0., 0.],
        -12.0,
        -24.0,
        24.0,
        23.0,
    )
    .unwrap();
    m.extrude("ext2", "sk2", 10.0).unwrap();

    let has_solid_2 = m.assert_has_solid("ext2").is_ok();
    let bodies_2 = count_visible_bodies(&m);
    eprintln!("\nStep 2: Rect extrude at x=10..20 (should auto-union with step 1)");
    eprintln!("  Has solid: {}", has_solid_2);
    eprintln!("  Visible bodies: {} (expected: 1 if union succeeded)", bodies_2);
    if has_solid_2 {
        let mesh = m.tessellate("ext2").unwrap();
        let vol = mesh_volume(&mesh);
        let (bb_min, bb_max) = mesh_bounding_box(&mesh);
        eprintln!("  Volume: {:.1}", vol);
        eprintln!(
            "  Bounding box: ({:.1}, {:.1}, {:.1}) → ({:.1}, {:.1}, {:.1})",
            bb_min[0], bb_min[1], bb_min[2], bb_max[0], bb_max[1], bb_max[2]
        );
    }
    eprintln!(
        "  Consumed features: {:?}",
        m.consumed_features().len()
    );

    // --- Step 3: Rect sketch at origin [20,*,*], normal [1,0,0], extrude depth=10 ---
    // From .waffle: rect ~ (-11, -20) to (10, -1) in sketch 2D
    // 3D: y ∈ [-11, 10], z ∈ [-20, -1], x ∈ [20, 30]
    m.rect_sketch(
        "sk3",
        [20., 0., 0.],
        [1., 0., 0.],
        -11.0,
        -20.0,
        21.0,
        19.0,
    )
    .unwrap();
    m.extrude("ext3", "sk3", 10.0).unwrap();

    let has_solid_3 = m.assert_has_solid("ext3").is_ok();
    let bodies_3 = count_visible_bodies(&m);
    eprintln!("\nStep 3: Rect extrude at x=20..30 (should auto-union)");
    eprintln!("  Has solid: {}", has_solid_3);
    eprintln!("  Visible bodies: {} (expected: 1 if union succeeded)", bodies_3);
    if has_solid_3 {
        let mesh = m.tessellate("ext3").unwrap();
        let vol = mesh_volume(&mesh);
        let (bb_min, bb_max) = mesh_bounding_box(&mesh);
        eprintln!("  Volume: {:.1}", vol);
        eprintln!(
            "  Bounding box: ({:.1}, {:.1}, {:.1}) → ({:.1}, {:.1}, {:.1})",
            bb_min[0], bb_min[1], bb_min[2], bb_max[0], bb_max[1], bb_max[2]
        );
    }

    // --- Step 4: Circle sketch at origin [30,*,*], normal [1,0,0], extrude depth=10 ---
    // From .waffle: circle center ~ (4.4, -6.8), radius ~ 4.6
    // 3D: center at (30, 4.4, -6.8), radius 4.6, x ∈ [30, 40]
    m.circle_sketch(
        "sk4",
        [30., 0., 0.],
        [1., 0., 0.],
        4.4,
        -6.8,
        4.6,
    )
    .unwrap();
    m.extrude("ext4", "sk4", 10.0).unwrap();

    let has_solid_4 = m.assert_has_solid("ext4").is_ok();
    let bodies_4 = count_visible_bodies(&m);
    eprintln!("\nStep 4: Circle extrude at x=30..40 (should auto-union)");
    eprintln!("  Has solid: {}", has_solid_4);
    eprintln!("  Visible bodies: {} (expected: 1 if union succeeded)", bodies_4);
    if has_solid_4 {
        let mesh = m.tessellate("ext4").unwrap();
        let vol = mesh_volume(&mesh);
        let (bb_min, bb_max) = mesh_bounding_box(&mesh);
        eprintln!("  Volume: {:.1}", vol);
        eprintln!(
            "  Bounding box: ({:.1}, {:.1}, {:.1}) → ({:.1}, {:.1}, {:.1})",
            bb_min[0], bb_min[1], bb_min[2], bb_max[0], bb_max[1], bb_max[2]
        );
    }

    // --- Step 5: Rect sketch at origin [0,*,*], normal [-1,0,0], extrude depth=100, cut=true ---
    // From .waffle: rect ~ (-11, -21) to (9, -6) in sketch 2D
    // For normal=[-1,0,0]: tangent_x = [0,-1,0], tangent_y = [0,0,1]
    // sketch_x → -Y, sketch_y → +Z
    // 3D: y ∈ [(-9)*(-1), 11*(-1)] → wait, let's be more careful:
    // 3D_y = origin_y + sketch_x * (-1) = 0 + sketch_x * (-1)
    // For sketch_x from -11 to 9: y from 11 to -9 → y ∈ [-9, 11]
    // 3D_z = origin_z + sketch_y * 1
    // For sketch_y from -21 to -6: z from -21 to -6 → z ∈ [-21, -6]
    //
    // Extrude direction: normal is [-1,0,0], so extrude goes in -X direction, depth=100
    // So the cut tool spans x ∈ [0, -100] → x ∈ [-100, 0]
    // But the bosses span x ∈ [0, 40], so the cut only overlaps at x=0.
    // Wait — the extrude direction for a cut is the plane normal direction.
    // Let me re-check: extrude uses the plane_normal as default direction.
    // normal=[-1,0,0] → extrude goes in [-1,0,0] direction, depth=100.
    // So the cut tool goes from x=0 to x=0 + (-1)*100 = x=-100.
    // That means it DOESN'T intersect the bosses at x=0..40!
    //
    // Actually wait. Looking more carefully at the rebuild code...
    // The default direction is plane_normal. But the extrude creates a solid
    // from the sketch plane in the normal direction.
    //
    // Hmm, but the user says "the cuts should subtract from that single body."
    // Looking at the .waffle file more carefully: plane_origin is [0, ...],
    // plane_normal is [-1,0,0]. Extrude depth=100. So the extrude creates
    // a tool body from x=0 going -100 in the -X direction: x ∈ [-100, 0].
    // The bosses are at x ∈ [0, 40]. So the cut tool only touches at the
    // x=0 face boundary.
    //
    // Wait — looking at this again. In the GUI, the cut with normal [-1,0,0]
    // is meant to cut THROUGH the existing bodies. The plane_origin is at
    // x=0 (the start face of the first boss), and the cut goes in the
    // normal direction [-1,0,0]. But that's AWAY from the bosses!
    //
    // Unless the extrude actually goes in the POSITIVE normal direction
    // (i.e., the cut goes from x=0 in the +X direction for depth 100).
    // Let me check the rebuild code to understand the actual direction.

    // Let me just replicate what the file says and see what happens.
    // The sketch is at x=0 with normal [-1,0,0].
    // Using approximate rect coords from the .waffle file.
    m.rect_sketch(
        "sk5",
        [0., 0., 0.],
        [-1., 0., 0.],
        -11.0,
        -21.0,
        20.0,
        15.0,
    )
    .unwrap();
    m.extrude_cut("ext5", "sk5", 100.0).unwrap();

    let has_solid_5 = m.assert_has_solid("ext5").is_ok();
    let bodies_5 = count_visible_bodies(&m);
    eprintln!("\nStep 5: Cut extrude from x=0, normal=[-1,0,0], depth=100");
    eprintln!("  Has solid: {}", has_solid_5);
    eprintln!("  Visible bodies: {} (expected: 1 if cut succeeded on single body)", bodies_5);
    if has_solid_5 {
        let mesh = m.tessellate("ext5").unwrap();
        let vol = mesh_volume(&mesh);
        let (bb_min, bb_max) = mesh_bounding_box(&mesh);
        eprintln!("  Volume: {:.1}", vol);
        eprintln!(
            "  Bounding box: ({:.1}, {:.1}, {:.1}) → ({:.1}, {:.1}, {:.1})",
            bb_min[0], bb_min[1], bb_min[2], bb_max[0], bb_max[1], bb_max[2]
        );
    }

    // --- Step 6: Same sketch, extrude depth=100, cut=true (duplicate of step 5) ---
    // The .waffle file reuses the same sketch for a second cut.
    // In the test harness, we can't reuse sketch names, so create a new sketch
    // with the same geometry.
    m.rect_sketch(
        "sk6",
        [0., 0., 0.],
        [-1., 0., 0.],
        -11.0,
        -21.0,
        20.0,
        15.0,
    )
    .unwrap();
    m.extrude_cut("ext6", "sk6", 100.0).unwrap();

    let has_solid_6 = m.assert_has_solid("ext6").is_ok();
    let bodies_6 = count_visible_bodies(&m);
    eprintln!("\nStep 6: Duplicate cut (same sketch as step 5)");
    eprintln!("  Has solid: {}", has_solid_6);
    eprintln!("  Visible bodies: {}", bodies_6);
    if has_solid_6 {
        let mesh = m.tessellate("ext6").unwrap();
        let vol = mesh_volume(&mesh);
        let (bb_min, bb_max) = mesh_bounding_box(&mesh);
        eprintln!("  Volume: {:.1}", vol);
        eprintln!(
            "  Bounding box: ({:.1}, {:.1}, {:.1}) → ({:.1}, {:.1}, {:.1})",
            bb_min[0], bb_min[1], bb_min[2], bb_max[0], bb_max[1], bb_max[2]
        );
    }

    print_diagnostics(&m, "FINAL STATE: several-extrudes");

    // Summary
    eprintln!("\n╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║  SUMMARY: several-extrudes                                  ║");
    eprintln!("╠══════════════════════════════════════════════════════════════╣");
    eprintln!(
        "║  Step 1 (rect boss):   solid={:<5} bodies={}                  ║",
        has_solid_1, bodies_1
    );
    eprintln!(
        "║  Step 2 (rect boss):   solid={:<5} bodies={}                  ║",
        has_solid_2, bodies_2
    );
    eprintln!(
        "║  Step 3 (rect boss):   solid={:<5} bodies={}                  ║",
        has_solid_3, bodies_3
    );
    eprintln!(
        "║  Step 4 (circle boss): solid={:<5} bodies={}                  ║",
        has_solid_4, bodies_4
    );
    eprintln!(
        "║  Step 5 (rect cut):    solid={:<5} bodies={}                  ║",
        has_solid_5, bodies_5
    );
    eprintln!(
        "║  Step 6 (dup cut):     solid={:<5} bodies={}                  ║",
        has_solid_6, bodies_6
    );
    eprintln!("╚══════════════════════════════════════════════════════════════╝");
}

// ═══════════════════════════════════════════════════════════════════════════
// Additional Test: "Several extrudes" with overlapping rectangles
// ═══════════════════════════════════════════════════════════════════════════
//
// The .waffle file sketches are very large (spanning ~27 units) and the
// extrude is only 10 deep. The bosses at x=0, x=10, x=20 overlap
// significantly in YZ. Let's test whether auto-union succeeds for
// abutting boxes (sharing a face at x=10, x=20, x=30).

#[test]
fn several_extrudes_simplified_abutting() {
    eprintln!("\n╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║  SIMPLIFIED: Abutting boxes on X axis                       ║");
    eprintln!("╚══════════════════════════════════════════════════════════════╝");

    let mut m = ModelBuilder::truck();

    // Four abutting 10x20x20 boxes along X axis: [0,10], [10,20], [20,30], [30,40]
    // All sketches on YZ plane (normal [1,0,0])
    // sketch_x → +Y, sketch_y → +Z

    // Box 1: x ∈ [0, 10]
    m.rect_sketch("s1", [0., 0., 0.], [1., 0., 0.], -10., -10., 20., 20.)
        .unwrap();
    m.extrude("e1", "s1", 10.0).unwrap();
    let b1 = count_visible_bodies(&m);
    let v1 = mesh_volume(&m.tessellate("e1").unwrap());
    eprintln!("Box 1 (x=0..10):  bodies={}, vol={:.0}", b1, v1);

    // Box 2: x ∈ [10, 20] — shares face at x=10 with box 1
    m.rect_sketch("s2", [10., 0., 0.], [1., 0., 0.], -10., -10., 20., 20.)
        .unwrap();
    m.extrude("e2", "s2", 10.0).unwrap();
    let b2 = count_visible_bodies(&m);
    eprintln!("Box 2 (x=10..20): bodies={} (1=union ok, 2=no union)", b2);
    if m.assert_has_solid("e2").is_ok() {
        let v2 = mesh_volume(&m.tessellate("e2").unwrap());
        eprintln!("  Volume of latest solid: {:.0} (expected: 8000 if unioned)", v2);
    }

    // Box 3: x ∈ [20, 30] — shares face at x=20
    m.rect_sketch("s3", [20., 0., 0.], [1., 0., 0.], -10., -10., 20., 20.)
        .unwrap();
    m.extrude("e3", "s3", 10.0).unwrap();
    let b3 = count_visible_bodies(&m);
    eprintln!("Box 3 (x=20..30): bodies={} (1=union ok)", b3);
    if m.assert_has_solid("e3").is_ok() {
        let v3 = mesh_volume(&m.tessellate("e3").unwrap());
        eprintln!("  Volume of latest solid: {:.0} (expected: 12000 if unioned)", v3);
    }

    // Cylinder 4: circle at x=30, r=5, depth 10
    m.circle_sketch("s4", [30., 0., 0.], [1., 0., 0.], 0., 0., 5.)
        .unwrap();
    m.extrude("e4", "s4", 10.0).unwrap();
    let b4 = count_visible_bodies(&m);
    eprintln!("Cyl 4 (x=30..40): bodies={} (1=union ok)", b4);

    // Cut: from x=0, normal [-1,0,0], rect 15x15, depth 100
    // This should create a cut tool spanning x ∈ [-100, 0] or x ∈ [0, 100]
    // depending on extrude direction logic
    m.rect_sketch("sc1", [0., 0., 0.], [-1., 0., 0.], -7.5, -7.5, 15., 15.)
        .unwrap();
    m.extrude_cut("ec1", "sc1", 100.0).unwrap();
    let bc1 = count_visible_bodies(&m);
    eprintln!("Cut 1 (from x=0): bodies={}", bc1);
    if m.assert_has_solid("ec1").is_ok() {
        let vc1 = mesh_volume(&m.tessellate("ec1").unwrap());
        eprintln!("  Volume after cut: {:.0}", vc1);
    }

    print_diagnostics(&m, "SIMPLIFIED: abutting boxes");
}

// ═══════════════════════════════════════════════════════════════════════════
// Test: "Several extrudes" with OVERLAPPING rectangles (not just abutting)
// ═══════════════════════════════════════════════════════════════════════════
//
// The actual .waffle file has sketches with different sizes that overlap
// in the YZ plane. Let's test with overlapping rectangles.

#[test]
fn several_extrudes_overlapping() {
    eprintln!("\n╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║  OVERLAPPING: Boxes that share volume, not just faces       ║");
    eprintln!("╚══════════════════════════════════════════════════════════════╝");

    let mut m = ModelBuilder::truck();

    // Box 1: sketch at x=0, 20x20 rect, extrude 10 → x ∈ [0,10]
    m.rect_sketch("s1", [0., 0., 0.], [1., 0., 0.], -10., -10., 20., 20.)
        .unwrap();
    m.extrude("e1", "s1", 10.0).unwrap();
    let b1 = count_visible_bodies(&m);
    eprintln!("Box 1 (x=0..10, y=-10..10, z=-10..10): bodies={}", b1);

    // Box 2: sketch at x=8, 15x15 rect, extrude 10 → x ∈ [8,18]
    // Overlaps with box 1 in region x ∈ [8,10]
    m.rect_sketch("s2", [8., 0., 0.], [1., 0., 0.], -7.5, -7.5, 15., 15.)
        .unwrap();
    m.extrude("e2", "s2", 10.0).unwrap();
    let b2 = count_visible_bodies(&m);
    eprintln!("Box 2 (x=8..18, overlap at x=8..10): bodies={} (1=union ok)", b2);

    // Box 3: sketch at x=16, 18x18 rect, extrude 10 → x ∈ [16,26]
    // Overlaps with box 2 in region x ∈ [16,18]
    m.rect_sketch("s3", [16., 0., 0.], [1., 0., 0.], -9., -9., 18., 18.)
        .unwrap();
    m.extrude("e3", "s3", 10.0).unwrap();
    let b3 = count_visible_bodies(&m);
    eprintln!("Box 3 (x=16..26, overlap at x=16..18): bodies={} (1=union ok)", b3);

    // Circle 4: sketch at x=24, r=5, extrude 10 → x ∈ [24,34]
    // Overlaps with box 3 in region x ∈ [24,26]
    m.circle_sketch("s4", [24., 0., 0.], [1., 0., 0.], 0., 0., 5.)
        .unwrap();
    m.extrude("e4", "s4", 10.0).unwrap();
    let b4 = count_visible_bodies(&m);
    eprintln!("Cyl 4 (x=24..34, overlap at x=24..26): bodies={} (1=union ok)", b4);

    // Now try a cut through the whole thing
    // Cut from x=0, spanning from x=0 deep into the body
    // Use extrude_directed to control direction explicitly
    m.rect_sketch("sc1", [0., 0., 0.], [1., 0., 0.], -5., -5., 10., 10.)
        .unwrap();
    m.extrude_directed("ec1", "sc1", 100.0, [1., 0., 0.], true)
        .unwrap();
    let bc1 = count_visible_bodies(&m);
    eprintln!("Cut (x=0..100, 10x10 rect): bodies={}", bc1);
    if m.assert_has_solid("ec1").is_ok() {
        let vc1 = mesh_volume(&m.tessellate("ec1").unwrap());
        eprintln!("  Volume after cut: {:.0}", vc1);
    }

    print_diagnostics(&m, "OVERLAPPING: several extrudes");
}

// ═══════════════════════════════════════════════════════════════════════════
// Test Case 2: "Multi cut" (multi-cut.waffle)
// ═══════════════════════════════════════════════════════════════════════════
//
// Feature sequence:
//   1. Sketch (rect) at origin [0,0,0], normal [1,0,0] → Extrude depth=10
//   2. Sketch (circle r=24.3) at origin [10,*,*], normal [1,0,0] → Extrude depth=10
//   3. Sketch (circle r=18.1) at origin [20,*,*], normal [1,0,0] → Extrude depth=20, CUT
//
// The rect is ~34x33 and the circles are very large (r=24 and r=18).
// The first two bosses should auto-union. The cut should subtract from both.
//
// User report: "Two extruded bosses are fine, but when executing a cut they
// become two bodies, and what should be a through cut only affects the body
// from the second extruded boss operation."

#[test]
fn multi_cut_replay() {
    eprintln!("\n╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║  TEST CASE 2: multi-cut.waffle                              ║");
    eprintln!("╚══════════════════════════════════════════════════════════════╝");

    let mut m = ModelBuilder::truck();

    // --- Step 1: Rect at origin [0,0,0], normal [1,0,0] ---
    // From .waffle: corners ~ (-17, -16) to (17, 17) in sketch 2D
    // 3D: y ∈ [-17, 17], z ∈ [-16, 17], x ∈ [0, 10]
    m.rect_sketch(
        "sk1",
        [0., 0., 0.],
        [1., 0., 0.],
        -17.0,
        -16.5,
        34.0,
        33.0,
    )
    .unwrap();
    m.extrude("ext1", "sk1", 10.0).unwrap();

    let has_solid_1 = m.assert_has_solid("ext1").is_ok();
    let bodies_1 = count_visible_bodies(&m);
    eprintln!("\nStep 1: Large rect extrude at x=0..10");
    eprintln!("  Has solid: {}", has_solid_1);
    eprintln!("  Visible bodies: {}", bodies_1);
    if has_solid_1 {
        let mesh = m.tessellate("ext1").unwrap();
        let vol = mesh_volume(&mesh);
        let (bb_min, bb_max) = mesh_bounding_box(&mesh);
        eprintln!("  Volume: {:.0}", vol);
        eprintln!(
            "  Bounding box: ({:.1}, {:.1}, {:.1}) → ({:.1}, {:.1}, {:.1})",
            bb_min[0], bb_min[1], bb_min[2], bb_max[0], bb_max[1], bb_max[2]
        );
    }

    // --- Step 2: Circle at origin [10,*,*], normal [1,0,0], r≈24 ---
    // From .waffle: center (14.8, 0.6), radius 24.3
    // 3D center: (10, 14.8, 0.6), extrude x ∈ [10, 20]
    m.circle_sketch(
        "sk2",
        [10., 0., 0.],
        [1., 0., 0.],
        14.8,
        0.6,
        24.3,
    )
    .unwrap();
    m.extrude("ext2", "sk2", 10.0).unwrap();

    let has_solid_2 = m.assert_has_solid("ext2").is_ok();
    let bodies_2 = count_visible_bodies(&m);
    eprintln!("\nStep 2: Large circle extrude at x=10..20 (should auto-union)");
    eprintln!("  Has solid: {}", has_solid_2);
    eprintln!("  Visible bodies: {} (expected: 1 if union succeeded)", bodies_2);
    if has_solid_2 {
        let mesh = m.tessellate("ext2").unwrap();
        let vol = mesh_volume(&mesh);
        let (bb_min, bb_max) = mesh_bounding_box(&mesh);
        eprintln!("  Volume: {:.0}", vol);
        eprintln!(
            "  Bounding box: ({:.1}, {:.1}, {:.1}) → ({:.1}, {:.1}, {:.1})",
            bb_min[0], bb_min[1], bb_min[2], bb_max[0], bb_max[1], bb_max[2]
        );
    }

    // --- Step 3: Circle at origin [20,*,*], normal [1,0,0], r≈18, CUT, depth=20 ---
    // From .waffle: center (19.8, -4.2), radius 18.1
    // 3D center: (20, 19.8, -4.2), extrude cut x ∈ [20, 40]
    // depth=20 means the cut tool spans 20 units from x=20
    m.circle_sketch(
        "sk3",
        [20., 0., 0.],
        [1., 0., 0.],
        19.8,
        -4.2,
        18.1,
    )
    .unwrap();
    m.extrude_cut("ext3", "sk3", 20.0).unwrap();

    let has_solid_3 = m.assert_has_solid("ext3").is_ok();
    let bodies_3 = count_visible_bodies(&m);
    eprintln!("\nStep 3: Large circle CUT at x=20..40 (should cut through unified body)");
    eprintln!("  Has solid: {}", has_solid_3);
    eprintln!("  Visible bodies: {} (expected: 1 if cut succeeded on single body)", bodies_3);
    if has_solid_3 {
        let mesh = m.tessellate("ext3").unwrap();
        let vol = mesh_volume(&mesh);
        let (bb_min, bb_max) = mesh_bounding_box(&mesh);
        eprintln!("  Volume: {:.0}", vol);
        eprintln!(
            "  Bounding box: ({:.1}, {:.1}, {:.1}) → ({:.1}, {:.1}, {:.1})",
            bb_min[0], bb_min[1], bb_min[2], bb_max[0], bb_max[1], bb_max[2]
        );
    }

    print_diagnostics(&m, "FINAL STATE: multi-cut");

    // Summary
    eprintln!("\n╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║  SUMMARY: multi-cut                                         ║");
    eprintln!("╠══════════════════════════════════════════════════════════════╣");
    eprintln!(
        "║  Step 1 (rect boss):   solid={:<5} bodies={}                  ║",
        has_solid_1, bodies_1
    );
    eprintln!(
        "║  Step 2 (circle boss): solid={:<5} bodies={}                  ║",
        has_solid_2, bodies_2
    );
    eprintln!(
        "║  Step 3 (circle cut):  solid={:<5} bodies={}                  ║",
        has_solid_3, bodies_3
    );
    eprintln!("╚══════════════════════════════════════════════════════════════╝");
}

// ═══════════════════════════════════════════════════════════════════════════
// Additional Test: Multi-cut simplified (smaller geometry, more controlled)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn multi_cut_simplified() {
    eprintln!("\n╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║  SIMPLIFIED: multi-cut with smaller geometry                ║");
    eprintln!("╚══════════════════════════════════════════════════════════════╝");

    let mut m = ModelBuilder::truck();

    // Box: 10x10x10 at x=0..10 (sketch on YZ plane)
    m.rect_sketch("s1", [0., 0., 0.], [1., 0., 0.], -5., -5., 10., 10.)
        .unwrap();
    m.extrude("e1", "s1", 10.0).unwrap();
    let b1 = count_visible_bodies(&m);
    let v1 = mesh_volume(&m.tessellate("e1").unwrap());
    eprintln!("Step 1: Box 10x10x10, x=0..10: bodies={}, vol={:.0}", b1, v1);

    // Cylinder: r=8 at x=10..20 (overlapping circle, center at sketch (0,0))
    m.circle_sketch("s2", [10., 0., 0.], [1., 0., 0.], 0., 0., 8.)
        .unwrap();
    m.extrude("e2", "s2", 10.0).unwrap();
    let b2 = count_visible_bodies(&m);
    eprintln!("Step 2: Cylinder r=8, x=10..20: bodies={} (1=union ok)", b2);
    if m.assert_has_solid("e2").is_ok() {
        let v2 = mesh_volume(&m.tessellate("e2").unwrap());
        eprintln!("  Volume: {:.0}", v2);
    }

    // Cut: circle r=6 at x=20, depth=20 (cuts through cylinder and possibly box)
    m.circle_sketch("s3", [20., 0., 0.], [1., 0., 0.], 0., 0., 6.)
        .unwrap();
    m.extrude_cut("e3", "s3", 20.0).unwrap();
    let b3 = count_visible_bodies(&m);
    eprintln!("Step 3: Circle CUT r=6, depth=20: bodies={}", b3);
    if m.assert_has_solid("e3").is_ok() {
        let v3 = mesh_volume(&m.tessellate("e3").unwrap());
        eprintln!("  Volume: {:.0}", v3);
    }

    print_diagnostics(&m, "SIMPLIFIED: multi-cut");
}

// ═══════════════════════════════════════════════════════════════════════════
// Test: Load actual .waffle files directly via the load() API
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn load_several_extrudes_waffle() {
    eprintln!("\n╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║  LOAD: several-extrudes.waffle (actual file)                ║");
    eprintln!("╚══════════════════════════════════════════════════════════════╝");

    let json =
        std::fs::read_to_string("../../app/tests/cases/several-extrudes.waffle")
            .expect("Failed to read several-extrudes.waffle");

    let mut m = ModelBuilder::truck();
    match m.load(&json) {
        Ok(_) => {
            eprintln!("  Load succeeded!");
            print_diagnostics(&m, "LOADED: several-extrudes");

            // Collect feature info first to avoid borrow issues
            let feature_info: Vec<_> = m
                .state
                .engine
                .tree
                .features
                .iter()
                .filter(|f| f.name.contains("Extrude") || f.name == "Extrude")
                .map(|f| {
                    let result = m.state.engine.get_result(f.id);
                    let handles: Vec<_> = result
                        .map(|r| {
                            r.outputs
                                .iter()
                                .map(|(key, body)| (format!("{:?}", key), body.handle.clone()))
                                .collect()
                        })
                        .unwrap_or_default();
                    let n_outputs = result.map(|r| r.outputs.len()).unwrap_or(0);
                    (f.name.clone(), f.id.to_string()[..8].to_string(), n_outputs, handles)
                })
                .collect();

            for (name, id_prefix, n_outputs, handles) in &feature_info {
                eprintln!(
                    "\n  Tessellating '{}' ({}) — {} output(s):",
                    name, id_prefix, n_outputs,
                );
                for (i, (key_str, handle)) in handles.iter().enumerate() {
                    eprintln!("    Output {}: key={}", i, key_str);
                    match m.kernel_mut().tessellate(handle, 0.1) {
                        Ok(mesh) => {
                            let vol = mesh_volume(&mesh);
                            let (bb_min, bb_max) = mesh_bounding_box(&mesh);
                            eprintln!("      Volume: {:.0}", vol);
                            eprintln!(
                                "      BB: ({:.1},{:.1},{:.1}) → ({:.1},{:.1},{:.1})",
                                bb_min[0], bb_min[1], bb_min[2],
                                bb_max[0], bb_max[1], bb_max[2]
                            );
                        }
                        Err(e) => {
                            eprintln!("      Tessellation failed: {}", e);
                        }
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("  Load FAILED: {}", e);
        }
    }
}

#[test]
fn load_multi_cut_waffle() {
    eprintln!("\n╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║  LOAD: multi-cut.waffle (actual file)                       ║");
    eprintln!("╚══════════════════════════════════════════════════════════════╝");

    let json =
        std::fs::read_to_string("../../app/tests/cases/multi-cut.waffle")
            .expect("Failed to read multi-cut.waffle");

    let mut m = ModelBuilder::truck();
    match m.load(&json) {
        Ok(_) => {
            eprintln!("  Load succeeded!");
            print_diagnostics(&m, "LOADED: multi-cut");

            // Collect feature info first to avoid borrow issues
            let feature_info: Vec<_> = m
                .state
                .engine
                .tree
                .features
                .iter()
                .filter(|f| f.name.contains("Extrude") || f.name == "Extrude")
                .map(|f| {
                    let result = m.state.engine.get_result(f.id);
                    let handles: Vec<_> = result
                        .map(|r| {
                            r.outputs
                                .iter()
                                .map(|(key, body)| (format!("{:?}", key), body.handle.clone()))
                                .collect()
                        })
                        .unwrap_or_default();
                    let n_outputs = result.map(|r| r.outputs.len()).unwrap_or(0);
                    (f.name.clone(), f.id.to_string()[..8].to_string(), n_outputs, handles)
                })
                .collect();

            for (name, id_prefix, n_outputs, handles) in &feature_info {
                eprintln!(
                    "\n  Tessellating '{}' ({}) — {} output(s):",
                    name, id_prefix, n_outputs,
                );
                for (i, (key_str, handle)) in handles.iter().enumerate() {
                    eprintln!("    Output {}: key={}", i, key_str);
                    match m.kernel_mut().tessellate(handle, 0.1) {
                        Ok(mesh) => {
                            let vol = mesh_volume(&mesh);
                            let (bb_min, bb_max) = mesh_bounding_box(&mesh);
                            eprintln!("      Volume: {:.0}", vol);
                            eprintln!(
                                "      BB: ({:.1},{:.1},{:.1}) → ({:.1},{:.1},{:.1})",
                                bb_min[0], bb_min[1], bb_min[2],
                                bb_max[0], bb_max[1], bb_max[2]
                            );
                        }
                        Err(e) => {
                            eprintln!("      Tessellation failed: {}", e);
                        }
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("  Load FAILED: {}", e);
        }
    }
}
