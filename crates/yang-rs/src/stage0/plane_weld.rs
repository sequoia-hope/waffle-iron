//! Stage 0 — the §4.5.5 shared plane, at the B-Rep level (spec
//! `specs/yang_455_coplanar_plane_weld.md`).
//!
//! Yang §4.5.5 replaces the overlap of two coplanar faces with ONE shared
//! trimmed surface and generates identical meshes on it for both models.
//! `stage0_preprocess` does that for the mesh: it snaps every loop VERTEX of a
//! near-coplanar cross pair onto the group's canonical plane (face A's) and
//! lifts every overlay vertex onto that same plane. What it left alone was the
//! analytic geometry those vertices bound: face B's stored `Surface::Plane`
//! and the `Curve::Circle` (ellipse, conic) anchor points of the edges on it.
//! For bit-exact coplanar input that is no gap at all. For the near-coplanar
//! class the scan welds (`gap ≤ band/100`, spec `yang_178_subres_coplanar_gap_
//! stop.md`) it is: everything Stage 1 derives from the stored circle — the
//! uniform rim samples, the opposite-rim image of a crossing — sits on B's
//! plane while the seam vertex, the corners and every overlay mint sit on A's.
//!
//! Measured 2026-10-04 (`error_oct4.waffle`): a through-cut cylinder whose cap
//! was authored on the frame's top face from an f32-rounded sketch origin,
//! 2.2e-10 below it. Each rim ring carried the same crossing twice, once per
//! plane; where the geometry's symmetry put that azimuth on a uniform sample
//! the ring build refused, and past that the operand's own cap and lateral met
//! in an improper (T-junction) contact between the twins.
//!
//! This step runs BEFORE `stage0_preprocess`, on the operands the §4.5.5
//! edge-in-plane identification produced: for every plane group the scan
//! would overlay, every participating face whose unit plane is not already the
//! canonical one is rewritten onto it — its `Surface::Plane` (orientation
//! kept), the in-plane anchor point of every curved edge on it, and its loop
//! vertices (the same `Frame::snap` Stage 0 applies to the mesh). A group in
//! which every face already carries the canonical plane bit for bit is left
//! untouched, so bit-exact coplanar input (the whole generated corpus) is
//! byte-identical through this step. A scan that `stage0_preprocess` would
//! refuse (an intra-solid pair, a sub-resolution gap) is left to it.
//!
//! `YANG_PLANE_WELD=off|0` disables the step (dev A/B);
//! `YANG_PLANE_WELD_PROBE=1` prints every rewritten face and anchor.

use cad_primitives::Point3;

use super::{build_plane_groups, canonical_frame, Frame};
use crate::scan_near_coplanar;
use crate::{BRep, BRepEdge, BRepFace, BRepVertex, Curve, InputId, Surface, Vector3, YangError};

pub(crate) fn enabled() -> bool {
    !matches!(
        std::env::var("YANG_PLANE_WELD").as_deref(),
        Ok("off") | Ok("0")
    )
}

fn probe() -> bool {
    std::env::var_os("YANG_PLANE_WELD_PROBE").is_some()
}

/// Weld every near-coplanar cross pair's faces onto their group's canonical
/// plane. `Ok(None)` = nothing to do (no pair, a scan Stage 0 refuses, or every
/// participating face already on its canonical plane) — the operands are
/// untouched and the caller keeps its references.
pub(crate) fn weld_coplanar_planes(a: &BRep, b: &BRep) -> Result<Option<(BRep, BRep)>, YangError> {
    let scan = scan_near_coplanar(a, b);
    if scan.intra.is_some() || scan.cross.is_empty() || scan.cross.iter().any(|p| p.sub_resolution)
    {
        return Ok(None);
    }
    let groups = build_plane_groups(&scan.cross);

    let mut work: [Weld; 2] = [Weld::new(a), Weld::new(b)];
    for g in &groups {
        let Some(frame) = canonical_frame(a, g.faces_a[0]) else {
            // Degenerate canonical normal: `stage0_preprocess` refuses it loudly.
            continue;
        };
        for (slot, faces, tag) in [
            (0usize, &g.faces_a, InputId::A),
            (1usize, &g.faces_b, InputId::B),
        ] {
            for &fi in faces {
                work[slot].weld_face(fi, &frame, tag);
            }
        }
    }

    let [wa, wb] = work;
    if !wa.changed && !wb.changed {
        return Ok(None);
    }
    let na = wa.finish(a)?;
    let nb = wb.finish(b)?;
    Ok(Some((na, nb)))
}

/// One operand's pending rewrite.
struct Weld {
    verts: Vec<BRepVertex>,
    edges: Vec<BRepEdge>,
    faces: Vec<BRepFace>,
    changed: bool,
}

impl Weld {
    fn new(brep: &BRep) -> Self {
        Self {
            verts: brep.vertices().to_vec(),
            edges: brep.edges().to_vec(),
            faces: brep.faces().to_vec(),
            changed: false,
        }
    }

    /// Rewrite face `fi` onto `frame`'s plane if its unit plane is not already
    /// that plane bit for bit. Orientation is the face's own: a B face stacked
    /// against A keeps its opposing normal.
    fn weld_face(&mut self, fi: usize, frame: &Frame, tag: InputId) {
        let Surface::Plane { normal, d } = self.faces[fi].surface else {
            return;
        };
        let na = normal.as_array();
        let len = (na[0] * na[0] + na[1] * na[1] + na[2] * na[2]).sqrt();
        if !len.is_finite() || len <= 0.0 {
            return;
        }
        let nf = [na[0] / len, na[1] / len, na[2] / len];
        let df = d / len;
        let dot = nf[0] * frame.n[0] + nf[1] * frame.n[1] + nf[2] * frame.n[2];
        if dot == 0.0 {
            return;
        }
        let s = if dot > 0.0 { 1.0 } else { -1.0 };
        let target_n = [s * frame.n[0], s * frame.n[1], s * frame.n[2]];
        let target_d = s * frame.d;
        if nf == target_n && df == target_d {
            return; // already the canonical plane — leave every bit alone
        }
        if probe() {
            eprintln!(
                "[plane-weld] {tag:?} face {fi}: plane n={nf:?} d={df:.17e} → n={target_n:?} \
                 d={target_d:.17e} (offset {:.3e}, normal {:.3e})",
                (df - target_d).abs(),
                ((nf[0] - target_n[0]).powi(2)
                    + (nf[1] - target_n[1]).powi(2)
                    + (nf[2] - target_n[2]).powi(2))
                .sqrt()
            );
        }
        self.faces[fi].surface = Surface::Plane {
            normal: Vector3::new(target_n[0], target_n[1], target_n[2]),
            d: target_d,
        };
        self.changed = true;

        // The face's loop vertices and the in-plane anchor of every curved
        // edge on it follow the plane. Straight edges carry no geometry of
        // their own.
        let loops: Vec<u32> = self.faces[fi]
            .outer_loop
            .iter()
            .chain(self.faces[fi].inner_loops.iter().flatten())
            .copied()
            .collect();
        for ei in loops {
            let e = &mut self.edges[ei as usize];
            for vi in [e.start, e.end] {
                let p = self.verts[vi as usize].point;
                let q = frame.snap(p);
                if q != p {
                    if probe() {
                        eprintln!("[plane-weld] {tag:?} v{vi} {p:?} → {q:?}");
                    }
                    self.verts[vi as usize] = BRepVertex { point: q };
                }
            }
            let anchor: Option<&mut Point3> = match &mut e.curve {
                Curve::LineSegment | Curve::SurfacePair { .. } => None,
                Curve::Circle { center, .. } => Some(center),
                Curve::Ellipse { center, .. } => Some(center),
                Curve::Parabola { vertex, .. } => Some(vertex),
                Curve::Hyperbola { center, .. } => Some(center),
            };
            if let Some(c) = anchor {
                let q = frame.snap(*c);
                if q != *c {
                    if probe() {
                        eprintln!("[plane-weld] {tag:?} edge {ei} anchor {c:?} → {q:?}");
                    }
                    *c = q;
                }
            }
        }
    }

    fn finish(self, brep: &BRep) -> Result<BRep, YangError> {
        if !self.changed {
            return Ok(brep.clone());
        }
        brep.rebuilt_with_geometry(self.verts, self.edges, self.faces)
    }
}
