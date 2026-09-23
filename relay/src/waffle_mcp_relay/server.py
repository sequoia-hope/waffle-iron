"""The MCP side: a stdio server exposing the relay's own tools plus the page manifest.

`waffle_connect` and `waffle_status` are answered here. Every other tool is a
page tool and is forwarded as a `call` frame; the page's `result` frame is
returned as the MCP result. The relay holds no modeling logic (I1): it only
checks a call's `arguments` against the tool's `inputSchema` (§6.1 protocol
errors) before forwarding.
"""

from __future__ import annotations

import asyncio
import json
import logging
import webbrowser
from typing import Any
from urllib.parse import urlencode

import mcp_types as types
from jsonschema import Draft202012Validator
from jsonschema.exceptions import best_match
from mcp.server.context import ServerRequestContext
from mcp.server.lowlevel.server import NotificationOptions, Server
from mcp.server.session import ServerSession
from mcp.server.stdio import stdio_server
from mcp.server.subscriptions import InMemorySubscriptionBus, ListenHandler
from mcp.shared.exceptions import MCPError
from mcp.shared.subscriptions import ToolsListChanged
from mcp_types import INVALID_PARAMS
from pydantic import ValidationError

from waffle_mcp_relay import __version__
from waffle_mcp_relay.backend import Backend, PageBackend
from waffle_mcp_relay.config import FALLBACK_AGENT_NAME, RelayConfig, valid_agent_name
from waffle_mcp_relay.host import HostBackend, HostError
from waffle_mcp_relay.link import LinkError, LinkServer, epoch_to_iso
from waffle_mcp_relay.manifest import canonical_json, load_bundled
from waffle_mcp_relay.pairing import Pairing
from waffle_mcp_relay.viewer import ViewerPairing, ViewerServer

log = logging.getLogger("waffle_mcp_relay")

RELAY_TOOLS: list[dict[str, Any]] = [
    {
        "name": "waffle_connect",
        "description": (
            "Start pairing with a Waffle Iron browser tab. Returns a pairing link; the user "
            "opens it in the browser and clicks Allow. The link is single use and expires "
            "after 300 s (a relay started with --persistent-link returns a reusable link "
            "that does not expire). Calling this again revokes any live connection, so do "
            "not call it when waffle_status is page_away: that tab resumes by itself."
        ),
        "inputSchema": {"type": "object", "properties": {}, "additionalProperties": False},
        "outputSchema": {
            "type": "object",
            "properties": {
                "pairing_url": {"type": "string"},
                "expires_at": {
                    "type": ["string", "null"],
                    "description": "RFC 3339 UTC; null for a persistent link",
                },
            },
            "required": ["pairing_url", "expires_at"],
        },
        "annotations": {"title": "Connect to Waffle Iron", "readOnlyHint": False},
    },
    {
        "name": "waffle_status",
        "description": (
            "Connection state of the relay and the paired Waffle Iron tab. page_away: the "
            "tab disconnected (backgrounded, reloading, network) and can still resume its "
            "session without a new pairing."
        ),
        "inputSchema": {"type": "object", "properties": {}, "additionalProperties": False},
        "outputSchema": {
            "type": "object",
            "properties": {
                "state": {
                    "type": "string",
                    "enum": [
                        "unpaired",
                        "awaiting_consent",
                        "page_away",
                        "ready",
                        "paused",
                        "busy",
                    ],
                },
                "busy_reason": {"type": "string"},
                "app_build": {},
                "document_name": {"type": "string"},
            },
            "required": ["state"],
        },
        "annotations": {"title": "Waffle Iron link status", "readOnlyHint": True},
    },
]
RELAY_TOOL_NAMES = frozenset(t["name"] for t in RELAY_TOOLS)


def error_result(
    code: str, message: str, details: dict[str, Any] | None = None
) -> types.CallToolResult:
    error = {"code": code, "message": message, "details": details or {}}
    return types.CallToolResult(
        content=[types.TextContent(text=f"{code}: {message}")],
        structured_content={"error": error},
        is_error=True,
    )


def ok_result(structured: dict[str, Any], text: str | None = None) -> types.CallToolResult:
    return types.CallToolResult(
        content=[types.TextContent(text=text if text is not None else json.dumps(structured))],
        structured_content=structured,
    )


def _pointer(path: Any) -> str:
    """RFC 6901 JSON pointer for a jsonschema error path, rooted at `/arguments`."""
    parts = [str(p).replace("~", "~0").replace("/", "~1") for p in path]
    return "/arguments" + "".join(f"/{p}" for p in parts)


class ArgumentValidator:
    """Checks `tools/call` arguments against the tool's `inputSchema` (spec §6.1).

    A failure is a JSON-RPC `-32602` protocol error whose `data` is the JSON
    pointer of the offending value; the call is never forwarded. Validators are
    cached by the canonical schema text, so an adopted manifest (§2.4) that
    changes a schema gets a fresh validator.
    """

    def __init__(self) -> None:
        self._cache: dict[str, Draft202012Validator] = {}

    def check(self, tool: dict[str, Any], arguments: dict[str, Any]) -> None:
        schema = tool.get("inputSchema") or {}
        key = canonical_json(schema)
        validator = self._cache.get(key)
        if validator is None:
            validator = Draft202012Validator(schema)
            self._cache[key] = validator
        error = best_match(validator.iter_errors(arguments))
        if error is not None:
            raise MCPError(
                code=INVALID_PARAMS,
                message=f"Invalid arguments for {tool['name']}: {error.message}",
                data=_pointer(error.absolute_path),
            )


class RelayApp:
    def __init__(self, config: RelayConfig, backend: Backend) -> None:
        self._config = config
        self._backend = backend
        self._session: ServerSession | None = None
        self._client_name: str | None = None
        self._validator = ArgumentValidator()
        # 2026-07-28-era clients receive change notifications only on a
        # `subscriptions/listen` stream they open; handshake-era clients get
        # them directly on the session (see `notify_tools_changed`).
        self.subscription_bus = InMemorySubscriptionBus()
        self.server: Server[Any] = Server(
            "waffle-mcp-relay",
            version=__version__,
            on_list_tools=self._list_tools,
            on_call_tool=self._call_tool,
            on_subscriptions_listen=ListenHandler(self.subscription_bus),
        )

    # -- identity ----------------------------------------------------------

    def agent_name(self) -> str:
        if self._config.agent_name is not None:
            return self._config.agent_name
        if self._client_name is not None:
            return self._client_name
        return FALLBACK_AGENT_NAME

    def _remember(self, ctx: ServerRequestContext[Any]) -> None:
        self._session = ctx.session
        params = ctx.session.client_params
        name = params.client_info.name if params is not None else None
        if isinstance(name, str) and valid_agent_name(name):
            self._client_name = name

    async def notify_tools_changed(self) -> None:
        """`notifications/tools/list_changed` for both protocol eras (spec §2.4, P14).

        Listen streams get it from the bus. The session copy reaches
        handshake-era clients; the SDK drops it on a 2026-07-28 connection,
        where it would be an unrequested notification.
        """
        await self.subscription_bus.publish(ToolsListChanged())
        if self._session is not None:
            await self._session.send_tool_list_changed()

    # -- handlers ----------------------------------------------------------

    def _all_tools(self) -> list[dict[str, Any]]:
        # In host mode the host's `ready` frame names what it serves (§3.2);
        # a tool it does not is not listed, so an agent never learns a name
        # that would only answer HostCapability.
        served = self._backend.tool_names()
        page_tools = [
            t
            for t in self._backend.manifest.tools
            if t["name"] not in RELAY_TOOL_NAMES and (served is None or t["name"] in served)
        ]
        return RELAY_TOOLS + page_tools

    def _tool(self, name: str) -> dict[str, Any] | None:
        return next((t for t in self._all_tools() if t["name"] == name), None)

    async def _list_tools(
        self, ctx: ServerRequestContext[Any], params: types.PaginatedRequestParams | None
    ) -> types.ListToolsResult:
        self._remember(ctx)
        return types.ListToolsResult(
            tools=[types.Tool.model_validate(t, by_name=False) for t in self._all_tools()]
        )

    async def _call_tool(
        self, ctx: ServerRequestContext[Any], params: types.CallToolRequestParams
    ) -> types.CallToolResult:
        self._remember(ctx)
        name = params.name
        arguments = params.arguments or {}
        tool = self._tool(name)
        if tool is None:
            raise MCPError(code=INVALID_PARAMS, message=f"Unknown tool: {name}", data="/name")
        self._validator.check(tool, arguments)
        if name == "waffle_connect":
            return await self._connect()
        if name == "waffle_status":
            return ok_result(await self._backend.status())
        # Rebuild progress (`specs/b4_balanced_union.md` §2.3): forwarded as
        # MCP progress notifications when the client sent a progressToken.
        # Without one nobody is listening, so the page is not asked for
        # frames (it still shows them in its own status bar).
        token = getattr(ctx.meta, "progress_token", None) if ctx.meta is not None else None
        session = ctx.session

        async def on_progress(frame: dict[str, Any]) -> None:
            progress = frame.get("progress")
            total = frame.get("total")
            message = frame.get("message")
            await session.send_progress_notification(
                token,
                float(progress) if isinstance(progress, (int, float)) else 0.0,
                total=float(total) if isinstance(total, (int, float)) else None,
                message=str(message) if message is not None else None,
            )

        try:
            frame = await self._backend.call(
                name, arguments, on_progress=on_progress if token is not None else None
            )
        except LinkError as err:
            return error_result(err.code, err.message, err.details)
        try:
            return types.CallToolResult.model_validate(
                {
                    "content": frame.get("content") or [],
                    "structuredContent": frame.get("structuredContent"),
                    "isError": bool(frame.get("isError")),
                },
                by_name=False,
            )
        except ValidationError as err:
            return error_result(
                "Internal", "the page returned a malformed result frame", {"reason": str(err)}
            )

    def pairing_url(self, code: str) -> str:
        if self._backend.kernel == "host":
            # §4.7: a viewer link — the page attaches to the relay's viewer
            # socket and draws what the host computes; no engine in the browser.
            query = urlencode({"host": self._config.relay_url, "code": code})
            return f"{self._config.app_url}view?{query}"
        query = urlencode(
            {"relay": self._config.relay_url, "code": code, "name": self.agent_name()}
        )
        return f"{self._config.app_url}agent?{query}"

    async def _connect(self) -> types.CallToolResult:
        try:
            code, expires_at = await self._backend.connect()
        except LinkError as err:
            return error_result(err.code, err.message, err.details)
        url = self.pairing_url(code)
        expires = None if expires_at is None else epoch_to_iso(expires_at)
        if self._config.open_browser:
            await asyncio.to_thread(webbrowser.open, url)
        lifetime = (
            "reusable, does not expire" if expires is None else f"single use, expires {expires}"
        )
        if self._backend.kernel == "host":
            text = (
                "Ask the user to open this viewer link on any device to watch the model as it "
                f"is built — no engine runs in that browser ({lifetime}):\n{url}"
            )
            return ok_result({"pairing_url": url, "expires_at": expires, "viewer": True}, text)
        text = (
            "Ask the user to open this link in the browser where Waffle Iron runs and click "
            f"Allow ({lifetime}):\n{url}"
        )
        return ok_result({"pairing_url": url, "expires_at": expires}, text)


async def run_relay(config: RelayConfig) -> None:
    app_ref: list[RelayApp] = []
    backend: Backend
    viewers: ViewerServer | None = None
    if config.kernel == "host":
        # §3.2: the host runs the tools; there is no page link. The port is
        # the VIEWER link's (§4): browsers attach to watch what the host
        # computes, and `waffle_connect` hands out their codes.
        assert config.host_binary is not None and config.documents is not None
        host = HostBackend(
            config.host_binary,
            config.documents,
            agent_name=lambda: app_ref[0].agent_name(),
        )
        backend = host
        app = RelayApp(config, backend)
        app_ref.append(app)
        try:
            await host.start()
        except HostError as err:
            raise SystemExit(f"waffle-mcp-relay: {err}") from None
        viewers = ViewerServer(
            host=host,
            pairing=ViewerPairing(
                resume_s=config.resume_window_s,
                persistent_code=config.persistent_code,
                secret=config.viewer_secret,
            ),
            allow_origins=config.allow_origins,
            ssl_context=config.ssl_context,
        )
        await viewers.start(config.bind, config.port)
        log.info(
            "kernel host: %s (documents in %s); viewer link on %s, advertised as %s "
            "(allowed origins: %s; viewer tokens signed with %s)",
            config.host_binary,
            config.documents,
            config.listen_address,
            config.relay_url,
            ", ".join(config.allow_origins),
            config.viewer_secret_file,
        )
        if config.persistent_code is not None:
            log.info(
                "persistent viewer link (reusable; code kept in %s): %s",
                config.persistent_link_file,
                app.pairing_url(config.persistent_code),
            )
    else:
        pairing = Pairing(resume_s=config.resume_window_s, persistent_code=config.persistent_code)
        link = LinkServer(
            pairing=pairing,
            allow_origins=config.allow_origins,
            manifest=load_bundled(),
            agent_name=lambda: app_ref[0].agent_name(),
            ssl_context=config.ssl_context,
        )
        backend = PageBackend(link)
        app = RelayApp(config, backend)
        app_ref.append(app)
        link.set_on_tools_changed(app.notify_tools_changed)

        await link.start(config.bind, config.port)
        log.info(
            "listening on %s, advertised as %s (allowed origins: %s)",
            config.listen_address,
            config.relay_url,
            ", ".join(config.allow_origins),
        )
        if config.persistent_code is not None:
            log.info(
                "persistent pairing link (reusable; code kept in %s): %s",
                config.persistent_link_file,
                app.pairing_url(config.persistent_code),
            )
    try:
        async with stdio_server() as (read_stream, write_stream):
            await app.server.run(
                read_stream,
                write_stream,
                app.server.create_initialization_options(NotificationOptions(tools_changed=True)),
            )
    finally:
        if viewers is not None:
            await viewers.close()
        await backend.close()
