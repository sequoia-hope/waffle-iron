"""The viewer side in host mode: `waffle-viewer/1` over WebSocket
(`specs/waffle_server_mode.md` §4).

A viewer holds no authoritative state: it attaches, gets the host's latest
snapshot (everything but the geometry), asks for the mesh blobs it does not
already hold by content id (`want`), and is sent a fresh snapshot after every
tool that changed the document. Blobs travel as binary frames:
`u32 BE header_len | header JSON | payload`.

Admission is the page link's model (§4.7): the handshake `Origin` must be
allowed, the first frame an `attach` carrying a code from `waffle_connect`
(single use, 300 s; or the relay's persistent code, reusable) or a session
token from an earlier `welcome` (resumable for `resume_s` after the socket
dropped). Several viewers may attach at once, each with its own code or
session. Tokens are random and live in this process (the spec's HMAC tokens,
which survive a relay restart, are a later step; a viewer whose relay
restarted re-attaches with a fresh code).

v1 sends every change as a whole `snapshot` rather than a keyed `update`:
the geometry is not in it, so it is small, and a viewer's state is a pure
function of the latest one.
"""

from __future__ import annotations

import asyncio
import json
import logging
import ssl
import struct
import time
from collections.abc import Callable, Iterable
from dataclasses import dataclass, field
from http import HTTPStatus
from typing import TYPE_CHECKING, Any

from websockets.asyncio.server import Server, ServerConnection, serve
from websockets.datastructures import MultipleValuesError
from websockets.exceptions import ConnectionClosed
from websockets.http11 import Request, Response

from waffle_mcp_relay.link import HELLO_TIMEOUT_S, PING_INTERVAL_S, PONG_TIMEOUT_S
from waffle_mcp_relay.pairing import CODE_TTL_S, SESSION_RESUME_S, _same, new_token

if TYPE_CHECKING:
    from waffle_mcp_relay.host import HostBackend

log = logging.getLogger("waffle_mcp_relay.viewer")

VIEWER_PROTOCOL = "waffle-viewer/1"
# A snapshot is KB; a blob header is smaller. Viewers never send anything big.
MAX_VIEWER_FRAME_BYTES = 1024 * 1024
_BLOB_PREFIX = struct.Struct(">I")


def encode_blob_frame(header: dict[str, Any], payload: bytes) -> bytes:
    """A binary viewer frame: `u32 BE header_len | header JSON | payload`."""
    body = json.dumps(header, separators=(",", ":")).encode("utf-8")
    return _BLOB_PREFIX.pack(len(body)) + body + payload


def decode_blob_frame(raw: bytes) -> tuple[dict[str, Any], bytes]:
    (header_len,) = _BLOB_PREFIX.unpack_from(raw, 0)
    start = _BLOB_PREFIX.size
    header = json.loads(raw[start : start + header_len])
    return header, raw[start + header_len :]


@dataclass(frozen=True)
class ViewerAdmission:
    ok: bool
    viewer_id: str | None = None
    session: str | None = None
    reason: str | None = None
    resumed: bool = False


@dataclass
class _Session:
    viewer_id: str
    connected: bool = False
    disconnected_at: float | None = None


class ViewerPairing:
    """Codes and sessions for any number of viewers (pure state, injected clock)."""

    def __init__(
        self,
        clock: Callable[[], float] = time.time,
        *,
        resume_s: float = SESSION_RESUME_S,
        persistent_code: str | None = None,
    ) -> None:
        self._clock = clock
        self._resume_s = resume_s
        self._persistent_code = persistent_code
        self._codes: dict[str, float] = {}
        self._sessions: dict[str, _Session] = {}

    def issue_code(self) -> tuple[str, float | None]:
        """A fresh single-use code (or the persistent one, which never expires)."""
        if self._persistent_code is not None:
            return self._persistent_code, None
        self._prune()
        code = new_token()
        self._codes[code] = self._clock() + CODE_TTL_S
        return code, self._codes[code]

    def admit_code(self, code: object) -> ViewerAdmission:
        self._prune()
        if _same(self._persistent_code, code):
            return self._start()
        if not isinstance(code, str):
            return ViewerAdmission(False, reason="invalid_code")
        for known, expires_at in list(self._codes.items()):
            if _same(known, code):
                del self._codes[known]
                if self._clock() >= expires_at:
                    return ViewerAdmission(False, reason="invalid_code")
                return self._start()
        return ViewerAdmission(False, reason="invalid_code")

    def admit_session(self, token: object) -> ViewerAdmission:
        self._prune()
        if not isinstance(token, str):
            return ViewerAdmission(False, reason="session_expired")
        for known, session in self._sessions.items():
            if _same(known, token):
                if session.connected:
                    return ViewerAdmission(False, reason="already_attached")
                session.connected = True
                session.disconnected_at = None
                return ViewerAdmission(
                    True, viewer_id=session.viewer_id, session=known, resumed=True
                )
        return ViewerAdmission(False, reason="session_expired")

    def disconnected(self, token: str) -> None:
        session = self._sessions.get(token)
        if session is not None:
            session.connected = False
            session.disconnected_at = self._clock()

    def _start(self) -> ViewerAdmission:
        token = new_token()
        viewer_id = new_token()[:16]
        self._sessions[token] = _Session(viewer_id=viewer_id, connected=True)
        return ViewerAdmission(True, viewer_id=viewer_id, session=token)

    def _prune(self) -> None:
        now = self._clock()
        for code, expires_at in list(self._codes.items()):
            if now >= expires_at:
                del self._codes[code]
        for token, session in list(self._sessions.items()):
            gone = session.disconnected_at
            if not session.connected and gone is not None and now - gone > self._resume_s:
                del self._sessions[token]


@dataclass
class ViewerConnection:
    ws: ServerConnection
    viewer_id: str
    session: str
    visible: bool = True
    last_pong: float = 0.0
    send_lock: asyncio.Lock = field(default_factory=asyncio.Lock)


def _parse(raw: str | bytes) -> dict[str, Any] | None:
    if isinstance(raw, bytes):
        return None
    try:
        frame = json.loads(raw)
    except ValueError:
        return None
    if not isinstance(frame, dict) or not isinstance(frame.get("type"), str):
        return None
    return frame


class ViewerServer:
    def __init__(
        self,
        *,
        host: HostBackend,
        pairing: ViewerPairing,
        allow_origins: Iterable[str],
        ssl_context: ssl.SSLContext | None = None,
        ping_interval: float = PING_INTERVAL_S,
        pong_timeout: float = PONG_TIMEOUT_S,
        hello_timeout: float = HELLO_TIMEOUT_S,
    ) -> None:
        self._host = host
        self.pairing = pairing
        self._origins = frozenset(allow_origins)
        self._ssl = ssl_context
        self._ping_interval = ping_interval
        self._pong_timeout = pong_timeout
        self._hello_timeout = hello_timeout
        self._server: Server | None = None
        self._viewers: dict[str, ViewerConnection] = {}
        host.attach_viewers(self)

    # -- lifecycle ---------------------------------------------------------

    async def start(self, bind: str, port: int) -> None:
        self._server = await serve(
            self._handle,
            bind,
            port,
            process_request=self._check_origin,
            ssl=self._ssl,
            ping_interval=None,  # the viewer protocol has its own ping/pong frames
            max_size=MAX_VIEWER_FRAME_BYTES,
        )

    async def close(self) -> None:
        if self._server is not None:
            self._server.close()
            await self._server.wait_closed()
            self._server = None

    @property
    def viewer_count(self) -> int:
        return len(self._viewers)

    def presence(self) -> list[dict[str, Any]]:
        return [{"viewer_id": v.viewer_id, "visible": v.visible} for v in self._viewers.values()]

    # -- pushes ------------------------------------------------------------

    async def broadcast(self, snapshot: dict[str, Any]) -> None:
        """Every attached viewer gets the host's new snapshot (§4.6 step 2)."""
        if not self._viewers:
            return
        text = json.dumps(snapshot)
        for viewer in list(self._viewers.values()):
            try:
                async with viewer.send_lock:
                    await viewer.ws.send(text)
            except ConnectionClosed:
                pass

    # -- handshake ---------------------------------------------------------

    def _check_origin(self, connection: ServerConnection, request: Request) -> Response | None:
        try:
            origin = request.headers.get("Origin")
        except MultipleValuesError:
            origin = None
        if origin is not None and origin in self._origins:
            return None
        log.warning("refused viewer handshake: Origin %r not allowed", origin)
        return connection.respond(HTTPStatus.FORBIDDEN, "Origin not allowed\n")

    async def _bye(self, ws: ServerConnection, reason: str, **extra: Any) -> None:
        try:
            await ws.send(json.dumps({"type": "bye", "reason": reason, **extra}))
            await ws.close(1000, reason)
        except ConnectionClosed:
            pass

    async def _handle(self, ws: ServerConnection) -> None:
        try:
            raw = await asyncio.wait_for(ws.recv(), self._hello_timeout)
        except (TimeoutError, ConnectionClosed):
            await ws.close(1008, "expected attach")
            return
        attach = _parse(raw)
        if attach is None or attach["type"] != "attach":
            await ws.close(1008, "expected attach")
            return
        if attach.get("protocol") != VIEWER_PROTOCOL:
            await self._bye(ws, "protocol_mismatch", supported=[VIEWER_PROTOCOL])
            return
        if "code" in attach:
            admission = self.pairing.admit_code(attach.get("code"))
        elif "session" in attach:
            admission = self.pairing.admit_session(attach.get("session"))
        else:
            admission = ViewerAdmission(False, reason="invalid_code")
        if not admission.ok:
            log.warning("refused viewer: %s", admission.reason)
            await self._bye(ws, admission.reason or "invalid_code")
            return
        assert admission.viewer_id is not None and admission.session is not None
        viewer = ViewerConnection(
            ws=ws,
            viewer_id=admission.viewer_id,
            session=admission.session,
            visible=attach.get("visible") is not False,
        )
        self._viewers[viewer.viewer_id] = viewer
        log.info("viewer %s %s", viewer.viewer_id, "resumed" if admission.resumed else "attached")
        try:
            welcome = {
                "type": "welcome",
                "protocol": VIEWER_PROTOCOL,
                "viewer_id": viewer.viewer_id,
                "session": viewer.session,
                "epoch": self._host.epoch,
                "host_build": self._host.host_build,
            }
            await ws.send(json.dumps(welcome))
            await self._send_snapshot_unless_held(viewer, attach.get("have"))
            await self._serve(viewer)
        except ConnectionClosed:
            pass
        finally:
            self._viewers.pop(viewer.viewer_id, None)
            self.pairing.disconnected(viewer.session)
            log.info("viewer %s disconnected", viewer.viewer_id)

    async def _send_snapshot_unless_held(self, viewer: ViewerConnection, have: object) -> None:
        snapshot = await self._host.latest_snapshot()
        if snapshot is None:
            return
        if (
            isinstance(have, dict)
            and have.get("epoch") == snapshot.get("epoch")
            and have.get("revision") == snapshot.get("revision")
        ):
            # §4.6 step 5: same state — the viewer's cache is the model.
            return
        async with viewer.send_lock:
            await viewer.ws.send(json.dumps(snapshot))

    # -- attached viewer ---------------------------------------------------

    async def _serve(self, viewer: ViewerConnection) -> None:
        loop = asyncio.get_running_loop()
        viewer.last_pong = loop.time()
        heartbeat = asyncio.create_task(self._heartbeat(viewer))
        try:
            async for raw in viewer.ws:
                frame = _parse(raw)
                if frame is None:
                    log.warning("ignored malformed frame from viewer %s", viewer.viewer_id)
                    continue
                kind = frame["type"]
                if kind == "want":
                    ids = frame.get("mesh_ids")
                    if isinstance(ids, list):
                        for mesh_id in ids:
                            if isinstance(mesh_id, str):
                                await self._send_blob(viewer, mesh_id)
                elif kind == "snapshot":
                    # An explicit resync (§4.4: any gap ⇒ snapshot).
                    snapshot = await self._host.latest_snapshot()
                    if snapshot is not None:
                        async with viewer.send_lock:
                            await viewer.ws.send(json.dumps(snapshot))
                elif kind == "visible":
                    viewer.visible = frame.get("visible") is not False
                elif kind == "pong":
                    viewer.last_pong = loop.time()
                elif kind == "ping":
                    async with viewer.send_lock:
                        await viewer.ws.send(json.dumps({"type": "pong"}))
                elif kind == "bye":
                    await viewer.ws.close(1000, "bye")
                    return
                else:
                    log.warning("ignored unknown frame type %r from viewer", kind)
        finally:
            heartbeat.cancel()

    async def _send_blob(self, viewer: ViewerConnection, mesh_id: str) -> None:
        found = await self._host.request_blob(mesh_id)
        async with viewer.send_lock:
            if found is None:
                await viewer.ws.send(
                    json.dumps({"type": "blob", "mesh_id": mesh_id, "missing": True})
                )
                return
            header, payload = found
            await viewer.ws.send(
                encode_blob_frame(
                    {
                        "type": "blob",
                        "mesh_id": mesh_id,
                        "encoding": header.get("encoding"),
                        "byte_length": len(payload),
                    },
                    payload,
                )
            )

    async def _heartbeat(self, viewer: ViewerConnection) -> None:
        loop = asyncio.get_running_loop()
        try:
            while True:
                await asyncio.sleep(self._ping_interval)
                if loop.time() - viewer.last_pong > self._pong_timeout:
                    log.warning("viewer %s missed heartbeat; disconnecting", viewer.viewer_id)
                    await viewer.ws.close(1001, "heartbeat timeout")
                    return
                async with viewer.send_lock:
                    await viewer.ws.send(json.dumps({"type": "ping"}))
        except ConnectionClosed:
            return


__all__ = [
    "VIEWER_PROTOCOL",
    "ViewerPairing",
    "ViewerServer",
    "decode_blob_frame",
    "encode_blob_frame",
]
