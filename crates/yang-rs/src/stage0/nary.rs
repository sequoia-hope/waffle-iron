//! M8 plane-grouped n-ary coplanar overlay (specs
//! `specs/m8_plane_group_nary_overlay.md` (slice f, task #129) +
//! `specs/m8_nary_tessellated_faces.md` (slice g, task #132)).
//!
//! `stage0_preprocess` used to wall any face participating in MORE than one
//! near-coplanar cross pair (`multi-pair` residue). The driver is the user
//! case `error_coplanar.waffle`: a bridge slab whose bottom face is flush
//! with BOTH tower tops of a U-shaped solid — two pairs sharing one B face,
//! two disjoint overlap regions on one plane.
//!
//! Pairs are grouped into PLANE GROUPS — connected components of the pair
//! graph, joined by a shared face (all pairs of one component necessarily
//! lie on the shared face's plane). A singleton group runs the existing
//! 1×1 path byte-identically; a multi-pair group runs ONE n-ary exact
//! overlay ([`coplanar_overlay_multi`]) — side A = the group's A faces,
//! side B = its B faces — so the §4.5.5 "three parts" segmentation is
//! computed once, set-level, per plane ([#24 Yang 2025 §4.5.5, Fig. 16]:
//! the A-only / B-only / overlap regions are regions OF THE PLANE, not
//! per-pair artifacts).
//!
//! ## Scope
//!
//! Slice f handled pure all-`LineSegment` planar faces; slice g extends the
//! group to the tessellated classes the 1×1 path supports — DISC, ANNULAR,
//! and MIXED Line+Arc faces — by wiring the same per-face machinery: exact
//! Stage-1 rim rings ([`face_polygon_2d_tessellated`]), rim-aware
//! clustering, on-circle chord mints ([`rim_chord_ctxs`] /
//! [`mixed_chord_ctxs`] with ALL other-side polygons), the sub-floor
//! shared-mint collapse, a reduced fold gate (attribution-constrained flips
//! plus mint revert — the amendment-5/6 cavity relocation is NOT wired; an
//! unflippable fold reverts, observable via kernel-v2's vertex-on-surface
//! tripwire), and per-face lateral crossing propagation
//! ([`collect_rim_crossings`] / [`collect_mixed_crossings`]).

#[allow(clippy::wildcard_imports)]
use super::*;
use crate::coplanar_overlay::coplanar_overlay_multi;
use crate::CrossCoplanarPair;
use dashu::rational::RBig;

/// One plane group: a connected component of the cross-pair graph (pairs
/// joined by a shared face). `pair_idxs` ascend (scan order); `faces_a` /
/// `faces_b` are the distinct participating faces, ascending.
pub(crate) struct PlaneGroup {
    pub(crate) pair_idxs: Vec<usize>,
    pub(crate) faces_a: Vec<usize>,
    pub(crate) faces_b: Vec<usize>,
}

/// Group cross pairs into plane groups (connected components over shared
/// faces). Deterministic: union-find with path compression, components
/// ordered by their smallest pair index.
pub(crate) fn build_plane_groups(cross: &[CrossCoplanarPair]) -> Vec<PlaneGroup> {
    let mut parent: Vec<usize> = (0..cross.len()).collect();
    fn find(parent: &mut Vec<usize>, i: usize) -> usize {
        if parent[i] != i {
            let r = find(parent, parent[i]);
            parent[i] = r;
        }
        parent[i]
    }
    // Union pairs sharing an A face or a B face.
    let mut by_face_a: BTreeMap<usize, usize> = BTreeMap::new();
    let mut by_face_b: BTreeMap<usize, usize> = BTreeMap::new();
    for (i, p) in cross.iter().enumerate() {
        for (map, key) in [(&mut by_face_a, p.face_a), (&mut by_face_b, p.face_b)] {
            if let Some(&j) = map.get(&key) {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    // Attach the larger root to the smaller so the
                    // component root is its smallest pair index.
                    let (lo, hi) = if ri < rj { (ri, rj) } else { (rj, ri) };
                    parent[hi] = lo;
                }
            } else {
                map.insert(key, i);
            }
        }
    }
    let mut groups: BTreeMap<usize, PlaneGroup> = BTreeMap::new();
    for (i, p) in cross.iter().enumerate() {
        let r = find(&mut parent, i);
        let g = groups.entry(r).or_insert_with(|| PlaneGroup {
            pair_idxs: Vec::new(),
            faces_a: Vec::new(),
            faces_b: Vec::new(),
        });
        g.pair_idxs.push(i);
        if !g.faces_a.contains(&p.face_a) {
            g.faces_a.push(p.face_a);
        }
        if !g.faces_b.contains(&p.face_b) {
            g.faces_b.push(p.face_b);
        }
    }
    let mut out: Vec<PlaneGroup> = groups.into_values().collect();
    for g in &mut out {
        g.faces_a.sort_unstable();
        g.faces_b.sort_unstable();
    }
    out
}

/// One side's collected per-face polygon data for the group overlay.
struct SidePolys {
    /// In-frame polygons, index-parallel with the group's face list.
    polys: Vec<PolygonWithHoles>,
    /// Mixed-face curved sub-chord masks per face (empty ⇔ not mixed).
    masks: Vec<Vec<Vec<Option<u32>>>>,
    /// Merged corner key → solid vertex index map (first insertion wins —
    /// bit-equal in-plane keys of one solid resolve to one snapped point).
    corners: BTreeMap<ExactPoint2, u32>,
    /// Merged rim key → exact 3D rim point map (disc/annular rings, mixed
    /// chain Steiner samples).
    rims: BTreeMap<ExactPoint2, Point3>,
}

/// Is any ring of this face's polygon subdivided by an overlay vertex —
/// outer ring AND hole rings (the annular generalization of
/// [`rim_subdivided`], which examines the outer ring only)?
fn any_ring_subdivided(poly: &PolygonWithHoles, overlay: &ClassifiedOverlay) -> bool {
    if rim_subdivided(poly, overlay) {
        return true;
    }
    poly.holes.iter().any(|h| {
        let hp = PolygonWithHoles {
            outer: h.clone(),
            holes: Vec::new(),
        };
        rim_subdivided(&hp, overlay)
    })
}

/// Run the n-ary overlay for one multi-pair plane group: snap already done
/// by the caller (group frame), this emits the group's `PairPlane`s,
/// per-face override triangulations, boundary-edge splits, and — for rim
/// crossings — lateral rim overrides.
///
/// On any scope violation returns the loud typed pair error for the group's
/// FIRST pair (probe tags under `YANG_COPLANAR_PROBE=1`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn overlay_nary_group(
    a: &BRep,
    b: &BRep,
    group: &PlaneGroup,
    cross: &[CrossCoplanarPair],
    frame: &Frame,
    va: &[Point3],
    vb: &[Point3],
    pairs: &mut Vec<PairPlane>,
    overrides_a: &mut BTreeMap<usize, Vec<[Point3; 3]>>,
    overrides_b: &mut BTreeMap<usize, Vec<[Point3; 3]>>,
    splits_a: &mut SplitMap,
    splits_b: &mut SplitMap,
    rim_overrides_a: &mut RimSplitMap,
    rim_overrides_b: &mut RimSplitMap,
    probe: &dyn Fn(&str, &str),
) -> Result<(), YangError> {
    let first = &cross[group.pair_idxs[0]];
    let pair_err = || YangError::CoplanarFacesUnsupported {
        input_a: InputId::A,
        face_a: first.face_a,
        input_b: InputId::B,
        face_b: first.face_b,
    };
    let group_tag = || {
        format!(
            "pairs={:?} faces_a={:?} faces_b={:?}",
            group.pair_idxs, group.faces_a, group.faces_b
        )
    };

    // ── Scope gates (slice g spec B3/B4) ────────────────────────────────
    // Admission = the 1×1 classes (`overlay_face_supported`: line / disc /
    // annular / mixed). `stage0_preprocess` pre-walls anything outside them
    // per pair, so this is defensive; census every offender for the probe.
    let mut census = String::new();
    for (brep, faces, side) in [(a, &group.faces_a, "A"), (b, &group.faces_b, "B")] {
        for &fi in faces.iter() {
            if !overlay_face_supported(brep, fi) {
                census.push_str(&format!(
                    " {side}{fi} hist[{}]",
                    face_curve_histogram(brep, fi)
                ));
            }
        }
    }
    if !census.is_empty() {
        probe("nary-face-unsupported", &format!("{}{census}", group_tag()));
        return Err(pair_err());
    }
    // B6: disc rim × annular hole rim strict crossings stay the loud 1×1
    // wall, applied pairwise across the group's cross pairs.
    for &pi in &group.pair_idxs {
        let p = &cross[pi];
        if annular_hole_rim_crossing(a, p.face_a, b, p.face_b)
            || annular_hole_rim_crossing(b, p.face_b, a, p.face_a)
        {
            probe(
                "annular-hole-rim-crossing",
                &format!("nary pair=({},{})", p.face_a, p.face_b),
            );
            return Err(pair_err());
        }
    }
    // Per-side uniform orientation vs the group's canonical normal. Side A
    // must AGREE with the frame (it was derived from an A face); side B's
    // sign is the group's `opposite` flag.
    let face_dot = |brep: &BRep, fi: usize| -> f64 {
        let Surface::Plane { normal, .. } = brep.faces()[fi].surface else {
            unreachable!("validated planar above");
        };
        let n = normalize3(normal.as_array());
        frame.n[0] * n[0] + frame.n[1] * n[1] + frame.n[2] * n[2]
    };
    // Task #147 (slice h): a plane group whose side-A faces have MIXED
    // orientation vs the frame — some agree (+n̂), some oppose (−n̂) — is a
    // VALID non-convex solid, NOT a defect: opposite-normal coplanar faces
    // must occupy 2D-DISJOINT regions of the plane (overlapping ones would be
    // a zero-thickness membrane, which a valid manifold cannot expose). The
    // exact overlay classifies coverage winding-INDEPENDENTLY (module contract
    // "outer/hole winding direction is irrelevant"), so its A-only / overlap
    // partition and per-triangle `poly_a` attribution are already correct for
    // both orientations. The ONLY orientation-dependent step is the per-A-face
    // override winding: an overlay triangle is CCW in the frame (⇒ +n̂), so a
    // face whose outward normal is −n̂ must SWAP, exactly as a B face does when
    // it opposes the frame. `face_swap_a` below computes that per face. For a
    // uniform +n̂ group (every currently-supported case) `face_dot > 0` for all
    // A faces ⇒ `swap == false` everywhere ⇒ byte-identical to the historical
    // path. So this admits mixed orientation with ZERO change to uniform
    // groups.
    let face_swap_a = |fa: usize| face_dot(a, fa) < 0.0;
    let opposite = face_dot(b, group.faces_b[0]) < 0.0;
    if group
        .faces_b
        .iter()
        .any(|&fi| (face_dot(b, fi) < 0.0) != opposite)
    {
        probe("nary-mixed-orientation", &group_tag());
        return Err(pair_err());
    }

    // ── PairPlane emission (one per scan pair, group frame + opposite) ──
    for &pi in &group.pair_idxs {
        let p = &cross[pi];
        pairs.push(PairPlane {
            n: frame.n,
            d: frame.d,
            band: p.band,
            face_a: p.face_a,
            face_b: p.face_b,
            opposite,
        });
    }

    // ── Shared-frame 2D polygons per face (tessellated: exact rim rings /
    // spliced mixed chains; corner + rim key maps merged per side) ──────
    let mut side_a = SidePolys {
        polys: Vec::with_capacity(group.faces_a.len()),
        masks: Vec::with_capacity(group.faces_a.len()),
        corners: BTreeMap::new(),
        rims: BTreeMap::new(),
    };
    let mut side_b = SidePolys {
        polys: Vec::with_capacity(group.faces_b.len()),
        masks: Vec::with_capacity(group.faces_b.len()),
        corners: BTreeMap::new(),
        rims: BTreeMap::new(),
    };
    for (brep, faces, verts, side, tag) in [
        (a, &group.faces_a, va, &mut side_a, "nary-polygon2d-a"),
        (b, &group.faces_b, vb, &mut side_b, "nary-polygon2d-b"),
    ] {
        for &fi in faces.iter() {
            let Some((poly, c, rim, mask)) = face_polygon_2d_tessellated(brep, fi, verts, frame)
            else {
                probe(tag, &format!("{} face={fi}", group_tag()));
                return Err(pair_err());
            };
            side.polys.push(poly);
            side.masks.push(mask);
            for (k, v) in c {
                side.corners.entry(k).or_insert(v);
            }
            for (k, v) in rim {
                side.rims.entry(k).or_insert(v);
            }
        }
    }

    // ── §2b/§2c in-frame coordinate clustering across the WHOLE group —
    // the same femto-reconciliation the 1×1 path applies, rim-aware: rim
    // sample coordinates are excluded from the cluster domain entirely.
    // Corner/rim keys remap through the pre→post map. ────────────────────
    let band = group
        .pair_idxs
        .iter()
        .map(|&pi| cross[pi].band)
        .fold(0.0_f64, f64::max);
    let rim_pts_a: Vec<Point2> = side_a
        .rims
        .keys()
        .map(|ex| Point2::new(ex.x.to_f64().value(), ex.y.to_f64().value()))
        .collect();
    let rim_pts_b: Vec<Point2> = side_b
        .rims
        .keys()
        .map(|ex| Point2::new(ex.x.to_f64().value(), ex.y.to_f64().value()))
        .collect();
    let pre: Vec<PolygonWithHoles> = side_a
        .polys
        .iter()
        .chain(side_b.polys.iter())
        .cloned()
        .collect();
    {
        let mut refs: Vec<&mut PolygonWithHoles> = side_a
            .polys
            .iter_mut()
            .chain(side_b.polys.iter_mut())
            .collect();
        cluster_frame_coords_rim_aware(
            &mut refs,
            &[rim_pts_a.as_slice(), rim_pts_b.as_slice()],
            band,
        );
    }
    let mut cluster_map: BTreeMap<(u64, u64), (u64, u64)> = BTreeMap::new();
    for (pre_p, post_p) in pre
        .iter()
        .zip(side_a.polys.iter().chain(side_b.polys.iter()))
    {
        for (lp_pre, lp_post) in std::iter::once(&pre_p.outer)
            .chain(pre_p.holes.iter())
            .zip(std::iter::once(&post_p.outer).chain(post_p.holes.iter()))
        {
            for (q_pre, q_post) in lp_pre.iter().zip(lp_post.iter()) {
                cluster_map.insert(
                    (q_pre.x().to_bits(), q_pre.y().to_bits()),
                    (q_post.x().to_bits(), q_post.y().to_bits()),
                );
            }
        }
    }
    let remap_exact = |ex: ExactPoint2| -> ExactPoint2 {
        let ux = ex.x.to_f64().value();
        let vy = ex.y.to_f64().value();
        match cluster_map.get(&(ux.to_bits(), vy.to_bits())) {
            Some(&(nx, ny)) => {
                ExactPoint2::from_f64(f64::from_bits(nx), f64::from_bits(ny)).unwrap_or(ex)
            }
            None => ex,
        }
    };
    let corners_a: BTreeMap<ExactPoint2, u32> = std::mem::take(&mut side_a.corners)
        .into_iter()
        .map(|(k, v)| (remap_exact(k), v))
        .collect();
    let corners_b: BTreeMap<ExactPoint2, u32> = std::mem::take(&mut side_b.corners)
        .into_iter()
        .map(|(k, v)| (remap_exact(k), v))
        .collect();
    let rims_a: BTreeMap<ExactPoint2, Point3> = std::mem::take(&mut side_a.rims)
        .into_iter()
        .map(|(k, v)| (remap_exact(k), v))
        .collect();
    let rims_b: BTreeMap<ExactPoint2, Point3> = std::mem::take(&mut side_b.rims)
        .into_iter()
        .map(|(k, v)| (remap_exact(k), v))
        .collect();

    // ── The n-ary exact overlay ─────────────────────────────────────────
    let mut overlay: ClassifiedOverlay = match coplanar_overlay_multi(&side_a.polys, &side_b.polys)
    {
        Ok(o) => o,
        Err(e) => {
            probe("nary-overlay-failed", &format!("{} err={e:?}", group_tag()));
            return Err(pair_err());
        }
    };

    if overlay.area_exact(RegionClass::Overlap) == RBig::ZERO {
        // No positive-area overlap anywhere in the group (in-plane touch):
        // the snap has already reconciled the planes; all faces tessellate
        // normally (cherchi deviation N17 passes the touch through).
        return Ok(());
    }

    // ── Crossing survey + N2-3a mint contexts per face (slice g): one ctx
    // vector per face, other-side segments = ALL other-side polygons. ────
    let mut any_same_normal_cross = false;
    let mut ctxs: Vec<RimChordCtx> = Vec::new();
    // (side, face-list index, ctx range) per face, for the collectors below.
    for (brep, faces, side, other_polys) in [
        (a, &group.faces_a, &side_a, &side_b.polys),
        (b, &group.faces_b, &side_b, &side_a.polys),
    ] {
        for (idx, &fi) in faces.iter().enumerate() {
            let poly = &side.polys[idx];
            let mask = &side.masks[idx];
            if !mask.is_empty() {
                if curved_chords_subdivided(poly, mask, &overlay) && !opposite {
                    any_same_normal_cross = true;
                }
                ctxs.extend(mixed_chord_ctxs(brep, poly, mask, other_polys, frame));
            } else {
                if any_ring_subdivided(poly, &overlay)
                    && !opposite
                    && (disc_circle_edge(brep, fi).is_some()
                        || annular_disc_face(brep, fi).is_some())
                {
                    any_same_normal_cross = true;
                }
                ctxs.extend(rim_chord_ctxs(brep, fi, poly, other_polys, frame));
            }
        }
    }
    if any_same_normal_cross {
        // Survey probe (the 1×1 `disc-crossing-same-normal` analog).
        probe("nary-crossing-same-normal", &group_tag());
    }
    let n_mint_slots = ctxs.len();

    // Tessellated rim points (f64 in-frame u,v → 3D) for the near-snap: a
    // curved rim fed through the exact overlay can spawn a sweep vertex a
    // few ULPs off a rim point; snap such a vertex to the exact rim point
    // it is essentially on (1×1 path, byte-identical rationale).
    let rim_pts: Vec<(f64, f64, Point3)> = rims_a
        .iter()
        .chain(rims_b.iter())
        .map(|(ex, &pt)| (ex.x.to_f64().value(), ex.y.to_f64().value(), pt))
        .collect();
    let snap_eps2 = {
        let scale = rim_pts
            .iter()
            .map(|(u, v, _)| u.abs().max(v.abs()))
            .fold(1.0_f64, f64::max);
        let e = 1.0e-9 * scale;
        e * e
    };

    // ── Resolve overlay vertices to shared 3D points: corners → exact rim
    // points → rim ULP-snap → on-circle chord mints → frame lift. ────────
    let mut coords: Vec<Point3> = Vec::with_capacity(overlay.verts.len());
    let mut minted_mark = vec![false; overlay.verts.len()];
    let mut minted_info: Vec<(usize, usize, bool)> = Vec::new();
    for (i, mark) in minted_mark.iter_mut().enumerate() {
        let exact = &overlay.exact_verts[i];
        let pt = if let Some(&ai) = corners_a.get(exact) {
            va[ai as usize]
        } else if let Some(&bi) = corners_b.get(exact) {
            vb[bi as usize]
        } else if let Some(&pt) = rims_a.get(exact) {
            pt
        } else if let Some(&pt) = rims_b.get(exact) {
            pt
        } else {
            let q = overlay.verts[i];
            let (qx, qy) = (q.x(), q.y());
            if let Some(&(_, _, pt)) = rim_pts.iter().find(|(u, v, _)| {
                let (du, dv) = (u - qx, v - qy);
                du * du + dv * dv <= snap_eps2
            }) {
                pt
            } else {
                let mut minted: Option<Point3> = None;
                for (slot, ctx) in ctxs.iter().enumerate() {
                    match resolve_rim_chord_vertex(ctx, exact, qx, qy, frame) {
                        RimResolve::NotOnChord => {}
                        RimResolve::OnCircle { point, crossing } => {
                            minted = Some(point);
                            minted_info.push((i, slot, crossing));
                            break;
                        }
                        RimResolve::NoIntersection => {
                            // The exact discriminant says the other input's
                            // edge line misses the circle — impossible for a
                            // genuine crossing. Loud Stage-0 stop.
                            probe(
                                "nary-rim-circle-line-no-intersection",
                                &format!("{} vert={i} uv=({qx},{qy})", group_tag()),
                            );
                            return Err(pair_err());
                        }
                    }
                }
                *mark = minted.is_some();
                minted.unwrap_or_else(|| frame.lift(qx, qy))
            }
        };
        coords.push(pt);
    }

    // ── Sub-floor shared-mint collapse (spec `m8_holed_disc_coplanar_
    // overlay` §8; slot space = every rim circle of the group). ──────────
    for slot in 0..n_mint_slots {
        let members: Vec<(usize, bool)> = minted_info
            .iter()
            .filter(|&&(_, s, _)| s == slot)
            .map(|&(vi, _, crossing)| (vi, crossing))
            .collect();
        let mut groups2: Vec<Vec<(usize, bool)>> = Vec::new();
        for &(vi, crossing) in &members {
            let p = coords[vi].as_array();
            let g = groups2.iter_mut().find(|g| {
                let q = coords[g[0].0].as_array();
                let d = [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
                d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
                    < cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE
            });
            match g {
                Some(g) => g.push((vi, crossing)),
                None => groups2.push(vec![(vi, crossing)]),
            }
        }
        for g in groups2.iter().filter(|g| g.len() > 1) {
            let target_vi = g.iter().find(|&&(_, c)| c).map_or(g[0].0, |&(vi, _)| vi);
            let target = coords[target_vi];
            for &(vi, _) in g {
                coords[vi] = target;
            }
        }
    }

    // ── Reduced fold gate (slice g spec B8): amendment-4 flips constrained
    // to same (class, attribution) + amendment-2 mint revert. An edge
    // between triangles of DIFFERENT input polygons is a face boundary —
    // as immovable as a class boundary (flipping it would move area between
    // faces). No cavity relocation in this slice: an unflippable fold
    // reverts its mints to the chord lift (observable downstream via
    // kernel-v2's vertex-on-surface tripwire — P9-loud, never blessed). ──
    let tri_area = |t: &[u32; 3], coords: &[Point3]| gate_tri_area(t, coords, frame);
    let tri_valid = |t: &[u32; 3], coords: &[Point3]| gate_tri_valid(t, coords, frame);
    let edge_key = |x: u32, y: u32| if x < y { [x, y] } else { [y, x] };
    let mut edge_map: BTreeMap<[u32; 2], Vec<usize>> = BTreeMap::new();
    for (ti, t) in overlay.tris.iter().enumerate() {
        for k in 0..3 {
            edge_map
                .entry(edge_key(t[k], t[(k + 1) % 3]))
                .or_default()
                .push(ti);
        }
    }
    loop {
        let mut changed = false;
        for ti in 0..overlay.tris.len() {
            let t = overlay.tris[ti];
            if gate_tri_degenerate(&t, &coords) || tri_area(&t, &coords) > 0.0 {
                continue;
            }
            if !t.iter().any(|&v| minted_mark[v as usize]) {
                continue;
            }
            // Constrained flip (amendment 4 + attribution constraint).
            let mut flipped = false;
            for k in 0..3 {
                let (ea, eb) = (t[k], t[(k + 1) % 3]);
                let c = t[(k + 2) % 3];
                let Some(inc) = edge_map.get(&edge_key(ea, eb)) else {
                    continue;
                };
                if inc.len() != 2 {
                    continue; // domain boundary
                }
                let tj = if inc[0] == ti { inc[1] } else { inc[0] };
                if overlay.class[tj] != overlay.class[ti]
                    || overlay.poly_a[tj] != overlay.poly_a[ti]
                    || overlay.poly_b[tj] != overlay.poly_b[ti]
                {
                    continue; // class or face-attribution boundary
                }
                let tn = overlay.tris[tj];
                if gate_tri_degenerate(&tn, &coords) {
                    continue;
                }
                let Some(d) = tn.iter().copied().find(|&v| v != ea && v != eb) else {
                    continue;
                };
                if edge_map.contains_key(&edge_key(c, d)) {
                    continue; // diagonal exists
                }
                let n1 = [ea, d, c];
                let n2 = [d, eb, c];
                if !tri_valid(&n1, &coords) || !tri_valid(&n2, &coords) {
                    continue;
                }
                for (idx, old) in [(ti, t), (tj, tn)] {
                    for k2 in 0..3 {
                        let kk = edge_key(old[k2], old[(k2 + 1) % 3]);
                        if let Some(v) = edge_map.get_mut(&kk) {
                            v.retain(|&x| x != idx);
                            if v.is_empty() {
                                edge_map.remove(&kk);
                            }
                        }
                    }
                }
                overlay.tris[ti] = n1;
                overlay.tris[tj] = n2;
                for (idx, newt) in [(ti, n1), (tj, n2)] {
                    for k2 in 0..3 {
                        edge_map
                            .entry(edge_key(newt[k2], newt[(k2 + 1) % 3]))
                            .or_default()
                            .push(idx);
                    }
                }
                flipped = true;
                changed = true;
                break;
            }
            if flipped {
                continue;
            }
            // Amendment-2 fallback: revert the fold's minted vertices to
            // the chord lift.
            for &v in &t {
                let vi = v as usize;
                if minted_mark[vi] {
                    let q = overlay.verts[vi];
                    let lifted = frame.lift(q.x(), q.y());
                    if coords[vi] != lifted {
                        probe(
                            "nary-fold-revert",
                            &format!(
                                "v={vi} exact={:?} chord_lift={:?}",
                                coords[vi].as_array(),
                                lifted.as_array()
                            ),
                        );
                        coords[vi] = lifted;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }

    // ── Per-face override triangulations (attribution-scoped tris_for).
    // Overlay triangles are CCW in the frame ⇒ normal +n̂: a side-A face keeps
    // the order iff its outward normal IS +n̂ (`face_swap_a`, task #147 — a −n̂
    // face swaps, like an opposing B face); side-B faces swap iff the group
    // opposes. The M-B degenerate-3D-image filter mirrors the 1×1 path
    // (femto-split 2D verts resolved to one exact point).
    let tris_for =
        |keep: [RegionClass; 2], attribution: &[u32], idx: u32, swap: bool| -> Vec<[Point3; 3]> {
            let bits = |p: Point3| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
            overlay
                .tris
                .iter()
                .zip(&overlay.class)
                .zip(attribution)
                .filter(|((_, c), &attr)| keep.contains(c) && attr == idx)
                .filter_map(|((t, _), _)| {
                    let mut tri = [
                        coords[t[0] as usize],
                        coords[t[1] as usize],
                        coords[t[2] as usize],
                    ];
                    let bb = [bits(tri[0]), bits(tri[1]), bits(tri[2])];
                    if bb[0] == bb[1] || bb[1] == bb[2] || bb[0] == bb[2] {
                        return None;
                    }
                    if swap {
                        tri.swap(1, 2);
                    }
                    Some(tri)
                })
                .collect()
        };
    for (idx, &fa) in group.faces_a.iter().enumerate() {
        overrides_a.insert(
            fa,
            tris_for(
                [RegionClass::AOnly, RegionClass::Overlap],
                &overlay.poly_a,
                idx as u32,
                // Task #147: −n̂ A faces swap, exactly like an opposing B face.
                face_swap_a(fa),
            ),
        );
    }
    for (idx, &fb) in group.faces_b.iter().enumerate() {
        overrides_b.insert(
            fb,
            tris_for(
                [RegionClass::BOnly, RegionClass::Overlap],
                &overlay.poly_b,
                idx as u32,
                opposite,
            ),
        );
    }

    // Emission sanity (defensive, loud): a face whose override dropped to
    // ZERO triangles while its polygon has positive area would tear the
    // shell — cannot happen for valid inputs (coverage identity), so any
    // occurrence is a bug surfaced immediately, not downstream.
    for (faces, overrides) in [
        (&group.faces_a, &*overrides_a),
        (&group.faces_b, &*overrides_b),
    ] {
        for &fi in faces.iter() {
            if overrides.get(&fi).is_some_and(|t| t.is_empty()) {
                probe("nary-empty-override", &format!("{} face={fi}", group_tag()));
                return Err(pair_err());
            }
        }
    }
    // ── §4.5.5 shared boundary sampling: overlay vertices subdividing a
    // face's boundary edges propagate into the adjacent faces (existing
    // per-face collector; `used` spans the whole side, the exact
    // on-open-segment test scopes splits to each face's own edges). ─────
    for &fa in &group.faces_a {
        collect_edge_splits(
            a,
            fa,
            va,
            frame,
            &cluster_map,
            &overlay,
            [RegionClass::AOnly, RegionClass::Overlap],
            &coords,
            splits_a,
        );
    }
    for &fb in &group.faces_b {
        collect_edge_splits(
            b,
            fb,
            vb,
            frame,
            &cluster_map,
            &overlay,
            [RegionClass::BOnly, RegionClass::Overlap],
            &coords,
            splits_b,
        );
    }

    // ── Rim / mixed crossing propagation into the laterals (slice g spec
    // B9): a disc/annular face whose rings the overlap boundary subdivided
    // routes each crossing into its own rim + the cylinder's opposite rim;
    // a mixed face routes per curved edge. Collector failures (e.g. a
    // torus-profile rim's `rim-lateral-none`) stay the loud pair error. ──
    for (brep, faces, side, rim_overrides) in [
        (a, &group.faces_a, &side_a, &mut *rim_overrides_a),
        (b, &group.faces_b, &side_b, &mut *rim_overrides_b),
    ] {
        for (idx, &fi) in faces.iter().enumerate() {
            let poly = &side.polys[idx];
            let mask = &side.masks[idx];
            let res = if !mask.is_empty() {
                if !curved_chords_subdivided(poly, mask, &overlay) {
                    continue;
                }
                collect_mixed_crossings(brep, fi, poly, mask, &overlay, &coords, rim_overrides)
            } else {
                if disc_circle_edge(brep, fi).is_none() && annular_disc_face(brep, fi).is_none() {
                    continue;
                }
                if !any_ring_subdivided(poly, &overlay) {
                    continue;
                }
                collect_rim_crossings(brep, fi, poly, &overlay, &coords, rim_overrides)
            };
            if let Err(tag) = res {
                probe(tag, &format!("{} face={fi}", group_tag()));
                return Err(pair_err());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage0::stage0_preprocess;
    use crate::tests_unit::n2_junction::{rj_box, rj_cylinder};
    use cad_primitives::BoolOp;
    use std::collections::BTreeSet;

    /// Task #133 regression (spec `yang_stage6_arc_orientation`): the
    /// partial-depth pocket operand's Stage-0 emission is watertight. Was
    /// ~92 unbalanced edges along the split z=1 arcs — the CW-traversing
    /// per-face arc copies declared the COMPLEMENTARY (≈2π) arcs before
    /// `orient_directed_curve` fixed the Stage-6 emission.
    #[test]
    fn t133_pocket_floor_emission_watertight() {
        let nb = crate::native_backend().expect("native backend");
        let cyl = rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 2.0, 2.0);
        let channel = rj_box([-0.5, -3.0, 1.0], [0.5, 3.0, 3.0]);
        let solid = crate::boolean(&cyl, &channel, BoolOp::Subtract, &nb).expect("cyl − channel");
        let va: Vec<Point3> = solid.vertices().iter().map(|v| v.point).collect();
        let (mesh, _) = build_stage0_mesh(
            &solid,
            &va,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap_or_else(|_| panic!("emission failed"));
        let key = |v: u32| {
            let p = mesh.verts[v as usize];
            [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]
        };
        let mut m: BTreeMap<([u64; 3], [u64; 3]), (usize, i64)> = BTreeMap::new();
        for t in &mesh.tris {
            for k in 0..3 {
                let (a, b) = (key(t[k]), key(t[(k + 1) % 3]));
                let (lo, hi, dir) = if a <= b { (a, b, 1) } else { (b, a, -1) };
                let e = m.entry((lo, hi)).or_insert((0, 0));
                e.0 += 1;
                e.1 += dir;
            }
        }
        let bad = m.values().filter(|&&(c, bal)| c != 2 || bal != 0).count();
        assert_eq!(bad, 0, "pocket operand Stage-0 emission must be watertight");
    }

    /// Slice g structural oracle (spec `m8_nary_tessellated_faces` §5):
    /// the {mixed, mixed} × {disc} flush-pocket group emits watertight
    /// Stage-0 meshes, and every tool-cap OVERLAP triangle is bit-identical
    /// between the two solids' meshes (I3 — §4.5.5 identical overlap
    /// meshes; a broken rim-ring reuse or attribution filter tears this).
    #[test]
    fn nary_tessellated_group_stage0_meshes() {
        let nb = crate::native_backend().expect("native backend");
        let cyl = rj_cylinder([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 2.0, 2.0);
        let channel = rj_box([-0.5, -3.0, -1.0], [0.5, 3.0, 3.0]);
        let solid = crate::boolean(&cyl, &channel, BoolOp::Subtract, &nb).expect("cyl − channel");
        let tool = rj_cylinder([0.0, 0.0, 1.5], [0.0, 0.0, 1.0], 1.0, 0.5);
        let s0 = stage0_preprocess(&solid, &tool)
            .expect("group handled")
            .expect("pairs detected");
        assert!(
            s0.pairs.len() >= 2,
            "tool cap must be in two pairs, got {}",
            s0.pairs.len()
        );

        let tri_keys = |mesh: &crate::Mesh, keep: &dyn Fn([f64; 3], [f64; 3], [f64; 3]) -> bool| {
            let mut set: BTreeSet<Vec<[u64; 3]>> = BTreeSet::new();
            for t in &mesh.tris {
                let ps: Vec<[f64; 3]> = t
                    .iter()
                    .map(|&v| mesh.verts[v as usize].as_array())
                    .collect();
                if !keep(ps[0], ps[1], ps[2]) {
                    continue;
                }
                let mut key: Vec<[u64; 3]> = ps
                    .iter()
                    .map(|p| [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()])
                    .collect();
                key.sort_unstable();
                set.insert(key);
            }
            set
        };

        // Watertightness of both emitted meshes.
        for (mesh, tag) in [(&s0.mesh_a, "mesh_a"), (&s0.mesh_b, "mesh_b")] {
            let key = |v: u32| {
                let p = mesh.verts[v as usize];
                [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]
            };
            let mut m: BTreeMap<([u64; 3], [u64; 3]), (usize, i64)> = BTreeMap::new();
            for t in &mesh.tris {
                for k in 0..3 {
                    let (a, b) = (key(t[k]), key(t[(k + 1) % 3]));
                    let (lo, hi, dir) = if a <= b { (a, b, 1) } else { (b, a, -1) };
                    let e = m.entry((lo, hi)).or_insert((0, 0));
                    e.0 += 1;
                    e.1 += dir;
                }
            }
            let bad = m.values().filter(|&&(c, bal)| c != 2 || bal != 0).count();
            assert_eq!(bad, 0, "{tag}: Stage-0 mesh must be watertight");
        }

        // I3: tool-cap OVERLAP triangles (z=2, centroid over a cap piece —
        // |x| > channel half-width) appear bit-identically in BOTH meshes.
        let on_plane_overlap = |p0: [f64; 3], p1: [f64; 3], p2: [f64; 3]| {
            let on_plane = p0[2] == 2.0 && p1[2] == 2.0 && p2[2] == 2.0;
            let cx = (p0[0] + p1[0] + p2[0]) / 3.0;
            let cr = {
                let cy = (p0[1] + p1[1] + p2[1]) / 3.0;
                (cx * cx + cy * cy).sqrt()
            };
            // Inside the tool disc (r=1) AND over a cap piece (|x| > 0.5).
            on_plane && cr < 1.0 && cx.abs() > 0.5
        };
        let a_set = tri_keys(&s0.mesh_a, &on_plane_overlap);
        let b_set = tri_keys(&s0.mesh_b, &on_plane_overlap);
        assert!(
            !b_set.is_empty(),
            "tool cap must carry overlap triangles over the cap pieces"
        );
        for key in &b_set {
            assert!(
                a_set.contains(key),
                "overlap triangle {key:?} in mesh_b missing from mesh_a (I3)"
            );
        }
    }

    /// Stage-0-level attribution oracle (FIP §6.3 mutation check: the
    /// mesh-level e2e oracles are INSENSITIVE to a dropped/swapped
    /// attribution filter — downstream duplicate welding + same-plane patch
    /// merge mask it — so the structural contract is pinned HERE):
    /// (1) the emitted Stage-0 meshes carry NO duplicate triangle (a dropped
    ///     filter emits every group triangle once per side face);
    /// (2) every mesh-A triangle attributed to a tower-top face lies within
    ///     THAT face's in-plane extent (a swapped filter crosses the gap).
    #[test]
    fn nary_overrides_are_disjoint_and_owned() {
        let nb = crate::native_backend().expect("native backend");
        let base = rj_box([-1.5, -0.5, 0.0], [1.5, 0.5, 0.2]);
        let ta = rj_box([-1.2, -0.4, 0.2], [-0.4, 0.4, 1.2]);
        let tb = rj_box([0.4, -0.4, 0.2], [1.2, 0.4, 1.2]);
        let u1 = crate::boolean(&base, &ta, BoolOp::Union, &nb).expect("base ∪ tower A");
        let u = crate::boolean(&u1, &tb, BoolOp::Union, &nb).expect("∪ tower B");
        let bridge = rj_box([-1.0, -0.3, 1.2], [1.0, 0.3, 1.4]);

        let s0 = stage0_preprocess(&u, &bridge)
            .expect("bridge group is handled")
            .expect("near-coplanar pairs detected");
        assert!(
            s0.pairs.len() >= 2,
            "bridge bottom must be in two pairs, got {}",
            s0.pairs.len()
        );

        // (1) No duplicate position-keyed triangles in either emitted mesh.
        for (mesh, tag) in [(&s0.mesh_a, "mesh_a"), (&s0.mesh_b, "mesh_b")] {
            let mut seen: BTreeSet<Vec<[u64; 3]>> = BTreeSet::new();
            for t in &mesh.tris {
                let mut key: Vec<[u64; 3]> = t
                    .iter()
                    .map(|&v| {
                        let p = mesh.verts[v as usize];
                        [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]
                    })
                    .collect();
                key.sort_unstable();
                assert!(seen.insert(key), "{tag}: duplicate triangle {t:?}");
            }
        }

        // (2) Attribution containment: tower-top faces of U are the two +z
        // planar faces at z = 1.2; every mesh-A triangle the tri_face map
        // attributes to one must lie inside that face's x-extent.
        let tower_tops: Vec<(usize, f64, f64)> = u
            .faces()
            .iter()
            .enumerate()
            .filter_map(|(fi, f)| {
                let Surface::Plane { normal, d } = f.surface else {
                    return None;
                };
                let n = normalize3(normal.as_array());
                if n[2] < 0.99 || (d + 1.2 * n[2]).abs() > 1e-9 {
                    return None;
                }
                let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
                    for &e in lp {
                        let edge = &u.edges()[e as usize];
                        for vi in [edge.start, edge.end] {
                            let x = u.vertices()[vi as usize].point.x();
                            lo = lo.min(x);
                            hi = hi.max(x);
                        }
                    }
                }
                Some((fi, lo, hi))
            })
            .collect();
        assert_eq!(tower_tops.len(), 2, "exactly two tower tops");
        for (t, &face) in s0.mesh_a.tris.iter().zip(&s0.tri_face_a) {
            let Some(&(_, lo, hi)) = tower_tops.iter().find(|&&(fi, _, _)| fi == face as usize)
            else {
                continue;
            };
            let cx = t
                .iter()
                .map(|&v| s0.mesh_a.verts[v as usize].x())
                .sum::<f64>()
                / 3.0;
            assert!(
                cx >= lo - 1e-9 && cx <= hi + 1e-9,
                "tri attributed to face {face} (x∈[{lo},{hi}]) has centroid x={cx}"
            );
        }
    }

    /// Task #147 (slice h): a plane group whose side-A faces have MIXED
    /// orientation vs the frame is a VALID non-convex solid, admitted by the
    /// per-face override winding (`face_swap_a`). Before the fix, Stage-0
    /// walled it at `nary-mixed-orientation` → `CoplanarFacesUnsupported`.
    ///
    /// Fixture: an offset flush-stack whose z=1 boundary carries a +z face
    /// (lower top, x∈[0,1]) AND a −z face (upper bottom, x∈[2,3]) — 2D-disjoint
    /// opposite-normal coplanar faces — unioned with a B box flush at z=1 that
    /// spans both. Oracles: (1) Stage-0 no longer walls; (2) the emitted mesh
    /// is watertight (edge-balanced); (3) every mesh-A triangle attributed to
    /// the −z face is wound −n̂ (nz < 0). Mutation-killer: reverting the
    /// per-face `face_swap_a` to `false` winds the −z face +n̂ → oracle (3)
    /// fails AND the shell tears (oracle 2).
    #[test]
    fn nary_mixed_orientation_group_stage0_watertight() {
        let nb = crate::native_backend().expect("native backend");
        let lower = rj_box([0.0, 0.0, 0.0], [2.0, 1.0, 1.0]);
        let upper = rj_box([1.0, 0.0, 1.0], [3.0, 1.0, 2.0]);
        let a = crate::boolean(&lower, &upper, BoolOp::Union, &nb).expect("A = lower ∪ upper");

        // The −z coplanar face at z=1 (upper's exposed bottom).
        let neg_z_face = a
            .faces()
            .iter()
            .enumerate()
            .find(|(_, f)| {
                let Surface::Plane { normal, d } = f.surface else {
                    return false;
                };
                let n = normalize3(normal.as_array());
                n[2] < -0.99 && (d + n[2]).abs() < 1e-6
            })
            .map(|(fi, _)| fi as u32)
            .expect("A has a −z coplanar face at z=1");

        // B: a box flush at z=1 (its bottom face) spanning both A z=1 faces.
        let b = rj_box([0.5, 0.25, 1.0], [2.5, 0.75, 1.5]);

        // (1) Stage-0 admits the mixed-orientation group (was a typed wall).
        let s0 = stage0_preprocess(&a, &b)
            .expect("mixed-orientation group is admitted (no CoplanarFacesUnsupported)")
            .expect("near-coplanar pairs detected");
        assert!(
            s0.pairs.len() >= 2,
            "B bottom pairs with both A z=1 faces, got {}",
            s0.pairs.len()
        );

        // (2) The emitted Stage-0 mesh_a is watertight (every edge used twice,
        // balanced) — a −n̂ override wound the wrong way would tear it.
        let key = |mesh: &Mesh, v: u32| {
            let p = mesh.verts[v as usize];
            [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]
        };
        let mut edges: BTreeMap<([u64; 3], [u64; 3]), (usize, i64)> = BTreeMap::new();
        for t in &s0.mesh_a.tris {
            for k in 0..3 {
                let (u, v) = (key(&s0.mesh_a, t[k]), key(&s0.mesh_a, t[(k + 1) % 3]));
                let (lo, hi, dir) = if u <= v { (u, v, 1) } else { (v, u, -1) };
                let e = edges.entry((lo, hi)).or_insert((0, 0));
                e.0 += 1;
                e.1 += dir;
            }
        }
        let bad = edges
            .values()
            .filter(|&&(c, bal)| c != 2 || bal != 0)
            .count();
        assert_eq!(
            bad, 0,
            "mixed-orientation Stage-0 mesh_a must be watertight"
        );

        // (3) Every mesh-A triangle attributed to the −z face winds −n̂ (nz<0).
        let mut neg_z_tris = 0usize;
        for (t, &face) in s0.mesh_a.tris.iter().zip(&s0.tri_face_a) {
            if face != neg_z_face {
                continue;
            }
            let p = |v: u32| {
                let q = s0.mesh_a.verts[v as usize];
                [q.x(), q.y(), q.z()]
            };
            let (p0, p1, p2) = (p(t[0]), p(t[1]), p(t[2]));
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let nz = e1[0] * e2[1] - e1[1] * e2[0];
            assert!(
                nz < 0.0,
                "−z face triangle {t:?} must wind −n̂ (nz<0), got nz={nz}"
            );
            neg_z_tris += 1;
        }
        assert!(
            neg_z_tris > 0,
            "the −z coplanar face must carry override triangles in mesh_a"
        );
    }
}
