"""The viewer link in host mode (`specs/waffle_server_mode.md` §4, `waffle-viewer/1`).

Against the fake host: a viewer attaches with a code and gets the snapshot,
asks for blobs by id (in the encoding it named) and gets the bytes (or
`missing`), every attached viewer gets the change after a tool that changed
the document — a keyed `update` when it holds the state it builds on, a
snapshot otherwise — with the host's `rebuild` frames ahead of it, a resume
whose `have` matches the host's state gets only a `welcome`, a session token
outlives the pairing that minted it while the secret holds, the three viewer
tools are served from the focused visible viewer, presence rides in
`activity`, and bad codes and origins are refused before any state is shared.
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
from waffle_mcp_relay.link import LinkError
from waffle_mcp_relay.viewer import (
    VIEWER_PROTOCOL,
    ViewerPairing,
    ViewerServer,
    decode_blob_frame,
    snapshot_update,
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

    async def recv_type(
        self, kind: str, timeout: float = 5.0, *, skip: tuple[str, ...] = ()
    ) -> Any:
        """The next frame of `kind`.

        Pings are answered; `skip` kinds and `session` refreshes are passed over.
        """
        while True:
            frame = await self.recv(timeout)
            header = frame[0] if isinstance(frame, tuple) else frame
            got = header.get("type")
            if got == "ping" and kind != "ping":
                await self.send({"type": "pong"})
                continue
            if got in skip or (got == "session" and kind != "session"):
                continue
            assert got == kind, f"expected {kind!r}, got {frame!r}"
            return frame

    async def close(self) -> None:
        await self.ws.close()


async def attached(url: str, server: ViewerServer, **attach: Any) -> tuple[FakeViewer, Any, Any]:
    code, _ = server.pairing.issue_code()
    viewer = await FakeViewer.connect(url)
    await viewer.attach(code=code, **attach)
    welcome = await viewer.recv_type("welcome")
    snapshot = await viewer.recv_type("snapshot", skip=("update",))
    return viewer, welcome, snapshot


@pytest.fixture
async def stack(tmp_path: Path) -> AsyncIterator[tuple[HostBackend, ViewerServer, str]]:
    backend = HostBackend(FAKE_HOST, tmp_path / "docs", agent_name=lambda: "viewer-test")
    await backend.start()
    server = ViewerServer(
        host=backend,
        pairing=ViewerPairing(secret=b"test-secret"),
        allow_origins=[APP_ORIGIN],
        ping_interval=0.2,
        pong_timeout=1.0,
        tool_timeout=2.0,
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
    assert welcome["encoding"] == "raw/1", "no preference named ⇒ raw/1"
    assert server.viewer_count == 1

    snapshot = await viewer.recv_type("snapshot")
    assert snapshot["revision"] == 0
    bodies = snapshot["bodies"]
    assert [b["mesh_id"] for b in bodies] == ["m1"]
    # The relay stamps every snapshot with who is doing what and who watches.
    assert snapshot["activity"]["agent"] == "viewer-test"
    assert snapshot["activity"]["tool"] is None
    assert [v["viewer_id"] for v in snapshot["activity"]["viewers"]] == [welcome["viewer_id"]]

    await viewer.send({"type": "want", "mesh_ids": ["m1", "nope"]})
    header, payload = await viewer.recv_type("blob", skip=("update",))
    assert header["mesh_id"] == "m1" and header["encoding"] == "raw/1"
    assert payload == b"BLOB-BYTES" and header["byte_length"] == len(payload)
    missing = await viewer.recv_type("blob", skip=("update",))
    assert missing == {"type": "blob", "mesh_id": "nope", "encoding": "raw/1", "missing": True}

    # The same blob again comes from the relay's cache (the fake host would
    # answer too; what matters is that the bytes are identical).
    await viewer.send({"type": "want", "mesh_ids": ["m1"]})
    _, again = await viewer.recv_type("blob", skip=("update",))
    assert again == payload

    # The code was single use.
    second = await FakeViewer.connect(url)
    await second.attach(code=code)
    bye = await second.recv_type("bye")
    assert bye["reason"] == "invalid_code"

    await viewer.close()
    await asyncio.sleep(0.05)
    assert server.viewer_count == 0


async def test_a_viewer_names_its_encoding_and_gets_blobs_in_it(
    stack: tuple[HostBackend, ViewerServer, str],
) -> None:
    _, server, url = stack
    viewer, welcome, _ = await attached(url, server, encodings=["mq/1", "raw/1"])
    assert welcome["encoding"] == "mq/1"

    await viewer.send({"type": "want", "mesh_ids": ["m1"]})
    header, payload = await viewer.recv_type("blob", skip=("update",))
    assert header["encoding"] == "mq/1" and payload == b"MQ-BYTES"

    # A `want` may name an encoding of its own (the fallback path).
    await viewer.send({"type": "want", "mesh_ids": ["m1"], "encoding": "raw/1"})
    header, payload = await viewer.recv_type("blob", skip=("update",))
    assert header["encoding"] == "raw/1" and payload == b"BLOB-BYTES"

    # An encoding this relay does not serve falls back to the viewer's.
    other, other_welcome, _ = await attached(url, server, encodings=["zip/9"])
    assert other_welcome["encoding"] == "raw/1"
    await viewer.close()
    await other.close()


async def test_a_change_pushes_an_update_to_every_viewer_behind_the_rebuild_frames(
    stack: tuple[HostBackend, ViewerServer, str],
) -> None:
    backend, server, url = stack
    viewers = [(await attached(url, server))[0] for _ in range(2)]

    result = await backend.call("feature_add", {})
    assert result["isError"] is False
    for v in viewers:
        # The host's rebuild bracket first (the spinner), then the change as
        # a keyed update: only what changed, on the revision each holds.
        started = await v.recv_type("rebuild", skip=("update",))
        assert started["state"] == "started" and started["tool"] == "feature_add"
        frame = await v.recv_type("rebuild")
        while frame["state"] == "progress":
            assert frame["feature_name"] == "Body"
            frame = await v.recv_type("rebuild")
        assert frame["state"] == "done" and frame["ok"] is True
        pushed = await v.recv_type("update")
        assert pushed["base_revision"] == 0 and pushed["revision"] == 1
        assert pushed["tree"] == {"features": [{"name": "Body 1"}], "active_index": None}
        assert "bodies" not in pushed, "an unchanged key is not in the update"
        assert pushed["activity"]["viewers"] and len(pushed["activity"]["viewers"]) == 2
    latest = await backend.latest_snapshot()
    assert latest is not None and latest["revision"] == 1

    # A read-only tool pushes nothing: the next frame is only the heartbeat.
    await backend.call("model_summary", {})
    ping = await viewers[0].recv_type("ping")
    assert ping == {"type": "ping"}
    for v in viewers:
        await v.close()


def test_snapshot_update_is_the_changed_keys_only() -> None:
    a = {
        "type": "snapshot",
        "epoch": "e",
        "revision": 3,
        "tree": {"n": 1},
        "bodies": [1],
        "errors": [],
    }
    b = {**a, "revision": 4, "tree": {"n": 2}}
    update = snapshot_update(a, b)
    assert update == {
        "type": "update",
        "protocol": VIEWER_PROTOCOL,
        "epoch": "e",
        "base_revision": 3,
        "revision": 4,
        "tree": {"n": 2},
    }
    assert snapshot_update(None, b) is None
    assert snapshot_update({**a, "epoch": "other"}, b) is None, "another epoch ⇒ snapshot"


async def test_a_viewer_behind_the_update_gets_a_snapshot_instead(
    stack: tuple[HostBackend, ViewerServer, str],
) -> None:
    backend, server, url = stack
    current, _, snapshot = await attached(url, server)
    # A viewer that holds an older state than the host's (its `have` names
    # revision 0 after the host moved to 1) cannot apply an update from 1 to 2.
    await backend.call("feature_add", {})
    await current.recv_type("update", skip=("rebuild",))
    code, _ = server.pairing.issue_code()
    stale = await FakeViewer.connect(url)
    await stale.attach(code=code, have={"epoch": snapshot["epoch"], "revision": 0})
    await stale.recv_type("welcome")
    got = await stale.recv_type("snapshot", skip=("update",))
    assert got["revision"] == 1, "a have that does not match gets the snapshot"

    await backend.call("feature_add", {})
    assert (await current.recv_type("update", skip=("rebuild",)))["base_revision"] == 1
    assert (await stale.recv_type("update", skip=("rebuild",)))["base_revision"] == 1

    # A viewer may ask for a resync outright (§4.4: any gap ⇒ snapshot).
    await stale.send({"type": "snapshot"})
    assert (await stale.recv_type("snapshot", skip=("update",)))["revision"] == 2
    await current.close()
    await stale.close()


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
    assert len(again["session"].split(".")) == 3, "an HMAC token: viewer_id.expiry.signature"
    # Nothing but the heartbeat (and its token refresh) follows: no snapshot, no blobs.
    assert (await resumed.recv_type("ping", skip=("update",))) == {"type": "ping"}
    refresh = await resumed.recv_type("session")
    assert refresh["expires_at"] >= again["expires_at"] > 0

    # A stale `have` gets the snapshot.
    await resumed.close()
    stale = await FakeViewer.connect(url)
    await stale.attach(session=refresh["session"], have={"epoch": "other", "revision": 0})
    await stale.recv_type("welcome")
    assert (await stale.recv_type("snapshot", skip=("update",)))["revision"] == snapshot["revision"]
    await stale.close()

    # An unknown session is refused.
    unknown = await FakeViewer.connect(url)
    await unknown.attach(session="not-a-session")
    assert (await unknown.recv_type("bye"))["reason"] == "session_expired"


async def test_a_session_token_outlives_the_pairing_that_minted_it(
    stack: tuple[HostBackend, ViewerServer, str],
) -> None:
    """§4.7 / §4.8: a relay restart keeps its viewers, as long as the secret does."""
    _, server, url = stack
    viewer, welcome, _ = await attached(url, server)
    await viewer.close()
    await asyncio.sleep(0.05)

    # The next process: a fresh pairing over the same secret admits the token…
    server.pairing = ViewerPairing(secret=b"test-secret")
    resumed = await FakeViewer.connect(url)
    await resumed.attach(session=welcome["session"])
    assert (await resumed.recv_type("welcome"))["viewer_id"] == welcome["viewer_id"]
    await resumed.close()
    await asyncio.sleep(0.05)

    # …and one over another secret (the file was deleted: every viewer revoked) does not.
    server.pairing = ViewerPairing(secret=b"another-secret")
    refused = await FakeViewer.connect(url)
    await refused.attach(session=welcome["session"])
    assert (await refused.recv_type("bye"))["reason"] == "session_expired"


async def test_the_viewer_tools_are_served_from_the_focused_visible_viewer(
    stack: tuple[HostBackend, ViewerServer, str],
) -> None:
    backend, server, url = stack
    assert backend.tool_names() is not None
    assert {"selection_get", "viewport_view", "viewport_capture"} <= set(backend.tool_names() or ())

    # No viewer: refused with the host's own code.
    with pytest.raises(LinkError) as refused:
        await backend.call("viewport_capture", {})
    assert refused.value.code == "ViewerUnavailable"

    viewer, welcome, _ = await attached(url, server)

    # selection_get reads the viewer's last `select` (empty until it sends one).
    empty = await backend.call("selection_get", {})
    assert empty["structuredContent"] == {
        "selection": [],
        "selected_feature_id": None,
        "instance_path": None,
    }
    picked = {
        "selection": [
            {"geom_ref": {"kind": {"type": "Face"}}, "kind": "Face", "body_id": "f1/Main"}
        ],
        "selected_feature_id": "f1",
        "instance_path": None,
    }
    await viewer.send({"type": "select", **picked})
    await asyncio.sleep(0.05)
    assert (await backend.call("selection_get", {}))["structuredContent"] == picked

    # viewport_capture round-trips through the viewer.
    async def answer_capture() -> None:
        request = await viewer.recv_type("capture_request", skip=("update",))
        assert request["max_edge_px"] == 640
        await viewer.send(
            {
                "type": "capture_result",
                "id": request["id"],
                "png_base64": "iVBORw0=",
                "width": 64,
                "height": 48,
            }
        )

    answering = asyncio.create_task(answer_capture())
    captured = await backend.call("viewport_capture", {"max_edge_px": 640})
    await answering
    assert captured["content"][0] == {"type": "image", "data": "iVBORw0=", "mimeType": "image/png"}
    assert captured["structuredContent"]["width"] == 64
    assert captured["structuredContent"]["viewer_id"] == welcome["viewer_id"]

    # viewport_view too; a viewer's refusal comes back as the tool's error.
    async def answer_view() -> None:
        request = await viewer.recv_type("view_request", skip=("update",))
        assert request["view"] == "iso" and request["fit"] is True
        await viewer.send(
            {"type": "view_result", "id": request["id"], "camera": {"position": [1, 2, 3]}}
        )

    answering = asyncio.create_task(answer_view())
    viewed = await backend.call("viewport_view", {"view": "iso"})
    await answering
    assert viewed["structuredContent"]["camera"] == {"position": [1, 2, 3]}

    async def refuse_capture() -> None:
        request = await viewer.recv_type("capture_request", skip=("update",))
        await viewer.send(
            {
                "type": "capture_result",
                "id": request["id"],
                "error": {
                    "code": "ViewportUnavailable",
                    "message": "hidden",
                    "details": {"reason": "hidden"},
                },
            }
        )

    answering = asyncio.create_task(refuse_capture())
    with pytest.raises(LinkError) as err:
        await backend.call("viewport_capture", {})
    await answering
    assert err.value.code == "ViewportUnavailable" and err.value.details == {"reason": "hidden"}

    # A hidden viewer is not asked; a second, visible one is the focused one.
    await viewer.send({"type": "visible", "visible": False})
    await asyncio.sleep(0.05)
    with pytest.raises(LinkError) as hidden:
        await backend.call("selection_get", {})
    assert hidden.value.code == "ViewerUnavailable"
    other, other_welcome, _ = await attached(url, server)
    presence = (await backend.status())["viewer_presence"]
    assert {v["viewer_id"]: v["focused"] for v in presence} == {
        welcome["viewer_id"]: False,
        other_welcome["viewer_id"]: True,
    }
    assert (await backend.call("selection_get", {}))["structuredContent"]["selection"] == []
    await viewer.close()
    await other.close()


async def test_presence_changes_reach_every_viewer_as_an_activity_update(
    stack: tuple[HostBackend, ViewerServer, str],
) -> None:
    _, server, url = stack
    first, first_welcome, _ = await attached(url, server)
    second, second_welcome, _ = await attached(url, server)
    joined = await first.recv_type("update")
    assert set(joined) == {"type", "protocol", "epoch", "base_revision", "revision", "activity"}
    assert joined["base_revision"] == joined["revision"]
    assert {v["viewer_id"] for v in joined["activity"]["viewers"]} == {
        first_welcome["viewer_id"],
        second_welcome["viewer_id"],
    }
    await second.close()
    left = await first.recv_type("update")
    assert [v["viewer_id"] for v in left["activity"]["viewers"]] == [first_welcome["viewer_id"]]
    await first.close()


async def test_a_host_crash_resyncs_every_viewer_without_refetching_its_blobs(
    stack: tuple[HostBackend, ViewerServer, str],
) -> None:
    """Oracle V2: the host dies mid-session; the next call restarts it and
    reopens its document; the viewer converges on the new epoch and asks for
    no blob it already holds (mesh ids are content hashes, so a restart keeps
    them — pinned host-side by `a_restarted_host_names_the_same_mesh_ids`)."""
    backend, server, url = stack
    viewer, welcome, snapshot = await attached(url, server)
    await viewer.send({"type": "want", "mesh_ids": ["m1"]})
    await viewer.recv_type("blob", skip=("update",))
    first_epoch = snapshot["epoch"]

    # Name the document the restarted host must reopen. Inside one process
    # this is an ordinary change: a keyed update, not a snapshot.
    await backend.call("document_open", {"id": "doc-1"})
    reopened = await viewer.recv_type("update", skip=("rebuild",))
    assert reopened["epoch"] == first_epoch

    with pytest.raises(LinkError) as crashed:
        await backend.call("feature_add", {"crash": True})
    assert crashed.value.code == "EngineCrashed"

    # The next call restarts the child and reopens the document; its snapshot
    # carries the NEW epoch, so the viewer takes the whole thing rather than
    # trying to apply an update across processes.
    await backend.call("model_summary", {})
    resynced = await viewer.recv_type("snapshot", skip=("update", "rebuild"))
    assert resynced["epoch"] != first_epoch
    assert [b["mesh_id"] for b in resynced["bodies"]] == ["m1"], "the same content, the same id"
    assert backend.crashes == 1
    await viewer.close()


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
    pairing = ViewerPairing(clock, resume_s=60.0, secret=b"s")
    code, expires = pairing.issue_code()
    assert expires == 1300.0
    now[0] = 1301.0
    assert pairing.admit_code(code).reason == "invalid_code"

    code, _ = pairing.issue_code()
    admitted = pairing.admit_code(code)
    assert admitted.ok and admitted.session is not None
    assert admitted.expires_at == 1361.0, "a token lives for the resume window"
    assert pairing.admit_session(admitted.session).reason == "already_attached"
    assert admitted.viewer_id is not None
    pairing.disconnected(admitted.viewer_id)
    now[0] += 30.0
    resumed = pairing.admit_session(admitted.session)
    assert resumed.resumed is True and resumed.viewer_id == admitted.viewer_id
    pairing.disconnected(admitted.viewer_id)
    now[0] += 61.0
    assert pairing.admit_session(admitted.session).reason == "session_expired"

    # A refreshed token pushes the expiry out; a forged one never verifies.
    token, expires_at = pairing.mint("viewer-x")
    assert expires_at == now[0] + 60.0
    assert pairing.admit_session(token).ok
    viewer_id, expiry, signature = token.split(".")
    assert (
        pairing.admit_session(f"{viewer_id}.{int(expiry) + 1000}.{signature}").reason
        == "session_expired"
    )
    assert (
        ViewerPairing(clock, resume_s=60.0, secret=b"other").admit_session(token).reason
        == "session_expired"
    )

    persistent = ViewerPairing(clock, persistent_code="keep", secret=b"s")
    assert persistent.issue_code() == ("keep", None)
    assert persistent.admit_code("keep").ok and persistent.admit_code("keep").ok
