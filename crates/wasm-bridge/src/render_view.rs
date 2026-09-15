//! The render view of an engine state: which bodies render, their display
//! names, GeomRef-enriched face and edge ranges, and ghost baking.
//!
//! Target-independent (native and wasm32). `wasm_api` hands it to the web
//! worker as typed-array views and JSON strings; a native host serves the
//! same data from the same code (`specs/waffle_server_mode.md` §2.3 S0).
//! `tests/render_view_parity.rs` pins it to the bundle's output.

use std::borrow::Cow;

use feature_engine::assembly::Transform;
use waffle_types::kernel::{EdgeRenderData, KernelId, KernelIntrospect, RenderMesh};
use waffle_types::{
    Anchor, GeomRef, OutputKey, RefScope, ResolvePolicy, Role, Selector, TopoKind, TopoSignature,
};

use crate::assembly_view::AssemblyView;
use crate::engine_state::EngineState;

// ── Per-feature accessors ──────────────────────────────────────────────────
//
// These collapse a feature to its first mesh-bearing output. The worker uses
// them only for bundles without the per-body accessors below.

/// The first output mesh of the feature at `feature_index`.
pub fn feature_mesh(state: &EngineState, feature_index: usize) -> Option<&RenderMesh> {
    let feature = state.engine.tree.features.get(feature_index)?;
    let result = state.engine.feature_results.get(&feature.id)?;
    result
        .outputs
        .iter()
        .find_map(|(_, body)| body.mesh.as_ref())
}

/// The first output edge data of the feature at `feature_index`.
pub fn feature_edges(state: &EngineState, feature_index: usize) -> Option<&EdgeRenderData> {
    let feature = state.engine.tree.features.get(feature_index)?;
    let result = state.engine.feature_results.get(&feature.id)?;
    result
        .outputs
        .iter()
        .find_map(|(_, body)| body.edges.as_ref())
}

/// The first output mesh of a feature as JSON, or a JSON `error` object.
pub fn feature_mesh_json(state: &EngineState, feature_index: usize) -> String {
    if feature_index >= state.engine.tree.features.len() {
        return r#"{"error":"Feature index out of range"}"#.to_string();
    }
    match feature_mesh(state, feature_index) {
        Some(mesh) => serde_json::to_string(mesh).unwrap_or_default(),
        None => r#"{"error":"No mesh for this feature"}"#.to_string(),
    }
}

/// The number of features with mesh data.
pub fn mesh_count(state: &EngineState) -> usize {
    let fe = &state.engine;
    fe.tree
        .features
        .iter()
        .filter(|feature| {
            fe.feature_results
                .get(&feature.id)
                .is_some_and(|r| r.outputs.iter().any(|(_, body)| body.mesh.is_some()))
        })
        .count()
}

/// Indices of features that have mesh data and are NOT consumed by a later
/// boolean operation. When a boolean union succeeds, the target feature is
/// consumed (its geometry is merged into the result feature). When union
/// fails, both features are renderable (multi-body mode).
pub fn renderable_feature_indices(state: &EngineState) -> Vec<u32> {
    let fe = &state.engine;
    let mut indices = Vec::new();
    for (i, feature) in fe.tree.features.iter().enumerate() {
        if fe.consumed_features.contains(&feature.id) {
            continue;
        }
        if let Some(result) = fe.feature_results.get(&feature.id) {
            if result.outputs.iter().any(|(_, body)| body.mesh.is_some()) {
                indices.push(i as u32);
            }
        }
    }
    indices
}

/// Face ranges of a feature's first mesh-bearing output, enriched with GeomRef
/// data. Each entry contains a `geom_ref` (persistent geometry reference) plus
/// `start_index` and `end_index` into the mesh indices array.
///
/// For faces with role assignments from provenance, a Role-based selector is used.
/// For faces without roles, a Signature-based selector with a centroid fallback is used.
pub fn feature_face_entries(
    state: &EngineState,
    introspect: &dyn KernelIntrospect,
    feature_index: usize,
) -> Vec<serde_json::Value> {
    let Some(feature) = state.engine.tree.features.get(feature_index) else {
        return Vec::new();
    };
    let Some(result) = state.engine.feature_results.get(&feature.id) else {
        return Vec::new();
    };
    let Some((output_key, mesh)) = result
        .outputs
        .iter()
        .find_map(|(key, body)| body.mesh.as_ref().map(|m| (key, m)))
    else {
        return Vec::new();
    };
    build_face_entries(
        feature.id,
        output_key,
        mesh,
        &result.provenance.role_assignments,
        &state.engine,
        introspect,
        None,
    )
}

/// Edge ranges of a feature's first edge-bearing output, enriched with GeomRef
/// data. `start_index`/`end_index` index the edge vertices array in vertex
/// count, not float count.
pub fn feature_edge_entries(state: &EngineState, feature_index: usize) -> Vec<serde_json::Value> {
    let Some(feature) = state.engine.tree.features.get(feature_index) else {
        return Vec::new();
    };
    let Some(result) = state.engine.feature_results.get(&feature.id) else {
        return Vec::new();
    };
    let Some((output_key, edges)) = result
        .outputs
        .iter()
        .find_map(|(key, body)| body.edges.as_ref().map(|e| (key, e)))
    else {
        return Vec::new();
    };
    build_edge_entries(feature.id, output_key, edges, None)
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
    introspect: &dyn KernelIntrospect,
    // Phase 3d-4: a ghost body's refs carry the scope, and each planar face
    // reports its plane (centroid + normal, the engine's definition) in the
    // edited part's frame so a sketch started on it uses the SAME origin the
    // engine re-derives on rebuild.
    ghost: Option<&Ghost>,
) -> Vec<serde_json::Value> {
    // The refs come from the builder `ListFaces` shares (ICR-3). A ghost's
    // roleless faces carry their fingerprint in the PART's own frame (that is
    // what the part's provenance records).
    let refs = crate::face_refs::face_geom_refs(
        feature_id,
        output_key,
        mesh,
        role_assignments,
        introspect,
        ghost.is_some(),
    );

    let mut entries = Vec::new();
    for (range, (_, geom_ref)) in mesh.face_ranges.iter().zip(refs) {
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
pub struct BodyAddr {
    pub feature_index: usize,
    pub feature_id: uuid::Uuid,
    pub output_index: usize,
    /// `(leaf index in the assembly view, index into its parts)`; `None`
    /// for the live part.
    pub instance: Option<(usize, usize)>,
    /// Set for a GHOST body of an edit context (Phase 3d-4): its geometry is
    /// baked into the edited part's frame by `relative`, and every reference
    /// into it carries `scope`.
    pub ghost: Option<Ghost>,
}

#[derive(Clone)]
pub struct Ghost {
    pub relative: Transform,
    pub scope: RefScope,
}

impl Ghost {
    pub fn scoped(&self, mut geom_ref: GeomRef) -> GeomRef {
        geom_ref.scope = Some(self.scope.clone());
        geom_ref
    }

    pub fn bake_points(&self, flat: &[f32]) -> Vec<f32> {
        let mut out = Vec::with_capacity(flat.len());
        for p in flat.chunks_exact(3) {
            let q = self.relative.apply([p[0] as f64, p[1] as f64, p[2] as f64]);
            out.extend_from_slice(&[q[0] as f32, q[1] as f32, q[2] as f32]);
        }
        out
    }

    pub fn bake_dirs(&self, flat: &[f32]) -> Vec<f32> {
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
pub fn view_of(state: &EngineState) -> Option<&AssemblyView> {
    state
        .assembly
        .as_ref()
        .or_else(|| state.context_view.as_ref().map(|cv| &cv.view))
}

/// The engine a body address lives in.
pub fn engine_of<'a>(
    state: &'a EngineState,
    addr: &BodyAddr,
) -> Option<&'a feature_engine::Engine> {
    match addr.instance {
        None => Some(&state.engine),
        Some((_, part_idx)) => view_of(state)
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
pub fn collect_renderable_bodies(state: &EngineState) -> Vec<BodyAddr> {
    if let Some(view) = state.assembly.as_ref() {
        let mut bodies = Vec::new();
        for (li, leaf) in view.leaves.iter().enumerate() {
            if let Some((_, fe)) = view.parts.get(leaf.part) {
                bodies.extend(bodies_of_engine(fe, Some((li, leaf.part)), None));
            }
        }
        return bodies;
    }
    let mut bodies = bodies_of_engine(&state.engine, None, None);
    if let Some(cv) = state.context_view.as_ref() {
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

/// A body's mesh and its address, by flat body index.
pub fn body_mesh(state: &EngineState, body_index: usize) -> Option<(&RenderMesh, BodyAddr)> {
    let addr = collect_renderable_bodies(state)
        .into_iter()
        .nth(body_index)?;
    let result = engine_of(state, &addr)?
        .feature_results
        .get(&addr.feature_id)?;
    let (_key, body) = result.outputs.get(addr.output_index)?;
    body.mesh.as_ref().map(|m| (m, addr))
}

/// A body's edge data and its address, by flat body index.
pub fn body_edges(state: &EngineState, body_index: usize) -> Option<(&EdgeRenderData, BodyAddr)> {
    let addr = collect_renderable_bodies(state)
        .into_iter()
        .nth(body_index)?;
    let result = engine_of(state, &addr)?
        .feature_results
        .get(&addr.feature_id)?;
    let (_key, body) = result.outputs.get(addr.output_index)?;
    body.edges.as_ref().map(|e| (e, addr))
}

/// Body vertex positions. A ghost's vertices are baked into the edited part's
/// frame (a copy); everything else is borrowed.
pub fn body_vertices(state: &EngineState, body_index: usize) -> Option<Cow<'_, [f32]>> {
    body_mesh(state, body_index).map(|(mesh, addr)| match &addr.ghost {
        Some(g) => Cow::Owned(g.bake_points(&mesh.vertices)),
        None => Cow::Borrowed(&mesh.vertices[..]),
    })
}

/// Body vertex normals (baked like `body_vertices`).
pub fn body_normals(state: &EngineState, body_index: usize) -> Option<Cow<'_, [f32]>> {
    body_mesh(state, body_index).map(|(mesh, addr)| match &addr.ghost {
        Some(g) => Cow::Owned(g.bake_dirs(&mesh.normals)),
        None => Cow::Borrowed(&mesh.normals[..]),
    })
}

/// Body triangle indices.
pub fn body_indices(state: &EngineState, body_index: usize) -> Option<&[u32]> {
    body_mesh(state, body_index).map(|(mesh, _)| &mesh.indices[..])
}

/// Body edge polyline vertices (baked like `body_vertices`).
pub fn body_edge_vertices(state: &EngineState, body_index: usize) -> Option<Cow<'_, [f32]>> {
    body_edges(state, body_index).map(|(edges, addr)| match &addr.ghost {
        Some(g) => Cow::Owned(g.bake_points(&edges.vertices)),
        None => Cow::Borrowed(&edges.vertices[..]),
    })
}

/// Metadata for every renderable body, in body-index order.
/// Each entry: `{ featureIndex, featureId, outputIndex, outputKey, bodyId, name }`.
///
/// `bodyId` (`"{featureId}/{outputKey.tag()}"`) is the body's persistent
/// identity — the key for selection and for the name-override registry. `name`
/// is the resolved display name: the user override if set, else the producing
/// feature's name (suffixed with an ordinal when one feature owns several
/// bodies). Naming is resolved here so the engine stays authoritative.
pub fn body_metadata(state: &EngineState) -> Vec<serde_json::Value> {
    let bodies = collect_renderable_bodies(state);

    // How many rendered bodies each feature owns, for ordinal disambiguation.
    let mut totals: std::collections::HashMap<uuid::Uuid, usize> = std::collections::HashMap::new();
    for addr in &bodies {
        *totals.entry(addr.feature_id).or_insert(0) += 1;
    }

    let mut seen: std::collections::HashMap<uuid::Uuid, usize> = std::collections::HashMap::new();
    let mut entries = Vec::new();
    for addr in &bodies {
        let Some(fe) = engine_of(state, addr) else {
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
                    .and_then(|(li, _)| view_of(state)?.leaves.get(li))
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
        if let (Some((li, _)), Some(view)) = (addr.instance, view_of(state)) {
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
                    None => serde_json::to_value(leaf.transform).unwrap_or(serde_json::Value::Null),
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
    entries
}

/// Face ranges (GeomRef-enriched) of one body, by flat body index.
pub fn body_face_entries(
    state: &EngineState,
    introspect: &dyn KernelIntrospect,
    body_index: usize,
) -> Vec<serde_json::Value> {
    let Some(addr) = collect_renderable_bodies(state).into_iter().nth(body_index) else {
        return Vec::new();
    };
    let Some(fe) = engine_of(state, &addr) else {
        return Vec::new();
    };
    let Some(result) = fe.feature_results.get(&addr.feature_id) else {
        return Vec::new();
    };
    let Some((key, body)) = result.outputs.get(addr.output_index) else {
        return Vec::new();
    };
    let Some(mesh) = body.mesh.as_ref() else {
        return Vec::new();
    };
    build_face_entries(
        addr.feature_id,
        key,
        mesh,
        &result.provenance.role_assignments,
        fe,
        introspect,
        addr.ghost.as_ref(),
    )
}

/// Edge ranges (GeomRef-enriched) of one body, by flat body index.
pub fn body_edge_entries(state: &EngineState, body_index: usize) -> Vec<serde_json::Value> {
    let Some(addr) = collect_renderable_bodies(state).into_iter().nth(body_index) else {
        return Vec::new();
    };
    let Some(fe) = engine_of(state, &addr) else {
        return Vec::new();
    };
    let Some(result) = fe.feature_results.get(&addr.feature_id) else {
        return Vec::new();
    };
    let Some((key, body)) = result.outputs.get(addr.output_index) else {
        return Vec::new();
    };
    let Some(edges) = body.edges.as_ref() else {
        return Vec::new();
    };
    build_edge_entries(addr.feature_id, key, edges, addr.ghost.as_ref())
}
