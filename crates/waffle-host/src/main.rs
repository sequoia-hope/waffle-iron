//! `waffle-host --documents DIR`: the stdio loop (`specs/waffle_server_mode.md`
//! §3.4). stdout carries frames only; logs go to stderr.
//!
//! Frames in: `tool{id, name, arguments, context{agent_name, progress}}`,
//! `cancel{id}`, `bye{reason}`. Frames out: `ready`, `progress{id, message,
//! elapsed_ms, progress, total}` (only while a call that asked for progress
//! is running), `result{id, content, structuredContent, isError}`, `bye`.
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

thread_local! {
    /// The call in flight on the engine thread, when it asked for progress.
    static ACTIVE_CALL: RefCell<Option<(String, Instant)>> = const { RefCell::new(None) };
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
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if let Err(err) = write_frame(&mut out, header, b"") {
        // The relay is gone: nothing to answer to. Exit quietly; the relay
        // sees EOF on its side and treats the host as crashed.
        eprintln!("waffle-host: stdout closed ({err}); exiting");
        std::process::exit(0);
    }
}

fn install_progress_sink() {
    feature_engine::progress::install(Box::new(|event| {
        ACTIVE_CALL.with(|active| {
            if let Some((id, started)) = active.borrow().as_ref() {
                emit(&json!({
                    "type": "progress",
                    "id": id,
                    "message": format!("{}: {}", event.feature_name, event.label),
                    "elapsed_ms": started.elapsed().as_millis() as u64,
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
    if wants_progress {
        if let Some(call_id) = id.as_str() {
            ACTIVE_CALL.with(|a| *a.borrow_mut() = Some((call_id.to_string(), Instant::now())));
        }
    }
    let result = host.call(name, &arguments, context.as_ref());
    ACTIVE_CALL.with(|a| *a.borrow_mut() = None);
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
