//! **The pid wire contract.** Every persistent id that crosses the WASM↔JS
//! boundary crosses as a decimal STRING — in every `EngineToUi` /
//! `UiToEngine` message and in every MCP tool argument and result.
//!
//! Why (`waffle_types::pid_str`, where the arithmetic is pinned): a
//! persistent id is a content-seeded `u64`, so it routinely exceeds `2^53`;
//! a JSON number in JavaScript is an `f64`; and `JSON.parse` does not round
//! such an id to a nearby id but to a DIFFERENT ENTITY. D4a found the loud
//! half of that — a dimension anchored on a rounded id refusing as "resolves
//! to no geometry". The half this file exists for is the quiet one, where the
//! rounded id lands on another live entity and resolves to the wrong face
//! with no warning at all.
//!
//! Three pins, because they fail on different things:
//!
//! 1. [`every_tool_result_that_carries_a_pid_crosses_as_a_string`] — a real
//!    kernel session, the enumerated tools, every answer walked. Fails if a
//!    tool's answer actually carries a number.
//! 2. [`a_pid_with_the_top_bit_set_round_trips_through_every_carrier`] —
//!    the enumerated carrier TYPES at `u64::MAX`. Fails if a representation
//!    is lossy rather than merely numeric.
//! 3. [`no_rust_pid_field_is_declared_without_the_rule`] — the drift oracle
//!    over the sources. The first two can only fail for a field that already
//!    exists; this one fails for the NEXT one, which is how the rule survives
//!    the session that added it.

use std::collections::HashMap;

use feature_engine::types::*;
use kernel_v2::KernelV2Adapter;
use serde_json::{json, Value};
use uuid::Uuid;
use waffle_types::*;
use wasm_bridge::messages::*;
use wasm_bridge::*;

/// The id whose rounding was measured (D4a), and the worst case for an `f64`.
const MEASURED: u64 = 2_216_071_694_111_992_607;
const TOP_BIT: u64 = u64::MAX;

// ─────────────────────────────── the walker ───────────────────────────────

/// Every `pid` / `root_pid` in `v` that is a JSON NUMBER, by path.
///
/// This is the property that makes a JavaScript round-trip safe, stated
/// directly: a string survives `JSON.parse`/`JSON.stringify` exactly, a
/// number above `2^53` does not.
fn numeric_pids(v: &Value, path: &str, out: &mut Vec<String>) {
    match v {
        Value::Object(map) => {
            for (k, child) in map {
                let here = format!("{path}/{k}");
                if (k == "pid" || k == "root_pid") && child.is_number() {
                    out.push(format!("{here} = {child}"));
                }
                numeric_pids(child, &here, out);
            }
        }
        Value::Array(items) => {
            for (i, child) in items.iter().enumerate() {
                numeric_pids(child, &format!("{path}/{i}"), out);
            }
        }
        _ => {}
    }
}

#[track_caller]
fn assert_no_numeric_pid(label: &str, v: &Value) {
    let mut found = Vec::new();
    numeric_pids(v, label, &mut found);
    assert!(
        found.is_empty(),
        "{label} carries a persistent id as a JSON NUMBER, which JavaScript rounds above 2^53 \
         into a different entity (waffle_types::pid_str): {found:#?}\nin: {v}"
    );
}

/// How many `pid`/`root_pid` keys appear at all — so a "no numbers" pass on
/// an answer that carries no pid cannot be mistaken for the contract holding.
fn pid_keys(v: &Value) -> usize {
    match v {
        Value::Object(map) => map
            .iter()
            .map(|(k, c)| usize::from(k == "pid" || k == "root_pid") + pid_keys(c))
            .sum(),
        Value::Array(items) => items.iter().map(pid_keys).sum(),
        _ => 0,
    }
}

// ───────────────────────────── a real session ─────────────────────────────

fn datum_xy() -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::new_v4(),
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
        scope: None,
    }
}

fn point(id: u32, x: f64, y: f64) -> SketchEntity {
    SketchEntity::Point {
        id,
        x,
        y,
        construction: false,
    }
}

fn line(id: u32, a: u32, b: u32) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id: a,
        end_id: b,
        construction: false,
    }
}

fn added_id(response: EngineToUi) -> Uuid {
    match response {
        EngineToUi::ModelUpdated {
            feature_id: Some(id),
            errors,
            ..
        } => {
            assert!(errors.is_empty(), "rebuild errors: {errors:?}");
            id
        }
        other => panic!("expected ModelUpdated with an id, got {other:?}"),
    }
}

/// A 30×20×10 mm block, and the body id the tools address it by.
fn block(state: &mut EngineState, kernel: &mut KernelV2Adapter) -> String {
    let (sx, sy, depth) = (0.030, 0.020, 0.010);
    let corners = [(1u32, 0.0, 0.0), (2, sx, 0.0), (3, sx, sy), (4, 0.0, sy)];
    let solved_positions: HashMap<u32, (f64, f64)> =
        corners.iter().map(|&(id, x, y)| (id, (x, y))).collect();
    let mut entities: Vec<SketchEntity> =
        corners.iter().map(|&(id, x, y)| point(id, x, y)).collect();
    entities.extend([
        line(11, 1, 2),
        line(12, 2, 3),
        line(13, 3, 4),
        line(14, 4, 1),
    ]);
    let sketch = Sketch {
        id: Uuid::new_v4(),
        plane_face: None,
        plane: datum_xy(),
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities,
        constraints: vec![],
        solve_status: SolveStatus::FullyConstrained,
        solved_positions,
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![11, 12, 13, 14],
            is_outer: true,
            vertex_ids: vec![1, 2, 3, 4],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        }],
        projected: vec![],
    };

    let sketch_feature = added_id(dispatch(
        state,
        UiToEngine::AddFeature {
            operation: Operation::Sketch { sketch },
            provenance: None,
        },
        kernel,
    ));
    let extrude = added_id(dispatch(
        state,
        UiToEngine::AddFeature {
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id: sketch_feature,
                    profile_index: 0,
                    profile_entity_ids: Some(vec![11, 12, 13, 14]),
                    depth,
                    depth_expr: None,
                    direction: None,
                    symmetric: false,
                    cut: false,
                    merge: false,
                    target_body: None,
                    depth_mode: DepthMode::Blind,
                    second_direction: None,
                    region: None,
                    regions: Vec::new(),
                    combine: Some(CombineMode::NewBody),
                    targets: None,
                },
            },
            provenance: None,
        },
        kernel,
    ));
    // The host tessellates after every dispatch; a listing needs a rendered
    // body.
    wasm_bridge::tessellation_runner::tessellate_missing_meshes(state, kernel);
    FeatureTree::body_id(extrude, &OutputKey::Main)
}

#[track_caller]
fn ok(state: &mut EngineState, kernel: &mut KernelV2Adapter, tool: &str, args: Value) -> Value {
    let r = execute_tool(
        state,
        kernel,
        tool,
        &args,
        Some(&json!({ "agent_name": "pid-contract" })),
    );
    assert!(!r.is_error, "{tool} failed: {r:?}");
    r.structured_content
}

/// **Pin 1.** The enumerated tools, over a real kernel, every answer walked.
///
/// The list is the D4a review's, re-derived against the tree: the tools whose
/// answers carry a `GeomRef` (hence a `Selector::Pid`) or a bare pid field.
/// `entity_list` is the one that carried bare numbers — Q6 shipped
/// `ListedEntity.pid` as a `u64` and its own test asserted `is_u64()`.
#[test]
fn every_tool_result_that_carries_a_pid_crosses_as_a_string() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = block(&mut state, &mut kernel);

    // Name an edge, so `names_list` has a `Selector::Pid` target to report
    // and `entity_meta` has a named entity to describe.
    let edges = ok(
        &mut state,
        &mut kernel,
        "entity_list",
        json!({ "body_id": &body, "kind": "edge" }),
    );
    let first_edge = edges["entities"][0]["geom_ref"].clone();
    assert!(first_edge.is_object(), "an edge ref: {edges}");
    let _ = ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "name": "front_edge", "target": { "type": "entity", "geom_ref": first_edge.clone() } }),
    );

    let mut carriers = 0usize;
    for (tool, args) in [
        ("entity_list", json!({ "body_id": &body, "kind": "face" })),
        ("entity_list", json!({ "body_id": &body, "kind": "edge" })),
        ("entity_list", json!({ "body_id": &body, "kind": "vertex" })),
        ("face_list", json!({ "body_id": &body })),
        ("names_list", json!({})),
        ("model_summary", json!({})),
        ("assembly_get", json!({})),
        ("entity_meta", json!({ "name": "front_edge" })),
    ] {
        let answer = execute_tool(
            &mut state,
            &mut kernel,
            tool,
            &args,
            Some(&json!({ "agent_name": "pid-contract" })),
        );
        // An answer that REFUSES is fine (there is no assembly tab here);
        // what must never happen is a numeric pid in either shape.
        let label = format!("{tool}{args}");
        assert_no_numeric_pid(&label, &answer.structured_content);
        carriers += pid_keys(&answer.structured_content);
    }

    // And the walk really saw pids — otherwise "no numeric pid" is vacuous.
    // A box lists 6 faces + 12 edges + 8 vertices, each with `pid` and
    // `root_pid`, so this is in the dozens.
    assert!(
        carriers >= 2 * (6 + 12 + 8),
        "the enumerated answers carried {carriers} pid keys; the walk is vacuous"
    );

    // Every engine MESSAGE that transports one, through the same walk.
    //
    // `ListFaces` is in the list and carries NO pid, which is a measurement
    // worth keeping rather than an omission: the D4a review listed
    // "`face_list`'s `EntityPid`", and the face refs `face_refs.rs` mints are
    // `Selector::Role`, not `Selector::Pid` (the `entity_list` FACE arm
    // reuses exactly those refs, which is why its `geom_ref` carries no pid
    // either while its `pid` field does). So it is asserted clean, not
    // asserted to carry one.
    let mut carriers_by_message: Vec<(&str, usize)> = Vec::new();
    for (label, message) in [
        (
            "ListEntities",
            UiToEngine::ListEntities {
                body_id: body.clone(),
                kind: EntityListKind::Edge,
                filter: None,
            },
        ),
        (
            "ListFaces",
            UiToEngine::ListFaces {
                body_id: body.clone(),
                filter: None,
            },
        ),
        (
            "QueryEntityNames",
            UiToEngine::QueryEntityNames { body_id: None },
        ),
    ] {
        let reply = dispatch(&mut state, message, &mut kernel);
        let json = serde_json::to_value(&reply).expect("the reply serializes");
        assert_no_numeric_pid(label, &json);
        carriers_by_message.push((label, pid_keys(&json)));
    }
    // The two that DO carry one carry one, so the walk above is not vacuous
    // for them; `ListFaces` is recorded at zero on purpose (see the note).
    assert_eq!(
        carriers_by_message
            .iter()
            .map(|(l, n)| (*l, *n > 0))
            .collect::<Vec<_>>(),
        vec![
            ("ListEntities", true),
            ("ListFaces", false),
            ("QueryEntityNames", true),
        ],
        "which messages transport a pid changed: {carriers_by_message:?}"
    );

    // The saved document too: the same `Selector::Pid`, through the writer.
    let saved = dispatch(&mut state, UiToEngine::SaveDocument {}, &mut kernel);
    if let EngineToUi::SaveReady { json_data, .. } = &saved {
        let file: Value = serde_json::from_str(json_data).expect("the file parses");
        assert_no_numeric_pid("SaveDocument", &file);
        assert!(
            pid_keys(&file) > 0,
            "the named edge's pid is in the file: {file}"
        );
    } else {
        panic!("expected SaveReady, got {saved:?}");
    }
}

// ─────────────────────── the carrier types, at u64::MAX ───────────────────

/// **Pin 2.** The enumerated carrier TYPES, at an id with the top bit set.
///
/// Pin 1 proves the wire shape; this proves the VALUE survives, which is a
/// different claim: a representation can be a string and still be lossy (a
/// `f64`-formatted one, say). Each case asserts the exact decimal text on the
/// wire and byte-equality after a round trip.
#[test]
fn a_pid_with_the_top_bit_set_round_trips_through_every_carrier() {
    #[track_caller]
    fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned>(label: &str, value: &T) {
        let json = serde_json::to_value(value).expect("serializes");
        assert_no_numeric_pid(label, &json);
        assert!(pid_keys(&json) > 0, "{label} carries a pid: {json}");
        let text = serde_json::to_string(value).expect("serializes");
        assert!(
            text.contains(&format!("\"{TOP_BIT}\"")) || text.contains(&format!("\"{MEASURED}\"")),
            "{label} writes the id as decimal text: {text}"
        );
        // And back, exactly.
        let back: T = serde_json::from_str(&text).expect("round-trips");
        let again = serde_json::to_value(&back).expect("serializes");
        assert_eq!(json, again, "{label} round-trips byte-identically");
    }

    // The shared carrier: every message and every tool argument that holds a
    // reference holds this.
    let pid_ref = GeomRef {
        kind: TopoKind::Edge,
        anchor: Anchor::FeatureOutput {
            feature_id: Uuid::nil(),
            output_key: OutputKey::Main,
        },
        selector: Selector::Pid {
            pid: TOP_BIT,
            root_pid: MEASURED,
        },
        policy: ResolvePolicy::Strict,
        scope: None,
    };
    round_trip("GeomRef/Selector::Pid", &pid_ref);
    assert_eq!(
        serde_json::to_value(&pid_ref).unwrap()["selector"],
        json!({
            "type": "Pid",
            "pid": "18446744073709551615",
            "root_pid": "2216071694111992607"
        })
    );

    // The bare pid fields.
    round_trip(
        "DrawingAnchorSpec",
        &DrawingAnchorSpec {
            pid: TOP_BIT,
            kind: TopoKind::Edge,
        },
    );
    round_trip(
        "ViewAnchor",
        &feature_engine::drawing::ViewAnchor {
            pid: TOP_BIT,
            shape: feature_engine::drawing::AnchorShape::Line,
            kind: TopoKind::Edge,
            at: None,
            radius: None,
        },
    );

    // A `Selector::Pid` still READS from a bare number, which is what keeps
    // every pre-flip `.waffle` and every hand-written tool argument working.
    let numeric: Selector = serde_json::from_value(json!({
        "type": "Pid", "pid": 42, "root_pid": 7
    }))
    .expect("a numeric pid still reads");
    match numeric {
        Selector::Pid { pid, root_pid } => {
            assert_eq!((pid, root_pid), (42, 7));
        }
        other => panic!("want a Pid selector, got {other:?}"),
    }
}

// ──────────────────────────── the drift oracle ────────────────────────────

/// **Pin 3.** No `pid` / `root_pid` field may be DECLARED without the rule.
///
/// The two pins above can only fail for a field that already exists. This
/// one fails for the next one: it reads the sources of the crates a pid can
/// cross from and requires every `pid: u64` / `root_pid: u64` /
/// `Option<u64>` field to carry a `pid_str` (or `pid_string`) serde
/// attribute — unless the type it sits in is not serialized at all, which it
/// must say so by being on the exemption list below, with its reason.
///
/// A source scan rather than a type-level trick because the hazard is a
/// MISSING attribute, and there is nothing in the type system to hang a
/// requirement on: `#[serde(with = "…")]` is a string literal the compiler
/// never reads. The companion in `file-format/tests/schema_golden.rs`
/// catches the same mistake from the other side, in the generated schemas.
///
/// The exemption is derived, not listed: a pid field needs the attribute
/// only if its enclosing type derives `Serialize`. That keeps the kernel's
/// own `EntityPid`/`FaceProvenance` (handed between Rust crates, never
/// written) and `DrawingError`'s display-only arms out of it without a
/// hand-maintained allowlist going stale — add `Serialize` to one of those
/// types and the scan starts demanding the rule, which is correct.
#[test]
fn no_rust_pid_field_is_declared_without_the_rule() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root");

    let mut offenders: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for crate_name in [
        "waffle-types",
        "feature-engine",
        "wasm-bridge",
        "file-format",
    ] {
        let src = root.join("crates").join(crate_name).join("src");
        for file in rust_files(&src) {
            let text = std::fs::read_to_string(&file).expect("readable");
            let lines: Vec<&str> = text.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                let trimmed = line.trim();
                // A STRUCT FIELD declaration: `pub pid: u64,` or
                // `root_pid: Option<u64>,`. Not a `let`, not a function
                // parameter (those end in `,` too, so require a type that is
                // exactly the pid type and a name that is exactly the pid
                // name).
                let Some((name, ty)) = field_decl(trimmed) else {
                    continue;
                };
                if name != "pid" && name != "root_pid" {
                    continue;
                }
                if ty != "u64" && ty != "Option<u64>" {
                    continue;
                }
                // A function PARAMETER, not a field (`resolve_by_pid(…, pid:
                // u64, root_pid: u64, …)` reads identically line by line).
                let Some(item) = enclosing_item(&lines, i) else {
                    continue;
                };
                if item.kind == ItemKind::Fn {
                    continue;
                }
                // A type that is never serialized carries no wire rule to
                // break — `EntityPid`/`FaceProvenance` (kernel-internal) and
                // `DrawingError`'s display-only arms.
                if !item.serialized {
                    continue;
                }
                checked += 1;
                // The rule: a `pid_str`/`pid_string` serde attribute within
                // the attribute lines above the field.
                //
                // COMMENTS DO NOT COUNT. Measured while writing this: a doc
                // comment that merely says "see `waffle_types::pid_str`" made
                // the scan pass for a field whose attribute had been removed,
                // which is the one way a drift oracle fails — silently, in
                // the direction of "fine".
                let has_rule = lines[i.saturating_sub(8)..i]
                    .iter()
                    .filter(|l| {
                        let t = l.trim();
                        !t.starts_with("//")
                    })
                    .any(|l| l.contains("pid_str") || l.contains("pid_string"));
                if !has_rule {
                    offenders.push(format!(
                        "{}:{}: `{}`",
                        file.strip_prefix(&root).unwrap_or(&file).display(),
                        i + 1,
                        trimmed
                    ));
                }
            }
        }
    }

    assert!(
        checked >= 3,
        "the scanner found only {checked} serialized pid fields; it has stopped matching \
         (expected at least Selector::Pid's two plus ListedEntity's two)"
    );
    assert!(
        offenders.is_empty(),
        "a persistent id field is declared without the wire rule. Every serialized pid is a \
         decimal STRING, because a JSON number in JavaScript is an f64 and a rounded id is a \
         DIFFERENT entity (waffle_types::pid_str). Add\n  \
         #[serde(with = \"waffle_types::pid_str\")]            // or `::option` for Option<u64>\n  \
         #[cfg_attr(feature = \"json-schema\", schemars(with = \"String\"))]\n\
         and regenerate the goldens with UPDATE_SCHEMA=1.\noffenders: {offenders:#?}"
    );

    // The derived exemption is honest for the two types it matters for:
    // `EntityPid` is what a serialized `Selector::Pid` is BUILT from, and if
    // it ever gained `Serialize` it would become a wire carrier itself.
    let types_src =
        std::fs::read_to_string(root.join("crates/waffle-types/src/kernel/types.rs")).unwrap();
    for name in ["FaceProvenance", "EntityPid"] {
        let at = types_src
            .find(&format!("pub struct {name} "))
            .unwrap_or_else(|| panic!("{name} is declared in kernel/types.rs"));
        let derives = &types_src[at.saturating_sub(200)..at];
        assert!(
            !derives.contains("Serialize"),
            "{name} gained `Serialize`, so its pid now crosses a boundary: give it the \
             `waffle_types::pid_str` rule (and this assertion a new home)"
        );
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ItemKind {
    Struct,
    Enum,
    Fn,
}

struct Item {
    kind: ItemKind,
    serialized: bool,
}

fn rust_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            out.extend(rust_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// `pub pid: u64,` → `("pid", "u64")`. `None` for anything else.
fn field_decl(line: &str) -> Option<(&str, &str)> {
    let line = line.strip_suffix(',')?;
    let line = line.strip_prefix("pub ").unwrap_or(line);
    let (name, ty) = line.split_once(": ")?;
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || name.is_empty() {
        return None;
    }
    Some((name, ty.trim()))
}

/// The nearest `struct` / `enum` / `fn` declaration above line `i`, and —
/// for a type — whether it derives `Serialize`.
///
/// Nearest-above is enough to tell a struct field from a function parameter,
/// which is the only distinction this oracle needs; a pid field is always
/// declared within a few lines of its type or its `fn`.
fn enclosing_item(lines: &[&str], i: usize) -> Option<Item> {
    let (at, kind) = lines[..i].iter().enumerate().rev().find_map(|(n, l)| {
        let t = l.trim();
        for (kw, kind) in [
            ("struct ", ItemKind::Struct),
            ("enum ", ItemKind::Enum),
            ("fn ", ItemKind::Fn),
        ] {
            let body = t.strip_prefix("pub ").unwrap_or(t);
            let body = body.strip_prefix("pub(crate) ").unwrap_or(body);
            let body = body.strip_prefix("pub(super) ").unwrap_or(body);
            if body.starts_with(kw) {
                return Some((n, kind));
            }
        }
        None
    })?;
    // The derive attributes sit immediately above the declaration (with doc
    // comments possibly between), so scan back until a blank line.
    let serialized = lines[..at]
        .iter()
        .rev()
        .take_while(|l| !l.trim().is_empty())
        .any(|l| l.contains("Serialize") || l.contains("serde::Serialize"));
    Some(Item { kind, serialized })
}
