//! v4 §2.10 (`specs/waffle_v4_document_model.md`): a sketch handed to the
//! engine without a solve — `solve_status` absent on the wire, or
//! `Unsolved` — is solved by the next rebuild, which writes the solution into
//! the entities and replaces the status. A tool never has to run the solver
//! itself, and a solve it could not have foreseen failing is reported loudly
//! rather than left `Unsolved`.

use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
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

fn hdist(a: u32, b: u32, value: f64) -> SketchConstraint {
    SketchConstraint::HDistance {
        point_a: a,
        point_b: b,
        value,
        expression: None,
        reference: false,
    }
}

/// A unit square with NO solved data and the given constraints, status
/// `Unsolved` — exactly what a solver-less writer emits.
fn unsolved_square(constraints: Vec<SketchConstraint>) -> Sketch {
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
        constraints,
        solve_status: SolveStatus::Unsolved,
        solved_positions: Default::default(),
        projected: vec![],
        solved_profiles: vec![],
    }
}

fn extrude(sketch_feature: Uuid) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id: sketch_feature,
            profile_index: 0,
            profile_entity_ids: None,
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

fn run(sketch: Sketch) -> (Engine, Uuid, Uuid) {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    // Bypass `add_feature` (which rebuilds immediately) so the tree holds the
    // raw, unsolved sketch when the ONE rebuild below runs.
    let sketch_feature = engine
        .tree
        .add_feature("Sketch".into(), Operation::Sketch { sketch });
    let extrude_feature = engine
        .tree
        .add_feature("Extrude".into(), extrude(sketch_feature));
    engine.rebuild_from_scratch(&mut kernel);
    (engine, sketch_feature, extrude_feature)
}

fn sketch_of(engine: &Engine, id: Uuid) -> &Sketch {
    let f = engine.tree.features.iter().find(|f| f.id == id).unwrap();
    let Operation::Sketch { sketch } = &f.operation else {
        panic!()
    };
    sketch
}

fn x_of(sketch: &Sketch, id: u32) -> f64 {
    sketch
        .entities
        .iter()
        .find_map(|e| match e {
            SketchEntity::Point { id: pid, x, .. } if *pid == id => Some(*x),
            _ => None,
        })
        .unwrap()
}

#[test]
fn rebuild_solves_an_unsolved_sketch_and_replaces_the_status() {
    // The constraint moves geometry: |x2 − x1| must become 2, not the 1 the
    // entities were written with — so a solve demonstrably ran.
    let (engine, sketch_id, extrude_id) = run(unsolved_square(vec![hdist(1, 2, 2.0)]));
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);

    let sketch = sketch_of(&engine, sketch_id);
    assert!(
        matches!(
            sketch.solve_status,
            SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
        ),
        "{:?}",
        sketch.solve_status
    );
    let dx = (x_of(sketch, 2) - x_of(sketch, 1)).abs();
    assert!((dx - 2.0).abs() < 1e-6, "|x2 - x1| = {dx}");

    // Derived data follows the solution, and the dependent extrude built on it.
    assert_eq!(sketch.solved_profiles.len(), 1);
    let mut ids = sketch.solved_profiles[0].entity_ids.clone();
    ids.sort_unstable();
    assert_eq!(ids, vec![10, 11, 12, 13]);
    assert!(engine.feature_results.contains_key(&extrude_id));
}

#[test]
fn an_unconstrained_unsolved_sketch_is_solved_in_place() {
    let (engine, sketch_id, extrude_id) = run(unsolved_square(vec![]));
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    let sketch = sketch_of(&engine, sketch_id);
    assert!(matches!(
        sketch.solve_status,
        SolveStatus::UnderConstrained { .. }
    ));
    assert_eq!(x_of(sketch, 2), 1.0, "nothing to move");
    assert!(engine.feature_results.contains_key(&extrude_id));
}

#[test]
fn a_contradictory_unsolved_sketch_is_reported_not_left_unsolved() {
    let (engine, sketch_id, _) = run(unsolved_square(vec![hdist(1, 2, 2.0), hdist(1, 2, 3.0)]));
    let sketch = sketch_of(&engine, sketch_id);
    assert!(
        matches!(
            sketch.solve_status,
            SolveStatus::OverConstrained { .. } | SolveStatus::SolveFailed { .. }
        ),
        "{:?}",
        sketch.solve_status
    );
    let (fid, msg) = engine
        .errors
        .iter()
        .find(|(fid, _)| *fid == sketch_id)
        .expect("a loud per-feature error on the sketch");
    assert_eq!(*fid, sketch_id);
    assert!(msg.contains("solve"), "{msg}");
}

#[test]
fn an_already_solved_sketch_is_left_alone() {
    // A FullyConstrained sketch whose constraint DISAGREES with its entity
    // positions: if the rebuild re-solved it, x2 would move. It must not —
    // only expression changes or an `Unsolved` status trigger a solve.
    let mut sketch = unsolved_square(vec![hdist(1, 2, 2.0)]);
    sketch.solve_status = SolveStatus::FullyConstrained;
    let (engine, sketch_id, _) = run(sketch);
    let sketch = sketch_of(&engine, sketch_id);
    assert_eq!(x_of(sketch, 2), 1.0);
    assert!(matches!(sketch.solve_status, SolveStatus::FullyConstrained));
}

#[test]
fn unsolved_is_the_wire_default_and_round_trips() {
    let raw = serde_json::json!({
        "id": Uuid::nil(),
        "plane": serde_json::to_value(&unsolved_square(vec![]).plane).unwrap(),
        "entities": [], "constraints": []
    });
    let sketch: Sketch = serde_json::from_value(raw).unwrap();
    assert!(matches!(sketch.solve_status, SolveStatus::Unsolved));

    let json = serde_json::to_value(&sketch).unwrap();
    assert_eq!(
        json["solve_status"],
        serde_json::json!({ "type": "Unsolved" })
    );
}
