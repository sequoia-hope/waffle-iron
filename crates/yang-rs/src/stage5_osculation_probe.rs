//! #188 inc-0 — read-only osculating-boundary-pair probe for the Stage-5/6
//! curved-patch emission (spec `specs/yang_188_f0082_j3_envelope_selection.md`
//! §3.1 detection vocabulary, shipped per the §5 inc-0 promotion ladder as a
//! probe FIRST; promotion to a typed STOP is an inc-3 decision).
//!
//! Env `YANG_S5_OSCULATION_PROBE` (any value): per cylinder-patch loop in
//! `emit_topology`, report each (intersection-conic support × neighboring
//! original-plane support) pair's analytic axial gap g(θ) = g0 + amp·cos(θ−φ),
//! the combined chord-sagitta observability floor, the exact gap zeros (the
//! switch/triple points) and the |g| < floor weave band, plus the mesh-level
//! weave signature: support alternation count, azimuth fold-backs, bare switch
//! chords, and mixed-side (submerged) segments. A value containing `walk`
//! additionally dumps the full per-vertex loop walk for firing pairs; firing
//! pairs also get the incident-kept-triangle arm (spec inc-0 open question b:
//! does the weave-band boundary bound kept mesh on the dead side?).
//!
//! Read-only: called only under the env guard; never mutates; production is
//! byte-identical with the env unset.

#[allow(clippy::wildcard_imports)]
use crate::*;

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn unit(a: [f64; 3]) -> [f64; 3] {
    let n = norm(a);
    [a[0] / n, a[1] / n, a[2] / n]
}

/// Plane carrier of a planar conic: `n·x + d = 0` through the conic's
/// center/vertex with the conic's plane normal. `SurfacePair` (non-planar)
/// and `LineSegment` (no carrier) yield `None`.
fn curve_plane(c: &Curve) -> Option<([f64; 3], f64, &'static str)> {
    let (p, n, kind) = match c {
        Curve::Circle { center, normal, .. } => (center, normal, "Circle"),
        Curve::Ellipse { center, normal, .. } => (center, normal, "Ellipse"),
        Curve::Parabola { vertex, normal, .. } => (vertex, normal, "Parabola"),
        Curve::Hyperbola { center, normal, .. } => (center, normal, "Hyperbola"),
        Curve::LineSegment | Curve::SurfacePair { .. } => return None,
    };
    let n = n.as_array();
    Some((n, -dot(n, p.as_array()), kind))
}

/// Sign-normalized approximate plane equality (either orientation).
fn planes_match(n1: [f64; 3], d1: f64, n2: [f64; 3], d2: f64) -> bool {
    let same = norm(sub(n1, n2)) < 1e-9 && (d1 - d2).abs() < 1e-9;
    let anti = norm(sub(n1, [-n2[0], -n2[1], -n2[2]])) < 1e-9 && (d1 + d2).abs() < 1e-9;
    same || anti
}

struct Support {
    n: [f64; 3],
    d: f64,
    label: String,
}

/// Wrap an angle difference into (−π, π].
fn wrap(mut dt: f64) -> f64 {
    while dt > std::f64::consts::PI {
        dt -= 2.0 * std::f64::consts::PI;
    }
    while dt <= -std::f64::consts::PI {
        dt += 2.0 * std::f64::consts::PI;
    }
    dt
}

pub(crate) fn osculation_probe_for_patch(
    mesh: &Mesh,
    infos: &[PatchInfo],
    info_index: usize,
    subdivided_cycles: &[Vec<Vec<(u32, u32)>>],
    intersection_curves: &std::collections::BTreeMap<(u32, u32), Curve>,
) {
    let info = &infos[info_index];
    let cycles = &subdivided_cycles[info_index];
    let walk_arm = std::env::var("YANG_S5_OSCULATION_PROBE")
        .map(|v| v.contains("walk"))
        .unwrap_or(false);

    let Surface::Cylinder {
        axis_point,
        axis_dir,
        radius,
    } = info.inherited
    else {
        eprintln!(
            "[s5-osc] patch info={info_index} face={} input={:?} surface={:?} — non-cylinder, \
             skipped (inc-0 analytic gap vocabulary is cylinder×plane; spec §3.2.1)",
            info.face_idx,
            info.input,
            std::mem::discriminant(&info.inherited),
        );
        return;
    };

    // Probe-local cylinder chart: deterministic frame (NOT §7.9's J-centered
    // chart — cross-reference via the printed 3D points, not via u values).
    let a_hat = unit(axis_dir.as_array());
    let seed = {
        let ax = [a_hat[0].abs(), a_hat[1].abs(), a_hat[2].abs()];
        let k = if ax[0] <= ax[1] && ax[0] <= ax[2] {
            0
        } else if ax[1] <= ax[2] {
            1
        } else {
            2
        };
        let mut e = [0.0f64; 3];
        e[k] = 1.0;
        e
    };
    let x_hat = {
        let pa = dot(seed, a_hat);
        unit(sub(seed, [pa * a_hat[0], pa * a_hat[1], pa * a_hat[2]]))
    };
    let y_hat = cross(a_hat, x_hat);
    let ap = axis_point.as_array();
    // (θ azimuth, v axial, radial error off the cylinder)
    let chart = |p: [f64; 3]| -> (f64, f64, f64) {
        let q = sub(p, ap);
        let v = dot(q, a_hat);
        let w = sub(q, [v * a_hat[0], v * a_hat[1], v * a_hat[2]]);
        (dot(w, y_hat).atan2(dot(w, x_hat)), v, norm(w) - radius)
    };
    let embed = |theta: f64, v: f64| -> [f64; 3] {
        let (c, s) = (theta.cos(), theta.sin());
        [
            ap[0] + radius * (c * x_hat[0] + s * y_hat[0]) + v * a_hat[0],
            ap[1] + radius * (c * x_hat[1] + s * y_hat[1]) + v * a_hat[1],
            ap[2] + radius * (c * x_hat[2] + s * y_hat[2]) + v * a_hat[2],
        ]
    };

    let vert_set: std::collections::BTreeSet<u32> = cycles
        .iter()
        .flat_map(|c| c.iter().map(|&(s, _)| s))
        .collect();
    eprintln!(
        "[s5-osc] patch info={info_index} face={} input={:?} Cylinder r={radius:.6} \
         cycles={} verts={}",
        info.face_idx,
        info.input,
        cycles.len(),
        vert_set.len()
    );

    // ---- Supports ----------------------------------------------------------
    // Intersection-conic supports: distinct planar carriers among this
    // patch's attributed loop edges.
    let mut ints: Vec<(Support, usize)> = Vec::new();
    let mut lineseg_edges = 0usize;
    for cycle in cycles {
        for &(s, e) in cycle {
            let key = if s < e { (s, e) } else { (e, s) };
            match intersection_curves.get(&key).and_then(curve_plane) {
                Some((n, d, kind)) => {
                    if let Some(entry) = ints
                        .iter_mut()
                        .find(|(sup, _)| planes_match(sup.n, sup.d, n, d))
                    {
                        entry.1 += 1;
                    } else {
                        ints.push((
                            Support {
                                n,
                                d,
                                label: format!("int{}({kind})", ints.len()),
                            },
                            1,
                        ));
                    }
                }
                None => lineseg_edges += 1,
            }
        }
    }
    // Original-plane supports: planar patches sharing ≥2 loop vertices.
    let mut origs: Vec<(Support, usize)> = Vec::new();
    for (j, other) in infos.iter().enumerate() {
        if j == info_index {
            continue;
        }
        let Surface::Plane { normal, d } = other.inherited else {
            continue;
        };
        let shared = subdivided_cycles[j]
            .iter()
            .flat_map(|c| c.iter().map(|&(s, _)| s))
            .filter(|v| vert_set.contains(v))
            .collect::<std::collections::BTreeSet<u32>>()
            .len();
        if shared >= 2 {
            // Dedup identical planes (several PatchInfos can sit on one plane);
            // keep the first, accumulate shared-vert counts.
            if let Some(entry) = origs
                .iter_mut()
                .find(|(sup, _)| planes_match(sup.n, sup.d, normal.as_array(), d))
            {
                entry.1 += shared;
            } else {
                origs.push((
                    Support {
                        n: normal.as_array(),
                        d,
                        label: format!(
                            "orig(info={j},face={},in={:?})",
                            other.face_idx, other.input
                        ),
                    },
                    shared,
                ));
            }
        }
    }
    for (sup, cnt) in &ints {
        eprintln!(
            "[s5-osc]   support {} n=({:.9},{:.9},{:.9}) d={:.9} edges={cnt}",
            sup.label, sup.n[0], sup.n[1], sup.n[2], sup.d
        );
    }
    for (sup, shared) in &origs {
        eprintln!(
            "[s5-osc]   support {} n=({:.9},{:.9},{:.9}) d={:.9} shared_verts={shared}",
            sup.label, sup.n[0], sup.n[1], sup.n[2], sup.d
        );
    }
    eprintln!("[s5-osc]   lineseg/unattributed edges={lineseg_edges}");

    // ---- Pairs -------------------------------------------------------------
    for (int_sup, _) in &ints {
        for (orig_sup, _) in &origs {
            probe_pair(
                mesh, cycles, int_sup, orig_sup, radius, &chart, &embed, walk_arm,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn probe_pair(
    mesh: &Mesh,
    cycles: &[Vec<(u32, u32)>],
    int_sup: &Support,
    orig_sup: &Support,
    radius: f64,
    chart: &dyn Fn([f64; 3]) -> (f64, f64, f64),
    embed: &dyn Fn(f64, f64) -> [f64; 3],
    walk_arm: bool,
) {
    let pair = format!("{}×{}", int_sup.label, orig_sup.label);
    if planes_match(int_sup.n, int_sup.d, orig_sup.n, orig_sup.d) {
        eprintln!("[s5-osc]   pair {pair}: coincident planes (conic IS this plane's cut) — skip");
        return;
    }
    // Axial profile of plane k on the cylinder: v_k(θ) = c0 + cc·cosθ + cs·sinθ,
    // from n·(A + r cosθ x̂ + r sinθ ŷ + v â) + d = 0. Needs the plane
    // non-parallel to the axis. Recover the frame from `embed`/`chart` by
    // probing (cheap, avoids threading 4 more args).
    let a_hat = unit(sub(embed(0.0, 1.0), embed(0.0, 0.0)));
    let center = {
        let a = embed(0.0, 0.0);
        let b = embed(std::f64::consts::PI, 0.0);
        [
            (a[0] + b[0]) / 2.0,
            (a[1] + b[1]) / 2.0,
            (a[2] + b[2]) / 2.0,
        ]
    };
    let rx = sub(embed(0.0, 0.0), center); // r·x̂
    let ry = sub(embed(std::f64::consts::FRAC_PI_2, 0.0), center); // r·ŷ
    let profile = |sup: &Support| -> Option<(f64, f64, f64)> {
        let na = dot(sup.n, a_hat);
        if na.abs() < 1e-9 {
            return None;
        }
        Some((
            -(dot(sup.n, center) + sup.d) / na,
            -dot(sup.n, rx) / na,
            -dot(sup.n, ry) / na,
        ))
    };
    let (Some((i0, ic, is_)), Some((j0, jc, js))) = (profile(int_sup), profile(orig_sup)) else {
        eprintln!("[s5-osc]   pair {pair}: a plane is axis-parallel — no axial profile, skip");
        return;
    };
    let v_int = |t: f64| i0 + ic * t.cos() + is_ * t.sin();
    let v_orig = |t: f64| j0 + jc * t.cos() + js * t.sin();
    let (g0, gc, gs) = (i0 - j0, ic - jc, is_ - js);
    let amp = gc.hypot(gs);
    let phi = gs.atan2(gc);

    // ---- per-edge classification ------------------------------------------
    // (cycle, idx, s, e, θs, θe, label_int?, sagitta, sd_int(mid), sd_orig(mid),
    //  axial dists at endpoints)
    struct EdgeRow {
        c: usize,
        i: usize,
        s: u32,
        e: u32,
        ts: f64,
        te: f64,
        is_int: bool,
        sag: f64,
        sd_i: f64,
        sd_j: f64,
        // per-endpoint axial distance to (int, orig)
        ends: [(f64, f64); 2],
    }
    let mut rows: Vec<EdgeRow> = Vec::new();
    for (c, cycle) in cycles.iter().enumerate() {
        for (i, &(s, e)) in cycle.iter().enumerate() {
            let ps = mesh.verts[s as usize].as_array();
            let pe = mesh.verts[e as usize].as_array();
            let (ts, vs, _) = chart(ps);
            let (te, ve, _) = chart(pe);
            let mid = [
                (ps[0] + pe[0]) / 2.0,
                (ps[1] + pe[1]) / 2.0,
                (ps[2] + pe[2]) / 2.0,
            ];
            let (tm, vm, _) = chart(mid);
            let (ai, aj) = ((vm - v_int(tm)).abs(), (vm - v_orig(tm)).abs());
            rows.push(EdgeRow {
                c,
                i,
                s,
                e,
                ts,
                te,
                is_int: ai <= aj,
                sag: radius * (1.0 - (wrap(te - ts) / 2.0).cos()),
                sd_i: dot(int_sup.n, mid) + int_sup.d,
                sd_j: dot(orig_sup.n, mid) + orig_sup.d,
                ends: [
                    ((vs - v_int(ts)).abs(), (vs - v_orig(ts)).abs()),
                    ((ve - v_int(te)).abs(), (ve - v_orig(te)).abs()),
                ],
            });
        }
    }
    // Combined chord-sagitta floor (C0118 vocabulary): max chord sagitta of
    // each chain, summed. Falls back to the overall max if a side is empty.
    let max_sag = |want_int: bool| {
        rows.iter()
            .filter(|r| r.is_int == want_int)
            .map(|r| r.sag)
            .fold(0.0f64, f64::max)
    };
    let (sag_i, sag_j) = (max_sag(true), max_sag(false));
    let floor = if sag_i > 0.0 && sag_j > 0.0 {
        sag_i + sag_j
    } else {
        2.0 * sag_i.max(sag_j)
    };

    // min |g| sampled at loop vertices (dense enough for the probe).
    let min_abs_g = rows
        .iter()
        .map(|r| (g0 + amp * (r.ts - phi).cos()).abs())
        .fold(f64::INFINITY, f64::min);

    // ---- weave signature ---------------------------------------------------
    // Relevance filter: an edge participates in this pair's signature only if
    // its midpoint is within the observability floor of at least one support
    // (kills forced-label noise from far transversal planes). Ambiguous =
    // within the floor of BOTH supports — the sub-observability band itself.
    let relevant =
        |r: &EdgeRow| r.ends[0].0.min(r.ends[0].1) < floor || r.ends[1].0.min(r.ends[1].1) < floor;
    let ambiguous = rows
        .iter()
        .map(|r| {
            let mid_i = (r.ends[0].0 + r.ends[1].0) / 2.0;
            let mid_j = (r.ends[0].1 + r.ends[1].1) / 2.0;
            mid_i < floor && mid_j < floor
        })
        .collect::<Vec<bool>>();
    let ambiguous_count = ambiguous.iter().filter(|&&a| a).count();
    // Alternations among consecutive RELEVANT edges per cycle.
    let mut alternations = vec![0usize; cycles.len()];
    for (c, alt) in alternations.iter_mut().enumerate() {
        let labels: Vec<bool> = rows
            .iter()
            .filter(|r| r.c == c && relevant(r))
            .map(|r| r.is_int)
            .collect();
        let m = labels.len();
        if m > 1 {
            *alt = (0..m).filter(|&i| labels[i] != labels[(i + 1) % m]).count();
        }
    }
    // Folds at relevant vertices only (azimuth direction reversal).
    let mut folds: Vec<(usize, usize, u32, f64, f64, f64)> = Vec::new();
    for (c, cycle) in cycles.iter().enumerate() {
        let m = cycle.len();
        for i in 0..m {
            let (t_prev, vp, _) = chart(mesh.verts[cycle[i].0 as usize].as_array());
            let (t_cur, vc, _) = chart(mesh.verts[cycle[i].1 as usize].as_array());
            let t_next = chart(mesh.verts[cycle[(i + 1) % m].1 as usize].as_array()).0;
            let near = |t: f64, v: f64| (v - v_int(t)).abs().min((v - v_orig(t)).abs()) < floor;
            if !(near(t_prev, vp) && near(t_cur, vc)) {
                continue;
            }
            let (d1, d2) = (wrap(t_cur - t_prev), wrap(t_next - t_cur));
            if d1.abs() > 1e-12 && d2.abs() > 1e-12 && d1.signum() != d2.signum() {
                folds.push((c, i, cycle[i].1, t_cur * radius, d1, d2));
            }
        }
    }
    // Switch edges: RELEVANT edges whose endpoint nearest-support labels
    // differ (each endpoint itself within the floor of some support); bare if
    // neither endpoint sits on BOTH curves (within 1e-7 axial) — no junction
    // vertex at the chain jump.
    let mut switch_edges: Vec<(&EdgeRow, bool)> = Vec::new();
    for r in &rows {
        let lab = |ends: (f64, f64)| ends.0 <= ends.1;
        let end_relevant = |ends: (f64, f64)| ends.0.min(ends.1) < floor;
        if end_relevant(r.ends[0]) && end_relevant(r.ends[1]) && lab(r.ends[0]) != lab(r.ends[1]) {
            let junction = r.ends.iter().any(|&(a, b)| a < 1e-7 && b < 1e-7);
            switch_edges.push((r, !junction));
        }
    }
    let bare = switch_edges.iter().filter(|(_, b)| *b).count();
    // Mixed-side counts over RELEVANT edges: orig-labeled edges by sign of
    // sd_int, and vice versa (a chain living on both sides of the other
    // support's plane = wrong-side emission candidate).
    let side_counts = |want_int: bool| {
        let (mut pos, mut neg, mut lo, mut hi) = (0usize, 0usize, f64::INFINITY, f64::NEG_INFINITY);
        for r in rows.iter().filter(|r| r.is_int == want_int && relevant(r)) {
            let sd = if want_int { r.sd_j } else { r.sd_i };
            if sd > 0.0 {
                pos += 1;
            } else {
                neg += 1;
            }
            lo = lo.min(sd);
            hi = hi.max(sd);
        }
        (pos, neg, lo, hi)
    };
    let (op_, on_, olo, ohi) = side_counts(false); // orig-labeled vs sd_int
    let (ip_, in_, ilo, ihi) = side_counts(true); // int-labeled vs sd_orig
    let mixed = op_.min(on_) + ip_.min(in_);

    let max_alt = alternations.iter().copied().max().unwrap_or(0);
    // Analytic fraction of the full circle where |g| < floor: 1.0 = the pair
    // is sub-observable EVERYWHERE (true osculation); ~0 = transversal.
    let band_frac = {
        let lo = ((-floor - g0) / amp).clamp(-1.0, 1.0);
        let hi = ((floor - g0) / amp).clamp(-1.0, 1.0);
        if amp > 0.0 {
            (lo.acos() - hi.acos()) / std::f64::consts::PI
        } else if g0.abs() < floor {
            1.0
        } else {
            0.0
        }
    };
    // FIRE = a genuine sub-observability band (≥3 ambiguous edges — a
    // transversal crossing yields at most 1-2) + a weave signature within it.
    let fire = ambiguous_count >= 3 && (max_alt > 2 || !folds.is_empty() || bare > 0 || mixed > 0);

    eprintln!(
        "[s5-osc]   pair {pair}: g0={g0:.6e} amp={amp:.6e} phi={phi:.6} floor={floor:.6e} \
         (sag_int={sag_i:.3e} sag_orig={sag_j:.3e}) min|g|_loop={min_abs_g:.6e} \
         band_frac={band_frac:.3} ambiguous={ambiguous_count} alt={alternations:?} folds={} \
         switch_edges={} bare={bare} mixed={mixed} => {}",
        folds.len(),
        switch_edges.len(),
        if fire { "FIRE" } else { "ok" }
    );
    eprintln!(
        "[s5-osc]   pair {pair}: orig-labeled sd_int pos/neg={op_}/{on_} range=[{olo:.3e},{ohi:.3e}] ; \
         int-labeled sd_orig pos/neg={ip_}/{in_} range=[{ilo:.3e},{ihi:.3e}]"
    );

    // Nearest loop vertex to a 3D point.
    let nearest_vert = |p: [f64; 3]| -> (u32, f64) {
        let mut best = (u32::MAX, f64::INFINITY);
        for r in &rows {
            for v in [r.s, r.e] {
                let d = norm(sub(mesh.verts[v as usize].as_array(), p));
                if d < best.1 {
                    best = (v, d);
                }
            }
        }
        best
    };
    // Exact gap zeros = the switch/triple points.
    if amp >= g0.abs() && amp > 0.0 {
        let dt = (-g0 / amp).acos();
        for t in [wrap(phi + dt), wrap(phi - dt)] {
            let p = embed(t, v_int(t));
            let (nv, nd) = nearest_vert(p);
            eprintln!(
                "[s5-osc]   pair {pair}: gap ZERO theta={t:.6} u={:.6} p=({:.9},{:.9},{:.9}) \
                 nearest_vert={nv} dist={nd:.3e}",
                t * radius,
                p[0],
                p[1],
                p[2]
            );
        }
        // Weave-band edges: |g(θ)| = floor (open question a: does the bare
        // switch chord sit at the band edge?).
        for f in [floor, -floor] {
            let c = (f - g0) / amp;
            if c.abs() <= 1.0 {
                let dt = c.acos();
                for t in [wrap(phi + dt), wrap(phi - dt)] {
                    let vm = (v_int(t) + v_orig(t)) / 2.0;
                    let p = embed(t, vm);
                    let (nv, nd) = nearest_vert(p);
                    eprintln!(
                        "[s5-osc]   pair {pair}: band edge g={f:.3e} theta={t:.6} u={:.6} \
                         p=({:.9},{:.9},{:.9}) nearest_vert={nv} dist={nd:.3e}",
                        t * radius,
                        p[0],
                        p[1],
                        p[2]
                    );
                }
            }
        }
    }
    for (c, i, v, u, d1, d2) in folds.iter().take(12) {
        eprintln!(
            "[s5-osc]   pair {pair}: FOLD cycle={c} idx={i} vert={v} u={u:.6} \
             dtheta=({d1:.3e},{d2:.3e})"
        );
    }
    for (r, bare) in switch_edges.iter().take(12) {
        eprintln!(
            "[s5-osc]   pair {pair}: SWITCH{} cycle={} idx={} {}->{} u={:.6}->{:.6} \
             ends_axial int/orig=({:.3e},{:.3e})/({:.3e},{:.3e})",
            if *bare { "(bare)" } else { "" },
            r.c,
            r.i,
            r.s,
            r.e,
            r.ts * radius,
            r.te * radius,
            r.ends[0].0,
            r.ends[0].1,
            r.ends[1].0,
            r.ends[1].1,
        );
    }

    if !fire {
        return;
    }
    // ---- incident kept-triangle arm (open question b) ----------------------
    // For wrong-side and switch edges: which kept triangles use this boundary
    // edge, and which side of each support plane do they extend to?
    let minority_orig_sign = if op_ < on_ { 1.0 } else { -1.0 };
    let mut printed = 0usize;
    for r in &rows {
        let wrongside = !r.is_int && r.sd_i * minority_orig_sign > 0.0;
        let is_switch = switch_edges.iter().any(|(sr, _)| std::ptr::eq(*sr, r));
        if !(wrongside || is_switch) || printed >= 24 {
            continue;
        }
        printed += 1;
        for (t, tri) in mesh.tris.iter().enumerate() {
            let has = |v: u32| tri.contains(&v);
            if has(r.s) && has(r.e) {
                let cen = {
                    let mut c = [0.0f64; 3];
                    for &v in tri {
                        let p = mesh.verts[v as usize].as_array();
                        c = [c[0] + p[0] / 3.0, c[1] + p[1] / 3.0, c[2] + p[2] / 3.0];
                    }
                    c
                };
                eprintln!(
                    "[s5-osc]   pair {pair}: KEPT-TRI edge {}->{} ({}) tri={t} verts={tri:?} \
                     centroid sd_int={:.6e} sd_orig={:.6e}",
                    r.s,
                    r.e,
                    if wrongside { "wrongside" } else { "switch" },
                    dot(int_sup.n, cen) + int_sup.d,
                    dot(orig_sup.n, cen) + orig_sup.d,
                );
            }
        }
    }
    // ---- full walk arm -----------------------------------------------------
    if walk_arm {
        for (c, cycle) in cycles.iter().enumerate() {
            for (i, &(s, _)) in cycle.iter().enumerate() {
                let p = mesh.verts[s as usize].as_array();
                let (t, v, rerr) = chart(p);
                let row = &rows[cycles[..c].iter().map(Vec::len).sum::<usize>() + i];
                eprintln!(
                    "[s5-osc]   walk c={c} i={i} v={s} u={:.6} vax={v:.6} rerr={rerr:.2e} \
                     sd_int={:.6e} sd_orig={:.6e} lab={} p=({:.9},{:.9},{:.9})",
                    t * radius,
                    dot(int_sup.n, p) + int_sup.d,
                    dot(orig_sup.n, p) + orig_sup.d,
                    if row.is_int { "INT" } else { "ORIG" },
                    p[0],
                    p[1],
                    p[2],
                );
            }
        }
    }
}
