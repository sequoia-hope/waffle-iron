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
//!    way; a child never targets an OUTER body in this milestone (M3 adds
//!    outer queries, and with them post-execution consumption reporting).
//! 5. The node's outputs are the bodies of the children no later child
//!    consumed, in child order (`Main` first). Any failure — header, parse,
//!    runtime, `ctx.fail`, a limit, a child's error — is a typed
//!    `EngineError::Script` and the node has NO outputs (P10).
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
use waffle_types::OutputKey;

use crate::sources::SourceStore;
use crate::types::{EngineError, Feature, FeatureTree, Operation, ScriptParams};
use header::{Literal, ParamType, ScriptInterface};
use host::{Child, PlaneRef, PlaneSpec, Recorder};

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
    })
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
    let _returned = interp::run(&mut engine, &ast, &params.entry, args, rec.clone())
        .map_err(|f| err(f.stage, f.reason))?;
    let rec = Rc::try_unwrap(rec)
        .map(RefCell::into_inner)
        .unwrap_or_else(|shared| shared.borrow().clone_state());
    Ok(Recorded {
        interface,
        children: rec.children,
        logs: rec.logs,
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
) -> Result<OpResult, EngineError> {
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
    if recorded.children.is_empty() {
        return Err(err("runtime", "the script recorded no operations"));
    }

    // The children see the outer tree + themselves, and the outer results +
    // the earlier children's.
    let mut sub_tree = tree.clone();
    let mut results: HashMap<Uuid, OpResult> = feature_results.clone();
    let mut consumed: HashSet<Uuid> = already_consumed.clone();
    let mut warnings: Vec<String> = recorded.logs.iter().map(|l| format!("log: {l}")).collect();
    let mut created = Vec::new();
    let mut deleted = Vec::new();
    let mut roles = Vec::new();

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
        for id in consumed_now {
            if !recorded.children.iter().any(|c| c.feature.id == id) {
                return Err(err(
                    "child",
                    format!(
                        "child {i} ({}) targets a body outside the script (feature {id}); \
                         outer references are a later milestone (M3)",
                        child.label
                    ),
                ));
            }
            consumed.insert(id);
        }
        for w in &result.diagnostics.warnings {
            warnings.push(format!("child {i} ({}): {w}", child.label));
        }
        created.extend(result.provenance.created.iter().cloned());
        deleted.extend(result.provenance.deleted.iter().cloned());
        roles.extend(result.provenance.role_assignments.iter().cloned());
        results.insert(child_feature.id, result);
    }

    // Outputs: every child body no later child consumed, in child order.
    let mut bodies = Vec::new();
    for child in &recorded.children {
        let id = child.feature.id;
        if consumed.contains(&id) {
            continue;
        }
        if let Some(r) = results.get(&id) {
            for (key, body) in &r.outputs {
                if matches!(key, OutputKey::Main | OutputKey::Body { .. }) {
                    bodies.push(body.clone());
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
    let outputs = bodies
        .into_iter()
        .enumerate()
        .map(|(i, b)| {
            let key = if i == 0 {
                OutputKey::Main
            } else {
                OutputKey::Body { index: i }
            };
            (key, b)
        })
        .collect();
    Ok(OpResult {
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
    })
}
