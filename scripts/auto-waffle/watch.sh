#!/usr/bin/env bash
#
# auto-waffle watch — tail the current iteration's output with readable formatting
#
# Usage:
#   ./scripts/auto-waffle/watch.sh           # auto-find latest iteration
#   ./scripts/auto-waffle/watch.sh raw       # raw JSON stream
#   ./scripts/auto-waffle/watch.sh plan      # show current plan
#   ./scripts/auto-waffle/watch.sh extend 30 # add 30 minutes to timeout
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOG_DIR="$SCRIPT_DIR/logs"

# Find the most recently modified iteration directory
latest_dir=$(ls -dt "$LOG_DIR"/*/ 2>/dev/null | head -1)

if [[ -z "$latest_dir" ]]; then
    echo "No auto-waffle iterations found in $LOG_DIR"
    exit 1
fi

log_file="$latest_dir/output.log"
plan_file="$latest_dir/plan.md"
extend_file="$latest_dir/extend"

case "${1:-}" in
    raw)
        echo "=== Tailing $log_file (raw JSON) ==="
        tail -f "$log_file"
        ;;
    plan)
        if [[ -f "$plan_file" ]]; then
            cat "$plan_file"
        else
            echo "No plan file yet in $latest_dir"
        fi
        ;;
    extend)
        mins="${2:-30}"
        echo "$mins" > "$extend_file"
        echo "Extended timeout by ${mins} minutes"
        ;;
    status)
        echo "=== Iteration: $(basename "$latest_dir") ==="
        if [[ -f "$latest_dir/meta.json" ]]; then
            cat "$latest_dir/meta.json"
        else
            echo "Still running..."
        fi
        echo ""
        echo "=== Plan ==="
        head -5 "$plan_file" 2>/dev/null || echo "(no plan yet)"
        echo ""
        echo "=== Output size ==="
        du -h "$log_file" 2>/dev/null
        echo ""
        echo "=== Tool usage ==="
        grep -o '"name":"[^"]*"' "$log_file" 2>/dev/null | sort | uniq -c | sort -rn | head -10
        ;;
    *)
        echo "=== Tailing $log_file (text only) ==="
        echo "=== Ctrl-C to stop ==="
        echo ""
        tail -f "$log_file" | python3 -c "
import sys, json
for line in sys.stdin:
    try:
        d = json.loads(line.strip())
        if d.get('type') == 'assistant':
            for c in d.get('message',{}).get('content',[]):
                if c.get('type') == 'text':
                    print(c['text'], end='', flush=True)
                elif c.get('type') == 'tool_use':
                    print(f\"\n> {c['name']}({c.get('input',{}).get('description','')[:80]})\", flush=True)
    except: pass
" 2>/dev/null
        ;;
esac
