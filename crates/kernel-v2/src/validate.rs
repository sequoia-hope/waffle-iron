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
//! 4. **Surface presence + planarity (debug builds only)** — every face
//!    carries `Some(Surface)`; in debug builds every loop vertex lies within
//!    [`PLANARITY_DEBUG_TOLERANCE`] of the face plane.
//! 5. **Newell normal consistency** — each face's stored unit normal matches
//!    the normalized Newell normal of its outer loop; every inner loop
//!    (ring) winds opposite (its Newell normal is antiparallel to the face
//!    normal).
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
    BrepArena, FaceId, HalfEdgeId, LoopBoundary, LoopId, LoopKind, SolidId, Surface, VertexId,
};
use crate::error::KernelV2Error;
use crate::geom;

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

    // ---- invariants 4+5: surface presence, planarity (debug), Newell -----
    for &f in &face_ids {
        let face = arena.face(f)?;
        let Some(Surface::Plane(plane)) = face.surface else {
            return Err(KernelV2Error::FaceWithoutSurface { face: f });
        };

        // Debug-tier planarity tripwire (see module docs): every vertex of
        // every loop of the face within tolerance of the stored plane.
        #[cfg(debug_assertions)]
        {
            let mut loops = vec![face.outer_loop];
            loops.extend(face.inner_loops.iter().copied());
            for lid in loops {
                for p in arena.loop_points(lid)? {
                    let d = (p.x() - plane.point.x()) * plane.normal.x
                        + (p.y() - plane.point.y()) * plane.normal.y
                        + (p.z() - plane.point.z()) * plane.normal.z;
                    if d.abs() > PLANARITY_DEBUG_TOLERANCE {
                        return Err(KernelV2Error::NonPlanarFace { face: f });
                    }
                }
            }
        }

        // Stored normal ≡ Newell(outer loop) — hard rule 2.
        let pts = arena.loop_points(face.outer_loop)?;
        let Some(newell) = geom::newell_unit(&pts) else {
            return Err(KernelV2Error::NewellMismatch { face: f });
        };
        if geom::dot(plane.normal, newell) < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
            return Err(KernelV2Error::NewellMismatch { face: f });
        }

        // Rings wind opposite to the outer loop.
        for &rid in &face.inner_loops {
            let ring_pts = arena.loop_points(rid)?;
            if ring_pts.is_empty() {
                continue; // lone-vertex ring has no winding
            }
            let rn = geom::newell(&ring_pts);
            let d = rn[0] * plane.normal.x + rn[1] * plane.normal.y + rn[2] * plane.normal.z;
            if d >= 0.0 {
                return Err(KernelV2Error::RingWindingMismatch { face: f, ring: rid });
            }
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
    // Face back-pointers + Newell construction invariant.
    for (i, slot) in arena.faces.iter().enumerate() {
        let Some(face) = slot else { continue };
        let f = FaceId(i as u32);
        let pts = arena.loop_points(face.outer_loop)?;
        match (geom::newell_unit(&pts), &face.surface) {
            (Some(newell), Some(Surface::Plane(plane))) => {
                if geom::dot(plane.normal, newell) < 1.0 - NORMAL_AGREEMENT_TOLERANCE {
                    return Err(KernelV2Error::NewellMismatch { face: f });
                }
            }
            (None, None) => {}
            _ => return Err(KernelV2Error::NewellMismatch { face: f }),
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
