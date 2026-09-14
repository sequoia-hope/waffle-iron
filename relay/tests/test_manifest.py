"""§2.4 tool manifest: the committed copy is the app's current generation, hashed identically."""

from __future__ import annotations

import json
import shutil
import subprocess

import pytest
from support import REPO_ROOT

from waffle_mcp_relay.manifest import (
    MANIFEST_FILE,
    Manifest,
    ManifestError,
    canonical_json,
    load_bundled,
    manifest_hash,
)

COMMITTED = REPO_ROOT / "relay" / "src" / "waffle_mcp_relay" / MANIFEST_FILE
GENERATOR = REPO_ROOT / "app" / "scripts" / "gen-agent-manifest.mjs"


def test_committed_manifest_equals_fresh_generation() -> None:
    node = shutil.which("node")
    if node is None:
        pytest.fail("node is required to regenerate the manifest from app/src/lib/agent/tools/")
    fresh = subprocess.run(
        [node, str(GENERATOR), "--stdout"], capture_output=True, check=True, timeout=60
    ).stdout
    assert COMMITTED.read_bytes() == fresh, (
        "agent-tools.manifest.json is stale: run `node app/scripts/gen-agent-manifest.mjs`"
    )


def test_js_hash_equals_python_hash() -> None:
    data = json.loads(COMMITTED.read_text("utf-8"))
    # manifest_hash in the file was computed by the JS generator.
    assert data["manifest_hash"] == manifest_hash(data["tools"])
    assert load_bundled().hash == data["manifest_hash"]


def test_bundled_manifest_has_model_summary() -> None:
    manifest = load_bundled()
    assert "model_summary" in manifest.names
    tool = next(t for t in manifest.tools if t["name"] == "model_summary")
    assert tool["annotations"]["readOnlyHint"] is True


GOLDEN = REPO_ROOT / "docs" / "schema" / "waffle-v5.schema.json"


def _refs(node: object) -> list[str]:
    if isinstance(node, dict):
        found = [node["$ref"]] if isinstance(node.get("$ref"), str) else []
        return found + [r for v in node.values() for r in _refs(v)]
    if isinstance(node, list):
        return [r for v in node for r in _refs(v)]
    return []


def test_o19_engine_schemas_resolve_and_equal_the_golden() -> None:
    # O19: every engine-type $ref resolves inside the tool's own $defs, and every
    # embedded definition is byte-equal (canonical JSON) to the CI-pinned golden.
    golden = json.loads(GOLDEN.read_text("utf-8"))["$defs"]
    tools = load_bundled().tools
    embedding = [t for t in tools if "$defs" in t["inputSchema"]]
    assert {t["name"] for t in embedding} >= {"feature_add", "feature_edit", "sketch_create"}
    for tool in tools:
        schema = tool["inputSchema"]
        defs = schema.get("$defs", {})
        for ref in _refs(schema):
            assert ref.startswith("#/$defs/"), (tool["name"], ref)
            assert ref.removeprefix("#/$defs/") in defs, (tool["name"], ref)
        for name, definition in defs.items():
            # Value equality, not text: the golden writes 0.0 where the JS generator writes 0.
            assert definition == golden[name], (tool["name"], name)


def test_canonical_json_form() -> None:
    assert (
        canonical_json({"b": 1, "a": [{"d": "é", "c": None}]}) == '{"a":[{"c":null,"d":"é"}],"b":1}'
    )


def test_invalid_tools_rejected() -> None:
    with pytest.raises(ManifestError):
        Manifest.from_tools([{"name": "x"}])
    with pytest.raises(ManifestError):
        Manifest.from_tools([{"name": "x", "inputSchema": {}}, {"name": "x", "inputSchema": {}}])
