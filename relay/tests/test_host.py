"""`--kernel host` (specs/waffle_server_mode.md §3.2, §3.4, §4.8; oracle H4).

Two layers: `HostBackend` driven directly against `fake_host.py` (frames,
progress, crash → `EngineCrashed` → restart → reopen), and the real relay
process over MCP stdio in host mode (tool listing filtered to what the host
serves, `waffle_status` / `waffle_connect` in host mode). A last test runs
the REAL `waffle-host` binary through MCP when one is built
(`$WAFFLE_HOST_BIN` or `target/release/waffle-host`), skipped otherwise.
"""

from __future__ import annotations

import asyncio
import json
import os
import shutil
from collections.abc import AsyncIterator
from pathlib import Path
from typing import Any

import pytest
from support import APP_URL, REPO_ROOT, StdioRelay, allocate_test_port, relay_env, wait_until

from waffle_mcp_relay.config import ConfigError, build_config, default_documents_dir
from waffle_mcp_relay.host import HOST_PROTOCOL, HostBackend, HostError, find_host_binary
from waffle_mcp_relay.link import LinkError

FAKE_HOST = Path(__file__).with_name("fake_host.py")


def real_host_binary() -> Path | None:
    explicit = os.environ.get("WAFFLE_HOST_BIN")
    if explicit:
        return Path(explicit)
    built = REPO_ROOT / "target" / "release" / "waffle-host"
    return built if built.is_file() else None


# -- HostBackend against the fake host ---------------------------------------


@pytest.fixture
async def backend(tmp_path: Path) -> AsyncIterator[HostBackend]:
    b = HostBackend(FAKE_HOST, tmp_path / "docs", agent_name=lambda: "test-agent")
    await b.start()
    yield b
    await b.close()


async def test_ready_names_the_tools_and_status_says_host(
    backend: HostBackend, tmp_path: Path
) -> None:
    assert backend.tool_names() == frozenset(
        {
            "model_summary",
            "feature_add",
            "document_info",
            "document_open",
            "document_new",
            "document_import",
            "storage_list",
        }
    )
    assert backend.host_build == {"version": "fake"}
    assert (tmp_path / "docs").is_dir(), "the documents directory is created before the host starts"
    status = await backend.status()
    assert status["state"] == "ready"
    assert status["kernel"] == "host"
    assert status["host_build"] == {"version": "fake"}
    assert "document_name" not in status


async def test_a_call_is_forwarded_with_the_agent_name(backend: HostBackend) -> None:
    result = await backend.call("model_summary", {"x": 1})
    assert result["isError"] is False
    assert result["structuredContent"]["echo"] == {
        "name": "model_summary",
        "arguments": {"x": 1},
        "agent_name": "test-agent",
    }
    assert set(result) == {"content", "structuredContent", "isError"}


async def test_progress_frames_reach_the_listener_only_when_asked(backend: HostBackend) -> None:
    seen: list[dict[str, Any]] = []

    async def on_progress(frame: dict[str, Any]) -> None:
        seen.append(frame)

    result = await backend.call("feature_add", {}, on_progress=on_progress)
    assert result["structuredContent"] == {"done": True}
    assert [f["message"] for f in seen] == ["step 1", "step 2", "step 3"]
    assert seen[-1]["progress"] == 3 and seen[-1]["total"] == 3

    seen.clear()
    await backend.call("feature_add", {})
    assert seen == [], "no listener, no progress frames requested"


async def test_h4_crash_fails_the_call_restarts_and_reopens(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    state_file = tmp_path / "state.json"
    monkeypatch.setenv("FAKE_HOST_STATE_FILE", str(state_file))
    backend = HostBackend(FAKE_HOST, tmp_path / "docs", agent_name=lambda: "test-agent")
    await backend.start()
    try:
        first_pid = (await backend.call("model_summary", {}))["structuredContent"]["pid"]
        created = await backend.call("document_new", {"name": "Art"})
        doc_id = created["structuredContent"]["storage_id"]
        assert (await backend.status())["document_name"] == "Art"

        with pytest.raises(LinkError) as err:
            await backend.call("feature_add", {"crash": True})
        assert err.value.code == "EngineCrashed"
        assert err.value.details == {"state_unknown": True}
        assert backend.crashes == 1
        assert (await backend.status())["state"] == "host_down"

        # The next call restarts the host and reopens the document first.
        after = await backend.call("model_summary", {})
        assert after["structuredContent"]["pid"] != first_pid
        reopened = json.loads(state_file.read_text())
        assert reopened["opened"] == doc_id
        assert reopened["pid"] == after["structuredContent"]["pid"]
        note = after["content"][-1]["text"]
        assert "restarted after a crash" in note
        # Once: the note rides on the first answer after the restart only.
        again = await backend.call("model_summary", {})
        assert all("restarted" not in c["text"] for c in again["content"])
        status = await backend.status()
        assert status["state"] == "ready" and status["host_restarts"] == 1
    finally:
        await backend.close()


async def test_a_host_speaking_another_protocol_is_refused(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("FAKE_HOST_PROTOCOL", "waffle-host/0")
    backend = HostBackend(FAKE_HOST, tmp_path / "docs", agent_name=lambda: "a")
    with pytest.raises(HostError, match="waffle-host/0"):
        await backend.start()


async def test_a_host_dying_before_ready_is_loud(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("FAKE_HOST_NO_READY", "1")
    backend = HostBackend(FAKE_HOST, tmp_path / "docs", agent_name=lambda: "a")
    with pytest.raises(HostError, match="exited before ready"):
        await backend.start()


# -- configuration -------------------------------------------------------------


def test_host_mode_needs_a_binary_and_defaults_the_documents_dir(tmp_path: Path) -> None:
    env = {"PATH": str(tmp_path), "HOME": str(tmp_path)}
    with pytest.raises(ConfigError, match="no host binary"):
        build_config(["--port", "20001", "--kernel", "host"], env)
    with pytest.raises(ConfigError, match="host binary not found"):
        build_config(
            ["--port", "20001", "--kernel", "host", "--host-binary", str(tmp_path / "nope")], env
        )
    config = build_config(
        ["--port", "20001", "--kernel", "host", "--host-binary", str(FAKE_HOST)], env
    )
    assert config.kernel == "host"
    assert config.host_binary == FAKE_HOST
    assert config.documents == tmp_path / ".local" / "share" / "waffle-iron" / "documents"
    assert default_documents_dir({"XDG_DATA_HOME": "/x"}) == Path("/x/waffle-iron/documents")

    on_path = tmp_path / "waffle-host"
    shutil.copy(FAKE_HOST, on_path)
    assert find_host_binary(None, env) == on_path
    assert find_host_binary(None, {**env, "WAFFLE_HOST_BIN": str(FAKE_HOST)}) == FAKE_HOST

    page = build_config(["--port", "20001"], env)
    assert page.kernel == "page" and page.documents is None and page.host_binary is None


# -- the relay process in host mode -------------------------------------------


@pytest.fixture
async def host_relay(tmp_path: Path) -> AsyncIterator[tuple[StdioRelay, Path]]:
    port = allocate_test_port()
    docs = tmp_path / "documents"
    proc = await StdioRelay.start(
        "--port",
        str(port),
        "--app-url",
        APP_URL,
        "--kernel",
        "host",
        "--host-binary",
        str(FAKE_HOST),
        "--documents",
        str(docs),
    )
    await proc.wait_for_stderr("kernel host")
    yield proc, docs
    if proc.proc.returncode is None:
        await proc.close()


async def test_host_mode_over_mcp_lists_only_served_tools(
    host_relay: tuple[StdioRelay, Path],
) -> None:
    proc, docs = host_relay
    await proc.initialize("pytest-agent")
    listed = await proc.request("tools/list")
    names = [t["name"] for t in listed["result"]["tools"]]
    assert names[:2] == ["waffle_connect", "waffle_status"]
    assert set(names[2:]) == {
        "model_summary",
        "feature_add",
        "document_info",
        "document_open",
        "document_new",
        "document_import",
        "storage_list",
    }, "only what the host's ready frame names"

    status = (await proc.call_tool("waffle_status"))["structuredContent"]
    assert status["state"] == "ready" and status["kernel"] == "host"
    assert status["documents"] == str(docs)

    connect = await proc.call_tool("waffle_connect")
    assert connect["isError"] is True
    assert connect["structuredContent"]["error"]["code"] == "HostCapability"

    summary = await proc.call_tool("model_summary")
    assert summary["isError"] is False
    assert summary["structuredContent"]["echo"]["agent_name"] == "pytest-agent"

    # A manifest tool the host does not serve is unknown to this relay.
    unknown = await proc.request("tools/call", {"name": "selection_get", "arguments": {}})
    assert unknown["error"]["code"] == -32602
    assert await proc.close() == 0


@pytest.mark.skipif(real_host_binary() is None, reason="no waffle-host binary built")
async def test_the_real_host_builds_a_part_over_mcp(tmp_path: Path) -> None:
    binary = real_host_binary()
    assert binary is not None
    port = allocate_test_port()
    docs = tmp_path / "documents"
    proc = await StdioRelay.start(
        "--port",
        str(port),
        "--app-url",
        APP_URL,
        "--kernel",
        "host",
        "--host-binary",
        str(binary),
        "--documents",
        str(docs),
        env=relay_env(),
    )
    try:
        await proc.wait_for_stderr("kernel host", timeout=60.0)
        await proc.initialize("pytest-agent")
        listed = await proc.request("tools/list")
        names = {t["name"] for t in listed["result"]["tools"]}
        assert {"sketch_create", "feature_add", "script_feature_add", "document_new"} <= names
        assert "selection_get" not in names and "tab_add" not in names

        created = await proc.call_tool("document_new", {"name": "Host part"})
        assert created["isError"] is False, created
        doc_id = created["structuredContent"]["document_id"]
        assert (docs / f"{doc_id}.waffle").is_file()

        sketch = await proc.call_tool(
            "sketch_create",
            {
                "plane": {"origin": [0, 0, 0], "normal": [0, 0, 1]},
                "entities": [
                    {"type": "Point", "id": 1, "x": 0, "y": 0},
                    {"type": "Point", "id": 2, "x": 0.02, "y": 0},
                    {"type": "Point", "id": 3, "x": 0.02, "y": 0.01},
                    {"type": "Point", "id": 4, "x": 0, "y": 0.01},
                    {"type": "Line", "id": 5, "start_id": 1, "end_id": 2},
                    {"type": "Line", "id": 6, "start_id": 2, "end_id": 3},
                    {"type": "Line", "id": 7, "start_id": 3, "end_id": 4},
                    {"type": "Line", "id": 8, "start_id": 4, "end_id": 1},
                ],
            },
        )
        assert sketch["isError"] is False, sketch
        extrude = await proc.call_tool(
            "feature_add",
            {
                "operation": {
                    "type": "Extrude",
                    "params": {
                        "sketch_id": sketch["structuredContent"]["feature_id"],
                        "profile_index": 0,
                        "profile_entity_ids": [5, 6, 7, 8],
                        "depth": 0.005,
                        "symmetric": False,
                        "cut": False,
                    },
                }
            },
        )
        assert extrude["isError"] is False, extrude
        summary = (await proc.call_tool("model_summary"))["structuredContent"]
        assert len(summary["bodies"]) == 1
        status = (await proc.call_tool("waffle_status"))["structuredContent"]
        assert status["document_name"] == "Host part"
        stored = json.loads((docs / f"{doc_id}.waffle").read_text())
        assert len(stored["tabs"][0]["kind"]["features"]["features"]) == 2, "autosaved"
    finally:
        assert await proc.close() == 0
        await wait_until(lambda: proc.proc.returncode is not None)


async def test_the_frame_helpers_agree_with_the_rust_codec() -> None:
    """The Python encoder and the Rust reader share one prefix layout: 2 × u32 BE."""
    from waffle_mcp_relay.host import encode_frame

    frame = encode_frame({"type": "tool", "id": "a"}, b"xyz")
    header_len, payload_len = int.from_bytes(frame[:4], "big"), int.from_bytes(frame[4:8], "big")
    assert json.loads(frame[8 : 8 + header_len]) == {"type": "tool", "id": "a"}
    assert payload_len == 3 and frame[8 + header_len :] == b"xyz"
    assert HOST_PROTOCOL == "waffle-host/1"
    await asyncio.sleep(0)
