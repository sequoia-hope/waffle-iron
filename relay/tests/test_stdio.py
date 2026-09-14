"""The real relay process over stdio: O20 (protocol hygiene), O17 via MCP, O15 socket, O18."""

from __future__ import annotations

import asyncio
import json
import sys
from collections.abc import AsyncIterator
from pathlib import Path

import pytest
from support import (
    APP_URL,
    FakePage,
    StdioRelay,
    allocate_test_port,
    pairing_params,
    wait_until,
)

from waffle_mcp_relay.manifest import Manifest, load_bundled


@pytest.fixture
async def relay() -> AsyncIterator[tuple[StdioRelay, int]]:
    port = allocate_test_port()
    proc = await StdioRelay.start("--port", str(port), "--app-url", APP_URL)
    await proc.wait_for_stderr("listening on")
    yield proc, port
    if proc.proc.returncode is None:
        await proc.close()


def listening_addresses(port: int) -> list[str]:
    """Local addresses of LISTEN sockets on `port`, from /proc/net/tcp{,6}."""
    found: list[str] = []
    for table, width in (("tcp", 4), ("tcp6", 16)):
        path = Path("/proc/net") / table
        if not path.exists():
            continue
        for line in path.read_text().splitlines()[1:]:
            fields = line.split()
            local, state = fields[1], fields[3]
            addr_hex, port_hex = local.split(":")
            if state != "0A" or int(port_hex, 16) != port:
                continue
            raw = bytes.fromhex(addr_hex)
            if width == 4:
                found.append(
                    ".".join(str(b) for b in int.from_bytes(raw, "big").to_bytes(4, sys.byteorder))
                )
            else:
                found.append(f"ipv6:{addr_hex}")
    return found


@pytest.mark.skipif(not Path("/proc/net/tcp").exists(), reason="needs Linux /proc/net/tcp")
async def test_o15_listens_on_loopback_only(relay: tuple[StdioRelay, int]) -> None:
    proc, port = relay
    await proc.initialize()
    assert listening_addresses(port) == ["127.0.0.1"]
    assert await proc.close() == 0


async def test_o20_full_session_protocol_hygiene(relay: tuple[StdioRelay, int]) -> None:
    proc, _ = relay
    init = await proc.initialize("pytest-agent")
    assert init["result"]["serverInfo"]["name"] == "waffle-mcp-relay"
    assert init["result"]["capabilities"]["tools"]["listChanged"] is True

    listed = await proc.request("tools/list")
    names = [t["name"] for t in listed["result"]["tools"]]
    assert names[:2] == ["waffle_connect", "waffle_status"]
    assert "model_summary" in names

    assert (await proc.call_tool("waffle_status"))["structuredContent"] == {"state": "unpaired"}

    # P15: page tool while unpaired.
    unpaired = await proc.call_tool("model_summary")
    assert unpaired["isError"] is True
    assert unpaired["structuredContent"]["error"]["code"] == "NotPaired"
    assert set(unpaired["structuredContent"]["error"]) == {"code", "message", "details"}

    connect = await proc.call_tool("waffle_connect")
    url = connect["structuredContent"]["pairing_url"]
    assert url.startswith(f"{APP_URL}agent?")
    params = pairing_params(url)
    assert params["name"] == "pytest-agent"
    assert (await proc.call_tool("waffle_status"))["structuredContent"] == {
        "state": "awaiting_consent"
    }

    page = await FakePage.connect(params["relay"])
    await page.hello(
        code=params["code"], manifest_hash=load_bundled().hash, app_build={"commit": "t"}
    )
    welcome = await page.recv_type("welcome")
    assert welcome["agent_name"] == "pytest-agent"
    await page.send({"type": "status", "state": "ready", "document_name": "Doc A"})
    status = {}
    for _ in range(50):
        status = (await proc.call_tool("waffle_status"))["structuredContent"]
        if status.get("document_name") == "Doc A":
            break
        await asyncio.sleep(0.05)
    assert status == {"state": "ready", "app_build": {"commit": "t"}, "document_name": "Doc A"}

    # A forwarded page call returns the page's result frame as the MCP result.
    summary = {
        "document_name": "Doc A",
        "features": [],
        "rollback_index": None,
        "bodies": [],
        "errors": [],
        "warnings": [],
        "parameters": [],
    }
    pending = asyncio.create_task(proc.call_tool("model_summary"))
    call = await page.recv_type("call")
    assert call["tool"] == "model_summary" and call["arguments"] == {} and call["progress"] is False
    await page.send(
        {
            "type": "result",
            "id": call["id"],
            "isError": False,
            "content": [{"type": "text", "text": json.dumps(summary)}],
            "structuredContent": summary,
        }
    )
    result = await pending
    assert result["structuredContent"] == summary
    assert result["isError"] is False

    # JSON-RPC protocol errors.
    unknown = await proc.request("no/such/method")
    assert unknown["error"]["code"] == -32601
    malformed = await proc.request("tools/call", {})
    assert malformed["error"]["code"] == -32602
    malformed2 = await proc.request("tools/call", {"name": 7})
    assert malformed2["error"]["code"] == -32602
    unknown_tool = await proc.request("tools/call", {"name": "no_such_tool", "arguments": {}})
    assert unknown_tool["error"]["code"] == -32602
    # §6.1: arguments failing the tool's inputSchema never reach the page.
    frames_before = len(page.frames)
    bad_args = await proc.request("tools/call", {"name": "model_summary", "arguments": {"x": 1}})
    assert bad_args["error"]["code"] == -32602
    assert bad_args["error"]["data"] == "/arguments"
    bad_relay_args = await proc.request(
        "tools/call", {"name": "waffle_status", "arguments": {"verbose": True}}
    )
    assert bad_relay_args["error"]["code"] == -32602
    assert len(page.frames) == frames_before

    # O17 end to end: the page drops during a forwarded call.
    pending = asyncio.create_task(proc.call_tool("model_summary"))
    await page.recv_type("call")
    await page.close()
    dropped = await pending
    assert dropped["isError"] is True
    assert dropped["structuredContent"]["error"]["code"] == "PageDisconnected"
    assert dropped["structuredContent"]["error"]["details"] == {"state_unknown": True}

    assert await proc.close() == 0
    assert proc.stdout_lines, "no stdout captured"
    assert proc.unparseable == []
    for line in proc.stdout_lines:
        assert json.loads(line)["jsonrpc"] == "2.0"
    assert "listening on" in proc.stderr.decode()


async def test_o18_list_changed_over_stdio(relay: tuple[StdioRelay, int]) -> None:
    proc, _ = relay
    await proc.initialize()
    connect = await proc.call_tool("waffle_connect")
    params = pairing_params(connect["structuredContent"]["pairing_url"])
    page_tools = [
        *load_bundled().tools,
        {
            "name": "page_only_tool",
            "description": "added by a newer page",
            "inputSchema": {"type": "object", "properties": {}},
        },
    ]
    page = await FakePage.connect(params["relay"])
    await page.hello(code=params["code"], manifest_hash=Manifest.from_tools(page_tools).hash)
    assert (await page.recv_type("welcome"))["manifest_required"] is True
    await page.send({"type": "manifest", "tools": page_tools})

    def changed() -> list[dict[str, object]]:
        return [
            n for n in proc.notifications if n.get("method") == "notifications/tools/list_changed"
        ]

    await wait_until(lambda: len(changed()) >= 1, timeout=10)
    listed = await proc.request("tools/list")
    assert "page_only_tool" in [t["name"] for t in listed["result"]["tools"]]
    assert len(changed()) == 1
    await page.close()
    assert await proc.close() == 0
