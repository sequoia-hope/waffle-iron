//! The geometric fingerprint of a face — `TopoSignature` — and the two
//! analytic-surface primitives it is built from.
//!
//! N0 of `specs/agent_mechanical_design.md` §5.1. Before N0 this lived in
//! `adapter.rs` and bailed on anything but `Surface::Plane`, returning
//! `TopoSignature::empty()` for every cylinder, cone, sphere and torus face.
//! An empty signature has NO field for `signature_similarity` to weigh, so it
//! scored 0.0 against every candidate: `Filter::SurfaceType { "cylindrical" }`
//! could never match a real body, `assign_boolean_roles` tied every curved
//! result face to operand A, and a reference carrying one resolved to
//! whichever face happened to be created first.
//!
//! ## What a curved face's fingerprint is made of
//!
//! - `surface_type` — the arena's surface vocabulary, the same strings
//!   `ImportedSurface::surface_type_str` uses so an arena face and an
//!   imported face are comparable (`planar`, `cylindrical`, `conical`,
//!   `spherical`, `toroidal`).
//! - `area` and `centroid` — the area and area-weighted centroid of the
//!   face's OWN render tessellation ([`crate::tessellate::tessellate_face`]),
//!   with the centroid then projected onto the analytic surface so it lies
//!   exactly ON the face's geometry. The tessellation is an inscribed
//!   partition, so a curved face's area sits BELOW the analytic value by the
//!   chord deficit (relative `O(chord_rel²)`, ≈5e-4 at the canonical band).
//!   That is honest for a fingerprint — a quantity compared against other
//!   fingerprints computed the same way — and it is the same convention
//!   `imported.rs` already uses for mesh-backed faces. It is NOT a
//!   measurement: `introspect::surface_area` is the exact door, and Q3 of the
//!   spec is where exact per-face integrals belong.
//! - `normal` — the OUTWARD unit normal AT the centroid
//!   ([`outward_normal_at`]), honouring the surface's cavity (`reversed`)
//!   sense. On a cylinder this is radial, not axial; `KernelIntrospect::
//!   entity_axis` remains the only door to a rotational surface's axis.
//!   **`None` for a face that goes all the way round its axis** — see below.
//! - `axis` — the rotation-invariant [`waffle_types::AxisDescriptor`] of every
//!   cylinder, cone, sphere and torus face: the canonicalised axis direction,
//!   the radii the surface holds, and how far the face reaches along the axis.
//!   Read straight off the analytic surface, so exact.
//!
//! ## Full-turn faces carry an axis, not a point normal
//!
//! A face that sweeps all the way round its axis has no distinguished point.
//! Its area-weighted centroid lies ON the axis, where the normal is undefined
//! and where the direction "from the axis to the centroid" is nothing but f64
//! summation rounding — **measured at 3.1e-17 m off a cylinder's axis**, so
//! changing that cylinder's height from 2 to 2.0001 swung the reported
//! centroid 58° round the axis and flipped the reported normal from
//! (−0.50, −0.87, 0) to (−1.00, −0.03, 0). A fingerprint exists to be stable,
//! so such a face reports `normal: None` and a centroid snapped exactly onto
//! the axis (which IS its area centroid, and pins its axial position), and the
//! `axis` descriptor carries the rest of its identity. Partial faces keep the
//! projected centroid and the normal there, both well defined.
//!
//! The gate is the distance from the mean to the surface's degeneracy locus
//! ([`distance_from_degeneracy`]) against `TAU_MODEL`, so it covers the full
//! turn and its genuine neighbourhood rather than a parametric special case.
//! A consumer that needs a point ON the face must therefore require `normal`
//! alongside `centroid`.
//! - `bbox` — over the tessellation's vertices for a curved face (which
//!   covers the bulge between boundary chords), over the loop points for a
//!   planar one.
//!
//! The planar arm is the PR-KV6a code moved verbatim: exact signed area
//! (`geom::planar_face_signed_area2`, arc corrections included), outer-loop
//! vertex mean as the centroid, loop-point bbox. Planar fingerprints are
//! bit-identical across N0.

use waffle_types::{AxisDescriptor, TopoSignature};

use crate::arena::{BrepArena, FaceId, PairSurface, Surface};
use cad_primitives::{Point3, TAU_MODEL};

/// The arena surface vocabulary, shared with `ImportedSurface::
/// surface_type_str` so arena and imported faces compare field-for-field.
pub fn surface_type_str(surface: &Surface) -> &'static str {
    match surface {
        Surface::Plane(_) => "planar",
        Surface::Cylinder { .. } => "cylindrical",
        Surface::Cone { .. } => "conical",
        Surface::Sphere { .. } => "spherical",
        Surface::Torus { .. } => "toroidal",
    }
}

/// `surface` as the equivalent [`PairSurface`] (the implicit form that
/// carries an exact signed distance and a unit gradient — see
/// [`crate::geom::pair_surface_residual_gradient`]). `None` for a plane,
/// which has no `PairSurface` counterpart and needs none.
fn pair_surface_of(surface: &Surface) -> Option<PairSurface> {
    match *surface {
        Surface::Plane(_) => None,
        Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
            ..
        } => Some(PairSurface::Cylinder {
            axis_point,
            axis_dir,
            radius,
        }),
        Surface::Cone {
            apex,
            axis_dir,
            half_angle,
            ..
        } => Some(PairSurface::Cone {
            apex,
            axis_dir,
            half_angle,
        }),
        Surface::Sphere { center, radius, .. } => Some(PairSurface::Sphere { center, radius }),
        Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
            ..
        } => Some(PairSurface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
        }),
    }
}

/// Whether `surface`'s stored outward sense is the cavity one (`reversed`):
/// the face's outward normal then points TOWARD the axis / centre / tube
/// circle, opposite the implicit gradient.
fn is_reversed(surface: &Surface) -> bool {
    match *surface {
        Surface::Plane(_) => false,
        Surface::Cylinder { reversed, .. }
        | Surface::Cone { reversed, .. }
        | Surface::Sphere { reversed, .. }
        | Surface::Torus { reversed, .. } => reversed,
    }
}

/// The signed distance from `p` to `surface`, POSITIVE on the side
/// [`outward_normal_at`] points to — so it is positive outside the material a
/// face of this surface bounds, for a cavity face as much as for a convex one.
///
/// This is the same implicit residual [`closest_point_on`] takes its Newton
/// step along, which for every arena surface is an exact signed distance
/// (cylinder `|r| − R`, sphere `|x − c| − R`, torus `|x − q| − r`, cone the
/// true distance to the nearest generator, plane the dot product) — so a
/// one-dimensional root find on it converges on the surface itself and not on
/// a scaled proxy of it.
///
/// Added for Q5 (`crate::measure::thickness`), which refines a ray's facet hit
/// onto the analytic surface by Newton along the ray: `g(t) = offset(ray(t))`,
/// `g'(t) = dir · outward_normal_at`. `None` where the gradient is undefined,
/// on the same degeneracies [`outward_normal_at`] refuses.
pub fn signed_offset_at(surface: &Surface, p: Point3) -> Option<f64> {
    match surface {
        Surface::Plane(plane) => {
            let d = [
                p.x() - plane.point.x(),
                p.y() - plane.point.y(),
                p.z() - plane.point.z(),
            ];
            let f = d[0] * plane.normal.x + d[1] * plane.normal.y + d[2] * plane.normal.z;
            f.is_finite().then_some(f)
        }
        other => {
            let pair = pair_surface_of(other)?;
            let (f, _) = crate::geom::pair_surface_residual_gradient(&pair, p.as_array())?;
            if !f.is_finite() {
                return None;
            }
            Some(if is_reversed(other) { -f } else { f })
        }
    }
}

/// The OUTWARD unit normal of `surface` at `p` — the implicit gradient,
/// negated for a cavity-sense (`reversed`) surface.
///
/// For every curved arena surface the implicit residual is an exact signed
/// distance whose gradient is already unit (cylinder: radial; cone:
/// `cosα·r̂ − sign(h)·sinα·â`; sphere: radial; torus: away from the tube
/// centre circle), so this is the true surface normal, not a difference
/// quotient. `None` where the normal is genuinely undefined: on a
/// cylinder's or cone's axis, at a sphere's centre, on a torus's axis or on
/// its tube centre circle.
pub fn outward_normal_at(surface: &Surface, p: Point3) -> Option<[f64; 3]> {
    match surface {
        Surface::Plane(plane) => Some([plane.normal.x, plane.normal.y, plane.normal.z]),
        other => {
            let pair = pair_surface_of(other)?;
            let (_, g) = crate::geom::pair_surface_residual_gradient(&pair, p.as_array())?;
            Some(if is_reversed(other) {
                [-g[0], -g[1], -g[2]]
            } else {
                g
            })
        }
    }
}

/// The axis of `surface` as `(a point on it, a unit direction)`, or `None` for
/// a plane and a sphere (neither has one).
fn axis_of(surface: &Surface) -> Option<(Point3, [f64; 3])> {
    match *surface {
        Surface::Plane(_) | Surface::Sphere { .. } => None,
        Surface::Cylinder {
            axis_point,
            axis_dir,
            ..
        } => Some((axis_point, [axis_dir.x, axis_dir.y, axis_dir.z])),
        Surface::Cone { apex, axis_dir, .. } => Some((apex, [axis_dir.x, axis_dir.y, axis_dir.z])),
        Surface::Torus {
            center, axis_dir, ..
        } => Some((center, [axis_dir.x, axis_dir.y, axis_dir.z])),
    }
}

/// `d` with its sign fixed by geometry rather than by construction order: the
/// first component whose magnitude clears `TAU_MODEL` is made positive. An
/// axis direction and its negative describe the SAME axis, and which one a
/// constructor stored is not a property of the face.
fn canonical_direction(d: [f64; 3]) -> [f64; 3] {
    for k in 0..3 {
        if d[k].abs() > TAU_MODEL {
            return if d[k] < 0.0 { [-d[0], -d[1], -d[2]] } else { d };
        }
    }
    d
}

/// How far `p` sits from the locus where this surface's closest-point
/// projection is undefined — a cylinder's or cone's axis, a sphere's centre, a
/// torus's axis — in meters. `None` for a plane, which has no such locus.
///
/// This is the quantity that decides whether a face's area-weighted mean can
/// be projected onto the surface at all. The full-turn case lands at f64
/// summation noise (3.1e-17 m measured on a cylinder), so `TAU_MODEL` — the
/// workspace's model-coincidence floor — separates "the mean is on the axis"
/// from "the mean is off the axis and the direction to it is real geometry".
/// A nearly-full patch sits well above the floor (a 359° cylinder band's mean
/// is ~1e-3·r off axis), so this gate is the full-turn condition and its
/// genuine neighbourhood, not a tuned band.
fn distance_from_degeneracy(surface: &Surface, p: Point3) -> Option<f64> {
    match *surface {
        Surface::Plane(_) => None,
        Surface::Sphere { center, .. } => Some(norm(sub(p.as_array(), center.as_array()))),
        _ => {
            let (a0, dir) = axis_of(surface)?;
            let v = sub(p.as_array(), a0.as_array());
            let along = v[0] * dir[0] + v[1] * dir[1] + v[2] * dir[2];
            Some(norm([
                v[0] - along * dir[0],
                v[1] - along * dir[1],
                v[2] - along * dir[2],
            ]))
        }
    }
}

/// `p` moved exactly onto the degeneracy locus — the foot on the axis, or a
/// sphere's centre. Used as the centroid of a full-turn face: that IS the area
/// centroid, and snapping removes the f64 transverse noise so the value is
/// reproducible to the last bit.
fn snap_to_degeneracy(surface: &Surface, p: Point3) -> Option<Point3> {
    match *surface {
        Surface::Plane(_) => None,
        Surface::Sphere { center, .. } => Some(center),
        _ => {
            let (a0, dir) = axis_of(surface)?;
            let v = sub(p.as_array(), a0.as_array());
            let along = v[0] * dir[0] + v[1] * dir[1] + v[2] * dir[2];
            Some(Point3::new(
                a0.x() + along * dir[0],
                a0.y() + along * dir[1],
                a0.z() + along * dir[2],
            ))
        }
    }
}

/// Write `mean` into `sig` as the centroid, with the normal there — or, when
/// the projection is degenerate, as the on-axis centroid with NO normal.
///
/// Two cases, and which one it is is readable from the result (a centroid with
/// a normal is on the face; one without is on the axis):
///
/// - `mean` is off the degeneracy locus by more than `TAU_MODEL`: the
///   direction from the axis to it is real geometry. Project onto the surface
///   — the mean of an inscribed partition sits just inside — and take the
///   outward normal there. This is every planar-trimmed patch and every
///   partial band.
/// - `mean` is ON the locus (a face that goes all the way round its axis, or
///   any band symmetric about it): there is no direction to project along, and
///   the one f64 rounding leaves is noise. Snap `mean` exactly onto the locus
///   and leave `normal` unset. The axis descriptor carries this face's
///   identity instead.
fn place_centroid(sig: &mut TopoSignature, surface: &Surface, mean: Point3) {
    let degenerate = distance_from_degeneracy(surface, mean).is_some_and(|d| d <= TAU_MODEL);
    if degenerate {
        if let Some(on_axis) = snap_to_degeneracy(surface, mean) {
            sig.centroid = Some(on_axis.as_array());
            sig.normal = None;
        }
        return;
    }
    if let Some(on) = closest_point_on(surface, mean) {
        sig.centroid = Some(on.as_array());
        sig.normal = outward_normal_at(surface, on);
    }
}

/// The rotation-invariant descriptor of `surface`, with `extent` measured over
/// `points` (the face's own tessellation or loop vertices). `None` for a plane.
fn axis_descriptor(surface: &Surface, points: &[[f64; 3]]) -> Option<AxisDescriptor> {
    if matches!(surface, Surface::Plane(_)) {
        return None;
    }
    let (radius, half_angle, minor_radius) = match *surface {
        Surface::Cylinder { radius, .. } => (Some(radius), None, None),
        Surface::Sphere { radius, .. } => (Some(radius), None, None),
        Surface::Cone { half_angle, .. } => (None, Some(half_angle), None),
        Surface::Torus {
            major_radius,
            minor_radius,
            ..
        } => (Some(major_radius), None, Some(minor_radius)),
        Surface::Plane(_) => unreachable!("returned above"),
    };
    let axis = axis_of(surface);
    let extent = axis.and_then(|(a0, dir)| {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for p in points {
            let v = sub(*p, a0.as_array());
            let t = v[0] * dir[0] + v[1] * dir[1] + v[2] * dir[2];
            lo = lo.min(t);
            hi = hi.max(t);
        }
        (hi >= lo).then_some(hi - lo)
    });
    Some(AxisDescriptor {
        direction: axis.map(|(_, d)| canonical_direction(d)),
        radius,
        half_angle,
        minor_radius,
        extent,
    })
}

/// The closest point to `p` on the UNBOUNDED surface `surface` (no trimming
/// — the caller owns the face's loops).
///
/// One step of `p − f·ĝ` is exact, not iterative: `f` is the true signed
/// distance to the surface and `ĝ` the unit gradient at `p`, for every form
/// [`crate::geom::pair_surface_residual_gradient`] carries (and trivially for
/// a plane). `None` where the gradient is undefined (same degeneracies as
/// [`outward_normal_at`]), and for a cone point whose foot would fall past
/// the apex onto the other nappe.
pub fn closest_point_on(surface: &Surface, p: Point3) -> Option<Point3> {
    match surface {
        Surface::Plane(plane) => {
            let n = [plane.normal.x, plane.normal.y, plane.normal.z];
            let d = [
                p.x() - plane.point.x(),
                p.y() - plane.point.y(),
                p.z() - plane.point.z(),
            ];
            let t = d[0] * n[0] + d[1] * n[1] + d[2] * n[2];
            Some(Point3::new(
                p.x() - t * n[0],
                p.y() - t * n[1],
                p.z() - t * n[2],
            ))
        }
        other => {
            let pair = pair_surface_of(other)?;
            let (f, g) = crate::geom::pair_surface_residual_gradient(&pair, p.as_array())?;
            if !f.is_finite() {
                return None;
            }
            let foot = Point3::new(p.x() - f * g[0], p.y() - f * g[1], p.z() - f * g[2]);
            // A cone's residual is the distance to the nearest generator of
            // the DOUBLE cone; `Surface::Cone` is the single `+axis_dir`
            // nappe, so refuse a foot that landed on the mirror nappe rather
            // than report a point the face's surface does not contain.
            if let Surface::Cone { apex, axis_dir, .. } = *other {
                let tau = (foot.x() - apex.x()) * axis_dir.x
                    + (foot.y() - apex.y()) * axis_dir.y
                    + (foot.z() - apex.z()) * axis_dir.z;
                if !tau.is_finite() || tau <= 0.0 {
                    return None;
                }
            }
            Some(foot)
        }
    }
}

/// The geometric fingerprint of `face`. Never panics; an unreadable face or
/// one still under construction (no surface yet) reports
/// [`TopoSignature::empty`].
pub fn face_signature(arena: &BrepArena, face: FaceId) -> TopoSignature {
    let Ok(f) = arena.face(face) else {
        return TopoSignature::empty();
    };
    let Some(surface) = f.surface else {
        return TopoSignature::empty();
    };
    match surface {
        Surface::Plane(_) => planar_signature(arena, face),
        _ => curved_signature(arena, face, &surface),
    }
}

/// The PR-KV6a planar fingerprint, unchanged: exact signed area (arc
/// corrections included — the chord Newell under-counts and SIGN-FLIPS
/// >180° annular sectors), outer-loop vertex mean, loop-point bbox.
fn planar_signature(arena: &BrepArena, fid: FaceId) -> TopoSignature {
    let Ok(face) = arena.face(fid) else {
        return TopoSignature::empty();
    };
    let Some(Surface::Plane(plane)) = face.surface else {
        return TopoSignature::empty();
    };
    let n = [plane.normal.x, plane.normal.y, plane.normal.z];

    let mut loops = vec![face.outer_loop];
    loops.extend(face.inner_loops.iter().copied());
    let twice_area = crate::geom::planar_face_signed_area2(arena, fid, face, n).unwrap_or(0.0);
    let mut bbox = empty_bbox();
    let mut centroid = [0.0f64; 3];
    let mut outer_count = 0usize;
    for (li, lid) in loops.iter().enumerate() {
        let Ok(pts) = arena.loop_points(*lid) else {
            continue;
        };
        for p in &pts {
            grow_bbox(&mut bbox, p.as_array());
            if li == 0 {
                let p = p.as_array();
                for k in 0..3 {
                    centroid[k] += p[k];
                }
                outer_count += 1;
            }
        }
    }
    if outer_count > 0 {
        for c in centroid.iter_mut() {
            *c /= outer_count as f64;
        }
    }
    TopoSignature {
        surface_type: Some("planar".to_string()),
        area: Some(twice_area / 2.0),
        centroid: Some(centroid),
        normal: Some(n),
        bbox: if outer_count > 0 { Some(bbox) } else { None },
        ..TopoSignature::empty()
    }
}

/// The fingerprint of a cylinder, cone, sphere or torus face (see the module
/// docs for what each field is and is not).
fn curved_signature(arena: &BrepArena, fid: FaceId, surface: &Surface) -> TopoSignature {
    let mut sig = TopoSignature {
        surface_type: Some(surface_type_str(surface).to_string()),
        ..TopoSignature::empty()
    };

    // The face's own triangles: an exact partition of the trimmed face up to
    // the chord band, so their area and area-weighted centroid are the
    // fingerprint's area and centroid.
    let mesh = crate::tessellate::tessellate_face(arena, fid).ok();
    if let Some(mesh) = &mesh {
        let mut bbox = empty_bbox();
        let mut verts: Vec<[f64; 3]> = Vec::with_capacity(mesh.positions.len() / 3);
        for p in mesh.positions.chunks_exact(3) {
            grow_bbox(&mut bbox, [p[0], p[1], p[2]]);
            verts.push([p[0], p[1], p[2]]);
        }
        if bbox[0].is_finite() {
            sig.bbox = Some(bbox);
        }
        let point = |i: u32| -> [f64; 3] {
            let i = i as usize * 3;
            [
                mesh.positions[i],
                mesh.positions[i + 1],
                mesh.positions[i + 2],
            ]
        };
        let mut area2_sum = 0.0f64;
        let mut moment = [0.0f64; 3];
        for t in mesh.indices.chunks_exact(3) {
            let (a, b, c) = (point(t[0]), point(t[1]), point(t[2]));
            let ab = sub(b, a);
            let ac = sub(c, a);
            let cr = cross(ab, ac);
            let area2 = norm(cr);
            area2_sum += area2;
            for k in 0..3 {
                moment[k] += area2 * (a[k] + b[k] + c[k]) / 3.0;
            }
        }
        sig.axis = axis_descriptor(surface, &verts);
        if area2_sum > 0.0 {
            sig.area = Some(area2_sum / 2.0);
            let mean = Point3::new(
                moment[0] / area2_sum,
                moment[1] / area2_sum,
                moment[2] / area2_sum,
            );
            place_centroid(&mut sig, surface, mean);
        }
    }

    // Tessellation refused (a KV5b/KV9 patch wall) or produced nothing: fall
    // back to the boundary loops, which every face has. Fewer fields, each
    // still computed — never a filled-in guess.
    if sig.centroid.is_none() {
        let mut bbox = empty_bbox();
        let mut mean = [0.0f64; 3];
        let mut count = 0usize;
        let mut pts: Vec<[f64; 3]> = Vec::new();
        let mut loops = Vec::new();
        if let Ok(f) = arena.face(fid) {
            loops.push(f.outer_loop);
            loops.extend(f.inner_loops.iter().copied());
        }
        for lid in loops {
            if let Ok(lp) = arena.loop_points(lid) {
                for p in &lp {
                    grow_bbox(&mut bbox, p.as_array());
                    let p = p.as_array();
                    pts.push(p);
                    for k in 0..3 {
                        mean[k] += p[k];
                    }
                    count += 1;
                }
            }
        }
        if count > 0 {
            if sig.bbox.is_none() {
                sig.bbox = Some(bbox);
            }
            if sig.axis.is_none() {
                sig.axis = axis_descriptor(surface, &pts);
            }
            let mean = Point3::new(
                mean[0] / count as f64,
                mean[1] / count as f64,
                mean[2] / count as f64,
            );
            place_centroid(&mut sig, surface, mean);
        }
    }

    sig
}

fn empty_bbox() -> [f64; 6] {
    [
        f64::INFINITY,
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ]
}

fn grow_bbox(bbox: &mut [f64; 6], p: [f64; 3]) {
    for k in 0..3 {
        bbox[k] = bbox[k].min(p[k]);
        bbox[k + 3] = bbox[k + 3].max(p[k]);
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::UnitVector3;
    use std::f64::consts::FRAC_PI_4;

    /// A cavity-sense surface's outward normal is the gradient NEGATED: the
    /// wall of a drilled hole faces the axis. (The fingerprint's whole point
    /// is telling a boss from a bore.)
    #[test]
    fn cavity_sense_normal_points_toward_the_axis() {
        let axis_dir = UnitVector3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        };
        let solid = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir,
            radius: 2.0,
            reversed: false,
        };
        let bore = Surface::Cylinder {
            axis_point: Point3::new(0.0, 0.0, 0.0),
            axis_dir,
            radius: 2.0,
            reversed: true,
        };
        let p = Point3::new(2.0, 0.0, 0.5);
        assert_eq!(outward_normal_at(&solid, p), Some([1.0, 0.0, 0.0]));
        assert_eq!(outward_normal_at(&bore, p), Some([-1.0, 0.0, 0.0]));
        // Undefined on the axis, loudly (None), never a default direction.
        assert_eq!(outward_normal_at(&solid, Point3::new(0.0, 0.0, 1.0)), None);
    }

    /// `closest_point_on` is a one-step exact projection for each implicit
    /// form, and refuses the cone's mirror nappe.
    #[test]
    fn closest_point_lands_on_the_surface() {
        let axis_dir = UnitVector3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        };
        let cyl = Surface::Cylinder {
            axis_point: Point3::new(1.0, 2.0, 3.0),
            axis_dir,
            radius: 0.5,
            reversed: false,
        };
        let on = closest_point_on(&cyl, Point3::new(4.0, 2.0, 7.0)).expect("cylinder foot");
        assert!((on.x() - 1.5).abs() < 1e-15 && (on.y() - 2.0).abs() < 1e-15);
        assert!((on.z() - 7.0).abs() < 1e-15, "the axial coordinate is kept");

        let sph = Surface::Sphere {
            center: Point3::new(0.0, 0.0, 0.0),
            radius: 3.0,
            reversed: false,
        };
        let on = closest_point_on(&sph, Point3::new(0.0, 0.0, -10.0)).expect("sphere foot");
        assert!((on.z() + 3.0).abs() < 1e-15, "{on:?}");

        let cone = Surface::Cone {
            apex: Point3::new(0.0, 0.0, 0.0),
            axis_dir,
            half_angle: FRAC_PI_4,
            reversed: false,
        };
        let on = closest_point_on(&cone, Point3::new(2.0, 0.0, 2.0)).expect("cone foot");
        assert!(
            (on.x() - 2.0).abs() < 1e-15 && (on.z() - 2.0).abs() < 1e-15,
            "a point already on the nappe is its own foot: {on:?}"
        );
        assert_eq!(
            closest_point_on(&cone, Point3::new(0.5, 0.0, -4.0)),
            None,
            "a foot on the mirror nappe is refused, not reported"
        );
    }
}
