#!/usr/bin/env python3
"""SI5 C4: the exact boundary shape of cylindrical/conical faces, measured.

§5.1 read the loop-size signatures (`si5_face_topology.py`) and concluded that
the dominant `CCLL` form — one loop of circle, circle, line, line — "already
matches" kernel-v2's canonical lateral (Stroud 2006 §3.1.4: two CLOSED rim
circles and one seam ruling traversed twice). **That inference is wrong, and
this probe is what refutes it.** Going one level deeper — to each edge's own
start/end vertices and record identity — shows the `CCLL` faces are not full
bands at all:

  * the two `L` edges are two DISTINCT `EDGE_CURVE` records, never one seam
    traversed twice, and
  * the two `C` edges are OPEN (`start != end`), i.e. circular ARCS.

So `CCLL` is the *partial* patch — a fillet, a half-round, a rounded corner —
bounded by two arcs and two rulings, and the full band always arrives in the
2×single-closed-circle form that needs a minted seam. A signature census cannot
see the difference; only the vertex identities can.

What this probe measures, each fact deciding a piece of C4 machinery:

1. **Form taxonomy per face** — FULL BAND (every loop one closed circle),
   ARC PATCH (no closed circle anywhere), or MIXED/OTHER.
2. **In a full band, the azimuth gap between the two rim anchor vertices.**
   The minted seam must be a RULING (`validate_cylinder_face` checks it), so the
   anchors have to share an azimuth. A nonzero gap means minting must also
   RE-ANCHOR a closed rim edge — legitimate gauge (the fake-edge position
   carries no geometry) only if that vertex is not load-bearing elsewhere.
3. **Is a rim anchor vertex named by any other edge?** If it is, re-anchoring
   would move a vertex another face's boundary depends on, and the face must be
   refused instead.
4. **Arc sweeps in an arc patch**, because the exactly-half-turn arc (which the
   first sample files are full of) is the case endpoint geometry alone cannot
   orient — the reason `AnalyticCurve` carries an `interior` point.
5. **Per-model reach**: how many whole models land entirely inside each
   candidate C4 vocabulary, which is what decides the checkpoint's scope.

    scripts/si5_seam_shape.py <corpus-root> [max_files]

Mini-reader shared with `si5_exactness.py` (no truck, no kernel) — the same
standalone-probe discipline, and the same `-1.E-02` number rule.
"""
import glob
import importlib.util
import math
import os
import random
import sys
from collections import Counter

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location(
    "si5x", os.path.join(os.path.dirname(os.path.abspath(__file__)),
                         "si5_exactness.py"))
si5x = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(si5x)

MAX_BYTES = 400_000
CURVED = ("CYLINDRICAL_SURFACE", "CONICAL_SURFACE")
C4_SURFACES = ("PLANE",) + CURVED
C4_CURVES = ("LINE", "CIRCLE")


def classify(loops):
    """loops = [[(curve_name, edge_ref, v_start, v_end)]] -> a form label."""
    if not loops:
        return "no loops"
    closed = sum(1 for lp in loops for (_, _, vs, ve) in lp if vs == ve)
    total = sum(len(lp) for lp in loops)
    if closed == 0:
        return "arc patch"
    if closed == total and all(len(lp) == 1 for lp in loops):
        return f"full band ({len(loops)} rim loop(s))"
    return "mixed"


def main():
    root = sys.argv[1]
    cap = int(sys.argv[2]) if len(sys.argv) > 2 else 200

    files = sorted(glob.glob(os.path.join(root, "*", "*.step"))) or \
        sorted(glob.glob(os.path.join(root, "**", "*.step"), recursive=True))
    random.seed(0)
    random.shuffle(files)

    form = Counter()
    curve_mix = Counter()
    azimuth = []
    anchor_shared = Counter()
    sweeps = Counter()
    model_verdict = Counter()
    on_circle = []
    scanned = 0
    faces = 0

    for path in files:
        if scanned >= cap:
            break
        try:
            text = open(path, errors="replace").read()
        except OSError:
            continue
        if len(text) > MAX_BYTES or not any(w in text for w in CURVED):
            continue
        scanned += 1
        ent = {int(x.group(1)): x.group(2) for x in si5x.STMT.finditer(text)}
        cache = {}

        def get(i):
            if i not in cache:
                raw = ent.get(i)
                cache[i] = (None, []) if raw is None else si5x.body(raw)
            return cache[i]

        def point(i):
            n, a = get(i)
            return si5x.nums(a[1]) if n == "CARTESIAN_POINT" and len(a) > 1 else None

        def direction(i):
            n, a = get(i)
            return si5x.unit(si5x.nums(a[1])) if n == "DIRECTION" and len(a) > 1 else None

        def placement(i):
            n, a = get(i)
            if n != "AXIS2_PLACEMENT_3D" or len(a) < 3:
                return None
            o = point(si5x.ref(a[1]))
            z = direction(si5x.ref(a[2])) if si5x.ref(a[2]) else [0.0, 0.0, 1.0]
            x = direction(si5x.ref(a[3])) if len(a) > 3 and si5x.ref(a[3]) else None
            return None if o is None or z is None else (o, z, x)

        def vertex_xyz(i):
            n, a = get(i)
            return point(si5x.ref(a[1])) if n == "VERTEX_POINT" and len(a) > 1 else None

        # Times each vertex is named by an EDGE_CURVE, over the whole file: a
        # closed rim's own anchor scores 2 (its start and its end), so >2 means
        # the anchor is load-bearing for some other edge too (question 3).
        vertex_uses = Counter()
        for i in list(ent):
            n, a = get(i)
            if n == "EDGE_CURVE" and len(a) >= 4:
                for t in (a[1], a[2]):
                    r = si5x.ref(t)
                    if r is not None:
                        vertex_uses[r] += 1

        model_surfaces, model_curves = set(), set()
        model_forms = set()

        for i in list(ent):
            n, a = get(i)
            if n not in ("ADVANCED_FACE", "FACE_SURFACE") or len(a) < 3:
                continue
            si = si5x.ref(a[2])
            if not si:
                continue
            sname, sargs = get(si)
            model_surfaces.add(sname)
            if sname not in CURVED:
                continue
            pl = placement(si5x.ref(sargs[1])) if len(sargs) > 1 else None
            if not pl:
                continue
            faces += 1
            origin, axis, refx = pl
            if refx is None:
                refx = [1.0, 0.0, 0.0]
            rx = si5x.sub(refx, [axis[k] * si5x.dot(refx, axis) for k in range(3)])
            if si5x.norm(rx) < 1e-12:
                continue
            rx = si5x.unit(rx)
            ry = si5x.cross(axis, rx)

            def azim(p):
                d = si5x.sub(p, origin)
                return math.atan2(si5x.dot(d, ry), si5x.dot(d, rx))

            loops, degenerate = [], False
            for bref in [si5x.ref(t) for t in si5x.split_args(a[1].strip("() "))
                         if si5x.ref(t)]:
                bn, ba = get(bref)
                if bn not in ("FACE_OUTER_BOUND", "FACE_BOUND") or len(ba) < 2:
                    continue
                ln, la = get(si5x.ref(ba[1]))
                if ln != "EDGE_LOOP":
                    degenerate = True
                    continue
                lp = []
                for oe in [si5x.ref(t) for t in si5x.split_args(la[1].strip("() "))
                           if si5x.ref(t)]:
                    on, oa = get(oe)
                    if on != "ORIENTED_EDGE" or len(oa) < 4:
                        degenerate = True
                        continue
                    er = si5x.ref(oa[3])
                    en, ea = get(er)
                    if en != "EDGE_CURVE" or len(ea) < 4:
                        degenerate = True
                        continue
                    cr = si5x.ref(ea[3])
                    cn = (get(cr)[0] or "?") if cr else "?"
                    lp.append((cn, er, si5x.ref(ea[1]), si5x.ref(ea[2])))
                loops.append(lp)
            if degenerate or not loops:
                form["(unreadable / vertex loop)"] += 1
                model_forms.add("other")
                continue

            label = classify(loops)
            form[(sname[:4], label)] += 1
            curve_mix[(label, tuple(sorted({c for lp in loops for (c, _, _, _) in lp})))] += 1
            model_forms.add(label if label.startswith(("arc patch", "full band"))
                            else "other")

            # (6) Is a closed rim's anchor vertex actually ON its own circle?
            # Spec §2.2 measured vertex-vs-SURFACE residuals and nothing else;
            # the vertex-vs-EDGE-CURVE dimension is independent, and it is the
            # one an exact assembler trips over when it places a seam foot.
            for lp in loops:
                for (cn, er, vs, ve) in lp:
                    if cn != "CIRCLE" or vs != ve:
                        continue
                    pa = vertex_xyz(vs)
                    _, ea = get(er)
                    _, cargs = get(si5x.ref(ea[3]))
                    cpl = placement(si5x.ref(cargs[1]))
                    if not (pa and cpl):
                        continue
                    r = si5x.nums(cargs[2])[0]
                    d = si5x.sub(pa, cpl[0])
                    axial = abs(si5x.dot(d, cpl[1]))
                    radial = si5x.norm(si5x.cross(d, cpl[1]))
                    on_circle.append((math.hypot(radial - r, axial), max(r, 1.0)))

            if label == "full band (2 rim loop(s))":
                anchors = [lp[0][2] for lp in loops]
                pts = [vertex_xyz(v) for v in anchors]
                if all(p and len(p) == 3 for p in pts):
                    gap = abs(azim(pts[0]) - azim(pts[1])) % (2 * math.pi)
                    azimuth.append(min(gap, 2 * math.pi - gap))
                for v in anchors:
                    anchor_shared["anchor also used by another edge"
                                  if vertex_uses[v] > 2
                                  else "anchor used by its own rim only"] += 1

            if label == "arc patch":
                for lp in loops:
                    for (cn, er, vs, ve) in lp:
                        if cn != "CIRCLE":
                            continue
                        pa, pb = vertex_xyz(vs), vertex_xyz(ve)
                        _, ca = get(er)
                        cr = si5x.ref(ca[3])
                        _, cargs = get(cr)
                        cpl = placement(si5x.ref(cargs[1]))
                        if not (pa and pb and cpl):
                            continue
                        r = si5x.nums(cargs[2])[0]
                        chord = si5x.norm(si5x.sub(pa, pb))
                        if r <= 0:
                            continue
                        ratio = min(1.0, chord / (2 * r))
                        half = math.degrees(2 * math.asin(ratio))
                        if chord < 1e-12 * max(r, 1.0):
                            sweeps["full turn (chord 0)"] += 1
                        elif abs(half - 180.0) < 1e-6:
                            sweeps["EXACTLY half turn (chord = 2r)"] += 1
                        elif half > 179.0:
                            sweeps["near half turn (179-180 deg)"] += 1
                        else:
                            sweeps[f"chord implies <= {int(half // 30 * 30 + 30)} deg"] += 1

        for i in list(ent):
            n, a = get(i)
            if n == "EDGE_CURVE" and len(a) >= 4:
                cr = si5x.ref(a[3])
                model_curves.add((get(cr)[0] or "?") if cr else "?")

        in_vocab = (model_surfaces <= set(C4_SURFACES)
                    and model_curves <= set(C4_CURVES)
                    and "VERTEX_LOOP" not in text)
        if not in_vocab:
            model_verdict["outside the C4 vocabulary"] += 1
        elif model_forms <= {"arc patch"}:
            model_verdict["in vocab: arc patches only"] += 1
        elif model_forms <= {"full band (2 rim loop(s))", "full band (1 rim loop(s))"}:
            model_verdict["in vocab: full bands only"] += 1
        elif model_forms <= {"arc patch", "full band (2 rim loop(s))",
                             "full band (1 rim loop(s))"}:
            model_verdict["in vocab: both forms"] += 1
        else:
            model_verdict["in vocab but a face form is outside both"] += 1

    print(f"files scanned: {scanned}   cylindrical/conical faces: {faces}\n")

    print("(1) face form:")
    for k, v in form.most_common():
        print(f"    {str(k):44s} {v:6d}  {100.0 * v / max(faces, 1):5.1f}%")

    print("\n    curve vocabulary per form:")
    for k, v in curve_mix.most_common(10):
        print(f"      {str(k):60s} {v:6d}")

    print("\n(2) full band — |azimuth gap| between the two rim anchors (radians)")
    if azimuth:
        azimuth.sort()
        n = len(azimuth)
        aligned = sum(1 for g in azimuth if g < 1e-9)
        print(f"    bands: {n}   already aligned (<1e-9): {aligned} "
              f"({100.0 * aligned / n:.1f}%)")
        print(f"    p50 {azimuth[n // 2]:.6f}   p90 {azimuth[min(n - 1, int(0.9 * n))]:.6f}"
              f"   max {azimuth[-1]:.6f}")
    else:
        print("    (none in this sample)")

    print("\n(3) is a rim anchor vertex shared with another edge?")
    for k, v in anchor_shared.most_common():
        print(f"    {k:36s} {v:6d}")

    print("\n(4) arc sweep, from the chord (an exact half turn is the ambiguous one)")
    for k, v in sweeps.most_common():
        print(f"    {k:36s} {v:6d}")

    print("\n(6) closed-rim anchor vs its OWN circle (absolute metres)")
    if on_circle:
        vals = sorted(x for x, _ in on_circle)
        n = len(vals)
        print(f"    anchors: {n}")
        for q in (0.5, 0.9, 0.99):
            print(f"    p{int(q * 100):<3} {vals[min(n - 1, int(q * n))]:.3e}")
        print(f"    max  {vals[-1]:.3e}")
        for band, name in ((1e-12, "CURVED_SURFACE_DEBUG_TOLERANCE"),
                           (1e-9, "import_band / TAU_EVAL")):
            inside = sum(1 for x in vals if x <= band)
            print(f"    within {band:.0e} ({name}): {inside} ({100.0 * inside / n:.1f}%)")
    else:
        print("    (none in this sample)")

    print("\n(5) per-model verdict:")
    tot = sum(model_verdict.values())
    for k, v in model_verdict.most_common():
        print(f"    {k:44s} {v:5d}  {100.0 * v / max(tot, 1):5.1f}%")


if __name__ == "__main__":
    main()
