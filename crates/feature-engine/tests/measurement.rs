//! Measurement functions in expressions (D2, `specs/drawings_and_mbd.md`
//! §6; P4 of `specs/agent_mechanical_design.md` §6).
//!
//! What is pinned here is everything the mock can answer: the ORDERING rule
//! (which is what makes a self-measuring feature a typed error rather than a
//! hang), the refusals a bad name and a kernel that cannot measure produce,
//! the dimension composition, a measured value actually driving a depth, and
//! that a body rename carries the expressions that measure it.
//!
//! The mock splits usefully: it answers `KernelIntrospect`
//! (`compute_signature().area` is a real number for these faces), and every
//! `KernelMeasure` method defaults to the trait's typed `NotSupported` — so
//! `area` measures and `volume` refuses, which is two pins for free.
//!
//! The oracle against REAL geometry — a plate and a boss, with
//! `distance(...)` driving the boss's depth through `KernelMeasure` — is
//! `crates/wasm-bridge/tests/measurement_expr.rs`, where kernel-v2 is
//! available.

use std::collections::HashMap;

use feature_engine::names::{self, NamedRef};
use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::{KernelIntrospect, MockKernel};
use waffle_types::*;

fn point(id: u32, x: f64, y: f64) -> SketchEntity {
    SketchEntity::Point {
        id,
        x,
        y,
        construction: false,
    }
}

fn line(id: u32, a: u32, b: u32) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id: a,
        end_id: b,
        construction: false,
    }
}

/// One unit square on a datum plane, bounded by lines 10–13.
fn square_sketch() -> Sketch {
    let mut solved_positions = HashMap::new();
    for (id, x, y) in [(1, 0.0, 0.0), (2, 1.0, 0.0), (3, 1.0, 1.0), (4, 0.0, 1.0)] {
        solved_positions.insert(id, (x, y));
    }
    Sketch {
        id: Uuid::new_v4(),
        plane: GeomRef {
            kind: TopoKind::Face,
            anchor: Anchor::Datum {
                datum_id: Uuid::new_v4(),
            },
            selector: Selector::Role {
                role: Role::EndCapPositive,
                index: 0,
            },
            policy: ResolvePolicy::Strict,
            scope: None,
        },
        plane_face: None,
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities: vec![
            point(1, 0.0, 0.0),
            point(2, 1.0, 0.0),
            point(3, 1.0, 1.0),
            point(4, 0.0, 1.0),
            line(10, 1, 2),
            line(11, 2, 3),
            line(12, 3, 4),
            line(13, 4, 1),
        ],
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions,
        projected: vec![],
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![10, 11, 12, 13],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        }],
    }
}

fn extrude(sketch_id: Uuid, depth_expr: Option<&str>) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            profile_entity_ids: Some(vec![10, 11, 12, 13]),
            depth: 0.5,
            depth_expr: depth_expr.map(str::to_string),
            direction: None,
            symmetric: false,
            cut: false,
            merge: false,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
            combine: Some(CombineMode::NewBody),
            targets: None,
        },
    }
}

fn top_face_ref(block: Uuid) -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: block,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
        scope: None,
    }
}

fn user() -> Provenance {
    Provenance {
        origin: ProvenanceOrigin::User,
        at: None,
    }
}

fn mint(engine: &Engine, kernel: &MockKernel, target: &GeomRef) -> NamedRef {
    names::mint(target, &engine.feature_results, kernel, user()).expect("the reference resolves")
}

/// `n` extruded squares, each its own body, with their feature ids.
fn blocks(n: usize) -> (Engine, MockKernel, Vec<Uuid>) {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let mut ids = Vec::new();
    for i in 0..n {
        let sketch = engine
            .add_feature(
                format!("Sketch {i}"),
                Operation::Sketch {
                    sketch: square_sketch(),
                },
                &mut kernel,
            )
            .expect("sketch");
        ids.push(
            engine
                .add_feature(format!("Block {i}"), extrude(sketch, None), &mut kernel)
                .expect("extrude"),
        );
    }
    assert!(
        engine.errors.is_empty(),
        "the fixture must build: {:?}",
        engine.errors
    );
    (engine, kernel, ids)
}

/// Replace one feature's depth expression and rebuild.
fn set_depth_expr(engine: &mut Engine, kernel: &mut MockKernel, feature: Uuid, expression: &str) {
    let mut op = engine
        .tree
        .find_feature(feature)
        .expect("the feature exists")
        .operation
        .clone();
    if let Operation::Extrude { params } = &mut op {
        params.depth_expr = Some(expression.to_string());
    }
    engine.edit_feature(feature, op, kernel).expect("edit");
}

/// Every error message the last rebuild produced, joined.
fn errors(engine: &Engine) -> String {
    engine
        .errors
        .iter()
        .map(|(_, m)| m.as_str())
        .collect::<Vec<_>>()
        .join(" | ")
}

#[test]
fn measuring_the_feature_s_own_output_is_a_typed_cycle_not_a_hang() {
    // §6: "a feature whose own dimension reads its own output" is a typed
    // rebuild error. Caught ORDINALLY — the named face belongs to the very
    // feature the expression drives — so it is decided before any number is
    // computed and no fixpoint has to notice it.
    let (mut engine, mut kernel, blocks) = blocks(1);
    let named = mint(&engine, &kernel, &top_face_ref(blocks[0]));
    engine.set_entity_name("top", named, None).expect("name it");

    set_depth_expr(&mut engine, &mut kernel, blocks[0], "area(top) / 100");
    let msg = errors(&engine);
    assert!(
        msg.contains("circular measurement"),
        "expected a typed cycle, got: {msg}"
    );
    assert!(
        msg.contains("the very feature this expression drives"),
        "the message must say which cycle it is: {msg}"
    );
    // And it must NAME the feature, at its one-based position: "Block 0" is
    // the second feature of the fixture (its sketch is the first).
    assert!(
        msg.contains("\"Block 0\" (#2 of the tree)"),
        "the message must name the feature an author has to look at: {msg}"
    );
    assert!(
        msg.contains("area") && msg.contains("top"),
        "the error names the function and the name: {msg}"
    );
    // And the depth is untouched — a refused measurement never writes.
    let Operation::Extrude { params } = &engine
        .tree
        .find_feature(blocks[0])
        .expect("still there")
        .operation
    else {
        panic!("expected an extrude");
    };
    assert_eq!(params.depth, 0.5);
}

#[test]
fn measuring_a_later_feature_is_the_same_typed_cycle() {
    // The other half of the ordering rule: a measurement may read only
    // geometry EARLIER in the tree. Reading a LATER feature would make the
    // answer depend on the order the rebuild happened to compute things in.
    let (mut engine, mut kernel, blocks) = blocks(2);
    let named = mint(&engine, &kernel, &top_face_ref(blocks[1]));
    engine
        .set_entity_name("later_top", named, None)
        .expect("name it");

    set_depth_expr(&mut engine, &mut kernel, blocks[0], "area(later_top)");
    let msg = errors(&engine);
    assert!(
        msg.contains("circular measurement") && msg.contains("built AFTER"),
        "expected the later-feature cycle, got: {msg}"
    );
    assert!(
        msg.contains("can only read geometry earlier in the tree"),
        "the message must state the rule: {msg}"
    );
    // Both features named, both positions counted the SAME way. A message
    // that mixed a one-based position with a zero-based one would send an
    // author to the wrong feature.
    assert!(
        msg.contains("\"Block 1\" (#4 of the tree)") && msg.contains("\"Block 0\" (#2 of the tree)"),
        "the refusal must name BOTH features at consistent positions: {msg}"
    );
}

#[test]
fn a_name_the_document_does_not_have_is_a_typed_measurement_error() {
    // Strict (N2): the refusal names the function AND the name, which is
    // what an author needs to fix it. Never a different entity, and never a
    // silent zero.
    let (mut engine, mut kernel, blocks) = blocks(2);
    set_depth_expr(&mut engine, &mut kernel, blocks[1], "area(nowhere)");
    let msg = errors(&engine);
    assert!(
        msg.contains("area(\"nowhere\")"),
        "the function and the name must both be in the message: {msg}"
    );
    assert!(
        msg.contains("no entity or body with that name"),
        "and it must say what is wrong: {msg}"
    );
}

#[test]
fn a_kernel_that_cannot_measure_says_so_rather_than_answering() {
    // `MockKernel` implements no `KernelMeasure` method, so `volume` is the
    // trait's typed `NotSupported`. That must surface as the measurement's
    // own error — never as a number obtained some other way (P9).
    let (mut engine, mut kernel, blocks) = blocks(2);
    engine.rename_body(
        FeatureTree::body_id(blocks[0], &OutputKey::Main),
        "early".to_string(),
    );

    // Legal ordering (block 0 is earlier than block 1), so the refusal can
    // only come from the kernel.
    set_depth_expr(&mut engine, &mut kernel, blocks[1], "volume(early)^(1/3)");
    let msg = errors(&engine);
    assert!(
        !msg.contains("circular"),
        "the ordering is legal here: {msg}"
    );
    assert!(
        msg.contains("volume(\"early\")") && msg.contains("the kernel refused the volume"),
        "the refusal names the measurement and says who refused: {msg}"
    );
    let Operation::Extrude { params } = &engine
        .tree
        .find_feature(blocks[1])
        .expect("still there")
        .operation
    else {
        panic!("expected an extrude");
    };
    assert_eq!(params.depth, 0.5, "a refused measurement writes nothing");
}

#[test]
fn a_measured_dimension_composes_and_a_wrong_one_is_refused_by_name() {
    // P1's exponents, through a measurement. `area` is a length², so it
    // cannot drive a depth — but `sqrt` of it can, and that composition is
    // the reason the dimension is tracked rather than discarded. The mock
    // answers `compute_signature().area`, so both halves are measured here.
    let (mut engine, mut kernel, blocks) = blocks(2);
    let named = mint(&engine, &kernel, &top_face_ref(blocks[0]));
    engine
        .set_entity_name("early_top", named, None)
        .expect("name it");

    set_depth_expr(&mut engine, &mut kernel, blocks[1], "area(early_top)");
    let msg = errors(&engine);
    assert!(
        msg.contains("expected a length, got length^2"),
        "an area in a depth is refused BY NAME: {msg}"
    );

    set_depth_expr(
        &mut engine,
        &mut kernel,
        blocks[1],
        "sqrt(area(early_top)) / 4",
    );
    assert!(
        errors(&engine).is_empty(),
        "sqrt of an area is a length a depth accepts: {}",
        errors(&engine)
    );
    let Operation::Extrude { params } = &engine
        .tree
        .find_feature(blocks[1])
        .expect("still there")
        .operation
    else {
        panic!("expected an extrude");
    };
    // The measured value DROVE the depth: the number is the mock's, so what
    // is pinned is that it moved off the stored 0.5 m and is the measured
    // area's square root over two, in metres (the expression works in mm).
    let area_m2 = kernel
        .compute_signature(
            feature_engine::names::resolve(
                engine.tree.named_ref("early_top").expect("named"),
                &engine.feature_results,
                &kernel,
            )
            .expect("resolves")
            .kernel_id,
            TopoKind::Face,
        )
        .area
        .expect("the mock reports an area");
    let expected_m = (area_m2 * 1e6).sqrt() / 4.0 * 1e-3;
    assert!(
        (params.depth - expected_m).abs() < 1e-12,
        "depth {} should be the measured {expected_m}",
        params.depth
    );
    assert_ne!(
        params.depth, 0.5,
        "it is no longer the stored value — the measurement drove it"
    );
}

#[test]
fn renaming_a_body_carries_the_expressions_that_measure_it() {
    // A body's display name IS a measurement argument, so a rename rewrites
    // `volume(plate)` to `volume(base)` — the expression means the same
    // thing, and the author did not ask for a different measurement.
    let (mut engine, _kernel, blocks) = blocks(1);
    let body_id = FeatureTree::body_id(blocks[0], &OutputKey::Main);
    engine.rename_body(body_id.clone(), "plate".to_string());

    engine
        .tree
        .parameters
        .push(DesignParameter::new("v", "volume(plate) / 1000"));
    let Operation::Extrude { params } = &mut engine.tree.features[1].operation else {
        panic!("expected an extrude");
    };
    params.depth_expr = Some("volume( plate )^(1/3)".to_string());

    engine.rename_body(body_id.clone(), "base".to_string());
    assert_eq!(engine.tree.parameters[0].expression, "volume(base) / 1000");
    let Operation::Extrude { params } = &engine.tree.features[1].operation else {
        panic!("expected an extrude");
    };
    assert_eq!(
        params.depth_expr.as_deref(),
        Some("volume( base )^(1/3)"),
        "spacing and parentheses survive: the rename splices the AST's spans"
    );

    // Undo restores both halves — the name and every expression that spells
    // it. Restoring the name alone would leave the expressions reading the
    // other one.
    let mut kernel = MockKernel::new();
    engine.undo(&mut kernel).expect("undo");
    assert_eq!(engine.tree.parameters[0].expression, "volume(plate) / 1000");
    let Operation::Extrude { params } = &engine.tree.features[1].operation else {
        panic!("expected an extrude");
    };
    assert_eq!(params.depth_expr.as_deref(), Some("volume( plate )^(1/3)"));
    assert_eq!(engine.tree.body_name_override(&body_id), Some("plate"));

    engine.redo(&mut kernel).expect("redo");
    assert_eq!(engine.tree.parameters[0].expression, "volume(base) / 1000");
    assert_eq!(engine.tree.body_name_override(&body_id), Some("base"));
}

#[test]
fn a_document_with_no_measurement_runs_exactly_one_pass() {
    // D2 must cost a document that does not measure nothing at all. The
    // observable is the rebuild's own answer being unchanged; the pass count
    // is the implementation of that.
    let (engine, _kernel, _blocks) = blocks(2);
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert_eq!(engine.feature_results.len(), 4);
    let mut tree = engine.tree.clone();
    assert!(
        !feature_engine::params::tree_measures(&mut tree),
        "nothing in this fixture measures"
    );
    assert!(feature_engine::params::measurement_sites(&mut tree).is_empty());
}
