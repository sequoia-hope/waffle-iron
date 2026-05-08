#!/usr/bin/env bash
# Extract text views of refs/*.pdf for agent paper-reading.
#
# refs/ is gitignored (academic papers are usually license-restricted),
# so output goes to refs/text/ which is also gitignored. The text view
# is purely local convenience — line-number citations, grep, scrolling.
#
# Run this once at session start if you (or your agents) need to read
# paper sections from refs/. Idempotent: skips outputs that are newer
# than their PDFs.
#
# Requires: poppler-utils (apt-get install poppler-utils)

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REFS_DIR="$ROOT/refs"
OUT_DIR="$REFS_DIR/text"

if [[ ! -d "$REFS_DIR" ]]; then
    echo "error: $REFS_DIR does not exist (refs/ is gitignored; obtain PDFs separately)" >&2
    exit 1
fi

if ! command -v pdftotext >/dev/null 2>&1; then
    echo "error: pdftotext not found. Install poppler-utils:" >&2
    echo "  Debian/Ubuntu:  sudo apt-get install poppler-utils" >&2
    echo "  macOS (brew):   brew install poppler" >&2
    exit 1
fi

mkdir -p "$OUT_DIR"

shopt -s nullglob
pdfs=("$REFS_DIR"/*.pdf)
if [[ ${#pdfs[@]} -eq 0 ]]; then
    echo "no PDFs in $REFS_DIR" >&2
    exit 0
fi

extracted=0
skipped=0
for pdf in "${pdfs[@]}"; do
    name=$(basename "$pdf" .pdf)
    out="$OUT_DIR/${name}.txt"
    if [[ -f "$out" && "$out" -nt "$pdf" ]]; then
        skipped=$((skipped + 1))
        continue
    fi
    # -layout preserves column layout, which matters for two-column papers.
    # pdftotext writes "Syntax Error: ..." for some PDFs (Yang 2025 has
    # benign XObject warnings); the extracted text is still correct.
    # Suppress noisy but harmless pdftotext diagnostics (e.g. Yang 2025 has
    # benign XObject XRefs; Piegl-Tiller has 312 font-type mismatches).
    pdftotext -layout "$pdf" "$out" 2> >(grep -vE "^Syntax (Error|Warning)|^I/O Error" >&2) || true
    if [[ -s "$out" ]]; then
        extracted=$((extracted + 1))
        echo "extracted: ${out#"$ROOT"/}"
    else
        echo "warn: extraction produced empty output for ${pdf#"$ROOT"/}" >&2
        rm -f "$out"
    fi
done

echo
echo "extracted=$extracted skipped(up-to-date)=$skipped total_pdfs=${#pdfs[@]}"
