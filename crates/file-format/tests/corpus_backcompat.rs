//! Backward-compatibility pin (`specs/waffle_v4_document_model.md` §4
//! invariant 3, §5): every `.waffle` file in the repository — the 312-case
//! assay corpus, the GUI fixtures, the harness fixture and the root samples,
//! all v3 or older — loads through both APIs, migrates to v4, re-saves, and
//! reloads with its feature count intact. The three root samples that carry
//! region and STEP payloads must also rebuild identically before and after
//! migration.

use std::path::{Path, PathBuf};

use feature_engine::Engine;
use file_format::{load_document, load_project, save_document};
use waffle_types::kernel::MockKernel;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// Every TRACKED `.waffle` file, from `git ls-files` at the repo root, in
/// git's (sorted) order. Tracked, not "present": the root is where the app
/// saves a user's own documents (gitignored by `/*.waffle`), and a
/// directory walk used to pick those up — a saved assembly document then
/// failed this pin locally while CI stayed green.
fn all_corpus_files() -> Vec<PathBuf> {
    let root = repo_root();
    let out = std::process::Command::new("git")
        .args(["ls-files", "-z", "--", "*.waffle"])
        .current_dir(&root)
        .output()
        .expect("git ls-files runs at the repo root");
    assert!(
        out.status.success(),
        "git ls-files failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
        .split(|&b| b == 0)
        .filter(|rel| !rel.is_empty())
        .map(|rel| root.join(String::from_utf8_lossy(rel).as_ref()))
        .collect()
}

/// First path at which two JSON values differ, for readable failures.
fn first_diff(a: &serde_json::Value, b: &serde_json::Value, path: String) -> Option<String> {
    use serde_json::Value;
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                match (x.get(k), y.get(k)) {
                    (Some(_), None) => return Some(format!("{path}/{k}: only in first")),
                    (None, Some(_)) => return Some(format!("{path}/{k}: only in second")),
                    (Some(xa), Some(yb)) => {
                        if let Some(d) = first_diff(xa, yb, format!("{path}/{k}")) {
                            return Some(d);
                        }
                    }
                    (None, None) => {}
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("{path}: len {} vs {}", x.len(), y.len()));
            }
            x.iter()
                .zip(y)
                .enumerate()
                .find_map(|(i, (xa, yb))| first_diff(xa, yb, format!("{path}[{i}]")))
        }
        _ => (a != b).then(|| format!("{path}: {a} vs {b}")),
    }
}

#[test]
fn every_repo_waffle_file_loads_migrates_and_round_trips() {
    let files = all_corpus_files();
    assert!(
        files.len() > 300,
        "expected the assay corpus (312 files) plus samples, found {}",
        files.len()
    );
    let mut versions = std::collections::BTreeMap::new();
    for path in &files {
        let json = std::fs::read_to_string(path).unwrap();
        let raw: serde_json::Value = serde_json::from_str(&json).unwrap();
        *versions
            .entry(raw["version"].as_u64().unwrap_or(0))
            .or_insert(0usize) += 1;

        let (tree, _) =
            load_project(&json).unwrap_or_else(|e| panic!("{}: load_project: {e}", path.display()));
        let loaded = load_document(&json)
            .unwrap_or_else(|e| panic!("{}: load_document: {e}", path.display()));
        let doc = loaded.document;
        assert!(
            uuid::Uuid::parse_str(&doc.active_tab).is_ok(),
            "{}: tab ids are UUIDs after migration",
            path.display()
        );

        let v4 = save_document(&doc);
        let again = load_document(&v4)
            .unwrap_or_else(|e| panic!("{}: reload of migrated v4: {e}", path.display()));
        let (tree_again, _) = load_project(&v4).unwrap();
        assert_eq!(
            tree.features.len(),
            tree_again.features.len(),
            "{}",
            path.display()
        );
        assert_eq!(
            again.document.document.id,
            doc.document.id,
            "{}",
            path.display()
        );
        assert_eq!(again.document.sources, doc.sources, "{}", path.display());
        // Structurally stable second save (byte order of HashMap-backed
        // fields such as `solved_positions` is not fixed, and never was).
        let first: serde_json::Value = serde_json::from_str(&v4).unwrap();
        let second: serde_json::Value =
            serde_json::from_str(&save_document(&again.document)).unwrap();
        if let Some(d) = first_diff(&first, &second, String::new()) {
            panic!("{}: second save differs at {d}", path.display());
        }
    }
    eprintln!("corpus versions: {versions:?}");
    assert!(versions.contains_key(&3), "the corpus is v3: {versions:?}");
}

/// The root samples with region payloads and an embedded STEP body rebuild
/// to the same result set from the v3 file and from its v4 migration.
#[test]
fn root_samples_rebuild_identically_before_and_after_migration() {
    let root = repo_root();
    for name in ["err.waffle", "minihexa.waffle", "step_extrude.waffle"] {
        let path = root.join(name);
        let Ok(json) = std::fs::read_to_string(&path) else {
            eprintln!("skipping missing sample {name}");
            continue;
        };
        let rebuild = |json: &str| {
            let (tree, _) = load_project(json).unwrap();
            let mut kernel = MockKernel::new();
            let mut engine = Engine::new();
            engine.tree = tree;
            engine.rebuild_from_scratch(&mut kernel);
            let mut errors = engine.errors.clone();
            errors.sort();
            let mut results: Vec<(uuid::Uuid, usize)> = engine
                .feature_results
                .iter()
                .map(|(id, r)| (*id, r.outputs.len()))
                .collect();
            results.sort();
            (errors, results)
        };
        let before = rebuild(&json);
        let v4 = save_document(&load_document(&json).unwrap().document);
        let after = rebuild(&v4);
        assert_eq!(before, after, "{name}: rebuild parity across migration");
        if name == "minihexa.waffle" || name == "step_extrude.waffle" {
            let doc = load_document(&v4).unwrap().document;
            assert!(
                !doc.sources.is_empty(),
                "{name}: STEP payload lifted into sources"
            );
            assert!(
                v4.len() < json.len() * 11 / 10,
                "{name}: migration does not bloat the file"
            );
        }
    }
}
