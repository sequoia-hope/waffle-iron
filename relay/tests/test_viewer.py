"""The viewer link in host mode (`specs/waffle_server_mode.md` §4, `waffle-viewer/1`).

Against the fake host: a viewer attaches with a code and gets the snapshot,
asks for blobs by id and gets the bytes (or `missing`), every attached viewer
gets a snapshot after a tool that changed the document, a resume whose `have`
matches the host's state gets only a `welcome`, and bad codes and origins are
refused before any state is shared.
"""

from __future__ import annotations

import asyncio
import json
from collections.abc import AsyncIterator, Callable
from pathlib import Path
from typing import Any

import pytest
from support import APP_ORIGIN, allocate_test_port
from websockets.asyncio.client import ClientConnection, connect
from websockets.exceptions import InvalidStatus

from waffle_mcp_relay.host import HostBackend
from waffle_mcp_relay.viewer import (
    VIEWER_PROTOCOL,
    ViewerPairing,
    ViewerServer,
    decode_blob_frame,
)

FAKE_HOST = Path(__file__).with_name("fake_host.py")


class FakeViewer:
    """A browser viewer speaking `waffle-viewer/1`; records every frame it receives."""

    def __init__(self, ws: ClientConnection) -> None:
        self.ws = ws
        self.frames: list[Any] = []

    @classmethod
    async def connect(cls, url: str, origin: str | None = APP_ORIGIN) -> FakeViewer:
        ws = await connect(url, origin=origin, ping_interval=None)  # type: ignore[arg-type]
        return cls(ws)

    async def send(self, frame: dict[str, Any]) -> None:
        await self.ws.send(json.dumps(frame))

    async def attach(self, **fields: Any) -> None:
        await self.send({"type": "attach", "protocol": VIEWER_PROTOCOL, **fields})

    async def recv(self, timeout: float = 5.0) -> Any:
        raw = await asyncio.wait_for(self.ws.recv(), timeout)
        frame: Any = decode_blob_frame(raw) if isinstance(raw, bytes) else json.loads(raw)
        self.frames.append(frame)
        return frame

    async def recv_type(self, kind: str, timeout: float = 5.0) -> Any:
        while True:
            frame = await self.recv(timeout)
            header = frame[0] if isinstance(frame, tuple) else frame
            if header.get("type") == "ping" and kind != "ping":
                await self.send({"type": "pong"})
                continue
            assert header.get("type") == kind, f"expected {kind!r}, got {frame!r}"
            return frame

    async def close(self) -> None:
        await self.ws.close()


@pytest.fixture
async def stack(tmp_path: Path) -> AsyncIterator[tuple[HostBackend, ViewerServer, str]]:
    backend = HostBackend(FAKE_HOST, tmp_path / "docs", agent_name=lambda: "viewer-test")
    await backend.start()
    server = ViewerServer(
        host=backend,
        pairing=ViewerPairing(),
        allow_origins=[APP_ORIGIN],
        ping_interval=0.2,
        pong_timeout=1.0,
    )
    port = allocate_test_port()
    await server.start("127.0.0.1", port)
    try:
        yield backend, server, f"ws://127.0.0.1:{port}"
    finally:
        await server.close()
        await backend.close()


async def test_a_viewer_attaches_gets_the_snapshot_and_fetches_blobs_by_id(
    stack: tuple[HostBackend, ViewerServer, str],
) -> None:
    backend, server, url = stack
    code, expires = server.pairing.issue_code()
    assert expires is not None

    viewer = await FakeViewer.connect(url)
    await viewer.attach(code=code)
    welcome = await viewer.recv_type("welcome")
    assert welcome["protocol"] == VIEWER_PROTOCOL
    assert welcome["epoch"] == backend.epoch and welcome["session"]
    assert server.viewer_count == 1

    snapshot = await viewer.recv_type("snapshot")
    assert snapshot["revision"] == 0
    bodies = snapshot["bodies"]
    assert [b["mesh_id"] for b in bodies] == ["m1"]

    await viewer.send({"type": "want", "mesh_ids": ["m1", "nope"]})
    header, payload = await viewer.recv_type("blob")
    assert header["mesh_id"] == "m1" and header["encoding"] == "raw/1"
    assert payload == b"BLOB-BYTES" and header["byte_length"] == len(payload)
    missing = await viewer.recv_type("blob")
    assert missing == {"type": "blob", "mesh_id": "nope", "missing": True}

    # The same blob again comes from the relay's cache (the fake host would
    # answer too; what matters is that the bytes are identical).
    await viewer.send({"type": "want", "mesh_ids": ["m1"]})
    _, again = await viewer.recv_type("blob")
    assert again == payload

    # The code was single use.
    second = await FakeViewer.connect(url)
    await second.attach(code=code)
    bye = await second.recv_type("bye")
    assert bye["reason"] == "invalid_code"

    await viewer.close()
    await asyncio.sleep(0.05)
    assert server.viewer_count == 0


async def test_a_change_pushes_a_snapshot_to_every_viewer(
    stack: tuple[HostBackend, ViewerServer, str],
) -> None:
    backend, server, url = stack
    viewers = []
    for _ in range(2):
        code, _ = server.pairing.issue_code()
        v = await FakeViewer.connect(url)
        await v.attach(code=code)
        await v.recv_type("welcome")
        await v.recv_type("snapshot")
        viewers.append(v)

    result = await backend.call("feature_add", {})
    assert result["isError"] is False
    for v in viewers:
        pushed = await v.recv_type("snapshot")
        assert pushed["revision"] == 1
    latest = await backend.latest_snapshot()
    assert latest is not None and latest["revision"] == 1

    # A read-only tool pushes nothing: the next frame is only the heartbeat.
    await backend.call("model_summary", {})
    ping = await viewers[0].recv_type("ping")
    assert ping == {"type": "ping"}
    for v in viewers:
        await v.close()


async def test_a_resume_that_holds_the_current_state_gets_only_a_welcome(
    stack: tuple[HostBackend, ViewerServer, str],
) -> None:
    backend, server, url = stack
    code, _ = server.pairing.issue_code()
    viewer = await FakeViewer.connect(url)
    await viewer.attach(code=code)
    welcome = await viewer.recv_type("welcome")
    snapshot = await viewer.recv_type("snapshot")
    await viewer.close()

    # §4.6 step 4–5: the tab was killed and reloaded; it remembers the
    # session and the state it holds.
    resumed = await FakeViewer.connect(url)
    await resumed.attach(
        session=welcome["session"],
        have={"epoch": snapshot["epoch"], "revision": snapshot["revision"]},
    )
    again = await resumed.recv_type("welcome")
    assert again["viewer_id"] == welcome["viewer_id"], "the same viewer"
    # Nothing but the heartbeat follows: no snapshot, no blobs.
    assert (await resumed.recv_type("ping")) == {"type": "ping"}

    # A stale `have` gets the snapshot.
    await resumed.close()
    stale = await FakeViewer.connect(url)
    await stale.attach(session=welcome["session"], have={"epoch": "other", "revision": 0})
    await stale.recv_type("welcome")
    assert (await stale.recv_type("snapshot"))["revision"] == snapshot["revision"]
    await stale.close()

    # An unknown session is refused.
    unknown = await FakeViewer.connect(url)
    await unknown.attach(session="not-a-session")
    assert (await unknown.recv_type("bye"))["reason"] == "session_expired"


async def test_a_wrong_origin_or_protocol_is_refused_before_any_state(
    stack: tuple[HostBackend, ViewerServer, str],
) -> None:
    _, server, url = stack
    with pytest.raises(InvalidStatus) as refused:
        await FakeViewer.connect(url, origin="https://evil.example")
    assert refused.value.response.status_code == 403

    code, _ = server.pairing.issue_code()
    viewer = await FakeViewer.connect(url)
    await viewer.send({"type": "attach", "protocol": "waffle-viewer/0", "code": code})
    bye = await viewer.recv_type("bye")
    assert bye["reason"] == "protocol_mismatch"
    assert bye["supported"] == [VIEWER_PROTOCOL]


def test_viewer_pairing_expires_codes_and_sessions() -> None:
    now = [1000.0]
    clock: Callable[[], float] = lambda: now[0]  # noqa: E731
    pairing = ViewerPairing(clock, resume_s=60.0)
    code, expires = pairing.issue_code()
    assert expires == 1300.0
    now[0] = 1301.0
    assert pairing.admit_code(code).reason == "invalid_code"

    code, _ = pairing.issue_code()
    admitted = pairing.admit_code(code)
    assert admitted.ok and admitted.session is not None
    assert pairing.admit_session(admitted.session).reason == "already_attached"
    pairing.disconnected(admitted.session)
    now[0] += 30.0
    assert pairing.admit_session(admitted.session).resumed is True
    pairing.disconnected(admitted.session)
    now[0] += 61.0
    assert pairing.admit_session(admitted.session).reason == "session_expired"

    persistent = ViewerPairing(clock, persistent_code="keep")
    assert persistent.issue_code() == ("keep", None)
    assert persistent.admit_code("keep").ok and persistent.admit_code("keep").ok
