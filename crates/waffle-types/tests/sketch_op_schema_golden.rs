//! JSON Schema golden for the sketch operation vocabulary
//! (`specs/agent_mechanical_design.md` §10.1/§10.3, S1/S3).
//!
//! Pinned to `docs/schema/sketch-op.schema.json`. Regenerate with
//!
//! ```text
//! UPDATE_SCHEMA=1 cargo test -p waffle-types --features json-schema \
//!     --test sketch_op_schema_golden
//! ```
//!
//! ## Why this one is pinned separately
//!
//! [`SketchOp`] is a WIRE type: nothing stores one, so it is not reachable
//! from `docs/schema/waffle-v5.schema.json`, which is generated from the
//! document tree. But it IS the input schema of the `sketch_edit` tool, which
//! the relay validates every call against before the page sees it
//! (`specs/waffle_mcp_server.md` §2.4), and
//! `app/scripts/gen-agent-manifest.mjs` reads THIS golden to build that
//! schema. So the file is the agent's contract for the whole operation
//! vocabulary, with no Rust test behind it unless this one exists — and
//! putting it in the file-format golden instead would claim an op is part of
//! the saved format.
#![cfg(feature = "json-schema")]

use std::path::{Path, PathBuf};

use schemars::schema_for;
use waffle_types::sketch_ops::SketchOp;

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/schema")
        .join(name)
}

fn check(name: &str, schema: serde_json::Value) {
    let pretty = serde_json::to_string_pretty(&schema).unwrap() + "\n";
    let path = golden(name);
    if std::env::var("UPDATE_SCHEMA").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &pretty).unwrap();
        eprintln!("wrote {}", path.display());
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}\nregenerate with: UPDATE_SCHEMA=1 cargo test -p waffle-types \
             --features json-schema --test sketch_op_schema_golden",
            path.display()
        )
    });
    let committed: serde_json::Value = serde_json::from_str(&committed).unwrap();
    assert_eq!(
        committed,
        schema,
        "{} is stale; regenerate with UPDATE_SCHEMA=1 cargo test -p waffle-types \
         --features json-schema --test sketch_op_schema_golden",
        path.display()
    );
}

#[test]
fn the_sketch_op_schema_is_current() {
    check(
        "sketch-op.schema.json",
        serde_json::to_value(schema_for!(SketchOp)).unwrap(),
    );
}

#[test]
fn the_schema_carries_all_thirteen_operations() {
    let schema = serde_json::to_value(schema_for!(SketchOp)).unwrap();
    let variants: Vec<String> = schema["oneOf"]
        .as_array()
        .expect("a tagged enum is a oneOf")
        .iter()
        .map(|v| {
            v["properties"]["type"]["const"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        variants,
        vec![
            "AddEntity",
            "RemoveEntity",
            "AddConstraint",
            "RemoveConstraint",
            "SetDimension",
            "SetConstruction",
            "MovePoint",
            "Trim",
            "Extend",
            "Offset",
            "Fillet",
            "Mirror",
            "Project",
        ],
        "§10.3's thirteen SketchOp variants"
    );
}

#[test]
fn an_operation_can_name_an_entity_and_a_constraint() {
    // `sketch_edit`'s input schema is only as complete as this `$defs` block:
    // the relay resolves every `$ref` against it with no access to the Rust
    // types, so an `AddEntity` whose `SketchEntity` definition went missing
    // would be refused at the relay with a dangling-reference error rather
    // than reaching the page.
    let schema = serde_json::to_value(schema_for!(SketchOp)).unwrap();
    let defs = schema["$defs"].as_object().expect("definitions");
    for name in [
        "SketchEntity",
        "SketchConstraint",
        "End",
        "Side",
        "ProjectShape",
        "ProjectedPoint",
    ] {
        assert!(
            defs.contains_key(name),
            "$defs has {name}: {:?}",
            defs.keys().collect::<Vec<_>>()
        );
    }
}
