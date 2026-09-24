//! Viewer sync, the host's half (`specs/waffle_server_mode.md` §4,
//! `waffle-viewer/1`): the document as a **snapshot** — everything a viewer
//! draws except the geometry — and the geometry as content-addressed
//! **blobs**, one per rendered body, that a viewer fetches only when it does
//! not hold them.
//!
//! "Stream meshes, never the op log" (§4.2): a viewer never runs the kernel.
//! Every body's mesh is encoded once per snapshot into `raw/1` bytes and
//! named by their hash, so an unchanged body keeps its id across edits,
//! reconnects and host restarts, and a viewer's cache answers for it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::{json, Map, Value};
use sha1::{Digest, Sha1};
use waffle_types::kernel::KernelIntrospect;
use wasm_bridge::render_view;
use wasm_bridge::EngineState;

use crate::host::Host;

pub const PROTOCOL: &str = "waffle-viewer/1";

/// The canonical encoding (§4.5), the one a `mesh_id` names: a JSON header,
/// then the buffers the browser worker transfers today — `Float32`
/// positions, `Float32` normals, `Uint32` indices, `Float32` edge polylines —
/// all little-endian, the header padded so every buffer starts 4-byte
/// aligned. The compact `mq/1` (`crate::mq`) is derived from it on request:
/// the id is the content's, the encoding is the viewer's choice.
pub const ENCODING: &str = "raw/1";

/// Every encoding a blob can be asked for.
pub const ENCODINGS: &[&str] = &[ENCODING, crate::mq::ENCODING];

/// Blobs kept beyond the current snapshot's, so a viewer that reconnects
/// after an edit still finds the body it asks for a moment later.
const STORE_CAP_BYTES: usize = 512 << 20;

/// The mesh blobs of recent snapshots by `mesh_id`.
#[derive(Default)]
pub struct MeshStore {
    blobs: HashMap<String, Arc<Vec<u8>>>,
    /// `mq/1` transcodings, made on first request and kept while the raw blob is.
    compact: HashMap<String, Arc<Vec<u8>>>,
    /// Insertion order, oldest first, for eviction.
    order: Vec<String>,
    bytes: usize,
    /// The current snapshot's ids: never evicted.
    current: HashSet<String>,
}

impl MeshStore {
    pub fn get(&self, mesh_id: &str) -> Option<Arc<Vec<u8>>> {
        self.blobs.get(mesh_id).cloned()
    }

    /// The blob in `encoding`: `raw/1` as stored, `mq/1` transcoded once.
    /// `None` for an unknown id or encoding.
    pub fn get_encoded(&mut self, mesh_id: &str, encoding: &str) -> Option<Arc<Vec<u8>>> {
        if encoding == ENCODING {
            return self.get(mesh_id);
        }
        if encoding != crate::mq::ENCODING {
            return None;
        }
        if let Some(done) = self.compact.get(mesh_id) {
            return Some(done.clone());
        }
        let raw = self.blobs.get(mesh_id)?;
        let encoded = Arc::new(crate::mq::encode(raw)?);
        self.compact.insert(mesh_id.to_string(), encoded.clone());
        Some(encoded)
    }

    pub fn len(&self) -> usize {
        self.blobs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blobs.is_empty()
    }

    fn insert(&mut self, mesh_id: &str, bytes: Vec<u8>) {
        if self.blobs.contains_key(mesh_id) {
            return;
        }
        self.bytes += bytes.len();
        self.blobs.insert(mesh_id.to_string(), Arc::new(bytes));
        self.order.push(mesh_id.to_string());
    }

    /// Pin the current snapshot's ids and evict the oldest others while the
    /// store is over its cap.
    fn settle(&mut self, current: HashSet<String>) {
        self.current = current;
        let mut kept = Vec::with_capacity(self.order.len());
        let mut order = std::mem::take(&mut self.order);
        order.reverse(); // newest first
        let mut over = self.bytes.saturating_sub(STORE_CAP_BYTES);
        let mut evicted = Vec::new();
        for id in order {
            if over > 0 && !self.current.contains(&id) {
                if let Some(blob) = self.blobs.remove(&id) {
                    over = over.saturating_sub(blob.len());
                    self.bytes -= blob.len();
                    self.compact.remove(&id);
                    evicted.push(id);
                    continue;
                }
            }
            kept.push(id);
        }
        kept.reverse();
        self.order = kept;
    }
}

/// One body encoded: its `raw/1` bytes, triangle count and bounding box.
struct Encoded {
    bytes: Vec<u8>,
    triangles: usize,
    min: [f64; 3],
    max: [f64; 3],
}

fn encode_body(
    state: &EngineState,
    introspect: &dyn KernelIntrospect,
    addr: &render_view::BodyAddr,
) -> Option<Encoded> {
    let vertices = render_view::body_vertices_at(state, addr)?;
    if vertices.is_empty() {
        return None;
    }
    let normals = render_view::body_normals_at(state, addr)?;
    let indices = render_view::body_indices_at(state, addr)?;
    let edge_vertices = render_view::body_edge_vertices_at(state, addr).unwrap_or_default();
    let header = json!({
        "encoding": ENCODING,
        "vertex_count": vertices.len() / 3,
        "index_count": indices.len(),
        "edge_vertex_count": edge_vertices.len() / 3,
        "face_ranges": render_view::body_face_entries_at(state, introspect, addr),
        "edge_ranges": render_view::body_edge_entries_at(state, addr),
    });
    let header = serde_json::to_vec(&header).ok()?;
    let padded = header.len().div_ceil(4) * 4;
    let mut out = Vec::with_capacity(
        4 + padded + (vertices.len() + normals.len() + edge_vertices.len()) * 4 + indices.len() * 4,
    );
    out.extend_from_slice(&(padded as u32).to_le_bytes());
    out.extend_from_slice(&header);
    out.resize(4 + padded, 0);
    for v in vertices.iter() {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for n in normals.iter() {
        out.extend_from_slice(&n.to_le_bytes());
    }
    for i in indices {
        out.extend_from_slice(&i.to_le_bytes());
    }
    for e in edge_vertices.iter() {
        out.extend_from_slice(&e.to_le_bytes());
    }
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in vertices.chunks_exact(3) {
        for k in 0..3 {
            min[k] = min[k].min(f64::from(p[k]));
            max[k] = max[k].max(f64::from(p[k]));
        }
    }
    Some(Encoded {
        bytes: out,
        triangles: indices.len() / 3,
        min,
        max,
    })
}

fn mesh_id(bytes: &[u8]) -> String {
    format!("{:x}", Sha1::digest(bytes))
}

impl Host {
    /// The tools after which the document a viewer shows has changed:
    /// every mutating engine tool, and the storage tools that open another
    /// document. The host pushes a snapshot after each.
    pub fn model_changed(name: &str) -> bool {
        wasm_bridge::tools::mutates(name)
            || matches!(name, "document_open" | "document_new" | "document_import")
    }

    /// Everything a viewer draws except the geometry (§4.3 `snapshot`): the
    /// document, the tree, errors and warnings, and the body list with each
    /// body's `mesh_id`. Encodes and stores every rendered body's blob, so
    /// [`Host::blob`] answers for every id the snapshot names.
    pub fn snapshot(&mut self) -> Value {
        let epoch = self.epoch().to_string();
        let revision = self.revision();
        let (state, kernel, meshes) = self.viewer_parts();
        let introspect: &dyn KernelIntrospect = kernel;
        // Collected ONCE and shared with every accessor below: resolving each
        // body by its flat index instead would re-walk this list six times per
        // body (docs/notes/eiffel/FEATURE_NOTES.md §0).
        let addrs = render_view::collect_renderable_bodies(state);
        let metadata = render_view::body_metadata_for(state, &addrs);
        let mut bodies = Vec::with_capacity(metadata.len());
        let mut current = HashSet::new();
        for (index, meta) in metadata.into_iter().enumerate() {
            let Some(addr) = addrs.get(index) else {
                continue;
            };
            let Some(Encoded {
                bytes,
                triangles,
                min,
                max,
            }) = encode_body(state, introspect, addr)
            else {
                // The worker skips a body with no vertices; so does a snapshot.
                continue;
            };
            let id = mesh_id(&bytes);
            let byte_length = bytes.len();
            meshes.insert(&id, bytes);
            current.insert(id.clone());
            let mut entry = match meta {
                Value::Object(map) => map,
                other => Map::from_iter([("meta".to_string(), other)]),
            };
            entry.insert("body_index".into(), json!(index));
            entry.insert("mesh_id".into(), json!(id));
            entry.insert("encoding".into(), json!(ENCODING));
            entry.insert("byte_length".into(), json!(byte_length));
            entry.insert("triangle_count".into(), json!(triangles));
            entry.insert("bbox".into(), json!({ "min": min, "max": max }));
            bodies.push(Value::Object(entry));
        }
        meshes.settle(current);

        let document = wasm_bridge::dispatch::document_info(state);
        let errors: Vec<Value> = state
            .engine
            .errors
            .iter()
            .map(|(id, message)| json!({ "feature_id": id, "message": message }))
            .collect();
        json!({
            "type": "snapshot",
            "protocol": PROTOCOL,
            "epoch": epoch,
            "revision": revision,
            "document": document,
            "tree": state.engine.tree,
            "errors": errors,
            "feature_errors": state.engine.feature_errors,
            "warnings": state.engine.warnings,
            "consumed_features": state.engine.consumed_features.iter().copied().collect::<Vec<_>>(),
            "sources": wasm_bridge::dispatch::source_statuses(state),
            "assembly": wasm_bridge::dispatch::assembly_status(state),
            "bodies": bodies,
        })
    }

    /// A blob by its id, if a recent snapshot named it, in `raw/1`.
    pub fn blob(&self, mesh_id: &str) -> Option<Arc<Vec<u8>>> {
        self.meshes().get(mesh_id)
    }

    /// A blob by its id in the encoding a viewer asked for (§4.5); `None`
    /// for an id no recent snapshot named or an encoding the host lacks.
    pub fn blob_encoded(&mut self, mesh_id: &str, encoding: &str) -> Option<Arc<Vec<u8>>> {
        self.meshes_mut().get_encoded(mesh_id, encoding)
    }
}
