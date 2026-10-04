//! S1 — the sketch operations over the bridge
//! (`specs/agent_mechanical_design.md` §10.1).
//!
//! The UI's trim, offset and fillet tools send these two messages now, so
//! what is pinned here is the contract they depend on: the ops answer with
//! the sketch AFTER the batch, the previews come from the same functions the
//! commit runs, and a refused operation is a typed error rather than an empty
//! success.

use std::collections::HashMap;

use waffle_types::{Side, SketchEntity, SketchOp};
use wasm_bridge::dispatch::dispatch;
use wasm_bridge::engine_state::EngineState;
use wasm_bridge::messages::{EngineToUi, LiveSketch, SketchQuery, SketchQueryResult, UiToEngine};

fn kb() -> kernel_v2::KernelV2Adapter {
    kernel_v2::KernelV2Adapter::new()
}

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

fn live(entities: Vec<SketchEntity>) -> LiveSketch {
    let mut solved = HashMap::new();
    for e in &entities {
        if let SketchEntity::Point { id, x, y, .. } = e {
            solved.insert(*id, (*x, *y));
        }
    }
    LiveSketch {
        entities,
        constraints: Vec::new(),
        solved_positions: solved,
        projected: Vec::new(),
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
    }
}

/// A unit square, lines 10..13 over points 1..4.
fn square() -> LiveSketch {
    live(vec![
        point(1, 0.0, 0.0),
        point(2, 1.0, 0.0),
        point(3, 1.0, 1.0),
        point(4, 0.0, 1.0),
        line(10, 1, 2),
        line(11, 2, 3),
        line(12, 3, 4),
        line(13, 4, 1),
    ])
}

/// An L with its corner at point 2.
fn elbow() -> LiveSketch {
    live(vec![
        point(1, 0.0, 1.0),
        point(2, 0.0, 0.0),
        point(3, 1.0, 0.0),
        line(10, 1, 2),
        line(11, 2, 3),
    ])
}

#[test]
fn applying_an_offset_answers_with_the_sketch_after_it() {
    let mut state = EngineState::new();
    let response = dispatch(
        &mut state,
        UiToEngine::ApplySketchOps {
            live: square(),
            ops: vec![SketchOp::Offset {
                chain: vec![10, 11, 12, 13],
                distance: 0.1,
                side: Side::Left,
            }],
            next_id: 100,
        },
        &mut kb(),
    );
    let EngineToUi::SketchOpsApplied {
        entities,
        edit,
        next_id,
        transient_constraints,
        ..
    } = response
    else {
        panic!("expected SketchOpsApplied, got {response:?}");
    };
    let lines = entities
        .iter()
        .filter(|e| matches!(e, SketchEntity::Line { .. }))
        .count();
    assert_eq!(lines, 8, "the square plus its inward offset");
    assert!(
        edit.added.iter().all(|e| e.id() >= 100),
        "the UI's id counter is a floor"
    );
    assert!(next_id > 100, "and it comes back advanced");
    assert!(transient_constraints.is_empty());
}

#[test]
fn applying_a_fillet_brings_back_its_tangent_constraints() {
    let mut state = EngineState::new();
    let response = dispatch(
        &mut state,
        UiToEngine::ApplySketchOps {
            live: elbow(),
            ops: vec![SketchOp::Fillet {
                corner: 2,
                radius: 0.2,
            }],
            next_id: 0,
        },
        &mut kb(),
    );
    let EngineToUi::SketchOpsApplied {
        entities,
        constraints,
        edit,
        ..
    } = response
    else {
        panic!("expected SketchOpsApplied, got {response:?}");
    };
    assert_eq!(
        entities
            .iter()
            .filter(|e| matches!(e, SketchEntity::Arc { .. }))
            .count(),
        1
    );
    assert_eq!(constraints.len(), 2, "a Tangent per line");
    assert!(edit.removed.contains(&2), "the corner point went");
}

#[test]
fn a_move_point_comes_back_as_a_transient_pin() {
    let mut state = EngineState::new();
    let response = dispatch(
        &mut state,
        UiToEngine::ApplySketchOps {
            live: square(),
            ops: vec![SketchOp::MovePoint {
                id: 3,
                to: [2.0, 2.0],
            }],
            next_id: 0,
        },
        &mut kb(),
    );
    let EngineToUi::SketchOpsApplied {
        constraints,
        transient_constraints,
        ..
    } = response
    else {
        panic!("expected SketchOpsApplied, got {response:?}");
    };
    assert!(constraints.is_empty(), "a drag stores no constraint");
    assert_eq!(
        transient_constraints.len(),
        1,
        "the solve still gets its pin"
    );
}

#[test]
fn a_refused_operation_is_an_error_and_not_an_empty_success() {
    let mut state = EngineState::new();
    let response = dispatch(
        &mut state,
        UiToEngine::ApplySketchOps {
            live: elbow(),
            ops: vec![SketchOp::Fillet {
                corner: 2,
                radius: 99.0,
            }],
            next_id: 0,
        },
        &mut kb(),
    );
    let EngineToUi::Error { message, .. } = response else {
        panic!("a fillet that does not fit must refuse, got {response:?}");
    };
    assert!(
        message.contains("fillet") && message.contains("99"),
        "the refusal says what and why: {message}"
    );
}

#[test]
fn the_chain_query_reports_the_connected_run_and_whether_it_closes() {
    let mut state = EngineState::new();
    let response = dispatch(
        &mut state,
        UiToEngine::QuerySketch {
            live: square(),
            query: SketchQuery::Chain {
                seed: 10,
                only_seed: false,
            },
        },
        &mut kb(),
    );
    let EngineToUi::SketchQueried {
        result: SketchQueryResult::Chain { ids, closed },
    } = response
    else {
        panic!("expected a Chain answer, got {response:?}");
    };
    assert_eq!(ids, vec![10, 11, 12, 13]);
    assert_eq!(closed, Some(true));
}

#[test]
fn the_chain_query_honours_the_alt_click_single_entity_case() {
    let mut state = EngineState::new();
    let response = dispatch(
        &mut state,
        UiToEngine::QuerySketch {
            live: square(),
            query: SketchQuery::Chain {
                seed: 10,
                only_seed: true,
            },
        },
        &mut kb(),
    );
    let EngineToUi::SketchQueried {
        result: SketchQueryResult::Chain { ids, closed },
    } = response
    else {
        panic!("expected a Chain answer, got {response:?}");
    };
    assert_eq!(ids, vec![10]);
    assert_eq!(closed, Some(false), "one line is an open run");
}

#[test]
fn the_trim_preview_brackets_the_cursor_between_two_cuts() {
    // A horizontal line crossed at x = -1 and x = 1; the cursor at the origin
    // is inside the middle piece.
    let mut state = EngineState::new();
    let sketch = live(vec![
        point(1, -3.0, 0.0),
        point(2, 3.0, 0.0),
        line(10, 1, 2),
        point(3, -1.0, -1.0),
        point(4, -1.0, 1.0),
        line(11, 3, 4),
        point(5, 1.0, -1.0),
        point(6, 1.0, 1.0),
        line(12, 5, 6),
    ]);
    let response = dispatch(
        &mut state,
        UiToEngine::QuerySketch {
            live: sketch,
            query: SketchQuery::TrimPreview {
                entity: 10,
                at: [0.0, 0.0],
            },
        },
        &mut kb(),
    );
    let EngineToUi::SketchQueried {
        result: SketchQueryResult::TrimPreview { start, end, cuts },
    } = response
    else {
        panic!("expected a TrimPreview, got {response:?}");
    };
    assert_eq!(cuts, 2);
    assert!((start[0] + 1.0).abs() < 1e-12, "{start:?}");
    assert!((end[0] - 1.0).abs() < 1e-12, "{end:?}");
}

#[test]
fn the_trim_preview_of_an_uncrossed_line_reports_no_cuts() {
    let mut state = EngineState::new();
    let response = dispatch(
        &mut state,
        UiToEngine::QuerySketch {
            live: live(vec![point(1, 0.0, 0.0), point(2, 1.0, 0.0), line(10, 1, 2)]),
            query: SketchQuery::TrimPreview {
                entity: 10,
                at: [0.5, 0.0],
            },
        },
        &mut kb(),
    );
    let EngineToUi::SketchQueried {
        result: SketchQueryResult::TrimPreview { cuts, .. },
    } = response
    else {
        panic!("expected a TrimPreview, got {response:?}");
    };
    assert_eq!(cuts, 0, "zero cuts means a trim takes the whole entity");
}

#[test]
fn the_fillet_preview_reports_the_arc_and_the_default_radius() {
    let mut state = EngineState::new();
    let response = dispatch(
        &mut state,
        UiToEngine::QuerySketch {
            live: elbow(),
            query: SketchQuery::FilletPreview {
                corner: 2,
                radius: None,
            },
        },
        &mut kb(),
    );
    let EngineToUi::SketchQueried {
        result:
            SketchQueryResult::FilletPreview {
                center,
                radius,
                default_radius,
                ..
            },
    } = response
    else {
        panic!("expected a FilletPreview, got {response:?}");
    };
    assert!(
        (default_radius - 1.0 / 3.0).abs() < 1e-12,
        "a third of the shorter leg: {default_radius}"
    );
    assert!(
        (radius - default_radius).abs() < 1e-12,
        "no radius ⇒ the default"
    );
    assert!(
        center[0] > 0.0 && center[1] > 0.0,
        "inside the corner: {center:?}"
    );
}

#[test]
fn a_fillet_preview_at_a_point_that_is_not_a_corner_is_refused() {
    let mut state = EngineState::new();
    let response = dispatch(
        &mut state,
        UiToEngine::QuerySketch {
            live: elbow(),
            query: SketchQuery::FilletPreview {
                corner: 1,
                radius: None,
            },
        },
        &mut kb(),
    );
    let EngineToUi::SketchQueried {
        result: SketchQueryResult::Refused { reason },
    } = response
    else {
        panic!("expected a refusal, got {response:?}");
    };
    assert!(reason.contains("corner"), "{reason}");
}

#[test]
fn the_offset_preview_derives_the_side_from_the_cursor() {
    let mut state = EngineState::new();
    let ask = |cursor: [f64; 2]| {
        let mut state2 = EngineState::new();
        let r = dispatch(
            &mut state2,
            UiToEngine::QuerySketch {
                live: square(),
                query: SketchQuery::OffsetPreview {
                    chain: vec![10, 11, 12, 13],
                    cursor: Some(cursor),
                    distance: None,
                    side: None,
                },
            },
            &mut kb(),
        );
        match r {
            EngineToUi::SketchQueried {
                result:
                    SketchQueryResult::OffsetPreview {
                        signed_distance, ..
                    },
            } => signed_distance.expect("a cursor gives a distance"),
            other => panic!("expected an OffsetPreview, got {other:?}"),
        }
    };
    // Inside the square the signed distance is positive (left of the CCW
    // traversal); outside it is negative.
    assert!(ask([0.5, 0.5]) > 0.0);
    assert!(ask([0.5, -0.5]) < 0.0);
    let _ = &mut state;
}

#[test]
fn an_offset_preview_with_a_typed_distance_draws_the_parallel_outline() {
    let mut state = EngineState::new();
    let response = dispatch(
        &mut state,
        UiToEngine::QuerySketch {
            live: square(),
            query: SketchQuery::OffsetPreview {
                chain: vec![10, 11, 12, 13],
                cursor: None,
                distance: Some(0.1),
                side: Some(Side::Left),
            },
        },
        &mut kb(),
    );
    let EngineToUi::SketchQueried {
        result:
            SketchQueryResult::OffsetPreview {
                polyline,
                closed,
                size,
                ..
            },
    } = response
    else {
        panic!("expected an OffsetPreview, got {response:?}");
    };
    assert!(closed);
    assert_eq!(size, 4);
    assert!(polyline.len() > 4, "a drawable outline: {polyline:?}");
    assert_eq!(polyline.first(), polyline.last(), "a closed loop");
}

#[test]
fn an_offset_preview_of_a_branching_selection_is_refused_by_tag() {
    let mut state = EngineState::new();
    let sketch = live(vec![
        point(1, -1.0, 0.0),
        point(2, 0.0, 0.0),
        point(3, 1.0, 0.0),
        point(4, 0.0, -1.0),
        line(10, 1, 2),
        line(11, 2, 3),
        line(12, 2, 4),
    ]);
    let response = dispatch(
        &mut state,
        UiToEngine::QuerySketch {
            live: sketch,
            query: SketchQuery::OffsetPreview {
                chain: vec![10, 11, 12],
                cursor: None,
                distance: Some(0.1),
                side: Some(Side::Left),
            },
        },
        &mut kb(),
    );
    let EngineToUi::SketchQueried {
        result: SketchQueryResult::Refused { reason },
    } = response
    else {
        panic!("expected a refusal, got {response:?}");
    };
    assert_eq!(reason, "branching", "the operation vocabulary's own tag");
}

#[test]
fn the_messages_round_trip_through_json() {
    // The UI sends these over `postMessage`; a field that cannot serialize
    // would only show up in the browser.
    let msg = UiToEngine::ApplySketchOps {
        live: square(),
        ops: vec![SketchOp::Trim {
            entity: 10,
            at: [0.5, 0.0],
        }],
        next_id: 7,
    };
    let json = serde_json::to_string(&msg).expect("serializes");
    assert!(json.contains("\"type\":\"ApplySketchOps\""));
    let back: UiToEngine = serde_json::from_str(&json).expect("deserializes");
    assert!(matches!(
        back,
        UiToEngine::ApplySketchOps { next_id: 7, .. }
    ));

    let query = UiToEngine::QuerySketch {
        live: square(),
        query: SketchQuery::OffsetPreview {
            chain: vec![10],
            cursor: Some([1.0, 2.0]),
            distance: None,
            side: None,
        },
    };
    let json = serde_json::to_string(&query).expect("serializes");
    let _back: UiToEngine = serde_json::from_str(&json).expect("deserializes");
}
