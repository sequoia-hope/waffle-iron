#!/usr/bin/env python3
"""Aggregate the SI5 probes into the numbers quoted by
`specs/step_import_si5_exact_analytic_ingestion.md` §2.

    scripts/si5_census_report.py si5_census.tsv [si5_exactness.tsv]

With the second argument it joins on path and reports the vertex-on-surface
residual distribution over the INGESTIBLE subset only — the all-models figure
is a different population (it includes models the vocabulary gate rejects) and
must not be quoted as SI5's.
"""
import csv
import math
import sys
from collections import Counter

SURF_EXACT = ["PLANE", "CYLINDRICAL_SURFACE", "CONICAL_SURFACE",
              "SPHERICAL_SURFACE", "TOROIDAL_SURFACE"]
SURF_OTHER = ["B_SPLINE_SURFACE_WITH_KNOTS", "RATIONAL_B_SPLINE_SURFACE",
              "BEZIER_SURFACE", "UNIFORM_SURFACE", "QUASI_UNIFORM_SURFACE",
              "SURFACE_OF_REVOLUTION", "SURFACE_OF_LINEAR_EXTRUSION",
              "OFFSET_SURFACE", "CURVE_BOUNDED_SURFACE",
              "RECTANGULAR_TRIMMED_SURFACE", "SWEPT_SURFACE"]
CURVE_EXACT = ["LINE", "CIRCLE", "ELLIPSE"]
CURVE_CONIC = ["PARABOLA", "HYPERBOLA"]
CURVE_OTHER = ["B_SPLINE_CURVE_WITH_KNOTS", "RATIONAL_B_SPLINE_CURVE",
               "BEZIER_CURVE", "POLYLINE", "TRIMMED_CURVE", "COMPOSITE_CURVE",
               "QUASI_UNIFORM_CURVE", "UNIFORM_CURVE"]
CURVE_WRAP = ["SURFACE_CURVE", "SEAM_CURVE", "INTERSECTION_CURVE", "PCURVE"]


def pct(n, d):
    return f"{100.0 * n / d:5.1f}%" if d else "    -"


def main():
    rows = list(csv.DictReader(open(sys.argv[1]), delimiter="\t"))
    n = len(rows)
    for r in rows:
        for k, v in r.items():
            if k not in ("path", "unit", "conv", "unclassified_geom"):
                r[k] = int(v)
        r["surf_exact"] = sum(r[c] for c in SURF_EXACT)
        r["surf_other"] = sum(r[c] for c in SURF_OTHER)
        r["curve_exact"] = sum(r[c] for c in CURVE_EXACT)
        r["curve_conic"] = sum(r[c] for c in CURVE_CONIC)
        r["curve_other"] = sum(r[c] for c in CURVE_OTHER)
        r["curve_wrap"] = sum(r[c] for c in CURVE_WRAP)
        r["faces"] = r["ADVANCED_FACE"] + r["FACE_SURFACE"]
        r["analytic_surf"] = r["surf_other"] == 0 and r["surf_exact"] > 0
        r["analytic_curve"] = r["curve_other"] == 0 and r["curve_conic"] == 0
        r["solid"] = r["MANIFOLD_SOLID_BREP"] > 0 or r["BREP_WITH_VOIDS"] > 0

    print(f"models: {n}\n")

    print("=== GATE 1: surfaces only (what the 2026-09-30 probe measured) ===")
    a_surf = [r for r in rows if r["analytic_surf"]]
    print(f"  analytic-surface-only : {len(a_surf):5d}  {pct(len(a_surf), n)}")
    print(f"  has freeform surface  : {n - len(a_surf):5d}  "
          f"{pct(n - len(a_surf), n)}\n")

    print("=== GATE 2: + every edge curve in {LINE, CIRCLE, ELLIPSE} ===")
    both = [r for r in a_surf if r["analytic_curve"]]
    print(f"  SI5-ingestible        : {len(both):5d}  {pct(len(both), n)}"
          f"   ({pct(len(both), len(a_surf))} of analytic-surface models)")
    lost = [r for r in a_surf if not r["analytic_curve"]]
    print(f"  lost to curve types   : {len(lost):5d}  {pct(len(lost), n)}")
    why = Counter()
    for r in lost:
        for c in CURVE_OTHER + CURVE_CONIC:
            if r[c]:
                why[c] += 1
    for c, k in why.most_common():
        print(f"      {c:32s} in {k:5d} models")
    print()

    print("=== GATE 2b: if PARABOLA/HYPERBOLA edges were also supported ===")
    relaxed = [r for r in a_surf
               if r["curve_other"] == 0]
    print(f"  ingestible            : {len(relaxed):5d}  {pct(len(relaxed), n)}"
          f"   (+{len(relaxed) - len(both)} over Gate 2)\n")

    print("=== Topology shapes SI5 assembly must handle (ingestible subset) ===")
    def share(pred, pool, label):
        k = sum(1 for r in pool if pred(r))
        print(f"  {label:38s} {k:5d}  {pct(k, len(pool))}")
    for pool, name in ((both, "SI5-ingestible"),):
        print(f"  -- {name} (n={len(pool)}) --")
        share(lambda r: r["VERTEX_LOOP"] > 0, pool, "has VERTEX_LOOP (degenerate loop)")
        share(lambda r: r["FACE_BOUND"] > 0, pool, "has inner loop (FACE_BOUND)")
        share(lambda r: r["BREP_WITH_VOIDS"] > 0, pool, "has voids (BREP_WITH_VOIDS)")
        share(lambda r: r["MANIFOLD_SOLID_BREP"] > 1, pool, "multi-solid")
        share(lambda r: not r["solid"], pool, "NO solid (shell/geom set only)")
        share(lambda r: r["curve_wrap"] > 0, pool, "has SURFACE_CURVE/SEAM_CURVE wrapper")
        share(lambda r: r["SPHERICAL_SURFACE"] > 0, pool, "has sphere")
        share(lambda r: r["TOROIDAL_SURFACE"] > 0, pool, "has torus")
        share(lambda r: r["CONICAL_SURFACE"] > 0, pool, "has cone")
        share(lambda r: r["PLANE"] == r["surf_exact"], pool, "planes only (polyhedral)")
    print()

    print("=== Size of the ingestible subset ===")
    fs = sorted(r["faces"] for r in both)
    if fs:
        def q(p):
            return fs[min(len(fs) - 1, int(p * len(fs)))]
        print(f"  faces p50={q(.5)} p90={q(.9)} p99={q(.99)} max={fs[-1]}")
    bs = sorted(r["bytes"] for r in both)
    if bs:
        def qb(p):
            return bs[min(len(bs) - 1, int(p * len(bs)))] // 1024
        print(f"  KB    p50={qb(.5)} p90={qb(.9)} p99={qb(.99)} max={bs[-1]//1024}")
    print()

    print("=== Units ===")
    for (u, c), k in Counter((r["unit"], r["conv"]) for r in rows).most_common(8):
        print(f"  SI_UNIT={u or 'NONE':8s} conv={c or '-':12s} {k:5d}  {pct(k, n)}")
    print()

    unk = Counter()
    for r in rows:
        if r["unclassified_geom"]:
            for t in r["unclassified_geom"].split(";"):
                unk[t] += 1
    print("=== Unclassified geometric entity names (census blind spots) ===")
    print("  none" if not unk else "")
    for t, k in unk.most_common(20):
        print(f"  {t:40s} {k:5d}")
    # A supertype token (BOUNDED_SURFACE, SURFACE, BOUNDED_CURVE) is harmless
    # only while it never appears without one of the subtypes we do track —
    # otherwise the gate leaks a freeform model into the ingestible set.
    leak = [r for r in rows if r["unclassified_geom"]
            and r["surf_other"] == 0 and r["curve_other"] == 0]
    print(f"  LEAK CHECK: ingestible rows carrying an unclassified geometric "
          f"token: {len(leak)}  (must be 0)")

    if len(sys.argv) > 2:
        exactness_report(sys.argv[2], {r["path"] for r in both})


def exactness_report(tsv, ingestible):
    """§2.2 — residuals over the ingestible subset only."""
    sub = [r for r in csv.DictReader(open(tsv), delimiter="\t")
           if r["path"] in ingestible and int(r["n"]) > 0]
    if not sub:
        print("\n=== Exactness: no ingestible rows in that file ===")
        return
    mx = sorted(float(r["max"]) for r in sub
                if not math.isnan(float(r["max"])))
    n_inc = sum(int(r["n"]) for r in sub)
    print(f"\n=== GATE 3: vertex-on-surface exactness (ingestible subset) ===")
    print(f"  models {len(sub)}, incidences measured {n_inc:,}")

    def q(p):
        return mx[min(len(mx) - 1, int(p * len(mx)))]
    print(f"  per-model max residual: p50={q(.5):.3e} p90={q(.9):.3e} "
          f"p99={q(.99):.3e} p99.9={q(.999):.3e} max={mx[-1]:.3e}")
    for label, band in (("1e-15", 1e-15),
                        ("1e-12  CURVED_SURFACE_DEBUG_TOLERANCE", 1e-12),
                        ("1e-9   TAU_EVAL / import_band", 1e-9),
                        ("1e-7   TAU_MODEL", 1e-7),
                        ("1e-6   MIN_FEATURE_SIZE", 1e-6)):
        k = sum(1 for v in mx if v <= band)
        print(f"  <= {label:38s} {k:5d}  {pct(k, len(mx))}")
    over = [r for r in sub if float(r["max"]) > 1e-9]
    print(f"  exceeding import_band -> loud refusal: {len(over)} models "
          f"({pct(len(over), len(mx))})")
    for r in sorted(over, key=lambda r: -float(r["max"]))[:6]:
        print(f"      {r['path'].split('/')[-2]} faces={r['faces']:>6s} "
              f"max={float(r['max']):.2e} declared={float(r['unc']):.0e} "
              f"{r['mix']}")


if __name__ == "__main__":
    main()
