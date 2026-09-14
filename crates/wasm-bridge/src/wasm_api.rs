//! WASM entry points for the web worker.
//!
//! This module is only compiled for the `wasm32` target. It provides the
//! `#[wasm_bindgen]` functions that JavaScript calls from the web worker.

use wasm_bindgen::prelude::*;

use crate::dispatch;
use crate::engine_state::EngineState;
use crate::messages::{EngineToUi, UiToEngine};
use feature_engine::assembly::Transform;
use modeling_ops::KernelBundle;
use waffle_types::kernel::{EdgeRenderData, KernelId, RenderMesh};
use waffle_types::{
    Anchor, GeomRef, OutputKey, RefScope, ResolvePolicy, Role, Selector, TopoKind, TopoSignature,
};

// Global engine state — single-threaded in the web worker.
thread_local! {
    static ENGINE_STATE: std::cell::RefCell<Option<WasmEngine>> = std::cell::RefCell::new(None);
}

/// Holds the engine state and kernel for the WASM module.
///
/// Since the Phase 6 migration (2026-06-11) the kernel is kernel-v2 behind
/// its legacy-trait adapter. The whole stack is Result-based — no panic
/// machinery is needed (the legacy catch_unwind wrappers existed to survive
/// panics deep in the old kernel's internals).
struct WasmEngine {
    state: EngineState,
    kernel: kernel_v2::KernelV2Adapter,
}

/// Initialize the WASM engine. Must be called once before any other function.
#[wasm_bindgen]
pub fn init() {
    // Surface panic messages in the browser console: without a hook a
    // panic reaches JS as a bare `unreachable` trap with no context (the
    // 2026-07-22 deployed-app debugging cost). Production paths are
    // Result-based and must not panic (crate contract) — this is the
    // loud last-resort diagnostic when one slips through, not a recovery
    // mechanism.
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&format!("WASM PANIC: {info}").into());
    }));
    ENGINE_STATE.with(|cell| {
        *cell.borrow_mut() = Some(WasmEngine {
            state: EngineState::new(),
            kernel: kernel_v2::KernelV2Adapter::new(),
        });
    });
}

/// Process a JSON message from the UI and return a JSON response.
///
/// This is the main entry point for the web worker's message handler.
/// The input should be a JSON-serialized `UiToEngine` message.
/// Returns a JSON-serialized `EngineToUi` response.
#[wasm_bindgen]
pub fn process_message(json_input: &str) -> String {
    let response = ENGINE_STATE.with(|cell| {
        let mut engine = cell.borrow_mut();
        let engine = engine
            .as_mut()
            .expect("Engine not initialized. Call init() first.");

        let msg: UiToEngine = match serde_json::from_str(json_input) {
            Ok(msg) => msg,
            Err(e) => {
                return EngineToUi::Error {
                    kind: None,
                    message: format!("Failed to parse message: {}", e),
                    feature_id: None,
                };
            }
        };

        let msg_type = format!("{:?}", std::mem::discriminant(&msg));
        let t0 = js_sys::Date::now();
        let response = dispatch::dispatch(&mut engine.state, msg, &mut engine.kernel);
        let dispatch_ms = js_sys::Date::now() - t0;

        // After dispatch, tessellate any solids that don't have mesh data yet
        if matches!(response, EngineToUi::ModelUpdated { .. }) {
            let t1 = js_sys::Date::now();
            tessellate_missing_meshes(&mut engine.state, &mut engine.kernel);
            let tess_ms = js_sys::Date::now() - t1;
            if dispatch_ms + tess_ms > 100.0 {
                web_sys::console::log_1(
                    &format!(
                        "[wasm] {} dispatch={:.1}s tess={:.1}s total={:.1}s",
                        msg_type,
                        dispatch_ms / 1000.0,
                        tess_ms / 1000.0,
                        (dispatch_ms + tess_ms) / 1000.0,
                    )
                    .into(),
                );
            }
        }

        response
    });

    serde_json::to_string(&response).unwrap_or_else(|e| {
        format!(
            r#"{{"type":"Error","message":"Serialization failed: {}","feature_id":null}}"#,
            e
        )
    })
}

/// Get the current feature tree as JSON.
///
/// Useful for the UI to query state without sending a full command.
#[wasm_bindgen]
pub fn get_feature_tree() -> String {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = match engine.as_ref() {
            Some(e) => e,
            None => return r#"{"features":[],"active_index":null}"#.to_string(),
        };
        serde_json::to_string(&engine.state.engine.tree).unwrap_or_default()
    })
}

/// Get mesh data for a specific feature by index.
///
/// Returns a JSON object with vertices, normals, and indices arrays.
/// For high-performance rendering, the web worker should use the
/// `get_mesh_vertices`, `get_mesh_normals`, and `get_mesh_indices`
/// functions instead, which return typed arrays directly.
#[wasm_bindgen]
pub fn get_mesh_json(feature_index: usize) -> String {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = match engine.as_ref() {
            Some(e) => e,
            None => return r#"{"error":"Engine not initialized"}"#.to_string(),
        };

        let results = &engine.state.engine.feature_results;
        let features = &engine.state.engine.tree.features;

        if feature_index >= features.len() {
            return r#"{"error":"Feature index out of range"}"#.to_string();
        }

        let feature_id = features[feature_index].id;
        if let Some(result) = results.get(&feature_id) {
            // Return the first output's mesh
            for (_key, body) in &result.outputs {
                if let Some(ref mesh) = body.mesh {
                    return serde_json::to_string(mesh).unwrap_or_default();
                }
            }
        }

        r#"{"error":"No mesh for this feature"}"#.to_string()
    })
}

/// Get mesh vertex positions as a Float32Array view into WASM memory.
///
/// Returns the vertices of the latest (last) feature's mesh as a zero-copy
/// typed array view. The array contains [x0, y0, z0, x1, y1, z1, ...].
///
/// IMPORTANT: The returned view is invalidated by any WASM memory growth.
/// Copy or transfer the data immediately after calling this function.
#[wasm_bindgen]
pub fn get_mesh_vertices(feature_index: usize) -> js_sys::Float32Array {
    with_mesh(feature_index, |mesh| unsafe {
        js_sys::Float32Array::view(&mesh.vertices)
    })
    .unwrap_or_else(|| js_sys::Float32Array::new_with_length(0))
}

/// Get mesh vertex normals as a Float32Array view into WASM memory.
///
/// Returns [nx0, ny0, nz0, nx1, ny1, nz1, ...].
#[wasm_bindgen]
pub fn get_mesh_normals(feature_index: usize) -> js_sys::Float32Array {
    with_mesh(feature_index, |mesh| unsafe {
        js_sys::Float32Array::view(&mesh.normals)
    })
    .unwrap_or_else(|| js_sys::Float32Array::new_with_length(0))
}

/// Get mesh triangle indices as a Uint32Array view into WASM memory.
///
/// Returns [i0, i1, i2, i3, i4, i5, ...] where each triple is a triangle.
#[wasm_bindgen]
pub fn get_mesh_indices(feature_index: usize) -> js_sys::Uint32Array {
    with_mesh(feature_index, |mesh| unsafe {
        js_sys::Uint32Array::view(&mesh.indices)
    })
    .unwrap_or_else(|| js_sys::Uint32Array::new_with_length(0))
}

/// Get the number of features with mesh data.
#[wasm_bindgen]
pub fn get_mesh_count() -> usize {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = match engine.as_ref() {
            Some(e) => e,
            None => return 0,
        };

        let mut count = 0;
        for feature in &engine.state.engine.tree.features {
            if let Some(result) = engine.state.engine.feature_results.get(&feature.id) {
                if result.outputs.iter().any(|(_, body)| body.mesh.is_some()) {
                    count += 1;
                }
            }
        }
        count
    })
}

/// Get which feature indices should be rendered.
///
/// Returns indices of features that have mesh data and are NOT consumed
/// by a later boolean operation. When a boolean union succeeds, the target
/// feature is consumed (its geometry is merged into the result feature).
/// When union fails, both features are renderable (multi-body mode).
#[wasm_bindgen]
pub fn get_renderable_feature_indices() -> js_sys::Uint32Array {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = match engine.as_ref() {
            Some(e) => e,
            None => return js_sys::Uint32Array::new_with_length(0),
        };

        let consumed = &engine.state.engine.consumed_features;
        let mut indices = Vec::new();

        for (i, feature) in engine.state.engine.tree.features.iter().enumerate() {
            if consumed.contains(&feature.id) {
                continue;
            }
            if let Some(result) = engine.state.engine.feature_results.get(&feature.id) {
                if result.outputs.iter().any(|(_, body)| body.mesh.is_some()) {
                    indices.push(i as u32);
                }
            }
        }

        let arr = js_sys::Uint32Array::new_with_length(indices.len() as u32);
        arr.copy_from(&indices);
        arr
    })
}

/// Get face data for a specific feature by index.
///
/// Returns a JSON array of face ranges enriched with GeomRef data.
/// Each entry contains a `geom_ref` (persistent geometry reference) plus
/// `start_index` and `end_index` into the mesh indices array.
///
/// For faces with role assignments from provenance, a Role-based selector is used.
/// For faces without roles, a Signature-based selector with a centroid fallback is used.
#[wasm_bindgen]
pub fn get_face_data(feature_index: usize) -> String {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = match engine.as_ref() {
            Some(e) => e,
            None => return "[]".to_string(),
        };

        let features = &engine.state.engine.tree.features;
        let feature = match features.get(feature_index) {
            Some(f) => f,
            None => return "[]".to_string(),
        };

        let feature_id = feature.id;
        let result = match engine.state.engine.feature_results.get(&feature_id) {
            Some(r) => r,
            None => return "[]".to_string(),
        };

        // Find the first output with a mesh
        let mut found_mesh = None;
        let mut found_key = None;
        for (key, body) in &result.outputs {
            if let Some(ref mesh) = body.mesh {
                found_mesh = Some(mesh);
                found_key = Some(key.clone());
                break;
            }
        }

        let mesh = match found_mesh {
            Some(m) => m,
            None => return "[]".to_string(),
        };
        let output_key = found_key.unwrap();

        let entries = build_face_entries(
            feature_id,
            &output_key,
            mesh,
            &result.provenance.role_assignments,
            &engine.state.engine,
            &engine.kernel,
            None,
        );
        serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string())
    })
}

/// Build face-range JSON entries (each with a persistent GeomRef anchored to the
/// given feature output). Shared by the per-feature and per-body face accessors.
// Seven inputs: the sixth and seventh (introspect, ghost) are the KV13 and
// Phase 3d-4 additions to a function that mirrors the face-entry wire shape.
#[allow(clippy::too_many_arguments)]
fn build_face_entries(
    feature_id: uuid::Uuid,
    output_key: &OutputKey,
    mesh: &RenderMesh,
    role_assignments: &[(KernelId, Role)],
    // KV13 F6b: resolve each face's INTRODUCING feature (through chained
    // booleans) for the face→feature UI.
    fe: &feature_engine::Engine,
    introspect: &dyn waffle_types::kernel::KernelIntrospect,
    // Phase 3d-4: a ghost body's refs carry the scope, and each planar face
    // reports its plane (centroid + normal, the engine's definition) in the
    // edited part's frame so a sketch started on it uses the SAME origin the
    // engine re-derives on rebuild.
    ghost: Option<&Ghost>,
) -> Vec<serde_json::Value> {
    // Lookup from KernelId → Role from provenance.
    let role_map: std::collections::HashMap<_, _> = role_assignments.iter().cloned().collect();

    let mut entries = Vec::new();
    for (face_idx, range) in mesh.face_ranges.iter().enumerate() {
        let geom_ref = if let Some(role) = role_map.get(&range.face_id) {
            // Role-based selector — stable across rebuilds
            GeomRef {
                kind: TopoKind::Face,
                anchor: Anchor::FeatureOutput {
                    feature_id,
                    output_key: output_key.clone(),
                },
                selector: Selector::Role {
                    role: role.clone(),
                    index: 0,
                },
                policy: ResolvePolicy::BestEffort,
                scope: None,
            }
        } else if ghost.is_some() {
            // A ghost face without a role (an imported body) must be
            // RESOLVABLE against the owning part's created-entity signatures
            // — `signature_similarity` ignores `adjacency_hash`, so the
            // index-only fallback below would match an arbitrary face. Carry
            // the face's geometric fingerprint in the PART's own frame (that
            // is what the part's provenance records).
            let sig = introspect.compute_signature(range.face_id, TopoKind::Face);
            GeomRef {
                kind: TopoKind::Face,
                anchor: Anchor::FeatureOutput {
                    feature_id,
                    output_key: output_key.clone(),
                },
                selector: Selector::Signature {
                    signature: TopoSignature {
                        surface_type: sig.surface_type.clone(),
                        area: sig.area,
                        centroid: sig.centroid,
                        normal: sig.normal,
                        bbox: None,
                        adjacency_hash: None,
                        length: None,
                    },
                },
                policy: ResolvePolicy::BestEffort,
                scope: None,
            }
        } else {
            // Signature-based fallback using face index
            GeomRef {
                kind: TopoKind::Face,
                anchor: Anchor::FeatureOutput {
                    feature_id,
                    output_key: output_key.clone(),
                },
                selector: Selector::Signature {
                    signature: TopoSignature {
                        surface_type: None,
                        area: None,
                        centroid: None,
                        normal: None,
                        bbox: None,
                        adjacency_hash: Some(face_idx as u64),
                        length: None,
                    },
                },
                policy: ResolvePolicy::BestEffort,
                scope: None,
            }
        };

        // KV13 F6b: the feature that INTRODUCED this face's geometry (through
        // chained booleans) — the original extrude/revolve, not the last
        // boolean. `null` when unresolved (e.g. carried before a rebuild point).
        let created_by_feature = fe
            .created_by_feature(introspect, range.face_id)
            .map(|id| id.to_string());

        let mut entry = serde_json::json!({
            "geom_ref": geom_ref,
            "start_index": range.start_index,
            "end_index": range.end_index,
            "created_by_feature": created_by_feature,
        });
        if let Some(g) = ghost {
            entry["geom_ref"] = serde_json::to_value(
                g.scoped(serde_json::from_value(entry["geom_ref"].clone()).expect("round-trip")),
            )
            .unwrap_or(serde_json::Value::Null);
            let sig = introspect.compute_signature(range.face_id, TopoKind::Face);
            if sig.surface_type.as_deref() == Some("planar") {
                if let (Some(c), Some(n)) = (sig.centroid, sig.normal) {
                    entry["plane"] = serde_json::json!({
                        "origin": g.relative.apply(c),
                        "normal": g.relative.apply_dir(n),
                    });
                }
            }
        }
        entries.push(entry);
    }
    entries
}

/// Get edge vertex positions as a Float32Array view into WASM memory.
///
/// Returns the edge polyline vertices for a feature as a zero-copy typed array.
/// The array contains [x0, y0, z0, x1, y1, z1, ...] where consecutive pairs
/// of vertices form line segments for rendering with THREE.LineSegments.
#[wasm_bindgen]
pub fn get_edge_vertices(feature_index: usize) -> js_sys::Float32Array {
    with_edges(feature_index, |edges| unsafe {
        js_sys::Float32Array::view(&edges.vertices)
    })
    .unwrap_or_else(|| js_sys::Float32Array::new_with_length(0))
}

/// Get edge range data for a specific feature by index.
///
/// Returns a JSON array of edge ranges enriched with GeomRef data.
/// Each entry contains a `geom_ref` (persistent geometry reference) plus
/// `start_index` and `end_index` into the edge vertices array (in vertex count,
/// not float count).
#[wasm_bindgen]
pub fn get_edge_data(feature_index: usize) -> String {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = match engine.as_ref() {
            Some(e) => e,
            None => return "[]".to_string(),
        };

        let features = &engine.state.engine.tree.features;
        let feature = match features.get(feature_index) {
            Some(f) => f,
            None => return "[]".to_string(),
        };

        let feature_id = feature.id;
        let result = match engine.state.engine.feature_results.get(&feature_id) {
            Some(r) => r,
            None => return "[]".to_string(),
        };

        // Find the first output with edge data
        let mut found_edges = None;
        let mut found_key = None;
        for (key, body) in &result.outputs {
            if let Some(ref edges) = body.edges {
                found_edges = Some(edges);
                found_key = Some(key.clone());
                break;
            }
        }

        let edges = match found_edges {
            Some(e) => e,
            None => return "[]".to_string(),
        };
        let output_key = found_key.unwrap();

        let entries = build_edge_entries(feature_id, &output_key, edges, None);
        serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string())
    })
}

/// Build edge-range JSON entries (each with a persistent GeomRef anchored to the
/// given feature output). Shared by the per-feature and per-body edge accessors.
fn build_edge_entries(
    feature_id: uuid::Uuid,
    output_key: &OutputKey,
    edges: &EdgeRenderData,
    ghost: Option<&Ghost>,
) -> Vec<serde_json::Value> {
    let mut entries = Vec::new();
    for (edge_idx, range) in edges.edge_ranges.iter().enumerate() {
        // Use Signature-based selector with edge index as adjacency_hash
        let mut geom_ref = GeomRef {
            kind: TopoKind::Edge,
            anchor: Anchor::FeatureOutput {
                feature_id,
                output_key: output_key.clone(),
            },
            selector: Selector::Signature {
                signature: TopoSignature {
                    surface_type: None,
                    area: None,
                    centroid: None,
                    normal: None,
                    bbox: None,
                    adjacency_hash: Some(edge_idx as u64),
                    length: None,
                },
            },
            policy: ResolvePolicy::BestEffort,
            scope: None,
        };
        if let Some(g) = ghost {
            geom_ref = g.scoped(geom_ref);
        }

        // EdgeOverlay.svelte expects start_index/end_index (vertex counts).
        // `curve` (Some for circular edges) lets sketch projection mint TRUE
        // Arc/Circle entities instead of polyline approximations.
        entries.push(serde_json::json!({
            "geom_ref": geom_ref,
            "start_index": range.start_vertex,
            "end_index": range.end_vertex,
            "curve": range.curve,
        }));
    }
    entries
}

/// Helper: access the mesh for a feature and apply a function to it.
fn with_mesh<T>(feature_index: usize, f: impl FnOnce(&RenderMesh) -> T) -> Option<T> {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = engine.as_ref()?;

        let features = &engine.state.engine.tree.features;
        let feature = features.get(feature_index)?;
        let result = engine.state.engine.feature_results.get(&feature.id)?;

        for (_key, body) in &result.outputs {
            if let Some(ref mesh) = body.mesh {
                return Some(f(mesh));
            }
        }
        None
    })
}

/// Helper: access the edge data for a feature and apply a function to it.
fn with_edges<T>(feature_index: usize, f: impl FnOnce(&EdgeRenderData) -> T) -> Option<T> {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = engine.as_ref()?;

        let features = &engine.state.engine.tree.features;
        let feature = features.get(feature_index)?;
        let result = engine.state.engine.feature_results.get(&feature.id)?;

        for (_key, body) in &result.outputs {
            if let Some(ref edges) = body.edges {
                return Some(f(edges));
            }
        }
        None
    })
}

// ── Per-body (per-output) accessors ────────────────────────────────────────
//
// A feature's `OpResult` can carry multiple bodies (`outputs: Vec<(OutputKey,
// BodyOutput)>`) — e.g. a boolean split. The per-feature accessors above
// collapse a feature to its first mesh-bearing output; these address each
// renderable body individually so multi-body features render every body.
//
// A "body" here is one mesh-bearing output of a non-consumed feature. The flat
// body index is the position in `collect_renderable_bodies`, which is stable
// for a given engine state and shared by every per-body accessor.

/// Address of one renderable body: which feature and which of its outputs —
/// and, in assembly mode, which instance (and which part engine) it belongs to.
struct BodyAddr {
    feature_index: usize,
    feature_id: uuid::Uuid,
    output_index: usize,
    /// `(leaf index in the assembly view, index into its parts)`; `None`
    /// for the live part.
    instance: Option<(usize, usize)>,
    /// Set for a GHOST body of an edit context (Phase 3d-4): its geometry is
    /// baked into the edited part's frame by `relative`, and every reference
    /// into it carries `scope`.
    ghost: Option<Ghost>,
}

#[derive(Clone)]
struct Ghost {
    relative: Transform,
    scope: RefScope,
}

impl Ghost {
    fn scoped(&self, mut geom_ref: GeomRef) -> GeomRef {
        geom_ref.scope = Some(self.scope.clone());
        geom_ref
    }

    fn bake_points(&self, flat: &[f32]) -> Vec<f32> {
        let mut out = Vec::with_capacity(flat.len());
        for p in flat.chunks_exact(3) {
            let q = self.relative.apply([p[0] as f64, p[1] as f64, p[2] as f64]);
            out.extend_from_slice(&[q[0] as f32, q[1] as f32, q[2] as f32]);
        }
        out
    }

    fn bake_dirs(&self, flat: &[f32]) -> Vec<f32> {
        let mut out = Vec::with_capacity(flat.len());
        for p in flat.chunks_exact(3) {
            let q = self
                .relative
                .apply_dir([p[0] as f64, p[1] as f64, p[2] as f64]);
            out.extend_from_slice(&[q[0] as f32, q[1] as f32, q[2] as f32]);
        }
        out
    }
}

/// The evaluated assembly whose leaves render: the open Assembly tab, or the
/// context an open Part is being edited in (its ghosts).
fn view_of(engine: &WasmEngine) -> Option<&crate::assembly_view::AssemblyView> {
    engine
        .state
        .assembly
        .as_ref()
        .or_else(|| engine.state.context_view.as_ref().map(|cv| &cv.view))
}

/// The engine a body address lives in.
fn engine_of<'a>(engine: &'a WasmEngine, addr: &BodyAddr) -> Option<&'a feature_engine::Engine> {
    match addr.instance {
        None => Some(&engine.state.engine),
        Some((_, part_idx)) => view_of(engine)
            .and_then(|v| v.parts.get(part_idx))
            .map(|(_, e)| e),
    }
}

fn bodies_of_engine(
    fe: &feature_engine::Engine,
    instance: Option<(usize, usize)>,
    ghost: Option<&Ghost>,
) -> Vec<BodyAddr> {
    let consumed = &fe.consumed_features;
    let mut bodies = Vec::new();
    for (fi, feature) in fe.tree.features.iter().enumerate() {
        if consumed.contains(&feature.id) {
            continue;
        }
        if let Some(result) = fe.feature_results.get(&feature.id) {
            for (oi, (_key, body)) in result.outputs.iter().enumerate() {
                if body.mesh.is_some() {
                    bodies.push(BodyAddr {
                        feature_index: fi,
                        feature_id: feature.id,
                        output_index: oi,
                        instance,
                        ghost: ghost.cloned(),
                    });
                }
            }
        }
    }
    bodies
}

/// Flat, ordered list of renderable bodies: every mesh-bearing output of every
/// feature that is not consumed by a later boolean. Order is feature order, then
/// output order within a feature. In assembly mode: every non-suppressed
/// instance's bodies, in instance order, each tagged with its instance. In
/// an edit context: the live part's bodies, then every OTHER instance's
/// bodies as ghosts baked into the part's frame.
fn collect_renderable_bodies(engine: &WasmEngine) -> Vec<BodyAddr> {
    if let Some(view) = engine.state.assembly.as_ref() {
        let mut bodies = Vec::new();
        for (li, leaf) in view.leaves.iter().enumerate() {
            if let Some((_, fe)) = view.parts.get(leaf.part) {
                bodies.extend(bodies_of_engine(fe, Some((li, leaf.part)), None));
            }
        }
        return bodies;
    }
    let mut bodies = bodies_of_engine(&engine.state.engine, None, None);
    if let Some(cv) = engine.state.context_view.as_ref() {
        for (li, relative) in &cv.ghosts {
            let Some(leaf) = cv.view.leaves.get(*li) else {
                continue;
            };
            if let Some((_, fe)) = cv.view.parts.get(leaf.part) {
                let ghost = Ghost {
                    relative: *relative,
                    scope: RefScope::in_assembly(cv.assembly_tab_id.clone(), leaf.path.clone()),
                };
                bodies.extend(bodies_of_engine(fe, Some((*li, leaf.part)), Some(&ghost)));
            }
        }
    }
    bodies
}

/// Access a body's mesh (and its address) by flat body index.
fn with_body_mesh<T>(body_index: usize, f: impl FnOnce(&RenderMesh, &BodyAddr) -> T) -> Option<T> {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = engine.as_ref()?;
        let addr = collect_renderable_bodies(engine)
            .into_iter()
            .nth(body_index)?;
        let result = engine_of(engine, &addr)?
            .feature_results
            .get(&addr.feature_id)?;
        let (_key, body) = result.outputs.get(addr.output_index)?;
        body.mesh.as_ref().map(|m| f(m, &addr))
    })
}

/// Access a body's edge data (and its address) by flat body index.
fn with_body_edges<T>(
    body_index: usize,
    f: impl FnOnce(&EdgeRenderData, &BodyAddr) -> T,
) -> Option<T> {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = engine.as_ref()?;
        let addr = collect_renderable_bodies(engine)
            .into_iter()
            .nth(body_index)?;
        let result = engine_of(engine, &addr)?
            .feature_results
            .get(&addr.feature_id)?;
        let (_key, body) = result.outputs.get(addr.output_index)?;
        body.edges.as_ref().map(|e| f(e, &addr))
    })
}

/// Number of renderable bodies (mesh-bearing outputs across non-consumed
/// features). This is the count the worker iterates for rendering.
#[wasm_bindgen]
pub fn get_body_count() -> usize {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        match engine.as_ref() {
            Some(e) => collect_renderable_bodies(e).len(),
            None => 0,
        }
    })
}

/// Metadata for every renderable body as a JSON array, in body-index order.
/// Each entry: `{ featureIndex, featureId, outputIndex, outputKey, bodyId, name }`.
///
/// `bodyId` (`"{featureId}/{outputKey.tag()}"`) is the body's persistent
/// identity — the key for selection and for the name-override registry. `name`
/// is the resolved display name: the user override if set, else the producing
/// feature's name (suffixed with an ordinal when one feature owns several
/// bodies). Naming is resolved here so the engine stays authoritative.
#[wasm_bindgen]
pub fn get_body_metadata() -> String {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = match engine.as_ref() {
            Some(e) => e,
            None => return "[]".to_string(),
        };
        let bodies = collect_renderable_bodies(engine);

        // How many rendered bodies each feature owns, for ordinal disambiguation.
        let mut totals: std::collections::HashMap<uuid::Uuid, usize> =
            std::collections::HashMap::new();
        for addr in &bodies {
            *totals.entry(addr.feature_id).or_insert(0) += 1;
        }

        let mut seen: std::collections::HashMap<uuid::Uuid, usize> =
            std::collections::HashMap::new();
        let mut entries = Vec::new();
        for addr in &bodies {
            let Some(fe) = engine_of(engine, addr) else {
                continue;
            };
            let tree = &fe.tree;
            let output_key = fe
                .feature_results
                .get(&addr.feature_id)
                .and_then(|r| r.outputs.get(addr.output_index))
                .map(|(k, _)| k.clone());

            let body_id = output_key
                .as_ref()
                .map(|k| feature_engine::types::FeatureTree::body_id(addr.feature_id, k))
                .map(|id| {
                    match addr
                        .instance
                        .and_then(|(li, _)| view_of(engine)?.leaves.get(li))
                    {
                        Some(leaf) => format!(
                            "{}/{id}",
                            leaf.path
                                .iter()
                                .map(|u| u.to_string())
                                .collect::<Vec<_>>()
                                .join("/")
                        ),
                        None => id,
                    }
                });

            // Ordinal among this feature's rendered bodies (1-based).
            let ordinal = {
                let n = seen.entry(addr.feature_id).or_insert(0);
                *n += 1;
                *n
            };
            let total = totals.get(&addr.feature_id).copied().unwrap_or(1);

            let name = body_id
                .as_deref()
                .and_then(|id| fe.display_body_name_override(id))
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    let base = tree
                        .features
                        .iter()
                        .find(|f| f.id == addr.feature_id)
                        .map(|f| f.name.clone())
                        .unwrap_or_else(|| "Body".to_string());
                    if total > 1 {
                        format!("{base} ({ordinal})")
                    } else {
                        base
                    }
                });

            let mut entry = serde_json::json!({
                "featureIndex": addr.feature_index,
                "featureId": addr.feature_id,
                "outputIndex": addr.output_index,
                "outputKey": output_key,
                "bodyId": body_id,
                "name": name,
            });
            if let (Some((li, _)), Some(view)) = (addr.instance, view_of(engine)) {
                if let Some(leaf) = view.leaves.get(li) {
                    let top = leaf.path[0];
                    let inst = view.tree.instance(top);
                    entry["instanceId"] = serde_json::json!(top);
                    entry["instancePath"] = serde_json::json!(leaf.path);
                    entry["instanceName"] = serde_json::json!(inst.map(|i| i.name.clone()));
                    entry["partTabId"] = serde_json::json!(inst.map(|i| i.source.tab_id.clone()));
                    // The LEAF's part (differs from `partTabId` for a member
                    // of a sub-assembly instance): what "edit in context" opens.
                    if let Some((part, _)) = view.parts.get(leaf.part) {
                        entry["leafPartTabId"] = serde_json::json!(part.tab_id);
                        entry["leafPartSourceId"] = serde_json::json!(part.source_id);
                    }
                    // A ghost's geometry is baked into the edited part's frame:
                    // no renderer-side placement.
                    entry["transform"] = match &addr.ghost {
                        Some(_) => serde_json::Value::Null,
                        None => {
                            serde_json::to_value(leaf.transform).unwrap_or(serde_json::Value::Null)
                        }
                    };
                    entry["context"] = serde_json::json!(addr.ghost.is_some());
                    if let Some(i) = inst {
                        let depth = if leaf.path.len() > 1 { " › …" } else { "" };
                        entry["name"] = serde_json::json!(format!(
                            "{}{depth} · {}",
                            i.name,
                            entry["name"].as_str().unwrap_or("Body")
                        ));
                    }
                }
            }
            entries.push(entry);
        }
        serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string())
    })
}

/// Body mesh vertex positions as a Float32Array view (by flat body index).
#[wasm_bindgen]
pub fn get_body_vertices(body_index: usize) -> js_sys::Float32Array {
    // A ghost's vertices are baked into the edited part's frame (a copy);
    // everything else is a zero-copy view.
    with_body_mesh(body_index, |mesh, addr| match &addr.ghost {
        Some(g) => js_sys::Float32Array::from(g.bake_points(&mesh.vertices).as_slice()),
        None => unsafe { js_sys::Float32Array::view(&mesh.vertices) },
    })
    .unwrap_or_else(|| js_sys::Float32Array::new_with_length(0))
}

/// Body mesh vertex normals as a Float32Array view (by flat body index).
#[wasm_bindgen]
pub fn get_body_normals(body_index: usize) -> js_sys::Float32Array {
    with_body_mesh(body_index, |mesh, addr| match &addr.ghost {
        Some(g) => js_sys::Float32Array::from(g.bake_dirs(&mesh.normals).as_slice()),
        None => unsafe { js_sys::Float32Array::view(&mesh.normals) },
    })
    .unwrap_or_else(|| js_sys::Float32Array::new_with_length(0))
}

/// Body mesh triangle indices as a Uint32Array view (by flat body index).
#[wasm_bindgen]
pub fn get_body_indices(body_index: usize) -> js_sys::Uint32Array {
    with_body_mesh(body_index, |mesh, _| unsafe {
        js_sys::Uint32Array::view(&mesh.indices)
    })
    .unwrap_or_else(|| js_sys::Uint32Array::new_with_length(0))
}

/// Body face-range data (GeomRef-enriched) as JSON, by flat body index.
#[wasm_bindgen]
pub fn get_body_face_data(body_index: usize) -> String {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = match engine.as_ref() {
            Some(e) => e,
            None => return "[]".to_string(),
        };
        let addr = match collect_renderable_bodies(engine)
            .into_iter()
            .nth(body_index)
        {
            Some(a) => a,
            None => return "[]".to_string(),
        };
        let Some(fe) = engine_of(engine, &addr) else {
            return "[]".to_string();
        };
        let result = match fe.feature_results.get(&addr.feature_id) {
            Some(r) => r,
            None => return "[]".to_string(),
        };
        let (key, body) = match result.outputs.get(addr.output_index) {
            Some(o) => o,
            None => return "[]".to_string(),
        };
        let mesh = match body.mesh.as_ref() {
            Some(m) => m,
            None => return "[]".to_string(),
        };
        let entries = build_face_entries(
            addr.feature_id,
            key,
            mesh,
            &result.provenance.role_assignments,
            fe,
            &engine.kernel,
            addr.ghost.as_ref(),
        );
        serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string())
    })
}

/// Body edge vertex positions as a Float32Array view (by flat body index).
#[wasm_bindgen]
pub fn get_body_edge_vertices(body_index: usize) -> js_sys::Float32Array {
    with_body_edges(body_index, |edges, addr| match &addr.ghost {
        Some(g) => js_sys::Float32Array::from(g.bake_points(&edges.vertices).as_slice()),
        None => unsafe { js_sys::Float32Array::view(&edges.vertices) },
    })
    .unwrap_or_else(|| js_sys::Float32Array::new_with_length(0))
}

/// Body edge-range data (GeomRef-enriched) as JSON, by flat body index.
#[wasm_bindgen]
pub fn get_body_edge_data(body_index: usize) -> String {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = match engine.as_ref() {
            Some(e) => e,
            None => return "[]".to_string(),
        };
        let addr = match collect_renderable_bodies(engine)
            .into_iter()
            .nth(body_index)
        {
            Some(a) => a,
            None => return "[]".to_string(),
        };
        let Some(fe) = engine_of(engine, &addr) else {
            return "[]".to_string();
        };
        let result = match fe.feature_results.get(&addr.feature_id) {
            Some(r) => r,
            None => return "[]".to_string(),
        };
        let (key, body) = match result.outputs.get(addr.output_index) {
            Some(o) => o,
            None => return "[]".to_string(),
        };
        let edges = match body.edges.as_ref() {
            Some(e) => e,
            None => return "[]".to_string(),
        };
        let entries = build_edge_entries(addr.feature_id, key, edges, addr.ghost.as_ref());
        serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string())
    })
}

/// Tessellate feature results that have a solid handle but no mesh data.
/// PR-VIZ-3a-fix: thin shim that delegates to
/// `crate::tessellation_runner::tessellate_missing_meshes` (extracted to a
/// non-wasm-gated module so it can be exercised by native integration tests).
fn tessellate_missing_meshes(state: &mut EngineState, kernel: &mut impl KernelBundle) {
    crate::tessellation_runner::tessellate_missing_meshes(state, kernel)
}
