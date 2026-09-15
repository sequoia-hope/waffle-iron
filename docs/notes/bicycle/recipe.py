"""Emit the ordered agent-link (MCP) calls that rebuild the bicycle frame + fork.

    python3 docs/notes/bicycle/recipe.py > docs/notes/bicycle/recipe.json

Each step is {"tool": ..., "args": {...}, "ref": name}. Later steps refer to
earlier results by "$ref:<name>" (feature id of that step's sketch/feature);
the executor substitutes the ids as it goes. Cut targets name
"$ref:<feature>/<OutputKey>" — see README "Cut plan" for the output-order rule.

All normals are normalized to full precision here (a 6-decimal normal breaks
circle extrudes and later booleans — failure log F1/F2). Region vertices are
exact on the circle (regions.py).
"""
import json, math, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from regions import region  # noqa: E402  (regions.py prints on import; silenced below)

def unit(v):
    n = math.sqrt(sum(x * x for x in v))
    return [x / n for x in v]

G = json.load(open(os.path.join(os.path.dirname(__file__), 'bike_geom_ye5.5.json')))
steps = []

def sketch_annulus(ref, origin, normal, od_expr, wall_expr, R, r):
    steps.append(dict(ref=ref, tool='sketch_create', args=dict(
        plane=dict(origin=origin, normal=unit(normal)),
        entities=[dict(type='Point', id=0, x=0, y=0), dict(type='Circle', id=1, center_id=0, radius=R),
                  dict(type='Circle', id=2, center_id=0, radius=r)],
        constraints=[dict(type='Pinned', point=0, x=0, y=0),
                     dict(type='Radius', entity=1, value=R, expression=f'{od_expr}/2'),
                     dict(type='Radius', entity=2, value=r, expression=f'{od_expr}/2 - {wall_expr}')])))

def sketch_circle(ref, origin, normal, od_expr, R):
    steps.append(dict(ref=ref, tool='sketch_create', args=dict(
        plane=dict(origin=origin, normal=unit(normal)),
        entities=[dict(type='Point', id=0, x=0, y=0), dict(type='Circle', id=1, center_id=0, radius=R)],
        constraints=[dict(type='Pinned', point=0, x=0, y=0),
                     dict(type='Radius', entity=1, value=R, expression=f'{od_expr}/2')])))

def sketch_poly(ref, origin, poly):
    n = len(poly)
    ents = [dict(type='Point', id=i, x=u, y=v) for i, (u, v) in enumerate(poly)]
    ents += [dict(type='Line', id=n + i, start_id=i, end_id=(i + 1) % n) for i in range(n)]
    cons = [dict(type='Pinned', point=i, x=u, y=v) for i, (u, v) in enumerate(poly)]
    steps.append(dict(ref=ref, tool='sketch_create', args=dict(
        plane=dict(origin=origin, normal=[0, 1, 0]), entities=ents, constraints=cons)))
    return list(range(n, 2 * n))

def extrude(ref, sketch, depth, *, region_=None, profile_ids=None, depth_expr=None,
            symmetric=False, cut_targets=None, name=None):
    p = dict(sketch_id=f'$ref:{sketch}', profile_index=0, depth=depth, direction=None,
             symmetric=symmetric, cut=bool(cut_targets), merge=False, target_body=None,
             depth_mode=dict(type='Blind'),
             combine=dict(type='Cut') if cut_targets else dict(type='NewBody'))
    if depth_expr: p['depth_expr'] = depth_expr
    if region_ is not None: p['region'] = region_
    if profile_ids is not None: p['profile_entity_ids'] = profile_ids
    if cut_targets:
        p['targets'] = [dict(kind=dict(type='Solid'),
                             anchor=dict(type='FeatureOutput', feature_id=f'$ref:{t.split("/")[0]}',
                                         output_key=(dict(type='Main') if t.endswith('/Main') else
                                                     dict(type='Body', index=int(t.split(':')[1])))),
                             selector=dict(type='Role', role=dict(type='EndCapPositive'), index=0),
                             policy=dict(type='Strict')) for t in cut_targets]
    steps.append(dict(ref=ref, tool='feature_add', body_name=name,
                      args=dict(operation=dict(type='Extrude', params=p))))

# ------------------------------------------------------------------ parameters
params = dict(ht_od='44', ht_wall='1.5', ht_len='150', tt_od='25.4', tt_wall='0.9', tt_len='560',
              dt_od='31.8', dt_wall='0.9', st_od='28.6', st_wall='0.9', st_len='560', bb_od='40',
              bb_wall='2.5', bb_width='68', cs_od='22.2', cs_wall='0.8', cs_len='410', bb_drop='70',
              rear_spacing='130', ss_od='16', ss_wall='0.8', fb_od='22', fb_wall='1', steer_od='28.6',
              steer_wall='1.6', steer_len='220', fork_rake='45', fork_a2c='335', front_spacing='100',
              drop_t='6', crown_t='20')
steps.append(dict(ref='params', tool='parameters_set',
                  args=dict(parameters=[dict(name=k, expression=v) for k, v in params.items()])))

H = [-0.292372, 0, 0.956305]; ST = [-0.284015, 0, 0.95882]; DT = [0.718286, 0, 0.695747]
# ------------------------------------------------------------------ main triangle (session 1)
sketch_annulus('sk_ht', [0.446016, 0, 0.388637], H, 'ht_od', 'ht_wall', 0.022, 0.0205)
sketch_annulus('sk_bb', [0, -0.034, 0], [0, 1, 0], 'bb_od', 'bb_wall', 0.02, 0.0175)
sketch_annulus('sk_tt', [-0.150531, 0, 0.508175], [1, 0, 0], 'tt_od', 'tt_wall', 0.0127, 0.0118)
sketch_annulus('sk_dt', [0, 0, 0], DT, 'dt_od', 'dt_wall', 0.0159, 0.015)
sketch_annulus('sk_st', [0, 0, 0], ST, 'st_od', 'st_wall', 0.0143, 0.0134)
extrude('ht', 'sk_ht', 0.15, region_=region(0.022, 0.0205), depth_expr='ht_len', name='Head tube')
extrude('bb', 'sk_bb', 0.068, region_=region(0.02, 0.0175), depth_expr='bb_width', name='BB shell')
extrude('tt', 'sk_tt', 0.56, region_=region(0.0127, 0.0118), depth_expr='tt_len', name='Top tube')
extrude('dt', 'sk_dt', 0.606698, region_=region(0.0159, 0.015), name='Down tube')
extrude('st', 'sk_st', 0.56, region_=region(0.0143, 0.0134), depth_expr='st_len', name='Seat tube')

# ------------------------------------------------------------------ rear triangle
for k, nm in (('CSL', 'Chainstay L'), ('CSR', 'Chainstay R'), ('SSL', 'Seatstay L'), ('SSR', 'Seatstay R')):
    s = G['stays'][k]; cs = k.startswith('CS')
    sketch_annulus('sk_' + k, s['origin'], s['normal'], 'cs_od' if cs else 'ss_od',
                   'cs_wall' if cs else 'ss_wall', 0.0111 if cs else 0.008, 0.0103 if cs else 0.0072)
    extrude(k, 'sk_' + k, s['depth'], region_=region(0.0111, 0.0103) if cs else region(0.008, 0.0072), name=nm)

# ------------------------------------------------------------------ dropouts
plate_ids = {}
for k, nm in (('RDL', 'Rear dropout L'), ('RDR', 'Rear dropout R'), ('FDL', 'Fork dropout L'), ('FDR', 'Fork dropout R')):
    pl = G['plates'][k]
    plate_ids[k] = sketch_poly('sk_' + k, pl['origin'], pl['poly'])
    steps[-1]['ref'] = 'sk_' + k
    extrude(k, 'sk_' + k, 0.006, profile_ids=plate_ids[k], depth_expr='drop_t', name=nm)

# ------------------------------------------------------------------ fork
cr = G['crown']
steps.append(dict(ref='sk_crown', tool='sketch_create', args=dict(
    plane=dict(origin=cr['origin'], normal=unit(cr['normal'])),
    entities=[dict(type='Point', id=0, x=0.04, y=0), dict(type='Point', id=1, x=-0.04, y=0),
              dict(type='Point', id=2, x=0.04, y=-0.016), dict(type='Point', id=3, x=0.04, y=0.016),
              dict(type='Point', id=4, x=-0.04, y=0.016), dict(type='Point', id=5, x=-0.04, y=-0.016),
              dict(type='Arc', id=6, center_id=0, start_id=2, end_id=3), dict(type='Line', id=7, start_id=3, end_id=4),
              dict(type='Arc', id=8, center_id=1, start_id=4, end_id=5), dict(type='Line', id=9, start_id=5, end_id=2)],
    constraints=[dict(type='Pinned', point=i, x=x, y=y) for i, (x, y) in
                 enumerate([(0.04, 0), (-0.04, 0), (0.04, -0.016), (0.04, 0.016), (-0.04, 0.016), (-0.04, -0.016)])])))
crown_region = dict(outer=[[0.04, -0.016], [0.056, 0], [0.04, 0.016], [-0.04, 0.016], [-0.056, 0], [-0.04, -0.016]],
                    holes=[], area=0.08 * 0.032 + math.pi * 0.016 ** 2, hole_edges=[], boundary_entity_ids=[6, 7, 8, 9],
                    outer_edges=[dict(kind='Arc', a=[0.04, -0.016], b=[0.056, 0], center=[0.04, 0], radius=0.016, ccw=True),
                                 dict(kind='Arc', a=[0.056, 0], b=[0.04, 0.016], center=[0.04, 0], radius=0.016, ccw=True),
                                 dict(kind='Line', a=[0.04, 0.016], b=[-0.04, 0.016]),
                                 dict(kind='Arc', a=[-0.04, 0.016], b=[-0.056, 0], center=[-0.04, 0], radius=0.016, ccw=True),
                                 dict(kind='Arc', a=[-0.056, 0], b=[-0.04, -0.016], center=[-0.04, 0], radius=0.016, ccw=True),
                                 dict(kind='Line', a=[-0.04, -0.016], b=[0.04, -0.016])])
extrude('crown', 'sk_crown', 0.02, region_=crown_region, depth_expr='crown_t', name='Fork crown')
st_ = G['steerer']
sketch_annulus('sk_steer', st_['origin'], st_['normal'], 'steer_od', 'steer_wall', 0.0143, 0.0127)
extrude('steer', 'sk_steer', 0.22, region_=region(0.0143, 0.0127), depth_expr='steer_len', name='Steerer')
for k, nm in (('BLL', 'Fork blade L'), ('BLR', 'Fork blade R')):
    b = G['blades'][k]
    sketch_annulus('sk_' + k, b['origin'], b['normal'], 'fb_od', 'fb_wall', 0.011, 0.010)
    extrude(k, 'sk_' + k, b['depth'], region_=region(0.011, 0.010), name=nm)

steps.append(dict(ref='save_uncut', tool='document_save', args={}))

# ------------------------------------------------------------------ cope tools (sketches)
sketch_circle('sk_T_HT', [0.424088, 0, 0.46036], H, 'ht_od', 0.022)
sketch_circle('sk_T_ST', [0, 0, 0], ST, 'st_od', 0.0143)
sketch_circle('sk_T_BB', [0, 0, 0], [0, 1, 0], 'bb_od', 0.02)
sketch_circle('sk_T_DT', [0, 0, 0], DT, 'dt_od', 0.0159)

# ------------------------------------------------------------------ cuts: ONE per call, save after each
# Output rule: a Cut's outputs are <feature>/Main, /Body:1, ... in TARGET ORDER.
# F9: never target only some outputs of a multi-output Cut; target all of them.
def cut(ref, sketch, depth, targets, *, region_=None, profile_ids=None, symmetric=False, depth_expr=None, note=''):
    extrude(ref, sketch, depth, region_=region_, profile_ids=profile_ids, symmetric=symmetric,
            depth_expr=depth_expr, cut_targets=targets)
    steps[-1]['note'] = note
    steps.append(dict(ref='save_' + ref, tool='document_save', args={}))

cut('c_ht', 'sk_T_HT', 0.2, ['tt/Main', 'dt/Main'], region_=region(0.022), symmetric=True,
    note='WORKED 2026-09-14. out Main=top tube, Body:1=down tube')
cut('c_bb_st', 'sk_T_BB', 0.045, ['st/Main'], region_=region(0.02), symmetric=True, note='WORKED')
cut('c_bb_dt', 'sk_T_BB', 0.045, ['c_ht/Main', 'c_ht/Body:1'], region_=region(0.02), symmetric=True,
    note='WORKED with BOTH outputs targeted (single-target deleted the top tube, F9). out Main=top tube, Body:1=down tube')
cut('c_bb_csl', 'sk_T_BB', 0.045, ['CSL/Main'], region_=region(0.02), symmetric=True, note='WORKED')
cut('c_bb_csr', 'sk_T_BB', 0.045, ['CSR/Main'], region_=region(0.02), symmetric=True, note='WORKED')
cut('c_rdl', 'sk_RDL', 0.006, ['c_bb_csl/Main', 'SSL/Main'], profile_ids=plate_ids['RDL'], depth_expr='drop_t',
    note='WORKED (chainstay L -0.26 cm3, seatstay L -1.19 cm3). out Main=CSL, Body:1=SSL')
cut('c_rdr', 'sk_RDR', 0.006, ['c_bb_csr/Main', 'SSR/Main'], profile_ids=plate_ids['RDR'], depth_expr='drop_t',
    note='NOT YET RUN (tab reloaded while it was queued)')
cut('c_dt_st', 'sk_T_DT', 0.1, ['c_bb_st/Main'], region_=region(0.0159), note='NOT YET RUN')
cut('c_fdl', 'sk_FDL', 0.006, ['BLL/Main'], profile_ids=plate_ids['FDL'], depth_expr='drop_t', note='NOT YET RUN')
cut('c_fdr', 'sk_FDR', 0.006, ['BLR/Main'], profile_ids=plate_ids['FDR'], depth_expr='drop_t', note='NOT YET RUN')
cut('c_crown', 'sk_crown', 0.02, ['c_fdl/Main', 'c_fdr/Main'], region_=crown_region, depth_expr='crown_t',
    note='NOT YET RUN')
# BLOCKED by the kernel (F8) — kept for when the Yang tail is fixed:
cut('c_st', 'sk_T_ST', 0.6, ['c_bb_dt/Main', 'c_rdl/Body:1', 'c_rdr/Body:1'], region_=region(0.0143),
    note='BLOCKED F8: top tube -> TessellationFailed; fresh seatstay -> Stage-4 LocalRefinementRequired')

json.dump(dict(generated_by='docs/notes/bicycle/recipe.py', steps=steps), sys.stdout, indent=1)
