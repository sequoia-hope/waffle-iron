//! Rim-junction override machinery for the `boolean()` driver: the Case-IV
//! phantom rim-segment guard (`cyl_pair_phantom_n`, `phantom_min_rim_segments`),
//! the exact cross-solid rim-junction insertion scan (`rim_junctions_against`),
//! and the rim/planar-face containment predicates it consults (`RimDesc`,
//! `point_in_rim_sweep`, `planar_face_segments`, `point_in_planar_face`,
//! `rim_junction_overrides`). Extracted verbatim from `boolean.rs` (move-only,
//! spec `specs/yang_rs_lib_decomposition.md` F9).

#[allow(clippy::wildcard_imports)]
use crate::*;

/// Case-IV phantom guard analysis (spec `yang_case_iv_phantom_guard`,
/// M8 increment 15): the forced minimum rim segment count over all
/// ANALYTICALLY DISJOINT cylinder-face pairs (A×B) whose Stage-1 chord
/// bands could otherwise overlap the gap between the surfaces (Yang Fig. 8
/// Case IV — the meshes would intersect where the surfaces do not,
/// manufacturing a phantom intersection curve; measured F0088 op 4).
///
/// For each pair: the axis-line distance gives the analytic gap (external
/// `d − r_a − r_b` for any axis pose; nested `r_large − d − r_small` for
/// parallel axes). A positive gap demands the smallest `N` with
/// `sag(r_a, N) + sag(r_b, N) ≤ gap/2` (`sag(r, N) = r(1 − cos(π/N))` —
/// the Stage-1 sagitta, A14.3 single source; the factor-2 margin keeps the
/// combined band strictly clear, and a finer N is always chord-valid). Far
/// pairs derive a tiny N that the natural Stage-1 `max()` absorbs — the
/// guard is self-limiting, no mode branch. True near-tangency (N would
/// exceed 4096) yields no requirement: the loud Stage-3 `AmbiguousCurve`
/// remains the tripwire (P9 — never silently proceed with phantom
/// topology).
/// The Case-IV pairwise requirement of two cylinder surfaces (spec
/// `yang_case_iv_phantom_guard`): `None` unless the pair is analytically
/// disjoint with a practical derived N — the smallest `N` with
/// `sag(r_a, N) + sag(r_b, N) ≤ gap/2` (`sag(r, N) = r(1 − cos(π/N))`, the
/// Stage-1 sagitta; the factor-2 margin keeps the combined chord band
/// strictly clear of the gap, and a finer N is always chord-valid). Shared
/// by the `boolean()` cross-pair guard AND Stage 1's intra-solid fold.
pub(crate) fn cyl_pair_phantom_n(
    (pa, da, ra): (Point3, Vector3, f64),
    (pb, db, rb): (Point3, Vector3, f64),
) -> Option<usize> {
    let ua = normalize3(da.as_array());
    let ub = normalize3(db.as_array());
    let w = [pb.x() - pa.x(), pb.y() - pa.y(), pb.z() - pa.z()];
    let cx = [
        ua[1] * ub[2] - ua[2] * ub[1],
        ua[2] * ub[0] - ua[0] * ub[2],
        ua[0] * ub[1] - ua[1] * ub[0],
    ];
    let cross_norm = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    // Axis-line distance: skew/crossing axes project the offset onto the
    // common normal; parallel axes take the perpendicular point-line
    // distance.
    let (parallel, d_axes) = if cross_norm > 1e-12 {
        let d = (w[0] * cx[0] + w[1] * cx[1] + w[2] * cx[2]).abs() / cross_norm;
        (false, d)
    } else {
        let t = w[0] * ua[0] + w[1] * ua[1] + w[2] * ua[2];
        let perp = [w[0] - t * ua[0], w[1] - t * ua[1], w[2] - t * ua[2]];
        let d = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
        (true, d)
    };
    let external = d_axes - (ra + rb);
    let nested = if parallel {
        ra.max(rb) - d_axes - ra.min(rb)
    } else {
        f64::NEG_INFINITY
    };
    let gap = external.max(nested);
    if gap.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return None; // surfaces intersect / NaN (degenerate input): real curve or no-op
    }
    let sag = |r: f64, n: usize| r * (1.0 - (std::f64::consts::PI / n as f64).cos());
    let mut n = 3usize;
    while sag(ra, n) + sag(rb, n) > gap / 2.0 {
        n += 1;
        if n > 4096 {
            // True near-tangency: no finite practical N — leave the loud
            // Stage-3 stop as the tripwire.
            return None;
        }
    }
    Some(n)
}

pub(crate) fn phantom_min_rim_segments(a: &BRep, b: &BRep) -> Option<usize> {
    let cyls = |brep: &BRep| -> Vec<(Point3, Vector3, f64)> {
        brep.faces()
            .iter()
            .filter_map(|f| match f.surface {
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                } => Some((axis_point, axis_dir, radius)),
                _ => None,
            })
            .collect()
    };
    let (ca, cb) = (cyls(a), cyls(b));
    let mut req: Option<usize> = None;
    // CROSS pairs only (A×B): the two operands' meshes must not intersect
    // where their surfaces do not (the measured F0088 cut-4 class).
    // INTRA-solid pairs are folded into Stage 1's own N selection
    // (`stage1_tessellate_inner` — M8 increment 16), where EVERY
    // tessellation of the solid picks them up (conversion, Stage-0 rebuilds,
    // this guard's rebuilds), so they need no handling here.
    for &sa in &ca {
        for &sb in &cb {
            if let Some(n) = cyl_pair_phantom_n(sa, sb) {
                req = Some(req.map_or(n, |r: usize| r.max(n)));
            }
        }
    }
    // Self-limiting gate: a requirement BOTH solids' natural Stage-1 N
    // already satisfies is dropped, keeping the common path byte-identical
    // (and rebuild-free). `natural_rim_n` mirrors the Stage-1 N derivation
    // (chord bound over all rim circles, N from the max radius).
    let natural_rim_n = |brep: &BRep| -> usize {
        let Some(d_eps) = curved_chord_bound(brep.edges()) else {
            return usize::MAX; // no circles: nothing to boost
        };
        let max_r = brep
            .edges()
            .iter()
            .filter_map(|e| match e.curve {
                Curve::Circle { radius, .. } => Some(radius),
                _ => None,
            })
            .fold(0.0f64, f64::max);
        let mut n = 3usize;
        if d_eps > 0.0 {
            while max_r * (1.0 - (std::f64::consts::PI / n as f64).cos()) > d_eps {
                n += 1;
            }
        }
        n
    };
    let gated = match req {
        Some(n) if n > natural_rim_n(a) || n > natural_rim_n(b) => Some(n),
        _ => None,
    };
    if std::env::var_os("YANG_SPLIT_PROBE").is_some() {
        eprintln!(
            "[phantom-guard] req={req:?} natural=({},{}) gated={gated:?} \
             cyl_faces=({},{})",
            natural_rim_n(a),
            natural_rim_n(b),
            a.faces()
                .iter()
                .filter(|f| matches!(f.surface, Surface::Cylinder { .. }))
                .count(),
            b.faces()
                .iter()
                .filter(|f| matches!(f.surface, Surface::Cylinder { .. }))
                .count(),
        );
    }
    gated
}

/// N2/F0059 epic increment 2, BANKED-UNWIRED (spec
/// `yang_rim_junction_insertion`): per full-circle rim edge of `x`, the
/// exact points where that rim circle transversally CROSSES one of `y`'s
/// cylinder laterals — the §4.3.3 Case-IV junction points that Stage-1
/// must carry as rim samples so the mesh-level seam chains can terminate
/// exactly at the junctions (the truncated-Steinmetz cap-lobe corners).
///
/// v1 closed-form scope (A13.3/P8 — no ad-hoc root finding): only
/// laterals whose axis is PARALLEL to the rim plane contribute (their
/// section in the rim plane is two lines ⇒ circle∩line quadratics; the
/// F0059 class). Transversal-axis laterals (ellipse section, quartic) and
/// non-cylinder surfaces are out of scope and keep today's loud walls.
/// Tangent grazes are excluded by a DERIVED resolution gate: a root pair
/// closer than `TAU_MODEL` along the section line is one model point
/// (A14.2), i.e. the §4.3.3 tangency class — not a transversal crossing.
///
/// Returned points satisfy `|‖p−c‖−r| ≤ TAU_WORK` and lie on the
/// contributing lateral to fp accuracy (unit-asserted). Deterministic:
/// faces in index order, both section lines, roots in ascending-t order.
pub(crate) fn rim_junctions_against(
    x: &BRep,
    y: &BRep,
) -> std::collections::BTreeMap<u32, Vec<Point3>> {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let add = |a: [f64; 3], b: [f64; 3]| [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
    let scl = |a: [f64; 3], s: f64| [a[0] * s, a[1] * s, a[2] * s];
    let crs = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    // The lateral's axial extent, from the Circle edges its loops carry
    // (both rims project onto the axis; a lateral without circle loop
    // edges yields None → skipped, loud walls preserved).
    let lateral_extent = |brep: &BRep, f: &BRepFace, ap: [f64; 3], d: [f64; 3]| {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
            if let Curve::Circle { center, .. } = brep.edges()[ei as usize].curve {
                let z = dot(sub(center.as_array(), ap), d);
                lo = lo.min(z);
                hi = hi.max(z);
            }
        }
        (lo < hi).then_some((lo, hi))
    };

    let probe = std::env::var_os("YANG_RIM_JUNCTION_PROBE").is_some();
    if probe {
        let full_rims = x
            .edges()
            .iter()
            .filter(|e| e.start == e.end && matches!(e.curve, Curve::Circle { .. }))
            .count();
        let mut kinds: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for f in y.faces() {
            let k = match f.surface {
                Surface::Plane { .. } => "plane",
                Surface::Cylinder { .. } => "cyl",
                Surface::Cone { .. } => "cone",
                Surface::Sphere { .. } => "sphere",
                Surface::Torus { .. } => "torus",
            };
            *kinds.entry(k).or_default() += 1;
        }
        eprintln!(
            "[rim-junction] x: edges={} full_circle_rims={full_rims}; y faces: {kinds:?}",
            x.edges().len()
        );
        let mut ekinds: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for e in x.edges() {
            let k = match e.curve {
                Curve::Circle { .. } => {
                    if e.start == e.end {
                        "circle-closed".to_string()
                    } else {
                        "circle-arc".to_string()
                    }
                }
                Curve::LineSegment => "line".to_string(),
                ref other => format!("{other:?}")
                    .split([' ', '{'])
                    .next()
                    .unwrap_or("?")
                    .to_string(),
            };
            *ekinds.entry(k).or_default() += 1;
        }
        eprintln!("[rim-junction] x edge kinds: {ekinds:?}");
    }
    let mut out: std::collections::BTreeMap<u32, Vec<Point3>> = std::collections::BTreeMap::new();
    // Rim geometry retained for the §4b coaxial propagation post-pass.
    let mut rims: Vec<RimDesc> = Vec::new();
    for (ei, e) in x.edges().iter().enumerate() {
        let Curve::Circle {
            center,
            normal,
            radius: r,
        } = e.curve
        else {
            continue;
        };
        let n = normalize3(normal.as_array());
        let c = center.as_array();
        // Increment 4 (measured scope correction): partial-revolve rims are
        // ARC edges — candidates are filtered to the CCW sweep window
        // (stage-1 arc-chain convention) by `point_in_rim_sweep`, which
        // also rejects candidates coinciding with the rim's own B-Rep
        // vertices (arc endpoints / the closed rim's seam): such a
        // junction already IS a mesh vertex, and inserting its twin would
        // trip the uniform-coincidence stop (the seam sits at ring slot 0).
        let arc = if e.start != e.end {
            Some((
                x.vertices()[e.start as usize].point.as_array(),
                x.vertices()[e.end as usize].point.as_array(),
            ))
        } else {
            None
        };
        let rim = RimDesc {
            edge: ei as u32,
            c,
            n,
            r,
            seam: x.vertices()[e.start as usize].point.as_array(),
            arc,
        };
        // Increment 4 v1 scope (demonstrated need — the whole measured
        // class is CONE-band lathes): the PLANE arm fires only on rims
        // flanked by ≥1 cone face. Cylinder-rim × plane-face junctions
        // have no demanding case, and the corpus proves that population
        // healthy without insertion (F0047/R0006/R0075/F0081 were CORRECT
        // pre-arm and regressed under it; R0091's cut-tool rim insertions
        // unmask the banked-§3b unverifiable-χ path). The LATERAL arm
        // (the F0059 cylinder class) is independent and unchanged.
        let cone_flanked = x.faces().iter().any(|f| {
            matches!(f.surface, Surface::Cone { .. })
                && f.outer_loop
                    .iter()
                    .chain(f.inner_loops.iter().flatten())
                    .any(|&le| le == ei as u32)
        });
        let mut pts: Vec<Point3> = Vec::new();
        // Shared circle∩line quadratic for a line (q0 + t·u) in the rim
        // plane: t² + 2t·(q0−c)·u + |q0−c|² − r² = 0. `None` = miss or
        // graze (derived tangency gate, A14.2: roots closer than model
        // resolution are ONE point, not two transversal crossings).
        let circle_line_roots = |q0: [f64; 3], u: [f64; 3]| -> Option<[f64; 2]> {
            let m = sub(q0, c);
            let bq = dot(m, u);
            let cq = dot(m, m) - r * r;
            let disc = bq * bq - cq;
            if disc <= 0.0 {
                return None; // no crossing / exact tangent
            }
            let sq = disc.sqrt();
            if 2.0 * sq < cad_primitives::TAU_MODEL {
                return None;
            }
            Some([-bq - sq, -bq + sq])
        };
        for f in y.faces() {
            match f.surface {
                Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius: rb,
                } => {
                    let d = normalize3(axis_dir.as_array());
                    // v1: axis parallel to the rim plane (same floor as the
                    // phantom guard's axis-parallel test).
                    if dot(n, d).abs() > 1e-12 {
                        continue;
                    }
                    let ap = axis_point.as_array();
                    let Some((z_lo, z_hi)) = lateral_extent(y, f, ap, d) else {
                        continue;
                    };
                    // Signed axis-to-rim-plane distance; |δ| ≥ r_b ⇒ empty
                    // or a plane-tangent lateral (the tangency class —
                    // skipped).
                    let delta = dot(n, sub(ap, c));
                    if delta.abs() >= rb {
                        continue;
                    }
                    // Section of the lateral in the rim plane: two lines
                    // parallel to the axis at in-plane offset ±√(r_b²−δ²)
                    // from the axis foot.
                    let w_half = (rb * rb - delta * delta).sqrt();
                    let foot = sub(ap, scl(n, delta));
                    let eo = normalize3(crs(d, n));
                    for sgn in [-1.0f64, 1.0] {
                        let q0 = add(foot, scl(eo, sgn * w_half));
                        let Some(roots) = circle_line_roots(q0, d) else {
                            continue;
                        };
                        for t in roots {
                            let pj = add(q0, scl(d, t));
                            // Inside the lateral's axial extent; the
                            // ±TAU_WORK slack keeps boundary-of-extent
                            // triple junctions (rim ∩ lateral ∩ far cap —
                            // the F0059 corners).
                            let z = dot(sub(pj, ap), d);
                            if z < z_lo - cad_primitives::TAU_WORK
                                || z > z_hi + cad_primitives::TAU_WORK
                            {
                                continue;
                            }
                            if !point_in_rim_sweep(&rim, pj) {
                                continue;
                            }
                            let pjp = Point3::new(pj[0], pj[1], pj[2]);
                            // Cross-arm dedup at model resolution (two
                            // laterals / both lines can meet the rim at one
                            // triple point).
                            let dup = pts.iter().any(|q| {
                                let qa = q.as_array();
                                let dd = sub(qa, pj);
                                dot(dd, dd) < cad_primitives::TAU_MODEL * cad_primitives::TAU_MODEL
                            });
                            if !dup {
                                pts.push(pjp);
                            }
                        }
                    }
                }
                // Increment 4 §4a (spec v1 table row 2, promoted): a PLANE
                // face sections the rim plane in a single line — the
                // coaxial cone-band junction class (R0017 et al.).
                Surface::Plane { normal: m, d } => {
                    if !cone_flanked {
                        continue; // v1 scope: cone-band rims only (see above)
                    }
                    let ma = m.as_array();
                    let mlen = dot(ma, ma).sqrt();
                    if mlen <= 0.0 {
                        continue;
                    }
                    let mh = scl(ma, 1.0 / mlen);
                    let dh = d / mlen;
                    let ndm = dot(n, mh);
                    let denom = 1.0 - ndm * ndm;
                    // Parallel/coincident planes have no transversal
                    // section line (same 1e-12 floor class as the lateral
                    // arm's axis test).
                    if denom <= 1e-12 {
                        continue;
                    }
                    // v1: polygonal faces only — every loop edge a
                    // LineSegment (arc-bounded caps keep today's walls).
                    let Some(face2d) = planar_face_segments(y, f, mh) else {
                        continue;
                    };
                    // Line P∩F: q0 lies in BOTH planes, direction u = n×m̂.
                    let alpha = -(dot(mh, c) + dh) / denom;
                    let mperp = sub(mh, scl(n, ndm));
                    let q0 = add(c, scl(mperp, alpha));
                    let u = normalize3(crs(n, mh));
                    let Some(roots) = circle_line_roots(q0, u) else {
                        continue;
                    };
                    for t in roots {
                        let pj = add(q0, scl(u, t));
                        // Within the face extents: boundary-inclusive
                        // (±TAU_WORK) 2D containment — the plane analog of
                        // the lateral arm's z-extent slack.
                        if !point_in_planar_face(&face2d, pj) {
                            continue;
                        }
                        if !point_in_rim_sweep(&rim, pj) {
                            continue;
                        }
                        let pjp = Point3::new(pj[0], pj[1], pj[2]);
                        let dup = pts.iter().any(|q| {
                            let qa = q.as_array();
                            let dd = sub(qa, pj);
                            dot(dd, dd) < cad_primitives::TAU_MODEL * cad_primitives::TAU_MODEL
                        });
                        if !dup {
                            pts.push(pjp);
                        }
                    }
                }
                _ => continue,
            }
        }
        if !pts.is_empty() {
            out.insert(ei as u32, pts);
        }
        rims.push(rim);
    }

    // §4b coaxial azimuth propagation: Stage-1 band strips
    // (`tessellate_cone_frustum_band`, the cylinder tube, the partial-arc
    // strips) pair rims ring-for-ring, so a junction azimuth inserted on
    // ONE rim of a coaxial stack must exist on ALL of them (where their
    // sweep covers it) — otherwise the stack's sample counts diverge and
    // the strip stops loudly.
    if !out.is_empty() {
        // Group rims by axis line: parallel normals (1e-12 floor) with
        // centers on one line (TAU_MODEL off-axis budget).
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for i in 0..rims.len() {
            let (ci, ni) = (rims[i].c, rims[i].n);
            let mut placed = false;
            for g in groups.iter_mut() {
                let (cj, nj) = (rims[g[0]].c, rims[g[0]].n);
                let cx = crs(ni, nj);
                if dot(cx, cx).sqrt() > 1e-12 {
                    continue;
                }
                let w = sub(ci, cj);
                let along = dot(w, nj);
                let off = sub(w, scl(nj, along));
                if dot(off, off).sqrt() > cad_primitives::TAU_MODEL {
                    continue;
                }
                g.push(i);
                placed = true;
                break;
            }
            if !placed {
                groups.push(vec![i]);
            }
        }
        for g in &groups {
            if !g.iter().any(|&i| out.contains_key(&rims[i].edge)) {
                continue;
            }
            // Vocabulary gate: every operand face touching a group rim
            // must be a Cone/Cylinder/Plane — the surfaces whose Stage-1
            // tessellation consumes shared rim rings. A torus/sphere band
            // stack keeps today's loud walls (never a half-inserted
            // stack).
            let rim_set: std::collections::BTreeSet<u32> =
                g.iter().map(|&i| rims[i].edge).collect();
            let vocab_ok = x.faces().iter().all(|f| {
                let touches = f
                    .outer_loop
                    .iter()
                    .chain(f.inner_loops.iter().flatten())
                    .any(|e| rim_set.contains(e));
                !touches
                    || matches!(
                        f.surface,
                        Surface::Cone { .. } | Surface::Cylinder { .. } | Surface::Plane { .. }
                    )
            });
            if !vocab_ok {
                for &i in g {
                    out.remove(&rims[i].edge);
                }
                continue;
            }
            // One shared frame about the group axis (g[0] is the
            // lowest-index rim — deterministic, I4). ALL window / dedup
            // decisions below are made in ANGLE space with ONE shared
            // tolerance `th_eps = TAU_MODEL / r_min` — per-radius chord
            // tolerances would let band-partner arcs (which share their
            // sweep window) disagree by a point and stop the Stage-1
            // strip loudly on a count mismatch (the R0019 161-vs-162
            // wall). Angle-space decisions are conformal by construction.
            let (c0, axis) = (rims[g[0]].c, rims[g[0]].n);
            let (b1v, b2v) = ortho_basis(Vector3::new(axis[0], axis[1], axis[2]));
            let (b1, b2) = (b1v.as_array(), b2v.as_array());
            let two_pi = 2.0 * std::f64::consts::PI;
            let group_az = |p: [f64; 3]| -> f64 {
                let w = sub(p, c0);
                dot(w, b2).atan2(dot(w, b1)).rem_euclid(two_pi)
            };
            let r_min = g
                .iter()
                .map(|&i| rims[i].r)
                .fold(f64::INFINITY, f64::min)
                .max(cad_primitives::MIN_FEATURE_SIZE);
            let th_eps = cad_primitives::TAU_MODEL / r_min;
            // A rim's admissible azimuth window, with the ±th_eps margin
            // excluding its own B-Rep vertices (arc endpoints / seam).
            let in_window = |rim: &RimDesc, th: f64| -> bool {
                match rim.arc {
                    Some((sp, ep)) => {
                        // Own-orientation sweep mapped through the GROUP
                        // frame: the CCW window about rim.n runs start->end;
                        // in the group frame it runs the same way when
                        // rim.n aligns with the group axis, reversed when
                        // anti-aligned.
                        let a0 = group_az(sp);
                        let a1 = group_az(ep);
                        let aligned = dot(rim.n, axis) >= 0.0;
                        let (w0, w1) = if aligned { (a0, a1) } else { (a1, a0) };
                        let sweep = (w1 - w0).rem_euclid(two_pi);
                        let off = (th - w0).rem_euclid(two_pi);
                        off > th_eps && off < sweep - th_eps
                    }
                    None => {
                        let off = (th - group_az(rim.seam)).rem_euclid(two_pi);
                        off > th_eps && off < two_pi - th_eps
                    }
                }
            };
            // Cluster ALL direct-junction azimuths at th_eps. Each cluster
            // is one physical junction column; its representative azimuth
            // is the smallest member (deterministic).
            let mut annotated: Vec<(f64, usize, Point3)> = Vec::new();
            for &i in g {
                if let Some(pts) = out.get(&rims[i].edge) {
                    for pt in pts {
                        annotated.push((group_az(pt.as_array()), i, *pt));
                    }
                }
            }
            annotated.sort_by(|x, y| x.0.total_cmp(&y.0));
            let mut clusters: Vec<Vec<(f64, usize, Point3)>> = Vec::new();
            for a in annotated {
                match clusters.last_mut() {
                    Some(cl) if (a.0 - cl.last().unwrap().0).abs() <= th_eps => cl.push(a),
                    _ => clusters.push(vec![a]),
                }
            }
            // Wrap-around: the first and last clusters may be one junction
            // column split at the 0/2pi cut.
            if clusters.len() > 1 {
                let lo = clusters.first().unwrap().first().unwrap().0;
                let hi = clusters.last().unwrap().last().unwrap().0;
                if (lo + two_pi - hi).abs() <= th_eps {
                    let merged = clusters.pop().unwrap();
                    clusters[0].extend(merged);
                }
            }
            // Rebuild every rim's list from the clusters: the rim's own
            // direct point where it has one (the exact junction position),
            // else the on-circle point at the cluster representative.
            for &i in g {
                let rim = &rims[i];
                let mut pts: Vec<Point3> = Vec::new();
                for cl in &clusters {
                    let th = cl.first().unwrap().0;
                    if !in_window(rim, th) {
                        continue;
                    }
                    if let Some(own) = cl.iter().find(|(_, ri, _)| *ri == i) {
                        pts.push(own.2);
                    } else {
                        let (st, ct) = th.sin_cos();
                        let pj = add(rim.c, add(scl(b1, rim.r * ct), scl(b2, rim.r * st)));
                        pts.push(Point3::new(pj[0], pj[1], pj[2]));
                    }
                }
                if pts.is_empty() {
                    out.remove(&rim.edge);
                } else {
                    out.insert(rim.edge, pts);
                }
            }
        }
    }
    out
}

/// Increment 4: rim descriptor for `rim_junctions_against` — a full-circle
/// rim or a partial ARC (the corpus partial-revolve shape). For an arc,
/// the sweep runs CCW about `n` from `arc.0` to `arc.1` (the stage-1
/// arc-chain convention).
pub(crate) struct RimDesc {
    pub(crate) edge: u32,
    pub(crate) c: [f64; 3],
    pub(crate) n: [f64; 3],
    pub(crate) r: f64,
    /// The edge's start vertex — the seam of a closed rim (ring slot 0).
    pub(crate) seam: [f64; 3],
    pub(crate) arc: Option<([f64; 3], [f64; 3])>,
}

/// Increment 4: candidate filter — never within TAU_MODEL of the rim's
/// own B-Rep vertices (arc endpoints / the closed rim's seam: a boundary
/// junction IS the existing vertex; inserting its ULP twin would trip the
/// uniform-coincidence stop or desynchronize the chain), and for an ARC,
/// inside the CCW sweep window. Full-circle rims accept everything else.
pub(crate) fn point_in_rim_sweep(rim: &RimDesc, pj: [f64; 3]) -> bool {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    {
        let dd = sub(pj, rim.seam);
        if dot(dd, dd) < cad_primitives::TAU_MODEL * cad_primitives::TAU_MODEL {
            return false;
        }
    }
    let Some((sp, ep)) = rim.arc else {
        return true;
    };
    for q in [sp, ep] {
        let dd = sub(pj, q);
        if dot(dd, dd) < cad_primitives::TAU_MODEL * cad_primitives::TAU_MODEL {
            return false;
        }
    }
    let (e1v, e2v) = ortho_basis(Vector3::new(rim.n[0], rim.n[1], rim.n[2]));
    let (e1, e2) = (e1v.as_array(), e2v.as_array());
    let ang = |q: [f64; 3]| -> f64 {
        let w = sub(q, rim.c);
        dot(w, e2).atan2(dot(w, e1))
    };
    let two_pi = 2.0 * std::f64::consts::PI;
    let phi0 = ang(sp);
    let sweep = (ang(ep) - phi0).rem_euclid(two_pi);
    let off = (ang(pj) - phi0).rem_euclid(two_pi);
    off < sweep
}

/// Increment 4 §4a: a planar face's loops as 2D segments + full circles in
/// the plane's own frame (frame returned alongside so containment projects
/// identically) — `None` when any loop edge is neither a `LineSegment` nor
/// a closed `Circle` (arc-bounded faces keep today's loud walls). Inner
/// loops (holes) are included: even-odd containment handles both segment
/// and circle boundaries by parity, so discs, annuli, polygons, and mixed
/// forms all work.
pub(crate) type PlanarFace2d = (
    [[f64; 3]; 2],
    Vec<([f64; 2], [f64; 2])>,
    Vec<([f64; 2], f64)>,
);

pub(crate) fn planar_face_segments(
    brep: &BRep,
    f: &BRepFace,
    plane_unit_normal: [f64; 3],
) -> Option<PlanarFace2d> {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let nh = plane_unit_normal;
    let (e1v, e2v) = ortho_basis(Vector3::new(nh[0], nh[1], nh[2]));
    let (e1, e2) = (e1v.as_array(), e2v.as_array());
    let mut segs: Vec<([f64; 2], [f64; 2])> = Vec::new();
    let mut circles: Vec<([f64; 2], f64)> = Vec::new();
    for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
        let e = &brep.edges()[ei as usize];
        match e.curve {
            Curve::LineSegment => {
                let a3 = brep.vertices()[e.start as usize].point.as_array();
                let b3 = brep.vertices()[e.end as usize].point.as_array();
                segs.push(([dot(a3, e1), dot(a3, e2)], [dot(b3, e1), dot(b3, e2)]));
            }
            Curve::Circle { center, radius, .. } if e.start == e.end => {
                let c3 = center.as_array();
                circles.push(([dot(c3, e1), dot(c3, e2)], radius));
            }
            _ => return None,
        }
    }
    Some(([e1, e2], segs, circles))
}

/// Increment 4 §4a: boundary-inclusive (±TAU_WORK) even-odd containment of
/// a 3D point (assumed ON the face plane) in the planar face's boundary
/// set. The TAU_WORK boundary band keeps triple junctions at face edges —
/// the plane analog of the lateral arm's z-extent slack. Holes are
/// excluded by parity (segment ray crossings + circle inside-count).
pub(crate) fn point_in_planar_face(face2d: &PlanarFace2d, p3: [f64; 3]) -> bool {
    let dot3 = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let ([e1, e2], segs, circles) = face2d;
    let p = [dot3(p3, *e1), dot3(p3, *e2)];
    // Boundary band first (a point within TAU_WORK of any loop boundary is
    // IN — never lose a face-edge triple junction to parity jitter).
    for &(a, b) in segs {
        let ab = [b[0] - a[0], b[1] - a[1]];
        let ap = [p[0] - a[0], p[1] - a[1]];
        let len2 = ab[0] * ab[0] + ab[1] * ab[1];
        let t = if len2 > 0.0 {
            ((ap[0] * ab[0] + ap[1] * ab[1]) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let dx = ap[0] - t * ab[0];
        let dy = ap[1] - t * ab[1];
        if (dx * dx + dy * dy).sqrt() <= cad_primitives::TAU_WORK {
            return true;
        }
    }
    for &(cc, r) in circles {
        let d = ((p[0] - cc[0]).powi(2) + (p[1] - cc[1]).powi(2)).sqrt();
        if (d - r).abs() <= cad_primitives::TAU_WORK {
            return true;
        }
    }
    // Even-odd parity: +x-ray crossings over segments (half-open on each
    // segment's y-range so shared loop vertices count once) + one toggle
    // per enclosing circle.
    let mut inside = false;
    for &(a, b) in segs {
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let xi = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if xi > p[0] {
                inside = !inside;
            }
        }
    }
    for &(cc, r) in circles {
        let d = ((p[0] - cc[0]).powi(2) + (p[1] - cc[1]).powi(2)).sqrt();
        if d < r {
            inside = !inside;
        }
    }
    inside
}

/// Increment-2 entry point: both operands' rim junction maps against each
/// other (wired in `boolean()` behind the no-Stage-0-interaction scope
/// gate; spec branch table row 3 records the pass-through trap that gate
/// avoids).
pub(crate) fn rim_junction_overrides(
    a: &BRep,
    b: &BRep,
) -> (
    std::collections::BTreeMap<u32, Vec<Point3>>,
    std::collections::BTreeMap<u32, Vec<Point3>>,
) {
    (rim_junctions_against(a, b), rim_junctions_against(b, a))
}
