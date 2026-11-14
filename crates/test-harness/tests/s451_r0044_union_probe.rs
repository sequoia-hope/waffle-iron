//! The R0044 UNION on its own.
//!
//! R0044 is three ops — revolve(rectangle) ∪ revolve(gear), then a circle
//! cut. The categorized runner judges the finished document, so the union's
//! own geometry is never validated there. This probe truncates the document
//! to the first two ops and runs the runner's full oracle set on the union
//! result: mesh checks, Euler characteristic (against the
//! reference-adjudicated genus 1, one shell), volume magnitude, and the
//! independent volume-composition oracle.
//!
//! The test sets the triple-domain census knob itself (process-wide), so it
//! is `#[ignore]` and must run ALONE:
//! ```text
//! cargo test -p test-harness --test s451_r0044_union_probe --release \
//!   -- --ignored --nocapture
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use test_harness::assay::volume_oracle_doc::{
    evaluate_composition, oracle_tol, truncate_ops, CompositionVerdict,
};
use test_harness::oracle;
use test_harness::workflow::ModelBuilder;

const CORPUS: &str = "../../app/tests/cases/assay";

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS)
}

#[test]
#[ignore = "sets a census knob process-wide; run alone"]
fn r0044_union_only() {
    // Report each out-of-domain triple relocation to stderr.
    std::env::set_var("YANG_451_TRIPLE_DOMAIN", "census");
    let d = corpus_dir();
    let waffle: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(d.join("R0044.waffle")).unwrap()).unwrap();
    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(d.join("R0044.meta.json")).unwrap()).unwrap();
    let scale = meta["scale"].as_f64().unwrap();
    let doc = truncate_ops(&waffle, 2).expect("corpus document shape");
    let names: Vec<String> = doc
        .pointer("/tabs/0/kind/features/features")
        .and_then(serde_json::Value::as_array)
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap_or("?").to_string())
        .collect();
    eprintln!("[s451-probe] features={names:?}");
    assert_eq!(names.len(), 4, "two sketches + two revolves");

    let json = serde_json::to_string(&doc).unwrap();
    let mut b = ModelBuilder::kernel_v2();
    b.load(&json).expect("load");
    let errors: Vec<String> = b
        .engine_errors()
        .iter()
        .map(|(id, m)| format!("{id}: {m}"))
        .collect();
    let warnings: Vec<String> = b.engine_warnings().to_vec();
    let union_failures: Vec<&String> = warnings
        .iter()
        .filter(|w| w.contains("Auto-union failed"))
        .collect();
    eprintln!("[s451-probe] engine errors={errors:?}");
    eprintln!("[s451-probe] auto-union failures={union_failures:?}");
    assert!(errors.is_empty(), "engine errors: {errors:?}");
    assert!(
        union_failures.is_empty(),
        "union failed: {union_failures:?}"
    );

    let tess_tol = (scale * 0.01).clamp(1e-9, 0.1);
    let mesh = b
        .tessellate_last_with_tol(tess_tol)
        .expect("tessellate the union");
    let mut failures = Vec::new();
    for v in oracle::run_all_mesh_checks(&mesh) {
        eprintln!(
            "[s451-probe] {}: {} — {}",
            v.oracle_name,
            if v.passed { "ok" } else { "FAIL" },
            v.detail
        );
        if !v.passed {
            failures.push(format!("{}: {}", v.oracle_name, v.detail));
        }
    }
    let v = oracle::check_volume_magnitude(&mesh, scale);
    eprintln!(
        "[s451-probe] {}: {} — {}",
        v.oracle_name,
        if v.passed { "ok" } else { "FAIL" },
        v.detail
    );
    if !v.passed {
        failures.push(format!("{}: {}", v.oracle_name, v.detail));
    }
    // The union's topology is genus 1 — ADJUDICATED against the reference
    // (2026-09-03, `assay_topology_oracle` with `TOPO_SIDECAR=1`): the
    // Cherchi sidecar's union of the two operand tessellations is one closed
    // shell with V − E + F = 0, the same reading as the kernel's union
    // (28685 − 86052 + 57367 = 0). The case meta's `euler_target = 2` names
    // the three-op result, not this prefix.
    let v = oracle::check_mesh_euler_characteristic_with_shells(&mesh, 0, Some(1));
    eprintln!(
        "[s451-probe] {}: {} — {}",
        v.oracle_name,
        if v.passed { "ok" } else { "FAIL" },
        v.detail
    );
    if !v.passed {
        failures.push(format!("{}: {}", v.oracle_name, v.detail));
    }
    let verdict = evaluate_composition("R0044-union", &doc, &[false, false], scale, 64);
    eprintln!(
        "[s451-probe] composition (oracle tol {:.3e}): {verdict:?}",
        oracle_tol(scale)
    );
    match verdict {
        CompositionVerdict::Agree { .. } => {}
        CompositionVerdict::Flag { rel, band } => failures.push(format!(
            "volume_composition: rel={rel:.3e} > band={band:.3e}"
        )),
        CompositionVerdict::NotCovered(why) => {
            failures.push(format!("volume_composition not covered: {why}"))
        }
    }
    assert!(failures.is_empty(), "union oracles failed: {failures:?}");
}
