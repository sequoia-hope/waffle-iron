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
fi

echo "Tailscale IP: $TAILSCALE_IP"

# --- Host ttyd (control terminal on 8081) ---
PID_FILE="/tmp/waffle-iron-control-ttyd.pid"

# Kill any existing control ttyd
if [ -f "$PID_FILE" ]; then
    old_pid=$(cat "$PID_FILE")
    if kill -0 "$old_pid" 2>/dev/null; then
        echo "Stopping previous control terminal (PID $old_pid)..."
        kill "$old_pid" 2>/dev/null || true
        sleep 0.5
    fi
    rm -f "$PID_FILE"
fi

# Check ttyd is installed on host
if ! command -v ttyd &>/dev/null; then
    echo "ERROR: ttyd not found on host. Install it first:"
    echo "  brew install ttyd   # macOS"
    echo "  sudo apt install ttyd  # Debian/Ubuntu"
    exit 1
fi

# Start host ttyd on port 8081
REPO_DIR="$(pwd)"
ttyd -p 8081 -i "$TAILSCALE_IP" bash -c "cd '$REPO_DIR' && exec bash" &
HOST_TTYD_PID=$!
echo "$HOST_TTYD_PID" > "$PID_FILE"
echo "Control terminal started (PID $HOST_TTYD_PID) on :8081"

# --- Docker services ---
./run.sh

echo ""
echo "=== All services running ==="
echo "  Landing page:       http://${TAILSCALE_IP}:8080"
echo "  Control terminal:   http://${TAILSCALE_IP}:8081"
echo "  Claude Code:        http://${TAILSCALE_IP}:8082"
echo "  Waffle Iron app:    http://${TAILSCALE_IP}:8083"
echo ""
echo "  Stop everything:    ./stop.sh"
echo "  Restart Docker:     ./run.sh  (from control terminal)"
