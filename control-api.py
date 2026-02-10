#!/usr/bin/env python3
"""Tiny HTTP API for controlling Docker services from the landing page.

Runs on the host, bound to the Tailscale IP on port 8084.
Endpoints:
  POST /restart-claude  — restart the Claude container
  POST /rebuild-claude  — full rebuild + restart
  GET  /status          — container status (JSON)
"""

import http.server
import json
import os
import subprocess
import sys
import threading

REPO_DIR = os.path.dirname(os.path.abspath(__file__))
COMPOSE = ["docker", "compose", "-f", os.path.join(REPO_DIR, "docker-compose.yml")]

# In-flight operation tracking
_lock = threading.Lock()
_running = {"op": None}


def run_compose(args, op_name):
    """Run a docker compose command, tracking it as an in-flight operation."""
    with _lock:
        if _running["op"]:
            return False, f"Already running: {_running['op']}"
        _running["op"] = op_name

    try:
        env = os.environ.copy()
        # Ensure TAILSCALE_IP is set for compose
        if "TAILSCALE_IP" not in env:
            result = subprocess.run(
                ["tailscale", "ip", "-4"],
                capture_output=True, text=True, timeout=5
            )
            if result.returncode == 0:
                env["TAILSCALE_IP"] = result.stdout.strip()
            else:
                env["TAILSCALE_IP"] = "127.0.0.1"

        proc = subprocess.run(
            COMPOSE + args,
            capture_output=True, text=True, timeout=300, env=env
        )
        return True, proc.stdout + proc.stderr
    except Exception as e:
        return False, str(e)
    finally:
        with _lock:
            _running["op"] = None


class Handler(http.server.BaseHTTPRequestHandler):
    def _cors(self):
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")

    def _json(self, code, data):
        body = json.dumps(data).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self._cors()
        self.end_headers()
        self.wfile.write(body)

    def do_OPTIONS(self):
        self.send_response(204)
        self._cors()
        self.end_headers()

    def do_GET(self):
        if self.path == "/status":
            ok, out = run_compose(
                ["ps", "--format", "json"], "status"
            )
            self._json(200, {"ok": ok, "output": out})
        else:
            self._json(404, {"error": "not found"})

    def do_POST(self):
        if self.path == "/restart-claude":
            ok, out = run_compose(
                ["restart", "claude-remote"], "restart"
            )
            self._json(200 if ok else 409, {"ok": ok, "output": out})

        elif self.path == "/rebuild-claude":
            ok, out = run_compose(
                ["up", "--build", "-d", "claude-remote"], "rebuild"
            )
            self._json(200 if ok else 409, {"ok": ok, "output": out})

        else:
            self._json(404, {"error": "not found"})

    def log_message(self, fmt, *args):
        print(f"[control-api] {args[0]}")


def main():
    bind = sys.argv[1] if len(sys.argv) > 1 else "0.0.0.0"
    port = int(sys.argv[2]) if len(sys.argv) > 2 else 8084
    server = http.server.HTTPServer((bind, port), Handler)
    print(f"[control-api] Listening on {bind}:{port}")
    server.serve_forever()


if __name__ == "__main__":
    main()
