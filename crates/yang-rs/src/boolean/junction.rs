//! P3a #146 conformal junction sampling — the pierce-point primitive
//! (increment 1a) and the Stage-1 override builder the increment-2 wiring
//! feeds into `boolean()` behind `YANG_JUNCTION_SAMPLING_ENABLE`
//! (spec `specs/yang_146_conformal_junction_sampling.md` §3.2/§4).
//!
//! Wiring scope (fail-closed — a missed mint is status quo, never
//! worse, spec §6):
//! - edges: `Curve::LineSegment` only, incident to two PLANAR faces (the
//!   F0082 lead-customer class; the chord IS the curve so the
//!   parameter-range test is exact, and the Stage-1 edge-override splice
//!   is planar-incident only);
//! - partner faces: `Surface::Plane` with ALL-LINE loops only — exact
//!   closed-form pierce (line ∩ plane) and exact 2D bounded-face
//!   containment (a chord polygon is the true region only when every loop
//!   edge is straight). Curved edges/partners are a later increment.
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PiercePoint {
    /// The exact junction point (on the edge's line and the partner plane;
    /// on both incident surfaces to within the `TAU_EVAL` band).
    pub point: Point3,
    /// Chord parameter along the edge, strictly inside `(0, 1)`.
    pub t: f64,
    /// `|t̂ · n̂_partner|` at the pierce — the transversality margin.
    pub transversality: f64,
    /// Index of the pierced face in the PARTNER operand — the face whose
    /// Stage-1 mesh must carry `point` as an interior Steiner vertex
    /// (increment 2, spec §3.3 second bullet).
    pub partner_face: u32,
}

/// Sine-scale transversality floor on `|t̂·n̂|` — below this the edge is
/// tangential to the partner surface (the #137 route), never a mint.
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
            let owner_planar =
                matches!(s1, Surface::Plane { .. }) && matches!(s2, Surface::Plane { .. });
            // P3b scope-sizing probe (read-only): enumerate curved-partner
            // pierce CANDIDATES this increment's planar scope skips — the
            // F0082 ellipse×wall corner class. Mints nothing.
            if std::env::var_os("YANG_P3B_PIERCE_PROBE").is_some() {
                let e = &x.edges()[copies[0] as usize];
                p3b_cylinder_pierce_probe(
                    input,
                    copies[0],
                    *s1,
                    *s2,
                    x.vertices()[e.start as usize].point,
                    x.vertices()[e.end as usize].point,
                    y,
                );
            }
            // Increment-2 wiring scope: the OWNER edge must be incident to two
            // PLANAR faces — the Stage-1 edge-override tessellator splices
            // line chains only into planar faces (the 1b fail-closed guard);
            // a curved-incident edge is a later increment (missed mint =
            // status quo, never worse).
            if !owner_planar {
                continue;
            }
            let e = &x.edges()[copies[0] as usize];
            let p0 = x.vertices()[e.start as usize].point;
            let p1 = x.vertices()[e.end as usize].point;
            let mut pierces: Vec<PiercePoint> = Vec::new();
            for (f_idx, f) in y.faces().iter().enumerate() {
                if let Some(pp) = line_edge_plane_face_pierce(p0, p1, *s1, *s2, f_idx as u32, f, y)
                {
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

/// P3b increment-0 probe (`YANG_P3B_PIERCE_PROBE`, read-only): transversal
/// pierce CANDIDATES of the segment `p0→p1` through the partner operand's
/// CYLINDER lateral faces — the class the planar wiring scope skips (the
/// F0082 ellipse×wall corner: an operand boundary edge piercing the other
/// operand's cylinder is exactly the never-minted section-curve terminus).
/// Line×cylinder is a quadratic in the chord parameter; both roots in (0,1)
/// are candidates. No containment test (scope sizing, not a mint) — the
/// printed height/azimuth let the reader judge against the face loops.
fn p3b_cylinder_pierce_probe(
    input: InputId,
    edge0: u32,
    s1: Surface,
    s2: Surface,
    p0: Point3,
    p1: Point3,
    y: &BRep,
) {
    let owner_planar = matches!(s1, Surface::Plane { .. }) && matches!(s2, Surface::Plane { .. });
    let (a0, a1) = (p0.as_array(), p1.as_array());
    let dir = [a1[0] - a0[0], a1[1] - a0[1], a1[2] - a0[2]];
    let chord = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if chord == 0.0 {
        return;
    }
    for (f_idx, f) in y.faces().iter().enumerate() {
        let Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } = f.surface
        else {
            continue;
        };
        let ap = axis_point.as_array();
        let ah = normalize3(axis_dir.as_array());
        // Radial component of p(t)−ap: w(t) = w0 + t·wd with the axis
        // projection removed; |w(t)|² − r² = At² + Bt + C.
        let proj = |v: [f64; 3]| -> [f64; 3] {
            let along = v[0] * ah[0] + v[1] * ah[1] + v[2] * ah[2];
            [
                v[0] - along * ah[0],
                v[1] - along * ah[1],
                v[2] - along * ah[2],
            ]
        };
        let w0 = proj([a0[0] - ap[0], a0[1] - ap[1], a0[2] - ap[2]]);
        let wd = proj(dir);
        let qa = wd[0] * wd[0] + wd[1] * wd[1] + wd[2] * wd[2];
        let qb = 2.0 * (w0[0] * wd[0] + w0[1] * wd[1] + w0[2] * wd[2]);
        let qc = w0[0] * w0[0] + w0[1] * w0[1] + w0[2] * w0[2] - radius * radius;
        let disc = qb * qb - 4.0 * qa * qc;
        if qa == 0.0 || disc < 0.0 {
            continue; // line parallel to axis or missing the cylinder
        }
        let sq = disc.sqrt();
        for t in [(-qb - sq) / (2.0 * qa), (-qb + sq) / (2.0 * qa)] {
            if !(0.0..=1.0).contains(&t) {
                continue;
            }
            let j = [a0[0] + t * dir[0], a0[1] + t * dir[1], a0[2] + t * dir[2]];
            // Radial (outward) unit normal at J and the line/surface angle.
            let wj = proj([j[0] - ap[0], j[1] - ap[1], j[2] - ap[2]]);
            let n = normalize3(wj);
            let t_hat = [dir[0] / chord, dir[1] / chord, dir[2] / chord];
            let transversality = (n[0] * t_hat[0] + n[1] * t_hat[1] + n[2] * t_hat[2]).abs();
            let height = (j[0] - ap[0]) * ah[0] + (j[1] - ap[1]) * ah[1] + (j[2] - ap[2]) * ah[2];
            eprintln!(
                "[p3b-pierce] {input:?} edge {edge0} (owner_planar={owner_planar}) × cyl \
                 face {f_idx} (r={radius:.6}): t={t:.6} J=({:.9},{:.9},{:.9}) \
                 transv={transversality:.3} h={height:.6}",
                j[0], j[1], j[2]
            );
        }
        // Increment-1 arm: the production-shaped primitive's verdict on the
        // same face — which candidates survive EVERY mint gate (containment,
        // margins, postconditions). The pre-inc-3 measurement of what the
        // wiring would actually mint.
        for pp in line_edge_cylinder_face_pierce(p0, p1, s1, s2, f_idx as u32, f, y) {
            eprintln!(
                "[p3b-pierce] MINT {input:?} edge {edge0} × cyl face {f_idx}: t={:.6} \
                 J=({:.9},{:.9},{:.9}) transv={:.3}",
                pp.t,
                pp.point.x(),
                pp.point.y(),
                pp.point.z(),
                pp.transversality
            );
        }
    }
}

/// P3b increment 1 (spec `yang_169_p3b_curved_partner_pierce.md` §3.1–3.2):
/// the transversal pierces of the segment `p0→p1` (whose two incident owner
/// surfaces are `s1`/`s2`) through the bounded CANONICAL-TUBE cylinder face
/// `f` of operand `y` — up to TWO per edge×face (both quadratic roots are
/// genuine crossings, unlike the planar arm's single root). UNWIRED this
/// increment: only the probe and the unit fixtures call it; wiring into
/// `junction_pierce_points` is increment 3, behind `YANG_P3B_PIERCE_ENABLE`.
///
/// Gates mirror `line_edge_plane_face_pierce` one-for-one — every margin is
/// the existing derived vocabulary, fail-closed (a missed mint = status quo):
/// - canonical-tube scope: hole-free face whose outer loop carries exactly
///   two FULL-circle rims (the `tessellate_lateral_face` tube vocabulary) —
///   axial containment is then EXACT via the rim planes. Strips / holed
///   patches are a later widening (spec §5 inc-4);
/// - roots in `[0,1]` with the endpoint margin `TAU_MODEL·(1+scale)` at both
///   owner endpoints (a near-endpoint pierce is a higher-order corner);
/// - transversality `|t̂·n̂(J)|` ≥ `TRANSVERSALITY_MIN` with the radial
///   normal at J (tangential grazes route to #137, never minted);
/// - on-surface postcondition `TAU_EVAL·(1+scale)` for `s1`/`s2` at J
///   (producer-fault guard, identical to the planar arm);
/// - axial containment `v_J ∈ (v_lo+margin, v_hi−margin)` from the rim-circle
///   centers projected on the axis (a rim-margin pierce is a rim corner —
///   P3b-later, fail closed).
pub(crate) fn line_edge_cylinder_face_pierce(
    p0: Point3,
    p1: Point3,
    s1: Surface,
    s2: Surface,
    f_idx: u32,
    f: &BRepFace,
    y: &BRep,
) -> Vec<PiercePoint> {
    let Surface::Cylinder {
        axis_point,
        axis_dir,
        radius,
    } = f.surface
    else {
        return Vec::new();
    };
    // Canonical-tube vocabulary gate (fail closed).
    if !f.inner_loops.is_empty() {
        return Vec::new();
    }
    let rims: Vec<&BRepEdge> = f
        .outer_loop
        .iter()
        .map(|&ei| &y.edges()[ei as usize])
        .filter(|e| matches!(e.curve, Curve::Circle { .. }) && e.start == e.end)
        .collect();
    let [rim0, rim1] = rims.as_slice() else {
        return Vec::new();
    };
    let ap = axis_point.as_array();
    let ah = normalize3(axis_dir.as_array());
    let axial = |p: Point3| -> f64 {
        let q = p.as_array();
        (q[0] - ap[0]) * ah[0] + (q[1] - ap[1]) * ah[1] + (q[2] - ap[2]) * ah[2]
    };
    let (mut v_lo, mut v_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for rim in [rim0, rim1] {
        let Curve::Circle { center, .. } = rim.curve else {
            unreachable!("filtered to circles above");
        };
        let v = axial(center);
        v_lo = v_lo.min(v);
        v_hi = v_hi.max(v);
    }
    let (a0, a1) = (p0.as_array(), p1.as_array());
    let dir = [a1[0] - a0[0], a1[1] - a0[1], a1[2] - a0[2]];
    let chord = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if chord == 0.0 {
        return Vec::new();
    }
    let proj = |v: [f64; 3]| -> [f64; 3] {
        let along = v[0] * ah[0] + v[1] * ah[1] + v[2] * ah[2];
        [
            v[0] - along * ah[0],
            v[1] - along * ah[1],
            v[2] - along * ah[2],
        ]
    };
    let w0 = proj([a0[0] - ap[0], a0[1] - ap[1], a0[2] - ap[2]]);
    let wd = proj(dir);
    let qa = wd[0] * wd[0] + wd[1] * wd[1] + wd[2] * wd[2];
    let qb = 2.0 * (w0[0] * wd[0] + w0[1] * wd[1] + w0[2] * wd[2]);
    let qc = w0[0] * w0[0] + w0[1] * w0[1] + w0[2] * w0[2] - radius * radius;
    let disc = qb * qb - 4.0 * qa * qc;
    if qa == 0.0 || disc < 0.0 {
        return Vec::new(); // parallel to the axis or missing the cylinder
    }
    let sq = disc.sqrt();
    let mut out = Vec::new();
    for t in [(-qb - sq) / (2.0 * qa), (-qb + sq) / (2.0 * qa)] {
        if !(0.0..=1.0).contains(&t) {
            continue;
        }
        let j = [a0[0] + t * dir[0], a0[1] + t * dir[1], a0[2] + t * dir[2]];
        let scale = j
            .iter()
            .chain(a0.iter())
            .chain(a1.iter())
            .fold(0.0f64, |m, &c| m.max(c.abs()));
        let margin = cad_primitives::TAU_MODEL * (1.0 + scale);
        // Endpoint margin: a pierce at/near an owner endpoint is a corner of
        // higher order (vertex-on-surface) — fail closed.
        let dist = |p: [f64; 3], q: [f64; 3]| -> f64 {
            ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
        };
        if dist(j, a0) <= margin || dist(j, a1) <= margin {
            continue;
        }
        // Transversality via the radial normal at J.
        let wj = proj([j[0] - ap[0], j[1] - ap[1], j[2] - ap[2]]);
        let n = normalize3(wj);
        let t_hat = [dir[0] / chord, dir[1] / chord, dir[2] / chord];
        let transversality = (n[0] * t_hat[0] + n[1] * t_hat[1] + n[2] * t_hat[2]).abs();
        if transversality < TRANSVERSALITY_MIN {
            continue; // tangential graze — the #137 route, never a mint
        }
        // On-surface postcondition for the owner's two incident surfaces.
        let band = cad_primitives::TAU_EVAL * (1.0 + scale);
        let on_owner = [s1, s2]
            .into_iter()
            .all(|s| surface_value_and_normal(s, j).is_some_and(|(fv, _)| fv.abs() <= band));
        if !on_owner {
            continue;
        }
        // Exact axial containment with the rim margin.
        let v_j = (j[0] - ap[0]) * ah[0] + (j[1] - ap[1]) * ah[1] + (j[2] - ap[2]) * ah[2];
        if v_j <= v_lo + margin || v_j >= v_hi - margin {
            continue; // outside the tube span, or a rim corner — fail closed
        }
        out.push(PiercePoint {
            point: Point3::new(j[0], j[1], j[2]),
            t,
            transversality,
            partner_face: f_idx,
        });
    }
    out.sort_by(|u, v| u.t.total_cmp(&v.t));
    out
}

/// The transversal pierce of the segment `p0→p1` (whose two incident
/// surfaces are `s1`/`s2`) through the bounded planar face `f` of operand
/// `y` — or `None` if any gate rejects (fail-closed).
fn line_edge_plane_face_pierce(
    p0: Point3,
    p1: Point3,
    s1: Surface,
    s2: Surface,
    f_idx: u32,
    f: &BRepFace,
    y: &BRep,
) -> Option<PiercePoint> {
    let Surface::Plane { normal, d } = f.surface else {
        return None; // increment-1a scope: planar partners only
    };
    // Increment-2 scope: ALL-LINE partner loops only — the 2D containment
    // below projects loop-edge START vertices, which is the exact bounded
    // region only when every loop edge is straight (an arc-bounded face's
    // chord polygon misjudges containment by up to the sagitta near the
    // arc). Curved-bounded partners are a later increment (fail closed).
    if f.outer_loop
        .iter()
        .chain(f.inner_loops.iter().flatten())
        .any(|&ei| y.edges()[ei as usize].curve != Curve::LineSegment)
    {
        return None;
    }
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
        partner_face: f_idx,
    })
}

/// Increment-2 wiring payload (spec §4): the four per-operand Stage-1
/// override maps derived from [`junction_pierce_points`] — each operand's
/// OWNER-side edge insertions plus the PARTNER-side interior face points the
/// other operand's edges pierce through it. Identical exact bits on both
/// sides of every junction (one mint, shared by identity).
#[derive(Default, Debug)]
pub(crate) struct JunctionStage1Overrides {
    /// A's `LineSegment` edge index → junction points on that edge.
    pub edge_a: BTreeMap<u32, Vec<Point3>>,
    /// A's face index → interior junction points (pierced by B's edges).
    pub face_a: BTreeMap<u32, Vec<Point3>>,
    /// B's `LineSegment` edge index → junction points on that edge.
    pub edge_b: BTreeMap<u32, Vec<Point3>>,
    /// B's face index → interior junction points (pierced by A's edges).
    pub face_b: BTreeMap<u32, Vec<Point3>>,
}

impl JunctionStage1Overrides {
    pub(crate) fn is_empty(&self) -> bool {
        self.edge_a.is_empty()
            && self.face_a.is_empty()
            && self.edge_b.is_empty()
            && self.face_b.is_empty()
    }
}

/// Build the Stage-1 override maps for both operands from the pierce
/// enumeration. Owner-side lists keep the per-(copy-)edge fan-out from
/// [`junction_pierce_points`]; partner-side face lists deduplicate the
/// per-copy repetition bitwise (each geometric edge contributes its pierce
/// point to the pierced face ONCE — same exact bits as the owner side).
///
/// Sub-weld-band CLUSTER filter (the F0016 lesson): two DISTINCT pierce
/// points closer than the §4.3 weld band `TAU_MODEL·(1+scale)` — e.g. from
/// near-duplicate chained edges piercing the same partner plane — would be
/// minted as separate sliver-spanning vertices that the downstream weld
/// fuses back into COINCIDENT triangles (the I6 non-manifold guard). Per
/// the junction contract, multiplicity below the resolution floor is NOT a
/// P3a mint: every point in such a cluster is DROPPED on both sides (fail
/// closed — a missed mint is status quo, never worse; the cluster itself is
/// P3b/upstream-twin territory). NO merged representative is minted: that
/// would be a tolerance merge (the R0091 hazard, spec §3.4).
pub(crate) fn junction_stage1_overrides(a: &BRep, b: &BRep) -> JunctionStage1Overrides {
    let pierce = junction_pierce_points(a, b);
    // Sub-weld-band cluster scan over ALL pierce points of the pair (both
    // directions — a mutual corner can put an A-side and a B-side mint in
    // one cluster). Bit-identical repeats are the SAME mint (kept); only
    // distinct-bits neighbours poison a cluster.
    let kb = |p: Point3| -> [u64; 3] { [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()] };
    let all: Vec<Point3> = {
        let mut v: Vec<Point3> = Vec::new();
        let mut seen: Vec<[u64; 3]> = Vec::new();
        for pps in pierce.values() {
            for pp in pps {
                let key = kb(pp.point);
                if !seen.contains(&key) {
                    seen.push(key);
                    v.push(pp.point);
                }
            }
        }
        v
    };
    let mut poisoned: Vec<[u64; 3]> = Vec::new();
    for (i, p) in all.iter().enumerate() {
        let pa = p.as_array();
        for q in all.iter().skip(i + 1) {
            let qa = q.as_array();
            let scale = pa
                .iter()
                .chain(qa.iter())
                .fold(0.0f64, |m, &c| m.max(c.abs()));
            let band = cad_primitives::TAU_MODEL * (1.0 + scale);
            let d2 = (pa[0] - qa[0]).powi(2) + (pa[1] - qa[1]).powi(2) + (pa[2] - qa[2]).powi(2);
            if d2 < band * band {
                poisoned.push(kb(*p));
                poisoned.push(kb(*q));
            }
        }
    }
    if !poisoned.is_empty() && std::env::var_os("YANG_JUNCTION_MINT_PROBE").is_some() {
        eprintln!(
            "[p3a-wire] sub-weld-band cluster: dropping {} of {} pierce points",
            poisoned.len(),
            all.len()
        );
    }
    let mut out = JunctionStage1Overrides::default();
    for ((input, ei), pps) in &pierce {
        let kept: Vec<&PiercePoint> = pps
            .iter()
            .filter(|pp| !poisoned.contains(&kb(pp.point)))
            .collect();
        if kept.is_empty() {
            continue;
        }
        let (edge_map, face_map) = match input {
            InputId::A => (&mut out.edge_a, &mut out.face_b),
            InputId::B => (&mut out.edge_b, &mut out.face_a),
        };
        edge_map.insert(*ei, kept.iter().map(|pp| pp.point).collect());
        for pp in &kept {
            let list = face_map.entry(pp.partner_face).or_default();
            let key = kb(pp.point);
            let dup = list.iter().any(|q| kb(*q) == key);
            if !dup {
                list.push(pp.point);
            }
        }
    }
    out
}

/// Even-odd point-in-polygon test (2D).
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
