//! `.waffle` v4 JSON Schema golden (`specs/waffle_v4_document_model.md` §5):
//! the schema generated from the Rust types must equal the committed
//! `docs/schema/waffle-v5.schema.json` (regenerate with `UPDATE_SCHEMA=1`),
//! and every `.waffle` file in the repository — migrated to v4 by the loader
//! and re-saved — must validate against it, so the schema is neither stale
//! nor stricter than the writer.
#![cfg(feature = "json-schema")]

use std::path::{Path, PathBuf};

use file_format::schema::waffle_file_schema;
use file_format::{load_document, save_document, WaffleDocument};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// A corpus document's text, inflating a gzipped example (see
/// `waffle_files_in`).
fn read_document(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    if bytes.starts_with(&[0x1f, 0x8b]) {
        use std::io::Read;
        let mut out = String::new();
        flate2::read::GzDecoder::new(&bytes[..])
            .read_to_string(&mut out)
            .unwrap_or_else(|e| panic!("{}: gunzip: {e}", path.display()));
        return out;
    }
    String::from_utf8(bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn golden_path() -> PathBuf {
    repo_root().join("docs/schema/waffle-v5.schema.json")
}

#[test]
fn schema_is_current() {
    let schema = waffle_file_schema();
    let pretty = serde_json::to_string_pretty(&schema).unwrap() + "\n";
    let path = golden_path();
    if std::env::var("UPDATE_SCHEMA").is_ok() {
        std::fs::write(&path, &pretty).unwrap();
        eprintln!("wrote {}", path.display());
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}\nregenerate with: UPDATE_SCHEMA=1 cargo test -p file-format --features json-schema --test schema_golden",
            path.display()
        )
    });
    let committed: serde_json::Value = serde_json::from_str(&committed).unwrap();
    assert_eq!(
        committed, schema,
        "docs/schema/waffle-v5.schema.json is stale; regenerate with UPDATE_SCHEMA=1 cargo test -p file-format --features json-schema --test schema_golden"
    );
}

#[test]
fn schema_has_the_v4_shape() {
    let schema = waffle_file_schema();
    assert_eq!(schema["title"], "Waffle Iron .waffle document (format v5)");
    let required = schema["required"].as_array().unwrap();
    for key in [
        "format",
        "version",
        "min_reader_version",
        "document",
        "tabs",
        "active_tab",
    ] {
        assert!(required.iter().any(|r| r == key), "{key} required");
    }
    let defs = schema["$defs"].as_object().expect("definitions");
    for name in [
        "DocumentMetadata",
        "Tab",
        "TabKind",
        "SourceEntry",
        "SourceKind",
        "Locator",
        "GitRef",
        "GitHost",
        "FeatureTree",
        "Feature",
        "Operation",
        "GeomRef",
        "Sketch",
        "SketchEntity",
        "SketchConstraint",
        "Region",
        "Provenance",
    ] {
        assert!(
            defs.contains_key(name),
            "$defs has {name}: {:?}",
            defs.keys().collect::<Vec<_>>()
        );
    }
}

/// **The pid drift oracle.** No `pid` / `root_pid` may be an `integer` in any
/// committed schema, in the live schema, or in anything generated from one.
///
/// A persistent id is a content-seeded `u64` and a JSON number in JavaScript
/// is an `f64`, so an id above `2^53` reaches the page as a DIFFERENT entity
/// (`waffle_types::pid_str`; the arithmetic is pinned in that module's
/// tests). The rule is that every pid crosses as a decimal string — and a
/// rule nothing checks is a rule that holds until the next field is added.
/// This is the check: a new `pid: u64` written without
/// `#[serde(with = "waffle_types::pid_str")]` lands in these schemas as an
/// `integer` and fails here, naming the path.
///
/// It reads the GOLDENS rather than only the live schema on purpose: the
/// relay manifest and `engineSchemas.generated.js` are generated from
/// `waffle-v5.schema.json`, so the golden is what the JS side actually gets.
/// The sibling `schema_is_current` keeps the golden equal to the live schema,
/// which is what makes scanning the golden a scan of the types.
#[test]
fn no_pid_field_crosses_as_a_number() {
    fn scan(node: &serde_json::Value, path: &str, out: &mut Vec<String>) {
        match node {
            serde_json::Value::Object(map) => {
                if let Some(props) = map.get("properties").and_then(|p| p.as_object()) {
                    for name in ["pid", "root_pid"] {
                        let Some(field) = props.get(name) else {
                            continue;
                        };
                        // `type` may be a string or a list (`["string",
                        // "null"]` for an optional pid).
                        let types: Vec<&str> = match field.get("type") {
                            Some(serde_json::Value::String(s)) => vec![s.as_str()],
                            Some(serde_json::Value::Array(a)) => {
                                a.iter().filter_map(|t| t.as_str()).collect()
                            }
                            _ => Vec::new(),
                        };
                        if types.iter().any(|t| *t == "integer" || *t == "number") {
                            out.push(format!("{path}/properties/{name}: {types:?}"));
                        }
                    }
                }
                for (k, v) in map {
                    scan(v, &format!("{path}/{k}"), out);
                }
            }
            serde_json::Value::Array(items) => {
                for (i, v) in items.iter().enumerate() {
                    scan(v, &format!("{path}/{i}"), out);
                }
            }
            _ => {}
        }
    }

    let dir = repo_root().join("docs/schema");
    let mut goldens: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("docs/schema")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    goldens.sort();
    assert!(
        goldens.len() >= 3,
        "expected the committed schema goldens, found {goldens:?}"
    );

    // The relay's bundled tool manifest too. It is generated from the golden
    // above AND from the hand-written JS tool schemas
    // (`app/src/lib/agent/tools/*.js`), which nothing in Rust can see — so
    // this is the one place a JS-side `pid: { type: 'number' }` is caught.
    // `entity_list`'s was exactly that, and it is what an MCP client reads to
    // decide what a pid is.
    goldens.push(repo_root().join("relay/src/waffle_mcp_relay/agent-tools.manifest.json"));

    let mut offenders = Vec::new();
    for path in &goldens {
        let schema: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap())
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        scan(&schema, &name, &mut offenders);
    }
    // The live schema too, so a stale golden cannot hide a new field.
    scan(&waffle_file_schema(), "<live>", &mut offenders);

    assert!(
        offenders.is_empty(),
        "a persistent id crosses as a JSON NUMBER, which JavaScript rounds above 2^53 \
         (see waffle_types::pid_str): annotate the field with \
         `#[serde(with = \"waffle_types::pid_str\")]` (or `::option` for an `Option<u64>`) plus \
         `#[cfg_attr(feature = \"json-schema\", schemars(with = \"String\"))]`, then regenerate \
         the goldens with UPDATE_SCHEMA=1.\noffenders: {offenders:#?}"
    );

    // And the rule is really being tested: the scanner finds a planted one.
    let mut planted = Vec::new();
    scan(
        &serde_json::json!({
            "$defs": { "Thing": { "properties": { "pid": { "type": "integer" } } } }
        }),
        "<planted>",
        &mut planted,
    );
    assert_eq!(planted.len(), 1, "the scanner detects a numeric pid");
}

fn waffle_files_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // `.waffle.gz` too: the shipped examples are stored gzipped
            // (`docs/notes/eiffel/FEATURE_NOTES.md` §6) and must still be
            // held to the schema.
            name.ends_with(".waffle") || name.ends_with(".waffle.gz")
        })
        .collect();
    files.sort();
    files
}

#[test]
fn every_repo_waffle_file_validates_after_migration() {
    let validator = jsonschema::validator_for(&waffle_file_schema()).expect("valid schema");
    let root = repo_root();
    let mut files = Vec::new();
    for dir in [
        "app/tests/cases/assay",
        "app/tests/gui/fixtures",
        "crates/test-harness/tests/fixtures",
        ".",
    ] {
        files.extend(waffle_files_in(&root.join(dir)));
    }
    assert!(files.len() > 300, "found {} files", files.len());

    for path in &files {
        let json = read_document(path);
        let doc = load_document(&json)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
            .document;
        let v4: serde_json::Value = serde_json::from_str(&save_document(&doc)).unwrap();
        let errors: Vec<String> = validator
            .iter_errors(&v4)
            .map(|e| format!("{} at {}", e, e.instance_path()))
            .take(5)
            .collect();
        assert!(errors.is_empty(), "{}: {errors:#?}", path.display());
    }

    // A fresh document and a document with every locator kind validate too.
    let mut doc = WaffleDocument::new("Fresh");
    use file_format::{Locator, SourceEntry, SourceKind};
    doc.sources.push(SourceEntry::linked(
        "a.waffle",
        SourceKind::Waffle,
        Locator::git_branch("https://github.com/a/b", "a.waffle", "main"),
    ));
    doc.sources.push(SourceEntry::linked(
        "b.waffle",
        SourceKind::Waffle,
        Locator::git_commit("https://gitlab.com/a/b", "b.waffle", &"0".repeat(40)),
    ));
    doc.sources.push(SourceEntry::linked(
        "c",
        SourceKind::Step,
        Locator::Relative {
            path: "../c.step".into(),
        },
    ));
    doc.sources.push(SourceEntry::linked(
        "d",
        SourceKind::KicadPcb,
        Locator::Url {
            url: "https://x/y".into(),
        },
    ));
    doc.sources.push(SourceEntry::linked(
        "e",
        SourceKind::Mesh,
        Locator::Local {
            provider: "local".into(),
            doc_id: "AbCdEfGh".into(),
        },
    ));
    doc.sources.push(SourceEntry::embedded(
        "f.step",
        SourceKind::Step,
        "ISO-10303-21;",
    ));
    let v4: serde_json::Value = serde_json::from_str(&save_document(&doc)).unwrap();
    let errors: Vec<String> = validator.iter_errors(&v4).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{errors:#?}");

    // And the schema REJECTS what the loader rejects: a malformed Part tab
    // and a source without a locator.
    let mut bad = v4.clone();
    bad["tabs"][0]["kind"] = serde_json::json!({ "type": "Part", "features": 42 });
    assert!(
        !validator.is_valid(&bad),
        "malformed Part must not validate"
    );
    let mut bad = v4.clone();
    bad["sources"][0].as_object_mut().unwrap().remove("locator");
    assert!(
        !validator.is_valid(&bad),
        "source without locator must not validate"
    );
    // …while an unknown tab kind (opaque) DOES validate. The stand-in was
    // `Drawing` until D4a made it a real kind; `Schematic` is the next one
    // nothing implements, and the mechanism under test is the opaque branch,
    // not the name.
    let mut future = v4.clone();
    future["tabs"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": "sch", "name": "Schematic 1", "kind": { "type": "Schematic", "nets": [] }
        }));
    let errors: Vec<String> = validator
        .iter_errors(&future)
        .map(|e| e.to_string())
        .collect();
    assert!(
        errors.is_empty(),
        "opaque tab kind must validate: {errors:#?}"
    );
    // A Drawing tab (D4a) validates against its real schema — and a
    // malformed one does not. The pair matters because the opaque branch
    // above would otherwise accept any `Drawing` object at all: once the tag
    // is in `TAB_KIND_TAGS` that branch's `not: {enum: …}` stops matching it,
    // so the real branch is the only thing left to be right.
    let mut drw = v4.clone();
    drw["tabs"].as_array_mut().unwrap().push(serde_json::json!({
        "id": "drw", "name": "Drawing 1",
        "kind": { "type": "Drawing", "drawing": { "sheets": [
            { "id": "6f1c2a4e-1111-4222-8333-44445555aaaa", "name": "Sheet 1",
              "size": { "type": "A3" }, "orientation": { "type": "Landscape" },
              "views": [
                { "id": "6f1c2a4e-1111-4222-8333-44445555bbbb", "name": "Top",
                  "source": { "tab_id": "t" },
                  "projection": { "type": "Named", "view": { "type": "Top" } },
                  "scale": 1.0, "placement_mm": [100.0, 100.0] }
              ] }
        ], "projection_angle": { "type": "Third" } } }
    }));
    let errors: Vec<String> = validator.iter_errors(&drw).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "drawing tab must validate: {errors:#?}");
    let mut bad_drw = v4.clone();
    bad_drw["tabs"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": "drw", "name": "Drawing 1", "kind": { "type": "Drawing", "drawing": 42 }
        }));
    assert!(
        !validator.is_valid(&bad_drw),
        "malformed Drawing must not validate"
    );
    // An Assembly tab (Phase 3) validates against its real schema — and a
    // malformed one (no `assembly`) does not.
    let mut asm = v4.clone();
    asm["tabs"].as_array_mut().unwrap().push(serde_json::json!({
        "id": "asm", "name": "Assembly 1",
        "kind": { "type": "Assembly", "assembly": { "instances": [
            { "id": "6f1c2a4e-1111-4222-8333-444455556666", "name": "P", "source": { "tab_id": "t" },
              "transform": { "translation_m": [0.0, 0.0, 0.01], "rotation_quat": [0.0, 0.0, 0.0, 1.0] } }
        ], "mates": [ { "id": "6f1c2a4e-1111-4222-8333-444455556677", "name": "m",
            "kind": { "type": "Fastened", "flip": true }, "connectors": ["6f1c2a4e-1111-4222-8333-444455556688", "6f1c2a4e-1111-4222-8333-444455556699"] } ] } }
    }));
    let errors: Vec<String> = validator.iter_errors(&asm).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "assembly tab must validate: {errors:#?}");
    let mut bad_asm = v4.clone();
    bad_asm["tabs"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": "asm", "name": "Assembly 1", "kind": { "type": "Assembly", "instances": [] }
        }));
    assert!(
        !validator.is_valid(&bad_asm),
        "an Assembly tab without `assembly` must not validate"
    );
}
