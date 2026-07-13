//! On-axis (solid-of-revolution / lathe) revolve builders — KV6 slice 1/2:
//! the cylinder-delegating, frustum, wedge, and apex-cone assemblers for a
//! full-turn revolve of a profile with exactly one on-axis edge. Extracted
//! verbatim from `revolve.rs` (move-only, F9); `on_axis_revolve` is
//! `pub(crate)` so `revolve` can dispatch to it, the frustum/wedge/apex-cone
//! sub-builders stay private (called only from `on_axis_revolve`).

use super::*;

use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use crate::error::KernelV2Error;
use crate::profile::{Profile, ProfileRegion};
use cad_primitives::{Point2, Point3, Vector3};

/// KV6 on-axis recovery (slice 1 `specs/kv6_on_axis_revolve_rectangle.md`,
/// task #65; slice 2 `specs/kv6_on_axis_revolve_oblique.md`, task #66):
/// full-turn revolve of a 3/4-gon with exactly ONE on-axis edge — the
/// lathe family. The on-axis edge sweeps the rotation's fixed line
/// (degenerate, interior to the solid); perpendicular edges sweep full
/// DISCS; the remaining edges sweep the lateral:
///
/// - 4-gon, axis-PARALLEL off-axis edge (slice 1): EXACTLY the canonical
///   KV5a cylinder — DELEGATES to `extrude` of a synthesized cap-plane
///   circle profile, bit-canonical with extrude-of-circle, no new
///   topology code.
/// - 4-gon, OBLIQUE off-axis edge (slice 2A, the C0064 class): the SOLID
///   FRUSTUM — same census with a `Surface::Cone` lateral, built by
///   [`build_on_axis_frustum`].
/// - 3-gon, one perpendicular cap + one oblique edge reaching the axis
///   (slice 2B, the C0063 primary): the SOLID CONE, built by
///   [`build_on_axis_apex_cone`] — the apex an interior singular point of
///   the lateral.
///
/// Reached only where `validate_revolve_geometry` returned
/// `RevolveAxisIntersectsProfile` (axis finite + in-plane and the region a
/// hole-free polygon are already verified — the clearance check is the
/// LAST rejection in that function). Everything that is not a slice-1/2
/// shape — crossing profiles, bicones, pencil quads, degenerate
/// rectangles — returns the ORIGINAL typed error, pre-mutation.
pub(crate) fn on_axis_revolve(
    arena: &mut BrepArena,
    profile: &Profile,
    axis_origin: Point3,
    axis_direction: Vector3,
    angle_rad: f64,
) -> Result<RevolveResult, KernelV2Error> {
    const REJECT: KernelV2Error = KernelV2Error::RevolveAxisIntersectsProfile;
    // Slice-3 dispatch: the caller validated `angle_rad ∈ (0, 2π]`; the
    // full-turn band is the same constant `revolve` classified with.
    let full_turn = (angle_rad - 2.0 * std::f64::consts::PI).abs() <= REVOLVE_FULL_TURN_TOLERANCE;
    // Axis frame — the same formulas and constants as
    // `validate_revolve_geometry` (â verified finite/in-plane there).
    let d = [axis_direction.x(), axis_direction.y(), axis_direction.z()];
    let d_len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let a = UnitVector3 {
        x: d[0] / d_len,
        y: d[1] / d_len,
        z: d[2] / d_len,
    };
    let n = profile.unit_normal();
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
    let outer = match profile.region() {
        ProfileRegion::Polygon { outer, holes } if holes.is_empty() => outer,
        _ => return Err(REJECT),
    };
    let embedded: Vec<Point3> = outer.iter().map(|&p| profile.embed(p)).collect();
    let radial = |p: &Point3, w: &UnitVector3| {
        (p.x() - axis_origin.x()) * w.x
            + (p.y() - axis_origin.y()) * w.y
            + (p.z() - axis_origin.z()) * w.z
    };
    // Same radial sign rule as the validator: ŵ points to the material side.
    if embedded.iter().map(|p| radial(p, &w)).sum::<f64>() < 0.0 {
        w = UnitVector3 {
            x: -w.x,
            y: -w.y,
            z: -w.z,
        };
    }
    let mut t = Vec::with_capacity(embedded.len());
    let mut s = Vec::with_capacity(embedded.len());
    let mut scale = 0.0f64;
    for p in &embedded {
        let dx = [
            p.x() - axis_origin.x(),
            p.y() - axis_origin.y(),
            p.z() - axis_origin.z(),
        ];
        t.push(dx[0] * a.x + dx[1] * a.y + dx[2] * a.z);
        s.push(radial(p, &w));
        scale = scale.max(dx[0].abs()).max(dx[1].abs()).max(dx[2].abs());
    }
    let clearance = REVOLVE_MIN_AXIS_CLEARANCE_REL * (1.0 + scale);
    // No explicit crossing branch: the ŵ sign rule normalizes an
    // all-negative profile to positive, and a genuinely mixed-sign
    // (crossing) profile cannot present 2 on-axis + 2 equal-radius
    // vertices below, so every crossing input falls out of the shape
    // gates with the same typed error (branch-minimal per Constitution §7;
    // `full_turn_crossing_profile_stays_rejected` pins it).

    // ---- shared shape gates: 3/4-gon with exactly one on-axis edge --------
    let k = embedded.len();
    if k != 3 && k != 4 {
        return Err(REJECT);
    }
    let on = |i: usize| s[i].abs() <= clearance;
    let on_idx: Vec<usize> = (0..k).filter(|&i| on(i)).collect();
    if on_idx.len() != 2 {
        return Err(REJECT);
    }
    let adjacent = on_idx[1] == on_idx[0] + 1 || (on_idx[0] == 0 && on_idx[1] == k - 1);
    if !adjacent {
        return Err(REJECT);
    }
    let off_idx: Vec<usize> = (0..k).filter(|&i| !on(i)).collect();
    let band = REVOLVE_EDGE_ALIGNMENT_TOLERANCE * (1.0 + scale);

    // ---- slice 2 increment B: apex triangle → solid cone ------------------
    // The single off-axis vertex has one connector edge to each on-axis
    // vertex: exactly ONE must be axis-perpendicular (it sweeps the disc
    // cap); the other is oblique and its on-axis endpoint is the APEX.
    // Both perpendicular is geometrically impossible (the on-axis vertices
    // are axially distinct simple-polygon vertices); both oblique is the
    // BICONE, outside the slice (typed, pre-mutation).
    if k == 3 {
        // The partial-angle apex-triangle wedge (a cone wedge whose apex
        // sits ON the boundary between the two pie sectors) is a distinct
        // vocabulary with no corpus driver — typed (slice-3 spec §2).
        if !full_turn {
            return Err(REJECT);
        }
        let c = off_idx[0];
        let radius = s[c];
        let perp: Vec<usize> = on_idx
            .iter()
            .copied()
            .filter(|&i| (t[i] - t[c]).abs() <= band)
            .collect();
        if perp.len() != 1 || radius <= clearance {
            return Err(REJECT); // bicone or degenerate — never a silent sliver
        }
        let cap_i = perp[0];
        let apex_i = if on_idx[0] == cap_i {
            on_idx[1]
        } else {
            on_idx[0]
        };
        if (t[apex_i] - t[cap_i]).abs() <= band {
            return Err(REJECT); // flat triangle — degenerate
        }
        return build_on_axis_apex_cone(arena, a, w, axis_origin, t[cap_i], t[apex_i], radius);
    }

    // ---- slice-1/2A shapes: the 4-gon lathe family -------------------------
    // Each cap edge must be axis-perpendicular (on-axis vertex and its
    // off-axis ring neighbor at the same axial coordinate) — an oblique
    // CAP edge would sweep an apex cone glued to a lateral (the "pencil"),
    // outside the slice-2 vocabulary.
    for &i in &on_idx {
        for j in [(i + 3) % 4, (i + 1) % 4] {
            if !on(j) && (t[i] - t[j]).abs() > band {
                return Err(REJECT);
            }
        }
    }

    // Off-axis pair ordered axially. The radii/height positivity gate is
    // defense-in-depth (slice-1 precedent): a mixed-sign quad that passes
    // the perpendicular-cap gates always self-intersects the on-axis edge
    // and is rejected upstream as ProfileNotSimple (pinned by
    // `crossing_oblique_quad_rejected_upstream_not_simple`), and an
    // all-negative profile is normalized positive by the ŵ sign rule.
    let (o0, o1) = (off_idx[0], off_idx[1]);
    let (bot, top) = if t[o0] <= t[o1] { (o0, o1) } else { (o1, o0) };
    let (r_bot, r_top) = (s[bot], s[top]);
    let height = t[top] - t[bot];
    if r_bot <= clearance || r_top <= clearance || height <= band {
        return Err(REJECT); // crossing or degenerate — never a silent sliver
    }

    // ---- slice 3: PARTIAL angle → the wedge (cylinder or frustum wall) ----
    if !full_turn {
        return build_on_axis_wedge(
            arena,
            a,
            w,
            axis_origin,
            t[bot],
            t[top],
            r_bot,
            r_top,
            band,
            angle_rad,
        );
    }

    // ---- slice 2 increment A: oblique off-axis edge → solid frustum -------
    if (r_bot - r_top).abs() > band {
        return build_on_axis_frustum(arena, a, w, axis_origin, t[bot], t[top], r_bot, r_top);
    }

    // ---- slice 1: canonical cylinder via the extrude-of-circle path -------
    let radius = r_bot;
    // Cap-plane frame: (ŵ, m̂ = â × ŵ) spans the plane ⊥ â with
    // ŵ × m̂ = â, so the synthesized profile's normal IS the axis and the
    // extrude direction is exactly normal (cosine 1).
    let m = UnitVector3 {
        x: a.y * w.z - a.z * w.y,
        y: a.z * w.x - a.x * w.z,
        z: a.x * w.y - a.y * w.x,
    };
    let origin = Point3::new(
        axis_origin.x() + t[bot] * a.x,
        axis_origin.y() + t[bot] * a.y,
        axis_origin.z() + t[bot] * a.z,
    );
    let circle = Profile::circle(
        origin,
        Vector3::new(w.x, w.y, w.z),
        Vector3::new(m.x, m.y, m.z),
        Point2::new(0.0, 0.0),
        radius,
    )?;
    let ex = extrude(arena, &circle, Vector3::new(a.x, a.y, a.z), height)?;
    // I3: same 360° result convention as the washer branch — start cap at
    // the axial minimum (outward −â), end cap at the maximum (+â).
    Ok(RevolveResult {
        solid: ex.solid,
        shell: ex.shell,
        start_cap: Some(ex.base),
        end_cap: Some(ex.top),
        walls: ex.walls,
    })
}

/// KV6 on-axis slice 2 increment A (spec
/// `specs/kv6_on_axis_revolve_oblique.md`, task #66): direct assembler for
/// the SOLID FRUSTUM swept full-turn by an on-axis 4-gon whose off-axis
/// edge is oblique (the C0064 class). Mirrors [`extrude_circle`] verbatim —
/// same id layout, same Stroud §3.1.4 single-fake-edge topology (2 seam
/// vertices, 2 vertex-anchored closed rims + 1 seam ruling, 3 faces), same
/// rim-curve orientation conventions — with per-rim radii and the analytic
/// [`Surface::Cone`] lateral. The cone parameters come from the slant with
/// the SAME formulas as [`EdgeClass::Oblique`] classification in
/// [`validate_revolve_geometry`]: extended to `s = 0` the slant meets the
/// axis at `t_apex = t_bot − r_bot·Δt/Δr`; `axis_dir` is oriented so both
/// rims sit at τ > 0 (apex behind); `half_angle = atan|Δr/Δt|`. Safety
/// obligation discharged by `finalize_solid` at exit (the extrude-circle
/// precedent for closed curved edges outside the Euler-operator
/// vocabulary).
///
/// Caller guarantees: `t_top − t_bot` and `|r_top − r_bot|` above the
/// alignment band, both radii strictly positive.
#[allow(clippy::too_many_arguments)]
fn build_on_axis_frustum(
    arena: &mut BrepArena,
    a: UnitVector3,
    w: UnitVector3,
    a0: Point3,
    t_bot: f64,
    t_top: f64,
    r_bot: f64,
    r_top: f64,
) -> Result<RevolveResult, KernelV2Error> {
    let neg_a = neg(a);
    let at = |t: f64| Point3::new(a0.x() + t * a.x, a0.y() + t * a.y, a0.z() + t * a.z);
    let (c0, c1) = (at(t_bot), at(t_top));
    // Seam anchors: radially along ŵ (θ = 0 of the sweep) — the same seam
    // azimuth the slice-1 delegation produces via `Profile::circle`'s u
    // basis.
    let on_rim = |c: Point3, r: f64| Point3::new(c.x() + r * w.x, c.y() + r * w.y, c.z() + r * w.z);
    let (v0, v1) = (on_rim(c0, r_bot), on_rim(c1, r_top));

    // Cone surface from the slant (EdgeClass::Oblique formulas; Δt > 0 by
    // the caller's bot/top ordering, so `axis_dir` follows sign(Δr)).
    let dt = t_top - t_bot;
    let ds = r_top - r_bot;
    let apex = at(t_bot - r_bot * dt / ds);
    let axis_dir = if ds > 0.0 { a } else { neg_a };
    let half_angle = (ds / dt).abs().atan();

    // ---- direct assembly (extrude_circle id layout) ------------------------
    let vid0 = VertexId(arena.vertices.len() as u32);
    let vid1 = VertexId(vid0.0 + 1);
    arena.vertices.push(Some(Vertex { point: v0 }));
    arena.vertices.push(Some(Vertex { point: v1 }));

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

    let rim = |center: Point3, normal: UnitVector3, radius: f64| Curve::Circle {
        center,
        normal,
        radius,
    };
    // Base cap boundary: one closed circle half-edge, CCW around the cap's
    // outward normal −a.
    arena.half_edges.push(Some(HalfEdge {
        twin: lat_b,
        next: cap_b,
        prev: cap_b,
        origin: vid0,
        loop_id: loop_base,
        curve: rim(c0, neg_a, r_bot),
    }));
    // Lateral loop: bottom rim (CCW around +a — toward the top rim), seam
    // up, top rim (CCW around −a — toward the bottom rim), seam down.
    arena.half_edges.push(Some(HalfEdge {
        twin: cap_b,
        next: seam_up,
        prev: seam_dn,
        origin: vid0,
        loop_id: loop_lat,
        curve: rim(c0, a, r_bot),
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: seam_dn,
        next: lat_t,
        prev: lat_b,
        origin: vid0,
        loop_id: loop_lat,
        curve: Curve::LineSegment,
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: cap_t,
        next: seam_dn,
        prev: seam_up,
        origin: vid1,
        loop_id: loop_lat,
        curve: rim(c1, neg_a, r_top),
    }));
    // Top cap boundary: CCW around the cap's outward normal +a.
    arena.half_edges.push(Some(HalfEdge {
        twin: lat_t,
        next: cap_t,
        prev: cap_t,
        origin: vid1,
        loop_id: loop_top,
        curve: rim(c1, a, r_top),
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: seam_up,
        next: lat_b,
        prev: lat_t,
        origin: vid1,
        loop_id: loop_lat,
        curve: Curve::LineSegment,
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
            point: c0,
            normal: neg_a,
        })),
        outer_loop: loop_base,
        inner_loops: Vec::new(),
        shell,
    }));
    arena.faces.push(Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: c1,
            normal: a,
        })),
        outer_loop: loop_top,
        inner_loops: Vec::new(),
        shell,
    }));
    arena.faces.push(Some(Face {
        surface: Some(Surface::Cone {
            apex,
            axis_dir,
            half_angle,
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

    // ---- full production validation (defense in depth) --------------------
    finalize_solid(arena, solid)?;
    // I3/I6: start cap at the axial minimum (outward −â), end cap at the
    // maximum (+â) — same 360° convention as the washer branch.
    Ok(RevolveResult {
        solid,
        shell,
        start_cap: Some(f_base),
        end_cap: Some(f_top),
        walls: vec![f_lat],
    })
}

/// KV6 on-axis slice 3 (spec `specs/kv6_on_axis_revolve_partial_wedge.md`,
/// task #85): direct assembler for the WEDGE — the single-on-axis-edge
/// lathe 4-gon swept a PARTIAL angle. The two on-axis vertices are fixed
/// by the rotation, so the θ=0 and θ=α caps SHARE the on-axis edge
/// directly (the swept face of the on-axis edge is degenerate and is not
/// emitted). Census: V=6, E=9 (7 cap segments + 2 sweep arcs), F=5
/// (2 caps + 2 planar pie sectors + 1 curved wall), χ=2. Every face is
/// existing vocabulary — planar-with-arcs sectors, the KV5b/KV6c-5
/// [seg, arc, seg, arc] wall — only the construction is new.
///
/// Vertex/loop conventions mirror [`build_partial_revolve`]: the start cap's
/// outward normal opposes the sweep velocity (−m̂), the end cap carries the
/// rotated +m̂; arcs carry axis-directional traversal normals; trig snapped
/// at the quadrant angles ([`snap_trig`]).
///
/// Caller guarantees: `0 < angle < 2π` (not the full-turn band), radii
/// strictly positive, `t_top − t_bot` above the band; equal radii (within
/// `band`) → cylinder wall, else the frustum-wedge `Surface::Cone` from the
/// slice-2A slant formulas.
#[allow(clippy::too_many_arguments)]
fn build_on_axis_wedge(
    arena: &mut BrepArena,
    a: UnitVector3,
    w: UnitVector3,
    a0: Point3,
    t_bot: f64,
    t_top: f64,
    r_bot: f64,
    r_top: f64,
    band: f64,
    angle: f64,
) -> Result<RevolveResult, KernelV2Error> {
    let neg = |u: UnitVector3| UnitVector3 {
        x: -u.x,
        y: -u.y,
        z: -u.z,
    };
    // m̂ = â × ŵ — the sweep-velocity direction at θ = 0.
    let m = UnitVector3 {
        x: a.y * w.z - a.z * w.y,
        y: a.z * w.x - a.x * w.z,
        z: a.x * w.y - a.y * w.x,
    };
    let (ca, sa) = (snap_trig(angle.cos()), snap_trig(angle.sin()));
    // Rotated radial ŵ_α and the end cap's outward normal m̂_α.
    let w_a = UnitVector3 {
        x: ca * w.x + sa * m.x,
        y: ca * w.y + sa * m.y,
        z: ca * w.z + sa * m.z,
    };
    let m_a = UnitVector3 {
        x: ca * m.x - sa * w.x,
        y: ca * m.y - sa * w.y,
        z: ca * m.z - sa * w.z,
    };
    let at = |t: f64| Point3::new(a0.x() + t * a.x, a0.y() + t * a.y, a0.z() + t * a.z);
    let radial = |c: Point3, u: UnitVector3, r: f64| {
        Point3::new(c.x() + r * u.x, c.y() + r * u.y, c.z() + r * u.z)
    };
    // A/B on the axis; C/D the off-axis ring at θ=0 (suffix 0) and θ=α (1).
    let (pa, pb) = (at(t_bot), at(t_top));
    let (c0, d0) = (radial(pa, w, r_bot), radial(pb, w, r_top));
    let (c1, d1) = (radial(pa, w_a, r_bot), radial(pb, w_a, r_top));

    // Wall surface: cylinder for equal radii, else the slice-2A cone.
    let wall_surface = if (r_bot - r_top).abs() <= band {
        Surface::Cylinder {
            axis_point: a0,
            axis_dir: a,
            radius: r_bot,
            reversed: false,
        }
    } else {
        let dt = t_top - t_bot;
        let ds = r_top - r_bot;
        Surface::Cone {
            apex: at(t_bot - r_bot * dt / ds),
            axis_dir: if ds > 0.0 { a } else { neg(a) },
            half_angle: (ds / dt).abs().atan(),
            reversed: false,
        }
    };

    // ---- direct assembly ---------------------------------------------------
    let vb = arena.vertices.len() as u32;
    let (va, vbx, vc0, vd0, vc1, vd1) = (
        VertexId(vb),
        VertexId(vb + 1),
        VertexId(vb + 2),
        VertexId(vb + 3),
        VertexId(vb + 4),
        VertexId(vb + 5),
    );
    for p in [pa, pb, c0, d0, c1, d1] {
        arena.vertices.push(Some(Vertex { point: p }));
    }

    // 18 half-edges. Naming: cap0 = start cap (θ=0, outward −m̂), cap1 =
    // end cap (θ=α, outward m̂_α), sb/st = bottom/top pie sectors (outward
    // ∓â), wl = the curved wall (outward radial).
    let hb = arena.half_edges.len() as u32;
    let h = |i: u32| HalfEdgeId(hb + i);
    // cap0 cycle: A→C0 (0), C0→D0 (1), D0→B (2), B→A (3)
    // cap1 cycle: A→B (4), B→D1 (5), D1→C1 (6), C1→A (7)
    // sector_bot cycle: A→C1 (8), arc C1→C0 (9), C0→A (10)
    // sector_top cycle: B→D0 (11), arc D0→D1 (12), D1→B (13)
    // wall cycle: arc C0→C1 (14), C1→D1 (15), arc D1→D0 (16), D0→C0 (17)
    let lb = arena.loops.len() as u32;
    let (l_cap0, l_cap1, l_sb, l_st, l_wl) = (
        LoopId(lb),
        LoopId(lb + 1),
        LoopId(lb + 2),
        LoopId(lb + 3),
        LoopId(lb + 4),
    );
    let fb = arena.faces.len() as u32;
    let (f_cap0, f_cap1, f_sb, f_st, f_wl) = (
        FaceId(fb),
        FaceId(fb + 1),
        FaceId(fb + 2),
        FaceId(fb + 3),
        FaceId(fb + 4),
    );
    let shell = ShellId(arena.shells.len() as u32);
    let solid = SolidId(arena.solids.len() as u32);

    let seg = Curve::LineSegment;
    let arc = |center: Point3, normal: UnitVector3, radius: f64| Curve::Arc {
        center,
        normal,
        radius,
    };
    // (id, twin, next, prev, origin, loop, curve)
    let hes: [(u32, u32, u32, u32, VertexId, LoopId, Curve); 18] = [
        // cap0: A→C0→D0→B→A, CCW around −m̂.
        (0, 10, 1, 3, va, l_cap0, seg),
        (1, 17, 2, 0, vc0, l_cap0, seg),
        (2, 11, 3, 1, vd0, l_cap0, seg),
        (3, 4, 0, 2, vbx, l_cap0, seg),
        // cap1: A→B→D1→C1→A, CCW around m̂_α.
        (4, 3, 5, 7, va, l_cap1, seg),
        (5, 13, 6, 4, vbx, l_cap1, seg),
        (6, 15, 7, 5, vd1, l_cap1, seg),
        (7, 8, 4, 6, vc1, l_cap1, seg),
        // sector_bot: A→C1, arc C1→C0 (CCW around −â), C0→A.
        (8, 7, 9, 10, va, l_sb, seg),
        (9, 14, 10, 8, vc1, l_sb, arc(pa, neg(a), r_bot)),
        (10, 0, 8, 9, vc0, l_sb, seg),
        // sector_top: B→D0, arc D0→D1 (CCW around +â), D1→B.
        (11, 2, 12, 13, vbx, l_st, seg),
        (12, 16, 13, 11, vd0, l_st, arc(pb, a, r_top)),
        (13, 5, 11, 12, vd1, l_st, seg),
        // wall: arc C0→C1 (+â), C1→D1, arc D1→D0 (−â), D0→C0.
        (14, 9, 15, 17, vc0, l_wl, arc(pa, a, r_bot)),
        (15, 6, 16, 14, vc1, l_wl, seg),
        (16, 12, 17, 15, vd1, l_wl, arc(pb, neg(a), r_top)),
        (17, 1, 14, 16, vd0, l_wl, seg),
    ];
    for &(_, twin, next, prev, origin, loop_id, curve) in &hes {
        arena.half_edges.push(Some(HalfEdge {
            twin: h(twin),
            next: h(next),
            prev: h(prev),
            origin,
            loop_id,
            curve,
        }));
    }

    for (face, boundary) in [
        (f_cap0, h(0)),
        (f_cap1, h(4)),
        (f_sb, h(8)),
        (f_st, h(11)),
        (f_wl, h(14)),
    ] {
        arena.loops.push(Some(Loop {
            face,
            boundary: LoopBoundary::Edges(boundary),
            kind: LoopKind::Outer,
        }));
    }
    let plane = |point: Point3, normal: UnitVector3| Some(Surface::Plane(Plane { point, normal }));
    for (surface, outer_loop) in [
        (plane(c0, neg(m)), l_cap0),
        (plane(c1, m_a), l_cap1),
        (plane(pa, neg(a)), l_sb),
        (plane(pb, a), l_st),
        (Some(wall_surface), l_wl),
    ] {
        arena.faces.push(Some(Face {
            surface,
            outer_loop,
            inner_loops: Vec::new(),
            shell,
        }));
    }
    arena.shells.push(Some(Shell {
        solid,
        faces: vec![f_cap0, f_cap1, f_sb, f_st, f_wl],
        genus: 0,
    }));
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));

    finalize_solid(arena, solid)?;
    // Result contract: start/end caps per the partial-revolve convention;
    // walls in profile order = [curved wall, bottom sector, top sector].
    Ok(RevolveResult {
        solid,
        shell,
        start_cap: Some(f_cap0),
        end_cap: Some(f_cap1),
        walls: vec![f_wl, f_sb, f_st],
    })
}

/// KV6 on-axis slice 2 increment B (spec
/// `specs/kv6_on_axis_revolve_oblique.md`, task #66): direct assembler for
/// the SOLID CONE swept full-turn by an on-axis apex triangle (the C0063
/// primary). The apex is an INTERIOR SINGULAR POINT of the lateral, not a
/// topological vertex — yang-rs's own cone model ([#24] Yang tessellates
/// the apex-pointed cone as a disk with a single base rim). Topology:
/// 1 seam vertex on the base rim, 1 edge (the rim circle — a twin pair of
/// closed half-edges, the PR-KV5a disc-cap form), 2 faces (disc cap +
/// apex lateral); V − E + F = 1 − 1 + 2 = 2, a ball. Orientation: the cap
/// rim is CCW around the cap's outward normal (away from the apex); the
/// lateral rim's traversal axis points TOWARD the apex — the apex analog
/// of the frustum's "toward the opposite rim" material-sense rule.
/// Safety obligation discharged by `finalize_solid` at exit.
///
/// Caller guarantees: `radius` strictly positive, `|t_apex − t_cap|`
/// above the alignment band.
fn build_on_axis_apex_cone(
    arena: &mut BrepArena,
    a: UnitVector3,
    w: UnitVector3,
    a0: Point3,
    t_cap: f64,
    t_apex: f64,
    radius: f64,
) -> Result<RevolveResult, KernelV2Error> {
    let at = |t: f64| Point3::new(a0.x() + t * a.x, a0.y() + t * a.y, a0.z() + t * a.z);
    let (c_cap, apex) = (at(t_cap), at(t_apex));
    // Outward cap normal points AWAY from the apex; the cone's axis_dir
    // (apex → rim, τ > 0) is the SAME direction.
    let n_cap = if t_cap < t_apex {
        UnitVector3 {
            x: -a.x,
            y: -a.y,
            z: -a.z,
        }
    } else {
        a
    };
    let toward_apex = UnitVector3 {
        x: -n_cap.x,
        y: -n_cap.y,
        z: -n_cap.z,
    };
    let height = (t_apex - t_cap).abs();
    let half_angle = (radius / height).atan();
    // Seam anchor: radially along ŵ (θ = 0), same azimuth as the other
    // on-axis constructions.
    let v0 = Point3::new(
        c_cap.x() + radius * w.x,
        c_cap.y() + radius * w.y,
        c_cap.z() + radius * w.z,
    );

    // ---- direct assembly ----------------------------------------------------
    let vid0 = VertexId(arena.vertices.len() as u32);
    arena.vertices.push(Some(Vertex { point: v0 }));

    let hb = arena.half_edges.len() as u32;
    let (cap_rim, lat_rim) = (HalfEdgeId(hb), HalfEdgeId(hb + 1));
    let lb = arena.loops.len() as u32;
    let (loop_cap, loop_lat) = (LoopId(lb), LoopId(lb + 1));
    let fb = arena.faces.len() as u32;
    let (f_cap, f_lat) = (FaceId(fb), FaceId(fb + 1));
    let shell = ShellId(arena.shells.len() as u32);
    let solid = SolidId(arena.solids.len() as u32);

    // Cap boundary: one closed circle half-edge, CCW around the cap's
    // outward normal.
    arena.half_edges.push(Some(HalfEdge {
        twin: lat_rim,
        next: cap_rim,
        prev: cap_rim,
        origin: vid0,
        loop_id: loop_cap,
        curve: Curve::Circle {
            center: c_cap,
            normal: n_cap,
            radius,
        },
    }));
    // Lateral boundary: the twin — traversal axis toward the apex.
    arena.half_edges.push(Some(HalfEdge {
        twin: cap_rim,
        next: lat_rim,
        prev: lat_rim,
        origin: vid0,
        loop_id: loop_lat,
        curve: Curve::Circle {
            center: c_cap,
            normal: toward_apex,
            radius,
        },
    }));

    for (face, boundary) in [(f_cap, cap_rim), (f_lat, lat_rim)] {
        arena.loops.push(Some(Loop {
            face,
            boundary: LoopBoundary::Edges(boundary),
            kind: LoopKind::Outer,
        }));
    }
    arena.faces.push(Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: c_cap,
            normal: n_cap,
        })),
        outer_loop: loop_cap,
        inner_loops: Vec::new(),
        shell,
    }));
    arena.faces.push(Some(Face {
        surface: Some(Surface::Cone {
            apex,
            axis_dir: n_cap,
            half_angle,
            reversed: false,
        }),
        outer_loop: loop_lat,
        inner_loops: Vec::new(),
        shell,
    }));
    arena.shells.push(Some(Shell {
        solid,
        faces: vec![f_cap, f_lat],
        genus: 0,
    }));
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));

    // ---- full production validation (defense in depth) --------------------
    finalize_solid(arena, solid)?;
    // I6: the apex end has no planar face to name — the single disc cap
    // fills both result fields; `walls` = the lateral.
    Ok(RevolveResult {
        solid,
        shell,
        start_cap: Some(f_cap),
        end_cap: Some(f_cap),
        walls: vec![f_lat],
    })
}
