#!/usr/bin/env bash
# Fetch the Patrikalakis-Maekawa-Cho hyperbook from MIT and extract text.
# Produces: docs/references/patrikalakis-shape-interrogation.txt
#
# Pages: node1.html through node246.html (246 pages)
# The body text is plain HTML; only nav links are XOR-obfuscated JS.
# We strip JS, then use w3m/lynx/python to convert HTML→text.

set -euo pipefail

BASE_URL="https://web.mit.edu/hyperbook/Patrikalakis-Maekawa-Cho"
WORK_DIR=$(mktemp -d)
OUT_DIR="$(cd "$(dirname "$0")/.." && pwd)/docs/references"
OUT_FILE="$OUT_DIR/patrikalakis-shape-interrogation.txt"
FIRST_NODE=1
LAST_NODE=246
PARALLEL=8

echo "Downloading $LAST_NODE pages from $BASE_URL ..."
echo "Working directory: $WORK_DIR"

# --- Download all pages in parallel batches ---
download_page() {
    local n=$1
    local url="$BASE_URL/node${n}.html"
    local out="$WORK_DIR/node${n}.html"
    curl -sL --retry 2 --max-time 15 -o "$out" "$url"
}

export -f download_page
export BASE_URL WORK_DIR

seq "$FIRST_NODE" "$LAST_NODE" | xargs -P "$PARALLEL" -I{} bash -c 'download_page {}'

downloaded=$(ls "$WORK_DIR"/node*.html 2>/dev/null | wc -l)
echo "Downloaded $downloaded pages."

if [ "$downloaded" -eq 0 ]; then
    echo "ERROR: No pages downloaded." >&2
    exit 1
fi

# --- Extract text from HTML ---
# Strategy: strip all <SCRIPT>...</SCRIPT> blocks (the XOR-encoded nav),
# strip <NOSCRIPT> blocks, then convert remaining HTML to plain text.

# Check for a text-mode browser for HTML→text conversion
if command -v w3m &>/dev/null; then
    HTML2TEXT="w3m -dump -T text/html"
elif command -v lynx &>/dev/null; then
    HTML2TEXT="lynx -dump -stdin -nolist"
elif command -v python3 &>/dev/null; then
    # Fallback: simple Python HTML stripper
    HTML2TEXT="python3 -c \"
import sys, html, re
text = sys.stdin.read()
text = re.sub(r'<br\s*/?>','\n', text, flags=re.I)
text = re.sub(r'<p\s*/?>','\n\n', text, flags=re.I)
text = re.sub(r'<[^>]+>','', text)
text = html.unescape(text)
# collapse blank lines
text = re.sub(r'\n{3,}','\n\n', text)
print(text)
\""
else
    echo "ERROR: Need w3m, lynx, or python3 for HTML-to-text conversion." >&2
    exit 1
fi

echo "Using: $HTML2TEXT"
echo "Extracting text..."

{
    echo "============================================================"
    echo "Patrikalakis, Maekawa & Cho"
    echo "Shape Interrogation for Computer Aided Design and Manufacturing"
    echo "(Hyperbook Edition, December 2009)"
    echo "Source: $BASE_URL"
    echo "Extracted: $(date -u +%Y-%m-%d)"
    echo "============================================================"
    echo ""

    for n in $(seq "$FIRST_NODE" "$LAST_NODE"); do
        f="$WORK_DIR/node${n}.html"
        [ -f "$f" ] || continue

        # Strip SCRIPT and NOSCRIPT blocks, then convert to text.
        # Use perl for robust multi-line regex on the raw HTML.
        cleaned=$(perl -0777 -pe '
            s/<SCRIPT[^>]*>.*?<\/SCRIPT>//gsi;
            s/<NOSCRIPT>.*?<\/NOSCRIPT>//gsi;
        ' "$f")

        # Convert cleaned HTML to plain text
        page_text=$(echo "$cleaned" | eval "$HTML2TEXT" 2>/dev/null || true)

        # Skip empty pages
        stripped=$(echo "$page_text" | tr -d '[:space:]')
        [ -z "$stripped" ] && continue

        echo "--- node${n}.html ---"
        echo "$page_text"
        echo ""
    done
} > "$OUT_FILE"

lines=$(wc -l < "$OUT_FILE")
size=$(wc -c < "$OUT_FILE")
echo ""
echo "Done. Output: $OUT_FILE"
echo "  $lines lines, $(( size / 1024 )) KB"

# Cleanup
rm -rf "$WORK_DIR"
