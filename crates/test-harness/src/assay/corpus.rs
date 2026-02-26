//! Regression corpus management for boolean test cases.
//!
//! Stores known-interesting scenarios as JSON files for replay testing.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Status of a corpus entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CorpusStatus {
    /// Test is expected to pass.
    Pass,
    /// Test is expected to fail (known bug).
    Fail,
    /// Test is skipped (too slow, flaky, etc).
    Ignore,
}

/// A single regression corpus entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorpusEntry {
    /// Unique bug/scenario identifier (e.g. "K8", "HP-1", "q4").
    pub id: String,
    /// Human-readable description.
    pub description: String,
    /// Expected status.
    pub status: CorpusStatus,
    /// Scenario specification as JSON value (flexible schema).
    pub scenario: serde_json::Value,
    /// Expected topology counts if known: (V, E, F).
    pub expected_topology: Option<(usize, usize, usize)>,
    /// Expected volume if known.
    pub expected_volume: Option<f64>,
}

/// Result of replaying a corpus entry.
#[derive(Debug)]
pub struct ReplayResult {
    pub entry_id: String,
    pub passed: bool,
    pub actual_topology: Option<(usize, usize, usize)>,
    pub actual_volume: Option<f64>,
    pub detail: String,
}

/// Load all corpus entries from a directory.
///
/// Each .json file in the directory is parsed as a `CorpusEntry`.
pub fn load_corpus(dir: &Path) -> Vec<CorpusEntry> {
    let mut entries = Vec::new();

    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return entries,
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(corpus_entry) = serde_json::from_str::<CorpusEntry>(&contents) {
                    entries.push(corpus_entry);
                }
            }
        }
    }

    entries.sort_by(|a, b| a.id.cmp(&b.id));
    entries
}

/// Save a corpus entry to a directory.
pub fn save_corpus_entry(dir: &Path, entry: &CorpusEntry) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create dir: {}", e))?;

    let filename = format!("{}.json", entry.id.to_lowercase().replace(' ', "_"));
    let path = dir.join(filename);
    let json =
        serde_json::to_string_pretty(entry).map_err(|e| format!("Serialization error: {}", e))?;

    std::fs::write(&path, json).map_err(|e| format!("Write error: {}", e))
}

/// Replay a corpus entry (stub — actual execution depends on scenario schema).
///
/// The scenario JSON must contain fields matching the strategy types.
/// This function returns a basic result; callers should extend with
/// domain-specific replay logic.
pub fn replay_corpus_entry(entry: &CorpusEntry) -> ReplayResult {
    if entry.status == CorpusStatus::Ignore {
        return ReplayResult {
            entry_id: entry.id.clone(),
            passed: true,
            actual_topology: None,
            actual_volume: None,
            detail: "Skipped (ignored)".to_string(),
        };
    }

    // Basic replay: try to extract scenario params and build
    // This is a placeholder — real replay would deserialize into BooleanScenario
    ReplayResult {
        entry_id: entry.id.clone(),
        passed: entry.status == CorpusStatus::Pass,
        actual_topology: None,
        actual_volume: None,
        detail: format!(
            "Corpus entry '{}': status={:?} (replay not yet wired)",
            entry.id, entry.status
        ),
    }
}
