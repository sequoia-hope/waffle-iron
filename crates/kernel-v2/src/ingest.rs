//! Exact ingestion of an imported analytic shell into the arena — STEP import
//! milestone **SI5 checkpoint C3**
//! (`specs/step_import_si5_exact_analytic_ingestion.md`).
//!
//! The input is [`AnalyticShellData`]: the index tables of a `CLOSED_SHELL` as
//! the exchange file itself wrote them, with every surface and every curve
//! carrying its full analytic parameters (produced by
//! `step_import::parse_step_analytic`, SI5 C2). The output is a real arena
//! solid — no tessellation anywhere in the path, so the ingested body is a
//! first-class kernel solid rather than the mesh-backed [`crate::imported`]
//! tier.
//!
//! **C3 ingests the planar vocabulary only**: every face a `PLANE`, every edge
//! curve a `LINE`. That is 15.7 % of the ingestible subset of ABC chunk 0000
//! measured on its own (spec §2.1) and it is the share that needs no seam
//! minting and no curved-orientation law — the two things C4/C5 add. A face or
//! curve outside it is a typed, loud refusal naming the index and the entity;
//! the caller falls back to the mesh tier (C6 wires that fallback).
//!
//! ## Why a direct assembler and not an Euler sequence
//!
//! Same reason as [`crate::boolean::from_yang_brep`], whose three-pass shape
//! this module follows deliberately: the file already *is* a half-edge
//! structure in index form (two faces reference the same `edges[i]` with
//! opposite orientation), so there is no stitching, no coordinate dedup and no
//! adjacency derivation to do. An Euler path would have to invent an order in
//! which to build a topology we have been handed whole.
//!
//! - **Pass 1 validates everything before the first arena mutation**:
//!   vocabulary, loop continuity and closure, the file's own edge-sharing as
//!   the manifold pairing, the import-tier on-surface gate, exact loop
//!   winding, connectivity, and the genus back-solve.
//! - **Pass 2 assembles** — the validated input makes it infallible.
//! - **Pass 3 is the production gate**: [`finalize_solid`] (hence
//!   [`crate::validate::validate_solid`]: twin pairing, loop closure, vertex
//!   manifoldness, per-surface orientation, exact Euler–Poincaré), the
//!   self-intersection gate, and outward orientation.
//!
//! ## What this module refuses, and why that is the point
//!
//! An ingested solid is epistemically on the **boolean** path, not the
//! constructor path: its geometry is not guaranteed by construction, it is
//! *asserted by a file somebody else wrote*. So every claim the file makes is
//! checked and a disagreement is a refusal, never a repair (P9/P10):
//!
//! - A boundary vertex off its own face's plane by more than the import band
//!   is [`KernelV2Error::AnalyticVertexOffSurface`] — the measurement of spec
//!   §2.2 (residuals ~1e-13 against a 1e-9 band over 6.18 M incidences)
//!   promoted from a probe to a production gate. Not a snap.
//! - Which loop of a face is its outer boundary is MEASURED, not read: STEP
//!   marks it with a subtype and the reader loses the marker, so the
//!   determination is the exact signed area about the face's outward normal
//!   (`AnalyticFace::loops` carries that finding). A face with no such loop,
//!   or two, is a refusal — not a flip to taste (spec §5.4). The sign test's
//!   one blind spot (an inverted sense flips BOTH signs, so a holed face's
//!   ring and perimeter swap consistently) is closed by the containment fact,
//!   exactly: a face's NET signed area over all its loops is positive.
//! - An inward-oriented shell is a refusal. A `BREP_WITH_VOIDS` void arrives
//!   as its own shell today (the solid grouping is lost upstream of C2, spec
//!   §5.2), and ingesting one as a solid would be a silently inside-out body;
//!   1.0 % of the corpus goes to the mesh tier instead until C4+ captures the
//!   grouping.

use std::collections::{BTreeMap, BTreeSet};

use cad_primitives::{Point3, TAU_EVAL};
use waffle_types::kernel::{AnalyticLoop, AnalyticShellData, AnalyticSurface};

use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use crate::construct::finalize_solid;
use crate::error::KernelV2Error;
use crate::geom;

/// How far a declared surface normal may be off unit length. The extractor
/// normalizes, so this is a corrupted-input wall rather than a rounding
/// allowance — and the normal is stored with the file's own bits, never
/// renormalized here, so that coplanar faces sharing one `PLANE` entity keep
/// bit-identical normals (the plane-fidelity rule that keeps a later boolean
/// from seeing two faces of one plane as near-coplanar strangers).
const INGEST_NORMAL_TOLERANCE: f64 = TAU_EVAL;

/// The import-tier on-surface band: `TAU_EVAL·(1 + ‖p‖∞)`, the same
/// scale-relative form as the boolean-output planarity gate
/// ([`crate::validate::validate_boolean_output_planarity`]). Spec §2.2
/// measured real writers at ~1e-13, so this sits ≥1000× above the input's
/// actual residual and a trip is a finding about the file, not noise.
fn on_surface_band(p: Point3) -> f64 {
    TAU_EVAL * (1.0 + p.x().abs().max(p.y().abs()).max(p.z().abs()))
}

/// One validated loop: its owning face index, kind, the vertex cycle in walk
/// order, and the shell-edge index walked out of each of those vertices.
struct LoopPlan {
    face: usize,
    kind: LoopKind,
    /// Entry vertex of each oriented edge, in walk order.
    cycle: Vec<u32>,
    /// `edges[k]` is the shell edge traversed from `cycle[k]` to
    /// `cycle[(k+1) % n]`.
    edges: Vec<u32>,
}

/// Ingest one exact analytic shell as a solid in `arena`.
///
/// C3 vocabulary: planar faces, line edges (see module docs). Returns the new
/// [`SolidId`], or a typed refusal — on `Err` the caller must treat `arena` as
/// having gained no usable solid (pass 2 may have mutated it, exactly as the
/// boolean assembler does when its own exit validation rejects).
pub fn ingest_analytic(
    arena: &mut BrepArena,
    shell: &AnalyticShellData,
) -> Result<SolidId, KernelV2Error> {
    let probe = std::env::var_os("KV2_INGEST_PROBE").is_some();

    // ---- pass 1 (NO arena mutation): validate the file's claims -----------
    if shell.faces.is_empty() {
        return Err(KernelV2Error::InvalidAnalyticShell("shell has no faces"));
    }
    for p in &shell.vertices {
        if !(p.x().is_finite() && p.y().is_finite() && p.z().is_finite()) {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "shell vertex is not finite",
            ));
        }
    }

    // 1a. Surface vocabulary, and the outward normal the solid will store.
    //     Orientation arrives AS orientation (`same_sense`) and is applied
    //     here by negating the normal — exact in f64, so the stored plane is
    //     still the file's own geometry.
    let mut normals: Vec<[f64; 3]> = Vec::with_capacity(shell.faces.len());
    for (fi, face) in shell.faces.iter().enumerate() {
        let AnalyticSurface::Plane { normal, .. } = face.surface else {
            return Err(KernelV2Error::AnalyticIngestUnsupportedSurface {
                face: fi,
                surface: face.surface.surface_type_str(),
            });
        };
        let n = normal.as_array();
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if !(len.is_finite() && (len - 1.0).abs() <= INGEST_NORMAL_TOLERANCE) {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "a face's plane normal is not unit length",
            ));
        }
        normals.push(if face.same_sense {
            n
        } else {
            [-n[0], -n[1], -n[2]]
        });
    }

    // 1b. Curve vocabulary and edge sanity. A `LINE` edge is defined by its
    //     endpoints, so an edge whose endpoints are the same vertex, or
    //     closer together than the band in which we certify positions at all,
    //     carries no direction and is refused rather than assembled into a
    //     zero-length half-edge pair.
    for (ei, edge) in shell.edges.iter().enumerate() {
        if !matches!(edge.curve, waffle_types::kernel::AnalyticCurve::Line) {
            return Err(KernelV2Error::AnalyticIngestUnsupportedCurve {
                edge: ei,
                curve: edge.curve.curve_type_str(),
            });
        }
        let (s, e) = (edge.start as usize, edge.end as usize);
        if s >= shell.vertices.len() || e >= shell.vertices.len() {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "an edge references an out-of-range vertex",
            ));
        }
        if s == e {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "a line edge closes on its own start vertex",
            ));
        }
        let (a, b) = (shell.vertices[s], shell.vertices[e]);
        let d =
            ((b.x() - a.x()).powi(2) + (b.y() - a.y()).powi(2) + (b.z() - a.z()).powi(2)).sqrt();
        if d <= on_surface_band(a) {
            if probe {
                eprintln!(
                    "[ingest-probe] edge {ei} length {d:.3e} <= band {:.3e} \
                     ({s} -> {e})",
                    on_surface_band(a)
                );
            }
            return Err(KernelV2Error::InvalidAnalyticShell(
                "an edge is shorter than the import band, so its direction is not knowable",
            ));
        }
    }

    // 1c. Loops: kind, continuity, closure. The file states each oriented
    //     edge's direction, so this VERIFIES the chain rather than inferring
    //     it the way the boolean assembler must.
    let mut plans: Vec<LoopPlan> = Vec::new();
    for (fi, face) in shell.faces.iter().enumerate() {
        if face.loops.is_empty() {
            return Err(KernelV2Error::InvalidAnalyticShell("a face has no loops"));
        }
        for (li, lp) in face.loops.iter().enumerate() {
            let AnalyticLoop::Edges(oriented) = lp else {
                // A `VERTEX_LOOP` is a cone apex or a sphere pole: real
                // topology the arena represents (`LoopBoundary::Lone`), but
                // not in the planar tier, and C2 refuses the whole file for
                // it anyway (the reader drops it silently, spec §5.3).
                return Err(KernelV2Error::AnalyticIngestUnsupported(
                    "a face boundary is a vertex loop",
                ));
            };
            if oriented.len() < 3 {
                // Every C3 edge is a straight chord, so a 1- or 2-edge loop
                // bounds no area. (Curved tiers legitimately have 1- and
                // 2-edge loops; this wall belongs to the planar vocabulary,
                // not to the arena.)
                return Err(KernelV2Error::InvalidAnalyticShell(
                    "a loop of line edges has fewer than three edges",
                ));
            }
            let mut cycle = Vec::with_capacity(oriented.len());
            let mut edges = Vec::with_capacity(oriented.len());
            let mut cur: Option<u32> = None;
            for oe in oriented {
                let Some(edge) = shell.edges.get(oe.edge as usize) else {
                    return Err(KernelV2Error::InvalidAnalyticShell(
                        "a loop references an out-of-range edge",
                    ));
                };
                let (from, to) = if oe.forward {
                    (edge.start, edge.end)
                } else {
                    (edge.end, edge.start)
                };
                if cur.is_some_and(|c| c != from) {
                    if probe {
                        eprintln!(
                            "[ingest-probe] face {fi} loop {li} discontinuous at edge {} \
                             (expected entry {:?}, got {from})",
                            oe.edge, cur
                        );
                    }
                    return Err(KernelV2Error::InvalidAnalyticShell(
                        "a loop is not edge-continuous in the direction the file declares",
                    ));
                }
                cycle.push(from);
                edges.push(oe.edge);
                cur = Some(to);
            }
            if cur != Some(cycle[0]) {
                return Err(KernelV2Error::InvalidAnalyticShell("a loop does not close"));
            }
            plans.push(LoopPlan {
                face: fi,
                // Provisional: which loop is the outer boundary is measured in
                // 1f, because the file does not say (see `AnalyticFace::loops`).
                kind: LoopKind::Inner,
                cycle,
                edges,
            });
        }
    }

    // 1d. Manifold pairing, keyed by the file's OWN edge index. The exchange
    //     file shares one `EDGE_CURVE` between the two faces that meet along
    //     it, so identity is given rather than derived from coordinates —
    //     stronger than the boolean assembler's `(v_min, v_max, curve)` key,
    //     and it means a writer that emitted two coincident edge records
    //     instead of one is refused here (as an unpaired edge) rather than
    //     welded silently.
    let mut uses: BTreeMap<u32, Vec<(usize, usize, bool)>> = BTreeMap::new();
    for (pi, plan) in plans.iter().enumerate() {
        let m = plan.cycle.len();
        for k in 0..m {
            let edge = &shell.edges[plan.edges[k] as usize];
            let forward = plan.cycle[k] == edge.start;
            uses.entry(plan.edges[k])
                .or_default()
                .push((pi, k, forward));
        }
    }
    for (&ei, u) in &uses {
        if u.len() != 2 {
            if probe {
                eprintln!(
                    "[ingest-probe] edge {ei} has {} uses (faces {:?})",
                    u.len(),
                    u.iter().map(|&(pi, ..)| plans[pi].face).collect::<Vec<_>>()
                );
            }
            return Err(KernelV2Error::InvalidAnalyticShell(
                "an edge is not used by exactly two oriented edges",
            ));
        }
        if u[0].2 == u[1].2 {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "an edge's two uses traverse it in the same direction",
            ));
        }
    }

    // 1e. The import-tier on-surface gate (spec §8 oracle 4): every boundary
    //     vertex on its own face's plane, at the band this tier claims
    //     exactness in. A trip is a measurement about the file — recorded by
    //     the probe, refused loudly, never snapped.
    for plan in &plans {
        let fi = plan.face;
        let AnalyticSurface::Plane { origin, .. } = shell.faces[fi].surface else {
            unreachable!("pass 1a admitted planes only");
        };
        let n = normals[fi];
        for &v in &plan.cycle {
            let p = shell.vertices[v as usize];
            let d = (p.x() - origin.x()) * n[0]
                + (p.y() - origin.y()) * n[1]
                + (p.z() - origin.z()) * n[2];
            let band = on_surface_band(p);
            if d.abs() > band {
                if probe {
                    eprintln!(
                        "[ingest-probe] face {fi} vertex {v} off plane: \
                         p=({:.17e},{:.17e},{:.17e}) d={d:.3e} band={band:.3e} \
                         origin=({:.17e},{:.17e},{:.17e}) n=({},{},{})",
                        p.x(),
                        p.y(),
                        p.z(),
                        origin.x(),
                        origin.y(),
                        origin.z(),
                        n[0],
                        n[1],
                        n[2]
                    );
                }
                return Err(KernelV2Error::AnalyticVertexOffSurface { face: fi });
            }
        }
    }

    // 1f. WHICH LOOP IS THE OUTER BOUNDARY — measured, because the file does
    //     not say. STEP marks the outer bound with a subtype
    //     (`FACE_OUTER_BOUND`) and the reader collapses it into the ordinary
    //     `FACE_BOUND` table, so the marker is gone before extraction
    //     (`AnalyticFace::loops` carries the finding; 7 of 28 polyhedral ABC
    //     models have a face whose first loop is a ring). What survives is
    //     ISO 10303-42's winding law — a bound runs with the material on its
    //     left — so about the face's OUTWARD normal exactly one loop has
    //     positive exact signed area and that one is the outer boundary. The
    //     arena enforces the same rule after assembly
    //     (`validate::faces::validate_planar_face`); doing it here names the
    //     file's own face index and makes the determination, rather than
    //     trusting an order that does not exist.
    //
    //     Zero positive loops means the face's declared sense (`same_sense`)
    //     contradicts its boundary; two or more means the file's loops do not
    //     describe a single region. Both are refusals, not flips to taste
    //     (spec §5.4): the identical disagreement on a curved face is
    //     unresolvable, and a silent flip there inverts a bore wall.
    let mut outer_of_face: Vec<Option<usize>> = vec![None; shell.faces.len()];
    let mut net_area: Vec<f64> = vec![0.0; shell.faces.len()];
    for (pi, plan) in plans.iter().enumerate() {
        let pts: Vec<Point3> = plan
            .cycle
            .iter()
            .map(|&v| shell.vertices[v as usize])
            .collect();
        let curves = vec![geom::LoopEdgeCurve::Line; pts.len()];
        let area = geom::planar_loop_signed_area(normals[plan.face], &pts, &curves);
        let Some(area) = area.filter(|a| a.is_finite() && *a != 0.0) else {
            return Err(KernelV2Error::InvalidAnalyticShell(
                "a loop encloses no area in its face's plane",
            ));
        };
        if probe {
            eprintln!(
                "[ingest-probe] face {} loop {pi} ({} edges) area={area:.6e}",
                plan.face,
                plan.cycle.len()
            );
        }
        net_area[plan.face] += area;
        if area > 0.0 {
            if outer_of_face[plan.face].is_some() {
                return Err(KernelV2Error::InvalidAnalyticShell(
                    "a face has two loops that both wind as an outer boundary",
                ));
            }
            outer_of_face[plan.face] = Some(pi);
        }
    }
    for (fi, outer) in outer_of_face.iter().enumerate() {
        match outer {
            Some(pi) => plans[*pi].kind = LoopKind::Outer,
            None => {
                if probe {
                    eprintln!(
                        "[ingest-probe] face {fi} has no outer boundary: every loop winds as a \
                         ring about the outward normal {:?}",
                        normals[fi]
                    );
                }
                return Err(KernelV2Error::InvalidAnalyticShell(
                    "a face has no loop winding as its outer boundary (its declared sense \
                     contradicts its own boundary)",
                ));
            }
        }
    }

    // The sign test alone has one blind spot, and it is a SILENT WRONG rather
    // than a refusal: if a face's declared sense is inverted, both signs flip,
    // so on a face with a hole the ring comes out "positive" and the perimeter
    // "negative" — a consistent-looking swap that `validate_solid` would also
    // accept, leaving a face whose perimeter is treated as a hole. The
    // containment fact rules it out exactly and with no threshold: rings lie
    // strictly inside the outer boundary, so the face's NET signed area (every
    // loop summed) is positive. A non-positive net area means the loops cannot
    // be a perimeter with holes in it, whichever one we called outer.
    for (fi, &net) in net_area.iter().enumerate() {
        if !(net.is_finite() && net > 0.0) {
            if probe {
                eprintln!("[ingest-probe] face {fi} net signed area {net:.6e} is not positive");
            }
            return Err(KernelV2Error::InvalidAnalyticShell(
                "a face's loops do not enclose positive area (its rings do not lie inside its \
                 outer boundary, or its declared sense is inverted)",
            ));
        }
    }

    // 1g. Connectivity. ISO 10303-42 requires a closed shell to be connected,
    //     and the contract hands us one shell; a disconnected one would
    //     silently become a multi-shell solid whose shells are really
    //     separate bodies.
    if components(shell.faces.len(), &plans) > 1 {
        return Err(KernelV2Error::InvalidAnalyticShell(
            "shell is not connected",
        ));
    }

    // 1h. Genus from the Euler–Poincaré formula, back-solved the way the
    //     boolean assembler does it: `Shell::genus` is stored state with no
    //     derivation helper, and STEP states no genus at all.
    let mut vset: BTreeSet<u32> = BTreeSet::new();
    let mut rings = 0i64;
    for plan in &plans {
        if plan.kind == LoopKind::Inner {
            rings += 1;
        }
        vset.extend(plan.cycle.iter().copied());
    }
    let lhs = vset.len() as i64 - uses.len() as i64 + shell.faces.len() as i64 - rings;
    if lhs % 2 != 0 || lhs > 2 {
        if probe {
            eprintln!(
                "[ingest-probe] V={} E={} F={} R={rings} lhs={lhs}",
                vset.len(),
                uses.len(),
                shell.faces.len()
            );
        }
        return Err(KernelV2Error::InvalidAnalyticShell(
            "shell's Euler characteristic is not genus-representable",
        ));
    }
    let genus = ((2 - lhs) / 2) as u32;

    // ---- pass 2: assemble (validated input ⇒ infallible) ------------------
    // Vertices: referenced ones only, in the file's index order, so the
    // assembly is a deterministic function of the file.
    let mut vert_ids: Vec<Option<VertexId>> = vec![None; shell.vertices.len()];
    for &v in &vset {
        let id = VertexId(arena.vertices.len() as u32);
        arena.vertices.push(Some(Vertex {
            point: shell.vertices[v as usize],
        }));
        vert_ids[v as usize] = Some(id);
    }

    let solid_id = SolidId(arena.solids.len() as u32);
    arena.solids.push(Some(Solid { shells: Vec::new() }));
    let shell_id = ShellId(arena.shells.len() as u32);
    arena.shells.push(Some(Shell {
        solid: solid_id,
        faces: Vec::new(),
        genus,
    }));
    if let Some(Some(solid)) = arena.solids.get_mut(solid_id.index()) {
        solid.shells.push(shell_id);
    }

    let mut twin_table: BTreeMap<u32, HalfEdgeId> = BTreeMap::new();
    let mut face_ids: Vec<Option<FaceId>> = vec![None; shell.faces.len()];
    for plan in &plans {
        let fi = plan.face;
        let face_id = match face_ids[fi] {
            Some(id) => id,
            None => {
                let AnalyticSurface::Plane { origin, .. } = shell.faces[fi].surface else {
                    unreachable!("pass 1a admitted planes only");
                };
                let n = normals[fi];
                let id = FaceId(arena.faces.len() as u32);
                arena.faces.push(Some(Face {
                    // The file's own plane, point and normal: the ingested
                    // face is NOT re-fitted to its loop.
                    surface: Some(Surface::Plane(Plane {
                        point: origin,
                        normal: UnitVector3 {
                            x: n[0],
                            y: n[1],
                            z: n[2],
                        },
                    })),
                    outer_loop: LoopId(0), // patched below
                    inner_loops: Vec::new(),
                    shell: shell_id,
                }));
                if let Some(Some(sh)) = arena.shells.get_mut(shell_id.index()) {
                    sh.faces.push(id);
                }
                face_ids[fi] = Some(id);
                id
            }
        };

        let loop_id = LoopId(arena.loops.len() as u32);
        let m = plan.cycle.len();
        let he_base = arena.half_edges.len() as u32;
        for k in 0..m {
            let h = HalfEdgeId(he_base + k as u32);
            let twin = match twin_table.get(&plan.edges[k]) {
                Some(&other) => {
                    if let Some(Some(o)) = arena.half_edges.get_mut(other.index()) {
                        o.twin = h;
                    }
                    other
                }
                None => {
                    twin_table.insert(plan.edges[k], h);
                    h // placeholder; overwritten by the partner's visit
                }
            };
            arena.half_edges.push(Some(HalfEdge {
                twin,
                next: HalfEdgeId(he_base + ((k + 1) % m) as u32),
                prev: HalfEdgeId(he_base + ((k + m - 1) % m) as u32),
                origin: vert_ids[plan.cycle[k] as usize].expect("referenced vertex was created"),
                loop_id,
                curve: Curve::LineSegment,
            }));
        }
        arena.loops.push(Some(Loop {
            face: face_id,
            boundary: LoopBoundary::Edges(HalfEdgeId(he_base)),
            kind: plan.kind,
        }));
        if let Some(Some(face)) = arena.faces.get_mut(face_id.index()) {
            match plan.kind {
                LoopKind::Outer => face.outer_loop = loop_id,
                LoopKind::Inner => face.inner_loops.push(loop_id),
            }
        }
    }

    // ---- pass 3: production gates (spec §8) -------------------------------
    // `validate_solid` (twin pairing, loop closure, vertex manifoldness,
    // per-surface orientation, exact Euler–Poincaré) plus the face-`Pid`
    // stamp every kernel solid carries.
    finalize_solid(arena, solid_id)?;
    // Oracle 3: no two faces of the ingested body may penetrate each other.
    // An imported file can assert a self-intersecting boundary and nothing
    // upstream would notice.
    crate::validate::validate_boolean_output_self_intersection(arena, solid_id)?;
    // Outward orientation. A consistently INWARD shell passes every check
    // above — it is a perfectly good closed oriented surface — but it is
    // either a `BREP_WITH_VOIDS` void (whose solid grouping is lost upstream
    // of C2, spec §5.2) or a file whose face senses are inverted. Either way
    // ingesting it as a solid would be a silently inside-out body.
    let volume = geom::signed_volume(arena, solid_id)?;
    if !(volume.is_finite() && volume > 0.0) {
        if probe {
            eprintln!("[ingest-probe] signed volume {volume:.6e} is not positive");
        }
        return Err(KernelV2Error::InvalidAnalyticShell(
            "shell is inward-oriented (a void, or inverted face senses)",
        ));
    }

    Ok(solid_id)
}

/// Number of connected components over face indices, joined by a shared
/// shell edge. Union-find, like the boolean assembler's `face_components`,
/// but keyed by the file's edge index.
fn components(num_faces: usize, plans: &[LoopPlan]) -> usize {
    let mut parent: Vec<usize> = (0..num_faces).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let mut edge_face: BTreeMap<u32, usize> = BTreeMap::new();
    for plan in plans {
        for &ei in &plan.edges {
            match edge_face.get(&ei) {
                Some(&other) => {
                    let (ra, rb) = (find(&mut parent, plan.face), find(&mut parent, other));
                    let (lo, hi) = (ra.min(rb), ra.max(rb));
                    parent[hi] = lo;
                }
                None => {
                    edge_face.insert(ei, plan.face);
                }
            }
        }
    }
    (0..num_faces)
        .map(|f| find(&mut parent, f))
        .collect::<BTreeSet<_>>()
        .len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_primitives::Vector3;
    use waffle_types::kernel::{AnalyticCurve, AnalyticEdge, AnalyticFace, OrientedEdge};

    fn v(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    fn edge(start: u32, end: u32) -> AnalyticEdge {
        AnalyticEdge {
            start,
            end,
            curve: AnalyticCurve::Line,
        }
    }

    fn oe(edge: u32, forward: bool) -> OrientedEdge {
        OrientedEdge { edge, forward }
    }

    fn plane_face(
        origin: Point3,
        normal: Vector3,
        same_sense: bool,
        loop_edges: Vec<OrientedEdge>,
    ) -> AnalyticFace {
        AnalyticFace {
            surface: AnalyticSurface::Plane { origin, normal },
            loops: vec![AnalyticLoop::Edges(loop_edges)],
            same_sense,
        }
    }

    /// The unit box, as an exchange file would write it: 8 vertices, 12 shared
    /// edges, 6 planar faces whose outer loops run CCW about the OUTWARD
    /// normal. The top face deliberately declares the *downward* plane normal
    /// with `same_sense: false` — the shape a real file takes when the face
    /// and the surface disagree — so the ingest path's orientation handling is
    /// exercised rather than only its identity case.
    fn unit_box() -> AnalyticShellData {
        AnalyticShellData {
            vertices: vec![
                v(0.0, 0.0, 0.0),
                v(1.0, 0.0, 0.0),
                v(1.0, 1.0, 0.0),
                v(0.0, 1.0, 0.0),
                v(0.0, 0.0, 1.0),
                v(1.0, 0.0, 1.0),
                v(1.0, 1.0, 1.0),
                v(0.0, 1.0, 1.0),
            ],
            edges: vec![
                edge(0, 1),
                edge(1, 2),
                edge(2, 3),
                edge(3, 0), // 0..3  bottom ring
                edge(4, 5),
                edge(5, 6),
                edge(6, 7),
                edge(7, 4), // 4..7  top ring
                edge(0, 4),
                edge(1, 5),
                edge(2, 6),
                edge(3, 7), // 8..11 posts
            ],
            faces: vec![
                // z = 0, outward −z: 0 → 3 → 2 → 1
                plane_face(
                    v(0.0, 0.0, 0.0),
                    Vector3::new(0.0, 0.0, -1.0),
                    true,
                    vec![oe(3, false), oe(2, false), oe(1, false), oe(0, false)],
                ),
                // z = 1, outward +z, declared as −z with same_sense = false:
                // 4 → 5 → 6 → 7
                plane_face(
                    v(0.0, 0.0, 1.0),
                    Vector3::new(0.0, 0.0, -1.0),
                    false,
                    vec![oe(4, true), oe(5, true), oe(6, true), oe(7, true)],
                ),
                // y = 0, outward −y: 0 → 1 → 5 → 4
                plane_face(
                    v(0.0, 0.0, 0.0),
                    Vector3::new(0.0, -1.0, 0.0),
                    true,
                    vec![oe(0, true), oe(9, true), oe(4, false), oe(8, false)],
                ),
                // x = 1, outward +x: 1 → 2 → 6 → 5
                plane_face(
                    v(1.0, 0.0, 0.0),
                    Vector3::new(1.0, 0.0, 0.0),
                    true,
                    vec![oe(1, true), oe(10, true), oe(5, false), oe(9, false)],
                ),
                // y = 1, outward +y: 2 → 3 → 7 → 6
                plane_face(
                    v(0.0, 1.0, 0.0),
                    Vector3::new(0.0, 1.0, 0.0),
                    true,
                    vec![oe(2, true), oe(11, true), oe(6, false), oe(10, false)],
                ),
                // x = 0, outward −x: 3 → 0 → 4 → 7
                plane_face(
                    v(0.0, 0.0, 0.0),
                    Vector3::new(-1.0, 0.0, 0.0),
                    true,
                    vec![oe(3, true), oe(8, true), oe(7, false), oe(11, false)],
                ),
            ],
        }
    }

    /// Reverse every loop AND negate every face sense: a consistently
    /// INWARD-oriented shell. Every structural and winding check still passes
    /// (it is a perfectly good closed oriented surface) — only the volume
    /// sign distinguishes it.
    fn inverted(mut shell: AnalyticShellData) -> AnalyticShellData {
        for face in &mut shell.faces {
            face.same_sense = !face.same_sense;
            for lp in &mut face.loops {
                if let AnalyticLoop::Edges(oriented) = lp {
                    oriented.reverse();
                    for o in oriented.iter_mut() {
                        o.forward = !o.forward;
                    }
                }
            }
        }
        shell
    }

    #[test]
    fn a_box_ingests_as_a_validated_solid() {
        let mut arena = BrepArena::new();
        let solid = ingest_analytic(&mut arena, &unit_box()).expect("box ingests");
        let report = crate::validate::validate_solid(&arena, solid).expect("validates");
        assert_eq!(
            (
                report.vertices,
                report.edges,
                report.faces,
                report.rings,
                report.shells,
                report.genus
            ),
            (8, 12, 6, 0, 1, 0)
        );
        // Exact: the ingested geometry is the file's own numbers, so a unit
        // box integrates to exactly 1 with no rounding to allow for.
        assert_eq!(geom::signed_volume(&arena, solid).unwrap(), 1.0);
    }

    #[test]
    fn a_reversed_sense_stores_the_outward_normal_not_the_declared_one() {
        // The top face declares −z with `same_sense: false`. The arena's law
        // is that a face's stored normal points out of the material, so the
        // stored plane must be +z — and its POINT must still be the file's
        // own plane origin (plane fidelity: faces sharing one `PLANE` entity
        // keep identical bits).
        let mut arena = BrepArena::new();
        let solid = ingest_analytic(&mut arena, &unit_box()).expect("box ingests");
        let shell = arena.shell(arena.solid(solid).unwrap().shells[0]).unwrap();
        let top = shell.faces[1];
        let Some(Surface::Plane(plane)) = arena.face(top).unwrap().surface else {
            panic!("planar face expected");
        };
        assert_eq!(
            (plane.normal.x, plane.normal.y, plane.normal.z),
            (0.0, 0.0, 1.0)
        );
        assert_eq!(plane.point, v(0.0, 0.0, 1.0));
    }

    #[test]
    fn a_curved_surface_is_a_typed_capability_refusal() {
        let mut shell = unit_box();
        shell.faces[0].surface = AnalyticSurface::Cylinder {
            axis_point: v(0.0, 0.0, 0.0),
            axis_dir: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
        };
        let mut arena = BrepArena::new();
        assert_eq!(
            ingest_analytic(&mut arena, &shell),
            Err(KernelV2Error::AnalyticIngestUnsupportedSurface {
                face: 0,
                surface: "cylindrical",
            })
        );
    }

    #[test]
    fn a_curved_edge_is_a_typed_capability_refusal() {
        let mut shell = unit_box();
        shell.edges[5].curve = AnalyticCurve::Circle {
            center: v(0.0, 0.0, 0.0),
            normal: Vector3::new(0.0, 0.0, 1.0),
            radius: 1.0,
            interior: v(-1.0, 0.0, 0.0),
        };
        let mut arena = BrepArena::new();
        assert_eq!(
            ingest_analytic(&mut arena, &shell),
            Err(KernelV2Error::AnalyticIngestUnsupportedCurve {
                edge: 5,
                curve: "circle",
            })
        );
    }

    #[test]
    fn a_vertex_loop_is_a_typed_capability_refusal() {
        let mut shell = unit_box();
        shell.faces[2].loops.push(AnalyticLoop::Vertex(0));
        let mut arena = BrepArena::new();
        assert_eq!(
            ingest_analytic(&mut arena, &shell),
            Err(KernelV2Error::AnalyticIngestUnsupported(
                "a face boundary is a vertex loop"
            ))
        );
    }

    #[test]
    fn a_face_sense_contradicting_its_boundary_is_refused_not_flipped() {
        // Flip ONE face's sense and leave its loop alone: the face's only
        // loop now winds as a ring, so the face has no outer boundary. The
        // arena could be made to accept this by reversing the loop — exactly
        // the repair SI5 refuses to make (spec §5.4), because the identical
        // disagreement on a curved face is unresolvable and a silent flip
        // there inverts a bore wall.
        let mut shell = unit_box();
        shell.faces[0].same_sense = false;
        let mut arena = BrepArena::new();
        assert_eq!(
            ingest_analytic(&mut arena, &shell),
            Err(KernelV2Error::InvalidAnalyticShell(
                "a face has no loop winding as its outer boundary (its declared sense \
                     contradicts its own boundary)"
            ))
        );
    }

    #[test]
    fn an_inward_oriented_shell_is_refused_by_the_volume_gate() {
        let mut arena = BrepArena::new();
        assert_eq!(
            ingest_analytic(&mut arena, &inverted(unit_box())),
            Err(KernelV2Error::InvalidAnalyticShell(
                "shell is inward-oriented (a void, or inverted face senses)"
            ))
        );
    }

    #[test]
    fn an_unpaired_edge_is_refused() {
        let mut shell = unit_box();
        shell.faces.remove(1); // the top: its four edges now have one use
        let mut arena = BrepArena::new();
        assert_eq!(
            ingest_analytic(&mut arena, &shell),
            Err(KernelV2Error::InvalidAnalyticShell(
                "an edge is not used by exactly two oriented edges"
            ))
        );
    }

    #[test]
    fn a_discontinuous_loop_is_refused() {
        let mut shell = unit_box();
        let AnalyticLoop::Edges(oriented) = &mut shell.faces[0].loops[0] else {
            panic!("edge loop expected");
        };
        oriented.swap(1, 2);
        let mut arena = BrepArena::new();
        assert_eq!(
            ingest_analytic(&mut arena, &shell),
            Err(KernelV2Error::InvalidAnalyticShell(
                "a loop is not edge-continuous in the direction the file declares"
            ))
        );
    }

    #[test]
    fn the_on_surface_gate_admits_a_real_writer_residual_and_refuses_a_defect() {
        // Spec §2.2 measured real writers at ~1e-13 m against a 1e-9 band
        // over 6.18 M (face, vertex) incidences, so the gate must pass the
        // former and wall the latter. Vertex 6 is on the top, +x and +y
        // faces; nudging it along z leaves the two vertical planes exact and
        // takes the top face off its own plane.
        for (dz, want) in [(1e-13, true), (1e-6, false)] {
            let mut shell = unit_box();
            shell.vertices[6] = v(1.0, 1.0, 1.0 + dz);
            let mut arena = BrepArena::new();
            let got = ingest_analytic(&mut arena, &shell);
            assert_eq!(
                got.is_ok(),
                want,
                "dz = {dz:e} should {} — got {got:?}",
                if want { "ingest" } else { "be refused" }
            );
            if !want {
                assert_eq!(
                    got,
                    Err(KernelV2Error::AnalyticVertexOffSurface { face: 1 })
                );
            }
        }
    }

    #[test]
    fn a_disconnected_shell_is_refused() {
        // Two boxes in one `CLOSED_SHELL`: structurally sound, every edge
        // paired, but ISO 10303-42 requires a closed shell to be connected
        // and the two halves are really separate bodies.
        let a = unit_box();
        let mut shell = a.clone();
        let (nv, ne) = (a.vertices.len() as u32, a.edges.len() as u32);
        shell
            .vertices
            .extend(a.vertices.iter().map(|p| v(p.x() + 3.0, p.y(), p.z())));
        shell.edges.extend(a.edges.iter().map(|e| AnalyticEdge {
            start: e.start + nv,
            end: e.end + nv,
            curve: e.curve,
        }));
        shell.faces.extend(a.faces.iter().map(|f| {
            let AnalyticSurface::Plane { origin, normal } = f.surface else {
                unreachable!("box faces are planes");
            };
            AnalyticFace {
                surface: AnalyticSurface::Plane {
                    origin: v(origin.x() + 3.0, origin.y(), origin.z()),
                    normal,
                },
                loops: f
                    .loops
                    .iter()
                    .map(|lp| match lp {
                        AnalyticLoop::Edges(os) => AnalyticLoop::Edges(
                            os.iter().map(|o| oe(o.edge + ne, o.forward)).collect(),
                        ),
                        AnalyticLoop::Vertex(i) => AnalyticLoop::Vertex(i + nv),
                    })
                    .collect(),
                same_sense: f.same_sense,
            }
        }));
        let mut arena = BrepArena::new();
        assert_eq!(
            ingest_analytic(&mut arena, &shell),
            Err(KernelV2Error::InvalidAnalyticShell(
                "shell is not connected"
            ))
        );
    }

    #[test]
    fn an_empty_shell_is_refused() {
        let mut arena = BrepArena::new();
        assert_eq!(
            ingest_analytic(&mut arena, &AnalyticShellData::default()),
            Err(KernelV2Error::InvalidAnalyticShell("shell has no faces"))
        );
    }
}
