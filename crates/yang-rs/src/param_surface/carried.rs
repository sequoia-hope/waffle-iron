//! Parameter coordinates carried on every mesh vertex (roadmap F5a, spec
//! `specs/yang_f5a_surface_interface.md` §4; matrix row 4.2-24).
//!
//! Yang 2025 keeps each mesh vertex's (u, v): Stage 1 samples the patches in
//! the parameter domain (`refs/text/yang2025_hybrid_boolean.txt:371-374`),
//! and after the mesh intersection each intersection point is mapped back to
//! both surfaces by barycentric coordinates in the triangles that meet there
//! (§4.2.3, `:510-519`). §4.3 optimizes from those coordinates and §4.4.1
//! triangulates in the parameter domain; F4 and F2 are their consumers.
//!
//! [`VertexParamMap`] is the vertex counterpart of the per-triangle
//! `TriangleErrorBoundMap`: one entry list per mesh vertex, in lockstep with
//! `mesh.verts`, holding the vertex's (u, v) on every face it lies on.
//!
//! - **Stage 1 / Stage 0.** [`VertexParamMap::for_operand`] assigns each
//!   operand vertex its (u, v) on every face whose triangles it is a corner
//!   of, by the closed-form inverse ([`super::AnalyticInverse`], ledger
//!   N109). The tessellators build their points from angles in frames that
//!   are not yet shown to be the pinned embedding's, so they do not write
//!   those angles yet; the inverse stands in for them (spec §4).
//! - **Stage 2.** [`VertexParamMap::for_arrangement`] is §4.2.3: an
//!   arrangement vertex's (u, v) on a face is the barycentric interpolation
//!   of its source triangle's corners. The paper calls these approximate
//!   (`:518-519`).
//! - **Stages 4 and 5.** Every vertex mutator keeps the map in lockstep: a
//!   moved vertex is re-inverted, seeded with its old (u, v) for the branch;
//!   an appended one gets the inverse on the faces it is built on.
//!
//! Nothing reads the map: `YANG_UV_CENSUS=<file>` ([`census`]) measures it.
#![cfg_attr(not(test), allow(dead_code))]

use super::{AnalyticInverse, ParamDomain, ParamSurface, Uv};
use crate::brep::{BRepFace, InputId};
use crate::Surface;
use cad_primitives::Point3;

/// A face of one input solid.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct FaceRef {
    pub input: InputId,
    pub face: u32,
}

/// A vertex's parameters on one face. The face's surface is carried with
/// it so a mutator can re-invert a moved vertex without the input solids.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct VertexParam {
    pub face: FaceRef,
    pub surface: Surface,
    pub uv: Uv,
}

/// For each mesh vertex, its (u, v) on every face it lies on (spec §4).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct VertexParamMap {
    pub params: Vec<Vec<VertexParam>>,
    /// Entries the inverse refused (a cone point behind the apex, a
    /// non-finite point). Counted, never clamped; the census reports it.
    pub refused: usize,
    /// Entries [`VertexParamMap::settle`] added for a face a vertex came to
    /// lie on by incidence (a collapse re-pointing a triangle, a CDT seed).
    pub settled: usize,
    /// The surface of every face an entry or a triangle attribution can
    /// name, so [`VertexParamMap::settle`] needs only the attribution.
    pub surfaces: std::collections::BTreeMap<FaceRef, Surface>,
}

impl VertexParamMap {
    /// `n` vertices with no parameters (a mesh with no face lineage).
    pub(crate) fn bare(n: usize) -> Self {
        Self {
            params: vec![Vec::new(); n],
            refused: 0,
            settled: 0,
            surfaces: std::collections::BTreeMap::new(),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.params.len()
    }

    pub(crate) fn assert_lockstep(&self, n_verts: usize, context: &str) {
        assert_eq!(
            self.params.len(),
            n_verts,
            "{context}: vertex params ({}) must be 1:1 with mesh.verts ({n_verts})",
            self.params.len(),
        );
    }

    /// Vertex `v`'s parameters on `face`, if it has them.
    pub(crate) fn on_face(&self, v: u32, face: FaceRef) -> Option<Uv> {
        self.params
            .get(v as usize)?
            .iter()
            .find(|e| e.face == face)
            .map(|e| e.uv)
    }

    /// Stage-1 / Stage-0 hand-off for one operand mesh: every vertex gets
    /// its (u, v) on each face it is a triangle corner of, by the inverse.
    /// A mesh whose `tri_face` lineage is absent gets bare vertices.
    pub(crate) fn for_operand(
        verts: &[Point3],
        tris: &[[u32; 3]],
        tri_face: &[u32],
        faces: &[BRepFace],
        input: InputId,
    ) -> Self {
        let mut out = Self::bare(verts.len());
        for (fi, f) in faces.iter().enumerate() {
            out.surfaces.insert(
                FaceRef {
                    input,
                    face: fi as u32,
                },
                f.surface,
            );
        }
        if tri_face.len() != tris.len() {
            return out;
        }
        for (t, tri) in tris.iter().enumerate() {
            let fi = tri_face[t];
            let Some(f) = faces.get(fi as usize) else {
                continue;
            };
            let face = FaceRef { input, face: fi };
            for &c in tri {
                if out.on_face(c, face).is_none() {
                    let p = verts[c as usize].as_array();
                    out.insert_inverted(c, face, f.surface, p, None);
                }
            }
        }
        out
    }

    /// Yang 2025 §4.2.3 (`:510-519`): each arrangement vertex's (u, v) on
    /// the face of every source triangle it is a corner of, interpolated from
    /// that triangle's corners by the vertex's barycentric coordinates in it.
    /// A vertex bit-equal to a source corner takes that corner's (u, v).
    /// `operands[k]` is input `k`'s mesh, `tri_face` and map. An arrangement
    /// without provenance (`la.source` empty) gets bare vertices.
    pub(crate) fn for_arrangement(
        la: &cherchi_rs::labeled_arrangement::LabeledArrangement,
        operands: [OperandParams<'_>; 2],
    ) -> Self {
        let mut out = Self::bare(la.mesh.verts.len());
        out.refused = operands[0].params.refused + operands[1].params.refused;
        for op in &operands {
            out.surfaces
                .extend(op.params.surfaces.iter().map(|(k, v)| (*k, *v)));
        }
        if la.source.len() != la.mesh.tris.len() {
            return out;
        }
        for (t, tri) in la.mesh.tris.iter().enumerate() {
            for &(cherchi_rs::labeled_arrangement::InputId(k), raw_t) in &la.source[t] {
                let Some(op) = operands.get(k as usize) else {
                    continue;
                };
                let Some(&fi) = op.tri_face.get(raw_t as usize) else {
                    continue;
                };
                let Some(f) = op.faces.get(fi as usize) else {
                    continue;
                };
                let face = FaceRef {
                    input: op.input,
                    face: fi,
                };
                let src = op.tris[raw_t as usize];
                let corners = src.map(|c| op.verts[c as usize]);
                let Some(corner_uv) = src
                    .iter()
                    .map(|&c| op.params.on_face(c, face))
                    .collect::<Option<Vec<Uv>>>()
                else {
                    continue;
                };
                let corner_uv = [corner_uv[0], corner_uv[1], corner_uv[2]];
                for &v in tri {
                    if out.on_face(v, face).is_some() {
                        continue;
                    }
                    let p = la.mesh.verts[v as usize];
                    let uv = match corners.iter().position(|&c| bit_eq(c, p)) {
                        Some(k) => Some(corner_uv[k]),
                        None => barycentric(corners, p)
                            .map(|w| interpolate(&f.surface.domain(), corner_uv, w)),
                    };
                    match uv {
                        Some(uv) => out.params[v as usize].push(VertexParam {
                            face,
                            surface: f.surface,
                            uv,
                        }),
                        None => out.refused += 1,
                    }
                }
            }
        }
        out
    }

    /// The map of a mesh compacted from this one: `members[c]` lists the old
    /// vertices that became new vertex `c` (one, or a weld cluster). The
    /// new vertex holds the union of their entries, first member first.
    pub(crate) fn compacted(&self, members: &[Vec<u32>]) -> Self {
        let mut out = Self::bare(members.len());
        out.refused = self.refused;
        out.settled = self.settled;
        out.surfaces = self.surfaces.clone();
        for (c, group) in members.iter().enumerate() {
            for &old in group {
                for e in &self.params[old as usize] {
                    if !out.params[c].iter().any(|x| x.face == e.face) {
                        out.params[c].push(*e);
                    }
                }
            }
        }
        out
    }

    /// Apply a dense old → new vertex remap (`None`: the vertex is gone).
    pub(crate) fn remap(&mut self, remap: &[Option<u32>]) {
        let n_new = remap.iter().flatten().count();
        let old = std::mem::take(&mut self.params);
        let mut out = vec![Vec::new(); n_new];
        for (i, slot) in remap.iter().enumerate() {
            if let (Some(c), Some(entries)) = (slot, old.get(i)) {
                out[*c as usize] = entries.clone();
            }
        }
        self.params = out;
    }

    pub(crate) fn truncate(&mut self, n: usize) {
        self.params.truncate(n);
    }

    /// Append a copy of vertex `v` (same position, same faces): a pinch
    /// split gives each sheet its own copy.
    pub(crate) fn push_copy(&mut self, v: u32) {
        let entries = self.params.get(v as usize).cloned().unwrap_or_default();
        self.params.push(entries);
    }

    /// Append `n` vertices whose faces their construction gives by
    /// incidence alone (CDT seeds of a patch, on-curve inserts of a seam):
    /// [`Self::settle`] assigns them once their triangles exist.
    pub(crate) fn push_bare(&mut self, n: usize) {
        self.params.extend(std::iter::repeat_with(Vec::new).take(n));
    }

    /// Give every vertex its (u, v) on each face it is now a triangle corner
    /// of and has none for, by the inverse seeded with another corner's
    /// (u, v) on that face (the branch the triangle lives on). A triangle's
    /// face is its attribution. Returns the entries added.
    pub(crate) fn settle(
        &mut self,
        verts: &[Point3],
        tris: &[[u32; 3]],
        attributions: &[Option<crate::brep::TriangleAttribution>],
    ) -> usize {
        self.assert_lockstep(verts.len(), "VertexParamMap::settle");
        let mut added = 0usize;
        for (t, tri) in tris.iter().enumerate() {
            let Some(Some(at)) = attributions.get(t) else {
                continue;
            };
            let face = FaceRef {
                input: at.input,
                face: at.face,
            };
            let Some(&surface) = self.surfaces.get(&face) else {
                continue;
            };
            for &c in tri {
                if self.on_face(c, face).is_some() {
                    continue;
                }
                let seed = tri.iter().find_map(|&o| self.on_face(o, face));
                let before = self.params[c as usize].len();
                self.insert_inverted(c, face, surface, verts[c as usize].as_array(), seed);
                added += self.params[c as usize].len() - before;
            }
        }
        self.settled += added;
        added
    }

    /// Vertex `v` moved to `p`: every entry re-inverted, seeded with its old
    /// (u, v) for the branch.
    pub(crate) fn moved(&mut self, v: u32, p: Point3) {
        let Some(old) = self.params.get_mut(v as usize).map(std::mem::take) else {
            return;
        };
        for e in old {
            self.insert_inverted(v, e.face, e.surface, p.as_array(), Some(e.uv));
        }
    }

    /// Vertex `survivor` took `victim`'s position and inherits its
    /// triangles: it lies on the faces of both. The victim's entries hold
    /// for the new position; the survivor's own are re-inverted there.
    pub(crate) fn merged_into(&mut self, survivor: u32, victim: u32, p: Point3) {
        let (s, w) = (survivor as usize, victim as usize);
        if s >= self.params.len() || w >= self.params.len() {
            return;
        }
        let taken = self.params[w].clone();
        let own = std::mem::replace(&mut self.params[s], taken);
        for e in own {
            if self.on_face(survivor, e.face).is_none() {
                self.insert_inverted(survivor, e.face, e.surface, p.as_array(), Some(e.uv));
            }
        }
    }

    fn insert_inverted(
        &mut self,
        v: u32,
        face: FaceRef,
        surface: Surface,
        p: [f64; 3],
        seed: Option<Uv>,
    ) {
        match surface.inverse(p, seed) {
            Ok(uv) => self.params[v as usize].push(VertexParam { face, surface, uv }),
            Err(_) => self.refused += 1,
        }
    }
}

/// Move mesh vertex `v` to `p` and re-invert its parameters there.
pub(crate) fn move_vertex(mesh: &mut crate::Mesh, params: &mut VertexParamMap, v: u32, p: Point3) {
    mesh.verts[v as usize] = p;
    params.moved(v, p);
}

/// One operand's Stage-2 inputs to [`VertexParamMap::for_arrangement`].
pub(crate) struct OperandParams<'a> {
    pub input: InputId,
    pub verts: &'a [Point3],
    pub tris: &'a [[u32; 3]],
    pub tri_face: &'a [u32],
    pub faces: &'a [BRepFace],
    pub params: &'a VertexParamMap,
}

fn bit_eq(a: Point3, b: Point3) -> bool {
    let (a, b) = (a.as_array(), b.as_array());
    (0..3).all(|k| a[k].to_bits() == b[k].to_bits())
}

/// Barycentric coordinates of `p`'s projection into the plane of `tri`;
/// `None` for a degenerate triangle.
///
/// Each coordinate is a signed sub-triangle area against the triangle's own
/// normal `n` (`λ_a = n·((c − b) × (p − b)) / n·n`, cyclically), not the
/// normal equations of the edge Gram matrix: those square the triangle's
/// condition number, and a sliver (two corners 1.7e-4 apart, the third 100
/// away, R0003's gear caps) then misplaces a point along its long edge by
/// 4e-3. A sub-area is a cross product of differences that stay accurate.
pub(crate) fn barycentric(tri: [Point3; 3], p: Point3) -> Option<[f64; 3]> {
    let [a, b, c] = tri.map(|q| q.as_array());
    let p = p.as_array();
    let sub = |x: [f64; 3], y: [f64; 3]| [x[0] - y[0], x[1] - y[1], x[2] - y[2]];
    let dot = |x: [f64; 3], y: [f64; 3]| x[0] * y[0] + x[1] * y[1] + x[2] * y[2];
    let cross = |x: [f64; 3], y: [f64; 3]| {
        [
            x[1] * y[2] - x[2] * y[1],
            x[2] * y[0] - x[0] * y[2],
            x[0] * y[1] - x[1] * y[0],
        ]
    };
    let n = cross(sub(b, a), sub(c, a));
    let nn = dot(n, n);
    if nn <= 0.0 || !nn.is_finite() {
        return None;
    }
    let la = dot(n, cross(sub(c, b), sub(p, b)));
    let lb = dot(n, cross(sub(a, c), sub(p, c)));
    let lc = dot(n, cross(sub(b, a), sub(p, a)));
    let sum = la + lb + lc;
    if sum == 0.0 || !sum.is_finite() {
        return None;
    }
    Some([la / sum, lb / sum, lc / sum])
}

/// `Σ w_k · uv_k`, with each periodic coordinate unwrapped to the first
/// corner's period first (spec §4). A corner on a degenerate iso-line (a
/// pole, an apex — any u names it) carries no u of its own: u is
/// interpolated over the other corners alone.
pub(crate) fn interpolate(dom: &ParamDomain, uv: [Uv; 3], w: [f64; 3]) -> Uv {
    let degenerate = |c: Uv| {
        dom.degenerate.iter().any(|d| match *d {
            super::Degeneracy::VIso(v) => c.y() == v,
            super::Degeneracy::UIso(u) => c.x() == u,
        })
    };
    let live: Vec<usize> = (0..3).filter(|&k| !degenerate(uv[k])).collect();
    let unwrap = |x: f64, reference: f64, period: Option<f64>| match period {
        Some(p) => x + p * ((reference - x) / p).round(),
        None => x,
    };
    let u = match live.first() {
        None => uv[0].x(),
        Some(&r) => {
            let u_ref = uv[r].x();
            let (mut num, mut den) = (0.0, 0.0);
            for &k in &live {
                num += w[k] * unwrap(uv[k].x(), u_ref, dom.u.period);
                den += w[k];
            }
            if den != 0.0 && den.is_finite() {
                num / den
            } else {
                u_ref
            }
        }
    };
    let v_ref = uv[0].y();
    let v = (0..3)
        .map(|k| w[k] * unwrap(uv[k].y(), v_ref, dom.v.period))
        .sum();
    Uv::new(u, v)
}

/// [`census`] with each face's d_ε read from its input solid.
pub(crate) fn census_of(
    stage: &str,
    map: &VertexParamMap,
    mesh: &crate::Mesh,
    a: &crate::BRep,
    b: &crate::BRep,
) {
    if std::env::var_os("YANG_UV_CENSUS").is_none() {
        return;
    }
    let budget = |f: FaceRef| {
        let operand = match f.input {
            InputId::A => a,
            InputId::B => b,
        };
        crate::brep::face_error_budget(operand, f.face as usize)
    };
    census(stage, map, &mesh.verts, &mesh.tris, &budget);
}

/// Print-only measure of a map (`YANG_UV_CENSUS=<file>`, spec §4): one line
/// per call, keyed by `ASSAY_CASE`, with the vertex count, the vertices a
/// triangle uses that have no parameters, the refused entries, the largest
/// `|eval(uv) − p| / d_ε` over curved faces (`budget` gives a face's d_ε),
/// and the largest `|eval(uv) − p|` over planar ones. No effect on any
/// result.
pub(crate) fn census(
    stage: &str,
    map: &VertexParamMap,
    verts: &[Point3],
    tris: &[[u32; 3]],
    budget: &dyn Fn(FaceRef) -> Option<f64>,
) {
    let Some(path) = std::env::var_os("YANG_UV_CENSUS") else {
        return;
    };
    let mut bare = 0usize;
    let mut entries = 0usize;
    let mut max_ratio = 0.0_f64;
    let mut max_plane = 0.0_f64;
    let mut eval_failed = 0usize;
    let mut no_budget = 0usize;
    let mut used = vec![false; map.len()];
    for tri in tris {
        for &c in tri {
            if let Some(u) = used.get_mut(c as usize) {
                *u = true;
            }
        }
    }
    for (v, list) in map.params.iter().enumerate() {
        if list.is_empty() && used[v] {
            bare += 1;
        }
        let Some(p) = verts.get(v).map(|q| q.as_array()) else {
            continue;
        };
        for e in list {
            entries += 1;
            let Ok(s) = e.surface.eval(e.uv) else {
                eval_failed += 1;
                continue;
            };
            let d = ((s[0] - p[0]).powi(2) + (s[1] - p[1]).powi(2) + (s[2] - p[2]).powi(2)).sqrt();
            if e.surface.is_affine() {
                max_plane = max_plane.max(d);
            } else {
                match budget(e.face).filter(|b| *b > 0.0 && b.is_finite()) {
                    Some(b) => max_ratio = max_ratio.max(d / b),
                    None => no_budget += 1,
                }
            }
        }
    }
    let case = std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        // One `write_all` per line: `writeln!` may split a line into several
        // writes, and concurrent assay cases append to one file.
        let line = format!(
            "{case}\t{stage}\tverts={}\tentries={entries}\tbare={bare}\trefused={}\t\
             settled={}\teval_failed={eval_failed}\tno_budget={no_budget}\t\
             max_curved_ratio={max_ratio:.3e}\tmax_plane_abs={max_plane:.3e}\n",
            map.len(),
            map.refused,
            map.settled,
        );
        let _ = f.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_primitives::Vector3;
    use cherchi_rs::labeled_arrangement::{InputId as LaInputId, LabeledArrangement};
    use std::f64::consts::{FRAC_PI_2, PI};

    fn pt(p: [f64; 3]) -> Point3 {
        Point3::new(p[0], p[1], p[2])
    }

    fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }

    fn face(surface: Surface) -> BRepFace {
        BRepFace {
            surface,
            outer_loop: Vec::new(),
            inner_loops: Vec::new(),
            reversed: false,
        }
    }

    fn cylinder() -> Surface {
        Surface::Cylinder {
            axis_point: Point3::new(0.1, -0.2, 0.3),
            axis_dir: Vector3::new(0.3, -0.4, 0.86),
            radius: 1.7,
        }
    }

    /// R0003's sliver (B face 3, raw triangle 2215): a point on the long
    /// edge a–c is interpolated back onto itself on the plane. The Gram
    /// normal equations put it 4.3e-3 away; the sub-area form is exact to
    /// rounding at this coordinate scale.
    #[test]
    fn barycentric_holds_a_sliver() {
        let a = [-215.34264753536232, -110.01520030145028, 12.96835856918974];
        let b = [-160.71153233984992, -185.3912306569187, 42.495117515095316];
        let c = [-160.71149109411056, -185.39106720150838, 42.49511751509532];
        let p = [-176.4858929311487, -163.62676106650585, 33.96945210914802];
        let w = barycentric([pt(a), pt(b), pt(c)], pt(p)).unwrap();
        let q: Vec<f64> = (0..3)
            .map(|k| w[0] * a[k] + w[1] * b[k] + w[2] * c[k])
            .collect();
        assert!(dist([q[0], q[1], q[2]], p) < 1e-9, "{w:?}");
        assert!(w[1].abs() < 1e-6);
    }

    /// §4.2.3 across a seam: corners on both sides of u = 0 are unwrapped to
    /// the first corner's period before interpolating, so the interpolated
    /// point stays between them (and on the surface to within the chord).
    #[test]
    fn interpolation_unwraps_a_seam_straddle() {
        let s = cylinder();
        let uv = [
            Uv::new(2.0 * PI - 0.05, 0.0),
            Uv::new(0.05, 0.0),
            Uv::new(0.0, 0.2),
        ];
        let corners = uv.map(|q| pt(s.eval(q).unwrap()));
        let centroid = {
            let c = corners.map(|q| q.as_array());
            pt([
                (c[0][0] + c[1][0] + c[2][0]) / 3.0,
                (c[0][1] + c[1][1] + c[2][1]) / 3.0,
                (c[0][2] + c[1][2] + c[2][2]) / 3.0,
            ])
        };
        let w = barycentric(corners, centroid).unwrap();
        let q = interpolate(&s.domain(), uv, w);
        let u = q.x().rem_euclid(2.0 * PI);
        assert!(!(0.05..=2.0 * PI - 0.05).contains(&u), "u = {}", q.x());
        let chord = 1.7 * (1.0 - (0.05_f64).cos());
        assert!(dist(s.eval(q).unwrap(), centroid.as_array()) <= 2.0 * chord);
    }

    /// A corner on a degenerate iso-line (here the sphere's pole) names no
    /// azimuth: u is interpolated over the other corners alone.
    #[test]
    fn interpolation_skips_a_pole_azimuth() {
        let s = Surface::Sphere {
            center: Point3::new(0.0, 0.0, 0.0),
            radius: 2.0,
        };
        let uv = [
            Uv::new(0.0, FRAC_PI_2),
            Uv::new(1.0, 1.2),
            Uv::new(1.2, 1.2),
        ];
        let q = interpolate(&s.domain(), uv, [0.5, 0.25, 0.25]);
        assert!((q.x() - 1.1).abs() < 1e-15);
        assert!((q.y() - (0.5 * FRAC_PI_2 + 0.6)).abs() < 1e-15);
    }

    /// The Stage-1 operand map gives every vertex its (u, v) on every face
    /// it is a corner of, and the Stage-2 map interpolates a split vertex
    /// in its source triangle; a vertex bit-equal to a source corner takes
    /// the corner's coordinates exactly.
    #[test]
    fn operand_and_arrangement_maps() {
        let s = cylinder();
        let plane = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 5.0,
        };
        let uv = [Uv::new(0.2, 0.0), Uv::new(0.6, 0.0), Uv::new(0.4, 0.5)];
        let verts: Vec<Point3> = uv.iter().map(|&q| pt(s.eval(q).unwrap())).collect();
        let tris = vec![[0u32, 1, 2]];
        let faces = vec![face(plane), face(s)];
        let map = VertexParamMap::for_operand(&verts, &tris, &[1], &faces, InputId::A);
        map.assert_lockstep(3, "test");
        let fa = FaceRef {
            input: InputId::A,
            face: 1,
        };
        for (k, q) in uv.iter().enumerate() {
            let got = map.on_face(k as u32, fa).unwrap();
            assert!((got.x() - q.x()).abs() < 1e-12 && (got.y() - q.y()).abs() < 1e-12);
            assert_eq!(map.params[k].len(), 1, "a vertex lies on face 1 only");
        }
        // The arrangement splits the triangle at the midpoint of edge 0–1.
        let m = {
            let (p, q) = (verts[0].as_array(), verts[1].as_array());
            pt([
                0.5 * (p[0] + q[0]),
                0.5 * (p[1] + q[1]),
                0.5 * (p[2] + q[2]),
            ])
        };
        let la = LabeledArrangement {
            mesh: crate::Mesh::new(
                vec![verts[0], verts[1], verts[2], m],
                vec![[0, 3, 2], [3, 1, 2]],
            ),
            surface: vec![vec![LaInputId(0)]; 2],
            inside: vec![vec![false, false]; 2],
            patch: vec![0, 0],
            source: vec![vec![(LaInputId(0), 0)]; 2],
            intersection_edges: Default::default(),
            lpi_through_vertex: Vec::new(),
            num_inputs: 2,
        };
        let empty = VertexParamMap::bare(0);
        let arr = VertexParamMap::for_arrangement(
            &la,
            [
                OperandParams {
                    input: InputId::A,
                    verts: &verts,
                    tris: &tris,
                    tri_face: &[1],
                    faces: &faces,
                    params: &map,
                },
                OperandParams {
                    input: InputId::B,
                    verts: &[],
                    tris: &[],
                    tri_face: &[],
                    faces: &[],
                    params: &empty,
                },
            ],
        );
        arr.assert_lockstep(4, "test");
        assert_eq!(arr.on_face(1, fa), map.on_face(1, fa));
        let mid = arr.on_face(3, fa).unwrap();
        assert!(
            (mid.x() - 0.4).abs() < 1e-12 && mid.y().abs() < 1e-12,
            "{mid:?}"
        );
        assert_eq!(arr.surfaces.get(&fa), Some(&s));
    }

    /// The mutators keep the map 1:1 with the vertices: a copy duplicates,
    /// a move re-inverts on the old branch, a merge holds both faces at the
    /// victim's position, a remap drops and renumbers, settle fills the
    /// faces a vertex came to lie on by incidence.
    #[test]
    fn mutators_keep_lockstep() {
        let s = cylinder();
        let fa = FaceRef {
            input: InputId::A,
            face: 0,
        };
        let fb = FaceRef {
            input: InputId::B,
            face: 0,
        };
        let plane = Surface::Plane {
            normal: Vector3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        let on = |u: f64, v: f64| pt(s.eval(Uv::new(u, v)).unwrap());
        let mut map = VertexParamMap::bare(0);
        map.surfaces.insert(fa, s);
        map.surfaces.insert(fb, plane);
        let mut verts = vec![on(2.0 * PI + 0.3, 0.0), on(0.5, 0.0), on(0.4, 0.4)];
        map.push_bare(3);
        let attribution = vec![Some(crate::brep::TriangleAttribution {
            input: InputId::A,
            face: 0,
        })];
        let tris = vec![[0u32, 1, 2]];
        assert_eq!(map.settle(&verts, &tris, &attribution), 3);
        // A move keeps the branch of the old coordinates.
        map.params[0][0].uv = Uv::new(2.0 * PI + 0.3, 0.0);
        let p = on(0.31, 0.1);
        verts[0] = p;
        map.moved(0, p);
        let got = map.on_face(0, fa).unwrap();
        assert!((got.x() - (2.0 * PI + 0.31)).abs() < 1e-12, "{got:?}");
        // A copy.
        map.push_copy(1);
        verts.push(verts[1]);
        assert_eq!(map.params[3], map.params[1]);
        // A merge: vertex 2 takes vertex 1's position and lies on both faces.
        map.params[1].push(VertexParam {
            face: fb,
            surface: plane,
            uv: Uv::new(9.0, 9.0),
        });
        map.merged_into(2, 1, verts[1]);
        assert_eq!(map.on_face(2, fb), Some(Uv::new(9.0, 9.0)));
        assert_eq!(map.on_face(2, fa), map.on_face(1, fa));
        // A remap drops vertex 1 and renumbers 2 → 1, 3 → 2.
        let before = map.params.clone();
        map.remap(&[Some(0), None, Some(1), Some(2)]);
        assert_eq!(map.len(), 3);
        assert_eq!(map.params[1], before[2]);
        assert_eq!(map.params[2], before[3]);
        map.truncate(2);
        map.assert_lockstep(2, "test");
    }
}
