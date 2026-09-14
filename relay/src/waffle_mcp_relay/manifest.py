"""The page tool manifest (spec §2.4).

The manifest is generated from `app/src/lib/agent/tools/` by
`app/scripts/gen-agent-manifest.mjs`. Its hash is SHA-256 over the canonical
JSON of the `tools` array (keys sorted, no whitespace, non-ASCII kept); the
page computes the same hash in `app/src/lib/agent/tools/manifest.js`.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from importlib import resources
from typing import Any

MANIFEST_FILE = "agent-tools.manifest.json"


def canonical_json(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def manifest_hash(tools: list[dict[str, Any]]) -> str:
    return hashlib.sha256(canonical_json(tools).encode("utf-8")).hexdigest()


class ManifestError(Exception):
    pass


def validate_tools(tools: object) -> list[dict[str, Any]]:
    if not isinstance(tools, list):
        raise ManifestError("tools must be a list")
    names: set[str] = set()
    for tool in tools:
        if not isinstance(tool, dict):
            raise ManifestError("each tool must be an object")
        name = tool.get("name")
        if not isinstance(name, str) or not name:
            raise ManifestError("each tool needs a name")
        if name in names:
            raise ManifestError(f"duplicate tool {name}")
        if not isinstance(tool.get("inputSchema"), dict):
            raise ManifestError(f"tool {name} needs an inputSchema object")
        names.add(name)
    return tools


@dataclass(frozen=True)
class Manifest:
    tools: list[dict[str, Any]]
    hash: str

    @classmethod
    def from_tools(cls, tools: object) -> Manifest:
        checked = validate_tools(tools)
        return cls(tools=checked, hash=manifest_hash(checked))

    @property
    def names(self) -> frozenset[str]:
        return frozenset(t["name"] for t in self.tools)


def load_bundled() -> Manifest:
    text = resources.files("waffle_mcp_relay").joinpath(MANIFEST_FILE).read_text("utf-8")
    data = json.loads(text)
    manifest = Manifest.from_tools(data.get("tools"))
    if data.get("manifest_hash") != manifest.hash:
        raise ManifestError("bundled manifest_hash does not match its tools")
    return manifest
