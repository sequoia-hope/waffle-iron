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
if [ -f target/pair-matrix/report.json ]; then
  python3 - target/pair-matrix/report.json "$(git rev-parse --short HEAD)" <<'PY'
import json, sys

r = json.load(open(sys.argv[1]))
head = sys.argv[2]
rep_commit = r.get("commit", "unstamped")
stale = "" if rep_commit == head else f" [STALE vs HEAD {head}]"
t = r["totals"]
vol_pairs = r.get("pairs_with_correct_volume_agreement")
vol_all = r.get("pairs_with_all_ops_correct_volume_agreement")
vol_clause = (
    f"; {vol_pairs}/{r['total_pairs']} CORRECT with volume agreement ({vol_all}/{r['total_pairs']} all 3 ops)"
    if vol_pairs is not None
    else ""
)
print(
    f"   surface-pair matrix ({sys.argv[1]}, commit {rep_commit}{stale}, {r['seeds_per_op']} seed(s)/op): "
    f"{r['pairs_with_correct_customer']}/{r['total_pairs']} pairs have a CORRECT customer "
    f"({r['pairs_with_all_ops_correct']}/{r['total_pairs']} all 3 ops{vol_clause}; "
    f"{r['buildable_pairs']} buildable, {r['not_buildable_pairs']} NOT BUILDABLE)"
)
ca = t.get("correct_volume_agree")
cnc = t.get("correct_volume_not_covered")
c_detail = f"{t['correct']}C({ca}A/{cnc}NC)" if ca is not None and cnc is not None else f"{t['correct']}C"
print(
    f"   runs: {c_detail} / {t['wrong']}W / {t['error']}E / "
    f"{t['timeout']}T / {t['unsupported']}U over {r['total_cases_run']}"
)
wrong_cases = [c["id"] for c in r.get("cases", []) if c.get("category") == "SUPPORTED_WRONG"]
if wrong_cases:
    print(f"   SUPPORTED_WRONG cases ({len(wrong_cases)}): {', '.join(wrong_cases)}")
missing = [
    row["pair"]
    for row in r.get("rows", [])
    if row.get("buildable") and not row.get("has_correct_customer")
]
if missing:
    print(f"   buildable pairs without a CORRECT customer ({len(missing)}): {', '.join(missing)}")
PY
elif ls crates/test-harness/tests/assay_pair_matrix*.rs >/dev/null 2>&1 || ls crates/test-harness/tests/surface_pair_matrix*.rs >/dev/null 2>&1; then
  echo "   surface-pair matrix test: present (no target/pair-matrix/report.json — run assay_pair_matrix)"
else
  echo "   surface-pair matrix test: not built (target 21/21)"
fi
echo
# Gate 3.7 (contact matrix), gate 3.2 (F5a) and gate 1.4 (OCCT oracle): each
# prints its report when one exists; until its harness is built it says so.
if [ -f target/contact-matrix/report.json ]; then
  echo "   degenerate-contact matrix: report at target/contact-matrix/report.json"
else
  echo "   degenerate-contact matrix: not built (target: every cell CORRECT or typed)"
fi
if grep -rqE "^pub trait ParametricSurface" crates/yang-rs/src 2>/dev/null; then
  echo "   surface interface (F5a): trait present"
else
  echo "   surface interface (F5a): not built; $(grep -rE 'Surface::(Plane|Cylinder|Cone|Sphere|Torus)' crates/yang-rs/src --include='*.rs' | grep -v tests_unit | wc -l) surface-type matches in yang-rs"
fi
echo
echo "Gate 1.4 — independent kernel (OCCT)"
if [ -f target/occt-oracle/report.json ]; then
  echo "   report at target/occt-oracle/report.json"
else
  echo "   oracle not built (target: 0 unadjudicated disagreements)"
fi
echo
echo "Gate 4 — panics (K7)"
echo "   not yet gated: K7 lands clippy::expect_used/unwrap_used deny in yang-rs"
echo "   (the 2026-10-28 review counted ~60 production sites)"
