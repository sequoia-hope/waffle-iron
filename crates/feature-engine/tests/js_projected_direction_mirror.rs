//! The app's hover labels for the eight `ProjectedDirection`s must be the
//! engine's own (D4e, `specs/drawings_and_mbd.md` §8).
//!
//! `app/src/lib/drawings/viewPlacement.js` carries a `PROJECTED_DIRECTIONS`
//! table so the projected-view tool can label a sector the instant the pointer
//! enters it — before any probe answers. Every label a view ENDS UP with comes
//! from the engine (`ProjectedDirection::label`, through `default_view_name`
//! and the probe's `name`), so the two must agree: a ghost that says
//! `Iso (up-right)` and a view that is then called something else is the kind
//! of disagreement a user reads as a bug in the tool.
//!
//! The arrangement is `file-format`'s `js_format_mirror.rs`, for the same
//! reason it exists there: no test tier compared the two copies, and the one
//! that drifted was found a day later by its symptom.

use feature_engine::drawing::ProjectedDirection;

/// `{ tag: 'UpRight', label: 'Iso (up-right)', step: [1, 1] }` → the pair.
fn js_table(src: &str) -> Vec<(String, String)> {
    let start = src
        .find("export const PROJECTED_DIRECTIONS = [")
        .expect("`PROJECTED_DIRECTIONS` not found in viewPlacement.js");
    let end = src[start..]
        .find("\n];")
        .map(|i| start + i)
        .expect("`PROJECTED_DIRECTIONS` is not closed");
    let mut out = Vec::new();
    for entry in src[start..end].split('{').skip(1) {
        let field = |name: &str| {
            let at = entry.find(&format!("{name}: '"))? + name.len() + 3;
            let rest = &entry[at..];
            Some(rest[..rest.find('\'')?].to_string())
        };
        if let (Some(tag), Some(label)) = (field("tag"), field("label")) {
            out.push((tag, label));
        }
    }
    out
}

#[test]
fn the_apps_direction_labels_are_the_engines() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../app/src/lib/drawings/viewPlacement.js");
    let src =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let js = js_table(&src);
    let rust: Vec<(String, String)> = ProjectedDirection::ALL
        .iter()
        .map(|d| (format!("{d:?}"), d.label().to_string()))
        .collect();
    assert_eq!(
        js, rust,
        "app/src/lib/drawings/viewPlacement.js's PROJECTED_DIRECTIONS must be \
         `ProjectedDirection::ALL`'s variants and labels, in order — the tool's hover \
         label and the view's own name come from the two tables and must agree"
    );
}

#[test]
fn the_apps_paper_steps_point_the_way_the_engines_do() {
    // The step's LENGTH is deliberately not compared: the engine's is unit
    // (`1/√2` per axis on a diagonal) because `auto_placement_step_mm`
    // normalizes whatever it is given, and the app's table is `[±1, ±1]`
    // because the sector test only reads the SIGNS. What must match is the
    // direction, which is what both are for.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../app/src/lib/drawings/viewPlacement.js");
    let src =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    for direction in ProjectedDirection::ALL {
        let tag = format!("{direction:?}");
        let entry = src
            .split('{')
            .find(|e| e.contains(&format!("tag: '{tag}'")))
            .unwrap_or_else(|| panic!("viewPlacement.js has no entry for {tag}"));
        let at = entry.find("step: [").expect("a step") + "step: [".len();
        let rest = &entry[at..];
        let inner = &rest[..rest.find(']').expect("a closed step")];
        let js: Vec<f64> = inner
            .split(',')
            .map(|s| s.trim().parse::<f64>().expect("a number"))
            .collect();
        let engine = direction.paper_step();
        for k in 0..2 {
            assert_eq!(
                js[k].signum() as i32 * (js[k] != 0.0) as i32,
                engine[k].signum() as i32 * (engine[k] != 0.0) as i32,
                "{tag}: the app steps {js:?} and the engine steps {engine:?}"
            );
        }
    }
}
