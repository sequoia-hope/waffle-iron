import json, math
def pts(R, cw=False):
    k = [(math.cos(i*math.pi/4), math.sin(i*math.pi/4)) for i in range(8)]
    p = [[R*c if abs(c) > 1e-12 else 0.0, R*s if abs(s) > 1e-12 else 0.0] for c, s in k]
    # snap exact axis points
    for q in p:
        for j in (0, 1):
            if abs(abs(q[j]) - R) < 1e-15: q[j] = math.copysign(R, q[j])
    if cw: p = [p[0]] + p[:0:-1]
    return p
def edges(p, R, ccw):
    return [dict(kind='Arc', a=p[i], b=p[(i+1) % 8], center=[0.0, 0.0], radius=R, ccw=ccw) for i in range(8)]
def region(R, r=None):
    o = pts(R); d = dict(outer=o, holes=[], area=math.pi*R*R, outer_edges=edges(o, R, True), hole_edges=[], boundary_entity_ids=[1])
    if r:
        h = pts(r, cw=True)
        d.update(holes=[h], hole_edges=[edges(h, r, False)], area=math.pi*(R*R - r*r), boundary_entity_ids=[1, 2])
    for q in o + (d['holes'][0] if r else []):
        assert abs(math.hypot(*q) - (R if q in o else r)) < 1e-17, q
    return d
if __name__ == '__main__':
    out = {n: region(*a) for n, a in dict(
        TT=(0.0127, 0.0118), DT=(0.0159, 0.015), ST=(0.0143, 0.0134),
        CS=(0.0111, 0.0103), SS=(0.008, 0.0072), BL=(0.011, 0.010),
        T_HT=(0.022,), T_ST=(0.0143,), T_BB=(0.02,), T_DT=(0.0159,)).items()}
    for n, d in out.items():
        print(n, json.dumps(d, separators=(',', ':')))
