//! C-series generator gate: generate the complexity corpus into a temp dir
//! and replay representative GROUP 1/3 cases (the in-boundary bug hunters)
//! through the real dispatch path, asserting the meta's exact-volume /
//! solid-count / Euler expectations hold. This validates the GENERATOR's
//! geometric model (cut aiming, plane bases, symmetric/ThroughAll spans,
//! combine targets, region extrudes) independently of the committed corpus.
//!
//! Group 2 milestone trackers are deliberately NOT gated here — their
//! categories are pinned against the committed corpus in `assay_kv2.rs`.

use std::collections::BTreeMap;
use std::path::Path;

use test_harness::assay::gen::AssayMeta;
use test_harness::assay::gen_complexity::generate_complexity_cases;
use test_harness::helpers::mesh_signed_volume;
use test_harness::ModelBuilder;

struct Replay {
    volume: f64,
    solids: usize,
    engine_errors: Vec<String>,
    warnings: Vec<String>,
}

fn replay(dir: &Path, id: &str) -> (AssayMeta, Replay) {
    let meta: AssayMeta = serde_json::from_str(
        &std::fs::read_to_string(dir.join(format!("{id}.meta.json"))).unwrap(),
    )
    .unwrap();
    let waffle = std::fs::read_to_string(dir.join(format!("{id}.waffle"))).unwrap();
    let mut b = ModelBuilder::kernel_v2();
    b.load(&waffle)
        .unwrap_or_else(|e| panic!("{id}: load failed: {e}"));
    let engine_errors: Vec<String> = b
        .engine_errors()
        .iter()
        .map(|(fid, m)| format!("{fid}: {m}"))
        .collect();
    let warnings = b.engine_warnings().to_vec();
    let tess_tol = (meta.scale * 0.01).clamp(1e-9, 0.1);
    let volume = b
        .tessellate_live_with_tol(tess_tol)
        .map(|meshes| meshes.iter().map(mesh_signed_volume).sum::<f64>())
        .unwrap_or(f64::NAN);
    (
        meta,
        Replay {
            volume,
            solids: b.distinct_solid_count(),
            engine_errors,
            warnings,
        },
    )
}

fn assert_case_correct(dir: &Path, id: &str) {
    let (meta, r) = replay(dir, id);
    assert!(
        r.engine_errors.is_empty(),
        "{id}: engine errors: {:?}",
        r.engine_errors
    );
    let bad: Vec<&String> = r
        .warnings
        .iter()
        .filter(|w| w.contains("Auto-union failed") || w.contains("operation not supported:"))
        .collect();
    assert!(
        bad.is_empty(),
        "{id}: NotSupported/auto-union warnings: {bad:?}"
    );
    let expected = meta
        .oracles
        .expected_volume
        .unwrap_or_else(|| panic!("{id}: gate cases must carry expected_volume"));
    let tol = meta.oracles.expected_volume_tol_rel.unwrap_or(1e-3);
    assert!(
        (r.volume - expected).abs() <= tol * expected.abs(),
        "{id}: volume {:.9e} vs expected {:.9e} (rel tol {tol:.1e})",
        r.volume,
        expected
    );
    let want_solids = meta.oracles.expected_solid_count.unwrap_or(1);
    assert_eq!(r.solids, want_solids, "{id}: solid count");
}

#[test]
fn complexity_generator_gate() {
    let dir = tempfile::tempdir().unwrap();
    let entries = generate_complexity_cases(dir.path());
    assert_eq!(entries.len(), 117);

    // Representative in-boundary cases, one+ per Group 1/3 family:
    // 1a genus, 1b chains, 1c non-convex, 1d near-degenerate,
    // 3a combine modes, 3b depth modes, 3c holed profiles, 3d regions.
    //
    // FINDING C0079-F1 (2026-07-05, this gate's first run): C0079
    // (multi-target Add with DISJOINT explicit targets [A, B]) silently
    // drops body B — no error, no warning, 1 solid, volume 1.625 = A∪tool
    // instead of 2.5 = A∪B∪tool. Real engine defect (silent material loss);
    // the case stays in the corpus as its repro (expected SUPPORTED_WRONG at
    // baseline) and is excluded from this generator gate per P9 — the gate
    // validates the GENERATOR, and blocking on an engine bug would invite
    // re-authoring the case to dodge it.
    let gate: &[&str] = &[
        "C0001", "C0005", "C0007", "C0009", // 1a
        "C0013", "C0016", // 1b
        "C0021", "C0026", // 1c
        "C0029", "C0032", "C0036", // 1d
        "C0080", "C0081", "C0083", "C0084", // 3a (C0079 = finding C0079-F1)
        "C0085", "C0087", "C0088", "C0089", // 3b
        "C0091", "C0093", "C0094", // 3c
        "C0097", "C0098", "C0100", // 3d
    ];
    let mut failures: BTreeMap<&str, String> = BTreeMap::new();
    for id in gate {
        if let Err(e) = std::panic::catch_unwind(|| assert_case_correct(dir.path(), id)) {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "non-string panic".into());
            failures.insert(id, msg);
        }
    }
    assert!(
        failures.is_empty(),
        "{} gate case(s) failed:\n{}",
        failures.len(),
        failures
            .iter()
            .map(|(k, v)| format!("  {k}: {v}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
