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
from waffle_mcp_relay.config import FALLBACK_AGENT_NAME, RelayConfig, valid_agent_name
from waffle_mcp_relay.link import LinkError, LinkServer, epoch_to_iso
from waffle_mcp_relay.manifest import canonical_json, load_bundled
from waffle_mcp_relay.pairing import Pairing

log = logging.getLogger("waffle_mcp_relay")

RELAY_TOOLS: list[dict[str, Any]] = [
    {
        "name": "waffle_connect",
        "description": (
            "Start pairing with a Waffle Iron browser tab. Returns a pairing link; the user "
            "opens it in the browser and clicks Allow. The link is single use and expires "
            "after 300 s. Calling this again revokes any live connection."
        ),
        "inputSchema": {"type": "object", "properties": {}, "additionalProperties": False},
        "outputSchema": {
            "type": "object",
            "properties": {
                "pairing_url": {"type": "string"},
                "expires_at": {"type": "string", "description": "RFC 3339 UTC"},
            },
            "required": ["pairing_url", "expires_at"],
        },
        "annotations": {"title": "Connect to Waffle Iron", "readOnlyHint": False},
    },
    {
        "name": "waffle_status",
        "description": "Connection state of the relay and the paired Waffle Iron tab.",
        "inputSchema": {"type": "object", "properties": {}, "additionalProperties": False},
        "outputSchema": {
            "type": "object",
            "properties": {
                "state": {
                    "type": "string",
                    "enum": ["unpaired", "awaiting_consent", "ready", "paused", "busy"],
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
    def __init__(self, config: RelayConfig, link: LinkServer) -> None:
        self._config = config
        self._link = link
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
        page_tools = [t for t in self._link.manifest.tools if t["name"] not in RELAY_TOOL_NAMES]
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
            return ok_result(self._link.status())
        try:
            frame = await self._link.call(name, arguments)
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

    async def _connect(self) -> types.CallToolResult:
        code, expires_at = await self._link.new_pairing()
        query = urlencode(
            {"relay": self._config.relay_url, "code": code, "name": self.agent_name()}
        )
        url = f"{self._config.app_url}agent?{query}"
        expires = epoch_to_iso(expires_at)
        if self._config.open_browser:
            await asyncio.to_thread(webbrowser.open, url)
        text = (
            "Ask the user to open this link in the browser where Waffle Iron runs and click "
            f"Allow (single use, expires {expires}):\n{url}"
        )
        return ok_result({"pairing_url": url, "expires_at": expires}, text)


async def run_relay(config: RelayConfig) -> None:
    pairing = Pairing()
    app_ref: list[RelayApp] = []
    link = LinkServer(
        pairing=pairing,
        allow_origins=config.allow_origins,
        manifest=load_bundled(),
        agent_name=lambda: app_ref[0].agent_name(),
        ssl_context=config.ssl_context,
    )
    app = RelayApp(config, link)
    app_ref.append(app)
    link.set_on_tools_changed(app.notify_tools_changed)

    await link.start(config.bind, config.port)
    log.info(
        "listening on %s, advertised as %s (allowed origins: %s)",
        config.listen_address,
        config.relay_url,
        ", ".join(config.allow_origins),
    )
    try:
        async with stdio_server() as (read_stream, write_stream):
            await app.server.run(
                read_stream,
                write_stream,
                app.server.create_initialization_options(NotificationOptions(tools_changed=True)),
            )
    finally:
        await link.close()
