"""Test doubles: the real relay subprocess driven over stdio, and a fake page WebSocket client."""

from __future__ import annotations

import asyncio
import json
import os
import socket
import sys
import time
from collections.abc import Callable
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, urlsplit

from websockets.asyncio.client import ClientConnection, connect

from waffle_mcp_relay import PROTOCOL

RELAY_DIR = Path(__file__).resolve().parents[1]
REPO_ROOT = RELAY_DIR.parent
APP_ORIGIN = "https://app.example"
APP_URL = f"{APP_ORIGIN}/"


class FakeClock:
    def __init__(self, start: float = 1_800_000_000.0) -> None:
        self.now = start

    def __call__(self) -> float:
        return self.now

    def advance(self, seconds: float) -> None:
        self.now += seconds


def allocate_test_port() -> int:
    # Test-fixture allocation, passed to the relay EXPLICITLY via --port / LinkServer.start;
    # the relay itself never picks a port.
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def relay_env(**overrides: str) -> dict[str, str]:
    env = dict(os.environ)
    env.pop("PORT", None)
    env.update(overrides)
    return env


async def wait_until(predicate: Callable[[], bool], timeout: float = 5.0) -> None:
    deadline = time.monotonic() + timeout
    while not predicate():
        if time.monotonic() > deadline:
            raise AssertionError("condition not reached before timeout")
        await asyncio.sleep(0.01)


def pairing_params(url: str) -> dict[str, str]:
    return {k: v[0] for k, v in parse_qs(urlsplit(url).query).items()}


class StdioRelay:
    """The real `waffle-mcp-relay` process, spoken to as an MCP client over stdio."""

    def __init__(self, proc: asyncio.subprocess.Process) -> None:
        self.proc = proc
        self.stdout_lines: list[bytes] = []
        self.unparseable: list[bytes] = []
        self.notifications: list[dict[str, Any]] = []
        self.stderr = bytearray()
        self._responses: dict[int, asyncio.Future[dict[str, Any]]] = {}
        self._next_id = 0
        self._tasks: list[asyncio.Task[None]] = []

    @classmethod
    async def start(cls, *args: str, env: dict[str, str] | None = None) -> StdioRelay:
        proc = await asyncio.create_subprocess_exec(
            sys.executable,
            "-m",
            "waffle_mcp_relay",
            *args,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=env if env is not None else relay_env(),
        )
        relay = cls(proc)
        relay._tasks.append(asyncio.create_task(relay._read_stdout()))
        relay._tasks.append(asyncio.create_task(relay._read_stderr()))
        return relay

    async def _read_stdout(self) -> None:
        assert self.proc.stdout is not None
        while line := await self.proc.stdout.readline():
            self.stdout_lines.append(line)
            try:
                msg = json.loads(line)
            except ValueError:
                self.unparseable.append(line)
                continue
            if not isinstance(msg, dict):
                self.unparseable.append(line)
                continue
            msg_id = msg.get("id")
            if msg_id in self._responses and ("result" in msg or "error" in msg):
                future = self._responses.pop(msg_id)
                if not future.done():
                    future.set_result(msg)
            else:
                self.notifications.append(msg)

    async def _read_stderr(self) -> None:
        assert self.proc.stderr is not None
        while chunk := await self.proc.stderr.read(4096):
            self.stderr += chunk

    async def wait_for_stderr(self, needle: str, timeout: float = 20.0) -> None:
        await wait_until(lambda: needle in self.stderr.decode(errors="replace"), timeout)

    async def send(self, message: dict[str, Any]) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.write((json.dumps(message) + "\n").encode())
        await self.proc.stdin.drain()

    async def request(
        self, method: str, params: dict[str, Any] | None = None, timeout: float = 20.0
    ) -> dict[str, Any]:
        self._next_id += 1
        msg_id = self._next_id
        future: asyncio.Future[dict[str, Any]] = asyncio.get_running_loop().create_future()
        self._responses[msg_id] = future
        message: dict[str, Any] = {"jsonrpc": "2.0", "id": msg_id, "method": method}
        if params is not None:
            message["params"] = params
        await self.send(message)
        return await asyncio.wait_for(future, timeout)

    async def initialize(self, client_name: str = "pytest-agent") -> dict[str, Any]:
        response = await self.request(
            "initialize",
            {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": client_name, "version": "0"},
            },
        )
        await self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        return response

    async def call_tool(self, name: str, arguments: dict[str, Any] | None = None) -> dict[str, Any]:
        response = await self.request("tools/call", {"name": name, "arguments": arguments or {}})
        assert "result" in response, response
        return response["result"]

    async def close(self, timeout: float = 20.0) -> int:
        if self.proc.stdin is not None and not self.proc.stdin.is_closing():
            self.proc.stdin.close()
        try:
            code = await asyncio.wait_for(self.proc.wait(), timeout)
        finally:
            if self.proc.returncode is None:
                self.proc.kill()
                await self.proc.wait()
        await asyncio.gather(*self._tasks, return_exceptions=True)
        return code


class FakePage:
    """A page speaking `waffle-agent-link/1`; records every frame it receives."""

    def __init__(self, ws: ClientConnection) -> None:
        self.ws = ws
        self.frames: list[dict[str, Any]] = []

    @classmethod
    async def connect(cls, relay_url: str, origin: str | None = APP_ORIGIN) -> FakePage:
        ws = await connect(relay_url, origin=origin, ping_interval=None)  # type: ignore[arg-type]
        return cls(ws)

    async def send(self, frame: dict[str, Any]) -> None:
        await self.ws.send(json.dumps(frame))

    async def hello(self, **fields: Any) -> None:
        frame: dict[str, Any] = {"type": "hello", "protocol": PROTOCOL, "app_build": None}
        frame.update(fields)
        await self.send(frame)

    async def recv(self, timeout: float = 5.0) -> dict[str, Any]:
        frame = json.loads(await asyncio.wait_for(self.ws.recv(), timeout))
        self.frames.append(frame)
        return frame

    async def recv_type(self, kind: str, timeout: float = 5.0) -> dict[str, Any]:
        """Next frame of `kind`, answering relay pings on the way."""
        while True:
            frame = await self.recv(timeout)
            if frame.get("type") == "ping" and kind != "ping":
                await self.send({"type": "pong"})
                continue
            return frame if frame.get("type") == kind else _unexpected(kind, frame)

    async def close(self) -> None:
        await self.ws.close()


def _unexpected(kind: str, frame: dict[str, Any]) -> dict[str, Any]:
    raise AssertionError(f"expected a {kind!r} frame, got {frame!r}")
