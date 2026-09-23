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
        // A snapshot the host pushed after the last change may still be
        // unread; the host answers `bye` after it.
        let frame = loop {
            let frame = self.recv();
            if frame.kind() == "bye" {
                break frame;
            }
            self.aside.push(frame);
        };
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
        // The tab and assembly tools moved into the engine (2026-09-23), so
        // the host serves an assembly document.
        "tab_add",
        "tab_switch",
        "assembly_get",
        "instance_add",
        "mate_add",
    ] {
        assert!(tools.contains(&name), "ready.tools lacks {name}");
    }
    for name in ["selection_get", "viewport_capture"] {
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
    // The assembly tools answer from the engine: a Part tab refuses them the
    // way the page did.
    let err = client.refused("assembly_get", json!({}));
    assert_eq!(err["code"], "TabKindNotSupported");
    // A tab tool's answer is the page's `document_info` shape: the engine's
    // share overlaid with this host's storage fields.
    let added = client.ok("tab_add", json!({ "kind": "Assembly", "name": "Asm" }));
    assert_eq!(added["tabs"].as_array().unwrap().len(), 2);
    assert_eq!(added["active_tab"], added["tab_id"]);
    assert_eq!(added["storage_provider"]["id"], "file");
    assert_eq!(added["unsaved"], false);
    let asm = client.ok("assembly_get", json!({}));
    assert_eq!(asm["tab_id"], added["tab_id"]);
    assert_eq!(asm["instances"].as_array().unwrap().len(), 0);
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
fn a_waffle_file_imports_under_its_own_identity_and_lands_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::spawn(dir.path());
    let ready = client.recv();
    assert!(ready.header["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t == "document_import"));

    // Compose a real file with the engine, then import that text as if it
    // came from disk (the file's own identity is the record key).
    let info = client.ok("document_new", json!({ "name": "Source" }));
    let source_id = info["document_id"].as_str().unwrap().to_string();
    client.ok(
        "sketch_create",
        json!({
            "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] },
            "entities": rectangle(),
        }),
    );
    let text = std::fs::read_to_string(dir.path().join(format!("{source_id}.waffle"))).unwrap();
    client.ok("document_new", json!({ "name": "Other" }));

    let imported = client.ok(
        "document_import",
        json!({ "file_name": "Pinwheel.waffle", "text": text }),
    );
    assert_eq!(imported["document_id"], source_id, "identity from the file");
    assert_eq!(imported["storage_id"], source_id);
    assert_eq!(
        imported["name"], "Pinwheel",
        "the file name wins over the stored name"
    );
    let summary = client.ok("model_summary", json!({}));
    assert_eq!(
        summary["features"].as_array().unwrap().len(),
        1,
        "loaded, not just stored"
    );
    let stored: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(format!("{source_id}.waffle"))).unwrap(),
    )
    .unwrap();
    assert_eq!(
        stored["document"]["name"], "Pinwheel",
        "the record on disk follows"
    );
    let listed = client.ok("storage_list", json!({}));
    assert_eq!(
        listed["documents"].as_array().unwrap().len(),
        2,
        "re-homed, not duplicated"
    );

    let named = client.ok(
        "document_import",
        json!({ "file_name": "x.json", "text": text, "name": "Given" }),
    );
    assert_eq!(named["name"], "Given");

    let err = client.refused(
        "document_import",
        json!({ "file_name": "bad.waffle", "text": "{not json" }),
    );
    assert_eq!(err["code"], "InvalidDocument");
    let err = client.refused(
        "document_import",
        json!({ "file_name": "future.waffle", "text": "{\"format\":\"waffle-iron\",\"version\":999,\"min_reader_version\":999}" }),
    );
    assert_eq!(err["code"], "FormatTooNew");
    assert_eq!(err["details"]["file_version"], 999);
    let err = client.refused(
        "document_import",
        json!({ "file_name": "wrong.waffle", "text": "{\"format\":\"other\"}" }),
    );
    assert_eq!(err["code"], "InvalidDocument");
    assert_eq!(
        client.ok("document_info", json!({}))["name"],
        "Given",
        "a refused import leaves the open document alone"
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

/// Viewer sync, §4.3 `rebuild` and §4.5 `mq/1`: a tool that can change the
/// document is bracketed by `rebuild{started}` / `rebuild{done}` frames
/// (a read-only tool by none), and a blob asked for in `mq/1` decodes to the
/// same triangles within quantization — while an unknown encoding is
/// `missing`, never a guess.
#[test]
fn a_rebuild_is_announced_and_a_blob_answers_in_the_compact_encoding() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::spawn(dir.path());
    assert_eq!(client.recv().kind(), "ready");
    client.ok("document_new", json!({ "name": "Compact" }));
    // The push after document_new is read during the next call; a read-only
    // tool drains it so the frames counted below are sketch_create's own.
    client.ok("model_summary", json!({}));
    client.aside.clear();
    let sketch = client.ok(
        "sketch_create",
        json!({ "plane": { "origin": [0, 0, 0], "normal": [0, 0, 1] }, "entities": rectangle() }),
    );
    let rebuilds: Vec<Value> = client
        .aside
        .iter()
        .filter(|f| f.kind() == "rebuild")
        .map(|f| f.header.clone())
        .collect();
    // `started` first, `done` last, any number of `progress` between (the
    // engine reports per feature), and the snapshot push after `done`.
    assert!(
        rebuilds.len() >= 2,
        "started + done around sketch_create: {rebuilds:?}"
    );
    assert_eq!(rebuilds[0]["state"], "started");
    assert_eq!(rebuilds[0]["tool"], "sketch_create");
    assert_eq!(rebuilds.last().unwrap()["state"], "done");
    assert_eq!(rebuilds.last().unwrap()["ok"], true);
    for middle in &rebuilds[1..rebuilds.len() - 1] {
        assert_eq!(middle["state"], "progress", "{middle}");
    }
    // Every frame before the result is a rebuild frame; the snapshot push
    // follows the result (and is read during the next call).
    assert!(client.aside.iter().all(|f| f.kind() == "rebuild"));

    client.aside.clear();
    client.ok("model_summary", json!({}));
    let kinds: Vec<&str> = client.aside.iter().map(Frame::kind).collect();
    assert_eq!(
        kinds,
        vec!["snapshot"],
        "the push after sketch_create; a read-only tool announces no rebuild"
    );

    client.ok(
        "feature_add",
        json!({
            "operation": {
                "type": "Extrude",
                "params": {
                    "sketch_id": sketch["feature_id"],
                    "profile_index": 0,
                    "profile_entity_ids": [5, 6, 7, 8],
                    "depth": 0.005,
                    "symmetric": false,
                    "cut": false
                }
            }
        }),
    );
    client.send(&json!({ "type": "snapshot", "id": "s1" }));
    let snapshot = loop {
        let frame = client.recv();
        if frame.kind() == "snapshot" && frame.header["id"] == "s1" {
            break frame.header;
        }
    };
    let mesh_id = snapshot["bodies"][0]["mesh_id"]
        .as_str()
        .unwrap()
        .to_string();

    client.send(&json!({ "type": "blob", "id": "raw", "mesh_id": mesh_id }));
    let raw = loop {
        let frame = client.recv();
        if frame.kind() == "blob" {
            break frame;
        }
    };
    client.send(&json!({ "type": "blob", "id": "mq", "mesh_id": mesh_id, "encoding": "mq/1" }));
    let compact = loop {
        let frame = client.recv();
        if frame.kind() == "blob" {
            break frame;
        }
    };
    assert_eq!(compact.header["encoding"], "mq/1");
    assert_eq!(compact.header["byte_length"], compact.payload.len());
    let raw_mesh = waffle_host::mq::parse_raw(&raw.payload).expect("raw/1 parses");
    let mq_mesh = waffle_host::mq::decode(&compact.payload).expect("mq/1 decodes");
    assert_eq!(
        waffle_host::mq::canonical_triangles(&mq_mesh.indices),
        waffle_host::mq::canonical_triangles(&raw_mesh.indices),
        "the triangles are exact (the codec may rotate a triangle's start vertex)"
    );
    assert_eq!(
        mq_mesh.header["face_ranges"],
        raw_mesh.header["face_ranges"]
    );
    for (a, b) in mq_mesh.positions.iter().zip(&raw_mesh.positions) {
        assert!((a - b).abs() < 1e-5, "{a} vs {b}");
    }
    // The same id in the same encoding is the same bytes (a cache answers it).
    client.send(&json!({ "type": "blob", "id": "mq2", "mesh_id": mesh_id, "encoding": "mq/1" }));
    let again = loop {
        let frame = client.recv();
        if frame.kind() == "blob" {
            break frame;
        }
    };
    assert_eq!(again.payload, compact.payload);

    client.send(&json!({ "type": "blob", "id": "x", "mesh_id": mesh_id, "encoding": "nope/9" }));
    let unknown = loop {
        let frame = client.recv();
        if frame.kind() == "blob" {
            break frame;
        }
    };
    assert_eq!(unknown.header["missing"], true);
    assert_eq!(client.bye(), 0);
}

/// A document whose active tab is an Assembly is evaluated when the host
/// opens or imports it, as the page evaluates it on open — otherwise the
/// host (and every viewer) shows the assembly with no bodies. Found
/// 2026-09-23 importing the gravel bike example headless.
#[test]
fn an_opened_assembly_document_is_evaluated() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::spawn(dir.path());
    assert_eq!(client.recv().kind(), "ready");
    let doc = client.ok("document_new", json!({ "name": "Asm" }));
    let part = doc["tabs"][0]["id"].as_str().unwrap().to_string();
    let sketch = client.ok(
        "sketch_create",
        json!({ "plane": { "origin": [0, 0, 0], "normal": [0, 0, 1] }, "entities": rectangle() }),
    );
    client.ok(
        "feature_add",
        json!({ "operation": { "type": "Extrude", "params": {
            "sketch_id": sketch["feature_id"], "profile_index": 0, "profile_entity_ids": [5, 6, 7, 8],
            "depth": 0.005, "symmetric": false, "cut": false
        } } }),
    );
    client.ok("tab_add", json!({ "kind": "Assembly", "name": "Top" }));
    client.ok("instance_add", json!({ "tab_id": part, "name": "One" }));
    let bodies_on_assembly = |client: &mut Client, tag: &str| -> usize {
        client.send(&json!({ "type": "snapshot", "id": tag }));
        loop {
            let frame = client.recv();
            if frame.kind() == "snapshot" && frame.header["id"] == tag {
                break frame.header["bodies"].as_array().unwrap().len();
            }
        }
    };
    assert_eq!(
        bodies_on_assembly(&mut client, "built"),
        1,
        "the placed instance renders"
    );
    let saved = client.ok("document_save", json!({}));
    let id = saved["id"].as_str().unwrap().to_string();
    let text = std::fs::read_to_string(dir.path().join(format!("{id}.waffle"))).unwrap();

    // Reopen from disk: the active Assembly tab is evaluated on open.
    client.ok("document_new", json!({ "name": "Other" }));
    assert_eq!(bodies_on_assembly(&mut client, "empty"), 0);
    let reopened = client.ok("document_open", json!({ "id": id }));
    assert_eq!(
        reopened["active_tab"],
        doc["tabs"][0]["id"]
            .as_str()
            .map(|_| reopened["active_tab"].clone())
            .unwrap()
    );
    assert_eq!(
        bodies_on_assembly(&mut client, "reopened"),
        1,
        "document_open evaluates the assembly"
    );

    // And on import.
    client.ok("document_new", json!({ "name": "Other 2" }));
    client.ok(
        "document_import",
        json!({ "file_name": "asm.waffle", "text": text }),
    );
    assert_eq!(
        bodies_on_assembly(&mut client, "imported"),
        1,
        "document_import evaluates the assembly"
    );

    // Switching to the part and back to the assembly keeps rendering.
    client.ok("tab_switch", json!({ "tab_id": part }));
    assert_eq!(
        bodies_on_assembly(&mut client, "part"),
        1,
        "the part's own body after the switch"
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

/// Viewer sync (`specs/waffle_server_mode.md` §4): a snapshot names every
/// rendered body by the content id of its `raw/1` blob, a blob answers by id
/// with the bytes as the frame's payload, an unknown id is `missing`, an
/// unchanged body keeps its id, and every tool that changed the document is
/// followed by an unsolicited snapshot.
#[test]
fn a_snapshot_names_every_body_and_blobs_answer_by_id() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::spawn(dir.path());
    assert_eq!(client.recv().kind(), "ready");

    let sketch = client.ok(
        "sketch_create",
        json!({
            "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] },
            "entities": rectangle(),
        }),
    );
    let sketch_id = sketch["feature_id"].as_str().unwrap().to_string();
    client.ok(
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
    // The push after `feature_add` is read on the way to the next result.
    client.ok("model_summary", json!({}));
    let pushed: Vec<&Frame> = client
        .aside
        .iter()
        .filter(|f| f.kind() == "snapshot")
        .collect();
    assert!(
        pushed.iter().any(|f| f.header.get("id").is_none()),
        "a change is followed by an unsolicited snapshot: {:?}",
        client.aside.iter().map(|f| f.kind()).collect::<Vec<_>>()
    );

    client.send(&json!({ "type": "snapshot", "id": "s1" }));
    let snapshot = loop {
        let frame = client.recv();
        if frame.kind() == "snapshot" && frame.header["id"] == "s1" {
            break frame.header;
        }
    };
    assert_eq!(snapshot["protocol"], "waffle-viewer/1");
    assert!(snapshot["epoch"].as_str().is_some_and(|e| !e.is_empty()));
    assert!(snapshot["revision"].as_u64().unwrap() > 0);
    assert_eq!(snapshot["document"]["tabs"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["tree"]["features"].as_array().unwrap().len(), 2);
    let bodies = snapshot["bodies"].as_array().unwrap();
    assert_eq!(bodies.len(), 1, "one extruded body");
    let body = &bodies[0];
    let mesh_id = body["mesh_id"].as_str().unwrap().to_string();
    assert_eq!(mesh_id.len(), 40, "a content hash, hex");
    assert_eq!(body["encoding"], "raw/1");
    assert!(body["byte_length"].as_u64().unwrap() > 0);
    assert_eq!(body["triangle_count"], 12, "a box is twelve triangles");
    assert!(body["bodyId"].as_str().unwrap().ends_with("/Main"));
    // The bbox is from the f32 render mesh.
    assert!((body["bbox"]["max"][2].as_f64().unwrap() - 0.005).abs() < 1e-6);

    client.send(&json!({ "type": "blob", "id": "b1", "mesh_id": mesh_id }));
    let blob = loop {
        let frame = client.recv();
        if frame.kind() == "blob" && frame.header["id"] == "b1" {
            break frame;
        }
    };
    assert_eq!(blob.header["mesh_id"], mesh_id);
    assert_eq!(blob.header["encoding"], "raw/1");
    assert_eq!(blob.header["byte_length"], blob.payload.len());
    assert_eq!(
        blob.payload.len(),
        body["byte_length"].as_u64().unwrap() as usize
    );
    // raw/1: u32 LE header length (padded to 4), JSON header, then the buffers.
    let header_len = u32::from_le_bytes(blob.payload[0..4].try_into().unwrap()) as usize;
    assert_eq!(header_len % 4, 0);
    let header_text = std::str::from_utf8(&blob.payload[4..4 + header_len])
        .unwrap()
        .trim_end_matches('\0');
    let header: Value = serde_json::from_str(header_text).unwrap();
    let v = header["vertex_count"].as_u64().unwrap() as usize;
    let i = header["index_count"].as_u64().unwrap() as usize;
    let e = header["edge_vertex_count"].as_u64().unwrap() as usize;
    assert_eq!(i, 36);
    assert_eq!(
        header["face_ranges"].as_array().unwrap().len(),
        6,
        "six faces"
    );
    assert!(!header["edge_ranges"].as_array().unwrap().is_empty());
    assert_eq!(
        blob.payload.len(),
        4 + header_len + v * 12 + v * 12 + i * 4 + e * 12
    );

    client.send(&json!({ "type": "blob", "id": "b2", "mesh_id": "nope" }));
    let missing = loop {
        let frame = client.recv();
        if frame.kind() == "blob" && frame.header["id"] == "b2" {
            break frame;
        }
    };
    assert_eq!(missing.header["missing"], true);
    assert!(missing.payload.is_empty());

    // An unchanged body keeps its id across snapshots.
    client.send(&json!({ "type": "snapshot", "id": "s2" }));
    let again = loop {
        let frame = client.recv();
        if frame.kind() == "snapshot" && frame.header["id"] == "s2" {
            break frame.header;
        }
    };
    assert_eq!(again["bodies"][0]["mesh_id"], mesh_id);

    assert_eq!(client.bye(), 0);
}

/// Oracle V2, the half that is the host's: a mesh id is the CONTENT's, so a
/// document reopened in a FRESH host process names the same blobs and a
/// viewer's cache answers for every one of them — "zero blob requests when H3
/// holds" (§4.4). Two processes, the same document, the same ids.
#[test]
fn a_restarted_host_names_the_same_mesh_ids() {
    let dir = tempfile::tempdir().unwrap();

    let ids_of = |client: &mut Client, tag: &str| -> Vec<String> {
        client.send(&json!({ "type": "snapshot", "id": tag }));
        loop {
            let frame = client.recv();
            if frame.kind() == "snapshot" && frame.header["id"] == tag {
                break frame.header["bodies"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|b| b["mesh_id"].as_str().unwrap().to_string())
                    .collect();
            }
        }
    };

    let mut first = Client::spawn(dir.path());
    assert_eq!(first.recv().kind(), "ready");
    let doc = first.ok("document_new", json!({ "name": "Restart" }));
    let id = doc["document_id"].as_str().unwrap().to_string();
    let sketch = first.ok(
        "sketch_create",
        json!({ "plane": { "origin": [0, 0, 0], "normal": [0, 0, 1] }, "entities": rectangle() }),
    );
    first.ok(
        "feature_add",
        json!({ "operation": { "type": "Extrude", "params": {
            "sketch_id": sketch["feature_id"], "profile_index": 0, "profile_entity_ids": [5, 6, 7, 8],
            "depth": 0.005, "symmetric": false, "cut": false
        } } }),
    );
    let before = ids_of(&mut first, "before");
    assert_eq!(before.len(), 1);
    let first_epoch = {
        first.send(&json!({ "type": "snapshot", "id": "e1" }));
        loop {
            let frame = first.recv();
            if frame.kind() == "snapshot" && frame.header["id"] == "e1" {
                break frame.header["epoch"].as_str().unwrap().to_string();
            }
        }
    };
    assert_eq!(first.bye(), 0);

    // A new process over the same documents directory: the autosaved record is
    // reopened and re-tessellated from scratch.
    let mut second = Client::spawn(dir.path());
    assert_eq!(second.recv().kind(), "ready");
    second.ok("document_open", json!({ "id": id }));
    let after = ids_of(&mut second, "after");
    assert_eq!(after, before, "the same geometry hashes to the same ids");
    let second_epoch = {
        second.send(&json!({ "type": "snapshot", "id": "e2" }));
        loop {
            let frame = second.recv();
            if frame.kind() == "snapshot" && frame.header["id"] == "e2" {
                break frame.header["epoch"].as_str().unwrap().to_string();
            }
        }
    };
    assert_ne!(
        second_epoch, first_epoch,
        "a new process is a new epoch, so a viewer resyncs rather than assuming"
    );
    // And the blob still answers by that id in the new process.
    second.send(&json!({ "type": "blob", "id": "b", "mesh_id": after[0] }));
    let blob = loop {
        let frame = second.recv();
        if frame.kind() == "blob" {
            break frame;
        }
    };
    assert!(blob.header["missing"].is_null());
    assert_eq!(blob.header["byte_length"], blob.payload.len());
    assert_eq!(second.bye(), 0);
}
