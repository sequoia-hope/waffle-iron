//! Backward-compatibility pin (`specs/waffle_v4_document_model.md` §4
//! invariant 3, §5): every `.waffle` file in the repository — the 312-case
//! assay corpus, the GUI fixtures, the harness fixture, the shipped examples
//! and the root samples — loads through `load_document`, migrates to v4,
//! re-saves and reloads with its identity, sources and structure intact. The
//! three root samples that carry region and STEP payloads must also rebuild
//! identically before and after migration.
//!
//! `load_project`, the part-only convenience, is exercised too — but a
//! document whose ACTIVE tab is an Assembly is refused by it BY CONTRACT
//! ("cannot be opened as a part"), so for those the pin is that the refusal
//! is the same before and after migration. `app/static/examples/`'s gravel
//! bike is the repo's first such document; before it, every tracked file
//! opened to a Part and the distinction never came up.

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
    let mut assemblies = 0usize;
    for path in &files {
        let json = std::fs::read_to_string(path).unwrap();
        let raw: serde_json::Value = serde_json::from_str(&json).unwrap();
        *versions
            .entry(raw["version"].as_u64().unwrap_or(0))
            .or_insert(0usize) += 1;

        // Part-only by contract: `Ok` for a Part-active document, a typed
        // refusal for an Assembly-active one. Both must survive migration
        // unchanged, which is what the comparison below asserts.
        let as_part = load_project(&json).map(|(tree, _)| tree.features.len());
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
        let as_part_again = load_project(&v4).map(|(tree, _)| tree.features.len());
        if as_part.is_err() {
            assemblies += 1;
        }
        match (&as_part, &as_part_again) {
            (Ok(before), Ok(after)) => {
                assert_eq!(before, after, "{}: feature count", path.display())
            }
            (Err(before), Err(after)) => assert_eq!(
                before.to_string(),
                after.to_string(),
                "{}: load_project refuses the same way after migration",
                path.display()
            ),
            _ => panic!(
                "{}: load_project changed its mind across migration ({as_part:?} then {as_part_again:?})",
                path.display()
            ),
        }
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
    assert!(
        assemblies > 0,
        "an Assembly-active document is part of what this pin covers \
         (app/static/examples/gravel-bike-v2.waffle)"
    );
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
