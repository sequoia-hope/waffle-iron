#!/usr/bin/env bash
# Register the waffle-iron MCP server (the agent-link relay) with Claude Code.
#
# The relay runs on the Docker host over ssh. Nothing here hardcodes a port:
# the Tailscale name, the HTTPS port, and the app and relay loopback ports all
# come from the host's `tailscale serve` config, which must mount the app at
# `/` and the relay at `/relay` on one HTTPS port (see relay/README.md).
#
# Usage: scripts/add-waffle-mcp.sh [--scope local|user|project] [--dry-run]
# Env:   WAFFLE_HOST_SSH  ssh target (default: sequoia@<default gateway>)
set -euo pipefail

scope=local
dry_run=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --scope) scope="$2"; shift 2 ;;
    --dry-run) dry_run=1; shift ;;
    *) echo "usage: $0 [--scope local|user|project] [--dry-run]" >&2; exit 2 ;;
  esac
done

# Default gateway from /proc (the container has no `ip`): little-endian hex.
g=$(awk '$2 == "00000000" { print $3; exit }' /proc/net/route)
gateway=${g:+$((16#${g:6:2})).$((16#${g:4:2})).$((16#${g:2:2})).$((16#${g:0:2}))}
host_ssh=${WAFFLE_HOST_SSH:-sequoia@${gateway:?no default gateway; set WAFFLE_HOST_SSH}}
ssh_opts=(-T -o BatchMode=yes -o UserKnownHostsFile="$HOME/.claude/host_known_hosts")

serve_json=$(ssh "${ssh_opts[@]}" "$host_ssh" tailscale serve status --json)

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

args=(
  "${ssh_opts[@]}" "$host_ssh"
  env UV_PROJECT_ENVIRONMENT=/home/sequoia/.cache/waffle-mcp-relay-venv
  /home/sequoia/.local/bin/uv run --frozen --no-dev --no-sync
  --project /home/sequoia/Software/waffle-iron/relay
  waffle-mcp-relay
  --port "$relay_port"
  --app-url "https://$public/"
  --allow-origin "https://$public"
  --allow-origin "http://localhost:$app_port"
  --public-url "wss://$public/relay"
)

if [[ $dry_run == 1 ]]; then
  printf '%q ' claude mcp add -s "$scope" waffle-iron -- ssh "${args[@]}"; echo
  exit 0
fi

claude mcp remove -s "$scope" waffle-iron >/dev/null 2>&1 || true
claude mcp add -s "$scope" waffle-iron -- ssh "${args[@]}"
echo "Added waffle-iron (scope $scope). Reconnect it with /mcp in Claude Code."
