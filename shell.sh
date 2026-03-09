#!/usr/bin/env bash
# Session manager for Waffle Iron Claude remote container.
# Presents a menu of tmux sessions; auto-launches "default" after 2s.
set -e

CONTAINER="waffle-iron-claude"
WORKSPACE="/home/claude/workspace"
DEFAULT_SESSION="default"
CLAUDE_CMD="CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1 claude --dangerously-skip-permissions"
AUTO_TIMEOUT=2

# ── Helpers ──────────────────────────────────────────────────────────

die() { echo "  $1" >&2; exit 1; }

container_running() {
    docker inspect -f '{{.State.Running}}' "$CONTAINER" 2>/dev/null | grep -q true
}

# List tmux sessions: name|windows|attached
get_sessions() {
    docker exec "$CONTAINER" \
        tmux list-sessions -F '#{session_name}|#{session_windows}|#{session_attached}' \
        2>/dev/null || true
}

attach_session() {
    local name="$1"
    echo "  → Attaching to '$name'"
    docker exec -it "$CONTAINER" tmux attach-session -t "$name"
}

create_and_attach() {
    local name="$1"
    echo "  → Creating '$name'"
    docker exec "$CONTAINER" \
        tmux new-session -d -s "$name" -c "$WORKSPACE" "$CLAUDE_CMD; exec bash"
    docker exec -it "$CONTAINER" tmux attach-session -t "$name"
}

launch_default() {
    if docker exec "$CONTAINER" tmux has-session -t "$DEFAULT_SESSION" 2>/dev/null; then
        attach_session "$DEFAULT_SESSION"
    else
        create_and_attach "$DEFAULT_SESSION"
    fi
}

# ── Menu ─────────────────────────────────────────────────────────────

show_menu() {
    local sessions
    sessions=$(get_sessions)

    echo ""
    echo "  Waffle Iron — Claude Sessions"
    echo "  ─────────────────────────────"

    local -a names=()
    local i=1

    if [ -n "$sessions" ]; then
        while IFS='|' read -r name windows attached; do
            local label="$name"
            [ "$name" = "$DEFAULT_SESSION" ] && label="$label *"
            [ "$attached" -gt 0 ] 2>/dev/null && label="$label (attached)"
            echo "  [$i] $label"
            names+=("$name")
            i=$((i + 1))
        done <<< "$sessions"
    fi

    echo "  [n] New session"
    echo "  [q] Exit"
    echo ""
    echo -n "  Select [default in ${AUTO_TIMEOUT}s]: "

    local choice
    if ! read -t "$AUTO_TIMEOUT" -r choice; then
        echo ""
        launch_default
        return
    fi

    case "$choice" in
        q|Q)
            echo "  Bye."
            exit 0
            ;;
        n|N)
            echo -n "  Session name: "
            read -r newname
            [ -z "$newname" ] && die "No name given."
            # Sanitise for tmux (letters, digits, hyphens, underscores)
            newname="${newname//[^a-zA-Z0-9_-]/}"
            [ -z "$newname" ] && die "Invalid name."
            if docker exec "$CONTAINER" tmux has-session -t "$newname" 2>/dev/null; then
                attach_session "$newname"
            else
                create_and_attach "$newname"
            fi
            ;;
        "")
            launch_default
            ;;
        *[0-9]*)
            local idx=$((choice - 1))
            if [ "$idx" -ge 0 ] && [ "$idx" -lt "${#names[@]}" ]; then
                attach_session "${names[$idx]}"
            else
                die "Invalid selection."
            fi
            ;;
        *)
            die "Invalid selection."
            ;;
    esac
}

# ── Main ─────────────────────────────────────────────────────────────

container_running || die "Container '$CONTAINER' not running. Start with: ./claude-remote/run.sh"
show_menu
