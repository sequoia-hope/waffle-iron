//! One document session on the real kernel, and the tool router in front of
//! it (`specs/waffle_server_mode.md` §3.3 "Host mode" column).

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{json, Value};
use wasm_bridge::messages::{EngineToUi, UiToEngine};
use wasm_bridge::process::process_message;
use wasm_bridge::tools::{mutates, MIGRATED};
use wasm_bridge::{EngineState, ExportFile, ToolResult};

use crate::documents;

/// The storage tools the host implements over its file provider.
pub const DOCUMENT_TOOLS: &[&str] = &[
    "document_info",
    "storage_list",
    "document_open",
    "document_new",
    "document_import",
    "document_save",
];

/// Page tools that read or drive a viewer (§3.3): refused until P-D.
pub const VIEWER_TOOLS: &[&str] = &["selection_get", "viewport_view", "viewport_capture"];

/// The host's build, reported in `ready.host_build`.
pub fn host_build() -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "crate": env!("CARGO_PKG_NAME"),
    })
}

/// A document session with its storage directory.
pub struct Host {
    state: EngineState,
    kernel: kernel_v2::KernelV2Adapter,
    documents: PathBuf,
    epoch: String,
    started: Instant,
    /// The mesh blobs of recent snapshots (viewer sync, `viewer.rs`).
    meshes: crate::viewer::MeshStore,
    /// Names the document state a viewer holds (§4.1): incremented after
    /// every tool that changed the document. The session's own revision
    /// counts tab-level commits only, so it is not this.
    revision: u64,
}

impl Host {
    /// Open a host over `documents` (created if missing, with its
    /// `exports/` subdirectory). The session starts on the engine's empty
    /// bootstrap document, exactly as a fresh browser tab does.
    pub fn new(documents: impl Into<PathBuf>) -> std::io::Result<Self> {
        let documents = documents.into();
        std::fs::create_dir_all(documents.join("exports"))?;
        Ok(Self {
            state: EngineState::new(),
            kernel: kernel_v2::KernelV2Adapter::new(),
            documents,
            epoch: uuid::Uuid::new_v4().to_string(),
            started: Instant::now(),
            meshes: Default::default(),
            revision: 0,
        })
    }

    /// The document state's name for viewers (see `revision`).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The state, the kernel and the mesh store at once, for the snapshot
    /// (the borrow checker wants them split before the loop).
    pub(crate) fn viewer_parts(
        &mut self,
    ) -> (
        &EngineState,
        &kernel_v2::KernelV2Adapter,
        &mut crate::viewer::MeshStore,
    ) {
        (&self.state, &self.kernel, &mut self.meshes)
    }

    pub(crate) fn meshes(&self) -> &crate::viewer::MeshStore {
        &self.meshes
    }

    pub(crate) fn meshes_mut(&mut self) -> &mut crate::viewer::MeshStore {
        &mut self.meshes
    }

    /// A new epoch: the document a viewer holds is not this one any more
    /// (§4.1 — "epoch changes when the host process restarts or the document
    /// is reopened"), so a viewer's `have` can never match across it.
    fn new_epoch(&mut self) {
        self.epoch = uuid::Uuid::new_v4().to_string();
    }

    /// Every tool this host answers. The relay lists exactly these (plus its
    /// own) in host mode, so a call for anything else never reaches it.
    pub fn tools() -> Vec<&'static str> {
        MIGRATED.iter().chain(DOCUMENT_TOOLS).copied().collect()
    }

    /// The `ready` frame header (§3.4).
    pub fn ready_frame(&self) -> Value {
        json!({
            "type": "ready",
            "protocol": crate::frames::PROTOCOL,
            "host_build": host_build(),
            "epoch": self.epoch,
            "tools": Self::tools(),
            "documents": self.documents.to_string_lossy(),
        })
    }

    pub fn epoch(&self) -> &str {
        &self.epoch
    }

    pub fn documents_dir(&self) -> &Path {
        &self.documents
    }

    pub(crate) fn state(&self) -> &EngineState {
        &self.state
    }

    /// Run one engine message through the shared pipeline.
    pub(crate) fn engine(&mut self, msg: UiToEngine) -> EngineToUi {
        let started = self.started;
        let now_ms = move || started.elapsed().as_secs_f64() * 1000.0;
        let log = |line: &str| eprintln!("waffle-host: {line}");
        process_message(&mut self.state, &mut self.kernel, msg, &now_ms, &log)
    }

    /// Run one agent tool. Never panics on a bad name or bad arguments: every
    /// refusal is a typed error result. A kernel panic is NOT caught here —
    /// it ends the process, which is the relay's crash-isolation contract
    /// (§2.4, oracle H4): the relay answers `EngineCrashed`, restarts the
    /// host and reopens the autosaved document.
    pub fn call(&mut self, name: &str, arguments: &Value, context: Option<&Value>) -> ToolResult {
        let result = self.route(name, arguments, context);
        if !result.is_error && Self::model_changed(name) {
            self.revision += 1;
        }
        result
    }

    fn route(&mut self, name: &str, arguments: &Value, context: Option<&Value>) -> ToolResult {
        if MIGRATED.contains(&name) {
            return self.engine_tool(name, arguments, context);
        }
        if DOCUMENT_TOOLS.contains(&name) {
            let result = documents::run(self, name, arguments);
            if !result.is_error
                && matches!(name, "document_open" | "document_new" | "document_import")
            {
                self.new_epoch();
            }
            return result;
        }
        if VIEWER_TOOLS.contains(&name) {
            return ToolResult::error(
                "ViewerUnavailable",
                &format!(
                    "{name} needs a viewer showing the document; the native host has none \
                     attached (server mode P-D)."
                ),
                json!({ "tool": name }),
            );
        }
        ToolResult::error(
            "ToolUnavailable",
            &format!("This host has no tool named \"{name}\"."),
            json!({ "tool": name }),
        )
    }

    fn engine_tool(
        &mut self,
        name: &str,
        arguments: &Value,
        context: Option<&Value>,
    ) -> ToolResult {
        let response = self.engine(UiToEngine::Tool {
            name: name.to_string(),
            arguments: arguments.clone(),
            context: context.cloned(),
        });
        let mut result = match response {
            EngineToUi::ToolResult { result, model: _ } => result,
            EngineToUi::Error { message, kind, .. } => ToolResult::error(
                "Internal",
                &format!("the engine refused the tool message: {message}"),
                json!({ "tool": name, "kind": kind }),
            ),
            other => ToolResult::error(
                "Internal",
                "the engine answered a tool with something other than a tool result",
                json!({ "tool": name, "response": format!("{:?}", std::mem::discriminant(&other)) }),
            ),
        };
        // A download is the host's to deliver (§3.3): into the exports
        // directory, the path in the answer. Taken out of the result so it
        // never reaches the wire.
        if let Some(file) = result.download.take() {
            match self.deliver_download(&file) {
                Ok(path) => {
                    result.structured_content["path"] = json!(path.to_string_lossy());
                    result.content.push(json!({
                        "type": "text",
                        "text": format!("Written to {}", path.display()),
                    }));
                }
                Err(err) => {
                    result = ToolResult::error(
                        "SaveFailed",
                        &format!("the export was produced but could not be written: {err}"),
                        json!({ "file_name": file.file_name }),
                    );
                }
            }
        }
        // A tab tool answers the document as the session knows it; what only
        // this host knows — the storage record and provider, read-only,
        // unsaved — is overlaid here, so the agent sees `document_info`'s
        // shape (the page overlays the same from its store).
        if !result.is_error && wasm_bridge::tools::TAB_TOOLS.contains(&name) {
            if let (Value::Object(map), Value::Object(info)) =
                (&mut result.structured_content, documents::info(self))
            {
                for (k, v) in info {
                    map.entry(k).or_insert(v);
                }
            }
        }
        // Durability (§4.8): the document on disk is never more than one
        // committed tool behind. Not debounced in S4 — the session composes
        // the file in far less time than any rebuild — and a failure is
        // reported on the answer, never swallowed: the tool DID run, and the
        // agent must know the disk did not follow.
        if mutates(name) {
            if let Err(err) = self.autosave() {
                result.content.push(json!({
                    "type": "text",
                    "text": format!("Note: autosave failed after this call: {err}"),
                }));
                if let Value::Object(map) = &mut result.structured_content {
                    map.insert("autosave_error".into(), json!(err));
                }
            }
        }
        result
    }

    /// Write an export into `DIR/exports/`, replacing an earlier file of the
    /// same name (the same document exported twice is the same file).
    fn deliver_download(&self, file: &ExportFile) -> Result<PathBuf, String> {
        let name = Path::new(&file.file_name)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty())
            .ok_or_else(|| format!("export has no usable file name ({:?})", file.file_name))?;
        let path = self.documents.join("exports").join(name);
        let bytes: Vec<u8> = match (&file.text, &file.blob) {
            (Some(text), _) => text.as_bytes().to_vec(),
            (None, Some(blob)) => {
                use base64::Engine as _;
                base64::engine::general_purpose::STANDARD
                    .decode(blob)
                    .map_err(|e| format!("export blob is not base64: {e}"))?
            }
            (None, None) => return Err("export carries neither text nor bytes".into()),
        };
        documents::write_atomic(&path, &bytes).map_err(|e| e.to_string())?;
        Ok(path)
    }

    /// Compose the open document and write it to its record
    /// (`DIR/<document id>.waffle`), atomically.
    pub fn autosave(&mut self) -> Result<PathBuf, String> {
        documents::save(self)
    }
}
