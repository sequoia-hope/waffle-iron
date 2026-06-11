//! Planar primitive constructors over the Euler-operator arena (PR-KV2).
//!
//! Two constructors, both pure Euler-operator sequences (no raw arena
//! mutation), both validated loudly:
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

use crate::arena::{BrepArena, FaceId, HalfEdgeId, LoopId, ShellId, SolidId};
use crate::error::KernelV2Error;
use crate::euler::{kemr, kfmrh, mef, mev, mev_lone, mvfs};
use crate::profile::{cross, Profile, ProfileRegion};
use crate::validate::validate_solid;
use cad_primitives::{Point2, Point3, Vector3};

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
    let ProfileRegion::Polygon { outer, holes } = profile.region() else {
        // A circular lamina (zero-height disk sheet) has no consumer; the
        // circle vocabulary exists for `extrude` → cylinder (PR-KV5a).
        return Err(KernelV2Error::NotImplemented(
            "make_face_from_profile on a circle profile (no consumer; use extrude)",
        ));
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

    validate_solid(arena, core.solid)?;
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

    validate_solid(arena, core.solid)?;
    Ok(ExtrudeResult {
        solid: core.solid,
        shell: core.shell,
        base: core.back,
        top: core.front,
        walls,
        hole_walls,
    })
}

/// Extrude a circle profile into a right circular cylinder (PR-KV5a).
///
/// Direct arena assembler (see module docs, "The cylinder assembler") —
/// stubbed in the RED commit.
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
    let _ = (arena, profile, center, radius, d, d_len, cosine, distance);
    Err(KernelV2Error::NotImplemented("PR-KV5a extrude_circle"))
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
