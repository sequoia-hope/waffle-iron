//! Shell contact census (spec `specs/untouched_shell_passthrough.md` §3).
//!
//! Which operand SHELLS does the other operand's exact surface reach? A
//! shell no intersection curve can reach is, per Yang 2025 §4.4.2, bounded
//! by its ORIGINAL curves alone and is restored as it was — kept or dropped
//! WHOLE by the op's in/out rule. A closed sphere/torus face is such a
//! shell on its own, and the Stage-1 mesh gives it no boundary edge to
//! segment along (P0030's `s6-curved-empty-cycles`), so the host
//! (kernel-v2) needs the verdict BEFORE the mesh pipeline, from the
//! operands' own Stage-1 meshes.
//!
//! "Untouched" must hold for the EXACT surfaces, not just the meshes: a
//! sub-sagitta graze leaves the meshes apart while the surfaces meet. The
//! gate is the paper's §4.3.1 conservative intersection check (Fig. 10a,
//! "triangles closer than 2dε are filtered"): a triangle pair is a CONTACT
//! when its distance is within both triangles' surface-deviation bounds
//! plus the YR24 weld band. A shell with no contact is CLEAR, and the
//! generalized winding number of the other operand's mesh at one of its
//! vertices says whether it lies inside or outside.

use cherchi_rs::predicates::{segment_intersects_triangle_3d, SegmentTriangleIntersection};

use crate::brep::BRep;
use crate::errors::YangError;
use crate::geom::Surface;
use cad_primitives::Point3;

/// One shell's verdict against the OTHER operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellContact {
    /// Some triangle of the shell lies within the contact band of the other
    /// operand's mesh: an intersection curve may reach it. The mesh pipeline
    /// owns it.
    Contact,
    /// The shell's exact surface is disjoint from the other operand's exact
    /// surface; `inside_other` is its side of the other operand.
    Clear {
        /// The shell lies inside the other operand's solid.
        inside_other: bool,
    },
}

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}
fn add_scaled(a: V3, d: V3, t: f64) -> V3 {
    [a[0] + t * d[0], a[1] + t * d[1], a[2] + t * d[2]]
}

/// Closest point on triangle `abc` to `p` (Ericson 2005 §5.1.5).
fn closest_on_triangle(p: V3, a: V3, b: V3, c: V3) -> V3 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return add_scaled(a, ab, d1 / (d1 - d3));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return add_scaled(a, ac, d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return add_scaled(b, sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    add_scaled(add_scaled(a, ab, v), ac, w)
}

/// Least distance between segments `p1q1` and `p2q2` (Ericson §5.1.9).
fn segment_segment_distance(p1: V3, q1: V3, p2: V3, q2: V3) -> f64 {
    let d1 = sub(q1, p1);
    let d2 = sub(q2, p2);
    let r = sub(p1, p2);
    let a = dot(d1, d1);
    let e = dot(d2, d2);
    let f = dot(d2, r);
    let (s, t);
    if a <= f64::MIN_POSITIVE && e <= f64::MIN_POSITIVE {
        return norm(r);
    }
    if a <= f64::MIN_POSITIVE {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = dot(d1, r);
        if e <= f64::MIN_POSITIVE {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = dot(d1, d2);
            let denom = a * e - b * b;
            let mut s0 = if denom > 0.0 {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut t0 = (b * s0 + f) / e;
            if t0 < 0.0 {
                t0 = 0.0;
                s0 = (-c / a).clamp(0.0, 1.0);
            } else if t0 > 1.0 {
                t0 = 1.0;
                s0 = ((b - c) / a).clamp(0.0, 1.0);
            }
            s = s0;
            t = t0;
        }
    }
    norm(sub(add_scaled(p1, d1, s), add_scaled(p2, d2, t)))
}

/// Least distance between two triangles: 0 when an edge of either crosses
/// the other (EXACT predicate), else the least of the 6 vertex–triangle and
/// 9 edge–edge distances.
pub(crate) fn triangle_distance(t: [Point3; 3], s: [Point3; 3]) -> f64 {
    for (x, y) in [(t, s), (s, t)] {
        for i in 0..3 {
            if segment_intersects_triangle_3d(x[i], x[(i + 1) % 3], y[0], y[1], y[2])
                == SegmentTriangleIntersection::Intersects
            {
                return 0.0;
            }
        }
    }
    let ta = t.map(|p| p.as_array());
    let sa = s.map(|p| p.as_array());
    let mut best = f64::INFINITY;
    for (x, y) in [(ta, sa), (sa, ta)] {
        for &p in &x {
            best = best.min(norm(sub(p, closest_on_triangle(p, y[0], y[1], y[2]))));
        }
    }
    for i in 0..3 {
        for j in 0..3 {
            best = best.min(segment_segment_distance(
                ta[i],
                ta[(i + 1) % 3],
                sa[j],
                sa[(j + 1) % 3],
            ));
        }
    }
    best
}

/// Least Euclidean distance between segment `[p0, p1]` and 3D triangle `s`.
fn segment_triangle_distance(p0: Point3, p1: Point3, s: [Point3; 3]) -> f64 {
    if segment_intersects_triangle_3d(p0, p1, s[0], s[1], s[2])
        == SegmentTriangleIntersection::Intersects
    {
        return 0.0;
    }
    let (a, b) = (p0.as_array(), p1.as_array());
    let sa = s.map(|p| p.as_array());
    let mut best = norm(sub(a, closest_on_triangle(a, sa[0], sa[1], sa[2])))
        .min(norm(sub(b, closest_on_triangle(b, sa[0], sa[1], sa[2]))));
    for j in 0..3 {
        best = best.min(segment_segment_distance(a, b, sa[j], sa[(j + 1) % 3]));
    }
    best
}

/// Closest points of two segments (Ericson, Real-Time Collision Detection
/// §5.1.9), clamped to both segments.
fn segment_segment_closest(p1: V3, q1: V3, p2: V3, q2: V3) -> (V3, V3) {
    let d1 = sub(q1, p1);
    let d2 = sub(q2, p2);
    let r = sub(p1, p2);
    let a = dot(d1, d1);
    let e = dot(d2, d2);
    let f = dot(d2, r);
    let eps = 1e-30;
    if a <= eps && e <= eps {
        return (p1, p2);
    }
    let (s_, t_) = if a <= eps {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = dot(d1, r);
        if e <= eps {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = dot(d1, d2);
            let denom = a * e - b * b;
            let mut s_ = if denom != 0.0 {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut t_ = (b * s_ + f) / e;
            if t_ < 0.0 {
                t_ = 0.0;
                s_ = (-c / a).clamp(0.0, 1.0);
            } else if t_ > 1.0 {
                t_ = 1.0;
                s_ = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s_, t_)
        }
    };
    (add_scaled(p1, d1, s_), add_scaled(p2, d2, t_))
}

/// The closest pair of points of two triangles and their distance (the
/// pair behind [`triangle_distance`]): vertex-to-triangle both ways and
/// edge-to-edge, the minimum over all.
fn triangle_closest_pair(t: [Point3; 3], s: [Point3; 3]) -> (V3, V3, f64) {
    let ta = t.map(|p| p.as_array());
    let sa = s.map(|p| p.as_array());
    // Intersecting triangles: distance 0 at the crossing point (mirrors
    // `triangle_distance`'s exact segment–triangle test; the pair search
    // below only sees vertices and edges and would overestimate).
    for (x, y, xa, ya) in [(t, s, ta, sa), (s, t, sa, ta)] {
        for i in 0..3 {
            if segment_intersects_triangle_3d(x[i], x[(i + 1) % 3], y[0], y[1], y[2])
                == SegmentTriangleIntersection::Intersects
            {
                let (p, q) = (xa[i], xa[(i + 1) % 3]);
                let n = cross(sub(ya[1], ya[0]), sub(ya[2], ya[0]));
                let denom = dot(n, sub(q, p));
                let pt = if denom.abs() > 0.0 {
                    let tt = (dot(n, sub(ya[0], p)) / denom).clamp(0.0, 1.0);
                    add_scaled(p, sub(q, p), tt)
                } else {
                    p
                };
                return (pt, pt, 0.0);
            }
        }
    }
    let mut best: (V3, V3, f64) = (ta[0], sa[0], f64::INFINITY);
    let mut consider = |x: V3, y: V3| {
        let d = norm(sub(x, y));
        if d < best.2 {
            best = (x, y, d);
        }
    };
    for &v in &ta {
        consider(v, closest_on_triangle(v, sa[0], sa[1], sa[2]));
    }
    for &v in &sa {
        consider(closest_on_triangle(v, ta[0], ta[1], ta[2]), v);
    }
    for i in 0..3 {
        for j in 0..3 {
            let (x, y) = segment_segment_closest(ta[i], ta[(i + 1) % 3], sa[j], sa[(j + 1) % 3]);
            consider(x, y);
        }
    }
    best
}

/// One cross-operand triangle pair whose coarse triangles are DISJOINT and
/// whose EXACT surface pieces CROSS (`pieces_cross_point`: refined leaf
/// pieces within the band of the surfaces intersect): the crossing member of
/// the paper's "two meshes do not intersect but are within the distance
/// tolerance d_ε" population (§4.3, `:525-530`) — the one a mesh can miss.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContactPair {
    /// Triangle index into the A mesh / the B mesh.
    pub(crate) tri_a: u32,
    pub(crate) tri_b: u32,
    /// Midpoint of the deciding leaf pieces' closest points.
    pub(crate) point: V3,
    /// The contact reach there: the deciding leaf pieces' two deviation
    /// bounds plus the band — the precision at which the contact is known.
    pub(crate) reach: f64,
}

/// Every A×B triangle pair in contact (see [`ContactPair`]) between the two
/// Stage-2 input meshes, given each mesh's per-triangle face lineage and the
/// faces. Pairs of IDENTICAL surfaces (a coplanar or coaxial-coincident
/// overlap, §4.5.5's and the coincident-cylinder Stage 0's population) are
/// not listed: they touch everywhere by construction. Sorted by `(tri_a,
/// tri_b)`.
/// Per-stage counters from [`contact_pairs_with_stats`] (exposed in-crate for
/// §4.2.2 Gauss-map filter unit pins).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ContactCensusStats {
    pub(crate) visited: usize,
    pub(crate) box_rejected: usize,
    pub(crate) coarse_rejected: usize,
    pub(crate) intersecting: usize,
    pub(crate) skipped: usize,
    pub(crate) gauss_rejected: usize,
    pub(crate) refined: usize,
}

#[derive(Copy, Clone)]
struct NearCandidate {
    t: u32,
    s: u32,
    d_coarse: f64,
    reach: f64,
}

/// Bitmask `1 << k` (`k ∈ 0..3`) per triangle marking edges `(tri[k], tri[(k+1)%3])`
/// that are NOT shared by exactly two triangles of the SAME face `tri_face[ti]`
/// (i.e. true face-trim boundary edges; periodic seam edges have the same face
/// on both sides and carry bit `0`).
fn tri_trim_edge_mask(mesh: &crate::Mesh, tri_face: &[u32]) -> Vec<u8> {
    let mut edges: Vec<((u32, u32, u32), u32, u8)> = Vec::with_capacity(mesh.tris.len() * 3);
    for (ti, tri) in mesh.tris.iter().enumerate() {
        let f = tri_face[ti];
        for k in 0..3 {
            let u = tri[k];
            let v = tri[(k + 1) % 3];
            edges.push(((f, u.min(v), u.max(v)), ti as u32, k as u8));
        }
    }
    edges.sort_unstable();
    let mut mask = vec![0u8; mesh.tris.len()];
    let mut i = 0;
    while i < edges.len() {
        let key = edges[i].0;
        let mut j = i + 1;
        while j < edges.len() && edges[j].0 == key {
            j += 1;
        }
        if j - i != 2 {
            for e in &edges[i..j] {
                mask[e.1 as usize] |= 1u8 << e.2;
            }
        }
        i = j;
    }
    mask
}

#[inline]
fn tris_share_vertex(a: [u32; 3], b: [u32; 3]) -> bool {
    a[0] == b[0]
        || a[0] == b[1]
        || a[0] == b[2]
        || a[1] == b[0]
        || a[1] == b[1]
        || a[1] == b[2]
        || a[2] == b[0]
        || a[2] == b[1]
        || a[2] == b[2]
}

fn pair_touches_trim_within_reach(
    pts_a: [Point3; 3],
    mask_a: u8,
    pts_b: [Point3; 3],
    mask_b: u8,
    reach: f64,
) -> bool {
    if mask_a != 0 {
        for k in 0..3 {
            if (mask_a & (1u8 << k)) != 0
                && segment_triangle_distance(pts_a[k], pts_a[(k + 1) % 3], pts_b) <= reach
            {
                return true;
            }
        }
    }
    if mask_b != 0 {
        for k in 0..3 {
            if (mask_b & (1u8 << k)) != 0
                && segment_triangle_distance(pts_b[k], pts_b[(k + 1) % 3], pts_a) <= reach
            {
                return true;
            }
        }
    }
    false
}

#[derive(Copy, Clone)]
struct OperandView<'a> {
    mesh: &'a crate::Mesh,
    tri_face: &'a [u32],
    set: &'a TriSet,
}

/// Yang 2025 §4.2.2 (`refs/text/yang2025_hybrid_boolean.txt:440-482` R;
/// spec `specs/yang_422_gauss_map_filter.md`): prune connected `2d_ε`
/// potential-intersection areas on each `(face_a, face_b)` whose regions are
/// strictly interior (neither bordering a `d_coarse == 0` mesh crossing nor
/// within `reach` of a face-trim boundary edge, where Sederberg 1989's closed
/// intersection loop precondition does not hold — e.g. `P0021`) and whose
/// conservative closed-form Gauss maps have no collinear normal pair.
fn filter_candidates_by_gauss_map(
    candidates: &[NearCandidate],
    intersecting_pairs: &[(u32, u32)],
    op_a: OperandView<'_>,
    op_b: OperandView<'_>,
) -> Vec<bool> {
    let mut rejected = vec![false; candidates.len()];
    if candidates.is_empty() {
        return rejected;
    }

    let trim_a = tri_trim_edge_mask(op_a.mesh, op_a.tri_face);
    let trim_b = tri_trim_edge_mask(op_b.mesh, op_b.tri_face);

    let mut by_face: std::collections::BTreeMap<(u32, u32), Vec<usize>> =
        std::collections::BTreeMap::new();
    for (idx, c) in candidates.iter().enumerate() {
        let key = (op_a.tri_face[c.t as usize], op_b.tri_face[c.s as usize]);
        by_face.entry(key).or_default().push(idx);
    }

    // Index `d_coarse == 0` pairs per `(face_a, face_b)` by `mesh_a` vertex ONLY
    // for face pairs that actually appear in `by_face`.
    let mut inter_by_face: std::collections::BTreeMap<
        (u32, u32),
        std::collections::BTreeMap<u32, Vec<u32>>,
    > = std::collections::BTreeMap::new();
    for &(ta, sb) in intersecting_pairs {
        let key = (op_a.tri_face[ta as usize], op_b.tri_face[sb as usize]);
        if !by_face.contains_key(&key) {
            continue;
        }
        let map = inter_by_face.entry(key).or_default();
        for &va in &op_a.mesh.tris[ta as usize] {
            map.entry(va).or_default().push(sb);
        }
    }

    let mut gauss_cache_a: std::collections::BTreeMap<u32, crate::gauss_map::GaussMap> =
        std::collections::BTreeMap::new();
    let mut gauss_cache_b: std::collections::BTreeMap<u32, crate::gauss_map::GaussMap> =
        std::collections::BTreeMap::new();

    for (face_key, idxs) in &by_face {
        let m = idxs.len();
        let mut parent: Vec<usize> = (0..m).collect();
        let find = |parent: &mut [usize], mut x: usize| -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        };

        // Connect `(t, s)` and `(t', s')` when `t, t'` share a vertex in `mesh_a`
        // and `s, s'` share a vertex in `mesh_b`.
        let mut vert_a_slots: std::collections::BTreeMap<u32, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (slot, &cand_idx) in idxs.iter().enumerate() {
            let c = candidates[cand_idx];
            let tri_a = op_a.mesh.tris[c.t as usize];
            let tri_b = op_b.mesh.tris[c.s as usize];
            for &va in &tri_a {
                let list = vert_a_slots.entry(va).or_default();
                for &other_slot in list.iter() {
                    let other_s = candidates[idxs[other_slot]].s as usize;
                    if tris_share_vertex(tri_b, op_b.mesh.tris[other_s]) {
                        let ra = find(&mut parent, slot);
                        let rb = find(&mut parent, other_slot);
                        if ra != rb {
                            parent[rb] = ra;
                        }
                    }
                }
                list.push(slot);
            }
        }

        let mut comps: std::collections::BTreeMap<usize, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (slot, &cand_idx) in idxs.iter().enumerate() {
            let root = find(&mut parent, slot);
            comps.entry(root).or_default().push(cand_idx);
        }

        let inter_verts = inter_by_face.get(face_key);
        for comp_idxs in comps.values() {
            let mut strictly_interior = true;
            for &cand_idx in comp_idxs {
                let c = candidates[cand_idx];
                let t = c.t as usize;
                let s = c.s as usize;
                if pair_touches_trim_within_reach(
                    op_a.set.pts[t],
                    trim_a[t],
                    op_b.set.pts[s],
                    trim_b[s],
                    c.reach,
                ) {
                    strictly_interior = false;
                    break;
                }
                if let Some(iv) = inter_verts {
                    let tri_a = op_a.mesh.tris[t];
                    let tri_b = op_b.mesh.tris[s];
                    let borders_inter = tri_a.iter().any(|va| {
                        iv.get(va).is_some_and(|sbs| {
                            sbs.iter()
                                .any(|&sb| tris_share_vertex(tri_b, op_b.mesh.tris[sb as usize]))
                        })
                    });
                    if borders_inter {
                        strictly_interior = false;
                        break;
                    }
                }
            }
            if !strictly_interior {
                continue;
            }

            let mut ta_list: Vec<u32> = comp_idxs.iter().map(|&i| candidates[i].t).collect();
            ta_list.sort_unstable();
            ta_list.dedup();
            let mut sb_list: Vec<u32> = comp_idxs.iter().map(|&i| candidates[i].s).collect();
            sb_list.sort_unstable();
            sb_list.dedup();

            let mut union_a: Option<crate::gauss_map::GaussMap> = None;
            let mut ga_vec = Vec::with_capacity(ta_list.len());
            for &t in &ta_list {
                let gm = *gauss_cache_a.entry(t).or_insert_with(|| {
                    let ti = t as usize;
                    crate::gauss_map::GaussMap::from_triangle(
                        &op_a.set.surf[ti],
                        op_a.set.pts[ti].map(|p| p.as_array()),
                    )
                });
                union_a = Some(match union_a {
                    None => gm,
                    Some(acc) => acc.union(&gm),
                });
                ga_vec.push(gm);
            }

            let mut union_b: Option<crate::gauss_map::GaussMap> = None;
            let mut gb_vec = Vec::with_capacity(sb_list.len());
            for &s in &sb_list {
                let gm = *gauss_cache_b.entry(s).or_insert_with(|| {
                    let si = s as usize;
                    crate::gauss_map::GaussMap::from_triangle(
                        &op_b.set.surf[si],
                        op_b.set.pts[si].map(|p| p.as_array()),
                    )
                });
                union_b = Some(match union_b {
                    None => gm,
                    Some(acc) => acc.union(&gm),
                });
                gb_vec.push(gm);
            }

            let overlaps = match (union_a, union_b) {
                (Some(ua), Some(ub)) => {
                    if !ua.overlaps(&ub) {
                        false
                    } else if ga_vec.len() * gb_vec.len() <= 256 {
                        ga_vec
                            .iter()
                            .any(|ga| gb_vec.iter().any(|gb| ga.overlaps(gb)))
                    } else {
                        true
                    }
                }
                _ => false,
            };

            if !overlaps {
                for &cand_idx in comp_idxs {
                    rejected[cand_idx] = true;
                }
            }
        }
    }

    rejected
}

pub(crate) fn contact_pairs(
    mesh_a: &crate::Mesh,
    tri_face_a: &[u32],
    faces_a: &[crate::BRepFace],
    mesh_b: &crate::Mesh,
    tri_face_b: &[u32],
    faces_b: &[crate::BRepFace],
    skip: &dyn Fn(u32, u32) -> bool,
) -> Result<Vec<ContactPair>, YangError> {
    contact_pairs_with_stats(
        mesh_a, tri_face_a, faces_a, mesh_b, tri_face_b, faces_b, skip,
    )
    .map(|(out, _)| out)
}

pub(crate) fn contact_pairs_with_stats(
    mesh_a: &crate::Mesh,
    tri_face_a: &[u32],
    faces_a: &[crate::BRepFace],
    mesh_b: &crate::Mesh,
    tri_face_b: &[u32],
    faces_b: &[crate::BRepFace],
    skip: &dyn Fn(u32, u32) -> bool,
) -> Result<(Vec<ContactPair>, ContactCensusStats), YangError> {
    let cost = std::env::var_os("YANG_T6_COST_PROBE").is_some();
    let t0 = cost.then(std::time::Instant::now); // wasm-ok: env-gated probe (YANG_T6_COST_PROBE), never set in wasm
    let set_a = TriSet::from_parts(mesh_a, tri_face_a, faces_a)?;
    let set_b = TriSet::from_parts(mesh_b, tri_face_b, faces_b)?;
    let scale = set_a
        .lo
        .iter()
        .chain(set_a.hi.iter())
        .chain(set_b.lo.iter())
        .chain(set_b.hi.iter())
        .flat_map(|v| v.iter())
        .fold(0.0_f64, |m, &c| m.max(c.abs()));
    let band = 2.0 * cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK);
    let bvh_b = Bvh::new(&set_b);
    let max_dev_b = set_b.dev.iter().copied().fold(0.0, f64::max);
    let probe = std::env::var_os("YANG_T6_PROBE").is_some();
    let t1 = cost.then(std::time::Instant::now); // wasm-ok: env-gated probe (YANG_T6_COST_PROBE), never set in wasm
    let mut stats = ContactCensusStats::default();
    let mut intersecting_pairs: Vec<(u32, u32)> = Vec::new();
    let mut candidates: Vec<NearCandidate> = Vec::new();

    #[allow(clippy::needless_range_loop)] // `t` indexes five parallel per-triangle vectors
    for t in 0..set_a.pts.len() {
        let pad = set_a.dev[t] + max_dev_b + band;
        bvh_b.for_each_near(set_a.lo[t], set_a.hi[t], pad, |s| {
            let s = s as usize;
            stats.visited += 1;
            if set_a.surf[t] == set_b.surf[s] {
                return;
            }
            let reach = set_a.dev[t] + set_b.dev[s] + band;
            if (0..3).any(|k| {
                set_a.lo[t][k] > set_b.hi[s][k] + reach || set_b.lo[s][k] > set_a.hi[t][k] + reach
            }) {
                stats.box_rejected += 1;
                return;
            }
            let d_coarse = triangle_distance(set_a.pts[t], set_b.pts[s]);
            // Intersecting (or exactly touching) triangles are the
            // arrangement's own population — their constraint segment IS an
            // intersection edge (Cherchi 2020 §4) — not a contact the mesh
            // could have missed. Skipping them here is also the census's
            // cost: every pair along every real curve would otherwise refine
            // its exact pieces to the leaf (four gear cases hit the 900 s
            // budget on 2026-10-14 before this line).
            if d_coarse == 0.0 {
                stats.intersecting += 1;
                intersecting_pairs.push((t as u32, s as u32));
                return;
            }
            if d_coarse > reach {
                stats.coarse_rejected += 1;
                if probe {
                    eprintln!(
                        "[t6] a{t}(f{}) b{s}(f{}) coarse d={d_coarse:.4e} > reach {reach:.4e}",
                        tri_face_a[t], tri_face_b[s]
                    );
                }
                return;
            }
            // The caller's cheap verdict first (T6: a pair whose one-rings
            // the arrangement already joined can never be a MISSED
            // intersection). Without it, a pair BESIDE a real crossing — its
            // pieces within reach all along the curve, never crossing
            // themselves — refines exhaustively to the depth limit: F0055
            // (a 1e4-scale model) spent 112 s on 21 such pairs.
            if skip(t as u32, s as u32) {
                stats.skipped += 1;
                return;
            }
            candidates.push(NearCandidate {
                t: t as u32,
                s: s as u32,
                d_coarse,
                reach,
            });
        });
    }

    let gauss_rejected = filter_candidates_by_gauss_map(
        &candidates,
        &intersecting_pairs,
        OperandView {
            mesh: mesh_a,
            tri_face: tri_face_a,
            set: &set_a,
        },
        OperandView {
            mesh: mesh_b,
            tri_face: tri_face_b,
            set: &set_b,
        },
    );

    let piece = |set: &TriSet, i: usize| Piece {
        pts: set.pts[i].map(|p| p.as_array()),
        surf: set.surf[i],
        dev: set.dev[i],
    };
    let mut out = Vec::new();
    for (idx, c) in candidates.iter().enumerate() {
        let t = c.t as usize;
        let s = c.s as usize;
        if gauss_rejected[idx] {
            stats.gauss_rejected += 1;
            if probe {
                eprintln!(
                    "[t6] a{t}(f{}) b{s}(f{}) coarse d={:.4e} reach {:.4e} gauss-rejected",
                    tri_face_a[t], tri_face_b[s], c.d_coarse, c.reach
                );
            }
            continue;
        }
        stats.refined += 1;
        let touch =
            pieces_cross_point(piece(&set_a, t), piece(&set_b, s), band, CROSS_REFINE_DEPTH);
        if probe {
            let (x, y, d2) = triangle_closest_pair(set_a.pts[t], set_b.pts[s]);
            eprintln!(
                "[t6] a{t}(f{}) b{s}(f{}) coarse d={:.4e} pair-d={d2:.4e} reach {:.4e} devs {:.3e}/{:.3e} touch={:?} x={x:?} y={y:?}",
                tri_face_a[t], tri_face_b[s], c.d_coarse, c.reach, set_a.dev[t], set_b.dev[s], touch.map(|(_, r)| r)
            );
        }
        if let Some((point, leaf_reach)) = touch {
            out.push(ContactPair {
                tri_a: c.t,
                tri_b: c.s,
                point,
                reach: leaf_reach,
            });
        }
    }

    out.sort_by_key(|p| (p.tri_a, p.tri_b));
    if let (Some(t0), Some(t1)) = (t0, t1) {
        eprintln!(
            "[t6-cost] tris {}×{} sets+bvh {:.3}s, pairs {:.3}s: visited {} \
             (box-rejected {}, coarse-rejected {}, intersecting {}, \
             skipped {}, gauss-rejected {}, refined {}) → {} contacts; max dev a {:.3e} b {:.3e}, \
             band {band:.3e}",
            set_a.pts.len(),
            set_b.pts.len(),
            t1.duration_since(t0).as_secs_f64(),
            t1.elapsed().as_secs_f64(),
            stats.visited,
            stats.box_rejected,
            stats.coarse_rejected,
            stats.intersecting,
            stats.skipped,
            stats.gauss_rejected,
            stats.refined,
            out.len(),
            set_a.dev.iter().copied().fold(0.0, f64::max),
            max_dev_b,
        );
    }
    Ok((out, stats))
}

/// Where the exact surface pieces over `t` and `s` CROSS: refine as
/// [`pieces_may_touch`] does (prune a pair whose chord distance exceeds both
/// deviations plus the band; split the piece with the larger deviation)
/// down to leaf pieces whose deviations no longer exceed the band, and
/// report the first leaf pair whose inscribed triangles INTERSECT — the
/// crossing point and that leaf's reach (both leaf deviations plus the
/// band). `None` when no leaf pair intersects.
///
/// Intersection at the leaf, not proximity: Yang §4.3's near-miss population
/// (`:525-530`) is "a tangent point, or a small loop, or no intersection",
/// and only a crossing is something refinement can make the meshes meet.
/// Two pieces that merely come within the band — two planar faces 2e-7
/// apart on a 1e-4 model (R0015, measured 2026-10-14: seven such sites took
/// its loop to the ceiling, every one at reach = band exactly) — are "no
/// intersection" at model resolution, not a missed one. A sub-band graze is
/// likewise invisible here by design: refinement cannot resolve it either —
/// the size-matched leaf pair's vertices must reach more than the band into
/// and out of each other's EXACT surfaces (a crossing deeper than the model
/// resolution, read locally).
fn pieces_cross_point(t: Piece, s: Piece, band: f64, depth: u32) -> Option<(V3, f64)> {
    let to_pts = |p: &Piece| p.pts.map(|v| Point3::new(v[0], v[1], v[2]));
    let (x, y, d) = triangle_closest_pair(to_pts(&t), to_pts(&s));
    let reach = t.dev + s.dev + band;
    if d > reach {
        return None;
    }
    let longest = |p: &Piece| {
        norm(sub(p.pts[1], p.pts[0]))
            .max(norm(sub(p.pts[2], p.pts[1])))
            .max(norm(sub(p.pts[0], p.pts[2])))
    };
    let (lt, ls) = (longest(&t), longest(&s));
    let within_band = t.dev + s.dev <= band;
    // Size-matched leaves: a piece more than twice its partner's size is
    // split further even once the deviations are within the band (a planar
    // piece splits with deviation 0), so the depth test below reads the
    // crossing LOCALLY rather than at a far corner of a large triangle.
    let comparable = lt <= 2.0 * ls && ls <= 2.0 * lt;
    if (within_band && comparable) || depth == 0 {
        // A RESOLVABLE crossing: each leaf's vertices reach more than the
        // band into AND out of the other piece's EXACT surface. A crossing
        // shallower than the model band is below the A14.2 feature floor —
        // `n2_rim_mint_adversary`'s δ = 1e-4·r row: a box cap crosses a
        // cylinder lateral transversally, but only over a lens 2.13e-8 deep
        // against a 2e-7 band; the loop chasing it adopted a ×8 body with a
        // vertex 2.13e-8 off its surface where the natural body was valid.
        let straddles = |x: &Piece, surf: &Surface| -> bool {
            let scale = match *surf {
                // The cone's residual is radial; ×cos α is the distance.
                Surface::Cone { half_angle, .. } => half_angle.cos(),
                _ => 1.0,
            };
            let (lo, hi) = x
                .pts
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &p| {
                    match crate::geom::signed_distance_to_surface(
                        *surf,
                        Point3::new(p[0], p[1], p[2]),
                    ) {
                        Ok(v) => (lo.min(v * scale), hi.max(v * scale)),
                        Err(_) => (lo, hi),
                    }
                });
            lo < -band && hi > band
        };
        return (d == 0.0 && straddles(&t, &s.surf) && straddles(&s, &t.surf)).then_some((
            [
                (x[0] + y[0]) * 0.5,
                (x[1] + y[1]) * 0.5,
                (x[2] + y[2]) * 0.5,
            ],
            reach,
        ));
    }
    // Split the piece that most needs it: the larger deviation while the
    // band is not met, else the larger piece.
    let split_t = if within_band {
        lt >= ls
    } else {
        t.dev >= s.dev
    };
    if split_t {
        t.split()
            .into_iter()
            .find_map(|c| pieces_cross_point(c, s, band, depth - 1))
    } else {
        s.split()
            .into_iter()
            .find_map(|c| pieces_cross_point(t, c, band, depth - 1))
    }
}

fn circumradius(a: V3, b: V3, c: V3) -> f64 {
    let (la, lb, lc) = (norm(sub(b, c)), norm(sub(c, a)), norm(sub(a, b)));
    let area2 = norm(cross(sub(b, a), sub(c, a)));
    if area2 <= 0.0 {
        // A degenerate triangle: its longest edge bounds every point of it.
        return la.max(lb).max(lc);
    }
    la * lb * lc / (2.0 * area2)
}

/// A conservative bound on the deviation of an inscribed triangle `t`
/// (vertices on `surface`) from `surface`: `max_{p ∈ t} dist(p, surface)`,
/// the quantity Yang §4.1's d_ε bounds. Read from each surface's own
/// distance field — never from the triangle's circumradius, whose
/// osculating-sphere sagitta `(1 − √(1 − (ρκ)²)) / κ` is only exact on a
/// sphere and read 6.3 on a 5 cm model for the sliver triangles of a gear
/// revolve (R0016, 2026-10-14): a sliver's circumradius is unbounded while
/// its deviation is the sagitta over its short span.
///
/// - plane: 0.
/// - sphere: `R − dist(center, t)` — the point of `t` nearest the centre.
///   Exact.
/// - cylinder: `R − min_{p ∈ t} dist(p, axis)`; the distance to a line is
///   convex, so its minimum over `t` is on an edge unless the axis pierces
///   `t` (then deviation `R`) — a closed-form segment-to-line distance per
///   edge. Exact.
/// - cone, and torus where the whole triangle lies at `ρ ≥ R_major`: the
///   deviation is the maximum of a CONCAVE function `g` over `t` (cone: `h
///   sin α − r cos α`, linear minus convex; outer torus: `r_minor − dist(p,
///   centre circle)`, the distance to the circle being convex where `ρ ≥ R`).
///   A concave function lies below each of its tangent planes, so for any
///   point `p`, `max_t g ≤ g(p) + max_j ∇g(p)·(t_j − p)` — a linear function's
///   maximum is at a corner. The bound is the least of that over seven
///   interior points; it is exact where `∇g` vanishes and within a small
///   factor of the maximum elsewhere. (A nested ternary search found the
///   exact maximum at ~5,000 evaluations per piece and cost R0085's census
///   45.8 s over 88k refined pairs; this costs seven gradients.)
/// - torus otherwise: no concavity to lean on; `dist(p, surface)` is
///   1-Lipschitz, so a barycentric sample grid of step `1/n` bounds the
///   maximum by the largest sample plus the grid's cover radius `L / n`.
///
/// Every branch is conservative; a degenerate frame (zero axis) falls back
/// to the circumradius.
fn surface_deviation(surface: &Surface, t: [V3; 3]) -> f64 {
    let unit = |v: V3| {
        let l = norm(v);
        (l > 0.0).then(|| [v[0] / l, v[1] / l, v[2] / l])
    };
    let longest = norm(sub(t[1], t[0]))
        .max(norm(sub(t[2], t[1])))
        .max(norm(sub(t[0], t[2])));
    // Least distance from the segment `a → b` to the line `o + s·u` (`u`
    // unit): the squared distance is a convex quadratic in the segment
    // parameter, minimised in closed form and clamped to the segment.
    let segment_line_distance = |a: V3, b: V3, o: V3, u: V3| -> f64 {
        let d = sub(b, a);
        let w0 = sub(a, o);
        let (du, wu) = (dot(d, u), dot(w0, u));
        let qa = dot(d, d) - du * du;
        let qb = 2.0 * (dot(w0, d) - wu * du);
        let qc = dot(w0, w0) - wu * wu;
        let f = |s: f64| (qa * s * s + qb * s + qc).max(0.0);
        let mut best = f(0.0).min(f(1.0));
        if qa > 0.0 {
            best = best.min(f((-qb / (2.0 * qa)).clamp(0.0, 1.0)));
        }
        best.sqrt()
    };
    // Does the line `a + s·u` pierce the triangle?
    let line_pierces = |a: V3, u: V3| -> bool {
        let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
        let denom = dot(n, u);
        if denom.abs() <= f64::MIN_POSITIVE {
            return false;
        }
        let s = dot(n, sub(t[0], a)) / denom;
        let q = add_scaled(a, u, s);
        let c = closest_on_triangle(q, t[0], t[1], t[2]);
        norm(sub(c, q)) <= 1e-12 * (1.0 + norm(q))
    };
    // Least distance from the triangle to the line `o + s·u`.
    let triangle_line_distance = |o: V3, u: V3| -> f64 {
        if line_pierces(o, u) {
            return 0.0;
        }
        segment_line_distance(t[0], t[1], o, u)
            .min(segment_line_distance(t[1], t[2], o, u))
            .min(segment_line_distance(t[2], t[0], o, u))
    };
    // Upper bound on a CONCAVE `g` over the triangle from its tangent planes
    // at seven interior points (see the doc above).
    let concave_bound = |g: &dyn Fn(V3) -> f64, grad: &dyn Fn(V3) -> V3| -> f64 {
        let bary = |a: f64, b: f64, c: f64| {
            [
                a * t[0][0] + b * t[1][0] + c * t[2][0],
                a * t[0][1] + b * t[1][1] + c * t[2][1],
                a * t[0][2] + b * t[1][2] + c * t[2][2],
            ]
        };
        let third = 1.0 / 3.0;
        let points = [
            bary(third, third, third),
            bary(0.5, 0.5, 0.0),
            bary(0.0, 0.5, 0.5),
            bary(0.5, 0.0, 0.5),
            bary(0.6, 0.2, 0.2),
            bary(0.2, 0.6, 0.2),
            bary(0.2, 0.2, 0.6),
        ];
        points
            .iter()
            .map(|&p| {
                let gr = grad(p);
                g(p) + t
                    .iter()
                    .map(|&v| dot(gr, sub(v, p)))
                    .fold(f64::NEG_INFINITY, f64::max)
            })
            .fold(f64::INFINITY, f64::min)
    };
    match *surface {
        Surface::Plane { .. } => 0.0,
        Surface::Sphere { center, radius } => {
            let c = center.as_array();
            let q = closest_on_triangle(c, t[0], t[1], t[2]);
            (radius - norm(sub(q, c))).max(0.0)
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            let Some(u) = unit(axis_dir.as_array()) else {
                return circumradius(t[0], t[1], t[2]);
            };
            (radius - triangle_line_distance(axis_point.as_array(), u)).max(0.0)
        }
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let o = apex.as_array();
            let Some(u) = unit(axis_dir.as_array()) else {
                return circumradius(t[0], t[1], t[2]);
            };
            // The nappe the triangle lies on: the sign of its vertices' axial
            // coordinate (an inscribed triangle is inside ONE convex nappe).
            let sign = {
                let h: f64 = t.iter().map(|&p| dot(sub(p, o), u)).sum();
                if h < 0.0 {
                    -1.0
                } else {
                    1.0
                }
            };
            let su = [u[0] * sign, u[1] * sign, u[2] * sign];
            let (sa, ca) = half_angle.sin_cos();
            let radial = |p: V3| {
                let w = sub(p, o);
                let h = dot(w, su);
                let rv = add_scaled(w, su, -h);
                (h, rv, norm(rv))
            };
            let g = |p: V3| {
                let (h, _, r) = radial(p);
                h * sa - r * ca
            };
            // ∇g = sin α·u − cos α·r̂ (on the axis, 0 is a supergradient of −r).
            let grad = |p: V3| {
                let (_, rv, r) = radial(p);
                let rhat = if r > 0.0 {
                    [rv[0] / r, rv[1] / r, rv[2] / r]
                } else {
                    [0.0; 3]
                };
                [
                    sa * su[0] - ca * rhat[0],
                    sa * su[1] - ca * rhat[1],
                    sa * su[2] - ca * rhat[2],
                ]
            };
            concave_bound(&g, &grad).max(0.0)
        }
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } => {
            let c = center.as_array();
            let Some(u) = unit(axis_dir.as_array()) else {
                return circumradius(t[0], t[1], t[2]);
            };
            // Nearest point of the centre circle and the offset to it.
            let to_circle = |p: V3| -> V3 {
                let w = sub(p, c);
                let z = dot(w, u);
                let rv = add_scaled(w, u, -z);
                let rho = norm(rv);
                let q = if rho > 0.0 {
                    add_scaled(c, rv, major_radius / rho)
                } else {
                    c
                };
                sub(p, q)
            };
            // ρ is convex, so the triangle lies at ρ ≥ R iff its least
            // distance to the axis does.
            if triangle_line_distance(c, u) >= major_radius {
                let g = |p: V3| minor_radius - norm(to_circle(p));
                // ∇g = −(p − q)/|p − q| (on the circle, 0 is a supergradient).
                let grad = |p: V3| {
                    let d = to_circle(p);
                    let l = norm(d);
                    if l > 0.0 {
                        [-d[0] / l, -d[1] / l, -d[2] / l]
                    } else {
                        [0.0; 3]
                    }
                };
                concave_bound(&g, &grad).max(0.0)
            } else {
                // Lipschitz-covered barycentric samples.
                const N: usize = 8;
                let mut best = 0.0_f64;
                for i in 0..=N {
                    for j in 0..=(N - i) {
                        let (a, b) = (i as f64 / N as f64, j as f64 / N as f64);
                        let g = 1.0 - a - b;
                        let p = [
                            a * t[0][0] + b * t[1][0] + g * t[2][0],
                            a * t[0][1] + b * t[1][1] + g * t[2][1],
                            a * t[0][2] + b * t[1][2] + g * t[2][2],
                        ];
                        best = best.max((minor_radius - norm(to_circle(p))).abs());
                    }
                }
                best + longest / N as f64
            }
        }
    }
}

/// Closest point of `surface` to `p` — the foot a subdivision midpoint is
/// moved to so the sub-triangle is again inscribed. `p` itself where the
/// foot is not unique (a point on a cylinder's / cone's axis, a torus's
/// tube-centre circle, a sphere's centre).
fn project_to_surface(surface: &Surface, p: V3) -> V3 {
    let unit = |v: V3| {
        let l = norm(v);
        (l > 0.0).then(|| [v[0] / l, v[1] / l, v[2] / l])
    };
    match *surface {
        Surface::Plane { .. } => p,
        Surface::Sphere { center, radius } => {
            let c = center.as_array();
            match unit(sub(p, c)) {
                Some(d) => add_scaled(c, d, radius),
                None => p,
            }
        }
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => {
            let (a, Some(u)) = (axis_point.as_array(), unit(axis_dir.as_array())) else {
                return p;
            };
            let q = sub(p, a);
            let foot = add_scaled(a, u, dot(q, u));
            match unit(sub(p, foot)) {
                Some(d) => add_scaled(foot, d, radius),
                None => p,
            }
        }
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => {
            let (o, Some(u)) = (apex.as_array(), unit(axis_dir.as_array())) else {
                return p;
            };
            let q = sub(p, o);
            let Some(w) = unit(add_scaled(q, u, -dot(q, u))) else {
                return p;
            };
            // The generator in p's half-plane, from the apex; the nappe is
            // the half-line, so a foot behind the apex is the apex.
            let g = [
                half_angle.cos() * u[0] + half_angle.sin() * w[0],
                half_angle.cos() * u[1] + half_angle.sin() * w[1],
                half_angle.cos() * u[2] + half_angle.sin() * w[2],
            ];
            add_scaled(o, g, dot(q, g).max(0.0))
        }
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } => {
            let (c, Some(u)) = (center.as_array(), unit(axis_dir.as_array())) else {
                return p;
            };
            let q = sub(p, c);
            let Some(w) = unit(add_scaled(q, u, -dot(q, u))) else {
                return p;
            };
            let m = add_scaled(c, w, major_radius);
            match unit(sub(p, m)) {
                Some(d) => add_scaled(m, d, minor_radius),
                None => p,
            }
        }
    }
}

/// One side of a refined pair test: a triangle inscribed in `surf`.
#[derive(Clone, Copy)]
struct Piece {
    pts: [V3; 3],
    surf: Surface,
    dev: f64,
}

impl Piece {
    fn new(pts: [V3; 3], surf: Surface) -> Self {
        Piece {
            pts,
            surf,
            dev: surface_deviation(&surf, pts),
        }
    }

    /// The four inscribed children (Yang 2025 §4.1's refinement: a patch
    /// whose deviation exceeds the tolerance is split into four).
    fn split(&self) -> [Piece; 4] {
        let mid = |i: usize, j: usize| {
            let (a, b) = (self.pts[i], self.pts[j]);
            project_to_surface(
                &self.surf,
                [
                    (a[0] + b[0]) * 0.5,
                    (a[1] + b[1]) * 0.5,
                    (a[2] + b[2]) * 0.5,
                ],
            )
        };
        let (m01, m12, m20) = (mid(0, 1), mid(1, 2), mid(2, 0));
        let [p0, p1, p2] = self.pts;
        [
            Piece::new([p0, m01, m20], self.surf),
            Piece::new([m01, p1, m12], self.surf),
            Piece::new([m20, m12, p2], self.surf),
            Piece::new([m01, m12, m20], self.surf),
        ]
    }
}

/// Deepest refinement: each level shrinks a piece's deviation bound about
/// fourfold, so 16 levels take any Stage-1 triangle far below the weld band.
const MAX_REFINE_DEPTH: u32 = 16;

/// The crossing search's depth: [`MAX_REFINE_DEPTH`] for the deviation
/// refinement plus eight size-matching halvings (a 256× size ratio between
/// a planar piece and its curved partner's leaf).
const CROSS_REFINE_DEPTH: u32 = MAX_REFINE_DEPTH + 8;

/// Can the exact surface patches over `t` and `s` come within `band` of each
/// other? The chord test is conservative (`dist ≤ dev_t + dev_s + band`); a
/// pair that fails it is refined — the piece with the larger deviation is
/// split into four inscribed children — until it either clears or its
/// deviations no longer exceed the band, where a remaining reach IS a
/// contact.
fn pieces_may_touch(t: Piece, s: Piece, band: f64, depth: u32) -> bool {
    let to_pts = |p: &Piece| p.pts.map(|v| Point3::new(v[0], v[1], v[2]));
    let d = triangle_distance(to_pts(&t), to_pts(&s));
    if d > t.dev + s.dev + band {
        return false;
    }
    if t.dev + s.dev <= band || depth == 0 {
        return true;
    }
    if t.dev >= s.dev {
        t.split()
            .into_iter()
            .any(|c| pieces_may_touch(c, s, band, depth - 1))
    } else {
        s.split()
            .into_iter()
            .any(|c| pieces_may_touch(t, c, band, depth - 1))
    }
}

/// One operand's Stage-1 triangles with their inflated boxes.
struct TriSet {
    pts: Vec<[Point3; 3]>,
    surf: Vec<Surface>,
    dev: Vec<f64>,
    lo: Vec<V3>,
    hi: Vec<V3>,
}

impl TriSet {
    fn new(brep: &BRep) -> Result<Self, YangError> {
        Self::from_parts(brep.as_mesh(), brep.tri_face(), brep.faces())
    }

    /// The set over an explicit mesh with its per-triangle face lineage and
    /// the faces' surfaces (the Stage-2 input meshes, which Stage 0 may have
    /// re-tessellated away from the B-Rep's own).
    fn from_parts(
        mesh: &crate::Mesh,
        tri_face: &[u32],
        faces: &[crate::BRepFace],
    ) -> Result<Self, YangError> {
        if tri_face.len() != mesh.tris.len() {
            // No Stage-1 face lineage (a `from_mesh` operand): no per-face
            // surface, so no deviation bound — refuse rather than guess.
            return Err(YangError::MalformedTopology(
                "shell contact census: operand has no Stage-1 face lineage".to_string(),
            ));
        }
        let mut out = TriSet {
            pts: Vec::with_capacity(mesh.tris.len()),
            surf: Vec::with_capacity(mesh.tris.len()),
            dev: Vec::with_capacity(mesh.tris.len()),
            lo: Vec::with_capacity(mesh.tris.len()),
            hi: Vec::with_capacity(mesh.tris.len()),
        };
        for (ti, tri) in mesh.tris.iter().enumerate() {
            let pts = tri.map(|v| mesh.verts[v as usize]);
            let arr = pts.map(|p| p.as_array());
            let surface = &faces[tri_face[ti] as usize].surface;
            let dev = surface_deviation(surface, arr);
            let mut lo = [f64::INFINITY; 3];
            let mut hi = [f64::NEG_INFINITY; 3];
            for p in arr {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            out.pts.push(pts);
            out.surf.push(*surface);
            out.dev.push(dev);
            out.lo.push(lo);
            out.hi.push(hi);
        }
        Ok(out)
    }
}

/// A median-split bounding-volume tree over a [`TriSet`]'s boxes.
struct Bvh {
    nodes: Vec<BvhNode>,
    order: Vec<u32>,
}

struct BvhNode {
    lo: V3,
    hi: V3,
    /// Leaf: `order[start..start + count]`; interior: `count == 0` and the
    /// children are `left` and `left + 1`.
    start: u32,
    count: u32,
    left: u32,
}

const LEAF: usize = 8;

impl Bvh {
    fn new(set: &TriSet) -> Self {
        let mut bvh = Bvh {
            nodes: Vec::new(),
            order: (0..set.pts.len() as u32).collect(),
        };
        if !set.pts.is_empty() {
            bvh.nodes.push(BvhNode {
                lo: [0.0; 3],
                hi: [0.0; 3],
                start: 0,
                count: 0,
                left: 0,
            });
            bvh.build(set, 0, 0, set.pts.len());
        }
        bvh
    }

    fn build(&mut self, set: &TriSet, node: usize, start: usize, end: usize) {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for &t in &self.order[start..end] {
            for k in 0..3 {
                lo[k] = lo[k].min(set.lo[t as usize][k]);
                hi[k] = hi[k].max(set.hi[t as usize][k]);
            }
        }
        self.nodes[node].lo = lo;
        self.nodes[node].hi = hi;
        if end - start <= LEAF {
            self.nodes[node].start = start as u32;
            self.nodes[node].count = (end - start) as u32;
            return;
        }
        let axis = (0..3)
            .max_by(|&a, &b| (hi[a] - lo[a]).total_cmp(&(hi[b] - lo[b])))
            .unwrap_or(0);
        let mid = (start + end) / 2;
        let key = |t: &u32| set.lo[*t as usize][axis] + set.hi[*t as usize][axis];
        self.order[start..end]
            .select_nth_unstable_by(mid - start, |x, y| key(x).total_cmp(&key(y)));
        let left = self.nodes.len();
        for _ in 0..2 {
            self.nodes.push(BvhNode {
                lo: [0.0; 3],
                hi: [0.0; 3],
                start: 0,
                count: 0,
                left: 0,
            });
        }
        self.nodes[node].left = left as u32;
        self.build(set, left, start, mid);
        self.build(set, left + 1, mid, end);
    }

    /// Calls `visit` with every triangle whose box is within `pad` of
    /// `[lo, hi]`; stops (returning `true`) as soon as `visit` does.
    /// Every stored triangle whose box lies within `pad` of `[lo, hi]`.
    fn for_each_near(&self, lo: V3, hi: V3, pad: f64, mut visit: impl FnMut(u32)) {
        if self.nodes.is_empty() {
            return;
        }
        let mut stack = vec![0usize];
        while let Some(n) = stack.pop() {
            let node = &self.nodes[n];
            if (0..3).any(|k| node.lo[k] > hi[k] + pad || lo[k] > node.hi[k] + pad) {
                continue;
            }
            if node.count > 0 {
                let s = node.start as usize;
                for &t in &self.order[s..s + node.count as usize] {
                    visit(t);
                }
            } else {
                stack.push(node.left as usize);
                stack.push(node.left as usize + 1);
            }
        }
    }

    fn any_near(&self, lo: V3, hi: V3, pad: f64, mut visit: impl FnMut(u32) -> bool) -> bool {
        if self.nodes.is_empty() {
            return false;
        }
        let mut stack = vec![0usize];
        while let Some(n) = stack.pop() {
            let node = &self.nodes[n];
            if (0..3).any(|k| node.lo[k] > hi[k] + pad || lo[k] > node.hi[k] + pad) {
                continue;
            }
            if node.count > 0 {
                let s = node.start as usize;
                for &t in &self.order[s..s + node.count as usize] {
                    if visit(t) {
                        return true;
                    }
                }
            } else {
                stack.push(node.left as usize);
                stack.push(node.left as usize + 1);
            }
        }
        false
    }
}

/// The generalized winding number of the closed mesh `brep` at `p`
/// (Van Oosterom–Strackee solid angles, Jacobson et al. 2013).
fn winding_number(brep: &BRep, p: V3) -> f64 {
    let mesh = brep.as_mesh();
    let mut omega = 0.0;
    for tri in &mesh.tris {
        let a = sub(mesh.verts[tri[0] as usize].as_array(), p);
        let b = sub(mesh.verts[tri[1] as usize].as_array(), p);
        let c = sub(mesh.verts[tri[2] as usize].as_array(), p);
        let (la, lb, lc) = (norm(a), norm(b), norm(c));
        let num = dot(a, cross(b, c));
        let den = la * lb * lc + dot(a, b) * lc + dot(b, c) * la + dot(c, a) * lb;
        omega += 2.0 * num.atan2(den);
    }
    omega / (4.0 * std::f64::consts::PI)
}

/// The census (spec §3): for each listed shell of `a` (resp. `b`) — a
/// shell is a list of the operand's face indices — whether the other
/// operand's exact surface may reach it, and if not which side of the
/// other operand it lies on.
///
/// Errors: an operand without Stage-1 face lineage, a shell that owns no
/// Stage-1 triangle, or a winding number that is not within 0.25 of an
/// integer (an open or self-overlapping mesh) — loud, never a guess.
pub fn shell_contact_census(
    a: &BRep,
    b: &BRep,
    a_shells: &[Vec<u32>],
    b_shells: &[Vec<u32>],
) -> Result<(Vec<ShellContact>, Vec<ShellContact>), YangError> {
    let set_a = TriSet::new(a)?;
    let set_b = TriSet::new(b)?;
    let scale = set_a
        .lo
        .iter()
        .chain(set_a.hi.iter())
        .chain(set_b.lo.iter())
        .chain(set_b.hi.iter())
        .flat_map(|v| v.iter())
        .fold(0.0_f64, |m, &c| m.max(c.abs()));
    // The YR24 weld margin `union_operands_strictly_disjoint` uses: a pair
    // inside it is Stage-0's to weld, so it must read as a contact.
    let band = 2.0 * cad_primitives::TAU_MODEL.max(scale * cad_primitives::TAU_WORK);
    let bvh_a = Bvh::new(&set_a);
    let bvh_b = Bvh::new(&set_b);
    let max_dev_a = set_a.dev.iter().copied().fold(0.0, f64::max);
    let max_dev_b = set_b.dev.iter().copied().fold(0.0, f64::max);

    let verdicts = |own: &BRep,
                    own_set: &TriSet,
                    shells: &[Vec<u32>],
                    other: &BRep,
                    other_set: &TriSet,
                    other_bvh: &Bvh,
                    other_max_dev: f64|
     -> Result<Vec<ShellContact>, YangError> {
        let tri_face = own.tri_face();
        let mut out = Vec::with_capacity(shells.len());
        for shell in shells {
            let mut members: Vec<usize> = (0..tri_face.len())
                .filter(|&t| shell.contains(&tri_face[t]))
                .collect();
            if members.is_empty() {
                return Err(YangError::MalformedTopology(format!(
                    "shell contact census: shell {shell:?} owns no Stage-1 triangle"
                )));
            }
            members.sort_unstable();
            let contact = members.iter().any(|&t| {
                let pad = own_set.dev[t] + other_max_dev + band;
                other_bvh.any_near(own_set.lo[t], own_set.hi[t], pad, |s| {
                    let s = s as usize;
                    let reach = own_set.dev[t] + other_set.dev[s] + band;
                    if (0..3).any(|k| {
                        own_set.lo[t][k] > other_set.hi[s][k] + reach
                            || other_set.lo[s][k] > own_set.hi[t][k] + reach
                    }) {
                        return false;
                    }
                    let piece = |set: &TriSet, i: usize| Piece {
                        pts: set.pts[i].map(|p| p.as_array()),
                        surf: set.surf[i],
                        dev: set.dev[i],
                    };
                    let d = triangle_distance(own_set.pts[t], other_set.pts[s]);
                    let touch = d <= reach
                        && pieces_may_touch(
                            piece(own_set, t),
                            piece(other_set, s),
                            band,
                            MAX_REFINE_DEPTH,
                        );
                    if touch && std::env::var_os("YANG_SHELL_CONTACT_PROBE").is_some() {
                        eprintln!(
                            "[shell-contact] contact: tri {t} × tri {s} dist={d:.6e} reach={reach:.6e} \
                             (dev {:.6e} + {:.6e} + band {band:.3e})",
                            own_set.dev[t], other_set.dev[s]
                        );
                        eprintln!(
                            "[shell-contact]   own {:?}\n[shell-contact]   other {:?}",
                            own_set.pts[t].map(|p| p.as_array()),
                            other_set.pts[s].map(|p| p.as_array())
                        );
                    }
                    touch
                })
            });
            if contact {
                out.push(ShellContact::Contact);
                continue;
            }
            let probe = own_set.pts[members[0]][0].as_array();
            let w = winding_number(other, probe);
            let k = w.round();
            if (w - k).abs() > 0.25 || !(k == 0.0 || k == 1.0) {
                return Err(YangError::MalformedTopology(format!(
                    "shell contact census: winding number {w} of the other operand at a clear \
                     shell's vertex is not 0 or 1"
                )));
            }
            out.push(ShellContact::Clear {
                inside_other: k == 1.0,
            });
        }
        Ok(out)
    };
    let va = verdicts(a, &set_a, a_shells, b, &set_b, &bvh_b, max_dev_b)?;
    let vb = verdicts(b, &set_b, b_shells, a, &set_a, &bvh_a, max_dev_a)?;
    Ok((va, vb))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exact point-to-surface distance, the brute-force oracle for
    /// `surface_deviation`.
    fn point_surface_distance(surface: &Surface, p: V3) -> f64 {
        let unit = |v: V3| {
            let l = norm(v);
            [v[0] / l, v[1] / l, v[2] / l]
        };
        match *surface {
            Surface::Plane { .. } => 0.0,
            Surface::Sphere { center, radius } => (norm(sub(p, center.as_array())) - radius).abs(),
            Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                let u = unit(axis_dir.as_array());
                let w = sub(p, axis_point.as_array());
                let h = dot(w, u);
                ((dot(w, w) - h * h).max(0.0).sqrt() - radius).abs()
            }
            Surface::Cone {
                apex,
                axis_dir,
                half_angle,
            } => {
                let u = unit(axis_dir.as_array());
                let w = sub(p, apex.as_array());
                let h = dot(w, u).abs();
                let r = (dot(w, w) - h * h).max(0.0).sqrt();
                (h * half_angle.sin() - r * half_angle.cos()).abs()
            }
            Surface::Torus {
                center,
                axis_dir,
                major_radius,
                minor_radius,
            } => {
                let u = unit(axis_dir.as_array());
                let w = sub(p, center.as_array());
                let z = dot(w, u);
                let rho = (dot(w, w) - z * z).max(0.0).sqrt();
                (((rho - major_radius).powi(2) + z * z).sqrt() - minor_radius).abs()
            }
        }
    }

    fn brute_deviation(surface: &Surface, t: [V3; 3]) -> f64 {
        const N: usize = 400;
        let mut best = 0.0_f64;
        for i in 0..=N {
            for j in 0..=(N - i) {
                let (a, b) = (i as f64 / N as f64, j as f64 / N as f64);
                let g = 1.0 - a - b;
                let p = [
                    a * t[0][0] + b * t[1][0] + g * t[2][0],
                    a * t[0][1] + b * t[1][1] + g * t[2][1],
                    a * t[0][2] + b * t[1][2] + g * t[2][2],
                ];
                best = best.max(point_surface_distance(surface, p));
            }
        }
        best
    }

    /// `surface_deviation` is CONSERVATIVE on an inscribed triangle of every
    /// surface kind and tight (exact for plane / sphere / cylinder, within 3×
    /// for the concave tangent-plane bound of cone and outer torus) —
    /// including a cylinder sliver whose circumradius is ~10⁴ times its
    /// deviation, the R0016 class (spec `yang_452_refinement_loop.md` §3
    /// "T6 as built", draft 2).
    #[test]
    fn surface_deviation_is_exact_on_inscribed_triangles() {
        let v3 = |x: f64, y: f64, z: f64| crate::Vector3::new(x, y, z);
        let p3 = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
        let cases: Vec<(Surface, [V3; 3], &str)> = vec![
            (
                Surface::Cylinder {
                    axis_point: p3(0.0, 0.0, 0.0),
                    axis_dir: v3(0.0, 0.0, 1.0),
                    radius: 1.0,
                },
                [
                    [1.0, 0.0, 0.0],
                    [0.6_f64.cos(), 0.6_f64.sin(), 0.0],
                    [1.0, 0.0, 1.0],
                ],
                "cylinder, a fat triangle",
            ),
            (
                Surface::Cylinder {
                    axis_point: p3(0.0, 0.0, 0.0),
                    axis_dir: v3(0.0, 0.0, 1.0),
                    radius: 1.0,
                },
                [
                    [1.0, 0.0, 0.0],
                    [1.0, 0.0, 3.0],
                    [1e-4_f64.cos(), 1e-4_f64.sin(), 1.5],
                ],
                "cylinder, an axial sliver (circumradius ≫ sagitta)",
            ),
            (
                Surface::Sphere {
                    center: p3(0.0, 0.0, 0.0),
                    radius: 2.0,
                },
                [[2.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 0.0, 2.0]],
                "sphere, an octant triangle",
            ),
            (
                Surface::Cone {
                    apex: p3(0.0, 0.0, 0.0),
                    axis_dir: v3(0.0, 0.0, 1.0),
                    half_angle: 0.4,
                },
                [
                    [0.4_f64.tan(), 0.0, 1.0],
                    [
                        2.0 * 0.4_f64.tan() * 0.5_f64.cos(),
                        2.0 * 0.4_f64.tan() * 0.5_f64.sin(),
                        2.0,
                    ],
                    [
                        1.5 * 0.4_f64.tan() * 0.25_f64.cos(),
                        -1.5 * 0.4_f64.tan() * 0.25_f64.sin(),
                        1.5,
                    ],
                ],
                "cone, a generic triangle",
            ),
            (
                Surface::Cone {
                    apex: p3(0.0, 0.0, 0.0),
                    axis_dir: v3(0.0, 0.0, 1.0),
                    half_angle: 0.4,
                },
                [
                    [0.4_f64.tan(), 0.0, 1.0],
                    [3.0 * 0.4_f64.tan(), 0.0, 3.0],
                    [
                        2.0 * 0.4_f64.tan() * 1e-3_f64.cos(),
                        2.0 * 0.4_f64.tan() * 1e-3_f64.sin(),
                        2.0,
                    ],
                ],
                "cone, a generator sliver",
            ),
            (
                Surface::Torus {
                    center: p3(0.0, 0.0, 0.0),
                    axis_dir: v3(0.0, 0.0, 1.0),
                    major_radius: 3.0,
                    minor_radius: 1.0,
                },
                [
                    [4.0, 0.0, 0.0],
                    [4.0 * 0.3_f64.cos(), 4.0 * 0.3_f64.sin(), 0.0],
                    [3.0 + 0.5_f64.cos(), 0.0, 0.5_f64.sin()],
                ],
                "torus, outer region",
            ),
            (
                Surface::Torus {
                    center: p3(0.0, 0.0, 0.0),
                    axis_dir: v3(0.0, 0.0, 1.0),
                    major_radius: 3.0,
                    minor_radius: 1.0,
                },
                [
                    [2.0, 0.0, 0.0],
                    [2.0 * 0.3_f64.cos(), 2.0 * 0.3_f64.sin(), 0.0],
                    [3.0 - 0.5_f64.cos(), 0.0, 0.5_f64.sin()],
                ],
                "torus, inner region (Lipschitz-covered samples)",
            ),
        ];
        for (surface, t, name) in cases {
            for p in t {
                assert!(
                    point_surface_distance(&surface, p) < 1e-12,
                    "{name}: fixture vertex {p:?} is not on the surface"
                );
            }
            let dev = surface_deviation(&surface, t);
            let brute = brute_deviation(&surface, t);
            assert!(
                dev >= brute * (1.0 - 1e-9) - 1e-15,
                "{name}: bound {dev:.6e} below the brute maximum {brute:.6e}"
            );
            let longest = norm(sub(t[1], t[0]))
                .max(norm(sub(t[2], t[1])))
                .max(norm(sub(t[0], t[2])));
            // Tightness: the brute grid itself under-reads by at most its
            // cover (a 1-Lipschitz field), and only the inner-torus branch
            // carries its own cover slack.
            // The cone and outer-torus branches are a tangent-plane BOUND on
            // a concave maximum, within a small factor of it.
            let factor = if name.starts_with("cone") || name.contains("outer") {
                3.0
            } else {
                1.0
            };
            let slack = if name.contains("inner") {
                longest / 8.0 + longest / 400.0
            } else {
                longest / 400.0 * 2.0
            };
            assert!(
                dev <= factor * brute + slack,
                "{name}: bound {dev:.6e} loose against brute {brute:.6e} (slack {slack:.3e})"
            );
            if name.contains("sliver") {
                let rho = circumradius(t[0], t[1], t[2]);
                assert!(
                    rho > 100.0 * dev,
                    "{name}: circumradius {rho:.3e} vs deviation {dev:.3e} — not a sliver"
                );
            }
        }
    }

    #[test]
    fn closest_pair_distance_matches_triangle_distance() {
        let mut seed = 7u64;
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 11) as f64) / ((1u64 << 53) as f64) * 4.0 - 2.0
        };
        for _ in 0..500 {
            let t = [
                Point3::new(next(), next(), next()),
                Point3::new(next(), next(), next()),
                Point3::new(next(), next(), next()),
            ];
            let s = [
                Point3::new(next(), next(), next()),
                Point3::new(next(), next(), next()),
                Point3::new(next(), next(), next()),
            ];
            let d = super::triangle_distance(t, s);
            let (x, y, d2) = super::triangle_closest_pair(t, s);
            assert!((d - d2).abs() <= 1e-9 * (1.0 + d), "d {d} vs pair {d2}");
            let dxy = super::norm(super::sub(x, y));
            assert!((dxy - d2).abs() <= 1e-9 * (1.0 + d2));
        }
    }

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn crossing_triangles_are_at_distance_zero() {
        let t = [p(0.0, 0.0, 0.0), p(2.0, 0.0, 0.0), p(0.0, 2.0, 0.0)];
        let s = [p(0.5, 0.5, -1.0), p(0.5, 0.5, 1.0), p(3.0, 3.0, 0.0)];
        assert_eq!(triangle_distance(t, s), 0.0);
    }

    #[test]
    fn parallel_triangles_report_their_gap() {
        let t = [p(0.0, 0.0, 0.0), p(2.0, 0.0, 0.0), p(0.0, 2.0, 0.0)];
        let s = [p(0.0, 0.0, 0.25), p(2.0, 0.0, 0.25), p(0.0, 2.0, 0.25)];
        assert!((triangle_distance(t, s) - 0.25).abs() < 1e-15);
    }

    #[test]
    fn skew_edges_report_their_gap() {
        // Two slivers whose closest features are interior edge points.
        let t = [p(-1.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 0.0, -1.0)];
        let s = [p(0.0, -1.0, 0.5), p(0.0, 1.0, 0.5), p(0.0, 0.0, 1.5)];
        assert!((triangle_distance(t, s) - 0.5).abs() < 1e-15);
    }

    #[test]
    fn a_planar_triangle_deviates_by_nothing() {
        let s = Surface::Plane {
            normal: cad_primitives::Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        assert_eq!(
            surface_deviation(&s, [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]),
            0.0
        );
    }

    #[test]
    fn a_sphere_triangle_deviates_by_its_cap_sagitta() {
        // An equilateral triangle inscribed in a unit sphere's circle of
        // latitude at height h: its circumcircle IS that latitude circle, and
        // the cap above it rises 1 − h — the bound must be exactly that.
        let h: f64 = 0.8;
        let r = (1.0 - h * h).sqrt();
        let v = |k: f64| {
            let a = k * 2.0 * std::f64::consts::PI / 3.0;
            [r * a.cos(), r * a.sin(), h]
        };
        let s = Surface::Sphere {
            center: p(0.0, 0.0, 0.0),
            radius: 1.0,
        };
        let dev = surface_deviation(&s, [v(0.0), v(1.0), v(2.0)]);
        assert!((dev - (1.0 - h)).abs() < 1e-12, "dev {dev}");
    }
}
