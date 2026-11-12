#!/usr/bin/env bash
# serve.sh — run the Waffle Iron GUI dev server (Vite) on its registered port.
#
# The number lives in the registry (~/scripts/launcher.toml), never in this
# file: --port, then $PORT when the hub started us, then `proj port`, which
# resolves this directory to the project registered for it. There is
# deliberately no fallback default: a project that cannot find its port should
# say so rather than quietly bind whatever looked free.
#
# Binds 0.0.0.0, so the app answers on loopback (the agent-link relay only
# pairs with an https or loopback app URL) and on the Tailscale address.
#
# Playwright is unaffected: its webServer starts its own `npm run dev` on
# app/vite.config.js's port when nothing answers there, so test runs never
# touch this instance.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

if [[ "${1:-}" == "--port" && -n "${2:-}" ]]; then
  PORT="$2"
fi
if [[ -z "${PORT:-}" ]]; then
  PROJ="$(command -v proj || echo "$HOME/scripts/proj")"
  PORT="$("$PROJ" port)" || { echo "serve.sh: no port (pass --port, set \$PORT, or register with proj)" >&2; exit 1; }
fi

cd app
# The dependencies are shared with whatever last installed them; make sure
# they match the lockfile before serving.
npm install --no-audit --no-fund
exec npx vite dev --host 0.0.0.0 --port "$PORT" --strictPort
