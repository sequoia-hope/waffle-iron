//! The read-only agent tools (`specs/waffle_mcp_server.md` §2.5 Inspection),
//! ported from `app/src/lib/agent/queries.js`.
//!
//! `feature_get` reads the tree. The other four each wrap one engine message,
//! so in the page they were "send and reshape"; here the send is a direct
//! dispatch, and the reshaping is what had to move.

use std::collections::HashMap;

use modeling_ops::KernelBundle;
use serde_json::{json, Map, Value};
use waffle_types::{GearParams, SketchEntity};

use crate::engine_state::EngineState;
use crate::messages::{EngineToUi, UiToEngine};
use crate::tools::{engine_call, require_body, require_feature, unexpected, Answer, ToolFailure};

/// One feature's full definition, as `feature_edit` would take it back.
pub(super) fn feature_get(state: &EngineState, args: &Value) -> Answer {
    let feature = require_feature(state, args)?;
    let tree = &state.engine.tree;

    let mut out = json!({
        "feature_id": feature.id,
        "name": feature.name,
        "suppressed": feature.suppressed,
        "operation": serde_json::to_value(&feature.operation).unwrap_or(Value::Null),
        "provenance": tree
            .provenance
            .get(&feature.id)
            .map(|p| json!(p.origin))
            .unwrap_or_else(|| json!({ "type": "User" })),
    });
    if let Some((_, message)) = state.engine.errors.iter().find(|(id, _)| *id == feature.id) {
        out["error"] = json!(message);
    }
    Ok(out)
}

/// Volume, area, bounding box and topology counts of one body (ICR-1).
///
/// `method` is `exact` only when BOTH quantities were integrated from the
/// B-Rep; a mesh value is never presented as exact, and the kernel's reason
/// for falling back is reported verbatim.
pub(super) fn body_measure(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let body_id = args.get("body_id").and_then(Value::as_str).unwrap_or("");
    require_body(state, body_id)?;

    let response = engine_call(
        state,
        kb,
        "MeasureBody",
        UiToEngine::MeasureBody {
            body_id: body_id.to_string(),
        },
    )?;
    let EngineToUi::BodyMeasured {
        body_id,
        volume_m3,
        surface_area_m2,
        bbox_min,
        bbox_max,
        face_count,
        edge_count,
        vertex_count,
        closed,
    } = &response
    else {
        return Err(unexpected("MeasureBody", "BodyMeasured", &response));
    };

    let method_of = |m| serde_json::to_value(m).unwrap_or(Value::Null);
    let exact = matches!(volume_m3.method, crate::messages::MeasureMethod::Exact)
        && matches!(
            surface_area_m2.method,
            crate::messages::MeasureMethod::Exact
        );

    let mut out = json!({
        "body_id": body_id,
        "volume_m3": volume_m3.value,
        "surface_area_m2": surface_area_m2.value,
        "method": if exact { "exact" } else { "mesh" },
        "methods": {
            "volume": method_of(volume_m3.method),
            "surface_area": method_of(surface_area_m2.method),
        },
        "bbox_min": bbox_min,
        "bbox_max": bbox_max,
        "face_count": face_count,
        "edge_count": edge_count,
        "vertex_count": vertex_count,
        "closed": closed,
    });

    let mut unavailable = Map::new();
    if let Some(reason) = &volume_m3.exact_unavailable {
        unavailable.insert("volume".to_string(), json!(reason));
    }
    if let Some(reason) = &surface_area_m2.exact_unavailable {
        unavailable.insert("surface_area".to_string(), json!(reason));
    }
    if !unavailable.is_empty() {
        out["exact_unavailable"] = Value::Object(unavailable);
    }
    Ok(out)
}

/// The faces of one body, each with the `GeomRef` that names it (ICR-3).
pub(super) fn face_list(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let body_id = args.get("body_id").and_then(Value::as_str).unwrap_or("");
    require_body(state, body_id)?;

    // An unparseable filter is what the page would have watched the engine
    // refuse: the message never forms, so it is that same `Internal`.
    let filter = match args.get("filter") {
        None | Some(Value::Null) => None,
        Some(value) => Some(serde_json::from_value(value.clone()).map_err(|e| {
            ToolFailure::new(
                "Internal",
                format!("ListFaces failed: filter is not a TopoQuery: {e}"),
                json!({ "engine_error": { "kind": Value::Null, "message": e.to_string() } }),
            )
        })?),
    };

    let response = engine_call(
        state,
        kb,
        "ListFaces",
        UiToEngine::ListFaces {
            body_id: body_id.to_string(),
            filter,
        },
    )?;
    let EngineToUi::FacesListed { body_id, faces } = &response else {
        return Err(unexpected("ListFaces", "FacesListed", &response));
    };
    Ok(json!({ "body_id": body_id, "faces": faces }))
}

/// The minimal closed regions of one committed sketch.
pub(super) fn sketch_regions(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let feature = require_feature(state, args)?;
    let feature_id = feature.id;
    let feature_engine::types::Operation::Sketch { sketch } = &feature.operation else {
        let kind = serde_json::to_value(&feature.operation)
            .ok()
            .and_then(|v| v.get("type").and_then(Value::as_str).map(str::to_string));
        return Err(ToolFailure::new(
            "OperationKindMismatch",
            format!(
                "Feature {feature_id} is a {}, not a Sketch.",
                kind.clone().unwrap_or_else(|| "undefined".to_string())
            ),
            json!({ "expected": "Sketch", "got": kind }),
        ));
    };

    let entities = sketch.entities.clone();
    let solved = sketch.solved_positions.clone();
    let (entities, solved_positions) = region_inputs(state, kb, &entities, &solved)?;

    let response = engine_call(
        state,
        kb,
        "ComputeRegions",
        UiToEngine::ComputeRegions {
            entities,
            solved_positions,
            chord_tolerance: None,
        },
    )?;
    let EngineToUi::RegionsComputed { regions } = &response else {
        return Err(unexpected("ComputeRegions", "RegionsComputed", &response));
    };

    Ok(json!({
        "feature_id": feature_id,
        "regions": regions
            .iter()
            .map(|r| json!({ "profile_entity_ids": r.profile_entity_ids, "area_m2": r.area }))
            .collect::<Vec<_>>(),
    }))
}

/// Solved sketch-point positions, by point id.
type SolvedPositions = HashMap<u32, (f64, f64)>;

/// What a `ComputeRegions` call takes: the entities to arrange, and where
/// their points ended up.
type RegionInputs = (Vec<SketchEntity>, SolvedPositions);

/// The entities and point positions a `ComputeRegions` call takes for a
/// committed sketch (JS `regionInputs` + `sketchRegionsRequest`).
///
/// The solver's output is the authoritative coordinate source: a point's
/// raw `x`/`y` is pre-solve scratch and is used only when the point has no
/// solved entry yet. Gears are stored compactly and expanded here.
fn region_inputs(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    entities: &[SketchEntity],
    solved: &SolvedPositions,
) -> Result<RegionInputs, ToolFailure> {
    let mut out = Vec::new();
    let mut positions = HashMap::new();

    for entity in entities {
        if let SketchEntity::Gear { id, params, .. } = entity {
            for expanded in expand_gear(state, kb, *id, params)? {
                if let SketchEntity::Point { id, x, y, .. } = &expanded {
                    positions.insert(*id, (*x, *y));
                }
                out.push(expanded);
            }
        } else {
            if let SketchEntity::Point { id, x, y, .. } = entity {
                positions.insert(*id, solved.get(id).copied().unwrap_or((*x, *y)));
            }
            out.push(entity.clone());
        }
    }
    Ok((out, positions))
}

/// Per-gear id range of a completed sketch's expansion, distinct from the
/// range the active sketch editor uses (JS `inactiveGearIdBase`).
fn gear_id_base(entity_id: u32) -> u32 {
    50_000_000 + entity_id * 100_000
}

/// One `Gear` entity as the primitives it stands for, with every id shifted
/// into the gear's own range (JS `remapGearResponse`, the entities half — the
/// outline and positions it also builds are display data the regions do not
/// use).
fn expand_gear(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    entity_id: u32,
    params: &GearParams,
) -> Result<Vec<SketchEntity>, ToolFailure> {
    let response = engine_call(
        state,
        kb,
        "GenerateGearProfile",
        UiToEngine::GenerateGearProfile {
            params: params.clone(),
        },
    )?;
    let EngineToUi::GearProfileGenerated {
        entities,
        pitch_radius,
        ..
    } = &response
    else {
        return Err(unexpected(
            "GenerateGearProfile",
            "GearProfileGenerated",
            &response,
        ));
    };

    let base = gear_id_base(entity_id);
    let mut out: Vec<SketchEntity> = entities.iter().map(|e| remap(e, base)).collect();

    // The pitch circle: a construction reference on the gear's center, which
    // is always the first emitted point (external and internal both lead with
    // it).
    if let Some(first) = entities.first() {
        out.push(SketchEntity::Circle {
            id: base + 90_000,
            center_id: base + first.id(),
            radius: *pitch_radius,
            construction: true,
        });
    }
    Ok(out)
}

/// One entity with its own id and every id it references shifted by `base`.
fn remap(entity: &SketchEntity, base: u32) -> SketchEntity {
    match entity {
        SketchEntity::Point {
            id,
            x,
            y,
            construction,
        } => SketchEntity::Point {
            id: base + id,
            x: *x,
            y: *y,
            construction: *construction,
        },
        SketchEntity::Line {
            id,
            start_id,
            end_id,
            construction,
        } => SketchEntity::Line {
            id: base + id,
            start_id: base + start_id,
            end_id: base + end_id,
            construction: *construction,
        },
        SketchEntity::Circle {
            id,
            center_id,
            radius,
            construction,
        } => SketchEntity::Circle {
            id: base + id,
            center_id: base + center_id,
            radius: *radius,
            construction: *construction,
        },
        SketchEntity::Arc {
            id,
            center_id,
            start_id,
            end_id,
            construction,
        } => SketchEntity::Arc {
            id: base + id,
            center_id: base + center_id,
            start_id: base + start_id,
            end_id: base + end_id,
            construction: *construction,
        },
        SketchEntity::Spline {
            id,
            point_ids,
            construction,
        } => SketchEntity::Spline {
            id: base + id,
            point_ids: point_ids.iter().map(|p| base + p).collect(),
            construction: *construction,
        },
        SketchEntity::Gear {
            id,
            params,
            construction,
        } => SketchEntity::Gear {
            id: base + id,
            params: params.clone(),
            construction: *construction,
        },
    }
}

/// Evaluate one mm-space expression against the document's parameters.
pub(super) fn expression_evaluate(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let expression = args
        .get("expression")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let response = engine_call(
        state,
        kb,
        "EvaluateExpression",
        UiToEngine::EvaluateExpression {
            expression: expression.clone(),
        },
    )?;
    let EngineToUi::ExpressionEvaluated { value, error } = &response else {
        return Err(unexpected(
            "EvaluateExpression",
            "ExpressionEvaluated",
            &response,
        ));
    };

    let mut out = json!({ "expression": expression, "value_mm": value });
    if let Some(error) = error {
        out["error"] = json!(error);
    }
    Ok(out)
}
