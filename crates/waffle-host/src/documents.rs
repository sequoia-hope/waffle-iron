//! The host's storage provider: a directory of `.waffle` files
//! (`specs/waffle_server_mode.md` §3.3 "Storage tools", provider id `file`).
//!
//! A record is `DIR/<document id>.waffle`, keyed by the document's own
//! identity (v4 P2-5), exactly as the browser's IndexedDB records are. The
//! five storage tools answer with the page's result shapes
//! (`app/src/lib/agent/tools/documents.js`), so an agent cannot tell the
//! providers apart except by `storage_provider.id`. Git providers are
//! `HostCapability` in this version (spec §7 open question 2).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use wasm_bridge::messages::{EngineToUi, UiToEngine};
use wasm_bridge::ToolResult;

use crate::host::Host;

pub const PROVIDER_ID: &str = "file";
const EXTENSION: &str = "waffle";

/// Dispatch one storage tool.
pub fn run(host: &mut Host, name: &str, args: &Value) -> ToolResult {
    let outcome = match name {
        "document_info" => Ok(info(host)),
        "storage_list" => storage_list(host, args),
        "document_open" => document_open(host, args),
        "document_new" => document_new(host, args),
        "document_import" => document_import(host, args),
        "document_save" => document_save(host),
        other => Err(refuse(
            "ToolUnavailable",
            &format!("the file provider has no tool named \"{other}\""),
            json!({ "tool": other }),
        )),
    };
    match outcome {
        Ok(structured) => ToolResult::ok(structured),
        Err(refusal) => *refusal,
    }
}

/// A refusal, boxed: a `ToolResult` is a large `Err` to pass around by value
/// (clippy `result_large_err`), and every refusal here is the rare path.
fn refuse(code: &str, message: &str, details: Value) -> Box<ToolResult> {
    Box::new(ToolResult::error(code, message, details))
}

fn provider_label(host: &Host) -> String {
    host.documents_dir().to_string_lossy().into_owned()
}

/// `document_info`: the page's shape, from the session.
pub fn info(host: &Host) -> Value {
    let state = host.state();
    let doc = wasm_bridge::dispatch::document_info(state);
    let sources: Vec<Value> = wasm_bridge::dispatch::source_statuses(state)
        .into_iter()
        .map(|s| json!({ "id": s.id, "name": s.name, "kind": s.kind, "available": s.available }))
        .collect();
    json!({
        "document_id": doc.id,
        "storage_id": doc.id,
        "name": doc.name,
        "storage_provider": { "id": PROVIDER_ID, "label": provider_label(host) },
        "tabs": doc.tabs.iter().map(|t| json!({ "id": t.id, "name": t.name, "kind": t.kind })).collect::<Vec<_>>(),
        "active_tab": doc.active_tab,
        "read_only": false,
        "sources": sources,
        // Autosave is synchronous with every mutating tool (host.rs), so
        // nothing is ever waiting.
        "unsaved": false,
    })
}

fn require_file_provider(args: &Value) -> Result<(), Box<ToolResult>> {
    match args.get("provider").and_then(Value::as_str) {
        None => Ok(()),
        Some(id) if id == PROVIDER_ID => Ok(()),
        Some(other) => Err(refuse(
            "HostCapability",
            &format!(
                "storage provider \"{other}\" is not available in the native host; only \
                 \"{PROVIDER_ID}\" (the --documents directory) is."
            ),
            json!({ "provider": other, "kernel": "host" }),
        )),
    }
}

/// One stored record's listing entry, read from the file's document header.
fn list_entry(path: &Path) -> Option<(Value, u128)> {
    let stem = path.file_stem()?.to_str()?.to_string();
    let text = fs::read_to_string(path).ok()?;
    let doc: Value = serde_json::from_str(&text).ok()?;
    let meta = doc.get("document")?;
    let name = meta
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Untitled");
    let stamp = |key: &str| -> Option<i64> {
        meta.get(key)
            .and_then(Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.timestamp_millis())
    };
    // The file's own mtime, at full resolution: the listing order's tie
    // breaker when two records were stamped in the same millisecond.
    let file_mtime = fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
    let modified = stamp("modified").or(file_mtime.map(|d| d.as_millis() as i64));
    let created = stamp("created").or(modified);
    let tab_count = doc
        .get("tabs")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let entry = json!({
        "id": stem,
        "name": name,
        "created": created,
        "modified": modified,
        "tab_count": tab_count,
        "linked": false,
    });
    Some((entry, file_mtime.map(|d| d.as_nanos()).unwrap_or(0)))
}

fn storage_list(host: &Host, args: &Value) -> Result<Value, Box<ToolResult>> {
    require_file_provider(args)?;
    let dir = host.documents_dir();
    let entries = fs::read_dir(dir).map_err(|e| {
        refuse(
            "StorageFailed",
            &format!("Listing {} failed: {e}", dir.display()),
            json!({ "provider": PROVIDER_ID, "reason": e.to_string() }),
        )
    })?;
    let mut documents: Vec<(Value, u128)> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some(EXTENSION))
        .filter_map(|p| list_entry(&p))
        .collect();
    // Newest first, as the page lists them: by the document's own stamp,
    // then the file's mtime, then the id, so the order is total and stable
    // across calls.
    documents.sort_by(|(a, a_mtime), (b, b_mtime)| {
        let m = |v: &Value| v["modified"].as_i64().unwrap_or(0);
        m(b).cmp(&m(a))
            .then_with(|| b_mtime.cmp(a_mtime))
            .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
    });
    let documents: Vec<Value> = documents.into_iter().map(|(entry, _)| entry).collect();
    Ok(json!({ "provider": PROVIDER_ID, "documents": documents }))
}

/// A record id is a file stem: never a path.
fn record_path(host: &Host, id: &str) -> Result<PathBuf, Box<ToolResult>> {
    let safe = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && id != "."
        && id != "..";
    if !safe {
        return Err(refuse(
            "DocumentNotFound",
            &format!("No document \"{id}\" in {}.", provider_label(host)),
            json!({ "provider": PROVIDER_ID, "id": id }),
        ));
    }
    Ok(host.documents_dir().join(format!("{id}.{EXTENSION}")))
}

fn document_open(host: &mut Host, args: &Value) -> Result<Value, Box<ToolResult>> {
    require_file_provider(args)?;
    let id = args.get("id").and_then(Value::as_str).unwrap_or("");
    let path = record_path(host, id)?;
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(refuse(
                "DocumentNotFound",
                &format!("No document \"{id}\" in {}.", provider_label(host)),
                json!({ "provider": PROVIDER_ID, "id": id }),
            ))
        }
        Err(e) => {
            return Err(refuse(
                "StorageFailed",
                &format!("Reading {} failed: {e}", path.display()),
                json!({ "provider": PROVIDER_ID, "reason": e.to_string() }),
            ))
        }
    };
    // `discard_unsaved` needs no confirmation: autosave is synchronous, so
    // there is never anything unsaved to discard.
    match host.engine(UiToEngine::LoadProject { data: text }) {
        EngineToUi::ModelUpdated { .. } => Ok(info(host)),
        EngineToUi::Error { message, .. } => Err(refuse(
            "StorageFailed",
            &format!("The document did not load: {message}"),
            json!({ "provider": PROVIDER_ID, "reason": message }),
        )),
        _ => Err(refuse(
            "Internal",
            "the engine answered LoadProject with an unexpected message",
            json!({}),
        )),
    }
}

fn document_new(host: &mut Host, args: &Value) -> Result<Value, Box<ToolResult>> {
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .filter(|n| !n.is_empty())
        .unwrap_or("Untitled")
        .to_string();
    if let EngineToUi::Error { message, .. } = host.engine(UiToEngine::NewDocument) {
        return Err(refuse(
            "Internal",
            &format!("the engine did not reset for a new document: {message}"),
            json!({}),
        ));
    }
    if let EngineToUi::Error { message, .. } = host.engine(UiToEngine::SetDocumentMeta {
        name: Some(name),
        display_unit: None,
        id: None,
        created: None,
    }) {
        return Err(refuse(
            "Internal",
            &format!("the engine did not take the new document's name: {message}"),
            json!({}),
        ));
    }
    save(host).map_err(|reason| {
        refuse(
            "SaveFailed",
            &format!(
                "Creating the document in {} failed: {reason}",
                provider_label(host)
            ),
            json!({ "provider": PROVIDER_ID, "reason": reason }),
        )
    })?;
    Ok(info(host))
}

/// `document_import`: a `.waffle` file's text becomes a stored, open
/// document (the page's file-picker rule: the file's own identity is the
/// record key, the file name wins over the stored name).
fn document_import(host: &mut Host, args: &Value) -> Result<Value, Box<ToolResult>> {
    let file_name = args
        .get("file_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let text = args.get("text").and_then(Value::as_str).unwrap_or("");
    let parsed: Value = serde_json::from_str(text).map_err(|e| {
        refuse(
            "InvalidDocument",
            &format!("{file_name} is not a .waffle document: {e}"),
            json!({ "file_name": file_name, "reason": e.to_string() }),
        )
    })?;
    let required = ["version", "min_reader_version"]
        .iter()
        .filter_map(|k| parsed.get(k).and_then(Value::as_u64))
        .max()
        .unwrap_or(0);
    if required > u64::from(file_format::FORMAT_VERSION) {
        return Err(refuse(
            "FormatTooNew",
            &format!("{file_name} was saved by a newer version of Waffle Iron."),
            json!({
                "file_name": file_name,
                "file_version": required,
                "supported_version": file_format::FORMAT_VERSION,
            }),
        ));
    }
    match host.engine(UiToEngine::LoadProject {
        data: text.to_string(),
    }) {
        EngineToUi::ModelUpdated { .. } => {}
        EngineToUi::Error { message, .. } => {
            return Err(refuse(
                "InvalidDocument",
                &format!("{file_name} did not load: {message}"),
                json!({ "file_name": file_name, "reason": message }),
            ))
        }
        _ => {
            return Err(refuse(
                "Internal",
                "the engine answered LoadProject with an unexpected message",
                json!({}),
            ))
        }
    }
    let name = match args.get("name").and_then(Value::as_str) {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => strip_document_extension(&file_name).to_string(),
    };
    if !name.is_empty() && name != wasm_bridge::dispatch::document_info(host.state()).name {
        if let EngineToUi::Error { message, .. } = host.engine(UiToEngine::SetDocumentMeta {
            name: Some(name),
            display_unit: None,
            id: None,
            created: None,
        }) {
            return Err(refuse(
                "Internal",
                &format!("the engine did not take the imported document's name: {message}"),
                json!({}),
            ));
        }
    }
    save(host).map_err(|reason| {
        refuse(
            "SaveFailed",
            &format!(
                "Storing the imported document in {} failed: {reason}",
                provider_label(host)
            ),
            json!({ "provider": PROVIDER_ID, "reason": reason }),
        )
    })?;
    Ok(info(host))
}

/// `Pinwheel.waffle` / `Pinwheel.json` → `Pinwheel` (case-insensitive, as the page's picker).
fn strip_document_extension(file_name: &str) -> &str {
    for ext in [".waffle", ".json"] {
        if file_name.len() > ext.len()
            && file_name[file_name.len() - ext.len()..].eq_ignore_ascii_case(ext)
        {
            return &file_name[..file_name.len() - ext.len()];
        }
    }
    file_name
}

fn document_save(host: &mut Host) -> Result<Value, Box<ToolResult>> {
    let path = save(host).map_err(|reason| {
        refuse(
            "SaveFailed",
            &format!("Saving failed: {reason}"),
            json!({ "provider": PROVIDER_ID, "reason": reason }),
        )
    })?;
    let id = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(json!({
        "provider": PROVIDER_ID,
        "id": id,
        "saved_at": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    }))
}

/// Compose the open document (the session is the single writer, v4 §4
/// inv. 7) and write it to its record. Returns the record's path.
pub fn save(host: &mut Host) -> Result<PathBuf, String> {
    let json_data = match host.engine(UiToEngine::SaveDocument) {
        EngineToUi::SaveReady { json_data } => json_data,
        EngineToUi::Error { message, .. } => {
            return Err(format!(
                "the engine did not compose the document: {message}"
            ))
        }
        _ => return Err("the engine answered SaveDocument with an unexpected message".into()),
    };
    let id = wasm_bridge::dispatch::document_info(host.state())
        .id
        .to_string();
    let path = host.documents_dir().join(format!("{id}.{EXTENSION}"));
    write_atomic(&path, json_data.as_bytes()).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Write `bytes` to `path` through a sibling temp file and a rename, so a
/// crash mid-write never leaves a half document where a whole one was.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default(),
        std::process::id()
    ));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}
