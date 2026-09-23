"""Gravel bike v2 — the official Waffle Iron example, and its generator.

The bike is eleven Part tabs (frame, fork, 700c wheel, 11-42 cassette, crankset,
chain, cockpit, seatpost & saddle, rear derailleur, brake caliper, water bottle)
and one Assembly tab that places fourteen instances with thirteen mates. It
exercises Sprocket sketch entities, the built-in `sprocket` script library,
PatternCircular / PatternLinear, Pipe sweeps (chain, drop bars, saddle rails)
and Revolve — with no booleans anywhere (tubes are overlapping NewBody solids).

Two ways to use it:

    python3 gravel-bike-v2.py recipe.json
        Emit, per tab, the ordered agent-tool calls (the MCP tools of
        docs/AGENT_LINK.md) that build the bike. "$SK" = the sketch feature id
        returned by the sketch_create just before; "$F:<label>" = the feature id
        of an earlier call in the same tab with that label; "$SPROCKET" = the
        source id from script_source_add; "$TAB:<name>", "$INST:<name>",
        "$MC:<part>/<connector>" and "$CONN:<instance>/<connector>" are the
        assembly's tab, instance, MateConnector-feature and connector ids.

    python3 gravel-bike-v2.py --build out.waffle [--host target/release/waffle-host]
        Build the document headless: drive the native host over its stdio
        frames (specs/waffle_server_mode.md §3.4) with the same calls and copy
        the .waffle it autosaves to `out.waffle`. This is how
        app/static/examples/gravel-bike-v2.waffle was produced.

World frame: meters, Z up, +X forward, BB centre at the origin, axles along Y;
+Y is the LEFT (non-drive) side, -Y the drive side.
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

NEWBODY = {"combine": {"type": "NewBody"}, "targets": [], "symmetric": False, "cut": False, "merge": False}
mm = 0.001
deg = math.pi / 180
X, Y, Z = [1, 0, 0], [0, 1, 0], [0, 0, 1]
PITCH, ROLLER = 12.7 * mm, 7.75 * mm          # 1/2" x 3/32" derailleur chain

def solid_ref(fid):
    return {"kind": {"type": "Solid"}, "anchor": {"type": "FeatureOutput", "feature_id": fid, "output_key": {"type": "Main"}},
            "selector": {"type": "Role", "role": {"type": "EndCapPositive"}, "index": 0}, "policy": {"type": "Strict"}}

class Part:
    def __init__(s, name):
        s.name, s.calls = name, []
    def sketch(s, origin, normal, entities, label):
        s.calls.append({"tool": "sketch_create", "label": label,
                        "args": {"plane": {"origin": origin, "normal": unit(normal)}, "entities": entities}})
    def extrude(s, ids, depth, label):
        params = dict(sketch_id="$SK", profile_index=0, depth=depth, **NEWBODY)
        if ids is not None: params["profile_entity_ids"] = ids
        s.calls.append({"tool": "feature_add", "label": label, "args": {"operation": {"type": "Extrude", "params": params}}})
    def cyl(s, p0, p1, r, label):
        d = sub(p1, p0)
        s.sketch(p0, d, [{"type": "Point", "id": 1, "x": 0, "y": 0}, {"type": "Circle", "id": 2, "center_id": 1, "radius": r}], label + " sketch")
        s.extrude([2], norm(d), label)
    def poly(s, origin, normal, pts3, depth, label):
        ents, n = [], len(pts3)
        for i, p in enumerate(pts3):
            u, v = uv(origin, normal, p)
            ents.append({"type": "Point", "id": i + 1, "x": u, "y": v})
        for i in range(n):
            ents.append({"type": "Line", "id": n + i + 1, "start_id": i + 1, "end_id": (i + 1) % n + 1})
        s.sketch(origin, normal, ents, label + " sketch")
        s.extrude(list(range(n + 1, 2 * n + 1)), depth, label)
    def sprocket(s, origin, normal, teeth, depth, label, offset=0.0):
        s.sketch(origin, normal, [{"type": "Sprocket", "id": 1, "params": {"toothCount": teeth, "pitch": PITCH, "rollerDiameter": ROLLER, "rotationOffset": offset}}], label + " sketch")
        s.extrude(None, depth, label)
    def sprocket_script(s, origin, normal, teeth, depth, label, offset=0.0):
        s.calls.append({"tool": "script_feature_add", "label": label, "args": {"source_id": "$SPROCKET", "args": {
            "tooth_count": teeth, "pitch": PITCH, "roller_diameter": ROLLER, "rotation_offset": offset, "face_width": depth,
            "plane": {"origin": origin, "normal": unit(normal)}}}})
    def revolve(s, origin, normal, ents, ids, axis_o, axis_d, label):
        s.sketch(origin, normal, ents, label + " sketch")
        s.calls.append({"tool": "feature_add", "label": label, "args": {"operation": {"type": "Revolve", "params": dict(
            sketch_id="$SK", profile_index=0, profile_entity_ids=ids, axis_origin=axis_o, axis_direction=axis_d, angle=360,
            combine={"type": "NewBody"}, targets=[], cut=False, merge=False)}}})
    def pattern_circular(s, seed_labels, origin, direction, count, label):
        s.calls.append({"tool": "feature_add", "label": label, "args": {"operation": {"type": "PatternCircular", "params": {
            "seeds": [solid_ref("$F:" + l) for l in seed_labels],
            "axis": {"method": "explicit", "origin": origin, "direction": direction}, "count": count, "angle_deg": 360}}}})
    def pattern_linear(s, seed_labels, direction, count, spacing, label):
        s.calls.append({"tool": "feature_add", "label": label, "args": {"operation": {"type": "PatternLinear", "params": {
            "seeds": [solid_ref("$F:" + l) for l in seed_labels],
            "direction": {"method": "explicit", "origin": [0, 0, 0], "direction": direction}, "count": count, "spacing": spacing}}}})
    def pipe(s, sketch_label, entity_ids, radius, label, inner=None):
        params = {"sketch_id": "$F:" + sketch_label, "entity_ids": entity_ids, "radius": radius, "combine": {"type": "NewBody"}, "targets": []}
        if inner: params["inner_radius"] = inner
        s.calls.append({"tool": "feature_add", "label": label, "args": {"operation": {"type": "Pipe", "params": params}}})
    def connector(s, name, origin, z, x):
        s.calls.append({"tool": "feature_add", "label": "connector " + name, "args": {"operation": {"type": "MateConnector", "params": {
            "name": name, "frame": {"origin": origin, "z_axis": unit(z), "x_axis": unit(x)}}}}})

# ---------------------------------------------------------------- planar paths
class Path:
    """Points/lines/arcs in a sketch plane, 3D in, uv out. Arcs are CCW start->end
    in the sketch; `arc` takes a midpoint to fix the sense and splits > 120 deg."""
    def __init__(s, origin, normal):
        s.origin, s.normal, s.ents, s.nid = origin, unit(normal), [], 0
    def pt(s, p3):
        s.nid += 1; u, v = uv(s.origin, s.normal, p3)
        s.ents.append({"type": "Point", "id": s.nid, "x": u, "y": v}); return s.nid
    def line(s, a, b):
        s.nid += 1; s.ents.append({"type": "Line", "id": s.nid, "start_id": a, "end_id": b}); return [s.nid]
    def arc(s, c3, a_id, b_id, mid3, max_sweep=120 * deg):
        """Arc from point a_id to b_id about c3 passing through mid3 (all 3D)."""
        c = uv(s.origin, s.normal, c3)
        pa = s.pos(a_id); pb = s.pos(b_id); pm = uv(s.origin, s.normal, mid3)
        ang = lambda p: math.atan2(p[1] - c[1], p[0] - c[0])
        aa, ab, am = ang(pa), ang(pb), ang(pm)
        ccw_sweep = (ab - aa) % (2 * math.pi)
        ccw = ((am - aa) % (2 * math.pi)) < ccw_sweep
        sweep = ccw_sweep if ccw else (2 * math.pi - ccw_sweep)
        r = math.hypot(pa[0] - c[0], pa[1] - c[1])
        n = max(1, math.ceil(sweep / max_sweep - 1e-9))
        cid = s.nid + 1; s.nid += 1; s.ents.append({"type": "Point", "id": cid, "x": c[0], "y": c[1], "construction": True})
        ids, prev = [], a_id
        for i in range(1, n + 1):
            if i == n: nxt = b_id
            else:
                t = aa + (1 if ccw else -1) * sweep * i / n
                s.nid += 1; nxt = s.nid
                s.ents.append({"type": "Point", "id": nxt, "x": c[0] + r * math.cos(t), "y": c[1] + r * math.sin(t)})
            s.nid += 1
            st, en = (prev, nxt) if ccw else (nxt, prev)
            s.ents.append({"type": "Arc", "id": s.nid, "center_id": cid, "start_id": st, "end_id": en}); ids.append(s.nid)
            prev = nxt
        return ids
    def pos(s, pid):
        e = next(e for e in s.ents if e["id"] == pid); return [e["x"], e["y"]]

def check_chain(path, ids):
    """Assert the listed entities form one open G1 chain (mirrors extract_open_chain)."""
    ents = {e["id"]: e for e in path.ents}
    P = lambda pid: path.pos(pid)
    def tangents(e):
        if e["type"] == "Line":
            a, b = P(e["start_id"]), P(e["end_id"]); d = unit([b[0]-a[0], b[1]-a[1], 0])
            return (e["start_id"], d), (e["end_id"], d)
        c = P(e["center_id"]); out = []
        for pid in (e["start_id"], e["end_id"]):
            p = P(pid); rv = [p[0]-c[0], p[1]-c[1]]; out.append((pid, unit([-rv[1], rv[0], 0])))  # CCW tangent
        return out[0], out[1]
    inc = {}
    for i in ids:
        for pid, t in tangents(ents[i]): inc.setdefault(pid, []).append((i, t))
    ends = [p for p, l in inc.items() if len(l) == 1]
    assert len(ends) == 2, ("chain not open/simple", ends)
    for p, l in inc.items():
        if len(l) == 2:
            (_, t1), (_, t2) = l
            err = min(norm(sub(t1, t2)), norm(add(t1, t2)))
            assert err < 1e-9, ("non-tangent joint at point", p, err)
    return True

def tangent_line(c1, r1, s1, c2, r2, s2):
    """2D external/internal tangent between circles wrapped with sense s (+1 CCW, -1 CW)
    travelling c1 -> c2. Returns the tangent points on each circle."""
    D = [c2[0]-c1[0], c2[1]-c1[1]]; L = lambda v: [-v[1], v[0]]
    w = s2 * r2 - s1 * r1; DD = D[0]**2 + D[1]**2; k = math.sqrt(DD - w * w)
    LD = L(D); d = [(k * D[0] - w * LD[0]) / DD, (k * D[1] - w * LD[1]) / DD]
    Ld = L(d)
    return [c1[0] - s1 * r1 * Ld[0], c1[1] - s1 * r1 * Ld[1]], [c2[0] - s2 * r2 * Ld[0], c2[1] - s2 * r2 * Ld[1]]

# ------------------------------------------------------------- geometry
BB_DROP, CS_LEN = 0.075, 0.425
HA, SA = 71.5 * deg, 73.5 * deg
STACK, REACH, HT_LEN, ST_LEN, RAKE = 0.580, 0.385, 0.150, 0.520, 0.050
AZ = BB_DROP
AX = -math.sqrt(CS_LEN**2 - BB_DROP**2)
h = [-math.cos(HA), 0, math.sin(HA)]          # head tube axis, bottom -> top
f = [h[2], 0, -h[0]]                           # forward, perpendicular to the steering axis
s = [-math.cos(SA), 0, math.sin(SA)]           # seat tube axis
fs = [s[2], 0, -s[0]]
HT_T = [REACH, 0, STACK]
HT_B = sub(HT_T, mul(h, HT_LEN))
CROWN = sub(HT_B, mul(h, 0.010))               # crown race seat
FORK_L = (CROWN[2] + RAKE * f[2] - AZ) / h[2]  # axle-to-crown for a level axle
F_AXLE = add(sub(CROWN, mul(h, FORK_L)), mul(f, RAKE))
ST_T = mul(s, ST_LEN)
DT_D = unit(sub(add(HT_B, mul(h, 0.028)), [0, 0, 0]))
WHEEL_R = 0.311 + 0.0225 + 0.0225             # 700 x 45c
CHAINLINE = -0.0475                            # drive-side chain plane (y)
COG_TEETH = [11, 13, 15, 17, 19, 22, 25, 28, 32, 36, 42]
COG_Y0, COG_PITCH, COG_W = -0.062, 0.0039, 0.0018
RING_T, DRIVE_COG = 40, 19
PULLEY_T = 12
GUIDE = [AX - 0.036, AZ - 0.070]; TENSION = [AX - 0.036, AZ - 0.140]   # pulley centres (x, z)
def pitch_r(z): return PITCH / (2 * math.sin(math.pi / z))

def frame():
    p = Part("Frame")
    p.cyl(HT_B, HT_T, 0.023, "Head tube")
    p.cyl([0, 0, 0], ST_T, 0.0165, "Seat tube")
    p.cyl([0, 0, 0], add(HT_B, mul(h, 0.028)), 0.0175, "Down tube")
    p.cyl(mul(s, 0.495), add(HT_B, mul(h, 0.135)), 0.0145, "Top tube")
    p.cyl([0, -0.034, 0], [0, 0.034, 0], 0.021, "BB shell")
    for side, sy in (("L", 1), ("R", -1)):
        p.cyl([-0.005, sy * 0.030, -0.004], [AX + 0.012, sy * 0.075, AZ - 0.002], 0.011, f"Chainstay {side}")
        p.cyl(add(mul(s, 0.500), [0, sy * 0.010, 0]), [AX - 0.006, sy * 0.075, AZ + 0.010], 0.008, f"Seatstay {side}")
        o = [0, sy * 0.071, 0]
        rel = [(-0.022, -0.022), (0.022, -0.022), (0.032, 0.008), (0.020, 0.040), (-0.014, 0.040), (-0.032, 0.008)]
        pts = [[AX + dx, sy * 0.071, AZ + dz] for dx, dz in rel]
        p.poly(o, [0, sy, 0], pts, 0.008, f"Dropout {side}")
    p.connector("Fork", CROWN, h, f)
    p.connector("Rear axle", [AX, 0, AZ], Y, X)
    p.connector("Bottom bracket", [0, 0, 0], Y, X)
    p.connector("Seatpost", ST_T, Z, X)
    p.connector("Rear derailleur", [AX, 0, AZ], Y, X)
    rc = [AX - 0.028, 0.040, AZ + 0.066]
    p.connector("Rear brake", rc, Y, [0.066, 0, 0.028])
    dt_up = [-DT_D[2], 0, DT_D[0]]
    p.connector("Bottle down tube", add(mul(DT_D, 0.20), mul(dt_up, 0.0175 + 0.037)), DT_D, dt_up)
    p.connector("Bottle seat tube", add(mul(s, 0.10), mul(fs, 0.0165 + 0.037)), s, fs)
    return p

def fork():
    p = Part("Fork")
    p.cyl([0, 0, -0.004], [0, 0, 0.240], 0.0143, "Steerer")
    # Crown stadium: semicircular caps at y = ±0.040 (r 0.018) joined by straight
    # sides at x = 0.008 ± 0.018. Walk the -y cap from +x round the bottom to -x,
    # then the +y cap from -x over the top back to +x, so the polygon is simple
    # (the 09-23 first cut bulged the caps opposite ways and made a bowtie).
    stad = []
    for i in range(9):
        t = -math.pi * i / 8
        stad.append([0.008 + 0.018 * math.cos(t), -0.040 + 0.018 * math.sin(t), 0])
    for i in range(9):
        t = math.pi - math.pi * i / 8
        stad.append([0.008 + 0.018 * math.cos(t), 0.040 + 0.018 * math.sin(t), 0])
    p.poly([0, 0, 0], [0, 0, -1], stad, 0.026, "Crown")
    axle = [RAKE, 0, -FORK_L]
    for side, sy in (("L", 1), ("R", -1)):
        p.cyl([0.008, sy * 0.040, -0.018], [RAKE - 0.004, sy * 0.054, -FORK_L + 0.012], 0.012, f"Blade {side}")
        rel = [(-0.020, -0.020), (0.020, -0.020), (0.028, 0.010), (0.014, 0.045), (-0.014, 0.045), (-0.028, 0.010)]
        pts = [[axle[0] + dx, sy * 0.050, axle[2] + dz] for dx, dz in rel]
        p.poly([0, sy * 0.050, 0], [0, sy, 0], pts, 0.008, f"Dropout {side}")
    p.connector("Crown race", [0, 0, 0], Z, X)
    p.connector("Front axle", axle, Y, X)
    p.connector("Stem", [0, 0, 0.205], Z, X)
    bc = [axle[0] - 0.028, 0.040, axle[2] + 0.066]
    p.connector("Front brake", bc, Y, [0.066, 0, 0.028])
    return p

def wheel():
    p = Part("Wheel 700c")
    ents = [{"type": "Point", "id": 1, "x": -0.0125, "y": 0.295}, {"type": "Point", "id": 2, "x": 0.0125, "y": 0.295},
            {"type": "Point", "id": 3, "x": 0.0125, "y": 0.3135}, {"type": "Point", "id": 4, "x": -0.0125, "y": 0.3135},
            {"type": "Line", "id": 5, "start_id": 1, "end_id": 2}, {"type": "Line", "id": 6, "start_id": 2, "end_id": 3},
            {"type": "Line", "id": 7, "start_id": 3, "end_id": 4}, {"type": "Line", "id": 8, "start_id": 4, "end_id": 1}]
    p.revolve([0, 0, 0], Z, ents, [5, 6, 7, 8], [0, 0, 0], Y, "Rim")
    p.revolve([0, 0, 0], Z, [{"type": "Point", "id": 1, "x": 0, "y": 0.3335}, {"type": "Circle", "id": 2, "center_id": 1, "radius": 0.0225}],
              [2], [0, 0, 0], Y, "Tire")
    p.cyl([0, -0.052, 0], [0, 0.052, 0], 0.017, "Hub shell")
    p.cyl([0, 0.026, 0], [0, 0.031, 0], 0.031, "Flange L")
    p.cyl([0, -0.031, 0], [0, -0.026, 0], 0.031, "Flange R")
    p.cyl([0, -0.071, 0], [0, 0.071, 0], 0.0075, "Thru axle")
    p.cyl([0, 0.040, 0], [0, 0.042, 0], 0.080, "Brake rotor 160")
    # 24 spokes: 4 seeds (side and crossing pattern repeat every 4) x 6 around Y
    seeds = []
    for i in range(4):
        a = i * 15 * deg
        sy = 0.0285 if i % 2 == 0 else -0.0285
        p0 = [0.026 * math.cos(a), sy, 0.026 * math.sin(a)]
        b = a + (22 * deg if i % 4 < 2 else -22 * deg)
        p1 = [0.296 * math.cos(b), 0, 0.296 * math.sin(b)]
        p.cyl(p0, p1, 0.0012, f"Spoke seed {i + 1}"); seeds.append(f"Spoke seed {i + 1}")
    p.pattern_circular(seeds, [0, 0, 0], Y, 6, "Spokes x6")
    p.connector("Axle", [0, 0, 0], Y, X)
    return p

def cassette():
    p = Part("Cassette 11-42")
    for i, t in enumerate(COG_TEETH):
        y = COG_Y0 + i * COG_PITCH
        p.sprocket_script([0, y, 0], Y, t, COG_W, f"Cog {t}T", offset=i * 7 * deg)
    p.cyl([0, COG_Y0 + COG_W, 0], [0, COG_Y0 + COG_PITCH, 0], 0.021, "Spacer seed")
    p.pattern_linear(["Spacer seed"], Y, len(COG_TEETH) - 1, COG_PITCH, "Spacers x10")
    p.cyl([0, -0.066, 0], [0, -0.019, 0], 0.019, "Freehub body")
    p.connector("Hub", [0, 0, 0], Y, X)
    return p

def crankset():
    p = Part("Crankset")
    p.cyl([0, -0.096, 0], [0, 0.096, 0], 0.012, "Spindle")
    ang = -40 * deg
    for side, sy, a in (("R", -1, ang), ("L", 1, ang + math.pi)):
        d = [math.cos(a), 0, math.sin(a)]
        n = [-d[2], 0, d[0]]
        tip = mul(d, 0.170)
        y0 = sy * 0.076
        rect = [add(mul(n, 0.012), [0, y0, 0]), add(add(tip, mul(n, 0.012)), [0, y0, 0]),
                add(add(tip, mul(n, -0.012)), [0, y0, 0]), add(mul(n, -0.012), [0, y0, 0])]
        p.poly([0, y0, 0], [0, sy, 0], rect, 0.014, f"Crank arm {side}")
        p.cyl([0, y0, 0], [0, y0 + sy * 0.014, 0], 0.020, f"Arm boss {side}")
        p.cyl([tip[0], y0, tip[2]], [tip[0], y0 + sy * 0.014, tip[2]], 0.013, f"Pedal boss {side}")
        p.cyl([tip[0], y0 + sy * 0.014, tip[2]], [tip[0], y0 + sy * 0.045, tip[2]], 0.0075, f"Pedal spindle {side}")
        py = y0 + sy * 0.040
        plat = [[tip[0] - 0.045, py, tip[2] - 0.009], [tip[0] + 0.045, py, tip[2] - 0.009],
                [tip[0] + 0.045, py, tip[2] + 0.009], [tip[0] - 0.045, py, tip[2] + 0.009]]
        p.poly([0, py, 0], [0, sy, 0], plat, 0.060, f"Pedal {side}")
    p.sprocket([0, CHAINLINE + 0.0015, 0], [0, -1, 0], RING_T, 0.003, f"Chainring {RING_T}T")
    p.cyl([0, -0.044, 0], [0, -0.058, 0], 0.045, "Chainring spider")
    p.cyl([0.056, -0.043, 0], [0.056, -0.060, 0], 0.005, "Chainring bolt seed")
    p.pattern_circular(["Chainring bolt seed"], [0, 0, 0], Y, 5, "Chainring bolts x5")
    p.connector("Bottom bracket", [0, 0, 0], Y, X)
    return p

def chain():
    """One closed chain loop as two open pipe sweeps in the chainline plane."""
    p = Part("Chain")
    cog_i = COG_TEETH.index(DRIVE_COG)
    circles = [  # travel order: cog -> chainring -> tension pulley -> guide pulley -> cog
        ("cog", [AX, AZ], pitch_r(DRIVE_COG), -1),
        ("ring", [0.0, 0.0], pitch_r(RING_T), -1),
        ("tension", TENSION, pitch_r(PULLEY_T), -1),
        ("guide", GUIDE, pitch_r(PULLEY_T), +1),
    ]
    n = len(circles)
    lines = []  # (entry point on circle i, exit... ) tangent from circle i to i+1
    for i in range(n):
        _, c1, r1, s1 = circles[i]; _, c2, r2, s2 = circles[(i + 1) % n]
        lines.append(tangent_line(c1, r1, s1, c2, r2, s2))
    y = CHAINLINE
    P3 = lambda q: [q[0], y, q[2] if len(q) == 3 else q[1]]
    path = Path([0, y, 0], [0, -1, 0])
    # segments: circle i has arc from arrival (line i-1 end) to departure (line i start)
    def mid_on(c, r, sense, a, b):
        ang = lambda q: math.atan2(q[1] - c[1], q[0] - c[0])
        aa, ab = ang(a), ang(b)
        sweep = (ab - aa) % (2 * math.pi) if sense > 0 else (aa - ab) % (2 * math.pi)
        t = aa + sense * sweep / 2
        return [c[0] + r * math.cos(t), y, c[1] + r * math.sin(t)]
    # split the top run (line 0: cog->ring) and bottom run (line 1: ring->tension) at their midpoints
    def midpoint(a, b): return [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2]
    top_mid, bot_mid = midpoint(*lines[0]), midpoint(*lines[1])
    # Pipe A: top_mid -> ring arrival -> ring arc -> ring departure -> bot_mid
    A_ids = []
    a0 = path.pt(P3(top_mid)); a1 = path.pt(P3(lines[0][1]))
    A_ids += path.line(a0, a1)
    a2 = path.pt(P3(lines[1][0]))
    _, cr, rr, sr = circles[1]
    A_ids += path.arc(P3(cr), a1, a2, mid_on(cr, rr, sr, lines[0][1], lines[1][0]))
    a3 = path.pt(P3(bot_mid))
    A_ids += path.line(a2, a3)
    # Pipe B: bot_mid -> tension arc -> guide arc -> cog arc -> top_mid
    B_ids = []
    prev = path.pt(P3(bot_mid))
    for i in (2, 3, 0):
        arrive = lines[(i - 1) % n][1]; depart = lines[i][0]
        q_in = path.pt(P3(arrive)); B_ids += path.line(prev, q_in)
        q_out = path.pt(P3(depart))
        _, c, r, sn = circles[i]
        B_ids += path.arc(P3(c), q_in, q_out, mid_on(c, r, sn, arrive, depart))
        prev = q_out
    b_end = path.pt(P3(top_mid)); B_ids += path.line(prev, b_end)
    check_chain(path, A_ids); check_chain(path, B_ids)
    p.sketch([0, y, 0], [0, -1, 0], path.ents, "Chain path")
    p.pipe("Chain path", A_ids, 0.0045, "Chain front run")
    p.pipe("Chain path", B_ids, 0.0045, "Chain rear run")
    p.connector("Bottom bracket", [0, 0, 0], Y, X)
    return p

def cockpit():
    p = Part("Cockpit")
    p.cyl([0, 0, -0.052], [0, 0, -0.020], 0.0185, "Headset spacers")
    p.cyl([0, 0, -0.020], [0, 0, 0.020], 0.022, "Stem clamp")
    p.cyl([0, 0, 0.020], [0, 0, 0.026], 0.017, "Top cap")
    B = [0.100 * math.cos(6 * deg), 0, -0.100 * math.sin(6 * deg)]
    p.cyl([0.010, 0, 0], B, 0.016, "Stem extension")
    p.cyl(add(B, [0, -0.021, 0]), add(B, [0, 0.021, 0]), 0.021, "Bar clamp")
    p.cyl(add(B, [0, -0.200, 0]), add(B, [0, 0.200, 0]), 0.0118, "Bar tops")
    for side, sy in (("L", 1), ("R", -1)):
        o = add(B, [0, sy * 0.200, 0]); nrm = [0, sy, 0]
        path = Path(o, nrm)
        q0 = path.pt(o); q1 = path.pt(add(o, [0.070, 0, 0]))
        ids = path.line(q0, q1)
        c = add(o, [0.070, 0, -0.065]); q2 = path.pt(add(o, [0.070, 0, -0.130]))
        ids += path.arc(c, q1, q2, add(o, [0.135, 0, -0.065]), max_sweep=95 * deg)
        q3 = path.pt(add(o, [-0.040, 0, -0.130])); ids += path.line(q2, q3)
        check_chain(path, ids)
        p.sketch(o, nrm, path.ents, f"Drop {side} path")
        p.pipe(f"Drop {side} path", ids, 0.0118, f"Drop {side}")
        hood0 = add(o, [0.060, 0, 0.004])
        p.cyl(hood0, add(hood0, [0.080, 0, -0.030]), 0.015, f"Lever hood {side}")
        p.cyl(add(o, [0.145, 0, -0.035]), add(o, [0.130, 0, -0.110]), 0.006, f"Brake lever {side}")
    p.connector("Steerer", [0, 0, 0], Z, X)
    return p

def seatpost():
    p = Part("Seatpost & saddle")
    top = mul(s, 0.200)
    p.cyl(mul(s, -0.120), top, 0.0136, "Seatpost")
    p.cyl(add(top, [0, -0.020, 0]), add(top, [0, 0.020, 0]), 0.016, "Head clamp")
    zr = top[2] + 0.024
    for side, sy in (("L", 1), ("R", -1)):
        o = [top[0], sy * 0.022, zr]; nrm = [0, sy, 0]
        path = Path(o, nrm)
        q0 = path.pt([top[0] + 0.075, sy * 0.022, zr]); q1 = path.pt([top[0] - 0.045, sy * 0.022, zr])
        ids = path.line(q0, q1)
        c = [top[0] - 0.045, sy * 0.022, zr + 0.020]
        q2 = path.pt([top[0] - 0.065, sy * 0.022, zr + 0.020])
        ids += path.arc(c, q1, q2, [top[0] - 0.045 - 0.020 * math.sin(math.pi / 4), sy * 0.022, zr + 0.020 - 0.020 * math.cos(math.pi / 4)])
        q3 = path.pt([top[0] - 0.065, sy * 0.022, zr + 0.032]); ids += path.line(q2, q3)
        check_chain(path, ids)
        p.sketch(o, nrm, path.ents, f"Rail {side} path")
        p.pipe(f"Rail {side} path", ids, 0.0035, f"Rail {side}")
    zs = zr + 0.004
    out = [(0.130, 0.0), (0.100, 0.014), (0.030, 0.026), (-0.050, 0.058), (-0.110, 0.072), (-0.145, 0.045), (-0.155, 0.0)]
    loop = [(x, y) for x, y in out] + [(x, -y) for x, y in reversed(out[1:-1])]
    pts = [[top[0] + x, y, zs] for x, y in loop]
    p.poly([0, 0, zs], Z, pts, 0.030, "Saddle")
    p.connector("Seat tube", [0, 0, 0], Z, X)
    return p

def derailleur():
    p = Part("Rear derailleur")
    gx, gz = GUIDE[0] - AX, GUIDE[1] - AZ
    yi, yo = CHAINLINE + 0.005, CHAINLINE - 0.005      # pulley faces
    p.poly([0, -0.074, 0], [0, -1, 0], [[-0.030, -0.074, 0.015], [0.010, -0.074, 0.015], [0.010, -0.074, -0.020], [-0.030, -0.074, -0.020]], 0.006, "Hanger")
    p.poly([0, -0.080, 0], [0, -1, 0], [[-0.055, -0.080, -0.010], [0.000, -0.080, -0.010], [0.000, -0.080, -0.060], [-0.055, -0.080, -0.060]], 0.024, "Body")
    p.cyl([gx, yo - 0.003, gz + 0.015], [gx, -0.080, gz + 0.015], 0.009, "Knuckle")
    cage = lambda yy: [[gx - 0.026, yy, gz + 0.030], [gx + 0.026, yy, gz + 0.030], [gx + 0.026, yy, gz - 0.100], [gx - 0.026, yy, gz - 0.100]]
    p.poly([0, yo, 0], [0, -1, 0], cage(yo), 0.003, "Cage outer")
    p.poly([0, yi + 0.003, 0], [0, -1, 0], cage(yi + 0.003), 0.003, "Cage inner")
    p.sprocket([gx, yi, gz], [0, -1, 0], PULLEY_T, 0.010, "Guide pulley")
    p.pattern_linear(["Guide pulley"], [0, 0, -1], 2, GUIDE[1] - TENSION[1], "Pulleys x2")
    p.connector("Axle", [0, 0, 0], Y, X)
    return p

def caliper():
    p = Part("Brake caliper")
    p.poly([0, 0, -0.019], Z, [[-0.037, -0.014, -0.019], [0.037, -0.014, -0.019], [0.037, 0.014, -0.019], [-0.037, 0.014, -0.019]], 0.038, "Caliper body")
    p.connector("Mount", [0, 0, 0], Z, X)
    return p

def bottle():
    p = Part("Water bottle")
    p.cyl([0, 0, 0], [0, 0, 0.180], 0.037, "Bottle")
    p.cyl([0, 0, 0.180], [0, 0, 0.215], 0.020, "Neck")
    p.cyl([0, 0, 0.215], [0, 0, 0.232], 0.016, "Cap")
    p.connector("Base", [0, 0, 0], Z, X)
    return p

PARTS = [frame, fork, wheel, cassette, crankset, chain, cockpit, seatpost, derailleur, caliper, bottle]

# ------------------------------------------------------------- assembly
# Fourteen instances, thirteen mates: every part meets the frame (or the fork)
# at a pair of MateConnector features authored to coincide, so every mate is
# `flip: false`. Fastened everywhere except the three things that turn.
INSTANCES = [  # (instance name, part tab name)
    ("Frame", "Frame"), ("Fork", "Fork"), ("Rear wheel", "Wheel 700c"), ("Front wheel", "Wheel 700c"),
    ("Cassette", "Cassette 11-42"), ("Crankset", "Crankset"), ("Chain", "Chain"), ("Cockpit", "Cockpit"),
    ("Seatpost", "Seatpost & saddle"), ("Rear derailleur", "Rear derailleur"),
    ("Rear caliper", "Brake caliper"), ("Front caliper", "Brake caliper"),
    ("Bottle down tube", "Water bottle"), ("Bottle seat tube", "Water bottle"),
]
MATES = [  # (name, kind, (instance a, connector a), (instance b, connector b))
    ("Fork in head tube", "Fastened", ("Frame", "Fork"), ("Fork", "Crown race")),
    ("Rear wheel", "Revolute", ("Frame", "Rear axle"), ("Rear wheel", "Axle")),
    ("Front wheel", "Revolute", ("Fork", "Front axle"), ("Front wheel", "Axle")),
    ("Cassette on hub", "Fastened", ("Frame", "Rear axle"), ("Cassette", "Hub")),
    ("Crankset", "Revolute", ("Frame", "Bottom bracket"), ("Crankset", "Bottom bracket")),
    ("Chain", "Fastened", ("Frame", "Bottom bracket"), ("Chain", "Bottom bracket")),
    ("Cockpit on steerer", "Fastened", ("Fork", "Stem"), ("Cockpit", "Steerer")),
    ("Seatpost", "Fastened", ("Frame", "Seatpost"), ("Seatpost", "Seat tube")),
    ("Rear derailleur", "Fastened", ("Frame", "Rear derailleur"), ("Rear derailleur", "Axle")),
    ("Rear brake", "Fastened", ("Frame", "Rear brake"), ("Rear caliper", "Mount")),
    ("Front brake", "Fastened", ("Fork", "Front brake"), ("Front caliper", "Mount")),
    ("Bottle down tube", "Fastened", ("Frame", "Bottle down tube"), ("Bottle down tube", "Base")),
    ("Bottle seat tube", "Fastened", ("Frame", "Bottle seat tube"), ("Bottle seat tube", "Base")),
]

def assembly():
    part_of = dict(INSTANCES)
    calls = [{"tool": "tab_add", "label": "assembly tab", "args": {"kind": "Assembly", "name": "Gravel bike"}}]
    for name, part in INSTANCES:
        calls.append({"tool": "instance_add", "label": "instance " + name,
                      "args": {"tab_id": "$TAB:" + part, "name": name, "fixed": name == "Frame"}})
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
    """{tab name: [calls]} for every part, then the assembly under "Gravel bike"."""
    out = {}
    for fn in PARTS:
        part = fn(); out[part.name] = part.calls
    out["Gravel bike"] = assembly()
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
        s.send({"type": "tool", "id": cid, "name": tool, "arguments": args, "context": {"agent_name": "gravel-bike-v2.py"}})
        while True:
            frame = s.recv()
            if frame["type"] == "result" and frame.get("id") == cid:
                if frame.get("isError"):
                    raise SystemExit(f"{tool} {json.dumps(args)[:300]}\n -> {json.dumps(frame.get('structuredContent'))[:1500]}")
                return frame.get("structuredContent") or {}
    def close(s):
        s.send({"type": "bye", "reason": "done"}); s.proc.wait(timeout=30)

def build(out_path, host_bin):
    import os, shutil, tempfile, time
    docs = tempfile.mkdtemp(prefix="gravel-bike-v2-")
    client = HostClient(host_bin, docs)
    ids = {}  # placeholder -> id
    def resolve(v, tab):
        if isinstance(v, str) and v.startswith("$"):
            if v == "$SK": return ids[("SK", tab)]
            if v == "$SPROCKET": return ids["SPROCKET"]
            kind, _, rest = v[1:].partition(":")
            key = (kind, tab, rest) if kind == "F" else (kind, rest)
            if key not in ids: raise SystemExit(f"unresolved placeholder {v} in tab {tab}")
            return ids[key]
        if isinstance(v, list): return [resolve(x, tab) for x in v]
        if isinstance(v, dict): return {k: resolve(x, tab) for k, x in v.items()}
        return v
    t0 = time.time()
    doc = client.call("document_new", {"name": "Gravel bike v2"})
    first_tab = doc["tabs"][0]["id"]
    ids["SPROCKET"] = client.call("script_source_add", {"library": "sprocket"})["source_id"]
    plan = recipe()
    total = sum(len(c) for c in plan.values()); done = 0
    for tab, calls in plan.items():
        if tab == "Gravel bike":
            pass  # the assembly's tab_add is its first call
        elif tab == "Frame":
            client.call("tab_rename", {"tab_id": first_tab, "name": tab}); ids[("TAB", tab)] = first_tab
        else:
            ids[("TAB", tab)] = client.call("tab_add", {"kind": "Part", "name": tab})["tab_id"]
        for call in calls:
            tool, label = call["tool"], call["label"]
            args = resolve(call["args"], tab)
            t1 = time.time(); r = client.call(tool, args); done += 1
            print(f"[{done}/{total}] {tab}: {tool} {label} ({time.time() - t1:.1f}s)", flush=True)
            if tool == "sketch_create": ids[("SK", tab)] = r["feature_id"]; ids[("F", tab, label)] = r["feature_id"]
            elif tool in ("feature_add", "script_feature_add"):
                ids[("F", tab, label)] = r["feature_id"]
                if label.startswith("connector "): ids[("MC", f"{tab}/{label[len('connector '):]}")] = r["feature_id"]
            elif tool == "tab_add": ids[("TAB", args["name"])] = r["tab_id"]
            elif tool == "instance_add": ids[("INST", args["name"])] = r["instance_id"]
            elif tool == "connector_add": ids[("CONN", label[len("connector "):])] = r["connector_id"]
    asm = client.call("assembly_get", {})
    print("assembly errors:", asm.get("errors"), "warnings:", asm.get("warnings"))
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
        build(out, host_bin)
    else:
        out = recipe()
        json.dump(out, open(sys.argv[1], "w"), indent=None)
        total = 0
        for k, v in out.items():
            print(k, len(v), "calls"); total += len(v)
        print("total", total)
        print("F_AXLE", F_AXLE, "AX", AX, "wheelbase", F_AXLE[0] - AX, "ring r", pitch_r(RING_T), "cog r", pitch_r(DRIVE_COG), "pulley r", pitch_r(PULLEY_T))
