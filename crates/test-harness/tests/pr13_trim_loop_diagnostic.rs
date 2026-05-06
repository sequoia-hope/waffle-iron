//! PR13 — Trim-loop chaining diagnostic for R0020/R0021 violations.
//!
//! AUDIT-ONLY scaffolding. Runs the boolean pipeline on R0020 and R0021
//! and captures, for each non-bijective face pair reported by the Stage 1
//! `BijectiveFacePairOracle`:
//!
//! - The pair coordinates (face_a, face_b, edge index).
//! - The unmatched directed edges (sample_unmatched_a, sample_unmatched_b).
//! - The shared B-Rep edge endpoint positions.
//! - Branch-point degree at each unmatched edge endpoint (how many outgoing
//!   half-edges does each face have at the branch vertex?) by walking the
//!   B-Rep arena's loop chains.
//! - Face-local frame (face_normal, face_u, face_v) computed the same way
//!   as `extract_trim_boundaries` does, for each face.
//!
//! Purpose: produce empirical evidence for PR13 spec §8 — which fix
//! approach (A/B/C/D) the data supports for the CW-angular sort defect at
//! branch points in `extract_trim_boundaries`.
//!
//! For the branch-point dump from inside `extract_trim_boundaries` itself,
//! this probe expects to be run with `PR13_DUMP_BRANCH=1` set: the
//! corresponding env-gated `eprintln!`s in the production code emit to
//! stderr, captured with `--nocapture`. **Those production-code
//! instrumentation hooks are TEMPORARY and MUST be reverted before the
//! probe is committed.** The probe itself (this file) is the audit
//! deliverable.
//!
//! `#[ignore]`-gated; `cargo test -p test-harness --test
//! pr13_trim_loop_diagnostic -- --ignored --nocapture`.
//!
//! Refs:
//! - `/home/claude/.claude/plans/fluttering-rolling-crystal.md` (PR13 plan).
//! - `specs/yang_stage1_bijective.md` §8 amendments (PR12 archaeological anchor).
//! - `docs/audits/pr12_stage1_diagnostic.md` (PR12 cluster classification).
//! - `crates/kernel/src/boolean/topology_extract.rs::extract_trim_boundaries`
//!   (lines 967-1224 — focus 1137-1194 CW-angular sort).
//! - `crates/kernel/src/tessellation/bijective.rs::NonBijectivePair` (lines 169-189).
//! - `crates/test-harness/tests/pr12_stage1_diagnostic.rs` (audit pattern).
//!
//! Companion report: `docs/audits/pr13_trim_loop_diagnostic.md`.

use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use kernel::diagnostics::{with_yang_oracle_capture, OracleRunSummary};
use kernel::tessellation::bijective::{
    check_face_pair_bijective, BijectivityReport, NonBijectivePair,
};
use kernel::topology::arena::TopoArena;
use kernel::topology::half_edge::{FaceIdx, HalfEdgeIdx, LoopIdx};
use kernel::{Kernel, RenderMesh, WaffleKernel};
use wasm_bridge::messages::UiToEngine;
use wasm_bridge::{dispatch, EngineState};

const ASSAY_DIR: &str = "../../app/tests/cases/assay";
const PER_CASE_TIMEOUT: Duration = Duration::from_secs(120);

/// Cases under analysis. Per PR12 §8 amendment + audit, these are the
/// X-non-coplanar cluster (no S0 stub, no flap, stable Stage 1 fire on
/// operand A).
const PR13_CASES: &[&str] = &["R0020", "R0021"];

#[derive(serde::Deserialize)]
struct CaseMeta {
    scale: f64,
}

fn fmt_pt(p: [f64; 3]) -> String {
    format!("({:.6e}, {:.6e}, {:.6e})", p[0], p[1], p[2])
}

fn pos_key(p: [f64; 3]) -> [u64; 3] {
    [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]
}

/// Walk a B-Rep loop's half-edge chain and collect (vertex_idx, position)
/// for each origin in CCW order.
fn collect_loop_vertices(arena: &TopoArena, loop_idx: LoopIdx) -> Vec<(usize, [f64; 3])> {
    let mut out = Vec::new();
    if loop_idx.0 >= arena.loops.len() {
        return out;
    }
    let start: HalfEdgeIdx = arena.loops[loop_idx.0].half_edge;
    let mut he = start;
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

/// One per-feature artifact captured after LoadProject.
struct PerFeatureArtifact {
    feature_index: usize,
    description: String,
    mesh: RenderMesh,
    face_map: std::collections::BTreeMap<u64, FaceIdx>,
    arena: TopoArena,
}

/// Result of running one case.
#[allow(dead_code)] // case_id is for future per-case dispatch
struct CaseDiagnostic {
    case_id: String,
    summary: Option<OracleRunSummary>,
    /// Per-feature artifacts gathered AFTER LoadProject completes. For
    /// multi-op cases the LAST artifact is the FINAL solid; the
    /// SECOND-TO-LAST is the input to the FINAL boolean op (which is
    /// what the Stage 1 oracle's `with_yang_oracle_capture` snapshotted
    /// during op execution). We collect ALL of them to allow the report
    /// to focus on whichever one the violations are most readable from.
    features: Vec<PerFeatureArtifact>,
}

fn run_one_case(case_id: &str) -> CaseDiagnostic {
    let waffle_path = Path::new(ASSAY_DIR).join(format!("{case_id}.waffle"));
    let meta_path = Path::new(ASSAY_DIR).join(format!("{case_id}.meta.json"));
    let waffle_json = match std::fs::read_to_string(&waffle_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[PR13_PROBE] failed to read {waffle_path:?}: {e}");
            return CaseDiagnostic {
                case_id: case_id.to_string(),
                summary: None,
                features: Vec::new(),
            };
        }
    };
    let meta_json = std::fs::read_to_string(&meta_path).expect("read meta");
    let meta: CaseMeta = serde_json::from_str(&meta_json).expect("parse meta");
    let tess_tol = (meta.scale * 0.01).clamp(1e-9, 0.1);
    eprintln!(
        "[PR13_PROBE] === {} === scale={:.6e}, tess_tol={:.6e}",
        case_id, meta.scale, tess_tol
    );

    std::env::set_var("YANG_BOOLEAN", "1");

    // Run LoadProject under oracle capture. The Stage 1 oracle's snapshot
    // is captured DURING the boolean execution (last Yang boolean inside
    // `f`). The branch-point `eprintln!`s in production code (env-gated
    // `PR13_DUMP_BRANCH=1` if instrumentation is present) emit to stderr
    // and appear in --nocapture output. AFTER the boolean ops complete,
    // we re-tessellate every intermediate solid via the kernel — the
    // INTERMEDIATE solid (n-2) is the input to the LAST boolean op, and
    // therefore is the artifact the Stage 1 oracle saw.
    let case_id_owned = case_id.to_string();
    let (summary, (engine_errors, features)) =
        with_yang_oracle_capture(&case_id_owned, move || {
            let mut state = EngineState::new();
            let mut k = WaffleKernel::new();
            let response = dispatch(
                &mut state,
                UiToEngine::LoadProject { data: waffle_json },
                &mut k,
            );
            let _ = response;

            let engine_errors = state.engine.errors.clone();

            // Collect per-feature artifacts: tessellation + B-Rep view.
            let mut features = Vec::new();
            let tree = &state.engine.tree;
            let limit = tree.active_index.unwrap_or(tree.features.len());
            for (idx, feature) in tree.features[..limit].iter().enumerate() {
                if feature.suppressed {
                    continue;
                }
                let result = match state.engine.get_result(feature.id) {
                    Some(r) => r,
                    None => continue,
                };
                let handle = match result.outputs.first().map(|(_, body)| body.handle.clone()) {
                    Some(h) => h,
                    None => continue,
                };
                let mesh = match k.tessellate(&handle, tess_tol) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let (arena_ref, fm_ref) = match k.brep_diagnostic_view(&handle) {
                    Some((a, f)) => (a, f),
                    None => continue,
                };
                features.push(PerFeatureArtifact {
                    feature_index: idx,
                    description: format!("feature[{}] name={:?}", idx, feature.name),
                    mesh,
                    face_map: fm_ref.clone(),
                    arena: arena_ref.clone(),
                });
            }

            (engine_errors, features)
        });

    eprintln!(
        "[PR13_PROBE] {case_id} engine_errors={}",
        engine_errors.len()
    );
    for (id, msg) in &engine_errors {
        eprintln!("[PR13_PROBE]   err: {id} — {msg}");
    }

    CaseDiagnostic {
        case_id: case_id.to_string(),
        summary: Some(summary),
        features,
    }
}

/// Walk the LAST solid's arena to count outgoing half-edges from a vertex
/// position (using f64 byte-equal vertex matching).
fn outgoing_half_edges_at_pos(arena: &TopoArena, pos: [f64; 3]) -> Vec<(usize, FaceIdx, [f64; 3])> {
    let key = pos_key(pos);
    let mut out = Vec::new();
    for (he_idx, he) in arena.half_edges.iter().enumerate() {
        let origin_pos = arena.vertices[he.origin.0].position;
        if pos_key(origin_pos) != key {
            continue;
        }
        if he.loop_.0 >= arena.loops.len() {
            continue;
        }
        let face_idx = arena.loops[he.loop_.0].face;
        // The outgoing edge ends at the next half-edge's origin
        let next_origin = arena.vertices[arena.half_edges[he.next.0].origin.0].position;
        out.push((he_idx, face_idx, next_origin));
    }
    out
}

/// Count outgoing MESH directed edges at the given byte-identical position
/// for a specific face label. Returns the list of (target_pos, edge_idx_in_mesh).
fn outgoing_mesh_edges_at_pos(
    mesh: &RenderMesh,
    face_map: &std::collections::BTreeMap<u64, FaceIdx>,
    target_face: FaceIdx,
    origin_pos: [f64; 3],
) -> Vec<[f64; 3]> {
    let target_key = pos_key(origin_pos);
    let mut out = Vec::new();
    for range in &mesh.face_ranges {
        let mapped = match face_map.get(&range.face_id.0).copied() {
            Some(f) => f,
            None => continue,
        };
        if mapped != target_face {
            continue;
        }
        let start = range.start_index as usize;
        let end = (range.end_index as usize).min(mesh.indices.len());
        let mut i = start;
        while i + 2 < end {
            let v: [usize; 3] = [
                mesh.indices[i] as usize,
                mesh.indices[i + 1] as usize,
                mesh.indices[i + 2] as usize,
            ];
            for k in 0..3 {
                let p = [
                    mesh.vertices[v[k] * 3] as f64,
                    mesh.vertices[v[k] * 3 + 1] as f64,
                    mesh.vertices[v[k] * 3 + 2] as f64,
                ];
                if pos_key(p) != target_key {
                    continue;
                }
                let q = [
                    mesh.vertices[v[(k + 1) % 3] * 3] as f64,
                    mesh.vertices[v[(k + 1) % 3] * 3 + 1] as f64,
                    mesh.vertices[v[(k + 1) % 3] * 3 + 2] as f64,
                ];
                out.push(q);
            }
            i += 3;
        }
    }
    out
}

/// Dump one non-bijective pair with branch-point analysis.
fn dump_nb_pair(
    arena: &TopoArena,
    mesh_for_pair: &RenderMesh,
    fm_for_pair: &std::collections::BTreeMap<u64, FaceIdx>,
    operand_label: &str,
    nb: &NonBijectivePair,
    idx: usize,
) {
    eprintln!();
    eprintln!("─── NB pair #{idx} (operand {operand_label}) ───");
    eprintln!(
        "face_a = FaceIdx({}), face_b = FaceIdx({}), edge = {:?}",
        nb.face_a.0, nb.face_b.0, nb.edge
    );
    eprintln!(
        "unmatched_a_count = {}, unmatched_b_count = {}",
        nb.unmatched_a_count, nb.unmatched_b_count
    );

    // Edge endpoint positions
    if let Some(edge_idx) = nb.edge {
        if edge_idx.0 < arena.edges.len() {
            let edge = &arena.edges[edge_idx.0];
            let he_a = &arena.half_edges[edge.half_edge.0];
            let he_b = &arena.half_edges[he_a.twin.0];
            let v0_pos = arena.vertices[he_a.origin.0].position;
            let v1_pos = arena.vertices[he_b.origin.0].position;
            eprintln!(
                "B-Rep edge[{}]: v0 = {} (vidx {}), v1 = {} (vidx {})",
                edge_idx.0,
                fmt_pt(v0_pos),
                he_a.origin.0,
                fmt_pt(v1_pos),
                he_b.origin.0,
            );
        }
    }

    // Face A's outer loop walk
    if nb.face_a.0 < arena.faces.len() {
        let outer_loop = arena.faces[nb.face_a.0].outer_loop;
        let walk = collect_loop_vertices(arena, outer_loop);
        eprintln!("face_a outer loop ({} verts):", walk.len());
        for (i, (v, p)) in walk.iter().enumerate() {
            eprintln!("  a[{i}]: VertexIdx({v}) {}", fmt_pt(*p));
        }
    }

    // Face B's outer loop walk
    if nb.face_b.0 < arena.faces.len() {
        let outer_loop = arena.faces[nb.face_b.0].outer_loop;
        let walk = collect_loop_vertices(arena, outer_loop);
        eprintln!("face_b outer loop ({} verts):", walk.len());
        for (i, (v, p)) in walk.iter().enumerate() {
            eprintln!("  b[{i}]: VertexIdx({v}) {}", fmt_pt(*p));
        }
    }

    // Sample unmatched directed edges
    eprintln!(
        "sample_unmatched_a (first {}):",
        nb.sample_unmatched_a.len()
    );
    for (i, (p, q)) in nb.sample_unmatched_a.iter().enumerate() {
        eprintln!("  a-edge[{i}]: {} → {}", fmt_pt(*p), fmt_pt(*q));

        // Reciprocal-equality check: is sample_unmatched_a[i] byte-equal to
        // sample_unmatched_b[i]? PR12's archaeological anchor said yes.
        if i < nb.sample_unmatched_b.len() {
            let (bp, bq) = nb.sample_unmatched_b[i];
            let same_dir = pos_key(*p) == pos_key(bp) && pos_key(*q) == pos_key(bq);
            let reciprocal = pos_key(*p) == pos_key(bq) && pos_key(*q) == pos_key(bp);
            eprintln!(
                "    ↳ vs b[{i}]={}→{} same_direction={} reciprocal={}",
                fmt_pt(bp),
                fmt_pt(bq),
                same_dir,
                reciprocal,
            );
        }
    }
    eprintln!(
        "sample_unmatched_b (first {}):",
        nb.sample_unmatched_b.len()
    );
    for (i, (p, q)) in nb.sample_unmatched_b.iter().enumerate() {
        eprintln!("  b-edge[{i}]: {} → {}", fmt_pt(*p), fmt_pt(*q));
    }

    // For the FIRST sample unmatched edge, dump branch-point degree at
    // both endpoints, in BOTH the B-Rep arena (coarse, face-corner only)
    // AND the rendermesh (interior tessellation vertices).
    if let Some((p, q)) = nb.sample_unmatched_a.first() {
        eprintln!("Branch-point analysis at sample_unmatched_a[0]:");
        for (label, pt) in [("p", *p), ("q", *q)] {
            // B-Rep arena view (coarse).
            let outs = outgoing_half_edges_at_pos(arena, pt);
            let outs_a: Vec<_> = outs.iter().filter(|(_, f, _)| *f == nb.face_a).collect();
            let outs_b: Vec<_> = outs.iter().filter(|(_, f, _)| *f == nb.face_b).collect();
            eprintln!(
                "  {} = {}: arena {} outgoing total ({} face_a, {} face_b)",
                label,
                fmt_pt(pt),
                outs.len(),
                outs_a.len(),
                outs_b.len(),
            );
            // Mesh view (interior verts included).
            let mesh_a = outgoing_mesh_edges_at_pos(mesh_for_pair, fm_for_pair, nb.face_a, pt);
            let mesh_b = outgoing_mesh_edges_at_pos(mesh_for_pair, fm_for_pair, nb.face_b, pt);
            eprintln!(
                "  {} = {}: mesh face_a {} outgoing, face_b {} outgoing",
                label,
                fmt_pt(pt),
                mesh_a.len(),
                mesh_b.len(),
            );
            for (i, t) in mesh_a.iter().enumerate() {
                eprintln!("    [face_a mesh out {}] → {}", i, fmt_pt(*t));
            }
            for (i, t) in mesh_b.iter().enumerate() {
                eprintln!("    [face_b mesh out {}] → {}", i, fmt_pt(*t));
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BranchDegree {
    /// 0 outgoing edges from face at this vertex (degenerate / missing).
    Zero,
    /// Single outgoing edge — no branch decision required.
    One,
    /// Two outgoing edges — minimal branching.
    Two,
    /// 3+ outgoing edges — real branch.
    ThreePlus,
}

#[allow(dead_code)] // edge_idx + deg_q_face_b useful for downstream analysis
#[derive(Debug, Clone)]
struct ViolationClass {
    case_id: String,
    pair_idx: usize,
    face_a: usize,
    face_b: usize,
    edge_idx: Option<usize>,
    /// Did sample_unmatched_a[0] match sample_unmatched_b[0] byte-for-byte?
    sample_byte_equal: bool,
    /// Or are they reciprocal (Yang §4.1.1 expected behavior)?
    sample_reciprocal: bool,
    /// Branch-point degree at p in face A.
    deg_p_face_a: BranchDegree,
    /// Branch-point degree at p in face B.
    deg_p_face_b: BranchDegree,
    /// Branch-point degree at q in face A.
    deg_q_face_a: BranchDegree,
    /// Branch-point degree at q in face B.
    deg_q_face_b: BranchDegree,
}

fn classify_branch_degree(n: usize) -> BranchDegree {
    match n {
        0 => BranchDegree::Zero,
        1 => BranchDegree::One,
        2 => BranchDegree::Two,
        _ => BranchDegree::ThreePlus,
    }
}

fn classify_violation(
    case_id: &str,
    pair_idx: usize,
    _arena: &TopoArena,
    mesh: &RenderMesh,
    face_map: &std::collections::BTreeMap<u64, FaceIdx>,
    nb: &NonBijectivePair,
) -> ViolationClass {
    let (sample_byte_equal, sample_reciprocal) = if let (Some((p, q)), Some((bp, bq))) =
        (nb.sample_unmatched_a.first(), nb.sample_unmatched_b.first())
    {
        let same_dir = pos_key(*p) == pos_key(*bp) && pos_key(*q) == pos_key(*bq);
        let reciprocal = pos_key(*p) == pos_key(*bq) && pos_key(*q) == pos_key(*bp);
        (same_dir, reciprocal)
    } else {
        (false, false)
    };

    // Branch-point degree from the MESH (interior verts included).
    let mut deg_p_a = BranchDegree::Zero;
    let mut deg_p_b = BranchDegree::Zero;
    let mut deg_q_a = BranchDegree::Zero;
    let mut deg_q_b = BranchDegree::Zero;
    if let Some((p, q)) = nb.sample_unmatched_a.first() {
        let mp_a = outgoing_mesh_edges_at_pos(mesh, face_map, nb.face_a, *p);
        let mp_b = outgoing_mesh_edges_at_pos(mesh, face_map, nb.face_b, *p);
        let mq_a = outgoing_mesh_edges_at_pos(mesh, face_map, nb.face_a, *q);
        let mq_b = outgoing_mesh_edges_at_pos(mesh, face_map, nb.face_b, *q);
        deg_p_a = classify_branch_degree(mp_a.len());
        deg_p_b = classify_branch_degree(mp_b.len());
        deg_q_a = classify_branch_degree(mq_a.len());
        deg_q_b = classify_branch_degree(mq_b.len());
    }

    ViolationClass {
        case_id: case_id.to_string(),
        pair_idx,
        face_a: nb.face_a.0,
        face_b: nb.face_b.0,
        edge_idx: nb.edge.map(|e| e.0),
        sample_byte_equal,
        sample_reciprocal,
        deg_p_face_a: deg_p_a,
        deg_p_face_b: deg_p_b,
        deg_q_face_a: deg_q_a,
        deg_q_face_b: deg_q_b,
    }
}

/// Run one case, capture diagnostic, dump to stderr, return classifications.
fn analyze_one_case(case_id: &str) -> Vec<ViolationClass> {
    eprintln!();
    eprintln!("════════════════════════════════════════════════════════════");
    eprintln!(" PR13 trim-loop diagnostic: {case_id}");
    eprintln!("════════════════════════════════════════════════════════════");

    // Spawn worker thread with timeout. Two owned clones so neither closure
    // borrows the &str argument.
    let case_id_for_thread = case_id.to_string();
    let case_id_for_panic = case_id.to_string();
    let (tx, rx) = mpsc::channel::<CaseDiagnostic>();
    let _h = thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_one_case(&case_id_for_thread)
        }));
        let payload = result.unwrap_or_else(|p| {
            let msg = p
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| p.downcast_ref::<String>().map(|s| s.as_str()))
                .unwrap_or("<panic>");
            eprintln!("[PR13_PROBE] case panicked: {msg}");
            CaseDiagnostic {
                case_id: case_id_for_panic,
                summary: None,
                features: Vec::new(),
            }
        });
        let _ = tx.send(payload);
    });

    let diag = match rx.recv_timeout(PER_CASE_TIMEOUT) {
        Ok(d) => d,
        Err(_) => {
            eprintln!("[PR13_PROBE] {case_id} TIMED OUT after {PER_CASE_TIMEOUT:?}");
            return Vec::new();
        }
    };

    // Run the bijective oracle on EVERY captured intermediate feature so
    // we can pinpoint which boolean op produced the violations. For
    // multi-op cases the SECOND-TO-LAST feature's solid IS the input to
    // the FINAL yang_boolean — that's where the Stage 1 oracle's
    // operand-A view comes from.
    if diag.features.is_empty() {
        eprintln!("[PR13_PROBE] {case_id}: no per-feature artifacts; skipping");
        return Vec::new();
    }
    eprintln!(
        "[PR13_PROBE] {case_id}: {} per-feature artifacts captured",
        diag.features.len()
    );
    for f in &diag.features {
        eprintln!(
            "  feature_idx={}: {} | mesh: {}v/{}i/{}r | arena: {}v/{}he/{}e/{}f",
            f.feature_index,
            f.description,
            f.mesh.vertices.len() / 3,
            f.mesh.indices.len(),
            f.mesh.face_ranges.len(),
            f.arena.vertices.len(),
            f.arena.half_edges.len(),
            f.arena.edges.len(),
            f.arena.faces.len(),
        );
    }

    // Run bijective oracle on each feature's solid; record the FIRST
    // feature whose tessellation has non-bijective pairs (that's the op
    // that introduced the defect).
    let mut feat_with_violations: Vec<(usize, BijectivityReport)> = Vec::new();
    for f in &diag.features {
        let r = check_face_pair_bijective(&f.mesh, &f.face_map, &f.arena);
        eprintln!(
            "  bijective[feat={}]: {}/{} pairs, {} non-bijective",
            f.feature_index,
            r.bijective_pairs,
            r.total_pairs_examined,
            r.non_bijective_pairs.len(),
        );
        if !r.is_bijective() {
            feat_with_violations.push((f.feature_index, r));
        }
    }

    if feat_with_violations.is_empty() {
        eprintln!(
            "[PR13_PROBE] {case_id}: NO per-feature violations found in re-tessellated artifacts. \
             Stage 1 oracle may have caught violations at a different snapshot point \
             (the IN-progress result during yang_boolean_inner)."
        );
        // Even so, the oracle summary's Stage 1 message indicates the count.
        if let Some(summary) = diag.summary.as_ref() {
            eprintln!();
            eprintln!("[PR13_PROBE] {case_id} oracle summary:");
            for v in &summary.per_oracle {
                let verdict = match &v.violation {
                    None => ".".to_string(),
                    Some(viol) => format!("X ({})", viol.message),
                };
                eprintln!("  {:?} [{}] = {}", v.stage, v.oracle_name, verdict);
            }
        }
        return Vec::new();
    }

    // Pick the EARLIEST feature with violations as the canonical artifact —
    // this is the boolean op that first produced a non-bijective B-Rep.
    let (target_idx, target_report) = feat_with_violations.into_iter().next().unwrap();
    let target_feat = diag
        .features
        .iter()
        .find(|f| f.feature_index == target_idx)
        .unwrap();
    let mesh = &target_feat.mesh;
    let arena = &target_feat.arena;
    let face_map = &target_feat.face_map;
    eprintln!();
    eprintln!(
        "[PR13_PROBE] {case_id} ANALYSIS TARGET: feature[{}] (first with NB violations)",
        target_idx
    );

    let mut classes = Vec::new();
    for (i, nb) in target_report.non_bijective_pairs.iter().enumerate() {
        dump_nb_pair(arena, mesh, face_map, "A_first_violating", nb, i);
        classes.push(classify_violation(case_id, i, arena, mesh, face_map, nb));
    }

    if let Some(summary) = diag.summary.as_ref() {
        eprintln!();
        eprintln!("[PR13_PROBE] {case_id} oracle summary:");
        for v in &summary.per_oracle {
            let verdict = match &v.violation {
                None => ".".to_string(),
                Some(viol) => format!("X ({})", viol.message),
            };
            eprintln!("  {:?} [{}] = {}", v.stage, v.oracle_name, verdict);
        }
        if let Some(fail_stage) = summary.first_failing_stage {
            eprintln!("  first_failing_stage = {fail_stage:?}");
        }
        if let Some(err) = &summary.pipeline_error {
            eprintln!("  pipeline_error = {err}");
        }
    }

    classes
}

/// Pretty-print a violation classification table.
fn dump_classification(all: &[ViolationClass]) {
    eprintln!();
    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!(
        " PR13 violation classification (total {} violations)",
        all.len()
    );
    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!(
        "| {:<5} | {:<3} | {:<5} | {:<5} | {:<5} | {:<5} | {:<5} | {:<10} | {:<10} |",
        "case", "#", "fA", "fB", "deg_pA", "deg_pB", "deg_qA", "byte_eq", "reciprocal"
    );
    eprintln!(
        "|-------|-----|-------|-------|--------|--------|--------|------------|------------|"
    );
    for c in all {
        eprintln!(
            "| {:<5} | {:<3} | F({:<3}) | F({:<3}) | {:<6?} | {:<6?} | {:<6?} | {:<10} | {:<10} |",
            c.case_id,
            c.pair_idx,
            c.face_a,
            c.face_b,
            c.deg_p_face_a,
            c.deg_p_face_b,
            c.deg_q_face_a,
            c.sample_byte_equal,
            c.sample_reciprocal,
        );
    }

    // Summary counts.
    let n_byte_eq = all.iter().filter(|c| c.sample_byte_equal).count();
    let n_recip = all.iter().filter(|c| c.sample_reciprocal).count();
    let n_branch_pa = all
        .iter()
        .filter(|c| matches!(c.deg_p_face_a, BranchDegree::ThreePlus))
        .count();
    let n_branch_pb = all
        .iter()
        .filter(|c| matches!(c.deg_p_face_b, BranchDegree::ThreePlus))
        .count();
    let n_zero_pa = all
        .iter()
        .filter(|c| matches!(c.deg_p_face_a, BranchDegree::Zero))
        .count();
    let n_zero_pb = all
        .iter()
        .filter(|c| matches!(c.deg_p_face_b, BranchDegree::Zero))
        .count();
    eprintln!();
    eprintln!("Summary:");
    eprintln!(
        "  byte-identical sample (PR12 anchor):    {}/{}",
        n_byte_eq,
        all.len()
    );
    eprintln!(
        "  reciprocal sample (Yang §4.1.1 OK):     {}/{}",
        n_recip,
        all.len()
    );
    eprintln!(
        "  real branch (≥3 outgoing) at p in fA:   {}/{}",
        n_branch_pa,
        all.len()
    );
    eprintln!(
        "  real branch (≥3 outgoing) at p in fB:   {}/{}",
        n_branch_pb,
        all.len()
    );
    eprintln!(
        "  zero outgoing at p in fA (missing):     {}/{}",
        n_zero_pa,
        all.len()
    );
    eprintln!(
        "  zero outgoing at p in fB (missing):     {}/{}",
        n_zero_pb,
        all.len()
    );
}

#[test]
#[ignore]
fn pr13_trim_loop_diagnostic_capture() {
    eprintln!("═══ PR13 trim-loop chaining diagnostic — R0020/R0021 ═══");
    eprintln!("Cases ({}): {}", PR13_CASES.len(), PR13_CASES.join(", "));
    if std::env::var("PR13_DUMP_BRANCH").ok().as_deref() == Some("1") {
        eprintln!(
            "PR13_DUMP_BRANCH=1 detected: extract_trim_boundaries will emit \
             [PR13_DUMP] / [PR13_DUMP_BRANCH] / [PR13_DUMP_FACE] lines to stderr."
        );
    } else {
        eprintln!(
            "[hint] re-run with `PR13_DUMP_BRANCH=1 cargo test ... -- --nocapture` \
             for branch-point traces."
        );
    }

    let mut all_violations = Vec::new();
    for case_id in PR13_CASES {
        let classes = analyze_one_case(case_id);
        all_violations.extend(classes);
    }

    dump_classification(&all_violations);

    eprintln!();
    eprintln!(
        "[PR13_PROBE] capture complete. {} total violations across {} cases.",
        all_violations.len(),
        PR13_CASES.len(),
    );
}
