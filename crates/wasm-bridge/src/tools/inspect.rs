//! The read-only agent tools (`specs/waffle_mcp_server.md` §2.5 Inspection),
//! ported from `app/src/lib/agent/queries.js`.
//!
//! `feature_get` reads the tree. The other four each wrap one engine message,
//! so in the page they were "send and reshape"; here the send is a direct
//! dispatch, and the reshaping is what had to move.

use std::collections::HashMap;

use modeling_ops::KernelBundle;
use serde_json::{json, Map, Value};
use waffle_types::{generated_entity_id_base, GearParams, SketchEntity, SprocketParams};

use feature_engine::expr::Dimension;

use crate::engine_state::EngineState;
use crate::messages::{EngineToUi, UiToEngine};
use crate::tools::{engine_call, require_body, require_feature, unexpected, Answer, ToolFailure};

/// One feature's full definition, as `feature_edit` would take it back.
///
/// Plus the state of its REFERENCES (N2 §5.3 item 4). An agent cannot see a
/// warning toast, so every reference resolution the last rebuild did for this
/// feature is reported here: `warnings` carries what the resolver said
/// verbatim, and `references` says whether each reference the feature stores
/// still answers, through which rung, and — when it does not — the typed
/// reason. Without this a reference that rebound by geometry was visible only
/// in the user's UI.
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
    // Last wins, like `model_summary`'s map and the authoring delta: if the
    // engine reported an id twice, every tool shows the same message.
    if let Some((_, message)) = state
        .engine
        .errors
        .iter()
        .rev()
        .find(|(id, _)| *id == feature.id)
    {
        out["error"] = json!(message);
    }
    // The typed class of that error, when it has one — so an agent branches on
    // `ResolutionFailed`'s reason instead of reading the sentence above.
    if let Some(e) = state
        .engine
        .feature_errors
        .iter()
        .rev()
        .find(|e| e.feature_id == feature.id)
    {
        out["error_kind"] = serde_json::to_value(&e.kind).unwrap_or(Value::Null);
    }
    let warnings: Vec<&String> = state
        .engine
        .feature_warnings
        .iter()
        .filter(|(id, _)| *id == feature.id)
        .map(|(_, w)| w)
        .collect();
    if !warnings.is_empty() {
        out["warnings"] = json!(warnings);
    }
    let references = reference_state(state, feature);
    if !references.is_empty() {
        out["references"] = json!(references);
    }
    Ok(out)
}

/// The resolution state of each reference this feature STORES, as of the last
/// rebuild (N2 §5.3 item 4).
///
/// Only references the engine can re-resolve without re-running the operation
/// are listed. Today that is a sketch's pinned plane face, which is the one
/// §5.3 item 3 added and the one whose failure stops the feature. A boolean's
/// `targets` resolve inside the operation against state the rebuild has moved
/// on from, so reporting them here would mean re-resolving against DIFFERENT
/// geometry and calling the answer the feature's — a worse lie than saying
/// nothing. Their outcome reaches the agent as the feature's error.
///
/// `resolves`, `resolved_via`, `rebound` and the typed reasons are the
/// REBUILD's own record (`Engine::feature_references`), not something inferred
/// here: `feature_get` has no kernel, and the previous cut of this read
/// inferred `resolves` from "did the feature fail?" — which reported a
/// perfectly good plane face as refused whenever the sketch failed for some
/// other reason (its x-axis, say), and could never say which rung answered.
fn reference_state(state: &EngineState, feature: &feature_engine::types::Feature) -> Vec<Value> {
    let feature_engine::types::Operation::Sketch { sketch } = &feature.operation else {
        return Vec::new();
    };
    let Some(face) = &sketch.plane_face else {
        return Vec::new();
    };
    let mut entry = json!({
        "role": feature_engine::rebuild::SKETCH_PLANE_FACE_ROLE,
        "kind": face.target.kind,
        "geom_ref": face.target,
        "recorded_signature": face.signature,
    });
    match state
        .engine
        .feature_references
        .iter()
        .find(|(id, r)| {
            *id == feature.id && r.role == feature_engine::rebuild::SKETCH_PLANE_FACE_ROLE
        })
        .map(|(_, r)| r)
    {
        Some(record) => {
            entry["resolves"] = json!(record.resolves);
            if let Some(via) = record.via {
                entry["resolved_via"] = serde_json::to_value(via).unwrap_or(Value::Null);
            }
            if record.rebound {
                entry["rebound"] = json!(true);
            }
            if let Some(reason) = &record.lost_identity {
                entry["lost_identity"] = serde_json::to_value(reason).unwrap_or(Value::Null);
            }
            if let Some(reason) = &record.refusal {
                entry["refusal"] = serde_json::to_value(reason).unwrap_or(Value::Null);
            }
        }
        // The rebuild never reached this reference — the feature failed
        // earlier, or it has not been rebuilt in this session. Say that
        // rather than answer for it.
        None => {
            if let Some(e) = state
                .engine
                .feature_errors
                .iter()
                .rev()
                .find(|e| e.feature_id == feature.id)
            {
                entry["blocked_by"] = json!(e.message);
            }
        }
    }
    if let Some(e) = state
        .engine
        .feature_errors
        .iter()
        .rev()
        .find(|e| e.feature_id == feature.id)
    {
        entry["message"] = json!(e.message);
    }
    vec![entry]
}

/// What a linked KiCad board knows about a body or an instance
/// (`specs/kicad_board_link.md` C4): the board record, the component record
/// (reference, value, footprint, datasheet, side, pads with nets) and the
/// source. All `null` for anything that derives from no board.
pub(super) fn entity_meta(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let body_id = args
        .get("body_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let instance_path = match args.get("instance_path") {
        Some(v) if !v.is_null() => Some(serde_json::from_value(v.clone()).map_err(|e| {
            ToolFailure::new(
                "InvalidArguments",
                format!("instance_path: {e}"),
                json!({ "reason": e.to_string() }),
            )
        })?),
        _ => None,
    };
    if body_id.is_none() && instance_path.is_none() {
        return Err(ToolFailure::new(
            "InvalidArguments",
            "body_id or instance_path is required.",
            json!({ "reason": "body_id or instance_path is required." }),
        ));
    }
    let response = engine_call(
        state,
        kb,
        "QueryEntityMeta",
        UiToEngine::QueryEntityMeta {
            body_id,
            instance_path,
        },
    )?;
    let EngineToUi::EntityMeta {
        board,
        component,
        source,
    } = response
    else {
        return Err(unexpected("QueryEntityMeta", "EntityMeta", &response));
    };
    Ok(json!({
        "board": board,
        "component": component,
        "source": source,
    }))
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
    let body_id = require_body(
        state,
        args.get("body_id").and_then(Value::as_str).unwrap_or(""),
    )?;
    let body_id = body_id.as_str();

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

/// The minimum distance between two operands, and the closest point on each
/// (Q1 of `specs/agent_mechanical_design.md` §4.2/§4.3).
///
/// `method` is `exact` only when the kernel certified the number analytically;
/// otherwise it is `mesh` and `chord_bound_m` is the band the true value lies
/// within. `along` asks for the gap along a direction instead of the minimum
/// distance, and comes back negative when the operands overlap along it.
pub(super) fn measure_distance(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let operand = |name: &str| -> Result<crate::messages::MeasureOperand, ToolFailure> {
        let value = args.get(name).ok_or_else(|| {
            ToolFailure::new(
                "InvalidArguments",
                format!("{name} is required."),
                json!({ "reason": format!("{name} is required.") }),
            )
        })?;
        serde_json::from_value(value.clone()).map_err(|e| {
            ToolFailure::new(
                "InvalidArguments",
                format!(
                    "{name}: {e}. An operand is {{\"type\":\"body\",\"body_id\":…}}, \
                     {{\"type\":\"entity\",\"geom_ref\":…}} or \
                     {{\"type\":\"point\",\"point\":[x,y,z]}}."
                ),
                json!({ "reason": e.to_string() }),
            )
        })
    };
    let (a, b) = (operand("a")?, operand("b")?);
    let along = match args.get("along") {
        None | Some(Value::Null) => None,
        Some(value) => Some(serde_json::from_value(value.clone()).map_err(|e| {
            ToolFailure::new(
                "InvalidArguments",
                format!("along: {e}. A direction is [x, y, z]."),
                json!({ "reason": e.to_string() }),
            )
        })?),
    };

    let response = engine_call(
        state,
        kb,
        "MeasureDistance",
        UiToEngine::MeasureDistance { a, b, along },
    )?;
    let EngineToUi::DistanceMeasured {
        value_m,
        method,
        chord_bound_m,
        points,
        on,
    } = &response
    else {
        return Err(unexpected("MeasureDistance", "DistanceMeasured", &response));
    };

    let exact = matches!(method, crate::messages::MeasureMethod::Exact);
    let mut out = json!({
        "distance_m": value_m,
        "method": if exact { "exact" } else { "mesh" },
        "points": points,
        "on": on,
    });
    if !exact {
        out["chord_bound_m"] = json!(chord_bound_m);
    }
    if along.is_some() {
        out["along"] = json!(along);
    }
    Ok(out)
}

/// Whether two bodies collide, touch, or are apart (Q2 of
/// `specs/agent_mechanical_design.md` §4.2/§4.3).
///
/// A boolean the kernel cannot run is an ERROR here, never a `disjoint`
/// answer — the whole point of the tool is to catch a collision, so "could
/// not tell" must not read as "no collision".
pub(super) fn measure_interference(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let body = |name: &str| -> Result<String, ToolFailure> {
        match args.get(name).and_then(Value::as_str) {
            Some(id) if !id.is_empty() => Ok(id.to_string()),
            _ => Err(ToolFailure::new(
                "InvalidArguments",
                format!("{name} is required and is a body id from model_summary.bodies."),
                json!({ "reason": format!("{name} is required.") }),
            )),
        }
    };
    let (a, b) = (body("a")?, body("b")?);
    let a = require_body(state, &a)?;
    let b = require_body(state, &b)?;

    let response = engine_call(
        state,
        kb,
        "MeasureInterference",
        UiToEngine::MeasureInterference {
            a: a.clone(),
            b: b.clone(),
        },
    )?;
    let EngineToUi::InterferenceMeasured { a, b, result } = &response else {
        return Err(unexpected(
            "MeasureInterference",
            "InterferenceMeasured",
            &response,
        ));
    };
    let mut out = serde_json::to_value(result).unwrap_or(Value::Null);
    out["a"] = json!(a);
    out["b"] = json!(b);
    Ok(out)
}

/// Volume, surface area, centroid and the inertia tensor about the centroid of
/// one body (Q3 of `specs/agent_mechanical_design.md` §4.2/§4.3).
pub(super) fn measure_mass(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let body_id = require_body(
        state,
        args.get("body_id").and_then(Value::as_str).unwrap_or(""),
    )?;
    let body_id = body_id.as_str();
    let density_kg_m3 = match args.get("density_kg_m3") {
        None | Some(Value::Null) => None,
        Some(value) => Some(value.as_f64().ok_or_else(|| {
            ToolFailure::new(
                "InvalidArguments",
                "density_kg_m3 must be a positive number in kg/m³.".to_string(),
                json!({ "reason": "density_kg_m3 is not a number." }),
            )
        })?),
    };

    let response = engine_call(
        state,
        kb,
        "MeasureMass",
        UiToEngine::MeasureMass {
            body_id: body_id.to_string(),
            density_kg_m3,
        },
    )?;
    let EngineToUi::MassMeasured {
        body_id,
        volume_m3,
        surface_area_m2,
        centroid,
        inertia_at_centroid,
        principal_moments,
        principal_axes,
        density_kg_m3,
        mass_kg,
        method,
        chord_bound_m,
    } = &response
    else {
        return Err(unexpected("MeasureMass", "MassMeasured", &response));
    };
    let exact = matches!(method, crate::messages::MeasureMethod::Exact);
    let mut out = json!({
        "body_id": body_id,
        "volume_m3": volume_m3,
        "surface_area_m2": surface_area_m2,
        "centroid": centroid,
        "inertia_at_centroid": inertia_at_centroid,
        "principal_moments": principal_moments,
        "principal_axes": principal_axes,
        "density_kg_m3": density_kg_m3,
        "mass_kg": mass_kg,
        "method": if exact { "exact" } else { "mesh" },
    });
    if !exact {
        out["chord_bound_m"] = json!(chord_bound_m);
    }
    Ok(out)
}

/// The faces of one body, each with the `GeomRef` that names it (ICR-3).
pub(super) fn face_list(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let body_id = require_body(
        state,
        args.get("body_id").and_then(Value::as_str).unwrap_or(""),
    )?;
    let body_id = body_id.as_str();

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
    // N2 §5.3 item 1: the references this tool hands out are what an agent
    // authors with, so they are `Strict` HERE, in the JSON the agent reads.
    //
    // `face_refs::face_geom_refs` is shared with the viewport's own face-range
    // accessors, deliberately — a ref a user picks and a ref an agent lists are
    // the same ref by construction — and so it builds them `BestEffort`, which
    // is what a user's pick needs. That default travelled into every agent
    // reference: `execute_tool`'s stamp only fills a policy a caller OMITS, and
    // an agent does not omit it, it echoes back the one this tool printed. So
    // the loudness item 1 asks for never reached the one path that sources
    // almost every agent reference (measured 2026-10-03: the N1 and N2 oracles
    // built their references from `face_list` and resolved `BestEffort`
    // throughout). An agent that wants a rebind still spells `BestEffort` for
    // itself, and then it is genuinely in the transcript.
    let mut faces = serde_json::to_value(faces).unwrap_or(Value::Null);
    if let Some(list) = faces.as_array_mut() {
        for face in list {
            if let Some(policy) = face.pointer_mut("/geom_ref/policy") {
                *policy = json!({ "type": "Strict" });
            }
        }
    }
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
pub(super) fn region_inputs(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    entities: &[SketchEntity],
    solved: &SolvedPositions,
) -> Result<RegionInputs, ToolFailure> {
    let mut out = Vec::new();
    let mut positions = HashMap::new();

    for entity in entities {
        let expanded = match entity {
            SketchEntity::Gear { id, params, .. } => Some(expand_gear(state, kb, *id, params)?),
            SketchEntity::Sprocket { id, params, .. } => {
                Some(expand_sprocket(state, kb, *id, params)?)
            }
            _ => None,
        };
        match expanded {
            Some(primitives) => {
                for expanded in primitives {
                    if let SketchEntity::Point { id, x, y, .. } = &expanded {
                        positions.insert(*id, (*x, *y));
                    }
                    out.push(expanded);
                }
            }
            None => {
                if let SketchEntity::Point { id, x, y, .. } = entity {
                    positions.insert(*id, solved.get(id).copied().unwrap_or((*x, *y)));
                }
                out.push(entity.clone());
            }
        }
    }
    Ok((out, positions))
}

/// Per-generator id range of a completed sketch's expansion, distinct from
/// the range the active sketch editor uses (JS `inactiveGearIdBase`), and the
/// same range `Sketch::expand_generators` gives a sprocket at rebuild.
fn gear_id_base(entity_id: u32) -> u32 {
    generated_entity_id_base(entity_id)
}

/// One `Sprocket` entity as the points and arcs it stands for, shifted into
/// its own id range like a gear, with the pitch circle as a construction
/// reference on the centre point.
fn expand_sprocket(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    entity_id: u32,
    params: &SprocketParams,
) -> Result<Vec<SketchEntity>, ToolFailure> {
    let response = engine_call(
        state,
        kb,
        "GenerateSprocketProfile",
        UiToEngine::GenerateSprocketProfile {
            params: params.clone(),
        },
    )?;
    let EngineToUi::SprocketProfileGenerated {
        entities,
        pitch_radius,
        ..
    } = &response
    else {
        return Err(unexpected(
            "GenerateSprocketProfile",
            "SprocketProfileGenerated",
            &response,
        ));
    };

    let base = gear_id_base(entity_id);
    let mut out: Vec<SketchEntity> = entities.iter().map(|e| remap(e, base)).collect();
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
    entity.with_ids_offset(base)
}

/// Evaluate one mm-space expression against the document's parameters,
/// reporting the value AND the dimension it produced (P1).
///
/// `dimension` in the arguments is optional: naming the kind of field the
/// expression is meant for makes this the same refusal the rebuild would
/// make there (`25deg` for a `Length` is an error, not 25 mm).
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
    let wanted = match args.get("dimension") {
        None | Some(Value::Null) => None,
        Some(v) => match serde_json::from_value::<Dimension>(v.clone()) {
            Ok(d) => Some(d),
            Err(_) => {
                return Err(ToolFailure::new(
                    "InvalidArguments",
                    format!("`dimension` must be one of Length, Angle, Count, Ratio (got {v})"),
                    json!({ "schema_path": "/dimension" }),
                ))
            }
        },
    };

    let response = engine_call(
        state,
        kb,
        "EvaluateExpression",
        UiToEngine::EvaluateExpression {
            expression: expression.clone(),
            dimension: wanted,
        },
    )?;
    let EngineToUi::ExpressionEvaluated {
        value,
        dimension,
        error,
    } = &response
    else {
        return Err(unexpected(
            "EvaluateExpression",
            "ExpressionEvaluated",
            &response,
        ));
    };

    let mut out = json!({ "expression": expression, "value_mm": value });
    if let Some(dimension) = dimension {
        out["dimension"] = json!(dimension);
    }
    if let Some(error) = error {
        out["error"] = json!(error);
    }
    Ok(out)
}
