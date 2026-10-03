//! Does a sketch's constraints actually reach the saved file?
//!
//! The question behind this file: **every one of the 1,172 sketches in the
//! 321-case assay corpus carries ZERO constraints**, and so does the
//! 26-sketch `docs/notes/planetary_gearbox/Planetary gearbox.waffle.json` —
//! a real document authored over the MCP (`specs/agent_mechanical_design.md`
//! §2.2 item 11). Three candidate causes: the writer drops them, the UI never
//! sends them, or nobody ever authored any.
//!
//! These tests measure the two machine-authored paths end to end — the
//! `sketch_create` tool and the UI's `FinishSketch` — through a real save and
//! a real load. Both carry constraints through intact, which leaves AUTHORING
//! as the cause: the corpus generator emits raw coordinates (`generator_version`
//! 4 in `app/tests/cases/assay/*.meta.json` has no constraint field at all),
//! and the gearbox was authored the same way. The sketches have no constraints
//! because none were ever written, and S4's sketch corpus is where constrained
//! ones come from.
//!
//! Keeping the measurement as a test, rather than as a note in a commit
//! message, is what stops the question being re-opened by the next reader of
//! that census.

use feature_engine::types::Operation;
use serde_json::{json, Value};
use waffle_types::kernel::MockKernel;
use waffle_types::{SketchConstraint, SketchEntity, SolveStatus};
use wasm_bridge::messages::{EngineToUi, LiveSketch, UiToEngine};
use wasm_bridge::*;

const FRONT_PLANE_ID: &str = "00000000-0000-0000-0000-000000000001";

fn point(id: u32, x: f64, y: f64) -> SketchEntity {
    SketchEntity::Point {
        id,
        x,
        y,
        construction: false,
    }
}

fn line(id: u32, start_id: u32, end_id: u32) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id,
        end_id,
        construction: false,
    }
}

/// A 2 × 3 rectangle, fully constrained: four H/V rails, a pin at the origin,
/// a width and a height — plus a REFERENCE width dimension, which must also
/// survive (it is the one kind of constraint a writer is tempted to drop,
/// because it does not drive).
fn constrained_rectangle() -> (Vec<SketchEntity>, Vec<SketchConstraint>) {
    let entities = vec![
        point(1, 0.0, 0.0),
        point(2, 1.0, 0.0),
        point(3, 1.0, 1.0),
        point(4, 0.0, 1.0),
        line(10, 1, 2),
        line(11, 2, 3),
        line(12, 3, 4),
        line(13, 4, 1),
    ];
    let constraints = vec![
        SketchConstraint::Horizontal { entity: 10 },
        SketchConstraint::Vertical { entity: 11 },
        SketchConstraint::Horizontal { entity: 12 },
        SketchConstraint::Vertical { entity: 13 },
        SketchConstraint::Pinned {
            point: 1,
            x: 0.0,
            y: 0.0,
        },
        SketchConstraint::HDistance {
            point_a: 1,
            point_b: 2,
            value: 2.0,
            expression: None,
            reference: false,
        },
        SketchConstraint::VDistance {
            point_a: 2,
            point_b: 3,
            value: 3.0,
            expression: None,
            reference: false,
        },
        SketchConstraint::HDistance {
            point_a: 1,
            point_b: 2,
            value: 9.0,
            expression: None,
            reference: true,
        },
    ];
    (entities, constraints)
}

/// Save the open document and load it back, as the host does.
fn save_and_reload(
    state: &mut EngineState,
    kernel: &mut MockKernel,
) -> file_format::WaffleDocument {
    let response = dispatch(state, UiToEngine::SaveDocument, kernel);
    let EngineToUi::SaveReady { json_data } = response else {
        panic!("SaveDocument did not answer with a file: {response:?}");
    };
    file_format::load_document(&json_data)
        .expect("the saved file loads")
        .document
}

/// Every sketch in a loaded document, in tree order.
fn sketches(doc: &file_format::WaffleDocument) -> Vec<waffle_types::Sketch> {
    doc.tabs
        .iter()
        .flat_map(|tab| match &tab.kind {
            file_format::TabKind::Part { features, .. } => features
                .features
                .iter()
                .filter_map(|f| match &f.operation {
                    Operation::Sketch { sketch } => Some(sketch.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect()
}

#[test]
fn a_sketch_authored_by_the_tool_keeps_its_constraints_through_a_save_and_load() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let (entities, constraints) = constrained_rectangle();

    let result = execute_tool(
        &mut state,
        &mut kernel,
        "sketch_create",
        &json!({
            "plane": { "anchor": { "type": "DatumPlane", "id": FRONT_PLANE_ID } },
            "entities": serde_json::to_value(&entities).unwrap(),
            "constraints": serde_json::to_value(&constraints).unwrap(),
        }),
        Some(&json!({ "agent_name": "persistence-test" })),
    );
    assert!(!result.is_error, "sketch_create failed: {result:?}");
    assert_eq!(
        result.structured_content["solve_status"], "FullyConstrained",
        "the fixture is fully constrained, and the tool says so: {:?}",
        result.structured_content
    );

    let doc = save_and_reload(&mut state, &mut kernel);
    let saved = sketches(&doc);
    assert_eq!(saved.len(), 1);
    assert_eq!(
        saved[0].constraints.len(),
        constraints.len(),
        "all eight constraints are in the file: {:?}",
        saved[0]
            .constraints
            .iter()
            .map(|c| c.kind())
            .collect::<Vec<_>>()
    );
    // The reference dimension too, with its flag and its value.
    let last = saved[0].constraints.last().expect("non-empty");
    assert!(
        last.is_reference(),
        "the driven dimension survived as driven"
    );
    assert_eq!(last.dimension_value(), Some(9.0));
    // And a driving dimension's value is the one that was authored.
    assert_eq!(saved[0].constraints[5].dimension_value(), Some(2.0));
}

#[test]
fn a_sketch_committed_the_way_the_ui_commits_keeps_its_constraints() {
    // The UI's path: BeginSketch, SolveSketch with the live state, then
    // FinishSketch carrying entities AND constraints. This is what
    // `store.svelte.js` `finishSketch` sends.
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let (entities, constraints) = constrained_rectangle();

    let begin = dispatch(
        &mut state,
        UiToEngine::BeginSketch {
            plane: waffle_types::GeomRef {
                kind: waffle_types::TopoKind::Face,
                anchor: waffle_types::Anchor::Datum {
                    datum_id: uuid::Uuid::nil(),
                },
                selector: waffle_types::Selector::Role {
                    role: waffle_types::Role::ProfileFace,
                    index: 0,
                },
                policy: waffle_types::ResolvePolicy::BestEffort,
                scope: None,
            },
        },
        &mut kernel,
    );
    assert!(
        !matches!(begin, EngineToUi::Error { .. }),
        "BeginSketch failed: {begin:?}"
    );

    let solved = dispatch(
        &mut state,
        UiToEngine::SolveSketch {
            entities: Some(entities.clone()),
            constraints: Some(constraints.clone()),
        },
        &mut kernel,
    );
    let EngineToUi::SketchSolved { solved } = solved else {
        panic!("SolveSketch did not answer: {solved:?}");
    };
    assert!(
        matches!(solved.status, SolveStatus::FullyConstrained),
        "status {:?}",
        solved.status
    );

    let finished = dispatch(
        &mut state,
        UiToEngine::FinishSketch {
            solved_positions: solved.positions.clone(),
            solved_profiles: solved.profiles.clone(),
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            plane_x_axis: None,
            entities,
            constraints: constraints.clone(),
            projected: Vec::new(),
            provenance: None,
        },
        &mut kernel,
    );
    assert!(
        !matches!(finished, EngineToUi::Error { .. }),
        "FinishSketch failed: {finished:?}"
    );

    let doc = save_and_reload(&mut state, &mut kernel);
    let saved = sketches(&doc);
    assert_eq!(saved.len(), 1);
    assert_eq!(
        saved[0].constraints.len(),
        constraints.len(),
        "the UI's commit path persists constraints too"
    );
}

#[test]
fn the_engine_applied_operations_constraints_are_persisted_too() {
    // S1's own addition: a fillet adds two `Tangent` constraints in the
    // engine. They must reach the file like any other, or a rounded corner
    // comes back as two lines and an unconstrained arc.
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let entities = vec![
        point(1, 0.0, 1.0),
        point(2, 0.0, 0.0),
        point(3, 1.0, 0.0),
        line(10, 1, 2),
        line(11, 2, 3),
    ];
    let mut solved_positions = std::collections::HashMap::new();
    for e in &entities {
        if let SketchEntity::Point { id, x, y, .. } = e {
            solved_positions.insert(*id, (*x, *y));
        }
    }

    let applied = dispatch(
        &mut state,
        UiToEngine::ApplySketchOps {
            live: LiveSketch {
                entities,
                constraints: Vec::new(),
                solved_positions,
                projected: Vec::new(),
                plane_origin: [0.0, 0.0, 0.0],
                plane_normal: [0.0, 0.0, 1.0],
                plane_x_axis: None,
            },
            ops: vec![waffle_types::SketchOp::Fillet {
                corner: 2,
                radius: 0.2,
            }],
            next_id: 0,
        },
        &mut kernel,
    );
    let EngineToUi::SketchOpsApplied {
        entities: after,
        constraints,
        ..
    } = applied
    else {
        panic!("the fillet was refused: {applied:?}");
    };
    assert_eq!(constraints.len(), 2, "two Tangents");

    // Commit it the way the UI does, then save and load.
    let result = execute_tool(
        &mut state,
        &mut kernel,
        "sketch_create",
        &json!({
            "plane": { "anchor": { "type": "DatumPlane", "id": FRONT_PLANE_ID } },
            "entities": serde_json::to_value(&after).unwrap(),
            "constraints": serde_json::to_value(&constraints).unwrap(),
        }),
        Some(&json!({ "agent_name": "persistence-test" })),
    );
    assert!(!result.is_error, "sketch_create failed: {result:?}");

    let doc = save_and_reload(&mut state, &mut kernel);
    let saved = sketches(&doc);
    assert_eq!(saved.len(), 1);
    let kinds: Vec<&str> = saved[0].constraints.iter().map(|c| c.kind()).collect();
    assert_eq!(kinds, vec!["Tangent", "Tangent"], "{kinds:?}");
}

#[test]
fn the_assay_corpus_sketches_have_no_constraints_because_none_were_authored() {
    // The census, as a test: the generator's own case files carry operations
    // described by profile TYPE and size — a rectangle, a star, a gear — and
    // no constraint anywhere. Nothing in the pipeline could have dropped what
    // was never written.
    //
    // Read from the repository so the claim stays true only as long as the
    // corpus does. The file is the generator's input record, not geometry.
    let meta = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../app/tests/cases/assay/C0001.meta.json"
    ))
    .expect("the corpus case record is in the tree");
    let meta: Value = serde_json::from_str(&meta).expect("valid JSON");
    assert!(
        meta["operations"].is_array(),
        "a case is a list of operations"
    );
    for op in meta["operations"].as_array().expect("an array") {
        assert!(
            op.get("constraints").is_none(),
            "a generated operation describes a profile, never a constraint: {op}"
        );
        assert!(
            op.get("profile_type").is_some(),
            "it names a profile type instead: {op}"
        );
    }
}
