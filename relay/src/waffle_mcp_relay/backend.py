"""Where a page tool call goes (`specs/waffle_server_mode.md` §3.2).

The relay has one `Backend` per process: `PageBackend` forwards to the paired
browser tab over the WebSocket (`--kernel page`, the default, today's
behaviour byte for byte), `HostBackend` (`host.py`) to a `waffle-host`
child process over stdio (`--kernel host`). `server.py` sees only this
interface: the same manifest, the same schema validation, the same error
codes, plus the two host-mode codes `ViewerUnavailable` and `HostCapability`
that the host itself answers with.
"""

from __future__ import annotations

from collections.abc import Awaitable, Callable
from typing import Any, Protocol

from waffle_mcp_relay.link import LinkError, LinkServer
from waffle_mcp_relay.manifest import Manifest

ProgressCallback = Callable[[dict[str, Any]], Awaitable[None]]


class Backend(Protocol):
    """One executing engine host, whichever kind."""

    @property
    def kernel(self) -> str:
        """`page` or `host`."""

    @property
    def manifest(self) -> Manifest:
        """The tool manifest in force (a page may adopt a newer one, §2.4)."""

    def tool_names(self) -> frozenset[str] | None:
        """The page tools this backend can run, or None for "every manifest tool"."""

    async def status(self) -> dict[str, Any]:
        """The `waffle_status` answer."""

    async def call(
        self,
        tool: str,
        arguments: dict[str, Any],
        on_progress: ProgressCallback | None = None,
    ) -> dict[str, Any]:
        """Run one page tool; returns its result frame (content, structuredContent, isError)."""

    async def connect(self) -> tuple[str, float | None]:
        """`waffle_connect`: a pairing code and its expiry; LinkError when nothing can pair."""

    async def close(self) -> None: ...


class PageBackend:
    """The paired browser tab (rev-2 behaviour, unchanged)."""

    kernel = "page"

    def __init__(self, link: LinkServer) -> None:
        self._link = link

    @property
    def link(self) -> LinkServer:
        return self._link

    @property
    def manifest(self) -> Manifest:
        return self._link.manifest

    def tool_names(self) -> frozenset[str] | None:
        return None

    async def status(self) -> dict[str, Any]:
        return self._link.status()

    async def call(
        self,
        tool: str,
        arguments: dict[str, Any],
        on_progress: ProgressCallback | None = None,
    ) -> dict[str, Any]:
        return await self._link.call(tool, arguments, on_progress=on_progress)

    async def connect(self) -> tuple[str, float | None]:
        return await self._link.new_pairing()

    async def close(self) -> None:
        await self._link.close()


__all__ = ["Backend", "LinkError", "PageBackend", "ProgressCallback"]
