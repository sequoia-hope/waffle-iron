#!/usr/bin/env bash
# PreToolUse hook (Bash matcher): before any `git push`, surface the latest CI
# runs so red CI is seen before more work lands on top of it. The status lags
# one push by design (it reflects runs from BEFORE the push about to happen).
# Non-blocking: always exits 0; the push proceeds regardless.

payload=$(cat)
case "$payload" in
  *"git push"*) ;;
  *) exit 0 ;;
esac

runs=$(gh run list --limit 5 2>&1) || runs="(gh run list failed: $runs)"

python3 - "$runs" <<'PY'
import json, sys
print(json.dumps({"hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "additionalContext": (
        "CI status before this push (lags one push: these runs are from BEFORE it). "
        "If the latest runs are red/failed, investigate before continuing to build on top:\n"
        + sys.argv[1]
    ),
}}))
PY
