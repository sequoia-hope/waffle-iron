//! Render-view parity: the target-independent `process` + `render_view`
//! pipeline gives a native host the same render data the wasm bundle gives the
//! web worker (`specs/waffle_server_mode.md` §2.3 S0).
//!
//! `fixtures/render_view/scenarios.json` holds message sequences: corpus loads
//! (extrude, revolve, a consuming boolean, arc edges, a loud boolean error), an
//! assembly, and an in-context edit with ghost bodies. `golden.json` is a census
//! of every accessor the worker calls after each message — lengths plus FNV-1a
//! hashes of the exact bytes and JSON strings — written by
//! `render_view_parity.mjs` from a wasm bundle.
//!
//! Two layers:
//!
//! - **Bundle, byte for byte** (`render_view_parity.mjs <pkg dir>` with no output
//!   path): a bundle's census must equal `golden.json` exactly. Run it for any
//!   change to the bridge; the S0 move was proven this way (the rebuilt bundle's
//!   census is byte-identical to the pre-move bundle's). It is not in a test
//!   tier because every tessellation change legitimately changes the bytes.
//! - **Native vs bundle, structure** (this test): the same response types, the
//!   same body metadata, and the same body, feature and array counts. Not the
//!   bytes: native and wasm32 float results differ at the noise level (measured
//!   on F0061: normal components near zero differ by ~1e-16, which permutes 20
//!   vertex slots and flips a few triangulation near-ties), and the response's
//!   `preview_mesh` is decimated through a `HashMap` whose iteration order is
//!   randomized per native process (`feature_engine::preview_mesh`).
//!
//! Regenerate when the scenarios or the render data change on purpose:
//!
//!   cargo test -p wasm-bridge --test render_view_parity -- --ignored write_scenarios
//!   ./scripts/build-wasm.sh
//!   node --stack-size=8000 crates/wasm-bridge/tests/render_view_parity.mjs \
//!     app/static/pkg crates/wasm-bridge/tests/fixtures/render_view/golden.json

use std::f64::consts::FRAC_1_SQRT_2;
use std::path::PathBuf;
use std::sync::OnceLock;

use feature_engine::assembly::{AssemblyTree, Instance, PartRef, Transform};
use kernel_v2::KernelV2Adapter;
use serde_json::{json, Map, Value};
use uuid::Uuid;
use wasm_bridge::{process, render_view, EngineState};

/// Small corpus cases covering a plain extrude, a revolve, a boolean that
/// consumes its operand, arc edges, and an expected loud error.
const CASES: [&str; 5] = ["C0077", "C0070", "R0052", "F0061", "F0074"];

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/render_view/{name}"))
}

fn read_fixture(name: &str) -> Value {
    let path = fixture(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path:?} is not JSON: {e}"))
}

fn load(id: &str) -> Value {
    let path = format!(
        "{}/../../app/tests/cases/assay/{id}.waffle",
        env!("CARGO_MANIFEST_DIR")
    );
    let data = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    json!({ "type": "LoadProject", "data": data })
}

fn run(state: &mut EngineState, kernel: &mut KernelV2Adapter, msg: &Value) -> String {
    process::process_message_json(state, kernel, &msg.to_string(), &|| 0.0, &|_| {})
}

fn instance(n: u128, name: &str, transform: Transform) -> Instance {
    Instance {
        id: Uuid::from_u128(n),
        name: name.into(),
        source: PartRef {
            source_id: None,
            tab_id: "part".into(),
        },
        transform,
        fixed: true,
        suppressed: false,
        external_key: None,
        parameter_overrides: None,
        extra: Map::new(),
    }
}

/// The scenarios, built from typed values so a message shape change fails to
/// compile here rather than silently in the fixture.
fn scenarios() -> &'static Vec<(String, Vec<Value>)> {
    static SCENARIOS: OnceLock<Vec<(String, Vec<Value>)>> = OnceLock::new();
    SCENARIOS.get_or_init(|| {
        let mut out: Vec<(String, Vec<Value>)> = CASES
            .iter()
            .map(|id| (id.to_string(), vec![load(id)]))
            .collect();

        // The part: C0077's tree as the engine holds it after loading.
        let mut state = EngineState::new();
        let mut kernel = KernelV2Adapter::new();
        run(&mut state, &mut kernel, &load("C0077"));
        let part = serde_json::to_value(&state.engine.tree).expect("tree serializes");

        let b_placement = Transform {
            translation_m: [0.05, 0.02, 0.0],
            rotation_quat: [0.0, 0.0, FRAC_1_SQRT_2, FRAC_1_SQRT_2],
        };
        let assembly = AssemblyTree {
            instances: vec![
                instance(0xA, "A", Transform::identity()),
                instance(0xB, "B", b_placement),
            ],
            ..Default::default()
        };
        let assembly = serde_json::to_value(&assembly).expect("assembly serializes");

        out.push((
            "assembly".into(),
            vec![json!({
                "type": "OpenAssembly",
                "assembly": assembly,
                "part_trees": { "part": part },
            })],
        ));
        out.push((
            "in_context".into(),
            vec![
                load("C0077"),
                json!({
                    "type": "OpenPartInContext",
                    "features": part,
                    "assembly_tab_id": "asm",
                    "instance_path": [Uuid::from_u128(0xB)],
                    "assembly": assembly,
                    "part_trees": { "part": part },
                }),
            ],
        ));
        out
    })
}

fn scenarios_json() -> Value {
    let list: Vec<Value> = scenarios()
        .iter()
        .map(|(name, messages)| json!({ "name": name, "messages": messages }))
        .collect();
    json!({ "scenarios": list })
}

/// FNV-1a 64 — mirrored byte for byte in `render_view_parity.mjs`.
fn fnv(bytes: impl IntoIterator<Item = u8>) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn f32s(v: Option<&[f32]>) -> Value {
    let v = v.unwrap_or(&[]);
    json!({ "n": v.len(), "fnv": fnv(v.iter().flat_map(|x| x.to_le_bytes())) })
}

fn u32s(v: Option<&[u32]>) -> Value {
    let v = v.unwrap_or(&[]);
    json!({ "n": v.len(), "fnv": fnv(v.iter().flat_map(|x| x.to_le_bytes())) })
}

fn text(s: &str) -> Value {
    json!({ "n": s.len(), "fnv": fnv(s.bytes()) })
}

/// The accessor's JSON string, as `wasm_api` serializes it.
fn entries(entries: Vec<Value>) -> Value {
    text(&serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string()))
}

/// Everything the worker reads after a message, in the shape the .mjs writes:
/// the response, the per-body accessors (`collectBodies`), and the per-feature
/// ones (`collectMeshes`' legacy path).
fn census(state: &EngineState, kernel: &KernelV2Adapter, response: &str) -> Value {
    let parsed: Value = serde_json::from_str(response).expect("response is JSON");
    let bodies: Vec<Value> = (0..render_view::collect_renderable_bodies(state).len())
        .map(|b| {
            json!({
                "vertices": f32s(render_view::body_vertices(state, b).as_deref()),
                "normals": f32s(render_view::body_normals(state, b).as_deref()),
                "indices": u32s(render_view::body_indices(state, b)),
                "faces": entries(render_view::body_face_entries(state, kernel, b)),
                "edge_vertices": f32s(render_view::body_edge_vertices(state, b).as_deref()),
                "edges": entries(render_view::body_edge_entries(state, b)),
            })
        })
        .collect();
    let features: Vec<Value> = (0..state.engine.tree.features.len())
        .map(|f| {
            let mesh = render_view::feature_mesh(state, f);
            let edges = render_view::feature_edges(state, f);
            json!({
                "mesh_json": text(&render_view::feature_mesh_json(state, f)),
                "vertices": f32s(mesh.map(|m| &m.vertices[..])),
                "normals": f32s(mesh.map(|m| &m.normals[..])),
                "indices": u32s(mesh.map(|m| &m.indices[..])),
                "faces": entries(render_view::feature_face_entries(state, kernel, f)),
                "edge_vertices": f32s(edges.map(|e| &e.vertices[..])),
                "edges": entries(render_view::feature_edge_entries(state, f)),
            })
        })
        .collect();
    json!({
        "response_type": parsed["type"],
        "response": text(response),
        "metadata": entries(render_view::body_metadata(state)),
        "bodies": bodies,
        "legacy": {
            "mesh_count": render_view::mesh_count(state),
            "renderable": render_view::renderable_feature_indices(state),
            "features": features,
        },
    })
}

/// The part of a census that must agree across targets: response types, body
/// metadata, and every count. Hashes of float-bearing data are left out.
fn structure(census: &Value) -> Value {
    let lengths = |item: &Value| {
        json!({
            "vertices": item["vertices"]["n"],
            "normals": item["normals"]["n"],
            "indices": item["indices"]["n"],
            "edge_vertices": item["edge_vertices"]["n"],
        })
    };
    let each = |items: &Value| -> Vec<Value> {
        items
            .as_array()
            .expect("census array")
            .iter()
            .map(lengths)
            .collect()
    };
    let scenarios: Vec<Value> = census["scenarios"]
        .as_array()
        .expect("census scenarios")
        .iter()
        .map(|scenario| {
            let steps: Vec<Value> = scenario["steps"]
                .as_array()
                .expect("census steps")
                .iter()
                .map(|step| {
                    json!({
                        "response_type": step["response_type"],
                        "metadata": step["metadata"],
                        "bodies": each(&step["bodies"]),
                        "mesh_count": step["legacy"]["mesh_count"],
                        "renderable": step["legacy"]["renderable"],
                        "features": each(&step["legacy"]["features"]),
                    })
                })
                .collect();
            json!({ "name": scenario["name"], "steps": steps })
        })
        .collect();
    json!({ "scenarios": scenarios })
}

/// Path of the first difference between two JSON values, if any.
fn first_diff(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            keys.into_iter().find_map(|k| match (x.get(k), y.get(k)) {
                (Some(p), Some(q)) => first_diff(p, q, &format!("{path}.{k}")),
                (p, q) => Some(format!("{path}.{k}: {p:?} vs {q:?}")),
            })
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .find_map(|(i, (p, q))| first_diff(p, q, &format!("{path}[{i}]"))),
        _ if a == b => None,
        _ => Some(format!("{path}: {a} vs {b}")),
    }
}

#[test]
#[ignore = "writes fixtures/render_view/scenarios.json; run when the scenarios change"]
fn write_scenarios() {
    let text = serde_json::to_string_pretty(&scenarios_json()).unwrap() + "\n";
    std::fs::write(fixture("scenarios.json"), text).expect("write scenarios.json");
}

// Both checks replay corpus documents through the real kernel: 17 s optimized,
// over 45 min unoptimized (measured 2026-09-15). They run under `--release`
// (`scripts/test.sh` fast tier, CI "render-view parity" step) and are skipped
// in debug builds.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only: cargo test --release -p wasm-bridge --test render_view_parity"
)]
fn scenarios_fixture_is_current() {
    if let Some(diff) = first_diff(&read_fixture("scenarios.json"), &scenarios_json(), "") {
        panic!(
            "scenarios.json is stale at {diff}: rerun write_scenarios and regenerate golden.json"
        );
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only: cargo test --release -p wasm-bridge --test render_view_parity"
)]
fn native_render_view_has_the_structure_of_the_wasm_bundle_census() {
    let mut steps = Vec::new();
    for (name, messages) in scenarios() {
        let mut state = EngineState::new();
        let mut kernel = KernelV2Adapter::new();
        let census: Vec<Value> = messages
            .iter()
            .map(|msg| {
                let response = run(&mut state, &mut kernel, msg);
                census(&state, &kernel, &response)
            })
            .collect();
        steps.push(json!({ "name": name, "steps": census }));
    }
    let native = structure(&json!({ "scenarios": steps }));
    let bundle = structure(&read_fixture("golden.json"));

    if let Some(diff) = first_diff(&bundle, &native, "") {
        panic!("native render view differs from the wasm bundle census at {diff}");
    }
}
