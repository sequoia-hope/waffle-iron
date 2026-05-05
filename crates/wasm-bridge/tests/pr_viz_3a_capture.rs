//! PR-VIZ-3a sub-phase 0b — RED integration tests for the in-memory
//! Yang stage capture + WASM bridge API.
//!
//! Spec (the contract these tests pin down):
//!   `specs/yang_pr_viz_3a_in_memory_capture.md`
//!
//! Plan:
//!   `/home/claude/.claude/plans/reactive-juggling-sloth.md` PR-VIZ-3a §0b.
//!
//! On RED (current code, before sub-phase 0c lands), all tests in this
//! file MUST fail to compile because:
//!   - `EngineState::yang_debug_capture_enabled` field does not exist
//!     (spec §6).
//!   - `EngineState::yang_debug_captures` field does not exist (spec §6).
//!   - `wasm_bridge::set_yang_debug_capture` free fn does not exist
//!     (spec §5; implementer-o adds a native shim alongside the
//!     `#[wasm_bindgen]` export so the same API surface is testable from
//!     native code).
//!   - `wasm_bridge::get_yang_stages_json` free fn does not exist
//!     (spec §5).
//!   - `wasm_bridge::clear_yang_debug_captures` free fn does not exist
//!     (spec §5).
//!   - `kernel::FeatureStageCapture` does not exist (spec §2).
//!
//! On GREEN (after sub-phase 0c lands), all tests in this file MUST
//! pass without modification.
//!
//! Sibling test patterns:
//!   - Helper builders + `dispatch` invocation pattern from
//!     `crates/wasm-bridge/tests/bridge_tests.rs:107-237`. We reuse the
//!     `MockKernel`-backed dispatch so the test stays fast + deterministic;
//!     the assertion is on the capture-map contract (not on Yang probe
//!     data, which only fires under `WaffleKernel` + a boolean feature).
//!     The FIP §4.2 test plan's "non-empty stages" criterion is therefore
//!     covered by the kernel-side `test_yang_capture_round_trip` unit
//!     test (which exercises `record_stage` directly), not here.

use feature_engine::types::*;
use kernel::MockKernel;
use uuid::Uuid;
use waffle_types::*;
use wasm_bridge::messages::*;
use wasm_bridge::*;

// ── Helper builders (mirror bridge_tests.rs) ─────────────────────────────

/// A minimal sketch operation — single closed quad profile, fully
/// constrained, on an arbitrary plane. Mirrors `make_sketch_op` from
/// `bridge_tests.rs:10-71` so the dispatched feature exercises the same
/// path as the existing native tests.
fn make_sketch_op() -> Operation {
    let mut solved_positions = std::collections::HashMap::new();
    solved_positions.insert(1, (0.0, 0.0));
    solved_positions.insert(2, (1.0, 0.0));
    solved_positions.insert(3, (1.0, 1.0));
    solved_positions.insert(4, (0.0, 1.0));

    let sketch = Sketch {
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
        },
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        entities: vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 1.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 1.0,
                y: 1.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 1.0,
                construction: false,
            },
        ],
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions,
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        }],
    };
    Operation::Sketch { sketch }
}

// ── RED tests (will fail to compile until sub-phase 0c lands) ────────────

/// Spec §6 + §7: `EngineState` carries the capture-enabled flag and the
/// per-feature capture map. With capture enabled, dispatching an
/// `AddFeature` MUST insert an entry into `state.yang_debug_captures`
/// keyed by the just-added feature's id.
///
/// On RED: fails to compile (`yang_debug_capture_enabled` /
/// `yang_debug_captures` fields don't exist on `EngineState`).
/// On GREEN: passes — the dispatch hook from spec §7 inserts a
/// `FeatureStageCapture` (possibly with `stages.len() == 0` under
/// `MockKernel`) into the map.
#[test]
fn test_capture_enabled_dispatch_inserts_map_entry() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // Spec §6: default is false; explicitly arm.
    state.yang_debug_capture_enabled = true;
    assert!(
        state.yang_debug_captures.is_empty(),
        "fresh EngineState must start with an empty captures map (spec §6 default)"
    );

    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: make_sketch_op(),
        },
        &mut kernel,
    );

    // Sanity: the dispatch itself succeeded (mirrors
    // `bridge_tests.rs::dispatch_add_feature_returns_model_updated`).
    assert!(
        matches!(response, EngineToUi::ModelUpdated { .. }),
        "expected ModelUpdated response, got {:?}",
        std::mem::discriminant(&response)
    );

    // Spec §7: with capture enabled, exactly one entry is inserted into
    // the captures map per AddFeature dispatch (key = the just-added
    // feature_id as a String).
    assert_eq!(
        state.yang_debug_captures.len(),
        1,
        "spec §7 dispatch hook MUST insert one capture entry per AddFeature; got {}",
        state.yang_debug_captures.len()
    );

    // Spec §2: the inserted value is a `FeatureStageCapture` with
    // `feature_id` matching the map key.
    let (key, capture) = state
        .yang_debug_captures
        .iter()
        .next()
        .expect("captures map has 1 entry per the assert above");
    assert_eq!(
        &capture.feature_id, key,
        "spec §2: FeatureStageCapture.feature_id must equal the map key (Uuid string)"
    );
}

/// Spec §6 + §7: with capture DISABLED (the default), dispatching
/// `AddFeature` MUST NOT insert anything into the captures map. This is
/// the production-safety / probe-off-identity guarantee from spec §1
/// "coexists with the file dumps (both run when both gates are on)" —
/// when the capture gate is off, the dispatch hook short-circuits.
///
/// On RED: fails to compile (`yang_debug_capture_enabled` /
/// `yang_debug_captures` fields don't exist).
/// On GREEN: passes — the dispatch hook's `if state.yang_debug_capture_enabled`
/// guard prevents any insert.
#[test]
fn test_capture_disabled_dispatch_inserts_nothing() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // Spec §6: default-disabled. Verify and leave it that way.
    assert!(
        !state.yang_debug_capture_enabled,
        "spec §6: yang_debug_capture_enabled defaults to false"
    );

    let _ = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: make_sketch_op(),
        },
        &mut kernel,
    );

    assert!(
        state.yang_debug_captures.is_empty(),
        "spec §7: capture-off MUST NOT touch the captures map; got {} entries",
        state.yang_debug_captures.len()
    );
}

/// Spec §5: the three new WASM exports MUST also be callable from native
/// code (so the dispatch contract is testable without wasm-pack). Mirror
/// the `wasm_bridge::dispatch` re-export at `lib.rs:9`. The free fns
/// thread-locally route through the same plumbing as `process_message` on
/// wasm32.
///
/// On RED: fails to compile — `wasm_bridge::set_yang_debug_capture`,
/// `wasm_bridge::get_yang_stages_json`, `wasm_bridge::clear_yang_debug_captures`
/// don't exist as free fns yet.
/// On GREEN: passes — the implementer adds native shims with the same
/// signatures as the `#[wasm_bindgen]` exports.
#[test]
fn test_wasm_api_callable_from_native() {
    // Set + clear should be no-op safe (no panics) in fresh process state.
    wasm_bridge::set_yang_debug_capture(true);
    wasm_bridge::set_yang_debug_capture(false);
    wasm_bridge::clear_yang_debug_captures();

    // Spec §5: "null" return for absent feature_id (lookup miss).
    let json = wasm_bridge::get_yang_stages_json("00000000-0000-0000-0000-000000000000");
    assert_eq!(
        json, "null",
        "spec §5: get_yang_stages_json returns the literal `null` JSON for absent feature_id; got `{}`",
        json
    );
}

/// Spec §2 + §5 + §7: the JSON shape for a populated capture must be
/// `serde_json::to_string(&FeatureStageCapture)`. Round-trip the
/// dispatch-inserted capture through the JSON path to lock the wire
/// format that PR-VIZ-3b's app side will consume.
///
/// On RED: fails to compile (same fields/types as above).
/// On GREEN: passes — the inserted FeatureStageCapture serializes to
/// JSON with `feature_id`, `stages`, `failed_at_stage` keys.
#[test]
fn test_capture_serializes_to_spec_json_shape() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    state.yang_debug_capture_enabled = true;

    let _ = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: make_sketch_op(),
        },
        &mut kernel,
    );

    let (_, capture) = state
        .yang_debug_captures
        .iter()
        .next()
        .expect("dispatch with capture enabled inserts an entry");

    let json = serde_json::to_string(capture).expect("FeatureStageCapture must serde::Serialize");
    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("FeatureStageCapture JSON must round-trip");

    // Spec §2 schema: three top-level keys.
    assert!(
        parsed.get("feature_id").is_some(),
        "spec §2: JSON has `feature_id`; got {}",
        json
    );
    assert!(
        parsed.get("stages").is_some(),
        "spec §2: JSON has `stages`; got {}",
        json
    );
    assert!(
        parsed.get("failed_at_stage").is_some(),
        "spec §2: JSON has `failed_at_stage` (Option<usize>, present as null when None); got {}",
        json
    );
    assert!(
        parsed["stages"].is_array(),
        "spec §2: stages is Vec<StageMesh>; got {}",
        parsed["stages"]
    );
}
