"""The WebSocket side: at most one paired page speaking `waffle-agent-link/1` (spec §2.3, §3.1).

Admission (I8) happens in two gates: the handshake `Origin` must be in the
allow list (else HTTP 403 before any frame is read, P4), and the first frame
must be a `hello` carrying a valid single-use code or a resumable session
token. Only the admitted connection is ever sent a `call` frame.
"""

from __future__ import annotations

import asyncio
import json
import logging
import ssl
import time
import uuid
from collections.abc import Awaitable, Callable, Iterable
from dataclasses import dataclass, field
from http import HTTPStatus
from typing import Any

from websockets.asyncio.server import Server, ServerConnection, serve
from websockets.datastructures import MultipleValuesError
from websockets.exceptions import ConnectionClosed
from websockets.http11 import Request, Response

from waffle_mcp_relay import PROTOCOL
from waffle_mcp_relay.manifest import Manifest, ManifestError, manifest_hash
from waffle_mcp_relay.pairing import Pairing

log = logging.getLogger("waffle_mcp_relay.link")

PING_INTERVAL_S = 15.0
PONG_TIMEOUT_S = 30.0
HELLO_TIMEOUT_S = 10.0
MAX_FRAME_BYTES = 32 * 1024 * 1024

NOT_PAIRED_MESSAGE = (
    "No Waffle Iron page is paired. Call waffle_connect and open the returned "
    "pairing link in the browser tab, then click Allow."
)
PAGE_DISCONNECTED_MESSAGE = (
    "The page disconnected while the call was in flight. The model state after "
    "the call is unknown; call model_summary before continuing."
)


class LinkError(Exception):
    """A refusal that becomes an `isError` tool result with `{code, message, details}`."""

    def __init__(self, code: str, message: str, details: dict[str, Any] | None = None):
        super().__init__(f"{code}: {message}")
        self.code = code
        self.message = message
        self.details = details or {}


@dataclass
class PageConnection:
    ws: ServerConnection
    session: str
    hello_manifest_hash: str | None
    app_build: Any = None
    state: str = "ready"
    busy_reason: str | None = None
    document_name: str | None = None
    revoked: bool = False
    last_pong: float = 0.0
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


class LinkServer:
    def __init__(
        self,
        *,
        pairing: Pairing,
        allow_origins: Iterable[str],
        manifest: Manifest,
        agent_name: Callable[[], str],
        on_tools_changed: Callable[[], Awaitable[None]] | None = None,
        ssl_context: ssl.SSLContext | None = None,
        ping_interval: float = PING_INTERVAL_S,
        pong_timeout: float = PONG_TIMEOUT_S,
        hello_timeout: float = HELLO_TIMEOUT_S,
    ) -> None:
        self._pairing = pairing
        self._origins = frozenset(allow_origins)
        self._manifest = manifest
        self._agent_name = agent_name
        self._on_tools_changed = on_tools_changed
        self._ssl = ssl_context
        self._ping_interval = ping_interval
        self._pong_timeout = pong_timeout
        self._hello_timeout = hello_timeout
        self._server: Server | None = None
        self._page: PageConnection | None = None

    # -- lifecycle ---------------------------------------------------------

    async def start(self, host: str, port: int) -> None:
        self._server = await serve(
            self._handle,
            host,
            port,
            process_request=self._check_origin,
            ssl=self._ssl,
            ping_interval=None,  # the link protocol has its own ping/pong frames
            max_size=MAX_FRAME_BYTES,
        )

    async def close(self) -> None:
        if self._server is not None:
            self._server.close()
            await self._server.wait_closed()

    def set_on_tools_changed(self, callback: Callable[[], Awaitable[None]]) -> None:
        self._on_tools_changed = callback

    # -- queries -----------------------------------------------------------

    @property
    def manifest(self) -> Manifest:
        return self._manifest

    def status(self) -> dict[str, Any]:
        paired = self._pairing.state()
        page = self._page
        if paired != "paired" or page is None:
            return {"state": paired}
        out: dict[str, Any] = {"state": page.state}
        if page.state == "busy" and page.busy_reason:
            out["busy_reason"] = page.busy_reason
        if page.app_build is not None:
            out["app_build"] = page.app_build
        if page.document_name is not None:
            out["document_name"] = page.document_name
        return out

    # -- commands ----------------------------------------------------------

    async def new_pairing(self) -> tuple[str, float]:
        """Issue a fresh code; a live page is sent `bye{revoked}` and closed (P11)."""
        old = self._page
        code, expires_at = self._pairing.issue_code()
        if old is not None:
            old.revoked = True
            self._page = None
            await self._bye(old.ws, "revoked")
        return code, expires_at

    async def call(self, tool: str, arguments: dict[str, Any]) -> dict[str, Any]:
        """Forward a page tool call; returns the page's `result` frame."""
        page = self._page
        if page is None:
            raise LinkError("NotPaired", NOT_PAIRED_MESSAGE, {"hint": "call waffle_connect"})
        call_id = uuid.uuid4().hex
        future: asyncio.Future[dict[str, Any]] = asyncio.get_running_loop().create_future()
        page.pending[call_id] = future
        frame = {
            "type": "call",
            "id": call_id,
            "tool": tool,
            "arguments": arguments,
            "progress": False,
        }
        try:
            await page.ws.send(json.dumps(frame))
        except ConnectionClosed:
            page.pending.pop(call_id, None)
            raise LinkError(
                "PageDisconnected", PAGE_DISCONNECTED_MESSAGE, {"state_unknown": True}
            ) from None
        try:
            return await future
        except asyncio.CancelledError:
            page.pending.pop(call_id, None)
            try:
                await page.ws.send(json.dumps({"type": "cancel", "id": call_id}))
            except ConnectionClosed:
                pass
            raise

    # -- handshake ---------------------------------------------------------

    def _check_origin(self, connection: ServerConnection, request: Request) -> Response | None:
        try:
            origin = request.headers.get("Origin")
        except MultipleValuesError:
            origin = None
        if origin is not None and origin in self._origins:
            return None
        log.warning("refused WebSocket handshake: Origin %r not allowed", origin)
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
            await ws.close(1008, "expected hello")
            return
        hello = _parse(raw)
        if hello is None or hello["type"] != "hello":
            await ws.close(1008, "expected hello")
            return
        if hello.get("protocol") != PROTOCOL:
            await self._bye(ws, "protocol_mismatch", supported=[PROTOCOL])
            return

        if "code" in hello:
            admission = self._pairing.admit_code(hello.get("code"))
        elif "session" in hello:
            admission = self._pairing.admit_session(hello.get("session"))
        else:
            admission = None
        if admission is None or not admission.ok:
            reason = admission.reason if admission is not None else "invalid_code"
            log.warning("refused page: %s", reason)
            await self._bye(ws, reason or "invalid_code")
            return

        assert admission.session is not None
        page_hash = hello.get("manifest_hash")
        page = PageConnection(
            ws=ws,
            session=admission.session,
            hello_manifest_hash=page_hash if isinstance(page_hash, str) else None,
            app_build=hello.get("app_build"),
        )
        self._page = page
        welcome: dict[str, Any] = {
            "type": "welcome",
            "session": admission.session,
            "agent_name": self._agent_name(),
            "protocol": PROTOCOL,
        }
        if page.hello_manifest_hash is not None and page.hello_manifest_hash != self._manifest.hash:
            welcome["manifest_required"] = True
        log.info("page %s", "resumed" if admission.resumed else "paired")
        try:
            await ws.send(json.dumps(welcome))
            await self._serve_page(page)
        except ConnectionClosed:
            pass
        finally:
            if self._page is page:
                self._page = None
            self._pairing.page_disconnected(page.session, revoke=page.revoked)
            for future in page.pending.values():
                if not future.done():
                    future.set_exception(
                        LinkError(
                            "PageDisconnected", PAGE_DISCONNECTED_MESSAGE, {"state_unknown": True}
                        )
                    )
            page.pending.clear()
            log.info("page disconnected")

    # -- paired page -------------------------------------------------------

    async def _serve_page(self, page: PageConnection) -> None:
        loop = asyncio.get_running_loop()
        page.last_pong = loop.time()
        heartbeat = asyncio.create_task(self._heartbeat(page))
        try:
            async for raw in page.ws:
                frame = _parse(raw)
                if frame is None:
                    log.warning("ignored malformed frame from page")
                    continue
                kind = frame["type"]
                if kind == "result":
                    future = page.pending.pop(str(frame.get("id")), None)
                    if future is not None and not future.done():
                        future.set_result(frame)
                elif kind == "pong":
                    page.last_pong = loop.time()
                elif kind == "ping":
                    await page.ws.send(json.dumps({"type": "pong"}))
                elif kind == "status":
                    self._apply_status(page, frame)
                elif kind == "manifest":
                    await self._adopt_manifest(page, frame)
                elif kind == "progress":
                    pass  # Phase 0 has no long-running page tools.
                elif kind == "bye":
                    reason = frame.get("reason")
                    if reason == "user_disconnected":
                        page.revoked = True  # P12: no resume
                    log.info("page said bye: %s", reason)
                    await page.ws.close(1000, "bye")
                    return
                else:
                    log.warning("ignored unknown frame type %r from page", kind)
        finally:
            heartbeat.cancel()

    async def _heartbeat(self, page: PageConnection) -> None:
        loop = asyncio.get_running_loop()
        try:
            while True:
                await asyncio.sleep(self._ping_interval)
                if loop.time() - page.last_pong > self._pong_timeout:
                    log.warning("page missed heartbeat; disconnecting")
                    await page.ws.close(1001, "heartbeat timeout")
                    return
                await page.ws.send(json.dumps({"type": "ping"}))
        except ConnectionClosed:
            return

    @staticmethod
    def _apply_status(page: PageConnection, frame: dict[str, Any]) -> None:
        state = frame.get("state")
        if isinstance(state, dict) and state.get("busy") is not None:
            page.state, page.busy_reason = "busy", str(state.get("busy", {}).get("reason", ""))
        elif state in ("ready", "paused", "busy"):
            page.state = state
            reason = frame.get("reason")
            page.busy_reason = str(reason) if state == "busy" and reason else None
        name = frame.get("document_name")
        if isinstance(name, str):
            page.document_name = name

    async def _adopt_manifest(self, page: PageConnection, frame: dict[str, Any]) -> None:
        tools = frame.get("tools")
        try:
            adopted = Manifest.from_tools(tools)
        except ManifestError as err:
            log.warning("ignored invalid manifest from page: %s", err)
            return
        if (
            page.hello_manifest_hash is None
            or manifest_hash(adopted.tools) != page.hello_manifest_hash
        ):
            log.warning("ignored page manifest: its hash does not match the hello manifest_hash")
            return
        if adopted.hash == self._manifest.hash:
            return
        self._manifest = adopted
        log.info("adopted the page's tool manifest (%d tools)", len(adopted.tools))
        if self._on_tools_changed is not None:
            await self._on_tools_changed()


def epoch_to_iso(seconds: float) -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(seconds))
