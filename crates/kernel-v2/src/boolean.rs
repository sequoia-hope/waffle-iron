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
    Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
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

/// Sweep band (radians) around π inside which an arc's minor side is
/// declared ambiguous and rejected (`UnsupportedBooleanOutputCurve`)
/// rather than guessed. Arrangement mesh edges subtend ≈ one Stage-1 facet
/// (2π/8 .. 2π/16 on the surveyed corpus), orders of magnitude below π, so
/// the band never fires on in-scope geometry — it exists to make the
/// minor-arc assumption a CHECKED precondition, not a silent one.
pub const ARC_MINOR_AMBIGUITY_BAND: f64 = 1e-6;

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

fn neg_unit(n: UnitVector3) -> UnitVector3 {
    UnitVector3 {
        x: -n.x,
        y: -n.y,
        z: -n.z,
    }
}

// ---------------------------------------------------------------------------
// to_yang_brep
// ---------------------------------------------------------------------------

/// Convert a kernel-v2 arena solid into yang-rs's `BRep` input type.
///
/// - Planar faces with all-`LineSegment` loops convert per loop (per-face
///   directed edges, `Curve::LineSegment`) exactly as in PR-KV3.
/// - Canonical cylinder solids (PR-KV5b) convert to the yang M5 fixture
///   shape with SHARED rim/seam edges — see the module docs for why
///   sharing is load-bearing.
/// - Partial curved faces (arc edges, non-canonical laterals,
///   `reversed` cylinder surfaces) cannot re-enter yang Stage 1 and are
///   the typed [`KernelV2Error::UnsupportedCurvedBoolean`].
pub fn to_yang_brep(arena: &BrepArena, solid: SolidId) -> Result<yang_rs::BRep, KernelV2Error> {
    let mut vid_map: BTreeMap<VertexId, u32> = BTreeMap::new();
    let mut yverts: Vec<yang_rs::BRepVertex> = Vec::new();
    let mut yedges: Vec<yang_rs::BRepEdge> = Vec::new();
    let mut yfaces: Vec<yang_rs::BRepFace> = Vec::new();
    // Shared curved edges (rims, seams), keyed by the lower half-edge id of
    // the twin pair.
    let mut shared_edges: BTreeMap<HalfEdgeId, u32> = BTreeMap::new();

    let map_vertex = |v: VertexId,
                      vid_map: &mut BTreeMap<VertexId, u32>,
                      yverts: &mut Vec<yang_rs::BRepVertex>,
                      arena: &BrepArena|
     -> Result<u32, KernelV2Error> {
        if let Some(&id) = vid_map.get(&v) {
            return Ok(id);
        }
        let id = vid_map.len() as u32;
        vid_map.insert(v, id);
        yverts.push(yang_rs::BRepVertex {
            point: arena.vertex(v)?.point,
        });
        Ok(id)
    };

    let solid_ref = arena.solid(solid)?;
    for &sh in &solid_ref.shells {
        for &f in &arena.shell(sh)?.faces {
            let face = arena.face(f)?;
            match face.surface {
                Some(Surface::Plane(plane)) => {
                    // Classify: a cap (single full-circle outer loop, no
                    // rings) vs an all-segment planar face. Anything else
                    // curved (arcs, mixed loops, circle rings) is the
                    // re-entry wall.
                    let outer_hes = arena.loop_half_edges(face.outer_loop)?;
                    let outer_has_curved = outer_hes.iter().try_fold(false, |acc, &h| {
                        Ok::<bool, KernelV2Error>(
                            acc || !matches!(arena.half_edge(h)?.curve, Curve::LineSegment),
                        )
                    })?;
                    if outer_has_curved {
                        // Cap form: exactly one closed full-circle half-edge.
                        let [h] = outer_hes[..] else {
                            return Err(KernelV2Error::UnsupportedCurvedBoolean { face: f });
                        };
                        let he = arena.half_edge(h)?;
                        let Curve::Circle {
                            center,
                            radius,
                            normal: rim_normal,
                        } = he.curve
                        else {
                            return Err(KernelV2Error::UnsupportedCurvedBoolean { face: f });
                        };
                        if !face.inner_loops.is_empty() {
                            return Err(KernelV2Error::UnsupportedCurvedBoolean { face: f });
                        }
                        let anchor = map_vertex(he.origin, &mut vid_map, &mut yverts, arena)?;
                        let key = h.min(he.twin);
                        let edge_idx = *shared_edges.entry(key).or_insert_with(|| {
                            let idx = yedges.len() as u32;
                            // Cap convention: the shared rim edge carries the
                            // cap's outward normal (== this half-edge's
                            // directional normal — the validated KV5a cap rule).
                            yedges.push(yang_rs::BRepEdge {
                                start: anchor,
                                end: anchor,
                                curve: yang_rs::Curve::Circle {
                                    center,
                                    normal: Vector3::new(rim_normal.x, rim_normal.y, rim_normal.z),
                                    radius,
                                },
                            });
                            idx
                        });
                        let p0 = arena.vertex(he.origin)?.point;
                        let n = plane.normal;
                        let d = -(n.x * p0.x() + n.y * p0.y() + n.z * p0.z());
                        yfaces.push(yang_rs::BRepFace {
                            surface: yang_rs::Surface::Plane {
                                normal: Vector3::new(n.x, n.y, n.z),
                                d,
                            },
                            outer_loop: vec![edge_idx],
                            inner_loops: Vec::new(),
                            reversed: false,
                        });
                        continue;
                    }

                    // All-segment planar face: the PR-KV3 per-loop path.
                    let mut convert_loop = |lid: LoopId| -> Result<Vec<u32>, KernelV2Error> {
                        let hes = arena.loop_half_edges(lid)?;
                        for &h in &hes {
                            if !matches!(arena.half_edge(h)?.curve, Curve::LineSegment) {
                                return Err(KernelV2Error::UnsupportedCurvedBoolean { face: f });
                            }
                        }
                        if hes.is_empty() {
                            // A lone-vertex loop has no boundary to give yang-rs.
                            return Err(KernelV2Error::NonManifoldTopology(
                                "to_yang_brep: lone-vertex loop has no edge boundary",
                            ));
                        }
                        let mut vids = Vec::with_capacity(hes.len());
                        for &h in &hes {
                            let v = arena.half_edge(h)?.origin;
                            vids.push(map_vertex(v, &mut vid_map, &mut yverts, arena)?);
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

                    // First outer-loop vertex anchors d so the plane passes
                    // exactly through the loop geometry (not through the
                    // possibly-stale `plane.point` cache).
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
                Some(Surface::Cylinder {
                    axis_point,
                    axis_dir,
                    radius,
                    reversed,
                }) => {
                    // Canonical lateral only: [rim, seam, rim, seam] with two
                    // full-circle rims, no rings, outward sense. Anything else
                    // (partial patches from a previous boolean, cavity
                    // laterals) cannot re-enter yang Stage 1.
                    if reversed || !face.inner_loops.is_empty() {
                        return Err(KernelV2Error::UnsupportedCurvedBoolean { face: f });
                    }
                    let mut hes = arena.loop_half_edges(face.outer_loop)?;
                    if hes.len() != 4 {
                        return Err(KernelV2Error::UnsupportedCurvedBoolean { face: f });
                    }
                    if matches!(arena.half_edge(hes[0])?.curve, Curve::LineSegment) {
                        hes.rotate_left(1);
                    }
                    let is_circle = |h: HalfEdgeId| -> Result<bool, KernelV2Error> {
                        Ok(matches!(arena.half_edge(h)?.curve, Curve::Circle { .. }))
                    };
                    let is_seg = |h: HalfEdgeId| -> Result<bool, KernelV2Error> {
                        Ok(matches!(arena.half_edge(h)?.curve, Curve::LineSegment))
                    };
                    if !(is_circle(hes[0])?
                        && is_seg(hes[1])?
                        && is_circle(hes[2])?
                        && is_seg(hes[3])?)
                    {
                        return Err(KernelV2Error::UnsupportedCurvedBoolean { face: f });
                    }
                    // The two segments must be the seam twin pair.
                    if arena.half_edge(hes[1])?.twin != hes[3] {
                        return Err(KernelV2Error::UnsupportedCurvedBoolean { face: f });
                    }

                    let mut loop_indices = Vec::with_capacity(4);
                    for &h in &hes {
                        let he = arena.half_edge(h)?;
                        let key = h.min(he.twin);
                        let idx = match shared_edges.get(&key) {
                            Some(&idx) => idx,
                            None => {
                                let idx = yedges.len() as u32;
                                match he.curve {
                                    Curve::Circle {
                                        center,
                                        radius,
                                        normal,
                                    } => {
                                        // Created from the lateral side: the
                                        // shared edge carries the CAP-outward
                                        // normal = the negation of the
                                        // lateral half-edge's directional
                                        // normal (twins are exact negations).
                                        let nu = neg_unit(normal);
                                        let anchor = map_vertex(
                                            he.origin,
                                            &mut vid_map,
                                            &mut yverts,
                                            arena,
                                        )?;
                                        yedges.push(yang_rs::BRepEdge {
                                            start: anchor,
                                            end: anchor,
                                            curve: yang_rs::Curve::Circle {
                                                center,
                                                normal: Vector3::new(nu.x, nu.y, nu.z),
                                                radius,
                                            },
                                        });
                                    }
                                    Curve::LineSegment => {
                                        let start = map_vertex(
                                            he.origin,
                                            &mut vid_map,
                                            &mut yverts,
                                            arena,
                                        )?;
                                        let dest = arena.half_edge(he.next)?.origin;
                                        let end =
                                            map_vertex(dest, &mut vid_map, &mut yverts, arena)?;
                                        yedges.push(yang_rs::BRepEdge {
                                            start,
                                            end,
                                            curve: yang_rs::Curve::LineSegment,
                                        });
                                    }
                                    _ => {
                                        return Err(KernelV2Error::UnsupportedCurvedBoolean {
                                            face: f,
                                        })
                                    }
                                }
                                shared_edges.insert(key, idx);
                                idx
                            }
                        };
                        loop_indices.push(idx);
                    }

                    yfaces.push(yang_rs::BRepFace {
                        surface: yang_rs::Surface::Cylinder {
                            axis_point,
                            axis_dir: Vector3::new(axis_dir.x, axis_dir.y, axis_dir.z),
                            radius,
                        },
                        outer_loop: loop_indices,
                        inner_loops: Vec::new(),
                        reversed: false,
                    });
                }
                None => return Err(KernelV2Error::FaceWithoutSurface { face: f }),
            }
        }
    }

    yang_rs::BRep::new(yverts, yedges, yfaces).map_err(|e| {
        KernelV2Error::BooleanFailed(format!("yang-rs rejected the converted input B-Rep: {e}"))
    })
}

// ---------------------------------------------------------------------------
// from_yang_brep
// ---------------------------------------------------------------------------

/// The curve vocabulary of one directed yang loop edge, KV5b-classified.
#[derive(Clone, Copy, PartialEq)]
enum EdgeKind {
    Seg,
    /// Full circle (`start == end`), canonical-cylinder vocabulary.
    Full {
        center: Point3,
        normal: [f64; 3],
        radius: f64,
    },
    /// Minor arc (`start != end`); `forward_normal` is the kernel-v2
    /// directional normal for THIS directed use (sweep < π).
    Arc {
        center: Point3,
        forward_normal: [f64; 3],
        radius: f64,
    },
}

/// One validated loop of the yang output: owning yang face, kind, the
/// vertex cycle in walk order (yang vertex indices), and the per-edge
/// curve classification.
struct LoopSpec {
    face: usize,
    kind: LoopKind,
    cycle: Vec<u32>,
    edges: Vec<EdgeKind>,
}

/// Surface classification of a yang output face.
enum FaceSurf {
    Plane {
        normal: [f64; 3],
    },
    Cylinder {
        axis_point: Point3,
        axis_dir: [f64; 3],
        radius: f64,
        reversed: bool,
    },
}

/// Reassemble a yang-rs *output* `BRep` into a kernel-v2 solid.
///
/// PR-KV5b vocabulary (see module docs): planar faces with polygonal /
/// arc-bearing loops or single full-circle caps; cylinder faces — the
/// canonical full lateral or partial patches with arc/segment boundary
/// loops, including `reversed` cavity walls. The output is validated
/// structurally BEFORE the first arena mutation (loop continuity and
/// closure, twin pairing with curve agreement, orientable planar Newell
/// normals, named-curve vocabulary walls), assembled directly into the
/// arena (see module docs for why a direct assembler rather than an Euler
/// sequence), split into connected shells with per-shell genus derived
/// from the Euler–Poincaré formula, and then re-checked by the full
/// [`crate::validate::validate_solid`] — whose curved orientation analysis
/// (unrolled winding, wrap pairing) is the production gate for the curved
/// faces assembled here.
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

    // 1a. Surface vocabulary. Planar output faces never carry `reversed`
    //     (sense belongs in the plane normal); cylinder faces may.
    let mut surfs: Vec<FaceSurf> = Vec::with_capacity(yfaces.len());
    for (i, f) in yfaces.iter().enumerate() {
        match f.surface {
            yang_rs::Surface::Plane { normal, .. } => {
                if f.reversed {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "planar output face with reversed = true (sense belongs in the plane normal)",
                    ));
                }
                let n = normal.as_array();
                if (norm3(n) - 1.0).abs() > YANG_NORMAL_AGREEMENT_TOLERANCE {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output face plane normal is not unit-length",
                    ));
                }
                surfs.push(FaceSurf::Plane { normal: n });
            }
            yang_rs::Surface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                let a = axis_dir.as_array();
                if (norm3(a) - 1.0).abs() > YANG_NORMAL_AGREEMENT_TOLERANCE {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output cylinder axis_dir is not unit-length",
                    ));
                }
                if !(radius.is_finite() && radius > 0.0) {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output cylinder radius is not finite and positive",
                    ));
                }
                surfs.push(FaceSurf::Cylinder {
                    axis_point,
                    axis_dir: a,
                    radius,
                    reversed: f.reversed,
                });
            }
            _ => return Err(KernelV2Error::UnsupportedBooleanOutputSurface { face: i }),
        }
    }

    // 1b. Loops: vocabulary, continuity, closure, per-edge classification.
    let mut loops: Vec<LoopSpec> = Vec::new();
    for (fi, f) in yfaces.iter().enumerate() {
        for (li, loop_edges) in std::iter::once(&f.outer_loop)
            .chain(f.inner_loops.iter())
            .enumerate()
        {
            if loop_edges.is_empty() {
                return Err(KernelV2Error::InvalidBooleanOutput("empty output loop"));
            }
            // Walk the loop, inferring each edge's traversal direction by
            // chaining: yang OUTPUT loops are directed-continuous
            // (`e.end == next.start`), but the canonical M5 INPUT shape
            // (the round-trip) reuses one shared seam edge in BOTH
            // directions within the lateral loop, so an edge may be
            // traversed against its stored (start, end).
            let mut cycle = Vec::with_capacity(loop_edges.len());
            let mut kinds = Vec::with_capacity(loop_edges.len());
            let mut has_full = false;
            let mut cur: Option<u32> = None;
            for &ei in loop_edges.iter() {
                let Some(e) = yedges.get(ei as usize) else {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output loop references an out-of-range edge",
                    ));
                };
                if (e.start as usize) >= yverts.len() || (e.end as usize) >= yverts.len() {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output edge references an out-of-range vertex",
                    ));
                }
                // The first edge is taken as stored; later edges chain off
                // the current exit vertex.
                let (from, to) = match cur {
                    None => (e.start, e.end),
                    Some(c) if e.start == c => (e.start, e.end),
                    Some(c) if e.end == c => (e.end, e.start),
                    Some(_) => {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "output loop is not edge-continuous",
                        ));
                    }
                };
                let kind = classify_edge(e, yverts, from, to)?;
                if matches!(kind, EdgeKind::Full { .. }) {
                    has_full = true;
                }
                cycle.push(from);
                kinds.push(kind);
                cur = Some(to);
            }
            if cur != Some(cycle[0]) {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output loop does not close",
                ));
            }
            if !has_full && cycle.len() < 3 {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output loop with fewer than 3 edges and no full-circle edge",
                ));
            }
            if has_full && cycle.len() != 1 && cycle.len() != 4 {
                // Full circles occur only in the canonical vocabulary: a
                // 1-edge cap loop or the 4-edge [rim, seam, rim, seam]
                // lateral.
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "full-circle edge in a non-canonical loop",
                ));
            }
            loops.push(LoopSpec {
                face: fi,
                kind: if li == 0 {
                    LoopKind::Outer
                } else {
                    LoopKind::Inner
                },
                cycle,
                edges: kinds,
            });
        }
    }

    // 1c. Manifold edge pairing: every undirected vertex pair is used by
    //     exactly two directed loop edges with consistent curve geometry —
    //     opposite directions for ordinary edges; full-circle self-pairs
    //     get their per-use directional normal DERIVED here (see module
    //     docs, "Circle-edge sense derivation").
    struct EdgeUse {
        loop_idx: usize,
        pos: usize,
        forward: bool, // a < b for ordinary edges; unused for self-pairs
    }
    let mut pair_uses: BTreeMap<(u32, u32), Vec<EdgeUse>> = BTreeMap::new();
    for (si, spec) in loops.iter().enumerate() {
        let m = spec.cycle.len();
        for k in 0..m {
            let (a, b) = (spec.cycle[k], spec.cycle[(k + 1) % m]);
            let key = (a.min(b), a.max(b));
            pair_uses.entry(key).or_default().push(EdgeUse {
                loop_idx: si,
                pos: k,
                forward: a < b,
            });
        }
    }
    // Per (loop, pos) directional normal for full-circle uses.
    let mut full_normals: BTreeMap<(usize, usize), UnitVector3> = BTreeMap::new();
    for (&(a, b), uses) in &pair_uses {
        if uses.len() != 2 {
            return Err(KernelV2Error::InvalidBooleanOutput(
                "an undirected output edge is not used by exactly two directed edges",
            ));
        }
        let (u0, u1) = (&uses[0], &uses[1]);
        let k0 = loops[u0.loop_idx].edges[u0.pos];
        let k1 = loops[u1.loop_idx].edges[u1.pos];
        if a != b {
            if u0.forward == u1.forward {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "an undirected output edge is not used by two OPPOSITE directed edges",
                ));
            }
            // Curve agreement between the two uses.
            match (k0, k1) {
                (EdgeKind::Seg, EdgeKind::Seg) => {}
                (
                    EdgeKind::Arc {
                        center: c0,
                        radius: r0,
                        forward_normal: n0,
                    },
                    EdgeKind::Arc {
                        center: c1,
                        radius: r1,
                        forward_normal: n1,
                    },
                ) => {
                    // The per-use forward normals are derived from each use's
                    // own (start, end), so the twin pair must come out as
                    // exact negations (same stored circle, opposite walks).
                    if c0 != c1 || r0 != r1 || n0[0] != -n1[0] || n0[1] != -n1[1] || n0[2] != -n1[2]
                    {
                        return Err(KernelV2Error::InvalidBooleanOutput(
                            "twin output edges carry inconsistent arc curves",
                        ));
                    }
                }
                _ => {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "twin output edges carry inconsistent curve kinds",
                    ));
                }
            }
        } else {
            // Full-circle self-pair: derive each use's directional normal.
            let (
                EdgeKind::Full {
                    center: c0,
                    radius: r0,
                    normal: n0,
                },
                EdgeKind::Full {
                    center: c1,
                    radius: r1,
                    normal: n1,
                },
            ) = (k0, k1)
            else {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "a self-loop output edge is not a full circle",
                ));
            };
            if c0 != c1 || r0 != r1 || (n0 != n1 && n0 != [-n1[0], -n1[1], -n1[2]]) {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "the two uses of a full-circle edge carry inconsistent circles",
                ));
            }
            // Planar use(s) take ±plane normal by loop kind; a cylinder use
            // takes the negation of its partner.
            let derive_planar = |u: &EdgeUse| -> Option<UnitVector3> {
                let spec = &loops[u.loop_idx];
                let FaceSurf::Plane { normal: pn } = &surfs[spec.face] else {
                    return None;
                };
                let stored = match spec.edges[u.pos] {
                    EdgeKind::Full { normal, .. } => normal,
                    _ => unreachable!("checked full above"),
                };
                let want_sign = match spec.kind {
                    LoopKind::Outer => 1.0,
                    LoopKind::Inner => -1.0,
                };
                let d = dot3(stored, *pn);
                if d.abs() < 1.0 - YANG_NORMAL_AGREEMENT_TOLERANCE {
                    return None; // circle axis disagrees with the face plane
                }
                let s = if d * want_sign > 0.0 { 1.0 } else { -1.0 };
                Some(UnitVector3 {
                    x: s * stored[0],
                    y: s * stored[1],
                    z: s * stored[2],
                })
            };
            let n_for = |u: &EdgeUse, partner: &EdgeUse| -> Result<UnitVector3, KernelV2Error> {
                if let Some(nu) = derive_planar(u) {
                    return Ok(nu);
                }
                if matches!(surfs[loops[u.loop_idx].face], FaceSurf::Cylinder { .. }) {
                    if let Some(nu) = derive_planar(partner) {
                        return Ok(neg_unit(nu));
                    }
                }
                Err(KernelV2Error::InvalidBooleanOutput(
                    "full-circle edge sense is underivable (no planar cap use with an aligned plane)",
                ))
            };
            let nu0 = n_for(u0, u1)?;
            let nu1 = n_for(u1, u0)?;
            if nu0.x != -nu1.x || nu0.y != -nu1.y || nu0.z != -nu1.z {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "the two uses of a full-circle edge do not traverse oppositely",
                ));
            }
            full_normals.insert((u0.loop_idx, u0.pos), nu0);
            full_normals.insert((u1.loop_idx, u1.pos), nu1);
        }
    }

    // 1d. Planar face orientation: outer-loop Newell normal orientable and
    //     in agreement with yang's stated plane normal; rings wind
    //     opposite. Single full-circle loops were checked against the
    //     plane in 1c. Cylinder faces are validated by `validate_solid`'s
    //     curved orientation analysis after assembly.
    let mut face_normals: Vec<Option<UnitVector3>> = vec![None; yfaces.len()];
    for spec in &loops {
        let FaceSurf::Plane { normal } = &surfs[spec.face] else {
            continue;
        };
        if spec.cycle.len() == 1 {
            // Single full-circle loop: plane agreement established in 1c
            // (the derived normal exists only when |dot| ≈ 1).
            if spec.kind == LoopKind::Outer {
                face_normals[spec.face] = Some(UnitVector3 {
                    x: normal[0],
                    y: normal[1],
                    z: normal[2],
                });
            }
            continue;
        }
        let pts: Vec<Point3> = spec
            .cycle
            .iter()
            .map(|&v| yverts[v as usize].point)
            .collect();
        match spec.kind {
            LoopKind::Outer => {
                let Some(nu) = geom::newell_unit(&pts) else {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output face outer loop has a degenerate (zero) Newell normal",
                    ));
                };
                let dotn = nu.x * normal[0] + nu.y * normal[1] + nu.z * normal[2];
                if dotn < 1.0 - YANG_NORMAL_AGREEMENT_TOLERANCE {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output face plane normal disagrees with its outer-loop Newell normal",
                    ));
                }
                face_normals[spec.face] = Some(nu);
            }
            LoopKind::Inner => {
                let nw = geom::newell(&pts);
                if nw[0] * normal[0] + nw[1] * normal[1] + nw[2] * normal[2] >= 0.0 {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output face ring does not wind opposite to its outer loop",
                    ));
                }
            }
        }
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
    for (si, spec) in loops.iter().enumerate() {
        let fi = spec.face;
        let face_id = match face_ids[fi] {
            Some(id) => id,
            None => {
                let id = FaceId(arena.faces.len() as u32);
                let p0 = yverts[spec.cycle[0] as usize].point;
                let surface = match &surfs[fi] {
                    FaceSurf::Plane { .. } => Surface::Plane(Plane {
                        point: p0,
                        normal: face_normals[fi].expect("outer loop set the planar normal"),
                    }),
                    FaceSurf::Cylinder {
                        axis_point,
                        axis_dir,
                        radius,
                        reversed,
                    } => Surface::Cylinder {
                        axis_point: *axis_point,
                        axis_dir: UnitVector3 {
                            x: axis_dir[0],
                            y: axis_dir[1],
                            z: axis_dir[2],
                        },
                        radius: *radius,
                        reversed: *reversed,
                    },
                };
                arena.faces.push(Some(Face {
                    surface: Some(surface),
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
            // both directions (pass 1c proved exactly two consistent uses).
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
            let curve = match spec.edges[k] {
                EdgeKind::Seg => Curve::LineSegment,
                EdgeKind::Arc {
                    center,
                    forward_normal,
                    radius,
                } => Curve::Arc {
                    center,
                    normal: UnitVector3 {
                        x: forward_normal[0],
                        y: forward_normal[1],
                        z: forward_normal[2],
                    },
                    radius,
                },
                EdgeKind::Full { center, radius, .. } => {
                    let nu = full_normals[&(si, k)];
                    Curve::Circle {
                        center,
                        normal: nu,
                        radius,
                    }
                }
            };
            let origin = vert_ids[a as usize].expect("referenced vertex was created");
            arena.half_edges.push(Some(HalfEdge {
                twin,
                next: HalfEdgeId(he_base + ((k + 1) % m) as u32),
                prev: HalfEdgeId(he_base + ((k + m - 1) % m) as u32),
                origin,
                loop_id,
                curve,
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

/// Classify one yang output edge into the KV5b vocabulary, applying the
/// named-curve walls and the minor-arc sense derivation (module docs).
/// `from`/`to` are the loop's TRAVERSAL endpoints (the stored
/// `(start, end)` or its reverse — see the loop walk in
/// [`from_yang_brep`]); the derived arc direction is for that traversal.
fn classify_edge(
    e: &yang_rs::BRepEdge,
    yverts: &[yang_rs::BRepVertex],
    from: u32,
    to: u32,
) -> Result<EdgeKind, KernelV2Error> {
    match e.curve {
        yang_rs::Curve::LineSegment => {
            if e.start == e.end {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "degenerate output edge (start == end)",
                ));
            }
            Ok(EdgeKind::Seg)
        }
        yang_rs::Curve::Circle {
            center,
            normal,
            radius,
        } => {
            let n = normal.as_array();
            if !(radius.is_finite() && radius > 0.0) {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output circle edge with a non-positive radius",
                ));
            }
            if (norm3(n) - 1.0).abs() > YANG_NORMAL_AGREEMENT_TOLERANCE {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output circle edge normal is not unit-length",
                ));
            }
            if e.start == e.end {
                return Ok(EdgeKind::Full {
                    center,
                    normal: n,
                    radius,
                });
            }
            let ps = yverts[from as usize].point;
            let pe = yverts[to as usize].point;
            // Endpoints on the circle (f64-construction allowance — the
            // relocated vertices are computed in closed form).
            for p in [ps, pe] {
                let d = sub(p, center);
                let on_plane = dot3(d, n);
                let radial = (dot3(d, d) - on_plane * on_plane).max(0.0).sqrt();
                let band = 1e-9 * (1.0 + radius.max(p.x().abs().max(p.y().abs().max(p.z().abs()))));
                if (radial - radius).abs() > band || on_plane.abs() > band {
                    return Err(KernelV2Error::InvalidBooleanOutput(
                        "output arc endpoint does not lie on its circle",
                    ));
                }
            }
            let Some(sweep) = crate::geom::ccw_sweep(center, n, ps, pe) else {
                return Err(KernelV2Error::InvalidBooleanOutput(
                    "output arc endpoint has no radial direction",
                ));
            };
            let pi = std::f64::consts::PI;
            if (sweep - pi).abs() <= ARC_MINOR_AMBIGUITY_BAND {
                return Err(KernelV2Error::UnsupportedBooleanOutputCurve {
                    curve: "near-half-circle arc (minor side ambiguous)",
                });
            }
            let forward_normal = if sweep < pi { n } else { [-n[0], -n[1], -n[2]] };
            Ok(EdgeKind::Arc {
                center,
                forward_normal,
                radius,
            })
        }
        yang_rs::Curve::Ellipse { .. } => {
            Err(KernelV2Error::UnsupportedBooleanOutputCurve { curve: "Ellipse" })
        }
        yang_rs::Curve::Parabola { .. } => {
            Err(KernelV2Error::UnsupportedBooleanOutputCurve { curve: "Parabola" })
        }
        yang_rs::Curve::Hyperbola { .. } => {
            Err(KernelV2Error::UnsupportedBooleanOutputCurve { curve: "Hyperbola" })
        }
    }
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
