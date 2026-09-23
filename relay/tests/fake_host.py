#!/usr/bin/env python3
"""A stand-in `waffle-host` speaking `waffle-host/1` over stdio, for the relay's own tests.

It serves a small tool set, echoes arguments back as results, emits progress
frames for a tool that asks, and exits abruptly (a "kernel crash") on one
named tool — enough to exercise the relay's `HostBackend` without the real
engine. Behaviour knobs come from the environment:

- `FAKE_HOST_PROTOCOL`: the protocol string to announce (default the real one).
- `FAKE_HOST_NO_READY=1`: exit before saying ready.
- `FAKE_HOST_STATE_FILE`: a path; the fake writes each `document_open` id
  there, so a test can see what a restarted host was told to reopen.
"""

from __future__ import annotations

import json
import os
import struct
import sys
from pathlib import Path
from typing import Any

PREFIX = struct.Struct(">II")
# All manifest names, so the relay lists them; `feature_add` is the one with
# behaviour: `{"crash": true}` ends the process mid-call (a kernel abort),
# anything else answers after three progress frames (when asked for them).
TOOLS = [
    "model_summary",
    "feature_add",
    "document_info",
    "document_open",
    "document_new",
    "document_import",
    "storage_list",
]


def write(header: dict[str, Any], payload: bytes = b"") -> None:
    body = json.dumps(header).encode()
    sys.stdout.buffer.write(PREFIX.pack(len(body), len(payload)) + body + payload)
    sys.stdout.buffer.flush()


# Viewer sync (spec §4): one body whose blob is these bytes; `feature_add`
# bumps the revision and pushes a snapshot, as the real host does.
BLOB_ID = "m1"
BLOB_BYTES = b"BLOB-BYTES"
REVISION = 0


def snapshot(epoch: str, request_id: str | None = None) -> None:
    header: dict[str, Any] = {
        "type": "snapshot",
        "protocol": "waffle-viewer/1",
        "epoch": epoch,
        "revision": REVISION,
        "document": {"id": "doc", "name": "Fake", "tabs": [], "active_tab": "t1"},
        "tree": {"features": [], "active_index": None},
        "errors": [],
        "warnings": [],
        "bodies": [
            {
                "body_index": 0,
                "bodyId": "f1/Main",
                "name": "Body",
                "mesh_id": BLOB_ID,
                "encoding": "raw/1",
                "byte_length": len(BLOB_BYTES),
            }
        ],
    }
    if request_id is not None:
        header["id"] = request_id
    write(header)


def read() -> dict[str, Any] | None:
    prefix = sys.stdin.buffer.read(PREFIX.size)
    if len(prefix) < PREFIX.size:
        return None
    header_len, payload_len = PREFIX.unpack(prefix)
    header = json.loads(sys.stdin.buffer.read(header_len))
    sys.stdin.buffer.read(payload_len)
    return header


def result(call_id: str, structured: Any, is_error: bool = False) -> None:
    write(
        {
            "type": "result",
            "id": call_id,
            "content": [{"type": "text", "text": json.dumps(structured)}],
            "structuredContent": structured,
            "isError": is_error,
        }
    )


def main() -> int:
    args = sys.argv[1:]
    documents = args[args.index("--documents") + 1] if "--documents" in args else None
    if os.environ.get("FAKE_HOST_NO_READY") == "1":
        print("fake-host: dying before ready", file=sys.stderr)
        return 3
    write(
        {
            "type": "ready",
            "protocol": os.environ.get("FAKE_HOST_PROTOCOL", "waffle-host/1"),
            "host_build": {"version": "fake"},
            "epoch": f"epoch-{os.getpid()}",
            "tools": TOOLS,
            "documents": documents,
        }
    )
    state_file = os.environ.get("FAKE_HOST_STATE_FILE")
    open_id: str | None = None
    epoch = f"epoch-{os.getpid()}"
    global REVISION
    while True:
        frame = read()
        if frame is None:
            return 0
        kind = frame.get("type")
        if kind == "bye":
            write({"type": "bye", "reason": "requested"})
            return 0
        if kind == "snapshot":
            snapshot(epoch, frame.get("id"))
            continue
        if kind == "blob":
            mesh_id = frame.get("mesh_id")
            if mesh_id == BLOB_ID:
                write(
                    {
                        "type": "blob",
                        "id": frame.get("id"),
                        "mesh_id": mesh_id,
                        "encoding": "raw/1",
                        "byte_length": len(BLOB_BYTES),
                    },
                    BLOB_BYTES,
                )
            else:
                write({"type": "blob", "id": frame.get("id"), "mesh_id": mesh_id, "missing": True})
            continue
        if kind != "tool":
            continue
        call_id = frame["id"]
        name = frame["name"]
        arguments = frame.get("arguments") or {}
        context = frame.get("context") or {}
        if name == "feature_add" and arguments.get("crash"):
            print("fake-host: simulated kernel abort", file=sys.stderr)
            os._exit(134)
        if name == "feature_add":
            for i in range(3):
                if context.get("progress"):
                    write(
                        {
                            "type": "progress",
                            "id": call_id,
                            "message": f"step {i + 1}",
                            "elapsed_ms": i * 10,
                            "progress": i + 1,
                            "total": 3,
                        }
                    )
            result(call_id, {"done": True})
            REVISION += 1
            snapshot(epoch)
            continue
        if name == "document_open":
            open_id = arguments.get("id")
            if state_file:
                Path(state_file).write_text(json.dumps({"opened": open_id, "pid": os.getpid()}))
            result(call_id, {"storage_id": open_id, "name": f"doc {open_id}"})
            continue
        if name == "document_new":
            open_id = "new-" + str(os.getpid())
            result(call_id, {"storage_id": open_id, "name": arguments.get("name", "Untitled")})
            continue
        if name == "document_info":
            result(call_id, {"storage_id": open_id, "name": "info"})
            continue
        result(
            call_id,
            {
                "echo": {
                    "name": name,
                    "arguments": arguments,
                    "agent_name": context.get("agent_name"),
                },
                "pid": os.getpid(),
            },
        )


if __name__ == "__main__":
    sys.exit(main())
