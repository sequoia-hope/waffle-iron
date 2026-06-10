//! Boolean delegation to yang-rs (PR-KV3, Phase 4a).
//!
//! kernel-v2 does NOT implement boolean labeling — that is `cherchi-rs` via
//! `yang-rs` (crate hard rule: this crate's only kernel-rewrite deps are
//! `cad-primitives` and `yang-rs`). This module is the **boundary**: it
//! converts a kernel-v2 solid to yang-rs's own `BRep` input type, invokes
//! `yang_rs::boolean` with the production native backend, and reassembles
//! yang-rs's output `BRep` into a kernel-v2 solid that passes the full
//! [`crate::validate::validate_solid`] invariant set. Per yang-rs's scope
//! rules the conversion lives HERE, on the kernel-v2 side of the boundary —
//! yang-rs never imports kernel-v2's types.
//!
//! ## The three conversion functions
//!
//! - [`to_yang_brep`] — kernel-v2 arena solid → `yang_rs::BRep` (planar
//!   faces, outer + inner loops, per-face directed edges).
//! - [`from_yang_brep`] — yang-rs *output* `BRep` → kernel-v2 solid.
//!   Planar output only in Phase 4a: a non-planar output surface is a
//!   typed, loud [`KernelV2Error::UnsupportedBooleanOutputSurface`].
//! - [`boolean_op`] — the composition: convert both inputs, run
//!   `yang_rs::boolean(a, b, op, native_backend)`, reassemble the output.
//!
//! ## Reassembly strategy: direct arena assembler (documented decision)
//!
//! `from_yang_brep` rebuilds topology with a **direct arena assembler**
//! (validated afterward by `validate_solid`), not an Euler-operator
//! sequence. Rationale: yang-rs output is a *face soup with arbitrary
//! (already-final) topology* — possibly holed faces (through-cut results),
//! genus > 0, even multiple disjoint closed shells. Deriving an Euler
//! construction schedule (which `mev` chain, where to `kemr`, which face
//! pairs to `kfmrh`) from such a soup is a graph-search problem that adds
//! no safety: the safety comes from the invariant CHECK, not from the
//! mutation path. The assembler validates the yang structure exhaustively
//! BEFORE the first arena mutation (same atomicity contract as the KV2
//! constructors), assembles, then runs the full production
//! `validate_solid` (twin pairing, loop closure, vertex fans, Newell,
//! ring winding, Euler–Poincaré) as defense in depth — the result meets
//! exactly the same bar as an Euler-built solid.
//!
//! ## Error mapping (loud, typed)
//!
//! - Coplanar input face pairs (touching or overlapping on a shared
//!   plane) are deferred by the cherchi-rs arrangement
//!   (`ArrangementError::CoplanarPairDeferred`) — Yang Stage 0 coplanar
//!   preprocessing is the M8 roadmap milestone and is NOT yet implemented.
//!   These surface as the typed [`KernelV2Error::UnsupportedCoplanar`].
//! - Every other yang-rs failure surfaces as
//!   [`KernelV2Error::BooleanFailed`] carrying the yang error's full
//!   Display text. No masking, no retry, no tolerance fallback (P9/P10).
//! - An empty result (e.g. intersection of disjoint solids) is the typed
//!   [`KernelV2Error::EmptyBooleanResult`] — kernel-v2 has no empty solid.

use std::collections::BTreeMap;

use crate::arena::{
    BrepArena, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind, Plane,
    Shell, ShellId, Solid, SolidId, Surface, Vertex, VertexId,
};
use crate::error::KernelV2Error;
use crate::geom;
use crate::validate::validate_solid;
use cad_primitives::{BoolOp, Point3, Vector3};

/// Tolerance on `1 − dot(Newell(loop), yang_plane_normal)` for the
/// cross-check that a yang output face's stated plane agrees with its
/// boundary walk. Same bar as `validate::NORMAL_AGREEMENT_TOLERANCE` —
/// both vectors are unit-length; only normalization rounding is absorbed.
const YANG_NORMAL_AGREEMENT_TOLERANCE: f64 = 1e-9;

/// Convert a kernel-v2 arena solid into yang-rs's `BRep` input type.
///
/// - Every face must carry `Some(Surface::Plane)` (the only KV surface so
///   far); the yang plane is `n·x + d = 0` with `n` the face's stored
///   (Newell-derived, outward) unit normal and `d = −n·p` for the loop's
///   first vertex `p`.
/// - Loops convert in walk order: kernel-v2's outer loops are CCW viewed
///   from outside (Newell ≡ normal) and rings wind opposite — exactly
///   yang-rs's `outer_loop` / `inner_loops` conventions.
/// - Edges are emitted per loop (per-face ownership, `Curve::LineSegment`),
///   matching yang-rs's own construction pattern; yang-rs never requires
///   shared undirected edge records.
pub fn to_yang_brep(arena: &BrepArena, solid: SolidId) -> Result<yang_rs::BRep, KernelV2Error> {
    let mut vid_map: BTreeMap<VertexId, u32> = BTreeMap::new();
    let mut yverts: Vec<yang_rs::BRepVertex> = Vec::new();
    let mut yedges: Vec<yang_rs::BRepEdge> = Vec::new();
    let mut yfaces: Vec<yang_rs::BRepFace> = Vec::new();

    let solid_ref = arena.solid(solid)?;
    for &sh in &solid_ref.shells {
        for &f in &arena.shell(sh)?.faces {
            let face = arena.face(f)?;
            let Some(Surface::Plane(plane)) = face.surface else {
                return Err(KernelV2Error::FaceWithoutSurface { face: f });
            };

            let mut convert_loop = |lid: LoopId| -> Result<Vec<u32>, KernelV2Error> {
                let hes = arena.loop_half_edges(lid)?;
                if hes.is_empty() {
                    // A lone-vertex loop has no boundary to give yang-rs.
                    return Err(KernelV2Error::NonManifoldTopology(
                        "to_yang_brep: lone-vertex loop has no edge boundary",
                    ));
                }
                // Map the loop's vertices (first-encounter order — walk
                // order — keeps the conversion deterministic).
                let mut vids = Vec::with_capacity(hes.len());
                for &h in &hes {
                    let v = arena.half_edge(h)?.origin;
                    let next = vid_map.len() as u32;
                    let yid = *vid_map.entry(v).or_insert(next);
                    if yid == next && yverts.len() == next as usize {
                        yverts.push(yang_rs::BRepVertex {
                            point: arena.vertex(v)?.point,
                        });
                    }
                    vids.push(yid);
                }
                // One directed edge per half-edge, in walk order.
                let base = yedges.len() as u32;
                let m = vids.len();
                for k in 0..m {
                    yedges.push(yang_rs::BRepEdge {
                        start: vids[k],
                        end: vids[(k + 1) % m],
                        curve: yang_rs::Curve::LineSegment,
                    });
                }
                Ok((base..base + m as u32).collect())
            };

            let outer = convert_loop(face.outer_loop)?;
            let mut inners = Vec::with_capacity(face.inner_loops.len());
            for &rid in &face.inner_loops {
                inners.push(convert_loop(rid)?);
            }

            // First outer-loop vertex anchors d so the plane passes exactly
            // through the loop geometry (not through the possibly-stale
            // `plane.point` cache).
            let first_he = arena.loop_half_edges(face.outer_loop)?[0];
            let p0 = arena.vertex(arena.half_edge(first_he)?.origin)?.point;
            let n = plane.normal;
            let d = -(n.x * p0.x() + n.y * p0.y() + n.z * p0.z());
            yfaces.push(yang_rs::BRepFace {
                surface: yang_rs::Surface::Plane {
                    normal: Vector3::new(n.x, n.y, n.z),
                    d,
                },
                outer_loop: outer,
                inner_loops: inners,
                reversed: false,
            });
        }
    }

    yang_rs::BRep::new(yverts, yedges, yfaces).map_err(|e| {
        KernelV2Error::BooleanFailed(format!("yang-rs rejected the converted input B-Rep: {e}"))
    })
}

// ---------------------------------------------------------------------------
// from_yang_brep
// ---------------------------------------------------------------------------

/// One validated loop of the yang output: owning yang face, kind, and the
/// vertex cycle in walk order (yang vertex indices).
struct LoopSpec {
    face: usize,
    kind: LoopKind,
    cycle: Vec<u32>,
}

/// Reassemble a yang-rs *output* `BRep` into a kernel-v2 solid.
///
/// Phase 4a scope: planar faces only — any other output surface is a
/// typed [`KernelV2Error::UnsupportedBooleanOutputSurface`]. The output is
/// validated structurally BEFORE the first arena mutation (loop continuity
/// and closure, exactly-two opposite directed half-edges per undirected
/// edge, orientable Newell normals agreeing with yang's stated planes),
/// assembled directly into the arena (see module docs for why a direct
/// assembler rather than an Euler sequence), split into connected shells
/// with per-shell genus derived from the Euler–Poincaré formula, and then
/// re-checked by the full [`crate::validate::validate_solid`].
pub fn from_yang_brep(
    arena: &mut BrepArena,
    brep: &yang_rs::BRep,
) -> Result<SolidId, KernelV2Error> {
    let yverts = brep.vertices();
    let yedges = brep.edges();
    let yfaces = brep.faces();

    // ---- pass 1 (NO arena mutation): validate the yang structure ---------
    if yfaces.is_empty() {
        return Err(KernelV2Error::EmptyBooleanResult);
    }

    // 1a. Planar surfaces only; planar faces never carry `reversed`.
    for (i, f) in yfaces.iter().enumerate() {
        let yang_rs::Surface::Plane { .. } = f.surface else {
            return Err(KernelV2Error::UnsupportedBooleanOutputSurface { face: i });
        };
        if f.reversed {
            return Err(KernelV2Error::InvalidBooleanOutput(
                "planar output face with reversed = true (sense belongs in the plane normal)",
            ));
        }
    }

    // 1b. Loops: ≥3 line-segment edges, directed-continuous, closed,
    //     in-range, non-degenerate.
    let mut loops: Vec<LoopSpec> = Vec::new();
    for (fi, f) in yfaces.iter().enumerate() {
        for (li, loop_edges) in std::iter::once(&f.outer_loop)
            .chain(f.inner_loops.iter())
            .enumerate()
        {
            if loop_edges.len() < 3 {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output loop with fewer than 3 edges",
                ));
            }
            let mut cycle = Vec::with_capacity(loop_edges.len());
            for (k, &ei) in loop_edges.iter().enumerate() {
                let Some(e) = yedges.get(ei as usize) else {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output loop references an out-of-range edge",
                    ));
                };
                if e.curve != yang_rs::Curve::LineSegment {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "non-line edge curve on a planar output face",
                    ));
                }
                if e.start == e.end {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "degenerate output edge (start == end)",
                    ));
                }
                if (e.start as usize) >= yverts.len() || (e.end as usize) >= yverts.len() {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output edge references an out-of-range vertex",
                    ));
                }
                let next = &yedges[loop_edges[(k + 1) % loop_edges.len()] as usize];
                if e.end != next.start {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output loop is not directed-continuous",
                    ));
                }
                cycle.push(e.start);
            }
            loops.push(LoopSpec {
                face: fi,
                kind: if li == 0 {
                    LoopKind::Outer
                } else {
                    LoopKind::Inner
                },
                cycle,
            });
        }
    }

    // 1c. Manifold edge pairing: every undirected vertex pair is used by
    //     exactly two directed loop edges, in opposite directions.
    let mut pair_uses: BTreeMap<(u32, u32), Vec<bool>> = BTreeMap::new();
    for spec in &loops {
        let m = spec.cycle.len();
        for k in 0..m {
            let (a, b) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
            let key = (a.min(b), a.max(b));
            pair_uses.entry(key).or_default().push(a < b);
        }
    }
    for uses in pair_uses.values() {
        if uses.len() != 2 || uses[0] == uses[1] {
            return Err(KernelV2Error::InvalidBooleanOutput(
                "an undirected output edge is not used by exactly two opposite directed edges",
            ));
        }
    }

    // 1d. Face orientation: outer-loop Newell normal orientable and in
    //     agreement with yang's stated plane normal; rings wind opposite.
    let mut face_normals: Vec<crate::arena::UnitVector3> = Vec::with_capacity(yfaces.len());
    {
        let mut outer_seen = vec![false; yfaces.len()];
        face_normals.resize(
            yfaces.len(),
            crate::arena::UnitVector3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
        );
        for spec in &loops {
            let pts: Vec<Point3> = spec
                .cycle
                .iter()
                .map(|&v| yverts[v as usize].point)
                .collect();
            let yang_rs::Surface::Plane { normal, .. } = yfaces[spec.face].surface else {
                unreachable!("checked in pass 1a");
            };
            match spec.kind {
                LoopKind::Outer => {
                    let Some(nu) = geom::newell_unit(&pts) else {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "output face outer loop has a degenerate (zero) Newell normal",
                        ));
                    };
                    let yn = normal.as_array();
                    let dot = nu.x * yn[0] + nu.y * yn[1] + nu.z * yn[2];
                    if dot < 1.0 - YANG_NORMAL_AGREEMENT_TOLERANCE {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "output face plane normal disagrees with its outer-loop Newell normal",
                        ));
                    }
                    face_normals[spec.face] = nu;
                    outer_seen[spec.face] = true;
                }
                LoopKind::Inner => {
                    let nw = geom::newell(&pts);
                    let yn = normal.as_array();
                    if nw[0] * yn[0] + nw[1] * yn[1] + nw[2] * yn[2] >= 0.0 {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "output face ring does not wind opposite to its outer loop",
                        ));
                    }
                }
            }
        }
        debug_assert!(
            outer_seen.iter().all(|&s| s),
            "every face has an outer loop"
        );
    }

    // 1e. Connected components over faces via shared undirected edges —
    //     one shell per component.
    let component = face_components(yfaces.len(), &loops);
    let mut shells_faces: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (fi, &c) in component.iter().enumerate() {
        shells_faces.entry(c).or_default().push(fi);
    }

    // 1f. Per-component genus from the Euler–Poincaré formula
    //     (V − E + F − R = 2 − 2g for one closed shell).
    let mut shell_genus: BTreeMap<usize, u32> = BTreeMap::new();
    for (&rep, faces) in &shells_faces {
        let mut vset: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        let mut eset: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
        let mut rings = 0i64;
        for spec in loops.iter().filter(|s| component[s.face] == rep) {
            if spec.kind == LoopKind::Inner {
                rings += 1;
            }
            let m = spec.cycle.len();
            for k in 0..m {
                let (a, b) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
                vset.insert(a);
                eset.insert((a.min(b), a.max(b)));
            }
        }
        let lhs = vset.len() as i64 - eset.len() as i64 + faces.len() as i64 - rings;
        if lhs % 2 != 0 || lhs > 2 {
            return Err(KernelV2Error::InvalidBooleanOutput(
                "output component's Euler characteristic is not genus-representable",
            ));
        }
        shell_genus.insert(rep, ((2 - lhs) / 2) as u32);
    }

    // ---- pass 2: assemble (validated input ⇒ infallible) -----------------
    // Vertices: referenced yang verts only, created in yang index order.
    let mut referenced = vec![false; yverts.len()];
    for spec in &loops {
        for &v in &spec.cycle {
            referenced[v as usize] = true;
        }
    }
    let mut vert_ids: Vec<Option<VertexId>> = vec![None; yverts.len()];
    for (i, yv) in yverts.iter().enumerate() {
        if referenced[i] {
            vert_ids[i] = Some(push_vertex(arena, yv.point));
        }
    }

    // Solid + shells (component order = ascending smallest face index).
    let solid_id = SolidId(arena.solids.len() as u32);
    arena.solids.push(Some(Solid { shells: Vec::new() }));
    let mut shell_of_face: Vec<ShellId> = vec![ShellId(0); yfaces.len()];
    for (&rep, faces) in &shells_faces {
        let shell_id = ShellId(arena.shells.len() as u32);
        arena.shells.push(Some(Shell {
            solid: solid_id,
            faces: Vec::new(),
            genus: shell_genus[&rep],
        }));
        if let Some(Some(solid)) = arena.solids.get_mut(solid_id.index()) {
            solid.shells.push(shell_id);
        }
        for &fi in faces {
            shell_of_face[fi] = shell_id;
        }
    }

    // Faces, loops, half-edges (faces in yang index order; loops outer
    // first then rings in yang order; half-edges in walk order).
    let mut twin_table: BTreeMap<(u32, u32), HalfEdgeId> = BTreeMap::new();
    let mut face_ids: Vec<Option<FaceId>> = vec![None; yfaces.len()];
    for spec in &loops {
        let fi = spec.face;
        let face_id = match face_ids[fi] {
            Some(id) => id,
            None => {
                let id = FaceId(arena.faces.len() as u32);
                let p0 = yverts[spec.cycle[0] as usize].point;
                arena.faces.push(Some(Face {
                    surface: Some(Surface::Plane(Plane {
                        point: p0,
                        normal: face_normals[fi],
                    })),
                    outer_loop: LoopId(0), // patched below
                    inner_loops: Vec::new(),
                    shell: shell_of_face[fi],
                }));
                if let Some(Some(shell)) = arena.shells.get_mut(shell_of_face[fi].index()) {
                    shell.faces.push(id);
                }
                face_ids[fi] = Some(id);
                id
            }
        };

        // The loop slot and its half-edge cycle.
        let loop_id = LoopId(arena.loops.len() as u32);
        let m = spec.cycle.len();
        let he_base = arena.half_edges.len() as u32;
        for k in 0..m {
            let (a, b) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
            let h = HalfEdgeId(he_base + k as u32);
            let key = (a.min(b), a.max(b));
            // Twin pairing: the second visitor of an undirected pair links
            // both directions (pass 1c proved exactly two opposite uses).
            let twin = match twin_table.get(&key) {
                Some(&other) => {
                    if let Some(Some(o)) = arena.half_edges.get_mut(other.index()) {
                        o.twin = h;
                    }
                    other
                }
                None => {
                    twin_table.insert(key, h);
                    h // placeholder; overwritten by the partner's visit
                }
            };
            let origin = vert_ids[a as usize].expect("referenced vertex was created");
            arena.half_edges.push(Some(HalfEdge {
                twin,
                next: HalfEdgeId(he_base + ((k + 1) % m) as u32),
                prev: HalfEdgeId(he_base + ((k + m - 1) % m) as u32),
                origin,
                loop_id,
            }));
        }
        arena.loops.push(Some(Loop {
            face: face_id,
            boundary: LoopBoundary::Edges(HalfEdgeId(he_base)),
            kind: spec.kind,
        }));
        if let Some(Some(face)) = arena.faces.get_mut(face_id.index()) {
            match spec.kind {
                LoopKind::Outer => face.outer_loop = loop_id,
                LoopKind::Inner => face.inner_loops.push(loop_id),
            }
        }
    }

    // ---- pass 3: full production validation (defense in depth) -----------
    validate_solid(arena, solid_id)?;
    Ok(solid_id)
}

fn push_vertex(arena: &mut BrepArena, point: Point3) -> VertexId {
    let id = VertexId(arena.vertices.len() as u32);
    arena.vertices.push(Some(Vertex { point }));
    id
}

/// Union-find over yang face indices, joined by shared undirected edges.
/// Returns each face's component representative (the smallest face index
/// in its component).
fn face_components(num_faces: usize, loops: &[LoopSpec]) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..num_faces).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let mut edge_face: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for spec in loops {
        let m = spec.cycle.len();
        for k in 0..m {
            let (a, b) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
            let key = (a.min(b), a.max(b));
            match edge_face.get(&key) {
                Some(&other) => {
                    let (ra, rb) = (find(&mut parent, spec.face), find(&mut parent, other));
                    let (lo, hi) = (ra.min(rb), ra.max(rb));
                    parent[hi] = lo;
                }
                None => {
                    edge_face.insert(key, spec.face);
                }
            }
        }
    }
    (0..num_faces).map(|f| find(&mut parent, f)).collect()
}

// ---------------------------------------------------------------------------
// boolean_op
// ---------------------------------------------------------------------------

/// Boolean operation on two kernel-v2 solids via the yang-rs pipeline with
/// the production native (cherchi-rs) backend.
///
/// The input solids stay live in the arena; the result is a NEW solid.
/// Error contract: [`KernelV2Error::UnsupportedCoplanar`] for coplanar
/// input face pairs (M8 boundary), [`KernelV2Error::EmptyBooleanResult`]
/// for an empty result, [`KernelV2Error::BooleanFailed`] for any other
/// yang-rs failure (loud Display text), plus the [`from_yang_brep`]
/// reassembly errors.
pub fn boolean_op(
    arena: &mut BrepArena,
    a: SolidId,
    b: SolidId,
    op: BoolOp,
) -> Result<SolidId, KernelV2Error> {
    let ya = to_yang_brep(arena, a)?;
    let yb = to_yang_brep(arena, b)?;
    let Some(backend) = yang_rs::native_backend() else {
        // Unreachable since cherchi-rs M7c (the backend is always available),
        // kept as a loud arm rather than an unwrap (P9, no-panic rule).
        return Err(KernelV2Error::BooleanFailed(
            "yang-rs native backend unavailable".to_string(),
        ));
    };
    let out = yang_rs::boolean(&ya, &yb, op, &backend).map_err(map_yang_error)?;
    from_yang_brep(arena, &out)
}

/// Map a yang-rs pipeline error to the kernel-v2 typed error contract.
///
/// The structurally-recognized cases are the M8 boundary:
/// - yang-rs's own Stage-1 NEAR-coplanar input gate (PR-YR24),
///   `YangError::CoplanarFacesUnsupported { .. }` — coplanar within the
///   sub-model-resolution band, per Yang 2025 §4.5.5 / roadmap M8;
/// - the cherchi-rs arrangement's bit-EXACT coplanar-pair deferral, nested
///   as `YangError::MeshBooleanFailed(NativeBooleanError::Arrangement(
///   ArrangementError::CoplanarPairDeferred { .. }))`.
///
/// Both map to the typed [`KernelV2Error::UnsupportedCoplanar`]. Everything
/// else is a loud [`KernelV2Error::BooleanFailed`] carrying the full
/// Display text.
fn map_yang_error(e: yang_rs::YangError) -> KernelV2Error {
    if let yang_rs::YangError::CoplanarFacesUnsupported { .. } = &e {
        return KernelV2Error::UnsupportedCoplanar;
    }
    if let yang_rs::YangError::MeshBooleanFailed(src) = &e {
        if let Some(yang_rs::NativeBooleanError::Arrangement(
            yang_rs::ArrangementError::CoplanarPairDeferred { .. },
        )) = src.downcast_ref::<yang_rs::NativeBooleanError>()
        {
            return KernelV2Error::UnsupportedCoplanar;
        }
    }
    KernelV2Error::BooleanFailed(e.to_string())
}
