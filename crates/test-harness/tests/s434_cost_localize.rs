//! I5-1 — §4.3.4 seam-density DOWNSTREAM COST localization (task #88;
//! spec `specs/yang_441_trim_cdt_construction.md` §4-I5 "NEXT ANCHOR").
//!
//! The gate-ON assay times out F0047/F0048/F0059 (>300s CPU) while the
//! in-process model build alone takes ~8s — so the cost lives DOWNSTREAM of
//! the boolean. This runner replays a case gate-off then gate-ON and times
//! every phase the assay's `replay_case` pays, separately:
//!
//!   load → B-Rep topology counts (per feature output: does Stage 6 emit one
//!   B-Rep edge per refined seam segment?) → render tessellation → each mesh
//!   oracle individually (self-intersection is the O(n²) suspect) → the
//!   cheap meta checks → the volume-composition oracle split into its
//!   operand/output scans (output_scan REBUILDS the whole model: a second
//!   full boolean) and its grid total.
//!
//! Measurement vehicle, not an oracle: it replays, prints, and passes.
//! Findings are recorded in the spec (§4-I5), where the localization
//! decision lives.
//!
//! Run (single-threaded — the gate env var is process-global):
//!
//! ```text
//! cargo test -p test-harness --test s434_cost_localize --release \
//!   -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `S434_COST_SKIP_SI=1` skips the `no_self_intersection` oracle: measured
//! 1227s gate-ON on F0059 (98% of the leg) vs 1.00s gate-off — once that
//! number is on record, the skip lets the OTHER phases of further cases be
//! measured in seconds instead of ~20min each.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use test_harness::assay::gen::AssayMeta;
use test_harness::assay::volume_oracle_doc;
use test_harness::oracle;
use test_harness::ModelBuilder;

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

fn timed<T>(case: &str, tag: &str, label: &str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let r = f();
    eprintln!(
        "[s434-cost] {case}/{tag} {label}: {:.2}s",
        t.elapsed().as_secs_f64()
    );
    r
}

/// One full assay-shaped replay with per-phase timing. Mirrors
/// `assay_kv2::replay_case` phase-for-phase (same tolerances, same oracle
/// set, same composition call) so the timings attribute the assay's budget.
fn run_leg(case_id: &str, gate_on: bool) {
    let tag = if gate_on { "ON" } else { "off" };
    if gate_on {
        std::env::set_var("YANG_434_INSERT", "1");
    } else {
        std::env::remove_var("YANG_434_INSERT");
    }
    eprintln!("[s434-cost] ---- {case_id} gate {tag} ----");
    let leg_start = Instant::now();

    let dir = assay_dir();
    let waffle_json = match fs::read_to_string(dir.join(format!("{case_id}.waffle"))) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[s434-cost] {case_id}/{tag} cannot read case: {e}");
            return;
        }
    };
    let meta: AssayMeta = match fs::read_to_string(dir.join(format!("{case_id}.meta.json")))
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
    {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[s434-cost] {case_id}/{tag} cannot read meta: {e}");
            return;
        }
    };

    // Phase 1: the model build (all ops + auto-union booleans).
    let mut builder = ModelBuilder::kernel_v2();
    let ok = timed(case_id, tag, "load (model build)", || {
        builder.load(&waffle_json).is_ok()
    });
    if !ok {
        eprintln!("[s434-cost] {case_id}/{tag} LoadProject failed");
        return;
    }
    for (id, msg) in builder.engine_errors() {
        eprintln!("[s434-cost] {case_id}/{tag} engine error {id}: {msg}");
    }
    for w in builder
        .engine_warnings()
        .iter()
        .filter(|w| w.contains("Auto-union failed"))
    {
        eprintln!("[s434-cost] {case_id}/{tag} warning: {w}");
    }

    // Phase 2: B-Rep topology per feature output — the (a) question. A
    // refined seam carries ~2k mesh segments; if E jumps by that order
    // gate-ON, Stage 6 emits one B-Rep edge per segment.
    {
        let tree = &builder.state.engine.tree;
        let limit = tree.active_index.unwrap_or(tree.features.len());
        let introspect = builder.kernel_ref().as_introspect();
        for (i, feature) in tree.features[..limit].iter().enumerate() {
            if feature.suppressed {
                continue;
            }
            let Some(result) = builder.state.engine.get_result(feature.id) else {
                continue;
            };
            for (key, body) in &result.outputs {
                let h = &body.handle;
                let v = introspect.list_vertices(h).len();
                let e = introspect.list_edges(h).len();
                let f = introspect.list_faces(h).len();
                eprintln!(
                    "[s434-cost] {case_id}/{tag} feature[{i}] \"{}\" {key:?}: V={v} E={e} F={f}",
                    feature.name
                );
            }
        }
    }

    // Phase 3: the render tessellation of the final body (assay tolerance).
    let tess_tol = (meta.scale * 0.01).clamp(1e-9, 0.1);
    let mesh = match timed(case_id, tag, "tessellate_last", || {
        builder.tessellate_last_with_tol(tess_tol)
    }) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[s434-cost] {case_id}/{tag} tessellation failed: {e}");
            return;
        }
    };
    eprintln!(
        "[s434-cost] {case_id}/{tag} render mesh: verts={} tris={} face_ranges={}",
        mesh.vertices.len() / 3,
        mesh.indices.len() / 3,
        mesh.face_ranges.len()
    );

    // Phase 4: each assay mesh oracle, individually timed.
    type NamedCheck<'a> = (&'a str, Box<dyn Fn() -> oracle::OracleVerdict + 'a>);
    let named: Vec<NamedCheck> = vec![
        (
            "watertight",
            Box::new(|| oracle::check_watertight_mesh(&mesh)),
        ),
        (
            "consistent_normals",
            Box::new(|| oracle::check_consistent_normals(&mesh)),
        ),
        (
            "no_degenerate_triangles",
            Box::new(|| oracle::check_no_degenerate_triangles(&mesh)),
        ),
        (
            "unit_normals",
            Box::new(|| oracle::check_unit_normals(&mesh)),
        ),
        (
            "face_range_coverage",
            Box::new(|| oracle::check_face_range_coverage(&mesh)),
        ),
        (
            "valid_indices",
            Box::new(|| oracle::check_valid_indices(&mesh)),
        ),
        (
            "outward_normals",
            Box::new(|| oracle::check_outward_normals(&mesh, 0.95)),
        ),
        (
            "positive_signed_volume",
            Box::new(|| oracle::check_positive_signed_volume(&mesh)),
        ),
        (
            "no_self_intersection",
            Box::new(|| oracle::check_no_self_intersection(&mesh)),
        ),
    ];
    let skip_si = std::env::var_os("S434_COST_SKIP_SI").is_some();
    for (name, check) in &named {
        if skip_si && *name == "no_self_intersection" {
            eprintln!("[s434-cost] {case_id}/{tag} oracle {name}: SKIPPED (S434_COST_SKIP_SI)");
            continue;
        }
        let v = timed(case_id, tag, &format!("oracle {name}"), check);
        if !v.passed {
            eprintln!(
                "[s434-cost] {case_id}/{tag} oracle {name} FAILED: {}",
                v.detail
            );
        }
    }

    // Phase 5: the cheap meta checks (min-tri, volume magnitude, euler,
    // bbox), lumped — they share one timer because none is a suspect.
    timed(
        case_id,
        tag,
        "meta checks (min-tri/vol-mag/euler/bbox)",
        || {
            let ops: Vec<(String, String)> = meta
                .operations
                .iter()
                .map(|o| (o.kind.clone(), o.profile_type.clone()))
                .collect();
            let _ = oracle::check_minimum_triangle_count(&mesh, &ops);
            let _ = oracle::check_volume_magnitude(&mesh, meta.scale);
            let _ = oracle::check_mesh_euler_characteristic(&mesh, meta.oracles.euler_target);
        },
    );

    // Phase 6: the volume-composition oracle, split. `output_scan` is a
    // SECOND full model build (booleans again) + tessellate_live; the
    // operand scans are single-op builds (no boolean). The evaluate_
    // composition total on top adds the grid volume arithmetic.
    let doc: serde_json::Value = match serde_json::from_str(&waffle_json) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[s434-cost] {case_id}/{tag} waffle JSON parse failed: {e}");
            return;
        }
    };
    let tol = volume_oracle_doc::oracle_tol(meta.scale);
    for k in 0..meta.operations.len() {
        let s = timed(
            case_id,
            tag,
            &format!("composition operand_scan[{k}]"),
            || volume_oracle_doc::operand_scan(&doc, k, tol),
        );
        if s.is_none() {
            eprintln!("[s434-cost] {case_id}/{tag} operand_scan[{k}] -> None");
        }
    }
    let s = timed(
        case_id,
        tag,
        "composition output_scan (2nd full build)",
        || volume_oracle_doc::output_scan(&doc, tol),
    );
    if s.is_none() {
        eprintln!("[s434-cost] {case_id}/{tag} output_scan -> None");
    }
    let cuts: Vec<bool> = meta.operations.iter().map(|o| o.is_cut).collect();
    let verdict = timed(
        case_id,
        tag,
        "evaluate_composition TOTAL (re-runs scans)",
        || volume_oracle_doc::evaluate_composition(case_id, &doc, &cuts, meta.scale, 64),
    );
    eprintln!("[s434-cost] {case_id}/{tag} composition verdict: {verdict:?}");

    eprintln!(
        "[s434-cost] ==== {case_id} gate {tag} LEG TOTAL {:.2}s ====",
        leg_start.elapsed().as_secs_f64()
    );
}

/// Hang guard, mirroring `s434_density_census`: the orphaned worker keeps
/// running, the sweep moves on and says so. Phase prints are eager, so a
/// timed-out leg still leaves its partial trail on stderr.
fn localize_with_timeout(case_id: &str, timeout: Duration) {
    let (tx, rx) = std::sync::mpsc::channel();
    let id = case_id.to_string();
    let worker = id.clone();
    let handle = std::thread::spawn(move || {
        run_leg(&worker, false);
        run_leg(&worker, true);
        std::env::remove_var("YANG_434_INSERT");
        let _ = tx.send(());
    });
    match rx.recv_timeout(timeout) {
        Ok(()) => {
            let _ = handle.join();
        }
        Err(_) => eprintln!(
            "[s434-cost] {id}: TIMEOUT after {}s (localization incomplete)",
            timeout.as_secs()
        ),
    }
}

macro_rules! localize {
    ($name:ident, $case:literal, $secs:literal, $why:literal) => {
        #[test]
        #[ignore = $why]
        fn $name() {
            localize_with_timeout($case, Duration::from_secs($secs));
        }
    };
}

// The gate-ON TIMEOUT trio (all 2-op single-boolean cases — the "chained
// booleans" attribution from the 08-15 sweep does not fit them; this runner
// exists to replace that guess with a measurement).
localize!(
    localize_f0059,
    "F0059",
    1800,
    "I5-1 cost localization vehicle (task #88, spec yang_441 §4-I5): F0059 cyl-cyl angled, \
     8 refined seams, the gate-ON TIMEOUT representative"
);
localize!(
    localize_f0047,
    "F0047",
    1800,
    "I5-1 cost localization vehicle: F0047 rect+circle cross-plane, gate-ON TIMEOUT"
);
localize!(
    localize_f0048,
    "F0048",
    1800,
    "I5-1 cost localization vehicle: F0048 rect+circle cross-plane, gate-ON TIMEOUT"
);
