//! Stage 0 — §4.5.5 one dimension down: an EDGE (or a vertex) of one operand
//! lying in a planar face of the other (spec
//! `specs/yang_455_edge_in_plane_conformity.md`).
//!
//! Yang §4.5.5 (`refs/text/yang2025_hybrid_boolean.txt:717-731`): "our
//! discretization method does not maintain coplanarity in triangle meshes
//! because of floating-point error … it is necessary to check coplanar planes
//! … before mesh discretizations … identical meshes are generated for both
//! models in this part … the common part and the other two parts share
//! identical sampling points on their boundaries." The face-pair form of that
//! rule is `stage0_preprocess`; this module is the same rule for the
//! degeneracy one dimension down, anchored on P0001: a needle star's tip edge
//! authored 4e-15 below the octagon prism's cap plane (the generator's
//! `cos 270°`), running INSIDE the cap region. Nothing identified the edge into
//! the plane, so the exact arrangement faithfully kept the 4e-15-wide wedge of
//! the upper flank between the tilted edge and the plane as "outside A" — a
//! kept triangle with all three corners on one line, a 180° fold in the
//! emitted loop, and kernel-v2's G1 render gate refusing the zero-area ear.
//!
//! Two arms, one contract:
//!
//! 1. **Identification** ([`identify_vertices`]) — before the §4.3.3 generator
//!    tangency and Stage 0: every vertex of X within the #178 coincidence line
//!    (`gap ≤ band/100`, `band = max(TAU_MODEL, scale·TAU_WORK)`) of a planar
//!    all-line face of Y that it INTERACTS with is moved onto that plane (least-
//!    norm displacement over every matched plane — a vertex on a Y edge or
//!    corner satisfies all of them). Vertices of a face that forms a Stage-0
//!    cross pair are Stage 0's and are never touched.
//! 2. **Conformity** ([`conformity_overrides`]) — in the P3a scope gate, on
//!    the identified operands: the sub-segments of every X edge inside a Y face
//!    F become identically-sampled shared elements of BOTH Stage-1 meshes —
//!    each crossing with F's boundary is minted once and inserted into every
//!    per-loop copy of both crossed edges, an X endpoint inside F becomes an
//!    interior Steiner point of F, and each inside sub-segment becomes an
//!    interior CONSTRAINT of F's CDT (`FaceConstraints`), so the arrangement
//!    sees one edge shared by identity rather than two femto-separated ones.
//!    On X's side the sub-segment is already a boundary chain of both faces
//!    incident to the edge.
//!
//! Exactness posture: the rotation experiments in the spec (an exactly
//! coplanar tip converts in 8 of 9 rigid motions, the femto-off original
//! flips CORRECT in 4 of 6) show the class is rounding luck in both
//! directions on oblique planes; the shared-edge conformity does not depend on
//! exact coplanarity at all. Fail-closed scope (status quo, probe-counted):
//! crossings within the corner margin of a loop vertex or of the X edge's
//! endpoints, X endpoints within the margin of F's boundary, collinear
//! contact with a loop edge, curved-bounded or non-planar F.
//!
//! `YANG_EDGE_IN_PLANE=off|0` disables both arms (dev A/B);
//! `YANG_EDGE_IN_PLANE_PROBE=1` prints every identification, contact, decline
//! and the census of gaps in the would-be STOP window `(band/100, band]`.

use std::collections::{BTreeMap, BTreeSet};

use cad_primitives::{Point3, TAU_MODEL, TAU_WORK};

use crate::scan_near_coplanar;
use crate::stage1_tessellate::FaceConstraints;
use crate::{ortho_basis, BRep, BRepVertex, Curve, InputId, Surface, YangError};

/// Both arms on unless `YANG_EDGE_IN_PLANE=off|0`.
pub(crate) fn enabled() -> bool {
    !matches!(
        std::env::var("YANG_EDGE_IN_PLANE").as_deref(),
        Ok("off") | Ok("0")
    )
}

fn probe() -> bool {
    std::env::var_os("YANG_EDGE_IN_PLANE_PROBE").is_some()
}

/// A planar, all-`LineSegment` face of the partner operand in its own Stage-1
/// projection frame (`ortho_basis(normal)`, coordinates `p·e1, p·e2` — the
/// frame `tessellate_planar_curved_cdt_face` triangulates in).
struct PartnerFace {
    idx: u32,
    /// Unit normal and unit-normal offset (`n·x + d = 0`).
    n: [f64; 3],
    d: f64,
    /// Loop-vertex AABB (the band's scale source).
    lo: [f64; 3],
    hi: [f64; 3],
    e1: [f64; 3],
    e2: [f64; 3],
    /// Outer loop: 2D vertex per loop edge (its `start`), and the edge index.
    outer: Vec<[f64; 2]>,
    outer_edges: Vec<u32>,
    holes: Vec<Vec<[f64; 2]>>,
    holes_edges: Vec<Vec<u32>>,
}

fn partner_faces(y: &BRep) -> Vec<PartnerFace> {
    let mut out = Vec::new();
    for (fi, f) in y.faces().iter().enumerate() {
        let Surface::Plane { normal, d } = f.surface else {
            continue;
        };
        if f.reversed {
            continue;
        }
        let all_line = f
            .outer_loop
            .iter()
            .chain(f.inner_loops.iter().flatten())
            .all(|&ei| matches!(y.edges()[ei as usize].curve, Curve::LineSegment));
        if !all_line || f.outer_loop.len() < 3 {
            continue;
        }
        let na = normal.as_array();
        let len = (na[0] * na[0] + na[1] * na[1] + na[2] * na[2]).sqrt();
        if len < cad_primitives::MIN_FEATURE_SIZE {
            continue;
        }
        let n = [na[0] / len, na[1] / len, na[2] / len];
        let (e1v, e2v) = ortho_basis(normal);
        let (e1, e2) = (e1v.as_array(), e2v.as_array());
        let project = |p: [f64; 3]| -> [f64; 2] {
            [
                p[0] * e1[0] + p[1] * e1[1] + p[2] * e1[2],
                p[0] * e2[0] + p[1] * e2[1] + p[2] * e2[2],
            ]
        };
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        let mut loop_2d = |lp: &[u32]| -> (Vec<[f64; 2]>, Vec<u32>) {
            let mut pts = Vec::with_capacity(lp.len());
            for &ei in lp {
                let e = &y.edges()[ei as usize];
                let p = y.vertices()[e.start as usize].point.as_array();
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
                pts.push(project(p));
            }
            (pts, lp.to_vec())
        };
        let (outer, outer_edges) = loop_2d(&f.outer_loop);
        let mut holes = Vec::new();
        let mut holes_edges = Vec::new();
        for h in &f.inner_loops {
            let (pts, es) = loop_2d(h);
            holes.push(pts);
            holes_edges.push(es);
        }
        out.push(PartnerFace {
            idx: fi as u32,
            n,
            d: d / len,
            lo,
            hi,
            e1,
            e2,
            outer,
            outer_edges,
            holes,
            holes_edges,
        });
    }
    out
}

impl PartnerFace {
    fn project(&self, p: [f64; 3]) -> [f64; 2] {
        [
            p[0] * self.e1[0] + p[1] * self.e1[1] + p[2] * self.e1[2],
            p[0] * self.e2[0] + p[1] * self.e2[1] + p[2] * self.e2[2],
        ]
    }

    /// Signed distance of `p` to the plane.
    fn gap(&self, p: [f64; 3]) -> f64 {
        self.n[0] * p[0] + self.n[1] * p[1] + self.n[2] * p[2] + self.d
    }

    /// The #178 detection band for this face and the point(s) at hand:
    /// `max(TAU_MODEL, scale·TAU_WORK)`, `scale` = max |coordinate| over the
    /// face's AABB corners and the points.
    fn band(&self, pts: &[[f64; 3]]) -> f64 {
        let mut scale: f64 = 0.0;
        for p in [&self.lo, &self.hi] {
            for &c in p.iter() {
                scale = scale.max(c.abs());
            }
        }
        for p in pts {
            for &c in p.iter() {
                scale = scale.max(c.abs());
            }
        }
        TAU_MODEL.max(scale * TAU_WORK)
    }

    /// Inside the outer loop and outside every hole (f64 crossing parity —
    /// a classification of a point the caller has already placed ≥ margin
    /// from every boundary segment, or of a segment midpoint).
    fn contains(&self, q: [f64; 2]) -> bool {
        if !point_in_polygon(q, &self.outer) {
            return false;
        }
        !self
            .holes
            .iter()
            .any(|h| h.len() >= 3 && point_in_polygon(q, h))
    }

    fn boundary_distance(&self, q: [f64; 2]) -> f64 {
        let mut best = boundary_distance(q, &self.outer);
        for h in &self.holes {
            if h.len() >= 2 {
                best = best.min(boundary_distance(q, h));
            }
        }
        best
    }

    /// Does the point interact with the face region: inside it, or within
    /// `band` of its boundary?
    fn touches(&self, q: [f64; 2], band: f64) -> bool {
        self.contains(q) || self.boundary_distance(q) <= band
    }

    /// Does the projected segment `q0→q1` cross any loop edge (proper or
    /// touching crossing, f64 orientation)?
    fn segment_crosses_boundary(&self, q0: [f64; 2], q1: [f64; 2]) -> bool {
        let crosses = |lp: &[[f64; 2]]| -> bool {
            let n = lp.len();
            (0..n).any(|k| {
                let (s0, s1) = (lp[k], lp[(k + 1) % n]);
                let a = orient2d(q0, q1, s0);
                let b = orient2d(q0, q1, s1);
                let c = orient2d(s0, s1, q0);
                let d = orient2d(s0, s1, q1);
                a * b <= 0.0 && c * d <= 0.0
            })
        };
        crosses(&self.outer) || self.holes.iter().any(|h| crosses(h))
    }
}

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

fn orient2d(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn dist3(p: [f64; 3], q: [f64; 3]) -> f64 {
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
}

fn key_of(p: Point3) -> [u64; 3] {
    [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()]
}

/// Faces of A and of B that form a Stage-0 cross pair with any partner face
/// — Stage 0 owns their vertices (§4.5.5 face-pair identification).
fn paired_faces(a: &BRep, b: &BRep) -> (BTreeSet<usize>, BTreeSet<usize>) {
    let scan = scan_near_coplanar(a, b);
    let mut pa = BTreeSet::new();
    let mut pb = BTreeSet::new();
    for p in &scan.cross {
        pa.insert(p.face_a);
        pb.insert(p.face_b);
    }
    (pa, pb)
}

/// Vertices touched by any face in `faces`.
fn vertices_of_faces(x: &BRep, faces: &BTreeSet<usize>) -> Vec<bool> {
    let mut out = vec![false; x.vertices().len()];
    for &fi in faces {
        let f = &x.faces()[fi];
        for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
            let e = &x.edges()[ei as usize];
            out[e.start as usize] = true;
            out[e.end as usize] = true;
        }
    }
    out
}

/// Per vertex, the `LineSegment` neighbours (both directions, deduped).
fn line_neighbours(x: &BRep) -> Vec<Vec<u32>> {
    let mut out: Vec<Vec<u32>> = vec![Vec::new(); x.vertices().len()];
    for e in x.edges() {
        if !matches!(e.curve, Curve::LineSegment) || e.start == e.end {
            continue;
        }
        if !out[e.start as usize].contains(&e.end) {
            out[e.start as usize].push(e.end);
        }
        if !out[e.end as usize].contains(&e.start) {
            out[e.end as usize].push(e.start);
        }
    }
    out
}

/// Least-norm displacement `δ` with `n_i·(v+δ) + d_i = 0` for every plane
/// (Gram solve on up to three independent normals; near-parallel duplicates
/// are dropped — two Y planes 1e-9-parallel and both within band of one point
/// are one plane authored twice, and either lands the point).
fn least_norm_onto_planes(v: [f64; 3], planes: &[([f64; 3], f64)]) -> [f64; 3] {
    let mut kept: Vec<([f64; 3], f64)> = Vec::new();
    for &(n, d) in planes {
        let parallel = kept.iter().any(|&(m, _)| {
            let c = [
                n[1] * m[2] - n[2] * m[1],
                n[2] * m[0] - n[0] * m[2],
                n[0] * m[1] - n[1] * m[0],
            ];
            (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() < 1e-9
        });
        if !parallel && kept.len() < 3 {
            kept.push((n, d));
        }
    }
    let r: Vec<f64> = kept
        .iter()
        .map(|&(n, d)| n[0] * v[0] + n[1] * v[1] + n[2] * v[2] + d)
        .collect();
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let lambda: Vec<f64> = match kept.len() {
        0 => return [0.0; 3],
        1 => vec![r[0] / dot(kept[0].0, kept[0].0)],
        2 => {
            let (g00, g01, g11) = (
                dot(kept[0].0, kept[0].0),
                dot(kept[0].0, kept[1].0),
                dot(kept[1].0, kept[1].0),
            );
            let det = g00 * g11 - g01 * g01;
            vec![
                (r[0] * g11 - r[1] * g01) / det,
                (g00 * r[1] - g01 * r[0]) / det,
            ]
        }
        _ => {
            let g: [[f64; 3]; 3] =
                std::array::from_fn(|i| std::array::from_fn(|j| dot(kept[i].0, kept[j].0)));
            let det3 = |m: [[f64; 3]; 3]| {
                m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
                    - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                    + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
            };
            let det = det3(g);
            (0..3)
                .map(|k| {
                    let mut m = g;
                    for row in 0..3 {
                        m[row][k] = r[row];
                    }
                    det3(m) / det
                })
                .collect()
        }
    };
    let mut delta = [0.0; 3];
    for (k, &(n, _)) in kept.iter().enumerate() {
        for c in 0..3 {
            delta[c] -= lambda[k] * n[c];
        }
    }
    delta
}

/// Arm 1 — identification. `Ok(None)` when no vertex of either operand needs
/// to move (the byte-identical identity: no rebuild). Otherwise both operands
/// are returned, each rebuilt from topology only if one of its vertices moved.
pub(crate) fn identify_vertices(a: &BRep, b: &BRep) -> Result<Option<(BRep, BRep)>, YangError> {
    let (paired_a, paired_b) = paired_faces(a, b);
    let mut moved: [Option<Vec<BRepVertex>>; 2] = [None, None];
    for (slot, x, y, paired_x, tag) in [
        (0usize, a, b, &paired_a, InputId::A),
        (1usize, b, a, &paired_b, InputId::B),
    ] {
        let faces = partner_faces(y);
        if faces.is_empty() {
            continue;
        }
        let excluded = vertices_of_faces(x, paired_x);
        let neighbours = line_neighbours(x);
        let mut planes_of: Vec<Vec<([f64; 3], f64, u32)>> = vec![Vec::new(); x.vertices().len()];
        let mut window = 0usize;
        for f in &faces {
            for (vi, v) in x.vertices().iter().enumerate() {
                if excluded[vi] {
                    continue;
                }
                let p = v.point.as_array();
                let gap = f.gap(p).abs();
                if gap == 0.0 {
                    continue;
                }
                let band = f.band(&[p]);
                if gap > band {
                    continue;
                }
                if gap > band / 100.0 {
                    window += 1;
                    if probe() {
                        eprintln!(
                            "[edge-in-plane] {tag:?} v{vi} vs partner face {}: gap {gap:.3e} in \
                             the STOP window (band {band:.3e}) — not identified",
                            f.idx
                        );
                    }
                    continue;
                }
                let q = f.project(p);
                let interacts = f.touches(q, band)
                    || neighbours[vi].iter().any(|&w| {
                        let pw = x.vertices()[w as usize].point.as_array();
                        f.gap(pw).abs() <= band / 100.0
                            && f.segment_crosses_boundary(q, f.project(pw))
                    });
                if !interacts {
                    continue;
                }
                planes_of[vi].push((f.n, f.d, f.idx));
            }
        }
        if probe() && window > 0 {
            eprintln!("[edge-in-plane] {tag:?}: {window} vertex×face gaps in the STOP window");
        }
        let mut verts = x.vertices().to_vec();
        let mut any = false;
        for (vi, planes) in planes_of.iter().enumerate() {
            if planes.is_empty() {
                continue;
            }
            let p = verts[vi].point.as_array();
            let nd: Vec<([f64; 3], f64)> = planes.iter().map(|&(n, d, _)| (n, d)).collect();
            let delta = least_norm_onto_planes(p, &nd);
            let moved_p = [p[0] + delta[0], p[1] + delta[1], p[2] + delta[2]];
            if moved_p == p {
                continue;
            }
            if probe() {
                eprintln!(
                    "[edge-in-plane] {tag:?} v{vi} identified onto partner plane(s) {:?}: \
                     |δ| = {:.3e}",
                    planes.iter().map(|t| t.2).collect::<Vec<_>>(),
                    (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt()
                );
            }
            verts[vi] = BRepVertex {
                point: Point3::new(moved_p[0], moved_p[1], moved_p[2]),
            };
            any = true;
        }
        if any {
            moved[slot] = Some(verts);
        }
    }
    if moved.iter().all(Option::is_none) {
        return Ok(None);
    }
    let [ma, mb] = moved;
    let na = match ma {
        Some(v) => a.rebuilt_with_vertices(v)?,
        None => a.clone(),
    };
    let nb = match mb {
        Some(v) => b.rebuilt_with_vertices(v)?,
        None => b.clone(),
    };
    Ok(Some((na, nb)))
}

/// Arm 2 payload: per-operand Stage-1 override maps (the P3a shapes) plus the
/// new face-constraint channel, and every crossing minted (for the Stage-4
/// junction registry).
#[derive(Default, Debug)]
pub(crate) struct EdgeInPlaneOverrides {
    pub edge_a: BTreeMap<u32, Vec<Point3>>,
    pub face_a: BTreeMap<u32, Vec<Point3>>,
    pub cons_a: FaceConstraints,
    pub edge_b: BTreeMap<u32, Vec<Point3>>,
    pub face_b: BTreeMap<u32, Vec<Point3>>,
    pub cons_b: FaceConstraints,
    pub mints: Vec<Point3>,
    /// Contacts declined by the fail-closed scope (probe census).
    pub declined: usize,
}

impl EdgeInPlaneOverrides {
    pub(crate) fn is_empty(&self) -> bool {
        self.edge_a.is_empty()
            && self.face_a.is_empty()
            && self.cons_a.is_empty()
            && self.edge_b.is_empty()
            && self.face_b.is_empty()
            && self.cons_b.is_empty()
    }
}

/// Geometric `LineSegment` edges of an operand: canonical endpoint bit-pair →
/// (every per-loop copy index, every incident face index).
/// Canonical endpoint bit-pair → (every per-loop copy index, every incident
/// face index) of one geometric `LineSegment` edge.
type LineEdgeGroups = BTreeMap<([u64; 3], [u64; 3]), (Vec<u32>, Vec<usize>)>;

fn geometric_line_edges(x: &BRep) -> LineEdgeGroups {
    let mut groups: LineEdgeGroups = BTreeMap::new();
    for (fi, f) in x.faces().iter().enumerate() {
        for &ei in f.outer_loop.iter().chain(f.inner_loops.iter().flatten()) {
            let e = &x.edges()[ei as usize];
            if !matches!(e.curve, Curve::LineSegment) || e.start == e.end {
                continue;
            }
            let k0 = key_of(x.vertices()[e.start as usize].point);
            let k1 = key_of(x.vertices()[e.end as usize].point);
            let key = if k0 <= k1 { (k0, k1) } else { (k1, k0) };
            let g = groups.entry(key).or_default();
            if !g.0.contains(&ei) {
                g.0.push(ei);
            }
            if !g.1.contains(&fi) {
                g.1.push(fi);
            }
        }
    }
    groups
}

fn push_point(map: &mut BTreeMap<u32, Vec<Point3>>, key: u32, p: Point3) {
    let slot = map.entry(key).or_default();
    if !slot.contains(&p) {
        slot.push(p);
    }
}

/// One operand's contacts: X's edges against Y's planar faces.
struct SideContacts {
    edge_x: BTreeMap<u32, Vec<Point3>>,
    edge_y: BTreeMap<u32, Vec<Point3>>,
    face_y: BTreeMap<u32, Vec<Point3>>,
    cons_y: FaceConstraints,
    mints: Vec<Point3>,
    declined: usize,
}

fn side_contacts(x: &BRep, y: &BRep, paired_x: &BTreeSet<usize>, tag: InputId) -> SideContacts {
    let mut sc = SideContacts {
        edge_x: BTreeMap::new(),
        edge_y: BTreeMap::new(),
        face_y: BTreeMap::new(),
        cons_y: FaceConstraints::new(),
        mints: Vec::new(),
        declined: 0,
    };
    let faces = partner_faces(y);
    if faces.is_empty() {
        return sc;
    }
    let x_groups = geometric_line_edges(x);
    let y_groups = geometric_line_edges(y);
    // Per-copy → every copy of the same geometric Y edge.
    let mut y_copies_of: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (copies, _) in y_groups.values() {
        for &c in copies {
            y_copies_of.insert(c, copies.clone());
        }
    }
    for (copies, incident) in x_groups.values() {
        if incident.iter().any(|fi| paired_x.contains(fi)) {
            continue;
        }
        // Planar-incident edges only: a straight edge of a CURVED face (a
        // cylinder's seam line in a partner plane, F0055) cannot take an edge
        // override — the lateral's strip tessellation does not read edge
        // polylines (the P3a increment-1b scope) — so the contact is declined,
        // counted, and left to the arrangement (status quo).
        if incident
            .iter()
            .any(|&fi| !matches!(x.faces()[fi].surface, Surface::Plane { .. }))
        {
            sc.declined += 1;
            if probe() {
                eprintln!(
                    "[edge-in-plane] {tag:?} edge {copies:?}: DECLINE (incident to a \
                     non-planar face)"
                );
            }
            continue;
        }
        let e = &x.edges()[copies[0] as usize];
        let p0 = x.vertices()[e.start as usize].point;
        let p1 = x.vertices()[e.end as usize].point;
        let (a0, a1) = (p0.as_array(), p1.as_array());
        for f in &faces {
            let band = f.band(&[a0, a1]);
            if f.gap(a0).abs() > band / 100.0 || f.gap(a1).abs() > band / 100.0 {
                continue;
            }
            let q0 = f.project(a0);
            let q1 = f.project(a1);
            let scale = a0
                .iter()
                .chain(a1.iter())
                .fold(0.0f64, |m, &c| m.max(c.abs()));
            let margin = TAU_MODEL * (1.0 + scale);
            // Endpoint classification: strictly inside (≥ margin from the
            // boundary), strictly outside, or ON the boundary (declined — a
            // corner junction of higher order).
            let classify = |q: [f64; 2]| -> Option<bool> {
                if f.boundary_distance(q) <= margin {
                    None
                } else {
                    Some(f.contains(q))
                }
            };
            let (Some(in0), Some(in1)) = (classify(q0), classify(q1)) else {
                sc.declined += 1;
                if probe() {
                    eprintln!(
                        "[edge-in-plane] {tag:?} edge {copies:?} vs partner face {}: DECLINE \
                         (endpoint on the face boundary)",
                        f.idx
                    );
                }
                continue;
            };
            // Crossings with every loop edge.
            let mut crossings: Vec<(f64, Point3, u32)> = Vec::new();
            let mut declined = false;
            let loops: Vec<(&Vec<[f64; 2]>, &Vec<u32>)> =
                std::iter::once((&f.outer, &f.outer_edges))
                    .chain(f.holes.iter().zip(f.holes_edges.iter()))
                    .collect();
            'loops: for (pts, es) in loops {
                let n = pts.len();
                for k in 0..n {
                    let (s0, s1) = (pts[k], pts[(k + 1) % n]);
                    let oa = orient2d(q0, q1, s0);
                    let ob = orient2d(q0, q1, s1);
                    let oc = orient2d(s0, s1, q0);
                    let od = orient2d(s0, s1, q1);
                    if oa * ob > 0.0 || oc * od > 0.0 {
                        continue; // no contact
                    }
                    if oa == 0.0 || ob == 0.0 || oc == 0.0 || od == 0.0 {
                        declined = true; // touching / collinear contact
                        break 'loops;
                    }
                    // Proper crossing: parameter along q0→q1.
                    let t = oc / (oc - od);
                    let p = [
                        a0[0] + t * (a1[0] - a0[0]),
                        a0[1] + t * (a1[1] - a0[1]),
                        a0[2] + t * (a1[2] - a0[2]),
                    ];
                    let ye = &y.edges()[es[k] as usize];
                    let ys0 = y.vertices()[ye.start as usize].point.as_array();
                    let ys1 = y.vertices()[ye.end as usize].point.as_array();
                    if dist3(p, a0) <= margin
                        || dist3(p, a1) <= margin
                        || dist3(p, ys0) <= margin
                        || dist3(p, ys1) <= margin
                    {
                        declined = true; // corner contact
                        break 'loops;
                    }
                    crossings.push((t, Point3::new(p[0], p[1], p[2]), es[k]));
                }
            }
            if declined {
                sc.declined += 1;
                if probe() {
                    eprintln!(
                        "[edge-in-plane] {tag:?} edge {copies:?} vs partner face {}: DECLINE \
                         (touching / collinear / corner contact)",
                        f.idx
                    );
                }
                continue;
            }
            if crossings.is_empty() && !in0 && !in1 {
                continue; // the edge lies in the plane but outside the face
            }
            crossings.sort_by(|u, v| u.0.partial_cmp(&v.0).unwrap_or(std::cmp::Ordering::Equal));
            // Stations along the edge: the endpoints and the crossings.
            let mut stations: Vec<(f64, Point3)> = Vec::with_capacity(crossings.len() + 2);
            stations.push((0.0, p0));
            for &(t, p, _) in &crossings {
                stations.push((t, p));
            }
            stations.push((1.0, p1));
            let mut segs: Vec<[Point3; 2]> = Vec::new();
            for w in stations.windows(2) {
                let (ta, pa) = w[0];
                let (tb, pb) = w[1];
                let tm = 0.5 * (ta + tb);
                let mid = [
                    a0[0] + tm * (a1[0] - a0[0]),
                    a0[1] + tm * (a1[1] - a0[1]),
                    a0[2] + tm * (a1[2] - a0[2]),
                ];
                if f.contains(f.project(mid)) {
                    segs.push([pa, pb]);
                }
            }
            if segs.is_empty() {
                continue;
            }
            if probe() {
                eprintln!(
                    "[edge-in-plane] {tag:?} edge {copies:?} lies in partner face {}: {} \
                     crossing(s), {} inside sub-segment(s), endpoints inside = ({in0}, {in1})",
                    f.idx,
                    crossings.len(),
                    segs.len()
                );
            }
            // Emit: crossings into every copy of BOTH edges (one mint each).
            for &(_, p, ye) in &crossings {
                for &c in copies {
                    push_point(&mut sc.edge_x, c, p);
                }
                let ycopies = y_copies_of.get(&ye).cloned().unwrap_or_else(|| vec![ye]);
                for c in ycopies {
                    push_point(&mut sc.edge_y, c, p);
                }
                if !sc.mints.contains(&p) {
                    sc.mints.push(p);
                }
            }
            // Endpoints inside F: interior Steiner points of F.
            if in0 {
                push_point(&mut sc.face_y, f.idx, p0);
            }
            if in1 {
                push_point(&mut sc.face_y, f.idx, p1);
            }
            // The inside sub-segments: constraints of F's CDT.
            let slot = sc.cons_y.entry(f.idx).or_default();
            for s in segs {
                let dup = slot
                    .iter()
                    .any(|&[u, v]| (u == s[0] && v == s[1]) || (u == s[1] && v == s[0]));
                if !dup {
                    slot.push(s);
                }
            }
        }
    }
    sc.cons_y.retain(|_, v| !v.is_empty());
    sc
}

fn merge_points(into: &mut BTreeMap<u32, Vec<Point3>>, from: BTreeMap<u32, Vec<Point3>>) {
    for (k, pts) in from {
        for p in pts {
            push_point(into, k, p);
        }
    }
}

/// Arm 2 — conformity overrides for the (identified) operands.
pub(crate) fn conformity_overrides(a: &BRep, b: &BRep) -> EdgeInPlaneOverrides {
    let (paired_a, paired_b) = paired_faces(a, b);
    let mut out = EdgeInPlaneOverrides::default();
    let sa = side_contacts(a, b, &paired_a, InputId::A);
    let sb = side_contacts(b, a, &paired_b, InputId::B);
    // A's edges in B's faces: X = A, Y = B.
    merge_points(&mut out.edge_a, sa.edge_x);
    merge_points(&mut out.edge_b, sa.edge_y);
    merge_points(&mut out.face_b, sa.face_y);
    out.cons_b = crate::brep::merge_face_constraints(&out.cons_b, &sa.cons_y);
    // B's edges in A's faces: X = B, Y = A.
    merge_points(&mut out.edge_b, sb.edge_x);
    merge_points(&mut out.edge_a, sb.edge_y);
    merge_points(&mut out.face_a, sb.face_y);
    out.cons_a = crate::brep::merge_face_constraints(&out.cons_a, &sb.cons_y);
    for p in sa.mints.into_iter().chain(sb.mints) {
        if !out.mints.contains(&p) {
            out.mints.push(p);
        }
    }
    out.declined = sa.declined + sb.declined;
    if probe() {
        eprintln!(
            "[edge-in-plane] overrides: edge_a={} face_a={} cons_a={} edge_b={} face_b={} \
             cons_b={} mints={} declined={}",
            out.edge_a.len(),
            out.face_a.len(),
            out.cons_a.values().map(Vec::len).sum::<usize>(),
            out.edge_b.len(),
            out.face_b.len(),
            out.cons_b.values().map(Vec::len).sum::<usize>(),
            out.mints.len(),
            out.declined
        );
    }
    out
}
