#!/usr/bin/env python3
"""SI5: what loop/edge shape do curved STEP faces actually arrive in?

The arena does not just need the right SURFACE — it needs the face's boundary
in a form one of its per-surface routines accepts. kernel-v2's canonical
cylinder lateral (Stroud 2006 §3.1.4, `arena.rs:36-57`) is a FOUR-half-edge loop
`[rim, seam, rim, seam]` = two circles and two straight rulings. Whether STEP
already writes that form, or writes two separate single-circle loops with no
seam, decides how much seam-minting machinery SI5 needs (spec §5.1).

    scripts/si5_face_topology.py <corpus-root> [max_files] [SURFACE_TYPE]

Default surface is CYLINDRICAL_SURFACE. Curve letters: L=line, C=circle,
E=ellipse, ?=anything else (b-spline etc — those models are gate-rejected
anyway, but they are counted so the shares are honest).
"""
import glob
import os
import random
import re
import sys
from collections import Counter

# Shares one mini-reader with the exactness probe.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import importlib.util

_spec = importlib.util.spec_from_file_location(
    "si5x", os.path.join(os.path.dirname(os.path.abspath(__file__)),
                         "si5_exactness.py"))
si5x = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(si5x)

CURVE_LETTER = {"LINE": "L", "CIRCLE": "C", "ELLIPSE": "E"}
# Skip the giant tail: this probe is about shape, and a 300 MB model's shape is
# the same as a 300 KB one's. The size distribution lives in si5_census.py.
MAX_BYTES = 400_000


def main():
    root = sys.argv[1]
    cap = int(sys.argv[2]) if len(sys.argv) > 2 else 120
    want = sys.argv[3] if len(sys.argv) > 3 else "CYLINDRICAL_SURFACE"

    files = sorted(glob.glob(os.path.join(root, "*", "*.step"))) or \
        sorted(glob.glob(os.path.join(root, "**", "*.step"), recursive=True))
    # Deterministic sample: seeded shuffle, so the number is reproducible.
    random.seed(0)
    random.shuffle(files)

    shape = Counter()
    curves = Counter()
    nloops = Counter()
    scanned = 0
    faces = 0

    for path in files:
        if scanned >= cap:
            break
        try:
            text = open(path, errors="replace").read()
        except OSError:
            continue
        if want not in text or len(text) > MAX_BYTES:
            continue
        scanned += 1
        ent = {int(x.group(1)): x.group(2) for x in si5x.STMT.finditer(text)}
        cache = {}

        def get(i):
            if i not in cache:
                raw = ent.get(i)
                cache[i] = (None, []) if raw is None else si5x.body(raw)
            return cache[i]

        for i in list(ent):
            n, a = get(i)
            if n not in ("ADVANCED_FACE", "FACE_SURFACE") or len(a) < 3:
                continue
            si = si5x.ref(a[2])
            if not si or get(si)[0] != want:
                continue
            faces += 1
            sizes = []
            for bref in [si5x.ref(t)
                         for t in si5x.split_args(a[1].strip("() "))
                         if si5x.ref(t)]:
                bn, ba = get(bref)
                if bn not in ("FACE_OUTER_BOUND", "FACE_BOUND") or not ba:
                    continue
                ln, la = get(si5x.ref(ba[1]))
                if ln != "EDGE_LOOP":
                    sizes.append(0)       # VERTEX_LOOP: degenerate
                    curves["(vertex loop)"] += 1
                    continue
                kinds = []
                for oe in [si5x.ref(t)
                           for t in si5x.split_args(la[1].strip("() "))
                           if si5x.ref(t)]:
                    on, oa = get(oe)
                    if on != "ORIENTED_EDGE" or len(oa) < 4:
                        continue
                    en, ea = get(si5x.ref(oa[3]))
                    if en != "EDGE_CURVE" or len(ea) < 4:
                        continue
                    cr = si5x.ref(ea[3])
                    kinds.append(CURVE_LETTER.get(get(cr)[0] if cr else None, "?"))
                sizes.append(len(kinds))
                curves["".join(sorted(kinds))] += 1
            nloops[len(sizes)] += 1
            shape[tuple(sorted(sizes))] += 1

    print(f"files scanned containing {want}: {scanned}   faces: {faces}\n")
    print("loops per face:", dict(sorted(nloops.items())))
    print("\nloop-size signature (sorted tuple) -> faces:")
    for k, v in shape.most_common(12):
        print(f"   {str(k):16s} {v:6d}  {100.0 * v / max(faces, 1):5.1f}%")
    print("\nper-loop curve multiset -> loops:")
    for k, v in curves.most_common(12):
        print(f"   {k or '(empty)':14s} {v:6d}")
    print("\nCCLL = kernel-v2's canonical lateral; 'C'+'C' as two loops = a full"
          "\nband whose seam SI5 must mint (spec §5.1).")


if __name__ == "__main__":
    main()
