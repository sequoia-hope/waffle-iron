#!/usr/bin/env bash
# Register the waffle-iron MCP server (the agent-link relay) with Claude Code.
#
# The relay runs on this machine, from the checkout's own uv project (the
# agents moved out of Docker on 2026-11-12, so there is no ssh hop any more).
# Nothing here hardcodes a port: the Tailscale name, the HTTPS port, and the
# app and relay loopback ports all come from this machine's `tailscale serve`
# config, which must mount the app at `/` and the relay at `/relay` on one
# HTTPS port (see relay/README.md).
#
# Usage: scripts/add-waffle-mcp.sh [--scope local|user|project] [--persistent-link] [--dry-run]
#   --persistent-link  development: the pairing link is reusable and never
#                      expires (its code lives in
#                      ~/.local/state/waffle-mcp-relay/link-<port>.code)
set -euo pipefail

scope=local
dry_run=0
persistent=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --scope) scope="$2"; shift 2 ;;
    --persistent-link) persistent=1; shift ;;
    --dry-run) dry_run=1; shift ;;
    *) echo "usage: $0 [--scope local|user|project] [--persistent-link] [--dry-run]" >&2; exit 2 ;;
  esac
done

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
uv_bin="$(command -v uv || true)"
[[ -z $uv_bin && -x $HOME/.local/bin/uv ]] && uv_bin="$HOME/.local/bin/uv"
: "${uv_bin:?uv not found (https://docs.astral.sh/uv/)}"

serve_json=$(tailscale serve status --json)

# Emits: <https host:port> <app loopback port> <relay loopback port>
read -r public app_port relay_port < <(python3 -c '
import json, sys
from urllib.parse import urlsplit
for hostport, web in json.load(sys.stdin).get("Web", {}).items():
    h = web.get("Handlers", {})
    app, relay = h.get("/", {}).get("Proxy"), h.get("/relay", {}).get("Proxy")
    if app and relay:
        print(hostport, urlsplit(app).port, urlsplit(relay).port)
        break
else:
    sys.exit("no tailscale serve entry mounts both / and /relay on the host")
' <<<"$serve_json")

# --frozen --no-dev: the lockfile's runtime set, synced into relay/.venv on
# first start.
args=(
  "$uv_bin" run --frozen --no-dev
  --project "$repo/relay"
  waffle-mcp-relay
  --port "$relay_port"
  --app-url "https://$public/"
  --allow-origin "https://$public"
  --allow-origin "http://localhost:$app_port"
  --public-url "wss://$public/relay"
)
if [[ $persistent == 1 ]]; then
  args+=(--persistent-link)
fi

if [[ $dry_run == 1 ]]; then
  printf '%q ' claude mcp add -s "$scope" waffle-iron -- "${args[@]}"; echo
  exit 0
fi

claude mcp remove -s "$scope" waffle-iron >/dev/null 2>&1 || true
claude mcp add -s "$scope" waffle-iron -- "${args[@]}"
echo "Added waffle-iron (scope $scope). Reconnect it with /mcp in Claude Code."
