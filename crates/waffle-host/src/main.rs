//! `waffle-host --documents DIR`: the stdio loop (`specs/waffle_server_mode.md`
//! §3.4). stdout carries frames only; logs go to stderr.
//!
//! Frames in: `tool{id, name, arguments, context{agent_name, progress}}`,
//! `cancel{id}`, `bye{reason}`, and for viewer sync (§4) `snapshot{id?}` and
//! `blob{id, mesh_id, encoding?}`. Frames out: `ready`, `progress{id,
//! message, elapsed_ms, progress, total}` (only while a call that asked for
//! progress is running), `rebuild{state: started | progress | done, tool,
//! feature_id?, feature_name?, message?, elapsed_ms}` (unsolicited, around
//! every tool that can change the document — the viewer's spinner, §4.3),
//! `result{id, content, structuredContent, isError}`, `bye`, `snapshot{…}`
//! (answering a request by `id`, and unsolicited after every tool that
//! changed the document), `blob{id, mesh_id, encoding, byte_length}` +
//! payload (or `missing: true`).
//!
//! One engine thread runs tools in arrival order; a reader thread feeds it,
//! so a `bye` or a `cancel` is seen as soon as it arrives even during a long
//! rebuild. The engine is synchronous, so a `cancel` cannot interrupt the
//! running tool: it is acknowledged by the result the tool produces anyway,
//! and logged. (The page behaves the same way: its worker finishes the
//! message it is on.)

use std::cell::RefCell;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use serde_json::{json, Value};
use waffle_host::{read_frame, write_frame, Frame, Host, PROTOCOL};

/// The call in flight on the engine thread: its id when it asked for
/// `progress` frames, and whether it can change the document (then every
/// progress event is also a `rebuild` frame for the viewers).
struct ActiveCall {
    progress_id: Option<String>,
    tool: String,
    rebuild: bool,
    started: Instant,
}

thread_local! {
    static ACTIVE_CALL: RefCell<Option<ActiveCall>> = const { RefCell::new(None) };
}

fn usage() -> ! {
    eprintln!("usage: waffle-host --documents DIR");
    std::process::exit(2)
}

fn parse_args() -> PathBuf {
    let mut args = std::env::args().skip(1);
    let mut documents: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--documents" => documents = args.next().map(PathBuf::from),
            "--version" => {
                println!("waffle-host {} ({PROTOCOL})", env!("CARGO_PKG_VERSION"));
                std::process::exit(0)
            }
            other if other.starts_with("--documents=") => {
                documents = Some(PathBuf::from(&other["--documents=".len()..]))
            }
            _ => usage(),
        }
    }
    documents.unwrap_or_else(|| usage())
}

/// Write one frame to stdout under its lock (a frame is one `write_all`).
fn emit(header: &Value) {
    emit_with(header, b"");
}

/// Write one frame with a binary payload (a viewer blob).
fn emit_with(header: &Value, payload: &[u8]) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if let Err(err) = write_frame(&mut out, header, payload) {
        // The relay is gone: nothing to answer to. Exit quietly; the relay
        // sees EOF on its side and treats the host as crashed.
        eprintln!("waffle-host: stdout closed ({err}); exiting");
        std::process::exit(0);
    }
}

fn install_progress_sink() {
    feature_engine::progress::install(Box::new(|event| {
        ACTIVE_CALL.with(|active| {
            let Some(call) = active.borrow().as_ref().map(|c| {
                (
                    c.progress_id.clone(),
                    c.tool.clone(),
                    c.rebuild,
                    c.started.elapsed().as_millis() as u64,
                )
            }) else {
                return;
            };
            let (progress_id, tool, rebuild, elapsed_ms) = call;
            if let Some(id) = progress_id {
                emit(&json!({
                    "type": "progress",
                    "id": id,
                    "message": format!("{}: {}", event.feature_name, event.label),
                    "elapsed_ms": elapsed_ms,
                    "progress": event.done,
                    "total": event.done + event.remaining,
                }));
            }
            if rebuild {
                emit(&json!({
                    "type": "rebuild",
                    "state": "progress",
                    "tool": tool,
                    "feature_id": event.feature_id,
                    "feature_name": event.feature_name,
                    "message": event.label,
                    "elapsed_ms": elapsed_ms,
                    "progress": event.done,
                    "total": event.done + event.remaining,
                }));
            }
        });
    }));
}

fn handle_tool(host: &mut Host, header: &Value) {
    let id = header.get("id").cloned().unwrap_or(Value::Null);
    let name = header.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = header
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let context = header.get("context").cloned();
    let wants_progress = context
        .as_ref()
        .and_then(|c| c.get("progress"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // Viewer sync (§4.3 `rebuild`): a tool that can change the document is
    // announced before it runs and after, so a viewer shows a spinner with
    // the feature the engine is on rather than a frozen model.
    let rebuild = Host::model_changed(name);
    let started = Instant::now();
    if rebuild {
        emit(&json!({ "type": "rebuild", "state": "started", "tool": name, "elapsed_ms": 0 }));
    }
    ACTIVE_CALL.with(|a| {
        *a.borrow_mut() = Some(ActiveCall {
            progress_id: if wants_progress {
                id.as_str().map(str::to_string)
            } else {
                None
            },
            tool: name.to_string(),
            rebuild,
            started,
        })
    });
    let result = host.call(name, &arguments, context.as_ref());
    ACTIVE_CALL.with(|a| *a.borrow_mut() = None);
    if rebuild {
        emit(&json!({
            "type": "rebuild",
            "state": "done",
            "tool": name,
            "ok": !result.is_error,
            "elapsed_ms": started.elapsed().as_millis() as u64,
        }));
    }
    let mut frame = serde_json::to_value(&result).unwrap_or_else(|e| {
        json!({
            "content": [{ "type": "text", "text": format!("Internal: {e}") }],
            "structuredContent": { "error": { "code": "Internal", "message": e.to_string(), "details": {} } },
            "isError": true,
        })
    });
    frame["type"] = json!("result");
    frame["id"] = id;
    // Never on the wire (the host delivered it, or it was never set).
    if let Value::Object(map) = &mut frame {
        map.remove("download");
    }
    emit(&frame);
    // Viewer sync (§4.6 step 2): every committed change is followed by the
    // document as a viewer draws it, so the relay can push an `update`
    // without asking. A refusal moved nothing a viewer shows — and neither
    // does a change nobody is watching (`Host::viewers_watching`).
    if Host::model_changed(name) && !result.is_error && host.viewers_watching() {
        emit(&host.snapshot());
    }
}

/// `snapshot{id?}`: the document as a viewer draws it, answered with the
/// request's `id` when it had one.
fn handle_snapshot(host: &mut Host, header: &Value) {
    host.note_viewer_request();
    let mut snapshot = host.snapshot();
    if let Some(id) = header.get("id") {
        snapshot["id"] = id.clone();
    }
    emit(&snapshot);
}

/// `blob{id, mesh_id, encoding?}`: the named blob as the frame's payload in
/// the encoding asked for (`raw/1` by default, §4.5), or `missing: true`
/// when no recent snapshot named it or the encoding is unknown (the viewer
/// then asks for a fresh snapshot, or falls back to `raw/1`).
fn handle_blob(host: &mut Host, header: &Value) {
    host.note_viewer_request();
    let id = header.get("id").cloned().unwrap_or(Value::Null);
    let mesh_id = header.get("mesh_id").and_then(Value::as_str).unwrap_or("");
    let encoding = header
        .get("encoding")
        .and_then(Value::as_str)
        .unwrap_or(waffle_host::viewer::ENCODING);
    match host.blob_encoded(mesh_id, encoding) {
        Some(bytes) => emit_with(
            &json!({
                "type": "blob",
                "id": id,
                "mesh_id": mesh_id,
                "encoding": encoding,
                "byte_length": bytes.len(),
            }),
            &bytes,
        ),
        None => emit(&json!({
            "type": "blob",
            "id": id,
            "mesh_id": mesh_id,
            "encoding": encoding,
            "missing": true,
        })),
    }
}

fn main() {
    let documents = parse_args();
    let mut host = match Host::new(&documents) {
        Ok(host) => host,
        Err(err) => {
            eprintln!(
                "waffle-host: cannot use documents directory {}: {err}",
                documents.display()
            );
            std::process::exit(2)
        }
    };
    install_progress_sink();
    emit(&host.ready_frame());
    eprintln!(
        "waffle-host: ready (epoch {}, documents {})",
        host.epoch(),
        documents.display()
    );

    let (tx, rx) = mpsc::channel::<Option<Frame>>();
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        loop {
            match read_frame(&mut input) {
                Ok(Some(frame)) => {
                    if tx.send(Some(frame)).is_err() {
                        return;
                    }
                }
                Ok(None) => {
                    let _ = tx.send(None);
                    return;
                }
                Err(err) => {
                    eprintln!("waffle-host: bad frame on stdin: {err}");
                    let _ = tx.send(None);
                    return;
                }
            }
        }
    });

    for frame in rx {
        let Some(frame) = frame else {
            eprintln!("waffle-host: stdin closed; exiting");
            break;
        };
        match frame.kind() {
            "tool" => handle_tool(&mut host, &frame.header),
            "snapshot" => handle_snapshot(&mut host, &frame.header),
            "blob" => handle_blob(&mut host, &frame.header),
            "cancel" => eprintln!(
                "waffle-host: cancel for {} noted; the engine finishes the tool it is on",
                frame.header.get("id").cloned().unwrap_or(Value::Null)
            ),
            "bye" => {
                eprintln!(
                    "waffle-host: bye ({})",
                    frame
                        .header
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                );
                emit(&json!({ "type": "bye", "reason": "requested" }));
                break;
            }
            other => eprintln!("waffle-host: ignoring unknown frame type {other:?}"),
        }
    }
    let _ = io::stdout().flush();
}
