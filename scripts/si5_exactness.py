#!/usr/bin/env python3
"""Measure how far a STEP file's declared vertices lie OFF its declared
analytic surfaces.

This is the load-bearing question for SI5 (exact ingestion): the arena wants a
vertex to lie on every incident surface. The file gives both, independently
rounded. If the residual is ~1e-15 we can snap and be exact; if it is ~1e-6 the
file is only self-consistent to its own declared uncertainty and SI5 needs a
real reconciliation story (re-derive the vertex from its incident surfaces).

Deliberately a standalone mini-reader for the analytic subset only — no truck,
no kernel. It doubles as a feasibility probe for the entity subset a
first-party reader would need.

Usage: si5_exactness.py <corpus-root> [max_models]
Emits TSV: path, faces, verts_checked, p50, p90, max residual (metres),
           declared_uncertainty, surface mix
"""
import math
import os
import re
import sys
from concurrent.futures import ProcessPoolExecutor

STMT = re.compile(r"#(\d+)\s*=\s*(.*?);", re.S)
NAME = re.compile(r"^\s*([A-Z_0-9]+)\s*\(")
# Top-level comma split that respects nesting and quotes.


def split_args(s):
    out, depth, cur, inq = [], 0, [], False
    for ch in s:
        if inq:
            cur.append(ch)
            if ch == "'":
                inq = False
            continue
        if ch == "'":
            inq = True
            cur.append(ch)
        elif ch == "(":
            depth += 1
            cur.append(ch)
        elif ch == ")":
            depth -= 1
            cur.append(ch)
        elif ch == "," and depth == 0:
            out.append("".join(cur).strip())
            cur = []
        else:
            cur.append(ch)
    if cur:
        out.append("".join(cur).strip())
    return out


def body(expr):
    """ENTITY( a, b ) -> ('ENTITY', [a, b]); complex instance -> ('*', raw)."""
    expr = expr.strip()
    m = NAME.match(expr)
    if not m:
        return None, expr
    inner = expr[m.end():]
    assert inner.endswith(")") or True
    depth, end = 1, None
    for i, ch in enumerate(inner):
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
            if depth == 0:
                end = i
                break
    return m.group(1), split_args(inner[:end] if end is not None else inner)


def ref(tok):
    tok = tok.strip()
    return int(tok[1:]) if tok.startswith("#") else None


# NOTE: must match OpenCascade's `-1.E-02` form (digits, point, NO fractional
# digits, then exponent) as ONE number. An earlier regex requiring digits after
# the point split that into `-1.` and `-02`, which silently fabricated a 4th
# coordinate and read a 9.9 m residual on USB_C.step. Onshape writes full
# digits, so the bug was invisible on ABC and only showed on the KiCad files.
NUM = re.compile(r"[-+]?(?:\d+\.\d*|\.\d+|\d+)(?:[EeDd][-+]?\d+)?")


def nums(tok):
    return [float(x) for x in NUM.findall(tok.replace("D", "E"))]


def unit(v):
    n = math.sqrt(sum(c * c for c in v))
    return [c / n for c in v] if n else v


def sub(a, b):
    return [a[0] - b[0], a[1] - b[1], a[2] - b[2]]


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0]]


def norm(a):
    return math.sqrt(dot(a, a))


def analyse(path):
    try:
        with open(path, "r", errors="replace") as fh:
            text = fh.read()
    except OSError as exc:
        return {"path": path, "err": str(exc)}

    ent = {}
    for m in STMT.finditer(text):
        ent[int(m.group(1))] = m.group(2)

    cache = {}

    def get(i):
        if i in cache:
            return cache[i]
        raw = ent.get(i)
        cache[i] = (None, []) if raw is None else body(raw)
        return cache[i]

    def point(i):
        n, a = get(i)
        return nums(a[1]) if n == "CARTESIAN_POINT" and len(a) > 1 else None

    def direction(i):
        n, a = get(i)
        return unit(nums(a[1])) if n == "DIRECTION" and len(a) > 1 else None

    def placement(i):
        """AXIS2_PLACEMENT_3D -> (origin, axis z, ref x)."""
        n, a = get(i)
        if n != "AXIS2_PLACEMENT_3D" or len(a) < 3:
            return None
        o = point(ref(a[1]))
        z = direction(ref(a[2])) if ref(a[2]) else [0.0, 0.0, 1.0]
        x = direction(ref(a[3])) if len(a) > 3 and ref(a[3]) else None
        return None if o is None or z is None else (o, z, x)

    def dist_to_surface(si, p):
        """Signed-magnitude distance from p to the analytic surface at si."""
        n, a = get(si)
        if n == "PLANE":
            pl = placement(ref(a[1]))
            return None if not pl else abs(dot(sub(p, pl[0]), pl[1]))
        if n == "CYLINDRICAL_SURFACE":
            pl, r = placement(ref(a[1])), nums(a[2])[0]
            if not pl:
                return None
            d = sub(p, pl[0])
            return abs(norm(cross(d, pl[1])) - r)
        if n == "SPHERICAL_SURFACE":
            pl, r = placement(ref(a[1])), nums(a[2])[0]
            return None if not pl else abs(norm(sub(p, pl[0])) - r)
        if n == "CONICAL_SURFACE":
            pl = placement(ref(a[1]))
            if not pl:
                return None
            r, half = nums(a[2])[0], nums(a[3])[0]
            d = sub(p, pl[0])
            h = dot(d, pl[1])
            radial = norm(cross(d, pl[1]))
            # distance to the cone surface, measured perpendicular to the ruling
            return abs(radial - (r + h * math.tan(half))) * math.cos(half)
        if n == "TOROIDAL_SURFACE":
            pl = placement(ref(a[1]))
            if not pl:
                return None
            rmaj, rmin = nums(a[2])[0], nums(a[3])[0]
            d = sub(p, pl[0])
            h = dot(d, pl[1])
            radial = norm(cross(d, pl[1]))
            return abs(math.hypot(radial - rmaj, h) - rmin)
        return None

    # Walk faces -> bounds -> loops -> oriented edges -> edge curve -> vertices.
    residuals = []
    faces = 0
    mix = set()
    for i, raw in ent.items():
        n, a = get(i)
        if n not in ("ADVANCED_FACE", "FACE_SURFACE") or len(a) < 3:
            continue
        si = ref(a[2])
        sname, _ = get(si) if si else (None, None)
        if sname not in ("PLANE", "CYLINDRICAL_SURFACE", "CONICAL_SURFACE",
                         "SPHERICAL_SURFACE", "TOROIDAL_SURFACE"):
            continue
        faces += 1
        mix.add(sname)
        for bref in [ref(t) for t in split_args(a[1].strip("() ")) if ref(t)]:
            bn, ba = get(bref)
            if bn not in ("FACE_OUTER_BOUND", "FACE_BOUND") or not ba:
                continue
            ln, la = get(ref(ba[1]))
            edges = []
            if ln == "EDGE_LOOP":
                edges = [ref(t) for t in split_args(la[1].strip("() ")) if ref(t)]
            elif ln == "VERTEX_LOOP":
                vn, va = get(ref(la[1]))
                p = point(ref(va[1])) if vn == "VERTEX_POINT" else None
                if p:
                    d = dist_to_surface(si, p)
                    if d is not None:
                        residuals.append(d)
                continue
            for oe in edges:
                on, oa = get(oe)
                if on != "ORIENTED_EDGE" or len(oa) < 4:
                    continue
                en, ea = get(ref(oa[3]))
                if en != "EDGE_CURVE" or len(ea) < 3:
                    continue
                for vt in (ref(ea[1]), ref(ea[2])):
                    vn, va = get(vt) if vt else (None, None)
                    if vn != "VERTEX_POINT":
                        continue
                    p = point(ref(va[1]))
                    if p is None:
                        continue
                    d = dist_to_surface(si, p)
                    if d is not None:
                        residuals.append(d)

    unc = re.search(r"LENGTH_MEASURE\(\s*([0-9.eE+-]+)", text)
    residuals.sort()

    def q(p):
        return residuals[min(len(residuals) - 1, int(p * len(residuals)))] \
            if residuals else float("nan")

    return {"path": path, "faces": faces, "n": len(residuals),
            "p50": q(.50), "p90": q(.90), "p99": q(.99),
            "max": residuals[-1] if residuals else float("nan"),
            "unc": float(unc.group(1)) if unc else float("nan"),
            "mix": ",".join(sorted(s[:4] for s in mix))}


def main():
    root = sys.argv[1]
    cap = int(sys.argv[2]) if len(sys.argv) > 2 else 10 ** 9
    paths = []
    for dirpath, _, names in os.walk(root):
        for nm in names:
            if nm.lower().endswith((".step", ".stp")):
                paths.append(os.path.join(dirpath, nm))
    paths.sort()
    paths = paths[:cap]
    cols = ["path", "faces", "n", "p50", "p90", "p99", "max", "unc", "mix"]
    print("\t".join(cols))
    with ProcessPoolExecutor(max_workers=int(os.environ.get("JOBS", "12"))) as ex:
        for r in ex.map(analyse, paths, chunksize=4):
            if "err" in r:
                sys.stderr.write(f"ERR\t{r['path']}\t{r['err']}\n")
                continue
            print("\t".join(f"{r[c]:.6e}" if isinstance(r[c], float) else str(r[c])
                            for c in cols))


if __name__ == "__main__":
    main()
