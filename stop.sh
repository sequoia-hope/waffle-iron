#!/bin/bash

cd "$(dirname "$0")"

# Need TAILSCALE_IP set for compose to parse the file
if [ -f .env ]; then
    set -a
    source .env
    set +a
fi
if [ -z "$TAILSCALE_IP" ]; then
    export TAILSCALE_IP=$(tailscale ip -4 2>/dev/null || echo "127.0.0.1")
fi

# Stop host control terminal
PID_FILE="/tmp/waffle-iron-control-ttyd.pid"
if [ -f "$PID_FILE" ]; then
    pid=$(cat "$PID_FILE")
    if kill -0 "$pid" 2>/dev/null; then
        echo "Stopping control terminal (PID $pid)..."
        kill "$pid" 2>/dev/null || true
    fi
    rm -f "$PID_FILE"
fi

# Stop control API
API_PID_FILE="/tmp/waffle-iron-control-api.pid"
if [ -f "$API_PID_FILE" ]; then
    pid=$(cat "$API_PID_FILE")
    if kill -0 "$pid" 2>/dev/null; then
        echo "Stopping control API (PID $pid)..."
        kill "$pid" 2>/dev/null || true
    fi
    rm -f "$API_PID_FILE"
fi

# Stop Docker services
docker compose down
echo "All services stopped."
