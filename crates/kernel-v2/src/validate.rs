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
        // A full circle closes on its own anchor; an arc never does
        // (distinct endpoints — else it would BE a full circle).
        let closes = arena.half_edge(he.next)?.origin == he.origin;
        if matches!(he.curve, Curve::Circle { .. }) && !closes {
            return Err(KernelV2Error::CurveTwinMismatch { half_edge: h });
        }
        if matches!(he.curve, Curve::Arc { .. }) && closes {
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
                reversed,
            }) => validate_cylinder_face(arena, f, face, axis_point, axis_dir, radius, reversed)?,
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

/// Twin curve agreement (PR-KV5a circles, PR-KV5b arcs): both
/// `LineSegment`, or both `Circle`/both `Arc` with identical center/radius
/// and exactly negated normals (the assemblers construct the negation, so
/// exact f64 comparison is the honest check — `-0.0 == 0.0` makes signed
/// zeros immaterial).
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
        )
        | (
            Curve::Arc {
                center: c1,
                normal: n1,
                radius: r1,
            },
            Curve::Arc {
                center: c2,
                normal: n2,
                radius: r2,
            },
        ) => c1 == c2 && r1 == r2 && n2.x == -n1.x && n2.y == -n1.y && n2.z == -n1.z,
        _ => false,
    }
}

/// The full-circle half-edges of a loop, with their curve data, in walk
/// order.
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

/// The arc half-edges of a loop, with their curve data, in walk order.
fn loop_arcs(
    arena: &BrepArena,
    hes: &[HalfEdgeId],
) -> Result<Vec<(Point3, crate::arena::UnitVector3, f64)>, KernelV2Error> {
    let mut out = Vec::new();
    for &h in hes {
        if let Curve::Arc {
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

/// Winding polyline of a loop for Newell-orientation checks: the loop's
/// vertex cycle with each [`Curve::Arc`]'s sweep MIDPOINT sample inserted
/// after its origin.
///
/// The raw vertex chord polygon winds identically to the true boundary
/// only for minor arcs (sweep < π) — the original KV5b assumption. Revolve
/// (PR-KV6a) produces sweep arcs anywhere in (0, 2π): a >180° annular
/// sector's chord quad has zero or NEGATIVE shoelace area, so the chord
/// Newell would wrongly reject a correctly wound face. One midpoint per
/// arc makes the polyline's winding match the true boundary for ANY sweep
/// < 2π; mis-wound loops still fail (the oracle's domain widens, its
/// strictness does not change).
fn winding_points(arena: &BrepArena, hes: &[HalfEdgeId]) -> Result<Vec<Point3>, KernelV2Error> {
    let mut pts = Vec::with_capacity(hes.len() * 2);
    for &h in hes {
        let he = arena.half_edge(h)?;
        let p0 = arena.vertex(he.origin)?.point;
        pts.push(p0);
        if let Curve::Arc { center, normal, .. } = he.curve {
            let p1 = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
            let nu = [normal.x, normal.y, normal.z];
            if let Some(sweep) = geom::ccw_sweep(center, nu, p0, p1) {
                pts.push(rotate_about(center, nu, p0, sweep / 2.0));
            }
        }
    }
    Ok(pts)
}

/// Rodrigues rotation of `p` about the axis (`center`, unit `axis`) by
/// `theta` (right-handed).
fn rotate_about(center: Point3, axis: [f64; 3], p: Point3, theta: f64) -> Point3 {
    let v = [p.x() - center.x(), p.y() - center.y(), p.z() - center.z()];
    let (c, s) = (theta.cos(), theta.sin());
    let dot = axis[0] * v[0] + axis[1] * v[1] + axis[2] * v[2];
    let cx = [
        axis[1] * v[2] - axis[2] * v[1],
        axis[2] * v[0] - axis[0] * v[2],
        axis[0] * v[1] - axis[1] * v[0],
    ];
    Point3::new(
        center.x() + v[0] * c + cx[0] * s + axis[0] * dot * (1.0 - c),
        center.y() + v[1] * c + cx[1] * s + axis[1] * dot * (1.0 - c),
        center.z() + v[2] * c + cx[2] * s + axis[2] * dot * (1.0 - c),
    )
}

/// Debug-band for IMPORTED curved geometry (PR-KV5b): yang-rs boolean
/// outputs are computed in f64 closed form (Stage-1 trig sampling, Stage-4
/// relocation), so an absolute 1e-12 band (the KV5a construction tripwire)
/// is dishonest at large coordinates. The allowance scales with the
/// geometry magnitude: `1e-9 · (1 + max(r, ‖p‖∞))` — still far below any
/// feature scale (MIN_FEATURE_SIZE relative), purely a rounding allowance.
fn import_band(radius: f64, p: Point3) -> f64 {
    let m = p.x().abs().max(p.y().abs()).max(p.z().abs());
    1e-9 * (1.0 + radius.max(m))
}

/// Invariants 4+5 for a planar face: surface agreement with the boundary
/// walk.
///
/// - Polygonal loops use the Newell normal (hard rule 2) exactly as in KV1.
/// - Loops bounded by a single closed circle half-edge (PR-KV5a) use the
///   directional [`Curve::Circle`] normal as the orientation source — a
///   cap's circle must traverse CCW around the face normal, a circle ring
///   CCW around its negation (the ring-winding analog).
/// - Loops mixing [`Curve::Arc`] and segment edges (PR-KV5b yang boolean
///   outputs; PR-KV6a revolve sectors with sweeps anywhere in (0, 2π))
///   use the midpoint-augmented winding polyline ([`winding_points`]) for
///   the Newell normal, which winds identically to the true boundary for
///   any sweep < 2π. Each arc additionally must lie in the face plane
///   (its circle axis parallel to the face normal — sign-free, since a
///   loop legitimately walks arcs both ways around their centers).
fn validate_planar_face(
    arena: &BrepArena,
    f: FaceId,
    face: &crate::arena::Face,
    plane: crate::arena::Plane,
) -> Result<(), KernelV2Error> {
    // Arc-in-plane production rule, shared by all loops of this face.
    let arcs_in_plane = |hes: &[HalfEdgeId]| -> Result<(), KernelV2Error> {
        for &(_, nu, _) in &loop_arcs(arena, hes)? {
            if geom::dot(nu, plane.normal).abs() < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
                return Err(KernelV2Error::CurvedGeometryMismatch {
                    face: f,
                    reason: "planar-face arc's circle axis is not parallel to the face normal",
                });
            }
        }
        Ok(())
    };

    // ---- outer loop orientation -------------------------------------------
    let outer_hes = arena.loop_half_edges(face.outer_loop)?;
    let outer_circles = loop_circles(arena, &outer_hes)?;
    if outer_circles.is_empty() {
        // Stored normal ≡ Newell(outer loop) — hard rule 2. Arc-bearing
        // loops use the midpoint-augmented winding polyline (see
        // `winding_points`) so ANY sweep < 2π winds correctly.
        arcs_in_plane(&outer_hes)?;
        let pts = winding_points(arena, &outer_hes)?;
        let Some(newell) = geom::newell_unit(&pts) else {
            return Err(KernelV2Error::NewellMismatch { face: f });
        };
        if geom::dot(plane.normal, newell) < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
            return Err(KernelV2Error::NewellMismatch { face: f });
        }
    } else {
        // Full-circle boundary: exactly ONE closed circle half-edge.
        if outer_hes.len() != 1 {
            return Err(KernelV2Error::CurvedGeometryMismatch {
                face: f,
                reason: "planar loop mixes a full circle with other edges",
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
            arcs_in_plane(&hes)?;
            let ring_pts = winding_points(arena, &hes)?;
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
                    reason: "planar ring mixes a full circle with other edges",
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
            // Circle/arc centers on the plane; endpoints on their circles.
            // Full circles keep the exact-construction band; arcs (imported
            // yang output, PR-KV5b) use the import band.
            let hes = arena.loop_half_edges(lid)?;
            for &h in &hes {
                let he = arena.half_edge(h)?;
                let (center, radius, is_arc) = match he.curve {
                    Curve::Circle { center, radius, .. } => (center, radius, false),
                    Curve::Arc { center, radius, .. } => (center, radius, true),
                    Curve::LineSegment => continue,
                };
                let plane_band = if is_arc {
                    import_band(radius, center)
                } else {
                    PLANARITY_DEBUG_TOLERANCE
                };
                let d = (center.x() - plane.point.x()) * plane.normal.x
                    + (center.y() - plane.point.y()) * plane.normal.y
                    + (center.z() - plane.point.z()) * plane.normal.z;
                if d.abs() > plane_band {
                    return Err(KernelV2Error::NonPlanarFace { face: f });
                }
                let mut endpoints = vec![arena.vertex(he.origin)?.point];
                if is_arc {
                    endpoints.push(arena.vertex(arena.half_edge(he.next)?.origin)?.point);
                }
                for p in endpoints {
                    let band = if is_arc {
                        import_band(radius, p)
                    } else {
                        CURVED_SURFACE_DEBUG_TOLERANCE
                    };
                    let dr = ((p.x() - center.x()).powi(2)
                        + (p.y() - center.y()).powi(2)
                        + (p.z() - center.z()).powi(2))
                    .sqrt();
                    if (dr - radius).abs() > band {
                        return Err(KernelV2Error::VertexOffSurface { face: f });
                    }
                }
            }
        }
    }
    Ok(())
}

/// Invariants 4+5 for a cylinder lateral face. Two vocabularies:
///
/// **Canonical full lateral (PR-KV5a)** — any loop carries a full-circle
/// half-edge. Production tier (the curved Newell analog, decided from
/// stored data, no geometric tolerance beyond unit-vector rounding):
///
/// - finite positive radius; unit axis; outward sense (`reversed` is the
///   KV5b cavity vocabulary and never canonical);
/// - no inner loops, and exactly TWO full-circle rim half-edges in the
///   outer loop (the Stroud single-fake-edge lateral);
/// - each rim's radius equals the surface radius and its normal is along
///   the axis;
/// - each rim's traversal axis points TOWARD the opposite rim — this is
///   what makes the boundary walk consistent with the radially-outward
///   surface orientation (walking a rim with the face on your left, viewed
///   from outside, runs CCW around the axis pointing into the lateral).
///
/// **Partial patch (PR-KV5b, yang boolean outputs)** — loops of
/// [`Curve::Arc`] and segment edges. Production tier (see
/// [`validate_cylinder_patch`]): per-arc surface agreement (radius, axis
/// parallelism) plus the UNROLLED-WINDING orientation analysis — the
/// developable-surface generalization of the Newell rule: in the unrolled
/// `(θ·r, h)` frame (mirrored for `reversed`), the boundary loops must
/// wind material-CCW: either exactly one non-wrapping loop is CCW with all
/// others CW (a bounded patch with windows), or exactly two loops wrap the
/// axis (±1) with the `+1` wrap at the lower axial height and every
/// non-wrapping loop CW (a barrel segment with windows).
///
/// Debug tier: loop vertices on the surface, rim/arc centers on the axis —
/// at [`CURVED_SURFACE_DEBUG_TOLERANCE`] for exact-constructed canonical
/// solids, at the scale-relative [`import_band`] for imported patches;
/// canonical seam segments parallel to the axis (partial patches carry
/// genuine chord segments, which are NOT rulings, so no seam rule there).
#[allow(clippy::too_many_arguments)]
fn validate_cylinder_face(
    arena: &BrepArena,
    f: FaceId,
    face: &crate::arena::Face,
    axis_point: Point3,
    axis_dir: crate::arena::UnitVector3,
    radius: f64,
    reversed: bool,
) -> Result<(), KernelV2Error> {
    let mismatch = |reason: &'static str| KernelV2Error::CurvedGeometryMismatch { face: f, reason };
    if !radius.is_finite() || radius <= 0.0 {
        return Err(mismatch("cylinder radius must be finite and positive"));
    }
    let alen = (axis_dir.x * axis_dir.x + axis_dir.y * axis_dir.y + axis_dir.z * axis_dir.z).sqrt();
    if (alen - 1.0).abs() > NORMAL_AGREEMENT_TOLERANCE {
        return Err(mismatch("cylinder axis_dir must be unit-length"));
    }

    // Vocabulary dispatch: any full-circle edge anywhere → canonical.
    let mut all_loops = vec![face.outer_loop];
    all_loops.extend(face.inner_loops.iter().copied());
    let mut has_full = false;
    for &lid in &all_loops {
        if !loop_circles(arena, &arena.loop_half_edges(lid)?)?.is_empty() {
            has_full = true;
        }
    }
    if !has_full {
        return validate_cylinder_patch(arena, f, face, axis_point, axis_dir, radius, reversed);
    }

    if !face.inner_loops.is_empty() {
        return Err(mismatch(
            "cylinder face with inner loops is outside the KV5a vocabulary",
        ));
    }
    let hes = arena.loop_half_edges(face.outer_loop)?;
    if !loop_arcs(arena, &hes)?.is_empty() {
        return Err(mismatch(
            "cylinder face mixes full-circle rims with arc edges",
        ));
    }
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
        // Outward lateral: each rim's traversal axis points TOWARD the
        // opposite rim. Cavity wall (reversed, PR-KV6a — the washer's
        // inner bore): the mirrored material sense, AWAY from it. (The twin
        // structure forces this: each rim twin lives in an adjacent face
        // whose own winding rules fix the sign.)
        if (!reversed && toward <= 0.0) || (reversed && toward >= 0.0) {
            return Err(mismatch(
                "rim traversal axis disagrees with the lateral's material sense",
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

/// Per-loop unrolled measurements over a cylinder patch (PR-KV5b): net
/// axis wrap, mean axial height, and (for non-wrapping loops) twice the
/// signed shoelace area in the unrolled `(θ, h)` frame.
struct LoopMeasure {
    loop_id: LoopId,
    wrap: i64,
    mean_h: f64,
    area2: f64,
}

/// Invariants 4+5 for a PARTIAL cylinder patch (PR-KV5b): boundary loops
/// of [`Curve::Arc`] and [`Curve::LineSegment`] edges, as assembled from
/// yang-rs boolean outputs. See [`validate_cylinder_face`]'s doc comment
/// for the rule set; this is the unrolled-winding orientation analysis —
/// the developable generalization of the Newell invariant.
///
/// Soundness of the per-edge angular steps: arcs carry their exact signed
/// sweep (their circle axis is parallel to the cylinder axis — checked);
/// segment chords take the principal-value step, sound while no single
/// chord subtends ≥ π around the axis (yang facet chords subtend one
/// Stage-1 facet, ≤ 2π/8). A violated assumption breaks the integrality
/// of the loop's net winding, which IS checked, loudly.
#[allow(clippy::too_many_arguments)]
fn validate_cylinder_patch(
    arena: &BrepArena,
    f: FaceId,
    face: &crate::arena::Face,
    axis_point: Point3,
    axis_dir: crate::arena::UnitVector3,
    radius: f64,
    reversed: bool,
) -> Result<(), KernelV2Error> {
    use std::f64::consts::PI;
    let mismatch = |reason: &'static str| KernelV2Error::CurvedGeometryMismatch { face: f, reason };
    let a = [axis_dir.x, axis_dir.y, axis_dir.z];
    let ap = [axis_point.x(), axis_point.y(), axis_point.z()];
    // The mirror sense: a cavity wall (reversed) is validated in the
    // mirrored frame u = −θ, where its boundary winds material-CCW again.
    let sense = if reversed { -1.0 } else { 1.0 };

    let mut all_loops = vec![face.outer_loop];
    all_loops.extend(face.inner_loops.iter().copied());

    // Shared angular frame: e1 from the first outer-loop vertex's radial
    // direction (each loop only needs internal consistency, but one shared
    // frame keeps the analysis deterministic and debuggable).
    let radial_theta_h = |p: Point3, e1: [f64; 3], e2: [f64; 3]| -> Option<(f64, f64)> {
        let d = [p.x() - ap[0], p.y() - ap[1], p.z() - ap[2]];
        let h = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
        let r = [d[0] - h * a[0], d[1] - h * a[1], d[2] - h * a[2]];
        let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        if !(rl.is_finite() && rl > 0.0) {
            return None;
        }
        let x = r[0] * e1[0] + r[1] * e1[1] + r[2] * e1[2];
        let y = r[0] * e2[0] + r[1] * e2[1] + r[2] * e2[2];
        Some((y.atan2(x), h))
    };
    let first_hes = arena.loop_half_edges(face.outer_loop)?;
    if first_hes.is_empty() {
        return Err(mismatch("cylinder patch with an empty boundary loop"));
    }
    let p0 = arena.vertex(arena.half_edge(first_hes[0])?.origin)?.point;
    let d0 = [p0.x() - ap[0], p0.y() - ap[1], p0.z() - ap[2]];
    let h0 = d0[0] * a[0] + d0[1] * a[1] + d0[2] * a[2];
    let r0 = [d0[0] - h0 * a[0], d0[1] - h0 * a[1], d0[2] - h0 * a[2]];
    let r0l = (r0[0] * r0[0] + r0[1] * r0[1] + r0[2] * r0[2]).sqrt();
    if !(r0l.is_finite() && r0l > 0.0) {
        return Err(mismatch("cylinder patch anchor vertex lies on the axis"));
    }
    let e1 = [r0[0] / r0l, r0[1] / r0l, r0[2] / r0l];
    let e2 = [
        a[1] * e1[2] - a[2] * e1[1],
        a[2] * e1[0] - a[0] * e1[2],
        a[0] * e1[1] - a[1] * e1[0],
    ];

    let mut measures: Vec<LoopMeasure> = Vec::with_capacity(all_loops.len());
    for &lid in &all_loops {
        let hes = arena.loop_half_edges(lid)?;
        if hes.len() < 3 {
            return Err(mismatch("cylinder patch loop with fewer than 3 edges"));
        }
        let mut us: Vec<f64> = Vec::with_capacity(hes.len());
        let mut hs: Vec<f64> = Vec::with_capacity(hes.len());
        let mut u_cur = f64::NAN; // set from the first vertex below
        let mut total = 0.0f64;
        for (i, &h) in hes.iter().enumerate() {
            let he = arena.half_edge(h)?;
            let p = arena.vertex(he.origin)?.point;
            let q = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
            let Some((theta_p, hp)) = radial_theta_h(p, e1, e2) else {
                return Err(mismatch("cylinder patch vertex lies on the axis"));
            };
            if i == 0 {
                u_cur = theta_p;
            }
            us.push(u_cur);
            hs.push(hp);

            let delta = match he.curve {
                Curve::LineSegment => {
                    let Some((theta_q, _)) = radial_theta_h(q, e1, e2) else {
                        return Err(mismatch("cylinder patch vertex lies on the axis"));
                    };
                    geom::wrap_to_pi(theta_q - theta_p)
                }
                Curve::Arc {
                    center,
                    normal,
                    radius: r_arc,
                } => {
                    // Production-tier per-arc surface agreement.
                    if (r_arc - radius).abs() > 1e-9 * radius {
                        return Err(mismatch("patch arc radius disagrees with the surface"));
                    }
                    let nd = geom::dot(normal, axis_dir);
                    if nd.abs() < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
                        return Err(mismatch(
                            "patch arc's circle axis is not parallel to the cylinder axis",
                        ));
                    }
                    #[cfg(debug_assertions)]
                    {
                        // Arc center on the axis (import band — see fn docs).
                        let dc = [center.x() - ap[0], center.y() - ap[1], center.z() - ap[2]];
                        let hc = dc[0] * a[0] + dc[1] * a[1] + dc[2] * a[2];
                        let rc = [dc[0] - hc * a[0], dc[1] - hc * a[1], dc[2] - hc * a[2]];
                        let off = (rc[0] * rc[0] + rc[1] * rc[1] + rc[2] * rc[2]).sqrt();
                        if off > import_band(radius, center) {
                            return Err(KernelV2Error::VertexOffSurface { face: f });
                        }
                    }
                    let n_arr = [normal.x, normal.y, normal.z];
                    let Some(sweep) = geom::ccw_sweep(center, n_arr, p, q) else {
                        return Err(mismatch("patch arc endpoint has no radial direction"));
                    };
                    if nd > 0.0 {
                        sweep
                    } else {
                        -sweep
                    }
                }
                Curve::Circle { .. } => {
                    // Unreachable: the dispatcher sends full-circle faces to
                    // the canonical path. Loud, defensively.
                    return Err(mismatch("full-circle edge inside a partial cylinder patch"));
                }
            };
            u_cur += delta;
            total += delta;
        }
        let wraps_f = total / (2.0 * PI);
        let wraps = wraps_f.round();
        if (wraps_f - wraps).abs() > 1e-3 {
            return Err(mismatch(
                "cylinder patch loop's net axis winding is not integral",
            ));
        }
        let wraps = wraps as i64;
        if wraps.abs() > 1 {
            return Err(mismatch(
                "cylinder patch loop wraps the axis more than once",
            ));
        }
        let m = us.len();
        let mut area2 = 0.0f64;
        for i in 0..m {
            let j = (i + 1) % m;
            area2 += us[i] * hs[j] - us[j] * hs[i];
        }
        measures.push(LoopMeasure {
            loop_id: lid,
            wrap: if sense < 0.0 { -wraps } else { wraps },
            mean_h: hs.iter().sum::<f64>() / m as f64,
            area2: sense * area2,
        });
    }

    // ---- face-level orientation rules (material-CCW in the unrolled frame)
    let wrapping: Vec<&LoopMeasure> = measures.iter().filter(|mm| mm.wrap != 0).collect();
    match wrapping.len() {
        0 => {
            // Bounded patch: exactly one CCW (material) loop, others CW
            // (windows).
            let mut positive = 0usize;
            for mm in &measures {
                if mm.area2 == 0.0 {
                    return Err(mismatch("cylinder patch loop has zero unrolled area"));
                }
                if mm.area2 > 0.0 {
                    positive += 1;
                }
            }
            if positive != 1 {
                return Err(mismatch(
                    "bounded cylinder patch must have exactly one material-CCW loop",
                ));
            }
        }
        2 => {
            // Barrel segment: a +1 and a −1 wrap, the +1 at the lower axial
            // height (the generalization of the KV5a rim rule "traversal
            // axis points toward the opposite rim"); windows wind CW.
            let (w0, w1) = (wrapping[0], wrapping[1]);
            if w0.wrap + w1.wrap != 0 {
                return Err(mismatch(
                    "cylinder patch wrapping loops do not wind oppositely",
                ));
            }
            let (plus, minus) = if w0.wrap > 0 { (w0, w1) } else { (w1, w0) };
            if plus.mean_h >= minus.mean_h {
                return Err(mismatch(
                    "cylinder patch wrapping loops are oriented away from the material",
                ));
            }
            for mm in &measures {
                if mm.wrap == 0 && mm.area2 >= 0.0 {
                    return Err(KernelV2Error::RingWindingMismatch {
                        face: f,
                        ring: mm.loop_id,
                    });
                }
            }
        }
        _ => {
            return Err(mismatch(
                "cylinder patch must have exactly 0 or 2 axis-wrapping loops",
            ));
        }
    }

    // ---- debug-tier geometric tripwire: loop vertices on the surface ------
    #[cfg(debug_assertions)]
    {
        for &lid in &all_loops {
            for p in arena.loop_points(lid)? {
                let d = [p.x() - ap[0], p.y() - ap[1], p.z() - ap[2]];
                let h = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
                let r = [d[0] - h * a[0], d[1] - h * a[1], d[2] - h * a[2]];
                let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
                if (rl - radius).abs() > import_band(radius, p) {
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
    // Face back-pointers + Newell construction invariant. Curved-bearing
    // faces (caps, cylinder laterals — PR-KV5a; arc-bounded patches —
    // PR-KV5b) have no purely-polygonal walk to take a Newell normal of;
    // they are created fully-formed by the direct assemblers, which run the
    // complete curved checks via `validate_solid` immediately, so the
    // construction-form check here only requires the surface to be present.
    // (A cylinder face whose loops are ALL chord segments is likewise a
    // KV5b assembler product — yang facets an original rim — and is
    // validated there.)
    for (i, slot) in arena.faces.iter().enumerate() {
        let Some(face) = slot else { continue };
        let f = FaceId(i as u32);
        let outer_hes = arena.loop_half_edges(face.outer_loop)?;
        let mut has_curved = false;
        for &h in &outer_hes {
            if !matches!(arena.half_edge(h)?.curve, Curve::LineSegment) {
                has_curved = true;
            }
        }
        let pts = arena.loop_points(face.outer_loop)?;
        match (&face.surface, has_curved) {
            (Some(_), true) => {}
            (None, true) => return Err(KernelV2Error::FaceWithoutSurface { face: f }),
            (Some(Surface::Cylinder { .. }), false) => {}
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
