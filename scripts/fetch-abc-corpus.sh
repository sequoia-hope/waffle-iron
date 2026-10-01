#!/usr/bin/env bash
#
# Fetch one chunk of the ABC dataset's STEP distribution, for the SI5 census
# (specs/step_import_si5_exact_analytic_ingestion.md §2) and the external-corpus
# boolean hardening plan (specs/boolean_hardening_external_corpus.md).
#
#   scripts/fetch-abc-corpus.sh [chunk] [dest]
#
# Defaults: chunk 0000, dest /tmp/abc. Chunk 0000 is ~1.6 GB compressed and
# ~13.7 GB extracted (10 000 models) — deliberately NOT under the repo and not
# in a session scratchpad, so it survives as a cache across sessions.
#
# The corpus is Onshape-sourced and license-restricted (see the ABC dataset
# terms); like refs/*.pdf it is local-only and never committed.
set -euo pipefail

CHUNK="${1:-0000}"
DEST="${2:-/tmp/abc}"
LINKS_URL="https://deep-geometry.github.io/abc-dataset/data/step_v00.txt"

mkdir -p "$DEST"
cd "$DEST"

if [ ! -s step_v00.txt ]; then
  echo "fetching chunk link list"
  curl -fsSL "$LINKS_URL" -o step_v00.txt
fi

ARCHIVE="abc_${CHUNK}_step_v00.7z"
URL=$(awk -v f="$ARCHIVE" '$2 == f { print $1 }' step_v00.txt)
if [ -z "$URL" ]; then
  echo "no such chunk in step_v00.txt: $ARCHIVE" >&2
  echo "available: $(awk '{print $2}' step_v00.txt | head -3) ... (100 chunks)" >&2
  exit 1
fi

if [ ! -s "$ARCHIVE" ]; then
  echo "downloading $ARCHIVE (~1.6 GB)"
  # -C - resumes a partial download; write to .part so an interrupted run never
  # leaves a truncated archive that looks complete to the next invocation.
  curl -fL --retry 5 --retry-delay 5 -C - -o "$ARCHIVE.part" "$URL"
  mv "$ARCHIVE.part" "$ARCHIVE"
fi

OUT="$DEST/chunk$CHUNK"
if [ ! -d "$OUT" ]; then
  command -v 7z >/dev/null || { echo "need 7z (p7zip)" >&2; exit 1; }
  echo "extracting to $OUT (~13.7 GB)"
  mkdir -p "$OUT"
  7z x -y -o"$OUT" "$ARCHIVE" > "$DEST/extract-$CHUNK.log"
  tail -4 "$DEST/extract-$CHUNK.log"
fi

echo "models: $(find "$OUT" -name '*.step' | wc -l)  in  $OUT"
echo
echo "next:"
echo "  JOBS=14 scripts/si5_census.py    $OUT > /tmp/si5_census.tsv"
echo "  JOBS=14 scripts/si5_exactness.py $OUT > /tmp/si5_exactness.tsv"
echo "  scripts/si5_census_report.py /tmp/si5_census.tsv /tmp/si5_exactness.tsv"
