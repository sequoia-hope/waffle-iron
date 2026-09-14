"""§2.1 relay process configuration: O15 (bind), O16 (port resolution)."""

from __future__ import annotations

import socket
import subprocess
import sys
from pathlib import Path

import pytest
from support import relay_env

from waffle_mcp_relay import cli
from waffle_mcp_relay.config import (
    HOSTED_APP_URL,
    NO_PORT_MESSAGE,
    ConfigError,
    build_config,
)


def run_relay_cli(args: list[str], env: dict[str, str]) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        [sys.executable, "-m", "waffle_mcp_relay", *args],
        env=env,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        timeout=30,
        check=False,
    )


def write_fake_proj(directory: Path, body: str) -> None:
    proj = directory / "proj"
    proj.write_text(f"#!/bin/sh\n{body}\n")
    proj.chmod(0o755)


# -- O16: port resolution ----------------------------------------------------


def test_o16_no_port_source_exits_2_with_exact_message(tmp_path: Path) -> None:
    empty_bin = tmp_path / "bin"
    empty_bin.mkdir()
    done = run_relay_cli([], relay_env(PATH=str(empty_bin)))
    assert done.returncode == 2
    assert done.stderr.decode() == f"{NO_PORT_MESSAGE}\n"
    assert done.stdout == b""


def test_o16_no_port_source_opens_no_socket(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.delenv("PORT", raising=False)
    monkeypatch.setenv("PATH", str(tmp_path))
    opened: list[object] = []
    real_socket = socket.socket

    class SpySocket(real_socket):  # type: ignore[misc, valid-type]
        def __init__(self, *args: object, **kwargs: object) -> None:
            opened.append(args)
            super().__init__(*args, **kwargs)  # type: ignore[arg-type]

    monkeypatch.setattr(socket, "socket", SpySocket)
    assert cli.main([]) == 2
    assert opened == []


def test_o16_failing_proj_is_unresolved(tmp_path: Path) -> None:
    write_fake_proj(tmp_path, "echo 'not registered' >&2; exit 3")
    done = run_relay_cli([], relay_env(PATH=str(tmp_path)))
    assert done.returncode == 2
    assert done.stderr.decode() == f"{NO_PORT_MESSAGE}\n"


def test_port_precedence_flag_then_env_then_proj() -> None:
    assert build_config(["--port", "20001"], {"PORT": "20002"}, lambda: "20003").port == 20001
    assert build_config([], {"PORT": "20002"}, lambda: "20003").port == 20002
    assert build_config([], {}, lambda: "20003").port == 20003
    with pytest.raises(ConfigError, match=f"^{NO_PORT_MESSAGE.replace('$', r'\$')}$"):
        build_config([], {}, lambda: None)


def test_proj_port_subprocess_is_used(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    write_fake_proj(tmp_path, "echo 20004")
    monkeypatch.setenv("PATH", str(tmp_path))
    assert build_config([], {}).port == 20004


@pytest.mark.parametrize("bad", ["80", "1023", "65536", "abc", ""])
def test_invalid_port(bad: str) -> None:
    with pytest.raises(ConfigError, match="^invalid port$"):
        build_config(["--port", bad], {}, lambda: None)


# -- O15: bind ---------------------------------------------------------------


def test_o15_default_bind_is_loopback() -> None:
    config = build_config(["--port", "20005"], {}, lambda: None)
    assert config.bind == "127.0.0.1"
    assert config.relay_url == "ws://127.0.0.1:20005"


def test_o15_non_loopback_without_tls_exits_2(tmp_path: Path) -> None:
    done = run_relay_cli(["--port", "20006", "--bind", "0.0.0.0"], relay_env())
    assert done.returncode == 2
    assert done.stderr.decode() == "non-loopback bind requires TLS\n"
    assert done.stdout == b""


def test_unreadable_tls_material() -> None:
    with pytest.raises(ConfigError, match="^cannot read TLS material$"):
        build_config(
            [
                "--port",
                "20007",
                "--bind",
                "0.0.0.0",
                "--tls-cert",
                "/nonexistent.pem",
                "--tls-key",
                "/nonexistent.key",
            ],
            {},
            lambda: None,
        )


# -- app url, origins, agent name -------------------------------------------


def test_default_app_url_and_origin() -> None:
    config = build_config(["--port", "20008"], {}, lambda: None)
    assert config.app_url == HOSTED_APP_URL
    assert config.allow_origins == ("https://sequoia-hope.github.io",)


def test_dev_app_url_and_repeatable_origins() -> None:
    config = build_config(
        [
            "--port",
            "20009",
            "--app-url",
            "http://localhost:20010",
            "--allow-origin",
            "http://localhost:20010",
            "--allow-origin",
            "http://127.0.0.1:20010",
        ],
        {},
        lambda: None,
    )
    assert config.app_url == "http://localhost:20010/"
    assert config.allow_origins == ("http://localhost:20010", "http://127.0.0.1:20010")


@pytest.mark.parametrize("url", ["http://example.com/", "ftp://x/", "not a url", "https://h/?q=1"])
def test_invalid_app_url(url: str) -> None:
    with pytest.raises(ConfigError, match="^invalid app url$"):
        build_config(["--port", "20011", "--app-url", url], {}, lambda: None)


@pytest.mark.parametrize(
    "origin", ["https://*.example", "https://a.example/path", "null", "a.example"]
)
def test_invalid_origin(origin: str) -> None:
    with pytest.raises(ConfigError, match="^invalid origin$"):
        build_config(["--port", "20012", "--allow-origin", origin], {}, lambda: None)


def test_public_url_replaces_advertised_relay_address() -> None:
    config = build_config(
        [
            "--port",
            "20014",
            "--public-url",
            "wss://Host.example:10000/relay",
            "--app-url",
            "https://host.example:10000/",
        ],
        {},
        lambda: None,
    )
    assert config.bind == "127.0.0.1"
    assert config.relay_url == "wss://host.example:10000/relay"
    assert config.allow_origins == ("https://host.example:10000",)


def test_public_url_without_path_gets_root() -> None:
    config = build_config(
        ["--port", "20015", "--public-url", "wss://host.example"], {}, lambda: None
    )
    assert config.relay_url == "wss://host.example/"


@pytest.mark.parametrize(
    "url", ["ws://host.example/", "https://host.example/", "wss://h/?q=1", "wss://h/#f", "x"]
)
def test_invalid_public_url(url: str) -> None:
    with pytest.raises(ConfigError, match="^invalid public url$"):
        build_config(["--port", "20016", "--public-url", url], {}, lambda: None)


@pytest.mark.parametrize("name", ["", "x" * 129, "bad\nname"])
def test_invalid_agent_name(name: str) -> None:
    with pytest.raises(ConfigError, match="^invalid agent name$"):
        build_config(["--port", "20013", "--agent-name", name], {}, lambda: None)
