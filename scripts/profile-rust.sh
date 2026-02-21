#!/usr/bin/env bash
set -euo pipefail

# Rust test profiler — times each workspace crate and test-harness binary
# Output: test-timings-rust.log in workspace root

WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
LOG="$WORKSPACE/test-timings-rust.log"

# Associative arrays for results
declare -A TIMES
declare -A RESULTS
ORDER=()

run_test() {
    local label="$1"
    shift
    local start end elapsed exit_code

    echo "========================================" | tee -a "$LOG"
    echo "Running: $label" | tee -a "$LOG"
    echo "Command: $*" | tee -a "$LOG"
    echo "========================================" | tee -a "$LOG"

    start=$(date +%s)
    set +e
    "$@" 2>&1 | tee -a "$LOG"
    exit_code=${PIPESTATUS[0]}
    set -e
    end=$(date +%s)
    elapsed=$((end - start))

    TIMES["$label"]=$elapsed
    if [ $exit_code -eq 0 ]; then
        RESULTS["$label"]="PASS"
    else
        RESULTS["$label"]="FAIL"
    fi
    ORDER+=("$label")

    echo "" | tee -a "$LOG"
    echo ">>> $label: ${elapsed}s ($([ $exit_code -eq 0 ] && echo PASS || echo FAIL))" | tee -a "$LOG"
    echo "" | tee -a "$LOG"
}

# Clear log
echo "Rust Test Profiling — $(date)" > "$LOG"
echo "" >> "$LOG"

TOTAL_START=$(date +%s)

# Phase 1: Individual crates
CRATES=(
    "waffle-types"
    "kernel-fork"
    "sketch-solver"
    "modeling-ops"
    "feature-engine"
    "file-format"
)

for crate in "${CRATES[@]}"; do
    run_test "$crate" cargo test -p "$crate"
done

# wasm-bridge needs --no-default-features (native-solver requires libslvs C++)
run_test "wasm-bridge" cargo test -p wasm-bridge --no-default-features

# Phase 2: test-harness as a whole
run_test "test-harness (all)" cargo test -p test-harness

# Phase 3: Individual test-harness binaries
TEST_BINARIES=(
    scenarios_mock
    workflow_tests
    oracle_tests
    report_tests
    boolean_workflows
    extrude_chains
    scenarios_truck
    scenarios_advanced
    geomref_truck
    auto_union_detection
    boolean_failures
    extrude_on_extrude
    stl_tests
    size_probe
)

for bin in "${TEST_BINARIES[@]}"; do
    run_test "test-harness::$bin" cargo test -p test-harness --test "$bin"
done

TOTAL_END=$(date +%s)
TOTAL_ELAPSED=$((TOTAL_END - TOTAL_START))

# Summary table
echo "" | tee -a "$LOG"
echo "========================================" | tee -a "$LOG"
echo "        TIMING SUMMARY" | tee -a "$LOG"
echo "========================================" | tee -a "$LOG"
printf "%-35s %8s %6s\n" "Target" "Time(s)" "Result" | tee -a "$LOG"
printf "%-35s %8s %6s\n" "-----------------------------------" "--------" "------" | tee -a "$LOG"

for label in "${ORDER[@]}"; do
    printf "%-35s %8s %6s\n" "$label" "${TIMES[$label]}" "${RESULTS[$label]}" | tee -a "$LOG"
done

printf "%-35s %8s %6s\n" "-----------------------------------" "--------" "------" | tee -a "$LOG"
printf "%-35s %8s\n" "TOTAL" "${TOTAL_ELAPSED}" | tee -a "$LOG"
echo "" | tee -a "$LOG"
echo "Log saved to: $LOG" | tee -a "$LOG"
