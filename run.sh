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

# Extract GitHub token from host's gh CLI
if [ -z "$GH_TOKEN" ] && command -v gh &>/dev/null; then
    GH_TOKEN=$(gh auth token 2>/dev/null || true)
    if [ -n "$GH_TOKEN" ]; then
        export GH_TOKEN
        echo "Extracted GitHub token from gh CLI"
    fi
fi

echo "Starting waffle-iron dev container..."
echo "  Bind address: ${TAILSCALE_IP}:7681"
echo ""

docker compose up --build -d

echo ""
echo "Container running. Access at:"
echo "  http://${TAILSCALE_IP}:7681"
echo ""
echo "Commands:"
echo "  Logs:          docker compose logs -f"
echo "  Stop:          docker compose down"
echo "  Attach local:  docker exec -it waffle-iron-claude tmux attach -t ${SESSION_NAME:-waffle-iron}"
