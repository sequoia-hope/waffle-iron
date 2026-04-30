//! PR7 — empirical mechanism classifier for R0033's non-bijective face pairs.
//!
//! Loads R0033.waffle through the same dispatch path the assay runner uses,
//! runs the bijective oracle, and for each non-bijective face pair invokes
//! `kernel::tessellation::pr7_classify::classify_pr7_pair` to classify the
//! mechanism into one of {ArenaMissingEdge, PoolNotShared, PositionalDrift,
//! DirectionReciprocity, Other}.
//!
//! Test PASSES on classification (not on fix). Phase A's deliverable is the
//! emitted classification, which anchors PR7's Phase B fix selection per the
//! stop condition in the PR7 brief.
//!
//! On `main` the classification is the empirical answer for R0033's two nb
//! pairs (`face_a=2 ↔ face_b=3, edge=6` and `face_a=2 ↔ face_b=5, edge=7`).
//! The classifier walks arena → discretization pool → rendermesh emission
//! → directed-edge reciprocity in that order, returning at the FIRST class
//! whose contract is violated.
//!
//! Refs: PR4 RED test `pr4_r0033_t_junction_diagnosis.rs`. Spec PR7 brief.
//! Spec lineage `specs/tessellation_bounded_residuals.md` §§1-10.

use std::collections::BTreeMap;
use std::path::Path;

use kernel::geometry::curve::CurveGeom;
use kernel::tessellation::bijective::check_face_pair_bijective;
use kernel::tessellation::pr7_classify::{classify_pr7_pair, Pr7Classification};
use kernel::topology::arena::TopoArena;
use kernel::topology::half_edge::{EdgeIdx, FaceIdx};
use kernel::{Kernel, KernelSolidHandle, RenderMesh, WaffleKernel};
use wasm_bridge::messages::UiToEngine;
use wasm_bridge::{dispatch, EngineState};

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

#[derive(serde::Deserialize)]
struct R0033Meta {
    scale: f64,
}

fn last_solid_handle(state: &EngineState) -> Option<KernelSolidHandle> {
    let tree = &state.engine.tree;
    let limit = tree.active_index.unwrap_or(tree.features.len());
    for feature in tree.features[..limit].iter().rev() {
        if feature.suppressed {
            continue;
        }
        if let Some(result) = state.engine.get_result(feature.id) {
            if !result.outputs.is_empty() {
                return Some(result.outputs[0].1.handle.clone());
            }
        }
    }
    None
}

/// Load R0033, tessellate, return rendermesh + arena + face_map + edge
/// geometry. Mirrors `pr4_r0033_t_junction_diagnosis::run_diagnosis` but
/// also returns the edge geometry the classifier needs.
fn load_r0033() -> (
    RenderMesh,
    TopoArena,
    BTreeMap<u64, FaceIdx>,
    BTreeMap<EdgeIdx, CurveGeom>,
) {
    let waffle_path = Path::new(ASSAY_DIR).join("R0033.waffle");
    let meta_path = Path::new(ASSAY_DIR).join("R0033.meta.json");
    assert!(waffle_path.exists(), "R0033.waffle missing");

    let waffle_json = std::fs::read_to_string(&waffle_path).expect("read R0033.waffle");
    let meta_json = std::fs::read_to_string(&meta_path).expect("read R0033.meta.json");
    let meta: R0033Meta = serde_json::from_str(&meta_json).expect("parse R0033.meta.json");

    std::env::set_var("YANG_BOOLEAN", "1");
    let tess_tol = (meta.scale * 0.01).clamp(1e-9, 0.1);
    eprintln!(
        "R0033 scale = {:.6e}, tess_tol = {:.6e}",
        meta.scale, tess_tol
    );

    let mut state = EngineState::new();
    let mut kernel = WaffleKernel::new();

    let response = dispatch(
        &mut state,
        UiToEngine::LoadProject { data: waffle_json },
        &mut kernel,
    );
    eprintln!(
        "LoadProject response variant: {:?}",
        std::mem::discriminant(&response)
    );

    let engine_errors = state.engine.errors.clone();
    assert!(
        engine_errors.is_empty(),
        "R0033 must load without engine errors; got {:?}",
        engine_errors
    );

    let handle = last_solid_handle(&state).expect("R0033 should have a solid feature");
    let mesh = kernel
        .tessellate(&handle, tess_tol)
        .expect("tessellate R0033 last solid");

    let (arena, face_map) = kernel
        .brep_diagnostic_view(&handle)
        .expect("brep_diagnostic_view");
    let edge_geometry = kernel
        .edge_geometry_for(&handle)
        .expect("edge_geometry_for");

    eprintln!(
        "B-Rep arena: {} verts, {} half_edges, {} edges, {} loops, {} faces",
        arena.vertices.len(),
        arena.half_edges.len(),
        arena.edges.len(),
        arena.loops.len(),
        arena.faces.len()
    );
    eprintln!(
        "rendermesh: {} verts, {} indices, {} face_ranges",
        mesh.vertices.len() / 3,
        mesh.indices.len(),
        mesh.face_ranges.len(),
    );

    (mesh, arena.clone(), face_map.clone(), edge_geometry.clone())
}

fn classification_label(c: &Pr7Classification) -> &'static str {
    match c {
        Pr7Classification::ArenaMissingEdge { .. } => "arena-missing-edge",
        Pr7Classification::PoolNotShared { .. } => "pool-not-shared",
        Pr7Classification::PositionalDrift { .. } => "positional-drift",
        Pr7Classification::DirectionReciprocity { .. } => "direction-reciprocity",
        Pr7Classification::Other { .. } => "other",
    }
}

#[test]
fn classify_r0033_mechanism() {
    let (mesh, arena, face_map, edge_geometry) = load_r0033();
    let report = check_face_pair_bijective(&mesh, &face_map, &arena);
    eprintln!(
        "bijective oracle: total_pairs_examined = {}, bijective_pairs = {}, non_bijective_pairs = {}",
        report.total_pairs_examined,
        report.bijective_pairs,
        report.non_bijective_pairs.len()
    );

    assert!(
        !report.non_bijective_pairs.is_empty(),
        "R0033 expected to be RED (>=1 nb pair); none found means an upstream change has shipped \
         and PR7's anchor is stale"
    );

    let mut classifications: Vec<(usize, FaceIdx, FaceIdx, Pr7Classification)> = Vec::new();
    for (i, nb) in report.non_bijective_pairs.iter().enumerate() {
        eprintln!();
        eprintln!(
            "─── classifying nb pair #{}: face_a=FaceIdx({}), face_b=FaceIdx({}), edge={:?} ───",
            i, nb.face_a.0, nb.face_b.0, nb.edge
        );
        let cls = classify_pr7_pair(
            &mesh,
            &arena,
            &face_map,
            &edge_geometry,
            nb.face_a,
            nb.face_b,
            nb.edge,
        );
        eprintln!("classification: {} → {:?}", classification_label(&cls), cls);
        classifications.push((i, nb.face_a, nb.face_b, cls));
    }

    eprintln!();
    eprintln!("═══ PR7 mechanism classification summary for R0033 ═══");
    for (i, fa, fb, cls) in &classifications {
        eprintln!(
            "  pair #{}: faces ({}, {}) → {}",
            i,
            fa.0,
            fb.0,
            classification_label(cls)
        );
    }

    // Phase A passes if every nb pair classified into ONE of the 5 known
    // categories (4 contract violations + Other for the 5th-class escape
    // hatch). The test ASSERTS classification — that's the deliverable.
    for (i, _, _, cls) in &classifications {
        match cls {
            Pr7Classification::ArenaMissingEdge { .. }
            | Pr7Classification::PoolNotShared { .. }
            | Pr7Classification::PositionalDrift { .. }
            | Pr7Classification::DirectionReciprocity { .. }
            | Pr7Classification::Other { .. } => {
                eprintln!("pair #{} classified as {}", i, classification_label(cls));
            }
        }
    }

    // Sanity: the five-class enum is exhaustive by construction; if any
    // future variant is added without test update, this will fail on the
    // match. No further assertion needed for Phase A.
}
