"""Eiffel Tower — an official Waffle Iron example, and its generator.

A structural model of the 1889 tower at full scale (metres), built from the
published dimensions: a 124.90 m base square, platforms at 57.63 m, 115.73 m,
196.10 m and 276.13 m, the finial of the original structure at 300.65 m, and
the 2022 broadcast mast reaching 330 m.

Ten Part tabs — Piers, Legs, Arches, First platform, Second platform, Upper
pylon, Intermediate platform, Top platform, Campanile, Antenna mast — and one
Assembly tab that fastens them at a shared ground connector. Every tab models
ONE quarter of the tower and raises it to four with a PatternCircular about the
vertical axis, which is how 1,964 riveted members come out of 1,052 tool calls.
There are no booleans anywhere: members are overlapping NewBody solids, exactly
as a riveted lattice is a pile of overlapping angle irons.

The silhouette is not eyeballed. Eiffel sized the legs so that the wind moment
at every height is carried by the leg axes alone, which makes the lower profile
very nearly exponential; `half()` fits a log-quadratic through the three
measured lower widths (124.90 / 70.69 / 40.96) and a pure exponential through
the two upper ones (40.96 / 18.65), so the curve passes through every published
anchor by construction rather than by eye.

Two ways to use it:

    python3 eiffel-tower.py recipe.json
        Emit, per tab, the ordered agent-tool calls (the MCP tools of
        docs/AGENT_LINK.md) that build the tower. "$SK" = the sketch feature id
        returned by the sketch_create just before; "$F:<label>" = the feature id
        of an earlier call in the same tab with that label; "$TAB:<name>",
        "$INST:<name>", "$MC:<part>/<connector>" and "$CONN:<instance>/<connector>"
        are the assembly's tab, instance, MateConnector-feature and connector ids.

    python3 eiffel-tower.py --build out.waffle [--host target/release/waffle-host]
        Build the document headless: drive the native host over its stdio
        frames (specs/waffle_server_mode.md §3.4) with the same calls and copy
        the .waffle it autosaves to `out.waffle`. This is how
        app/static/examples/eiffel-tower.waffle was produced. `--tabs a,b`
        builds only those tabs (a debugging convenience — the result is a
        partial document, not something to ship).

World frame: metres, Z up, the origin at the centre of the base square at
ground level, the tower's four faces parallel to the X and Y axes and the four
legs in the four quadrants.
"""
import json, math, sys

def add(a, b): return [a[i] + b[i] for i in range(3)]
def sub(a, b): return [a[i] - b[i] for i in range(3)]
def mul(a, s): return [a[i] * s for i in range(3)]
def dot(a, b): return sum(a[i] * b[i] for i in range(3))
def cross(a, b): return [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
def norm(a): return math.sqrt(dot(a, a))
def unit(a): return mul(a, 1 / norm(a))

def basis(n):
    n = unit(n)
    ref = [0.0, 0.0, 1.0] if abs(n[2]) < 0.99 else [1.0, 0.0, 0.0]
    x = unit(cross(ref, n)); y = unit(cross(n, x))
    return x, y, n

def uv(origin, normal, p):
    x, y, _ = basis(normal); r = sub(p, origin)
    return [dot(r, x), dot(r, y)]

def lerp(z, table):
    """Piecewise-linear interpolation of (height, value) pairs, clamped at both ends."""
    if z <= table[0][0]: return table[0][1]
    for (z0, v0), (z1, v1) in zip(table, table[1:]):
        if z <= z1: return v0 + (v1 - v0) * (z - z0) / (z1 - z0)
    return table[-1][1]

NEWBODY = {"combine": {"type": "NewBody"}, "targets": [], "symmetric": False, "cut": False, "merge": False}
X, Y, Z = [1, 0, 0], [0, 1, 0], [0, 0, 1]

def solid_ref(fid):
    return {"kind": {"type": "Solid"}, "anchor": {"type": "FeatureOutput", "feature_id": fid, "output_key": {"type": "Main"}},
            "selector": {"type": "Role", "role": {"type": "EndCapPositive"}, "index": 0}, "policy": {"type": "Strict"}}

# ------------------------------------------------------- published dimensions
BASE_SIDE = 124.90              # ground level, corner post to corner post
F1_Z, F1_S = 57.63, 70.69       # first platform  (4,200 m^2)
F2_Z, F2_S = 115.73, 40.96      # second platform (1,650 m^2)
MID_Z = 196.10                  # intermediate platform
F3_Z, F3_S = 276.13, 18.65      # top platform    (250 m^2)
TOP_Z = 300.65                  # finial of the 1889 structure
MAST_Z = 330.00                 # tip of the 2022 broadcast mast

# Member sizes: the corner posts are box girders several metres across at the
# base, slimming to under a metre at the top; the belts and diagonals are
# shallower lattice girders of a constant plate thickness.
POST = [(0, 3.6), (F1_Z, 2.2), (F2_Z, 1.4), (F3_Z, 0.85), (TOP_Z, 0.6)]
BRACE = [(0, 1.7), (F1_Z, 1.15), (F2_Z, 0.8), (F3_Z, 0.5), (TOP_Z, 0.4)]
BRACE_T = 0.30

def post_at(z): return lerp(z, POST)
def brace_at(z): return lerp(z, BRACE)

# The leg profile. Lower section: a log-quadratic through the three measured
# widths, which is within a few centimetres of the exponential Eiffel's
# wind-moment reasoning produces. Upper section: the exponential through the
# two measured widths, the near-straight tapering pylon you see above the
# second platform. The two meet at the second platform with a slope break,
# which the real tower has too — that is where the four legs become one pylon.
_L0 = math.log(BASE_SIDE / 2)
def _fit_lower():
    z1, z2 = F1_Z, F2_Z
    d1, d2 = math.log(F1_S / 2) - _L0, math.log(F2_S / 2) - _L0
    det = z1 * z2 * z2 - z2 * z1 * z1
    return (d1 * z2 * z2 - d2 * z1 * z1) / det, (z1 * d2 - z2 * d1) / det
_C1, _C2 = _fit_lower()
_LU = (F3_Z - F2_Z) / math.log(F2_S / F3_S)

def half(z):
    """Half the tower's published width at height z — the OUTER FACE of the
    corner posts, which is what every quoted dimension measures. half(0) = 62.45."""
    if z <= F2_Z: return math.exp(_L0 + _C1 * z + _C2 * z * z)
    return (F2_S / 2) * math.exp(-(z - F2_Z) / _LU)

def post_c(z):
    """The corner post's CENTRELINE offset: half a post inside the outer face."""
    return half(z) - post_at(z) / 2

# Each leg is a square-section lattice column whose OUTER corner rides `half(z)`
# and whose inner corner closes on the axis as it rises, stopping CORE metres
# short of it — that gap is the shaft the lifts and stairs climb, and it is why
# the first platform has a square hole in the middle.
PIER_SIDE, CORE = 25.0, 4.0
def leg_w(z): return PIER_SIDE + (half(F2_Z) - CORE - PIER_SIDE) * z / F2_Z

# Bracing levels. The legs are panelled ten times between the ground and the
# second platform (the first platform is one of the levels); the pylon fourteen
# times between the second and top platforms (the intermediate platform is one).
LEG_LEVELS = [0.0, 10.5, 22.0, 34.0, 45.5, F1_Z, 69.5, 81.0, 92.0, 104.0, F2_Z]
PYLON_LEVELS = [F2_Z, 127.0, 138.5, 150.0, 161.5, 173.0, 184.5, MID_Z,
                207.5, 219.0, 230.5, 242.0, 253.5, 265.0, F3_Z]

# The decorative arch under the first platform. It springs off the inner face
# of a pier, stays inscribed in the (narrowing) gap between two piers, and
# flattens out under the deck — see `arches()` for the curve.
ARCH_SPRING_Z, ARCH_CROWN_Z = 8.0, 48.5
ARCH_BAND, ARCH_INSET, ARCH_N = 6.2, 1.4, 11

DECK1_T, DECK2_T, DECKM_T, DECK3_T = 1.4, 1.1, 0.8, 0.8
DECK_LIP = 0.9                  # every deck oversails the structure by this much
RAIL_H, RAIL_T = 1.10, 0.14
FASCIA_H = 2.6                  # the openwork band under the first platform
                                # that carries the 72 engraved names


class Part:
    """One tab's worth of ordered tool calls."""

    def __init__(s, name):
        s.name, s.calls, s.seeds = name, [], []

    # ------------------------------------------------------------- primitives
    def sketch(s, origin, normal, entities, label):
        s.calls.append({"tool": "sketch_create", "label": label,
                        "args": {"plane": {"origin": origin, "normal": unit(normal)}, "entities": entities}})

    def extrude(s, ids, depth, label):
        params = dict(sketch_id="$SK", profile_index=0, depth=depth, **NEWBODY)
        if ids is not None: params["profile_entity_ids"] = ids
        s.calls.append({"tool": "feature_add", "label": label, "args": {"operation": {"type": "Extrude", "params": params}}})
        s.seeds.append(label)

    def poly(s, origin, normal, pts3, depth, label):
        """Extrude the closed polygon `pts3` (which must lie in the sketch plane)."""
        ents, n = [], len(pts3)
        for i, p in enumerate(pts3):
            u, v = uv(origin, normal, p)
            ents.append({"type": "Point", "id": i + 1, "x": u, "y": v})
        for i in range(n):
            ents.append({"type": "Line", "id": n + i + 1, "start_id": i + 1, "end_id": (i + 1) % n + 1})
        s.sketch(origin, normal, ents, label + " sketch")
        s.extrude(list(range(n + 1, 2 * n + 1)), depth, label)

    def cyl(s, p0, p1, r, label):
        d = sub(p1, p0)
        s.sketch(p0, d, [{"type": "Point", "id": 1, "x": 0, "y": 0},
                         {"type": "Circle", "id": 2, "center_id": 1, "radius": r}], label + " sketch")
        s.extrude([2], norm(d), label)

    def bar(s, p0, p1, w, t, label, wdir=None):
        """A rectangular member from p0 to p1: `w` across `wdir`, `t` across d x wdir."""
        d = sub(p1, p0); length = norm(d)
        if length < 1e-6: raise ValueError(f"zero-length member {label}")
        dn = mul(d, 1 / length)
        if wdir is None: wdir = X if abs(dn[2]) > 0.9 else Z
        e1 = sub(wdir, mul(dn, dot(wdir, dn)))
        if norm(e1) < 1e-9: raise ValueError(f"member {label}: wdir is parallel to it")
        e1 = unit(e1); e2 = unit(cross(dn, e1))
        pts = [add(add(p0, mul(e1, sw * w / 2)), mul(e2, st * t / 2))
               for sw, st in ((1, 1), (-1, 1), (-1, -1), (1, -1))]
        s.poly(p0, dn, pts, length, label)

    def box(s, centre_xy, z0, z1, side_x, side_y, label):
        """An axis-aligned block, z0 up to z1."""
        cx, cy = centre_xy
        pts = [[cx + sx * side_x / 2, cy + sy * side_y / 2, z0]
               for sx, sy in ((1, 1), (-1, 1), (-1, -1), (1, -1))]
        s.poly([cx, cy, z0], Z, pts, z1 - z0, label)

    # ------------------------------------------------------------- composites
    def lathe(s, pts3, label):
        """A solid of revolution about the tower axis from a lathe polygon in the
        XZ half-plane. kernel-v2's on-axis arm takes a 3-gon (an apex cone) or a
        4-gon (a solid frustum) with exactly one edge lying on the axis, so
        curved profiles arrive as a stack of frusta rather than one polyline."""
        ents, n = [], len(pts3)
        for i, p in enumerate(pts3):
            u, v = uv([0, 0, 0], Y, p)
            ents.append({"type": "Point", "id": i + 1, "x": u, "y": v})
        for i in range(n):
            ents.append({"type": "Line", "id": n + i + 1, "start_id": i + 1, "end_id": (i + 1) % n + 1})
        s.sketch([0, 0, 0], Y, ents, label + " sketch")
        s.calls.append({"tool": "feature_add", "label": label, "args": {"operation": {"type": "Revolve", "params": dict(
            sketch_id="$SK", profile_index=0, profile_entity_ids=list(range(n + 1, 2 * n + 1)),
            axis_origin=[0, 0, 0], axis_direction=Z, angle=360,
            combine={"type": "NewBody"}, targets=[], cut=False, merge=False)}}})
        s.seeds.append(label)

    def frustum(s, r0, z0, r1, z1, label):
        s.lathe([[0, 0, z0], [r0, 0, z0], [r1, 0, z1], [0, 0, z1]], label)

    def cone(s, r0, z0, z1, label):
        s.lathe([[0, 0, z0], [r0, 0, z0], [0, 0, z1]], label)

    def quarter_turns(s, label, seeds=None):
        """Raise this tab's members to four by rotating them about the tower axis."""
        seeds = s.seeds if seeds is None else seeds
        s.calls.append({"tool": "feature_add", "label": label, "args": {"operation": {"type": "PatternCircular", "params": {
            "seeds": [solid_ref("$F:" + l) for l in seeds],
            "axis": {"method": "explicit", "origin": [0, 0, 0], "direction": Z}, "count": 4, "angle_deg": 360}}}})
        s.seeds = []

    def ground(s):
        """Every part carries the same connector, so the assembly fastens them where authored."""
        s.calls.append({"tool": "feature_add", "label": "connector Ground", "args": {"operation": {"type": "MateConnector", "params": {
            "name": "Ground", "frame": {"origin": [0, 0, 0], "z_axis": Z, "x_axis": X}}}}})


# ----------------------------------------------------------------- the quarter
def leg_nodes(z):
    """The four corner posts of the (+,+) leg at height z, counter-clockwise."""
    o = post_c(z); i = o - leg_w(z)
    return [[o, o, z], [i, o, z], [i, i, z], [o, i, z]]


def mix(p0, p1, t): return [p0[i] + (p1[i] - p0[i]) * t for i in range(3)]


def brace_panel(p, a0, b0, a1, b1, label):
    """Cross-brace one lattice panel (bottom edge a0-b0, top edge a1-b1).

    Eiffel's panels are close to square, so a wide panel is divided by light
    mullions into roughly square bays and every bay gets its own X. That
    division is what gives the tower its lace; a single X across a 25 m face
    would read as a pylon, not as the Eiffel Tower."""
    z = (a0[2] + a1[2]) / 2
    width = (norm(sub(b0, a0)) + norm(sub(b1, a1))) / 2
    height = max(abs(a1[2] - a0[2]), 1e-6)
    n = max(1, min(3, round(width / height)))
    b = brace_at(z)
    bot = [mix(a0, b0, i / n) for i in range(n + 1)]
    top = [mix(a1, b1, i / n) for i in range(n + 1)]
    for i in range(1, n):
        p.bar(bot[i], top[i], b * 0.75, BRACE_T, f"{label} mullion {i}")
    for i in range(n):
        p.bar(bot[i], top[i + 1], b, BRACE_T, f"{label} bay {i} a")
        p.bar(bot[i + 1], top[i], b, BRACE_T, f"{label} bay {i} b")

def deck_ring(p, z_top, side, inner, thick, label):
    """One side of a square ring deck; the quarter turn closes it into a frame."""
    z0 = z_top - thick
    side = side + 2 * DECK_LIP
    pts = [[-side / 2, inner / 2, z0], [side / 2, inner / 2, z0], [side / 2, side / 2, z0], [-side / 2, side / 2, z0]]
    p.poly([0, 0, z0], Z, pts, thick, label)

def edge_panel(p, z0, height, side, thick, label):
    """A thin panel standing on the outer edge of one side of a square ring."""
    y = side / 2
    pts = [[-side / 2, y - thick / 2, z0], [side / 2, y - thick / 2, z0],
           [side / 2, y + thick / 2, z0], [-side / 2, y + thick / 2, z0]]
    p.poly([0, 0, z0], Z, pts, height, label)


def piers():
    """The masonry plinths the four legs stand on: a stepped block under each."""
    p = Part("Piers")
    c = post_c(0.0) - PIER_SIDE / 2
    p.box([c, c], -2.5, 1.5, PIER_SIDE + 4.0, PIER_SIDE + 4.0, "Plinth footing")
    p.box([c, c], 1.5, 6.0, PIER_SIDE + 0.6, PIER_SIDE + 0.6, "Plinth")
    p.quarter_turns("Four piers")
    p.ground()
    return p


def legs():
    """One iron leg, ground to the second platform: four corner posts, a belt at
    every bracing level and an X across every panel of all four faces."""
    p = Part("Legs")
    rows = [leg_nodes(z) for z in LEG_LEVELS]

    for c in range(4):
        for k in range(len(LEG_LEVELS) - 1):
            z = (LEG_LEVELS[k] + LEG_LEVELS[k + 1]) / 2
            w = post_at(z)
            p.bar(rows[k][c], rows[k + 1][c], w, w, f"Post {c} panel {k}")
    p.quarter_turns("Four legs: posts")

    for k, z in enumerate(LEG_LEVELS):
        for c in range(4):
            p.bar(rows[k][c], rows[k][(c + 1) % 4], brace_at(z), BRACE_T, f"Belt {k} side {c}", wdir=Z)
    p.quarter_turns("Four legs: belts")

    for c in range(4):
        d = (c + 1) % 4
        for k in range(len(LEG_LEVELS) - 1):
            brace_panel(p, rows[k][c], rows[k][d], rows[k + 1][c], rows[k + 1][d], f"Face {c}-{d} panel {k}")
    p.quarter_turns("Four legs: bracing")

    p.ground()
    return p


def arches():
    """The decorative arch under the first platform, on the +Y face.

    The arch is inscribed in the gap between two piers, and that gap NARROWS as
    it rises, because the legs lean in. So the intrados is not a free ellipse:
    its half-span at every height is the leg's own inner face, `gap(z)`, scaled
    by a quarter-circle easing that closes it at the crown. The arch therefore
    springs tangent to the pier — hugging it, as the real one does — instead of
    starting at the widest point and being swallowed by the legs above it.

    In y it rides the tower's batter, `half(z) - ARCH_INSET`, so the arch leans
    back exactly as the face it belongs to does."""
    p = Part("Arches")
    zs, zc = ARCH_SPRING_Z, ARCH_CROWN_Z

    def gap(z): return post_c(z) - leg_w(z)          # the leg's inner post centreline
    def on_face(x, z): return [x, half(z) - ARCH_INSET, z]

    def intrados(theta):
        """theta = 0 at the springing, pi/2 at the crown. Sampling by angle and
        not by height is what keeps the crown flat: the curve turns fastest
        there, and uniform steps in z would cut the corner into a gothic point."""
        z = zs + (zc - zs) * math.sin(theta)
        return gap(z) * math.cos(theta), z

    limb = [intrados((math.pi / 2) * i / (ARCH_N - 1)) for i in range(ARCH_N)]  # springing -> crown
    pts = [(-x, z) for x, z in limb] + list(reversed(limb))[1:]  # left springing -> crown -> right
    n = len(pts)

    intra = [on_face(x, z) for x, z in pts]
    extra = []
    for i, (x, z) in enumerate(pts):
        # Offset along the outward normal of the intrados, away from the opening.
        a, b = pts[max(i - 1, 0)], pts[min(i + 1, n - 1)]
        tx, tz = b[0] - a[0], b[1] - a[1]
        tl = math.hypot(tx, tz) or 1.0
        nx, nz = tz / tl, -tx / tl
        if nz < 0: nx, nz = -nx, -nz                 # the band sits above the opening
        extra.append(on_face(x + ARCH_BAND * nx, z + ARCH_BAND * nz))

    band_w, band_t = 1.35, 0.60
    for i in range(n - 1):
        p.bar(intra[i], intra[i + 1], band_w, band_t, f"Intrados {i}", wdir=Y)
        p.bar(extra[i], extra[i + 1], band_w, band_t, f"Extrados {i}", wdir=Y)
    for i in range(n):
        p.bar(intra[i], extra[i], 0.80, 0.42, f"Strut {i}", wdir=Y)
    for i in range(n - 1):
        p.bar(intra[i], extra[i + 1], 0.60, 0.36, f"Band diagonal {i}", wdir=Y)

    # The spandrel: the openwork between the arch's back and the underside of
    # the first platform, hung and cross-braced wherever the band has fallen
    # clear of the deck.
    deck_under = F1_Z - DECK1_T - FASCIA_H
    def at_deck(x): return [x, half(deck_under) - ARCH_INSET, deck_under]
    feet = [e for e in extra if e[2] < deck_under - 2.5 and abs(e[0]) < gap(e[2]) - 1.5]
    for i, e in enumerate(feet):
        p.bar(e, at_deck(e[0]), 0.55, 0.34, f"Hanger {i}", wdir=Y)
    for i in range(len(feet) - 1):
        a, b = feet[i], feet[i + 1]
        if abs(a[0] - b[0]) < 0.3: continue
        p.bar(a, at_deck(b[0]), 0.45, 0.30, f"Spandrel brace {i}", wdir=Y)
    p.quarter_turns("Four arches")
    p.ground()
    return p


def platform(name, z_top, side, inner, thick, rail=True, fascia=0.0, ground=True):
    """A square ring deck with an optional railing and openwork fascia, built as
    one side and turned four times. `side` is the platform's published width;
    the deck oversails the structure by DECK_LIP and the railing stands inboard
    of that lip, as a walkable edge does."""
    p = Part(name)
    deck_ring(p, z_top, side, inner, thick, "Deck")
    if fascia:
        edge_panel(p, z_top - thick - fascia, fascia, side, 0.5, "Fascia band")
    if rail:
        edge_panel(p, z_top, RAIL_H, side, RAIL_T, "Railing")
    p.quarter_turns("Four sides")
    if ground: p.ground()
    return p


def first_platform():
    return platform("First platform", F1_Z, F1_S,
                    2 * (half(F1_Z) - leg_w(F1_Z)), DECK1_T, fascia=FASCIA_H)

def second_platform():
    return platform("Second platform", F2_Z, F2_S, 10.0, DECK2_T, fascia=1.6)

def intermediate_platform():
    return platform("Intermediate platform", MID_Z, 2 * half(MID_Z), 9.0, DECKM_T)

def top_platform():
    p = platform("Top platform", F3_Z, F3_S, 9.0, DECK3_T, ground=False)
    # Gustave Eiffel's office and the enclosed top level sit on the gallery.
    p.box([0, 0], F3_Z, 281.4, 15.0, 15.0, "Enclosed level")
    p.ground()
    return p


def upper_pylon():
    """The single tapering pylon above the second platform: one corner post and
    one braced face, turned four times."""
    p = Part("Upper pylon")
    lv = PYLON_LEVELS
    corner = [[post_c(z), post_c(z), z] for z in lv]       # the (+,+) post
    left = [[-post_c(z), post_c(z), z] for z in lv]        # the (-,+) post: the same face

    for k in range(len(lv) - 1):
        z = (lv[k] + lv[k + 1]) / 2
        w = post_at(z)
        p.bar(corner[k], corner[k + 1], w, w, f"Post panel {k}")
    for k, z in enumerate(lv):
        p.bar(corner[k], left[k], brace_at(z), BRACE_T, f"Belt {k}", wdir=Z)
    for k in range(len(lv) - 1):
        brace_panel(p, corner[k], left[k], corner[k + 1], left[k + 1], f"Panel {k}")
    p.quarter_turns("Four faces")
    p.ground()
    return p


def campanile():
    """Above the top platform: the lattice cage, the round lantern, its dome and
    the spire that ends the 1889 structure at 300.65 m."""
    p = Part("Campanile")
    cage = [(281.4, 8.0), (284.2, 6.6), (287.0, 5.5)]
    corner = [[h, h, z] for z, h in cage]
    left = [[-h, h, z] for z, h in cage]
    for k in range(len(cage) - 1):
        w = post_at(cage[k][0])
        p.bar(corner[k], corner[k + 1], w, w, f"Cage post {k}")
    for k in range(len(cage)):
        p.bar(corner[k], left[k], brace_at(cage[k][0]), BRACE_T, f"Cage belt {k}", wdir=Z)
    for k in range(len(cage) - 1):
        b = brace_at(cage[k][0])
        p.bar(corner[k], left[k + 1], b, BRACE_T, f"Cage diagonal {k} a")
        p.bar(left[k], corner[k + 1], b, BRACE_T, f"Cage diagonal {k} b")
    p.quarter_turns("Four cage faces")

    p.cyl([0, 0, 287.0], [0, 0, 292.5], 3.6, "Lantern")
    # The quarter-ellipse roof, as six stacked frusta, and the spire above it.
    DOME = [(292.5, 4.3, 3.6, 1.5)]
    z_b, rise, r_b, r_t = DOME[0]
    ring = lambda i: (r_t + (r_b - r_t) * math.cos((math.pi / 2) * i / 6),
                      z_b + rise * math.sin((math.pi / 2) * i / 6))
    for i in range(6):
        (ra, za), (rb, zb) = ring(i), ring(i + 1)
        p.frustum(ra, za, rb, zb, f"Dome band {i}")
    p.cone(1.5, z_b + rise, TOP_Z, "Spire")
    p.ground()
    return p


def antenna_mast():
    """The broadcast mast that has carried the tower's height since 1957, at the
    330 m of its 2022 re-fit."""
    p = Part("Antenna mast")
    p.cyl([0, 0, TOP_Z], [0, 0, 318.0], 0.62, "Mast lower")
    p.cyl([0, 0, 318.0], [0, 0, 327.0], 0.34, "Mast upper")
    p.cyl([0, 0, 327.0], [0, 0, MAST_Z], 0.13, "Mast tip")
    for i, z in enumerate((306.0, 312.0)):
        p.bar([0, 0, z], [2.0, 0, z], 0.13, 0.13, f"Crossarm {i}", wdir=Z)
    p.quarter_turns("Four crossarms", seeds=[l for l in p.seeds if l.startswith("Crossarm")])
    p.ground()
    return p


PARTS = [piers, legs, arches, first_platform, second_platform, upper_pylon,
         intermediate_platform, top_platform, campanile, antenna_mast]

INSTANCES = [(n, n) for n in ("Piers", "Legs", "Arches", "First platform", "Second platform",
                              "Upper pylon", "Intermediate platform", "Top platform",
                              "Campanile", "Antenna mast")]
MATES = [(f"{n} on ground", "Fastened", ("Piers", "Ground"), (n, "Ground"))
         for n, _ in INSTANCES if n != "Piers"]


def assembly():
    part_of = dict(INSTANCES)
    calls = [{"tool": "tab_add", "label": "assembly tab", "args": {"kind": "Assembly", "name": "Eiffel Tower"}}]
    for name, part in INSTANCES:
        calls.append({"tool": "instance_add", "label": "instance " + name,
                      "args": {"tab_id": "$TAB:" + part, "name": name, "fixed": name == "Piers"}})
    seen = set()
    for _, _, *ends in MATES:
        for inst, conn in ends:
            if (inst, conn) in seen: continue
            seen.add((inst, conn))
            calls.append({"tool": "connector_add", "label": f"connector {inst}/{conn}", "args": {
                "instance_path": ["$INST:" + inst], "part_connector": f"$MC:{part_of[inst]}/{conn}", "name": conn}})
    for name, kind, (ia, ca), (ib, cb) in MATES:
        calls.append({"tool": "mate_add", "label": "mate " + name, "args": {
            "a": f"$CONN:{ia}/{ca}", "b": f"$CONN:{ib}/{cb}", "kind": kind, "flip": False, "name": name}})
    return calls


def recipe():
    """{tab name: [calls]} for every part, then the assembly under "Eiffel Tower"."""
    out = {}
    for fn in PARTS:
        part = fn(); out[part.name] = part.calls
    out["Eiffel Tower"] = assembly()
    return out


# ------------------------------------------------------------- headless build
class HostClient:
    """`waffle-host` over its stdio frames: u32 BE header_len | u32 BE payload_len | header | payload."""
    def __init__(s, binary, documents):
        import subprocess, struct
        s.struct = struct
        s.proc = subprocess.Popen([binary, "--documents", documents], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        s.n = 0
        ready = s.recv()
        assert ready["type"] == "ready", ready
    def send(s, header):
        body = json.dumps(header, separators=(",", ":")).encode()
        s.proc.stdin.write(s.struct.pack(">II", len(body), 0) + body); s.proc.stdin.flush()
    def recv(s):
        prefix = s.proc.stdout.read(8)
        if len(prefix) < 8: raise SystemExit("host closed its stdout")
        hl, pl = s.struct.unpack(">II", prefix)
        header = json.loads(s.proc.stdout.read(hl)); s.proc.stdout.read(pl)
        return header
    def call(s, tool, args):
        s.n += 1; cid = f"c{s.n}"
        s.send({"type": "tool", "id": cid, "name": tool, "arguments": args, "context": {"agent_name": "eiffel-tower.py"}})
        while True:
            frame = s.recv()
            if frame["type"] == "result" and frame.get("id") == cid:
                if frame.get("isError"):
                    raise SystemExit(f"{tool} {json.dumps(args)[:300]}\n -> {json.dumps(frame.get('structuredContent'))[:1500]}")
                return frame.get("structuredContent") or {}
    def close(s):
        s.send({"type": "bye", "reason": "done"}); s.proc.wait(timeout=60)


def build(out_path, host_bin, only=None):
    import os, shutil, tempfile, time
    docs = tempfile.mkdtemp(prefix="eiffel-tower-")
    client = HostClient(host_bin, docs)
    ids = {}
    def resolve(v, tab):
        if isinstance(v, str) and v.startswith("$"):
            if v == "$SK": return ids[("SK", tab)]
            kind, _, rest = v[1:].partition(":")
            key = (kind, tab, rest) if kind == "F" else (kind, rest)
            if key not in ids: raise SystemExit(f"unresolved placeholder {v} in tab {tab}")
            return ids[key]
        if isinstance(v, list): return [resolve(x, tab) for x in v]
        if isinstance(v, dict): return {k: resolve(x, tab) for k, x in v.items()}
        return v
    t0 = time.time()
    doc = client.call("document_new", {"name": "Eiffel Tower"})
    first_tab = doc["tabs"][0]["id"]
    plan = recipe()
    if only: plan = {k: v for k, v in plan.items() if k in only}
    first = True
    total = sum(len(c) for c in plan.values()); done = 0
    for tab, calls in plan.items():
        if tab == "Eiffel Tower":
            pass  # the assembly's tab_add is its first call
        elif first:
            client.call("tab_rename", {"tab_id": first_tab, "name": tab}); ids[("TAB", tab)] = first_tab
        else:
            ids[("TAB", tab)] = client.call("tab_add", {"kind": "Part", "name": tab})["tab_id"]
        first = False
        for call in calls:
            tool, label = call["tool"], call["label"]
            args = resolve(call["args"], tab)
            t1 = time.time(); r = client.call(tool, args); done += 1
            dt = time.time() - t1
            if dt > 0.4 or tool != "sketch_create":
                print(f"[{done}/{total}] {tab}: {tool} {label} ({dt:.1f}s)", flush=True)
            if tool == "sketch_create": ids[("SK", tab)] = r["feature_id"]; ids[("F", tab, label)] = r["feature_id"]
            elif tool in ("feature_add", "script_feature_add"):
                ids[("F", tab, label)] = r["feature_id"]
                if label.startswith("connector "): ids[("MC", f"{tab}/{label[len('connector '):]}")] = r["feature_id"]
            elif tool == "tab_add": ids[("TAB", args["name"])] = r["tab_id"]
            elif tool == "instance_add": ids[("INST", args["name"])] = r["instance_id"]
            elif tool == "connector_add": ids[("CONN", label[len("connector "):])] = r["connector_id"]
    if not only:
        asm = client.call("assembly_get", {})
        print("assembly errors:", asm.get("errors"), "warnings:", asm.get("warnings"))
    summary = client.call("model_summary", {})
    print("bodies:", summary.get("body_count"), "errors:", summary.get("errors"), "warnings:", summary.get("warnings"))
    saved = client.call("document_save", {})
    client.close()
    src = os.path.join(docs, saved["id"] + ".waffle")
    shutil.copyfile(src, out_path)
    shutil.rmtree(docs, ignore_errors=True)
    print(f"wrote {out_path} ({os.path.getsize(out_path)} bytes) in {time.time() - t0:.0f}s")


if __name__ == "__main__":
    if "--build" in sys.argv:
        out = sys.argv[sys.argv.index("--build") + 1]
        host_bin = sys.argv[sys.argv.index("--host") + 1] if "--host" in sys.argv else "target/release/waffle-host"
        only = sys.argv[sys.argv.index("--tabs") + 1].split(",") if "--tabs" in sys.argv else None
        build(out, host_bin, only)
    else:
        out = recipe()
        total = 0
        for k, v in out.items():
            print(f"{k:24s} {len(v):4d} calls"); total += len(v)
        print(f"{'total':24s} {total:4d} calls")
        print(f"half(0)={half(0):.2f} half({F1_Z})={half(F1_Z):.2f} half({F2_Z})={half(F2_Z):.2f} "
              f"half({MID_Z})={half(MID_Z):.2f} half({F3_Z})={half(F3_Z):.2f}")
        if len(sys.argv) > 1:
            json.dump(out, open(sys.argv[1], "w"), indent=None)
