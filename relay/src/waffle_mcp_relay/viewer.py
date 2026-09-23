"""The viewer side in host mode: `waffle-viewer/1` over WebSocket
(`specs/waffle_server_mode.md` §4).

A viewer holds no authoritative state: it attaches, gets the host's latest
snapshot (everything but the geometry), asks for the mesh blobs it does not
already hold by content id (`want`), and is sent the change after every tool
that changed the document — as a keyed `update` (§4.3: only the top-level
keys that changed, latest-wins) when it holds the state the update builds
on, else a whole `snapshot`. `rebuild` frames from the host ride through
while a tool runs (§4.6 step 2, the viewer's spinner). Blobs travel as binary
frames: `u32 BE header_len | header JSON | payload`, in the encoding the
viewer asked for (§4.5: `raw/1`, or the compact `mq/1`).

Admission is the page link's model (§4.7): the handshake `Origin` must be
allowed, the first frame an `attach` carrying a code from `waffle_connect`
(single use, 300 s; or the relay's persistent code, reusable) or a session
token from an earlier `welcome`. A session token is `HMAC(secret, viewer_id,
expiry)`: it survives a host or relay restart as long as the secret file
does, and it is refreshed with every heartbeat so a tab killed mid-session
resumes for the resume window after its last sign of life. Several viewers
may attach at once, each with its own code or session.

The three viewer tools (`selection_get`, `viewport_view`, `viewport_capture`)
are served here from the FOCUSED visible viewer — the one whose input the
relay saw last — through `capture_request` / `view_request` round trips and
the viewer's last `select` frame; with no visible viewer they are refused
with `ViewerUnavailable`, the code the host itself answers.
"""

from __future__ import annotations

import asyncio
import hashlib
import hmac
import json
import logging
import ssl
import struct
import time
import uuid
from collections.abc import Callable, Iterable
from dataclasses import dataclass, field
from http import HTTPStatus
from typing import TYPE_CHECKING, Any

from websockets.asyncio.server import Server, ServerConnection, serve
from websockets.datastructures import MultipleValuesError
from websockets.exceptions import ConnectionClosed
from websockets.http11 import Request, Response

from waffle_mcp_relay.link import HELLO_TIMEOUT_S, PING_INTERVAL_S, PONG_TIMEOUT_S, LinkError
from waffle_mcp_relay.pairing import CODE_TTL_S, SESSION_RESUME_S, _same, new_token

if TYPE_CHECKING:
    from waffle_mcp_relay.host import HostBackend

log = logging.getLogger("waffle_mcp_relay.viewer")

VIEWER_PROTOCOL = "waffle-viewer/1"
# The page tools a viewer answers (host.rs `VIEWER_TOOLS`).
VIEWER_TOOLS = ("selection_get", "viewport_view", "viewport_capture")
# Blob encodings, in the order a viewer that names none gets them.
ENCODINGS = ("raw/1", "mq/1")
DEFAULT_ENCODING = "raw/1"
# A snapshot is KB; a blob header is smaller; a capture result is a PNG.
MAX_VIEWER_FRAME_BYTES = 32 * 1024 * 1024
# How long a viewer gets to answer a capture or view request.
VIEWER_TOOL_TIMEOUT_S = 30.0
_BLOB_PREFIX = struct.Struct(">I")
# Snapshot keys that name the frame, not the document: never in an update's diff.
_FRAME_KEYS = frozenset({"type", "protocol", "epoch", "revision", "id", "activity"})

NO_VIEWER_MESSAGE = (
    "No visible viewer is attached to this document: the viewport and the selection are a "
    "viewer's. Call waffle_connect, open the viewer link in a browser and keep it in the "
    "foreground, then retry."
)


def encode_blob_frame(header: dict[str, Any], payload: bytes) -> bytes:
    """A binary viewer frame: `u32 BE header_len | header JSON | payload`."""
    body = json.dumps(header, separators=(",", ":")).encode("utf-8")
    return _BLOB_PREFIX.pack(len(body)) + body + payload


def decode_blob_frame(raw: bytes) -> tuple[dict[str, Any], bytes]:
    (header_len,) = _BLOB_PREFIX.unpack_from(raw, 0)
    start = _BLOB_PREFIX.size
    header = json.loads(raw[start : start + header_len])
    return header, raw[start + header_len :]


def snapshot_update(
    previous: dict[str, Any] | None, current: dict[str, Any]
) -> dict[str, Any] | None:
    """The `update` frame (§4.3) from `previous` to `current`, or None when
    no update can express it (no previous state, or another epoch): only the
    top-level keys that changed, latest-wins, `bodies` as the full list."""
    if previous is None or previous.get("epoch") != current.get("epoch"):
        return None
    update: dict[str, Any] = {
        "type": "update",
        "protocol": VIEWER_PROTOCOL,
        "epoch": current.get("epoch"),
        "base_revision": previous.get("revision"),
        "revision": current.get("revision"),
    }
    for key, value in current.items():
        if key in _FRAME_KEYS:
            continue
        if previous.get(key) != value:
            update[key] = value
    return update


@dataclass(frozen=True)
class ViewerAdmission:
    ok: bool
    viewer_id: str | None = None
    session: str | None = None
    expires_at: float | None = None
    reason: str | None = None
    resumed: bool = False


@dataclass
class _Session:
    connected: bool = False
    disconnected_at: float | None = None


class ViewerPairing:
    """Codes and HMAC session tokens for any number of viewers (pure state, injected clock).

    A token is `<viewer_id>.<expiry>.<hmac-sha256 hex>` over the secret. It
    is minted at admission and re-minted on every refresh (the heartbeat),
    each time `resume_s` ahead, so a viewer that goes quiet resumes within
    the window after its last sign of life — with this process, or with the
    next one holding the same secret (§4.7, §4.8). A viewer that is still
    connected cannot be attached twice; one this process saw disconnect is
    held to the window from that moment as well.
    """

    def __init__(
        self,
        clock: Callable[[], float] = time.time,
        *,
        resume_s: float = SESSION_RESUME_S,
        persistent_code: str | None = None,
        secret: bytes | None = None,
    ) -> None:
        self._clock = clock
        self._resume_s = resume_s
        self._persistent_code = persistent_code
        # Without a secret file, tokens die with the process (as before).
        self._secret = secret if secret else new_token().encode("ascii")
        self._codes: dict[str, float] = {}
        self._sessions: dict[str, _Session] = {}

    @property
    def resume_s(self) -> float:
        return self._resume_s

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

    def mint(self, viewer_id: str) -> tuple[str, float]:
        """A token for `viewer_id` valid for the resume window from now."""
        expires_at = int(self._clock() + self._resume_s)
        message = f"{viewer_id}.{expires_at}"
        signature = hmac.new(self._secret, message.encode("ascii"), hashlib.sha256).hexdigest()
        return f"{message}.{signature}", float(expires_at)

    def _verify(self, token: object) -> str | None:
        """The viewer id a token names, when its signature holds and it has not expired."""
        if not isinstance(token, str):
            return None
        parts = token.split(".")
        if len(parts) != 3:
            return None
        viewer_id, expiry, signature = parts
        message = f"{viewer_id}.{expiry}"
        expected = hmac.new(self._secret, message.encode("ascii"), hashlib.sha256).hexdigest()
        if not hmac.compare_digest(expected.encode("ascii"), signature.encode("ascii")):
            return None
        try:
            if self._clock() >= int(expiry):
                return None
        except ValueError:
            return None
        return viewer_id

    def admit_session(self, token: object) -> ViewerAdmission:
        self._prune()
        viewer_id = self._verify(token)
        if viewer_id is None:
            return ViewerAdmission(False, reason="session_expired")
        session = self._sessions.get(viewer_id)
        if session is not None:
            if session.connected:
                return ViewerAdmission(False, reason="already_attached")
            gone = session.disconnected_at
            if gone is not None and self._clock() - gone > self._resume_s:
                del self._sessions[viewer_id]
                return ViewerAdmission(False, reason="session_expired")
        self._sessions[viewer_id] = _Session(connected=True)
        fresh, expires_at = self.mint(viewer_id)
        return ViewerAdmission(
            True, viewer_id=viewer_id, session=fresh, expires_at=expires_at, resumed=True
        )

    def disconnected(self, viewer_id: str) -> None:
        session = self._sessions.get(viewer_id)
        if session is not None:
            session.connected = False
            session.disconnected_at = self._clock()

    def _start(self) -> ViewerAdmission:
        viewer_id = new_token()[:16]
        self._sessions[viewer_id] = _Session(connected=True)
        token, expires_at = self.mint(viewer_id)
        return ViewerAdmission(True, viewer_id=viewer_id, session=token, expires_at=expires_at)

    def _prune(self) -> None:
        now = self._clock()
        for code, expires_at in list(self._codes.items()):
            if now >= expires_at:
                del self._codes[code]
        for viewer_id, session in list(self._sessions.items()):
            gone = session.disconnected_at
            if not session.connected and gone is not None and now - gone > self._resume_s:
                del self._sessions[viewer_id]


@dataclass
class ViewerConnection:
    ws: ServerConnection
    viewer_id: str
    session: str
    visible: bool = True
    encoding: str = DEFAULT_ENCODING
    # The (epoch, revision) this viewer holds, as far as this relay sent it.
    held: tuple[Any, Any] | None = None
    # When this viewer's input was last seen: the latest one is focused (§4.9).
    focused_at: float = 0.0
    selection: dict[str, Any] | None = None
    last_pong: float = 0.0
    send_lock: asyncio.Lock = field(default_factory=asyncio.Lock)
    # Capture / view requests in flight, by id.
    pending: dict[str, asyncio.Future[dict[str, Any]]] = field(default_factory=dict)


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


def _pick_encoding(wanted: object) -> str:
    """The first encoding a viewer named that this relay serves; `raw/1` otherwise."""
    if isinstance(wanted, list):
        for name in wanted:
            if isinstance(name, str) and name in ENCODINGS:
                return name
    return DEFAULT_ENCODING


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
        tool_timeout: float = VIEWER_TOOL_TIMEOUT_S,
    ) -> None:
        self._host = host
        self.pairing = pairing
        self._origins = frozenset(allow_origins)
        self._ssl = ssl_context
        self._ping_interval = ping_interval
        self._pong_timeout = pong_timeout
        self._hello_timeout = hello_timeout
        self._tool_timeout = tool_timeout
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
        """`activity.viewers` (§4.9): every attached viewer, which one is focused."""
        focused = self._focused()
        return [
            {
                "viewer_id": v.viewer_id,
                "visible": v.visible,
                "focused": focused is not None and v.viewer_id == focused.viewer_id,
                "encoding": v.encoding,
            }
            for v in self._viewers.values()
        ]

    def activity(self) -> dict[str, Any]:
        """The `activity` block a snapshot or update carries: the agent's doing plus presence."""
        return {**self._host.activity(), "viewers": self.presence()}

    def _focused(self) -> ViewerConnection | None:
        """The visible viewer whose input was seen last (§4.3 `select`, §4.9)."""
        visible = [v for v in self._viewers.values() if v.visible]
        if not visible:
            return None
        return max(visible, key=lambda v: v.focused_at)

    # -- pushes ------------------------------------------------------------

    async def _send_json(self, viewer: ViewerConnection, frame: dict[str, Any]) -> None:
        try:
            async with viewer.send_lock:
                await viewer.ws.send(json.dumps(frame))
        except ConnectionClosed:
            pass

    def _stamp(self, frame: dict[str, Any]) -> dict[str, Any]:
        return {**frame, "activity": self.activity()}

    async def push(self, snapshot: dict[str, Any], update: dict[str, Any] | None) -> None:
        """Every attached viewer gets the change (§4.6 step 2): the keyed
        `update` when it holds the state the update builds on, else the
        snapshot (§4.4: any gap ⇒ snapshot)."""
        if not self._viewers:
            return
        state = (snapshot.get("epoch"), snapshot.get("revision"))
        for viewer in list(self._viewers.values()):
            base = None if update is None else (update.get("epoch"), update.get("base_revision"))
            frame = update if update is not None and viewer.held == base else snapshot
            await self._send_json(viewer, self._stamp(frame))
            viewer.held = state

    async def broadcast(self, snapshot: dict[str, Any]) -> None:
        """A whole snapshot to every viewer (the resync path)."""
        await self.push(snapshot, None)

    async def broadcast_frame(self, frame: dict[str, Any]) -> None:
        """A host frame every viewer sees as is (`rebuild`)."""
        for viewer in list(self._viewers.values()):
            await self._send_json(viewer, frame)

    async def push_activity(self, *, exclude: ViewerConnection | None = None) -> None:
        """Presence changed: an `update` carrying only `activity`, to every
        viewer that holds the current state (the others get it with their
        next snapshot); `exclude` the viewer whose own snapshot just said it."""
        snapshot = await self._host.latest_snapshot()
        if snapshot is None:
            return
        state = (snapshot.get("epoch"), snapshot.get("revision"))
        frame = {
            "type": "update",
            "protocol": VIEWER_PROTOCOL,
            "epoch": state[0],
            "base_revision": state[1],
            "revision": state[1],
            "activity": self.activity(),
        }
        for viewer in list(self._viewers.values()):
            if viewer is not exclude and viewer.held == state:
                await self._send_json(viewer, frame)

    # -- the viewer tools --------------------------------------------------

    async def serve_tool(self, tool: str, arguments: dict[str, Any]) -> dict[str, Any]:
        """`selection_get` / `viewport_view` / `viewport_capture` from the
        focused visible viewer; `ViewerUnavailable` without one."""
        viewer = self._focused()
        if viewer is None:
            raise LinkError(
                "ViewerUnavailable", NO_VIEWER_MESSAGE, {"tool": tool, "viewers": self.viewer_count}
            )
        if tool == "selection_get":
            structured = viewer.selection or {
                "selection": [],
                "selected_feature_id": None,
                "instance_path": None,
            }
            return {
                "content": [{"type": "text", "text": json.dumps(structured)}],
                "structuredContent": structured,
                "isError": False,
            }
        request_id = uuid.uuid4().hex
        if tool == "viewport_capture":
            frame = {
                "type": "capture_request",
                "id": request_id,
                "max_edge_px": arguments.get("max_edge_px", 1024),
            }
        elif tool == "viewport_view":
            frame = {
                "type": "view_request",
                "id": request_id,
                "view": arguments.get("view"),
                "fit": arguments.get("fit", True),
            }
        else:
            raise LinkError("ToolUnavailable", f"{tool} is not a viewer tool", {"tool": tool})
        future: asyncio.Future[dict[str, Any]] = asyncio.get_running_loop().create_future()
        viewer.pending[request_id] = future
        await self._send_json(viewer, frame)
        try:
            answer = await asyncio.wait_for(future, self._tool_timeout)
        except TimeoutError:
            viewer.pending.pop(request_id, None)
            raise LinkError(
                "ViewerUnavailable",
                "The viewer did not answer in time (its tab may be in the background).",
                {"tool": tool, "viewer_id": viewer.viewer_id},
            ) from None
        except ConnectionClosed:
            raise LinkError(
                "ViewerUnavailable", "The viewer disconnected while answering.", {"tool": tool}
            ) from None
        error = answer.get("error")
        if isinstance(error, dict):
            raise LinkError(
                str(error.get("code") or "ViewerUnavailable"),
                str(error.get("message") or "the viewer refused"),
                error.get("details") if isinstance(error.get("details"), dict) else {},
            )
        if tool == "viewport_capture":
            png = answer.get("png_base64")
            if not isinstance(png, str):
                raise LinkError("ViewerUnavailable", "the viewer sent no image", {"tool": tool})
            structured = {
                "mime_type": "image/png",
                "width": answer.get("width"),
                "height": answer.get("height"),
                "viewer_id": viewer.viewer_id,
            }
            return {
                "content": [{"type": "image", "data": png, "mimeType": "image/png"}],
                "structuredContent": structured,
                "isError": False,
            }
        structured = {
            "view": arguments.get("view"),
            "fitted": arguments.get("fit", True),
            "camera": answer.get("camera"),
            "viewer_id": viewer.viewer_id,
        }
        return {
            "content": [{"type": "text", "text": json.dumps(structured)}],
            "structuredContent": structured,
            "isError": False,
        }

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
        loop = asyncio.get_running_loop()
        viewer = ViewerConnection(
            ws=ws,
            viewer_id=admission.viewer_id,
            session=admission.session,
            visible=attach.get("visible") is not False,
            encoding=_pick_encoding(attach.get("encodings")),
            focused_at=loop.time(),
        )
        have = attach.get("have")
        if isinstance(have, dict):
            viewer.held = (have.get("epoch"), have.get("revision"))
        self._viewers[viewer.viewer_id] = viewer
        log.info("viewer %s %s", viewer.viewer_id, "resumed" if admission.resumed else "attached")
        try:
            welcome = {
                "type": "welcome",
                "protocol": VIEWER_PROTOCOL,
                "viewer_id": viewer.viewer_id,
                "session": viewer.session,
                "expires_at": admission.expires_at,
                "encoding": viewer.encoding,
                "epoch": self._host.epoch,
                "host_build": self._host.host_build,
            }
            await ws.send(json.dumps(welcome))
            await self._send_snapshot_unless_held(viewer)
            await self.push_activity(exclude=viewer)
            await self._serve(viewer)
        except ConnectionClosed:
            pass
        finally:
            self._viewers.pop(viewer.viewer_id, None)
            self.pairing.disconnected(viewer.viewer_id)
            for future in viewer.pending.values():
                if not future.done():
                    future.set_exception(ConnectionClosed(None, None))
            viewer.pending.clear()
            log.info("viewer %s disconnected", viewer.viewer_id)
            await self.push_activity()

    async def _send_snapshot_unless_held(self, viewer: ViewerConnection) -> None:
        snapshot = await self._host.latest_snapshot()
        if snapshot is None:
            return
        state = (snapshot.get("epoch"), snapshot.get("revision"))
        if viewer.held == state:
            # §4.6 step 5: same state — the viewer's cache is the model.
            return
        await self._send_json(viewer, self._stamp(snapshot))
        viewer.held = state

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
                    encoding = frame.get("encoding")
                    if not isinstance(encoding, str) or encoding not in ENCODINGS:
                        encoding = viewer.encoding
                    if isinstance(ids, list):
                        for mesh_id in ids:
                            if isinstance(mesh_id, str):
                                await self._send_blob(viewer, mesh_id, encoding)
                elif kind == "snapshot":
                    # An explicit resync (§4.4: any gap ⇒ snapshot).
                    snapshot = await self._host.latest_snapshot()
                    if snapshot is not None:
                        await self._send_json(viewer, self._stamp(snapshot))
                        viewer.held = (snapshot.get("epoch"), snapshot.get("revision"))
                elif kind == "select":
                    viewer.selection = {
                        "selection": frame.get("selection")
                        if isinstance(frame.get("selection"), list)
                        else [],
                        "selected_feature_id": frame.get("selected_feature_id"),
                        "instance_path": frame.get("instance_path"),
                    }
                    was_focused = self._focused()
                    viewer.focused_at = loop.time()
                    if was_focused is not viewer:
                        await self.push_activity()
                elif kind == "visible":
                    viewer.visible = frame.get("visible") is not False
                    if viewer.visible:
                        viewer.focused_at = loop.time()
                    await self.push_activity()
                elif kind in ("capture_result", "view_result"):
                    future = viewer.pending.pop(str(frame.get("id")), None)
                    if future is not None and not future.done():
                        future.set_result(frame)
                elif kind == "pong":
                    viewer.last_pong = loop.time()
                elif kind == "ping":
                    await self._send_json(viewer, {"type": "pong"})
                elif kind == "bye":
                    await viewer.ws.close(1000, "bye")
                    return
                else:
                    log.warning("ignored unknown frame type %r from viewer", kind)
        finally:
            heartbeat.cancel()

    async def _send_blob(self, viewer: ViewerConnection, mesh_id: str, encoding: str) -> None:
        found = await self._host.request_blob(mesh_id, encoding)
        if found is None:
            await self._send_json(
                viewer, {"type": "blob", "mesh_id": mesh_id, "encoding": encoding, "missing": True}
            )
            return
        header, payload = found
        try:
            async with viewer.send_lock:
                await viewer.ws.send(
                    encode_blob_frame(
                        {
                            "type": "blob",
                            "mesh_id": mesh_id,
                            "encoding": header.get("encoding", encoding),
                            "byte_length": len(payload),
                        },
                        payload,
                    )
                )
        except ConnectionClosed:
            pass

    async def _heartbeat(self, viewer: ViewerConnection) -> None:
        loop = asyncio.get_running_loop()
        try:
            while True:
                await asyncio.sleep(self._ping_interval)
                if loop.time() - viewer.last_pong > self._pong_timeout:
                    log.warning("viewer %s missed heartbeat; disconnecting", viewer.viewer_id)
                    await viewer.ws.close(1001, "heartbeat timeout")
                    return
                # A fresh token with every ping (§4.7): the resume window
                # runs from the viewer's last sign of life, restart or not.
                token, expires_at = self.pairing.mint(viewer.viewer_id)
                viewer.session = token
                async with viewer.send_lock:
                    await viewer.ws.send(json.dumps({"type": "ping"}))
                    await viewer.ws.send(
                        json.dumps({"type": "session", "session": token, "expires_at": expires_at})
                    )
        except ConnectionClosed:
            return


__all__ = [
    "DEFAULT_ENCODING",
    "ENCODINGS",
    "VIEWER_PROTOCOL",
    "VIEWER_TOOLS",
    "ViewerPairing",
    "ViewerServer",
    "decode_blob_frame",
    "encode_blob_frame",
    "snapshot_update",
]
