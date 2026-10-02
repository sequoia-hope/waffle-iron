//! SI5 checkpoint **C6** — the two-tier STEP import on the REAL kernel
//! (`specs/step_import_si5_exact_analytic_ingestion.md` §7;
//! `feature_engine::import_tiers`).
//!
//! The mock-kernel tests in `feature-engine/tests/imported_body.rs` prove
//! the feature-level plumbing; these prove the thing C6 exists for: an
//! in-vocabulary file imported through the app's own feature path is a
//! first-class kernel-v2 solid (exact volume, analytic export), a file the
//! extractor refuses is served by the mesh tier with the reason on the
//! feature, and a file the extractor ACCEPTS but the kernel refuses falls
//! back the same way — loudly, from the same canonical shell list.

use feature_engine::types::*;
use feature_engine::Engine;
use modeling_ops::KernelBundle;
use std::path::PathBuf;
use uuid::Uuid;
use waffle_types::kernel::KernelError;
use waffle_types::OutputKey;

fn fixture(rel: &str) -> String {
    let path = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../step-import/tests/fixtures/"
    ))
    .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn new_kernel() -> Box<dyn KernelBundle> {
    Box::new(kernel_v2::adapter::KernelV2Adapter::new())
}

fn import(engine: &mut Engine, kb: &mut dyn KernelBundle, rel: &str, scale: f64) -> Uuid {
    let text = fixture(rel);
    let mut params = ImportedBodyParams::embedded(rel, &text);
    params.scale = scale;
    engine
        .add_feature(
            format!("Import {rel}"),
            Operation::ImportedBody { params },
            kb,
        )
        .expect("the import feature adds")
}

fn fallback_warnings(engine: &Engine) -> Vec<&String> {
    engine
        .warnings
        .iter()
        .filter(|w| w.contains("served from the mesh tier"))
        .collect()
}

/// The exact tier, end to end: the rounded puck (a fillet — the C5a torus
/// band beside a cylinder band and two discs) imported through the feature
/// path is an arena solid. Its volume integrates exactly, its STEP export
/// is analytic (the fillet written as a `TOROIDAL_SURFACE`), and nothing
/// warned about a fallback.
#[test]
fn an_in_vocabulary_file_is_a_first_class_kernel_solid_through_the_feature_path() {
    let mut kb = new_kernel();
    let mut engine = Engine::new();
    let id = import(&mut engine, kb.as_mut(), "analytic/rounded_puck.step", 1.0);
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert!(
        fallback_warnings(&engine).is_empty(),
        "{:?}",
        engine.warnings
    );

    let result = &engine.feature_results[&id];
    assert_eq!(result.outputs.len(), 1);
    assert_eq!(result.outputs[0].0, OutputKey::Main);
    let handle = &result.outputs[0].1.handle;
    assert!(kb.solid_is_exact(handle));

    let volume = kb.solid_volume(handle).expect("an exact solid integrates");
    assert!(volume > 0.0, "{volume}");
    // The fixture is written in millimetres from metre geometry of
    // centimetre scale: a sanity bound, not an oracle (the ingest tests pin
    // the closed form against the solid that wrote it).
    assert!(volume < 1e-3, "{volume} m³ is not a part a few cm across");

    let step = kb
        .export_step(handle, "puck-out.step")
        .expect("an exact solid exports analytically");
    assert!(
        step.contains("TOROIDAL_SURFACE"),
        "the fillet is written exactly"
    );
    assert!(step.contains("CYLINDRICAL_SURFACE"));
}

/// The mesh tier, end to end: truck's cylinder fixture writes its rims as
/// NURBS, so the extractor refuses the shell by name; the feature still
/// builds (one composite mesh body), the warning says which shell and why,
/// and the body is NOT exact — no volume, no export.
#[test]
fn an_out_of_vocabulary_file_is_served_by_the_mesh_tier_and_says_so() {
    let mut kb = new_kernel();
    let mut engine = Engine::new();
    let id = import(&mut engine, kb.as_mut(), "cylinder.step", 1.0);
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);

    let warnings = fallback_warnings(&engine);
    assert_eq!(warnings.len(), 1, "{:?}", engine.warnings);
    assert!(
        warnings[0].contains("shell 0") && warnings[0].contains("no exact representation"),
        "{}",
        warnings[0]
    );

    let result = &engine.feature_results[&id];
    assert_eq!(result.outputs.len(), 1);
    let handle = &result.outputs[0].1.handle;
    assert!(!kb.solid_is_exact(handle));
    assert!(matches!(
        kb.solid_volume(handle),
        Err(KernelError::NotSupported { .. })
    ));
    assert!(matches!(
        kb.export_step(handle, "cyl-out.step"),
        Err(KernelError::NotSupported { .. })
    ));
    // It still renders, like every imported body since SI1.
    assert!(kb.tessellate(handle, 1e-3).is_ok());
}

/// The second fallback: a shell the EXTRACTOR accepts but the KERNEL
/// refuses. The closed sphere fixture is every surface in vocabulary, yet
/// our own exporter writes it as a seam slit — a loop walking one edge
/// twice — which `ingest_analytic` names as a C5c refusal. The feature falls
/// back to the mesh tier for that shell, from the whole-file mesh parse, and
/// the warning carries the kernel's own finding.
#[test]
fn a_shell_the_kernel_refuses_falls_back_to_the_mesh_tier_with_the_kernels_reason() {
    let mut kb = new_kernel();
    let mut engine = Engine::new();

    // The control: the extractor does accept the shell.
    let text = fixture("analytic/sphere.step");
    let tiered = step_import::parse_step_tiered(&text, "sphere").expect("parses");
    assert_eq!(tiered.exact_count(), 1, "{:?}", tiered.rejections());

    let id = import(&mut engine, kb.as_mut(), "analytic/sphere.step", 1.0);
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    let warnings = fallback_warnings(&engine);
    assert_eq!(warnings.len(), 1, "{:?}", engine.warnings);
    assert!(
        warnings[0].contains("shell 0") && warnings[0].contains("exact ingestion refused"),
        "{}",
        warnings[0]
    );

    let result = &engine.feature_results[&id];
    assert_eq!(result.outputs.len(), 1, "one composite mesh body");
    assert!(!kb.solid_is_exact(&result.outputs[0].1.handle));
    assert!(kb.tessellate(&result.outputs[0].1.handle, 1e-3).is_ok());
}

/// A placement edit replays the import feature (the cached tiered parse)
/// and the EXACT body moves with it: the feature's scale and placement are
/// applied to the analytic parameters, not to a mesh.
#[test]
fn editing_the_placement_moves_the_exact_body() {
    let mut kb = new_kernel();
    let mut engine = Engine::new();
    let id = import(&mut engine, kb.as_mut(), "analytic/block.step", 1.0);
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    let before = kb
        .solid_aabb(&engine.feature_results[&id].outputs[0].1.handle)
        .expect("aabb");
    let v_before = kb
        .solid_volume(&engine.feature_results[&id].outputs[0].1.handle)
        .expect("exact volume");

    let text = fixture("analytic/block.step");
    let mut params = ImportedBodyParams::embedded("analytic/block.step", &text);
    params.scale = 2.0;
    params.translation_m = [0.5, 0.0, 0.0];
    engine
        .edit_feature(id, Operation::ImportedBody { params }, kb.as_mut())
        .expect("edits");
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert!(
        fallback_warnings(&engine).is_empty(),
        "{:?}",
        engine.warnings
    );

    let handle = &engine.feature_results[&id].outputs[0].1.handle;
    let after = kb.solid_aabb(handle).expect("aabb");
    let v_after = kb.solid_volume(handle).expect("exact volume");
    assert!(
        (after.0[0] - (0.5 + 2.0 * before.0[0])).abs() < 1e-12,
        "min x: {:?} → {:?}",
        before,
        after
    );
    assert!(
        ((v_after - 8.0 * v_before) / (8.0 * v_before)).abs() < 1e-12,
        "a 2× scale is 8× the volume: {v_before} → {v_after}"
    );
}
