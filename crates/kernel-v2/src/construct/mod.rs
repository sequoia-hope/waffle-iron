//! Primitive constructors over the B-Rep arena (PR-KV2 planar; PR-KV5a
//! adds circle profiles → right circular cylinders).
//!
//! Two public constructors. On polygon profiles both are pure
//! Euler-operator sequences (no raw arena mutation); on circle profiles
//! both dispatch to direct assemblers ([`extrude_circle`],
//! [`circle_lamina`] — justification on `extrude_circle`). All paths are
//! validated loudly:
//!
//! - [`make_face_from_profile`] — a **lamina**: two opposite-normal faces
//!   sharing the profile's boundary edges (Stroud 2006 §3.4 / fig. 3.9's
//!   "square lamina" intermediate; the closed-sheet starting state for
//!   sweeping). Holes become one ring on each face via the lid/`kemr`/
//!   `kfmrh` sequence at zero height (a holed lamina is torus-like:
//!   `V − E + F − R = 0 = 2(S − G)` with `G` = number of holes).
//! - [`extrude`] — the classic linear sweep (Stroud 2006 §6.2, fig. 6.11's
//!   vertex-based sweep: "MEV — first extension vertex; MEV, MFE — per
//!   extension face; …; MFE — final extension face"): build the base
//!   lamina, erect one post (`mev`) per profile vertex and one wall (`mef`)
//!   per profile edge, the receiving face's residual loop becoming the top;
//!   then drill each hole through with the KV1 through-hole sequence
//!   (bridge `mev` + spur chain + lid `mef` + `kemr`, posts down, hole
//!   walls, membrane `kfmrh`).
//!
//! ## Result invariants (asserted by the KV2 oracles)
//!
//! - `validate_solid` green (manifold, twin-paired, Newell-consistent,
//!   Euler–Poincaré with `G` = number of through-holes).
//! - Outward orientation: `geom::signed_volume > 0` for every extrude,
//!   regardless of whether the sweep direction is along `+n` or `−n` (the
//!   constructor reverses the working loop orientation when `d · n < 0`).
//! - Base and top faces are parallel (top is the base loop translated by
//!   the sweep vector); walls are planar quads (each wall is a
//!   parallelogram — two translated copies of one profile edge).
//! - Hole-wall outward normals point INTO the hole void (outward from
//!   material).
//!
//! ## Error contract
//!
//! All input validation happens BEFORE the first arena mutation, so an
//! `Err` from argument checking leaves the arena untouched. Once a
//! (pre-validated) construction sequence starts, the individual Euler
//! operators uphold their own invariants; a mid-sequence operator error
//! would indicate a kernel-v2 bug and is returned loudly rather than
//! masked. As defense in depth, both constructors run `validate_solid` on
//! the finished solid and propagate its verdict.

use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use crate::error::KernelV2Error;
use crate::euler::{kemr, kfmrh, mef, mev, mev_lone, mvfs};
use crate::profile::{cross, Profile, ProfileEdge, ProfileRegion};
use crate::validate::validate_solid;
use cad_primitives::{Point2, Point3, Vector3};

/// The universal constructor exit (KV13 F1): validate the finished solid,
/// then stamp a persistent id on every face that lacks one. Every public
/// constructor ends here, so all output faces carry a `Pid`. Validation runs
/// first; `Pid`s are pure metadata and never affect the geometry validate
/// checks. (Raw Euler-op test arenas that never reach a constructor simply
/// have no `Pid`s — `validate_solid` itself does not require them.)
pub(crate) fn finalize_solid(
    arena: &mut BrepArena,
    solid: crate::arena::SolidId,
) -> Result<(), KernelV2Error> {
    validate_solid(arena, solid)?;
    arena.assign_face_pids(solid)
}

/// `|d̂ · n̂|` floor below which an extrude direction is rejected as
/// in-plane ([`KernelV2Error::ExtrudeDirectionInPlane`]). A *sheared*
/// extrude (direction oblique to the plane normal) is legal — walls remain
/// planar parallelograms — but a direction this close to the plane spans
/// (numerically) no volume.
pub const EXTRUDE_MIN_NORMAL_COSINE: f64 = 1e-9;

/// `|d̂ × n̂|` ceiling above which a **circle** profile's extrude direction
/// is rejected as oblique
/// ([`KernelV2Error::ExtrudeObliqueCircleUnsupported`]). Sweeping a circle
/// obliquely produces an *elliptic* cylinder — out of the KV5a surface
/// vocabulary, so only right cylinders (`direction ∥ ±n̂`) are built. The
/// tolerance absorbs only unit-vector rounding (same bar as
/// `profile::CIRCLE_FRAME_ORTHONORMALITY_TOLERANCE`); the assay corpus
/// extrudes exclusively along the sketch-plane normal (`direction: null` in
/// all 339 extrudes of the 53 circle-bearing cases), so no corpus case is
/// excluded.
pub const CIRCLE_EXTRUDE_MAX_AXIS_SINE: f64 = 1e-9;

/// Entities produced by [`make_face_from_profile`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaminaResult {
    /// The new solid.
    pub solid: SolidId,
    /// Its single shell.
    pub shell: ShellId,
    /// The face whose outward normal is the profile's `+normalize(u × v)`.
    pub front: FaceId,
    /// The face whose outward normal is `−normalize(u × v)`.
    pub back: FaceId,
}

/// Entities produced by [`extrude`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtrudeResult {
    /// The new solid.
    pub solid: SolidId,
    /// Its single shell.
    pub shell: ShellId,
    /// The face lying in the profile plane. Its outward normal opposes the
    /// sweep vector's normal component.
    pub base: FaceId,
    /// The translated face (base loop + sweep vector); outward normal along
    /// the sweep vector's normal component.
    pub top: FaceId,
    /// Outer wall faces, one per outer-loop edge, in loop walk order.
    pub walls: Vec<FaceId>,
    /// Hole wall faces, `hole_walls[k]` = walls of hole `k`, one per hole
    /// edge, in loop walk order.
    pub hole_walls: Vec<Vec<FaceId>>,
}

/// Build a lamina (two opposite-normal faces sharing the profile boundary)
/// from a validated [`Profile`]. See the module docs for the construction
/// and invariants. Holes yield one ring on each face and increment shell
/// genus (the holed lamina is torus-like).
pub fn make_face_from_profile(
    arena: &mut BrepArena,
    profile: &Profile,
) -> Result<LaminaResult, KernelV2Error> {
    let (outer, holes) = match profile.region() {
        ProfileRegion::Circle { center, radius } => {
            return circle_lamina(arena, profile, *center, *radius);
        }
        ProfileRegion::ArcPolygon { .. } => {
            return Err(KernelV2Error::ArcPolygonProfileUnsupported);
        }
        ProfileRegion::Polygon { outer, holes } => (outer, holes),
    };
    // Outer boundary, CCW as stored ⇒ front face normal +normalize(u × v).
    let outer3: Vec<Point3> = outer.iter().map(|&p| profile.embed(p)).collect();
    let core = build_boundary_lamina(arena, &outer3)?;

    // Holes: lid + kemr (ring on front) + kfmrh (ring on back) at zero
    // height — the KV1 through-hole sequence without the sweep.
    for hole in holes {
        let hole3: Vec<Point3> = hole.iter().map(|&p| profile.embed(p)).collect();
        drill_hole(arena, core.front_anchor, &hole3, None, core.back)?;
    }

    finalize_solid(arena, core.solid)?;
    Ok(LaminaResult {
        solid: core.solid,
        shell: core.shell,
        front: core.front,
        back: core.back,
    })
}

/// Extrude a validated [`Profile`] along `normalize(direction) * distance`.
/// See the module docs for the construction sequence and result invariants.
///
/// Errors (all pre-mutation): [`KernelV2Error::ExtrudeNonPositiveDistance`]
/// (zero, negative, or non-finite distance),
/// [`KernelV2Error::ExtrudeDegenerateDirection`] (zero / non-finite
/// direction), [`KernelV2Error::ExtrudeDirectionInPlane`]
/// (`|d̂ · n̂| < EXTRUDE_MIN_NORMAL_COSINE`).
pub fn extrude(
    arena: &mut BrepArena,
    profile: &Profile,
    direction: Vector3,
    distance: f64,
) -> Result<ExtrudeResult, KernelV2Error> {
    // ---- argument validation (ALL before the first mutation) -------------
    if !distance.is_finite() || distance <= 0.0 {
        return Err(KernelV2Error::ExtrudeNonPositiveDistance);
    }
    let d = [direction.x(), direction.y(), direction.z()];
    let d_sq = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if !d_sq.is_finite() || d_sq <= 0.0 {
        return Err(KernelV2Error::ExtrudeDegenerateDirection);
    }
    let d_len = d_sq.sqrt();
    let n = cross(profile.u(), profile.v());
    let n_len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let cosine = (d[0] * n[0] + d[1] * n[1] + d[2] * n[2]) / (d_len * n_len);
    if cosine.abs() < EXTRUDE_MIN_NORMAL_COSINE {
        return Err(KernelV2Error::ExtrudeDirectionInPlane);
    }

    // ---- circle profile → right circular cylinder (PR-KV5a) --------------
    let (outer, holes) = match profile.region() {
        ProfileRegion::Circle { center, radius } => {
            return extrude_circle(arena, profile, *center, *radius, d, d_len, cosine, distance);
        }
        ProfileRegion::ArcPolygon { outer, holes } => {
            return extrude_arc_profile(arena, profile, outer, holes, d, d_len, cosine, distance);
        }
        ProfileRegion::Polygon { outer, holes } => (outer, holes),
    };

    // Sweep vector w = d̂ · distance.
    let w = [
        d[0] / d_len * distance,
        d[1] / d_len * distance,
        d[2] / d_len * distance,
    ];
    // When the sweep opposes the profile normal, reverse the working loop
    // orientation: the construction below assumes the front face's normal
    // (= Newell of the loop as built) has a positive component along `w`,
    // which is exactly what makes the finished solid outward-oriented.
    let reverse = cosine < 0.0;

    // ---- base lamina in the profile plane --------------------------------
    let outer3 = embed_loop(profile, outer, reverse, None);
    let core = build_boundary_lamina(arena, &outer3)?;

    // ---- erect the outer walls (Stroud §6.2 vertex-based sweep) ----------
    // Snapshot the front loop, then one post (mev) per vertex: anchoring at
    // the half-edge LEAVING each vertex inserts the spur at that vertex,
    // and the pre-snapshot stays valid because mev only inserts before its
    // anchor.
    let front_hes = arena.loop_half_edges(core.front_loop)?;
    let mut posts = Vec::with_capacity(front_hes.len());
    for &h in &front_hes {
        let p = arena.vertex(arena.half_edge(h)?.origin)?.point;
        posts.push(mev(arena, h, translate(p, w))?);
    }
    // One wall (mef) per edge: rim edge between consecutive post tips. The
    // final wall closes onto the FIRST wall's old-side rim edge because the
    // first post's `he_in` was consumed into the first wall's loop (KV1
    // cube steps 10–13). The front face's residual loop becomes the top.
    let mut walls = Vec::with_capacity(posts.len());
    let first_wall = mef(arena, posts[0].he_in, posts[1].he_in)?;
    walls.push(first_wall.face);
    for i in 1..posts.len() - 1 {
        walls.push(mef(arena, posts[i].he_in, posts[i + 1].he_in)?.face);
    }
    walls.push(mef(arena, posts[posts.len() - 1].he_in, first_wall.he_old_side)?.face);

    // ---- drill each hole through (KV1 through-hole sequence) -------------
    // `first_wall.he_old_side` is a top-rim edge that stays in the top
    // face's loop (mef's he_to side), so it anchors every hole bridge.
    let neg_w = [-w[0], -w[1], -w[2]];
    let mut hole_walls = Vec::with_capacity(holes.len());
    for hole in holes {
        let top_pts = embed_loop(profile, hole, reverse, Some(w));
        hole_walls.push(drill_hole(
            arena,
            first_wall.he_old_side,
            &top_pts,
            Some(neg_w),
            core.back,
        )?);
    }

    finalize_solid(arena, core.solid)?;
    Ok(ExtrudeResult {
        solid: core.solid,
        shell: core.shell,
        base: core.back,
        top: core.front,
        walls,
        hole_walls,
    })
}

mod revolve;
pub use revolve::{
    revolve, RevolveResult, REVOLVE_AXIS_IN_PLANE_TOLERANCE, REVOLVE_EDGE_ALIGNMENT_TOLERANCE,
    REVOLVE_FULL_TURN_TOLERANCE, REVOLVE_MIN_AXIS_CLEARANCE_REL,
};

/// Extrude a circle profile into a right circular cylinder (PR-KV5a).
///
/// ## Why a direct assembler, not an Euler-operator sequence
///
/// The Euler operators create `LineSegment` half-edges and derive face
/// planes from Newell normals of polygonal walks; a closed circle edge (a
/// self-loop half-edge pair) and a `Cylinder` surface are outside that
/// operator vocabulary — no sequence from Stroud's operator table lands on
/// this topology without first inventing curved operator variants that
/// would have exactly one caller. Per the KV3 `from_yang_brep` precedent,
/// the safety obligation is discharged by full `validate_solid` at exit
/// (the assembled arena is checked against every invariant — twin pairing,
/// curve-twin consistency, vertex fans, curved orientation rules,
/// Euler–Poincaré), not by the mutation path.
///
/// ## Topology (arena module docs, "Closed curved edges")
///
/// Stroud 2006 §3.1.4 single-fake-edge representation / yang-rs M5 fixture
/// topology: 2 seam vertices, 3 edges (two vertex-anchored closed rim
/// circles + one straight seam ruling), 3 faces. V−E+F−R = 2 = 2(S−G).
///
/// ## Geometry and orientation
///
/// The sweep axis is `a = ±n̂` (the profile plane's unit normal, signed by
/// the direction's normal component): the oblique gate has already
/// established `|d̂ × n̂| ≤ CIRCLE_EXTRUDE_MAX_AXIS_SINE`, and a right
/// cylinder is BY DEFINITION swept along its base normal — snapping to
/// `±n̂` keeps the rims exactly in the cap planes instead of carrying a
/// sub-1e-9 elliptic perturbation. Outward orientation: base cap normal
/// `−a`, top cap `+a`, lateral radially outward; the rim circle half-edges'
/// directional normals follow the conventions on [`crate::arena::Curve`]
/// (cap rim CCW around the cap's outward normal; each lateral rim's
/// traversal axis points toward the opposite rim).
#[allow(clippy::too_many_arguments)]
fn extrude_circle(
    arena: &mut BrepArena,
    profile: &Profile,
    center: Point2,
    radius: f64,
    d: [f64; 3],
    d_len: f64,
    cosine: f64,
    distance: f64,
) -> Result<ExtrudeResult, KernelV2Error> {
    // ---- oblique gate (still pre-mutation) --------------------------------
    let n_unit = profile.unit_normal();
    let dn = [d[0] / d_len, d[1] / d_len, d[2] / d_len];
    let cx = [
        dn[1] * n_unit.z - dn[2] * n_unit.y,
        dn[2] * n_unit.x - dn[0] * n_unit.z,
        dn[0] * n_unit.y - dn[1] * n_unit.x,
    ];
    let sine = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    if sine > CIRCLE_EXTRUDE_MAX_AXIS_SINE {
        return Err(KernelV2Error::ExtrudeObliqueCircleUnsupported);
    }

    // ---- geometry ---------------------------------------------------------
    // Axis: the profile normal, signed by the sweep sense.
    let a = if cosine >= 0.0 { n_unit } else { neg(n_unit) };
    let neg_a = neg(a);
    let w = [a.x * distance, a.y * distance, a.z * distance];
    let c0 = profile.embed(center);
    let c1 = translate(c0, w);
    // Seam anchors: radially along the (unit, in-plane) `u` basis vector.
    let v0 = radial_point(c0, profile.u(), radius);
    let v1 = radial_point(c1, profile.u(), radius);

    // ---- direct assembly --------------------------------------------------
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

    let rim = |center: Point3, normal: UnitVector3| Curve::Circle {
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
        curve: rim(c0, neg_a),
    }));
    // Lateral loop: bottom rim (CCW around +a — toward the top rim), seam
    // up, top rim (CCW around −a — toward the bottom rim), seam down.
    arena.half_edges.push(Some(HalfEdge {
        twin: cap_b,
        next: seam_up,
        prev: seam_dn,
        origin: vid0,
        loop_id: loop_lat,
        curve: rim(c0, a),
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
        curve: rim(c1, neg_a),
    }));
    // Top cap boundary: CCW around the cap's outward normal +a.
    arena.half_edges.push(Some(HalfEdge {
        twin: lat_t,
        next: cap_t,
        prev: cap_t,
        origin: vid1,
        loop_id: loop_top,
        curve: rim(c1, a),
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
        surface: Some(Surface::Cylinder {
            axis_point: c0,
            axis_dir: a,
            radius,
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
    Ok(ExtrudeResult {
        solid,
        shell,
        base: f_base,
        top: f_top,
        walls: vec![f_lat],
        hole_walls: Vec::new(),
    })
}

/// Per-edge geometry of an `ArcPolygon` extrude, precomputed before any arena
/// mutation. Edge `i` joins working vertex `i` to `i + 1`.
struct ArcEdgeGeom {
    /// `true` for a [`ProfileEdge::Arc`] (→ cylinder wall), else a line edge
    /// (→ planar wall).
    is_arc: bool,
    /// Arc circle center, embedded in the bottom plane (arc edges only).
    c_bot: Point3,
    /// Arc circle center, embedded in the top plane (`c_bot + w`).
    c_top: Point3,
    /// Arc radius (arc edges only).
    radius: f64,
    /// Directional normal of the BOTTOM wall half-edge `wb[i]` (ring0[i] →
    /// ring0[i+1]): `+a` if that traversal is CCW around the sweep axis,
    /// else `−a`. Twins / cap edges derive their normals by negation.
    wb_normal: UnitVector3,
    /// Cylinder cavity sense: `true` for a concave arc (center on the
    /// material's far side — wall outward points toward the axis).
    reversed: bool,
    /// Outward normal of a planar (line-edge) wall: `normalize((B−A) × a)`.
    wall_normal: UnitVector3,
}

/// Per-loop precomputed geometry for the arc extrude assembler.
struct ArcLoopGeom {
    /// Edge count of this loop.
    k: usize,
    /// Global edge-index offset (Σ of earlier loops' `k`).
    off: usize,
    /// Base vertex id (ring0 then ring1 follow).
    vbase: u32,
    /// Bottom-ring vertices (embedded boundary).
    ring0: Vec<Point3>,
    /// Top-ring vertices (`ring0 + w`).
    ring1: Vec<Point3>,
    /// Per-edge surface/curve geometry.
    eg: Vec<ArcEdgeGeom>,
}

/// Reverse a working loop: reverse edge order AND swap each edge's endpoints
/// (circle centre/radius are orientation-free). Used to put the outer loop
/// CCW-around-`+a` and each hole loop CW-around-`+a`.
fn reverse_arc_loop(edges: &[ProfileEdge]) -> Vec<ProfileEdge> {
    edges
        .iter()
        .rev()
        .map(|e| match *e {
            ProfileEdge::Line { a, b } => ProfileEdge::Line { a: b, b: a },
            ProfileEdge::Arc {
                a,
                b,
                center,
                radius,
                ccw,
            } => ProfileEdge::Arc {
                a: b,
                b: a,
                center,
                radius,
                ccw: !ccw,
            },
        })
        .collect()
}

/// Extrude a validated mixed line/arc [`Profile`] (`ProfileRegion::ArcPolygon`,
/// PR-KV12 Tier 2) perpendicular to its plane: planar caps bounded by the
/// line+arc loops, a planar side wall per line edge, and an exact
/// [`Surface::Cylinder`] patch per arc edge (an arc swept linearly along the
/// normal IS a cylinder lateral). Direct arena assembler — same half-edge /
/// twin wiring as [`build_partial_revolve`] (linear seams replace the swept
/// arcs; the profile edges carry the `Line`/`Arc` curves); the safety
/// obligation is discharged by `validate_solid` at exit.
///
/// Holes (E4b): each hole loop is wound CW-around-`+a` (the reverse of the
/// outer), so the SAME per-edge generation yields a cap inner loop with the
/// correct (opposite) winding and wall normals pointing INTO the cavity. The
/// caps become annular faces (`inner_loops`); the shell genus is the hole
/// count (each through-hole adds genus 1).
#[allow(clippy::too_many_arguments)]
fn extrude_arc_profile(
    arena: &mut BrepArena,
    profile: &Profile,
    outer: &[ProfileEdge],
    holes: &[Vec<ProfileEdge>],
    d: [f64; 3],
    d_len: f64,
    cosine: f64,
    distance: f64,
) -> Result<ExtrudeResult, KernelV2Error> {
    // ---- oblique gate (pre-mutation) -------------------------------------
    // Oblique sweep of an arc → elliptic-section cylinder (out of Tier-2 v1).
    let n_unit = profile.unit_normal();
    let dn = [d[0] / d_len, d[1] / d_len, d[2] / d_len];
    let cx = [
        dn[1] * n_unit.z - dn[2] * n_unit.y,
        dn[2] * n_unit.x - dn[0] * n_unit.z,
        dn[0] * n_unit.y - dn[1] * n_unit.x,
    ];
    let sine = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    if sine > CIRCLE_EXTRUDE_MAX_AXIS_SINE {
        return Err(KernelV2Error::ExtrudeObliqueArcUnsupported);
    }

    // ---- geometry (sweep axis `a`) ---------------------------------------
    // `a` is the profile normal signed by the sweep sense; the OUTER loop must
    // wind CCW around `+a` (so the solid is outward-oriented), holes CW.
    let a = if cosine >= 0.0 { n_unit } else { neg(n_unit) };
    let w = [a.x * distance, a.y * distance, a.z * distance];
    let reverse_outer = cosine < 0.0;

    // Working loops: index 0 = outer (CCW +a), 1.. = holes (CW +a).
    let mut loops_work: Vec<Vec<ProfileEdge>> = Vec::with_capacity(1 + holes.len());
    loops_work.push(if reverse_outer {
        reverse_arc_loop(outer)
    } else {
        outer.to_vec()
    });
    for hole in holes {
        loops_work.push(if reverse_outer {
            hole.to_vec()
        } else {
            reverse_arc_loop(hole)
        });
    }
    let n_loops = loops_work.len() as u32;

    let cross3 = |u: [f64; 3], v: [f64; 3]| {
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    };
    let unit = |v: [f64; 3]| -> Option<UnitVector3> {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        (l > 0.0).then_some(UnitVector3 {
            x: v[0] / l,
            y: v[1] / l,
            z: v[2] / l,
        })
    };

    // ---- per-loop geometry (all before mutation) -------------------------
    let mut geoms: Vec<ArcLoopGeom> = Vec::with_capacity(loops_work.len());
    let mut off = 0usize;
    for lw in &loops_work {
        let k = lw.len();
        let ring0: Vec<Point3> = lw.iter().map(|e| profile.embed(e.start())).collect();
        let ring1: Vec<Point3> = ring0.iter().map(|p| translate(*p, w)).collect();
        let mut eg: Vec<ArcEdgeGeom> = Vec::with_capacity(k);
        for (i, e) in lw.iter().enumerate() {
            let a3 = ring0[i];
            let b3 = ring0[(i + 1) % k];
            let edge_vec = [b3.x() - a3.x(), b3.y() - a3.y(), b3.z() - a3.z()];
            match *e {
                ProfileEdge::Line { .. } => {
                    let wn = unit(cross3(edge_vec, [a.x, a.y, a.z]))
                        .ok_or(KernelV2Error::ProfileArcEdgeInvalid)?;
                    eg.push(ArcEdgeGeom {
                        is_arc: false,
                        c_bot: a3,
                        c_top: a3,
                        radius: 0.0,
                        wb_normal: a,
                        reversed: false,
                        wall_normal: wn,
                    });
                }
                ProfileEdge::Arc { center, radius, .. } => {
                    let c_bot = profile.embed(center);
                    let c_top = translate(c_bot, w);
                    let va = [a3.x() - c_bot.x(), a3.y() - c_bot.y(), a3.z() - c_bot.z()];
                    let vb = [b3.x() - c_bot.x(), b3.y() - c_bot.y(), b3.z() - c_bot.z()];
                    let cr = cross3(va, vb);
                    let sdot = cr[0] * a.x + cr[1] * a.y + cr[2] * a.z;
                    let ccw = sdot > 0.0;
                    eg.push(ArcEdgeGeom {
                        is_arc: true,
                        c_bot,
                        c_top,
                        radius,
                        wb_normal: if ccw { a } else { neg(a) },
                        reversed: !ccw,
                        wall_normal: a,
                    });
                }
            }
        }
        geoms.push(ArcLoopGeom {
            k,
            off,
            vbase: 0,
            ring0,
            ring1,
            eg,
        });
        off += k;
    }
    let total_k = off;

    // ---- arena id layout --------------------------------------------------
    // Vertices: ring0 then ring1, loop by loop (record each loop's base).
    for g in geoms.iter_mut() {
        g.vbase = arena.vertices.len() as u32;
        for p in g.ring0.iter().chain(g.ring1.iter()) {
            arena.vertices.push(Some(Vertex { point: *p }));
        }
    }
    let hb = arena.half_edges.len() as u32;
    let lb = arena.loops.len() as u32;
    let fb = arena.faces.len() as u32;
    let shell = ShellId(arena.shells.len() as u32);
    let solid = SolidId(arena.solids.len() as u32);

    // Half-edges: 6 per global edge index; loops are emitted in order so the
    // global index `off + i` is contiguous and matches these accessors.
    let sc = |g: u32| HalfEdgeId(hb + 6 * g);
    let ec = |g: u32| HalfEdgeId(hb + 6 * g + 1);
    let wb = |g: u32| HalfEdgeId(hb + 6 * g + 2);
    let wt = |g: u32| HalfEdgeId(hb + 6 * g + 3);
    let af = |g: u32| HalfEdgeId(hb + 6 * g + 4);
    let ab = |g: u32| HalfEdgeId(hb + 6 * g + 5);
    // Loops: bottom caps (one LoopId per profile loop), then top caps, then
    // one wall loop per global edge.
    let loop_bot = |l: u32| LoopId(lb + l);
    let loop_top = |l: u32| LoopId(lb + n_loops + l);
    let loop_wall = |g: u32| LoopId(lb + 2 * n_loops + g);
    let f_bot = FaceId(fb);
    let f_top = FaceId(fb + 1);
    let f_wall = |g: u32| FaceId(fb + 2 + g);

    let arc_or_line = |g: &ArcEdgeGeom, center: Point3, normal: UnitVector3| {
        if g.is_arc {
            Curve::Arc {
                center,
                normal,
                radius: g.radius,
            }
        } else {
            Curve::LineSegment
        }
    };

    // Emit half-edges loop by loop (so half-edge index hb + 6·(off+i) holds).
    for (l, lg) in geoms.iter().enumerate() {
        let l = l as u32;
        let k = lg.k;
        let off_l = lg.off as u32;
        let vbase = lg.vbase;
        let v0 = |i: usize| VertexId(vbase + (i % k) as u32);
        let v1 = |i: usize| VertexId(vbase + k as u32 + (i % k) as u32);
        // Global edge index for local edge `i` (wrapping within this loop).
        let gi = |i: usize| off_l + (i % k) as u32;
        let lbot = loop_bot(l);
        let ltop = loop_top(l);
        for (i, g) in lg.eg.iter().enumerate() {
            // sc[i]: bottom cap, ring0[i+1] → ring0[i].
            arena.half_edges.push(Some(HalfEdge {
                twin: wb(gi(i)),
                next: sc(gi(i + k - 1)),
                prev: sc(gi(i + 1)),
                origin: v0(i + 1),
                loop_id: lbot,
                curve: arc_or_line(g, g.c_bot, neg(g.wb_normal)),
            }));
            // ec[i]: top cap, ring1[i] → ring1[i+1].
            arena.half_edges.push(Some(HalfEdge {
                twin: wt(gi(i)),
                next: ec(gi(i + 1)),
                prev: ec(gi(i + k - 1)),
                origin: v1(i),
                loop_id: ltop,
                curve: arc_or_line(g, g.c_top, g.wb_normal),
            }));
            // Wall i cycle: wb[i] → af[i+1] → wt[i] → ab[i] → wb[i].
            arena.half_edges.push(Some(HalfEdge {
                twin: sc(gi(i)),
                next: af(gi(i + 1)),
                prev: ab(gi(i)),
                origin: v0(i),
                loop_id: loop_wall(gi(i)),
                curve: arc_or_line(g, g.c_bot, g.wb_normal),
            }));
            arena.half_edges.push(Some(HalfEdge {
                twin: ec(gi(i)),
                next: ab(gi(i)),
                prev: af(gi(i + 1)),
                origin: v1(i + 1),
                loop_id: loop_wall(gi(i)),
                curve: arc_or_line(g, g.c_top, neg(g.wb_normal)),
            }));
            // af[i]: seam up at vertex i; lives in wall (i−1)'s loop.
            arena.half_edges.push(Some(HalfEdge {
                twin: ab(gi(i)),
                next: wt(gi(i + k - 1)),
                prev: wb(gi(i + k - 1)),
                origin: v0(i),
                loop_id: loop_wall(gi(i + k - 1)),
                curve: Curve::LineSegment,
            }));
            // ab[i]: seam down at vertex i; twin af[i].
            arena.half_edges.push(Some(HalfEdge {
                twin: af(gi(i)),
                next: wb(gi(i)),
                prev: wt(gi(i)),
                origin: v1(i),
                loop_id: loop_wall(gi(i)),
                curve: Curve::LineSegment,
            }));
        }
    }

    // ---- loops ------------------------------------------------------------
    // Bottom caps: loop 0 is the outer boundary of f_bot, holes are inner.
    for l in 0..n_loops {
        arena.loops.push(Some(Loop {
            face: f_bot,
            boundary: LoopBoundary::Edges(sc(geoms[l as usize].off as u32)),
            kind: if l == 0 {
                LoopKind::Outer
            } else {
                LoopKind::Inner
            },
        }));
    }
    for l in 0..n_loops {
        arena.loops.push(Some(Loop {
            face: f_top,
            boundary: LoopBoundary::Edges(ec(geoms[l as usize].off as u32)),
            kind: if l == 0 {
                LoopKind::Outer
            } else {
                LoopKind::Inner
            },
        }));
    }
    for g in 0..total_k as u32 {
        arena.loops.push(Some(Loop {
            face: f_wall(g),
            boundary: LoopBoundary::Edges(wb(g)),
            kind: LoopKind::Outer,
        }));
    }

    // ---- faces ------------------------------------------------------------
    let bot_inner: Vec<LoopId> = (1..n_loops).map(loop_bot).collect();
    let top_inner: Vec<LoopId> = (1..n_loops).map(loop_top).collect();
    arena.faces.push(Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: geoms[0].ring0[0],
            normal: neg(a),
        })),
        outer_loop: loop_bot(0),
        inner_loops: bot_inner,
        shell,
    }));
    arena.faces.push(Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: geoms[0].ring1[0],
            normal: a,
        })),
        outer_loop: loop_top(0),
        inner_loops: top_inner,
        shell,
    }));
    let mut walls = Vec::with_capacity(geoms[0].k);
    let mut hole_walls: Vec<Vec<FaceId>> = Vec::with_capacity(holes.len());
    for (l, lg) in geoms.iter().enumerate() {
        let mut loop_faces = Vec::with_capacity(lg.k);
        for (i, g) in lg.eg.iter().enumerate() {
            let gidx = (lg.off + i) as u32;
            let surface = if g.is_arc {
                Surface::Cylinder {
                    axis_point: g.c_bot,
                    axis_dir: a,
                    radius: g.radius,
                    reversed: g.reversed,
                }
            } else {
                Surface::Plane(Plane {
                    point: lg.ring0[i],
                    normal: g.wall_normal,
                })
            };
            arena.faces.push(Some(Face {
                surface: Some(surface),
                outer_loop: loop_wall(gidx),
                inner_loops: Vec::new(),
                shell,
            }));
            loop_faces.push(f_wall(gidx));
        }
        if l == 0 {
            walls = loop_faces;
        } else {
            hole_walls.push(loop_faces);
        }
    }

    let mut shell_faces = vec![f_bot, f_top];
    for g in 0..total_k as u32 {
        shell_faces.push(f_wall(g));
    }
    arena.shells.push(Some(Shell {
        solid,
        faces: shell_faces,
        genus: holes.len() as u32,
    }));
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));

    finalize_solid(arena, solid)?;
    Ok(ExtrudeResult {
        solid,
        shell,
        base: f_bot,
        top: f_top,
        walls,
        hole_walls,
    })
}

/// Build the circular lamina (the zero-height analog of the polygon
/// lamina): one seam vertex, one closed circle edge, two opposite-normal
/// disk faces sharing it. `V − E + F − R = 1 − 1 + 2 = 2 = 2(S − G)`.
/// Same direct-assembly + `validate_solid` justification as
/// [`extrude_circle`].
fn circle_lamina(
    arena: &mut BrepArena,
    profile: &Profile,
    center: Point2,
    radius: f64,
) -> Result<LaminaResult, KernelV2Error> {
    let n = profile.unit_normal();
    let c0 = profile.embed(center);
    let v0 = radial_point(c0, profile.u(), radius);

    let vid = VertexId(arena.vertices.len() as u32);
    arena.vertices.push(Some(Vertex { point: v0 }));
    let hb = arena.half_edges.len() as u32;
    let (he_front, he_back) = (HalfEdgeId(hb), HalfEdgeId(hb + 1));
    let lb = arena.loops.len() as u32;
    let fb = arena.faces.len() as u32;
    let (f_front, f_back) = (FaceId(fb), FaceId(fb + 1));
    let shell = ShellId(arena.shells.len() as u32);
    let solid = SolidId(arena.solids.len() as u32);

    // Front face winds CCW around +n, back around −n; each disk's single
    // circle half-edge carries the face's outward normal.
    for (k, (he, twin, normal)) in [(he_front, he_back, n), (he_back, he_front, neg(n))]
        .into_iter()
        .enumerate()
    {
        arena.half_edges.push(Some(HalfEdge {
            twin,
            next: he,
            prev: he,
            origin: vid,
            loop_id: LoopId(lb + k as u32),
            curve: Curve::Circle {
                center: c0,
                normal,
                radius,
            },
        }));
        arena.loops.push(Some(Loop {
            face: FaceId(fb + k as u32),
            boundary: LoopBoundary::Edges(he),
            kind: LoopKind::Outer,
        }));
        arena.faces.push(Some(Face {
            surface: Some(Surface::Plane(Plane { point: c0, normal })),
            outer_loop: LoopId(lb + k as u32),
            inner_loops: Vec::new(),
            shell,
        }));
    }
    arena.shells.push(Some(Shell {
        solid,
        faces: vec![f_front, f_back],
        genus: 0,
    }));
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));

    finalize_solid(arena, solid)?;
    Ok(LaminaResult {
        solid,
        shell,
        front: f_front,
        back: f_back,
    })
}

fn neg(n: UnitVector3) -> UnitVector3 {
    UnitVector3 {
        x: -n.x,
        y: -n.y,
        z: -n.z,
    }
}

/// `c + r·u` — a point at radial offset `r` along the unit vector `u`.
fn radial_point(c: Point3, u: Vector3, r: f64) -> Point3 {
    Point3::new(c.x() + r * u.x(), c.y() + r * u.y(), c.z() + r * u.z())
}

// ---------------------------------------------------------------------------
// Internal construction helpers
// ---------------------------------------------------------------------------

fn translate(p: Point3, w: [f64; 3]) -> Point3 {
    Point3::new(p.x() + w[0], p.y() + w[1], p.z() + w[2])
}

/// Embed a stored-CCW profile loop into 3D, optionally reversing the
/// winding (for sweeps opposing the profile normal) and optionally
/// offsetting by a sweep vector (for hole loops drilled from the top).
fn embed_loop(
    profile: &Profile,
    loop2: &[Point2],
    reverse: bool,
    offset: Option<[f64; 3]>,
) -> Vec<Point3> {
    let embed = |&p: &Point2| {
        let q = profile.embed(p);
        match offset {
            Some(w) => translate(q, w),
            None => q,
        }
    };
    if reverse {
        loop2.iter().rev().map(embed).collect()
    } else {
        loop2.iter().map(embed).collect()
    }
}

/// The boundary lamina shared by both constructors.
struct LaminaCore {
    solid: SolidId,
    shell: ShellId,
    /// Face whose normal is the Newell normal of `pts` in given order.
    front: FaceId,
    /// Opposite face.
    back: FaceId,
    /// The front face's outer loop.
    front_loop: LoopId,
    /// A half-edge guaranteed to remain in the front face's outer loop
    /// (`mef`'s `he_to`, which stays in the old loop) — the lamina-path
    /// drill anchor.
    front_anchor: HalfEdgeId,
}

/// Build a lamina from a closed 3D polygon: `mvfs` + `mev_lone` +
/// `(k − 2) × mev` + closing `mef` (the KV1 cube steps 1–5 generalized).
/// The mef's new face takes the REVERSED cycle (`he_from` = last spur's
/// inbound half-edge), so `front` (the mvfs face) winds with `pts`.
fn build_boundary_lamina(
    arena: &mut BrepArena,
    pts: &[Point3],
) -> Result<LaminaCore, KernelV2Error> {
    let m = mvfs(arena, pts[0])?;
    let first = mev_lone(arena, m.outer_loop, pts[1])?;
    let mut prev_in = first.he_in;
    for &p in &pts[2..] {
        prev_in = mev(arena, prev_in, p)?.he_in;
    }
    let closing = mef(arena, prev_in, first.he_out)?;
    Ok(LaminaCore {
        solid: m.solid,
        shell: m.shell,
        front: m.face,
        back: closing.face,
        front_loop: m.outer_loop,
        front_anchor: first.he_out,
    })
}

/// Drill one hole: bridge `mev` from a receiving-face loop vertex, spur
/// chain around the hole, lid `mef`, `kemr` on the bridge (ring on the
/// receiving face), then either transfer the lid directly (`sweep: None` —
/// the lamina case) or sweep the lid down (`posts + walls`) and transfer
/// the residual membrane — `kfmrh` to `membrane_recv` either way (genus + 1).
///
/// `top_pts` must wind CCW with respect to the receiving face's normal
/// (the KV1 through-hole steps 1–15). Returns the hole wall faces (empty
/// for the lamina case), in lid-loop walk order.
fn drill_hole(
    arena: &mut BrepArena,
    anchor: HalfEdgeId,
    top_pts: &[Point3],
    sweep: Option<[f64; 3]>,
    membrane_recv: FaceId,
) -> Result<Vec<FaceId>, KernelV2Error> {
    let bridge = mev(arena, anchor, top_pts[0])?;
    let mut spurs = Vec::with_capacity(top_pts.len() - 1);
    let mut prev_in = bridge.he_in;
    for &q in &top_pts[1..] {
        let s = mev(arena, prev_in, q)?;
        prev_in = s.he_in;
        spurs.push(s);
    }
    // Lid = the forward side of the spur chain (q0 → … → q_{m−1} → q0).
    let lid = mef(arena, spurs[0].he_out, spurs[spurs.len() - 1].he_in)?;
    // Kill the bridge: the chain's BACK side becomes the ring of the
    // receiving face (winding opposite its outer loop).
    kemr(arena, bridge.he_out)?;

    let mut walls = Vec::new();
    if let Some(wv) = sweep {
        // Sweep the lid loop down: posts + walls, same pattern as the outer
        // erection (the lid face's residual loop becomes the membrane).
        let lid_hes = arena.loop_half_edges(lid.new_loop)?;
        let mut posts = Vec::with_capacity(lid_hes.len());
        for &h in &lid_hes {
            let p = arena.vertex(arena.half_edge(h)?.origin)?.point;
            posts.push(mev(arena, h, translate(p, wv))?);
        }
        let first_wall = mef(arena, posts[0].he_in, posts[1].he_in)?;
        walls.push(first_wall.face);
        for i in 1..posts.len() - 1 {
            walls.push(mef(arena, posts[i].he_in, posts[i + 1].he_in)?.face);
        }
        walls.push(mef(arena, posts[posts.len() - 1].he_in, first_wall.he_old_side)?.face);
    }
    // Open the hole: kill the lid/membrane face, its loop becomes a ring of
    // the receiver, shell genus increments (Stroud §F.9 same-shell case).
    kfmrh(arena, lid.face, membrane_recv)?;
    Ok(walls)
}
