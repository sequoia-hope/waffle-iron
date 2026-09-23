//! `Operation::UnionAll` on the REAL kernel (kernel-v2) — the oracles of
//! `specs/b4_balanced_union.md` §3:
//!
//! 1. A chain of five overlapping boxes unions to ONE shell with the
//!    inclusion–exclusion exact volume, χ = 2, watertight.
//! 2. A far-away cluster stays a separate body; the conservative-box gate
//!    never runs the kernel across clusters (union count = 4 for 6 bodies).
//! 3. Determinism: the tree gives the same exact volume and census as the
//!    same bodies chained through four `BooleanCombine` features, and two
//!    identical builds tessellate identically.
//! 4. Custody: every source feature is consumed, `Main` is the first body's
//!    lump, a later feature addresses `Body{1}`.
//! 5. A `BooleanCombine` on a consumed operand is a typed error, no output.

use std::cell::RefCell;
use std::rc::Rc;

use feature_engine::progress::{self, ProgressEvent};
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

fn union_all() -> Operation {
    Operation::UnionAll {
        params: UnionAllParams::default(),
    }
}

fn assert_clean(b: &ModelBuilder, label: &str) {
    let errors = b.engine_errors().to_vec();
    assert!(errors.is_empty(), "{label}: engine errors: {errors:?}");
}

/// Box `i` of the chain: x ∈ [0.8i, 0.8i + 1], y ∈ [0.1i, 0.1i + 1],
/// z ∈ [0.05i, 0.05i + 1] (unit cube, volume 1). Consecutive boxes overlap
/// by 0.2 × 0.9 × 0.95 = 0.171; boxes two apart do not overlap (x gap 0.6);
/// no two boxes share a face plane (no Stage-0 coplanar path).
fn chain_box(b: &mut ModelBuilder, i: usize) -> Uuid {
    let f = i as f64;
    b.rect_sketch(
        &format!("Sketch {i}"),
        [0.0, 0.0, 0.05 * f],
        [0.0, 0.0, 1.0],
        0.8 * f,
        0.1 * f,
        1.0,
        1.0,
    )
    .unwrap();
    b.extrude_no_merge(&format!("Box {i}"), &format!("Sketch {i}"), 1.0)
        .unwrap()
}

/// A unit cube far along x (x ∈ [100, 101]).
fn far_box(b: &mut ModelBuilder, name: &str, x: f64) -> Uuid {
    b.rect_sketch(
        &format!("{name} sketch"),
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        x,
        0.0,
        1.0,
        1.0,
    )
    .unwrap();
    b.extrude_no_merge(name, &format!("{name} sketch"), 1.0)
        .unwrap()
}

const CHAIN_VOLUME: f64 = 5.0 - 4.0 * 0.171;

fn exact_volume(b: &ModelBuilder, handle: &waffle_types::kernel::KernelSolidHandle) -> f64 {
    b.kernel_ref()
        .as_introspect()
        .solid_volume(handle)
        .expect("exact volume")
}

fn capture_progress() -> Rc<RefCell<Vec<ProgressEvent>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s2 = Rc::clone(&seen);
    progress::install(Box::new(move |e| s2.borrow_mut().push(e.clone())));
    seen
}

#[test]
fn chain_of_five_boxes_is_one_shell_with_inclusion_exclusion_volume() {
    let mut b = ModelBuilder::kernel_v2();
    let boxes: Vec<Uuid> = (0..5).map(|i| chain_box(&mut b, i)).collect();
    let seen = capture_progress();
    let u = b.add_operation("Union", union_all()).unwrap();
    progress::clear();
    assert_clean(&b, "union of five");

    let r = b.op_result("Union").unwrap();
    assert_eq!(r.outputs.len(), 1, "one lump: {:?}", r.diagnostics.warnings);
    assert_eq!(r.outputs[0].0, OutputKey::Main);
    let v = exact_volume(&b, &r.outputs[0].1.handle);
    assert!((v - CHAIN_VOLUME).abs() < 1e-9, "{v} vs {CHAIN_VOLUME}");
    for f in &boxes {
        assert!(b.consumed_features().contains(f), "box {f} consumed");
    }
    assert!(!b.consumed_features().contains(&u));

    // Four unions connect five bodies; every frame names the feature.
    let done: Vec<usize> = seen.borrow().iter().map(|e| e.done).collect();
    assert_eq!(done, vec![0, 1, 2, 3, 4, 4], "{:?}", seen.borrow());
    assert!(seen.borrow().iter().all(|e| e.feature_id == u));

    let mesh = b.tessellate_last_with_tol(0.001).unwrap();
    let chi = oracle::check_mesh_euler_characteristic(&mesh, 2);
    assert!(chi.passed, "{}", chi.detail);
    let wt = oracle::check_watertight_mesh(&mesh);
    assert!(wt.passed, "{}", wt.detail);
    let mv = mesh_signed_volume(&mesh);
    assert!((mv - CHAIN_VOLUME).abs() < 1e-6, "mesh volume {mv}");
}

#[test]
fn far_cluster_stays_a_separate_body_and_the_gate_skips_it() {
    let mut b = ModelBuilder::kernel_v2();
    for i in 0..5 {
        chain_box(&mut b, i);
    }
    let far = far_box(&mut b, "Far", 100.0);
    let seen = capture_progress();
    b.add_operation("Union", union_all()).unwrap();
    progress::clear();
    assert_clean(&b, "union with a far body");

    let r = b.op_result("Union").unwrap();
    assert_eq!(r.outputs.len(), 2, "{:?}", r.diagnostics.warnings);
    assert_eq!(r.outputs[0].0, OutputKey::Main);
    assert_eq!(r.outputs[1].0, OutputKey::Body { index: 1 });
    let v0 = exact_volume(&b, &r.outputs[0].1.handle);
    let v1 = exact_volume(&b, &r.outputs[1].1.handle);
    assert!((v0 - CHAIN_VOLUME).abs() < 1e-9, "chain lump {v0}");
    assert!((v1 - 1.0).abs() < 1e-9, "far lump {v1}");
    // Still four unions: the far box never reached the kernel.
    let unions = seen.borrow().iter().map(|e| e.done).max().unwrap();
    assert_eq!(unions, 4, "{:?}", seen.borrow());
    assert!(b.consumed_features().contains(&far));

    // A later feature addresses the second lump: a box overlapping the far
    // cube unions into `Body{1}` and consumes the UnionAll feature.
    let u = b.feature_id("Union").unwrap();
    far_box(&mut b, "Far 2", 100.5);
    let far2 = b.feature_id("Far 2").unwrap();
    b.add_operation(
        "Onto body 1",
        Operation::BooleanCombine {
            params: BooleanParams {
                body_a: body_ref(u, OutputKey::Body { index: 1 }),
                body_b: body_ref(far2, OutputKey::Main),
                operation: BooleanOp::Union,
            },
        },
    )
    .unwrap();
    assert_clean(&b, "union onto body 1");
    let r2 = b.op_result("Onto body 1").unwrap();
    let v = exact_volume(&b, &r2.outputs[0].1.handle);
    assert!((v - 1.5).abs() < 1e-9, "far ∪ far2 = 1 + 1 − 0.5: {v}");
    assert!(b.consumed_features().contains(&u));
}

#[test]
fn tree_matches_the_chain_of_pairwise_booleans_and_is_deterministic() {
    // The chain: four BooleanCombine features.
    let mut chain = ModelBuilder::kernel_v2();
    for i in 0..5 {
        chain_box(&mut chain, i);
    }
    chain.boolean_union("U01", "Box 0", "Box 1").unwrap();
    chain.boolean_union("U012", "U01", "Box 2").unwrap();
    chain.boolean_union("U0123", "U012", "Box 3").unwrap();
    chain.boolean_union("U01234", "U0123", "Box 4").unwrap();
    assert_clean(&chain, "chain");
    let rc = chain.op_result("U01234").unwrap();
    assert_eq!(rc.outputs.len(), 1);
    let vc = exact_volume(&chain, &rc.outputs[0].1.handle);
    let mc = chain.tessellate_last_with_tol(0.001).unwrap();

    // The tree, built twice.
    let build = || {
        let mut b = ModelBuilder::kernel_v2();
        for i in 0..5 {
            chain_box(&mut b, i);
        }
        b.add_operation("Union", union_all()).unwrap();
        assert_clean(&b, "tree");
        let r = b.op_result("Union").unwrap();
        assert_eq!(r.outputs.len(), 1);
        let v = exact_volume(&b, &r.outputs[0].1.handle);
        let m = b.tessellate_last_with_tol(0.001).unwrap();
        (v, m)
    };
    let (v1, m1) = build();
    let (v2, m2) = build();
    assert_eq!(
        v1.to_bits(),
        v2.to_bits(),
        "identical builds: identical exact volume"
    );
    assert!((v1 - vc).abs() < 1e-9, "tree {v1} vs chain {vc}");
    assert!((v1 - CHAIN_VOLUME).abs() < 1e-9);
    assert_eq!(
        m1.vertices, m2.vertices,
        "identical builds tessellate identically"
    );
    assert_eq!(m1.indices, m2.indices);
    let chi_c = oracle::check_mesh_euler_characteristic(&mc, 2);
    let chi_t = oracle::check_mesh_euler_characteristic(&m1, 2);
    assert!(
        chi_c.passed && chi_t.passed,
        "{} / {}",
        chi_c.detail,
        chi_t.detail
    );
    assert!(
        (mesh_signed_volume(&mc) - mesh_signed_volume(&m1)).abs() < 1e-6,
        "mesh volumes agree"
    );
}

#[test]
fn boolean_combine_on_a_consumed_operand_is_a_typed_error() {
    let mut b = ModelBuilder::kernel_v2();
    let b0 = chain_box(&mut b, 0);
    chain_box(&mut b, 1);
    b.boolean_union("U01", "Box 0", "Box 1").unwrap();
    assert_clean(&b, "first union");
    let b2 = chain_box(&mut b, 2);
    // Box 0 was consumed by U01: unioning it again must not duplicate it.
    let stale = b
        .add_operation(
            "Stale",
            Operation::BooleanCombine {
                params: BooleanParams {
                    body_a: body_ref(b0, OutputKey::Main),
                    body_b: body_ref(b2, OutputKey::Main),
                    operation: BooleanOp::Union,
                },
            },
        )
        .unwrap();
    let err = b
        .engine_errors()
        .iter()
        .find(|(f, _)| *f == stale)
        .map(|(_, m)| m.clone())
        .expect("stale operand is loud");
    assert!(err.contains("already consumed"), "{err}");
    assert!(b.op_result("Stale").map_or(true, |r| r.outputs.is_empty()));
    assert!(!b.consumed_features().contains(&b2));
}
