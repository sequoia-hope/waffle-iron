"""`--kernel host`: the `waffle-host` child process over `waffle-host/1` frames
(`specs/waffle_server_mode.md` §2.4, §3.4, §4.8).

The relay spawns the host, reads its `ready` frame (which names the tools it
serves — the host is the authority on what it implements, so the manifest
needs no per-tool host tags), and forwards each page tool call as a `tool`
frame. Crash isolation (oracle H4): a host that exits — a kernel abort, an
OOM — fails every in-flight call with `EngineCrashed`, is restarted on the
next call, and reopens the document it had open from the host's own
autosave (`document_open` of the last known id), so the agent's next call
finds the model as of the last committed tool.

Frame format: `u32 BE header_len | u32 BE payload_len | header JSON |
payload` (`crates/waffle-host/src/frames.rs`).
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
import shutil
import struct
import uuid
from collections.abc import Awaitable, Callable
from pathlib import Path
from typing import Any

from waffle_mcp_relay.link import LinkError
from waffle_mcp_relay.manifest import Manifest, load_bundled

log = logging.getLogger("waffle_mcp_relay.host")

HOST_PROTOCOL = "waffle-host/1"
HOST_BINARY_NAME = "waffle-host"
READY_TIMEOUT_S = 60.0
MAX_HEADER_BYTES = 64 * 1024 * 1024
MAX_PAYLOAD_BYTES = 256 * 1024 * 1024
_PREFIX = struct.Struct(">II")

CRASHED_MESSAGE = (
    "The engine host exited while the call was in flight (a kernel abort or out of "
    "memory). It is restarted on the next call and reopens the document from its last "
    "autosave; the state of THIS call is unknown — call model_summary before continuing."
)
NO_VIEWER_MESSAGE = (
    "The relay runs the engine in a native host (--kernel host); there is no browser page "
    "to pair. Viewers attach in server mode P-D."
)


class HostError(Exception):
    """The host could not be started or spoke a protocol the relay does not."""


def find_host_binary(explicit: str | None, env: dict[str, str] | None = None) -> Path:
    """`--host-binary`, else `$WAFFLE_HOST_BIN`, else `waffle-host` on PATH; loud otherwise."""
    env = os.environ if env is None else env
    candidate = explicit or env.get("WAFFLE_HOST_BIN")
    if candidate:
        path = Path(candidate)
        if not path.is_file():
            raise HostError(f"host binary not found: {path}")
        return path
    found = shutil.which(HOST_BINARY_NAME, path=env.get("PATH"))
    if found is None:
        raise HostError(
            f"no host binary: pass --host-binary, set $WAFFLE_HOST_BIN, or put "
            f"`{HOST_BINARY_NAME}` on PATH (cargo build -p waffle-host --release)"
        )
    return Path(found)


def encode_frame(header: dict[str, Any], payload: bytes = b"") -> bytes:
    body = json.dumps(header, separators=(",", ":")).encode("utf-8")
    return _PREFIX.pack(len(body), len(payload)) + body + payload


async def read_frame(stream: asyncio.StreamReader) -> tuple[dict[str, Any], bytes] | None:
    """The next frame, or None at a clean end of stream."""
    try:
        prefix = await stream.readexactly(_PREFIX.size)
    except asyncio.IncompleteReadError as err:
        if err.partial:
            raise HostError("host stream ended inside a frame prefix") from None
        return None
    header_len, payload_len = _PREFIX.unpack(prefix)
    if header_len > MAX_HEADER_BYTES or payload_len > MAX_PAYLOAD_BYTES:
        raise HostError(f"host frame too large ({header_len} + {payload_len} bytes)")
    try:
        header_bytes = await stream.readexactly(header_len)
        payload = await stream.readexactly(payload_len)
    except asyncio.IncompleteReadError:
        raise HostError("host stream ended inside a frame") from None
    header = json.loads(header_bytes)
    if not isinstance(header, dict) or not isinstance(header.get("type"), str):
        raise HostError("host frame header must be an object with a string `type`")
    return header, payload


class HostBackend:
    """One `waffle-host` child; restarted after a crash."""

    kernel = "host"

    def __init__(
        self,
        binary: Path,
        documents: Path,
        *,
        agent_name: Callable[[], str],
        ready_timeout: float = READY_TIMEOUT_S,
    ) -> None:
        self._binary = binary
        self._documents = documents
        self._manifest = load_bundled()
        self._agent_name = agent_name
        self._ready_timeout = ready_timeout
        self._proc: asyncio.subprocess.Process | None = None
        self._reader: asyncio.Task[None] | None = None
        self._ready: dict[str, Any] | None = None
        self._tools: frozenset[str] | None = None
        self._pending: dict[str, asyncio.Future[dict[str, Any]]] = {}
        self._progress: dict[str, Callable[[dict[str, Any]], Awaitable[None]]] = {}
        # One tool at a time reaches the host, in arrival order (§3.3 "host command queue").
        self._queue = asyncio.Lock()
        self._crashes = 0
        self._crashed_note_pending = False
        self._document_id: str | None = None
        self._document_name: str | None = None
        self._closing = False

    # -- lifecycle ---------------------------------------------------------

    @property
    def documents(self) -> Path:
        return self._documents

    @property
    def host_build(self) -> Any:
        return None if self._ready is None else self._ready.get("host_build")

    @property
    def crashes(self) -> int:
        return self._crashes

    @property
    def manifest(self) -> Manifest:
        return self._manifest

    def tool_names(self) -> frozenset[str] | None:
        return self._tools

    async def start(self) -> None:
        """Spawn the host and wait for `ready`. Raises HostError, loudly, on anything else."""
        self._documents.mkdir(parents=True, exist_ok=True)
        try:
            proc = await asyncio.create_subprocess_exec(
                str(self._binary),
                "--documents",
                str(self._documents),
                stdin=asyncio.subprocess.PIPE,
                stdout=asyncio.subprocess.PIPE,
                # The host logs to stderr; it flows into the relay's own stderr.
                stderr=None,
                limit=MAX_HEADER_BYTES + MAX_PAYLOAD_BYTES + _PREFIX.size,
            )
        except OSError as err:
            raise HostError(f"cannot start {self._binary}: {err}") from None
        assert proc.stdout is not None
        try:
            first = await asyncio.wait_for(read_frame(proc.stdout), self._ready_timeout)
        except TimeoutError:
            proc.kill()
            await proc.wait()
            raise HostError(f"host sent no ready frame within {self._ready_timeout:g} s") from None
        except HostError:
            proc.kill()
            await proc.wait()
            raise
        if first is None:
            await proc.wait()
            raise HostError(f"host exited before ready (exit code {proc.returncode})")
        ready, _ = first
        if ready.get("type") != "ready" or ready.get("protocol") != HOST_PROTOCOL:
            proc.kill()
            await proc.wait()
            raise HostError(
                f"host spoke {ready.get('protocol')!r} ({ready.get('type')!r}), not {HOST_PROTOCOL}"
            )
        tools = ready.get("tools")
        if not isinstance(tools, list) or not all(isinstance(t, str) for t in tools):
            proc.kill()
            await proc.wait()
            raise HostError("host ready frame names no tools")
        self._proc = proc
        self._ready = ready
        self._tools = frozenset(tools)
        self._reader = asyncio.create_task(self._read_loop(proc))
        log.info(
            "host ready (pid %s, epoch %s, %d tools, documents %s)",
            proc.pid,
            ready.get("epoch"),
            len(tools),
            self._documents,
        )

    async def close(self) -> None:
        self._closing = True
        proc = self._proc
        if proc is None:
            return
        self._proc = None
        if proc.stdin is not None and not proc.stdin.is_closing():
            try:
                proc.stdin.write(encode_frame({"type": "bye", "reason": "relay closing"}))
                await proc.stdin.drain()
                proc.stdin.close()
            except (ConnectionError, OSError):
                pass
        try:
            await asyncio.wait_for(proc.wait(), 10.0)
        except TimeoutError:
            proc.kill()
            await proc.wait()
        if self._reader is not None:
            await asyncio.gather(self._reader, return_exceptions=True)
            self._reader = None

    # -- queries -----------------------------------------------------------

    async def status(self) -> dict[str, Any]:
        out: dict[str, Any] = {
            "state": "ready" if self._proc is not None else "host_down",
            "kernel": "host",
            "documents": str(self._documents),
        }
        if self.host_build is not None:
            out["host_build"] = self.host_build
        if self._document_name is not None:
            out["document_name"] = self._document_name
        if self._crashes:
            out["host_restarts"] = self._crashes
        return out

    async def connect(self) -> tuple[str, float | None]:
        raise LinkError("HostCapability", NO_VIEWER_MESSAGE, {"kernel": "host"})

    # -- calls -------------------------------------------------------------

    async def call(
        self,
        tool: str,
        arguments: dict[str, Any],
        on_progress: Callable[[dict[str, Any]], Awaitable[None]] | None = None,
    ) -> dict[str, Any]:
        async with self._queue:
            if self._proc is None:
                await self._restart()
            proc = self._proc
            assert proc is not None and proc.stdin is not None
            call_id = uuid.uuid4().hex
            future: asyncio.Future[dict[str, Any]] = asyncio.get_running_loop().create_future()
            self._pending[call_id] = future
            if on_progress is not None:
                self._progress[call_id] = on_progress
            frame = {
                "type": "tool",
                "id": call_id,
                "name": tool,
                "arguments": arguments,
                "context": {"agent_name": self._agent_name(), "progress": on_progress is not None},
            }
            try:
                proc.stdin.write(encode_frame(frame))
                await proc.stdin.drain()
            except (ConnectionError, OSError):
                self._pending.pop(call_id, None)
                self._progress.pop(call_id, None)
                raise LinkError("EngineCrashed", CRASHED_MESSAGE, {"state_unknown": True}) from None
            try:
                result = await future
            except asyncio.CancelledError:
                self._pending.pop(call_id, None)
                self._progress.pop(call_id, None)
                try:
                    proc.stdin.write(encode_frame({"type": "cancel", "id": call_id}))
                    await proc.stdin.drain()
                except (ConnectionError, OSError):
                    pass
                raise
        self._track_document(tool, result)
        if self._crashed_note_pending:
            self._crashed_note_pending = False
            note = {
                "type": "text",
                "text": (
                    "Note: the engine host was restarted after a crash and reopened the "
                    "document from its last autosave; the undo history before that is gone."
                ),
            }
            content = result.get("content")
            result = {**result, "content": [*(content if isinstance(content, list) else []), note]}
        return result

    def _track_document(self, tool: str, result: dict[str, Any]) -> None:
        """Remember which document is open, for status and for reopening after a crash."""
        if result.get("isError"):
            return
        structured = result.get("structuredContent")
        if not isinstance(structured, dict):
            return
        if tool in ("document_info", "document_new", "document_open"):
            doc_id = structured.get("storage_id")
            if isinstance(doc_id, str):
                self._document_id = doc_id
            name = structured.get("name")
            if isinstance(name, str):
                self._document_name = name
        elif tool == "document_save":
            doc_id = structured.get("id")
            if isinstance(doc_id, str):
                self._document_id = doc_id
        elif tool == "model_summary":
            name = structured.get("document_name")
            if isinstance(name, str):
                self._document_name = name

    async def _read_loop(self, proc: asyncio.subprocess.Process) -> None:
        assert proc.stdout is not None
        try:
            while True:
                frame = await read_frame(proc.stdout)
                if frame is None:
                    break
                header, _ = frame
                kind = header.get("type")
                call_id = header.get("id")
                if kind == "result" and isinstance(call_id, str):
                    self._progress.pop(call_id, None)
                    future = self._pending.pop(call_id, None)
                    if future is not None and not future.done():
                        future.set_result(
                            {
                                "content": header.get("content") or [],
                                "structuredContent": header.get("structuredContent"),
                                "isError": bool(header.get("isError")),
                            }
                        )
                elif kind == "progress" and isinstance(call_id, str):
                    callback = self._progress.get(call_id)
                    if callback is not None:
                        try:
                            await callback(header)
                        except Exception:  # noqa: BLE001 — a listener's failure must not stop the host
                            log.exception("progress listener failed")
                elif kind == "bye":
                    break
                else:
                    log.warning("ignoring host frame %r", kind)
        except HostError as err:
            log.error("host protocol error: %s", err)
        finally:
            await proc.wait()
            self._on_exit(proc)

    def _on_exit(self, proc: asyncio.subprocess.Process) -> None:
        if self._proc is proc:
            self._proc = None
        if not self._closing:
            self._crashes += 1
            self._crashed_note_pending = True
            log.error("host exited (code %s); it restarts on the next call", proc.returncode)
        for future in self._pending.values():
            if not future.done():
                future.set_exception(
                    LinkError("EngineCrashed", CRASHED_MESSAGE, {"state_unknown": True})
                )
        self._pending.clear()
        self._progress.clear()

    async def _restart(self) -> None:
        """Spawn a fresh host and reopen the document it last had open (§4.8)."""
        if self._reader is not None:
            await asyncio.gather(self._reader, return_exceptions=True)
            self._reader = None
        try:
            await self.start()
        except HostError as err:
            raise LinkError(
                "EngineCrashed",
                f"the engine host could not be restarted: {err}",
                {"state_unknown": True},
            ) from None
        doc_id = self._document_id
        if doc_id is None:
            return
        proc = self._proc
        assert proc is not None and proc.stdin is not None
        call_id = uuid.uuid4().hex
        future: asyncio.Future[dict[str, Any]] = asyncio.get_running_loop().create_future()
        self._pending[call_id] = future
        proc.stdin.write(
            encode_frame(
                {
                    "type": "tool",
                    "id": call_id,
                    "name": "document_open",
                    "arguments": {"id": doc_id},
                    "context": {"agent_name": self._agent_name(), "progress": False},
                }
            )
        )
        await proc.stdin.drain()
        reopened = await future
        if reopened.get("isError"):
            log.error("host restarted but could not reopen document %s: %s", doc_id, reopened)
        else:
            log.info("host restarted and reopened document %s", doc_id)


__all__ = [
    "HOST_PROTOCOL",
    "HostBackend",
    "HostError",
    "encode_frame",
    "find_host_binary",
    "read_frame",
]
