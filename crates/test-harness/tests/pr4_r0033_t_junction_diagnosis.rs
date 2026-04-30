//! PR4 — empirical R0033 T-junction diagnostic.
//!
//! PR3's pivot (`specs/tessellation_bounded_residuals.md`) falsified the
//! `discretize_edges` dedup hypothesis and revised the residual non-
//! bijectivity diagnosis to **B-Rep T-junctions** introduced at boolean
//! assembly time: face A's outer-loop walks N edges along a shared
//! boundary while face B's loop walks N−1 edges (one face has an extra
//! mid-edge vertex the other doesn't have).
//!
//! PR3's corpus dump (`specs/tessellation_pr3_corpus_dump.md`) ranked
//! `R0033` as the smallest linear-bounded case with multiple non-bij
//! pairs: 12 face pairs / 2 nb / 16.7%. This test is the empirical
//! anchor — it loads `R0033.waffle` via the assay-runner's full
//! LoadProject path, tessellates with the same scale-adaptive tolerance
//! the runner uses, runs the bijective oracle, and walks the B-Rep
//! outer-loop of each non-bijective face pair to identify the actual
//! T-junction vertex.
//!
//! On `main` this test is RED — the final assertion fires because R0033
//! has 2 non-bijective pairs. PR5 will be GREEN once the B-Rep assembly
//! fix lands. The test exists to satisfy FIP §2 (red-before-green) for
//! PR5; the diagnostic stderr is captured into
//! `specs/tessellation_bounded_residuals.md` so PR5's implementer
//! doesn't have to re-run.
//!
//! Refs:
//! - Yang 2025 §4.1.1 — bijective contract
//! - Cherchi port audit D-10 — `weld_mesh_vertices` (Cluster I, blocked
//!   by upstream tessellation)
//! - PR1 oracle: `5f5423c`. PR2 fix: `f01dd68`. PR3 pivot: `8ad64b5`.
//!   PR3 corpus dump: `720fa8d`.

use std::collections::BTreeMap;
use std::path::Path;

use kernel::tessellation::bijective::{check_face_pair_bijective, NonBijectivePair};
use kernel::topology::arena::TopoArena;
use kernel::topology::half_edge::{FaceIdx, HalfEdgeIdx, LoopIdx};
use kernel::{Kernel, KernelSolidHandle, RenderMesh, WaffleKernel};
use wasm_bridge::messages::UiToEngine;
use wasm_bridge::{dispatch, EngineState};

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// Metadata fields we read straight off the .meta.json — only `scale` is
/// load-bearing for the diagnostic (it sets the tolerance the assay
/// runner used when measuring R0033's nb count).
#[derive(serde::Deserialize)]
struct R0033Meta {
    scale: f64,
}

/// Walk a B-Rep loop's half-edge chain and collect the position of every
/// vertex at the start of each half-edge (i.e., the loop's CCW vertex
/// sequence as seen by tessellation).
fn collect_loop_vertices(arena: &TopoArena, loop_idx: LoopIdx) -> Vec<(usize, [f64; 3])> {
    let mut out = Vec::new();
    if loop_idx.0 >= arena.loops.len() {
        return out;
    }
    let start: HalfEdgeIdx = arena.loops[loop_idx.0].half_edge;
    let mut he = start;
    // Defensive cap so a malformed arena can't loop forever in test output.
    for _ in 0..100_000 {
        let he_data = &arena.half_edges[he.0];
        let v_idx = he_data.origin.0;
        let pos = arena.vertices[v_idx].position;
        out.push((v_idx, pos));
        he = he_data.next;
        if he == start {
            break;
        }
    }
    out
}

fn fmt_pt(p: [f64; 3]) -> String {
    format!("({:.6e}, {:.6e}, {:.6e})", p[0], p[1], p[2])
}

/// Set difference (a − b) on f64 bit patterns — the same equality the
/// bijective oracle uses (`bijective.rs::pos_key`).
fn vertex_set_diff(a: &[(usize, [f64; 3])], b: &[(usize, [f64; 3])]) -> Vec<(usize, [f64; 3])> {
    let key = |p: [f64; 3]| -> [u64; 3] { [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()] };
    let b_keys: std::collections::HashSet<[u64; 3]> = b.iter().map(|(_, p)| key(*p)).collect();
    a.iter()
        .filter(|(_, p)| !b_keys.contains(&key(*p)))
        .copied()
        .collect()
}

fn dump_face_pair(arena: &TopoArena, nb: &NonBijectivePair, idx: usize) {
    eprintln!();
    eprintln!("─── non-bijective pair #{idx} ───");
    eprintln!(
        "face_a = FaceIdx({}), face_b = FaceIdx({}), edge = {:?}",
        nb.face_a.0, nb.face_b.0, nb.edge
    );
    eprintln!(
        "unmatched_a_count = {}, unmatched_b_count = {}",
        nb.unmatched_a_count, nb.unmatched_b_count
    );

    // Outer loop walks
    let face_a = &arena.faces[nb.face_a.0];
    let face_b = &arena.faces[nb.face_b.0];
    let outer_a = collect_loop_vertices(arena, face_a.outer_loop);
    let outer_b = collect_loop_vertices(arena, face_b.outer_loop);

    eprintln!("face_a outer_loop has {} boundary vertices:", outer_a.len());
    for (i, (v, p)) in outer_a.iter().enumerate() {
        eprintln!("  a[{i}]: VertexIdx({v}) {}", fmt_pt(*p));
    }
    eprintln!("face_b outer_loop has {} boundary vertices:", outer_b.len());
    for (i, (v, p)) in outer_b.iter().enumerate() {
        eprintln!("  b[{i}]: VertexIdx({v}) {}", fmt_pt(*p));
    }

    // Inner-loop dump (if any)
    if !face_a.inner_loops.is_empty() {
        eprintln!("face_a has {} inner_loops:", face_a.inner_loops.len());
        for (li, &lp) in face_a.inner_loops.iter().enumerate() {
            let inner = collect_loop_vertices(arena, lp);
            eprintln!("  inner_a[{li}] ({} verts):", inner.len());
            for (i, (v, p)) in inner.iter().enumerate() {
                eprintln!("    {i}: VertexIdx({v}) {}", fmt_pt(*p));
            }
        }
    }
    if !face_b.inner_loops.is_empty() {
        eprintln!("face_b has {} inner_loops:", face_b.inner_loops.len());
        for (li, &lp) in face_b.inner_loops.iter().enumerate() {
            let inner = collect_loop_vertices(arena, lp);
            eprintln!("  inner_b[{li}] ({} verts):", inner.len());
            for (i, (v, p)) in inner.iter().enumerate() {
                eprintln!("    {i}: VertexIdx({v}) {}", fmt_pt(*p));
            }
        }
    }

    // T-junction candidates: vertex on one face's loop but not the other.
    let only_in_a = vertex_set_diff(&outer_a, &outer_b);
    let only_in_b = vertex_set_diff(&outer_b, &outer_a);
    eprintln!(
        "T-junction candidates: {} vertex(es) on face_a but not face_b, {} on face_b but not face_a",
        only_in_a.len(),
        only_in_b.len()
    );
    for (v, p) in &only_in_a {
        eprintln!("  only_in_a: VertexIdx({v}) {}", fmt_pt(*p));
    }
    for (v, p) in &only_in_b {
        eprintln!("  only_in_b: VertexIdx({v}) {}", fmt_pt(*p));
    }

    // Sample unmatched directed edges from the oracle.
    eprintln!(
        "sample_unmatched_a (first {}):",
        nb.sample_unmatched_a.len()
    );
    for (i, (p, q)) in nb.sample_unmatched_a.iter().enumerate() {
        eprintln!("  a-edge[{i}]: {} → {}", fmt_pt(*p), fmt_pt(*q));
    }
    eprintln!(
        "sample_unmatched_b (first {}):",
        nb.sample_unmatched_b.len()
    );
    for (i, (p, q)) in nb.sample_unmatched_b.iter().enumerate() {
        eprintln!("  b-edge[{i}]: {} → {}", fmt_pt(*p), fmt_pt(*q));
    }
}

/// Locate the last non-suppressed feature with a solid output, mirroring
/// `ModelBuilder::tessellate_last_with_tol`.
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

/// Run the full diagnostic. Returned `(face_a_count, face_b_count)` per
/// nb pair lets the caller (or the `--nocapture` dump consumer) confirm
/// determinism across runs without re-eyeballing 200 lines of stderr.
fn run_diagnosis() -> (usize, RenderMesh, BTreeMap<u64, FaceIdx>, TopoArena) {
    let waffle_path = Path::new(ASSAY_DIR).join("R0033.waffle");
    let meta_path = Path::new(ASSAY_DIR).join("R0033.meta.json");
    assert!(
        waffle_path.exists(),
        "R0033.waffle missing — run from repo root or check ASSAY_DIR"
    );

    let waffle_json = std::fs::read_to_string(&waffle_path).expect("read R0033.waffle");
    let meta_json = std::fs::read_to_string(&meta_path).expect("read R0033.meta.json");
    let meta: R0033Meta = serde_json::from_str(&meta_json).expect("parse R0033.meta.json");

    // Mirror the assay-runner: Yang pipeline ON, scale-adaptive tolerance.
    std::env::set_var("YANG_BOOLEAN", "1");
    let tess_tol = (meta.scale * 0.01).clamp(1e-9, 0.1);
    eprintln!(
        "R0033 scale = {:.6e}, tess_tol = {:.6e}",
        meta.scale, tess_tol
    );

    let mut state = EngineState::new();
    let mut kernel = WaffleKernel::new();

    // LoadProject through the same dispatch path the assay runner uses.
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
    eprintln!("engine_errors after load: {} entries", engine_errors.len());
    for (id, msg) in &engine_errors {
        eprintln!("  err: {id} — {msg}");
    }
    assert!(
        engine_errors.is_empty(),
        "R0033 should load without engine errors under YANG_BOOLEAN=1; got {:?}",
        engine_errors
    );

    let handle = last_solid_handle(&state).expect("R0033 should have at least one solid feature");

    let mesh = kernel
        .tessellate(&handle, tess_tol)
        .expect("tessellate R0033 last solid");
    eprintln!(
        "tessellated mesh: {} vertices, {} indices ({} tris), {} face_ranges",
        mesh.vertices.len() / 3,
        mesh.indices.len(),
        mesh.indices.len() / 3,
        mesh.face_ranges.len()
    );

    let (arena, face_map) = kernel
        .brep_diagnostic_view(&handle)
        .expect("R0033 solid should be retrievable");
    eprintln!(
        "B-Rep arena: {} vertices, {} half_edges, {} edges, {} loops, {} faces",
        arena.vertices.len(),
        arena.half_edges.len(),
        arena.edges.len(),
        arena.loops.len(),
        arena.faces.len()
    );

    let nb_count = {
        let report = check_face_pair_bijective(&mesh, face_map, arena);
        eprintln!(
            "bijective oracle: total_pairs_examined = {}, bijective_pairs = {}, non_bijective_pairs = {}",
            report.total_pairs_examined,
            report.bijective_pairs,
            report.non_bijective_pairs.len()
        );
        for (i, nb) in report.non_bijective_pairs.iter().enumerate() {
            dump_face_pair(arena, nb, i);
        }
        report.non_bijective_pairs.len()
    };

    // Clone the arena + face_map so the test can return owned data
    // without holding a borrow on `kernel`. Kept after the dump so the
    // dump used live references and we paid the clone cost only once.
    let arena_owned = arena.clone();
    let face_map_owned = face_map.clone();
    (nb_count, mesh, face_map_owned, arena_owned)
}

#[test]
fn diagnose_r0033_t_junction_pattern() {
    let (nb_count, _mesh, _face_map, _arena) = run_diagnosis();

    // Re-run inside the same test process to characterize determinism.
    // The first call in a fresh process is the canonical measurement
    // — across 5 invocations of this test, run-1 always reports 2 nb
    // pairs, matching `specs/tessellation_pr3_corpus_dump.md`. Run-2
    // sometimes reports 3 (a third nb pair appears), suggesting
    // iteration-order non-determinism in the boolean pipeline (likely
    // Rust's `HashMap` RandomState reseeding between sequential calls).
    // The flap is RECORDED but not asserted away — fixing the
    // underlying T-junction defect should also stabilize the count.
    // See the spec dump in `specs/tessellation_bounded_residuals.md`.
    eprintln!();
    eprintln!("═══ Cross-call stability check: re-running diagnosis ═══");
    let (nb_count_2, _, _, _) = run_diagnosis();
    eprintln!("first-call nb_count = {nb_count}, second-call nb_count = {nb_count_2}");
    if nb_count != nb_count_2 {
        eprintln!(
            "NOTE: nb count flapped between calls ({nb_count} vs {nb_count_2}). \
             Anchor on the first-call value (matches PR3 corpus dump)."
        );
    }

    // Final RED assertion (Yang §4.1.1 contract; PR5 will turn this GREEN).
    // We assert on the first-call value, which is stable across runs.
    assert!(
        nb_count == 0,
        "R0033 has {nb_count} non-bijective face pair(s) on first tessellation; \
         expected 0 per Yang 2025 §4.1.1. \
         See specs/tessellation_bounded_residuals.md PR4 dump for diagnostic data."
    );
}
