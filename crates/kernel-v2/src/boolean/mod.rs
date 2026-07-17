//! Boolean delegation to yang-rs (PR-KV3 planar, PR-KV5b curved).
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
//! - [`to_yang_brep`] — kernel-v2 arena solid → `yang_rs::BRep`. Planar
//!   faces convert per-loop as in PR-KV3. CANONICAL cylinder solids
//!   (PR-KV5b): the KV5a vertex-anchored topology converts to exactly the
//!   yang-rs M5 fixture shape — SHARED full-circle rim edges (each
//!   referenced by its cap loop and the lateral loop, normal = the cap's
//!   outward normal) and a shared seam edge appearing twice in the lateral
//!   loop. Sharing is REQUIRED: yang Stage 1 builds one cached rim ring per
//!   circle edge index and the lateral stitches to the caps' rings — split
//!   edges would tessellate non-watertight. PARTIAL curved faces (arc
//!   edges, non-canonical laterals — the shape a previous curved boolean
//!   produced) cannot re-enter yang Stage 1 (`BRep::new` requires laterals
//!   with exactly 2 full circle rims) and are rejected with the typed
//!   [`KernelV2Error::UnsupportedCurvedBoolean`].
//! - [`from_yang_brep`] — yang-rs *output* `BRep` → kernel-v2 solid.
//!   PR-KV5b vocabulary, established by the survey
//!   (`tests/kv5b_survey.rs`): `Plane` + `Cylinder` surfaces (`reversed`
//!   only on cylinder cavity walls), `LineSegment` + `Circle` edges where
//!   `start != end` circles are MINOR ARCS of the exact intersection
//!   circles ([`crate::arena::Curve::Arc`]), plus the full
//!   (`start == end`) circles of the canonical M5 shape for the mechanical
//!   round-trip. Anything else — `Ellipse`/`Parabola`/`Hyperbola` (oblique
//!   sections), near-half-circle arcs (minor side ambiguous) — is the
//!   typed, NAMED [`KernelV2Error::UnsupportedBooleanOutputCurve`];
//!   `Sphere`/`Cone` surfaces stay
//!   [`KernelV2Error::UnsupportedBooleanOutputSurface`].
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
//! `validate_solid` (twin pairing, loop closure, vertex fans, Newell and
//! its curved analogs, ring winding, Euler–Poincaré) as defense in depth —
//! the result meets exactly the same bar as an Euler-built solid.
//!
//! ## Circle-edge sense derivation (PR-KV5b)
//!
//! yang's `Curve::Circle` normal is the circle's plane normal; it does NOT
//! encode traversal direction, while kernel-v2's [`Curve`] normals are
//! directional (the curved orientation source of truth). The conversion
//! derives the direction:
//!
//! - **Arc edges**: the geometry IS the minor arc (each arrangement mesh
//!   edge subtends one facet, far below π). The CCW sweep around the
//!   stored normal from `start` to `end` decides the directional normal
//!   (`< π` → as stored, `> π` → negated); a sweep within
//!   [`ARC_MINOR_AMBIGUITY_BAND`] of π is rejected loudly rather than
//!   guessed.
//! - **Full-circle edges**: the planar (cap) use takes the stored normal
//!   sign-adjusted to the face's plane normal (`+` for an outer loop, `−`
//!   for a ring — the KV5a cap convention); the cylinder-lateral use takes
//!   the exact negation. Underivable configurations (both uses on cylinder
//!   faces) are rejected loudly.
//!
//! ## Error mapping (loud, typed)
//!
//! - Coplanar input pairs in the unsupported Stage-0 residue surface as the
//!   typed [`KernelV2Error::UnsupportedCoplanar`] (see [`map_yang_error`]).
//! - Every other yang-rs failure surfaces as
//!   [`KernelV2Error::BooleanFailed`] carrying the yang error's full
//!   Display text — including the cylinder×cylinder Stage-3 SSI wall
//!   (`SsiRefinementFailed`/AmbiguousCurve: the lateral∩lateral
//!   intersection is a degree-4 space curve with no analytic conic). No
//!   masking, no retry, no tolerance fallback (P9/P10).
//! - An empty result (e.g. intersection of disjoint solids) is the typed
//!   [`KernelV2Error::EmptyBooleanResult`] — kernel-v2 has no empty solid.

use std::collections::BTreeMap;

use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    PairSurface, Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use crate::construct::finalize_solid;
use crate::error::KernelV2Error;
use crate::geom;
use cad_primitives::{BoolOp, Point3, Vector3};

/// Tolerance on `1 − dot(Newell(loop), yang_plane_normal)` for the
/// cross-check that a yang output face's stated plane agrees with its
/// boundary walk. Same bar as `validate::NORMAL_AGREEMENT_TOLERANCE` —
/// both vectors are unit-length; only normalization rounding is absorbed.
/// (= the central [`cad_primitives::TAU_EVAL`] rounding tier, F8.)
const YANG_NORMAL_AGREEMENT_TOLERANCE: f64 = cad_primitives::TAU_EVAL;

/// Sweep band (radians) around π inside which an arc's minor side is
/// declared ambiguous and rejected (`UnsupportedBooleanOutputCurve`)
/// rather than guessed. Arrangement mesh edges subtend ≈ one Stage-1 facet
/// (2π/8 .. 2π/16 on the surveyed corpus), orders of magnitude below π, so
/// the band never fires on in-scope geometry — it exists to make the
/// minor-arc assumption a CHECKED precondition, not a silent one.
pub const ARC_MINOR_AMBIGUITY_BAND: f64 = 1e-6;

mod canonicalize;
use self::canonicalize::{canonicalize_sibling_planes, canonicalize_vertices_to_planes};

mod to_yang;
pub use self::to_yang::{to_yang_brep, to_yang_brep_indexed};

mod from_yang;
pub use self::from_yang::{from_yang_brep, from_yang_brep_indexed};

// ---------------------------------------------------------------------------
// small vector helpers (component math local to this module)
// ---------------------------------------------------------------------------

fn sub(a: Point3, b: Point3) -> [f64; 3] {
    [a.x() - b.x(), a.y() - b.y(), a.z() - b.z()]
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm3(a: [f64; 3]) -> f64 {
    dot3(a, a).sqrt()
}

fn normalize3_arr(a: [f64; 3]) -> [f64; 3] {
    let n = norm3(a);
    [a[0] / n, a[1] / n, a[2] / n]
}

fn neg_unit(n: UnitVector3) -> UnitVector3 {
    UnitVector3 {
        x: -n.x,
        y: -n.y,
        z: -n.z,
    }
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
/// for an empty result, [`KernelV2Error::UnsupportedCurvedBoolean`] for
/// curved input shapes that cannot re-enter yang Stage 1 (partial patches
/// from a previous curved boolean), [`KernelV2Error::BooleanFailed`] for
/// any other yang-rs failure (loud Display text — including the
/// cylinder×cylinder Stage-3 SSI wall), plus the [`from_yang_brep`]
/// reassembly errors (named curve/surface walls).
pub fn boolean_op(
    arena: &mut BrepArena,
    a: SolidId,
    b: SolidId,
    op: BoolOp,
) -> Result<SolidId, KernelV2Error> {
    // Spec `kv2_multishell_boolean_operands` (KV7-F2): multi-shell operands
    // — disjoint lumps, interlocking lumps, AND internal voids — are
    // admitted. `to_yang_brep_indexed` emits every shell into one
    // multi-component BRep, and the pipeline is component- and
    // cavity-agnostic: the Cherchi 2022 §2.4/§5 in/out labeling is
    // ray-cast parity against each whole input mesh (a cavity-interior
    // point crosses two boundaries and classifies OUTSIDE), the Stage-4
    // Euler gate is per connected shell, and `from_yang_brep` assembles
    // multi-component outputs into multi-shell solids (production since
    // the disjoint-union path). The former PR-KV7 wall
    // (`UnsupportedMultiShellBoolean`) pinned a stale "reassembly cannot
    // rebuild voids" claim and is deleted.
    let (ya, a_faces) = to_yang_brep_indexed(arena, a)?;
    let (yb, b_faces) = to_yang_brep_indexed(arena, b)?;
    // Task #134 (spec `yang_disjoint_union_passthrough`): a UNION of
    // strictly AABB-disjoint operands (yang's own predicate — beyond the
    // YR24 weld band, conservative curved bounds) is the DISJOINT SUM.
    // Merge the shells at the ARENA level: every face/edge/curve of both
    // operands is preserved bit-for-bit (yang's passthrough output is
    // INPUT-convention topology, which `from_yang_brep` does not ingest,
    // so the merge happens here instead). Lineage: every operand face is
    // `Same` (identity — the faces ARE the output faces).
    if op == BoolOp::Union && yang_rs::union_operands_strictly_disjoint(&ya, &yb) {
        let mut shells = arena.solid(a)?.shells.clone();
        shells.extend(arena.solid(b)?.shells.iter().copied());
        let new_id = SolidId(arena.solids.len() as u32);
        arena.solids.push(Some(Solid {
            shells: shells.clone(),
        }));
        for sh in shells {
            arena.shell_mut(sh)?.solid = new_id;
        }
        {
            use crate::journal::{EvoKind, Evolution, OpTag};
            let modified: Vec<_> = a_faces
                .iter()
                .chain(b_faces.iter())
                .filter_map(|&fid| arena.face_pid(fid))
                .map(|pid| (pid, pid, EvoKind::Same))
                .collect();
            arena.journal.push(Evolution {
                op: OpTag::Boolean(op),
                generated: Vec::new(),
                modified,
                deleted: Vec::new(),
            });
        }
        return Ok(new_id);
    }
    if std::env::var_os("KV2_PLANE_TRACE").is_some() {
        for (tag, y) in [("A", &ya), ("B", &yb)] {
            for (i, f) in y.faces().iter().enumerate() {
                if let yang_rs::Surface::Plane { normal, d } = f.surface {
                    let n = normal.as_array();
                    eprintln!(
                        "[plane-trace] input {tag} face {i} n=({:.17e},{:.17e},{:.17e}) d={d:.17e}",
                        n[0], n[1], n[2]
                    );
                }
            }
        }
    }
    let Some(backend) = yang_rs::native_backend() else {
        // Unreachable since cherchi-rs M7c (the backend is always available),
        // kept as a loud arm rather than an unwrap (P9, no-panic rule).
        return Err(KernelV2Error::BooleanFailed(
            "yang-rs native backend unavailable".to_string(),
        ));
    };
    let out = yang_rs::boolean(&ya, &yb, op, &backend).map_err(map_yang_error)?;
    if std::env::var_os("KV2_PLANE_TRACE").is_some() {
        for (i, f) in out.faces().iter().enumerate() {
            if let yang_rs::Surface::Plane { normal, d } = f.surface {
                let n = normal.as_array();
                eprintln!(
                    "[plane-trace] OUTPUT face {i} n=({:.17e},{:.17e},{:.17e}) d={d:.17e}",
                    n[0], n[1], n[2]
                );
            }
        }
    }
    // KV9-F3 diagnosis probe (read-only, env-gated): census near-twin
    // vertex pairs in yang's OUTPUT B-Rep, with incident-edge context —
    // localizes output-identity defects to the yang side vs the assembler.
    if std::env::var_os("KV2_OUT_TWIN_PROBE").is_some() {
        let vs: Vec<_> = out.vertices().iter().map(|v| v.point).collect();
        let scale = vs
            .iter()
            .flat_map(|p| p.as_array())
            .fold(1.0_f64, |m, c| m.max(c.abs()));
        let band = 1.0e-9 * scale;
        for i in 0..vs.len() {
            for j in (i + 1)..vs.len() {
                let (p, q) = (vs[i].as_array(), vs[j].as_array());
                let d2 = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
                if d2 <= band * band {
                    eprintln!(
                        "[out-twin-probe] verts {i}/{j} dist={:e}\n  {i}: ({},{},{})\n  {j}: ({},{},{})",
                        d2.sqrt(),
                        p[0],
                        p[1],
                        p[2],
                        q[0],
                        q[1],
                        q[2]
                    );
                    for (ei, e) in out.edges().iter().enumerate() {
                        if e.start == i as u32
                            || e.end == i as u32
                            || e.start == j as u32
                            || e.end == j as u32
                        {
                            eprintln!(
                                "    edge {ei}: {} -> {} curve {:?}",
                                e.start,
                                e.end,
                                std::mem::discriminant(&e.curve)
                            );
                        }
                    }
                }
            }
        }
    }
    let (out_solid, out_face_ids) = from_yang_brep_indexed(arena, &out)?;
    // F1 (design review 2026-07-12): PRODUCTION planarity gate for the
    // assembled boolean output. The debug-only tripwire in `validate_solid`
    // rests on "planar by construction", which is false for yang re-entry —
    // the F0064/R0051 class shipped planar faces with off-plane loop
    // vertices that evaded the (averaged) Newell orientation checks. Loud
    // typed reject at the boundary; never a snap or repair (P9).
    crate::validate::validate_boolean_output_planarity(arena, out_solid)?;
    // N6 / §4.5.4 (task #173): PRODUCTION self-intersection gate at render
    // resolution — the layer where sub-sagitta B-Rep-level penetrations
    // (the C0116 cyl×cyl graze class) become observable. Loud typed
    // reject; never a snap or trim repair (P9). See `validate::selfx`.
    crate::validate::validate_boolean_output_self_intersection(arena, out_solid)?;
    // KV13 F2: record the boolean's per-face lineage in the journal.
    record_boolean_evolution(arena, op, &out, &out_face_ids, &a_faces, &b_faces);
    Ok(out_solid)
}

/// KV13 F2: append the boolean's [`Evolution`] to the arena journal. Each
/// output face descends (via yang's per-face attribution) from an operand
/// face → a `modified` edge `(operand_pid → output_pid)`; the first output
/// face from a given operand face is `Same`, additional ones are `Split`.
/// Operand faces that produced no output are `deleted`. An output face with no
/// resolvable operand lineage is `generated` (defensive — yang attributes
/// every patch, so this is normally empty). Infallible: a missing Pid simply
/// drops that edge (no false lineage).
fn record_boolean_evolution(
    arena: &mut BrepArena,
    op: BoolOp,
    out: &yang_rs::BRep,
    out_face_ids: &[Option<FaceId>],
    a_faces: &[FaceId],
    b_faces: &[FaceId],
) {
    use crate::arena::Pid;
    use crate::journal::{EvoKind, Evolution, OpTag};
    use std::collections::BTreeSet;

    let attr = out.face_attribution();
    let mut generated: Vec<Pid> = Vec::new();
    let mut modified: Vec<(Pid, Pid, EvoKind)> = Vec::new();
    let mut claimed: BTreeSet<Pid> = BTreeSet::new();
    let mut sourced: BTreeSet<Pid> = BTreeSet::new();
    for (yidx, out_fid) in out_face_ids.iter().enumerate() {
        let Some(out_fid) = out_fid else { continue };
        let Some(out_pid) = arena.face_pid(*out_fid) else {
            continue;
        };
        let operand_pid = attr
            .get(yidx)
            .and_then(|a| {
                let faces = match a.input {
                    yang_rs::InputId::A => a_faces,
                    yang_rs::InputId::B => b_faces,
                };
                faces.get(a.face as usize).copied()
            })
            .and_then(|fid| arena.face_pid(fid));
        match operand_pid {
            Some(opid) => {
                sourced.insert(opid);
                let kind = if claimed.insert(opid) {
                    EvoKind::Same
                } else {
                    EvoKind::Split
                };
                modified.push((opid, out_pid, kind));
            }
            None => generated.push(out_pid),
        }
    }
    let mut deleted: Vec<Pid> = a_faces
        .iter()
        .chain(b_faces.iter())
        .filter_map(|&fid| arena.face_pid(fid))
        .filter(|pid| !sourced.contains(pid))
        .collect();
    deleted.sort_unstable();
    deleted.dedup();
    arena.journal.push(Evolution {
        op: OpTag::Boolean(op),
        generated,
        modified,
        deleted,
    });
}

/// Cluster shells by AABB overlap (closed intervals — touching boxes also
/// cluster; conservative). Returned in deterministic (union-find root)
/// order. Shared by [`split_solid_into_bodies`] (body decomposition) and
/// `boolean_op`'s operand admission (spec `kv2_multishell_boolean_operands`):
/// a cluster of ≥2 shells is either a lump with an internal void (nested
/// AABBs) or interlocking lumps — both stay behind the typed multi-shell
/// wall, while singleton clusters are freely re-enterable disjoint lumps.
fn shell_aabb_clusters(
    arena: &BrepArena,
    shells: &[ShellId],
) -> Result<Vec<Vec<ShellId>>, KernelV2Error> {
    // Per-shell AABB over its faces' loop vertices.
    let mut boxes: Vec<([f64; 3], [f64; 3])> = Vec::with_capacity(shells.len());
    for &sh in shells {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for &f in &arena.shell(sh)?.faces {
            let face = arena.face(f)?;
            let mut loops = vec![face.outer_loop];
            loops.extend(face.inner_loops.iter().copied());
            for lid in loops {
                for h in arena.loop_half_edges(lid)? {
                    let v = arena.half_edge(h)?.origin;
                    let p = arena.vertex(v)?.point.as_array();
                    for k in 0..3 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
            }
        }
        boxes.push((lo, hi));
    }

    // Union-find: cluster shells whose AABBs overlap (closed intervals, so
    // touching boxes also cluster — conservative).
    let n = shells.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let overlap = |a: &([f64; 3], [f64; 3]), b: &([f64; 3], [f64; 3])| {
        (0..3).all(|k| a.1[k] >= b.0[k] && b.1[k] >= a.0[k])
    };
    for i in 0..n {
        for j in (i + 1)..n {
            if overlap(&boxes[i], &boxes[j]) {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }

    // Group shells by cluster root (BTreeMap → deterministic order).
    let mut groups: BTreeMap<usize, Vec<ShellId>> = BTreeMap::new();
    for (i, &sh) in shells.iter().enumerate() {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(sh);
    }
    Ok(groups.into_values().collect())
}

/// Decompose a solid into separate body solids by grouping its shells into
/// spatially-disjoint clusters. Two shells whose axis-aligned bounding boxes
/// overlap go in the same cluster — so a void shell (its AABB nested inside the
/// lump's) stays with its lump, while genuinely disjoint lumps (e.g. a union of
/// far-apart bosses) separate. Returns the original `solid` first (rewritten to
/// hold the first cluster's shells), then one fresh solid per additional
/// cluster.
///
/// Returns `vec![solid]` unchanged when there is ≤1 shell or every shell
/// clusters together (one body, possibly with internal voids). AABB-overlap
/// clustering is deliberately conservative: it never over-splits (two truly
/// disjoint lumps with overlapping boxes stay one body) — under-splitting is a
/// benign display artifact, over-splitting would invent bodies.
pub fn split_solid_into_bodies(
    arena: &mut BrepArena,
    solid: SolidId,
) -> Result<Vec<SolidId>, KernelV2Error> {
    let shells: Vec<ShellId> = arena.solid(solid)?.shells.clone();
    if shells.len() <= 1 {
        return Ok(vec![solid]);
    }

    let groups = shell_aabb_clusters(arena, &shells)?;
    if groups.len() <= 1 {
        return Ok(vec![solid]);
    }

    // First cluster stays in the original solid; the rest get fresh solids.
    let mut result = Vec::with_capacity(groups.len());
    let mut clusters = groups.into_iter();
    let first = clusters.next().expect("groups.len() > 1");
    arena.solid_mut(solid)?.shells = first;
    result.push(solid);
    for cluster in clusters {
        let new_id = SolidId(arena.solids.len() as u32);
        arena.solids.push(Some(Solid {
            shells: cluster.clone(),
        }));
        for sh in cluster {
            arena.shell_mut(sh)?.solid = new_id;
        }
        result.push(new_id);
    }
    Ok(result)
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
