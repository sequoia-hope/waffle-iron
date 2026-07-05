//! CLI binary for generating Assay v3 test corpus.
//!
//! Usage: assay_gen [--seed N] [--count N] [--output DIR] [--complexity-only]
//!
//! `--complexity-only` writes ONLY the C-series complexity cases
//! (C0001–C0100) and MERGES their entries into the existing manifest — the
//! legacy R/F files are not regenerated (regeneration mints fresh UUIDs and
//! would churn every committed file).

use std::path::PathBuf;
use test_harness::assay::gen::{generate_corpus, CorpusConfig, CorpusManifest};
use test_harness::assay::gen_complexity::generate_complexity_cases;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut seed: u64 = 42;
    let mut count: usize = 100;
    let mut output = PathBuf::from("app/tests/cases/assay");
    let mut complexity_only = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--seed" => {
                i += 1;
                seed = args[i].parse().expect("--seed requires a u64 value");
            }
            "--count" => {
                i += 1;
                count = args[i].parse().expect("--count requires a usize value");
            }
            "--output" => {
                i += 1;
                output = PathBuf::from(&args[i]);
            }
            "--complexity-only" => {
                complexity_only = true;
            }
            "--help" | "-h" => {
                println!("assay_gen — Generate Assay v3 test corpus");
                println!();
                println!("Options:");
                println!("  --seed N            Master seed (default: 42)");
                println!("  --count N           Number of cases (default: 100)");
                println!("  --output DIR        Output directory (default: app/tests/cases/assay)");
                println!(
                    "  --complexity-only   Write only C-series cases; merge into the manifest"
                );
                return;
            }
            other => {
                eprintln!("Unknown argument: {}", other);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    if complexity_only {
        let manifest_path = output.join("manifest.json");
        let mut manifest: CorpusManifest = serde_json::from_str(
            &std::fs::read_to_string(&manifest_path)
                .expect("--complexity-only requires an existing manifest.json"),
        )
        .expect("parse manifest.json");
        let entries = generate_complexity_cases(&output);
        let added = entries.len();
        manifest.cases.retain(|c| !c.id.starts_with('C'));
        manifest.cases.extend(entries);
        manifest.count = manifest.cases.len();
        std::fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&manifest).expect("serialize manifest"),
        )
        .expect("write manifest.json");
        println!(
            "Wrote {added} C-series cases; manifest now lists {} cases",
            manifest.count
        );
        return;
    }

    let config = CorpusConfig {
        master_seed: seed,
        case_count: count,
        output_dir: output,
    };

    println!(
        "Generating {} cases with seed {} into {}",
        config.case_count,
        config.master_seed,
        config.output_dir.display()
    );

    let stats = generate_corpus(&config);

    println!(
        "Generated {} cases ({} extrudes, {} revolves)",
        stats.count, stats.extrude_count, stats.revolve_count
    );
}
