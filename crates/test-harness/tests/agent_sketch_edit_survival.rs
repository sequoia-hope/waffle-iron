//! An agent's extrude must survive an edit to the sketch it stands on
//! (`specs/agent_mechanical_design.md` §10.3, the S3 notes' finding 3).
//!
//! `sketch_create` and `sketch_regions` offer the agent exactly ONE way to name
//! a profile: `profile_entity_ids`, the loop's bounding entity set. A fillet
//! adds an arc to that loop — {5,6,7,8} becomes {5,6,12,7,8} — so under v4
//! §2.9's equal-sets rule the set the extrude stored was no longer any loop's
//! set, and the feature that had been building correctly a moment before was
//! refused `ProfileNotFound`. The agent could author a plate and could round
//! its corner, but not both.
//!
//! Driven through the real tools (`wasm_bridge::execute_tool`) against
//! kernel-v2, so the oracle is the SOLID: the plate's volume before the fillet,
//! and after it the same volume less what rounding one corner removes,
//! r²(1 − π/4) × t. A rule that re-pointed the extrude at some OTHER loop would
//! pass a "no error" check and fail this one.

use std::f64::consts::PI;

use serde_json::{json, Value};
use test_harness::workflow::ModelBuilder;

const W: f64 = 0.060;
const H: f64 = 0.040;
const R: f64 = 0.004;
const T: f64 = 0.006;

fn point(id: u32, x: f64, y: f64) -> Value {
    json!({ "type": "Point", "id": id, "x": x, "y": y })
}

fn line(id: u32, start: u32, end: u32) -> Value {
    json!({ "type": "Line", "id": id, "start_id": start, "end_id": end })
}

/// A W × H rectangle: points 1–4 counter-clockwise from the origin, lines 5–8.
fn rectangle() -> Vec<Value> {
    vec![
        point(1, 0.0, 0.0),
        point(2, W, 0.0),
        point(3, W, H),
        point(4, 0.0, H),
        line(5, 1, 2),
        line(6, 2, 3),
        line(7, 3, 4),
        line(8, 4, 1),
    ]
}

/// The exact volume of the body the extrude built, through the kernel's own
/// integrator. `None` when the extrude produced no solid at all.
fn plate_volume(m: &ModelBuilder) -> Option<f64> {
    let handle = m.solid_handle("plate").ok()?;
    Some(
        m.kernel_ref()
            .as_measure()
            .mass_properties(&handle, None)
            .expect("mass properties of the plate")
            .volume,
    )
}

/// A plate authored the way an agent authors one, with its extrude addressing
/// the profile by the only name the agent is given.
fn plate() -> (ModelBuilder, String) {
    let mut m = ModelBuilder::kernel_v2();
    let created = m.agent_tool(
        "sketch_create",
        json!({
            "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0], "x_axis": [1.0, 0.0, 0.0] },
            "entities": rectangle(),
            "constraints": [],
        }),
    );
    assert!(!created.is_error, "sketch_create: {created:?}");
    let sketch_id = created.structured_content["feature_id"]
        .as_str()
        .expect("a sketch feature id")
        .to_string();

    let extruded = m.agent_tool(
        "feature_add",
        json!({ "operation": {
            "type": "Extrude",
            "params": {
                "sketch_id": sketch_id,
                "profile_index": 0,
                "profile_entity_ids": [5, 6, 7, 8],
                "depth": T,
                "symmetric": false,
                "cut": false,
            }
        }}),
    );
    assert!(!extruded.is_error, "feature_add(Extrude): {extruded:?}");
    m.name_feature_by_id(
        "plate",
        extruded.structured_content["feature_id"]
            .as_str()
            .expect("an extrude feature id"),
    );

    let v = plate_volume(&m).expect("the plate has a solid before the edit");
    let want = W * H * T;
    assert!(
        (v - want).abs() / want < 1e-9,
        "the unrounded plate measures {v:e} m³, not {want:e}"
    );
    (m, sketch_id)
}

#[test]
fn a_fillet_under_an_extrude_keeps_the_extrude() {
    let (mut m, sketch_id) = plate();

    // Round the corner at point 3 — the one between lines 6 and 7.
    let edited = m.agent_tool(
        "sketch_edit",
        json!({
            "feature_id": sketch_id,
            "ops": [{ "type": "Fillet", "corner": 3, "radius": R }],
        }),
    );
    assert!(
        !edited.is_error,
        "sketch_edit refused the fillet: {}",
        edited.structured_content["error"]
    );
    assert!(
        m.engine_errors().is_empty(),
        "the rebuild after the fillet reported {:?}",
        m.engine_errors()
    );

    // And the extrude is the ROUNDED plate — not the old footprint, and not
    // some other loop of the sketch.
    let v = plate_volume(&m).expect("the plate still has a solid after the edit");
    let want = (W * H - R * R * (1.0 - PI / 4.0)) * T;
    let unrounded = W * H * T;
    assert!(
        (v - want).abs() / want < 1e-4,
        "the rounded plate measures {v:e} m³ where the arithmetic says {want:e}; \
         the unrounded footprint is {unrounded:e}"
    );
}

/// The other half of the rule: an edit that REMOVES a named entity does not
/// get re-pointed. What is left of the author's loop may bound a larger region
/// nobody asked for, so the feature refuses rather than quietly building it.
#[test]
fn deleting_a_named_entity_still_refuses_loudly() {
    let (mut m, sketch_id) = plate();

    let edited = m.agent_tool(
        "sketch_edit",
        json!({
            "feature_id": sketch_id,
            "ops": [{ "type": "RemoveEntity", "id": 7 }],
        }),
    );
    // The op itself is legal; the extrude standing on the loop it opened is
    // not, and that is what must be loud — in the tool's refusal or in the
    // engine's errors, never in silence.
    let loud = edited.is_error || !m.engine_errors().is_empty();
    assert!(
        loud,
        "removing a named boundary entity left no error at all: {edited:?}"
    );
}
