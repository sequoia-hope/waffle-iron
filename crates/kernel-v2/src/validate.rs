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
//! predicate is available in this crate. For CONSTRUCTOR-path solids that
//! is acceptable: planarity of constructed faces is **guaranteed by
//! construction** (the Euler operators create geometry only at
//! caller-supplied points, and the KV2 consumers — planar profile →
//! extrude — supply coplanar points by construction), so the f64 residual
//! check below is a *debug-tier tripwire for construction-sequence bugs*,
//! compiled only under `debug_assertions`.
//!
//! For BOOLEAN-path solids that rationale is FALSE — yang output re-enters
//! with real-scale f64 noise, and the F0064/R0051 class (task #146) shipped
//! "planar" faces with off-plane loop vertices that passed the production
//! orientation checks. Since design review 2026-07-12 F1, boolean outputs
//! are additionally gated by the PRODUCTION-tier
//! [`validate_boolean_output_planarity`] (called from
//! `boolean::boolean_op`), at the scale-relative
//! [`PLANARITY_BOOLEAN_OUTPUT_TOLERANCE`] band.

use std::collections::{BTreeMap, BTreeSet};

use crate::arena::{
    BrepArena, Curve, FaceId, HalfEdgeId, LoopBoundary, LoopId, LoopKind, SolidId, Surface,
    VertexId,
};
use crate::error::KernelV2Error;
use crate::geom;
use cad_primitives::Point3;

mod faces;
pub(crate) use faces::*;

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
/// only absorbs normalization rounding (= the central
/// [`cad_primitives::TAU_EVAL`] rounding tier, F8).
pub const NORMAL_AGREEMENT_TOLERANCE: f64 = cad_primitives::TAU_EVAL;

/// PRODUCTION-tier planarity band for BOOLEAN-PATH solids, as a RELATIVE
/// band — multiplied by `(1 + max|coordinate|)` at the check site, like
/// [`PLANARITY_DEBUG_TOLERANCE`].
///
/// Design review 2026-07-12 F1: the debug tripwire's "planar by
/// construction" rationale is FALSE for boolean outputs, which re-enter
/// from the yang mesh pipeline carrying real-scale f64 noise. The
/// F0064/R0051/F0067 class (task #146) produced "planar" output faces whose
/// loop vertices sat measurably off the face plane while the AVERAGED
/// Newell normal stayed within [`NORMAL_AGREEMENT_TOLERANCE`] — so
/// production validation passed and the defect surfaced downstream as
/// tessellation self-intersections. This gate makes the class loud at the
/// boolean boundary instead.
///
/// Value: `1e-9` relative — the same import-band tier as
/// [`YANG_NORMAL_AGREEMENT`](crate::boolean) and `recover::BAND`
/// (f64-construction/normalization rounding, to be centralized as the named
/// `TAU_EVAL` tier per design-review F8). Healthy yang output is exact to
/// ~`TAU_WORK` (1e-12) after Stage-4 relocation and rational vertex
/// canonicalization, so this is ≥1000× above legitimate noise, while
/// defect-class residuals (≥ `MIN_FEATURE_SIZE`-scale) exceed it by ≥1000×.
/// A reject here is a REJECT (typed `NonPlanarFace`), never a snap or a
/// repair (P9).
pub const PLANARITY_BOOLEAN_OUTPUT_TOLERANCE: f64 = cad_primitives::TAU_EVAL;

/// Production planarity gate for solids assembled from yang boolean output
/// (see [`PLANARITY_BOOLEAN_OUTPUT_TOLERANCE`]). Checks every loop vertex of
/// every PLANAR face against its face plane at the boolean-output band.
/// Constructor-path solids are exempt by design — their planarity is
/// guaranteed by construction and guarded by the debug tripwire.
///
/// Called by [`crate::boolean::boolean_op`] on every assembled boolean
/// output (the disjoint-union shell-merge path is exempt: it reuses the
/// operands' constructor-validated shells bit-for-bit).
pub fn validate_boolean_output_planarity(
    arena: &BrepArena,
    solid: SolidId,
) -> Result<(), KernelV2Error> {
    let solid_ref = arena.solid(solid)?;
    for &sh in &solid_ref.shells {
        let shell = arena.shell(sh)?;
        for &f in &shell.faces {
            let face = arena.face(f)?;
            let Some(Surface::Plane(plane)) = &face.surface else {
                continue;
            };
            let mut loops = vec![face.outer_loop];
            loops.extend(face.inner_loops.iter().copied());
            for lid in loops {
                for p in arena.loop_points(lid)? {
                    // Signed distance from the stored face plane — the same
                    // residual as the debug tripwire, at the boolean-output
                    // band. Invariant: every loop vertex of a planar output
                    // face lies ON the face plane (a violation is a yang
                    // emission defect, not noise — reject, never repair).
                    let d = (p.x() - plane.point.x()) * plane.normal.x
                        + (p.y() - plane.point.y()) * plane.normal.y
                        + (p.z() - plane.point.z()) * plane.normal.z;
                    let band = PLANARITY_BOOLEAN_OUTPUT_TOLERANCE
                        * (1.0 + p.x().abs().max(p.y().abs()).max(p.z().abs()));
                    if d.abs() > band {
                        if std::env::var_os("KV2_PLANARITY_PROBE").is_some() {
                            eprintln!(
                                "[boolean-planarity-gate] face={f:?} loop={lid:?} \
                                 p=({:.17e},{:.17e},{:.17e}) d={d:.3e} band={band:.3e} \
                                 plane.n=({:.6},{:.6},{:.6}) plane.pt=({:.6},{:.6},{:.6})",
                                p.x(),
                                p.y(),
                                p.z(),
                                plane.normal.x,
                                plane.normal.y,
                                plane.normal.z,
                                plane.point.x(),
                                plane.point.y(),
                                plane.point.z(),
                            );
                        }
                        return Err(KernelV2Error::NonPlanarFace { face: f });
                    }
                }
            }
        }
    }
    Ok(())
}

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
