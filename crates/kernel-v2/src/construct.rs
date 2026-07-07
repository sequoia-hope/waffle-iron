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

/// Entities produced by [`revolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevolveResult {
    /// The new solid.
    pub solid: SolidId,
    /// Its single shell.
    pub shell: ShellId,
    /// The profile face at sweep angle 0. For a partial revolve its outward
    /// normal opposes the sweep velocity; for the 360° branch it is the
    /// annular cap at the axial minimum (outward normal `−â`).
    pub start_cap: FaceId,
    /// The profile face at the sweep angle (partial) / the annular cap at
    /// the axial maximum (360°, outward normal `+â`).
    pub end_cap: FaceId,
    /// Lateral faces, one per profile edge, in loop walk order: cylinder
    /// patches for axis-parallel edges, planar annular sectors for
    /// axis-perpendicular edges (partial); the outer + inner full cylinders
    /// (360°).
    pub walls: Vec<FaceId>,
}

/// `|â · n̂|` ceiling above which the revolve axis direction is rejected as
/// out of the profile plane, and the relative band (scaled by the geometry
/// magnitude) for the axis origin's distance to the plane. Absorbs only
/// unit-vector rounding — the assay generator emits exactly in-plane axes.
pub const REVOLVE_AXIS_IN_PLANE_TOLERANCE: f64 = 1e-9;

/// Per-edge alignment band: a profile edge is axis-parallel when its radial
/// extent is below `tol · length`, axis-perpendicular when its axial extent
/// is; anything in between is an oblique edge (a CONE — KV6c), rejected
/// typed. Corpus rectangles are exactly axis-aligned.
pub const REVOLVE_EDGE_ALIGNMENT_TOLERANCE: f64 = 1e-9;

/// Relative clearance the profile must keep from the axis (scaled by the
/// geometry magnitude). Touching or crossing the axis is invalid input
/// ([`KernelV2Error::RevolveAxisIntersectsProfile`]): crossing
/// self-intersects, touching pinches a non-manifold seam (the on-axis
/// solid-of-revolution is a later capability).
pub const REVOLVE_MIN_AXIS_CLEARANCE_REL: f64 = 1e-9;

/// `|α − 2π|` band inside which a revolve angle is the full-turn branch
/// (the washer topology); `α > 2π + band` is rejected. Absorbs only the
/// degrees→radians conversion rounding of an exact 360°.
pub const REVOLVE_FULL_TURN_TOLERANCE: f64 = 1e-9;

/// Per-edge classification of an axis-aligned profile edge.
#[derive(Debug, Clone, Copy)]
enum EdgeClass {
    /// Constant radius: sweeps a cylinder wall. `reversed` = material on
    /// the larger-radius side (the wall of an inner bore).
    Parallel { radius: f64, reversed: bool },
    /// Constant axial height: sweeps a planar annular sector (partial) or
    /// vanishes into an annulus bounded by its endpoint rims (full turn).
    /// `outward_plus_axis` = the face's outward normal is `+â`.
    Perpendicular { outward_plus_axis: bool },
    /// Both radius and axial height change: sweeps a CONE (frustum) band
    /// (KV6c). Topologically a wall, exactly like [`EdgeClass::Parallel`] —
    /// two rim circles (at the edge's two radii) plus two seams — only the
    /// surface differs. The cone parameters are derived from the slant in
    /// [`validate_revolve_geometry`]: `apex` is where the slant, extended,
    /// meets the axis; `axis_dir` is oriented so both rims have a positive
    /// axial coordinate (apex behind); `half_angle = atan|Δs/Δt|`;
    /// `reversed` = material on the larger-radius side (an inner bore),
    /// the same `dt > 0` rule as `Parallel`. Only the full-turn builder
    /// handles it; a partial revolve of an oblique edge sweeps an
    /// arc-bounded cone patch (KV6c increment 5) and is rejected typed.
    Oblique {
        apex: Point3,
        axis_dir: UnitVector3,
        half_angle: f64,
        reversed: bool,
    },
}

/// Validated revolve geometry, computed before any arena mutation.
struct RevolveFrame {
    /// Unit axis direction.
    a: UnitVector3,
    /// Unit in-plane radial direction; every profile vertex has a strictly
    /// positive radial coordinate along it.
    w: UnitVector3,
    /// `â × ŵ` — the sweep-velocity direction at θ = 0 (`±` the profile
    /// normal). The working loop is ordered so its Newell normal is `+m`.
    m: UnitVector3,
    /// Axis origin.
    a0: Point3,
    /// Working-order outer loop, embedded (Newell normal `+m`).
    ring0: Vec<Point3>,
    /// Per-vertex axial coordinate `(p − a0) · â`.
    t: Vec<f64>,
    /// Per-vertex radial coordinate `(p − a0) · ŵ` (all > clearance).
    s: Vec<f64>,
    /// Per-edge classification (edge `i` joins vertex `i` to `i + 1`).
    edges: Vec<EdgeClass>,
}

/// Revolve a validated polygon [`Profile`] about an in-plane axis by
/// `angle_rad ∈ (0, 2π]` radians (PR-KV6a). See `tests/kv6a_revolve.rs`
/// for the pinned contract: geometry, topology census, Pappus volume,
/// rejection semantics. Like [`extrude_circle`], both branches are direct
/// assemblers (arcs / closed rims are outside the Euler-operator
/// vocabulary); the safety obligation is discharged by `validate_solid`
/// at exit.
pub fn revolve(
    arena: &mut BrepArena,
    profile: &Profile,
    axis_origin: Point3,
    axis_direction: Vector3,
    angle_rad: f64,
) -> Result<RevolveResult, KernelV2Error> {
    // ---- argument validation (ALL before the first mutation) -------------
    if !angle_rad.is_finite() || angle_rad <= 0.0 {
        return Err(KernelV2Error::RevolveInvalidAngle);
    }
    let two_pi = 2.0 * std::f64::consts::PI;
    let full_turn = (angle_rad - two_pi).abs() <= REVOLVE_FULL_TURN_TOLERANCE;
    if !full_turn && angle_rad > two_pi {
        return Err(KernelV2Error::RevolveInvalidAngle);
    }

    // A circle profile sweeps a TORUS (KV6d). Partial revolves build a bent
    // solid tube (2 disk caps + a toroidal lateral with longitude-arc seams);
    // the full-turn case (a closed genus-1 torus) is a later increment.
    if let ProfileRegion::Circle { center, radius } = profile.region() {
        if full_turn {
            return Err(KernelV2Error::RevolveCircleProfileUnsupported);
        }
        return build_torus_revolve(
            arena,
            profile,
            *center,
            *radius,
            axis_origin,
            axis_direction,
            angle_rad,
        );
    }

    let frame = match validate_revolve_geometry(profile, axis_origin, axis_direction) {
        // KV6 on-axis slice 1 (spec `specs/kv6_on_axis_revolve_rectangle.md`,
        // task #65): the clearance rejection conflates CROSSING (invalid
        // input) with TOUCHING (a legitimate solid of revolution). Recover
        // the full-turn single-on-axis-edge rectangle — the lathe shaft —
        // as the canonical solid cylinder; every other on-axis shape keeps
        // the typed error (C0063/C0064 cones/frusta are KV6c slice 2).
        Err(KernelV2Error::RevolveAxisIntersectsProfile) if full_turn => {
            return on_axis_revolve(arena, profile, axis_origin, axis_direction);
        }
        f => f?,
    };

    if full_turn {
        build_full_revolve(arena, &frame)
    } else {
        // A partial revolve of an oblique edge sweeps an ARC-bounded cone
        // patch, which neither the cone tessellation nor `validate_cone_face`
        // handles yet (KV6c increment 5). Reject before any mutation.
        if frame
            .edges
            .iter()
            .any(|e| matches!(e, EdgeClass::Oblique { .. }))
        {
            return Err(KernelV2Error::RevolveObliqueEdgeUnsupported);
        }
        build_partial_revolve(arena, &frame, angle_rad)
    }
}

/// All revolve input validation: axis in plane, polygon region without
/// holes, profile strictly on one side of the axis, every edge axis-aligned.
/// Pure — no arena access.
fn validate_revolve_geometry(
    profile: &Profile,
    axis_origin: Point3,
    axis_direction: Vector3,
) -> Result<RevolveFrame, KernelV2Error> {
    // Axis direction: finite, nonzero, in the profile plane.
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
    if (a.x * n.x + a.y * n.y + a.z * n.z).abs() > REVOLVE_AXIS_IN_PLANE_TOLERANCE {
        return Err(KernelV2Error::RevolveAxisNotInPlane);
    }
    // Axis origin on the profile plane (band scaled by the magnitudes).
    let o = profile.origin();
    let plane_dist = (axis_origin.x() - o.x()) * n.x
        + (axis_origin.y() - o.y()) * n.y
        + (axis_origin.z() - o.z()) * n.z;
    let mag = axis_origin
        .x()
        .abs()
        .max(axis_origin.y().abs())
        .max(axis_origin.z().abs())
        .max(o.x().abs())
        .max(o.y().abs())
        .max(o.z().abs());
    if plane_dist.abs() > REVOLVE_AXIS_IN_PLANE_TOLERANCE * (1.0 + mag) {
        return Err(KernelV2Error::RevolveAxisNotInPlane);
    }

    // Region: polygon, no holes (typed walls for the rest).
    let outer = match profile.region() {
        ProfileRegion::Circle { .. } => {
            return Err(KernelV2Error::RevolveCircleProfileUnsupported);
        }
        ProfileRegion::Polygon { holes, .. } if !holes.is_empty() => {
            return Err(KernelV2Error::RevolveProfileHolesUnsupported);
        }
        ProfileRegion::ArcPolygon { .. } => {
            return Err(KernelV2Error::ArcPolygonProfileUnsupported);
        }
        ProfileRegion::Polygon { outer, .. } => outer,
    };

    // Radial direction: ŵ = ±normalize(n̂ × â), signed so the profile's
    // radial coordinates come out positive. m̂ = â × ŵ = ±n̂ is then the
    // sweep-velocity direction; the working loop is reordered so its
    // Newell normal is +m̂ (the extrude `reverse` flag, revolve edition).
    let wx = [
        n.y * a.z - n.z * a.y,
        n.z * a.x - n.x * a.z,
        n.x * a.y - n.y * a.x,
    ];
    let w_len = (wx[0] * wx[0] + wx[1] * wx[1] + wx[2] * wx[2]).sqrt();
    // |n̂ × â| = 1 up to rounding (â ⊥ n̂ just verified).
    let mut w = UnitVector3 {
        x: wx[0] / w_len,
        y: wx[1] / w_len,
        z: wx[2] / w_len,
    };

    let embedded: Vec<Point3> = outer.iter().map(|&p| profile.embed(p)).collect();
    let radial = |p: &Point3, w: &UnitVector3| {
        (p.x() - axis_origin.x()) * w.x
            + (p.y() - axis_origin.y()) * w.y
            + (p.z() - axis_origin.z()) * w.z
    };
    let s_sum: f64 = embedded.iter().map(|p| radial(p, &w)).sum();
    let flip_w = s_sum < 0.0;
    if flip_w {
        w = UnitVector3 {
            x: -w.x,
            y: -w.y,
            z: -w.z,
        };
    }
    // m̂ = â × ŵ (exactly ±n̂; recompute for numerical hygiene).
    let m = UnitVector3 {
        x: a.y * w.z - a.z * w.y,
        y: a.z * w.x - a.x * w.z,
        z: a.x * w.y - a.y * w.x,
    };

    // Working order: stored loops are CCW around n̂; the construction wants
    // Newell ≡ +m̂. m̂ = +n̂ exactly when ŵ was not flipped.
    let ring0: Vec<Point3> = if flip_w {
        embedded.into_iter().rev().collect()
    } else {
        embedded
    };

    // Per-vertex axis coordinates + strict one-side clearance.
    let mut t = Vec::with_capacity(ring0.len());
    let mut s = Vec::with_capacity(ring0.len());
    let mut scale = 0.0f64;
    for p in &ring0 {
        let dx = [
            p.x() - axis_origin.x(),
            p.y() - axis_origin.y(),
            p.z() - axis_origin.z(),
        ];
        t.push(dx[0] * a.x + dx[1] * a.y + dx[2] * a.z);
        s.push(dx[0] * w.x + dx[1] * w.y + dx[2] * w.z);
        scale = scale.max(dx[0].abs()).max(dx[1].abs()).max(dx[2].abs());
    }
    let clearance = REVOLVE_MIN_AXIS_CLEARANCE_REL * (1.0 + scale);
    if s.iter().any(|&si| si <= clearance) {
        // Mixed signs = crossing; near-zero = touching. Both invalid input.
        // (Straight in-plane edges between positive-radius vertices cannot
        // dip below their endpoint minimum, so the vertex check is
        // sufficient for the polygon.)
        return Err(KernelV2Error::RevolveAxisIntersectsProfile);
    }

    // Edge classification (working order; edge i joins i → i+1).
    let k = ring0.len();
    let mut edges = Vec::with_capacity(k);
    for i in 0..k {
        let j = (i + 1) % k;
        let dt = t[j] - t[i];
        let ds = s[j] - s[i];
        let len = (dt * dt + ds * ds).sqrt();
        if ds.abs() <= REVOLVE_EDGE_ALIGNMENT_TOLERANCE * len {
            // Material lies LEFT of the working-CCW edge: +ŝ for dt > 0 —
            // that face's outward normal points toward the axis (an inner
            // bore wall), the cavity sense.
            edges.push(EdgeClass::Parallel {
                radius: s[i],
                reversed: dt > 0.0,
            });
        } else if dt.abs() <= REVOLVE_EDGE_ALIGNMENT_TOLERANCE * len {
            // Outward normal +â exactly when the material is on the −â
            // side, i.e. when the working-CCW edge runs radially outward.
            edges.push(EdgeClass::Perpendicular {
                outward_plus_axis: ds > 0.0,
            });
        } else {
            // Oblique edge → cone frustum band (KV6c). The slant, extended,
            // meets the axis at `t_apex` (where s = 0): from s = s[i] +
            // (t − t[i])·ds/dt, set s = 0. `half_angle = atan|ds/dt|`; orient
            // `axis_dir` toward increasing radius so both rims have τ > 0
            // (apex behind). `reversed = dt > 0`, the same material-sense rule
            // as `Parallel` (the cone's default outward normal points away
            // from the axis; `reversed` flips it toward the axis for a bore).
            let t_apex = t[i] - s[i] * dt / ds;
            let apex = Point3::new(
                axis_origin.x() + t_apex * a.x,
                axis_origin.y() + t_apex * a.y,
                axis_origin.z() + t_apex * a.z,
            );
            let axis_dir = if (ds > 0.0) == (dt > 0.0) { a } else { neg(a) };
            edges.push(EdgeClass::Oblique {
                apex,
                axis_dir,
                half_angle: (ds / dt).abs().atan(),
                reversed: dt > 0.0,
            });
        }
    }

    Ok(RevolveFrame {
        a,
        w,
        m,
        a0: axis_origin,
        ring0,
        t,
        s,
        edges,
    })
}

/// Snap a trig value to exactly 0 / ±1 when within 1e-12 of it. At the
/// quadrant angles (90°, 180°, 270°, 360°) `sin`/`cos` carry ~1e-16
/// residue; snapping makes a 180° revolve's two caps carry BIT-IDENTICAL
/// planes — which yang's intra-solid near-coplanar gate excludes as the
/// benign "one plane, several faces" class. Without the snap the caps
/// differ by 1 ulp and the gate (correctly conservative for the femto-seam
/// hazard class) would defer every boolean near them.
fn snap_trig(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        0.0
    } else if (x - 1.0).abs() < 1e-12 {
        1.0
    } else if (x + 1.0).abs() < 1e-12 {
        -1.0
    } else {
        x
    }
}

impl RevolveFrame {
    /// Rotate a profile point by `theta` about the axis: the point's
    /// in-plane decomposition is `a0 + t·â + s·ŵ`, which maps to
    /// `a0 + t·â + s·(cos θ·ŵ + sin θ·m̂)`. Trig snapped at the quadrant
    /// angles (see [`snap_trig`]).
    fn rotate(&self, i: usize, theta: f64) -> Point3 {
        let (c, sn) = (snap_trig(theta.cos()), snap_trig(theta.sin()));
        let radial = self.s[i];
        Point3::new(
            self.a0.x() + self.t[i] * self.a.x + radial * (c * self.w.x + sn * self.m.x),
            self.a0.y() + self.t[i] * self.a.y + radial * (c * self.w.y + sn * self.m.y),
            self.a0.z() + self.t[i] * self.a.z + radial * (c * self.w.z + sn * self.m.z),
        )
    }

    /// Foot of the axis perpendicular through vertex `i` (= arc center).
    fn axis_foot(&self, i: usize) -> Point3 {
        Point3::new(
            self.a0.x() + self.t[i] * self.a.x,
            self.a0.y() + self.t[i] * self.a.y,
            self.a0.z() + self.t[i] * self.a.z,
        )
    }
}

/// Partial-angle branch: caps + one wall per profile edge, sweep arcs
/// between the θ=0 and θ=α vertex rings. Topology (k = edge count):
/// V = 2k, E = 3k (k cap segments ×2 + k arcs), F = k + 2, χ = 2.
fn build_partial_revolve(
    arena: &mut BrepArena,
    fr: &RevolveFrame,
    angle: f64,
) -> Result<RevolveResult, KernelV2Error> {
    let k = fr.ring0.len();
    let neg = |u: UnitVector3| UnitVector3 {
        x: -u.x,
        y: -u.y,
        z: -u.z,
    };

    // ---- vertices: ring 0 (working order) + ring α -------------------------
    let vb = arena.vertices.len() as u32;
    for p in &fr.ring0 {
        arena.vertices.push(Some(Vertex { point: *p }));
    }
    for i in 0..k {
        arena.vertices.push(Some(Vertex {
            point: fr.rotate(i, angle),
        }));
    }
    let v0 = |i: usize| VertexId(vb + (i % k) as u32);
    let v1 = |i: usize| VertexId(vb + k as u32 + (i % k) as u32);

    // ---- half-edge id layout (6 per edge index i) ---------------------------
    // sc[i]: start cap, ring0[i+1] → ring0[i]   (cap winds CCW around −m̂)
    // ec[i]: end cap,   ring1[i]   → ring1[i+1] (cap winds CCW around rot(+m̂))
    // wb[i]: wall i bottom, ring0[i] → ring0[i+1] (twin sc[i])
    // wt[i]: wall i top,    ring1[i+1] → ring1[i] (twin ec[i])
    // af[i]: forward sweep arc at vertex i, ring0[i] → ring1[i], normal +â
    //        (lives in wall (i−1+k)%k's loop)
    // ab[i]: backward arc at vertex i, ring1[i] → ring0[i], normal −â
    //        (twin af[i]; lives in wall i's loop)
    let hb = arena.half_edges.len() as u32;
    let sc = |i: usize| HalfEdgeId(hb + 6 * ((i % k) as u32));
    let ec = |i: usize| HalfEdgeId(hb + 6 * ((i % k) as u32) + 1);
    let wb = |i: usize| HalfEdgeId(hb + 6 * ((i % k) as u32) + 2);
    let wt = |i: usize| HalfEdgeId(hb + 6 * ((i % k) as u32) + 3);
    let af = |i: usize| HalfEdgeId(hb + 6 * ((i % k) as u32) + 4);
    let ab = |i: usize| HalfEdgeId(hb + 6 * ((i % k) as u32) + 5);

    let lb = arena.loops.len() as u32;
    let loop_start = LoopId(lb);
    let loop_end = LoopId(lb + 1);
    let loop_wall = |i: usize| LoopId(lb + 2 + (i % k) as u32);
    let fb = arena.faces.len() as u32;
    let f_start = FaceId(fb);
    let f_end = FaceId(fb + 1);
    let f_wall = |i: usize| FaceId(fb + 2 + (i % k) as u32);
    let shell = ShellId(arena.shells.len() as u32);
    let solid = SolidId(arena.solids.len() as u32);

    for i in 0..k {
        let arc_curve = |normal: UnitVector3| Curve::Arc {
            center: fr.axis_foot(i),
            normal,
            radius: fr.s[i],
        };
        // sc[i]: origin ring0[i+1]; cap cycle visits vertices in reverse,
        // so next(sc[i]) starts at ring0[i] — that is sc[i−1].
        arena.half_edges.push(Some(HalfEdge {
            twin: wb(i),
            next: sc(i + k - 1),
            prev: sc(i + 1),
            origin: v0(i + 1),
            loop_id: loop_start,
            curve: Curve::LineSegment,
        }));
        // ec[i]: origin ring1[i]; forward cycle.
        arena.half_edges.push(Some(HalfEdge {
            twin: wt(i),
            next: ec(i + 1),
            prev: ec(i + k - 1),
            origin: v1(i),
            loop_id: loop_end,
            curve: Curve::LineSegment,
        }));
        // Wall i cycle: wb[i] → af[i+1] → wt[i] → ab[i] → wb[i].
        arena.half_edges.push(Some(HalfEdge {
            twin: sc(i),
            next: af(i + 1),
            prev: ab(i),
            origin: v0(i),
            loop_id: loop_wall(i),
            curve: Curve::LineSegment,
        }));
        arena.half_edges.push(Some(HalfEdge {
            twin: ec(i),
            next: ab(i),
            prev: af(i + 1),
            origin: v1(i + 1),
            loop_id: loop_wall(i),
            curve: Curve::LineSegment,
        }));
        // af[i] lives in wall (i−1)'s loop: prev = wb[i−1], next = wt[i−1].
        arena.half_edges.push(Some(HalfEdge {
            twin: ab(i),
            next: wt(i + k - 1),
            prev: wb(i + k - 1),
            origin: v0(i),
            loop_id: loop_wall(i + k - 1),
            curve: arc_curve(fr.a),
        }));
        arena.half_edges.push(Some(HalfEdge {
            twin: af(i),
            next: wb(i),
            prev: wt(i),
            origin: v1(i),
            loop_id: loop_wall(i),
            curve: arc_curve(neg(fr.a)),
        }));
    }

    // ---- loops, faces ------------------------------------------------------
    arena.loops.push(Some(Loop {
        face: f_start,
        boundary: LoopBoundary::Edges(sc(0)),
        kind: LoopKind::Outer,
    }));
    arena.loops.push(Some(Loop {
        face: f_end,
        boundary: LoopBoundary::Edges(ec(0)),
        kind: LoopKind::Outer,
    }));
    for i in 0..k {
        arena.loops.push(Some(Loop {
            face: f_wall(i),
            boundary: LoopBoundary::Edges(wb(i)),
            kind: LoopKind::Outer,
        }));
    }

    // Start cap: outward normal −m̂ (opposes the sweep velocity); end cap:
    // +m̂ rotated by the sweep angle = cos α·m̂ − sin α·ŵ.
    arena.faces.push(Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: fr.ring0[0],
            normal: neg(fr.m),
        })),
        outer_loop: loop_start,
        inner_loops: Vec::new(),
        shell,
    }));
    let (ca, sa) = (snap_trig(angle.cos()), snap_trig(angle.sin()));
    arena.faces.push(Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: fr.rotate(0, angle),
            normal: UnitVector3 {
                x: ca * fr.m.x - sa * fr.w.x,
                y: ca * fr.m.y - sa * fr.w.y,
                z: ca * fr.m.z - sa * fr.w.z,
            },
        })),
        outer_loop: loop_end,
        inner_loops: Vec::new(),
        shell,
    }));
    let mut walls = Vec::with_capacity(k);
    for (i, cls) in fr.edges.iter().enumerate() {
        let surface = match *cls {
            EdgeClass::Parallel { radius, reversed } => Surface::Cylinder {
                axis_point: fr.a0,
                axis_dir: fr.a,
                radius,
                reversed,
            },
            EdgeClass::Perpendicular { outward_plus_axis } => Surface::Plane(Plane {
                point: fr.ring0[i],
                normal: if outward_plus_axis { fr.a } else { neg(fr.a) },
            }),
            // Oblique edges are rejected before `build_partial_revolve` is
            // called (see `revolve`); this arm keeps the match exhaustive.
            EdgeClass::Oblique { .. } => return Err(KernelV2Error::RevolveObliqueEdgeUnsupported),
        };
        arena.faces.push(Some(Face {
            surface: Some(surface),
            outer_loop: loop_wall(i),
            inner_loops: Vec::new(),
            shell,
        }));
        walls.push(f_wall(i));
    }

    let mut shell_faces = vec![f_start, f_end];
    shell_faces.extend(walls.iter().copied());
    arena.shells.push(Some(Shell {
        solid,
        faces: shell_faces,
        genus: 0,
    }));
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));

    finalize_solid(arena, solid)?;
    Ok(RevolveResult {
        solid,
        shell,
        start_cap: f_start,
        end_cap: f_end,
        walls,
    })
}

/// KV6 on-axis recovery (slice 1 `specs/kv6_on_axis_revolve_rectangle.md`,
/// task #65; slice 2 `specs/kv6_on_axis_revolve_oblique.md`, task #66):
/// full-turn revolve of a 4-gon with exactly ONE on-axis edge — the lathe
/// shaft family. The on-axis edge sweeps the rotation's fixed line
/// (degenerate, interior to the solid); its adjacent perpendicular edges
/// sweep full DISCS; the off-axis edge sweeps the lateral:
///
/// - axis-PARALLEL off-axis edge (slice 1): EXACTLY the canonical KV5a
///   cylinder — DELEGATES to `extrude` of a synthesized cap-plane circle
///   profile, bit-canonical with extrude-of-circle, no new topology code.
/// - OBLIQUE off-axis edge (slice 2 increment A, the C0064 class): the
///   SOLID FRUSTUM — same census with a `Surface::Cone` lateral, built by
///   [`build_on_axis_frustum`].
///
/// Reached only where `validate_revolve_geometry` returned
/// `RevolveAxisIntersectsProfile` (axis finite + in-plane and the region a
/// hole-free polygon are already verified — the clearance check is the
/// LAST rejection in that function). Everything that is not a slice-1/2
/// shape — crossing profiles, the on-axis apex triangle (C0063, slice 2
/// increment B), pencil quads, degenerate rectangles — returns the
/// ORIGINAL typed error, pre-mutation.
fn on_axis_revolve(
    arena: &mut BrepArena,
    profile: &Profile,
    axis_origin: Point3,
    axis_direction: Vector3,
) -> Result<RevolveResult, KernelV2Error> {
    const REJECT: KernelV2Error = KernelV2Error::RevolveAxisIntersectsProfile;
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

    // ---- slice-1 shape: 4-gon, exactly one on-axis edge -------------------
    if embedded.len() != 4 {
        return Err(REJECT);
    }
    let on = |i: usize| s[i].abs() <= clearance;
    let on_idx: Vec<usize> = (0..4).filter(|&i| on(i)).collect();
    if on_idx.len() != 2 {
        return Err(REJECT);
    }
    let adjacent = on_idx[1] == on_idx[0] + 1 || (on_idx[0] == 0 && on_idx[1] == 3);
    if !adjacent {
        return Err(REJECT);
    }
    let off_idx: Vec<usize> = (0..4).filter(|&i| !on(i)).collect();
    let band = REVOLVE_EDGE_ALIGNMENT_TOLERANCE * (1.0 + scale);
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
        start_cap: ex.base,
        end_cap: ex.top,
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
        start_cap: f_base,
        end_cap: f_top,
        walls: vec![f_lat],
    })
}

/// Full-turn branch: the washer. Perpendicular profile edges become
/// seamless annuli (outer circle loop + circle ring); parallel edges
/// become canonical full cylinders (2 rims + a seam at θ = 0, the KV5a
/// lateral shape). Rectangle: V=4, E=6 (4 rims + 2 seams), F=4, R=2,
/// G=1 ⇒ χ = 0 = 2(S − G).
///
/// KV6a builds the alternating case (every vertex joins one parallel and
/// one perpendicular edge — every rectangle, the whole corpus). A
/// rectilinear profile with consecutive same-class edges is typed
/// [`KernelV2Error::NotImplemented`] rather than guessed.
fn build_full_revolve(
    arena: &mut BrepArena,
    fr: &RevolveFrame,
) -> Result<RevolveResult, KernelV2Error> {
    let k = fr.ring0.len();
    let neg = |u: UnitVector3| UnitVector3 {
        x: -u.x,
        y: -u.y,
        z: -u.z,
    };
    // Walls (Parallel cylinders and Oblique cones) must alternate with
    // Perpendicular annuli: each shared vertex pairs one wall rim with one
    // annulus rim (the twin structure). Two consecutive walls — or two
    // consecutive annuli — break that pairing.
    let is_wall =
        |c: EdgeClass| matches!(c, EdgeClass::Parallel { .. } | EdgeClass::Oblique { .. });
    for i in 0..k {
        if is_wall(fr.edges[i]) == is_wall(fr.edges[(i + 1) % k]) {
            return Err(KernelV2Error::NotImplemented(
                "PR-KV6a full-turn revolve of non-alternating profiles",
            ));
        }
    }

    // ---- vertices: the θ=0 ring only (every rim circle is anchored there) --
    let vb = arena.vertices.len() as u32;
    for p in &fr.ring0 {
        arena.vertices.push(Some(Vertex { point: *p }));
    }
    let v = |i: usize| VertexId(vb + (i % k) as u32);

    // ---- half-edge layout (4 per edge index) --------------------------------
    // Per PARALLEL edge i (cylinder wall, vertices i and i+1):
    //   rim_w[i]   : rim circle at vertex i, in the wall loop
    //   seam_f[i]  : seam ring0[i] → ring0[i+1], in the wall loop
    //   rim_w2[i]  : rim circle at vertex i+1, in the wall loop
    //   seam_b[i]  : seam ring0[i+1] → ring0[i], in the wall loop (twin of f)
    //   wall cycle: rim_w[i] → seam_f[i] → rim_w2[i] → seam_b[i] → rim_w[i]
    // Per PERPENDICULAR edge j (annulus, vertices j and j+1): two closed
    //   circle half-edges, ann_o[j] (outer loop, at the larger-radius
    //   vertex) and ann_r[j] (ring, at the smaller-radius vertex); they twin
    //   with the adjacent walls' rim half-edges at the same vertices.
    let hb = arena.half_edges.len() as u32;
    let he = |i: usize, slot: u32| HalfEdgeId(hb + 4 * ((i % k) as u32) + slot);

    let lb = arena.loops.len() as u32;
    let fb = arena.faces.len() as u32;
    let shell = ShellId(arena.shells.len() as u32);
    let solid = SolidId(arena.solids.len() as u32);

    // Loop/face layout: one outer loop per edge face; annuli get one ring
    // loop each (allocated after the k outer loops).
    let outer_loop = |i: usize| LoopId(lb + (i % k) as u32);
    let face_of = |i: usize| FaceId(fb + (i % k) as u32);

    // Rim circle half-edge ids at a VERTEX: vertex i is shared by edge
    // (i−1) and edge i; exactly one of them is parallel (the wall side) and
    // the other perpendicular (the annulus side) — the alternating
    // guarantee. Wall-side rim at vertex i: slot 0 if the wall is edge i
    // (rim_w), slot 2 if the wall is edge i−1 (rim_w2). Annulus-side rim:
    // slot 0/2 on the perpendicular edge analogously.
    let rim_on_edge = |edge: usize, vertex: usize| -> HalfEdgeId {
        if vertex % k == edge % k {
            he(edge, 0)
        } else {
            he(edge, 2)
        }
    };

    let mut ring_loops: Vec<(usize, LoopId)> = Vec::new(); // (edge idx, ring loop)
    let mut next_ring = lb + k as u32;
    for (i, cls) in fr.edges.iter().enumerate() {
        if matches!(cls, EdgeClass::Perpendicular { .. }) {
            ring_loops.push((i, LoopId(next_ring)));
            next_ring += 1;
        }
    }
    let ring_loop_of = |edge: usize, ring_loops: &[(usize, LoopId)]| -> LoopId {
        ring_loops
            .iter()
            .find(|(e, _)| *e == edge)
            .map(|(_, l)| *l)
            .expect("perpendicular edge has a ring loop")
    };

    // ---- emit half-edges (4 dense slots per edge; perpendicular edges use
    //      slots 0/2 and leave 1/3 as dead `None` slots so the id arithmetic
    //      stays uniform) ----------------------------------------------------
    for (i, cls) in fr.edges.iter().enumerate() {
        let j = (i + 1) % k;
        match *cls {
            EdgeClass::Parallel { reversed, .. } | EdgeClass::Oblique { reversed, .. } => {
                // Rim normals: for an outward wall the rim's traversal axis
                // points TOWARD the opposite rim (the KV5a canonical rule);
                // for a reversed (inner-bore) wall it points AWAY — the
                // mirrored material sense, forced by the twin structure
                // (each rim twin lives in an adjacent annulus whose
                // outer/ring winding rules fix the sign).
                let toward_j = if fr.t[j] >= fr.t[i] { fr.a } else { neg(fr.a) };
                let (n_i, n_j) = if reversed {
                    (neg(toward_j), toward_j)
                } else {
                    (toward_j, neg(toward_j))
                };
                let rim = |vi: usize, normal: UnitVector3| Curve::Circle {
                    center: fr.axis_foot(vi),
                    normal,
                    radius: fr.s[vi],
                };
                // Wall cycle: rim(i) → seam i→j → rim(j) → seam j→i.
                arena.half_edges.push(Some(HalfEdge {
                    twin: rim_on_edge((i + k - 1) % k, i),
                    next: he(i, 1),
                    prev: he(i, 3),
                    origin: v(i),
                    loop_id: outer_loop(i),
                    curve: rim(i, n_i),
                }));
                arena.half_edges.push(Some(HalfEdge {
                    twin: he(i, 3),
                    next: he(i, 2),
                    prev: he(i, 0),
                    origin: v(i),
                    loop_id: outer_loop(i),
                    curve: Curve::LineSegment,
                }));
                arena.half_edges.push(Some(HalfEdge {
                    twin: rim_on_edge(j, j),
                    next: he(i, 3),
                    prev: he(i, 1),
                    origin: v(j),
                    loop_id: outer_loop(i),
                    curve: rim(j, n_j),
                }));
                arena.half_edges.push(Some(HalfEdge {
                    twin: he(i, 1),
                    next: he(i, 0),
                    prev: he(i, 2),
                    origin: v(j),
                    loop_id: outer_loop(i),
                    curve: Curve::LineSegment,
                }));
            }
            EdgeClass::Perpendicular { outward_plus_axis } => {
                // Annulus face normal ±â; the outer circle (larger radius)
                // traverses CCW around the face normal, the ring circle CCW
                // around its negation. Each twins with the wall-side rim at
                // the same vertex.
                let normal = if outward_plus_axis { fr.a } else { neg(fr.a) };
                let vo = if fr.s[i] >= fr.s[j] { i } else { j };
                let ring_l = ring_loop_of(i, &ring_loops);
                for (slot, vi) in [(0u32, i), (2u32, j)] {
                    let is_outer = vi == vo;
                    let nu = if is_outer { normal } else { neg(normal) };
                    let lid = if is_outer { outer_loop(i) } else { ring_l };
                    let hid = he(i, slot);
                    let other_edge = if vi == i { (i + k - 1) % k } else { j };
                    arena.half_edges.push(Some(HalfEdge {
                        twin: rim_on_edge(other_edge, vi),
                        next: hid,
                        prev: hid,
                        origin: v(vi),
                        loop_id: lid,
                        curve: Curve::Circle {
                            center: fr.axis_foot(vi),
                            normal: nu,
                            radius: fr.s[vi],
                        },
                    }));
                    if slot == 0 {
                        arena.half_edges.push(None); // dead slot 1
                    }
                }
                arena.half_edges.push(None); // dead slot 3
            }
        }
    }

    // ---- loops --------------------------------------------------------------
    // k outer loops (edge order), then one ring loop per perpendicular edge
    // (the order ring_loops was collected in).
    for (i, cls) in fr.edges.iter().enumerate() {
        let j = (i + 1) % k;
        let boundary = match *cls {
            EdgeClass::Parallel { .. } | EdgeClass::Oblique { .. } => he(i, 0),
            EdgeClass::Perpendicular { .. } => {
                let vo = if fr.s[i] >= fr.s[j] { i } else { j };
                he(i, if vo == i { 0 } else { 2 })
            }
        };
        arena.loops.push(Some(Loop {
            face: face_of(i),
            boundary: LoopBoundary::Edges(boundary),
            kind: LoopKind::Outer,
        }));
    }
    for &(i, _lid) in &ring_loops {
        let j = (i + 1) % k;
        let vr = if fr.s[i] >= fr.s[j] { j } else { i };
        arena.loops.push(Some(Loop {
            face: face_of(i),
            boundary: LoopBoundary::Edges(he(i, if vr == i { 0 } else { 2 })),
            kind: LoopKind::Inner,
        }));
    }

    // ---- faces ----------------------------------------------------------------
    let mut start_cap = None; // perpendicular face at the axial minimum
    let mut end_cap = None;
    let mut walls = Vec::new();
    for (i, cls) in fr.edges.iter().enumerate() {
        let surface = match *cls {
            EdgeClass::Parallel { radius, reversed } => Surface::Cylinder {
                axis_point: fr.a0,
                axis_dir: fr.a,
                radius,
                reversed,
            },
            EdgeClass::Perpendicular { outward_plus_axis } => Surface::Plane(Plane {
                point: fr.ring0[i],
                normal: if outward_plus_axis { fr.a } else { neg(fr.a) },
            }),
            EdgeClass::Oblique {
                apex,
                axis_dir,
                half_angle,
                reversed,
            } => Surface::Cone {
                apex,
                axis_dir,
                half_angle,
                reversed,
            },
        };
        let inner: Vec<LoopId> = ring_loops
            .iter()
            .filter(|(e, _)| *e == i)
            .map(|(_, l)| *l)
            .collect();
        arena.faces.push(Some(Face {
            surface: Some(surface),
            outer_loop: outer_loop(i),
            inner_loops: inner,
            shell,
        }));
        match *cls {
            EdgeClass::Perpendicular { outward_plus_axis } => {
                // The −â annulus is the start cap, the +â one the end cap
                // (extremes for the rectangle; extra annuli of a staircase
                // would join `walls`, but non-alternating profiles were
                // rejected above so each class appears alternately).
                if !outward_plus_axis && start_cap.is_none() {
                    start_cap = Some(face_of(i));
                } else if outward_plus_axis && end_cap.is_none() {
                    end_cap = Some(face_of(i));
                } else {
                    walls.push(face_of(i));
                }
            }
            EdgeClass::Parallel { .. } | EdgeClass::Oblique { .. } => walls.push(face_of(i)),
        }
    }
    let (Some(start_cap), Some(end_cap)) = (start_cap, end_cap) else {
        // A closed rectilinear profile strictly off the axis always has
        // both a +â and a −â perpendicular extreme.
        return Err(KernelV2Error::NotImplemented(
            "PR-KV6a full-turn revolve without two opposite annular caps",
        ));
    };

    arena.shells.push(Some(Shell {
        solid,
        faces: (0..k).map(face_of).collect(),
        genus: 1,
    }));
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));

    finalize_solid(arena, solid)?;
    Ok(RevolveResult {
        solid,
        shell,
        start_cap,
        end_cap,
        walls,
    })
}

/// Revolve a circle [`Profile`] by `angle ∈ (0, 2π)` into a PARTIAL torus
/// (KV6d): a bent solid tube. Topology mirrors [`extrude_circle`] — 2 seam
/// vertices, 6 half-edges, 3 faces — but the caps are the profile disks at
/// `θ = 0` and `θ = α` (meridian planes), the rims are the profile circles
/// (minor radius), the seams are longitude ARCS (the φ = 0 latitude, radius
/// `R + r`), and the lateral is a [`Surface::Torus`]. Direct assembler; the
/// safety obligation is discharged by `validate_solid` at exit.
#[allow(clippy::too_many_arguments)]
fn build_torus_revolve(
    arena: &mut BrepArena,
    profile: &Profile,
    center2d: Point2,
    minor_radius: f64,
    axis_origin: Point3,
    axis_direction: Vector3,
    angle: f64,
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
        start_cap: f_base,
        end_cap: f_top,
        walls: vec![f_lat],
    })
}

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
