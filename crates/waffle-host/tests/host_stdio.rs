//! The host binary over its stdio frames (`specs/waffle_server_mode.md` §3.4,
//! oracle H2-lite): one relay-shaped client drives the real `waffle-host`
//! through a document's life — new, sketch, extrude, measure, export,
//! list, reopen — and checks the file provider on disk behind it. The
//! kernel is real (kernel-v2), so the numbers are the engine's, not a mock's.

use std::io::{BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};
use waffle_host::{read_frame, write_frame, Frame};

struct Client {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u32,
    /// Frames that were not the awaited result (progress, bye), in order.
    pub aside: Vec<Frame>,
}

impl Client {
    fn spawn(documents: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_waffle-host"))
            .arg("--documents")
            .arg(documents)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn waffle-host");
        let stdin = BufWriter::new(child.stdin.take().unwrap());
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
            next_id: 0,
            aside: Vec::new(),
        }
    }

    fn recv(&mut self) -> Frame {
        read_frame(&mut self.stdout)
            .expect("read frame")
            .expect("host closed its stdout")
    }

    fn send(&mut self, header: &Value) {
        write_frame(&mut self.stdin, header, b"").expect("write frame");
        self.stdin.flush().unwrap();
    }

    /// Call a tool and return its `result` frame.
    fn call(&mut self, name: &str, arguments: Value) -> Value {
        self.call_with(name, arguments, json!({ "agent_name": "host-test" }))
    }

    fn call_with(&mut self, name: &str, arguments: Value, context: Value) -> Value {
        self.next_id += 1;
        let id = format!("c{}", self.next_id);
        self.send(&json!({
            "type": "tool",
            "id": id,
            "name": name,
            "arguments": arguments,
            "context": context,
        }));
        loop {
            let frame = self.recv();
            if frame.kind() == "result" {
                assert_eq!(frame.header["id"], id, "result for another call: {frame:?}");
                return frame.header;
            }
            self.aside.push(frame);
        }
    }

    fn ok(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.call(name, arguments);
        assert_eq!(
            result["isError"], false,
            "{name} failed: {}",
            result["structuredContent"]
        );
        result["structuredContent"].clone()
    }

    fn refused(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.call(name, arguments);
        assert_eq!(
            result["isError"], true,
            "{name} unexpectedly succeeded: {result}"
        );
        result["structuredContent"]["error"].clone()
    }

    fn bye(mut self) -> i32 {
        self.send(&json!({ "type": "bye", "reason": "test done" }));
        let frame = self.recv();
        assert_eq!(frame.kind(), "bye");
        let status = self.child.wait().expect("wait");
        status.code().unwrap_or(-1)
    }
}

fn point(id: u32, x: f64, y: f64) -> Value {
    json!({ "type": "Point", "id": id, "x": x, "y": y })
}

fn line(id: u32, start: u32, end: u32) -> Value {
    json!({ "type": "Line", "id": id, "start_id": start, "end_id": end })
}

/// A 20 × 10 mm rectangle: points 1–4, lines 5–8.
fn rectangle() -> Vec<Value> {
    vec![
        point(1, 0.0, 0.0),
        point(2, 0.02, 0.0),
        point(3, 0.02, 0.01),
        point(4, 0.0, 0.01),
        line(5, 1, 2),
        line(6, 2, 3),
        line(7, 3, 4),
        line(8, 4, 1),
    ]
}

#[test]
fn a_document_lives_through_the_frames_and_lands_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::spawn(dir.path());

    // -- ready ------------------------------------------------------------
    let ready = client.recv();
    assert_eq!(ready.kind(), "ready");
    assert_eq!(ready.header["protocol"], waffle_host::PROTOCOL);
    assert!(ready.header["epoch"]
        .as_str()
        .is_some_and(|e| !e.is_empty()));
    let tools: Vec<&str> = ready.header["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap())
        .collect();
    for name in [
        "model_summary",
        "feature_add",
        "sketch_create",
        "document_new",
        "storage_list",
    ] {
        assert!(tools.contains(&name), "ready.tools lacks {name}");
    }
    for name in ["selection_get", "tab_add", "viewport_capture"] {
        assert!(!tools.contains(&name), "ready.tools must not list {name}");
    }
    assert!(
        dir.path().join("exports").is_dir(),
        "exports/ is created at start"
    );

    // -- the bootstrap document, before anything is stored ------------------
    let info = client.ok("document_info", json!({}));
    assert_eq!(info["storage_provider"]["id"], "file");
    assert_eq!(info["read_only"], false);
    assert_eq!(info["unsaved"], false);
    assert_eq!(info["tabs"].as_array().unwrap().len(), 1);
    let listed = client.ok("storage_list", json!({}));
    assert_eq!(
        listed["documents"],
        json!([]),
        "nothing is stored until a tool changes something"
    );

    // -- document_new: a record appears, keyed by the document's identity ----
    let info = client.ok("document_new", json!({ "name": "Pinwheel" }));
    assert_eq!(info["name"], "Pinwheel");
    let doc_id = info["document_id"].as_str().unwrap().to_string();
    assert_eq!(info["storage_id"], doc_id);
    let record = dir.path().join(format!("{doc_id}.waffle"));
    assert!(record.is_file(), "document_new writes {}", record.display());
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&record).unwrap()).unwrap();
    assert_eq!(stored["document"]["name"], "Pinwheel");
    assert_eq!(stored["document"]["id"], doc_id);

    // -- sketch + extrude through the shared engine tools --------------------
    let sketch = client.ok(
        "sketch_create",
        json!({
            "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] },
            "entities": rectangle(),
        }),
    );
    let sketch_id = sketch["feature_id"].as_str().unwrap().to_string();
    let extrude = client.ok(
        "feature_add",
        json!({
            "operation": {
                "type": "Extrude",
                "params": {
                    "sketch_id": sketch_id,
                    "profile_index": 0,
                    "profile_entity_ids": [5, 6, 7, 8],
                    "depth": 0.005,
                    "symmetric": false,
                    "cut": false,
                }
            }
        }),
    );
    assert_eq!(
        extrude["bodies_added"].as_array().unwrap().len(),
        1,
        "{extrude}"
    );

    let summary = client.ok("model_summary", json!({}));
    assert_eq!(summary["features"].as_array().unwrap().len(), 2);
    assert_eq!(summary["bodies"].as_array().unwrap().len(), 1);
    let body_id = summary["bodies"][0]["body_id"]
        .as_str()
        .unwrap()
        .to_string();

    let measured = client.ok("body_measure", json!({ "body_id": body_id }));
    let volume = measured["volume_m3"].as_f64().unwrap();
    assert!(
        (volume - 0.02 * 0.01 * 0.005).abs() < 1e-12,
        "volume {volume}"
    );
    assert_eq!(measured["methods"]["volume"], "exact");

    // The autosave after each mutating tool kept the record current.
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&record).unwrap()).unwrap();
    let features = stored["tabs"][0]["kind"]["features"]["features"]
        .as_array()
        .unwrap();
    assert_eq!(
        features.len(),
        2,
        "record after two authoring tools: {stored}"
    );

    // -- a download is written into exports/ and the answer names it --------
    let export = client.ok("export_stl", json!({ "deliver": "download" }));
    assert_eq!(export["deliver"], "download");
    let path = std::path::PathBuf::from(export["path"].as_str().expect("path in the answer"));
    assert!(
        path.starts_with(dir.path().join("exports")),
        "{}",
        path.display()
    );
    let bytes = std::fs::metadata(&path).unwrap().len();
    assert_eq!(
        bytes,
        export["bytes"].as_u64().unwrap(),
        "the file on disk is the answer's size"
    );
    // The same export delivered inline is unchanged by the host.
    let inline = client.call("export_stl", json!({ "deliver": "agent" }));
    assert_eq!(inline["isError"], false);
    assert!(inline["structuredContent"].get("path").is_none());
    assert!(
        inline.get("download").is_none(),
        "download never reaches the wire"
    );

    // -- document_save answers the page's shape ----------------------------
    let saved = client.ok("document_save", json!({}));
    assert_eq!(saved["provider"], "file");
    assert_eq!(saved["id"], doc_id);
    assert!(saved["saved_at"].as_str().unwrap().ends_with('Z'));

    // -- storage_list, then leave and come back -----------------------------
    let listed = client.ok("storage_list", json!({}));
    let docs = listed["documents"].as_array().unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0]["id"], doc_id);
    assert_eq!(docs[0]["name"], "Pinwheel");
    assert_eq!(docs[0]["tab_count"], 1);
    assert_eq!(docs[0]["linked"], false);

    let other = client.ok("document_new", json!({ "name": "Other" }));
    assert_ne!(other["document_id"], doc_id);
    assert_eq!(client.ok("model_summary", json!({}))["features"], json!([]));
    let listed = client.ok("storage_list", json!({}));
    assert_eq!(listed["documents"].as_array().unwrap().len(), 2);
    assert_eq!(listed["documents"][0]["name"], "Other", "newest first");

    let reopened = client.ok("document_open", json!({ "id": doc_id }));
    assert_eq!(reopened["name"], "Pinwheel");
    assert_eq!(reopened["document_id"], doc_id);
    let summary = client.ok("model_summary", json!({}));
    assert_eq!(summary["features"].as_array().unwrap().len(), 2);
    assert_eq!(
        summary["bodies"].as_array().unwrap().len(),
        1,
        "reopened and rebuilt"
    );

    // -- refusals are typed, never silent ------------------------------------
    let err = client.refused("document_open", json!({ "id": "no-such" }));
    assert_eq!(err["code"], "DocumentNotFound");
    let err = client.refused("document_open", json!({ "id": "../etc/passwd" }));
    assert_eq!(
        err["code"], "DocumentNotFound",
        "a path is never a record id"
    );
    let err = client.refused("storage_list", json!({ "provider": "github" }));
    assert_eq!(err["code"], "HostCapability");
    let err = client.refused("selection_get", json!({}));
    assert_eq!(err["code"], "ViewerUnavailable");
    let err = client.refused("tab_add", json!({}));
    assert_eq!(err["code"], "HostCapability");
    let err = client.refused("no_such_tool", json!({}));
    assert_eq!(err["code"], "ToolUnavailable");
    let err = client.refused("feature_get", json!({ "feature_id": "not-a-uuid" }));
    assert_eq!(
        err["code"], "FeatureNotFound",
        "engine refusals pass through unchanged"
    );

    assert_eq!(client.bye(), 0);
}

#[test]
fn progress_frames_ride_only_for_a_call_that_asked() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::spawn(dir.path());
    assert_eq!(client.recv().kind(), "ready");
    client.ok("document_new", json!({ "name": "Progress" }));

    // Two separate boxes, then one UnionAll: the balanced union reports a
    // progress step per pairwise union (specs/b4_balanced_union.md §2.3).
    for (i, x0) in [0.0, 0.05].into_iter().enumerate() {
        let entities = vec![
            point(1, x0, 0.0),
            point(2, x0 + 0.02, 0.0),
            point(3, x0 + 0.02, 0.01),
            point(4, x0, 0.01),
            line(5, 1, 2),
            line(6, 2, 3),
            line(7, 3, 4),
            line(8, 4, 1),
        ];
        let sketch = client.ok(
            "sketch_create",
            json!({ "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] }, "entities": entities }),
        );
        let extrude = client.ok(
            "feature_add",
            json!({ "operation": { "type": "Extrude", "params": {
                "sketch_id": sketch["feature_id"], "profile_index": 0,
                "profile_entity_ids": [5, 6, 7, 8], "depth": 0.005, "symmetric": false,
                "cut": false, "combine": { "type": "NewBody" },
            }}}),
        );
        assert_eq!(
            extrude["bodies_added"].as_array().unwrap().len(),
            1,
            "box {i}: {extrude}"
        );
    }
    client.aside.clear();
    let union = client.call_with(
        "feature_add",
        json!({ "operation": { "type": "UnionAll", "params": {} } }),
        json!({ "agent_name": "host-test", "progress": true }),
    );
    assert_eq!(union["isError"], false, "{union}");
    let progress: Vec<&Frame> = client
        .aside
        .iter()
        .filter(|f| f.kind() == "progress")
        .collect();
    assert!(
        !progress.is_empty(),
        "a UnionAll under progress:true reports its steps"
    );
    for frame in &progress {
        assert_eq!(frame.header["id"], union["id"]);
        assert!(frame.header["message"]
            .as_str()
            .is_some_and(|m| !m.is_empty()));
        assert!(frame.header["elapsed_ms"].is_u64());
        assert!(frame.header["progress"].is_u64() && frame.header["total"].is_u64());
    }

    // The same tool without the flag: silent.
    client.aside.clear();
    let undo = client.call("undo", json!({}));
    assert_eq!(undo["isError"], false, "{undo}");
    client.call("redo", json!({}));
    assert!(
        client.aside.iter().all(|f| f.kind() != "progress"),
        "no progress frames without progress:true: {:?}",
        client.aside
    );
    assert_eq!(client.bye(), 0);
}

#[test]
fn a_closed_stdin_ends_the_host_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::spawn(dir.path());
    assert_eq!(client.recv().kind(), "ready");
    drop(client.stdin);
    let status = client.child.wait().unwrap();
    assert_eq!(status.code(), Some(0));
}

#[test]
fn a_missing_documents_flag_is_a_usage_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_waffle-host"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--documents"));
}
