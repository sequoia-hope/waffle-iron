//! The assembly tools in the engine (`specs/waffle_server_mode.md` §2.3, the
//! P-C remainder): `assembly_get`, `instance_*`, `connector_*`, `mate_*`.
//!
//! Until 2026-09-23 these were the page's JS (`app/src/lib/agent/assembly.js`
//! over the store's `addInstance` / `addConnector` / `addMate` flows), which
//! is why a native host could not hold an assembly document. The semantics
//! are the same, tool by tool: every edit is a mutation of the open Assembly
//! tab's tree followed by one `EditAssembly`, which re-evaluates the tab and
//! writes the solved placements back; the answer is the evaluated state read
//! after that. An assembly edit is not an undo step (undo/redo act on a Part
//! tab's feature tree), so nothing here rolls back: a refused edit never
//! reaches `EditAssembly`, and an evaluation that fails restores the tree.
//!
//! The gates a HOST keeps (§3.3): `DocumentReadOnly` (the page knows what is
//! linked read-only; the native host's documents never are) and the busy /
//! paused gates. The Assembly-tab gate is semantic and lives here.

use std::collections::BTreeMap;

use feature_engine::assembly::{
    AssemblyTree, AxialAnchor, Frame, Instance, Mate, MateConnector, MateKind, PartRef, Transform,
    MATE_KIND_TAGS,
};
use modeling_ops::KernelBundle;
use serde_json::{json, Map, Value};
use uuid::Uuid;
use waffle_types::GeomRef;

use super::{engine_call, Answer, ToolFailure};
use crate::engine_state::EngineState;
use crate::messages::{AssemblyStatus, EngineToUi, UiToEngine};

/// The assembly tools: the ones whose gate is "an Assembly tab is active"
/// (the inverse of the feature tools' Part-tab gate, G7).
pub const ASSEMBLY_TOOLS: &[&str] = &[
    "assembly_get",
    "instance_add",
    "instance_edit",
    "instance_delete",
    "connector_add",
    "connector_edit",
    "connector_delete",
    "mate_add",
    "mate_edit",
    "mate_delete",
];

/// The active Assembly tab, or the G7-shaped refusal (JS `requireAssemblyTab`).
fn require_assembly_tab(state: &EngineState) -> Result<crate::session::TabInfo, ToolFailure> {
    let active = state.session.active_tab_id().to_string();
    let tab = state.session.tabs().into_iter().find(|t| t.id == active);
    let kind = tab
        .as_ref()
        .map(|t| t.kind.clone())
        .unwrap_or_else(|| "Part".to_string());
    match tab {
        Some(tab) if kind == "Assembly" => Ok(tab),
        _ => Err(ToolFailure::new(
            "TabKindNotSupported",
            format!(
                "The active tab is a {kind} tab; assembly tools need an Assembly tab \
                 (tab_add kind:\"Assembly\" or tab_switch)."
            ),
            json!({ "kind": kind }),
        )),
    }
}

/// The open assembly's tree (the active tab holds one: checked by the caller).
fn tree(state: &EngineState, tab_id: &str) -> Result<AssemblyTree, ToolFailure> {
    state
        .session
        .assembly(tab_id)
        .cloned()
        .map_err(|e| ToolFailure::new("Internal", e.to_string(), json!({})))
}

/// Evaluate the open assembly if nothing has yet (a document opened straight
/// onto its Assembly tab): the answer's placements and frames come from the
/// evaluated view.
fn ensure_evaluated(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    tab_id: &str,
) -> Result<(), ToolFailure> {
    if state.assembly.is_none() {
        engine_call(
            state,
            kb,
            "OpenAssembly",
            UiToEngine::OpenAssembly {
                tab_id: tab_id.to_string(),
            },
        )?;
    }
    Ok(())
}

/// A linked source's tabs: `(tab id, name, kind)` each.
type SourceTabList = Vec<(String, String, String)>;

/// The tabs of every available linked `.waffle` source: `(source id, source
/// name, tabs)` — what the page caches from `ListSourceTabs`.
fn source_tabs(state: &EngineState) -> Vec<(Uuid, String, SourceTabList)> {
    crate::dispatch::source_statuses(state)
        .into_iter()
        .filter(|s| s.kind == "Waffle" && s.available)
        .filter_map(|s| {
            crate::assembly_view::source_tabs(s.id, &state.engine.sources)
                .ok()
                .map(|tabs| (s.id, s.name, tabs))
        })
        .collect()
}

/// One entry of `available_parts`.
struct Part {
    tab_id: String,
    name: String,
    kind: String,
    source_id: Option<Uuid>,
    source_name: Option<String>,
}

/// Every tab an instance can be made of: this document's Part tabs and its
/// other Assembly tabs (sub-assemblies; never the open one), then the tabs of
/// each available linked source (JS `availableParts`).
fn available_parts(state: &EngineState, active: &str) -> Vec<Part> {
    let mut out: Vec<Part> = state
        .session
        .tabs()
        .into_iter()
        .filter(|t| t.kind == "Part" || (t.kind == "Assembly" && t.id != active))
        .map(|t| Part {
            tab_id: t.id,
            name: t.name,
            kind: t.kind,
            source_id: None,
            source_name: None,
        })
        .collect();
    for (source_id, source_name, tabs) in source_tabs(state) {
        for (id, name, kind) in tabs {
            if kind != "Part" && kind != "Assembly" {
                continue;
            }
            out.push(Part {
                tab_id: id,
                name,
                kind,
                source_id: Some(source_id),
                source_name: Some(source_name.clone()),
            });
        }
    }
    out
}

fn part_json(p: &Part) -> Value {
    json!({
        "tab_id": p.tab_id,
        "name": p.name,
        "kind": p.kind,
        "source_id": p.source_id,
        "source_name": p.source_name,
    })
}

/// The instance's part tab name, for reading (JS `partName`).
fn part_name(parts: &[Part], source: &PartRef) -> String {
    parts
        .iter()
        .find(|p| p.tab_id == source.tab_id && p.source_id == source.source_id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| source.tab_id.clone())
}

fn frame_json(f: &Frame) -> Value {
    json!({ "origin": f.origin, "z_axis": f.z_axis, "x_axis": f.x_axis })
}

/// The open assembly as the tools return it (JS `assemblyState`, the page's
/// `assemblyStateSchema`).
fn assembly_state(state: &mut EngineState, kb: &mut dyn KernelBundle) -> Answer {
    let tab = require_assembly_tab(state)?;
    ensure_evaluated(state, kb, &tab.id)?;
    let asm = tree(state, &tab.id)?;
    let status: Option<AssemblyStatus> = crate::dispatch::assembly_status(state);
    let placements: BTreeMap<Uuid, Transform> = status
        .as_ref()
        .map(|s| s.placements.clone())
        .unwrap_or_else(|| asm.placements.clone());
    let parts = available_parts(state, &tab.id);

    let instances: Vec<Value> = asm
        .instances
        .iter()
        .map(|i| {
            json!({
                "id": i.id,
                "name": i.name,
                "source": { "tab_id": i.source.tab_id, "source_id": i.source.source_id },
                "part_name": part_name(&parts, &i.source),
                "transform": i.transform,
                "fixed": i.fixed,
                "suppressed": i.suppressed,
                "placement": placements.get(&i.id),
            })
        })
        .collect();

    let connectors: Vec<Value> = asm
        .connectors
        .iter()
        .map(|c| {
            let world = status
                .as_ref()
                .and_then(|s| s.connectors.iter().find(|f| f.id == c.id))
                .map(|f| {
                    json!({
                        "kind": f.kind,
                        "origin": f.origin,
                        "x_axis": f.x_axis,
                        "y_axis": f.y_axis,
                        "z_axis": f.z_axis,
                    })
                });
            json!({
                "id": c.id,
                "name": c.name,
                "instance_path": c.instance_path,
                "part_connector": c.part_connector,
                "geom_ref": c.geom_ref,
                "frame": frame_json(&c.frame),
                "anchor": c.anchor,
                "flip_z": c.flip_z,
                "rotation_deg": c.rotation_deg,
                "offset_m": c.offset_m,
                "world_frame": world,
            })
        })
        .collect();

    let mates: Vec<Value> = asm
        .mates
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "name": m.name,
                "kind": m.kind,
                "connectors": m.connectors,
                "suppressed": m.suppressed,
            })
        })
        .collect();

    let part_connectors: Vec<Value> = status
        .as_ref()
        .map(|s| {
            s.part_connectors
                .iter()
                .map(|p| {
                    json!({
                        "feature_id": p.feature_id,
                        "name": p.name,
                        "instance_path": p.instance_path,
                        "kind": p.kind,
                        "origin": p.origin,
                        "x_axis": p.x_axis,
                        "y_axis": p.y_axis,
                        "z_axis": p.z_axis,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(json!({
        "tab_id": tab.id,
        "name": tab.name,
        "instances": instances,
        "connectors": connectors,
        "mates": mates,
        "part_connectors": part_connectors,
        "available_parts": parts.iter().map(part_json).collect::<Vec<_>>(),
        "errors": status.as_ref().map(|s| s.errors.clone()).unwrap_or_default(),
        "warnings": status.as_ref().map(|s| s.warnings.clone()).unwrap_or_default(),
    }))
}

/// `assembly_get`.
pub(crate) fn assembly_get(state: &mut EngineState, kb: &mut dyn KernelBundle) -> Answer {
    assembly_state(state, kb)
}

/// Write `tree` into the open Assembly tab and re-evaluate it (the page's
/// `editAssembly`: one `EditAssembly`, placements recomputed). An evaluation
/// the engine refuses is `AssemblyEditFailed`, and the tab keeps the tree it
/// had.
fn commit(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    tab_id: &str,
    mut tree: AssemblyTree,
) -> Result<(), ToolFailure> {
    let previous = state.session.assembly(tab_id).cloned().ok();
    // Placements are derived; the engine recomputes them.
    tree.placements.clear();
    let response = crate::dispatch::dispatch(
        state,
        UiToEngine::EditAssembly {
            tab_id: tab_id.to_string(),
            assembly: tree,
        },
        kb,
    );
    if let EngineToUi::Error { message, .. } = response {
        if let Some(previous) = previous {
            // Best effort: the refusal is what the agent gets either way.
            let _ = state.session.set_assembly(tab_id, previous);
        }
        return Err(ToolFailure::new(
            "AssemblyEditFailed",
            format!("The assembly edit failed: {message}"),
            json!({ "reason": message }),
        ));
    }
    // The evaluation (re)built part engines whose bodies have no meshes
    // yet; only a top-level `ModelUpdated` is tessellated by
    // `process_message`, so the tool does it here — else the answer's model
    // update, and a viewer's snapshot, list the assembly with no bodies.
    crate::tessellation_runner::tessellate_missing_meshes(state, kb);
    Ok(())
}

// ── argument helpers ──────────────────────────────────────────────────────

fn str_arg(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// JS `!!v`.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// JS `Number(v) || 0`.
fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::String(s) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn invalid(message: &str, details: Value) -> ToolFailure {
    ToolFailure::new("InvalidArguments", message, details)
}

fn vec3(v: &Value, field: &str) -> Result<[f64; 3], ToolFailure> {
    serde_json::from_value(v.clone()).map_err(|_| {
        invalid(
            &format!("{field} must be three numbers."),
            json!({ "field": field }),
        )
    })
}

/// The document `Transform` for a tool's transform argument over `current`
/// (JS `transformFrom`).
fn transform_from(input: Option<&Value>, current: Transform) -> Result<Transform, ToolFailure> {
    let mut t = current;
    let Some(input) = input.filter(|v| !v.is_null()) else {
        return Ok(t);
    };
    let quat = input.get("rotation_quat").filter(|v| !v.is_null());
    let euler = input.get("rotation_euler_deg").filter(|v| !v.is_null());
    if quat.is_some() && euler.is_some() {
        return Err(invalid(
            "Give rotation_quat or rotation_euler_deg, not both.",
            json!({ "field": "transform" }),
        ));
    }
    if let Some(v) = input.get("translation_m").filter(|v| !v.is_null()) {
        t.translation_m = vec3(v, "transform.translation_m")?;
    }
    if let Some(v) = quat {
        let q: [f64; 4] = serde_json::from_value(v.clone()).map_err(|_| {
            invalid(
                "rotation_quat must be a non-zero quaternion.",
                json!({ "field": "transform.rotation_quat" }),
            )
        })?;
        let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        if n.is_nan() || n <= 0.0 {
            return Err(invalid(
                "rotation_quat must be a non-zero quaternion.",
                json!({ "field": "transform.rotation_quat" }),
            ));
        }
        t.rotation_quat = [q[0] / n, q[1] / n, q[2] / n, q[3] / n];
    }
    if let Some(v) = euler {
        t.rotation_quat = euler_deg_to_quat(vec3(v, "transform.rotation_euler_deg")?);
    }
    Ok(t)
}

/// three.js `Quaternion.setFromEuler`, order XYZ, degrees — the page's
/// `eulerDegToQuat` (`app/src/lib/engine/rotation.js`), so an agent's Euler
/// angles mean what the assembly panel's do.
fn euler_deg_to_quat([x, y, z]: [f64; 3]) -> [f64; 4] {
    let d = std::f64::consts::PI / 180.0;
    let (c1, c2, c3) = (
        (x * d / 2.0).cos(),
        (y * d / 2.0).cos(),
        (z * d / 2.0).cos(),
    );
    let (s1, s2, s3) = (
        (x * d / 2.0).sin(),
        (y * d / 2.0).sin(),
        (z * d / 2.0).sin(),
    );
    let q = [
        s1 * c2 * c3 + c1 * s2 * s3,
        c1 * s2 * c3 - s1 * c2 * s3,
        c1 * c2 * s3 + s1 * s2 * c3,
        c1 * c2 * c3 - s1 * s2 * s3,
    ];
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    let n = if n == 0.0 { 1.0 } else { n };
    [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
}

fn require_instance<'a>(asm: &'a AssemblyTree, id: &str) -> Result<&'a Instance, ToolFailure> {
    asm.instances
        .iter()
        .find(|i| i.id.to_string() == id)
        .ok_or_else(|| {
            ToolFailure::new(
                "InstanceNotFound",
                format!("The open assembly has no instance {id}."),
                json!({ "instance_id": id }),
            )
        })
}

fn require_connector<'a>(
    asm: &'a AssemblyTree,
    id: &str,
) -> Result<&'a MateConnector, ToolFailure> {
    asm.connectors
        .iter()
        .find(|c| c.id.to_string() == id)
        .ok_or_else(|| {
            ToolFailure::new(
                "ConnectorNotFound",
                format!("The open assembly has no connector {id}."),
                json!({ "connector_id": id }),
            )
        })
}

fn require_mate<'a>(asm: &'a AssemblyTree, id: &str) -> Result<&'a Mate, ToolFailure> {
    asm.mates
        .iter()
        .find(|m| m.id.to_string() == id)
        .ok_or_else(|| {
            ToolFailure::new(
                "MateNotFound",
                format!("The open assembly has no mate {id}."),
                json!({ "mate_id": id }),
            )
        })
}

fn with_id(key: &str, id: Uuid, mut state: Value) -> Value {
    if let Some(obj) = state.as_object_mut() {
        obj.insert(key.to_string(), json!(id));
    }
    state
}

// ── instances ─────────────────────────────────────────────────────────────

/// `instance_add {tab_id, source_id?, name?, transform?, fixed?}`.
pub(crate) fn instance_add(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab = require_assembly_tab(state)?;
    let tab_id = str_arg(args, "tab_id");
    let source_id = args
        .get("source_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let source_uuid = source_id.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    let parts = available_parts(state, &tab.id);
    let part = parts
        .iter()
        .find(|p| p.tab_id == tab_id && p.source_id == source_uuid && (source_id.is_none() || source_uuid.is_some()))
        .ok_or_else(|| {
            ToolFailure::new(
                "TabNotFound",
                format!(
                    "No Part or Assembly tab {tab_id}{} to place (an assembly cannot contain itself).",
                    source_id
                        .as_ref()
                        .map(|s| format!(" in source {s}"))
                        .unwrap_or_default()
                ),
                json!({ "tab_id": tab_id, "source_id": source_id }),
            )
        })?;
    let transform = transform_from(args.get("transform"), Transform::identity())?;
    let fixed = args.get("fixed").is_some_and(truthy);
    let mut asm = tree(state, &tab.id)?;
    let source = PartRef {
        source_id: part.source_id,
        tab_id: part.tab_id.clone(),
    };
    let count = asm.instances.iter().filter(|i| i.source == source).count();
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{} {}", part.name, count + 1));
    let id = Uuid::new_v4();
    asm.instances.push(Instance {
        id,
        name,
        source,
        transform,
        fixed,
        suppressed: false,
        external_key: None,
        parameter_overrides: None,
        extra: Map::new(),
    });
    commit(state, kb, &tab.id, asm)?;
    Ok(with_id("instance_id", id, assembly_state(state, kb)?))
}

/// `instance_edit {instance_id, name?, fixed?, suppressed?, transform?}`.
pub(crate) fn instance_edit(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab = require_assembly_tab(state)?;
    let mut asm = tree(state, &tab.id)?;
    let id = str_arg(args, "instance_id");
    let current = require_instance(&asm, &id)?.transform;
    let transform = match args.get("transform") {
        Some(t) => Some(transform_from(Some(t), current)?),
        None => None,
    };
    let inst = asm
        .instances
        .iter_mut()
        .find(|i| i.id.to_string() == id)
        .expect("checked above");
    if let Some(name) = args.get("name") {
        inst.name = name.as_str().unwrap_or("").to_string();
    }
    if let Some(v) = args.get("fixed") {
        inst.fixed = truthy(v);
    }
    if let Some(v) = args.get("suppressed") {
        inst.suppressed = truthy(v);
    }
    if let Some(t) = transform {
        inst.transform = t;
    }
    commit(state, kb, &tab.id, asm)?;
    assembly_state(state, kb)
}

/// `instance_delete {instance_id}`: the instance, its connectors and the
/// mates that used them.
pub(crate) fn instance_delete(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab = require_assembly_tab(state)?;
    let mut asm = tree(state, &tab.id)?;
    let id = str_arg(args, "instance_id");
    let uuid = require_instance(&asm, &id)?.id;
    asm.instances.retain(|i| i.id != uuid);
    let gone: Vec<Uuid> = asm
        .connectors
        .iter()
        .filter(|c| c.instance_path.first() == Some(&uuid))
        .map(|c| c.id)
        .collect();
    asm.connectors.retain(|c| !gone.contains(&c.id));
    asm.mates
        .retain(|m| !m.connectors.iter().any(|c| gone.contains(c)));
    asm.placements.remove(&uuid);
    commit(state, kb, &tab.id, asm)?;
    assembly_state(state, kb)
}

// ── connectors ────────────────────────────────────────────────────────────

/// `connector_add {instance_path, part_connector? | geom_ref? | frame?, name?}`.
pub(crate) fn connector_add(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab = require_assembly_tab(state)?;
    ensure_evaluated(state, kb, &tab.id)?;
    let mut asm = tree(state, &tab.id)?;
    let path_values: Vec<Value> = args
        .get("instance_path")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let first = path_values
        .first()
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let owner = require_instance(&asm, &first)?;
    let owner_name = owner.name.clone();
    let path: Vec<Uuid> = path_values
        .iter()
        .map(|v| {
            v.as_str()
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or_else(|| {
                    invalid(
                        "instance_path must be instance ids (the top-level instance, then sub-assembly members).",
                        json!({ "field": "instance_path" }),
                    )
                })
        })
        .collect::<Result<_, _>>()?;

    let part_connector = args.get("part_connector").filter(|v| !v.is_null());
    let geom_ref = args.get("geom_ref").filter(|v| !v.is_null());
    let frame = args.get("frame").filter(|v| !v.is_null());
    if [part_connector, geom_ref, frame].iter().flatten().count() > 1 {
        return Err(invalid(
            "Give one of part_connector, geom_ref or frame, not several.",
            json!({}),
        ));
    }

    let status = crate::dispatch::assembly_status(state);
    let mut named: Option<String> = None;
    let part_connector_id = match part_connector {
        Some(v) => {
            let text = v.as_str().unwrap_or("").to_string();
            let id = Uuid::parse_str(&text).ok();
            let known = status.as_ref().and_then(|s| {
                s.part_connectors
                    .iter()
                    .find(|p| Some(p.feature_id) == id && p.instance_path == path)
            });
            match known {
                Some(p) => {
                    named = Some(p.name.clone());
                    id
                }
                None => {
                    return Err(ToolFailure::new(
                        "ConnectorNotFound",
                        format!(
                            "The part of instance {} has no evaluated MateConnector feature {text} \
                             (see assembly_get.part_connectors).",
                            path.iter().map(Uuid::to_string).collect::<Vec<_>>().join("/")
                        ),
                        json!({ "part_connector": text, "instance_path": path }),
                    ))
                }
            }
        }
        None => None,
    };

    let geom_ref: Option<GeomRef> = match geom_ref {
        Some(v) => Some(serde_json::from_value(v.clone()).map_err(|e| {
            invalid(
                &format!("geom_ref is not a GeomRef: {e}"),
                json!({ "field": "geom_ref" }),
            )
        })?),
        None => None,
    };
    if let (Some(g), None) = (&geom_ref, part_connector_id) {
        // Judge the pick BEFORE minting a connector (the page's
        // `probeConnectorRef`): a reference the engine cannot derive a frame
        // from must not fall back to a default frame at solve time.
        let reason = match state
            .assembly
            .as_ref()
            .and_then(|v| v.engine_for_path(&path))
        {
            None => Some(format!(
                "{path:?} is not a rendered part of the open assembly"
            )),
            Some(engine) => feature_engine::connector::resolve_connector_frame(
                g,
                &engine.feature_results,
                kb.as_introspect(),
                AxialAnchor::Middle,
            )
            .err()
            .map(|e| e.to_string()),
        };
        if let Some(reason) = reason {
            return Err(ToolFailure::new(
                "ConnectorRefused",
                format!("Cannot put a connector here: {reason}"),
                json!({ "reason": reason }),
            ));
        }
    }

    let frame = match frame {
        Some(f) => Frame {
            origin: f
                .get("origin")
                .filter(|v| !v.is_null())
                .map(|v| vec3(v, "frame.origin"))
                .transpose()?
                .unwrap_or([0.0; 3]),
            z_axis: f
                .get("z_axis")
                .filter(|v| !v.is_null())
                .map(|v| vec3(v, "frame.z_axis"))
                .transpose()?
                .unwrap_or([0.0, 0.0, 1.0]),
            x_axis: f
                .get("x_axis")
                .filter(|v| !v.is_null())
                .map(|v| vec3(v, "frame.x_axis"))
                .transpose()?
                .unwrap_or([0.0; 3]),
        },
        None => Frame::default(),
    };

    let owner = format!(
        "{owner_name}{}",
        if path.len() > 1 { " › member" } else { "" }
    );
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| match &named {
            Some(n) => format!("{owner} › {n}"),
            None => format!("{owner} connector {}", asm.connectors.len() + 1),
        });
    let id = Uuid::new_v4();
    asm.connectors.push(MateConnector {
        id,
        name,
        instance_path: path,
        geom_ref: if part_connector_id.is_some() {
            None
        } else {
            geom_ref
        },
        part_connector: part_connector_id,
        frame,
        anchor: AxialAnchor::Middle,
        flip_z: false,
        rotation_deg: 0.0,
        offset_m: [0.0; 3],
        extra: Map::new(),
    });
    commit(state, kb, &tab.id, asm)?;
    Ok(with_id("connector_id", id, assembly_state(state, kb)?))
}

/// `connector_edit {connector_id, name?, anchor?, flip_z?, rotation_deg?, offset_m?}`
/// (the store's `updateConnector`: each adjustment at its default is removed).
pub(crate) fn connector_edit(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab = require_assembly_tab(state)?;
    let mut asm = tree(state, &tab.id)?;
    let id = str_arg(args, "connector_id");
    require_connector(&asm, &id)?;
    let c = asm
        .connectors
        .iter_mut()
        .find(|c| c.id.to_string() == id)
        .expect("checked above");
    if let Some(name) = args.get("name") {
        let n = name.as_str().unwrap_or("").to_string();
        if !n.is_empty() {
            c.name = n;
        }
    }
    if let Some(anchor) = args.get("anchor") {
        c.anchor = match anchor.as_str() {
            Some("positive_end") => AxialAnchor::PositiveEnd,
            Some("negative_end") => AxialAnchor::NegativeEnd,
            _ => AxialAnchor::Middle,
        };
    }
    if let Some(v) = args.get("flip_z") {
        c.flip_z = truthy(v);
    }
    if let Some(v) = args.get("rotation_deg") {
        c.rotation_deg = num(v);
    }
    if let Some(v) = args.get("offset_m") {
        let o = v.as_array().cloned().unwrap_or_default();
        c.offset_m = [0, 1, 2].map(|k| o.get(k).map(num).unwrap_or(0.0));
    }
    commit(state, kb, &tab.id, asm)?;
    assembly_state(state, kb)
}

/// `connector_delete {connector_id}`: the connector and the mates using it.
pub(crate) fn connector_delete(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab = require_assembly_tab(state)?;
    let mut asm = tree(state, &tab.id)?;
    let id = str_arg(args, "connector_id");
    let uuid = require_connector(&asm, &id)?.id;
    asm.connectors.retain(|c| c.id != uuid);
    asm.mates.retain(|m| !m.connectors.contains(&uuid));
    commit(state, kb, &tab.id, asm)?;
    assembly_state(state, kb)
}

// ── mates ─────────────────────────────────────────────────────────────────

/// The store's `mateKind`: an unknown kind is `Fastened`; `flip` is dropped
/// for `Ball`; `rotation_deg` is kept only for `Fastened`.
fn mate_kind(kind: &str, flip: bool, rotation_deg: f64) -> MateKind {
    let kind = if MATE_KIND_TAGS.contains(&kind) {
        kind
    } else {
        "Fastened"
    };
    match kind {
        "Revolute" => MateKind::Revolute { flip },
        "Slider" => MateKind::Slider { flip },
        "Cylindrical" => MateKind::Cylindrical { flip },
        "Planar" => MateKind::Planar { flip },
        "Ball" => MateKind::Ball,
        _ => MateKind::Fastened { flip, rotation_deg },
    }
}

/// `mate_add {a, b, kind = Fastened, flip = true, rotation_deg = 0, name?}`.
pub(crate) fn mate_add(state: &mut EngineState, kb: &mut dyn KernelBundle, args: &Value) -> Answer {
    let tab = require_assembly_tab(state)?;
    let mut asm = tree(state, &tab.id)?;
    let a = str_arg(args, "a");
    let b = str_arg(args, "b");
    let a_id = require_connector(&asm, &a)?.id;
    let b_id = require_connector(&asm, &b)?.id;
    if a == b {
        return Err(invalid(
            "A mate needs two different connectors.",
            json!({ "a": a, "b": b }),
        ));
    }
    let kind = mate_kind(
        args.get("kind")
            .and_then(Value::as_str)
            .unwrap_or("Fastened"),
        args.get("flip").map(truthy).unwrap_or(true),
        args.get("rotation_deg").map(num).unwrap_or(0.0),
    );
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{} {}", kind.type_tag(), asm.mates.len() + 1));
    let id = Uuid::new_v4();
    asm.mates.push(Mate {
        id,
        name,
        kind,
        connectors: [a_id, b_id],
        suppressed: false,
        extra: Map::new(),
    });
    commit(state, kb, &tab.id, asm)?;
    Ok(with_id("mate_id", id, assembly_state(state, kb)?))
}

/// `mate_edit {mate_id, name?, kind?, flip?, rotation_deg?, suppressed?}`.
pub(crate) fn mate_edit(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab = require_assembly_tab(state)?;
    let mut asm = tree(state, &tab.id)?;
    let id = str_arg(args, "mate_id");
    require_mate(&asm, &id)?;
    let m = asm
        .mates
        .iter_mut()
        .find(|m| m.id.to_string() == id)
        .expect("checked above");
    if let Some(name) = args.get("name") {
        m.name = name.as_str().unwrap_or("").to_string();
    }
    if let Some(v) = args.get("suppressed") {
        m.suppressed = truthy(v);
    }
    if args.get("kind").is_some()
        || args.get("flip").is_some()
        || args.get("rotation_deg").is_some()
    {
        let current_type = m.kind.type_tag().to_string();
        let kind = args
            .get("kind")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or(current_type);
        let flip = args
            .get("flip")
            .map(truthy)
            .unwrap_or_else(|| m.kind.flip());
        let rotation = args.get("rotation_deg").map(num).unwrap_or(match &m.kind {
            MateKind::Fastened { rotation_deg, .. } => *rotation_deg,
            _ => 0.0,
        });
        m.kind = mate_kind(&kind, flip, rotation);
    }
    commit(state, kb, &tab.id, asm)?;
    assembly_state(state, kb)
}

/// `mate_delete {mate_id}`.
pub(crate) fn mate_delete(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    args: &Value,
) -> Answer {
    let tab = require_assembly_tab(state)?;
    let mut asm = tree(state, &tab.id)?;
    let id = str_arg(args, "mate_id");
    let uuid = require_mate(&asm, &id)?.id;
    asm.mates.retain(|m| m.id != uuid);
    commit(state, kb, &tab.id, asm)?;
    assembly_state(state, kb)
}
