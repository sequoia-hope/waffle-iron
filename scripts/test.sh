#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Waffle Iron Test Runner
# Usage: scripts/test.sh <subcommand>
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
APP_DIR="$ROOT_DIR/app"

# ---------------------------------------------------------------------------
# Colors
# ---------------------------------------------------------------------------
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# ---------------------------------------------------------------------------
# Rust Fast Tier — full crates (run all tests)
# ---------------------------------------------------------------------------
RUST_FAST_FULL_CRATES=(
  waffle-types
  sketch-solver
  feature-engine
  modeling-ops
  file-format
)

# wasm-bridge is special — needs --no-default-features
WASM_BRIDGE_CRATE="wasm-bridge"

# ---------------------------------------------------------------------------
# Kernel-rewrite crates (root CLAUDE.md §"Kernel Rewrite In Progress").
# Run in BOTH fast and full tiers — these are the project's #1 priority and
# their suites are quick.
# ---------------------------------------------------------------------------
RUST_REWRITE_CRATES=(
  cad-primitives
  cherchi-rs
  ssi-rs
  yang-rs
  kernel-v2
  cherchi-sidecar-rs
  indirect-predicates-sidecar-rs
)

# (PR-CR-M7c) The former cherchi-rs `--features indirect-predicates` FFI tier
# is gone: the M6 native arrangement now uses the clean-room pure-Rust
# predicates and compiles unconditionally, so the plain `cargo test -p
# cherchi-rs` run above covers it. The FFI shim survives only as a
# dev-dependency parity oracle inside cherchi-rs's own test suite.

# ---------------------------------------------------------------------------
# Rust Fast Tier — kernel filtered modules
# ---------------------------------------------------------------------------
KERNEL_FAST_FILTERS=(
  mock_kernel
  "types::"
  "primitives::"
  "tessellation::"
)

# ---------------------------------------------------------------------------
# Rust Fast Tier — test-harness filtered binaries
# ---------------------------------------------------------------------------
TEST_HARNESS_FAST_BINS=(
  scenarios_mock
  workflow_tests
  oracle_tests
  report_tests
  scenarios_advanced
  stl_tests
)

# ---------------------------------------------------------------------------
# Rust Full Tier — all crates (run individually)
# ---------------------------------------------------------------------------
RUST_FULL_CRATES=(
  waffle-types
  kernel
  sketch-solver
  feature-engine
  modeling-ops
  file-format
  test-harness
)

# ---------------------------------------------------------------------------
# GUI Fast Tier — spec files (relative to app/tests/gui/)
# ---------------------------------------------------------------------------
GUI_FAST_SPECS=(
  sketch-draw.spec.js
  sketch-tools.spec.js
  sketch-entry.spec.js
  sketch-edit.spec.js
  sketch-finish.spec.js
  sketch-feedback.spec.js
  sketch-drawing-regression.spec.js
  sketch-draw-diagnostic.spec.js
  sketch-circle-workflow.spec.js
  arc-regression.spec.js
  construction-mode.spec.js
  constraint-toolbar.spec.js
  dimension-tool.spec.js
  snap-labels.spec.js
  snap-detect-new-types.spec.js
  snap-preview-integration.spec.js
  snap-preview-candidates.spec.js
  snap-hover-indicator.spec.js
  datum-planes.spec.js
  feature-tree.spec.js
  feature-tree-origin.spec.js
  feature-tree-operations.spec.js
  modeling-buttons.spec.js
  keyboard-shortcuts.spec.js
  input-validation.spec.js
  toast-notifications.spec.js
  error-paths.spec.js
  cancel-and-recovery.spec.js
  viewport.spec.js
  viewcube-contextmenu.spec.js
  property-editor.spec.js
  property-editor-advanced.spec.js
  undo-redo.spec.js
  selection/box-select.spec.js
  selection/edge-pick.spec.js
  selection/select-other.spec.js
  sketch-polyline-drag.spec.js
  tool-switching-mid-operation.spec.js
  unit-conversion-display.spec.js
)

# ---------------------------------------------------------------------------
# Concurrency — maximize parallelism within memory safety bounds.
# Each boolean cascade test: 200-500MB. Each Chromium+WASM worker: 200-500MB.
# Default 8 threads/workers ≈ 8GB peak. Override: TEST_THREADS=12 scripts/test.sh full
# ---------------------------------------------------------------------------
TEST_THREADS="${TEST_THREADS:-8}"

# ---------------------------------------------------------------------------
# Memory guard — prevent test processes from triggering system-wide OOM.
# Uses prlimit to cap virtual address space for child processes.
# Reserve ~10GB for Claude + OS, allow tests to use the rest.
# Set MEM_LIMIT_GB=0 to disable.
# ---------------------------------------------------------------------------
MEM_LIMIT_GB="${MEM_LIMIT_GB:-80}"

mem_guard() {
  if [[ "$MEM_LIMIT_GB" -gt 0 ]] && command -v prlimit &>/dev/null; then
    local bytes=$(( MEM_LIMIT_GB * 1073741824 ))
    prlimit --as="$bytes" -- "$@"
  else
    "$@"
  fi
}

# ---------------------------------------------------------------------------
# State
# ---------------------------------------------------------------------------
OVERALL_STATUS=0

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
timer_start() { date +%s; }
timer_elapsed() {
  local start=$1
  local end
  end=$(date +%s)
  echo $(( end - start ))
}

header() {
  echo ""
  echo -e "${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
  echo -e "${YELLOW}  $1${NC}"
  echo -e "${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

pass() { echo -e "  ${GREEN}✓ $1${NC}"; }
fail() { echo -e "  ${RED}✗ $1${NC}"; OVERALL_STATUS=1; }

run_cargo_test() {
  local crate="$1"
  shift
  local label="$crate"
  local start rc=0
  start=$(timer_start)

  mem_guard cargo test -p "$crate" "$@" -- --test-threads="$TEST_THREADS" || rc=$?

  local elapsed
  elapsed=$(timer_elapsed "$start")
  if [[ $rc -eq 0 ]]; then
    pass "$label (${elapsed}s)"
  else
    fail "$label (${elapsed}s)"
  fi
}

run_cargo_test_filter() {
  local crate="$1"
  local filter="$2"
  local label="$crate [$filter]"
  local start rc=0
  start=$(timer_start)

  mem_guard cargo test -p "$crate" -- "$filter" --test-threads="$TEST_THREADS" || rc=$?

  local elapsed
  elapsed=$(timer_elapsed "$start")
  if [[ $rc -eq 0 ]]; then
    pass "$label (${elapsed}s)"
  else
    fail "$label (${elapsed}s)"
  fi
}

run_cargo_test_binary() {
  local crate="$1"
  local binary="$2"
  local label="$crate --test $binary"
  local start rc=0
  start=$(timer_start)

  mem_guard cargo test -p "$crate" --test "$binary" -- --test-threads="$TEST_THREADS" || rc=$?

  local elapsed
  elapsed=$(timer_elapsed "$start")
  if [[ $rc -eq 0 ]]; then
    pass "$label (${elapsed}s)"
  else
    fail "$label (${elapsed}s)"
  fi
}

# ---------------------------------------------------------------------------
# Tier: Kernel Rewrite (new crates; part of fast AND full)
# ---------------------------------------------------------------------------
run_rust_rewrite() {
  header "Kernel Rewrite Crates"
  local tier_start
  tier_start=$(timer_start)

  for crate in "${RUST_REWRITE_CRATES[@]}"; do
    run_cargo_test "$crate"
  done

  local elapsed
  elapsed=$(timer_elapsed "$tier_start")
  echo ""
  echo -e "${CYAN}  Kernel Rewrite tier completed in ${elapsed}s${NC}"
}

# ---------------------------------------------------------------------------
# Tier: Rust Fast (~420 tests, target <30s)
# ---------------------------------------------------------------------------
run_rust_fast() {
  run_rust_rewrite

  header "Rust Fast Tier"
  local tier_start
  tier_start=$(timer_start)

  # Full crates
  for crate in "${RUST_FAST_FULL_CRATES[@]}"; do
    run_cargo_test "$crate"
  done

  # wasm-bridge with --no-default-features
  run_cargo_test "$WASM_BRIDGE_CRATE" --no-default-features

  # kernel filtered
  for filter in "${KERNEL_FAST_FILTERS[@]}"; do
    run_cargo_test_filter kernel "$filter"
  done

  # test-harness fast binaries
  for binary in "${TEST_HARNESS_FAST_BINS[@]}"; do
    run_cargo_test_binary test-harness "$binary"
  done

  local elapsed
  elapsed=$(timer_elapsed "$tier_start")
  echo ""
  echo -e "${CYAN}  Rust Fast tier completed in ${elapsed}s${NC}"
}

# ---------------------------------------------------------------------------
# Tier: Rust Full (~910 tests)
# ---------------------------------------------------------------------------
run_rust_full() {
  run_rust_rewrite

  header "Rust Full Tier"
  local tier_start
  tier_start=$(timer_start)

  # All crates except wasm-bridge (run with default features)
  for crate in "${RUST_FULL_CRATES[@]}"; do
    run_cargo_test "$crate"
  done

  # wasm-bridge with --no-default-features
  run_cargo_test "$WASM_BRIDGE_CRATE" --no-default-features

  local elapsed
  elapsed=$(timer_elapsed "$tier_start")
  echo ""
  echo -e "${CYAN}  Rust Full tier completed in ${elapsed}s${NC}"
}

# ---------------------------------------------------------------------------
# Tier: GUI Fast (~260 tests)
# ---------------------------------------------------------------------------
run_gui_fast() {
  header "GUI Fast Tier"
  local tier_start rc=0
  tier_start=$(timer_start)

  # Build spec file arguments
  local spec_args=()
  for spec in "${GUI_FAST_SPECS[@]}"; do
    spec_args+=("tests/gui/$spec")
  done

  (cd "$APP_DIR" && PW_WORKERS="$TEST_THREADS" mem_guard npx playwright test "${spec_args[@]}") || rc=$?

  local elapsed
  elapsed=$(timer_elapsed "$tier_start")
  if [[ $rc -eq 0 ]]; then
    pass "GUI Fast (${elapsed}s)"
  else
    fail "GUI Fast (${elapsed}s)"
  fi
  echo ""
  echo -e "${CYAN}  GUI Fast tier completed in ${elapsed}s${NC}"
}

# ---------------------------------------------------------------------------
# Tier: GUI Full (~425 tests)
# ---------------------------------------------------------------------------
run_gui_full() {
  header "GUI Full Tier"
  local tier_start rc=0
  tier_start=$(timer_start)

  (cd "$APP_DIR" && PW_WORKERS="$TEST_THREADS" mem_guard npx playwright test tests/gui/) || rc=$?

  local elapsed
  elapsed=$(timer_elapsed "$tier_start")
  if [[ $rc -eq 0 ]]; then
    pass "GUI Full (${elapsed}s)"
  else
    fail "GUI Full (${elapsed}s)"
  fi
  echo ""
  echo -e "${CYAN}  GUI Full tier completed in ${elapsed}s${NC}"
}

# ---------------------------------------------------------------------------
# Tier: Assay Quick (proptest with small case count, <30s)
# ---------------------------------------------------------------------------
run_assay_quick() {
  header "Assay Quick Tier (proptest, small cases)"
  local tier_start
  tier_start=$(timer_start)

  # Regression corpus replay (fast — just loads JSON)
  run_cargo_test_binary test-harness assay_regression

  # Box-box property tests with reduced case count
  local start rc=0
  start=$(timer_start)
  PROPTEST_CASES=5 mem_guard cargo test -p test-harness --test assay_box_box -- --test-threads="$TEST_THREADS" || rc=$?
  local elapsed
  elapsed=$(timer_elapsed "$start")
  if [[ $rc -eq 0 ]]; then
    pass "assay_box_box (5 cases, ${elapsed}s)"
  else
    fail "assay_box_box (5 cases, ${elapsed}s)"
  fi

  elapsed=$(timer_elapsed "$tier_start")
  echo ""
  echo -e "${CYAN}  Assay Quick tier completed in ${elapsed}s${NC}"
}

# ---------------------------------------------------------------------------
# Tier: Assay (~3min, default proptest case count)
# ---------------------------------------------------------------------------
run_assay() {
  header "Assay Tier (proptest, default cases)"
  local tier_start
  tier_start=$(timer_start)

  run_cargo_test_binary test-harness assay_regression
  run_cargo_test_binary test-harness assay_box_box
  run_cargo_test_binary test-harness assay_determinism

  local elapsed
  elapsed=$(timer_elapsed "$tier_start")
  echo ""
  echo -e "${CYAN}  Assay tier completed in ${elapsed}s${NC}"
}

# ---------------------------------------------------------------------------
# Tier: Assay Deep (nightly, large proptest case count)
# ---------------------------------------------------------------------------
run_assay_deep() {
  header "Assay Deep Tier (proptest, 100 cases — nightly)"
  local tier_start
  tier_start=$(timer_start)

  run_cargo_test_binary test-harness assay_regression

  local start rc

  start=$(timer_start)
  rc=0
  PROPTEST_CASES=100 mem_guard cargo test -p test-harness --test assay_box_box -- --test-threads="$TEST_THREADS" || rc=$?
  local elapsed
  elapsed=$(timer_elapsed "$start")
  if [[ $rc -eq 0 ]]; then
    pass "assay_box_box (100 cases, ${elapsed}s)"
  else
    fail "assay_box_box (100 cases, ${elapsed}s)"
  fi

  start=$(timer_start)
  rc=0
  PROPTEST_CASES=50 mem_guard cargo test -p test-harness --test assay_determinism -- --test-threads="$TEST_THREADS" || rc=$?
  elapsed=$(timer_elapsed "$start")
  if [[ $rc -eq 0 ]]; then
    pass "assay_determinism (50 cases, ${elapsed}s)"
  else
    fail "assay_determinism (50 cases, ${elapsed}s)"
  fi

  elapsed=$(timer_elapsed "$tier_start")
  echo ""
  echo -e "${CYAN}  Assay Deep tier completed in ${elapsed}s${NC}"
}

# ---------------------------------------------------------------------------
# Help / Usage
# ---------------------------------------------------------------------------
print_help() {
  echo -e "${BOLD}Waffle Iron Test Runner${NC}"
  echo ""
  echo -e "Usage: ${CYAN}scripts/test.sh <subcommand>${NC}"
  echo ""
  echo -e "${BOLD}Subcommands:${NC}"
  echo -e "  ${GREEN}rewrite${NC}      Kernel-rewrite crates only (new-crate suites)"
  echo -e "  ${GREEN}fast${NC}         Rust fast tier       (rewrite crates + legacy fast, <60s target)"
  echo -e "  ${GREEN}full${NC}         Rust full tier        (~910 tests)"
  echo -e "  ${GREEN}gui-fast${NC}     GUI fast tier         (~260 tests, 35 spec files)"
  echo -e "  ${GREEN}gui-full${NC}     GUI full tier         (~425 tests, all spec files)"
  echo -e "  ${GREEN}all-fast${NC}     fast + gui-fast"
  echo -e "  ${GREEN}all${NC}          full + gui-full"
  echo -e "  ${GREEN}assay-quick${NC}  Assay proptest        (5 cases, <30s)"
  echo -e "  ${GREEN}assay${NC}        Assay proptest        (default cases, ~3min)"
  echo -e "  ${GREEN}assay-deep${NC}   Assay proptest        (100 cases, nightly)"
  echo -e "  ${GREEN}profile${NC}      Run Rust test profiler (delegates to scripts/profile-rust.sh)"
  echo -e "  ${GREEN}help${NC}         Print this help"
  echo ""
  echo -e "${BOLD}Kernel Rewrite Crates (run in fast AND full):${NC}"
  echo "  ${RUST_REWRITE_CRATES[*]}"
  echo ""
  echo -e "${BOLD}Rust Fast Tier:${NC}"
  echo "  Full crates: ${RUST_FAST_FULL_CRATES[*]} $WASM_BRIDGE_CRATE"
  echo "  kernel filters: ${KERNEL_FAST_FILTERS[*]}"
  echo "  test-harness filters: ${TEST_HARNESS_FAST_BINS[*]}"
  echo ""
  echo -e "${BOLD}Rust Full Tier:${NC}"
  echo "  All crates: ${RUST_FULL_CRATES[*]} $WASM_BRIDGE_CRATE"
  echo ""
  echo -e "${BOLD}GUI Fast Tier:${NC}"
  echo "  ${#GUI_FAST_SPECS[@]} spec files (sketch, snap, feature-tree, selection, etc.)"
  echo ""
  echo -e "${BOLD}GUI Full Tier:${NC}"
  echo "  All spec files in app/tests/gui/"
  echo ""
  echo -e "${BOLD}Assay Tiers:${NC}"
  echo "  assay-quick: corpus replay + box-box proptest (5 cases)"
  echo "  assay:       corpus replay + box-box + determinism (default cases)"
  echo "  assay-deep:  corpus replay + box-box (100 cases) + determinism (50 cases)"
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
  local cmd="${1:-help}"

  case "$cmd" in
    rewrite)
      run_rust_rewrite
      ;;
    fast)
      run_rust_fast
      ;;
    full)
      run_rust_full
      ;;
    gui-fast)
      run_gui_fast
      ;;
    gui-full)
      run_gui_full
      ;;
    all-fast)
      run_rust_fast
      run_gui_fast
      ;;
    all)
      run_rust_full
      run_gui_full
      ;;
    assay-quick)
      run_assay_quick
      ;;
    assay)
      run_assay
      ;;
    assay-deep)
      run_assay_deep
      ;;
    profile)
      exec "$SCRIPT_DIR/profile-rust.sh" "${@:2}"
      ;;
    help|--help|-h)
      print_help
      ;;
    *)
      echo -e "${RED}Unknown subcommand: $cmd${NC}" >&2
      echo ""
      print_help
      exit 1
      ;;
  esac

  # Final summary
  if [[ "$cmd" != "help" && "$cmd" != "--help" && "$cmd" != "-h" && "$cmd" != "profile" ]]; then
    echo ""
    if [[ $OVERALL_STATUS -eq 0 ]]; then
      echo -e "${GREEN}${BOLD}All tests passed.${NC}"
    else
      echo -e "${RED}${BOLD}Some tests failed.${NC}"
    fi
    exit $OVERALL_STATUS
  fi
}

main "$@"
