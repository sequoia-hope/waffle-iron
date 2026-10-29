#!/usr/bin/env bash
# Kernel-stack content fingerprint, shared by `scripts/test.sh` (which records
# a passing rewrite tier against it) and `.githooks/pre-push` (which requires
# one before kernel changes reach main). Remediation plan G1 / decision D-1,
# docs/review/remediation_2026-10-28.md.
#
#   kernel_fingerprint.sh worktree   fingerprint of the working tree as it is
#                                    now (tracked + untracked, ignoring
#                                    .gitignore'd files) — what a test run saw
#   kernel_fingerprint.sh <commit>   fingerprint of a commit's content
#   kernel_fingerprint.sh touches <from> <to>
#                                    exit 0 if <from>..<to> changes a kernel path
#   kernel_fingerprint.sh stamp-dir  directory holding recorded passes (shared
#                                    by every worktree of this clone)
#
# The fingerprint is the list of git tree/blob ids of the kernel paths, so a
# docs-only commit after a passing run does not invalidate it, and any edit
# to a kernel crate (or Cargo.lock) does.
set -euo pipefail

KERNEL_PATHS=(
  crates/cad-primitives
  crates/nurbs-rs
  crates/cherchi-rs
  crates/ssi-rs
  crates/yang-rs
  crates/kernel-v2
  crates/waffle-types
  crates/predicate-gen
  Cargo.lock
)

root=$(git rev-parse --show-toplevel)
cd "$root"

ids_of_tree() {
  local tree=$1 p
  for p in "${KERNEL_PATHS[@]}"; do
    printf '%s %s\n' "$(git rev-parse --verify --quiet "$tree:$p" || echo absent)" "$p"
  done | sha1sum | cut -d' ' -f1
}

case "${1:-}" in
  worktree)
    tmp=$(mktemp)
    trap 'rm -f "$tmp"' EXIT
    cp "$(git rev-parse --git-dir)/index" "$tmp"
    GIT_INDEX_FILE=$tmp git add -A -- "${KERNEL_PATHS[@]}" 2>/dev/null
    ids_of_tree "$(GIT_INDEX_FILE=$tmp git write-tree)"
    ;;
  touches)
    [ -n "$(git diff --name-only "$2" "$3" -- "${KERNEL_PATHS[@]}")" ]
    ;;
  stamp-dir)
    echo "$(git rev-parse --path-format=absolute --git-common-dir)/rewrite-pass"
    ;;
  "")
    echo "usage: $0 worktree | <commit> | touches <from> <to> | stamp-dir" >&2
    exit 2
    ;;
  *)
    ids_of_tree "$(git rev-parse --verify "$1^{tree}")"
    ;;
esac
