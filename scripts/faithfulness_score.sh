#!/usr/bin/env bash
# Compute the Yang 2025 faithfulness scoreboard directly from:
#   - docs/yang_deviations.md (status index debt: OPEN + DELEGATED + PROPOSED)
#   - docs/audits/yang_2025_audit.md (per-slice and total status tallies)
#
# Usage:
#   ./scripts/faithfulness_score.sh [--check]
#
# Mixed-status cell rule:
#   Rows whose status cell combines two tokens (e.g. `4.4-15` `PARTIAL / SILENT`,
#   `4.2-18` `DEVIATES + ORDER`, `4.3-19` `PARTIAL + ORDER`) are counted under
#   their PRIMARY (leading) status token so that each of the 194 rows is counted
#   exactly once and row totals equal the sum of the seven status columns.
#
# With `--check`, exits non-zero if:
#   - any index row or matrix row has an unrecognized status token, or
#   - the `## Summary` table in `docs/audits/yang_2025_audit.md` disagrees with
#     the live per-slice or total row tallies.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

python3 - "$@" <<'PY'
import re
import sys
from pathlib import Path

check_mode = "--check" in sys.argv[1:]
extra_args = [a for a in sys.argv[1:] if a != "--check"]
if extra_args:
    sys.stderr.write(f"usage: {sys.argv[0]} [--check]\n")
    sys.exit(2)

deviations_path = Path("docs/yang_deviations.md")
audit_path = Path("docs/audits/yang_2025_audit.md")

# ---------------------------------------------------------------------------
# 1. Parse docs/yang_deviations.md status index
# ---------------------------------------------------------------------------
dev_text = deviations_path.read_text(encoding="utf-8")
m_idx = re.search(
    r"^## Status index[^\n]*\n+\| ID \| State \| Entry \|\n\|[-| ]+\|\n((?:\|.*\n)+)",
    dev_text,
    flags=re.M,
)
if not m_idx:
    sys.stderr.write("error: could not locate Status index table in docs/yang_deviations.md\n")
    sys.exit(1)

KNOWN_LEDGER_STATES = ("OPEN", "DELEGATED", "PROPOSED", "RESOLVED", "PERMANENT", "HISTORICAL")
ledger_groups = {k: [] for k in KNOWN_LEDGER_STATES}
unknown_ledger = []

for raw_line in m_idx.group(1).strip().splitlines():
    line = raw_line.strip()
    if not line.startswith("|"):
        continue
    # Split only the first two column separators so pipes inside the State or
    # Entry text (e.g. `0|off` in N77) do not fragment the row.
    parts = line[1:].split("|", 2)
    if len(parts) < 3:
        continue
    entry_id = parts[0].strip()
    state_cell = parts[1].strip()
    # Strip leading/trailing markdown bold markers (`**OPEN ...**`)
    cleaned = re.sub(r"^\*+|\*+$", "", state_cell).strip()
    m_state = re.match(r"^([A-Z]+)\b", cleaned)
    state_tok = m_state.group(1) if m_state else ""
    if state_tok in ledger_groups:
        ledger_groups[state_tok].append(entry_id)
    else:
        unknown_ledger.append((entry_id, state_cell))

open_ids = ledger_groups["OPEN"]
delegated_ids = ledger_groups["DELEGATED"]
proposed_ids = ledger_groups["PROPOSED"]
debt_total = len(open_ids) + len(delegated_ids) + len(proposed_ids)
index_total = sum(len(v) for v in ledger_groups.values()) + len(unknown_ledger)

# ---------------------------------------------------------------------------
# 2. Parse docs/audits/yang_2025_audit.md slices & Summary table
# ---------------------------------------------------------------------------
audit_text = audit_path.read_text(encoding="utf-8")

STATUS_COLS = ("FAITHFUL", "PARTIAL", "DEVIATES", "MISSING", "ORDER", "SILENT", "N/A")
STATUS_RE = re.compile(r"^(FAITHFUL|PARTIAL|DEVIATES|MISSING|ORDER|SILENT|N/A)(?:\b|(?=\s|$))")
MIXED_RE = re.compile(
    r"^(FAITHFUL|PARTIAL|DEVIATES|MISSING|ORDER|SILENT|N/A)\s*[/+]\s*"
    r"(FAITHFUL|PARTIAL|DEVIATES|MISSING|ORDER|SILENT|N/A)\b"
)
ROW_ID_RE = re.compile(r"^\|\s*([0-9A-Z.]+-[0-9]+)\s*\|")

slice_parts = re.split(r"(?=^## Slice: )", audit_text, flags=re.M)
if len(slice_parts) < 2:
    sys.stderr.write("error: could not locate `## Slice:` sections in docs/audits/yang_2025_audit.md\n")
    sys.exit(1)

slice_tallies = []
mixed_rows = []
unknown_matrix = []

for block in slice_parts[1:]:
    header_line = block.splitlines()[0]
    slice_title = header_line.removeprefix("## Slice:").strip()
    counts = {k: 0 for k in STATUS_COLS}
    rows_in_slice = 0
    for line in block.splitlines():
        if not ROW_ID_RE.match(line):
            continue
        rows_in_slice += 1
        raw = line.strip()
        if raw.startswith("|"):
            raw = raw[1:]
        if raw.endswith("|"):
            raw = raw[:-1]
        # Split on unescaped pipes (`\|` inside code cells is preserved).
        cols = [c.strip() for c in re.split(r"(?<!\\)\|", raw)]
        if len(cols) < 5:
            unknown_matrix.append((cols[0] if cols else "?", line))
            continue
        row_id = cols[0]
        status_cell = cols[4]
        m_st = STATUS_RE.match(status_cell)
        if not m_st:
            unknown_matrix.append((row_id, status_cell))
            continue
        primary = m_st.group(1)
        counts[primary] += 1
        m_mix = MIXED_RE.match(status_cell)
        if m_mix:
            sep = "+" if "+" in m_mix.group(0) else "/"
            mixed_rows.append((row_id, f"{m_mix.group(1)} {sep} {m_mix.group(2)}", primary))
    slice_tallies.append((slice_title, rows_in_slice, counts))

totals = {k: sum(c[k] for _, _, c in slice_tallies) for k in STATUS_COLS}
total_rows = sum(r for _, r, _ in slice_tallies)

def not_faithful(c):
    return c["PARTIAL"] + c["DEVIATES"] + c["MISSING"] + c["ORDER"]

# Parse existing `## Summary` table in docs/audits/yang_2025_audit.md
m_sum = re.search(
    r"^## Summary\n+(?:(?:\n|(?!\|)[^\n]+\n)*)\| slice \| rows \| FAITHFUL \| PARTIAL \| DEVIATES \| MISSING \| ORDER \| SILENT \| N/A \|\n\|[-| ]+\|\n((?:\|.*\n)+)",
    audit_text,
    flags=re.M,
)
summary_rows = []
if m_sum:
    for raw_line in m_sum.group(1).strip().splitlines():
        cols = [c.strip() for c in raw_line.strip().strip("|").split("|")]
        if len(cols) >= 9:
            label = re.sub(r"^\*+|\*+$", "", cols[0]).strip()
            nums = [int(re.sub(r"^\*+|\*+$", "", x).strip()) for x in cols[1:9]]
            summary_rows.append((label, nums))

# ---------------------------------------------------------------------------
# 3. Print scoreboard report
# ---------------------------------------------------------------------------
print("=== Yang 2025 Faithfulness Scoreboard ===")
print()
print(f"1. Ledger status index ({deviations_path}, {index_total} entries)")
print(
    f"   Debt (OPEN + DELEGATED + PROPOSED): {debt_total} "
    f"= {len(open_ids)} OPEN + {len(delegated_ids)} DELEGATED + {len(proposed_ids)} PROPOSED"
)
print(
    f"   Excluded from debt: "
    f"{len(ledger_groups['RESOLVED'])} RESOLVED, "
    f"{len(ledger_groups['PERMANENT'])} PERMANENT ({', '.join(ledger_groups['PERMANENT'])}), "
    f"{len(ledger_groups['HISTORICAL'])} HISTORICAL"
)
print(f"   OPEN ({len(open_ids)}): {', '.join(open_ids)}")
print(f"   DELEGATED ({len(delegated_ids)}): {', '.join(delegated_ids)}")
print(f"   PROPOSED ({len(proposed_ids)}): {', '.join(proposed_ids)}")
print()
print(f"2. Compliance matrix ({audit_path}, {total_rows} rows)")
print(
    f"   Not faithful (PARTIAL + DEVIATES + MISSING + ORDER): {not_faithful(totals)} of {total_rows} "
    f"= {totals['PARTIAL']} PARTIAL + {totals['DEVIATES']} DEVIATES "
    f"+ {totals['MISSING']} MISSING + {totals['ORDER']} ORDER"
)
print(
    f"   Other rows: {totals['FAITHFUL']} FAITHFUL, "
    f"{totals['SILENT']} SILENT, {totals['N/A']} N/A"
)
if mixed_rows:
    mix_desc = ", ".join(f"{rid} ({raw} -> {prim})" for rid, raw, prim in mixed_rows)
    print(f"   Mixed cells (counted by primary leading token): {mix_desc}")
print()
header_fmt = "   {:<68} {:>5} {:>8} {:>7} {:>8} {:>7} {:>5} {:>6} {:>3} | {:>12}"
print(header_fmt.format("slice", "rows", "FAITHFUL", "PARTIAL", "DEVIATES", "MISSING", "ORDER", "SILENT", "N/A", "not-faithful"))
print("   " + "-" * 138)
for title, r_cnt, c in slice_tallies:
    print(
        header_fmt.format(
            title,
            r_cnt,
            c["FAITHFUL"],
            c["PARTIAL"],
            c["DEVIATES"],
            c["MISSING"],
            c["ORDER"],
            c["SILENT"],
            c["N/A"],
            not_faithful(c),
        )
    )
print("   " + "-" * 138)
print(
    header_fmt.format(
        "total",
        total_rows,
        totals["FAITHFUL"],
        totals["PARTIAL"],
        totals["DEVIATES"],
        totals["MISSING"],
        totals["ORDER"],
        totals["SILENT"],
        totals["N/A"],
        not_faithful(totals),
    )
)

# ---------------------------------------------------------------------------
# 4. Verify Summary table agreement
# ---------------------------------------------------------------------------
disagreements = []
if unknown_ledger:
    disagreements.append(f"unrecognized ledger states: {unknown_ledger}")
if unknown_matrix:
    disagreements.append(f"unrecognized matrix row statuses: {unknown_matrix}")

expected_vectors = [
    (title, [r_cnt] + [c[k] for k in STATUS_COLS])
    for title, r_cnt, c in slice_tallies
] + [("total", [total_rows] + [totals[k] for k in STATUS_COLS])]

if len(summary_rows) != len(expected_vectors):
    disagreements.append(
        f"Summary table has {len(summary_rows)} rows, expected {len(expected_vectors)}"
    )
else:
    for idx, ((sum_label, sum_nums), (exp_label, exp_nums)) in enumerate(
        zip(summary_rows, expected_vectors)
    ):
        if sum_nums != exp_nums:
            disagreements.append(
                f"row {idx + 1} ({sum_label!r}): Summary has {sum_nums}, live rows tally to {exp_nums}"
            )

print()
if disagreements:
    print("Summary table check: MISMATCH")
    for d in disagreements:
        print(f"  - {d}")
    if check_mode:
        sys.exit(1)
else:
    print("Summary table check: OK (docs/audits/yang_2025_audit.md ## Summary matches live row tallies)")
PY
