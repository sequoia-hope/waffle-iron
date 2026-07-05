//! Regression tests for full-depth circular cut "no Z overlap" bug.
//!
//! These tests exercise the full feature-engine pipeline (sketch → extrude → cut)
//! using ModelBuilder::kernel_v2() to reproduce the exact failure path a GUI user hits.
//!
//! See: crates/kernel/src/boolean.rs:1057-1060 — `cyl_cyl_boolean()` "no Z overlap" check.

use test_harness::workflow::ModelBuilder;

/// ZR1: Circle boss + circle cut, full depth, both at origin with default direction.
///
/// This is the simplest reproduction: outer circle extruded up, inner circle sketch
/// placed at z=20 (top face), extrude_cut with depth=20.
///
/// extrude_cut sends `direction: None`, which triggers auto-reversal in rebuild.rs.
/// If reversal works correctly, the cut cylinder goes downward from z=20 to z=0,
/// and the boolean succeeds. If it fails, we get "no Z overlap".
#[test]
#[ignore = "kernel-v2: coplanar input face pair, NotSupported until Yang Stage 0 (roadmap M8)"]
fn zr1_circle_boss_circle_cut_full_depth() {
    let mut m = ModelBuilder::kernel_v2();

    // Step 1: Circle r=5 at origin, extrude up 20
    m.true_circle_sketch("boss_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude("boss", "boss_sk", 20.0).unwrap();

    // Step 2: Circle r=2 at z=20 (top face), cut full depth 20
    m.true_circle_sketch("cut_sk", [0., 0., 20.], [0., 0., 1.], 0., 0., 2.)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 20.0).unwrap();

    // If we get here without error, the pipeline handled it.
    // Check for engine errors (the feature-engine stores errors rather than returning Err).
    let has_errors = m.assert_no_errors();
    assert!(
        has_errors.is_ok(),
        "ZR1: Full-depth circle cut should succeed without errors: {:?}",
        has_errors.err()
    );
}

/// ZR2: Circle boss + circle cut from top face with explicit downward direction.
///
/// Uses extrude_directed with direction=[0,0,-1] and cut=true to bypass
/// the auto-reversal logic and explicitly cut downward from z=20.
#[test]
#[ignore = "kernel-v2: coplanar input face pair, NotSupported until Yang Stage 0 (roadmap M8)"]
fn zr2_circle_boss_cut_from_top_face_explicit_down() {
    let mut m = ModelBuilder::kernel_v2();

    m.true_circle_sketch("boss_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude("boss", "boss_sk", 20.0).unwrap();

    m.true_circle_sketch("cut_sk", [0., 0., 20.], [0., 0., 1.], 0., 0., 2.)
        .unwrap();
    // Explicit direction [0,0,-1] — cuts downward from z=20. Should work.
    m.extrude_directed("hole", "cut_sk", 20.0, [0.0, 0.0, -1.0], true)
        .unwrap();

    let has_errors = m.assert_no_errors();
    assert!(
        has_errors.is_ok(),
        "ZR2: Explicit downward cut should succeed: {:?}",
        has_errors.err()
    );
}

/// ZR3: Circle boss + polygon (rect) cut at z=20, full depth.
///
/// Uses a rectangle cut instead of circle cut (the cyl-minus-box boolean
/// path). This test originally documented cyl-minus-box as UNSUPPORTED; the
/// capability landed with the KV5b/TH2 cylinder work (the F0036–F0040
/// corpus family is pinned SUPPORTED_CORRECT), so the expectation flips:
/// the cut must now SUCCEED with the exact through-hole volume. Stale-
/// expectation reconciliation — the file lagged the capability.
#[test]
fn zr3_circle_boss_polygon_cut() {
    let mut m = ModelBuilder::kernel_v2();

    m.true_circle_sketch("boss_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude("boss", "boss_sk", 20.0).unwrap();

    // Rectangle cut at z=20, full depth — square through-hole.
    m.rect_sketch("cut_sk", [0., 0., 20.], [0., 0., 1.], -2., -2., 4., 4.)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 20.0).unwrap();

    m.assert_no_errors()
        .expect("ZR3: cylinder minus box is supported (KV5b/TH2)");
    let mesh = m.tessellate_last().expect("ZR3: tessellates");
    let vol = test_harness::helpers::mesh_signed_volume(&mesh);
    let expected = std::f64::consts::PI * 25.0 * 20.0 - 4.0 * 4.0 * 20.0;
    assert!(
        (vol - expected).abs() / expected < 0.05,
        "ZR3: through-hole volume {vol} (expected ≈ {expected})"
    );
}

/// ZR4: Rect boss + circle cut at z=20, full depth.
///
/// Tests box_cyl_boolean path (polygon boss, circle tool).
#[test]
#[ignore = "kernel-v2: coplanar input face pair, NotSupported until Yang Stage 0 (roadmap M8)"]
fn zr4_polygon_boss_circle_cut() {
    let mut m = ModelBuilder::kernel_v2();

    m.rect_sketch("boss_sk", [0., 0., 0.], [0., 0., 1.], -5., -5., 10., 10.)
        .unwrap();
    m.extrude("boss", "boss_sk", 20.0).unwrap();

    m.true_circle_sketch("cut_sk", [0., 0., 20.], [0., 0., 1.], 0., 0., 2.)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 20.0).unwrap();

    let has_errors = m.assert_no_errors();
    assert!(
        has_errors.is_ok(),
        "ZR4: Circle cut on polygon boss should succeed: {:?}",
        has_errors.err()
    );
}

/// ZR5: Circle boss + circle cut with direction: None (auto-reversal).
///
/// This is the MOST LIKELY test to reproduce the GUI bug.
/// The GUI sends `direction: None` for cuts, relying on rebuild.rs to
/// auto-reverse the direction. If auto-reversal fails or produces wrong
/// parameters, the kernel gets a tool cylinder that doesn't overlap the boss.
///
/// Uses extrude_cut (which sends direction: None) with sketch at z=20.
#[test]
#[ignore = "kernel-v2: coplanar input face pair, NotSupported until Yang Stage 0 (roadmap M8)"]
fn zr5_boss_cut_with_direction_none() {
    let mut m = ModelBuilder::kernel_v2();

    // Boss: circle r=5, extruded 20 upward from z=0
    m.true_circle_sketch("boss_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude("boss", "boss_sk", 20.0).unwrap();
    m.assert_has_solid("boss").unwrap();

    // Cut: circle r=2, sketch at z=20 (top face), direction=None, depth=20
    m.true_circle_sketch("cut_sk", [0., 0., 20.], [0., 0., 1.], 0., 0., 2.)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 20.0).unwrap();

    // This is the critical assertion: does the pipeline produce errors?
    // If "no Z overlap" occurs, the error will be in state.engine.errors.
    let errors_result = m.assert_no_errors();
    if errors_result.is_err() {
        let err_detail = format!("{:?}", errors_result.err().unwrap());
        // If this fires, the bug is confirmed in the feature-engine pipeline
        panic!(
            "ZR5: CONFIRMED BUG — full-depth circle cut with direction:None \
             produced errors (likely 'no Z overlap'): {}",
            err_detail
        );
    }
}

/// ZR6: Half-depth circle cut (baseline — should always work).
///
/// Boss: r=5, h=20. Cut: r=2 at z=20, depth=10.
/// The cut only goes halfway, so even if direction is wrong, there's
/// more room for Z overlap. Acts as a control test.
#[test]
#[ignore = "kernel-v2: coplanar input face pair, NotSupported until Yang Stage 0 (roadmap M8)"]
fn zr6_half_depth_circle_cut() {
    let mut m = ModelBuilder::kernel_v2();

    m.true_circle_sketch("boss_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 5.)
        .unwrap();
    m.extrude("boss", "boss_sk", 20.0).unwrap();

    m.true_circle_sketch("cut_sk", [0., 0., 20.], [0., 0., 1.], 0., 0., 2.)
        .unwrap();
    m.extrude_cut("hole", "cut_sk", 10.0).unwrap();

    let has_errors = m.assert_no_errors();
    assert!(
        has_errors.is_ok(),
        "ZR6: Half-depth circle cut should succeed: {:?}",
        has_errors.err()
    );
}

/// ZR7: Circle boss + circle cut on YZ plane (normal=[1,0,0]).
///
/// Same geometry as ZR1 but rotated to the YZ plane.
/// Catches axis assumptions in cyl_z_range() which only uses the Z component.
/// When the extrude direction is along X (not Z), cyl_z_range computes zero
/// Z extent, causing a spurious "no Z overlap" error.
#[test]
#[ignore = "kernel-v2: coplanar input face pair, NotSupported until Yang Stage 0 (roadmap M8)"]
fn zr7_circle_boss_circle_cut_yz_plane() {
    let mut m = ModelBuilder::kernel_v2();

    // Boss: circle r=5 on YZ plane at origin, extrude along X by 20
    m.true_circle_sketch("boss_sk", [0., 0., 0.], [1., 0., 0.], 0., 0., 5.)
        .unwrap();
    m.extrude_directed("boss", "boss_sk", 20.0, [1., 0., 0.], false)
        .unwrap();

    // Cut: circle r=2 on YZ plane at x=20 (front face), cut back along -X
    m.true_circle_sketch("cut_sk", [20., 0., 0.], [1., 0., 0.], 0., 0., 2.)
        .unwrap();
    m.extrude_directed("hole", "cut_sk", 20.0, [-1., 0., 0.], true)
        .unwrap();

    // Fixed: frame rotation in cyl_cyl_boolean rotates X-axis cylinders to Z-aligned
    // frame before processing, then rotates result back.
    let errors_result = m.assert_no_errors();
    assert!(
        errors_result.is_ok(),
        "ZR7: YZ-plane circle cut should succeed without errors: {:?}",
        errors_result.err()
    );
}

/// ZR8: Circle boss + circle cut on XZ plane (normal=[0,1,0]).
///
/// Same as ZR7 but on the XZ plane (extrude along Y).
#[test]
#[ignore = "kernel-v2: coplanar input face pair, NotSupported until Yang Stage 0 (roadmap M8)"]
fn zr8_circle_boss_circle_cut_xz_plane() {
    let mut m = ModelBuilder::kernel_v2();

    // Boss: circle r=5 on XZ plane at origin, extrude along Y by 20
    m.true_circle_sketch("boss_sk", [0., 0., 0.], [0., 1., 0.], 0., 0., 5.)
        .unwrap();
    m.extrude_directed("boss", "boss_sk", 20.0, [0., 1., 0.], false)
        .unwrap();

    // Cut: circle r=2 on XZ plane at y=20, cut back along -Y
    m.true_circle_sketch("cut_sk", [0., 20., 0.], [0., 1., 0.], 0., 0., 2.)
        .unwrap();
    m.extrude_directed("hole", "cut_sk", 20.0, [0., -1., 0.], true)
        .unwrap();

    let errors_result = m.assert_no_errors();
    assert!(
        errors_result.is_ok(),
        "ZR8: XZ-plane circle cut should succeed without errors: {:?}",
        errors_result.err()
    );
}

/// ZR10: Non-coaxial circle cut on XY plane (Z-aligned, no frame rotation needed).
///
/// Same offset geometry as ZR9 but on the default XY plane. If this passes
/// but ZR9 fails, the bug is in frame rotation. If both fail, the bug is in
/// build_partial_cyl_cyl or partial cylinder tessellation.
#[test]
#[ignore = "kernel-v2: coplanar input face pair, NotSupported until Yang Stage 0 (roadmap M8)"]
fn zr10_non_coaxial_circle_cut_xy_plane() {
    let mut m = ModelBuilder::kernel_v2();

    // Boss: circle r≈14mm on XY plane (default)
    m.true_circle_sketch(
        "boss_sk",
        [0., 0., 0.],
        [0., 0., 1.],
        -0.0004819277108433728,
        0.0004578313253011935,
        0.014081714123876037,
    )
    .unwrap();
    m.extrude("boss", "boss_sk", 0.01).unwrap();

    // Cut: circle r≈8.6mm, off-center — NOT coaxial with boss
    m.true_circle_sketch(
        "cut_sk",
        [0.000917914459326615, -0.008883279146781812, 0.01],
        [0., 0., 1.],
        -0.0018912733441652384,
        -0.009989036219065389,
        0.008591190998971142,
    )
    .unwrap();
    m.extrude_cut("hole", "cut_sk", 0.01).unwrap();

    m.assert_no_errors().unwrap();
    m.assert_has_solid("hole").unwrap();

    let mesh = m
        .tessellate("hole")
        .expect("ZR10: tessellation should succeed");

    let n_verts = mesh.vertices.len() / 3;
    let n_tris = mesh.indices.len() / 3;
    let n_faces = mesh.face_ranges.len();
    eprintln!(
        "ZR10 mesh: {} vertices, {} triangles, {} faces",
        n_verts, n_tris, n_faces
    );

    assert!(!mesh.vertices.is_empty(), "ZR10: mesh has no vertices");
    assert!(n_faces >= 4, "ZR10: expected >= 4 faces, got {}", n_faces);

    // Check for degenerate triangles
    let mut degenerate = 0u32;
    for tri in 0..n_tris {
        let i0 = mesh.indices[tri * 3] as usize;
        let i1 = mesh.indices[tri * 3 + 1] as usize;
        let i2 = mesh.indices[tri * 3 + 2] as usize;
        let v0 = [
            mesh.vertices[i0 * 3] as f64,
            mesh.vertices[i0 * 3 + 1] as f64,
            mesh.vertices[i0 * 3 + 2] as f64,
        ];
        let v1 = [
            mesh.vertices[i1 * 3] as f64,
            mesh.vertices[i1 * 3 + 1] as f64,
            mesh.vertices[i1 * 3 + 2] as f64,
        ];
        let v2 = [
            mesh.vertices[i2 * 3] as f64,
            mesh.vertices[i2 * 3 + 1] as f64,
            mesh.vertices[i2 * 3 + 2] as f64,
        ];
        let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
        let cross = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let area = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt() * 0.5;
        if area < 1e-18 {
            degenerate += 1;
        }
    }
    eprintln!(
        "ZR10: {} degenerate triangles out of {}",
        degenerate, n_tris
    );
    assert!(
        (degenerate as usize) < n_tris / 2,
        "ZR10: {} of {} triangles are degenerate — mesh is invisible",
        degenerate,
        n_tris
    );
}

/// ZR9: Reproduce circle-cut-nobody.waffle — non-coaxial circle cut on YZ plane.
///
/// The cut circle is NOT coaxial with the boss (offset in sketch UV space).
/// This triggers `build_partial_cyl_cyl` (non-concentric path) which creates
/// arc-bounded faces. Combined with frame rotation (YZ → Z-aligned → back),
/// the arc angles and tessellation circle axes become mismatched, causing
/// all tessellated vertices to collapse to the same point → degenerate
/// triangles → invisible body ("no body" in the GUI).
///
/// Root cause: `build_partial_cyl_cyl` computes arc geometry in the Z-aligned
/// frame, `rotate_boolean_result` rotates it back, but `tessellate_cylindrical_patch`
/// derives its own circle axes via `make_circle_axes(rotated_axis)` which produces
/// a different frame than the one the arc angles were computed in.
///
/// ZR10 (same geometry on XY plane, no rotation) passes — proving the bug
/// is specifically in the frame rotation + partial cylinder tessellation interaction.
#[test]
#[ignore = "kernel-v2: coplanar input face pair, NotSupported until Yang Stage 0 (roadmap M8)"]
fn zr9_circle_cut_nobody_yz_plane_non_coaxial() {
    let mut m = ModelBuilder::kernel_v2();

    // Boss: circle r≈14.08mm, slightly off-center, on YZ plane
    m.true_circle_sketch(
        "boss_sk",
        [0., 0., 0.],
        [1., 0., 0.],
        -0.0004819277108433728,
        0.0004578313253011935,
        0.014081714123876037,
    )
    .unwrap();
    m.extrude("boss", "boss_sk", 0.01).unwrap();

    // Cut: circle r≈8.59mm, center offset in UV — NOT coaxial with boss
    m.true_circle_sketch(
        "cut_sk",
        [
            0.009999999776482582,
            0.008883279146781812,
            0.000917914459326615,
        ],
        [1., 0., 0.],
        -0.009989036219065389,
        0.0018912733441652384,
        0.008591190998971142,
    )
    .unwrap();
    m.extrude_cut("hole", "cut_sk", 0.01).unwrap();

    // Boolean succeeds (frame rotation fix handles the "no Z overlap" error)
    m.assert_no_errors().unwrap();
    m.assert_has_solid("hole").unwrap();

    // Tessellate and validate the mesh is actually visible
    let mesh = m.tessellate("hole").expect("tessellation should succeed");

    let n_verts = mesh.vertices.len() / 3;
    let n_tris = mesh.indices.len() / 3;
    let n_faces = mesh.face_ranges.len();
    eprintln!(
        "ZR9 mesh: {} vertices, {} triangles, {} faces",
        n_verts, n_tris, n_faces
    );

    assert!(!mesh.vertices.is_empty(), "ZR9: no vertices");
    assert!(n_faces >= 4, "ZR9: expected >= 4 faces, got {}", n_faces);

    // Count degenerate (zero-area) triangles
    let mut degenerate = 0u32;
    for tri in 0..n_tris {
        let i0 = mesh.indices[tri * 3] as usize;
        let i1 = mesh.indices[tri * 3 + 1] as usize;
        let i2 = mesh.indices[tri * 3 + 2] as usize;
        let v0 = [
            mesh.vertices[i0 * 3] as f64,
            mesh.vertices[i0 * 3 + 1] as f64,
            mesh.vertices[i0 * 3 + 2] as f64,
        ];
        let v1 = [
            mesh.vertices[i1 * 3] as f64,
            mesh.vertices[i1 * 3 + 1] as f64,
            mesh.vertices[i1 * 3 + 2] as f64,
        ];
        let v2 = [
            mesh.vertices[i2 * 3] as f64,
            mesh.vertices[i2 * 3 + 1] as f64,
            mesh.vertices[i2 * 3 + 2] as f64,
        ];
        let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
        let cross = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let area = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt() * 0.5;
        if area < 1e-18 {
            degenerate += 1;
        }
    }

    eprintln!("ZR9: {} degenerate of {} triangles", degenerate, n_tris);

    // KNOWN BUG: All triangles are currently degenerate due to frame rotation +
    // partial cylinder tessellation mismatch. This assertion documents the failure.
    assert!(
        (degenerate as usize) < n_tris / 2,
        "ZR9: {} of {} triangles are degenerate — body is invisible.\n\
         Root cause: build_partial_cyl_cyl arc angles are in Z-aligned frame,\n\
         but after rotate_boolean_result, tessellate_cylindrical_patch derives\n\
         different circle axes via make_circle_axes(rotated_axis), causing all\n\
         parametric vertices to collapse to identical positions.",
        degenerate,
        n_tris
    );
}
