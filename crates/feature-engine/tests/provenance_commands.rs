//! Provenance inside the undo step (`specs/waffle_mcp_server.md` ICR-4, I3/I4/I5).
//!
//! An agent's feature carries `Agent{name}` provenance, and one undo of the
//! agent's call must restore the tree EXACTLY — including the provenance side
//! table. So the record travels with the `AddFeature` / `EditFeature` command:
//! undo removes (or restores) it, redo re-applies it. A record set outside a
//! command must not survive the undo of the feature it names.

use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;

const CUBE_STEP: &str = include_str!("../../step-import/tests/fixtures/cube.step");

fn cube_op(x: f64) -> Operation {
    let mut params = ImportedBodyParams::embedded("cube.step", CUBE_STEP);
    params.translation_m = [x, 0.0, 0.0];
    Operation::ImportedBody { params }
}

fn agent(name: &str) -> Provenance {
    Provenance {
        origin: ProvenanceOrigin::Agent {
            name: name.to_string(),
        },
        at: Some("2026-09-14T04:00:00Z".to_string()),
    }
}

/// The whole persisted tree, provenance table included.
fn snapshot(engine: &Engine) -> serde_json::Value {
    serde_json::to_value(&engine.tree).unwrap()
}

#[test]
fn add_with_provenance_is_recorded_undo_removes_it_redo_restores_it() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let before = snapshot(&engine);

    let prov = agent("claude-code");
    let id = engine
        .add_feature_with_provenance(
            "Import".to_string(),
            cube_op(0.0),
            Some(prov.clone()),
            &mut kernel,
        )
        .expect("adds");
    assert_eq!(engine.tree.provenance_of(id), Some(&prov));
    let after = snapshot(&engine);

    engine.undo(&mut kernel).unwrap();
    assert_eq!(snapshot(&engine), before, "undo restores the tree exactly");

    engine.redo(&mut kernel).unwrap();
    assert_eq!(
        engine.tree.provenance_of(id),
        Some(&prov),
        "redo restores it"
    );
    assert_eq!(snapshot(&engine), after);
}

#[test]
fn undo_of_a_plain_add_leaves_no_dangling_provenance() {
    // The import path used to record provenance AFTER the add, outside the
    // command: undo removed the feature and left an orphan record in the file.
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let before = snapshot(&engine);

    let id = engine
        .add_feature("Import".to_string(), cube_op(0.0), &mut kernel)
        .expect("adds");
    engine.set_provenance(id, Some(agent("late"))).unwrap();

    engine.undo(&mut kernel).unwrap();
    assert!(
        engine.tree.provenance.is_empty(),
        "orphan record: {:?}",
        engine.tree.provenance
    );
    assert_eq!(snapshot(&engine), before);
}

#[test]
fn edit_with_provenance_replaces_it_and_undo_restores_the_previous_record() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let source_id = Uuid::new_v4();
    let import = Provenance {
        origin: ProvenanceOrigin::Import { source_id },
        at: None,
    };
    let id = engine
        .add_feature_with_provenance(
            "Import".to_string(),
            cube_op(0.0),
            Some(import.clone()),
            &mut kernel,
        )
        .unwrap();
    let before_edit = snapshot(&engine);

    let prov = agent("claude-code");
    engine
        .edit_feature_with_provenance(id, cube_op(1.0), Some(prov.clone()), &mut kernel)
        .unwrap();
    assert_eq!(engine.tree.provenance_of(id), Some(&prov));
    let after_edit = snapshot(&engine);

    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.provenance_of(id), Some(&import));
    assert_eq!(
        snapshot(&engine),
        before_edit,
        "operation and record restored"
    );

    engine.redo(&mut kernel).unwrap();
    assert_eq!(snapshot(&engine), after_edit);
}

#[test]
fn edit_with_provenance_on_an_unrecorded_feature_undoes_to_no_record() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let id = engine
        .add_feature("Import".to_string(), cube_op(0.0), &mut kernel)
        .unwrap();
    let before_edit = snapshot(&engine);

    engine
        .edit_feature_with_provenance(id, cube_op(1.0), Some(agent("a")), &mut kernel)
        .unwrap();
    engine.undo(&mut kernel).unwrap();

    assert_eq!(engine.tree.provenance_of(id), None);
    assert_eq!(snapshot(&engine), before_edit);
}

#[test]
fn edit_without_provenance_keeps_the_existing_record() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let prov = agent("claude-code");
    let id = engine
        .add_feature_with_provenance(
            "Import".to_string(),
            cube_op(0.0),
            Some(prov.clone()),
            &mut kernel,
        )
        .unwrap();

    engine.edit_feature(id, cube_op(2.0), &mut kernel).unwrap();
    assert_eq!(engine.tree.provenance_of(id), Some(&prov));

    engine.undo(&mut kernel).unwrap();
    assert_eq!(engine.tree.provenance_of(id), Some(&prov));
}
