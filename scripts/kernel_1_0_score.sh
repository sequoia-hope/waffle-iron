#!/usr/bin/env bash
# Kernel 1.0 scoreboard (docs/kernel_1_0.md §2): prints each gate's computed
# numbers from the latest corpus report, the prospector seed reports and the
# faithfulness scoreboard. Items not yet measurable by a script say so.
#
#   ./scripts/kernel_1_0_score.sh [corpus_report.json]
#
# The corpus report defaults to target/assay_kv2_report.json (written by
# every full_corpus_categorized run).
set -euo pipefail
cd "$(dirname "$0")/.."

report="${1:-target/assay_kv2_report.json}"

echo "=== Kernel 1.0 scoreboard (docs/kernel_1_0.md) ==="
echo "commit: $(git rev-parse --short HEAD)"
echo

python3 - "$report" <<'PY'
import glob, json, os, re, sys

report = sys.argv[1]
print("Gate 1 — Correct / Gate 4 — Bounded (corpus)")
if os.path.exists(report):
    r = json.load(open(report))
    print(f"   corpus report: {report}")
    print(f"   {r['supported_correct']}C / {r['supported_wrong']}W / {r['error']}E / "
          f"{r['expected_error']}EE / {r['timeout']}T over {r['total']}")
    record_only = [c for c in r["cases"]
                   if re.search(r"exact_vs_mesh: .*bound_ratio ([0-9.e+-]+)", c.get("detail", ""))
                   and float(re.search(r"bound_ratio ([0-9.e+-]+)", c["detail"]).group(1)) > 1.0]
    mono = [c for c in r["cases"]
            if re.search(r"monotonicity: \d+ checked, \d+ not covered, ([1-9]\d*) violation", c.get("detail", ""))]
    print(f"   record-only O2 (exact_vs_mesh bound_ratio > 1): {len(record_only)} case(s)"
          + (": " + ", ".join(c['id'] for c in record_only) if record_only else ""))
    print(f"   record-only O5 (monotonicity violations): {len(mono)} case(s)"
          + (": " + ", ".join(c['id'] for c in mono) if mono else ""))
else:
    print(f"   no corpus report at {report}")

print()
print("Gate 1 / 3 / 4 — fresh prospector seeds (target/prospect/seed-N/report.jsonl)")
seeds = []
for path in glob.glob("target/prospect/seed-*/report.jsonl"):
    m = re.fullmatch(r"target/prospect/seed-(\d+)/report.jsonl", path)
    if m:
        seeds.append((int(m.group(1)), path))
for seed, path in sorted(seeds)[-3:]:
    cats = {}
    for line in open(path):
        line = line.strip()
        if line:
            c = json.loads(line)["category"]
            cats[c] = cats.get(c, 0) + 1
    n = sum(cats.values())
    w = sum(v for k, v in cats.items() if "WRONG" in k)
    e = cats.get("ERROR", 0)
    t = cats.get("TIMEOUT", 0)
    print(f"   seed {seed}: n={n}  WRONG {w}  ERROR {e} ({100.0*e/max(n,1):.1f} %)  TIMEOUT {t}  "
          + " ".join(f"{k}={v}" for k, v in sorted(cats.items())))
PY

echo
echo "Gate 2 — Faithful"
./scripts/faithfulness_score.sh | grep -E 'Debt \(|Not faithful \(' | sed 's/^ */   /'
echo "   consecutive clean independent audits: see docs/kernel_1_0.md §5 (manual)"
echo
echo "Gate 3 — Capable"
if ls crates/test-harness/tests/surface_pair_matrix*.rs >/dev/null 2>&1; then
  echo "   surface-pair matrix test: present (run it for the pair count)"
else
  echo "   surface-pair matrix test: not built (target 21/21)"
fi
echo
echo "Gate 4 — panics (K7)"
echo "   not yet gated: K7 lands clippy::expect_used/unwrap_used deny in yang-rs"
echo "   (the 2026-10-28 review counted ~60 production sites)"
