"""§6.1 protocol errors: arguments failing `inputSchema`; P14 change notifications for both eras."""

from __future__ import annotations

import pytest
from mcp.server.lowlevel.server import NotificationOptions
from mcp.shared.exceptions import MCPError
from mcp.shared.subscriptions import ToolsListChanged
from support import APP_ORIGIN, APP_URL

from waffle_mcp_relay.backend import PageBackend
from waffle_mcp_relay.config import RelayConfig
from waffle_mcp_relay.link import LinkServer
from waffle_mcp_relay.manifest import Manifest, ManifestError, load_bundled
from waffle_mcp_relay.pairing import Pairing
from waffle_mcp_relay.server import ArgumentValidator, RelayApp

TOOL = {
    "name": "feature_rename",
    "inputSchema": {
        "type": "object",
        "properties": {
            "feature_id": {"type": "string"},
            "new_name": {"type": "string", "minLength": 1},
            "tags": {"type": "array", "items": {"type": "string"}},
        },
        "required": ["feature_id", "new_name"],
        "additionalProperties": False,
    },
}


def invalid(arguments: dict[str, object]) -> MCPError:
    with pytest.raises(MCPError) as caught:
        ArgumentValidator().check(TOOL, arguments)
    assert caught.value.error.code == -32602
    return caught.value


def test_valid_arguments_pass() -> None:
    ArgumentValidator().check(TOOL, {"feature_id": "f", "new_name": "Base"})


def test_missing_required_property_points_at_arguments() -> None:
    err = invalid({"feature_id": "f"})
    assert err.error.data == "/arguments"
    assert "new_name" in err.error.message


def test_wrong_type_points_at_the_value() -> None:
    assert invalid({"feature_id": 7, "new_name": "x"}).error.data == "/arguments/feature_id"


def test_nested_error_pointer_includes_array_index() -> None:
    err = invalid({"feature_id": "f", "new_name": "x", "tags": ["a", 3]})
    assert err.error.data == "/arguments/tags/1"


def test_unknown_property_refused() -> None:
    assert invalid({"feature_id": "f", "new_name": "x", "extra": 1}).error.data == "/arguments"


def test_pointer_escapes_tilde_and_slash() -> None:
    tool = {
        "name": "t",
        "inputSchema": {"type": "object", "properties": {"a/b~c": {"type": "string"}}},
    }
    with pytest.raises(MCPError) as caught:
        ArgumentValidator().check(tool, {"a/b~c": 1})
    assert caught.value.error.data == "/arguments/a~1b~0c"


def test_manifest_with_invalid_input_schema_rejected() -> None:
    with pytest.raises(ManifestError, match="invalid inputSchema"):
        Manifest.from_tools([{"name": "x", "inputSchema": {"type": 12}}])


def make_app() -> RelayApp:
    config = RelayConfig(
        port=1024,
        bind="127.0.0.1",
        app_url=APP_URL,
        allow_origins=(APP_ORIGIN,),
        agent_name=None,
        open_browser=False,
        ssl_context=None,
    )
    link = LinkServer(
        pairing=Pairing(),
        allow_origins=[APP_ORIGIN],
        manifest=load_bundled(),
        agent_name=lambda: "test-agent",
    )
    return RelayApp(config, PageBackend(link))


async def test_tools_changed_is_published_for_listen_streams() -> None:
    # 2026-07-28 clients receive list_changed only on a subscriptions/listen stream.
    app = make_app()
    events: list[object] = []
    app.subscription_bus.subscribe(events.append)
    await app.notify_tools_changed()
    assert events == [ToolsListChanged()]


def test_list_changed_capability_advertised_in_both_eras() -> None:
    app = make_app()
    options = NotificationOptions(tools_changed=True)
    modern = app.server.get_capabilities(options, protocol_version="2026-07-28")
    handshake = app.server.get_capabilities(options, protocol_version="2025-11-25")
    assert modern.tools is not None and modern.tools.list_changed is True
    assert handshake.tools is not None and handshake.tools.list_changed is True
