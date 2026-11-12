#!/usr/bin/env bash
# launch.sh — launch claude or agy in a host tmux session in this directory.
# Reconnects if the session already exists. Use -r to force restart,
# and pass 'claude' (default) or 'agy' (or -a claude|agy) to select the agent.
#
# Replaces shell.sh, which ran the agents inside the waffle-iron-claude Docker
# container (see HOST-MOVE-CHECKLIST.md for why the agents moved to the host).
set -euo pipefail

SESSION="waffle-iron"
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLAUDE_CMD="CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1 claude --dangerously-skip-permissions"
AGY_CMD="agy --dangerously-skip-permissions"
AGENT="claude"
FORCE_RESTART=false

while getopts "ra:" opt; do
  case "$opt" in
    r) FORCE_RESTART=true ;;
    a) AGENT="$OPTARG" ;;
    *) echo "Usage: $0 [-r] [-a claude|agy] [claude|agy]"; exit 1 ;;
  esac
done
shift $((OPTIND - 1))
if [[ $# -gt 0 ]]; then
  AGENT="$1"
fi

case "$AGENT" in
  claude)
    AGENT_CMD="$CLAUDE_CMD"
    ;;
  agy)
    SESSION="${SESSION}-agy"
    AGENT_CMD="$AGY_CMD"
    ;;
  *)
    echo "Unknown agent '$AGENT' (expected: claude or agy)" >&2
    exit 1
    ;;
esac

# `=` means an exact session name: a bare target is a prefix match, and would
# find waffle-iron-opencode when waffle-iron itself is not running.
if $FORCE_RESTART && tmux has-session -t "=$SESSION" 2>/dev/null; then
  tmux kill-session -t "=$SESSION"
fi

# `exec bash` keeps the pane alive if the agent exits.
if ! tmux has-session -t "=$SESSION" 2>/dev/null; then
  tmux new-session -d -s "$SESSION" -c "$DIR" "$AGENT_CMD; exec bash"
  echo "Created tmux session '$SESSION' in $DIR"
fi

if [[ -n "${TMUX:-}" ]]; then
  exec tmux switch-client -t "=$SESSION"
else
  exec tmux attach-session -t "=$SESSION"
fi
