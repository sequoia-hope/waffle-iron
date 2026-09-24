//! `sketch_create` — author a whole sketch in one call
//! (`specs/waffle_mcp_server.md` §2.5, `specs/waffle_server_mode.md` §2.3 S3 C5).
//!
//! Ported from `app/src/lib/agent/commands.js`. Unlike the twelve authoring
//! tools of C4 this one orchestrates four messages — `BeginSketch`,
//! `SolveSketch`, `FinishSketch`, then a regions query — so the page could not
//! simply hand it over: the profile payload it builds between the solve and
//! the commit (`buildFinishProfiles`) had no Rust twin until C5 ported it into
//! `waffle_types::profiles`.
//!
//! Three things here are not obvious:
//!
//! - **The plane can arrive in a shape Rust cannot type.** The page mints datum
//!   plane refs as `{anchor: {type: "DatumPlane", id}}` (`planes.js`), which is
//!   not a variant of [`waffle_types::Anchor`] — deserializing one into a
//!   `GeomRef` fails outright. So the plane is resolved from the RAW JSON,
//!   branching on `anchor.type`, and only a face ref is typed.
//! - **Reference dimensions are committed but do not drive.** Only
//!   non-`reference` constraints go to the solver; all of them go into the
//!   feature, exactly as the page does it.
//! - **A failed regions query must not fail the call.** The sketch is already
//!   committed by then, so the failure is reported beside the answer.

use feature_engine::types::Operation;
use modeling_ops::KernelBundle;
use serde_json::{json, Map, Value};
use uuid::Uuid;
use waffle_types::profiles::build_finish_profiles;
use waffle_types::{
    Anchor, GeomRef, ResolvePolicy, Role, Selector, SketchConstraint, SketchEntity, SolveStatus,
    TopoKind,
};

use crate::engine_state::EngineState;
use crate::messages::{EngineToUi, UiToEngine};
use crate::tools::author::{agent_provenance, apply_step, OnError};
use crate::tools::{unexpected, Answer, ToolFailure};

/// The three built-in datum planes, by the well-known UUIDs `planes.js` and
/// `feature_engine::rebuild` both hard-code.
const FRONT_PLANE_ID: &str = "00000000-0000-0000-0000-000000000001";
const TOP_PLANE_ID: &str = "00000000-0000-0000-0000-000000000002";
const RIGHT_PLANE_ID: &str = "00000000-0000-0000-0000-000000000003";

/// Entity fields that must name a `Point` (JS `ENTITY_POINT_FIELDS`).
fn point_fields(type_tag: &str) -> &'static [&'static str] {
    match type_tag {
        "Line" => &["start_id", "end_id"],
        "Circle" => &["center_id"],
        "Arc" => &["center_id", "start_id", "end_id"],
        _ => &[],
    }
}

/// Constraint fields that name a sketch entity (JS `CONSTRAINT_REF_FIELDS`).
const CONSTRAINT_REF_FIELDS: &[&str] = &[
    "point",
    "point_a",
    "point_b",
    "entity",
    "entity_a",
    "entity_b",
    "line",
    "line_a",
    "line_b",
    "line_c",
    "line_d",
    "curve",
    "symmetry_line",
];

/// Shape checks for `sketch_create` input (A13), ported from
/// `app/src/lib/agent/sketchInput.js`.
///
/// Ids only: unique entity ids, entity references that name existing Points,
/// and constraint references that name existing entities. No geometry is
/// checked (§6.4) — degenerate values go to the solver unchanged. The message
/// is prefixed with a JSON pointer, which is part of the contract.
pub(super) fn sketch_input_problem(entities: &[Value], constraints: &[Value]) -> Option<String> {
    let mut types: Map<String, Value> = Map::new();
    for (i, e) in entities.iter().enumerate() {
        let id = e.get("id").cloned().unwrap_or(Value::Null);
        let key = id.to_string();
        if types.contains_key(&key) {
            return Some(format!("/entities/{i}/id: duplicate entity id {id}"));
        }
        types.insert(key, e.get("type").cloned().unwrap_or(Value::Null));
    }
    let is_point = |id: &Value| types.get(&id.to_string()) == Some(&json!("Point"));
    let known = |id: &Value| types.contains_key(&id.to_string());

    for (i, e) in entities.iter().enumerate() {
        let tag = e.get("type").and_then(Value::as_str).unwrap_or("");
        for field in point_fields(tag) {
            let named = e.get(*field).cloned().unwrap_or(Value::Null);
            if !is_point(&named) {
                return Some(format!(
                    "/entities/{i}/{field}: no Point entity with id {named}"
                ));
            }
        }
        if tag == "Spline" {
            let ids = e
                .get("point_ids")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for (k, id) in ids.iter().enumerate() {
                if !is_point(id) {
                    return Some(format!(
                        "/entities/{i}/point_ids/{k}: no Point entity with id {id}"
                    ));
                }
            }
        }
    }

    for (i, c) in constraints.iter().enumerate() {
        for field in CONSTRAINT_REF_FIELDS {
            if let Some(named) = c.get(*field) {
                if !known(named) {
                    return Some(format!(
                        "/constraints/{i}/{field}: no entity with id {named}"
                    ));
                }
            }
        }
    }
    None
}

/// The plane a sketch is created on: where it is, and the face it came from.
struct SketchPlane {
    origin: [f64; 3],
    normal: [f64; 3],
    /// The caller's chosen in-plane +u direction, when it gave one.
    x_axis: Option<[f64; 3]>,
    /// The caller's face ref, when it named one — carried into `BeginSketch`
    /// only if it is scoped (in-context editing).
    face_ref: Option<Value>,
}

fn invalid_sketch(message: impl Into<String>) -> ToolFailure {
    let message = message.into();
    ToolFailure::new(
        "InvalidSketch",
        message.clone(),
        json!({ "reason": message }),
    )
}

/// A datum plane's UUID from a page-shaped ref, including the legacy
/// `{plane: "XY"}` spelling `planes.js` still accepts.
fn datum_id_of(anchor: &Value) -> Option<Uuid> {
    if let Some(id) = anchor.get("id").and_then(Value::as_str) {
        return Uuid::parse_str(id).ok();
    }
    if let Some(id) = anchor.get("datum_id").and_then(Value::as_str) {
        return Uuid::parse_str(id).ok();
    }
    let legacy = match anchor.get("plane").and_then(Value::as_str)? {
        "XY" => FRONT_PLANE_ID,
        "XZ" => TOP_PLANE_ID,
        "YZ" => RIGHT_PLANE_ID,
        _ => return None,
    };
    Uuid::parse_str(legacy).ok()
}

fn vec3(value: Option<&Value>) -> Option<[f64; 3]> {
    let a = value?.as_array()?;
    if a.len() < 3 {
        return None;
    }
    Some([a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?])
}

/// Resolve the `plane` argument (JS `computeFacePlane`, plus the explicit
/// `{origin, normal}` form).
///
/// A datum plane ref is read from the RAW JSON: the page's `DatumPlane` anchor
/// is not a `waffle_types::Anchor` variant, so typing it first would refuse
/// every origin-plane the user can pick.
fn resolve_plane(
    state: &EngineState,
    kb: &mut dyn KernelBundle,
    plane: Option<&Value>,
) -> Result<SketchPlane, ToolFailure> {
    let plane = plane.ok_or_else(|| invalid_sketch("plane is required."))?;

    // `x_axis` orients the sketch on ANY plane form — a bare origin/normal,
    // a datum, or a face — so it is read before the branch
    // (`docs/notes/eiffel/FEATURE_NOTES.md` §3).
    let x_axis = match plane.get("x_axis") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            vec3(Some(v)).ok_or_else(|| invalid_sketch("plane.x_axis must be three numbers."))?,
        ),
    };

    if plane.get("origin").is_some() {
        let origin = vec3(plane.get("origin"))
            .ok_or_else(|| invalid_sketch("plane.origin must be three numbers."))?;
        let normal = vec3(plane.get("normal"))
            .ok_or_else(|| invalid_sketch("plane.normal must be three numbers."))?;
        check_x_axis(normal, x_axis)?;
        return Ok(SketchPlane {
            origin,
            normal,
            x_axis,
            face_ref: None,
        });
    }

    let unresolved = || {
        ToolFailure::new(
            "InvalidSketch",
            "plane does not resolve to a face or datum plane of the open Part.",
            json!({ "reason": "unresolved plane" }),
        )
    };

    let anchor = plane.get("anchor").ok_or_else(unresolved)?;
    let anchor_type = anchor.get("type").and_then(Value::as_str).unwrap_or("");
    let (origin, normal) = match anchor_type {
        // The page's own datum-plane shape, and the engine's.
        "DatumPlane" | "Datum" => {
            let datum_id = datum_id_of(anchor).ok_or_else(unresolved)?;
            feature_engine::rebuild::resolve_datum_plane(
                datum_id,
                &state.engine.tree,
                &state.engine.feature_results,
                kb.as_introspect(),
            )
            .map_err(|_| unresolved())?
        }
        // A face of the model: typed, and resolved from the current geometry.
        _ => {
            let geom_ref: GeomRef =
                serde_json::from_value(plane.clone()).map_err(|_| unresolved())?;
            feature_engine::rebuild::resolve_face_plane(
                &geom_ref,
                &state.engine.feature_results,
                kb.as_introspect(),
            )
            .map_err(|_| unresolved())?
        }
    };

    check_x_axis(normal, x_axis)?;
    Ok(SketchPlane {
        origin,
        normal,
        x_axis,
        face_ref: Some(plane.clone()),
    })
}

/// An `x_axis` that cannot orient the plane is refused HERE, where the caller
/// can fix it, rather than committing a sketch whose rebuild would fail.
fn check_x_axis(normal: [f64; 3], x_axis: Option<[f64; 3]>) -> Result<(), ToolFailure> {
    match x_axis {
        Some(x) if !waffle_types::SketchPlaneBasis::x_axis_is_usable(normal, x) => {
            Err(invalid_sketch(format!(
                "plane.x_axis {x:?} cannot orient a plane of normal {normal:?}: it is \
                 zero-length, non-finite, or parallel to the normal."
            )))
        }
        _ => Ok(()),
    }
}

/// The `GeomRef` a `BeginSketch` carries (JS `beginSketchPlaneRef`).
///
/// A scoped face ref is passed through, because in-context editing re-derives
/// the plane from it on every rebuild. Anything else becomes a placeholder
/// datum: the committed sketch's real plane travels in `plane_origin` /
/// `plane_normal`, so the ref only has to be well formed.
fn begin_sketch_plane_ref(face_ref: Option<&Value>) -> Result<GeomRef, ToolFailure> {
    if let Some(reference) = face_ref {
        if reference.get("scope").is_some_and(|s| !s.is_null()) {
            return serde_json::from_value(reference.clone())
                .map_err(|e| invalid_sketch(format!("plane is not a GeomRef: {e}")));
        }
    }
    Ok(GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::new_v4(),
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
        scope: None,
    })
}

/// Create a sketch in one call: solve the given geometry and commit it as a
/// `Sketch` feature (one undo step).
pub(super) fn sketch_create(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
    context: Option<&Value>,
) -> Answer {
    let entities_json = args
        .get("entities")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let constraints_json = args
        .get("constraints")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    if let Some(problem) = sketch_input_problem(&entities_json, &constraints_json) {
        return Err(invalid_sketch(problem));
    }

    let plane = resolve_plane(state, kb, args.get("plane"))?;
    let on_error = OnError::from_args(args);

    let entities: Vec<SketchEntity> = serde_json::from_value(json!(entities_json))
        .map_err(|e| invalid_sketch(format!("entities are not sketch entities: {e}")))?;
    // A generator with parameters it cannot expand is refused here, where
    // the author can fix them, not at the rebuild that would otherwise leave
    // the sketch without a profile.
    for (i, e) in entities.iter().enumerate() {
        if let SketchEntity::Sprocket { params, .. } = e {
            if let Err(err) = waffle_types::sprocket_dimensions(params) {
                return Err(invalid_sketch(format!("/entities/{i}/params: {err}")));
            }
        }
    }
    let constraints: Vec<SketchConstraint> = serde_json::from_value(json!(constraints_json))
        .map_err(|e| invalid_sketch(format!("constraints are not sketch constraints: {e}")))?;
    // A reference dimension is committed with the sketch but must not drive
    // the solve, or it would fight the geometry it only measures.
    let driving_json: Vec<Value> = constraints_json
        .iter()
        .filter(|c| c.get("reference") != Some(&json!(true)))
        .cloned()
        .collect();
    let driving: Vec<SketchConstraint> = serde_json::from_value(json!(driving_json))
        .map_err(|e| invalid_sketch(format!("constraints are not sketch constraints: {e}")))?;

    crate::tools::author::send(
        state,
        kb,
        UiToEngine::BeginSketch {
            plane: begin_sketch_plane_ref(plane.face_ref.as_ref())?,
        },
        "Internal",
    )?;

    // From here until `FinishSketch` commits, the engine has an open sketch.
    // A refusal on this stretch must close it (`abandon_on_err`): left open,
    // it shadows any sketch the user had begun and a later
    // `SolveSketch { entities: None }` would solve the abandoned one.
    let response = abandon_on_err(state, |state| {
        crate::tools::author::send(
            state,
            kb,
            UiToEngine::SolveSketch {
                entities: Some(entities.clone()),
                constraints: Some(driving),
            },
            "InvalidSketch",
        )
    })?;
    let EngineToUi::SketchSolved { solved } = &response else {
        state.active_sketch = None;
        return Err(unexpected("SolveSketch", "SketchSolved", &response));
    };

    let status = solved.status.clone();
    let failed_solve = matches!(
        status,
        SolveStatus::OverConstrained { .. } | SolveStatus::SolveFailed { .. }
    );
    let status_tag = solve_status_tag(&status);
    if failed_solve && on_error == OnError::Rollback {
        state.active_sketch = None;
        return Err(ToolFailure::new(
            "SketchSolveFailed",
            format!("The sketch did not solve ({status_tag}); nothing was committed."),
            json!({
                "status": status_tag,
                "conflicts": match &status {
                    SolveStatus::OverConstrained { conflicts } => json!(conflicts),
                    _ => json!([]),
                },
                "reason": match &status {
                    SolveStatus::SolveFailed { reason } => json!(reason),
                    _ => Value::Null,
                },
            }),
        ));
    }

    // The solver reports a circle's radius separately (a Diameter/Radius
    // constraint solves the radius param, which never travels through
    // `positions`), and the profile payload reads it from the entity.
    let positions = solved.positions.clone();
    let radii = solved.radii.clone();
    let solved_entities: Vec<SketchEntity> = entities
        .into_iter()
        .map(|e| match e {
            SketchEntity::Circle {
                id,
                center_id,
                radius,
                construction,
            } => SketchEntity::Circle {
                id,
                center_id,
                radius: radii.get(&id).copied().unwrap_or(radius),
                construction,
            },
            other => other,
        })
        .collect();

    let extracted = waffle_types::extract_profiles(&solved_entities, &positions);
    let finished = build_finish_profiles(&extracted, &solved_entities, &positions);

    // `finish_sketch` closes the open sketch itself when it commits; a
    // refusal before that point leaves it open, so this is the last stretch
    // `abandon_on_err` covers.
    let step = abandon_on_err(state, |state| {
        apply_step(
            state,
            kb,
            UiToEngine::FinishSketch {
                solved_positions: finished.solved_positions,
                solved_profiles: finished.profiles,
                plane_origin: plane.origin,
                plane_normal: plane.normal,
                plane_x_axis: plane.x_axis,
                entities: solved_entities,
                constraints,
                projected: Vec::new(),
                provenance: agent_provenance(context),
            },
            if failed_solve {
                OnError::Keep
            } else {
                on_error
            },
            "InvalidSketch",
        )
    })?;

    // The basis the sketch actually got, so a caller never has to reproduce
    // the engine's choice to know where its +x went
    // (`docs/notes/eiffel/FEATURE_NOTES.md` §3).
    let basis = waffle_types::SketchPlaneBasis::from_origin_normal_x(
        plane.origin,
        plane.normal,
        plane.x_axis,
    );
    let mut out = json!({
        "feature_id": step.feature_id,
        "solve_status": status_tag,
        "dof": match &status {
            SolveStatus::FullyConstrained => json!(0),
            SolveStatus::UnderConstrained { dof } => json!(dof),
            _ => Value::Null,
        },
        "plane": {
            "origin": basis.origin,
            "normal": basis.normal,
            "x_axis": basis.x_axis,
            "y_axis": basis.y_axis,
        },
        "regions": [],
    });

    // The sketch is committed: a regions query that fails is reported beside
    // the answer, never as a failed step.
    match step.feature_id.map(|id| regions_of(state, kb, id)) {
        Some(Ok(regions)) => out["regions"] = json!(regions),
        Some(Err(failure)) => out["regions_error"] = json!(failure.message),
        None => {}
    }

    if let (Some(target), Some(delta)) = (out.as_object_mut(), step.delta.as_object()) {
        for (key, value) in delta {
            target.insert(key.clone(), value.clone());
        }
    }
    Ok(out)
}

/// The serde tag of a solve status, as the answer reports it.
/// Run one step of an open sketch; on refusal, close the sketch first.
///
/// `BeginSketch` opens `state.active_sketch` and only a committed
/// `FinishSketch` closes it — there is no cancel message — so every refusal
/// in between would otherwise leave the engine with a sketch nobody owns.
fn abandon_on_err<T>(
    state: &mut EngineState,
    step: impl FnOnce(&mut EngineState) -> Result<T, ToolFailure>,
) -> Result<T, ToolFailure> {
    let result = step(state);
    if result.is_err() {
        state.active_sketch = None;
    }
    result
}

fn solve_status_tag(status: &SolveStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.get("type").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| "Unsolved".to_string())
}

/// The committed sketch's closed regions, through the same request
/// `sketch_regions` sends (gears expanded).
fn regions_of(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    feature_id: Uuid,
) -> Result<Vec<Value>, ToolFailure> {
    let Some(Operation::Sketch { sketch }) = state
        .engine
        .tree
        .features
        .iter()
        .find(|f| f.id == feature_id)
        .map(|f| f.operation.clone())
    else {
        return Err(ToolFailure::new(
            "Internal",
            "the committed sketch is not in the tree",
            json!({}),
        ));
    };

    let (entities, solved_positions) = crate::tools::inspect::region_inputs(
        state,
        kb,
        &sketch.entities,
        &sketch.solved_positions,
    )?;
    // Through `engine_call`, as `sketch_regions` sends the same message: one
    // failure, one `regions_error` text, whichever tool asked.
    let response = crate::tools::engine_call(
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
    Ok(regions
        .iter()
        .map(|r| json!({ "profile_entity_ids": r.profile_entity_ids, "area_m2": r.area }))
        .collect())
}
