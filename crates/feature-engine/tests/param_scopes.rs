//! P2 — the document-level parameter table and per-instance overrides.
//!
//! `specs/agent_mechanical_design.md` §6 P2. Three claims are measured here:
//!
//! 1. A Part's expression resolves **local first, then document**, and the
//!    shadowing is total (a local row cannot read the document row it hides).
//! 2. An instance's `parameter_overrides` **pin** a parameter of the part, so
//!    one Part tab builds a different solid per instance — measured as a
//!    different tessellated extent, not just a different field.
//! 3. The build identity (`assembly::PartBuild`) separates those builds, so a
//!    geometry cache keyed on it cannot hand one instance another's bodies.

use std::collections::BTreeMap;

use feature_engine::assembly::{Instance, PartBuild, PartRef, Transform};
use feature_engine::expr::Dimension;
use feature_engine::params;
use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::{Kernel, MockKernel};
use waffle_types::*;

// ── fixtures ────────────────────────────────────────────────────────────────

/// A closed 20 mm × 10 mm rectangle with no driving dimension expression —
/// the profile every extrude below is built on.
fn rect_sketch() -> Sketch {
    let entities = vec![
        SketchEntity::Point {
            id: 1,
            x: 0.0,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: 2,
            x: 0.02,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: 3,
            x: 0.02,
            y: 0.01,
            construction: false,
        },
        SketchEntity::Point {
            id: 4,
            x: 0.0,
            y: 0.01,
            construction: false,
        },
        SketchEntity::Line {
            id: 5,
            start_id: 1,
            end_id: 2,
            construction: false,
        },
        SketchEntity::Line {
            id: 6,
            start_id: 2,
            end_id: 3,
            construction: false,
        },
        SketchEntity::Line {
            id: 7,
            start_id: 3,
            end_id: 4,
            construction: false,
        },
        SketchEntity::Line {
            id: 8,
            start_id: 4,
            end_id: 1,
            construction: false,
        },
    ];
    let mut sketch = Sketch {
        id: Uuid::new_v4(),
        plane: GeomRef {
            kind: TopoKind::Face,
            anchor: Anchor::Datum {
                datum_id: Uuid::new_v4(),
            },
            selector: Selector::Position {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            policy: ResolvePolicy::BestEffort,
            scope: None,
        },
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities,
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions: Default::default(),
        solved_profiles: Vec::new(),
        projected: Vec::new(),
        plane_face: None,
    };
    sketch.recompute_derived();
    sketch
}

fn extrude_op(sketch_id: Uuid, depth: f64, depth_expr: Option<&str>) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            profile_entity_ids: None,
            depth,
            depth_expr: depth_expr.map(str::to_string),
            direction: None,
            symmetric: false,
            cut: false,
            merge: false,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
            combine: Some(CombineMode::NewBody),
            targets: None,
        },
    }
}

/// A one-part tree: rectangle sketch + an extrude whose depth is `expr`.
/// Returns the tree and the extrude's feature id.
fn part_tree(parameters: Vec<DesignParameter>, depth_expr: &str) -> (FeatureTree, Uuid) {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let sketch_fid = engine
        .add_feature(
            "Sketch1".to_string(),
            Operation::Sketch {
                sketch: rect_sketch(),
            },
            &mut kernel,
        )
        .unwrap();
    let extrude_id = engine
        .add_feature(
            "Extrude1".to_string(),
            extrude_op(sketch_fid, 0.004, Some(depth_expr)),
            &mut kernel,
        )
        .unwrap();
    let mut tree = engine.tree.clone();
    tree.parameters = parameters;
    (tree, extrude_id)
}

fn depth_of(engine: &Engine, extrude_id: Uuid) -> f64 {
    match &engine.tree.find_feature(extrude_id).unwrap().operation {
        Operation::Extrude { params } => params.depth,
        other => panic!("expected extrude, got {other:?}"),
    }
}

/// The z thickness of the solid a feature produced, read off the tessellated
/// mesh. The SOLID's own answer, not the field the apply pass wrote — this is
/// what makes "two different solids" a measurement rather than a restatement.
///
/// `RenderMesh.vertices` is `f32`, so the answer carries that precision;
/// every assertion below compares it to a 0.1 µm band, which is four orders
/// of magnitude tighter than the depths it has to tell apart.
fn solid_thickness_m(kernel: &mut MockKernel, engine: &Engine, feature_id: Uuid) -> f64 {
    let result = engine
        .get_result(feature_id)
        .unwrap_or_else(|| panic!("feature {feature_id} produced no result"));
    let handle = &result.outputs[0].1.handle;
    let mesh = kernel.tessellate(handle, 1e-5).expect("tessellate");
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in mesh.vertices.chunks(3) {
        lo = lo.min(p[2] as f64);
        hi = hi.max(p[2] as f64);
    }
    hi - lo
}

/// `solid_thickness_m` within 0.1 µm of `want`.
#[track_caller]
fn assert_thickness(kernel: &mut MockKernel, engine: &Engine, feature_id: Uuid, want: f64) {
    let got = solid_thickness_m(kernel, engine, feature_id);
    assert!(
        (got - want).abs() < 1e-7,
        "the solid is {got} m thick, expected {want} m"
    );
}

/// Build `tree` as one placed occurrence: the document table plus this
/// instance's overrides, exactly as `assembly_view` does it.
fn build_instance(
    tree: &FeatureTree,
    document: &[DesignParameter],
    overrides: Option<BTreeMap<String, f64>>,
    kernel: &mut MockKernel,
) -> Engine {
    let mut engine = Engine::new();
    engine.tree = tree.clone();
    engine.document_parameters = document.to_vec();
    engine.parameter_overrides = overrides;
    engine.rebuild_from_scratch(kernel);
    engine
}

fn instance(source: PartRef, overrides: Option<BTreeMap<String, f64>>) -> Instance {
    Instance {
        id: Uuid::new_v4(),
        name: "Instance".to_string(),
        source,
        transform: Transform::default(),
        fixed: false,
        suppressed: false,
        external_key: None,
        parameter_overrides: overrides,
        extra: Default::default(),
    }
}

fn overrides(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
    pairs.iter().map(|(n, v)| ((*n).to_string(), *v)).collect()
}

// ── 1. the document scope ───────────────────────────────────────────────────

#[test]
fn a_tab_expression_resolves_through_the_document_table() {
    let mut kernel = MockKernel::new();
    let (tree, extrude_id) = part_tree(Vec::new(), "tube_id - 1.5");
    let mut engine = Engine::new();
    engine.tree = tree;
    // Nothing declares `tube_id` locally: without the document scope this is
    // an unknown variable and the depth keeps its stored 4 mm.
    engine.rebuild_from_scratch(&mut kernel);
    assert_eq!(depth_of(&engine, extrude_id), 0.004);
    assert_eq!(engine.errors.len(), 1, "{:?}", engine.errors);
    assert!(engine.errors[0].1.contains("unknown variable 'tube_id'"));

    engine.set_document_parameters(vec![DesignParameter::new("tube_id", "30")], &mut kernel);
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert!((depth_of(&engine, extrude_id) - 0.0285).abs() < 1e-15);
    // And the document row's evaluated value is readable where it lives, so
    // the host does not evaluate the table a second time to display it.
    assert_eq!(engine.document_parameters[0].value, 30.0);
    assert!(engine.document_parameters[0].error.is_none());
}

#[test]
fn a_document_parameter_edit_rebuilds_what_reads_it() {
    let mut kernel = MockKernel::new();
    let (tree, extrude_id) = part_tree(Vec::new(), "wall * 2");
    let mut engine = Engine::new();
    engine.tree = tree;
    engine.set_document_parameters(vec![DesignParameter::new("wall", "5")], &mut kernel);
    assert!((depth_of(&engine, extrude_id) - 0.010).abs() < 1e-15);
    assert_thickness(&mut kernel, &engine, extrude_id, 0.010);

    engine.set_document_parameters(vec![DesignParameter::new("wall", "9")], &mut kernel);
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert!((depth_of(&engine, extrude_id) - 0.018).abs() < 1e-15);
    // The SOLID follows the document table, not just the field.
    assert_thickness(&mut kernel, &engine, extrude_id, 0.018);
}

#[test]
fn a_local_parameter_shadows_a_document_one() {
    let mut kernel = MockKernel::new();
    let (tree, extrude_id) = part_tree(vec![DesignParameter::new("w", "7")], "w");
    let mut engine = Engine::new();
    engine.tree = tree;
    engine.set_document_parameters(vec![DesignParameter::new("w", "40")], &mut kernel);
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert!(
        (depth_of(&engine, extrude_id) - 0.007).abs() < 1e-15,
        "local first, then document"
    );
}

#[test]
fn a_shadowing_row_cannot_read_the_document_row_it_hides() {
    // The shadowing is TOTAL: `w = "w * 2"` over a document `w` is a
    // self-reference, not a silent doubling of a value this table cannot
    // otherwise see. Half-visibility would make `w` mean two things in one
    // table.
    let mut kernel = MockKernel::new();
    let (tree, extrude_id) = part_tree(vec![DesignParameter::new("w", "w * 2")], "w");
    let mut engine = Engine::new();
    engine.tree = tree;
    engine.set_document_parameters(vec![DesignParameter::new("w", "40")], &mut kernel);
    let messages: Vec<&str> = engine.errors.iter().map(|(_, m)| m.as_str()).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("circular reference: w → w")),
        "{messages:?}"
    );
    assert_eq!(
        depth_of(&engine, extrude_id),
        0.004,
        "the depth keeps its last-good value"
    );
}

#[test]
fn the_document_table_cannot_read_a_tab_parameter() {
    // The document scope is ABOVE the tabs: if it could read a Part's
    // parameters there would be no answer to which Part it meant.
    let mut kernel = MockKernel::new();
    let (tree, _) = part_tree(vec![DesignParameter::new("local", "3")], "local");
    let mut engine = Engine::new();
    engine.tree = tree;
    engine.set_document_parameters(
        vec![DesignParameter::new("derived", "local + 1")],
        &mut kernel,
    );
    let err = engine.document_parameters[0].error.as_deref().unwrap();
    assert!(err.contains("unknown variable 'local'"), "{err}");
    assert!(
        engine
            .errors
            .iter()
            .any(|(_, m)| m.starts_with("document parameter 'derived':")),
        "{:?}",
        engine.errors
    );
}

#[test]
fn a_document_parameter_declares_its_dimension_like_any_other() {
    let mut kernel = MockKernel::new();
    let (tree, extrude_id) = part_tree(Vec::new(), "turn");
    let mut engine = Engine::new();
    engine.tree = tree;
    engine.set_document_parameters(
        vec![DesignParameter::new("turn", "90").with_unit(Dimension::Angle)],
        &mut kernel,
    );
    assert_eq!(depth_of(&engine, extrude_id), 0.004, "an angle is no depth");
    assert_eq!(engine.errors.len(), 1, "{:?}", engine.errors);
    assert!(
        engine.errors[0]
            .1
            .contains("expected a length, got an angle"),
        "{}",
        engine.errors[0].1
    );
}

#[test]
fn the_preview_env_agrees_with_the_rebuild_across_both_scopes() {
    let mut document = vec![
        DesignParameter::new("w", "40"),
        DesignParameter::new("h", "2cm"),
    ];
    let mut local = vec![DesignParameter::new("w", "7")];
    let mut tree = FeatureTree::new();
    tree.parameters = local.clone();
    let rebuild = params::apply_parameters_scoped(&mut tree, &mut document, None, None);
    assert!(rebuild.errors.is_empty(), "{:?}", rebuild.errors);
    local = tree.parameters.clone();

    let preview = params::cached_env_in(&local, &document);
    assert_eq!(preview["w"].value, 7.0, "the local row shadows here too");
    assert_eq!(preview["h"].value, 20.0);
    assert_eq!(
        preview["h"].dimension(),
        Some(Dimension::Length),
        "a committed document dimension survives into the preview"
    );
    assert!(preview["h"].as_angle_degrees().is_err());
}

// ── 2. per-instance overrides ───────────────────────────────────────────────

#[test]
fn one_part_tab_builds_two_different_solids_from_two_instances() {
    // §6's oracle, measured on the solid: "a per-instance override produces
    // two different solids from one Part tab".
    let mut kernel = MockKernel::new();
    let (tree, extrude_id) = part_tree(vec![DesignParameter::new("height", "10")], "height");

    let short = build_instance(&tree, &[], Some(overrides(&[("height", 6.0)])), &mut kernel);
    let tall = build_instance(
        &tree,
        &[],
        Some(overrides(&[("height", 25.0)])),
        &mut kernel,
    );
    let plain = build_instance(&tree, &[], None, &mut kernel);

    assert!(short.errors.is_empty(), "{:?}", short.errors);
    assert!(tall.errors.is_empty(), "{:?}", tall.errors);
    assert!((depth_of(&short, extrude_id) - 0.006).abs() < 1e-15);
    assert!((depth_of(&tall, extrude_id) - 0.025).abs() < 1e-15);
    assert!((depth_of(&plain, extrude_id) - 0.010).abs() < 1e-15);

    assert_thickness(&mut kernel, &short, extrude_id, 0.006);
    assert_thickness(&mut kernel, &tall, extrude_id, 0.025);
    assert_thickness(&mut kernel, &plain, extrude_id, 0.010);
}

#[test]
fn an_override_pins_the_row_and_its_dependents_follow() {
    let mut kernel = MockKernel::new();
    let (tree, extrude_id) = part_tree(
        vec![
            DesignParameter::new("od", "20"),
            DesignParameter::new("wall", "od / 4"),
        ],
        "wall",
    );
    let plain = build_instance(&tree, &[], None, &mut kernel);
    assert!((depth_of(&plain, extrude_id) - 0.005).abs() < 1e-15);

    // Overriding `od` must reach `wall`, which means the pin enters the
    // environment BEFORE the fixpoint, not after it.
    let big = build_instance(&tree, &[], Some(overrides(&[("od", 40.0)])), &mut kernel);
    assert!(big.errors.is_empty(), "{:?}", big.errors);
    assert_eq!(big.tree.parameters[0].value, 40.0);
    assert_eq!(big.tree.parameters[1].value, 10.0);
    assert!((depth_of(&big, extrude_id) - 0.010).abs() < 1e-15);
}

#[test]
fn an_override_of_a_name_the_part_does_not_declare_is_loud() {
    let mut kernel = MockKernel::new();
    let (tree, extrude_id) = part_tree(vec![DesignParameter::new("height", "10")], "height");
    let engine = build_instance(
        &tree,
        &[],
        Some(overrides(&[("heigth", 25.0)])),
        &mut kernel,
    );
    assert_eq!(
        depth_of(&engine, extrude_id),
        0.010,
        "the typo must not quietly build the default"
    );
    assert_eq!(engine.errors.len(), 1, "{:?}", engine.errors);
    assert!(
        engine.errors[0]
            .1
            .contains("override of 'heigth': this part declares no parameter of that name"),
        "{}",
        engine.errors[0].1
    );
}

#[test]
fn an_override_takes_its_dimension_from_the_parameter_not_from_itself() {
    let mut kernel = MockKernel::new();
    // A declared Angle parameter overridden with a plain number is still an
    // angle, so a depth still refuses it.
    let (tree, extrude_id) = part_tree(
        vec![DesignParameter::new("turn", "90").with_unit(Dimension::Angle)],
        "turn",
    );
    let engine = build_instance(&tree, &[], Some(overrides(&[("turn", 45.0)])), &mut kernel);
    assert_eq!(depth_of(&engine, extrude_id), 0.004);
    assert!(
        engine
            .errors
            .iter()
            .any(|(_, m)| m.contains("expected a length, got an angle")),
        "{:?}",
        engine.errors
    );

    // And an expression that committed its own dimension keeps it.
    let (tree, extrude_id) = part_tree(vec![DesignParameter::new("turn", "90deg")], "turn");
    let engine = build_instance(&tree, &[], Some(overrides(&[("turn", 45.0)])), &mut kernel);
    assert_eq!(depth_of(&engine, extrude_id), 0.004);
    assert!(
        engine
            .errors
            .iter()
            .any(|(_, m)| m.contains("expected a length, got an angle")),
        "{:?}",
        engine.errors
    );
}

#[test]
fn a_count_parameter_refuses_a_fractional_override() {
    let mut kernel = MockKernel::new();
    let (tree, _) = part_tree(
        vec![DesignParameter::new("teeth", "20").with_unit(Dimension::Count)],
        "teeth",
    );
    let engine = build_instance(&tree, &[], Some(overrides(&[("teeth", 20.5)])), &mut kernel);
    assert!(
        engine
            .errors
            .iter()
            .any(|(_, m)| m.contains("override of 'teeth'") && m.contains("whole non-negative")),
        "{:?}",
        engine.errors
    );
}

#[test]
fn an_override_and_the_document_table_compose() {
    let mut kernel = MockKernel::new();
    let document = vec![DesignParameter::new("stock", "12")];
    let (tree, extrude_id) = part_tree(vec![DesignParameter::new("t", "stock")], "t");

    let plain = build_instance(&tree, &document, None, &mut kernel);
    assert!((depth_of(&plain, extrude_id) - 0.012).abs() < 1e-15);

    // The override wins over the document value it would otherwise inherit.
    let thin = build_instance(
        &tree,
        &document,
        Some(overrides(&[("t", 3.0)])),
        &mut kernel,
    );
    assert!(thin.errors.is_empty(), "{:?}", thin.errors);
    assert!((depth_of(&thin, extrude_id) - 0.003).abs() < 1e-15);
}

// ── 3. build identity (the cache key) ───────────────────────────────────────

#[test]
fn two_instances_with_different_overrides_are_different_builds() {
    let part = PartRef {
        source_id: None,
        tab_id: "tab-a".to_string(),
    };
    let a = PartBuild::of(&instance(part.clone(), Some(overrides(&[("h", 6.0)]))));
    let b = PartBuild::of(&instance(part.clone(), Some(overrides(&[("h", 25.0)]))));
    let plain = PartBuild::of(&instance(part.clone(), None));
    assert_ne!(a, b, "a cache keyed on this must not conflate them");
    assert_ne!(a, plain);
    assert_eq!(
        a,
        PartBuild::of(&instance(part.clone(), Some(overrides(&[("h", 6.0)]))))
    );
    assert_eq!(plain, PartBuild::plain(part.clone()));
}

#[test]
fn an_empty_override_map_is_the_plain_build() {
    // Otherwise `{}` and absent are two cache entries for one solid.
    let part = PartRef {
        source_id: None,
        tab_id: "tab-a".to_string(),
    };
    assert_eq!(
        PartBuild::of(&instance(part.clone(), Some(BTreeMap::new()))),
        PartBuild::plain(part.clone())
    );
}

#[test]
fn two_instances_of_the_same_part_are_the_same_build() {
    let part = PartRef {
        source_id: None,
        tab_id: "tab-a".to_string(),
    };
    let same = overrides(&[("h", 6.0), ("w", 2.0)]);
    // Insertion order cannot change the key: a BTreeMap is ordered.
    let reversed = overrides(&[("w", 2.0), ("h", 6.0)]);
    assert_eq!(
        PartBuild::of(&instance(part.clone(), Some(same))),
        PartBuild::of(&instance(part.clone(), Some(reversed)))
    );
}

#[test]
fn a_different_part_tab_is_a_different_build_whatever_the_overrides() {
    let a = PartRef {
        source_id: None,
        tab_id: "tab-a".to_string(),
    };
    let b = PartRef {
        source_id: None,
        tab_id: "tab-b".to_string(),
    };
    assert_ne!(PartBuild::plain(a.clone()), PartBuild::plain(b.clone()));
    let o = overrides(&[("h", 6.0)]);
    assert_ne!(
        PartBuild::of(&instance(a, Some(o.clone()))),
        PartBuild::of(&instance(b, Some(o)))
    );
}

#[test]
fn a_table_signature_ignores_the_evaluated_output() {
    // The cache compares what a table SAYS. `value`/`error`/`tag` are the
    // last evaluation's output, so comparing whole rows would call one table
    // two things depending on whether it had been evaluated yet.
    let fresh = vec![DesignParameter::new("w", "20")];
    let mut evaluated = fresh.clone();
    params::evaluate_parameters(&mut evaluated);
    assert_eq!(evaluated[0].value, 20.0);
    assert_eq!(
        params::table_signature(&fresh),
        params::table_signature(&evaluated)
    );

    // But a changed expression, name or declared unit IS a different table.
    let renamed = vec![DesignParameter::new("w2", "20")];
    let rewritten = vec![DesignParameter::new("w", "21")];
    let declared = vec![DesignParameter::new("w", "20").with_unit(Dimension::Length)];
    for other in [renamed, rewritten, declared] {
        assert_ne!(
            params::table_signature(&fresh),
            params::table_signature(&other)
        );
    }
}
