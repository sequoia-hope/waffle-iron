#!/usr/bin/env bash
# opencode.sh — run opencode against this tree in a host tmux session
# ("waffle-iron-opencode"), alongside launch.sh's Claude session. Reconnects if
# the session already exists; -r forces a restart.
set -euo pipefail

SESSION="waffle-iron-opencode"
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PATH="$HOME/.opencode/bin:$HOME/.local/bin:$PATH"

if [[ "${1:-}" == "-r" ]] && tmux has-session -t "=$SESSION" 2>/dev/null; then
  tmux kill-session -t "=$SESSION"
fi

if ! tmux has-session -t "=$SESSION" 2>/dev/null; then
  tmux new-session -d -s "$SESSION" -c "$DIR" "opencode; exec bash"
  echo "Created tmux session '$SESSION' in $DIR"
fi

if [[ -n "${TMUX:-}" ]]; then
  exec tmux switch-client -t "=$SESSION"
else
  exec tmux attach-session -t "=$SESSION"
fi
