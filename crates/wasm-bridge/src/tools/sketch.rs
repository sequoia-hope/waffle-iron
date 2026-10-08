//! The sketch door: `sketch_create`, `sketch_edit`, `sketch_solve_state`
//! (`specs/waffle_mcp_server.md` §2.5, `specs/waffle_server_mode.md` §2.3 S3 C5,
//! `specs/agent_mechanical_design.md` §10.3).
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
//! - **Reference dimensions are committed but do not drive.** The whole array
//!   goes to the solver, which drops them from the DRIVING set itself (S2) and
//!   reports every index against the array it was handed. Until S3 this tool
//!   filtered them out first and so reported indices in the filtered space.
//! - **A failed regions query must not fail the call.** The sketch is already
//!   committed by then, so the failure is reported beside the answer.
//!
//! ## S3 — editing a sketch that already exists
//!
//! Before S3 the only way an agent could change a sketch was to read the whole
//! `Sketch` operation back with `feature_get`, mutate the JSON and hand all of
//! it to `feature_edit` — wholesale replacement, with no id allocation, no
//! operation vocabulary (no trim, no fillet, no drag) and nothing in the answer
//! about whether the result still solves. [`sketch_edit`] applies the
//! [`SketchOp`](waffle_types::sketch_ops::SketchOp) batch S1 implements against
//! the STORED sketch, solves once, and commits once;
//! [`sketch_solve_state`] solves and commits nothing.
//!
//! Both answer with the S2 solver report, which until S3 reached no caller at
//! all: `sketch_create` read `solved.status` and dropped `solved.report` on the
//! floor, and the report is not a field of `Sketch`, so it was not in the
//! document either. An agent's whole picture of a sketch was a status tag.

use feature_engine::types::Operation;
use modeling_ops::KernelBundle;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use uuid::Uuid;
use waffle_types::profiles::build_finish_profiles;
use waffle_types::sketch_ops::SketchOp;
use waffle_types::{
    Anchor, GeomRef, ResolvePolicy, Role, Selector, SketchConstraint, SketchEntity, SolveStatus,
    SolvedSketch, TopoKind,
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
///
/// `pub(super)` since Q4: `measure_section` resolves its cut plane through
/// [`resolve_plane`] so that every plane an agent can name for a sketch is a
/// plane it can section with, in one resolver rather than two.
pub(super) struct SketchPlane {
    pub(super) origin: [f64; 3],
    pub(super) normal: [f64; 3],
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
pub(super) fn resolve_plane(
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
            x_axis: x_axis.or_else(|| waffle_types::SketchPlaneBasis::default_x_axis(normal)),
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
        // No caller choice ⇒ the same default the page stamps on a sketch it
        // starts (`SketchPlaneBasis::default_x_axis`): world +X on a plane
        // facing ±Z, derived everywhere else. An agent's Top sketch and the
        // user's Top sketch must agree on which way +x runs.
        x_axis: x_axis.or_else(|| waffle_types::SketchPlaneBasis::default_x_axis(normal)),
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
/// Every face reference is passed through now — scoped or local. A scoped one
/// is the sketch's plane anchor, because in-context editing re-derives the
/// plane from it on every rebuild; a LOCAL one is pinned to the face's
/// persistent identity by `dispatch::pin_sketch_plane_face` and recorded in
/// `Sketch::plane_face`, which is what lets the rebuild prove the face is
/// still there instead of sketching into space (N2 §5.3 item 3). Before N2 a
/// local face ref was thrown away here and replaced by the placeholder below.
///
/// Anything that is not a face reference still becomes the placeholder datum:
/// the committed sketch's real plane travels in `plane_origin` /
/// `plane_normal`, so the ref only has to be well formed.
fn begin_sketch_plane_ref(face_ref: Option<&Value>) -> Result<GeomRef, ToolFailure> {
    if let Some(reference) = face_ref {
        // A model face: the anchor is a feature's output. The page's own
        // `{"type":"DatumPlane"}` anchor is NOT a `waffle_types::Anchor`
        // variant, so typing it here would refuse every origin-plane sketch —
        // the reason `resolve_plane` reads that form from raw JSON.
        let anchored_at_a_feature = reference
            .get("anchor")
            .and_then(|a| a.get("type"))
            .and_then(Value::as_str)
            == Some("FeatureOutput");
        let scoped = reference.get("scope").is_some_and(|s| !s.is_null());
        if anchored_at_a_feature || scoped {
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
    // The WHOLE constraint array goes to the solver, reference dimensions
    // included. The solver drops them from the driving set itself (S2) and maps
    // every index it reports — `conflicts`, `redundant`, `residuals[].index` —
    // back to this array. Until S3 this tool filtered them out HERE first, so
    // the indices it reported were in the FILTERED space: with a reference
    // dimension ahead of a conflict, the conflict named the wrong constraint.
    // That is the three-places filter S2's notes describe, and this was the
    // third place.
    let n_constraints = constraints.len();
    let response = abandon_on_err(state, |state| {
        crate::tools::author::send(
            state,
            kb,
            UiToEngine::SolveSketch {
                entities: Some(entities.clone()),
                constraints: Some(constraints.clone()),
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
        // The full S2 report (§10.3: "`sketch_create` … gains `positions` and
        // the full `SketchState`"). The flat `solve_status`/`dof` above predate
        // it and keep their shape — `dof` is null there for a failed solve,
        // where `state.dof` is the number the solver actually computed.
        "state": sketch_state_json(solved, n_constraints, &[]),
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

    regions_from(state, kb, &sketch.entities, &sketch.solved_positions)
}

/// The closed regions of geometry that may not be committed yet, so that
/// `sketch_solve_state` reports the regions of the solve it just ran rather
/// than the ones the stored sketch was last saved with.
fn regions_from(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    entities: &[SketchEntity],
    solved_positions: &HashMap<u32, (f64, f64)>,
) -> Result<Vec<Value>, ToolFailure> {
    let (entities, solved_positions) =
        crate::tools::inspect::region_inputs(state, kb, entities, solved_positions)?;
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

// ── S3: editing and reading a stored sketch ─────────────────────────────────

/// The stored sketch a `feature_id` argument names.
///
/// `OperationKindMismatch` for a feature that is not a sketch, in the spelling
/// `sketch_regions` and `sketch3d_get` already use: an agent that has learned
/// one of these refusals should not have to learn a second.
fn require_sketch(
    state: &EngineState,
    args: &Value,
) -> Result<(Uuid, waffle_types::Sketch), ToolFailure> {
    let feature = crate::tools::require_feature(state, args)?;
    let feature_id = feature.id;
    if let Operation::Sketch { sketch } = &feature.operation {
        return Ok((feature_id, sketch.clone()));
    }
    let kind = serde_json::to_value(&feature.operation)
        .ok()
        .and_then(|v| v.get("type").and_then(Value::as_str).map(str::to_string));
    Err(ToolFailure::new(
        "OperationKindMismatch",
        format!(
            "Feature {feature_id} is a {}, not a Sketch.",
            kind.clone().unwrap_or_else(|| "undefined".to_string())
        ),
        json!({ "expected": "Sketch", "got": kind }),
    ))
}

/// A `{"<id>": …}` object in ascending id order.
///
/// Sorted, not `HashMap` order: these maps are the coordinates a test asserts
/// against, and a map that reorders between two runs of the same input makes
/// every byte-comparison of an answer worthless.
fn by_id<T>(map: &HashMap<u32, T>, value: impl Fn(&T) -> Value) -> Value {
    let mut ids: Vec<u32> = map.keys().copied().collect();
    ids.sort_unstable();
    Value::Object(
        ids.into_iter()
            .map(|id| (id.to_string(), value(&map[&id])))
            .collect(),
    )
}

/// `SketchState` (§10.3): the S2 solver report, plus the geometry the solve
/// produced.
///
/// **Index space.** Every index in here — `conflicts`, `redundant`,
/// `residuals[].index` — indexes the constraint array of the sketch that was
/// SOLVED, which is the sketch's own array followed by `transient`
/// (a `MovePoint`'s pin, §10.3). So an index below `constraints` names a
/// stored constraint and anything at or above it is named in
/// `transient_constraints`. Nothing is remapped or dropped: a drag whose pin
/// is the offender has to be able to say so, and S2's whole point was to stop
/// each consumer inventing its own index space.
fn sketch_state_json(
    solved: &SolvedSketch,
    constraints: usize,
    transient: &[SketchConstraint],
) -> Value {
    let report = &solved.report;
    let mut out = json!({
        "status": solve_status_tag(&solved.status),
        "dof": report.dof,
        "params": report.params,
        "rank": report.rank,
        "rows": report.rows,
        "constraints": constraints,
        "conflicts": report.conflicts,
        "redundant": report.redundant,
        "residuals": report.residuals,
        "moved": report.moved,
        "free": report.free,
        "convergence": report.convergence,
        "positions": by_id(&solved.positions, |(x, y)| json!([x, y])),
        "radii": by_id(&solved.radii, |r| json!(r)),
    });
    if !transient.is_empty() {
        out["transient_constraints"] = Value::Array(
            transient
                .iter()
                .enumerate()
                .map(|(i, c)| json!({ "index": constraints + i, "kind": c.kind() }))
                .collect(),
        );
    }
    out
}

/// A refused operation, named. Never a silent no-op: the typed
/// [`SketchOpError`](waffle_types::sketch_ops::SketchOpError) travels whole, so
/// a caller can branch on `details.reason.type` rather than read prose.
fn op_refused(error: waffle_types::sketch_ops::SketchOpError) -> ToolFailure {
    ToolFailure::new(
        "SketchOpRefused",
        error.to_string(),
        json!({ "reason": error }),
    )
}

/// Solve a sketch for its own sake: no commit, no undo step, nothing changed.
pub(super) fn sketch_solve_state(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let (feature_id, sketch) = require_sketch(state, args)?;
    let solved = sketch_solver::solve_sketch(&sketch);

    let mut out = json!({
        "feature_id": feature_id,
        "solve_status": solve_status_tag(&solved.status),
        "dof": solved.report.dof,
        "state": sketch_state_json(&solved, sketch.constraints.len(), &[]),
        "regions": [],
    });
    // The regions of the solve just run, from its own positions — not the
    // stored `solved_positions`, which a `feature_edit` may have left behind.
    match regions_from(state, kb, &sketch.entities, &solved.positions) {
        Ok(regions) => out["regions"] = json!(regions),
        Err(failure) => out["regions_error"] = json!(failure.message),
    }
    Ok(out)
}

/// Apply a batch of operations to a stored sketch: one solve, one undo step
/// (§10.3).
///
/// The engine's sketch MODE is never entered. `BeginSketch`/`FinishSketch`
/// append a feature; this edits one that exists, which is the path
/// `feature_engine::params` already takes when a dimension expression re-solves
/// a sketch in the tree.
pub(super) fn sketch_edit(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
    context: Option<&Value>,
) -> Answer {
    let (feature_id, stored) = require_sketch(state, args)?;

    let ops_json = args
        .get("ops")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if ops_json.is_empty() {
        return Err(invalid_sketch("/ops: name at least one operation."));
    }
    let ops: Vec<SketchOp> = serde_json::from_value(json!(ops_json))
        .map_err(|e| invalid_sketch(format!("/ops: not sketch operations: {e}")))?;
    let on_error = OnError::from_args(args);

    // Nothing is solved or committed in here: each op runs against the state
    // the one before it produced, which is what makes the batch one undo step.
    let applied = sketch_solver::ops::apply_ops(&stored, &ops, 0).map_err(op_refused)?;

    // A `MovePoint`'s pin drives this solve and is then gone. It goes AFTER the
    // stored constraints so that every index the report carries about a stored
    // constraint is the index that constraint has in the committed sketch.
    let constraints = applied.sketch.constraints.len();
    let mut to_solve = applied.sketch.clone();
    to_solve
        .constraints
        .extend(applied.transient_constraints.iter().cloned());
    let solved = sketch_solver::solve_sketch(&to_solve);

    let failed_solve = matches!(
        solved.status,
        SolveStatus::OverConstrained { .. } | SolveStatus::SolveFailed { .. }
    );
    if failed_solve && on_error == OnError::Rollback {
        // Refused before the edit was sent, so there is nothing to roll back:
        // the document has not moved, and `apply_step` never ran.
        return Err(ToolFailure::new(
            "SketchSolveFailed",
            format!(
                "The edited sketch did not solve ({}); nothing was committed.",
                solve_status_tag(&solved.status)
            ),
            json!({
                "status": solve_status_tag(&solved.status),
                "conflicts": solved.report.conflicts,
                "reason": match &solved.status {
                    SolveStatus::SolveFailed { reason } => json!(reason),
                    _ => Value::Null,
                },
                "state": sketch_state_json(&solved, constraints, &applied.transient_constraints),
            }),
        ));
    }

    // The solution is written back into the entities and the derived data
    // re-derived from them — the same order `feature_engine::params` uses when
    // a dimension expression re-solves a stored sketch. A failed solve's
    // positions are not written back: they are wherever LM stopped, and the
    // `keep` path is meant to preserve what the caller asked for, not a
    // half-solved relocation of it.
    let mut edited = applied.sketch;
    if !failed_solve {
        for e in &mut edited.entities {
            match e {
                SketchEntity::Point { id, x, y, .. } => {
                    if let Some((sx, sy)) = solved.positions.get(id) {
                        *x = *sx;
                        *y = *sy;
                    }
                }
                SketchEntity::Circle { id, radius, .. } => {
                    if let Some(r) = solved.radii.get(id) {
                        *radius = *r;
                    }
                }
                _ => {}
            }
        }
    }
    edited.solve_status = solved.status.clone();
    edited.solved_positions.clear();
    edited.solved_profiles.clear();
    edited.recompute_derived();

    let step = apply_step(
        state,
        kb,
        UiToEngine::EditFeature {
            feature_id,
            operation: Operation::Sketch { sketch: edited },
            provenance: agent_provenance(context),
        },
        if failed_solve {
            OnError::Keep
        } else {
            on_error
        },
        "InvalidSketch",
    )?;

    let mut out = json!({
        "feature_id": feature_id,
        "solve_status": solve_status_tag(&solved.status),
        "dof": solved.report.dof,
        "state": sketch_state_json(&solved, constraints, &applied.transient_constraints),
        "edit": edit_json(&applied.edit),
        "regions": [],
    });
    match regions_of(state, kb, feature_id) {
        Ok(regions) => out["regions"] = json!(regions),
        Err(failure) => out["regions_error"] = json!(failure.message),
    }
    if let (Some(target), Some(delta)) = (out.as_object_mut(), step.delta.as_object()) {
        for (key, value) in delta {
            target.insert(key.clone(), value.clone());
        }
    }
    Ok(out)
}

/// What the batch did, by id — the ids a caller needs to address what it just
/// made (the arc a fillet minted, the line a trim kept). The entities
/// themselves are a `feature_get` away, and the coordinates are in
/// `state.positions`.
fn edit_json(edit: &waffle_types::sketch_ops::SketchEdit) -> Value {
    let named = |list: &[SketchEntity]| {
        Value::Array(
            list.iter()
                .map(|e| json!({ "id": e.id(), "type": entity_tag(e) }))
                .collect(),
        )
    };
    json!({
        "added": named(&edit.added),
        "removed": edit.removed,
        "changed": named(&edit.changed),
        "constraints_added": edit.constraints_added.len(),
        "constraints_removed": edit.constraints_removed,
    })
}

fn entity_tag(entity: &SketchEntity) -> String {
    serde_json::to_value(entity)
        .ok()
        .and_then(|v| v.get("type").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| "undefined".to_string())
}
