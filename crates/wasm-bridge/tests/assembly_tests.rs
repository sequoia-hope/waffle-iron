//! Assemblies at the bridge (v4 Phase 3b): `OpenAssembly` builds each
//! distinct part once, derives connector frames from the parts' geometry,
//! solves placements, and reports them in `ModelUpdated.assembly`; the
//! evaluated view is what the per-body accessors enumerate. Run with the
//! real kernel-v2 adapter.

use std::collections::{BTreeMap, HashMap};

use feature_engine::assembly::{
    AssemblyTree, AxialAnchor, Frame, Instance, Mate, MateConnector, MateKind, PartRef, Transform,
};
use feature_engine::types::*;
use kernel_v2::KernelV2Adapter;
use modeling_ops::KernelBundle;
use serde_json::Map;
use uuid::Uuid;
use waffle_types::{Anchor, GeomRef, OutputKey, ResolvePolicy, Selector, TopoKind};
use wasm_bridge::messages::*;
use wasm_bridge::*;

const CUBE_STEP: &str = include_str!("../../step-import/tests/fixtures/cube.step");

/// A Part tab tree: the 10 mm cube imported from STEP (through the bridge so
/// the source table holds its content).
fn cube_part(state: &mut EngineState, kernel: &mut KernelV2Adapter) -> FeatureTree {
    dispatch(
        state,
        UiToEngine::ImportStep {
            file_name: "cube.step".into(),
            data: CUBE_STEP.to_string(),
        },
        kernel,
    );
    state.engine.tree.clone()
}

fn instance(name: &str, tab: &str, t: Transform, fixed: bool) -> Instance {
    Instance {
        id: Uuid::new_v4(),
        name: name.into(),
        source: PartRef {
            source_id: None,
            tab_id: tab.into(),
        },
        transform: t,
        fixed,
        suppressed: false,
        external_key: None,
        parameter_overrides: None,
        extra: Map::new(),
    }
}

fn connector(name: &str, inst: Uuid, geom_ref: Option<GeomRef>, frame: Frame) -> MateConnector {
    MateConnector {
        id: Uuid::new_v4(),
        name: name.into(),
        instance_path: vec![inst],
        geom_ref,
        part_connector: None,
        frame,
        anchor: AxialAnchor::Middle,
        flip_z: false,
        rotation_deg: 0.0,
        offset_m: [0.0; 3],
        extra: Map::new(),
    }
}

fn fastened(a: Uuid, b: Uuid, flip: bool) -> Mate {
    Mate {
        id: Uuid::new_v4(),
        name: "m".into(),
        kind: MateKind::Fastened {
            flip,
            rotation_deg: 0.0,
        },
        connectors: [a, b],
        suppressed: false,
        extra: Map::new(),
    }
}

fn open(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    tree: &AssemblyTree,
    parts: &HashMap<String, FeatureTree>,
) -> AssemblyStatus {
    let r = dispatch(
        state,
        UiToEngine::OpenAssembly {
            assembly: tree.clone(),
            part_trees: parts.clone(),
            assembly_trees: HashMap::new(),
        },
        kernel,
    );
    let EngineToUi::ModelUpdated { assembly, .. } = r else {
        panic!("{r:?}")
    };
    assembly.expect("assembly status while an assembly is open")
}

#[test]
fn open_assembly_builds_parts_once_places_instances_and_reports_placements() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let part = cube_part(&mut state, &mut kernel);
    let parts: HashMap<String, FeatureTree> = HashMap::from([("part".to_string(), part)]);

    let a = instance("A", "part", Transform::identity(), true);
    let b = instance("B", "part", Transform::translation([0.03, 0.0, 0.0]), false);
    let (ida, idb) = (a.id, b.id);
    let tree = AssemblyTree {
        instances: vec![a, b],
        ..Default::default()
    };
    let status = open(&mut state, &mut kernel, &tree, &parts);
    assert!(status.errors.is_empty(), "{:?}", status.errors);
    assert_eq!(status.parts.len(), 1, "one distinct part built once");
    assert_eq!(status.placements.len(), 2);
    assert_eq!(status.placements[&idb].translation_m, [0.03, 0.0, 0.0]);
    assert!(
        status.warnings.iter().any(|w| w.contains("`B`")),
        "{:?}",
        status.warnings
    );

    // The live tree is empty while an assembly is open; the view holds the
    // part engine with the cube built.
    assert!(state.engine.tree.features.is_empty());
    let view = state.assembly.as_ref().unwrap();
    assert_eq!(view.parts.len(), 1);
    assert!(
        view.parts[0].1.errors.is_empty(),
        "{:?}",
        view.parts[0].1.errors
    );
    assert_eq!(view.parts[0].1.feature_results.len(), 1);
    assert!(view.engine_for_instance(ida).is_some());
    assert!(view
        .placement(idb)
        .approx_eq(&Transform::translation([0.03, 0.0, 0.0]), 1e-12));
    let _ = ida;

    // Switching to a Part tab drops the assembly view.
    dispatch(
        &mut state,
        UiToEngine::SwitchTab {
            features: FeatureTree::new(),
        },
        &mut kernel,
    );
    assert!(state.assembly.is_none());
}

#[test]
fn connector_frames_come_from_the_parts_geometry_and_fastened_stacks_the_cubes() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let part = cube_part(&mut state, &mut kernel);
    let import_id = part.features[0].id;
    let parts: HashMap<String, FeatureTree> = HashMap::from([("part".to_string(), part)]);

    let a = instance("A", "part", Transform::identity(), true);
    let b = instance("B", "part", Transform::identity(), false);
    let (ida, idb) = (a.id, b.id);
    // A's connector on a real face of the imported cube: the face whose
    // outward normal is +z (found through the engine's own resolution of a
    // signature selector). B's connector: explicit bottom-face frame.
    let top_face = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: import_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Signature {
            signature: waffle_types::TopoSignature {
                surface_type: Some("planar".into()),
                normal: Some([0.0, 0.0, 1.0]),
                ..waffle_types::TopoSignature::empty()
            },
        },
        policy: ResolvePolicy::BestEffort,
        scope: None,
    };
    let ca = connector("A top", ida, Some(top_face), Frame::default());
    let cb = connector(
        "B bottom",
        idb,
        None,
        Frame::on_plane([0.005, 0.005, 0.0], [0.0, 0.0, -1.0]),
    );
    let (cida, cidb) = (ca.id, cb.id);
    let tree = AssemblyTree {
        instances: vec![a, b],
        connectors: vec![ca, cb],
        mates: vec![fastened(cida, cidb, true)],
        ..Default::default()
    };
    let status = open(&mut state, &mut kernel, &tree, &parts);
    assert!(status.errors.is_empty(), "{:?}", status.errors);
    assert!(status.warnings.is_empty(), "{:?}", status.warnings);
    let view = state.assembly.as_ref().unwrap();
    let fa = view.frames[&cida];
    // Derived from geometry: the cube's top face is at z = 10 mm, normal +z.
    assert!((fa.origin[2] - 0.01).abs() < 1e-6, "{fa:?}");
    assert!((fa.z_axis[2] - 1.0).abs() < 1e-9, "{fa:?}");
    // B stacked on A, upright.
    let tb = status.placements[&idb];
    assert!((tb.translation_m[2] - 0.01).abs() < 1e-6, "{tb:?}");
    assert!((tb.rotation_quat[3].abs() - 1.0).abs() < 1e-9, "{tb:?}");
    let _ = ida;
}

#[test]
fn a_part_the_document_lacks_and_a_bad_face_are_loud_but_the_rest_renders() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let part = cube_part(&mut state, &mut kernel);
    let parts: HashMap<String, FeatureTree> = HashMap::from([("part".to_string(), part)]);

    let a = instance("A", "part", Transform::identity(), true);
    let ghost = instance("Ghost", "missing-tab", Transform::identity(), false);
    let ida = a.id;
    let bad_face = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: Uuid::new_v4(),
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: waffle_types::roles::Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
        scope: None,
    };
    let c = connector(
        "A ?",
        ida,
        Some(bad_face),
        Frame::on_plane([0.0; 3], [0.0, 0.0, 1.0]),
    );
    let tree = AssemblyTree {
        instances: vec![a, ghost],
        connectors: vec![c],
        placements: BTreeMap::new(),
        ..Default::default()
    };
    let status = open(&mut state, &mut kernel, &tree, &parts);
    assert!(
        status.errors.iter().any(|e| e.contains("missing-tab")),
        "{:?}",
        status.errors
    );
    assert!(
        status
            .errors
            .iter()
            .any(|e| e.contains("could not be derived from its geometry")),
        "{:?}",
        status.errors
    );
    assert_eq!(status.parts.len(), 1);
    // The buildable instance is still placed.
    assert!(status.placements.contains_key(&ida));
}

// ── 3d-2 sub-assemblies, 3d-3b linked-source tabs ───────────────────────

#[test]
fn a_sub_assembly_instance_renders_its_members_with_composed_placements_and_connectors_reach_them()
{
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let part = cube_part(&mut state, &mut kernel);
    let parts: HashMap<String, FeatureTree> = HashMap::from([("part".to_string(), part)]);

    // Sub-assembly "stack": two cubes fastened (B on A).
    let a = instance("A", "part", Transform::identity(), true);
    let b = instance("B", "part", Transform::identity(), false);
    let (ida, idb) = (a.id, b.id);
    let ca = connector(
        "A top",
        ida,
        None,
        Frame::on_plane([0.005, 0.005, 0.01], [0.0, 0.0, 1.0]),
    );
    let cb = connector(
        "B bottom",
        idb,
        None,
        Frame::on_plane([0.005, 0.005, 0.0], [0.0, 0.0, -1.0]),
    );
    let (cida, cidb) = (ca.id, cb.id);
    let stack = AssemblyTree {
        instances: vec![a, b],
        connectors: vec![ca, cb],
        mates: vec![fastened(cida, cidb, true)],
        ..Default::default()
    };
    let asm_trees: HashMap<String, AssemblyTree> = HashMap::from([("stack".to_string(), stack)]);

    // Top: two instances of the stack, the second moved 50 mm in x, plus a
    // lone cube fastened onto the FIRST stack's top cube (connector path
    // [stack1, B]).
    let s1 = instance("Stack 1", "stack", Transform::identity(), true);
    let s2 = instance(
        "Stack 2",
        "stack",
        Transform::translation([0.05, 0.0, 0.0]),
        false,
    );
    let lone = instance("Lone", "part", Transform::identity(), false);
    let (ids1, ids2, idl) = (s1.id, s2.id, lone.id);
    let mut c_top_of_b = connector(
        "stack1 B top",
        ids1,
        None,
        Frame::on_plane([0.005, 0.005, 0.01], [0.0, 0.0, 1.0]),
    );
    c_top_of_b.instance_path = vec![ids1, idb];
    let c_lone = connector(
        "lone bottom",
        idl,
        None,
        Frame::on_plane([0.005, 0.005, 0.0], [0.0, 0.0, -1.0]),
    );
    let (cid_top, cid_lone) = (c_top_of_b.id, c_lone.id);
    let top = AssemblyTree {
        instances: vec![s1, s2, lone],
        connectors: vec![c_top_of_b, c_lone],
        mates: vec![fastened(cid_top, cid_lone, true)],
        ..Default::default()
    };

    let r = dispatch(
        &mut state,
        UiToEngine::OpenAssembly {
            assembly: top,
            part_trees: parts,
            assembly_trees: asm_trees,
        },
        &mut kernel,
    );
    let EngineToUi::ModelUpdated { assembly, .. } = r else {
        panic!("{r:?}")
    };
    let status = assembly.unwrap();
    assert!(status.errors.is_empty(), "{:?}", status.errors);
    let view = state.assembly.as_ref().unwrap();
    assert_eq!(view.parts.len(), 1, "the cube is built once for everything");
    // Leaves: [s1,A], [s1,B], [s2,A], [s2,B], [lone].
    let paths: Vec<Vec<Uuid>> = view.leaves.iter().map(|l| l.path.clone()).collect();
    assert_eq!(
        paths,
        vec![
            vec![ids1, ida],
            vec![ids1, idb],
            vec![ids2, ida],
            vec![ids2, idb],
            vec![idl]
        ]
    );
    let z_of = |p: &[Uuid]| {
        view.leaves
            .iter()
            .find(|l| l.path == p)
            .unwrap()
            .transform
            .translation_m
    };
    assert!((z_of(&[ids1, idb])[2] - 0.01).abs() < 1e-6);
    assert!(
        (z_of(&[ids2, idb])[2] - 0.01).abs() < 1e-6 && (z_of(&[ids2, idb])[0] - 0.05).abs() < 1e-6
    );
    // The lone cube sits on stack 1's TOP cube: z = 20 mm.
    let lone_t = status.placements[&idl];
    assert!((lone_t.translation_m[2] - 0.02).abs() < 1e-6, "{lone_t:?}");
    assert!(lone_t.translation_m[0].abs() < 1e-6, "{lone_t:?}");

    // A self-referencing sub-assembly is a loud error, not a hang.
    let selfref = instance("Me", "loop", Transform::identity(), false);
    let looping = AssemblyTree {
        instances: vec![selfref],
        ..Default::default()
    };
    let r = dispatch(
        &mut state,
        UiToEngine::OpenAssembly {
            assembly: looping.clone(),
            part_trees: HashMap::new(),
            assembly_trees: HashMap::from([("loop".to_string(), looping)]),
        },
        &mut kernel,
    );
    let EngineToUi::ModelUpdated { assembly, .. } = r else {
        panic!("{r:?}")
    };
    assert!(assembly.unwrap().errors.iter().any(|e| e.contains("cycle")));
}

#[test]
fn list_source_tabs_reads_a_linked_document_and_its_parts_can_be_instanced() {
    use file_format::{SourceEntry, SourceKind, WaffleDocument};
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let part = cube_part(&mut state, &mut kernel);

    // A linked .waffle document with a Part tab (the cube) and an Assembly tab.
    let mut linked = WaffleDocument::new("Linked");
    let linked_part_tab = linked.tabs[0].id.clone();
    if let file_format::TabKind::Part { features, .. } = &mut linked.tabs[0].kind {
        *features = part;
    }
    linked
        .tabs
        .push(file_format::Tab::assembly("Asm", AssemblyTree::default()));
    // Its own STEP source must travel with it for the cube to build.
    linked.sources = state.sources.clone();
    let linked_json = file_format::save_document(&linked);
    let entry = SourceEntry::embedded("linked.waffle", SourceKind::Waffle, &linked_json);
    let sid = entry.id;
    state.engine.sources.insert_text(sid, &linked_json);
    state.sources.push(entry);

    let r = dispatch(
        &mut state,
        UiToEngine::ListSourceTabs { source_id: sid },
        &mut kernel,
    );
    let EngineToUi::SourceTabsListed { tabs, .. } = r else {
        panic!("{r:?}")
    };
    assert_eq!(tabs.len(), 2);
    assert_eq!(
        (tabs[0].id.as_str(), tabs[0].kind.as_str()),
        (linked_part_tab.as_str(), "Part")
    );
    assert_eq!(tabs[1].kind, "Assembly");

    let inst = Instance {
        id: Uuid::new_v4(),
        name: "Linked cube".into(),
        source: PartRef {
            source_id: Some(sid),
            tab_id: linked_part_tab,
        },
        transform: Transform::identity(),
        fixed: true,
        suppressed: false,
        external_key: None,
        parameter_overrides: None,
        extra: Map::new(),
    };
    let tree = AssemblyTree {
        instances: vec![inst],
        ..Default::default()
    };
    let status = open(&mut state, &mut kernel, &tree, &HashMap::new());
    assert!(status.errors.is_empty(), "{:?}", status.errors);
    assert_eq!(status.parts.len(), 1);
    assert!(status.parts[0].source_id == Some(sid));
    let view = state.assembly.as_ref().unwrap();
    assert_eq!(view.leaves.len(), 1);
    assert_eq!(view.parts[0].1.feature_results.len(), 1);

    let bad = dispatch(
        &mut state,
        UiToEngine::ListSourceTabs {
            source_id: Uuid::new_v4(),
        },
        &mut kernel,
    );
    assert!(matches!(bad, EngineToUi::Error { .. }));
}

// ── In-context editing (Phase 3d-4) ────────────────────────────────────────

/// A sketch feature on `plane` (with a deliberately stale snapshot plane) —
/// the unit square, so an extrude of it builds.
fn sketch_on(plane: GeomRef) -> Operation {
    let mut solved_positions = HashMap::new();
    solved_positions.insert(1, (0.0, 0.0));
    solved_positions.insert(2, (0.002, 0.0));
    solved_positions.insert(3, (0.002, 0.002));
    solved_positions.insert(4, (0.0, 0.002));
    Operation::Sketch {
        sketch: waffle_types::Sketch {
            id: Uuid::new_v4(),
            plane,
            plane_origin: [9.0, 9.0, 9.0],
            plane_normal: [1.0, 0.0, 0.0],
            entities: vec![
                waffle_types::SketchEntity::Point {
                    id: 1,
                    x: 0.0,
                    y: 0.0,
                    construction: false,
                },
                waffle_types::SketchEntity::Point {
                    id: 2,
                    x: 0.002,
                    y: 0.0,
                    construction: false,
                },
                waffle_types::SketchEntity::Point {
                    id: 3,
                    x: 0.002,
                    y: 0.002,
                    construction: false,
                },
                waffle_types::SketchEntity::Point {
                    id: 4,
                    x: 0.0,
                    y: 0.002,
                    construction: false,
                },
            ],
            constraints: Vec::new(),
            solve_status: waffle_types::SolveStatus::FullyConstrained,
            solved_positions,
            solved_profiles: vec![waffle_types::ClosedProfile {
                entity_ids: vec![1, 2, 3, 4],
                is_outer: true,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            }],
            projected: vec![],
        },
    }
}

/// The +z face of the imported cube, as a reference scoped to `path` of `asm`.
fn cube_top_face(import_id: Uuid, scope: Option<waffle_types::RefScope>) -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: import_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Signature {
            signature: waffle_types::TopoSignature {
                surface_type: Some("planar".into()),
                normal: Some([0.0, 0.0, 1.0]),
                ..waffle_types::TopoSignature::empty()
            },
        },
        policy: ResolvePolicy::BestEffort,
        scope,
    }
}

fn open_in_context(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    features: &FeatureTree,
    tree: &AssemblyTree,
    parts: &HashMap<String, FeatureTree>,
    path: Vec<Uuid>,
) -> Result<ContextStatus, String> {
    let r = dispatch(
        state,
        UiToEngine::OpenPartInContext {
            features: features.clone(),
            assembly_tab_id: "asm".into(),
            instance_path: path,
            assembly: tree.clone(),
            part_trees: parts.clone(),
            assembly_trees: HashMap::new(),
        },
        kernel,
    );
    match r {
        EngineToUi::ModelUpdated { context, .. } => {
            Ok(context.expect("context status while a part is open in context"))
        }
        EngineToUi::Error { message, .. } => Err(message),
        other => panic!("{other:?}"),
    }
}

fn sketch_plane(state: &EngineState, sid: Uuid) -> ([f64; 3], [f64; 3]) {
    let Operation::Sketch { sketch } = &state.engine.tree.find_feature(sid).unwrap().operation
    else {
        unreachable!()
    };
    (sketch.plane_origin, sketch.plane_normal)
}

fn near3(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < tol)
}

#[test]
fn open_part_in_context_snapshots_the_other_instances_and_scoped_planes_follow_them() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let part = cube_part(&mut state, &mut kernel);
    let import_id = part.features[0].id;
    let parts: HashMap<String, FeatureTree> = HashMap::from([("part".to_string(), part.clone())]);

    let a = instance("A", "part", Transform::identity(), true);
    let b = instance("B", "part", Transform::translation([0.03, 0.0, 0.0]), true);
    let (ida, idb) = (a.id, b.id);
    let tree = AssemblyTree {
        instances: vec![a, b],
        ..Default::default()
    };

    // Open B in context: the live tree is B's part, the view holds A as the
    // one ghost at A relative to B, and the engine's snapshot agrees.
    let status = open_in_context(&mut state, &mut kernel, &part, &tree, &parts, vec![idb]).unwrap();
    assert_eq!(status.assembly_tab_id, "asm");
    assert_eq!(status.instance_path, vec![idb]);
    assert_eq!(status.instance_name, "B");
    assert!(status.errors.is_empty(), "{:?}", status.errors);
    assert_eq!(status.instances.len(), 1);
    assert_eq!(status.instances[0].path, vec![ida]);
    assert_eq!(status.instances[0].name, "A");
    assert_eq!(status.instances[0].part_tab_id, "part");
    assert!(status
        .placement
        .approx_eq(&Transform::translation([0.03, 0.0, 0.0]), 1e-12));
    assert!(state.assembly.is_none());
    assert_eq!(
        state.engine.tree.features.len(),
        1,
        "the live tree is the part"
    );
    let ctx = state.engine.context.as_ref().expect("engine context");
    assert_eq!(ctx.instances.len(), 1);
    assert!(ctx.instances[0]
        .relative
        .approx_eq(&Transform::translation([-0.03, 0.0, 0.0]), 1e-12));
    assert!(
        ctx.instances[0].feature_results.values().all(|r| r
            .outputs
            .iter()
            .all(|(_, b)| b.mesh.is_none() && b.edges.is_none())),
        "the snapshot carries handles, not meshes"
    );
    let cv = state.context_view.as_ref().unwrap();
    assert_eq!(cv.ghosts.len(), 1);

    // A sketch on A's top face, scoped: the engine derives the plane from the
    // context — A's face centroid (5, 5, 10) mm in A's frame, at x − 30 mm in
    // B's — replacing the stale snapshot the feature arrived with.
    let scope = waffle_types::RefScope::in_assembly("asm", vec![ida]);
    let r = dispatch(
        &mut state,
        UiToEngine::AddFeature {
            provenance: None,
            operation: sketch_on(cube_top_face(import_id, Some(scope.clone()))),
        },
        &mut kernel,
    );
    assert!(matches!(r, EngineToUi::ModelUpdated { .. }), "{r:?}");
    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);
    let sid = state.engine.tree.features[1].id;
    let (o, n) = sketch_plane(&state, sid);
    assert!(near3(o, [-0.025, 0.005, 0.01], 1e-9), "{o:?}");
    assert!(near3(n, [0.0, 0.0, 1.0], 1e-9), "{n:?}");

    // Extrude it: a second body of B, built in B's frame on the derived plane.
    let r = dispatch(
        &mut state,
        UiToEngine::AddFeature {
            provenance: None,
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    combine: None,
                    targets: Some(vec![]),
                    sketch_id: sid,
                    profile_index: 0,
                    profile_entity_ids: None,
                    depth: 0.004,
                    direction: None,
                    symmetric: false,
                    cut: false,
                    merge: false,
                    target_body: None,
                    depth_mode: DepthMode::Blind,
                    second_direction: None,
                    region: None,
                    regions: Vec::new(),
                    depth_expr: None,
                },
            },
        },
        &mut kernel,
    );
    assert!(matches!(r, EngineToUi::ModelUpdated { .. }), "{r:?}");
    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);
    let xid = state.engine.tree.features[2].id;
    let xr = &state.engine.feature_results[&xid];
    assert_eq!(xr.outputs.len(), 1);
    let top = feature_engine::rebuild::resolve_face_plane(
        &cube_top_face(xid, None),
        &state.engine.feature_results,
        kernel.as_introspect(),
    )
    .unwrap();
    assert!(
        (top.0[2] - 0.014).abs() < 1e-9,
        "extrude top at z = {}",
        top.0[2]
    );

    // The live tree is what the UI saves; the scoped reference is in it.
    let live = state.engine.tree.clone();
    let Operation::Sketch { sketch } = &live.features[1].operation else {
        unreachable!()
    };
    assert_eq!(sketch.plane.scope.as_ref(), Some(&scope));

    // Update the context after A moved (+20 mm in y): re-open B with the
    // current trees — the plane follows A, the extrude rebuilds on it.
    let mut moved = tree.clone();
    moved.instances[0].transform = Transform::translation([0.0, 0.02, 0.0]);
    let parts_now: HashMap<String, FeatureTree> =
        HashMap::from([("part".to_string(), live.clone())]);
    let status = open_in_context(
        &mut state,
        &mut kernel,
        &live,
        &moved,
        &parts_now,
        vec![idb],
    )
    .unwrap();
    assert!(status.errors.is_empty(), "{:?}", status.errors);
    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);
    let (o, _) = sketch_plane(&state, sid);
    assert!(near3(o, [-0.025, 0.025, 0.01], 1e-9), "{o:?}");
    let top = feature_engine::rebuild::resolve_face_plane(
        &cube_top_face(xid, None),
        &state.engine.feature_results,
        kernel.as_introspect(),
    )
    .unwrap();
    // The 2 mm square's centroid sits 1 mm from the plane origin along the
    // sketch basis's v axis (whose sign is the plane basis convention, not
    // this feature's concern).
    assert!(
        ((top.0[1] - 0.025).abs() - 0.001).abs() < 1e-9,
        "extrude follows: top centroid y = {}",
        top.0[1]
    );

    // Opening the part on its own (SwitchTab) drops the context: the sketch
    // keeps its last derived plane and says what it depends on, loudly.
    let live_now = state.engine.tree.clone();
    let r = dispatch(
        &mut state,
        UiToEngine::SwitchTab { features: live_now },
        &mut kernel,
    );
    let EngineToUi::ModelUpdated {
        warnings, context, ..
    } = r
    else {
        panic!("{r:?}")
    };
    assert!(context.is_none());
    assert!(state.engine.context.is_none() && state.context_view.is_none());
    assert!(
        warnings.iter().any(|w| w.contains("of assembly `asm`")
            && w.contains("open the part in that assembly's context")),
        "{warnings:?}"
    );
    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);
    let (o, _) = sketch_plane(&state, sid);
    assert!(near3(o, [-0.025, 0.025, 0.01], 1e-9), "{o:?}");

    // Back in the assembly, BOTH instances carry the new extrude (propagation
    // is by recipe: every instance of the part rebuilds from the same tree).
    let status = open(&mut state, &mut kernel, &moved, &parts_now);
    assert!(status.errors.is_empty(), "{:?}", status.errors);
    let view = state.assembly.as_ref().unwrap();
    assert_eq!(view.parts.len(), 1);
    assert_eq!(view.parts[0].1.feature_results.len(), 3);
    assert_eq!(view.leaves.len(), 2);
}

#[test]
fn open_part_in_context_refuses_what_it_cannot_edit() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let part = cube_part(&mut state, &mut kernel);
    let parts: HashMap<String, FeatureTree> = HashMap::from([("part".to_string(), part.clone())]);
    let mut a = instance("A", "part", Transform::identity(), true);
    let ida = a.id;
    let tree = AssemblyTree {
        instances: vec![a.clone()],
        ..Default::default()
    };

    // An instance that is not in the assembly.
    let err = open_in_context(
        &mut state,
        &mut kernel,
        &part,
        &tree,
        &parts,
        vec![Uuid::new_v4()],
    )
    .unwrap_err();
    assert!(
        err.contains("not a rendered part of assembly `asm`"),
        "{err}"
    );
    assert!(state.engine.context.is_none() && state.context_view.is_none());

    // A suppressed instance is not rendered either.
    a.suppressed = true;
    let hidden = AssemblyTree {
        instances: vec![a],
        ..Default::default()
    };
    let err =
        open_in_context(&mut state, &mut kernel, &part, &hidden, &parts, vec![ida]).unwrap_err();
    assert!(err.contains("not a rendered part"), "{err}");

    // A scoped reference to an instance the context does not have: a loud
    // per-feature error, the part still opens and builds.
    open_in_context(&mut state, &mut kernel, &part, &tree, &parts, vec![ida]).unwrap();
    let import_id = part.features[0].id;
    dispatch(
        &mut state,
        UiToEngine::AddFeature {
            provenance: None,
            operation: sketch_on(cube_top_face(
                import_id,
                Some(waffle_types::RefScope::in_assembly(
                    "asm",
                    vec![Uuid::new_v4()],
                )),
            )),
        },
        &mut kernel,
    );
    let sid = state.engine.tree.features[1].id;
    let (fid, msg) = state
        .engine
        .errors
        .iter()
        .find(|(id, _)| *id == sid)
        .expect("the sketch reports its missing instance");
    assert_eq!(*fid, sid);
    assert!(msg.contains("not in the open context"), "{msg}");
    // A reference to the edited instance ITSELF is refused too (it must be local).
    dispatch(
        &mut state,
        UiToEngine::AddFeature {
            provenance: None,
            operation: sketch_on(cube_top_face(
                import_id,
                Some(waffle_types::RefScope::in_assembly("asm", vec![ida])),
            )),
        },
        &mut kernel,
    );
    let sid2 = state.engine.tree.features[2].id;
    let (_, msg) = state
        .engine
        .errors
        .iter()
        .find(|(id, _)| *id == sid2)
        .expect("self-scoped reference is an error");
    assert!(msg.contains("edited instance itself"), "{msg}");
}

// ── Connector frames from real curved geometry ──────────────────────────
//
// `specs/assembly_connector_frame_resolver.md` §3: the canonical mate — a pin
// in a hole — must be authorable by picking the two CYLINDRICAL faces, which
// Phase 3's planar-only resolver refused (and then silently replaced with a
// default frame).

/// A circle sketch of radius `r` centred on the world axis at height `z`,
/// extruded `depth`. The bridge path a user's click-and-extrude takes.
fn circle_extrude(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    (z, normal): (f64, [f64; 3]),
    r: f64,
    depth: f64,
    combine: CombineMode,
) {
    dispatch(
        state,
        UiToEngine::BeginSketch {
            plane: GeomRef {
                kind: TopoKind::Face,
                anchor: Anchor::Datum {
                    datum_id: Uuid::new_v4(),
                },
                selector: Selector::Role {
                    role: waffle_types::Role::EndCapPositive,
                    index: 0,
                },
                policy: ResolvePolicy::BestEffort,
                scope: None,
            },
        },
        kernel,
    );
    let entities = vec![
        waffle_types::SketchEntity::Point {
            id: 1,
            x: 0.0,
            y: 0.0,
            construction: true,
        },
        waffle_types::SketchEntity::Circle {
            id: 2,
            center_id: 1,
            radius: r,
            construction: false,
        },
    ];
    for e in &entities {
        dispatch(
            state,
            UiToEngine::AddSketchEntity { entity: e.clone() },
            kernel,
        );
    }
    let r_sketch = dispatch(
        state,
        UiToEngine::FinishSketch {
            provenance: None,
            solved_positions: HashMap::from([(1, (0.0, 0.0))]),
            solved_profiles: vec![waffle_types::ClosedProfile {
                entity_ids: vec![2],
                is_outer: true,
                vertex_ids: vec![],
                circle: Some(waffle_types::CircleProfile {
                    center_u: 0.0,
                    center_v: 0.0,
                    radius: r,
                }),
                spline_segments: vec![],
                arc_segments: vec![],
            }],
            plane_origin: [0.0, 0.0, z],
            plane_normal: normal,
            entities,
            constraints: vec![],
            projected: vec![],
        },
        kernel,
    );
    let EngineToUi::ModelUpdated { feature_tree, .. } = r_sketch else {
        panic!("sketch")
    };
    let sketch_id = feature_tree.features.last().expect("sketch").id;
    dispatch(
        state,
        UiToEngine::AddFeature {
            provenance: None,
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id,
                    profile_index: 0,
                    profile_entity_ids: None,
                    depth,
                    depth_expr: None,
                    direction: None,
                    symmetric: false,
                    cut: matches!(combine, CombineMode::Cut),
                    merge: true,
                    target_body: None,
                    depth_mode: DepthMode::Blind,
                    second_direction: None,
                    region: None,
                    regions: vec![],
                    combine: Some(combine),
                    targets: None,
                },
            },
        },
        kernel,
    );
    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);
}

/// The `GeomRef` the viewport mints for the cylindrical face of the smallest
/// radius — a click on the bore of a washer, or on the barrel of a pin. Uses
/// the same role-based selector `body_face_entries` builds.
fn smallest_cylinder_face_ref(state: &EngineState, kernel: &KernelV2Adapter) -> (GeomRef, f64) {
    use waffle_types::kernel::KernelIntrospect as _;
    let mut best: Option<(GeomRef, f64)> = None;
    for feature in &state.engine.tree.features {
        let Some(res) = state.engine.feature_results.get(&feature.id) else {
            continue;
        };
        for (id, role) in &res.provenance.role_assignments {
            let Some(axis) = kernel.entity_axis(*id, TopoKind::Face) else {
                continue;
            };
            let Some(radius) = axis.radius else { continue };
            if best.as_ref().is_some_and(|(_, r)| *r <= radius) {
                continue;
            }
            best = Some((
                GeomRef {
                    kind: TopoKind::Face,
                    anchor: Anchor::FeatureOutput {
                        feature_id: feature.id,
                        output_key: OutputKey::Main,
                    },
                    selector: Selector::Role {
                        role: role.clone(),
                        index: 0,
                    },
                    policy: ResolvePolicy::BestEffort,
                    scope: None,
                },
                radius,
            ));
        }
    }
    best.expect("the part has a cylindrical face with a role")
}

/// A washer (Ø20 × 4 mm) with a Ø6 mm bore drilled through it, and a
/// Ø5 mm × 10 mm pin, mated Revolute by their two CYLINDRICAL faces: the pin
/// lands on the bore's axis, centred in it, from wherever it started.
///
/// The bore is sketched below the washer and extruded past it, so no cap is
/// coplanar with a washer cap (the M8 boundary) — the hole is a genuine
/// drilled through-hole whose wall is a cavity-sense cylinder.
#[test]
fn a_revolute_mate_on_two_cylindrical_faces_puts_the_pin_on_the_bore_axis() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();

    circle_extrude(
        &mut state,
        &mut kernel,
        (0.0, [0.0, 0.0, 1.0]),
        0.010,
        0.004,
        CombineMode::Add,
    );
    // The bore is sketched on the washer's top face (how the app records a
    // sketch-on-face) and cut 2 mm PAST the bottom, so only the top cap is
    // coplanar with the target — the ordinary drilled-through case.
    circle_extrude(
        &mut state,
        &mut kernel,
        (0.004, [0.0, 0.0, 1.0]),
        0.003,
        0.006,
        CombineMode::Cut,
    );
    let (bore_ref, bore_r) = smallest_cylinder_face_ref(&state, &kernel);
    assert!(
        (bore_r - 0.003).abs() < 1e-9,
        "the pick is the BORE wall, not the outer wall (r = {bore_r})"
    );
    let washer = state.engine.tree.clone();

    dispatch(&mut state, UiToEngine::NewDocument, &mut kernel);
    circle_extrude(
        &mut state,
        &mut kernel,
        (0.0, [0.0, 0.0, 1.0]),
        0.0025,
        0.010,
        CombineMode::Add,
    );
    let (pin_ref, pin_r) = smallest_cylinder_face_ref(&state, &kernel);
    assert!(
        (pin_r - 0.0025).abs() < 1e-9,
        "the pin barrel (r = {pin_r})"
    );
    let pin = state.engine.tree.clone();
    dispatch(&mut state, UiToEngine::NewDocument, &mut kernel);

    let parts: HashMap<String, FeatureTree> =
        HashMap::from([("washer".to_string(), washer), ("pin".to_string(), pin)]);

    let w = instance("W", "washer", Transform::identity(), true);
    // The pin starts translated and turned right off the axis.
    let p = instance(
        "P",
        "pin",
        Transform {
            translation_m: [0.05, 0.02, 0.03],
            rotation_quat: [0.382_683_432_365_09, 0.0, 0.0, 0.923_879_532_511_287],
        },
        false,
    );
    let (idw, idp) = (w.id, p.id);
    let cw = connector("bore", idw, Some(bore_ref.clone()), Frame::default());
    let cp = connector("barrel", idp, Some(pin_ref), Frame::default());
    let (idcw, idcp) = (cw.id, cp.id);
    let tree = AssemblyTree {
        instances: vec![w, p],
        connectors: vec![cw, cp],
        mates: vec![Mate {
            id: Uuid::new_v4(),
            name: "hinge".into(),
            kind: MateKind::Revolute { flip: false },
            connectors: [idcw, idcp],
            suppressed: false,
            extra: Map::new(),
        }],
        placements: BTreeMap::new(),
        extra: Map::new(),
    };

    let status = open(&mut state, &mut kernel, &tree, &parts);
    assert!(
        status.errors.is_empty(),
        "a cylindrical pick is no longer an error: {:?}",
        status.errors
    );

    // Both frames are reported, labelled by what they came from.
    let frame_of = |id: Uuid| {
        status
            .connectors
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("connector {id} reported"))
            .clone()
    };
    let bore = frame_of(idcw);
    let barrel = frame_of(idcp);
    assert_eq!(bore.kind.as_deref(), Some("cylindrical face"));
    assert_eq!(barrel.kind.as_deref(), Some("cylindrical face"));

    // The bore's frame: on the washer's axis, at mid-depth of the 4 mm wall.
    assert!(
        near3(bore.origin, [0.0, 0.0, 0.002], 1e-9),
        "bore frame at mid-depth on the axis, got {:?}",
        bore.origin
    );
    // Parallel to the washer's axis. The SENSE is the kernel's construction
    // direction (this bore was drilled downward, so −Z); a cylinder has no
    // preferred end, which is what the mate's `flip` is for.
    assert!(
        bore.z_axis[2].abs() > 1.0 - 1e-9 && bore.z_axis[0].abs() < 1e-9,
        "bore axis along ±Z, got {:?}",
        bore.z_axis
    );

    // What the mate is for: the pin's axis IS the bore's axis, and its
    // connector origin coincides — the pin sits centred in the hole.
    assert!(
        near3(barrel.origin, bore.origin, 1e-6),
        "the pin's frame moved onto the bore's, got {:?}",
        barrel.origin
    );
    let dot: f64 = (0..3).map(|k| barrel.z_axis[k] * bore.z_axis[k]).sum();
    assert!(
        (dot.abs() - 1.0).abs() < 1e-6,
        "the pin's axis is parallel to the bore's (dot = {dot})"
    );
}

/// The pick is judged BEFORE a connector exists (§2.4): the app asks, and a
/// pick that cannot derive a frame comes back refused with the resolver's own
/// reason instead of minting a connector that resolves to a default frame.
#[test]
fn probe_connector_ref_accepts_a_cylindrical_pick_and_refuses_what_has_no_frame() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();

    // Without an assembly open there is nothing to place a connector on.
    let r = dispatch(
        &mut state,
        UiToEngine::ProbeConnectorRef {
            instance_path: vec![Uuid::new_v4()],
            geom_ref: cube_top_face(Uuid::new_v4(), None),
        },
        &mut kernel,
    );
    assert!(matches!(r, EngineToUi::Error { .. }), "{r:?}");

    circle_extrude(
        &mut state,
        &mut kernel,
        (0.0, [0.0, 0.0, 1.0]),
        0.010,
        0.004,
        CombineMode::Add,
    );
    let (wall_ref, _) = smallest_cylinder_face_ref(&state, &kernel);
    let part = state.engine.tree.clone();
    dispatch(&mut state, UiToEngine::NewDocument, &mut kernel);

    let parts: HashMap<String, FeatureTree> = HashMap::from([("part".to_string(), part)]);
    let a = instance("A", "part", Transform::identity(), true);
    let id = a.id;
    let tree = AssemblyTree {
        instances: vec![a],
        connectors: Vec::new(),
        mates: Vec::new(),
        placements: BTreeMap::new(),
        extra: Map::new(),
    };
    open(&mut state, &mut kernel, &tree, &parts);

    let probe = |state: &mut EngineState, kernel: &mut KernelV2Adapter, geom_ref: GeomRef| {
        let r = dispatch(
            state,
            UiToEngine::ProbeConnectorRef {
                instance_path: vec![id],
                geom_ref,
            },
            kernel,
        );
        match r {
            EngineToUi::ConnectorRefProbed { ok, kind, reason } => (ok, kind, reason),
            other => panic!("{other:?}"),
        }
    };

    let (ok, kind, reason) = probe(&mut state, &mut kernel, wall_ref);
    assert!(ok, "a cylindrical face carries a connector: {reason:?}");
    assert_eq!(kind.as_deref(), Some("cylindrical face"));

    // A reference into a feature this part does not have resolves to nothing.
    let (ok, _, reason) = probe(&mut state, &mut kernel, cube_top_face(Uuid::new_v4(), None));
    assert!(!ok, "an unresolvable pick is refused");
    assert!(reason.is_some(), "with a reason the panel can show");
}

/// A connector's adjustments (`specs/assembly_connector_adjustments.md`) on
/// real geometry — the washer's bore anchored at an end, flipped, and
/// offset, and an explicit-frame connector turned about z. The bore spans
/// z = 0 … 4 mm; "+z end" is the end the REPORTED z points toward, whichever
/// sense the kernel drilled the hole in.
#[test]
fn connector_adjustments_move_the_frame_in_its_own_axes() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();

    circle_extrude(
        &mut state,
        &mut kernel,
        (0.0, [0.0, 0.0, 1.0]),
        0.010,
        0.004,
        CombineMode::Add,
    );
    circle_extrude(
        &mut state,
        &mut kernel,
        (0.004, [0.0, 0.0, 1.0]),
        0.003,
        0.006,
        CombineMode::Cut,
    );
    let (bore_ref, _) = smallest_cylinder_face_ref(&state, &kernel);
    let washer = state.engine.tree.clone();
    dispatch(&mut state, UiToEngine::NewDocument, &mut kernel);
    let parts: HashMap<String, FeatureTree> = HashMap::from([("washer".to_string(), washer)]);

    let w = instance("W", "washer", Transform::identity(), true);
    let idw = w.id;
    let mut end = connector("end", idw, Some(bore_ref.clone()), Frame::default());
    end.anchor = AxialAnchor::PositiveEnd;
    let mut flipped = connector("flipped", idw, Some(bore_ref.clone()), Frame::default());
    flipped.anchor = AxialAnchor::PositiveEnd;
    flipped.flip_z = true;
    let mut shifted = connector("shifted", idw, Some(bore_ref), Frame::default());
    shifted.offset_m = [0.0, 0.0, 0.001];
    let mut turned = connector(
        "turned",
        idw,
        None,
        Frame::on_plane([0.0, 0.0, 0.004], [0.0, 0.0, 1.0]),
    );
    turned.rotation_deg = 90.0;
    turned.offset_m = [0.001, 0.0, 0.0];
    let (id_end, id_flipped, id_shifted, id_turned) = (end.id, flipped.id, shifted.id, turned.id);
    let tree = AssemblyTree {
        instances: vec![w],
        connectors: vec![end, flipped, shifted, turned],
        mates: Vec::new(),
        placements: BTreeMap::new(),
        extra: Map::new(),
    };

    let status = open(&mut state, &mut kernel, &tree, &parts);
    assert!(status.errors.is_empty(), "{:?}", status.errors);
    let frame_of = |id: Uuid| {
        status
            .connectors
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("connector {id} reported"))
            .clone()
    };

    let e = frame_of(id_end);
    assert_eq!(e.kind.as_deref(), Some("cylindrical face"));
    let rim_z = if e.z_axis[2] > 0.0 { 0.004 } else { 0.0 };
    assert!(
        near3(e.origin, [0.0, 0.0, rim_z], 1e-9),
        "the rim +z points toward (z = {rim_z}), got {:?} with z {:?}",
        e.origin,
        e.z_axis
    );

    // Flipping reverses the reported z, and "+z end" follows it to the
    // other rim.
    let f = frame_of(id_flipped);
    assert!(
        (f.z_axis[2] + e.z_axis[2]).abs() < 1e-9,
        "z reversed: {:?} vs {:?}",
        f.z_axis,
        e.z_axis
    );
    assert!(
        near3(f.origin, [0.0, 0.0, 0.004 - rim_z], 1e-9),
        "the other rim, got {:?}",
        f.origin
    );

    // 1 mm along the connector's own z from mid-depth, the way z points.
    let s = frame_of(id_shifted);
    assert!(
        near3(s.origin, [0.0, 0.0, 0.002 + 0.001 * s.z_axis[2]], 1e-9),
        "mid-depth + 1 mm along z, got {:?} with z {:?}",
        s.origin,
        s.z_axis
    );

    // An explicit frame: x turned onto +Y, then 1 mm along that x.
    let t = frame_of(id_turned);
    assert!(
        t.kind.is_none(),
        "an explicit frame is derived from nothing"
    );
    assert!(
        near3(t.x_axis, [0.0, 1.0, 0.0], 1e-9),
        "x turned, got {:?}",
        t.x_axis
    );
    assert!(
        near3(t.origin, [0.0, 0.001, 0.004], 1e-9),
        "offset along the turned x, got {:?}",
        t.origin
    );
}

// ── part mate connectors (specs/part_mate_connectors.md) ────────────────────

/// The imported cube's +z face, by signature.
fn cube_face(import_id: Uuid, kind: TopoKind, normal: [f64; 3]) -> GeomRef {
    GeomRef {
        kind,
        anchor: Anchor::FeatureOutput {
            feature_id: import_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Signature {
            signature: waffle_types::TopoSignature {
                surface_type: Some("planar".into()),
                normal: Some(normal),
                ..waffle_types::TopoSignature::empty()
            },
        },
        policy: ResolvePolicy::BestEffort,
        scope: None,
    }
}

/// A named connector authored in the PART is reported by the part's
/// `ModelUpdated`, carried into every instance of the part, and usable by an
/// assembly connector (`part_connector`) in a mate.
#[test]
fn a_part_mate_connector_is_evaluated_in_the_part_and_mates_its_instances() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let part = cube_part(&mut state, &mut kernel);
    let import_id = part.features[0].id;

    let r = dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: Operation::MateConnector {
                params: MateConnectorParams {
                    name: "Top".into(),
                    geom_ref: Some(cube_face(import_id, TopoKind::Face, [0.0, 0.0, 1.0])),
                    ..Default::default()
                },
            },
            provenance: None,
        },
        &mut kernel,
    );
    let EngineToUi::ModelUpdated {
        feature_id,
        feature_tree,
        errors,
        connectors,
        ..
    } = r
    else {
        panic!("{r:?}")
    };
    assert!(errors.is_empty(), "{errors:?}");
    let top_id = feature_id.expect("the new feature's id");
    let added = feature_tree.find_feature(top_id).expect("in the tree");
    assert_eq!(added.name, "Top", "created with its params name");
    assert_eq!(connectors.len(), 1, "{connectors:?}");
    let c = &connectors[0];
    assert_eq!((c.feature_id, c.name.as_str()), (top_id, "Top"));
    assert_eq!(c.kind.as_deref(), Some("planar face"));
    assert!((c.origin[2] - 0.01).abs() < 1e-6, "on the top face: {c:?}");
    assert!((c.z_axis[2] - 1.0).abs() < 1e-9, "{c:?}");
    assert!(state
        .engine
        .tree
        .features
        .iter()
        .all(|f| f.id != Uuid::nil()));

    let parts: HashMap<String, FeatureTree> =
        HashMap::from([("part".to_string(), state.engine.tree.clone())]);
    let a = instance("A", "part", Transform::identity(), true);
    let b = instance("B", "part", Transform::identity(), false);
    let (ida, idb) = (a.id, b.id);
    let mut ca = connector("A › Top", ida, None, Frame::default());
    ca.part_connector = Some(top_id);
    let cb = connector(
        "B bottom",
        idb,
        None,
        Frame::on_plane([0.005, 0.005, 0.0], [0.0, 0.0, -1.0]),
    );
    let mut dangling = connector("gone", idb, None, Frame::default());
    dangling.part_connector = Some(Uuid::new_v4());
    let (cida, cidb) = (ca.id, cb.id);
    let tree = AssemblyTree {
        instances: vec![a, b],
        connectors: vec![ca, cb, dangling],
        mates: vec![fastened(cida, cidb, true)],
        ..Default::default()
    };
    let status = open(&mut state, &mut kernel, &tree, &parts);

    // B stacked on A's part connector.
    let tb = status.placements[&idb];
    assert!((tb.translation_m[2] - 0.01).abs() < 1e-6, "{tb:?}");
    let frame = status.connectors.iter().find(|f| f.id == cida).unwrap();
    assert_eq!(frame.kind.as_deref(), Some("part connector · planar face"));
    assert!((frame.origin[2] - 0.01).abs() < 1e-6, "{frame:?}");

    // Every instance of the part offers the connector, placed in the world.
    assert_eq!(
        status.part_connectors.len(),
        2,
        "{:?}",
        status.part_connectors
    );
    let on_b = status
        .part_connectors
        .iter()
        .find(|p| p.instance_path == vec![idb])
        .expect("B's copy");
    assert_eq!((on_b.feature_id, on_b.name.as_str()), (top_id, "Top"));
    assert!(
        (on_b.origin[2] - 0.02).abs() < 1e-6,
        "B's top face: {on_b:?}"
    );

    // A reference to a connector the part does not have is loud.
    assert!(
        status
            .errors
            .iter()
            .any(|e| e.contains("`gone`") && e.contains("no working mate connector")),
        "{:?}",
        status.errors
    );
    let _ = ida;
}

/// A pick with no frame fails the feature loudly and reports no connector.
#[test]
fn a_part_mate_connector_on_a_vertex_fails_its_feature() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let part = cube_part(&mut state, &mut kernel);
    let import_id = part.features[0].id;
    let r = dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: Operation::MateConnector {
                params: MateConnectorParams {
                    geom_ref: Some(cube_face(import_id, TopoKind::Vertex, [0.0, 0.0, 1.0])),
                    ..Default::default()
                },
            },
            provenance: None,
        },
        &mut kernel,
    );
    let EngineToUi::ModelUpdated {
        feature_id,
        feature_tree,
        errors,
        connectors,
        ..
    } = r
    else {
        panic!("{r:?}")
    };
    let id = feature_id.expect("added even though it fails");
    assert_eq!(
        feature_tree.find_feature(id).unwrap().name,
        "Mate connector",
        "the default name"
    );
    assert!(errors.iter().any(|(fid, _)| *fid == id), "{errors:?}");
    assert!(connectors.is_empty(), "{connectors:?}");
}
