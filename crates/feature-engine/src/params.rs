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

use crate::expr::{self, Dimension, Env, ExprError, Quantity, Span};
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
    let mut env: Env = Env::new();

    // Pre-validate names; mark duplicates (first occurrence wins).
    let mut pending: Vec<usize> = Vec::new();
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
        pending.push(i);
    }

    // Fixpoint resolution: each round evaluates what it can against the
    // already-resolved set. A round with no progress means the leftovers are
    // cycles or reference unknown names.
    loop {
        let mut progressed = false;
        let mut still_pending = Vec::new();
        for &i in &pending {
            match evaluate_declared(&params[i], &env) {
                Ok(q) => {
                    env.insert(params[i].name.clone(), q);
                    params[i].value = q.value;
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
        let msg = match evaluate_declared(&params[i], &env) {
            Err(ExprError::UnknownIdentifier(name)) if seen.contains_key(&name) => {
                format!("circular reference involving '{name}'")
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
fn evaluate_declared(param: &DesignParameter, env: &Env) -> Result<Quantity, ExprError> {
    let q = expr::evaluate_quantity(&param.expression, env)?;
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
    let mut env = Env::new();
    for p in params {
        if p.error.is_some() {
            continue;
        }
        // The cached magnitude plus the declared dimension, which is what
        // the next rebuild will put in the environment.
        let q = Quantity::untagged(p.value);
        let q = match p.unit {
            Some(want) => q.retag(want, Span::new(0, p.expression.len())).unwrap_or(q),
            None => q,
        };
        env.entry(p.name.clone()).or_insert(q);
    }
    env
}

/// Evaluate + apply all expressions on the tree. See module docs.
pub fn apply_parameters(tree: &mut FeatureTree) -> ParamOutcome {
    let mut outcome = ParamOutcome::default();

    let env = evaluate_parameters(&mut tree.parameters);
    for p in &tree.parameters {
        if let Some(err) = &p.error {
            outcome
                .errors
                .push((p.id, format!("parameter '{}': {}", p.name, err)));
        }
    }

    for (idx, feature) in tree.features.iter_mut().enumerate() {
        let mut errs: Vec<String> = Vec::new();
        let changed = match &mut feature.operation {
            Operation::Extrude { params } => apply_field(
                "depth",
                Dimension::Length,
                &mut params.depth,
                params.depth_expr.as_deref(),
                &env,
                &mut errs,
            ),
            Operation::Revolve { params } => apply_field(
                "angle",
                Dimension::Angle,
                &mut params.angle,
                params.angle_expr.as_deref(),
                &env,
                &mut errs,
            ),
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
                PlaneDefinition::PointNormal { .. } => false,
            },
            Operation::Sketch { sketch } => apply_sketch(sketch, &env, &mut errs),
            Operation::Sketch3d { sketch } => apply_sketch3d(sketch, &env, &mut errs),
            Operation::PatternCircular { params } => apply_field(
                "angle",
                Dimension::Angle,
                &mut params.angle_deg,
                params.angle_expr.as_deref(),
                &env,
                &mut errs,
            ),
            Operation::Script { params } => {
                // Expression-driven script arguments: evaluate each into the
                // raw (mm-space / degrees / plain) cache. The declared type
                // lives in the script's header, which is in the document's
                // sources table and not reachable from here, so the DIMENSION
                // travels with the value in `arg_dimensions` and the script
                // layer checks it against the `@param` type it declared.
                let mut changed = false;
                for (name, expression) in &params.arg_exprs {
                    match expr::evaluate_quantity(expression, &env) {
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
                }
                changed
            }
            // Fillet/chamfer/shell are deferred (disabled in the UI);
            // booleans and imports carry no dimension measurements.
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
    env: &Env,
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
    env: &Env,
    errs: &mut Vec<String>,
) -> bool {
    let Some(expression) = expression else {
        return false;
    };
    match expr::evaluate_quantity(expression, env).and_then(|q| q.accept(dimension)) {
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

/// Re-evaluate a sketch's expression-driven dimensions; if any value changed —
/// or the sketch has never been solved (`SolveStatus::Unsolved`, v4 §2.10:
/// written by a tool that did not run the solver) — solve it from its current
/// geometry and recompute derived data. Returns true if the sketch's geometry
/// or status was updated.
fn apply_sketch(sketch: &mut waffle_types::Sketch, env: &Env, errs: &mut Vec<String>) -> bool {
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
        // The constraint's own unit IS the dimension it asks for: a length
        // dim stores meters, an angle dim degrees. A `25deg` on a length
        // dimension is refused here rather than read as 25 mm.
        let dimension = sketch_dimension(unit);
        match expr::evaluate_quantity(&expression, env).and_then(|q| q.accept(dimension)) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DepthMode, ExtrudeParams, Feature, RevolveParams};
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
}
