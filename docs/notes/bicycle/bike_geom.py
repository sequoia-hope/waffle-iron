"""Fork, dropouts, restaged stays and cope tools for the bicycle frame.

World frame: meters, Z up, +X forward, BB axis on Y. Sketch basis mirrors
waffle-types/src/sketch_plane.rs. Verifies (by sampling) that after the planned
cuts no two bodies overlap, coped tube ends are fully consumed, and no cut
leaves a detached fragment.
"""
import json, math, sys

def add(a, b): return [a[i] + b[i] for i in range(3)]
def sub(a, b): return [a[i] - b[i] for i in range(3)]
def mul(a, s): return [a[i] * s for i in range(3)]
def dot(a, b): return sum(a[i] * b[i] for i in range(3))
def cross(a, b): return [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
def norm(a): return math.sqrt(dot(a, a))
def unit(a): return mul(a, 1 / norm(a))
def r6(v): return [round(x, 6) for x in v]

def basis(n):
    n = unit(n)
    ref = [0, 0, 1] if abs(n[2]) < 0.99 else [1, 0, 0]
    x = unit(cross(ref, n)); y = unit(cross(n, x))
    return x, y, n

# ---------------------------------------------------------------- shapes
class Cyl:
    """Solid (ri=0) or hollow cylinder from p0 along d, axial [a0, a1]."""
    def __init__(s, p0, d, a0, a1, ro, ri=0.0):
        s.p0, s.d, s.a0, s.a1, s.ro, s.ri = p0, unit(d), a0, a1, ro, ri
    def contains(s, p, eps=0.0):
        r = sub(p, s.p0); a = dot(r, s.d)
        if a < s.a0 - eps or a > s.a1 + eps: return False
        rad = norm(sub(r, mul(s.d, a)))
        return s.ri - eps <= rad <= s.ro + eps

def pip(poly, u, v):
    inside = False; n = len(poly)
    for i in range(n):
        (x1, y1), (x2, y2) = poly[i], poly[(i + 1) % n]
        if (y1 > v) != (y2 > v) and u < x1 + (v - y1) * (x2 - x1) / (y2 - y1):
            inside = not inside
    return inside

class Prism:
    def __init__(s, origin, normal, depth, poly):
        s.o, s.depth, s.poly = origin, depth, poly
        s.x, s.y, s.n = basis(normal)
    def contains(s, p, eps=0.0):
        r = sub(p, s.o); w = dot(r, s.n)
        if w < -eps or w > s.depth + eps: return False
        return pip(s.poly, dot(r, s.x), dot(r, s.y))

def stadium_poly(c, rad, seg=48):
    """Stadium with circle centers (±c, 0) in sketch (u, v)."""
    pts = []
    for i in range(seg + 1):
        t = -math.pi / 2 + math.pi * i / seg
        pts.append((c + rad * math.cos(t), rad * math.sin(t)))
    for i in range(seg + 1):
        t = math.pi / 2 + math.pi * i / seg
        pts.append((-c + rad * math.cos(t), rad * math.sin(t)))
    return pts

# ---------------------------------------------------------------- frame (existing)
mm = 0.001
HT_B = [0.446016, 0, 0.388637]; h = unit([-0.292372, 0, 0.956305]); HT_LEN = 0.150
f = [h[2], 0, -h[0]]                         # forward, perpendicular to steering axis
ST_D = unit([-0.284015, 0, 0.95882]); DT_D = unit([0.718286, 0, 0.695747])
TT_O = [-0.150531, 0, 0.508175]
Y = [0, 1, 0]

tubes = {}
tubes['HT'] = Cyl(HT_B, h, 0, HT_LEN, 22*mm, 20.5*mm)
tubes['BB'] = Cyl([0, -0.034, 0], Y, 0, 0.068, 20*mm, 17.5*mm)
tubes['TT'] = Cyl(TT_O, [1, 0, 0], 0, 0.56, 12.7*mm, 11.8*mm)
tubes['DT'] = Cyl([0, 0, 0], DT_D, 0, 0.606698, 15.9*mm, 15*mm)
tubes['ST'] = Cyl([0, 0, 0], ST_D, 0, 0.56, 14.3*mm, 13.4*mm)

# ---------------------------------------------------------------- rear triangle
AX, AZ = -0.40398, 0.07
TRIM = 25*mm             # stays stop this far from the axle (inside the dropout)
MID = 35*mm              # stay centerline sits mid-plate this far from the axle
PLATE_IN, PLATE_T = 0.065, 0.006            # rear: 130 mm OLD, 6 mm plates
PLATE_MID = PLATE_IN + PLATE_T / 2

def solve_axle_y(far, plate_mid):
    """Axle-end y so the stay centerline (axle → far) is at plate_mid, MID from the axle."""
    ya = plate_mid
    for _ in range(50):
        A = [AX_, ya, AZ_]
        d = unit(sub(far, A))
        ya += plate_mid - (ya + d[1] * MID)
    return ya

ST_J = add([0, 0, 0], mul(ST_D, 0.5))        # seatstay junction on the seat tube axis
SS_YE = float(sys.argv[1]) * mm if len(sys.argv) > 1 else 5.5 * mm

stay = {}
for side, sg in (('L', 1), ('R', -1)):
    AX_, AZ_ = AX, AZ
    cs_far = [0, sg * 0.020, 0]
    ya = solve_axle_y([0, abs(cs_far[1]), 0], PLATE_MID) * sg
    A = [AX, ya, AZ]; d = unit(sub(cs_far, A)); L = norm(sub(cs_far, A))
    # chainstay sketched at the BB end, extruded toward the axle
    stay['CS' + side] = dict(origin=cs_far, normal=mul(d, -1), depth=L - TRIM, ro=11.1*mm, ri=10.3*mm, axle=A)
    tubes['CS' + side] = Cyl(cs_far, mul(d, -1), 0, L - TRIM, 11.1*mm, 10.3*mm)

    ss_far = [ST_J[0], sg * SS_YE, ST_J[2]]
    ya = solve_axle_y([ss_far[0], SS_YE, ss_far[2]], PLATE_MID) * sg
    A = [AX, ya, AZ]; d = unit(sub(ss_far, A)); L = norm(sub(ss_far, A))
    start = add(A, mul(d, TRIM))
    stay['SS' + side] = dict(origin=start, normal=d, depth=L - TRIM, ro=8*mm, ri=7.2*mm, axle=A)
    tubes['SS' + side] = Cyl(start, d, 0, L - TRIM, 8*mm, 7.2*mm)

# rear dropout outline, axle-relative world (x, z) in mm
def u2(v): m = math.hypot(*v); return (v[0]/m, v[1]/m)
cs2 = u2((-AX, -AZ)); ss2 = u2((ST_J[0] - AX, ST_J[2] - AZ))
pc = (-cs2[1], cs2[0]); ps = (ss2[1], -ss2[0])   # pc: upper side of CS; ps: forward side of SS
W_CS, W_SS, REACH = 14, 12, 45
def lin(a, v, b, w): return (a * v[0] + b * w[0], a * v[1] + b * w[1])
P1 = lin(REACH, cs2, -W_CS, pc); P2 = lin(REACH, cs2, W_CS, pc)
# crotch: t*cs2 + W_CS*pc == r*ss2 + W_SS*ps
rhs = (W_SS*ps[0] - W_CS*pc[0], W_SS*ps[1] - W_CS*pc[1])
det = cs2[0] * (-ss2[1]) - (-ss2[0]) * cs2[1]
t = (rhs[0] * (-ss2[1]) - (-ss2[0]) * rhs[1]) / det
P3 = lin(t, cs2, W_CS, pc)
P4 = lin(REACH, ss2, W_SS, ps); P5 = lin(REACH, ss2, -W_SS, ps)
rear_xz = [(5, -14), P1, P2, P3, P4, P5, (-12, 8), (-14, -14), (-5, -14), (-5, 5), (5, 5)]

# ---------------------------------------------------------------- fork
C = sub(HT_B, mul(h, 0.012))                 # crown race seat (lower headset stack 12 mm)
CROWN_T = 0.020; CROWN_C = 0.040; CROWN_R = 0.016
RAKE = 0.045
# axle-to-crown chosen so the front axle is level with the rear axle
A2C = (C[2] + RAKE * f[2] - AZ) / h[2]
AF0 = add(sub(C, mul(h, A2C)), mul(f, RAKE))
F_PLATE_IN = 0.050                          # 100 mm OLD
F_PLATE_MID = F_PLATE_IN + PLATE_T / 2
tubes['STEER'] = Cyl(C, h, 0, 0.220, 14.3*mm, 12.7*mm)
fork = {}
for side, sg in (('L', 1), ('R', -1)):
    top = add(sub(C, mul(h, CROWN_T / 2)), [0, sg * CROWN_C, 0])
    ya = F_PLATE_MID
    for _ in range(50):
        A = [AF0[0], ya, AF0[2]]; d = unit(sub([top[0], CROWN_C, top[2]], A))
        ya += F_PLATE_MID - (ya + d[1] * MID)
    A = [AF0[0], sg * ya, AF0[2]]; d = unit(sub(top, A)); L = norm(sub(top, A))
    start = add(A, mul(d, TRIM))
    fork['BL' + side] = dict(origin=start, normal=d, depth=L - TRIM, ro=11*mm, ri=10*mm, axle=A)
    tubes['BL' + side] = Cyl(start, d, 0, L - TRIM, 11*mm, 10*mm)

bd = u2((top[0] - AF0[0], top[2] - AF0[2])); bp = (-bd[1], bd[0])
front_ap = [(-14, -14), (45, -14), (45, 14), (-14, 14), (-14, 5), (5, 5), (5, -5), (-14, -5)]
front_xz = [lin(a, bd, p, bp) for a, p in front_ap]

# ---------------------------------------------------------------- plates as prisms (+Y normal)
def plate_poly_uv(xz):
    # +Y normal basis: u = -dx, v = +dz  (x_axis = -X, y_axis = +Z)
    pts = [(-x * mm, z * mm) for x, z in xz]
    area = sum(pts[i][0] * pts[(i+1) % len(pts)][1] - pts[(i+1) % len(pts)][0] * pts[i][1] for i in range(len(pts))) / 2
    if area < 0: pts = pts[::-1]
    return pts

bx, by, bn = basis([0, 1, 0])
assert r6(bx) == [-1, 0, 0] and r6(by) == [0, 0, 1], (bx, by)
rear_uv = plate_poly_uv(rear_xz); front_uv = plate_poly_uv(front_xz)
shapes = dict(tubes)
plates = {}
for side, sg in (('L', 1), ('R', -1)):
    y0 = PLATE_IN if sg > 0 else -(PLATE_IN + PLATE_T)
    plates['RD' + side] = dict(origin=[AX, y0, AZ], poly=rear_uv)
    shapes['RD' + side] = Prism([AX, y0, AZ], [0, 1, 0], PLATE_T, rear_uv)
    y0 = F_PLATE_IN if sg > 0 else -(F_PLATE_IN + PLATE_T)
    plates['FD' + side] = dict(origin=[AF0[0], y0, AF0[2]], poly=front_uv)
    shapes['FD' + side] = Prism([AF0[0], y0, AF0[2]], [0, 1, 0], PLATE_T, front_uv)
cx, cy, cn = basis(mul(h, -1))
assert r6(cx) == [0, 1, 0] and r6(cy) == r6(f), (cx, cy)
crown_uv = stadium_poly(CROWN_C, CROWN_R)
shapes['CROWN'] = Prism(C, mul(h, -1), CROWN_T, crown_uv)

# ---------------------------------------------------------------- cope tools
tools = {
    'T_HT': Cyl(add(HT_B, mul(h, 0.075)), h, -0.2, 0.2, 22*mm),
    'T_ST': Cyl([0, 0, 0], ST_D, 0, 0.6, 14.3*mm),
    'T_BB': Cyl([0, 0, 0], Y, -0.045, 0.045, 20*mm),
    'T_DT': Cyl([0, 0, 0], DT_D, 0, 0.1, 15.9*mm),
}
for k in ('RDL', 'RDR', 'FDL', 'FDR', 'CROWN'): tools['T_' + k] = shapes[k]
cuts = {
    'TT': ['T_HT', 'T_ST'], 'DT': ['T_HT', 'T_BB'], 'ST': ['T_BB', 'T_DT'],
    'CSL': ['T_BB', 'T_RDL'], 'CSR': ['T_BB', 'T_RDR'],
    'SSL': ['T_ST', 'T_RDL'], 'SSR': ['T_ST', 'T_RDR'],
    'BLL': ['T_CROWN', 'T_FDL'], 'BLR': ['T_CROWN', 'T_FDR'],
}
# ends that must be fully consumed by a cope (0 = start cap, 1 = end cap)
coped_ends = {'TT': [0, 1], 'DT': [0, 1], 'ST': [0], 'CSL': [0], 'CSR': [0], 'SSL': [1], 'SSR': [1], 'BLL': [1], 'BLR': [1]}

def alive(name, p):
    return not any(tools[t].contains(p) for t in cuts.get(name, []))

def slices(tb, a):
    x, y, _ = basis(tb.d)
    for rr in (tb.ri + 0.1*mm, (tb.ri + tb.ro) / 2, tb.ro - 0.1*mm):
        for k in range(36):
            th = 2 * math.pi * k / 36
            yield add(add(tb.p0, mul(tb.d, a)), add(mul(x, rr * math.cos(th)), mul(y, rr * math.sin(th))))

problems = []
STEP = 1*mm
for name, tb in tubes.items():
    L = tb.a1
    ats = [i * STEP for i in range(int(L / STEP) + 1)]
    ats = [min(max(a, 0.2*mm), L - 0.2*mm) for a in ats]
    live = []
    for a in ats:
        pts = list(slices(tb, a)); alv = [p for p in pts if alive(name, p)]
        live.append(len(alv))
        for p in alv:
            for other, sh in shapes.items():
                if other == name: continue
                if sh.contains(p) and (other not in tubes or alive(other, p)):
                    problems.append((name, other, round(a*1000, 1)))
    # fragments: live -> dead -> live along the tube
    state = 0; runs = 0
    for n_ in live:
        if n_ > 0 and state == 0: runs += 1
        state = 1 if n_ > 0 else 0
    if runs > 1: problems.append((name, 'FRAGMENTED', runs))
    for e in coped_ends.get(name, []):
        n_ = live[0] if e == 0 else live[-1]
        if n_: problems.append((name, 'END%d_NOT_CONSUMED' % e, n_))

from collections import Counter
summary = Counter((p[0], p[1]) for p in problems)
out = dict(
    SS_YE_mm=SS_YE*1000, A2C_mm=round(A2C*1000, 1), front_axle=r6(AF0),
    wheelbase_mm=round((AF0[0] - AX)*1000, 1), crown_seat=r6(C),
    problems={f'{a}|{b}': n for (a, b), n in summary.items()},
    problem_samples=problems[:12],
)
print(json.dumps(out, indent=1))

# ---------------------------------------------------------------- payloads
def annulus_region(R, r):
    k = math.sqrt(0.5)
    o = [[R, 0], [R*k, R*k], [0, R], [-R*k, R*k], [-R, 0], [-R*k, -R*k], [0, -R], [R*k, -R*k]]
    hl = [[r, 0], [r*k, -r*k], [0, -r], [-r*k, -r*k], [-r, 0], [-r*k, r*k], [0, r], [r*k, r*k]]
    o = [r6(p + [0])[:2] for p in o]; hl = [r6(p + [0])[:2] for p in hl]
    oe = [dict(kind='Arc', a=o[i], b=o[(i+1) % 8], center=[0, 0], radius=R, ccw=True) for i in range(8)]
    he = [dict(kind='Arc', a=hl[i], b=hl[(i+1) % 8], center=[0, 0], radius=r, ccw=False) for i in range(8)]
    return dict(outer=o, holes=[hl], area=round(math.pi*(R*R - r*r), 12), outer_edges=oe, hole_edges=[he], boundary_entity_ids=[1, 2])

payload = dict(
    stays={k: dict(origin=r6(v['origin']), normal=r6(v['normal']), depth=round(v['depth'], 6), axle=r6(v['axle'])) for k, v in stay.items()},
    blades={k: dict(origin=r6(v['origin']), normal=r6(v['normal']), depth=round(v['depth'], 6), axle=r6(v['axle'])) for k, v in fork.items()},
    plates={k: dict(origin=r6(v['origin']), poly=[[round(u, 6), round(w, 6)] for u, w in v['poly']]) for k, v in plates.items()},
    crown=dict(origin=r6(C), normal=r6(mul(h, -1)), depth=CROWN_T, c=CROWN_C, r=CROWN_R),
    steerer=dict(origin=r6(C), normal=r6(h), depth=0.22),
    dt_tool=dict(origin=[0, 0, 0], normal=r6(DT_D), depth=0.1),
    regions=dict(cs=annulus_region(0.0111, 0.0103), ss=annulus_region(0.008, 0.0072),
                 bl=annulus_region(0.011, 0.010), steer=annulus_region(0.0143, 0.0127)),
)
json.dump(payload, open(__file__.replace('.py', '.json'), 'w'), indent=1)
