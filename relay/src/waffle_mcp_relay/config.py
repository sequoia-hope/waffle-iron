"""Command line and startup configuration (spec §2.1).

Every configuration error is a `ConfigError`; `cli.main` prints its message
to stderr and exits with code 2. There is deliberately no default port: the
port comes from `--port`, then `$PORT`, then `proj port`, and otherwise the
relay refuses to start.
"""

from __future__ import annotations

import argparse
import ipaddress
import shutil
import ssl
import subprocess
import unicodedata
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from urllib.parse import urlsplit

NO_PORT_MESSAGE = "no port: pass --port, set $PORT, or register with proj"
HOSTED_APP_URL = "https://sequoia-hope.github.io/waffle-iron/"
FALLBACK_AGENT_NAME = "mcp-client"
LOOPBACK_HTTP_HOSTS = ("localhost", "127.0.0.1", "::1")
DEFAULT_PORTS = {"http": 80, "https": 443}


class ConfigError(Exception):
    """A startup configuration error: the relay exits with code 2 and this message."""


@dataclass(frozen=True)
class RelayConfig:
    port: int
    bind: str
    app_url: str
    allow_origins: tuple[str, ...]
    agent_name: str | None
    open_browser: bool
    ssl_context: ssl.SSLContext | None
    public_url: str | None = None

    @property
    def relay_url(self) -> str:
        """The WebSocket address the page connects to, as carried in the pairing link."""
        if self.public_url is not None:
            return self.public_url
        return self.listen_address

    @property
    def listen_address(self) -> str:
        """The socket the relay itself binds, whatever the pairing link advertises."""
        scheme = "wss" if self.ssl_context is not None else "ws"
        host = f"[{self.bind}]" if ":" in self.bind else self.bind
        return f"{scheme}://{host}:{self.port}"


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="waffle-mcp-relay",
        description="MCP server on stdio relaying tool calls to a paired Waffle Iron browser tab.",
    )
    p.add_argument("--port", help="WebSocket port (1024-65535); else $PORT, else `proj port`")
    p.add_argument("--bind", default="127.0.0.1", help="listen address (default 127.0.0.1)")
    p.add_argument("--tls-cert", help="PEM certificate (required for a non-loopback --bind)")
    p.add_argument("--tls-key", help="PEM private key (required for a non-loopback --bind)")
    p.add_argument(
        "--public-url",
        help="wss:// address advertised in the pairing link when a TLS proxy "
        "(e.g. `tailscale serve`) fronts the loopback relay",
    )
    p.add_argument("--app-url", default=HOSTED_APP_URL, help="Waffle Iron app base URL")
    p.add_argument(
        "--allow-origin",
        action="append",
        default=None,
        help="allowed WebSocket Origin (repeatable; default: the origin of --app-url)",
    )
    p.add_argument("--agent-name", help="agent name shown on the consent screen")
    p.add_argument("--open", action="store_true", help="open the pairing link with the OS")
    return p


def parse_port(text: str) -> int:
    try:
        port = int(str(text).strip(), 10)
    except ValueError:
        raise ConfigError("invalid port") from None
    if not 1024 <= port <= 65535:
        raise ConfigError("invalid port")
    return port


def run_proj_port() -> str | None:
    """Ask the machine's port registry for this directory's port.

    An absent `proj` binary, a non-zero exit or empty output all mean
    "unresolved" (the caller then fails loudly). Its stdout is captured, so
    nothing it prints can reach the relay's JSON-RPC stdout.
    """
    exe = shutil.which("proj")
    if exe is None:
        return None
    try:
        done = subprocess.run(
            [exe, "port"],
            capture_output=True,
            text=True,
            timeout=10,
            stdin=subprocess.DEVNULL,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    out = done.stdout.strip()
    if done.returncode != 0 or not out:
        return None
    return out


def resolve_port(
    flag: str | None, env: Mapping[str, str], proj_port: Callable[[], str | None]
) -> int:
    if flag is not None:
        return parse_port(flag)
    env_port = env.get("PORT")
    if env_port:
        return parse_port(env_port)
    from_proj = proj_port()
    if from_proj is not None:
        return parse_port(from_proj)
    raise ConfigError(NO_PORT_MESSAGE)


def _origin_from_parts(scheme: str, host: str, port: int | None) -> str:
    host = host.lower()
    if ":" in host:
        host = f"[{host}]"
    if port is None or DEFAULT_PORTS.get(scheme) == port:
        return f"{scheme}://{host}"
    return f"{scheme}://{host}:{port}"


def normalize_app_url(url: str) -> str:
    try:
        parts = urlsplit(url)
        port = parts.port
    except ValueError:
        raise ConfigError("invalid app url") from None
    host = parts.hostname
    ok_scheme = parts.scheme == "https" or (parts.scheme == "http" and host in LOOPBACK_HTTP_HOSTS)
    if not host or not ok_scheme or parts.query or parts.fragment or parts.username:
        raise ConfigError("invalid app url")
    path = parts.path or "/"
    if not path.endswith("/"):
        path += "/"
    return _origin_from_parts(parts.scheme, host, port) + path


def origin_of(app_url: str) -> str:
    parts = urlsplit(app_url)
    return _origin_from_parts(parts.scheme, parts.hostname or "", parts.port)


def normalize_origin(origin: str) -> str:
    if "*" in origin:
        raise ConfigError("invalid origin")
    try:
        parts = urlsplit(origin)
        port = parts.port
    except ValueError:
        raise ConfigError("invalid origin") from None
    if (
        parts.scheme not in ("http", "https")
        or not parts.hostname
        or parts.path not in ("", "/")
        or parts.query
        or parts.fragment
        or parts.username
    ):
        raise ConfigError("invalid origin")
    return _origin_from_parts(parts.scheme, parts.hostname, port)


def normalize_public_url(url: str) -> str:
    """A proxy-fronted relay address: wss only, since the proxy terminates TLS."""
    try:
        parts = urlsplit(url)
        port = parts.port
    except ValueError:
        raise ConfigError("invalid public url") from None
    if (
        parts.scheme != "wss"
        or not parts.hostname
        or parts.query
        or parts.fragment
        or parts.username
    ):
        raise ConfigError("invalid public url")
    return _origin_from_parts("wss", parts.hostname, port) + (parts.path or "/")


def valid_agent_name(name: str) -> bool:
    return 1 <= len(name) <= 128 and not any(unicodedata.category(c) == "Cc" for c in name)


def _tls_context(cert: str | None, key: str | None) -> ssl.SSLContext | None:
    if cert is None and key is None:
        return None
    if cert is None or key is None:
        raise ConfigError("cannot read TLS material")
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    try:
        ctx.load_cert_chain(cert, key)
    except (OSError, ssl.SSLError):
        raise ConfigError("cannot read TLS material") from None
    return ctx


def build_config(
    argv: Sequence[str] | None,
    env: Mapping[str, str],
    proj_port: Callable[[], str | None] = run_proj_port,
) -> RelayConfig:
    args = build_parser().parse_args(argv)
    port = resolve_port(args.port, env, proj_port)

    try:
        bind_ip = ipaddress.ip_address(args.bind)
    except ValueError:
        raise ConfigError("invalid bind address") from None
    has_tls = args.tls_cert is not None and args.tls_key is not None
    if not bind_ip.is_loopback and not has_tls:
        raise ConfigError("non-loopback bind requires TLS")
    ssl_context = _tls_context(args.tls_cert, args.tls_key)

    public_url = None if args.public_url is None else normalize_public_url(args.public_url)
    app_url = normalize_app_url(args.app_url)
    if args.allow_origin:
        origins = tuple(dict.fromkeys(normalize_origin(o) for o in args.allow_origin))
    else:
        origins = (origin_of(app_url),)

    if args.agent_name is not None and not valid_agent_name(args.agent_name):
        raise ConfigError("invalid agent name")

    return RelayConfig(
        port=port,
        bind=str(bind_ip),
        app_url=app_url,
        allow_origins=origins,
        agent_name=args.agent_name,
        open_browser=args.open,
        ssl_context=ssl_context,
        public_url=public_url,
    )
