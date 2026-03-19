#!/usr/bin/env bash
#
# auto-waffle watch — monitor a running auto-waffle session from the host.
# Runs via docker exec into the waffle-iron container.
#
# Usage:
#   ./watch.sh              — readable text stream (default)
#   ./watch.sh raw          — raw JSON stream
#   ./watch.sh plan         — show current plan
#   ./watch.sh status       — iteration info + tool usage summary
#   ./watch.sh extend 30    — add 30 minutes to timeout
#

set -e

CONTAINER="waffle-iron-claude"
WATCH_SCRIPT="/home/claude/workspace/scripts/auto-waffle/watch.sh"

die() { echo "  $1" >&2; exit 1; }

container_running() {
    docker inspect -f '{{.State.Running}}' "$CONTAINER" 2>/dev/null | grep -q true
}

container_running || die "Container '$CONTAINER' not running. Start with: ./claude-remote/run.sh"

# Pass all arguments through to the container's watch.sh
if [ $# -eq 0 ]; then
    # Default: readable text stream (interactive, needs -it for ctrl-c)
    docker exec -it "$CONTAINER" bash "$WATCH_SCRIPT"
else
    docker exec -it "$CONTAINER" bash "$WATCH_SCRIPT" "$@"
fi
