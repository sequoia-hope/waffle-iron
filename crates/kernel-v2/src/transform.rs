//! Rigid transform of a solid: a deep copy of every reachable entity with
//! its geometry moved by `p' = R·p + t` (spec
//! `specs/custom_features_and_modeling_roadmap.md` §B1 — the one kernel
//! primitive a pattern needs).
//!
//! ## Why a copy, not an in-place move
//!
//! A pattern instances its seed N times; the seed itself survives as body 0.
//! Copying also keeps the arena's determinism contract intact: slots are
//! appended in ascending source-id order, never reused, so two identical
//! transform calls on identical arenas yield bit-identical arenas.
//!
//! ## What is exact
//!
//! A proper rigid motion maps every analytic surface and curve the arena
//! carries onto the same kind of surface/curve with transformed frame data —
//! a cylinder stays a cylinder of the same radius, a circle stays a circle.
//! No point is sampled, no tolerance is consulted; the copy is exact up to
//! floating-point rounding of the matrix product. Unit vectors are NOT
//! renormalized after rotation (an orthonormal matrix keeps them unit to
//! rounding; renormalizing would perturb an identity transform by an ulp —
//! the R0081 trap, memory `session_2026_09_15_f11_seam_feet_r0081_unit_normal`).
//!
//! ## What is refused
//!
//! An improper rotation (a reflection, `det R = −1`) would flip every face's
//! outward sense and every half-edge's traversal direction.
//! [`transform_solid`] rejects it loudly
//! ([`KernelV2Error::TransformNotRigid`]) rather than producing an
//! inside-out solid. Mirroring has its own entry point,
//! [`mirror_solid`], which does that orientation bookkeeping: the same
//! geometry map (a reflection maps every analytic surface and curve onto the
//! same kind, exactly) plus a reversal of every loop, so each copied face's
//! Newell normal still agrees with its mapped surface normal.
//!
//! ## Provenance
//!
//! Every copied face gets a fresh [`Pid`] and a journal edge
//! `(source_pid → copy_pid, Same)` under [`OpTag::Transform`], so
//! `face_lineage` of a copy's face roots at the SEED's originating face —
//! "this is copy 7's top face" is answerable from the journal.

use std::collections::BTreeMap;

use cad_primitives::Point3;
use waffle_types::kernel::{MirrorPlane, RigidPlacement};

use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, PairSurface,
    Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use crate::error::KernelV2Error;
use crate::journal::{EvoKind, Evolution, OpTag};
use crate::validate::validate_solid;

/// Tolerance on `R·Rᵀ − I` entries and on `det R − 1` for accepting a
/// rotation as proper orthonormal. A matrix built from a unit quaternion or
/// an axis-angle is orthonormal to a few ulps; anything off by 1e-9 is a
/// caller bug (a scaled or sheared matrix), not rounding.
pub const RIGID_ORTHONORMAL_TOLERANCE: f64 = 1e-9;

/// Determinant of a 3×3 row-major matrix.
fn det3(m: &[[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// Check that `placement.rotation` is a proper rotation (orthonormal,
/// `det = +1`) and that the translation is finite.
pub fn check_rigid(placement: &RigidPlacement) -> Result<(), KernelV2Error> {
    let m = &placement.rotation;
    for row in m {
        for &c in row {
            if !c.is_finite() {
                return Err(KernelV2Error::TransformNotRigid {
                    reason: "rotation has a non-finite entry",
                });
            }
        }
    }
    if placement.translation.iter().any(|t| !t.is_finite()) {
        return Err(KernelV2Error::TransformNotRigid {
            reason: "translation has a non-finite entry",
        });
    }
    // R·Rᵀ = I
    for i in 0..3 {
        for j in 0..3 {
            let dot = m[i][0] * m[j][0] + m[i][1] * m[j][1] + m[i][2] * m[j][2];
            let expect = if i == j { 1.0 } else { 0.0 };
            if (dot - expect).abs() > RIGID_ORTHONORMAL_TOLERANCE {
                return Err(KernelV2Error::TransformNotRigid {
                    reason: "rotation is not orthonormal",
                });
            }
        }
    }
    let det = det3(m);
    if (det - 1.0).abs() > RIGID_ORTHONORMAL_TOLERANCE {
        return Err(KernelV2Error::TransformNotRigid {
            reason: if det < 0.0 {
                "rotation is a reflection (det = -1); mirroring is not a rigid transform"
            } else {
                "rotation determinant is not +1"
            },
        });
    }
    Ok(())
}

/// The affine map a copy applies: `p' = L·p + t`. `L` is a proper rotation
/// for [`transform_solid`] and a reflection for [`mirror_solid`]; everything
/// between the two builders is the same code over this.
#[derive(Debug, Clone, Copy)]
pub struct Xform {
    linear: [[f64; 3]; 3],
    translation: [f64; 3],
}

impl Xform {
    /// Apply to a point.
    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let r = self.apply_dir(p);
        [
            r[0] + self.translation[0],
            r[1] + self.translation[1],
            r[2] + self.translation[2],
        ]
    }

    /// Apply the linear part only (directions).
    pub fn apply_dir(&self, v: [f64; 3]) -> [f64; 3] {
        let m = &self.linear;
        [
            m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
            m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
            m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
        ]
    }
}

impl From<&RigidPlacement> for Xform {
    fn from(p: &RigidPlacement) -> Self {
        Xform {
            linear: p.rotation,
            translation: p.translation,
        }
    }
}

impl From<&MirrorPlane> for Xform {
    /// `I − 2n̂n̂ᵀ` about the plane's own point: `p' = p − 2((p − q)·n̂) n̂`.
    /// Caller has checked the normal (see [`mirror_solid`]).
    fn from(plane: &MirrorPlane) -> Self {
        let n = plane.unit_normal().unwrap_or([0.0, 0.0, 1.0]);
        let mut linear = [[0.0; 3]; 3];
        for (i, row) in linear.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = if i == j { 1.0 } else { 0.0 } - 2.0 * n[i] * n[j];
            }
        }
        // t = 2(q·n̂) n̂.
        let d = plane.point[0] * n[0] + plane.point[1] * n[1] + plane.point[2] * n[2];
        Xform {
            linear,
            translation: [2.0 * d * n[0], 2.0 * d * n[1], 2.0 * d * n[2]],
        }
    }
}

fn map_point(p: &Xform, pt: Point3) -> Point3 {
    Point3::from(p.apply(pt.as_array()))
}

fn map_dir(p: &Xform, n: UnitVector3) -> UnitVector3 {
    let r = p.apply_dir([n.x, n.y, n.z]);
    UnitVector3 {
        x: r[0],
        y: r[1],
        z: r[2],
    }
}

/// Transform a surface descriptor. Every variant maps onto the same variant
/// with moved frame data; the `reversed` cavity sense is preserved because a
/// proper rotation preserves orientation.
pub fn map_surface(p: &Xform, s: &Surface) -> Surface {
    match *s {
        Surface::Plane(Plane { point, normal }) => Surface::Plane(Plane {
            point: map_point(p, point),
            normal: map_dir(p, normal),
        }),
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
            reversed,
        } => Surface::Cylinder {
            axis_point: map_point(p, axis_point),
            axis_dir: map_dir(p, axis_dir),
            radius,
            reversed,
        },
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
            reversed,
        } => Surface::Cone {
            apex: map_point(p, apex),
            axis_dir: map_dir(p, axis_dir),
            half_angle,
            reversed,
        },
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
            reversed,
        } => Surface::Torus {
            center: map_point(p, center),
            axis_dir: map_dir(p, axis_dir),
            major_radius,
            minor_radius,
            reversed,
        },
        Surface::Sphere {
            center,
            radius,
            reversed,
        } => Surface::Sphere {
            center: map_point(p, center),
            radius,
            reversed,
        },
    }
}

fn map_pair_surface(p: &Xform, s: &PairSurface) -> PairSurface {
    match *s {
        PairSurface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        } => PairSurface::Cylinder {
            axis_point: map_point(p, axis_point),
            axis_dir: map_dir(p, axis_dir),
            radius,
        },
        PairSurface::Cone {
            apex,
            axis_dir,
            half_angle,
        } => PairSurface::Cone {
            apex: map_point(p, apex),
            axis_dir: map_dir(p, axis_dir),
            half_angle,
        },
        PairSurface::Sphere { center, radius } => PairSurface::Sphere {
            center: map_point(p, center),
            radius,
        },
        PairSurface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        } => PairSurface::Torus {
            center: map_point(p, center),
            axis_dir: map_dir(p, axis_dir),
            major_radius,
            minor_radius,
        },
    }
}

/// Transform a curve descriptor. Directional normals rotate with the frame,
/// so a half-edge's counterclockwise sense is preserved.
pub fn map_curve(p: &Xform, c: &Curve) -> Curve {
    match *c {
        Curve::LineSegment => Curve::LineSegment,
        Curve::SurfacePair { ref a, ref b } => Curve::SurfacePair {
            a: map_pair_surface(p, a),
            b: map_pair_surface(p, b),
        },
        Curve::Circle {
            center,
            normal,
            radius,
        } => Curve::Circle {
            center: map_point(p, center),
            normal: map_dir(p, normal),
            radius,
        },
        Curve::Arc {
            center,
            normal,
            radius,
        } => Curve::Arc {
            center: map_point(p, center),
            normal: map_dir(p, normal),
            radius,
        },
        Curve::EllipseArc {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } => Curve::EllipseArc {
            center: map_point(p, center),
            normal: map_dir(p, normal),
            major_axis: map_dir(p, major_axis),
            major_radius,
            minor_radius,
        },
        Curve::HyperbolaArc {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } => Curve::HyperbolaArc {
            center: map_point(p, center),
            normal: map_dir(p, normal),
            major_axis: map_dir(p, major_axis),
            semi_transverse,
            semi_conjugate,
        },
    }
}

/// Deep-copy `solid` into the same arena with every point, surface and curve
/// moved by `placement`. Returns the new solid's id; the source is untouched.
///
/// Errors: [`KernelV2Error::TransformNotRigid`] for a non-rigid placement,
/// `InvalidId` for a dead source id, and any `validate_solid` failure on the
/// copy (which would indicate a source solid that was itself invalid).
pub fn transform_solid(
    arena: &mut BrepArena,
    solid: SolidId,
    placement: &RigidPlacement,
) -> Result<SolidId, KernelV2Error> {
    check_rigid(placement)?;
    copy_solid(arena, solid, &Xform::from(placement), Handedness::Kept)
}

/// Deep-copy `solid` into the same arena REFLECTED through `plane`. Returns
/// the new solid's id; the source is untouched.
///
/// The geometry maps exactly the same way a rigid copy's does — every
/// analytic surface and curve onto the same kind with reflected frame data —
/// but a reflection is improper, so every loop of the copy is traversed the
/// other way round ([`Handedness::Flipped`]). That is the whole difference:
/// without it every face of the copy would be inside out, which is why
/// [`transform_solid`] refuses a reflection rather than quietly doing half
/// the job.
///
/// Errors: a zero-length or non-finite plane normal, `InvalidId` for a dead
/// source id, and any `validate_solid` failure on the copy.
pub fn mirror_solid(
    arena: &mut BrepArena,
    solid: SolidId,
    plane: &MirrorPlane,
) -> Result<SolidId, KernelV2Error> {
    if plane.unit_normal().is_none() {
        return Err(KernelV2Error::TransformNotRigid {
            reason: "mirror plane normal is zero-length or non-finite",
        });
    }
    copy_solid(arena, solid, &Xform::from(plane), Handedness::Flipped)
}

/// Whether the copy preserves orientation (a rigid motion) or reverses it
/// (a reflection), which decides the loop traversal of every copied face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handedness {
    /// Proper: loops are copied as they are.
    Kept,
    /// Improper: every loop is traversed the other way (`next` ↔ `prev`, and
    /// each half-edge starts where it used to end), so the face's Newell
    /// normal still agrees with its mapped surface normal.
    Flipped,
}

fn copy_solid(
    arena: &mut BrepArena,
    solid: SolidId,
    placement: &Xform,
    handedness: Handedness,
) -> Result<SolidId, KernelV2Error> {
    let flip = handedness == Handedness::Flipped;

    // ── Pass 1: collect every reachable entity, keyed by source id ──────
    let src_solid = arena.solid(solid)?.clone();
    let mut shells: BTreeMap<ShellId, Shell> = BTreeMap::new();
    let mut faces: BTreeMap<FaceId, Face> = BTreeMap::new();
    let mut loops: BTreeMap<LoopId, Loop> = BTreeMap::new();
    let mut half_edges: BTreeMap<HalfEdgeId, HalfEdge> = BTreeMap::new();
    let mut vertices: BTreeMap<VertexId, Vertex> = BTreeMap::new();

    for &sh in &src_solid.shells {
        let shell = arena.shell(sh)?.clone();
        for &f in &shell.faces {
            let face = arena.face(f)?.clone();
            let mut lids = vec![face.outer_loop];
            lids.extend(face.inner_loops.iter().copied());
            for lid in lids {
                let lp = *arena.loop_(lid)?;
                match lp.boundary {
                    LoopBoundary::Lone(v) => {
                        vertices.insert(v, *arena.vertex(v)?);
                    }
                    LoopBoundary::Edges(_) => {
                        for h in arena.loop_half_edges(lid)? {
                            let he = *arena.half_edge(h)?;
                            vertices.insert(he.origin, *arena.vertex(he.origin)?);
                            half_edges.insert(h, he);
                        }
                    }
                }
                loops.insert(lid, lp);
            }
            faces.insert(f, face);
        }
        shells.insert(sh, shell);
    }

    // Every half-edge's twin must be in the collected set (2-manifold, closed).
    for he in half_edges.values() {
        if !half_edges.contains_key(&he.twin) {
            return Err(KernelV2Error::TwinPairingBroken { half_edge: he.twin });
        }
    }

    // ── Pass 2: allocate new slots in ascending source-id order ─────────
    // Ascending order preserves relative id order, so "canonical = lower-id
    // twin" holds on the copy exactly as it did on the source.
    let base_v = arena.vertices.len() as u32;
    let base_h = arena.half_edges.len() as u32;
    let base_l = arena.loops.len() as u32;
    let base_f = arena.faces.len() as u32;
    let base_s = arena.shells.len() as u32;
    let new_solid = SolidId(arena.solids.len() as u32);

    let vmap: BTreeMap<VertexId, VertexId> = vertices
        .keys()
        .enumerate()
        .map(|(i, &k)| (k, VertexId(base_v + i as u32)))
        .collect();
    let hmap: BTreeMap<HalfEdgeId, HalfEdgeId> = half_edges
        .keys()
        .enumerate()
        .map(|(i, &k)| (k, HalfEdgeId(base_h + i as u32)))
        .collect();
    let lmap: BTreeMap<LoopId, LoopId> = loops
        .keys()
        .enumerate()
        .map(|(i, &k)| (k, LoopId(base_l + i as u32)))
        .collect();
    let fmap: BTreeMap<FaceId, FaceId> = faces
        .keys()
        .enumerate()
        .map(|(i, &k)| (k, FaceId(base_f + i as u32)))
        .collect();
    let smap: BTreeMap<ShellId, ShellId> = shells
        .keys()
        .enumerate()
        .map(|(i, &k)| (k, ShellId(base_s + i as u32)))
        .collect();

    // ── Pass 3: emit the copy ───────────────────────────────────────────
    for v in vertices.values() {
        arena.vertices.push(Some(Vertex {
            point: map_point(placement, v.point),
        }));
    }
    for he in half_edges.values() {
        // A flipped copy walks each loop backwards, so this half-edge runs
        // from what used to be its destination (`next.origin`). The TWIN
        // pairing is untouched: both half-edges of an edge reverse together,
        // so they still traverse it oppositely.
        let (next, prev, origin) = if flip {
            (
                hmap[&he.prev],
                hmap[&he.next],
                vmap[&half_edges[&he.next].origin],
            )
        } else {
            (hmap[&he.next], hmap[&he.prev], vmap[&he.origin])
        };
        arena.half_edges.push(Some(HalfEdge {
            twin: hmap[&he.twin],
            next,
            prev,
            origin,
            loop_id: lmap[&he.loop_id],
            curve: map_curve(placement, &he.curve),
        }));
    }
    for lp in loops.values() {
        arena.loops.push(Some(Loop {
            face: fmap[&lp.face],
            boundary: match lp.boundary {
                LoopBoundary::Lone(v) => LoopBoundary::Lone(vmap[&v]),
                LoopBoundary::Edges(h) => LoopBoundary::Edges(hmap[&h]),
            },
            kind: lp.kind,
        }));
    }
    for face in faces.values() {
        arena.faces.push(Some(Face {
            surface: face.surface.as_ref().map(|s| map_surface(placement, s)),
            outer_loop: lmap[&face.outer_loop],
            inner_loops: face.inner_loops.iter().map(|l| lmap[l]).collect(),
            shell: smap[&face.shell],
        }));
    }
    for shell in shells.values() {
        arena.shells.push(Some(Shell {
            solid: new_solid,
            faces: shell.faces.iter().map(|f| fmap[f]).collect(),
            genus: shell.genus,
        }));
    }
    arena.solids.push(Some(Solid {
        shells: src_solid.shells.iter().map(|s| smap[s]).collect(),
    }));

    // ── Validate BEFORE stamping provenance (finalize_solid order) ──────
    validate_solid(arena, new_solid)?;

    // ── Provenance: fresh pids, lineage back to the source faces ────────
    let mut modified = Vec::new();
    let mut generated = Vec::new();
    for (&src, &dst) in &fmap {
        let pid = arena.alloc_pid();
        arena.face_pids.insert(dst, pid);
        match arena.face_pid(src) {
            Some(src_pid) => modified.push((src_pid, pid, EvoKind::Same)),
            None => generated.push(pid),
        }
    }
    arena.journal.push(Evolution {
        op: if flip {
            OpTag::Mirror
        } else {
            OpTag::Transform
        },
        generated,
        modified,
        deleted: Vec::new(),
    });

    Ok(new_solid)
}
