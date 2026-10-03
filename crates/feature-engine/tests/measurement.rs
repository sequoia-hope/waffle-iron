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

/// A SIDE face of an extruded block. Its mock area is `width × depth`, so
/// unlike the end cap it MOVES when the block's own depth moves — which is
/// what lets a chain of measurements be built and its pass count measured.
fn side_face_ref(block: Uuid) -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: block,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::SideFace { index: 0 },
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
        msg.contains("\"Block 1\" (#4 of the tree)")
            && msg.contains("\"Block 0\" (#2 of the tree)"),
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
    assert_eq!(
        engine.measure_passes, 1,
        "one pass, and the measuring loop was never entered"
    );
}

/// Name one block's first side face, so an expression can measure it.
fn name_side(engine: &mut Engine, kernel: &MockKernel, block: Uuid, name: &str) {
    let named = mint(engine, kernel, &side_face_ref(block));
    engine
        .set_entity_name(name, named, None)
        .unwrap_or_else(|e| panic!("naming {name}: {e:?}"));
}

/// Set one design parameter's expression and rebuild through a no-op edit,
/// so the engine's own rebuild path (and its pass counter) runs.
fn set_param(
    engine: &mut Engine,
    kernel: &mut MockKernel,
    name: &str,
    expression: &str,
    reader: Uuid,
) {
    engine
        .tree
        .parameters
        .push(DesignParameter::new(name, expression));
    set_depth_expr(engine, kernel, reader, name);
}

#[test]
fn a_parameter_measuring_an_earlier_feature_is_legal_and_costs_one_extra_pass() {
    // §6's legal direction: a parameter that measures feature #1 and drives
    // feature #3. The first pass cannot know the measured number (the
    // geometry is not built when the parameter pass runs), so the loop runs
    // a SECOND pass — and a third is not needed, because the value it
    // measured on the second pass is the same one.
    let (mut engine, mut kernel, blocks) = blocks(3);
    name_side(&mut engine, &kernel, blocks[0], "first_side");
    set_param(
        &mut engine,
        &mut kernel,
        "lift",
        "sqrt(area(first_side))",
        blocks[2],
    );
    assert!(errors(&engine).is_empty(), "{}", errors(&engine));
    assert_eq!(
        engine.measure_passes, 2,
        "one site, one extra pass — not the sites+1 worst case"
    );
    let Operation::Extrude { params } = &engine
        .tree
        .find_feature(blocks[2])
        .expect("still there")
        .operation
    else {
        panic!("expected an extrude");
    };
    assert_ne!(params.depth, 0.5, "the measurement drove the depth");
}

#[test]
fn a_parameter_measuring_a_later_feature_is_refused_at_its_earliest_reader() {
    // The illegal direction, and the reason a parameter needs
    // `earliest_readers`: the parameter has no index of its own, so the rule
    // is stated where it is READ. Measuring feature #5 while feature #3
    // reads it would make each rebuild's answer depend on the previous
    // one's.
    let (mut engine, mut kernel, blocks) = blocks(3);
    name_side(&mut engine, &kernel, blocks[2], "last_side");
    set_param(
        &mut engine,
        &mut kernel,
        "drop",
        "sqrt(area(last_side))",
        blocks[1],
    );
    let msg = errors(&engine);
    assert!(
        msg.contains("circular measurement") && msg.contains("built AFTER"),
        "a parameter measuring a later feature is the same refusal: {msg}"
    );
    assert!(
        msg.contains("\"Block 2\"") && msg.contains("\"Block 1\""),
        "both features named: {msg}"
    );
}

#[test]
fn two_chained_measuring_fields_settle_at_the_sites_plus_one_bound() {
    // The worst case the bound exists for, built on purpose. Block 1's depth
    // measures block 0; block 2's depth measures BLOCK 1 — whose side-face
    // area is width × that very depth — so block 2's number cannot be right
    // until block 1's has landed. The chain settles one link per pass: three
    // passes for two sites, which is exactly sites + 1.
    let (mut engine, mut kernel, blocks) = blocks(3);
    name_side(&mut engine, &kernel, blocks[0], "a_side");
    name_side(&mut engine, &kernel, blocks[1], "b_side");
    set_depth_expr(
        &mut engine,
        &mut kernel,
        blocks[1],
        "sqrt(area(a_side)) * 2",
    );
    set_depth_expr(
        &mut engine,
        &mut kernel,
        blocks[2],
        "sqrt(area(b_side)) * 2",
    );
    assert!(errors(&engine).is_empty(), "{}", errors(&engine));
    assert_eq!(
        feature_engine::params::measurement_sites(&mut engine.tree.clone()).len(),
        2,
        "two sites: the two depths"
    );
    // Editing the two depths one at a time already settled the first link,
    // so the INCREMENTAL rebuild had only the second left to do.
    assert_eq!(
        engine.measure_passes, 2,
        "an incremental rebuild has only the unsettled link left"
    );

    // The COLD case is the one the bound is about: put both driven depths
    // back to a stale value and rebuild everything. Now neither link is
    // settled, and the chain can only settle one link per pass.
    for block in [blocks[1], blocks[2]] {
        let Some(feature) = engine.tree.find_feature_mut(block) else {
            panic!("still there");
        };
        if let Operation::Extrude { params } = &mut feature.operation {
            params.depth = 0.5;
        }
    }
    engine.rebuild_from_scratch(&mut kernel);
    assert!(errors(&engine).is_empty(), "{}", errors(&engine));
    assert_eq!(
        engine.measure_passes, 3,
        "a cold two-link chain settles one link per pass, at the sites+1 bound"
    );
    // And the second link really did read the first link's geometry: block
    // 2's depth is derived from block 1's driven depth, not from the stored
    // 0.5 m either block started at.
    let depth_of = |feature: Uuid| {
        let Operation::Extrude { params } = &engine
            .tree
            .find_feature(feature)
            .expect("still there")
            .operation
        else {
            panic!("expected an extrude");
        };
        params.depth
    };
    assert_ne!(depth_of(blocks[1]), 0.5);
    assert_ne!(depth_of(blocks[2]), 0.5);
    assert_ne!(
        depth_of(blocks[1]),
        depth_of(blocks[2]),
        "the two links measure different faces"
    );
}

#[test]
fn fifteen_independent_measuring_sites_cost_two_passes_not_sixteen() {
    // The performance question: `sites + 1` is the BOUND, not the cost. A
    // document whose measurements do not feed each other settles all of them
    // in one measuring pass however many there are, because every one of
    // them reads geometry that the first build already produced.
    //
    // Nine feature fields and six parameters, fifteen sites over a
    // twenty-feature tree, all reading block 0.
    let (mut engine, mut kernel, blocks) = blocks(10);
    assert_eq!(engine.tree.features.len(), 20);
    name_side(&mut engine, &kernel, blocks[0], "datum_side");
    for (k, block) in blocks.iter().enumerate().skip(1) {
        let expression = if k <= 6 {
            // A measuring PARAMETER plus a measuring field, so both kinds of
            // site are in the count.
            engine.tree.parameters.push(DesignParameter::new(
                format!("p{k}"),
                "sqrt(area(datum_side))",
            ));
            format!("sqrt(area(datum_side)) + p{k} * 0")
        } else {
            "sqrt(area(datum_side))".to_string()
        };
        set_depth_expr(&mut engine, &mut kernel, *block, &expression);
    }
    assert!(errors(&engine).is_empty(), "{}", errors(&engine));
    assert_eq!(
        feature_engine::params::measurement_sites(&mut engine.tree.clone()).len(),
        15,
        "nine depths and six parameters"
    );
    assert_eq!(
        engine.measure_passes, 2,
        "fifteen independent sites settle in ONE measuring pass, not fifteen"
    );
}
