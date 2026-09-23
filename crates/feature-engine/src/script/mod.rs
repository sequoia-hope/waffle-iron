//! Custom feature scripts (`specs/custom_features_and_modeling_roadmap.md`
//! Part A, milestones A-M0–A-M2): a Rhai script from the document's sources
//! table runs INSIDE the engine over the same operations the feature tree
//! has, and appears in the tree as ONE node.
//!
//! ## Execution model (§A4, as built)
//!
//! 1. Load the script text by `source_id` from the [`SourceStore`].
//! 2. Parse its header (`@feature`, `@param`) and resolve the node's
//!    arguments against it — typed, defaulted, range-checked; expression-
//!    driven arguments come from the parameter pass (`arg_values`).
//! 3. Evaluate `entry(ctx, p)` under the sandbox limits. The script's API
//!    calls RECORD child operations into a private sub-tree (`host`);
//!    sketches are derived (pure Rust) so the script sees its regions.
//! 4. Execute the recorded children in order through the ordinary
//!    executor, each child seeing the outer results plus the earlier
//!    children's. Consumption between children is tracked the ordinary
//!    way; a child that targets an OUTER body (a `body` parameter, A-M3)
//!    consumes it on the node's behalf — [`ScriptOutcome::consumed_outer`]
//!    reports it to the rebuild loop after the run, which marks it as it
//!    would for a boolean.
//! 5. The node's outputs are the bodies of the children no later child
//!    consumed, in child order, keyed by the script's RETURN VALUE (A-M3,
//!    §A6): `#{ main: …, hub: …, top: face_query }` ⇒ `Main`,
//!    `Named { "hub" }`, and `Role::Named { "top" }` on the resolved face;
//!    a bare feature ref is `main`. `@output name: kind` header lines are
//!    the contract the return value must satisfy. `ctx.mate_connector`
//!    children become the node's connectors ([`ScriptOutcome::connectors`]).
//!    Any failure — header, parse, runtime, `ctx.fail`, a limit, a child's
//!    error, a broken output contract — is a typed `EngineError::Script`
//!    and the node has NO outputs (P10).
//!
//! The private sub-tree is re-derived on every regeneration and never
//! persisted.

pub mod header;
pub mod host;
pub mod interp;
pub mod library;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use modeling_ops::{KernelBundle, OpResult};
use rhai::{Dynamic, Map};
use uuid::Uuid;
use waffle_types::{Anchor, GeomRef, OutputKey, Role, TopoKind};

use crate::connector::PartConnector;
use crate::sources::SourceStore;
use crate::types::{EngineError, Feature, FeatureTree, Operation, ScriptParams};
use header::{Literal, OutputKind, ParamType, ScriptInterface};
use host::{Child, OutputValue, PlaneRef, PlaneSpec, Query, Recorder};

/// Millimeters (the expression engine's length unit) to meters.
const MM_TO_METERS: f64 = 1e-3;

fn err(stage: &str, reason: impl Into<String>) -> EngineError {
    EngineError::Script {
        stage: stage.to_string(),
        reason: reason.into(),
    }
}

/// A script's recorded children after one evaluation, without executing
/// them: what a dry run (`script_run_check`, the gear-parity oracle) needs.
#[derive(Debug)]
pub struct Recorded {
    pub interface: ScriptInterface,
    pub children: Vec<Child>,
    pub logs: Vec<String>,
    /// The public outputs the return value named (`main` first when present),
    /// checked against the header's `@output` contract.
    pub outputs: Vec<(String, OutputValue)>,
}

/// What executing a `Script` node yields beyond its `OpResult`: the OUTER
/// features its children consumed (the rebuild loop marks them, as it would
/// for a boolean), and the mate connectors it placed (exposed on the node).
#[derive(Debug)]
pub struct ScriptOutcome {
    pub result: OpResult,
    pub consumed_outer: Vec<Uuid>,
    pub connectors: Vec<PartConnector>,
}

/// Resolve the node's arguments against the header: every declared
/// parameter gets a typed value (from `args`, an expression's cached value,
/// or the header default); unknown arguments are refused.
fn resolve_args(iface: &ScriptInterface, params: &ScriptParams) -> Result<Map, EngineError> {
    for name in params.args.keys().chain(params.arg_exprs.keys()) {
        if iface.param(name).is_none() {
            return Err(err(
                "args",
                format!(
                    "`{name}` is not a parameter of this script (declared: {})",
                    iface
                        .params
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
    }
    let mut out = Map::new();
    for decl in &iface.params {
        let name = decl.name.as_str();
        // Expression-driven: the cached raw value (mm-space / degrees /
        // plain), converted by declared type.
        let value: Dynamic = if params.arg_exprs.contains_key(name) {
            let Some(raw) = params.arg_values.get(name) else {
                return Err(err(
                    "args",
                    format!(
                        "expression for `{name}` has no evaluated value yet (a parameter error?)"
                    ),
                ));
            };
            match decl.ty {
                ParamType::Length => Dynamic::from(raw * MM_TO_METERS),
                ParamType::Angle | ParamType::Number => Dynamic::from(*raw),
                ParamType::Int => {
                    if raw.fract() != 0.0 {
                        return Err(err(
                            "args",
                            format!("`{name}` must be an integer, got {raw}"),
                        ));
                    }
                    Dynamic::from(*raw as i64)
                }
                other => {
                    return Err(err(
                        "args",
                        format!(
                            "`{name}` is a {} and cannot be driven by an expression",
                            other.label()
                        ),
                    ))
                }
            }
        } else if let Some(v) = params.args.get(name) {
            json_arg(decl.ty, name, v)?
        } else if let Some(d) = &decl.default {
            match (d, decl.ty) {
                (Literal::Number(n), ParamType::Int) => Dynamic::from(*n as i64),
                (Literal::Number(n), _) => Dynamic::from(*n),
                (Literal::Bool(b), _) => Dynamic::from(*b),
                (Literal::Text(s), _) => Dynamic::from(s.clone()),
            }
        } else {
            return Err(err(
                "args",
                format!("`{name}` ({}) is required", decl.ty.label()),
            ));
        };
        // Range check on numeric parameters.
        let numeric = value
            .as_float()
            .ok()
            .or_else(|| value.as_int().ok().map(|i| i as f64));
        if let Some(x) = numeric {
            if !x.is_finite() {
                return Err(err("args", format!("`{name}` is not finite")));
            }
            if let Some(min) = decl.min {
                if x < min {
                    return Err(err(
                        "args",
                        format!("`{name}` = {x} is below its minimum {min}"),
                    ));
                }
            }
            if let Some(max) = decl.max {
                if x > max {
                    return Err(err(
                        "args",
                        format!("`{name}` = {x} is above its maximum {max}"),
                    ));
                }
            }
        }
        out.insert(name.into(), value);
    }
    Ok(out)
}

/// One JSON argument to the script value its declared type wants.
fn json_arg(ty: ParamType, name: &str, v: &serde_json::Value) -> Result<Dynamic, EngineError> {
    use serde_json::Value as J;
    let bad = |what: &str| err("args", format!("`{name}` must be {what}, got {v}"));
    Ok(match ty {
        ParamType::Int => match v {
            J::Number(n) => {
                let f = n.as_f64().unwrap_or(f64::NAN);
                if f.fract() != 0.0 || !f.is_finite() {
                    return Err(bad("an integer"));
                }
                Dynamic::from(f as i64)
            }
            _ => return Err(bad("an integer")),
        },
        ParamType::Number | ParamType::Length | ParamType::Angle => match v {
            J::Number(n) => Dynamic::from(n.as_f64().unwrap_or(f64::NAN)),
            _ => return Err(bad("a number (model units: meters / degrees)")),
        },
        ParamType::Bool => match v {
            J::Bool(b) => Dynamic::from(*b),
            _ => return Err(bad("true or false")),
        },
        ParamType::String => match v {
            J::String(s) => Dynamic::from(s.clone()),
            _ => return Err(bad("a string")),
        },
        ParamType::Plane => match v {
            J::String(s) => match Uuid::parse_str(s) {
                Ok(id) => Dynamic::from(PlaneRef(PlaneSpec::Datum(id))),
                Err(_) => return Err(bad("a datum plane id or {origin, normal}")),
            },
            J::Object(o) => {
                let v3 = |k: &str| -> Result<Option<[f64; 3]>, EngineError> {
                    match o.get(k) {
                        None => Ok(None),
                        Some(J::Array(a)) if a.len() == 3 => {
                            let mut out = [0.0; 3];
                            for (i, c) in a.iter().enumerate() {
                                out[i] = c
                                    .as_f64()
                                    .ok_or_else(|| bad(&format!("{k}: [x, y, z] numbers")))?;
                            }
                            Ok(Some(out))
                        }
                        _ => Err(bad(&format!("{k}: [x, y, z]"))),
                    }
                };
                let origin = v3("origin")?.unwrap_or([0.0; 3]);
                let Some(normal) = v3("normal")? else {
                    return Err(bad("{origin, normal} with a normal"));
                };
                Dynamic::from(PlaneRef(PlaneSpec::OriginNormal { origin, normal }))
            }
            _ => return Err(bad("a datum plane id or {origin, normal}")),
        },
        ParamType::Body | ParamType::Face | ParamType::Edge => {
            let want = match ty {
                ParamType::Body => TopoKind::Solid,
                ParamType::Face => TopoKind::Face,
                _ => TopoKind::Edge,
            };
            let gr: GeomRef = serde_json::from_value(v.clone())
                .map_err(|e| bad(&format!("a geometry reference ({want:?} GeomRef): {e}")))?;
            if gr.kind != want {
                return Err(bad(&format!(
                    "a reference of kind {want:?} (got {:?})",
                    gr.kind
                )));
            }
            if !matches!(gr.anchor, Anchor::FeatureOutput { .. }) {
                return Err(bad("a reference anchored at a feature output"));
            }
            if gr.scope.is_some() {
                return Err(bad(
                    "a reference in this tab (a reference scoped to an assembly instance resolves \
                     only through the open context, which a script does not see)",
                ));
            }
            Dynamic::from(Query::of_outer(gr))
        }
    })
}

/// Check the return value against the header's `@output` contract: every
/// declared output present with its declared kind (a `connector` is a
/// `ctx.mate_connector` child of that name); a `main` declared under another
/// name is normalized to `main`.
fn check_output_contract(
    iface: &ScriptInterface,
    children: &[Child],
    outputs: &mut [(String, OutputValue)],
) -> Result<(), EngineError> {
    // A bare return satisfies a declared `main` whatever it was named.
    if let Some(main_decl) = iface.outputs.iter().find(|o| o.kind == OutputKind::Main) {
        if let Some(entry) = outputs.iter_mut().find(|(n, _)| *n == "main") {
            entry.0 = main_decl.name.clone();
        }
    }
    let mut seen_children = HashSet::new();
    for (name, value) in outputs.iter() {
        if let Some(decl) = iface.output(name) {
            let ok = match (decl.kind, value) {
                (OutputKind::Main | OutputKind::Body, OutputValue::Body { .. }) => true,
                (OutputKind::Face, OutputValue::Entity(gr)) => gr.kind == TopoKind::Face,
                (OutputKind::Edge, OutputValue::Entity(gr)) => gr.kind == TopoKind::Edge,
                _ => false,
            };
            if !ok {
                return Err(err(
                    "runtime",
                    format!(
                        "output `{name}` is declared `{}` but the script returned a {}",
                        decl.kind.label(),
                        match value {
                            OutputValue::Body { .. } => "body".to_string(),
                            OutputValue::Entity(gr) => format!("{:?}", gr.kind).to_lowercase(),
                        }
                    ),
                ));
            }
        }
        if let OutputValue::Body { child, .. } = value {
            if !seen_children.insert(*child) {
                return Err(err(
                    "runtime",
                    format!("output `{name}` names a body another output already names"),
                ));
            }
        }
    }
    for decl in &iface.outputs {
        match decl.kind {
            OutputKind::Connector => {
                let present = children.iter().any(|c| {
                    matches!(c.feature.operation, Operation::MateConnector { .. })
                        && c.feature.name == decl.name
                });
                if !present {
                    return Err(err(
                        "runtime",
                        format!(
                            "declared connector `{}` was not placed (no ctx.mate_connector(#{{ name: \"{}\" }}))",
                            decl.name, decl.name
                        ),
                    ));
                }
            }
            _ => {
                if !outputs.iter().any(|(n, _)| *n == decl.name) {
                    return Err(err(
                        "runtime",
                        format!(
                            "declared output `{}` ({}) is missing from the return value",
                            decl.name,
                            decl.kind.label()
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Phases 1–2 without arguments or a kernel: parse the header, compile the
/// script, and confirm it defines `fn entry(ctx, p)`. What an editor's
/// "Check" and the `script_run_check` tool do before a node exists (A-M4,
/// §A8); the interface it returns is what generates the parameter dialog.
/// A dry run with arguments is [`record`].
pub fn check(text: &str, entry: &str) -> Result<ScriptInterface, EngineError> {
    let interface = header::parse_header(text).map_err(|e| err("header", e))?;
    let limits = interp::Limits::default();
    let engine = interp::build_engine(&limits);
    let ast = interp::compile(&engine, text).map_err(|f| err(f.stage, f.reason))?;
    if !ast.iter_functions().any(|f| f.name == entry) {
        return Err(err(
            "parse",
            format!("the script defines no `fn {entry}(ctx, p)`"),
        ));
    }
    Ok(interface)
}

/// The header's declared `@feature name`, when `text` has a valid header:
/// the display name a `Script` node takes on creation.
pub fn display_name(text: &str) -> Option<String> {
    header::parse_header(text)
        .ok()
        .map(|h| h.name)
        .filter(|n| !n.trim().is_empty())
}

/// Phases 1–3 without the kernel: parse, resolve arguments, evaluate,
/// return the recorded children. `args` are the node's `ScriptParams`.
pub fn record(text: &str, params: &ScriptParams) -> Result<Recorded, EngineError> {
    let interface = header::parse_header(text).map_err(|e| err("header", e))?;
    let args = resolve_args(&interface, params)?;
    let limits = interp::Limits::default();
    let mut engine = interp::build_engine(&limits);
    let ast = interp::compile(&engine, text).map_err(|f| err(f.stage, f.reason))?;
    let rec: host::Shared = Rc::new(RefCell::new(Recorder::default()));
    let returned = interp::run(&mut engine, &ast, &params.entry, args, rec.clone())
        .map_err(|f| err(f.stage, f.reason))?;
    let rec = Rc::try_unwrap(rec)
        .map(RefCell::into_inner)
        .unwrap_or_else(|shared| shared.borrow().clone_state());
    if rec.children.is_empty() {
        return Err(err("runtime", "the script recorded no operations"));
    }
    let mut outputs = host::lower_outputs(&returned).map_err(|e| err("runtime", e))?;
    check_output_contract(&interface, &rec.children, &mut outputs)?;
    Ok(Recorded {
        interface,
        children: rec.children,
        logs: rec.logs,
        outputs,
    })
}

impl Recorder {
    fn clone_state(&self) -> Recorder {
        Recorder {
            children: self.children.clone(),
            logs: self.logs.clone(),
            failed: self.failed.clone(),
        }
    }
}

/// Execute a `Script` node: record, then run the children through the
/// ordinary executor.
pub(crate) fn execute(
    feature: &Feature,
    kb: &mut dyn KernelBundle,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    already_consumed: &HashSet<Uuid>,
    sources: &SourceStore,
) -> Result<ScriptOutcome, EngineError> {
    let Operation::Script { params } = &feature.operation else {
        return Err(err("internal", "not a script feature"));
    };
    let Some(text) = sources.text(params.source_id) else {
        return Err(EngineError::SourceUnavailable {
            feature_name: feature.name.clone(),
            source_id: Some(params.source_id),
            reason: format!(
                "script source {} is not in this session's source store",
                params.source_id
            ),
        });
    };
    let recorded = record(&text, params)?;

    // The children see the outer tree + themselves, and the outer results +
    // the earlier children's.
    let mut sub_tree = tree.clone();
    let mut results: HashMap<Uuid, OpResult> = feature_results.clone();
    let mut consumed: HashSet<Uuid> = already_consumed.clone();
    let mut consumed_outer: Vec<Uuid> = Vec::new();
    let mut warnings: Vec<String> = recorded.logs.iter().map(|l| format!("log: {l}")).collect();
    let mut created = Vec::new();
    let mut deleted = Vec::new();
    let mut roles = Vec::new();
    let is_child = |id: &Uuid| recorded.children.iter().any(|c| c.feature.id == *id);

    for (i, child) in recorded.children.iter().enumerate() {
        let mut child_feature = child.feature.clone();
        // A sketch's plane is resolved now, against real geometry.
        if let (Some(plane), Operation::Sketch { sketch }) =
            (&child.plane, &mut child_feature.operation)
        {
            let (origin, normal) = match plane {
                PlaneSpec::OriginNormal { origin, normal } => (*origin, *normal),
                PlaneSpec::Datum(id) => crate::rebuild::find_datum_plane_data(
                    *id,
                    &sub_tree,
                    &results,
                    kb.as_introspect(),
                )
                .map_err(|e| err("child", format!("child {i} (sketch plane): {e}")))?,
                PlaneSpec::Face(gr) => {
                    crate::rebuild::resolve_face_plane(gr, &results, kb.as_introspect())
                        .map_err(|e| err("child", format!("child {i} (sketch plane): {e}")))?
                }
            };
            let len =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            if !(len.is_finite() && len > 0.0) {
                return Err(err(
                    "child",
                    format!("child {i} (sketch plane): zero normal"),
                ));
            }
            sketch.plane_origin = origin;
            sketch.plane_normal = crate::rebuild::unit_normal(normal);
        }
        sub_tree.features.push(child_feature.clone());

        let consumed_now = crate::rebuild::find_consumed_feature_ids(
            &child_feature,
            &results,
            &sub_tree,
            &consumed,
            Some(kb.as_introspect()),
        );
        let result = crate::rebuild::execute_feature(
            &child_feature,
            kb,
            &results,
            &sub_tree,
            &consumed,
            sources,
            None,
        )
        .map_err(|e| err("child", format!("child {i} ({}): {e}", child.label)))?;
        // A union-failed auto-merge leaves its targets alive; the loop applies
        // the same rule to a tree feature.
        let union_failed = result
            .diagnostics
            .warnings
            .iter()
            .any(|w| w.contains("Auto-union failed"));
        if !union_failed {
            for id in consumed_now {
                if !is_child(&id) {
                    // An OUTER body (a `body` parameter's target): consumed
                    // by this node as a whole; reported to the loop.
                    if !consumed_outer.contains(&id) {
                        consumed_outer.push(id);
                    }
                }
                consumed.insert(id);
            }
        }
        for w in &result.diagnostics.warnings {
            warnings.push(format!("child {i} ({}): {w}", child.label));
        }
        created.extend(result.provenance.created.iter().cloned());
        deleted.extend(result.provenance.deleted.iter().cloned());
        roles.extend(result.provenance.role_assignments.iter().cloned());
        results.insert(child_feature.id, result);
    }

    // Bodies: every child body no later child consumed, in child order.
    let mut bodies: Vec<(Uuid, OutputKey, modeling_ops::BodyOutput)> = Vec::new();
    for child in &recorded.children {
        let id = child.feature.id;
        if consumed.contains(&id) {
            continue;
        }
        if let Some(r) = results.get(&id) {
            for (key, body) in &r.outputs {
                if matches!(key, OutputKey::Main | OutputKey::Body { .. }) {
                    bodies.push((id, key.clone(), body.clone()));
                }
            }
        }
    }
    if bodies.is_empty() {
        return Err(err(
            "runtime",
            "the script produced no bodies (every child was consumed or made no solid)",
        ));
    }

    // Named outputs (spec §A6): the `main` entry (or the header's declared
    // main) is `Main`; other body entries are `Named`; face/edge entries are
    // resolved now and tagged `Role::Named`.
    let main_name = recorded
        .interface
        .outputs
        .iter()
        .find(|o| o.kind == OutputKind::Main)
        .map(|o| o.name.as_str())
        .unwrap_or("main");
    let mut main_child: Option<Uuid> = None;
    let mut named_children: Vec<(String, Uuid)> = Vec::new();
    for (name, value) in &recorded.outputs {
        match value {
            OutputValue::Body { id, .. } => {
                if consumed.contains(id) {
                    return Err(err(
                        "runtime",
                        format!("output `{name}` names a body a later child consumed"),
                    ));
                }
                if !bodies
                    .iter()
                    .any(|(cid, key, _)| cid == id && *key == OutputKey::Main)
                {
                    return Err(err(
                        "runtime",
                        format!("output `{name}` names a child that produced no body"),
                    ));
                }
                if name == main_name || name == "main" {
                    main_child = Some(*id);
                } else {
                    named_children.push((name.clone(), *id));
                }
            }
            OutputValue::Entity(gr) => {
                let resolved =
                    crate::resolve::resolve_geom_ref_live(gr, &results, kb.as_introspect())
                        .map_err(|e| err("runtime", format!("output `{name}`: {e}")))?;
                for w in resolved.warnings {
                    warnings.push(format!("output `{name}`: {w}"));
                }
                roles.push((resolved.kernel_id, Role::Named { name: name.clone() }));
            }
        }
    }
    // The main body first: the named one, else the first child body.
    if let Some(mc) = main_child {
        if let Some(pos) = bodies
            .iter()
            .position(|(cid, key, _)| *cid == mc && *key == OutputKey::Main)
        {
            let b = bodies.remove(pos);
            bodies.insert(0, b);
        }
    }
    let outputs: Vec<(OutputKey, modeling_ops::BodyOutput)> = bodies
        .into_iter()
        .enumerate()
        .map(|(i, (cid, key, b))| {
            let out_key = if i == 0 {
                OutputKey::Main
            } else if let Some((name, _)) = named_children
                .iter()
                .find(|(_, id)| *id == cid && key == OutputKey::Main)
            {
                OutputKey::Named { name: name.clone() }
            } else {
                OutputKey::Body { index: i }
            };
            (out_key, b)
        })
        .collect();

    // Mate connectors the script placed, in the node's own coordinates: the
    // children proved their frames derive; evaluate them once more against
    // the final sub-tree results so the node exposes the frames by name.
    let mut connectors = Vec::new();
    for child in &recorded.children {
        let Operation::MateConnector { params: cp } = &child.feature.operation else {
            continue;
        };
        let (frame, geometry) =
            crate::connector::part_connector_frame(cp, &results, kb.as_introspect()).map_err(
                |e| {
                    err(
                        "child",
                        format!("mate connector `{}`: {e}", child.feature.name),
                    )
                },
            )?;
        connectors.push(PartConnector {
            feature_id: feature.id,
            name: child.feature.name.clone(),
            frame,
            geometry,
        });
    }

    Ok(ScriptOutcome {
        result: OpResult {
            outputs,
            provenance: modeling_ops::Provenance {
                created,
                deleted,
                modified: Vec::new(),
                role_assignments: roles,
            },
            diagnostics: modeling_ops::Diagnostics {
                warnings,
                ..modeling_ops::Diagnostics::default()
            },
        },
        consumed_outer,
        connectors,
    })
}
