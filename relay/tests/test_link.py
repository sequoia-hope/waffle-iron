"""§3.1 relay/pairing branches over a real WebSocket server: O13, O17, O18, heartbeat."""

from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator
from dataclasses import dataclass

import pytest
from support import APP_ORIGIN, FakeClock, FakePage, allocate_test_port, wait_until
from websockets.exceptions import ConnectionClosed, InvalidStatus

from waffle_mcp_relay import PROTOCOL
from waffle_mcp_relay.link import LinkError, LinkServer
from waffle_mcp_relay.manifest import Manifest, load_bundled
from waffle_mcp_relay.pairing import CODE_TTL_S, SESSION_RESUME_S, Pairing


@dataclass
class Env:
    clock: FakeClock
    pairing: Pairing
    link: LinkServer
    url: str
    tools_changed: list[int]


async def make_env(**link_kwargs: float) -> Env:
    clock = FakeClock()
    pairing = Pairing(clock)
    changed: list[int] = []

    async def on_changed() -> None:
        changed.append(1)

    link = LinkServer(
        pairing=pairing,
        allow_origins=[APP_ORIGIN],
        manifest=load_bundled(),
        agent_name=lambda: "test-agent",
        on_tools_changed=on_changed,
        hello_timeout=link_kwargs.pop("hello_timeout", 1.0),
        **link_kwargs,
    )
    port = allocate_test_port()
    await link.start("127.0.0.1", port)
    return Env(clock, pairing, link, f"ws://127.0.0.1:{port}", changed)


@pytest.fixture
async def env() -> AsyncIterator[Env]:
    e = await make_env()
    yield e
    await e.link.close()


async def pair(env: Env) -> tuple[FakePage, dict[str, object]]:
    code, _ = await env.link.new_pairing()
    page = await FakePage.connect(env.url)
    await page.hello(code=code, manifest_hash=env.link.manifest.hash)
    welcome = await page.recv_type("welcome")
    return page, welcome


async def expect_bye(page: FakePage, reason: str) -> dict[str, object]:
    frame = await page.recv_type("bye")
    assert frame["reason"] == reason
    with pytest.raises(ConnectionClosed):
        await page.recv()
    return frame


# -- O13: Origin gate (P4) ----------------------------------------------------


async def test_o13_bad_origin_gets_403(env: Env) -> None:
    with pytest.raises(InvalidStatus) as err:
        await FakePage.connect(env.url, origin="https://evil.example")
    assert err.value.response.status_code == 403


async def test_o13_missing_origin_gets_403(env: Env) -> None:
    with pytest.raises(InvalidStatus) as err:
        await FakePage.connect(env.url, origin=None)
    assert err.value.response.status_code == 403


# -- O13: codes and sessions (P2, P5, P6, P8, P11-P13) -------------------------


async def test_valid_code_pairs(env: Env) -> None:
    page, welcome = await pair(env)
    assert welcome["protocol"] == PROTOCOL
    assert welcome["agent_name"] == "test-agent"
    assert isinstance(welcome["session"], str) and len(welcome["session"]) == 43
    assert "manifest_required" not in welcome
    assert env.link.status() == {"state": "ready"}
    await page.close()


async def test_o13_reused_code_refused(env: Env) -> None:
    code, _ = await env.link.new_pairing()
    first = await FakePage.connect(env.url)
    await first.hello(code=code)
    await first.recv_type("welcome")
    await first.close()
    await wait_until(lambda: not env.pairing.page_connected)
    second = await FakePage.connect(env.url)
    await second.hello(code=code)
    await expect_bye(second, "invalid_code")
    assert [f["type"] for f in second.frames] == ["bye"]


async def test_o13_expired_code_refused(env: Env) -> None:
    code, _ = await env.link.new_pairing()
    env.clock.advance(CODE_TTL_S)
    page = await FakePage.connect(env.url)
    await page.hello(code=code)
    await expect_bye(page, "invalid_code")
    assert env.link.status() == {"state": "unpaired"}


async def test_o13_second_page_refused_and_never_sees_calls(env: Env) -> None:
    page, _ = await pair(env)
    intruder = await FakePage.connect(env.url)
    await intruder.hello(code="guessed-code")
    await expect_bye(intruder, "already_paired")

    call = asyncio.create_task(env.link.call("model_summary", {}))
    frame = await page.recv_type("call")
    await page.send(
        {
            "type": "result",
            "id": frame["id"],
            "content": [],
            "isError": False,
            "structuredContent": {"ok": True},
        }
    )
    assert (await call)["structuredContent"] == {"ok": True}
    assert [f["type"] for f in intruder.frames] == ["bye"]
    await page.close()


async def test_o13_silent_socket_gets_nothing(env: Env) -> None:
    lurker = await FakePage.connect(env.url)
    with pytest.raises(ConnectionClosed):
        await lurker.recv(timeout=5)
    assert lurker.frames == []


async def test_o13_session_from_other_connection_after_expiry(env: Env) -> None:
    page, welcome = await pair(env)
    await page.close()
    await wait_until(lambda: not env.pairing.page_connected)
    env.clock.advance(SESSION_RESUME_S + 1)
    other = await FakePage.connect(env.url)
    await other.hello(session=welcome["session"])
    await expect_bye(other, "session_expired")


async def test_session_resumes_within_window(env: Env) -> None:
    page, welcome = await pair(env)
    await page.close()
    await wait_until(lambda: not env.pairing.page_connected)
    env.clock.advance(SESSION_RESUME_S - 1)
    again = await FakePage.connect(env.url)
    await again.hello(session=welcome["session"])
    resumed = await again.recv_type("welcome")
    assert resumed["session"] == welcome["session"]
    await again.close()


async def test_user_disconnected_bye_revokes_session(env: Env) -> None:
    page, welcome = await pair(env)
    await page.send({"type": "bye", "reason": "user_disconnected"})
    await wait_until(lambda: not env.pairing.page_connected)
    again = await FakePage.connect(env.url)
    await again.hello(session=welcome["session"])
    await expect_bye(again, "session_expired")
    assert env.link.status() == {"state": "unpaired"}


async def test_new_pairing_revokes_live_page(env: Env) -> None:
    page, _ = await pair(env)
    await env.link.new_pairing()
    await expect_bye(page, "revoked")
    assert env.link.status() == {"state": "awaiting_consent"}


async def test_protocol_mismatch(env: Env) -> None:
    code, _ = await env.link.new_pairing()
    page = await FakePage.connect(env.url)
    await page.hello(code=code, protocol="waffle-agent-link/99")
    frame = await expect_bye(page, "protocol_mismatch")
    assert frame["supported"] == [PROTOCOL]


# -- status frames -------------------------------------------------------------


async def test_status_frames_reach_waffle_status(env: Env) -> None:
    page, _ = await pair(env)
    await page.send(
        {"type": "status", "state": "busy", "reason": "sketch_mode", "document_name": "Bracket"}
    )
    await wait_until(lambda: env.link.status().get("state") == "busy")
    assert env.link.status() == {
        "state": "busy",
        "busy_reason": "sketch_mode",
        "document_name": "Bracket",
    }
    await page.close()


# -- O17: page drops mid-call (P9) -----------------------------------------------


async def test_o17_page_drop_mid_call_is_page_disconnected(env: Env) -> None:
    page, _ = await pair(env)
    call = asyncio.create_task(env.link.call("model_summary", {}))
    await page.recv_type("call")
    await page.close()
    with pytest.raises(LinkError) as err:
        await call
    assert err.value.code == "PageDisconnected"
    assert err.value.details == {"state_unknown": True}


async def test_a18_cancelled_call_sends_cancel_frame_with_its_id(env: Env) -> None:
    # A18: the MCP request is cancelled (the SDK cancels the handler task); the
    # page is told which call, so it can undo the step once it completes.
    page, _ = await pair(env)
    call = asyncio.create_task(env.link.call("feature_add", {"operation": {}}))
    sent = await page.recv_type("call")
    call.cancel()
    with pytest.raises(asyncio.CancelledError):
        await call
    cancel = await page.recv_type("cancel")
    assert cancel == {"type": "cancel", "id": sent["id"]}
    # A late result for the cancelled call is ignored, and the link stays usable.
    await page.send({"type": "result", "id": sent["id"], "content": [], "isError": False})
    later = asyncio.create_task(env.link.call("model_summary", {}))
    second = await page.recv_type("call")
    await page.send({"type": "result", "id": second["id"], "content": [], "isError": False})
    assert (await later)["id"] == second["id"]
    await page.close()


async def test_paused_status_reaches_waffle_status(env: Env) -> None:
    page, _ = await pair(env)
    await page.send({"type": "status", "state": "paused", "document_name": "Bracket"})
    await wait_until(lambda: env.link.status().get("state") == "paused")
    assert env.link.status() == {"state": "paused", "document_name": "Bracket"}
    await page.send({"type": "status", "state": "ready", "document_name": "Bracket"})
    await wait_until(lambda: env.link.status().get("state") == "ready")
    await page.close()


async def test_call_unpaired_is_not_paired(env: Env) -> None:
    with pytest.raises(LinkError) as err:
        await env.link.call("model_summary", {})
    assert err.value.code == "NotPaired"


# -- O18: manifest adoption (P14) ------------------------------------------------


async def test_o18_manifest_mismatch_adopts_page_manifest(env: Env) -> None:
    page_tools = [
        *env.link.manifest.tools,
        {
            "name": "page_only_tool",
            "description": "x",
            "inputSchema": {"type": "object", "properties": {}},
        },
    ]
    page_manifest = Manifest.from_tools(page_tools)
    code, _ = await env.link.new_pairing()
    page = await FakePage.connect(env.url)
    await page.hello(code=code, manifest_hash=page_manifest.hash)
    welcome = await page.recv_type("welcome")
    assert welcome["manifest_required"] is True
    await page.send({"type": "manifest", "tools": page_tools})
    await wait_until(lambda: env.tools_changed == [1])
    assert "page_only_tool" in env.link.manifest.names
    await page.close()


async def test_manifest_not_matching_hello_hash_is_ignored(env: Env) -> None:
    code, _ = await env.link.new_pairing()
    page = await FakePage.connect(env.url)
    await page.hello(code=code, manifest_hash="0" * 64)
    await page.recv_type("welcome")
    await page.send({"type": "manifest", "tools": [{"name": "sneaky", "inputSchema": {}}]})
    await page.send({"type": "status", "state": "ready", "document_name": "sync"})
    await wait_until(lambda: env.link.status().get("document_name") == "sync")
    assert env.tools_changed == []
    assert "sneaky" not in env.link.manifest.names
    await page.close()


# -- heartbeat (§2.2) -----------------------------------------------------------


async def test_heartbeat_disconnects_silent_page() -> None:
    e = await make_env(ping_interval=0.05, pong_timeout=0.2)
    try:
        code, _ = await e.link.new_pairing()
        page = await FakePage.connect(e.url)
        await page.hello(code=code)
        await page.recv()  # welcome; afterwards never answer pings
        await wait_until(lambda: not e.pairing.page_connected, timeout=5)
    finally:
        await e.link.close()


async def test_heartbeat_keeps_answering_page() -> None:
    e = await make_env(ping_interval=0.05, pong_timeout=0.2)
    try:
        code, _ = await e.link.new_pairing()
        page = await FakePage.connect(e.url)
        await page.hello(code=code)
        await page.recv_type("welcome")
        for _ in range(12):  # ~0.6 s, three pong timeouts' worth
            ping = await page.recv_type("ping")
            assert ping == {"type": "ping"}
            await page.send({"type": "pong"})
        assert e.pairing.page_connected
        await page.close()
    finally:
        await e.link.close()
