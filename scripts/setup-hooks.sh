#!/usr/bin/env bash
# Point git at the versioned hooks in .githooks/ (one-time, per clone).
set -e
cd "$(git rev-parse --show-toplevel)"
git config core.hooksPath .githooks
chmod +x .githooks/* 2>/dev/null || true
echo "git hooks enabled: core.hooksPath -> .githooks"
