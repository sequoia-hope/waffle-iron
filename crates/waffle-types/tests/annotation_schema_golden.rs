//! JSON Schema golden for the annotation model and its layout record
//! (`specs/drawings_and_mbd.md` §7, increment D3).
//!
//! Two schemas, pinned to `docs/schema/annotation.schema.json` and
//! `docs/schema/annotation-layout.schema.json`. Regenerate both with
//!
//! ```text
//! UPDATE_SCHEMA=1 cargo test -p waffle-types --features json-schema \
//!     --test annotation_schema_golden
//! ```
//!
//! ## Why pin these now, before anything persists them
//!
//! `Annotation` is not reachable from a `.waffle` file yet — it becomes so
//! when D4a adds `TabKind::Drawing` and M2 adds the `Pmi` feature, and
//! `docs/schema/waffle-v5.schema.json` will pick it up then through
//! `file-format`'s own golden. Until then the shape has no oracle at all, and
//! the window between D3 and D4a is exactly when it is cheapest to change by
//! accident. The layout schema is pinned for a second reason: it is the
//! contract the app's SVG renderer reads
//! (`app/src/lib/drawings/`), so a silent change to it breaks the
//! renderer with no Rust test going red.
#![cfg(feature = "json-schema")]

use std::path::{Path, PathBuf};

use schemars::schema_for;
use waffle_types::annotation::layout::ViewLayout;
use waffle_types::annotation::Annotation;

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
             --features json-schema --test annotation_schema_golden",
            path.display()
        )
    });
    let committed: serde_json::Value = serde_json::from_str(&committed).unwrap();
    assert_eq!(
        committed,
        schema,
        "{} is stale; regenerate with UPDATE_SCHEMA=1 cargo test -p waffle-types \
         --features json-schema --test annotation_schema_golden",
        path.display()
    );
}

#[test]
fn the_annotation_schema_is_current() {
    check(
        "annotation.schema.json",
        serde_json::to_value(schema_for!(Annotation)).unwrap(),
    );
}

#[test]
fn the_layout_schema_is_current() {
    check(
        "annotation-layout.schema.json",
        serde_json::to_value(schema_for!(ViewLayout)).unwrap(),
    );
}

#[test]
fn the_annotation_schema_has_the_shape_section_7_describes() {
    let schema = serde_json::to_value(schema_for!(Annotation)).unwrap();
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
        vec!["Dimension", "Note", "CentreMark", "CentreLine", "Datum"],
        "the §7 variants, minus M1's FeatureControlFrame"
    );

    let defs = schema["$defs"].as_object().expect("definitions");
    for name in [
        "DimensionKind",
        "Measured",
        "Placement2",
        "GeomRef",
        "Selector",
    ] {
        assert!(
            defs.contains_key(name),
            "$defs has {name}: {:?}",
            defs.keys().collect::<Vec<_>>()
        );
    }

    // The eight dimension kinds: §7's "seven sketch dimension kinds, with
    // `Ordinate` added".
    let kinds: Vec<String> = defs["DimensionKind"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            v["properties"]["type"]["const"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            "Distance",
            "PointLineDistance",
            "HDistance",
            "VDistance",
            "Angle",
            "Radius",
            "Diameter",
            "Ordinate",
        ]
    );

    // An anchor is a `GeomRef`, so `Selector::Pid` is reachable from an
    // annotation — the whole point of D0 item 4.
    let selectors: Vec<String> = defs["Selector"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            v["properties"]["type"]["const"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert!(selectors.contains(&"Pid".to_string()), "{selectors:?}");
}

#[test]
fn the_layout_schema_carries_no_geom_ref() {
    // The architectural claim, checked against the schema rather than one
    // instance: the record the renderer reads has no path back to the model.
    //
    // Checked on the DEFINITIONS and the `$ref` targets, not on the schema's
    // text — the doc comments legitimately mention `GeomRef` to explain that
    // it is gone, and a substring search over the whole document reads those
    // as structure.
    let schema = serde_json::to_value(schema_for!(ViewLayout)).unwrap();
    let defs = schema["$defs"].as_object().expect("definitions");
    for forbidden in ["GeomRef", "Selector", "TopoSignature", "Anchor", "Measured"] {
        assert!(
            !defs.contains_key(forbidden),
            "{forbidden} reachable from ViewLayout: {:?}",
            defs.keys().collect::<Vec<_>>()
        );
    }
    // Every `$ref` in the document resolves inside `$defs`, so the key check
    // above is the whole reachable set.
    let refs = collect_refs(&schema);
    for r in &refs {
        let name = r
            .strip_prefix("#/$defs/")
            .unwrap_or_else(|| panic!("odd $ref {r}"));
        assert!(defs.contains_key(name), "dangling $ref {r}");
    }
    for name in [
        "LayoutCurve",
        "LayoutCurveEntry",
        "AnchorGeometry",
        "AnnotationLayout",
        "DimensionKind",
        "Visibility",
        "CurveKind",
    ] {
        assert!(
            defs.contains_key(name),
            "$defs has {name}: {:?}",
            defs.keys().collect::<Vec<_>>()
        );
    }
}

/// Every `$ref` string anywhere in `value`.
fn collect_refs(value: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                if k == "$ref" {
                    if let Some(s) = v.as_str() {
                        out.push(s.to_string());
                    }
                } else {
                    out.extend(collect_refs(v));
                }
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                out.extend(collect_refs(v));
            }
        }
        _ => {}
    }
    out
}
