#[allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn corner_transit_apply(
    mesh: &mut Mesh,
    attribution: &mut TriangleAttributionMap,
    side_labels: &mut TriangleSideLabelMap,
    a: &BRep,
    b: &BRep,
    entry: &[[f64; 3]],
    full_arrangement: bool,
) -> Result<bool, YangError> {
    use crate::stage4_corridor as s4c;
    use crate::stage4_transit as s4t;
    let fires = corner_crossing_fires(mesh, attribution, a, b, entry);
    if fires.is_empty() {
        return Ok(false);
    }
    let nv = mesh.verts.len();
    let refuse = move |why: &str| {
        eprintln!("[451-transit] REFUSE (verts={nv}): {why} — the standing §4-I9 STOP applies");
    };
    eprintln!("[451-transit] fires={fires:?} verts={nv}");
    let Some(d_eps) = stage4_chord_band(a, b) else {
        refuse("no chord band");
        return Ok(false);
    };
    // Mesh-side lookups (the census constructions, quiet).
    let mut patch_map: std::collections::BTreeMap<u32, std::collections::BTreeSet<(InputId, u32)>> =
        std::collections::BTreeMap::new();
    for (ti, tri) in mesh.tris.iter().enumerate() {
        if let Some(Some(att)) = attribution.attributions.get(ti) {
            for &tv in tri {
                patch_map
                    .entry(tv)
                    .or_default()
                    .insert((att.input, att.face));
            }
        }
    }
    let empty: std::collections::BTreeSet<(InputId, u32)> = Default::default();
    let patches_of = |t: u32| -> &std::collections::BTreeSet<(InputId, u32)> {
        patch_map.get(&t).unwrap_or(&empty)
    };
    let adj = build_live_adjacency(mesh);
    let fired: std::collections::BTreeSet<u32> = fires.iter().map(|&(v, _)| v).collect();
    let d3 = |x: [f64; 3], y: [f64; 3]| {
        ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt()
    };
    let faces_of = |op: InputId| match op {
        InputId::A => a.faces(),
        InputId::B => b.faces(),
    };
    // Per-site planning + APPLY-semantics walks.
    let mut walks: Vec<s4t::SiteWalk> = Vec::new();
    for &(v, q) in &fires {
        let (pv, pq) = (patches_of(v), patches_of(q));
        let qpos = mesh.verts[q as usize].as_array();
        let site = match s4t::read_site(a, b, pv, pq, qpos) {
            Ok(s) => s,
            Err(d) => {
                refuse(&format!("site v{v}: {d:?}"));
                return Ok(false);
            }
        };
        if let Err(d) = s4t::classify(&site, qpos) {
            refuse(&format!("site v{v}: {d:?}"));
            return Ok(false);
        }
        let brep_n = match site.next.0 {
            InputId::A => a,
            InputId::B => b,
        };
        let Some(far_surf) = faces_of(site.far.0)
            .get(site.far.1 as usize)
            .map(|f| f.surface)
        else {
            refuse(&format!("site v{v}: far face missing"));
            return Ok(false);
        };
        let walk_adj = s4t::build_edge_adjacency(brep_n);
        let (far_patch, next_op) = (site.far, site.next.0);
        let existing = |ff: u32, ft: u32, pos: [f64; 3]| -> Option<(u32, f64)> {
            let want = [far_patch, (next_op, ff), (next_op, ft)];
            patch_map
                .iter()
                .filter(|(w, ps)| !fired.contains(w) && want.iter().all(|t| ps.contains(t)))
                .map(|(&w, _)| (w, d3(mesh.verts[w as usize].as_array(), pos)))
                .min_by(|x, y| x.1.total_cmp(&y.1))
        };
        let ctx = s4t::WalkCtx {
            brep: brep_n,
            far: far_surf,
            adj: &walk_adj,
            existing: &existing,
            splice_band: s4t::WalkBand::Contract,
        };
        for c in &site.cands {
            if !c.real {
                continue;
            }
            let (Some(sa), Some(er)) = (c.sol, c.edge.as_ref()) else {
                continue;
            };
            let Some(other) = site.cands.iter().find(|o| o.shared != c.shared) else {
                continue;
            };
            if c.shared.0 != site.next.0 || other.shared.0 != site.next.0 {
                refuse(&format!("site v{v}: cross-operand shape"));
                return Ok(false);
            }
            let (juncs, end) = s4t::walk_corridor(
                &ctx,
                s4t::WalkStart {
                    face: site.next.1,
                    entry_key: s4t::edge_key(brep_n, er.edge),
                    entry: sa,
                },
                other.shared.1,
                64,
            );
            walks.push(s4t::SiteWalk {
                site: v,
                corner: q,
                far: site.far,
                far_surf,
                walk_op: site.next.0,
                entry: s4t::WalkJunction {
                    face_from: c.shared.1,
                    face_to: site.next.1,
                    edge: er.edge,
                    sol: sa,
                    d_on_edge: er.d_on_edge,
                },
                juncs,
                end,
            });
        }
    }
    // Assembly.
    let existing_g =
        |far: (InputId, u32), op: InputId, ff: u32, ft: u32, pos: [f64; 3]| -> Option<(u32, f64)> {
            let want = [far, (op, ff), (op, ft)];
            patch_map
                .iter()
                .filter(|(w, ps)| !fired.contains(w) && want.iter().all(|t| ps.contains(t)))
                .map(|(&w, _)| (w, d3(mesh.verts[w as usize].as_array(), pos)))
                .min_by(|x, y| x.1.total_cmp(&y.1))
        };
    let facet_chain = |far: (InputId, u32), op: InputId, f: u32| -> Vec<s4t::FacetChain> {
        let want = [far, (op, f)];
        let members: std::collections::BTreeSet<u32> = patch_map
            .iter()
            .filter(|(w, ps)| !fired.contains(w) && want.iter().all(|t| ps.contains(t)))
            .map(|(&w, _)| w)
            .collect();
        let ind_nbrs = |x: u32| -> Vec<u32> {
            adj.get(&x)
                .into_iter()
                .flatten()
                .copied()
                .filter(|y| members.contains(y))
                .collect()
        };
        let mut seen: std::collections::BTreeSet<u32> = Default::default();
        let mut comps = Vec::new();
        for &s in &members {
            if seen.contains(&s) {
                continue;
            }
            let mut comp = std::collections::BTreeSet::from([s]);
            let mut stack = vec![s];
            seen.insert(s);
            while let Some(x) = stack.pop() {
                for y in ind_nbrs(x) {
                    if comp.insert(y) {
                        seen.insert(y);
                        stack.push(y);
                    }
                }
            }
            let degs: Vec<(u32, usize)> = comp
                .iter()
                .map(|&x| (x, ind_nbrs(x).iter().filter(|y| comp.contains(y)).count()))
                .collect();
            let ends: Vec<u32> = degs
                .iter()
                .filter(|&&(_, d)| d <= 1)
                .map(|&(x, _)| x)
                .collect();
            let is_path = degs.iter().all(|&(_, d)| d <= 2) && (comp.len() == 1 || ends.len() == 2);
            let verts: Vec<(u32, [f64; 3])> = if is_path && comp.len() > 1 {
                let mut order = vec![*ends.iter().min().expect("two ends")];
                let mut prev: Option<u32> = None;
                while order.len() < comp.len() {
                    let x = *order.last().expect("non-empty");
                    let nxt = ind_nbrs(x)
                        .into_iter()
                        .find(|&y| comp.contains(&y) && Some(y) != prev);
                    match nxt {
                        Some(y) => {
                            prev = Some(x);
                            order.push(y);
                        }
                        None => break,
                    }
                }
                order
                    .iter()
                    .map(|&x| (x, mesh.verts[x as usize].as_array()))
                    .collect()
            } else {
                comp.iter()
                    .map(|&x| (x, mesh.verts[x as usize].as_array()))
                    .collect()
            };
            comps.push(s4t::FacetChain {
                path: is_path && verts.len() == comp.len(),
                verts,
            });
        }
        comps
    };
    let face_surface = |op: InputId, f: u32| -> Option<Surface> {
        faces_of(op).get(f as usize).map(|x| x.surface)
    };
    let actx = s4t::AssembleCtx {
        existing: &existing_g,
        facet_chain: &facet_chain,
        face_surface: &face_surface,
        d_eps,
    };
    let (corridors, adeclines) = s4t::assemble_corridors(&actx, &walks);
    if !adeclines.is_empty() {
        refuse(&format!("assembly declines: {}", adeclines.len()));
        return Ok(false);
    }
    if !(corridors.iter().all(|c| c.applyable()) && !corridors.is_empty()) {
        refuse("not every corridor is applyable");
        return Ok(false);
    }
    let consumed: std::collections::BTreeSet<u32> = corridors
        .iter()
        .flat_map(|c| c.phantoms.iter().copied())
        .collect();
    if consumed != fired {
        refuse("not every fired site is consumed");
        return Ok(false);
    }
    // Components + cycle planning.
    let adjacency_t = crate::stage5_topology::triangle_adjacency(mesh);
    let raw_patches = crate::stage5_topology::flood_fill_patches_with_side_labels(
        mesh,
        attribution,
        full_arrangement.then_some(&*side_labels),
        &adjacency_t,
    );
    let vband = |v: u32| -> f64 {
        let p = mesh.verts[v as usize].as_array();
        cad_primitives::TAU_WORK
            .max(8.0 * f64::EPSILON * p[0].abs().max(p[1].abs()).max(p[2].abs()))
    };
    let cycles_of = |pi: usize| -> Option<Vec<Vec<u32>>> {
        crate::stage5_topology::patch_boundary_cycle(&raw_patches[pi], mesh)
            .ok()
            .map(|cy| {
                cy.iter()
                    .map(|c| c.iter().map(|&(s, _)| s).collect())
                    .collect()
            })
    };
    // inc-2c-3b-9b (spec §3q) — the CONTINUATION arm. A corridor whose
    // terminal junction meets a carried chain that immediately doubles back
    // (the §3j connector certificate) has not reached the healthy
    // resumption. When the defective span carries CREASE-CROSSING vertices
    // (≥2 walk-operand faces — carried junctions), the curve provably
    // continues across further boundary curves (their in-domain rim roots
    // exist — the `[451-bleg-rim]` census), and the corridor EXTENDS:
    // resume the walk from the terminal junction with the span excluded,
    // mint the true crossings, and terminate on the healthy anchor's face
    // (the paper's Fig-12(e): the intersections between the curve and each
    // boundary curve are solved). The span is recorded as the corridor's
    // extension-certified `absorbed` set for the planner's removability and
    // outward anchor resolution. A span WITHOUT crossers is the classic
    // §3j absorb — untouched. (The report-only `-PLAN3B` census mirror
    // does not extend; its rows show the unextended corridors.)
    let mut corridors = corridors;
    let ext_census = std::env::var("YANG_451_HOSTS").as_deref() == Ok("census");
    let standing_at = |far: (InputId, u32), walk_op: InputId, v: u32| -> bool {
        // Two triple shapes (spec §3q): a WALK-OP crease crossing
        // {far, f1, f2} (one far-op + two walk-op faces), and a FAR-OP
        // crease crossing {fa1, fa2, wb} (two far-op + one walk-op face —
        // the intersection curve passing from one far face onto its
        // neighbour: R0044's v107 on {A2, A3, B1}).
        let pv = patches_of(v);
        let far_op = far.0;
        let wf: Vec<u32> = pv
            .iter()
            .filter(|(op, _)| *op == walk_op)
            .map(|&(_, f)| f)
            .collect();
        let ff: Vec<u32> = pv
            .iter()
            .filter(|(op, _)| *op == far_op)
            .map(|&(_, f)| f)
            .collect();
        let surfaces: Option<(Surface, Surface, Surface)> = match (ff.as_slice(), wf.as_slice()) {
            (_, [f1, f2]) => match (
                faces_of(far.0).get(far.1 as usize).map(|f| f.surface),
                faces_of(walk_op).get(*f1 as usize).map(|f| f.surface),
                faces_of(walk_op).get(*f2 as usize).map(|f| f.surface),
            ) {
                (Some(x), Some(y), Some(z)) => Some((x, y, z)),
                _ => None,
            },
            ([a1, a2], [w1]) => match (
                faces_of(far_op).get(*a1 as usize).map(|f| f.surface),
                faces_of(far_op).get(*a2 as usize).map(|f| f.surface),
                faces_of(walk_op).get(*w1 as usize).map(|f| f.surface),
            ) {
                (Some(x), Some(y), Some(z)) => Some((x, y, z)),
                _ => None,
            },
            _ => None,
        };
        let Some((s0, s1, s2)) = surfaces else {
            return false;
        };
        let pos = mesh.verts[v as usize];
        crate::stage4_relocate::relocate_onto_implicit_triple(pos, s0, s1, s2).is_some_and(|sol| {
            let pa = pos.as_array();
            d3(sol.as_array(), pa)
                <= s4t::contract_band(pa[0].abs().max(pa[1].abs()).max(pa[2].abs()))
        })
    };
    #[allow(clippy::needless_range_loop)]
    for k in 0..corridors.len() {
        let c0 = corridors[k].clone();
        let Some((fpi, _)) = raw_patches.iter().enumerate().find(|(_, pt)| {
            (pt.attribution.input, pt.attribution.face) == c0.far
                && pt.tri_indices.iter().any(|&t| {
                    let tri = mesh.tris[t as usize];
                    c0.phantoms.iter().any(|&p| tri.contains(&p))
                })
        }) else {
            continue;
        };
        let Some(fcycles) = cycles_of(fpi) else {
            continue;
        };
        let standing_v = |_k: usize, v: u32| standing_at(c0.far, c0.walk_op, v);
        let att_junction = |w: u32| -> Option<usize> {
            let pw = patches_of(w);
            let hits: Vec<usize> = c0
                .junctions
                .iter()
                .enumerate()
                .filter(|(_, j)| {
                    pw.contains(&(c0.walk_op, j.faces.0)) || pw.contains(&(c0.walk_op, j.faces.1))
                })
                .map(|(i, _)| i)
                .collect();
            if let [one] = hits.as_slice() {
                Some(*one)
            } else {
                None
            }
        };
        let mut extend_at: Vec<(usize, Vec<u32>, u32)> = Vec::new(); // (end junction idx, span, healthy anchor)
        for cy in &fcycles {
            let n = cy.len();
            let Some(p_i) = cy.iter().position(|v| c0.phantoms.contains(v)) else {
                continue;
            };
            for dir in [-1isize, 1isize] {
                // The end junction this direction belongs to: the first
                // non-fired vertex with a UNIQUE attachment under the
                // PRE-extension corridor.
                let mut je: Option<usize> = None;
                let mut kx = p_i as isize;
                for _ in 0..n {
                    kx = (kx + dir).rem_euclid(n as isize);
                    let v = cy[kx as usize];
                    if fired.contains(&v) {
                        continue;
                    }
                    if let Some(j) = att_junction(v) {
                        je = Some(j);
                        break;
                    }
                    // Keep scanning: mid-face curve vertices resolve nothing.
                }
                let Some(je) = je else { continue };
                let last = c0.junctions.len() - 1;
                if je != 0 && je != last {
                    continue;
                }
                // Only a MINT terminal can extend: a Splice end IS an
                // existing healthy junction — the existing curve owns the
                // continuation there, and its defects (if any) are not this
                // corridor's fire (measured: the v513 corridors' splice
                // ends read their healthy continuations as spans via the
                // degenerate connect≈0 of the splice vertex itself).
                if !matches!(c0.junctions[je].disposition, s4t::JunctionDisposition::Mint) {
                    continue;
                }
                let jsol = c0.junctions[je].sol;
                // The connector walk from the phantom outward, anchored on
                // the terminal junction: defective while the arrive ring
                // doubles back toward it. Fired vertices are skipped
                // (already consumed); the first healthy vertex is the
                // resumption anchor.
                let mut span: Vec<u32> = Vec::new();
                let mut anchor: Option<u32> = None;
                let mut kx = p_i as isize;
                for _ in 0..n {
                    kx = (kx + dir).rem_euclid(n as isize);
                    let v = cy[kx as usize];
                    if fired.contains(&v) {
                        continue;
                    }
                    let pv = mesh.verts[v as usize].as_array();
                    if d3(pv, jsol)
                        <= s4t::contract_band(pv[0].abs().max(pv[1].abs()).max(pv[2].abs()))
                    {
                        // The junction's own carrier — connect ≈ 0 is
                        // degenerate testimony; skip it.
                        continue;
                    }
                    // A STANDING junction anchors the walk, whatever its
                    // ring angles read (spec §3q: v157 — corridor #4's
                    // healthy splice junction — read doubled-back against
                    // corridor #5's mint 74 away).
                    if standing_v(k, v) {
                        anchor = Some(v);
                        break;
                    }
                    let further = cy[(kx + dir).rem_euclid(n as isize) as usize];
                    let pf = mesh.verts[further as usize].as_array();
                    let arrive = [pv[0] - pf[0], pv[1] - pf[1], pv[2] - pf[2]];
                    let connect = [jsol[0] - pv[0], jsol[1] - pv[1], jsol[2] - pv[2]];
                    let dot =
                        arrive[0] * connect[0] + arrive[1] * connect[1] + arrive[2] * connect[2];
                    if dot >= 0.0 {
                        anchor = Some(v);
                        break;
                    }
                    span.push(v);
                }
                let Some(anchor) = anchor else { continue };
                if span.is_empty() {
                    continue;
                }
                let crossers: Vec<u32> = span
                    .iter()
                    .copied()
                    .filter(|&v| {
                        patches_of(v)
                            .iter()
                            .filter(|(op, _)| *op == c0.walk_op)
                            .count()
                            >= 2
                    })
                    .collect();
                if ext_census {
                    eprintln!(
                        "[451-ext] #{k} end={je} span={span:?} crossers={crossers:?} \
                         anchor=v{anchor}"
                    );
                }
                if crossers.is_empty() {
                    continue; // the classic absorb's territory
                }
                // A STANDING anchor (a healthy existing junction) rides
                // TWO walk-op faces; the walk then terminates on the
                // junction itself (ReachedExistingJunction) rather than by
                // crossing into a face — pass the never-matching sentinel.
                let aface = if standing_v(k, anchor) {
                    u32::MAX
                } else {
                    let afaces: Vec<u32> = patches_of(anchor)
                        .iter()
                        .filter(|(op, _)| *op == c0.walk_op)
                        .map(|&(_, f)| f)
                        .collect();
                    let [one] = afaces.as_slice() else {
                        refuse(&format!(
                            "continuation anchor v{anchor} of corridor #{k} rides {} \
                             walk-op faces — no unambiguous termination face",
                            afaces.len()
                        ));
                        return Ok(false);
                    };
                    *one
                };
                extend_at.push((je, span, aface));
            }
        }
        for (je, span, aface) in extend_at {
            let c0 = corridors[k].clone();
            let last = c0.junctions.len() - 1;
            let je_idx = if je == 0 { 0 } else { c0.junctions.len() - 1 };
            let je_j = &c0.junctions[je_idx];
            let f_out = if je_idx == 0 {
                je_j.faces.0
            } else {
                je_j.faces.1
            };
            let brep_n = match c0.walk_op {
                InputId::A => a,
                InputId::B => b,
            };
            let Some(far_surf) = faces_of(c0.far.0).get(c0.far.1 as usize).map(|f| f.surface)
            else {
                refuse("continuation: far surface missing");
                return Ok(false);
            };
            let walk_adj = s4t::build_edge_adjacency(brep_n);
            let excl: std::collections::BTreeSet<u32> =
                fired.iter().copied().chain(span.iter().copied()).collect();
            let existing_x = |ff: u32, ft: u32, pos: [f64; 3]| -> Option<(u32, f64)> {
                let want = [c0.far, (c0.walk_op, ff), (c0.walk_op, ft)];
                patch_map
                    .iter()
                    .filter(|(w, ps)| !excl.contains(w) && want.iter().all(|t| ps.contains(t)))
                    .map(|(&w, _)| (w, d3(mesh.verts[w as usize].as_array(), pos)))
                    .min_by(|x, y| x.1.total_cmp(&y.1))
            };
            let wctx = s4t::WalkCtx {
                brep: brep_n,
                far: far_surf,
                adj: &walk_adj,
                existing: &existing_x,
                splice_band: s4t::WalkBand::Contract,
            };
            let (juncs, end) = s4t::walk_corridor(
                &wctx,
                s4t::WalkStart {
                    face: f_out,
                    entry_key: s4t::edge_key(brep_n, je_j.edge),
                    entry: je_j.sol,
                },
                aface,
                64,
            );
            if ext_census {
                eprintln!(
                    "[451-ext] #{k} end={je} walk juncs={:?} end={end:?}",
                    juncs
                        .iter()
                        .map(|j| ((j.face_from, j.face_to), j.sol))
                        .collect::<Vec<_>>(),
                );
            }
            if !matches!(
                end,
                s4t::WalkEnd::ReachedOtherChain | s4t::WalkEnd::ReachedExistingJunction { .. }
            ) || juncs.is_empty()
            {
                refuse(&format!(
                    "continuation walk of corridor #{k} end {je} failed: {end:?} after {} \
                     junctions",
                    juncs.len()
                ));
                return Ok(false);
            }
            // Build the extension junctions + fresh runs.
            let dispose = |ff: u32, ft: u32, sol: [f64; 3]| -> s4t::JunctionDisposition {
                match existing_x(ff, ft, sol) {
                    Some((v, d))
                        if d <= s4t::contract_band(
                            sol[0].abs().max(sol[1].abs()).max(sol[2].abs()),
                        ) =>
                    {
                        s4t::JunctionDisposition::Splice { vertex: v, d }
                    }
                    _ => s4t::JunctionDisposition::Mint,
                }
            };
            let mut new_j: Vec<s4t::CorridorJunction> = juncs
                .iter()
                .map(|wj| s4t::CorridorJunction {
                    sol: wj.sol,
                    faces: (wj.face_from, wj.face_to),
                    edge: wj.edge,
                    disposition: dispose(wj.face_from, wj.face_to, wj.sol),
                })
                .collect();
            let source_run = |facet: u32, p0: [f64; 3], p1: [f64; 3]| {
                let fs = faces_of(c0.walk_op).get(facet as usize).map(|f| f.surface);
                match fs.and_then(|fs| s4t::sample_run_chord(far_surf, fs, p0, p1, d_eps)) {
                    Some(s) => Ok(s4t::RunSource::Samples(s)),
                    None => Err(s4t::RunIssue::SampleFailed),
                }
            };
            let cmut = &mut corridors[k];
            if je_idx == last && je_idx != 0 {
                let mut prev = cmut.junctions[cmut.junctions.len() - 1].sol;
                for nj in new_j {
                    let facet = nj.faces.0;
                    cmut.runs.push((facet, source_run(facet, prev, nj.sol)));
                    prev = nj.sol;
                    cmut.junctions.push(nj);
                }
            } else {
                // Prepend reversed with the face pairs swapped so the
                // corridor stays oriented j0 → j_last along the curve.
                new_j.reverse();
                for nj in &mut new_j {
                    nj.faces = (nj.faces.1, nj.faces.0);
                }
                let mut add_runs: Vec<(u32, Result<s4t::RunSource, s4t::RunIssue>)> = Vec::new();
                for w in new_j.windows(2) {
                    let facet = w[0].faces.1;
                    add_runs.push((facet, source_run(facet, w[0].sol, w[1].sol)));
                }
                let facet = new_j[new_j.len() - 1].faces.1;
                add_runs.push((
                    facet,
                    source_run(facet, new_j[new_j.len() - 1].sol, cmut.junctions[0].sol),
                ));
                let mut all_j = new_j;
                all_j.append(&mut cmut.junctions);
                cmut.junctions = all_j;
                add_runs.append(&mut cmut.runs);
                cmut.runs = add_runs;
            }
            cmut.absorbed.extend(span);
            cmut.absorbed.sort_unstable();
            cmut.absorbed.dedup();
        }
    }
    // The extension added runs — re-check applyability (a run that cannot
    // be sourced keeps the standing STOP).
    if !corridors.iter().all(|c| c.applyable()) {
        refuse("a continuation-extended corridor is not applyable");
        return Ok(false);
    }
    let corridors = corridors;
    let hosts_on = |k: usize,
                    patch: &crate::stage5_topology::Patch,
                    cycles_v: &[Vec<u32>]|
     -> Vec<s4c::HostEdge> {
        let c = &corridors[k];
        let census = std::env::var("YANG_451_HOSTS").as_deref() == Ok("census");
        let mut out = Vec::new();
        for (ji, j) in c.junctions.iter().enumerate() {
            if !matches!(j.disposition, s4t::JunctionDisposition::Mint) {
                continue;
            }
            let scale = j.sol.iter().fold(0.0f64, |m, &x| m.max(x.abs()));
            let eval = cad_primitives::TAU_WORK.max(8.0 * f64::EPSILON * scale);
            let contract = s4t::contract_band(scale);
            // §3i's arc-host wall (inc-2c-3b-4): today's eval arm verbatim,
            // plus the CERTIFIED ARC arm — d ≤ max(eval, d_eps) (the chord
            // sag the tessellation certifies), interior chord projection,
            // and the junction ON the chord's own curve: both surfaces the
            // edge separates, at the junction, within the CONTRACT band
            // (the splice-identity yardstick). Measured on R0044: true
            // hosts ≤ 5e-13 vs off-curve components at 1e-1…9e1 — the
            // blunt d_eps band alone admitted those and their HostMismatch
            // declines killed whole plans. Rule: `s4t::arc_host_admit`.
            let val_at = |inp: InputId, f: u32| -> Option<f64> {
                let s = faces_of(inp).get(f as usize)?.surface;
                surface_value_and_normal(s, j.sol).map(|(v, _)| v)
            };
            let on_curve = |x: u32, y: u32| -> bool {
                val_at(patch.attribution.input, patch.attribution.face)
                    .is_some_and(|v| v.abs() <= contract)
                    && mesh.tris.iter().enumerate().any(|(ti, tri)| {
                        tri.contains(&x)
                            && tri.contains(&y)
                            && !patch.tri_indices.contains(&(ti as u32))
                            && attribution
                                .attributions
                                .get(ti)
                                .and_then(|a| a.as_ref())
                                .and_then(|att| val_at(att.input, att.face))
                                .is_some_and(|v| v.abs() <= contract)
                    })
            };
            for cy in cycles_v {
                let n = cy.len();
                for i in 0..n {
                    let (x, y) = (cy[i], cy[(i + 1) % n]);
                    let (px, py) = (
                        mesh.verts[x as usize].as_array(),
                        mesh.verts[y as usize].as_array(),
                    );
                    let admit = s4t::arc_host_admit(j.sol, px, py, eval, d_eps, || on_curve(x, y));
                    if census {
                        // inc-2c-3b-4 census probe (`YANG_451_HOSTS=census`,
                        // default off): every candidate within the wide band,
                        // with the discriminator data and the verdict.
                        let d = s4t::dist_point_segment(j.sol, px, py);
                        if d <= eval.max(d_eps) {
                            let e = [py[0] - px[0], py[1] - px[1], py[2] - px[2]];
                            let w = [j.sol[0] - px[0], j.sol[1] - px[1], j.sol[2] - px[2]];
                            let len2 = e[0] * e[0] + e[1] * e[1] + e[2] * e[2];
                            let t = (w[0] * e[0] + w[1] * e[1] + w[2] * e[2]) / len2.max(1e-300);
                            let own = val_at(patch.attribution.input, patch.attribution.face)
                                .map_or("None".into(), |v| format!("{v:.3e}"));
                            eprintln!(
                                "[451-hosts] #{k} junc{ji} faces=({},{}) comp=({:?}:{}) \
                                 cand=({x},{y}) d={d:.4e} t={t:+.3} own={own} \
                                 contract={contract:.3e} admit={admit}",
                                j.faces.0,
                                j.faces.1,
                                patch.attribution.input,
                                patch.attribution.face,
                            );
                        }
                    }
                    if admit {
                        out.push((ji, (x, y)));
                    }
                }
            }
        }
        out
    };
    let mut comp_map: std::collections::BTreeMap<u32, s4c::ComponentInput> = Default::default();
    let mut host_map: std::collections::BTreeMap<(usize, u32), Vec<s4c::HostEdge>> =
        Default::default();
    for (k, c) in corridors.iter().enumerate() {
        // inc-2c-3b-7 (§3n): the affected set includes every patch a
        // PHANTOM lives on — the phantom must vanish everywhere (the I13
        // interference-group lesson; R0044's crossed corner q=v513 puts
        // the twin phantoms on the prism base B:0, which the key formula
        // alone never names). Planning there uses the same generators.
        let mut keys = s4c::affected_keys(c);
        for &ph in &c.phantoms {
            keys.extend(patches_of(ph).iter().copied());
        }
        keys.sort_unstable();
        keys.dedup();
        for key in keys {
            for (pi, patch) in raw_patches
                .iter()
                .enumerate()
                .filter(|(_, p)| (p.attribution.input, p.attribution.face) == key)
            {
                let holds_phantom = patch.tri_indices.iter().any(|&t| {
                    let tri = mesh.tris[t as usize];
                    c.phantoms.iter().any(|&p| tri.contains(&p))
                });
                let Some(cycles_v) = cycles_of(pi) else {
                    refuse("component cycle walk failed");
                    return Ok(false);
                };
                let hosts = hosts_on(k, patch, &cycles_v);
                if std::env::var("YANG_451_HOSTS").as_deref() == Ok("census") {
                    eprintln!(
                        "[451-comp] #{k} comp={pi} key={key:?} phantom={holds_phantom} \
                         hosts={hosts:?} cycles={:?}",
                        cycles_v.iter().map(|c| c.len()).collect::<Vec<_>>(),
                    );
                    for &p in &c.phantoms {
                        for cy in &cycles_v {
                            if let Some(i) = cy.iter().position(|&v| v == p) {
                                let n = cy.len();
                                let win: Vec<String> = (0..13)
                                    .map(|o| {
                                        let v = cy[(i + n + o - 6) % n];
                                        let f = faces_of(c.far.0)
                                            .get(c.far.1 as usize)
                                            .and_then(|fc| {
                                                surface_value_and_normal(
                                                    fc.surface,
                                                    mesh.verts[v as usize].as_array(),
                                                )
                                            })
                                            .map_or("?".into(), |(x, _)| format!("{x:.2e}"));
                                        let tag = if v == p { "[P]" } else { "" };
                                        format!("v{v}{tag}(far={f})")
                                    })
                                    .collect();
                                eprintln!("[451-win] #{k} comp={pi} {}", win.join(" "));
                            }
                        }
                    }
                }
                if holds_phantom || !hosts.is_empty() {
                    comp_map.entry(pi as u32).or_insert(s4c::ComponentInput {
                        key,
                        comp: pi as u32,
                        cycles: cycles_v,
                    });
                    host_map.insert((k, pi as u32), hosts);
                }
            }
        }
    }
    let comp_vec: Vec<s4c::ComponentInput> = comp_map.into_values().collect();
    let far_value = |k: usize, v: u32| -> Option<f64> {
        let (fi, ff) = corridors[k].far;
        let s = faces_of(fi).get(ff as usize)?.surface;
        surface_value_and_normal(s, mesh.verts[v as usize].as_array()).map(|(f, _)| f)
    };
    let attachments = |k: usize, p: u32| -> Vec<(u32, usize)> {
        let c = &corridors[k];
        let mut out = Vec::new();
        for &w in adj.get(&p).into_iter().flatten() {
            if !vertex_on_curve(&patch_map, w) {
                continue;
            }
            let pw = patches_of(w);
            let hits: Vec<usize> = c
                .junctions
                .iter()
                .enumerate()
                .filter(|(_, j)| {
                    pw.contains(&(c.walk_op, j.faces.0)) || pw.contains(&(c.walk_op, j.faces.1))
                })
                .map(|(i, _)| i)
                .collect();
            // inc-2c-3b-9b: an extended corridor's junctions can SHARE a
            // face (the extension crosses further creases of the same
            // band), so face-hits alone stop being unique — the neighbour
            // attaches to the NEAREST face-matching junction (its curve
            // continues from exactly one of them; a wrong pick fails the
            // generators' orientation-unique checks loudly).
            match hits.as_slice() {
                [one] => out.push((w, *one)),
                [] => {}
                many => {
                    let pwv = mesh.verts[w as usize].as_array();
                    if let Some(&best) = many.iter().min_by(|&&x, &&y| {
                        d3(c.junctions[x].sol, pwv).total_cmp(&d3(c.junctions[y].sol, pwv))
                    }) {
                        out.push((w, best));
                    }
                }
            }
        }
        out
    };
    let hosts = |k: usize, comp: u32| -> Vec<s4c::HostEdge> {
        host_map.get(&(k, comp)).cloned().unwrap_or_default()
    };
    let vpos = |v: u32| -> Option<[f64; 3]> { mesh.verts.get(v as usize).map(|p| p.as_array()) };
    // inc-2c-3b-9b (spec §3q): the STANDING-JUNCTION certificate — the
    // vertex carries exactly two walk-op faces and sits ON the {far, f1,
    // f2} triple solution within the CONTRACT band. Fig-11(b) made exact:
    // only what is NOT standing may merge into a corridor junction.
    let standing = |k: usize, v: u32| -> bool {
        let c = &corridors[k];
        standing_at(c.far, c.walk_op, v)
    };
    let pctx = s4c::PlanCtx {
        far_value: &far_value,
        band: &vband,
        pos: &vpos,
        attachments: &attachments,
        hosts: &hosts,
        standing: &standing,
    };
    let mut pool = s4c::MintPool::default();
    if std::env::var("YANG_451_HOSTS").as_deref() == Ok("census") {
        for (k, c) in corridors.iter().enumerate() {
            for &q in &c.corners {
                eprintln!(
                    "[451-corner] #{k} q=v{q} far={:?} band={:.3e}",
                    far_value(k, q),
                    vband(q),
                );
            }
            for &p in &c.phantoms {
                eprintln!(
                    "[451-att] #{k} phantom=v{p} attachments={:?} juncs={:?}",
                    attachments(k, p),
                    c.junctions
                        .iter()
                        .map(|j| (j.faces, format!("{:?}", j.disposition)))
                        .collect::<Vec<_>>(),
                );
            }
            // inc-2c-3b-8 census: the crossed corner's THIRD faces (its
            // memberships beyond far and the walk's junction faces) — does
            // a true junction {far, third, junction-face} exist near the
            // corner (the base-boundary re-closure candidate)? Part 2 (the
            // cycle shapes): B-Rep rim roots on the third + junction faces
            // (the definitive crossing census with in-domain flags), host
            // admission of every candidate solution against the third /
            // junction / far components' cycles, the third face's cycle
            // windows near q, the leg run's sourceability, and each
            // phantom's distance to its candidate true home.
            for &q in &c.corners {
                let pq = patches_of(q);
                let jfaces: std::collections::BTreeSet<(InputId, u32)> = c
                    .junctions
                    .iter()
                    .flat_map(|j| [(c.walk_op, j.faces.0), (c.walk_op, j.faces.1)])
                    .collect();
                let far_s = faces_of(c.far.0).get(c.far.1 as usize).map(|f| f.surface);
                let qpos = mesh.verts[q as usize];
                let d3p = |a: Point3, b: [f64; 3]| -> f64 {
                    ((a.x() - b[0]).powi(2) + (a.y() - b[1]).powi(2) + (a.z() - b[2]).powi(2))
                        .sqrt()
                };
                type ThirdSol = ((InputId, u32), (InputId, u32), [f64; 3]);
                let mut thirds: std::collections::BTreeSet<(InputId, u32)> = Default::default();
                let mut sols: Vec<ThirdSol> = Vec::new();
                for &t in pq
                    .iter()
                    .filter(|kk| **kk != c.far && !jfaces.contains(*kk))
                {
                    thirds.insert(t);
                    let ts = faces_of(t.0).get(t.1 as usize).map(|f| f.surface);
                    for &(fi, ff) in &jfaces {
                        let fs = faces_of(fi).get(ff as usize).map(|f| f.surface);
                        if let (Some(a0), Some(a1), Some(a2)) = (far_s, ts, fs) {
                            let sol = crate::stage4_relocate::relocate_onto_implicit_triple(
                                qpos, a0, a1, a2,
                            );
                            match sol {
                                Some(x) => {
                                    eprintln!(
                                        "[451-base] #{k} q=v{q} third={t:?} jf=({fi:?},{ff}) \
                                         sol=({:.3},{:.3},{:.3}) d_q={:.4e}",
                                        x.x(),
                                        x.y(),
                                        x.z(),
                                        d3p(x, qpos.as_array()),
                                    );
                                    sols.push((t, (fi, ff), x.as_array()));
                                }
                                None => eprintln!(
                                    "[451-base] #{k} q=v{q} third={t:?} \
                                     jf=({fi:?},{ff}) NO-CONVERGE"
                                ),
                            }
                        }
                    }
                }
                // (a) B-Rep rim roots: every far∩edge root on the third and
                // junction faces, with in-domain flags — the crossing
                // parity census. Root-free edges are elided.
                if let Some(fs) = far_s {
                    for &(ti, tf) in thirds.iter().chain(jfaces.iter()) {
                        let brep_t = match ti {
                            InputId::A => a,
                            InputId::B => b,
                        };
                        for l in s4t::face_edge_roots_probe(brep_t, fs, tf, qpos.as_array()) {
                            if !l.contains("roots=0") {
                                eprintln!("[451-bleg-rim] #{k} q=v{q} face=({ti:?},{tf}) {l}");
                            }
                        }
                    }
                }
                let maxdq = sols
                    .iter()
                    .map(|&(_, _, s)| d3p(qpos, s))
                    .fold(0.0f64, f64::max);
                // (a2) Face-loop edge inventory near q: every loop edge of
                // the junction faces (their loops are small), and the third
                // faces' edges with an endpoint within 8× the farthest
                // candidate d_q — id, curve kind, endpoint distances.
                for &(ti, tf) in thirds.iter().chain(jfaces.iter()) {
                    let brep_t = match ti {
                        InputId::A => a,
                        InputId::B => b,
                    };
                    let Some(f) = brep_t.faces().get(tf as usize) else {
                        continue;
                    };
                    for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
                        let e = &brep_t.edges()[ei as usize];
                        let ps = brep_t.vertices()[e.start as usize].point.as_array();
                        let pe = brep_t.vertices()[e.end as usize].point.as_array();
                        let (dqs, dqe) = (d3p(qpos, ps), d3p(qpos, pe));
                        if !jfaces.contains(&(ti, tf)) && dqs.min(dqe) > 8.0 * maxdq {
                            continue;
                        }
                        let kind = match e.curve {
                            Curve::LineSegment => "Line",
                            Curve::Circle { .. } => "Circle",
                            _ => "Other",
                        };
                        eprintln!(
                            "[451-bleg-edge] #{k} face=({ti:?},{tf}) edge={ei} {kind} \
                             vs={}@dq={dqs:.1} ve={}@dq={dqe:.1}",
                            e.start, e.end
                        );
                    }
                }
                // (f) Mesh vertices near the corner (within 2× the farthest
                // candidate d_q): id, d_q, far value, patch memberships.
                for (&v, ps) in patch_map.iter() {
                    let vp = mesh.verts[v as usize].as_array();
                    let dq = d3p(qpos, vp);
                    if dq <= 2.0 * maxdq {
                        let f = far_value(k, v).map_or("?".into(), |x| format!("{x:.2e}"));
                        eprintln!("[451-bleg-v] #{k} v{v} d_q={dq:.1} far={f} memb={ps:?}");
                    }
                }
                // (g) The phantom/corner star: every triangle touching a
                // fired traveller or the corner, with its attribution.
                for (ti2, tri) in mesh.tris.iter().enumerate() {
                    if tri
                        .iter()
                        .any(|v| c.phantoms.contains(v) || c.corners.contains(v))
                    {
                        let att = attribution
                            .attributions
                            .get(ti2)
                            .and_then(|x| x.as_ref())
                            .map(|x| (x.input, x.face));
                        eprintln!("[451-bleg-tri] #{k} t{ti2} {tri:?} att={att:?}");
                    }
                }
                // (b) Host admission of each candidate solution against the
                // cycles of every component keyed by its third face, its
                // junction face, or the far patch — which chords would host
                // the leg's junctions.
                for &(t, jf, sol) in &sols {
                    let scale = sol.iter().fold(0.0f64, |m, &x| m.max(x.abs()));
                    let eval = cad_primitives::TAU_WORK.max(8.0 * f64::EPSILON * scale);
                    let contract = s4t::contract_band(scale);
                    let val_at = |inp: InputId, f: u32| -> Option<f64> {
                        let s = faces_of(inp).get(f as usize)?.surface;
                        surface_value_and_normal(s, sol).map(|(v, _)| v)
                    };
                    for key in [t, jf, c.far] {
                        for (pi, patch) in raw_patches
                            .iter()
                            .enumerate()
                            .filter(|(_, p)| (p.attribution.input, p.attribution.face) == key)
                        {
                            let Some(cycles_v) = cycles_of(pi) else {
                                continue;
                            };
                            let on_curve = |x: u32, y: u32| -> bool {
                                val_at(patch.attribution.input, patch.attribution.face)
                                    .is_some_and(|v| v.abs() <= contract)
                                    && mesh.tris.iter().enumerate().any(|(ti2, tri)| {
                                        tri.contains(&x)
                                            && tri.contains(&y)
                                            && !patch.tri_indices.contains(&(ti2 as u32))
                                            && attribution
                                                .attributions
                                                .get(ti2)
                                                .and_then(|a| a.as_ref())
                                                .and_then(|att| val_at(att.input, att.face))
                                                .is_some_and(|v| v.abs() <= contract)
                                    })
                            };
                            for cy in &cycles_v {
                                let n = cy.len();
                                for i in 0..n {
                                    let (x, y) = (cy[i], cy[(i + 1) % n]);
                                    let (px, py) = (
                                        mesh.verts[x as usize].as_array(),
                                        mesh.verts[y as usize].as_array(),
                                    );
                                    let d = s4t::dist_point_segment(sol, px, py);
                                    if d <= eval.max(d_eps) {
                                        let e = [py[0] - px[0], py[1] - px[1], py[2] - px[2]];
                                        let w = [sol[0] - px[0], sol[1] - px[1], sol[2] - px[2]];
                                        let l2 = e[0] * e[0] + e[1] * e[1] + e[2] * e[2];
                                        let tp = (w[0] * e[0] + w[1] * e[1] + w[2] * e[2])
                                            / l2.max(1e-300);
                                        let admit =
                                            s4t::arc_host_admit(sol, px, py, eval, d_eps, || {
                                                on_curve(x, y)
                                            });
                                        eprintln!(
                                            "[451-bleg-host] #{k} jf={jf:?} comp={pi} \
                                             key={key:?} cand=({x},{y}) d={d:.4e} t={tp:+.3} \
                                             admit={admit}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                // (c) The third-face components' cycle windows near q — the
                // surgery target shapes (contiguous runs within 3× the
                // farthest candidate's d_q).
                let rmax = sols
                    .iter()
                    .map(|&(_, _, s)| d3p(qpos, s))
                    .fold(0.0f64, f64::max)
                    * 3.0;
                for &t in &thirds {
                    for (pi, _) in raw_patches
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| (p.attribution.input, p.attribution.face) == t)
                    {
                        let Some(cycles_v) = cycles_of(pi) else {
                            continue;
                        };
                        for (ci, cy) in cycles_v.iter().enumerate() {
                            let n = cy.len();
                            let near: Vec<bool> = cy
                                .iter()
                                .map(|&v| d3p(qpos, mesh.verts[v as usize].as_array()) <= rmax)
                                .collect();
                            if !near.iter().any(|&x| x) {
                                continue;
                            }
                            let mut i = 0usize;
                            while i < n {
                                if !near[i] || (i > 0 && near[i - 1]) {
                                    i += 1;
                                    continue;
                                }
                                let mut jj = i;
                                while jj + 1 < n && near[jj + 1] {
                                    jj += 1;
                                }
                                let row: Vec<String> = (i..=jj)
                                    .map(|ix| {
                                        let v = cy[ix];
                                        let f = far_value(k, v)
                                            .map_or("?".into(), |x| format!("{x:.2e}"));
                                        format!(
                                            "v{v}(far={f},d_q={:.1})",
                                            d3p(qpos, mesh.verts[v as usize].as_array())
                                        )
                                    })
                                    .collect();
                                eprintln!(
                                    "[451-bleg-cyc] #{k} comp={pi} key={t:?} cyc={ci} \
                                     idx={i}..{jj}/{n}: {}",
                                    row.join(" ")
                                );
                                i = jj + 1;
                            }
                        }
                    }
                }
                // (d) The leg run between candidate pairs on the same third
                // face — sourceability at the chord band.
                for i in 0..sols.len() {
                    for j2 in (i + 1)..sols.len() {
                        let (t1, jf1, s1) = sols[i];
                        let (t2, jf2, s2) = sols[j2];
                        if t1 != t2 || jf1 == jf2 {
                            continue;
                        }
                        let third_s = faces_of(t1.0).get(t1.1 as usize).map(|f| f.surface);
                        if let (Some(fs), Some(ts2)) = (far_s, third_s) {
                            let run = s4t::sample_run_chord(fs, ts2, s1, s2, d_eps);
                            eprintln!(
                                "[451-bleg-run] #{k} third={t1:?} {jf1:?}->{jf2:?} \
                                 chord={:.3e} run={:?}",
                                {
                                    let dd = [s2[0] - s1[0], s2[1] - s1[1], s2[2] - s1[2]];
                                    (dd[0] * dd[0] + dd[1] * dd[1] + dd[2] * dd[2]).sqrt()
                                },
                                run.map(|v| v.len())
                            );
                        }
                    }
                }
                // (e) Each phantom's distance to q and to every candidate —
                // which solution is each glided traveller's true home.
                for &p in &c.phantoms {
                    let pp = mesh.verts[p as usize].as_array();
                    let ds: Vec<String> = sols
                        .iter()
                        .map(|&(_, jf, s)| {
                            let dd = [s[0] - pp[0], s[1] - pp[1], s[2] - pp[2]];
                            format!(
                                "{jf:?}:{:.3e}",
                                (dd[0] * dd[0] + dd[1] * dd[1] + dd[2] * dd[2]).sqrt()
                            )
                        })
                        .collect();
                    eprintln!(
                        "[451-bleg-ph] #{k} v{p} d_q={:.3e} {}",
                        d3p(qpos, pp),
                        ds.join(" ")
                    );
                }
            }
        }
    }
    let (plans, pdeclines) = s4c::plan_invocation(&corridors, &comp_vec, &pctx, &mut pool);
    if !pdeclines.is_empty() {
        refuse(&format!("plan declines: {pdeclines:?}"));
        return Ok(false);
    }
    // Every affected patch key of every corridor must be covered by a plan.
    for c in &corridors {
        for key in s4c::affected_keys(c) {
            if !plans.iter().any(|pl| pl.key == key) {
                refuse(&format!("affected patch {key:?} has no plan"));
                return Ok(false);
            }
        }
    }
    if std::env::var("YANG_451_HOSTS").as_deref() == Ok("census") {
        // §3r follow-up census: the far plans' corrected cycles WITH
        // positions — the output-boundary provenance for the FaceId ring
        // walls (compare against KV2_RING_REJECT_PROBE dumps).
        for pl in &plans {
            if !corridors.iter().any(|c| c.far == pl.key) {
                continue;
            }
            for (ci, cy) in pl.corrected.iter().enumerate() {
                let row: Vec<String> = cy
                    .iter()
                    .map(|r| match *r {
                        s4c::CycleRef::Old(v) => {
                            let p = mesh.verts[v as usize].as_array();
                            format!("v{v}@({:.1},{:.1},{:.1})", p[0], p[1], p[2])
                        }
                        s4c::CycleRef::New(i) => {
                            let p = pool.verts[i as usize];
                            format!("N{i}@({:.1},{:.1},{:.1})", p[0], p[1], p[2])
                        }
                    })
                    .collect();
                eprintln!(
                    "[451-farcyc] key={:?} comp={} cyc={ci}: {}",
                    pl.key,
                    pl.comp,
                    row.join(" ")
                );
            }
        }
    }
    // ---- MUTATION (all-or-nothing) ------------------------------------
    // The batch-wide removed union (every plan's removed set): the refill
    // orientation testimony must exclude any triangle touching condemned
    // territory (spec §3q).
    let removed_union: std::collections::BTreeSet<u32> = plans
        .iter()
        .flat_map(|pl| pl.removed.iter().copied())
        .collect();
    if std::env::var_os("YANG_451_CHI").is_some() {
        let mut vs: std::collections::BTreeSet<u32> = Default::default();
        let mut es: std::collections::BTreeSet<(u32, u32)> = Default::default();
        for t in &mesh.tris {
            for k in 0..3 {
                vs.insert(t[k]);
                let (x, y) = (t[k], t[(k + 1) % 3]);
                es.insert((x.min(y), x.max(y)));
            }
        }
        eprintln!(
            "[451-transit] CHI pre-apply V={} E={} F={} chi={}",
            vs.len(),
            es.len(),
            mesh.tris.len(),
            vs.len() as i64 - es.len() as i64 + mesh.tris.len() as i64
        );
    }
    let base = mesh.verts.len() as u32;
    mesh.verts
        .extend(pool.verts.iter().map(|p| Point3::new(p[0], p[1], p[2])));
    let rollback = |mesh: &mut Mesh| {
        mesh.verts.truncate(base as usize);
    };
    let mut rebuilds: Vec<crate::stage4_construct::PatchRebuild> = Vec::new();
    let mut replaced: std::collections::BTreeSet<u32> = Default::default();
    let mut removed_all: std::collections::BTreeSet<u32> = Default::default();
    // FAR patches get FAN-LOCAL surgery (delete the phantom's fan, refill
    // the link + corridor-chain polygon in a local chart window) — the
    // measured I6/I13 lesson: a wholesale re-CDT of a big curved lateral
    // fails on pre-existing boundary folds elsewhere on the patch and
    // certifies coarser chords (measured live: R0011's far is a
    // radius-6277 cylinder — TriangulationFailed / ChordDegradation).
    // The generator-A ComponentPlan remains the coherence certificate;
    // the mutation implements the same swap locally.
    let far_keys: std::collections::BTreeSet<(InputId, u32)> =
        corridors.iter().map(|c| c.far).collect();
    // inc-2c-3b-6 (spec §3m): the far arm consumes phantoms GLOBALLY — a
    // JOINT region (adjacent corridors' overshoot fans meeting on one far
    // patch, the measured R0044 v76+v75 anatomy) is deleted once, and its
    // link RUNS are stitched with the corrected cycle's all-New arcs into
    // one refill polygon. The single-corridor case is the one-run instance
    // of the same rule (link + one all-New arc), replacing the former
    // per-corridor path/junction-attachment construction.
    let census = std::env::var("YANG_451_HOSTS").as_deref() == Ok("census");
    let mut far_done: std::collections::BTreeSet<(usize, u32)> = Default::default();
    for c in corridors.iter() {
        for &p in &c.phantoms {
            let matching_far: Vec<(usize, &crate::stage5_topology::Patch)> = raw_patches
                .iter()
                .enumerate()
                .filter(|(_, pt)| {
                    (pt.attribution.input, pt.attribution.face) == c.far
                        && pt
                            .tri_indices
                            .iter()
                            .any(|&t| mesh.tris[t as usize].contains(&p))
                })
                .collect();
            if matching_far.is_empty() {
                refuse(&format!("far fan of v{p} not found"));
                rollback(mesh);
                return Ok(false);
            }
            for (pi, patch) in matching_far {
                if far_done.contains(&(pi, p)) {
                    continue;
                }
                let Some(surface) = faces_of(c.far.0).get(c.far.1 as usize).map(|f| f.surface)
                else {
                    refuse("far surface missing");
                    rollback(mesh);
                    return Ok(false);
                };
                // §3j: victims = the far plan's removed set reachable from the
                // phantom through removed vertices — the PLANNER is the single
                // absorb authority; the fan arm executes its decision.
                let Some(far_plan) = plans
                    .iter()
                    .find(|pl| pl.key == c.far && pl.comp == pi as u32)
                else {
                    refuse(&format!("far component {pi} of v{p} has no plan"));
                    rollback(mesh);
                    return Ok(false);
                };
                let removed_set: std::collections::BTreeSet<u32> =
                    far_plan.removed.iter().copied().collect();
                let mut victims = std::collections::BTreeSet::from([p]);
                let mut stack = vec![p];
                while let Some(x) = stack.pop() {
                    for &y in adj.get(&x).into_iter().flatten() {
                        if removed_set.contains(&y) && victims.insert(y) {
                            stack.push(y);
                        }
                    }
                }
                // Joint admission: every fired vertex the flood reaches must be
                // a FAR-MATE (its corridor shares this far patch) — the joint
                // region then deletes once. Anything else is a genuine
                // cross-far entanglement, refused.
                for &v in &victims {
                    if fired.contains(&v)
                        && !corridors
                            .iter()
                            .any(|c2| c2.far == c.far && c2.phantoms.contains(&v))
                    {
                        refuse(&format!(
                            "far fan of v{p} floods into a foreign-far phantom v{v}"
                        ));
                        rollback(mesh);
                        return Ok(false);
                    }
                }
                far_done.extend(
                    victims
                        .iter()
                        .copied()
                        .filter(|v| fired.contains(v))
                        .map(|v| (pi, v)),
                );
                let sp = crate::stage4_splice::SplicePatch {
                    cycles: Vec::new(),
                    tris: patch.tri_indices.clone(),
                    surface,
                };
                let (del, runs, _fan) = match crate::stage4_construct::delete_boundary_fan_runs(
                    mesh, pi, &sp, &victims,
                ) {
                    Ok(r) => r,
                    Err(e) => {
                        refuse(&format!("far fan delete v{p} ({victims:?}): {e:?}"));
                        rollback(mesh);
                        return Ok(false);
                    }
                };
                if census {
                    eprintln!("[451-joint] p=v{p} comp={pi} victims={victims:?} runs={runs:?}");
                }
                // Stitch: the refill polygon alternates link runs with
                // corrected-cycle arcs whose INTERIOR is all-New (the minted
                // path replacing each boundary gap). Every choice must be
                // unique; every failure is typed (`stitch_fan_polygon`).
                let polygon = match stitch_fan_polygon(&far_plan.corrected, &runs, base) {
                    Ok(pg) => pg,
                    Err(why) => {
                        refuse(&format!("far stitch of v{p}: {why}"));
                        rollback(mesh);
                        return Ok(false);
                    }
                };
                // Seed budget = the fossil's own vertex spend (its victims):
                // like-for-like in density as well as d(T) (§3m).
                let seed_base = mesh.verts.len() as u32;
                let (new_tris, seed_pts) = match crate::stage4_construct::refill_fan_hole_seeded(
                    mesh,
                    pi,
                    &sp,
                    &polygon,
                    &del.old_tris,
                    victims.len(),
                    seed_base,
                    &removed_union,
                ) {
                    Ok(t) => t,
                    Err(e) => {
                        refuse(&format!("far fan refill v{p}: {e:?}"));
                        rollback(mesh);
                        return Ok(false);
                    }
                };
                if census && !seed_pts.is_empty() {
                    eprintln!(
                        "[451-joint] p=v{p} refill seeded {} interior verts",
                        seed_pts.len()
                    );
                }
                // Seeds ride the batch (`new_verts` + the I2e remap): an eager
                // append here would move the vertex baseline mid-mutation and
                // stale every other rebuild's plan stamp (measured: R0044's
                // first live seed → batch StalePlan, spec §3p).
                replaced.extend(del.old_tris.iter().copied());
                removed_all.extend(victims.iter().copied());
                rebuilds.push(crate::stage4_construct::PatchRebuild {
                    patch: pi,
                    old_tris: del.old_tris,
                    new_tris,
                    new_verts: seed_pts,
                    dropped: victims.clone(),
                    plan_verts: seed_base,
                    plan_tris: mesh.tris.len() as u32,
                });
            }
        }
    }
    // Non-far components: wholesale rebuild from the corrected cycles.
    for pl in &plans {
        if far_keys.contains(&pl.key) {
            removed_all.extend(pl.removed.iter().copied());
            continue;
        }
        let cycles_u: Vec<Vec<u32>> = pl
            .corrected
            .iter()
            .map(|cy| {
                cy.iter()
                    .map(|r| match *r {
                        s4c::CycleRef::Old(v) => v,
                        s4c::CycleRef::New(i) => base + i,
                    })
                    .collect()
            })
            .collect();
        let patch = &raw_patches[pl.comp as usize];
        let Some(surface) = faces_of(patch.attribution.input)
            .get(patch.attribution.face as usize)
            .map(|f| f.surface)
        else {
            refuse("component surface missing");
            rollback(mesh);
            return Ok(false);
        };
        let sp = crate::stage4_splice::SplicePatch {
            cycles: cycles_u,
            tris: patch.tri_indices.clone(),
            surface,
        };
        match crate::stage4_construct::rebuild_patch_planar(mesh, pl.comp as usize, &sp) {
            Ok(r) => {
                replaced.extend(r.old_tris.iter().copied());
                rebuilds.push(r);
            }
            Err(
                crate::stage4_construct::ConstructError::ThetaUnwrap { .. }
                | crate::stage4_construct::ConstructError::ChordDegradation { .. }
                | crate::stage4_construct::ConstructError::Cdt { .. },
            ) => {
                // inc-2c-3b-6 (§3m): the WHOLESALE tool refuses this band —
                // either the boundary winds a whole period (ThetaUnwrap —
                // no disc chart exists; R0044's B:154 cone annulus) or the
                // whole-patch re-CDT certifies coarser than the fossil
                // (ChordDegradation — B:155's near-wrap band blew up 61×)
                // or the projected boundary self-crosses under the disc
                // chart (Cdt — B:359, the I6/I13 fold anatomy).
                // Both are the wholesale tool's own typed capability
                // verdicts, and the transit edit is LOCAL, so the far
                // arm's region machinery applies verbatim: per connected
                // removed region, delete the fan, stitch link runs with
                // the corrected all-New arcs, refill seeded. Every failure
                // stays typed; nothing is accepted that the local tool's
                // certificates do not certify.
                let removed_here: std::collections::BTreeSet<u32> =
                    pl.removed.iter().copied().collect();
                let mut remaining = removed_here.clone();
                while let Some(&seed0) = remaining.iter().next() {
                    let mut victims = std::collections::BTreeSet::from([seed0]);
                    let mut stack = vec![seed0];
                    while let Some(x) = stack.pop() {
                        for &y in adj.get(&x).into_iter().flatten() {
                            if removed_here.contains(&y) && victims.insert(y) {
                                stack.push(y);
                            }
                        }
                    }
                    remaining.retain(|v| !victims.contains(v));
                    let (del, runs, _fan) = match crate::stage4_construct::delete_boundary_fan_runs(
                        mesh,
                        pl.comp as usize,
                        &sp,
                        &victims,
                    ) {
                        Ok(r) => r,
                        Err(e2) => {
                            refuse(&format!(
                                "wrap-band fan delete comp={} ({victims:?}): {e2:?}",
                                pl.comp
                            ));
                            rollback(mesh);
                            return Ok(false);
                        }
                    };
                    let polygon = match stitch_fan_polygon(&pl.corrected, &runs, base) {
                        Ok(pg) => pg,
                        Err(why) => {
                            refuse(&format!("wrap-band stitch comp={}: {why}", pl.comp));
                            rollback(mesh);
                            return Ok(false);
                        }
                    };
                    let seed_base = mesh.verts.len() as u32;
                    let (new_tris, seed_pts) = match crate::stage4_construct::refill_fan_hole_seeded(
                        mesh,
                        pl.comp as usize,
                        &sp,
                        &polygon,
                        &del.old_tris,
                        victims.len(),
                        seed_base,
                        &removed_union,
                    ) {
                        Ok(t) => t,
                        Err(e2) => {
                            refuse(&format!("wrap-band refill comp={}: {e2:?}", pl.comp));
                            rollback(mesh);
                            return Ok(false);
                        }
                    };
                    replaced.extend(del.old_tris.iter().copied());
                    rebuilds.push(crate::stage4_construct::PatchRebuild {
                        patch: pl.comp as usize,
                        old_tris: del.old_tris,
                        new_tris,
                        new_verts: seed_pts,
                        dropped: victims.clone(),
                        plan_verts: seed_base,
                        plan_tris: mesh.tris.len() as u32,
                    });
                }
            }
            Err(e) => {
                if census {
                    if let Some(ch) = crate::stage4_project::SurfaceChart::new(surface) {
                        for cy in &sp.cycles {
                            let th: Vec<String> = cy
                                .iter()
                                .map(|&v| {
                                    let uv = ch.project(mesh.verts[v as usize]);
                                    format!("v{v}:{:.3}/{:.2}", uv.x(), uv.y())
                                })
                                .collect();
                            eprintln!("[451-rebuild] comp={} cycle: {}", pl.comp, th.join(" "));
                        }
                    }
                }
                refuse(&format!(
                    "rebuild comp={} key={:?} {e:?} surface={surface:?} cycles={:?}",
                    pl.comp,
                    pl.key,
                    pl.corrected.iter().map(|c| c.len()).collect::<Vec<_>>(),
                ));
                rollback(mesh);
                return Ok(false);
            }
        }
        removed_all.extend(pl.removed.iter().copied());
    }
    // inc-2c-3b-7 (§3n) — the REMOVED-MEMBERSHIP closure sweep: a comp
    // that holds removed vertices but was neither planned nor far-processed
    // (the operand-neighbour face, the incursion strips) gets the same
    // fan-local surgery with a SYNTHESIZED correction — its own boundary
    // cycle minus the removed run (the stitch arcs are empty: the removed
    // verts were certified doubled-back/past-the-junction by the planning
    // comps, so the surviving neighbours reconnect directly, to chord
    // accuracy — the absorb splice one vertex earlier, seen from the other
    // patch). Conformality stays adjudicated downstream (the §4-I9 re-run,
    // stage-5/6 manifoldness, the standing oracle gate) — loud, never
    // silent, and the whole apply is gated.
    let handled: std::collections::BTreeSet<usize> = rebuilds.iter().map(|r| r.patch).collect();
    for (pi, patch) in raw_patches.iter().enumerate() {
        if handled.contains(&pi) {
            continue;
        }
        let holds: std::collections::BTreeSet<u32> = patch
            .tri_indices
            .iter()
            .flat_map(|&t| mesh.tris[t as usize])
            .filter(|v| removed_all.contains(v))
            .collect();
        if holds.is_empty() {
            continue;
        }
        // inc-2c-3b-8 (spec §3p) — a component whose EVERY vertex is
        // removed-certified rebuilds EMPTY as one unit: the wholly-condemned
        // third-face pocket (R0044's B:0 corner sliver [v144 v141 v142 q] —
        // the far body swallows the whole pocket; each vertex is removed by
        // another component's certified plan, and the planner left the comp
        // unplanned because it keeps nothing). A partially-condemned comp
        // falls through to the fan-local path below, whose refusals stay
        // typed.
        let all_verts: std::collections::BTreeSet<u32> = patch
            .tri_indices
            .iter()
            .flat_map(|&t| mesh.tris[t as usize])
            .collect();
        if all_verts.iter().all(|v| removed_all.contains(v)) {
            if census {
                eprintln!(
                    "[451-sweep] comp={pi} key={:?} WHOLE-COMPONENT excision: verts={all_verts:?} \
                     tris={}",
                    (patch.attribution.input, patch.attribution.face),
                    patch.tri_indices.len(),
                );
            }
            replaced.extend(patch.tri_indices.iter().copied());
            rebuilds.push(crate::stage4_construct::PatchRebuild {
                patch: pi,
                old_tris: patch.tri_indices.clone(),
                new_tris: Vec::new(),
                new_verts: Vec::new(),
                dropped: all_verts,
                plan_verts: mesh.verts.len() as u32,
                plan_tris: mesh.tris.len() as u32,
            });
            continue;
        }
        let surface = match faces_of(patch.attribution.input)
            .get(patch.attribution.face as usize)
            .map(|f| f.surface)
        {
            Some(sf) => sf,
            None => {
                refuse("closure-sweep surface missing");
                rollback(mesh);
                return Ok(false);
            }
        };
        let synth: Vec<Vec<s4c::CycleRef>> =
            match crate::stage5_topology::patch_boundary_cycle(patch, mesh) {
                Ok(cys) => cys
                    .iter()
                    .map(|cy| {
                        cy.iter()
                            .map(|&(v, _)| v)
                            .filter(|v| !removed_all.contains(v))
                            .map(s4c::CycleRef::Old)
                            .collect()
                    })
                    .collect(),
                Err(_) => {
                    refuse(&format!("closure-sweep cycle walk failed comp={pi}"));
                    rollback(mesh);
                    return Ok(false);
                }
            };
        let spd = crate::stage4_splice::SplicePatch {
            cycles: Vec::new(),
            tris: patch.tri_indices.clone(),
            surface,
        };
        let mut remaining = holds.clone();
        while let Some(&seed0) = remaining.iter().next() {
            let mut victims = std::collections::BTreeSet::from([seed0]);
            let mut stack = vec![seed0];
            while let Some(x) = stack.pop() {
                for &y in adj.get(&x).into_iter().flatten() {
                    if holds.contains(&y) && victims.insert(y) {
                        stack.push(y);
                    }
                }
            }
            remaining.retain(|v| !victims.contains(v));
            let (del, runs, _fan) =
                match crate::stage4_construct::delete_boundary_fan_runs(mesh, pi, &spd, &victims) {
                    Ok(r) => r,
                    Err(e2) => {
                        refuse(&format!(
                            "closure-sweep fan delete comp={pi} ({victims:?}): {e2:?}"
                        ));
                        rollback(mesh);
                        return Ok(false);
                    }
                };
            let polygon = match stitch_fan_polygon(&synth, &runs, base) {
                Ok(pg) => pg,
                Err(why) => {
                    refuse(&format!("closure-sweep stitch comp={pi}: {why}"));
                    rollback(mesh);
                    return Ok(false);
                }
            };
            if census {
                let key = (patch.attribution.input, patch.attribution.face);
                let old: Vec<[u32; 3]> = del
                    .old_tris
                    .iter()
                    .map(|&t| mesh.tris[t as usize])
                    .collect();
                // Each victim's distance to the direct-reconnection chord —
                // the sliver rule's own yardstick (chord accuracy).
                let ds: Vec<String> = if let [a, b] = polygon.as_slice() {
                    let (pa, pb) = (
                        mesh.verts[*a as usize].as_array(),
                        mesh.verts[*b as usize].as_array(),
                    );
                    victims
                        .iter()
                        .map(|&v| {
                            format!(
                                "v{v}:{:.3e}",
                                crate::stage4_transit::dist_point_segment(
                                    mesh.verts[v as usize].as_array(),
                                    pa,
                                    pb,
                                )
                            )
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                eprintln!(
                    "[451-sweep] comp={pi} key={key:?} victims={victims:?} runs={runs:?} \
                     polygon={polygon:?} old={old:?} d_eps={d_eps:.3e} chord_d=[{}]",
                    ds.join(" ")
                );
            }
            if polygon.len() < 3 {
                // A 2-vertex hole rebuilds EMPTY: the surviving rim
                // collapses to one edge, and direct reconnection consumes
                // the residual fan strip between it and the condemned
                // vertices (the fan_rebuild_core sliver, A:3's v107; the
                // measured 3-triangle overshoot strip on R0044's B:371 —
                // victims v90/v91/v92, the §3m absorb chain — is the same
                // shape one fan wider, spec §3p). Every victim is already
                // in the invocation's certified-removed set by the sweep's
                // own definition; the only new obligation is that the
                // strip is SELF-CONTAINED — a deleted triangle referencing
                // a vertex that neither dies nor sits on the rim would
                // strand a survivor silently, refused.
                let rim: std::collections::BTreeSet<u32> = polygon.iter().copied().collect();
                let contained = del.old_tris.iter().all(|&t| {
                    mesh.tris[t as usize]
                        .iter()
                        .all(|v| victims.contains(v) || rim.contains(v))
                });
                if !contained {
                    refuse(&format!(
                        "closure-sweep comp={pi}: {}-gon hole over {} tris strands a \
                         survivor victims={victims:?} runs={runs:?}",
                        polygon.len(),
                        del.old_tris.len()
                    ));
                    rollback(mesh);
                    return Ok(false);
                }
                replaced.extend(del.old_tris.iter().copied());
                rebuilds.push(crate::stage4_construct::PatchRebuild {
                    patch: pi,
                    old_tris: del.old_tris,
                    new_tris: Vec::new(),
                    new_verts: Vec::new(),
                    dropped: victims.clone(),
                    plan_verts: mesh.verts.len() as u32,
                    plan_tris: mesh.tris.len() as u32,
                });
                continue;
            }
            let seed_base = mesh.verts.len() as u32;
            let (new_tris, seed_pts) = match crate::stage4_construct::refill_fan_hole_seeded(
                mesh,
                pi,
                &spd,
                &polygon,
                &del.old_tris,
                victims.len(),
                seed_base,
                &removed_union,
            ) {
                Ok(t) => t,
                Err(e2) => {
                    refuse(&format!("closure-sweep refill comp={pi}: {e2:?}"));
                    rollback(mesh);
                    return Ok(false);
                }
            };
            replaced.extend(del.old_tris.iter().copied());
            rebuilds.push(crate::stage4_construct::PatchRebuild {
                patch: pi,
                old_tris: del.old_tris,
                new_tris,
                new_verts: seed_pts,
                dropped: victims.clone(),
                plan_verts: seed_base,
                plan_tris: mesh.tris.len() as u32,
            });
        }
    }
    // The excised vertices must be referenced ONLY by replaced triangles.
    for (ti, tri) in mesh.tris.iter().enumerate() {
        if replaced.contains(&(ti as u32)) {
            continue;
        }
        if let Some(&v) = tri.iter().find(|v| removed_all.contains(v)) {
            let att = attribution
                .attributions
                .get(ti)
                .and_then(|a| a.as_ref())
                .map(|a| (a.input, a.face));
            if census {
                // Foreign-fan census (§3m): every removed vertex with a
                // surviving reference — its foreign comps and whether it
                // sits on their boundary cycles or interiors.
                for &rv in &removed_all {
                    for (pi2, pt2) in raw_patches.iter().enumerate() {
                        let holds = pt2.tri_indices.iter().any(|&t2| {
                            !replaced.contains(&t2) && mesh.tris[t2 as usize].contains(&rv)
                        });
                        if holds {
                            let on_cycle = crate::stage5_topology::patch_boundary_cycle(
                                &raw_patches[pi2],
                                mesh,
                            )
                            .ok()
                            .map(|cys| cys.iter().any(|cy| cy.iter().any(|&(s2, _)| s2 == rv)));
                            eprintln!(
                                "[451-foreign] removed v{rv} comp={pi2} \
                                 key=({:?},{}) on_cycle={on_cycle:?}",
                                pt2.attribution.input, pt2.attribution.face,
                            );
                        }
                    }
                }
            }
            refuse(&format!(
                "removed v{v} still referenced by tri {ti} {tri:?} att={att:?}"
            ));
            rollback(mesh);
            return Ok(false);
        }
    }
    if let Err(e) =
        crate::stage4_construct::apply_rebuild_batch(mesh, attribution, side_labels, &rebuilds)
    {
        refuse(&format!("batch: {e:?}"));
        rollback(mesh);
        return Ok(false);
    }
    eprintln!(
        "[451-transit] APPLIED corridors={} plans={} mints={} removed={}",
        corridors.len(),
        plans.len(),
        pool.verts.len(),
        removed_all.len(),
    );
    if census {
        // Post-batch watertightness audit: the result mesh must pair every
        // directed edge (fwd == rev). An unpaired edge here localizes a
        // repair-introduced hole at its source instead of at the stage-6
        // boundary walk.
        let mut dir: std::collections::BTreeMap<(u32, u32), i64> = Default::default();
        for t in &mesh.tris {
            for k in 0..3 {
                let (x, y) = (t[k], t[(k + 1) % 3]);
                *dir.entry((x.min(y), x.max(y))).or_default() += if x < y { 1 } else { -1 };
            }
        }
        let bad: Vec<((u32, u32), i64)> =
            dir.into_iter().filter(|&(_, n)| n != 0).take(40).collect();
        if !bad.is_empty() {
            eprintln!("[451-audit] unpaired directed edges post-batch: {bad:?}");
            // inc-2c-3b-9 census: per unpaired edge, the witness triangles
            // with their post-batch attributions — which rebuild emitted
            // each side, and who failed to match.
            for &((x, y), n) in bad.iter().take(20) {
                let wits: Vec<String> = mesh
                    .tris
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.contains(&x) && t.contains(&y))
                    .map(|(ti, t)| {
                        let att = attribution
                            .attributions
                            .get(ti)
                            .and_then(|a| a.as_ref())
                            .map(|a| (a.input, a.face));
                        format!("t{ti}{t:?}@{att:?}")
                    })
                    .collect();
                let d = d3(
                    mesh.verts[x as usize].as_array(),
                    mesh.verts[y as usize].as_array(),
                );
                eprintln!(
                    "[451-audit-edge] ({x},{y}) n={n} len={d:.3e} wits: {}",
                    wits.join(" ")
                );
            }
        }
    }
    if std::env::var_os("YANG_451_CHI").is_some() {
        let chi = |mesh: &Mesh| -> (usize, usize, usize) {
            let mut vs: std::collections::BTreeSet<u32> = Default::default();
            let mut es: std::collections::BTreeSet<(u32, u32)> = Default::default();
            for t in &mesh.tris {
                for k in 0..3 {
                    vs.insert(t[k]);
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    es.insert((a.min(b), a.max(b)));
                }
            }
            (vs.len(), es.len(), mesh.tris.len())
        };
        let (v, e, f) = chi(mesh);
        eprintln!(
            "[451-transit] CHI post-apply V={v} E={e} F={f} chi={}",
            v as i64 - e as i64 + f as i64
        );
    }
    Ok(true)
}
