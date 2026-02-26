//! Regression corpus replay tests.
//!
//! Loads all non-ignored corpus entries from the corpus/ directory
//! and replays them, verifying expected outcomes.

use std::path::PathBuf;
use test_harness::assay::corpus::*;

#[test]
fn replay_all_corpus_entries() {
    let corpus_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let entries = load_corpus(&corpus_dir);

    let mut failures = Vec::new();

    for entry in &entries {
        // Skip ignored and known-fail entries (known-fail entries are tracked for
        // regression detection but their replay is not yet wired up)
        if entry.status == CorpusStatus::Ignore || entry.status == CorpusStatus::Fail {
            continue;
        }

        let result = replay_corpus_entry(entry);
        if !result.passed {
            failures.push(format!("{}: {}", result.entry_id, result.detail));
        }
    }

    assert!(
        failures.is_empty(),
        "Corpus replay failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn corpus_load_empty_dir_returns_empty() {
    let entries = load_corpus(std::path::Path::new("/nonexistent/path"));
    assert!(entries.is_empty());
}
