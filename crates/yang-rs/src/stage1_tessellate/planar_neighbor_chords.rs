//! P0008 (2026-09-30): no interior chord of a curved face's Stage-1
//! triangulation may have both ends on the same PLANAR neighbor.
//!
//! The holed-lateral CDT (`tessellate_lateral_holed_cdt`) triangulates the
//! unrolled boundary polygon with no interior points (the cone kind, and
//! the cylinder kind under the seed gate). Where a planar section curve
//! (an ellipse / hyperbola arc, an oblique circle) bounds the face, that
//! curve is locally convex in the chart and the CDT clips ears over
//! consecutive samples of it and, one level up, uses chords between two of
//! its samples as interior diagonals. Every such chord joins two points of
//! ONE plane — the neighbor face's — so it lies inside the neighbor's own
//! sheet: the mesh carries the neighbor's ear twice (a doubled triangle) or
//! two faces share an interior diagonal. Stage 0's input conformality
//! check then STOPs `i6-edge-overuse` one boolean later (the seed-1
//! index-19 four-step lineage — the loud twin of kernel-v2's render double
//! cover, `tessellate/developable.rs`, same criterion).
//!
//! The rule is combinatorial — no constant, no band: tag every boundary
//! vertex with the planar face(s) across the B-Rep edge(s) it lies on; an
//! interior triangle edge whose ends share a tag is split at its midpoint
//! lifted onto the surface (radius linear in the axial coordinate — exact
//! for cones and cylinders). The minted vertex carries no tag, so one split
//! per such edge terminates the pass. Both incident triangles split
//! (conforming); the lifted vertex is a `BRepFace` source of this face.

use super::*;

/// For every B-Rep edge, the (up to two) PLANAR faces it bounds.
pub(crate) fn edge_planar_faces(faces: &[BRepFace], n_edges: usize) -> Vec<[Option<u32>; 2]> {
    let mut out = vec![[None, None]; n_edges];
    for (fi, f) in faces.iter().enumerate() {
        if !matches!(f.surface, Surface::Plane { .. }) {
            continue;
        }
        for lp in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
            for &e in lp {
                let Some(slot) = out.get_mut(e as usize) else {
                    continue;
                };
                if slot.contains(&Some(fi as u32)) {
                    continue;
                }
                if slot[0].is_none() {
                    slot[0] = Some(fi as u32);
                } else if slot[1].is_none() {
                    slot[1] = Some(fi as u32);
                }
            }
        }
    }
    out
}

/// Planar-neighbor tags of face `f_idx`'s boundary vertices from its
/// attributed loop polylines (`loop_polyline_attributed`: vertex `i`'s
/// following segment lies on edge `lp[i].1`, its preceding one on
/// `lp[i−1].1`): the planar faces OTHER than `f_idx` across those edges.
pub(crate) fn vertex_planar_tags(
    f_idx: usize,
    loops: &[Vec<(u32, u32)>],
    edge_planar: &[[Option<u32>; 2]],
) -> std::collections::HashMap<u32, [Option<u32>; 2]> {
    let nbr = |e: u32| -> Option<u32> {
        edge_planar
            .get(e as usize)
            .and_then(|s| s.iter().flatten().copied().find(|&g| g as usize != f_idx))
    };
    let mut tags: std::collections::HashMap<u32, [Option<u32>; 2]> = Default::default();
    for lp in loops {
        let n = lp.len();
        for i in 0..n {
            let (v, e_after) = lp[i];
            let e_before = lp[(i + n - 1) % n].1;
            let entry = tags.entry(v).or_insert([None, None]);
            for t in [nbr(e_after), nbr(e_before)].into_iter().flatten() {
                if entry.contains(&Some(t)) {
                    continue;
                }
                if entry[0].is_none() {
                    entry[0] = Some(t);
                } else if entry[1].is_none() {
                    entry[1] = Some(t);
                }
            }
        }
    }
    tags.retain(|_, t| t[0].is_some());
    tags
}

/// Split every interior triangle edge of `tris` whose two ends share a
/// planar-neighbor tag, at its midpoint lifted onto the axis-symmetric
/// surface (azimuth and axial coordinate averaged, radius interpolated
/// linearly in the axial coordinate). Returns the number of splits.
#[allow(clippy::too_many_arguments)]
pub(crate) fn split_planar_neighbor_chords(
    f_idx: usize,
    tris: &mut Vec<[u32; 3]>,
    tags: &std::collections::HashMap<u32, [Option<u32>; 2]>,
    is_boundary: &dyn Fn(u32, u32) -> bool,
    axis_point: Point3,
    axis_dir: Vector3,
    out_verts: &mut Vec<Point3>,
    sources: &mut Vec<TessellationSource>,
) -> usize {
    if tags.is_empty() {
        return 0;
    }
    let au = normalize3(axis_dir.as_array());
    let (e1, e2) = ortho_basis(axis_dir);
    let (e1a, e2a) = (e1.as_array(), e2.as_array());
    let ap = axis_point.as_array();
    let cyl = |p: Point3| -> (f64, f64, f64) {
        let p = p.as_array();
        let w = [p[0] - ap[0], p[1] - ap[1], p[2] - ap[2]];
        let v = w[0] * au[0] + w[1] * au[1] + w[2] * au[2];
        let x = w[0] * e1a[0] + w[1] * e1a[1] + w[2] * e1a[2];
        let y = w[0] * e2a[0] + w[1] * e2a[1] + w[2] * e2a[2];
        (y.atan2(x), v, (x * x + y * y).sqrt())
    };
    let shares_tag = |a: u32, b: u32| -> bool {
        match (tags.get(&a), tags.get(&b)) {
            (Some(ta), Some(tb)) => ta
                .iter()
                .flatten()
                .any(|f| tb.iter().flatten().any(|g| g == f)),
            _ => false,
        }
    };
    let mut n_splits = 0usize;
    loop {
        // Flagged edges of the current triangulation, each once.
        let mut flagged: Vec<(u32, u32)> = Vec::new();
        let mut seen: std::collections::HashSet<(u32, u32)> = Default::default();
        for t in tris.iter() {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (a, b) = (t[i].min(t[j]), t[i].max(t[j]));
                if a == b || is_boundary(a, b) || !shares_tag(a, b) {
                    continue;
                }
                if seen.insert((a, b)) {
                    flagged.push((a, b));
                }
            }
        }
        if flagged.is_empty() {
            return n_splits;
        }
        for (a, b) in flagged {
            let (ta, va, ra) = cyl(out_verts[a as usize]);
            let (tb, vb, rb) = cyl(out_verts[b as usize]);
            let mut dt = tb - ta;
            let two_pi = 2.0 * std::f64::consts::PI;
            while dt > std::f64::consts::PI {
                dt -= two_pi;
            }
            while dt < -std::f64::consts::PI {
                dt += two_pi;
            }
            let tm = ta + 0.5 * dt;
            let vm = 0.5 * (va + vb);
            let rm = 0.5 * (ra + rb);
            let (ct, st) = (tm.cos(), tm.sin());
            let pt = Point3::new(
                ap[0] + vm * au[0] + rm * (ct * e1a[0] + st * e2a[0]),
                ap[1] + vm * au[1] + rm * (ct * e1a[1] + st * e2a[1]),
                ap[2] + vm * au[2] + rm * (ct * e1a[2] + st * e2a[2]),
            );
            let m = out_verts.len() as u32;
            out_verts.push(pt);
            sources.push(TessellationSource::BRepFace {
                face: f_idx as u32,
                u: tm,
                v: vm,
            });
            // Every triangle carrying the edge (a, b) — two for a manifold
            // interior edge — splits into its two halves, cyclic order kept.
            let n = tris.len();
            for ti in 0..n {
                let t = tris[ti];
                let Some(ia) = t.iter().position(|&x| x == a) else {
                    continue;
                };
                let Some(ib) = t.iter().position(|&x| x == b) else {
                    continue;
                };
                let mut first = t;
                first[ib] = m;
                let mut second = t;
                second[ia] = m;
                tris[ti] = first;
                tris.push(second);
            }
            n_splits += 1;
        }
    }
}
