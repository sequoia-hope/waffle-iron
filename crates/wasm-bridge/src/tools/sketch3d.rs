//! `sketch3d_get` — read a 3D sketch's EVALUATED geometry
//! (`specs/sketch3d.md` S3).
//!
//! Authoring needs no tool of its own: a 3D sketch is a declarative operation,
//! so `feature_add` / `feature_edit` carry it like any other kind once
//! `Sketch3d` is authorable. (`sketch_create` exists as its own tool only
//! because a 2D sketch orchestrates four engine messages and builds a profile
//! payload between the solve and the commit; a 3D sketch does none of that.)
//!
//! Reading it back DOES need a tool. The resolved coordinates and the chains
//! are not in the document — a point can be attached to model geometry, which
//! only resolves during the rebuild walk — so `feature_get` returns the
//! declaration and nothing else. This is the only way to ask what the sketch
//! actually came out as: where an attached point landed, how many chains the
//! graph split into, and which joints are tangent (which is what decides a
//! mitre from a smooth bend when a sweep consumes it).

use serde_json::{json, Value};
use waffle_types::sketch3d::{Chain3d, Edge3dKind};

use crate::engine_state::EngineState;
use crate::tools::{require_feature, Answer, ToolFailure};

pub(super) fn sketch3d_get(state: &mut EngineState, args: &Value) -> Answer {
    let feature = require_feature(state, args)?;
    let feature_id = feature.id;
    let feature_engine::types::Operation::Sketch3d { sketch } = &feature.operation else {
        let kind = serde_json::to_value(&feature.operation)
            .ok()
            .and_then(|v| v.get("type").and_then(Value::as_str).map(str::to_string));
        return Err(ToolFailure::new(
            "OperationKindMismatch",
            format!(
                "Feature {feature_id} is a {}, not a Sketch3d.",
                kind.clone().unwrap_or_else(|| "undefined".to_string())
            ),
            json!({ "expected": "Sketch3d", "got": kind }),
        ));
    };
    let entity_count = sketch.entities.len();

    // The evaluation lives beside the rebuild, not in the document.
    let Some(ev) = state.engine.sketch3d.get(&feature_id) else {
        return Err(ToolFailure::new(
            "NotEvaluated",
            format!(
                "Feature {feature_id} has no evaluated geometry — it is suppressed, rolled \
                 back, or its last rebuild failed (see its error)."
            ),
            json!({ "feature_id": feature_id }),
        ));
    };

    Ok(json!({
        "feature_id": feature_id,
        "entity_count": entity_count,
        "points": ev
            .resolved
            .iter()
            .map(|(id, p)| json!({ "id": id, "xyz": p }))
            .collect::<Vec<_>>(),
        "chains": ev.chains.iter().map(chain_json).collect::<Vec<_>>(),
        // A best-effort attachment that re-bound onto the NEAREST entity
        // after the geometry moved says so here; an agent reading the points
        // needs to know one of them landed somewhere other than the pick.
        "warnings": ev.warnings,
    }))
}

fn chain_json(c: &Chain3d) -> Value {
    json!({
        "closed": c.closed,
        "length_m": c.length(),
        // `g1[i]` is the joint between edge i and i+1 (and, when closed, the
        // wrap-around joint last) — a sweep reads it to choose a smooth join
        // over a mitre.
        "tangent_joints": c.g1,
        "edges": c.edges.iter().map(|e| match e.kind {
            Edge3dKind::Line => json!({
                "type": "Line",
                "entity_id": e.entity_id,
                "a": e.a,
                "b": e.b,
                "length_m": e.length(),
            }),
            Edge3dKind::Arc { center, normal, radius } => json!({
                "type": "Arc",
                "entity_id": e.entity_id,
                "a": e.a,
                "b": e.b,
                "center": center,
                "normal": normal,
                "radius_m": radius,
                "length_m": e.length(),
            }),
        }).collect::<Vec<_>>(),
    })
}
