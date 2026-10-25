#!/bin/bash
set -e

cd "$(dirname "$0")"

# Load .env if it exists
if [ -f .env ]; then
    set -a
    source .env
    set +a
fi

# Auto-detect Tailscale IP if not set
if [ -z "$TAILSCALE_IP" ]; then
    TAILSCALE_IP=$(tailscale ip -4 2>/dev/null || true)
    if [ -z "$TAILSCALE_IP" ]; then
        echo "ERROR: Could not detect Tailscale IP."
        echo "       Make sure Tailscale is running, or set TAILSCALE_IP in .env"
        exit 1
    fi
    export TAILSCALE_IP
    echo "Detected Tailscale IP: $TAILSCALE_IP"
fi

# Extract GitHub token from host's gh CLI (stored in OS keychain, not in config files)
if [ -z "$GH_TOKEN" ] && command -v gh &>/dev/null; then
    GH_TOKEN=$(gh auth token 2>/dev/null || true)
    if [ -n "$GH_TOKEN" ]; then
        export GH_TOKEN
        echo "Extracted GitHub token from gh CLI"
    else
        echo "WARNING: Could not extract GitHub token. Run 'gh auth login' on the host first."
    fi
fi

# Extract Antigravity OAuth token from host keyring so container file-based
# token storage (~/.gemini/antigravity-cli/antigravity-oauth-token) stays in sync
if [ -d "$HOME/.gemini/antigravity-cli" ]; then
    python3 -c '
import os, secretstorage
try:
    bus = secretstorage.dbus_init()
    col = secretstorage.get_default_collection(bus)
    for item in col.get_all_items():
        a = item.get_attributes()
        if a.get("service") == "gemini" and a.get("username") == "antigravity":
            p = os.path.expanduser("~/.gemini/antigravity-cli/antigravity-oauth-token")
            with open(p, "wb") as f:
                f.write(item.get_secret())
            os.chmod(p, 0o600)
            break
except Exception:
    pass
' 2>/dev/null || true
fi

echo "Starting Docker services..."

if [[ "${1:-}" == "--build" ]] || ! docker image inspect waffle-iron-claude-remote >/dev/null 2>&1; then
    docker compose up --build -d
else
    docker compose up -d
fi

echo ""
echo "Docker services running:"
echo "  Landing page:       http://${TAILSCALE_IP}:8080"
echo "  Claude Code:        http://${TAILSCALE_IP}:8082"
echo "  Waffle Iron app:    http://${TAILSCALE_IP}:8083"
echo ""
echo "Commands:"
echo "  Logs:          docker compose logs -f"
echo "  Stop:          ./stop.sh"
echo "  Attach local:  docker exec -it waffle-iron-claude tmux attach -t ${SESSION_NAME:-waffle-iron}"
