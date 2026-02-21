#!/usr/bin/env bash
set -euo pipefail

# GUI test profiler — runs Playwright tests and extracts per-spec durations
# Output: test-timings-gui.json in workspace root

WORKSPACE="$(cd "$(dirname "$0")/.." && pwd)"
JSON_OUT="$WORKSPACE/test-timings-gui.json"
APP_DIR="$WORKSPACE/app"

echo "GUI Test Profiling — $(date)"
echo ""

# Run Playwright tests with JSON reporter
cd "$APP_DIR"
echo "Running Playwright tests with JSON reporter..."
echo "Output: $JSON_OUT"
echo ""

set +e
npx playwright test tests/gui/ --reporter=json > "$JSON_OUT" 2>/dev/null
EXIT_CODE=$?
set -e

if [ ! -s "$JSON_OUT" ]; then
    echo "ERROR: No JSON output produced. Playwright may not be configured or tests failed to start."
    echo "Exit code: $EXIT_CODE"
    exit 1
fi

echo ""
echo "========================================"
echo "     TOP 20 SLOWEST SPEC FILES"
echo "========================================"
echo ""

# Parse JSON to extract per-spec durations
# The Playwright JSON report has suites[].suites[].specs[].tests[].results[].duration
# We aggregate by spec file
node -e "
const fs = require('fs');
const data = JSON.parse(fs.readFileSync('$JSON_OUT', 'utf8'));

const specTimes = {};

function walkSuites(suites, filePath) {
    for (const suite of (suites || [])) {
        const file = suite.file || filePath;
        // Aggregate spec durations
        for (const spec of (suite.specs || [])) {
            for (const test of (spec.tests || [])) {
                for (const result of (test.results || [])) {
                    const duration = result.duration || 0;
                    if (!specTimes[file]) specTimes[file] = 0;
                    specTimes[file] += duration;
                }
            }
        }
        // Recurse into nested suites
        if (suite.suites) {
            walkSuites(suite.suites, file);
        }
    }
}

walkSuites(data.suites, '');

// Sort by duration descending
const sorted = Object.entries(specTimes)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 20);

console.log(
    'Rank'.padEnd(6) +
    'Spec File'.padEnd(55) +
    'Duration(ms)'.padStart(12)
);
console.log('-'.repeat(73));

sorted.forEach(([file, ms], i) => {
    const shortFile = file.replace(/^.*tests\/gui\//, '');
    console.log(
        String(i + 1).padEnd(6) +
        shortFile.padEnd(55) +
        String(Math.round(ms)).padStart(12)
    );
});

console.log('-'.repeat(73));
const totalMs = Object.values(specTimes).reduce((a, b) => a + b, 0);
console.log('Total: ' + Math.round(totalMs / 1000) + 's across ' + Object.keys(specTimes).length + ' spec files');
" 2>&1 || echo "Failed to parse JSON output. Check $JSON_OUT manually."

echo ""
echo "Full JSON saved to: $JSON_OUT"
