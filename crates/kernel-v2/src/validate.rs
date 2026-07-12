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

/// Debug-tier planarity tripwire: maximum |signed distance| of a loop
/// vertex from its face plane, as a RELATIVE band — multiplied by
/// `(1 + max|coordinate|)` at the check site ([`planarity_band`]).
/// f64 guarantees ~2e-16 RELATIVE precision, so an exactly-constructed
/// vertex at world coordinate ~70 legitimately carries ~1e-13 of mapping
/// rounding (PR-KV8 gear profiles on oblique planes, hundreds of mapped
/// vertices) — an absolute band would mis-flag scale, not geometry. The
/// relative form stays strict: far below `TAU_MODEL` (1e-7 relative at
/// metre scale) while genuinely non-planar loops (feature size ≥
/// `MIN_FEATURE_SIZE`) fail loudly. See the module docs for why this is
/// not a production gate.
pub const PLANARITY_DEBUG_TOLERANCE: f64 = 1e-12;

/// Scale-relative planarity band at point `p` (see
/// [`PLANARITY_DEBUG_TOLERANCE`]).
fn planarity_band(p: Point3) -> f64 {
    PLANARITY_DEBUG_TOLERANCE * (1.0 + p.x().abs().max(p.y().abs()).max(p.z().abs()))
}

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

/// Construct a [`KernelV2Error::VertexOffSurface`], first dumping the
/// failing site when `KV2_OFFSURF_PROBE` is set (env-gated, zero-cost
/// off): which check tripped, the offending point, the residual, the band
/// it exceeded, and the surface being validated. An off-surface wall in a
/// chained boolean then self-localizes without a debugger — the tripwire
/// only names the face, and nine distinct checks share the variant.
fn vertex_off_surface(
    f: FaceId,
    site: &str,
    p: Point3,
    residual: f64,
    band: f64,
    surface: &str,
) -> KernelV2Error {
    if std::env::var_os("KV2_OFFSURF_PROBE").is_some() {
        eprintln!(
            "[offsurf-probe] face {f:?} site={site} p=({:.17e}, {:.17e}, {:.17e}) \
             residual={residual:.3e} band={band:.3e} surface={surface}",
            p.x(),
            p.y(),
            p.z()
        );
    }
    KernelV2Error::VertexOffSurface { face: f }
}

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
        if matches!(
            he.curve,
            Curve::Arc { .. }
                | Curve::EllipseArc { .. }
                | Curve::HyperbolaArc { .. }
                | Curve::SurfacePair { .. }
        ) && closes
        {
            // A closed single-edge surface-pair loop has no producer (M5
            // outputs are per-mesh-edge chains) — rejected like a closed
            // arc. A closed hyperbola edge is IMPOSSIBLE (the branch is
            // unbounded, KV16).
            return Err(KernelV2Error::CurveTwinMismatch { half_edge: h });
        }
    }
    debug_assert_eq!(he_set.len() % 2, 0);
    let edges = he_set.len() / 2;

    // ---- invariant 1b: surface-pair endpoint residuals (M5, K7) ----------
    // Every endpoint of a surface-pair edge lies ON BOTH defining surfaces
    // within the import band — the per-point certification contract of the
    // procedural curve (specs/m5_surface_pair_curve.md; [#24] §4.1.2). The
    // origin check per half-edge covers both endpoints across the twin pair.
    for &h in &he_set {
        let he = arena.half_edge(h)?;
        let Curve::SurfacePair { a, b } = he.curve else {
            continue;
        };
        let f = arena.loop_(he.loop_id)?.face;
        // Placement rule (M5, K8): a transversal quadric-pair curve is degree-4
        // and never planar — degenerate configs decompose into conics upstream.
        // So a surface-pair edge must bound only the two curved surfaces it is
        // the intersection of, never a PLANAR face.
        if matches!(arena.face(f)?.surface, Some(Surface::Plane(_))) {
            return Err(KernelV2Error::CurvedGeometryMismatch {
                face: f,
                reason: "surface-pair (degree-4) edge on a planar face",
            });
        }
        let p = arena.vertex(he.origin)?.point;
        for (s, which) in [
            (a, "surface-pair-endpoint-a"),
            (b, "surface-pair-endpoint-b"),
        ] {
            let Some((residual, _)) =
                geom::pair_surface_residual_gradient(&s, [p.x(), p.y(), p.z()])
            else {
                return Err(KernelV2Error::CurvedGeometryMismatch {
                    face: f,
                    reason: "surface-pair endpoint on a defining surface's axis",
                });
            };
            let band = import_band(geom::pair_surface_scale(&s), p);
            if residual.abs() > band {
                return Err(vertex_off_surface(
                    f,
                    which,
                    p,
                    residual.abs(),
                    band,
                    &format!("{s:?}"),
                ));
            }
        }
    }

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
            Some(Surface::Cone {
                apex,
                axis_dir,
                half_angle,
                reversed,
            }) => validate_cone_face(arena, f, face, apex, axis_dir, half_angle, reversed)?,
            Some(Surface::Torus {
                center,
                axis_dir,
                major_radius,
                minor_radius,
                ..
            }) => {
                validate_torus_face(arena, f, face, center, axis_dir, major_radius, minor_radius)?
            }
            Some(Surface::Sphere { center, radius, .. }) => {
                validate_sphere_face(arena, f, face, center, radius)?
            }
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
        (
            Curve::EllipseArc {
                center: c1,
                normal: n1,
                major_axis: m1,
                major_radius: a1,
                minor_radius: b1,
            },
            Curve::EllipseArc {
                center: c2,
                normal: n2,
                major_axis: m2,
                major_radius: a2,
                minor_radius: b2,
            },
        ) => {
            // PR-KV9: twins keep the SAME major_axis and negate the normal
            // (the frame's minor direction n̂×m̂ flips with n̂, so the point
            // set is identical, traversed oppositely).
            c1 == c2
                && a1 == a2
                && b1 == b2
                && m1 == m2
                && n2.x == -n1.x
                && n2.y == -n1.y
                && n2.z == -n1.z
        }
        (Curve::SurfacePair { a: a1, b: b1 }, Curve::SurfacePair { a: a2, b: b2 }) => {
            // M5: twins carry BIT-IDENTICAL surface pairs — both are minted
            // from the same ssi descriptor; there is no directional normal
            // to negate (traversal is endpoint-determined).
            a1 == a2 && b1 == b2
        }
        (
            Curve::HyperbolaArc {
                center: c1,
                normal: n1,
                major_axis: m1,
                semi_transverse: a1,
                semi_conjugate: b1,
            },
            Curve::HyperbolaArc {
                center: c2,
                normal: n2,
                major_axis: m2,
                semi_transverse: a2,
                semi_conjugate: b2,
            },
        ) => {
            // KV16: twins carry BIT-IDENTICAL fields (the open-branch arc
            // between two endpoints is unique — traversal is
            // endpoint-determined, like SurfacePair; no normal negation).
            c1 == c2 && n1 == n2 && m1 == m2 && a1 == a2 && b1 == b2
        }
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
                pts.push(geom::rotate_about_axis(center, nu, p0, sweep / 2.0));
            }
        }
        if let Curve::EllipseArc {
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
        } = he.curve
        {
            // PR-KV9: midpoint at half the PARAMETRIC sweep (the bulge point
            // the chord polygon misses), same role as the arc midpoint.
            let p1 = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
            let nu = [normal.x, normal.y, normal.z];
            let mr = [major_axis.x, major_axis.y, major_axis.z];
            if let (Some(t0), Some(sweep)) = (
                geom::ellipse_param(center, nu, mr, major_radius, minor_radius, p0),
                geom::ellipse_ccw_sweep(center, nu, mr, major_radius, minor_radius, p0, p1),
            ) {
                pts.push(geom::ellipse_point_at(
                    center,
                    nu,
                    mr,
                    major_radius,
                    minor_radius,
                    t0 + sweep / 2.0,
                ));
            }
        }
        if let Curve::HyperbolaArc {
            center,
            normal,
            major_axis,
            semi_transverse,
            semi_conjugate,
        } = he.curve
        {
            // KV16: parametric midpoint — the hyperbola arc dips toward its
            // center relative to the chord, the same winding-bulge role as
            // the arc/ellipse midpoints.
            let p1 = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
            let nu = [normal.x, normal.y, normal.z];
            let mr = [major_axis.x, major_axis.y, major_axis.z];
            if let (Some(t0), Some(t1)) = (
                geom::hyperbola_param(center, nu, mr, semi_conjugate, p0),
                geom::hyperbola_param(center, nu, mr, semi_conjugate, p1),
            ) {
                pts.push(geom::hyperbola_point_at(
                    center,
                    nu,
                    mr,
                    semi_transverse,
                    semi_conjugate,
                    0.5 * (t0 + t1),
                ));
            }
        }
    }
    Ok(pts)
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
            if std::env::var_os("KV2_PLANARITY_PROBE").is_some() {
                for p in arena.loop_points(lid)? {
                    let d = (p.x() - plane.point.x()) * plane.normal.x
                        + (p.y() - plane.point.y()) * plane.normal.y
                        + (p.z() - plane.point.z()) * plane.normal.z;
                    eprintln!(
                        "[planarity-probe] face={f:?} loop={lid:?} p=({:.17e},{:.17e},{:.17e}) \
                         d={d:.3e} band={:.3e} viol={} plane n=({:.17},{:.17},{:.17})",
                        p.x(),
                        p.y(),
                        p.z(),
                        planarity_band(p),
                        d.abs() > planarity_band(p),
                        plane.normal.x,
                        plane.normal.y,
                        plane.normal.z,
                    );
                }
            }
            for p in arena.loop_points(lid)? {
                let d = (p.x() - plane.point.x()) * plane.normal.x
                    + (p.y() - plane.point.y()) * plane.normal.y
                    + (p.z() - plane.point.z()) * plane.normal.z;
                if d.abs() > planarity_band(p) {
                    return Err(KernelV2Error::NonPlanarFace { face: f });
                }
            }
            // Circle/arc centers on the plane; endpoints on their circles.
            // Full circles keep the exact-construction band; arcs (imported
            // yang output, PR-KV5b) use the import band.
            let hes = arena.loop_half_edges(lid)?;
            for &h in &hes {
                let he = arena.half_edge(h)?;
                // PR-KV9: ellipse arcs check center-on-plane + endpoints on
                // the ellipse (frame residual scaled by the minor radius,
                // the conservative in-plane length conversion) at the import
                // band, then continue — the circle logic below is
                // radius-based and does not apply.
                if let Curve::EllipseArc {
                    center,
                    normal,
                    major_axis,
                    major_radius,
                    minor_radius,
                } = he.curve
                {
                    let band = import_band(major_radius, center);
                    let d = (center.x() - plane.point.x()) * plane.normal.x
                        + (center.y() - plane.point.y()) * plane.normal.y
                        + (center.z() - plane.point.z()) * plane.normal.z;
                    if d.abs() > band {
                        return Err(KernelV2Error::NonPlanarFace { face: f });
                    }
                    let nu = [normal.x, normal.y, normal.z];
                    let mr = [major_axis.x, major_axis.y, major_axis.z];
                    for p in [
                        arena.vertex(he.origin)?.point,
                        arena.vertex(arena.half_edge(he.next)?.origin)?.point,
                    ] {
                        let w = [
                            nu[1] * mr[2] - nu[2] * mr[1],
                            nu[2] * mr[0] - nu[0] * mr[2],
                            nu[0] * mr[1] - nu[1] * mr[0],
                        ];
                        let dv = [p.x() - center.x(), p.y() - center.y(), p.z() - center.z()];
                        let u = (dv[0] * mr[0] + dv[1] * mr[1] + dv[2] * mr[2]) / major_radius;
                        let v = (dv[0] * w[0] + dv[1] * w[1] + dv[2] * w[2]) / minor_radius;
                        let band = import_band(major_radius, p);
                        let residual = (u.hypot(v) - 1.0).abs() * minor_radius;
                        if residual > band {
                            return Err(vertex_off_surface(
                                f,
                                "planar-ellipse-arc-endpoint",
                                p,
                                residual,
                                band,
                                &format!(
                                    "plane; ellipse center=({:.17e},{:.17e},{:.17e}) \
                                     major_r={major_radius:.17e} minor_r={minor_radius:.17e}",
                                    center.x(),
                                    center.y(),
                                    center.z()
                                ),
                            ));
                        }
                    }
                    continue;
                }
                // KV16: hyperbola arcs check center-on-plane + endpoints on
                // the branch (first-order in-plane distance + out-of-plane
                // component, `geom::hyperbola_branch_residual`) at the
                // import band, then continue.
                if let Curve::HyperbolaArc {
                    center,
                    normal,
                    major_axis,
                    semi_transverse,
                    semi_conjugate,
                } = he.curve
                {
                    let band = import_band(semi_transverse.max(semi_conjugate), center);
                    let d = (center.x() - plane.point.x()) * plane.normal.x
                        + (center.y() - plane.point.y()) * plane.normal.y
                        + (center.z() - plane.point.z()) * plane.normal.z;
                    if d.abs() > band {
                        return Err(KernelV2Error::NonPlanarFace { face: f });
                    }
                    let nu = [normal.x, normal.y, normal.z];
                    let mr = [major_axis.x, major_axis.y, major_axis.z];
                    for p in [
                        arena.vertex(he.origin)?.point,
                        arena.vertex(arena.half_edge(he.next)?.origin)?.point,
                    ] {
                        let (in_plane, out_of_plane, u) = geom::hyperbola_branch_residual(
                            center,
                            nu,
                            mr,
                            semi_transverse,
                            semi_conjugate,
                            p,
                        );
                        let band = import_band(semi_transverse.max(semi_conjugate), p);
                        if u <= 0.0 || in_plane > band || out_of_plane.abs() > band {
                            return Err(vertex_off_surface(
                                f,
                                "planar-hyperbola-arc-endpoint",
                                p,
                                in_plane.max(out_of_plane.abs()),
                                band,
                                &format!(
                                    "plane; hyperbola center=({:.17e},{:.17e},{:.17e}) \
                                     a={semi_transverse:.17e} b={semi_conjugate:.17e} u={u:.3e}",
                                    center.x(),
                                    center.y(),
                                    center.z()
                                ),
                            ));
                        }
                    }
                    continue;
                }
                let (center, radius, is_arc) = match he.curve {
                    Curve::Circle { center, radius, .. } => (center, radius, false),
                    Curve::Arc { center, radius, .. } => (center, radius, true),
                    // EllipseArc/HyperbolaArc handled (and continued) above.
                    Curve::LineSegment | Curve::EllipseArc { .. } | Curve::HyperbolaArc { .. } => {
                        continue
                    }
                    // M5 K8: a transversal quadric-pair curve is never
                    // planar — degenerate configurations produce conics
                    // upstream in ssi-rs. Placement on a plane face is a
                    // defect, typed and loud.
                    Curve::SurfacePair { .. } => {
                        return Err(KernelV2Error::CurvedGeometryMismatch {
                            face: f,
                            reason: "surface-pair edge on a planar face (a transversal \
                                     quadric-pair curve is never planar)",
                        });
                    }
                };
                let plane_band = if is_arc {
                    import_band(radius, center)
                } else {
                    planarity_band(center)
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
                        return Err(vertex_off_surface(
                            f,
                            if is_arc {
                                "planar-arc-endpoint"
                            } else {
                                "planar-circle-anchor"
                            },
                            p,
                            (dr - radius).abs(),
                            band,
                            &format!(
                                "plane; circle center=({:.17e},{:.17e},{:.17e}) r={radius:.17e}",
                                center.x(),
                                center.y(),
                                center.z()
                            ),
                        ));
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
        let cyl_desc = || {
            format!(
                "cylinder axis_point=({:.17e},{:.17e},{:.17e}) \
                 axis=({:.17e},{:.17e},{:.17e}) r={radius:.17e}",
                axis_point.x(),
                axis_point.y(),
                axis_point.z(),
                axis_dir.x,
                axis_dir.y,
                axis_dir.z
            )
        };
        for p in arena.loop_points(face.outer_loop)? {
            if (dist_to_axis(p) - radius).abs() > CURVED_SURFACE_DEBUG_TOLERANCE {
                return Err(vertex_off_surface(
                    f,
                    "cyl-canonical-vertex",
                    p,
                    (dist_to_axis(p) - radius).abs(),
                    CURVED_SURFACE_DEBUG_TOLERANCE,
                    &cyl_desc(),
                ));
            }
        }
        for &(c, _, _) in &rims {
            if dist_to_axis(c) > CURVED_SURFACE_DEBUG_TOLERANCE {
                return Err(vertex_off_surface(
                    f,
                    "cyl-rim-center-off-axis",
                    c,
                    dist_to_axis(c),
                    CURVED_SURFACE_DEBUG_TOLERANCE,
                    &cyl_desc(),
                ));
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
                    return Err(vertex_off_surface(
                        f,
                        "cyl-seam-not-ruling",
                        p0,
                        off,
                        CURVED_SURFACE_DEBUG_TOLERANCE * len.max(1.0),
                        &cyl_desc(),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Validate a [`Surface::Cone`] face (KV6c increment 1).
///
/// Structurally the curved analog of [`validate_cylinder_face`], but the
/// full-circle rims sit at DIFFERENT radii — each rim radius must equal
/// `τ · tan(half_angle)` for its axial coordinate `τ = (center − apex) ·
/// axis_dir` (the on-cone relation, [`geom::cone_radius_at`]). Two accepted
/// forms: the canonical FRUSTUM band (exactly two full-circle rims) and the
/// KV6-slice-2B APEX form (a single closed base rim; the apex is an interior
/// singular point, and only the outward solid sense has a producer). No
/// inner loops, no arc edges — arc-patch cones (boolean output) reject
/// loudly here and land in a later increment.
///
/// Orientation: rim circle normals run along the axis, and each rim's
/// traversal axis points TOWARD the opposite rim for an outward (solid)
/// frustum (`reversed == false`) — the same material-sense convention as the
/// cylinder lateral, so the swept frustum built by the KV6c revolve
/// (increment 4) validates by the same rule the cylinder sweep already obeys.
#[allow(clippy::too_many_arguments)]
fn validate_cone_face(
    arena: &BrepArena,
    f: FaceId,
    face: &crate::arena::Face,
    apex: Point3,
    axis_dir: crate::arena::UnitVector3,
    half_angle: f64,
    reversed: bool,
) -> Result<(), KernelV2Error> {
    let mismatch = |reason: &'static str| KernelV2Error::CurvedGeometryMismatch { face: f, reason };
    if !half_angle.is_finite() || half_angle <= 0.0 || half_angle >= std::f64::consts::FRAC_PI_2 {
        return Err(mismatch("cone half_angle must be finite in (0, π/2)"));
    }
    let alen = (axis_dir.x * axis_dir.x + axis_dir.y * axis_dir.y + axis_dir.z * axis_dir.z).sqrt();
    if (alen - 1.0).abs() > NORMAL_AGREEMENT_TOLERANCE {
        return Err(mismatch("cone axis_dir must be unit-length"));
    }

    // Vocabulary dispatch (KV6c increment 5, mirroring the cylinder): any
    // full-circle edge anywhere → the canonical frustum-band / apex forms
    // below; NO full-circle edge → the partial arc-bounded patch (the
    // partial-revolve oblique wall).
    let mut all_loops = vec![face.outer_loop];
    all_loops.extend(face.inner_loops.iter().copied());
    let mut has_full = false;
    for &lid in &all_loops {
        if !loop_circles(arena, &arena.loop_half_edges(lid)?)?.is_empty() {
            has_full = true;
        }
    }
    if !has_full {
        return validate_cone_patch(arena, f, face, apex, axis_dir, half_angle, reversed);
    }

    // Canonical frustum band only (increment 1).
    if !face.inner_loops.is_empty() {
        return Err(mismatch(
            "cone face with inner loops is outside the KV6c vocabulary",
        ));
    }
    let hes = arena.loop_half_edges(face.outer_loop)?;
    if !loop_arcs(arena, &hes)?.is_empty() {
        return Err(mismatch("cone face mixes full-circle rims with arc edges"));
    }
    let rims = loop_circles(arena, &hes)?;

    // Axial coordinate τ = (center − apex) · axis_dir.
    let tau = |c: Point3| {
        (c.x() - apex.x()) * axis_dir.x
            + (c.y() - apex.y()) * axis_dir.y
            + (c.z() - apex.z()) * axis_dir.z
    };

    // KV6 slice 2B: the APEX form — a single closed base rim, the apex an
    // interior singular point (yang's own cone model). Only the outward
    // solid sense has a producer (`build_on_axis_apex_cone`); a `reversed`
    // apex cavity is outside the vocabulary, typed.
    let apex_form = rims.len() == 1 && hes.len() == 1;
    if apex_form {
        if reversed {
            return Err(mismatch(
                "apex-cone cavity (reversed) is outside the KV6c vocabulary",
            ));
        }
    } else if rims.len() != 2 {
        return Err(mismatch(
            "cone face must be bounded by exactly two full-circle rims (KV6c)",
        ));
    }
    for (i, &(c, nu, r)) in rims.iter().enumerate() {
        let t = tau(c);
        if !t.is_finite() || t <= 0.0 {
            return Err(mismatch("cone rim lies at or behind the apex"));
        }
        let expected = geom::cone_radius_at(t, half_angle);
        if (r - expected).abs() > 1e-9 * expected.max(1.0) {
            return Err(mismatch(
                "rim circle radius disagrees with the cone surface",
            ));
        }
        if geom::dot(nu, axis_dir).abs() < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
            return Err(mismatch("rim circle normal must be along the cone axis"));
        }
        if apex_form {
            // Material sense: the base rim's traversal axis points TOWARD
            // the apex (τ decreasing) — the apex analog of "toward the
            // opposite rim".
            if geom::dot(nu, axis_dir) >= 0.0 {
                return Err(mismatch(
                    "rim traversal axis disagrees with the apex cone's material sense",
                ));
            }
            continue;
        }
        let other = rims[1 - i].0;
        let toward =
            (other.x() - c.x()) * nu.x + (other.y() - c.y()) * nu.y + (other.z() - c.z()) * nu.z;
        // Outward frustum: each rim's traversal axis points TOWARD the
        // opposite rim; cavity (reversed) bore wall: AWAY (see the cylinder
        // analog in `validate_cylinder_face`).
        if (!reversed && toward <= 0.0) || (reversed && toward >= 0.0) {
            return Err(mismatch(
                "rim traversal axis disagrees with the cone's material sense",
            ));
        }
    }

    #[cfg(debug_assertions)]
    {
        let on_cone_residual = |p: Point3| {
            let d = [p.x() - apex.x(), p.y() - apex.y(), p.z() - apex.z()];
            let t = d[0] * axis_dir.x + d[1] * axis_dir.y + d[2] * axis_dir.z;
            let radial = [
                d[0] - t * axis_dir.x,
                d[1] - t * axis_dir.y,
                d[2] - t * axis_dir.z,
            ];
            let rho =
                (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
            (rho - geom::cone_radius_at(t, half_angle)).abs()
        };
        for p in arena.loop_points(face.outer_loop)? {
            if on_cone_residual(p) > CURVED_SURFACE_DEBUG_TOLERANCE {
                return Err(vertex_off_surface(
                    f,
                    "cone-vertex",
                    p,
                    on_cone_residual(p),
                    CURVED_SURFACE_DEBUG_TOLERANCE,
                    &format!(
                        "cone apex=({:.17e},{:.17e},{:.17e}) half_angle={half_angle:.17e}",
                        apex.x(),
                        apex.y(),
                        apex.z()
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Invariants 4+5 for a PARTIAL cone patch (KV6c increment 5, spec
/// `kv6c_partial_revolve_cone_patch.md` §3 I3): boundary loops of
/// [`Curve::Arc`] and [`Curve::LineSegment`] edges — the partial-revolve
/// oblique wall and, later, yang boolean outputs. This is
/// [`validate_cylinder_patch`]'s unrolled-winding orientation analysis in
/// the cone's (θ, τ) development (a cone is developable; the same
/// material-CCW Newell generalization applies): τ = (p − apex) · axis_dir
/// replaces the axial height, and per-arc surface agreement checks the
/// on-cone relation `r_arc = τ_c · tan(half_angle)` instead of a constant
/// radius. Ellipse arcs (oblique cone sections) are outside this slice —
/// typed and loud.
#[allow(clippy::too_many_arguments)]
fn validate_cone_patch(
    arena: &BrepArena,
    f: FaceId,
    face: &crate::arena::Face,
    apex: Point3,
    axis_dir: crate::arena::UnitVector3,
    half_angle: f64,
    reversed: bool,
) -> Result<(), KernelV2Error> {
    use std::f64::consts::PI;
    let mismatch = |reason: &'static str| KernelV2Error::CurvedGeometryMismatch { face: f, reason };
    let a = [axis_dir.x, axis_dir.y, axis_dir.z];
    let ap = [apex.x(), apex.y(), apex.z()];
    // The mirror sense: a conical bore wall (reversed) is validated in the
    // mirrored frame u = −θ, where its boundary winds material-CCW again.
    let sense = if reversed { -1.0 } else { 1.0 };

    let mut all_loops = vec![face.outer_loop];
    all_loops.extend(face.inner_loops.iter().copied());

    // Shared angular frame anchored at the first outer-loop vertex's radial
    // direction; returns (θ, τ) with τ the axial coordinate FROM THE APEX.
    let radial_theta_tau = |p: Point3, e1: [f64; 3], e2: [f64; 3]| -> Option<(f64, f64)> {
        let d = [p.x() - ap[0], p.y() - ap[1], p.z() - ap[2]];
        let tau = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
        let r = [d[0] - tau * a[0], d[1] - tau * a[1], d[2] - tau * a[2]];
        let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        if !(rl.is_finite() && rl > 0.0) {
            return None;
        }
        let x = r[0] * e1[0] + r[1] * e1[1] + r[2] * e1[2];
        let y = r[0] * e2[0] + r[1] * e2[1] + r[2] * e2[2];
        Some((y.atan2(x), tau))
    };
    let first_hes = arena.loop_half_edges(face.outer_loop)?;
    if first_hes.is_empty() {
        return Err(mismatch("cone patch with an empty boundary loop"));
    }
    let p0 = arena.vertex(arena.half_edge(first_hes[0])?.origin)?.point;
    let d0 = [p0.x() - ap[0], p0.y() - ap[1], p0.z() - ap[2]];
    let t0 = d0[0] * a[0] + d0[1] * a[1] + d0[2] * a[2];
    let r0 = [d0[0] - t0 * a[0], d0[1] - t0 * a[1], d0[2] - t0 * a[2]];
    let r0l = (r0[0] * r0[0] + r0[1] * r0[1] + r0[2] * r0[2]).sqrt();
    if !(r0l.is_finite() && r0l > 0.0) {
        return Err(mismatch("cone patch anchor vertex lies on the axis"));
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
            return Err(mismatch("cone patch loop with fewer than 3 edges"));
        }
        let mut us: Vec<f64> = Vec::with_capacity(hes.len());
        let mut vs: Vec<f64> = Vec::with_capacity(hes.len());
        let mut u_cur = f64::NAN; // set from the first vertex below
        let mut total = 0.0f64;
        for (i, &h) in hes.iter().enumerate() {
            let he = arena.half_edge(h)?;
            let p = arena.vertex(he.origin)?.point;
            let q = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
            let Some((theta_p, tau_p)) = radial_theta_tau(p, e1, e2) else {
                return Err(mismatch("cone patch vertex lies on the axis"));
            };
            if i == 0 {
                u_cur = theta_p;
            }
            us.push(u_cur);
            vs.push(tau_p);

            let delta = match he.curve {
                Curve::LineSegment => {
                    let Some((theta_q, _)) = radial_theta_tau(q, e1, e2) else {
                        return Err(mismatch("cone patch vertex lies on the axis"));
                    };
                    geom::wrap_to_pi(theta_q - theta_p)
                }
                Curve::Arc {
                    center,
                    normal,
                    radius: r_arc,
                } => {
                    // Production-tier per-arc surface agreement: axis-parallel
                    // circle at τ_c > 0 whose radius satisfies the on-cone
                    // relation r = τ_c · tan(half_angle).
                    let nd = geom::dot(normal, axis_dir);
                    if nd.abs() < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
                        return Err(mismatch(
                            "patch arc's circle axis is not parallel to the cone axis",
                        ));
                    }
                    let tau_c = (center.x() - ap[0]) * a[0]
                        + (center.y() - ap[1]) * a[1]
                        + (center.z() - ap[2]) * a[2];
                    if !(tau_c.is_finite() && tau_c > 0.0) {
                        return Err(mismatch("patch arc lies at or behind the apex"));
                    }
                    let expected = geom::cone_radius_at(tau_c, half_angle);
                    if (r_arc - expected).abs() > 1e-9 * expected.max(1.0) {
                        return Err(mismatch("patch arc radius disagrees with the cone surface"));
                    }
                    #[cfg(debug_assertions)]
                    {
                        // Arc center on the axis (import band — see
                        // `validate_cylinder_patch`'s arc-center check).
                        let dc = [center.x() - ap[0], center.y() - ap[1], center.z() - ap[2]];
                        let hc = dc[0] * a[0] + dc[1] * a[1] + dc[2] * a[2];
                        let rc = [dc[0] - hc * a[0], dc[1] - hc * a[1], dc[2] - hc * a[2]];
                        let off = (rc[0] * rc[0] + rc[1] * rc[1] + rc[2] * rc[2]).sqrt();
                        if off > import_band(r_arc, center) {
                            return Err(vertex_off_surface(
                                f,
                                "conepatch-arc-center-off-axis",
                                center,
                                off,
                                import_band(r_arc, center),
                                &format!(
                                    "cone apex=({:.17e},{:.17e},{:.17e}) \
                                     axis=({:.17e},{:.17e},{:.17e}) half_angle={half_angle:.17e}",
                                    ap[0], ap[1], ap[2], a[0], a[1], a[2]
                                ),
                            ));
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
                // KV16: a conic section piece ON this cone (EllipseArc =
                // the oblique plane∩cone section, HyperbolaArc = the
                // axis-steep one) advances the walk by its endpoint
                // azimuths, exactly like the SurfacePair arm below —
                // boolean-output pieces are sub-facet sized (Δθ far below
                // π). A cone-section ellipse has no constant-radius axis-⊥
                // projection (unlike the cylinder-section ellipse above),
                // so the parametric-sweep shortcut does NOT apply — the
                // endpoint-azimuth walk is the honest advance.
                Curve::EllipseArc { .. } | Curve::HyperbolaArc { .. } => {
                    let Some((theta_q, _)) = radial_theta_tau(q, e1, e2) else {
                        return Err(mismatch("cone patch vertex lies on the axis"));
                    };
                    geom::wrap_to_pi(theta_q - theta_p)
                }
                Curve::Circle { .. } => {
                    // Unreachable: the dispatcher sends full-circle faces to
                    // the canonical path. Loud, defensively.
                    return Err(mismatch("full-circle edge inside a partial cone patch"));
                }
                // M5: a surface-pair boundary piece advances the walk by its
                // endpoint azimuths (endpoint-determined traversal; the
                // on-surface certification is invariant 1b, and the endpoint
                // residual against THIS face's surface is the shared
                // off-surface sweep below).
                Curve::SurfacePair { .. } => {
                    let Some((theta_q, _)) = radial_theta_tau(q, e1, e2) else {
                        return Err(mismatch("cone patch vertex lies on the axis"));
                    };
                    geom::wrap_to_pi(theta_q - theta_p)
                }
            };
            u_cur += delta;
            total += delta;
        }
        let wraps_f = total / (2.0 * PI);
        let wraps = wraps_f.round();
        if (wraps_f - wraps).abs() > 1e-3 {
            return Err(mismatch(
                "cone patch loop's net axis winding is not integral",
            ));
        }
        let wraps = wraps as i64;
        if wraps.abs() > 1 {
            return Err(mismatch("cone patch loop wraps the axis more than once"));
        }
        let m = us.len();
        let mut area2 = 0.0f64;
        for i in 0..m {
            let j = (i + 1) % m;
            area2 += us[i] * vs[j] - us[j] * vs[i];
        }
        measures.push(LoopMeasure {
            loop_id: lid,
            wrap: if sense < 0.0 { -wraps } else { wraps },
            mean_h: vs.iter().sum::<f64>() / m as f64,
            area2: sense * area2,
        });
    }

    // ---- face-level orientation rules (material-CCW in the developed frame)
    let wrapping: Vec<&LoopMeasure> = measures.iter().filter(|mm| mm.wrap != 0).collect();
    match wrapping.len() {
        0 => {
            // Bounded patch: exactly one CCW (material) loop, others CW
            // (windows).
            let mut positive = 0usize;
            for mm in &measures {
                if mm.area2 == 0.0 {
                    return Err(mismatch("cone patch loop has zero developed area"));
                }
                if mm.area2 > 0.0 {
                    positive += 1;
                }
            }
            if positive != 1 {
                return Err(mismatch(
                    "bounded cone patch must have exactly one material-CCW loop",
                ));
            }
        }
        2 => {
            // Band segment: a +1 and a −1 wrap, the +1 at the lower axial
            // coordinate (the cylinder barrel rule with τ for height);
            // windows wind CW.
            let (w0, w1) = (wrapping[0], wrapping[1]);
            if w0.wrap + w1.wrap != 0 {
                return Err(mismatch("cone patch wrapping loops do not wind oppositely"));
            }
            let (plus, minus) = if w0.wrap > 0 { (w0, w1) } else { (w1, w0) };
            if plus.mean_h >= minus.mean_h {
                return Err(mismatch(
                    "cone patch wrapping loops are oriented away from the material",
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
                "cone patch must have exactly 0 or 2 axis-wrapping loops",
            ));
        }
    }

    // ---- debug-tier geometric tripwire: loop vertices on the surface ------
    #[cfg(debug_assertions)]
    {
        for &lid in &all_loops {
            for p in arena.loop_points(lid)? {
                let d = [p.x() - ap[0], p.y() - ap[1], p.z() - ap[2]];
                let tau = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
                let r = [d[0] - tau * a[0], d[1] - tau * a[1], d[2] - tau * a[2]];
                let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
                let expected = geom::cone_radius_at(tau.max(0.0), half_angle);
                if (rl - expected).abs() > import_band(expected.max(rl), p) {
                    return Err(vertex_off_surface(
                        f,
                        "conepatch-vertex",
                        p,
                        (rl - expected).abs(),
                        import_band(expected.max(rl), p),
                        &format!(
                            "cone apex=({:.17e},{:.17e},{:.17e}) \
                             axis=({:.17e},{:.17e},{:.17e}) half_angle={half_angle:.17e}",
                            ap[0], ap[1], ap[2], a[0], a[1], a[2]
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Validate a [`Surface::Torus`] face (KV6d increment 1 — foundation).
///
/// Checks the analytic parameters (a ring torus needs `major > minor > 0` and a
/// unit axis) and, in debug builds, that every loop vertex (outer + inner) lies
/// on the torus surface via [`geom::torus_residual`]. The detailed boundary
/// topology (profile-circle rims + longitude seam arcs for a partial torus, or
/// the seam loops of a full torus) is pinned and exercised end to end when the
/// KV6d revolve constructor (increment 3) produces it; this foundation
/// validator is deliberately topology-agnostic so it accepts whatever shape the
/// constructor settles on while still guarding the surface geometry.
fn validate_torus_face(
    arena: &BrepArena,
    f: FaceId,
    face: &crate::arena::Face,
    center: Point3,
    axis_dir: crate::arena::UnitVector3,
    major_radius: f64,
    minor_radius: f64,
) -> Result<(), KernelV2Error> {
    let mismatch = |reason: &'static str| KernelV2Error::CurvedGeometryMismatch { face: f, reason };
    if !minor_radius.is_finite() || minor_radius <= 0.0 {
        return Err(mismatch("torus minor_radius must be finite and positive"));
    }
    if !major_radius.is_finite() || major_radius <= minor_radius {
        return Err(mismatch(
            "torus major_radius must be finite and exceed minor_radius (ring torus)",
        ));
    }
    let alen = (axis_dir.x * axis_dir.x + axis_dir.y * axis_dir.y + axis_dir.z * axis_dir.z).sqrt();
    if (alen - 1.0).abs() > NORMAL_AGREEMENT_TOLERANCE {
        return Err(mismatch("torus axis_dir must be unit-length"));
    }

    #[cfg(debug_assertions)]
    {
        let on_torus_residual = |p: Point3| {
            let d = [p.x() - center.x(), p.y() - center.y(), p.z() - center.z()];
            let tau = d[0] * axis_dir.x + d[1] * axis_dir.y + d[2] * axis_dir.z;
            let radial = [
                d[0] - tau * axis_dir.x,
                d[1] - tau * axis_dir.y,
                d[2] - tau * axis_dir.z,
            ];
            let rho =
                (radial[0] * radial[0] + radial[1] * radial[1] + radial[2] * radial[2]).sqrt();
            geom::torus_residual(tau, rho, major_radius, minor_radius).abs()
        };
        let mut loops = vec![face.outer_loop];
        loops.extend(face.inner_loops.iter().copied());
        for lid in loops {
            for p in arena.loop_points(lid)? {
                // The residual is in length², so compare against a band scaled
                // by the minor radius (a length·length tolerance).
                if on_torus_residual(p) > CURVED_SURFACE_DEBUG_TOLERANCE * minor_radius.max(1.0) {
                    return Err(vertex_off_surface(
                        f,
                        "torus-vertex",
                        p,
                        on_torus_residual(p),
                        CURVED_SURFACE_DEBUG_TOLERANCE * minor_radius.max(1.0),
                        &format!(
                            "torus center=({:.17e},{:.17e},{:.17e}) \
                             major_r={major_radius:.17e} minor_r={minor_radius:.17e}",
                            center.x(),
                            center.y(),
                            center.z()
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Validate a [`Surface::Sphere`] face (KV6d increment 2, spec
/// `kv6d_sphere_revolve.md`).
///
/// Checks the analytic parameters (finite `radius > 0`, finite center) and,
/// in debug builds, that every loop vertex (outer + inner) lies on the
/// sphere via [`geom::sphere_residual`]. Deliberately topology-agnostic
/// (the torus-validator precedent): it accepts both the closed seam-arc
/// loop the revolve constructor builds and boolean-output trimmed patches.
fn validate_sphere_face(
    arena: &BrepArena,
    f: FaceId,
    face: &crate::arena::Face,
    center: Point3,
    radius: f64,
) -> Result<(), KernelV2Error> {
    let mismatch = |reason: &'static str| KernelV2Error::CurvedGeometryMismatch { face: f, reason };
    if !radius.is_finite() || radius <= 0.0 {
        return Err(mismatch("sphere radius must be finite and positive"));
    }
    if !(center.x().is_finite() && center.y().is_finite() && center.z().is_finite()) {
        return Err(mismatch("sphere center must be finite"));
    }

    #[cfg(debug_assertions)]
    {
        // `sphere_residual` is a plain length; scale the band by the radius
        // (a length·length tolerance, matching the torus convention).
        let band = CURVED_SURFACE_DEBUG_TOLERANCE * radius.max(1.0);
        let mut loops = vec![face.outer_loop];
        loops.extend(face.inner_loops.iter().copied());
        for lid in loops {
            for p in arena.loop_points(lid)? {
                let res = geom::sphere_residual(p, center, radius).abs();
                if res > band {
                    return Err(vertex_off_surface(
                        f,
                        "sphere-vertex",
                        p,
                        res,
                        band,
                        &format!(
                            "sphere center=({:.17e},{:.17e},{:.17e}) radius={radius:.17e}",
                            center.x(),
                            center.y(),
                            center.z()
                        ),
                    ));
                }
            }
        }
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = (arena, face);
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
                            return Err(vertex_off_surface(
                                f,
                                "cylpatch-arc-center-off-axis",
                                center,
                                off,
                                import_band(radius, center),
                                &format!(
                                    "cylinder axis_point=({:.17e},{:.17e},{:.17e}) \
                                     axis=({:.17e},{:.17e},{:.17e}) r={radius:.17e}",
                                    ap[0], ap[1], ap[2], a[0], a[1], a[2]
                                ),
                            ));
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
                Curve::EllipseArc {
                    center,
                    normal,
                    major_axis,
                    major_radius,
                    minor_radius,
                } => {
                    // PR-KV9: oblique-section arc on this cylinder. The
                    // azimuth advance equals the SIGNED parametric sweep:
                    // the axis-⊥ projection of a cylinder-section ellipse is
                    // the radius-r circle itself (minor radius = r, minor
                    // direction ⊥ axis), so Δazimuth = s_w·Δt with
                    // s_w = sign((n̂×m̂)·(â×ê1)) the frame handedness.
                    if (minor_radius - radius).abs() > 1e-9 * (1.0 + radius) {
                        return Err(mismatch(
                            "patch ellipse-arc minor radius disagrees with the surface",
                        ));
                    }
                    let nu = [normal.x, normal.y, normal.z];
                    let mr = [major_axis.x, major_axis.y, major_axis.z];
                    let m_dot_a = mr[0] * a[0] + mr[1] * a[1] + mr[2] * a[2];
                    let e1r = [
                        mr[0] - m_dot_a * a[0],
                        mr[1] - m_dot_a * a[1],
                        mr[2] - m_dot_a * a[2],
                    ];
                    let e1l = (e1r[0] * e1r[0] + e1r[1] * e1r[1] + e1r[2] * e1r[2]).sqrt();
                    if e1l < 1e-12 {
                        return Err(mismatch(
                            "patch ellipse-arc major axis parallel to the cylinder axis",
                        ));
                    }
                    let e1v = [e1r[0] / e1l, e1r[1] / e1l, e1r[2] / e1l];
                    let e2v = [
                        a[1] * e1v[2] - a[2] * e1v[1],
                        a[2] * e1v[0] - a[0] * e1v[2],
                        a[0] * e1v[1] - a[1] * e1v[0],
                    ];
                    let w = [
                        nu[1] * mr[2] - nu[2] * mr[1],
                        nu[2] * mr[0] - nu[0] * mr[2],
                        nu[0] * mr[1] - nu[1] * mr[0],
                    ];
                    let s_w = if w[0] * e2v[0] + w[1] * e2v[1] + w[2] * e2v[2] >= 0.0 {
                        1.0
                    } else {
                        -1.0
                    };
                    let Some(sweep) =
                        geom::ellipse_ccw_sweep(center, nu, mr, major_radius, minor_radius, p, q)
                    else {
                        return Err(mismatch("patch ellipse-arc endpoint degenerate"));
                    };
                    s_w * sweep
                }
                Curve::Circle { .. } => {
                    // Unreachable: the dispatcher sends full-circle faces to
                    // the canonical path. Loud, defensively.
                    return Err(mismatch("full-circle edge inside a partial cylinder patch"));
                }
                // KV16: a plane∩cylinder section is never a hyperbola — its
                // presence on a cylinder patch is a defect (no producer),
                // typed and loud.
                Curve::HyperbolaArc { .. } => {
                    return Err(mismatch(
                        "hyperbola arc on a cylinder patch (a plane∩cylinder section is \
                         never a hyperbola)",
                    ));
                }
                // M5: a surface-pair boundary piece advances the walk by its
                // endpoint azimuths (endpoint-determined traversal; on-curve
                // certification is invariant 1b, per-vertex surface agreement
                // is the shared off-surface sweep).
                Curve::SurfacePair { .. } => {
                    let Some((theta_q, _)) = radial_theta_h(q, e1, e2) else {
                        return Err(mismatch("cylinder patch vertex lies on the axis"));
                    };
                    geom::wrap_to_pi(theta_q - theta_p)
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
            // Diagnostic probe (env-gated, zero-cost off): dump the per-loop
            // wrap/height/area measures so a wrapping-count wall
            // self-localizes (which loop is unexpected, and where).
            if std::env::var_os("KV2_CYLPATCH_PROBE").is_some() {
                eprintln!(
                    "[cylpatch-probe] face {f:?} radius={radius} axis_point={ap:?} axis={a:?} \
                     wrapping={} loops={}",
                    wrapping.len(),
                    measures.len()
                );
                for mm in &measures {
                    eprintln!(
                        "  loop {:?} wrap={} mean_h={} area2={}",
                        mm.loop_id, mm.wrap, mm.mean_h, mm.area2
                    );
                    if let Ok(hes) = arena.loop_half_edges(mm.loop_id) {
                        for &h in &hes {
                            if let Ok(he) = arena.half_edge(h) {
                                let p = arena.vertex(he.origin).map(|v| v.point);
                                eprintln!("    he {h:?} curve={:?} origin={p:?}", he.curve);
                            }
                        }
                    }
                }
            }
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
                    return Err(vertex_off_surface(
                        f,
                        "cylpatch-vertex",
                        p,
                        (rl - radius).abs(),
                        import_band(radius, p),
                        &format!(
                            "cylinder axis_point=({:.17e},{:.17e},{:.17e}) \
                             axis=({:.17e},{:.17e},{:.17e}) r={radius:.17e}",
                            ap[0], ap[1], ap[2], a[0], a[1], a[2]
                        ),
                    ));
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
            // Cone laterals (like cylinders) have no polygonal walk to
            // Newell-check; `validate_cone_face` validates the rim geometry.
            (Some(Surface::Cone { .. }), false) => {}
            // Torus faces (KV6d) likewise have no polygonal walk;
            // `validate_torus_face` validates the surface geometry.
            (Some(Surface::Torus { .. }), false) => {}
            // Sphere faces (KV6d increment 2) likewise have no polygonal
            // walk; `validate_sphere_face` validates the surface geometry.
            (Some(Surface::Sphere { .. }), false) => {}
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

#[cfg(test)]
mod cone_tests {
    use crate::arena::UnitVector3;
    use crate::cone_fixtures::build_frustum;
    use crate::error::KernelV2Error;
    use cad_primitives::Point3;
    use std::f64::consts::{FRAC_PI_3, FRAC_PI_4};

    use super::validate_solid;

    const PLUS_Z: UnitVector3 = UnitVector3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };

    #[test]
    fn frustum_with_matching_half_angle_validates() {
        // 45° frustum, apex at the origin: rims at radii 1 and 2.
        let (arena, solid, _lat) = build_frustum(
            Point3::new(0.0, 0.0, 0.0),
            PLUS_Z,
            1.0,
            2.0,
            FRAC_PI_4,
            FRAC_PI_4,
        );
        let report = validate_solid(&arena, solid).expect("45° frustum must validate");
        assert_eq!(report.faces, 3);
        assert_eq!(report.vertices, 2);
        assert_eq!(report.edges, 3);
    }

    #[test]
    fn frustum_with_wrong_half_angle_is_rejected() {
        // Geometry is 45° but the stored cone claims 60°: the rim radii no
        // longer satisfy τ·tan(half_angle), so validation rejects it loudly.
        let (arena, solid, _lat) = build_frustum(
            Point3::new(0.0, 0.0, 0.0),
            PLUS_Z,
            1.0,
            2.0,
            FRAC_PI_4,
            FRAC_PI_3,
        );
        let err = validate_solid(&arena, solid).expect_err("mismatched half-angle must fail");
        assert!(
            matches!(err, KernelV2Error::CurvedGeometryMismatch { .. }),
            "expected CurvedGeometryMismatch, got {err:?}"
        );
    }

    /// KV6 slice 2B: a `reversed` APEX cone (a conical cavity mouth) has no
    /// producer and is outside the vocabulary — typed rejection, never a
    /// silently-accepted inverted solid.
    #[test]
    fn reversed_apex_cone_is_rejected() {
        use crate::arena::Surface;
        use crate::construct::revolve;
        use crate::profile::Profile;
        use cad_primitives::{Point2, Vector3};

        let triangle = Profile::new(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(3.0, 0.0),
                Point2::new(0.0, 2.0),
            ],
            vec![],
        )
        .expect("apex triangle");
        let mut arena = crate::arena::BrepArena::new();
        let r = revolve(
            &mut arena,
            &triangle,
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            2.0 * std::f64::consts::PI,
        )
        .expect("apex cone builds");
        // Flip the stored material sense: the arena now claims a cavity.
        let lat = r.walls[0];
        if let Some(face) = arena.faces[lat.0 as usize].as_mut() {
            if let Some(Surface::Cone { reversed, .. }) = face.surface.as_mut() {
                *reversed = true;
            }
        }
        let err = validate_solid(&arena, r.solid).expect_err("reversed apex cone must fail");
        assert!(
            matches!(err, KernelV2Error::CurvedGeometryMismatch { .. }),
            "expected CurvedGeometryMismatch, got {err:?}"
        );
    }
}
