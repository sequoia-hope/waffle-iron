//! Production validation suite for solids built in the B-Rep arena.
//!
//! `validate_solid` checks the full invariant set on everything reachable
//! from one solid:
//!
//! 1. **Twin pairing** — every half-edge has a live twin, `twin(twin) == h`,
//!    `twin != h`, twins run between the same two vertices in opposite
//!    directions (`origin(twin(h)) == origin(next(h))`), and both halves of
//!    every edge are reachable from the solid. With pairing intact, "each
//!    edge has exactly 2 half-edges" holds structurally and the edge count
//!    is `half-edges / 2`.
//! 2. **Loop closure** — every loop's `next` orbit returns to its
//!    representative, `prev` is the inverse of `next`, and every member
//!    points back at the loop.
//! 3. **Vertex manifoldness** — the half-edges leaving each vertex form a
//!    single radial fan under the orbit `h ↦ next(twin(h))` (2-manifold
//!    vertex condition).
//! 4. **Surface presence + on-surface geometry (debug builds only)** —
//!    every face carries `Some(Surface)`; in debug builds every loop vertex
//!    lies within [`PLANARITY_DEBUG_TOLERANCE`] of a planar face's plane /
//!    within [`CURVED_SURFACE_DEBUG_TOLERANCE`] of a cylinder face's
//!    surface (plus rim-center-on-axis, anchor-on-circle, seam-is-ruling).
//! 5. **Orientation consistency (the Newell invariant and its curved
//!    analog)** — a planar face with polygonal loops: the stored unit
//!    normal matches the normalized Newell normal of its outer loop, and
//!    every inner loop (ring) winds opposite. A planar face bounded by a
//!    full-circle edge (PR-KV5a): the circle half-edge's directional normal
//!    equals the face normal (ring circles: its negation). A cylinder face:
//!    exactly two full-circle rims whose normals run along the axis, whose
//!    radii equal the surface radius, and whose traversal axes each point
//!    toward the opposite rim (outward lateral orientation); twin curve
//!    pairing holds for every edge ([`KernelV2Error::CurveTwinMismatch`]).
//! 6. **Euler–Poincaré bookkeeping** — `V − E + F − R = 2(S − G)`
//!    (Stroud 2006 §4 rule 4, with his h/b written R/S).
//!
//! ## The planarity boundary (documented per the KV1 mandate)
//!
//! kernel-v2 may not depend on `cherchi-rs`, so no exact coplanarity
//! predicate is available in this crate. That is acceptable for KV1 because
//! planarity of constructed faces is **guaranteed by construction**: the
//! Euler operators create geometry only at caller-supplied points, and the
//! KV2 consumers (planar profile → extrude) supply coplanar points by
//! construction. The f64 residual check below is therefore a *debug-tier
//! tripwire for construction-sequence bugs*, compiled only under
//! `debug_assertions`, and is **not** a production correctness gate —
//! production correctness for planarity rests on the constructors, and any
//! future need for exact coplanarity decisions (e.g. boolean input
//! validation) belongs to the yang-rs/cherchi-rs layers.

use std::collections::{BTreeMap, BTreeSet};

use crate::arena::{
    BrepArena, Curve, FaceId, HalfEdgeId, LoopBoundary, LoopId, LoopKind, SolidId, Surface,
    VertexId,
};
use crate::error::KernelV2Error;
use crate::geom;
use cad_primitives::Point3;

/// Debug-tier planarity tripwire: maximum |signed distance| (meters) of a
/// loop vertex from its face plane. Strict — far below `TAU_MODEL` (1e-7 m)
/// and chosen so that exactly-constructed prismatic solids (residual 0.0)
/// pass with nine orders of magnitude to spare while genuinely non-planar
/// loops (feature size ≥ `MIN_FEATURE_SIZE` = 1e-6 m) fail loudly.
/// See the module docs for why this is not a production gate.
pub const PLANARITY_DEBUG_TOLERANCE: f64 = 1e-12;

/// Tolerance on `1 − dot(stored_normal, newell_unit)` for invariant 5.
/// Both vectors are unit-length f64; agreement is by construction, so this
/// only absorbs normalization rounding.
pub const NORMAL_AGREEMENT_TOLERANCE: f64 = 1e-9;

/// Debug-tier curved-surface tripwire (PR-KV5a): maximum |distance defect|
/// (meters) of curved-face geometry — loop vertices off the cylinder
/// surface (`|dist(p, axis) − r|`), rim circle centers off the axis, rim
/// anchors off the circle, seam segments off the ruling direction. Same
/// value and same rationale as [`PLANARITY_DEBUG_TOLERANCE`]: curved
/// geometry is exact **by construction** (the assembler places rim anchors
/// at `center + r·û`), so this is a construction-bug tripwire compiled only
/// under `debug_assertions`, not a production gate. Production-tier curved
/// checks are the ORIENTATION/consistency rules (the Newell analog): curve
/// twin pairing, rim count, rim normal direction, radius agreement.
pub const CURVED_SURFACE_DEBUG_TOLERANCE: f64 = 1e-12;

/// Element counts and bookkeeping for a validated solid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopologyReport {
    /// Vertices reachable from the solid.
    pub vertices: usize,
    /// Edges (half-edge pairs).
    pub edges: usize,
    /// Faces.
    pub faces: usize,
    /// Inner loops (rings / hole-loops).
    pub rings: usize,
    /// Shells.
    pub shells: usize,
    /// Total genus across the solid's shells.
    pub genus: usize,
    /// `V − E + F − R`.
    pub euler_lhs: i64,
    /// `2(S − G)`.
    pub euler_rhs: i64,
}

/// Validate everything reachable from `solid`. See module docs for the
/// checked invariant set. Returns the first violation found, or a
/// [`TopologyReport`] when all checks pass.
pub fn validate_solid(arena: &BrepArena, solid: SolidId) -> Result<TopologyReport, KernelV2Error> {
    let solid_ref = arena.solid(solid)?;

    // ---- gather reachable entities + invariant 2 (loop closure) ----------
    let mut face_ids: Vec<FaceId> = Vec::new();
    let mut loop_ids: Vec<(FaceId, LoopId)> = Vec::new();
    let mut he_set: BTreeSet<HalfEdgeId> = BTreeSet::new();
    let mut vertex_set: BTreeSet<VertexId> = BTreeSet::new();
    let mut rings = 0usize;
    let mut genus = 0usize;

    for &sh in &solid_ref.shells {
        let shell = arena.shell(sh)?;
        genus += shell.genus as usize;
        for &f in &shell.faces {
            let face = arena.face(f)?;
            face_ids.push(f);
            let mut loops = vec![face.outer_loop];
            loops.extend(face.inner_loops.iter().copied());
            for lid in loops {
                let lp = arena.loop_(lid)?;
                if lp.face != f {
                    return Err(KernelV2Error::LoopNotClosed { loop_id: lid });
                }
                if lp.kind == LoopKind::Inner {
                    rings += 1;
                }
                loop_ids.push((f, lid));
                match lp.boundary {
                    LoopBoundary::Lone(v) => {
                        arena.vertex(v)?;
                        vertex_set.insert(v);
                    }
                    LoopBoundary::Edges(_) => {
                        // `loop_half_edges` itself returns LoopNotClosed on a
                        // runaway orbit.
                        for h in arena.loop_half_edges(lid)? {
                            let he = arena.half_edge(h)?;
                            if he.loop_id != lid {
                                return Err(KernelV2Error::LoopNotClosed { loop_id: lid });
                            }
                            // prev must invert next.
                            if arena.half_edge(he.next)?.prev != h {
                                return Err(KernelV2Error::LoopNotClosed { loop_id: lid });
                            }
                            he_set.insert(h);
                            vertex_set.insert(he.origin);
                        }
                    }
                }
            }
        }
    }

    // ---- invariant 1: twin pairing ---------------------------------------
    for &h in &he_set {
        let he = arena.half_edge(h)?;
        if he.twin == h {
            return Err(KernelV2Error::TwinPairingBroken { half_edge: h });
        }
        let Ok(twin) = arena.half_edge(he.twin) else {
            return Err(KernelV2Error::TwinPairingBroken { half_edge: h });
        };
        if twin.twin != h || !he_set.contains(&he.twin) {
            return Err(KernelV2Error::TwinPairingBroken { half_edge: h });
        }
        // Twins traverse the same two vertices in opposite directions.
        if twin.origin != arena.half_edge(he.next)?.origin {
            return Err(KernelV2Error::TwinPairingBroken { half_edge: h });
        }
        // Curve consistency (PR-KV5a): twins describe the same undirected
        // edge in opposite directions; a circle half-edge closes on its
        // own anchor vertex.
        if !curves_twin_consistent(he.curve, twin.curve) {
            return Err(KernelV2Error::CurveTwinMismatch { half_edge: h });
        }
        if matches!(he.curve, Curve::Circle { .. }) && arena.half_edge(he.next)?.origin != he.origin
        {
            return Err(KernelV2Error::CurveTwinMismatch { half_edge: h });
        }
    }
    debug_assert_eq!(he_set.len() % 2, 0);
    let edges = he_set.len() / 2;

    // ---- invariant 3: vertex manifoldness (single radial fan) ------------
    let mut fans: BTreeMap<VertexId, Vec<HalfEdgeId>> = BTreeMap::new();
    for &h in &he_set {
        fans.entry(arena.half_edge(h)?.origin).or_default().push(h);
    }
    for (&v, hes) in &fans {
        // Orbit around the vertex: h ↦ next(twin(h)) also leaves v.
        let start = hes[0];
        let mut seen = 1usize;
        let mut cur = start;
        loop {
            let twin = arena.half_edge(cur)?.twin;
            cur = arena.half_edge(twin)?.next;
            if cur == start {
                break;
            }
            seen += 1;
            if seen > hes.len() {
                return Err(KernelV2Error::NonManifoldVertex { vertex: v });
            }
        }
        if seen != hes.len() {
            return Err(KernelV2Error::NonManifoldVertex { vertex: v });
        }
    }

    // ---- invariants 4+5: surface presence + per-surface orientation ------
    // Planar faces: planarity tripwire (debug) + Newell agreement + ring
    // winding — exactly as before, EXTENDED to circle-bounded loops (the
    // directional `Curve::Circle::normal` replaces the Newell normal of a
    // loop that has no polygonal walk). Cylinder faces: the curved
    // orientation rules (see `validate_cylinder_face`).
    for &f in &face_ids {
        let face = arena.face(f)?;
        match face.surface {
            Some(Surface::Plane(plane)) => validate_planar_face(arena, f, face, plane)?,
            Some(Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            }) => validate_cylinder_face(arena, f, face, axis_point, axis_dir, radius)?,
            None => return Err(KernelV2Error::FaceWithoutSurface { face: f }),
        }
    }

    // ---- invariant 6: Euler–Poincaré -------------------------------------
    let counts = arena.euler_counts(solid)?;
    if !counts.holds() {
        return Err(KernelV2Error::EulerFormulaViolation {
            lhs: counts.lhs(),
            rhs: counts.rhs(),
        });
    }

    Ok(TopologyReport {
        vertices: vertex_set.len(),
        edges,
        faces: face_ids.len(),
        rings,
        shells: solid_ref.shells.len(),
        genus,
        euler_lhs: counts.lhs(),
        euler_rhs: counts.rhs(),
    })
}

/// Twin curve agreement (PR-KV5a): both `LineSegment`, or both `Circle`
/// with identical center/radius and exactly negated normals (the assembler
/// constructs the negation, so exact f64 comparison is the honest check —
/// `-0.0 == 0.0` makes signed zeros immaterial).
fn curves_twin_consistent(a: Curve, b: Curve) -> bool {
    match (a, b) {
        (Curve::LineSegment, Curve::LineSegment) => true,
        (
            Curve::Circle {
                center: c1,
                normal: n1,
                radius: r1,
            },
            Curve::Circle {
                center: c2,
                normal: n2,
                radius: r2,
            },
        ) => c1 == c2 && r1 == r2 && n2.x == -n1.x && n2.y == -n1.y && n2.z == -n1.z,
        _ => false,
    }
}

/// The circle half-edges of a loop, with their curve data, in walk order.
fn loop_circles(
    arena: &BrepArena,
    hes: &[HalfEdgeId],
) -> Result<Vec<(Point3, crate::arena::UnitVector3, f64)>, KernelV2Error> {
    let mut out = Vec::new();
    for &h in hes {
        if let Curve::Circle {
            center,
            normal,
            radius,
        } = arena.half_edge(h)?.curve
        {
            out.push((center, normal, radius));
        }
    }
    Ok(out)
}

/// Invariants 4+5 for a planar face: surface agreement with the boundary
/// walk. Polygonal loops use the Newell normal (hard rule 2) exactly as in
/// KV1; circle-bounded loops (PR-KV5a) use the directional
/// [`Curve::Circle`] normal as the orientation source — a cap's single
/// circle half-edge must traverse CCW around the face normal, and a circle
/// ring CCW around its negation (the ring-winding analog).
fn validate_planar_face(
    arena: &BrepArena,
    f: FaceId,
    face: &crate::arena::Face,
    plane: crate::arena::Plane,
) -> Result<(), KernelV2Error> {
    // ---- outer loop orientation -------------------------------------------
    let outer_hes = arena.loop_half_edges(face.outer_loop)?;
    let outer_circles = loop_circles(arena, &outer_hes)?;
    if outer_circles.is_empty() {
        // Stored normal ≡ Newell(outer loop) — hard rule 2.
        let pts = arena.loop_points(face.outer_loop)?;
        let Some(newell) = geom::newell_unit(&pts) else {
            return Err(KernelV2Error::NewellMismatch { face: f });
        };
        if geom::dot(plane.normal, newell) < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
            return Err(KernelV2Error::NewellMismatch { face: f });
        }
    } else {
        // Full-circle boundary: exactly ONE closed circle half-edge (mixed
        // circle/segment planar loops are arc territory — future variant).
        if outer_hes.len() != 1 {
            return Err(KernelV2Error::CurvedGeometryMismatch {
                face: f,
                reason: "planar loop mixes circle and segment edges (outside the KV5a vocabulary)",
            });
        }
        let (_, nu, _) = outer_circles[0];
        if geom::dot(nu, plane.normal) < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
            return Err(KernelV2Error::CurvedGeometryMismatch {
                face: f,
                reason: "cap circle half-edge must traverse CCW around the face normal",
            });
        }
    }

    // ---- ring winding -----------------------------------------------------
    for &rid in &face.inner_loops {
        let hes = arena.loop_half_edges(rid)?;
        let circles = loop_circles(arena, &hes)?;
        if circles.is_empty() {
            let ring_pts = arena.loop_points(rid)?;
            if ring_pts.is_empty() {
                continue; // lone-vertex ring has no winding
            }
            let rn = geom::newell(&ring_pts);
            let d = rn[0] * plane.normal.x + rn[1] * plane.normal.y + rn[2] * plane.normal.z;
            if d >= 0.0 {
                return Err(KernelV2Error::RingWindingMismatch { face: f, ring: rid });
            }
        } else {
            if hes.len() != 1 {
                return Err(KernelV2Error::CurvedGeometryMismatch {
                    face: f,
                    reason:
                        "planar ring mixes circle and segment edges (outside the KV5a vocabulary)",
                });
            }
            let (_, nu, _) = circles[0];
            if geom::dot(nu, plane.normal) > -(1.0 - NORMAL_AGREEMENT_TOLERANCE) {
                return Err(KernelV2Error::RingWindingMismatch { face: f, ring: rid });
            }
        }
    }

    // ---- debug-tier geometric tripwires (see module docs) -----------------
    #[cfg(debug_assertions)]
    {
        let mut loops = vec![face.outer_loop];
        loops.extend(face.inner_loops.iter().copied());
        for lid in loops {
            // Every loop vertex (incl. circle anchors) on the plane.
            for p in arena.loop_points(lid)? {
                let d = (p.x() - plane.point.x()) * plane.normal.x
                    + (p.y() - plane.point.y()) * plane.normal.y
                    + (p.z() - plane.point.z()) * plane.normal.z;
                if d.abs() > PLANARITY_DEBUG_TOLERANCE {
                    return Err(KernelV2Error::NonPlanarFace { face: f });
                }
            }
            // Circle centers on the plane; anchors on their circles.
            let hes = arena.loop_half_edges(lid)?;
            for &h in &hes {
                let he = arena.half_edge(h)?;
                if let Curve::Circle { center, radius, .. } = he.curve {
                    let d = (center.x() - plane.point.x()) * plane.normal.x
                        + (center.y() - plane.point.y()) * plane.normal.y
                        + (center.z() - plane.point.z()) * plane.normal.z;
                    if d.abs() > PLANARITY_DEBUG_TOLERANCE {
                        return Err(KernelV2Error::NonPlanarFace { face: f });
                    }
                    let p = arena.vertex(he.origin)?.point;
                    let dr = ((p.x() - center.x()).powi(2)
                        + (p.y() - center.y()).powi(2)
                        + (p.z() - center.z()).powi(2))
                    .sqrt();
                    if (dr - radius).abs() > CURVED_SURFACE_DEBUG_TOLERANCE {
                        return Err(KernelV2Error::VertexOffSurface { face: f });
                    }
                }
            }
        }
    }
    Ok(())
}

/// Invariants 4+5 for a cylinder lateral face (PR-KV5a). Production tier —
/// the curved Newell analog (orientation/consistency, decided from stored
/// data, no geometric tolerance band beyond unit-vector rounding):
///
/// - finite positive radius; unit axis;
/// - no inner loops, and exactly TWO full-circle rim half-edges in the
///   outer loop (the Stroud single-fake-edge lateral; arcs/partial laterals
///   are a future vocabulary);
/// - each rim's radius equals the surface radius and its normal is along
///   the axis;
/// - each rim's traversal axis points TOWARD the opposite rim — this is
///   what makes the boundary walk consistent with the radially-outward
///   surface orientation (walking a rim with the face on your left, viewed
///   from outside, runs CCW around the axis pointing into the lateral).
///
/// Debug tier ([`CURVED_SURFACE_DEBUG_TOLERANCE`]): loop vertices on the
/// surface, rim centers on the axis, seam segments parallel to the axis.
fn validate_cylinder_face(
    arena: &BrepArena,
    f: FaceId,
    face: &crate::arena::Face,
    axis_point: Point3,
    axis_dir: crate::arena::UnitVector3,
    radius: f64,
) -> Result<(), KernelV2Error> {
    let mismatch = |reason: &'static str| KernelV2Error::CurvedGeometryMismatch { face: f, reason };
    if !radius.is_finite() || radius <= 0.0 {
        return Err(mismatch("cylinder radius must be finite and positive"));
    }
    let alen = (axis_dir.x * axis_dir.x + axis_dir.y * axis_dir.y + axis_dir.z * axis_dir.z).sqrt();
    if (alen - 1.0).abs() > NORMAL_AGREEMENT_TOLERANCE {
        return Err(mismatch("cylinder axis_dir must be unit-length"));
    }
    if !face.inner_loops.is_empty() {
        return Err(mismatch(
            "cylinder face with inner loops is outside the KV5a vocabulary",
        ));
    }
    let hes = arena.loop_half_edges(face.outer_loop)?;
    let rims = loop_circles(arena, &hes)?;
    if rims.len() != 2 {
        return Err(mismatch(
            "cylinder face must be bounded by exactly two full-circle rims (KV5a)",
        ));
    }
    for (i, &(c, nu, r)) in rims.iter().enumerate() {
        if (r - radius).abs() > 1e-9 * radius {
            return Err(mismatch("rim circle radius disagrees with the surface"));
        }
        if (geom::dot(nu, axis_dir)).abs() < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
            return Err(mismatch(
                "rim circle normal must be along the cylinder axis",
            ));
        }
        let other = rims[1 - i].0;
        let toward =
            (other.x() - c.x()) * nu.x + (other.y() - c.y()) * nu.y + (other.z() - c.z()) * nu.z;
        if toward <= 0.0 {
            return Err(mismatch(
                "rim traversal axis must point toward the opposite rim (outward orientation)",
            ));
        }
    }

    #[cfg(debug_assertions)]
    {
        let dist_to_axis = |p: Point3| {
            let d = [
                p.x() - axis_point.x(),
                p.y() - axis_point.y(),
                p.z() - axis_point.z(),
            ];
            let t = d[0] * axis_dir.x + d[1] * axis_dir.y + d[2] * axis_dir.z;
            let r = [
                d[0] - t * axis_dir.x,
                d[1] - t * axis_dir.y,
                d[2] - t * axis_dir.z,
            ];
            (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt()
        };
        for p in arena.loop_points(face.outer_loop)? {
            if (dist_to_axis(p) - radius).abs() > CURVED_SURFACE_DEBUG_TOLERANCE {
                return Err(KernelV2Error::VertexOffSurface { face: f });
            }
        }
        for &(c, _, _) in &rims {
            if dist_to_axis(c) > CURVED_SURFACE_DEBUG_TOLERANCE {
                return Err(KernelV2Error::VertexOffSurface { face: f });
            }
        }
        // Seam segments must be rulings (parallel to the axis).
        for &h in &hes {
            let he = arena.half_edge(h)?;
            if matches!(he.curve, Curve::LineSegment) {
                let p0 = arena.vertex(he.origin)?.point;
                let p1 = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
                let d = [p1.x() - p0.x(), p1.y() - p0.y(), p1.z() - p0.z()];
                let cx = [
                    d[1] * axis_dir.z - d[2] * axis_dir.y,
                    d[2] * axis_dir.x - d[0] * axis_dir.z,
                    d[0] * axis_dir.y - d[1] * axis_dir.x,
                ];
                let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                let off = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
                if off > CURVED_SURFACE_DEBUG_TOLERANCE * len.max(1.0) {
                    return Err(KernelV2Error::VertexOffSurface { face: f });
                }
            }
        }
    }
    Ok(())
}

/// Whole-arena invariant re-verification used by the Euler operators'
/// exit `debug_assert!`s. Checks structural integrity (twin pairing, loop
/// closure, back-pointers), the Newell invariant in its *construction* form
/// (`surface` is `Some` **iff** the outer loop is orientable, and then the
/// normals agree), and the Euler–Poincaré formula for every live solid.
///
/// Unlike [`validate_solid`] it tolerates faces that are legitimately under
/// construction (`surface: None` with a degenerate loop), does not require
/// vertex fans to be complete (mid-construction states are valid), and runs
/// over the whole arena rather than one solid.
pub(crate) fn debug_check_arena(arena: &BrepArena) -> Result<(), KernelV2Error> {
    // Half-edge structural checks.
    for (i, slot) in arena.half_edges.iter().enumerate() {
        let Some(he) = slot else { continue };
        let h = HalfEdgeId(i as u32);
        let Ok(twin) = arena.half_edge(he.twin) else {
            return Err(KernelV2Error::TwinPairingBroken { half_edge: h });
        };
        if he.twin == h || twin.twin != h {
            return Err(KernelV2Error::TwinPairingBroken { half_edge: h });
        }
        if twin.origin != arena.half_edge(he.next)?.origin {
            return Err(KernelV2Error::TwinPairingBroken { half_edge: h });
        }
        if !curves_twin_consistent(he.curve, twin.curve) {
            return Err(KernelV2Error::CurveTwinMismatch { half_edge: h });
        }
        if arena.half_edge(he.next)?.prev != h || arena.half_edge(he.prev)?.next != h {
            return Err(KernelV2Error::LoopNotClosed {
                loop_id: he.loop_id,
            });
        }
        arena.loop_(he.loop_id)?;
        arena.vertex(he.origin)?;
    }
    // Loop closure + membership.
    for (i, slot) in arena.loops.iter().enumerate() {
        let Some(lp) = slot else { continue };
        let lid = LoopId(i as u32);
        arena.face(lp.face)?;
        for h in arena.loop_half_edges(lid)? {
            if arena.half_edge(h)?.loop_id != lid {
                return Err(KernelV2Error::LoopNotClosed { loop_id: lid });
            }
        }
    }
    // Face back-pointers + Newell construction invariant. Circle-bearing
    // faces (caps, cylinder laterals — PR-KV5a) have no polygonal walk to
    // take a Newell normal of; they are created fully-formed by the direct
    // assemblers, which run the complete curved checks via `validate_solid`
    // immediately, so the construction-form check here only requires the
    // surface to be present.
    for (i, slot) in arena.faces.iter().enumerate() {
        let Some(face) = slot else { continue };
        let f = FaceId(i as u32);
        let outer_hes = arena.loop_half_edges(face.outer_loop)?;
        let mut has_circle = false;
        for &h in &outer_hes {
            if matches!(arena.half_edge(h)?.curve, Curve::Circle { .. }) {
                has_circle = true;
            }
        }
        let pts = arena.loop_points(face.outer_loop)?;
        match (&face.surface, has_circle) {
            (Some(_), true) => {}
            (None, true) => return Err(KernelV2Error::FaceWithoutSurface { face: f }),
            (Some(Surface::Cylinder { .. }), false) => {
                return Err(KernelV2Error::CurvedGeometryMismatch {
                    face: f,
                    reason: "cylinder face without circle rim edges",
                })
            }
            (Some(Surface::Plane(plane)), false) => match geom::newell_unit(&pts) {
                Some(newell) => {
                    if geom::dot(plane.normal, newell) < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
                        return Err(KernelV2Error::NewellMismatch { face: f });
                    }
                }
                None => return Err(KernelV2Error::NewellMismatch { face: f }),
            },
            (None, false) => {
                if geom::newell_unit(&pts).is_some() {
                    return Err(KernelV2Error::NewellMismatch { face: f });
                }
            }
        }
        if arena.loop_(face.outer_loop)?.kind != LoopKind::Outer {
            return Err(KernelV2Error::LoopNotClosed {
                loop_id: face.outer_loop,
            });
        }
        for &rid in &face.inner_loops {
            if arena.loop_(rid)?.kind != LoopKind::Inner {
                return Err(KernelV2Error::LoopNotClosed { loop_id: rid });
            }
        }
    }
    // Euler–Poincaré for every live solid.
    for (i, slot) in arena.solids.iter().enumerate() {
        if slot.is_none() {
            continue;
        }
        let counts = arena.euler_counts(SolidId(i as u32))?;
        if !counts.holds() {
            return Err(KernelV2Error::EulerFormulaViolation {
                lhs: counts.lhs(),
                rhs: counts.rhs(),
            });
        }
    }
    Ok(())
}
