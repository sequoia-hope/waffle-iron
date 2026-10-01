#!/usr/bin/env python3
"""SI5 ingestibility census over a STEP corpus.

Pure text census of ISO-10303-21 entity names — no parser, no truck, no
tessellation. Answers: what fraction of the corpus is ingestible as an EXACT
kernel-v2 arena solid, where ingestible means every surface AND every curve
falls inside the kernel's exact vocabulary.

The 2026-09-30 probe measured surfaces only (67.3% analytic-only). The curve
dimension is unmeasured and is an independent gate on SI5's reach.

One TSV row per model on stdout; aggregate with si5_census_report.py.
"""
import os
import re
import sys
from concurrent.futures import ProcessPoolExecutor

# kernel-v2 exact Surface vocabulary (plus sphere, in flight).
SURF_EXACT = ["PLANE", "CYLINDRICAL_SURFACE", "CONICAL_SURFACE",
              "SPHERICAL_SURFACE", "TOROIDAL_SURFACE"]
# `B_SPLINE_SURFACE` / `B_SPLINE_CURVE` bare are the complex-instance members;
# they are the only trace a RATIONAL b-spline leaves.
SURF_OTHER = ["B_SPLINE_SURFACE", "B_SPLINE_SURFACE_WITH_KNOTS",
              "RATIONAL_B_SPLINE_SURFACE",
              "BEZIER_SURFACE", "UNIFORM_SURFACE", "QUASI_UNIFORM_SURFACE",
              "SURFACE_OF_REVOLUTION", "SURFACE_OF_LINEAR_EXTRUSION",
              "OFFSET_SURFACE", "CURVE_BOUNDED_SURFACE",
              "RECTANGULAR_TRIMMED_SURFACE", "SWEPT_SURFACE"]
# Curves the kernel can hold exactly on an edge.
CURVE_EXACT = ["LINE", "CIRCLE", "ELLIPSE"]
# Conics we have SSI solvers for but may not hold as an edge curve.
CURVE_CONIC = ["PARABOLA", "HYPERBOLA"]
CURVE_OTHER = ["B_SPLINE_CURVE", "B_SPLINE_CURVE_WITH_KNOTS",
               "RATIONAL_B_SPLINE_CURVE",
               "BEZIER_CURVE", "POLYLINE", "TRIMMED_CURVE", "COMPOSITE_CURVE",
               "QUASI_UNIFORM_CURVE", "UNIFORM_CURVE"]
# Wrappers: an edge geometry that *references* a basis curve + pcurves.
CURVE_WRAP = ["SURFACE_CURVE", "SEAM_CURVE", "INTERSECTION_CURVE", "PCURVE"]
# Topology shapes that decide how hard assembly is.
TOPO = ["ADVANCED_FACE", "FACE_SURFACE", "CLOSED_SHELL", "OPEN_SHELL",
        "MANIFOLD_SOLID_BREP", "BREP_WITH_VOIDS", "SHELL_BASED_SURFACE_MODEL",
        "FACE_OUTER_BOUND", "FACE_BOUND", "VERTEX_LOOP", "EDGE_LOOP",
        "EDGE_CURVE", "ORIENTED_EDGE", "VERTEX_POINT",
        "MANIFOLD_SURFACE_SHAPE_REPRESENTATION", "GEOMETRIC_SET"]

COLS = SURF_EXACT + SURF_OTHER + CURVE_EXACT + CURVE_CONIC + CURVE_OTHER + \
    CURVE_WRAP + TOPO

# Entity names are counted as TOKENS, not as `= NAME(` instance heads.
#
# A rational B-spline surface is written ONLY as a complex instance:
#   #9 = ( BOUNDED_SURFACE() B_SPLINE_SURFACE(..) B_SPLINE_SURFACE_WITH_KNOTS(..)
#          RATIONAL_B_SPLINE_SURFACE(..) SURFACE() );
# so nothing follows the `=` but a paren. An `= NAME(` scan therefore misses
# every rational b-spline and over-reports the analytic share. Verified on the
# corpus: `= B_SPLINE_SURFACE(` occurs 0 times, the token 84 times, in one file.
# Any uppercase entity-ish token, for the blind-spot sweep.
ANYNAME = re.compile(rb"(?<![A-Z0-9_])([A-Z][A-Z0-9_]{2,})\s*\(")
# `SI_UNIT( $, .METRE. )` — prefix is `$` when unset, `.MILLI.` etc otherwise.
UNIT = re.compile(rb"SI_UNIT\s*\(\s*(\$|\.[A-Z]+\.)\s*,\s*\.METRE\.")
CONV = re.compile(rb"CONVERSION_BASED_UNIT\s*\(\s*'([^']*)'")


# Tokens that are inline constructed values or admin entities, not geometry —
# excluded from the blind-spot sweep so it stays readable.
NOISE = re.compile(r"_MEASURE$|_UNIT$|^SI_UNIT$|^NAMED_UNIT$")


def census(path):
    try:
        with open(path, "rb") as fh:
            blob = fh.read()
    except OSError as exc:
        return {"path": path, "error": str(exc)}
    counts = {}
    for name in ANYNAME.findall(blob):
        key = name.decode("ascii", "replace")
        counts[key] = counts.get(key, 0) + 1
    unit = UNIT.search(blob)
    conv = CONV.search(blob)
    row = {"path": path, "bytes": len(blob),
           "unit": unit.group(1).decode() if unit else "ABSENT",
           "conv": conv.group(1).decode() if conv else ""}
    for col in COLS:
        row[col] = counts.get(col, 0)
    # Anything we did not classify but that looks geometric, so the census
    # cannot silently miss a surface/curve type we have never seen.
    row["unclassified_geom"] = ";".join(sorted(
        k for k, v in counts.items()
        if k not in COLS and v > 0 and not NOISE.search(k)
        and (k.endswith("_SURFACE") or k.endswith("_CURVE")
             or k.endswith("_SURFACE_WITH_KNOTS") or k == "SURFACE"
             or k in ("PLANE", "LINE", "CIRCLE", "ELLIPSE", "PARABOLA",
                      "HYPERBOLA", "POLYLINE"))))
    return row


def main():
    root = sys.argv[1]
    paths = []
    for dirpath, _, names in os.walk(root):
        for n in names:
            if n.lower().endswith((".step", ".stp")):
                paths.append(os.path.join(dirpath, n))
    paths.sort()
    header = ["path", "bytes", "unit", "conv"] + COLS + ["unclassified_geom"]
    out = sys.stdout
    out.write("\t".join(header) + "\n")
    with ProcessPoolExecutor(max_workers=int(os.environ.get("JOBS", "12"))) as ex:
        for row in ex.map(census, paths, chunksize=16):
            if "error" in row:
                sys.stderr.write(f"ERROR\t{row['path']}\t{row['error']}\n")
                continue
            out.write("\t".join(str(row[c]) for c in header) + "\n")


if __name__ == "__main__":
    main()
