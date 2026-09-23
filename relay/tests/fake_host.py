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
    "storage_list",
]


def write(header: dict[str, Any]) -> None:
    body = json.dumps(header).encode()
    sys.stdout.buffer.write(PREFIX.pack(len(body), 0) + body)
    sys.stdout.buffer.flush()


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
    while True:
        frame = read()
        if frame is None:
            return 0
        kind = frame.get("type")
        if kind == "bye":
            write({"type": "bye", "reason": "requested"})
            return 0
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
