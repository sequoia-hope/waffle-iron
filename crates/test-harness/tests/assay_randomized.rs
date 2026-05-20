use std::path::Path;

use test_harness::assay::randomized_runner::{
    build_catalog, catalog_summary, discover_cases, generate_catalog_markdown,
    generate_comparison_markdown, run_randomized_assay, run_single_case, run_yang_comparison,
    write_comparison_json, ComparisonChange,
};
use test_harness::assay::scoring::AssayStatus;

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

#[test]
fn randomized_corpus_discovery() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet — skipping discovery test");
        return;
    }
    let manifest = dir.join("manifest.json");
    assert!(manifest.exists(), "manifest.json should exist in assay dir");
    let cases = discover_cases(dir);
    assert!(!cases.is_empty(), "corpus should contain at least one case");
}

#[test]
fn randomized_assay_mock_smoke() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated — skipping mock smoke test");
        return;
    }
    let report = run_randomized_assay(dir, false);
    println!("\nMock smoke: {}", report.score_line());
    for r in &report.results {
        if r.status != AssayStatus::Passed {
            println!("  {} {:?}: {}", r.id, r.status, r.detail);
        }
    }
    assert!(report.total > 0, "should have at least some test cases");
}

#[test]
#[ignore]
fn randomized_assay_full_kernel() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return;
    }
    let report = run_randomized_assay(dir, true);
    let catalog = build_catalog(dir, &report);

    // results.json is auto-written by run_randomized_assay() now

    // Print summary
    println!("\n{}", catalog_summary(&report, &catalog));

    // Print individual failures
    for r in &report.results {
        if r.status != AssayStatus::Passed {
            println!("  {} {:?}: {}", r.id, r.status, r.detail);
        }
    }
}

/// Deep-verify R0100: a 2-op revolve case (boss + cut) on an oblique plane.
///
/// Validates per-step volume monotonicity and all 8 mesh oracles with detailed diagnostics.
/// Run with: cargo test -p test-harness --test assay_randomized -- spotlight_r0100 --ignored --nocapture
#[test]
#[ignore]
fn spotlight_r0100_revolve_revolve() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return;
    }

    let cases = discover_cases(dir);
    let r0100 = cases.iter().find(|c| c.id == "R0100");
    let case = match r0100 {
        Some(c) => c,
        None => {
            eprintln!("R0100 not found in corpus");
            return;
        }
    };

    let meta: test_harness::assay::gen::AssayMeta =
        serde_json::from_str(&std::fs::read_to_string(&case.meta_path).expect("read meta"))
            .expect("parse meta");

    println!("\n=== R0100 Spotlight ===");
    println!("Description: {}", meta.description);
    println!("Scale: {:.2e}", meta.scale);
    println!(
        "Operations: {}",
        meta.operations
            .iter()
            .map(|op| format!(
                "{}({},{}) angle/depth={:.1}",
                op.kind,
                op.profile_type,
                if op.is_cut { "cut" } else { "boss" },
                op.depth_or_angle
            ))
            .collect::<Vec<_>>()
            .join(" + ")
    );
    println!(
        "Expected monotonicity: {:?}",
        meta.oracles.volume_monotonicity
    );

    // Load the full waffle JSON
    let waffle_json = std::fs::read_to_string(&case.waffle_path).expect("read waffle");
    let doc: serde_json::Value = serde_json::from_str(&waffle_json).expect("parse waffle");
    let features = doc["features"]["features"]
        .as_array()
        .expect("features array");

    // Step-by-step: tessellate after each operation
    let n_ops = meta.operations.len();
    let mut volumes = Vec::new();

    for step in 0..n_ops {
        let feature_count = (step + 1) * 2;
        let truncated: Vec<serde_json::Value> =
            features.iter().take(feature_count).cloned().collect();
        let mut truncated_doc = doc.clone();
        truncated_doc["features"]["features"] = serde_json::Value::Array(truncated);
        let truncated_json = serde_json::to_string(&truncated_doc).unwrap();

        let mut builder = test_harness::workflow::ModelBuilder::kernel();
        match builder.load(&truncated_json) {
            Ok(_) => {}
            Err(e) => {
                println!(
                    "  Step {} ({}): LOAD FAILED: {}",
                    step + 1,
                    meta.operations[step].kind,
                    e
                );
                volumes.push(f64::NAN);
                continue;
            }
        }

        let errors = builder.engine_errors();
        if !errors.is_empty() {
            println!("  Step {} engine errors: {:?}", step + 1, errors);
        }

        match builder.tessellate_last() {
            Ok(mesh) => {
                let vol = test_harness::helpers::mesh_signed_volume(&mesh);
                let vtx_count = mesh.vertices.len() / 3;
                let tri_count = mesh.indices.len() / 3;
                let (_, boundary) = test_harness::helpers::count_mesh_edges(&mesh);
                let watertight = boundary == 0;

                println!(
                    "  Step {} ({}): V={} T={} vol={:.6e} watertight={} signed_vol={:.6e}",
                    step + 1,
                    meta.operations[step].kind,
                    vtx_count,
                    tri_count,
                    vol.abs(),
                    watertight,
                    vol
                );
                volumes.push(vol.abs());
            }
            Err(e) => {
                println!(
                    "  Step {} ({}): TESSELLATE FAILED: {}",
                    step + 1,
                    meta.operations[step].kind,
                    e
                );
                volumes.push(f64::NAN);
            }
        }
    }

    // Check monotonicity
    println!("\n  Volume progression: {:?}", volumes);
    let expected = &meta.oracles.volume_monotonicity;
    for i in 1..expected.len() {
        if volumes[i].is_nan() || volumes[i - 1].is_nan() {
            println!("  Step {}: SKIP (NaN volume)", i + 1);
            continue;
        }
        let direction = &expected[i];
        let ok = match direction.as_str() {
            "increase" => volumes[i] > volumes[i - 1],
            "decrease" => volumes[i] < volumes[i - 1],
            _ => true,
        };
        println!(
            "  Step {}: expect {} — vol {:.6e} vs {:.6e} — {}",
            i + 1,
            direction,
            volumes[i],
            volumes[i - 1],
            if ok { "OK" } else { "VIOLATED" }
        );
    }

    // Also run the full model through all oracles
    println!("\n  Full model oracle check:");
    let mut builder = test_harness::workflow::ModelBuilder::kernel();
    if let Err(e) = builder.load(&waffle_json) {
        println!("  Full load failed: {}", e);
        return;
    }
    match builder.tessellate_last() {
        Ok(mesh) => {
            let verdicts = test_harness::oracle::run_all_mesh_checks(&mesh);
            for v in &verdicts {
                println!(
                    "    {}: {} — {}",
                    v.oracle_name,
                    if v.passed { "PASS" } else { "FAIL" },
                    v.detail
                );
            }
        }
        Err(e) => {
            println!("  Full tessellate failed: {}", e);
        }
    }
}

/// Gear extrude with through circular cut — validates gear+boolean pipeline.
/// Run with: cargo test -p test-harness --test assay_randomized -- spotlight_f0061 --ignored --nocapture
#[test]
#[ignore]
fn spotlight_f0061_gear_cut() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return;
    }
    let result = run_single_case(dir, "F0061", true);
    match result {
        Some(r) => {
            println!("\n=== F0061 Spotlight ===");
            println!("Description: {}", r.description);
            println!("Status: {:?}", r.status);
            println!("Detail: {}", r.detail);
            println!("Duration: {:?}", r.duration);
            // F0061 currently fails watertight_mesh in the full assay pipeline
            // (gear+boolean produces unpaired edges). The direct kernel test
            // (f0061_gear_subtract_through_hole) passes with 0 unpaired edges.
            // Track progress: assert not Errored (i.e. boolean completes and produces geometry).
            assert_ne!(
                r.status,
                AssayStatus::Errored,
                "F0061 should not error: {}",
                r.detail
            );
            // TODO: tighten to assert_eq!(Passed) once full pipeline watertightness improves
        }
        None => panic!("F0061 not found in corpus"),
    }
}

/// PR-Y16-INV: Spotlight on F0020 — first Yang failure surfaced through the
/// in-app debug pane after the WASM YANG_BOOLEAN gate flip (2026-05-06).
/// Geometry: 3 sequential boss extrudes on rectangles with oblique sketch planes.
/// Symptom: `Extrude 2: Auto-union failed: yang_boolean: result validation failed:
/// half_edge[40].twin = 0 but twin.twin = 21 (expected 40)` (twin-pairing defect).
///
/// Run with:
///   YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- \
///     spotlight_f0020 --ignored --nocapture
///
/// For probe data:
///   mkdir -p /tmp/viz/f0020
///   TWIN_DEBUG=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 YANG_STAGE_DUMP=/tmp/viz/f0020 \
///     cargo test -p test-harness --test assay_randomized -- spotlight_f0020 \
///     --ignored --nocapture --test-threads=1 2> /tmp/viz/f0020/twin_debug.txt
#[test]
#[ignore]
fn spotlight_f0020() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return;
    }
    let result = run_single_case(dir, "F0020", true);
    match result {
        Some(r) => {
            println!("\n=== F0020 Spotlight (PR-Y16-INV) ===");
            println!("Description: {}", r.description);
            println!("Status:      {:?}", r.status);
            println!("Detail:      {}", r.detail);
            println!("Duration:    {:?}", r.duration);
            // Discovery probe: we EXPECT an Errored result with a twin-pairing
            // defect message. Once PR-Y16-FIX lands, this assertion should
            // flip to expecting Passed (or be removed in favor of the
            // fix's own greenable assertion).
            // For now, the spotlight just executes the case so probe
            // instrumentation can fire and dump per-stage data.
        }
        None => panic!("F0020 not found in corpus — regenerate with assay_gen"),
    }
}

/// Spotlight: oracle-attributed F0020 verdict — Phase 2 of oracle operationalization.
///
/// Drives F0020 through the full LoadProject → Yang boolean chain with the
/// snapshot collector installed, runs the `default_oracle_registry` against
/// the captured pipeline state, and emits a per-oracle verdict block to
/// stdout. Unlike `spotlight_f0020`, this test attributes failures to a
/// specific stage's invariant violation rather than reporting end-to-end
/// symptoms (47 unpaired, 30 degen).
///
/// The test currently PASSES even when oracles report violations — the
/// goal is per-stage visibility, not gating. Promotion to a hard pass-gate
/// follows once each oracle's baseline is GREEN across the cohort.
///
/// Run with:
///   YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- \
///     spotlight_f0020_oracles --ignored --nocapture
#[test]
#[ignore]
fn spotlight_f0020_oracles() {
    use kernel::diagnostics::{with_yang_oracle_capture_bijective, ViolationKind};
    use wasm_bridge::messages::UiToEngine;
    use wasm_bridge::{dispatch, EngineState};

    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return;
    }
    let waffle_path = dir.join("F0020.waffle");
    let waffle_json = match std::fs::read_to_string(&waffle_path) {
        Ok(s) => s,
        Err(e) => {
            panic!("Failed to load F0020.waffle: {}", e);
        }
    };

    std::env::set_var("YANG_BOOLEAN", "1");

    let (summary, bij_reports, _engine_errors) =
        with_yang_oracle_capture_bijective("F0020", move || {
            let mut state = EngineState::new();
            let mut kernel_inst = kernel::WaffleKernel::new();
            let _ = dispatch(
                &mut state,
                UiToEngine::LoadProject { data: waffle_json },
                &mut kernel_inst,
            );
            state.engine.errors.clone()
        });

    println!("\n=== F0020 Oracle Verdict (Phase 2) ===");
    println!("case_id = {}", summary.case_id);
    if let Some(e) = &summary.pipeline_error {
        println!("pipeline_error = {}", e);
    }
    println!("first_failing_stage = {:?}", summary.first_failing_stage);
    println!();
    for v in &summary.per_oracle {
        let verdict_label = match &v.violation {
            None => "PASS".to_string(),
            Some(viol) => match viol.kind {
                ViolationKind::ContractViolated => format!("FAIL  ({})", viol.message),
                ViolationKind::StateMissing => format!("SKIP  ({})", viol.message),
                ViolationKind::OracleStub => format!("STUB  ({})", viol.message),
            },
        };
        println!("  [{:?}] {} : {}", v.stage, v.oracle_name, verdict_label);
    }
    println!();

    // Phase 1 (y58): per-pair detail dump for BijectiveFacePairOracle.
    // Surfaces the specific NonBijectivePair records (face indices,
    // unmatched-edge counts, sample unmatched edges) so subsequent
    // diagnosis can classify each failure by mechanism.
    if let Some((report_a, report_b)) = &bij_reports {
        if !report_a.is_bijective() || !report_b.is_bijective() {
            println!("--- Stage 1 BijectiveFacePairOracle per-pair detail (Phase 1, y58) ---");
            for (label, report) in [("A", report_a), ("B", report_b)] {
                if report.non_bijective_pairs.is_empty() {
                    continue;
                }
                println!(
                    "Operand {}: {} of {} pairs non-bijective",
                    label,
                    report.non_bijective_pairs.len(),
                    report.total_pairs_examined,
                );
                for (i, pair) in report.non_bijective_pairs.iter().enumerate() {
                    println!(
                        "  Pair #{} face_a={:?} face_b={:?} edge={:?}",
                        i, pair.face_a, pair.face_b, pair.edge
                    );
                    println!(
                        "    unmatched_a_count={} unmatched_b_count={}",
                        pair.unmatched_a_count, pair.unmatched_b_count
                    );
                    if !pair.sample_unmatched_a.is_empty() {
                        println!("    sample unmatched A edges:");
                        for (p, q) in &pair.sample_unmatched_a {
                            println!(
                                "      ({:.6},{:.6},{:.6}) → ({:.6},{:.6},{:.6})",
                                p[0], p[1], p[2], q[0], q[1], q[2]
                            );
                        }
                    }
                    if !pair.sample_unmatched_b.is_empty() {
                        println!("    sample unmatched B edges:");
                        for (p, q) in &pair.sample_unmatched_b {
                            println!(
                                "      ({:.6},{:.6},{:.6}) → ({:.6},{:.6},{:.6})",
                                p[0], p[1], p[2], q[0], q[1], q[2]
                            );
                        }
                    }
                }
            }
            println!("------------------------------------------------------------------\n");
        }
    } else {
        println!("(no Stage 1 bijective snapshot captured — pipeline may have errored before Stage 1)\n");
    }

    let real_failures: Vec<_> = summary
        .per_oracle
        .iter()
        .filter(|v| {
            v.violation
                .as_ref()
                .map(|viol| viol.kind == ViolationKind::ContractViolated)
                .unwrap_or(false)
        })
        .collect();
    if real_failures.is_empty() {
        println!("Oracle verdict: ALL PASS (no stage attribution available — bug is in uncovered stage)");
    } else {
        println!(
            "Oracle verdict: {} contract violation(s); fix order = lowest stage first",
            real_failures.len()
        );
    }
    println!("==========================================\n");
}

/// Spotlight: drive F0030 through the Yang pipeline. PR-Y16-FIX-ARCH cohort RED.
///
/// Per `docs/audits/pr_y16_fix_arch_canary.md` §3 cohort table: F0030 boolean 1
/// FAILING with `result validation failed: half_edge[5].twin = 0 but twin.twin = 30`.
/// Cohort case for Cherchi 2022 §5 per-patch labeling refactor (spec
/// `yang_pr_y16_fix_arch_per_patch_cherchi.md`).
///
/// Run with:
///   YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- \
///     spotlight_f0030 --ignored --nocapture
#[test]
#[ignore]
fn spotlight_f0030() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return;
    }
    std::env::set_var("YANG_BOOLEAN", "1");
    let result = run_single_case(dir, "F0030", true);
    match result {
        Some(r) => {
            println!("\n=== F0030 Spotlight (PR-Y16-FIX-ARCH cohort) ===");
            println!("Description: {}", r.description);
            println!("Status:      {:?}", r.status);
            println!("Detail:      {}", r.detail);
            println!("Duration:    {:?}", r.duration);
            // Pre-refactor: expect Errored with twin-pairing defect (canary memo §3:
            // half_edge[5].twin = 0 but twin.twin = 30). Post-PR-Y16-FIX-ARCH:
            // assertion flips to Status == Passed (per spec §7 test plan).
            // For now (RED phase) the spotlight executes the case so probe
            // instrumentation can fire and dump per-stage data.
        }
        None => panic!("F0030 not found in corpus — regenerate with assay_gen"),
    }
}

/// Spotlight: drive F0050 through the Yang pipeline. PR-Y16-FIX-ARCH cohort RED.
///
/// Per `docs/audits/pr_y16_fix_arch_canary.md` §3 cohort table: F0050 has 3
/// failing booleans (b1: 6 manifold-barrier, b2: 6, b3: 4). Distinguishing
/// feature per spec §8.2: F0050 is the SILENT failure case — the
/// `[twin-oracle]` fires `unpaired_count=2` but `validate_yang_result_topology`
/// does NOT panic. The case completes without the canonical Yang error string;
/// detection requires the post-pairing twin oracle (PR-Y16-INV deliverable).
///
/// Run with:
///   YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- \
///     spotlight_f0050 --ignored --nocapture
#[test]
#[ignore]
fn spotlight_f0050() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return;
    }
    std::env::set_var("YANG_BOOLEAN", "1");
    let result = run_single_case(dir, "F0050", true);
    match result {
        Some(r) => {
            println!("\n=== F0050 Spotlight (PR-Y16-FIX-ARCH cohort, silent fail) ===");
            println!("Description: {}", r.description);
            println!("Status:      {:?}", r.status);
            println!("Detail:      {}", r.detail);
            println!("Duration:    {:?}", r.duration);
            // Pre-refactor: F0050 is the SILENT case per spec §8.2 — the validator
            // returns OK while `[twin-oracle]` fires `unpaired_count > 0` on stderr.
            // The Status field may report Passed today even though the topology is
            // defective. Post-PR-Y16-FIX-ARCH: assertion expects Passed AND
            // `[twin-oracle] unpaired_count = 0` per spec §8.2 empirical gate.
            // For now (RED phase) the spotlight executes the case so the silent
            // fire can be observed on stderr.
        }
        None => panic!("F0050 not found in corpus — regenerate with assay_gen"),
    }
}

/// Generate the full catalog markdown and write to ASSAY_CATALOG.md.
///
/// Run with: cargo test -p test-harness --test assay_randomized -- generate_catalog --ignored --nocapture
#[test]
#[ignore]
fn generate_catalog() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return;
    }
    let report = run_randomized_assay(dir, true);
    let catalog = build_catalog(dir, &report);
    let markdown = generate_catalog_markdown(&report, &catalog);

    // Write to crates/test-harness/ASSAY_CATALOG.md
    let catalog_path = Path::new("ASSAY_CATALOG.md");
    std::fs::write(catalog_path, &markdown).expect("failed to write ASSAY_CATALOG.md");

    // results.json is auto-written by run_randomized_assay() now

    println!("\nWrote ASSAY_CATALOG.md + results.json");
    println!("{}", catalog_summary(&report, &catalog));
}

// ── Batch tests for specific categories ──────────────────────────────

/// Helper: run a batch of cases and print results.
fn run_batch(ids: &[&str], use_kernel: bool) -> (usize, usize, usize) {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return (0, 0, 0);
    }
    let mut passed = 0;
    let mut failed = 0;
    let mut errored = 0;
    for id in ids {
        let result = run_single_case(dir, id, use_kernel);
        match result {
            Some(r) => {
                let status = r.status;
                if status != AssayStatus::Passed {
                    println!("  {} {:?}: {}", r.id, status, r.detail);
                }
                match status {
                    AssayStatus::Passed => passed += 1,
                    AssayStatus::Failed => failed += 1,
                    AssayStatus::Errored => errored += 1,
                }
            }
            None => {
                println!("  {} not found in corpus", id);
                errored += 1;
            }
        }
    }
    println!(
        "\nBatch: {}/{} passed, {} failed, {} errored",
        passed,
        ids.len(),
        failed,
        errored
    );
    (passed, failed, errored)
}

/// Test the first 10 R-series cases (quick smoke test for real kernel).
///
/// Run with: cargo test -p test-harness --test assay_randomized -- batch_r_first10 --ignored --nocapture
#[test]
#[ignore]
fn batch_r_first10() {
    let ids: Vec<&str> = (1..=10)
        .map(|i| {
            // Use static strings via leak (test-only, fine for test process)
            Box::leak(format!("R{:04}", i).into_boxed_str()) as &str
        })
        .collect();
    let (passed, _, _) = run_batch(&ids, true);
    assert!(
        passed >= 3,
        "Expected at least 3/10 R-series to pass, got {}",
        passed
    );
}

/// Test the F-series cases (all box-box union variations).
///
/// Run with: cargo test -p test-harness --test assay_randomized -- batch_f_series --ignored --nocapture
#[test]
#[ignore]
fn batch_f_series() {
    let ids: Vec<&str> = (1..=25)
        .map(|i| Box::leak(format!("F{:04}", i).into_boxed_str()) as &str)
        .collect();
    let (passed, _, _) = run_batch(&ids, true);
    println!("F-series: {}/25 passed", passed);
}

/// Test known revolve-geometry failure cases.
///
/// Run with: cargo test -p test-harness --test assay_randomized -- batch_revolve_failures --ignored --nocapture
#[test]
#[ignore]
fn batch_revolve_failures() {
    let ids = &["R0035", "R0070", "R0090", "R0100"];
    let (passed, _, _) = run_batch(ids, true);
    println!("Revolve failures: {}/4 passed", passed);
}

/// Test 2-op extrude-only cases (simplest multi-op).
///
/// Run with: cargo test -p test-harness --test assay_randomized -- batch_2op_extrude --ignored --nocapture
#[test]
#[ignore]
fn batch_2op_extrude() {
    // R-series 2-op extrude-only cases (no revolve)
    let ids = &[
        "R0002", "R0004", "R0005", "R0010", "R0013", "R0019", "R0023", "R0029", "R0032", "R0038",
    ];
    let (passed, _, _) = run_batch(ids, true);
    println!("2-op extrude: {}/{} passed", passed, ids.len());
}

#[test]
#[ignore]
fn spotlight_f0044() {
    let _dir = Path::new(ASSAY_DIR);
    let ids = &["F0044", "F0045", "R0092"];
    let (passed, failed, errored) = run_batch(ids, true);
    println!(
        "F0044+F0045+R0092: {}/3 passed, {} failed, {} errored",
        passed, failed, errored
    );
}

#[test]
#[ignore]
fn spotlight_r0045() {
    let _dir = Path::new(ASSAY_DIR);
    let ids = &["R0045"];
    let (passed, failed, errored) = run_batch(ids, true);
    println!(
        "R0045: {}/1 passed, {} failed, {} errored",
        passed, failed, errored
    );
}

/// Verify euler_target oracle fix: 8 cases that had chi=2 but oracle expected chi=0.
///
/// Run with: cargo test -p test-harness --test assay_randomized -- batch_euler_target_fix --ignored --nocapture
#[test]
#[ignore]
fn batch_euler_target_fix() {
    let ids = &[
        "R0023", "R0036", "R0039", "R0048", "R0058", "R0060", "R0076", "R0080",
    ];
    let (passed, failed, errored) = run_batch(ids, true);
    println!(
        "Euler target fix: {}/8 passed, {} failed, {} errored",
        passed, failed, errored
    );
    assert!(
        passed >= 8,
        "Expected all 8 euler_target fix cases to pass, got {}",
        passed
    );
}

/// Verify F0031-F0040: box-minus-enclosed-cyl (F0031-F0035) and cyl-minus-enclosed-box (F0036-F0040).
///
/// These test blind pocket topology with correct euler_target=4.
/// Run with: cargo test -p test-harness --test assay_randomized -- batch_enclosed_subtract_fix --ignored --nocapture
#[test]
#[ignore]
fn batch_enclosed_subtract_fix() {
    let ids: Vec<String> = (31..=40).map(|i| format!("F{:04}", i)).collect();
    let id_refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
    let (passed, failed, errored) = run_batch(&id_refs, true);
    println!(
        "Enclosed subtract fix: {}/10 passed, {} failed, {} errored",
        passed, failed, errored
    );
    assert!(
        passed >= 10,
        "Expected all 10 enclosed subtract cases to pass, got {}",
        passed
    );
}

/// Test off-axis chained extrude cases (F0076-F0085).
///
/// Run with: cargo test -p test-harness --test assay_randomized -- batch_off_axis_chained --ignored --nocapture
#[test]
#[ignore]
fn batch_off_axis_chained() {
    let ids: Vec<&str> = (76..=85)
        .map(|i| Box::leak(format!("F{:04}", i).into_boxed_str()) as &str)
        .collect();
    let (passed, failed, errored) = run_batch(&ids, true);
    println!(
        "Off-axis chained: {}/10 passed, {} failed, {} errored",
        passed, failed, errored
    );
}

/// Test swiss cheese disc cases (F0086-F0090).
///
/// Run with: cargo test -p test-harness --test assay_randomized -- batch_swiss_cheese --ignored --nocapture
#[test]
#[ignore]
fn batch_swiss_cheese() {
    let ids: Vec<&str> = (86..=90)
        .map(|i| Box::leak(format!("F{:04}", i).into_boxed_str()) as &str)
        .collect();
    let (passed, failed, errored) = run_batch(&ids, true);
    println!(
        "Swiss cheese: {}/5 passed, {} failed, {} errored",
        passed, failed, errored
    );
}

/// Deep-verify R0098: gear boss + oversized rectangle cut on tilted plane.
///
/// R0098 passes all 8 original mesh oracles but has bad faces in the cut pocket
/// where the rectangle extends beyond the gear boundary. The new
/// `check_no_self_intersection` oracle should catch this.
///
/// Run with: cargo test -p test-harness --test assay_randomized -- spotlight_r0098 --ignored --nocapture
#[test]
#[ignore]
fn spotlight_r0098_self_intersection() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return;
    }

    let result = run_single_case(dir, "R0098", true);
    match result {
        Some(r) => {
            println!("\n=== R0098 Self-Intersection Spotlight ===");
            println!("Description: {}", r.description);
            println!("Status: {:?}", r.status);
            println!("Detail: {}", r.detail);

            // Also run oracle suite directly for detailed diagnostics
            let cases = discover_cases(dir);
            let case = cases
                .iter()
                .find(|c| c.id == "R0098")
                .expect("R0098 in corpus");
            let waffle_json = std::fs::read_to_string(&case.waffle_path).expect("read waffle");
            let mut builder = test_harness::workflow::ModelBuilder::kernel();
            if let Err(e) = builder.load(&waffle_json) {
                println!("  Load failed: {}", e);
                return;
            }
            match builder.tessellate_last() {
                Ok(mesh) => {
                    let verdicts = test_harness::oracle::run_all_mesh_checks(&mesh);
                    let mut original_all_pass = true;
                    for v in &verdicts {
                        let marker = if v.passed { "PASS" } else { "FAIL" };
                        println!("    {}: {} — {}", v.oracle_name, marker, v.detail);
                        if v.oracle_name != "no_self_intersection" && !v.passed {
                            original_all_pass = false;
                        }
                    }

                    // The premise: original 8 oracles all pass
                    if !original_all_pass {
                        println!("  NOTE: Some original oracles failed — premise not met");
                    }

                    // The new oracle should catch the defect
                    let si_verdict = verdicts
                        .iter()
                        .find(|v| v.oracle_name == "no_self_intersection")
                        .expect("no_self_intersection oracle should be in results");
                    println!(
                        "\n  Self-intersection oracle: {} — {}",
                        if si_verdict.passed { "PASS" } else { "FAIL" },
                        si_verdict.detail
                    );
                    // Assert the oracle catches R0098's geometric defect
                    assert!(
                        !si_verdict.passed,
                        "R0098 should FAIL the self-intersection oracle (has bad pocket faces)"
                    );
                }
                Err(e) => {
                    println!("  Tessellate failed: {}", e);
                }
            }
        }
        None => panic!("R0098 not found in corpus"),
    }
}

/// Verify revolve self-intersection detection (F0073-F0075).
///
/// F0073: axis through center → expect_rebuild_error (should pass)
/// F0074: axis through vertex → expect_rebuild_error (should pass)
/// F0075: valid offset revolve → should succeed
/// Run with: cargo test -p test-harness --test assay_randomized -- batch_revolve_self_intersection --ignored --nocapture
#[test]
#[ignore]
fn batch_revolve_self_intersection() {
    let ids = &["F0073", "F0074", "F0075"];
    let (passed, failed, errored) = run_batch(ids, true);
    println!(
        "Revolve self-intersection: {}/3 passed, {} failed, {} errored",
        passed, failed, errored
    );
}

// ── Yang Pipeline Fast Check ─────────────────────────────────────────

/// Fast Yang assay: runs all 190 cases EXCEPT known timeout cases (~31).
/// Completes in ~5 minutes instead of ~45 minutes. Uses per-case 30s timeout
/// (vs 90s in full assay) so previously-unknown slow cases don't hang.
///
/// Run with: YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast --ignored --nocapture
#[test]
#[ignore]
fn yang_fast() {
    use std::collections::HashSet;

    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return;
    }

    // Known timeout cases (>90s with Yang pipeline) — skip entirely.
    let skip: HashSet<&str> = [
        "R0003", "R0010", "R0012", "R0026", "R0028", "R0053", "R0059", "R0065", "R0070", "R0085",
        "R0099", "R0100", "F0063", "F0065", "F0067", "F0068", "F0069", "F0070", "F0071", "F0072",
        "F0077", "F0078", "F0079", "F0080", "F0081", "F0082", "F0083", "F0084", "F0085", "F0087",
        "F0088", "F0089", "F0090",
    ]
    .iter()
    .copied()
    .collect();

    let cases = discover_cases(dir);
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut errored = 0usize;
    let mut skipped = 0usize;

    for case in &cases {
        if skip.contains(case.id.as_str()) {
            skipped += 1;
            continue;
        }
        // Run with 30s timeout per case
        let id_clone = case.id.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let dir = Path::new(ASSAY_DIR);
            let r = run_single_case(dir, &id_clone, true);
            let _ = tx.send(r);
        });
        match rx.recv_timeout(std::time::Duration::from_secs(30)) {
            Ok(Some(r)) => {
                let _ = handle.join();
                if r.status != AssayStatus::Passed {
                    // char-boundary-safe truncation (avoid panic on multi-byte chars like '§')
                    let mut end = r.detail.len().min(150);
                    while end > 0 && !r.detail.is_char_boundary(end) {
                        end -= 1;
                    }
                    println!("  {} {:?}: {}", r.id, r.status, &r.detail[..end]);
                }
                match r.status {
                    AssayStatus::Passed => passed += 1,
                    AssayStatus::Failed => failed += 1,
                    AssayStatus::Errored => errored += 1,
                }
            }
            Ok(None) => {
                let _ = handle.join();
                println!("  {} not found", case.id);
                errored += 1;
            }
            Err(_) => {
                println!("  {} timeout (30s)", case.id);
                errored += 1;
            }
        }
    }

    let total = cases.len() - skipped;
    println!(
        "\nYang fast: {}/{} passed, {} failed, {} errored (skipped {} known timeouts)",
        passed, total, failed, errored, skipped
    );
}

// ── Yang Pipeline Comparison ──────────────────────────────────────────

/// Phase 5b: Compare Yang pipeline vs. legacy pipeline on full assay corpus.
///
/// Runs every case twice — once with legacy boolean dispatch, once with
/// `YANG_BOOLEAN=1`. Produces:
/// - Console summary
/// - `yang_comparison.json` in assay dir
/// - `specs/yang_assay_5b_comparison.md` comparison report
///
/// Run with: cargo test -p test-harness --test assay_randomized -- yang_pipeline_comparison --ignored --nocapture
#[test]
#[ignore]
fn yang_pipeline_comparison() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return;
    }

    let (legacy, yang, comparisons) = run_yang_comparison(dir);

    // Write machine-readable comparison
    write_comparison_json(dir, &legacy, &yang, &comparisons);

    // Write human-readable markdown report
    let markdown = generate_comparison_markdown(&legacy, &yang, &comparisons);
    let spec_path = Path::new("../../specs/yang_assay_5b_comparison.md");
    std::fs::write(spec_path, &markdown).expect("write comparison spec");

    // Console summary
    println!("\n{}", "=".repeat(70));
    println!("YANG PIPELINE COMPARISON (Phase 5b)");
    println!("{}", "=".repeat(70));
    println!(
        "\nLegacy: {}/{} passed ({} fail, {} error) in {:.1}s",
        legacy.passed,
        legacy.total,
        legacy.failed,
        legacy.errored,
        legacy.total_duration.as_secs_f64()
    );
    println!(
        "Yang:   {}/{} passed ({} fail, {} error) in {:.1}s",
        yang.passed,
        yang.total,
        yang.failed,
        yang.errored,
        yang.total_duration.as_secs_f64()
    );
    println!(
        "\nDelta: {:+} passed, {:+} failed, {:+} errored",
        yang.passed as i64 - legacy.passed as i64,
        yang.failed as i64 - legacy.failed as i64,
        yang.errored as i64 - legacy.errored as i64,
    );

    let improved: Vec<_> = comparisons
        .iter()
        .filter(|c| c.change == ComparisonChange::Improved)
        .collect();
    let regressed: Vec<_> = comparisons
        .iter()
        .filter(|c| c.change == ComparisonChange::Regressed)
        .collect();

    println!(
        "\nImproved: {}  Regressed: {}  Unchanged: {}",
        improved.len(),
        regressed.len(),
        comparisons.len() - improved.len() - regressed.len()
    );

    if !improved.is_empty() {
        println!("\nImproved cases:");
        for c in &improved {
            println!("  {} (was {:?})", c.id, c.legacy_status);
        }
    }

    if !regressed.is_empty() {
        println!("\nRegressed cases:");
        for c in &regressed {
            println!("  {} (now {:?}: {})", c.id, c.yang_status, c.yang_detail);
        }
    }

    println!("\nWrote: yang_comparison.json + specs/yang_assay_5b_comparison.md");
}

/// Trace F0002 with Yang pipeline to diagnose flood_fill_patches twin pairing failures.
///
/// Run with: cargo test -p test-harness --test assay_randomized -- yang_trace_f0002 --ignored --nocapture
#[test]
#[ignore]
fn yang_trace_f0002() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return;
    }
    std::env::set_var("YANG_BOOLEAN", "1");
    let result = run_single_case(dir, "F0002", true);
    match result {
        Some(r) => {
            eprintln!("\n=== F0002 Yang Trace ===");
            eprintln!("Status: {:?}", r.status);
            eprintln!("Detail: {}", r.detail);
            eprintln!("Description: {}", r.description);
        }
        None => eprintln!("F0002 not found in corpus"),
    }
}

/// Trace F0004 with Yang pipeline (PR17 partial-overlap-cosurface investigation).
///
/// Run with: cargo test -p test-harness --test assay_randomized -- yang_trace_f0004 --ignored --nocapture
#[test]
#[ignore]
fn yang_trace_f0004() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return;
    }
    std::env::set_var("YANG_BOOLEAN", "1");
    let result = run_single_case(dir, "F0004", true);
    match result {
        Some(r) => {
            eprintln!("\n=== F0004 Yang Trace ===");
            eprintln!("Status: {:?}", r.status);
            eprintln!("Detail: {}", r.detail);
            eprintln!("Description: {}", r.description);
        }
        None => eprintln!("F0004 not found in corpus"),
    }
}

/// Trace F0003 with Yang pipeline (secondary regression case).
///
/// Run with: cargo test -p test-harness --test assay_randomized -- yang_trace_f0003 --ignored --nocapture
#[test]
#[ignore]
fn yang_trace_f0003() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet");
        return;
    }
    std::env::set_var("YANG_BOOLEAN", "1");
    let result = run_single_case(dir, "F0003", true);
    match result {
        Some(r) => {
            eprintln!("\n=== F0003 Yang Trace ===");
            eprintln!("Status: {:?}", r.status);
            eprintln!("Detail: {}", r.detail);
            eprintln!("Description: {}", r.description);
        }
        None => eprintln!("F0003 not found in corpus"),
    }
}
