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

use crate::expr::{self, ExprError, MM_TO_METERS};
use crate::types::{DesignParameter, FeatureTree, Operation, PlaneDefinition};
use waffle_types::{DimensionUnit, SketchEntity, SolveStatus};

/// Result of the apply pass.
#[derive(Debug, Default)]
pub struct ParamOutcome {
    /// Lowest feature index whose effective values changed (rebuild must
    /// start at or before it). `None` = nothing changed.
    pub first_changed: Option<usize>,
    /// Loud errors: parameter-table errors carry the parameter's id;
    /// feature-expression errors carry the feature's id.
    pub errors: Vec<(Uuid, String)>,
}

/// Evaluate the parameter table into name → mm-space value, refreshing each
/// parameter's cached `value` and `error` in place. Order-independent:
/// unresolved parameters are retried until a fixpoint, so forward references
/// work; leftovers (unknown names, cycles) get per-parameter errors and keep
/// their last-good cached value.
pub fn evaluate_parameters(params: &mut [DesignParameter]) -> HashMap<String, f64> {
    let mut env: HashMap<String, f64> = HashMap::new();

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
            match expr::evaluate(&params[i].expression, &env) {
                Ok(v) => {
                    env.insert(params[i].name.clone(), v);
                    params[i].value = v;
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
        let msg = match expr::evaluate(&params[i].expression, &env) {
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

/// Environment from the parameters' CACHED values (no re-evaluation): every
/// parameter whose last evaluation succeeded, first-of-name wins. Used by the
/// bridge's stateless expression preview, which must match what the next
/// rebuild will compute without mutating anything.
pub fn cached_env(params: &[DesignParameter]) -> HashMap<String, f64> {
    let mut env = HashMap::new();
    for p in params {
        if p.error.is_none() {
            env.entry(p.name.clone()).or_insert(p.value);
        }
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
            Operation::Extrude { params } => apply_length_field(
                "depth",
                &mut params.depth,
                params.depth_expr.as_deref(),
                &env,
                &mut errs,
            ),
            Operation::Revolve { params } => apply_angle_field(
                "angle",
                &mut params.angle,
                params.angle_expr.as_deref(),
                &env,
                &mut errs,
            ),
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
                    apply_length_field("distance", distance, expr.as_deref(), &env, &mut errs)
                }
                PlaneDefinition::PointNormal { .. } => false,
            },
            Operation::Sketch { sketch } => apply_sketch(sketch, &env, &mut errs),
            // Fillet/chamfer/shell are deferred (disabled in the UI);
            // booleans and imports carry no dimension measurements.
            _ => false,
        };
        if changed {
            outcome.first_changed = Some(outcome.first_changed.map_or(idx, |c| c.min(idx)));
        }
        for e in errs {
            outcome
                .errors
                .push((feature.id, format!("{}: {}", feature.name, e)));
        }
    }

    outcome
}

/// Evaluate a length expression (mm-space → meters) into `field`.
/// Returns true if the value changed. Errors leave the field untouched.
fn apply_length_field(
    label: &str,
    field: &mut f64,
    expression: Option<&str>,
    env: &HashMap<String, f64>,
    errs: &mut Vec<String>,
) -> bool {
    let Some(expression) = expression else {
        return false;
    };
    match expr::evaluate(expression, env) {
        Ok(v) => {
            let meters = v * MM_TO_METERS;
            if meters != *field {
                *field = meters;
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

/// Evaluate an angle expression (degrees, verbatim) into `field`.
fn apply_angle_field(
    label: &str,
    field: &mut f64,
    expression: Option<&str>,
    env: &HashMap<String, f64>,
    errs: &mut Vec<String>,
) -> bool {
    let Some(expression) = expression else {
        return false;
    };
    match expr::evaluate(expression, env) {
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

/// Re-evaluate a sketch's expression-driven dimensions; if any value changed,
/// re-solve the sketch from its current geometry and recompute derived data.
/// Returns true if the sketch's geometry was updated.
fn apply_sketch(
    sketch: &mut waffle_types::Sketch,
    env: &HashMap<String, f64>,
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
        match expr::evaluate(&expression, env) {
            Ok(v) => {
                let new_value = match unit {
                    DimensionUnit::Length => v * MM_TO_METERS,
                    DimensionUnit::AngleDegrees => v,
                };
                let old = c.dimension_value().unwrap_or(0.0);
                if new_value != old {
                    c.set_dimension_value(new_value);
                    changed.push((i, old));
                }
            }
            Err(e) => errs.push(format!("dimension expression '{expression}': {e}")),
        }
    }
    if changed.is_empty() {
        return false;
    }

    // Re-solve with DRIVING constraints only (reference dims display, never
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
            // dimension values; the error names the failed re-solve.
            for (i, old) in changed {
                sketch.constraints[i].set_dimension_value(old);
            }
            let reason = match &solved.status {
                SolveStatus::SolveFailed { reason } => reason.clone(),
                _ => "over-constrained".to_string(),
            };
            errs.push(format!(
                "sketch re-solve failed after applying dimension expressions ({reason}); \
                 previous dimensions kept"
            ));
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
        assert_eq!(env.get("a"), Some(&10.0));
        assert_eq!(env.get("b"), Some(&20.0));
        assert_eq!(params[0].value, 20.0);
        assert!(params[0].error.is_none());
        assert_eq!(params[1].value, 10.0);
    }

    #[test]
    fn table_reports_cycles_without_hanging() {
        let mut params = vec![param("a", "b + 1"), param("b", "a + 1"), param("c", "5")];
        let env = evaluate_parameters(&mut params);
        assert_eq!(env.get("c"), Some(&5.0));
        assert!(!env.contains_key("a"));
        assert!(params[0].error.as_deref().unwrap().contains("circular"));
        assert!(params[1].error.as_deref().unwrap().contains("circular"));
        assert!(params[2].error.is_none());
    }

    #[test]
    fn table_reports_duplicates_and_bad_names_first_wins() {
        let mut params = vec![param("w", "1"), param("w", "2"), param("mm", "3")];
        let env = evaluate_parameters(&mut params);
        assert_eq!(env.get("w"), Some(&1.0));
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

    fn tree_with(parameters: Vec<DesignParameter>, features: Vec<Feature>) -> FeatureTree {
        FeatureTree {
            features,
            active_index: None,
            body_names: Default::default(),
            parameters,
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
            },
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
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
