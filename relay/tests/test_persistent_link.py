"""P17/P18 end to end: a relay started with --persistent-link, driven over stdio."""

from __future__ import annotations

from pathlib import Path

from support import APP_URL, FakePage, StdioRelay, allocate_test_port, pairing_params

from waffle_mcp_relay.manifest import load_bundled


async def test_p17_persistent_link_is_stable_logged_and_reusable(tmp_path: Path) -> None:
    port = allocate_test_port()
    code_file = tmp_path / "link.code"
    proc = await StdioRelay.start(
        "--port", str(port), "--app-url", APP_URL, "--persistent-link", str(code_file)
    )
    try:
        await proc.wait_for_stderr("persistent pairing link")
        await proc.initialize("pytest-agent")
        first = await proc.call_tool("waffle_connect")
        second = await proc.call_tool("waffle_connect")
        assert first["structuredContent"]["expires_at"] is None
        assert (
            first["structuredContent"]["pairing_url"] == second["structuredContent"]["pairing_url"]
        )
        assert "does not expire" in first["content"][0]["text"]
        params = pairing_params(first["structuredContent"]["pairing_url"])
        assert params["code"] == code_file.read_text().strip()

        for _ in range(2):  # the same link pairs again after the page is gone
            page = await FakePage.connect(params["relay"])
            await page.hello(code=params["code"], manifest_hash=load_bundled().hash)
            await page.recv_type("welcome")
            await page.close()
    finally:
        assert await proc.close() == 0
