//! CLI binary for generating Assay v3 test corpus.
//!
//! Usage: assay_gen [--seed N] [--count N] [--output DIR]

use std::path::PathBuf;
use test_harness::assay::gen::{generate_corpus, CorpusConfig};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut seed: u64 = 42;
    let mut count: usize = 100;
    let mut output = PathBuf::from("app/tests/cases/assay");

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
            "--help" | "-h" => {
                println!("assay_gen — Generate Assay v3 test corpus");
                println!();
                println!("Options:");
                println!("  --seed N     Master seed (default: 42)");
                println!("  --count N    Number of cases (default: 100)");
                println!("  --output DIR Output directory (default: app/tests/cases/assay)");
                return;
            }
            other => {
                eprintln!("Unknown argument: {}", other);
                std::process::exit(1);
            }
        }
        i += 1;
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
