//! P3a #146 conformal junction sampling — increment 1a: the pure pierce-point
//! primitive (spec `specs/yang_146_conformal_junction_sampling.md` §3.2/§4).
//!
//! UNWIRED: nothing in the production pipeline calls this yet. The
//! increment-2 wiring feeds its output into Stage-1 edge overrides behind
//! `YANG_JUNCTION_SAMPLING_ENABLE`; until then the only consumers are the
//! unit fixtures.
//!
//! Increment-1a scope (fail-closed — a missed mint is status quo, never
//! worse, spec §6):
//! - edges: `Curve::LineSegment` only (the F0082 lead-customer class; the
//!   chord IS the curve so the parameter-range test is exact);
//! - partner faces: `Surface::Plane` only, with exact closed-form pierce
//!   (line ∩ plane) and exact 2D bounded-face containment. Curved partner
//!   faces are conservatively skipped (increment 1b adds their containment).
//!
//! Every gate is derived, not tuned:
//! - endpoint margin `TAU_MODEL·(1+scale)`: a pierce at/near an edge
//!   endpoint is a CORNER junction — P3b stitch territory, not a mid-edge
//!   sample (spec §6);
//! - boundary margin `TAU_MODEL·(1+scale)` inside the partner face: a
//!   pierce on the partner's own boundary edge is likewise a corner;
//! - transversality floor `1e-9` on `|t̂·n̂|` (the sine of the line/surface
//!   angle; same collinearity band as the backtrack-spike test): below it
//!   the contact is tangential and routes to the #137 path, never minted;
//! - on-surface postcondition `TAU_EVAL·(1+scale)` on both of the edge's
//!   incident surfaces (loud contract: a line edge lies ON both by
//!   construction, so a violation is a producer fault, not a near-miss).

use crate::*;
use std::collections::BTreeMap;

/// One transversal pierce point of a geometric edge through a bounded
/// partner face.
#[cfg_attr(not(test), allow(dead_code))] // UNWIRED until increment 2 (spec §4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PiercePoint {
    /// The exact junction point (on the edge's line and the partner plane;
    /// on both incident surfaces to within the `TAU_EVAL` band).
    pub point: Point3,
    /// Chord parameter along the edge, strictly inside `(0, 1)`.
    pub t: f64,
    /// `|t̂ · n̂_partner|` at the pierce — the transversality margin.
    pub transversality: f64,
}

/// Sine-scale transversality floor on `|t̂·n̂|` — below this the edge is
/// tangential to the partner surface (the #137 route), never a mint.
#[cfg_attr(not(test), allow(dead_code))] // UNWIRED until increment 2 (spec §4).
const TRANSVERSALITY_MIN: f64 = 1e-9;

/// All transversal pierce points of each operand's geometric `LineSegment`
/// edges through the OTHER operand's bounded planar faces.
///
/// Keys are `(owning input, edge index)` — and because `LineSegment` edges
/// use the per-loop-copy convention (kernel-v2 `to_yang.rs` m1: one directed
/// yang edge per half-edge), the SAME pierce list is fanned out to EVERY
/// copy of the geometric edge, so both incident faces of the owner see the
/// identical insertion (the spec's conformality-by-identity requirement;
/// keying by a single copy index would silently break it).
#[cfg_attr(not(test), allow(dead_code))] // UNWIRED until increment 2 (spec §4).
pub(crate) fn junction_pierce_points(
    a: &BRep,
    b: &BRep,
) -> BTreeMap<(InputId, u32), Vec<PiercePoint>> {
    let mut out: BTreeMap<(InputId, u32), Vec<PiercePoint>> = BTreeMap::new();
    for (x, y, input) in [(a, b, InputId::A), (b, a, InputId::B)] {
        // Group the per-loop LineSegment edge copies by canonical (unordered,
        // bitwise) endpoint pair; collect every copy index and the DISTINCT
        // incident surfaces.
        let kb = |p: Point3| -> [u64; 3] { [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()] };
        type Group = (Vec<u32>, Vec<Surface>);
        let mut groups: BTreeMap<([u64; 3], [u64; 3]), Group> = BTreeMap::new();
        for f in x.faces() {
            for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
                let e = &x.edges()[ei as usize];
                if e.curve != Curve::LineSegment {
                    continue; // increment-1a scope: straight edges only
                }
                let k0 = kb(x.vertices()[e.start as usize].point);
                let k1 = kb(x.vertices()[e.end as usize].point);
                let key = if k0 <= k1 { (k0, k1) } else { (k1, k0) };
                let g = groups.entry(key).or_default();
                if !g.0.contains(&ei) {
                    g.0.push(ei);
                }
                if !g.1.contains(&f.surface) {
                    g.1.push(f.surface);
                }
            }
        }
        for (copies, surfs) in groups.values() {
            let [s1, s2] = surfs.as_slice() else {
                continue; // border/defective incidence — not a 2-surface edge
            };
            let e = &x.edges()[copies[0] as usize];
            let p0 = x.vertices()[e.start as usize].point;
            let p1 = x.vertices()[e.end as usize].point;
            let mut pierces: Vec<PiercePoint> = Vec::new();
            for f in y.faces() {
                if let Some(pp) = line_edge_plane_face_pierce(p0, p1, *s1, *s2, f, y) {
                    pierces.push(pp);
                }
            }
            if pierces.is_empty() {
                continue;
            }
            pierces.sort_by(|u, v| u.t.total_cmp(&v.t));
            for &ei in copies {
                out.insert((input, ei), pierces.clone());
            }
        }
    }
    out
}

/// The transversal pierce of the segment `p0→p1` (whose two incident
/// surfaces are `s1`/`s2`) through the bounded planar face `f` of operand
/// `y` — or `None` if any gate rejects (fail-closed).
#[cfg_attr(not(test), allow(dead_code))] // UNWIRED until increment 2 (spec §4).
fn line_edge_plane_face_pierce(
    p0: Point3,
    p1: Point3,
    s1: Surface,
    s2: Surface,
    f: &BRepFace,
    y: &BRep,
) -> Option<PiercePoint> {
    let Surface::Plane { normal, d } = f.surface else {
        return None; // increment-1a scope: planar partners only
    };
    let n = normalize3(normal.as_array());
    // The plane's `d` is scaled to the RAW normal; renormalize it too.
    let n_len = {
        let r = normal.as_array();
        (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt()
    };
    if n_len < cad_primitives::TAU_WORK {
        return None; // degenerate plane descriptor — producer fault, skip
    }
    let d = d / n_len;
    let (a0, a1) = (p0.as_array(), p1.as_array());
    let dir = [a1[0] - a0[0], a1[1] - a0[1], a1[2] - a0[2]];
    let chord = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if chord == 0.0 {
        return None;
    }
    let t_hat = [dir[0] / chord, dir[1] / chord, dir[2] / chord];
    let transversality = (n[0] * t_hat[0] + n[1] * t_hat[1] + n[2] * t_hat[2]).abs();
    if transversality < TRANSVERSALITY_MIN {
        return None; // tangential contact — the #137 route, never a mint
    }
    let denom = n[0] * dir[0] + n[1] * dir[1] + n[2] * dir[2];
    let t = -(n[0] * a0[0] + n[1] * a0[1] + n[2] * a0[2] + d) / denom;
    if !(0.0..=1.0).contains(&t) {
        return None;
    }
    let j = Point3::new(a0[0] + t * dir[0], a0[1] + t * dir[1], a0[2] + t * dir[2]);
    let ja = j.as_array();
    let scale = ja
        .iter()
        .chain(a0.iter())
        .chain(a1.iter())
        .fold(0.0f64, |m, &c| m.max(c.abs()));
    let margin = cad_primitives::TAU_MODEL * (1.0 + scale);
    // Endpoint margin: a pierce at/near an edge endpoint is a corner (P3b).
    let dist = |p: [f64; 3], q: [f64; 3]| -> f64 {
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
    };
    if dist(ja, a0) <= margin || dist(ja, a1) <= margin {
        return None;
    }
    // On-surface postcondition: the line edge lies ON both incident surfaces
    // by construction, so J (on the line) must evaluate on-band for both — a
    // violation is a producer fault, and fail-closed means no mint.
    let band = cad_primitives::TAU_EVAL * (1.0 + scale);
    for s in [s1, s2] {
        let (fv, _) = surface_value_and_normal(s, ja)?;
        if fv.abs() > band {
            return None;
        }
    }
    // Exact 2D bounded-face containment with a boundary margin: J must lie
    // strictly inside the outer loop, strictly outside every hole, and
    // ≥ margin from every boundary segment (a pierce ON the partner's own
    // boundary is a corner — P3b).
    let nv = Vector3::new(n[0], n[1], n[2]);
    let (u, v) = ortho_basis(nv);
    let (ua, va) = (u.as_array(), v.as_array());
    let project = |p: Point3| -> [f64; 2] {
        let q = p.as_array();
        [
            q[0] * ua[0] + q[1] * ua[1] + q[2] * ua[2],
            q[0] * va[0] + q[1] * va[1] + q[2] * va[2],
        ]
    };
    let j2 = [
        ja[0] * ua[0] + ja[1] * ua[1] + ja[2] * ua[2],
        ja[0] * va[0] + ja[1] * va[1] + ja[2] * va[2],
    ];
    let loop_pts = |lp: &[u32]| -> Vec<[f64; 2]> {
        lp.iter()
            .map(|&ei| {
                let e = &y.edges()[ei as usize];
                project(y.vertices()[e.start as usize].point)
            })
            .collect()
    };
    let outer = loop_pts(&f.outer_loop);
    if outer.len() < 3 || !point_in_polygon(j2, &outer) {
        return None;
    }
    if boundary_distance(j2, &outer) <= margin {
        return None;
    }
    for hole in &f.inner_loops {
        let hp = loop_pts(hole);
        if hp.len() >= 3 && point_in_polygon(j2, &hp) {
            return None;
        }
        if hp.len() >= 2 && boundary_distance(j2, &hp) <= margin {
            return None;
        }
    }
    Some(PiercePoint {
        point: j,
        t,
        transversality,
    })
}

/// Even-odd point-in-polygon test (2D).
#[cfg_attr(not(test), allow(dead_code))] // UNWIRED until increment 2 (spec §4).
fn point_in_polygon(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// Minimum distance from `p` to any segment of the closed polyline `poly`.
#[cfg_attr(not(test), allow(dead_code))] // UNWIRED until increment 2 (spec §4).
fn boundary_distance(p: [f64; 2], poly: &[[f64; 2]]) -> f64 {
    let n = poly.len();
    let mut best = f64::INFINITY;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = dx * dx + dy * dy;
        let t = if len2 == 0.0 {
            0.0
        } else {
            (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
        };
        let (qx, qy) = (a[0] + t * dx, a[1] + t * dy);
        best = best.min(((p[0] - qx).powi(2) + (p[1] - qy).powi(2)).sqrt());
    }
    best
}
