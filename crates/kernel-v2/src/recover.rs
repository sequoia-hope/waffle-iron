//! PR-KV7 — boolean OUTPUT curve recovery ("output curve tagging").
//!
//! A yang boolean output carries its surviving input-rim boundaries as
//! untagged chord polylines: runs of `LineSegment` mesh edges whose
//! endpoints lie exactly on a circle that was never written down. This
//! module recovers B-Rep granularity BEFORE [`crate::boolean::from_yang_brep`]
//! classification, using the Yang-paper principle that output SURFACES are
//! exact and boundaries are surface∩surface:
//!
//! 1. **Retag**: an untagged edge between a `Cylinder` face and a `Plane`
//!    face whose plane is ⊥ the axis lies on the exact circle
//!    `cylinder ∩ plane` (computable from the two surfaces alone). The
//!    chord endpoints — Stage-1 Steiner samples and Stage-4-relocated
//!    junction vertices — are verified ON that circle within the same
//!    1e-9 import band `classify_edge` uses; the chord itself is a mesh
//!    artifact.
//! 2. **Fuse**: a vertex with exactly TWO incident undirected boundary
//!    edges, co-curve (same circle, or collinear segments) and bordering
//!    the same face pair, is a Steiner/T vertex — fuse its edges. Chains
//!    collapse to single segments, minor arcs (split ≤ ~2.6 rad so the
//!    downstream minor-arc classification never sees the π ambiguity), or
//!    closed rims.
//! 3. **Canonicalize**: a cylinder face whose two loops are both closed
//!    rims with an azimuth-aligned vertex pair becomes the canonical
//!    4-edge `[rim, seam, rim, seam]` lateral — the SAME vocabulary
//!    `construct::extrude` produces, so the assembled solid round-trips
//!    through `to_yang_brep` with no further changes. Closed rims without
//!    a canonical pairing fall back to 3 sub-π arcs (assemblable, but the
//!    face stays a re-entry wall).
//!
//! Recovery is a pure rewrite (yang output lists → yang output lists). It
//! is CONSERVATIVE: any structural anomaly bails out with the original
//! lists unchanged, so `from_yang_brep`'s pass-1 validation remains the
//! single authority on malformed outputs (P9 — recovery may only convert
//! a mesh-granular representation of the SAME geometry, never repair).

use cad_primitives::Point3;
use std::collections::BTreeMap;
use yang_rs::{BRepEdge, BRepFace, BRepVertex, Curve, Surface};

/// Maximum sweep of one emitted open arc (radians). Comfortably below the
/// π minor-arc ambiguity band even after appending one more chord of any
/// Stage-1 sampling density (a chord subtends ≤ 2π/3 at the N = 3 floor).
const MAX_ARC_PIECE_SWEEP: f64 = 2.6;

/// Relative band for on-circle membership / circle-identity / collinearity,
/// mirroring `classify_edge`'s import band (= the central
/// [`cad_primitives::TAU_EVAL`] rounding tier, F8).
const BAND: f64 = cad_primitives::TAU_EVAL;

fn sub(a: Point3, b: Point3) -> [f64; 3] {
    let (a, b) = (a.as_array(), b.as_array());
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm3(a: [f64; 3]) -> f64 {
    dot3(a, a).sqrt()
}
fn normalize3(a: [f64; 3]) -> Option<[f64; 3]> {
    let n = norm3(a);
    if n > 1e-300 && n.is_finite() {
        Some([a[0] / n, a[1] / n, a[2] / n])
    } else {
        None
    }
}

/// Deterministic orthonormal basis ⊥ `n` (unit), shared by both rims of a
/// face so azimuth comparison is frame-consistent.
fn ortho_basis(n: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let ref_v = if n[2].abs() < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let e1 = normalize3(cross3(ref_v, n)).expect("ref not parallel");
    let e2 = cross3(n, e1);
    (e1, e2)
}

/// `(axis_point, axis_dir)` (raw, un-normalized) of a cylinder/cone lateral;
/// `None` for any other surface. The cone reports its apex as the axis point.
fn curved_axis(s: &Surface) -> Option<([f64; 3], [f64; 3])> {
    match *s {
        Surface::Cylinder {
            axis_point,
            axis_dir,
            ..
        } => Some((axis_point.as_array(), axis_dir.as_array())),
        Surface::Cone { apex, axis_dir, .. } => Some((apex.as_array(), axis_dir.as_array())),
        _ => None,
    }
}

/// The on-surface rim radius of a cylinder/cone lateral at the axial foot
/// `center` (assumed on the axis), measured with the UNIT axis `a`. Constant
/// for a cylinder; `axial·tan(half_angle)` from the apex for a cone.
fn curved_radius_at(s: &Surface, center: [f64; 3], a: [f64; 3]) -> Option<f64> {
    match *s {
        Surface::Cylinder { radius, .. } => Some(radius),
        Surface::Cone {
            apex, half_angle, ..
        } => {
            let ap = apex.as_array();
            let w = [center[0] - ap[0], center[1] - ap[1], center[2] - ap[2]];
            Some(dot3(w, a).abs() * half_angle.tan())
        }
        _ => None,
    }
}

/// The effective exact curve carried by one undirected output edge.
#[derive(Clone, Copy, PartialEq, Debug)]
enum EffCurve {
    Seg,
    Circle {
        center: Point3,
        normal: [f64; 3],
        radius: f64,
    },
    Other,
}

fn same_circle(a: &EffCurve, b: &EffCurve, scale: f64) -> bool {
    let (
        EffCurve::Circle {
            center: c1,
            normal: n1,
            radius: r1,
        },
        EffCurve::Circle {
            center: c2,
            normal: n2,
            radius: r2,
        },
    ) = (a, b)
    else {
        return false;
    };
    let band = BAND * (1.0 + scale);
    norm3(sub(*c1, *c2)) <= band && (r1 - r2).abs() <= band && norm3(cross3(*n1, *n2)) <= BAND
    // same axis, either sign
}

struct VpairInfo {
    /// Sorted distinct face indices bordering this undirected edge.
    faces: Vec<usize>,
    curve: EffCurve,
}

/// One fused chain of co-curve edges.
struct Chain {
    curve: EffCurve,
    /// Ordered vertices: open `v0..=vk` (k = #edges), or closed cycle
    /// `v0..vk` with the wrap edge `vk → v0` implicit.
    verts: Vec<u32>,
    closed: bool,
    /// Replacement (start, end, curve) triples in chain order, filled
    /// after anchoring. For closed chains the canonical replacement is a
    /// single full-circle edge anchored per `anchor`.
    anchor: Option<u32>,
}

/// Recover B-Rep granularity from a yang boolean OUTPUT. Returns rewritten
/// `(vertices, edges, faces)` lists for `from_yang_brep` pass 1, or a clone
/// of the originals when recovery does not apply / bails out.
pub(crate) fn recover_output_curves(
    brep: &yang_rs::BRep,
) -> (Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>) {
    // Diagnostic probe (env-gated, zero-cost off): dump the RAW yang output
    // faces + loop edges (pre-recovery), with azimuth-around-z for each
    // endpoint, so a mangled boundary self-localizes before fuse/canonicalize.
    if std::env::var_os("KV2_RECOVER_PROBE").is_some() {
        let az = |p: Point3| p.y().atan2(p.x());
        for (fi, f) in brep.faces().iter().enumerate() {
            eprintln!("[recover-probe] face {fi} surface={:?}", f.surface);
            for (li, lp) in std::iter::once(&f.outer_loop)
                .chain(f.inner_loops.iter())
                .enumerate()
            {
                eprintln!("  loop {li}: {} edges", lp.len());
                for &ei in lp.iter() {
                    let e = &brep.edges()[ei as usize];
                    let ps = brep.vertices()[e.start as usize].point;
                    let pe = brep.vertices()[e.end as usize].point;
                    let r = |p: Point3| (p.x() * p.x() + p.y() * p.y()).sqrt();
                    eprintln!(
                        "    e{ei} {:?} v{}(az {:.4}, r {:.6}, z {:.4}) -> v{}(az {:.4}, r {:.6}, z {:.4})",
                        std::mem::discriminant(&e.curve),
                        e.start,
                        az(ps),
                        r(ps),
                        ps.z(),
                        e.end,
                        az(pe),
                        r(pe),
                        pe.z(),
                    );
                }
            }
        }
    }
    let orig = || {
        (
            brep.vertices().to_vec(),
            brep.edges().to_vec(),
            brep.faces().to_vec(),
        )
    };
    match try_recover(brep) {
        Some(out) => out,
        None => orig(),
    }
}

#[allow(clippy::type_complexity)]
fn try_recover(brep: &yang_rs::BRep) -> Option<(Vec<BRepVertex>, Vec<BRepEdge>, Vec<BRepFace>)> {
    let yverts = brep.vertices();
    let yedges = brep.edges();
    let yfaces = brep.faces();

    let scale = yverts
        .iter()
        .map(|v| {
            let p = v.point.as_array();
            p[0].abs().max(p[1].abs()).max(p[2].abs())
        })
        .fold(0.0f64, f64::max);
    let band = BAND * (1.0 + scale);

    // ---- directed loop walks (same chaining rule as from_yang pass 1) ----
    // loop_walks[i] = (face, is_outer, Vec<(edge_idx, from, to)>)
    struct LoopWalk {
        face: usize,
        steps: Vec<(u32, u32, u32)>,
    }
    let mut loop_walks: Vec<LoopWalk> = Vec::new();
    for (fi, f) in yfaces.iter().enumerate() {
        for loop_edges in std::iter::once(&f.outer_loop).chain(f.inner_loops.iter()) {
            if loop_edges.is_empty() {
                return None;
            }
            let mut steps = Vec::with_capacity(loop_edges.len());
            let mut cur: Option<u32> = None;
            for &ei in loop_edges {
                let e = yedges.get(ei as usize)?;
                if (e.start as usize) >= yverts.len() || (e.end as usize) >= yverts.len() {
                    return None;
                }
                let (from, to) = match cur {
                    None => (e.start, e.end),
                    Some(c) if e.start == c => (e.start, e.end),
                    Some(c) if e.end == c => (e.end, e.start),
                    Some(_) => return None,
                };
                steps.push((ei, from, to));
                cur = Some(to);
            }
            if cur != Some(steps[0].1) {
                return None;
            }
            loop_walks.push(LoopWalk { face: fi, steps });
        }
    }

    // ---- vpair census ------------------------------------------------------
    let mut vpairs: BTreeMap<(u32, u32), VpairInfo> = BTreeMap::new();
    for lw in &loop_walks {
        for &(ei, from, to) in &lw.steps {
            if from == to {
                // Already-canonical self-pair edge: nothing to recover here;
                // bail to keep the pass conservative (outputs from the mesh
                // pipeline never carry these today).
                return None;
            }
            let key = (from.min(to), from.max(to));
            let e = &yedges[ei as usize];
            let curve = match e.curve {
                Curve::LineSegment => EffCurve::Seg,
                Curve::Circle {
                    center,
                    normal,
                    radius,
                } => {
                    // Sign-canonicalize (first nonzero component positive):
                    // a circle's POINT SET is normal-sign-invariant, and
                    // yang Stage 6 orients each directed edge copy's normal
                    // for its own traversal (task #133, spec
                    // `yang_stage6_arc_orientation`) — so twin copies carry
                    // NEGATED normals and the exact twin-consistency check
                    // below must compare canonical forms. Negation is exact
                    // in f64; downstream chain marches are sign-robust and
                    // `from_yang` re-derives every arc's directional sense.
                    let n = normalize3(normal.as_array())?;
                    let flip = match n.iter().find(|v| **v != 0.0) {
                        Some(v) => *v < 0.0,
                        None => false,
                    };
                    let n = if flip { [-n[0], -n[1], -n[2]] } else { n };
                    EffCurve::Circle {
                        center,
                        normal: n,
                        radius,
                    }
                }
                _ => EffCurve::Other,
            };
            let info = vpairs.entry(key).or_insert_with(|| VpairInfo {
                faces: Vec::new(),
                curve,
            });
            if info.curve != curve {
                return None; // inconsistent twin curves: leave to pass 1
            }
            if !info.faces.contains(&lw.face) {
                info.faces.push(lw.face);
                info.faces.sort_unstable();
            }
        }
    }
    // Manifold precondition for recovery: every vpair is used by loops of
    // one or two faces (two faces, or twice within one face).
    if vpairs
        .values()
        .any(|i| i.faces.is_empty() || i.faces.len() > 2)
    {
        return None;
    }

    // ---- retag: cylinder × ⊥-plane chords → exact circles ----------------
    for (key, info) in vpairs.iter_mut() {
        if info.curve != EffCurve::Seg || info.faces.len() != 2 {
            continue;
        }
        let (s0, s1) = (
            &yfaces[info.faces[0]].surface,
            &yfaces[info.faces[1]].surface,
        );
        // A cylinder or cone lateral meeting a plane ⊥ its axis makes a rim
        // CIRCLE (a cone frustum survives a flat cut as a whole two-rim band —
        // KV6c 5c). `curved` is whichever surface is the lateral.
        let (curved, pl) = match (s0, s1) {
            (Surface::Cylinder { .. } | Surface::Cone { .. }, Surface::Plane { .. }) => (s0, s1),
            (Surface::Plane { .. }, Surface::Cylinder { .. } | Surface::Cone { .. }) => (s1, s0),
            // KV7 extension — curved ∩ curved coaxial rim. Two coaxial
            // cylinder/cone laterals (e.g. the stacked bands of a
            // partial-revolve gear) meet at a shared rim CIRCLE. The mesh
            // boolean leaves that rim as an untagged chord run; because
            // neither face is a plane the ⊥-plane retag above never fires, so
            // the coarse chord polyline survives into render — its chord-split
            // midpoints (necessarily on-chord for watertightness) then fold
            // against the on-surface interior in a thin band (the KV9-F2
            // "patch triangulation folded" render failure). Recover the exact
            // circle from the shared axis: the rim IS `surface0 ∩ surface1`, an
            // exact circle on BOTH laterals (analytical primacy, A15).
            //
            // Guards (scale-relative `band`, matching the ⊥-plane path):
            //   1. the two laterals are coaxial (parallel axis directions),
            //   2. the chord endpoints share an axial coordinate — a
            //      ruling/seam spans height and is correctly excluded,
            //   3. both endpoints lie on the surface-0 rim circle,
            //   4. the arc MIDPOINT lies on surface 1 — the definitive guard
            //      that the recovered circle is genuinely SHARED (a skew or
            //      laterally-offset pair fails here even if 1–3 pass).
            _ => {
                let (Some((ap0, ad0)), Some((_, ad1))) = (curved_axis(s0), curved_axis(s1)) else {
                    continue;
                };
                let (Some(a), Some(a1)) = (normalize3(ad0), normalize3(ad1)) else {
                    continue;
                };
                if (dot3(a, a1).abs() - 1.0).abs() > band {
                    continue; // guard 1: not coaxial
                }
                let axial = |v: u32| {
                    let p = yverts[v as usize].point.as_array();
                    dot3([p[0] - ap0[0], p[1] - ap0[1], p[2] - ap0[2]], a)
                };
                let (h0, h1) = (axial(key.0), axial(key.1));
                if (h0 - h1).abs() > band {
                    continue; // guard 2: a ruling/seam, not a rim
                }
                let h = 0.5 * (h0 + h1);
                let center = Point3::new(ap0[0] + h * a[0], ap0[1] + h * a[1], ap0[2] + h * a[2]);
                let Some(radius) = curved_radius_at(s0, center.as_array(), a) else {
                    continue;
                };
                if !radius.is_finite() || radius <= 0.0 {
                    continue;
                }
                let on_circle = |v: u32| -> bool {
                    let w = sub(yverts[v as usize].point, center);
                    let ax = dot3(w, a);
                    let radial = (dot3(w, w) - ax * ax).max(0.0).sqrt();
                    ax.abs() <= band && (radial - radius).abs() <= band
                };
                if !(on_circle(key.0) && on_circle(key.1)) {
                    continue; // guard 3
                }
                // Guard 4: arc midpoint (mean azimuth on the surface-0 circle)
                // must lie on surface 1.
                let (e1, e2) = ortho_basis(a);
                let azimuth = |v: u32| {
                    let w = sub(yverts[v as usize].point, center);
                    dot3(w, e2).atan2(dot3(w, e1))
                };
                let tm = 0.5 * (azimuth(key.0) + azimuth(key.1));
                let (sn, cs) = tm.sin_cos();
                let m = [
                    center.x() + radius * (cs * e1[0] + sn * e2[0]),
                    center.y() + radius * (cs * e1[1] + sn * e2[1]),
                    center.z() + radius * (cs * e1[2] + sn * e2[2]),
                ];
                let (Some(rm1), Some((ap1, _))) = (curved_radius_at(s1, m, a1), curved_axis(s1))
                else {
                    continue;
                };
                let w1 = [m[0] - ap1[0], m[1] - ap1[1], m[2] - ap1[2]];
                let ax1 = dot3(w1, a1);
                let radial1 = (dot3(w1, w1) - ax1 * ax1).max(0.0).sqrt();
                if (radial1 - rm1).abs() > band {
                    continue; // guard 4: circle not shared with surface 1
                }
                info.curve = EffCurve::Circle {
                    center,
                    normal: a,
                    radius,
                };
                continue;
            }
        };
        let Surface::Plane { normal, d } = *pl else {
            unreachable!()
        };
        // Axis point (apex for a cone) + direction of the lateral.
        let (axis_point, axis_dir) = match *curved {
            Surface::Cylinder {
                axis_point,
                axis_dir,
                ..
            } => (axis_point, axis_dir),
            Surface::Cone { apex, axis_dir, .. } => (apex, axis_dir),
            _ => unreachable!(),
        };
        let Some(a) = normalize3(axis_dir.as_array()) else {
            continue;
        };
        let Some(n) = normalize3(normal.as_array()) else {
            continue;
        };
        // Plane must be ⊥ the axis (a rim plane). Parallel planes make
        // LINES (left as segments); an oblique plane on a cone makes a conic
        // (ellipse/parabola/hyperbola — out of the circle-recovery vocabulary).
        let c = dot3(n, a);
        if (c.abs() - 1.0).abs() > BAND {
            continue;
        }
        // center = axis ∩ plane, with the plane n·x + d = 0 (normal not
        // assumed unit in storage; use the unit form n̂·x + d/|n| = 0).
        let nlen = norm3(normal.as_array());
        let p0 = axis_point.as_array();
        let t = -(dot3(n, p0) + d / nlen) / c;
        let center = Point3::new(p0[0] + t * a[0], p0[1] + t * a[1], p0[2] + t * a[2]);
        // Rim radius: constant for a cylinder; `axial·tan(half_angle)` from the
        // apex for a cone (the center's axial coordinate IS that distance).
        let radius = match *curved {
            Surface::Cylinder { radius, .. } => radius,
            Surface::Cone { half_angle, .. } => {
                let w = sub(center, axis_point);
                dot3(w, a).abs() * half_angle.tan()
            }
            _ => unreachable!(),
        };
        if !radius.is_finite() || radius <= 0.0 {
            continue;
        }
        // Both endpoints exactly on the circle?
        let on_circle = |v: u32| -> bool {
            let w = sub(yverts[v as usize].point, center);
            let axial = dot3(w, a);
            let radial = (dot3(w, w) - axial * axial).max(0.0).sqrt();
            axial.abs() <= band && (radial - radius).abs() <= band
        };
        if on_circle(key.0) && on_circle(key.1) {
            info.curve = EffCurve::Circle {
                center,
                normal: a,
                radius,
            };
        }
    }

    // ---- fusible vertices --------------------------------------------------
    let mut incident: BTreeMap<u32, Vec<(u32, u32)>> = BTreeMap::new();
    for &key in vpairs.keys() {
        incident.entry(key.0).or_default().push(key);
        incident.entry(key.1).or_default().push(key);
    }
    let other_end = |key: (u32, u32), v: u32| if key.0 == v { key.1 } else { key.0 };
    let mut fused: BTreeMap<u32, [(u32, u32); 2]> = BTreeMap::new();
    for (&v, inc) in &incident {
        if inc.len() != 2 {
            continue;
        }
        let (k1, k2) = (inc[0], inc[1]);
        let (i1, i2) = (&vpairs[&k1], &vpairs[&k2]);
        if i1.faces != i2.faces {
            continue;
        }
        let co_curve = match (&i1.curve, &i2.curve) {
            (EffCurve::Seg, EffCurve::Seg) => {
                // Collinear through v (anti-parallel arms).
                let p = yverts[v as usize].point;
                let arm1 = sub(yverts[other_end(k1, v) as usize].point, p);
                let arm2 = sub(yverts[other_end(k2, v) as usize].point, p);
                let (l1, l2) = (norm3(arm1), norm3(arm2));
                l1 > 0.0
                    && l2 > 0.0
                    && norm3(cross3(arm1, arm2)) <= BAND * l1 * l2
                    && dot3(arm1, arm2) < 0.0
            }
            (a @ EffCurve::Circle { .. }, b @ EffCurve::Circle { .. }) => same_circle(a, b, scale),
            _ => false,
        };
        if co_curve {
            fused.insert(v, [k1, k2]);
        }
    }
    if fused.is_empty() {
        return None; // nothing to recover
    }

    // ---- chains -------------------------------------------------------------
    let mut chain_of: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    let mut chains: Vec<Chain> = Vec::new();
    for &start_key in vpairs.keys() {
        if chain_of.contains_key(&start_key) {
            continue;
        }
        // Walk from this edge in both directions through fused vertices.
        let mut keys = vec![start_key];
        let mut verts = vec![start_key.0, start_key.1];
        let mut closed = false;
        // extend forward from verts.last(), backward from verts[0]
        for dir in 0..2 {
            loop {
                let (end_v, prev_key) = if dir == 0 {
                    (*verts.last().unwrap(), *keys.last().unwrap())
                } else {
                    (verts[0], keys[0])
                };
                let Some(pair) = fused.get(&end_v) else { break };
                let next_key = if pair[0] == prev_key {
                    pair[1]
                } else {
                    pair[0]
                };
                if next_key == keys[0] && dir == 0 && keys.len() > 1 {
                    closed = true; // walked all the way around
                    verts.pop(); // last == first; drop duplicate
                    break;
                }
                let nv = other_end(next_key, end_v);
                if dir == 0 {
                    keys.push(next_key);
                    verts.push(nv);
                } else {
                    keys.insert(0, next_key);
                    verts.insert(0, nv);
                }
            }
            if closed {
                break;
            }
        }
        let curve = vpairs[&start_key].curve;
        let id = chains.len();
        for k in &keys {
            chain_of.insert(*k, id);
        }
        chains.push(Chain {
            curve,
            verts,
            closed,
            anchor: None,
        });
    }

    // ---- canonical lateral pairing -----------------------------------------
    // face -> (outer chain ids per loop) for cylinder faces whose every loop
    // is exactly one closed circle chain.
    let mut lateral_pairs: BTreeMap<usize, ((usize, u32), (usize, u32))> = BTreeMap::new();
    {
        // Group this face's loops by chain.
        let mut face_loop_chains: BTreeMap<usize, Vec<Option<usize>>> = BTreeMap::new();
        for lw in &loop_walks {
            let entry = face_loop_chains.entry(lw.face).or_default();
            let mut ids: Vec<usize> = Vec::new();
            for &(_, from, to) in &lw.steps {
                let key = (from.min(to), from.max(to));
                match chain_of.get(&key) {
                    Some(&c) => ids.push(c),
                    None => {
                        ids.clear();
                        break;
                    }
                }
            }
            ids.dedup();
            let single = if ids.len() == 1 && chains[ids[0]].closed {
                Some(ids[0])
            } else {
                None
            };
            entry.push(single);
        }
        for (&fi, loop_chains) in &face_loop_chains {
            // Diagnostic probe (env-gated): per-face loop→chain resolution, so
            // a canonicalization miss self-localizes (which loop failed to be
            // a single closed chain, and what its chains look like).
            if std::env::var_os("KV2_RECOVER_PROBE").is_some() {
                eprintln!(
                    "[recover-probe] canonicalize face {fi} surface={:?} loop_chains={loop_chains:?}",
                    yfaces[fi].surface
                );
            }
            // Cylinder OR cone laterals canonicalize identically: two closed
            // rims joined by one azimuth-aligned seam ruling (axis-parallel for
            // a cylinder, a slant generator for a cone — both connect equal
            // azimuths). The cone uses its larger rim radius for the angular
            // band (KV6c 5c).
            let (axis_dir, radius) = match yfaces[fi].surface {
                Surface::Cylinder {
                    axis_dir, radius, ..
                } => (axis_dir, radius),
                Surface::Cone { axis_dir, .. } => {
                    // Representative radius for the angular band = the larger rim
                    // radius (the two cone rims differ).
                    let r = loop_chains
                        .iter()
                        .flatten()
                        .filter_map(|&ci| match chains[ci].curve {
                            EffCurve::Circle { radius, .. } => Some(radius),
                            _ => None,
                        })
                        .fold(0.0f64, f64::max);
                    (axis_dir, r)
                }
                _ => continue,
            };
            let [Some(ca), Some(cb)] = loop_chains.as_slice() else {
                continue;
            };
            let (ca, cb) = (*ca, *cb);
            if !matches!(chains[ca].curve, EffCurve::Circle { .. })
                || !matches!(chains[cb].curve, EffCurve::Circle { .. })
            {
                continue;
            }
            let Some(a) = normalize3(axis_dir.as_array()) else {
                continue;
            };
            let (e1, e2) = ortho_basis(a);
            let EffCurve::Circle { center: cca, .. } = chains[ca].curve else {
                continue;
            };
            let EffCurve::Circle { center: ccb, .. } = chains[cb].curve else {
                continue;
            };
            let az = |v: u32, c: Point3| -> f64 {
                let w = sub(yverts[v as usize].point, c);
                dot3(w, e2).atan2(dot3(w, e1))
            };
            // Find the azimuth-aligned vertex pair (deterministic: smallest
            // |Δaz|, ties by (va, vb) index order).
            let mut best: Option<(f64, u32, u32)> = None;
            for &va in &chains[ca].verts {
                let ta = az(va, cca);
                for &vb in &chains[cb].verts {
                    let tb = az(vb, ccb);
                    let mut daz = (ta - tb).abs();
                    if daz > std::f64::consts::PI {
                        daz = 2.0 * std::f64::consts::PI - daz;
                    }
                    let better = match best {
                        None => true,
                        Some((d, pa, pb)) => daz < d || (daz == d && (va, vb) < (pa, pb)),
                    };
                    if better {
                        best = Some((daz, va, vb));
                    }
                }
            }
            // The aligned pair's chord must be a true ruling: azimuth-equal
            // within the angular band (length-scaled by radius).
            let Some((daz, va, vb)) = best else { continue };
            if daz * radius > band {
                continue;
            }
            chains[ca].anchor = Some(va);
            chains[cb].anchor = Some(vb);
            lateral_pairs.insert(fi, ((ca, va), (cb, vb)));
        }
    }

    // ---- replacement edge builders -----------------------------------------
    // For an OPEN circle chain: sub-π arc pieces, split at retained chain
    // vertices so no piece's sweep reaches the minor-arc ambiguity band.
    // Returns (start, end) vertex pairs in chain order.
    let open_circle_pieces = |chain: &Chain| -> Option<Vec<(u32, u32)>> {
        let EffCurve::Circle { center, normal, .. } = chain.curve else {
            return None;
        };
        let (e1, e2) = ortho_basis(normal);
        let theta = |v: u32| -> f64 {
            let w = sub(yverts[v as usize].point, center);
            dot3(w, e2).atan2(dot3(w, e1))
        };
        // March direction: per-chord CCW deltas around `normal` are either
        // all small (CCW march) or all near 2π (CW march); normalize so
        // deltas are the small positive ones.
        let tau = 2.0 * std::f64::consts::PI;
        let deltas: Vec<f64> = chain
            .verts
            .windows(2)
            .map(|w| (theta(w[1]) - theta(w[0])).rem_euclid(tau))
            .collect();
        let cw = deltas.iter().filter(|&&d| d > std::f64::consts::PI).count();
        let deltas: Vec<f64> = if cw * 2 > deltas.len() {
            deltas.iter().map(|d| tau - d).collect()
        } else {
            deltas
        };
        if deltas
            .iter()
            .any(|&d| !(d > 0.0 && d < MAX_ARC_PIECE_SWEEP))
        {
            return None; // degenerate or absurd chord — bail
        }
        let mut pieces = Vec::new();
        let mut start = chain.verts[0];
        let mut acc = 0.0;
        for (i, &d) in deltas.iter().enumerate() {
            if acc > 0.0 && acc + d > MAX_ARC_PIECE_SWEEP {
                pieces.push((start, chain.verts[i]));
                start = chain.verts[i];
                acc = 0.0;
            }
            acc += d;
        }
        pieces.push((start, *chain.verts.last().unwrap()));
        Some(pieces)
    };
    // For a CLOSED chain with no canonical anchor: sub-π arc pieces split by
    // ACCUMULATED SWEEP, exactly like the open-chain builder (task #62, the
    // F0086 chained swiss-cheese wall). The former vertex-count-thirds split
    // was wrong for NON-UNIFORM chord spacing (coplanar rim-override
    // clusters pack dozens of femto-spaced vertices into a fraction of a
    // radian): a count-third can subtend MORE than π, and the downstream
    // minor-side arc classification then reconstructs the wrong side — the
    // rim loop walks out-and-back with net winding 0 ("cylinder patch must
    // have exactly 0 or 2 axis-wrapping loops"). Sweep-based cuts guarantee
    // every piece < MAX_ARC_PIECE_SWEEP < π, and ≥ 3 pieces fall out of
    // 2π / MAX_ARC_PIECE_SWEEP > 2 (loop arity + distinct endpoint pairs).
    let closed_fallback_pieces = |chain: &Chain| -> Option<Vec<(u32, u32)>> {
        let EffCurve::Circle { center, normal, .. } = chain.curve else {
            return None;
        };
        let n = chain.verts.len();
        if n < 3 {
            return None;
        }
        let (e1, e2) = ortho_basis(normal);
        let theta = |v: u32| -> f64 {
            let w = sub(yverts[v as usize].point, center);
            dot3(w, e2).atan2(dot3(w, e1))
        };
        let tau = 2.0 * std::f64::consts::PI;
        // Per-chord CCW deltas INCLUDING the implicit wrap edge vk → v0.
        let mut deltas: Vec<f64> = chain
            .verts
            .windows(2)
            .map(|w| (theta(w[1]) - theta(w[0])).rem_euclid(tau))
            .collect();
        deltas.push((theta(chain.verts[0]) - theta(*chain.verts.last().unwrap())).rem_euclid(tau));
        // March direction normalization (same rule as the open builder).
        let cw = deltas.iter().filter(|&&d| d > std::f64::consts::PI).count();
        let deltas: Vec<f64> = if cw * 2 > deltas.len() {
            deltas.iter().map(|d| tau - d).collect()
        } else {
            deltas
        };
        if deltas
            .iter()
            .any(|&d| !(d > 0.0 && d < MAX_ARC_PIECE_SWEEP))
        {
            return None; // degenerate / zigzag / absurd chord — bail
        }
        let mut pieces: Vec<(u32, u32)> = Vec::new();
        let mut start = chain.verts[0];
        let mut acc = 0.0;
        for (i, &d) in deltas.iter().enumerate() {
            if acc > 0.0 && acc + d > MAX_ARC_PIECE_SWEEP {
                pieces.push((start, chain.verts[i]));
                start = chain.verts[i];
                acc = 0.0;
            }
            acc += d;
        }
        pieces.push((start, chain.verts[0])); // close the cycle
        if pieces.len() < 3 {
            return None; // cannot happen for a true 2π cycle — bail loudly
        }
        Some(pieces)
    };

    // Pre-compute each chain's replacement endpoint list (in chain order).
    let mut chain_pieces: Vec<Vec<(u32, u32)>> = Vec::with_capacity(chains.len());
    for chain in &chains {
        let pieces = match (&chain.curve, chain.closed) {
            (EffCurve::Seg, false) => {
                vec![(chain.verts[0], *chain.verts.last().unwrap())]
            }
            (EffCurve::Seg, true) => return None, // degenerate closed polyline
            (EffCurve::Circle { .. }, false) => open_circle_pieces(chain)?,
            (EffCurve::Circle { .. }, true) => match chain.anchor {
                Some(a) => vec![(a, a)], // canonical full-circle edge
                None => closed_fallback_pieces(chain)?,
            },
            (EffCurve::Other, _) => return None,
        };
        chain_pieces.push(pieces);
    }

    // PR-KV9: minimum-loop-arity repair. Fusion can collapse a LENS face
    // (two arcs between the same two vertices — parallel cylinder×cylinder
    // caps) to a 2-edge loop, which the assembler's vertex-pair edge keying
    // cannot represent (two distinct edges would share one undirected key).
    // Split chains at retained interior vertices until every loop emits ≥3
    // edges with no duplicated endpoint pair — the SAME exact curve, one
    // more vertex, applied at CHAIN level so all faces sharing the chain
    // stay consistent.
    {
        // Per-loop chain runs (each open chain appears as one run).
        let mut changed = true;
        while changed {
            changed = false;
            for lw in &loop_walks {
                let mut run_chains: Vec<usize> = Vec::new();
                for &(_, from, to) in &lw.steps {
                    let key = (from.min(to), from.max(to));
                    if let Some(&c) = chain_of.get(&key) {
                        if run_chains.last() != Some(&c) {
                            run_chains.push(c);
                        }
                    }
                }
                run_chains.dedup();
                if run_chains.is_empty() {
                    continue;
                }
                let total: usize = run_chains.iter().map(|&c| chain_pieces[c].len()).sum();
                if total >= 3 || run_chains.iter().any(|&c| chains[c].closed) {
                    continue;
                }
                // Split the chain with the most retained interior vertices.
                let Some(&pick) = run_chains.iter().max_by_key(|&&c| chains[c].verts.len()) else {
                    continue;
                };
                let chain = &chains[pick];
                if chain.verts.len() < 3 {
                    // No interior vertex anywhere to split at — a 2-mesh-edge
                    // lens; bail conservatively (original lists keep the mesh
                    // granularity, which has ≥3 edges per loop by
                    // construction).
                    return None;
                }
                // Re-split the FIRST oversized piece at the chain vertex
                // nearest its middle (deterministic).
                let pieces = &mut chain_pieces[pick];
                let Some(big_idx) = (0..pieces.len()).find(|&i| {
                    let (ps, pe) = pieces[i];
                    let si = chain.verts.iter().position(|&v| v == ps);
                    let ei = chain.verts.iter().position(|&v| v == pe);
                    matches!((si, ei), (Some(a), Some(b)) if b > a + 1)
                }) else {
                    return None; // every piece is a single mesh edge already
                };
                let (ps, pe) = pieces[big_idx];
                let a = chain.verts.iter().position(|&v| v == ps).unwrap();
                let b = chain.verts.iter().position(|&v| v == pe).unwrap();
                let mid = chain.verts[(a + b) / 2];
                pieces.splice(big_idx..=big_idx, [(ps, mid), (mid, pe)]);
                changed = true;
            }
        }
    }

    let chain_curve_for_edge = |chain: &Chain| -> Curve {
        match chain.curve {
            EffCurve::Seg => Curve::LineSegment,
            EffCurve::Circle {
                center,
                normal,
                radius,
            } => Curve::Circle {
                center,
                normal: cad_primitives::Vector3::new(normal[0], normal[1], normal[2]),
                radius,
            },
            EffCurve::Other => Curve::LineSegment,
        }
    };

    // ---- rewrite ------------------------------------------------------------
    let mut new_edges: Vec<BRepEdge> = Vec::new();
    let mut new_faces: Vec<BRepFace> = Vec::with_capacity(yfaces.len());
    let push_edge = |edges: &mut Vec<BRepEdge>, start: u32, end: u32, curve: Curve| -> u32 {
        let i = edges.len() as u32;
        edges.push(BRepEdge { start, end, curve });
        i
    };

    // Walk loop_walks grouped per face (they were generated in face order,
    // outer first).
    let mut walks_by_face: BTreeMap<usize, Vec<&LoopWalk>> = BTreeMap::new();
    for lw in &loop_walks {
        walks_by_face.entry(lw.face).or_default().push(lw);
    }

    for (fi, f) in yfaces.iter().enumerate() {
        // Canonical lateral: replace the whole face with the 4-edge form.
        if let Some(&((ca, va), (cb, vb))) = lateral_pairs.get(&fi) {
            let curve_a = chain_curve_for_edge(&chains[ca]);
            let curve_b = chain_curve_for_edge(&chains[cb]);
            let e0 = push_edge(&mut new_edges, va, va, curve_a);
            let e1 = push_edge(&mut new_edges, va, vb, Curve::LineSegment);
            let e2 = push_edge(&mut new_edges, vb, vb, curve_b);
            let e3 = push_edge(&mut new_edges, vb, va, Curve::LineSegment);
            new_faces.push(BRepFace {
                surface: f.surface,
                outer_loop: vec![e0, e1, e2, e3],
                inner_loops: vec![],
                reversed: f.reversed,
            });
            continue;
        }

        let mut loops_out: Vec<Vec<u32>> = Vec::new();
        for lw in &walks_by_face[&fi] {
            let steps = &lw.steps;
            let m = steps.len();
            let mut out: Vec<u32> = Vec::new();
            // Group consecutive steps by chain membership. Rotate the walk so
            // it does not start mid-chain (unless the whole loop is one
            // chain).
            let key_at = |k: usize| -> (u32, u32) {
                (steps[k].1.min(steps[k].2), steps[k].1.max(steps[k].2))
            };
            let chain_at = |k: usize| -> Option<usize> { chain_of.get(&key_at(k)).copied() };
            let mut start = 0usize;
            if m > 1 {
                // find a boundary where chain changes (or an unchained step)
                let mut found = false;
                for k in 0..m {
                    let prev = (k + m - 1) % m;
                    if chain_at(k).is_none()
                        || chain_at(prev).is_none()
                        || chain_at(k) != chain_at(prev)
                    {
                        start = k;
                        found = true;
                        break;
                    }
                }
                if !found {
                    // Whole loop is one closed chain. Emit the replacement
                    // pieces in the LOOP's traversal direction (the chain's
                    // stored cycle order is a walk artifact; a planar ring's
                    // winding is semantic).
                    let cid = chain_at(0)?;
                    let chain = &chains[cid];
                    if !chain.closed {
                        return None;
                    }
                    let (from0, to0) = (steps[0].1, steps[0].2);
                    let n = chain.verts.len();
                    let pos = chain.verts.iter().position(|&v| v == from0)?;
                    let forward = chain.verts[(pos + 1) % n] == to0;
                    let curve = chain_curve_for_edge(chain);
                    let pieces = &chain_pieces[cid];
                    if forward {
                        for &(s, e) in pieces {
                            out.push(push_edge(&mut new_edges, s, e, curve));
                        }
                    } else {
                        for &(s, e) in pieces.iter().rev() {
                            out.push(push_edge(&mut new_edges, e, s, curve));
                        }
                    }
                    loops_out.push(out);
                    continue;
                }
            }
            let mut k = 0usize;
            while k < m {
                let idx = (start + k) % m;
                match chain_at(idx) {
                    None => {
                        // Unchained step: copy, applying any retag.
                        let (_, from, to) = steps[idx];
                        let key = key_at(idx);
                        let curve = match vpairs[&key].curve {
                            EffCurve::Seg => Curve::LineSegment,
                            EffCurve::Circle {
                                center,
                                normal,
                                radius,
                            } => Curve::Circle {
                                center,
                                normal: cad_primitives::Vector3::new(
                                    normal[0], normal[1], normal[2],
                                ),
                                radius,
                            },
                            EffCurve::Other => yedges[steps[idx].0 as usize].curve,
                        };
                        out.push(push_edge(&mut new_edges, from, to, curve));
                        k += 1;
                    }
                    Some(cid) => {
                        // Consume the full run of this chain.
                        let mut run = 1usize;
                        while run < m && chain_at((start + k + run) % m) == Some(cid) {
                            run += 1;
                        }
                        let chain = &chains[cid];
                        if chain.closed || run != chain.verts.len() - 1 {
                            // A closed chain inside a multi-chain loop, or a
                            // partial run of an open chain: structure we do
                            // not expect — bail conservatively.
                            return None;
                        }
                        let entry = steps[(start + k) % m].1;
                        let forward = entry == chain.verts[0];
                        if !forward && entry != *chain.verts.last().unwrap() {
                            return None;
                        }
                        let curve = chain_curve_for_edge(chain);
                        let pieces = &chain_pieces[cid];
                        if forward {
                            for &(s, e) in pieces {
                                out.push(push_edge(&mut new_edges, s, e, curve));
                            }
                        } else {
                            for &(s, e) in pieces.iter().rev() {
                                out.push(push_edge(&mut new_edges, e, s, curve));
                            }
                        }
                        k += run;
                    }
                }
            }
            loops_out.push(out);
        }
        // Canonicalize loop rotation: start at a LineSegment edge when one
        // exists, mirroring the constructor convention (construct::revolve
        // emits [seg, arc, seg, arc]). Besides determinism, this avoids a
        // LATENT rotation sensitivity in `tessellate_cylinder_patch`
        // (KV7-F1): a partial-lateral loop starting mid-arc folds its
        // unrolled triangulation at specific chord densities (reproduced on
        // the R0084 oblique revolve at rel tol 1e-3). The patch path bug is
        // logged in the roadmap; this rotation is a valid canonical form on
        // its own, not the fix.
        let loops_out: Vec<Vec<u32>> = loops_out
            .into_iter()
            .map(|lp| {
                if let Some(k) = lp
                    .iter()
                    .position(|&ei| matches!(new_edges[ei as usize].curve, Curve::LineSegment))
                {
                    let mut r = lp.clone();
                    r.rotate_left(k);
                    r
                } else {
                    lp
                }
            })
            .collect();
        let mut it = loops_out.into_iter();
        let outer = it.next()?;
        new_faces.push(BRepFace {
            surface: f.surface,
            outer_loop: outer,
            inner_loops: it.collect(),
            reversed: f.reversed,
        });
    }

    Some((yverts.to_vec(), new_edges, new_faces))
}
