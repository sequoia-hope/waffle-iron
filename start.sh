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

# Free a TCP port by terminating whatever process actually listens on it.
# PID files alone are unreliable: a later run can overwrite the file while the
# original port-holder keeps running, leaving the file pointing at a dead PID
# and the real listener orphaned (which then causes "Address already in use").
# This kills the actual owner of the port, independent of any PID file.
free_port() {
    local port="$1" label="$2" pids
    # ss -H: no header; -t: tcp; -l: listening; -n: numeric; -p: process info.
    pids=$(ss -Htlnp "sport = :$port" 2>/dev/null \
        | grep -oE 'pid=[0-9]+' | grep -oE '[0-9]+' | sort -u)
    if [ -n "$pids" ]; then
        echo "Freeing port $port (${label}); killing PID(s): $pids"
        # shellcheck disable=SC2086
        kill $pids 2>/dev/null || true
        sleep 0.5
        # Escalate to SIGKILL for any survivors.
        pids=$(ss -Htlnp "sport = :$port" 2>/dev/null \
            | grep -oE 'pid=[0-9]+' | grep -oE '[0-9]+' | sort -u)
        if [ -n "$pids" ]; then
            echo "  port $port still held; sending SIGKILL to: $pids"
            # shellcheck disable=SC2086
            kill -9 $pids 2>/dev/null || true
            sleep 0.5
        fi
    fi
}

# --- Host ttyd (control terminal on 8081) ---
PID_FILE="/tmp/waffle-iron-control-ttyd.pid"

# Kill any existing control ttyd (PID file first, then whatever owns the port).
if [ -f "$PID_FILE" ]; then
    old_pid=$(cat "$PID_FILE")
    if kill -0 "$old_pid" 2>/dev/null; then
        echo "Stopping previous control terminal (PID $old_pid)..."
        kill "$old_pid" 2>/dev/null || true
        sleep 0.5
    fi
    rm -f "$PID_FILE"
fi
free_port 8081 "control terminal"

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

# --- Control API (restart buttons on landing page) ---
API_PID_FILE="/tmp/waffle-iron-control-api.pid"

if [ -f "$API_PID_FILE" ]; then
    old_pid=$(cat "$API_PID_FILE")
    if kill -0 "$old_pid" 2>/dev/null; then
        kill "$old_pid" 2>/dev/null || true
        sleep 0.3
    fi
    rm -f "$API_PID_FILE"
fi
free_port 8084 "control API"

python3 "$REPO_DIR/control-api.py" "$TAILSCALE_IP" 8084 &
API_PID=$!
echo "$API_PID" > "$API_PID_FILE"
echo "Control API started (PID $API_PID) on :8084"

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
