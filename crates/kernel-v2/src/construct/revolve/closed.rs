//! Closed-surface revolve builders — KV6d: the circle-profile torus revolve
//! plus the closed-torus (T²) and on-axis closed-sphere assemblers.
//! Extracted verbatim from `revolve.rs` (move-only, F9); `build_torus_revolve`
//! is `pub(crate)` so `revolve` can dispatch to it, the `assemble_closed_*`
//! helpers stay private (called only from `build_torus_revolve`).

use super::*;

use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use crate::error::KernelV2Error;
use crate::profile::Profile;
use cad_primitives::{Point2, Point3, Vector3};

/// Revolve a circle [`Profile`] about an in-plane axis (KV6d).
///
/// Partial angles `∈ (0, 2π)` build a bent solid tube: topology mirrors
/// [`extrude_circle`] — 2 seam vertices, 6 half-edges, 3 faces — but the
/// caps are the profile disks at `θ = 0` and `θ = α` (meridian planes), the
/// rims are the profile circles (minor radius), the seams are longitude
/// ARCS (the φ = 0 latitude, radius `R + r`), and the lateral is a
/// [`Surface::Torus`].
///
/// The full turn (`full_turn`) builds the CLOSED ring torus (spec
/// `specs/kv6d_closed_torus_revolve.md`): the minimal CW structure of T²
/// (Stroud 2006 §3.1.4 seam representation) — 1 seam anchor vertex at the
/// outer equator, 2 closed seam circles (poloidal profile + toroidal outer
/// equator), 1 torus face whose outer loop is the aba⁻¹b⁻¹ square with
/// BOTH twin pairs internal. `V − E + F − R = 0 = 2(S − G)`, genus 1. The
/// on-axis circle (a sphere) is a typed wall; the off-center crossing
/// circle is invalid input, exactly like the partial branch.
///
/// Direct assembler; the safety obligation is discharged by
/// `validate_solid` at exit.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_torus_revolve(
    arena: &mut BrepArena,
    profile: &Profile,
    center2d: Point2,
    minor_radius: f64,
    axis_origin: Point3,
    axis_direction: Vector3,
    angle: f64,
    full_turn: bool,
) -> Result<RevolveResult, KernelV2Error> {
    // ---- frame + validation (ALL before the first mutation) ----------------
    let d = [axis_direction.x(), axis_direction.y(), axis_direction.z()];
    let d_sq = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if !d_sq.is_finite()
        || d_sq <= 0.0
        || !axis_origin.x().is_finite()
        || !axis_origin.y().is_finite()
        || !axis_origin.z().is_finite()
    {
        return Err(KernelV2Error::RevolveAxisNotInPlane);
    }
    let d_len = d_sq.sqrt();
    let a = UnitVector3 {
        x: d[0] / d_len,
        y: d[1] / d_len,
        z: d[2] / d_len,
    };
    let n = profile.unit_normal();
    // Axis must lie IN the profile plane (⊥ n, origin on the plane).
    if (a.x * n.x + a.y * n.y + a.z * n.z).abs() > REVOLVE_AXIS_IN_PLANE_TOLERANCE {
        return Err(KernelV2Error::RevolveAxisNotInPlane);
    }
    let o = profile.origin();
    let mag = axis_origin
        .x()
        .abs()
        .max(axis_origin.y().abs())
        .max(axis_origin.z().abs())
        .max(o.x().abs())
        .max(o.y().abs())
        .max(o.z().abs());
    let plane_dist = (axis_origin.x() - o.x()) * n.x
        + (axis_origin.y() - o.y()) * n.y
        + (axis_origin.z() - o.z()) * n.z;
    if plane_dist.abs() > REVOLVE_AXIS_IN_PLANE_TOLERANCE * (1.0 + mag) {
        return Err(KernelV2Error::RevolveAxisNotInPlane);
    }
    // Radial ŵ = ±normalize(n̂ × â), signed so the circle center is on the +ŵ
    // side; m̂ = â × ŵ is the sweep-velocity direction at θ = 0.
    let wx = [
        n.y * a.z - n.z * a.y,
        n.z * a.x - n.x * a.z,
        n.x * a.y - n.y * a.x,
    ];
    let w_len = (wx[0] * wx[0] + wx[1] * wx[1] + wx[2] * wx[2]).sqrt();
    let mut w = UnitVector3 {
        x: wx[0] / w_len,
        y: wx[1] / w_len,
        z: wx[2] / w_len,
    };
    let c3 = profile.embed(center2d);
    let radial = |p: Point3, w: &UnitVector3| {
        (p.x() - axis_origin.x()) * w.x
            + (p.y() - axis_origin.y()) * w.y
            + (p.z() - axis_origin.z()) * w.z
    };
    if radial(c3, &w) < 0.0 {
        w = neg(w);
    }
    let m = UnitVector3 {
        x: a.y * w.z - a.z * w.y,
        y: a.z * w.x - a.x * w.z,
        z: a.x * w.y - a.y * w.x,
    };
    let t_c = (c3.x() - axis_origin.x()) * a.x
        + (c3.y() - axis_origin.y()) * a.y
        + (c3.z() - axis_origin.z()) * a.z;
    let major = radial(c3, &w); // R: axis → tube center circle
    let r = minor_radius;
    // Ring torus: the circle's closest approach to the axis is R − r > 0.
    let clearance = REVOLVE_MIN_AXIS_CLEARANCE_REL * (1.0 + mag.max(major));
    // Full-turn on-axis circle sweeps a SPHERE (KV6d increment 2, spec
    // `kv6d_sphere_revolve.md`): the ball of radius `r` about the profile
    // center's axis projection (the sub-clearance off-axis component is
    // snapped away). Distinct from the off-center crossing (invalid input).
    if full_turn && major.abs() <= clearance {
        let center_sphere = Point3::new(
            axis_origin.x() + t_c * a.x,
            axis_origin.y() + t_c * a.y,
            axis_origin.z() + t_c * a.z,
        );
        return assemble_closed_sphere(arena, center_sphere, r);
    }
    if major - r <= clearance {
        return Err(KernelV2Error::RevolveAxisIntersectsProfile);
    }

    // ---- geometry ----------------------------------------------------------
    let center_torus = Point3::new(
        axis_origin.x() + t_c * a.x,
        axis_origin.y() + t_c * a.y,
        axis_origin.z() + t_c * a.z,
    );
    // Point at radial `cw` along ŵ + `cm` along m̂ from the tube-plane center.
    let lin = |cw: f64, cm: f64| {
        Point3::new(
            center_torus.x() + cw * w.x + cm * m.x,
            center_torus.y() + cw * w.y + cm * m.y,
            center_torus.z() + cw * w.z + cm * m.z,
        )
    };
    if full_turn {
        return assemble_closed_torus(arena, c3, center_torus, a, w, m, major, r);
    }
    let (ca, sa) = (snap_trig(angle.cos()), snap_trig(angle.sin()));
    let c_alpha = lin(major * ca, major * sa); // end-cap circle center
    let v0 = lin(major + r, 0.0); // seam anchor at θ=0, φ=0 (outer)
    let v_alpha = lin((major + r) * ca, (major + r) * sa); // at θ=α, φ=0
    let m_alpha = UnitVector3 {
        x: ca * m.x - sa * w.x,
        y: ca * m.y - sa * w.y,
        z: ca * m.z - sa * w.z,
    };
    let neg_m = neg(m);

    // ---- direct assembly (mirrors `extrude_circle`) ------------------------
    let vid0 = VertexId(arena.vertices.len() as u32);
    let vid1 = VertexId(vid0.0 + 1);
    arena.vertices.push(Some(Vertex { point: v0 }));
    arena.vertices.push(Some(Vertex { point: v_alpha }));

    let hb = arena.half_edges.len() as u32;
    let (cap_b, lat_b, seam_up, lat_t, cap_t, seam_dn) = (
        HalfEdgeId(hb),
        HalfEdgeId(hb + 1),
        HalfEdgeId(hb + 2),
        HalfEdgeId(hb + 3),
        HalfEdgeId(hb + 4),
        HalfEdgeId(hb + 5),
    );
    let lb = arena.loops.len() as u32;
    let (loop_base, loop_top, loop_lat) = (LoopId(lb), LoopId(lb + 1), LoopId(lb + 2));
    let fb = arena.faces.len() as u32;
    let (f_base, f_top, f_lat) = (FaceId(fb), FaceId(fb + 1), FaceId(fb + 2));
    let shell = ShellId(arena.shells.len() as u32);
    let solid = SolidId(arena.solids.len() as u32);

    let profile_rim = |center: Point3, normal: UnitVector3| Curve::Circle {
        center,
        normal,
        radius: r,
    };
    let seam_arc = |normal: UnitVector3| Curve::Arc {
        center: center_torus,
        normal,
        radius: major + r,
    };
    // Start cap boundary: profile circle CCW around the cap's outward −m̂.
    arena.half_edges.push(Some(HalfEdge {
        twin: lat_b,
        next: cap_b,
        prev: cap_b,
        origin: vid0,
        loop_id: loop_base,
        curve: profile_rim(c3, neg_m),
    }));
    // Lateral loop: start rim (toward +m̂), seam arc fwd, end rim, seam back.
    arena.half_edges.push(Some(HalfEdge {
        twin: cap_b,
        next: seam_up,
        prev: seam_dn,
        origin: vid0,
        loop_id: loop_lat,
        curve: profile_rim(c3, m),
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: seam_dn,
        next: lat_t,
        prev: lat_b,
        origin: vid0,
        loop_id: loop_lat,
        curve: seam_arc(a),
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: cap_t,
        next: seam_dn,
        prev: seam_up,
        origin: vid1,
        loop_id: loop_lat,
        curve: profile_rim(c_alpha, neg(m_alpha)),
    }));
    // End cap boundary: profile circle CCW around the cap's outward +m̂_α.
    arena.half_edges.push(Some(HalfEdge {
        twin: lat_t,
        next: cap_t,
        prev: cap_t,
        origin: vid1,
        loop_id: loop_top,
        curve: profile_rim(c_alpha, m_alpha),
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: seam_up,
        next: lat_b,
        prev: lat_t,
        origin: vid1,
        loop_id: loop_lat,
        curve: seam_arc(neg(a)),
    }));

    for (face, boundary) in [(f_base, cap_b), (f_top, cap_t), (f_lat, lat_b)] {
        arena.loops.push(Some(Loop {
            face,
            boundary: LoopBoundary::Edges(boundary),
            kind: LoopKind::Outer,
        }));
    }
    arena.faces.push(Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: c3,
            normal: neg_m,
        })),
        outer_loop: loop_base,
        inner_loops: Vec::new(),
        shell,
    }));
    arena.faces.push(Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: c_alpha,
            normal: m_alpha,
        })),
        outer_loop: loop_top,
        inner_loops: Vec::new(),
        shell,
    }));
    arena.faces.push(Some(Face {
        surface: Some(Surface::Torus {
            center: center_torus,
            axis_dir: a,
            major_radius: major,
            minor_radius: r,
            reversed: false,
        }),
        outer_loop: loop_lat,
        inner_loops: Vec::new(),
        shell,
    }));
    arena.shells.push(Some(Shell {
        solid,
        faces: vec![f_base, f_top, f_lat],
        genus: 0,
    }));
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));

    finalize_solid(arena, solid)?;
    Ok(RevolveResult {
        solid,
        shell,
        start_cap: Some(f_base),
        end_cap: Some(f_top),
        walls: vec![f_lat],
    })
}

/// Assemble the CLOSED ring torus (KV6d full turn, spec
/// `specs/kv6d_closed_torus_revolve.md`): the minimal CW structure of T².
///
/// - 1 vertex: the seam anchor `v0` at the outer equator (θ = 0, φ = 0).
/// - 2 closed edges through `v0`: the poloidal PROFILE circle at θ = 0
///   (radius `r`, center `c3`) and the toroidal OUTER-EQUATOR circle
///   (radius `R + r`, center `center_torus`).
/// - 1 face: the torus, outer loop = `[prof_fwd, eq_fwd, prof_back,
///   eq_back]` — the aba⁻¹b⁻¹ square of the cut torus; BOTH twin pairs are
///   internal to the loop (the partial-tube seam twin pair precedent,
///   closed in the second direction too).
///
/// Directional normals follow the partial-tube conventions continuously:
/// the θ = 0 rim traversal carries `+m̂` (the sweep velocity — the θ = 2π
/// return carries `−m̂`), the equator pair carries `±â` exactly like the
/// longitude seam arcs. Euler–Poincaré: `1 − 2 + 1 − 0 = 0 = 2(S − G)`,
/// genus 1.
#[allow(clippy::too_many_arguments)]
fn assemble_closed_torus(
    arena: &mut BrepArena,
    c3: Point3,
    center_torus: Point3,
    a: UnitVector3,
    w: UnitVector3,
    m: UnitVector3,
    major: f64,
    r: f64,
) -> Result<RevolveResult, KernelV2Error> {
    let v0 = Point3::new(
        center_torus.x() + (major + r) * w.x,
        center_torus.y() + (major + r) * w.y,
        center_torus.z() + (major + r) * w.z,
    );
    let neg_m = neg(m);

    let vid0 = VertexId(arena.vertices.len() as u32);
    arena.vertices.push(Some(Vertex { point: v0 }));

    let hb = arena.half_edges.len() as u32;
    let (prof_fwd, eq_fwd, prof_back, eq_back) = (
        HalfEdgeId(hb),
        HalfEdgeId(hb + 1),
        HalfEdgeId(hb + 2),
        HalfEdgeId(hb + 3),
    );
    let loop_lat = LoopId(arena.loops.len() as u32);
    let f_lat = FaceId(arena.faces.len() as u32);
    let shell = ShellId(arena.shells.len() as u32);
    let solid = SolidId(arena.solids.len() as u32);

    let profile_rim = |normal: UnitVector3| Curve::Circle {
        center: c3,
        normal,
        radius: r,
    };
    let equator_rim = |normal: UnitVector3| Curve::Circle {
        center: center_torus,
        normal,
        radius: major + r,
    };
    // Loop cycle prof_fwd → eq_fwd → prof_back → eq_back (aba⁻¹b⁻¹).
    arena.half_edges.push(Some(HalfEdge {
        twin: prof_back,
        next: eq_fwd,
        prev: eq_back,
        origin: vid0,
        loop_id: loop_lat,
        curve: profile_rim(m),
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: eq_back,
        next: prof_back,
        prev: prof_fwd,
        origin: vid0,
        loop_id: loop_lat,
        curve: equator_rim(a),
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: prof_fwd,
        next: eq_back,
        prev: eq_fwd,
        origin: vid0,
        loop_id: loop_lat,
        curve: profile_rim(neg_m),
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: eq_fwd,
        next: prof_fwd,
        prev: prof_back,
        origin: vid0,
        loop_id: loop_lat,
        curve: equator_rim(neg(a)),
    }));

    arena.loops.push(Some(Loop {
        face: f_lat,
        boundary: LoopBoundary::Edges(prof_fwd),
        kind: LoopKind::Outer,
    }));
    arena.faces.push(Some(Face {
        surface: Some(Surface::Torus {
            center: center_torus,
            axis_dir: a,
            major_radius: major,
            minor_radius: r,
            reversed: false,
        }),
        outer_loop: loop_lat,
        inner_loops: Vec::new(),
        shell,
    }));
    arena.shells.push(Some(Shell {
        solid,
        faces: vec![f_lat],
        genus: 1,
    }));
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));

    finalize_solid(arena, solid)?;
    Ok(RevolveResult {
        solid,
        shell,
        start_cap: None,
        end_cap: None,
        walls: vec![f_lat],
    })
}

/// Assemble the CLOSED solid sphere (KV6d increment 2, spec
/// `kv6d_sphere_revolve.md`): the full-turn revolve of an on-axis circle.
///
/// Minimal seam structure of S² (the PR-YR12 yang contract mirrored into
/// the arena): V = 2 (south/north poles), E = 1 (a meridian seam Arc twin
/// pair), F = 1 (`Surface::Sphere`), genus 0 —
/// `V − E + F − R = 2 = 2(S − G)` ✓.
///
/// The seam frame is CANONICAL world-z-up regardless of the revolve axis
/// (the sphere is isotropic; the fixed frame matches yang's fixed z-up
/// lat/long parameterization so `to_yang_brep` is a direct emission):
/// poles at `center ± r·ẑ`, seam on the X–Z great circle through
/// `center + r·x̂`. The forward (south → north) half-edge sweeps CCW
/// around `−ŷ` (tangent `+x̂` at the south pole — through the `+x̂`
/// meridian); its twin carries the negated normal (the existing
/// curve-twin sign-canonicalized consistency rule).
fn assemble_closed_sphere(
    arena: &mut BrepArena,
    center: Point3,
    r: f64,
) -> Result<RevolveResult, KernelV2Error> {
    let v_south = Point3::new(center.x(), center.y(), center.z() - r);
    let v_north = Point3::new(center.x(), center.y(), center.z() + r);

    let vid_s = VertexId(arena.vertices.len() as u32);
    let vid_n = VertexId(vid_s.0 + 1);
    arena.vertices.push(Some(Vertex { point: v_south }));
    arena.vertices.push(Some(Vertex { point: v_north }));

    let hb = arena.half_edges.len() as u32;
    let (seam_fwd, seam_back) = (HalfEdgeId(hb), HalfEdgeId(hb + 1));
    let loop_sph = LoopId(arena.loops.len() as u32);
    let f_sph = FaceId(arena.faces.len() as u32);
    let shell = ShellId(arena.shells.len() as u32);
    let solid = SolidId(arena.solids.len() as u32);

    let meridian = |normal: UnitVector3| Curve::Arc {
        center,
        normal,
        radius: r,
    };
    let neg_y = UnitVector3 {
        x: 0.0,
        y: -1.0,
        z: 0.0,
    };
    // Loop cycle seam_fwd → seam_back (the twin pair internal to one loop —
    // closed-torus precedent).
    arena.half_edges.push(Some(HalfEdge {
        twin: seam_back,
        next: seam_back,
        prev: seam_back,
        origin: vid_s,
        loop_id: loop_sph,
        curve: meridian(neg_y),
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: seam_fwd,
        next: seam_fwd,
        prev: seam_fwd,
        origin: vid_n,
        loop_id: loop_sph,
        curve: meridian(neg(neg_y)),
    }));

    arena.loops.push(Some(Loop {
        face: f_sph,
        boundary: LoopBoundary::Edges(seam_fwd),
        kind: LoopKind::Outer,
    }));
    arena.faces.push(Some(Face {
        surface: Some(Surface::Sphere {
            center,
            radius: r,
            reversed: false,
        }),
        outer_loop: loop_sph,
        inner_loops: Vec::new(),
        shell,
    }));
    arena.shells.push(Some(Shell {
        solid,
        faces: vec![f_sph],
        genus: 0,
    }));
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));

    finalize_solid(arena, solid)?;
    Ok(RevolveResult {
        solid,
        shell,
        start_cap: None,
        end_cap: None,
        walls: vec![f_sph],
    })
}
