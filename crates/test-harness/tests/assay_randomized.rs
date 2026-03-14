use std::path::Path;

use test_harness::assay::randomized_runner::{
    build_catalog, catalog_summary, discover_cases, generate_catalog_markdown,
    run_randomized_assay, write_results_json,
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

    // Write results.json to assay dir for GUI consumption
    write_results_json(dir, &catalog);

    println!("\nWrote ASSAY_CATALOG.md + results.json");
    println!("{}", catalog_summary(&report, &catalog));
}
