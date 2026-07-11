//! M8 plane-grouped n-ary coplanar overlay (spec
//! `specs/m8_plane_group_nary_overlay.md`, task #129).
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
//! ## Scope (branch table B2–B4 of the spec)
//!
//! Handled: groups whose faces are ALL planar with pure all-`LineSegment`
//! loops (holes allowed) and per-side uniform outward orientation. The
//! disc / annular / mixed / rim machinery of the 1×1 path is deliberately
//! NOT wired here — a multi-pair group containing such a face stays the
//! loud typed `CoplanarFacesUnsupported` residue.

use std::collections::BTreeMap;

use cad_primitives::Point3;
use dashu::rational::RBig;

use crate::coplanar_overlay::{
    coplanar_overlay_multi, ClassifiedOverlay, ExactPoint2, PolygonWithHoles, RegionClass,
};
use crate::{normalize3, BRep, CrossCoplanarPair, InputId, Surface, YangError};

use super::{
    cluster_frame_coords_rim_aware, collect_edge_splits, face_polygon_2d, mixed_planar_face,
    overlay_face_supported, Frame, PairPlane, SplitMap,
};

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

/// Is this face a PLANAR face whose loops are pure `LineSegment` edges —
/// the n-ary group's supported shape (stricter than
/// [`overlay_face_supported`]: no disc / annular / mixed admission).
fn pure_line_face(brep: &BRep, fi: usize) -> bool {
    overlay_face_supported(brep, fi)
        && super::disc_circle_edge(brep, fi).is_none()
        && super::annular_disc_face(brep, fi).is_none()
        && !mixed_planar_face(brep, fi)
}

/// Run the n-ary overlay for one multi-pair plane group: snap already done
/// by the caller (group frame), this emits the group's `PairPlane`s,
/// per-face override triangulations, and boundary-edge splits.
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

    // ── Scope gates (spec B3/B4) ────────────────────────────────────────
    for (brep, faces) in [(a, &group.faces_a), (b, &group.faces_b)] {
        for &fi in faces.iter() {
            if !pure_line_face(brep, fi) {
                probe("nary-face-unsupported", &group_tag());
                return Err(pair_err());
            }
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
    if group.faces_a.iter().any(|&fi| face_dot(a, fi) <= 0.0) {
        probe("nary-mixed-orientation", &group_tag());
        return Err(pair_err());
    }
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

    // ── Shared-frame 2D polygons (pure line loops ⇒ corner maps only) ──
    let mut polys_a: Vec<PolygonWithHoles> = Vec::with_capacity(group.faces_a.len());
    let mut polys_b: Vec<PolygonWithHoles> = Vec::with_capacity(group.faces_b.len());
    let mut corners_a: BTreeMap<ExactPoint2, u32> = BTreeMap::new();
    let mut corners_b: BTreeMap<ExactPoint2, u32> = BTreeMap::new();
    for (brep, faces, verts, polys, corners, tag) in [
        (
            a,
            &group.faces_a,
            va,
            &mut polys_a,
            &mut corners_a,
            "nary-polygon2d-a",
        ),
        (
            b,
            &group.faces_b,
            vb,
            &mut polys_b,
            &mut corners_b,
            "nary-polygon2d-b",
        ),
    ] {
        for &fi in faces.iter() {
            let Some((poly, c)) = face_polygon_2d(brep, fi, verts, frame) else {
                probe(tag, &group_tag());
                return Err(pair_err());
            };
            polys.push(poly);
            // Merged corner map: bit-equal in-plane keys from two faces of
            // one solid resolve to the same snapped 3D point (both were
            // snapped by the group frame); first insertion wins.
            for (k, v) in c {
                corners.entry(k).or_insert(v);
            }
        }
    }

    // ── §2b in-frame coordinate clustering across the WHOLE group (the
    // same femto-reconciliation the 1×1 path applies; rim domain empty —
    // pure line loops only). Corner keys remap through the pre→post map.
    let band = group
        .pair_idxs
        .iter()
        .map(|&pi| cross[pi].band)
        .fold(0.0_f64, f64::max);
    let pre: Vec<PolygonWithHoles> = polys_a.iter().chain(polys_b.iter()).cloned().collect();
    {
        let mut refs: Vec<&mut PolygonWithHoles> =
            polys_a.iter_mut().chain(polys_b.iter_mut()).collect();
        cluster_frame_coords_rim_aware(&mut refs, &[], band);
    }
    let mut cluster_map: BTreeMap<(u64, u64), (u64, u64)> = BTreeMap::new();
    for (pre_p, post_p) in pre.iter().zip(polys_a.iter().chain(polys_b.iter())) {
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
    let remap = |corners: BTreeMap<ExactPoint2, u32>| -> BTreeMap<ExactPoint2, u32> {
        corners
            .into_iter()
            .map(|(k, v)| {
                let (ux, vy) = (k.x.to_f64().value(), k.y.to_f64().value());
                match cluster_map.get(&(ux.to_bits(), vy.to_bits())) {
                    Some(&(nx, ny)) => (
                        ExactPoint2::from_f64(f64::from_bits(nx), f64::from_bits(ny)).unwrap_or(k),
                        v,
                    ),
                    None => (k, v),
                }
            })
            .collect()
    };
    let corners_a = remap(corners_a);
    let corners_b = remap(corners_b);

    // ── The n-ary exact overlay ─────────────────────────────────────────
    let overlay: ClassifiedOverlay = match coplanar_overlay_multi(&polys_a, &polys_b) {
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

    // ── Resolve overlay vertices to shared 3D points (corners → the
    // snapped/welded solid vertices; everything else the frame lift — no
    // rim machinery in the pure-line scope). ────────────────────────────
    let coords: Vec<Point3> = overlay
        .exact_verts
        .iter()
        .enumerate()
        .map(|(i, exact)| {
            if let Some(&ai) = corners_a.get(exact) {
                va[ai as usize]
            } else if let Some(&bi) = corners_b.get(exact) {
                vb[bi as usize]
            } else {
                let q = overlay.verts[i];
                frame.lift(q.x(), q.y())
            }
        })
        .collect();

    // ── Per-face override triangulations (attribution-scoped tris_for).
    // Overlay triangles are CCW in the frame ⇒ normal +n̂: side-A faces
    // keep the order (n̂ IS their outward normal); side-B faces swap iff
    // the group opposes. The M-B degenerate-3D-image filter mirrors the
    // 1×1 path (femto-split 2D verts resolved to one exact point).
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
                false,
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage0::stage0_preprocess;
    use crate::tests_unit::n2_junction::rj_box;
    use cad_primitives::BoolOp;
    use std::collections::BTreeSet;

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
}
