//! Render-view material & appearance projection (`specs/studio_pbr_renderer.md` §2.3, §5.1).
//!
//! Verifies that `render_view::body_metadata` omits `material` and `appearance`
//! when a body has no material assigned (preserving byte parity with existing
//! render_view fixtures), and projects authoritative `material` + `appearance`
//! from the owning `FeatureTree` in both Part tabs and Assembly leaf instances.

use feature_engine::assembly::{AssemblyTree, Instance, PartRef, Transform};
use feature_engine::types::{Appearance, Material};
use kernel_v2::KernelV2Adapter;
use serde_json::{json, Map};
use uuid::Uuid;
use wasm_bridge::messages::{MaterialEdit, UiToEngine};
use wasm_bridge::{process, render_view, EngineState};

const CUBE_STEP: &str = include_str!("../../step-import/tests/fixtures/cube.step");

fn send(state: &mut EngineState, kernel: &mut KernelV2Adapter, msg: UiToEngine) {
    process::process_message(state, kernel, msg, &|| 0.0, &|_| {});
}

fn import_cube(state: &mut EngineState, kernel: &mut KernelV2Adapter) -> String {
    send(
        state,
        kernel,
        UiToEngine::ImportStep {
            file_name: "cube.step".into(),
            data: CUBE_STEP.to_string(),
        },
    );
    let meta = render_view::body_metadata(state);
    assert_eq!(meta.len(), 1, "expected one imported cube body");
    meta[0]["bodyId"]
        .as_str()
        .expect("bodyId string")
        .to_string()
}

#[test]
fn unassigned_body_omits_material_and_appearance_keys() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    import_cube(&mut state, &mut kernel);

    let meta = render_view::body_metadata(&state);
    let obj = meta[0].as_object().expect("metadata object");
    assert!(
        !obj.contains_key("material"),
        "unassigned body must omit `material`: {obj:?}"
    );
    assert!(
        !obj.contains_key("appearance"),
        "unassigned body must omit `appearance`: {obj:?}"
    );
}

#[test]
fn part_body_projects_assigned_material_and_appearance_and_clears_on_unassign() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body_id = import_cube(&mut state, &mut kernel);

    send(
        &mut state,
        &mut kernel,
        UiToEngine::EditMaterials {
            edit: MaterialEdit::Upsert {
                material: Material {
                    name: "Brass C360".into(),
                    density_kg_m3: 8500.0,
                    appearance: Some(Appearance {
                        color: [0.91, 0.78, 0.45],
                        metalness: Some(0.95),
                        roughness: Some(0.22),
                    }),
                },
            },
        },
    );
    send(
        &mut state,
        &mut kernel,
        UiToEngine::EditMaterials {
            edit: MaterialEdit::AssignBody {
                body_id: body_id.clone(),
                material: Some("Brass C360".into()),
            },
        },
    );

    let meta = render_view::body_metadata(&state);
    assert_eq!(meta[0]["material"], json!("Brass C360"));
    assert_eq!(
        meta[0]["appearance"],
        json!({
            "color": [0.91, 0.78, 0.45],
            "metalness": 0.95,
            "roughness": 0.22,
        })
    );

    // Unassigning restores exact omission.
    send(
        &mut state,
        &mut kernel,
        UiToEngine::EditMaterials {
            edit: MaterialEdit::AssignBody {
                body_id,
                material: None,
            },
        },
    );
    let cleared = render_view::body_metadata(&state);
    let obj = cleared[0].as_object().expect("metadata object");
    assert!(!obj.contains_key("material"));
    assert!(!obj.contains_key("appearance"));
}

#[test]
fn assembly_instances_project_their_leaf_part_material_appearance() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body_id = import_cube(&mut state, &mut kernel);
    let part_tab_id = state.session.tabs()[0].id.clone();

    send(
        &mut state,
        &mut kernel,
        UiToEngine::EditMaterials {
            edit: MaterialEdit::Upsert {
                material: Material {
                    name: "Anodized Red".into(),
                    density_kg_m3: 2700.0,
                    appearance: Some(Appearance {
                        color: [0.78, 0.14, 0.16],
                        metalness: Some(0.82),
                        roughness: Some(0.26),
                    }),
                },
            },
        },
    );
    send(
        &mut state,
        &mut kernel,
        UiToEngine::EditMaterials {
            edit: MaterialEdit::AssignBody {
                body_id,
                material: Some("Anodized Red".into()),
            },
        },
    );

    let asm = AssemblyTree {
        instances: vec![
            Instance {
                id: Uuid::new_v4(),
                name: "Housing A".into(),
                source: PartRef {
                    source_id: None,
                    tab_id: part_tab_id.clone(),
                },
                transform: Transform::identity(),
                fixed: true,
                suppressed: false,
                external_key: None,
                parameter_overrides: None,
                extra: Map::new(),
            },
            Instance {
                id: Uuid::new_v4(),
                name: "Housing B".into(),
                source: PartRef {
                    source_id: None,
                    tab_id: part_tab_id,
                },
                transform: Transform::translation([0.03, 0.0, 0.0]),
                fixed: false,
                suppressed: false,
                external_key: None,
                parameter_overrides: None,
                extra: Map::new(),
            },
        ],
        ..Default::default()
    };

    let asm_tab_id = state
        .session
        .add_tab("Assembly", None)
        .expect("create assembly tab");
    state
        .session
        .set_assembly(&asm_tab_id, asm)
        .expect("populate assembly");
    send(
        &mut state,
        &mut kernel,
        UiToEngine::OpenAssembly { tab_id: asm_tab_id },
    );

    let meta = render_view::body_metadata(&state);
    assert_eq!(meta.len(), 2, "two assembly instances rendered");
    for (idx, entry) in meta.iter().enumerate() {
        assert_eq!(
            entry["material"],
            json!("Anodized Red"),
            "instance {idx} material"
        );
        assert_eq!(
            entry["appearance"]["color"],
            json!([0.78, 0.14, 0.16]),
            "instance {idx} appearance.color"
        );
        assert_eq!(entry["appearance"]["metalness"], json!(0.82));
        assert_eq!(entry["appearance"]["roughness"], json!(0.26));
    }
}
