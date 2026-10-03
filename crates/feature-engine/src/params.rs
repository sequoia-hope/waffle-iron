//! Design-parameter evaluation and the expression apply pass.
//!
//! Runs at the START of every rebuild (`Engine::rebuild`):
//!
//! 1. Evaluate the parameter table (`FeatureTree::parameters`) into an
//!    environment of name → mm-space value, refreshing each parameter's
//!    cached `value`/`error`.
//! 2. Walk the features and re-evaluate every expression-driven measurement
//!    (sketch dimensions, extrude depth, revolve angle, datum offsets),
//!    writing results into the plain numeric fields the rest of the engine
//!    consumes. A sketch whose dimension values changed is re-solved from its
//!    current geometry and its derived data recomputed (same recompute path
//!    the projected-sketch feature uses).
//!
//! The pass is idempotent: unchanged expressions produce bit-identical values
//! and touch nothing, so incremental rebuilds stay incremental. All failures
//! are loud per-feature/per-parameter errors; a failing expression leaves the
//! previous value (and geometry) in place rather than guessing.

use std::collections::HashMap;

use uuid::Uuid;

use crate::expr::{self, Dimension, Env, ExprError, Measurer, Quantity, Span, Tag};
use crate::types::{DesignParameter, FeatureTree, Operation, PlaneDefinition};
use waffle_types::{DimensionUnit, SketchEntity, SolveStatus};

/// The dimension a sketch dimension constraint drives.
fn sketch_dimension(unit: DimensionUnit) -> Dimension {
    match unit {
        DimensionUnit::Length => Dimension::Length,
        DimensionUnit::AngleDegrees => Dimension::Angle,
    }
}

/// Result of the apply pass.
#[derive(Debug, Default)]
pub struct ParamOutcome {
    /// Lowest feature index whose effective values changed (rebuild must
    /// start at or before it). `None` = nothing changed.
    pub first_changed: Option<usize>,
    /// Every feature whose effective values changed: the rebuild re-executes
    /// these and what depends on them.
    pub changed: Vec<Uuid>,
    /// Loud errors: parameter-table errors carry the parameter's id;
    /// feature-expression errors carry the feature's id.
    pub errors: Vec<(Uuid, String)>,
}

/// Evaluate the parameter table into an environment of dimensioned
/// working-space values, refreshing each parameter's cached `value` and
/// `error` in place. Order-independent: unresolved parameters are retried
/// until a fixpoint, so forward references work; leftovers (unknown names,
/// cycles) get per-parameter errors and keep their last-good cached value.
///
/// A parameter with a declared `unit` is checked against it and enters the
/// environment COMMITTED to that dimension; one without stays a plain
/// number that adopts whatever field consumes it.
pub fn evaluate_parameters(params: &mut [DesignParameter]) -> Env {
    evaluate_parameters_with(params, None, &HashMap::new())
}

/// [`evaluate_parameters`], with the model available to measurement
/// functions (D2).
///
/// `floors` positions each measuring parameter: a parameter has no index of
/// its own, so the ordering rule is stated at its EARLIEST READER
/// ([`earliest_readers`]) — a parameter read by feature #3 may measure
/// features #1 and #2 and nothing later. Without that, a parameter
/// measuring feature #5 and read by feature #1's depth would make each
/// rebuild's answer depend on the previous one's.
pub fn evaluate_parameters_with(
    params: &mut [DesignParameter],
    measurer: Option<&crate::measure::TreeMeasurer<'_>>,
    floors: &HashMap<String, usize>,
) -> Env {
    evaluate_table(params, &Env::new(), &HashMap::new(), measurer, floors)
}

/// [`evaluate_parameters`] inside an OUTER scope — P2's document table seen
/// from a Part's table.
///
/// A name the local table declares SHADOWS the outer one, and the shadowing
/// is total: the outer value is not visible to the local table at all, not
/// even to the shadowing row's own expression. `w = "w * 2"` over a document
/// `w` is therefore a self-reference and a loud cycle, not a silent doubling
/// of a value from a table the author cannot see from here. Half-visibility
/// is the alternative and it is worse: `w` would mean the document's `w` in
/// one row of this table and the local `w` in every other.
///
/// Returns the COMBINED environment — the outer names the local table does
/// not shadow, plus every local name that resolved — which is what the
/// feature fields read.
pub fn evaluate_parameters_in(params: &mut [DesignParameter], outer: &Env) -> Env {
    evaluate_table(params, outer, &HashMap::new(), None, &HashMap::new())
}

/// Every parameter whose magnitude an instance override replaces, with the
/// dimension the parameter itself carries. See [`pin_overrides`].
pub type Pinned = HashMap<String, Quantity>;

/// Evaluate a parameter table: the one fixpoint, with every scope and every
/// deferral the two increments added.
///
/// - `outer` is P2's enclosing scope (the document table seen from a Part's).
///   A name this table declares SHADOWS an outer one, totally.
/// - `pinned` is P2's instance overrides. A pinned parameter's own expression
///   is not evaluated — the override IS its value — and the pin enters the
///   environment before the fixpoint starts rather than being patched into it
///   afterwards, so everything that reads it follows.
/// - `measurer`/`floors` are D2's model access. Without a measurer an
///   expression that MEASURES is deferred at its last-good value rather than
///   refused.
///
/// A pin beats a deferral: an overridden parameter's value comes from the
/// instance whether or not its expression would have measured anything.
fn evaluate_table(
    params: &mut [DesignParameter],
    outer: &Env,
    pinned: &Pinned,
    measurer: Option<&crate::measure::TreeMeasurer<'_>>,
    floors: &HashMap<String, usize>,
) -> Env {
    // Shadowed outer names are removed before the fixpoint starts, so an
    // unresolved local name never falls back to an outer one.
    let declared: std::collections::HashSet<String> =
        params.iter().map(|p| p.name.clone()).collect();
    let mut env: Env = outer
        .iter()
        .filter(|(name, _)| !declared.contains(name.as_str()))
        .map(|(name, q)| (name.clone(), *q))
        .collect();

    // The dependency cycles, read off the graph BEFORE any evaluation: the
    // fixpoint below can only report that a parameter never resolved, which
    // looks identical to a typo'd name. Naming the loop is P5's requirement
    // and it is cheap (one parse per parameter, no evaluation).
    let loops = cycles(params);

    // Pre-validate names; mark duplicates (first occurrence wins).
    let mut pending: Vec<usize> = Vec::new();
    let mut deferred: Vec<usize> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (i, p) in params.iter_mut().enumerate() {
        if let Err(msg) = expr::validate_name(&p.name) {
            p.error = Some(format!("invalid name: {msg}"));
            continue;
        }
        if seen.contains_key(&p.name) {
            p.error = Some(format!("duplicate parameter name '{}'", p.name));
            continue;
        }
        seen.insert(p.name.clone(), i);
        // A pinned name resolves to the pin, not to its expression: the
        // override replaces the row's value for this build. BEFORE the
        // deferral check, because an overridden parameter's value comes from
        // the instance whether or not its expression measures anything.
        if let Some(q) = pinned.get(&p.name) {
            env.insert(p.name.clone(), *q);
            p.value = q.value;
            p.tag = Some(q.tag);
            p.error = None;
            continue;
        }
        if measurer.is_none() && expr::measures(&p.expression) {
            // D2, deferred: this pass has no model. The parameter enters the
            // environment at its LAST-GOOD value so its dependents still
            // evaluate (and still drive the same geometry they did last
            // rebuild) — the measuring pass that follows replaces it with
            // the measured one and reports any error. Not `pending`: it is
            // not waiting on a name, and leaving it there would make the
            // fixpoint report a cycle that does not exist.
            deferred.push(i);
            continue;
        }
        pending.push(i);
    }

    // Deferred measurements enter first, at their cached values, so a
    // dependent's fixpoint can resolve against them.
    for &i in &deferred {
        let q = Quantity {
            value: params[i].value,
            tag: params[i].tag.unwrap_or(crate::expr::Tag::Untagged),
        };
        env.insert(params[i].name.clone(), q);
        params[i].error = None;
    }

    // Fixpoint resolution: each round evaluates what it can against the
    // already-resolved set. A round with no progress means the leftovers are
    // cycles or reference unknown names.
    loop {
        let mut progressed = false;
        let mut still_pending = Vec::new();
        for &i in &pending {
            if let Some(m) = measurer {
                m.set_floor(floors.get(&params[i].name).copied());
            }
            match evaluate_declared(&params[i], &env, measurer) {
                Ok(q) => {
                    env.insert(params[i].name.clone(), q);
                    params[i].value = q.value;
                    // The DIMENSION is cached alongside the magnitude, so
                    // `cached_env` can rebuild this exact environment for
                    // the bridge's preview instead of guessing it from
                    // `unit` (see `DesignParameter::tag`).
                    params[i].tag = Some(q.tag);
                    params[i].error = None;
                    progressed = true;
                }
                Err(ExprError::UnknownIdentifier(_)) => still_pending.push(i),
                Err(e) => {
                    params[i].error = Some(e.to_string());
                }
            }
        }
        pending = still_pending;
        if !progressed || pending.is_empty() {
            break;
        }
    }

    // Whatever is left is stuck on an unknown name — either a genuine
    // unknown or a cycle. Re-evaluate once for the specific message.
    for &i in &pending {
        // A parameter stuck on a DECLARED name is in a cycle, or reads one
        // that is. Report the loop itself when the graph has it.
        let in_cycle = loops
            .iter()
            .find(|c| c.iter().any(|n| *n == params[i].name))
            .map(|c| c.join(" → "));
        if let Some(m) = measurer {
            m.set_floor(floors.get(&params[i].name).copied());
        }
        let msg = match evaluate_declared(&params[i], &env, measurer) {
            Err(ExprError::UnknownIdentifier(name)) if seen.contains_key(&name) => {
                match &in_cycle {
                    Some(path) => format!("circular reference: {path}"),
                    // Not in a cycle itself: it reads a parameter that is, so it
                    // can never resolve either. Name the one it waits on.
                    None => format!(
                        "depends on '{name}', which does not resolve{}",
                        match loops
                            .iter()
                            .find(|c| c.contains(&name))
                            .map(|c| c.join(" → "))
                        {
                            Some(path) => format!(" (circular reference: {path})"),
                            None => String::new(),
                        }
                    ),
                }
            }
            Err(e) => e.to_string(),
            Ok(_) => unreachable!("pending parameter evaluated cleanly"),
        };
        params[i].error = Some(msg);
    }

    env
}

/// Evaluate one parameter's expression and apply its declared unit, if any.
/// A declared unit the expression contradicts is this parameter's error.
fn evaluate_declared(
    param: &DesignParameter,
    env: &Env,
    measurer: Option<&crate::measure::TreeMeasurer<'_>>,
) -> Result<Quantity, ExprError> {
    let q = expr::eval_with(
        &expr::parse(&param.expression)?,
        env,
        measurer.map(|m| m as &dyn Measurer),
    )?;
    match param.unit {
        Some(want) => q.retag(want, Span::new(0, param.expression.len())),
        None => Ok(q),
    }
}

/// Environment from the parameters' CACHED values (no re-evaluation): every
/// parameter whose last evaluation succeeded, first-of-name wins. Used by the
/// bridge's stateless expression preview, which must match what the next
/// rebuild will compute without mutating anything.
pub fn cached_env(params: &[DesignParameter]) -> Env {
    cached_env_in(params, &[])
}

/// [`cached_env`] over both scopes (P2): the document table's cached values
/// first, then the tab's, which SHADOW them — the same precedence the
/// rebuild applies, so the preview still refuses exactly what the rebuild
/// refuses.
pub fn cached_env_in(params: &[DesignParameter], document: &[DesignParameter]) -> Env {
    let mut env = cached_env_rows(document);
    for (name, q) in cached_env_rows(params) {
        env.insert(name, q);
    }
    env
}

fn cached_env_rows(params: &[DesignParameter]) -> Env {
    let mut env = Env::new();
    for p in params {
        if p.error.is_some() {
            continue;
        }
        // The cached magnitude plus the dimension the last evaluation
        // actually produced. `unit` is NOT enough on its own: an undeclared
        // parameter whose expression commits a dimension (`width = "2cm"`)
        // is a length in the rebuild's environment, and reading it here as
        // a plain number would make the preview accept what the rebuild
        // refuses. `tag` is unset only before the first pass of a freshly
        // deserialized tree, where falling back to the declared unit is the
        // best available answer.
        let q = match p.tag {
            Some(tag) => Quantity {
                value: p.value,
                tag,
            },
            None => {
                let q = Quantity::untagged(p.value);
                match p.unit {
                    Some(want) => q.retag(want, Span::new(0, p.expression.len())).unwrap_or(q),
                    None => q,
                }
            }
        };
        env.entry(p.name.clone()).or_insert(q);
    }
    env
}

/// Turn an instance's `name → magnitude` overrides into pins for
/// [`evaluate_table`], and name every override that addresses nothing.
///
/// The magnitude is a working-space number (mm for a length, degrees for an
/// angle — the same space `value_mm` reports), because that is what
/// `Instance.parameter_overrides` has always been declared to hold. Its
/// DIMENSION comes from the parameter being overridden, never from the
/// override: a declared `unit` commits it (and a `Count` override of `20.5`
/// is refused by name, through the same `retag` the table itself uses), and
/// otherwise the dimension the row's own expression produces is kept, so
/// overriding `width = "2cm"` with `30` still yields a length.
///
/// An override naming a parameter the part does not declare is a loud error,
/// not a new parameter. An instance cannot introduce a variable the part has
/// no field reading, so the only thing such an override can be is a typo or a
/// name the part has since renamed — and silently accepting it would leave
/// the instance looking parameterised while building the default geometry.
fn pin_overrides(
    params: &mut [DesignParameter],
    outer: &Env,
    overrides: &std::collections::BTreeMap<String, f64>,
) -> (Pinned, Vec<String>) {
    // Pass one, as written: a row with no declared unit only reveals its
    // dimension by evaluating. Cheap (a parse per row) and it also leaves
    // `value`/`error` correct for the rows no override touches.
    let plain = evaluate_table(params, outer, &HashMap::new(), None, &HashMap::new());
    let mut pinned = Pinned::new();
    let mut errs = Vec::new();
    for (name, value) in overrides {
        let Some(p) = params.iter().find(|p| p.name == *name) else {
            errs.push(format!(
                "override of '{name}': this part declares no parameter of that name"
            ));
            continue;
        };
        let q = Quantity::untagged(*value);
        let tag = match p.unit {
            Some(unit) => match q.retag(unit, Span::new(0, 0)) {
                Ok(q) => q.tag,
                Err(e) => {
                    errs.push(format!("override of '{name}': {e}"));
                    continue;
                }
            },
            // No declared unit: keep whatever the row's own expression
            // committed, and nothing if it committed nothing.
            None => plain.get(name).map(|q| q.tag).unwrap_or(Tag::Untagged),
        };
        pinned.insert(name.clone(), Quantity { value: *value, tag });
    }
    (pinned, errs)
}

/// What a parameter table SAYS, with every derived field left out: name,
/// expression and declared unit, in table order.
///
/// Two tables with the same signature drive identical geometry, and that is
/// the question a geometry cache has to answer. `value`, `error` and `tag`
/// are the last evaluation's output, so comparing whole rows would call two
/// identical tables different (and, worse, call a table that has since been
/// re-evaluated the same as one that has not).
pub fn table_signature(params: &[DesignParameter]) -> Vec<(String, String, Option<Dimension>)> {
    params
        .iter()
        .map(|p| (p.name.clone(), p.expression.clone(), p.unit))
        .collect()
}

/// What an apply pass evaluates against: the parameter values, plus the
/// model when there is one to measure (D2).
///
/// One struct rather than two arguments through five functions, and it is
/// the place the `Option<&TreeMeasurer>` to `Option<&dyn Measurer>` coercion
/// happens, so no call site repeats it.
pub struct ApplyEnv<'a> {
    pub env: Env,
    /// `None` is a pass with no model behind it — the pre-rebuild pass, and
    /// every caller that has no kernel. A measurement is then a typed
    /// refusal, never a guessed number.
    pub measurer: Option<&'a crate::measure::TreeMeasurer<'a>>,
}

impl ApplyEnv<'_> {
    /// Evaluate one expression, measurements included.
    fn eval(&self, expression: &str) -> Result<Quantity, ExprError> {
        expr::eval_with(
            &expr::parse(expression)?,
            &self.env,
            self.measurer.map(|m| m as &dyn Measurer),
        )
    }

    /// Whether this pass must DEFER `expression` instead of evaluating it:
    /// it measures the model and this pass has no model.
    ///
    /// Deferred SILENTLY — no error, no change, the field keeps the value
    /// the last measuring pass gave it. The alternative, reporting "no
    /// geometry is available here", would put a permanent error on a
    /// perfectly correct document every time the pre-rebuild pass ran.
    /// `Engine::rebuild`'s measuring pass is where these are evaluated and
    /// where their errors belong.
    fn defers(&self, expression: &str) -> bool {
        self.measurer.is_none() && expr::measures(expression)
    }

    /// Position the measurer at the feature whose expressions are about to
    /// be evaluated — the ordering rule's floor (see
    /// `crate::measure::TreeMeasurer`). A no-op without a measurer.
    fn at_feature(&self, index: usize) {
        if let Some(m) = self.measurer {
            m.set_floor(Some(index));
        }
    }
}

/// Evaluate + apply all expressions on the tree, with no model: a
/// measurement function (D2) is a typed per-field error. See module docs.
pub fn apply_parameters(tree: &mut FeatureTree) -> ParamOutcome {
    apply_parameters_with(tree, None)
}

/// [`apply_parameters`], answering measurement functions through `measurer`
/// (D2, `specs/drawings_and_mbd.md` §6).
///
/// The measurer must NOT borrow `tree` (it holds owned copies of the name
/// tables for exactly that reason), because this pass writes to it.
pub fn apply_parameters_with(
    tree: &mut FeatureTree,
    measurer: Option<&crate::measure::TreeMeasurer<'_>>,
) -> ParamOutcome {
    apply_parameters_scoped(tree, &mut [], None, measurer)
}

/// [`apply_parameters_with`] with the two outer scopes P2 adds.
///
/// `document` is the document-level table (`DocumentMetadata.parameters`):
/// evaluated first, in its own scope, and seen by the tree's table and by
/// every expression field. Its rows' `value`/`error` are refreshed in place,
/// so the caller holding the document table reads the same answer the
/// rebuild computed rather than evaluating it a second time.
///
/// A document parameter gets NO measurer, deliberately, even on a measuring
/// pass: a measurement resolves through entity NAMES, which are per-tab
/// (`FeatureTree.names`), and the document scope sits above the tabs — there
/// is no answer to which tab's `plate.top_face` it would mean. So `volume(…)`
/// in a document parameter is the same typed refusal it is in any pass with
/// no model, and the place to write it is the tab that owns the geometry.
///
/// `overrides` is the instance's `parameter_overrides` when this tree is
/// being built as one placed occurrence of a part (`None` for the Part tab
/// itself). It pins magnitudes on the tree's OWN table, so one part
/// definition builds N different solids.
pub fn apply_parameters_scoped(
    tree: &mut FeatureTree,
    document: &mut [DesignParameter],
    overrides: Option<&std::collections::BTreeMap<String, f64>>,
    measurer: Option<&crate::measure::TreeMeasurer<'_>>,
) -> ParamOutcome {
    let mut outcome = ParamOutcome::default();

    // The document scope first and on its own: it is ABOVE the tabs, so it
    // cannot read a Part's parameters (which Part would it mean?) and it
    // cannot measure one either (see the note above).
    let doc_env = evaluate_parameters(document);
    for p in document.iter() {
        if let Some(err) = &p.error {
            outcome
                .errors
                .push((p.id, format!("document parameter '{}': {}", p.name, err)));
        }
    }

    let mut override_errors: Vec<String> = Vec::new();
    let pinned = match overrides.filter(|o| !o.is_empty()) {
        Some(o) => {
            let (pinned, errs) = pin_overrides(&mut tree.parameters, &doc_env, o);
            override_errors = errs;
            pinned
        }
        None => Pinned::new(),
    };

    // Where each parameter is first read, so a MEASURING parameter can be
    // positioned (it has no index of its own). Computed before anything is
    // evaluated, from the parser, so it does not depend on this pass.
    let floors = if measurer.is_some() {
        earliest_readers(tree)
    } else {
        HashMap::new()
    };
    let env = ApplyEnv {
        env: evaluate_table(&mut tree.parameters, &doc_env, &pinned, measurer, &floors),
        measurer,
    };
    for p in &tree.parameters {
        if let Some(err) = &p.error {
            outcome
                .errors
                .push((p.id, format!("parameter '{}': {}", p.name, err)));
        }
    }
    // An override that addresses nothing belongs to no parameter and no
    // feature, so it is reported against the nil id — the same channel the
    // apply pass already uses for a table-level complaint.
    for e in override_errors {
        outcome.errors.push((Uuid::nil(), e));
    }

    for (idx, feature) in tree.features.iter_mut().enumerate() {
        let mut errs: Vec<String> = Vec::new();
        // D2: every measurement in this feature's own expressions may read
        // only geometry EARLIER than this index.
        env.at_feature(idx);
        let changed = match &mut feature.operation {
            Operation::Extrude { params } => {
                let mut changed = apply_field(
                    "depth",
                    Dimension::Length,
                    &mut params.depth,
                    params.depth_expr.as_deref(),
                    &env,
                    &mut errs,
                );
                if let Some(crate::types::SecondDirection::Blind { depth, depth_expr }) =
                    params.second_direction.as_mut()
                {
                    let expression = depth_expr.clone();
                    changed |= apply_field(
                        "second depth",
                        Dimension::Length,
                        depth,
                        expression.as_deref(),
                        &env,
                        &mut errs,
                    );
                }
                changed
            }
            Operation::Revolve { params } => {
                let mut changed = apply_field(
                    "angle",
                    Dimension::Angle,
                    &mut params.angle,
                    params.angle_expr.as_deref(),
                    &env,
                    &mut errs,
                );
                let expressions = params.axis_origin_expr.clone();
                changed |= apply_vec3_field(
                    "axis origin",
                    Dimension::Length,
                    &mut params.axis_origin,
                    expressions.as_ref(),
                    &env,
                    &mut errs,
                );
                changed
            }
            Operation::Pipe { params } => {
                let a = apply_field(
                    "radius",
                    Dimension::Length,
                    &mut params.radius,
                    params.radius_expr.as_deref(),
                    &env,
                    &mut errs,
                );
                let b = match (
                    &mut params.inner_radius,
                    params.inner_radius_expr.as_deref(),
                ) {
                    (Some(ri), expr @ Some(_)) => {
                        apply_field("inner_radius", Dimension::Length, ri, expr, &env, &mut errs)
                    }
                    (None, Some(expr)) => {
                        let mut v = 0.0;
                        let changed = apply_field(
                            "inner_radius",
                            Dimension::Length,
                            &mut v,
                            Some(expr),
                            &env,
                            &mut errs,
                        );
                        if changed {
                            params.inner_radius = Some(v);
                        }
                        changed
                    }
                    _ => false,
                };
                a || b
            }
            Operation::DatumPlane { params } => match &mut params.definition {
                PlaneDefinition::Offset {
                    distance,
                    distance_expr,
                    ..
                }
                | PlaneDefinition::OffsetFromFace {
                    distance,
                    distance_expr,
                    ..
                } => {
                    let expr = distance_expr.clone();
                    apply_field(
                        "distance",
                        Dimension::Length,
                        distance,
                        expr.as_deref(),
                        &env,
                        &mut errs,
                    )
                }
                PlaneDefinition::PointNormal {
                    origin,
                    origin_expr,
                    ..
                } => {
                    let expressions = origin_expr.clone();
                    apply_vec3_field(
                        "origin",
                        Dimension::Length,
                        origin,
                        expressions.as_ref(),
                        &env,
                        &mut errs,
                    )
                }
            },
            Operation::Sketch { sketch } => apply_sketch(sketch, &env, &mut errs),
            Operation::Sketch3d { sketch } => apply_sketch3d(sketch, &env, &mut errs),
            Operation::PatternCircular { params } => {
                let mut changed = apply_field(
                    "angle",
                    Dimension::Angle,
                    &mut params.angle_deg,
                    params.angle_expr.as_deref(),
                    &env,
                    &mut errs,
                );
                changed |= apply_count_field(
                    "count",
                    &mut params.count,
                    params.count_expr.as_deref(),
                    &env,
                    &mut errs,
                );
                changed |= apply_axis(&mut params.axis, "axis", &env, &mut errs);
                changed
            }
            Operation::Script { params } => {
                // Expression-driven script arguments: evaluate each into the
                // raw (mm-space / degrees / plain) cache. The declared type
                // lives in the script's header, which is in the document's
                // sources table and not reachable from here, so the DIMENSION
                // travels with the value in `arg_dimensions` and the script
                // layer checks it against the `@param` type it declared.
                let mut changed = false;
                for (name, expression) in &params.arg_exprs {
                    if env.defers(expression) {
                        continue;
                    }
                    match env.eval(expression) {
                        Ok(q) => {
                            if params.arg_values.get(name) != Some(&q.value) {
                                params.arg_values.insert(name.clone(), q.value);
                                changed = true;
                            }
                            params.arg_dimensions.insert(name.clone(), q.tag);
                        }
                        Err(e) => errs.push(format!("{name} expression '{expression}': {e}")),
                    }
                }
                changed
            }
            Operation::PatternLinear { params } => {
                let mut changed = apply_field(
                    "spacing",
                    Dimension::Length,
                    &mut params.spacing,
                    params.spacing_expr.as_deref(),
                    &env,
                    &mut errs,
                );
                changed |= apply_count_field(
                    "count",
                    &mut params.count,
                    params.count_expr.as_deref(),
                    &env,
                    &mut errs,
                );
                changed |= apply_axis(&mut params.direction, "direction", &env, &mut errs);
                if let Some(second) = params.second.as_mut() {
                    let expr = second.spacing_expr.clone();
                    changed |= apply_field(
                        "second spacing",
                        Dimension::Length,
                        &mut second.spacing,
                        expr.as_deref(),
                        &env,
                        &mut errs,
                    );
                    changed |= apply_count_field(
                        "second count",
                        &mut second.count,
                        second.count_expr.clone().as_deref(),
                        &env,
                        &mut errs,
                    );
                    changed |=
                        apply_axis(&mut second.direction, "second direction", &env, &mut errs);
                }
                changed
            }
            Operation::PatternMirror { params } => {
                apply_axis(&mut params.plane, "plane", &env, &mut errs)
            }
            Operation::MateConnector { params } => {
                let mut changed = apply_field(
                    "rotation",
                    Dimension::Angle,
                    &mut params.rotation_deg,
                    params.rotation_expr.as_deref(),
                    &env,
                    &mut errs,
                );
                let expressions = params.offset_m_expr.clone();
                changed |= apply_vec3_field(
                    "offset",
                    Dimension::Length,
                    &mut params.offset_m,
                    expressions.as_ref(),
                    &env,
                    &mut errs,
                );
                changed
            }
            Operation::ImportedBody { params } => {
                let translation = params.translation_m_expr.clone();
                let mut changed = apply_vec3_field(
                    "translation",
                    Dimension::Length,
                    &mut params.translation_m,
                    translation.as_ref(),
                    &env,
                    &mut errs,
                );
                let rotation = params.rotation_deg_expr.clone();
                changed |= apply_vec3_field(
                    "rotation",
                    Dimension::Angle,
                    &mut params.rotation_deg,
                    rotation.as_ref(),
                    &env,
                    &mut errs,
                );
                changed |= apply_field(
                    "scale",
                    Dimension::Ratio,
                    &mut params.scale,
                    params.scale_expr.as_deref(),
                    &env,
                    &mut errs,
                );
                changed
            }
            // Fillet/chamfer/shell are deferred (disabled in the UI);
            // booleans carry no dimension measurements, and a sweep's
            // section and path are both sketches (which carry their own).
            _ => false,
        };
        if changed {
            outcome.first_changed = Some(outcome.first_changed.map_or(idx, |c| c.min(idx)));
            outcome.changed.push(feature.id);
        }
        for e in errs {
            outcome
                .errors
                .push((feature.id, format!("{}: {}", feature.name, e)));
        }
    }

    outcome
}

/// Evaluate a 3D sketch's driving expressions into its stored values.
///
/// A 3D sketch has no solver to re-run afterwards (`specs/sketch3d.md` §4),
/// so unlike [`apply_sketch`] this is only the expression pass: point
/// coordinates in mm-space and fillet radii the same. Attachments are NOT
/// resolved here — they can need model geometry, so they wait for the rebuild
/// walk (`crate::sketch3d`).
fn apply_sketch3d(
    sketch: &mut waffle_types::sketch3d::Sketch3d,
    env: &ApplyEnv<'_>,
    errs: &mut Vec<String>,
) -> bool {
    use waffle_types::sketch3d::Sketch3dEntity;
    let mut changed = false;
    for entity in &mut sketch.entities {
        match entity {
            Sketch3dEntity::Point {
                id, xyz, xyz_expr, ..
            } => {
                let Some(exprs) = xyz_expr.clone() else {
                    continue;
                };
                for (axis, expression) in exprs.iter().enumerate() {
                    let Some(expression) = expression.as_deref() else {
                        continue;
                    };
                    changed |= apply_field(
                        &format!("point {id} {}", ["x", "y", "z"][axis]),
                        Dimension::Length,
                        &mut xyz[axis],
                        Some(expression),
                        env,
                        errs,
                    );
                }
            }
            Sketch3dEntity::Fillet {
                id,
                radius,
                radius_expr,
                ..
            } => {
                let expression = radius_expr.clone();
                changed |= apply_field(
                    &format!("fillet {id} radius"),
                    Dimension::Length,
                    radius,
                    expression.as_deref(),
                    env,
                    errs,
                );
            }
            Sketch3dEntity::Line { .. } | Sketch3dEntity::Arc { .. } => {}
        }
    }
    changed
}

/// Evaluate an expression into `field`, accepting it for `dimension` — the
/// typed boundary. `Length` writes METERS, `Angle` writes DEGREES (what the
/// fields store), `Count`/`Ratio` the plain number. Returns true if the
/// value changed; any error (including a dimension the field cannot take,
/// such as `25deg` in a depth) leaves the field untouched and is reported.
fn apply_field(
    label: &str,
    dimension: Dimension,
    field: &mut f64,
    expression: Option<&str>,
    env: &ApplyEnv<'_>,
    errs: &mut Vec<String>,
) -> bool {
    let Some(expression) = expression else {
        return false;
    };
    if env.defers(expression) {
        return false;
    }
    match env.eval(expression).and_then(|q| q.accept(dimension)) {
        Ok(v) => {
            if v != *field {
                *field = v;
                true
            } else {
                false
            }
        }
        Err(e) => {
            errs.push(format!("{label} expression '{expression}': {e}"));
            false
        }
    }
}

/// [`apply_field`] per component of a `[f64; 3]` (P3).
///
/// Each component is its own optional expression, so `x` can be driven while
/// `y` and `z` stay as drawn — an author who parameterises one axis has said
/// nothing about the other two, and filling them in from somewhere would be
/// inventing geometry. The label names the component (`translation x`), which
/// is what makes a dimension refusal readable: "translation z expression
/// '25deg'" says which of three numbers is wrong.
///
/// Only POSITION-like vectors go through this. A direction vector (an axis's
/// `direction`, a plane's `normal`, a sketch's in-plane x) has no sidecar at
/// all: it is normalized at rebuild, so a per-component expression for it
/// drives nothing an author can predict — two thirds of what they typed is
/// scaled away.
fn apply_vec3_field(
    label: &str,
    dimension: Dimension,
    field: &mut [f64; 3],
    expressions: Option<&[Option<String>; 3]>,
    env: &ApplyEnv<'_>,
    errs: &mut Vec<String>,
) -> bool {
    let Some(expressions) = expressions else {
        return false;
    };
    let mut changed = false;
    for (axis, expression) in expressions.iter().enumerate() {
        changed |= apply_field(
            &format!("{label} {}", ["x", "y", "z"][axis]),
            dimension,
            &mut field[axis],
            expression.as_deref(),
            env,
            errs,
        );
    }
    changed
}

/// [`apply_field`] for a `u32` COUNT (P3).
///
/// `Dimension::Count` is what makes this safe to parameterise: the boundary
/// demands a whole, non-negative number, so `teeth / 2` of a 20-tooth gear is
/// 10 and `teeth / 3` is a loud refusal rather than a silent truncation to 6.
/// A count that does not fit a `u32` is refused for the same reason — a
/// saturating cast is a wrong answer with no complaint.
///
/// The operation's OWN validator still runs afterwards (`pattern::check_count`
/// wants ≥ 2): this boundary decides whether the expression produced a count
/// at all, not whether the count is usable.
fn apply_count_field(
    label: &str,
    field: &mut u32,
    expression: Option<&str>,
    env: &ApplyEnv<'_>,
    errs: &mut Vec<String>,
) -> bool {
    let Some(expression) = expression else {
        return false;
    };
    // A count can measure the model too (D2): `count_expr = "length(rail) /
    // 50"`. Deferred on a pass with no model, like every other field.
    if env.defers(expression) {
        return false;
    }
    match env
        .eval(expression)
        .and_then(|q| q.accept(Dimension::Count))
    {
        Ok(v) => {
            if v > u32::MAX as f64 {
                errs.push(format!(
                    "{label} expression '{expression}': {v} is too large for a count"
                ));
                return false;
            }
            let v = v as u32;
            if v != *field {
                *field = v;
                true
            } else {
                false
            }
        }
        Err(e) => {
            errs.push(format!("{label} expression '{expression}': {e}"));
            false
        }
    }
}

/// An [`crate::types::AxisRef`]'s driving expressions: the explicit origin's
/// three components (P3).
///
/// An `Entity` axis has none — it is derived from the picked geometry every
/// rebuild, and an expression beside it would be a second driver that the
/// resolution overwrites.
fn apply_axis(
    axis: &mut crate::types::AxisRef,
    label: &str,
    env: &ApplyEnv<'_>,
    errs: &mut Vec<String>,
) -> bool {
    match axis {
        crate::types::AxisRef::Explicit {
            origin,
            origin_expr,
            ..
        } => {
            let expressions = origin_expr.clone();
            apply_vec3_field(
                &format!("{label} origin"),
                Dimension::Length,
                origin,
                expressions.as_ref(),
                env,
                errs,
            )
        }
        crate::types::AxisRef::Entity { .. } => false,
    }
}

/// Re-evaluate a sketch's expression-driven dimensions; if any value changed —
/// or the sketch has never been solved (`SolveStatus::Unsolved`, v4 §2.10:
/// written by a tool that did not run the solver) — solve it from its current
/// geometry and recompute derived data. Returns true if the sketch's geometry
/// or status was updated.
fn apply_sketch(
    sketch: &mut waffle_types::Sketch,
    env: &ApplyEnv<'_>,
    errs: &mut Vec<String>,
) -> bool {
    // Pass 1: evaluate every expression-driven dimension, recording previous
    // values so a failed solve can restore a consistent sketch.
    let mut changed: Vec<(usize, f64)> = Vec::new(); // (constraint idx, old value)
    for (i, c) in sketch.constraints.iter_mut().enumerate() {
        let Some(expression) = c.expression().map(str::to_string) else {
            continue;
        };
        let Some(unit) = c.dimension_unit() else {
            continue;
        };
        if env.defers(&expression) {
            continue;
        }
        // The constraint's own unit IS the dimension it asks for: a length
        // dim stores meters, an angle dim degrees. A `25deg` on a length
        // dimension is refused here rather than read as 25 mm.
        let dimension = sketch_dimension(unit);
        match env.eval(&expression).and_then(|q| q.accept(dimension)) {
            Ok(new_value) => {
                let old = c.dimension_value().unwrap_or(0.0);
                if new_value != old {
                    c.set_dimension_value(new_value);
                    changed.push((i, old));
                }
            }
            Err(e) => errs.push(format!("dimension expression '{expression}': {e}")),
        }
    }
    let first_solve = matches!(sketch.solve_status, SolveStatus::Unsolved);
    if changed.is_empty() && !first_solve {
        return false;
    }

    // (Re-)solve with DRIVING constraints only (reference dims display, never
    // constrain — same filter the sketch UI applies before solving).
    let mut solve_input = sketch.clone();
    solve_input.constraints.retain(|c| !c.is_reference());
    let solved = sketch_solver::solve_sketch(&solve_input);

    match solved.status {
        SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. } => {
            // Write the solution back into the entities, then recompute
            // derived data from them (the projected-sketch rebuild pattern:
            // positions + profiles re-derive from entity state).
            for e in &mut sketch.entities {
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
            sketch.solve_status = solved.status;
            sketch.solved_positions.clear();
            sketch.solved_profiles.clear();
            sketch.recompute_derived();
            true
        }
        SolveStatus::OverConstrained { .. } | SolveStatus::SolveFailed { .. } => {
            // Loud STOP: keep the sketch consistent by restoring the previous
            // dimension values; the error names the failed solve.
            for (i, old) in changed {
                sketch.constraints[i].set_dimension_value(old);
            }
            let reason = match &solved.status {
                SolveStatus::SolveFailed { reason } => reason.clone(),
                _ => "over-constrained".to_string(),
            };
            if first_solve {
                // A first solve has no previous good state to fall back to:
                // record the failed status (the sketch is no longer
                // `Unsolved`) and derive what geometry there is, so the
                // failure is visible in the file and the UI, not retried
                // silently on every rebuild.
                sketch.solve_status = solved.status;
                sketch.solved_positions.clear();
                sketch.solved_profiles.clear();
                sketch.recompute_derived();
                errs.push(format!("sketch's first solve failed ({reason})"));
                true
            } else {
                errs.push(format!(
                    "sketch re-solve failed after applying dimension expressions ({reason}); \
                     previous dimensions kept"
                ));
                false
            }
        }
        // The solver never returns `Unsolved`; treat it as a failed solve so
        // the status cannot silently stay unsolved.
        SolveStatus::Unsolved => {
            for (i, old) in changed {
                sketch.constraints[i].set_dimension_value(old);
            }
            errs.push("sketch solve returned no status".to_string());
            false
        }
    }
}

// ── P5: the parameter table as data ──────────────────────────────────────
//
// `specs/agent_mechanical_design.md` §6 P5. Two questions an agent (and the
// panel) must be able to ask without evaluating anything itself: what does
// this parameter depend on, and who depends on it. Both are answered from
// the AST P1 introduced — `Expr::identifiers` for a dependency, and the
// reverse of it for a dependent.
//
// Everything below shares ONE enumeration of where an expression can live
// ([`expression_sites`]). The apply pass above keeps its own traversal
// because it also needs each field's VALUE slot and the sketch re-solve, so
// the two lists could drift; `every_expression_field_is_enumerated` in the
// tests below is the oracle that says they have not.

/// One place an expression lives on the feature tree.
pub struct ExprSite<'a> {
    /// The feature that owns it.
    pub feature: Uuid,
    /// That feature's display name — what a person reads in the answer.
    pub feature_name: String,
    /// Which field, named the way the apply pass names it in an error:
    /// `depth`, `angle`, `inner_radius`, `point 7 x`, `dimension #3`,
    /// `arg teeth`.
    pub field: String,
    /// The expression source. Writable, because a parameter rename splices
    /// it (see [`rename_parameter`]).
    pub expression: &'a mut String,
}

/// Every expression field of one operation, in a stable order.
///
/// THE enumeration: the reverse index and the rename rewrite both walk this,
/// so neither can learn about a field the other does not know. Only fields
/// that actually CARRY an expression are yielded — an absent sidecar is not
/// a site, and a rewrite must never invent one.
/// The set components of a `[f64; 3]` sidecar, labelled per axis — the
/// enumeration half of [`apply_vec3_field`]. An unset component is NOT a
/// site: an absent sidecar is not an expression, and a rewrite must never
/// invent one.
fn push_vec3<'a>(
    out: &mut Vec<(String, &'a mut String)>,
    label: &str,
    expressions: Option<&'a mut [Option<String>; 3]>,
) {
    let Some(expressions) = expressions else {
        return;
    };
    for (axis, expression) in expressions.iter_mut().enumerate() {
        if let Some(e) = expression.as_mut() {
            out.push((format!("{label} {}", ["x", "y", "z"][axis]), e));
        }
    }
}

/// An axis reference's expression sites — the enumeration half of
/// [`apply_axis`].
fn push_axis<'a>(
    out: &mut Vec<(String, &'a mut String)>,
    label: &str,
    axis: &'a mut crate::types::AxisRef,
) {
    match axis {
        crate::types::AxisRef::Explicit { origin_expr, .. } => {
            push_vec3(out, &format!("{label} origin"), origin_expr.as_mut())
        }
        crate::types::AxisRef::Entity { .. } => {}
    }
}

fn expression_sites(op: &mut Operation) -> Vec<(String, &mut String)> {
    use waffle_types::sketch3d::Sketch3dEntity;
    let mut out: Vec<(String, &mut String)> = Vec::new();
    match op {
        Operation::Extrude { params } => {
            if let Some(e) = params.depth_expr.as_mut() {
                out.push(("depth".to_string(), e));
            }
            if let Some(crate::types::SecondDirection::Blind {
                depth_expr: Some(e),
                ..
            }) = params.second_direction.as_mut()
            {
                out.push(("second depth".to_string(), e));
            }
        }
        Operation::Revolve { params } => {
            if let Some(e) = params.angle_expr.as_mut() {
                out.push(("angle".to_string(), e));
            }
            push_vec3(&mut out, "axis origin", params.axis_origin_expr.as_mut());
        }
        Operation::Pipe { params } => {
            if let Some(e) = params.radius_expr.as_mut() {
                out.push(("radius".to_string(), e));
            }
            if let Some(e) = params.inner_radius_expr.as_mut() {
                out.push(("inner_radius".to_string(), e));
            }
        }
        Operation::DatumPlane { params } => match &mut params.definition {
            PlaneDefinition::Offset { distance_expr, .. }
            | PlaneDefinition::OffsetFromFace { distance_expr, .. } => {
                if let Some(e) = distance_expr.as_mut() {
                    out.push(("distance".to_string(), e));
                }
            }
            PlaneDefinition::PointNormal { origin_expr, .. } => {
                push_vec3(&mut out, "origin", origin_expr.as_mut());
            }
        },
        Operation::Sketch { sketch } => {
            for (i, c) in sketch.constraints.iter_mut().enumerate() {
                // A constraint with no `dimension_unit` drives nothing, so
                // its expression (if any) is not a site either — the apply
                // pass skips it for the same reason.
                if c.dimension_unit().is_none() {
                    continue;
                }
                if let Some(e) = c.expression_mut() {
                    out.push((format!("dimension #{i}"), e));
                }
            }
        }
        Operation::Sketch3d { sketch } => {
            for entity in &mut sketch.entities {
                match entity {
                    Sketch3dEntity::Point { id, xyz_expr, .. } => {
                        let id = *id;
                        if let Some(exprs) = xyz_expr.as_mut() {
                            for (axis, expression) in exprs.iter_mut().enumerate() {
                                if let Some(e) = expression.as_mut() {
                                    out.push((format!("point {id} {}", ["x", "y", "z"][axis]), e));
                                }
                            }
                        }
                    }
                    Sketch3dEntity::Fillet {
                        id, radius_expr, ..
                    } => {
                        let id = *id;
                        if let Some(e) = radius_expr.as_mut() {
                            out.push((format!("fillet {id} radius"), e));
                        }
                    }
                    Sketch3dEntity::Line { .. } | Sketch3dEntity::Arc { .. } => {}
                }
            }
        }
        Operation::PatternCircular { params } => {
            if let Some(e) = params.angle_expr.as_mut() {
                out.push(("angle".to_string(), e));
            }
            if let Some(e) = params.count_expr.as_mut() {
                out.push(("count".to_string(), e));
            }
            push_axis(&mut out, "axis", &mut params.axis);
        }
        Operation::PatternMirror { params } => {
            push_axis(&mut out, "plane", &mut params.plane);
        }
        Operation::MateConnector { params } => {
            if let Some(e) = params.rotation_expr.as_mut() {
                out.push(("rotation".to_string(), e));
            }
            push_vec3(&mut out, "offset", params.offset_m_expr.as_mut());
        }
        Operation::ImportedBody { params } => {
            push_vec3(&mut out, "translation", params.translation_m_expr.as_mut());
            push_vec3(&mut out, "rotation", params.rotation_deg_expr.as_mut());
            if let Some(e) = params.scale_expr.as_mut() {
                out.push(("scale".to_string(), e));
            }
        }
        Operation::PatternLinear { params } => {
            if let Some(e) = params.spacing_expr.as_mut() {
                out.push(("spacing".to_string(), e));
            }
            if let Some(e) = params.count_expr.as_mut() {
                out.push(("count".to_string(), e));
            }
            push_axis(&mut out, "direction", &mut params.direction);
            if let Some(second) = params.second.as_mut() {
                if let Some(e) = second.spacing_expr.as_mut() {
                    out.push(("second spacing".to_string(), e));
                }
                if let Some(e) = second.count_expr.as_mut() {
                    out.push(("second count".to_string(), e));
                }
                push_axis(&mut out, "second direction", &mut second.direction);
            }
        }
        Operation::Script { params } => {
            // `arg_exprs` is a BTreeMap, so this order is deterministic.
            for (name, expression) in params.arg_exprs.iter_mut() {
                out.push((format!("arg {name}"), expression));
            }
        }
        // Fillet/chamfer/shell are deferred; booleans, imports and the
        // remaining operations carry no expression-driven measurement.
        // A field added to the apply pass above MUST be added here too —
        // `every_expression_field_is_enumerated` fails if it is not.
        _ => {}
    }
    out
}

/// Visit every expression on the tree, in feature order.
///
/// Takes `&mut` because the rename rewrite needs it; a read-only caller
/// (the reverse index) simply does not write through the site.
pub fn visit_expressions(tree: &mut FeatureTree, mut visit: impl FnMut(ExprSite<'_>)) {
    for feature in &mut tree.features {
        let feature = &mut *feature;
        let id = feature.id;
        let name = feature.name.clone();
        for (field, expression) in expression_sites(&mut feature.operation) {
            visit(ExprSite {
                feature: id,
                feature_name: name.clone(),
                field,
                expression,
            });
        }
    }
}

/// One feature field that reads design parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldUse {
    /// The owning feature.
    pub feature: Uuid,
    /// Its display name.
    pub feature_name: String,
    /// The field, as [`ExprSite::field`] names it.
    pub field: String,
    /// The expression, verbatim.
    pub expression: String,
    /// The parameter names it reads, sorted. EMPTY when the expression does
    /// not parse — an unparseable expression has no dependency list, and the
    /// field's own error (from the apply pass) is where that is reported.
    pub reads: Vec<String>,
}

/// Every expression field on the tree with the parameter names it reads —
/// the reverse index behind P5's `used_by`.
///
/// Computed on demand rather than cached on the engine: the answer is a pure
/// function of the tree, and a cache refreshed "every rebuild" is a cache
/// that is wrong whenever something reaches this without one.
pub fn field_uses(tree: &mut FeatureTree) -> Vec<FieldUse> {
    let mut out = Vec::new();
    visit_expressions(tree, |site| {
        let reads = expr::dependencies(site.expression)
            .map(|ids| ids.into_iter().collect())
            .unwrap_or_default();
        out.push(FieldUse {
            feature: site.feature,
            feature_name: site.feature_name,
            field: site.field,
            expression: site.expression.clone(),
            reads,
        });
    });
    out
}

// ── D2: measurement sites and their ordering ─────────────────────────────
//
// `specs/drawings_and_mbd.md` §6. A measuring expression cannot be evaluated
// before the feature it measures has been built, so the parameter pass runs
// twice: once before the rebuild (no measurer, every measurement a typed
// refusal that leaves the field alone) and again after it, with the model in
// hand. `Engine::rebuild` drives that loop; what is here is the inventory it
// needs and the ordering rule the loop's termination rests on.

/// One place on the tree where an expression reads the MODEL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasurementSite {
    /// The owning feature, or `None` for a design parameter (which has no
    /// position in the tree).
    pub feature: Option<Uuid>,
    /// The field as [`ExprSite::field`] names it, or the parameter's name.
    pub field: String,
    /// The expression, verbatim.
    pub expression: String,
    /// The entity names it measures, sorted.
    pub entities: Vec<String>,
}

/// Every expression on the tree — parameters included — that reads the
/// model.
///
/// Walks the same [`expression_sites`] enumeration as everything else, so a
/// field added there is a measurement site for free.
pub fn measurement_sites(tree: &mut FeatureTree) -> Vec<MeasurementSite> {
    let mut out: Vec<MeasurementSite> = tree
        .parameters
        .iter()
        .filter_map(|p| {
            // `expr::measures` first: it answers without parsing for an
            // expression with no call in it, which is every expression in
            // every document that does not measure. This walk runs at the
            // start of every rebuild.
            if !expr::measures(&p.expression) {
                return None;
            }
            let ast = expr::parse(&p.expression).ok()?;
            Some(MeasurementSite {
                feature: None,
                field: p.name.clone(),
                expression: p.expression.clone(),
                entities: ast.entity_references().into_iter().collect(),
            })
        })
        .collect();
    visit_expressions(tree, |site| {
        if !expr::measures(site.expression) {
            return;
        }
        let Ok(ast) = expr::parse(site.expression) else {
            return;
        };
        out.push(MeasurementSite {
            feature: Some(site.feature),
            field: site.field,
            expression: site.expression.clone(),
            entities: ast.entity_references().into_iter().collect(),
        });
    });
    out
}

/// Whether anything on the tree reads the model.
///
/// The question `Engine::rebuild` asks before it builds a measurer at all,
/// so a document with no measurements pays nothing for D2.
pub fn tree_measures(tree: &mut FeatureTree) -> bool {
    if tree
        .parameters
        .iter()
        .any(|p| expr::measures(&p.expression))
    {
        return true;
    }
    let mut found = false;
    visit_expressions(tree, |site| {
        found |= expr::measures(site.expression);
    });
    found
}

/// The earliest feature index that reads each parameter — directly, or
/// through another parameter.
///
/// This is how a MEASURING parameter is positioned for the ordering rule: it
/// has no index of its own, so the constraint belongs to whoever reads it. A
/// parameter first read by feature #3 may measure #0, #1 and #2; one that
/// measures #5 and is read by #1 is a cycle, because feature #1's geometry
/// would then depend on feature #5's, which depends on #1's.
///
/// Indexes are into `FeatureTree::features` (not `active_features`):
/// suppression and rollback change which features BUILD, not the order the
/// author wrote them in, and the ordering rule is about that order.
pub fn earliest_readers(tree: &mut FeatureTree) -> HashMap<String, usize> {
    let index_of: HashMap<Uuid, usize> = tree
        .features
        .iter()
        .enumerate()
        .map(|(i, f)| (f.id, i))
        .collect();
    let mut earliest: HashMap<String, usize> = HashMap::new();
    for use_ in field_uses(tree) {
        let Some(&at) = index_of.get(&use_.feature) else {
            continue;
        };
        for name in use_.reads {
            let slot = earliest.entry(name).or_insert(at);
            *slot = (*slot).min(at);
        }
    }
    // Propagate backwards along parameter → parameter edges: if `b` is read
    // by feature #2 and `b` reads `a`, then `a` is read by #2 too. A
    // fixpoint, which terminates because every step either adds a name or
    // lowers an index that is bounded below by zero.
    loop {
        let mut progressed = false;
        for p in &tree.parameters {
            let Some(&at) = earliest.get(&p.name) else {
                continue;
            };
            for dep in expr::dependencies(&p.expression).unwrap_or_default() {
                if earliest.get(&dep).is_none_or(|&cur| cur > at) {
                    earliest.insert(dep, at);
                    progressed = true;
                }
            }
        }
        if !progressed {
            break;
        }
    }
    earliest
}

/// One expression field's text, addressed by feature and field.
///
/// The field labels [`visit_expressions`] produces are a pure function of the
/// tree's shape, so an edit recorded this way can be replayed onto the same
/// tree — which is how a rename that rewrote feature expressions is undone
/// and redone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprEdit {
    pub feature: Uuid,
    pub field: String,
    pub text: String,
}

/// Rewrite every reference to `from` as `to`, in the parameter table AND in
/// every expression field on the tree. Returns the PREVIOUS text of each
/// field it changed, so the edit can be undone.
///
/// The rewrite goes through the AST ([`expr::rename_identifier`]): `w2` is
/// not a reference to `w`, a `mm` suffix is not an identifier, and spacing
/// is preserved byte-for-byte. A string replace would corrupt all three.
///
/// An expression that does not parse is left ALONE. It has no references to
/// rewrite, and editing text nobody has made sense of yet is how a rename
/// turns a typo into a different typo.
pub fn rename_parameter(tree: &mut FeatureTree, from: &str, to: &str) -> Vec<ExprEdit> {
    for p in &mut tree.parameters {
        if p.name == from {
            p.name = to.to_string();
        }
        if let Some(rewritten) = expr::rename_identifier(&p.expression, from, to) {
            p.expression = rewritten;
        }
    }
    let mut undo = Vec::new();
    visit_expressions(tree, |site| {
        if let Some(rewritten) = expr::rename_identifier(site.expression, from, to) {
            undo.push(ExprEdit {
                feature: site.feature,
                field: site.field,
                text: site.expression.clone(),
            });
            *site.expression = rewritten;
        }
    });
    undo
}

/// Rewrite every measurement of the ENTITY `from` as `to`, in the parameter
/// table AND in every expression field on the tree (D2). Returns the
/// PREVIOUS text of each field it changed, so the edit can be undone.
///
/// The entity twin of [`rename_parameter`], and a SEPARATE function for the
/// reason the two namespaces are separate: renaming the body `plate` must
/// rewrite `volume(plate)` and must NOT rewrite a design parameter that
/// happens to be called `plate`. One function doing both would conflate two
/// different things that share a spelling.
///
/// Same mechanism as the parameter rename — splice the AST's byte spans —
/// so spacing, dots and parentheses survive, and an unparseable expression
/// is left alone.
pub fn rename_entity(tree: &mut FeatureTree, from: &str, to: &str) -> EntityRenameEdits {
    let mut undo = EntityRenameEdits::default();
    for p in &mut tree.parameters {
        if let Some(rewritten) = expr::rename_entity_reference(&p.expression, from, to) {
            undo.parameters.push((p.id, p.expression.clone()));
            p.expression = rewritten;
        }
    }
    visit_expressions(tree, |site| {
        if let Some(rewritten) = expr::rename_entity_reference(site.expression, from, to) {
            undo.fields.push(ExprEdit {
                feature: site.feature,
                field: site.field,
                text: site.expression.clone(),
            });
            *site.expression = rewritten;
        }
    });
    undo
}

/// Everything one entity rename rewrote — enough to put it back exactly.
///
/// Both halves, because an entity name can be measured from a design
/// parameter as well as from a feature field, and a body rename carries no
/// copy of the parameter table to restore from (unlike `SetParameters`,
/// which does).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntityRenameEdits {
    /// Feature expression fields, addressed as [`ExprEdit`] does.
    pub fields: Vec<ExprEdit>,
    /// Design parameters, by id, with the expression each held before.
    pub parameters: Vec<(Uuid, String)>,
}

impl EntityRenameEdits {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.parameters.is_empty()
    }
}

/// Replay an [`EntityRenameEdits`] onto the tree (undo or redo of an entity
/// rename). A site the tree no longer has is skipped, as
/// [`restore_expressions`] does.
pub fn restore_entity_rename(tree: &mut FeatureTree, edits: &EntityRenameEdits) {
    for (id, text) in &edits.parameters {
        if let Some(p) = tree.parameters.iter_mut().find(|p| p.id == *id) {
            p.expression = text.clone();
        }
    }
    restore_expressions(tree, &edits.fields);
}

/// Read the current text at every site an [`EntityRenameEdits`] addresses —
/// the redo half, as [`read_expressions`] is for a parameter rename.
pub fn read_entity_rename(tree: &mut FeatureTree, at: &EntityRenameEdits) -> EntityRenameEdits {
    EntityRenameEdits {
        parameters: at
            .parameters
            .iter()
            .filter_map(|(id, _)| {
                tree.parameters
                    .iter()
                    .find(|p| p.id == *id)
                    .map(|p| (*id, p.expression.clone()))
            })
            .collect(),
        fields: read_expressions(tree, &at.fields),
    }
}

/// Read the CURRENT text at each addressed site — the other half of an undo
/// record: `rename_parameter` returns what the fields held before, this
/// returns what they hold after, and redo replays the second.
pub fn read_expressions(tree: &mut FeatureTree, at: &[ExprEdit]) -> Vec<ExprEdit> {
    if at.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    visit_expressions(tree, |site| {
        if at
            .iter()
            .any(|e| e.feature == site.feature && e.field == site.field)
        {
            out.push(ExprEdit {
                feature: site.feature,
                field: site.field,
                text: site.expression.clone(),
            });
        }
    });
    out
}

/// Replay recorded expression texts onto the tree (undo/redo of a rename).
/// A site the tree no longer has is skipped — the feature it named is gone,
/// and there is nothing to restore.
pub fn restore_expressions(tree: &mut FeatureTree, edits: &[ExprEdit]) {
    if edits.is_empty() {
        return;
    }
    visit_expressions(tree, |site| {
        if let Some(edit) = edits
            .iter()
            .find(|e| e.feature == site.feature && e.field == site.field)
        {
            *site.expression = edit.text.clone();
        }
    });
}

/// Every dependency cycle in the parameter table, each as the names around
/// the loop with the entry name repeated at the end (`["a", "b", "a"]`).
///
/// A cycle is a property of the GRAPH, not of one evaluation: the fixpoint in
/// [`evaluate_parameters`] can only report that a parameter never resolved,
/// which is the same symptom as a typo'd name. This names the loop instead,
/// which is what P5 asks for.
///
/// Each cycle is reported once, canonicalised to start at its
/// lexicographically smallest member, and the list is sorted — the answer
/// does not depend on table order.
pub fn cycles(params: &[DesignParameter]) -> Vec<Vec<String>> {
    // Adjacency over DECLARED names only: a reference to a name the table
    // does not have is an unknown identifier, not an edge.
    let mut edges: HashMap<&str, Vec<String>> = HashMap::new();
    for p in params {
        if edges.contains_key(p.name.as_str()) {
            continue; // A duplicate name is its own error; first wins.
        }
        let deps = expr::dependencies(&p.expression).unwrap_or_default();
        edges.insert(p.name.as_str(), deps.into_iter().collect());
    }
    let declared: Vec<&str> = {
        let mut v: Vec<&str> = edges.keys().copied().collect();
        v.sort_unstable();
        v
    };

    let mut found: Vec<Vec<String>> = Vec::new();
    let mut done: std::collections::HashSet<&str> = std::collections::HashSet::new();
    // Iterative DFS keeping the current path, so a cycle is read off the
    // path rather than inferred. Bounded by the table: every node is
    // explored once.
    for start in declared {
        if done.contains(start) {
            continue;
        }
        let mut path: Vec<&str> = Vec::new();
        let mut on_path: std::collections::HashSet<&str> = std::collections::HashSet::new();
        // (node, next child index)
        let mut stack: Vec<(&str, usize)> = vec![(start, 0)];
        path.push(start);
        on_path.insert(start);
        while let Some((node, child)) = stack.pop() {
            let children = edges.get(node).map(Vec::as_slice).unwrap_or(&[]);
            if child >= children.len() {
                path.pop();
                on_path.remove(node);
                done.insert(node);
                continue;
            }
            stack.push((node, child + 1));
            let next = children[child].as_str();
            let Some((next, _)) = edges.get_key_value(next).map(|(k, v)| (*k, v)) else {
                continue; // Not a declared parameter: no edge.
            };
            if on_path.contains(next) {
                // The loop is the tail of the path from `next` onward.
                let at = path.iter().position(|n| *n == next).unwrap_or(0);
                let mut loop_names: Vec<String> =
                    path[at..].iter().map(|n| (*n).to_string()).collect();
                // Rotate to start at the smallest member so the same cycle
                // reported from two entry points is one answer.
                if let Some(min) = loop_names
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.cmp(b.1))
                    .map(|(i, _)| i)
                {
                    loop_names.rotate_left(min);
                }
                loop_names.push(loop_names[0].clone());
                if !found.contains(&loop_names) {
                    found.push(loop_names);
                }
                continue;
            }
            if done.contains(next) {
                continue;
            }
            path.push(next);
            on_path.insert(next);
            stack.push((next, 0));
        }
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DepthMode, ExtrudeParams, Feature, RevolveParams};
    use serde_json::{json, Value};
    use waffle_types::SketchConstraint;

    fn param(name: &str, expression: &str) -> DesignParameter {
        DesignParameter::new(name, expression)
    }

    // -- evaluate_parameters --

    #[test]
    fn table_resolves_forward_references_any_order() {
        let mut params = vec![param("b", "a * 2"), param("a", "10")];
        let env = evaluate_parameters(&mut params);
        assert_eq!(env.get("a").map(|q| q.value), Some(10.0));
        assert_eq!(env.get("b").map(|q| q.value), Some(20.0));
        assert_eq!(params[0].value, 20.0);
        assert!(params[0].error.is_none());
        assert_eq!(params[1].value, 10.0);
    }

    #[test]
    fn table_reports_cycles_without_hanging() {
        let mut params = vec![param("a", "b + 1"), param("b", "a + 1"), param("c", "5")];
        let env = evaluate_parameters(&mut params);
        assert_eq!(env.get("c").map(|q| q.value), Some(5.0));
        assert!(!env.contains_key("a"));
        assert!(params[0].error.as_deref().unwrap().contains("circular"));
        assert!(params[1].error.as_deref().unwrap().contains("circular"));
        assert!(params[2].error.is_none());
    }

    #[test]
    fn table_reports_duplicates_and_bad_names_first_wins() {
        let mut params = vec![param("w", "1"), param("w", "2"), param("mm", "3")];
        let env = evaluate_parameters(&mut params);
        assert_eq!(env.get("w").map(|q| q.value), Some(1.0));
        assert!(params[1].error.as_deref().unwrap().contains("duplicate"));
        assert!(params[2].error.as_deref().unwrap().contains("reserved"));
    }

    #[test]
    fn table_keeps_last_good_value_on_error() {
        let mut params = vec![param("a", "10")];
        evaluate_parameters(&mut params);
        assert_eq!(params[0].value, 10.0);
        params[0].expression = "1 /".to_string();
        evaluate_parameters(&mut params);
        assert_eq!(params[0].value, 10.0, "cache must survive a bad edit");
        assert!(params[0].error.is_some());
    }

    // -- apply_parameters over features --

    fn extrude_feature(depth: f64, depth_expr: Option<&str>) -> Feature {
        Feature {
            id: Uuid::new_v4(),
            name: "Extrude".to_string(),
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id: Uuid::new_v4(),
                    profile_index: 0,
                    profile_entity_ids: None,
                    depth,
                    depth_expr: depth_expr.map(str::to_string),
                    direction: None,
                    symmetric: false,
                    cut: false,
                    merge: true,
                    target_body: None,
                    depth_mode: DepthMode::Blind,
                    second_direction: None,
                    region: None,
                    regions: Vec::new(),
                    combine: None,
                    targets: None,
                },
            },
            suppressed: false,
            references: Vec::new(),
        }
    }

    fn revolve_feature(angle: f64, angle_expr: Option<&str>) -> Feature {
        Feature {
            id: Uuid::new_v4(),
            name: "Revolve".to_string(),
            operation: Operation::Revolve {
                params: RevolveParams {
                    sketch_id: Uuid::new_v4(),
                    profile_index: 0,
                    profile_entity_ids: None,
                    axis_origin: [0.0; 3],
                    axis_origin_expr: None,
                    axis_direction: [0.0, 0.0, 1.0],
                    angle,
                    angle_expr: angle_expr.map(str::to_string),
                    cut: false,
                    merge: true,
                    combine: None,
                    targets: None,
                },
            },
            suppressed: false,
            references: Vec::new(),
        }
    }

    fn revolve_angle(tree: &FeatureTree, idx: usize) -> f64 {
        match &tree.features[idx].operation {
            Operation::Revolve { params } => params.angle,
            other => panic!("expected revolve, got {other:?}"),
        }
    }

    fn tree_with(parameters: Vec<DesignParameter>, features: Vec<Feature>) -> FeatureTree {
        FeatureTree {
            features,
            active_index: None,
            body_names: Default::default(),
            parameters,
            ..Default::default()
        }
    }

    fn extrude_depth(tree: &FeatureTree, idx: usize) -> f64 {
        match &tree.features[idx].operation {
            Operation::Extrude { params } => params.depth,
            other => panic!("expected extrude, got {other:?}"),
        }
    }

    #[test]
    fn extrude_depth_expression_drives_depth_in_meters() {
        let mut tree = tree_with(
            vec![param("height", "25")],
            vec![extrude_feature(0.010, Some("height"))],
        );
        let outcome = apply_parameters(&mut tree);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.first_changed, Some(0));
        assert!((extrude_depth(&tree, 0) - 0.025).abs() < 1e-15);

        // Idempotent: a second pass changes nothing.
        let outcome2 = apply_parameters(&mut tree);
        assert_eq!(outcome2.first_changed, None);
    }

    #[test]
    fn expression_without_parameters_works() {
        let mut tree = tree_with(vec![], vec![extrude_feature(0.010, Some("1in + 2mm"))]);
        let outcome = apply_parameters(&mut tree);
        assert!(outcome.errors.is_empty());
        assert!((extrude_depth(&tree, 0) - 0.0274).abs() < 1e-15);
    }

    #[test]
    fn bad_expression_keeps_value_and_reports_error() {
        let mut tree = tree_with(vec![], vec![extrude_feature(0.010, Some("nope * 2"))]);
        let feature_id = tree.features[0].id;
        let outcome = apply_parameters(&mut tree);
        assert_eq!(extrude_depth(&tree, 0), 0.010, "value must not change");
        assert_eq!(outcome.first_changed, None);
        assert_eq!(outcome.errors.len(), 1);
        assert_eq!(outcome.errors[0].0, feature_id);
        assert!(outcome.errors[0].1.contains("unknown variable 'nope'"));
    }

    // -- P1: the typed boundary (`specs/agent_mechanical_design.md` §6) --

    #[test]
    fn a_bare_number_in_a_depth_still_means_millimeters() {
        // The pre-P1 contract, pinned: a bare number is mm-space, so a
        // depth of `25` is 25 mm = 0.025 m, not 25 m.
        let mut tree = tree_with(vec![], vec![extrude_feature(0.0, Some("25"))]);
        assert!(apply_parameters(&mut tree).errors.is_empty());
        assert_eq!(extrude_depth(&tree, 0), 0.025);
    }

    #[test]
    fn an_angle_in_a_depth_is_refused_not_read_as_millimeters() {
        // THE defect P1 exists to kill: before P1, `25deg` as a depth
        // evaluated to the plain number 25 and the length boundary read it
        // as 25 mm.
        let mut tree = tree_with(vec![], vec![extrude_feature(0.010, Some("25deg"))]);
        let outcome = apply_parameters(&mut tree);
        assert_eq!(extrude_depth(&tree, 0), 0.010, "the depth must not change");
        assert_eq!(outcome.first_changed, None);
        assert_eq!(outcome.errors.len(), 1, "{:?}", outcome.errors);
        let msg = &outcome.errors[0].1;
        assert!(
            msg.contains("expected a length, got an angle"),
            "message was {msg:?}"
        );
        assert!(msg.contains("at bytes 0..5"), "message was {msg:?}");
    }

    #[test]
    fn a_length_in_an_angle_is_refused_too() {
        let mut tree = tree_with(vec![], vec![revolve_feature(360.0, Some("1in"))]);
        let outcome = apply_parameters(&mut tree);
        assert_eq!(revolve_angle(&tree, 0), 360.0);
        assert_eq!(outcome.errors.len(), 1, "{:?}", outcome.errors);
        assert!(
            outcome.errors[0]
                .1
                .contains("expected an angle, got a length"),
            "{}",
            outcome.errors[0].1
        );
    }

    #[test]
    fn explicit_length_units_convert_at_the_boundary() {
        for (expression, meters) in [
            ("25 mm", 0.025),
            ("1 in", 0.0254),
            ("2cm", 0.020),
            ("1m", 1.0),
            ("10mm + 1in", 0.0354),
        ] {
            let mut tree = tree_with(vec![], vec![extrude_feature(0.0, Some(expression))]);
            let outcome = apply_parameters(&mut tree);
            assert!(
                outcome.errors.is_empty(),
                "{expression}: {:?}",
                outcome.errors
            );
            let got = extrude_depth(&tree, 0);
            assert!(
                (got - meters).abs() < 1e-15,
                "{expression}: expected {meters} m, got {got}"
            );
        }
    }

    #[test]
    fn an_area_cannot_be_a_depth_but_its_root_can() {
        let mut tree = tree_with(vec![], vec![extrude_feature(0.010, Some("5mm * 5mm"))]);
        let outcome = apply_parameters(&mut tree);
        assert_eq!(outcome.errors.len(), 1, "{:?}", outcome.errors);
        assert!(
            outcome.errors[0].1.contains("got length^2"),
            "{}",
            outcome.errors[0].1
        );

        let mut tree = tree_with(vec![], vec![extrude_feature(0.0, Some("sqrt(5mm * 5mm)"))]);
        assert!(apply_parameters(&mut tree).errors.is_empty());
        assert_eq!(extrude_depth(&tree, 0), 0.005);
    }

    #[test]
    fn a_declared_parameter_unit_propagates_into_the_fields_that_read_it() {
        let mut tree = tree_with(
            vec![
                param("height", "25").with_unit(Dimension::Length),
                param("turn", "90").with_unit(Dimension::Angle),
            ],
            vec![
                extrude_feature(0.0, Some("height")),
                extrude_feature(0.010, Some("turn")),
            ],
        );
        let outcome = apply_parameters(&mut tree);
        assert_eq!(extrude_depth(&tree, 0), 0.025, "a length parameter fits");
        assert_eq!(
            extrude_depth(&tree, 1),
            0.010,
            "an angle parameter in a depth is refused"
        );
        assert_eq!(outcome.errors.len(), 1, "{:?}", outcome.errors);
        assert!(
            outcome.errors[0]
                .1
                .contains("expected a length, got an angle"),
            "{}",
            outcome.errors[0].1
        );
    }

    #[test]
    fn a_declared_unit_the_expression_contradicts_is_the_parameters_own_error() {
        let mut params = vec![param("height", "25deg").with_unit(Dimension::Length)];
        evaluate_parameters(&mut params);
        let err = params[0].error.as_deref().unwrap();
        assert!(err.contains("expected a length, got an angle"), "{err}");

        let mut params = vec![param("teeth", "20.5").with_unit(Dimension::Count)];
        evaluate_parameters(&mut params);
        let err = params[0].error.as_deref().unwrap();
        assert!(err.contains("whole non-negative count"), "{err}");
    }

    #[test]
    fn one_radian_in_a_revolve_angle_is_degrees_at_the_boundary() {
        // `rad` is P1's only new suffix, and the working space is degrees,
        // so the conversion happens in the literal: 1 rad = 57.29577…°.
        let mut tree = tree_with(vec![], vec![revolve_feature(0.0, Some("1rad"))]);
        let outcome = apply_parameters(&mut tree);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        let got = revolve_angle(&tree, 0);
        assert!(
            (got - 180.0 / std::f64::consts::PI).abs() < 1e-12,
            "expected 57.29577951308232 degrees, got {got}"
        );
        // And `pi rad` is the half turn it should be.
        let mut tree = tree_with(vec![], vec![revolve_feature(0.0, Some("pi * 1rad"))]);
        assert!(apply_parameters(&mut tree).errors.is_empty());
        assert!((revolve_angle(&tree, 0) - 180.0).abs() < 1e-12);
        // A radian is an ANGLE, so a depth refuses it like any other.
        let mut tree = tree_with(vec![], vec![extrude_feature(0.010, Some("1rad"))]);
        let outcome = apply_parameters(&mut tree);
        assert_eq!(extrude_depth(&tree, 0), 0.010);
        assert!(outcome.errors[0]
            .1
            .contains("expected a length, got an angle"));
    }

    #[test]
    fn cached_env_reproduces_a_dimension_the_expression_committed_itself() {
        // The preview must refuse exactly what the rebuild refuses. A
        // parameter with NO declared unit whose expression commits one is
        // the case `unit` alone cannot reconstruct.
        let mut params = vec![param("width", "2cm"), param("turn", "90deg")];
        let rebuild = evaluate_parameters(&mut params);
        let preview = cached_env(&params);
        for name in ["width", "turn"] {
            let r = rebuild.get(name).copied().unwrap();
            let p = preview.get(name).copied().unwrap();
            assert_eq!(p.value, r.value, "{name}");
            assert_eq!(p.tag, r.tag, "{name}: the preview must agree on the kind");
        }
        assert_eq!(preview["width"].dimension(), Some(Dimension::Length));
        assert!(preview["width"].as_angle_degrees().is_err());
        assert_eq!(preview["turn"].as_angle_degrees().unwrap(), 90.0);
        assert!(preview["turn"].as_length_meters().is_err());
        // A bare expression stays uncommitted on both sides.
        let mut params = vec![param("plain", "25")];
        evaluate_parameters(&mut params);
        assert_eq!(cached_env(&params)["plain"].dimension(), None);
    }

    #[test]
    fn cached_env_carries_the_declared_dimension_like_the_rebuild_will() {
        let mut params = vec![param("turn", "90").with_unit(Dimension::Angle)];
        evaluate_parameters(&mut params);
        let env = cached_env(&params);
        let q = env.get("turn").copied().unwrap();
        assert_eq!(q.value, 90.0);
        assert_eq!(q.dimension(), Some(Dimension::Angle));
        assert_eq!(q.as_angle_degrees().unwrap(), 90.0);
        assert!(q.as_length_meters().is_err());
    }

    #[test]
    fn revolve_angle_expression_is_degrees_verbatim() {
        let mut tree = tree_with(
            vec![param("turn", "90")],
            vec![Feature {
                id: Uuid::new_v4(),
                name: "Revolve".to_string(),
                operation: Operation::Revolve {
                    params: RevolveParams {
                        sketch_id: Uuid::new_v4(),
                        profile_index: 0,
                        profile_entity_ids: None,
                        axis_origin: [0.0; 3],
                        axis_origin_expr: None,
                        axis_direction: [0.0, 0.0, 1.0],
                        angle: 360.0,
                        angle_expr: Some("turn * 2".to_string()),
                        cut: false,
                        merge: true,
                        combine: None,
                        targets: None,
                    },
                },
                suppressed: false,
                references: Vec::new(),
            }],
        );
        let outcome = apply_parameters(&mut tree);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        match &tree.features[0].operation {
            Operation::Revolve { params } => assert_eq!(params.angle, 180.0),
            _ => unreachable!(),
        }
    }

    #[test]
    fn first_changed_is_minimum_across_features() {
        let mut tree = tree_with(
            vec![param("d", "40")],
            vec![
                extrude_feature(0.040, Some("d")), // already equal — no change
                extrude_feature(0.010, Some("d")), // changes
                extrude_feature(0.010, Some("d")), // changes
            ],
        );
        let outcome = apply_parameters(&mut tree);
        assert_eq!(outcome.first_changed, Some(1));
    }

    // -- sketch re-solve --

    /// A 10mm x 10mm rectangle driven by two Distance dims (bottom width,
    /// right height), pinned at the origin corner.
    fn rectangle_sketch(
        width_expr: Option<&str>,
        height_expr: Option<&str>,
    ) -> waffle_types::Sketch {
        use waffle_types::SketchEntity as E;
        let entities = vec![
            E::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            E::Point {
                id: 2,
                x: 0.010,
                y: 0.0,
                construction: false,
            },
            E::Point {
                id: 3,
                x: 0.010,
                y: 0.010,
                construction: false,
            },
            E::Point {
                id: 4,
                x: 0.0,
                y: 0.010,
                construction: false,
            },
            E::Line {
                id: 5,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            E::Line {
                id: 6,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            E::Line {
                id: 7,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            E::Line {
                id: 8,
                start_id: 4,
                end_id: 1,
                construction: false,
            },
        ];
        let constraints = vec![
            SketchConstraint::Pinned {
                point: 1,
                x: 0.0,
                y: 0.0,
            },
            SketchConstraint::Horizontal { entity: 5 },
            SketchConstraint::Horizontal { entity: 7 },
            SketchConstraint::Vertical { entity: 6 },
            SketchConstraint::Vertical { entity: 8 },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 2,
                value: 0.010,
                expression: width_expr.map(str::to_string),
                reference: false,
            },
            SketchConstraint::Distance {
                entity_a: 2,
                entity_b: 3,
                value: 0.010,
                expression: height_expr.map(str::to_string),
                reference: false,
            },
        ];
        let mut sketch = waffle_types::Sketch {
            id: Uuid::new_v4(),
            plane: waffle_types::GeomRef {
                kind: waffle_types::TopoKind::Face,
                anchor: waffle_types::Anchor::Datum {
                    datum_id: Uuid::new_v4(),
                },
                selector: waffle_types::Selector::Position {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                policy: waffle_types::ResolvePolicy::BestEffort,
                scope: None,
            },
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            plane_x_axis: None,
            entities,
            constraints,
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: Default::default(),
            solved_profiles: Vec::new(),
            projected: Vec::new(),
            plane_face: None,
        };
        sketch.recompute_derived();
        sketch
    }

    fn sketch_feature(sketch: waffle_types::Sketch) -> Feature {
        Feature {
            id: Uuid::new_v4(),
            name: "Sketch".to_string(),
            operation: Operation::Sketch { sketch },
            suppressed: false,
            references: Vec::new(),
        }
    }

    fn sketch_of(tree: &FeatureTree, idx: usize) -> &waffle_types::Sketch {
        match &tree.features[idx].operation {
            Operation::Sketch { sketch } => sketch,
            other => panic!("expected sketch, got {other:?}"),
        }
    }

    #[test]
    fn sketch_dimension_expression_resolves_and_moves_geometry() {
        let mut tree = tree_with(
            vec![param("width", "30")],
            vec![sketch_feature(rectangle_sketch(Some("width"), None))],
        );
        let outcome = apply_parameters(&mut tree);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.first_changed, Some(0));

        let sketch = sketch_of(&tree, 0);
        // The driven Distance dim now reads 30mm...
        assert_eq!(sketch.constraints[5].dimension_value(), Some(0.030));
        // ...and the geometry followed: point 2 sits at (0.030, 0).
        let p2 = sketch.solved_positions.get(&2).copied().unwrap();
        assert!((p2.0 - 0.030).abs() < 1e-9, "p2.x = {}", p2.0);
        assert!(p2.1.abs() < 1e-9);
        // Height was numeric-only and untouched.
        let p3 = sketch.solved_positions.get(&3).copied().unwrap();
        assert!((p3.1 - 0.010).abs() < 1e-9, "p3.y = {}", p3.1);
        // Profiles recomputed and still closed.
        assert!(!sketch.solved_profiles.is_empty());

        // Change the variable; geometry follows again.
        tree.parameters[0].expression = "42".to_string();
        let outcome = apply_parameters(&mut tree);
        assert_eq!(outcome.first_changed, Some(0));
        let sketch = sketch_of(&tree, 0);
        let p2 = sketch.solved_positions.get(&2).copied().unwrap();
        assert!((p2.0 - 0.042).abs() < 1e-9, "p2.x = {}", p2.0);
    }

    #[test]
    fn sketch_with_unchanged_expression_values_is_untouched() {
        let mut tree = tree_with(
            vec![param("width", "10")],
            vec![sketch_feature(rectangle_sketch(Some("width"), None))],
        );
        // width = 10mm matches the built rectangle exactly: no change.
        // (Compare fields, not whole-sketch JSON — HashMap serialization
        // order is nondeterministic even for identical maps.)
        let before = sketch_of(&tree, 0).clone();
        let outcome = apply_parameters(&mut tree);
        assert!(outcome.errors.is_empty());
        assert_eq!(outcome.first_changed, None);
        let after = sketch_of(&tree, 0);
        assert_eq!(before.solved_positions, after.solved_positions);
        assert_eq!(
            serde_json::to_string(&before.entities).unwrap(),
            serde_json::to_string(&after.entities).unwrap(),
            "no-op pass must not touch entities"
        );
        assert_eq!(
            serde_json::to_string(&before.constraints).unwrap(),
            serde_json::to_string(&after.constraints).unwrap(),
            "no-op pass must not touch constraints"
        );
    }

    #[test]
    fn reference_dimension_is_excluded_from_the_resolve() {
        // A reference copy of the width dim with a WRONG value would
        // over-constrain the solve if it were treated as driving.
        let mut sketch = rectangle_sketch(Some("width"), None);
        sketch.constraints.push(SketchConstraint::Distance {
            entity_a: 1,
            entity_b: 2,
            value: 0.001, // contradicts the driving 30mm dim
            expression: None,
            reference: true,
        });
        let mut tree = tree_with(vec![param("width", "30")], vec![sketch_feature(sketch)]);
        let outcome = apply_parameters(&mut tree);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        let p2 = sketch_of(&tree, 0)
            .solved_positions
            .get(&2)
            .copied()
            .unwrap();
        assert!((p2.0 - 0.030).abs() < 1e-9);
    }

    #[test]
    fn failed_resolve_restores_previous_dimensions() {
        // Contradictory DRIVING dims: width both 30mm (expression) and 10mm
        // (numeric) on the same pair — the re-solve cannot satisfy both.
        let mut sketch = rectangle_sketch(Some("width"), None);
        sketch.constraints.push(SketchConstraint::Distance {
            entity_a: 1,
            entity_b: 2,
            value: 0.010,
            expression: None,
            reference: false,
        });
        let before_positions = sketch.solved_positions.clone();
        let mut tree = tree_with(vec![param("width", "30")], vec![sketch_feature(sketch)]);
        let outcome = apply_parameters(&mut tree);
        assert_eq!(outcome.errors.len(), 1, "{:?}", outcome.errors);
        assert!(outcome.errors[0].1.contains("re-solve failed"));
        let sketch = sketch_of(&tree, 0);
        // Dimension value restored to its pre-pass state...
        assert_eq!(sketch.constraints[5].dimension_value(), Some(0.010));
        // ...and geometry untouched.
        assert_eq!(sketch.solved_positions, before_positions);
    }

    #[test]
    fn angle_dimension_expression_is_degrees() {
        // Two lines from the origin; drive the angle between them.
        use waffle_types::SketchEntity as E;
        let entities = vec![
            E::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            E::Point {
                id: 2,
                x: 0.010,
                y: 0.0,
                construction: false,
            },
            E::Point {
                id: 3,
                x: 0.010,
                y: 0.010,
                construction: false,
            },
            E::Line {
                id: 4,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            E::Line {
                id: 5,
                start_id: 1,
                end_id: 3,
                construction: false,
            },
        ];
        let constraints = vec![
            SketchConstraint::Pinned {
                point: 1,
                x: 0.0,
                y: 0.0,
            },
            SketchConstraint::Pinned {
                point: 2,
                x: 0.010,
                y: 0.0,
            },
            SketchConstraint::Distance {
                entity_a: 1,
                entity_b: 3,
                value: 0.010 * std::f64::consts::SQRT_2,
                expression: None,
                reference: false,
            },
            SketchConstraint::Angle {
                line_a: 4,
                line_b: 5,
                value_degrees: 45.0,
                expression: Some("a".to_string()),
                reference: false,
            },
        ];
        let mut sketch = rectangle_sketch(None, None);
        sketch.entities = entities;
        sketch.constraints = constraints;
        sketch.solved_positions.clear();
        sketch.solved_profiles.clear();
        sketch.recompute_derived();

        let mut tree = tree_with(vec![param("a", "30")], vec![sketch_feature(sketch)]);
        let outcome = apply_parameters(&mut tree);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        let sketch = sketch_of(&tree, 0);
        assert_eq!(sketch.constraints[3].dimension_value(), Some(30.0));
        let p3 = sketch.solved_positions.get(&3).copied().unwrap();
        let angle = p3.1.atan2(p3.0).to_degrees();
        assert!((angle - 30.0).abs() < 1e-6, "solved angle = {angle}");
    }

    // -- P5: the table as data (§6 P5) --

    /// One feature per expression-carrying operation kind, every sidecar
    /// filled. Built through serde so the test cannot quietly skip a field
    /// the struct gained.
    fn every_expression_feature() -> Vec<Feature> {
        let sketch_id = Uuid::new_v4();
        let ops: Vec<Value> = vec![
            json!({ "type": "Extrude", "params": {
                "sketch_id": sketch_id, "profile_index": 0, "depth": 0.004,
                "depth_expr": "a", "symmetric": false, "cut": false,
                "second_direction": { "type": "Blind", "depth": 0.002,
                                      "depth_expr": "a" } }}),
            json!({ "type": "Revolve", "params": {
                "sketch_id": sketch_id, "profile_index": 0,
                "axis_origin": [0.0, 0.0, 0.0],
                "axis_origin_expr": ["a", "a", "a"],
                "axis_direction": [0.0, 0.0, 1.0],
                "angle": 90.0, "angle_expr": "a", "cut": false }}),
            json!({ "type": "Pipe", "params": {
                "sketch_id": sketch_id, "entity_ids": [1, 2], "radius": 0.005,
                "radius_expr": "a", "inner_radius": 0.002, "inner_radius_expr": "a" }}),
            json!({ "type": "DatumPlane", "params": { "name": "Datum", "definition": {
                "method": "offset", "basePlaneId": Uuid::new_v4(),
                "distance": 0.01, "distance_expr": "a" }}}),
            json!({ "type": "DatumPlane", "params": { "name": "Datum2", "definition": {
                "method": "point-normal", "origin": [0.0, 0.0, 0.0],
                "origin_expr": ["a", "a", "a"], "normal": [0.0, 0.0, 1.0] }}}),
            json!({ "type": "PatternCircular", "params": {
                "axis": { "method": "explicit", "origin": [0.0, 0.0, 0.0],
                          "origin_expr": ["a", "a", "a"],
                          "direction": [0.0, 0.0, 1.0] },
                "count": 4, "count_expr": "a",
                "angle_deg": 360.0, "angle_expr": "a" }}),
            json!({ "type": "PatternLinear", "params": {
                "direction": { "method": "explicit", "origin": [0.0, 0.0, 0.0],
                               "origin_expr": ["a", "a", "a"],
                               "direction": [1.0, 0.0, 0.0] },
                "count": 3, "count_expr": "a", "spacing": 0.01, "spacing_expr": "a",
                "second": { "direction": { "method": "explicit",
                                           "origin": [0.0, 0.0, 0.0],
                                           "origin_expr": ["a", "a", "a"],
                                           "direction": [0.0, 1.0, 0.0] },
                            "count": 2, "count_expr": "a",
                            "spacing": 0.02, "spacing_expr": "a" }}}),
            json!({ "type": "PatternMirror", "params": {
                "plane": { "method": "explicit", "origin": [0.0, 0.0, 0.0],
                           "origin_expr": ["a", "a", "a"],
                           "direction": [1.0, 0.0, 0.0] } }}),
            json!({ "type": "MateConnector", "params": {
                "name": "C1",
                "frame": { "origin": [0.0, 0.0, 0.0], "z_axis": [0.0, 0.0, 1.0],
                           "x_axis": [1.0, 0.0, 0.0] },
                "rotation_deg": 0.0, "rotation_expr": "a",
                "offset_m": [0.0, 0.0, 0.0], "offset_m_expr": ["a", "a", "a"] }}),
            json!({ "type": "ImportedBody", "params": {
                "file_name": "x.step", "source_id": Uuid::new_v4(),
                "translation_m": [0.0, 0.0, 0.0],
                "translation_m_expr": ["a", "a", "a"],
                "rotation_deg": [0.0, 0.0, 0.0],
                "rotation_deg_expr": ["a", "a", "a"],
                "scale": 1.0, "scale_expr": "a" }}),
            json!({ "type": "Script", "params": {
                "source_id": Uuid::new_v4(), "args": {},
                "arg_exprs": { "teeth": "a" }}}),
            json!({ "type": "Sketch3d", "sketch": { "id": Uuid::new_v4(), "entities": [
                { "type": "Point", "id": 1, "xyz": [0.0, 0.0, 0.0],
                  "xyz_expr": ["a", "a", "a"] },
                { "type": "Point", "id": 2, "xyz": [0.01, 0.0, 0.0] },
                { "type": "Line", "id": 3, "start_id": 1, "end_id": 2 },
                { "type": "Fillet", "id": 4, "at_point_id": 1, "radius": 0.001,
                  "radius_expr": "a" }
            ]}}),
        ];
        let mut features: Vec<Feature> = ops
            .into_iter()
            .map(|op| Feature {
                id: Uuid::new_v4(),
                name: op["type"].as_str().unwrap().to_string(),
                operation: serde_json::from_value(op.clone())
                    .unwrap_or_else(|e| panic!("{}: {e}", op["type"])),
                suppressed: false,
                references: Vec::new(),
            })
            .collect();
        // A sketch with two expression-driven dimensions.
        features.push(sketch_feature(rectangle_sketch(Some("a"), Some("a"))));
        features
    }

    /// The drift oracle. `expression_sites` is a SECOND traversal beside the
    /// apply pass, so the two could learn about different fields. Give every
    /// site the same unresolvable expression: the apply pass must report
    /// exactly as many errors as the enumeration finds sites, which is false
    /// the moment either one knows a field the other does not.
    #[test]
    fn every_expression_field_is_enumerated() {
        let mut tree = tree_with(Vec::new(), every_expression_feature());
        let sites = field_uses(&mut tree);
        // Extrude depth + second depth (2); revolve angle + axis origin x/y/z
        // (4); pipe radius + inner_radius (2); datum offset distance (1);
        // datum point-normal origin x/y/z (3); circular count + angle + axis
        // origin x/y/z (5); linear count + spacing + direction origin x/y/z,
        // and the same four again for the second leg (10); mirror plane
        // origin x/y/z (3); mate connector rotation + offset x/y/z (4);
        // imported body translation x/y/z + rotation x/y/z + scale (7); one
        // script arg (1); three 3D-point coordinates + a 3D fillet radius
        // (4); the rectangle sketch's two dimensions (2).
        assert_eq!(sites.len(), 48, "{sites:#?}");
        for site in &sites {
            assert_eq!(site.expression, "a", "{} {}", site.feature_name, site.field);
            assert_eq!(site.reads, vec!["a".to_string()]);
        }

        // `a` is not declared, so every one of those fields fails loudly.
        let outcome = apply_parameters(&mut tree);
        let per_field = outcome
            .errors
            .iter()
            .filter(|(id, _)| *id != Uuid::nil())
            .count();
        assert_eq!(
            per_field,
            sites.len(),
            "the apply pass reported {per_field} field errors for {} enumerated sites: {:#?}",
            sites.len(),
            outcome.errors
        );
    }

    #[test]
    fn every_expression_field_can_measure_and_is_enumerated_as_one() {
        // D2's half of the drift oracle above: `measurement_sites` must see
        // the same fields `field_uses` does, plus the parameters. A field
        // added to `expression_sites` is a measurement site for free; one
        // added only to the apply pass is caught by the oracle above.
        //
        // The count tracks the oracle above (48 since P3 added ten
        // sidecars), plus the one measuring parameter — which is the whole
        // claim: P3's ten fields became measurement sites without D2 being
        // told about any of them.
        let mut tree = tree_with(
            vec![DesignParameter::new("a", "area(f)")],
            every_expression_feature(),
        );
        for site in field_uses(&mut tree) {
            assert_eq!(site.expression, "a");
        }
        visit_expressions(&mut tree, |site| {
            *site.expression = "length(rim)".to_string();
        });
        let sites = measurement_sites(&mut tree);
        assert_eq!(
            sites.len(),
            49,
            "the 48 enumerated fields and one parameter: {sites:#?}"
        );
        assert_eq!(
            sites.iter().filter(|s| s.feature.is_none()).count(),
            1,
            "the parameter is the one site with no feature"
        );
        for site in &sites {
            assert_eq!(site.entities.len(), 1, "{} measures one entity", site.field);
        }
        assert!(tree_measures(&mut tree));

        // A pass with no model DEFERS every one of them: no error, nothing
        // written. Reporting "no geometry here" would put a permanent error
        // on a correct document every rebuild.
        let outcome = apply_parameters(&mut tree);
        assert_eq!(
            outcome
                .errors
                .iter()
                .filter(|(id, _)| *id != Uuid::nil())
                .count(),
            0,
            "a deferred measurement is silent: {:#?}",
            outcome.errors
        );
        assert!(outcome.changed.is_empty(), "{:?}", outcome.changed);
        assert!(
            tree.parameters[0].error.is_none(),
            "a deferred measuring parameter carries no error: {:?}",
            tree.parameters[0].error
        );
    }

    #[test]
    fn a_measurement_is_not_a_parameter_dependency_in_the_reverse_index() {
        // The namespaces are disjoint all the way out to P5's `used_by`.
        let mut tree = tree_with(
            vec![DesignParameter::new("w", "10")],
            every_expression_feature(),
        );
        visit_expressions(&mut tree, |site| {
            *site.expression = "distance(w, plate.face) + w".to_string();
        });
        for site in field_uses(&mut tree) {
            assert_eq!(
                site.reads,
                vec!["w".to_string()],
                "only the parameter `w` is read, never the entity `w`: {}",
                site.field
            );
        }
    }

    #[test]
    fn earliest_readers_positions_a_parameter_at_its_first_reader() {
        // `outer` is read by `inner`, which the extrude's depth reads. The
        // extrude is feature index 0 in this fixture, so BOTH names are
        // positioned there — a measuring `outer` may read nothing.
        let mut tree = tree_with(
            vec![
                DesignParameter::new("outer", "1"),
                DesignParameter::new("inner", "outer * 2"),
            ],
            vec![extrude_feature(0.004, Some("inner"))],
        );
        let readers = earliest_readers(&mut tree);
        assert_eq!(readers.get("inner"), Some(&0));
        assert_eq!(
            readers.get("outer"),
            Some(&0),
            "a parameter read through another parameter inherits its reader"
        );
        // A parameter nothing reads has no position, so nothing constrains a
        // measurement in it.
        let mut tree = tree_with(vec![DesignParameter::new("unread", "1")], Vec::new());
        assert_eq!(earliest_readers(&mut tree).get("unread"), None);
    }

    #[test]
    fn an_entity_rename_rewrites_expressions_and_restores() {
        let mut tree = tree_with(
            vec![DesignParameter::new("v", "volume(plate) / 1000")],
            vec![extrude_feature(0.004, Some("v"))],
        );
        visit_expressions(&mut tree, |site| {
            *site.expression = "area( plate.top )/plate_w".to_string();
        });
        let undo = rename_entity(&mut tree, "plate", "base");
        assert_eq!(tree.parameters[0].expression, "volume(base) / 1000");
        let after: Vec<String> = field_uses(&mut tree)
            .into_iter()
            .map(|u| u.expression)
            .collect();
        assert_eq!(
            after,
            vec!["area( plate.top )/plate_w".to_string()],
            "`plate.top` is a different name and `plate_w` is a parameter: neither is renamed"
        );
        // Undo puts the parameter back byte-for-byte.
        restore_entity_rename(&mut tree, &undo);
        assert_eq!(tree.parameters[0].expression, "volume(plate) / 1000");

        // The dotted name renames as the whole name it is.
        let undo = rename_entity(&mut tree, "plate.top", "base.lid");
        assert_eq!(
            field_uses(&mut tree)[0].expression,
            "area( base.lid )/plate_w"
        );
        restore_entity_rename(&mut tree, &undo);
        assert_eq!(
            field_uses(&mut tree)[0].expression,
            "area( plate.top )/plate_w"
        );
    }

    #[test]
    fn field_uses_names_the_feature_and_field_that_consume_a_parameter() {
        let extrude = extrude_feature(0.004, Some("height * 2"));
        let revolve = revolve_feature(90.0, Some("sweep"));
        let mut tree = tree_with(
            vec![param("height", "25"), param("sweep", "180")],
            vec![extrude, revolve],
        );
        let uses = field_uses(&mut tree);
        assert_eq!(uses.len(), 2);
        assert_eq!(uses[0].field, "depth");
        assert_eq!(uses[0].feature_name, "Extrude");
        assert_eq!(uses[0].reads, vec!["height".to_string()]);
        assert_eq!(uses[1].field, "angle");
        assert_eq!(uses[1].reads, vec!["sweep".to_string()]);
    }

    #[test]
    fn an_unparseable_field_is_a_site_with_no_dependency_list() {
        let mut tree = tree_with(Vec::new(), vec![extrude_feature(0.004, Some("h +"))]);
        let uses = field_uses(&mut tree);
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].expression, "h +");
        assert!(
            uses[0].reads.is_empty(),
            "an expression that does not parse has no dependency list"
        );
    }

    // -- cycles --

    #[test]
    fn cycles_names_the_loop_not_just_a_member() {
        let params = vec![
            param("a", "b + 1"),
            param("b", "c + 1"),
            param("c", "a + 1"),
            param("d", "5"),
        ];
        assert_eq!(
            cycles(&params),
            vec![vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "a".to_string()
            ]]
        );
    }

    #[test]
    fn a_self_reference_is_a_one_name_cycle() {
        assert_eq!(
            cycles(&[param("a", "a + 1")]),
            vec![vec!["a".to_string(), "a".to_string()]]
        );
    }

    #[test]
    fn two_independent_cycles_are_two_answers_and_order_free() {
        let forward = vec![
            param("a", "b"),
            param("b", "a"),
            param("x", "y"),
            param("y", "x"),
        ];
        let reversed: Vec<DesignParameter> = forward.iter().rev().cloned().collect();
        let expected = vec![
            vec!["a".to_string(), "b".to_string(), "a".to_string()],
            vec!["x".to_string(), "y".to_string(), "x".to_string()],
        ];
        assert_eq!(cycles(&forward), expected);
        assert_eq!(
            cycles(&reversed),
            expected,
            "the answer must not depend on table order"
        );
    }

    #[test]
    fn a_diamond_is_not_a_cycle() {
        let params = vec![
            param("a", "1"),
            param("b", "a * 2"),
            param("c", "a * 3"),
            param("d", "b + c"),
        ];
        assert!(cycles(&params).is_empty());
    }

    #[test]
    fn the_cycle_error_names_the_loop_and_a_downstream_reader_says_so() {
        let mut params = vec![param("a", "b"), param("b", "a"), param("reads_a", "a + 1")];
        evaluate_parameters(&mut params);
        assert_eq!(
            params[0].error.as_deref(),
            Some("circular reference: a → b → a")
        );
        assert_eq!(
            params[1].error.as_deref(),
            Some("circular reference: a → b → a")
        );
        // Not in the loop itself, but it can never resolve either, and the
        // message says which name it is waiting on and why.
        let downstream = params[2].error.as_deref().unwrap();
        assert!(
            downstream.contains("depends on 'a'")
                && downstream.contains("circular reference: a → b → a"),
            "{downstream}"
        );
    }

    // -- rename through the AST --

    #[test]
    fn a_rename_rewrites_dependents_and_leaves_a_longer_name_alone() {
        let extrude = extrude_feature(0.004, Some("w * 2 + w2"));
        let revolve = revolve_feature(90.0, Some("w2"));
        let mut tree = tree_with(
            vec![
                param("w", "10"),
                param("w2", "20"),
                // Spacing, parentheses and a unit suffix must survive.
                param("area", "w*(w  + 3mm)"),
            ],
            vec![extrude, revolve],
        );
        let undo = rename_parameter(&mut tree, "w", "width");

        assert_eq!(tree.parameters[0].name, "width");
        assert_eq!(
            tree.parameters[1].name, "w2",
            "a name that merely STARTS with the renamed one is not a reference"
        );
        assert_eq!(tree.parameters[2].expression, "width*(width  + 3mm)");
        let uses = field_uses(&mut tree);
        assert_eq!(uses[0].expression, "width * 2 + w2");
        assert_eq!(uses[1].expression, "w2", "untouched");

        // One field changed, and its previous text is recorded for undo.
        assert_eq!(undo.len(), 1);
        assert_eq!(undo[0].field, "depth");
        assert_eq!(undo[0].text, "w * 2 + w2");
        restore_expressions(&mut tree, &undo);
        assert_eq!(field_uses(&mut tree)[0].expression, "w * 2 + w2");
    }

    #[test]
    fn a_rename_does_not_touch_a_unit_suffix_or_a_function_name() {
        // `in` and `min` are reserved, so neither can BE a parameter — but a
        // string replace of `i` → `q` would still corrupt both. The rename
        // goes through the AST, which has no node for either.
        let mut tree = tree_with(
            vec![param("i", "1"), param("len", "min(i, 2in)")],
            Vec::new(),
        );
        rename_parameter(&mut tree, "i", "q");
        assert_eq!(tree.parameters[0].name, "q");
        assert_eq!(tree.parameters[1].expression, "min(q, 2in)");
    }

    #[test]
    fn a_rename_leaves_an_unparseable_expression_alone() {
        let mut tree = tree_with(
            vec![param("w", "10"), param("broken", "w +")],
            vec![extrude_feature(0.004, Some("w *"))],
        );
        let undo = rename_parameter(&mut tree, "w", "width");
        assert_eq!(
            tree.parameters[1].expression, "w +",
            "an expression nobody has parsed is not text to rewrite"
        );
        assert_eq!(field_uses(&mut tree)[0].expression, "w *");
        assert!(undo.is_empty());
    }

    /// A rename must reach EVERY enumerated site, not just the one a test
    /// happens to look at. `rename_parameter` and the reverse index walk the
    /// same enumeration, so this and `every_expression_field_is_enumerated`
    /// together say the rewrite covers a script argument and a 3D-sketch
    /// coordinate as surely as an extrude depth.
    #[test]
    fn a_rename_reaches_every_enumerated_site() {
        let mut tree = tree_with(vec![param("a", "1")], every_expression_feature());
        let before = field_uses(&mut tree).len();
        let undo = rename_parameter(&mut tree, "a", "alpha");
        assert_eq!(
            undo.len(),
            before,
            "every site read `a`, so every site must have been rewritten"
        );
        let after = field_uses(&mut tree);
        assert_eq!(after.len(), before);
        for site in &after {
            assert_eq!(
                site.expression, "alpha",
                "{} {} was left behind",
                site.feature_name, site.field
            );
            assert_eq!(site.reads, vec!["alpha".to_string()]);
        }
        assert_eq!(tree.parameters[0].name, "alpha");

        // And the undo record puts every one of them back.
        restore_expressions(&mut tree, &undo);
        for site in &field_uses(&mut tree) {
            assert_eq!(site.expression, "a", "{} {}", site.feature_name, site.field);
        }
    }

    #[test]
    fn a_renamed_parameter_still_drives_its_field() {
        let mut tree = tree_with(
            vec![param("h", "25")],
            vec![extrude_feature(0.004, Some("h"))],
        );
        apply_parameters(&mut tree);
        assert!((extrude_depth(&tree, 0) - 0.025).abs() < 1e-15);

        rename_parameter(&mut tree, "h", "height");
        let outcome = apply_parameters(&mut tree);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert!((extrude_depth(&tree, 0) - 0.025).abs() < 1e-15);
    }
}
