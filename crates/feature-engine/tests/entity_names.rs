//! Entity names (N1, `specs/agent_mechanical_design.md` §5.2): a name is an
//! alias over a persistent reference, stored on the tree, keyed by the name,
//! and never GC'd.
//!
//! What is pinned here is the TABLE and its rules — the grammar, the
//! uniqueness refusal, the dotted body segment, what a name stores (a
//! `Selector::Pid`, never an index), undo, and what a name does when its
//! entity is gone. Whether a pid SURVIVES a rebuild is a different claim, and
//! the mock cannot make it (its ids are allocated per session, so a rebuild
//! renames everything); that oracle is
//! `crates/test-harness/tests/n1_entity_names.rs` against kernel-v2.

use std::collections::HashMap;

use feature_engine::names::{self, NamedRef, ResolvedBy};
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
        plane_face: None,
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

fn extrude(sketch_id: Uuid) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            profile_entity_ids: Some(vec![10, 11, 12, 13]),
            depth: 0.5,
            depth_expr: None,
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

/// An engine with one extruded square, plus the extrude's feature id.
fn one_block() -> (Engine, MockKernel, Uuid) {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let sketch = engine
        .add_feature(
            "Sketch".into(),
            Operation::Sketch {
                sketch: square_sketch(),
            },
            &mut kernel,
        )
        .expect("sketch");
    let block = engine
        .add_feature("Block".into(), extrude(sketch), &mut kernel)
        .expect("extrude");
    assert!(
        engine.errors.is_empty(),
        "the fixture must build: {:?}",
        engine.errors
    );
    (engine, kernel, block)
}

/// A reference to the block's positive end cap, the way a tool would author
/// one before it is turned into a name.
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

#[test]
fn a_name_stores_the_entity_s_persistent_id_not_its_index() {
    let (engine, kernel, block) = one_block();
    let named = mint(&engine, &kernel, &top_face_ref(block));

    let Selector::Pid { pid, root_pid } = named.target.selector else {
        panic!(
            "a name must store a persistent id, got {:?}",
            named.target.selector
        );
    };
    assert_ne!(pid, 0, "a pid is never 0");
    assert_eq!(named.kind, TopoKind::Face);
    // The pid target is Strict: it names one entity or none, so there is
    // nothing for a policy to relax.
    assert_eq!(named.target.policy, ResolvePolicy::Strict);
    // The authored reference is kept as the fallback, with the policy it was
    // AUTHORED with — that is what decides whether it may answer once the pid
    // is gone (N2 §5.3 item 1, settled 2026-10-03). See
    // `the_fallback_keeps_the_authored_policy_because_that_decides_the_rebind`.
    let fallback = named.fallback.as_ref().expect("the authored ref is kept");
    assert!(matches!(fallback.selector, Selector::Role { .. }));
    assert_eq!(fallback.policy, ResolvePolicy::BestEffort);
    // And it names the entity the authored reference named.
    let direct = feature_engine::resolve::resolve_geom_ref_live(
        &top_face_ref(block),
        &engine.feature_results,
        &kernel,
    )
    .expect("the authored ref resolves");
    let through_name =
        names::resolve(&named, &engine.feature_results, &kernel).expect("the name resolves");
    assert_eq!(through_name.kernel_id, direct.kernel_id);
    assert_eq!(through_name.resolved_by, ResolvedBy::Pid);
    assert_eq!(root_pid, pid, "a face's own root is its pid on the mock");
}

/// N2 §5.3 item 1, settled 2026-10-03: a stored reference's POLICY is what
/// decides whether its fallback may answer after the persistent id is gone, so
/// `pin_identity` must carry the authored policy into the fallback rather than
/// overwrite it. An agent's reference arrives `Strict` (`execute_tool` stamps
/// it), and a `Strict` reference refuses instead of rebinding by geometry; a
/// user's viewport pick arrives `BestEffort` and keeps the warned rebind.
///
/// The pinning RESOLVE is still Strict either way — a reference that does not
/// identify one entity is a thing to fix while the author is looking at it.
#[test]
fn the_fallback_keeps_the_authored_policy_because_that_decides_the_rebind() {
    let (engine, kernel, block) = one_block();

    let mut strict = top_face_ref(block);
    strict.policy = ResolvePolicy::Strict;
    let named = mint(&engine, &kernel, &strict);
    assert_eq!(
        named
            .fallback
            .as_ref()
            .expect("the authored ref is kept")
            .policy,
        ResolvePolicy::Strict,
        "an agent's reference stays Strict, so a lost identity refuses"
    );

    let named = mint(&engine, &kernel, &top_face_ref(block));
    assert_eq!(
        named
            .fallback
            .as_ref()
            .expect("the authored ref is kept")
            .policy,
        ResolvePolicy::BestEffort,
        "a user's pick stays BestEffort, so a lost identity rebinds and warns"
    );
}

#[test]
fn a_named_entity_is_reachable_by_name_from_the_tree() {
    let (mut engine, kernel, block) = one_block();
    let named = mint(&engine, &kernel, &top_face_ref(block));
    engine
        .set_entity_name("top_face", named, None)
        .expect("a bare name needs no body");

    let stored = engine
        .tree
        .named_ref("top_face")
        .expect("the tree holds the name");
    let resolved =
        names::resolve(stored, &engine.feature_results, &kernel).expect("the name resolves");
    assert!(
        kernel
            .entity_pid(resolved.kernel_id, TopoKind::Face)
            .is_some(),
        "the name resolved to a face the kernel knows"
    );
    assert_eq!(engine.tree.names.len(), 1);
}

#[test]
fn a_taken_name_is_refused_rather_than_rebound() {
    let (mut engine, kernel, block) = one_block();
    let named = mint(&engine, &kernel, &top_face_ref(block));
    engine
        .set_entity_name("top_face", named.clone(), None)
        .expect("first use");

    let err = engine
        .set_entity_name("top_face", named, None)
        .expect_err("the second use must be refused");
    match &err {
        EngineError::NameTaken { name } => assert_eq!(name, "top_face"),
        other => panic!("want NameTaken, got {other:?}"),
    }
    assert_eq!(
        ErrorKind::from(&err),
        ErrorKind::NameRefused {
            name: "top_face".to_string()
        }
    );
    assert_eq!(engine.tree.names.len(), 1, "nothing was replaced");
}

#[test]
fn an_illegal_name_is_invalid_name_and_stores_nothing() {
    let (mut engine, kernel, block) = one_block();
    let named = mint(&engine, &kernel, &top_face_ref(block));
    for bad in ["", "1face", "top face", "top-face", "a.b.c"] {
        match engine.set_entity_name(bad, named.clone(), None) {
            Err(EngineError::InvalidName { name, reason }) => {
                assert_eq!(name, bad);
                assert!(!reason.is_empty(), "{bad:?}: the refusal says why");
            }
            other => panic!("{bad:?}: want InvalidName, got {other:?}"),
        }
    }
    assert!(engine.tree.names.is_empty());
}

#[test]
fn a_dotted_name_must_name_the_body_the_entity_lives_in() {
    let (mut engine, kernel, block) = one_block();
    let named = mint(&engine, &kernel, &top_face_ref(block));

    // The right body: accepted, and the key is the dotted name.
    engine
        .set_entity_name("plate.top_face", named.clone(), Some("plate"))
        .expect("the body segment matches");
    assert!(engine.tree.names.contains_key("plate.top_face"));

    // Another body's name: refused, and the refusal says what the body IS.
    match engine.set_entity_name("bracket.top_face", named.clone(), Some("plate")) {
        Err(EngineError::InvalidName { reason, .. }) => {
            assert!(reason.contains("bracket"), "{reason}");
            assert!(reason.contains("plate"), "{reason}");
        }
        other => panic!("want InvalidName, got {other:?}"),
    }

    // A body display name that is not an identifier (the derived "Base plate")
    // cannot be a segment, and the refusal says so rather than inventing one.
    match engine.set_entity_name("Base.top_face", named, Some("Base plate")) {
        Err(EngineError::InvalidName { reason, .. }) => {
            assert!(reason.contains("Base plate"), "{reason}");
            assert!(reason.contains("rename the body"), "{reason}");
        }
        other => panic!("want InvalidName, got {other:?}"),
    }
    assert_eq!(engine.tree.names.len(), 1);
}

#[test]
fn a_dotted_name_with_no_known_body_is_refused_not_accepted_unchecked() {
    let (mut engine, kernel, block) = one_block();
    let named = mint(&engine, &kernel, &top_face_ref(block));
    match engine.set_entity_name("plate.top_face", named, None) {
        Err(EngineError::InvalidName { reason, .. }) => {
            assert!(reason.contains("cannot be checked"), "{reason}")
        }
        other => panic!("want InvalidName, got {other:?}"),
    }
    assert!(engine.tree.names.is_empty());
}

#[test]
fn unnaming_removes_it_and_an_unknown_name_is_name_not_found() {
    let (mut engine, kernel, block) = one_block();
    let named = mint(&engine, &kernel, &top_face_ref(block));
    engine
        .set_entity_name("top_face", named, None)
        .expect("named");

    engine.clear_entity_name("top_face").expect("unnamed");
    assert!(engine.tree.names.is_empty());

    match engine.clear_entity_name("top_face") {
        Err(EngineError::NameNotFound { name }) => assert_eq!(name, "top_face"),
        other => panic!("want NameNotFound, got {other:?}"),
    }
}

#[test]
fn naming_and_unnaming_are_each_one_undo_step() {
    let (mut engine, mut kernel, block) = one_block();
    let named = mint(&engine, &kernel, &top_face_ref(block));
    engine
        .set_entity_name("top_face", named, None)
        .expect("named");

    engine.undo(&mut kernel).expect("undo the name");
    assert!(engine.tree.names.is_empty(), "undo removed the name");
    engine.redo(&mut kernel).expect("redo the name");
    assert!(
        engine.tree.names.contains_key("top_face"),
        "redo put it back"
    );

    engine.clear_entity_name("top_face").expect("unnamed");
    engine.undo(&mut kernel).expect("undo the unname");
    assert!(
        engine.tree.names.contains_key("top_face"),
        "undo restored what the unname took"
    );
}

#[test]
fn a_name_whose_feature_is_deleted_stays_and_stops_resolving() {
    let (mut engine, mut kernel, block) = one_block();
    let named = mint(&engine, &kernel, &top_face_ref(block));
    engine
        .set_entity_name("top_face", named, None)
        .expect("named");

    engine
        .remove_feature(block, &mut kernel)
        .expect("delete the extrude");

    // §5.2: the name stays, so the agent sees the hole rather than losing the
    // record — and it resolves to nothing, loudly.
    let stored = engine
        .tree
        .named_ref("top_face")
        .expect("the name outlives the feature that introduced the entity");
    let err = names::resolve(stored, &engine.feature_results, &kernel)
        .expect_err("a deleted entity must not resolve");
    assert!(
        err.resolution_text().is_some(),
        "want a resolution refusal, got {err:?}"
    );
    // N2 §5.3 item 2: and it is classified, so an agent branches on the
    // reason instead of reading the sentence. The feature is gone, so is its
    // result, so there is nothing of the anchor left to look a pid up in —
    // `NoMatch`, not `PidGone`.
    assert_eq!(
        err.resolution_reason(),
        Some(&feature_engine::types::ResolutionReason::NoMatch),
        "got {err:?}"
    );
}

#[test]
fn the_name_table_round_trips_through_the_document_json() {
    let (mut engine, kernel, block) = one_block();
    let named = mint(&engine, &kernel, &top_face_ref(block));
    engine
        .set_entity_name("plate.top_face", named, Some("plate"))
        .expect("named");

    let json = serde_json::to_string(&engine.tree).expect("serialize");
    assert!(json.contains("\"names\""), "the table is written");
    assert!(json.contains("\"Pid\""), "and it carries a Pid selector");

    let back: FeatureTree = serde_json::from_str(&json).expect("deserialize");
    let stored = back
        .named_ref("plate.top_face")
        .expect("the name came back");
    assert_eq!(stored.kind, TopoKind::Face);
    assert!(matches!(stored.target.selector, Selector::Pid { .. }));
    assert!(stored.fallback.is_some());
}

#[test]
fn a_tree_with_no_names_writes_no_table_so_old_documents_are_unchanged() {
    let tree = FeatureTree::new();
    let json = serde_json::to_value(&tree).expect("serialize");
    assert!(
        json.get("names").is_none(),
        "an empty name table is not written: {json}"
    );
    // And a document without the key loads.
    let back: FeatureTree =
        serde_json::from_value(serde_json::json!({ "features": [], "active_index": null }))
            .expect("a pre-N1 tree loads");
    assert!(back.names.is_empty());
}
