"""The MCP side: a stdio server exposing the relay's own tools plus the page manifest.

`waffle_connect` and `waffle_status` are answered here. Every other tool is a
page tool and is forwarded as a `call` frame; the page's `result` frame is
returned as the MCP result. The relay holds no modeling logic (I1).
"""

from __future__ import annotations

import asyncio
import json
import logging
import webbrowser
from typing import Any
from urllib.parse import urlencode

import mcp_types as types
from mcp.server.context import ServerRequestContext
from mcp.server.lowlevel.server import NotificationOptions, Server
from mcp.server.session import ServerSession
from mcp.server.stdio import stdio_server
from mcp.shared.exceptions import MCPError
from mcp_types import INVALID_PARAMS
from pydantic import ValidationError

from waffle_mcp_relay import __version__
from waffle_mcp_relay.config import FALLBACK_AGENT_NAME, RelayConfig, valid_agent_name
from waffle_mcp_relay.link import LinkError, LinkServer, epoch_to_iso
from waffle_mcp_relay.manifest import load_bundled
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


class RelayApp:
    def __init__(self, config: RelayConfig, link: LinkServer) -> None:
        self._config = config
        self._link = link
        self._session: ServerSession | None = None
        self._client_name: str | None = None
        self.server: Server[Any] = Server(
            "waffle-mcp-relay",
            version=__version__,
            on_list_tools=self._list_tools,
            on_call_tool=self._call_tool,
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
        if self._session is not None:
            await self._session.send_tool_list_changed()

    # -- handlers ----------------------------------------------------------

    def _all_tools(self) -> list[dict[str, Any]]:
        page_tools = [t for t in self._link.manifest.tools if t["name"] not in RELAY_TOOL_NAMES]
        return RELAY_TOOLS + page_tools

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
        if name == "waffle_connect":
            return await self._connect()
        if name == "waffle_status":
            return ok_result(self._link.status())
        if name not in self._link.manifest.names:
            raise MCPError(code=INVALID_PARAMS, message=f"Unknown tool: {name}", data="/name")
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
        "listening on %s (allowed origins: %s)", config.relay_url, ", ".join(config.allow_origins)
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
