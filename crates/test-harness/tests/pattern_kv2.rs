//! Circular / linear patterns on the REAL kernel (kernel-v2) — spec
//! `specs/custom_features_and_modeling_roadmap.md` §B1 oracles:
//!
//! 1. A pattern of N disjoint copies has volume N × seed (mesh volume within
//!    chord tolerance; the exact kernel volume where the evaluator covers the
//!    fixture) and N watertight bodies.
//! 2. `Add` of overlapping copies into a target is ONE shell with the
//!    inclusion–exclusion volume and χ = 2.
//! 3. `Cut` of copies from a target has the complementary volume and χ = 2.
//! 4. An `AxisRef::Entity` pick (a cylindrical face) drives the pattern axis.
//! 5. Provenance resolves on every copy (`PatternInstance { index }`), and a
//!    later feature can address a copy's body.
//! 6. Determinism: two identical builds tessellate identically.

use std::f64::consts::PI;

use feature_engine::types::*;
use test_harness::helpers::mesh_signed_volume;
use test_harness::oracle;
use test_harness::ModelBuilder;
use uuid::Uuid;
use waffle_types::*;

fn body_ref(feature_id: Uuid, key: OutputKey) -> GeomRef {
    GeomRef {
        kind: TopoKind::Solid,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: key,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
        scope: None,
    }
}

fn circular_about_z(
    seed: Uuid,
    count: u32,
    combine: Option<CombineMode>,
    targets: Option<Vec<GeomRef>>,
) -> Operation {
    Operation::PatternCircular {
        params: PatternCircularParams {
            seeds: PatternSeeds::Selected(vec![body_ref(seed, OutputKey::Main)]),
            axis: AxisRef::Explicit {
                origin: [0.0, 0.0, 0.0],
                direction: [0.0, 0.0, 1.0],
            },
            count,
            angle_deg: 360.0,
            angle_expr: None,
            skip: vec![],
            combine,
            targets,
        },
    }
}

/// Axis-aligned bounding box of a render mesh: `(min, max)`.
fn bbox(m: &waffle_types::kernel::RenderMesh) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for v in m.vertices.chunks(3) {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k] as f64);
            hi[k] = hi[k].max(v[k] as f64);
        }
    }
    (lo, hi)
}

fn center(m: &waffle_types::kernel::RenderMesh) -> [f64; 3] {
    let (lo, hi) = bbox(m);
    [
        (lo[0] + hi[0]) / 2.0,
        (lo[1] + hi[1]) / 2.0,
        (lo[2] + hi[2]) / 2.0,
    ]
}

fn assert_near3(a: [f64; 3], b: [f64; 3], tol: f64, what: &str) {
    for k in 0..3 {
        assert!(
            (a[k] - b[k]).abs() < tol,
            "{what}: axis {k}: {a:?} vs {b:?}"
        );
    }
}

fn live_volume(b: &mut ModelBuilder) -> f64 {
    b.tessellate_live_with_tol(0.001)
        .expect("tessellate live")
        .iter()
        .map(mesh_signed_volume)
        .sum()
}

fn assert_clean(b: &ModelBuilder, label: &str) {
    let errors = b.engine_errors().to_vec();
    assert!(errors.is_empty(), "{label}: engine errors: {errors:?}");
}

/// A spoke: box x∈[0.1, 0.6], y∈[−0.025, 0.025], z∈[0.05, 0.15] (volume 0.0025).
fn add_spoke(b: &mut ModelBuilder) -> Uuid {
    b.rect_sketch(
        "Spoke sketch",
        [0.0, 0.0, 0.05],
        [0.0, 0.0, 1.0],
        0.1,
        -0.025,
        0.5,
        0.05,
    )
    .unwrap();
    b.extrude_no_merge("Spoke", "Spoke sketch", 0.1).unwrap()
}

/// A hub: box x,y∈[−0.2, 0.2], z∈[0, 0.2] (volume 0.032). Its z faces are
/// NOT coplanar with a spoke's (z∈[0.05, 0.15]), so no M8 coplanar wall.
fn add_hub(b: &mut ModelBuilder) -> Uuid {
    b.rect_sketch(
        "Hub sketch",
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        -0.2,
        -0.2,
        0.4,
        0.4,
    )
    .unwrap();
    b.extrude_no_merge("Hub", "Hub sketch", 0.2).unwrap()
}

#[test]
fn circular_pattern_of_cylinders_has_n_times_the_seed_volume() {
    let mut b = ModelBuilder::kernel_v2();
    b.true_circle_sketch(
        "Peg sketch",
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        0.5,
        0.0,
        0.1,
    )
    .unwrap();
    let peg = b.extrude_no_merge("Peg", "Peg sketch", 0.2).unwrap();
    let seed_exact = b
        .kernel_ref()
        .as_introspect()
        .solid_volume(&b.solid_handle("Peg").unwrap())
        .expect("exact cylinder volume");
    assert!((seed_exact - PI * 0.01 * 0.2).abs() < 1e-12);

    let p = b
        .add_operation("Pegs", circular_about_z(peg, 6, None, None))
        .unwrap();
    assert_clean(&b, "6 pegs");
    assert!(b.consumed_features().contains(&peg));
    let r = b.op_result("Pegs").unwrap().clone();
    assert_eq!(r.outputs.len(), 6);
    assert_eq!(
        b.distinct_solid_count(),
        1,
        "one live feature (the pattern) owns all bodies"
    );

    // Exact volume of every instance equals the seed's (rigid copy).
    for (i, (_, body)) in r.outputs.iter().enumerate() {
        let v = b
            .kernel_ref()
            .as_introspect()
            .solid_volume(&body.handle)
            .expect("exact volume");
        assert!(
            (v - seed_exact).abs() < 1e-12,
            "instance {i}: {v} vs {seed_exact}"
        );
    }
    // Mesh volume: N × seed within chord tolerance; each body watertight.
    let meshes = b.tessellate_live_with_tol(0.001).unwrap();
    assert_eq!(meshes.len(), 6);
    let total: f64 = meshes.iter().map(mesh_signed_volume).sum();
    assert!(
        (total - 6.0 * seed_exact).abs() < 6.0 * seed_exact * 2e-3,
        "{total}"
    );
    for (i, m) in meshes.iter().enumerate() {
        let wt = oracle::check_watertight_mesh(m);
        assert!(wt.passed, "instance {i}: {}", wt.detail);
    }
    // Instance 3 (180° about z) is the seed's centre reflected through the
    // axis (the sketch's u/v are not world x/y, so read the seed's centre
    // from its own bbox rather than assuming it).
    let c0 = center(&meshes[0]);
    let c3 = center(&meshes[3]);
    assert!(c0[0].hypot(c0[1]) > 0.49, "seed is off-axis: {c0:?}");
    assert_near3(c3, [-c0[0], -c0[1], c0[2]], 1e-3, "instance 3 centre");
    // Instance 1 (60°): same radius, rotated by 60°.
    let c1 = center(&meshes[1]);
    let (s60, c60) = (PI / 3.0).sin_cos();
    assert_near3(
        c1,
        [c0[0] * c60 - c0[1] * s60, c0[0] * s60 + c0[1] * c60, c0[2]],
        1e-3,
        "instance 1 centre",
    );

    // Provenance: every instance's faces carry PatternInstance { index }.
    let introspect = b.kernel_ref().as_introspect();
    for (i, (_, body)) in r.outputs.iter().enumerate() {
        for f in introspect.list_faces(&body.handle) {
            assert!(
                r.provenance
                    .role_assignments
                    .contains(&(f, Role::PatternInstance { index: i })),
                "instance {i} face {f:?} lacks its role"
            );
        }
    }
    // A later feature can address instance 4's body as a boolean operand:
    // a bore through instance 4, sketched on a plane whose origin is that
    // instance's axis (so the circle at sketch (0,0) is centred on it).
    let inst4 = body_ref(p, OutputKey::Body { index: 4 });
    let c4 = center(&meshes[4]);
    b.true_circle_sketch(
        "Bore sketch",
        [c4[0], c4[1], 0.0],
        [0.0, 0.0, 1.0],
        0.0,
        0.0,
        0.03,
    )
    .unwrap();
    let bore = Operation::Extrude {
        params: ExtrudeParams {
            combine: Some(CombineMode::Cut),
            targets: Some(vec![inst4]),
            sketch_id: b.feature_id("Bore sketch").unwrap(),
            profile_index: 0,
            profile_entity_ids: None,
            depth: 0.3,
            direction: None,
            symmetric: false,
            cut: true,
            merge: false,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
            depth_expr: None,
        },
    };
    b.add_operation("Bore", bore).unwrap();
    assert_clean(&b, "bore into instance 4");
    // The cut consumed the pattern feature; the cut feature carries the other
    // five instances unchanged (custody rule) — six live bodies, total volume
    // 6·seed − bore∩peg (bore: r=0.03 over the peg's full height 0.2).
    let total = live_volume(&mut b);
    let expect = 6.0 * seed_exact - PI * 0.03 * 0.03 * 0.2;
    assert!(
        (total - expect).abs() < expect * 2e-3,
        "{total} vs {expect}"
    );
}

#[test]
fn add_patterned_spokes_into_a_hub_is_one_shell_with_inclusion_exclusion_volume() {
    let mut b = ModelBuilder::kernel_v2();
    let hub = add_hub(&mut b);
    let spoke = add_spoke(&mut b);
    b.add_operation(
        "Spokes",
        circular_about_z(
            spoke,
            4,
            Some(CombineMode::Add),
            Some(vec![body_ref(hub, OutputKey::Main)]),
        ),
    )
    .unwrap();
    assert_clean(&b, "4 spokes Add");
    assert!(b.consumed_features().contains(&hub) && b.consumed_features().contains(&spoke));
    let r = b.op_result("Spokes").unwrap();
    assert_eq!(
        r.outputs.len(),
        1,
        "one connected body: {:?}",
        r.diagnostics.warnings
    );
    // hub 0.032 + 4·0.0025 − 4·(0.1·0.05·0.1)
    let expect = 0.032 + 0.01 - 0.002;
    let v = b
        .kernel_ref()
        .as_introspect()
        .solid_volume(&r.outputs[0].1.handle)
        .expect("exact volume");
    assert!((v - expect).abs() < 1e-9, "{v} vs {expect}");
    let mesh = b.tessellate_last_with_tol(0.001).unwrap();
    let chi = oracle::check_mesh_euler_characteristic(&mesh, 2);
    assert!(chi.passed, "{}", chi.detail);
    let wt = oracle::check_watertight_mesh(&mesh);
    assert!(wt.passed, "{}", wt.detail);
}

#[test]
fn cut_patterned_spokes_from_a_hub_leaves_four_pockets() {
    let mut b = ModelBuilder::kernel_v2();
    let hub = add_hub(&mut b);
    let spoke = add_spoke(&mut b);
    b.add_operation(
        "Pockets",
        circular_about_z(
            spoke,
            4,
            Some(CombineMode::Cut),
            Some(vec![body_ref(hub, OutputKey::Main)]),
        ),
    )
    .unwrap();
    assert_clean(&b, "4 spokes Cut");
    let r = b.op_result("Pockets").unwrap();
    assert_eq!(r.outputs.len(), 1);
    let expect = 0.032 - 0.002;
    let v = b
        .kernel_ref()
        .as_introspect()
        .solid_volume(&r.outputs[0].1.handle)
        .expect("exact volume");
    assert!((v - expect).abs() < 1e-9, "{v} vs {expect}");
    let mesh = b.tessellate_last_with_tol(0.001).unwrap();
    let chi = oracle::check_mesh_euler_characteristic(&mesh, 2);
    assert!(chi.passed, "{}", chi.detail);
    // The spoke instances are consumed by the cut: only the hub remains live.
    assert_eq!(b.distinct_solid_count(), 1);
    assert!((live_volume(&mut b) - expect).abs() < expect * 2e-3);
}

#[test]
fn linear_pattern_and_grid_volumes() {
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("Tile sketch", [0.0; 3], [0.0, 0.0, 1.0], 0.0, 0.0, 0.1, 0.1)
        .unwrap();
    let tile = b.extrude_no_merge("Tile", "Tile sketch", 0.05).unwrap();
    let seed_v = 0.1 * 0.1 * 0.05;
    let op = Operation::PatternLinear {
        params: PatternLinearParams {
            seeds: PatternSeeds::Selected(vec![body_ref(tile, OutputKey::Main)]),
            direction: AxisRef::Explicit {
                origin: [0.0; 3],
                direction: [2.0, 0.0, 0.0], // non-unit: normalized
            },
            count: 3,
            spacing: 0.15,
            spacing_expr: None,
            second: Some(LinearSecondDirection {
                direction: AxisRef::Explicit {
                    origin: [0.0; 3],
                    direction: [0.0, 1.0, 0.0],
                },
                count: 2,
                spacing: -0.2, // negative: toward −y
                spacing_expr: None,
            }),
            skip: vec![5],
            combine: None,
            targets: None,
        },
    };
    b.add_operation("Tiles", op).unwrap();
    assert_clean(&b, "3×2 grid minus one");
    let r = b.op_result("Tiles").unwrap();
    assert_eq!(r.outputs.len(), 5);
    let meshes = b.tessellate_live_with_tol(0.001).unwrap();
    assert_eq!(meshes.len(), 5);
    let total: f64 = meshes.iter().map(mesh_signed_volume).sum();
    assert!((total - 5.0 * seed_v).abs() < 1e-9, "{total}");
    // Index 4 = (i=1, j=1): the seed shifted by (+0.15, −0.2, 0) in WORLD
    // space (directions are world vectors regardless of the sketch's axes).
    let c0 = center(&meshes[0]);
    let c4 = center(&meshes[4]);
    assert_near3(
        c4,
        [c0[0] + 0.15, c0[1] - 0.2, c0[2]],
        1e-6,
        "index 4 centre",
    );
    // Index 2 = (i=2, j=0): +0.30 along x only.
    let c2 = center(&meshes[2]);
    assert_near3(c2, [c0[0] + 0.30, c0[1], c0[2]], 1e-6, "index 2 centre");
}

#[test]
fn entity_axis_from_a_cylindrical_face_drives_the_pattern() {
    let mut b = ModelBuilder::kernel_v2();
    // Post: cylinder r=0.05 at (1, 1), z∈[0, 0.3].
    b.true_circle_sketch("Post sketch", [0.0; 3], [0.0, 0.0, 1.0], 1.0, 1.0, 0.05)
        .unwrap();
    b.extrude_no_merge("Post", "Post sketch", 0.3).unwrap();
    // Seed: box at (1.3..1.4, 0.95..1.05), z∈[0.1, 0.2] — 0.3 m from the post axis.
    b.rect_sketch(
        "Bump sketch",
        [0.0, 0.0, 0.1],
        [0.0, 0.0, 1.0],
        1.3,
        0.95,
        0.1,
        0.1,
    )
    .unwrap();
    let bump = b.extrude_no_merge("Bump", "Bump sketch", 0.1).unwrap();
    // The post's lateral face, by role (a circle extrude's single side face).
    let lateral = b
        .select_face_by_role("Post", Role::SideFace { index: 0 }, 0)
        .unwrap();
    let post_center = {
        let m = b.tessellate("Post").unwrap();
        center(&m)
    };
    let bump_center = {
        let m = b.tessellate("Bump").unwrap();
        center(&m)
    };
    let op = Operation::PatternCircular {
        params: PatternCircularParams {
            seeds: PatternSeeds::Selected(vec![body_ref(bump, OutputKey::Main)]),
            axis: AxisRef::Entity { geom_ref: lateral },
            count: 4,
            angle_deg: 360.0,
            angle_expr: None,
            skip: vec![],
            combine: None,
            targets: None,
        },
    };
    b.add_operation("Bumps", op).unwrap();
    assert_clean(&b, "entity axis");
    let meshes = b.tessellate_live_with_tol(0.001).unwrap();
    // Post + 4 bumps live.
    assert_eq!(meshes.len(), 5);
    // Instance 2 (180° about the post's axis): the bump's centre reflected
    // through the post's centre in the xy plane, same z.
    let r = b.op_result("Bumps").unwrap().clone();
    let m = b
        .kernel_mut()
        .tessellate(&r.outputs[2].1.handle.clone(), 0.001)
        .unwrap();
    let c2 = center(&m);
    assert_near3(
        c2,
        [
            2.0 * post_center[0] - bump_center[0],
            2.0 * post_center[1] - bump_center[1],
            bump_center[2],
        ],
        // The post's centre is read off a chord-polygon mesh (tol 1e-3), so
        // the EXPECTATION carries mesh error; the instance itself is exact.
        2e-3,
        "instance 2 centre",
    );
}

#[test]
fn identical_builds_tessellate_identically() {
    let build = || {
        let mut b = ModelBuilder::kernel_v2();
        let hub = add_hub(&mut b);
        let spoke = add_spoke(&mut b);
        b.add_operation(
            "Spokes",
            circular_about_z(
                spoke,
                5,
                Some(CombineMode::Add),
                Some(vec![body_ref(hub, OutputKey::Main)]),
            ),
        )
        .unwrap();
        assert_clean(&b, "determinism build");
        b.tessellate_last_with_tol(0.001).unwrap()
    };
    let a = build();
    let c = build();
    assert_eq!(a.vertices, c.vertices);
    assert_eq!(a.indices, c.indices);
}

// ── Mirror pattern on real geometry ─────────────────────────────────────────

fn mirror_in(seed: Uuid, origin: [f64; 3], normal: [f64; 3]) -> Operation {
    Operation::PatternMirror {
        params: PatternMirrorParams {
            seeds: PatternSeeds::Selected(vec![body_ref(seed, OutputKey::Main)]),
            plane: AxisRef::Explicit {
                origin,
                direction: normal,
            },
            combine: None,
            targets: None,
        },
    }
}

/// 7. A mirrored body is a REAL solid: watertight, χ = 2, the seed's volume
///    (positive — an inside-out reflection would integrate to −V), and at the
///    reflected position.
#[test]
fn mirrored_spoke_is_a_watertight_body_at_the_reflected_position() {
    let mut b = ModelBuilder::kernel_v2();
    let spoke = add_spoke(&mut b);
    // The sketch's u/v are not world x/y: the spoke lands at y ∈ [−0.6, −0.1],
    // x ∈ [−0.025, 0.025]. Mirror in y = 0 ⇒ y ∈ [0.1, 0.6]. (Mirroring in
    // x = 0 would land the copy exactly on the seed, which tests nothing.)
    b.add_operation("Mirror", mirror_in(spoke, [0.0; 3], [0.0, 1.0, 0.0]))
        .unwrap();
    assert_clean(&b, "mirror");
    let r = b.op_result("Mirror").unwrap();
    assert_eq!(r.outputs.len(), 2, "the seed and its reflection");
    let v = b
        .kernel_ref()
        .as_introspect()
        .solid_volume(&r.outputs[1].1.handle)
        .expect("exact volume");
    assert!((v - 0.0025).abs() < 1e-12, "mirrored volume {v}");
    let meshes = b.tessellate_live_with_tol(0.001).expect("tessellate");
    assert_eq!(meshes.len(), 2);
    for m in &meshes {
        let wt = oracle::check_watertight_mesh(m);
        assert!(wt.passed, "{}", wt.detail);
        let chi = oracle::check_mesh_euler_characteristic(m, 2);
        assert!(chi.passed, "{}", chi.detail);
        assert!(
            mesh_signed_volume(m) > 0.0,
            "a mirrored body is not inside out"
        );
    }
    let (lo, hi) = bbox(&meshes[1]);
    assert_near3(lo, [-0.025, 0.1, 0.05], 1e-6, "mirrored min");
    assert_near3(hi, [0.025, 0.6, 0.15], 1e-6, "mirrored max");
}

/// 8. The mirrored copy is a boolean operand like any other: mirroring a
///    spoke ACROSS the hub's centre and adding both gives one shell with the
///    inclusion–exclusion volume.
#[test]
fn a_mirrored_spoke_adds_into_a_hub_as_one_shell() {
    let mut b = ModelBuilder::kernel_v2();
    let hub = add_hub(&mut b);
    let spoke = add_spoke(&mut b);
    b.add_operation(
        "Both spokes",
        Operation::PatternMirror {
            params: PatternMirrorParams {
                seeds: PatternSeeds::Selected(vec![body_ref(spoke, OutputKey::Main)]),
                plane: AxisRef::Explicit {
                    origin: [0.0; 3],
                    direction: [0.0, 1.0, 0.0],
                },
                combine: Some(CombineMode::Add),
                targets: Some(vec![body_ref(hub, OutputKey::Main)]),
            },
        },
    )
    .unwrap();
    assert_clean(&b, "mirror Add");
    let r = b.op_result("Both spokes").unwrap();
    assert_eq!(r.outputs.len(), 1, "one connected body");
    // hub 0.032 + 2·0.0025 − 2·(0.1·0.05·0.1 overlap inside the hub)
    let expect = 0.032 + 0.005 - 0.001;
    let v = b
        .kernel_ref()
        .as_introspect()
        .solid_volume(&r.outputs[0].1.handle)
        .expect("exact volume");
    assert!((v - expect).abs() < 1e-9, "{v} vs {expect}");
    let mesh = b.tessellate_last_with_tol(0.001).unwrap();
    let chi = oracle::check_mesh_euler_characteristic(&mesh, 2);
    assert!(chi.passed, "{}", chi.detail);
    let wt = oracle::check_watertight_mesh(&mesh);
    assert!(wt.passed, "{}", wt.detail);
}

/// 9. `seeds: All` patterns every live body without naming one — the whole
///    point of the set (FEATURE_NOTES §7). Four bodies out of two.
#[test]
fn all_seeds_patterns_every_live_body() {
    let mut b = ModelBuilder::kernel_v2();
    add_hub(&mut b);
    add_spoke(&mut b);
    b.add_operation(
        "Mirror everything",
        Operation::PatternMirror {
            params: PatternMirrorParams {
                seeds: PatternSeeds::All,
                plane: AxisRef::Explicit {
                    origin: [0.0, 0.0, -0.5],
                    direction: [0.0, 0.0, 1.0],
                },
                combine: None,
                targets: None,
            },
        },
    )
    .unwrap();
    assert_clean(&b, "mirror all");
    let r = b.op_result("Mirror everything").unwrap();
    assert_eq!(r.outputs.len(), 4, "hub + spoke, each with its reflection");
    // Reflected in z = −0.5, so every copy sits below it; total volume is
    // twice the originals'.
    assert!((live_volume(&mut b) - 2.0 * (0.032 + 0.0025)).abs() < 1e-4);
}
